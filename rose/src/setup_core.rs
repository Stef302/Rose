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

/* ── mise à jour depuis GitHub ─────────────────────────────────────── */

/// Une release publiée sur GitHub.
pub struct Release {
    pub tag: String,
    pub version: String,
    /// (nom de l'asset, URL de téléchargement)
    pub assets: Vec<(String, String)>,
}

fn version_en_nombres(v: &str) -> Vec<u64> {
    v.trim_start_matches('v')
        .split(['.', '-', '+'])
        .map(|p| p.parse::<u64>().unwrap_or(0))
        .collect()
}

/// Compare deux versions (a > b → 1, a == b → 0, a < b → -1).
pub fn comparer_versions(a: &str, b: &str) -> i32 {
    let va = version_en_nombres(a);
    let vb = version_en_nombres(b);
    for i in 0..va.len().max(vb.len()) {
        let x = va.get(i).copied().unwrap_or(0);
        let y = vb.get(i).copied().unwrap_or(0);
        if x != y { return if x > y { 1 } else { -1 }; }
    }
    0
}

/// Nom de l'asset à télécharger pour la plateforme courante.
pub fn nom_asset() -> &'static str {
    #[cfg(target_os = "windows")]
    { "Rose-Setup" }              // complété par le tag
    #[cfg(target_os = "linux")]
    {
        if cfg!(target_arch = "aarch64") { "rose-aarch64-linux" } else { "rose-x86_64-linux" }
    }
    #[cfg(target_os = "macos")]
    { "rose-aarch64-macos" }
}

/// Exécute `curl` (ou `wget`) pour récupérer une URL.
fn http_get(url: &str) -> Option<String> {
    let out = std::process::Command::new("curl")
        .args(["-sSL", "--max-time", "30", url])
        .output();
    if let Ok(o) = out {
        if o.status.success() {
            return Some(String::from_utf8_lossy(&o.stdout).into_owned());
        }
    }
    // repli : wget
    let out = std::process::Command::new("wget")
        .args(["-q", "-O", "-", "--timeout=30", url])
        .output();
    if let Ok(o) = out {
        if o.status.success() {
            return Some(String::from_utf8_lossy(&o.stdout).into_owned());
        }
    }
    None
}

/// Télécharge une URL dans un fichier. Retourne true si le fichier existe
/// et n'est pas vide.
fn http_telecharger(url: &str, dest: &std::path::Path) -> bool {
    let _ = std::fs::remove_file(dest);
    let st = std::process::Command::new("curl")
        .args(["-sSL", "--max-time", "180", "-o", dest.to_string_lossy().as_ref(), url])
        .status();
    if let Ok(s) = st {
        if s.success() {
            if let Ok(m) = std::fs::metadata(dest) {
                if m.len() > 0 { return true; }
            }
        }
    }
    let st = std::process::Command::new("wget")
        .args(["-q", "-O", dest.to_string_lossy().as_ref(), url])
        .status();
    if let Ok(s) = st {
        if s.success() {
            if let Ok(m) = std::fs::metadata(dest) {
                return m.len() > 0;
            }
        }
    }
    false
}

/// URL de base du miroir des artefacts (site de Rose).
/// On évite l'API GitHub (limites de rate, besoin de jeton) : les fichiers
/// sont servis par GitHub Pages / raw.githubusercontent.com.
pub fn url_base_site() -> &'static str {
    "https://raw.githubusercontent.com/Stef302/Rose/main/docs/telechargements"
}

/// URL de base secondaire (site GitHub Pages) — repli.
pub fn url_base_pages() -> &'static str {
    "https://stef302.github.io/Rose/telechargements"
}

/// Cherche la dernière version publiée sur le SITE de Rose (pas GitHub).
/// Lit le fichier VERSION (un tag « v5.12.0 ») et construit l'URL de chaque
/// artefact à partir de l'URL de base du site.
pub fn chercher_version_site() -> Option<Release> {
    let url = format!("{}/VERSION", url_base_site());
    let tag = http_get(&url)?;
    let tag = tag.trim().to_string();
    if tag.is_empty() || !tag.starts_with('v') { return None; }
    let version = tag.trim_start_matches('v').to_string();

    // construire la liste des assets connus pour cette version
    let mut assets = Vec::new();
    let base = url_base_site();
    let mut noms = Vec::new();
    #[cfg(target_os = "windows")]
    { noms.push(format!("Rose-Setup-{}.exe", version)); }
    #[cfg(target_os = "linux")]
    {
        if cfg!(target_arch = "aarch64") { noms.push("rose-aarch64-linux".to_string()); }
        else { noms.push("rose-x86_64-linux".to_string()); }
    }
    noms.push("SHA256SUMS".to_string());
    for nom in noms {
        assets.push((nom.clone(), format!("{}/{}", base, nom)));
    }
    Some(Release { tag, version, assets })
}

/// Extrait la valeur scalaire qui suit une clé dans du JSON (sans parseur).
fn extraire_valeur_json(json: &str, cle: &str) -> String {
    match json.find(cle) {
        Some(i) => {
            let apres = &json[i + cle.len()..];
            match apres.find('"') {
                Some(j) => {
                    let debut = &apres[j + 1..];
                    match debut.find('"') {
                        Some(k) => debut[..k].to_string(),
                        None => String::new(),
                    }
                }
                None => String::new(),
            }
        }
        None => String::new(),
    }
}

/// Version du binaire installé (exécuté) — None si absent.
pub fn version_installee(prefixe: &str) -> Option<String> {
    let exe = PathBuf::from(prefixe).join("bin").join(if cfg!(windows) { "rose.exe" } else { "rose" });
    let out = std::process::Command::new(&exe).arg("--version").output().ok()?;
    let txt = String::from_utf8_lossy(&out.stdout);
    // « Rose 5.11.0 — … »
    let apres = txt.strip_prefix("Rose ")?;
    let v: String = apres.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    if v.is_empty() { None } else { Some(v) }
}

/// SHA-256 d'un fichier local (hexadécimal).
pub fn sha256_fichier(chemin: &std::path::Path) -> Option<String> {
    let data = std::fs::read(chemin).ok()?;
    Some(crate::sha256::sha256_hex(&data))
}

/// Met à jour l'installation depuis GitHub. Retourne le journal.
pub fn mettre_a_jour(prefixe: &str, force: bool) -> Journal {
    let mut j = Journal::new();
    let installée = version_installee(prefixe).unwrap_or_else(|| "0".to_string());
    j.push(format!("Version installée : {}", installée));

    let release = match chercher_version_site() {
        Some(r) => r,
        None => {
            // repli : site GitHub Pages
            let url = format!("{}/VERSION", url_base_pages());
            match http_get(&url) {
                Some(t) => {
                    let tag = t.trim().to_string();
                    if tag.is_empty() || !tag.starts_with('v') {
                        j.push("✗ impossible de joindre le site de Rose (réseau ?)".to_string());
                        return j;
                    }
                    let version = tag.trim_start_matches('v').to_string();
                    let mut assets = Vec::new();
                    let base = url_base_pages();
                    #[cfg(target_os = "windows")]
                    { assets.push((format!("Rose-Setup-{}.exe", version), format!("{}/Rose-Setup-{}.exe", base, version))); }
                    #[cfg(target_os = "linux")]
                    {
                        let nom = if cfg!(target_arch = "aarch64") { "rose-aarch64-linux" } else { "rose-x86_64-linux" };
                        assets.push((nom.to_string(), format!("{}/{}", base, nom)));
                    }
                    assets.push(("SHA256SUMS".to_string(), format!("{}/SHA256SUMS", base)));
                    Release { tag, version, assets }
                }
                None => {
                    j.push("✗ impossible de joindre le site de Rose (réseau ?)".to_string());
                    return j;
                }
            }
        }
    };
    j.push(format!("Dernière release : {}", release.tag));

    if !force && comparer_versions(&release.version, &installée) <= 0 {
        j.push(format!("✓ Rose {} est déjà à jour", installée));
        return j;
    }

    // choisir l'asset
    let mut asset: Option<(String, String)> = None;
    if cfg!(windows) {
        asset = release.assets.iter()
            .find(|(n, _)| n.starts_with("Rose-Setup") && n.ends_with(".exe"))
            .cloned();
    } else {
        let cible = nom_asset();
        asset = release.assets.iter().find(|(n, _)| n == cible).cloned();
    }
    let (nom, url) = match asset {
        Some(a) => a,
        None => {
            j.push(format!("✗ aucun artefact « {} » dans la release", nom_asset()));
            return j;
        }
    };

    // téléchargement dans un dossier temporaire
    let tmp = std::env::temp_dir().join(format!("rose-update-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp);
    let fichier = tmp.join(&nom);
    j.push(format!("Téléchargement : {}", nom));
    if !http_telecharger(&url, &fichier) {
        j.push("✗ téléchargement impossible".to_string());
        let _ = std::fs::remove_dir_all(&tmp);
        return j;
    }

    // vérification SHA-256 via SHA256SUMS
    if let Some((_, url_sums)) = release.assets.iter().find(|(n, _)| n == "SHA256SUMS") {
        let sums = tmp.join("SHA256SUMS");
        if http_telecharger(url_sums, &sums) {
            if let Ok(contenu) = std::fs::read_to_string(&sums) {
                for ligne in contenu.lines() {
                    let parts: Vec<&str> = ligne.split_whitespace().collect();
                    if parts.len() == 2 && parts[1] == nom {
                        if let Some(attendu) = sha256_fichier(&fichier) {
                            if attendu == parts[0] {
                                j.push("✓ SHA-256 vérifié".to_string());
                            } else {
                                j.push(format!("✗ SHA-256 incorrect (attendu {})", parts[0]));
                                let _ = std::fs::remove_dir_all(&tmp);
                                return j;
                            }
                        }
                    }
                }
            }
        }
    }

    // application
    #[cfg(windows)]
    {
        // Sur Windows, on ne peut pas remplacer un .exe en cours
        // d'exécution : on télécharge le nouvel installeur graphique et on
        // le lance (il détectera l'installation existante et proposera de
        // réinstaller). L'installeur embarque déjà le moteur + les icônes.
        if let Some((n, url)) = release.assets.iter().find(|(n, _)| n.starts_with("Rose-Setup") && n.ends_with(".exe")).cloned() {
            let dest = tmp.join(&n);
            j.push(format!("Téléchargement de l'installeur : {}", n));
            if http_telecharger(&url, &dest) {
                j.push("✓ Installeur téléchargé — lancement…".to_string());
                let _ = std::process::Command::new(&dest).spawn();
            } else {
                j.push("✗ téléchargement de l'installeur impossible".to_string());
            }
        } else {
            j.push("✗ aucun installeur dans la release".to_string());
        }
    }
    #[cfg(not(windows))]
    {
        let dest = PathBuf::from(prefixe).join("bin").join("rose");
        let _ = std::fs::remove_file(&dest);
        if std::fs::copy(&fichier, &dest).is_ok() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755));
            }
            j.push(format!("✓ {} mis à jour", dest.display()));
        } else {
            j.push("✗ remplacement impossible".to_string());
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);
    j.push(format!("✓ Rose {} installé", release.version));
    j
}
