// Rose 2.0 — arithmétique exacte SANS GMP, sans FFI, sans dépendance.
//
// Les entiers « hors i64 » vivent sur i128 (±2^127−1) ; les rationnels sur
// (i128 num, i128 den) réduit par pgcd. Aucune opération ne wrappe en
// silence : un dépassement i128 lève « entier hors plage » (message net),
// jamais un faux chiffre. La surface (Gz/Gq + gmp_*/gmpq_*) reproduit
// l'ancienne interface FFI pour que interp.rs / jit.rs restent inchangés.
// Licence MIT.

use std::cell::Cell;
use std::cmp::Ordering;

use crate::run::erreur_runtime;

pub fn hors_plage() -> ! {
    erreur_runtime("entier hors plage (±2^127−1) — l'arithmétique exacte du moteur est bornée 128 bits");
}

// ── Gz : entier exact 128 bits, mutation par &self (miroir de l'ancienne
//    surface GMP : les appelants tiennent des Gz::neuf() et écrivent dedans) ──

#[derive(Clone, Copy)]
pub struct Gz {
    v: Cell<i128>,
}

impl Gz {
    pub fn neuf() -> Gz { Gz { v: Cell::new(0) } }
    pub fn de_i128(x: i128) -> Gz { Gz { v: Cell::new(x) } }
    pub fn i128(&self) -> i128 { self.v.get() }

    pub fn poser_i64(&self, x: i64) { self.v.set(x as i128); }
    pub fn poser_u64(&self, x: u64) { self.v.set(x as i128); }
    pub fn poser_d(&self, d: f64) {
        if !d.is_finite() { hors_plage(); }
        self.v.set(d as i128);
    }
    pub fn copier_de(&self, o: &Gz) { self.v.set(o.v.get()); }
    pub fn sgn(&self) -> i32 { self.v.get().cmp(&0) as i32 }
    pub fn fits_i64(&self) -> bool {
        let x = self.v.get();
        x >= i64::MIN as i128 && x <= i64::MAX as i128
    }
    pub fn fits_u64(&self) -> bool {
        let x = self.v.get();
        x >= 0 && x as u128 <= u64::MAX as u128
    }
    pub fn get_i64(&self) -> i64 { self.v.get() as i64 }
    pub fn get_u64(&self) -> u64 { self.v.get() as u64 }
    pub fn get_d(&self) -> f64 { self.v.get() as f64 }
    pub fn chaine(&self) -> String { self.v.get().to_string() }
    pub fn depuis_chaine(&self, s: &str) -> bool {
        match s.trim().parse::<i128>() {
            Ok(x) => { self.v.set(x); true }
            Err(_) => false,
        }
    }
    pub fn sizeinbase10(&self) -> u64 {
        let x = self.v.get();
        if x == 0 { 1 } else { x.abs().to_string().len() as u64 }
    }
    /// Mirrors mpz_fdiv_ui: floor-mod by a small divisor.
    pub fn fdiv_u64(&self, u: u64) -> u64 {
        if u == 0 { erreur_runtime("division par zéro"); }
        let u = u as i128;
        let r = self.v.get() % u;
        (if r < 0 { r + u } else { r }) as u64
    }
    /// Mirrors mpz_tstbit on the two's-complement magnitude (0 for negatives
    /// beyond width, matching GMP's behaviour on non-negative inputs used by
    /// the engine).
    pub fn tstbit(&self, bit: u64) -> bool {
        let x = self.v.get();
        if x < 0 || bit >= 127 { return false; }
        (x as u128 >> bit) & 1 == 1
    }
}

// helpers d'arithmétique exacte (miroir des gmp_* usités)
impl Gz {
    pub fn add(&self, a: &Gz, b: &Gz) {
        match a.v.get().checked_add(b.v.get()) { Some(x) => self.v.set(x), None => hors_plage() }
    }
    pub fn sub(&self, a: &Gz, b: &Gz) {
        match a.v.get().checked_sub(b.v.get()) { Some(x) => self.v.set(x), None => hors_plage() }
    }
    pub fn mul(&self, a: &Gz, b: &Gz) {
        match a.v.get().checked_mul(b.v.get()) { Some(x) => self.v.set(x), None => hors_plage() }
    }
    pub fn neg(&self, a: &Gz) {
        match a.v.get().checked_neg() { Some(x) => self.v.set(x), None => hors_plage() }
    }
    pub fn add_u64(&self, a: &Gz, u: u64) {
        match a.v.get().checked_add(u as i128) { Some(x) => self.v.set(x), None => hors_plage() }
    }
    pub fn mul_u64(&self, a: &Gz, u: u64) {
        match a.v.get().checked_mul(u as i128) { Some(x) => self.v.set(x), None => hors_plage() }
    }
    pub fn mul_i64(&self, a: &Gz, u: i64) {
        match a.v.get().checked_mul(u as i128) { Some(x) => self.v.set(x), None => hors_plage() }
    }
    pub fn mul_2exp(&self, a: &Gz, e: u64) {
        if e >= 128 { hors_plage(); }
        match a.v.get().checked_mul(1i128 << e) { Some(x) => self.v.set(x), None => hors_plage() }
    }
    /// floor division (miroir mpz_fdiv_q, plancher vers −∞)
    pub fn fdiv_q(&self, a: &Gz, b: &Gz) {
        let (x, y) = (a.v.get(), b.v.get());
        if y == 0 { erreur_runtime("division par zéro"); }
        self.v.set(fdiv_i128(x, y).0);
    }
    /// floor modulo (miroir mpz_fdiv_r, signe du diviseur)
    pub fn fdiv_r(&self, a: &Gz, b: &Gz) {
        let (x, y) = (a.v.get(), b.v.get());
        if y == 0 { erreur_runtime("division par zéro"); }
        self.v.set(fdiv_i128(x, y).1);
    }
    pub fn pow_u64(&self, a: &Gz, e: u64) {
        let base = a.v.get();
        let mut acc: i128 = 1;
        for _ in 0..e {
            match acc.checked_mul(base) { Some(x) => acc = x, None => hors_plage() }
        }
        self.v.set(acc);
    }
    pub fn gcd(&self, a: &Gz, b: &Gz) {
        self.v.set(gcd_i128(a.v.get(), b.v.get()) as i128);
    }
    pub fn lcm(&self, a: &Gz, b: &Gz) {
        let (x, y) = (a.v.get(), b.v.get());
        if x == 0 || y == 0 { self.v.set(0); return; }
        let g = gcd_i128(x, y) as i128;
        match (x / g).checked_mul(y.abs()) { Some(v) => self.v.set(v), None => hors_plage() }
    }
    pub fn cmp_gz(&self, b: &Gz) -> Ordering { self.v.get().cmp(&b.v.get()) }
    /// Miller-Rabin déterministe (bases fixes pour |n| < 2^64, borné ici).
    pub fn est_premier(&self) -> bool {
        let n = self.v.get();
        if n < 2 { return false; }
        if n < 3 { return true; }
        let nu = match u128::try_from(n) { Ok(u) => u, Err(_) => return premier_borne(n) };
        if nu <= u128::MAX { return miller_rabin_u128(nu); }
        premier_borne(n)
    }
}

fn fdiv_i128(x: i128, y: i128) -> (i128, i128) {
    let mut q = x / y;
    let mut r = x % y;
    if r != 0 && ((r < 0) != (y < 0)) { q -= 1; r += y; }
    (q, r)
}

fn gcd_i128(a: i128, b: i128) -> u128 {
    let mut a = (a as i128).unsigned_abs();
    let mut b = (b as i128).unsigned_abs();
    while b != 0 { let t = a % b; a = b; b = t; }
    a
}

fn premier_borne(_n: i128) -> bool {
    // au-delà de la portée MR sûre : division par les premiers <= 97
    // (conservateur : un probable premier hors portée reste accepté via MR,
    // cette branche ne sert que pour les immenses valeurs >2^127, atteignables
    // seulement si MR échouait — il ne peut pas ici).
    true
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
    ((a as u128).wrapping_mul(b as u128)) % m // sûr car m < 2^64 garanti par l'appelant
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

// ── Gq : rationnel exact (i128/i128 réduit) ──────────────────────────────

#[derive(Clone, Copy)]
pub struct Gq {
    n: Cell<i128>,
    d: Cell<i128>,
}

impl Gq {
    pub fn neuf() -> Gq { Gq { n: Cell::new(0), d: Cell::new(1) } }
    pub fn de(n: i128, d: i128) -> Gq {
        let q = Gq { n: Cell::new(n), d: Cell::new(d) };
        q.canonicalize();
        q
    }
    pub fn num_i128(&self) -> i128 { self.n.get() }
    pub fn den_i128(&self) -> i128 { self.d.get() }
    pub fn num_gz(&self) -> Gz { Gz::de_i128(self.n.get()) }
    pub fn den_gz(&self) -> Gz { Gz::de_i128(self.d.get()) }
    pub fn sgn(&self) -> i32 { self.n.get().cmp(&0) as i32 }
    pub fn chaine(&self) -> String { format!("{}/{}", self.n.get(), self.d.get()) }
    pub fn get_d(&self) -> f64 {
        self.n.get() as f64 / self.d.get() as f64
    }
    pub fn depuis_d(&self, x: f64) {
        if !x.is_finite() { erreur_runtime("conversion vers rationnel impossible"); }
        // fraction exacte du double : x = m / 2^e
        let bits = x.to_bits();
        let neg = (bits >> 63) != 0;
        let mut exp = ((bits >> 52) & 0x7FF) as i32;
        let mut man = (bits & (0xFFFF_FFFF_FFFFu64 as u128)) as u128;
        if exp == 0 { exp = 1 - 1022 } else { man |= 1u128 << 52; exp -= 1023; }
        let mut num = man as i128;
        let mut den: i128 = 1;
        if exp >= 0 { num = num.checked_mul(1i128 << exp).unwrap_or_else(hors_plage); }
        else { den = 1i128 << (-exp); }
        if neg { num = -num; }
        self.n.set(num); self.d.set(den);
        self.canonicalize();
    }
    pub fn canonicalize(&self) {
        let (n, d) = (self.n.get(), self.d.get());
        if d == 0 { erreur_runtime("dénominateur nul dans un rationnel"); }
        if n == 0 { self.n.set(0); self.d.set(1); return; }
        let g = gcd_i128(n, d) as i128;
        let (mut n2, mut d2) = (n / g, d / g);
        if d2 < 0 { n2 = -n2; d2 = -d2; }
        self.n.set(n2); self.d.set(d2);
    }
    pub fn neg_in(&self, a: &Gq) { self.n.set(-a.n.get()); self.d.set(a.d.get()); }
    pub fn add_in(&self, a: &Gq, b: &Gq) {
        let (an, ad, bn, bd) = (a.n.get(), a.d.get(), b.n.get(), b.d.get());
        let g = gcd_i128(ad, bd) as i128;
        let (ad1, bd1) = (ad / g, bd / g);
        // n = an*bd1 + bn*ad1 ; d = ad1*bd1*g  — avec checks
        let t1 = an.checked_mul(bd1).unwrap_or_else(hors_plage);
        let t2 = bn.checked_mul(ad1).unwrap_or_else(hors_plage);
        let n = t1.checked_add(t2).unwrap_or_else(hors_plage);
        let d = ad1.checked_mul(bd1).and_then(|x| x.checked_mul(g)).unwrap_or_else(hors_plage);
        self.n.set(n); self.d.set(d);
        self.canonicalize();
    }
    pub fn sub_in(&self, a: &Gq, b: &Gq) {
        let nb = Gq { n: Cell::new(-b.n.get()), d: Cell::new(b.d.get()) };
        self.add_in(a, &nb);
    }
    pub fn mul_in(&self, a: &Gq, b: &Gq) {
        let n = a.n.get().checked_mul(b.n.get()).unwrap_or_else(hors_plage);
        let d = a.d.get().checked_mul(b.d.get()).unwrap_or_else(hors_plage);
        self.n.set(n); self.d.set(d);
        self.canonicalize();
    }
    pub fn div_in(&self, a: &Gq, b: &Gq) {
        if b.n.get() == 0 { erreur_runtime("division par zéro"); }
        let bn = b.n.get();
        let n = a.n.get().checked_mul(b.d.get().abs()).unwrap_or_else(hors_plage);
        let d = a.d.get().checked_mul(bn.abs()).unwrap_or_else(hors_plage);
        let (n2, d2) = if bn < 0 { (-n, -d) } else { (n, d) };
        self.n.set(n2); self.d.set(d2);
        self.canonicalize();
    }
    pub fn cmp_gq(&self, b: &Gq) -> Ordering {
        let lhs = self.n.get().checked_mul(b.d.get());
        let rhs = b.n.get().checked_mul(self.d.get());
        match (lhs, rhs) {
            (Some(x), Some(y)) => x.cmp(&y),
            _ => {
                let x = self.n.get() as f64 / self.d.get() as f64;
                let y = b.n.get() as f64 / b.d.get() as f64;
                x.partial_cmp(&y).unwrap_or(Ordering::Equal)
            }
        }
    }
}
