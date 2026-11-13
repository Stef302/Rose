// Rose 2.0 — moteur d'exécution : valeurs exactes et bibliothèque standard.
// Sémantique miroir du moteur C++ (moteur/cpp/src/rose.cpp) : conversions,
// comparaisons, rendus, RNG splitmix64+Lemire, messages d'erreur.
// ZÉRO DÉPENDANCE : les entiers dépassant i64 et les rationnels vivent sur
// l'arithmétique exacte native i128 (src/i128x.rs) — plus aucun GMP, plus
// aucune liaison externe. Un dépassement 128 bits est une erreur runtime
// nette (« entier hors plage »), jamais un wrap silencieux.
// Licence MIT.

use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;

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
#[derive(Clone)]
pub struct Table(pub Rc<RefCell<Vec<(Valeur, Valeur)>>>);
#[derive(Clone)]
pub struct Tampon(pub Rc<RefCell<Vec<u8>>>);
#[derive(Clone)]
pub struct Tableau(pub Rc<RefCell<Vec<i64>>>);

#[derive(Clone)]
pub enum Valeur {
    Vide,
    Entier(i64, Option<Rc<Gz>>),          // i64 quand ça tient, i128 au-delà
    Rationnel(Rc<Gq>),
    Flottant(f64),
    Booleen(bool),
    Chaine(Rc<String>),
    Liste(Liste),
    Table(Table),
    Tampon(Tampon),
    Tableau(Tableau),
    Complexe { re: f64, im: f64 },
    Irrationnel(Rc<Gz>),
}

impl Valeur {
    pub fn z(v: i64) -> Valeur { Valeur::Entier(v, None) }
    pub fn zg(z: Gz) -> Valeur {
        if z.fits_i64() { Valeur::z(z.get_i64()) } else { Valeur::Entier(0, Some(Rc::new(z))) }
    }
    pub fn qq(q: Gq) -> Valeur { Valeur::Rationnel(Rc::new(q)) }
    pub fn f(v: f64) -> Valeur { Valeur::Flottant(v) }
    pub fn b(v: bool) -> Valeur { Valeur::Booleen(v) }
    pub fn s(v: impl Into<String>) -> Valeur { Valeur::Chaine(Rc::new(v.into())) }
    #[allow(dead_code)]
    pub fn liste() -> Valeur { Valeur::Liste(Liste(Rc::new(RefCell::new(Vec::new())))) }
    pub fn liste_de(v: Vec<Valeur>) -> Valeur { Valeur::Liste(Liste(Rc::new(RefCell::new(v)))) }
    pub fn table() -> Valeur { Valeur::Table(Table(Rc::new(RefCell::new(Vec::new())))) }
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

// ── Sorties et erreur runtime (miroir erreur_runtime du C++) ──────────────

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

/// Erreur levée pendant le repli de constantes : le C++ la signale comme une
/// erreur du vérificateur, avec le préfixe de position [Ligne L, Colonne C]
/// (les expressions repliées ne portent pas de position : 1,0, comme le C++
/// qui replie à la racine avec ligne=1 par défaut).
pub fn erreur_runtime_ligne(msg: &str) -> ! {
    flush_out();
    panic!("ROSE_ERR:[Ligne {}, Colonne 0] {}", ligne_programme(), msg);
}

/// Erreur levée pendant le vérificateur / le repli de constantes : le C++
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
/// erreurs de vérification/repli, là où le C++ connaît e->ligne).
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

pub fn vers_z(v: &Valeur) -> Gz {
    let z = Gz::neuf();
    let ligne = LIGNE_COURANTE.with(|c| c.get());
    match v {
        Valeur::Entier(i, None) => z.poser_i64(*i),
        Valeur::Entier(_, Some(o)) => z.copier_de(o),
        Valeur::Booleen(b) => z.poser_i64(if *b { 1 } else { 0 }),
        Valeur::Rationnel(q) => { let n = q.num_i128(); let d = q.den_i128(); if d == 0 { erreur_runtime("division par zéro"); } z.poser_i64((n / d) as i64); }
        Valeur::Flottant(d) => z.poser_d(*d),
        Valeur::Chaine(s) => {
            if s.is_empty() { z.poser_i64(0); }
            else if !z.depuis_chaine(s) { erreur_runtime(&format!("conversion vers entier impossible (chaine: {:?})", s)); }
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

pub fn vers_d(v: &Valeur) -> f64 {
    match v {
        Valeur::Flottant(d) => *d,
        Valeur::Entier(i, None) => *i as f64,
        Valeur::Entier(_, Some(z)) => z.get_d(),
        Valeur::Rationnel(q) => q.get_d(),
        Valeur::Booleen(b) => if *b { 1.0 } else { 0.0 },
        // le C++ convertit une chaine en flottant via chaine_vers_flottant dans
        // les conversions ; miroir : 'flottant' depuis chaine est possible ici.
        _ => { let _ = &v; vers_d_chaine(&v) },
    }
}

fn vers_d_chaine(v: &Valeur) -> f64 {
    match v {
        Valeur::Chaine(s) => chaine_vers_flottant(s),
        _ => erreur_runtime("conversion vers flottant impossible"),
    }
}

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

pub fn cmp_vals(g: &Valeur, d: &Valeur) -> i32 {
    if let (Valeur::Chaine(a), Valeur::Chaine(b)) = (g, d) {
        return if a == b { 0 } else if a < b { -1 } else { 1 };
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

pub fn valeur_egale(a: &Valeur, b: &Valeur) -> bool {
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
            if x.len() != y.len() { return false; }
            x.iter().all(|(k, v)| y.iter().any(|(k2, v2)| valeur_egale(k, k2) && valeur_egale(v, v2)))
        }
        (_, Valeur::Table(..)) | (Valeur::Table(..), _) => false,
        (Valeur::Chaine(x), Valeur::Chaine(y)) => x == y,
        (Valeur::Chaine(..), _) | (_, Valeur::Chaine(..)) => false,
        (Valeur::Booleen(x), Valeur::Booleen(y)) => x == y,
        (Valeur::Booleen(..), _) | (_, Valeur::Booleen(..)) => false,
        _ => cmp_vals(a, b) == 0,
    }
}

// ── Rendus (miroir ecri / chaine_de / texte_flottant) ─────────────────────

pub fn q_chaine(q: &Gq) -> String {
    if q.den_i128() == 1 { q.num_i128().to_string() }
    else { format!("{}/{}", q.num_i128(), q.den_i128()) }
}

/// ecri T_RATIONNEL : a/b, forme décimale périodique (1/3 → 0.[3]),
/// seuil 500 chiffres → a/b brut.
pub fn texte_rationnel(q: &Gq) -> String {
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

pub fn chaine_de(v: &Valeur) -> String {
    // miroir C++ chaine_de() : a/b BRUT a tous les niveaux (les listes et
    // tables rendent leurs rationnels en « 12/5 »). La forme periodique est
    // reservee a afficher() (miroir ecri).
    chaine_de_p(v, false)
}

/// Miroir de chaine_de() du C++. « peri » = affichage direct (>> ) : le
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
        Valeur::Chaine(s) => s.to_string(),
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
            let items = t.0.borrow();
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

/// Affiche une valeur accompagnée de sa FORMULE EXACTE en LaTeX.
/// Miroir de la convention déjà en place pour π, φ et e :
///     π = 4\cdot\arctan(1/5) - \arctan(1/239)   π ≈ 3.14159…
/// On rend donc « formule   valeur », la formule étant produite par le
/// module `latex` à partir de l'AST du programme.
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
    // miroir ecri() du C++ : T_RATIONNEL et T_LISTE passent par la forme
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

/// hasard(n) : Lemire, miroir exact du C++ (-borné hors i64 inclus).
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
    // arbre, comme le C++
    fn arbre(i: u64, j: u64) -> Gz {
        if i > j { let r = Gz::neuf(); r.poser_u64(1); return r; }
        if i == j { let r = Gz::neuf(); r.poser_u64(i); return r; }
        let m = i + (j - i) / 2;
        let a = arbre(i, m);
        let b = arbre(m + 1, j);
        let r = Gz::neuf();
        r.mul(&a, &b);
        r
    }
    arbre(1, n)
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
    // Formule exacte (LaTeX)
    sortie.push_str("π = 4\\cdot\\arctan(1/5) - \\arctan(1/239)");
    sortie.push_str("   ");
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
    // Formule exacte (LaTeX)
    sortie.push_str("e = \\sum_{k=0}^{\\infty} \\frac{1}{k!}");
    sortie.push_str("   ");
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
    // Formule exacte (LaTeX)
    sortie.push_str("φ = \\frac{1 + \\sqrt{5}}{2}");
    sortie.push_str("   ");
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

            unsafe extern "C" {
                fn cuInit(flags: u32) -> CuResult;
                fn cuDeviceGet(device: *mut CuDevice, ordinal: i32) -> CuResult;
                fn cuCtxCreate_v2(ctx: *mut CuContext, flags: u32, dev: CuDevice) -> CuResult;
                fn cuModuleLoadData(module: *mut CuModule, image: *const i8) -> CuResult;
                fn cuModuleGetFunction(hfunc: *mut CuFunction, hmod: CuModule, name: *const i8) -> CuResult;
                fn cuMemAlloc(dptr: *mut CuDevicePtr, bytesize: usize) -> CuResult;
                fn cuMemcpyHtoD(dst: CuDevicePtr, src: *const i8, bytecount: usize) -> CuResult;
                fn cuMemsetD8(dst: CuDevicePtr, uc: u8, n: usize) -> CuResult;
                fn cuLaunchKernel(f: CuFunction, gridDimX: u32, gridDimY: u32, gridDimZ: u32,
                                  blockDimX: u32, blockDimY: u32, blockDimZ: u32,
                                  sharedMemBytes: usize, hStream: CuStream,
                                  kernelParams: *mut *mut std::ffi::c_void, extra: *mut *mut std::ffi::c_void) -> CuResult;
                fn cuMemcpyDtoH(dst: *mut i8, src: CuDevicePtr, bytecount: usize) -> CuResult;
                fn cuMemFree(dptr: CuDevicePtr) -> CuResult;
                fn cuModuleUnload(hmod: CuModule) -> CuResult;
                fn cuCtxDestroy_v2(ctx: CuContext) -> CuResult;
            }

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
    // Miroir exact du C++ : un bloc Philox (4 mots de 32 bits) remplit 4 OCTETS,
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
                    // « 1/3 » accepté aussi (miroir C++ strtod puis fraction)
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
/// Miroir de `jit_erreur` du moteur C++ (rose-cpp/src/jit.cpp:388) : une
/// erreur levée dans du code JIT ne peut pas être rattrapée (une trame
/// Cranelift n'a pas d'information de défilement) — on affiche l'erreur et on
/// quitte, exactement comme le fait le JIT C++.
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
    if at == 0 {
        if a == i64::MIN {
            let mut r = Box::new(Mpz { v: BigInt::from(0) });
            r.v = -BigInt::from(a);
            unsafe { *out = Box::into_raw(r) as i64; *outt = 1; }
            return;
        }
        unsafe { *out = a.wrapping_neg(); *outt = 0; }
        return;
    }
    unsafe {
        let src = &*(a as *const Mpz);
        let mut r = Box::new(Mpz { v: -src.v.clone() });
        if r.v >= BigInt::from(i64::MIN) && r.v <= BigInt::from(i64::MAX) {
            *out = r.v.to_i64().unwrap_or(0);
            *outt = 0;
            let _ = Box::from_raw(Box::into_raw(r));
        } else {
            *out = Box::into_raw(r) as i64;
            *outt = 1;
        }
    }
}

#[derive(Clone, Copy)]
enum TaggedOp2 { Add, Sub, Mul }

fn to_bigint(av: i64, at: i64) -> BigInt {
    if at == 0 { BigInt::from(av) }
    else { unsafe { (*(av as *const Mpz)).v.clone() } }
}

fn tagged_binop_abi(a: i64, at: i64, b: i64, bt: i64, out: *mut i64, outt: *mut i64, op: TaggedOp2) {
    if at == 0 && bt == 0 {
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
        let r = Box::new(Mpz { v: br });
        unsafe { *out = Box::into_raw(r) as i64; *outt = 1; }
        return;
    }
    // In-place mutation quand un operande est deja un BigInt (tag 1) :
    // on reutilise sa boite, pas d'allocation de resultat.
    if at == 1 {
        let bb = to_bigint(b, bt);
        unsafe {
            let ptr = a as *mut Mpz;
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
    if bt == 1 {
        let aa = to_bigint(a, at);
        unsafe {
            let ptr = b as *mut Mpz;
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
    // les deux i64 avec debordement : promotion BigInt (allocation unique)
    let ba = BigInt::from(a);
    let bb = BigInt::from(b);
    let br = match op {
        TaggedOp2::Add => ba + bb,
        TaggedOp2::Sub => ba - bb,
        TaggedOp2::Mul => ba * bb,
    };
    if br >= BigInt::from(i64::MIN) && br <= BigInt::from(i64::MAX) {
        unsafe { *out = br.to_i64().unwrap_or(0); *outt = 0; }
        return;
    }
    let r = Box::new(Mpz { v: br });
    unsafe { *out = Box::into_raw(r) as i64; *outt = 1; }
}

#[unsafe(no_mangle)]
pub extern "C" fn rose_clone_ent(v: i64, t: i64, out: *mut i64, outt: *mut i64) {
    if t == 0 {
        unsafe { *out = v; *outt = 0; }
    } else {
        unsafe {
            let src = &*(v as *const Mpz);
            let r = Box::new(Mpz { v: src.v.clone() });
            *out = Box::into_raw(r) as i64;
            *outt = 1;
        }
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

    loop {
        // Calculer la fenêtre courante
        let mut idx = 0;
        let start = pos;
        let mut end = pos;
        let mut count = 0;
        for (i, c) in s.char_indices() {
            if i >= pos {
                count += 1;
                if count >= chars_per_page {
                    end = i + c.len_utf8();
                    break;
                }
            }
        }
        if count == 0 || pos >= total {
            end = total;
        } else if end <= pos {
            end = total;
        }

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
