//! Auto-installation du helper AOT — « chercher sur internet tout seul ».
//!
//! Le binaire principal de Rose est LÉGER : il ne contient pas LLVM et ne
//! charge pas libLLVM au démarrage. Mais la chaîne AOT (`rose orc`,
//! `rose link`) a besoin de LLVM. Au lieu d'imposer une reconstruction
//! (`cargo build --features llvm`), Rose cherche le helper AOT sur la
//! machine et, s'il est absent, le TÉLÉCHARGE tout seul.
//!
//! Ordre de recherche du helper `rose-aot` :
//!   1. à côté de l'exécutable courant ;
//!   2. dans le cache utilisateur `~/.cache/rose/` ;
//!   3. dans le PATH ;
//!   4. sinon : téléchargement depuis l'URL de la release, vérification
//!      SHA-256, installation dans le cache, puis exécution.
//!
//! L'URL et l'empreinte attendue peuvent être surchargées par les variables
//! d'environnement `ROSE_AOT_URL` et `ROSE_AOT_SHA256` (utile pour un
//! miroir interne).
//!
//! Licence MIT.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Version du helper AOT à installer (doit correspondre au TAG de release).
pub const VERSION_HELPER: &str = "v5.10.0";

/// Empreintes SHA-256 attendues par plateforme (vides = pas de vérification ;
/// dans ce cas un avertissement est affiché). L'empreinte peut aussi être
/// surchargée par ROSE_AOT_SHA256 (miroir interne, tests, déploiement privé).
pub fn sha256_attendu(cible: &str) -> &'static str {
    match cible {
        "aarch64-linux" => SHA256_HELPER_AARCH64_LINUX,
        _ => "",
    }
}

const SHA256_HELPER_AARCH64_LINUX: &str =
    "4cc7db58eb91de3db05bd61c2c3a819bfaa4e9c3b972c813572853dc2573da48";

/// Empreinte de référence de la release (miroir versionné :
/// docs/telechargements/, SHA256SUMS).
pub const SHA256_HELPER: &str =
    "4cc7db58eb91de3db05bd61c2c3a819bfaa4e9c3b972c813572853dc2573da48";

/// Nom du binaire helper.
pub const NOM_HELPER: &str = "rose-aot";

/// Plateforme cible (pour le nom de l'artefact de release).
pub fn cible() -> String {
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("aarch64", "linux") => "aarch64-linux".to_string(),
        ("x86_64", "linux") => "x86_64-linux".to_string(),
        ("x86_64", "windows") => "x86_64-windows.exe".to_string(),
        ("aarch64", "macos") => "aarch64-macos".to_string(),
        ("x86_64", "macos") => "x86_64-macos".to_string(),
        (a, o) => format!("{}-{}", a, o),
    }
}

/// URL par défaut du helper AOT : la release GitHub du projet ; si le
/// fichier n'y est pas (404), repli sur le miroir versionné du dépôt
/// (docs/telechargements, servi par raw.githubusercontent.com).
pub fn url_par_defaut() -> String {
    format!(
        "https://github.com/Stef302/Rose/releases/download/{}/{}-{}",
        VERSION_HELPER,
        NOM_HELPER,
        cible()
    )
}

pub fn url_miroir() -> String {
    format!(
        "https://raw.githubusercontent.com/Stef302/Rose/main/docs/telechargements/{}-{}",
        NOM_HELPER,
        cible()
    )
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

/// Chemin du helper installé dans le cache.
pub fn chemin_cache() -> PathBuf {
    dossier_cache().join(format!("{}-{}", NOM_HELPER, VERSION_HELPER))
}

/// Cherche un helper déjà présent sur la machine.
pub fn trouver_helper() -> Option<PathBuf> {
    // 1. à côté de l'exécutable courant
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            let p = d.join(NOM_HELPER);
            if est_executable(&p) {
                return Some(p);
            }
        }
    }
    // 2. dans le cache utilisateur
    let p = chemin_cache();
    if est_executable(&p) {
        return Some(p);
    }
    // 3. dans le PATH
    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(':') {
            if dir.is_empty() { continue; }
            let p = Path::new(dir).join(NOM_HELPER);
            if est_executable(&p) {
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

/// Télécharge le helper, vérifie son empreinte et l'installe dans le cache.
pub fn installer_helper() -> Result<PathBuf, String> {
    let url = std::env::var("ROSE_AOT_URL").unwrap_or_else(|_| url_par_defaut());
    let attendu = std::env::var("ROSE_AOT_SHA256")
        .unwrap_or_else(|_| sha256_attendu(&cible()).to_string());

    eprintln!("rose: chaîne AOT LLVM absente — téléchargement du helper…");
    eprintln!("      {}", url);

    let cache = dossier_cache();
    std::fs::create_dir_all(&cache).map_err(|e| format!("création du cache : {}", e))?;
    let tmp = cache.join(format!(".{}.tmp", NOM_HELPER));

    // téléchargement : curl ou wget (pas de dépendance TLS à embarquer).
    // La release d'abord ; si elle ne sert pas l'artefact, le miroir
    // versionné du dépôt (docs/telechargements) prend le relais.
    let mut telecharge = downloadable(&url, &tmp)?;
    if !telecharge {
        let miroir = url_miroir();
        eprintln!("      (release absente — essai du miroir {})", miroir);
        telecharge = downloadable(&miroir, &tmp)?;
    }
    if !telecharge {
        let _ = std::fs::remove_file(&tmp);
        return Err("téléchargement impossible (outil absent ou URL inaccessible)".to_string());
    }

    // vérification d'intégrité
    let octets = std::fs::read(&tmp).map_err(|e| format!("lecture du téléchargé : {}", e))?;
    if !attendu.trim().is_empty() {
        let obtenu = crate::sha256::sha256_hex(&octets);
        if obtenu != attendu.trim().to_lowercase() {
            let _ = std::fs::remove_file(&tmp);
            return Err(format!(
                "empreinte SHA-256 inattendue (obtenu {}, attendu {})",
                obtenu, attendu
            ));
        }
    } else {
        eprintln!("      attention : aucune empreinte de référence (ROSE_AOT_SHA256 non définie)");
    }

    // installation
    let cible = chemin_cache();
    let _ = std::fs::remove_file(&cible);
    std::fs::rename(&tmp, &cible).map_err(|e| format!("installation : {}", e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(&cible).map_err(|e| e.to_string())?.permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(&cible, perm).map_err(|e| format!("permissions : {}", e))?;
    }
    Ok(cible)
}

fn downloadable(url: &str, dest: &Path) -> Result<bool, String> {
    if which("curl").is_some() {
        let st = Command::new("curl")
            .args(["-fsSL", "--retry", "2", "-o"])
            .arg(dest)
            .arg(url)
            .status();
        if let Ok(st) = st {
            if st.success() { return Ok(true); }
        }
    }
    if which("wget").is_some() {
        let st = Command::new("wget")
            .args(["-q", "-O"])
            .arg(dest)
            .arg(url)
            .status();
        if let Ok(st) = st {
            if st.success() { return Ok(true); }
        }
    }
    Ok(false)
}

fn which(prog: &str) -> Option<PathBuf> {
    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(':') {
            if dir.is_empty() { continue; }
            let p = Path::new(dir).join(prog);
            if est_executable(&p) { return Some(p); }
        }
    }
    None
}

/// Point d'entrée : trouve (ou installe) le helper AOT puis lui passe la
/// main. Ne rend jamais : termine par le code de sortie du helper.
pub fn lance(action: &str, argv: &[String]) -> ! {
    let helper = match trouver_helper() {
        Some(p) => p,
        None => match installer_helper() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("rose: la chaîne AOT LLVM n'est pas disponible.");
                eprintln!("      {}", e);
                eprintln!("      autre solution : cargo build --release --features llvm");
                std::process::exit(1);
            }
        },
    };
    lance_depuis(&helper, action, argv)
}

/// Exécute le helper AOT donné (sans recherche ni téléchargement).
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
