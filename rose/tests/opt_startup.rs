// VAGUE 4 — garde-fous du chemin de démarrage.
//
// Ces tests ne mesurent PAS des durées (trop bruité sur une machine
// partagée) : ils verrouillent ce qui DOIT rester vrai pendant qu'on
// optimise l'amorçage — les sorties bit-identiques des commandes
// informationnelles, et la disponibilité de la décomposition des phases.
// Les chiffres, eux, sont dans docs/OPT_STARTUP.md.

use std::process::{Command, Output};

fn rose() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rose"))
}

fn run(args: &[&str]) -> Output {
    rose().args(args).output().expect("lancement de rose")
}

#[test]
fn version_identique_avec_et_sans_court_circuit() {
    // Le court-circuit ne s'applique qu'à exactement 2 arguments : avec
    // un argument de plus, l'ancien chemin doit rendre la MÊME version.
    let court = run(&["version"]);
    let long = run(&["version", "--trace-inert"]);
    assert!(court.status.success() && long.status.success());
    assert_eq!(court.stdout, long.stdout, "version doit être bit-identique");
    assert!(court.stdout.starts_with(b"Rose "));
}

#[test]
fn version_longue_et_alias_sont_identiques() {
    assert_eq!(run(&["--version"]).stdout, run(&["version"]).stdout);
}

#[test]
fn aide_est_identique_sur_les_trois_alias() {
    let reference = run(&["--help"]).stdout;
    assert!(!reference.is_empty());
    assert_eq!(run(&["-h"]).stdout, reference);
    assert_eq!(run(&["aide"]).stdout, reference);
    // Le chemin historique (`help` avec 2 arguments) reste inchangé.
    assert_eq!(run(&["help"]).stdout, reference);
}

#[test]
fn info_reste_identique() {
    let out = run(&["info"]).stdout;
    assert!(String::from_utf8_lossy(&out).contains("Rose — langage"));
}

#[test]
fn trace_startup_decompose_sans_toucher_stdout() {
    let sortie = rose()
        .arg("bench/perf/src/startup.rose")
        .env("ROSE_STARTUP_TRACE", "1")
        .output();
    // Le fichier du banc peut ne pas être présent quand on exécute les
    // tests depuis une copie partielle : utiliser un programme écrit par
    // le test lui-même dans ce cas.
    let sortie = match sortie {
        Ok(o) if o.status.success() => o,
        _ => {
            let chemin = std::env::temp_dir().join("rose-opt-startup-trace.rose");
            std::fs::write(&chemin, ">> \"RESULT:0\"\n").unwrap();
            rose().arg(&chemin).env("ROSE_STARTUP_TRACE", "1").output().unwrap()
        }
    };
    let err = String::from_utf8_lossy(&sortie.stderr);
    assert!(err.contains("TRACE-STARTUP depuis_entree_main="), "{err}");
    for phase in ["console", "argv", "dispatch", "avant_execution"] {
        assert!(err.contains(&format!("TRACE-STARTUP {phase}=")), "phase {phase} absente : {err}");
    }
    // Aucune pollution de stdout par la trace.
    assert!(String::from_utf8_lossy(&sortie.stdout).contains("RESULT:0"));
    assert!(!String::from_utf8_lossy(&sortie.stdout).contains("TRACE-STARTUP"));
}

#[test]
fn sans_trace_aucune_ligne_sur_stderr() {
    let chemin = std::env::temp_dir().join("rose-opt-startup-silence.rose");
    std::fs::write(&chemin, ">> \"RESULT:0\"\n").unwrap();
    let o = rose().arg(&chemin).output().unwrap();
    assert!(o.status.success());
    assert!(o.stderr.is_empty(), "stderr non vide : {:?}", String::from_utf8_lossy(&o.stderr));
}
