// VAGUE FINALE — garde-fous du TEMPS MUR (constats datés du 23/11/2026).
//
// Ces tests ne mesurent PAS des durées : sur une machine partagée une durée
// est bruitée et un seuil serait faux dès la salve suivante. Ils verrouillent
// ce qui a ÉTÉ MESURÉ et ce qui ne doit PAS régresser pendant les optimisations
// du mur — voir `docs/OPT_WALL_FINAL.md` pour les chiffres, le protocole
// A/B/A entrelacé (21 lots, pin c15, perf_counter_ns, sha binaire) et les
// trois leviers REJETÉS (non-PIE, lazy binding, RELR/lld).
//
// En particulier, `elf_configuration_mesuree` échoue volontairement si un
// drapeau d'édition de liens change : toute modification du linking doit
// être RE-MESURÉE avec le harnais avant d'être acceptée, pas seulement
// « essayée ». Les commandes informationnelles sont couvertes ici par le
// contrôle de stabilité bit-à-bit (elles sont déjà testées en détail dans
// `opt_startup.rs`).

use std::process::{Command, Output};

fn rose() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rose"))
}

fn run(args: &[&str]) -> Output {
    rose().args(args).output().expect("lancement de rose")
}

/// Programme minimal écrit par le test : aucune dépendance à un fichier du
/// dépôt (le binaire de test peut tourner depuis une copie partielle).
fn programme_minimal() -> std::path::PathBuf {
    let chemin = std::env::temp_dir().join("rose-opt-wall-final.rose");
    std::fs::write(&chemin, ">> \"RESULT:0\"\n>> \"INTERNAL_NS:0\"\n").unwrap();
    chemin
}

#[test]
fn version_stable_et_conforme_sur_trois_lancements() {
    let a = run(&["version"]);
    let b = run(&["version"]);
    let c = run(&["version"]);
    assert!(a.status.success() && b.status.success() && c.status.success());
    assert_eq!(a.stdout, b.stdout);
    assert_eq!(a.stdout, c.stdout);
    assert!(a.stdout.starts_with(b"Rose "), "version inattendue");
    assert!(a.stderr.is_empty(), "stderr pollué");
}

#[test]
fn programme_minimal_jit_reste_conforme() {
    let chemin = programme_minimal();
    let o = rose().arg(&chemin).output().unwrap();
    assert!(o.status.success(), "rc={} : {}", o.status, String::from_utf8_lossy(&o.stderr));
    assert!(String::from_utf8_lossy(&o.stdout).contains("RESULT:0"));
    assert!(String::from_utf8_lossy(&o.stdout).contains("INTERNAL_NS:0"));
    assert!(o.stderr.is_empty(), "stderr non vide : {:?}", String::from_utf8_lossy(&o.stderr));
}

#[test]
fn ventilation_timing_ne_pollue_pas_stdout() {
    let chemin = programme_minimal();
    let o = rose()
        .arg(&chemin)
        .env("ROSE_TIMING", "1")
        .output()
        .unwrap();
    assert!(o.status.success());
    // La ventilation JIT (compiletout / jitnew) doit rester sur stderr :
    // les oracles du banc lisent stdout et restent bit-identiques.
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("TIMING"), "ventilation JIT absente : {err}");
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(out.contains("RESULT:0"));
    assert!(!out.contains("TIMING"));
}

/// Garde de CONFIGURATION, pas de performance : le binaire mesuré est un
/// static-pie avec relocations RELATIV et BIND_NOW. Si un levier d'édition
/// de liens est appliqué (non-PIE / lazy / RELR), ce test échoue et impose
/// de re-mesurer le mur avant d'accepter le changement.
#[cfg(target_os = "linux")]
#[test]
fn elf_configuration_mesuree() {
    let exe = env!("CARGO_BIN_EXE_rose");
    let bytes = std::fs::read(exe).unwrap();
    assert!(bytes.len() > 6 && &bytes[0..4] == b"\x7fELF", "{exe} n'est pas un ELF");

    // `readelf` peut être absent d'une image CI minimale : dans ce cas on
    // saute le contrôle plutôt que de faire échouer un build sain.
    let dyno = match Command::new("readelf").arg("-d").arg(exe).output() {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
        _ => return,
    };
    assert!(dyno.contains("BIND_NOW"),
        "BIND_NOW absent : le lazy binding doit être re-mesuré (OPT_WALL_FINAL)");
    assert!(dyno.contains("PIE"),
        "PIE absent : le non-PIE perd l'ASLR et doit être re-mesuré (OPT_WALL_FINAL)");

    let relo = match Command::new("readelf").arg("-r").arg(exe).output() {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
        _ => return,
    };
    assert!(relo.contains("RELATIV"),
        "relocations RELATIV absentes : configuration statique non mesurée (OPT_WALL_FINAL)");
}
