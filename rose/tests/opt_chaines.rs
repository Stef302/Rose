//! Banc reproductible — coût des chaînes Rose (rope `ChaineRep`) vs `String` Rust.
//!
//! But : mesurer la STRUCTURE de données seule (pas le dispatch de
//! l'interpréteur), dans le MÊME binaire de test, avec 3 échantillons par
//! point et médianes. Trois familles :
//!   * `concat`   — n concaténations `acc = acc + "x"` ;
//!   * `tranche`  — 1 000 `sous_chaine(i, i+1)` APRÈS la concaténation ;
//!   * `longueur` — 20 000 lectures de `n` (compte mémorisé).
//!
//! Usage :
//!   export CARGO_TARGET_DIR=/tmp/opencode/opt_shared_target
//!   bash scripts/garde_fou_mem.sh 12000000 cargo test --release \
//!       --test opt_chaines -- --nocapture bench_chaines
//!
//! Les assertions d' exactitude (Unicode, parité avec `String`, in-place)
//! sont dans `semantique_chaines_preservee`.

use std::hint::black_box;
use std::rc::Rc;
use std::time::Instant;

use rose::run::{ChaineRep, NoeudChaine};

// ── utilitaires de mesure ───────────────────────────────────────────────────

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

/// Une passe concat : Rose (rope) et Rust (`String::push_str`).
/// `possessif` mesure le chemin réellement emprunté par l'accélérateur RIR
/// (`concat_possessif` : zéro allocation quand l'accumulateur est unique) ;
/// `false` mesure le `concat` générique, persistant, pour les préfixes
/// partagés (une allocation O(1) par tour).
fn passe_concat(n: usize, possessif: bool) -> (f64, f64) {
    let feuille = ChaineRep::plein("x".to_string());
    let t = Instant::now();
    let mut acc = ChaineRep::plein(String::new());
    for _ in 0..n {
        acc = if possessif {
            ChaineRep::concat_possessif(acc, &feuille)
        } else {
            ChaineRep::concat(&acc, &feuille)
        };
    }
    let rose = t.elapsed().as_secs_f64() * 1000.0;
    black_box(&acc);

    let t = Instant::now();
    let mut s = String::new();
    for _ in 0..n { s.push_str("x"); }
    let rust = t.elapsed().as_secs_f64() * 1000.0;
    black_box(s);
    (rose, rust)
}

/// Construit une rope de n feuilles (hors chrono) et mesure 1 000 tranches.
fn passe_tranche(n: usize, combien: usize) -> (f64, f64, String) {
    let feuille = ChaineRep::plein("x".to_string());
    let mut acc = ChaineRep::plein(String::new());
    for _ in 0..n { acc = ChaineRep::concat(&acc, &feuille); }
    let attendu = acc.aplatir();

    let t = Instant::now();
    let mut total = 0usize;
    for i in 0..combien { total += acc.tranche(i, i + 1).len(); }
    let rose = t.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(total, combien);

    let t = Instant::now();
    let mut total = 0usize;
    for i in 0..combien { total += attendu[i..i + 1].len(); }
    let rust = t.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(total, combien);
    black_box(total);
    (rose, rust, attendu)
}

/// Lecture du compte de caractères (O(1) mémorisé) vs `String::len`.
fn passe_longueur(rep: &Rc<ChaineRep>, s: &str, combien: usize) -> (f64, f64) {
    let t = Instant::now();
    let mut n = 0usize;
    for _ in 0..combien { n = n.wrapping_add(rep.n); }
    let rose = t.elapsed().as_secs_f64() * 1000.0;

    let t = Instant::now();
    let mut m = 0usize;
    for _ in 0..combien { m = m.wrapping_add(s.len()); }
    let rust = t.elapsed().as_secs_f64() * 1000.0;
    black_box((n, m));
    (rose, rust)
}

fn medians3_concat(n: usize, possessif: bool) -> (f64, f64) {
    let mut r = Vec::new();
    let mut h = Vec::new();
    for _ in 0..3 {
        let (a, b) = passe_concat(n, possessif);
        r.push(a); h.push(b);
    }
    (median(r), median(h))
}

fn medians3_tranche(n: usize, combien: usize) -> (f64, f64, String) {
    let mut r = Vec::new();
    let mut h = Vec::new();
    let mut attendu = String::new();
    for _ in 0..3 {
        let (a, b, s) = passe_tranche(n, combien);
        r.push(a); h.push(b); attendu = s;
    }
    (median(r), median(h), attendu)
}

fn medians3_longueur(rep: &Rc<ChaineRep>, s: &str, combien: usize) -> (f64, f64) {
    let mut r = Vec::new();
    let mut h = Vec::new();
    for _ in 0..3 {
        let (a, b) = passe_longueur(rep, s, combien);
        r.push(a); h.push(b);
    }
    (median(r), median(h))
}

#[test]
fn bench_chaines() {
    let rss0 = pic_rss_kio();
    println!("\n=== BANC CHAÎNES (3 échantillons, médianes) ===");
    println!("rss_initial={rss0} KiB");
    println!("{:<7} {:<12} {:>11} {:>11} {:>10}",
             "n", "op", "rose_ms", "rust_ms", "ratio");

    for &n in &[1_000usize, 10_000, 20_000] {
        let (r, h) = medians3_concat(n, false);
        println!("{:<7} {:<12} {:>11.4} {:>11.4} {:>10.2}x",
                 n, "concat", r, h, r / h.max(1e-9));
        let (r, h) = medians3_concat(n, true);
        println!("{:<7} {:<12} {:>11.4} {:>11.4} {:>10.2}x",
                 n, "concat_poss", r, h, r / h.max(1e-9));
    }

    let (tr, th, attendu) = medians3_tranche(20_000, 1_000);
    println!("{:<7} {:<12} {:>11.4} {:>11.4} {:>10.2}x",
             "20k", "tranche×1000", tr, th, tr / th.max(1e-9));
    assert_eq!(attendu.len(), 20_000);

    let feuille = ChaineRep::plein("x".to_string());
    let mut rep = ChaineRep::plein(String::new());
    for _ in 0..20_000 { rep = ChaineRep::concat(&rep, &feuille); }
    let (lr, lh) = medians3_longueur(&rep, &attendu, 20_000);
    println!("{:<7} {:<12} {:>11.4} {:>11.4} {:>10.2}x",
             "20k", "longueur×20k", lr, lh, lr / lh.max(1e-9));

    let t = Instant::now();
    let plat = rep.aplatir();
    let aplat = t.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(plat, attendu);
    println!("{:<7} {:<12} {:>11.4}", "20k", "aplatir", aplat);
    println!("rss_final={} KiB (VmHWM)", pic_rss_kio());
}

// ── non-régression fonctionnelle ────────────────────────────────────────────

#[test]
fn semantique_chaines_preservee() {
    // 1. concaténation + aplatissement, ASCII et Unicode.
    let a = ChaineRep::plein("café".to_string());
    let b = ChaineRep::plein("🌍".to_string());
    let c = ChaineRep::concat(&a, &b);
    assert_eq!(c.n, 5);                     // c a f é + 🌍 = 5 points de code
    assert_eq!(c.octets, "café🌍".len());
    assert_eq!(c.aplatir(), "café🌍");

    // 2. tranche par points de code, y compris sur une rope concaténée.
    assert_eq!(c.tranche(0, 1), "c");
    assert_eq!(c.tranche(3, 4), "é");
    assert_eq!(c.tranche(4, 5), "🌍");
    assert_eq!(c.tranche(0, 5), "café🌍");
    assert_eq!(c.tranche(2, 2), "");

    // 3. parité avec String sur 500 concaténations ASCII + Unicode.
    let mut rope = ChaineRep::plein(String::new());
    let mut chaine = String::new();
    for i in 0..500 {
        let morceau = if i % 7 == 0 { "é" } else if i % 11 == 0 { "🌍" } else { "x" };
        rope = ChaineRep::concat(&rope, &ChaineRep::plein(morceau.to_string()));
        chaine.push_str(morceau);
    }
    assert_eq!(rope.n, chaine.chars().count());
    assert_eq!(rope.octets, chaine.len());
    assert_eq!(rope.aplatir(), chaine);
    for i in 0..50 {
        let d = i * 3;
        let f = (i * 7 + 1).min(chaine.chars().count());
        let refer: String = chaine.chars().skip(d).take(f - d).collect();
        assert_eq!(rope.tranche(d, f), refer, "tranche({d},{f})");
    }

    // 4. ajout en place (feuille possédée) : même sémantique que `concat`.
    let mut feuille = ChaineRep::plein("ab".to_string());
    assert!(Rc::get_mut(&mut feuille).unwrap().pousser("c"));
    assert_eq!(&feuille.aplatir(), "abc");
    assert!(Rc::get_mut(&mut feuille).unwrap().pousser("dé"));
    assert_eq!(&feuille.aplatir(), "abcdé");
    assert_eq!(feuille.n, 5);
    assert_eq!(feuille.octets, "abcdé".len());
    // 12 octets → 15 → 18 : promotion PleinCourt → Plein à la 3e écriture.
    let mut courte = ChaineRep::plein(String::new());
    assert!(matches!(courte.noeud, NoeudChaine::PleinCourt { .. }));
    for _ in 0..20 { assert!(Rc::get_mut(&mut courte).unwrap().pousser("é")); }
    assert!(matches!(courte.noeud, NoeudChaine::Plein(_)));
    assert_eq!(courte.n, 20);
    assert_eq!(courte.octets, 40);
    // une rope concaténée refuse l'ajout en place (l'appelant doit faire un Concat).
    let mut rope2 = ChaineRep::concat(&a, &b);
    assert!(!Rc::get_mut(&mut rope2).unwrap().pousser("!"));
    assert_eq!(rope2.aplatir(), "café🌍");

    // 5. concaténation possédée : ajout en place,mais un préfixe ALIASÉ
    //    retombe sur le `concat` persistant et n'est jamais muté.
    let base = ChaineRep::plein("ab".to_string());
    let alias = base.clone();
    let poss = ChaineRep::concat_possessif(base, &ChaineRep::plein("c".to_string()));
    assert_eq!(poss.aplatir(), "abc");
    assert_eq!(alias.aplatir(), "ab");
    let seule = ChaineRep::concat_possessif(ChaineRep::plein("xy".to_string()),
                                            &ChaineRep::plein("z".to_string()));
    assert_eq!(seule.aplatir(), "xyz");
}
