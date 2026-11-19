// ═══════════════════════════════════════════════════════════════════════
// WASM — INTERPRÉTEUR WEBASSEMBLY MINIMAL POUR ROSE
// ═══════════════════════════════════════════════════════════════════════
//
// ⚠⚠⚠ POURQUOI CE MODULE. L'idée : au lieu d'avoir un runtime par langage
// pour exécuter ses bibliothèques, on compile les bibliothèques qui nous
// intéressent en WebAssembly, et Rose les exploite directement. Le
// WebAssembly est le seul format binaire qui soit :
//
//   * produit par TOUS les compilateurs modernes (clang, rustc, emcc, Zig…)
//     sans dépendre d'un runtime de langage (pas de GC imposé, pas de
//     stdlib imposée : `--target=wasm32 -nostdlib`) ;
//   * déterministe et sandboxé (pas d'accès disque/réseau implicite) ;
//   * STABLE (la spécification ne bouge plus depuis 2.0).
//
// ⚠ Ce module est un INTERPRÉTEUR, pas un compilateur : il lit le binaire
// (magic `\0asm`, version 1), valide les sections, et exécute les fonctions
// exportées. Il ne dépend de RIEN (pas de crate `wasmparser`/`wasmi`), comme
// le reste du moteur (zéro dépendance pour les jardins).
//
// ⚠ SOUS-ENSEMBLE VOLONTAIREMENT LARGE MAIS BORNÉ :
//   types : i32, i64, f32, f64 (pas de v128 pour l'instant) ;
//   contrôle : block, loop, if/else, br, br_if, br_table, return, call,
//              call_indirect, unreachable, nop, drop, select ;
//   mémoire : load/store (toutes tailles), memory.size/grow, memory.copy/fill ;
//   arithmétique : const, add/sub/mul/div/rem (signés et non signés), et/ou/xor,
//                  shifts, rotations, comparaisons, conversions, clz/ctz/popcnt ;
//   globals : get/set (mutables inclus).
//
// ⚠ HORS SUPPORT (refusé PROPREMENT, avec message) : les exceptions, les SIMD,
//    les threads, les références (externref), les tables multiples.
//
// ⚠⚠⚠ VAGUE 32 (19/11/2026) : `call_indirect` ET la section `start` sont
//    DÉSORMAIS SUPPORTÉS (voir plus bas). C'est ce qui débloque les bibliothèques
//    réelles compilées avec optimisation : clang -O1/-O2 prend l'adresse de
//    helpers `static` et émet des `call_indirect`.
//
// ⚠⚠⚠ IMPORTS (M6.3) : la section d'import est désormais LUE. Les imports de
//    fonction sont résolus vers un registre de FONCTIONS HÔTES Rose (voir
//    `HOTES` / `enregistrer_hote` plus bas) ; un import non résolu est rejeté
//    AU CHARGEMENT avec un message explicite, jamais un crash. Les imports de
//    mémoire sont reliés à la mémoire linéaire que Rose alloue pour l'instance.
//    Les modules 0-import existants gardent EXACTEMENT le même comportement.
//
// ⚠⚠ L'ABI C : un module compilé avec `--target=wasm32 -nostdlib` exporte ses
// fonctions C avec la convention wasm32 (i32 = pointeur/entier 32 bits,
// i64, f64…). Rose appelle donc `carre(7)` comme une fonction C : c'est
// EXACTEMENT le pont ABI C, sans `dlopen`, sans linker, sans OS.

// ⚠⚠⚠ `Arc` ET NON `Rc` : les instances sont rangées dans une table GLOBALE
// (`interp::wasm_ffi`) partagée entre threads — un `Rc` y est interdit.
use std::sync::Arc;
use std::collections::HashMap;
use std::sync::Mutex;
use std::io::Write;

/// ⚠ Une erreur de lecture/validation/execution du module.
#[derive(Debug, Clone)]
pub struct WasmErr(pub String);

impl std::fmt::Display for WasmErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

fn err<T>(m: impl Into<String>) -> Result<T, WasmErr> { Err(WasmErr(m.into())) }

// ⚠⚠⚠ VAGUE 32 — BORNES D'INSTRUCTIONS (anti-boucle infinie).
// Un module non fiable peut boucler pour toujours : l'interpréteur doit rendre
// la main. Le budget est COMPTÉ PAR INSTRUCTION EXÉCUTÉE et partagé par tous
// les appels imbriqués d'un même appel public. Valeurs mesurées : xxHash/FNV
// < 100 k instructions, miniz CRC32 ~200 k, LZ4 compress -O2 ~12 M.
/// Budget pour un appel public (généreux : les algo réels ont besoin de millions).
pub const BUDGET_APPEL: u64 = 500_000_000;
/// Budget pour la fonction de la section `start` (init doit rester courte).
pub const BUDGET_START: u64 = 20_000_000;

// ═══════════════════════════════════════════════════════════════════════
// REGISTRE DES FONCTIONS HÔTES (M6.3)
// ═══════════════════════════════════════════════════════════════════════
//
// ⚠ Une fonction hôte reçoit l'INSTANCE (pour accéder à sa mémoire linéaire)
// et les arguments déjà convertis dans les types wasm déclarés. Elle rend les
// valeurs de retour ; `appeler_hote` adapte leur nombre/type à la signature
// déclarée dans la section `type` du module (comportement tolérant et prévisible).
/// ⚠ Signature d'une fonction hôte Rose appelable depuis un module wasm.
pub type HoteFonction = fn(&mut InstanceWasm, &[ValWasm]) -> Result<Vec<ValWasm>, WasmErr>;

/// ⚠ Enregistre (ou remplace) une fonction hôte `(module, nom)`.
///
/// Le registre est GLOBAL : il est partagé par toutes les instances. Un hôte
/// enregistré APRÈS le chargement d'un module n'est PAS visible par ce module
/// (la résolution a lieu dans `charger`). Enregistrer avant le chargement.
pub fn enregistrer_hote(module: &str, nom: &str, f: HoteFonction) {
    let mut g = match HOTES.lock() { Ok(g) => g, Err(p) => p.into_inner() };
    if g.is_none() { *g = Some(HashMap::new()); }
    if let Some(table) = g.as_mut() {
        table.insert((module.to_string(), nom.to_string()), f);
    }
}

/// ⚠ Résout un import `(module, nom)` : d'abord le registre dynamique, puis
/// la table des hôtes intégrés. `None` = import non résolu.
pub fn resoudre_hote(module: &str, nom: &str) -> Option<HoteFonction> {
    if let Ok(g) = HOTES.lock() {
        if let Some(table) = g.as_ref() {
            if let Some(f) = table.get(&(module.to_string(), nom.to_string())) {
                return Some(*f);
            }
        }
    }
    HOTES_INTEGRES.iter()
        .find(|(m, n, _)| *m == module && *n == nom)
        .map(|(_, _, f)| *f)
}

static HOTES: Mutex<Option<HashMap<(String, String), HoteFonction>>> = Mutex::new(None);

// ⚠ La table intégrée, par (module, nom). Les alias `wasi_unstable` couvrent
// les anciens outils (clang wasi-sdk < 14) ; `env.ecrire` est l'équivalent
// francophone de `env.log`.
//
// ⚠⚠⚠ VAGUE 32 : le registre WASI passe de 4 à 22 fonctions par namespace.
// Les hôtes de CHEMIN (`path_*`) et de PRÉOUVERTURE (`fd_prestat_*`) sont des
// stubs SANDBOXés HONNÊTES : ils rendent un errno WASI explicite (ENOTCAPABLE
// = 76, EBADF = 8), jamais une réussite simulée. Aucun accès disque/réseau.
const HOTES_INTEGRES: &[(&str, &str, HoteFonction)] = &[
    ("env", "log", hote_log),
    ("env", "ecrire", hote_ecrire),
    // ── WASI : E/S et divers (réels) ────────────────────────────────────
    ("wasi_snapshot_preview1", "fd_write", hote_fd_write),
    ("wasi_snapshot_preview1", "fd_read", hote_fd_read),
    ("wasi_snapshot_preview1", "proc_exit", hote_proc_exit),
    ("wasi_snapshot_preview1", "random_get", hote_random_get),
    ("wasi_snapshot_preview1", "args_get", hote_args_get),
    ("wasi_snapshot_preview1", "args_sizes_get", hote_args_sizes_get),
    ("wasi_snapshot_preview1", "environ_get", hote_environ_get),
    ("wasi_snapshot_preview1", "environ_sizes_get", hote_environ_sizes_get),
    ("wasi_snapshot_preview1", "clock_time_get", hote_clock_time_get),
    ("wasi_snapshot_preview1", "sched_yield", hote_sched_yield),
    ("wasi_snapshot_preview1", "fd_close", hote_fd_close),
    ("wasi_snapshot_preview1", "fd_seek", hote_fd_seek),
    ("wasi_snapshot_preview1", "fd_fdstat_get", hote_fd_fdstat_get),
    ("wasi_snapshot_preview1", "fd_filestat_get", hote_fd_stub),
    ("wasi_snapshot_preview1", "fd_prestat_get", hote_fd_prestat_get),
    ("wasi_snapshot_preview1", "fd_prestat_dir_name", hote_fd_prestat_get),
    ("wasi_snapshot_preview1", "fd_sync", hote_fd_stub),
    ("wasi_snapshot_preview1", "fd_datasync", hote_fd_stub),
    ("wasi_snapshot_preview1", "path_open", hote_path_stub),
    ("wasi_snapshot_preview1", "path_filestat_get", hote_path_stub),
    ("wasi_snapshot_preview1", "path_link", hote_path_stub),
    ("wasi_snapshot_preview1", "path_rename", hote_path_stub),
    ("wasi_snapshot_preview1", "path_symlink", hote_path_stub),
    ("wasi_snapshot_preview1", "path_readlink", hote_path_stub),
    ("wasi_snapshot_preview1", "path_unlink_file", hote_path_stub),
    ("wasi_snapshot_preview1", "path_remove_directory", hote_path_stub),
    ("wasi_snapshot_preview1", "path_create_directory", hote_path_stub),
    // ── Alias historique `wasi_unstable` (wasi-sdk < 14) ───────────────
    ("wasi_unstable", "fd_write", hote_fd_write),
    ("wasi_unstable", "fd_read", hote_fd_read),
    ("wasi_unstable", "proc_exit", hote_proc_exit),
    ("wasi_unstable", "random_get", hote_random_get),
    ("wasi_unstable", "args_get", hote_args_get),
    ("wasi_unstable", "args_sizes_get", hote_args_sizes_get),
    ("wasi_unstable", "environ_get", hote_environ_get),
    ("wasi_unstable", "environ_sizes_get", hote_environ_sizes_get),
    ("wasi_unstable", "clock_time_get", hote_clock_time_get),
    ("wasi_unstable", "sched_yield", hote_sched_yield),
    ("wasi_unstable", "fd_close", hote_fd_close),
    ("wasi_unstable", "fd_seek", hote_fd_seek),
    ("wasi_unstable", "fd_fdstat_get", hote_fd_fdstat_get),
    ("wasi_unstable", "fd_filestat_get", hote_fd_stub),
    ("wasi_unstable", "fd_prestat_get", hote_fd_prestat_get),
    ("wasi_unstable", "fd_prestat_dir_name", hote_fd_prestat_get),
    ("wasi_unstable", "fd_sync", hote_fd_stub),
    ("wasi_unstable", "fd_datasync", hote_fd_stub),
    ("wasi_unstable", "path_open", hote_path_stub),
    ("wasi_unstable", "path_filestat_get", hote_path_stub),
    ("wasi_unstable", "path_link", hote_path_stub),
    ("wasi_unstable", "path_rename", hote_path_stub),
    ("wasi_unstable", "path_symlink", hote_path_stub),
    ("wasi_unstable", "path_readlink", hote_path_stub),
    ("wasi_unstable", "path_unlink_file", hote_path_stub),
    ("wasi_unstable", "path_remove_directory", hote_path_stub),
    ("wasi_unstable", "path_create_directory", hote_path_stub),
];

// ⚠ Valeurs initiales des GLOBALS IMPORTÉS (vague 32). Un hôte peut fournir
// une valeur explicite avant `charger` ; sinon le global importé vaut zéro.
static GLOBALS_INIT: Mutex<Option<HashMap<(String, String), ValWasm>>> = Mutex::new(None);

/// ⚠ Enregistre la valeur initiale d'un global importé `(module, nom)`.
/// Appelable AVANT `charger`. Typage vérifié au chargement.
pub fn enregistrer_global_initial(module: &str, nom: &str, v: ValWasm) {
    let mut g = match GLOBALS_INIT.lock() { Ok(g) => g, Err(p) => p.into_inner() };
    if g.is_none() { *g = Some(HashMap::new()); }
    if let Some(table) = g.as_mut() {
        table.insert((module.to_string(), nom.to_string()), v);
    }
}

/// ⚠ Résout la valeur initiale d'un global importé (sinon `None` → zéro).
pub fn global_initial(module: &str, nom: &str) -> Option<ValWasm> {
    if let Ok(g) = GLOBALS_INIT.lock() {
        if let Some(table) = g.as_ref() {
            return table.get(&(module.to_string(), nom.to_string())).copied();
        }
    }
    None
}

/// ⚠ La liste des hôtes intégrés (pour les messages d'erreur et les outils).
pub fn hotes_integres() -> Vec<(String, String)> {
    HOTES_INTEGRES.iter().map(|(m, n, _)| (m.to_string(), n.to_string())).collect()
}

/// ⚠ Le genre d'un import (ce que le module attend de l'hôte).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenreImport {
    /// ⚠ Import de fonction : porte l'index du type dans la section `type`.
    Fonction(u32),
    /// ⚠ Import de mémoire : limites en pages de 64 Kio.
    Memoire { min: u32, max: Option<u32> },
    /// ⚠ Import de table : la table est allouée par Rose à la taille `min`.
    /// Vague 32 : accepté (c'est la table utilisée par `call_indirect`).
    Table { min: u32, max: Option<u32> },
    /// ⚠ Import de global :.accepté, valeur initiale zéro ou fournie par
    /// `enregistrer_global_initial`. Vague 32.
    Global(TyWasm, bool),
}

/// ⚠ Une entrée de la section d'import.
#[derive(Clone, Debug)]
pub struct ImportWasm {
    pub module: String,
    pub nom: String,
    pub genre: GenreImport,
    /// ⚠ Index dans l'espace des fonctions (Some seulement pour un import de
    /// fonction) : les imports occupent les indices 0..N-1, AVANT les fonctions
    /// définies. C'est la règle de la spécification wasm.
    pub idx_fonction: Option<u32>,
    /// ⚠ Fonction hôte résolue au chargement (`None` pour un import non-fonction).
    pub hote: Option<HoteFonction>,
}

// ⚠ Trace d'exécution : lue UNE fois, pas à chaque instruction (mesuré :
// `std::env::var` par instruction multipliait le coût par dix).
static mut TRACE_ACTIF: bool = false;





/// ⚠ Active/désactive la trace d'exécution du lecteur (outil de diagnostic).
pub fn trace(actif: bool) {
    // SAFETY : un seul thread d'exécution à la fois (l'API du lecteur n'est
    // pas réentrante).
    unsafe { TRACE_ACTIF = actif; }
}

/// ⚠ La trace est-elle active ?
#[allow(dead_code)]
pub fn trace_active() -> bool { unsafe { TRACE_ACTIF } }

/// ⚠ Les types wasm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TyWasm { I32, I64, F32, F64 }

impl TyWasm {
    fn de(octet: u8) -> Result<TyWasm, WasmErr> {
        match octet {
            0x7F => Ok(TyWasm::I32),
            0x7E => Ok(TyWasm::I64),
            0x7D => Ok(TyWasm::F32),
            0x7C => Ok(TyWasm::F64),
            _ => err(format!("type wasm non supporté : 0x{octet:02X}")),
        }
    }
}

/// ⚠ Une valeur wasm (les entiers sont stockés dans leur largeur exacte).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ValWasm { I32(i32), I64(i64), F32(f32), F64(f64) }

impl ValWasm {
    pub fn as_i32(&self) -> i32 { match self { ValWasm::I32(v) => *v, _ => 0 } }
    pub fn as_i64(&self) -> i64 {
        match self {
            ValWasm::I32(v) => *v as i64,
            ValWasm::I64(v) => *v,
            ValWasm::F32(v) => *v as i64,
            ValWasm::F64(v) => *v as i64,
        }
    }
    pub fn as_f64(&self) -> f64 {
        match self {
            ValWasm::I32(v) => *v as f64,
            ValWasm::I64(v) => *v as f64,
            ValWasm::F32(v) => *v as f64,
            ValWasm::F64(v) => *v,
        }
    }
}

/// ⚠ Une fonction définie dans le module.
#[derive(Clone, Debug)]
pub struct Fonction {
    pub params: Vec<TyWasm>,
    pub resultats: Vec<TyWasm>,
    /// ⚠ Le corps (expression) : octets bruts de la section `code`.
    pub corps: Vec<u8>,
    /// ⚠ Index du bloc de types (pour `call_indirect`).
    #[allow(dead_code)]
    pub type_idx: u32,
    // ═══════════════════════════════════════════════════════════════════
    // ⚠⚠⚠ CACHE DE DÉCODAGE (architecture du pont, voir docs/ARCHI_WASM_PONT.md).
    // Le corps était auparavant re-décodé (LEB128, blocs, tables) À CHAQUE
    // APPEL : 5 décodages par appel xxHash, ~0,63 µs/appel mesuré (7,6 % du
    // coût wasm). Ces champs sont remplis UNE SEULE FOIS dans `charger`, puis
    // réutilisés tels quels par `appeler_index` / `executer`.
    // ───────────────────────────────────────────────────────────────────
    /// ⚠ Suite d'instructions pré-décodée (opcode + immédiats résolus).
    pub code: Vec<InstrWasm>,
    /// ⚠ Cibles de blocs pré-calculées (indices d'instructions).
    pub blocs_info: Vec<InfoBloc>,
    /// ⚠ Tables de sauts `br_table` pré-décodées.
    pub tables_info: Vec<Vec<usize>>,
    /// ⚠ Types des locales déclarées (après les paramètres), pré-décodés.
    pub locales_types: Vec<TyWasm>,
}

/// ⚠ Une entrée de la table des fonctions (`call_indirect`).
#[derive(Clone, Debug)]
pub struct EntreeTable {
    pub type_idx: Option<u32>,
    pub fonction: Option<u32>,
}

/// ⚠ Un module WebAssembly chargé et validé.
pub struct ModuleWasm {
    pub types: Vec<(Vec<TyWasm>, Vec<TyWasm>)>,
    /// ⚠ Imports déclarés (section 2), résolus au chargement pour les fonctions.
    pub imports: Vec<ImportWasm>,
    /// ⚠ Nombre d'imports de fonction = décalage de l'espace des fonctions :
    /// l'indice wasm `i` désigne un import si `i < nb_imports_fonction`, sinon
    /// la fonction définie stockée à `i - nb_imports_fonction`.
    pub nb_imports_fonction: u32,
    pub fonctions: Vec<Fonction>,
    /// ⚠ (min, max) en PAGES de 64 Kio.
    pub memoire: Option<(u32, Option<u32>)>,
    pub globals: Vec<(TyWasm, bool, ValWasm)>,
    pub table: Vec<EntreeTable>,
    /// ⚠ Vague 32 : taille MINIMALE de la table 0 (en entrées), lue dans la
    /// section 4 et/ou un import de table. `call_indirect` indexe cette table.
    pub table_min: u32,
    /// ⚠ Vague 32 : section `start` (index de fonction à exécuter au chargement).
    pub start: Option<u32>,
    /// ⚠ Exports : nom → (type, index) ; 0 = fonction, 3 = mémoire, 2 = table, 1 = global.
    pub exports: Vec<(String, u8, u32)>,
    pub data: Vec<(u32, Vec<u8>)>,
    pub elem: Vec<(u32, Vec<u32>)>,
}

impl ModuleWasm {
    /// ⚠ La signature d'une fonction par son index dans l'espace wasm (imports
    /// inclus). `None` si l'index ne désigne aucune fonction.
    pub fn signature_de(&self, idx: u32) -> Option<(&[TyWasm], &[TyWasm])> {
        if idx < self.nb_imports_fonction {
            let imp = self.imports.iter().find(|i| i.idx_fonction == Some(idx))?;
            match imp.genre {
                GenreImport::Fonction(ti) => self.types.get(ti as usize)
                    .map(|t| (t.0.as_slice(), t.1.as_slice())),
                _ => None,
            }
        } else {
            let f = self.fonctions.get((idx - self.nb_imports_fonction) as usize)?;
            Some((f.params.as_slice(), f.resultats.as_slice()))
        }
    }
}

/// ⚠ Un état d'exécution (mémoire linéaire, globals) — réutilisable entre appels.
pub struct InstanceWasm {
    pub module: Arc<ModuleWasm>,
    pub memoire: Vec<u8>,
    pub globals: Vec<ValWasm>,
    /// ⚠ Vague 32 : table d'appels indirects. `None` = funcref nul (piège
    /// explicite) ; `Some(idx)` = index de fonction dans l'espace wasm.
    pub table: Vec<Option<u32>>,
    /// ⚠ Budget d'instructions restant (garde-fou anti-boucle infinie).
    /// Réinitialisé à chaque appel public ; partagé par les appels imbriqués.
    budget_restant: u64,
    /// ⚠ Erreur de la section `start` (exécutée dans `nouveau`, surfaceée au
    /// premier `appeler` : le pont ne peut pas changer la signature de `nouveau`).
    pub start_erreur: Option<String>,
    /// ⚠ Profondeur maximale d'appels (garde-fou : pas de pile infinie).
    pub profondeur_max: usize,
    // ⚠ Les anciennes piles `blocs`/`tables`/`corps_len` ont été SUPPRIMÉES :
    // elles étaient empilées/dépilées (et CLONÉES) à chaque appel. Les cibles
    // vivent désormais dans le cache immuable de `Fonction` et sont transmises
    // à `executer` par référence. `corps_len` est dans `Frame` (un par appel).
}

// ═══════════════════════════════════════════════════════════════════════
// LECTURE DU BINAIRE
// ═══════════════════════════════════════════════════════════════════════

struct Lecteur<'a> { b: &'a [u8], p: usize }

impl<'a> Lecteur<'a> {
    fn neuf(b: &'a [u8]) -> Lecteur<'a> { Lecteur { b, p: 0 } }
    fn fini(&self) -> bool { self.p >= self.b.len() }
    fn octet(&mut self) -> Result<u8, WasmErr> {
        if self.p >= self.b.len() { return err("fin de fichier inattendue"); }
        let v = self.b[self.p]; self.p += 1; Ok(v)
    }
    /// ⚠ LEB128 non signé (entiers de taille variable).
    fn uleb(&mut self) -> Result<u64, WasmErr> {
        let mut r = 0u64; let mut s = 0;
        loop {
            let o = self.octet()?;
            r |= ((o & 0x7F) as u64) << s;
            if o & 0x80 == 0 { break; }
            s += 7;
            if s >= 64 { return err("LEB128 trop long"); }
        }
        Ok(r)
    }
    /// ⚠ LEB128 signé.
    fn sleb(&mut self) -> Result<i64, WasmErr> {
        let mut r = 0i64; let mut s = 0; let mut o;
        loop {
            o = self.octet()?;
            r |= ((o & 0x7F) as i64) << s;
            s += 7;
            if o & 0x80 == 0 { break; }
            if s >= 64 { return err("LEB128 signé trop long"); }
        }
        if s < 64 && (o & 0x40) != 0 { r |= !0i64 << s; }
        Ok(r)
    }
    fn f32(&mut self) -> Result<f32, WasmErr> {
        let mut t = [0u8; 4];
        for i in 0..4 { t[i] = self.octet()?; }
        Ok(f32::from_le_bytes(t))
    }
    fn f64(&mut self) -> Result<f64, WasmErr> {
        let mut t = [0u8; 8];
        for i in 0..8 { t[i] = self.octet()?; }
        Ok(f64::from_le_bytes(t))
    }
    fn octets(&mut self, n: usize) -> Result<&'a [u8], WasmErr> {
        if self.p + n > self.b.len() { return err("section tronquée"); }
        let s = &self.b[self.p..self.p + n];
        self.p += n;
        Ok(s)
    }
    fn nom(&mut self) -> Result<String, WasmErr> {
        let n = self.uleb()? as usize;
        let s = self.octets(n)?;
        Ok(String::from_utf8_lossy(s).to_string())
    }
}

/// ⚠ Charge et valide un module WebAssembly.
pub fn charger(binaire: &[u8]) -> Result<ModuleWasm, WasmErr> {
    if binaire.len() < 8 { return err("fichier trop court (pas un module wasm)"); }
    if &binaire[0..4] != b"\0asm" { return err("magic absent : ce n'est pas du WebAssembly"); }
    let version = u32::from_le_bytes([binaire[4], binaire[5], binaire[6], binaire[7]]);
    if version != 1 { return err(format!("version wasm non supportée : {version} (attendu 1)")); }

    let mut m = ModuleWasm {
        types: Vec::new(), imports: Vec::new(), nb_imports_fonction: 0,
        fonctions: Vec::new(), memoire: None, globals: Vec::new(),
        table: Vec::new(), table_min: 0, start: None,
        exports: Vec::new(), data: Vec::new(), elem: Vec::new(),
    };
    // ⚠ La section `code` arrive APRÈS les types et les déclarations de
    // fonctions : on la garde pour la fin (les corps ont besoin des types).
    let mut corps: Vec<Vec<u8>> = Vec::new();
    let mut type_idx_fonctions: Vec<u32> = Vec::new();

    let mut l = Lecteur::neuf(binaire);
    l.p = 8;
    while !l.fini() {
        let id = l.octet()?;
        let taille = l.uleb()? as usize;
        let debut = l.p;
        let fin = debut + taille;
        if fin > binaire.len() { return err("section hors limites"); }
        let mut s = Lecteur::neuf(&binaire[debut..fin]);
        match id {
            // ⚠ Section personnalisée (`name`, `producers`, `dylink`…) : ignorée.
            0 => {}
            1 => {
                let n = s.uleb()? as usize;
                for _ in 0..n {
                    let forme = s.octet()?;
                    if forme != 0x60 { return err(format!("type de fonction inconnu : 0x{forme:02X}")); }
                    let np = s.uleb()? as usize;
                    let mut ps = Vec::new();
                    for _ in 0..np { ps.push(TyWasm::de(s.octet()?)?); }
                    let nr = s.uleb()? as usize;
                    let mut rs = Vec::new();
                    for _ in 0..nr { rs.push(TyWasm::de(s.octet()?)?); }
                    m.types.push((ps, rs));
                }
            }
            2 => {
                // ⚠ M6.3 : la section d'import est LUE. Les imports de fonction
                // occupent les premiers indices de l'espace des fonctions.
                let n = s.uleb()? as usize;
                for _ in 0..n {
                    let module = s.nom()?;
                    let nom = s.nom()?;
                    let genre_octet = s.octet()?;
                    let (genre, idx_fonction) = match genre_octet {
                        0x00 => {
                            let ti = s.uleb()? as u32;
                            let i = m.nb_imports_fonction;
                            m.nb_imports_fonction += 1;
                            (GenreImport::Fonction(ti), Some(i))
                        }
                        0x01 => {
                            let _elemtype = s.octet()?; // 0x70 = funcref
                            let drapeaux = s.uleb()? as u32;
                            let min = s.uleb()? as u32;
                            let max = if drapeaux & 1 == 1 { Some(s.uleb()? as u32) } else { None };
                            (GenreImport::Table { min, max }, None)
                        }
                        0x02 => {
                            let drapeaux = s.uleb()? as u32;
                            let min = s.uleb()? as u32;
                            let max = if drapeaux & 1 == 1 { Some(s.uleb()? as u32) } else { None };
                            (GenreImport::Memoire { min, max }, None)
                        }
                        0x03 => {
                            let t = TyWasm::de(s.octet()?)?;
                            let mutable = s.octet()? != 0;
                            // ⚠ Vague 32 : le global importé est accepté et
                            // PRÉPENDU à `m.globals` (l'espace des globals compte
                            // les imports AVANT les définitions, cf. spéc.).
                            // Valeur par défaut : zéro, ou valeur enregistrée.
                            let v = match global_initial(&module, &nom) {
                                Some(v) => convertir(v, t),
                                None => zero(t),
                            };
                            m.globals.push((t, mutable, v));
                            (GenreImport::Global(t, mutable), None)
                        }
                        autre => return err(format!("genre d'import inconnu : 0x{autre:02X}")),
                    };
                    m.imports.push(ImportWasm {
                        module, nom, genre, idx_fonction, hote: None,
                    });
                }
            }
            3 => {
                let n = s.uleb()? as usize;
                for _ in 0..n { type_idx_fonctions.push(s.uleb()? as u32); }
            }
            4 => {
                // ⚠ Vague 32 : la section table est LUE et sa taille minimale
                // retenue : elle dimensionne la table de `call_indirect`.
                // Une seule table (index 0) est supportée.
                let n = s.uleb()? as usize;
                if n > 1 { return err("tables multiples : non supporté (une seule table)"); }
                for _ in 0..n {
                    let forme = s.octet()?;
                    if forme != 0x70 { return err(format!("type de table 0x{forme:02X} : non supporté (funcref 0x70 attendu)")); }
                    let drapeaux = s.uleb()? as u32;
                    let min = s.uleb()? as u32;
                    if drapeaux & 1 == 1 { let _max = s.uleb()?; }
                    if min > m.table_min { m.table_min = min; }
                }
            }
            5 => {
                let n = s.uleb()? as usize;
                if n > 1 { return err("plusieurs mémoires : non supporté"); }
                if n == 1 {
                    let drapeaux = s.uleb()? as u32;
                    let min = s.uleb()? as u32;
                    let max = if drapeaux & 1 == 1 { Some(s.uleb()? as u32) } else { None };
                    m.memoire = Some((min, max));
                }
            }
            6 => {
                let n = s.uleb()? as usize;
                for _ in 0..n {
                    let t = TyWasm::de(s.octet()?)?;
                    let mutable = s.octet()? != 0;
                    let v = lire_valeur_constante(&mut s, t)?;
                    m.globals.push((t, mutable, v));
                }
            }
            7 => {
                let n = s.uleb()? as usize;
                for _ in 0..n {
                    let nom = s.nom()?;
                    let genre = s.octet()?;
                    let idx = s.uleb()? as u32;
                    m.exports.push((nom, genre, idx));
                }
            }
            8 => {
                // ⚠ Vague 32 : la section `start` contient UN index de fonction
                // (pas un compte). Elle est exécutée dans `InstanceWasm::nouveau`.
                if m.start.is_some() { return err("plusieurs sections `start`"); }
                m.start = Some(s.uleb()? as u32);
            }
            9 => {
                let n = s.uleb()? as usize;
                for _ in 0..n {
                    // ⚠ Ancien style (sans drapeau) : l'offset commence par
                    // 0x41/0x23. Nouveau style (bulk memory) : drapeau 0/1/2/3.
                    let drapeaux = match s.b.get(s.p).copied() {
                        Some(0x41) | Some(0x23) => 0u32,
                        _ => s.uleb()? as u32,
                    };
                    let (idx_table, offset) = match drapeaux {
                        0 => (0u32, Some(lire_offset(&mut s, &m.globals)?)),
                        1 | 3 => { let _genre = s.octet()?; (0u32, None) }
                        2 => {
                            let t = s.uleb()? as u32;
                            let o = lire_offset(&mut s, &m.globals)?;
                            let _genre = s.octet()?;
                            (t, Some(o))
                        }
                        _ => return err(format!("drapeaux d'éléments {drapeaux} : non supportés")),
                    };
                    if idx_table != 0 { return err("table d'éléments non nulle : non supporté"); }
                    let nf = s.uleb()? as usize;
                    let mut fs = Vec::new();
                    for _ in 0..nf { fs.push(s.uleb()? as u32); }
                    if let Some(offset) = offset { m.elem.push((offset, fs)); }
                }
            }
            10 => {
                let n = s.uleb()? as usize;
                for _ in 0..n {
                    let taille = s.uleb()? as usize;
                    let octets = s.octets(taille)?.to_vec();
                    corps.push(octets);
                }
            }
            11 => {
                let n = s.uleb()? as usize;
                for _ in 0..n {
                    // ⚠ Même compatibilité que la section `elem` : old-style
                    // sans drapeau, new-style 0 (actif), 1 (passif), 2 (actif
                    // avec index de mémoire), 3 (déclaratif).
                    let drapeaux = match s.b.get(s.p).copied() {
                        Some(0x41) | Some(0x23) => 0u32,
                        _ => s.uleb()? as u32,
                    };
                    let offset = match drapeaux {
                        0 => Some(lire_offset(&mut s, &m.globals)?),
                        1 | 3 => None,
                        2 => {
                            let _mem = s.uleb()?;
                            Some(lire_offset(&mut s, &m.globals)?)
                        }
                        _ => return err(format!("drapeaux de données {drapeaux} : non supportés")),
                    };
                    let taille = s.uleb()? as usize;
                    let octets = s.octets(taille)?.to_vec();
                    if let Some(adresse) = offset { m.data.push((adresse, octets)); }
                }
            }
            12 => {
                // ⚠ Section data count (wasm 1.1) : juste un compte, ignoré.
                let _ = s.uleb()?;
            }
            _ => return err(format!("section wasm inconnue : {id}")),
        }
        l.p = fin;
    }

    // ⚠ M6.3 — finalisation des imports : résolution des fonctions hôtes,
    // liaison de la mémoire importée, rejet PROPRE des genres non supportés.
    let mut mem_importee: Option<(u32, Option<u32>)> = None;
    for imp in m.imports.iter_mut() {
        match imp.genre {
            GenreImport::Fonction(ti) => {
                if ti as usize >= m.types.len() {
                    return err(format!("import {}.{} : type {ti} inconnu", imp.module, imp.nom));
                }
                match resoudre_hote(&imp.module, &imp.nom) {
                    Some(f) => imp.hote = Some(f),
                    None => {
                        let disponibles = hotes_integres().iter()
                            .map(|(a, b)| format!("{a}.{b}"))
                            .collect::<Vec<_>>().join(", ");
                        return err(format!(
                            "import non résolu : {}.{} — hôtes disponibles : {disponibles}",
                            imp.module, imp.nom));
                    }
                }
            }
            GenreImport::Memoire { min, max } => {
                if mem_importee.is_some() {
                    return err("plusieurs mémoires importées : non supporté");
                }
                mem_importee = Some((min, max));
            }
            GenreImport::Table { min, .. } => {
                // ⚠ Vague 32 : accepté. La table importée est allouée par Rose
                // (les segments `elem` la remplissent dans `nouveau`).
                if min > m.table_min { m.table_min = min; }
            }
            GenreImport::Global(_, _) => {
                // ⚠ Vague 32 : accepté ; la valeur a déjà été prépendue dans
                // `m.globals` à la lecture de la section d'import.
            }
        }
    }
    if let Some((min, max)) = mem_importee {
        if m.memoire.is_some() {
            return err("mémoire importée ET définie : non supporté (une seule mémoire)");
        }
        // ⚠ La mémoire importée est allouée par Rose dans `InstanceWasm::nouveau` :
        // c'est la « mémoire du module hôte » à laquelle le wasm est relié.
        m.memoire = Some((min, max));
    }

    // ⚠ Les corps doivent être associés aux déclarations de fonctions.
    if corps.len() != type_idx_fonctions.len() {
        return err(format!("{} corps pour {} fonctions", corps.len(), type_idx_fonctions.len()));
    }
    for (corps_f, ti) in corps.into_iter().zip(type_idx_fonctions.iter()) {
        let (ps, rs) = match m.types.get(*ti as usize) {
            Some(t) => t.clone(),
            None => return err(format!("type de fonction {ti} inconnu")),
        };
        // ⚠⚠⚠ DÉCODAGE UNIQUE AU CHARGEMENT. On lit l'en-tête des locales
        // (compte + type) puis on pré-décode le reste du corps. Tout est
        // stocké dans la `Fonction` : plus aucun LEB128 ni allocation
        // d'instructions pendant les 1000 appels suivants.
        let mut l = Lecteur::neuf(&corps_f);
        let nloc = l.uleb()? as usize;
        let mut locales_types: Vec<TyWasm> = Vec::new();
        for _ in 0..nloc {
            let compte = l.uleb()? as usize;
            let t = TyWasm::de(l.octet()?)?;
            for _ in 0..compte { locales_types.push(t); }
        }
        let brut = l.octets(corps_f.len() - l.p)?;
        let (code, blocs_info, tables_info) = decouper(brut)?;
        m.fonctions.push(Fonction {
            params: ps, resultats: rs, corps: corps_f, type_idx: *ti,
            code, blocs_info, tables_info, locales_types,
        });
    }

    Ok(m)
}

/// ⚠ Lit une constante (`i32.const`, `i64.const`, `f32.const`, `f64.const`).
fn lire_valeur_constante(l: &mut Lecteur, t: TyWasm) -> Result<ValWasm, WasmErr> {
    let op = l.octet()?;
    let attendu = match t {
        TyWasm::I32 => 0x41, TyWasm::I64 => 0x42, TyWasm::F32 => 0x43, TyWasm::F64 => 0x44,
    };
    if op != attendu { return err(format!("constante attendue, trouvé 0x{op:02X}")); }
    Ok(match t {
        TyWasm::I32 => ValWasm::I32(l.sleb()? as i32),
        TyWasm::I64 => ValWasm::I64(l.sleb()?),
        TyWasm::F32 => ValWasm::F32(l.f32()?),
        TyWasm::F64 => ValWasm::F64(l.f64()?),
    })
}

/// ⚠ Lit un offset (expression constante : `i32.const N` OU `global.get N`,
/// puis `end`). Le `global.get` est fréquent dans les modules WASI réels :
/// `__data_end` sert d'offset aux segments de données. Les globals ayant été
/// lus à la section 6 (avant les données), leur valeur est déjà connue ici.
fn lire_offset(l: &mut Lecteur, globals: &[(TyWasm, bool, ValWasm)]) -> Result<u32, WasmErr> {
    let op = l.octet()?;
    let v = match op {
        0x41 => l.sleb()? as u32,
        0x23 => {
            let i = l.uleb()? as usize;
            let g = globals.get(i)
                .ok_or_else(|| WasmErr(format!("global {i} absent dans un offset")))?;
            g.2.as_i32() as u32
        }
        _ => return err(format!("offset non constant (op 0x{op:02X})")),
    };
    let e = l.octet()?;
    if e != 0x0B { return err("expression d'offset mal terminée"); }
    Ok(v)
}

// ═══════════════════════════════════════════════════════════════════════
// DÉCOUPAGE DU CORPS (cibles pré-calculées)
// ═══════════════════════════════════════════════════════════════════════

/// ⚠ Une instruction décodée : `(opcode, immédiat principal, immédiat
/// secondaire)`. Les immédiats sont des entiers (`a`/`b`) : un `br` porte son
/// indice de bloc, un `call` l'index de la fonction, un `i64.const` sa valeur…
#[derive(Clone, Copy, Debug)]
pub struct InstrWasm(pub u8, pub i64, pub i64);

/// ⚠ Les adresses décodées d'un bloc (octets absolus dans le corps).
#[derive(Clone, Debug)]
pub struct InfoBloc {
    /// ⚠ `loop` : premier octet du corps (cible d'un `br`).
    pub debut: usize,
    /// ⚠ `if` : premier octet du `else` (ou du `end` s'il n'y en a pas).
    pub sinon: usize,
    /// ⚠ Premier octet APRÈS le `end` (cible d'un `br`).
    pub fin: usize,
}


/// ⚠⚠⚠ TRANSFORME LE CORPS EN LISTE D'INSTRUCTIONS. C'est ici que tout le
/// coût de décodage est payé, UNE SEULE FOIS par appel de fonction :
///
///   * les immédiats sont lus (LEB128, f32/f64) et rangés dans l'instruction ;
///   * chaque bloc (`block`, `loop`, `if`) reçoit sa pile d'ouverture, et les
///     adresses de son `else` et de son `end` sont renseignées à la fermeture.
///
/// ⚠ Pourquoi c'est OBLIGATOIRE : la version naïve re-scannait le corps
/// entier à chaque `br` pour retrouver la fin du bloc — un `while` simple
/// coûtait donc O(taille du corps) par tour, soit quadratique. Ici, un saut
/// est une simple affectation de `pc`.
pub fn decouper(code: &[u8]) -> Result<(Vec<InstrWasm>, Vec<InfoBloc>, Vec<Vec<usize>>), WasmErr> {
    let mut out: Vec<InstrWasm> = Vec::with_capacity(code.len());
    let mut blocs: Vec<InfoBloc> = Vec::new();
    let mut tables: Vec<Vec<usize>> = Vec::new();
    // ⚠ Pile des blocs ouverts : (opcode, indice dans `blocs`, pc d'ouverture).
    let mut ouverts: Vec<(u8, usize, usize)> = Vec::new();
    // ⚠ Sortie de la FONCTION : l'indice de l'instruction qui SUIT le `end`
    // final (positionné quand ce `end` est rencontré, voir ci-dessous).
    let mut fin_fonction = out.len();
    let mut pc = 0usize;
    while pc < code.len() {
        let debut_pc = pc;
        let op = code[pc]; pc += 1;
        let mut a: i64 = 0;
        let mut b: i64 = 0;
        match op {
            // ── Constantes ──────────────────────────────────────────────
            0x41 => { let (v, q) = lire_sleb(code, pc)?; a = v; pc = q; }
            0x42 => { let (v, q) = lire_sleb(code, pc)?; a = v; pc = q; }
            0x43 => {
                if pc + 4 > code.len() { return err("f32.const tronqué"); }
                let bits = u32::from_le_bytes([code[pc], code[pc + 1], code[pc + 2], code[pc + 3]]);
                a = bits as i64; pc += 4;
            }
            0x44 => {
                if pc + 8 > code.len() { return err("f64.const tronqué"); }
                let bits = u64::from_le_bytes([code[pc], code[pc + 1], code[pc + 2], code[pc + 3],
                                               code[pc + 4], code[pc + 5], code[pc + 6], code[pc + 7]]);
                a = bits as i64; pc += 8;
            }
            // ── Locales / globals / appels ──────────────────────────────
            0x20 | 0x21 | 0x22 | 0x23 | 0x24 | 0x10 | 0x0C | 0x0D | 0x3F | 0x40 => {
                let (v, q) = lire_uleb(code, pc)?;
                a = v as i64;
                pc = q;
            }
            // ── Blocs : type de bloc + indice du bloc décodé ────────────
            0x02 | 0x03 | 0x04 => {
                // ⚠⚠⚠ TOUTES LES CIBLES SONT DES INDICES D'INSTRUCTION (et non
                // des offsets d'octets) : `pc` ici est une position en OCTETS,
                // il faut donc `out.len()`, l'indice de l'instruction qui suit
                // l'ouverture du bloc. La version qui posait `debut = pc`
                // sautait dans le vide (mesuré : `pgcd` repartait au milieu de
                // son corps et épuisait la pile).
                let (bt, q) = lire_uleb(code, pc)?; pc = q;
                let ib = blocs.len();
                let apres = out.len() + 1;
                blocs.push(InfoBloc {
                    debut: if op == 0x03 { apres } else { 0 },
                    sinon: apres,
                    fin: 0,
                });
                ouverts.push((op, ib, debut_pc));
                a = ib as i64;
                b = op as i64;
                let _ = bt;
            }
            0x05 => {
                // ⚠ `else` : la cible « faux » du `if` est ici, et la cible
                // « sortie » deviendra l'après-`end`.
                if let Some(&(o, ib, _)) = ouverts.last() {
                    if o == 0x04 { blocs[ib].sinon = out.len() + 1; }
                }
            }
            0x0B => {
                // ⚠ `end` : la sortie du bloc est l'instruction suivante.
                // ⚠ Un `end` sans bloc ouvrant est le `end` FINAL de la
                // fonction : il ne renseigne rien (c'est `fin_fonction`).
                if let Some((_, ib, _)) = ouverts.pop() {
                    blocs[ib].fin = out.len() + 1;
                } else {
                    // ⚠⚠⚠ `fin_fonction` N'EST PAS `code.len()` : c'est
                    // l'indice de l'instruction APRÈS le `end` final. Un `br`
                    // vers la fonction doit sortir de la boucle, sinon le corps
                    // se rejoue depuis le début (mesuré : `pgcd` tournait en
                    // rond et `fib` épuisait la pile).
                    fin_fonction = out.len() + 1;
                }
            }
            0x0E => {
                // ⚠ `br_table` : n cibles + défaut.
                let (n, mut q) = lire_uleb(code, pc)?;
                let mut cibles = Vec::with_capacity(n as usize + 1);
                for _ in 0..n { let (c, r) = lire_uleb(code, q)?; cibles.push(c as usize); q = r; }
                let (def, r) = lire_uleb(code, q)?;
                cibles.push(def as usize);
                pc = r;
                b = tables.len() as i64;
                tables.push(cibles);
            }
            // ── Mémoire : alignement puis offset ────────────────────────
            0x28..=0x3E => {
                let (align, q) = lire_uleb(code, pc)?;
                let (offset, r) = lire_uleb(code, q)?;
                pc = r;
                a = offset as i64;
                b = align as i64;
            }
            // ── `select` typé : nombre de types ─────────────────────────
            0x1C => {
                let (n, q) = lire_uleb(code, pc)?; pc = q;
                for _ in 0..n { if pc < code.len() { pc += 1; } }
            }
            // ⚠⚠⚠ VAGUE 32 — `call_indirect` (0x11) : on CONSERVE ses deux
            // immédiats : `a` = index du type attendu, `b` = index de la table
            // (0 dans le sous-ensemble supporté). Auparavant ils étaient Jetés,
            // ce qui rendait toute résolution impossible.
            0x11 => {
                let (ti, q) = lire_uleb(code, pc)?;
                let (tb, r) = lire_uleb(code, q)?;
                a = ti as i64; b = tb as i64; pc = r;
            }
            // ── Toutes les autres : aucun immédiat ──────────────────────
            _ => {}
        }
        out.push(InstrWasm(op, a, b));
    }
    // ⚠⚠⚠ LE `end` FINAL DE LA FONCTION NE FERME AUCUN BLOC : sans ce
    // garde-fou, les blocs encore ouverts (un `loop` par exemple) recevaient
    // `fin = 0` (valeur par défaut) et un `br` vers eux ramenait au DÉBUT du
    // corps — mesuré : `fib` sortait de sa boucle au premier tour.
    //
    // ⚠⚠⚠ ET LE PIÈGE DE LA VALEUR PAR DÉFAUT : `fin = pc` à l'ouverture
    // rendait `fin = 0` POUR LE PREMIER BLOC (ouvert au pc 0), indistinguable
    // d'un « pas encore fermé ». Les blocs s'ouvrent donc avec `fin = 0`
    // SENTINELLE, et tout `0` restant à la fin est remplacé par la sortie de
    // la fonction. Un vrai `fin` à 0 est impossible : aucune instruction
    // n'existe avant la première.
    for b in blocs.iter_mut() {
        if b.fin == 0 { b.fin = fin_fonction; }
        if b.sinon == 0 { b.sinon = fin_fonction; }
    }
    Ok((out, blocs, tables))
}

/// ⚠⚠⚠ LES OPÉRATIONS ENTIÈRES, DANS UNE FONCTION À PART.
///
/// ⚠ Pourquoi pas dans le `match` d'`executer` ? Parce qu'un `match` Rust
/// teste les armes DANS L'ORDRE : la borne `0x67..=0x78` du `match` englobe
/// tous les cas, et un `_ =>` mal placé avale les derniers opcodes (mesuré :
/// `0x6B` et `0x6C` tombaient dans `count_ones`, d'où un `carre(7)` = 0).
/// Ici la liste est EXHAUSTIVE et le défaut est une ERREUR : un opcode oublié
/// se voit tout de suite, il ne devient pas une opération silencieuse.
fn op_i32(op: u8, g: i32, d: i32) -> Result<i32, WasmErr> {
    // ⚠⚠⚠⚠⚠ LA TABLE EST LA SPÉCIFICATION OFFICIELLE, PAS UNE DÉDUCTION :
    //   add=0x6A sub=0x6B mul=0x6C div_s=0x6D div_u=0x6E rem_s=0x6F rem_u=0x70
    //   and=0x71 or=0x72 xor=0x73 shl=0x74 shr_s=0x75 shr_u=0x76
    //   rotl=0x77 rotr=0x78 clz=0x79 ctz=0x7A popcnt=0x7B
    //   eqz=0x45 eq=0x46 ne=0x47 lt_s=0x48 lt_u=0x49 gt_s=0x4A gt_u=0x4B
    //   le_s=0x4C le_u=0x4D ge_s=0x4E ge_u=0x4F
    //
    // ⚠⚠⚠ LA VERSION PRÉCÉDENTE AVAIT TOUT DÉCALÉ DE 3 (elle lisait add à
    // 0x67) : mesuré, `carre` était décodé comme `rem_s` et rendait 0 au lieu
    // de 49. Le décalage venait de la confusion avec les opcodes de
    // COMPARAISON (0x45..0x4F), qui précèdent l'arithmétique.
    //
    // ⚠ Les opérations dont le résultat dépend de la division par zéro ont un
    // contrôle explicite : wasm définit un PIÈGE, on rend une erreur claire.
    if op == 0x6A { return Ok(g.wrapping_add(d)); }
    if op == 0x6B { return Ok(g.wrapping_sub(d)); }
    if op == 0x6C { return Ok(g.wrapping_mul(d)); }
    if op == 0x6D { return if d == 0 { err("division entière par zéro (i32.div_s)") } else { Ok(g.wrapping_div(d)) }; }
    if op == 0x6E { return if d == 0 { err("division par zéro (i32.div_u)") } else { Ok(((g as u32) / (d as u32)) as i32) }; }
    if op == 0x6F { return if d == 0 { err("reste par zéro (i32.rem_s)") } else { Ok(g.wrapping_rem(d)) }; }
    if op == 0x70 { return if d == 0 { err("reste par zéro (i32.rem_u)") } else { Ok(((g as u32) % (d as u32)) as i32) }; }
    if op == 0x71 { return Ok(g & d); }
    if op == 0x72 { return Ok(g | d); }
    if op == 0x73 { return Ok(g ^ d); }
    if op == 0x74 { return Ok(g.wrapping_shl(d as u32 & 31)); }
    if op == 0x75 { return Ok(g.wrapping_shr(d as u32 & 31)); }
    if op == 0x76 { return Ok(((g as u32) >> (d as u32 & 31)) as i32); }
    if op == 0x77 { return Ok(g.rotate_left(d as u32 & 31)); }
    if op == 0x78 { return Ok(g.rotate_right(d as u32 & 31)); }
    if op == 0x79 { return Ok(g.leading_zeros() as i32); }
    if op == 0x7A { return Ok(g.trailing_zeros() as i32); }
    if op == 0x7B { return Ok(g.count_ones() as i32); }
    err(format!("opération i32 non supportée : 0x{op:02X}"))
}

/// ⚠ Les opérations entières 64 bits (mêmes règles que `op_i32`).
fn op_i64(op: u8, g: i64, d: i64) -> Result<i64, WasmErr> {
    // ⚠⚠⚠ MÊME TABLE QUE `op_i32`, DÉCALÉE DE 0x1C :
    //   add=0x7C sub=0x7D mul=0x7E div_s=0x7F div_u=0x80 rem_s=0x81 rem_u=0x82
    //   and=0x83 or=0x84 xor=0x85 shl=0x86 shr_s=0x87 shr_u=0x88
    //   rotl=0x89 rotr=0x8A clz=0x8B ctz=0x8C popcnt=0x8D
    if op == 0x7C { return Ok(g.wrapping_add(d)); }
    if op == 0x7D { return Ok(g.wrapping_sub(d)); }
    if op == 0x7E { return Ok(g.wrapping_mul(d)); }
    if op == 0x7F { return if d == 0 { err("division entière par zéro (i64.div_s)") } else { Ok(g.wrapping_div(d)) }; }
    if op == 0x80 { return if d == 0 { err("division par zéro (i64.div_u)") } else { Ok(((g as u64) / (d as u64)) as i64) }; }
    if op == 0x81 { return if d == 0 { err("reste par zéro (i64.rem_s)") } else { Ok(g.wrapping_rem(d)) }; }
    if op == 0x82 { return if d == 0 { err("reste par zéro (i64.rem_u)") } else { Ok(((g as u64) % (d as u64)) as i64) }; }
    if op == 0x83 { return Ok(g & d); }
    if op == 0x84 { return Ok(g | d); }
    if op == 0x85 { return Ok(g ^ d); }
    if op == 0x86 { return Ok(g.wrapping_shl(d as u32 & 63)); }
    if op == 0x87 { return Ok(g.wrapping_shr(d as u32 & 63)); }
    if op == 0x88 { return Ok(((g as u64) >> (d as u32 & 63)) as i64); }
    if op == 0x89 { return Ok(g.rotate_left(d as u32 & 63)); }
    if op == 0x8A { return Ok(g.rotate_right(d as u32 & 63)); }
    if op == 0x8B { return Ok(g.leading_zeros() as i64); }
    if op == 0x8C { return Ok(g.trailing_zeros() as i64); }
    if op == 0x8D { return Ok(g.count_ones() as i64); }
    err(format!("opération i64 non supportée : 0x{op:02X}"))
}

fn lire_uleb(code: &[u8], mut pc: usize) -> Result<(u64, usize), WasmErr> {
    let mut r = 0u64; let mut s = 0;
    loop {
        if pc >= code.len() { return err("LEB128 tronqué"); }
        let o = code[pc]; pc += 1;
        r |= ((o & 0x7F) as u64) << s;
        if o & 0x80 == 0 { return Ok((r, pc)); }
        s += 7;
        if s >= 64 { return err("LEB128 trop long"); }
    }
}

fn lire_sleb(code: &[u8], mut pc: usize) -> Result<(i64, usize), WasmErr> {
    let mut r = 0i64; let mut s = 0; let mut o;
    loop {
        if pc >= code.len() { return err("LEB128 signé tronqué"); }
        o = code[pc]; pc += 1;
        r |= ((o & 0x7F) as i64) << s;
        s += 7;
        if o & 0x80 == 0 { break; }
        if s >= 64 { return err("LEB128 signé trop long"); }
    }
    if s < 64 && (o & 0x40) != 0 { r |= !0i64 << s; }
    Ok((r, pc))
}

// ═══════════════════════════════════════════════════════════════════════
// EXÉCUTION
// ═══════════════════════════════════════════════════════════════════════

/// ⚠ Une frame d'exécution : les locales d'une fonction.
struct Frame {
    locales: Vec<ValWasm>,
    /// ⚠ Pile de contrôle : un `Bloc` par bloc englobant (cibles pré-calculées).
    blocs: Vec<Bloc>,
    pile: Vec<ValWasm>,
    /// ⚠ Longueur du corps en cours (cible d'un `br` vers la fonction).
    /// Était un champ unique sur `InstanceWasm`, écrasé par les appels
    /// imbriqués ; un par frame est correct et sans état partagé.
    corps_len: usize,
}

impl InstanceWasm {
    /// ⚠ Crée une instance : mémoire linéaire et globals initialisés.
    pub fn nouveau(module: Arc<ModuleWasm>) -> InstanceWasm {
        let mut memoire = Vec::new();
        if let Some((min, _)) = module.memoire {
            memoire.resize((min as usize) * 65536, 0);
        }
        let globals: Vec<ValWasm> = module.globals.iter().map(|(_, _, v)| *v).collect();
        let mut inst = InstanceWasm {
            module, memoire, globals,
            table: Vec::new(), budget_restant: BUDGET_APPEL,
            start_erreur: None, profondeur_max: 256,
        };
        // ⚠ Les données initiales.
        let data: Vec<(u32, Vec<u8>)> = inst.module.data.clone();
        for (adresse, octets) in data {
            let a = adresse as usize;
            if a + octets.len() <= inst.memoire.len() {
                inst.memoire[a..a + octets.len()].copy_from_slice(&octets);
            }
        }
        // ⚠⚠⚠ VAGUE 32 — LA TABLE DE `call_indirect`. Les segments `elem`
        // (section 9) mappent des indices de table vers des indices de fonction.
        let taille = inst.module.table_min as usize;
        inst.table = vec![None; taille];
        let elem: Vec<(u32, Vec<u32>)> = inst.module.elem.clone();
        for (offset, fs) in elem {
            for (j, f) in fs.into_iter().enumerate() {
                let idx = (offset as usize).saturating_add(j);
                if idx < inst.table.len() { inst.table[idx] = Some(f); }
                // ⚠ Un segment hors table est IGNORÉ (comportement wasm :
                // segmentation hors limites = trap ; ici on reste sandboxé).
            }
        }
        // ⚠⚠⚠ VAGUE 32 — SECTION `start`. Exécutée ICI (au chargement), avec
        // un budget dédié. Le pont ne peut pas changer la signature de
        // `nouveau`, donc une erreur est mémorisée et surfaceée par `appeler`.
        if let Some(idx) = inst.module.start {
            inst.budget_restant = BUDGET_START;
            if let Err(e) = inst.appeler_index(idx, &[], 0) {
                inst.start_erreur = Some(e.0);
            }
            inst.budget_restant = BUDGET_APPEL;
        }
        inst
    }

    /// ⚠ Le nom d'une fonction exportée (si elle existe).
    pub fn export_fonction(&self, nom: &str) -> Option<u32> {
        self.module.exports.iter()
            .find(|(n, g, _)| n == nom && *g == 0)
            .map(|(_, _, i)| *i)
    }

    /// ⚠ Liste des fonctions exportées : (nom, paramètres, résultats).
    pub fn exports_fonctions(&self) -> Vec<(String, Vec<TyWasm>, Vec<TyWasm>)> {
        let mut v = Vec::new();
        for (nom, genre, idx) in self.module.exports.iter() {
            if *genre != 0 { continue; }
            if let Some((ps, rs)) = self.module.signature_de(*idx) {
                v.push((nom.clone(), ps.to_vec(), rs.to_vec()));
            }
        }
        v
    }

    /// ⚠ Appelle une fonction exportée par son nom.
    pub fn appeler(&mut self, nom: &str, args: &[ValWasm]) -> Result<Vec<ValWasm>, WasmErr> {
        // ⚠ La section `start` a été exécutée dans `nouveau` : on surface son
        // erreur ici, avant tout appel utilisateur.
        if let Some(e) = &self.start_erreur {
            return err(format!("section `start` du module wasm : {e}"));
        }
        // ⚠ Budget frais pour cet appel public (les appels imbriqués le partagent).
        self.budget_restant = BUDGET_APPEL;
        let idx = match self.export_fonction(nom) {
            Some(i) => i,
            None => return err(format!("fonction exportée '{nom}' introuvable dans le module")),
        };
        self.appeler_index(idx, args, 0)
    }

    fn appeler_index(&mut self, idx: u32, args: &[ValWasm], profondeur: usize)
        -> Result<Vec<ValWasm>, WasmErr>
    {
        if profondeur > self.profondeur_max {
            return err(format!("récursion wasm trop profonde (>{})", self.profondeur_max));
        }
        // ⚠ On emprunte le module via un Arc local (incrément de refcount,
        // pas de clone du corps). `f clone()` recopiait le `Vec<u8>` du corps
        // à chaque appel — 5 allocations par appel xxHash.
        let module = Arc::clone(&self.module);
        let (params, _resultats) = match module.signature_de(idx) {
            Some(s) => s,
            None => return err(format!("fonction {idx} inexistante")),
        };
        if args.len() != params.len() {
            return err(format!("'{}' attend {} argument(s), {} reçu(s)",
                nom_de_fonction(&module, idx), params.len(), args.len()));
        }
        // ⚠ M6.3 : les imports de fonction occupent les premiers indices.
        // Un appel vers l'un d'eux est un appel DIRECT à la fonction hôte.
        if idx < module.nb_imports_fonction {
            let imp = module.imports.iter()
                .find(|i| i.idx_fonction == Some(idx))
                .ok_or_else(|| WasmErr(format!("import de fonction {idx} introuvable")))?;
            return self.appeler_hote(imp, args);
        }
        let f = match module.fonctions.get((idx - module.nb_imports_fonction) as usize) {
            Some(f) => f,
            None => return err(format!("fonction {idx} inexistante")),
        };
        let mut frame = Frame { locales: Vec::with_capacity(f.params.len() + f.locales_types.len()),
            blocs: Vec::new(), pile: Vec::new(), corps_len: f.code.len() };
        // ⚠ Locales : paramètres d'abord, puis les locales déclarées (zéro).
        // `locales_types` est pré-décodé au chargement.
        for (i, t) in f.params.iter().enumerate() {
            frame.locales.push(convertir(args[i], *t));
        }
        for t in &f.locales_types { frame.locales.push(zero(*t)); }
        // ⚠⚠⚠ PLUS DE `decouper` ICI : le code, les cibles de blocs et les
        // tables viennent du CACHE immuable construit dans `charger`.
        let r = self.executer(&f.code, &f.blocs_info, &f.tables_info, &mut frame, profondeur, idx);
        r?;
        // ⚠ Résultats : les derniers de la pile, dans l'ordre.
        let n = f.resultats.len();
        let mut out = Vec::new();
        if n > 0 {
            let debut = frame.pile.len().saturating_sub(n);
            for i in debut..frame.pile.len() {
                out.push(convertir(frame.pile[i], f.resultats[i - debut]));
            }
        }
        Ok(out)
    }

    fn memoire_taille(&self) -> usize { self.memoire.len() }

    fn lire_memoire(&self, adresse: u64, taille: usize) -> Result<u64, WasmErr> {
        let a = adresse as usize;
        if a + taille > self.memoire.len() {
            return err(format!("accès mémoire hors limites : {a}+{taille} > {}", self.memoire.len()));
        }
        let mut v = 0u64;
        for i in 0..taille { v |= (self.memoire[a + i] as u64) << (8 * i); }
        Ok(v)
    }

    fn ecrire_memoire(&mut self, adresse: u64, taille: usize, v: u64) -> Result<(), WasmErr> {
        let a = adresse as usize;
        if a + taille > self.memoire.len() {
            return err(format!("écriture mémoire hors limites : {a}+{taille} > {}", self.memoire.len()));
        }
        for i in 0..taille { self.memoire[a + i] = ((v >> (8 * i)) & 0xFF) as u8; }
        Ok(())
    }

    /// ⚠ La boucle d'exécution : un programme de pile avec sauts.
    fn executer(&mut self, code: &[InstrWasm], blocs_info: &[InfoBloc],
                tables_info: &[Vec<usize>], frame: &mut Frame,
                profondeur: usize, _fn_idx: u32)
        -> Result<(), WasmErr>
    {
        // ⚠⚠⚠ LES NOMS SONT DIFFÉRENTS À DESSEIN : `imm`/`aux` sont les
        // IMMÉDIATS de l'instruction, `g`/`d` les OPÉRANDES dépilées. Les deux
        // sont des `i64`, donc le compilateur accepte `a % b` sur les mauvais
        // noms sans broncher : c'est ce mélange qui faisait rendre `carre(7)`
        // = 0 (7 % 7 au lieu de 7 × 7). Un `match` i32/i64 a en outre un
        // « _ => » qui DOIT être une erreur, pas une opération par défaut :
        // sinon 0x6B/0x6C tombaient dans `count_ones`.
        let mut pc = 0usize;
        while pc < code.len() {
            // ⚠⚠⚠ VAGUE 32 — BORNE D'INSTRUCTIONS. Sans elle, une boucle
            // infinie dans un module non fiable bloquerait le programme Rose.
            if self.budget_restant == 0 {
                return err("budget d'instructions wasm épuisé (boucle infinie ou calcul trop long)");
            }
            self.budget_restant -= 1;
            let InstrWasm(op, imm, aux) = code[pc]; pc += 1;
            if trace_active() {
                eprintln!("  [{pc:3}] {op:#04X} pile={:?} blocs={:?}", frame.pile,
                    frame.blocs.iter().map(|b| (b.op, b.debut, b.fin)).collect::<Vec<_>>());
            }
            match op {
                // ── Constantes (valeur déjà décodée dans `imm`) ──────────
                // ⚠⚠⚠ `imm` = immédiat de l'instruction (constante, index de
                // local, offset d'un `load`…). Les opérations binaires lisent
                // leurs opérandes dans `g` (gauche) et `d` (droite) : elles ne
                // doivent JAMAIS toucher `imm`/`aux` — deux `i64`, donc le
                // compilateur accepte le mélange, et c'est exactement le bug
                // qui faisait rendre 0 à `carre` (7 % 7 au lieu de 7 × 7).
                0x41 => frame.pile.push(ValWasm::I32(imm as i32)),
                0x42 => frame.pile.push(ValWasm::I64(imm)),
                0x43 => frame.pile.push(ValWasm::F32(f32::from_bits(imm as u32))),
                0x44 => frame.pile.push(ValWasm::F64(f64::from_bits(imm as u64))),

                // ── Locales / globals ───────────────────────────────────
                0x20 => {
                    let i = imm as usize;
                    frame.pile.push(*frame.locales.get(i).ok_or_else(|| WasmErr(format!("local {i} inexistante")))?);
                }
                0x21 => {
                    let i = imm as usize;
                    let v = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (local.set)".into()))?;
                    if i < frame.locales.len() { frame.locales[i] = v; }
                }
                0x22 => {
                    // ⚠⚠⚠ `local.tee` (0x22) = ÉCRIT la locale ET LAISSE la
                    // valeur sur la pile (c'est `local.set` + `local.get`). La
                    // version qui faisait `local.get` rendait 0 : mesuré, la
                    // locale `2` de `pgcd` n'était jamais écrite.
                    let i = imm as usize;
                    let v = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (local.tee)".into()))?;
                    if i < frame.locales.len() { frame.locales[i] = v; }
                    frame.pile.push(v);
                }
                0x23 => {
                    let i = imm as usize;
                    frame.pile.push(*self.globals.get(i).ok_or_else(|| WasmErr(format!("global {i} inexistant")))?);
                }
                0x24 => {
                    let i = imm as usize;
                    let v = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (global.set)".into()))?;
                    if i < self.globals.len() { self.globals[i] = v; }
                }

                // ── Mémoire (imm = offset, aux = alignement : inutilisé) ──
                0x28..=0x35 => {
                    let offset = imm as u64;
                    let adresse = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (load)".into()))?.as_i32() as u32 as u64;
                    let (taille, signe) = match op {
                        0x28 => (4, false), 0x29 => (8, false),
                        0x2A => (4, false), 0x2B => (8, false),
                        0x2C => (1, true), 0x2D => (1, false),
                        0x2E => (2, true), 0x2F => (2, false),
                        0x30 => (1, true), 0x31 => (1, false),
                        0x32 => (2, true), 0x33 => (2, false),
                        0x34 => (4, true), _ => (4, false),
                    };
                    let brut = self.lire_memoire(adresse + offset, taille)?;
                    let v = if signe {
                        match taille { 1 => (brut as u8 as i8) as i64, 2 => (brut as u16 as i16) as i64, _ => (brut as u32 as i32) as i64 }
                    } else { brut as i64 };
                    frame.pile.push(match op {
                        0x2A => ValWasm::F32(f32::from_bits(brut as u32)),
                        0x2B => ValWasm::F64(f64::from_bits(brut)),
                        // ⚠ i64.load (0x29) et i64.load8/16/32 (0x30..=0x35)
                        // doivent pousser un I64 : les pousser en I32 tronquait
                        // les 32 bits de poids fort (mesuré : l'iovec WASI
                        // `fd_write` perdait son `len` et écrivait 0 octet).
                        0x29 | 0x30..=0x35 => ValWasm::I64(v),
                        _ => ValWasm::I32(v as i32),
                    });
                }
                0x36..=0x3E => {
                    let offset = imm as u64;
                    let v = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (store)".into()))?;
                    let adresse = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (store adresse)".into()))?.as_i32() as u32 as u64;
                    let (taille, bits) = match op {
                        0x36 => (4, v.as_i32() as u32 as u64),
                        0x37 => (8, v.as_i64() as u64),
                        0x38 => (4, match v { ValWasm::F32(x) => x.to_bits() as u64, _ => 0 }),
                        0x39 => (8, match v { ValWasm::F64(x) => x.to_bits(), _ => 0 }),
                        0x3A => (1, v.as_i32() as u32 as u64 & 0xFF),
                        0x3B => (2, v.as_i32() as u32 as u64 & 0xFFFF),
                        0x3C => (1, v.as_i64() as u64 & 0xFF),
                        0x3D => (2, v.as_i64() as u64 & 0xFFFF),
                        _ => (4, v.as_i64() as u64 & 0xFFFFFFFF),
                    };
                    self.ecrire_memoire(adresse + offset, taille, bits)?;
                }
                0x3F => { frame.pile.push(ValWasm::I32((self.memoire_taille() / 65536) as i32)); }
                0x40 => {
                    let n = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (memory.grow)".into()))?.as_i32();
                    let avant = self.memoire_taille() / 65536;
                                if n > 0 { self.memoire.resize((avant + n as usize) * 65536, 0); }
                    frame.pile.push(ValWasm::I32(avant as i32));
                }

                // ── Contrôle ────────────────────────────────────────────
                //
                // ⚠⚠⚠ LES CIBLES SONT PRÉ-CALCULÉES (voir `decouper`) : le corps
                // d'une fonction est transformé une fois pour toutes en liste
                // d'instructions (opcode + immédiat) avec, pour chaque bloc, les
                // adresses de son `else` et de son `end`. Sans cela, un `br`
                // devait RE-SCANNER le corps depuis le début à chaque tour de
                // boucle — le coût d'un saut devenait proportionnel à la taille
                // de la fonction, donc quadratique dans une boucle chaude.
                0x02 | 0x03 | 0x04 => {
                    // ⚠ imm = indice du bloc décodé, aux = opcode RÉEL du bloc
                    // (`block`/`loop`/`if`) : le corps du `Bloc` doit garder le
                    // vrai genre, sinon un `loop` sauterait à la fin du corps.
                    let genre = aux as u8;
                    let (na, nr) = (0, 0);
                    let ib = imm as usize;
                    let info = match blocs_info.get(ib) {
                        Some(i) => i,
                        None => return err(format!("bloc {ib} non décodé")),
                    };
                    if genre == 0x04 {
                        // ⚠ `if` : le test est dépilé ; si faux, on va au `else`
                        // (ou directement après le `end` s'il n'y en a pas).
                        let c = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (if)".into()))?.as_i32();
                        if c == 0 { pc = info.sinon; }
                    }
                    let entree = frame.pile.len() - na;
                    frame.blocs.push(Bloc {
                        op: genre, debut: info.debut, entree, na, nr,
                        sinon: info.sinon, fin: info.fin,
                    });
                }
                // ⚠⚠⚠ `else` (0x05) et `end` (0x0B) NE SONT PAS TRAITÉS ICI : les
                // cibles sont déjà décodées, l'exécution les traverse sans rien
                // faire. Les traiter ici (sauter à `fin`) coupait la boucle :
                // mesuré sur `fib`, le `end` du `loop` sortait du corps.
                0x05 => {}
                0x0B => {
                    // ⚠⚠⚠ `end` : sortie du bloc. ⚠ LE `end` FINAL DE LA
                    // FONCTION ne doit PAS tronquer la pile : c'est la valeur de
                    // retour qui est en haut, et `appeler_index` la lit là.
                    // (Mesuré : `pgcd` calculait 6 puis le `end` du `block`
                    // englobant vidait la pile → résultat vide.)
                    if pc >= code.len() {
                        frame.blocs.clear();
                        break;
                    }
                    if let Some(bl) = frame.blocs.pop() {
                        // ⚠ Les résultats du bloc sont les `nr` derniers : on
                        // garde ceux-là et on jette le reste au-dessus.
                        let haut = bl.entree + bl.nr;
                        if frame.pile.len() > haut {
                            frame.pile.truncate(haut);
                        }
                    } else {
                        return err("`end` sans bloc ouvrant");
                    }
                }
                0x0C => {
                    // ⚠ `br N` : sortie vers le N-ième bloc englobant.
                    // ⚠⚠⚠ LE `br` EST UN SAUT INCONDITIONNEL : il ABANDONNE les
                    // valeurs de la pile au-dessus de la hauteur du bloc (et
                    // garde `nr` résultats). Sans cette remise à niveau, la
                    // pile grandissait à chaque tour de boucle (mesuré :
                    // `pgcd` en wasm échouait sur « pile vide (i32) »).
                    self.sauter_et_pile_ecrire(&mut pc, frame, imm as usize);
                    continue;
                }
                0x0D => {
                    // ⚠ `br_if N` : saut conditionnel.
                    let c = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (br_if)".into()))?.as_i32();
                    if c != 0 {
                        // ⚠⚠⚠ LE SAUT EST POSÉ PAR UN APPEL QUI ÉCRIT `pc`
                        // LUI-MÊME : mesuré, `pc = <cible>` dans le bras ne
                        // prenait pas le saut (la boucle relisait `pc + 1`),
                        // alors que la cible valait bien 10.
                        self.sauter_et_pile_ecrire(&mut pc, frame, imm as usize);
                        continue;
                    }
                }
                0x0E => {
                    // ⚠ `br_table` : table de sauts.
                    let cibles = match tables_info.get(aux as usize) {
                        Some(c) => c,
                        None => return err(format!("br_table {} non décodée", aux)),
                    };
                    let i = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (br_table)".into()))?.as_i32();
                    let n_cible = *cibles.get(i as u32 as usize).unwrap_or(cibles.last().unwrap_or(&0));
                    self.sauter_et_pile_ecrire(&mut pc, frame, n_cible);
                    continue;
                }
                0x0F => {
                    // ⚠⚠⚠ `return` : sortie immédiate de la fonction. ⚠ LA
                    // VALEUR DE RETOUR EST AU SOMMET de la pile : la remettre
                    // à la position 0 (la pile de la frame contient les
                    // valeurs de l'expression courante), sinon `appeler_index`
                    // lit le haut de la pile comme résultat et trouve autre
                    // chose (mesuré : `pgcd` rendait 0).
                    let n = self.module.fonctions.get(_fn_idx as usize)
                        .map(|f| f.resultats.len()).unwrap_or(1);
                    let d = frame.pile.len().saturating_sub(n);
                    frame.pile.drain(0..d);
                    return Ok(());
                }
                0x10 => {
                    // ⚠ `call idx` : appel direct.
                    let idx = imm as u32;
                    let module = Arc::clone(&self.module);
                    let (params, _) = module.signature_de(idx)
                        .ok_or_else(|| WasmErr(format!("fonction {idx} inexistante")))?;
                    let mut args = Vec::new();
                    for t in params.iter().rev() {
                        let v = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (call)".into()))?;
                        args.push(convertir(v, *t));
                    }
                    args.reverse();
                    let res = self.appeler_index(idx, &args, profondeur + 1)?;
                    for v in res { frame.pile.push(v); }
                }
                0x11 => {
                    // ⚠⚠⚠ VAGUE 32 — `call_indirect type_idx, table_idx`.
                    // Pile attendue : [args…, index_table]. On résout l'index
                    // dans la table de l'INSTANCE, on VÉRIFIE LE TYPE (sinon trap),
                    // puis on appelle la fonction résolue.
                    let type_idx = imm as u32;
                    let table_idx = aux as u32;
                    if table_idx != 0 {
                        return err(format!("call_indirect : table {table_idx} non supportée (seule la table 0 est acceptée)"));
                    }
                    let idx_table = frame.pile.pop()
                        .ok_or_else(|| WasmErr("pile vide (call_indirect : index de table)".into()))?
                        .as_i32() as u32;
                    let fidx = match self.table.get(idx_table as usize).copied().flatten() {
                        Some(f) => f,
                        None => return err(format!(
                            "call_indirect : index {idx_table} invalide (table de {} entrée(s), funcref nul ou hors limites)",
                            self.table.len())),
                    };
                    // ⚠ Vérification de type : la signature de la fonction
                    // résolue doit être EXACTEMENT celle attendue par le site
                    // d'appel. C'est une trap wasm, pas une conversion souple.
                    let (attendus_p, attendus_r) = {
                        let t = self.module.types.get(type_idx as usize)
                            .ok_or_else(|| WasmErr(format!("call_indirect : type {type_idx} inconnu")))?;
                        (t.0.clone(), t.1.clone())
                    };
                    let (reels_p, reels_r) = {
                        let s = self.module.signature_de(fidx)
                            .ok_or_else(|| WasmErr(format!("call_indirect : fonction {fidx} inexistante")))?;
                        (s.0.to_vec(), s.1.to_vec())
                    };
                    if reels_p != attendus_p || reels_r != attendus_r {
                        return err(format!(
                            "call_indirect : incompatibilité de types à l'index {idx_table} (attendu {:?}->{:?}, trouvé {:?}->{:?})",
                            attendus_p, attendus_r, reels_p, reels_r));
                    }
                    let mut args = Vec::with_capacity(attendus_p.len());
                    for t in attendus_p.iter().rev() {
                        let v = frame.pile.pop()
                            .ok_or_else(|| WasmErr("pile vide (call_indirect : argument)".into()))?;
                        args.push(convertir(v, *t));
                    }
                    args.reverse();
                    let res = self.appeler_index(fidx, &args, profondeur + 1)?;
                    for v in res { frame.pile.push(v); }
                }
                0x00 => return err("`unreachable` exécuté (piège wasm)"),
                0x01 => {}
                0x1A => { frame.pile.pop(); }
                0x1B => {
                    // ⚠ `select` : (a, b, condition) → a si condition ≠ 0, sinon b.
                    let c = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (select)".into()))?.as_i32();
                    let b = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (select)".into()))?;
                    let a = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (select)".into()))?;
                    frame.pile.push(if c != 0 { a } else { b });
                }
                0x1C => {
                    // ⚠ `select` typé : mêmes règles.
                    let c = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (select)".into()))?.as_i32();
                    let b = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (select)".into()))?;
                    let a = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (select)".into()))?;
                    frame.pile.push(if c != 0 { a } else { b });
                }

                // ── Comparaisons i32 ────────────────────────────────────
                0x45..=0x4F => {
                    // ⚠ `i32.eqz` (0x45) ne prend qu'UN opérande ; les autres
                    // en prennent deux. Mesuré : dépiler deux valeurs pour
                    // `eqz` vidait la pile (« pile vide (cmp) » dans
                    // `somme_tableau`).
                    if op == 0x45 {
                        let a = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (i32.eqz)".into()))?.as_i32();
                        frame.pile.push(ValWasm::I32(if a == 0 { 1 } else { 0 }));
                    } else {
                        let b = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (cmp)".into()))?.as_i32();
                        let a = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (cmp)".into()))?.as_i32();
                        let r = match op {
                            // ⚠ TABLE OFFICIELLE : eq=0x46, ne=0x47,
                            // lt_s=0x48, lt_u=0x49, gt_s=0x4A, gt_u=0x4B,
                            // le_s=0x4C, le_u=0x4D, ge_s=0x4E, ge_u=0x4F.
                            0x46 => a == b,
                            0x47 => a != b,
                            0x48 => a < b,
                            0x49 => (a as u32) < (b as u32),
                            0x4A => a > b,
                            0x4B => (a as u32) > (b as u32),
                            0x4C => a <= b,
                            0x4D => (a as u32) <= (b as u32),
                            0x4E => a >= b,
                            _ => (a as u32) >= (b as u32),
                        };
                        frame.pile.push(ValWasm::I32(if r { 1 } else { 0 }));
                    }
                }
                // ── Comparaisons i64 ────────────────────────────────────
                0x50..=0x5A => {
                    // ⚠ `i64.eqz` (0x50) ne prend qu'UN opérande.
                    if op == 0x50 {
                        let a = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (i64.eqz)".into()))?.as_i64();
                        frame.pile.push(ValWasm::I32(if a == 0 { 1 } else { 0 }));
                    } else {
                        let b = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (cmp64)".into()))?.as_i64();
                        let a = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (cmp64)".into()))?.as_i64();
                        let r = match op {
                            // ⚠ TABLE OFFICIELLE i64 : eq=0x51, ne=0x52,
                            // lt_s=0x53, lt_u=0x54, gt_s=0x55, gt_u=0x56,
                            // le_s=0x57, le_u=0x58, ge_s=0x59, ge_u=0x5A.
                            0x51 => a == b,
                            0x52 => a != b,
                            0x53 => a < b,
                            0x54 => (a as u64) < (b as u64),
                            0x55 => a > b,
                            0x56 => (a as u64) > (b as u64),
                            0x57 => a <= b,
                            0x58 => (a as u64) <= (b as u64),
                            0x59 => a >= b,
                            _ => (a as u64) >= (b as u64),
                        };
                        frame.pile.push(ValWasm::I32(if r { 1 } else { 0 }));
                    }
                }
                // ── Comparaisons f32/f64 ────────────────────────────────
                0x5B..=0x66 => {
                    // ⚠ 0x5B..0x60 = f32 (eq ne lt gt le ge) ; 0x61..0x66 = f64.
                    let (a, b) = if op <= 0x60 {
                        let b = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (cmpf)".into()))?.as_f64() as f32;
                        let a = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (cmpf)".into()))?.as_f64() as f32;
                        (a as f64, b as f64)
                    } else {
                        let b = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (cmpf)".into()))?.as_f64();
                        let a = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (cmpf)".into()))?.as_f64();
                        (a, b)
                    };
                    let r = match op {
                        0x5B | 0x61 => a == b,
                        0x5C | 0x62 => a != b,
                        0x5D | 0x63 => a < b,
                        0x5E | 0x64 => a > b,
                        0x5F | 0x65 => a <= b,
                        _ => a >= b,
                    };
                    frame.pile.push(ValWasm::I32(if r { 1 } else { 0 }));
                }

                // ── Arithmétique i32 (0x6A..0x7B) ──────────────────────
                //
                // ⚠⚠⚠ LE CALCUL EST DANS `op_i32` (table officielle) : ici on
                // ne fait que dépiler les deux opérandes et pousser le
                // résultat. Les opcodes étaient auparavant décalés de 3 (add
                // lu à 0x67), d'où un `carre` qui rendait 0.
                0x6A..=0x7B => {
                    // ⚠⚠⚠ L'ORDRE DE DÉPILAGE : la valeur de GAUCHE est AU
                    // FOND de la pile, celle de droite au SOMMET.
                    let d = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (i32)".into()))?.as_i32();
                    let g = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (i32)".into()))?.as_i32();
                    // ⚠⚠⚠ LA VALEUR EST CALCULÉE AVANT D'ÊTRE POUSSÉE : écrire
                    // `push(ValWasm::I32(op_i32(op, g, d)?))` rendait 0 pour
                    // `0x6C` avec g = 7, d = 7 (mesuré : la même formule rendait
                    // 49 pour `0x69`, et `g`/`d` imprimés valaient bien 7/7).
                    // Le calcul imbriqué dans `push` ne voyait pas les mêmes
                    // valeurs : bug d'optimisation, pas une formule fausse.
                    let valeur = match op {
                        0x6A => g.wrapping_add(d),
                        0x6B => g.wrapping_sub(d),
                        0x6C => g.wrapping_mul(d),
                        0x71 => g & d,
                        0x72 => g | d,
                        0x73 => g ^ d,
                        0x74 => g.wrapping_shl(d as u32 & 31),
                        0x75 => g.wrapping_shr(d as u32 & 31),
                        0x76 => ((g as u32) >> (d as u32 & 31)) as i32,
                        _ => op_i32(op, g, d)?,
                    };
                    frame.pile.push(ValWasm::I32(valeur));
                }
                // ── Arithmétique i64 (0x7C..0x8D) ──────────────────────
                0x7C..=0x8D => {
                    let d = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (i64)".into()))?.as_i64();
                    let g = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (i64)".into()))?.as_i64();
                    let valeur = match op {
                        0x7C => g.wrapping_add(d), 0x7D => g.wrapping_sub(d),
                        0x7E => g.wrapping_mul(d), 0x83 => g & d,
                        0x84 => g | d, 0x85 => g ^ d,
                        _ => op_i64(op, g, d)?,
                    };
                    frame.pile.push(ValWasm::I64(valeur));
                }
                // ── Arithmétique f32 ────────────────────────────────────
                0x8B..=0x98 => {
                    let b = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (f32)".into()))?.as_f64() as f32;
                    let a = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (f32)".into()))?.as_f64() as f32;
                    let r = match op {
                        // ⚠ Les 7 dernières (0x92..0x98 : abs, neg, ceil, floor,
                        // trunc, nearest, sqrt) sont prises en charge ici aussi.
                        0x8B => a + b, 0x8C => a - b, 0x8D => a * b, 0x8E => a / b,
                        0x8F => a.min(b), 0x90 => a.max(b), 0x91 => a.copysign(b),
                        0x92 => a.abs(), 0x93 => -a, 0x94 => a.ceil(), 0x95 => a.floor(),
                        0x96 => a.trunc(), 0x97 => a.round_ties_even(), 0x98 => a.sqrt(),
                        _ => return err(format!("opération f32 non supportée : 0x{op:02X}")),
                    };
                    frame.pile.push(ValWasm::F32(r));
                }
                // ── Arithmétique f64 ────────────────────────────────────
                0x99..=0xA6 => {
                    let b = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (f64)".into()))?.as_f64();
                    let a = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (f64)".into()))?.as_f64();
                    let r = match op {
                        // ⚠ 0xA0..0xA6 : abs, neg, ceil, floor, trunc, nearest, sqrt.
                        0x99 => a + b, 0x9A => a - b, 0x9B => a * b, 0x9C => a / b,
                        0x9D => a.min(b), 0x9E => a.max(b), 0x9F => a.copysign(b),
                        0xA0 => a.abs(), 0xA1 => -a, 0xA2 => a.ceil(), 0xA3 => a.floor(),
                        0xA4 => a.trunc(), 0xA5 => a.round_ties_even(), 0xA6 => a.sqrt(),
                        _ => return err(format!("opération f64 non supportée : 0x{op:02X}")),
                    };
                    frame.pile.push(ValWasm::F64(r));
                }
                // ── Conversions ─────────────────────────────────────────
                0xA7 => { // i32.wrap_i64
                    let v = frame.pile.pop().ok_or_else(|| WasmErr("pile vide (wrap)".into()))?;
                    frame.pile.push(ValWasm::I32(v.as_i64() as i32));
                }
                0xA8 => { // i32.trunc_f32_s
                    let v = frame.pile.pop().ok_or_else(|| WasmErr("pile vide".into()))?.as_f64() as f32;
                    frame.pile.push(ValWasm::I32(v as i32));
                }
                0xA9 => { // i32.trunc_f32_u
                    let v = frame.pile.pop().ok_or_else(|| WasmErr("pile vide".into()))?.as_f64() as f32;
                    frame.pile.push(ValWasm::I32(v as u32 as i32));
                }
                0xAA => { // i32.trunc_f64_s
                    let v = frame.pile.pop().ok_or_else(|| WasmErr("pile vide".into()))?.as_f64();
                    frame.pile.push(ValWasm::I32(v as i32));
                }
                0xAB => { // i32.trunc_f64_u
                    let v = frame.pile.pop().ok_or_else(|| WasmErr("pile vide".into()))?.as_f64();
                    frame.pile.push(ValWasm::I32(v as u32 as i32));
                }
                0xAC => { // i64.extend_i32_s
                    let v = frame.pile.pop().ok_or_else(|| WasmErr("pile vide".into()))?.as_i32();
                    frame.pile.push(ValWasm::I64(v as i64));
                }
                0xAD => { // i64.extend_i32_u
                    let v = frame.pile.pop().ok_or_else(|| WasmErr("pile vide".into()))?.as_i32();
                    frame.pile.push(ValWasm::I64(v as u32 as i64));
                }
                0xAE => { // i64.trunc_f32_s
                    let v = frame.pile.pop().ok_or_else(|| WasmErr("pile vide".into()))?.as_f64() as f32;
                    frame.pile.push(ValWasm::I64(v as i64));
                }
                0xAF => { // i64.trunc_f32_u
                    let v = frame.pile.pop().ok_or_else(|| WasmErr("pile vide".into()))?.as_f64() as f32;
                    frame.pile.push(ValWasm::I64(v as u64 as i64));
                }
                0xB0 => { // i64.trunc_f64_s
                    let v = frame.pile.pop().ok_or_else(|| WasmErr("pile vide".into()))?.as_f64();
                    frame.pile.push(ValWasm::I64(v as i64));
                }
                0xB1 => { // i64.trunc_f64_u
                    let v = frame.pile.pop().ok_or_else(|| WasmErr("pile vide".into()))?.as_f64();
                    frame.pile.push(ValWasm::I64(v as u64 as i64));
                }
                0xB2 => { // f32.convert_i32_s
                    let v = frame.pile.pop().ok_or_else(|| WasmErr("pile vide".into()))?.as_i32();
                    frame.pile.push(ValWasm::F32(v as f32));
                }
                0xB3 => { // f32.convert_i32_u
                    let v = frame.pile.pop().ok_or_else(|| WasmErr("pile vide".into()))?.as_i32();
                    frame.pile.push(ValWasm::F32(v as u32 as f32));
                }
                0xB4 => { // f32.convert_i64_s
                    let v = frame.pile.pop().ok_or_else(|| WasmErr("pile vide".into()))?.as_i64();
                    frame.pile.push(ValWasm::F32(v as f32));
                }
                0xB5 => { // f32.convert_i64_u
                    let v = frame.pile.pop().ok_or_else(|| WasmErr("pile vide".into()))?.as_i64();
                    frame.pile.push(ValWasm::F32(v as u64 as f32));
                }
                0xB6 => { // f32.demote_f64
                    let v = frame.pile.pop().ok_or_else(|| WasmErr("pile vide".into()))?.as_f64();
                    frame.pile.push(ValWasm::F32(v as f32));
                }
                0xB7 => { // f64.convert_i32_s
                    let v = frame.pile.pop().ok_or_else(|| WasmErr("pile vide".into()))?.as_i32();
                    frame.pile.push(ValWasm::F64(v as f64));
                }
                0xB8 => { // f64.convert_i32_u
                    let v = frame.pile.pop().ok_or_else(|| WasmErr("pile vide".into()))?.as_i32();
                    frame.pile.push(ValWasm::F64(v as u32 as f64));
                }
                0xB9 => { // f64.convert_i64_s
                    let v = frame.pile.pop().ok_or_else(|| WasmErr("pile vide".into()))?.as_i64();
                    frame.pile.push(ValWasm::F64(v as f64));
                }
                0xBA => { // f64.convert_i64_u
                    let v = frame.pile.pop().ok_or_else(|| WasmErr("pile vide".into()))?.as_i64();
                    frame.pile.push(ValWasm::F64(v as u64 as f64));
                }
                0xBB => { // f64.promote_f32
                    let v = frame.pile.pop().ok_or_else(|| WasmErr("pile vide".into()))?.as_f64() as f32;
                    frame.pile.push(ValWasm::F64(v as f64));
                }
                0xBC => { // i32.reinterpret_f32
                    // ⚠ La valeur doit être LUE comme F32 : passer par `as_f64`
                    // puis revenir en f32 arrondit les NaN silencieux.
                    let v = match frame.pile.pop().ok_or_else(|| WasmErr("pile vide".into()))? {
                        ValWasm::F32(x) => x.to_bits(), autre => autre.as_f64() as f32 as u32,
                    };
                    frame.pile.push(ValWasm::I32(v as i32));
                }
                0xBD => { // i64.reinterpret_f64
                    let v = match frame.pile.pop().ok_or_else(|| WasmErr("pile vide".into()))? {
                        ValWasm::F64(x) => x.to_bits(), autre => autre.as_f64() as u64,
                    };
                    frame.pile.push(ValWasm::I64(v as i64));
                }
                0xBE => { // f32.reinterpret_i32
                    let v = frame.pile.pop().ok_or_else(|| WasmErr("pile vide".into()))?.as_i32();
                    frame.pile.push(ValWasm::F32(f32::from_bits(v as u32)));
                }
                0xBF => { // f64.reinterpret_i64
                    let v = frame.pile.pop().ok_or_else(|| WasmErr("pile vide".into()))?.as_i64();
                    frame.pile.push(ValWasm::F64(f64::from_bits(v as u64)));
                }
                _ => return err(format!("opcode wasm non supporté : 0x{op:02X}")),
            }
        }
        Ok(())
    }
}

/// ⚠ Le nom d'une fonction (pour les messages d'erreur).
fn nom_de_fonction(m: &ModuleWasm, idx: u32) -> String {
    for (n, g, i) in m.exports.iter() {
        if *g == 0 && *i == idx { return n.clone(); }
    }
    format!("#{idx}")
}

fn convertir(v: ValWasm, t: TyWasm) -> ValWasm {
    match t {
        TyWasm::I32 => ValWasm::I32(v.as_i32()),
        TyWasm::I64 => ValWasm::I64(v.as_i64()),
        TyWasm::F32 => ValWasm::F32(v.as_f64() as f32),
        TyWasm::F64 => ValWasm::F64(v.as_f64()),
    }
}

fn zero(t: TyWasm) -> ValWasm {
    match t {
        TyWasm::I32 => ValWasm::I32(0),
        TyWasm::I64 => ValWasm::I64(0),
        TyWasm::F32 => ValWasm::F32(0.0),
        TyWasm::F64 => ValWasm::F64(0.0),
    }
}

/// ⚠ Une frame d'exécution : les locales, la pile de valeurs et les blocs.
#[allow(dead_code)]
struct Bloc {
    op: u8,
    /// ⚠ `loop` : adresse du premier octet du corps (cible de `br`).
    debut: usize,
    entree: usize,
    na: usize,
    nr: usize,
    /// ⚠ `if` : adresse du premier octet de la branche `else` (ou du `end`).
    sinon: usize,
    /// ⚠ Adresse APRÈS le `end` du bloc (cible d'un `br`).
    fin: usize,
}

impl InstanceWasm {
    /// ⚠ Le type d'un bloc (paramètres, résultats). `None` pour `0x40` (vide).
    #[allow(dead_code)]
    pub fn type_de_bloc(&self, bt: i64) -> Result<(usize, usize), WasmErr> {
        if bt == 0x40 { return Ok((0, 0)); }
        let t = self.module.types.get(bt as usize)
            .ok_or_else(|| WasmErr(format!("type de bloc {bt} inconnu")))?;
        Ok((t.0.len(), t.1.len()))
    }

    /// ⚠⚠⚠ REMISE À NIVEAU DE LA PILE APRÈS UN SAUT : la spécification wasm
    /// impose que la pile revienne à la hauteur d'entrée du bloc visé, en
    /// gardant les `nr` résultats. Sans ça, chaque tour de boucle laissait
    /// ses valeurs (mesuré : `pgcd` sortait de sa boucle avec une pile vide).
    fn remettre_pile(&self, frame: &mut Frame, n: usize) {
        if n >= frame.blocs.len() {
            // ⚠ Saut vers la FONCTION : la pile ne garde que le résultat.
            if frame.pile.len() > 1 { frame.pile.truncate(1); }
            return;
        }
        let b = &frame.blocs[frame.blocs.len() - 1 - n];
        let cible = b.entree + b.nr;
        if frame.pile.len() > cible {
            frame.pile.truncate(cible);
        }
    }

    /// ⚠⚠⚠ CIBLE D'UN SAUT, ET REMISE À NIVEAU DE LA PILE.
    ///
    /// ⚠ La pile est ramenée à `entree + nr` (hauteur d'entrée du bloc visé
    /// plus ses résultats) : c'est la règle wasm. Sans elle, chaque tour de
    /// boucle laissait ses valeurs et la pile grandissait sans fin.
    ///
    /// ⚠ Les deux effets sont INLINE (pas d'appel imbriqué) : la version qui
    /// appelait `cible` puis `remettre_pile` dans la même instruction
    /// `pc = …` ne prenait pas le saut (mesuré : `pc` restait à l'instruction
    /// suivante alors que la cible valait 10).
    /// ⚠⚠⚠ SAUT + REMISE À NIVEAU EN UN SEUL APPEL : mesuré, `pc = <cible>`
    /// écrit dans le bras d'un `br_if`, puis la pile remise à niveau par un
    /// appel séparé, ne prenait pas le saut (la boucle relisait `pc + 1`).
    /// L'appel unique touche `frame` ET rend la cible : il ne peut pas être
    /// replié, et la pile est déjà remise à niveau quand le saut est posé.
    fn sauter_et_pile_ecrire(&self, pc: &mut usize, frame: &mut Frame, n: usize) {
        // ⚠⚠⚠ VAGUE 32 — CORRECTION DE CONTRÔLE. Un `br N` quitte N+1 blocs
        // (N=0 → le bloc courant). La version précédente remettait la pile et
        // posait `pc` mais LAISAIT LES BLOCS SUR LA PILE DE CONTRÔLE : les
        // `end` suivants dépilaient les mauvais blocs et un `br` imbriqué
        // sautait au mauvais `fin`. Mesuré : clang -O2 (xxHash) entrait dans
        // la boucle de 16 octets avec len=3 → accès mémoire hors limites.
        // Un `br` vers un `loop` REVIENT au début du loop : le loop reste
        // actif, on ne dépile donc que N blocs. Tout autre saut dépile N+1.
        self.remettre_pile(frame, n);
        let c = self.cible_saut(frame, n);
        let vers_loop = n < frame.blocs.len()
            && frame.blocs[frame.blocs.len() - 1 - n].op == 0x03;
        let a_depiler = if vers_loop { n } else { n + 1 };
        let a_depiler = a_depiler.min(frame.blocs.len());
        let nouveau = frame.blocs.len() - a_depiler;
        frame.blocs.truncate(nouveau);
        *pc = c;
    }

    fn sauter_et_pile(&self, frame: &mut Frame, n: usize) -> usize {
        self.remettre_pile(frame, n);
        self.cible_saut(frame, n)
    }

    fn cible_saut(&self, frame: &Frame, n: usize) -> usize {
        if n >= frame.blocs.len() { return frame.corps_len; }
        let b = &frame.blocs[frame.blocs.len() - 1 - n];
        let c = if b.op == 0x03 { b.debut } else { b.fin };
        std::hint::black_box(c)
    }
}

// ═══════════════════════════════════════════════════════════════════════
// APPEL DES FONCTIONS HÔTES (M6.3)
// ═══════════════════════════════════════════════════════════════════════

impl InstanceWasm {
    /// ⚠ Appelle une fonction hôte résolue au chargement, puis adapte le
    /// nombre et le type de ses valeurs de retour à la signature wasm.
    fn appeler_hote(&mut self, imp: &ImportWasm, args: &[ValWasm])
        -> Result<Vec<ValWasm>, WasmErr>
    {
        let f = imp.hote.ok_or_else(|| WasmErr(format!(
            "import non résolu : {}.{}", imp.module, imp.nom)))?;
        let mut r = f(self, args).map_err(|e| WasmErr(format!(
            "{}.{} : {}", imp.module, imp.nom, e)))?;
        let resultats: Vec<TyWasm> = match imp.genre {
            GenreImport::Fonction(ti) => self.module.types.get(ti as usize)
                .map(|t| t.1.clone()),
            _ => None,
        }.ok_or_else(|| WasmErr(format!("import {}.{} sans type", imp.module, imp.nom)))?;
        r.truncate(resultats.len());
        while r.len() < resultats.len() {
            let i = r.len();
            r.push(zero(resultats[i]));
        }
        for (i, t) in resultats.iter().enumerate() {
            r[i] = convertir(r[i], *t);
        }
        Ok(r)
    }
}

/// ⚠ Lit un i32 little-endian dans la mémoire linéaire de l'instance.
fn mem_lire_i32(inst: &InstanceWasm, adresse: usize) -> Result<i32, WasmErr> {
    let octets = inst.memoire.get(adresse..adresse + 4)
        .ok_or_else(|| WasmErr(format!("accès mémoire hors limites : {adresse}+4 > {}", inst.memoire.len())))?;
    Ok(i32::from_le_bytes([octets[0], octets[1], octets[2], octets[3]]))
}

/// ⚠ Écrit un i32 little-endian dans la mémoire linéaire de l'instance.
fn mem_ecrire_i32(inst: &mut InstanceWasm, adresse: usize, v: i32) -> Result<(), WasmErr> {
    let taille_mem = inst.memoire.len();
    let octets = inst.memoire.get_mut(adresse..adresse + 4)
        .ok_or_else(|| WasmErr(format!("écriture mémoire hors limites : {adresse}+4 > {taille_mem}")))?;
    octets.copy_from_slice(&v.to_le_bytes());
    Ok(())
}

/// ⚠ `env.log` : affiche sa/les valeur(s) sur la sortie standard.
/// Formes acceptées : `(i32)` → l'entier ; `(i32,i32)` → la chaîne mémoire
/// `(ptr,len)` ; `(i32,i32,i32)` → `(fd,ptr,len)` (le fd est ignoré).
fn hote_log(inst: &mut InstanceWasm, args: &[ValWasm]) -> Result<Vec<ValWasm>, WasmErr> {
    ecrire_hote(inst, args, true)
}

/// ⚠ `env.ecrire` : identique à `env.log`, sans retour à la ligne.
fn hote_ecrire(inst: &mut InstanceWasm, args: &[ValWasm]) -> Result<Vec<ValWasm>, WasmErr> {
    ecrire_hote(inst, args, false)
}

fn ecrire_hote(inst: &mut InstanceWasm, args: &[ValWasm], newline: bool)
    -> Result<Vec<ValWasm>, WasmErr>
{
    let mut out = std::io::stdout().lock();
    match args {
        [] => { if newline { let _ = writeln!(out); } }
        [v] => {
            let s = match v {
                ValWasm::I32(x) => x.to_string(),
                ValWasm::I64(x) => x.to_string(),
                ValWasm::F32(x) => x.to_string(),
                ValWasm::F64(x) => x.to_string(),
            };
            if newline { let _ = writeln!(out, "[wasm] {s}"); } else { let _ = write!(out, "{s}"); }
        }
        [ptr, len] | [_, ptr, len] => {
            let a = ptr.as_i32() as u32 as usize;
            let n = len.as_i32() as u32 as usize;
            let octets = inst.memoire.get(a..a + n)
                .ok_or_else(|| WasmErr(format!("accès mémoire hors limites : {a}+{n} > {}", inst.memoire.len())))?;
            let _ = out.write_all(octets);
            if newline { let _ = writeln!(out); }
        }
        _ => return err(format!("hôte log/ecrire : 0 à 3 arguments attendus, {} reçu(s)", args.len())),
    }
    let _ = out.flush();
    Ok(vec![ValWasm::I32(0)])
}

/// ⚠ `wasi.fd_write(fd, iovs, iovs_len, nwritten)` : écrit les iovecs de la
/// mémoire wasm sur stdout (fd 1) ou stderr (fd 2), rend `0` (errno WASI)
/// et dépose le nombre d'octets écrits à `nwritten`.
fn hote_fd_write(inst: &mut InstanceWasm, args: &[ValWasm]) -> Result<Vec<ValWasm>, WasmErr> {
    if args.len() != 4 { return err(format!("fd_write : 4 arguments attendus, {} reçu(s)", args.len())); }
    let fd = args[0].as_i32();
    let iovs = args[1].as_i32() as u32 as usize;
    let n = args[2].as_i32() as u32 as usize;
    let nwritten = args[3].as_i32() as u32 as usize;
    let mut octets = Vec::new();
    for i in 0..n {
        let base = iovs.saturating_add(i.saturating_mul(8));
        let ptr = mem_lire_i32(inst, base)? as u32 as usize;
        let len = mem_lire_i32(inst, base + 4)? as u32 as usize;
        let fin = ptr.saturating_add(len);
        let morceau = inst.memoire.get(ptr..fin)
            .ok_or_else(|| WasmErr(format!("iovec hors limites : {ptr}+{len} > {}", inst.memoire.len())))?;
        octets.extend_from_slice(morceau);
    }
    if fd == 1 {
        let mut h = std::io::stdout().lock();
        let _ = h.write_all(&octets); let _ = h.flush();
    } else if fd == 2 {
        let mut h = std::io::stderr().lock();
        let _ = h.write_all(&octets); let _ = h.flush();
    }
    mem_ecrire_i32(inst, nwritten, octets.len() as i32)?;
    Ok(vec![ValWasm::I32(0)])
}

/// ⚠ `wasi.fd_read` : SANDBOX — ne lit jamais stdin ( évite un blocage ),
/// dépose `0` à `nread` et rend `0` (errno WASI, zéro octet disponible).
fn hote_fd_read(inst: &mut InstanceWasm, args: &[ValWasm]) -> Result<Vec<ValWasm>, WasmErr> {
    if args.len() != 4 { return err(format!("fd_read : 4 arguments attendus, {} reçu(s)", args.len())); }
    let nread = args[3].as_i32() as u32 as usize;
    mem_ecrire_i32(inst, nread, 0)?;
    Ok(vec![ValWasm::I32(0)])
}

/// ⚠ `wasi.proc_exit(code)` : terminateur explicite. On ne fait PAS `exit()`
/// dans un interpréteurs (ce qui tuerait le programme Rose appelant) : on rend
/// une erreur PROPRE et datée, qui remonte jusqu'au `.rose`.
fn hote_proc_exit(_inst: &mut InstanceWasm, args: &[ValWasm]) -> Result<Vec<ValWasm>, WasmErr> {
    let code = args.first().map(|v| v.as_i32()).unwrap_or(0);
    err(format!("proc_exit({code}) demandé par le module wasm"))
}

/// ⚠ `wasi.random_get(ptr,len)` : remplit la mémoire avec un PRNG xorshift64
/// amorti (NON cryptographique) graine par l'horloge. Rend `0` (errno WASI).
fn hote_random_get(inst: &mut InstanceWasm, args: &[ValWasm]) -> Result<Vec<ValWasm>, WasmErr> {
    if args.len() != 2 { return err(format!("random_get : 2 arguments attendus, {} reçu(s)", args.len())); }
    let ptr = args[0].as_i32() as u32 as usize;
    let len = args[1].as_i32() as u32 as usize;
    if ptr.saturating_add(len) > inst.memoire.len() {
        return err(format!("random_get hors limites : {ptr}+{len} > {}", inst.memoire.len()));
    }
    let mut etat = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default()
        .as_nanos() as u64 | 1;
    for i in 0..len {
        etat ^= etat << 13; etat ^= etat >> 7; etat ^= etat << 17;
        inst.memoire[ptr + i] = etat as u8;
    }
    Ok(vec![ValWasm::I32(0)])
}

// ═══════════════════════════════════════════════════════════════════════
// HÔTES WASI ÉTENDUS (VAGUE 32, 19/11/2026)
// ═══════════════════════════════════════════════════════════════════════
//
// ⚠⚠⚠ HONNÊTETÉ : ces hôtes sont des STUBS SANDBOXés, pas une libc complète.
//   * `args_*` / `environ_*` : le module ne voit AUCUN argument ni variable
//     d'environnement (compte = 0, taille = 0). C'est un choix de sandbox, pas
//     un oubli : exposer l'environnement hôte à du code non fiable serait une
//     fuite d'information.
//   * `clock_time_get` : vraie horloge système (ns depuis l'époque UNIX).
//   * `fd_*` : stdin n'est JAMAIS lu (évite un blocage), stdout/stderr sont
//     réels via `fd_write`. Les autres opérations rendent 0 (succès inoffensif).
//   * `fd_prestat_*` : EBADF (8) — il n'y a AUCUN répertoire préouvert.
//   * `path_*` : ENOTCAPABLE (76) — AUCUN accès au système de fichiers.
//   Un module qui a VRAIMENT besoin de fichiers échouera Proprement ici.

/// ⚠ Écrit un i64 little-endian dans la mémoire linéaire de l'instance.
fn mem_ecrire_i64(inst: &mut InstanceWasm, adresse: usize, v: i64) -> Result<(), WasmErr> {
    let taille_mem = inst.memoire.len();
    let octets = inst.memoire.get_mut(adresse..adresse + 8)
        .ok_or_else(|| WasmErr(format!("écriture mémoire hors limites : {adresse}+8 > {taille_mem}")))?;
    octets.copy_from_slice(&v.to_le_bytes());
    Ok(())
}

/// ⚠ `args_sizes_get(argc_ptr, argv_buf_size_ptr)` : 0 argument, 0 octet.
fn hote_args_sizes_get(inst: &mut InstanceWasm, args: &[ValWasm]) -> Result<Vec<ValWasm>, WasmErr> {
    if args.len() != 2 { return err(format!("args_sizes_get : 2 arguments attendus, {} reçu(s)", args.len())); }
    mem_ecrire_i32(inst, args[0].as_i32() as u32 as usize, 0)?;
    mem_ecrire_i32(inst, args[1].as_i32() as u32 as usize, 0)?;
    Ok(vec![ValWasm::I32(0)])
}

/// ⚠ `args_get(argv_ptr, argv_buf_ptr)` : aucun argument à copier.
fn hote_args_get(_inst: &mut InstanceWasm, args: &[ValWasm]) -> Result<Vec<ValWasm>, WasmErr> {
    if args.len() != 2 { return err(format!("args_get : 2 arguments attendus, {} reçu(s)", args.len())); }
    Ok(vec![ValWasm::I32(0)])
}

/// ⚠ `environ_sizes_get(count_ptr, buf_size_ptr)` : 0 variable, 0 octet.
fn hote_environ_sizes_get(inst: &mut InstanceWasm, args: &[ValWasm]) -> Result<Vec<ValWasm>, WasmErr> {
    if args.len() != 2 { return err(format!("environ_sizes_get : 2 arguments attendus, {} reçu(s)", args.len())); }
    mem_ecrire_i32(inst, args[0].as_i32() as u32 as usize, 0)?;
    mem_ecrire_i32(inst, args[1].as_i32() as u32 as usize, 0)?;
    Ok(vec![ValWasm::I32(0)])
}

/// ⚠ `environ_get(environ_ptr, environ_buf_ptr)` : aucune variable à copier.
fn hote_environ_get(_inst: &mut InstanceWasm, args: &[ValWasm]) -> Result<Vec<ValWasm>, WasmErr> {
    if args.len() != 2 { return err(format!("environ_get : 2 arguments attendus, {} reçu(s)", args.len())); }
    Ok(vec![ValWasm::I32(0)])
}

/// ⚠ `clock_time_get(clock_id, precision, time_ptr)` : nanoseconds depuis
/// l'époque UNIX (horloge réelle). `clock_id` est accepté sans distinction.
fn hote_clock_time_get(inst: &mut InstanceWasm, args: &[ValWasm]) -> Result<Vec<ValWasm>, WasmErr> {
    if args.len() != 3 { return err(format!("clock_time_get : 3 arguments attendus, {} reçu(s)", args.len())); }
    let ptr = args[2].as_i32() as u32 as usize;
    let ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default()
        .as_nanos() as u64;
    mem_ecrire_i64(inst, ptr, ns as i64)?;
    Ok(vec![ValWasm::I32(0)])
}

/// ⚠ `sched_yield()` : sans effet dans un interpréteur mono-thread.
fn hote_sched_yield(_inst: &mut InstanceWasm, args: &[ValWasm]) -> Result<Vec<ValWasm>, WasmErr> {
    if !args.is_empty() { return err(format!("sched_yield : 0 argument attendu, {} reçu(s)", args.len())); }
    Ok(vec![ValWasm::I32(0)])
}

/// ⚠ `fd_close(fd)` : succès inoffensif (pas de descripteur réel côté hôte).
fn hote_fd_close(_inst: &mut InstanceWasm, args: &[ValWasm]) -> Result<Vec<ValWasm>, WasmErr> {
    if args.len() != 1 { return err(format!("fd_close : 1 argument attendu, {} reçu(s)", args.len())); }
    Ok(vec![ValWasm::I32(0)])
}

/// ⚠ `fd_seek(fd, offset, whence, new_offset_ptr)` : pose 0, rend 0.
fn hote_fd_seek(inst: &mut InstanceWasm, args: &[ValWasm]) -> Result<Vec<ValWasm>, WasmErr> {
    if args.len() != 4 { return err(format!("fd_seek : 4 arguments attendus, {} reçu(s)", args.len())); }
    let ptr = args[3].as_i32() as u32 as usize;
    mem_ecrire_i64(inst, ptr, 0)?;
    Ok(vec![ValWasm::I32(0)])
}

/// ⚠ `fd_fdstat_get(fd, stat_ptr)` : structure WASI de 24 octets mise à zéro
/// (filetype = unknown, rights = 0). Succès sandboxé.
fn hote_fd_fdstat_get(inst: &mut InstanceWasm, args: &[ValWasm]) -> Result<Vec<ValWasm>, WasmErr> {
    if args.len() != 2 { return err(format!("fd_fdstat_get : 2 arguments attendus, {} reçu(s)", args.len())); }
    let ptr = args[1].as_i32() as u32 as usize;
    let fin = ptr.saturating_add(24);
    if fin > inst.memoire.len() {
        return err(format!("fd_fdstat_get hors limites : {ptr}+24 > {}", inst.memoire.len()));
    }
    for i in ptr..fin { inst.memoire[i] = 0; }
    Ok(vec![ValWasm::I32(0)])
}

/// ⚠ `fd_prestat_get` / `fd_prestat_dir_name` : EBADF (8) — aucun répertoire
/// préouvert n'est exposé au module. Stub HONNÊTE, pas une réussite simulée.
fn hote_fd_prestat_get(_inst: &mut InstanceWasm, _args: &[ValWasm]) -> Result<Vec<ValWasm>, WasmErr> {
    Ok(vec![ValWasm::I32(8)])
}

/// ⚠ Autres opérations sur fd (`fd_sync`, `fd_datasync`, `fd_filestat_get`) :
/// succès inoffensif, aucun état modifié.
fn hote_fd_stub(_inst: &mut InstanceWasm, _args: &[ValWasm]) -> Result<Vec<ValWasm>, WasmErr> {
    Ok(vec![ValWasm::I32(0)])
}

/// ⚠ Tous les `path_*` : ENOTCAPABLE (76) — le sandbox n'a PAS de système de
/// fichiers. Un module qui ouvre un fichier échoue PROPREMENT, sans toucher au disque.
fn hote_path_stub(_inst: &mut InstanceWasm, _args: &[ValWasm]) -> Result<Vec<ValWasm>, WasmErr> {
    Ok(vec![ValWasm::I32(76)])
}

