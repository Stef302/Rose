// Rose 2.0 — arithmétique exacte SANS GMP, sans FFI, sans dépendance externe.
// Entiers : num_bigint::BigInt (illimité, pur Rust). Rationnels : (BigInt,BigInt).
// Surface (Gz/Gq + gmp_*/gmpq_*) inchangée pour interp.rs / run.rs / jit.rs.
// Licence MIT.

use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::{One, Signed, ToPrimitive, Zero};
use std::cmp::Ordering;

use crate::run::erreur_runtime;

#[derive(Clone)]
pub struct Mpz { pub v: BigInt }
#[derive(Clone)]
pub struct Mpq { pub num: Mpz, pub den: Mpz }

pub fn hors_plage() -> ! { erreur_runtime("entier hors plage"); }

#[derive(Clone)]
pub struct Gz(pub *mut Mpz);

impl Gz {
    pub fn neuf() -> Gz { Gz(Box::into_raw(Box::new(Mpz { v: BigInt::zero() }))) }
    pub fn p(&self) -> *mut Mpz { self.0 }
    pub fn de_i128(x: i128) -> Gz { Gz(Box::into_raw(Box::new(Mpz { v: BigInt::from(x) }))) }
    pub fn de_i64(x: i64) -> Gz { Self::de_i128(x as i128) }
    pub fn de_u64(x: u64) -> Gz { Gz(Box::into_raw(Box::new(Mpz { v: BigInt::from(x) }))) }
    pub fn i128(&self) -> i128 { self.v().to_i128().unwrap_or(0) }
    pub fn v(&self) -> BigInt { unsafe { (*self.0).v.clone() } }
    pub fn clone_val(&self) -> Gz { let r = Gz::neuf(); r.set_v(self.v()); r }
    pub fn set_v(&self, x: BigInt) { unsafe { (*self.0).v = x } }

    pub fn poser_i64(&self, x: i64) { self.set_v(BigInt::from(x)); }
    pub fn poser_u64(&self, x: u64) { self.set_v(BigInt::from(x)); }
    pub fn poser_d(&self, d: f64) {
        if !d.is_finite() { erreur_runtime("nombre non fini"); }
        let s = format!("{}", d);
        match s.parse::<BigInt>() {
            Ok(x) => self.set_v(x),
            Err(_) => self.set_v(BigInt::from(d as i64)),
        }
    }
    pub fn copier_de(&self, o: &Gz) { self.set_v(o.v()); }
    pub fn copier_de_ptr(&self, o: *const Mpz) { self.set_v(unsafe { (*o).v.clone() }); }
    pub fn sgn(&self) -> i32 { self.v().cmp(&BigInt::zero()) as i32 }
    pub fn fits_i64(&self) -> bool {
        let x = self.v();
        x >= BigInt::from(i64::MIN) && x <= BigInt::from(i64::MAX)
    }
    pub fn fits_u64(&self) -> bool {
        let x = self.v();
        x >= BigInt::zero() && x <= BigInt::from(u64::MAX)
    }
    pub fn fits_u64_lt(&self, max: u64) -> bool {
        let x = self.v();
        x >= BigInt::zero() && x < BigInt::from(max)
    }
    pub fn fits_u64_max(&self, max: u64) -> bool {
        let x = self.v();
        x >= BigInt::zero() && x < BigInt::from(max)
    }
    pub fn fits_usize(&self, max: usize) -> bool { self.fits_u64_max(max as u64) }
    pub fn get_i64(&self) -> i64 { self.v().to_i64().unwrap_or(0) }
    pub fn get_u64(&self) -> u64 { self.v().to_u64().unwrap_or(0) }
    pub fn get_d(&self) -> f64 { self.v().to_f64().unwrap_or(0.0) }
    pub fn chaine(&self) -> String { self.v().to_string() }
    pub fn depuis_chaine(&self, s: &str) -> bool {
        match s.trim().parse::<BigInt>() {
            Ok(x) => { self.set_v(x); true }
            Err(_) => false,
        }
    }
    pub fn sizeinbase10(&self) -> u64 {
        let x = self.v();
        if x.is_zero() { 1 } else { x.to_string().len() as u64 }
    }
    pub fn taille_bits(&self) -> u64 {
        let x = self.v().abs();
        if x.is_zero() { 0 } else { x.bits() }
    }
    pub fn fdiv_u64(&self, u: u64) -> u64 {
        if u == 0 { erreur_runtime("division par zéro"); }
        let u = BigInt::from(u);
        let r = self.v().mod_floor(&u);
        r.to_u64().unwrap_or(0)
    }
    pub fn tstbit(&self, bit: u64) -> bool {
        let x = self.v();
        if x < BigInt::zero() || bit >= 1_000_000 { return false; }
        ((x >> bit) & BigInt::one()) == BigInt::one()
    }
    pub fn add(&self, a: &Gz, b: &Gz) { self.set_v(a.v() + b.v()); }
    pub fn sub(&self, a: &Gz, b: &Gz) { self.set_v(a.v() - b.v()); }
    pub fn mul(&self, a: &Gz, b: &Gz) { self.set_v(a.v() * b.v()); }
    pub fn neg(&self, a: &Gz) { self.set_v(-a.v()); }
    pub fn add_u64(&self, a: &Gz, u: u64) { self.set_v(a.v() + BigInt::from(u)); }
    pub fn mul_u64(&self, a: &Gz, u: u64) { self.set_v(a.v() * BigInt::from(u)); }
    pub fn mul_i64(&self, a: &Gz, u: i64) { self.set_v(a.v() * BigInt::from(u)); }
    pub fn mul_2exp(&self, a: &Gz, e: u64) { self.set_v(a.v() << (e as usize)); }
    pub fn fdiv_q(&self, a: &Gz, b: &Gz) {
        let bv = b.v();
        if bv.is_zero() { erreur_runtime("division par zéro"); }
        self.set_v(a.v().div_floor(&bv));
    }
    pub fn fdiv_r(&self, a: &Gz, b: &Gz) {
        let bv = b.v();
        if bv.is_zero() { erreur_runtime("division par zéro"); }
        self.set_v(a.v().mod_floor(&bv));
    }
    pub fn pow_u64(&self, a: &Gz, e: u64) {
        let mut base = a.v();
        let mut res = BigInt::one();
        let mut exp = e;
        while exp > 0 {
            if exp & 1 == 1 { res = res * base.clone(); }
            base = base.clone() * base;
            exp >>= 1;
        }
        self.set_v(res);
    }
    pub fn gcd(&self, a: &Gz, b: &Gz) { self.set_v(a.v().gcd(&b.v())); }
    pub fn lcm(&self, a: &Gz, b: &Gz) { self.set_v(a.v().lcm(&b.v())); }
    pub fn cmp_gz(&self, b: &Gz) -> Ordering { self.v().cmp(&b.v()) }
    pub fn est_premier(&self) -> bool { miller_rabin(&self.v()) }
}
impl Drop for Gz {
    fn drop(&mut self) { unsafe { let _ = Box::from_raw(self.0); } }
}

pub struct Gq(pub *mut Mpq);

impl Gq {
    pub fn neuf() -> Gq {
        Gq(Box::into_raw(Box::new(Mpq { num: Mpz { v: BigInt::zero() }, den: Mpz { v: BigInt::one() } })))
    }
    pub fn set_ui(&self, n: u64, d: u64) {
        unsafe {
            (*self.0).num.v = BigInt::from(n);
            (*self.0).den.v = if d == 0 { BigInt::one() } else { BigInt::from(d) };
        }
        self.canonicalize();
    }
    pub fn p(&self) -> *mut Mpq { self.0 }
    pub fn de(n: i128, d: i128) -> Gq {
        let mut q = Self::neuf();
        unsafe {
            (*q.0).num.v = BigInt::from(n);
            (*q.0).den.v = if d == 0 { BigInt::one() } else { BigInt::from(d) };
        }
        q.canonicalize();
        q
    }
    pub fn num(&self) -> *mut Mpz { unsafe { &mut (*self.0).num as *mut Mpz } }
    pub fn den(&self) -> *mut Mpz { unsafe { &mut (*self.0).den as *mut Mpz } }
    pub fn num_raw(&self) -> *const Mpz { self.num() }
    pub fn den_raw(&self) -> *const Mpz { self.den() }
    pub fn num_mut(&self) -> *mut Mpz { self.num() }
    pub fn den_mut(&self) -> *mut Mpz { self.den() }
    pub fn copier_de(&self, o: &Gq) { unsafe { *self.0 = (*o.0).clone() } }
    pub fn num_bigint(&self) -> BigInt { unsafe { (*self.0).num.v.clone() } }
    pub fn den_bigint(&self) -> BigInt { unsafe { (*self.0).den.v.clone() } }
    pub fn num_i128(&self) -> i128 { self.num_bigint().to_i128().unwrap_or(0) }
    pub fn den_i128(&self) -> i128 { self.den_bigint().to_i128().unwrap_or(0) }
    pub fn sgn(&self) -> i32 {
        self.num_bigint().cmp(&BigInt::zero()) as i32 * self.den_bigint().cmp(&BigInt::zero()) as i32
    }
    pub fn chaine(&self) -> String {
        let n = self.num_bigint();
        let d = self.den_bigint();
        if d == BigInt::one() { n.to_string() } else { format!("{}/{}", n, d) }
    }
    pub fn get_d(&self) -> f64 {
        let n = self.num_bigint().to_f64().unwrap_or(0.0);
        let d = self.den_bigint().to_f64().unwrap_or(1.0);
        n / d
    }
    pub fn depuis_d(&self, x: f64) {
        if x.fract() == 0.0 {
            unsafe { (*self.0).num.v = BigInt::from(x as i64); (*self.0).den.v = BigInt::one(); }
        } else {
            let scale: i64 = 1_000_000_000;
            unsafe {
                (*self.0).num.v = BigInt::from((x * scale as f64) as i64);
                (*self.0).den.v = BigInt::from(scale);
            }
            self.canonicalize();
        }
    }
    pub fn canonicalize(&self) {
        unsafe {
            let n = (*self.0).num.v.clone();
            let d = (*self.0).den.v.clone();
            if d.is_zero() { (*self.0).den.v = BigInt::one(); return; }
            let (n, d) = if d < BigInt::zero() { (-n, -d) } else { (n, d) };
            let g = n.gcd(&d).abs();
            let g = if g.is_zero() { BigInt::one() } else { g };
            (*self.0).num.v = n / g.clone();
            (*self.0).den.v = d / g;
        }
    }
    pub fn neg_in(&self, a: &Gq) {
        unsafe { *self.0 = (*a.0).clone(); }
        unsafe { (*self.0).num.v = -(*self.0).num.v.clone(); }
    }
    pub fn add_in(&self, a: &Gq, b: &Gq) {
        let na = a.num_bigint(); let da = a.den_bigint();
        let nb = b.num_bigint(); let db = b.den_bigint();
        unsafe {
            (*self.0).num.v = na * db.clone() + nb * da.clone();
            (*self.0).den.v = da * db;
        }
        self.canonicalize();
    }
    pub fn sub_in(&self, a: &Gq, b: &Gq) {
        let na = a.num_bigint(); let da = a.den_bigint();
        let nb = b.num_bigint(); let db = b.den_bigint();
        unsafe {
            (*self.0).num.v = na * db.clone() - nb * da.clone();
            (*self.0).den.v = da * db;
        }
        self.canonicalize();
    }
    pub fn mul_in(&self, a: &Gq, b: &Gq) {
        unsafe {
            (*self.0).num.v = a.num_bigint() * b.num_bigint();
            (*self.0).den.v = a.den_bigint() * b.den_bigint();
        }
        self.canonicalize();
    }
    pub fn div_in(&self, a: &Gq, b: &Gq) {
        unsafe {
            (*self.0).num.v = a.num_bigint() * b.den_bigint();
            (*self.0).den.v = a.den_bigint() * b.num_bigint();
        }
        self.canonicalize();
    }
    pub fn cmp_gq(&self, b: &Gq) -> Ordering {
        let a = self.num_bigint(); let da = self.den_bigint();
        let c = b.num_bigint(); let db = b.den_bigint();
        (a * db).cmp(&(c * da))
    }
}
impl Drop for Gq {
    fn drop(&mut self) { unsafe { let _ = Box::from_raw(self.0); } }
}

// ── façade gmp_* ────────────────────────────────────────────────────────────

pub unsafe fn gmp_add(r: *mut Mpz, a: *const Mpz, b: *const Mpz) {
    (*r).v = (*a).v.clone() + (*b).v.clone();
}
pub unsafe fn gmp_sub(r: *mut Mpz, a: *const Mpz, b: *const Mpz) {
    (*r).v = (*a).v.clone() - (*b).v.clone();
}
pub unsafe fn gmp_mul(r: *mut Mpz, a: *const Mpz, b: *const Mpz) {
    (*r).v = (*a).v.clone() * (*b).v.clone();
}
pub unsafe fn gmp_mul_ui(r: *mut Mpz, a: *const Mpz, u: u64) {
    (*r).v = (*a).v.clone() * BigInt::from(u);
}
pub unsafe fn gmp_mul_si(r: *mut Mpz, a: *const Mpz, u: i64) {
    (*r).v = (*a).v.clone() * BigInt::from(u);
}
pub unsafe fn gmp_neg(r: *mut Mpz, a: *const Mpz) { (*r).v = -(*a).v.clone(); }
pub unsafe fn gmp_set(r: *mut Mpz, a: *const Mpz) { (*r).v = (*a).v.clone(); }
pub unsafe fn gmp_set_ui(r: *mut Mpz, u: u64) { (*r).v = BigInt::from(u); }
pub unsafe fn gmp_set_si(r: *mut Mpz, i: i64) { (*r).v = BigInt::from(i); }
pub unsafe fn gmp_set_d(r: *mut Mpz, a: f64) { (*r).v = BigInt::from(a as i64); }
pub unsafe fn gmp_get_d(a: *const Mpz) -> f64 { (*a).v.to_f64().unwrap_or(0.0) }
pub unsafe fn gmp_sgn(a: *const Mpz) -> i32 { (*a).v.sign() as i32 }
pub unsafe fn gmp_cmp(a: *const Mpz, b: *const Mpz) -> i32 { (*a).v.cmp(&(*b).v) as i32 }
pub unsafe fn gmp_add_ui(r: *mut Mpz, a: *const Mpz, u: u64) {
    (*r).v = (*a).v.clone() + BigInt::from(u);
}
pub unsafe fn gmp_fdiv_q(r: *mut Mpz, a: *const Mpz, b: *const Mpz) {
    let bv = (*b).v.clone();
    if bv.is_zero() { erreur_runtime("division par zéro"); }
    (*r).v = (*a).v.clone().div_floor(&bv);
}
pub unsafe fn gmp_fdiv_r(r: *mut Mpz, a: *const Mpz, b: *const Mpz) {
    let bv = (*b).v.clone();
    if bv.is_zero() { erreur_runtime("division par zéro"); }
    (*r).v = (*a).v.clone().mod_floor(&bv);
}
pub unsafe fn gmp_fdiv_ui(a: *const Mpz, u: u64) -> u64 {
    if u == 0 { erreur_runtime("division par zéro"); }
    let u = BigInt::from(u);
    (*a).v.clone().mod_floor(&u).to_u64().unwrap_or(0)
}
pub unsafe fn gmp_mul_2exp(r: *mut Mpz, a: *const Mpz, e: u64) {
    (*r).v = (*a).v.clone() << (e as usize);
}
pub unsafe fn gmp_pow_ui(r: *mut Mpz, a: *const Mpz, e: u64) {
    let mut base = (*a).v.clone();
    let mut res = BigInt::one();
    let mut exp = e;
    while exp > 0 {
        if exp & 1 == 1 { res = res * base.clone(); }
        base = base.clone() * base;
        exp >>= 1;
    }
    (*r).v = res;
}
pub unsafe fn gmp_ui_pow_ui(r: *mut Mpz, base: u64, e: u64) {
    let mut b = BigInt::from(base);
    let mut res = BigInt::one();
    let mut exp = e;
    while exp > 0 {
        if exp & 1 == 1 { res = res * b.clone(); }
        b = b.clone() * b;
        exp >>= 1;
    }
    (*r).v = res;
}
pub unsafe fn gmp_gcd(r: *mut Mpz, a: *const Mpz, b: *const Mpz) {
    (*r).v = (*a).v.clone().gcd(&(*b).v);
}
pub unsafe fn gmp_lcm(r: *mut Mpz, a: *const Mpz, b: *const Mpz) {
    (*r).v = (*a).v.clone().lcm(&(*b).v);
}
pub unsafe fn gmp_fits_slong(a: *const Mpz) -> bool {
    (*a).v >= BigInt::from(i64::MIN) && (*a).v <= BigInt::from(i64::MAX)
}
pub unsafe fn gmp_get_si(a: *const Mpz) -> i64 { (*a).v.to_i64().unwrap_or(0) }
pub unsafe fn gmp_get_ui(a: *const Mpz) -> u64 { (*a).v.to_u64().unwrap_or(0) }
pub unsafe fn gmp_sizeinbase(a: *const Mpz, _base: i32) -> u64 {
    (*a).v.to_string().len() as u64
}
pub unsafe fn gmp_tstbit(a: *const Mpz, bit: u64) -> bool {
    let x = (*a).v.clone();
    if x < BigInt::zero() || bit >= 1_000_000 { return false; }
    ((x >> bit) & BigInt::one()) == BigInt::one()
}
pub unsafe fn gmp_probab_prime(a: *const Mpz) -> bool { miller_rabin(&(*a).v) }
pub fn gmp_cmp_ge(a: &Gz, b: &Gz) -> bool { a.cmp_gz(b) != Ordering::Less }

// gmpq
pub unsafe fn gmpq_set(r: *mut Mpq, a: *const Mpq) { *r = (*a).clone(); }
pub unsafe fn gmpq_set_z(q: *mut Mpq, z: *const Mpz) {
    (*q).num.v = (*z).v.clone();
    (*q).den.v = BigInt::one();
}
pub unsafe fn gmpq_set_z_field(dst: *mut Mpz, src: *const Mpz) {
    (*dst).v = (*src).v.clone();
}
pub unsafe fn gmpq_set_ui(q: *mut Mpq, n: u64, d: u64) {
    (*q).num.v = BigInt::from(n);
    (*q).den.v = if d == 0 { BigInt::one() } else { BigInt::from(d) };
}
pub unsafe fn gmpq_set_d(q: *mut Mpq, x: f64) {
    (*q).num.v = BigInt::from(x as i64);
    (*q).den.v = BigInt::one();
}
pub unsafe fn gmpq_add(r: *mut Mpq, a: *const Mpq, b: *const Mpq) {
    let na = (*a).num.v.clone(); let da = (*a).den.v.clone();
    let nb = (*b).num.v.clone(); let db = (*b).den.v.clone();
    (*r).num.v = na * db.clone() + nb * da.clone();
    (*r).den.v = da * db;
    let n = (*r).num.v.clone(); let d = (*r).den.v.clone();
    if d.is_zero() { (*r).den.v = BigInt::one(); return; }
    let (n, d) = if d < BigInt::zero() { (-n, -d) } else { (n, d) };
    let g = n.clone().gcd(&d).abs();
    let g = if g.is_zero() { BigInt::one() } else { g };
    (*r).num.v = n / g.clone();
    (*r).den.v = d / g;
}
pub unsafe fn gmpq_sub(r: *mut Mpq, a: *const Mpq, b: *const Mpq) {
    let na = (*a).num.v.clone(); let da = (*a).den.v.clone();
    let nb = (*b).num.v.clone(); let db = (*b).den.v.clone();
    (*r).num.v = na * db.clone() - nb * da.clone();
    (*r).den.v = da * db;
    let n = (*r).num.v.clone(); let d = (*r).den.v.clone();
    if d.is_zero() { (*r).den.v = BigInt::one(); return; }
    let (n, d) = if d < BigInt::zero() { (-n, -d) } else { (n, d) };
    let g = n.clone().gcd(&d).abs();
    let g = if g.is_zero() { BigInt::one() } else { g };
    (*r).num.v = n / g.clone();
    (*r).den.v = d / g;
}
pub unsafe fn gmpq_mul(r: *mut Mpq, a: *const Mpq, b: *const Mpq) {
    (*r).num.v = (*a).num.v.clone() * (*b).num.v.clone();
    (*r).den.v = (*a).den.v.clone() * (*b).den.v.clone();
    let n = (*r).num.v.clone(); let d = (*r).den.v.clone();
    if d.is_zero() { (*r).den.v = BigInt::one(); return; }
    let (n, d) = if d < BigInt::zero() { (-n, -d) } else { (n, d) };
    let g = n.clone().gcd(&d).abs();
    let g = if g.is_zero() { BigInt::one() } else { g };
    (*r).num.v = n / g.clone();
    (*r).den.v = d / g;
}
pub unsafe fn gmpq_div(r: *mut Mpq, a: *const Mpq, b: *const Mpq) {
    (*r).num.v = (*a).num.v.clone() * (*b).den.v.clone();
    (*r).den.v = (*a).den.v.clone() * (*b).num.v.clone();
    let n = (*r).num.v.clone(); let d = (*r).den.v.clone();
    if d.is_zero() { (*r).den.v = BigInt::one(); return; }
    let (n, d) = if d < BigInt::zero() { (-n, -d) } else { (n, d) };
    let g = n.clone().gcd(&d).abs();
    let g = if g.is_zero() { BigInt::one() } else { g };
    (*r).num.v = n / g.clone();
    (*r).den.v = d / g;
}
pub unsafe fn gmpq_neg(r: *mut Mpq, a: *const Mpq) {
    (*r).num.v = -(*a).num.v.clone();
    (*r).den.v = (*a).den.v.clone();
}
pub unsafe fn gmpq_cmp(a: *const Mpq, b: *const Mpq) -> i32 {
    let left = (*a).num.v.clone() * (*b).den.v.clone();
    let right = (*b).num.v.clone() * (*a).den.v.clone();
    left.cmp(&right) as i32
}
pub unsafe fn gmpq_canonicalize(q: *mut Mpq) {
    let n = (*q).num.v.clone(); let d = (*q).den.v.clone();
    if d.is_zero() { (*q).den.v = BigInt::one(); return; }
    let (n, d) = if d < BigInt::zero() { (-n, -d) } else { (n, d) };
    let g = n.clone().gcd(&d).abs();
    let g = if g.is_zero() { BigInt::one() } else { g };
    (*q).num.v = n / g.clone();
    (*q).den.v = d / g;
}
// Miller-Rabin
fn miller_rabin(n: &BigInt) -> bool {
    if n < &BigInt::from(2) { return false; }
    let small = [2i64,3,5,7,11,13,17,19,23,29,31,37];
    for &p in &small {
        let pb = BigInt::from(p);
        if n == &pb { return true; }
        if n % &pb == BigInt::zero() { return false; }
    }
    let mut d = n.clone() - BigInt::one();
    let mut s = 0u32;
    while &d % 2 == BigInt::zero() { d /= 2; s += 1; }
    for &a in &small {
        let a = BigInt::from(a);
        if &a % n == BigInt::zero() { continue; }
        let mut x = modpow(&a, &d, n);
        if x == BigInt::one() || x == n.clone() - BigInt::one() { continue; }
        let mut composite = true;
        for _ in 0..(s-1) {
            x = (&x * &x) % n;
            if x == n.clone() - BigInt::one() { composite = false; break; }
        }
        if composite { return false; }
    }
    true
}
fn modpow(base: &BigInt, exp: &BigInt, m: &BigInt) -> BigInt {
    let mut result = BigInt::one();
    let mut b = base % m;
    let mut e = exp.clone();
    while e > BigInt::zero() {
        if &e % 2 == BigInt::one() { result = (&result * &b) % m; }
        b = (&b * &b) % m;
        e /= 2;
    }
    result
}
