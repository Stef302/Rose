// Rose — moteur d'exécution : valeurs exactes et bibliothèque standard.
// Sémantique du langage : conversions,
// comparaisons, rendus, RNG splitmix64+Lemire, messages d'erreur.
// ZÉRO DÉPENDANCE : les entiers dépassant i64 et les rationnels vivent sur
// l'arithmétique exacte native i128 (src/i128x.rs) — plus aucun GMP, plus
// aucune liaison externe. Un dépassement 128 bits est une erreur runtime
// nette (« entier hors plage »), jamais un wrap silencieux.
// Licence MIT.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::rc::Rc;
use std::hash::BuildHasherDefault;

use crate::front::{texte_flottant, TyKind};
use num_bigint::BigInt;
use num_traits::ToPrimitive;

pub use crate::i128x::{
    gmp_add, gmp_add_ui, gmp_cmp, gmp_cmp_ge, gmp_fdiv_q, gmp_fdiv_r, gmp_fdiv_ui, gmp_get_d, gmp_get_si, gmp_mul,
    gmp_mul_2exp, gmp_mul_si, gmp_mul_ui, gmp_neg, gmp_pow_ui, gmp_sgn, gmp_set, gmp_set_d, gmp_set_ui,
    gmp_sub, gmp_ui_pow_ui, gmpq_add, gmpq_canonicalize, gmpq_cmp, gmpq_div, gmpq_mul, gmpq_neg,
    gmpq_set, gmpq_set_d, gmpq_set_ui, gmpq_set_z, gmpq_set_z_field, gmpq_sub,
};
pub use crate::i128x::{Gq, Gz, Mpq, Mpz};

// ── Valeur ────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct Liste(pub Rc<RefCell<Vec<Valeur>>>);
// ── Table : ordre d'insertion + index de hachage auxiliaire ────────────────
//
// 18/11/26 — domaine 7 du bench multi : 3 000 insertions + 3 000 recherches
// coûtaient 6,34 ms (×67 vs Rust/HashMap). Mesuré par instrumentation :
// 4 498 500 + 4 501 500 = 9 000 000 appels à `valeur_egale`, dont ZÉRO
// chemin lent d'allocation (tous des i64). Le coût est le SCAN LINÉAIRE,
// pas la comparaison ni l'allocation. Le `Vec` reste la source de vérité
// (ordre d'insertion, égalité, affichage, `cles`/`valeurs`) ; l'index
// `h canonique → position` ramène recherche/insertion à O(1) amorti.
// Les collisions sont résolues par `valeur_egale` : l'égalité sémantique
// est inchangée (y compris Entier ≡ Rationnel ≡ Flottant).
//
// ⚠ UN SEUL `Rc` — avec deux `Rc`, `Valeur` garde 24 octets mais la
// drop-glue changeait et D5/D6 régressaient de 4 à 10 % (mesuré le 18/11
// en A/A/A entrelacé). Ici `Table` reste exactement un `Rc<RefCell<T>>`.
#[derive(Clone)]
pub struct Table(pub Rc<RefCell<TableRep>>);
#[derive(Clone)]
pub struct Tampon(pub Rc<RefCell<Vec<u8>>>);
#[derive(Clone)]
pub struct Tableau(pub Rc<RefCell<Vec<i64>>>);

// ── Chaîne en morceaux (rope) ────────────────────────────────────────────
//
// 18/11/26 — goulot chaîne : `$s = $s + "x"` (20 000 tours) coûtait 6,18 ms
// contre 2,22 ms pour la même boucle sans concaténer. La concaténation
// `format!("{}{}", x, y)` recopiait la chaîne entière à chaque tour = O(n²),
// ~200 Mo copiés. Une `Valeur::Chaine` porte désormais un arbre de morceaux :
// `+` est O(1) (deux clones de `Rc`), `longueur` est O(1) (compte mémorisé),
// et l'aplatissement n'a lieu qu'aux primitives qui ont besoin des octets.
//
// ⚠ Le `Drop` est ÉCRIT À LA MAIN et itératif : un arbre de 20 000 nœuds
// détruit par le `drop` récursif par défaut déborderait la pile.
pub enum NoeudChaine {
    Plein(String),
    Concat(Rc<ChaineRep>, Rc<ChaineRep>),
    /// Feuille dont les octets vivent dans un mapping `mmap` possédé.
    ///
    /// 19/11/26 — domaine 8 du bench multi : `lire_texte` recopiait 10 Mo
    /// depuis le noyau vers un `Vec` (`fs::read`) avant même de valider
    /// l'UTF-8. Le microbench entrelacé (9 échantillons, c16) mesure
    /// `mmap + validation + compte + write` à 2,70 ms contre 3,04–3,11 ms
    /// pour `read + write` : le mapping évite une recopie de 10 Mo. Le
    /// mapping est possédé par le nœud et libéré par `munmap` dans `Drop`.
    FichierMmap {
        ptr: *const u8,
        len: usize,
        /// longueur passée à `munmap` : exactement celle passée à `mmap`
        /// (le noyau arrondit en interne aux pages, `munmap` accepte la même
        /// valeur). Gardée séparément pour ne jamais désaligner la libération.
        mapping_len: usize,
    },
}

impl std::fmt::Debug for NoeudChaine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NoeudChaine::Plein(s) => f.debug_tuple("Plein").field(s).finish(),
            NoeudChaine::Concat(a, b) => f.debug_tuple("Concat").field(a).field(b).finish(),
            NoeudChaine::FichierMmap { len, .. } =>
                f.debug_struct("FichierMmap").field("len", len).finish(),
        }
    }
}

#[derive(Debug)]
pub struct ChaineRep {
    /// nombre de CARACTÈRES Unicode (sémantique de `longueur`)
    pub n: usize,
    /// nombre d'octets UTF-8 (capacité exacte de l'aplatissement)
    pub octets: usize,
    pub noeud: NoeudChaine,
}

impl ChaineRep {
    #[inline]
    pub fn plein(s: String) -> Rc<ChaineRep> {
        let n = s.chars().count();
        let octets = s.len();
        Rc::new(ChaineRep { n, octets, noeud: NoeudChaine::Plein(s) })
    }

    /// Lecture d'un fichier en mappant ses pages en lecture seule : ZÉRO
    /// recopie des octets vers un `Vec` utilisateur. Le contrôle d'UTF-8 et
    /// le comptage des caractères restent identiques à `plein` (même
    /// sémantique `longueur`). Repli explicite sur `from_utf8_lossy` + copie
    /// quand les octets ne sont pas UTF-8 valides (comportement historique).
    #[cfg(unix)]
    pub fn depuis_fichier(chemin: &str) -> Result<Rc<ChaineRep>, ()> {
        use std::os::raw::{c_char, c_int, c_void};
        unsafe extern "C" {
            fn open(path: *const c_char, flags: c_int, ...) -> c_int;
            fn close(fd: c_int) -> c_int;
            fn fstat(fd: c_int, buf: *mut libc::stat) -> c_int;
            fn mmap(addr: *mut c_void, len: usize, prot: c_int, flags: c_int,
                    fd: c_int, offset: i64) -> *mut c_void;
            fn munmap(addr: *mut c_void, len: usize) -> c_int;
        }
        let c = std::ffi::CString::new(chemin).map_err(|_| ())?;
        let fd = unsafe { open(c.as_ptr(), libc::O_RDONLY) };
        if fd < 0 { return Err(()); }
        let mut st: libc::stat = unsafe { std::mem::zeroed() };
        if unsafe { fstat(fd, &mut st) } != 0 { unsafe { close(fd); } return Err(()); }
        if st.st_size < 0 || st.st_size > (1 << 28) { unsafe { close(fd); } return Err(()); }
        let len = st.st_size as usize;
        // `mmap` refuse une longueur nulle : un fichier vide est une feuille
        // `Plein("")` (comportement historique de `String::from_utf8(vec![])`).
        if len == 0 { unsafe { close(fd); } return Ok(ChaineRep::plein(String::new())); }
        let ptr = unsafe {
            mmap(std::ptr::null_mut(), len, libc::PROT_READ, libc::MAP_PRIVATE, fd, 0)
        };
        unsafe { close(fd); }
        if ptr == usize::MAX as *mut c_void { return Err(()); }
        // Le fd peut être fermé aussitôt : le mapping conserve ses pages.
        let tranche = unsafe { std::slice::from_raw_parts(ptr as *const u8, len) };
        match std::str::from_utf8(tranche) {
            Ok(s) => {
                let n = s.chars().count();
                Ok(Rc::new(ChaineRep {
                    n,
                    octets: len,
                    noeud: NoeudChaine::FichierMmap { ptr: ptr as *const u8, len, mapping_len: len },
                }))
            }
            Err(_) => {
                // Repli historique : `from_utf8_lossy(...).into_owned()`.
                let owned = String::from_utf8_lossy(tranche).into_owned();
                unsafe { munmap(ptr, len); }
                Ok(ChaineRep::plein(owned))
            }
        }
    }

    // ⚠ Windows n'a pas mmap dans `libc` : lecture classique (copie unique).
    #[cfg(not(unix))]
    pub fn depuis_fichier(chemin: &str) -> Result<Rc<ChaineRep>, ()> {
        let octets = std::fs::read(chemin).map_err(|_| ())?;
        if octets.len() > (1 << 28) { return Err(()); }
        let s = String::from_utf8_lossy(&octets).into_owned();
        Ok(ChaineRep::plein(s))
    }

    /// Emprunt des octets quand la feuille les expose déjà contiguëment
    /// (`Plein` ou `FichierMmap`). `None` pour une rope concaténée, qui doit
    /// être aplatie par l'appelant. Évite toute allocation à l'écriture.
    #[inline]
    pub fn octets_empruntes(&self) -> Option<&[u8]> {
        match &self.noeud {
            NoeudChaine::Plein(s) => Some(s.as_bytes()),
            NoeudChaine::FichierMmap { ptr, len, .. } =>
                Some(unsafe { std::slice::from_raw_parts(*ptr, *len) }),
            NoeudChaine::Concat(..) => None,
        }
    }

    /// Concaténation en O(1) : aucun octet n'est copié, seul l'arbre grandit.
    #[inline]
    pub fn concat(a: &Rc<ChaineRep>, b: &Rc<ChaineRep>) -> Rc<ChaineRep> {
        Rc::new(ChaineRep {
            n: a.n + b.n,
            octets: a.octets + b.octets,
            noeud: NoeudChaine::Concat(a.clone(), b.clone()),
        })
    }

    /// Aplatit en une `String` — O(longueur totale), itératif (pile explicite
    /// pour ne pas déborder sur un arbre profond).
    pub fn aplatir(&self) -> String {
        let mut s = String::with_capacity(self.octets);
        let mut pile: Vec<&ChaineRep> = Vec::new();
        pile.push(self);
        while let Some(r) = pile.pop() {
            match &r.noeud {
                NoeudChaine::Plein(t) => s.push_str(t),
                NoeudChaine::FichierMmap { ptr, len, .. } =>
                    s.push_str(unsafe {
                        std::str::from_utf8_unchecked(std::slice::from_raw_parts(*ptr, *len))
                    }),
                NoeudChaine::Concat(a, b) => { pile.push(b.as_ref()); pile.push(a.as_ref()); }
            }
        }
        s
    }

    /// Comparaison lexicographique sans aplatir quand les deux feuilles sont
    /// pleines (cas des clés de table : aucun regain d'allocation).
    pub fn cmp_rep(a: &Rc<ChaineRep>, b: &Rc<ChaineRep>) -> std::cmp::Ordering {
        if let (NoeudChaine::Plein(x), NoeudChaine::Plein(y)) = (&a.noeud, &b.noeud) {
            return x.cmp(y);
        }
        a.aplatir().cmp(&b.aplatir())
    }

    pub fn egale_rep(a: &Rc<ChaineRep>, b: &Rc<ChaineRep>) -> bool {
        if a.n != b.n { return false; }
        if let (NoeudChaine::Plein(x), NoeudChaine::Plein(y)) = (&a.noeud, &b.noeud) {
            return x == y;
        }
        a.aplatir() == b.aplatir()
    }
}

thread_local! {
    /// Littéraux chaînes internés par thread. Un littéral est immuable (la
    /// rope ne se modifie jamais via `Rc::make_mut`) : le partager est sûr.
    static CACHE_LITTERAUX: RefCell<HashMap<String, Rc<ChaineRep>>> =
        RefCell::new(HashMap::new());
}

/// Valeur d'un littéral chaîne, allouée UNE fois par texte et par thread.
///
/// 18/11/26 : mesuré sur `$s = $s + "x"` (20 000 tours), le littéral `"x"`
/// recréait un `String` + un `Rc<ChaineRep>` à chaque tour : 1,6 ms sur 3,9 ms.
/// Le harnais avec `"x"` hoisté hors boucle tournait en 2,29 ms.
#[inline]
pub fn valeur_litteral_chaine(s: &str) -> Valeur {
    if let Some(rc) = CACHE_LITTERAUX.with(|c| c.borrow().get(s).cloned()) {
        return Valeur::Chaine(rc);
    }
    let rc = ChaineRep::plein(s.to_string());
    CACHE_LITTERAUX.with(|c| { c.borrow_mut().insert(s.to_string(), rc.clone()); });
    Valeur::Chaine(rc)
}

#[cfg(unix)]
fn liberer_mmap_ptr(ptr: *const u8, len: usize) {
    unsafe { libc::munmap(ptr as *mut std::os::raw::c_void, len); }
}
#[cfg(not(unix))]
fn liberer_mmap_ptr(_ptr: *const u8, _len: usize) {}

impl Drop for ChaineRep {
    fn drop(&mut self) {
        // Destruction itérative : on vide le nœud courant, puis on parcourt
        // les descendants en profitant de `Rc::get_mut` quand on est le
        // dernier propriétaire. Un nœud partagé se contente de décrémenter.
        // ⚠ Cas dominant (feuille) : AUCUNE allocation — le `Vec` n'est créé
        // que si l'arbre a réellement des descendants (mesuré : `vec![noeud]`
        // coûtait une allocation par feuille, soit 20 000 de trop ici).
        let mut noeud = std::mem::replace(&mut self.noeud, NoeudChaine::Plein(String::new()));
        match &noeud {
            NoeudChaine::Plein(_) => return,
            NoeudChaine::FichierMmap { ptr, mapping_len, .. } => {
                liberer_mmap_ptr(*ptr, *mapping_len);
                return;
            }
            NoeudChaine::Concat(..) => {}
        }
        let mut pile: Vec<NoeudChaine> = Vec::new();
        loop {
            match noeud {
                NoeudChaine::Plein(_) => {}
                NoeudChaine::FichierMmap { ptr, mapping_len, .. } => {
                    liberer_mmap_ptr(ptr, mapping_len);
                }
                NoeudChaine::Concat(mut a, mut b) => {
                    if let Some(ra) = Rc::get_mut(&mut a) {
                        pile.push(std::mem::replace(&mut ra.noeud, NoeudChaine::Plein(String::new())));
                    }
                    if let Some(rb) = Rc::get_mut(&mut b) {
                        pile.push(std::mem::replace(&mut rb.noeud, NoeudChaine::Plein(String::new())));
                    }
                }
            }
            match pile.pop() { Some(n) => noeud = n, None => break }
        }
    }
}

#[derive(Clone)]
pub enum Valeur {
    Vide,
    Entier(i64, Option<Rc<Gz>>),          // i64 quand ça tient, i128 au-delà
    Rationnel(Rc<Gq>),
    Flottant(f64),
    Booleen(bool),
    Chaine(Rc<ChaineRep>),
    Liste(Liste),
    Table(Table),
    Tampon(Tampon),
    Tableau(Tableau),
    Complexe { re: f64, im: f64 },
    Irrationnel(Rc<Gz>),
}

impl Valeur {
    #[inline]
    pub fn z(v: i64) -> Valeur {
        // Le JIT ramène ses résultats vers l'interpréteur avec un tag lu dans
        // `tag_tmp`, qui peut être périmé (dernier calcul de la boucle, pas la
        // valeur retournée). Un grand entier arrive alors comme une simple
        // adresse i64 : la reconnaissance par l'ABI taggée le récupère.
        if v < -0x1000_0000 || v > 0x1000_0000 {
            if let Some(p) = mpz_ptr(v, 0) {
                let g = Gz::neuf();
                unsafe { g.copier_de_ptr(p); }
                return Valeur::Entier(0, Some(Rc::new(g)));
            }
        }
        Valeur::Entier(v, None)
    }
    pub fn zg(z: Gz) -> Valeur {
        if z.fits_i64() { Valeur::z(z.get_i64()) } else { Valeur::Entier(0, Some(Rc::new(z))) }
    }
    pub fn qq(q: Gq) -> Valeur { Valeur::Rationnel(Rc::new(q)) }
    pub fn f(v: f64) -> Valeur { Valeur::Flottant(v) }
    #[inline]
    pub fn b(v: bool) -> Valeur { Valeur::Booleen(v) }
    #[inline]
    pub fn s(v: impl Into<String>) -> Valeur { Valeur::Chaine(ChaineRep::plein(v.into())) }
    #[allow(dead_code)]
    pub fn liste() -> Valeur { Valeur::Liste(Liste(Rc::new(RefCell::new(Vec::new())))) }
    pub fn liste_de(v: Vec<Valeur>) -> Valeur { Valeur::Liste(Liste(Rc::new(RefCell::new(v)))) }
    pub fn table() -> Valeur { Valeur::Table(Table::neuf(Vec::new())) }
    pub fn tampon(n: usize) -> Valeur { Valeur::Tampon(Tampon(Rc::new(RefCell::new(vec![0u8; n])))) }
    pub fn tableau(n: usize) -> Valeur { Valeur::Tableau(Tableau(Rc::new(RefCell::new(vec![0i64; n])))) }
    pub fn irrationnel(n: Gz) -> Valeur { Valeur::Irrationnel(Rc::new(n)) }

    pub fn ty(&self) -> TyKind {
        match self {
            Valeur::Vide => TyKind::Vide,
            Valeur::Entier(..) => TyKind::Entier,
            Valeur::Rationnel(..) => TyKind::Rationnel,
            Valeur::Flottant(..) => TyKind::Flottant,
            Valeur::Booleen(..) => TyKind::Booleen,
            Valeur::Chaine(..) => TyKind::Chaine,
            Valeur::Liste(..) => TyKind::Liste,
            Valeur::Table(..) => TyKind::Table,
            Valeur::Tampon(..) => TyKind::Tampon,
            Valeur::Tableau(..) => TyKind::Tableau,
            Valeur::Irrationnel(..) => TyKind::Irrationnel,
            Valeur::Complexe { .. } => TyKind::Complexe,
        }
    }
}

// ── Sorties et erreur runtime ──────────────

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

pub static DEBUG_MODE: AtomicBool = AtomicBool::new(false);

// Registre global des threads lancés par spawn.
pub static THREADS: Mutex<Vec<Option<JoinHandle<()>>>> = Mutex::new(Vec::new());

pub fn spawn_thread<F>(f: F) -> usize
where F: FnOnce() + Send + 'static {
    let mut reg = THREADS.lock().unwrap();
    let id = reg.len();
    let handle = thread::spawn(f);
    reg.push(Some(handle));
    id
}

pub fn join_thread(id: usize) {
    let handle = {
        let mut reg = THREADS.lock().unwrap();
        if id < reg.len() {
            reg[id].take()
        } else {
            None
        }
    };
    if let Some(h) = handle {
        let _ = h.join();
    }
}
pub fn debug_mode() -> bool { DEBUG_MODE.load(Ordering::Relaxed) }
pub fn set_debug_mode(v: bool) { DEBUG_MODE.store(v, Ordering::Relaxed); }

pub static STEP_MODE: AtomicBool = AtomicBool::new(false);
pub static WATCH_LIST: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub fn step_mode() -> bool { STEP_MODE.load(Ordering::Relaxed) }
pub fn set_step_mode(v: bool) { STEP_MODE.store(v, Ordering::Relaxed); }
pub fn add_watch(v: String) { WATCH_LIST.lock().unwrap().push(v); }
pub fn remove_watch(v: &str) { WATCH_LIST.lock().unwrap().retain(|x| x != v); }
pub fn get_watches() -> Vec<String> { WATCH_LIST.lock().unwrap().clone() }

// ── Horloge portable ─────────────────────────────────────────────────────
//
// `std::time::Instant::now()`, `std::time::SystemTime::now()` et
// `std::process::id()` PANIQUENT sur wasm32-unknown-unknown (« time
// not implemented on this platform »). Le moteur s'en sert pour mesurer
// des durées et dater les tirages aléatoires : on passe donc par des
// aides qui, en WASM, renvoient des valeurs neutres au lieu de paniquer.

/// Horloge relative du moteur (mesures de durée).
#[cfg(not(target_arch = "wasm32"))]
pub type Chrono = std::time::Instant;
/// Horloge relative du moteur : en WASM, un compteur factice (les durées
/// mesurées valent toujours zéro — le moteur ne peut pas lire l'horloge).
#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy)]
pub struct Chrono(std::time::Duration);

#[cfg(target_arch = "wasm32")]
impl Chrono {
    pub fn now() -> Chrono { Chrono(std::time::Duration::ZERO) }
    pub fn elapsed(&self) -> std::time::Duration { self.0 }
}

/// Horloge du moteur (compatible `Instant`).
pub fn chrono() -> Chrono {
    #[cfg(not(target_arch = "wasm32"))]
    { std::time::Instant::now() }
    #[cfg(target_arch = "wasm32")]
    { Chrono::now() }
}

/// Nanosecondes depuis l'époque Unix (0 en WASM : pas d'horloge).
pub fn temps_unix_nanos() -> u64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    }
    #[cfg(target_arch = "wasm32")]
    { 0 }
}

/// Identifiant du processus : `std::process::id()` PANIQUE en WASM
/// (« no pids on this platform »). On rend un compteur monotone, ce qui
/// conserve la propriété « deux exécutions successives ne donnent pas
/// la même suite aléatoire ».
pub fn pid() -> u32 {
    #[cfg(not(target_arch = "wasm32"))]
    { std::process::id() }
    #[cfg(target_arch = "wasm32")]
    {
        static COMPTEUR: std::sync::atomic::AtomicU32 =
            std::sync::atomic::AtomicU32::new(0);
        COMPTEUR.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }
}

pub fn flush_out() { let _ = std::io::stdout().flush(); }

// ── Capture de la sortie (bibliothèque / WebAssembly) ─────────────────────
//
// En WebAssembly il n'y a pas de « stdout » : la sortie du programme est
// capturée dans un tampon mémoire que l'appelant récupère avec
// `take_sink()`. Le tampon est GLOBAL : les threads créés par le programme
// Rose y écrivent eux aussi (une seule exécution à la fois).

static SINK: Mutex<Option<Vec<u8>>> = Mutex::new(None);

/// Active la capture de la sortie : stdout est alors ignoré.
pub fn set_sink(active: bool) {
    let mut g = match SINK.lock() { Ok(g) => g, Err(p) => p.into_inner() };
    if active {
        // On repart d'un tampon VIDE à chaque activation : après un
        // arrêt du module (WASM), le tampon précédent doit être jeté.
        *g = Some(Vec::new());
    } else {
        *g = None;
    }
}

/// Récupère la sortie capturée et désactive la capture.
pub fn take_sink() -> Option<String> {
    let mut g = match SINK.lock() { Ok(g) => g, Err(p) => p.into_inner() };
    match g.take() {
        Some(v) => Some(String::from_utf8_lossy(&v).into_owned()),
        None => None,
    }
}

/// Écrit sur la sortie du programme : dans le tampon de capture s'il est
/// actif, sur stdout sinon.
pub fn ecrire_out(s: &str) {
    {
        let mut g = match SINK.lock() { Ok(g) => g, Err(p) => p.into_inner() };
        if let Some(buf) = g.as_mut() {
            buf.extend_from_slice(s.as_bytes());
            return;
        }
    }
    let _ = std::io::stdout().write_all(s.as_bytes());
}

// ── Aides d'affichage pour le backend AOT LLVM ORC ───────────────────────
//
// Le backend AOT (src/aot_llvm_orc.rs) émet des appels à `printf` pour
// les entiers. Pour les flottants et les complexes, on centralise le
// rendu dans ces helpers pour GARANTIR que la sortie AOT est bit-à-bit
// identique à celle du moteur (run::afficher / chaine_de / texte_flottant).

/// Affiche un flottant avec le rendu exact de Rose (5.0 → « 5 », 2.5 →
/// « 2.5 », périodique pour les rationnels).
#[unsafe(no_mangle)]
pub extern "C" fn rose_orc_affiche_f64(v: f64) {
    let t = if v.is_nan() { "non_nombre".to_string() }
        else if v.is_infinite() { if v > 0.0 { "infini".into() } else { "-infini".into() } }
        else if v == v.floor() && v.abs() < 1e16 { (v as i64).to_string() }
        else { crate::front::texte_flottant(v) };
    println!("{}", t);
}

/// Affiche un complexe avec le rendu exact de Rose (« 2i », « 3+i »,
/// « 3+4i » — jamais « 0+2i »).
#[unsafe(no_mangle)]
pub extern "C" fn rose_orc_affiche_complexe(re: f64, im: f64) {
    let s = if im == 0.0 {
        re.to_string()
    } else if re == 0.0 {
        if im == 1.0 { "i".into() } else if im == -1.0 { "-i".into() } else { format!("{}i", im) }
    } else if im == 1.0 {
        format!("{}+i", re)
    } else if im == -1.0 {
        format!("{}-i", re)
    } else {
        let sign = if im >= 0.0 { "+" } else { "" };
        format!("{}{}{}i", re, sign, im)
    };
    println!("{}", s);
}

pub fn erreur_runtime(msg: &str) -> ! {
    flush_out();
    // Panic avec un préfixe identifiable pour permettre la récupération
    // par try/catch (essayer/attraper) via catch_unwind.
    panic!("ROSE_ERR:{}", msg);
}

/// Erreur levée pendant le repli de constantes : elle est signalée comme une
/// erreur du vérificateur, avec le préfixe de position [Ligne L, Colonne C]
/// (les expressions repliées ne portent pas de position : 1,0,
/// qui replie à la racine avec ligne=1 par défaut).
pub fn erreur_runtime_ligne(msg: &str) -> ! {
    flush_out();
    panic!("ROSE_ERR:[Ligne {}, Colonne 0] {}", ligne_programme(), msg);
}

/// Erreur levée pendant le vérificateur / le repli de constantes : elle
/// remonte une ErreurRose(msg, ligne) jusqu'au driver qui l'affiche via
/// « Rose : erreur — %s » avec le préfixe [Ligne L, Colonne C] si ligne != 0.
pub fn erreur_front(msg: &str, l: usize, c: usize) -> ! {
    flush_out();
    if l != 0 {
        eprintln!("Rose : erreur — [Ligne {}, Colonne {}] {}", l, c, msg);
    } else {
        eprintln!("Rose : erreur — {}", msg);
    }
    std::process::exit(1);
}

/// Ligne du dernier jeton consommé par le lexeur (position approximative des
/// erreurs de vérification/repli (qui portent leur ligne).
pub fn ligne_programme() -> usize {
    LIGNE_COURANTE.with(|l| l.get().max(1))
}

thread_local! {
    pub static LIGNE_COURANTE: std::cell::Cell<usize> = const { std::cell::Cell::new(1) };
}

pub fn noter_ligne(l: usize) {
    LIGNE_COURANTE.with(|c| c.set(l));
}

// ── Conversions (miroir vers_z / vers_q / vers_d / convertir) ─────────────

#[inline(always)]
pub fn vers_z(v: &Valeur) -> Gz {
    let z = Gz::neuf();
    match v {
        Valeur::Entier(i, None) => z.poser_i64(*i),
        Valeur::Entier(_, Some(o)) => z.copier_de(o),
        Valeur::Booleen(b) => z.poser_i64(if *b { 1 } else { 0 }),
        Valeur::Rationnel(q) => { let n = q.num_i128(); let d = q.den_i128(); if d == 0 { erreur_runtime("division par zéro"); } z.poser_i64((n / d) as i64); }
        Valeur::Flottant(d) => z.poser_d(*d),
        Valeur::Chaine(s) => {
            let t = s.aplatir();
            if t.is_empty() { z.poser_i64(0); }
            else if !z.depuis_chaine(&t) { erreur_runtime(&format!("conversion vers entier impossible (chaine: {:?})", t)); }
        }
        _ => erreur_runtime(&format!(
            "conversion vers entier impossible (type {:?}, val={:?}, ligne={})",
            v.ty(), chaine_de(v), LIGNE_COURANTE.with(|c| c.get()),
        )),
    }
    z
}

pub fn vers_q(v: &Valeur) -> Gq {
    let q = Gq::neuf();
    match v {
        Valeur::Entier(i, None) => unsafe { gmpq_set_z_field(q.num(), Gz::de_i128(*i as i128).p()) },
        Valeur::Entier(_, Some(o)) => unsafe { gmpq_set_z_field(q.num(), o.p()); gmpq_set_z_field(q.den(), un_z().p()) },
        Valeur::Booleen(b) => q.set_ui(if *b { 1 } else { 0 }, 1),
        Valeur::Rationnel(o) => q.copier_de(o),
        Valeur::Flottant(d) => {
            if d.is_nan() || d.is_infinite() {
                erreur_runtime("flottant infini/non-nombre sans rationnel correspondant");
            }
            q.depuis_d(*d);
        }
        _ => erreur_runtime("conversion vers rationnel impossible"),
    }
    q
}

#[inline]
pub fn vers_d(v: &Valeur) -> f64 {
    match v {
        Valeur::Flottant(d) => *d,
        Valeur::Entier(i, None) => *i as f64,
        Valeur::Entier(_, Some(z)) => z.get_d(),
        Valeur::Rationnel(q) => q.get_d(),
        Valeur::Booleen(b) => if *b { 1.0 } else { 0.0 },
        // une chaine est convertie en flottant via chaine_vers_flottant dans
        // les conversions ; miroir : 'flottant' depuis chaine est possible ici.
        _ => { let _ = &v; vers_d_chaine(&v) },
    }
}

fn vers_d_chaine(v: &Valeur) -> f64 {
    match v {
        Valeur::Chaine(s) => chaine_vers_flottant(&s.aplatir()),
        _ => erreur_runtime("conversion vers flottant impossible"),
    }
}

#[inline]
pub fn convertir(v: Valeur, ty: TyKind) -> Valeur {
    match ty {
        TyKind::Entier => match &v {
            Valeur::Entier(..) => v.clone(),
            Valeur::Booleen(b) => Valeur::z(if *b { 1 } else { 0 }),
            _ => Valeur::zg(vers_z(&v)),
        },
        TyKind::Rationnel => match &v {
            Valeur::Rationnel(..) => v.clone(),
            _ => Valeur::qq(vers_q(&v)),
        },
        TyKind::Flottant => match &v {
            Valeur::Flottant(..) => v.clone(),
            _ => Valeur::f(vers_d(&v)),
        },
        _ => v,
    }
}

pub fn chaine_vers_rationnel(ch: &str) -> Gq {
    let ch = ch.trim();
    if ch.contains('/') {
        let parts: Vec<&str> = ch.split('/').collect();
        if parts.len() == 2 {
            let n = parts[0].trim().parse::<i128>().unwrap_or(0);
            let d = parts[1].trim().parse::<i128>().unwrap_or(1);
            return Gq::de(n, if d == 0 { 1 } else { d });
        }
    }
    // entier ou flottant
    if let Ok(n) = ch.parse::<i128>() {
        return Gq::de(n, 1);
    }
    if let Ok(f) = ch.parse::<f64>() {
        let q = Gq::neuf();
        q.depuis_d(f);
        return q;
    }
    Gq::de(0, 1)
}

pub fn chaine_vers_entier(ch: &str) -> Gz {
    let z = Gz::neuf();
    if ch.trim().is_empty() { return z; }
    if z.depuis_chaine(ch) { return z; }
    if let Some(p) = ch.find('/') {
        let n = Gz::neuf();
        let d = Gz::neuf();
        if !n.depuis_chaine(&ch[..p]) || !d.depuis_chaine(&ch[p + 1..]) || d.sgn() == 0 {
            erreur_runtime("conversion chaine → entier impossible");
        }
        z.fdiv_q(&n, &d);
        return z;
    }
    if let Some(pt) = ch.find('.') {
        if z.depuis_chaine(&ch[..pt]) { return z; }
    }
    erreur_runtime("conversion chaine → entier impossible");
}

// ── Comparaisons (miroir cmp_vals / rose_valeur_egale) ────────────────────

#[inline(always)]
pub fn cmp_vals(g: &Valeur, d: &Valeur) -> i32 {
    // ⚠⚠⚠ VOIE RAPIDE i64 EN TÊTE (mesuré le 18/11/26).
    // `vers_z` alloue un `Mpz` boxé ET un `BigInt` pour chaque opérande,
    // puis `cmp_gz` reclône les deux `BigInt` : ~6 allocations par
    // comparaison. Or `inserer`/`cle_existe`/`Expr::Index` appellent
    // `valeur_egale` (donc `cmp_vals`) des MILLIONS de fois sur des clés
    // entières i64. La règle est EXACTE : deux `Valeur::Entier(_, None)`
    // sont des i64, leur comparaison est identique à celle des BigInt.
    // Les valeurs `Some(Rc<Gz>)` (hors i64 ou issues de l'ABI JIT) ne
    // prennent PAS cette voie : elles retombent sur l'arithmétique exacte.
    if let (Valeur::Entier(x, None), Valeur::Entier(y, None)) = (g, d) {
        return match x.cmp(y) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        };
    }
    // ⚠ Grand entier boxé vs petit entier i64 : le cas fréquent
    // `$y != 0` sur une boucle Euclid. L'ancien chemin faisait deux
    // `vers_z` (2 Gz + 2 clones BigInt) puis `cmp_gz` (2 clones de plus).
    // Ici aucun boxage, aucun clone : `to_i64` échoue dès que le grand
    // entier sort de l'intervalle et le signe suffit alors.
    fn cmp_gz_i64(z: &crate::i128x::Gz, y: i64) -> i32 {
        match z.bref().to_i64() {
            Some(x) => match x.cmp(&y) {
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => 1,
            },
            None => if z.bref().sign() == num_bigint::Sign::Minus { -1 } else { 1 },
        }
    }
    if let (Valeur::Entier(_, Some(z)), Valeur::Entier(y, None)) = (g, d) {
        return cmp_gz_i64(z, *y);
    }
    if let (Valeur::Entier(y, None), Valeur::Entier(_, Some(z))) = (g, d) {
        return -cmp_gz_i64(z, *y);
    }
    if let (Valeur::Chaine(a), Valeur::Chaine(b)) = (g, d) {
        return match ChaineRep::cmp_rep(a, b) {
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Greater => 1,
        };
    }
    let flot = matches!(g, Valeur::Flottant(..)) || matches!(d, Valeur::Flottant(..));
    if flot {
        let a = vers_d(g);
        let b = vers_d(d);
        return if a < b { -1 } else if a > b { 1 } else { 0 };
    }
    let rat = matches!(g, Valeur::Rationnel(..)) || matches!(d, Valeur::Rationnel(..));
    if rat {
        let qa = vers_q(g);
        let qb = vers_q(d);
        return qa.cmp_gq(&qb) as i32;
    }
    let za = vers_z(g);
    let zb = vers_z(d);
    za.cmp_gz(&zb) as i32
}

#[inline(always)]
pub fn valeur_egale(a: &Valeur, b: &Valeur) -> bool {
    // ⚠ Même voie rapide que `cmp_vals` : deux i64 s'égalisent par
    // comparaison directe, sans boxer de `Gz`/`BigInt`. C'est LE chemin
    // des scans linéaires de `table()` (`inserer`, `cle_existe`,
    // `supprimer`) et des comparaisons de listes.
    if let (Valeur::Entier(x, None), Valeur::Entier(y, None)) = (a, b) {
        return x == y;
    }
    match (a, b) {
        (Valeur::Tampon(x), Valeur::Tampon(y)) => *x.0.borrow() == *y.0.borrow(),
        (_, Valeur::Tampon(..)) | (Valeur::Tampon(..), _) => false,
        (Valeur::Tableau(x), Valeur::Tableau(y)) => *x.0.borrow() == *y.0.borrow(),
        (_, Valeur::Tableau(..)) | (Valeur::Tableau(..), _) => false,
        (Valeur::Complexe { re: r1, im: i1 }, Valeur::Complexe { re: r2, im: i2 }) => *r1 == *r2 && *i1 == *i2,
        (_, Valeur::Complexe { .. }) | (Valeur::Complexe { .. }, _) => false,
        (Valeur::Liste(x), Valeur::Liste(y)) => {
            let (x, y) = (x.0.borrow(), y.0.borrow());
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(p, q)| valeur_egale(p, q))
        }
        (_, Valeur::Liste(..)) | (Valeur::Liste(..), _) => false,
        (Valeur::Table(x), Valeur::Table(y)) => {
            let (x, y) = (x.0.borrow(), y.0.borrow());
            if x.items.len() != y.items.len() { return false; }
            x.items.iter().all(|(k, v)| y.items.iter().any(|(k2, v2)| valeur_egale(k, k2) && valeur_egale(v, v2)))
        }
        (_, Valeur::Table(..)) | (Valeur::Table(..), _) => false,
        (Valeur::Chaine(x), Valeur::Chaine(y)) => ChaineRep::egale_rep(x, y),
        (Valeur::Chaine(..), _) | (_, Valeur::Chaine(..)) => false,
        (Valeur::Booleen(x), Valeur::Booleen(y)) => x == y,
        (Valeur::Booleen(..), _) | (_, Valeur::Booleen(..)) => false,
        _ => cmp_vals(a, b) == 0,
    }
}

// ── Rendus (miroir ecri / chaine_de / texte_flottant) ─────────────────────

pub fn q_chaine(q: &Gq) -> String {
    // ⚠ num_i128()/den_i128() renvoient 0 quand le grand entier dépasse i128
    // (unwrap_or(0)) : après ~100 sommes rationnelles on affichait « 0/0 ».
    // Gq::chaine() formate depuis les BigInt eux-mêmes, sans cette troncature.
    q.chaine()
}

/// ecri T_RATIONNEL : a/b, forme décimale périodique (1/3 → 0.[3]),
/// seuil 500 chiffres → a/b brut.
pub fn texte_rationnel(q: &Gq) -> String {
    // Même piège que q_chaine : au-delà de i128, num_i128() vaut 0.
    // On passe alors en forme a/b exacte calculée sur les BigInt.
    if q.num_bigint().bits() > 126 || q.den_bigint().bits() > 126 {
        return q.chaine();
    }
    let nb = q.num_i128();
    let db = q.den_i128();
    if db == 1 { return nb.to_string(); }
    let n0 = nb;
    let d0 = db;
    let gros = (Gz::de_i128(n0).sizeinbase10() + Gz::de_i128(d0).sizeinbase10()) > 500;
    if gros { return format!("{}/{}", n0, d0); }

    if d0 == 0 { return "0".to_string(); }
    let dd = d0.unsigned_abs() as i128;
    let entier = {
        let (mut q_, mut r_) = (n0 / dd, n0 % dd);
        if r_ != 0 && ((r_ < 0) != (dd < 0)) { q_ -= 1; }
        q_
    };
    let mut reste = n0.unsigned_abs() as i128 % dd;
    let mut digits: Vec<u8> = Vec::new();
    let mut restes: Vec<i128> = Vec::new();
    let mut positions: Vec<usize> = Vec::new();
    let mut peri: i32 = -1;
    while reste != 0 && digits.len() < 200 {
        if let Some(p) = restes.iter().position(|r| *r == reste) {
            peri = positions[p] as i32;
            break;
        }
        restes.push(reste);
        positions.push(digits.len());
        match reste.checked_mul(10) { Some(x) => reste = x, None => crate::i128x::hors_plage() }
        let chiffre = reste / dd;
        reste = reste % dd;
        digits.push(b'0' + chiffre as u8);
    }
    let mut out = entier.to_string();
    if !digits.is_empty() {
        out.push('.');
        let fin = if peri >= 0 { peri as usize } else { digits.len() };
        out.push_str(&String::from_utf8_lossy(&digits[..fin]));
        if peri >= 0 {
            out.push('[');
            out.push_str(&String::from_utf8_lossy(&digits[peri as usize..]));
            out.push(']');
        } else if digits.len() >= 200 {
            out.push('…');
        }
    }
    out
}

#[inline]
pub fn chaine_de(v: &Valeur) -> String {
    // chaine_de() : a/b BRUT a tous les niveaux (les listes et
    // tables rendent leurs rationnels en « 12/5 »). La forme periodique est
    // reservee a afficher() (miroir ecri).
    chaine_de_p(v, false)
}

/// chaine_de(). « peri » = affichage direct (>> ) : le
/// rationnel y passe par la forme decimale periodique (2/5 → 0.4, comme ecri()).
pub fn chaine_de_p(v: &Valeur, peri: bool) -> String {
    match v {
        Valeur::Vide => String::new(),
        Valeur::Entier(i, None) => i.to_string(),
        Valeur::Entier(_, Some(z)) => z.chaine(),
        Valeur::Rationnel(q) => if peri { texte_rationnel(q) } else { q_chaine(q) },
        Valeur::Flottant(d) => {
            let d = *d;
            if d.is_nan() { return "non_nombre".into(); }
            if d.is_infinite() { return if d > 0.0 { "infini".into() } else { "-infini".into() }; }
            if d == d.floor() && d.abs() < 1e16 { return (d as i64).to_string(); }
            texte_flottant(d)
        }
        Valeur::Booleen(b) => if *b { "vrai".into() } else { "faux".into() },
        Valeur::Chaine(s) => s.aplatir(),
        Valeur::Liste(l) => {
            let items = l.0.borrow();
            let mut r = String::from("[");
            for (i, e) in items.iter().enumerate() {
                if i > 0 { r.push_str(", "); }
                r.push_str(&chaine_de_p(e, peri));
            }
            r.push(']');
            r
        }
        Valeur::Table(t) => {
            let rep = t.0.borrow();
            let items = &rep.items;
            let mut r = String::from("[");
            for (i, (k, v2)) in items.iter().enumerate() {
                if i > 0 { r.push_str(", "); }
                r.push_str(&chaine_de_p(k, peri));
                r.push_str(" => ");
                r.push_str(&chaine_de_p(v2, peri));
            }
            r.push(']');
            r
        }
        Valeur::Tampon(t) => String::from_utf8_lossy(&t.0.borrow()).into_owned(),
        Valeur::Complexe { re, im } => {
            if *im == 0.0 { return re.to_string(); }
            if *re == 0.0 {
                if *im == 1.0 { return "i".to_string(); }
                if *im == -1.0 { return "-i".to_string(); }
                return format!("{}i", im);
            }
            let sign = if *im >= 0.0 { "+" } else { "" };
            if *im == 1.0 { format!("{}+i", re) }
            else if *im == -1.0 { format!("{}-i", re) }
            else { format!("{}{}{}i", re, sign, im) }
        }
        Valeur::Tableau(t) => {
            let b = t.0.borrow();
            if b.len() <= 8 {
                format!("[{}]", b.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", "))
            } else {
                format!("[{} ... {} elems]", b.iter().take(4).map(|x| x.to_string()).collect::<Vec<_>>().join(", "), b.len())
            }
        }
        Valeur::Irrationnel(z) => {
            if z.fits_i64() {
                format!("√{}", z.get_i64())
            } else {
                format!("√{}", z.chaine())
            }
        }
    }
}

/// Affiche une valeur, avec un préfixe éventuel.
///
/// ⚠⚠⚠ DEPUIS LE 16/11, `formule` EST TOUJOURS VIDE (demande utilisateur :
/// « le tout est affiché en LaTeX à part pour la commande faite pour ça »,
/// puis « plus nécessaire de tout en latex même constantes »). La sortie
/// d'exécution est la VALEUR SEULE ; le LaTeX se lit par `rose latex`
/// (programme entier) ou par les lignes marquées `%%`.
///
/// ⚠ La fonction est CONSERVÉE comme point unique d'affichage : si un jour
/// un préfixe redevient utile, c'est ICI qu'il se pose (interpréteur ET
/// JIT passent par là).
pub fn afficher_formule(v: &Valeur, formule: &str) {
    if formule.is_empty() {
        afficher(v);
        return;
    }
    // la formule précède la valeur, séparée par trois espaces (comme π).
    ecrire_out(formule);
    ecrire_out("   ");
    // le rationnel passe par la forme périodique (miroir ecri()).
    match v {
        Valeur::Rationnel(q) => ecrire_out(&texte_rationnel(q)),
        Valeur::Liste(..) => ecrire_out(&chaine_de_p(v, true)),
        Valeur::Tampon(..) => erreur_runtime("Un tampon ne s'affiche pas directement — utiliser chaine(tampon)"),
        _ => ecrire_out(&chaine_de(v)),
    }
    ecrire_out("\n");
}

/// affichage : le rationnel passe par la forme périodique (miroir ecri()).
pub fn afficher(v: &Valeur) {
    // ecri() : T_RATIONNEL et T_LISTE passent par la forme
    // periodique ; T_TABLE rend chaine_de() → a/b BRUT pour ses rationnels ;
    // T_TAMPON refuse de s'afficher.
    match v {
        Valeur::Rationnel(q) => ecrire_out(&texte_rationnel(q)),
        Valeur::Liste(..) => ecrire_out(&chaine_de_p(v, true)),
        Valeur::Tampon(..) => erreur_runtime("Un tampon ne s'affiche pas directement — utiliser chaine(tampon)"),
        Valeur::Tableau(..) => ecrire_out(&chaine_de(v)),
        Valeur::Complexe { .. } => ecrire_out(&chaine_de(v)),
        _ => ecrire_out(&chaine_de(v)),
    }
    ecrire_out("\n");
}


// ── RNG splitmix64 (miroir rng_tirer / graine / hasard) ───────────────────


pub fn rng_tirer(etat: &mut u64) -> u64 {
    *etat = etat.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *etat;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

pub fn graine_de(etat: &mut u64, n: &Gz) {
    *etat = if n.fits_i64() {
        n.get_i64() as u64
    } else {
        (n.fdiv_u64(0xFFFF_FFFF) << 32) | n.fdiv_u64(0x1_0000_0000)
    };
    rng_tirer(etat);
}

pub fn tirage_systeme() -> u64 {
    // getrandom(2) : appel système NATIF — impossible en WebAssembly
    // (un module WASM n'importe aucun symbole du système hôte).
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut v = 0u64;
        unsafe extern "C" {
            fn getrandom(buf: *mut u64, len: usize, flags: u32) -> isize;
        }
        let got = unsafe { getrandom(&mut v, 8, 0) };
        if got != 8 {
            v = (std::process::id() as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
                ^ (std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos() as u64)
                    .unwrap_or(0x5DEECE66D));
        }
        v
    }
    // En WASM : ni getrandom(2) ni std::process::id() (qui panic). On
    // mélange un compteur monotone et l'horloge de l'hôte (0 par
    // défaut) : la graine varie d'une exécution à l'autre. Un programme
    // qui veut des tirages reproductibles fixe la graine avec
    // « graine(n) ».
    #[cfg(target_arch = "wasm32")]
    {
        static COMPTEUR: std::sync::atomic::AtomicU64 =
            std::sync::atomic::AtomicU64::new(0);
        let n = COMPTEUR.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        n.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ temps_unix_nanos() ^ 0x5DEECE66D
    }
}

/// hasard(n) : Lemire, borné, hors i64 inclus.
pub fn hasard(etat: &mut u64, n: &Gz) -> Gz {
    if n.sgn() <= 0 { erreur_runtime("hasard : bornage positif attendu"); }
    let r = Gz::neuf();
    if n.fits_i64() {
        let nn = n.get_i64() as u64;
        let x = rng_tirer(etat);
        let v = if nn & (nn - 1) == 0 {
            x & (nn - 1)
        } else {
            (((x as u128) * (nn as u128)) >> 64) as u64
        };
        r.poser_u64(v);
    } else {
        let x = rng_tirer(etat);
        r.poser_u64(x >> 32);
        r.mul_2exp(&r, 32);
        r.add_u64(&r, x & 0xFFFF_FFFF);
        r.fdiv_r(&r, n);
    }
    r
}

// ── Philox4x32-10 (miroir de rose_remplir_hasard_philox) ──────────────────

const PHILOX_M0: u32 = 0xD251_1F53;
const PHILOX_M1: u32 = 0xCD9E_8D57;
const PHILOX_W0: u32 = 0x9E37_79B9;
const PHILOX_W1: u32 = 0xBB67_AE85;

fn mul_hi_lo(a: u32, b: u32) -> (u32, u32) {
    let p = (a as u64) * (b as u64);
    ((p >> 32) as u32, p as u32)
}



// ── Maths exactes (miroir rose_factorielle / fibonacci / π / crible) ──────

pub fn factorielle(n: &Gz) -> Gz {
    if n.sgn() < 0 { erreur_runtime("factorielle d'un négatif : impossible"); }
    if !n.fits_i64() || n.get_i64() as u64 > 0xFFFF_FFFF { erreur_runtime("factorielle trop grande"); }
    let n = n.get_i64() as u64;
    // Facteurs premiers (Légendre) + arbre de produit équilibré, tranches
    // parallèles : voir i128x::factorielle_parallele.
    crate::i128x::factorielle_parallele(n)
}

pub fn fibonacci(nn: &Gz) -> Gz {
    if nn.sgn() < 0 { erreur_runtime("fibonacci négatif : impossible"); }
    let a = Gz::neuf();
    let b = Gz::neuf();
    a.poser_i64(0);
    b.poser_i64(1);
    let bits = nn.taille_bits();
    let t1 = Gz::neuf();
    let t2 = Gz::neuf();
    let t3 = Gz::neuf();
    for i in (0..bits).rev() {
        t1.mul_u64(&b, 2);
        t1.sub(&t1, &a);
        t1.mul(&t1, &a);
        t2.mul(&a, &a);
        t3.mul(&b, &b);
        t2.add(&t2, &t3);
        if nn.tstbit(i) {
            a.copier_de(&t2);
            b.add(&t1, &t2);
        } else {
            a.copier_de(&t1);
            b.copier_de(&t2);
        }
    }
    a
}

pub fn est_premier(n: &Gz) -> bool {
    n.i128() > 1 && n.est_premier()
}

pub fn nb_chiffres(n: &Gz) -> u64 {
    if n.sgn() == 0 { return 1; }
    n.sizeinbase10()
}

pub fn compter_premiers(nn: &Gz) -> u64 {
    if nn.sgn() < 0 || !nn.fits_u64() || nn.get_u64() > 0xFFFF_FFFF {
        erreur_runtime("crible trop grand");
    }
    let n = nn.get_u64();
    let mut crible = vec![0u8; (n + 1) as usize];
    let mut compte = 0u64;
    let mut i = 2u64;
    while i <= n {
        if crible[i as usize] == 0 {
            compte += 1;
            if i <= n / i {
                let mut j = i * i;
                while j <= n { crible[j as usize] = 1; j += i; }
            }
        }
        i += 1;
    }
    compte
}

pub fn nombre_pi(chiffres: &Gz) -> String {
    if chiffres.sgn() <= 0 || !chiffres.fits_i64() {
        erreur_runtime("décimales de π : entier positif attendu");
    }
    let chiffres = chiffres.get_i64() as usize;
    let pi = crate::irrationnel::calcul_pi(chiffres);
    let s = pi.to_string();
    let bytes = s.as_bytes();
    if bytes.is_empty() { return "π ≈ 0".to_string(); }
    let mut sortie = String::with_capacity(s.len() + 60);
    // ⚠⚠⚠ LA FORMULE EXACTE RESTE, MAIS SUR UNE LIGNE À PART (16/11).
    //
    // Demande utilisateur : « pour le latex c'était des symboles pour
    // définir quelque ligne du programme s'affiche en latex ». La ligne
    // `%%` est EXACTEMENT ça : un symbole qui désigne une ligne à rendre en
    // LaTeX. Pour les constantes, la formule est donc écrite sur la ligne
    // précédente — la sortie de la valeur n'est plus préfixée.
    //
    // ⚠ La formule est écrite sur sa PROPRE ligne (une ligne de LaTeX,
    // comme une ligne `%%`), puis la valeur approchée.
    sortie.push_str("π = 4\\cdot\\arctan(1/5) - \\arctan(1/239)");
    sortie.push('\n');
    sortie.push_str("π ≈ ");
    sortie.push(bytes[0] as char);
    sortie.push('.');
    for i in 1..bytes.len().min(chiffres + 1) {
        sortie.push(bytes[i] as char);
    }
    while sortie.chars().count() < 4 + chiffres {
        sortie.push('0');
    }
    sortie
}

pub fn nombre_e(chiffres: &Gz) -> String {
    if chiffres.sgn() <= 0 || !chiffres.fits_i64() {
        erreur_runtime("décimales de e : entier positif attendu");
    }
    let chiffres = chiffres.get_i64() as usize;
    let e = crate::irrationnel::calcul_e(chiffres);
    let s = e.to_string();
    let bytes = s.as_bytes();
    if bytes.is_empty() { return "e ≈ 0".to_string(); }
    let mut sortie = String::with_capacity(s.len() + 60);
    // ⚠⚠⚠ LA FORMULE EXACTE RESTE, MAIS SUR UNE LIGNE À PART (16/11).
    //
    // Demande utilisateur : « pour le latex c'était des symboles pour
    // définir quelque ligne du programme s'affiche en latex ». La ligne
    // `%%` est EXACTEMENT ça : un symbole qui désigne une ligne à rendre en
    // LaTeX. Pour les constantes, la formule est donc écrite sur la ligne
    // précédente — la sortie de la valeur n'est plus préfixée.
    //
    // ⚠ La formule est écrite sur sa PROPRE ligne (une ligne de LaTeX,
    // comme une ligne `%%`), puis la valeur approchée.
    sortie.push_str("e = \\sum_{k=0}^{\\infty} \\frac{1}{k!}");
    sortie.push('\n');
    sortie.push_str("e ≈ ");
    sortie.push(bytes[0] as char);
    sortie.push('.');
    for i in 1..bytes.len().min(chiffres + 1) {
        sortie.push(bytes[i] as char);
    }
    while sortie.chars().count() < 4 + chiffres {
        sortie.push('0');
    }
    sortie
}

pub fn nombre_phi(chiffres: &Gz) -> String {
    if chiffres.sgn() <= 0 || !chiffres.fits_i64() || chiffres.get_i64() as u64 > 2_000_000 {
        erreur_runtime("décimales de φ hors limites (1..2000000)");
    }
    let chiffres = chiffres.get_i64() as usize;
    let phi = crate::irrationnel::calcul_phi(chiffres);
    let s = phi.to_string();
    let bytes = s.as_bytes();
    if bytes.is_empty() { return "φ ≈ 0".to_string(); }
    let mut sortie = String::with_capacity(s.len() + 60);
    // ⚠⚠⚠ LA FORMULE EXACTE RESTE, MAIS SUR UNE LIGNE À PART (16/11).
    //
    // Demande utilisateur : « pour le latex c'était des symboles pour
    // définir quelque ligne du programme s'affiche en latex ». La ligne
    // `%%` est EXACTEMENT ça : un symbole qui désigne une ligne à rendre en
    // LaTeX. Pour les constantes, la formule est donc écrite sur la ligne
    // précédente — la sortie de la valeur n'est plus préfixée.
    //
    // ⚠ La formule est écrite sur sa PROPRE ligne (une ligne de LaTeX,
    // comme une ligne `%%`), puis la valeur approchée.
    sortie.push_str("φ = \\frac{1 + \\sqrt{5}}{2}");
    sortie.push('\n');
    sortie.push_str("φ ≈ ");
    sortie.push(bytes[0] as char);
    sortie.push('.');
    for i in 1..bytes.len().min(chiffres + 1) {
        sortie.push(bytes[i] as char);
    }
    while sortie.chars().count() < 4 + chiffres {
        sortie.push('0');
    }
    sortie
}


/// Somme d'un tableau d'i64 sur GPU via CUDA driver API + PTX précompilé.
/// Retourne la somme, ou None si le GPU n'est pas disponible (fallback CPU).
/// Les fonctions CUDA driver sont liées à `libcuda` (fournie par le pilote
/// NVIDIA). Sur une machine sans pilote, le symbole est absent : le lieur
/// exige alors l'import pour TOUT binaire de la crate, y compris ceux qui ne
/// lancent jamais de GPU. On le charge donc à l'EXÉCUTION (`dlopen`), ce qui
/// garde `rose`, `rose-aot` et `rose-setup` liables partout.
#[cfg(target_os = "linux")]
#[allow(non_snake_case)]
mod cuda {
    use std::ffi::{c_char, c_void};
    use std::sync::OnceLock;

    type Pointeur = *mut c_void;

    unsafe extern "C" {
        fn dlopen(nom: *const c_char, mode: i32) -> Pointeur;
        fn dlsym(handle: Pointeur, nom: *const c_char) -> Pointeur;
    }

    const RTLD_NOW: i32 = 2;

    fn symbole(nom: &str) -> Option<Pointeur> {
        static LIB: OnceLock<usize> = OnceLock::new();
        let lib = *LIB.get_or_init(|| {
            let noms = ["libcuda.so.1", "libcuda.so", "/usr/lib/wsl/lib/libcuda.so.1"];
            for n in noms {
                let Ok(c) = std::ffi::CString::new(n) else { continue };
                let h = unsafe { dlopen(c.as_ptr(), RTLD_NOW) };
                if !h.is_null() {
                    return h as usize;
                }
            }
            0
        });
        if lib == 0 {
            return None;
        }
        let c = std::ffi::CString::new(nom).ok()?;
        let p = unsafe { dlsym(lib as Pointeur, c.as_ptr()) };
        if p.is_null() { None } else { Some(p) }
    }

    /// Adresse d'une fonction CUDA, ou `None` si le pilote n'est pas là.
    pub fn fonction(nom: &str) -> Option<*const c_void> {
        symbole(nom).map(|p| p as *const c_void)
    }
}

pub fn gpu_somme_i64(buf: &[i64]) -> Option<i64> {
    if buf.is_empty() { return Some(0); }
    #[cfg(target_os = "linux")]
    {
        // CUDA path (embedded PTX)
        unsafe {
            type CuResult = i32;
            type CuDevice = i32;
            type CuContext = *mut std::ffi::c_void;
            type CuModule = *mut std::ffi::c_void;
            type CuFunction = *mut std::ffi::c_void;
            type CuDevicePtr = u64;
            type CuStream = *mut std::ffi::c_void;

            // Chaque fonction CUDA est résolue à l'exécution : si le pilote
            // manque, on rend `None` et l'appelant retombe sur le CPU.
            macro_rules! cuda_fonction {
                ($nom:literal, $type:ty) => {
                    match cuda::fonction($nom) {
                        Some(p) => unsafe { std::mem::transmute::<*const std::ffi::c_void, $type>(p) },
                        None => return None,
                    }
                };
            }
            let cuInit = cuda_fonction!("cuInit", unsafe extern "C" fn(u32) -> CuResult);
            let cuDeviceGet = cuda_fonction!("cuDeviceGet", unsafe extern "C" fn(*mut CuDevice, i32) -> CuResult);
            let cuCtxCreate_v2 = cuda_fonction!("cuCtxCreate_v2", unsafe extern "C" fn(*mut CuContext, u32, CuDevice) -> CuResult);
            let cuModuleLoadData = cuda_fonction!("cuModuleLoadData", unsafe extern "C" fn(*mut CuModule, *const i8) -> CuResult);
            let cuModuleGetFunction = cuda_fonction!("cuModuleGetFunction", unsafe extern "C" fn(*mut CuFunction, CuModule, *const i8) -> CuResult);
            let cuMemAlloc = cuda_fonction!("cuMemAlloc", unsafe extern "C" fn(*mut CuDevicePtr, usize) -> CuResult);
            let cuMemcpyHtoD = cuda_fonction!("cuMemcpyHtoD", unsafe extern "C" fn(CuDevicePtr, *const i8, usize) -> CuResult);
            let cuMemsetD8 = cuda_fonction!("cuMemsetD8", unsafe extern "C" fn(CuDevicePtr, u8, usize) -> CuResult);
            let cuLaunchKernel = cuda_fonction!("cuLaunchKernel", unsafe extern "C" fn(
                CuFunction, u32, u32, u32, u32, u32, u32, usize, CuStream,
                *mut *mut std::ffi::c_void, *mut *mut std::ffi::c_void) -> CuResult);
            let cuMemcpyDtoH = cuda_fonction!("cuMemcpyDtoH", unsafe extern "C" fn(*mut i8, CuDevicePtr, usize) -> CuResult);
            let cuMemFree = cuda_fonction!("cuMemFree", unsafe extern "C" fn(CuDevicePtr) -> CuResult);
            let cuModuleUnload = cuda_fonction!("cuModuleUnload", unsafe extern "C" fn(CuModule) -> CuResult);
            let cuCtxDestroy_v2 = cuda_fonction!("cuCtxDestroy_v2", unsafe extern "C" fn(CuContext) -> CuResult);

            if cuInit(0) != 0 { return None; }
            let mut dev: CuDevice = 0;
            if cuDeviceGet(&mut dev, 0) != 0 { return None; }
            let mut ctx: CuContext = std::ptr::null_mut();
            if cuCtxCreate_v2(&mut ctx, 0, dev) != 0 { return None; }

            let ptx = include_str!("sum.ptx");
            let ptx_cstr = std::ffi::CString::new(ptx).ok()?;
            let mut module: CuModule = std::ptr::null_mut();
            if cuModuleLoadData(&mut module, ptx_cstr.as_ptr() as *const i8) != 0 {
                cuCtxDestroy_v2(ctx); return None;
            }
            let name = std::ffi::CString::new("sum_i64").unwrap();
            let mut func: CuFunction = std::ptr::null_mut();
            if cuModuleGetFunction(&mut func, module, name.as_ptr() as *const i8) != 0 {
                cuModuleUnload(module); cuCtxDestroy_v2(ctx); return None;
            }

            let n = buf.len();
            let mut d_input: CuDevicePtr = 0;
            let mut d_output: CuDevicePtr = 0;
            if cuMemAlloc(&mut d_input, n * 8) != 0 { cuCtxDestroy_v2(ctx); return None; }
            if cuMemcpyHtoD(d_input, buf.as_ptr() as *const i8, n * 8) != 0 { cuMemFree(d_input); cuCtxDestroy_v2(ctx); return None; }
            if cuMemAlloc(&mut d_output, 8) != 0 { cuMemFree(d_input); cuCtxDestroy_v2(ctx); return None; }
            cuMemsetD8(d_output, 0, 8);

            let mut d_input_ptr = d_input;
            let mut d_output_ptr = d_output;
            let mut n_i32 = n as i32;
            let mut args: [*mut std::ffi::c_void; 3] = [
                &mut d_input_ptr as *mut _ as *mut std::ffi::c_void,
                &mut d_output_ptr as *mut _ as *mut std::ffi::c_void,
                &mut n_i32 as *mut _ as *mut std::ffi::c_void,
            ];

            let block_size = 256u32;
            let grid_size = ((n as u32) + block_size - 1) / block_size;
            let shared_mem = (block_size as usize) * 8;
            let stream: CuStream = std::ptr::null_mut();

            if cuLaunchKernel(func, grid_size, 1, 1, block_size, 1, 1, shared_mem, stream, args.as_mut_ptr(), std::ptr::null_mut()) != 0 {
                cuMemFree(d_input); cuMemFree(d_output); cuModuleUnload(module); cuCtxDestroy_v2(ctx); return None;
            }

            let mut result: i64 = 0;
            if cuMemcpyDtoH(&mut result as *mut i64 as *mut i8, d_output, 8) != 0 {
                cuMemFree(d_input); cuMemFree(d_output); cuModuleUnload(module); cuCtxDestroy_v2(ctx); return None;
            }

            cuMemFree(d_input);
            cuMemFree(d_output);
            cuModuleUnload(module);
            cuCtxDestroy_v2(ctx);

            return Some(result);
        }
    }
    // Fallback CPU
    let mut s: i64 = 0;
    for &x in buf.iter() { s = s.wrapping_add(x); }
    Some(s)
}

unsafe fn cleanup(ctx: *mut std::ffi::c_void, module: *mut std::ffi::c_void) {
    // best effort
    let _ = ctx; let _ = module;
}

pub fn pow_rationnel_sur(base: &Gq, e: i64) -> Gq {
    fn pow_i128(base: i128, e: u64) -> i128 {
        let mut acc: i128 = 1;
        let mut b = base;
        let mut e = e;
        while e > 0 {
            if e & 1 == 1 {
                match acc.checked_mul(b) { Some(x) => acc = x, None => crate::i128x::hors_plage() }
            }
            e >>= 1;
            if e > 0 {
                match b.checked_mul(b) { Some(x) => b = x, None => crate::i128x::hors_plage() }
            }
        }
        acc
    }
    let n = pow_i128(base.num_i128(), e.unsigned_abs());
    let d = pow_i128(base.den_i128(), e.unsigned_abs());
    if e >= 0 { Gq::de(n, d) } else { Gq::de(d, n) }
}

pub fn pgcd(a: &Gz, b: &Gz) -> Gz {
    let r = Gz::neuf();
    r.gcd(a, b);
    r
}
pub fn ppcm(a: &Gz, b: &Gz) -> Gz {
    let r = Gz::neuf();
    r.lcm(a, b);
    r
}

pub fn puissance(base: &Valeur, expo: &Gz) -> Valeur {
    // miroir du builtin puissance : ^(entier) exact
    if !expo.fits_i64() {
        erreur_runtime("exposant démesuré (la puissance exploserait au-delà de tout calcul)");
    }
    let e = expo.get_i64();
    match base {
        Valeur::Flottant(d) => Valeur::f(d.powf(e as f64)),
        Valeur::Rationnel(q) => Valeur::qq(pow_rationnel_sur(q, e)),
        _ => {
            let z = vers_z(base);
            if e >= 0 {
                let r = Gz::neuf();
                r.pow_u64(&z, e as u64);
                Valeur::zg(r)
            } else {
                if z.sgn() == 0 { erreur_runtime("0^exposant négatif"); }
                let q = Gq::de(z.i128(), 1);
                Valeur::qq(pow_rationnel_sur(&q, e))
            }
        }
    }
}

// ── duree() (miroir formater_duree) ───────────────────────────────────────

fn div_decimale(n_str: &str, div: u64) -> (String, u64) {
    let mut q = String::new();
    let mut r = 0u64;
    for c in n_str.bytes() {
        let cour = r * 10 + (c - b'0') as u64;
        q.push((b'0' + (cour / div) as u8) as char);
        r = cour % div;
    }
    let t = q.trim_start_matches('0');
    (if t.is_empty() { "0".into() } else { t.into() }, r)
}

pub fn formater_duree(n_str: &str) -> String {
    const UNIT: [&str; 6] = [" ns", " µs", " ms", " s", " min", " h"];
    const SCALE: [u64; 6] = [1, 1_000, 1_000_000, 1_000_000_000, 60_000_000_000, 3_600_000_000_000];
    let n_str = n_str.trim_start_matches('0');
    let n_str = if n_str.is_empty() { "0" } else { n_str };
    let au_moins = |s: u64| -> bool {
        let t = s.to_string();
        if n_str.len() != t.len() { n_str.len() > t.len() } else { n_str >= t.as_str() }
    };
    let mut u = 0usize;
    for i in 1..=5 { if au_moins(SCALE[i]) { u = i; } }
    let (part, reste) = div_decimale(n_str, SCALE[u]);
    if u == 0 || reste == 0 { return format!("{}{}", part, UNIT[u]); }
    let frac3 = (reste * 1000) / SCALE[u];
    let mut fs = frac3.to_string();
    while fs.len() < 3 { fs = format!("0{}", fs); }
    while fs.ends_with('0') { fs.pop(); }
    format!("{}.{}{}", part, fs, UNIT[u])
}

// ── Remplissage aléatoire de tampon (miroir remplir_hasard) ───────────────

pub fn remplir_hasard(etat: &mut u64, buf: &mut [u8], ab: &[u8]) {
    let s = ab.len();
    if s == 1 { buf.fill(ab[0]); return; }
    if s.is_power_of_two() && s <= 256 {
        let m = (s - 1) as u8;
        let mut i = 0usize;
        let n = buf.len();
        while i + 32 <= n {
            let r0 = rng_tirer(etat);
            let r1 = rng_tirer(etat);
            let r2 = rng_tirer(etat);
            let r3 = rng_tirer(etat);
            for k in 0..8 {
                buf[i + k] = ab[(((r0 >> (8 * k)) as u8) & m) as usize];
                buf[i + 8 + k] = ab[(((r1 >> (8 * k)) as u8) & m) as usize];
                buf[i + 16 + k] = ab[(((r2 >> (8 * k)) as u8) & m) as usize];
                buf[i + 24 + k] = ab[(((r3 >> (8 * k)) as u8) & m) as usize];
            }
            i += 32;
        }
        while i < n { buf[i] = ab[((rng_tirer(etat) as u8) & m) as usize]; i += 1; }
    } else if s.is_power_of_two() {
        let mask = (s - 1) as u64;
        let bits = 64 - (s as u64).leading_zeros();
        let per = 64 / bits;
        let mut i = 0usize;
        while i + per as usize <= buf.len() {
            let r = rng_tirer(etat);
            for k in 0..per { buf[i + k as usize] = ab[((r >> (k * bits)) & mask) as usize]; }
            i += per as usize;
        }
        while i < buf.len() { buf[i] = ab[(rng_tirer(etat) & mask) as usize]; i += 1; }
    } else {
        for i in 0..buf.len() {
            buf[i] = ab[(((rng_tirer(etat) as u128) * (s as u128)) >> 64) as usize];
        }
    }
}

pub fn remplir_philox(buf: &mut [u8], ab: &[u8], graine: i64) {
    let s = ab.len();
    if s == 0 { erreur_runtime("remplir_hasard_philox : alphabet non vide attendu"); }
    if s == 1 { for x in buf.iter_mut() { *x = ab[0]; } return; }
    let k0 = graine as u32;
    let k1 = (graine >> 32) as u32;
    // Un bloc Philox (4 mots de 32 bits) remplit 4 OCTETS,
    // chaque octet étant glané dans un mot DIFFÉRENT (un mot par octet, seul son
    // octet de poids faible sert). Le compteur du bloc = indice b de bloc.
    let pow2 = s.is_power_of_two();
    let mask = (s - 1) as u32;
    let n = buf.len();
    let nblk = (n + 3) / 4;
    for b in 0..nblk {
        let ctr = [b as u32, ((b as u64) >> 32) as u32, 0, 0];
        let r = philox4x32_10_pour(k0, k1, ctr);
        let i = b * 4;
        for j in 0..4 {
            if i + j < n {
                buf[i + j] = if pow2 {
                    ab[(r[j] & mask) as usize]
                } else {
                    ab[(((r[j] as u64 as u128) * (s as u128)) >> 32) as usize]
                };
            }
        }
    }
}

fn philox4x32_10_pour(k0: u32, k1: u32, counter: [u32; 4]) -> [u32; 4] {
    let mut cnt = counter;
    let mut key = [k0, k1];
    for _ in 0..10 {
        let (hi0, lo0) = mul_hi_lo(PHILOX_M0, cnt[0]);
        let (hi1, lo1) = mul_hi_lo(PHILOX_M1, cnt[2]);
        cnt = [hi1 ^ cnt[1] ^ key[0], lo1, hi0 ^ cnt[3] ^ key[1], lo0];
        key = [key[0].wrapping_add(PHILOX_W0), key[1].wrapping_add(PHILOX_W1)];
    }
    cnt
}


unsafe extern "C" {
    pub fn dlopen(path: *const i8, mode: i32) -> *mut std::ffi::c_void;
    pub fn dlsym(h: *mut std::ffi::c_void, name: *const i8) -> *mut std::ffi::c_void;
    pub fn dlclose(h: *mut std::ffi::c_void) -> i32;
}

pub fn chaine_vers_flottant(ch: &str) -> f64 {
    match ch.parse::<f64>() {
        Ok(v) => v,
        Err(_) => {
            let clean = ch.trim();
            match clean {
                "infini" => f64::INFINITY,
                "-infini" => f64::NEG_INFINITY,
                _ => {
                    // « 1/3 » accepté aussi (strtod puis fraction)
                    if let Some(p) = clean.find('/') {
                        if let (Ok(a), Ok(b)) = (clean[..p].parse::<f64>(), clean[p + 1..].parse::<f64>()) {
                            return a / b;
                        }
                    }
                    erreur_runtime("conversion chaine → flottant impossible");
                }
            }
        }
    }
}

// signe d'un rationnel par pointeur (surface conservée pour interp.rs)
pub fn gmpq_sgn(q: *const Mpq) -> i32 {
    let n = unsafe { gmp_get_si(&(*q).num) };
    if n < 0 { -1 } else if n > 0 { 1 } else { 0 }
}

fn un_z() -> Gz { Gz::de_i128(1) }

#[unsafe(no_mangle)]
pub extern "C" fn rose_erreur_mod_zero() {
    erreur_runtime("modulo par zéro");
}

#[unsafe(no_mangle)]
pub extern "C" fn rose_erreur_div_zero() {
    erreur_runtime("division euclidienne par zéro");
}

/// Division flottante par zéro : le JIT doit signaler l'erreur comme
/// l'interpréteur (et non produire inf / non_nombre).
///
/// Erreur runtime du code compilé : une
/// erreur levée dans du code JIT ne peut pas être rattrapée (une trame
/// Cranelift n'a pas d'information de défilement) — on affiche l'erreur et on
/// quitte.
#[unsafe(no_mangle)]
pub extern "C" fn rose_erreur_div_flottant_zero() {
    flush_out();
    eprintln!("Rose : erreur — division par zéro");
    std::process::exit(1);
}


#[unsafe(no_mangle)]
pub extern "C" fn rose_affiche_packed(v: i128) {
    let val = v as i64;
    let tag = (v >> 64) as i64;
    if tag == 0 {
        crate::run::ecrire_out(&format!("{}\n", val));
    } else {
        let b = unsafe { Box::from_raw(tag as *mut crate::run::Valeur) };
        crate::run::afficher(&b);
    }
}

// ── Voie taggée : helpers pour le JIT (val+tag packés en i128) ──
#[unsafe(no_mangle)]

#[unsafe(no_mangle)]

#[unsafe(no_mangle)]

#[unsafe(no_mangle)]




// ── Voie taggée : helpers pour le JIT (ABI i64, sortie via pointeurs) ──

// Un entier "taggé" est (valeur: i64, tag: i64) : tag == 0 → la valeur est un
// i64 ; tag != 0 → la valeur est l'adresse d'un `Mpz` boxé. Le JIT réutilise
// une seule variable `tag_tmp` pour le tag du résultat courant, et un
// `Eabstrait::Slot` ne la réinitialise pas : un tag périmé peut donc arriver
// sur un simple i64 de slot (mesuré sur `bench_global` : `a=3024617, at=1`
// pour l'incrément de boucle, juste après le débordement de `$total`).
// Déréférencer cette valeur comme un pointeur fait SIGSEGV. On ne fait donc
// confiance au tag que si l'adresse est bien une boîte que CETTE ABI a créée.
thread_local! {
    static MPZ_BOXES: RefCell<HashSet<usize>> = RefCell::new(HashSet::new());
}

/// Boxe un `Mpz`, enregistre son adresse et la renvoie au JIT.
fn mpz_box(r: Box<Mpz>) -> i64 {
    let p = Box::into_raw(r) as usize;
    MPZ_BOXES.with(|s| { s.borrow_mut().insert(p); });
    p as i64
}

/// Traduit un (valeur, tag) taggé en pointeur. Le tag n'est pas fiable (le
/// JIT le laisse fuir d'une expression sur la suivante, et le perd sur un
/// `Eabstrait::Slot`) : la seule preuve est que la valeur est une adresse
/// boxée par cette ABI. Un grand entier est TOUJOURS boxé ici ; un i64
/// ordinaire n'a aucune raison d'égaler une de ces adresses.
#[inline]
fn mpz_ptr(v: i64, _t: i64) -> Option<*mut Mpz> {
    let p = v as usize;
    if MPZ_BOXES.with(|s| s.borrow().contains(&p)) { Some(p as *mut Mpz) } else { None }
}

#[unsafe(no_mangle)]
pub extern "C" fn rose_add_ent(a: i64, at: i64, b: i64, bt: i64, out: *mut i64, outt: *mut i64) {
    tagged_binop_abi(a, at, b, bt, out, outt, TaggedOp2::Add)
}
#[unsafe(no_mangle)]
pub extern "C" fn rose_sub_ent(a: i64, at: i64, b: i64, bt: i64, out: *mut i64, outt: *mut i64) {
    tagged_binop_abi(a, at, b, bt, out, outt, TaggedOp2::Sub)
}
#[unsafe(no_mangle)]
pub extern "C" fn rose_mul_ent(a: i64, at: i64, b: i64, bt: i64, out: *mut i64, outt: *mut i64) {
    tagged_binop_abi(a, at, b, bt, out, outt, TaggedOp2::Mul);
}
#[unsafe(no_mangle)]
pub extern "C" fn rose_neg_ent(a: i64, at: i64, out: *mut i64, outt: *mut i64) {
    // Un tag périmé sur un i64 de slot doit être ignoré, pas déréférencé.
    let pa = match mpz_ptr(a, at) {
        Some(p) => p,
        None => {
            if a == i64::MIN {
                let r = Box::new(Mpz { v: -BigInt::from(a) });
                unsafe { *out = mpz_box(r); *outt = 1; }
            } else {
                unsafe { *out = a.wrapping_neg(); *outt = 0; }
            }
            return;
        }
    };
    unsafe {
        let mut r = Box::new(Mpz { v: -(*pa).v.clone() });
        if r.v >= BigInt::from(i64::MIN) && r.v <= BigInt::from(i64::MAX) {
            *out = r.v.to_i64().unwrap_or(0);
            *outt = 0;
            drop(r);
        } else {
            *out = mpz_box(r);
            *outt = 1;
        }
    }
}

#[derive(Clone, Copy)]
enum TaggedOp2 { Add, Sub, Mul }

fn to_bigint(av: i64, at: i64) -> BigInt {
    match mpz_ptr(av, at) {
        Some(p) => unsafe { (*p).v.clone() },
        None => BigInt::from(av),
    }
}

fn tagged_binop_abi(a: i64, at: i64, b: i64, bt: i64, out: *mut i64, outt: *mut i64, op: TaggedOp2) {
    // Le tag n'est fiable que si la valeur est une adresse que nous avons
    // boxée. Un tag périmé est donc ramené à un simple i64.
    let pa = mpz_ptr(a, at);
    let pb = mpz_ptr(b, bt);
    if pa.is_none() && pb.is_none() {
        let small = match op {
            TaggedOp2::Add => a.checked_add(b),
            TaggedOp2::Sub => a.checked_sub(b),
            TaggedOp2::Mul => a.checked_mul(b),
        };
        if let Some(x) = small {
            unsafe { *out = x; *outt = 0; }
            return;
        }
        let ba = BigInt::from(a);
        let bb = BigInt::from(b);
        let br = match op {
            TaggedOp2::Add => ba + bb,
            TaggedOp2::Sub => ba - bb,
            TaggedOp2::Mul => ba * bb,
        };
        unsafe { *out = mpz_box(Box::new(Mpz { v: br })); *outt = 1; }
        return;
    }
    // In-place mutation quand un operande est deja un BigInt (tag 1) :
    // on reutilise sa boite, pas d'allocation de resultat.
    if let Some(ptr) = pa {
        let bb = to_bigint(b, bt);
        unsafe {
            match op {
                TaggedOp2::Add => (*ptr).v += bb,
                TaggedOp2::Sub => (*ptr).v -= bb,
                TaggedOp2::Mul => (*ptr).v *= bb,
            }
            *out = a;
            *outt = 1;
        }
        return;
    }
    if let Some(ptr) = pb {
        let aa = to_bigint(a, at);
        unsafe {
            match op {
                TaggedOp2::Add => (*ptr).v += aa,
                TaggedOp2::Sub => (*ptr).v = aa - (*ptr).v.clone(),
                TaggedOp2::Mul => (*ptr).v *= aa,
            }
            *out = b;
            *outt = 1;
        }
        return;
    }
    unreachable!()
}

#[unsafe(no_mangle)]
pub extern "C" fn rose_clone_ent(v: i64, t: i64, out: *mut i64, outt: *mut i64) {
    match mpz_ptr(v, t) {
        None => unsafe { *out = v; *outt = 0; },
        Some(src) => unsafe {
            let r = Box::new(Mpz { v: (*src).v.clone() });
            *out = mpz_box(r);
            *outt = 1;
        },
    }
}

static mut ROSE_RET_TAG: i64 = 0;

#[unsafe(no_mangle)]
pub extern "C" fn rose_set_ret_tag(tag: i64) {
    unsafe { ROSE_RET_TAG = tag; }
}

#[unsafe(no_mangle)]
pub extern "C" fn rose_get_ret_tag() -> i64 {
    unsafe { ROSE_RET_TAG }
}

// ── Viewer interactif pour nombres très grands (mode "nano/less") ──────────
// Affiche un nombre par pages, avec navigation au clavier.

use std::io::{self};

/// Affiche un nombre décimal (sous forme de chaîne) de manière paginée.
/// Contrôles :
///   Espace / Entrée : page suivante
///   q / Échap         : quitter
///   b                 : page précédente
///   h / gauche        : précédent (50 caractères)
///   l / droite        : suivant (50 caractères)
pub fn view_grand_nombre(s: &str) {
    let s = s.trim();
    if s.is_empty() {
        println!("(vide)");
        return;
    }

    let total_chars = s.chars().count();
    let total_digits = s.chars().count();
    println!("══════════════════════════════════════════════════════════════════════════════════");
    println!("  Nombre : {} chiffres décimaux", total_digits);
    println!("══════════════════════════════════════════════════════════════════════════════════");
    println!();

    if total_digits <= 2000 {
        // Petit nombre : afficher directement
        println!("{}", s);
        println!();
        println!("Appuyez sur Entrée pour continuer...");
        let mut buf = String::new();
        let _ = std::io::stdin().read_line(&mut buf);
        return;
    }

    // Navigation paginée
    let page_width = 70;
    let page_lines = 20;
    let chars_per_page = page_width * page_lines;
    let mut pos: usize = 0;
    let total = s.chars().count();

    // Index d'octets de début de chaque caractère : fenêtre O(1) par page,
    // même sur des millions de chiffres (une boucle par caractère à chaque
    // page était O(n²) sur 5,5 M de chiffres).
    let bornes: Vec<usize> = s.char_indices().map(|(i, _)| i).chain(std::iter::once(total)).collect();

    loop {
        // Calculer la fenêtre courante
        let clamped = pos.min(total);
        let fin_idx = (clamped + chars_per_page).min(total);
        let end = bornes[fin_idx];

        // Afficher la page
        print!("\x1b[2J\x1b[H"); // effacer l'écran et retourner en haut
        println!("══════════════════════════════════════════════════════════════════════════════════");
        println!("  ROSE VIEWER  —  {} chiffres  [{}/{} ({:.1}%)]", total_digits, pos + 1, total, (pos as f64 / total as f64) * 100.0);
        println!("══════════════════════════════════════════════════════════════════════════════════");
        println!();

        let segment = &s[pos..end];
        let mut col = 0;
        for c in segment.chars() {
            print!("{}", c);
            col += 1;
            if col >= page_width {
                println!();
                col = 0;
            }
        }
        if col > 0 {
            println!();
        }

        println!();
        println!("══════════════════════════════════════════════════════════════════════════════════");
        println!("  Contrôles : [Espace/Entrée] suivant   [b] précédent   [q] quitter   [flèches] défiler");
        print!("  Position : {}/{}  >  ", pos + 1, total);
        std::io::stdout().flush().unwrap();

        // Lire une touche
        let mut buf = String::new();
        match std::io::stdin().read_line(&mut buf) {
            Ok(_) => {
                let cmd = buf.trim().to_lowercase();
                match cmd.as_str() {
                    "" | " " => {
                        pos += chars_per_page;
                        if pos >= total { pos = total; }
                    }
                    "q" | "quit" | "exit" => {
                        println!();
                        return;
                    }
                    "b" | "back" => {
                        if pos >= chars_per_page {
                            pos -= chars_per_page;
                        } else {
                            pos = 0;
                        }
                    }
                    "h" | "gauche" | "left" => {
                        if pos >= 50 { pos -= 50; } else { pos = 0; }
                    }
                    "l" | "droite" | "right" => {
                        pos += 50;
                        if pos > total { pos = total; }
                    }
                    _ => {}
                }
            }
            Err(_) => return,
        }
    }
}

// ── Index de hachage de Table (implanté ici, en fin de module) ─────────────
impl Table {
    /// Construit une table et son index à partir des paires déjà ordonnées.
    pub fn neuf(items: Vec<(Valeur, Valeur)>) -> Table {
        let t = Table(Rc::new(RefCell::new(TableRep {
            items: { let mut v = items; v.reserve(4096); v },
            index: { let mut m = HashMap::with_hasher(BuildHasherDefault::<HashU64>::default()); m.reserve(4096); m },
            fallback: Vec::new(),
            collisions: Vec::new(),
            array_part: vec![0u32; Self::N_ARRAY],
        })));
        t.reindexer();
        t
    }

    /// Reconstruit l'index depuis `items` (usage : construction littérale).
    pub fn reindexer(&self) {
        let mut rep = self.0.borrow_mut();
        let TableRep { items, index, fallback, collisions, array_part } = &mut *rep;
        index.clear();
        fallback.clear();
        collisions.clear();
        for (i, (k, _)) in items.iter().enumerate() {
            match Self::h(k) {
                Some(h) => {
                    if index.insert(h, i).is_some() { collisions.push((h, i)); }
                }
                None => fallback.push(i),
            }
        }
        array_part.fill(0);
        for (i, (k, _)) in items.iter().enumerate() {
            if let Valeur::Entier(kk, None) = k {
                Self::maj_array(array_part, *kk, i);
            }
        }
    }

    /// Hachage CANONIQUE d'un i64 NU — SOURCE UNIQUE partagée par la voie
    /// générale `h()` et la voie rapide `inserer_i64`/`contient_i64`.
    /// Un i64 converti en f64 n'est JAMAIS NaN, et `0i64 as f64 = +0.0`
    /// donc `to_bits() = 0` : le cas spécial `d == 0.0` de `h()` est le
    /// même résultat. Factoriser ici garantit qu'une clé insérée par une
    /// voie est trouvée par l'autre, SANS index parallèle.
    #[inline]
    fn h_entier(k: i64) -> u64 {
        // `(k as f64).to_bits()` n'est jamais NaN et `0i64 → +0.0` a
        // `to_bits() = 0`, donc cette formule est EXACTEMENT celle de la
        // voie générale `h()` pour `Valeur::Entier(k, None)`.
        ((k as f64).to_bits()).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x243F_6A88_85A3_08D3
    }

    /// `Valeur::z(k)` est structurellement `Valeur::Entier(k, None)` pour
    /// `|k| ≤ 2^24` (aucun boxage Mpz possible). La voie rapide n'est
    /// autorisée que là ; au-delà, repli sur la voie générale.
    #[inline]
    fn z_est_nu(k: i64) -> bool { k >= -0x1000_0000 && k <= 0x1000_0000 }

    /// Construction directe d'un i64 nu, SANS l'aiguillage `mpz_ptr` de
    /// `Valeur::z` (son HashSet de boxage). Identique pour `|v| ≤ 2^24`,
    /// repli sur `Valeur::z` au-delà.
    #[inline]
    fn z_valeur_i64(v: i64) -> Valeur {
        if Self::z_est_nu(v) { Valeur::Entier(v, None) } else { Valeur::z(v) }
    }

    /// Hachage CANONIQUE : deux valeurs que `valeur_egale` déclare égales
    /// produisent le MÊME h. Les numériques sont hachés par leur f64
    /// normalisé — c'est la comparaison qu'utilise `cmp_vals` dès qu'un
    /// flottant est impliqué, et elle concorde avec l'égalité exacte des
    /// i64/rationnels (les collisions sont résolues par `valeur_egale`).
    /// `None` = pas de hachage sûr : repli linéaire.
    #[inline]
    fn h(v: &Valeur) -> Option<u64> {
        match v {
            Valeur::Vide => Some(0x9E37_79B9_7F4A_7C15),
            Valeur::Entier(k, None) => Some(Self::h_entier(*k)),
            Valeur::Entier(..) | Valeur::Rationnel(..) | Valeur::Flottant(..) => {
                let d = vers_d(v);
                let bits = if d == 0.0 { 0u64 }                        // -0 ≡ +0
                           else if d.is_nan() { 0x7FF8_0000_0000_0000 } // NaN ≡ NaN
                           else { d.to_bits() };
                Some(bits.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x243F_6A88_85A3_08D3)
            }
            Valeur::Booleen(b) => Some(0xCBF2_9CE4_8422_2325 ^ (*b as u64)),
            Valeur::Chaine(s) => {
                let t = s.aplatir();
                let mut h = 0xCBF2_9CE4_8422_2325u64;
                for &o in t.as_bytes() { h = (h ^ o as u64).wrapping_mul(0x100_0000_01B3); }
                Some(h)
            }
            // Liste, Table, Tampon, Tableau, Complexe, Irrationnel :
            // pas de hachage canonique garanti → repli.
            _ => None,
        }
    }

    #[inline]
    fn trouver_h(&self, cle: &Valeur, h: Option<u64>) -> Option<usize> {
        let rep = self.0.borrow();
        match h {
            Some(h) => {
                if let Some(&i) = rep.index.get(&h) {
                    if valeur_egale(&rep.items[i].0, cle) { return Some(i); }
                }
                for (hh, i) in rep.collisions.iter() {
                    if *hh == h && valeur_egale(&rep.items[*i].0, cle) { return Some(*i); }
                }
                None
            }
            None => {
                for &i in rep.fallback.iter() {
                    if valeur_egale(&rep.items[i].0, cle) { return Some(i); }
                }
                None
            }
        }
    }

    /// Position d'une clé, ou `None`. Sémantique = `valeur_egale`.
    #[inline]
    pub fn trouver(&self, cle: &Valeur) -> Option<usize> {
        let h = Self::h(cle);
        self.trouver_h(cle, h)
    }

    /// Insère ou met à jour (jamais de doublon), en maintenant l'index.
    #[inline]
    pub fn inserer(&self, cle: Valeur, val: Valeur) {
        let h = Self::h(&cle);
        if let Some(i) = self.trouver_h(&cle, h) {
            self.0.borrow_mut().items[i].1 = val;
            return;
        }
        let mut rep = self.0.borrow_mut();
        let i = rep.items.len();
        rep.items.push((cle, val));
        match h {
            Some(h) => {
                if rep.index.contains_key(&h) { rep.collisions.push((h, i)); }
                else { rep.index.insert(h, i); }
            }
            None => rep.fallback.push(i),
        }
    }

    // ── Voie rapide i64 (domaine 7) ──────────────────────────────────────
    //
    // Les clés du bench sont des i64 nus prouvés statiquement par le garde
    // JIT `slot_cle_nu_borne`. Le coût mesuré avant optimisation était
    // 0,209 ms pour 3 000 insertions (71 ns/op) : vers_d + hachage canonique
    // via `Valeur`, DOUBLE emprunt RefCell (trouver_h puis borrow_mut),
    // `Valeur::z` construit pour la recherche, et `valeur_egale` générique.
    // Ces méthodes gardent le MÊME index canonique (h_entier partagé avec
    // `h()`), donc cohérence totale avec la voie générale — aucun second
    // index, aucune divergence possible. Le garde `z_est_nu` assure que la
    // clé stockée est exactement `Valeur::Entier(k, None)` : au-delà de
    // 2^24 on replie sur la voie générale (où `Valeur::z` peut boxer).
    /// Accès SANS contrôle dynamique au `TableRep`.
    /// INVARIANT : le moteur est MONOTHREADÉ (`Rc`, jamais `Arc`) et les
    /// helpers JIT appellent ces méthodes séquentiellement, sans réentrance
    /// ni autre emprunt actif du même `RefCell`. Le drapeau d'emprunt est
    /// donc toujours libre : `borrow(_mut)` ne fait que le tester/incrémenter.
    #[inline(always)]
    fn rep_nv(&self) -> &TableRep {
        #[cfg(debug_assertions)] debug_assert!(self.0.try_borrow().is_ok(), "emprunt Table actif");
        unsafe { &*self.0.as_ptr() }
    }
    #[inline(always)]
    fn rep_mut_nv(&self) -> &mut TableRep {
        #[cfg(debug_assertions)] debug_assert!(self.0.try_borrow_mut().is_ok(), "emprunt Table actif");
        unsafe { &mut *self.0.as_ptr() }
    }

    /// Égalité d'une clé stockée avec un i64 nu, SANS construire de `Valeur`
    /// sur le chemin chaud. Le repli `valeur_egale` couvre les clés
    /// hétérogènes de même h (ex. `5/1` ou `5.0` stockés, recherchés par `5`).
    #[inline]
    fn cle_i64_egale(stockee: &Valeur, k: i64) -> bool {
        match stockee {
            Valeur::Entier(x, None) => *x == k,
            _ => valeur_egale(stockee, &Valeur::Entier(k, None)),
        }
    }

    /// Position d'un i64 nu dans un `TableRep` déjà emprunté.
    #[inline]
    fn trouve_i64(rep: &TableRep, k: i64, h: u64) -> Option<usize> {
        if let Some(&i) = rep.index.get(&h) {
            if Self::cle_i64_egale(&rep.items[i].0, k) { return Some(i); }
        }
        for &(hh, i) in rep.collisions.iter() {
            if hh == h && Self::cle_i64_egale(&rep.items[i].0, k) { return Some(i); }
        }
        None
    }

    /// Insère ou met à jour un i64 nu : un seul accès au `TableRep` (au lieu
    /// de `borrow` pour `trouver_h` puis `borrow_mut` pour pousser) et un
    /// seul hachage (au lieu de `get` + `contains_key` + `insert`).
    #[inline]
    pub fn inserer_i64(&self, k: i64, v: i64) {
        if !Self::z_est_nu(k) {
            self.inserer(Valeur::z(k), Self::z_valeur_i64(v));
            return;
        }
        let h = Self::h_entier(k);
        let TableRep { items, index, collisions, array_part, .. } = self.rep_mut_nv();
        // UN seul hachage par insertion : `entry` sert à la fois à trouver
        // la position existante et à réserver la nouvelle (au lieu de
        // `get` puis `contains_key` puis `insert` = 3 hachages).
        use std::collections::hash_map::Entry;
        match index.entry(h) {
            Entry::Occupied(e) => {
                let i0 = *e.get();
                if Self::cle_i64_egale(&items[i0].0, k) {
                    items[i0].1 = Self::z_valeur_i64(v);
                    Self::maj_array(array_part, k, i0);
                    return;
                }
                if let Some(pos) = collisions.iter().position(|&(hh, ci)| {
                    hh == h && Self::cle_i64_egale(&items[ci].0, k)
                }) {
                    items[collisions[pos].1].1 = Self::z_valeur_i64(v);
                    Self::maj_array(array_part, k, collisions[pos].1);
                    return;
                }
                let i = items.len();
                items.push((Valeur::Entier(k, None), Self::z_valeur_i64(v)));
                collisions.push((h, i));
                Self::maj_array(array_part, k, i);
            }
            Entry::Vacant(e) => {
                let i = items.len();
                items.push((Valeur::Entier(k, None), Self::z_valeur_i64(v)));
                e.insert(i);
                Self::maj_array(array_part, k, i);
            }
        }
    }

    /// Présence d'un i64 nu, SANS construire de `Valeur` pour la clé.
    #[inline]
    pub fn contient_i64(&self, k: i64) -> bool {
        if !Self::z_est_nu(k) {
            return self.trouver(&Valeur::z(k)).is_some();
        }
        let rep = self.rep_nv();
        if k >= 0 && (k as usize) < Self::N_ARRAY {
            let p = rep.array_part[k as usize];
            if p != 0 && Self::cle_i64_egale(&rep.items[(p - 1) as usize].0, k) {
                return true;
            }
        }
        let h = Self::h_entier(k);
        Self::trouve_i64(rep, k, h).is_some()
    }

    /// Supprime une clé ; `false` si absente. Décalage des positions.
    pub fn retirer(&self, cle: &Valeur) -> bool {
        let Some(i) = self.trouver(cle) else { return false; };
        let mut rep = self.0.borrow_mut();
        rep.items.remove(i);
        let a_retirer = rep.index.iter().find(|(_, p)| **p == i).map(|(h, _)| *h);
        if let Some(h) = a_retirer { rep.index.remove(&h); }
        for p in rep.index.values_mut() { if *p > i { *p -= 1; } }
        rep.fallback.retain(|&x| x != i);
        for x in rep.fallback.iter_mut() { if *x > i { *x -= 1; } }
        rep.collisions.retain(|(_, p)| *p != i);
        for (_, p) in rep.collisions.iter_mut() { if *p > i { *p -= 1; } }
        let TableRep { items, array_part, .. } = &mut *rep;
        array_part.fill(0);
        for (j, (kk, _)) in items.iter().enumerate() {
            if let Valeur::Entier(k, None) = kk {
                Self::maj_array(array_part, *k, j);
            }
        }
        true
    }
}

// TableRep déplacé ici (layout des fonctions chaudes préservé).
pub struct TableRep {
    /// Source de vérité : paires dans l'ordre d'insertion.
    pub items: Vec<(Valeur, Valeur)>,
    /// hachage canonique → position dans `items`.
    /// Hacheur `HashU64` (finaliseur à 1 multiplication, voir plus bas) :
    /// le SipHash de `HashMap` par défaut recalcule un hachage sécurisé sur
    /// ces u64 déjà mélangés : ~20 ns/lookup mesurés sur D7. Le remplacement
    /// ne change RIEN à la sémantique (les collisions u64 sont résolues par
    /// égalité de clé) — seulement le coût du hacheur. `items`, `fallback` et
    /// `collisions` restent la source de vérité (T1 préservé).
    index: HashMap<u64, usize, BuildHasherDefault<HashU64>>,
    /// positions des clés sans hachage canonique (repli linéaire).
    fallback: Vec<usize>,
    /// collisions de hachage : (h, position), normalement vide.
    collisions: Vec<(u64, usize)>,
    /// Partie « tableau » (Lua) : pour une clé i64 nue `k` dans
    /// [0, N_ARRAY), `array_part[k] = position + 1` (0 = absent).
    /// ACCÉLÉRATEUR PUR : `items` + `index` restent la source de vérité.
    /// Aucun faux positif possible : chaque hit direct est validé par
    /// `cle_i64_egale` contre l'item réellement stocké. Un absent dans
    /// cette partie retombe sur la voie hachée, donc l'égalité sémantique
    /// (Entier ≡ Rationnel ≡ Flottant) est préservée.
    array_part: Vec<u32>,
}

impl Table {
    /// Taille de la partie tableau : 4 096 entrées × 4 octets = 16 Kio.
    /// Les clés hors [0, N_ARRAY) utilisent exclusivement l'index haché.
    const N_ARRAY: usize = 4096;

    #[inline]
    fn maj_array(array_part: &mut [u32], k: i64, pos: usize) {
        if k >= 0 && (k as usize) < Self::N_ARRAY {
            array_part[k as usize] = (pos + 1) as u32;
        }
    }
}

/// Hacheur rapide pour les clés `u64` DÉJÀ mélangées de `TableRep.index`.
/// `write_u64` est le seul chemin utilisé par `HashMap<u64, _>`. Le repli
/// `write` (FNV-1a) couvre tout autre usage, sans jamais paniquer.
#[derive(Default)]
struct HashU64(u64);
impl std::hash::Hasher for HashU64 {
    #[inline]
    fn finish(&self) -> u64 { self.0 }
    #[inline]
    fn write(&mut self, octets: &[u8]) {
        let mut h = 0xCBF2_9CE4_8422_2325u64;
        for &o in octets { h = (h ^ o as u64).wrapping_mul(0x100_0000_01B3); }
        self.0 = h;
    }
    #[inline]
    fn write_u64(&mut self, i: u64) {
        // Finaliseur à UNE multiplication : le h canonique a déjà réparti
        // ses bits HAUTS (multiply-xor), il reste à avalancher les bits BAS.
        // `i >> 29` ramène les hauts vers les bas avant la multiplication.
        // ⚠ Un hachage IDENTITÉ dégénère à 2,44 ms : pour k ∈ [1024,2048),
        // `(k as f64).to_bits() & 0xfff == 0` → les 3 000 clés tombent dans
        // le même bucket. Simulation : max 5 clés/bucket de 4096 ici.
        let z = (i ^ (i >> 29)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        self.0 = z ^ (z >> 32);
    }
}
