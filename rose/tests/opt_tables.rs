//! Banc reproductible — coût des tables Rose vs `HashMap` Rust.
//!
//! But : mesurer la STRUCTURE de données seule (pas le dispatch de
//! l'interpréteur), dans le MÊME binaire de test, avec 3 échantillons par
//! point et médianes. Deux familles de clés :
//!   * `entier` — clés denses `0..n` (voie `array_part` de `TableRep`) ;
//!   * `chaine` — clés `"k0".."kn"` (voie hachage ouvert `IndexRapide`).
//!
//! Usage :
//!   export CARGO_TARGET_DIR=/tmp/opencode/opt_shared_target
//!   bash scripts/garde_fou_mem.sh 12000000 cargo test --release \
//!       --test opt_tables -- --nocapture bench_tables
//!
//! Les assertions fonctionnelles (ordre d'insertion, mise à jour, suppressions,
//! égalité numérique hétérogène) sont dans `semantique_table_preservee`.

use std::collections::HashMap;
use std::hint::black_box;
use std::time::Instant;

use rose::run::{Table, Valeur, chaine_de, valeur_egale};

// ── utilitaires de mesure ───────────────────────────────────────────────────

/// Pic RSS du processus, en KiB (VmHWM = high water mark, Linux).
fn pic_rss_kio() -> u64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(s) = std::fs::read_to_string("/proc/self/status") {
            for l in s.lines() {
                if let Some(v) = l.strip_prefix("VmHWM:") {
                    return v.split_whitespace().next()
                        .and_then(|x| x.parse().ok()).unwrap_or(0);
                }
            }
        }
    }
    0
}

fn median(mut xs: Vec<f64>) -> f64 {
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs[xs.len() / 2]
}

#[derive(Clone, Copy)]
enum Cle { Entier, Chaine }

impl Cle {
    fn nom(self) -> &'static str {
        match self { Cle::Entier => "entier", Cle::Chaine => "chaine" }
    }
    fn cle_rose(self, i: i64) -> Valeur {
        match self {
            Cle::Entier => Valeur::Entier(i, None),
            Cle::Chaine => Valeur::s(format!("k{i}")),
        }
    }
}

/// Une passe : n insertions puis n recherches. Rend (insertion_ms, recherche_ms).
fn passe_rose(n: i64, kind: Cle) -> (f64, f64) {
    let t = Table::neuf(Vec::new());
    let ti = Instant::now();
    for i in 0..n {
        t.inserer(kind.cle_rose(i), Valeur::Entier(i * i, None));
    }
    let ins = ti.elapsed().as_secs_f64() * 1000.0;
    let tr = Instant::now();
    let mut trouves = 0usize;
    for i in 0..n {
        if t.trouver(&kind.cle_rose(i)).is_some() { trouves += 1; }
    }
    black_box(trouves);
    assert_eq!(trouves as i64, n);
    (ins, tr.elapsed().as_secs_f64() * 1000.0)
}

/// Référence Rust : `HashMap` sur le MÊME genre de clé, même binaire.
fn passe_rust(n: i64, kind: Cle) -> (f64, f64) {
    match kind {
        Cle::Entier => passe_rust_t(n, |i| i),
        Cle::Chaine => passe_rust_t(n, |i| format!("k{i}")),
    }
}

fn passe_rust_t<K: std::hash::Hash + Eq>(n: i64, cle: impl Fn(i64) -> K) -> (f64, f64) {
    let ti = Instant::now();
    let mut m: HashMap<K, i64> = HashMap::new();
    for i in 0..n { m.insert(cle(i), i * i); }
    let ins = ti.elapsed().as_secs_f64() * 1000.0;
    let tr = Instant::now();
    let mut trouves = 0usize;
    for i in 0..n {
        if m.contains_key(&cle(i)) { trouves += 1; }
    }
    black_box(trouves);
    assert_eq!(trouves as i64, n);
    (ins, tr.elapsed().as_secs_f64() * 1000.0)
}

/// Référence secondaire : liste d'association linéaire (l'ancien `table()`),
/// pour quantifier le gain de structure indépendamment du gain de langage.
fn passe_rust_lineaire(n: i64) -> (f64, f64) {
    let ti = Instant::now();
    let mut v: Vec<(i64, i64)> = Vec::new();
    for i in 0..n { v.push((i, i * i)); }
    let ins = ti.elapsed().as_secs_f64() * 1000.0;
    let tr = Instant::now();
    let mut trouves = 0usize;
    for i in 0..n {
        if v.iter().any(|&(k, _)| k == i) { trouves += 1; }
    }
    black_box(trouves);
    assert_eq!(trouves as i64, n);
    (ins, tr.elapsed().as_secs_f64() * 1000.0)
}

fn medians3(n: i64, kind: Cle, rose: bool) -> (f64, f64) {
    let mut ins = Vec::new();
    let mut rch = Vec::new();
    for _ in 0..3 {
        let (a, b) = if rose { passe_rose(n, kind) } else { passe_rust(n, kind) };
        ins.push(a);
        rch.push(b);
    }
    (median(ins), median(rch))
}

#[test]
fn bench_tables() {
    let rss0 = pic_rss_kio();
    println!("\n=== BANC TABLES (3 échantillons, médianes) ===");
    println!("rss_initial={rss0} KiB");
    println!("{:<8} {:<7} {:<12} {:>11} {:>11} {:>11} {:>11} {:>9}",
             "n", "clés", "impl", "ins_ms", "rch_ms", "total_ms", "rust_tot", "ratio");

    for &n in &[1000i64, 3000, 10000] {
        for &kind in &[Cle::Entier, Cle::Chaine] {
            let (ri, rr) = medians3(n, kind, true);
            let (hi, hr) = medians3(n, kind, false);
            let rt = ri + rr;
            let ht = hi + hr;
            println!("{:<8} {:<7} {:<12} {:>11.4} {:>11.4} {:>11.4} {:>11.4} {:>9.2}x",
                     n, kind.nom(), "Rose/Table", ri, rr, rt, ht, rt / ht.max(1e-9));
        }
        let (li, lr) = {
            let mut i = Vec::new(); let mut r = Vec::new();
            for _ in 0..3 { let (a, b) = passe_rust_lineaire(n); i.push(a); r.push(b); }
            (median(i), median(r))
        };
        println!("{:<8} {:<7} {:<12} {:>11.4} {:>11.4} {:>11.4}",
                 n, "entier", "Rust/Vec-lin", li, lr, li + lr);
    }
    println!("rss_final={} KiB (VmHWM)", pic_rss_kio());
}

// ── non-régression fonctionnelle ────────────────────────────────────────────

#[test]
fn semantique_table_preservee() {
    let t = Table::neuf(Vec::new());
    t.inserer(Valeur::s("b"), Valeur::Entier(2, None));
    t.inserer(Valeur::s("a"), Valeur::Entier(1, None));
    t.inserer(Valeur::s("c"), Valeur::Entier(3, None));
    // Ordre d'insertion strictement conservé pour cles()/valeurs().
    let items = &t.0.borrow().items;
    assert_eq!(items.len(), 3);
    assert!(valeur_egale(&items[0].0, &Valeur::s("b")));
    assert!(valeur_egale(&items[1].0, &Valeur::s("a")));
    assert!(valeur_egale(&items[2].0, &Valeur::s("c")));

    // Mise à jour : pas de doublon, ordre inchangé.
    t.inserer(Valeur::s("a"), Valeur::Entier(10, None));
    assert_eq!(t.0.borrow().items.len(), 3);
    assert_eq!(chaine_de(&t.0.borrow().items[1].1), "10");

    // Égalité numérique hétérogène : 5, 5.0 et 5/1 désignent la même clé.
    let n = Table::neuf(Vec::new());
    n.inserer(Valeur::Entier(5, None), Valeur::s("cinq"));
    assert!(n.trouver(&Valeur::Flottant(5.0)).is_some());
    assert!(n.trouver(&Valeur::Entier(5, None)).is_some());
    n.inserer(Valeur::Flottant(5.0), Valeur::s("remplace"));
    assert_eq!(n.0.borrow().items.len(), 1);

    // Suppression puis réinsertion : l'index doit rester cohérent.
    assert!(n.retirer(&Valeur::Entier(5, None)));
    assert!(n.trouver(&Valeur::Entier(5, None)).is_none());
    n.inserer(Valeur::Entier(5, None), Valeur::s("apres"));
    assert!(n.trouver(&Valeur::Flottant(5.0)).is_some());

    // Clés sans hachage canonique (liste) : repli linéaire, ordre conservé.
    let l = Table::neuf(Vec::new());
    let k1 = Valeur::liste_de(vec![Valeur::Entier(1, None)]);
    let k2 = Valeur::liste_de(vec![Valeur::Entier(2, None)]);
    l.inserer(k1.clone(), Valeur::Entier(11, None));
    l.inserer(k2.clone(), Valeur::Entier(22, None));
    assert!(l.trouver(&k1).is_some());
    assert!(l.trouver(&k2).is_some());
    assert_eq!(l.0.borrow().items.len(), 2);
}
