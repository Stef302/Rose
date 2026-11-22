//! Chantier OPT_VM — harnais de mesure interne (bac à sable, aucun commit).
//!
//! Les temps sont mesurés DANS LE PROCESSUS (une seule initialisation du
//! moteur), avec `std::hint::black_box` pour empêcher le compilateur de
//! supprimer le travail. Chaque ligne `BENCH` est imprimée avec
//! `cargo test --release --test opt_vm -- --nocapture --test-threads=1`.
//!
//! Émission : lots de 200 appels par échantillon, 5 échantillons → médiane
//! divisée par 200 (les appels isolés durent ~1 µs et seraient noyés dans le
//! bruit). Exécution : 3 appels réels (dont un échauffement du pipeline JIT).
//! `OPTVM_ROUNDS` règle le nombre d'échantillons (défaut 5).

use std::hint::black_box;
use std::sync::Mutex;
use std::time::Instant;

/// `run::SINK` et le pointeur d'interpréteur du JIT sont GLOBAUX : les deux
/// tests de ce binaire ne doivent jamais tourner en parallèle.
static VERROU: Mutex<()> = Mutex::new(());

const FIB: &str = r#"
fn fib(n: entier) -> entier {
  si (n < 2) { retour n }
  retour fib(n - 1) + fib(n - 2)
}
>> fib(28)
"#;

const BOUCLES: &str = r#"
fn somme(n: entier) -> entier {
  $s = 0
  $k = 0
  tant_que ($k < n) {
    $s = $s + $k
    $k = $k + 1
  }
  retour $s
}
>> somme(2000)
"#;

const TABLE: &str = r#"
$t = table()
inserer($t, "a", 1)
inserer($t, "b", 2)
inserer($t, "c", 3)
>> $t["a"] + $t["b"] + $t["c"]
"#;

/// Grossissant (120 fonctions, ~1 400 instructions) : rend visibles les
/// recopies d'AST lors de l'émission (snapshot et WASM).
fn grand_source() -> String {
    let mut s = String::new();
    for i in 0..120 {
        s.push_str(&format!(
            "fn f{i}(x: entier) -> entier {{\n  $a = x + {i}\n  $b = $a * 2\n  $c = $b - x\n  $d = $c + 1\n  retour $d\n}}\n"));
    }
    s.push_str(">> f0(3)\n");
    s
}

fn mediane(mut v: Vec<u128>) -> u128 {
    v.sort_unstable();
    v[v.len() / 2]
}

/// Une médiane sur `n` échantillons, chaque échantillon mesurant `lot` appels.
fn mesurer<R>(n: usize, lot: usize, mut f: impl FnMut() -> R) -> (u128, R) {
    let mut temps = Vec::with_capacity(n);
    let mut dernier = None;
    for _ in 0..n {
        let t = Instant::now();
        for _ in 0..lot { black_box(f()); }
        temps.push(t.elapsed().as_nanos() / lot as u128);
        dernier = Some(f()); // valeur témoin hors mesure
    }
    (mediane(temps), dernier.unwrap())
}

#[test]
fn bench_opt_vm() {
    let _g = VERROU.lock().unwrap();
    let rounds: usize = std::env::var("OPTVM_ROUNDS").ok()
        .and_then(|s| s.parse().ok()).unwrap_or(5);
    let grand = grand_source();
    let scenarios = [
        ("fib", FIB), ("boucles", BOUCLES), ("table", TABLE),
        ("grand", grand.as_str()),
    ];

    for (nom, source) in scenarios {
        let prog = rose::front::analyser(source)
            .unwrap_or_else(|e| panic!("analyse {nom}: {e}"));

        let (ns, bc) = mesurer(rounds, 200, || rose::rosevm::ecrire_snapshot(&prog));
        println!("BENCH\t{nom}\tbc_emission\t{ns}\t{}", bc.len());

        let (ns, lu) = mesurer(rounds, 200, || {
            let p = rose::rosevm::lire_snapshot(black_box(&bc)).expect("lecture snapshot");
            black_box(p.instructions.len())
        });
        println!("BENCH\t{nom}\tbc_lecture\t{ns}\t{lu}");

        // ⚠ L'exécution RVM est mesurée EN DERNIER : le JIT laisse des pages
        // de code et un état global qui perturbent les micro-mesures d'émission
        // suivantes (mesuré : transpile_js x1,8 après fib JIT).
        for (cible, f) in [("js", 0), ("py", 1), ("c", 2), ("wasm", 3)] {
            let r = mesurer(rounds, 100, || match f {
                0 => rose::transpile::transpiler_programme(&prog, rose::transpile::Cible::Js).map(|s| s.into_bytes()),
                1 => rose::transpile::transpiler_programme(&prog, rose::transpile::Cible::Py).map(|s| s.into_bytes()),
                2 => rose::transpile::transpiler_programme(&prog, rose::transpile::Cible::C).map(|s| s.into_bytes()),
                _ => rose::transpile::transpiler_programme_wasm(&prog),
            });
            match r.1 {
                Ok(o) => println!("BENCH\t{nom}\ttranspile_{cible}\t{}\t{}", r.0, o.len()),
                Err(e) => println!("BENCH\t{nom}\ttranspile_{cible}\t{}\tERR:{e}", r.0),
            }
        }

        // Échauffement du cache JIT global : les médianes portent sur le
        // régime établi, identique avant et après optimisation.
        let _ = rose::rosevm::executer_snapshot(&bc, &[]);
        let (ns, sortie) = mesurer(rounds.max(3), 1, || {
            let s = rose::rosevm::executer_snapshot(black_box(&bc), &[]).expect("exécution RVM");
            let n = s.len();
            black_box(s);
            n
        });
        println!("BENCH\t{nom}\trvm_execution\t{ns}\t{sortie}");
    }
}

#[test]
fn verifier_sorties() {
    let _g = VERROU.lock().unwrap();
    let prog = rose::front::analyser(FIB).unwrap();
    let bc = rose::rosevm::ecrire_snapshot(&prog);
    let sortie = rose::rosevm::executer_snapshot(&bc, &[]).unwrap();
    assert_eq!(sortie.trim(), "317811", "sortie RVM fib");
    assert_eq!(rose::rosevm::version_fichier(&bc).unwrap(), rose::rosevm::VERSION_AST);

    let wasm = rose::transpile::transpiler_source_wasm(FIB).unwrap();
    assert_eq!(&wasm[..8], &[0, 0x61, 0x73, 0x6d, 1, 0, 0, 0], "magic WASM");
    assert!(rose::transpile::transpiler_source(FIB, rose::transpile::Cible::Js).unwrap().contains("function"));
}
