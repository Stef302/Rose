//! Banc reproductible — coût des listes Rose vs `Vec<i64>` Rust.
//!
//! Mesure la STRUCTURE DE DONNÉES seule (pas le dispatch de l'interpréteur),
//! dans le MÊME binaire de test, avec 3 échantillons par point et médianes.
//! Deux chemins sont couverts :
//!   * `rose_canonique` — `joindre` classique (24 octets/élément) ;
//!   * `rose_compact` — miroir `slots i64` contigus (8 octets/élément).
//!
//! Usage :
//!   export CARGO_TARGET_DIR=/tmp/opencode/opt_shared_target
//!   bash scripts/garde_fou_mem.sh 12000000 cargo test --release \
//!       --test opt_listes -- --nocapture bench_listes
//!
//! Les assertions fonctionnelles (miroir hétérogène, alias, grand entier,
//! cohérence JIT ↔ interpréteur) sont dans `semantique_liste_preservee`.

use std::hint::black_box;
use std::time::Instant;

use rose::interp::{indexer_valeur, joindre_liste, longueur_valeur};
use rose::run::{valeur_egale, Valeur};

fn mediane(mut xs: Vec<f64>) -> f64 {
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs[xs.len() / 2]
}

fn ms(t: Instant) -> f64 { t.elapsed().as_secs_f64() * 1000.0 }

/// Construction canonique (COMPORTEMENT D'AVANT OPTIMISATION) : les
/// `Valeur::Entier` (24 octets) sont poussés directement dans le `Vec`
/// canonique, le miroir reste vide — `idx` doit alors relire le canonique.
fn construire_canonique(n: usize) -> Valeur {
    let l = Valeur::liste();
    if let Valeur::Liste(ref list) = l {
        let mut c = list.0.borrow_mut();
        c.reserve(n);
        for i in 0..n { c.push(Valeur::Entier(i as i64, None)); }
    }
    l
}

/// Construction compacte : le miroir `Vec<i64>` est rempli en une plage.
fn construire_compact(n: usize) -> Valeur {
    let l = Valeur::liste();
    if let Valeur::Liste(ref list) = l {
        list.pousser_i64_plage(0, n as i64);
    }
    l
}

/// Une passe complète : construction, idx répété, longueur, puis somme.
/// Rend (construction_ms, idx_ms, longueur_ms, somme_ms).
fn passe_rose(n: usize, compact: bool) -> (f64, f64, f64, f64) {
    let t0 = Instant::now();
    let l = if compact { construire_compact(n) } else { construire_canonique(n) };
    let constr = ms(t0);

    let t1 = Instant::now();
    let mut total = 0i64;
    for j in 0..n {
        let v = indexer_valeur(&l, &Valeur::Entier(j as i64, None)).unwrap();
        if let Valeur::Entier(x, None) = v { total = total.wrapping_add(x); }
    }
    let idx = ms(t1);
    assert_eq!(total, (n as i64 * (n as i64 - 1)) / 2);
    black_box(total);

    let t2 = Instant::now();
    let mut len = 0u64;
    for _ in 0..n { len = longueur_valeur(&l); }
    let longueur = ms(t2);
    assert_eq!(len, n as u64);

    let t3 = Instant::now();
    let mut somme = Valeur::Entier(0, None);
    for j in 0..n {
        let e = indexer_valeur(&l, &Valeur::Entier(j as i64, None)).unwrap();
        somme = rose::interp::binaire_valeurs("+", &somme, &e).unwrap();
    }
    let somme_ms = ms(t3);
    (constr, idx, longueur, somme_ms)
}

/// Référence Rust : le même travail sur `Vec<i64>` — même binaire, même
/// nombre d'itérations, `black_box` sur chaque lecture.
fn passe_rust(n: usize) -> (f64, f64, f64, f64) {
    let t0 = Instant::now();
    let mut v = Vec::with_capacity(n);
    for i in 0..n { v.push(black_box(i as i64)); }
    let constr = ms(t0);

    let t1 = Instant::now();
    let mut total = 0i64;
    for j in 0..n { total = total.wrapping_add(black_box(v[j])); }
    let idx = ms(t1);
    assert_eq!(total, (n as i64 * (n as i64 - 1)) / 2);
    black_box(total);

    let t2 = Instant::now();
    let mut len = 0usize;
    for _ in 0..n { len = black_box(v.len()); }
    let longueur = ms(t2);
    assert_eq!(len, n);

    let t3 = Instant::now();
    let mut somme = 0i64;
    for j in 0..n { somme = somme.wrapping_add(black_box(v[j])); }
    let somme_ms = ms(t3);
    (constr, idx, longueur, somme_ms)
}

#[test]
fn bench_listes() {
    for &n in &[10_000usize, 100_000usize] {
        // 3 échantillons par point, médianes (protocole de la mission).
        let mut rc = Vec::new(); let mut rk = Vec::new(); let mut rl = Vec::new(); let mut rs = Vec::new();
        for _ in 0..3 {
            let (a, b, c, d) = passe_rust(n);
            rc.push(a); rk.push(b); rl.push(c); rs.push(d);
        }
        let (rc, rk, rl, rs) = (mediane(rc), mediane(rk), mediane(rl), mediane(rs));

        for &compact in &[false, true] {
            let mut cs = Vec::new(); let mut ci = Vec::new(); let mut cl = Vec::new(); let mut ct = Vec::new();
            for _ in 0..3 {
                let (a, b, c, d) = passe_rose(n, compact);
                cs.push(a); ci.push(b); cl.push(c); ct.push(d);
            }
            let (cs, ci, cl, ct) = (mediane(cs), mediane(ci), mediane(cl), mediane(ct));
            let nom = if compact { "compact" } else { "canonique" };
            println!("BENCH\t{n}\t{nom}\tconstruction\t{cs:.3}\trust\t{rc:.3}\tratio\t{:.2}", rc / cs);
            println!("BENCH\t{n}\t{nom}\tidxi64\t{ci:.3}\trust\t{rk:.3}\tratio\t{:.2}", rk / ci);
            println!("BENCH\t{n}\t{nom}\tlongueur\t{cl:.3}\trust\t{rl:.3}\tratio\t{:.2}", rl / cl);
            println!("BENCH\t{n}\t{nom}\tsomme\t{ct:.3}\trust\t{rs:.3}\tratio\t{:.2}", rs / ct);
        }
    }
}

#[test]
fn semantique_liste_preservee() {
    // Liste hétérogène : le miroir doit être invalidé, la lecture canonique.
    let h = Valeur::liste_de(vec![
        Valeur::Entier(1, None),
        Valeur::s("deux"),
        Valeur::b(true),
        Valeur::Entier(9_007_199_254_740_993, None),
    ]);
    assert!(matches!(&indexer_valeur(&h, &Valeur::Entier(1, None)).unwrap(), Valeur::Chaine(_)));
    match &indexer_valeur(&h, &Valeur::Entier(3, None)).unwrap() {
        Valeur::Entier(x, None) => assert_eq!(*x, 9_007_199_254_740_993),
        _ => panic!("grand entier perdu"),
    }
    assert_eq!(longueur_valeur(&h), 4);
    // `erreur_runtime` panique (récupéré par `catch_unwind` dans le moteur).
    let hors = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = indexer_valeur(&h, &Valeur::Entier(4, None));
    }));
    assert!(hors.is_err(), "un index hors limites doit être signalé");

    // Alias : deux handles partagent la même arène, la mutation est visible.
    let a = Valeur::liste();
    let b = a.clone();
    joindre_liste(&a, Valeur::Entier(7, None));
    assert_eq!(longueur_valeur(&b), 1);
    assert!(valeur_egale(&indexer_valeur(&b, &Valeur::Entier(0, None)).unwrap(), &Valeur::Entier(7, None)));
    assert!(valeur_egale(&a, &b));

    // Écriture JIT directe dans le canonique (miroir désynchronisé) :
    // la lecture doit REVENIR au canonique, jamais au miroir périmé.
    let j = Valeur::liste_de(vec![Valeur::Entier(11, None)]);
    if let Valeur::Liste(l) = &j {
        l.0.borrow_mut().clear();
        l.0.borrow_mut().push(Valeur::Entier(22, None));
        l.0.borrow_mut().push(Valeur::Entier(33, None));
    }
    assert_eq!(longueur_valeur(&j), 2);
    assert!(valeur_egale(&indexer_valeur(&j, &Valeur::Entier(1, None)).unwrap(), &Valeur::Entier(33, None)));

    // Ajout après désynchronisation : le miroir ne doit pas ressusciter.
    joindre_liste(&j, Valeur::Entier(44, None));
    assert_eq!(longueur_valeur(&j), 3);
    assert!(valeur_egale(&indexer_valeur(&j, &Valeur::Entier(2, None)).unwrap(), &Valeur::Entier(44, None)));

    // Taille de `Valeur` : ABI JIT inchangée (24 octets).
    assert_eq!(std::mem::size_of::<Valeur>(), 24, "ABI Valeur cassée");
}
