use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const MANIFEST: &str = include_str!("../../ui/installer.json");

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct InstallerManifest {
    product_name: String,
    version: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InstallerInfo {
    product_name: String,
    version: String,
    default_install_dir: String,
}

fn read_manifest() -> Result<InstallerManifest, String> {
    let manifest: InstallerManifest = serde_json::from_str(MANIFEST)
        .map_err(|error| format!("Manifest d'installation invalide : {error}"))?;

    validate_product_name(&manifest.product_name)?;
    if manifest.version.trim().is_empty() {
        return Err("La version du produit ne peut pas etre vide.".into());
    }

    Ok(manifest)
}

fn validate_product_name(name: &str) -> Result<(), String> {
    let invalid = name.trim().is_empty()
        || name == "."
        || name == ".."
        || name.chars().any(|character| {
            matches!(
                character,
                '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
            )
        });

    if invalid {
        return Err("Le nom du produit contient des caracteres invalides.".into());
    }
    Ok(())
}

#[tauri::command]
fn installer_info() -> Result<InstallerInfo, String> {
    let manifest = read_manifest()?;
    let local_app_data =
        env::var_os("LOCALAPPDATA").ok_or("La variable LOCALAPPDATA est indisponible.")?;
    let default_install_dir = PathBuf::from(local_app_data)
        .join("Programs")
        .join(&manifest.product_name);

    Ok(InstallerInfo {
        product_name: manifest.product_name,
        version: manifest.version,
        default_install_dir: default_install_dir.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
fn install_app(destination: String) -> Result<String, String> {
    let target = PathBuf::from(destination.trim());
    if !target.is_absolute() || target.file_name().is_none() {
        return Err("Choisissez un chemin absolu qui n'est pas la racine d'un lecteur.".into());
    }

    let executable = env::current_exe()
        .map_err(|error| format!("Impossible de localiser l'installeur : {error}"))?;
    let payload = executable
        .parent()
        .ok_or("Impossible de localiser le dossier de l'installeur.")?
        .join("payload");
    let payload = fs::canonicalize(&payload)
        .map_err(|error| format!("Dossier payload introuvable : {error}"))?;

    fs::create_dir_all(&target)
        .map_err(|error| format!("Impossible de creer le dossier cible : {error}"))?;
    let target = fs::canonicalize(&target)
        .map_err(|error| format!("Impossible de resoudre le dossier cible : {error}"))?;

    if target.starts_with(&payload) || payload.starts_with(&target) {
        return Err("Le dossier cible ne peut pas chevaucher le dossier payload.".into());
    }

    let copied = copy_payload(&payload, &target)?;
    if copied == 0 {
        return Err("Le dossier payload ne contient aucun fichier a installer.".into());
    }

    Ok(format!(
        "{copied} fichier(s) installe(s) dans {}",
        target.display()
    ))
}

fn copy_payload(source: &Path, destination: &Path) -> Result<usize, String> {
    let entries = fs::read_dir(source)
        .map_err(|error| format!("Impossible de lire le dossier payload : {error}"))?;
    let mut copied = 0;

    for entry in entries {
        let entry = entry.map_err(|error| format!("Erreur de lecture du payload : {error}"))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("Impossible d'inspecter un element du payload : {error}"))?;
        let target = destination.join(entry.file_name());

        if file_type.is_symlink() {
            return Err(format!(
                "Les liens symboliques ne sont pas autorises dans payload : {}",
                entry.path().display()
            ));
        }
        if file_type.is_dir() {
            fs::create_dir_all(&target).map_err(|error| {
                format!(
                    "Impossible de creer le dossier {} : {error}",
                    target.display()
                )
            })?;
            copied += copy_payload(&entry.path(), &target)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), &target).map_err(|error| {
                format!("Impossible de copier {} : {error}", entry.path().display())
            })?;
            copied += 1;
        } else {
            return Err(format!(
                "Type de fichier non pris en charge dans payload : {}",
                entry.path().display()
            ));
        }
    }

    Ok(copied)
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![installer_info, install_app])
        .run(tauri::generate_context!())
        .expect("Erreur au demarrage de l'installeur Tauri");
}
