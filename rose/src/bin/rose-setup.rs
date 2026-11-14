//! Installeur graphique autonome de Rose (Windows x64, PE32+ GUI).
//!
//! Binaire séparé du moteur : il n'embarque PAS `rose.exe` (le paquet
//! Velopack le pose à côté de lui), il ouvre la fenêtre Win32 dessinée à la
//! main de `setup_gui` et laisse `setup_core` faire le travail (copie dans
//! `%LOCALAPPDATA%\Programs\Rose`, PATH utilisateur, association `.rose`,
//! entrée de désinstallation) — aucun droit administrateur.
//!
//! Produit par `installateur/velopack/build.ps1` (Windows) ou
//! `installateur/velopack/build-linux.sh` (Linux, clang + lld-link + SDK
//! xwin) sous le nom `Rose-win-Setup.exe`.
//!
//! Licence MIT.

fn main() {
    // Mode console : `Rose-win-Setup.exe --console` rejoue l'installation
    // texte de `rose update` (utile en SSH ou dans un script silencieux).
    if std::env::args().any(|a| a == "--console" || a == "-c") {
        let prefixe = rose::setup_core::prefixe_defaut();
        let journal = rose::setup_core::installer(&prefixe, None);
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
        let journal = rose::setup_core::installer(&prefixe, None);
        for ligne in &journal {
            println!("{ligne}");
        }
        std::process::exit(if journal.iter().any(|l| l.starts_with("Échec")) { 1 } else { 0 });
    }
}
