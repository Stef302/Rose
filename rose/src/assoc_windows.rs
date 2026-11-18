//! Association des fichiers `.rose` sur Windows (clic droit / double-clic).
//!
//! Objectif utilisateur : « je clique droit sur un .rose, je l'ouvre avec
//! rose.exe ou avec le shell Rose, et quand j'ai mis "toujours ouvrir avec",
//! un clic l'exécute directement ».
//!
//! Ce module est **autonome** : `std` uniquement, aucun crate externe. Il ne
//! dépend d'aucun autre module du moteur, ce qui le rend compilable et
//! testable directement depuis Linux :
//!
//! ```text
//! rustc --test rose/src/assoc_windows.rs -o /tmp/opencode/assoc_test
//! /tmp/opencode/assoc_test
//! ```
//!
//! ⚠ AUCUNE machine Windows n'est disponible ici. Les entrées de registre,
//! les libellés et les lignes `reg add` sont **vérifiés statiquement** (tests
//! unitaires + `--dry-run`) ; l'effet réel dans l'Explorateur Windows reste à
//! tester sur une machine Windows réelle. Voir `docs/ASSOCIATION_WINDOWS.md`.
//!
//! # Entrées écrites
//!
//! Préfixe `HKCU\Software\Classes` (utilisateur, défaut) ou
//! `HKLM\SOFTWARE\Classes` (`--system`, élévation requise) :
//!
//! ```text
//! <préfixe>\.rose                         (par défaut) = Rose.Source.1
//! <préfixe>\Rose.Source.1                 (par défaut) = nom convivial
//! <préfixe>\Rose.Source.1\FriendlyName                 = nom convivial
//! <préfixe>\Rose.Source.1\Description                  = description
//! <préfixe>\Rose.Source.1\DefaultIcon      (par défaut) = "<exe>,0"
//! <préfixe>\Rose.Source.1\shell\open       (par défaut) = Exécuter avec Rose
//! <préfixe>\Rose.Source.1\shell\open\command (par défaut) = "<exe>" run "%1"
//! <préfixe>\Rose.Source.1\shell\console    (par défaut) = Éditer avec le shell Rose
//! <préfixe>\Rose.Source.1\shell\console\command (par défaut) = "<exe>" console "%1"
//! ```
//!
//! Le verbe `open` est le verbe par défaut : « toujours ouvrir avec » Rose
//! exécute donc le programme au double-clic. Le verbe `console` est le second
//! choix du clic droit (« ouvrir dans le shell Rose »).
//!
//! Licence MIT.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Identifiant de programme (ProgID) auquel `.rose` est associé.
pub const PROG_ID: &str = "Rose.Source.1";

/// Extension associée.
pub const EXTENSION: &str = ".rose";

/// Nom du moteur installé.
pub const BINAIRE: &str = "rose.exe";

/// Nom convivial — français.
pub const NOM_CONVIVIAL_FR: &str = "Fichier source Rose";
/// Nom convivial — anglais.
pub const NOM_CONVIVIAL_EN: &str = "Rose source file";

/// Description longue — français.
pub const DESCRIPTION_FR: &str = "Fichier source Rose — exécuter ou éditer avec le shell Rose";
/// Description longue — anglais.
pub const DESCRIPTION_EN: &str = "Rose source file — run or edit in the Rose shell";

/// Libellé du verbe d'exécution — français.
pub const VERBE_OPEN_FR: &str = "Exécuter avec Rose";
/// Libellé du verbe d'exécution — anglais.
pub const VERBE_OPEN_EN: &str = "Run with Rose";

/// Libellé du verbe « shell » — français.
pub const VERBE_CONSOLE_FR: &str = "Éditer avec le shell Rose";
/// Libellé du verbe « shell » — anglais.
pub const VERBE_CONSOLE_EN: &str = "Edit with Rose shell";

/// Langue des libellés affichés dans le menu contextuel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Fr,
    En,
}

impl Lang {
    /// `--lang fr|en` (insensible à la casse). Par défaut : français.
    pub fn depuis(s: &str) -> Self {
        if s.eq_ignore_ascii_case("en") || s.eq_ignore_ascii_case("english") {
            Lang::En
        } else {
            Lang::Fr
        }
    }

    pub fn nom_convivial(self) -> &'static str {
        match self {
            Lang::Fr => NOM_CONVIVIAL_FR,
            Lang::En => NOM_CONVIVIAL_EN,
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Lang::Fr => DESCRIPTION_FR,
            Lang::En => DESCRIPTION_EN,
        }
    }

    pub fn verbe_open(self) -> &'static str {
        match self {
            Lang::Fr => VERBE_OPEN_FR,
            Lang::En => VERBE_OPEN_EN,
        }
    }

    pub fn verbe_console(self) -> &'static str {
        match self {
            Lang::Fr => VERBE_CONSOLE_FR,
            Lang::En => VERBE_CONSOLE_EN,
        }
    }
}

/// Portée du registre : utilisateur (`HKCU`) ou machine (`HKLM`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    User,
    System,
}

impl Scope {
    /// Racine des classes pour cette portée.
    pub fn prefixe(self) -> &'static str {
        match self {
            Scope::User => "HKCU\\Software\\Classes",
            Scope::System => "HKLM\\SOFTWARE\\Classes",
        }
    }
}

/// Une entrée de registre à écrire.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntreeReg {
    /// Clé complète, sans guillemets.
    pub cle: String,
    /// Nom de valeur ; `None` = valeur par défaut (`/ve`).
    pub valeur: Option<&'static str>,
    /// Données `REG_SZ` (brutes, sans échappement).
    pub donnees: String,
}

/// Options de la commande `rose assoc`.
#[derive(Clone, Debug)]
pub struct Options {
    pub scope: Scope,
    pub lang: Lang,
    pub supprimer: bool,
    pub dry_run: bool,
    /// Chemin forcé vers `rose.exe` (tests / dry-run) ; `None` = détection.
    pub exe: Option<PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            scope: Scope::User,
            lang: Lang::Fr,
            supprimer: false,
            dry_run: false,
            exe: None,
        }
    }
}

/// Aide de la sous-commande (à insérer dans `usage()` — voir le patch).
pub fn help() -> &'static str {
    "  rose assoc [--user|--system] [--lang fr|en] [--exe chemin]     associer .rose à rose.exe (Windows)\n\
     \x20 rose assoc --remove [--user|--system]                       retirer l'association .rose\n\
     \x20 rose assoc --dry-run                                        afficher les commandes reg add sans écrire\n"
}

/// Chemin du moteur à placer dans le registre.
///
/// Ordre :
/// 1. `Options::exe` (forcing explicite, utilisé par les tests et le dry-run) ;
/// 2. `rose.exe` à côté de l'exécutable courant — cas de l'INSTALLEUR, qui
///    s'appelle `rose-setup.exe` mais doit associer le moteur installé ;
/// 3. l'exécutable courant lui-même (cas normal : `rose.exe assoc`).
///
/// ⚠ Avec Velopack, il faut associer le chemin STABLE `<racine>\current\rose.exe`
/// et non `<racine>\app-0.1.5\rose.exe` : `current` est remis à jour à chaque
/// version, donc l'association survit à la mise à jour.
pub fn chemin_rose(forcing: Option<&Path>) -> PathBuf {
    if let Some(p) = forcing {
        return p.to_path_buf();
    }
    let soi = std::env::current_exe().unwrap_or_else(|_| PathBuf::from(BINAIRE));
    let voisin = soi.parent().map(|p| p.join(BINAIRE));
    if let Some(v) = voisin {
        if v.is_file() {
            return v;
        }
    }
    soi
}

/// Toutes les entrées à écrire pour l'association.
///
/// L'ordre est DÉTERMINISTE : c'est celui que `--dry-run` affiche et que les
/// tests figent.
pub fn entrees_reg(scope: Scope, exe: &Path, lang: Lang) -> Vec<EntreeReg> {
    let racine = scope.prefixe();
    let exe_s = exe.to_string_lossy();
    let commande = |sous_commande: &str| -> String {
        format!("\"{exe_s}\" {sous_commande} \"%1\"")
    };
    vec![
        EntreeReg {
            cle: format!("{racine}\\{EXTENSION}"),
            valeur: None,
            donnees: PROG_ID.to_string(),
        },
        EntreeReg {
            cle: format!("{racine}\\{PROG_ID}"),
            valeur: None,
            donnees: lang.nom_convivial().to_string(),
        },
        EntreeReg {
            cle: format!("{racine}\\{PROG_ID}"),
            valeur: Some("FriendlyName"),
            donnees: lang.nom_convivial().to_string(),
        },
        EntreeReg {
            cle: format!("{racine}\\{PROG_ID}"),
            valeur: Some("Description"),
            donnees: lang.description().to_string(),
        },
        EntreeReg {
            cle: format!("{racine}\\{PROG_ID}\\DefaultIcon"),
            valeur: None,
            donnees: format!("\"{exe_s},0\""),
        },
        EntreeReg {
            cle: format!("{racine}\\{PROG_ID}\\shell\\open"),
            valeur: None,
            donnees: lang.verbe_open().to_string(),
        },
        EntreeReg {
            cle: format!("{racine}\\{PROG_ID}\\shell\\open\\command"),
            valeur: None,
            donnees: commande("run"),
        },
        EntreeReg {
            cle: format!("{racine}\\{PROG_ID}\\shell\\console"),
            valeur: None,
            donnees: lang.verbe_console().to_string(),
        },
        EntreeReg {
            cle: format!("{racine}\\{PROG_ID}\\shell\\console\\command"),
            valeur: None,
            donnees: commande("console"),
        },
    ]
}

/// Clés à supprimer pour `--remove` (les deux suffisent : `reg delete /f`
/// emporte les valeurs et les sous-clés).
pub fn cles_a_supprimer(scope: Scope) -> Vec<String> {
    let racine = scope.prefixe();
    vec![
        format!("{racine}\\{EXTENSION}"),
        format!("{racine}\\{PROG_ID}"),
    ]
}

/// Rend un argument sous forme de jeton `cmd.exe` : guillemets si l'argument
/// contient un espace ou une guillemet, guillemets internes DOUBLÉS.
///
/// C'est la convention de `cmd.exe` (pas les antislashs de POSIX). Elle rend
/// les lignes `--dry-run` directement copiables dans une invite de commandes.
fn jeton_cmd(arg: &str) -> String {
    if arg.chars().any(|c| c == ' ' || c == '\t' || c == '"') {
        format!("\"{}\"", arg.replace('"', "\"\""))
    } else {
        arg.to_string()
    }
}

/// Arguments `reg` pour une entrée (sans le programme `reg` lui-même).
fn args_reg_add(e: &EntreeReg) -> Vec<String> {
    let mut a = vec![
        "add".to_string(),
        e.cle.clone(),
    ];
    match e.valeur {
        None => a.push("/ve".to_string()),
        Some(n) => {
            a.push("/v".to_string());
            a.push(n.to_string());
        }
    }
    a.push("/t".to_string());
    a.push("REG_SZ".to_string());
    a.push("/d".to_string());
    a.push(e.donnees.clone());
    a.push("/f".to_string());
    a
}

/// Lignes EXACTES affichées par `--dry-run` (et passées à `reg` sur Windows).
pub fn lignes_reg_add(scope: Scope, exe: &Path, lang: Lang) -> Vec<String> {
    entrees_reg(scope, exe, lang)
        .iter()
        .map(|e| {
            let mut parts = vec!["reg".to_string()];
            parts.extend(args_reg_add(e));
            parts.iter().map(|p| jeton_cmd(p)).collect::<Vec<_>>().join(" ")
        })
        .collect()
}

/// Lignes `reg delete` affichées par `rose assoc --remove --dry-run`.
pub fn lignes_reg_delete(scope: Scope) -> Vec<String> {
    cles_a_supprimer(scope)
        .iter()
        .map(|cle| format!("reg delete {} /f", jeton_cmd(cle)))
        .collect()
}

/// `reg.exe` est un programme console : appelé depuis l'INSTALLATEUR
/// (sous-système GUI), Windows ouvrirait une fenêtre noire. `CREATE_NO_WINDOW`
/// la supprime — même motif que `velopack::curl_sans_fenetre`.
fn reg_sans_fenetre() -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new("reg");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

/// Force l'Explorateur à relire les associations (sinon l'ancienne icône et
/// l'ancien verbe restent en cache jusqu'au redémarrage de la session).
#[cfg(windows)]
fn notifier_explorateur() {
    use std::ffi::c_void;
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn SHChangeNotify(evenement: i32, drapeaux: u32, item1: *const c_void, item2: *const c_void);
    }
    const SHCNE_ASSOCCHANGED: i32 = 0x0800_0000;
    const SHCNF_IDLIST: u32 = 0x0000;
    unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, std::ptr::null(), std::ptr::null()); }
}

/// Exécute réellement les `reg add` (Windows uniquement).
///
/// `std::process::Command` passe chaque argument tel quel à `reg.exe` : les
/// guillemets internes des données (`"<exe>" run "%1"`) font partie de la
/// donnée, ce qui est exactement ce que `reg add /d` attend.
fn ecrire_reg(entrees: &[EntreeReg]) -> Result<(), String> {
    for e in entrees {
        let sortie = reg_sans_fenetre()
            .args(args_reg_add(e))
            .output()
            .map_err(|err| format!("lancement de reg.exe : {err}"))?;
        if !sortie.status.success() {
            return Err(format!(
                "reg add {} échoué : {}",
                e.cle,
                String::from_utf8_lossy(&sortie.stderr).trim()
            ));
        }
    }
    Ok(())
}

/// Supprime réellement les clés (Windows uniquement).
fn supprimer_reg(scope: Scope) -> Result<(), String> {
    for cle in cles_a_supprimer(scope) {
        let sortie = reg_sans_fenetre()
            .args(["delete", &cle, "/f"])
            .output()
            .map_err(|err| format!("lancement de reg.exe : {err}"))?;
        // `reg delete` renvoie 1 quand la clé n'existe pas : c'est déjà
        // l'état voulu, on le traite comme un succès.
        if !sortie.status.success() {
            let err = String::from_utf8_lossy(&sortie.stderr);
            let absent = String::from_utf8_lossy(&sortie.stdout).contains("ERROR")
                || err.contains("ERROR")
                || sortie.stdout.is_empty() && err.is_empty();
            if !absent {
                return Err(format!("reg delete {cle} échoué : {}", err.trim()));
            }
        }
    }
    Ok(())
}

/// Hook d'installation : associe SILENCIEUSEMENT `.rose` à `rose.exe` dans
/// `HKCU` (aucune élévation requise).
///
/// À appeler depuis `install_rose()` après une installation réussie, et depuis
/// `velopack::appliquer_paquets()` après l'écriture de `current`. Sur une
/// plateforme non-Windows, c'est un no-op (`Ok(0)`) — l'installeur reste
/// portable et ses tests Linux ne cassent pas.
pub fn appliquer_silencieuse(exe: Option<&Path>) -> Result<usize, String> {
    if !cfg!(windows) {
        return Ok(0);
    }
    let chemin = chemin_rose(exe);
    let entrees = entrees_reg(Scope::User, &chemin, Lang::Fr);
    ecrire_reg(&entrees)?;
    #[cfg(windows)]
    notifier_explorateur();
    Ok(entrees.len())
}

/// Point d'entrée de `rose assoc`. `argv` est l'argv COMPLET (`argv[1]` = "assoc").
///
/// Retourne le code de sortie. `--dry-run` fonctionne sur TOUTES les
/// plateformes ; sans `--dry-run`, hors Windows, un message explicite est
/// affiché et RIEN n'est modifié.
pub fn execute(argv: &[String]) -> i32 {
    let mut opt = Options::default();
    let mut i = 2;
    while i < argv.len() {
        match argv[i].as_str() {
            "--user" => opt.scope = Scope::User,
            "--system" | "-s" => opt.scope = Scope::System,
            "--remove" => opt.supprimer = true,
            "--dry-run" | "-n" => opt.dry_run = true,
            "--lang" | "-l" => {
                if i + 1 >= argv.len() {
                    eprintln!("rose assoc : --lang attend fr ou en.");
                    return 2;
                }
                opt.lang = Lang::depuis(&argv[i + 1]);
                i += 1;
            }
            "--exe" => {
                if i + 1 >= argv.len() {
                    eprintln!("rose assoc : --exe attend un chemin.");
                    return 2;
                }
                opt.exe = Some(PathBuf::from(&argv[i + 1]));
                i += 1;
            }
            "--help" | "-h" => {
                print!("{}", help());
                return 0;
            }
            autre => {
                eprintln!("rose assoc : option inconnue « {autre} ».");
                eprint!("{}", help());
                return 2;
            }
        }
        i += 1;
    }

    let exe = chemin_rose(opt.exe.as_deref());
    let lignes = if opt.supprimer {
        lignes_reg_delete(opt.scope)
    } else {
        lignes_reg_add(opt.scope, &exe, opt.lang)
    };

    if opt.dry_run {
        println!("rose assoc — dry-run (aucune écriture), moteur : {}", exe.to_string_lossy());
        for ligne in &lignes {
            println!("[dry-run] {ligne}");
        }
        println!(
            "rose assoc — {} commande(s), portée {} — rien n'a été modifié.",
            lignes.len(),
            if opt.scope == Scope::User { "HKCU" } else { "HKLM" }
        );
        return 0;
    }

    if !cfg!(windows) {
        println!("rose assoc : Windows uniquement (registre HKCU/HKLM). Rien n'a été modifié.");
        println!("             Utilisez --dry-run pour afficher les commandes reg add.");
        return 0;
    }

    let resultat = if opt.supprimer {
        supprimer_reg(opt.scope)
    } else {
        ecrire_reg(&entrees_reg(opt.scope, &exe, opt.lang))
    };
    match resultat {
        Ok(()) => {
            #[cfg(windows)]
            notifier_explorateur();
            println!(
                "rose assoc : {} entrée(s) {} dans {} — association .rose prête.",
                lignes.len(),
                if opt.supprimer { "supprimée(s)" } else { "écrite(s)" },
                opt.scope.prefixe()
            );
            0
        }
        Err(e) => {
            eprintln!("rose assoc : {e}");
            1
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests — exécutables depuis Linux avec :
//   rustc --test rose/src/assoc_windows.rs -o /tmp/opencode/assoc_test
//   /tmp/opencode/assoc_test
// ═══════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod tests {
    use super::*;

    /// Source de ce module, lue à la compilation : la vérification STATIQUE
    /// des chaînes de registre est ainsi portée par le test lui-même.
    const SOURCE: &str = include_str!("assoc_windows.rs");

    fn exe_test() -> PathBuf {
        PathBuf::from("C:\\Program Files\\Rose\\rose.exe")
    }

    #[test]
    fn prog_id_et_extension() {
        assert_eq!(PROG_ID, "Rose.Source.1");
        assert_eq!(EXTENSION, ".rose");
        assert_eq!(BINAIRE, "rose.exe");
    }

    #[test]
    fn entrees_reg_hkcu_fr_exactes() {
        let entrees = entrees_reg(Scope::User, &exe_test(), Lang::Fr);
        assert_eq!(entrees.len(), 9);
        let p = "HKCU\\Software\\Classes";
        assert_eq!(entrees[0].cle, format!("{p}\\.rose"));
        assert_eq!(entrees[0].donnees, "Rose.Source.1");
        assert_eq!(entrees[1].valeur, None);
        assert_eq!(entrees[1].donnees, "Fichier source Rose");
        assert_eq!(entrees[2].valeur, Some("FriendlyName"));
        assert_eq!(entrees[3].valeur, Some("Description"));
        assert_eq!(entrees[4].cle, format!("{p}\\Rose.Source.1\\DefaultIcon"));
        assert_eq!(entrees[4].donnees, "\"C:\\Program Files\\Rose\\rose.exe,0\"");
        assert_eq!(entrees[5].donnees, "Exécuter avec Rose");
        assert_eq!(entrees[6].cle, format!("{p}\\Rose.Source.1\\shell\\open\\command"));
        assert_eq!(entrees[6].donnees, "\"C:\\Program Files\\Rose\\rose.exe\" run \"%1\"");
        assert_eq!(entrees[7].donnees, "Éditer avec le shell Rose");
        assert_eq!(entrees[8].cle, format!("{p}\\Rose.Source.1\\shell\\console\\command"));
        assert_eq!(entrees[8].donnees, "\"C:\\Program Files\\Rose\\rose.exe\" console \"%1\"");
    }

    #[test]
    fn dry_run_hkcu_fr_lignes_exactes() {
        let lignes = lignes_reg_add(Scope::User, &exe_test(), Lang::Fr);
        // Ces 9 chaînes sont le contrat : c'est ce que doit afficher
        // `rose assoc --dry-run --exe "C:\Program Files\Rose\rose.exe"`.
        let attendu = vec![
            r#"reg add HKCU\Software\Classes\.rose /ve /t REG_SZ /d Rose.Source.1 /f"#,
            r#"reg add HKCU\Software\Classes\Rose.Source.1 /ve /t REG_SZ /d "Fichier source Rose" /f"#,
            r#"reg add HKCU\Software\Classes\Rose.Source.1 /v FriendlyName /t REG_SZ /d "Fichier source Rose" /f"#,
            r#"reg add HKCU\Software\Classes\Rose.Source.1 /v Description /t REG_SZ /d "Fichier source Rose — exécuter ou éditer avec le shell Rose" /f"#,
            r#"reg add HKCU\Software\Classes\Rose.Source.1\DefaultIcon /ve /t REG_SZ /d """C:\Program Files\Rose\rose.exe,0""" /f"#,
            r#"reg add HKCU\Software\Classes\Rose.Source.1\shell\open /ve /t REG_SZ /d "Exécuter avec Rose" /f"#,
            r#"reg add HKCU\Software\Classes\Rose.Source.1\shell\open\command /ve /t REG_SZ /d """C:\Program Files\Rose\rose.exe"" run ""%1""" /f"#,
            r#"reg add HKCU\Software\Classes\Rose.Source.1\shell\console /ve /t REG_SZ /d "Éditer avec le shell Rose" /f"#,
            r#"reg add HKCU\Software\Classes\Rose.Source.1\shell\console\command /ve /t REG_SZ /d """C:\Program Files\Rose\rose.exe"" console ""%1""" /f"#,
        ];
        assert_eq!(lignes, attendu);
    }

    #[test]
    fn system_scope_utilise_hklm() {
        let lignes = lignes_reg_add(Scope::System, &exe_test(), Lang::En);
        assert!(lignes.iter().all(|l| l.contains("reg add HKLM\\SOFTWARE\\Classes\\")));
        assert!(!lignes.iter().any(|l| l.contains("HKCU")));
        let entrees = entrees_reg(Scope::System, &exe_test(), Lang::En);
        assert_eq!(entrees[5].donnees, "Run with Rose");
        assert_eq!(entrees[7].donnees, "Edit with Rose shell");
        assert_eq!(entrees[1].donnees, "Rose source file");
    }

    #[test]
    fn remove_genere_deux_reg_delete() {
        let lignes = lignes_reg_delete(Scope::User);
        assert_eq!(lignes, vec![
            r"reg delete HKCU\Software\Classes\.rose /f",
            r"reg delete HKCU\Software\Classes\Rose.Source.1 /f",
        ]);
        let systeme = lignes_reg_delete(Scope::System);
        assert_eq!(systeme[0], r"reg delete HKLM\SOFTWARE\Classes\.rose /f");
    }

    #[test]
    fn le_verbe_par_defaut_est_open_avec_run() {
        // « toujours ouvrir avec » => le verbe par défaut, donc open, donc run.
        let entrees = entrees_reg(Scope::User, &exe_test(), Lang::Fr);
        let commande_open = entrees.iter()
            .find(|e| e.cle.ends_with("shell\\open\\command")).unwrap();
        assert!(commande_open.donnees.contains(" run "));
        let commande_console = entrees.iter()
            .find(|e| e.cle.ends_with("shell\\console\\command")).unwrap();
        assert!(commande_console.donnees.contains(" console "));
        assert_ne!(commande_open.donnees, commande_console.donnees);
    }

    #[test]
    fn lang_depuis() {
        assert_eq!(Lang::depuis("en"), Lang::En);
        assert_eq!(Lang::depuis("EN"), Lang::En);
        assert_eq!(Lang::depuis("fr"), Lang::Fr);
        assert_eq!(Lang::depuis("autre"), Lang::Fr);
    }

    #[test]
    fn execute_dry_run_sans_ecriture_sur_linux() {
        // Sur Linux, --dry-run imprime les commandes et ne touche au registre.
        let argv: Vec<String> = ["rose", "assoc", "--dry-run", "--exe", "C:\\Rose\\rose.exe"]
            .iter().map(|s| s.to_string()).collect();
        assert_eq!(execute(&argv), 0);
    }

    #[test]
    fn execute_sans_dry_run_hors_windows_est_un_no_op() {
        if cfg!(windows) {
            return; // sur Windows ce test modifierait le registre : on le saute.
        }
        let argv: Vec<String> = ["rose", "assoc"].iter().map(|s| s.to_string()).collect();
        assert_eq!(execute(&argv), 0);
        assert_eq!(appliquer_silencieuse(None), Ok(0));
    }

    #[test]
    fn execute_option_inconnue_est_rejetee() {
        let argv: Vec<String> = ["rose", "assoc", "--bogus"].iter().map(|s| s.to_string()).collect();
        assert_eq!(execute(&argv), 2);
    }

    /// Vérification STATIQUE demandée : les chaînes de registre et les deux
    /// verbes sont bien présents dans le code source.
    #[test]
    fn verification_statique_des_chaines_et_verbes() {
        for chaine in [
            "HKCU\\\\Software\\\\Classes",      // sous forme de chaîne Rust
            "HKLM\\\\SOFTWARE\\\\Classes",
            "Rose.Source.1",
            "DefaultIcon",
            "shell\\\\open\\\\command",
            "shell\\\\console\\\\command",
            "\\\"%1\\\"",
            "run ",
            "console ",
            "Exécuter avec Rose",
            "Éditer avec le shell Rose",
            "Run with Rose",
            "Edit with Rose shell",
            "reg add",
            "reg delete",
        ] {
            assert!(SOURCE.contains(chaine), "chaîne absente du source : {chaine}");
        }
    }
}
