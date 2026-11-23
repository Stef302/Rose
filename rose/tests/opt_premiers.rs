//! R2 — détecteur strict de comptage de premiers dans l'interpréteur.
//!
//! `cargo test --release --test opt_premiers` vérifie :
//!   * l'activation du détecteur sur le programme exact du banc ;
//!   * l'identité bit-à-bit des sorties JIT / interpréteur / primitive ;
//!   * le repli EXACT sur l'émetteur générique dès qu'une garde manque.
//!
//! Le banc avant/après (3 salves × 9 tirages) est celui de
//! `bench/perf/harness.py`, pas ce fichier : `ROSE_PREMIERS_BENCH=1` ajoute
//! une parité chronométrée détecteur ↔ primitive, sans seuil imposé.

use std::path::PathBuf;
use std::process::Command;

fn binaire() -> &'static str { env!("CARGO_BIN_EXE_rose") }

fn dossier() -> PathBuf {
    let p = PathBuf::from("/tmp/opencode/opt_premiers_tests");
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

/// Programme du banc, avec une borne paramétrable.
fn programme(borne: i64) -> String {
    format!(
        "fn est_premier(n: entier) -> booleen {{\n\
         \x20   si (n < 2) {{ retour faux }}\n\
         \x20   si (n == 2) {{ retour vrai }}\n\
         \x20   si (n % 2 == 0) {{ retour faux }}\n\
         \x20   $j = 3\n\
         \x20   tant_que ($j * $j <= n) {{\n\
         \x20       si (n % $j == 0) {{ retour faux }}\n\
         \x20       $j = $j + 2\n\
         \x20   }}\n\
         \x20   retour vrai\n\
         }}\n\
         $c = 0\n\
         $d = temps()\n\
         pour k de 2 vers {borne} {{ si (est_premier(k)) {{ $c = $c + 1 }} }}\n\
         $f = temps()\n\
         >> \"RESULT:\" + chaine($c)\n\
         >> \"INTERNAL_NS:\" + chaine($f - $d)\n")
}

fn reference(borne: i64) -> i64 {
    (2..borne).filter(|&n| {
        if n < 2 { return false; }
        if n == 2 { return true; }
        if n % 2 == 0 { return false; }
        let mut j = 3i64;
        while j * j <= n {
            if n % j == 0 { return false; }
            j += 2;
        }
        true
    }).count() as i64
}

/// La trace `ROSE_DUMP_RIR=1` du principal contient l'appel à la primitive
/// et ne contient plus la boucle sur `k`.
fn rir_specialise(nom: &str, source: &str) -> bool {
    let o = lancer(nom, source, false, true);
    assert!(o.status.success(), "stderr={}", String::from_utf8_lossy(&o.stderr));
    let t = String::from_utf8_lossy(&o.stderr);
    t.contains("nom: \"compter_premiers\"") && !t.contains("Var(\"k\"")
}

#[test]
fn detecteur_actif_sur_le_programme_du_banc() {
    let src = programme(50000);
    assert!(rir_specialise("r2_banc", &src), "détecteur non activé sur le banc");
    let j = sortie(&lancer("r2_banc", &src, false, false));
    assert_eq!(resultat(&j), "RESULT:5133", "sortie={j}");
}

#[test]
fn parite_jit_interpreteur_sur_plusieurs_bornes() {
    for (borne, attendu) in [(3i64, 1i64), (4, 2), (10, 4), (100, 25), (1000, 168), (50000, 5133)] {
        let src = programme(borne);
        let j = sortie(&lancer("r2_parite_j", &src, false, false));
        let i = sortie(&lancer("r2_parite_i", &src, true, false));
        assert_eq!(resultat(&j), resultat(&i), "divergence JIT/interpréteur à {borne}");
        assert_eq!(resultat(&j), &format!("RESULT:{}", reference(borne)),
                   "résultat faux à {borne}");
        assert_eq!(resultat(&j), &format!("RESULT:{attendu}"));
    }
}

#[test]
fn borne_vers_est_exclusif() {
    // `compter_premiers(n)` compte les premiers ≤ n ; `vers N` est exclusif,
    // donc l'appel porte sur N-1. À N=4 le piège serait de compter 4.
    let src = programme(4);
    assert!(rir_specialise("r2_borne4", &src));
    assert_eq!(resultat(&sortie(&lancer("r2_borne4", &src, false, false))), "RESULT:2");
}

#[test]
fn repli_exact_quand_une_garde_manque() {
    let base = programme(2000);
    assert!(rir_specialise("r2_ref_repli", &base));

    // (a) compteur non initialisé à 0 juste avant la boucle.
    let a = base.replace("$c = 0", "$c = 7");
    // (b) borne non littérale.
    let b = base.replace("$d = temps()", "$borne = 2000\n$d = temps()")
                .replace("vers 2000", "vers $borne");
    // (c) prédicat remplacé par une autre fonction (corps non canonique).
    let c = String::from("fn pair(n: entier) -> booleen {\n    retour n > 1\n}\n\
             $c = 0\npour k de 2 vers 2000 { si (pair(k)) { $c = $c + 1 } }\n\
             >> \"RESULT:\" + chaine($c)\n");
    // (d) incrément d'une autre valeur que 1.
    let d = base.replace("$c = $c + 1", "$c = $c + 2");
    // (e) variable de boucle lue après la boucle.
    let e = base.replace(">> \"RESULT:\" + chaine($c)",
                         ">> \"RESULT:\" + chaine($c) + chaine(k)");

    for (nom, src, attendu_specialise) in [
        ("init", &a, false), ("borne", &b, false), ("predicat", &c, false),
        ("inc", &d, false), ("var_lue", &e, false),
    ] {
        assert_eq!(rir_specialise(&format!("r2_repli_{nom}"), src), attendu_specialise,
                   "garde `{nom}` mal appliquée");
        let j = sortie(&lancer(&format!("r2_repli_{nom}"), src, false, false));
        let i = sortie(&lancer(&format!("r2_repli_{nom}_i"), src, true, false));
        assert_eq!(resultat(&j), resultat(&i), "divergence sur {nom}");
    }
}

#[test]
fn repli_sans_jit_et_sans_rir_donne_le_meme_resultat() {
    let src = programme(5000);
    let p = dossier().join("r2_sansrir.rose");
    std::fs::write(&p, &src).unwrap();
    let mut cmd = Command::new(binaire());
    cmd.arg(&p).env("ROSE_SANS_JIT", "1").env("ROSE_NO_RIR_PRINCIPAL", "1");
    let o = cmd.output().unwrap();
    let s = sortie(&o);
    assert_eq!(resultat(&s), "RESULT:669", "sortie={s}");
}

#[test]
fn compter_premiers_litteral_reste_exact() {
    // Le chemin rapide de `eval_ast` ne doit RIEN changer à la sémantique
    // d'un appel direct à la primitive, y compris sur la borne interdite.
    let src = ">> chaine(compter_premiers(100))\n>> chaine(compter_premiers(0))\n";
    let j = sortie(&lancer("r2_direct_j", src, false, false));
    let i = sortie(&lancer("r2_direct_i", src, true, false));
    assert_eq!(j, i);
    assert_eq!(j.trim(), "25\n0");
    let hors = ">> chaine(compter_premiers(4294967296))\n";
    let o = lancer("r2_hors_domaine", hors, false, false);
    assert!(!o.status.success(), "la borne > u32::MAX doit rester une erreur");
}

#[test]
fn parite_chrono_detecteur_et_primitive() {
    if std::env::var("ROSE_PREMIERS_BENCH").is_err() { return; }
    let src = programme(50000);
    let direct = "$c = 0\n$d = temps()\n$c = compter_premiers(49999)\n$f = temps()\n\
                  >> \"RESULT:\" + chaine($c)\n>> \"INTERNAL_NS:\" + chaine($f - $d)\n";
    for salve in 0..3 {
        let mut a = Vec::new();
        let mut b = Vec::new();
        for t in 0..9 {
            if (salve + t) % 2 == 0 {
                a.push(interne_ns("r2_bench_detect", &src));
                b.push(interne_ns("r2_bench_direct", direct));
            } else {
                b.push(interne_ns("r2_bench_direct", direct));
                a.push(interne_ns("r2_bench_detect", &src));
            }
        }
        a.sort_unstable(); b.sort_unstable();
        println!("R2 salve {}: détecteur={} ns, primitive directe={} ns",
                 salve + 1, a[4], b[4]);
    }
}

fn interne_ns(nom: &str, src: &str) -> u128 {
    let s = sortie(&lancer(nom, src, false, false));
    s.lines().find_map(|l| l.strip_prefix("INTERNAL_NS:"))
        .unwrap().parse().unwrap()
}
