//! Recherche LOCALE du helper AOT — zéro réseau, zéro compilation.
//!
//! Le binaire principal de Rose est léger : la chaîne AOT (`rose orc`,
//! `rose aot`, `rose link`) n'y est pas embarquée. Elle n'est disponible
//! que si un helper (ou un binaire Rose complet avec la chaîne AOT) est
//! déjà installée sur la machine.
//!
//! Ordre de recherche — TOUJOURS avant toute autre action :
//!   1. à côté de l'exécutable courant (`rose.exe` sur Windows) ;
//!   2. dans le cache utilisateur `~/.cache/rose/` ;
//!   3. dans le PATH.
//!
//! Aucune URL de téléchargement n'est maintenue : la release v0.1.7 ne
//! contient PAS d'artefact `rose-aot` (vérifié le 21/11/2026 sur
//! `api.github.com/repos/Stef302/Rose/releases/tags/v0.1.7`). Toute URL
//! historique (`v5.10.0/rose-aot-x86_64-windows.exe`, miroir brut…) était
//! morte et a été retirée. Si aucun helper local n'est trouvé, Rose affiche
//! un message clair orientant vers `rose comp` (exécutable autonome) ou vers
//! une installation locale de LLVM. Jamais de message de compilation.
//!
//! Licence MIT.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Version courante de Rose, utilisée pour nommer le helper éventuellement
/// présent dans le cache. Correspond au tag de release `v0.1.7`.
pub const VERSION_HELPER: &str = env!("CARGO_PKG_VERSION");

/// Message unique affiché quand aucune chaîne AOT locale n'est disponible.
/// Aucune mention d'outil de compilation : `rose comp` est la solution.
pub const MESSAGE_AOT_INDISPONIBLE: &str = "chaîne AOT LLVM non disponible — utilisez `rose comp` (autonome) ou installez LLVM localement";

/// Nom du binaire helper sur cette plateforme.
pub fn nom_helper() -> &'static str {
    if cfg!(windows) { "rose-aot.exe" } else { "rose-aot" }
}

/// Dossier de cache utilisateur (`~/.cache/rose`).
pub fn dossier_cache() -> PathBuf {
    if let Ok(v) = std::env::var("ROSE_CACHE") {
        return PathBuf::from(v);
    }
    let base = std::env::var("XDG_CACHE_HOME").ok().map(PathBuf::from)
        .or_else(|| std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("rose")
}

/// Chemin du helper versionné dans le cache (recherche uniquement).
pub fn chemin_cache() -> PathBuf {
    dossier_cache().join(format!("{}-{}", nom_helper(), VERSION_HELPER))
}

/// Un helper qui annonce LUI-MÊME l'absence de chaîne AOT ne doit jamais
/// recevoir la main : son message (périmé) remplacerait le nôtre et pourrait
/// contenir des conseils de compilation obsolètes. Sonde sans effet de bord
/// (`orc` sans programme = message d'usage, aucun fichier touché).
fn helper_utilisable(helper: &Path) -> bool {
    let sortie = match Command::new(helper).arg("orc").output() {
        Ok(s) => s,
        Err(_) => return false,
    };
    let mut texte = String::from_utf8_lossy(&sortie.stdout).into_owned();
    texte.push_str(&String::from_utf8_lossy(&sortie.stderr));
    let bas = texte.to_ascii_lowercase();
    !bas.contains("pas embarqu") && !bas.contains("non disponible")
}

/// Cherche un helper déjà présent sur la machine, SANS aucun accès réseau.
pub fn trouver_helper() -> Option<PathBuf> {
    // 1. à côté de l'exécutable courant — la priorité demandée.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            let p = d.join(nom_helper());
            if est_executable(&p) && helper_utilisable(&p) {
                return Some(p);
            }
        }
    }
    // 2. dans le cache utilisateur.
    let p = chemin_cache();
    if est_executable(&p) && helper_utilisable(&p) {
        return Some(p);
    }
    // 3. dans le PATH.
    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(':') {
            if dir.is_empty() { continue; }
            let p = Path::new(dir).join(nom_helper());
            if est_executable(&p) && helper_utilisable(&p) {
                return Some(p);
            }
        }
    }
    None
}

fn est_executable(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        match std::fs::metadata(p) {
            Ok(m) => m.is_file() && m.permissions().mode() & 0o111 != 0,
            Err(_) => false,
        }
    }
    #[cfg(not(unix))]
    {
        p.is_file()
    }
}

/// Point d'entrée : cherche le helper LOCAL puis lui passe la main.
/// S'il est absent, message clair et sortie — jamais de téléchargement.
pub fn lance(action: &str, argv: &[String]) -> ! {
    match trouver_helper() {
        Some(helper) => lance_depuis(&helper, action, argv),
        None => {
            eprintln!("rose: {}", MESSAGE_AOT_INDISPONIBLE);
            std::process::exit(1);
        }
    }
}

/// Exécute le helper AOT donné (sans recherche).
pub fn lance_depuis(helper: &Path, action: &str, argv: &[String]) -> ! {
    let mut cmd = Command::new(helper);
    cmd.arg(action);
    for a in argv.iter().skip(2) {
        cmd.arg(a);
    }
    match cmd.status() {
        Ok(st) => std::process::exit(st.code().unwrap_or(1)),
        Err(e) => {
            eprintln!("rose: impossible d'exécuter le helper AOT : {}", e);
            std::process::exit(1);
        }
    }
}
