//! QAA (vague 53) — tests de bout en bout de la refonte « grands nombres façon Wolfram ».
//!
//! Principe : on compile le binaire `rose` via `CARGO_BIN_EXE_rose`, on écrit de
//! vrais programmes `.rose` dans `/tmp/opencode/qaa_agents/tests/`, on les
//! exécute et on vérifie la SORTIE RÉELLE (never une intention, jamais une
//! valeur recopiée d'un document).
//!
//! Mécanisme de skip documenté — INT1 câble encore `entier_geant_factorielle`,
//! `detail`, `chiffres` et `plus_de_chiffres` dans `interp.rs`. Tant qu'une
//! primitive répond « Fonction '…' inconnue », le test concerné affiche un
//! `eprintln!("SKIP …")` et rend la main SANS inventer de résultat. Dès que
//! INT1 a livré, les mêmes tests s'exécutent normalement : aucune modification
//! n'est nécessaire pour les activer.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Instant;

const DIR: &str = "/tmp/opencode/qaa_agents/tests";

/// Binaire compilé par Cargo pour CE test (jamais `rose/rose`, jamais
/// `rose/target/release/rose` écrit à la main).
fn binaire() -> &'static str {
    env!("CARGO_BIN_EXE_rose")
}

fn ecrire_programme(nom: &str, source: &str) -> PathBuf {
    fs::create_dir_all(DIR).expect("création du dossier de tests QAA");
    let p = PathBuf::from(DIR).join(nom);
    fs::write(&p, source).expect("écriture du programme .rose");
    p
}

fn lancer(prog: &Path) -> Output {
    Command::new(binaire()).arg(prog).output().expect("lancement de rose")
}

fn texte(o: &Output) -> String {
    let mut s = String::from_utf8_lossy(&o.stdout).into_owned();
    s.push_str(&String::from_utf8_lossy(&o.stderr));
    s
}

/// Exécute un programme et saute le test (avec trace) si une primitive QAA
/// n'est pas encore câblée. Toute autre erreur est laissée au test : c'est
/// une anomalie réelle, pas un skip.
fn lancer_ou_skip(nom: &str, source: &str) -> Option<Output> {
    let p = ecrire_programme(nom, source);
    let o = lancer(&p);
    let t = texte(&o);
    for prim in [
        "entier_geant_factorielle",
        "detail",
        "chiffres",
        "plus_de_chiffres",
        "afficher_detail",
    ] {
        if t.contains(&format!("'{prim}' inconnue")) {
            eprintln!(
                "SKIP {nom} : primitive `{prim}` pas encore câblée (INT1 en cours) — sortie réelle : {:?}",
                t.trim()
            );
            return None;
        }
    }
    Some(o)
}

/// Compare malgré les séparateurs de milliers (« 5 565 709 » vs « 5565709 »).
fn sans_espaces(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn lignes(o: &Output) -> Vec<String> {
    texte(o)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

// ── 1. 100! : 158 chiffres, préfixe, 24 zéros ──────────────────────────────

#[test]
fn cent_facteurielle_affichage_30_chiffres() {
    let o = match lancer_ou_skip(
        "qaa_100_affichage.rose",
        ">> entier_geant_factorielle(100)\n",
    ) {
        Some(o) => o,
        None => return,
    };
    assert!(o.status.success(), "rose a échoué : {:?}", texte(&o));
    let l = lignes(&o);
    assert_eq!(l.len(), 1, "une seule ligne attendue, obtenu : {l:?}");
    assert!(
        l[0].starts_with("933262154439441526816992388562"),
        "préfixe faux : {:?}",
        l[0]
    );
    assert!(l[0].ends_with('…'), "ellipse manquante : {:?}", l[0]);
    assert_eq!(
        l[0].chars().count(),
        31,
        "30 chiffres + « … » attendus, obtenu : {:?}",
        l[0]
    );
}

#[test]
fn cent_facteurielle_detail_longueur_et_zeros() {
    let o = match lancer_ou_skip(
        "qaa_100_detail.rose",
        ">> detail(entier_geant_factorielle(100))\n",
    ) {
        Some(o) => o,
        None => return,
    };
    assert!(o.status.success(), "rose a échoué : {:?}", texte(&o));
    let t = sans_espaces(&texte(&o));
    assert!(t.contains("933262154439441526816992388562"), "préfixe absent : {t}");
    assert!(t.contains("158"), "longueur 158 absente : {t}");
    assert!(t.contains("24"), "24 zéros finaux absents : {t}");
}

// ── 2. 1 000 000! : 5 565 709 chiffres, 249 998 zéros ─────────────────────

#[test]
fn un_million_facteurielle_detail_sans_materialiser() {
    let o = match lancer_ou_skip(
        "qaa_1m_detail.rose",
        ">> detail(entier_geant_factorielle(1000000))\n",
    ) {
        Some(o) => o,
        None => return,
    };
    assert!(o.status.success(), "rose a échoué : {:?}", texte(&o));
    let t = sans_espaces(&texte(&o));
    assert!(t.contains("5565709"), "longueur 5 565 709 absente : {t}");
    assert!(t.contains("249998"), "249 998 zéros absents : {t}");
    assert!(
        t.contains("826393168833124006237664610317"),
        "préfixe 1000000! absent : {t}"
    );
    // Un détail ne doit PAS déverser les 5,5 millions de chiffres.
    assert!(
        texte(&o).len() < 4_000,
        "sortie démesurée ({:?} octets) : le nombre a été matérialisé",
        texte(&o).len()
    );
}

// ── 3. (10^37)! : longueur, zéros, préfixe géants ──────────────────────────

#[test]
fn dix_puissance_37_detail_proprietes_exactes() {
    let o = match lancer_ou_skip(
        "qaa_10p37_detail.rose",
        ">> detail(entier_geant_factorielle(10^37))\n",
    ) {
        Some(o) => o,
        None => return,
    };
    assert!(o.status.success(), "rose a échoué : {:?}", texte(&o));
    let t = sans_espaces(&texte(&o));
    assert!(
        t.contains("901820028144300502340101362711"),
        "préfixe (10^37)! absent : {t}"
    );
    assert!(
        t.contains("2499999999999999999999999999999999990"),
        "zéros finaux absents : {t}"
    );
    assert!(
        t.contains("365657055180967481723488710810833949196"),
        "longueur absente : {t}"
    );
}

#[test]
fn dix_puissance_37_affichage_sans_materialisation_est_rapide() {
    let p = ecrire_programme(
        "qaa_10p37_affichage.rose",
        ">> entier_geant_factorielle(10^37)\n",
    );
    let premier = lancer(&p);
    if texte(&premier).contains("'entier_geant_factorielle' inconnue") {
        eprintln!(
            "SKIP dix_puissance_37_affichage_sans_materialisation_est_rapide : primitive pas encore câblée"
        );
        return;
    }
    assert!(
        premier.status.success(),
        "rose a échoué : {:?}",
        texte(&premier)
    );
    let l = lignes(&premier);
    assert_eq!(l.len(), 1, "une seule ligne attendue : {l:?}");
    assert!(
        l[0].starts_with("901820028144300502340101362711") && l[0].ends_with('…'),
        "affichage court incorrect : {:?}",
        l[0]
    );

    // 3 mesures entrelacées, médiane : on publie un temps, pas une spec.
    let mut dures = Vec::new();
    for _ in 0..3 {
        let t = Instant::now();
        let o = lancer(&p);
        dures.push(t.elapsed());
        assert!(o.status.success(), "échec pendant la mesure");
    }
    dures.sort();
    let med = dures[1];
    eprintln!("(10^37)! : affichage médian = {:?} (3 mesures : {dures:?})", med);
    assert!(
        med.as_secs_f64() < 1.0,
        "affichage trop lent ({:?}) : le nombre est probablement matérialisé",
        med
    );
}

// ── 4. plus_de_chiffres : 30 → 60, préfixe stable ──────────────────────────

#[test]
fn plus_de_chiffres_30_vers_60_prefixe_stable() {
    let o = match lancer_ou_skip(
        "qaa_plus_de_chiffres.rose",
        concat!(
            "$o = entier_geant_factorielle(100)\n",
            ">> chiffres($o, 30)\n",
            ">> plus_de_chiffres($o, 30)\n",
        ),
    ) {
        Some(o) => o,
        None => return,
    };
    assert!(o.status.success(), "rose a échoué : {:?}", texte(&o));
    let l = lignes(&o);
    // Une ligne de 30 chiffres, une ligne d'au moins 60 chiffres.
    let court = l
        .iter()
        .find(|s| s.chars().count() == 30 && s.chars().all(|c| c.is_ascii_digit()));
    let long = l
        .iter()
        .filter(|s| s.chars().all(|c| c.is_ascii_digit() || c == '…'))
        .max_by_key(|s| s.chars().count());
    match (court, long) {
        (Some(c), Some(g)) if g.chars().count() >= 60 => {
            assert_eq!(
                g.chars().take(30).collect::<String>(),
                *c,
                "préfixe instable après plus_de_chiffres : {c} vs {g}"
            );
            assert_eq!(
                &c[..],
                "933262154439441526816992388562",
                "mauvais préfixe initial : {c}"
            );
        }
        _ => panic!("sortie inattendue (30 puis ≥60 chiffres) : {l:?}"),
    }
}

// ── 5. REPL :more / :plus / :digits ────────────────────────────────────────

#[test]
fn repl_more_ajoute_30_chiffres_et_reste_stable() {
    let mut enfant = Command::new(binaire())
        .arg("console")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("lancement de rose console");
    {
        let mut entree = enfant.stdin.take().expect("stdin du REPL");
        write!(
            entree,
            "entier_geant_factorielle(100)\n:more\n:plus\n:more 10\n:quit\n"
        )
        .expect("écriture dans le REPL");
    }
    let o = enfant.wait_with_output().expect("attente du REPL");
    let t = texte(&o);
    if t.contains("'entier_geant_factorielle' inconnue") {
        eprintln!("SKIP repl_more_ajoute_30_chiffres_et_reste_stable : primitive pas encore câblée");
        return;
    }
    assert!(
        !t.contains("nécessite d'abord detail(expr)"),
        "le REPL a perdu le dernier résultat symbolique : {t:?}"
    );
    let court = "933262154439441526816992388562";
    let l: Vec<String> = t
        .lines()
        .map(|s| s.trim().trim_start_matches("rose> ").to_string())
        .filter(|s| s.starts_with(court))
        .collect();
    assert_eq!(l.len(), 4, "4 affichages attendus (30/60/90/100 chiffres) : {l:?}");
    assert_eq!(l[0].chars().count(), 31, "affichage initial : {:?}", l[0]);
    for (i, n) in [61usize, 91, 101].iter().enumerate() {
        assert_eq!(l[i + 1].chars().count(), *n, "ligne {} : {:?}", i + 2, l[i + 1]);
        assert!(
            l[i + 1].starts_with(court),
            "préfixe instable sur la ligne {} : {:?}",
            i + 2,
            l[i + 1]
        );
    }
}

// ── 6. GUI : fenêtre et bouton du jardin ───────────────────────────────────

#[test]
fn gui_grand_nombre_fenetre_bouton_et_clic_simule() {
    let racine = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("racine du dépôt")
        .to_path_buf();
    let jardin = racine.join("jardins/gui_native.rose");
    assert!(jardin.exists(), "jardin GUI absent : {:?}", jardin);
    let source = format!(
        "importer \"{}\"\n\
         $f = grand_nombre_fenetre(\"entier_geant_factorielle(100)\")\n\
         >> decrire_grand_nombre($f)\n\
         >> geometrie_boutons($f)\n\
         $h = fenetre_creer_simulee(\"Grands nombres\", 520, 240)\n\
         fenetre_boutons_creer($h, geometrie_boutons($f))\n\
         bouton_clic_simuler($h, 1)\n\
         >> bouton_attendre_clic_limite($h, 20)\n",
        jardin.display()
    );
    // `importer` résout ses chemins depuis le CWD : on se place à la racine,
    // comme le fait l'utilisateur qui lance `rose` depuis le dépôt.
    let p = ecrire_programme("qaa_gui_fenetre.rose", &source);
    let o = Command::new(binaire())
        .arg(&p)
        .current_dir(&racine)
        .output()
        .expect("lancement de rose depuis la racine");
    let t0 = texte(&o);
    if t0.contains("'grand_nombre_fenetre' inconnue")
        || t0.contains("'fenetre_boutons_creer' inconnue")
    {
        eprintln!("SKIP gui_grand_nombre_fenetre_bouton_et_clic_simule : pont GUI pas encore câblé");
        return;
    }
    assert!(o.status.success(), "rose a échoué : {:?}", t0);
    let t = texte(&o);
    for attendu in [
        "FENETRE|Grands nombres : entier_geant_factorielle(100)|520|240",
        "W|champ|chiffres|12|16",
        "W|bouton|Plus de chiffres|12|56",
        "ACTION_PLUS|1|plus_de_chiffres",
        "B|1|Plus de chiffres|12|56|160|28",
        "1", // bouton_attendre_clic_limite rend l'identifiant du bouton cliqué
    ] {
        assert!(t.contains(attendu), "ligne absente de la sortie GUI : {attendu:?}\n{t}");
    }
}
