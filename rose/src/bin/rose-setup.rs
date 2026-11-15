//! Installeur graphique autonome de Rose (Windows x64, PE32+ GUI).
//!
//! Binaire séparé du moteur : il n'embarque PAS `rose.exe`, il ouvre la
//! fenêtre Win32 de `setup_gui` et laisse `setup_core` faire le travail
//! (copie dans `%LOCALAPPDATA%\Programs\Rose`, PATH utilisateur, association
//! `.rose`, entrée de désinstallation) — aucun droit administrateur.
//!
//! La source du binaire à installer est résolue dans cet ordre :
//!   1. `payload\rose.exe` à côté de l'installeur (dossier de distribution
//!      Velopack) ;
//!   2. `rose.exe` à côté de l'installeur ;
//!   3. l'exécutable courant, quand `Rose-win-Setup.exe` a été renommé en
//!      `rose.exe` (installeur autonome « un seul fichier »).
//!
//! Produit par `installateur/velopack/build.ps1` (Windows) ou
//! `installateur/velopack/build-linux.sh` (Linux, clang + lld-link + SDK
//! xwin) sous le nom `Rose-win-Setup.exe`.
//!
//! Licence MIT.

use std::path::PathBuf;

/// Binaire Rose à installer.
///
/// Ordre :
/// 1. `payload\rose.exe` à côté de l'installeur (distribution Velopack) ;
/// 2. le moteur EMBARQUÉ dans les ressources (`RT_RCDATA 3`), qui fait de
///    `Rose-win-Setup.exe` un fichier unique.
///
/// ⚠⚠ NE JAMAIS retomber sur `current_exe()`. La version précédente le
/// faisait : dans `Rose-win-Setup.exe`, l'installeur se copiait donc
/// lui-même sous `bin\rose.exe`, puis s'inscrivait dans le PATH. Chaque
/// `rose` relançait le setup, qui réinstallait, qui relançait le setup —
/// jusqu'à 60 processus simultanés chez l'utilisateur, obligé de redémarrer
/// son PC. Si rien n'est trouvé, on rend `None` et `installer` refuse
/// proprement avec un message au lieu de boucler.
pub fn source_moteur() -> Option<PathBuf> {
    let dossier = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let candidats = [
        dossier.join("payload").join("rose.exe"),
        dossier.join("rose.exe"),
        dossier.join("payload").join("rose"),
        dossier.join("rose"),
    ];
    if let Some(p) = candidats.into_iter().find(|p| p.is_file()) {
        return Some(p);
    }
    // Moteur embarqué dans les ressources de l'installeur (Windows seul :
    // `setup_gui` n'est compilé que sur cette cible).
    #[cfg(windows)]
    {
        if let Some(p) = rose::setup_gui::moteur_embarque() {
            return Some(p);
        }
    }
    None
}

fn main() {
    // Mode console : `Rose-win-Setup.exe --console` rejoue l'installation
    // texte de `rose update` (utile en SSH ou dans un script silencieux).
    if std::env::args().any(|a| a == "--console" || a == "-c") {
        let prefixe = rose::setup_core::prefixe_defaut();
        let source = source_moteur();
        let journal = rose::setup_core::installer(&prefixe, source.as_deref());
        for ligne in &journal {
            println!("{ligne}");
        }
        let echec = journal.iter().any(|l| l.starts_with("Échec"));
        std::process::exit(if echec { 1 } else { 0 });
    }
    // La fenêtre Win32 n'existe que sous Windows (setup_gui est `#[cfg(windows)]`) :
    // ailleurs le binôme se compile quand même et retombe sur la console.
    #[cfg(windows)]
    rose::setup_gui::lancer();
    #[cfg(not(windows))]
    {
        let prefixe = rose::setup_core::prefixe_defaut();
        let source = source_moteur();
        let journal = rose::setup_core::installer(&prefixe, source.as_deref());
        for ligne in &journal {
            println!("{ligne}");
        }
        std::process::exit(if journal.iter().any(|l| l.starts_with("Échec")) { 1 } else { 0 });
    }
}
