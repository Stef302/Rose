//! OPT_INTLOOP_FINAL — exactitude du déroulage ×4 de la boucle entière nue.
//!
//! Le correctif ne touche QUE la voie `boucle_nu` de `jit.rs`
//! (`motif_acc_plus_ctr_unroll` + son émetteur). Ces tests verrouillent :
//!   1. le résultat du banc `int_loop` (1 000 000 tours) ;
//!   2. le RESTE de la boucle déroulée (bornes 1..9 → 0, 1, 2, 3 itérations
//!      résiduelles) ;
//!   3. les valeurs initiales non canoniques (compteur ≠ 1, accumulateur
//!      ≠ 0, accumulateur négatif), qui doivent emprunter le MÊME chemin ;
//!   4. le repli sur la boucle générique vérifiée quand le garde échoue
//!      (borne > 2^20 ou corps non strict) ;
//!   5. la parité JIT ↔ interpréteur sur chacun de ces programmes.
//!
//! Aucun seuil de temps n'est asserts : les durées sont des constats datés,
//! pas des contrats de test (voir `rose/tests/opt_jit.rs`).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const DIR: &str = "/tmp/opencode/opt_intloop_final";

fn binaire() -> &'static str {
    env!("CARGO_BIN_EXE_rose")
}

fn ecrire(nom: &str, source: &str) -> PathBuf {
    std::fs::create_dir_all(DIR).expect("création du dossier de test");
    let p = Path::new(DIR).join(format!("{nom}.rose"));
    std::fs::write(&p, source).expect("écriture du programme de test");
    p
}

fn lancer(prog: &Path, sans_jit: bool) -> Output {
    let mut cmd = Command::new(binaire());
    cmd.arg(prog).env("ROSE_DUMP_RIR", "1");
    if sans_jit { cmd.env("ROSE_SANS_JIT", "1"); }
    cmd.output().expect("lancement de Rose")
}

fn resultat(out: &Output) -> String {
    let txt = String::from_utf8_lossy(&out.stdout);
    let ligne = txt.lines().find(|l| l.starts_with("RESULT:"))
        .unwrap_or_else(|| panic!("pas de ligne RESULT dans {:?}", txt));
    ligne.trim_start_matches("RESULT:").trim().to_string()
}

fn verifier(nom: &str, source: &str, attendu: &str, doit_jit: bool) {
    let prog = ecrire(nom, source);
    let jit = lancer(&prog, false);
    assert!(jit.status.success(), "JIT a échoué sur {nom} : {}",
            String::from_utf8_lossy(&jit.stderr));
    assert_eq!(resultat(&jit), attendu, "résultat JIT faux sur {nom}");
    let interp = lancer(&prog, true);
    assert!(interp.status.success(), "interpréteur a échoué sur {nom}");
    assert_eq!(resultat(&interp), attendu, "résultat interpréteur faux sur {nom}");
    assert_eq!(jit.stdout, interp.stdout, "divergence JIT ↔ interpréteur sur {nom}");
    let trace_nu = String::from_utf8_lossy(&jit.stderr).contains("VAGUE2 boucle-nu test=");
    assert_eq!(trace_nu, doit_jit,
               "voie boucle-nu inattendue sur {nom} (attendu {doit_jit})");
}

fn programme(s_init: &str, i_init: &str, borne: usize) -> String {
    // La variable de boucle `i` est déclarée par `pour` : ne PAS la précéder
    // d'un `$i = ...` (le front-end refuserait « variable existe déjà »).
    format!("$s = {s_init}\npour i de {i_init} vers {borne} {{\n  $s = $s + i\n}}\n>> \"RESULT:\" + chaine($s)\n")
}

#[test]
fn banc_int_loop_reference() {
    let src = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../bench/perf/src/int_loop.rose")
    ).expect("source du banc");
    // La source du banc contient aussi temps()/INTERNAL_NS : on compare
    // seulement RESULT, identique dans toutes les voies.
    let prog = ecrire("banc_int_loop", &src);
    let jit = lancer(&prog, false);
    assert!(jit.status.success());
    assert_eq!(resultat(&jit), "500000500000");
}

#[test]
fn reste_de_la_boucle_unroulee() {
    // 250000 tours ×4 + 3 tours résiduels dans le cas du banc ; ici on
    // balaie tous les restes possibles (borne - depart = 1..9).
    for n in 1..=9usize {
        // `pour i de 1 vers 1+n` => i = 1..n (n tours), tous les restes
        // possibles de la boucle déroulée ×4 (n mod 4 = 0,1,2,3).
        let somme: i128 = (1..=n as i128).sum();
        verifier(&format!("reste_{n}"), &programme("0", "1", n + 1),
                 &somme.to_string(), true);
    }
}

#[test]
fn departs_non_canoniques() {
    // compteur initial 3, borne 12 -> i = 3..11, somme = 63 + 3*9 = 90.
    verifier("depart_3", &programme("0", "3", 12), "63", true);
    // accumulateur initial 10, i = 1..4 -> 10 + 1+2+3+4 = 20.
    verifier("acc_10", &programme("10", "1", 5), "20", true);
    // accumulateur négatif : -100 + (1+..+6) = -79.
    verifier("acc_neg", &programme("-100", "1", 7), "-79", true);
    // départ 2, borne 9 -> i = 2..8, somme = 35 ; avec acc = -35 -> 0.
    verifier("acc_annule", &programme("-35", "2", 9), "0", true);
}

#[test]
fn repli_boucle_generique() {
    // Borne > 2^20 : le garde `boucle_nu` échoue, la boucle vérifiée
    // s'exécute ; le résultat doit rester exact.
    let borne = (1usize << 20) + 4;
    let somme: i128 = (1..borne as i128).sum();
    verifier("hors_garde_borne", &programme("0", "1", borne),
             &somme.to_string(), false);
    // Corps non strict (deux accumulateurs) : le détecteur de déroulage
    // refuse, l'émetteur nu générique prend le relais.
    let src = "$s = 0\n$t = 0\npour i de 1 vers 21 {\n  $s = $s + i\n  $t = $t + $s\n}\n>> \"RESULT:\" + chaine($t)\n";
    let prog = ecrire("corps_non_strict", src);
    let jit = lancer(&prog, false);
    assert!(jit.status.success());
    let interp = lancer(&prog, true);
    assert_eq!(jit.stdout, interp.stdout, "divergence sur corps_non_strict");
    let t: i128 = (1..=20i128).map(|i| (i * (i + 1)) / 2).sum();
    assert_eq!(resultat(&jit), t.to_string());
}
