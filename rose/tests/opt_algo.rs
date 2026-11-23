//! OPT_ALGO — micro-banc de REMPLACEMENT D'ALGORITHMES (vague OPT_BIGINT).
//!
//! Deux usages :
//!   * `RUST_TEST_THREADS=1 cargo test --release --test opt_algo`
//!       → parité bit-à-bit de TOUS les candidats (centaines de valeurs aléatoires) ;
//!   * `OPT_ALGO_BENCH=1 RUST_TEST_THREADS=1 cargo test --release --test opt_algo -- --nocapture`
//!       → banc entrelacé, 3 médianes, candidat contre implantation actuelle et Rust.
//!
//! Les candidats sont des COPIES AUTONOMES : rien n'est intégré tant que le
//! banc n'a pas tranché. Les chiffres sont commentés dans docs/OPT_ALGOS.md.

use num_bigint::{BigInt, BigUint};
use num_integer::Integer;
use num_traits::{One, ToPrimitive, Zero};
use rose::i128x::{factorielle_parallele, gcd_rapide, Gz};
use rose::mul_rapide;
use std::hint::black_box;
use std::time::Instant;

fn mediane(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2]
}

fn bench3(f: &mut impl FnMut(), reps: usize) -> f64 {
    f();
    let mut t = Vec::with_capacity(3);
    for _ in 0..3 {
        let t0 = Instant::now();
        for _ in 0..reps { f(); }
        t.push(t0.elapsed().as_secs_f64() * 1e6 / reps as f64);
    }
    mediane(t)
}

fn lcg(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

fn grand_nb(chiffres: usize, graine: u64) -> BigInt {
    let mut s = String::with_capacity(chiffres);
    let mut x = graine | 1;
    for _ in 0..chiffres {
        x = lcg(&mut x);
        s.push(char::from(b'0' + ((x % 9) + 1) as u8));
    }
    s.parse().unwrap()
}

// ═══════════════════════════════════════════════════════════════════════════
// CANDIDAT A — PGCD : Lehmer à FENÊTRE LARGE (coefficients i128 jusqu'à 2^62,
// jusqu'à 96 étapes u64 par application de matrice, au lieu de 2^32/32).
// ═══════════════════════════════════════════════════════════════════════════

fn combinaison_limbs(a: &BigUint, b: &BigUint, ca: i128, cb: i128) -> BigUint {
    let ta: BigUint = a * BigUint::from(ca.unsigned_abs());
    let tb: BigUint = b * BigUint::from(cb.unsigned_abs());
    if (ca >= 0 && cb >= 0) || (ca < 0 && cb < 0) { ta + tb }
    else if ta >= tb { ta - tb } else { tb - ta }
}

fn gcd_lehmer_large(a: &BigInt, b: &BigInt, lim: i128, etapes: usize) -> BigInt {
    let (mut u, mut v) = (a.magnitude().clone(), b.magnitude().clone());
    if u < v { std::mem::swap(&mut u, &mut v); }
    if v.bits() <= 1024 { return BigInt::from(u.gcd(&v)); }
    let seuil: BigUint = BigUint::one() << 128u32;
    while v >= seuil {
        let sh = (u.bits() as usize - 64) as u32;
        let uu = (&u >> sh).to_u64().unwrap_or(0);
        let vv = (&v >> sh).to_u64().unwrap_or(0);
        if uu == 0 || vv == 0 {
            let r = &u % &v;
            u = v; v = r;
            continue;
        }
        let (mut aa1, mut aa2): (i128, i128) = (1, 0);
        let (mut bb1, mut bb2): (i128, i128) = (0, 1);
        let (mut x, mut y) = (uu, vv);
        for _ in 0..etapes {
            let q = (x / y) as i128;
            let na1 = aa1 - q * bb1;
            let na2 = aa2 - q * bb2;
            if na1.checked_abs().map_or(true, |z| z > lim)
                || na2.checked_abs().map_or(true, |z| z > lim) { break; }
            let t = x - y * q as u64;
            aa1 = bb1; aa2 = bb2;
            bb1 = na1; bb2 = na2;
            x = y; y = t;
            if y == 0 { break; }
        }
        let nu = combinaison_limbs(&u, &v, aa1, aa2);
        let nv = combinaison_limbs(&u, &v, bb1, bb2);
        if nv >= v {
            let r = &u % &v;
            u = v; v = r;
        } else {
            u = nu; v = nv;
            if u < v { std::mem::swap(&mut u, &mut v); }
        }
    }
    BigInt::from(u.gcd(&v))
}

/// Jebelean : on arrête la fenêtre u64 dès que le reste `y` devient petit —
/// en dessous, les quotients des 64 bits de tête ne sont plus ceux des
/// grands nombres, et la matrice ne fait plus progresser (repli coûteux).
fn gcd_lehmer_guarde(a: &BigInt, b: &BigInt, lim: i128, etapes: usize, min_y: u64) -> BigInt {
    let (mut u, mut v) = (a.magnitude().clone(), b.magnitude().clone());
    if u < v { std::mem::swap(&mut u, &mut v); }
    if v.bits() <= 1024 { return BigInt::from(u.gcd(&v)); }
    let seuil: BigUint = BigUint::one() << 128u32;
    while v >= seuil {
        let sh = (u.bits() as usize - 64) as u32;
        let uu = (&u >> sh).to_u64().unwrap_or(0);
        let vv = (&v >> sh).to_u64().unwrap_or(0);
        if uu == 0 || vv == 0 {
            let r = &u % &v;
            u = v; v = r;
            continue;
        }
        let (mut aa1, mut aa2): (i128, i128) = (1, 0);
        let (mut bb1, mut bb2): (i128, i128) = (0, 1);
        let (mut x, mut y) = (uu, vv);
        for _ in 0..etapes {
            if x < min_y || y < min_y { break; }
            let q = (x / y) as i128;
            let na1 = aa1 - q * bb1;
            let na2 = aa2 - q * bb2;
            if na1.checked_abs().map_or(true, |z| z > lim)
                || na2.checked_abs().map_or(true, |z| z > lim) { break; }
            let t = x - y * q as u64;
            aa1 = bb1; aa2 = bb2;
            bb1 = na1; bb2 = na2;
            x = y; y = t;
            if y == 0 { break; }
        }
        let nu = combinaison_limbs(&u, &v, aa1, aa2);
        let nv = combinaison_limbs(&u, &v, bb1, bb2);
        if nv >= v {
            let r = &u % &v;
            u = v; v = r;
        } else {
            u = nu; v = nv;
            if u < v { std::mem::swap(&mut u, &mut v); }
        }
    }
    BigInt::from(u.gcd(&v))
}

// ═══════════════════════════════════════════════════════════════════════════
// CANDIDAT B — PRIMALITÉ u64 : 7 témoins déterministes (Sinclair), au lieu
// de 12. Même crible de petits facteurs, mêmes mulmod.
// ═══════════════════════════════════════════════════════════════════════════

const PETITS: [u64; 12] = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37];
const TEMOINS7: [u64; 7] = [2, 325, 9375, 28178, 450775, 9780504, 1795265022];

#[inline]
fn mulmod_u64(a: u64, b: u64, m: u64) -> u64 {
    ((a as u128 * b as u128) % m as u128) as u64
}
fn mulmod_pow_u64(mut base: u64, mut e: u64, m: u64) -> u64 {
    let mut r = 1u64 % m;
    base %= m;
    while e > 0 {
        if e & 1 == 1 { r = mulmod_u64(r, base, m); }
        base = mulmod_u64(base, base, m);
        e >>= 1;
    }
    r
}
fn mr_temoins(n: u64, temoins: &[u64]) -> bool {
    if n < 2 { return false; }
    for &p in &PETITS {
        if n == p { return true; }
        if n % p == 0 { return false; }
    }
    let mut d = n - 1;
    let mut s = 0u32;
    while d & 1 == 0 { d >>= 1; s += 1; }
    't: for &a in temoins {
        let a = a % n;
        if a == 0 { continue; }
        let mut x = mulmod_pow_u64(a, d, n);
        if x == 1 || x == n - 1 { continue; }
        for _ in 0..s - 1 {
            x = mulmod_u64(x, x, n);
            if x == n - 1 { continue 't; }
        }
        return false;
    }
    true
}
fn mr7(n: u64) -> bool { mr_temoins(n, &TEMOINS7) }
fn mr12(n: u64) -> bool { mr_temoins(n, &PETITS) }

// ═══════════════════════════════════════════════════════════════════════════
// CANDIDAT C — PUISSANCE : exponentiation par FENÊTRE k-aire (table complète),
// contre l'exponentiation binaire actuelle.
// ═══════════════════════════════════════════════════════════════════════════

fn pow_binaire(base: &BigInt, mut e: u64) -> BigInt {
    if base.is_one() { return BigInt::one(); }
    let mut b = base.clone();
    let mut r = BigInt::one();
    while e > 0 {
        if e & 1 == 1 { r *= &b; }
        e >>= 1;
        if e > 0 { b = &b * &b; }
    }
    r
}

fn pow_fenetre(base: &BigInt, e: u64, w: u32) -> BigInt {
    if e == 0 || base.is_one() { return BigInt::one(); }
    let k = 1usize << w;
    let mut tab: Vec<BigInt> = vec![BigInt::one(); k];
    tab[1] = base.clone();
    for i in 2..k {
        tab[i] = if i % 2 == 0 { &tab[i / 2] * &tab[i / 2] }
                 else { &tab[i - 1] * base };
    }
    let nbits = 64 - e.leading_zeros();
    let premier = (nbits as u32) % w;
    let mut pos = nbits as u32;
    let mut r = BigInt::one();
    let mut demarre = false;
    while pos > 0 {
        let len = if !demarre && premier > 0 { premier } else { w };
        pos -= len;
        let chiffre = ((e >> pos) & ((1u64 << len) - 1)) as usize;
        if !demarre {
            r = tab[chiffre].clone();
            demarre = true;
        } else {
            for _ in 0..len { r = &r * &r; }
            if chiffre != 0 { r = &r * &tab[chiffre]; }
        }
    }
    r
}

// ═══════════════════════════════════════════════════════════════════════════
// CANDIDAT D — FACTORIELLE : arbre binaire équilibré de produits de groupes
// u64 (Borwein), SANS décomposition de Legendre, pour le régime n ≤ 4000.
// ═══════════════════════════════════════════════════════════════════════════

fn fact_groupee(n: u64) -> BigInt {
    let mut acc = BigInt::one();
    let mut p = 1u64;
    for k in 2..=n {
        match p.checked_mul(k) {
            Some(np) => p = np,
            None => { acc *= p; p = k; }
        }
    }
    if p != 1 { acc *= p; }
    acc
}

/// Regroupement à base u128 : jusqu'à ~10 facteurs de 3000 dans un seul
/// limbs machine virtuel, donc ~2× moins de passes BigInt.
fn fact_groupee_u128(n: u64) -> BigInt {
    let mut acc = BigInt::one();
    let mut p = 1u128;
    for k in 2..=n {
        match p.checked_mul(k as u128) {
            Some(np) => p = np,
            None => { acc *= BigInt::from(p); p = k as u128; }
        }
    }
    if p != 1 { acc *= BigInt::from(p); }
    acc
}

fn fact_arbre(n: u64) -> BigInt {
    if n < 2 { return BigInt::one(); }
    let mut v: Vec<BigInt> = Vec::new();
    let mut p = 1u64;
    for k in 2..=n {
        match p.checked_mul(k) {
            Some(np) => p = np,
            None => { v.push(BigInt::from(p)); p = k; }
        }
    }
    if p != 1 { v.push(BigInt::from(p)); }
    while v.len() > 1 {
        let m = v.len();
        let mut i = 0usize;
        while i + 1 < m {
            let a = std::mem::take(&mut v[i]);
            let prod = mul_rapide::mul(&a, &v[i + 1]);
            v[i / 2] = prod;
            i += 2;
        }
        if m % 2 == 1 { v[(m - 1) / 2] = std::mem::take(&mut v[m - 1]); }
        v.truncate((m + 1) / 2);
    }
    v.pop().unwrap()
}

// ═══════════════════════════════════════════════════════════════════════════
// PARITÉ — des centaines de valeurs aléatoires, bit-à-bit.
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn parite_pgcd_lehmer_large() {
    let mut s = 0x9E37_79B9_7F4A_7C15u64;
    for i in 0..240 {
        let a = grand_nb(80 + (i % 9) * 90, lcg(&mut s));
        let b = grand_nb(80 + ((i * 7) % 11) * 80, lcg(&mut s));
        let ref1 = a.gcd(&b);
        assert_eq!(gcd_rapide(&a, &b), ref1, "actuel, cas {i}");
        for (lim, et) in [(1i128 << 32, 32usize), (1i128 << 48, 64), (1i128 << 62, 96)] {
            assert_eq!(gcd_lehmer_large(&a, &b, lim, et), ref1, "lim={lim} et={et}, cas {i}");
        }
        for (lim, et, my) in [(1i128 << 62, 96usize, 1u64 << 32), (1i128 << 62, 96, 1u64 << 40),
                              (1i128 << 48, 64, 1u64 << 32)] {
            assert_eq!(gcd_lehmer_guarde(&a, &b, lim, et, my), ref1, "garde my={my}, cas {i}");
        }
    }
    for (a, b) in [(BigInt::zero(), BigInt::from(7u64)),
                   (BigInt::from(1u64) << 2000, BigInt::from(3u64) << 1500),
                   (BigInt::from(-1i64) * ((BigInt::from(1u64) << 400) - BigInt::from(1u64)),
                    BigInt::from(1u64) << 300)] {
        let r = a.gcd(&b);
        assert_eq!(gcd_lehmer_large(&a, &b, 1i128 << 62, 96), r);
    }
}

#[test]
fn parite_primalite_7_temoins() {
    for n in 0u64..100_000 {
        let z = Gz::de_u64(n);
        let attendu = z.est_premier();
        assert_eq!(mr7(n), attendu, "mr7({n})");
        assert_eq!(mr12(n), attendu, "mr12({n})");
    }
    let mut s = 0xD1B5_4A32_D192_ED03u64;
    for n in [0u64, 1, 2, 3, 4, u64::MAX, u64::MAX - 1, 1u64 << 63,
              3_215_031_751, 3_474_749_660_383, 3825123056546413051]
        .into_iter().chain((0..400).map(|_| lcg(&mut s))) {
        let z = Gz::de_u64(n);
        assert_eq!(mr7(n), z.est_premier(), "mr7({n})");
    }
}

#[test]
fn parite_puissance_fenetre() {
    let mut s = 0x1234_5678_9ABC_DEF0u64;
    for i in 0..120 {
        let base = BigInt::from(lcg(&mut s) % 100_000 + 2);
        let e = (lcg(&mut s) % 20_000) as u64 + 1;
        let attendu = base.pow(e as u32);
        assert_eq!(pow_binaire(&base, e), attendu, "binaire {i}");
        for w in [2u32, 3, 4, 5, 6] {
            let got = pow_fenetre(&base, e, w);
            if got != attendu {
                panic!("fenetre w={w} cas {i}: base={base} e={e}");
            }
        }
    }
}

#[test]
fn parite_integration_i128x() {
    let mut s = 0xA5A5_1234_5678_9ABCu64;
    for i in 0..200 {
        let base = (lcg(&mut s) % 2_000_000) as i64 - 1_000_000;
        let e = (lcg(&mut s) % 12_000) as u64;
        let attendu = BigInt::from(base).pow(e as u32);
        let z = Gz::de_i64(base);
        let r = Gz::neuf();
        r.pow_u64(&z, e);
        assert_eq!(r.v(), attendu, "Gz::pow_u64 base={base} e={e} cas {i}");
        if base >= 0 {
            let r2 = Gz::neuf();
            unsafe { rose::i128x::gmp_ui_pow_ui(r2.p(), base as u64, e); }
            assert_eq!(r2.v(), attendu, "gmp_ui_pow_ui base={base} e={e}");
        }
    }
    for e in [0u64, 1, 2, 3, 7, 8, 9, 15, 16, 17, 255, 256, 257, 4095, 65535, 65536] {
        let z = Gz::de_i64(-12345);
        let r = Gz::neuf();
        r.pow_u64(&z, e);
        assert_eq!(r.v(), BigInt::from(-12345i64).pow(e as u32), "e={e}");
    }
    for n in 0u64..20_000 {
        let z = Gz::de_u64(n);
        assert_eq!(z.est_premier(), mr12(n), "est_premier({n})");
    }
    for n in [u64::MAX, u64::MAX - 1, 3_825_123_056_546_413_051, 1u64 << 62] {
        let z = Gz::de_u64(n);
        assert_eq!(z.est_premier(), mr12(n), "est_premier({n})");
    }
}

#[test]
fn parite_factorielle_arbre() {
    for n in 0u64..=300 {
        let attendu = fact_groupee(n);
        assert_eq!(fact_arbre(n), attendu, "arbre {n}!");
    }
    for n in [1000u64, 2000, 3000, 4000] {
        let attendu = fact_groupee(n);
        assert_eq!(fact_arbre(n), attendu, "arbre {n}!");
        assert_eq!(fact_groupee_u128(n), attendu, "u128 {n}!");
        assert_eq!(factorielle_parallele(n).v(), attendu, "actuel {n}!");
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// BANC
// ═══════════════════════════════════════════════════════════════════════════

fn bench_pgcd() {
    println!("\n=== PGCD (1000 chiffres, banc bench/perf) ===");
    let a = grand_nb(1000, 0x1234_5678);
    let b = grand_nb(1000, 0x89AB_CDEF);
    println!("  Rust num-bigint Stein : {:>10.2} µs", bench3(&mut || { black_box(a.gcd(&b)); }, 30));
    println!("  Rose actuel Lehmer 2^32/32 : {:>7.2} µs", bench3(&mut || { black_box(gcd_rapide(&a, &b)); }, 30));
    for (lim, et) in [(1i128 << 32, 32usize), (1i128 << 48, 64), (1i128 << 62, 64), (1i128 << 62, 96)] {
        println!("  candidat lim=2^{} et={} : {:>14.2} µs",
                 (127 - lim.leading_zeros()) as i32, et,
                 bench3(&mut || { black_box(gcd_lehmer_large(&a, &b, lim, et)); }, 30));
    }
    for (bits, et, my) in [(40usize, 32usize, 1u64 << 32), (48, 32, 1u64 << 32),
                           (62, 96, 1u64 << 32), (62, 96, 1u64 << 40), (48, 64, 1u64 << 32)] {
        println!("  garde lim=2^{bits} et={et} my=2^{} : {:>10.2} µs",
                 63 - my.leading_zeros(),
                 bench3(&mut || { black_box(gcd_lehmer_guarde(&a, &b, 1i128 << bits, et, my)); }, 30));
    }
}

fn bench_primalite() {
    println!("\n=== PRIMALITÉ u64 (mélange premiers/composés sans petits facteurs) ===");
    let mut s = 0xFEED_FACE_CAFE_BEEFu64;
    let suite: Vec<u64> = (0..256).map(|_| lcg(&mut s) | 1).collect();
    println!("  Rose actuel 12 témoins : {:>10.2} µs / 256 appels",
             bench3(&mut || { for &n in &suite { black_box(mr12(n)); } }, 40));
    println!("  candidat 7 témoins     : {:>10.2} µs / 256 appels",
             bench3(&mut || { for &n in &suite { black_box(mr7(n)); } }, 40));
    println!("  Rose actuel 12 témoins (0..50k) : {:>7.2} µs",
             bench3(&mut || { for n in 0..50_000u64 { black_box(mr12(n)); } }, 8));
    println!("  candidat 7 témoins (0..50k)     : {:>7.2} µs",
             bench3(&mut || { for n in 0..50_000u64 { black_box(mr7(n)); } }, 8));
}

fn bench_puissance() {
    println!("\n=== PUISSANCE (base géante, exposant u64) ===");
    for (base_s, e) in [(123_456_789u64, 5000u64), (123_456_789, 20_000)] {
        let base = BigInt::from(base_s);
        let reps = if e == 5000 { 20 } else { 8 };
        println!("  base={base_s} e={e}");
        println!("    binaire Rust pow     : {:>10.2} µs", bench3(&mut || { black_box(base.pow(e as u32)); }, reps));
        println!("    binaire Rose         : {:>10.2} µs", bench3(&mut || { black_box(pow_binaire(&base, e)); }, reps));
        for w in [2u32, 3, 4, 5, 6] {
            println!("    fenêtre w={w}          : {:>10.2} µs",
                     bench3(&mut || { black_box(pow_fenetre(&base, e, w)); }, reps));
        }
    }
}

fn bench_factorielle() {
    println!("\n=== FACTORIELLE (régime n ≤ 4000, sans Legendre) ===");
    for n in [1000u64, 2000, 3000, 4000] {
        let reps = if n <= 1000 { 60 } else if n <= 2000 { 30 } else { 15 };
        println!("  n={n}");
        println!("    groupes u64 (actuel) : {:>10.2} µs",
                 bench3(&mut || { black_box(fact_groupee(n)); }, reps));
        println!("    groupes u128         : {:>10.2} µs",
                 bench3(&mut || { black_box(fact_groupee_u128(n)); }, reps));
        println!("    arbre binaire        : {:>10.2} µs",
                 bench3(&mut || { black_box(fact_arbre(n)); }, reps));
        if n <= 3000 {
            println!("    factorielle_parallele: {:>10.2} µs",
                     bench3(&mut || { black_box(factorielle_parallele(n)); }, reps));
        }
    }
}

fn bench_integre() {
    println!("\n=== SURFACE INTÉGRÉE (après remplacement) ===");
    let base = BigInt::from(123_456_789u64);
    let z = Gz::de_bigint(base.clone());
    for e in [5000u64, 20_000] {
        let r = Gz::neuf();
        let t_new = bench3(&mut || { black_box(r.pow_u64(&z, e)); black_box(r.bref().bits()); }, 20);
        let t_old = bench3(&mut || {
            r.set_v(pow_binaire(&base, e));
            black_box(r.bref().bits());
        }, 20);
        println!("  Gz::pow_u64 e={e:<6} : ancien(binaire) {:>9.2} µs → intégré {:>9.2} µs ({:.2}×)",
                 t_old, t_new, t_old / t_new);
    }
    let mut s = 0xFEED_FACE_CAFE_BEEFu64;
    let suite: Vec<u64> = (0..256).map(|_| lcg(&mut s) | 1).collect();
    let t_new = bench3(&mut || { for &n in &suite { black_box(Gz::de_u64(n).est_premier()); } }, 40);
    let t_old = bench3(&mut || { for &n in &suite { black_box(mr12(n)); } }, 40);
    println!("  Gz::est_premier (256 u64) : ancien(12 bases) {:>9.2} µs → intégré {:>9.2} µs ({:.2}×)",
             t_old, t_new, t_old / t_new);
}

#[test]
fn banc() {
    if std::env::var("OPT_ALGO_BENCH").as_deref() != Ok("1") { return; }
    bench_pgcd();
    bench_primalite();
    bench_puissance();
    bench_factorielle();
    bench_integre();
}
