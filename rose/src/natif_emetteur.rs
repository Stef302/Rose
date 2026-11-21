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
    for op in &corps.ops {
        op_offsets.push(b.code.len());
        b.operation(op)?;
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
        for op in &corps.ops {
            op_offsets.push(b.code.len());
            b.operation(op)?;
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
            appels_lies: Vec::new(),
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

    fn u32(&mut self, v: u32) { self.code.extend_from_slice(&v.to_le_bytes()); }
    fn octets(&mut self, v: &[u8]) { self.code.extend_from_slice(v); }

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
        // REX.W ; modrm(mod=01, reg, rm=011=rbx) ; disp8
        // Pour disp8 >= 128, passer en disp32 (mod=10).
        self.octets(&[0x48]);
        if off <= 127 {
            self.octets(&[opcode, 0x43 | (reg << 3), off as u8]);
        } else {
            self.octets(&[opcode, 0x83 | (reg << 3)]);
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
    fn alu(&mut self, op: Oper) {
        match self.arch {
            Architecture::Aarch64 => {
                match op {
                    Oper::Add => self.u32(0x8B000000 | (A64_SEC << 16) | (A64_VAL << 5) | A64_VAL),
                    Oper::Sub => self.u32(0xCB000000 | (A64_SEC << 16) | (A64_VAL << 5) | A64_VAL),
                    Oper::Mul => self.u32(0x9B007C00 | (A64_SEC << 16) | (A64_VAL << 5) | A64_VAL),
                    Oper::Lt | Oper::Le | Oper::Gt | Oper::Ge | Oper::Eq | Oper::Ne => {
                        // cmp val, sec ; cset val, cond
                        self.u32(0xEB000000 | (A64_SEC << 16) | (A64_VAL << 5) | 31);
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
                match op {
                    Oper::Add => self.octets(&[0x48, 0x01, 0xC8]), // add rax, rcx
                    Oper::Sub => self.octets(&[0x48, 0x29, 0xC8]), // sub rax, rcx
                    Oper::Mul => self.octets(&[0x48, 0x0F, 0xAF, 0xC1]), // imul rax, rcx
                    Oper::Lt | Oper::Le | Oper::Gt | Oper::Ge | Oper::Eq | Oper::Ne => {
                        self.octets(&[0x48, 0x39, 0xC8]); // cmp rax, rcx
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
        match self.arch {
            Architecture::Aarch64 => {
                // ⚠ On charge les arguments AVANT de bouger sp (x0..x3), puis
                // on réserve la trame et on les écrit dans la trame enfant.
                for (i, &s) in args.iter().enumerate().take(n_args) {
                    self.load_slot(i as u32, s);
                }
                // ⚠ Forme « extended register » (bit 21 + option=3) :
                // c'est la seule qui puisse encoder SP comme Rn/Rd.
                // x9 est caller-saved : on le RECHARGE après le `bl`.
                self.mov_imm(9, f as i64);
                self.u32(0xCB000000 | (1 << 21) | (9 << 16) | (3 << 13)
                         | (31 << 5) | 31); // sub sp, sp, x9
                for i in 0..n_args {
                    // str x{i}, [sp, #8*i]  (imm12 = déplacement / 8 = i)
                    self.u32(0xF9000000 | ((i as u32) << 10) | (31 << 5) | i as u32);
                }
                self.u32(0x910003E0); // mov x0, sp  (base de l'appelé)
                self.u32(0xAA1F03E1); // mov x1, xzr (trampoline inutilisée)
                let correctif = self.code.len();
                self.u32(0x9400_0000); // bl (patché par emettre_programme)
                self.mov_imm(9, f as i64); // ⚠ rechargé : l'appelé a clobber x9
                self.u32(0x8B000000 | (1 << 21) | (9 << 16) | (3 << 13)
                         | (31 << 5) | 31); // add sp, sp, x9
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
            }
            Eabstrait::Binaire { op, a, b } => {
                if !matches!(op, Oper::Add | Oper::Sub | Oper::Mul | Oper::Lt
                              | Oper::Le | Oper::Gt | Oper::Ge | Oper::Eq | Oper::Ne) {
                    // Div, Mod, Puissance, DivEntier, Et, Ou → trampoline
                    // (refusé en mode AOT : aucun runtime dans le binaire).
                    self.verifier_sans_fallback(e)?;
                    self.fallback_runtime(e)?;
                    self.store_slot(self.val_reg(), out);
                    return Ok(());
                }
                let ta = self.fresh()?;
                let tb = self.fresh()?;
                self.evaluer(a, ta)?;
                self.evaluer(b, tb)?;
                self.load_slot(self.val_reg(), ta);
                self.load_slot(self.sec_reg(), tb);
                self.alu(*op);
                self.store_slot(self.val_reg(), out);
            }
            Eabstrait::Neg(x) => {
                let t = self.fresh()?;
                self.evaluer(x, t)?;
                self.load_slot(self.val_reg(), t);
                self.neg_val();
                self.store_slot(self.val_reg(), out);
            }
            Eabstrait::Non(x) => {
                let t = self.fresh()?;
                self.evaluer(x, t)?;
                self.load_slot(self.val_reg(), t);
                self.non_val();
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

    fn sec_reg(&self) -> u32 {
        match self.arch { Architecture::Aarch64 => A64_SEC, Architecture::X86_64 => X86_SEC as u32 }
    }

    // ── opérations RIR ─────────────────────────────────────────────────
    fn operation(&mut self, op: &Op) -> Result<(), EmitError> {
        match op {
            Op::Affect { slot, expr } => {
                self.evaluer(expr, *slot)?;
                Ok(())
            }
            Op::RetourAbstrait(expr) => {
                let t = self.fresh()?;
                self.evaluer(expr, t)?;
                self.load_slot(self.val_reg(), t);
                // Retour = saut vers l'épilogue (offset sentinel = ops.len()).
                let cible = self.corps.ops.len();
                self.saut(cible);
                Ok(())
            }
            Op::Saut { cible } => {
                self.saut(*cible);
                Ok(())
            }
            Op::SautSiFauxAbstrait { test, cible } => {
                let t = self.fresh()?;
                self.evaluer(test, t)?;
                self.saut_si_faux(t, *cible);
                Ok(())
            }
            other => Err(EmitError::OperationNonSupportee(format!("{:?}", other))),
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
