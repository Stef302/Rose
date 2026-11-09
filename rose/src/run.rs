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
        (n.fdiv_u64(0xFFFF_FFFF) << 32) | n.fdiv_u64(0x1_0000_0000)
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
    let chiffres = chiffres.get_i64() as u64;
    let s = chiffres + 40;
    let dix_s = Gz::neuf();
    unsafe { gmp_pow_ui(dix_s.p(), Gz::de_u64(10).p(), s); }
    let mut cibles = Vec::new();
    for &x in &[5u64, 239u64] {
        let dest = Gz::neuf();
        let x2 = Gz::neuf();
        let p = Gz::neuf();
        let terme = Gz::neuf();
        unsafe {
            gmp_set_ui(x2.p(), x);
            gmp_mul(x2.p(), x2.p(), x2.p());
            gmp_set_ui(p.p(), x);
            gmp_fdiv_q(terme.p(), dix_s.p(), p.p());
            gmp_set(dest.p(), terme.p());
        }
        let mut k: u64 = 1;
        let mut signe = -1i32;
        loop {
            unsafe {
                gmp_mul(p.p(), p.p(), x2.p());
                k += 2;
                gmp_mul_ui(terme.p(), p.p(), k);
                gmp_fdiv_q(terme.p(), dix_s.p(), terme.p());
                if gmp_cmp(terme.p(), Gz::de_i64(0).p()) == 0 { break; }
                if signe < 0 { gmp_sub(dest.p(), dest.p(), terme.p()); }
                else { gmp_add(dest.p(), dest.p(), terme.p()); }
            }
            signe = -signe;
        }
        cibles.push(dest);
    }
    let at5 = Gz::neuf();
    let at239 = Gz::neuf();
    let pi = Gz::neuf();
    unsafe {
        gmp_set(at5.p(), cibles[0].p());
        gmp_set(at239.p(), cibles[1].p());
        gmp_mul_ui(at5.p(), at5.p(), 16);
        gmp_mul_ui(at239.p(), at239.p(), 4);
        gmp_sub(pi.p(), at5.p(), at239.p());
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

pub fn nombre_phi(chiffres: &Gz) -> String {
    if chiffres.sgn() <= 0 || !chiffres.fits_i64() || chiffres.get_i64() as u64 > 2_000_000 {
        erreur_runtime("décimales de φ hors limites (1..2000000)");
    }
    let n = chiffres.get_i64() as u64;
    // sqrt(5) a n+10 decimales via Newton
    let prec = n + 10;
    let scale = {
        let z = Gz::neuf();
        unsafe { gmp_pow_ui(z.p(), Gz::de_u64(10).p(), prec); }
        z
    };
    // x = 2 * scale (approximation initiale)
    let mut x = Gz::neuf();
    unsafe { gmp_mul_ui(x.p(), scale.p(), 2); }
    let five_scale = Gz::neuf();
    unsafe {
        gmp_set_ui(five_scale.p(), 5);
        gmp_mul(five_scale.p(), five_scale.p(), scale.p());
        gmp_mul(five_scale.p(), five_scale.p(), scale.p());
    }
    // Newton: x = (x + 5*scale*scale / x) / 2, 50 iterations
    for _ in 0..50 {
        let q = Gz::neuf();
        unsafe {
            gmp_fdiv_q(q.p(), five_scale.p(), x.p());
            gmp_add(x.p(), x.p(), q.p());
            let two = Gz::de_u64(2);
            gmp_fdiv_q(x.p(), x.p(), two.p());
        }
    }
    // phi = (1*scale + sqrt5) / 2  (avec scale pour les decimales)
    // sqrt5 = x / scale ; phi = (1 + sqrt5)/2 = (scale + x) / (2*scale)
    let num = Gz::neuf();
    let den = Gz::neuf();
    unsafe {
        gmp_add(num.p(), scale.p(), x.p());
        gmp_mul_ui(den.p(), scale.p(), 2);
    }
    // division entiere : num/den avec n decimales
    let pi_scale = Gz::neuf();
    unsafe {
        gmp_mul(pi_scale.p(), num.p(), scale.p());
        gmp_fdiv_q(pi_scale.p(), pi_scale.p(), den.p());
    }
    let brut = pi_scale.chaine();
    let brut: Vec<u8> = brut.into_bytes();
    let lbrut = brut.len();
    let mut sortie: Vec<u8> = vec![b' '; (n + 2) as usize];
    sortie[0] = b'1';
    sortie[1] = b'.';
    for i in 0..n {
        sortie[(2 + i) as usize] = if (i + 1) < lbrut as u64 { brut[(i + 1) as usize] } else { b'0' };
    }
    if (1 + n) < lbrut as u64 && brut[(1 + n) as usize] >= b'5' {
        let mut i = n as i64 + 1;
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
