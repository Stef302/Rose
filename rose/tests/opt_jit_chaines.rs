//! VAGUE 3 — micro-banc de la concaténation chaîne sur le chemin JIT.
//!
//! Mesure le coût réel de `$s = $s + "x"` (1 000 / 10 000 / 20 000 morceaux)
//! dans le binaire Rose compilé, en mode JIT (défaut) et en mode interprété
//! (`ROSE_SANS_JIT=1`, référence de parité). 3 échantillons par point,
//! médianes — les durées sont des CONSTATS DATÉS, jamais des seuils de test.
//!
//! Les assertions portent sur la parité octet pour octet des résultats et sur
//! un cas d'aliasage (`$t = $s` puis `$s = $s + "x"`) : toute optimisation
//! qui muterait le `Valeur` derrière un handle partagé est détectée ici.
//!
//! Usage :
//!   RUST_TEST_THREADS=1 cargo test --release --test opt_jit_chaines -- --nocapture

use std::path::PathBuf;
use std::process::{Command, Output};

const DIR: &str = "/tmp/opencode/opt_jit_chaines_tests";

fn binaire() -> &'static str {
    env!("CARGO_BIN_EXE_rose")
}

fn mediane(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn lancer(prog: &PathBuf, sans_jit: bool) -> Output {
    let mut cmd = Command::new(binaire());
    cmd.arg(prog);
    if sans_jit { cmd.env("ROSE_SANS_JIT", "1"); }
    cmd.output().expect("lancement de rose")
}

fn champ(txt: &str, cle: &str) -> String {
    txt.lines().find(|l| l.starts_with(cle))
        .unwrap_or_else(|| panic!("ligne {cle} absente : {txt}"))
        .to_string()
}

fn ns_interne(ligne: &str) -> f64 {
    ligne.strip_prefix("INTERNAL_NS:").unwrap().parse::<f64>().unwrap()
}

fn source(n: usize) -> String {
    format!(
        "$s = \"\"\n$d = temps()\npour i de 1 vers {} {{ $s = $s + \"x\" }}\n$f = temps()\n\
         >> \"RESULT:\" + chaine(longueur($s))\n>> \"INTERNAL_NS:\" + chaine($f - $d)\n",
        n + 1
    )
}

#[test]
fn micro_banc_concat_jit() {
    std::fs::create_dir_all(DIR).expect("création du dossier du micro-banc");
    println!("\n=== MICRO-BANC CONCAT JIT (3 échantillons, médianes) ===");
    println!("{:<8} {:>13} {:>13} {:>10}", "morceaux", "jit_ms", "interp_ms", "x(interp/jit)");
    for &n in &[1000usize, 10_000, 20_000] {
        let prog = PathBuf::from(DIR).join(format!("concat_{n}.rose"));
        std::fs::write(&prog, source(n)).expect("écriture du programme");
        let mut jit = Vec::new();
        let mut interp = Vec::new();
        let mut jout = String::new();
        let mut iout = String::new();
        for _ in 0..3 {
            let o = lancer(&prog, false);
            assert!(o.status.success(), "JIT rc={} stderr={}", o.status,
                    String::from_utf8_lossy(&o.stderr));
            jout = String::from_utf8_lossy(&o.stdout).into_owned();
            jit.push(ns_interne(&champ(&jout, "INTERNAL_NS:")));
            let o = lancer(&prog, true);
            assert!(o.status.success(), "interp rc={} stderr={}", o.status,
                    String::from_utf8_lossy(&o.stderr));
            iout = String::from_utf8_lossy(&o.stdout).into_owned();
            interp.push(ns_interne(&champ(&iout, "INTERNAL_NS:")));
        }
        let j = mediane(jit) / 1e6;
        let s = mediane(interp) / 1e6;
        assert_eq!(champ(&jout, "RESULT:"), format!("RESULT:{n}"));
        assert_eq!(champ(&jout, "RESULT:"), champ(&iout, "RESULT:"), "parité JIT/interp");
        println!("{:<8} {:>13.4} {:>13.4} {:>10.2}x", n, j, s, s / j);
    }
}

/// Les spécialisations doivent conserver la sémantique UTF-8 (comptage en
/// POINTS DE CODE) et le compteur de boucle, y compris avec un préfixe non
/// vide et un morceau multi-octets.
#[test]
fn parite_unicode_accumulation() {
    std::fs::create_dir_all(DIR).expect("création du dossier du micro-banc");
    let prog = PathBuf::from(DIR).join("unicode.rose");
    std::fs::write(&prog,
        "$s = \"début\"\n$d = temps()\n\
         pour i de 1 vers 101 { $s = $s + \"é🌍\" }\n$f = temps()\n\
         >> \"LEN:\" + chaine(longueur($s))\n>> \"I:\" + chaine($i)\n\
         >> \"S:\" + $s\n>> \"OK:\" + chaine($f - $d > 0)\n")
        .expect("écriture du programme");
    let j = lancer(&prog, false);
    let i = lancer(&prog, true);
    assert!(j.status.success() && i.status.success());
    let js = String::from_utf8_lossy(&j.stdout).into_owned();
    let is = String::from_utf8_lossy(&i.stdout).into_owned();
    assert_eq!(js, is, "parité Unicode JIT/interp rompue");
    assert!(js.contains("LEN:205") && js.contains("I:101"), "résultat faux : {js}");
}

/// Un handle JIT peut être partagé entre slots (`$t = $s`) : l'accumulation
/// ne doit PAS muter la `Valeur` derrière ce handle, sinon `$t` changerait.
#[test]
fn parite_aliasage_accumulation() {
    std::fs::create_dir_all(DIR).expect("création du dossier du micro-banc");
    let prog = PathBuf::from(DIR).join("alias.rose");
    std::fs::write(&prog,
        "$s = \"ab\"\n$t = $s\n$s = $s + \"x\"\n>> \"T:\" + $t\n>> \"S:\" + $s\n")
        .expect("écriture du programme");
    let j = lancer(&prog, false);
    let i = lancer(&prog, true);
    assert!(j.status.success() && i.status.success());
    let js = String::from_utf8_lossy(&j.stdout).into_owned();
    let is = String::from_utf8_lossy(&i.stdout).into_owned();
    assert!(js.contains("T:ab"), "aliasage cassé par le JIT : {js}");
    assert!(js.contains("S:abx"), "résultat faux : {js}");
    assert_eq!(js, is, "parité JIT/interp rompue");
}
