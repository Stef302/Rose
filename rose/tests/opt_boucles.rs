//! VAGUE 2 — boucles entières nues : parité et activation du codegen.
//!
//! Le JIT émet désormais une boucle sans checks de tag/overflow derrière un
//! garde exécuté une seule fois (`rir::boucle_nu`). Ces tests ne mesurent
//! AUCUN seuil de performance : ils prouvent (a) que la boucle rapide est
//! bien sélectionnée sur les motifs ciblés, (b) que sa sortie est octet pour
//! octet identique à l'interpréteur, y compris quand le garde doit replier,
//! (c) que les résultats restent exacts.

use std::path::PathBuf;
use std::process::Command;

fn binaire() -> &'static str { env!("CARGO_BIN_EXE_rose") }

fn dossier() -> PathBuf {
    let p = PathBuf::from("/tmp/opencode/opt_boucles_tests");
    std::fs::create_dir_all(&p).expect("création du dossier de test");
    p
}

fn lancer(nom: &str, source: &str, sans_jit: bool, trace: bool) -> std::process::Output {
    let prog = dossier().join(format!("{nom}.rose"));
    std::fs::write(&prog, source).expect("écriture du programme");
    let mut cmd = Command::new(binaire());
    cmd.arg(&prog);
    if sans_jit { cmd.env("ROSE_SANS_JIT", "1"); }
    if trace { cmd.env("ROSE_DUMP_RIR", "1"); }
    cmd.output().expect("lancement de rose")
}

fn sortie(o: &std::process::Output) -> String {
    assert!(o.status.success(), "rc={} stderr={}", o.status,
            String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn resultat(s: &str) -> &str {
    s.lines().find(|l| l.starts_with("RESULT:")).unwrap()
}

/// La trace `ROSE_DUMP_RIR=1` imprime `VAGUE2 boucle-nu …` uniquement quand
/// le détecteur a retenu la boucle et que l'émetteur a posé le garde.
fn boucle_nu_activee(nom: &str, source: &str) -> bool {
    let o = lancer(nom, source, false, true);
    assert!(o.status.success(), "stderr={}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stderr).contains("VAGUE2 boucle-nu")
}

const SOMME: &str = "\
$s = 0
$d = temps()
pour i de 1 vers 1000001 { $s = $s + i }
$f = temps()
>> \"RESULT:\" + chaine($s)
>> \"INTERNAL_NS:\" + chaine($f - $d)
";

const FIB: &str = "\
$a = 0
$b = 1
$d = temps()
pour i de 1 vers 1000001 {
    $t = ($a + $b) % 1000000007
    $a = $b
    $b = $t
}
$f = temps()
>> \"RESULT:\" + chaine($a)
>> \"INTERNAL_NS:\" + chaine($f - $d)
";

#[test]
fn somme_boucle_nu_activee_et_exacte() {
    assert!(boucle_nu_activee("v2_somme", SOMME), "boucle somme non spécialisée");
    let j = sortie(&lancer("v2_somme", SOMME, false, false));
    assert!(j.contains("RESULT:500000500000"), "sortie={j}");
    // Parité JIT ↔ interpréteur, ligne de chrono exclue.
    let i = sortie(&lancer("v2_somme", SOMME, true, false));
    assert_eq!(resultat(&j), resultat(&i));
}

#[test]
fn fib_boucle_nu_activee_et_exacte() {
    assert!(boucle_nu_activee("v2_fib", FIB), "boucle fib non spécialisée");
    let j = sortie(&lancer("v2_fib", FIB, false, false));
    assert!(j.contains("RESULT:918091266"), "sortie={j}");
    let i = sortie(&lancer("v2_fib", FIB, true, false));
    assert_eq!(resultat(&j), resultat(&i));
}

#[test]
fn parite_bornes_et_initials_variables() {
    // Borne plus petite, accumulateur non nul, pas de modulo : mêmes preuves.
    for (nom, borne, init, attendu) in [
        ("v2_p1", 10, 0i64, 45i64),
        ("v2_p2", 1000, -7, 499493),
        ("v2_p3", 1, 41, 41),
    ] {
        let src = format!(
            "$s = {init}\npour i de 1 vers {borne} {{ $s = $s + i }}\n>> chaine($s)\n");
        let j = sortie(&lancer(nom, &src, false, false));
        let i = sortie(&lancer(nom, &src, true, false));
        assert_eq!(j, i, "divergence JIT/interpréteur sur {nom}");
        assert_eq!(j.trim(), attendu.to_string(), "résultat faux sur {nom}");
    }
}

#[test]
fn parite_fib_modules_variables() {
    for (nom, borne, m) in [("v2_m1", 50, 7i64), ("v2_m2", 500, 97), ("v2_m3", 1000, 1000000007)] {
        let src = format!(
            "$a = 0\n$b = 1\npour i de 1 vers {borne} {{\n\
             $t = ($a + $b) % {m}\n$a = $b\n$b = $t\n}}\n>> chaine($a)\n");
        let j = sortie(&lancer(nom, &src, false, false));
        let i = sortie(&lancer(nom, &src, true, false));
        assert_eq!(j, i, "divergence JIT/interpréteur sur {nom}");
    }
}

#[test]
fn repli_quand_le_garde_ne_peut_pas_prouver() {
    // Borne > 2^20 : le détecteur refuse la voie nue, la boucle générique
    // vérifiée doit rendre le même résultat (y compris le boxage GMP).
    let src = "$s = 0\npour i de 1 vers 1048578 { $s = $s + i }\n>> chaine($s)\n";
    assert!(!boucle_nu_activee("v2_repli_borne", src));
    let j = sortie(&lancer("v2_repli_borne", src, false, false));
    let i = sortie(&lancer("v2_repli_borne", src, true, false));
    assert_eq!(j, i);
    // Valeur initiale au-delà du garde |v| ≤ 2^40 : repli également.
    let src2 = "$s = 1099511627776\npour i de 1 vers 10 { $s = $s + i }\n>> chaine($s)\n";
    let j2 = sortie(&lancer("v2_repli_valeur", src2, false, false));
    let i2 = sortie(&lancer("v2_repli_valeur", src2, true, false));
    assert_eq!(j2, i2);
    // `vers 10` est exclusif : 1..9, soit 45.
    assert_eq!(j2.trim(), "1099511627821");
}

#[test]
fn detecteur_unitaire() {
    // Le détecteur ne retient que les boucles dont il peut prouver la borne.
    use rose::front::analyser;
    let p = analyser("fn f() -> entier {\n $s = 0\n pour j de 1 vers 1000001 { $s = $s + j }\n retour $s\n}").unwrap();
    let sig = &p.signatures["f"];
    let rir = rose::rir::abaisser(sig.corps.as_ref().unwrap(), &[], &[]).unwrap();
    let b = rose::rir::boucle_nu(&rir).unwrap();
    assert_eq!(b.borne, 1000001);
    assert_eq!(b.garder, vec![0, 1]); // $s (slot 0) et i (slot 1)
    assert!(b.mod_rapide.is_none());

    let p2 = analyser("fn f() -> entier {\n $a = 0\n $b = 1\n pour j de 1 vers 1000001 {\n\
        $t = ($a + $b) % 1000000007\n $a = $b\n $b = $t\n }\n retour $a\n}").unwrap();
    let sig2 = &p2.signatures["f"];
    let rir2 = rose::rir::abaisser(sig2.corps.as_ref().unwrap(), &[], &[]).unwrap();
    let b2 = rose::rir::boucle_nu(&rir2).unwrap();
    let mr = b2.mod_rapide.expect("% m non reconnu");
    assert_eq!(mr.m, 1000000007);
    assert!(mr.a < 2 && mr.b < 2);
}
