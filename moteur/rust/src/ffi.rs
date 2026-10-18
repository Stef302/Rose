// ═══════════ Rose — moteur natif Rust : liaisons GMP (C) et FFI ═══════════
// GMP est liée statiquement dans le binaire : aucune dépendance d'exécution.
#![allow(dead_code, non_upper_case_globals, non_camel_case_types)]

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_double, c_int, c_long, c_ulong, c_void};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MpzT {
    pub alloc: c_int,
    pub size: c_int,
    pub d: *mut c_ulong,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MpqT {
    pub num: MpzT,
    pub den: MpzT,
}


extern "C" {
    #[link_name = "__gmpz_init"]
    pub fn mpz_init(x: *mut MpzT);
    #[link_name = "__gmpz_clear"]
    pub fn mpz_clear(x: *mut MpzT);
    #[link_name = "__gmpz_set"]
    pub fn mpz_set(r: *mut MpzT, x: *const MpzT);
    #[link_name = "__gmpz_set_ui"]
    pub fn mpz_set_ui(r: *mut MpzT, x: c_ulong);
    #[link_name = "__gmpz_set_si"]
    pub fn mpz_set_si(r: *mut MpzT, x: c_long);
    #[link_name = "__gmpz_set_d"]
    pub fn mpz_set_d(r: *mut MpzT, x: c_double);
    #[link_name = "__gmpz_set_str"]
    pub fn mpz_set_str(r: *mut MpzT, s: *const c_char, base: c_int) -> c_int;
    #[link_name = "__gmpz_get_str"]
    pub fn mpz_get_str(out: *mut c_char, base: c_int, x: *const MpzT) -> *mut c_char;
    #[link_name = "__gmpz_get_si"]
    pub fn mpz_get_si(x: *const MpzT) -> c_long;
    #[link_name = "__gmpz_get_ui"]
    pub fn mpz_get_ui(x: *const MpzT) -> c_ulong;
    #[link_name = "__gmpz_get_d"]
    pub fn mpz_get_d(x: *const MpzT) -> c_double;
    #[link_name = "__gmpz_fits_slong_p"]
    pub fn mpz_fits_slong_p(x: *const MpzT) -> c_int;
    #[link_name = "__gmpz_fits_ulong_p"]
    pub fn mpz_fits_ulong_p(x: *const MpzT) -> c_int;
    #[link_name = "__gmpz_add"]
    pub fn mpz_add(r: *mut MpzT, a: *const MpzT, b: *const MpzT);
    #[link_name = "__gmpz_sub"]
    pub fn mpz_sub(r: *mut MpzT, a: *const MpzT, b: *const MpzT);
    #[link_name = "__gmpz_mul"]
    pub fn mpz_mul(r: *mut MpzT, a: *const MpzT, b: *const MpzT);
    #[link_name = "__gmpz_mul_ui"]
    pub fn mpz_mul_ui(r: *mut MpzT, a: *const MpzT, b: c_ulong);
    #[link_name = "__gmpz_tdiv_q"]
    pub fn mpz_tdiv_q(r: *mut MpzT, a: *const MpzT, b: *const MpzT);
    #[link_name = "__gmpz_fdiv_q"]
    pub fn mpz_fdiv_q(r: *mut MpzT, a: *const MpzT, b: *const MpzT);
    #[link_name = "__gmpz_fdiv_r"]
    pub fn mpz_fdiv_r(r: *mut MpzT, a: *const MpzT, b: *const MpzT);
    #[link_name = "__gmpz_gcd"]
    pub fn mpz_gcd(r: *mut MpzT, a: *const MpzT, b: *const MpzT);
    #[link_name = "__gmpz_lcm"]
    pub fn mpz_lcm(r: *mut MpzT, a: *const MpzT, b: *const MpzT);
    #[link_name = "__gmpz_pow_ui"]
    pub fn mpz_pow_ui(r: *mut MpzT, a: *const MpzT, e: c_ulong);
    #[link_name = "__gmpz_abs"]
    pub fn mpz_abs(r: *mut MpzT, a: *const MpzT);
    #[link_name = "__gmpz_neg"]
    pub fn mpz_neg(r: *mut MpzT, a: *const MpzT);
    #[link_name = "__gmpz_cmp"]
    pub fn mpz_cmp(a: *const MpzT, b: *const MpzT) -> c_int;
    #[link_name = "__gmpz_cmp_ui"]
    pub fn mpz_cmp_ui(a: *const MpzT, b: c_ulong) -> c_int;
    #[link_name = "__gmpz_sizeinbase"]
    pub fn mpz_sizeinbase(a: *const MpzT, base: c_int) -> usize;
    #[link_name = "__gmpz_probab_prime_p"]
    pub fn mpz_probab_prime_p(n: *const MpzT, reps: c_int) -> c_int;
    #[link_name = "__gmpz_fib_ui"]
    pub fn mpz_fib_ui(r: *mut MpzT, n: c_ulong);
    #[link_name = "__gmpz_import"]
    pub fn mpz_import(rop: *mut MpzT, count: usize, order: c_int, size: usize, endian: c_int, nails: usize, op: *const c_void);
    #[link_name = "__gmpz_export"]
    pub fn mpz_export(op: *mut c_void, count: *mut usize, order: c_int, size: usize, endian: c_int, nails: usize, src: *const MpzT) -> *mut c_void;
    #[link_name = "__gmpz_tstbit"]
    pub fn mpz_tstbit(x: *const MpzT, bit: u64) -> c_int;
    #[link_name = "__gmpz_ui_pow_ui"]
    pub fn mpz_ui_pow_ui(r: *mut MpzT, base: c_ulong, exp: u64);
    #[link_name = "__gmpz_powm"]
    pub fn mpz_powm(r: *mut MpzT, b: *const MpzT, e: *const MpzT, m: *const MpzT);
    #[link_name = "__gmpq_init"]
    pub fn mpq_init(x: *mut MpqT);
    #[link_name = "__gmpq_clear"]
    pub fn mpq_clear(x: *mut MpqT);
    #[link_name = "__gmpq_set"]
    pub fn mpq_set(r: *mut MpqT, x: *const MpqT);
    #[link_name = "__gmpq_set_z"]
    pub fn mpq_set_z(r: *mut MpqT, x: *const MpzT);
    #[link_name = "__gmpq_set_d"]
    pub fn mpq_set_d(r: *mut MpqT, x: c_double);
    #[link_name = "__gmpq_set_str"]
    pub fn mpq_set_str(r: *mut MpqT, s: *const c_char, base: c_int) -> c_int;
    #[link_name = "__gmpq_canonicalize"]
    pub fn mpq_canonicalize(x: *mut MpqT);
    #[link_name = "__gmpq_get_d"]
    pub fn mpq_get_d(x: *const MpqT) -> c_double;
    #[link_name = "__gmpq_cmp"]
    pub fn mpq_cmp(a: *const MpqT, b: *const MpqT) -> c_int;
    #[link_name = "__gmpq_add"]
    pub fn mpq_add(r: *mut MpqT, a: *const MpqT, b: *const MpqT);
    #[link_name = "__gmpq_sub"]
    pub fn mpq_sub(r: *mut MpqT, a: *const MpqT, b: *const MpqT);
    #[link_name = "__gmpq_mul"]
    pub fn mpq_mul(r: *mut MpqT, a: *const MpqT, b: *const MpqT);
    #[link_name = "__gmpq_div"]
    pub fn mpq_div(r: *mut MpqT, a: *const MpqT, b: *const MpqT);
}

extern "C" {
    pub fn free(ptr: *mut c_void);
    pub fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
    pub fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
    pub fn dlclose(handle: *mut c_void) -> c_int;
}
pub const RTLD_NOW: c_int = 2;
pub const RTLD_LOCAL: c_int = 0;

/* ═══════════════════ Wrappers sûrs (Drop = mpz_clear) ═══════════════════ */

pub struct Z(Box<MpzT>);

impl Z {
    pub fn new() -> Z {
        unsafe {
            let mut b = Box::new(MpzT { alloc: 0, size: 0, d: std::ptr::null_mut() });
            mpz_init(&mut *b);
            Z(b)
        }
    }
    pub fn i(v: i64) -> Z {
        let z = Z::u(v.unsigned_abs());
        if v < 0 { z.neg() } else { z }
    }
    pub fn u(v: u64) -> Z {
        // Import d'un mot de 8 octets : exact sur toutes les plateformes
        // (mpz_set_si/ui dépend de « long », 32 bits sous Windows).
        let mut z = Z::new();
        let mut w: u64 = v;
        unsafe { mpz_import(z.0.as_mut() as *mut _, 1, -1, std::mem::size_of::<u64>(), 0, 0,
                            &mut w as *mut _ as *const c_void) };
        z
    }
    pub fn de_f64(v: f64) -> Z {
        let mut z = Z::new();
        unsafe { mpz_set_d(z.0.as_mut() as *mut _, v) };
        z
    }
    pub fn depuis_texte(s: &str) -> Option<Z> {
        let c = CString::new(s).ok()?;
        let mut z = Z::new();
        unsafe {
            if mpz_set_str(z.0.as_mut() as *mut _, c.as_ptr(), 10) == 0 { Some(z) } else { None }
        }
    }
    pub fn texte(&self) -> String {
        unsafe {
            let p = mpz_get_str(std::ptr::null_mut(), 10, self.0.as_ref() as *const _);
            let s = CStr::from_ptr(p).to_string_lossy().into_owned();
            free(p as *mut c_void);
            s
        }
    }
    pub fn sgn(&self) -> i32 {
        // mpz_sgn est une macro gmp.h : accès direct à _mp_size
        let taille = unsafe { (*self.0.as_ref()).size };
        if taille < 0 { -1 } else if taille > 0 { 1 } else { 0 }
    }
    pub fn cmp(&self, a: &Z) -> i32 {
        unsafe { mpz_cmp(self.0.as_ref() as *const _, a.0.as_ref() as *const _) }
    }
    pub fn tient_i64(&self) -> bool {
        unsafe { mpz_fits_slong_p(self.0.as_ref() as *const _) != 0 }
    }
    pub fn tient_u64(&self) -> bool {
        unsafe { mpz_fits_ulong_p(self.0.as_ref() as *const _) != 0 }
    }
    pub fn en_u64(&self) -> u64 {
        let mut w: u64 = 0;
        let mut cnt: usize = 1;
        unsafe { mpz_export(&mut w as *mut _ as *mut c_void, &mut cnt, -1,
                            std::mem::size_of::<u64>(), 0, 0, self.0.as_ref() as *const _) };
        w
    }
    pub fn en_i64(&self) -> i64 {
        let u = self.en_u64();
        if self.sgn() < 0 { 0u64.wrapping_sub(u) as i64 } else { u as i64 }
    }
    pub fn en_f64(&self) -> f64 {
        unsafe { mpz_get_d(self.0.as_ref() as *const _) }
    }
    pub fn chiffres10(&self) -> usize {
        unsafe { mpz_sizeinbase(self.0.as_ref() as *const _, 10) }
    }
    pub fn add(&self, a: &Z) -> Z {
        let mut r = Z::new();
        unsafe { mpz_add(r.0.as_mut() as *mut _, self.0.as_ref() as *const _, a.0.as_ref() as *const _) };
        r
    }
    pub fn sub(&self, a: &Z) -> Z {
        let mut r = Z::new();
        unsafe { mpz_sub(r.0.as_mut() as *mut _, self.0.as_ref() as *const _, a.0.as_ref() as *const _) };
        r
    }
    pub fn mul(&self, a: &Z) -> Z {
        let mut r = Z::new();
        unsafe { mpz_mul(r.0.as_mut() as *mut _, self.0.as_ref() as *const _, a.0.as_ref() as *const _) };
        r
    }
    pub fn mul_u(&self, a: u64) -> Z {
        if a <= u32::MAX as u64 {
            let mut r = Z::new();
            unsafe { mpz_mul_ui(r.0.as_mut() as *mut _, self.0.as_ref() as *const _, a as c_ulong) };
            return r;
        }
        self.mul(&Z::u(a))
    }
    pub fn gcd(&self, a: &Z) -> Z {
        let mut r = Z::new();
        unsafe { mpz_gcd(r.0.as_mut() as *mut _, self.0.as_ref() as *const _, a.0.as_ref() as *const _) };
        r
    }
    pub fn lcm(&self, a: &Z) -> Z {
        let mut r = Z::new();
        unsafe { mpz_lcm(r.0.as_mut() as *mut _, self.0.as_ref() as *const _, a.0.as_ref() as *const _) };
        r
    }
    pub fn tdiv_q(&self, a: &Z) -> Z {
        let mut r = Z::new();
        unsafe { mpz_tdiv_q(r.0.as_mut() as *mut _, self.0.as_ref() as *const _, a.0.as_ref() as *const _) };
        r
    }
    pub fn fdiv_q(&self, a: &Z) -> Z {
        let mut r = Z::new();
        unsafe { mpz_fdiv_q(r.0.as_mut() as *mut _, self.0.as_ref() as *const _, a.0.as_ref() as *const _) };
        r
    }
    pub fn fdiv_r(&self, a: &Z) -> Z {
        let mut r = Z::new();
        unsafe { mpz_fdiv_r(r.0.as_mut() as *mut _, self.0.as_ref() as *const _, a.0.as_ref() as *const _) };
        r
    }
    pub fn pow_u(&self, e: u64) -> Z {
        if e <= u32::MAX as u64 {
            let mut r = Z::new();
            unsafe { mpz_pow_ui(r.0.as_mut() as *mut _, self.0.as_ref() as *const _, e as c_ulong) };
            return r;
        }
        self.pow_carres(e)
    }
    fn pow_carres(&self, e: u64) -> Z {
        let mut base = Z::new();
        unsafe { mpz_set(base.0.as_mut() as *mut _, self.0.as_ref() as *const _) };
        let mut r = Z::i(1);
        let mut exp = e;
        while exp > 0 {
            if exp & 1 == 1 { r = r.mul(&base); }
            base = base.mul(&base);
            exp >>= 1;
        }
        r
    }
    pub fn abs(&self) -> Z {
        let mut r = Z::new();
        unsafe { mpz_abs(r.0.as_mut() as *mut _, self.0.as_ref() as *const _) };
        r
    }
    pub fn neg(&self) -> Z {
        let mut r = Z::new();
        unsafe { mpz_neg(r.0.as_mut() as *mut _, self.0.as_ref() as *const _) };
        r
    }
    pub fn est_premier(&self) -> bool {
        unsafe { mpz_probab_prime_p(self.0.as_ref() as *const _, 25) != 0 }
    }
    pub fn fib(n: u64) -> Z {
        if n <= u32::MAX as u64 {
            let mut r = Z::new();
            unsafe { mpz_fib_ui(r.0.as_mut() as *mut _, n as c_ulong) };
            return r;
        }
        Self::fib_doublage(n)
    }
    fn fib_doublage(n: u64) -> Z {
        // Doublement rapide : de (F(k), F(k+1)) on tire (F(2k), F(2k+1)).
        let mut a = Z::i(0);
        let mut b = Z::i(1);
        for i in (0..64).rev() {
            let c = a.mul(&b.mul_u(2).add(&a));
            let d = a.mul(&a).add(&b.mul(&b));
            if (n >> i) & 1 == 1 { b = c.add(&d); a = d; } else { a = c; b = d; }
        }
        a
    }
    pub fn pow10(e: u64) -> Z {
        if e <= u32::MAX as u64 {
            let mut r = Z::new();
            unsafe { mpz_ui_pow_ui(r.0.as_mut() as *mut _, 10, e) };
            return r;
        }
        Z::i(10).pow_carres(e)
    }
    pub fn bit(&self, i: u64) -> bool {
        if i > u32::MAX as u64 { return false; }
        unsafe { mpz_tstbit(self.0.as_ref() as *const _, i) != 0 }
    }
    pub fn bits(&self) -> u64 {
        if self.sgn() == 0 { return 0; }
        self.chiffres_base(2) as u64
    }
    pub fn chiffres_base(&self, b: i32) -> usize {
        unsafe { mpz_sizeinbase(self.0.as_ref() as *const _, b) }
    }
}

impl Clone for Z {
    fn clone(&self) -> Z {
        let mut r = Z::new();
        unsafe { mpz_set(r.0.as_mut() as *mut _, self.0.as_ref() as *const _) };
        r
    }
}

impl Drop for Z {
    fn drop(&mut self) {
        unsafe { mpz_clear(self.0.as_mut() as *mut _) };
    }
}

pub struct Q(Box<MpqT>);

impl Q {
    pub fn new() -> Q {
        unsafe {
            let mut b = Box::new(MpqT {
                num: MpzT { alloc: 0, size: 0, d: std::ptr::null_mut() },
                den: MpzT { alloc: 0, size: 0, d: std::ptr::null_mut() },
            });
            mpq_init(&mut *b);
            Q(b)
        }
    }
    pub fn depuis_z(z: &Z) -> Q {
        let mut q = Q::new();
        unsafe { mpq_set_z(q.0.as_mut() as *mut _, z.0.as_ref() as *const _) };
        q
    }
    pub fn depuis_f64(d: f64) -> Q {
        let mut q = Q::new();
        unsafe { mpq_set_d(q.0.as_mut() as *mut _, d) };
        q
    }
    pub fn depuis_texte(s: &str) -> Option<Q> {
        let c = CString::new(s).ok()?;
        let mut q = Q::new();
        unsafe {
            if mpq_set_str(q.0.as_mut() as *mut _, c.as_ptr(), 10) == 0 {
                mpq_canonicalize(q.0.as_mut() as *mut _);
                Some(q)
            } else {
                None
            }
        }
    }
    pub fn canon(&mut self) {
        unsafe { mpq_canonicalize(self.0.as_mut() as *mut _) };
    }
    pub fn avec_num_den(num: &Z, den: &Z) -> Q {
        let mut q = Q::new();
        unsafe {
            mpz_set(&mut (*q.0.as_mut()).num as *mut MpzT, num.0.as_ref() as *const _);
            mpz_set(&mut (*q.0.as_mut()).den as *mut MpzT, den.0.as_ref() as *const _);
            mpq_canonicalize(q.0.as_mut() as *mut _);
        }
        q
    }
    pub fn neg(&self) -> Q {
        Q::avec_num_den(&self.numerateur().neg(), &self.denominateur())
    }
    pub fn numerateur(&self) -> Z {
        let mut r = Z::new();
        unsafe { mpz_set(r.0.as_mut() as *mut _, &(*self.0.as_ref()).num as *const MpzT) };
        r
    }
    pub fn denominateur(&self) -> Z {
        let mut r = Z::new();
        unsafe { mpz_set(r.0.as_mut() as *mut _, &(*self.0.as_ref()).den as *const MpzT) };
        r
    }
    pub fn en_f64(&self) -> f64 {
        unsafe { mpq_get_d(self.0.as_ref() as *const _) }
    }
    pub fn sgn(&self) -> i32 {
        // mpq_sgn est une macro gmp.h : signe du numérateur
        let taille = unsafe { (*(&(*self.0.as_ref()) as *const MpqT).cast::<MpzT>()).size };
        if taille < 0 { -1 } else if taille > 0 { 1 } else { 0 }
    }
    pub fn cmp(&self, a: &Q) -> i32 {
        unsafe { mpq_cmp(self.0.as_ref() as *const _, a.0.as_ref() as *const _) }
    }
    pub fn add(&self, a: &Q) -> Q {
        let mut r = Q::new();
        unsafe { mpq_add(r.0.as_mut() as *mut _, self.0.as_ref() as *const _, a.0.as_ref() as *const _) };
        r
    }
    pub fn sub(&self, a: &Q) -> Q {
        let mut r = Q::new();
        unsafe { mpq_sub(r.0.as_mut() as *mut _, self.0.as_ref() as *const _, a.0.as_ref() as *const _) };
        r
    }
    pub fn mul(&self, a: &Q) -> Q {
        let mut r = Q::new();
        unsafe { mpq_mul(r.0.as_mut() as *mut _, self.0.as_ref() as *const _, a.0.as_ref() as *const _) };
        r
    }
    pub fn div(&self, a: &Q) -> Q {
        let mut r = Q::new();
        unsafe { mpq_div(r.0.as_mut() as *mut _, self.0.as_ref() as *const _, a.0.as_ref() as *const _) };
        r
    }
}

impl Clone for Q {
    fn clone(&self) -> Q {
        let mut r = Q::new();
        unsafe { mpq_set(r.0.as_mut() as *mut _, self.0.as_ref() as *const _) };
        r
    }
}

impl Drop for Q {
    fn drop(&mut self) {
        unsafe { mpq_clear(self.0.as_mut() as *mut _) };
    }
}

/* ═══════════════ bibliothèque dynamique portative ═══════════════════════ */

#[cfg(windows)]
extern "system" {
    fn LoadLibraryA(nom: *const c_char) -> *mut c_void;
    fn GetProcAddress(h: *mut c_void, nom: *const c_char) -> *mut c_void;
    fn FreeLibrary(h: *mut c_void) -> i32;
}

pub unsafe fn ouvrir_dll(chemin: *const c_char, mode: c_int) -> *mut c_void {
    #[cfg(windows)] { unsafe { LoadLibraryA(chemin) } }
    #[cfg(not(windows))] { unsafe { dlopen(chemin, mode) } }
}

pub unsafe fn symbole_dll(h: *mut c_void, nom: *const c_char) -> *mut c_void {
    #[cfg(windows)] { unsafe { GetProcAddress(h, nom) } }
    #[cfg(not(windows))] { unsafe { dlsym(h, nom) } }
}

pub unsafe fn fermer_dll(h: *mut c_void) {
    #[cfg(windows)] { unsafe { FreeLibrary(h); } }
    #[cfg(not(windows))] { unsafe { dlclose(h); } }
}

/// Fins de ligne normalisées en \n : un fichier édité sous Windows arrive
/// en \r\n, et le lexeur doit voir une seule ligne par ligne.
pub fn normaliser_fins_de_ligne(source: &str) -> String {
    let b = source.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0usize;
    while i < b.len() {
        if b[i] == b'\r' {
            out.push(b'\n');
            if i + 1 < b.len() && b[i + 1] == b'\n' { i += 1; }
        } else {
            out.push(b[i]);
        }
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}
