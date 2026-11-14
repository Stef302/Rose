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
    let sortie = std::process::Command::new("curl")
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
    let statut = std::process::Command::new("curl")
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

}
