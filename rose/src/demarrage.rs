//! Auto-démarrage SANS droits administrateur — dossier « Démarrage » de la
//! session utilisateur (`shell:startup`).
//!
//! Le helper `rose-tray.exe` n'est ni un service Windows, ni une tâche
//! planifiée, ni une clé `HKLM`. Quand l'utilisateur choisit
//! « Automatique (discret) », le setup dépose un tout petit lanceur `.cmd`
//! dans le dossier Démarrage de SON profil :
//!
//! - Windows : `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup\rose-tray.cmd`
//! - équivalent : `shell:startup` ( touche Windows + R → `shell:startup` )
//!
//! Ce n'est PAS un raccourci `.lnk` (qui exigerait COM/IShellLinkW, donc
//! cent lignes de FFI non testables ici) : un `.cmd` de quatre lignes suffit,
//! s'écrit avec `std::fs` et se relit pour vérification. Il ne demande aucun
//! privilège : tout est sous le profil de l'utilisateur courant.
//!
//! Le lanceur est **passif** : il teste `if exist` puis `start /B` le helper.
//! Si le binaire a été désinstallé, ou si la config est repassée en
//! `desactive`, le helper se relance mais sort immédiatement (garde-fou
//! `ModeMiseAJour::doit_demarrer_timer_discret()` dans `rose-tray.rs`).
//! Une entrée oubliée est donc inoffensive.
//!
//! `rose-tray --disable` et le menu « Désactiver le helper » appellent
//! `desactiver()` : le fichier est supprimé et sa disparition est RELUE.
//!
//! Aucune dépendance externe, aucun crate, aucune console au-delà du court
//! instant où Windows exécute le `.cmd` au logon (limite documentée dans
//! `docs/TRAY3.md`).

use std::path::{Path, PathBuf};

/// Nom du lanceur déposé dans le dossier Démarrage.
pub const NOM_LANCEUR: &str = "rose-tray.cmd";

/// Sous-chemin de `%APPDATA%` vers le dossier Démarrage.
///
/// Séparateurs `/` volontaires : `PathBuf::join` les accepte sur Windows
/// comme sur Linux, ce qui rend la composition testable sur cette machine.
pub const SOUS_CHEMIN_STARTUP: &str = "Microsoft/Windows/Start Menu/Programs/Startup";

/// Résultat d'une écriture ou d'une suppression, avec relecture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultatSession {
    /// Chemin complet du lanceur (None si le dossier Démarrage est introuvable).
    pub chemin: Option<PathBuf>,
    /// Vrai seulement si l'état RELU du fichier correspond à la demande.
    pub reussi: bool,
    /// Lignes destinées à `%TEMP%\rose-tray.log`.
    pub journal: Vec<String>,
}

impl ResultatSession {
    fn nouveau(chemin: Option<PathBuf>) -> Self {
        Self { chemin, reussi: false, journal: Vec::new() }
    }
    fn new(chemin: Option<PathBuf>) -> Self { Self::nouveau(chemin) }
}

/// Dossier Démarrage de l'utilisateur courant.
///
/// Sur Windows : `APPDATA` (répertoire Roaming), avec repli
/// `USERPROFILE\AppData\Roaming`. Le dossier est créé s'il manque : les
/// profils nomades ou verrouillés peuvent ne pas l'avoir encore.
/// Hors Windows : `None` (il n'existe pas de `shell:startup` Linux/macOS).
pub fn dossier_demarrage() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let racine = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("USERPROFILE")
                .map(|u| PathBuf::from(u).join("AppData").join("Roaming")))?;
        let dossier = racine.join(SOUS_CHEMIN_STARTUP);
        if dossier.is_dir() || std::fs::create_dir_all(&dossier).is_ok() {
            return Some(dossier);
        }
    }
    #[cfg(not(windows))]
    let _ = SOUS_CHEMIN_STARTUP;
    None
}

/// Chemin complet du lanceur dans le dossier Démarrage courant.
pub fn chemin_lanceur() -> Option<PathBuf> {
    dossier_demarrage().map(|d| d.join(NOM_LANCEUR))
}

/// Contenu EXACT du lanceur. Fonction pure, testable sur Linux.
///
/// ASCII uniquement (pas d'accent) : `cmd.exe` utilise la page de code
/// système, et un commentaire accentué peut être mal décodé sur un profil
/// non francophone. `start "" /B` évite d'ouvrir une SECONDE fenêtre ; le
/// helper lui-même est en sous-système Windows (aucune console).
pub fn contenu_lanceur(exe: &Path) -> String {
    // Un chemin Windows ne peut pas contenir de guillemet double : la mise
    // entre guillemets est donc sûre, y compris avec des espaces.
    let chemin = exe.display().to_string();
    format!(
        "@echo off\r\n\
         rem Rose - verification discrete des mises a jour (session utilisateur)\r\n\
         if exist \"{chemin}\" start \"\" /B \"{chemin}\"\r\n\
         exit /B 0\r\n"
    )
}

/// Écrit (ou réécrit, de façon idempotente) le lanceur dans `dossier`.
/// Le contenu est RELU après écriture : `reussi` ne ment pas.
pub fn activer_dans(dossier: &Path, exe: &Path) -> ResultatSession {
    let chemin = dossier.join(NOM_LANCEUR);
    let mut resultat = ResultatSession::new(Some(chemin.clone()));
    let contenu = contenu_lanceur(exe);
    if std::fs::create_dir_all(dossier).is_err() {
        resultat.journal.push(format!("démarrage : création de {} impossible", dossier.display()));
        return resultat;
    }
    match std::fs::write(&chemin, contenu.as_bytes()) {
        Ok(()) => resultat.journal.push(format!("démarrage : lanceur écrit {}", chemin.display())),
        Err(e) => {
            resultat.journal.push(format!("démarrage : écriture de {} impossible — {e}", chemin.display()));
            return resultat;
        }
    }
    match std::fs::read_to_string(&chemin) {
        Ok(relu) if relu == contenu => {
            resultat.reussi = true;
            resultat.journal.push("démarrage : lanceur RELU conforme (session suivante couverte)".to_string());
        }
        Ok(_) => resultat.journal.push("démarrage : lanceur relu DIFFÉRENT du contenu attendu".to_string()),
        Err(e) => resultat.journal.push(format!("démarrage : relecture impossible — {e}")),
    }
    resultat
}

/// Supprime le lanceur de `dossier`. Une entrée déjà absente est un succès
/// (le mode « désactivée » doit rester idempotent). La disparition est RELUE.
pub fn desactiver_dans(dossier: &Path) -> ResultatSession {
    let chemin = dossier.join(NOM_LANCEUR);
    let mut resultat = ResultatSession::new(Some(chemin.clone()));
    if !chemin.exists() {
        resultat.reussi = true;
        resultat.journal.push(format!("démarrage : aucune entrée à retirer ({})", chemin.display()));
        return resultat;
    }
    match std::fs::remove_file(&chemin) {
        Ok(()) => resultat.journal.push(format!("démarrage : entrée retirée {}", chemin.display())),
        Err(e) => {
            resultat.journal.push(format!("démarrage : suppression de {} impossible — {e}", chemin.display()));
            return resultat;
        }
    }
    if !chemin.exists() {
        resultat.reussi = true;
        resultat.journal.push("démarrage : absence RELUE (plus de relance au prochain logon)".to_string());
    } else {
        resultat.journal.push("démarrage : le fichier existe ENCORE après suppression".to_string());
    }
    resultat
}

/// Active l'auto-démarrage pour le chemin RÉEL du helper.
pub fn activer(exe: &Path) -> ResultatSession {
    match dossier_demarrage() {
        Some(dossier) => activer_dans(&dossier, exe),
        None => {
            let mut r = ResultatSession::new(None);
            r.journal.push("démarrage : dossier shell:startup introuvable (hors Windows ?)".to_string());
            r
        }
    }
}

/// Retire l'auto-démarrage. Appelé par `rose-tray --disable`, par le menu
/// « Désactiver le helper » et quand le setup repasse en « Désactivée ».
pub fn desactiver() -> ResultatSession {
    match dossier_demarrage() {
        Some(dossier) => desactiver_dans(&dossier),
        None => {
            let mut r = ResultatSession::new(None);
            r.journal.push("démarrage : dossier shell:startup introuvable (hors Windows ?)".to_string());
            r
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dossier_test(nom: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        std::env::temp_dir().join(format!("rose-demarrage-{nom}-{}-{}", std::process::id(), nanos))
    }

    #[test]
    fn sous_chemin_startup_est_celui_de_shell_startup() {
        assert_eq!(SOUS_CHEMIN_STARTUP, "Microsoft/Windows/Start Menu/Programs/Startup");
        assert_eq!(NOM_LANCEUR, "rose-tray.cmd");
        // Le chemin complet attendu, tel qu'il est documenté dans TRAY3.md.
        let appdata = PathBuf::from("C:/Users/Toto/AppData/Roaming");
        assert_eq!(appdata.join(SOUS_CHEMIN_STARTUP).join(NOM_LANCEUR),
                   PathBuf::from("C:/Users/Toto/AppData/Roaming/Microsoft/Windows/Start Menu/Programs/Startup/rose-tray.cmd"));
    }

    #[test]
    fn contenu_lanceur_est_un_cmd_ascii_sans_fenetre_supplementaire() {
        let exe = PathBuf::from("C:/Users/Toto/AppData/Local/Programs/Rose/bin/rose-tray.exe");
        let c = contenu_lanceur(&exe);
        assert!(c.starts_with("@echo off\r\n"));
        assert!(c.ends_with("exit /B 0\r\n"));
        assert!(c.contains("if exist"));
        assert!(c.contains("start \"\" /B"));
        assert_eq!(c.matches(&exe.display().to_string()).count(), 2,
                   "le chemin doit servir au test ET au lancement");
        assert!(c.is_ascii(), "un .cmd accentué casse sous une autre page de code");
    }

    #[test]
    fn contenu_lanceur_quote_les_espaces_du_chemin() {
        let exe = PathBuf::from("C:/Program Files/Rose/bin/rose-tray.exe");
        let c = contenu_lanceur(&exe);
        assert!(c.contains("\"C:/Program Files/Rose/bin/rose-tray.exe\""),
                "un chemin avec espace doit être entre guillemets");
    }

    #[test]
    fn activer_puis_desactiver_sont_verifies_par_relecture() {
        let dossier = dossier_test("aller-retour");
        let exe = PathBuf::from("C:/Program Files/Rose/bin/rose-tray.exe");
        let a = activer_dans(&dossier, &exe);
        assert!(a.reussi, "{}", a.journal.join(" | "));
        let chemin = a.chemin.expect("chemin du lanceur");
        assert_eq!(std::fs::read_to_string(&chemin).unwrap(), contenu_lanceur(&exe));
        // Idempotence : relancer l'activation n'empile pas de fichiers.
        assert!(activer_dans(&dossier, &exe).reussi);
        assert_eq!(std::fs::read_dir(&dossier).unwrap().count(), 1);
        let d = desactiver_dans(&dossier);
        assert!(d.reussi, "{}", d.journal.join(" | "));
        assert!(!chemin.exists());
        // Désactiver deux fois reste un succès.
        assert!(desactiver_dans(&dossier).reussi);
        let _ = std::fs::remove_dir_all(&dossier);
    }

    #[test]
    fn desactiver_sans_entree_existante_reussit() {
        let dossier = dossier_test("absent");
        assert!(!dossier.exists());
        let d = desactiver_dans(&dossier);
        assert!(d.reussi);
        assert!(d.journal.iter().any(|l| l.contains("aucune entrée")));
    }
}
