// ═══════════════════════════════════════════════════════════════════════
// NOYAU « JVM Rose » — bytecode `.rosebc`, chargeur vérifié, VM RIR,
// GC minimal (pool de `Gz`), sandbox (slots / budget / sous-ensemble).
// ═══════════════════════════════════════════════════════════════════════
//
// ⚠⚠⚠ CE N'EST PAS UNE JVM COMPLÈTE. C'est un **format de snapshot** du RIR
// (déjà la bytecode interne de Rose, voir `rir.rs`) avec :
//
//   * un conteneur binaire stable et versionné (magic `ROSEBC`, version u32) ;
//   * un chargeur/vérificateur qui refuse proprement magic, version, sauts,
//     slots, opcodes et opérations hors sous-ensemble ;
//   * une exécution par la VM RIR existante (`rir::executer_avec` /
//     `rir::executer_avec_ptr`), à travers un hôte minimal (`HoteBc`) ;
//   * un **pool de `Gz` avec comptage de prêt** (NOTRE « GC » — voir plus bas) ;
//   * une **sandbox** : nombre de slots borné, budget d'instructions borné,
//     aucun accès fichier/réseau/clavier possible depuis le bytecode.
//
// ⚠ DEPUIS LA PARTIE 2 (voir plus bas) : un **chargeur de classes**, un pool
// de constantes versionné, des méthodes avec pile d'opérandes, une table de
// dispatch par héritage simple et des exceptions structurées ont été ajoutés
// au format v2. Ce n'est toujours PAS une JVM Java complète (voir le doc
// `docs/RVM_JVM_COMPLETE.md`, § limites honnêtes) : ni GC traçant, ni JIT
// depuis `.rosebc`, ni threads, ni chargement dynamique de bibliothèques.
//
// ⚠ ZÉRO DÉPENDANCE AJOUTÉE : encodage écrit à la main, comme `natif_emetteur`.
// Les entiers exacts restent du **texte décimal longueur-préfixée** — jamais
// des i64 fixes, jamais GMP comme dépendance.
//
// ⚠ Le `CacheSlot(Cell)` contenu dans `Eabstrait::Slot` n'est JAMAIS sérialisé :
// il est reconstruit à vide (`CacheSlot::neuve()`) au chargement, exactement
// comme le fait `rir::abstraire`.
//
// ── Format v1 (un seul corps, little-endian) ──────────────────────────────
//
//   magic[6] = "ROSEBC" | version u32 (= 1) | n_slots u32 | n_ops u32
//   n_globaux u32
//   noms      : n_slots × (u32 len + UTF-8)
//   globaux   : n_globaux × u16
//   ops       : n_ops × (u8 opcode + opérandes)
//
// Les chaînes (noms et littéraux entiers) utilisent une longueur u32, pas u16 :
// un BigInt maison peut dépasser 65 535 chiffres, et un préfixe u16 tronquerait
// silencieusement. C'est une différence volontaire avec l'étude
// `docs/BYTECODE_ROSE.md` (§3.3), qui proposait u16 et un conteneur multi-corps.
//
// ── Format v2 (fichier de classe « JVM Rose ») ─────────────────────────────
//
//   magic[6] | version u32 (= 2) | nom_classe (u32 len + UTF-8)
//   version_classe u32 | super: u8 + (chaine si 1) | n_interfaces u32 + chaines
//   pool: version u16 | n u32 | n × (tag u8 + payload)
//   n_methodes u32 | n × methode :
//       nom (chaine) | descripteur | drapeaux u16 | n_slots u16
//       noms_slots | n_code u32 + instructions | n_exc u32 + entrées
//
// `lire_classe` accepte AUSSI le v1 (promotion en classe `<principal>`).
// Spécification complète : `docs/RVM_JVM_COMPLETE.md`.
//
// Licence MIT.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;

use crate::front::{Expr, Instr, Programme, Signature, Ty, TyKind};
use crate::i128x::Gz;
use crate::rir::{
    CacheSlot, CorpsRir, Eabstrait, FinRir, HoteRir, Op, Oper, executer_avec, poser_slot,
};
/// Ré-export public : le câblage CLI (`rose rvm`) peut construire/lire des
/// valeurs sans importer `crate::run` (voir `docs/RVM_JVM_COMPLETE.md` §5.2).
pub use crate::run::Valeur;
pub use crate::run::chaine_de as chaine_publique;

// ── Constantes du format ──────────────────────────────────────────────────

/// Magic ASCII, distinct de `ROSE_MAGIC` (queue des exécutables `rose comp`).
pub const MAGIC: &[u8; 6] = b"ROSEBC";
/// Version du format snapshot v1 (`CorpsRir`). Conservée telle quelle pour
/// rétrocompatibilité : `ecrire_bytecode` / `lire_bytecode` écrivent et lisent
/// toujours ce format.
pub const VERSION: u32 = 1;
/// Version du format « fichier de classe » v2 (JVM Rose) : pool de constantes,
/// classes, méthodes, table d'exceptions. `lire_classe` lit AUSSI le v1 en le
/// promouvant en une classe à une seule méthode.
pub const VERSION_JVM: u32 = 2;
/// Version du format « snapshot AST complet » (RVM complète). Ce format
/// sérialise le programme Rose ENTIER ( toutes les fonctions + le principal )
/// sous forme d'opcodes tagués ; c'est un snapshot du RIR/AST, pas du bytecode
/// Java, et il n'a aucune dépendance à une JVM. Exécuté par la RVM à travers
/// la mécanique Rose (slots + pile d'appels), il couvre 100 % du langage.
pub const VERSION_AST: u32 = 3;

const OP_AFFECT: u8 = 0x01;
const OP_AFFICHER_ABSTRAIT: u8 = 0x02;
const OP_LIRE_RESERVE: u8 = 0x03; // réservé (étude §3.4) — refusé par la sandbox
const OP_SAUT: u8 = 0x04;
const OP_SAUT_SI_FAUX_ABSTRAIT: u8 = 0x05;
const OP_RETOUR_ABSTRAIT: u8 = 0x06;
const OP_RETOUR_VIDE: u8 = 0x07;
/// Toutes les variantes de `Op` qui portent de l'AST sont marquées par ce code
/// à l'écriture, puis REFUSÉES à la lecture par le vérificateur.
const OP_REFUSE: u8 = 0x80;

const EX_SLOT: u8 = 0x10;
const EX_ENTIER: u8 = 0x11;
const EX_FLOTTANT: u8 = 0x12;
const EX_BINAIRE: u8 = 0x13;
const EX_NEG: u8 = 0x14;
const EX_NON: u8 = 0x15;
const EX_APPEL_RIR_REFUSE: u8 = 0x16;

const OPER_ADD: u8 = 0x01;
const OPER_SUB: u8 = 0x02;
const OPER_MUL: u8 = 0x03;
const OPER_DIV: u8 = 0x04;
const OPER_MOD: u8 = 0x05;
const OPER_PUISSANCE: u8 = 0x06;
const OPER_DIV_ENTIER: u8 = 0x07;
const OPER_LT: u8 = 0x08;
const OPER_LE: u8 = 0x09;
const OPER_GT: u8 = 0x0A;
const OPER_GE: u8 = 0x0B;
const OPER_EQ: u8 = 0x0C;
const OPER_NE: u8 = 0x0D;
const OPER_ET: u8 = 0x0E;
const OPER_OU: u8 = 0x0F;

/// Profondeur maximale d'une expression abstraite chargée : borne la
/// récursion du vérificateur face à un fichier hostile.
const PROFONDEUR_MAX: usize = 64;

// ── Erreurs ───────────────────────────────────────────────────────────────

/// Toutes les erreurs sont **propres** : le vérificateur ne panique jamais sur
/// un fichier hostile (anti-DoS), et la sandbox convertit les paniques
/// éventuelles de l'évaluateur en `Erreur::PaniqueInterne`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Erreur {
    /// Les 6 premiers octets ne sont pas `ROSEBC`.
    MagicInvalide,
    /// Version majeure du format inconnue.
    VersionInsupportee(u32),
    /// Le fichier s'arrête avant la fin d'un champ.
    Tronque { attendu: usize, present: usize },
    /// Des octets restent après la fin déclarée du corps.
    OctetsEnTrop(usize),
    /// UTF-8 invalide dans un nom ou un littéral.
    Utf8Invalide,
    /// Un index de slot (ou de global) dépasse `n_slots`.
    SlotHorsBornes { slot: u32, n_slots: u32 },
    /// Une cible de saut dépasse `n_ops`.
    SautHorsBornes { cible: u32, n_ops: u32 },
    /// Opcode inconnu.
    OpcodeInconnu(u8),
    /// Opération refusée par le sous-ensemble sandbox (AST, `AppelRir`, `Lire`).
    OperationRefusee(&'static str),
    /// Expression imbriquée trop profondément.
    ExpressionTropProfonde,
    /// `n_slots` dépasse la limite mémoire de la sandbox.
    MemoireDepassee(usize),
    /// Budget d'instructions épuisé (boucle infinie).
    BudgetDepasse(u64),
    /// Limite de `Gz` simultanés atteinte.
    GzEpuises(usize),
    /// Erreur posée par l'évaluateur (division par zéro, type invalide…).
    ExecutionEnErreur,
    /// Une panique interne a été interceptée et transformée en erreur propre.
    PaniqueInterne,

    // ── Format de classe v2 (JVM Rose) ────────────────────────────────────
    /// Index de pool de constantes hors bornes.
    IndexConstantHorsBornes { index: u16, n: usize },
    /// Entrée de pool du mauvais type (ex. `Appel` sur un entier).
    TypeConstantInattendu { index: u16, attendu: &'static str, trouve: &'static str },
    /// Pool de constantes plein (index > u16::MAX).
    PoolPlein,
    /// Tag de constante inconnu.
    TagConstantInconnu(u8),
    /// Tag d'instruction de méthode inconnu.
    OpcodeMethodeInconnu(u8),
    /// Classe introuvable dans le chargeur.
    ClasseIntrouvable(String),
    /// Méthode introuvable (après parcours de la chaîne d'héritage).
    MethodeIntrouvable { classe: String, methode: String },
    /// Arity d'appel différente de celle du descripteur.
    AriteIncoherente { methode: String, attendus: usize, fournis: usize },
    /// Profondeur d'appel maximale atteinte.
    ProfondeurDepassee(usize),
    /// Nombre maximal d'appels atteint.
    AppelsEpuises(u64),
    /// Pile d'opérandes sous-dimensionnée (vérificateur ou exécution).
    PileSousDimensionnee { methode: String, pc: usize },
    /// Profondeur de pile incohérente entre deux chemins de saut.
    PileDesequilibree { methode: String, pc: usize },
    /// Type de slot incompatible avec le descripteur / une affectation précédente.
    TypeSlotInoherent { methode: String, slot: u16, attendu: String, trouve: String },
    /// Plage d'exception invalide (debut >= fin, ou hors code).
    PlageExceptionInvalide { methode: String, debut: u32, fin: u32, cible: u32, n_ops: usize },
    /// Exception levée mais aucun gestionnaire (ni ici, ni dans un appelant).
    ExceptionNonAttrapee(String),
    /// Tag de type de descripteur inconnu.
    TypeDescripteurInconnu(u8),
    /// Incohérence de structure de méthode (slots / noms / paramètres).
    StructureMethodeInvalide(String),

    // ── Format v3 (snapshot AST complet) ───────────────────────────────────
    /// Snapshot AST mal formé (tag inconnu, booléen invalide, compteur hostile).
    SnapshotInvalide(&'static str),
    /// Erreur de runtime Rose (`erreur_runtime`) interceptée proprement.
    ExecutionRose(String),
}

impl fmt::Display for Erreur {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Erreur::MagicInvalide => write!(f, "bytecode Rose : magic `ROSEBC` absent"),
            Erreur::VersionInsupportee(v) =>
                write!(f, "bytecode Rose : version {} non supportée (attendu {})", v, VERSION),
            Erreur::Tronque { attendu, present } =>
                write!(f, "bytecode Rose : fichier tronqué ({} octets attendus, {} présents)", attendu, present),
            Erreur::OctetsEnTrop(n) =>
                write!(f, "bytecode Rose : {} octet(s) en trop après le corps", n),
            Erreur::Utf8Invalide => write!(f, "bytecode Rose : chaîne UTF-8 invalide"),
            Erreur::SlotHorsBornes { slot, n_slots } =>
                write!(f, "bytecode Rose : slot {} hors bornes (n_slots = {})", slot, n_slots),
            Erreur::SautHorsBornes { cible, n_ops } =>
                write!(f, "bytecode Rose : saut vers {} hors bornes (n_ops = {})", cible, n_ops),
            Erreur::OpcodeInconnu(o) => write!(f, "bytecode Rose : opcode inconnu 0x{:02X}", o),
            Erreur::OperationRefusee(n) =>
                write!(f, "bytecode Rose : opération `{}` refusée par la sandbox", n),
            Erreur::ExpressionTropProfonde => write!(f, "bytecode Rose : expression trop profonde"),
            Erreur::MemoireDepassee(n) =>
                write!(f, "sandbox Rose : {} slots > limite mémoire", n),
            Erreur::BudgetDepasse(n) =>
                write!(f, "sandbox Rose : budget de {} instructions épuisé", n),
            Erreur::GzEpuises(n) =>
                write!(f, "sandbox Rose : limite de {} entiers exacts simultanés atteinte", n),
            Erreur::ExecutionEnErreur => write!(f, "VM Rose : erreur d'exécution (voir l'évaluateur)"),
            Erreur::PaniqueInterne => write!(f, "VM Rose : erreur interne interceptée (sandbox)"),

            Erreur::IndexConstantHorsBornes { index, n } =>
                write!(f, "RVM v2 : index de constante {} hors bornes (pool de {})", index, n),
            Erreur::TypeConstantInattendu { index, attendu, trouve } =>
                write!(f, "RVM v2 : constante {} : {} attendu, {} trouvé", index, attendu, trouve),
            Erreur::PoolPlein => write!(f, "RVM v2 : pool de constantes plein (max 65535)"),
            Erreur::TagConstantInconnu(t) => write!(f, "RVM v2 : tag de constante inconnu 0x{:02X}", t),
            Erreur::OpcodeMethodeInconnu(o) => write!(f, "RVM v2 : opcode de méthode inconnu 0x{:02X}", o),
            Erreur::ClasseIntrouvable(c) => write!(f, "RVM v2 : classe `{}` introuvable", c),
            Erreur::MethodeIntrouvable { classe, methode } =>
                write!(f, "RVM v2 : méthode `{}.{}` introuvable", classe, methode),
            Erreur::AriteIncoherente { methode, attendus, fournis } =>
                write!(f, "RVM v2 : `{}` attend {} argument(s), {} fourni(s)", methode, attendus, fournis),
            Erreur::ProfondeurDepassee(p) => write!(f, "sandbox RVM : profondeur d'appel {} atteinte", p),
            Erreur::AppelsEpuises(n) => write!(f, "sandbox RVM : limite de {} appels atteinte", n),
            Erreur::PileSousDimensionnee { methode, pc } =>
                write!(f, "vérificateur RVM : `{}` pc {} : pile d'opérandes sous-dimensionnée", methode, pc),
            Erreur::PileDesequilibree { methode, pc } =>
                write!(f, "vérificateur RVM : `{}` pc {} : profondeur de pile incohérente", methode, pc),
            Erreur::TypeSlotInoherent { methode, slot, attendu, trouve } =>
                write!(f, "vérificateur RVM : `{}` slot {} : {} attendu, {} trouvé", methode, slot, attendu, trouve),
            Erreur::PlageExceptionInvalide { methode, debut, fin, cible, n_ops } =>
                write!(f, "vérificateur RVM : `{}` plage d'exception [{},{}) -> {} invalide (n_ops={})",
                       methode, debut, fin, cible, n_ops),
            Erreur::ExceptionNonAttrapee(t) =>
                write!(f, "RVM v2 : exception `{}` non attrapée", t),
            Erreur::TypeDescripteurInconnu(t) =>
                write!(f, "RVM v2 : tag de type de descripteur inconnu 0x{:02X}", t),
            Erreur::StructureMethodeInvalide(m) =>
                write!(f, "RVM v2 : structure de méthode invalide : {}", m),
            Erreur::SnapshotInvalide(m) =>
                write!(f, "RVM v3 : snapshot AST invalide : {}", m),
            Erreur::ExecutionRose(m) => write!(f, "{}", m),
        }
    }
}

impl std::error::Error for Erreur {}

// ── GC minimal : pool de Gz avec comptage de prêt ─────────────────────────
//
// ⚠⚠ STRATÉGIE (à ne pas sur-vendre) : **(liste libre + comptage de prêt)**,
// PAS un ramasse-miettes traçant, PAS un GC générationnel, PAS de cycles à
// collecter : un `Gz` est une poignée possédée sur un `Box<Mpz>` et ne contient
// aucune référence vers un autre `Gz` (voir `i128x.rs:457-587`).
//
//   * `prendre()` prélève une boîte libre (ou en alloue une via `Gz::neuf`) et
//     incrémente `prets`. Une limite bornée par la sandbox empêche qu'un
//     bytecode hostile épuise la mémoire en multipliant les temporaires.
//   * `rendre()` remet la boîte en réserve (cap `MAX_LIBRES`) et décrémente
//     `prets`. Le `Drop` de `Gz` libère la boîte — jamais deux fois : une boîte
//     n'appartient qu'à UN détenteur à la fois (pool OU emprunteur OU Valeur).
//   * `echapper()` comptabilise un `Gz` qui quitte le pool pour devenir une
//     `Valeur` (`Rc<Gz>`) : sa durée de vie est alors gérée par le comptage de
//     références Rust, pas par ce pool. C'est le même modèle que
//     `gz_pool.rs:23-29` de l'interpréteur.
//
// Le bytecode ne peut PAS allouer de `Gz` lui-même : le sous-ensemble binaire
// n'a aucun opcode d'allocation. Seul l'hôte (`HoteBc::entier_texte`) passe par
// ce pool pour matérialiser les littéraux hors i64. Le reste de l'arithmétique
// délègue à `interp::binaire_valeurs` (règles exactes du moteur).

/// Réserve de `Gz` réutilisables, avec comptage de prêts et limite ferme.
pub struct GcPool {
    libres: Vec<Gz>,
    /// Boîtes actuellement empruntées (temporaires en cours).
    prets: usize,
    /// Boîtes parties dans une `Valeur` (gérées par `Rc`).
    echappes: usize,
    /// Nombre total de `Gz::neuf()` effectués (diagnostic).
    total: usize,
    /// Limite de prêts simultanés (sandbox).
    limite: usize,
}

impl GcPool {
    /// Taille de réserve au-delà de laquelle un `Gz` rendu est simplement
    /// détruit (on ne garde pas une réserve sans fin).
    pub const MAX_LIBRES: usize = 64;
    /// Limite par défaut de temporaires simultanés.
    pub const LIMITE_DEFAUT: usize = 64;

    pub fn neuf(limite: usize) -> GcPool {
        GcPool { libres: Vec::new(), prets: 0, echappes: 0, total: 0, limite }
    }

    /// Prend un `Gz` : réutilise une boîte libre ou en alloue une neuve.
    /// Refuse quand la limite de prêts simultanés est atteinte.
    pub fn prendre(&mut self) -> Result<Gz, Erreur> {
        if self.prets >= self.limite {
            return Err(Erreur::GzEpuises(self.limite));
        }
        self.prets += 1;
        if let Some(g) = self.libres.pop() { return Ok(g); }
        self.total += 1;
        Ok(Gz::neuf())
    }

    /// Rend un `Gz` au pool (ou le laisse tomber si la réserve est pleine).
    pub fn rendre(&mut self, g: Gz) {
        if self.prets > 0 { self.prets -= 1; }
        if self.libres.len() < Self::MAX_LIBRES { self.libres.push(g); }
    }

    /// Un `Gz` quitte le pool : il appartient désormais à une `Valeur` (`Rc`).
    pub fn echapper(&mut self) {
        if self.prets > 0 { self.prets -= 1; }
        self.echappes += 1;
    }

    pub fn prets(&self) -> usize { self.prets }
    pub fn echappes(&self) -> usize { self.echappes }
    pub fn total(&self) -> usize { self.total }
    pub fn limite(&self) -> usize { self.limite }
    pub fn libres(&self) -> usize { self.libres.len() }
}

// ── Limites de la sandbox ─────────────────────────────────────────────────

/// Limites appliquées par `executer_sandbox`.
#[derive(Clone, Debug)]
pub struct Limites {
    /// Nombre maximum d'opérations RIR exécutées (anti-boucle-infinie).
    pub budget_instructions: u64,
    /// Nombre maximum de slots du corps (anti-DoS mémoire).
    pub max_slots: usize,
    /// Nombre maximum de `Gz` prêtés simultanément (anti-DoS entiers exacts).
    pub max_gz: usize,
}

impl Default for Limites {
    fn default() -> Limites {
        Limites {
            budget_instructions: 10_000_000,
            max_slots: u16::MAX as usize, // les slots sont encodés en u16
            max_gz: GcPool::LIMITE_DEFAUT,
        }
    }
}

// ── Sérialisation ─────────────────────────────────────────────────────────

fn pusher_u16(dst: &mut Vec<u8>, v: u16) { dst.extend_from_slice(&v.to_le_bytes()); }
fn pusher_u32(dst: &mut Vec<u8>, v: u32) { dst.extend_from_slice(&v.to_le_bytes()); }

fn pusher_chaine(dst: &mut Vec<u8>, s: &str) {
    pusher_u32(dst, s.len() as u32);
    dst.extend_from_slice(s.as_bytes());
}

fn code_oper(op: Oper) -> u8 {
    match op {
        Oper::Add => OPER_ADD, Oper::Sub => OPER_SUB, Oper::Mul => OPER_MUL,
        Oper::Div => OPER_DIV, Oper::Mod => OPER_MOD, Oper::Puissance => OPER_PUISSANCE,
        Oper::DivEntier => OPER_DIV_ENTIER,
        Oper::Lt => OPER_LT, Oper::Le => OPER_LE, Oper::Gt => OPER_GT, Oper::Ge => OPER_GE,
        Oper::Eq => OPER_EQ, Oper::Ne => OPER_NE, Oper::Et => OPER_ET, Oper::Ou => OPER_OU,
    }
}

fn ecrire_expr(dst: &mut Vec<u8>, e: &Eabstrait) {
    match e {
        Eabstrait::Slot(s, _) => { dst.push(EX_SLOT); pusher_u16(dst, *s); }
        Eabstrait::Entier(t) => { dst.push(EX_ENTIER); pusher_chaine(dst, t); }
        Eabstrait::Flottant(x) => { dst.push(EX_FLOTTANT); dst.extend_from_slice(&x.to_le_bytes()); }
        Eabstrait::Binaire { op, a, b } => {
            dst.push(EX_BINAIRE); dst.push(code_oper(*op));
            ecrire_expr(dst, a); ecrire_expr(dst, b);
        }
        Eabstrait::Neg(a) => { dst.push(EX_NEG); ecrire_expr(dst, a); }
        Eabstrait::Non(a) => { dst.push(EX_NON); ecrire_expr(dst, a); }
        // `AppelRir` porte des `Expr` AST : marqué REFUSÉ, le vérificateur
        // le rejettera à la lecture (jamais exécuté depuis `.rosebc`).
        Eabstrait::AppelRir { .. } => { dst.push(EX_APPEL_RIR_REFUSE); }
    }
}

fn code_op(op: &Op) -> u8 {
    match op {
        Op::Affect { .. } => OP_AFFECT,
        Op::AfficherAbstrait(_) => OP_AFFICHER_ABSTRAIT,
        // `Lire` = clavier : pas une fichier/réseau, mais une entrée non
        // déterministe — refusée en v1 (opcode réservé).
        Op::Lire { .. } => OP_LIRE_RESERVE,
        Op::Saut { .. } => OP_SAUT,
        Op::SautSiFauxAbstrait { .. } => OP_SAUT_SI_FAUX_ABSTRAIT,
        Op::RetourAbstrait(_) => OP_RETOUR_ABSTRAIT,
        Op::Retour(None) => OP_RETOUR_VIDE,
        // 8 variantes à AST : refusées (voir `BYTECODE_ROSE.md` §2.1).
        Op::AffectAst { .. } | Op::AffectIndex { .. } | Op::Afficher(_)
        | Op::SautSiFaux { .. } | Op::Retour(Some(_)) | Op::ExprStmt(_)
        | Op::Lever(_) | Op::Bloc(_) => OP_REFUSE,
    }
}

/// Sérialise un `CorpsRir` en `.rosebc` (little-endian, version `VERSION`).
///
/// ⚠ Cette fonction **réussit toujours** : les opérations hors sous-ensemble
/// sont écrites avec l'opcode sentinelle `0x80` et **refusées au chargement**
/// par `lire_bytecode`. C'est le vérificateur — côté chargement, donc côté
/// attaquant — qui fait autorité. Utiliser `verifier_sous_ensemble` en amont
/// pour ne jamais produire un fichier inutilisable.
pub fn ecrire_bytecode(corps: &CorpsRir) -> Vec<u8> {
    let mut out = Vec::with_capacity(64 + corps.ops.len() * 6);
    out.extend_from_slice(MAGIC);
    pusher_u32(&mut out, VERSION);
    pusher_u32(&mut out, corps.n_slots as u32);
    pusher_u32(&mut out, corps.ops.len() as u32);
    pusher_u32(&mut out, corps.globaux.len() as u32);
    for nom in &corps.noms { pusher_chaine(&mut out, nom); }
    for &g in &corps.globaux { pusher_u16(&mut out, g); }
    for op in &corps.ops {
        let code = code_op(op);
        out.push(code);
        match op {
            Op::Affect { slot, expr } => { pusher_u16(&mut out, *slot); ecrire_expr(&mut out, expr); }
            Op::AfficherAbstrait(e) | Op::RetourAbstrait(e) => ecrire_expr(&mut out, e),
            Op::Saut { cible } => pusher_u32(&mut out, *cible as u32),
            Op::SautSiFauxAbstrait { test, cible } => {
                ecrire_expr(&mut out, test); pusher_u32(&mut out, *cible as u32);
            }
            _ => {} // opcode réservé / refusé : aucune opérande
        }
    }
    out
}

/// Vérifie qu'un corps est entièrement dans le sous-ensemble binaire.
/// À appeler avant d'écrire un `.rosebc` destiné à être distribué.
pub fn verifier_sous_ensemble(corps: &CorpsRir) -> Result<(), Erreur> {
    fn expr(e: &Eabstrait, n: u32, prof: usize) -> Result<(), Erreur> {
        if prof > PROFONDEUR_MAX { return Err(Erreur::ExpressionTropProfonde); }
        match e {
            Eabstrait::Slot(s, _) => verifier_slot(*s, n),
            Eabstrait::Entier(_) | Eabstrait::Flottant(_) => Ok(()),
            Eabstrait::Binaire { a, b, .. } => { expr(a, n, prof + 1)?; expr(b, n, prof + 1) }
            Eabstrait::Neg(a) | Eabstrait::Non(a) => expr(a, n, prof + 1),
            Eabstrait::AppelRir { .. } => Err(Erreur::OperationRefusee("AppelRir")),
        }
    }
    let n = corps.n_slots as u32;
    for &g in &corps.globaux { verifier_slot(g, n)?; }
    for op in &corps.ops {
        match op {
            Op::Affect { slot, expr: e } => { verifier_slot(*slot, n)?; expr(e, n, 0)?; }
            Op::AfficherAbstrait(e) | Op::RetourAbstrait(e) => expr(e, n, 0)?,
            Op::Saut { cible } => verifier_saut(*cible, corps.ops.len())?,
            Op::SautSiFauxAbstrait { test, cible } => {
                expr(test, n, 0)?; verifier_saut(*cible, corps.ops.len())?;
            }
            Op::Retour(None) => {}
            Op::Lire { .. } => return Err(Erreur::OperationRefusee("Lire")),
            Op::AffectAst { .. } | Op::AffectIndex { .. } | Op::Afficher(_)
            | Op::SautSiFaux { .. } | Op::Retour(Some(_)) | Op::ExprStmt(_)
            | Op::Lever(_) | Op::Bloc(_) => return Err(Erreur::OperationRefusee("AST")),
        }
    }
    Ok(())
}

fn verifier_slot(slot: u16, n_slots: u32) -> Result<(), Erreur> {
    if (slot as u32) < n_slots { Ok(()) }
    else { Err(Erreur::SlotHorsBornes { slot: slot as u32, n_slots }) }
}

fn verifier_saut(cible: usize, n_ops: usize) -> Result<(), Erreur> {
    if cible < n_ops { Ok(()) }
    else { Err(Erreur::SautHorsBornes { cible: cible as u32, n_ops: n_ops as u32 }) }
}

// ── Chargement + vérification ─────────────────────────────────────────────

struct Lecteur<'a> {
    octets: &'a [u8],
    pos: usize,
}

impl<'a> Lecteur<'a> {
    /// Nombre d'octets non encore lus (garde-fou avant allocation).
    fn reste(&self) -> usize { self.octets.len() - self.pos }

    fn prendre(&mut self, n: usize) -> Result<&'a [u8], Erreur> {
        let fin = self.pos.checked_add(n)
            .ok_or(Erreur::Tronque { attendu: n, present: self.octets.len() - self.pos })?;
        if fin > self.octets.len() {
            return Err(Erreur::Tronque { attendu: fin - self.pos, present: self.octets.len() - self.pos });
        }
        let r = &self.octets[self.pos..fin];
        self.pos = fin;
        Ok(r)
    }
    fn u8(&mut self) -> Result<u8, Erreur> { Ok(self.prendre(1)?[0]) }
    fn u16(&mut self) -> Result<u16, Erreur> {
        let b = self.prendre(2)?; Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> Result<u32, Erreur> {
        let b = self.prendre(4)?; Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn f64(&mut self) -> Result<f64, Erreur> {
        let b = self.prendre(8)?;
        Ok(f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
    }
    fn chaine(&mut self) -> Result<String, Erreur> {
        let n = self.u32()? as usize;
        let b = self.prendre(n)?;
        std::str::from_utf8(b).map(|s| s.to_string()).map_err(|_| Erreur::Utf8Invalide)
    }
}

fn lire_oper(o: u8) -> Result<Oper, Erreur> {
    match o {
        OPER_ADD => Ok(Oper::Add), OPER_SUB => Ok(Oper::Sub), OPER_MUL => Ok(Oper::Mul),
        OPER_DIV => Ok(Oper::Div), OPER_MOD => Ok(Oper::Mod), OPER_PUISSANCE => Ok(Oper::Puissance),
        OPER_DIV_ENTIER => Ok(Oper::DivEntier),
        OPER_LT => Ok(Oper::Lt), OPER_LE => Ok(Oper::Le), OPER_GT => Ok(Oper::Gt),
        OPER_GE => Ok(Oper::Ge), OPER_EQ => Ok(Oper::Eq), OPER_NE => Ok(Oper::Ne),
        OPER_ET => Ok(Oper::Et), OPER_OU => Ok(Oper::Ou),
        _ => Err(Erreur::OpcodeInconnu(o)),
    }
}

fn lire_expr(l: &mut Lecteur<'_>, n_slots: u32, prof: usize) -> Result<Eabstrait, Erreur> {
    if prof > PROFONDEUR_MAX { return Err(Erreur::ExpressionTropProfonde); }
    let code = l.u8()?;
    match code {
        EX_SLOT => {
            let s = l.u16()?; verifier_slot(s, n_slots)?;
            // ⚠ LE CACHE N'EST JAMAIS SÉRIALISÉ : reconstruit à vide.
            Ok(Eabstrait::Slot(s, CacheSlot::neuve()))
        }
        EX_ENTIER => Ok(Eabstrait::Entier(l.chaine()?)),
        EX_FLOTTANT => Ok(Eabstrait::Flottant(l.f64()?)),
        EX_BINAIRE => {
            let op = lire_oper(l.u8()?)?;
            let a = Box::new(lire_expr(l, n_slots, prof + 1)?);
            let b = Box::new(lire_expr(l, n_slots, prof + 1)?);
            Ok(Eabstrait::Binaire { op, a, b })
        }
        EX_NEG => Ok(Eabstrait::Neg(Box::new(lire_expr(l, n_slots, prof + 1)?))),
        EX_NON => Ok(Eabstrait::Non(Box::new(lire_expr(l, n_slots, prof + 1)?))),
        EX_APPEL_RIR_REFUSE => Err(Erreur::OperationRefusee("AppelRir")),
        _ => Err(Erreur::OpcodeInconnu(code)),
    }
}

/// Charge et **vérifie** un `.rosebc`.
///
/// Contrôles effectués (tous propres, zéro panic) :
/// magic, version, troncature, octets en trop, UTF-8, nombre de slots cohérent
/// avec la table de noms, index de slots, cibles de sauts, opcodes, profondeur
/// d'expression, et refus des opérations à AST / `AppelRir` / `Lire`.
pub fn lire_bytecode(octets: &[u8]) -> Result<CorpsRir, Erreur> {
    let mut l = Lecteur { octets, pos: 0 };
    if l.prendre(6)? != MAGIC { return Err(Erreur::MagicInvalide); }
    let version = l.u32()?;
    if version != VERSION { return Err(Erreur::VersionInsupportee(version)); }
    let n_slots_u32 = l.u32()?;
    let n_ops_u32 = l.u32()?;
    let n_globaux_u32 = l.u32()?;
    let n_slots = n_slots_u32 as usize;
    let n_ops = n_ops_u32 as usize;
    let n_globaux = n_globaux_u32 as usize;

    // Cohérence des compteurs avant toute allocation : un `n_slots` énorme
    // ne doit pas déclencher un `Vec::with_capacity` géant.
    let reste = octets.len() - l.pos;
    let min_par_nom = 4usize; // préfixe u32 de longueur, au minimum
    if reste < n_slots.saturating_mul(min_par_nom)
        || reste < n_globaux.saturating_mul(2)
        || reste < n_ops
    {
        return Err(Erreur::Tronque { attendu: n_slots + n_globaux + n_ops, present: reste });
    }

    let mut noms = Vec::with_capacity(n_slots.min(octets.len()));
    for _ in 0..n_slots { noms.push(l.chaine()?); }

    let mut globaux = Vec::with_capacity(n_globaux.min(octets.len()));
    for _ in 0..n_globaux {
        let g = l.u16()?; verifier_slot(g, n_slots_u32)?; globaux.push(g);
    }

    let mut ops = Vec::with_capacity(n_ops.min(octets.len()));
    for _ in 0..n_ops {
        let code = l.u8()?;
        let op = match code {
            OP_AFFECT => {
                let slot = l.u16()?; verifier_slot(slot, n_slots_u32)?;
                Op::Affect { slot, expr: lire_expr(&mut l, n_slots_u32, 0)? }
            }
            OP_AFFICHER_ABSTRAIT => Op::AfficherAbstrait(lire_expr(&mut l, n_slots_u32, 0)?),
            OP_SAUT => {
                let cible = l.u32()?; verifier_saut(cible as usize, n_ops)?;
                Op::Saut { cible: cible as usize }
            }
            OP_SAUT_SI_FAUX_ABSTRAIT => {
                let test = lire_expr(&mut l, n_slots_u32, 0)?;
                let cible = l.u32()?; verifier_saut(cible as usize, n_ops)?;
                Op::SautSiFauxAbstrait { test, cible: cible as usize }
            }
            OP_RETOUR_ABSTRAIT => Op::RetourAbstrait(lire_expr(&mut l, n_slots_u32, 0)?),
            OP_RETOUR_VIDE => Op::Retour(None),
            OP_LIRE_RESERVE => return Err(Erreur::OperationRefusee("Lire")),
            OP_REFUSE => return Err(Erreur::OperationRefusee("AST")),
            _ => return Err(Erreur::OpcodeInconnu(code)),
        };
        ops.push(op);
    }

    if l.pos != octets.len() {
        return Err(Erreur::OctetsEnTrop(octets.len() - l.pos));
    }

    let corps = CorpsRir { ops, noms, globaux, n_slots };
    // Dernière passe : invariants sémantiques (redondants avec les contrôles
    // ci-dessus, mais ils garantissent que le chargement ne peut JAMAIS
    // produire un corps que `executer_avec_ptr` lirait hors bornes).
    verifier_sous_ensemble(&corps)?;
    Ok(corps)
}

// ── Hôte minimal de la VM ─────────────────────────────────────────────────

/// Corps vide servant uniquement aux méthodes du trait `HoteRir` qui ne
/// reçoivent pas de `&CorpsRir` (`eval_abstrait`, `afficher_abstrait`).
/// L'évaluation abstraite de `HoteBc` n'utilise jamais ce paramètre (aucun
/// `AppelRir` dans le sous-ensemble), donc ce fantôme est sans danger.
fn corps_fantome() -> &'static CorpsRir {
    static C: std::sync::OnceLock<CorpsRir> = std::sync::OnceLock::new();
    C.get_or_init(|| CorpsRir { ops: Vec::new(), noms: Vec::new(), globaux: Vec::new(), n_slots: 0 })
}

/// Hôte minimal pour exécuter un `CorpsRir` chargé depuis `.rosebc`.
///
/// ⚠ Il n'a aucune E/S : la sortie est capturée dans `sortie` (pas de stdout,
/// pas de fichier, pas de réseau, pas de clavier). C'est la sandbox.
pub struct HoteBc {
    /// Sortie capturée du programme (équivalent du `AfficherAbstrait` RIR).
    pub sortie: String,
    /// GC minimal : pool de `Gz` avec comptage de prêt.
    pub gc: GcPool,
    /// Erreur posée par l'évaluateur rapide (0 = aucune), comme dans `interp`.
    erreur: u8,
    /// Slots du cadre courant (utilisés par le chemin sandboxed).
    slots: Vec<Valeur>,
}

impl HoteBc {
    pub fn neuf(limite_gz: usize) -> HoteBc {
        HoteBc {
            sortie: String::new(),
            gc: GcPool::neuf(limite_gz),
            erreur: 0,
            slots: Vec::new(),
        }
    }

    fn signale_erreur<T>(&mut self) -> Result<T, Option<i32>> {
        self.erreur = 1;
        Err(Some(1))
    }

    /// Matérialise un littéral entier exact. Les entiers hors i64 passent par
    /// le GC minimal : le `Gz` est prêté, rempli depuis le texte décimal, puis
    /// « échappe » dans une `Valeur` (`Rc<Gz>`) — jamais un i64 tronqué.
    fn entier_texte(&mut self, t: &str) -> Valeur {
        let s = t.trim();
        if let Ok(v) = s.parse::<i64>() { return Valeur::z(v); }
        match self.gc.prendre() {
            Ok(g) => {
                if !g.depuis_chaine(s) {
                    self.gc.rendre(g);
                    self.erreur = 1;
                    return Valeur::Vide;
                }
                self.gc.echapper();
                Valeur::zg(g)
            }
            Err(_) => { self.erreur = 1; Valeur::Vide }
        }
    }
}

impl HoteRir for HoteBc {
    fn eval_slots(&mut self, _e: &Expr, _slots: &[Valeur], _noms: &[String])
        -> Result<Valeur, Option<i32>>
    { self.signale_erreur() }

    fn eval_abstrait(&mut self, e: &Eabstrait, slots: &[Valeur])
        -> Result<Valeur, Option<i32>>
    {
        if slots.is_empty() { return self.signale_erreur(); }
        let base = slots.as_ptr() as *mut Valeur;
        let v = self.eval_abstrait_rapide(e, base, corps_fantome());
        if self.erreur != 0 { Err(Some(1)) } else { Ok(v) }
    }

    fn eval_abstrait_rapide(&mut self, e: &Eabstrait, base: *mut Valeur, _corps: &CorpsRir)
        -> Valeur
    {
        match e {
            Eabstrait::Slot(i, cache) => {
                let idx = *i as usize;
                let (ptr, ty) = cache.lire();
                let (ptr, ty) = if ptr == base as usize && ty != 0 {
                    (ptr, ty)
                } else {
                    let p = unsafe { base.add(idx) } as usize;
                    let v = unsafe { &*(p as *const Valeur) };
                    let t = match v {
                        Valeur::Entier(_, None) => 1,
                        Valeur::Flottant(_) => 2,
                        Valeur::Booleen(_) => 3,
                        Valeur::Vide => 4,
                        _ => 0,
                    };
                    cache.ecrire((p, t));
                    (p, t)
                };
                let v: &Valeur = unsafe { &*(ptr as *const Valeur) };
                match ty {
                    1 => match v { Valeur::Entier(x, None) => Valeur::Entier(*x, None), _ => v.clone() },
                    2 => match v { Valeur::Flottant(x) => Valeur::Flottant(*x), _ => v.clone() },
                    3 => match v { Valeur::Booleen(b) => Valeur::Booleen(*b), _ => v.clone() },
                    4 => Valeur::Vide,
                    _ => v.clone(),
                }
            }
            Eabstrait::Entier(t) => self.entier_texte(t),
            Eabstrait::Flottant(x) => Valeur::f(*x),
            Eabstrait::Neg(a) => {
                let v = self.eval_abstrait_rapide(a, base, _corps);
                if self.erreur != 0 { return Valeur::Vide; }
                match crate::interp::unaire_valeur("-", &v) {
                    Ok(r) => r,
                    Err(_) => { self.erreur = 1; Valeur::Vide }
                }
            }
            Eabstrait::Non(a) => {
                let v = self.eval_abstrait_rapide(a, base, _corps);
                if self.erreur != 0 { return Valeur::Vide; }
                match crate::interp::unaire_valeur("non", &v) {
                    Ok(r) => r,
                    Err(_) => { self.erreur = 1; Valeur::Vide }
                }
            }
            Eabstrait::Binaire { op, a, b } => {
                // Voie rapide i64 (slot ⊕ slot) — mêmes règles que le RIR.
                if let (Eabstrait::Slot(i, _), Eabstrait::Slot(j, _)) = (&**a, &**b) {
                    let x = unsafe { &*base.add(*i as usize) };
                    let y = unsafe { &*base.add(*j as usize) };
                    if let (Valeur::Entier(p, None), Valeur::Entier(q, None)) = (x, y) {
                        let (p, q) = (*p, *q);
                        match op {
                            Oper::Add => if let Some(r) = p.checked_add(q) { return Valeur::z(r); },
                            Oper::Sub => if let Some(r) = p.checked_sub(q) { return Valeur::z(r); },
                            Oper::Mul => if let Some(r) = p.checked_mul(q) { return Valeur::z(r); },
                            Oper::Lt | Oper::Le | Oper::Gt | Oper::Ge | Oper::Eq | Oper::Ne => {
                                let c = p.cmp(&q);
                                return Valeur::b(match op {
                                    Oper::Lt => c.is_lt(), Oper::Le => c.is_le(),
                                    Oper::Gt => c.is_gt(), Oper::Ge => c.is_ge(),
                                    Oper::Eq => c.is_eq(), _ => c.is_ne(),
                                });
                            }
                            _ => {}
                        }
                    }
                }
                // Court-circuit de `et` / `ou`.
                if *op == Oper::Et || *op == Oper::Ou {
                    let g = self.eval_abstrait_rapide(a, base, _corps);
                    if self.erreur != 0 { return Valeur::Vide; }
                    let vrai_g = matches!(g, Valeur::Booleen(true));
                    if *op == Oper::Et && !vrai_g { return Valeur::b(false); }
                    if *op == Oper::Ou && vrai_g { return Valeur::b(true); }
                    let d = self.eval_abstrait_rapide(b, base, _corps);
                    if self.erreur != 0 { return Valeur::Vide; }
                    return Valeur::b(matches!(d, Valeur::Booleen(true)));
                }
                let va = self.eval_abstrait_rapide(a, base, _corps);
                if self.erreur != 0 { return Valeur::Vide; }
                let vb = self.eval_abstrait_rapide(b, base, _corps);
                if self.erreur != 0 { return Valeur::Vide; }
                let txt = match op {
                    Oper::Add => "+", Oper::Sub => "-", Oper::Mul => "*", Oper::Div => "/",
                    Oper::Mod => "%", Oper::Puissance => "^", Oper::DivEntier => "div",
                    Oper::Lt => "<", Oper::Le => "<=", Oper::Gt => ">", Oper::Ge => ">=",
                    Oper::Eq => "==", Oper::Ne => "!=", Oper::Et => "et", Oper::Ou => "ou",
                };
                match crate::interp::binaire_valeurs(txt, &va, &vb) {
                    Ok(r) => r,
                    Err(_) => { self.erreur = 1; Valeur::Vide }
                }
            }
            // Le vérificateur refuse ce cas avant toute exécution.
            Eabstrait::AppelRir { .. } => { self.erreur = 1; Valeur::Vide }
        }
    }

    fn appel_rir_direct(&mut self, _nom: &str, _args: &[Expr], _base: *mut Valeur,
                        _corps: Option<&CorpsRir>) -> Option<Result<Valeur, Option<i32>>>
    { None } // aucun appel de fonction depuis `.rosebc` (sous-ensemble v1)

    fn eval_appel_ast(&mut self, _nom: &str, _args: &[Expr], _base: *mut Valeur)
        -> Result<Valeur, Option<i32>>
    { self.signale_erreur() }

    fn eval_ast(&mut self, _e: &Expr, _base: *mut Valeur, _corps: &CorpsRir)
        -> Result<Valeur, Option<i32>>
    { self.signale_erreur() }

    fn affecter_index_ast(&mut self, _nom: &str, _index: &Expr, _valeur: &Expr,
                          _base: *mut Valeur, _corps: &CorpsRir) -> Result<(), Option<i32>>
    { self.signale_erreur() }

    fn afficher_expression(&mut self, _e: &Expr, _base: *mut Valeur, _corps: &CorpsRir)
        -> Result<(), Option<i32>>
    { self.signale_erreur() }

    fn lire_slot(&mut self, _ty: crate::front::TyKind, _base: *mut Valeur,
                 _corps: &CorpsRir, _slot: usize) -> Valeur
    { self.erreur = 1; Valeur::Vide }

    fn exec_bloc_slots(&mut self, _b: &[Instr], _base: *mut Valeur, _corps: &CorpsRir)
        -> Result<(), Option<i32>>
    { self.signale_erreur() }

    fn synchroniser_slots(&mut self, _base: *mut Valeur, _corps: &CorpsRir) {
        // Sandbox isolée : aucune globale de l'interpréteur n'est visible.
    }

    fn tete_pile(&self) -> usize { 0 }
    fn depiler_n(&mut self, _n: usize) {}

    fn erreur_en_attente(&self) -> u8 { self.erreur }
    fn effacer_erreur(&mut self) { self.erreur = 0; }

    fn empiler_slots(&mut self, n: usize) -> *mut Valeur {
        if self.slots.len() < n { self.slots.resize(n, Valeur::Vide); }
        for i in 0..n.min(self.slots.len()) { self.slots[i] = Valeur::Vide; }
        self.slots.as_mut_ptr()
    }
    fn slots_courants(&mut self, n: usize) -> *mut Valeur {
        if self.slots.len() < n { self.slots.resize(n, Valeur::Vide); }
        self.slots.as_mut_ptr()
    }
    fn depiler_slots(&mut self) {}

    fn exec_bloc_imbrique(&mut self, _b: &[Instr]) -> FinRir { FinRir::Quitter(1) }

    fn afficher_valeur(&mut self, v: &Valeur) {
        // ⚠ Aucune E/S réelle : tout est capturé en mémoire (sandbox).
        self.sortie.push_str(&crate::run::chaine_de(v));
    }

    fn afficher_abstrait(&mut self, e: &Eabstrait, slots: &[Valeur])
        -> Result<(), Option<i32>>
    {
        if slots.is_empty() { return self.signale_erreur(); }
        let base = slots.as_ptr() as *mut Valeur;
        let v = self.eval_abstrait_rapide(e, base, corps_fantome());
        if self.erreur != 0 { return Err(Some(1)); }
        self.afficher_valeur(&v);
        Ok(())
    }

    fn affecter_index(&mut self, _nom: &str, _index: &Expr, _valeur: &Expr)
        -> Result<(), Option<i32>>
    { self.signale_erreur() }
}

// ── Exécution ─────────────────────────────────────────────────────────────

/// Exécute un corps chargé **via la VM RIR existante** (`rir::executer_avec`),
/// avec les arguments posés dans les premiers slots.
///
/// ⚠ Chemin « nu » : aucune limite de budget (utiliser `executer_sandbox`
/// pour exécuter un bytecode non fiable). La sortie est capturée dans `hote`.
pub fn executer_avec_vm(corps: &CorpsRir, arguments: &[Valeur], hote: &mut HoteBc) -> FinRir {
    let mut slots = vec![Valeur::Vide; corps.n_slots];
    let base = slots.as_mut_ptr();
    for (i, v) in arguments.iter().take(corps.n_slots).enumerate() {
        poser_slot(base, i, v.clone());
    }
    executer_avec(corps, &mut slots, hote)
}

fn executer_sandbox_interne(corps: &CorpsRir, arguments: &[Valeur], hote: &mut HoteBc,
                            limites: &Limites) -> Result<Valeur, Erreur>
{
    verifier_sous_ensemble(corps)?;
    if corps.n_slots > limites.max_slots {
        return Err(Erreur::MemoireDepassee(corps.n_slots));
    }
    if hote.slots.len() < corps.n_slots { hote.slots.resize(corps.n_slots, Valeur::Vide); }
    let base: *mut Valeur = hote.slots.as_mut_ptr();
    for i in 0..corps.n_slots { unsafe { *base.add(i) = Valeur::Vide; } }
    for (i, v) in arguments.iter().take(corps.n_slots).enumerate() {
        unsafe { *base.add(i) = v.clone(); }
    }
    hote.effacer_erreur();

    let mut pc: usize = 0;
    let mut pas: u64 = 0;
    while pc < corps.ops.len() {
        pas += 1;
        if pas > limites.budget_instructions {
            return Err(Erreur::BudgetDepasse(limites.budget_instructions));
        }
        let op = &corps.ops[pc];
        pc += 1;
        match op {
            Op::Affect { slot, expr } => {
                let v = hote.eval_abstrait_rapide(expr, base, corps);
                if hote.erreur_en_attente() != 0 { return Err(Erreur::ExecutionEnErreur); }
                unsafe { *base.add(*slot as usize) = v; }
            }
            Op::AfficherAbstrait(expr) => {
                let v = hote.eval_abstrait_rapide(expr, base, corps);
                if hote.erreur_en_attente() != 0 { return Err(Erreur::ExecutionEnErreur); }
                hote.afficher_valeur(&v);
            }
            Op::Saut { cible } => { pc = *cible; }
            Op::SautSiFauxAbstrait { test, cible } => {
                let v = hote.eval_abstrait_rapide(test, base, corps);
                if hote.erreur_en_attente() != 0 { return Err(Erreur::ExecutionEnErreur); }
                if !matches!(v, Valeur::Booleen(true)) { pc = *cible; }
            }
            Op::RetourAbstrait(expr) => {
                let v = hote.eval_abstrait_rapide(expr, base, corps);
                if hote.erreur_en_attente() != 0 { return Err(Erreur::ExecutionEnErreur); }
                return Ok(v);
            }
            Op::Retour(None) => return Ok(Valeur::Vide),
            _ => return Err(Erreur::OperationRefusee("opcode hors sous-ensemble")),
        }
    }
    Ok(Valeur::Vide)
}

/// Exécute un `.rosebc` chargé dans la **sandbox** : budget d'instructions,
/// limite de slots, limite de `Gz`, aucune E/S, aucune fonction, aucune AST.
/// `arguments` sont posés dans les premiers slots (comme pour une fonction).
///
/// ⚠ Les paniques éventuelles de l'évaluateur (ex. `erreur_runtime`) sont
/// converties en `Erreur::PaniqueInterne` : un bytecode hostile ne fait pas
/// tomber le processus hôte.
pub fn executer_sandbox(corps: &CorpsRir, arguments: &[Valeur], hote: &mut HoteBc,
                        limites: &Limites) -> Result<Valeur, Erreur>
{
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        executer_sandbox_interne(corps, arguments, hote, limites)
    }));
    match result {
        Ok(r) => r,
        Err(_) => Err(Erreur::PaniqueInterne),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PARTIE 2 — « JVM Rose » : fichier de classe v2, pool de constantes,
// classes / méthodes, invocation, exceptions structurées, vérification.
// ═══════════════════════════════════════════════════════════════════════════
//
// ⚠ CE QU'EST (ET CE QUE N'EST PAS) CETTE COUCHE — honnêteté absolue :
//
// C'est une **machine virtuelle à cadre de méthode et pile d'opérandes**,
// inspirée de la JVM (pool de constantes indexé, descripteurs, table de
// méthodes, table d'exceptions par plage, déroulement de pile). Elle tourne
// sur les mêmes `Valeur` que l'interpréteur Rose (arithmétique exacte incluse).
//
// Ce n'est **PAS** une JVM Java : pas de bytecode Java, pas de vérificateur de
// types JVM au sens JVMS §4.10, pas de ramasse-miettes traçant, pas de threads,
// pas de monitores, pas de charges dynamiques, pas de JIT depuis ce format.
// Le « GC » reste le `GcPool` à comptage de prêt de la partie 1.
//
// Le format v2 est **auto-contained** : aucune dépendance, aucune modification
// de `rir.rs` / `interp.rs` / `run.rs`. Le module n'est pas encore câblé dans
// `lib.rs` / `main.rs` (voir `docs/RVM_JVM_COMPLETE.md`, § patch CLI proposé).

// ── Pool de constantes ─────────────────────────────────────────────────────

pub const CP_ENTIER: u8 = 1;
pub const CP_BIGINT: u8 = 2;
pub const CP_FLOTTANT: u8 = 3;
pub const CP_CHAINE: u8 = 4;
pub const CP_SYMBOLE: u8 = 5;
pub const CP_METHODE: u8 = 6;

/// Une entrée du pool de constantes. Les bigints sont du **texte décimal
/// longueur-préfixé** (jamais de GMP, jamais de i64 tronqué).
#[derive(Clone, Debug, PartialEq)]
pub enum Const {
    Entier(i64),
    BigInt(String),
    Flottant(f64),
    Chaine(String),
    /// Nom de classe, de méthode ou de type d'exception.
    Symbole(String),
    /// Référence de méthode : trois index dans le pool (classe, nom, descripteur).
    Methode { classe: u16, nom: u16, descr: u16 },
}

impl Const {
    /// Fabrique une constante entière à partir d'un texte décimal exact.
    pub fn depuis_entier_texte(t: &str) -> Const {
        match t.trim().parse::<i64>() {
            Ok(v) => Const::Entier(v),
            Err(_) => Const::BigInt(t.trim().to_string()),
        }
    }

    /// Tag binaire stable (voir `docs/RVM_JVM_COMPLETE.md`).
    pub fn tag(&self) -> u8 {
        match self {
            Const::Entier(_) => CP_ENTIER,
            Const::BigInt(_) => CP_BIGINT,
            Const::Flottant(_) => CP_FLOTTANT,
            Const::Chaine(_) => CP_CHAINE,
            Const::Symbole(_) => CP_SYMBOLE,
            Const::Methode { .. } => CP_METHODE,
        }
    }

    /// Nom lisible du type (pour les messages d'erreur).
    pub fn nom_type(&self) -> &'static str {
        match self {
            Const::Entier(_) => "entier",
            Const::BigInt(_) => "bigint",
            Const::Flottant(_) => "flottant",
            Const::Chaine(_) => "chaîne",
            Const::Symbole(_) => "symbole",
            Const::Methode { .. } => "méthode",
        }
    }

    /// Type abstrait utilisé par le vérificateur.
    pub fn ty_abstrait(&self) -> TyDesc {
        match self {
            Const::Entier(_) | Const::BigInt(_) => TyDesc::Entier,
            Const::Flottant(_) => TyDesc::Flottant,
            Const::Chaine(_) | Const::Symbole(_) => TyDesc::Chaine,
            Const::Methode { .. } => TyDesc::Inconnu,
        }
    }
}

/// Pool de constantes **versionné** (le champ `version` est sérialisé).
#[derive(Clone, Debug, Default)]
pub struct ConstantPool {
    pub version: u16,
    entrees: Vec<Const>,
}

impl ConstantPool {
    /// Version du pool. Incrémenter quand le jeu de tags change.
    pub const VERSION_CP: u16 = 1;

    pub fn neuf() -> ConstantPool {
        ConstantPool { version: Self::VERSION_CP, entrees: Vec::new() }
    }

    pub fn len(&self) -> usize { self.entrees.len() }
    pub fn est_vide(&self) -> bool { self.entrees.is_empty() }
    pub fn entrees(&self) -> &[Const] { &self.entrees }

    /// Ajoute une entrée et rend son index stable (u16, comme la JVM).
    pub fn ajouter(&mut self, c: Const) -> Result<u16, Erreur> {
        if self.entrees.len() >= u16::MAX as usize { return Err(Erreur::PoolPlein); }
        let i = self.entrees.len() as u16;
        self.entrees.push(c);
        Ok(i)
    }

    /// Ajoute une entrée, ou rend l'index d'une entrée déjà présente et égale
    /// (déduplication — utile quand on compile plusieurs fois le même littéral).
    pub fn ajouter_unique(&mut self, c: Const) -> Result<u16, Erreur> {
        if let Some(i) = self.entrees.iter().position(|e| e == &c) { return Ok(i as u16); }
        self.ajouter(c)
    }

    pub fn get(&self, index: u16) -> Result<&Const, Erreur> {
        self.entrees.get(index as usize)
            .ok_or(Erreur::IndexConstantHorsBornes { index, n: self.entrees.len() })
    }

    /// Récupère une constante en vérifiant son type.
    pub fn get_comme(&self, index: u16, attendu: &'static str,
                     est: fn(&Const) -> bool) -> Result<&Const, Erreur> {
        let c = self.get(index)?;
        if est(c) { Ok(c) }
        else { Err(Erreur::TypeConstantInattendu {
            index, attendu, trouve: c.nom_type(),
        }) }
    }

    pub fn symbole(&self, index: u16) -> Result<&str, Erreur> {
        match self.get_comme(index, "symbole", |c| matches!(c, Const::Symbole(_)))? {
            Const::Symbole(s) => Ok(s),
            _ => unreachable!(),
        }
    }

    pub fn chaine(&self, index: u16) -> Result<&str, Erreur> {
        match self.get_comme(index, "chaîne", |c| matches!(c, Const::Chaine(_)))? {
            Const::Chaine(s) => Ok(s),
            _ => unreachable!(),
        }
    }

    pub fn methode(&self, index: u16)
        -> Result<(u16, u16, u16), Erreur> {
        match self.get_comme(index, "méthode", |c| matches!(c, Const::Methode { .. }))? {
            Const::Methode { classe, nom, descr } => Ok((*classe, *nom, *descr)),
            _ => unreachable!(),
        }
    }
}

fn ecrire_cp(dst: &mut Vec<u8>, cp: &ConstantPool) {
    pusher_u16(dst, cp.version);
    pusher_u32(dst, cp.entrees.len() as u32);
    for e in &cp.entrees {
        dst.push(e.tag());
        match e {
            Const::Entier(v) => dst.extend_from_slice(&v.to_le_bytes()),
            Const::BigInt(t) => pusher_chaine(dst, t),
            Const::Flottant(x) => dst.extend_from_slice(&x.to_le_bytes()),
            Const::Chaine(s) | Const::Symbole(s) => pusher_chaine(dst, s),
            Const::Methode { classe, nom, descr } => {
                pusher_u16(dst, *classe); pusher_u16(dst, *nom); pusher_u16(dst, *descr);
            }
        }
    }
}

fn lire_cp(l: &mut Lecteur<'_>) -> Result<ConstantPool, Erreur> {
    let version = l.u16()?;
    let n = l.u32()? as usize;
    // Garde-fou avant allocation (un compteur hostile ne doit pas réserver 4 Go).
    if n > l.reste() { return Err(Erreur::Tronque { attendu: n, present: l.reste() }); }
    let mut entrees = Vec::with_capacity(n);
    for _ in 0..n {
        let tag = l.u8()?;
        let e = match tag {
            CP_ENTIER => {
                let b = l.prendre(8)?;
                Const::Entier(i64::from_le_bytes([b[0],b[1],b[2],b[3],b[4],b[5],b[6],b[7]]))
            }
            CP_BIGINT => Const::BigInt(l.chaine()?),
            CP_FLOTTANT => Const::Flottant(l.f64()?),
            CP_CHAINE => Const::Chaine(l.chaine()?),
            CP_SYMBOLE => Const::Symbole(l.chaine()?),
            CP_METHODE => Const::Methode {
                classe: l.u16()?, nom: l.u16()?, descr: l.u16()?,
            },
            _ => return Err(Erreur::TagConstantInconnu(tag)),
        };
        entrees.push(e);
    }
    Ok(ConstantPool { version, entrees })
}

// ── Jeu d'instructions des méthodes (pile d'opérandes) ────────────────────

pub const BC_POUSSER_CONST: u8 = 0x01;
pub const BC_CHARGER_SLOT: u8 = 0x02;
pub const BC_STOCKER_SLOT: u8 = 0x03;
pub const BC_DUPLIQUER: u8 = 0x04;
pub const BC_BINAIRE: u8 = 0x05;
pub const BC_NEG: u8 = 0x06;
pub const BC_NON: u8 = 0x07;
pub const BC_SAUT: u8 = 0x08;
pub const BC_SAUT_SI_FAUX: u8 = 0x09;
pub const BC_APPEL: u8 = 0x0A;
pub const BC_RETOUR_VIDE: u8 = 0x0B;
pub const BC_RETOUR_VALEUR: u8 = 0x0C;
pub const BC_LEVER: u8 = 0x0D;
pub const BC_AFFICHER: u8 = 0x0E;

/// Une instruction de méthode. Machine à pile : les opérandes sont dans la
/// pile du cadre, les paramètres dans les premiers slots (comme la JVM).
#[derive(Clone, Debug, PartialEq)]
pub enum InstrBc {
    /// Empile une constante du pool (entier, bigint, flottant, chaîne).
    PousserConst { index: u16 },
    /// Empile la valeur d'un slot local.
    ChargerSlot { slot: u16 },
    /// Dépile dans un slot local.
    StockerSlot { slot: u16 },
    /// Duplique le sommet de pile.
    Dupliquer,
    /// Dépile b puis a, empile `a op b` (sémantique exacte de Rose).
    Binaire { op: Oper },
    Neg,
    Non,
    /// Saut inconditionnel (index d'instruction).
    Saut { cible: u32 },
    /// Dépile une condition, saut si fausse.
    SautSiFaux { cible: u32 },
    /// Appel de méthode : dépile `arite` arguments, empile le résultat
    /// (`Valeur::Vide` pour un descripteur `Vide`). L'index pointe une
    /// constante `CP_METHODE`.
    Appel { methode: u16 },
    /// Retour sans valeur (empile `Vide` chez l'appelant).
    RetourVide,
    /// Dépile et retourne une valeur.
    RetourValeur,
    /// Lève une exception typée : `type_index` est un `Symbole`,
    /// `msg_index` une constante de valeur (souvent une `Chaine`).
    Lever { type_index: u16, msg_index: u16 },
    /// Dépile et affiche dans `HoteBc::sortie` (aucune E/S réelle).
    Afficher,
}

fn ecrire_instr(dst: &mut Vec<u8>, i: &InstrBc) {
    match i {
        InstrBc::PousserConst { index } => { dst.push(BC_POUSSER_CONST); pusher_u16(dst, *index); }
        InstrBc::ChargerSlot { slot } => { dst.push(BC_CHARGER_SLOT); pusher_u16(dst, *slot); }
        InstrBc::StockerSlot { slot } => { dst.push(BC_STOCKER_SLOT); pusher_u16(dst, *slot); }
        InstrBc::Dupliquer => dst.push(BC_DUPLIQUER),
        InstrBc::Binaire { op } => { dst.push(BC_BINAIRE); dst.push(code_oper(*op)); }
        InstrBc::Neg => dst.push(BC_NEG),
        InstrBc::Non => dst.push(BC_NON),
        InstrBc::Saut { cible } => { dst.push(BC_SAUT); pusher_u32(dst, *cible); }
        InstrBc::SautSiFaux { cible } => { dst.push(BC_SAUT_SI_FAUX); pusher_u32(dst, *cible); }
        InstrBc::Appel { methode } => { dst.push(BC_APPEL); pusher_u16(dst, *methode); }
        InstrBc::RetourVide => dst.push(BC_RETOUR_VIDE),
        InstrBc::RetourValeur => dst.push(BC_RETOUR_VALEUR),
        InstrBc::Lever { type_index, msg_index } => {
            dst.push(BC_LEVER); pusher_u16(dst, *type_index); pusher_u16(dst, *msg_index);
        }
        InstrBc::Afficher => dst.push(BC_AFFICHER),
    }
}

fn lire_instr(l: &mut Lecteur<'_>) -> Result<InstrBc, Erreur> {
    let tag = l.u8()?;
    Ok(match tag {
        BC_POUSSER_CONST => InstrBc::PousserConst { index: l.u16()? },
        BC_CHARGER_SLOT => InstrBc::ChargerSlot { slot: l.u16()? },
        BC_STOCKER_SLOT => InstrBc::StockerSlot { slot: l.u16()? },
        BC_DUPLIQUER => InstrBc::Dupliquer,
        BC_BINAIRE => InstrBc::Binaire { op: lire_oper(l.u8()?)? },
        BC_NEG => InstrBc::Neg,
        BC_NON => InstrBc::Non,
        BC_SAUT => InstrBc::Saut { cible: l.u32()? },
        BC_SAUT_SI_FAUX => InstrBc::SautSiFaux { cible: l.u32()? },
        BC_APPEL => InstrBc::Appel { methode: l.u16()? },
        BC_RETOUR_VIDE => InstrBc::RetourVide,
        BC_RETOUR_VALEUR => InstrBc::RetourValeur,
        BC_LEVER => InstrBc::Lever { type_index: l.u16()?, msg_index: l.u16()? },
        BC_AFFICHER => InstrBc::Afficher,
        _ => return Err(Erreur::OpcodeMethodeInconnu(tag)),
    })
}

// ── Promotion v1 → classe v2 (compilateur CorpsRir → méthode) ─────────────

fn compiler_expr(e: &Eabstrait, cp: &mut ConstantPool, out: &mut Vec<InstrBc>)
    -> Result<(), Erreur>
{
    match e {
        Eabstrait::Slot(s, _) => out.push(InstrBc::ChargerSlot { slot: *s }),
        Eabstrait::Entier(t) => {
            let index = cp.ajouter_unique(Const::depuis_entier_texte(t))?;
            out.push(InstrBc::PousserConst { index });
        }
        Eabstrait::Flottant(x) => {
            let index = cp.ajouter_unique(Const::Flottant(*x))?;
            out.push(InstrBc::PousserConst { index });
        }
        Eabstrait::Binaire { op, a, b } => {
            compiler_expr(a, cp, out)?;
            compiler_expr(b, cp, out)?;
            out.push(InstrBc::Binaire { op: *op });
        }
        Eabstrait::Neg(a) => { compiler_expr(a, cp, out)?; out.push(InstrBc::Neg); }
        Eabstrait::Non(a) => { compiler_expr(a, cp, out)?; out.push(InstrBc::Non); }
        Eabstrait::AppelRir { .. } => return Err(Erreur::OperationRefusee("AppelRir")),
    }
    Ok(())
}

fn compiler_op(op: &Op, cp: &mut ConstantPool, out: &mut Vec<InstrBc>) -> Result<(), Erreur> {
    match op {
        Op::Affect { slot, expr } => {
            compiler_expr(expr, cp, out)?;
            out.push(InstrBc::StockerSlot { slot: *slot });
        }
        Op::AfficherAbstrait(e) => { compiler_expr(e, cp, out)?; out.push(InstrBc::Afficher); }
        Op::Saut { cible } => out.push(InstrBc::Saut { cible: *cible as u32 }),
        Op::SautSiFauxAbstrait { test, cible } => {
            compiler_expr(test, cp, out)?;
            out.push(InstrBc::SautSiFaux { cible: *cible as u32 });
        }
        Op::RetourAbstrait(e) => { compiler_expr(e, cp, out)?; out.push(InstrBc::RetourValeur); }
        Op::Retour(None) => out.push(InstrBc::RetourVide),
        Op::Lire { .. } => return Err(Erreur::OperationRefusee("Lire")),
        _ => return Err(Erreur::OperationRefusee("AST")),
    }
    Ok(())
}

// ── Descripteurs, classes, méthodes ───────────────────────────────────────

pub const TD_ENTIER: u8 = 1;
pub const TD_FLOTTANT: u8 = 2;
pub const TD_BOOLEEN: u8 = 3;
pub const TD_CHAINE: u8 = 4;
pub const TD_VIDE: u8 = 5;
pub const TD_OBJET: u8 = 6;
pub const TD_INCONNU: u8 = 7; // usage interne du vérificateur uniquement

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TyDesc {
    Entier,
    Flottant,
    Booleen,
    Chaine,
    Vide,
    Objet(String),
    /// « type non prouvé » : compatible avec tout, utilisé par le vérificateur.
    Inconnu,
}

impl TyDesc {
    fn tag(&self) -> u8 {
        match self {
            TyDesc::Entier => TD_ENTIER, TyDesc::Flottant => TD_FLOTTANT,
            TyDesc::Booleen => TD_BOOLEEN, TyDesc::Chaine => TD_CHAINE,
            TyDesc::Vide => TD_VIDE, TyDesc::Objet(_) => TD_OBJET,
            TyDesc::Inconnu => TD_INCONNU,
        }
    }
    fn ecrire(&self, dst: &mut Vec<u8>) {
        dst.push(self.tag());
        if let TyDesc::Objet(n) = self { pusher_chaine(dst, n); }
    }
    fn lire(l: &mut Lecteur<'_>) -> Result<TyDesc, Erreur> {
        Ok(match l.u8()? {
            TD_ENTIER => TyDesc::Entier,
            TD_FLOTTANT => TyDesc::Flottant,
            TD_BOOLEEN => TyDesc::Booleen,
            TD_CHAINE => TyDesc::Chaine,
            TD_VIDE => TyDesc::Vide,
            TD_OBJET => TyDesc::Objet(l.chaine()?),
            TD_INCONNU => TyDesc::Inconnu,
            t => return Err(Erreur::TypeDescripteurInconnu(t)),
        })
    }
    pub fn nom(&self) -> String {
        match self {
            TyDesc::Entier => "entier".to_string(),
            TyDesc::Flottant => "flottant".to_string(),
            TyDesc::Booleen => "booléen".to_string(),
            TyDesc::Chaine => "chaîne".to_string(),
            TyDesc::Vide => "vide".to_string(),
            TyDesc::Objet(n) => format!("objet {}", n),
            TyDesc::Inconnu => "inconnu".to_string(),
        }
    }
    /// Un type inconnu est compatible avec tout ; sinon égalité stricte.
    fn compatible(&self, autre: &TyDesc) -> bool {
        matches!(self, TyDesc::Inconnu) || matches!(autre, TyDesc::Inconnu) || self == autre
    }
}

/// Descripteur d'entrée/sortie d'une méthode (analogue d'un descripteur JVM).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Descripteur {
    pub parametres: Vec<TyDesc>,
    pub retour: TyDesc,
}

impl Descripteur {
    pub fn neuf(parametres: Vec<TyDesc>, retour: TyDesc) -> Descripteur {
        Descripteur { parametres, retour }
    }
    fn ecrire(&self, dst: &mut Vec<u8>) {
        pusher_u32(dst, self.parametres.len() as u32);
        for p in &self.parametres { p.ecrire(dst); }
        self.retour.ecrire(dst);
    }
    fn lire(l: &mut Lecteur<'_>) -> Result<Descripteur, Erreur> {
        let n = l.u32()? as usize;
        if n > l.reste() { return Err(Erreur::Tronque { attendu: n, present: l.reste() }); }
        let mut parametres = Vec::with_capacity(n);
        for _ in 0..n { parametres.push(TyDesc::lire(l)?); }
        Ok(Descripteur { parametres, retour: TyDesc::lire(l)? })
    }
}

/// Une entrée de la table d'exceptions : si `pc ∈ [debut, fin)` et que le type
/// levé correspond à `type_index` (0 = attrape tout), sauter à `cible`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntreeException {
    pub debut: u32,
    pub fin: u32,
    pub cible: u32,
    /// Index dans le pool : `Symbole` du type, ou 0 pour « attrape tout ».
    pub type_index: u16,
}

#[derive(Clone, Debug)]
pub struct MethodeRose {
    pub nom: String,
    pub descr: Descripteur,
    /// Drapeaux (0 = méthode d'instance ; bit 0 = publique, bit 1 = statique).
    pub drapeaux: u16,
    pub n_slots: u16,
    pub noms_slots: Vec<String>,
    pub code: Vec<InstrBc>,
    pub exceptions: Vec<EntreeException>,
}

impl MethodeRose {
    pub fn neuf(nom: &str, descr: Descripteur, noms_slots: &[&str],
                code: Vec<InstrBc>) -> Result<MethodeRose, Erreur> {
        let noms = noms_slots.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let n_slots = u16::try_from(noms.len())
            .map_err(|_| Erreur::StructureMethodeInvalide("trop de slots".to_string()))?;
        Ok(MethodeRose {
            nom: nom.to_string(), descr, drapeaux: 0, n_slots,
            noms_slots: noms, code, exceptions: Vec::new(),
        })
    }
}

/// Une classe Rose chargée (pool + méthodes + table de dispatch).
#[derive(Clone, Debug)]
pub struct ClasseRose {
    pub nom: String,
    /// Version du fichier de classe (1 pour cette révision).
    pub version: u32,
    pub cp: ConstantPool,
    pub super_classe: Option<String>,
    pub interfaces: Vec<String>,
    pub methodes: Vec<MethodeRose>,
    /// Table de dispatch : nom de méthode -> index dans `methodes`.
    /// Reconstruite par le chargeur (héritage simple, écrasement par la classe).
    pub table_dispatch: HashMap<String, usize>,
}

impl ClasseRose {
    /// Version courante d'une classe (distincte de la version du conteneur).
    pub const VERSION_CLASSE: u32 = 1;

    pub fn neuf(nom: &str) -> ClasseRose {
        ClasseRose {
            nom: nom.to_string(), version: Self::VERSION_CLASSE,
            cp: ConstantPool::neuf(), super_classe: None, interfaces: Vec::new(),
            methodes: Vec::new(), table_dispatch: HashMap::new(),
        }
    }

    pub fn ajouter_methode(&mut self, m: MethodeRose) -> Result<usize, Erreur> {
        if m.noms_slots.len() != m.n_slots as usize {
            return Err(Erreur::StructureMethodeInvalide(format!(
                "`{}` : noms_slots={} mais n_slots={}", m.nom, m.noms_slots.len(), m.n_slots)));
        }
        if m.descr.parametres.len() > m.n_slots as usize {
            return Err(Erreur::StructureMethodeInvalide(format!(
                "`{}` : plus de paramètres que de slots", m.nom)));
        }
        let i = self.methodes.len();
        self.table_dispatch.insert(m.nom.clone(), i);
        self.methodes.push(m);
        Ok(i)
    }

    /// ⚠ Cherche PARMI LES MÉTHODES DÉFINIES localement. La table
    /// `table_dispatch` peut contenir des indices hérités qui pointent dans le
    /// tableau de la classe mère : ils ne sont valides que pour la classe
    /// définissante, retrouvée par `ChargeurClasses::resoudre`.
    pub fn index_methode(&self, nom: &str) -> Option<usize> {
        self.methodes.iter().position(|m| m.nom == nom)
    }
    pub fn methodes(&self) -> &[MethodeRose] { &self.methodes }
    pub fn methode(&self, nom: &str) -> Result<&MethodeRose, Erreur> {
        self.index_methode(nom)
            .map(|i| &self.methodes[i])
            .ok_or_else(|| Erreur::MethodeIntrouvable {
                classe: self.nom.clone(), methode: nom.to_string(),
            })
    }
}

/// Résout une référence `CP_METHODE` vers (classe, nom, texte du descripteur).
fn ref_method_depuis_cp(cp: &ConstantPool, index: u16)
    -> Result<(String, String, String), Erreur>
{
    let (ci, ni, di) = cp.methode(index)?;
    Ok((cp.symbole(ci)?.to_string(), cp.symbole(ni)?.to_string(),
        cp.chaine(di)?.to_string()))
}

/// Chargeur de classes : détient les classes et résout les appels en parcourant
/// la chaîne d'héritage simple (une classe fille hérite de la table du parent).
#[derive(Clone, Debug, Default)]
pub struct ChargeurClasses {
    pub classes: HashMap<String, ClasseRose>,
}

impl ChargeurClasses {
    pub fn neuf() -> ChargeurClasses {
        ChargeurClasses { classes: HashMap::new() }
    }

    fn ajouter_brut(&mut self, c: ClasseRose) { self.classes.insert(c.nom.clone(), c); }

    pub fn ajouter(&mut self, c: ClasseRose) -> Result<(), Erreur> {
        verifier_classe(&c)?;
        self.ajouter_brut(c);
        self.recalculer_dispatches();
        self.verifier_lien()
    }

    /// Reconstruit toutes les tables de dispatch (héritage simple).
    pub fn recalculer_dispatches(&mut self) {
        let noms: Vec<String> = self.classes.keys().cloned().collect();
        for nom in noms {
            let mut table = HashMap::new();
            if let Some(c) = self.classes.get(&nom) {
                if let Some(sp) = &c.super_classe {
                    if let Some(parent) = self.classes.get(sp) {
                        table = parent.table_dispatch.clone();
                    }
                }
                for (i, m) in c.methodes.iter().enumerate() {
                    table.insert(m.nom.clone(), i);
                }
            }
            if let Some(c) = self.classes.get_mut(&nom) { c.table_dispatch = table; }
        }
    }

    /// Résout `methode` depuis `classe` en remontant la chaîne `super_classe`.
    /// Rend le nom de la classe DÉFINISSANTE et l'index LOCAL de la méthode
    /// (les indices hérités de `table_dispatch` ne sont pas utilisables ici).
    pub fn resoudre(&self, classe: &str, methode: &str)
        -> Result<(String, usize), Erreur>
    {
        let mut courant = Some(classe);
        let mut sauts = 0usize;
        while let Some(cn) = courant {
            sauts += 1;
            if sauts > 256 { break; } // cycle hostile : borne ferme
            let c = self.classes.get(cn)
                .ok_or_else(|| Erreur::ClasseIntrouvable(cn.to_string()))?;
            if let Some(i) = c.index_methode(methode) {
                return Ok((cn.to_string(), i));
            }
            courant = c.super_classe.as_deref();
        }
        Err(Erreur::MethodeIntrouvable {
            classe: classe.to_string(), methode: methode.to_string(),
        })
    }

    pub fn arite(&self, classe: &str, methode: &str) -> Result<usize, Erreur> {
        let (def, i) = self.resoudre(classe, methode)?;
        Ok(self.classes[&def].methodes[i].descr.parametres.len())
    }

    /// Vérifie toutes les classes (structure + flot + liens de méthodes/arité).
    pub fn verifier_lien(&self) -> Result<(), Erreur> {
        for c in self.classes.values() {
            verifier_structure(c)?;
            verifier_flot(c, self)?;
        }
        Ok(())
    }
}

// ── Vérificateur strict ────────────────────────────────────────────────────

/// Vérification structurelle d'une classe (indépendante du chargeur).
pub fn verifier_classe(c: &ClasseRose) -> Result<(), Erreur> { verifier_structure(c) }

fn verifier_structure(c: &ClasseRose) -> Result<(), Erreur> {
    if c.nom.is_empty() {
        return Err(Erreur::StructureMethodeInvalide("nom de classe vide".to_string()));
    }
    if c.version > ClasseRose::VERSION_CLASSE {
        return Err(Erreur::VersionInsupportee(c.version));
    }
    let cp = &c.cp;
    // Pool : toute référence de méthode doit pointer vers des symboles/chaînes.
    for e in cp.entrees().iter() {
        if let Const::Methode { classe, nom, descr } = e {
            cp.get_comme(*classe, "symbole", |x| matches!(x, Const::Symbole(_)))?;
            cp.get_comme(*nom, "symbole", |x| matches!(x, Const::Symbole(_)))?;
            cp.get_comme(*descr, "chaîne", |x| matches!(x, Const::Chaine(_)))?;
        }
    }
    for m in &c.methodes {
        if m.noms_slots.len() != m.n_slots as usize {
            return Err(Erreur::StructureMethodeInvalide(format!(
                "`{}` : noms_slots={} != n_slots={}", m.nom, m.noms_slots.len(), m.n_slots)));
        }
        if m.descr.parametres.len() > m.n_slots as usize {
            return Err(Erreur::StructureMethodeInvalide(format!(
                "`{}` : arité > slots", m.nom)));
        }
        let n_ops = m.code.len();
        for (pc, ins) in m.code.iter().enumerate() {
            match ins {
                InstrBc::ChargerSlot { slot } | InstrBc::StockerSlot { slot } => {
                    if (*slot as usize) >= m.n_slots as usize {
                        return Err(Erreur::SlotHorsBornes {
                            slot: *slot as u32, n_slots: m.n_slots as u32 });
                    }
                }
                InstrBc::PousserConst { index } => {
                    match cp.get(*index)? {
                        Const::Entier(_) | Const::BigInt(_) | Const::Flottant(_)
                        | Const::Chaine(_) | Const::Symbole(_) => {}
                        other => return Err(Erreur::TypeConstantInattendu {
                            index: *index, attendu: "valeur", trouve: other.nom_type(),
                        }),
                    }
                }
                InstrBc::Appel { methode } => { cp.methode(*methode)?; }
                InstrBc::Lever { type_index, msg_index } => {
                    cp.get_comme(*type_index, "symbole",
                                 |x| matches!(x, Const::Symbole(_)))?;
                    cp.get(*msg_index)?;
                }
                InstrBc::Saut { cible } | InstrBc::SautSiFaux { cible } => {
                    if (*cible as usize) >= n_ops {
                        return Err(Erreur::SautHorsBornes {
                            cible: *cible, n_ops: n_ops as u32 });
                    }
                }
                _ => {}
            }
            let _ = pc;
        }
        for e in &m.exceptions {
            if e.debut >= e.fin || (e.fin as usize) > n_ops || (e.cible as usize) >= n_ops {
                return Err(Erreur::PlageExceptionInvalide {
                    methode: m.nom.clone(), debut: e.debut, fin: e.fin,
                    cible: e.cible, n_ops,
                });
            }
            if e.type_index != 0 {
                cp.get_comme(e.type_index, "symbole",
                             |x| matches!(x, Const::Symbole(_)))?;
            }
        }
    }
    Ok(())
}

#[derive(Clone, PartialEq)]
struct EtatBc {
    pile: Vec<TyDesc>,
    slots: Vec<TyDesc>,
}

fn fusionner_etats(a: &EtatBc, b: &EtatBc, methode: &str, pc: usize)
    -> Result<EtatBc, Erreur>
{
    if a.pile.len() != b.pile.len() {
        return Err(Erreur::PileDesequilibree { methode: methode.to_string(), pc });
    }
    let pile = a.pile.iter().zip(&b.pile)
        .map(|(x, y)| if x == y { x.clone() } else { TyDesc::Inconnu }).collect();
    let slots = a.slots.iter().zip(&b.slots)
        .map(|(x, y)| if x == y { x.clone() } else { TyDesc::Inconnu }).collect();
    Ok(EtatBc { pile, slots })
}

fn pop_type(pile: &mut Vec<TyDesc>, methode: &str, pc: usize) -> Result<TyDesc, Erreur> {
    pile.pop().ok_or_else(||
        Erreur::PileSousDimensionnee { methode: methode.to_string(), pc })
}

fn est_comparaison(op: Oper) -> bool {
    matches!(op, Oper::Lt | Oper::Le | Oper::Gt | Oper::Ge | Oper::Eq | Oper::Ne
                | Oper::Et | Oper::Ou)
}

/// Vérification de flot (types de slots, profondeur de pile, arité des appels).
/// Nécessite le chargeur pour résoudre les références de méthode.
fn verifier_flot(c: &ClasseRose, chargeur: &ChargeurClasses) -> Result<(), Erreur> {
    for m in &c.methodes {
        let n = m.code.len();
        if n == 0 { continue; }
        let slots0: Vec<TyDesc> = (0..m.n_slots as usize).map(|i| {
            m.descr.parametres.get(i).cloned().unwrap_or(TyDesc::Inconnu)
        }).collect();
        let mut etats: Vec<Option<EtatBc>> = vec![None; n];
        etats[0] = Some(EtatBc { pile: Vec::new(), slots: slots0 });
        let mut file = VecDeque::new();
        file.push_back(0usize);
        let max_pas = n.saturating_mul(n + 1).saturating_mul(8).max(64);
        let mut pas = 0usize;
        while let Some(pc) = file.pop_front() {
            pas += 1;
            if pas > max_pas {
                return Err(Erreur::StructureMethodeInvalide(format!(
                    "`{}` : vérificateur non convergent", m.nom)));
            }
            let mut e = match &etats[pc] { Some(s) => s.clone(), None => continue };
            let mut succ: Vec<(usize, EtatBc)> = Vec::new();
            match &m.code[pc] {
                InstrBc::PousserConst { index } => {
                    e.pile.push(c.cp.get(*index)?.ty_abstrait());
                    succ.push((pc + 1, e.clone()));
                }
                InstrBc::ChargerSlot { slot } => {
                    e.pile.push(e.slots[*slot as usize].clone());
                    succ.push((pc + 1, e.clone()));
                }
                InstrBc::StockerSlot { slot } => {
                    let t = pop_type(&mut e.pile, &m.nom, pc)?;
                    let s = &mut e.slots[*slot as usize];
                    if !t.compatible(s) {
                        return Err(Erreur::TypeSlotInoherent {
                            methode: m.nom.clone(), slot: *slot,
                            attendu: s.nom(), trouve: t.nom(),
                        });
                    }
                    if matches!(s, TyDesc::Inconnu) { *s = t; }
                    succ.push((pc + 1, e.clone()));
                }
                InstrBc::Dupliquer => {
                    let t = pop_type(&mut e.pile, &m.nom, pc)?;
                    e.pile.push(t.clone()); e.pile.push(t);
                    succ.push((pc + 1, e.clone()));
                }
                InstrBc::Binaire { op } => {
                    let b = pop_type(&mut e.pile, &m.nom, pc)?;
                    let a = pop_type(&mut e.pile, &m.nom, pc)?;
                    let r = if est_comparaison(*op) { TyDesc::Booleen }
                        else if a == TyDesc::Flottant || b == TyDesc::Flottant { TyDesc::Flottant }
                        else if a == TyDesc::Entier && b == TyDesc::Entier { TyDesc::Entier }
                        else { TyDesc::Inconnu };
                    e.pile.push(r);
                    succ.push((pc + 1, e.clone()));
                }
                InstrBc::Neg => {
                    let a = pop_type(&mut e.pile, &m.nom, pc)?;
                    e.pile.push(a); succ.push((pc + 1, e.clone()));
                }
                InstrBc::Non => {
                    pop_type(&mut e.pile, &m.nom, pc)?;
                    e.pile.push(TyDesc::Booleen); succ.push((pc + 1, e.clone()));
                }
                InstrBc::Saut { cible } => succ.push((*cible as usize, e.clone())),
                InstrBc::SautSiFaux { cible } => {
                    let t = pop_type(&mut e.pile, &m.nom, pc)?;
                    if !t.compatible(&TyDesc::Booleen) {
                        return Err(Erreur::TypeSlotInoherent {
                            methode: m.nom.clone(), slot: 0,
                            attendu: TyDesc::Booleen.nom(), trouve: t.nom(),
                        });
                    }
                    succ.push((*cible as usize, e.clone()));
                    succ.push((pc + 1, e.clone()));
                }
                InstrBc::Appel { methode } => {
                    let (dc, dn, _) = ref_method_depuis_cp(&c.cp, *methode)?;
                    let (def, _) = chargeur.resoudre(&dc, &dn)?;
                    let cm = &chargeur.classes[&def];
                    let mi = cm.index_methode(&dn).unwrap();
                    let meth = &cm.methodes[mi];
                    let arite = meth.descr.parametres.len();
                    if e.pile.len() < arite {
                        return Err(Erreur::PileSousDimensionnee {
                            methode: m.nom.clone(), pc });
                    }
                    for k in 0..arite {
                        let t = pop_type(&mut e.pile, &m.nom, pc)?;
                        let attendu = &meth.descr.parametres[arite - 1 - k];
                        if !t.compatible(attendu) {
                            return Err(Erreur::AriteIncoherente {
                                methode: dn.clone(), attendus: arite, fournis: arite,
                            });
                        }
                    }
                    e.pile.push(meth.descr.retour.clone());
                    succ.push((pc + 1, e.clone()));
                }
                InstrBc::RetourVide => {}
                InstrBc::RetourValeur => { pop_type(&mut e.pile, &m.nom, pc)?; }
                InstrBc::Lever { .. } => {
                    for ex in &m.exceptions {
                        if pc >= ex.debut as usize && pc < ex.fin as usize {
                            // Le gestionnaire reçoit la pile REMPLACÉE par la
                            // valeur de l'exception, mais les slots du cadre.
                            succ.push((ex.cible as usize, EtatBc {
                                pile: vec![TyDesc::Chaine],
                                slots: e.slots.clone(),
                            }));
                        }
                    }
                }
                InstrBc::Afficher => {
                    pop_type(&mut e.pile, &m.nom, pc)?;
                    succ.push((pc + 1, e.clone()));
                }
            }
            for (dst, ne) in succ {
                if dst >= n { continue; } // retombée implicite hors code = retour vide
                match &etats[dst] {
                    None => { etats[dst] = Some(ne); file.push_back(dst); }
                    Some(old) => {
                        let f = fusionner_etats(old, &ne, &m.nom, dst)?;
                        if &f != old { etats[dst] = Some(f); file.push_back(dst); }
                    }
                }
            }
        }
    }
    Ok(())
}

// ── Machine JVM : cadres, invocation, exceptions ──────────────────────────

/// Limites de la sandbox JVM (toutes bornées, aucune E/S).
#[derive(Clone, Debug)]
pub struct LimitesJvm {
    pub budget_instructions: u64,
    pub profondeur_max: usize,
    pub max_appels: u64,
    pub max_slots_par_cadre: usize,
    pub max_constantes: usize,
}

impl Default for LimitesJvm {
    fn default() -> LimitesJvm {
        LimitesJvm {
            budget_instructions: 10_000_000,
            profondeur_max: 64,
            max_appels: 1_000_000,
            max_slots_par_cadre: u16::MAX as usize,
            max_constantes: u16::MAX as usize,
        }
    }
}

/// Un cadre de méthode : slots locaux + pile d'opérandes + pointeur de code.
/// ⚠ Pas de `Debug` : `Valeur` ne l'implémente pas (contrainte hors de ce module).
#[derive(Clone)]
pub struct CadreJvm {
    pub classe: String,
    pub methode: String,
    pub slots: Vec<Valeur>,
    pub pile: Vec<Valeur>,
    pub pc: usize,
}

/// La machine. Elle EMPRUNTE le chargeur (immuable) et l'hôte (sortie capturée,
/// GC). Aucun état global, aucun thread, aucun fichier.
pub struct MachineJvm<'a> {
    pub chargeur: &'a ChargeurClasses,
    pub hote: &'a mut HoteBc,
    pub limites: LimitesJvm,
    pub instructions: u64,
    pub appels: u64,
}

fn pop_pile(pile: &mut Vec<Valeur>, methode: &str, pc: usize) -> Result<Valeur, Erreur> {
    pile.pop().ok_or_else(||
        Erreur::PileSousDimensionnee { methode: methode.to_string(), pc })
}

impl<'a> MachineJvm<'a> {
    pub fn neuf(chargeur: &'a ChargeurClasses, hote: &'a mut HoteBc,
                limites: LimitesJvm) -> MachineJvm<'a> {
        MachineJvm { chargeur, hote, limites, instructions: 0, appels: 0 }
    }

    fn cp_de(&self, classe: &str) -> Result<&ConstantPool, Erreur> {
        self.chargeur.classes.get(classe)
            .map(|c| &c.cp)
            .ok_or_else(|| Erreur::ClasseIntrouvable(classe.to_string()))
    }

    fn symbole_de(&self, classe: &str, index: u16) -> Result<String, Erreur> {
        Ok(self.cp_de(classe)?.symbole(index)?.to_string())
    }

    fn valeur_const(&mut self, classe: &str, index: u16) -> Result<Valeur, Erreur> {
        let c = self.cp_de(classe)?.get(index)?.clone();
        Ok(match c {
            Const::Entier(v) => Valeur::z(v),
            Const::BigInt(t) => self.hote.entier_texte(&t),
            Const::Flottant(x) => Valeur::f(x),
            Const::Chaine(s) | Const::Symbole(s) => Valeur::s(s),
            Const::Methode { .. } => Valeur::Vide,
        })
    }

    fn nouveau_cadre(&self, classe: &str, nom_meth: &str, m: &MethodeRose,
                     args: &[Valeur]) -> Result<CadreJvm, Erreur> {
        let attendus = m.descr.parametres.len();
        if args.len() != attendus {
            return Err(Erreur::AriteIncoherente {
                methode: nom_meth.to_string(), attendus, fournis: args.len(),
            });
        }
        if m.n_slots as usize > self.limites.max_slots_par_cadre {
            return Err(Erreur::MemoireDepassee(m.n_slots as usize));
        }
        let mut slots = vec![Valeur::Vide; m.n_slots as usize];
        for (i, v) in args.iter().enumerate() { slots[i] = v.clone(); }
        Ok(CadreJvm {
            classe: classe.to_string(), methode: nom_meth.to_string(),
            slots, pile: Vec::new(), pc: 0,
        })
    }

    /// Table d'exceptions « résolue » d'une méthode : types comparés par NOM
    /// de symbole (donc valables même à travers un déroulement inter-classes).
    fn gestionnaires(&self, classe: &str, methode: &str)
        -> Result<Vec<(u32, u32, u32, Option<String>)>, Erreur>
    {
        let c = self.chargeur.classes.get(classe)
            .ok_or_else(|| Erreur::ClasseIntrouvable(classe.to_string()))?;
        let mi = c.index_methode(methode).ok_or_else(|| Erreur::MethodeIntrouvable {
            classe: classe.to_string(), methode: methode.to_string(),
        })?;
        let mut out = Vec::new();
        for e in &c.methodes[mi].exceptions {
            let t = if e.type_index == 0 { None }
                    else { Some(c.cp.symbole(e.type_index)?.to_string()) };
            out.push((e.debut, e.fin, e.cible, t));
        }
        Ok(out)
    }

    /// Termine le cadre au sommet : dépile, remet la valeur à l'appelant.
    fn terminer(&self, frames: &mut Vec<CadreJvm>, valeur: Valeur) -> Result<Option<Valeur>, Erreur> {
        frames.pop();
        if frames.is_empty() { return Ok(Some(valeur)); }
        let appelant = frames.last_mut().unwrap();
        appelant.pc += 1;
        appelant.pile.push(valeur);
        Ok(None)
    }

    /// Invoque une méthode et rend sa valeur (ou une erreur propre).
    ///
    /// Le `pc` de l'appelant reste sur l'instruction `Appel` pendant que le
    /// callee tourne : c'est ce qui permet à la table d'exceptions de couvrir
    /// l'appel exactement comme le fait la JVM.
    pub fn invoquer(&mut self, classe: &str, methode: &str, args: &[Valeur])
        -> Result<Valeur, Erreur>
    {
        let (def, idx) = self.chargeur.resoudre(classe, methode)?;
        let m0 = &self.chargeur.classes[&def].methodes[idx];
        let premier = self.nouveau_cadre(&def, methode, m0, args)?;
        let mut frames = vec![premier];

        while !frames.is_empty() {
            let (nom_classe, nom_meth, pc) = {
                let f = frames.last().unwrap();
                (f.classe.clone(), f.methode.clone(), f.pc)
            };
            let instr_opt = {
                let c = self.chargeur.classes.get(&nom_classe)
                    .ok_or_else(|| Erreur::ClasseIntrouvable(nom_classe.clone()))?;
                let mi = c.index_methode(&nom_meth).ok_or_else(|| Erreur::MethodeIntrouvable {
                    classe: nom_classe.clone(), methode: nom_meth.clone(),
                })?;
                c.methodes[mi].code.get(pc).cloned()
            };
            // Fin implicite du code = retour (sommet de pile ou Vide).
            let instr = match instr_opt {
                Some(i) => i,
                None => {
                    let v = frames.last_mut().unwrap().pile.pop().unwrap_or(Valeur::Vide);
                    if let Some(r) = self.terminer(&mut frames, v)? { return Ok(r); }
                    continue;
                }
            };

            self.instructions += 1;
            if self.instructions > self.limites.budget_instructions {
                return Err(Erreur::BudgetDepasse(self.limites.budget_instructions));
            }

            match instr {
                InstrBc::PousserConst { index } => {
                    let v = self.valeur_const(&nom_classe, index)?;
                    let f = frames.last_mut().unwrap();
                    f.pile.push(v); f.pc += 1;
                }
                InstrBc::ChargerSlot { slot } => {
                    let f = frames.last_mut().unwrap();
                    let v = f.slots.get(slot as usize).cloned()
                        .ok_or(Erreur::SlotHorsBornes { slot: slot as u32, n_slots: f.slots.len() as u32 })?;
                    f.pile.push(v); f.pc += 1;
                }
                InstrBc::StockerSlot { slot } => {
                    let f = frames.last_mut().unwrap();
                    let v = pop_pile(&mut f.pile, &nom_meth, pc)?;
                    if (slot as usize) >= f.slots.len() {
                        return Err(Erreur::SlotHorsBornes { slot: slot as u32, n_slots: f.slots.len() as u32 });
                    }
                    f.slots[slot as usize] = v; f.pc += 1;
                }
                InstrBc::Dupliquer => {
                    let f = frames.last_mut().unwrap();
                    let v = pop_pile(&mut f.pile, &nom_meth, pc)?;
                    f.pile.push(v.clone()); f.pile.push(v); f.pc += 1;
                }
                InstrBc::Binaire { op } => {
                    let b = { let f = frames.last_mut().unwrap(); pop_pile(&mut f.pile, &nom_meth, pc)? };
                    let a = { let f = frames.last_mut().unwrap(); pop_pile(&mut f.pile, &nom_meth, pc)? };
                    let txt = match op {
                        Oper::Add => "+", Oper::Sub => "-", Oper::Mul => "*", Oper::Div => "/",
                        Oper::Mod => "%", Oper::Puissance => "^", Oper::DivEntier => "div",
                        Oper::Lt => "<", Oper::Le => "<=", Oper::Gt => ">", Oper::Ge => ">=",
                        Oper::Eq => "==", Oper::Ne => "!=", Oper::Et => "et", Oper::Ou => "ou",
                    };
                    let r = crate::interp::binaire_valeurs(txt, &a, &b)
                        .map_err(|_| Erreur::ExecutionEnErreur)?;
                    let f = frames.last_mut().unwrap();
                    f.pile.push(r); f.pc += 1;
                }
                InstrBc::Neg | InstrBc::Non => {
                    let v = { let f = frames.last_mut().unwrap(); pop_pile(&mut f.pile, &nom_meth, pc)? };
                    let txt = if matches!(instr, InstrBc::Neg) { "-" } else { "non" };
                    let r = crate::interp::unaire_valeur(txt, &v)
                        .map_err(|_| Erreur::ExecutionEnErreur)?;
                    let f = frames.last_mut().unwrap();
                    f.pile.push(r); f.pc += 1;
                }
                InstrBc::Saut { cible } => {
                    let f = frames.last_mut().unwrap(); f.pc = cible as usize;
                }
                InstrBc::SautSiFaux { cible } => {
                    let f = frames.last_mut().unwrap();
                    let v = pop_pile(&mut f.pile, &nom_meth, pc)?;
                    if !matches!(v, Valeur::Booleen(true)) { f.pc = cible as usize; }
                    else { f.pc += 1; }
                }
                InstrBc::Appel { methode } => {
                    self.appels += 1;
                    if self.appels > self.limites.max_appels {
                        return Err(Erreur::AppelsEpuises(self.limites.max_appels));
                    }
                    let (dc, dn, _) = {
                        let cp = self.cp_de(&nom_classe)?;
                        ref_method_depuis_cp(cp, methode)?
                    };
                    let (def, di) = self.chargeur.resoudre(&dc, &dn)?;
                    let arite = self.chargeur.classes[&def].methodes[di].descr.parametres.len();
                    let mut args_appel = Vec::with_capacity(arite);
                    {
                        let f = frames.last_mut().unwrap();
                        for _ in 0..arite {
                            args_appel.push(pop_pile(&mut f.pile, &nom_meth, pc)?);
                        }
                    }
                    args_appel.reverse();
                    if frames.len() >= self.limites.profondeur_max {
                        return Err(Erreur::ProfondeurDepassee(self.limites.profondeur_max));
                    }
                    let nom_appel = dn.clone();
                    let cadre = {
                        let m = &self.chargeur.classes[&def].methodes[di];
                        self.nouveau_cadre(&def, &nom_appel, m, &args_appel)?
                    };
                    frames.push(cadre);
                }
                InstrBc::RetourVide => {
                    if let Some(r) = self.terminer(&mut frames, Valeur::Vide)? { return Ok(r); }
                }
                InstrBc::RetourValeur => {
                    let v = { let f = frames.last_mut().unwrap(); pop_pile(&mut f.pile, &nom_meth, pc)? };
                    if let Some(r) = self.terminer(&mut frames, v)? { return Ok(r); }
                }
                InstrBc::Afficher => {
                    let v = { let f = frames.last_mut().unwrap(); pop_pile(&mut f.pile, &nom_meth, pc)? };
                    self.hote.afficher_valeur(&v);
                    let f = frames.last_mut().unwrap(); f.pc += 1;
                }
                InstrBc::Lever { type_index, msg_index } => {
                    let typ = self.symbole_de(&nom_classe, type_index)?;
                    let msg = crate::run::chaine_de(&self.valeur_const(&nom_classe, msg_index)?);
                    let mut attrape = false;
                    loop {
                        let (fc, fm, fpc) = {
                            let f = frames.last().unwrap();
                            (f.classe.clone(), f.methode.clone(), f.pc)
                        };
                        let handlers = self.gestionnaires(&fc, &fm)?;
                        let trouve = handlers.iter().find(|(d, fi, _, t)| {
                            fpc >= *d as usize && fpc < *fi as usize
                                && (t.is_none() || t.as_deref() == Some(typ.as_str()))
                        }).map(|(_, _, c, _)| *c as usize);
                        if let Some(cible) = trouve {
                            let f = frames.last_mut().unwrap();
                            f.pile.clear();
                            f.pile.push(Valeur::s(&msg));
                            f.pc = cible;
                            attrape = true;
                            break;
                        }
                        if frames.len() == 1 { break; }
                        frames.pop();
                    }
                    if !attrape { return Err(Erreur::ExceptionNonAttrapee(typ)); }
                }
            }
        }
        Ok(Valeur::Vide)
    }
}

/// Raccourci : vérifie, charge et exécute un fichier `.rosebc` v1 ou v2 dans
/// une sandbox complète (instructions, profondeur, appels, mémoire, E/S).
pub fn executer_fichier(octets: &[u8], classe: &str, methode: &str,
                        arguments: &[Valeur], hote: &mut HoteBc,
                        limites: &LimitesJvm) -> Result<Valeur, Erreur> {
    let chargeur = charger_fichier(octets)?;
    let mut machine = MachineJvm::neuf(&chargeur, hote, limites.clone());
    machine.invoquer(classe, methode, arguments)
}

// ── Sérialisation du fichier de classe v2 ──────────────────────────────────

/// Écrit une classe au format `.rosebc` v2 (little-endian, auto-contained).
pub fn ecrire_classe(c: &ClasseRose) -> Vec<u8> {
    let mut out = Vec::with_capacity(64 + c.methodes.len() * 32);
    out.extend_from_slice(MAGIC);
    pusher_u32(&mut out, VERSION_JVM);
    pusher_chaine(&mut out, &c.nom);
    pusher_u32(&mut out, c.version);
    match &c.super_classe {
        None => out.push(0),
        Some(s) => { out.push(1); pusher_chaine(&mut out, s); }
    }
    pusher_u32(&mut out, c.interfaces.len() as u32);
    for i in &c.interfaces { pusher_chaine(&mut out, i); }
    ecrire_cp(&mut out, &c.cp);
    pusher_u32(&mut out, c.methodes.len() as u32);
    for m in &c.methodes {
        pusher_chaine(&mut out, &m.nom);
        m.descr.ecrire(&mut out);
        pusher_u16(&mut out, m.drapeaux);
        pusher_u16(&mut out, m.n_slots);
        for nom in &m.noms_slots { pusher_chaine(&mut out, nom); }
        pusher_u32(&mut out, m.code.len() as u32);
        for i in &m.code { ecrire_instr(&mut out, i); }
        pusher_u32(&mut out, m.exceptions.len() as u32);
        for e in &m.exceptions {
            pusher_u32(&mut out, e.debut); pusher_u32(&mut out, e.fin);
            pusher_u32(&mut out, e.cible); pusher_u16(&mut out, e.type_index);
        }
    }
    out
}

/// Lit un `.rosebc`. Accepte la **version 1** (snapshot `CorpsRir`, promu en
/// une classe `<principal>`) et la **version 2** (fichier de classe complet).
pub fn lire_classe(octets: &[u8]) -> Result<ClasseRose, Erreur> {
    if octets.len() < 10 { return Err(Erreur::Tronque { attendu: 10, present: octets.len() }); }
    if &octets[..6] != MAGIC { return Err(Erreur::MagicInvalide); }
    let version = u32::from_le_bytes([octets[6], octets[7], octets[8], octets[9]]);
    if version == VERSION {
        // Rétrocompatibilité : rejouer le lecteur v1, puis promouvoir.
        let corps = lire_bytecode(octets)?;
        return promouvoir_corps(&corps, "<principal>");
    }
    if version != VERSION_JVM { return Err(Erreur::VersionInsupportee(version)); }

    let mut l = Lecteur { octets, pos: 10 };
    let nom = l.chaine()?;
    let version_classe = l.u32()?;
    let super_classe = if l.u8()? == 1 { Some(l.chaine()?) } else { None };
    let n_int = l.u32()? as usize;
    if n_int > l.reste() { return Err(Erreur::Tronque { attendu: n_int, present: l.reste() }); }
    let mut interfaces = Vec::with_capacity(n_int);
    for _ in 0..n_int { interfaces.push(l.chaine()?); }
    let cp = lire_cp(&mut l)?;
    if cp.entrees().len() > u16::MAX as usize { return Err(Erreur::PoolPlein); }
    let n_m = l.u32()? as usize;
    if n_m > l.reste() { return Err(Erreur::Tronque { attendu: n_m, present: l.reste() }); }
    let mut methodes = Vec::with_capacity(n_m);
    for _ in 0..n_m {
        let m_nom = l.chaine()?;
        let descr = Descripteur::lire(&mut l)?;
        let drapeaux = l.u16()?;
        let n_slots = l.u16()?;
        let mut noms_slots = Vec::with_capacity(n_slots as usize);
        for _ in 0..n_slots { noms_slots.push(l.chaine()?); }
        let n_code = l.u32()? as usize;
        if n_code > l.reste() { return Err(Erreur::Tronque { attendu: n_code, present: l.reste() }); }
        let mut code = Vec::with_capacity(n_code);
        for _ in 0..n_code { code.push(lire_instr(&mut l)?); }
        let n_exc = l.u32()? as usize;
        if n_exc > l.reste() { return Err(Erreur::Tronque { attendu: n_exc, present: l.reste() }); }
        let mut exceptions = Vec::with_capacity(n_exc);
        for _ in 0..n_exc {
            exceptions.push(EntreeException {
                debut: l.u32()?, fin: l.u32()?, cible: l.u32()?, type_index: l.u16()?,
            });
        }
        methodes.push(MethodeRose {
            nom: m_nom, descr, drapeaux, n_slots, noms_slots, code, exceptions,
        });
    }
    if l.pos != octets.len() { return Err(Erreur::OctetsEnTrop(octets.len() - l.pos)); }
    let c = ClasseRose {
        nom, version: version_classe, cp, super_classe, interfaces, methodes,
        table_dispatch: HashMap::new(),
    };
    verifier_classe(&c)?;
    let mut chargeur = ChargeurClasses::neuf();
    chargeur.ajouter_brut(c.clone());
    chargeur.recalculer_dispatches();
    chargeur.verifier_lien()?;
    Ok(c)
}

/// Charge un fichier v1 ou v2 dans un `ChargeurClasses` prêt à exécuter.
pub fn charger_fichier(octets: &[u8]) -> Result<ChargeurClasses, Erreur> {
    let c = lire_classe(octets)?;
    let mut ch = ChargeurClasses::neuf();
    ch.ajouter_brut(c);
    ch.recalculer_dispatches();
    ch.verifier_lien()?;
    Ok(ch)
}

/// Compile un `CorpsRir` v1 en classe v2 à une seule méthode. C'est le pont
/// qui garantit que tout bytecode v1 reste exécutable par la machine v2.
pub fn promouvoir_corps(corps: &CorpsRir, nom_methode: &str) -> Result<ClasseRose, Erreur> {
    // Le RIR ne conserve pas l'arity : on expose TOUS les slots comme
    // paramètres (le snapshot v1 les posait tous au démarrage). Un appelant
    // qui connaît l'arity réelle peut utiliser `promouvoir_corps_avec_parametres`.
    promouvoir_corps_avec_parametres(corps, nom_methode, corps.n_slots)
}

/// Comme `promouvoir_corps`, mais avec une arité explicite (connue de la
/// signature Rose d'origine).
pub fn promouvoir_corps_avec_parametres(corps: &CorpsRir, nom_methode: &str,
                                        n_parametres: usize) -> Result<ClasseRose, Erreur> {
    verifier_sous_ensemble(corps)?;
    if n_parametres > corps.n_slots {
        return Err(Erreur::AriteIncoherente {
            methode: nom_methode.to_string(), attendus: n_parametres, fournis: corps.n_slots,
        });
    }
    let n_slots = u16::try_from(corps.n_slots)
        .map_err(|_| Erreur::MemoireDepassee(corps.n_slots))?;
    let mut cp = ConstantPool::neuf();
    let mut code: Vec<InstrBc> = Vec::with_capacity(corps.ops.len());
    let mut frontieres: Vec<usize> = Vec::with_capacity(corps.ops.len());
    for op in &corps.ops {
        frontieres.push(code.len());
        compiler_op(op, &mut cp, &mut code)?;
    }
    // Réécriture des cibles : indices d'opérations RIR -> indices d'instructions.
    for ins in &mut code {
        match ins {
            InstrBc::Saut { cible } | InstrBc::SautSiFaux { cible } => {
                let src = *cible as usize;
                let dst = frontieres.get(src).copied().ok_or(Erreur::SautHorsBornes {
                    cible: *cible, n_ops: frontieres.len() as u32,
                })?;
                *cible = dst as u32;
            }
            _ => {}
        }
    }
    let descr = Descripteur {
        parametres: vec![TyDesc::Inconnu; n_parametres],
        retour: TyDesc::Inconnu,
    };
    let methode = MethodeRose {
        nom: nom_methode.to_string(), descr, drapeaux: 0, n_slots,
        noms_slots: corps.noms.clone(), code, exceptions: Vec::new(),
    };
    let mut classe = ClasseRose::neuf("Principal");
    classe.cp = cp;
    classe.ajouter_methode(methode)?;
    Ok(classe)
}

// ═══════════════════════════════════════════════════════════════════════════
// PARTIE 3 — SNAPSHOT AST COMPLET (ROSEBC v3) : UNE RVM COMPLÈTE
// ═══════════════════════════════════════════════════════════════════════════
//
// ⚠⚠⚠ POURQUOI CETTE COUCHE. Le snapshot v1 (PARTIE 1) ne capture que le
// corps principal ABAISSE en RIR : les appels de fonction, les chaînes, les
// tables, les rationnels, les E/S et les expressions hors sous-ensemble y
// sont marqués `OP_REFUSE` (0x80) et rejetés au chargement. La classe v2
// (PARTIE 2) sait exécuter des METHODES avec une vraie pile d'appels — mais
// son compilateur n'accepte, lui aussi, que les expressions abstraites.
//
// Le format v3 comble le vide SANS inventer une dépendance et SANS réécrire
// le langage : il sérialise le **programme Rose entier** (toutes les
// signatures + le principal) en une suite d'opcodes tagués. C'est un
// SNAPSHOT DU RIR/AST — le même arbre que l'analyseur a produit — avec :
//
//   * un conteneur versionné (magic `ROSEBC`, version u32 = 3) ;
//   * des longueurs u32 préfixées (les textes exacts, les big signs, etc.) ;
//   * un vérificateur qui ne panique jamais sur un fichier hostile ;
//   * une exécution par la RVM à travers **la mécanique Rose** : `Interp`,
//     ses slots et sa pile de trames. C'est exactement le mécanisme de
//     l'interpréteur, demandé par la mission — et c'est ce qui garantit une
//     parité TOTALE (appels, récursion, chaînes, tables, rationnels, E/S,
//     exceptions, hasard…) au lieu d'une parité de sous-ensemble.
//
// ⚠ CE N'EST PAS UNE JVM JAVA : aucun bytecode Java, aucun démarrage de JVM,
// aucune classe `.class`, aucun JVMS §4.10. Les « opcodes » ci-dessous sont
// les nôtres, et le moteur d'exécution est celui de Rose. On a seulement
// emprunté à Java l'IDEE d'un format de fichier exécutable indépendant.
//
// ── Format v3 (little-endian) ─────────────────────────────────────────────
//
//   magic[6] | version u32 (= 3)
//   n_imports u32 | n_imports × chaine
//   n_signatures u32 | n_signatures × signature
//       signature = nom chaine | n_params u32 | n × (nom chaine, type Ty)
//                   | retour Ty | a_retour u8 | present_corps u8
//                   | [n_instr u32 + n_instr × instruction]
//                   | dll chaine | dll_son chaine | symbole chaine | ligne u32
//   n_instructions u32 | n_instructions × instruction
//
//   type Ty = tag TyKind u8 | tag elt TyKind u8
//   expression = tag u8 | charges (voir `ecrire_expr3`)
//   instruction = tag u8 | charges (voir `ecrire_instr3`)
//   chaine = u32 len | UTF-8
//
// ⚠ Le champ `Signature::rir` n'est PAS sérialisé : c'est un cache de
// l'interpréteur (avec des pointeurs/caches de slots), reconstruit à la
// demande. Le snapshot contient les CORPS SOURCE, qui sont la vérité.

// ── Tags du snapshot v3 ────────────────────────────────────────────────────

// TyKind
const TK3_VIDE: u8 = 0;
const TK3_ENTIER: u8 = 1;
const TK3_RATIONNEL: u8 = 2;
const TK3_FLOTTANT: u8 = 3;
const TK3_BOOLEEN: u8 = 4;
const TK3_CHAINE: u8 = 5;
const TK3_LISTE: u8 = 6;
const TK3_TABLE: u8 = 7;
const TK3_TAMPON: u8 = 8;
const TK3_TABLEAU: u8 = 9;
const TK3_COMPLEXE: u8 = 10;
const TK3_IRRATIONNEL: u8 = 11;
const TK3_DYN: u8 = 12;

// Expr
const EX3_LITTERAL: u8 = 0x01;
const EX3_LITCOMPLEXE: u8 = 0x02;
const EX3_CHAINE: u8 = 0x03;
const EX3_BOOLEEN: u8 = 0x04;
const EX3_VAR: u8 = 0x05;
const EX3_APPEL: u8 = 0x06; // Appel de fonction OU de primitive std (nom + module)
const EX3_BINAIRE: u8 = 0x07;
const EX3_UNAIRE: u8 = 0x08;
const EX3_FACTORIELLE: u8 = 0x09;
const EX3_MULTIFACTORIELLE: u8 = 0x0A;
const EX3_SOUSFACTORIELLE: u8 = 0x0B;
const EX3_PRIMORIELLE: u8 = 0x0C;
const EX3_TERMINAL: u8 = 0x0D;
const EX3_SUPERFACTORIELLE: u8 = 0x0E;
const EX3_INDEX: u8 = 0x0F;
const EX3_LISTE: u8 = 0x10; // table/liste littérale
const EX3_LISTEVIDETYPEE: u8 = 0x11;
const EX3_TABLE: u8 = 0x12; // table associative (clé → valeur)

// Instr
const IN3_AFFICHER: u8 = 0x01; // E/S sortie
const IN3_LIRE: u8 = 0x02; // E/S entrée
const IN3_AFFECT: u8 = 0x03;
const IN3_AFFECTINDEX: u8 = 0x04;
const IN3_SI: u8 = 0x05;
const IN3_TANTQUE: u8 = 0x06;
const IN3_POUR: u8 = 0x07;
const IN3_RETOUR: u8 = 0x08;
const IN3_EXPRSTMT: u8 = 0x09;
const IN3_BLOC: u8 = 0x0A;
const IN3_SPAWN: u8 = 0x0B;
const IN3_ESSAYER: u8 = 0x0C;
const IN3_LEVER: u8 = 0x0D;

/// Profondeur maximale d'imbrication des INSTRUCTIONS du snapshot (anti-DoS).
/// les expressions utilisent `PROFONDEUR_MAX` (déjà défini, 64).
const PROFONDEUR_INSTR_MAX: usize = 256;

fn pusher_bool(dst: &mut Vec<u8>, b: bool) { dst.push(if b { 1 } else { 0 }); }
fn lire_bool(l: &mut Lecteur<'_>) -> Result<bool, Erreur> {
    match l.u8()? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(Erreur::SnapshotInvalide("booléen autre que 0/1")),
    }
}

fn code_tykind(t: TyKind) -> u8 {
    match t {
        TyKind::Vide => TK3_VIDE, TyKind::Entier => TK3_ENTIER,
        TyKind::Rationnel => TK3_RATIONNEL, TyKind::Flottant => TK3_FLOTTANT,
        TyKind::Booleen => TK3_BOOLEEN, TyKind::Chaine => TK3_CHAINE,
        TyKind::Liste => TK3_LISTE, TyKind::Table => TK3_TABLE,
        TyKind::Tampon => TK3_TAMPON, TyKind::Tableau => TK3_TABLEAU,
        TyKind::Complexe => TK3_COMPLEXE, TyKind::Irrationnel => TK3_IRRATIONNEL,
        TyKind::Dyn => TK3_DYN,
    }
}
fn lire_tykind(l: &mut Lecteur<'_>) -> Result<TyKind, Erreur> {
    Ok(match l.u8()? {
        TK3_VIDE => TyKind::Vide, TK3_ENTIER => TyKind::Entier,
        TK3_RATIONNEL => TyKind::Rationnel, TK3_FLOTTANT => TyKind::Flottant,
        TK3_BOOLEEN => TyKind::Booleen, TK3_CHAINE => TyKind::Chaine,
        TK3_LISTE => TyKind::Liste, TK3_TABLE => TyKind::Table,
        TK3_TAMPON => TyKind::Tampon, TK3_TABLEAU => TyKind::Tableau,
        TK3_COMPLEXE => TyKind::Complexe, TK3_IRRATIONNEL => TyKind::Irrationnel,
        TK3_DYN => TyKind::Dyn,
        _ => return Err(Erreur::SnapshotInvalide("tag de type inconnu")),
    })
}
fn ecrire_ty(dst: &mut Vec<u8>, t: &Ty) {
    dst.push(code_tykind(t.n));
    dst.push(code_tykind(t.elt));
}
fn lire_ty(l: &mut Lecteur<'_>) -> Result<Ty, Erreur> {
    Ok(Ty { n: lire_tykind(l)?, elt: lire_tykind(l)? })
}

fn ecrire_expr3(dst: &mut Vec<u8>, e: &Expr) {
    match e {
        Expr::Litteral { texte, flott, rationnel, neg, ty, ligne } => {
            dst.push(EX3_LITTERAL);
            pusher_chaine(dst, texte);
            pusher_bool(dst, *flott); pusher_bool(dst, *rationnel); pusher_bool(dst, *neg);
            dst.push(code_tykind(*ty)); // Expr::Litteral porte un TyKind (pas un Ty)
            pusher_u32(dst, *ligne as u32);
        }
        Expr::LitComplexe { re, im, ligne } => {
            dst.push(EX3_LITCOMPLEXE);
            dst.extend_from_slice(&re.to_le_bytes());
            dst.extend_from_slice(&im.to_le_bytes());
            pusher_u32(dst, *ligne as u32);
        }
        Expr::ChaineLit(s, ligne) => {
            dst.push(EX3_CHAINE); pusher_chaine(dst, s); pusher_u32(dst, *ligne as u32);
        }
        Expr::BooleenLit(b, ligne) => {
            dst.push(EX3_BOOLEEN); pusher_bool(dst, *b); pusher_u32(dst, *ligne as u32);
        }
        Expr::Var(n, ligne) => {
            dst.push(EX3_VAR); pusher_chaine(dst, n); pusher_u32(dst, *ligne as u32);
        }
        Expr::Appel { nom, module, args, ligne } => {
            dst.push(EX3_APPEL);
            pusher_chaine(dst, nom); pusher_chaine(dst, module);
            pusher_u32(dst, args.len() as u32);
            for a in args { ecrire_expr3(dst, a); }
            pusher_u32(dst, *ligne as u32);
        }
        Expr::Binaire { op, a, b, ligne } => {
            dst.push(EX3_BINAIRE); pusher_chaine(dst, op);
            ecrire_expr3(dst, a); ecrire_expr3(dst, b);
            pusher_u32(dst, *ligne as u32);
        }
        Expr::Unaire { op, a, ligne } => {
            dst.push(EX3_UNAIRE); pusher_chaine(dst, op);
            ecrire_expr3(dst, a); pusher_u32(dst, *ligne as u32);
        }
        Expr::Factorielle(a, ligne) => {
            dst.push(EX3_FACTORIELLE); ecrire_expr3(dst, a); pusher_u32(dst, *ligne as u32);
        }
        Expr::MultiFactorielle(a, k, ligne) => {
            dst.push(EX3_MULTIFACTORIELLE); ecrire_expr3(dst, a);
            dst.push(*k); pusher_u32(dst, *ligne as u32);
        }
        Expr::SousFactorielle(a, ligne) => {
            dst.push(EX3_SOUSFACTORIELLE); ecrire_expr3(dst, a); pusher_u32(dst, *ligne as u32);
        }
        Expr::Primorielle(a, ligne) => {
            dst.push(EX3_PRIMORIELLE); ecrire_expr3(dst, a); pusher_u32(dst, *ligne as u32);
        }
        Expr::Terminal(a, ligne) => {
            dst.push(EX3_TERMINAL); ecrire_expr3(dst, a); pusher_u32(dst, *ligne as u32);
        }
        Expr::SuperFactorielle(a, ligne) => {
            dst.push(EX3_SUPERFACTORIELLE); ecrire_expr3(dst, a); pusher_u32(dst, *ligne as u32);
        }
        Expr::Index { a, b, ligne } => {
            dst.push(EX3_INDEX); ecrire_expr3(dst, a); ecrire_expr3(dst, b);
            pusher_u32(dst, *ligne as u32);
        }
        Expr::ListeLit(v, ligne) => {
            dst.push(EX3_LISTE); pusher_u32(dst, v.len() as u32);
            for x in v { ecrire_expr3(dst, x); }
            pusher_u32(dst, *ligne as u32);
        }
        Expr::ListeVideTypee(t, ligne) => {
            dst.push(EX3_LISTEVIDETYPEE); dst.push(code_tykind(*t)); pusher_u32(dst, *ligne as u32);
        }
        Expr::TableLit(v, ligne) => {
            dst.push(EX3_TABLE); pusher_u32(dst, v.len() as u32);
            for (k, x) in v { pusher_chaine(dst, k); ecrire_expr3(dst, x); }
            pusher_u32(dst, *ligne as u32);
        }
    }
}

fn lire_expr3(l: &mut Lecteur<'_>, prof: usize) -> Result<Expr, Erreur> {
    if prof > PROFONDEUR_MAX { return Err(Erreur::ExpressionTropProfonde); }
    let tag = l.u8()?;
    Ok(match tag {
        EX3_LITTERAL => {
            let texte = l.chaine()?;
            let flott = lire_bool(l)?;
            let rationnel = lire_bool(l)?;
            let neg = lire_bool(l)?;
            let ty = lire_tykind(l)?;
            let ligne = l.u32()? as usize;
            Expr::Litteral { texte, flott, rationnel, neg, ty, ligne }
        }
        EX3_LITCOMPLEXE => {
            let b = l.prendre(16)?;
            let re = f64::from_le_bytes([b[0],b[1],b[2],b[3],b[4],b[5],b[6],b[7]]);
            let im = f64::from_le_bytes([b[8],b[9],b[10],b[11],b[12],b[13],b[14],b[15]]);
            let ligne = l.u32()? as usize;
            Expr::LitComplexe { re, im, ligne }
        }
        EX3_CHAINE => { let s = l.chaine()?; let ligne = l.u32()? as usize; Expr::ChaineLit(s, ligne) }
        EX3_BOOLEEN => { let b = lire_bool(l)?; let ligne = l.u32()? as usize; Expr::BooleenLit(b, ligne) }
        EX3_VAR => { let n = l.chaine()?; let ligne = l.u32()? as usize; Expr::Var(n, ligne) }
        EX3_APPEL => {
            let nom = l.chaine()?;
            let module = l.chaine()?;
            let n = l.u32()? as usize;
            if n > l.reste() { return Err(Erreur::SnapshotInvalide("trop d'arguments d'appel")); }
            let mut args = Vec::with_capacity(n);
            for _ in 0..n { args.push(lire_expr3(l, prof + 1)?); }
            let ligne = l.u32()? as usize;
            Expr::Appel { nom, module, args, ligne }
        }
        EX3_BINAIRE => {
            let op = l.chaine()?;
            let a = Box::new(lire_expr3(l, prof + 1)?);
            let b = Box::new(lire_expr3(l, prof + 1)?);
            let ligne = l.u32()? as usize;
            Expr::Binaire { op, a, b, ligne }
        }
        EX3_UNAIRE => {
            let op = l.chaine()?;
            let a = Box::new(lire_expr3(l, prof + 1)?);
            let ligne = l.u32()? as usize;
            Expr::Unaire { op, a, ligne }
        }
        EX3_FACTORIELLE => {
            let a = Box::new(lire_expr3(l, prof + 1)?);
            let ligne = l.u32()? as usize;
            Expr::Factorielle(a, ligne)
        }
        EX3_MULTIFACTORIELLE => {
            let a = Box::new(lire_expr3(l, prof + 1)?);
            let k = l.u8()?;
            let ligne = l.u32()? as usize;
            Expr::MultiFactorielle(a, k, ligne)
        }
        EX3_SOUSFACTORIELLE => {
            let a = Box::new(lire_expr3(l, prof + 1)?);
            let ligne = l.u32()? as usize;
            Expr::SousFactorielle(a, ligne)
        }
        EX3_PRIMORIELLE => {
            let a = Box::new(lire_expr3(l, prof + 1)?);
            let ligne = l.u32()? as usize;
            Expr::Primorielle(a, ligne)
        }
        EX3_TERMINAL => {
            let a = Box::new(lire_expr3(l, prof + 1)?);
            let ligne = l.u32()? as usize;
            Expr::Terminal(a, ligne)
        }
        EX3_SUPERFACTORIELLE => {
            let a = Box::new(lire_expr3(l, prof + 1)?);
            let ligne = l.u32()? as usize;
            Expr::SuperFactorielle(a, ligne)
        }
        EX3_INDEX => {
            let a = Box::new(lire_expr3(l, prof + 1)?);
            let b = Box::new(lire_expr3(l, prof + 1)?);
            let ligne = l.u32()? as usize;
            Expr::Index { a, b, ligne }
        }
        EX3_LISTE => {
            let n = l.u32()? as usize;
            if n > l.reste() { return Err(Erreur::SnapshotInvalide("trop d'éléments de liste")); }
            let mut v = Vec::with_capacity(n);
            for _ in 0..n { v.push(lire_expr3(l, prof + 1)?); }
            let ligne = l.u32()? as usize;
            Expr::ListeLit(v, ligne)
        }
        EX3_LISTEVIDETYPEE => {
            let t = lire_tykind(l)?;
            let ligne = l.u32()? as usize;
            Expr::ListeVideTypee(t, ligne)
        }
        EX3_TABLE => {
            let n = l.u32()? as usize;
            if n > l.reste() { return Err(Erreur::SnapshotInvalide("trop d'entrées de table")); }
            let mut v = Vec::with_capacity(n);
            for _ in 0..n {
                let k = l.chaine()?;
                let x = lire_expr3(l, prof + 1)?;
                v.push((k, x));
            }
            let ligne = l.u32()? as usize;
            Expr::TableLit(v, ligne)
        }
        _ => return Err(Erreur::SnapshotInvalide("tag d'expression inconnu")),
    })
}

fn ecrire_instr3(dst: &mut Vec<u8>, i: &Instr) {
    match i {
        Instr::Rir(_) => {
            // Ne JAMAIS sérialiser un cache RIR : c'est un dérivé du corps
            // source, reconstruit par l'interpréteur. Un snapshot incomplet
            // serait une erreur de notre compilateur, on le signale.
            dst.push(0xFF);
        }
        Instr::Afficher(es, ligne) => {
            dst.push(IN3_AFFICHER); pusher_u32(dst, es.len() as u32);
            for e in es { ecrire_expr3(dst, e); }
            pusher_u32(dst, *ligne as u32);
        }
        Instr::Lire { nom, ty, ligne } => {
            dst.push(IN3_LIRE); pusher_chaine(dst, nom); dst.push(code_tykind(*ty));
            pusher_u32(dst, *ligne as u32);
        }
        Instr::Affect { nom, e, ligne } => {
            dst.push(IN3_AFFECT); pusher_chaine(dst, nom); ecrire_expr3(dst, e);
            pusher_u32(dst, *ligne as u32);
        }
        Instr::AffectIndex { nom, index, valeur, ligne } => {
            dst.push(IN3_AFFECTINDEX); pusher_chaine(dst, nom);
            ecrire_expr3(dst, index); ecrire_expr3(dst, valeur);
            pusher_u32(dst, *ligne as u32);
        }
        Instr::Si { test, alors, sinon, ligne } => {
            dst.push(IN3_SI); ecrire_expr3(dst, test);
            pusher_u32(dst, alors.len() as u32);
            for x in alors { ecrire_instr3(dst, x); }
            pusher_bool(dst, sinon.is_some());
            if let Some(s) = sinon {
                pusher_u32(dst, s.len() as u32);
                for x in s { ecrire_instr3(dst, x); }
            }
            pusher_u32(dst, *ligne as u32);
        }
        Instr::TantQue { test, corps, ligne } => {
            dst.push(IN3_TANTQUE); ecrire_expr3(dst, test);
            pusher_u32(dst, corps.len() as u32);
            for x in corps { ecrire_instr3(dst, x); }
            pusher_u32(dst, *ligne as u32);
        }
        Instr::Pour { nom, debut, fin, corps, ligne } => {
            dst.push(IN3_POUR); pusher_chaine(dst, nom);
            ecrire_expr3(dst, debut); ecrire_expr3(dst, fin);
            pusher_u32(dst, corps.len() as u32);
            for x in corps { ecrire_instr3(dst, x); }
            pusher_u32(dst, *ligne as u32);
        }
        Instr::Retour(e, ligne) => {
            dst.push(IN3_RETOUR); pusher_bool(dst, e.is_some());
            if let Some(x) = e { ecrire_expr3(dst, x); }
            pusher_u32(dst, *ligne as u32);
        }
        Instr::ExprStmt(e) => { dst.push(IN3_EXPRSTMT); ecrire_expr3(dst, e); }
        Instr::Bloc(b) => {
            dst.push(IN3_BLOC); pusher_u32(dst, b.len() as u32);
            for x in b { ecrire_instr3(dst, x); }
        }
        Instr::Spawn(b, ligne) => {
            dst.push(IN3_SPAWN); pusher_u32(dst, b.len() as u32);
            for x in b { ecrire_instr3(dst, x); }
            pusher_u32(dst, *ligne as u32);
        }
        Instr::Essayer { corps, attraper, finalment, ligne } => {
            dst.push(IN3_ESSAYER);
            pusher_u32(dst, corps.len() as u32);
            for x in corps { ecrire_instr3(dst, x); }
            pusher_bool(dst, attraper.is_some());
            if let Some((nom, b)) = attraper {
                pusher_chaine(dst, nom);
                pusher_u32(dst, b.len() as u32);
                for x in b { ecrire_instr3(dst, x); }
            }
            pusher_bool(dst, finalment.is_some());
            if let Some(b) = finalment {
                pusher_u32(dst, b.len() as u32);
                for x in b { ecrire_instr3(dst, x); }
            }
            pusher_u32(dst, *ligne as u32);
        }
        Instr::Lever(e, ligne) => {
            dst.push(IN3_LEVER); ecrire_expr3(dst, e); pusher_u32(dst, *ligne as u32);
        }
    }
}

fn lire_instrs3(l: &mut Lecteur<'_>, prof: usize) -> Result<Vec<Instr>, Erreur> {
    if prof > PROFONDEUR_INSTR_MAX { return Err(Erreur::ExpressionTropProfonde); }
    let n = l.u32()? as usize;
    if n > l.reste() { return Err(Erreur::SnapshotInvalide("trop d'instructions")); }
    let mut out = Vec::with_capacity(n);
    for _ in 0..n { out.push(lire_instr3(l, prof)?); }
    Ok(out)
}

fn lire_instr3(l: &mut Lecteur<'_>, prof: usize) -> Result<Instr, Erreur> {
    if prof > PROFONDEUR_INSTR_MAX { return Err(Erreur::ExpressionTropProfonde); }
    let tag = l.u8()?;
    Ok(match tag {
        IN3_AFFICHER => {
            let n = l.u32()? as usize;
            if n > l.reste() { return Err(Erreur::SnapshotInvalide("trop d'expressions affichées")); }
            let mut es = Vec::with_capacity(n);
            for _ in 0..n { es.push(lire_expr3(l, 0)?); }
            let ligne = l.u32()? as usize;
            Instr::Afficher(es, ligne)
        }
        IN3_LIRE => {
            let nom = l.chaine()?; let ty = lire_tykind(l)?; let ligne = l.u32()? as usize;
            Instr::Lire { nom, ty, ligne }
        }
        IN3_AFFECT => {
            let nom = l.chaine()?; let e = lire_expr3(l, 0)?; let ligne = l.u32()? as usize;
            Instr::Affect { nom, e, ligne }
        }
        IN3_AFFECTINDEX => {
            let nom = l.chaine()?;
            let index = lire_expr3(l, 0)?;
            let valeur = lire_expr3(l, 0)?;
            let ligne = l.u32()? as usize;
            Instr::AffectIndex { nom, index, valeur, ligne }
        }
        IN3_SI => {
            let test = lire_expr3(l, 0)?;
            let alors = lire_instrs3(l, prof + 1)?;
            let a_sinon = lire_bool(l)?;
            let sinon = if a_sinon { Some(lire_instrs3(l, prof + 1)?) } else { None };
            let ligne = l.u32()? as usize;
            Instr::Si { test, alors, sinon, ligne }
        }
        IN3_TANTQUE => {
            let test = lire_expr3(l, 0)?;
            let corps = lire_instrs3(l, prof + 1)?;
            let ligne = l.u32()? as usize;
            Instr::TantQue { test, corps, ligne }
        }
        IN3_POUR => {
            let nom = l.chaine()?;
            let debut = lire_expr3(l, 0)?;
            let fin = lire_expr3(l, 0)?;
            let corps = lire_instrs3(l, prof + 1)?;
            let ligne = l.u32()? as usize;
            Instr::Pour { nom, debut, fin, corps, ligne }
        }
        IN3_RETOUR => {
            let present = lire_bool(l)?;
            let e = if present { Some(lire_expr3(l, 0)?) } else { None };
            let ligne = l.u32()? as usize;
            Instr::Retour(e, ligne)
        }
        IN3_EXPRSTMT => Instr::ExprStmt(lire_expr3(l, 0)?),
        IN3_BLOC => Instr::Bloc(lire_instrs3(l, prof + 1)?),
        IN3_SPAWN => {
            let b = lire_instrs3(l, prof + 1)?;
            let ligne = l.u32()? as usize;
            Instr::Spawn(b, ligne)
        }
        IN3_ESSAYER => {
            let corps = lire_instrs3(l, prof + 1)?;
            let a_attraper = lire_bool(l)?;
            let attraper = if a_attraper {
                let nom = l.chaine()?;
                let b = lire_instrs3(l, prof + 1)?;
                Some((nom, b))
            } else { None };
            let a_finalment = lire_bool(l)?;
            let finalment = if a_finalment { Some(lire_instrs3(l, prof + 1)?) } else { None };
            let ligne = l.u32()? as usize;
            Instr::Essayer { corps, attraper, finalment, ligne }
        }
        IN3_LEVER => {
            let e = lire_expr3(l, 0)?;
            let ligne = l.u32()? as usize;
            Instr::Lever(e, ligne)
        }
        0xFF => return Err(Erreur::SnapshotInvalide(
            "un corps RIR (cache) ne doit jamais être sérialisé")),
        _ => return Err(Erreur::SnapshotInvalide("tag d'instruction inconnu")),
    })
}

fn ecrire_signature3(dst: &mut Vec<u8>, s: &Signature) {
    pusher_chaine(dst, &s.nom);
    pusher_u32(dst, s.params.len() as u32);
    for (n, t) in &s.params { pusher_chaine(dst, n); ecrire_ty(dst, t); }
    ecrire_ty(dst, &s.retour);
    pusher_bool(dst, s.a_retour);
    pusher_bool(dst, s.corps.is_some());
    if let Some(c) = &s.corps {
        pusher_u32(dst, c.len() as u32);
        for i in c { ecrire_instr3(dst, i); }
    }
    pusher_chaine(dst, &s.dll);
    pusher_chaine(dst, &s.dll_son);
    pusher_chaine(dst, &s.symbole);
    pusher_u32(dst, s.ligne as u32);
}

fn lire_signature3(l: &mut Lecteur<'_>) -> Result<Signature, Erreur> {
    let nom = l.chaine()?;
    let n = l.u32()? as usize;
    if n > l.reste() { return Err(Erreur::SnapshotInvalide("trop de paramètres")); }
    let mut params = Vec::with_capacity(n);
    for _ in 0..n { params.push((l.chaine()?, lire_ty(l)?)); }
    let retour = lire_ty(l)?;
    let a_retour = lire_bool(l)?;
    let present = lire_bool(l)?;
    let corps = if present { Some(lire_instrs3(l, 0)?) } else { None };
    let dll = l.chaine()?;
    let dll_son = l.chaine()?;
    let symbole = l.chaine()?;
    let ligne = l.u32()? as usize;
    Ok(Signature {
        nom, params, retour, a_retour, corps,
        // ⚠ Le cache RIR n'est PAS sérialisé : reconstruit par l'interpréteur.
        rir: None, dll, dll_son, symbole, ligne,
    })
}

/// Écrit un snapshot AST complet (`ROSEBC` v3) : TOUTES les fonctions et le
/// programme principal. Aucune opération n'est refusée — contrairement aux
/// formats v1/v2, `rose bc` ne peut plus produire un fichier inutilisable.
pub fn ecrire_snapshot(prog: &Programme) -> Vec<u8> {
    let mut out = Vec::with_capacity(256 + prog.instructions.len() * 16);
    out.extend_from_slice(MAGIC);
    pusher_u32(&mut out, VERSION_AST);

    pusher_u32(&mut out, prog.imports.len() as u32);
    for i in &prog.imports { pusher_chaine(&mut out, i); }

    // `ordre` fait autorité : il donne l'ordre de déclaration des signatures.
    // Les signatures absentes de `ordre` (ne devrait pas arriver) sont ajoutées
    // en fin pour ne rien perdre.
    // Cas normal : `ordre` et `signatures` décrivent exactement les mêmes
    // fonctions → une seule copie, ZÉRO allocation de table de hachage
    // (un HashSet coûte ~100 ns sur un programme d'une fonction).
    let noms = if prog.ordre.len() == prog.signatures.len() {
        prog.ordre.clone()
    } else {
        let mut presents: HashSet<&str> = HashSet::with_capacity(prog.ordre.len() + prog.signatures.len());
        let mut noms: Vec<String> = Vec::with_capacity(prog.ordre.len() + prog.signatures.len());
        for nom in &prog.ordre {
            if presents.insert(nom.as_str()) { noms.push(nom.clone()); }
        }
        for nom in prog.signatures.keys() {
            if presents.insert(nom.as_str()) { noms.push(nom.clone()); }
        }
        noms
    };
    pusher_u32(&mut out, noms.len() as u32);
    for nom in &noms {
        if let Some(s) = prog.signatures.get(nom) { ecrire_signature3(&mut out, s); }
    }

    pusher_u32(&mut out, prog.instructions.len() as u32);
    for i in &prog.instructions { ecrire_instr3(&mut out, i); }
    out
}

/// Lit et **vérifie** un snapshot AST v3. Tous les contrôles sont propres
/// (aucune panique sur fichier hostile) : magic, version, troncature,
/// compteurs, UTF-8, tags, booléens et profondeurs d'imbrication.
pub fn lire_snapshot(octets: &[u8]) -> Result<Programme, Erreur> {
    let mut l = Lecteur { octets, pos: 0 };
    if l.prendre(6)? != MAGIC { return Err(Erreur::MagicInvalide); }
    let version = l.u32()?;
    if version != VERSION_AST { return Err(Erreur::VersionInsupportee(version)); }

    let n_imp = l.u32()? as usize;
    if n_imp > l.reste() { return Err(Erreur::SnapshotInvalide("trop d'imports")); }
    let mut imports = Vec::with_capacity(n_imp);
    for _ in 0..n_imp { imports.push(l.chaine()?); }

    let n_sig = l.u32()? as usize;
    if n_sig > l.reste() { return Err(Erreur::SnapshotInvalide("trop de signatures")); }
    let mut ordre = Vec::with_capacity(n_sig);
    let mut signatures = HashMap::with_capacity(n_sig);
    for _ in 0..n_sig {
        let s = lire_signature3(&mut l)?;
        ordre.push(s.nom.clone());
        signatures.insert(s.nom.clone(), s);
    }

    let n_ins = l.u32()? as usize;
    if n_ins > l.reste() { return Err(Erreur::SnapshotInvalide("trop d'instructions")); }
    let mut instructions = Vec::with_capacity(n_ins);
    for _ in 0..n_ins { instructions.push(lire_instr3(&mut l, 0)?); }

    if l.pos != octets.len() { return Err(Erreur::OctetsEnTrop(octets.len() - l.pos)); }
    Ok(Programme { signatures, ordre, instructions, imports })
}

/// Exécute un snapshot v3 dans la RVM : reconstruit le programme puis le fait
/// tourner par la mécanique Rose (slots + pile d'appels de l'interpréteur).
/// Rend TOUTE la sortie du programme (exactement ce que `rose run` écrirait).
///
/// ⚠ Aucun fichier, aucun réseau : les seules E/S sont celles du langage
/// (Afficher / Lire), capturées dans le tampon de sortie de `run`.
pub fn executer_snapshot(octets: &[u8], arguments: &[String]) -> Result<String, Erreur> {
    let mut prog = lire_snapshot(octets)?;

    // ⚠⚠ MIROIR DU PIPELINE DE `executer_source_analyse` (interp.rs).
    // Sans cette étape, le corps principal serait interprété en AST pur et
    // afficherait les formules LaTeX des boucles `pour`, alors que `rose run`
    // les exécute en RIR (sans formule). On abaisse donc les fonctions ET le
    // principal exactement comme l'interpréteur le fait, pour une parité
    // OCTET POUR OCTET avec `rose run`. L'AST source reste la vérité du
    // snapshot : le RIR n'est qu'un cache reconstruit ici, jamais sérialisé.
    //
    // ⚠ EXCEPTION : le RIR ne sait pas résoudre les constantes irrationnelles
    // `pi`/`phi`/`e` en variable (mesuré : `>> pi` échoue en RIR alors que
    // l'AST pur les connaît). Un programme qui les utilise tourne donc en AST
    // pur, plus complet — c'est la règle « exécuter TOUT ce que Rose sait
    // faire » qui prime sur l'égalité de formatage des formules.
    fn expr_irrationnel(e: &Expr) -> bool {
        match e {
            Expr::Var(n, _) => matches!(n.as_str(), "pi" | "phi" | "e"),
            Expr::Binaire { a, b, .. } | Expr::Index { a, b, .. } =>
                expr_irrationnel(a) || expr_irrationnel(b),
            Expr::Unaire { a, .. } | Expr::Factorielle(a, _) | Expr::MultiFactorielle(a, _, _)
            | Expr::SousFactorielle(a, _) | Expr::Primorielle(a, _) | Expr::Terminal(a, _)
            | Expr::SuperFactorielle(a, _) => expr_irrationnel(a),
            Expr::Appel { args, .. } | Expr::ListeLit(args, _) => args.iter().any(expr_irrationnel),
            Expr::TableLit(v, _) => v.iter().any(|(_, x)| expr_irrationnel(x)),
            _ => false,
        }
    }
    fn instrs_irrationnel(b: &[Instr]) -> bool {
        b.iter().any(|i| match i {
            Instr::Afficher(es, _) => es.iter().any(expr_irrationnel),
            Instr::Affect { e, .. } | Instr::ExprStmt(e) | Instr::Lever(e, _) => expr_irrationnel(e),
            Instr::AffectIndex { index, valeur, .. } =>
                expr_irrationnel(index) || expr_irrationnel(valeur),
            Instr::Si { test, alors, sinon, .. } => expr_irrationnel(test)
                || instrs_irrationnel(alors)
                || sinon.as_ref().map_or(false, |s| instrs_irrationnel(s)),
            Instr::TantQue { test, corps, .. } => expr_irrationnel(test) || instrs_irrationnel(corps),
            Instr::Pour { debut, fin, corps, .. } => expr_irrationnel(debut)
                || expr_irrationnel(fin) || instrs_irrationnel(corps),
            Instr::Retour(Some(e), _) => expr_irrationnel(e),
            Instr::Bloc(b) | Instr::Spawn(b, _) => instrs_irrationnel(b),
            Instr::Essayer { corps, attraper, finalment, .. } => instrs_irrationnel(corps)
                || attraper.as_ref().map_or(false, |(_, x)| instrs_irrationnel(x))
                || finalment.as_ref().map_or(false, |x| instrs_irrationnel(x)),
            _ => false,
        })
    }
    let utilise_irrationnel = instrs_irrationnel(&prog.instructions)
        || prog.ordre.iter().any(|n| prog.signatures.get(n)
            .and_then(|s| s.corps.as_deref()).map_or(false, instrs_irrationnel));

    // OPT_VM — le JIT doit vivre jusqu'à la fin de l'exécution : il possède
    // les pages de code exécutable appelées par le principal compilé.
    let mut _jit_garde: Option<crate::jit::Jit> = None;
    let mut jit_principal: Option<crate::jit::Compile> = None;
    if !utilise_irrationnel {
        let candidats: Vec<String> = prog.ordre.iter()
            .filter(|n| prog.signatures.get(*n).map_or(false, |s| s.corps.is_some()))
            .cloned()
            .collect();
        crate::rir::declarer_rir(&candidats);
        for nom in &prog.ordre {
            if let Some(sig) = prog.signatures.get(nom) {
                let Some(corps) = sig.corps.as_ref() else { continue };
                let params: Vec<String> = sig.params.iter().map(|(n, _)| n.clone()).collect();
                if let Some(rir) = crate::rir::abaisser(corps, &params, &[]) {
                    prog.signatures.get_mut(nom).unwrap().rir = Some(rir);
                }
            }
        }
        // Une boucle euclidienne reconnue reste en AST (spécialisation native),
        // comme dans l'interpréteur. ⚠ Miroir de `interp::reconnait_euclide`
        // (privé) : on reproduit exactement son motif pour les mêmes valeurs.
        fn est_euclide(test: &Expr, corps: &[Instr]) -> bool {
            let cv = match test {
                Expr::Binaire { op, a, b, .. } if op == "!=" => match (&**a, &**b) {
                    (Expr::Var(v, _), Expr::Litteral { texte, flott: false, rationnel: false, neg: false, .. })
                        if texte == "0" => v.clone(),
                    (Expr::Litteral { texte, flott: false, rationnel: false, neg: false, .. }, Expr::Var(v, _))
                        if texte == "0" => v.clone(),
                    _ => return false,
                },
                _ => return false,
            };
            if corps.len() != 3 { return false; }
            matches!(
                (&corps[0], &corps[1], &corps[2]),
                (
                    Instr::Affect { nom: r, e: Expr::Binaire { op, a, b, .. }, .. },
                    Instr::Affect { nom: x, e: Expr::Var(vy, ..), .. },
                    Instr::Affect { nom: y, e: Expr::Var(vr, ..), .. },
                ) if op == "%"
                    && matches!(&**a, Expr::Var(v, ..) if v == x)
                    && matches!(&**b, Expr::Var(v, ..) if v == vy)
                    && vr == r && vy == &cv && y == &cv
            )
        }
        let pas_euclide = !prog.instructions.iter().any(|i| match i {
            Instr::TantQue { test, corps, .. } => est_euclide(test, corps),
            _ => false,
        });
        let rir_principal = if pas_euclide {
            crate::rir::abaisser_principal(&prog.instructions)
        } else { None };

        // OPT_VM — compiler coûte ~0,5 ms ; sur un programme sans récursion
        // (boucle courte, table) ce coût DÉPASSE l'exécution interprétée.
        // On ne déclenche donc le JIT que si une fonction s'appelle elle-même
        // (cas typique : fib, factorielle récursive), où le gain est massif.
        fn expr_recursive(nom: &str, e: &Expr) -> bool {
            match e {
                Expr::Appel { nom: n, args, .. } =>
                    n == nom || args.iter().any(|a| expr_recursive(nom, a)),
                Expr::Binaire { a, b, .. } | Expr::Index { a, b, .. } =>
                    expr_recursive(nom, a) || expr_recursive(nom, b),
                Expr::Unaire { a, .. } | Expr::Factorielle(a, _) | Expr::MultiFactorielle(a, _, _)
                | Expr::SousFactorielle(a, _) | Expr::Primorielle(a, _) | Expr::Terminal(a, _)
                | Expr::SuperFactorielle(a, _) => expr_recursive(nom, a),
                Expr::ListeLit(v, _) => v.iter().any(|x| expr_recursive(nom, x)),
                Expr::TableLit(v, _) => v.iter().any(|(_, x)| expr_recursive(nom, x)),
                _ => false,
            }
        }
        fn instrs_recursives(nom: &str, b: &[Instr]) -> bool {
            b.iter().any(|i| match i {
                Instr::Afficher(es, _) => es.iter().any(|e| expr_recursive(nom, e)),
                Instr::Affect { e, .. } | Instr::ExprStmt(e) | Instr::Lever(e, _)
                | Instr::Retour(Some(e), _) => expr_recursive(nom, e),
                Instr::AffectIndex { index, valeur, .. } =>
                    expr_recursive(nom, index) || expr_recursive(nom, valeur),
                Instr::Si { test, alors, sinon, .. } => expr_recursive(nom, test)
                    || instrs_recursives(nom, alors)
                    || sinon.as_ref().map_or(false, |s| instrs_recursives(nom, s)),
                Instr::TantQue { test, corps, .. } =>
                    expr_recursive(nom, test) || instrs_recursives(nom, corps),
                Instr::Pour { debut, fin, corps, .. } => expr_recursive(nom, debut)
                    || expr_recursive(nom, fin) || instrs_recursives(nom, corps),
                Instr::Bloc(b) | Instr::Spawn(b, _) => instrs_recursives(nom, b),
                Instr::Essayer { corps, attraper, finalment, .. } => instrs_recursives(nom, corps)
                    || attraper.as_ref().map_or(false, |(_, x)| instrs_recursives(nom, x))
                    || finalment.as_ref().map_or(false, |x| instrs_recursives(nom, x)),
                _ => false,
            })
        }
        let recursif = prog.ordre.iter().any(|nom| prog.signatures.get(nom)
            .and_then(|s| s.corps.as_deref())
            .map_or(false, |c| instrs_recursives(nom, c)));

        // OPT_VM — la RVM exécutait TOUT en interpréteur (fib(28) : 123 ms)
        // alors que `rose run` compile le principal ET les fonctions avec
        // Cranelift. On applique le MÊME pipeline ici. ⚠ On ne prend la voie
        // JIT que si TOUTES les fonctions ont été réellement compilées :
        // sinon le code compilé déléguerait à un interpréteur sans table JIT
        // installée (changements de sémantique observés sur les appels RIR).
        if recursif {
            let prog_pour_jit = prog.clone();
            let (a_du_travail, _natifs) = crate::jit::prepare_jit(
                &prog_pour_jit, &prog.signatures, rir_principal.as_ref());
            if a_du_travail {
                let mut jit = crate::jit::Jit::nouveau(&prog.signatures);
                let principal = jit.compiler_principal(&prog_pour_jit);
                let fonctions = jit.compiler_tout();
                let tout_natif = fonctions.values().all(|c| matches!(
                    c, crate::jit::Compile::Natif(_) | crate::jit::Compile::Principal(_)));
                if tout_natif {
                    jit_principal = principal;
                    _jit_garde = Some(jit);
                }
            }
        }
        // Sans principal JIT, on garde le corps RIR linéaire (bien plus rapide
        // que l'AST), exactement comme avant optimisation.
        if jit_principal.is_none() {
            if let Some(rir) = rir_principal {
                prog.instructions = vec![Instr::Rir(rir)];
            }
        }
    }

    crate::run::set_sink(true);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut interp = crate::interp::Interp::nouveau(prog)
            .avec_arguments(arguments.to_vec());
        if let Some(crate::jit::Compile::Principal(f)) = jit_principal {
            // Le code compilé peut déléguer à l'interpréteur via
            // `rose_appel_interpreteur` : l'interpréteur doit rester vivant
            // (et publié) pendant TOUT l'appel au principal.
            crate::interp::poser_interp_courant(&mut interp);
            f();
            crate::interp::poser_interp_courant(std::ptr::null_mut());
        } else {
            interp.executer_principal();
        }
    }));
    // Même en cas de panique, ne jamais laisser un pointeur pendant vers un
    // interpréteur détruit (les helpers JIT le liraient).
    crate::interp::poser_interp_courant(std::ptr::null_mut());
    let sortie = crate::run::take_sink().unwrap_or_default();
    match result {
        Ok(()) => Ok(sortie),
        Err(payload) => {
            // `erreur_runtime` panique avec `ROSE_ERR:<message>` : on rend le
            // VRAI message Rose au lieu d'un « erreur interne » opaque.
            let brut = if let Some(s) = payload.downcast_ref::<String>() { s.clone() }
                       else if let Some(s) = payload.downcast_ref::<&str>() { (*s).to_string() }
                       else { String::new() };
            let message = match brut.strip_prefix("ROSE_ERR:") {
                Some(m) if !m.is_empty() => format!("Rose : erreur — {}", m),
                _ => "Rose : erreur — erreur interne d'exécution".to_string(),
            };
            Err(Erreur::ExecutionRose(message))
        }
    }
}

/// Discriminateur CLI : version d'un `.rosebc`, sans rien exécuter ni décoder.
pub fn version_fichier(octets: &[u8]) -> Result<u32, Erreur> {
    if octets.len() < 10 { return Err(Erreur::Tronque { attendu: 10, present: octets.len() }); }
    if &octets[..6] != MAGIC { return Err(Erreur::MagicInvalide); }
    Ok(u32::from_le_bytes([octets[6], octets[7], octets[8], octets[9]]))
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::front::analyser;
    use crate::rir::abaisser;
    use crate::run::{set_sink, take_sink};

    // Le tampon de sortie (`run::SINK`) est GLOBAL : plusieurs tests qui
    // l'activent en parallèle se le volent mutuellement (échec intermittent
    // `sortie inattendue : ""`). Ce verrou rend les tests qui capturent la
    // sortie séquentiels, sans toucher au moteur.
    static TEST_SINK_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    fn sink_guard() -> std::sync::MutexGuard<'static, ()> {
        match TEST_SINK_LOCK.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        }
    }

    const SOURCE_CARRE: &str = "fn carre(n: entier) -> entier {\n  retour n * n\n}\n";

    fn corps_carre() -> CorpsRir {
        let prog = analyser(SOURCE_CARRE).expect("analyse carre");
        let sig = prog.signatures.get("carre").expect("signature carre");
        let params: Vec<String> = sig.params.iter().map(|(n, _)| n.clone()).collect();
        abaisser(sig.corps.as_ref().expect("corps carre"), &params, &[]).expect("abaissable")
    }

    fn cle(v: &Valeur) -> String { crate::run::chaine_de(v) }

    fn en_tete(n_slots: u32, n_ops: u32, n_globaux: u32) -> Vec<u8> {
        let mut v = Vec::from(MAGIC.as_slice());
        pusher_u32(&mut v, VERSION);
        pusher_u32(&mut v, n_slots);
        pusher_u32(&mut v, n_ops);
        pusher_u32(&mut v, n_globaux);
        v
    }

    #[test]
    fn round_trip_carre_chargement_execution_et_parite() {
        let _sink = sink_guard();
        let corps = corps_carre();
        verifier_sous_ensemble(&corps).expect("carre dans le sous-ensemble");

        let octets = ecrire_bytecode(&corps);
        println!("[rosevm] taille .rosebc pour carre = {} octets", octets.len());
        assert!(octets.starts_with(MAGIC));
        assert_eq!(octets.len(), 36,
            "format v1 attendu : 22 o d'en-tête + 5 o de nom (u32 len + 'n') + 9 o d'op");

        let recharge = lire_bytecode(&octets).expect("rechargement carre");
        assert_eq!(recharge.n_slots, corps.n_slots);
        assert_eq!(recharge.noms, corps.noms);
        assert_eq!(recharge.globaux, corps.globaux);
        assert_eq!(recharge.ops.len(), corps.ops.len());
        // ⚠ Le `CacheSlot(Cell)` doit être reconstruit à vide au chargement.
        for (a, b) in corps.ops.iter().zip(recharge.ops.iter()) {
            if let (Op::RetourAbstrait(Eabstrait::Binaire { a: x, b: y, .. }),
                    Op::RetourAbstrait(Eabstrait::Binaire { a: u, b: v, .. })) = (a, b)
            {
                if let (Eabstrait::Slot(sx, cx), Eabstrait::Slot(su, cu)) = (&**x, &**u) {
                    assert_eq!(sx, su);
                    assert_eq!(cx.lire(), (0, 0));
                    assert_eq!(cu.lire(), (0, 0));
                }
                let _ = (y, v);
            }
        }

        // Exécution des DEUX corps par la VM RIR existante, même hôte minimal.
        let arg = Valeur::z(7);
        let mut h_orig = HoteBc::neuf(8);
        let r_orig = executer_avec_vm(&corps, &[arg.clone()], &mut h_orig);
        let mut h_rech = HoteBc::neuf(8);
        let r_rech = executer_avec_vm(&recharge, &[arg], &mut h_rech);
        let v_orig = match r_orig { FinRir::Retour(v) => v, _ => panic!("retour attendu") };
        let v_rech = match r_rech { FinRir::Retour(v) => v, _ => panic!("retour attendu") };
        println!("[rosevm] RIR source  : carre(7) = {}", cle(&v_orig));
        println!("[rosevm] RIR .rosebc : carre(7) = {}", cle(&v_rech));
        assert_eq!(cle(&v_orig), "49");
        assert_eq!(cle(&v_rech), cle(&v_orig));

        // Parité avec l'interpréteur COMPLET (source → analyse → exécution).
        let source = format!("{}>> carre(7)\n", SOURCE_CARRE);
        set_sink(true);
        let rc = crate::interp::executer_source(&source, Vec::new());
        let sortie = take_sink().unwrap_or_default();
        println!("[rosevm] interpréteur : rc={} sortie={:?}", rc, sortie);
        assert_eq!(rc, 0);
        assert!(sortie.contains("49"), "sortie inattendue : {:?}", sortie);
        assert_eq!(sortie.trim(), cle(&v_rech));
    }

    #[test]
    fn execution_sandbox_capture_sans_es() {
        let corps = lire_bytecode(&ecrire_bytecode(&corps_carre())).unwrap();
        let mut hote = HoteBc::neuf(8);
        let limites = Limites { budget_instructions: 100, max_slots: 16, max_gz: 8 };
        let v = executer_sandbox(&corps, &[Valeur::z(9)], &mut hote, &limites)
            .expect("sandbox carre");
        assert_eq!(cle(&v), "81");
        println!("[rosevm] sandbox carre(9) = {}", cle(&v));
        assert_eq!(hote.sortie, "");
        assert_eq!(hote.gc.prets(), 0);
    }

    #[test]
    fn refus_magic_et_version() {
        let mut octets = ecrire_bytecode(&corps_carre());
        octets[0] = b'X';
        assert!(matches!(lire_bytecode(&octets), Err(Erreur::MagicInvalide)));

        let mut octets = ecrire_bytecode(&corps_carre());
        octets[6..10].copy_from_slice(&99u32.to_le_bytes());
        assert!(matches!(lire_bytecode(&octets), Err(Erreur::VersionInsupportee(99))));
    }

    #[test]
    fn refus_saut_hors_bornes() {
        // `Saut { cible }` avec cible >= n_ops : le vérificateur doit refuser.
        let mut bon = en_tete(0, 1, 0);
        bon.push(OP_SAUT);
        bon.extend_from_slice(&0u32.to_le_bytes());
        assert!(lire_bytecode(&bon).is_ok(), "saut vers 0 dans un corps de 1 op est valide");

        let mut mauvais = en_tete(0, 1, 0);
        mauvais.push(OP_SAUT);
        mauvais.extend_from_slice(&999u32.to_le_bytes());
        match lire_bytecode(&mauvais) {
            Err(Erreur::SautHorsBornes { cible, n_ops }) => {
                assert_eq!(cible, 999); assert_eq!(n_ops, 1);
                println!("[rosevm] saut hors bornes refusé : cible={} n_ops={}", cible, n_ops);
            }
            autre => panic!("refus attendu, obtenu {:?}", autre.map(|_| ())),
        }
    }

    #[test]
    fn refus_troncature_octets_en_trop_et_slot_hors_bornes() {
        let bon = ecrire_bytecode(&corps_carre());
        assert!(matches!(lire_bytecode(&bon[..bon.len() - 1]), Err(Erreur::Tronque { .. })));
        let mut trop = bon.clone(); trop.push(0);
        assert!(matches!(lire_bytecode(&trop), Err(Erreur::OctetsEnTrop(1))));

        // slot 3 dans un corps de 1 slot → refus.
        let mut mauvais = en_tete(1, 1, 0);
        pusher_chaine(&mut mauvais, "n");
        mauvais.push(OP_AFFECT);
        pusher_u16(&mut mauvais, 3);
        mauvais.push(EX_SLOT);
        pusher_u16(&mut mauvais, 0);
        assert!(matches!(lire_bytecode(&mauvais), Err(Erreur::SlotHorsBornes { slot: 3, .. })));
    }

    #[test]
    fn sandbox_refuse_ast_et_boucle_infinie() {
        let prog = analyser("fn f() -> entier {\n  $x = longueur(\"abc\")\n  retour $x\n}\n").unwrap();
        let sig = prog.signatures.get("f").unwrap();
        let corps = abaisser(sig.corps.as_ref().unwrap(), &[], &[]).unwrap();
        assert!(corps.ops.iter().any(|o| matches!(o, Op::AffectAst { .. })));
        let octets = ecrire_bytecode(&corps);
        assert!(octets.contains(&OP_REFUSE));
        // Le vérificateur refuse l'AST marquée 0x80 au chargement.
        assert!(matches!(lire_bytecode(&octets), Err(Erreur::OperationRefusee("AST"))));

        // Boucle infinie `Saut 0` : le budget doit être consommé, pas le CPU.
        let mut octets = en_tete(0, 1, 0);
        octets.push(OP_SAUT);
        octets.extend_from_slice(&0u32.to_le_bytes());
        let corps = lire_bytecode(&octets).unwrap();
        let mut hote = HoteBc::neuf(4);
        let limites = Limites { budget_instructions: 10, max_slots: 4, max_gz: 4 };
        match executer_sandbox(&corps, &[], &mut hote, &limites) {
            Err(Erreur::BudgetDepasse(10)) => println!("[rosevm] budget épuisé proprement"),
            autre => panic!("budget attendu, obtenu {:?}", autre.map(|_| ())),
        }
    }

    #[test]
    fn gc_pool_limite_et_recyclage() {
        let mut gc = GcPool::neuf(2);
        let a = gc.prendre().unwrap();
        let b = gc.prendre().unwrap();
        assert!(matches!(gc.prendre(), Err(Erreur::GzEpuises(2))));
        assert_eq!(gc.prets(), 2);
        gc.rendre(a);
        gc.rendre(b);
        assert_eq!(gc.prets(), 0);
        assert_eq!(gc.libres(), 2);
        let c = gc.prendre().unwrap(); // réutilise une boîte : pas de nouveau Gz::neuf
        assert_eq!(gc.total(), 2);
        gc.echapper(); // la boîte part dans une Valeur (Rc)
        drop(c);
        assert_eq!(gc.echappes(), 1);
        assert_eq!(gc.prets(), 0);
        println!("[rosevm] GC pool : total={} libres={} echappes={}",
                 gc.total(), gc.libres(), gc.echappes());
    }

    #[test]
    fn sandbox_limite_slots() {
        let corps = lire_bytecode(&ecrire_bytecode(&corps_carre())).unwrap();
        let mut hote = HoteBc::neuf(4);
        let limites = Limites { budget_instructions: 100, max_slots: 0, max_gz: 4 };
        assert!(matches!(executer_sandbox(&corps, &[], &mut hote, &limites),
                         Err(Erreur::MemoireDepassee(1))));
    }

    // ══════════════════════════════════════════════════════════════════════
    // JVM Rose — tests des briques v2
    // ══════════════════════════════════════════════════════════════════════

    fn sym(cp: &mut ConstantPool, s: &str) -> u16 {
        cp.ajouter(Const::Symbole(s.to_string())).unwrap()
    }
    fn chn(cp: &mut ConstantPool, s: &str) -> u16 {
        cp.ajouter(Const::Chaine(s.to_string())).unwrap()
    }
    fn ent(cp: &mut ConstantPool, v: i64) -> u16 {
        cp.ajouter(Const::Entier(v)).unwrap()
    }
    fn mref(cp: &mut ConstantPool, c: u16, n: u16, d: u16) -> u16 {
        cp.ajouter(Const::Methode { classe: c, nom: n, descr: d }).unwrap()
    }
    fn charge(c: ClasseRose) -> ChargeurClasses {
        let mut ch = ChargeurClasses::neuf();
        ch.ajouter(c).unwrap();
        ch
    }

    #[test]
    fn cp_tous_types_et_roundtrip_binaire() {
        let mut cp = ConstantPool::neuf();
        let cs = sym(&mut cp, "C");
        let ns = sym(&mut cp, "m");
        let ds = chn(&mut cp, "(I)I");
        let i = ent(&mut cp, 42);
        let big = cp.ajouter(Const::BigInt("123456789012345678901234567890".to_string())).unwrap();
        let fl = cp.ajouter(Const::Flottant(1.5)).unwrap();
        let ch = chn(&mut cp, "salut");
        let sy = sym(&mut cp, "Erreur");
        let mr = mref(&mut cp, cs, ns, ds);
        assert_eq!(cp.len(), 9);
        assert_eq!(cp.get(i).unwrap(), &Const::Entier(42));
        assert_eq!(cp.chaine(ch).unwrap(), "salut");
        assert_eq!(cp.symbole(sy).unwrap(), "Erreur");
        assert_eq!(cp.methode(mr).unwrap(), (cs, ns, ds));
        assert_eq!(cp.get(big).unwrap().ty_abstrait(), TyDesc::Entier);
        assert_eq!(cp.get(fl).unwrap().ty_abstrait(), TyDesc::Flottant);
        // Le bigint reste du TEXTE exact, jamais un i64 tronqué.
        assert_eq!(cp.get(big).unwrap(),
                   &Const::BigInt("123456789012345678901234567890".to_string()));

        let mut oct = Vec::new();
        ecrire_cp(&mut oct, &cp);
        let mut l = Lecteur { octets: &oct, pos: 0 };
        let cp2 = lire_cp(&mut l).unwrap();
        assert_eq!(l.pos, oct.len());
        assert_eq!(cp2.version, cp.version);
        assert_eq!(cp2.entrees(), cp.entrees());
        println!("[rvm] CP 6 tags sérialisés : {} entrées, {} octets", cp.len(), oct.len());
    }

    #[test]
    fn cp_index_hors_bornes_et_type_inattendu() {
        let mut cp = ConstantPool::neuf();
        ent(&mut cp, 1);
        assert!(matches!(cp.get(7), Err(Erreur::IndexConstantHorsBornes { index: 7, n: 1 })));
        assert!(matches!(cp.symbole(0),
            Err(Erreur::TypeConstantInattendu { index: 0, attendu: "symbole", .. })));
        assert!(matches!(cp.methode(0),
            Err(Erreur::TypeConstantInattendu { attendu: "méthode", .. })));
    }

    #[test]
    fn promotion_v1_vers_classe_et_execution() {
        let corps = corps_carre();
        let c = promouvoir_corps(&corps, "carre").unwrap();
        assert_eq!(c.nom, "Principal");
        assert_eq!(c.methodes.len(), 1);
        let m = &c.methodes[0];
        assert_eq!(m.n_slots, 1);
        assert_eq!(m.code.len(), 4);
        assert_eq!(m.code[0], InstrBc::ChargerSlot { slot: 0 });
        assert_eq!(m.code[1], InstrBc::ChargerSlot { slot: 0 });
        assert_eq!(m.code[2], InstrBc::Binaire { op: Oper::Mul });
        assert_eq!(m.code[3], InstrBc::RetourValeur);

        let ch = charge(c);
        let mut hote = HoteBc::neuf(8);
        let mut vm = MachineJvm::neuf(&ch, &mut hote, LimitesJvm::default());
        let v = vm.invoquer("Principal", "carre", &[Valeur::z(7)]).unwrap();
        assert_eq!(cle(&v), "49");
        println!("[rvm] CorpsRir promu -> carre(7) = {}", cle(&v));
    }

    #[test]
    fn round_trip_classe_v2_et_parite_interpreteur() {
        let _sink = sink_guard();
        let c = promouvoir_corps(&corps_carre(), "carre").unwrap();
        let octets = ecrire_classe(&c);
        assert!(octets.starts_with(MAGIC));
        assert!(octets[6..10] == VERSION_JVM.to_le_bytes());
        let c2 = lire_classe(&octets).unwrap();
        assert_eq!(c2.nom, c.nom);
        assert_eq!(c2.cp.entrees(), c.cp.entrees());
        assert_eq!(c2.methodes[0].code, c.methodes[0].code);

        let mut hote = HoteBc::neuf(8);
        let v = executer_fichier(&octets, "Principal", "carre", &[Valeur::z(7)],
                                 &mut hote, &LimitesJvm::default()).unwrap();
        assert_eq!(cle(&v), "49");
        // Parité avec l'interpréteur COMPLET sur la même source Rose.
        let source = format!("{}>> carre(7)\n", SOURCE_CARRE);
        set_sink(true);
        let rc = crate::interp::executer_source(&source, Vec::new());
        let sortie = take_sink().unwrap_or_default();
        assert_eq!(rc, 0);
        assert_eq!(sortie.trim(), "49");
        assert_eq!(cle(&v), sortie.trim());
        println!("[rvm] .rosebc v2 carre(7)={} == interpréteur {}", cle(&v), sortie.trim());
    }

    #[test]
    fn lecture_v1_promue_en_classe_executable() {
        let octets = ecrire_bytecode(&corps_carre()); // v1 historique
        let c = lire_classe(&octets).unwrap();
        assert_eq!(c.nom, "Principal");
        assert_eq!(c.methodes[0].nom, "<principal>");
        let ch = charge(c);
        let mut hote = HoteBc::neuf(8);
        let mut vm = MachineJvm::neuf(&ch, &mut hote, LimitesJvm::default());
        let v = vm.invoquer("Principal", "<principal>", &[Valeur::z(6)]).unwrap();
        assert_eq!(cle(&v), "36");
        println!("[rvm] rétrocompat v1 -> <principal>(6) = {}", cle(&v));
    }

    #[test]
    fn invocation_carre_depuis_somme_compteurs() {
        let mut c = ClasseRose::neuf("Maths");
        let cs = sym(&mut c.cp, "Maths");
        let cn = sym(&mut c.cp, "carre"); let cd = chn(&mut c.cp, "(I)I");
        let r_carre = mref(&mut c.cp, cs, cn, cd);
        let sn = sym(&mut c.cp, "somme"); let sd = chn(&mut c.cp, "(II)I");
        let r_somme = mref(&mut c.cp, cs, sn, sd);

        c.ajouter_methode(MethodeRose::neuf(
            "carre", Descripteur::neuf(vec![TyDesc::Entier], TyDesc::Entier), &["n"],
            vec![InstrBc::ChargerSlot { slot: 0 }, InstrBc::Dupliquer,
                 InstrBc::Binaire { op: Oper::Mul }, InstrBc::RetourValeur],
        ).unwrap()).unwrap();
        c.ajouter_methode(MethodeRose::neuf(
            "somme", Descripteur::neuf(
                vec![TyDesc::Entier, TyDesc::Entier], TyDesc::Entier), &["a", "b"],
            vec![InstrBc::ChargerSlot { slot: 0 }, InstrBc::Appel { methode: r_carre },
                 InstrBc::ChargerSlot { slot: 1 }, InstrBc::Appel { methode: r_carre },
                 InstrBc::Binaire { op: Oper::Add }, InstrBc::RetourValeur],
        ).unwrap()).unwrap();
        let _ = r_somme;

        let octets = ecrire_classe(&c);
        let mut hote = HoteBc::neuf(8);
        let ch = charger_fichier(&octets).unwrap();
        let mut vm = MachineJvm::neuf(&ch, &mut hote, LimitesJvm::default());
        let v = vm.invoquer("Maths", "somme", &[Valeur::z(3), Valeur::z(4)]).unwrap();
        assert_eq!(cle(&v), "25");
        assert_eq!(vm.appels, 2, "deux appels de `carre` attendus");
        assert!(vm.instructions > 0);
        println!("[rvm] somme(3,4) = {} (appels={}, instructions={})",
                 cle(&v), vm.appels, vm.instructions);
    }

    #[test]
    fn dispatch_heritage_simple() {
        let mut mere = ClasseRose::neuf("Mere");
        let cm = sym(&mut mere.cp, "Mere");
        let dm = sym(&mut mere.cp, "double");
        let des = chn(&mut mere.cp, "(I)I");
        let r = mref(&mut mere.cp, cm, dm, des);
        let index_deux = ent(&mut mere.cp, 2);
        mere.ajouter_methode(MethodeRose::neuf(
            "double", Descripteur::neuf(vec![TyDesc::Entier], TyDesc::Entier), &["n"],
            vec![InstrBc::ChargerSlot { slot: 0 },
                 InstrBc::PousserConst { index: index_deux },
                 InstrBc::Binaire { op: Oper::Mul }, InstrBc::RetourValeur],
        ).unwrap()).unwrap();
        let _ = r;

        let mut fille = ClasseRose::neuf("Fille");
        fille.super_classe = Some("Mere".to_string());
        let cf = sym(&mut fille.cp, "Fille");
        let df = sym(&mut fille.cp, "double");
        let des2 = chn(&mut fille.cp, "(I)I");
        let _ref_fille = mref(&mut fille.cp, cf, df, des2);

        let mut ch = ChargeurClasses::neuf();
        ch.ajouter(mere).unwrap();
        ch.ajouter(fille).unwrap();
        assert_eq!(ch.classes["Fille"].table_dispatch.get("double"), Some(&0));
        let mut hote = HoteBc::neuf(8);
        let mut vm = MachineJvm::neuf(&ch, &mut hote, LimitesJvm::default());
        let v = vm.invoquer("Fille", "double", &[Valeur::z(21)]).unwrap();
        assert_eq!(cle(&v), "42");
        println!("[rvm] dispatch hérité Fille.double(21) = {}", cle(&v));
    }

    #[test]
    fn exceptions_lever_et_attraper_dans_methode() {
        let mut c = ClasseRose::neuf("Essais");
        let type_e = sym(&mut c.cp, "E");
        let msg = chn(&mut c.cp, "boom");
        let mut m = MethodeRose::neuf(
            "essayer", Descripteur::neuf(vec![], TyDesc::Vide), &[],
            vec![InstrBc::PousserConst { index: msg },
                 InstrBc::Lever { type_index: type_e, msg_index: msg },
                 InstrBc::RetourVide,
                 InstrBc::Afficher,
                 InstrBc::RetourVide],
        ).unwrap();
        m.exceptions.push(EntreeException {
            debut: 0, fin: 2, cible: 3, type_index: type_e,
        });
        c.ajouter_methode(m).unwrap();

        let octets = ecrire_classe(&c);
        let mut hote = HoteBc::neuf(8);
        let v = executer_fichier(&octets, "Essais", "essayer", &[],
                                 &mut hote, &LimitesJvm::default()).unwrap();
        assert!(matches!(v, Valeur::Vide));
        assert_eq!(hote.sortie, "boom");
        println!("[rvm] try/catch : sortie capturée = {:?}", hote.sortie);
    }

    fn classe_propagation() -> Vec<u8> {
        let mut c = ClasseRose::neuf("Prop");
        let cp_cl = sym(&mut c.cp, "Prop");
        let type_e = sym(&mut c.cp, "E");
        let msg = chn(&mut c.cp, "propagé");
        let n_int = sym(&mut c.cp, "interne");
        let n_ext = sym(&mut c.cp, "externe");
        let descr = chn(&mut c.cp, "()V");
        let r_int = mref(&mut c.cp, cp_cl, n_int, descr);
        let r_ext = mref(&mut c.cp, cp_cl, n_ext, descr);

        c.ajouter_methode(MethodeRose::neuf(
            "interne", Descripteur::neuf(vec![], TyDesc::Vide), &[],
            vec![InstrBc::Lever { type_index: type_e, msg_index: msg },
                 InstrBc::RetourVide],
        ).unwrap()).unwrap();
        let mut ext = MethodeRose::neuf(
            "externe", Descripteur::neuf(vec![], TyDesc::Vide), &[],
            vec![InstrBc::Appel { methode: r_int },
                 InstrBc::RetourVide,
                 InstrBc::Afficher,
                 InstrBc::RetourVide],
        ).unwrap();
        ext.exceptions.push(EntreeException {
            debut: 0, fin: 1, cible: 2, type_index: 0, // attrape-tout
        });
        c.ajouter_methode(ext).unwrap();
        let _ = r_ext;
        ecrire_classe(&c)
    }

    #[test]
    fn exception_se_propage_au_cadre_appelant() {
        let octets = classe_propagation();
        let mut hote = HoteBc::neuf(8);
        executer_fichier(&octets, "Prop", "externe", &[],
                         &mut hote, &LimitesJvm::default()).unwrap();
        assert_eq!(hote.sortie, "propagé");
        println!("[rvm] déroulement inter-cadres : sortie = {:?}", hote.sortie);
    }

    #[test]
    fn exception_non_attrapee_erreur_propre() {
        let octets = classe_propagation();
        let mut hote = HoteBc::neuf(8);
        match executer_fichier(&octets, "Prop", "interne", &[],
                               &mut hote, &LimitesJvm::default()) {
            Err(Erreur::ExceptionNonAttrapee(t)) => {
                assert_eq!(t, "E");
                println!("[rvm] exception non attrapée : `{}`", t);
            }
            autre => panic!("ExceptionNonAttrapee attendue, obtenu {:?}", autre.map(|_| ())),
        }
    }

    fn classe_recursive(nom: &str, code: Vec<InstrBc>) -> Vec<u8> {
        let mut c = ClasseRose::neuf(nom);
        let cn = sym(&mut c.cp, nom);
        let cm = sym(&mut c.cp, "boucle");
        let cd = chn(&mut c.cp, "()V");
        let r = mref(&mut c.cp, cn, cm, cd);
        let mut corps = code;
        // Toute référence d'appel sera `r` (une seule méthode dans la classe).
        for i in &mut corps {
            if let InstrBc::Appel { methode } = i { *methode = r; }
        }
        c.ajouter_methode(MethodeRose::neuf(
            "boucle", Descripteur::neuf(vec![], TyDesc::Vide), &[], corps,
        ).unwrap()).unwrap();
        ecrire_classe(&c)
    }

    #[test]
    fn sandbox_profondeur_appels() {
        let octets = classe_recursive("RecP", vec![
            InstrBc::Appel { methode: 0 }, InstrBc::RetourVide,
        ]);
        let mut hote = HoteBc::neuf(8);
        let lim = LimitesJvm { profondeur_max: 4, ..LimitesJvm::default() };
        match executer_fichier(&octets, "RecP", "boucle", &[], &mut hote, &lim) {
            Err(Erreur::ProfondeurDepassee(4)) =>
                println!("[rvm] profondeur d'appel bornée à 4"),
            autre => panic!("ProfondeurDepassee attendue, obtenu {:?}", autre.map(|_| ())),
        }
    }

    #[test]
    fn sandbox_budget_appels() {
        let octets = classe_recursive("RecA", vec![
            InstrBc::Appel { methode: 0 }, InstrBc::RetourVide,
        ]);
        let mut hote = HoteBc::neuf(8);
        let lim = LimitesJvm { profondeur_max: 100, max_appels: 3, ..LimitesJvm::default() };
        match executer_fichier(&octets, "RecA", "boucle", &[], &mut hote, &lim) {
            Err(Erreur::AppelsEpuises(3)) => println!("[rvm] budget de 3 appels épuisé"),
            autre => panic!("AppelsEpuises attendu, obtenu {:?}", autre.map(|_| ())),
        }
    }

    #[test]
    fn sandbox_budget_instructions_boucle_infinie() {
        let mut c = ClasseRose::neuf("Tourne");
        let cn = sym(&mut c.cp, "Tourne");
        let cm = sym(&mut c.cp, "tourne");
        let cd = chn(&mut c.cp, "()V");
        let _r = mref(&mut c.cp, cn, cm, cd);
        c.ajouter_methode(MethodeRose::neuf(
            "tourne", Descripteur::neuf(vec![], TyDesc::Vide), &[],
            vec![InstrBc::Saut { cible: 0 }],
        ).unwrap()).unwrap();
        let octets = ecrire_classe(&c);
        let mut hote = HoteBc::neuf(8);
        let lim = LimitesJvm { budget_instructions: 25, ..LimitesJvm::default() };
        match executer_fichier(&octets, "Tourne", "tourne", &[], &mut hote, &lim) {
            Err(Erreur::BudgetDepasse(25)) => println!("[rvm] budget 25 instructions épuisé"),
            autre => panic!("BudgetDepasse attendu, obtenu {:?}", autre.map(|_| ())),
        }
    }

    #[test]
    fn verificateur_refuse_saut_et_slot_hors_bornes() {
        let mut c = ClasseRose::neuf("Mauvais");
        c.ajouter_methode(MethodeRose::neuf(
            "m", Descripteur::neuf(vec![], TyDesc::Vide), &[],
            vec![InstrBc::Saut { cible: 99 }],
        ).unwrap()).unwrap();
        assert!(matches!(verifier_classe(&c), Err(Erreur::SautHorsBornes { cible: 99, .. })));

        let mut c2 = ClasseRose::neuf("Mauvais2");
        c2.ajouter_methode(MethodeRose::neuf(
            "m", Descripteur::neuf(vec![], TyDesc::Vide), &["x"],
            vec![InstrBc::ChargerSlot { slot: 5 }, InstrBc::RetourVide],
        ).unwrap()).unwrap();
        assert!(matches!(verifier_classe(&c2), Err(Erreur::SlotHorsBornes { slot: 5, .. })));
        println!("[rvm] vérificateur : saut et slot hors bornes refusés");
    }

    #[test]
    fn verificateur_refuse_arite_d_appel() {
        let mut c = ClasseRose::neuf("Ver");
        let cv = sym(&mut c.cp, "Ver");
        let cible = sym(&mut c.cp, "cible");
        let des = chn(&mut c.cp, "(I)I");
        let r = mref(&mut c.cp, cv, cible, des);
        c.ajouter_methode(MethodeRose::neuf(
            "cible", Descripteur::neuf(vec![TyDesc::Entier], TyDesc::Entier), &["x"],
            vec![InstrBc::ChargerSlot { slot: 0 }, InstrBc::RetourValeur],
        ).unwrap()).unwrap();
        c.ajouter_methode(MethodeRose::neuf(
            "appele", Descripteur::neuf(vec![], TyDesc::Vide), &[],
            vec![InstrBc::Appel { methode: r }, InstrBc::RetourVide],
        ).unwrap()).unwrap();
        let mut ch = ChargeurClasses::neuf();
        // `ajouter` enchaîne vérification structure + flot + liens/arité.
        match ch.ajouter(c) {
            Err(Erreur::PileSousDimensionnee { .. }) =>
                println!("[rvm] vérificateur : arité d'appel incohérente refusée"),
            autre => panic!("PileSousDimensionnee attendue, obtenu {:?}", autre.map(|_| ())),
        }
    }

    #[test]
    fn programme_reel_fib_recursif_par_methodes() {
        let _sink = sink_guard();
        // fib compilé en méthode JVM Rose, avec Appel récursif.
        let mut c = ClasseRose::neuf("Fib");
        let cf = sym(&mut c.cp, "Fib");
        let nf = sym(&mut c.cp, "fib");
        let df = chn(&mut c.cp, "(I)I");
        let deux = ent(&mut c.cp, 2);
        let un = ent(&mut c.cp, 1);
        let r_fib = mref(&mut c.cp, cf, nf, df);

        let code = vec![
            InstrBc::ChargerSlot { slot: 0 },
            InstrBc::PousserConst { index: deux },
            InstrBc::Binaire { op: Oper::Lt },
            InstrBc::SautSiFaux { cible: 6 },
            InstrBc::ChargerSlot { slot: 0 },
            InstrBc::RetourValeur,
            InstrBc::ChargerSlot { slot: 0 },
            InstrBc::PousserConst { index: un },
            InstrBc::Binaire { op: Oper::Sub },
            InstrBc::Appel { methode: r_fib },
            InstrBc::StockerSlot { slot: 1 },
            InstrBc::ChargerSlot { slot: 0 },
            InstrBc::PousserConst { index: deux },
            InstrBc::Binaire { op: Oper::Sub },
            InstrBc::Appel { methode: r_fib },
            InstrBc::StockerSlot { slot: 2 },
            InstrBc::ChargerSlot { slot: 1 },
            InstrBc::ChargerSlot { slot: 2 },
            InstrBc::Binaire { op: Oper::Add },
            InstrBc::RetourValeur,
        ];
        c.ajouter_methode(MethodeRose::neuf(
            "fib", Descripteur::neuf(vec![TyDesc::Entier], TyDesc::Entier),
            &["n", "a", "b"], code,
        ).unwrap()).unwrap();
        let octets = ecrire_classe(&c);

        let mut hote = HoteBc::neuf(8);
        let ch = charger_fichier(&octets).unwrap();
        let mut vm = MachineJvm::neuf(&ch, &mut hote, LimitesJvm::default());
        let v = vm.invoquer("Fib", "fib", &[Valeur::z(25)]).unwrap();
        assert_eq!(cle(&v), "75025");
        println!("[rvm] fib(25) = {} (appels={}, instructions={})",
                 cle(&v), vm.appels, vm.instructions);

        // Parité avec l'interpréteur Rose sur la même récursion source.
        let source = "fn fib(n: entier) -> entier { si (n < 2) { retour n }; retour fib(n-1) + fib(n-2) }\n>> fib(25)\n";
        set_sink(true);
        let rc = crate::interp::executer_source(source, Vec::new());
        let sortie = take_sink().unwrap_or_default();
        assert_eq!(rc, 0);
        assert_eq!(sortie.trim(), "75025");
        println!("[rvm] fib25 interpréteur = {} (égal)", sortie.trim());
    }

    #[test]
    fn affichage_capture_et_bigint_exact_via_pool() {
        let mut c = ClasseRose::neuf("Donnees");
        let msg = chn(&mut c.cp, "salut JVM");
        c.ajouter_methode(MethodeRose::neuf(
            "saluer", Descripteur::neuf(vec![], TyDesc::Vide), &[],
            vec![InstrBc::PousserConst { index: msg },
                 InstrBc::Afficher, InstrBc::RetourVide],
        ).unwrap()).unwrap();
        let big = c.cp.ajouter(Const::BigInt("123456789012345678901234567890".to_string())).unwrap();
        c.ajouter_methode(MethodeRose::neuf(
            "grand", Descripteur::neuf(vec![], TyDesc::Entier), &[],
            vec![InstrBc::PousserConst { index: big }, InstrBc::RetourValeur],
        ).unwrap()).unwrap();
        let octets = ecrire_classe(&c);

        let mut hote = HoteBc::neuf(8);
        let g = {
            let ch = charger_fichier(&octets).unwrap();
            let mut vm = MachineJvm::neuf(&ch, &mut hote, LimitesJvm::default());
            vm.invoquer("Donnees", "saluer", &[]).unwrap();
            vm.invoquer("Donnees", "grand", &[]).unwrap()
        };
        let grand = cle(&g);
        assert_eq!(grand, "123456789012345678901234567890");
        let sortie = hote.sortie.clone();
        assert_eq!(sortie, "salut JVM");
        println!("[rvm] sortie={:?} bigint={}", sortie, grand);
    }

    // ══════════════════════════════════════════════════════════════════════
    // Snapshot AST complet (v3) — la RVM exécute TOUT le langage.
    // ══════════════════════════════════════════════════════════════════════

    /// Compare la sortie de la RVM (snapshot v3) à celle de l'interpréteur,
    /// sur la MÊME source, sous le verrou du tampon de sortie global.
    fn parite_snapshot(src: &str) -> (String, String) {
        let _g = sink_guard();
        let prog = crate::front::analyser(src).expect("analyse snapshot");
        let octets = ecrire_snapshot(&prog);
        let rvm = executer_snapshot(&octets, &[]).expect("exécution snapshot");
        crate::run::set_sink(true);
        let rc = crate::interp::executer_source(src, Vec::new());
        let interp = crate::run::take_sink().unwrap_or_default();
        assert_eq!(rc, 0, "interpréteur en échec sur : {}", src);
        (rvm, interp)
    }

    #[test]
    fn snapshot_v3_round_trip_structure() {
        let src = "fn f(n: entier) -> entier { si (n < 2) { retour n }; retour f(n-1) + f(n-2) }\n>> f(10)\n";
        let prog = crate::front::analyser(src).unwrap();
        let octets = ecrire_snapshot(&prog);
        assert!(octets.starts_with(MAGIC));
        assert_eq!(version_fichier(&octets).unwrap(), VERSION_AST);
        let prog2 = lire_snapshot(&octets).expect("relecture snapshot");
        assert_eq!(prog2.ordre, prog.ordre);
        assert_eq!(prog2.instructions.len(), prog.instructions.len());
        assert_eq!(prog2.imports, prog.imports);
        for nom in &prog.ordre {
            let a = prog.signatures[nom].corps.as_ref().unwrap().len();
            let b = prog2.signatures[nom].corps.as_ref().unwrap().len();
            assert_eq!(a, b, "corps de `{}` tronqué", nom);
            assert!(prog2.signatures[nom].rir.is_none(), "cache RIR ne doit pas être sérialisé");
        }
        println!("[rvm v3] snapshot {} octets, {} fonction(s)",
                 octets.len(), prog.ordre.len());
    }

    #[test]
    fn snapshot_v3_fib_recursif_parite_interpreteur() {
        let src = "fn fib(n: entier) -> entier { si (n < 2) { retour n }; retour fib(n-1) + fib(n-2) }\n>> fib(25)\n";
        let (rvm, interp) = parite_snapshot(src);
        assert_eq!(rvm.trim(), "75025", "sortie RVM : {:?}", rvm);
        assert_eq!(rvm, interp);
        let src28 = "fn fib(n: entier) -> entier { si (n < 2) { retour n }; retour fib(n-1) + fib(n-2) }\n>> fib(28)\n";
        let (rvm28, interp28) = parite_snapshot(src28);
        assert_eq!(rvm28.trim(), "317811", "sortie RVM : {:?}", rvm28);
        assert_eq!(rvm28, interp28);
        println!("[rvm v3] fib(25)=75025, fib(28)=317811 (parité interpréteur)");
    }

    #[test]
    fn snapshot_v3_chaines_tables_rationnels_et_appels() {
        let src = concat!(
            "fn double(n: entier) -> entier { retour n * 2 }\n",
            ">> double(21)\n",
            ">> \"bonjour\"\n",
            ">> [1, 2, 3]\n",
            ">> 1/3 + 1/6\n",
            "$t = table()\n",
            "$t[\"a\"] = 1\n",
            "$t[\"b\"] = 2\n",
            ">> $t\n",
        );
        let (rvm, interp) = parite_snapshot(src);
        assert_eq!(rvm, interp);
        assert!(rvm.contains("42"), "appel de fonction : {:?}", rvm);
        assert!(rvm.contains("bonjour"), "chaîne : {:?}", rvm);
        assert!(rvm.contains("[1, 2, 3]"), "liste : {:?}", rvm);
        assert!(rvm.contains("0.5"), "rationnel : {:?}", rvm);
        assert!(rvm.contains("a => 1"), "table : {:?}", rvm);
        println!("[rvm v3] chaînes/tables/rationnels/appels : {:?}", rvm);
    }

    #[test]
    fn snapshot_v3_parite_boucles_et_irrationnelles() {
        // Une boucle `pour` ne doit PAS afficher la formule LaTeX que l'AST
        // pur émet : la RVM abaisse le principal en RIR, comme `rose run`.
        let src = "$s = 0\npour j de 0 vers 10 { $s = $s + j }\n>> $s\n";
        let (rvm, interp) = parite_snapshot(src);
        assert_eq!(rvm, interp);
        assert_eq!(rvm.trim(), "45");
        assert!(!rvm.contains("\\sum"), "formule LaTeX parasite : {:?}", rvm);

        // ⚠ `pi` est une constante du langage, mais le RIR ne la résout pas.
        // La RVM doit détecter ce cas et rester en AST pur (plus complet).
        let src_pi = ">> pi\n";
        let _g = sink_guard();
        let prog = crate::front::analyser(src_pi).unwrap();
        let sortie = executer_snapshot(&ecrire_snapshot(&prog), &[]).unwrap();
        assert!(sortie.contains("π"), "sortie π : {:?}", sortie);
        println!("[rvm v3] boucles (sans formule) et π OK");
    }

    #[test]
    fn snapshot_v3_refuse_fichier_hostile() {
        let src = ">> 1 + 1\n";
        let mut octets = ecrire_snapshot(&crate::front::analyser(src).unwrap());
        let mut mauvais = octets.clone();
        mauvais[0] = b'X';
        assert!(matches!(lire_snapshot(&mauvais), Err(Erreur::MagicInvalide)));
        let mut mauvais = octets.clone();
        mauvais[6..10].copy_from_slice(&99u32.to_le_bytes());
        assert!(matches!(lire_snapshot(&mauvais), Err(Erreur::VersionInsupportee(99))));
        assert!(matches!(lire_snapshot(&octets[..octets.len() - 1]), Err(Erreur::Tronque { .. })));
        octets.push(0);
        assert!(matches!(lire_snapshot(&octets), Err(Erreur::OctetsEnTrop(1))));
    }
}
