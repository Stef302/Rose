// ═══════════════════════════════════════════════════════════════════════
// NATIF — ÉMETTEUR MACHINE DEPUIS LE RIR (sans LLVM, sans Cranelift)
// ═══════════════════════════════════════════════════════════════════════
//
// M3.1. Traduit un `CorpsRir` (voir `rir.rs`) en octets machine x86-64 ou
// aarch64. AUCUN OUTIL EXTERNE : l'émetteur écrit les octets lui-même.
//
// ⚠⚠⚠ MODÈLE DE VALEURS — CHOIX DE CONCEPTION CENTRAL
//
// Le code émis travaille sur des **i64 non boxés** (un slot = 8 octets).
// Il NE touche JAMAIS à la représentation de `Valeur` (enum Rust dont la
// layout n'est pas garantie). Tout ce qui ne peut pas être réduit à un i64
// est délégué à une **trampoline** passée en paramètre (voir `Trampoline`).
// C'est ce qui rend l'émetteur robuste : aucun couplage à la layout Rust.
//
//   signature machine (SysV AMD64 / AAPCS64) :
//       i64 code(base: *mut i64, tramp: *const Trampoline)
//
//   registres réservés :
//       aarch64 : x19 = base, x20 = tramp, x0 = valeur (ET retour), x9 = second
//       x86-64  : rbx = base, r12 = tramp, rax = valeur, rcx = second
//
// ⚠ LES SAUTS portent des INDEX D'OPÉRATIONS (comme le RIR). L'émetteur
// construit une table op→octet et corrige les déplacements en fin d'émission.
//
// ⚠ SOUS-ENSEMBLE COUVERT (le « cas chaud ») :
//   Op   : Affect, RetourAbstrait, Saut, SautSiFauxAbstrait
//   Expr : Slot, Entier, Binaire(+,-,*,<,<=,>,>=,==,!=), Neg, Non,
//          AppelRir (via trampoline)
// Tout autre `Op` fait échouer `emettre` avec `EmitError::OperationNonSupportee`
// (repli interprété — aucune régression). Tout autre `Eabstrait` passe par
// `Trampoline::eval_fallback`, avec un identifiant enregistré dans
// `Emitted::fallbacks` (l'hôte associe cet id à l'expression dans le même ordre).
//
// ⚠ BOOLÉENS non boxés : vrai = 1, faux = 0. `SautSiFauxAbstrait` saute
// quand la valeur != 1 (donc un entier quelconque est « faux », comme dans
// le moteur qui n'accepte que `Valeur::Booleen(true)`). L'entier littéral 1
// utilisé comme test est la seule divergence connue (documentée dans NATIF.md).

use crate::front::Expr;
use crate::rir::{CorpsRir, Eabstrait, Op, Oper};
use std::fmt;
use std::io;

// ── API PUBLIQUE ───────────────────────────────────────────────────────

/// Architecture cible de l'émission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Architecture { Aarch64, X86_64 }

impl Architecture {
    /// Architecture de la machine sur laquelle ce code est compilé.
    pub fn native() -> Self {
        #[cfg(target_arch = "aarch64")]
        { Architecture::Aarch64 }
        #[cfg(target_arch = "x86_64")]
        { Architecture::X86_64 }
        #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
        { Architecture::Aarch64 }
    }
    pub fn nom(self) -> &'static str {
        match self { Architecture::Aarch64 => "aarch64", Architecture::X86_64 => "x86-64" }
    }
}

/// Nombre de slots de temporaires réservés APRÈS les slots utilisateur.
/// Ils servent à évaluer les sous-expressions sans clobber les vrais slots.
pub const SCRATCH_SLOTS: u16 = 16;

/// Table de fonctions fournie par l'hôte au code machine.
///
/// ⚠ `#[repr(C)]` : les décalages sont garantis (0 / 8 / 16) sur les deux
/// cibles — le code émis lit `[tramp+0]`, `[tramp+8]` et `[tramp+16]`.
#[repr(C)]
pub struct Trampoline {
    /// `appel_rir(a0, a1, a2, a3, nom)` : appelle la fonction Rose `nom`
    /// avec jusqu'à 4 arguments i64. ABI identique à
    /// `jit::rose_appel_interpreteur` (réutilisable telle quelle).
    pub appel_rir: extern "C" fn(i64, i64, i64, i64, *const u8) -> i64,
    /// `eval_fallback(ctx, expr_id, base)` : évalue l'expression non-native
    /// numéro `expr_id` (ordre d'apparition dans `Emitted::fallbacks`).
    pub eval_fallback: extern "C" fn(*mut std::ffi::c_void, u32, *mut i64) -> i64,
    /// Contexte opaque de l'hôte (transmis à `eval_fallback`).
    pub ctx: *mut std::ffi::c_void,
}

/// Un appel RIR émis (pour introspection / câblage de la table de noms).
#[derive(Clone, Debug)]
pub struct Appel {
    pub nom: String,
    /// Slot de chaque argument (None = non-résolu, remplacé par 0).
    pub args_slots: [Option<u16>; 4],
}

/// Résultat d'une émission.
#[derive(Clone, Debug)]
pub struct Emitted {
    /// Octets machine (code, puis chaînes de noms NUL-terminées).
    pub code: Vec<u8>,
    pub architecture: Architecture,
    /// Nombre de slots utilisateur (`corps.n_slots`).
    pub n_slots: usize,
    /// Nombre total de slots à allouer (utilisateur + temporaires).
    pub n_slots_total: usize,
    /// Décalage octet de chaque opération (len = ops.len() + 1 ; le dernier
    /// est le début de l'épilogue). Utile pour le débogage / les points d'entrée.
    pub op_offsets: Vec<usize>,
    /// Appels RIR émis, dans l'ordre.
    pub appels: Vec<Appel>,
    /// Identifiants des expressions évaluées par la trampoline, dans l'ordre.
    pub fallbacks: Vec<u32>,
}

/// Un appel résolu directement vers une autre fonction du MÊME programme.
///
/// ⚠ Aucun pointeur de fonction n'est stocké ici : seulement l'INDEX de la
/// fonction cible dans `ProgrammeEmis::noms`. La correction de l'instruction
/// de saut se fait une fois que toutes les fonctions ont été émises et
/// concaténées (les déplacements sont donc purement internes au segment).
#[derive(Clone, Debug)]
pub struct AppelLie {
    pub nom: String,
    pub cible: usize,
    /// Décalage, dans le code de la fonction émettrice, du champ à corriger :
    /// début de l'instruction `bl` (aarch64) ou des 4 octets `rel32` (x86-64).
    pub correctif: usize,
}

/// Informations de liaison du runtime natif AOT6 : décalages des routines
/// dans le blob `code`, utilisés par le stub du linker pour formater la
/// valeur finale.
#[derive(Clone, Debug)]
pub struct RuntimeLiens {
    /// Décalage de `$rt_sortir` (écrit SORTIE sur stdout).
    pub sortir: usize,
    /// Décalage de `$rt_formater_flottant`, si le programme affiche un flottant.
    pub formater: Option<usize>,
    /// Tag de la valeur finale (0 = entier → stub décimal historique).
    pub tag: u8,
    /// Dernière adresse absolue que le runtime doit voir mappée. Les
    /// programmes sans bigint n'utilisent ni TMP_ADDR ni le tampon de
    /// digits : le linker peut donc s'arrêter juste après SORTIE (−8 Ko).
    pub zone_fin: u64,
}

/// Résultat d'une émission de TOUTES les fonctions d'un programme, avec
/// appels inter-fonctions résolus par des adresses relatives internes.
///
/// Le code est la concaténation des corps dans l'ordre de `noms` ; `offsets[i]`
/// donne le début du corps `i`. Le stub d'entrée du linker appelle `offsets[0]`.
#[derive(Clone, Debug)]
pub struct ProgrammeEmis {
    pub code: Vec<u8>,
    pub architecture: Architecture,
    /// Noms des fonctions, dans l'ordre (index 0 = fonction d'entrée).
    pub noms: Vec<String>,
    pub offsets: Vec<usize>,
    /// Taille d'une trame de slots, IDENTIQUE pour toutes les fonctions.
    /// C'est elle que le stub d'entrée doit réserver pour le premier appel.
    pub frame_bytes: usize,
    pub n_slots_total: usize,
    /// Appels directs (inclus pour introspection / vérification).
    pub appels: Vec<AppelLie>,
    /// Doit rester vide : le mode « lié » refuse toute expression qui
    /// demanderait la trampoline (elle rendrait une valeur fausse).
    pub fallbacks: Vec<u32>,
    /// ⚠⚠⚠ AOT « valeurs exactes » (vague 78) : octets à écrire sur stdout
    /// par le stub d'entrée (texte décimal + `'\n'`, déjà calculé par
    /// l'interpréteur Rose à la compilation). `None` dans le chemin RIR
    /// historique : le stub continue d'imprimer la valeur i64 rendue par le
    /// code machine, sans aucun changement de comportement.
    pub sortie: Option<Vec<u8>>,
    /// AOT6 : routines du runtime natif présentes dans `code`.
    pub runtime: Option<RuntimeLiens>,
}

/// Erreurs d'émission.
#[derive(Debug)]
pub enum EmitError {
    /// L'opération n'appartient pas au sous-ensemble natif (repli interprété).
    OperationNonSupportee(String),
    /// Profondeur d'expression trop grande pour les slots temporaires.
    TropDeScratch,
    /// Littéral entier hors de l'intervalle i64.
    EntierHorsPlage(String),
    /// Argument d'appel non résolu en slot.
    SlotInconnu(String),
    /// Saut cible invalide.
    SautHorsPlage(usize),
    /// Appel RIR avec plus de 4 arguments (limite de l'ABI trampoline).
    TropArguments,
    /// Appel à une fonction absente du programme compilé.
    FonctionInconnue(String),
    /// Trame de slots trop grande pour un encodage d'immédiat machine.
    TrameTropGrande(usize),
    /// Appel trop éloigné pour l'encodage relatif de la cible.
    DepassementAppel(String),
}
impl fmt::Display for EmitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EmitError::OperationNonSupportee(s) => write!(f, "op RIR non-native: {}", s),
            EmitError::TropDeScratch => write!(f, "expression trop profonde (> {} temporaires)", SCRATCH_SLOTS),
            EmitError::EntierHorsPlage(s) => write!(f, "entier hors plage i64: {}", s),
            EmitError::SlotInconnu(s) => write!(f, "argument '{}' n'est pas un slot", s),
            EmitError::SautHorsPlage(i) => write!(f, "saut vers op {} hors plage", i),
            EmitError::TropArguments => write!(f, "AppelRir: plus de 4 arguments (ABI)"),
            EmitError::FonctionInconnue(n) => write!(f, "appel à une fonction non compilée: {}", n),
            EmitError::TrameTropGrande(n) => write!(f, "trame de {} octets trop grande", n),
            EmitError::DepassementAppel(n) => write!(f, "appel vers {} trop éloigné", n),
        }
    }
}
impl std::error::Error for EmitError {}

/// Vrai si TOUTES les opérations du corps appartiennent au sous-ensemble
/// émettable. L'orchestrateur peut appeler ceci pour choisir la voie native
/// sans avoir à traiter une `EmitError`.
pub fn peut_emettre(corps: &CorpsRir) -> bool {
    fn expr_ok(e: &Eabstrait) -> bool {
        match e {
            Eabstrait::Slot(..) | Eabstrait::Entier(_) => true,
            Eabstrait::Binaire { op, a, b } => {
                matches!(op, Oper::Add | Oper::Sub | Oper::Mul | Oper::Lt
                          | Oper::Le | Oper::Gt | Oper::Ge | Oper::Eq | Oper::Ne)
                    && expr_ok(a) && expr_ok(b)
            }
            Eabstrait::Neg(a) | Eabstrait::Non(a) => expr_ok(a),
            Eabstrait::AppelRir { .. } => true,
            // Flottant, Div, Mod, Puissance, DivEntier, Et, Ou → fallback.
            _ => true,
        }
    }
    corps.ops.iter().all(|op| match op {
        Op::Affect { expr, .. } | Op::RetourAbstrait(expr)
        | Op::SautSiFauxAbstrait { test: expr, .. } => expr_ok(expr),
        Op::Saut { cible } => *cible <= corps.ops.len(),
        _ => false,
    })
}

/// Émet le code machine d'un corps RIR.
pub fn emettre(corps: &CorpsRir, arch: Architecture) -> Result<Emitted, EmitError> {
    let mut b = Builder::new(arch, corps);
    b.prologue();

    let mut op_offsets = Vec::with_capacity(corps.ops.len() + 1);
    for (i, op) in corps.ops.iter().enumerate() {
        op_offsets.push(b.code.len());
        b.operation(op, i + 1 == corps.ops.len())?;
    }
    op_offsets.push(b.code.len()); // début de l'épilogue (= cible des `retour`)
    b.epilogue();

    for cible in op_offsets.iter().take(corps.ops.len()) {
        if *cible > b.code.len() { return Err(EmitError::SautHorsPlage(*cible)); }
    }

    // Les sauts ciblent des ops : corriger maintenant que les offsets sont connus.
    b.corriger_sauts(&op_offsets);

    // Ajouter les chaînes APRÈS le code, puis corriger les références PC-relatives.
    let base_chaines = b.code.len();
    let mut offsets_chaines = Vec::with_capacity(b.noms.len());
    for nom in &b.noms {
        offsets_chaines.push(b.code.len() - base_chaines);
        b.code.extend_from_slice(nom.as_bytes());
        b.code.push(0);
    }
    b.corriger_chaines(base_chaines, &offsets_chaines);

    Ok(Emitted {
        code: b.code,
        architecture: arch,
        n_slots: corps.n_slots,
        n_slots_total: corps.n_slots + SCRATCH_SLOTS as usize,
        op_offsets,
        appels: b.appels,
        fallbacks: b.fallbacks,
    })
}

// ═══════════════════════════════════════════════════════════════════════
// AOT — TOUTES LES FONCTIONS DANS UN SEUL SEGMENT, APPELS DIRECTS
// ═══════════════════════════════════════════════════════════════════════
//
// Ici, plus de trampoline : chaque `AppelRir` devient un `bl`/`call` vers
// l'adresse de la fonction émise. La trame de l'appelé est allouée SOUS le
// pointeur de trame courant (`sp` / `rsp`), donc la récursion est naturelle
// et aucun espace fixe n'est à réserver. Le registre de base (x19 / rbx) est
// callee-saved : l'appelé le préserve, l'appelant le relit après le retour.

/// Arrondit à 16 octets (alignement des piles SysV / AAPCS64).
fn aligner16(v: usize) -> usize { (v + 15) & !15 }

/// Émet toutes les fonctions et résout les appels entre elles.
///
/// `fonctions[0]` est la fonction d'entrée (le `$principal` AOT). Toutes les
/// trames ont la MÊME taille `frame_bytes`, calculée comme le maximum des
/// besoins des fonctions : un appel n'a donc pas besoin de connaître la
/// taille de la trame de l'appelé, seulement son adresse.
pub fn emettre_programme(fonctions: &[(String, CorpsRir)], arch: Architecture)
    -> Result<ProgrammeEmis, EmitError>
{
    if fonctions.is_empty() {
        return Err(EmitError::OperationNonSupportee("programme sans fonction".into()));
    }
    let noms: Vec<String> = fonctions.iter().map(|(n, _)| n.clone()).collect();
    let max_slots = fonctions.iter()
        .map(|(_, c)| c.n_slots + SCRATCH_SLOTS as usize)
        .max().unwrap_or(0);
    let frame_bytes = aligner16(max_slots.checked_mul(8)
        .ok_or_else(|| EmitError::TrameTropGrande(max_slots))?);
    if frame_bytes > 0x10_0000 {
        return Err(EmitError::TrameTropGrande(frame_bytes));
    }

    let mut code: Vec<u8> = Vec::new();
    let mut offsets = Vec::with_capacity(fonctions.len());
    let mut appels: Vec<AppelLie> = Vec::new();
    let mut fallbacks: Vec<u32> = Vec::new();

    for (_, corps) in fonctions {
        let mut b = Builder::new_lie(arch, corps, &noms, frame_bytes);
        b.prologue();
        let mut op_offsets = Vec::with_capacity(corps.ops.len() + 1);
        for (i, op) in corps.ops.iter().enumerate() {
            op_offsets.push(b.code.len());
            b.operation(op, i + 1 == corps.ops.len())?;
        }
        op_offsets.push(b.code.len()); // épilogue (cible des `retour`)
        b.epilogue();
        // ⚠ Indispensable : sans cela, les `si` et les `retour` sautent à
        // l'offset 0 (boucle infinie). Même correction que `emettre`.
        b.corriger_sauts(&op_offsets);
        let debut = code.len();
        offsets.push(debut);
        for a in b.appels_lies {
            appels.push(AppelLie { nom: a.nom, cible: a.cible, correctif: debut + a.correctif });
        }
        fallbacks.extend(b.fallbacks);
        code.extend_from_slice(&b.code);
    }

    // Les functions sont contiguës : la correction est indépendante de
    // l'adresse de chargement (même segment PT_LOAD / section .text).
    for a in &appels {
        let cible = *offsets.get(a.cible).ok_or_else(|| EmitError::FonctionInconnue(a.nom.clone()))?;
        match arch {
            Architecture::Aarch64 => {
                let disp = (cible as i64 - a.correctif as i64) / 4;
                if disp < -(1 << 25) || disp >= (1 << 25) {
                    return Err(EmitError::DepassementAppel(a.nom.clone()));
                }
                let insn = 0x9400_0000u32 | ((disp as u32) & 0x03FF_FFFF);
                code[a.correctif..a.correctif + 4].copy_from_slice(&insn.to_le_bytes());
            }
            Architecture::X86_64 => {
                let fin = a.correctif + 4;
                let disp = cible as i64 - fin as i64;
                if disp < i32::MIN as i64 || disp > i32::MAX as i64 {
                    return Err(EmitError::DepassementAppel(a.nom.clone()));
                }
                code[a.correctif..fin].copy_from_slice(&(disp as i32).to_le_bytes());
            }
        }
    }

    Ok(ProgrammeEmis {
        code,
        architecture: arch,
        noms,
        offsets,
        frame_bytes,
        n_slots_total: frame_bytes / 8,
        appels,
        fallbacks,
        sortie: None,
        runtime: None,
    })
}

/// Configuration AOT6 : runtime natif et types des slots.
#[derive(Clone, Debug)]
pub struct ConfigAot {
    pub primitifs: Vec<crate::runtime_natif::Primitif>,
    /// Types des slots (0 = entier, 1 = flottant, 2 = bigint, 3 = chaîne).
    pub types_slots: Vec<u8>,
    /// Routine de formatage à appeler sur la valeur rendue (flottant).
    pub formater_sortie: bool,
    /// Tag de la valeur finale.
    pub tag_sortie: u8,
}

/// Émet un programme AOT6 : les fonctions Rose, puis le runtime natif
/// demandé, dans un seul segment avec appels relatifs résolus. Les routines
/// du runtime suivent la même signature machine, elles sont donc dans la
/// table d'appels comme n'importe quelle fonction Rose.
pub fn emettre_programme_aot(
    fonctions: &[(String, CorpsRir)],
    arch: Architecture,
    cfg: &ConfigAot,
) -> Result<ProgrammeEmis, EmitError> {
    // `natif_emetteur` est compilé deux fois (module `jit::ne_local` en
    // configuration autonome) : on convertit explicitement l'architecture.
    let runtime_arch = match arch {
        Architecture::Aarch64 => crate::natif_emetteur::Architecture::Aarch64,
        Architecture::X86_64 => crate::natif_emetteur::Architecture::X86_64,
    };
    let runtime = crate::runtime_natif::emettre_runtime(runtime_arch, &cfg.primitifs);
    let mut noms: Vec<String> = fonctions.iter().map(|(n, _)| n.clone()).collect();
    for (n, _) in &runtime.routines { noms.push(n.clone()); }
    let mut types = cfg.types_slots.clone();

    let max_slots = fonctions.iter()
        .map(|(_, c)| c.n_slots + SCRATCH_SLOTS as usize)
        .max().unwrap_or(0);
    let frame_bytes = aligner16(max_slots.checked_mul(8)
        .ok_or_else(|| EmitError::TrameTropGrande(max_slots))?);
    if frame_bytes > 0x10_0000 { return Err(EmitError::TrameTropGrande(frame_bytes)); }

    let mut code: Vec<u8> = Vec::new();
    let mut offsets = Vec::with_capacity(fonctions.len());
    let mut appels: Vec<AppelLie> = Vec::new();

    for (_, corps) in fonctions {
        let mut b = Builder::new_lie(arch, corps, &noms, frame_bytes);
        b.types = types.clone();
        b.avec_runtime = true;
        b.prologue();
        let mut op_offsets = Vec::with_capacity(corps.ops.len() + 1);
        for (i, op) in corps.ops.iter().enumerate() {
            op_offsets.push(b.code.len());
            b.operation(op, i + 1 == corps.ops.len())?;
        }
        op_offsets.push(b.code.len());
        b.epilogue();
        b.corriger_sauts(&op_offsets);
        let debut = code.len();
        offsets.push(debut);
        for a in b.appels_lies {
            appels.push(AppelLie { nom: a.nom, cible: a.cible, correctif: debut + a.correctif });
        }
        // On propage les types appris (les affectations du principal).
        if types.len() < b.types.len() { types = b.types.clone(); }
        code.extend_from_slice(&b.code);
    }

    // Le runtime est concaténé après les fonctions Rose.
    let mut runtime_offsets = Vec::new();
    for (nom, off) in runtime.routines {
        runtime_offsets.push((nom, code.len() + off));
    }
    code.extend_from_slice(&runtime.code);

    for a in &appels {
        let cible = if let Some(i) = noms.iter().position(|n| n == &a.nom) {
            if i < offsets.len() { offsets[i] } else {
                runtime_offsets.iter().find(|(n, _)| n == &a.nom).map(|(_, o)| *o)
                    .ok_or_else(|| EmitError::FonctionInconnue(a.nom.clone()))?
            }
        } else {
            return Err(EmitError::FonctionInconnue(a.nom.clone()));
        };
        match arch {
            Architecture::Aarch64 => {
                let disp = (cible as i64 - a.correctif as i64) / 4;
                if disp < -(1 << 25) || disp >= (1 << 25) {
                    return Err(EmitError::DepassementAppel(a.nom.clone()));
                }
                let insn = 0x9400_0000u32 | ((disp as u32) & 0x03FF_FFFF);
                code[a.correctif..a.correctif + 4].copy_from_slice(&insn.to_le_bytes());
            }
            Architecture::X86_64 => {
                let fin = a.correctif + 4;
                let disp = cible as i64 - fin as i64;
                if disp < i32::MIN as i64 || disp > i32::MAX as i64 {
                    return Err(EmitError::DepassementAppel(a.nom.clone()));
                }
                code[a.correctif..fin].copy_from_slice(&(disp as i32).to_le_bytes());
            }
        }
    }

    let sortir = runtime_offsets.iter()
        .find(|(n, _)| n == "$rt_sortir").map(|(_, o)| *o)
        .ok_or_else(|| EmitError::FonctionInconnue("$rt_sortir".into()))?;
    let formater = if cfg.formater_sortie {
        Some(runtime_offsets.iter()
            .find(|(n, _)| n == "$rt_formater_flottant").map(|(_, o)| *o)
            .ok_or_else(|| EmitError::FonctionInconnue("$rt_formater_flottant".into()))?)
    } else { None };

    Ok(ProgrammeEmis {
        code,
        architecture: arch,
        noms,
        offsets,
        frame_bytes,
        n_slots_total: frame_bytes / 8,
        appels,
        fallbacks: Vec::new(),
        sortie: None,
        runtime: Some(RuntimeLiens {
            sortir, formater, tag: cfg.tag_sortie,
            zone_fin: if cfg.primitifs.contains(&crate::runtime_natif::Primitif::BigintPuissance) {
                crate::runtime_natif::ZONE_FIN
            } else {
                // Ni TMP ni le tampon de digits ne sont touchés sans bigint :
                // 1 Kio dans SORTIE suffit largement (un f64 exact < 40 o).
                crate::runtime_natif::SORTIE_ADDR + 1024
            },
        }),
    })
}

/// ⚠ Émet un programme AOT « valeur exacte » : le code machine est le
/// plus petit possible (`retour <partie i64>`), et la valeur décimale
/// complète est mémorisée dans `sortie` pour être écrite par le stub
/// (`link_natif::elf_bytes`). Le texte fourni DOIT être exactement celui que
/// l'interpréteur afficherait (y compris son `'\n'` final).
pub fn emettre_valeur_exacte(corps: &CorpsRir, arch: Architecture, texte: &[u8])
    -> Result<ProgrammeEmis, EmitError>
{
    let mut p = emettre_programme(&[("$principal".to_string(), corps.clone())], arch)?;
    p.sortie = Some(texte.to_vec());
    Ok(p)
}

// ═══════════════════════════════════════════════════════════════════════
// AOT — BUILTIN RNG : `hasard(n)` EN PUR CODE MACHINE
// ═══════════════════════════════════════════════════════════════════════
//
// `hasard` est une primitive du catalogue (pas une fonction Rose) : le RIR
// historique ne peut donc pas l'abaisser (`abstraire` rend `None`). Plutôt que
// de recopier le moteur dans le binaire, on écrit ici directement les octets
// de splitmix64 + réduction de Lemire — les mêmes constantes et la même
// formule que `run::rng_tirer` / `run::hasard` (voir run.rs). Aucun appel à
// une fonction du moteur : la graine vient du syscall `getrandom` (mélangé à
// l'adresse ASLR de la pile), et le résultat est rendu dans x0/rax pour que
// le stub du linker l'imprime en décimal.
//
// ⚠ `n <= 0` est refusé AVANT émission : le moteur lèverait
// « hasard : bornage positif attendu ». On ne produit jamais de binaire qui
// afficherait une valeur fausse.

const SM_C: u64 = 0x9E37_79B9_7F4A_7C15;
const SM_M1: u64 = 0xBF58_476D_1CE4_E5B9;
const SM_M2: u64 = 0x94D0_49BB_1331_11EB;

/// Petit assembleur local : les opcodes sont calculés (pas écrits en dur)
/// pour que l'encodage des registres reste lisible et vérifiable.
struct Asm { v: Vec<u8> }
impl Asm {
    fn new() -> Self { Asm { v: Vec::with_capacity(256) } }
    fn u32(&mut self, w: u32) { self.v.extend_from_slice(&w.to_le_bytes()); }
    fn oct(&mut self, xs: &[u8]) { self.v.extend_from_slice(xs); }
    fn b(&mut self, x: u8) { self.v.push(x); }

    // ── aarch64 (AAPCS64) ──────────────────────────────────────────────
    fn a64_mov_imm(&mut self, r: u32, v: u64) {
        let bits = v;
        self.u32(0xD280_0000 | (((bits & 0xFFFF) as u32) << 5) | r);
        for hw in 1..4u32 {
            let chunk = ((bits >> (16 * hw)) & 0xFFFF) as u32;
            if chunk != 0 {
                self.u32(0xF280_0000 | (hw << 21) | (chunk << 5) | r);
            }
        }
    }
    fn a64_orr(&mut self, rd: u32, rn: u32, rm: u32) {
        self.u32(0xAA00_0000 | (rm << 16) | (rn << 5) | rd);
    }
    fn a64_add(&mut self, rd: u32, rn: u32, rm: u32) {
        self.u32(0x8B00_0000 | (rm << 16) | (rn << 5) | rd);
    }
    fn a64_mul(&mut self, rd: u32, rn: u32, rm: u32) {
        self.u32(0x9B00_7C00 | (rm << 16) | (rn << 5) | rd);
    }
    fn a64_umulh(&mut self, rd: u32, rn: u32, rm: u32) {
        self.u32(0x9BC0_7C00 | (rm << 16) | (rn << 5) | rd);
    }
    fn a64_and(&mut self, rd: u32, rn: u32, rm: u32) {
        self.u32(0x8A00_0000 | (rm << 16) | (rn << 5) | rd);
    }
    /// `eor rd, rn, rm` (LSL #0).
    fn a64_eor(&mut self, rd: u32, rn: u32, rm: u32) {
        self.u32(0xCA00_0000 | (rm << 16) | (rn << 5) | rd);
    }
    /// `eor rd, rn, rm, lsr #sh` (shift code 01 = LSR).
    fn a64_eor_lsr(&mut self, rd: u32, rn: u32, rm: u32, sh: u32) {
        self.u32(0xCA00_0000 | (1u32 << 22) | (sh << 10) | (rm << 16) | (rn << 5) | rd);
    }
    fn a64_ldr(&mut self, rt: u32, rn: u32) { self.u32(0xF940_0000 | (rn << 5) | rt); }
    fn a64_str(&mut self, rt: u32, rn: u32) { self.u32(0xF900_0000 | (rn << 5) | rt); }

    // ── x86-64 (SysV) ──────────────────────────────────────────────────
    /// REX.W + opcode(s) + ModRM(mod=11) : `opcode reg, rm`.
    fn x86_rr(&mut self, opcode: &[u8], reg: u8, rm: u8) {
        let rex = 0x48 | if reg >= 8 { 0x04 } else { 0 } | if rm >= 8 { 0x01 } else { 0 };
        self.b(rex);
        self.oct(opcode);
        self.b(0xC0 | ((reg & 7) << 3) | (rm & 7));
    }
    fn x86_mov_imm(&mut self, r: u8, v: u64) {
        self.b(0x48 | if r >= 8 { 0x01 } else { 0 });
        self.b(0xB8 | (r & 7));
        self.oct(&v.to_le_bytes());
    }
    fn x86_shr(&mut self, r: u8, sh: u8) {
        self.b(0x48 | if r >= 8 { 0x01 } else { 0 });
        self.b(0xC1);
        self.b(0xC8 | (5u8 << 3) | (r & 7)); // reg=5 = SHR
        self.b(sh);
    }
    fn x86_load_base(&mut self, r: u8) {
        // mov r, [rbx]  (rbx = base, jamais >= 8 → pas de REX.B)
        let rex = 0x48 | if r >= 8 { 0x04 } else { 0 };
        self.b(rex); self.b(0x8B); self.b(0x03 | ((r & 7) << 3));
    }
    fn x86_store_base(&mut self, r: u8) {
        // mov [rbx], r : opcode 89, ModRM mod=00 reg=r rm=rbx(011).
        // ⚠ PAS `x86_rr` (mod=11) : cela produirait `mov rbx, r` et détruirait
        // la base au lieu d'écrire le slot (vérifié par désassemblage croisé).
        let rex = 0x48 | if r >= 8 { 0x04 } else { 0 };
        self.b(rex); self.b(0x89); self.b(0x03 | ((r & 7) << 3));
    }
}

/// Émet `hasard(n)` en code machine autonome. `n` doit être un entier > 0 ;
/// sa valeur est intégrée comme immédiat dans le code émis.
///
/// Le binaire résultant ne contient AUCUNE référence au moteur Rose : le
/// résultat est rendu dans le registre de retour (x0 / rax) et le stub
/// d'entrée du linker l'imprime en décimal (`afficher_valeur = true`).
pub fn emettre_hasard(borne: u64, arch: Architecture) -> Result<ProgrammeEmis, EmitError> {
    if borne == 0 {
        return Err(EmitError::OperationNonSupportee(
            "hasard : bornage positif attendu".to_string()));
    }
    let mut a = Asm::new();
    match arch {
        Architecture::Aarch64 => {
            // prologue : stp x29,x30,[sp,#-16]! ; mov x29,sp ; mov x19,x0 (base)
            a.u32(0xA9BF_7BFD); a.u32(0x9100_03FD); a.a64_orr(19, 31, 0);
            // slot 0 = état RNG, remis à zéro avant d'appeler getrandom.
            a.a64_str(31, 19);
            // getrandom(base, 8, 0) — syscall 278 sur aarch64.
            a.a64_orr(0, 31, 19);       // x0 = base (tampon)
            a.u32(0xD280_0101);         // mov x1, #8
            a.a64_orr(2, 31, 31);       // x2 = 0 (flags)
            a.a64_mov_imm(8, 278);      // x8 = __NR_getrandom
            a.u32(0xD400_0001);         // svc #0
            // état = graine XOR adresse ASLR de la pile (robuste si getrandom échoue)
            a.a64_ldr(9, 19);
            a.u32(0x9100_03EA);         // mov x10, sp
            a.a64_eor(9, 9, 10);        // eor x9, x9, x10
            // splitmix64 : état += C ; z = état
            a.a64_mov_imm(11, SM_C);
            a.a64_add(9, 9, 11);
            a.a64_str(9, 19);           // état persisté dans le slot 0
            a.a64_orr(12, 31, 9);       // x12 = z
            a.a64_eor_lsr(12, 12, 12, 30);
            a.a64_mov_imm(13, SM_M1);
            a.a64_mul(12, 12, 13);
            a.a64_eor_lsr(12, 12, 12, 27);
            a.a64_mov_imm(14, SM_M2);
            a.a64_mul(12, 12, 14);
            a.a64_eor_lsr(12, 12, 12, 31); // x12 = tirage u64
            // réduction de Lemire (miroir de run::hasard)
            if borne.is_power_of_two() {
                a.a64_mov_imm(15, borne - 1);
                a.a64_and(0, 12, 15);   // x0 = tirage & (borne-1)
            } else {
                a.a64_mov_imm(15, borne);
                a.a64_umulh(0, 12, 15); // x0 = (tirage * borne) >> 64
            }
            // épilogue
            a.u32(0xA8C1_7BFD); a.u32(0xD65F_03C0);
        }
        Architecture::X86_64 => {
            a.oct(&[0x55]);                 // push rbp
            a.oct(&[0x48, 0x89, 0xE5]);     // mov rbp, rsp
            a.oct(&[0x53]);                 // push rbx
            a.oct(&[0x41, 0x54]);           // push r12
            a.oct(&[0x48, 0x89, 0xFB]);     // mov rbx, rdi (base)
            a.oct(&[0x48, 0xC7, 0x03, 0, 0, 0, 0]); // mov qword [rbx], 0
            // getrandom(base, 8, 0) — syscall 318 sur x86-64.
            let rdi: u8 = 7; let rbx: u8 = 3;
            a.x86_rr(&[0x89], rbx, rdi);    // mov rdi, rbx
            a.oct(&[0x48, 0xC7, 0xC6, 8, 0, 0, 0]); // mov rsi, 8
            a.oct(&[0x31, 0xD2]);           // xor edx, edx
            a.oct(&[0xB8, 0x3E, 0x01, 0, 0]); // mov eax, 318
            a.oct(&[0x0F, 0x05]);           // syscall
            // état = graine XOR rsp
            a.x86_load_base(9);             // mov r9, [rbx]
            a.x86_rr(&[0x89], 4, 10);       // mov r10, rsp
            a.x86_rr(&[0x31], 10, 9);       // xor r9, r10
            // splitmix64
            a.x86_mov_imm(11, SM_C);
            a.x86_rr(&[0x01], 11, 9);       // add r9, r11
            a.x86_store_base(9);            // état persisté
            let r12: u8 = 12; let r13: u8 = 13;
            a.x86_rr(&[0x89], 9, r12);      // mov r12, r9
            a.x86_rr(&[0x89], r12, r13);    // copie pour le décalage
            a.x86_shr(r13, 30);
            a.x86_rr(&[0x31], r13, r12);    // xor r12, r13
            a.x86_mov_imm(14, SM_M1);
            a.x86_rr(&[0x0F, 0xAF], 14, r12); // imul r12, r14
            a.x86_rr(&[0x89], r12, r13);
            a.x86_shr(r13, 27);
            a.x86_rr(&[0x31], r13, r12);
            a.x86_mov_imm(14, SM_M2);
            a.x86_rr(&[0x0F, 0xAF], 14, r12);
            a.x86_rr(&[0x89], r12, r13);
            a.x86_shr(r13, 31);
            a.x86_rr(&[0x31], r13, r12);   // r12 = tirage
            // rax = tirage
            a.x86_rr(&[0x89], r12, 0);
            // réduction de Lemire
            a.x86_mov_imm(15, borne);
            if borne.is_power_of_two() {
                a.x86_mov_imm(15, borne - 1);
                a.x86_rr(&[0x21], 15, 0);  // and rax, r15
            } else {
                // mul r15 : rdx:rax = rax * r15 ; on rend la partie haute.
                a.b(0x49); a.b(0xF7); a.b(0xC0 | (4u8 << 3) | (15 & 7)); // mul r15
                a.x86_rr(&[0x89], 2, 0);   // mov rax, rdx
            }
            a.oct(&[0x41, 0x5C]);           // pop r12
            a.oct(&[0x5B]);                 // pop rbx
            a.oct(&[0x5D]);                 // pop rbp
            a.oct(&[0xC3]);                 // ret
        }
    }
    Ok(ProgrammeEmis {
        code: a.v,
        architecture: arch,
        noms: vec!["$principal".to_string()],
        offsets: vec![0],
        frame_bytes: 16,
        n_slots_total: 2,
        appels: Vec::new(),
        fallbacks: Vec::new(),
        sortie: None,
        runtime: None,
    })
}

// ═══════════════════════════════════════════════════════════════════════
// MÉMOIRE EXÉCUTABLE (mmap + mprotect) — aucun `libc` requis
// ═══════════════════════════════════════════════════════════════════════

/// Octets machine rendus exécutables et appelables.
pub struct MemoireExecutable {
    ptr: *mut u8,
    len: usize,
}

unsafe extern "C" {
    fn mmap(addr: *mut std::ffi::c_void, len: usize, prot: i32, flags: i32,
            fd: i32, offset: i64) -> *mut std::ffi::c_void;
    fn mprotect(addr: *mut std::ffi::c_void, len: usize, prot: i32) -> i32;
    fn munmap(addr: *mut std::ffi::c_void, len: usize) -> i32;
}

impl MemoireExecutable {
    const PROT_READ: i32 = 0x1;
    const PROT_WRITE: i32 = 0x2;
    const PROT_EXEC: i32 = 0x4;
    const MAP_PRIVATE: i32 = 0x02;
    const MAP_ANONYMOUS: i32 = 0x20;

    /// Copie `code` dans une page mmap'ée, puis la rend en lecture + exécution.
    pub fn nouveau(code: &[u8]) -> io::Result<Self> {
        if code.is_empty() { return Err(io::Error::new(io::ErrorKind::InvalidInput, "code vide")); }
        unsafe {
            let p = mmap(std::ptr::null_mut(), code.len(),
                         Self::PROT_READ | Self::PROT_WRITE,
                         Self::MAP_PRIVATE | Self::MAP_ANONYMOUS, -1, 0);
            if p as isize == -1 { return Err(io::Error::last_os_error()); }
            std::ptr::copy_nonoverlapping(code.as_ptr(), p as *mut u8, code.len());
            if mprotect(p, code.len(), Self::PROT_READ | Self::PROT_EXEC) != 0 {
                let e = io::Error::last_os_error();
                munmap(p, code.len());
                return Err(e);
            }
            Ok(MemoireExecutable { ptr: p as *mut u8, len: code.len() })
        }
    }

    pub fn as_ptr(&self) -> *const u8 { self.ptr as *const u8 }
    pub fn taille(&self) -> usize { self.len }

    /// Appelle le code émis : `i64 f(base: *mut i64, tramp: *const Trampoline)`.
    ///
    /// ⚠ L'appelant doit avoir alloué `n_slots_total` i64 dans `base` et
    /// posé les paramètres dans les `n_slots` premiers.
    pub unsafe fn appeler(&self, base: *mut i64, tramp: *const Trampoline) -> i64 {
        let f: extern "C" fn(*mut i64, *const Trampoline) -> i64 =
            std::mem::transmute(self.ptr);
        f(base, tramp)
    }
}
unsafe impl Send for MemoireExecutable {}
unsafe impl Sync for MemoireExecutable {}
impl Drop for MemoireExecutable {
    fn drop(&mut self) { unsafe { munmap(self.ptr as *mut std::ffi::c_void, self.len); } }
}

// ═══════════════════════════════════════════════════════════════════════
// ELF64 x86-64 AUTONOME — exécution CROSS-ARCH réelle (qemu-user / box64)
// ═══════════════════════════════════════════════════════════════════════
//
// L'émetteur PE32+ (`compilateur/emetteur_x64.rose`) produit un binaire
// WINDOWS : qemu-x86_64-static et box64 ne chargent QUE des ELF (mesuré :
// qemu lit l'en-tête `MZ` puis sort 1 ; box64 : « Reading elf header »).
// Cette fonction enveloppe le code machine x86-64 émis par `emettre()` dans
// un ELF64 ET_EXEC STATIQUE autonome, avec un `_start` écrit en dur :
//
//   _start :
//     pour chaque (slot, valeur) : movabs rax,valeur ; mov [base+8*slot],rax
//     mov rdi, base        ; rdi = pointeur des slots (signature machine)
//     mov rsi, 0           ; rsi = trampoline (NUL : aucun AppelRir/fallback)
//     call code_emis       ; i64 f(base, tramp)
//     cmp rax, attendu
//     je  ok
//     mov edi, 255 ; mov eax,60 ; syscall     ← désaccord (255)
//   ok : mov rdi, rax ; mov eax,60 ; syscall   ← exit(resultat)
//
// ⚠ Le sous-ensemble émettable ne contient PAS `afficher` : le résultat est
//   observable par le CODE DE SORTIE du processus (42 / 55), lisible par
//   `echo $?` après qemu. Aucun runtime, aucune libc, aucun linker externe.
// ⚠ Le bit exécutable est posé explicitement : qemu-user refuse un ELF non
//   exécutable (mesuré : exit 1 silencieux avant chmod +x).

/// Adresse virtuelle de base de l'ELF autonome.
pub const ELF_BASE: u64 = 0x40_0000;
const ELF_OFF_START: usize = 0x100;
const ELF_OFF_SLOTS: usize = 0x1000;
const ELF_TAILLE: usize = 0x2000;

/// Écrit un ELF64 x86-64 ET_EXEC statique qui appelle `code` (émis par
/// `emettre(..., Architecture::X86_64)`) avec les slots initiaux donnés.
///
/// `slots_initiaux` : paires `(index_slot, valeur_i64)` à poser avant l'appel.
/// `attendu` : valeur de retour attendue ; si `rax != attendu`, le programme
/// sort avec 255 au lieu du résultat (détection d'un écart sans ambiguïté).
#[cfg(unix)]
pub fn ecrire_elf_x86_64_autonome(
    code: &[u8],
    slots_initiaux: &[(u16, i64)],
    attendu: i64,
    chemin: &str,
) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut shim: Vec<u8> = Vec::with_capacity(128 + code.len());
    // Initialisation des slots (adressage absolu : 48 A3 = mov [imm64], rax).
    for &(idx, val) in slots_initiaux {
        shim.extend_from_slice(&[0x48, 0xB8]);                 // movabs rax, val
        shim.extend_from_slice(&val.to_le_bytes());
        shim.extend_from_slice(&[0x48, 0xA3]);                 // mov [imm64], rax
        let addr = ELF_BASE + ELF_OFF_SLOTS as u64 + 8 * idx as u64;
        shim.extend_from_slice(&addr.to_le_bytes());
    }
    shim.extend_from_slice(&[0x48, 0xBF]);                     // mov rdi, &slots
    shim.extend_from_slice(&(ELF_BASE + ELF_OFF_SLOTS as u64).to_le_bytes());
    shim.extend_from_slice(&[0x48, 0xBE, 0, 0, 0, 0, 0, 0, 0, 0]); // mov rsi, 0
    let call_pos = shim.len();
    shim.extend_from_slice(&[0xE8, 0, 0, 0, 0]);               // call code (patché)
    shim.extend_from_slice(&[0x48, 0x3D]);                     // cmp rax, attendu
    shim.extend_from_slice(&(attendu as i32).to_le_bytes());
    let je_pos = shim.len();
    shim.extend_from_slice(&[0x74, 0]);                        // je ok (patché)
    shim.extend_from_slice(&[0xBF, 255, 0, 0, 0]);             // mov edi, 255
    shim.extend_from_slice(&[0xB8, 60, 0, 0, 0]);              // mov eax, 60
    shim.extend_from_slice(&[0x0F, 0x05]);                     // syscall
    let ok_pos = shim.len();
    shim[je_pos + 1] = (ok_pos - je_pos - 2) as u8;
    shim.extend_from_slice(&[0x48, 0x89, 0xC7]);               // mov rdi, rax
    shim.extend_from_slice(&[0xB8, 60, 0, 0, 0]);              // mov eax, 60
    shim.extend_from_slice(&[0x0F, 0x05]);                     // syscall

    let user_off = ELF_OFF_START + shim.len();
    let disp = (user_off - (ELF_OFF_START + call_pos + 5)) as i32;
    shim[call_pos + 1..call_pos + 5].copy_from_slice(&disp.to_le_bytes());

    let filesz = user_off + code.len();
    let mut blob = vec![0u8; ELF_TAILLE];
    // Elf64_Ehdr
    blob[0..16].copy_from_slice(&[0x7F, b'E', b'L', b'F', 2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    fn w16(b: &mut [u8], o: usize, v: u16) { b[o..o + 2].copy_from_slice(&v.to_le_bytes()); }
    fn w32(b: &mut [u8], o: usize, v: u32) { b[o..o + 4].copy_from_slice(&v.to_le_bytes()); }
    fn w64(b: &mut [u8], o: usize, v: u64) { b[o..o + 8].copy_from_slice(&v.to_le_bytes()); }
    w16(&mut blob, 16, 2);    // ET_EXEC
    w16(&mut blob, 18, 62);   // EM_X86_64
    w32(&mut blob, 20, 1);    // EV_CURRENT
    w64(&mut blob, 24, ELF_BASE + ELF_OFF_START as u64); // e_entry
    w64(&mut blob, 32, 64);   // e_phoff
    w64(&mut blob, 40, 0);    // e_shoff
    w32(&mut blob, 48, 0);    // e_flags
    w16(&mut blob, 52, 64);   // e_ehsize
    w16(&mut blob, 54, 56);   // e_phentsize
    w16(&mut blob, 56, 1);    // e_phnum
    // Elf64_Phdr : un seul PT_LOAD RWX couvrant tout le fichier.
    w32(&mut blob, 64, 1);    // PT_LOAD
    w32(&mut blob, 68, 7);    // PF_R|PF_W|PF_X
    w64(&mut blob, 72, 0);    // p_offset
    w64(&mut blob, 80, ELF_BASE); // p_vaddr
    w64(&mut blob, 88, ELF_BASE); // p_paddr
    w64(&mut blob, 96, filesz as u64); // p_filesz
    w64(&mut blob, 104, ELF_TAILLE as u64); // p_memsz
    w64(&mut blob, 112, 0x1000); // p_align
    blob[ELF_OFF_START..user_off].copy_from_slice(&shim);
    blob[user_off..filesz].copy_from_slice(code);
    std::fs::write(chemin, &blob)?;
    std::fs::set_permissions(chemin, std::fs::Permissions::from_mode(0o755))?;
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// CONSTRUCTEUR INTERNE
// ═══════════════════════════════════════════════════════════════════════

enum CorrectifSaut {
    /// `b imm26` (aarch64) : 4 octets de déplacement relatif au PC de l'insn.
    A64B { insn: usize, cible: usize },
    /// `b.cond imm19` (aarch64).
    A64BCond { insn: usize, cible: usize },
    /// `e9 rel32` (x86-64) : déplacement relatif à la FIN de l'instruction.
    X86Jmp { insn: usize, cible: usize },
    /// `0f 85 rel32` (x86-64).
    X86Jcc { insn: usize, cible: usize },
}

enum CorrectifChaine {
    /// `adr x4, imm` (aarch64) : champs immlo[29:29], immhi[23:5].
    A64 { insn: usize, chaine: usize },
    /// `lea r8, [rip+rel32]` (x86-64) : rel32 à insn+3.
    X86 { insn: usize, chaine: usize },
}

/// Mode de résolution des `AppelRir` : trampoline (JIT / `emettre`) ou
/// appel direct vers une fonction du même segment (AOT / `emettre_programme`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModeAppel { Trampoline, Direct }

struct Builder<'a> {
    arch: Architecture,
    corps: &'a CorpsRir,
    code: Vec<u8>,
    sauts: Vec<CorrectifSaut>,
    chaines_ref: Vec<CorrectifChaine>,
    noms: Vec<String>,
    appels: Vec<Appel>,
    fallbacks: Vec<u32>,
    scratch: u16,
    mode: ModeAppel,
    /// Table des fonctions du programme (pour résoudre les appels directs).
    table: &'a [String],
    /// Taille d'une trame de slots, identique pour toutes les fonctions.
    frame_bytes: usize,
    /// Appels directs émis, avec la position de leur champ à corriger.
    appels_lies: Vec<AppelLie>,
    /// AOT6 : types des slots (0 = entier, 1 = flottant, 2 = bigint,
    /// 3 = chaîne, 4 = booléen). Utilisé pour choisir les instructions IEEE.
    types: Vec<u8>,
    /// AOT6 : le runtime natif est disponible dans le segment.
    avec_runtime: bool,
    /// Registres « second » qui portent encore un opérande vivant et qui
    /// seraient détruits par un appel (caller-saved). Ils sont sauvés dans
    /// des slots scratch autour de chaque `appel_direct`.
    secs_vifs: Vec<u32>,
}

// Registres aarch64.
const A64_BASE: u32 = 19;
const A64_TRAMP: u32 = 20;
const A64_VAL: u32 = 0; // ⚠ x0 = registre de RETOUR AAPCS64 (pas x8 !)
const A64_SEC: u32 = 9;
const A64_NOM: u32 = 4;

// Registres x86-64 (encodage SysV).
// rbx = base et r12 = tramp sont encodés en dur dans x86_mem/x86_load_tramp.
const X86_VAL: u8 = 0;   // rax
const X86_SEC: u8 = 1;   // rcx

impl<'a> Builder<'a> {
    fn new(arch: Architecture, corps: &'a CorpsRir) -> Self {
        Builder {
            arch, corps,
            code: Vec::with_capacity(256),
            sauts: Vec::new(), chaines_ref: Vec::new(), noms: Vec::new(),
            appels: Vec::new(), fallbacks: Vec::new(), scratch: 0,
            mode: ModeAppel::Trampoline, table: &[], frame_bytes: 0,
            appels_lies: Vec::new(), types: Vec::new(), avec_runtime: false,
            secs_vifs: Vec::new(),
        }
    }

    /// Constructeur pour l'AOT : appels directs, aucune chaîne de nom, aucun
    /// fallback (une expression qui demanderait la trampoline est refusée).
    fn new_lie(arch: Architecture, corps: &'a CorpsRir, table: &'a [String],
               frame_bytes: usize) -> Self {
        let mut b = Self::new(arch, corps);
        b.mode = ModeAppel::Direct;
        b.table = table;
        b.frame_bytes = frame_bytes;
        b
    }

    /// Registre de valeur de l'architecture (x8 sur aarch64, rax sur x86-64).
    fn val_reg(&self) -> u32 {
        match self.arch { Architecture::Aarch64 => A64_VAL, Architecture::X86_64 => X86_VAL as u32 }
    }

    fn fresh(&mut self) -> Result<u16, EmitError> {
        if self.scratch >= SCRATCH_SLOTS { return Err(EmitError::TropDeScratch); }
        let s = self.corps.n_slots as u16 + self.scratch;
        self.scratch += 1;
        Ok(s)
    }

    /// AOT6 : note le type d'un slot (pour les opérations flottantes).
    fn set_type(&mut self, slot: u16, t: u8) {
        let i = slot as usize;
        if self.types.len() <= i { self.types.resize(i + 1, 0); }
        self.types[i] = t;
    }

    /// AOT6 : appel direct à une routine du runtime natif. Le registre de
    /// base (x0 / rdi) doit déjà pointer vers ses arguments ; aucune trame
    /// supplémentaire n'est nécessaire (la routine gère la sienne).
    fn appel_runtime_par_nom(&mut self, nom: &str) -> Result<(), EmitError> {
        let cible = self.table.iter().position(|n| n == nom)
            .ok_or_else(|| EmitError::FonctionInconnue(nom.to_string()))?;
        match self.arch {
            Architecture::Aarch64 => {
                let correctif = self.code.len();
                self.u32(0x9400_0000); // bl (patché)
                self.appels_lies.push(AppelLie { nom: nom.to_string(), cible, correctif });
            }
            Architecture::X86_64 => {
                let insn = self.code.len();
                self.octets(&[0xE8, 0, 0, 0, 0]); // call rel32 (patché)
                self.appels_lies.push(AppelLie {
                    nom: nom.to_string(), cible, correctif: insn + 1,
                });
            }
        }
        Ok(())
    }

    fn u32(&mut self, v: u32) { self.code.extend_from_slice(&v.to_le_bytes()); }
    fn octets(&mut self, v: &[u8]) { self.code.extend_from_slice(v); }

    /// `mov dst, src` entre registres entiers (x0/x9 ou rax/rcx).
    /// Évite un aller-retour par les slots quand une sous-expression est
    /// directement disponible dans le registre de valeur.
    fn mov_reg(&mut self, dst: u32, src: u32) {
        match self.arch {
            Architecture::Aarch64 => {
                // ORR dst, xzr, src
                self.u32(0xAA000000 | (src << 16) | (31 << 5) | dst);
            }
            Architecture::X86_64 => {
                // mov r/m64, r64 : REX.W + REX.R(src>7) + REX.B(dst>7).
                let s = src as u8; let d = dst as u8;
                self.octets(&[0x48 | ((s >> 3) << 2) | (d >> 3), 0x89,
                              0xC0 | ((s & 7) << 3) | (d & 7)]);
            }
        }
    }

    // ── prologue / épilogue ────────────────────────────────────────────
    fn prologue(&mut self) {
        match self.arch {
            Architecture::Aarch64 => {
                // stp x29,x30,[sp,#-16]! ; mov x29,sp
                self.u32(0xA9BF7BFD); self.u32(0x910003FD);
                // stp x19,x20,[sp,#-16]! ; mov x19,x0 ; mov x20,x1
                self.u32(0xA9BF53F3);
                self.u32(0xAA0003F3); // mov x19, x0 (ORR x19, xzr, x0)
                self.u32(0xAA0103F4); // mov x20, x1 (ORR x20, xzr, x1)
            }
            Architecture::X86_64 => {
                self.octets(&[0x55]);             // push rbp
                self.octets(&[0x48, 0x89, 0xe5]); // mov rbp, rsp
                self.octets(&[0x53]);             // push rbx
                self.octets(&[0x41, 0x54]);       // push r12
                self.octets(&[0x48, 0x89, 0xfb]); // mov rbx, rdi (REX.W sans B)
                self.octets(&[0x49, 0x89, 0xf4]); // mov r12, rsi
            }
        }
    }

    fn epilogue(&mut self) {
        match self.arch {
            Architecture::Aarch64 => {
                self.u32(0xA8C153F3); // ldp x19,x20,[sp],#16
                self.u32(0xA8C17BFD); // ldp x29,x30,[sp],#16
                self.u32(0xD65F03C0); // ret
            }
            Architecture::X86_64 => {
                self.octets(&[0x41, 0x5c]); // pop r12
                self.octets(&[0x5b]);       // pop rbx
                self.octets(&[0x5d]);       // pop rbp
                self.octets(&[0xc3]);       // ret
            }
        }
    }

    // ── mémoire ────────────────────────────────────────────────────────
    fn load_slot(&mut self, dreg: u32, slot: u16) {
        let off = slot as usize * 8;
        match self.arch {
            Architecture::Aarch64 => {
                assert!(off < 32768);
                self.u32(0xF9400000 | (((off / 8) as u32) << 10)
                         | (A64_BASE << 5) | dreg);
            }
            Architecture::X86_64 => {
                self.x86_mem(0x8B, dreg as u8, off);
            }
        }
    }

    fn store_slot(&mut self, sreg: u32, slot: u16) {
        let off = slot as usize * 8;
        match self.arch {
            Architecture::Aarch64 => {
                assert!(off < 32768);
                self.u32(0xF9000000 | (((off / 8) as u32) << 10)
                         | (A64_BASE << 5) | sreg);
            }
            Architecture::X86_64 => {
                self.x86_mem(0x89, sreg as u8, off);
            }
        }
    }

    /// `mov r64, [rbx+off]` (opcode 0x8B) ou `mov [rbx+off], r64` (0x89).
    fn x86_mem(&mut self, opcode: u8, reg: u8, off: usize) {
        // REX.W(+R si reg >= 8) ; modrm(mod=01, reg, rm=011=rbx) ; disp8
        // Pour disp8 >= 128, passer en disp32 (mod=10).
        self.octets(&[0x48 | ((reg >> 3) << 2)]);
        let r = reg & 7;
        if off <= 127 {
            self.octets(&[opcode, 0x43 | (r << 3), off as u8]);
        } else {
            self.octets(&[opcode, 0x83 | (r << 3)]);
            self.octets(&(off as u32).to_le_bytes());
        }
    }

    /// `mov dst, [r12+off]` (r12 exige un octet SIB).
    fn x86_load_tramp(&mut self, dst: u8, off: u8) {
        self.octets(&[0x49, 0x8B, 0x44 | (dst << 3), 0x24, off]);
    }

    // ── littéraux ──────────────────────────────────────────────────────
    fn mov_imm(&mut self, dreg: u32, v: i64) {
        match self.arch {
            Architecture::Aarch64 => {
                let bits = v as u64;
                // movz du premier mot (toujours émis), puis movk des suivants.
                self.u32(0xD2800000 | (((bits & 0xFFFF) as u32) << 5) | dreg);
                for hw in 1..4u32 {
                    let chunk = ((bits >> (16 * hw)) & 0xFFFF) as u32;
                    if chunk != 0 {
                        self.u32(0xF2800000 | (hw << 21) | (chunk << 5) | dreg);
                    }
                }
            }
            Architecture::X86_64 => {
                let bits = v as u64;
                let rex = 0x48 | if dreg >= 8 { 0x01 } else { 0 };
                let s = v;
                if s >= i32::MIN as i64 && s <= i32::MAX as i64 {
                    self.octets(&[rex, 0xC7, 0xC0 | ((dreg as u8) & 7)]);
                    self.octets(&(s as i32 as u32).to_le_bytes());
                } else {
                    self.octets(&[rex, 0xB8 | ((dreg as u8) & 7)]);
                    self.octets(&bits.to_le_bytes());
                }
            }
        }
    }

    // ── ALU (résultat dans le registre de valeur) ──────────────────────
    /// Calcule `val ← val OP sec` (aarch64 : x0/x9 ; x86 : rax/rcx).
    /// `sec` est passé en paramètre pour permettre l'évaluation sans slot
    /// des sous-expressions imbriquées (chaque niveau utilise son registre).
    fn alu(&mut self, op: Oper, sec: u32) {
        match self.arch {
            Architecture::Aarch64 => {
                match op {
                    Oper::Add => self.u32(0x8B000000 | (sec << 16) | (A64_VAL << 5) | A64_VAL),
                    Oper::Sub => self.u32(0xCB000000 | (sec << 16) | (A64_VAL << 5) | A64_VAL),
                    Oper::Mul => self.u32(0x9B007C00 | (sec << 16) | (A64_VAL << 5) | A64_VAL),
                    Oper::Lt | Oper::Le | Oper::Gt | Oper::Ge | Oper::Eq | Oper::Ne => {
                        // cmp val, sec ; cset val, cond
                        self.u32(0xEB000000 | (sec << 16) | (A64_VAL << 5) | 31);
                        let cond = match op {
                            Oper::Lt => 11, Oper::Le => 13, Oper::Gt => 12,
                            Oper::Ge => 10, Oper::Eq => 0, Oper::Ne => 1, _ => unreachable!(),
                        };
                        self.u32(0x9A9F07E0 | (((cond ^ 1) as u32) << 12) | A64_VAL);
                    }
                    _ => unreachable!("alu: op non natif"),
                }
            }
            Architecture::X86_64 => {
                let s = sec as u8;
                // ModRM(mode=11, reg=s, rm=rax) + REX.W/R/B pour les
                // registres étendus (rcx, rdx, r8-r11 selon la profondeur).
                let rex_rm = 0x48 | ((s >> 3) << 2);
                let modrm_rm_rax = 0xC0 | ((s & 7) << 3);
                match op {
                    Oper::Add => self.octets(&[rex_rm, 0x01, modrm_rm_rax]), // add rax, s
                    Oper::Sub => self.octets(&[rex_rm, 0x29, modrm_rm_rax]), // sub rax, s
                    Oper::Mul => self.octets(&[
                        0x48 | (s >> 3), 0x0F, 0xAF,
                        0xC0 | (s & 7),                                  // imul rax, s
                    ]),
                    Oper::Lt | Oper::Le | Oper::Gt | Oper::Ge | Oper::Eq | Oper::Ne => {
                        self.octets(&[rex_rm, 0x39, modrm_rm_rax]); // cmp rax, s
                        let setcc: u8 = match op {
                            Oper::Lt => 0x9c, Oper::Le => 0x9e, Oper::Gt => 0x9f,
                            Oper::Ge => 0x9d, Oper::Eq => 0x94, Oper::Ne => 0x95,
                            _ => unreachable!(),
                        };
                        self.octets(&[0x0F, setcc, 0xC0]); // setcc al
                        self.octets(&[0x0F, 0xB6, 0xC0]); // movzx eax, al
                    }
                    _ => unreachable!("alu: op non natif"),
                }
            }
        }
    }

    fn neg_val(&mut self) {
        match self.arch {
            Architecture::Aarch64 => self.u32(0xCB000000 | (A64_VAL << 16) | (31 << 5) | A64_VAL),
            Architecture::X86_64 => self.octets(&[0x48, 0xF7, 0xD8]), // neg rax
        }
    }

    fn non_val(&mut self) {
        match self.arch {
            Architecture::Aarch64 => {
                // cmp val, #0 ; cset val, eq
                self.u32(0xF1000000 | (A64_VAL << 5) | 31); // cmp x8, #0
                self.u32(0x9A9F07E0 | ((0 ^ 1) << 12) | A64_VAL); // cset eq
            }
            Architecture::X86_64 => {
                self.octets(&[0x48, 0x85, 0xC0]); // test rax, rax
                self.octets(&[0x0F, 0x94, 0xC0]); // sete al
                self.octets(&[0x0F, 0xB6, 0xC0]); // movzx eax, al
            }
        }
    }

    // ── AOT6 : virgule flottante (bits IEEE dans les slots i64) ────────

    /// Type AOT6 d'une expression : 1 = flottant, 2 = bigint,
    /// 3 = chaîne, 4 = booléen, 0 = entier/inconnu.
    fn type_of(&self, e: &Eabstrait) -> u8 {
        match e {
            Eabstrait::Flottant(_) => 1,
            Eabstrait::Entier(_) => 0,
            Eabstrait::Slot(s, _) => self.types.get(*s as usize).copied().unwrap_or(0),
            Eabstrait::Binaire { op, a, b } => {
                if matches!(op, Oper::Lt | Oper::Le | Oper::Gt | Oper::Ge | Oper::Eq | Oper::Ne) {
                    4
                } else if matches!(op, Oper::Puissance) && self.type_of(a) == 0 && self.type_of(b) == 0 {
                    2
                } else {
                    let ta = self.type_of(a); let tb = self.type_of(b);
                    if ta == 1 || tb == 1 { 1 } else { 0 }
                }
            }
            Eabstrait::Neg(a) => self.type_of(a),
            Eabstrait::Non(_) => 4,
            Eabstrait::AppelRir { nom, .. } if nom.starts_with("$rt_") => {
                if nom == "$rt_lire_texte" { 3 } else { 0 }
            }
            Eabstrait::AppelRir { .. } => 0,
        }
    }

    /// Charge un f64 du slot dans un registre FP (idx 0 → d0/xmm0, 1 → d9/xmm1).
    fn float_load(&mut self, idx: u32, slot: u16) {
        let off = slot as u32 * 8; // octets (x86)
        match self.arch {
            Architecture::Aarch64 => {
                let r = if idx == 0 { 0u32 } else { 9u32 };
                // aarch64 : imm12 est en unités de 8 octets.
                self.u32(0xFD400000 | ((slot as u32) << 10) | (A64_BASE << 5) | r);
            }
            Architecture::X86_64 => {
                let reg = if idx == 0 { 0u8 } else { 1u8 };
                if off <= 127 {
                    self.octets(&[0xF2, 0x0F, 0x10, 0x43 | (reg << 3), off as u8]);
                } else {
                    self.octets(&[0xF2, 0x0F, 0x10, 0x83 | (reg << 3)]);
                    self.octets(&off.to_le_bytes());
                }
            }
        }
    }

    /// Stocke le f64 du registre FP (idx 0 → d0/xmm0) dans un slot.
    fn float_store(&mut self, idx: u32, slot: u16) {
        let off = slot as u32 * 8; // octets (x86)
        match self.arch {
            Architecture::Aarch64 => {
                let r = if idx == 0 { 0u32 } else { 9u32 };
                self.u32(0xFD000000 | ((slot as u32) << 10) | (A64_BASE << 5) | r);
            }
            Architecture::X86_64 => {
                let reg = if idx == 0 { 0u8 } else { 1u8 };
                if off <= 127 {
                    self.octets(&[0xF2, 0x0F, 0x11, 0x43 | (reg << 3), off as u8]);
                } else {
                    self.octets(&[0xF2, 0x0F, 0x11, 0x83 | (reg << 3)]);
                    self.octets(&off.to_le_bytes());
                }
            }
        }
    }

    /// `d0 ← d0 OP d9` (aarch64) / `xmm0 ← xmm0 OP xmm1` (x86-64).
    fn float_alu(&mut self, op: Oper) {
        match self.arch {
            Architecture::Aarch64 => {
                match op {
                    Oper::Add => self.u32(0x1E602800 | (9 << 16)),
                    Oper::Sub => self.u32(0x1E603800 | (9 << 16)),
                    Oper::Mul => self.u32(0x1E600800 | (9 << 16)),
                    Oper::Div => self.u32(0x1E601800 | (9 << 16)),
                    Oper::Lt | Oper::Le | Oper::Gt | Oper::Ge | Oper::Eq | Oper::Ne => {
                        self.u32(0x1E602000 | (9 << 16)); // fcmp d0, d9
                        let cond = match op {
                            Oper::Lt => 11, Oper::Le => 13, Oper::Gt => 12,
                            Oper::Ge => 10, Oper::Eq => 0, Oper::Ne => 1, _ => unreachable!(),
                        };
                        self.u32(0x9A9F07E0 | (((cond ^ 1) as u32) << 12) | A64_VAL);
                    }
                    _ => unreachable!("float_alu: op non flottant"),
                }
            }
            Architecture::X86_64 => {
                match op {
                    Oper::Add => self.octets(&[0xF2, 0x0F, 0x58, 0xC1]),
                    Oper::Sub => self.octets(&[0xF2, 0x0F, 0x5C, 0xC1]),
                    Oper::Mul => self.octets(&[0xF2, 0x0F, 0x59, 0xC1]),
                    Oper::Div => self.octets(&[0xF2, 0x0F, 0x5E, 0xC1]),
                    Oper::Lt | Oper::Le | Oper::Gt | Oper::Ge | Oper::Eq | Oper::Ne => {
                        self.octets(&[0xF2, 0x0F, 0x2E, 0xC1]); // ucomisd xmm0,xmm1
                        let setcc: u8 = match op {
                            Oper::Lt => 0x92, Oper::Le => 0x96, Oper::Gt => 0x97,
                            Oper::Ge => 0x93, Oper::Eq => 0x94, Oper::Ne => 0x95,
                            _ => unreachable!(),
                        };
                        self.octets(&[0x0F, setcc, 0xC0]);
                        self.octets(&[0x0F, 0xB6, 0xC0]);
                    }
                    _ => unreachable!("float_alu: op non flottant"),
                }
            }
        }
    }

    // ── sauts ──────────────────────────────────────────────────────────
    fn saut(&mut self, cible: usize) {
        if cible > self.corps.ops.len() { return; } // validé plus haut
        match self.arch {
            Architecture::Aarch64 => {
                let insn = self.code.len();
                self.u32(0x14000000); // b (patché)
                self.sauts.push(CorrectifSaut::A64B { insn, cible });
            }
            Architecture::X86_64 => {
                let insn = self.code.len();
                self.octets(&[0xE9, 0, 0, 0, 0]); // jmp rel32 (patché)
                self.sauts.push(CorrectifSaut::X86Jmp { insn, cible });
            }
        }
    }

    /// Saute vers `cible` si la valeur dans `slot` n'est PAS exactement 1.
    fn saut_si_faux(&mut self, slot: u16, cible: usize) {
        self.load_slot(self.val_reg(), slot); // charge dans x8 / rax
        match self.arch {
            Architecture::Aarch64 => {
                // cmp x8, #1 ; b.ne cible
                self.u32(0xF1000000 | (1 << 10) | (A64_VAL << 5) | 31);
                let insn = self.code.len();
                self.u32(0x54000000 | 1); // b.ne (patché)
                self.sauts.push(CorrectifSaut::A64BCond { insn, cible });
            }
            Architecture::X86_64 => {
                self.octets(&[0x48, 0x83, 0xF8, 0x01]); // cmp rax, 1
                let insn = self.code.len();
                self.octets(&[0x0F, 0x85, 0, 0, 0, 0]); // jne rel32 (patché)
                self.sauts.push(CorrectifSaut::X86Jcc { insn, cible });
            }
        }
    }

    /// Saute si la valeur DÉJÀ dans x0/rax n'est pas exactement 1 (évite le
    /// `ldr` de `saut_si_faux` quand l'évaluation vient de poser la valeur).
    fn saut_si_faux_val(&mut self, cible: usize) {
        if cible > self.corps.ops.len() { return; }
        match self.arch {
            Architecture::Aarch64 => {
                self.u32(0xF1000000 | (1 << 10) | (A64_VAL << 5) | 31);
                let insn = self.code.len();
                self.u32(0x54000000 | 1);
                self.sauts.push(CorrectifSaut::A64BCond { insn, cible });
            }
            Architecture::X86_64 => {
                self.octets(&[0x48, 0x83, 0xF8, 0x01]);
                let insn = self.code.len();
                self.octets(&[0x0F, 0x85, 0, 0, 0, 0]);
                self.sauts.push(CorrectifSaut::X86Jcc { insn, cible });
            }
        }
    }

    fn corriger_sauts(&mut self, op_offsets: &[usize]) {
        let sauts = std::mem::take(&mut self.sauts);
        for c in sauts {
            let (_insn, cible, patch): (usize, usize, Box<dyn Fn(&mut Vec<u8>, usize)>) = match c {
                CorrectifSaut::A64B { insn, cible } => (insn, cible, Box::new(move |code, tgt| {
                    let disp = (tgt as i64 - insn as i64) / 4;
                    let v = 0x14000000u32 | ((disp as u32) & 0x03FF_FFFF);
                    code[insn..insn + 4].copy_from_slice(&v.to_le_bytes());
                })),
                CorrectifSaut::A64BCond { insn, cible } => (insn, cible, Box::new(move |code, tgt| {
                    let disp = (tgt as i64 - insn as i64) / 4;
                    let v = 0x54000001u32 | (((disp as u32) & 0x7FFFF) << 5);
                    code[insn..insn + 4].copy_from_slice(&v.to_le_bytes());
                })),
                CorrectifSaut::X86Jmp { insn, cible } => (insn, cible, Box::new(move |code, tgt| {
                    let disp = tgt as i64 - (insn as i64 + 5);
                    code[insn + 1..insn + 5].copy_from_slice(&(disp as i32).to_le_bytes());
                })),
                CorrectifSaut::X86Jcc { insn, cible } => (insn, cible, Box::new(move |code, tgt| {
                    let disp = tgt as i64 - (insn as i64 + 6);
                    code[insn + 2..insn + 6].copy_from_slice(&(disp as i32).to_le_bytes());
                })),
            };
            let cible_octet = op_offsets.get(cible).copied().unwrap_or(*op_offsets.last().unwrap());
            patch(&mut self.code, cible_octet);
        }
    }

    // ── chaînes / appel runtime ────────────────────────────────────────
    fn reference_chaine(&mut self, nom: &str) -> usize {
        if let Some(i) = self.noms.iter().position(|n| n == nom) { return i; }
        self.noms.push(nom.to_string());
        self.noms.len() - 1
    }

    /// `call tramp.appel_rir` avec 4 arguments i64 + pointeur de nom.
    fn appel_runtime(&mut self, args: &[u16; 4], nom: &str) {
        let nom_idx = self.reference_chaine(nom);
        match self.arch {
            Architecture::Aarch64 => {
                // x0..x3 = arguments, x4 = nom (adr patché), x9 = fn ptr
                for (i, &s) in args.iter().enumerate() {
                    self.load_slot(i as u32, s); // x0..x3
                }
                let insn = self.code.len();
                self.u32(0x10000000 | (A64_NOM)); // adr x4, 0 (patché)
                self.chaines_ref.push(CorrectifChaine::A64 { insn, chaine: nom_idx });
                self.u32(0xF9400000 | ((20) << 5) | 9); // ldr x9,[x20,#0]
                self.u32(0xD63F0000 | (9 << 5));        // blr x9
            }
            Architecture::X86_64 => {
                // rdi,rsi,rdx,rcx = arguments ; r8 = nom ; rax = fn ptr
                const ARG_REGS: [u8; 4] = [7, 6, 2, 1];
                for (i, &s) in args.iter().enumerate() {
                    self.x86_mem(0x8B, ARG_REGS[i], s as usize * 8);
                }
                let insn = self.code.len();
                self.octets(&[0x4C, 0x8D, 0x05, 0, 0, 0, 0]); // lea r8,[rip+0] (patché)
                self.chaines_ref.push(CorrectifChaine::X86 { insn, chaine: nom_idx });
                self.x86_load_tramp(X86_VAL, 0); // mov rax,[r12]
                self.octets(&[0xFF, 0xD0]);       // call rax
            }
        }
    }

    /// `mov [rsp + off], rax` (SIB avec base rsp).
    fn x86_store_rsp(&mut self, reg: u8, off: usize) {
        if off <= 127 {
            self.octets(&[0x48, 0x89, 0x44 | (reg << 3), 0x24, off as u8]);
        } else {
            self.octets(&[0x48, 0x89, 0x84 | (reg << 3), 0x24]);
            self.octets(&(off as u32).to_le_bytes());
        }
    }

    /// Appel DIRECT à une autre fonction du même segment (mode AOT).
    ///
    /// La trame de l'appelé est allouée SOUS `rsp`/`sp`. La valeur de retour
    /// reste dans x0/rax, exactement comme pour un appel via trampoline.
    fn appel_direct(&mut self, args: &[u16; 4], n_args: usize, nom: &str) -> Result<(), EmitError> {
        let cible = self.table.iter().position(|n| n == nom)
            .ok_or_else(|| EmitError::FonctionInconnue(nom.to_string()))?;
        let f = self.frame_bytes;
        // Un opérande gauche peut être vivant dans un registre « second »
        // (caller-saved) : l'appelé l'écraserait. On le met à l'abri dans
        // un slot scratch du FRAME DE L'APPELANT, puis on le recharge.
        let mut secs_sauves: Vec<(u32, u16)> = Vec::new();
        for i in 0..self.secs_vifs.len() {
            let r = self.secs_vifs[i];
            let s = self.fresh()?;
            self.store_slot(r, s);
            secs_sauves.push((r, s));
        }
        match self.arch {
            Architecture::Aarch64 => {
                // ⚠ On charge les arguments AVANT de bouger sp (x0..x3), puis
                // on réserve la trame et on les écrit dans la trame enfant.
                for (i, &s) in args.iter().enumerate().take(n_args) {
                    self.load_slot(i as u32, s);
                }
                // ⚠ Forme « extended register » (bit 21 + option=3) :
                // c'est la seule qui puisse encoder SP comme Rn/Rd.
                // x10 est caller-saved et N'EST PAS x9 (= registre second
                // de l'évaluateur) : l'appel ne clobber donc pas l'opérande
                // gauche déjà chargée dans x9. On le RECHARGE après le `bl`.
                self.mov_imm(10, f as i64);
                self.u32(0xCB000000 | (1 << 21) | (10 << 16) | (3 << 13)
                         | (31 << 5) | 31); // sub sp, sp, x10
                for i in 0..n_args {
                    // str x{i}, [sp, #8*i]  (imm12 = déplacement / 8 = i)
                    self.u32(0xF9000000 | ((i as u32) << 10) | (31 << 5) | i as u32);
                }
                self.u32(0x910003E0); // mov x0, sp  (base de l'appelé)
                self.u32(0xAA1F03E1); // mov x1, xzr (trampoline inutilisée)
                let correctif = self.code.len();
                self.u32(0x9400_0000); // bl (patché par emettre_programme)
                self.mov_imm(10, f as i64); // rechargé : l'appelé clobber x10
                self.u32(0x8B000000 | (1 << 21) | (10 << 16) | (3 << 13)
                         | (31 << 5) | 31); // add sp, sp, x10
                for (r, s) in &secs_sauves { self.load_slot(*r, *s); }
                self.appels_lies.push(AppelLie { nom: nom.to_string(), cible, correctif });
            }
            Architecture::X86_64 => {
                self.octets(&[0x48, 0x81, 0xEC]); // sub rsp, imm32
                self.octets(&(f as u32).to_le_bytes());
                for (i, &s) in args.iter().enumerate().take(n_args) {
                    self.x86_mem(0x8B, X86_VAL, s as usize * 8); // mov rax,[rbx+slot]
                    self.x86_store_rsp(X86_VAL, i * 8);          // mov [rsp+i*8],rax
                }
                self.octets(&[0x48, 0x8D, 0x3C, 0x24]); // lea rdi,[rsp] = base
                self.octets(&[0x31, 0xF6]);             // xor esi,esi = tramp
                let insn = self.code.len();
                self.octets(&[0xE8, 0, 0, 0, 0]);       // call rel32 (patché)
                self.octets(&[0x48, 0x81, 0xC4]);       // add rsp, imm32
                self.octets(&(f as u32).to_le_bytes());
                for (r, s) in &secs_sauves { self.load_slot(*r, *s); }
                self.appels_lies.push(AppelLie {
                    nom: nom.to_string(), cible, correctif: insn + 1,
                });
            }
        }
        Ok(())
    }

    /// Refuse (en mode AOT) une expression qui demanderait la trampoline.
    fn verifier_sans_fallback(&self, e: &Eabstrait) -> Result<(), EmitError> {
        if self.mode == ModeAppel::Trampoline { return Ok(()); }
        Err(EmitError::OperationNonSupportee(format!(
            "expression non-native dans un appel lié : {:?}", e)))
    }

    /// `call tramp.eval_fallback(ctx, expr_id, base)`.
    fn fallback_runtime(&mut self, expr: &Eabstrait) -> Result<(), EmitError> {
        let id = self.fallbacks.len() as u32;
        self.fallbacks.push(id);
        let _ = expr; // l'hôte reconstruit la table dans le même ordre
        match self.arch {
            Architecture::Aarch64 => {
                self.u32(0xF9400000 | (2u32 << 10) | (A64_TRAMP << 5) | 0); // ldr x0,[x20,#16] = ctx
                self.u32(0xD2800000 | ((id as u32) << 5) | 1);              // mov x1,#id
                self.u32(0xAA000000 | (A64_BASE << 16) | (31 << 5) | 2);    // mov x2,x19 = base
                self.u32(0xF9400000 | (1u32 << 10) | (A64_TRAMP << 5) | 9); // ldr x9,[x20,#8]
                self.u32(0xD63F0000 | (9 << 5));                            // blr x9
            }
            Architecture::X86_64 => {
                self.x86_load_tramp(7, 16);                 // mov rdi,[r12+16] = ctx
                self.octets(&[0x48, 0xC7, 0xC6]);           // mov rsi, imm32
                self.octets(&(id as u32).to_le_bytes());
                self.octets(&[0x48, 0x89, 0xDA]);           // mov rdx, rbx = base
                self.x86_load_tramp(X86_VAL, 8);            // mov rax,[r12+8]
                self.octets(&[0xFF, 0xD0]);                 // call rax
            }
        }
        Ok(())
    }

    fn corriger_chaines(&mut self, base_chaines: usize, offsets: &[usize]) {
        let refs = std::mem::take(&mut self.chaines_ref);
        for c in refs {
            match c {
                CorrectifChaine::A64 { insn, chaine } => {
                    let cible = base_chaines + offsets[chaine];
                    let disp = cible as i64 - insn as i64;
                    let immlo = (disp as u32) & 3;
                    let immhi = ((disp >> 2) as u32) & 0x7FFFF;
                    let v = 0x10000000u32 | (immlo << 29) | (immhi << 5) | A64_NOM;
                    self.code[insn..insn + 4].copy_from_slice(&v.to_le_bytes());
                }
                CorrectifChaine::X86 { insn, chaine } => {
                    let cible = base_chaines + offsets[chaine];
                    let disp = cible as i64 - (insn as i64 + 7);
                    self.code[insn + 3..insn + 7].copy_from_slice(&(disp as i32).to_le_bytes());
                }
            }
        }
    }

    // ── évaluation d'expressions ───────────────────────────────────────
    /// Une expression est « sec-sûre » si son évaluation ne touche pas au
    /// registre second (x9 / rcx). C'est le cas des feuilles et de Neg/Non.
    /// `AppelRir` l'est aussi depuis que `appel_direct` utilise x10 (et non
    /// x9) pour la taille de trame sur aarch64.
    fn sec_sur(&self, e: &Eabstrait) -> bool {
        match e {
            Eabstrait::Slot(..) | Eabstrait::Entier(_) | Eabstrait::AppelRir { .. }
            | Eabstrait::Neg(_) | Eabstrait::Non(_) => true,
            _ => false,
        }
    }

    /// Vrai quand `Binaire(op, a, b)` peut être évalué SANS slot temporaire :
    /// le résultat de `a` reste dans le registre second, `b` est évaluée
    /// dans le registre de valeur, puis l'ALU combine les deux.
    fn binaire_sans_slot(&self, op: &Oper, a: &Eabstrait, b: &Eabstrait) -> bool {
        self.mode == ModeAppel::Direct && !self.avec_runtime
            && matches!(op, Oper::Add | Oper::Sub | Oper::Mul | Oper::Lt
                        | Oper::Le | Oper::Gt | Oper::Ge | Oper::Eq | Oper::Ne)
            && self.sec_sur(b)
            && self.peut_evaluer_reg(a) && self.peut_evaluer_reg(b)
    }

    /// Évalue `e` en laissant le résultat (i64) dans le registre de valeur
    /// (x0 / rax), sans écrire de slot. Utilisé pour les expressions natives
    /// du chemin AOT sans runtime : supprime les `str`/`ldr` redondants des
    /// sous-expressions (chacune était auparavant stockée puis rechargée).
    fn evaluer_reg(&mut self, e: &Eabstrait, niveau: usize) -> Result<(), EmitError> {
        match e {
            Eabstrait::Slot(s, _) => {
                self.load_slot(self.val_reg(), *s);
            }
            Eabstrait::Entier(t) => {
                let v = t.parse::<i64>().map_err(|_| EmitError::EntierHorsPlage(t.clone()))?;
                self.mov_imm(self.val_reg(), v);
            }
            Eabstrait::Neg(x) => {
                self.evaluer_reg(x, niveau)?;
                self.neg_val();
            }
            Eabstrait::Non(x) => {
                self.evaluer_reg(x, niveau)?;
                self.non_val();
            }
            Eabstrait::Binaire { op, a, b } if self.binaire_sans_slot(op, a, b) => {
                // ⚠ ordre : `alu` calcule `val ← val OP sec`. On évalue donc
                // le OPERANDE DROIT en premier (sauvé dans `sec`), puis le
                // gauche dans `val` : c'est `a OP b`, pas `b OP a`.
                let sec = self.sec_reg_at(niveau);
                self.evaluer_reg(b, niveau + 1)?;
                self.mov_reg(sec, self.val_reg());
                self.secs_vifs.push(sec);
                let r = self.evaluer_reg(a, niveau + 1);
                self.secs_vifs.pop();
                r?;
                self.alu(*op, sec);
            }
            Eabstrait::AppelRir { nom, args } => {
                let mut slots = [0u16; 4];
                if args.len() > 4 { return Err(EmitError::TropArguments); }
                let mut appel = Appel { nom: nom.clone(), args_slots: [None; 4] };
                for (i, a) in args.iter().enumerate() {
                    match a {
                        Expr::Var(n, _) => {
                            let s = self.corps.noms.iter().position(|x| x == n)
                                .ok_or_else(|| EmitError::SlotInconnu(n.clone()))? as u16;
                            slots[i] = s;
                            appel.args_slots[i] = Some(s);
                        }
                        _ => return Err(EmitError::SlotInconnu(format!("{:?}", a))),
                    }
                }
                self.appels.push(appel);
                if self.mode == ModeAppel::Direct {
                    self.appel_direct(&slots, args.len(), nom)?;
                } else {
                    self.appel_runtime(&slots, nom);
                }
                // Le résultat est déjà dans x0 / rax : aucun `str` nécessaire.
            }
            _ => {
                let t = self.fresh()?;
                self.evaluer(e, t)?;
                self.load_slot(self.val_reg(), t);
            }
        }
        Ok(())
    }

    /// Évalue `e` et pose le résultat (i64) dans le slot `out`.
    fn evaluer(&mut self, e: &Eabstrait, out: u16) -> Result<(), EmitError> {
        match e {
            Eabstrait::Slot(s, _) => {
                self.load_slot(self.val_reg(), *s);
                self.store_slot(self.val_reg(), out);
            }
            Eabstrait::Entier(t) => {
                let v = t.parse::<i64>().map_err(|_| EmitError::EntierHorsPlage(t.clone()))?;
                self.mov_imm(self.val_reg(), v);
                self.store_slot(self.val_reg(), out);
                self.set_type(out, 0);
            }
            Eabstrait::Flottant(f) => {
                if !self.avec_runtime {
                    self.verifier_sans_fallback(e)?;
                    self.fallback_runtime(e)?;
                    self.store_slot(self.val_reg(), out);
                    return Ok(());
                }
                self.mov_imm(self.val_reg(), f.to_bits() as i64);
                self.store_slot(self.val_reg(), out);
                self.set_type(out, 1);
            }
            Eabstrait::Binaire { op, a, b } => {
                let ta_type = self.type_of(a);
                let tb_type = self.type_of(b);
                // ── AOT6 : puissance entière → runtime bigint natif ──
                if self.avec_runtime && matches!(op, Oper::Puissance)
                    && ta_type == 0 && tb_type == 0 {
                    let ta = self.fresh()?;
                    let tb = self.fresh()?;
                    self.evaluer(a, ta)?;
                    self.evaluer(b, tb)?;
                    // La routine lit base[0] et base[1] : on lui passe
                    // directement l'adresse des deux slots temporaires.
                    match self.arch {
                        Architecture::Aarch64 => {
                            // add x0, x19, #ta*8 (imm12 non mis à l'échelle)
                            let imm = ta as u32 * 8;
                            self.u32(0x91000000 | (imm << 10) | (A64_BASE << 5) | 0);
                            self.u32(0xAA1F03E1); // x1 = 0 (trampoline)
                        }
                        Architecture::X86_64 => {
                            self.octets(&[0x48, 0x8D, 0x43, (ta as u8) * 8]); // lea rax,[rbx+ta*8]
                            self.octets(&[0x48, 0x89, 0xC7]); // mov rdi,rax
                            self.octets(&[0x31, 0xF6]);       // xor esi,esi
                        }
                    }
                    self.appel_runtime_par_nom("$rt_bigint_puissance")?;
                    self.store_slot(self.val_reg(), out);
                    self.set_type(out, 2);
                    return Ok(());
                }
                // ── AOT6 : arithmétique IEEE-754 native ──
                if self.avec_runtime && (ta_type == 1 || tb_type == 1)
                    && matches!(op, Oper::Add | Oper::Sub | Oper::Mul | Oper::Div
                                | Oper::Lt | Oper::Le | Oper::Gt | Oper::Ge | Oper::Eq | Oper::Ne) {
                    let ta = self.fresh()?;
                    let tb = self.fresh()?;
                    self.evaluer(a, ta)?;
                    self.evaluer(b, tb)?;
                    self.float_load(0, ta);
                    self.float_load(1, tb);
                    self.float_alu(*op);
                    self.float_store(0, out);
                    self.set_type(out, if matches!(op, Oper::Lt | Oper::Le | Oper::Gt | Oper::Ge | Oper::Eq | Oper::Ne) { 4 } else { 1 });
                    return Ok(());
                }
                if !matches!(op, Oper::Add | Oper::Sub | Oper::Mul | Oper::Lt
                              | Oper::Le | Oper::Gt | Oper::Ge | Oper::Eq | Oper::Ne) {
                    // Sans runtime : trampoline (refusé en mode AOT).
                    self.verifier_sans_fallback(e)?;
                    self.fallback_runtime(e)?;
                    self.store_slot(self.val_reg(), out);
                    return Ok(());
                }
                // ── chemin natif sans runtime : zéro slot temporaire ──
                if let Eabstrait::Binaire { op, a, b } = e {
                    if self.binaire_sans_slot(op, a, b) {
                        self.evaluer_reg(e, 0)?;
                        self.store_slot(self.val_reg(), out);
                        self.set_type(out, if matches!(op, Oper::Lt | Oper::Le
                                        | Oper::Gt | Oper::Ge | Oper::Eq | Oper::Ne) { 4 } else { 0 });
                        return Ok(());
                    }
                }
                let ta = self.fresh()?;
                let tb = self.fresh()?;
                self.evaluer(a, ta)?;
                self.evaluer(b, tb)?;
                self.load_slot(self.val_reg(), ta);
                let sec = self.sec_reg();
                self.load_slot(sec, tb);
                self.alu(*op, sec);
                self.store_slot(self.val_reg(), out);
                self.set_type(out, 0);
            }
            Eabstrait::Neg(x) => {
                if self.mode == ModeAppel::Direct && !self.avec_runtime {
                    self.evaluer_reg(x, 0)?;
                } else {
                    let t = self.fresh()?;
                    self.evaluer(x, t)?;
                    self.load_slot(self.val_reg(), t);
                }
                self.neg_val();
                self.store_slot(self.val_reg(), out);
            }
            Eabstrait::Non(x) => {
                if self.mode == ModeAppel::Direct && !self.avec_runtime {
                    self.evaluer_reg(x, 0)?;
                } else {
                    let t = self.fresh()?;
                    self.evaluer(x, t)?;
                    self.load_slot(self.val_reg(), t);
                }
                self.non_val();
                self.store_slot(self.val_reg(), out);
            }
            Eabstrait::AppelRir { .. } if self.mode == ModeAppel::Direct && !self.avec_runtime => {
                self.evaluer_reg(e, 0)?;
                self.store_slot(self.val_reg(), out);
            }
            Eabstrait::AppelRir { nom, args } => {
                let mut slots = [0u16; 4];
                if args.len() > 4 { return Err(EmitError::TropArguments); }
                let mut appel = Appel { nom: nom.clone(), args_slots: [None; 4] };
                for (i, a) in args.iter().enumerate() {
                    match a {
                        Expr::Var(n, _) => {
                            let s = self.corps.noms.iter().position(|x| x == n)
                                .ok_or_else(|| EmitError::SlotInconnu(n.clone()))? as u16;
                            slots[i] = s;
                            appel.args_slots[i] = Some(s);
                        }
                        _ => return Err(EmitError::SlotInconnu(format!("{:?}", a))),
                    }
                }
                self.appels.push(appel);
                if self.mode == ModeAppel::Direct {
                    self.appel_direct(&slots, args.len(), nom)?;
                } else {
                    self.appel_runtime(&slots, nom);
                }
                self.store_slot(self.val_reg(), out);
            }
            _ => {
                // Flottant, et autres variantes → trampoline générique
                // (refusé en mode AOT).
                self.verifier_sans_fallback(e)?;
                self.fallback_runtime(e)?;
                self.store_slot(self.val_reg(), out);
            }
        }
        Ok(())
    }

    /// Registre « second » disponible à une profondeur d'imbrication donnée.
    /// Les niveaux différents utilisent des registres différents pour que
    /// l'évaluation d'un opérande ne détruise pas l'autre déjà sauvegardé.
    fn sec_reg_at(&self, niveau: usize) -> u32 {
        match self.arch {
            Architecture::Aarch64 => [9u32, 11, 12, 13, 14, 15][niveau.min(5)],
            Architecture::X86_64 => [1u32, 2, 8, 9, 10, 11][niveau.min(5)],
        }
    }
    fn sec_reg(&self) -> u32 { self.sec_reg_at(0) }

    // ── opérations RIR ─────────────────────────────────────────────────
    /// `est_dernier` : l'opération est la dernière du corps. Un `retour`
    /// final tombe alors directement sur l'épilogue : le saut est superflu.
    fn operation(&mut self, op: &Op, est_dernier: bool) -> Result<(), EmitError> {
        match op {
            Op::Affect { slot, expr } => {
                self.evaluer(expr, *slot)?;
                Ok(())
            }
            Op::RetourAbstrait(expr) => {
                if self.mode == ModeAppel::Direct && !self.avec_runtime && self.peut_evaluer_reg(expr) {
                    self.evaluer_reg(expr, 0)?;
                } else {
                    let t = self.fresh()?;
                    self.evaluer(expr, t)?;
                    self.load_slot(self.val_reg(), t);
                }
                // Retour = saut vers l'épilogue (offset sentinel = ops.len()).
                // Dernière opération : l'épilogue suit immédiatement.
                if !est_dernier {
                    let cible = self.corps.ops.len();
                    self.saut(cible);
                }
                Ok(())
            }
            Op::Saut { cible } => {
                self.saut(*cible);
                Ok(())
            }
            Op::SautSiFauxAbstrait { test, cible } => {
                if self.mode == ModeAppel::Direct && !self.avec_runtime && self.peut_evaluer_reg(test) {
                    self.evaluer_reg(test, 0)?;
                    self.saut_si_faux_val(*cible);
                } else {
                    let t = self.fresh()?;
                    self.evaluer(test, t)?;
                    self.saut_si_faux(t, *cible);
                }
                Ok(())
            }
            other => Err(EmitError::OperationNonSupportee(format!("{:?}", other))),
        }
    }

    /// Les expressions qui peuvent être evaluées directement dans x0/rax.
    fn peut_evaluer_reg(&self, e: &Eabstrait) -> bool {
        match e {
            Eabstrait::Slot(..) | Eabstrait::Entier(_) | Eabstrait::AppelRir { .. } => true,
            Eabstrait::Neg(x) | Eabstrait::Non(x) => self.peut_evaluer_reg(x),
            Eabstrait::Binaire { op, a, b } => self.binaire_sans_slot(op, a, b),
            _ => false,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// TESTS UNITAIRES (compilés avec le module)
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rir::{CacheSlot, Oper};

    fn slot(i: u16) -> Eabstrait { Eabstrait::Slot(i, CacheSlot::neuve()) }
    fn bin(op: Oper, a: Eabstrait, b: Eabstrait) -> Eabstrait {
        Eabstrait::Binaire { op, a: Box::new(a), b: Box::new(b) }
    }
    fn corps(ops: Vec<Op>, n: usize) -> CorpsRir {
        CorpsRir { ops, noms: (0..n).map(|i| format!("s{}", i)).collect(),
                   globaux: vec![], n_slots: n }
    }

    #[test]
    fn emet_mul_sans_trampoline_x86() {
        // retour (bin * slot0 slot0) → mul + ret, AUCUN appel runtime.
        let c = corps(vec![Op::RetourAbstrait(bin(Oper::Mul, slot(0), slot(0)))], 1);
        let e = emettre(&c, Architecture::X86_64).unwrap();
        assert!(e.appels.is_empty());
        assert!(e.fallbacks.is_empty());
        // rechercher `48 0f af c1` (imul rax, rcx) et le `c3` final.
        let fenetres = e.code.windows(4).any(|w| w == [0x48, 0x0F, 0xAF, 0xC1]);
        assert!(fenetres, "imul absent: {:02X?}", e.code);
        assert_eq!(*e.code.last().unwrap(), 0xC3);
    }

    #[test]
    fn emet_mul_sans_trampoline_a64() {
        let c = corps(vec![Op::RetourAbstrait(bin(Oper::Mul, slot(0), slot(0)))], 1);
        let e = emettre(&c, Architecture::Aarch64).unwrap();
        let mul = 0x9B007C00u32 | (9 << 16) | (0 << 5) | 0;
        let octets = mul.to_le_bytes();
        assert!(e.code.windows(4).any(|w| w == octets), "mul absent: {:08X?}", e.code);
        assert_eq!(&e.code[e.code.len() - 4..], &0xD65F03C0u32.to_le_bytes());
    }

    #[test]
    fn sauts_sont_patches() {
        // s0 = 1 ; saut vers op 2 ; s0 = 2
        let c = corps(vec![
            Op::Affect { slot: 0, expr: Eabstrait::Entier("1".into()) },
            Op::Saut { cible: 2 },
            Op::Affect { slot: 0, expr: Eabstrait::Entier("2".into()) },
        ], 1);
        let e = emettre(&c, Architecture::X86_64).unwrap();
        assert_eq!(e.op_offsets.len(), 4);
        assert_eq!(e.op_offsets[2], e.op_offsets[1] + 5); // jmp rel32 = 5 octets
    }

    #[test]
    fn aot5_hasard_est_du_pur_code_machine() {
        for arch in [Architecture::Aarch64, Architecture::X86_64] {
            let p = emettre_hasard(10, arch).unwrap();
            assert!(p.appels.is_empty(), "appel runtime dans le RNG {:?}", arch);
            assert!(p.fallbacks.is_empty(), "fallback dans le RNG {:?}", arch);
            assert_eq!(p.noms, vec!["$principal"]);
            assert_eq!(p.offsets, vec![0]);
            assert!(p.code.len() < 200, "code RNG trop long: {}", p.code.len());
            // Constante splitmix64 présente : movabs contigu sur x86-64,
            // movz + 3×movk (16 bits par instruction) sur aarch64.
            let sm = match arch {
                Architecture::X86_64 => SM_C.to_le_bytes().to_vec(),
                Architecture::Aarch64 =>
                    (0xD280_0000u32 | (((SM_C & 0xFFFF) as u32) << 5) | 11).to_le_bytes().to_vec(),
            };
            assert!(p.code.windows(sm.len()).any(|w| w == sm),
                    "constante splitmix absente sur {:?}", arch);
        }
        // bornage non positif : refus explicite, aucun binaire.
        assert!(emettre_hasard(0, Architecture::Aarch64).is_err());
    }

    #[test]
    fn operation_hors_sous_ensemble_refusee() {
        let c = corps(vec![Op::Afficher(vec![])], 1);
        assert!(emettre(&c, Architecture::X86_64).is_err());
        assert!(!peut_emettre(&c));
    }

    // ── exécution réelle (mmap) ────────────────────────────────────────
    extern "C" fn pas_de_tramp(_: i64, _: i64, _: i64, _: i64, _: *const u8) -> i64 { 0 }
    extern "C" fn pas_de_fallback(_: *mut std::ffi::c_void, _: u32, _: *mut i64) -> i64 { 0 }

    #[test]
    fn execute_code_emission_reelle() {
        // f(a, b) = a*a - b  (arithmétique native, aucun appel).
        let c = corps(vec![Op::RetourAbstrait(
            bin(Oper::Sub, bin(Oper::Mul, slot(0), slot(0)), slot(1)))], 2);
        // ⚠ On n'exécute QUE l'architecture hôte : exécuter des octets
        // x86-64 sur aarch64 (ou l'inverse) donnerait SIGILL.
        let arch = Architecture::native();
        let e = emettre(&c, arch).unwrap();
        let mem = MemoireExecutable::nouveau(&e.code).unwrap();
        let tramp = Trampoline { appel_rir: pas_de_tramp,
                                 eval_fallback: pas_de_fallback, ctx: std::ptr::null_mut() };
        let mut slots = vec![0i64; e.n_slots_total];
        slots[0] = 7; slots[1] = 5;
        let r = unsafe { mem.appeler(slots.as_mut_ptr(), &tramp) };
        assert_eq!(r, 44, "résultat faux sur {}", arch.nom());
    }

    // ── AOT : appels directs, SANS trampoline ──────────────────────────
    #[test]
    fn programme_emetteur_appel_direct_sans_trampoline() {
        let arch = Architecture::native();
        let g = corps(vec![Op::RetourAbstrait(
            bin(Oper::Mul, slot(0), Eabstrait::Entier("6".into())))], 1);
        let var = |i: u16| Expr::Var(format!("s{}", i), 0);
        let principal = corps(vec![Op::RetourAbstrait(Eabstrait::AppelRir {
            nom: "g".into(), args: vec![var(0)],
        })], 1);
        let prog = emettre_programme(&[
            ("$principal".to_string(), principal),
            ("g".to_string(), g),
        ], arch).unwrap();
        assert_eq!(prog.appels.len(), 1);
        assert_eq!(prog.appels[0].nom, "g");
        assert_eq!(prog.appels[0].cible, 1);
        assert!(prog.fallbacks.is_empty());
        let mem = MemoireExecutable::nouveau(&prog.code).unwrap();
        let tramp = Trampoline { appel_rir: pas_de_tramp,
                                 eval_fallback: pas_de_fallback,
                                 ctx: std::ptr::null_mut() };
        let mut slots = vec![0i64; prog.n_slots_total];
        slots[0] = 7;
        assert_eq!(unsafe { mem.appeler(slots.as_mut_ptr(), &tramp) }, 42,
                   "appel direct faux sur {}", arch.nom());
    }

    #[test]
    fn programme_emetteur_refuse_fonction_inconnue_et_fallback() {
        let c = corps(vec![Op::RetourAbstrait(Eabstrait::AppelRir {
            nom: "absente".into(), args: vec![],
        })], 0);
        assert!(matches!(emettre_programme(&[("$principal".into(), c)], Architecture::native()),
                         Err(EmitError::FonctionInconnue(_))));
        let f = corps(vec![Op::RetourAbstrait(Eabstrait::Flottant(1.5))], 0);
        assert!(emettre_programme(&[("$principal".into(), f)], Architecture::native()).is_err());
    }

    // ── fib(25) : émission + récursion native via la trampoline ────────
    use std::cell::Cell;
    thread_local! {
        /// Pointeur vers le code émis, utilisé par la trampoline récursive.
        static CODE_FIB: Cell<*const MemoireExecutable> =
            Cell::new(std::ptr::null());
    }

    extern "C" fn fib_tramp(a0: i64, _: i64, _: i64, _: i64, _: *const u8) -> i64 {
        let mem = CODE_FIB.with(|c| c.get());
        assert!(!mem.is_null());
        unsafe {
            let mut slots = [0i64; 5 + SCRATCH_SLOTS as usize];
            slots[0] = a0;
            let tramp = Trampoline { appel_rir: fib_tramp,
                                     eval_fallback: pas_de_fallback,
                                     ctx: std::ptr::null_mut() };
            (*mem).appeler(slots.as_mut_ptr(), &tramp)
        }
    }

    #[test]
    fn ebenche_fib25() {
        // fib(n) = si n < 2 alors n sinon fib(n-1) + fib(n-2).
        // s0 = n ; s1 = n-1 ; s2 = n-2 ; s3/s4 = résultats des appels.
        let var = |i: u16| Expr::Var(format!("s{}", i), 0);
        let app = |i: u16| Eabstrait::AppelRir {
            nom: "fib".into(), args: vec![var(i)],
        };
        let c = corps(vec![
            Op::SautSiFauxAbstrait {
                test: bin(Oper::Lt, slot(0), Eabstrait::Entier("2".into())),
                cible: 2,
            },
            Op::RetourAbstrait(slot(0)),
            Op::Affect { slot: 1, expr: bin(Oper::Sub, slot(0), Eabstrait::Entier("1".into())) },
            Op::Affect { slot: 3, expr: app(1) },
            Op::Affect { slot: 2, expr: bin(Oper::Sub, slot(0), Eabstrait::Entier("2".into())) },
            Op::Affect { slot: 4, expr: app(2) },
            Op::RetourAbstrait(bin(Oper::Add, slot(3), slot(4))),
        ], 5);

        let arch = Architecture::native();
        let e = emettre(&c, arch).unwrap();
        assert_eq!(e.appels.len(), 2);
        let mem = MemoireExecutable::nouveau(&e.code).unwrap();
        CODE_FIB.with(|x| x.set(&mem));
        let tramp = Trampoline { appel_rir: fib_tramp,
                                 eval_fallback: pas_de_fallback, ctx: std::ptr::null_mut() };
        let mut slots = vec![0i64; e.n_slots_total];

        // Balayage de conformité : n = 0..=20 contre une référence Rust.
        fn reference(n: u64) -> u64 {
            if n < 2 { n } else { reference(n - 1) + reference(n - 2) }
        }
        for n in 0..=20u64 {
            slots[0] = n as i64;
            let r = unsafe { mem.appeler(slots.as_mut_ptr(), &tramp) };
            assert_eq!(r, reference(n) as i64, "fib({}) faux sur {}", n, arch.nom());
        }

        slots[0] = 25;
        let t0 = std::time::Instant::now();
        let r = unsafe { mem.appeler(slots.as_mut_ptr(), &tramp) };
        let dt = t0.elapsed();
        assert_eq!(r, 75025, "fib(25) faux sur {}", arch.nom());
        eprintln!("BENCH fib(25) sur {} : {} en {:?} ({} octets émis)",
                  arch.nom(), r, dt, e.code.len());
        CODE_FIB.with(|x| x.set(std::ptr::null()));
    }
}
