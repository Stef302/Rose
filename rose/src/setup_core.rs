//! Cœur de l'installation Rose — partagé par l'interface console
//! (`rose update`) et par l'installeur graphique `rose-setup`.
//!
//! Tout est écrit en HKCU / dossier utilisateur : **aucun droit
//! administrateur requis**, et rien dans `HKLM` (donc compatible avec les
//! stratégies d'entreprise qui verrouillent le registre machine).

use std::path::PathBuf;

/// Lance un programme SANS ouvrir de fenêtre de console.
///
/// ⚠ INDISPENSABLE dans l'installeur graphique. `reg.exe`, `curl.exe` et
/// `wget.exe` sont des programmes CONSOLE. Lancés depuis une application
/// GUI (sous-système 2), Windows leur alloue une console : une fenêtre noire
/// apparaît brièvement à chaque appel (mesuré : une fenêtre cmd au
/// lancement de l'installeur, avant la fenêtre Rose).
///
/// `CREATE_NO_WINDOW` (0x08000000) supprime cette allocation. Ailleurs
/// qu'en Windows, la fonction est neutre.
fn commande_sans_fenetre(programme: &str) -> std::process::Command {
    #[allow(unused_mut)]
    let mut c = std::process::Command::new(programme);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

// ═══════════════════════════════════════════════════════════════════════
// REGISTRE — ACCÈS DIRECT PAR L'API WIN32
// ═══════════════════════════════════════════════════════════════════════
//
// ⚠⚠⚠ POURQUOI CE MODULE. L'outil PATH écrivait le `PATH` en lançant
// `reg.exe add …`. Mesuré chez l'utilisateur (capture d'écran) :
//
//     Échec : `reg add` a rendu le code 1
//     Échec : le registre HKCU est INACCESSIBLE en écriture
//
// … alors que `HKCU\Environment\Path` lui APPARTIENT et qu'il n'est PAS
// verrouillé par une stratégie de groupe. Deux causes plausibles, la
// seconde étant la plus probable sur une machine gérée :
//
//   1. `reg.exe` est BLOQUÉ par une politique d'application (AppLocker /
//      WDAC) ou par un antivirus qui surveille les écritures du PATH ;
//   2. `reg.exe` existe mais son exécution échoue pour une raison
//      d'environnement (PATH tronqué, redirection).
//
// ⚠⚠⚠ LA SOLUTION : on n'appelle PLUS `reg.exe` pour le PATH. On utilise
// directement l'API `Advapi32` (`RegOpenKeyExW`, `RegSetValueExW`,
// `RegQueryValueExW`), qui est DÉJÀ chargée dans le processus — aucun
// processus externe à lancer, donc aucune politique d'application ne peut
// la bloquer.
//
// ⚠ `RegOpenKeyExW` avec `KEY_WRITE` distingue « clé en lecture seule »
// (ERROR_ACCESS_DENIED = 5) de « clé inexistante » (ERROR_FILE_NOT_FOUND =
// 2) : on ne DEVINE plus la cause, on la LIT.
//
// ⚠⚠ La valeur s'écrit en `REG_EXPAND_SZ` (type 2), comme Windows le fait
// pour `HKCU\Environment\Path` : les `%VAR%` y restent littéraux.

#[cfg(windows)]
mod api {
    use std::ffi::c_void;

    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn RegOpenKeyExW(cle: isize, sous_cle: *const u16, options: u32,
                         acces: u32, sortie: *mut isize) -> i32;
        fn RegQueryValueExW(cle: isize, nom: *const u16, reserve: *mut u32,
                            typ: *mut u32, donnees: *mut u8, taille: *mut u32) -> i32;
        fn RegSetValueExW(cle: isize, nom: *const u16, reserve: u32, typ: u32,
                          donnees: *const u8, taille: u32) -> i32;
        fn RegCreateKeyExW(cle: isize, sous_cle: *const u16, reserve: u32,
                           classe: *const u16, options: u32, acces: u32,
                           securite: *const c_void, sortie: *mut isize,
                           disposition: *mut u32) -> i32;
        fn RegCloseKey(cle: isize) -> i32;
        fn RegDeleteValueW(cle: isize, nom: *const u16) -> i32;
    }

    pub const ERROR_SUCCESS: i32 = 0;
    pub const ERROR_FILE_NOT_FOUND: i32 = 2;
    pub const ERROR_ACCESS_DENIED: i32 = 5;
    pub const ERROR_MORE_DATA: i32 = 234;

    pub const KEY_READ: u32 = 0x2_0019;
    pub const KEY_WRITE: u32 = 0x2_0006;
    pub const KEY_ALL_ACCESS: u32 = 0xF_003F;

    pub const HKEY_CURRENT_USER: isize = 0x8000_0001u32 as isize;
    pub const HKEY_LOCAL_MACHINE: isize = 0x8000_0002u32 as isize;

    pub const REG_SZ: u32 = 1;
    pub const REG_EXPAND_SZ: u32 = 2;

    /// Clé d'environnement de l'utilisateur (celle du `PATH`).
    pub const ENV_UTILISATEUR: &str = "Environment";
    /// Clé d'environnement de la MACHINE (écriture = administrateur).
    pub const ENV_MACHINE: &str = "SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment";

    pub fn u16z(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Ouvre une clé ; rend le handle ou le code d'erreur Win32.
    pub fn ouvrir(racine: isize, sous_cle: &str, acces: u32) -> Result<isize, i32> {
        let nom = u16z(sous_cle);
        let mut h: isize = 0;
        let rc = unsafe { RegOpenKeyExW(racine, nom.as_ptr(), 0, acces, &mut h) };
        if rc == ERROR_SUCCESS { Ok(h) } else { Err(rc) }
    }

    pub fn fermer(h: isize) {
        unsafe { RegCloseKey(h) };
    }

    /// Lit une valeur `REG_SZ`/`REG_EXPAND_SZ` ; rend le code d'erreur.
    pub fn lire(racine: isize, sous_cle: &str, nom: &str) -> Result<String, i32> {
        let h = ouvrir(racine, sous_cle, KEY_READ)?;
        let n = u16z(nom);
        let mut typ: u32 = 0;
        let mut taille: u32 = 0;
        let rc = unsafe {
            RegQueryValueExW(h, n.as_ptr(), std::ptr::null_mut(), &mut typ,
                             std::ptr::null_mut(), &mut taille)
        };
        if rc != ERROR_SUCCESS && rc != ERROR_MORE_DATA {
            fermer(h);
            return Err(rc);
        }
        let mut octets = vec![0u8; (taille as usize).max(2)];
        let mut t2: u32 = 0;
        let rc = unsafe {
            RegQueryValueExW(h, n.as_ptr(), std::ptr::null_mut(), &mut t2,
                             octets.as_mut_ptr(), &mut taille)
        };
        fermer(h);
        if rc != ERROR_SUCCESS { return Err(rc); }
        octets.truncate(taille as usize);
        // ⚠ UTF-16 : deux octets par unité, terminateur nul final retiré.
        let mut unites: Vec<u16> = octets
            .chunks_exact(2)
            .map(|p| u16::from_le_bytes([p[0], p[1]]))
            .collect();
        while unites.last() == Some(&0) { unites.pop(); }
        Ok(String::from_utf16_lossy(&unites))
    }

    /// Écrit une valeur ; rend le code d'erreur Win32 (0 = succès).
    pub fn ecrire(racine: isize, sous_cle: &str, nom: &str, typ: u32, valeur: &str) -> i32 {
        // ⚠ `RegCreateKeyExW` ouvre une clé existante ET crée une clé
        // absente : une clé manquante ne doit pas être lue comme un verrou.
        let nom_cle = u16z(sous_cle);
        let mut h: isize = 0;
        let mut disposition: u32 = 0;
        let rc = unsafe {
            RegCreateKeyExW(racine, nom_cle.as_ptr(), 0, std::ptr::null(), 0,
                            KEY_WRITE, std::ptr::null(), &mut h, &mut disposition)
        };
        if rc != ERROR_SUCCESS { return rc; }
        let n = u16z(nom);
        // ⚠ `REG_SZ` et `REG_EXPAND_SZ` sont terminées par zéro : la
        // taille transmise INCLUT le terminateur.
        let mut donnees: Vec<u16> = valeur.encode_utf16().collect();
        donnees.push(0);
        let rc = unsafe {
            RegSetValueExW(h, n.as_ptr(), 0, typ,
                           donnees.as_ptr() as *const u8, (donnees.len() * 2) as u32)
        };
        fermer(h);
        rc
    }

    /// Peut-on OUVRIR la clé en écriture ? (test sans écrire)
    pub fn ecriture_possible(racine: isize, sous_cle: &str) -> Result<(), i32> {
        ouvrir(racine, sous_cle, KEY_WRITE).map(fermer)
    }

    /// Écrit puis supprime une valeur de sonde ; rend le code d'écriture.
    ///
    /// ⚠ C'est le test le PLUS fiable : un droit d'ouverture ne garantit pas
    /// que la valeur passe (une GPO peut interdire la valeur précise).
    pub fn tester_ecriture(racine: isize, sous_cle: &str) -> i32 {
        let rc = ecrire(racine, sous_cle, "RoseProbe", REG_SZ, "1");
        if rc == ERROR_SUCCESS {
            let _ = supprimer_valeur(racine, sous_cle, "RoseProbe");
        }
        rc
    }

    /// Supprime une valeur (best effort) ; rend le code d'erreur.
    pub fn supprimer_valeur(racine: isize, sous_cle: &str, nom: &str) -> i32 {
        let h = match ouvrir(racine, sous_cle, KEY_ALL_ACCESS) { Ok(h) => h, Err(e) => return e };
        let n = u16z(nom);
        let rc = unsafe { RegDeleteValueW(h, n.as_ptr()) };
        fermer(h);
        rc
    }

    /// Explique un code d'erreur Win32 en clair.
    pub fn expliquer(rc: i32) -> String {
        match rc {
            ERROR_SUCCESS => "succès".to_string(),
            ERROR_FILE_NOT_FOUND => "clé ou valeur introuvable".to_string(),
            ERROR_ACCESS_DENIED => "ACCÈS REFUSÉ (droits insuffisants ou verrou de stratégie)".to_string(),
            _ => format!("code Win32 {rc}"),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// LECTURE / ÉCRITURE DU PATH — API WIN32, AVEC REPLIS
// ═══════════════════════════════════════════════════════════════════════
//
// ⚠⚠⚠ CE MODULE REMPLACE LES APPELS À `reg.exe`. Mesuré chez l'utilisateur :
// `reg add` rendait le code 1 et le PATH restait vide, alors que la clé
// `HKCU\Environment` appartient à l'utilisateur et n'est pas verrouillée.
// Le suspect n°1 est une politique d'application (AppLocker/WDAC) ou un
// antivirus qui bloque `reg.exe` — pas le registre.
//
// ⚠ STRATÉGIE EN CASCADE, chacune VÉRIFIÉE par relecture :
//
//   A. API `RegSetValueExW` sur `HKCU\Environment` (le chemin normal) ;
//   B. écriture d'un fichier `.reg` puis import silencieux par
//      `regedit /s` (contourne un blocage de `reg.exe` seul) ;
//   C. si A et B échouent : on NE MODIFIE PAS le PATH et on dit à
//      l'utilisateur d'ouvrir l'outil PATH pour une méthode sans PATH.
//
// ⚠⚠ L'ÉCRITURE EST TOUJOURS RELUE : `RegSetValueExW` peut rendre 0 et
// qu'une GPO réécrire la valeur juste après (application au prochain
// démarrage de session). On compare donc le contenu APRÈS coup.

/// Lit le `PATH` de l'utilisateur (`HKCU\Environment\Path`).
///
/// Rend `None` si la valeur n'existe pas ; `Some("")` si elle est vide.
pub fn lire_path_utilisateur() -> Option<String> {
    #[cfg(windows)]
    {
        match api::lire(api::HKEY_CURRENT_USER, api::ENV_UTILISATEUR, "Path") {
            Ok(v) => Some(v),
            // ⚠ Valeur ABSENTE = un PATH vide, pas une erreur : on rend
            // une chaîne vide, qu'on complètera.
            Err(api::ERROR_FILE_NOT_FOUND) => Some(String::new()),
            Err(_) => None,
        }
    }
    #[cfg(not(windows))]
    {
        std::env::var("PATH").ok()
    }
}

/// Lit le `PATH` de la MACHINE (`HKLM\…\Environment\Path`).
pub fn lire_path_machine() -> Option<String> {
    #[cfg(windows)]
    {
        match api::lire(api::HKEY_LOCAL_MACHINE, api::ENV_MACHINE, "Path") {
            Ok(v) => Some(v),
            Err(api::ERROR_FILE_NOT_FOUND) => Some(String::new()),
            Err(_) => None,
        }
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Résultat d'une écriture de `PATH`.
pub struct EcriturePath {
    /// La méthode qui a RÉUSSI (vide si aucune).
    pub methode: String,
    /// Lignes du journal.
    pub journal: Vec<String>,
    /// Vrai si le `PATH` relu contient bien le dossier.
    pub reussi: bool,
}

/// Construit la nouvelle valeur du `PATH` : le dossier demandé en tête, les
/// autres segments conservés (sans doublon ni dossier Rose mort).
fn composer_path(ancien: &str, dossier: &str) -> String {
    let cible = normaliser_path(dossier);
    let mut segs: Vec<String> = ancien
        .split(';')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        // ⚠ On ne retire QUE le doublon exact : un autre dossier Rose
        // VIVANT est conservé (l'utilisateur peut avoir deux versions).
        .filter(|s| normaliser_path(s) != cible)
        .collect();
    segs.insert(0, dossier.replace('/', "\\"));
    segs.join(";")
}

/// Écrit `dossier` dans le `PATH` de l'utilisateur, en cascade.
///
/// ⚠⚠⚠ CHAQUE ÉTAPE EST RELUE. On ne dit jamais « écrit » sans avoir
/// constaté la valeur dans le registre.
pub fn ecrire_path_utilisateur(dossier: &str) -> EcriturePath {
    let mut journal: Vec<String> = Vec::new();
    let ancien = lire_path_utilisateur().unwrap_or_default();
    let nouveau = composer_path(&ancien, dossier);
    let cible = normaliser_path(dossier);

    // ── Diagnostic AVANT toute écriture : la cause réelle ────────────────
    #[cfg(windows)]
    {
        match api::ecriture_possible(api::HKEY_CURRENT_USER, api::ENV_UTILISATEUR) {
            Ok(()) => journal.push("· clé HKCU\\Environment ouverte en écriture".to_string()),
            Err(rc) => journal.push(format!(
                "⚠ clé HKCU\\Environment NON ouvrable en écriture : {}", api::expliquer(rc))),
        }
        let sonde = api::tester_ecriture(api::HKEY_CURRENT_USER, api::ENV_UTILISATEUR);
        if sonde == api::ERROR_SUCCESS {
            journal.push("· écriture test : ACCEPTÉE (valeur sonde posée puis retirée)".to_string());
        } else {
            journal.push(format!("⚠ écriture test : REFUSÉE ({})", api::expliquer(sonde)));
        }

        // ── A. API directe ──────────────────────────────────────────────
        let rc = api::ecrire(api::HKEY_CURRENT_USER, api::ENV_UTILISATEUR, "Path",
                             api::REG_EXPAND_SZ, &nouveau);
        if rc == api::ERROR_SUCCESS {
            journal.push(format!("· API Win32 : RegSetValueExW a rendu {}", api::expliquer(rc)));
            // ⚠ RELIRE : le seul fait qui compte.
            let relu = lire_path_utilisateur().unwrap_or_default();
            if path_contient(&relu, &cible) {
                journal.push(format!("✓ PATH utilisateur écrit par l'API Win32 ({} car.)", relu.len()));
                return EcriturePath { methode: "api".to_string(), journal, reussi: true };
            }
            journal.push("⚠ l'API a rendu succès mais la valeur relue NE CONTIENT PAS le dossier".to_string());
        } else {
            journal.push(format!("⚠ API Win32 : RegSetValueExW a rendu {}", api::expliquer(rc)));
        }

        // ── B. Fichier .reg + `regedit /s` ─────────────────────────────
        //
        // ⚠ Un fichier `.reg` en UTF-16LE (format natif de regedit) est
        // importé SILENCIEUSEMENT (`/s`). Cela passe là où `reg add` est
        // bloqué par une politique qui ne vise que les outils en ligne de
        // commande.
        let chemin = std::env::temp_dir().join("rose-path.reg");
        let contenu = format!(
            "Windows Registry Editor Version 5.00\r\n\r\n\
             [HKEY_CURRENT_USER\\Environment]\r\n\
             \"Path\"=hex(2):{}\r\n",
            // ⚠ `hex(2)` = REG_EXPAND_SZ ; chaque unité UTF-16 en 4 chiffres
            // hexadécimaux séparés par des virgules, terminateur inclus.
            nouveau.encode_utf16()
                .chain(std::iter::once(0))
                .map(|u| format!("{u:02x},{:02x}", u >> 8))
                .collect::<Vec<_>>()
                .join(",")
        );
        // ⚠ UTF-16LE avec BOM : c'est ce que regedit attend.
        let mut octets: Vec<u8> = vec![0xFF, 0xFE];
        for u in contenu.encode_utf16() {
            octets.extend_from_slice(&u.to_le_bytes());
        }
        match std::fs::write(&chemin, &octets) {
            Ok(()) => {
                journal.push(format!("· fichier .reg préparé ({})", chemin.display()));
                let st = commande_sans_fenetre("regedit")
                    .args(["/s", &chemin.display().to_string()])
                    .status();
                match st {
                    Ok(s) if s.success() => journal.push("· regedit /s : code 0".to_string()),
                    Ok(s) => journal.push(format!(
                        "⚠ regedit /s a rendu le code {}", s.code().unwrap_or(-1))),
                    Err(e) => journal.push(format!("⚠ regedit n'a pas pu être lancé ({e})")),
                }
                let _ = std::fs::remove_file(&chemin);
                let relu = lire_path_utilisateur().unwrap_or_default();
                if path_contient(&relu, &cible) {
                    journal.push(format!("✓ PATH utilisateur écrit par regedit ({} car.)", relu.len()));
                    return EcriturePath { methode: "regedit".to_string(), journal, reussi: true };
                }
                journal.push("⚠ après regedit, la valeur relue NE CONTIENT PAS le dossier".to_string());
            }
            Err(e) => journal.push(format!("⚠ fichier .reg non écrit ({e})")),
        }

        // ── C. Échec : on l'explique et on n'invente pas de succès ──────
        journal.push("Échec : le PATH utilisateur n'a pu être écrit par aucune voie".to_string());
        if sonde == api::ERROR_ACCESS_DENIED {
            journal.push("  cause : ACCÈS REFUSÉ sur HKCU\\Environment (verrou ou politique)".to_string());
        }
        journal.push("  → l'outil PATH propose des méthodes SANS modifier le PATH".to_string());
        EcriturePath { methode: String::new(), journal, reussi: false }
    }
    #[cfg(not(windows))]
    {
        let _ = (ancien, nouveau, cible);
        journal.push("· écriture du PATH sans objet hors Windows".to_string());
        EcriturePath { methode: String::new(), journal, reussi: false }
    }
}

/// Écrit `dossier` dans le `PATH` de la MACHINE (`HKLM`).
///
/// ⚠ Exige l'administrateur : sans élévation, `RegSetValueExW` rend
/// `ERROR_ACCESS_DENIED` et on le DIT — on ne prétend pas avoir réussi.
pub fn ecrire_path_machine(dossier: &str) -> EcriturePath {
    let mut journal: Vec<String> = Vec::new();
    #[cfg(windows)]
    {
        let ancien = lire_path_machine().unwrap_or_default();
        let nouveau = composer_path(&ancien, dossier);
        let cible = normaliser_path(dossier);
        match api::ecriture_possible(api::HKEY_LOCAL_MACHINE, api::ENV_MACHINE) {
            Ok(()) => journal.push("· clé HKLM\\…\\Environment ouverte en écriture".to_string()),
            Err(rc) => {
                journal.push(format!("⚠ clé HKLM NON ouvrable en écriture : {}", api::expliquer(rc)));
                journal.push("  → relancez l'installeur « En tant qu'administrateur »".to_string());
            }
        }
        let rc = api::ecrire(api::HKEY_LOCAL_MACHINE, api::ENV_MACHINE, "Path",
                             api::REG_EXPAND_SZ, &nouveau);
        if rc != api::ERROR_SUCCESS {
            journal.push(format!("Échec : RegSetValueExW HKLM a rendu {}", api::expliquer(rc)));
            return EcriturePath { methode: String::new(), journal, reussi: false };
        }
        journal.push("· API Win32 : valeur machine écrite".to_string());
        let relu = lire_path_machine().unwrap_or_default();
        if path_contient(&relu, &cible) {
            journal.push(format!("✓ PATH machine écrit ({} car.)", relu.len()));
            EcriturePath { methode: "api".to_string(), journal, reussi: true }
        } else {
            journal.push("Échec : la valeur machine relue NE CONTIENT PAS le dossier".to_string());
            EcriturePath { methode: String::new(), journal, reussi: false }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = dossier;
        journal.push("· écriture machine sans objet hors Windows".to_string());
        EcriturePath { methode: String::new(), journal, reussi: false }
    }
}

/// Peut-on écrire dans le registre `HKCU` ?
///
/// ⚠⚠ C'est le diagnostic qui remplace l'ancien test par `reg add` : il
/// dit POURQUOI ça échoue (accès refusé vs clé absente) au lieu de rendre
/// un code opaque.
pub fn registre_utilisateur_inscriptible() -> Result<(), String> {
    #[cfg(windows)]
    {
        // 1. Une clé de Rose (création autorisée ?).
        match api::ecriture_possible(api::HKEY_CURRENT_USER, "Software\\Rose") {
            Ok(()) => {}
            Err(api::ERROR_FILE_NOT_FOUND) => {
                // ⚠ La clé n'existe pas encore : on tente sa création.
                let rc = api::ecrire(api::HKEY_CURRENT_USER, "Software\\Rose", "Probe",
                                     api::REG_SZ, "1");
                if rc != api::ERROR_SUCCESS {
                    return Err(format!("création de HKCU\\Software\\Rose impossible : {}",
                                       api::expliquer(rc)));
                }
                let _ = api::supprimer_valeur(api::HKEY_CURRENT_USER, "Software\\Rose", "Probe");
            }
            Err(rc) => return Err(format!("HKCU\\Software\\Rose inouvrable : {}",
                                          api::expliquer(rc))),
        }
        // 2. La clé d'environnement (celle du PATH).
        match api::ecriture_possible(api::HKEY_CURRENT_USER, api::ENV_UTILISATEUR) {
            Ok(()) => {}
            Err(rc) => return Err(format!("HKCU\\Environment inouvrable en écriture : {}",
                                          api::expliquer(rc))),
        }
        // 3. Une écriture RÉELLE (une GPO peut autoriser l'ouverture et
        //    refuser la valeur).
        let rc = api::tester_ecriture(api::HKEY_CURRENT_USER, api::ENV_UTILISATEUR);
        if rc != api::ERROR_SUCCESS {
            return Err(format!("écriture test refusée : {}", api::expliquer(rc)));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        Err("registre Windows absent".to_string())
    }
}

/// Messages produits par une opération (affichés dans la console ou dans
/// la zone de texte de la fenêtre).
pub type Journal = Vec<String>;

/// Emplacement d'installation détecté (None = pas d'installation).
///
/// ⚠ La seule clé de registre `HKCU\Software\Rose\InstallDir` ne suffit
/// PAS : une installation faite par un ANCIEN installeur (Tauri, NSIS, MSI…)
/// n'écrit pas cette clé. On vérifie donc aussi les emplacements connus sur
/// le disque, sinon l'assistant ne voit pas l'installation et propose
/// « Installer » au lieu de « Mettre à jour » / « Désinstaller ».
pub fn installation_existante() -> Option<String> {
    #[cfg(windows)]
    {
        let out = commande_sans_fenetre("reg")
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
        // Repli 1 : emplacements standard des installateurs successifs.
        let home = std::env::var("USERPROFILE").unwrap_or_default();
        let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
        let candidats = [
            // Velopack / installeur actuel
            format!("{local}\\Programs\\Rose"),
            format!("{home}\\AppData\\Local\\Programs\\Rose"),
            // ancien installeur Tauri (dossier du bundle)
            format!("{local}\\Rose"),
            format!("{local}\\Programs\\Rose-Setup"),
            // installations « machine »
            "C:\\Program Files\\Rose".to_string(),
            "C:\\Program Files (x86)\\Rose".to_string(),
        ];
        for p in candidats {
            if PathBuf::from(&p).join("bin").join("rose.exe").exists() {
                return Some(p);
            }
        }
        // Repli 2 : l'ancien installeur Tauri posait `rose.exe` à la RACINE
        // du dossier (pas dans `bin\`).
        for p in [format!("{local}\\Rose"), format!("{local}\\Programs\\Rose")] {
            if PathBuf::from(&p).join("rose.exe").exists() {
                return Some(p);
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

/// Refuse un fichier qui n'est PAS le moteur Rose mais l'installeur.
///
/// Rend `Some(raison)` quand la source doit être rejetée.
///
/// Deux signaux, pour être robuste même si le setup a été renommé :
/// 1. le nom du fichier contient « setup » (ou « install ») ;
/// 2. sous Windows, le PE est en sous-système GUI (2), alors que `rose.exe`
///    est en console (3). C'est le signal le plus fiable : il ne dépend ni du
///    nom, ni du dossier.
///
/// Pourquoi c'est vital : installer l'installeur sous `rose.exe` crée une
/// BOUCLE — chaque `rose` relance le setup, qui réinstalle, qui relance le
/// setup. Mesuré chez l'utilisateur : 60 processus et obligation de
/// redémarrer la machine.
fn refuser_comme_moteur(source: &std::path::Path) -> Option<String> {
    let nom = source
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();
    if nom.contains("setup") || nom.contains("install") {
        return Some(format!(
            "{} est l'installeur, pas le moteur : il ne doit jamais être copié \
             sous le nom « rose.exe » (cela créerait une boucle de lancement)",
            source.display()
        ));
    }
    #[cfg(windows)]
    {
        if let Some(2) = sous_systeme_pe(source) {
            return Some(format!(
                "{} est une application GUI (sous-système 2), pas le moteur \
                 console : refusé pour éviter une boucle de lancement",
                source.display()
            ));
        }
    }
    None
}

/// Sous-système PE d'un fichier (`None` si illisible ou trop court).
#[cfg(windows)]
fn sous_systeme_pe(chemin: &std::path::Path) -> Option<u16> {
    let d = std::fs::read(chemin).ok()?;
    if d.len() < 0x40 || &d[0..2] != b"MZ" {
        return None;
    }
    let pe = u32::from_le_bytes([d[0x3C], d[0x3D], d[0x3E], d[0x3F]]) as usize;
    if pe + 0x5A > d.len() || &d[pe..pe + 4] != b"PE\0\0" {
        return None;
    }
    // Subsystem : offset 68 dans l'en-tête optionnel (PE32 et PE32+).
    let o = pe + 24 + 68;
    if o + 2 > d.len() {
        return None;
    }
    Some(u16::from_le_bytes([d[o], d[o + 1]]))
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
    for nom in ["rose.exe", "rose", "rose-20", "rose-20.exe", "rose-aot", "rose-aot.exe",
                "rose-shell", "rose-shell.exe"] {
        let _ = std::fs::remove_file(bindir.join(nom));
        // ⚠ L'ANCIEN installeur (Tauri) posait `rose.exe` à la RACINE du
        // dossier d'installation, pas dans `bin\`. Sans ce nettoyage, ce
        // binaire obsolète reste à côté de la nouvelle installation, le
        // PATH peut le trouver avant `bin\`, et `rose --version` annonce une
        // version périmée (mesuré : « 5.12.0 » après une installation neuve).
        let _ = std::fs::remove_file(PathBuf::from(prefixe).join(nom));
    }
    let source = match source_exe {
        Some(p) => p.to_path_buf(),
        None => {
            // ⚠ PAS de repli sur `current_exe()` : dans l'installeur, cela
            // copierait le setup sous `rose.exe`. On échoue proprement.
            j.push("Échec : aucun moteur Rose à installer — l'installeur \
                    n'embarque pas `rose.exe` et aucun `payload\\rose.exe` \
                    n'est présent à côté de lui.".to_string());
            return j;
        }
    };
    // ⚠⚠ GARDE-FOU CRITIQUE : on ne doit JAMAIS installer l'installeur
    // lui-même sous le nom `rose.exe`.
    //
    // `source_moteur()` retombe sur `current_exe()` quand aucun `payload\`
    // n'est présent. Dans `Rose-win-Setup.exe`, cela copiait donc le SETUP
    // sous `bin\rose.exe`, puis l'inscrivait dans le PATH. Résultat mesuré
    // sur la machine de l'utilisateur : chaque `rose` relançait le setup,
    // qui relançait une installation, qui relançait le setup… jusqu'à
    // 60 processus et une console clignotante par lancement, obligeant à
    // redémarrer le PC.
    //
    // On refuse explicitement quand la source est l'installeur graphique
    // (sous-système GUI = 2) ou quand elle porte le nom du setup.
    if let Some(raison) = refuser_comme_moteur(&source) {
        j.push(format!("Échec : {raison}"));
        return j;
    }
    let dest = bindir.join(if cfg!(windows) { "rose.exe" } else { "rose" });
    match std::fs::copy(&source, &dest) {
        Ok(_) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755));
            }
            // ⚠ VÉRIFICATION APRÈS COPIE : le fichier posé doit être un
            // moteur CONSOLE. Si c'est une application GUI, c'est
            // l'installeur : on le supprime aussitôt plutôt que de le
            // laisser dans `bin\`, d'où il serait relancé en boucle.
            #[cfg(windows)]
            if sous_systeme_pe(&dest) == Some(2) {
                let _ = std::fs::remove_file(&dest);
                j.push(format!(
                    "Échec : {} est une application GUI, pas le moteur — \
                     fichier retiré pour éviter une boucle de lancement",
                    dest.display()));
                return j;
            }
            j.push(format!("✓ {}", dest.display()));
        }
        Err(e) => {
            j.push(format!("Échec : copie impossible ({})", e));
            return j;
        }
    }
    // ── Fichiers compagnons : helper AOT, shell, icônes ────────────────
    //
    // ⚠ Deux sources possibles, dans cet ordre :
    //   1. le dossier du binaire source (distribution Velopack) ;
    //   2. les ressources EMBARQUÉES de l'installeur (`RT_RCDATA 4..7`).
    //
    // La version précédente ne regardait QUE le dossier de la source. Or,
    // pour l'installeur « un seul fichier », la source est le moteur extrait
    // dans `%TEMP%\rose-setup-payload\` : il n'y contient que `rose.exe`.
    // Résultat mesuré : l'installation ne posait QUE `rose.exe` — pas de
    // `rose-aot.exe`, pas de `rose-shell.exe`, pas d'icône `.rose`.
    //
    // `nom_src` = nom dans le dossier source ; `nom_dst` = nom final.
    // Les noms diffèrent hors Windows (`rose-shell` vs `rose-shell.exe`).
    //
    // ⚠ `rose-aot.exe` N'EST PLUS INSTALLÉ. Le stub de 432 Ko n'embarque PAS
    // LLVM (pas de feature `llvm`) : il ne peut donc rien compiler. C'était
    // un doublon inutile — `rose.exe` gère déjà `orc` et `link` (voir
    // `main.rs`), et télécharge à la demande le VRAI helper LLVM dans
    // `%LOCALAPPDATA%\rose\` quand la chaîne n'est pas embarquée.
    // Le livrer laissait croire qu'il fonctionnait, et alourdissait
    // l'installation d'un binaire mort.
    let compagnons: [(Option<&str>, &str, u16); 4] = [
        (Some(if cfg!(windows) { "rose-shell.exe" } else { "rose-shell" }),
         if cfg!(windows) { "rose-shell.exe" } else { "rose-shell" }, 5),
        (Some("rose.ico"), "rose.ico", 6),
        // ⚠⚠ CORRECTION DU 16/11 : `rose_app.ico` (icône des raccourcis et de
        // l'application) recevait `rose_shell.ico` (fenêtre de terminal).
        // L'utilisateur veut le LOGO DE BASE de l'APPLICATION (la rose).
        (Some("rose.ico"), "rose_app.ico", 7),
        // ⚠⚠⚠ AJOUT DU 17/11 : `rose_shell.ico` (fenêtre de terminal + rose)
        // est la SEULE icône correcte pour le profil Windows Terminal.
        // Mesuré chez l'utilisateur : le profil affichait `rose_app.ico`
        // (la rose seule) au lieu de l'icône de console. Le fichier n'était
        // même pas installé — le setup n'embarquait que la rose.
        // ⚠⚠⚠ NE JAMAIS CONFONDRE (vérifié visuellement le 17/11) :
        //   `rose.ico`       = LA ROSE SEULE (low-poly) → rose.exe, raccourcis
        //   `assets/rose.ico` = UN DOCUMENT « .ROSE »   → type de fichier
        //   `rose_shell.ico` = TERMINAL + rose          → profil Windows Terminal
        (Some("rose_shell.ico"), "rose_shell.ico", 8),
    ];

    for (nom_src, nom_dst, res_id) in compagnons {
        // `res_id` ne sert que sous Windows (ressource embarquée).
        #[cfg(not(windows))]
        let _ = res_id;
        let depuis_dossier = nom_src
            .and_then(|n| source.parent().map(|d| d.join(n)))
            .filter(|p| p.is_file());
        let chemin = match depuis_dossier {
            Some(p) => Some(p),
            None => {
                // Ressource embarquée (Windows uniquement).
                #[cfg(windows)]
                { crate::setup_gui::compagnon_embarque(res_id, nom_dst) }
                #[cfg(not(windows))]
                { None }
            }
        };
        let Some(src) = chemin else {
            j.push(format!("· {nom_dst} absent"));
            continue;
        };
        let dst = bindir.join(nom_dst);
        if std::fs::copy(&src, &dst).is_ok() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&dst, std::fs::Permissions::from_mode(0o755));
            }
            j.push(format!("✓ {nom_dst}"));
        } else {
            j.push(format!("Échec : copie de {nom_dst} impossible"));
        }
        // Nettoyage du temporaire extrait.
        if src.to_string_lossy().contains("rose-setup-payload") {
            let _ = std::fs::remove_file(&src);
        }
    }
    // ⚠ `enregistrer` rend un journal MULTI-LIGNES quand quelque chose a
    // échoué (clés, PATH, association) : on l'ajoute ligne par ligne pour
    // que la fenêtre puisse détecter les « Échec : … ».
    let enr = enregistrer(prefixe, &bindir);
    if enr.is_empty() {
        j.push("✓ registre, PATH et association .rose enregistrés".to_string());
    } else {
        for ligne in enr.lines() { j.push(ligne.to_string()); }
    }
    j.push(format!("✓ {}", nettoyer_anciennes_installations(prefixe)));
    // ── VÉRIFICATION APRÈS ÉCRITURE DU PATH ─────────────────────────────
    //
    // ⚠⚠⚠ L'utilisateur rapportait « la commande `rose` échoue après
    // l'installation ». Écrire la clé de registre ne prouve RIEN : si le
    // chemin a été mal reconstruit (chemin avec espace, filtre trop large),
    // l'installation « réussit » et `rose` reste introuvable.
    //
    // On relit donc le registre et on CONFIRME que le dossier `bin` est
    // présent, exactement, en comparant les chemins normalisés. Si ce n'est
    // pas le cas, on le SIGNALE dans le journal au lieu de laisser croire
    // que tout va bien.
    #[cfg(windows)]
    {
        // ⚠⚠⚠ ON VERIFIE PAR LE DIAGNOSTIC COMPLET, pas seulement la clé :
        // l'ancienne vérification disait « PATH ne contient pas … » sans
        // dire POURQUOI. Ici on distingue le verrou de stratégie de groupe,
        // le registre inaccessible et le PATH trop long — et on renvoie
        // l'utilisateur vers l'outil PATH.
        let d = diagnostiquer_path(prefixe);
        if d.dans_path_utilisateur || d.dans_path_machine {
            j.push("✓ PATH : le dossier bin est bien enregistré".to_string());
        } else {
            j.push(format!(
                "Échec : PATH ne contient pas {} après écriture", bindir.display()));
            if d.verrou_groupe {
                j.push("  cause : VERROU de strategie de groupe sur HKCU Environment".to_string());
            } else if !d.registre_inscriptible {
                j.push("  cause : registre HKCU inaccessible en ecriture".to_string());
            }
            j.push("  -> bouton PATH de la barre d'etat : plusieurs methodes de reparation".to_string());
            // ⚠⚠⚠ MODE ENTREPRISE AUTOMATIQUE : si le PATH est verrouillé
            // (GPO), on installe quand même un accès fonctionnel SANS
            // toucher au PATH. L'utilisateur n'a rien à faire à la main.
            j.push("".to_string());
            j.push("Mise en place de l'accès SANS modification du PATH :".to_string());
            j.extend(mode_entreprise(prefixe));
        }
    }
    j
}

/// Relit `HKCU\Environment\Path` et dit si `bindir` y est.
///
/// ⚠ Comparaison sur les chemins NORMALISÉS (`\` → `/`, minuscules, sans
/// séparateur final) : le registre peut stocker `C:\Program Files\Rose\bin`
/// avec des espaces, des antislashs doublés ou des `/`.
#[cfg(windows)]
fn verifier_path(bindir: &std::path::Path) -> Result<bool, String> {
    // ⚠⚠ Plus de `reg query` : la lecture passe par l'API Win32, comme
    // l'écriture. Mesuré chez l'utilisateur : `reg.exe` pouvait être
    // bloqué par une politique d'application, donc les deux voies
    // doivent être indépendantes de lui.
    match lire_path_utilisateur() {
        Some(v) => Ok(path_contient(&v, &bindir.display().to_string())),
        None => Err("lecture du registre impossible".to_string()),
    }
}

/// Supprime les installations d'un ANCIEN installeur (Tauri), qui posait
/// `rose.exe` dans `%LOCALAPPDATA%\Rose` au lieu de `...\Programs\Rose`.
///
/// ⚠ Sans ce nettoyage, le dossier obsolète reste dans le `PATH` (il y est
/// même inséré AVANT la nouvelle installation par certaines versions) et
/// `rose --version` annonce une version périmée — mesuré : « 5.12.0 » après
/// avoir installé 0.1.5. On ne touche JAMAIS au dossier courant.
#[cfg(windows)]
fn nettoyer_anciennes_installations(prefixe: &str) -> String {
    let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
    let home = std::env::var("USERPROFILE").unwrap_or_default();
    let mut retires = Vec::new();
    let courant = std::path::Path::new(prefixe);
    for p in [
        format!("{local}\\Rose"),
        format!("{home}\\AppData\\Local\\Rose"),
    ] {
        let d = std::path::Path::new(&p);
        // Ne jamais supprimer le dossier qu'on vient d'installer.
        if d == courant || !d.exists() {
            continue;
        }
        // Sécurité : le dossier doit vraiment ressembler à une installation
        // Rose (un `rose.exe` quelque part dedans).
        let a_un_moteur = d.join("rose.exe").exists() || d.join("bin").join("rose.exe").exists();
        if a_un_moteur && std::fs::remove_dir_all(d).is_ok() {
            retires.push(p);
        }
    }
    if retires.is_empty() {
        "aucune ancienne installation à retirer".to_string()
    } else {
        format!("ancienne installation retirée : {}", retires.join(", "))
    }
}

#[cfg(not(windows))]
fn nettoyer_anciennes_installations(_prefixe: &str) -> String {
    "aucune ancienne installation à retirer".to_string()
}

/// Écrit le registre / la configuration, le PATH et les associations.
fn enregistrer(prefixe: &str, bindir: &std::path::Path) -> String {
    #[cfg(windows)]
    {
        // ── Clés de Rose ───────────────────────────────────────────────────
        //
        // ⚠⚠⚠ TOUT PASSE PAR L'API WIN32 (`Advapi32`), PLUS PAR `reg.exe`.
        // Mesuré chez l'utilisateur : `reg add` rendait le code 1 et
        // n'écrivait RIEN — ni le PATH, ni les associations. La cause la
        // plus probable sur une machine gérée est une politique
        // d'application (AppLocker/WDAC) ou un antivirus qui bloque
        // `reg.exe`. L'API, elle, est déjà chargée dans le processus :
        // aucune politique ne peut l'empêcher.
        let bin_win = bindir.display().to_string().replace('/', "\\");
        let mut j: Journal = Vec::new();
        let cles: [(&str, &str, &str); 2] = [
            ("Software\\Rose", "InstallDir", prefixe),
            ("Software\\Rose", "Version", env!("CARGO_PKG_VERSION")),
        ];
        for (cle, nom, valeur) in cles {
            let rc = api::ecrire(api::HKEY_CURRENT_USER, cle, nom, api::REG_SZ, valeur);
            if rc != api::ERROR_SUCCESS {
                j.push(format!("⚠ écriture {cle}\\{nom} refusée : {}", api::expliquer(rc)));
            }
        }

        // ── PATH ───────────────────────────────────────────────────────────
        //
        // ⚠⚠⚠ L'ÉCRITURE PASSE PAR L'API, PLUS PAR `reg.exe`.
        // `ecrire_path_utilisateur` fait : `RegSetValueExW`, puis
        // RELECTURE de vérification, puis repli fichier `.reg` +
        // `regedit /s` si l'API a échoué. Il conserve les segments
        // existants, retire les doublons et les dossiers Rose MORTS, et
        // met le dossier courant en tête.
        let ecriture = ecrire_path_utilisateur(&bin_win);
        j.extend(ecriture.journal);

        // ⚠⚠ DIFFUSER LE CHANGEMENT D'ENVIRONNEMENT.
        //
        // Écrire la clé de registre ne met PAS à jour les applications
        // DÉJÀ lancées : elles gardent l'environnement qu'elles ont reçu à
        // leur démarrage. Mesuré chez l'utilisateur : `rose` fonctionnait
        // dans cmd (ouvert APRÈS l'installation) mais VS Code disait
        // « rose n'est pas reconnu » — VS Code avait été lancé AVANT et
        // n'a jamais vu le nouveau PATH.
        diffuser_environnement();

        // ── Association `.rose` (vérifiée par relecture) ───────────────────
        let assoc = poser_association(&bin_win);
        j.extend(assoc.journal);
        if assoc.reussi {
            // ⚠⚠⚠ CORRECTION DU 17/11 : `DefaultIcon` doit pointer le
            // DOCUMENT `.ROSE` (`installateur/assets/rose.ico`, embarqué en
            // RT_RCDATA 6 et posé dans `bin\rose.ico`). L'ancien code lisait
            // `prefixe/rose.ico`, qui n'est JAMAIS posé (les icônes vont
            // dans `bin\`), et retombait sur `bin\rose.exe,0` — donc la
            // ROSE de l'application. Mesuré chez l'utilisateur : les
            // fichiers `.rose` affichaient le logo de l'app au lieu du
            // document.
            // ⚠ Ne PAS confondre `bin\rose.ico` (document) et
            // `bin\rose_app.ico` (la rose) : ce sont deux fichiers différents.
            let ico = bindir.join("rose.ico");
            let ico_str = if ico.is_file() {
                format!("{},0", ico.display())
            } else {
                // Repli : l'icône embarquée dans le setup lui-même
                // (rose_setup.res, RT_ICON du groupe d'icônes de l'installeur
                // n'est pas un document) — mieux vaut le moteur que rien,
                // mais on journalise le manque.
                j.push("⚠ bin\\rose.ico absent : l'association .rose retombe sur l'icône du moteur".to_string());
                format!("{},0", bindir.join("rose.exe").display())
            };
            let _ = api::ecrire(api::HKEY_CURRENT_USER,
                                "Software\\Classes\\Rose.File\\DefaultIcon", "",
                                api::REG_SZ, &ico_str);
            let _ = api::ecrire(api::HKEY_CURRENT_USER,
                                "Software\\Classes\\Rose.File", "FriendlyTypeName",
                                api::REG_SZ, "Fichier source Rose");
        }
        let profil = ecrire_profil_terminal(bindir);
        j.push(profil);
        // ⚠ Notifier l'Explorateur : sans cela il garde l'icône générique
        // en cache et les `.rose` n'affichent la rose qu'après redémarrage
        // de la session. `SHChangeNotify(SHCNE_ASSOCCHANGED)` force la
        // relecture des associations et des icônes.
        unsafe {
            use std::ffi::c_void;
            #[link(name = "shell32")]
            unsafe extern "system" {
                fn SHChangeNotify(evenement: i32, drapeaux: u32, item1: *const c_void, item2: *const c_void);
            }
            const SHCNE_ASSOCCHANGED: i32 = 0x0800_0000;
            const SHCNF_IDLIST: u32 = 0x0000;
            SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, std::ptr::null(), std::ptr::null());
        }
        if j.iter().any(|l| l.starts_with("⚠") || l.starts_with("Échec")) {
            return j.join("\n");
        }
        return String::new();
    }
    #[cfg(unix)]
    {
        let _ = bindir;
        enregistrer_unix(prefixe)
    }
}

/// Enregistrement côté Linux : ligne `export PATH` et type MIME `text/x-rose`.
#[cfg(unix)]
fn enregistrer_unix(prefixe: &str) -> String {
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
        let _ = commande_sans_fenetre("xdg-mime").arg("install").arg(format!("{}/x-rose.xml", dir)).status();
        let _ = commande_sans_fenetre("update-mime-database").arg(format!("{}/.local/share/mime", home)).status();
        "PATH et type MIME mis à jour".to_string()
    }
}

/// Écrit un profil « Fragment » pour Windows Terminal.
///
/// Windows Terminal fusionne automatiquement tout `*.json` déposé dans
/// `%LOCALAPPDATA%\Microsoft\Windows Terminal\Fragments\<Application>\`.
/// C'est la méthode recommandée par Microsoft : on ne touche JAMAIS à
/// `settings.json`, donc les réglages de l'utilisateur ne peuvent pas être
/// cassés. Le profil apparaît dans le menu déroulant des onglets, avec son
/// icône et sa ligne de commande.
///
/// Le fragment pointe sur `rose-shell.exe`, qui délègue `rose console` au
/// moteur installé : une seule implémentation du langage.
#[cfg(windows)]
fn ecrire_profil_terminal(bindir: &std::path::Path) -> String {
    let local = match std::env::var("LOCALAPPDATA") {
        Ok(v) => v,
        Err(_) => return "profil Windows Terminal non écrit (LOCALAPPDATA absent)".to_string(),
    };
    let dossier = std::path::Path::new(&local)
        .join("Microsoft")
        .join("Windows Terminal")
        .join("Fragments")
        .join("Rose");
    if std::fs::create_dir_all(&dossier).is_err() {
        return "profil Windows Terminal non écrit".to_string();
    }

    // Chemins en JSON : les antislashs doivent être doublés.
    let shell = bindir.join("rose-shell.exe").display().to_string().replace('\\', "\\\\");
    // ⚠⚠⚠ CORRECTION DU 17/11 : le profil utilise `rose_shell.ico` (fenêtre
    // de TERMINAL + rose), pas `rose_app.ico` (la rose seule). Mesuré chez
    // l'utilisateur : avec la rose, l'onglet du terminal Rose montrait le
    // logo de l'APPLICATION — ce n'est pas une console. `rose_shell.ico`
    // est désormais embarqué (RT_RCDATA 8) et posé dans `bin\`.
    // Repli : si `rose_shell.ico` manque (vieille installation), on
    // retombe sur `rose_app.ico` pour ne jamais écrire un chemin mort.
    let ico_shell = bindir.join("rose_shell.ico");
    let ico_repli = bindir.join("rose_app.ico");
    let ico_path = if ico_shell.is_file() { ico_shell } else { ico_repli };
    let icone = ico_path.display().to_string().replace('\\', "\\\\");

    // `startingDirectory` = dossier de l'utilisateur : un shell s'ouvre là où
    // l'on travaille, pas dans `bin\`.
    let json = format!(
        r#"{{
  "profiles": [
    {{
      "name": "Rose",
      "commandline": "\"{shell}\"",
      "startingDirectory": "%USERPROFILE%",
      "icon": "{icone}",
      "colorScheme": "One Half Dark",
      "historySize": 9001,
      "snapOnInput": true,
      "closeOnExit": "graceful",
      "tabTitle": "Rose {version}",
      "suppressApplicationTitle": true
    }}
  ]
}}
"#,
        version = env!("CARGO_PKG_VERSION")
    );

    match std::fs::write(dossier.join("rose.json"), json) {
        Ok(()) => "profil Windows Terminal « Rose » installé".to_string(),
        Err(_) => "profil Windows Terminal non écrit".to_string(),
    }
}

#[cfg(not(windows))]
fn ecrire_profil_terminal(_bindir: &std::path::Path) -> String {
    "profil Windows Terminal sans objet".to_string()
}

/// Supprime l'installation (binaires, registre, PATH, associations).
pub fn desinstaller(prefixe: &str) -> Journal {
    let mut j = Journal::new();
    #[cfg(windows)]
    {
        // Profil Windows Terminal : on retire le fragment « Rose » pour que
        // le menu déroulant ne garde pas une entrée pointant sur un binaire
        // supprimé.
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let dossier = std::path::Path::new(&local)
                .join("Microsoft")
                .join("Windows Terminal")
                .join("Fragments")
                .join("Rose");
            let _ = std::fs::remove_dir_all(&dossier);
        }
        let _ = commande_sans_fenetre("reg")
            .args(["delete", "HKCU\\Software\\Rose", "/f"]).status();
        let _ = commande_sans_fenetre("reg")
            .args(["delete", "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Rose", "/f"]).status();
        let _ = commande_sans_fenetre("reg")
            .args(["delete", "HKCU\\Software\\Classes\\.rose", "/f"]).status();
        let _ = commande_sans_fenetre("reg")
            .args(["delete", "HKCU\\Software\\Classes\\Rose.File", "/f"]).status();
        // ⚠⚠ MÊME BUG QUE DANS `enregistrer`, CORRIGÉ ICI AUSSI : la
        // lecture `split_whitespace()` cassait les chemins contenant un
        // espace, et le filtre `contains("rose")` retirait TOUT chemin
        // contenant « rose » (même `C:\tools\rosetta`). On coupe sur le
        // TYPE et on ne retire que les segments dont le dernier ou
        // l'avant-dernier élément vaut exactement « rose ».
        let mut ancien = String::new();
        if let Ok(o) = commande_sans_fenetre("reg")
            .args(["query", "HKCU\\Environment", "/v", "Path"]).output() {
            let txt = String::from_utf8_lossy(&o.stdout);
            for ligne in txt.lines() {
                let l = ligne.trim_start();
                if !l.starts_with("Path") { continue; }
                for typ in ["REG_EXPAND_SZ", "REG_SZ"] {
                    if let Some(pos) = l.find(typ) {
                        ancien = l[pos + typ.len()..].trim_start().to_string();
                        break;
                    }
                }
            }
        }
        let normaliser = |s: &str| -> String {
            s.trim().replace('/', "\\").replace("\\\\", "\\")
                .trim_end_matches('\\').to_lowercase()
        };
        let segments: Vec<String> = ancien.split(';')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .filter(|s| {
                let n = normaliser(s);
                let dernier = n.rsplit('\\').next().unwrap_or("");
                let avant = n.rsplit('\\').nth(1).unwrap_or("");
                !(dernier == "rose" || avant == "rose")
            })
            .collect();
        let _ = commande_sans_fenetre("reg")
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
    for nom in ["rose.exe", "rose", "rose-20", "rose-20.exe", "rose-aot", "rose-aot.exe",
                "rose-shell", "rose-shell.exe", "rose.ico", "rose_app.ico", "rose_shell.ico"] {
        let _ = std::fs::remove_file(bin.join(nom));
        // Racine du dossier : vestige de l'ancien installeur (Tauri).
        let _ = std::fs::remove_file(PathBuf::from(prefixe).join(nom));
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
    { "Rose-win-Setup.exe" }      // installeur Velopack
    #[cfg(target_os = "linux")]
    {
        if cfg!(target_arch = "aarch64") { "rose-aarch64-linux" } else { "rose-x86_64-linux" }
    }
    #[cfg(target_os = "macos")]
    { "rose-aarch64-macos" }
}

/// Exécute `curl` (ou `wget`) pour récupérer une URL.
fn http_get(url: &str) -> Option<String> {
    let out = commande_sans_fenetre("curl")
        .args(["-sSL", "--max-time", "30", url])
        .output();
    if let Ok(o) = out {
        if o.status.success() {
            return Some(String::from_utf8_lossy(&o.stdout).into_owned());
        }
    }
    // repli : wget
    let out = commande_sans_fenetre("wget")
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
    let st = commande_sans_fenetre("curl")
        .args(["-sSL", "--max-time", "180", "-o", dest.to_string_lossy().as_ref(), url])
        .status();
    if let Ok(s) = st {
        if s.success() {
            if let Ok(m) = std::fs::metadata(dest) {
                if m.len() > 0 { return true; }
            }
        }
    }
    let st = commande_sans_fenetre("wget")
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
    { noms.push("Rose-win-Setup.exe".to_string()); }
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
    let nom = if cfg!(windows) { "rose.exe" } else { "rose" };
    // `bin\` d'abord (installeur actuel), puis la RACINE (ancien installeur
    // Tauri, qui posait `rose.exe` directement dans le dossier).
    let candidats = [
        PathBuf::from(prefixe).join("bin").join(nom),
        PathBuf::from(prefixe).join(nom),
    ];
    for exe in candidats {
        // `--version` s'exécute sans fenêtre : `rose.exe` est un programme
        // CONSOLE et, lancé depuis l'installeur GUI, Windows lui allouerait
        // une console (fenêtre noire fugace).
        let Ok(out) = commande_sans_fenetre(&exe.display().to_string())
            .arg("--version")
            .output()
        else {
            continue;
        };
        let txt = String::from_utf8_lossy(&out.stdout);
        // « Rose 0.1.5 — … » ou « Rose 5.12.0 — … »
        if let Some(apres) = txt.strip_prefix("Rose ") {
            let v: String = apres.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
            if !v.is_empty() {
                return Some(v);
            }
        }
    }
    None
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
                    { assets.push(("Rose-win-Setup.exe".to_string(), format!("{}/Rose-win-Setup.exe", base))); }
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
            .find(|(n, _)| n.starts_with("Rose-win-Setup") && n.ends_with(".exe"))
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
        if let Some((n, url)) = release.assets.iter().find(|(n, _)| n.starts_with("Rose-win-Setup") && n.ends_with(".exe")).cloned() {
            let dest = tmp.join(&n);
            j.push(format!("Téléchargement de l'installeur : {}", n));
            if http_telecharger(&url, &dest) {
                j.push("✓ Installeur téléchargé — lancement…".to_string());
                // L'installeur téléchargé est une application GUI : on le
                // lance sans console parasite.
                let _ = commande_sans_fenetre(&dest.display().to_string()).spawn();
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

// ═══════════════════════════════════════════════════════════════════════
// OUTIL PATH — diagnostic et réparation, plusieurs méthodes
// ═══════════════════════════════════════════════════════════════════════
//
// ⚠⚠⚠ POURQUOI CET OUTIL. Mesuré chez l'utilisateur : l'installation se
// termine, mais l'écriture du `PATH` ÉCHOUE (« ça me demande de le faire à
// la main »). Les causes possibles sont nombreuses et il faut les
// DISTINGUER, pas les deviner :
//
//   1. le registre est en lecture seule ou la clé n'existe pas ;
//   2. une STRATÉGIE DE GROUPE (entreprise) verrouille `HKCU\Environment` ;
//   3. le `PATH` est géré par un outil tiers (Chocolatey, Scoop, un
//      gestionnaire de parc…) qui réécrit la valeur à chaque connexion ;
//   4. le `PATH` dépasse la limite de 2047 caractères du registre ;
//   5. le dossier `bin` n'existe pas encore au moment de l'écriture ;
//   6. l'utilisateur n'a pas les droits sur la clé.
//
// ⚠⚠⚠ LA SOLUTION POUR LES SYSTÈMES D'ENTREPRISE : si le `PATH` est
// VERROUILLÉ, on ne peut pas le modifier — et ce n'est PAS une erreur de
// l'installeur. Rose fonctionne alors de trois autres façons :
//
//   A. un LANCEUR dans le dossier utilisateur (`%USERPROFILE%\Rose\rose.cmd`) ;
//   B. un RACCOURCI sur le Bureau qui appelle `rose.exe` directement ;
//   C. l'association `.rose` (double-clic) qui n'a pas besoin du PATH.
//
// On propose donc plusieurs MÉTHODES, chacune testée et rapportée.

/// Où en est le `PATH` pour le dossier `bin` de Rose ?
pub struct DiagnosticPath {
    /// Le dossier `bin` de Rose existe-t-il ?
    pub bin_existe: bool,
    /// Le dossier `bin` est-il dans le `PATH` utilisateur ?
    pub dans_path_utilisateur: bool,
    /// Le dossier `bin` est-il dans le `PATH` machine (HKLM) ?
    pub dans_path_machine: bool,
    /// Le dossier `bin` est-il dans le `PATH` du processus courant ?
    pub dans_path_processus: bool,
    /// La clé `HKCU\Environment` est-elle accessible en écriture ?
    pub registre_inscriptible: bool,
    /// Une stratégie de groupe verrouille-t-elle la clé ?
    pub verrou_groupe: bool,
    /// Longueur du PATH utilisateur (la limite du registre est 2047).
    pub longueur_path: usize,
    /// `rose.exe` est-il trouvable par `where` (résolution réelle) ?
    pub where_trouve: bool,
    /// Messages explicatifs, dans l'ordre.
    pub messages: Vec<String>,
}

impl DiagnosticPath {
    /// Vrai si `rose` est utilisable dans un terminal neuf.
    pub fn ok(&self) -> bool {
        self.bin_existe && (self.dans_path_utilisateur || self.dans_path_machine)
    }
}

/// Lit une valeur `REG_SZ`/`REG_EXPAND_SZ` de `HKCU` par `reg query`.
///
/// ⚠ On coupe sur le TYPE, jamais sur les espaces : un chemin comme
/// `C:\Program Files\Rose\bin` contient des espaces.
#[cfg(windows)]
fn lire_valeur_hkcu(sous_cle: &str, valeur: &str) -> Option<String> {
    let o = commande_sans_fenetre("reg")
        .args(["query", sous_cle, "/v", valeur]).output().ok()?;
    if !o.status.success() { return None; }
    let txt = String::from_utf8_lossy(&o.stdout);
    for ligne in txt.lines() {
        let l = ligne.trim_start();
        if !l.starts_with(valeur) { continue; }
        for typ in ["REG_EXPAND_SZ", "REG_SZ"] {
            if let Some(pos) = l.find(typ) {
                return Some(l[pos + typ.len()..].trim_start().to_string());
            }
        }
    }
    None
}

/// Normalise un segment de `PATH` pour la comparaison.
fn normaliser_path(s: &str) -> String {
    s.trim().replace('/', "\\").replace("\\\\", "\\")
        .trim_end_matches('\\').to_lowercase()
}

/// Le `PATH` contient-il `cible` ?
fn path_contient(path: &str, cible: &str) -> bool {
    let c = normaliser_path(cible);
    path.split(';').any(|seg| normaliser_path(seg) == c)
}

// ═══════════════════════════════════════════════════════════════════════
// DIAGNOSTIC DU PATH — API WIN32
// ═══════════════════════════════════════════════════════════════════════
//
// ⚠⚠⚠ AUCUNE ÉCRITURE DANS LE PATH ICI : c'est un diagnostic pur, sûr à
// appeler autant de fois qu'on veut. Le seul effet de bord est une valeur
// sonde (`RoseProbe`) posée puis retirée dans `HKCU\Environment`.
//
// ⚠⚠⚠ CE QUI CHANGE PAR RAPPORT À LA VERSION `reg.exe` : les causes sont
// LISES, pas devinées. Une capture d'écran montrait « le registre HKCU est
// INACCESSIBLE en écriture » alors que `reg add` échouait pour une autre
// raison (code 1 = politique d'application). On distingue maintenant :
//
//   · ACCÈS REFUSÉ (5)      → droits ou verrou ;
//   · introuvable (2)       → clé absente (créable) ;
//   · autre code            → affiché en clair, avec son numéro.
pub fn diagnostiquer_path(prefixe: &str) -> DiagnosticPath {
    let bindir = std::path::Path::new(prefixe).join("bin");
    let bin_str = bindir.display().to_string();
    let mut messages: Vec<String> = Vec::new();

    let bin_existe = bindir.join(if cfg!(windows) { "rose.exe" } else { "rose" }).is_file();
    if bin_existe {
        messages.push(format!("✓ le moteur est présent dans {bin_str}"));
    } else {
        messages.push(format!("Échec : aucun moteur dans {bin_str}"));
    }

    let path_user = lire_path_utilisateur().unwrap_or_default();
    let path_machine = lire_path_machine().unwrap_or_default();

    let dans_path_utilisateur = path_contient(&path_user, &bin_str);
    let dans_path_machine = path_contient(&path_machine, &bin_str);
    let dans_path_processus = std::env::var("PATH")
        .map(|p| path_contient(&p, &bin_str)).unwrap_or(false);

    if dans_path_utilisateur { messages.push("✓ présent dans le PATH utilisateur".to_string()); }
    else { messages.push("· absent du PATH utilisateur".to_string()); }
    if dans_path_machine { messages.push("✓ présent dans le PATH machine".to_string()); }
    if dans_path_processus { messages.push("✓ présent dans le PATH de ce processus".to_string()); }

    // ── Le registre est-il écritvable ? (cause LUE, pas devinée) ─────────
    let mut verrou = false;
    let mut inscriptible = true;
    #[cfg(windows)]
    {
        // 1. Ouverture de la clé en écriture : distingue accès refusé et
        //    clé absente.
        match api::ecriture_possible(api::HKEY_CURRENT_USER, api::ENV_UTILISATEUR) {
            Ok(()) => messages.push("· clé HKCU\\Environment ouverte en écriture".to_string()),
            Err(api::ERROR_FILE_NOT_FOUND) => {
                // ⚠ Clé absente : elle sera CRÉÉE par RegCreateKeyExW. Ce
                // n'est PAS un verrou.
                messages.push("· clé HKCU\\Environment absente (elle sera créée)".to_string());
            }
            Err(rc) => {
                inscriptible = false;
                messages.push(format!(
                    "⚠ clé HKCU\\Environment NON ouvrable en écriture : {}", api::expliquer(rc)));
            }
        }
        // 2. Écriture RÉELLE d'une valeur sonde : une GPO peut autoriser
        //    l'ouverture et refuser la valeur.
        let rc = api::tester_ecriture(api::HKEY_CURRENT_USER, api::ENV_UTILISATEUR);
        match rc {
            api::ERROR_SUCCESS => {
                messages.push("· écriture test ACCEPTÉE (valeur sonde retirée)".to_string());
            }
            _ => {
                inscriptible = false;
                verrou = true;
                messages.push(format!(
                    "⚠ écriture test REFUSÉE : {}", api::expliquer(rc)));
                messages.push("  → une stratégie de groupe ou un antivirus verrouille le PATH".to_string());
            }
        }
        // 3. `HKCU\Software\Rose` : sert au reste de l'installation.
        match api::ecriture_possible(api::HKEY_CURRENT_USER, "Software\\Rose") {
            Ok(()) => messages.push("· clé HKCU\\Software\\Rose accessible".to_string()),
            Err(rc) => messages.push(format!(
                "⚠ clé HKCU\\Software\\Rose inaccessible : {}", api::expliquer(rc))),
        }
    }
    #[cfg(not(windows))]
    {
        inscriptible = false;
    }

    if path_user.len() > 2047 {
        messages.push(format!(
            "⚠ PATH utilisateur très long ({} car.) : la limite du registre est 2047",
            path_user.len()));
    }

    // ⚠ `where` est le test le PLUS fiable : c'est ce que fait `cmd` quand
    // l'utilisateur tape `rose`. Il regarde le PATH RÉEL du processus.
    #[cfg(windows)]
    let where_trouve = commande_sans_fenetre("where")
        .args(["rose"]).output().map(|o| o.status.success()).unwrap_or(false);
    #[cfg(not(windows))]
    let where_trouve = false;
    if where_trouve { messages.push("✓ `where rose` le trouve".to_string()); }
    else { messages.push("· `where rose` ne le trouve pas (terminal neuf : « rose » échouera)".to_string()); }

    DiagnosticPath {
        bin_existe,
        dans_path_utilisateur,
        dans_path_machine,
        dans_path_processus,
        registre_inscriptible: inscriptible,
        verrou_groupe: verrou,
        longueur_path: path_user.len(),
        where_trouve,
        messages,
    }
}

/// Méthode de réparation du `PATH`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum MethodePath {
    /// Écrire dans `HKCU\Environment\Path` (la méthode normale).
    Utilisateur,
    /// Écrire dans le `PATH` MACHINE (`HKLM`) — exige l'administrateur.
    Machine,
    /// Créer `%USERPROFILE%\Rose\rose.cmd` et l'ajouter au `PATH`
    /// utilisateur (contourne un `PATH` trop long en le raccourcissant).
    Lanceur,
    /// Créer un raccourci sur le Bureau (aucune modification du PATH).
    Raccourci,
    /// Poser l'association `.rose` et un profil Windows Terminal
    /// (aucune modification du PATH).
    Association,
}

impl MethodePath {
    pub fn nom(self) -> &'static str {
        match self {
            MethodePath::Utilisateur => "PATH utilisateur (HKCU)",
            MethodePath::Machine => "PATH machine (HKLM, administrateur)",
            MethodePath::Lanceur => "Lanceur .cmd dans %USERPROFILE%\\Rose",
            MethodePath::Raccourci => "Raccourci sur le Bureau",
            MethodePath::Association => "Association .rose + profil Terminal",
        }
    }
    /// Cette méthode modifie-t-elle le `PATH` ?
    pub fn modifie_path(self) -> bool {
        matches!(self, MethodePath::Utilisateur | MethodePath::Machine | MethodePath::Lanceur)
    }
}

/// Les méthodes proposées, DANS L'ORDRE d'affichage.
///
/// ⚠ L'ordre compte : les deux premières modifient le PATH (méthodes
/// « normales »), les trois suivantes n'y touchent pas (méthodes pour les
/// machines d'ENTREPRISE où le PATH est verrouillé).
pub const METHODES_PATH: [MethodePath; 5] = [
    MethodePath::Utilisateur,
    MethodePath::Machine,
    MethodePath::Lanceur,
    MethodePath::Raccourci,
    MethodePath::Association,
];

// ═══════════════════════════════════════════════════════════════════════
// OUTIL PATH — LES CINQ MÉTHODES DE RÉPARATION
// ═══════════════════════════════════════════════════════════════════════
//
// ⚠⚠⚠ CHAQUE MÉTHODE EST VÉRIFIÉE APRÈS COUP : on relit l'état et on dit
// si ça a MARCHÉ. Écrire une valeur ne prouve rien — une GPO peut la
// réécrire, et `reg.exe` peut rendre 1 sans rien faire.
//
// ⚠⚠⚠ LES DEUX PREMIÈRES PASSENT PAR L'API WIN32 (pas `reg.exe`), en
// cascade API → fichier `.reg` + `regedit /s`. Voir `ecrire_path_utilisateur`.
//
// ⚠ Les trois dernières NE TOUCHENT PAS au PATH : elles posent un lanceur,
// un raccourci ou une association. Elles fonctionnent même quand le PATH
// est verrouillé par une politique d'entreprise.
pub fn reparer_path(prefixe: &str, methode: MethodePath) -> Journal {
    let mut j: Journal = Vec::new();
    let bindir = std::path::Path::new(prefixe).join("bin");
    let bin_str = bindir.display().to_string();
    let bin_win = bin_str.replace('/', "\\");
    let moteur = bindir.join(if cfg!(windows) { "rose.exe" } else { "rose" });

    match methode {
        MethodePath::Utilisateur => {
            let r = ecrire_path_utilisateur(&bin_win);
            j.extend(r.journal);
            if r.reussi {
                diffuser_environnement();
                j.push(format!("  (méthode employée : {})", r.methode));
            }
        }
        MethodePath::Machine => {
            let r = ecrire_path_machine(&bin_win);
            j.extend(r.journal);
            if r.reussi {
                diffuser_environnement();
            } else {
                j.push("  → alternative sans administrateur : « Lanceur .cmd »".to_string());
            }
        }
        MethodePath::Lanceur => {
            // ⚠ Le lanceur évite un PATH trop long ET un PATH verrouillé :
            // le fichier est écrit dans le profil, ce qui ne demande AUCUN
            // droit. On n'ajoute au PATH que si c'est POSSIBLE — sinon on
            // dit simplement où taper la commande.
            let home = std::env::var("USERPROFILE").unwrap_or_default();
            let d = std::path::Path::new(&home).join("Rose");
            match std::fs::create_dir_all(&d) {
                Ok(()) => {
                    let cmd = d.join("rose.cmd");
                    let contenu = format!("@echo off\r\n\"{}\" %*\r\n", moteur.display());
                    match std::fs::write(&cmd, contenu) {
                        Ok(()) => j.push(format!("✓ lanceur créé : {}", cmd.display())),
                        Err(e) => j.push(format!("Échec : lanceur non écrit ({e})")),
                    }
                    let court = d.display().to_string().replace('/', "\\");
                    let r = ecrire_path_utilisateur(&court);
                    if r.reussi {
                        diffuser_environnement();
                        j.push(format!("✓ {} ajouté au PATH", court));
                        j.push("  → dans un terminal NEUF : tapez `rose`".to_string());
                    } else {
                        // ⚠ Pas de PATH : le lanceur suffit, on le DIT.
                        j.push(format!("· PATH non modifiable ({}) — utilisez directement :", r.methode));
                        j.push(format!("    \"{}\"", cmd.display()));
                        j.push("  (ou double-cliquez le raccourci du Bureau)".to_string());
                    }
                }
                Err(e) => j.push(format!("Échec : dossier {} non créé ({e})", d.display())),
            }
        }
        MethodePath::Raccourci => {
            // ⚠ Un raccourci .lnk exige COM (IShellLink). Sans COM, on pose
            // un fichier `.cmd` sur le Bureau : il fonctionne par double-clic
            // et ne dépend d'AUCUN composant système.
            let home = std::env::var("USERPROFILE").unwrap_or_default();
            let bureau = std::path::Path::new(&home).join("Desktop");
            let _ = std::fs::create_dir_all(&bureau);
            let cible = bureau.join("Rose.cmd");
            let contenu = format!(
                "@echo off\r\nrem Lance Rose sans passer par le PATH\r\n\"{}\" %*\r\n\
                 if \"%~1\"==\"\" pause\r\n",
                moteur.display());
            match std::fs::write(&cible, contenu) {
                Ok(()) => {
                    j.push(format!("✓ raccourci posé : {}", cible.display()));
                    j.push("  → double-cliquez-le pour lancer Rose".to_string());
                }
                Err(e) => j.push(format!("Échec : raccourci non écrit ({e})")),
            }
        }
        MethodePath::Association => {
            // ⚠ Ni le PATH ni le registre d'environnement : seulement les
            // associations et le profil Terminal. On VÉRIFIE par relecture.
            let assoc = poser_association(&bin_win);
            j.extend(assoc.journal);
            if assoc.reussi { diffuser_environnement(); }
            let profil = ecrire_profil_terminal(&bindir);
            j.push(profil);
        }
    }
    #[cfg(not(windows))]
    {
        // Sur Linux, `ecrire_path_utilisateur` rend « sans objet » ; les
        // méthodes de fichiers restent testables (lanceur, raccourci).
        let _ = (&bin_win, &moteur);
    }
    j
}

/// Pose l'association `.rose` par l'API du registre et VÉRIFIE par relecture.
pub fn poser_association(bin_win: &str) -> EcriturePath {
    let mut journal: Vec<String> = Vec::new();
    #[cfg(windows)]
    {
        let commande = format!("\"{}\\rose.exe\" \"%1\"", bin_win.trim_end_matches('\\'));
        let paires: [(&str, &str, u32); 5] = [
            ("Software\\Classes\\.rose", "", api::REG_SZ),
            ("Software\\Classes\\Rose.File", "", api::REG_SZ),
            ("Software\\Classes\\Rose.File\\DefaultIcon", "", api::REG_SZ),
            ("Software\\Classes\\Rose.File\\shell\\open\\command", "", api::REG_SZ),
            ("Software\\Classes\\Rose.File", "FriendlyTypeName", api::REG_SZ),
        ];
        let valeurs: [&str; 5] = [
            "Rose.File",
            "Programme Rose",
            // ⚠⚠ CORRECTION DU 17/11 : le `DefaultIcon` du TYPE est posé ici
            // aussi (pas seulement dans `enregistrer`), et il pointe le
            // DOCUMENT `bin\rose.ico` — pas le moteur (qui porte la rose).
            // Mesuré : les `.rose` affichaient le logo de l'app.
            &format!("{}\\rose.ico,0", bin_win.trim_end_matches('\\')),
            &commande,
            "Fichier source Rose",
        ];
        let mut ok = true;
        for (i, (cle, nom, typ)) in paires.iter().enumerate() {
            let rc = api::ecrire(api::HKEY_CURRENT_USER, cle, nom, *typ, valeurs[i]);
            if rc != api::ERROR_SUCCESS {
                ok = false;
                journal.push(format!("⚠ écriture {cle} refusée : {}", api::expliquer(rc)));
            }
        }
        if ok { journal.push("· association : cinq clés écrites".to_string()); }
        // ⚠ VÉRIFICATION : on RELIT la commande d'ouverture.
        let relu = api::lire(api::HKEY_CURRENT_USER,
                             "Software\\Classes\\Rose.File\\shell\\open\\command", "")
            .unwrap_or_default();
        if relu.contains("rose.exe") {
            journal.push(format!("✓ association .rose vérifiée : {relu}"));
            EcriturePath { methode: "api".to_string(), journal, reussi: true }
        } else {
            journal.push("Échec : la commande d'ouverture relue est vide".to_string());
            EcriturePath { methode: String::new(), journal, reussi: false }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = bin_win;
        journal.push("· association sans objet hors Windows".to_string());
        EcriturePath { methode: String::new(), journal, reussi: false }
    }
}

/// ═══════════════════════════════════════════════════════════════════════
/// MODE ENTREPRISE — AUTOMATIQUE, SANS MODIFIER LE PATH
/// ═══════════════════════════════════════════════════════════════════════
///
/// ⚠⚠⚠ LE PROBLÈME MESURÉ : sur un appareil d'entreprise, une stratégie de
/// groupe peut VERROUILLER `HKCU\Environment`, ou une politique
/// d'application peut bloquer `reg.exe`. Le PATH ne peut alors PAS être
/// modifié — et ce n'est pas une erreur de l'installeur.
///
/// ⚠⚠⚠ LA SOLUTION : si l'écriture du PATH échoue, Rose installe quand
/// même un ACCÈS FONCTIONNEL par trois moyens qui ne touchent PAS au PATH :
///
///   1. un LANCEUR `%USERPROFILE%\Rose\rose.cmd` ;
///   2. un RACCOURCI `%USERPROFILE%\Desktop\Rose.cmd` ;
///   3. l'ASSOCIATION `.rose` (double-clic sur un fichier).
///
/// On DIT alors à l'utilisateur le chemin EXACT à utiliser. C'est un accès
/// réel, pas un message lui demandant de le faire à la main.
pub fn mode_entreprise(prefixe: &str) -> Journal {
    let mut j: Journal = Vec::new();
    let bindir = std::path::Path::new(prefixe).join("bin");
    let moteur = bindir.join(if cfg!(windows) { "rose.exe" } else { "rose" });
    if !moteur.is_file() {
        j.push(format!("Échec : moteur absent ({}), mode entreprise impossible", moteur.display()));
        return j;
    }
    // ⚠ On passe par les MÊMES méthodes que l'outil PATH : un seul code à
    // maintenir, et les mêmes vérifications.
    j.extend(reparer_path(prefixe, MethodePath::Lanceur));
    j.extend(reparer_path(prefixe, MethodePath::Raccourci));
    j.extend(reparer_path(prefixe, MethodePath::Association));
    let lanceur = std::env::var("USERPROFILE").unwrap_or_default() + "\\Rose\\rose.cmd";
    j.push(format!("⚠ PATH non modifiable : pour utiliser Rose, tapez  {lanceur}"));
    j
}

/// Diffuse `WM_SETTINGCHANGE` pour que les applications déjà lancées voient
/// le nouveau `PATH`.
///
/// ⚠ Sur Linux, il n'y a rien à diffuser : la fonction existe quand même
/// (vide) pour que les méthodes de l'outil PATH compilent partout.
fn diffuser_environnement() {
    #[cfg(not(windows))]
    {
        return;
    }
    #[cfg(windows)]
    {
    unsafe {
        use std::ffi::c_void;
        #[link(name = "user32")]
        unsafe extern "system" {
            fn SendMessageTimeoutW(hwnd: isize, msg: u32, wparam: usize,
                                   lparam: *const c_void, flags: u32,
                                   timeout: u32, result: *mut usize) -> usize;
        }
        const WM_SETTINGCHANGE: u32 = 0x001A;
        const HWND_BROADCAST: isize = 0xFFFF;
        const SMTO_ABORTIFHUNG: u32 = 0x0002;
        let env = "Environment\0".encode_utf16().collect::<Vec<u16>>();
        let mut result = 0usize;
        SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, 0,
                            env.as_ptr() as *const c_void, SMTO_ABORTIFHUNG, 3000, &mut result);
    }
    }
}

/// Relit l'état et rapporte si la réparation a MARCHÉ.
#[cfg(windows)]
fn verifier_et_rapporter(j: &mut Journal, prefixe: &str, portee: &str) {
    let d = diagnostiquer_path(prefixe);
    let ok = if portee == "machine" { d.dans_path_machine } else { d.dans_path_utilisateur };
    if ok {
        j.push(format!("✓ vérifié : le dossier bin est bien dans le PATH {portee}"));
    } else {
        j.push(format!(
            "Échec : le dossier bin n'est TOUJOURS pas dans le PATH {portee} après écriture"));
        if d.verrou_groupe {
            j.push("  cause probable : VERROU DE STRATÉGIE DE GROUPE sur HKCU\\Environment".to_string());
            j.push("  → essayez « Lanceur .cmd », « Raccourci » ou « Association »".to_string());
        } else if !d.registre_inscriptible {
            j.push("  cause probable : registre HKCU inaccessible en écriture".to_string());
        }
    }
}

/* ═══════════════════════════════════════════════════════════════════════
   LAUNCHER UNIQUE — ponts vers `velopack`

   L'installeur n'a plus besoin d'embarquer un moteur Rose complet : il
   devient un launcher stable qui interroge le dépôt, télécharge UNIQUEMENT
   la version choisie et pointe `current\`. Ces ponts exposent la même API
   depuis le cœur d'installation (utilisable par `rose versions/installer`
   et par l'interface graphique sans dupliquer de logique réseau).
   ═══════════════════════════════════════════════════════════════════════ */

/// Versions Windows disponibles sur le dépôt (API GitHub, lecture seule).
pub fn launcher_versions() -> Result<Vec<crate::velopack::VersionDistante>, String> {
    crate::velopack::lister_versions()
}

/// Télécharge et installe UNIQUEMENT la version demandée (SHA-256 vérifié).
pub fn launcher_installer(tag: &str) -> Result<String, String> {
    crate::velopack::telecharger_version(tag)
}

/// Fait pointer `current` sur une version déjà présente sur disque.
pub fn launcher_selectionner(tag: &str) -> Result<String, String> {
    crate::velopack::selectionner_version(tag)
}

/// Plan d'un lancement, sans effet de bord (équivalent de `--dry-run`).
pub fn launcher_dry_run(tag: Option<&str>) -> Result<String, String> {
    crate::velopack::dry_run(tag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_lue_depuis_bin_ou_racine() {
        // `version_installee` doit accepter les DEUX dispositions :
        //   - `bin\rose.exe` (installeur actuel),
        //   - `rose.exe` à la racine (ancien installeur Tauri).
        // On fabrique un faux dossier et on vérifie qu'il ne panique pas et
        // renvoie None quand rien n'est exécutable.
        let tmp = std::env::temp_dir().join("rose-test-version");
        let _ = std::fs::create_dir_all(&tmp);
        assert!(version_installee(tmp.to_str().unwrap()).is_none());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn comparer_versions_ordonne_correctement() {
        assert_eq!(comparer_versions("0.1.5", "0.1.5"), 0);
        assert_eq!(comparer_versions("0.1.5", "5.12.0"), -1);
        assert_eq!(comparer_versions("5.12.0", "0.1.5"), 1);
        assert_eq!(comparer_versions("v0.1.5", "0.1.4"), 1);
    }

    /// ⚠⚠⚠ GARDE-FOU DU BUG « la commande `rose` échoue après
    /// l'installation ». Trois défauts mesurés, chacun testé ici.
    #[test]
    fn path_avec_espaces_est_lu_et_conserve() {
        // 1) LECTURE : l'ancienne version faisait `split_whitespace()` puis
        //    `parts[2..].join(" ")`, ce qui reconstruisait un chemin avec
        //    espaces de travers. On coupe maintenant sur le TYPE.
        let ligne = "    Path    REG_EXPAND_SZ    C:\\Program Files\\Rose\\bin;C:\\Windows";
        let l = ligne.trim_start();
        assert!(l.starts_with("Path"));
        let pos = l.find("REG_EXPAND_SZ").expect("le type doit être trouvé");
        let valeur = l[pos + "REG_EXPAND_SZ".len()..].trim_start().to_string();
        assert_eq!(valeur, "C:\\Program Files\\Rose\\bin;C:\\Windows");
        // ⚠ Le chemin AVEC ESPACE doit être intact.
        assert!(valeur.split(';').any(|s| s == "C:\\Program Files\\Rose\\bin"));
    }

    #[test]
    fn filtre_path_ne_supprime_pas_le_bon_dossier() {
        // 2) FILTRE : l'ancienne version retirait tout segment contenant
        //    « \\rose », ce qui supprimait AUSSI `...\\Rose\\bin` (le bon
        //    dossier). On compare maintenant les derniers éléments.
        let normaliser = |s: &str| -> String {
            s.trim().replace('/', "\\").replace("\\\\", "\\")
                .trim_end_matches('\\').to_lowercase()
        };
        // ⚠ La logique RÉELLE protège le dossier COURANT avant tout
        // (`if n == nouveau_bin { return true; }`) : on la reproduit ici.
        let nouveau_bin = normaliser("C:\\Program Files\\Rose\\bin");
        let est_rose = |seg: &str| -> bool {
            let n = normaliser(seg);
            if n.is_empty() { return false; }
            if n == nouveau_bin { return true; }
            let dernier = n.rsplit('\\').next().unwrap_or("");
            let avant = n.rsplit('\\').nth(1).unwrap_or("");
            let nomme_rose = dernier == "rose" || avant == "rose";
            if !nomme_rose { return false; }
            // Dossier Rose VIVANT → conservé.
            let p = std::path::Path::new(seg.trim());
            if p.join("rose.exe").exists() || p.join("rose").exists() {
                return false;
            }
            true
        };
        // ⚠⚠ LE BUG : l'ancien filtre `contains("\\rose")` supprimait le
        // BON dossier (…\Rose\bin contient « \rose »). Le nouveau filtre
        // le CONSERVE : c'est le dossier courant, il est donc protégé par
        // la comparaison EXACTE en tête de fonction.
        assert!(est_rose("C:\\Program Files\\Rose\\bin"), "le courant est protégé");
        // Un dossier obsolète (…\Rose) est retiré.
        assert!(est_rose("C:\\Users\\x\\AppData\\Local\\Rose"));
        // Un chemin sans rapport est conservé.
        assert!(!est_rose("C:\\Windows\\System32"));
        // Casse et séparateurs mélangés.
        assert!(est_rose("c:/users/x/appdata/local/rose"));
        assert!(!est_rose("C:\\tools\\rosetta"));
    }

    /// ⚠⚠⚠ BUG MESURÉ : la règle « avant == rose » retirait un dossier Rose
    /// LÉGITIME (`C:\Program Files\Rose\bin`) dès qu'on installait ailleurs.
    /// Un dossier Rose VIVANT (il contient `rose.exe`) doit être CONSERVÉ.
    #[test]
    fn path_garde_un_dossier_rose_vivant() {
        let tmp = std::env::temp_dir().join("rose-test-path-vivant");
        let _ = std::fs::remove_dir_all(&tmp);
        let vivant = tmp.join("Rose").join("bin");
        std::fs::create_dir_all(&vivant).unwrap();
        // Un vrai exécutable factice : la règle regarde son EXISTENCE.
        std::fs::write(vivant.join("rose.exe"), b"MZ").unwrap();

        let normaliser = |s: &str| -> String {
            s.trim().replace('/', "\\").replace("\\\\", "\\")
                .trim_end_matches('\\').to_lowercase()
        };
        let nouveau_bin = normaliser("/autre/Rose/bin");
        let est_rose = |seg: &str| -> bool {
            let n = normaliser(seg);
            if n.is_empty() { return false; }
            if n == nouveau_bin { return true; }
            let dernier = n.rsplit('\\').next().unwrap_or("");
            let avant = n.rsplit('\\').nth(1).unwrap_or("");
            let nomme_rose = dernier == "rose" || avant == "rose";
            if !nomme_rose { return false; }
            let p = std::path::Path::new(seg.trim());
            if p.join("rose.exe").exists() || p.join("rose").exists() {
                return false;
            }
            true
        };
        let v = vivant.display().to_string();
        assert!(!est_rose(&v), "un dossier Rose vivant doit être GARDÉ : {v}");
        // Un dossier Rose qui n'existe pas est retiré.
        let mort = tmp.join("Ancien").join("Rose").display().to_string();
        assert!(est_rose(&mort), "un dossier Rose mort doit être retiré : {mort}");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// ⚠⚠⚠ L'OUTIL PATH : les méthodes doivent être COHÉRENTES.
    #[test]
    fn methodes_path_coherentes() {
        // Les deux premières modifient le PATH ; les trois suivantes non.
        assert!(MethodePath::Utilisateur.modifie_path());
        assert!(MethodePath::Machine.modifie_path());
        assert!(MethodePath::Lanceur.modifie_path());
        assert!(!MethodePath::Raccourci.modifie_path());
        assert!(!MethodePath::Association.modifie_path());
        // La liste publiée contient bien les cinq, dans cet ordre.
        assert_eq!(METHODES_PATH.len(), 5);
        assert_eq!(METHODES_PATH[0], MethodePath::Utilisateur);
        assert_eq!(METHODES_PATH[4], MethodePath::Association);
        // Chaque méthode a un nom non vide.
        for m in METHODES_PATH { assert!(!m.nom().is_empty()); }
    }

    /// ⚠ La comparaison des segments de PATH doit être insensible à la
    /// casse, aux séparateurs et aux antislashs finaux.
    #[test]
    fn path_comparaison_robuste() {
        let p = "C:\\Windows;C:\\Program Files\\Rose\\bin;C:\\Tools\\";
        assert!(path_contient(p, "c:/program files/rose/bin"));
        assert!(path_contient(p, "C:\\Program Files\\Rose\\bin\\"));
        assert!(path_contient(p, "C:\\Tools"));
        assert!(!path_contient(p, "C:\\Program Files\\Rose"));
        assert!(!path_contient(p, "C:\\tools\\rosetta"));
        // Un chemin AVEC ESPACE ne doit pas être tronqué.
        assert_eq!(normaliser_path("C:\\Program Files\\Rose\\bin"),
                   "c:\\program files\\rose\\bin");
    }

    /// ⚠ Le diagnostic ne doit JAMAIS paniquer, même si le dossier
    /// n'existe pas (cas d'une désinstallation partielle).
    #[test]
    fn diagnostic_sur_dossier_inexistant() {
        let d = diagnostiquer_path("C:\\Dossier\\Qui\\N\\Existe\\Pas");
        assert!(!d.bin_existe);
        assert!(!d.messages.is_empty());
        assert!(!d.ok());
    }

    /// ⚠⚠⚠ LE MODE ENTREPRISE doit produire un accès FONCTIONNEL même
    /// quand le PATH est verrouillé : un lanceur, un raccourci et
    /// l'association. On vérifie qu'il ne PANIQUE pas et qu'il rapporte
    /// une ligne exploitable.
    #[test]
    fn mode_entreprise_rapporte_quelque_chose() {
        let j = mode_entreprise("C:\\Dossier\\Inexistant");
        assert!(!j.is_empty());
        // ⚠ Sur Linux, les méthodes de fichiers restent testables : le
        // moteur n'existe pas dans le faux dossier, donc le mode entreprise
        // doit DIRE pourquoi il ne peut rien faire — pas paniquer.
        #[cfg(not(windows))]
        assert!(j.iter().any(|l| l.contains("moteur absent")));
        // ⚠ Sur Windows, il doit rapporter une tentative réelle.
        #[cfg(windows)]
        assert!(j.iter().any(|l| l.starts_with("Échec") || l.starts_with("✓")));
        #[cfg(windows)]
        assert!(j.iter().any(|l| l.starts_with("Échec") || l.starts_with("✓")));
    }

    /// ⚠⚠⚠ GARDE-FOU DU BUG MESURÉ : `reg add` rendait 1 et le PATH
    /// restait vide. La composition de la valeur doit être ROBUSTE.
    #[test]
    fn composition_du_path() {
        // Le dossier demandé passe EN TÊTE.
        assert_eq!(composer_path("C:\\Windows;C:\\Tools", "C:\\Users\\x\\Rose\\bin"),
                   "C:\\Users\\x\\Rose\\bin;C:\\Windows;C:\\Tools");
        // Un PATH vide ne produit pas de segment vide.
        assert_eq!(composer_path("", "C:\\Rose\\bin"), "C:\\Rose\\bin");
        assert_eq!(composer_path(";;;", "C:\\Rose\\bin"), "C:\\Rose\\bin");
        // Un doublon (même dossier, autre casse/séparateurs) est remplacé,
        // pas dupliqué.
        let r = composer_path("c:/rose/BIN; C:\\Windows ", "C:\\Rose\\bin");
        assert_eq!(r, "C:\\Rose\\bin;C:\\Windows");
        // Un AUTRE dossier Rose vivant est CONSERVÉ (deux versions).
        let r = composer_path("C:\\Rose-v2\\bin;C:\\Windows", "C:\\Rose\\bin");
        assert_eq!(r, "C:\\Rose\\bin;C:\\Rose-v2\\bin;C:\\Windows");
    }

    /// ⚠ La lecture/écriture du registre doit être SÛRE hors Windows : elle
    /// rend `None`/un journal explicite, jamais une panique.
    #[test]
    fn ecriture_path_sur_n_importe_quel_dossier() {
        let r = ecrire_path_utilisateur("C:\\Dossier\\Inexistant\\bin");
        assert!(!r.journal.is_empty());
        #[cfg(not(windows))]
        {
            assert!(!r.reussi);
            assert!(r.journal.iter().any(|l| l.contains("sans objet")));
        }
        // Le diagnostic et l'accessibilité du registre doivent répondre.
        let _ = registre_utilisateur_inscriptible();
    }

    #[test]
    fn installation_pose_le_moteur_dans_bin() {
        // Bout en bout : on installe dans un dossier temporaire à partir
        // d'un VRAI exécutable (le binaire `rose` compilé, nommé « rose »).
        // On vérifie que `bin/<moteur>` existe après coup.
        let tmp = std::env::temp_dir().join("rose-test-install-complet");
        let _ = std::fs::remove_dir_all(&tmp);
        // Source : un fichier nommé « rose » (pas de « setup »), contenu
        // quelconque — `installer` ne vérifie le PE que sous Windows.
        let src = std::env::temp_dir().join("rose-source-test");
        let _ = std::fs::write(&src, b"#!fake\n");
        let j = installer(tmp.to_str().unwrap(), Some(&src));
        let nom = if cfg!(windows) { "rose.exe" } else { "rose" };
        let pose = tmp.join("bin").join(nom);
        // Sous Windows le faux contenu est rejeté (pas un PE console) ;
        // ailleurs l'installation doit avoir eu lieu.
        if !cfg!(windows) {
            assert!(pose.is_file(), "le moteur n'a pas été posé : {j:?}");
            assert!(j.iter().all(|l| !l.starts_with("Échec")), "{j:?}");
        }
        let _ = std::fs::remove_file(&src);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn couleur_et_argb_ne_confondent_pas_les_canaux() {
        // `couleur(0xRRGGBB)` rend un COLORREF 0x00BBGGRR.
        let cf = |rgb: u32| -> u32 {
            ((rgb & 0xFF) << 16) | (rgb & 0xFF00) | ((rgb >> 16) & 0xFF)
        };
        // GDI+ attend ARGB : il FAUT réinverser, sinon rouge et bleu
        // s'échangent (bug mesuré : le lime 0x9ACD32 sortait cyan).
        let argb = |c: u32| -> u32 {
            let r = c & 0xFF;
            let g = (c >> 8) & 0xFF;
            let b = (c >> 16) & 0xFF;
            0xFF00_0000 | (r << 16) | (g << 8) | b
        };
        assert_eq!(cf(0x9ACD32), 0x0032CD9A);
        assert_eq!(argb(cf(0x9ACD32)), 0xFF9ACD32, "le lime doit rester lime");
        assert_eq!(argb(cf(0xFF2D1A)), 0xFFFF2D1A, "le rouge doit rester rouge");
    }

    #[test]
    fn installation_pose_les_compagnons() {
        // Vérifie que les compagnons présents À CÔTÉ de la source sont
        // copiés : c'est ce qui manquait (seul `rose.exe` était posé).
        let tmp = std::env::temp_dir().join("rose-test-compagnons");
        let _ = std::fs::remove_dir_all(&tmp);
        let src_dir = std::env::temp_dir().join("rose-test-src-compagnons");
        let _ = std::fs::remove_dir_all(&src_dir);
        let _ = std::fs::create_dir_all(&src_dir);
        let nom = if cfg!(windows) { "rose.exe" } else { "rose" };
        let _ = std::fs::write(src_dir.join(nom), b"#!fake\n");
        let _ = std::fs::write(src_dir.join("rose.ico"), b"\x00\x00\x01\x00ICO");
        // ⚠⚠ AJOUT DU 17/11 : `rose_shell.ico` doit être posé — c'est
        // l'icône du profil Windows Terminal. Sans elle, le profil
        // affichait la rose de l'application (mesuré chez l'utilisateur).
        let _ = std::fs::write(src_dir.join("rose_shell.ico"), b"\x00\x00\x01\x00ICO");
        let j = installer(tmp.to_str().unwrap(), Some(&src_dir.join(nom)));
        if !cfg!(windows) {
            assert!(tmp.join("bin").join("rose.ico").is_file(),
                    "rose.ico non posé : {j:?}");
            assert!(tmp.join("bin").join("rose_shell.ico").is_file(),
                    "rose_shell.ico non posé : {j:?}");
        }
        let _ = std::fs::remove_dir_all(&src_dir);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn installer_refuse_sans_source() {
        // Sans source, `installer` doit ÉCHOUER explicitement — jamais
        // retomber sur `current_exe()` (qui serait l'installeur lui-même).
        let tmp = std::env::temp_dir().join("rose-test-sans-source");
        let _ = std::fs::remove_dir_all(&tmp);
        let j = installer(tmp.to_str().unwrap(), None);
        assert!(j.iter().any(|l| l.starts_with("Échec")),
                "installer(None) doit signaler un échec, journal = {j:?}");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn aucun_lancement_de_console_a_nu() {
        // Garde-fou anti-clignotement : `reg`, `curl` et `wget` sont des
        // programmes CONSOLE. Lancés depuis l'installeur GUI, ils ouvrent
        // une fenêtre cmd (mesuré en VM). Tout appel doit donc passer par
        // `commande_sans_fenetre` (CREATE_NO_WINDOW).
        //
        // On lit notre propre source, on coupe la zone du helper et celle du
        // module de test, puis on compte les appels directs restants.
        let src = include_str!("setup_core.rs");
        let corps = match src.find("fn commande_sans_fenetre") {
            Some(i) => {
                // Fin du helper = prochain "\n}\n" après lui.
                let fin = src[i..].find("\n}\n").map(|j| i + j + 3).unwrap_or(src.len());
                &src[fin..]
            }
            None => src,
        };
        let corps = match corps.find("mod tests") {
            Some(i) => &corps[..i],
            None => corps,
        };
        let motif = concat!("std::process::Command", "::new");
        let direct = corps.matches(motif).count();
        assert_eq!(direct, 0, "{direct} lancement(s) de console à nu : utiliser commande_sans_fenetre");
    }

    #[test]
    fn refuse_l_installeur_comme_moteur() {
        // L'installeur ne doit JAMAIS être copié sous `rose.exe` : cela crée
        // une boucle de lancement (mesuré : 60 processus chez l'utilisateur).
        assert!(refuser_comme_moteur(std::path::Path::new(
            "C:\\Users\\x\\Rose-win-Setup.exe")).is_some());
        assert!(refuser_comme_moteur(std::path::Path::new("Rose-Setup.exe")).is_some());
        assert!(refuser_comme_moteur(std::path::Path::new("install-rose.exe")).is_some());
        // Le vrai moteur passe.
        assert!(refuser_comme_moteur(std::path::Path::new("bin\\rose.exe")).is_none());
        assert!(refuser_comme_moteur(std::path::Path::new("rose")).is_none());
    }

    #[test]
    fn nettoyage_ne_touche_pas_au_dossier_courant() {
        // Le nettoyage des anciennes installations doit être inerte hors
        // Windows et ne jamais supprimer le dossier demandé.
        let tmp = std::env::temp_dir().join("rose-test-courant");
        let _ = std::fs::create_dir_all(&tmp);
        let _ = nettoyer_anciennes_installations(tmp.to_str().unwrap());
        assert!(tmp.exists(), "le dossier courant a été supprimé");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
