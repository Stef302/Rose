//! Diagnostic hors ligne de la source PRIMAIRE de découverte des releases.
//!
//! ```bash
//! ROSE_LAUNCHER_SITE_MANIFESTE=file:///chemin/vers/releases.json \
//!   cargo run --release --example verifier_source_update --manifest-path rose/Cargo.toml
//! ```
//!
//! N'imprime QUE ce que `decouvrir_versions_update()` a réellement lu : la
//! source annoncée, le nombre de versions et la plus récente. Aucun télé-
//! chargement, aucune écriture, aucun appel à `gh`.

fn main() {
    match rose::velopack::decouvrir_versions_update() {
        Ok(resultat) => {
            println!("source={}", resultat.source.libelle());
            println!("versions={}", resultat.versions.len());
            for version in resultat.versions.iter().take(12) {
                println!("  {} {} installable={}", version.version, version.tag,
                         version.installable);
            }
            match resultat.versions.first() {
                Some(derniere) => println!("derniere={}", derniere.version),
                None => println!("derniere=AUCUNE"),
            }
        }
        Err(erreur) => {
            eprintln!("echec={erreur}");
            std::process::exit(1);
        }
    }
}
