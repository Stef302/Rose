//! Micro-banc reproductible — helpers JIT de listes (`jit.rs`) vs `Vec<i64>` Rust.
//!
//! Mesure les DEUX helpers du chemin chaud, dans le MÊME binaire de test,
//! avec 3 échantillons par point et médianes :
//!   * `joindre`      — `rose_jit_joindre(h, i)` (n appels) ;
//!   * `idx`          — `rose_jit_index_i64(h, j)` (n appels) ;
//!   * `construction` — passe complète joindre + idx.
//!
//! Deux tailles : 10 000 et 100 000 éléments. Le handle JIT vient de
//! `rose_jit_liste_vide()` (ABI i64 inchangée) ; le MÊME handle est passé aux
//! helpers avant et après l'optimisation, donc la mesure est comparative.
//!
//! Usage :
//!   export CARGO_TARGET_DIR=/tmp/opencode/opt_shared_target
//!   RUST_TEST_THREADS=1 bash scripts/garde_fou_mem.sh 12000000 \
//!       cargo test --release --test opt_jit_listes -- --nocapture bench_jit_listes
//!
//! Les assertions d'exactitude (miroir, hétérogène, ABI handle, parité
//! JIT ↔ interpréteur) sont dans `semantique_jit_listes_preservee`.

use std::hint::black_box;
use std::sync::Mutex;
use std::time::Instant;

use rose::run::Valeur;

/// L'arène JIT (`JIT_HANDLES`, pool de listes) est thread-local : deux tests
/// qui l'utilisent en parallèle dans le MÊME binaire peuvent se recouvrir
/// (SIGSEGV observé sans ce verrou). Les tests reproductibles du dépôt
/// s'exécutent avec `RUST_TEST_THREADS=1` ; le verrou rend aussi la suite
/// parallèle-saine quand `validation_finale.sh` ne pose pas la variable.
static SERIE: Mutex<()> = Mutex::new(());

fn mediane(mut xs: Vec<f64>) -> f64 {
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs[xs.len() / 2]
}

fn ms(t: Instant) -> f64 { t.elapsed().as_secs_f64() * 1000.0 }

/// Handle JIT neuf. `rose_jit_liste_vide` est le producteur de handle pointeur
/// tagué : on l'appelle UNE fois par passe, hors chrono.
fn handle_neuf() -> i64 { rose::jit::rose_jit_liste_vide() }

/// n appels à `rose_jit_joindre` (un seul handle, comme dans la boucle JIT).
fn passe_joindre_jit(h: i64, n: i64) -> f64 {
    let t = Instant::now();
    for i in 1..=n { rose::jit::rose_jit_joindre(h, black_box(i)); }
    ms(t)
}

/// n appels à `rose_jit_index_i64` sur une liste déjà remplie.
fn passe_idx_jit(h: i64, n: i64) -> (f64, i64) {
    let t = Instant::now();
    let mut total = 0i64;
    for j in 0..n {
        total = total.wrapping_add(black_box(rose::jit::rose_jit_index_i64(h, j)));
    }
    (ms(t), total)
}

/// Référence Rust : le même travail, même binaire, `black_box` identique.
fn passe_rust(n: i64) -> (f64, f64, i64) {
    let t0 = Instant::now();
    let mut v = Vec::with_capacity(n as usize);
    for i in 1..=n { v.push(black_box(i)); }
    let constr = ms(t0);

    let t1 = Instant::now();
    let mut total = 0i64;
    for j in 0..n { total = total.wrapping_add(black_box(v[j as usize])); }
    (constr, ms(t1), total)
}

/// Référence intermédiaire : pousser des `Valeur::Entier` (24 octets) dans un
/// `Vec<Valeur>` Rust. Isolate le coût de la représentation canonique du coût
/// du helper JIT lui-même.
fn passe_valeur(n: i64) -> f64 {
    let t = Instant::now();
    let mut v = Vec::with_capacity(n as usize);
    for i in 1..=n { v.push(Valeur::Entier(black_box(i), None)); }
    black_box(v);
    ms(t)
}

#[test]
fn bench_jit_listes() {
    let _garde = SERIE.lock().unwrap();
    for &n in &[10_000i64, 100_000i64] {
        let mut jj = Vec::new(); let mut ji = Vec::new();
        let mut rj = Vec::new(); let mut ri = Vec::new();
        let mut rv = Vec::new();
        for _ in 0..3 {
            // Ordre entrelacé : Rust puis JIT, dans la même salve.
            let (rc, ridx, rtotal) = passe_rust(n);
            let vv = passe_valeur(n);
            let h = handle_neuf();
            let jconstr = passe_joindre_jit(h, n);
            let (jidx, jtotal) = passe_idx_jit(h, n);
            assert_eq!(jtotal, rtotal, "parité JIT/Rust sur l'indexation");
            jj.push(jconstr); ji.push(jidx);
            rj.push(rc); ri.push(ridx); rv.push(vv);
            black_box(h);
        }
        let (mjc, mji, mrc, mri, mrv) =
            (mediane(jj), mediane(ji), mediane(rj), mediane(ri), mediane(rv));
        println!(
            "n={n:<7} | joindre JIT {mjc:8.4} ms  Rust Vec<Valeur> {mrv:8.4} ms  \
             Rust Vec<i64> {mrc:8.4} ms  x{:.2}/{:.2} | \
             idx JIT {mji:8.4} ms  Rust {mri:8.4} ms  x{:.2}",
            mjc / mrv, mjc / mrc, mji / mri
        );
    }
}

#[test]
fn semantique_jit_listes_preservee() {
    let _garde = SERIE.lock().unwrap();
    // 1. ABI handle i64 : handle pointeur tagué (bit 63), forme produite par
    //    `rose_jit_liste_vide` et acceptée par tous les helpers liste.
    let h = handle_neuf();
    assert_eq!(h & i64::MIN, i64::MIN, "handle liste doit être tagué bit 63");
    for i in 1..=100i64 { rose::jit::rose_jit_joindre(h, i); }
    assert_eq!(rose::jit::rose_jit_longueur(h), 100, "longueur JIT");

    // 2. Le canonique ET le miroir voient les mêmes éléments : le miroir doit
    //    être synchronisé par `rose_jit_joindre` (sinon `slots` = -1).
    let valeurs: Vec<i64> = (1..=100i64).collect();
    for (j, &attendu) in valeurs.iter().enumerate() {
        assert_eq!(rose::jit::rose_jit_index_i64(h, j as i64), attendu,
                   "idx JIT↔canonique en j={j}");
    }
    assert_eq!(rose::jit::rose_jit_liste_slots_count(h, 200), 100,
               "miroir i64 maintenu par le helper joindre");

    // 3. Repli exact sur liste hétérogène : le helper d'invalidation du miroir
    //    force la relecture du canonique (le JIT ne produit jamais d'hétérogène
    //    sous le garde `principal_sans_boxage`, mais le repli doit être sûr).
    let h2 = handle_neuf();
    rose::jit::rose_jit_joindre(h2, 7);
    // Un observateur non-JIT matérialise le canonique ; on invalide ensuite
    // le miroir pour forcer le repli sur ce canonique.
    rose::jit::rose_jit_liste_materiel(h2);
    rose::jit::rose_jit_liste_miroir_invalider(h2);
    assert_eq!(rose::jit::rose_jit_liste_slots_count(h2, 10), -1,
               "miroir Opac après invalidation");
    assert_eq!(rose::jit::rose_jit_index_i64(h2, 0), 7,
               "repli canonique après invalidation");
    // Le canonique reste la source de vérité (longueur inchangée).
    assert_eq!(rose::jit::rose_jit_longueur(h2), 1);

    // 4. Hors limites / index négatif : valeur neutre, jamais d'UB.
    assert_eq!(rose::jit::rose_jit_index_i64(h, -1), 0);
    assert_eq!(rose::jit::rose_jit_index_i64(h, 500), 0);

    // 5. Parité avec l'interpréteur : les valeurs lues via le miroir JIT sont
    //    exactement celles que `Valeur::Entier` produirait.
    let from_jit: Vec<i64> = (0..100).map(|j| rose::jit::rose_jit_index_i64(h, j)).collect();
    let from_interp: Vec<i64> = valeurs.iter().map(|&x| match Valeur::z(x) {
        Valeur::Entier(n, None) => n, _ => panic!("boxage inattendu"),
    }).collect();
    assert_eq!(from_jit, from_interp, "parité JIT ↔ interpréteur");
}
