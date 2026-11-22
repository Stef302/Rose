//! Micro-banc du JIT Cranelift (vague OPT-JIT).
//!
//! Chaque charge est exécutée trois fois avec `ROSE_TIMING=1`, en mode JIT
//! (défaut) et en mode interpréteur pur (`ROSE_SANS_JIT=1`). On relève :
//!   * `exec`        — temps d'exécution du corps compilé / interprété ;
//!   * `compiletout` — temps de compilation JIT (entre `Jit::nouveau` et
//!     l'entrée dans `executer_principal`) ; 0 côté interpréteur.
//!
//! Les médianes sont affichées avec `cargo test --release --test opt_jit --
//! --nocapture`. Les assertions portent sur la PARITÉ octet pour octet des
//! sorties : les durées sont des constats datés, jamais des seuils de test
//! (une machine chargée ne doit pas faire échouer la suite).

use std::path::PathBuf;
use std::process::{Command, Output};

const DIR: &str = "/tmp/opencode/opt_jit_bench";

fn binaire() -> &'static str {
    env!("CARGO_BIN_EXE_rose")
}

/// Découpe le champ `TIMING … exec=<durée> …` (format Debug de `Duration`).
fn champ(timing: &str, cle: &str) -> Option<f64> {
    let debut = timing.find(&format!("{cle}="))? + cle.len() + 1;
    let fin = timing[debut..].find(' ').map(|i| debut + i).unwrap_or(timing.len());
    duree_us(&timing[debut..fin])
}

/// `4.816µs`, `1.313692ms`, `291.247µs`, `1.027421ms`, `1.2s` → microsecondes.
fn duree_us(s: &str) -> Option<f64> {
    let s = s.trim();
    if let Some(v) = s.strip_suffix("ns") { return v.parse::<f64>().ok().map(|x| x / 1000.0); }
    if let Some(v) = s.strip_suffix("µs") { return v.parse::<f64>().ok(); }
    if let Some(v) = s.strip_suffix("ms") { return v.parse::<f64>().ok().map(|x| x * 1000.0); }
    if let Some(v) = s.strip_suffix('s') { return v.parse::<f64>().ok().map(|x| x * 1_000_000.0); }
    None
}

fn mediane(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

struct Resultat {
    sortie: Vec<u8>,
    /// Médiane de `exec` en microsecondes.
    exec_us: f64,
    /// Médiane de `compiletout` en microsecondes (0 si pas de JIT).
    compile_us: f64,
}

fn lancer(prog: &PathBuf, sans_jit: bool) -> Output {
    let mut cmd = Command::new(binaire());
    cmd.arg(prog).env("ROSE_TIMING", "1");
    // A/B des réglages Cranelift : `ROSE_JIT_OPT`, `ROSE_JIT_VERIFIER`, …
    for (k, v) in std::env::vars() {
        if k.starts_with("ROSE_JIT_") { cmd.env(k, v); }
    }
    if sans_jit { cmd.env("ROSE_SANS_JIT", "1"); }
    cmd.output().expect("lancement de rose")
}

fn mesurer(nom: &str, source: &str, sans_jit: bool) -> Resultat {
    std::fs::create_dir_all(DIR).expect("création du dossier du micro-banc");
    let prog = PathBuf::from(DIR).join(format!("{nom}.rose"));
    std::fs::write(&prog, source).expect("écriture du programme du micro-banc");
    let mut execs = Vec::new();
    let mut compiles = Vec::new();
    let mut sortie = Vec::new();
    for _ in 0..3 {
        let o = lancer(&prog, sans_jit);
        sortie = o.stdout.clone();
        let timing = String::from_utf8_lossy(&o.stderr).into_owned();
        execs.push(champ(&timing, "exec").unwrap_or_else(|| panic!("TIMING exec absent pour {nom}")));
        compiles.push(champ(&timing, "compiletout").unwrap_or(0.0));
    }
    Resultat { sortie, exec_us: mediane(execs), compile_us: mediane(compiles) }
}

struct Charge {
    nom: &'static str,
    source: &'static str,
}

fn charges() -> Vec<Charge> {
    vec![
        Charge {
            nom: "fib",
            source: "fn fib(n: entier) -> entier {\n\
                       si (n < 2) { retour n }\n\
                       retour fib(n - 1) + fib(n - 2)\n\
                     }\n\
                     >> fib(21)\n",
        },
        Charge {
            nom: "factorielle",
            source: "fn f(n: entier) -> entier {\n\
                       si (n <= 1) { retour 1 }\n\
                       retour n * f(n - 1)\n\
                     }\n\
                     >> f(20)\n",
        },
        Charge {
            nom: "boucle",
            source: "s = 0\npour i de 1 vers 100000 { s = s + i }\n>> s\n",
        },
        Charge {
            nom: "bigint",
            source: ">> 2 ^ 3000\n",
        },
        Charge {
            nom: "chaine",
            source: "s = \"\"\npour i de 1 vers 500 { s = s + \"x\" }\n>> longueur(s)\n",
        },
        Charge {
            // ⚠ Le compteur s'appelle volontairement `i` : c'est le cas que
            // le correctif rir.rs rend JIT-compilable (auparavant interprété).
            nom: "table",
            source: "t = table()\npour i de 1 vers 500 { inserer(t, i, i * 2) }\n>> longueur(t)\n",
        },
        Charge {
            // Double boucle + modulo : mesure l'effet du chemin rapide `*`
            // sans masquer une régression des optimisations de vitesse.
            nom: "primes",
            source: "n = 2\nc = 0\n\
                     tant_que (n < 3000) {\n\
                       d = 2\n\
                       premier = 1\n\
                       tant_que (d * d <= n) {\n\
                         si (n % d == 0) { premier = 0 }\n\
                         d = d + 1\n\
                       }\n\
                       c = c + premier\n\
                       n = n + 1\n\
                     }\n\
                     >> c\n",
        },
    ]
}

/// Parité JIT ↔ interpréteur sur les 25 programmes de référence. `20_hasard`
/// est exclu : sa sortie est aléatoire PAR CONCEPTION, donc jamais comparable
/// entre deux exécutions (même en mode interpréteur).
#[test]
fn parite_25_programmes() {
    let racine = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dossier = racine.join("tests_aot8/programmes");
    let mut n = 0;
    for entree in std::fs::read_dir(&dossier).expect("dossier des 25 programmes") {
        let prog = entree.expect("entrée du dossier").path();
        if prog.file_name().and_then(|s| s.to_str()) == Some("20_hasard.rose") { continue; }
        let jit = lancer(&prog, false);
        let interp = lancer(&prog, true);
        assert_eq!(jit.stdout, interp.stdout,
                   "divergence JIT/interpréteur sur {}", prog.display());
        n += 1;
    }
    assert_eq!(n, 24, "25 programmes moins `hasard`");
}

#[test]
fn jit_vs_interpreteur() {
    println!("charge        JIT exec    interp exec   accélération   JIT compile");
    println!("-----------  ----------  ------------   ------------   ------------");
    for c in charges() {
        let jit = mesurer(c.nom, c.source, false);
        let interp = mesurer(c.nom, c.source, true);
        assert_eq!(jit.sortie, interp.sortie,
                   "divergence JIT/interpréteur sur {}", c.nom);
        println!("{:11}  {:>8.1}µs  {:>10.1}µs   {:>9.2}×   {:>9.1}µs",
                 c.nom, jit.exec_us, interp.exec_us,
                 interp.exec_us / jit.exec_us.max(1e-9), jit.compile_us);
    }
}
