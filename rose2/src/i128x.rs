// Rose 2.0 — arithmétique exacte SANS GMP, sans FFI, sans dépendance.
//
// Les entiers « hors i64 » vivent sur i128 (±2^127−1) ; les rationnels sur
// (i128 num, i128 den) réduit par pgcd. Aucune opération ne wrappe en
// silence : un dépassement i128 lève « entier hors plage » (message net),
// jamais un faux chiffre. La surface (Gz/Gq + gmp_*/gmpq_*) reproduit
// l'ancienne interface FFI pour que interp.rs / jit.rs restent inchangés.
// Licence MIT.

use std::cmp::Ordering;

use crate::run::erreur_runtime;

/// Disposition native (128 bits) d'un entier exact : le « mpz_t » du moteur
/// Rust — plus rien de GMP ici, juste un i128.
#[derive(Clone, Copy)]
pub struct Mpz { pub v: i128 }
/// Disposition native d'un rationnel exact : (numérateur, dénominateur) i128.
#[derive(Clone, Copy)]
pub struct Mpq { pub num: Mpz, pub den: Mpz }

pub fn hors_plage() -> ! {
    erreur_runtime("entier hors plage (±2^127−1) — l'arithmétique exacte du moteur est bornée 128 bits");
}

// variante pour unwrap_or_else (le divergent doit rester typable en i128)
#[allow(dead_code)]
fn hp128() -> i128 { hors_plage() }

#[inline]
unsafe fn vv(a: *const Mpz) -> i128 { (*a).v }
#[inline]
unsafe fn setv(r: *mut Mpz, x: i128) { (*r).v = x }
#[inline]
unsafe fn qt(a: *const Mpq) -> (i128, i128) { ((*a).num.v, (*a).den.v) }

// ── Gz : entier exact 128 bits (poignée boxée : les appelants tiennent des
//    Gz::neuf() et écrivent dedans via p(), comme sur l'ancienne surface) ────

pub struct Gz(pub *mut Mpz);

impl Gz {
    pub fn neuf() -> Gz { Gz(Box::into_raw(Box::new(Mpz { v: 0 }))) }
    pub fn p(&self) -> *mut Mpz { self.0 }
    pub fn de_i128(x: i128) -> Gz { Gz(Box::into_raw(Box::new(Mpz { v: x }))) }
    pub fn i128(&self) -> i128 { unsafe { (*self.0).v } }
    fn v(&self) -> i128 { unsafe { (*self.0).v } }
    fn set_v(&self, x: i128) { unsafe { (*self.0).v = x } }

    pub fn poser_i64(&self, x: i64) { self.set_v(x as i128); }
    pub fn poser_u64(&self, x: u64) { self.set_v(x as i128); }
    pub fn poser_d(&self, d: f64) {
        if !d.is_finite() { hors_plage(); }
        self.set_v(d as i128);
    }
    pub fn copier_de(&self, o: &Gz) { self.set_v(o.v()); }
    pub fn copier_de_ptr(&self, o: *const Mpz) { self.set_v(unsafe { (*o).v }); }
    pub fn sgn(&self) -> i32 { self.v().cmp(&0) as i32 }
    pub fn fits_i64(&self) -> bool {
        let x = self.v();
        x >= i64::MIN as i128 && x <= i64::MAX as i128
    }
    pub fn fits_u64(&self) -> bool {
        let x = self.v();
        x >= 0 && (x as u128) <= u64::MAX as u128
    }
    /// miroir de l'ancienne surface : borne exclusive (0 <= v < max)
    pub fn fits_u64_lt(&self, max: u64) -> bool {
        let x = self.v();
        x >= 0 && (x as u128) < max as u128
    }
    pub fn fits_u64_max(&self, max: u64) -> bool {
        let x = self.v();
        x >= 0 && (x as u64) < max
    }
    pub fn fits_usize(&self, max: usize) -> bool { self.fits_u64_max(max as u64) }
    pub fn get_i64(&self) -> i64 { self.v() as i64 }
    pub fn get_u64(&self) -> u64 { self.v() as u64 }
    pub fn get_d(&self) -> f64 { self.v() as f64 }
    pub fn chaine(&self) -> String { self.v().to_string() }
    pub fn depuis_chaine(&self, s: &str) -> bool {
        match s.trim().parse::<i128>() {
            Ok(x) => { self.set_v(x); true }
            Err(_) => false,
        }
    }
    pub fn sizeinbase10(&self) -> u64 {
        let x = self.v();
        if x == 0 { 1 } else { x.unsigned_abs().to_string().len() as u64 }
    }
    pub fn taille_bits(&self) -> u64 {
        let x = self.v().unsigned_abs();
        if x == 0 { 0 } else { 128 - x.leading_zeros() as u64 }
    }
    /// Mirrors mpz_fdiv_ui: floor-mod by a small divisor.
    pub fn fdiv_u64(&self, u: u64) -> u64 {
        if u == 0 { erreur_runtime("division par zéro"); }
        let u = u as i128;
        let r = self.v() % u;
        (if r < 0 { r + u } else { r }) as u64
    }
    /// Mirrors mpz_tstbit on the two's-complement magnitude.
    pub fn tstbit(&self, bit: u64) -> bool {
        let x = self.v();
        if x < 0 || bit >= 127 { return false; }
        ((x as u128) >> bit) & 1 == 1
    }
}
impl Drop for Gz {
    fn drop(&mut self) { unsafe { let _ = Box::from_raw(self.0); } }
}

// helpers d'arithmétique exacte (miroir des gmp_* usités)
impl Gz {
    pub fn add(&self, a: &Gz, b: &Gz) {
        match a.v().checked_add(b.v()) { Some(x) => self.set_v(x), None => { hors_plage(); } }
    }
    pub fn sub(&self, a: &Gz, b: &Gz) {
        match a.v().checked_sub(b.v()) { Some(x) => self.set_v(x), None => { hors_plage(); } }
    }
    pub fn mul(&self, a: &Gz, b: &Gz) {
        match a.v().checked_mul(b.v()) { Some(x) => self.set_v(x), None => { hors_plage(); } }
    }
    pub fn neg(&self, a: &Gz) {
        match a.v().checked_neg() { Some(x) => self.set_v(x), None => { hors_plage(); } }
    }
    pub fn add_u64(&self, a: &Gz, u: u64) {
        match a.v().checked_add(u as i128) { Some(x) => self.set_v(x), None => { hors_plage(); } }
    }
    pub fn mul_u64(&self, a: &Gz, u: u64) {
        match a.v().checked_mul(u as i128) { Some(x) => self.set_v(x), None => { hors_plage(); } }
    }
    pub fn mul_i64(&self, a: &Gz, u: i64) {
        match a.v().checked_mul(u as i128) { Some(x) => self.set_v(x), None => { hors_plage(); } }
    }
    pub fn mul_2exp(&self, a: &Gz, e: u64) {
        if e >= 128 { hors_plage(); }
        match a.v().checked_mul(1i128 << e) { Some(x) => self.set_v(x), None => { hors_plage(); } }
    }
    /// floor division (miroir mpz_fdiv_q, plancher vers −∞)
    pub fn fdiv_q(&self, a: &Gz, b: &Gz) {
        let (x, y) = (a.v(), b.v());
        if y == 0 { erreur_runtime("division par zéro"); }
        self.set_v(fdiv_i128(x, y).0);
    }
    /// floor modulo (miroir mpz_fdiv_r, signe du diviseur)
    pub fn fdiv_r(&self, a: &Gz, b: &Gz) {
        let (x, y) = (a.v(), b.v());
        if y == 0 { erreur_runtime("division par zéro"); }
        self.set_v(fdiv_i128(x, y).1);
    }
    pub fn pow_u64(&self, a: &Gz, e: u64) {
        let base = a.v();
        let mut acc: i128 = 1;
        for _ in 0..e {
            match acc.checked_mul(base) { Some(x) => acc = x, None => { hors_plage(); } }
        }
        self.set_v(acc);
    }
    pub fn gcd(&self, a: &Gz, b: &Gz) {
        self.set_v(gcd_i128(a.v(), b.v()) as i128);
    }
    pub fn lcm(&self, a: &Gz, b: &Gz) {
        let (x, y) = (a.v(), b.v());
        if x == 0 || y == 0 { self.set_v(0); return; }
        let g = gcd_i128(x, y) as i128;
        match (x / g).checked_mul(y.abs()) { Some(v) => self.set_v(v), None => { hors_plage(); } }
    }
    pub fn cmp_gz(&self, b: &Gz) -> Ordering { self.v().cmp(&b.v()) }
    /// Miller-Rabin déterministe (bornes du moteur : n < 2^127).
    pub fn est_premier(&self) -> bool {
        let n = self.v();
        if n < 2 { return false; }
        miller_rabin_u128(n as u128)
    }
}

fn fdiv_i128(x: i128, y: i128) -> (i128, i128) {
    let mut q = x / y;
    let mut r = x % y;
    if r != 0 && ((r < 0) != (y < 0)) { q -= 1; r += y; }
    (q, r)
}

fn gcd_i128(a: i128, b: i128) -> u128 {
    let mut a = a.unsigned_abs();
    let mut b = b.unsigned_abs();
    while b != 0 { let t = a % b; a = b; b = t; }
    a
}

fn miller_rabin_u128(n: u128) -> bool {
    if n < 2 { return false; }
    for p in [2u128,3,5,7,11,13,17,19,23,29,31,37] {
        if n == p { return true; }
        if n % p == 0 { return false; }
    }
    let mut d = n - 1;
    let mut r = 0u32;
    while d & 1 == 0 { d >>= 1; r += 1; }
    'a: for &a in [2u128,3,5,7,11,13,17,19,23,29,31,37].iter() {
        let mut x = pow_mod_u128(a, d, n);
        if x == 1 || x == n - 1 { continue; }
        for _ in 1..r {
            x = mul_mod_u128(x, x, n);
            if x == n - 1 { continue 'a; }
        }
        return false;
    }
    true
}

fn mul_mod_u128(a: u128, b: u128, m: u128) -> u128 {
    // multiplication modulaire sans dépassement par addition doublante
    // (m < 2^127 garanti par la portée i128 du moteur)
    let mut res: u128 = 0;
    let mut x = a % m;
    let mut y = b % m;
    while y > 0 {
        if y & 1 == 1 { res = add_mod_u128(res, x, m); }
        x = add_mod_u128(x, x, m);
        y >>= 1;
    }
    res
}
fn add_mod_u128(a: u128, b: u128, m: u128) -> u128 {
    // (a+b) mod m sans dépassement (a,b < m < 2^127)
    let s = a.wrapping_add(b);
    if s >= m || s < a { s.wrapping_sub(m) } else { s }
}
fn pow_mod_u128(mut a: u128, mut e: u128, m: u128) -> u128 {
    let mut acc: u128 = 1;
    a %= m;
    while e > 0 {
        if e & 1 == 1 { acc = mul_mod_u128(acc, a, m); }
        a = mul_mod_u128(a, a, m);
        e >>= 1;
    }
    acc
}

// ── Gq : rationnel exact (i128/i128 réduit, poignée boxée) ────────────────

pub struct Gq(pub *mut Mpq);

impl Gq {
    pub fn neuf() -> Gq {
        Gq(Box::into_raw(Box::new(Mpq { num: Mpz { v: 0 }, den: Mpz { v: 1 } })))
    }
    /// miroir gmpq_set_ui + normalisation (booleen -> rationnel)
    pub fn set_ui(&self, n: u64, d: u64) {
        if d == 0 { erreur_runtime("dénominateur nul dans un rationnel"); }
        unsafe { (*self.0).num.v = n as i128; (*self.0).den.v = d as i128; }
        self.canonicalize();
    }
    pub fn p(&self) -> *mut Mpq { self.0 }
    pub fn de(n: i128, d: i128) -> Gq {
        let q = Gq(Box::into_raw(Box::new(Mpq { num: Mpz { v: n }, den: Mpz { v: d } })));
        q.canonicalize();
        q
    }
    pub fn num(&self) -> *mut Mpz { unsafe { &mut (*self.0).num as *mut Mpz } }
    pub fn den(&self) -> *mut Mpz { unsafe { &mut (*self.0).den as *mut Mpz } }
    pub fn num_raw(&self) -> *const Mpz { self.num() }
    pub fn den_raw(&self) -> *const Mpz { self.den() }
    pub fn num_mut(&self) -> *mut Mpz { self.num() }
    pub fn den_mut(&self) -> *mut Mpz { self.den() }
    pub fn copier_de(&self, o: &Gq) { unsafe { *self.0 = *o.0 } }
    pub fn num_i128(&self) -> i128 { unsafe { (*self.0).num.v } }
    pub fn den_i128(&self) -> i128 { unsafe { (*self.0).den.v } }
    pub fn sgn(&self) -> i32 { self.num_i128().cmp(&0) as i32 }
    pub fn chaine(&self) -> String { format!("{}/{}", self.num_i128(), self.den_i128()) }
    pub fn get_d(&self) -> f64 { self.num_i128() as f64 / self.den_i128() as f64 }
    pub fn depuis_d(&self, x: f64) {
        if !x.is_finite() { erreur_runtime("conversion vers rationnel impossible"); }
        // fraction exacte du double : x = m / 2^e
        let bits = x.to_bits();
        let neg = (bits >> 63) != 0;
        let mut exp = ((bits >> 52) & 0x7FF) as i32;
        let mut man = (bits & 0xFFFF_FFFF_FFFFu64) as u128;
        if exp == 0 { exp = 1 - 1022 } else { man |= 1u128 << 52; exp -= 1023; }
        let mut num = man as i128;
        let mut den: i128 = 1;
        if exp >= 0 {
            if exp >= 128 { hors_plage(); }
            num = num.checked_mul(1i128 << exp).unwrap_or_else(hp128);
        } else {
            let e = (-exp) as u32;
            if e >= 128 { hors_plage(); }
            den = 1i128 << e;
        }
        if neg { num = -num; }
        unsafe { (*self.0).num.v = num; (*self.0).den.v = den; }
        self.canonicalize();
    }
    pub fn canonicalize(&self) {
        let (n, d) = unsafe { ((*self.0).num.v, (*self.0).den.v) };
        if d == 0 { erreur_runtime("dénominateur nul dans un rationnel"); }
        if n == 0 { unsafe { (*self.0).num.v = 0; (*self.0).den.v = 1; } return; }
        let g = gcd_i128(n, d) as i128;
        let (mut n2, mut d2) = (n / g, d / g);
        if d2 < 0 { n2 = -n2; d2 = -d2; }
        unsafe { (*self.0).num.v = n2; (*self.0).den.v = d2; }
    }
    pub fn neg_in(&self, a: &Gq) {
        match a.num_i128().checked_neg() { Some(x) => unsafe { (*self.0).num.v = x; } None => { hors_plage(); } }
        unsafe { (*self.0).den.v = a.den_i128(); }
    }
    pub fn add_in(&self, a: &Gq, b: &Gq) {
        let (an, ad, bn, bd) = (a.num_i128(), a.den_i128(), b.num_i128(), b.den_i128());
        let g = gcd_i128(ad, bd) as i128;
        let (ad1, bd1) = (ad / g, bd / g);
        let t1 = an.checked_mul(bd1).unwrap_or_else(hp128);
        let t2 = bn.checked_mul(ad1).unwrap_or_else(hp128);
        let n = t1.checked_add(t2).unwrap_or_else(hp128);
        let d = ad1.checked_mul(bd1).and_then(|x| x.checked_mul(g)).unwrap_or_else(hp128);
        unsafe { (*self.0).num.v = n; (*self.0).den.v = d; }
        self.canonicalize();
    }
    pub fn sub_in(&self, a: &Gq, b: &Gq) {
        let (an, ad, bn, bd) = (a.num_i128(), a.den_i128(), b.num_i128(), b.den_i128());
        let g = gcd_i128(ad, bd) as i128;
        let (ad1, bd1) = (ad / g, bd / g);
        let t1 = an.checked_mul(bd1).unwrap_or_else(hp128);
        let t2 = bn.checked_mul(ad1).unwrap_or_else(hp128);
        let n = t1.checked_sub(t2).unwrap_or_else(hp128);
        let d = ad1.checked_mul(bd1).and_then(|x| x.checked_mul(g)).unwrap_or_else(hp128);
        unsafe { (*self.0).num.v = n; (*self.0).den.v = d; }
        self.canonicalize();
    }
    pub fn mul_in(&self, a: &Gq, b: &Gq) {
        let n = a.num_i128().checked_mul(b.num_i128()).unwrap_or_else(hp128);
        let d = a.den_i128().checked_mul(b.den_i128()).unwrap_or_else(hp128);
        unsafe { (*self.0).num.v = n; (*self.0).den.v = d; }
        self.canonicalize();
    }
    pub fn div_in(&self, a: &Gq, b: &Gq) {
        if b.num_i128() == 0 { erreur_runtime("division par zéro"); }
        let bn = b.num_i128();
        let mut n = a.num_i128().checked_mul(b.den_i128().abs()).unwrap_or_else(hp128);
        let mut d = a.den_i128().checked_mul(bn.abs()).unwrap_or_else(hp128);
        if bn < 0 {
            n = n.checked_neg().unwrap_or_else(hp128);
            d = d.checked_neg().unwrap_or_else(hp128);
        }
        unsafe { (*self.0).num.v = n; (*self.0).den.v = d; }
        self.canonicalize();
    }
    pub fn cmp_gq(&self, b: &Gq) -> Ordering {
        let lhs = self.num_i128().checked_mul(b.den_i128());
        let rhs = b.num_i128().checked_mul(self.den_i128());
        match (lhs, rhs) {
            (Some(x), Some(y)) => x.cmp(&y),
            _ => {
                let x = self.get_d();
                let y = b.get_d();
                x.partial_cmp(&y).unwrap_or(Ordering::Equal)
            }
        }
    }
}
impl Drop for Gq {
    fn drop(&mut self) { unsafe { let _ = Box::from_raw(self.0); } }
}

// ── façade « gmp_* » : même surface que l'ancien FFI, implémentation native ─

pub unsafe fn gmp_add(r: *mut Mpz, a: *const Mpz, b: *const Mpz) {
    match vv(a).checked_add(vv(b)) { Some(x) => setv(r, x), None => { hors_plage(); } }
}
pub unsafe fn gmp_sub(r: *mut Mpz, a: *const Mpz, b: *const Mpz) {
    match vv(a).checked_sub(vv(b)) { Some(x) => setv(r, x), None => { hors_plage(); } }
}
pub unsafe fn gmp_mul(r: *mut Mpz, a: *const Mpz, b: *const Mpz) {
    match vv(a).checked_mul(vv(b)) { Some(x) => setv(r, x), None => { hors_plage(); } }
}
pub unsafe fn gmp_mul_ui(r: *mut Mpz, a: *const Mpz, u: u64) {
    match vv(a).checked_mul(u as i128) { Some(x) => setv(r, x), None => { hors_plage(); } }
}
pub unsafe fn gmp_mul_si(r: *mut Mpz, a: *const Mpz, u: i64) {
    match vv(a).checked_mul(u as i128) { Some(x) => setv(r, x), None => { hors_plage(); } }
}
pub unsafe fn gmp_neg(r: *mut Mpz, a: *const Mpz) {
    match vv(a).checked_neg() { Some(x) => setv(r, x), None => { hors_plage(); } }
}
pub unsafe fn gmp_set(r: *mut Mpz, a: *const Mpz) { setv(r, vv(a)) }
pub unsafe fn gmp_set_ui(r: *mut Mpz, u: u64) { setv(r, u as i128) }
pub unsafe fn gmp_set_si(r: *mut Mpz, i: i64) { setv(r, i as i128) }
pub unsafe fn gmp_set_d(r: *mut Mpz, a: f64) {
    if !a.is_finite() { hors_plage(); }
    setv(r, a as i128)
}
pub unsafe fn gmp_get_d(a: *const Mpz) -> f64 { vv(a) as f64 }
pub unsafe fn gmp_sgn(a: *const Mpz) -> i32 { vv(a).cmp(&0) as i32 }
pub unsafe fn gmp_cmp(a: *const Mpz, b: *const Mpz) -> i32 { vv(a).cmp(&vv(b)) as i32 }
pub unsafe fn gmp_add_ui(r: *mut Mpz, a: *const Mpz, u: u64) {
    match vv(a).checked_add(u as i128) { Some(x) => setv(r, x), None => { hors_plage(); } }
}
pub unsafe fn gmp_fdiv_q(r: *mut Mpz, a: *const Mpz, b: *const Mpz) {
    let (x, y) = (vv(a), vv(b));
    if y == 0 { erreur_runtime("division par zéro"); }
    setv(r, fdiv_i128(x, y).0);
}
pub unsafe fn gmp_fdiv_r(r: *mut Mpz, a: *const Mpz, b: *const Mpz) {
    let (x, y) = (vv(a), vv(b));
    if y == 0 { erreur_runtime("division par zéro"); }
    setv(r, fdiv_i128(x, y).1);
}
pub unsafe fn gmp_fdiv_ui(a: *const Mpz, u: u64) -> u64 {
    if u == 0 { erreur_runtime("division par zéro"); }
    let u = u as i128;
    let r = vv(a) % u;
    (if r < 0 { r + u } else { r }) as u64
}
pub unsafe fn gmp_mul_2exp(r: *mut Mpz, a: *const Mpz, e: u64) {
    if e >= 128 { hors_plage(); }
    match vv(a).checked_mul(1i128 << e) { Some(x) => setv(r, x), None => { hors_plage(); } }
}
pub unsafe fn gmp_pow_ui(r: *mut Mpz, a: *const Mpz, e: u64) {
    let base = vv(a);
    let mut acc: i128 = 1;
    for _ in 0..e {
        match acc.checked_mul(base) { Some(x) => acc = x, None => { hors_plage(); } }
    }
    setv(r, acc);
}
pub unsafe fn gmp_ui_pow_ui(r: *mut Mpz, base: u64, e: u64) {
    let b = base as i128;
    let mut acc: i128 = 1;
    for _ in 0..e {
        match acc.checked_mul(b) { Some(x) => acc = x, None => { hors_plage(); } }
    }
    setv(r, acc);
}
pub unsafe fn gmp_gcd(r: *mut Mpz, a: *const Mpz, b: *const Mpz) {
    setv(r, gcd_i128(vv(a), vv(b)) as i128);
}
pub unsafe fn gmp_lcm(r: *mut Mpz, a: *const Mpz, b: *const Mpz) {
    let (x, y) = (vv(a), vv(b));
    if x == 0 || y == 0 { setv(r, 0); return; }
    let g = gcd_i128(x, y) as i128;
    match (x / g).checked_mul(y.abs()) { Some(v) => setv(r, v), None => { hors_plage(); } }
}
pub unsafe fn gmp_fits_slong(a: *const Mpz) -> bool {
    let v = vv(a); v >= i64::MIN as i128 && v <= i64::MAX as i128
}
pub unsafe fn gmp_get_si(a: *const Mpz) -> i64 { vv(a) as i64 }
pub unsafe fn gmp_get_ui(a: *const Mpz) -> u64 { vv(a) as u64 }
pub unsafe fn gmp_sizeinbase(a: *const Mpz, base: i32) -> u64 {
    let v = vv(a);
    if base == 10 {
        if v == 0 { 1 } else { v.unsigned_abs().to_string().len() as u64 }
    } else {
        let uv = v.unsigned_abs();
        if uv == 0 { 0 } else { 128 - uv.leading_zeros() as u64 }
    }
}
pub unsafe fn gmp_tstbit(a: *const Mpz, bit: u64) -> bool {
    let v = vv(a);
    if v < 0 || bit >= 127 { return false; }
    ((v as u128) >> bit) & 1 == 1
}
/// primauté exacte (Miller-Rabin, bases déterministes sous 3,3·10^24).
pub unsafe fn gmp_probab_prime(a: *const Mpz) -> bool {
    let n = vv(a);
    if n < 2 { return false; }
    miller_rabin_u128(n as u128)
}
pub fn gmp_cmp_ge(a: &Gz, b: &Gz) -> bool { a.i128() >= b.i128() }

pub unsafe fn gmpq_set(r: *mut Mpq, a: *const Mpq) { *r = *a }
pub unsafe fn gmpq_set_z(q: *mut Mpq, z: *const Mpz) {
    (*q).num.v = vv(z); (*q).den.v = 1;
}
pub unsafe fn gmpq_set_z_field(dst: *mut Mpz, src: *const Mpz) { setv(dst, vv(src)) }
pub unsafe fn gmpq_set_ui(q: *mut Mpq, n: u64, d: u64) {
    if d == 0 { erreur_runtime("dénominateur nul dans un rationnel"); }
    (*q).num.v = n as i128; (*q).den.v = d as i128;
    gmpq_canonicalize(q);
}
pub unsafe fn gmpq_set_d(q: *mut Mpq, x: f64) {
    if !x.is_finite() { erreur_runtime("conversion vers rationnel impossible"); }
    let bits = x.to_bits();
    let neg = (bits >> 63) != 0;
    let mut exp = ((bits >> 52) & 0x7FF) as i32;
    let mut man = (bits & 0xFFFF_FFFF_FFFFu64) as u128;
    if exp == 0 { exp = 1 - 1022 } else { man |= 1u128 << 52; exp -= 1023; }
    let mut num = man as i128;
    let mut den: i128 = 1;
    if exp >= 0 {
        if exp >= 128 { hors_plage(); }
        num = num.checked_mul(1i128 << exp).unwrap_or_else(hp128);
    } else {
        let e = (-exp) as u32;
        if e >= 128 { hors_plage(); }
        den = 1i128 << e;
    }
    if neg { num = -num; }
    (*q).num.v = num; (*q).den.v = den;
    gmpq_canonicalize(q);
}
pub unsafe fn gmpq_get_d(q: *const Mpq) -> f64 {
    let (n, d) = qt(q);
    n as f64 / d as f64
}
pub unsafe fn gmpq_canonicalize(q: *mut Mpq) {
    let (n, d) = ((*q).num.v, (*q).den.v);
    if d == 0 { erreur_runtime("dénominateur nul dans un rationnel"); }
    if n == 0 { (*q).num.v = 0; (*q).den.v = 1; return; }
    let g = gcd_i128(n, d) as i128;
    let (mut n2, mut d2) = (n / g, d / g);
    if d2 < 0 { n2 = -n2; d2 = -d2; }
    (*q).num.v = n2; (*q).den.v = d2;
}
pub unsafe fn gmpq_cmp(a: *const Mpq, b: *const Mpq) -> i32 {
    let (an, ad) = qt(a);
    let (bn, bd) = qt(b);
    match (an.checked_mul(bd), bn.checked_mul(ad)) {
        (Some(x), Some(y)) => x.cmp(&y) as i32,
        _ => {
            let x = an as f64 / ad as f64;
            let y = bn as f64 / bd as f64;
            x.partial_cmp(&y).map(|o| o as i32).unwrap_or(0)
        }
    }
}
pub unsafe fn gmpq_add(r: *mut Mpq, a: *const Mpq, b: *const Mpq) {
    let (an, ad) = qt(a);
    let (bn, bd) = qt(b);
    let g = gcd_i128(ad, bd) as i128;
    let (ad1, bd1) = (ad / g, bd / g);
    let n = an.checked_mul(bd1).and_then(|x| bn.checked_mul(ad1).and_then(|y| x.checked_add(y)))
        .unwrap_or_else(hp128);
    let d = ad1.checked_mul(bd1).and_then(|x| x.checked_mul(g)).unwrap_or_else(hp128);
    (*r).num.v = n; (*r).den.v = d;
    gmpq_canonicalize(r);
}
pub unsafe fn gmpq_sub(r: *mut Mpq, a: *const Mpq, b: *const Mpq) {
    let (an, ad) = qt(a);
    let (bn, bd) = qt(b);
    let g = gcd_i128(ad, bd) as i128;
    let (ad1, bd1) = (ad / g, bd / g);
    let n = an.checked_mul(bd1).and_then(|x| bn.checked_mul(ad1).and_then(|y| x.checked_sub(y)))
        .unwrap_or_else(hp128);
    let d = ad1.checked_mul(bd1).and_then(|x| x.checked_mul(g)).unwrap_or_else(hp128);
    (*r).num.v = n; (*r).den.v = d;
    gmpq_canonicalize(r);
}
pub unsafe fn gmpq_mul(r: *mut Mpq, a: *const Mpq, b: *const Mpq) {
    let (an, ad) = qt(a);
    let (bn, bd) = qt(b);
    (*r).num.v = an.checked_mul(bn).unwrap_or_else(hp128);
    (*r).den.v = ad.checked_mul(bd).unwrap_or_else(hp128);
    gmpq_canonicalize(r);
}
pub unsafe fn gmpq_div(r: *mut Mpq, a: *const Mpq, b: *const Mpq) {
    let (an, ad) = qt(a);
    let (bn, bd) = qt(b);
    if bn == 0 { erreur_runtime("division par zéro"); }
    let mut n = an.checked_mul(bd.abs()).unwrap_or_else(hp128);
    let mut d = ad.checked_mul(bn.abs()).unwrap_or_else(hp128);
    if bn < 0 { n = n.checked_neg().unwrap_or_else(hp128); d = d.checked_neg().unwrap_or_else(hp128); }
    (*r).num.v = n; (*r).den.v = d;
    gmpq_canonicalize(r);
}
pub unsafe fn gmpq_neg(r: *mut Mpq, a: *const Mpq) {
    let (n, d) = qt(a);
    match n.checked_neg() { Some(v) => { (*r).num.v = v; (*r).den.v = d; } None => { hors_plage(); } }
}
pub unsafe fn gmpq_sgn_val(a: *const Mpq) -> i32 { qt(a).0.cmp(&0) as i32 }
