//! OPT_ARITH — micro-banc isolant les causes des domaines arithmétiques
//! encore perdus au banc `bench/perf` (VAGUE 4, HEAD 31c9e74) :
//!   gcd (2,28× Rust), premiers (1,43×), factorial (1,16×).
//!
//! Deux usages :
//!   * `cargo test --release --test opt_arith`                      → garde-fous d'exactitude ;
//!   * `OPT_ARITH_BENCH=1 cargo test --release --test opt_arith -- --nocapture`
//!     → micro-banc (3 salves entrelacées, médianes, temps + allocations).
//!
//! On mesure SÉPARÉMENT la conversion Valeur↔BigInt, l'appel PGCD, le crible
//! et la factorielle : c'est ce qui permet d'attribuer l'écart du banc
//! « moteur » (appel, conversion) ou « calcul » (algorithme).

use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::Zero;
use rose::i128x::{factorielle_parallele, gcd_rapide, Gz};
use rose::run::{est_premier, pgcd, vers_z, Valeur};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

// ── compteur d'allocations (actif uniquement pendant les appels mesurés) ──
static ACTIF: AtomicBool = AtomicBool::new(false);
static N_ALLOCS: AtomicUsize = AtomicUsize::new(0);
static N_OCTETS: AtomicUsize = AtomicUsize::new(0);

struct Compteur;
unsafe impl GlobalAlloc for Compteur {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        if ACTIF.load(Ordering::Relaxed) {
            N_ALLOCS.fetch_add(1, Ordering::Relaxed);
            N_OCTETS.fetch_add(l.size(), Ordering::Relaxed);
        }
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) { unsafe { System.dealloc(p, l) } }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        if ACTIF.load(Ordering::Relaxed) {
            N_ALLOCS.fetch_add(1, Ordering::Relaxed);
            N_OCTETS.fetch_add(n, Ordering::Relaxed);
        }
        unsafe { System.realloc(p, l, n) }
    }
}
#[global_allocator]
static ALLOC: Compteur = Compteur;

#[derive(Clone, Copy)]
struct Mesure { micro_s: f64, allocs: usize, octets: usize }

fn mediane(mut v: Vec<f64>) -> f64 { v.sort_by(|a, b| a.total_cmp(b)); v[v.len() / 2] }
fn mediane_usize(mut v: Vec<usize>) -> usize { v.sort_unstable(); v[v.len() / 2] }

fn mesurer(f: &mut impl FnMut(), reps: usize) -> Mesure {
    f();
    let mut temps = Vec::with_capacity(3);
    let mut allocs = Vec::with_capacity(3);
    let mut octets = Vec::with_capacity(3);
    for _ in 0..3 {
        N_ALLOCS.store(0, Ordering::Relaxed);
        N_OCTETS.store(0, Ordering::Relaxed);
        ACTIF.store(true, Ordering::SeqCst);
        let t0 = Instant::now();
        for _ in 0..reps { f(); }
        let el = t0.elapsed().as_secs_f64();
        ACTIF.store(false, Ordering::SeqCst);
        temps.push(el * 1e6 / reps as f64);
        allocs.push(N_ALLOCS.load(Ordering::Relaxed) / reps);
        octets.push(N_OCTETS.load(Ordering::Relaxed) / reps);
    }
    Mesure { micro_s: mediane(temps), allocs: mediane_usize(allocs), octets: mediane_usize(octets) }
}

fn afficher(nom: &str, m: Mesure) {
    println!("{nom:<34} {:>11.3} µs  {:>7} allocs  {:>10} o",
             m.micro_s, m.allocs, m.octets);
}

// ── entrées déterministes, identiques au banc bench/perf ────────────────────
fn entrees_gcd() -> (BigInt, BigInt) {
    (("1234567890".repeat(100)).parse().unwrap(),
     ("9876543210".repeat(100)).parse().unwrap())
}
fn valeur_grande(x: &BigInt) -> Valeur {
    Valeur::Entier(0, Some(Rc::new(Gz::de_bigint(x.clone()))))
}

/// Crible d'Ératosthène (même forme que dans `factorielle_parallele` et que
/// dans le compilateur de facteurs premiers).
pub fn crible(n: usize) -> Vec<u64> {
    if n < 2 { return Vec::new(); }
    let mut est_premier = vec![true; n + 1];
    est_premier[0] = false;
    est_premier[1] = false;
    let mut i = 2usize;
    while i * i <= n {
        if est_premier[i] {
            let mut j = i * i;
            while j <= n { est_premier[j] = false; j += i; }
        }
        i += 1;
    }
    (2..=n).filter(|&p| est_premier[p]).map(|p| p as u64).collect()
}

/// Factorielle Rust de référence (num-bigint, la version utilisée par Rose).
fn factorielle_rust(n: u32) -> BigInt {
    let mut r = BigInt::from(1u8);
    for k in 2..=n { r *= k; }
    r
}

#[test]
fn exactitude_gcd_facade() {
    let (a, b) = entrees_gcd();
    let ga = Gz::de_bigint(a.clone());
    let gb = Gz::de_bigint(b.clone());
    let gr = pgcd(&ga, &gb);
    assert_eq!(gr.bref(), &a.gcd(&b));
    assert!(gr.bref().to_string().starts_with("9000000000900000000090000000009000000000"));
    // Le même PGCD via le chemin Valeur (façade stdlib) doit concorder.
    let va = valeur_grande(&a);
    let vb = valeur_grande(&b);
    let converted_a = vers_z(&va);
    let converted_b = vers_z(&vb);
    assert_eq!(pgcd(&converted_a, &converted_b).bref(), &a.gcd(&b));
    assert_eq!(gcd_rapide(&a, &b), a.gcd(&b));
}

#[test]
fn exactitude_crible_et_factorielle() {
    let p = crible(50_000);
    assert_eq!(p.len(), 5_133);
    assert_eq!(crible(3_000).len(), 430);
    assert_eq!(factorielle_parallele(3_000).bref(), &factorielle_rust(3_000));
    assert_eq!(factorielle_parallele(20).bref(), &factorielle_rust(20));
}

#[test]
fn exactitude_est_premier() {
    for n in 0..2000u64 {
        let z = Gz::de_u64(n);
        assert_eq!(est_premier(&z), crate::crible(2000).binary_search(&n).is_ok(), "n={n}");
    }
}

#[test]
fn banc() {
    if std::env::var("OPT_ARITH_BENCH").is_err() { return; }
    let (a, b) = entrees_gcd();
    let ga = Gz::de_bigint(a.clone());
    let gb = Gz::de_bigint(b.clone());
    println!("\n── OPT_ARITH : médianes de 3 salves × 3 passes ──");

    // 1. Conversion Valeur → BigInt : le coût payé à chaque appel de primitive.
    let va = valeur_grande(&a);
    let petit = Valeur::z(42);
    afficher("conversion Valeur→BigInt (i64)", mesurer(&mut || { black_box(vers_z(&petit)); }, 20_000));
    afficher("conversion Valeur→BigInt (1000ch)", mesurer(&mut || { black_box(vers_z(&va)); }, 2_000));

    // 2. Appel PGCD à tous les niveaux.
    afficher("pgcd() façade run (Gz)", mesurer(&mut || { black_box(pgcd(&ga, &gb)); }, 400));
    afficher("gcd_rapide (BigInt)", mesurer(&mut || { black_box(gcd_rapide(&a, &b)); }, 400));
    afficher("Rust a.gcd(&b) (num-bigint 0.5)", mesurer(&mut || { black_box(a.gcd(&b)); }, 400));

    // 3. Crible et primalité.
    afficher("crible n=3000", mesurer(&mut || { black_box(crible(3_000)); }, 100));
    afficher("crible n=50000", mesurer(&mut || { black_box(crible(50_000)); }, 10));
    let z100 = Gz::de_u64(49_999);
    afficher("est_premier(49999) Miller-Rabin", mesurer(&mut || { black_box(est_premier(&z100)); }, 2_000));

    // 4. Factorielle : calcul seul, sans conversion décimale.
    afficher("factorielle_parallele(3000)", mesurer(&mut || { black_box(factorielle_parallele(3_000)); }, 20));
    afficher("factorielle Rust (3000)", mesurer(&mut || { black_box(factorielle_rust(3_000)); }, 20));
    // Un produit bigint × petit entier, isolé de l'arbre de facteurs.
    let dep = factorielle_parallele(2_900).v();
    let gd = Gz::de_bigint(dep.clone());
    afficher("Gz::mul 2900! × u32", mesurer(&mut || { let r = Gz::neuf(); r.mul(&gd, &Gz::de_u64(2_901)); black_box(r.bref()); }, 200));
    afficher("BigInt × &BigInt(u32)", mesurer(&mut || { black_box(gd.bref() * &BigInt::from(2_901u32)); }, 200));
    afficher("BigInt × u32 (spécialisé)", mesurer(&mut || { black_box(gd.bref() * 2_901u32); }, 200));
    afficher("clone + *= u32 (en place)", mesurer(&mut || { let mut v = gd.bref().clone(); v *= 2_901u32; black_box(v); }, 200));
    afficher("chemin actuel: clone + mul", mesurer(&mut || { let x = gd.bref().clone(); let r = Gz::de_bigint(&x * &BigInt::from(2_901u32)); black_box(r.bref()); }, 200));
    afficher("chemin direct: sans clone", mesurer(&mut || { let r = Gz::de_bigint(gd.bref() * 2_901u32); black_box(r.bref()); }, 200));
    // VAGUE 4 — regrouper PLUSIEURS petits facteurs en un seul `u64` avant
    // de multiplier l'accumulateur : moins d'allocations et moins de passes
    // sur les limbs, le produit de 4 facteurs ≤ 3000^4 < 2^63 tient en u64.
    fn factorielle_groupee(n: u32, grp: u32) -> BigInt {
        let mut acc = BigInt::from(1u8);
        let mut k = 2u32;
        while k <= n {
            let mut p = 1u64;
            let fin = (k + grp - 1).min(n);
            while k <= fin { p *= k as u64; k += 1; }
            acc *= p;
        }
        acc
    }
    for &g in &[1u32, 2, 3, 4, 5] {
        assert_eq!(factorielle_groupee(3_000, g), factorielle_rust(3_000), "groupe {g}");
        let m = mesurer(&mut || { black_box(factorielle_groupee(3_000, g)); }, 10);
        println!("groupe g={g:<3} 3000! {:>9.1} µs (attendu égal)", m.micro_s);
    }
    // Crossover séquentiel (Rust-like) vs arbre de facteurs parallèle.
    for &n in &[1_000u32, 2_000, 3_000, 5_000, 10_000, 20_000] {
        let seq = mesurer(&mut || { black_box(factorielle_rust(n)); }, 6);
        let par = mesurer(&mut || { black_box(factorielle_parallele(n as u64)); }, 6);
        println!("crossover n={n:<6} séquentiel {:>9.1} µs | arbre {:>9.1} µs | {:.2}×",
                 seq.micro_s, par.micro_s, par.micro_s / seq.micro_s);
    }
}

/// Garde-fou : la médiane doit être calculée sur des mesures réelles.
#[test]
fn banc_medianes_non_vides() {
    let m = mesurer(&mut || { black_box(1u64 + 1); }, 10);
    assert!(m.micro_s > 0.0);
    assert_eq!(mediane_usize(vec![1, 2, 3]), 2);
}
