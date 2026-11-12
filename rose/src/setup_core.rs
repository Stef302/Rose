//! Cœur de l'installation Rose — partagé par l'interface console
//! (`rose update`) et par l'installeur graphique `rose-setup`.
//!
//! Tout est écrit en HKCU / dossier utilisateur : **aucun droit
//! administrateur requis**, et rien dans `HKLM` (donc compatible avec les
//! stratégies d'entreprise qui verrouillent le registre machine).

use std::path::PathBuf;

/// Messages produits par une opération (affichés dans la console ou dans
/// la zone de texte de la fenêtre).
pub type Journal = Vec<String>;

/// Emplacement d'installation détecté (None = pas d'installation).
pub fn installation_existante() -> Option<String> {
    #[cfg(windows)]
    {
        let out = std::process::Command::new("reg")
            .args(["query", "HKCU\\Software\\Rose", "/v", "InstallDir"])
            .output()
            .ok()?;
        let txt = String::from_utf8_lossy(&out.stdout);
        for ligne in txt.lines() {
            if ligne.contains("InstallDir") {
                let parts: Vec<&str> = ligne.split_whitespace().collect();
                if parts.len() >= 3 {
                    let p = parts[2..].join(" ");
                    if PathBuf::from(&p).join("bin").join("rose.exe").exists() {
                        return Some(p);
                    }
                }
            }
        }
        None
    }
    #[cfg(unix)]
    {
        let home = std::env::var("HOME").unwrap_or_default();
        for p in ["/usr/local", &format!("{}/.local", home), "/opt/rose"] {
            if PathBuf::from(p).join("bin").join("rose").exists() {
                return Some(p.to_string());
            }
        }
        None
    }
}

/// Préfixe d'installation par défaut (celui qui sera proposé).
pub fn prefixe_defaut() -> String {
    #[cfg(windows)]
    {
        let home = std::env::var("USERPROFILE").unwrap_or_default();
        format!("{}\\AppData\\Local\\Programs\\Rose", home)
    }
    #[cfg(unix)]
    {
        let home = std::env::var("HOME").unwrap_or_default();
        if crate::setup_core::inscriptible("/usr/local/bin") {
            "/usr/local".to_string()
        } else {
            format!("{}/.local", home)
        }
    }
}

/// Le dossier est-il réellement inscriptible ?
pub fn inscriptible(dossier: &str) -> bool {
    let test = format!("{}/.rose_test_write", dossier);
    if std::fs::write(&test, b"x").is_ok() {
        let _ = std::fs::remove_file(&test);
        true
    } else {
        false
    }
}

/// Copie le binaire Rose + les icônes dans `prefixe`.
pub fn installer(prefixe: &str, source_exe: Option<&std::path::Path>) -> Journal {
    let mut j = Journal::new();
    let bindir = PathBuf::from(prefixe).join("bin");
    if std::fs::create_dir_all(&bindir).is_err() {
        j.push(format!("Échec : impossible de créer {}", bindir.display()));
        return j;
    }
    // nettoyage des anciens binaires (plus de rose-20.exe)
    for nom in ["rose.exe", "rose", "rose-20", "rose-20.exe", "rose-aot"] {
        let _ = std::fs::remove_file(bindir.join(nom));
    }
    let source = match source_exe {
        Some(p) => p.to_path_buf(),
        None => match std::env::current_exe() {
            Ok(p) => p,
            Err(e) => {
                j.push(format!("Échec : binaire courant introuvable ({})", e));
                return j;
            }
        },
    };
    let dest = bindir.join(if cfg!(windows) { "rose.exe" } else { "rose" });
    match std::fs::copy(&source, &dest) {
        Ok(_) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755));
            }
            j.push(format!("✓ {}", dest.display()));
        }
        Err(e) => {
            j.push(format!("Échec : copie impossible ({})", e));
            return j;
        }
    }
    // icônes à côté du binaire source
    if let Some(parent) = source.parent() {
        for (src, dst) in [("rose.ico", "rose.ico"), ("setup.ico", "rose_app.ico")] {
            let p = parent.join(src);
            if p.exists() {
                let _ = std::fs::copy(&p, bindir.join(dst));
                j.push(format!("✓ {}", dst));
            }
        }
        // helper AOT
        let aot = parent.join("rose-aot");
        if aot.exists() {
            let _ = std::fs::copy(&aot, bindir.join("rose-aot"));
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(bindir.join("rose-aot"), std::fs::Permissions::from_mode(0o755));
            }
            j.push("✓ rose-aot".to_string());
        }
    }
    j.push(format!("✓ {}", enregistrer(prefixe, &bindir)));
    j
}

/// Écrit le registre / la configuration, le PATH et les associations.
fn enregistrer(prefixe: &str, bindir: &std::path::Path) -> String {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("reg")
            .args(["add", "HKCU\\Software\\Rose", "/v", "InstallDir", "/t", "REG_SZ", "/d", prefixe, "/f"]).status();
        let _ = std::process::Command::new("reg")
            .args(["add", "HKCU\\Software\\Rose", "/v", "Version", "/t", "REG_SZ", "/d", env!("CARGO_PKG_VERSION"), "/f"]).status();
        // PATH : on retire d'abord TOUTES les entrées contenant « Rose »
        let mut ancien = String::new();
        if let Ok(o) = std::process::Command::new("reg")
            .args(["query", "HKCU\\Environment", "/v", "Path"]).output() {
            let txt = String::from_utf8_lossy(&o.stdout);
            for ligne in txt.lines() {
                if ligne.contains("Path") && ligne.contains("REG_") {
                    let parts: Vec<&str> = ligne.split_whitespace().collect();
                    if parts.len() >= 3 { ancien = parts[2..].join(" "); }
                }
            }
        }
        let mut segments: Vec<String> = ancien.split(';')
            .filter(|s| !s.trim().is_empty())
            .filter(|s| !s.to_lowercase().contains("rose"))
            .map(|s| s.to_string())
            .collect();
        segments.insert(0, bindir.display().to_string());
        let _ = std::process::Command::new("reg")
            .args(["add", "HKCU\\Environment", "/v", "Path", "/t", "REG_EXPAND_SZ",
                   "/d", &segments.join(";"), "/f"]).status();
        // association .rose
        let _ = std::process::Command::new("reg")
            .args(["add", "HKCU\\Software\\Classes\\.rose", "/ve", "/t", "REG_SZ", "/d", "Rose.File", "/f"]).status();
        let _ = std::process::Command::new("reg")
            .args(["add", "HKCU\\Software\\Classes\\Rose.File", "/ve", "/t", "REG_SZ", "/d", "Programme Rose", "/f"]).status();
        let _ = std::process::Command::new("reg")
            .args(["add", "HKCU\\Software\\Classes\\Rose.File\\DefaultIcon", "/ve", "/t", "REG_SZ",
                   "/d", &format!("{}\\rose.ico,0", bindir.display()), "/f"]).status();
        let _ = std::process::Command::new("reg")
            .args(["add", "HKCU\\Software\\Classes\\Rose.File\\shell\\open\\command", "/ve", "/t", "REG_SZ",
                   "/d", &format!("\"{}\\rose.exe\" \"%1\"", bindir.display()), "/f"]).status();
        "PATH et association .rose mis à jour".to_string()
    }
    #[cfg(unix)]
    {
        let home = std::env::var("HOME").unwrap_or_default();
        let ligne = format!("export PATH=\"{}/bin:$PATH\"", prefixe);
        for f in [format!("{}/.bashrc", home), format!("{}/.profile", home)] {
            let p = std::path::Path::new(&f);
            if p.exists() {
                let contenu = std::fs::read_to_string(&p).unwrap_or_default();
                if !contenu.contains(&format!("{}/bin", prefixe)) {
                    let _ = std::fs::write(&p, format!("{}\n{}", contenu, ligne));
                }
            }
        }
        // type MIME
        let dir = format!("{}/.local/share/mime/packages", home);
        let _ = std::fs::create_dir_all(&dir);
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<mime-info xmlns=\"http://www.freedesktop.org/standards/shared-mime-info\">\n  <mime-type type=\"text/x-rose\">\n    <comment>Rose source file</comment>\n    <glob pattern=\"*.rose\"/>\n  </mime-type>\n</mime-info>\n";
        let _ = std::fs::write(format!("{}/x-rose.xml", dir), xml);
        let _ = std::process::Command::new("xdg-mime").arg("install").arg(format!("{}/x-rose.xml", dir)).status();
        let _ = std::process::Command::new("update-mime-database").arg(format!("{}/.local/share/mime", home)).status();
        "PATH et type MIME mis à jour".to_string()
    }
}

/// Supprime l'installation (binaires, registre, PATH, associations).
pub fn desinstaller(prefixe: &str) -> Journal {
    let mut j = Journal::new();
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("reg")
            .args(["delete", "HKCU\\Software\\Rose", "/f"]).status();
        let _ = std::process::Command::new("reg")
            .args(["delete", "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Rose", "/f"]).status();
        let _ = std::process::Command::new("reg")
            .args(["delete", "HKCU\\Software\\Classes\\.rose", "/f"]).status();
        let _ = std::process::Command::new("reg")
            .args(["delete", "HKCU\\Software\\Classes\\Rose.File", "/f"]).status();
        let mut ancien = String::new();
        if let Ok(o) = std::process::Command::new("reg")
            .args(["query", "HKCU\\Environment", "/v", "Path"]).output() {
            let txt = String::from_utf8_lossy(&o.stdout);
            for ligne in txt.lines() {
                if ligne.contains("Path") && ligne.contains("REG_") {
                    let parts: Vec<&str> = ligne.split_whitespace().collect();
                    if parts.len() >= 3 { ancien = parts[2..].join(" "); }
                }
            }
        }
        let segments: Vec<String> = ancien.split(';')
            .filter(|s| !s.trim().is_empty())
            .filter(|s| !s.to_lowercase().contains("rose"))
            .map(|s| s.to_string())
            .collect();
        let _ = std::process::Command::new("reg")
            .args(["add", "HKCU\\Environment", "/v", "Path", "/t", "REG_EXPAND_SZ",
                   "/d", &segments.join(";"), "/f"]).status();
    }
    #[cfg(unix)]
    {
        let home = std::env::var("HOME").unwrap_or_default();
        for f in [format!("{}/.bashrc", home), format!("{}/.profile", home), format!("{}/.zshrc", home)] {
            let p = std::path::Path::new(&f);
            if p.exists() {
                let contenu = std::fs::read_to_string(&p).unwrap_or_default();
                let garde: Vec<&str> = contenu.lines()
                    .filter(|l| !l.contains(&format!("{}/bin", prefixe))).collect();
                let _ = std::fs::write(&p, garde.join("\n"));
            }
        }
    }
    // suppression des fichiers : uniquement ceux que nous avons posés
    let bin = PathBuf::from(prefixe).join("bin");
    for nom in ["rose.exe", "rose", "rose-20", "rose-20.exe", "rose-aot", "rose.ico", "rose_app.ico"] {
        let _ = std::fs::remove_file(bin.join(nom));
    }
    let _ = std::fs::remove_dir(&bin);
    let _ = std::fs::remove_dir(prefixe);
    j.push("✓ Rose désinstallé".to_string());
    j
}
