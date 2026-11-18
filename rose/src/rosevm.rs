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
// Il n'y a **ni GC générationnel, ni chargeur de classes, ni compilation JIT
// depuis `.rosebc`, ni chargement dynamique**. Voir `docs/ROSE_VM.md`.
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
// Licence MIT.

use std::fmt;

use crate::front::{Expr, Instr};
use crate::i128x::Gz;
use crate::rir::{
    CacheSlot, CorpsRir, Eabstrait, FinRir, HoteRir, Op, Oper, executer_avec, poser_slot,
};
use crate::run::Valeur;

// ── Constantes du format ──────────────────────────────────────────────────

/// Magic ASCII, distinct de `ROSE_MAGIC` (queue des exécutables `rose comp`).
pub const MAGIC: &[u8; 6] = b"ROSEBC";
/// Version du format. Un changement incompatible doit l'incrémenter.
pub const VERSION: u32 = 1;

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

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::front::analyser;
    use crate::rir::abaisser;
    use crate::run::{set_sink, take_sink};

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
}
