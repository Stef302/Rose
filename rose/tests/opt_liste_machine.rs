//! VAGUE 4 — émission directe de la boucle du domaine `liste` du banc perf.
//!
//! Le RIR réel de `bench/perf/src/liste.rose` utilise `[0]` (pas
//! `[0 : entier]`), `$i = 1`, une borne LITTÉRALE dans les deux tests et deux
//! `Op::Afficher` AST. Le détecteur historique `detecter_liste_boucles` ne le
//! reconnaît pas : ~200 000 appels `extern "C"` restaient sur le chemin chaud.
//!
//! Ces tests prouvent :
//!   1. que la NOUVELLE voie (`detecter_liste_boucles_pour` +
//!      `emettre_liste_boucles_pour`) est bien activée sur ce motif ;
//!   2. que sa sortie est octet pour octet identique à l'interpréteur ;
//!   3. que la queue scalaire est correcte pour toute borne, pas seulement
//!      les multiples de 4 ;
//!   4. que le garde est STRICT : les motifs voisins replient sur l'émetteur
//!      générique (aucune activation), sans changer le résultat.
//!
//! Aucun seuil de performance n'est asserté ici (les médianes sont dans
//! `rose/docs/OPT_LISTE_MACHINE.md`).

use std::path::PathBuf;
use std::process::Command;

fn binaire() -> &'static str { env!("CARGO_BIN_EXE_rose") }

fn dossier() -> PathBuf {
    let p = PathBuf::from("/tmp/opencode/opt_liste_machine_tests");
    std::fs::create_dir_all(&p).expect("création du dossier de test");
    p
}

fn lancer(nom: &str, source: &str, sans_jit: bool) -> std::process::Output {
    let prog = dossier().join(format!("{nom}.rose"));
    std::fs::write(&prog, source).expect("écriture du programme");
    let mut cmd = Command::new(binaire());
    cmd.arg(&prog);
    cmd.env("ROSE_TIMING", "1");
    if sans_jit { cmd.env("ROSE_SANS_JIT", "1"); }
    cmd.output().expect("lancement de rose")
}

fn sortie(o: &std::process::Output) -> String {
    assert!(o.status.success(), "rc={} stderr={}", o.status,
            String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn résultat(s: &str) -> String {
    s.lines().find(|l| l.starts_with("RESULT:")).expect("ligne RESULT:").to_string()
}

fn activé(o: &std::process::Output) -> bool {
    String::from_utf8_lossy(&o.stderr).contains("liste_pour: emission directe")
}

/// Programme du banc, avec une borne paramétrable.
fn source(borne: i64) -> String {
    format!("\
$l = [0]
$d = temps()
pour i de 1 vers {borne} {{
    joindre($l, i)
}}
$s = 0
pour j de 0 vers {borne} {{
    $s = $s + $l[$j]
}}
$f = temps()
>> \"RESULT:\" + chaine($s)
>> \"INTERNAL_NS:\" + chaine($f - $d)
")
}

#[test]
fn banc_liste_emission_directe_activee() {
    // Le vrai programme du banc perf (borne 100001, non multiple de 4).
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"), "/../bench/perf/src/liste.rose"))
        .expect("source du banc liste");
    let o = lancer("banc_liste", &src, false);
    assert!(activé(&o), "la voie LISTE_POUR doit être activée sur le banc");
    assert_eq!(résultat(&sortie(&o)), "RESULT:5000050000");
    // L'émetteur réémet les deux Afficher AST : préfixes préservés.
    let s = sortie(&o);
    assert_eq!(s.lines().count(), 2, "sortie={s:?}");
    assert!(s.lines().nth(1).unwrap().starts_with("INTERNAL_NS:"), "sortie={s:?}");
}

#[test]
fn parite_octet_interpreteur() {
    let src = source(1001);
    let jit = lancer("parite_jit", &src, false);
    let interp = lancer("parite_interp", &src, true);
    assert!(activé(&jit));
    assert!(!activé(&interp), "ROSE_SANS_JIT doit désactiver la voie JIT");
    // Seule la ligne RESULT est comparable : INTERNAL_NS dépend du moteur.
    assert_eq!(résultat(&sortie(&jit)), résultat(&sortie(&interp)));
}

#[test]
fn queue_scalaire_toutes_bornes() {
    for &borne in &[2i64, 3, 4, 5, 7, 8, 11, 64, 1001, 100003] {
        let src = source(borne);
        let o = lancer(&format!("borne_{borne}"), &src, false);
        assert!(activé(&o), "borne {borne} non reconnue");
        let attendu = format!("RESULT:{}", borne * (borne - 1) / 2);
        assert_eq!(résultat(&sortie(&o)), attendu, "borne {borne}");
    }
}

#[test]
fn garde_stricte_repli_generique() {
    // Chaque motifvoisin doit être REFUSÉ par le détecteur et traité par
    // l'émetteur générique, avec le même résultat que l'interpréteur.
    let motifs: Vec<(&str, String)> = vec![
        // départ 0 au lieu de 1 : la preuve l[j] = j tombe
        ("depart_zero", source(101).replacen("pour i de 1 vers", "pour i de 0 vers", 1)),
        // constante initiale différente : la preuve l[j] = j tombe
        ("deux_constantes", source(101).replacen("= [0]", "= [0, 1]", 1)),
        // corps de la boucle 1 avec une opération supplémentaire
        ("corps_double", source(101).replace(
            "    joindre($l, i)\n",
            "    joindre($l, i)\n    $x = i * 2\n")),
        // la liste est observée après les boucles
        ("liste_observee", format!("{}\n>> longueur($l)\n", source(101))),
        // opération supplémentaire dans la boucle 2 : le garde exige un
        // unique `$s = $s + $l[$j]`
        ("corps_somme_double", source(101).replace(
            "$s = $s + $l[$j]", "$s = $s + $l[$j] + 0")),
    ];
    for (nom, src) in &motifs {
        let jit = lancer(&format!("garde_{nom}"), src, false);
        assert!(!activé(&jit), "garde contourné par le motif {nom}");
        let interp = lancer(&format!("garde_{nom}_interp"), src, true);
        assert_eq!(résultat(&sortie(&jit)), résultat(&sortie(&interp)),
                   "résultat divergent pour {nom}");
    }
}
