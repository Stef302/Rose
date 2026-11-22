//! OPT_BIGINT — banc reproductible et garde-fous d'exactitude pour les
//! mathématiques exactes (BigInt, rationnels, irrationnels) du moteur Rose.
//!
//! Deux usages :
//!   * `cargo test --release --test opt_bigint` → tests d'exactitude rapides ;
//!   * `OPT_BIGINT_BENCH=1 cargo test --release --test opt_bigint -- --nocapture`
//!     → banc (3 passes entrelacées, médianes, temps + allocations + octets).
//!     Le pic RSS du processus se lit avec `/usr/bin/time -v`.
//!
//! Protocole : même cœur (`taskset -c …`), mêmes entrées déterministes,
//!的抗-optimisation par `black_box`, compteur d'allocations actif UNIQUEMENT
//! pendant les appels mesurés. Aucun résultat n'est inventé : toutes les
//! médianes sont calculées sur des mesures réelles.

use num_bigint::{BigInt, BigUint};
use num_integer::Integer;
use num_traits::{One, ToPrimitive, Zero};
use rose::i128x::{
    gmp_gcd, gmp_pow_ui, gmp_ui_pow_ui, gmpq_add, gmpq_cmp, gmpq_div, gmpq_mul,
    gmpq_sub, Gq, Gz,
};
use rose::irrationnel::calcul_racine;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

// ── compteur d'allocations ──────────────────────────────────────────────────

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
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
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

// ── harnais de mesure ───────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct Mesure {
    micro_s: f64,
    allocs: usize,
    octets: usize,
}

fn mediane(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2]
}

fn mediane_usize(mut v: Vec<usize>) -> usize {
    v.sort_unstable();
    v[v.len() / 2]
}

fn mesurer(f: &mut impl FnMut(), reps: usize) -> Mesure {
    f(); // échauffement (hors mesure)
    let mut temps = Vec::with_capacity(3);
    let mut allocs = Vec::with_capacity(3);
    let mut octets = Vec::with_capacity(3);
    for _ in 0..3 {
        N_ALLOCS.store(0, Ordering::Relaxed);
        N_OCTETS.store(0, Ordering::Relaxed);
        ACTIF.store(true, Ordering::SeqCst);
        let t0 = Instant::now();
        for _ in 0..reps {
            f();
        }
        let elapse = t0.elapsed().as_secs_f64();
        ACTIF.store(false, Ordering::SeqCst);
        temps.push(elapse * 1e6 / reps as f64);
        allocs.push(N_ALLOCS.load(Ordering::Relaxed) / reps);
        octets.push(N_OCTETS.load(Ordering::Relaxed) / reps);
    }
    Mesure {
        micro_s: mediane(temps),
        allocs: mediane_usize(allocs),
        octets: mediane_usize(octets),
    }
}

fn afficher(nom: &str, rose: Mesure, rust: Option<Mesure>, extra: Option<(&str, Mesure)>) {
    print!("{nom:<22} Rose {:>11.2} µs  {:>7} allocs  {:>10} o", rose.micro_s, rose.allocs, rose.octets);
    if let Some(r) = rust {
        print!("   | Rust {:>11.2} µs  ratio {:>7.2}×", r.micro_s, rose.micro_s / r.micro_s);
    }
    if let Some((n, m)) = extra {
        print!("   | {n} {:>11.2} µs  {:>7} allocs", m.micro_s, m.allocs);
    }
    println!();
}

/// Entier décimal déterministe de `chiffres` chiffres (xorshift64*, jamais zéro en tête).
fn grand_nb(chiffres: usize, graine: u64) -> BigInt {
    let mut s = String::with_capacity(chiffres);
    let mut x = graine | 1;
    for _ in 0..chiffres {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        // 1..=9 : jamais de zéro en tête, jamais de chiffre invalide.
        s.push(char::from(b'0' + ((x % 9) + 1) as u8));
    }
    s.parse().unwrap()
}

// ── candidat indépendant : PGCD de Lehmer hybride (utilisé seulement comme
//    point de comparaison du banc ; l'implémentation intégrée est dans i128x.rs) ──

/// Transforme `ca*a + cb*b` en BigUint (valeur absolue), coefficients signés.
fn combinaison(a: &BigUint, b: &BigUint, ca: i128, cb: i128) -> BigUint {
    // Les coefficients sont plafonnés à 2^62 par l'appelant : u64 suffit.
    let ta: BigUint = a * (ca.unsigned_abs() as u64);
    let tb: BigUint = b * (cb.unsigned_abs() as u64);
    if (ca >= 0 && cb >= 0) || (ca < 0 && cb < 0) {
        ta + tb
    } else if ta >= tb {
        ta - tb
    } else {
        tb - ta
    }
}

pub fn gcd_lehmer_candidate(a: &BigInt, b: &BigInt) -> BigInt {
    let (mut u, mut v) = (a.magnitude().clone(), b.magnitude().clone());
    if u < v {
        std::mem::swap(&mut u, &mut v);
    }
    // Seuil : sous 2^128, num-bigint (Stein) est imbattable.
    let seuil: BigUint = BigUint::one() << 128u32;
    while v >= seuil {
        let sh = (u.bits() as usize - 64) as u32;
        let uu = (&u >> sh).to_u64().unwrap_or(0);
        let vv = (&v >> sh).to_u64().unwrap_or(0);
        if uu == 0 || vv == 0 {
            // Écart de taille trop grand : un vrai pas d'Euclide remet à niveau.
            let r = &u % &v;
            u = v;
            v = r;
            continue;
        }
        // Lignes de la matrice unimodulaire : (x, y) = M·(u, v).
        // Un pas d'Euclide sur (x,y) : (x,y) ← (y, x−q·y), donc
        // la ligne de x devient l'ancienne ligne de y, et la ligne de y
        // devient (ligne_x − q·ligne_y). Le déterminant reste ±1 :
        // le PGCD est INVARIANT, même si q approché est « faux ».
        let (mut aa1, mut aa2): (i128, i128) = (1, 0);
        let (mut bb1, mut bb2): (i128, i128) = (0, 1);
        let (mut x, mut y) = (uu, vv);
        for _ in 0..24 {
            let q = (x / y) as i128;
            // q ≤ u64::MAX et |coeff| ≤ 2^62 : le produit tient dans i128 (2^126).
            let na1 = aa1 - q * bb1;
            let na2 = aa2 - q * bb2;
            let lim = 1i128 << 62;
            if na1.abs() > lim || na2.abs() > lim { break; }
            let t = x - y * q as u64; // q·y ≤ x ≤ u64::MAX : pas de débordement
            aa1 = bb1; aa2 = bb2;
            bb1 = na1; bb2 = na2;
            x = y;
            y = t;
            if y == 0 { break; }
        }
        let nu = combinaison(&u, &v, aa1, aa2);
        let nv = combinaison(&u, &v, bb1, bb2);
        if nv >= v {
            let r = &u % &v;
            u = v;
            v = r;
        } else {
            u = nu;
            v = nv;
            if u < v { std::mem::swap(&mut u, &mut v); }
        }
    }
    BigInt::from(u.gcd(&v))
}

// ── banc ────────────────────────────────────────────────────────────────────

#[test]
fn banc() {
    if std::env::var("OPT_BIGINT_BENCH").is_err() {
        return;
    }
    println!("\n=== BANC OPT_BIGINT (3 passes, médianes, black_box) ===");

    // 1. PGCD sur 1 000 chiffres décimaux.
    {
        let a = grand_nb(1000, 0x9E37_79B9_7F4A_7C15);
        let b = grand_nb(1000, 0xD1B5_4A32_D192_ED03);
        let ga = Gz::de_bigint(a.clone());
        let gb = Gz::de_bigint(b.clone());
        let gr = Gz::neuf();
        let rose = mesurer(&mut || unsafe { gmp_gcd(gr.p(), ga.p(), gb.p()); black_box(gr.bref()); }, 20);
        let rust = mesurer(&mut || { black_box(a.gcd(&b)); }, 20);
        let cand = mesurer(&mut || { black_box(gcd_lehmer_candidate(&a, &b)); }, 20);
        let direct = mesurer(&mut || { black_box(rose::i128x::gcd_rapide(&a, &b)); }, 20);
        afficher("PGCD 1000 chiffres", rose, Some(rust), Some(("Lehmer", cand)));
        afficher("PGCD gcd_rapide", direct, None, None);
        assert_eq!(gr.bref(), &a.gcd(&b));
    }

    // 2. Puissances 2^10000.
    {
        let r = Gz::neuf();
        let deux = Gz::de_u64(2);
        let rose_ui = mesurer(&mut || unsafe { gmp_ui_pow_ui(r.p(), 2, 10000); black_box(r.bref()); }, 200);
        let rose_pow = mesurer(&mut || unsafe { gmp_pow_ui(r.p(), deux.p(), 10000); black_box(r.bref()); }, 100);
        let rust_pow = mesurer(&mut || { black_box(BigInt::from(2u8).pow(10000)); }, 200);
        let rust_shift = mesurer(&mut || { black_box(BigInt::from(1u8) << 10000); }, 200);
        afficher("2^10000 gmp_ui_pow_ui", rose_ui, Some(rust_pow), Some(("gmp_pow_ui", rose_pow)));
        afficher("2^10000 (réf. <<)", rose_ui, Some(rust_shift), None);
        assert_eq!(r.bref(), &(BigInt::one() << 10000));
    }

    // 3. Factorielle 5000 (arbre de produits Rose vs produit séquentiel Rust).
    {
        let rose = mesurer(&mut || { black_box(rose::i128x::factorielle_parallele(5000)); }, 3);
        let rust = mesurer(&mut || {
            let mut f = BigInt::one();
            for k in 2..=5000 { f *= k; }
            black_box(f);
        }, 3);
        afficher("Factorielle 5000", rose, Some(rust), None);
    }

    // 4. Rationnels (addition + réduction sur 120 chiffres).
    {
        let na = grand_nb(120, 0x0123_4567_89AB_CDEF);
        let da = grand_nb(120, 0xFEDC_BA98_7654_3210);
        let nb = grand_nb(120, 0xCAFE_BABE_0123_4567);
        let db = grand_nb(120, 0x0F0F_F0F0_1234_5678);
        let qa = Gq::neuf();
        let qb = Gq::neuf();
        let qr = Gq::neuf();
        unsafe {
            (*qa.p()).num.v = na.clone(); (*qa.p()).den.v = da.clone();
            (*qb.p()).num.v = nb.clone(); (*qb.p()).den.v = db.clone();
        }
        let rose = mesurer(&mut || unsafe { gmpq_add(qr.p(), qa.p(), qb.p()); black_box(qr.num_bigint()); }, 300);
        let rust = mesurer(&mut || {
            let n = &na * &db + &nb * &da;
            let d = &da * &db;
            let g = n.gcd(&d);
            black_box((&n / &g, &d / &g));
        }, 300);
        afficher("Rationnels add 120ch", rose, Some(rust), None);

        // les autres opérations rationnelles, mesurées sans référence (exactitude testée ailleurs)
        let add = rose;
        let mul = mesurer(&mut || unsafe { gmpq_mul(qr.p(), qa.p(), qb.p()); black_box(qr.num_bigint()); }, 300);
        let div = mesurer(&mut || unsafe { gmpq_div(qr.p(), qa.p(), qb.p()); black_box(qr.num_bigint()); }, 300);
        let sub = mesurer(&mut || unsafe { gmpq_sub(qr.p(), qa.p(), qb.p()); black_box(qr.num_bigint()); }, 300);
        afficher("Rationnels mul 120ch", mul, None, None);
        afficher("Rationnels div 120ch", div, None, None);
        afficher("Rationnels sub 120ch", sub, None, None);
        assert!(add.micro_s > 0.0);
    }

    // 5. Racine carrée √2 à 1 000 décimales.
    {
        let deux = BigInt::from(2u8);
        let rose = mesurer(&mut || { black_box(calcul_racine(&deux, 1000)); }, 10);
        let a = &deux * (BigInt::from(10u8).pow(2000));
        let rust = mesurer(&mut || { black_box(a.sqrt()); }, 10);
        afficher("Racine √2 1000 déc.", rose, Some(rust), None);
    }

    // 6. Comparaisons (rationnels 200 chiffres + entiers 1 000 chiffres).
    {
        let na = grand_nb(200, 0xDEAD_BEEF_0123_4567);
        let da = grand_nb(200, 0x1234_5678_9ABC_DEF0);
        let nb = grand_nb(200, 0x0BAD_F00D_1357_9BDF);
        let db = grand_nb(200, 0xFEED_FACE_0246_8ACE);
        let qa = Gq::neuf();
        let qb = Gq::neuf();
        unsafe {
            (*qa.p()).num.v = na.clone(); (*qa.p()).den.v = da.clone();
            (*qb.p()).num.v = nb.clone(); (*qb.p()).den.v = db.clone();
        }
        let rose = mesurer(&mut || unsafe { black_box(gmpq_cmp(qa.p(), qb.p())); }, 3000);
        let rust = mesurer(&mut || { black_box((&na * &db).cmp(&(&nb * &da))); }, 3000);
        afficher("Comparaison rationnels", rose, Some(rust), None);
    }
}

// ── exactitude ──────────────────────────────────────────────────────────────

#[test]
fn exactitude_gcd_lehmer() {
    let graine = 0x1234_5678_9ABC_DEF0u64;
    for chiffres in [1usize, 3, 20, 40, 90, 300, 1000] {
        for k in 0..8u64 {
            let aa = grand_nb(chiffres, graine.wrapping_add(k));
            let bb = grand_nb(chiffres, graine.wrapping_add(100 + k));
            assert_eq!(gcd_lehmer_candidate(&aa, &bb), aa.gcd(&bb), "chiffres={chiffres} k={k}");
        }
    }
    // Diviseur commun massif : le chemin Lehmer doit conserver le facteur.
    let base = grand_nb(500, 0xDEAD_BEEF_1234_5678);
    let triple = &base * 3u32;
    assert_eq!(gcd_lehmer_candidate(&base, &triple), base);
    assert_eq!(gcd_lehmer_candidate(&triple, &base), base);
    // Cas limites : zéro, égalité, diviseur, signes.
    for (a, b) in [(0, 0), (0, 7), (7, 0), (6, 6), (6, 3), (-6, 3), (6, -3), (-6, -3)] {
        let aa = BigInt::from(a);
        let bb = BigInt::from(b);
        assert_eq!(gcd_lehmer_candidate(&aa, &bb), aa.gcd(&bb), "cas ({a},{b})");
    }
}

#[test]
fn exactitude_gcd_rapide() {
    // Le chemin INTÉGRÉ (différent du candidat du banc) doit être exact
    // sur toutes les tailles, y compris au-delà du banc (1 500 chiffres).
    let graine = 0xFEED_FACE_CAFE_BABEu64;
    for chiffres in [1usize, 20, 60, 120, 300, 500, 1000, 1500] {
        for k in 0..10u64 {
            let aa = grand_nb(chiffres, graine.wrapping_add(k));
            let bb = grand_nb(chiffres, graine.wrapping_add(100 + k));
            let attendu = aa.gcd(&bb);
            assert_eq!(rose::i128x::gcd_rapide(&aa, &bb), attendu, "chiffres={chiffres} k={k}");
            // Le couple inversé et le PGCD avec un multiple doivent concorder.
            assert_eq!(rose::i128x::gcd_rapide(&bb, &aa), attendu);
            let multiple = &attendu * 7u32;
            assert_eq!(rose::i128x::gcd_rapide(&aa, &multiple), aa.gcd(&multiple));
        }
    }
    // Tailles très déséquilibrées (le décalage commun doit rester correct).
    let petit = BigInt::from(1234567u64);
    let grand = grand_nb(1000, 0x0123_4567_89AB_CDEF);
    assert_eq!(rose::i128x::gcd_rapide(&grand, &petit), grand.gcd(&petit));
    assert_eq!(rose::i128x::gcd_rapide(&petit, &grand), grand.gcd(&petit));
    // Zéros et signes.
    for (a, b) in [(0, 0), (0, 12), (12, 0), (-12, 18), (12, -18), (-12, -18)] {
        let aa = BigInt::from(a);
        let bb = BigInt::from(b);
        assert_eq!(rose::i128x::gcd_rapide(&aa, &bb), aa.gcd(&bb));
    }
}

#[test]
fn exactitude_puissances() {
    let deux = BigInt::from(2u8);
    let attendu = BigInt::one() << 10000;
    let r = Gz::neuf();
    unsafe { gmp_ui_pow_ui(r.p(), 2, 10000); }
    assert_eq!(r.bref(), &attendu);
    unsafe { gmp_pow_ui(r.p(), Gz::de_u64(2).p(), 10000); }
    assert_eq!(r.bref(), &attendu);
    r.pow_u64(&Gz::de_u64(2), 10000);
    assert_eq!(r.bref(), &attendu);
    assert_eq!(attendu.to_string().len(), 3011);
    // Référence u128 (primitives standard) pour les petites valeurs.
    let trois = BigInt::from(3u8);
    // 3^80 ≈ 1,48·10^38 tient encore dans u128 (3^81 déborde).
    for e in 0..=80u32 {
        unsafe { gmp_ui_pow_ui(r.p(), 3, e as u64); }
        let mut attendu_u128 = 1u128;
        for _ in 0..e { attendu_u128 *= 3; }
        assert_eq!(r.bref(), &BigInt::from(attendu_u128));
        assert_eq!(attendu_u128, trois.pow(e).to_u128().unwrap());
    }
    let _ = deux;
}

#[test]
fn exactitude_rationnels() {
    let na = grand_nb(60, 0x1111_2222_3333_4444);
    let da = grand_nb(60, 0x5555_6666_7777_8888);
    let nb = grand_nb(60, 0x9999_AAAA_BBBB_CCCC);
    let db = grand_nb(60, 0xDDDD_EEEE_FFFF_0000);
    let qa = Gq::neuf();
    let qb = Gq::neuf();
    let qr = Gq::neuf();
    unsafe {
        (*qa.p()).num.v = na.clone(); (*qa.p()).den.v = da.clone();
        (*qb.p()).num.v = nb.clone(); (*qb.p()).den.v = db.clone();
    }
    let verifier = |qr: &Gq, attendu: &BigInt, atten_d: &BigInt| {
        let g = attendu.gcd(atten_d);
        assert_eq!(qr.num_bigint(), attendu / &g);
        assert_eq!(qr.den_bigint(), atten_d / &g);
        assert!(qr.den_bigint() > BigInt::zero());
    };
    unsafe { gmpq_add(qr.p(), qa.p(), qb.p()); }
    verifier(&qr, &(&na * &db + &nb * &da), &(&da * &db));
    unsafe { gmpq_sub(qr.p(), qa.p(), qb.p()); }
    verifier(&qr, &(&na * &db - &nb * &da), &(&da * &db));
    unsafe { gmpq_mul(qr.p(), qa.p(), qb.p()); }
    verifier(&qr, &(&na * &nb), &(&da * &db));
    unsafe { gmpq_div(qr.p(), qa.p(), qb.p()); }
    verifier(&qr, &(&na * &db), &(&da * &nb));
    // Comparaison.
    let cmp_rose = unsafe { gmpq_cmp(qa.p(), qb.p()) };
    let cmp_rust = (&na * &db).cmp(&(&nb * &da)) as i32;
    assert_eq!(cmp_rose.signum(), cmp_rust.signum());
}

#[test]
fn exactitude_reference_u128() {
    // Équivalent Rust « primitif » : PGCD de Stein et modulo sur u128,
    // comparés aux mêmes opérations Rose sur des valeurs qui tiennent en u64.
    fn stein(mut a: u128, mut b: u128) -> u128 {
        if a == 0 { return b; }
        if b == 0 { return a; }
        let sh = (a | b).trailing_zeros();
        a >>= a.trailing_zeros();
        loop {
            b >>= b.trailing_zeros();
            if a > b { std::mem::swap(&mut a, &mut b); }
            b -= a;
            if b == 0 { break; }
        }
        a << sh
    }
    let mut x = 0x9E37_79B9_7F4A_7C15u64;
    let mut suivant = || { x ^= x << 13; x ^= x >> 7; x ^= x << 17; x };
    for _ in 0..500 {
        let (av, bv) = (suivant() as u128, suivant() as u128);
        assert_eq!(stein(av, bv), Integer::gcd(&av, &bv));
        // Le même PGCD via la façade Rose doit donner le même résultat.
        let (ga, gb, gr) = (Gz::de_u64(av as u64), Gz::de_u64(bv as u64), Gz::neuf());
        unsafe { gmp_gcd(gr.p(), ga.p(), gb.p()); }
        assert_eq!(gr.bref(), &BigInt::from(stein(av, bv)));
    }
}

#[test]
fn exactitude_racine() {
    let r = calcul_racine(&BigInt::from(2u8), 100);
    let s = r.to_string();
    // Oracle indépendante : Python `decimal`, 120 dps. `calcul_racine`
    // rend √2·10^100, donc 1 chiffre entier puis 99 décimales dans s[..100].
    assert_eq!(&s[..100], "1414213562373095048801688724209698078569671875376948073176679737990732478462107038850387534327641572");
}
