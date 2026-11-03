// Rose 2.0 — moteur d'exécution : valeurs exactes et bibliothèque standard.
// Sémantique miroir du moteur C++ (moteur/cpp/src/rose.cpp) : conversions,
// comparaisons, rendus, RNG splitmix64+Lemire, messages d'erreur.
// Les entiers dépassant i64 et les rationnels passent par GMP via FFI.
// Licence MIT.

use std::cell::RefCell;
use std::ffi::CString;
use std::io::Write;
use std::rc::Rc;

use crate::front::{texte_flottant, TyKind};

// ── GMP par FFI ───────────────────────────────────────────────────────────

// dispositions GMP exactes (mpz_t = {alloc, size, d} sur aarch64/x86-64 LP64).
// #[repr(C)] garantit la disposition ; la « non FFI-safe » signalée par rustc
// est un faux positif : ces structs sont la disposition que GMP attend.
#[allow(private_interfaces)]
#[repr(C)]
pub struct Mpz { alloc: i32, size: i32, d: *mut u64 }
#[repr(C)]
pub struct Mpq { pub num: Mpz, pub den: Mpz }

#[allow(non_snake_case)]
unsafe extern "C" {
    fn __gmpz_init(x: *mut Mpz);
    fn __gmpz_clear(x: *mut Mpz);
    fn __gmpz_set_ui(x: *mut Mpz, u: u64);
    fn __gmpz_set_si(x: *mut Mpz, i: i64);
    fn __gmpz_set(x: *mut Mpz, y: *const Mpz);
    fn __gmpz_set_str(x: *mut Mpz, s: *const i8, base: i32) -> i32;
    fn __gmpz_get_str(buf: *mut i8, base: i32, x: *const Mpz) -> *mut i8;
    fn __gmpz_sizeinbase(x: *const Mpz, base: i32) -> u64;
    
    fn __gmpz_cmp(x: *const Mpz, y: *const Mpz) -> i32;
    fn __gmpz_cmp_ui(x: *const Mpz, u: u64) -> i32;
    fn __gmpz_fits_slong_p(x: *const Mpz) -> i32;
    fn __gmpz_get_si(x: *const Mpz) -> i64;
    fn __gmpz_get_ui(x: *const Mpz) -> u64;
    fn __gmpz_get_d(x: *const Mpz) -> f64;
    fn __gmpz_set_d(x: *mut Mpz, d: f64);
    fn __gmpz_add(r: *mut Mpz, a: *const Mpz, b: *const Mpz);
    fn __gmpz_sub(r: *mut Mpz, a: *const Mpz, b: *const Mpz);
    fn __gmpz_mul(r: *mut Mpz, a: *const Mpz, b: *const Mpz);
    fn __gmpz_mul_ui(r: *mut Mpz, a: *const Mpz, u: u64);
    fn __gmpz_mul_si(r: *mut Mpz, a: *const Mpz, b: i64);
    fn __gmpz_add_ui(r: *mut Mpz, a: *const Mpz, u: u64);
    fn __gmpz_fdiv_q(r: *mut Mpz, a: *const Mpz, b: *const Mpz);
    fn __gmpz_fdiv_r(r: *mut Mpz, a: *const Mpz, b: *const Mpz);
    fn __gmpz_mul_2exp(r: *mut Mpz, a: *const Mpz, e: u64);
    fn __gmpz_tstbit(x: *const Mpz, bit: u64) -> i32;
    fn __gmpz_pow_ui(r: *mut Mpz, a: *const Mpz, e: u64);
    fn __gmpz_ui_pow_ui(r: *mut Mpz, base: u64, e: u64);
    fn __gmpz_gcd(r: *mut Mpz, a: *const Mpz, b: *const Mpz);
    fn __gmpz_lcm(r: *mut Mpz, a: *const Mpz, b: *const Mpz);
    fn __gmpz_probab_prime_p(x: *const Mpz, reps: i32) -> i32;
    fn __gmpz_fdiv_ui(x: *const Mpz, u: u64) -> u64;
    fn __gmpq_init(x: *mut Mpq);
    fn __gmpq_clear(x: *mut Mpq);
    fn __gmpq_set(q: *mut Mpq, y: *const Mpq);
    fn __gmpq_set_z(q: *mut Mpq, z: *const Mpz);
    fn __gmpq_set_ui(q: *mut Mpq, n: u64, d: u64);
    fn __gmpq_set_d(q: *mut Mpq, d: f64);
    fn __gmpq_canonicalize(q: *mut Mpq);
    fn __gmpq_cmp(a: *const Mpq, b: *const Mpq) -> i32;
    fn __gmpq_get_d(q: *const Mpq) -> f64;
    fn __gmpq_add(r: *mut Mpq, a: *const Mpq, b: *const Mpq);
    fn __gmpq_sub(r: *mut Mpq, a: *const Mpq, b: *const Mpq);
    fn __gmpq_mul(r: *mut Mpq, a: *const Mpq, b: *const Mpq);
    fn __gmpq_div(r: *mut Mpq, a: *const Mpq, b: *const Mpq);
    fn free(p: *mut std::ffi::c_void);
}

pub struct Gz(*mut Mpz);

thread_local! { static ZERO: Gz = Gz::neuf(); }
fn zero_ptr() -> *const Mpz { ZERO.with(|z| z.0 as *const Mpz) }
pub struct Gq(*mut Mpq);

impl Gz {
    pub fn neuf() -> Gz {
        let p = Box::into_raw(Box::new(Mpz { alloc: 0, size: 0, d: std::ptr::null_mut() }));
        unsafe { __gmpz_init(p) };
        Gz(p)
    }
    pub fn p(&self) -> *mut Mpz { self.0 }
    pub fn poser_i64(&self, v: i64) { unsafe { __gmpz_set_si(self.p(), v) } }
    pub fn poser_u64(&self, v: u64) { unsafe { __gmpz_set_ui(self.p(), v) } }
    pub fn copier_de(&self, o: &Gz) { unsafe { __gmpz_set(self.p(), o.p()) } }
    pub fn chaine(&self) -> String {
        unsafe {
            let raw = __gmpz_get_str(std::ptr::null_mut(), 10, self.p());
            let s = std::ffi::CStr::from_ptr(raw as *const std::ffi::c_char).to_string_lossy().into_owned();
            free(raw as *mut std::ffi::c_void);
            s
        }
    }
    pub fn depuis_chaine(&self, s: &str) -> bool {
        match CString::new(s) {
            Ok(c) => unsafe { __gmpz_set_str(self.p(), c.as_ptr() as *const i8, 10) == 0 },
            Err(_) => false,
        }
    }
    pub fn sgn(&self) -> i32 {
        // __gmpz_sgn n'est pas exporté par toutes les libgmp ; cmp_ui(0) suffit
        // pour les tests de signe non négatifs ; pour le signe exact on passe par
        // cmp avec zéro.
        let z = Gz::neuf();
        unsafe { __gmpz_cmp(self.p(), z.p()) }
    }
    pub fn fits_i64(&self) -> bool { unsafe { __gmpz_fits_slong_p(self.p()) != 0 } }
    pub fn get_i64(&self) -> i64 { unsafe { __gmpz_get_si(self.p()) } }
}
impl Drop for Gz {
    fn drop(&mut self) {
        unsafe { __gmpz_clear(self.0); let _ = Box::from_raw(self.0); }
    }
}

impl Gq {
    pub fn neuf() -> Gq {
        let p = Box::into_raw(Box::new(Mpq {
            num: Mpz { alloc: 0, size: 0, d: std::ptr::null_mut() },
            den: Mpz { alloc: 0, size: 0, d: std::ptr::null_mut() },
        }));
        unsafe { __gmpq_init(p) };
        Gq(p)
    }
    pub fn p(&self) -> *mut Mpq { self.0 }
    pub fn num(&self) -> *mut Mpz { unsafe { &mut (*self.p()).num as *mut Mpz } }
    pub fn den(&self) -> *mut Mpz { unsafe { &mut (*self.p()).den as *mut Mpz } }
    pub fn copier_de(&self, o: &Gq) { unsafe { __gmpq_set(self.p(), o.p()) } }
}
impl Drop for Gq {
    fn drop(&mut self) {
        unsafe { __gmpq_clear(self.0); let _ = Box::from_raw(self.0); }
    }
}

// ── Valeur ────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct Liste(pub Rc<RefCell<Vec<Valeur>>>);
#[derive(Clone)]
pub struct Table(pub Rc<RefCell<Vec<(Valeur, Valeur)>>>);
#[derive(Clone)]
pub struct Tampon(pub Rc<RefCell<Vec<u8>>>);

#[derive(Clone)]
pub enum Valeur {
    Vide,
    Entier(i64, Option<Rc<Gz>>),          // i64 quand ça tient, GMP au-delà
    Rationnel(Rc<Gq>),
    Flottant(f64),
    Booleen(bool),
    Chaine(Rc<String>),
    Liste(Liste),
    Table(Table),
    Tampon(Tampon),
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
        }
    }
}

// ── Sorties et erreur runtime (miroir erreur_runtime du C++) ──────────────

pub fn flush_out() { let _ = std::io::stdout().flush(); }

pub fn ecrire_out(s: &str) { let _ = std::io::stdout().write_all(s.as_bytes()); }

pub fn erreur_runtime(msg: &str) -> ! {
    flush_out();
    eprintln!("Rose : erreur — {}", msg);
    std::process::exit(1);
}

/// Erreur levée pendant le repli de constantes : le C++ la signale comme une
/// erreur du vérificateur, avec le préfixe de position [Ligne L, Colonne C]
/// (les expressions repliées ne portent pas de position : 1,0, comme le C++
/// qui replie à la racine avec ligne=1 par défaut).
pub fn erreur_runtime_ligne(msg: &str) -> ! {
    flush_out();
    eprintln!("Rose : erreur — [Ligne {}, Colonne 0] {}", ligne_programme(), msg);
    std::process::exit(1);
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
    match v {
        Valeur::Entier(i, None) => z.poser_i64(*i),
        Valeur::Entier(_, Some(o)) => z.copier_de(o),
        Valeur::Booleen(b) => z.poser_i64(if *b { 1 } else { 0 }),
        Valeur::Rationnel(q) => unsafe { __gmpz_fdiv_q(z.p(), q.num(), q.den()) },
        Valeur::Flottant(d) => unsafe { __gmpz_set_d(z.p(), *d) },
        _ => erreur_runtime(&format!("conversion vers entier impossible (type {:?})", v.ty())),
    }
    z
}

pub fn vers_q(v: &Valeur) -> Gq {
    let q = Gq::neuf();
    match v {
        Valeur::Entier(i, None) => { let z = Gz::neuf(); z.poser_i64(*i); unsafe { __gmpq_set_z(q.p(), z.p()) } }
        Valeur::Entier(_, Some(o)) => unsafe { __gmpq_set_z(q.p(), o.p()) },
        Valeur::Booleen(b) => unsafe { __gmpq_set_ui(q.p(), if *b { 1 } else { 0 }, 1) },
        Valeur::Rationnel(o) => q.copier_de(o),
        Valeur::Flottant(d) => {
            if d.is_nan() || d.is_infinite() {
                erreur_runtime("flottant infini/non-nombre sans rationnel correspondant");
            }
            unsafe { __gmpq_set_d(q.p(), *d) }
        }
        _ => erreur_runtime("conversion vers rationnel impossible"),
    }
    q
}

pub fn vers_d(v: &Valeur) -> f64 {
    match v {
        Valeur::Flottant(d) => *d,
        Valeur::Entier(i, None) => *i as f64,
        Valeur::Entier(_, Some(z)) => unsafe { __gmpz_get_d(z.p()) },
        Valeur::Rationnel(q) => unsafe { __gmpq_get_d(q.p()) },
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

pub fn chaine_vers_entier(ch: &str) -> Gz {
    let z = Gz::neuf();
    if z.depuis_chaine(ch) { return z; }
    if let Some(p) = ch.find('/') {
        let n = Gz::neuf();
        let d = Gz::neuf();
        if !n.depuis_chaine(&ch[..p]) || !d.depuis_chaine(&ch[p + 1..]) || d.sgn() == 0 {
            erreur_runtime("conversion chaine → entier impossible");
        }
        unsafe { __gmpz_fdiv_q(z.p(), n.p(), d.p()) };
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
        return unsafe { __gmpq_cmp(qa.p(), qb.p()) };
    }
    let za = vers_z(g);
    let zb = vers_z(d);
    unsafe { __gmpz_cmp(za.p(), zb.p()) }
}

pub fn valeur_egale(a: &Valeur, b: &Valeur) -> bool {
    match (a, b) {
        (Valeur::Tampon(x), Valeur::Tampon(y)) => *x.0.borrow() == *y.0.borrow(),
        (_, Valeur::Tampon(..)) | (Valeur::Tampon(..), _) => false,
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
    let numer = Gz::neuf();
    let denom = Gz::neuf();
    numer.copier_de_ptr(q.num());
    denom.copier_de_ptr(q.den());
    if unsafe { __gmpz_cmp_ui(denom.p(), 1) } == 0 { numer.chaine() }
    else { format!("{}/{}", numer.chaine(), denom.chaine()) }
}

impl Gz {
    fn copier_de_ptr(&self, o: *const Mpz) { unsafe { __gmpz_set(self.p(), o) } }
}

/// ecri T_RATIONNEL : a/b, forme décimale périodique (1/3 → 0.[3]),
/// seuil 500 chiffres → a/b brut.
pub fn texte_rationnel(q: &Gq) -> String {
    let numer = Gz::neuf();
    let denom = Gz::neuf();
    numer.copier_de_ptr(q.num());
    denom.copier_de_ptr(q.den());
    if unsafe { __gmpz_cmp_ui(denom.p(), 1) } == 0 { return numer.chaine(); }
    let gros = unsafe {
        __gmpz_sizeinbase(numer.p(), 10) + __gmpz_sizeinbase(denom.p(), 10) > 500
    };
    if gros { return format!("{}/{}", numer.chaine(), denom.chaine()); }

    let entier = Gz::neuf();
    let reste = Gz::neuf();
    unsafe {
        __gmpz_fdiv_q(entier.p(), numer.p(), denom.p());
        if __gmpz_cmp(numer.p(), zero_ptr()) < 0 {
            __gmpz_mul_si(reste.p(), numer.p(), -1);
        } else {
            __gmpz_set(reste.p(), numer.p());
        }
        __gmpz_fdiv_r(reste.p(), reste.p(), denom.p());
    }
    let mut digits: Vec<u8> = Vec::new();
    let mut restes: Vec<String> = Vec::new();
    let mut positions: Vec<usize> = Vec::new();
    let mut peri: i32 = -1;
    while unsafe { __gmpz_cmp(reste.p(), zero_ptr()) } != 0 && digits.len() < 200 {
        let rs = reste.chaine();
        if let Some(p) = restes.iter().position(|r| *r == rs) {
            peri = positions[p] as i32;
            break;
        }
        restes.push(rs);
        positions.push(digits.len());
        unsafe { __gmpz_mul_ui(reste.p(), reste.p(), 10) };
        let chiffre = Gz::neuf();
        unsafe {
            __gmpz_fdiv_q(chiffre.p(), reste.p(), denom.p());
            __gmpz_fdiv_r(reste.p(), reste.p(), denom.p());
        }
        digits.push(b'0' + unsafe { __gmpz_get_ui(chiffre.p()) } as u8);
    }
    let mut out = entier.chaine();
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
    }
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
        unsafe {
            (__gmpz_fdiv_ui(n.p(), 0xFFFFFFFF) as u64) << 32
                | __gmpz_fdiv_ui(n.p(), 0x100000000) as u64
        }
    };
    rng_tirer(etat);
}

pub fn tirage_systeme() -> u64 {
    let mut v = 0u64;
    // getrandom(2)
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
        unsafe {
            __gmpz_mul_2exp(r.p(), r.p(), 32);
            __gmpz_add_ui(r.p(), r.p(), (x & 0xFFFF_FFFF) as u64);
            __gmpz_fdiv_r(r.p(), r.p(), n.p());
        }
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
        unsafe { __gmpz_mul(r.p(), a.p(), b.p()) };
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
    let bits = unsafe { __gmpz_sizeinbase(nn.p(), 2) };
    let t1 = Gz::neuf();
    let t2 = Gz::neuf();
    let t3 = Gz::neuf();
    for i in (0..bits).rev() {
        unsafe {
            __gmpz_mul_ui(t1.p(), b.p(), 2);
            __gmpz_sub(t1.p(), t1.p(), a.p());
            __gmpz_mul(t1.p(), t1.p(), a.p());
            __gmpz_mul(t2.p(), a.p(), a.p());
            __gmpz_mul(t3.p(), b.p(), b.p());
            __gmpz_add(t2.p(), t2.p(), t3.p());
            if __gmpz_tstbit(nn.p(), i) != 0 {
                __gmpz_set(a.p(), t2.p());
                __gmpz_add(b.p(), t1.p(), t2.p());
            } else {
                __gmpz_set(a.p(), t1.p());
                __gmpz_set(b.p(), t2.p());
            }
        }
    }
    a
}

pub fn est_premier(n: &Gz) -> bool {
    unsafe { __gmpz_probab_prime_p(n.p(), 25) != 0 && __gmpz_cmp_ui(n.p(), 1) > 0 }
}

pub fn nb_chiffres(n: &Gz) -> u64 {
    if n.sgn() == 0 { return 1; }
    let t = Gz::neuf();
    unsafe {
        if __gmpz_cmp(n.p(), zero_ptr()) < 0 { __gmpz_mul_si(t.p(), n.p(), -1) } else { __gmpz_set(t.p(), n.p()) }
    }
    unsafe { __gmpz_sizeinbase(t.p(), 10) }
}

pub fn compter_premiers(nn: &Gz) -> u64 {
    if nn.sgn() < 0 || !nn.fits_i64() || nn.get_i64() as u64 > 0xFFFF_FFFF {
        erreur_runtime("crible trop grand");
    }
    let n = nn.get_i64() as u64;
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
    if chiffres.sgn() <= 0 || !chiffres.fits_i64() || chiffres.get_i64() as u64 > 2_000_000 {
        erreur_runtime("décimales de π hors limites (1..2000000)");
    }
    let chiffres = chiffres.get_i64() as u64;
    let s = chiffres + 40;
    let dix_s = Gz::neuf();
    unsafe { __gmpz_ui_pow_ui(dix_s.p(), 10, s) };
    let mut cibles = Vec::new();
    for &x in &[5u64, 239u64] {
        let dest = Gz::neuf();
        let x2 = Gz::neuf();
        let p = Gz::neuf();
        let terme = Gz::neuf();
        unsafe {
            __gmpz_set_ui(x2.p(), x);
            __gmpz_mul(x2.p(), x2.p(), x2.p());
            __gmpz_set_ui(p.p(), x);
            __gmpz_fdiv_q(terme.p(), dix_s.p(), p.p());
            __gmpz_set(dest.p(), terme.p());
        }
        let mut k: u64 = 1;
        let mut signe = -1i32;
        loop {
            unsafe {
                __gmpz_mul(p.p(), p.p(), x2.p());
                k += 2;
                __gmpz_mul_ui(terme.p(), p.p(), k);
                __gmpz_fdiv_q(terme.p(), dix_s.p(), terme.p());
                if __gmpz_cmp(terme.p(), zero_ptr()) == 0 { break; }
                if signe < 0 { __gmpz_sub(dest.p(), dest.p(), terme.p()); }
                else { __gmpz_add(dest.p(), dest.p(), terme.p()); }
            }
            signe = -signe;
        }
        cibles.push(dest);
    }
    let at5 = Gz::neuf();
    let at239 = Gz::neuf();
    let pi = Gz::neuf();
    unsafe {
        __gmpz_set(at5.p(), cibles[0].p());
        __gmpz_set(at239.p(), cibles[1].p());
        __gmpz_mul_ui(at5.p(), at5.p(), 16);
        __gmpz_mul_ui(at239.p(), at239.p(), 4);
        __gmpz_sub(pi.p(), at5.p(), at239.p());
    }
    let brut = pi.chaine();
    let brut: Vec<u8> = brut.into_bytes();
    let lbrut = brut.len();
    let mut sortie: Vec<u8> = vec![b' '; (chiffres + 2) as usize];
    sortie[0] = if brut[0] == b'-' { brut[0] } else { brut[0] };
    sortie[1] = b'.';
    for i in 0..chiffres {
        sortie[(2 + i) as usize] = if (i + 1) < lbrut as u64 { brut[(i + 1) as usize] } else { b'0' };
    }
    if (1 + chiffres) < lbrut as u64 && brut[(1 + chiffres) as usize] >= b'5' {
        let mut i = chiffres as i64 + 1;
        while i >= 0 {
            if sortie[i as usize] == b'.' { i -= 1; continue; }
            sortie[i as usize] += 1;
            if sortie[i as usize] <= b'9' { break; }
            sortie[i as usize] = b'0';
            i -= 1;
        }
    }
    String::from_utf8_lossy(&sortie).into_owned()
}

// Remarque : les champs num/den d'une Mpq sont des __mpz_struct contigus ;
// on y accède par les macros ci-dessous, jamais par « .alloc ».
macro_rules! q_num {
    ($q:expr) => { &mut (*($q as *mut Mpq)).num as *mut Mpz };
}
macro_rules! q_den {
    ($q:expr) => { &mut (*($q as *mut Mpq)).den as *mut Mpz };
}

pub fn pow_rationnel_sur(base: &Gq, e: i64) -> Gq {
    let r = Gq::neuf();
    let zi = Gz::neuf();
    let den = Gz::neuf();
    unsafe {
        __gmpz_set(zi.p(), q_num!(base.p()));
        __gmpz_pow_ui(zi.p(), zi.p(), e.unsigned_abs());
        __gmpz_set(den.p(), q_den!(base.p()));
        __gmpz_pow_ui(den.p(), den.p(), e.unsigned_abs());
        if e >= 0 {
            __gmpz_set(q_num!(r.p()), zi.p());
            __gmpz_set(q_den!(r.p()), den.p());
        } else {
            __gmpz_set(q_num!(r.p()), den.p());
            __gmpz_set(q_den!(r.p()), zi.p());
        }
        __gmpq_canonicalize(r.p());
    }
    r
}

pub fn pgcd(a: &Gz, b: &Gz) -> Gz {
    let r = Gz::neuf();
    unsafe { __gmpz_gcd(r.p(), a.p(), b.p()) };
    r
}
pub fn ppcm(a: &Gz, b: &Gz) -> Gz {
    let r = Gz::neuf();
    unsafe { __gmpz_lcm(r.p(), a.p(), b.p()) };
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
                unsafe { __gmpz_pow_ui(r.p(), z.p(), e as u64) };
                Valeur::zg(r)
            } else {
                if z.sgn() == 0 { erreur_runtime("0^exposant négatif"); }
                let q = Gq::neuf();
                unsafe { __gmpq_set_z(q.p(), z.p()) };
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
        let ctr = [b as u32, (b >> 32) as u32, 0, 0];
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

impl Gz {
    pub fn fits_u64(&self, max: u64) -> bool {
        unsafe { __gmpz_fits_ulong_p(self.p()) != 0 && __gmpz_get_ui(self.p()) < max }
    }
    pub fn fits_usize(&self, max: usize) -> bool { self.fits_u64(max as u64) }
    pub fn get_u64(&self) -> u64 { unsafe { __gmpz_get_ui(self.p()) } }
}

thread_local! { static ZERO_Q: Gq = Gq::neuf(); }
pub fn gmpq_sgn(q: *const Mpq) -> i32 {
    ZERO_Q.with(|z| unsafe { __gmpq_cmp(q, z.p() as *const Mpq) })
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

// ── Alias GMP lisibles (miroir des noms du moteur C++) ────────────────────
#[allow(non_snake_case)]
unsafe extern "C" {
    fn __gmpz_neg(r: *mut Mpz, a: *const Mpz);
    fn __gmpq_neg(r: *mut Mpq, a: *const Mpq);
    fn __gmpz_fits_ulong_p(x: *const Mpz) -> i32;
}

pub unsafe fn gmp_add(r: *mut Mpz, a: *const Mpz, b: *const Mpz) { __gmpz_add(r, a, b) }
pub unsafe fn gmp_sub(r: *mut Mpz, a: *const Mpz, b: *const Mpz) { __gmpz_sub(r, a, b) }
pub unsafe fn gmp_mul(r: *mut Mpz, a: *const Mpz, b: *const Mpz) { __gmpz_mul(r, a, b) }
pub unsafe fn gmp_neg(r: *mut Mpz, a: *const Mpz) { __gmpz_neg(r, a) }
pub unsafe fn gmp_set(r: *mut Mpz, a: *const Mpz) { __gmpz_set(r, a) }
pub unsafe fn gmp_set_d(r: *mut Mpz, a: f64) { __gmpz_set_d(r, a) }
pub unsafe fn gmp_sgn(a: *const Mpz) -> i32 {
    let z = Gz::neuf();
    __gmpz_cmp(a, z.p())
}
pub unsafe fn gmp_add_ui(r: *mut Mpz, a: *const Mpz, u: u64) { __gmpz_add_ui(r, a, u) }
pub unsafe fn gmp_fdiv_q(r: *mut Mpz, a: *const Mpz, b: *const Mpz) { __gmpz_fdiv_q(r, a, b) }
pub unsafe fn gmp_fdiv_r(r: *mut Mpz, a: *const Mpz, b: *const Mpz) { __gmpz_fdiv_r(r, a, b) }
pub unsafe fn gmp_fdiv_ui(a: *const Mpz, u: u64) -> u64 { __gmpz_fdiv_ui(a, u) }
pub unsafe fn gmp_mul_2exp(r: *mut Mpz, a: *const Mpz, e: u64) { __gmpz_mul_2exp(r, a, e) }
pub unsafe fn gmp_pow_ui(r: *mut Mpz, a: *const Mpz, e: u64) { __gmpz_pow_ui(r, a, e) }
pub unsafe fn gmpq_add(r: *mut Mpq, a: *const Mpq, b: *const Mpq) { __gmpq_add(r, a, b) }
pub unsafe fn gmpq_sub(r: *mut Mpq, a: *const Mpq, b: *const Mpq) { __gmpq_sub(r, a, b) }
pub unsafe fn gmpq_mul(r: *mut Mpq, a: *const Mpq, b: *const Mpq) { __gmpq_mul(r, a, b) }
pub unsafe fn gmpq_div(r: *mut Mpq, a: *const Mpq, b: *const Mpq) { __gmpq_div(r, a, b) }
pub unsafe fn gmpq_neg(r: *mut Mpq, a: *const Mpq) { __gmpq_neg(r, a) }
pub unsafe fn gmpq_set(r: *mut Mpq, a: *const Mpq) { __gmpq_set(r, a) }
pub unsafe fn gmpq_set_z_field(dst: *mut Mpz, src: *const Mpz) { __gmpz_set(dst, src) }
pub unsafe fn gmpq_canonicalize(q: *mut Mpq) { __gmpq_canonicalize(q) }

pub fn gmp_cmp_ge(a: &Gz, b: &Gz) -> bool { unsafe { __gmpz_cmp(a.p(), b.p()) >= 0 } }

impl Gq {
    #[allow(dead_code)]
    pub fn num_mut(&self) -> *mut Mpz { self.num() }
    #[allow(dead_code)]
    pub fn den_mut(&self) -> *mut Mpz { self.den() }
    pub fn num_raw(&self) -> *const Mpz { self.num() }
    pub fn den_raw(&self) -> *const Mpz { self.den() }
}
