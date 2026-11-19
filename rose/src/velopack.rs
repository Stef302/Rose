//! Mise à jour Windows par Velopack (deltas binaires + rollback).
//!
//! Velopack REMPLACE l'installeur WiX / NSIS : il installe l'application dans
//! un dossier versionné (`current\` désigne `app-0.1.4\`) et laisse un
//! launcher (`Update.exe`) rejouer la mise à jour et revenir à la version
//! précédente si la nouvelle ne démarre pas.
//!
//! Ce module reproduit côté Rust le PROTOCOLE Velopack, sans dépendance
//! (aucun crate `velopack`) :
//!
//! - `is_update_launcher` : Velopack relance l'application avec des arguments
//!   internes (`--veloapp-updated`, `--veloapp-obsolete`) ;
//! - `run_update_launcher` : sort en succès après l'application du paquet
//!   (`updated`) ou après un rollback (`obsolete`) ;
//! - `check_and_update` : lit `releases.win.json` (le manifeste Velopack),
//!   compare la version installée, télécharge le paquet COMPLET (`Type: Full`,
//!   les deltas ne sont pas rejoués ici), vérifie son SHA-256 puis le dépose
//!   dans `packages\` ;
//! - `appliquer_paquets` : quand AUCUN launcher Velopack n'est installé
//!   (`Rose-win-Setup.exe` téléchargé à la main), on applique nous-mêmes les
//!   paquets en attente : extraction ZIP du `.nupkg`, copie de `lib/net45\`
//!   dans `current\`, écriture du fichier `current`. La version précédente
//!   reste sur disque (`app-<ancienne>`), donc un rollback manuel est possible.
//!
//! Sous Linux et macOS, `rose update` garde la voie artefacts GitHub
//! (`docs/telechargements/` + SHA-256) : Velopack n'a pas d'installeur natif
//! Linux. La voie reste testable ici avec `ROSE_VELOPACK_ROOT` (racine de
//! l'installation), `ROSE_VELOPACK_URL` (manifeste `file://` ou HTTP) et
//! `ROSE_VELOPACK=1`.
//!
//! Licence MIT.


use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Lance `curl` SANS fenêtre de console.
///
/// ⚠ `curl.exe` est un programme console : appelé depuis une application
/// GUI (l'installeur), Windows lui alloue une console — une fenêtre noire
/// apparaît brièvement. `CREATE_NO_WINDOW` la supprime.
fn curl_sans_fenetre() -> std::process::Command {
    #[allow(unused_mut)]
    let mut c = std::process::Command::new("curl");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

/// Variable posée par le launcher Velopack : sa présence prouve que Rose
/// tourne sous une installation Velopack.
pub const VAR_LAUNCHER: &str = "VELOCABI";

/// Dossier racine posé par Velopack : `%LOCALAPPDATA%\Rose`.
pub const DOSSIER_VELOPACK: &str = "Rose";

/// Arguments internes du launcher.
pub const ARG_MIS_A_JOUR: &str = "--veloapp-updated";
pub const ARG_OBSOLETE: &str = "--veloapp-obsolete";

/// Vrai si le processus a été lancé par le launcher Velopack avec un argument
/// interne (mise à jour appliquée ou ancienne version devenue obsolète).
pub fn is_update_launcher(args: &[String]) -> bool {
    args.iter()
        .skip(1)
        .any(|a| a == ARG_MIS_A_JOUR || a == ARG_OBSOLETE)
}

/// Répond au launcher et rend le code de sortie.
///
/// - `--veloapp-updated` : la nouvelle version est posée ; le launcher a déjà
///   écrit `current\` et le PATH — on sort en succès.
/// - `--veloapp-obsolete` : Velopack a rejoué l'ancienne version après un
///   rollback ; on ne touche à rien et on sort en succès.
pub fn run_update_launcher(args: &[String]) -> i32 {
    let _ = args;
    0
}

/// Racine de l'installation Velopack (`%LOCALAPPDATA%\Rose`).
///
/// `ROSE_VELOPACK_ROOT` force la racine (tests et installations hors profil).
pub fn dossier_racine() -> Option<PathBuf> {
    if let Some(r) = std::env::var_os("ROSE_VELOPACK_ROOT") {
        return Some(PathBuf::from(r));
    }
    let base = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(base).join(DOSSIER_VELOPACK))
}

/// Vrai si Rose tourne depuis une installation Velopack : le launcher pose
/// `VELOCABI` (ou `ROSE_VELOPACK=1` pour forcer la voie, tests compris).
pub fn est_velopack() -> bool {
    std::env::var_os(VAR_LAUNCHER).is_some() || std::env::var_os("ROSE_VELOPACK").is_some()
}

/// Version installée : contenu du fichier `current` (`app-0.1.4`), sinon le
/// dossier `app-*` le plus récent. `None` hors d'une installation Velopack.
pub fn version_installee() -> Option<String> {
    let racine = dossier_racine()?;
    let courant = racine.join("current");
    // `current` est un petit fichier texte qui nomme le dossier versionné.
    if let Ok(nom) = fs::read_to_string(&courant) {
        let nom = nom.trim();
        if let Some(v) = nom.strip_prefix("app-") {
            return Some(v.to_string());
        }
    }
    let mut versions: Vec<String> = fs::read_dir(&racine)
        .ok()?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let nom = e.file_name().to_string_lossy().into_owned();
            nom.strip_prefix("app-").map(|v| v.to_string())
        })
        .collect();
    versions.sort_by(|a, b| comparer_versions(a, b));
    versions.pop()
}

/// Dossier réellement utilisé par l'installation : `current\` s'il existe,
/// sinon `app-<version la plus récente>`.
pub fn dossier_courant() -> Option<PathBuf> {
    let racine = dossier_racine()?;
    let courant = racine.join("current");
    if courant.is_dir() {
        return Some(courant);
    }
    version_installee().map(|v| racine.join(format!("app-{v}")))
}

/// Compare deux versions `x.y.z` (segments absents = 0).
///
/// Chaque segment est lu chiffre par chiffre : un suffixe non numérique
/// (« 4-beta ») ne peut PAS s'effondrer sur 0, ce qui ferait croire que
/// « 0.1.4 » est inférieure à « 0.1.3 ».
pub fn comparer_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let seg = |s: &str| -> Vec<u64> {
        s.split(['.', '-', '+'])
            .map(|p| {
                let chiffres: String = p.chars().take_while(|c| c.is_ascii_digit()).collect();
                chiffres.parse::<u64>().unwrap_or(0)
            })
            .collect()
    };
    let (va, vb) = (seg(a), seg(b));
    for i in 0..va.len().max(vb.len()) {
        let x = *va.get(i).unwrap_or(&0);
        let y = *vb.get(i).unwrap_or(&0);
        if x != y {
            return x.cmp(&y);
        }
    }
    std::cmp::Ordering::Equal
}

/// Entrée d'un paquet dans `releases.win.json`.
#[derive(Debug, Clone)]
pub struct Paquet {
    pub version: String,
    pub nom: String,
    pub taille: u64,
    pub sha256: String,
    pub sha1: String,
    /// Vrai pour le paquet complet, faux pour un delta.
    pub complet: bool,
}

/// Manifeste Velopack réduit aux paquets Windows.
#[derive(Debug, Clone)]
pub struct Manifeste {
    pub paquets: Vec<Paquet>,
}

/// Extrait la valeur d'une clé dans un objet JSON plat (sans parseur : le
/// manifeste Velopack est une suite d'objets `{ "Clé": "Valeur", … }`).
fn valeur(json: &str, cle: &str) -> Option<String> {
    let motif = format!("\"{}\"", cle);
    let i = json.find(&motif)?;
    let reste = &json[i + motif.len()..];
    let d = reste.find(':')?;
    let reste = reste[d + 1..].trim_start();
    if let Some(r) = reste.strip_prefix('"') {
        let fin = r.find('"')?;
        return Some(r[..fin].to_string());
    }
    let fin = reste.find([',', '}']).unwrap_or(reste.len());
    Some(reste[..fin].trim().to_string())
}

impl Manifeste {
    /// Analyse `releases.win.json` : chaque paquet porte `PackageId`,
    /// `Version`, `SHA256`, `SHA1`, `Size` ; `Type: "Full"` ou `"Delta"`.
    pub fn analyser(json: &str) -> Result<Manifeste, String> {
        let mut paquets = Vec::new();
        let mut profondeur = 0i32;
        let mut debut = None;
        for (i, c) in json.char_indices() {
            match c {
                '{' => {
                    if profondeur == 0 {
                        debut = Some(i);
                    }
                    profondeur += 1;
                }
                '}' => {
                    profondeur -= 1;
                    if profondeur == 0 {
                        if let Some(d) = debut {
                            let obj = &json[d..=i];
                            if let Some(nom) = valeur(obj, "PackageId") {
                                let complet = valeur(obj, "Type")
                                    .map(|t| !t.eq_ignore_ascii_case("delta"))
                                    .unwrap_or(true);
                                paquets.push(Paquet {
                                    version: valeur(obj, "Version").unwrap_or_default(),
                                    nom,
                                    taille: valeur(obj, "Size")
                                        .and_then(|s| s.parse::<u64>().ok())
                                        .unwrap_or(0),
                                    sha256: valeur(obj, "SHA256").unwrap_or_default(),
                                    sha1: valeur(obj, "SHA1").unwrap_or_default(),
                                    complet,
                                });
                            }
                        }
                        debut = None;
                    }
                }
                _ => {}
            }
        }
        if paquets.is_empty() {
            return Err("manifeste Velopack vide ou illisible".into());
        }
        Ok(Manifeste { paquets })
    }

    /// Paquet COMPLET de la version la plus récente strictement supérieure à
    /// `installee`. Les deltas ne sont pas rejoués : seul le paquet complet
    /// est téléchargé.
    pub fn paquet_a_jour(&self, installee: &str) -> Option<&Paquet> {
        self.paquets
            .iter()
            .filter(|p| p.complet)
            .filter(|p| comparer_versions(&p.version, installee) == std::cmp::Ordering::Greater)
            .max_by(|a, b| comparer_versions(&a.version, &b.version))
    }
}

/// Lit un manifeste depuis une URL HTTP(S) ou un fichier `file://` —
/// `curl.exe` est présent sur Windows 10/11, aucune dépendance Rust.
fn lire_source(url: &str) -> Result<String, String> {
    if let Some(chemin) = url.strip_prefix("file://") {
        return fs::read_to_string(chemin).map_err(|e| format!("lecture {chemin} : {e}"));
    }
    let sortie = curl_sans_fenetre()
        .args(["-sSL", "--fail", url])
        .output()
        .map_err(|e| format!("curl indisponible : {e}"))?;
    if !sortie.status.success() {
        return Err(format!("téléchargement de {url} impossible (code {})", sortie.status));
    }
    String::from_utf8(sortie.stdout).map_err(|e| format!("réponse non UTF-8 : {e}"))
}

/// URL du manifeste Velopack : `ROSE_VELOPACK_URL` sinon la dernière release.
pub fn url_manifeste() -> String {
    if let Ok(u) = std::env::var("ROSE_VELOPACK_URL") {
        return u;
    }
    "https://github.com/Stef302/Rose/releases/latest/download/releases.win.json".to_string()
}

/// SHA-256 d'un fichier (implémentation maison, sans dépendance).
fn sha256_fichier(chemin: &Path) -> Result<String, String> {
    let mut f = fs::File::open(chemin).map_err(|e| format!("ouverture : {e}"))?;
    let mut octets = Vec::new();
    f.read_to_end(&mut octets).map_err(|e| format!("lecture : {e}"))?;
    Ok(crate::sha256::sha256_hex(&octets))
}

/// Interroge le manifeste, télécharge le paquet complet de la dernière
/// version et le dépose dans `packages\`.
///
/// `Ok(Some(version))` : paquet téléchargé et vérifié ;
/// `Ok(None)` : Rose est déjà à jour ; `Err` : tout autre échec.
pub fn check_and_update(_silencieux: bool) -> Result<Option<String>, String> {
    let installee = version_installee().unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
    let manifeste = Manifeste::analyser(&lire_source(&url_manifeste())?)?;
    let paquet = match manifeste.paquet_a_jour(&installee) {
        Some(p) => p.clone(),
        None => return Ok(None),
    };

    let racine = dossier_racine().ok_or("LOCALAPPDATA indisponible")?;
    let paquets = racine.join("packages");
    fs::create_dir_all(&paquets).map_err(|e| format!("création {} : {e}", paquets.display()))?;
    let cible = paquets.join(&paquet.nom);

    let url_paquet = match url_manifeste().rsplit_once('/') {
        Some((d, _)) => format!("{}/{}", d, paquet.nom),
        None => return Err("URL du manifeste invalide".into()),
    };
    let statut = curl_sans_fenetre()
        .args(["-sSL", "--fail", "-o"])
        .arg(&cible)
        .arg(&url_paquet)
        .status()
        .map_err(|e| format!("curl indisponible : {e}"))?;
    if !statut.success() {
        return Err(format!("téléchargement du paquet {} impossible", paquet.nom));
    }

    if !paquet.sha256.is_empty() {
        let h = sha256_fichier(&cible)?;
        if !h.eq_ignore_ascii_case(&paquet.sha256) {
            let _ = fs::remove_file(&cible);
            return Err(format!(
                "SHA-256 du paquet invalide (attendu {}, obtenu {})",
                paquet.sha256, h
            ));
        }
    }
    Ok(Some(paquet.version))
}

/// Vrai si un launcher Velopack est installé dans la racine (fichier
/// `Update.exe` ou `sq.version` à côté de `current\`).
fn launcher_present(racine: &Path) -> bool {
    racine.join("Update.exe").is_file() || racine.join("sq.version").is_file()
}

/// Applique les paquets `.nupkg` présents dans `packages\` quand AUCUN
/// launcher Velopack n'est installé (installation faite à la main par
/// `Rose-win-Setup.exe`) : extraction ZIP du paquet, copie de `lib/net45\`
/// dans `app-<version>\`, puis écriture de `current`. La version précédente
/// reste sur disque : un rollback manuel consiste à réécrire `current`.
///
/// Rend `Ok(Some(version appliquée))`, `Ok(None)` s'il n'y a rien à faire
/// (launcher présent, ou aucun paquet en attente).
pub fn appliquer_paquets() -> Result<Option<String>, String> {
    let racine = dossier_racine().ok_or("LOCALAPPDATA indisponible")?;
    if launcher_present(&racine) {
        // Le launcher Velopack fait le travail lui-même au prochain démarrage.
        return Ok(None);
    }
    let paquets = racine.join("packages");
    let mut nupkg: Option<PathBuf> = None;
    if let Ok(entrees) = fs::read_dir(&paquets) {
        for e in entrees.flatten() {
            let nom = e.file_name().to_string_lossy().into_owned();
            if nom.ends_with("-full.nupkg") {
                nupkg = Some(e.path());
            }
        }
    }
    let nupkg = match nupkg {
        Some(p) => p,
        None => return Ok(None),
    };

    // version lue dans `sq.version` du paquet, sinon dans le nom de fichier
    let version = lire_sq_version(&nupkg)
        .or_else(|| {
            nupkg.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .and_then(|s| s.strip_prefix("Rose-").map(|v| v.trim_end_matches("-full").to_string()))
        })
        .ok_or("version du paquet illisible")?;

    let destination = racine.join(format!("app-{version}"));
    fs::create_dir_all(&destination).map_err(|e| format!("création {} : {e}", destination.display()))?;
    extraire_net45(&nupkg, &destination)?;
    // `current` : fichier texte qui nomme le dossier versionné (format Velopack)
    fs::write(racine.join("current"), format!("app-{version}"))
        .map_err(|e| format!("écriture de current : {e}"))?;
    let _ = fs::remove_file(&nupkg);
    // Réaffirme l'association APRÈS la mise à jour, en pointant le chemin
    // STABLE `current\rose.exe` (pas `app-<version>\rose.exe`) : l'icône
    // et les verbes restent valides quand une nouvelle version est posée.
    // `appliquer_silencieuse` est un no-op hors Windows (tests Linux intact).
    let exe_courant = racine.join("current").join("rose.exe");
    if let Err(e) = crate::assoc_windows::appliquer_silencieuse(Some(&exe_courant)) {
        eprintln!("Association .rose après mise à jour : {e}");
    }
    Ok(Some(version))
}

/// Extrait `sq.version` d'un paquet (ZIP) et rend la version lue.
fn lire_sq_version(nupkg: &Path) -> Option<String> {
    let octets = fs::read(nupkg).ok()?;
    let donnees = entrees_zip(&octets)
        .into_iter()
        .find(|(n, _)| n == "sq.version")
        .map(|(_, d)| d)?;
    let xml = String::from_utf8_lossy(&donnees).into_owned();
    let i = xml.find("<version>")? + "<version>".len();
    let j = xml[i..].find("</version>")? + i;
    Some(xml[i..j].trim().to_string())
}

/// Extrait toutes les entrées `lib/net45/...` du paquet vers `destination`
/// (sous-dossiers recréés). Lecture du ZIP local uniquement : en-têtes
/// locaux + méthode « stored » (0) ou « deflate » (8).
fn extraire_net45(nupkg: &Path, destination: &Path) -> Result<(), String> {
    let octets = fs::read(nupkg).map_err(|e| format!("lecture du paquet : {e}"))?;
    let mut extraits = 0;
    for (nom, donnees) in entrees_zip(&octets) {
        let Some(relatif) = nom.strip_prefix("lib/net45/") else { continue };
        if relatif.is_empty() || relatif.ends_with('/') {
            continue;
        }
        let cible = destination.join(relatif.replace('/', std::path::MAIN_SEPARATOR_STR));
        if let Some(parent) = cible.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("création {} : {e}", parent.display()))?;
        }
        fs::write(&cible, &donnees).map_err(|e| format!("écriture {} : {e}", cible.display()))?;
        extraits += 1;
    }
    if extraits == 0 {
        return Err("paquet sans entrée lib/net45 (format inattendu)".into());
    }
    Ok(())
}


/// Parcourt les en-têtes locaux d'une archive ZIP et rend (nom, données).
fn entrees_zip(octets: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut sortie = Vec::new();
    let mut i = 0usize;
    while i + 30 <= octets.len() {
        if &octets[i..i + 4] != b"PK\x03\x04" {
            break;
        }
        let methode = u16::from_le_bytes([octets[i + 8], octets[i + 9]]);
        let taille_comp = u32::from_le_bytes([octets[i + 18], octets[i + 19], octets[i + 20], octets[i + 21]]) as usize;
        let taille_nom = u16::from_le_bytes([octets[i + 26], octets[i + 27]]) as usize;
        let taille_ext = u16::from_le_bytes([octets[i + 28], octets[i + 29]]) as usize;
        let debut_nom = i + 30;
        let debut_donnees = debut_nom + taille_nom + taille_ext;
        if debut_nom + taille_nom > octets.len() {
            break;
        }
        let nom = String::from_utf8_lossy(&octets[debut_nom..debut_nom + taille_nom]).into_owned();
        // Certains écrivains (flux ZIP non cherchable) laissent les tailles à 0
        // dans l'en-tête local : on prend alors la taille de l'entrée centrale.
        let mut taille = taille_comp;
        if taille == 0 {
            if let Some(t) = taille_centrale(octets, &nom) {
                taille = t;
            }
        }
        if debut_donnees + taille > octets.len() {
            break;
        }
        let brut = &octets[debut_donnees..debut_donnees + taille];
        let donnees = match methode {
            0 => brut.to_vec(),
            8 => crate::inflate::decompresser(brut).unwrap_or_default(),
            _ => Vec::new(),
        };
        sortie.push((nom, donnees));
        i = debut_donnees + taille;
    }
    sortie
}

/// Taille compressée d'une entrée, lue dans le répertoire central (EOCD →
/// en-têtes centraux `PK\x01\x02`).
fn taille_centrale(octets: &[u8], nom: &str) -> Option<usize> {
    // EOCD : signature PK\x05\x06, dernière occurrence
    let eocd = (0..octets.len().saturating_sub(4))
        .rev()
        .find(|&k| &octets[k..k + 4] == b"PK\x05\x06")?;
    let nb = u16::from_le_bytes([octets[eocd + 10], octets[eocd + 11]]) as usize;
    let mut i = u32::from_le_bytes([
        octets[eocd + 16],
        octets[eocd + 17],
        octets[eocd + 18],
        octets[eocd + 19],
    ]) as usize;
    for _ in 0..nb {
        if i + 46 > octets.len() || &octets[i..i + 4] != b"PK\x01\x02" {
            return None;
        }
        let taille = u32::from_le_bytes([octets[i + 20], octets[i + 21], octets[i + 22], octets[i + 23]]) as usize;
        let l_nom = u16::from_le_bytes([octets[i + 28], octets[i + 29]]) as usize;
        let l_ext = u16::from_le_bytes([octets[i + 30], octets[i + 31]]) as usize;
        let l_com = u16::from_le_bytes([octets[i + 32], octets[i + 33]]) as usize;
        let nom_i = String::from_utf8_lossy(&octets[i + 46..i + 46 + l_nom]).into_owned();
        if nom_i == nom {
            return Some(taille);
        }
        i += 46 + l_nom + l_ext + l_com;
    }
    None
}

/* ═══════════════════════════════════════════════════════════════════════
   LAUNCHER UNIQUE — un seul exécutable, les versions viennent du dépôt

   Principe (remplace « un Rose-win-Setup-<version>.exe par release ») :

   1. `lister_versions()` interroge l'API PUBLIQUE des releases GitHub
      (`GET /repos/Stef302/Rose/releases`). Aucune version n'est codée en
      dur : une release future apparaît dès qu'elle est publiée.
   2. L'utilisateur choisit un tag (0.1.3, 0.1.5, …).
   3. `telecharger_version(tag)` ne télécharge QUE le paquet complet
      (`Rose-<v>-full.nupkg`) de CE tag, vérifie son SHA-256, l'extrait dans
      `app-<v>\` et fait pointer `current\`. Le moteur Rose n'est PAS
      embarqué dans le launcher : il n'y a qu'UN téléchargement, une seule
      fois, pour la version choisie.
   4. `selectionner_version(tag)` rebascule `current\` sur une version déjà
      présente sur disque (changement de version sans rien retélécharger).
   5. Le binaire launcher reste IDENTIQUE d'une release à l'autre.

   Accès réseau : `gh api` si l'outil est présent, sinon `curl` (présent sur
   Windows 10/11). Aucun crate externe. `file://` + variables
   `ROSE_LAUNCHER_API`, `ROSE_LAUNCHER_BASE`, `ROSE_LAUNCHER_REPO` permettent
   de tout tester hors ligne (voir les tests en fin de module).
   ═══════════════════════════════════════════════════════════════════════ */

/// Dépôt GitHub des releases (lecture seule — aucune publication ici).
pub const DEPOT: &str = "Stef302/Rose";

/// API qui liste TOUTES les releases (contrairement à `/releases/latest`,
/// qui ne donne que la dernière, et au manifeste `releases.win.json`, qui
/// ne contient qu'UNE version).
pub const API_RELEASES: &str =
    "https://api.github.com/repos/Stef302/Rose/releases?per_page=100";

/// Base des artefacts téléchargeables : `<base>/<tag>/<fichier>`.
pub const BASE_ARTIFACTS: &str = "https://github.com/Stef302/Rose/releases/download";

/// Nom du cache local du manifeste (dernière version consultée).
pub const NOM_CACHE: &str = "releases.win.json";

/// `v0.1.5` → `0.1.5`. Le préfixe `v` n'est retiré que s'il est suivi d'un
/// chiffre (un tag exotique comme `version-test` reste intact).
fn version_depuis_tag(tag: &str) -> String {
    let t = tag.trim();
    if let Some(reste) = t.strip_prefix('v') {
        if matches!(reste.as_bytes().first(), Some(b'0'..=b'9')) {
            return reste.to_string();
        }
    }
    t.to_string()
}

fn api_url() -> String {
    std::env::var("ROSE_LAUNCHER_API").unwrap_or_else(|_| API_RELEASES.to_string())
}

fn depot() -> String {
    std::env::var("ROSE_LAUNCHER_REPO").unwrap_or_else(|_| DEPOT.to_string())
}

fn base_artifacts() -> String {
    std::env::var("ROSE_LAUNCHER_BASE").unwrap_or_else(|_| BASE_ARTIFACTS.to_string())
}

fn url_asset(tag: &str, fichier: &str) -> String {
    format!("{}/{}/{}", base_artifacts(), tag, fichier)
}

/// `gh api` si l'outil est présent ET répond du JSON ; `None` sinon. `gh`
/// n'est pas obligatoire : c'est une optimisation (déjà authentifié, quota
/// API plus large). Le CLI de cette machine est connecté comme `clausake`,
/// mais l'API est PUBLIQUE : aucune publication n'est faite ici.
fn essayer_gh(endpoint: &str) -> Option<String> {
    let sortie = std::process::Command::new("gh")
        .args(["api", endpoint])
        .output()
        .ok()?;
    if !sortie.status.success() {
        return None;
    }
    let texte = String::from_utf8(sortie.stdout).ok()?;
    let t = texte.trim_start();
    if !(t.starts_with('[') || t.starts_with('{')) {
        return None;
    }
    Some(texte)
}

/// Lit du JSON depuis `file://` (tests), `gh` (si disponible) ou `curl`.
fn requeter_json(url: &str, endpoint: &str) -> Result<String, String> {
    if let Some(chemin) = url.strip_prefix("file://") {
        return fs::read_to_string(chemin).map_err(|e| format!("lecture {chemin} : {e}"));
    }
    if url == API_RELEASES {
        if let Some(texte) = essayer_gh(endpoint) {
            return Ok(texte);
        }
    }
    let sortie = curl_sans_fenetre()
        .args(["-sSL", "--fail", "-H", "Accept: application/vnd.github+json", url])
        .output()
        .map_err(|e| format!("curl indisponible : {e}"))?;
    if !sortie.status.success() {
        return Err(format!("API GitHub inaccessible ({url}) : code {}", sortie.status));
    }
    String::from_utf8(sortie.stdout).map_err(|e| format!("réponse non UTF-8 : {e}"))
}

/// Découpe un tableau JSON en objets de PREMIER NIVEAU, en ignorant les
/// accolades à l'intérieur des chaînes (un titre peut contenir `{...}`).
fn objets_racine(json: &str) -> Vec<String> {
    let mut sortie = Vec::new();
    let mut profondeur = 0i32;
    let mut debut = None;
    let mut chaine = false;
    let mut echap = false;
    for (i, c) in json.char_indices() {
        if chaine {
            if echap {
                echap = false;
            } else if c == '\\' {
                echap = true;
            } else if c == '"' {
                chaine = false;
            }
            continue;
        }
        match c {
            '"' => chaine = true,
            '{' => {
                if profondeur == 0 {
                    debut = Some(i);
                }
                profondeur += 1;
            }
            '}' => {
                profondeur -= 1;
                if profondeur == 0 {
                    if let Some(d) = debut {
                        sortie.push(json[d..=i].to_string());
                    }
                    debut = None;
                }
            }
            _ => {}
        }
    }
    sortie
}

/// Lit un champ de PREMIER NIVEAU d'un objet JSON. Les champs des objets
/// imbriqués (`author`, `assets`) sont ignorés : sans cela, `"name"` de
/// l'auteur pourrait masquer le titre de la release.
fn champ_racine(objet: &str, cle: &str) -> Option<String> {
    let motif = format!("\"{cle}\"");
    let octets = objet.as_bytes();
    let mut profondeur = 0i32;
    let mut chaine = false;
    let mut echap = false;
    let mut i = 0usize;
    while i < octets.len() {
        let c = octets[i];
        if chaine {
            if echap {
                echap = false;
            } else if c == b'\\' {
                echap = true;
            } else if c == b'"' {
                chaine = false;
            }
            i += 1;
            continue;
        }
        if c == b'"' {
            if profondeur == 1 && objet[i..].starts_with(&motif) {
                let mut j = i + motif.len();
                while j < octets.len() && (octets[j] as char).is_ascii_whitespace() {
                    j += 1;
                }
                if j < octets.len() && octets[j] == b':' {
                    j += 1;
                    while j < octets.len() && (octets[j] as char).is_ascii_whitespace() {
                        j += 1;
                    }
                    if j < octets.len() && octets[j] == b'"' {
                        let debut = j + 1;
                        let mut k = debut;
                        let mut ech = false;
                        while k < octets.len() {
                            let ch = octets[k];
                            if ech {
                                ech = false;
                            } else if ch == b'\\' {
                                ech = true;
                            } else if ch == b'"' {
                                break;
                            }
                            k += 1;
                        }
                        if k <= octets.len() {
                            return Some(objet[debut..k].to_string());
                        }
                    }
                }
            }
            chaine = true;
        } else if c == b'{' || c == b'[' {
            profondeur += 1;
        } else if c == b'}' || c == b']' {
            profondeur -= 1;
        }
        i += 1;
    }
    None
}

/// Une release Windows visible par le launcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionDistante {
    pub tag: String,
    pub version: String,
    pub titre: String,
    /// `Rose-<v>-full.nupkg`, vide si la release n'en a pas.
    pub nupkg: String,
    /// Vrai si le launcher sait installer cette version (manifeste + nupkg).
    pub installable: bool,
    /// Ancien installeur `Rose-Setup-<v>.exe`, vide si absent.
    pub legacy: String,
}

/// Analyse pure de la réponse de l'API GitHub (séparée du réseau pour être
/// testable). La plus récente est placée en premier.
fn analyser_releases(json: &str) -> Vec<VersionDistante> {
    let mut versions: Vec<VersionDistante> = Vec::new();
    for objet in objets_racine(json) {
        let tag = match champ_racine(&objet, "tag_name") {
            Some(t) if !t.is_empty() => t,
            _ => continue,
        };
        let version = version_depuis_tag(&tag);
        // La présence du nom de fichier dans l'objet (qui contient son
        // tableau `assets`) suffit : on n'a pas besoin d'un parseur complet.
        let nupkg = format!("Rose-{version}-full.nupkg");
        let legacy = format!("Rose-Setup-{version}.exe");
        let a_nupkg = objet.contains(&nupkg);
        let a_legacy = objet.contains(&legacy);
        if !a_nupkg && !a_legacy {
            continue; // release sans installateur Windows
        }
        versions.push(VersionDistante {
            titre: champ_racine(&objet, "name").unwrap_or_else(|| tag.clone()),
            tag,
            version,
            nupkg: if a_nupkg { nupkg } else { String::new() },
            installable: a_nupkg,
            legacy: if a_legacy { legacy } else { String::new() },
        });
    }
    versions.sort_by(|a, b| comparer_versions(&b.version, &a.version));
    versions.dedup_by(|a, b| a.version == b.version);
    versions
}

/// Liste DYNAMIQUEMENT les versions disponibles (lecture seule).
///
/// `Ok(vec)` même si certaines releases n'ont pas de paquet Velopack : elles
/// sont marquées `installable: false` avec un `legacy` renseigné. `Err` only
/// si l'API est inaccessible ou ne renvoie aucune release Windows.
pub fn lister_versions() -> Result<Vec<VersionDistante>, String> {
    let url = api_url();
    let endpoint = format!("repos/{}/releases?per_page=100", depot());
    let json = requeter_json(&url, &endpoint)?;
    let versions = analyser_releases(&json);
    if versions.is_empty() {
        return Err("aucune release Windows installable trouvée dans l'API GitHub".into());
    }
    Ok(versions)
}

/// Télécharge un fichier vers `cible` : copie locale pour `file://` (tests),
/// sinon `curl -o` (sans fenêtre de console sous Windows).
fn telecharger_fichier(url: &str, cible: &Path) -> Result<(), String> {
    if let Some(chemin) = url.strip_prefix("file://") {
        fs::copy(chemin, cible).map_err(|e| format!("copie {chemin} : {e}"))?;
        return Ok(());
    }
    let statut = curl_sans_fenetre()
        .args(["-sSL", "--fail", "-o"])
        .arg(cible)
        .arg(url)
        .status()
        .map_err(|e| format!("curl indisponible : {e}"))?;
    if !statut.success() {
        return Err(format!("téléchargement de {url} impossible (code {statut})"));
    }
    Ok(())
}

/// Versions déjà sur disque (`app-<v>`), la plus récente d'abord.
pub fn versions_installees() -> Vec<String> {
    let mut versions = Vec::new();
    if let Some(racine) = dossier_racine() {
        if let Ok(entrees) = fs::read_dir(&racine) {
            for entree in entrees.flatten() {
                let nom = entree.file_name().to_string_lossy().into_owned();
                if entree.path().is_dir() {
                    if let Some(v) = nom.strip_prefix("app-") {
                        versions.push(v.to_string());
                    }
                }
            }
        }
    }
    versions.sort_by(|a, b| comparer_versions(b, a));
    versions
}

/// Versions lues dans le cache local `releases.win.json` (mode hors ligne).
pub fn versions_cachees() -> Vec<String> {
    let Some(racine) = dossier_racine() else {
        return Vec::new();
    };
    let Ok(texte) = fs::read_to_string(racine.join(NOM_CACHE)) else {
        return Vec::new();
    };
    let Ok(manifeste) = Manifeste::analyser(&texte) else {
        return Vec::new();
    };
    let mut versions: Vec<String> = manifeste
        .paquets
        .into_iter()
        .filter(|p| p.complet)
        .map(|p| p.version)
        .collect();
    versions.sort_by(|a, b| comparer_versions(b, a));
    versions.dedup();
    versions
}

/// Fait pointer `current\` sur `app-<tag>`. NE TÉLÉCHARGE RIEN : la version
/// doit déjà être installée. Rend la version sélectionnée.
pub fn selectionner_version(tag: &str) -> Result<String, String> {
    let version = version_depuis_tag(tag);
    let racine = dossier_racine().ok_or("LOCALAPPDATA indisponible")?;
    let dossier = racine.join(format!("app-{version}"));
    if !dossier.is_dir() {
        return Err(format!(
            "version {version} absente de {} — lancez d'abord le téléchargement",
            racine.display()
        ));
    }
    fs::write(racine.join("current"), format!("app-{version}"))
        .map_err(|e| format!("écriture de current : {e}"))?;
    let exe = racine
        .join("current")
        .join(if cfg!(windows) { "rose.exe" } else { "rose" });
    if let Err(e) = crate::assoc_windows::appliquer_silencieuse(Some(&exe)) {
        eprintln!("Association .rose après sélection : {e}");
    }
    Ok(version)
}

/// Télécharge UNIQUEMENT la version demandée, vérifie son SHA-256, l'extrait
/// dans `app-<version>\` et fait pointer `current\`. Le manifeste est gardé
/// en cache local (`releases.win.json`). Aucun moteur n'est embarqué dans le
/// launcher : c'est le seul téléchargement nécessaire.
pub fn telecharger_version(tag: &str) -> Result<String, String> {
    let version = version_depuis_tag(tag);
    let racine = dossier_racine().ok_or("LOCALAPPDATA indisponible")?;

    // Le manifeste est propre à CE tag (celui de /latest ne connaît que la
    // dernière version). On le met en cache pour le mode hors ligne.
    let url_manifeste = url_asset(tag, NOM_CACHE);
    let json = lire_source(&url_manifeste).map_err(|e| {
        format!(
            "release {tag} : manifeste Velopack inaccessible ({e}) — \
             si c'est une ancienne release, voir telecharger_legacy()"
        )
    })?;
    let cache = racine.join(NOM_CACHE);
    if let Err(e) = fs::write(&cache, &json) {
        eprintln!("Cache {NOM_CACHE} non écrit : {e}");
    }
    let manifeste = Manifeste::analyser(&json)?;
    let paquet = manifeste
        .paquets
        .iter()
        .find(|p| p.complet && comparer_versions(&p.version, &version) == std::cmp::Ordering::Equal)
        .ok_or_else(|| {
            format!("release {tag} : aucun paquet complet pour {version} dans le manifeste")
        })?
        .clone();

    let dossier_paquets = racine.join("packages");
    fs::create_dir_all(&dossier_paquets)
        .map_err(|e| format!("création {} : {e}", dossier_paquets.display()))?;
    let cible = dossier_paquets.join(&paquet.nom);
    telecharger_fichier(&url_asset(tag, &paquet.nom), &cible)?;

    if paquet.sha256.is_empty() {
        let _ = fs::remove_file(&cible);
        return Err(format!(
            "release {tag} : SHA-256 absent du manifeste, téléchargement refusé"
        ));
    }
    let empreinte = sha256_fichier(&cible)?;
    if !empreinte.eq_ignore_ascii_case(&paquet.sha256) {
        let _ = fs::remove_file(&cible);
        return Err(format!(
            "SHA-256 invalide pour {} (attendu {}, obtenu {})",
            paquet.nom, paquet.sha256, empreinte
        ));
    }

    let destination = racine.join(format!("app-{}", paquet.version));
    fs::create_dir_all(&destination)
        .map_err(|e| format!("création {} : {e}", destination.display()))?;
    extraire_net45(&cible, &destination)?;
    let _ = fs::remove_file(&cible);
    selectionner_version(&paquet.version)
}

/// Anciennes releases (ex. v0.1.3) : pas de manifeste Velopack, mais un
/// installeur autonome `Rose-Setup-<v>.exe` ET un `SHA256SUMS`. On télécharge
/// et VÉRIFIE l'exe ; son EXÉCUTION (silencieuse) dépend du format de cet
/// installeur et reste à valider sur Windows — rien n'est lancé ici.
pub fn telecharger_legacy(tag: &str) -> Result<PathBuf, String> {
    let version = version_depuis_tag(tag);
    let exe = format!("Rose-Setup-{version}.exe");
    let racine = dossier_racine().ok_or("LOCALAPPDATA indisponible")?;
    let sommes = lire_source(&url_asset(tag, "SHA256SUMS"))?;
    let attendu = sommes
        .lines()
        .find_map(|ligne| {
            let (h, n) = ligne.split_once(' ')?;
            if n.trim() == exe {
                Some(h.trim().to_string())
            } else {
                None
            }
        })
        .ok_or_else(|| format!("{exe} absent de SHA256SUMS"))?;
    let dossier_paquets = racine.join("packages");
    fs::create_dir_all(&dossier_paquets)
        .map_err(|e| format!("création {} : {e}", dossier_paquets.display()))?;
    let cible = dossier_paquets.join(&exe);
    telecharger_fichier(&url_asset(tag, &exe), &cible)?;
    let empreinte = sha256_fichier(&cible)?;
    if !empreinte.eq_ignore_ascii_case(&attendu) {
        let _ = fs::remove_file(&cible);
        return Err(format!(
            "SHA-256 invalide pour {exe} (attendu {attendu}, obtenu {empreinte})"
        ));
    }
    Ok(cible)
}

/// Décrit ce que le launcher ferait, SANS télécharger ni installer quoi que
/// ce soit. `tag = None` = afficher la liste et laisser choisir ; `Some(tag)`
/// = installation ciblée. La liste distante est lue (lecture seule) pour être
/// exacte, mais aucun fichier n'est modifié.
pub fn dry_run(tag: Option<&str>) -> Result<String, String> {
    let racine = dossier_racine();
    let mut lignes: Vec<String> = Vec::new();
    lignes.push("[dry-run] aucun fichier ne sera modifié".to_string());
    lignes.push(format!(
        "[dry-run] racine      : {}",
        racine
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "(indéfinie — poser LOCALAPPDATA ou ROSE_VELOPACK_ROOT)".into())
    ));
    lignes.push(format!(
        "[dry-run] installée   : {}",
        version_installee().unwrap_or_else(|| "(aucune)".into())
    ));
    let sur_disque = versions_installees();
    lignes.push(format!(
        "[dry-run] sur disque  : {}",
        if sur_disque.is_empty() {
            "(aucune)".into()
        } else {
            sur_disque.join(", ")
        }
    ));
    let en_cache = versions_cachees();
    lignes.push(format!(
        "[dry-run] cache      : {}",
        if en_cache.is_empty() {
            "(vide)".into()
        } else {
            en_cache.join(", ")
        }
    ));

    let versions = lister_versions().unwrap_or_default();
    if versions.is_empty() {
        lignes.push(
            "[dry-run] API GitHub : inaccessible — un lancement réel échouera sans réseau".into(),
        );
    } else {
        lignes.push(format!("[dry-run] versions distantes ({}) :", versions.len()));
        for v in &versions {
            lignes.push(format!(
                "[dry-run]   {:<8} {}",
                v.tag,
                if v.installable {
                    "paquet complet disponible"
                } else {
                    "ancien installeur (legacy)"
                }
            ));
        }
    }
    match tag {
        None => lignes.push(
            "[dry-run] action     : afficher la liste, laisser l'utilisateur choisir, puis telecharger_version(choix)".into(),
        ),
        Some(t) => {
            let v = version_depuis_tag(t);
            match versions.iter().find(|x| x.version == v) {
                Some(x) if x.installable => lignes.push(format!(
                    "[dry-run] action     : telecharger_version({t}) → SHA-256 → app-{v} → current"
                )),
                Some(x) => lignes.push(format!(
                    "[dry-run] action     : telecharger_legacy({t}) → SHA-256 via SHA256SUMS ({}) — exécution Windows à valider",
                    x.legacy
                )),
                None => lignes.push(format!("[dry-run] action     : '{t}' inconnu — refuser")),
            }
        }
    }
    Ok(lignes.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_comparees() {
        assert_eq!(comparer_versions("0.1.4", "0.1.3"), std::cmp::Ordering::Greater);
        assert_eq!(comparer_versions("0.1", "0.1.0"), std::cmp::Ordering::Equal);
        assert_eq!(comparer_versions("1.0.0", "0.9.9"), std::cmp::Ordering::Greater);
        // un suffixe non numérique ne doit pas s'effondrer sur 0
        assert_eq!(comparer_versions("0.1.4-beta", "0.1.3"), std::cmp::Ordering::Greater);
    }

    #[test]
    fn manifeste_analyse() {
        let json = r#"[
          {"PackageId":"Rose-0.1.3-full.nupkg","Version":"0.1.3","SHA256":"aa","SHA1":"11","Size":"100","Type":"Full"},
          {"PackageId":"Rose-0.1.4-full.nupkg","Version":"0.1.4","SHA256":"bb","SHA1":"22","Size":"200","Type":"Full"},
          {"PackageId":"Rose-0.1.4-delta.nupkg","Version":"0.1.4","SHA256":"cc","SHA1":"33","Size":"50","Type":"Delta"}
        ]"#;
        let m = Manifeste::analyser(json).expect("manifeste lisible");
        assert_eq!(m.paquets.len(), 3);
        let p = m.paquet_a_jour("0.1.3").cloned().expect("paquet 0.1.4 attendu");
        assert_eq!(p.version, "0.1.4");
        assert_eq!(p.nom, "Rose-0.1.4-full.nupkg");
        assert!(p.complet);
        assert!(m.paquet_a_jour("0.1.4").is_none());
    }

    #[test]
    fn launcher_detecte() {
        let args = vec!["rose".to_string(), ARG_MIS_A_JOUR.to_string()];
        assert!(is_update_launcher(&args));
        let args = vec!["rose".to_string(), "run".to_string(), "a.rose".to_string()];
        assert!(!is_update_launcher(&args));
        assert_eq!(run_update_launcher(&["rose".into(), ARG_OBSOLETE.into()]), 0);
    }

    /// Fausse réponse de l'API GitHub : 4 releases, dont une sans
    /// installateur Windows (v0.1.2) et une en installeur legacy (v0.1.3).
    /// Le titre de v0.1.5 contient des accolades pour éprouver
    /// `objets_racine` et le champ `author` porte un `name` piège.
    const API_FIXTURE: &str = r#"[
      {"author":{"login":"Stef302","name":"Piège"},"tag_name":"v0.1.5",
       "name":"Rose {v0.1.5} — installeur Windows refait",
       "assets":[{"name":"Rose-0.1.5-full.nupkg","size":8213857},
                 {"name":"releases.win.json","size":402}]},
      {"author":{"login":"Stef302","name":"Piège"},"tag_name":"v0.1.4",
       "name":"Rose v0.1.4",
       "assets":[{"name":"Rose-0.1.4-full.nupkg"},{"name":"releases.win.json"}]},
      {"author":{"login":"Stef302"},"tag_name":"v0.1.3","name":"Rose v0.1.3",
       "assets":[{"name":"Rose-Setup-0.1.3.exe"}]},
      {"author":{"login":"Stef302"},"tag_name":"v0.1.2","name":"Rose v0.1.2","assets":[]}
    ]"#;

    #[test]
    fn launcher_etiquettes() {
        assert_eq!(version_depuis_tag("v0.1.5"), "0.1.5");
        assert_eq!(version_depuis_tag("0.1.3"), "0.1.3");
        assert_eq!(version_depuis_tag("version-test"), "version-test");
    }

    #[test]
    fn launcher_liste_versions() {
        let versions = analyser_releases(API_FIXTURE);
        assert_eq!(versions.len(), 3);
        assert_eq!(versions[0].tag, "v0.1.5");
        assert_eq!(versions[0].version, "0.1.5");
        assert_eq!(versions[0].titre, "Rose {v0.1.5} — installeur Windows refait");
        assert!(versions[0].installable);
        assert_eq!(versions[0].nupkg, "Rose-0.1.5-full.nupkg");
        assert_eq!(versions[1].tag, "v0.1.4");
        assert!(!versions[2].installable);
        assert_eq!(versions[2].legacy, "Rose-Setup-0.1.3.exe");
        // v0.1.2 (sans artefact Windows) est écarté
        assert!(versions.iter().all(|v| v.tag != "v0.1.2"));
    }

    /// ZIP minimal en méthode « stored » (0) : `entrees_zip` lit les
    /// en-têtes locaux et ne vérifie pas le CRC, un seul en-tête suffit.
    fn zip_stored(nom: &str, donnees: &[u8]) -> Vec<u8> {
        let mut z = Vec::new();
        z.extend_from_slice(b"PK\x03\x04");
        z.extend_from_slice(&20u16.to_le_bytes()); // version nécessaire
        z.extend_from_slice(&0u16.to_le_bytes());  // drapeaux
        z.extend_from_slice(&0u16.to_le_bytes());  // méthode 0 = stored
        z.extend_from_slice(&0u16.to_le_bytes());  // heure
        z.extend_from_slice(&0u16.to_le_bytes());  // date
        z.extend_from_slice(&0u32.to_le_bytes());  // CRC (non vérifié ici)
        z.extend_from_slice(&(donnees.len() as u32).to_le_bytes());
        z.extend_from_slice(&(donnees.len() as u32).to_le_bytes());
        z.extend_from_slice(&(nom.len() as u16).to_le_bytes());
        z.extend_from_slice(&0u16.to_le_bytes());  // champ extra
        z.extend_from_slice(nom.as_bytes());
        z.extend_from_slice(donnees);
        z
    }

    /// Test RÉEL en lecture seule (ignoré par défaut pour que `cargo test`
    /// reste hors réseau). Commande :
    /// `cargo test --release launcher_api_reelle -- --ignored --nocapture`
    /// `--dry-run` réel : imprime le plan complet, sans rien modifier.
    #[test]
    #[ignore = "accès réseau à api.github.com, lecture seule"]
    fn launcher_dry_run_reel() {
        println!("{}", dry_run(None).expect("plan général"));
        println!("{}", dry_run(Some("v0.1.5")).expect("plan v0.1.5"));
        println!("{}", dry_run(Some("v0.1.3")).expect("plan v0.1.3"));
    }

    #[test]
    #[ignore = "accès réseau à api.github.com, lecture seule"]
    fn launcher_api_reelle() {
        let versions = lister_versions().expect("API GitHub accessible en lecture seule");
        assert!(!versions.is_empty());
        for v in &versions {
            println!(
                "TAG REEL {} | version={} | installable={} | nupkg={} | legacy={}",
                v.tag, v.version, v.installable,
                if v.nupkg.is_empty() { "-" } else { &v.nupkg },
                if v.legacy.is_empty() { "-" } else { &v.legacy }
            );
        }
        assert!(versions.iter().any(|v| v.version == "0.1.5"));
    }

    /// Installation RÉELLE (ignorée par défaut) : pointe `ROSE_LAUNCHER_BASE`
    /// vers un miroir LOCAL des artefacts v0.1.5 déjà téléchargés, avec
    /// `ROSE_VELOPACK_ROOT` dans /tmp. Aucun fichier du dépôt n'est touché.
    #[test]
    #[ignore = "nécessite ROSE_VELOPACK_ROOT + ROSE_LAUNCHER_BASE (miroir local)"]
    fn launcher_installation_reelle_locale() {
        let racine = dossier_racine().expect("ROSE_VELOPACK_ROOT requis");
        let version = telecharger_version("v0.1.5").expect("installation v0.1.5 réelle");
        assert_eq!(version, "0.1.5");
        assert_eq!(
            fs::read_to_string(racine.join("current")).unwrap().trim(),
            "app-0.1.5"
        );
        let nb = fs::read_dir(racine.join("app-0.1.5"))
            .expect("app-0.1.5 présent")
            .count();
        println!("INSTALLATION REELLE v{version} : {nb} entrées dans {}", racine.display());
        assert!(nb > 0);
    }

    #[test]
    fn launcher_selection_et_cache() {
        // Un SEUL test pose les variables d'environnement : les tests Rust
        // tournant en parallèle, on évite ainsi toute interférence.
        let racine = std::env::temp_dir().join(format!(
            "rose_launcher_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let _ = fs::create_dir_all(racine.join("app-0.1.4"));
        let _ = fs::create_dir_all(racine.join("app-0.1.5"));
        let _ = fs::write(
            racine.join(NOM_CACHE),
            r#"[{"PackageId":"x","Version":"0.1.5","SHA256":"aa","Type":"Full"}]"#,
        );
        let _ = fs::write(racine.join("api.json"), API_FIXTURE);

        // Édition 2024 : set_var/remove_var sont `unsafe` (concurrence
        // inter-threads). Un seul test les pose, les autres ne les lisent pas.
        unsafe {
            std::env::set_var("ROSE_VELOPACK_ROOT", &racine);
            std::env::set_var(
                "ROSE_LAUNCHER_API",
                format!("file://{}", racine.join("api.json").display()),
            );
        }

        assert_eq!(selectionner_version("v0.1.4").unwrap(), "0.1.4");
        assert_eq!(
            fs::read_to_string(racine.join("current")).unwrap().trim(),
            "app-0.1.4"
        );
        assert_eq!(version_installee().as_deref(), Some("0.1.4"));
        assert_eq!(versions_installees(), vec!["0.1.5".to_string(), "0.1.4".to_string()]);
        assert_eq!(versions_cachees(), vec!["0.1.5".to_string()]);
        assert!(selectionner_version("v0.1.3").is_err()); // pas sur disque

        let plan = dry_run(Some("v0.1.5")).unwrap();
        assert!(plan.contains("telecharger_version(v0.1.5)"));
        assert!(plan.contains("Rose {v0.1.5}") || plan.contains("v0.1.5"));
        let plan_legacy = dry_run(Some("v0.1.3")).unwrap();
        assert!(plan_legacy.contains("telecharger_legacy(v0.1.3)"));
        let plan_liste = dry_run(None).unwrap();
        assert!(plan_liste.contains("versions distantes (3)"));

        // ── parcours COMPLET, hors ligne : manifeste → téléchargement →
        // SHA-256 → extraction → `current`. La source `file://` tient lieu
        // de GitHub ; le moteur factice prouve qu'aucun moteur n'est embarqué.
        let artefacts = racine.join("artefacts").join("v0.1.6");
        let _ = fs::create_dir_all(&artefacts);
        let moteur_factice = b"MOTEUR ROSE FACTICE - telecharge, pas embarque";
        let paquet = zip_stored("lib/net45/rose.exe", moteur_factice);
        let h = crate::sha256::sha256_hex(&paquet);
        let manifeste = format!(
            r#"[{{"PackageId":"Rose-0.1.6-full.nupkg","Version":"0.1.6","SHA256":"{h}","SHA1":"00","Size":"{}","Type":"Full"}}]"#,
            paquet.len()
        );
        let _ = fs::write(artefacts.join("releases.win.json"), &manifeste);
        let _ = fs::write(artefacts.join("Rose-0.1.6-full.nupkg"), &paquet);
        unsafe {
            std::env::set_var(
                "ROSE_LAUNCHER_BASE",
                format!("file://{}", racine.join("artefacts").display()),
            );
        }
        assert_eq!(telecharger_version("v0.1.6").unwrap(), "0.1.6");
        assert_eq!(
            fs::read(racine.join("app-0.1.6").join("rose.exe")).unwrap(),
            moteur_factice
        );
        assert_eq!(
            fs::read_to_string(racine.join("current")).unwrap().trim(),
            "app-0.1.6"
        );
        // Le paquet téléchargé est supprimé après extraction.
        assert!(!racine.join("packages").join("Rose-0.1.6-full.nupkg").exists());

        // ── SHA-256 erroné : REFUS, aucune corruption de l'installation.
        let mauvais = manifeste.replace(&h, &"0".repeat(64));
        let _ = fs::write(artefacts.join("releases.win.json"), mauvais);
        let erreur = telecharger_version("v0.1.6").unwrap_err();
        assert!(erreur.contains("SHA-256 invalide"), "{erreur}");
        assert_eq!(
            fs::read_to_string(racine.join("current")).unwrap().trim(),
            "app-0.1.6"
        );

        unsafe {
            std::env::remove_var("ROSE_LAUNCHER_BASE");
            std::env::remove_var("ROSE_LAUNCHER_API");
            std::env::remove_var("ROSE_VELOPACK_ROOT");
        }
        let _ = fs::remove_dir_all(&racine);
    }

}
