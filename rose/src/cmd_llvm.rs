//! Commandes de compilation de la ligne de commande :
//! `rose llvm` (transpilation vers C), `rose orc` / `rose aot`
//! (exécutable machine natif) et `rose link` (vérification d'un produit).
//!
//! Le chemin natif utilise UNIQUEMENT les modules intégrés au binaire :
//!   front (analyse) → rir (abaissement) → natif_emetteur (octets machine)
//!   → link_natif (ELF64 / PE64 exécutable).
//! Aucun programme tiers, aucun téléchargement, aucun réseau.
//!
//! `rose llvm` garde sa sémantique historique : le transpileur intégré
//! (`src/transpile.rs`) produit du code C. Cette partie n'est pas modifiée.
//!
//! Licence MIT.

use std::path::Path;

/// Conseil unique quand le programme ne peut pas devenir un exécutable natif.
/// Aucun nom d'outil externe n'apparaît dans les messages utilisateur.
const CONSEIL_COMP: &str = "utilisez `rose comp`, qui produit un exécutable autonome";

/// Exécute l'action demandée. Ne rend jamais (termine le processus).
pub fn execute(action: &str, argv: &[String]) -> ! {
    match action {
        "llvm" => llvm_codegen(argv),
        "orc" | "aot" => orc(argv),
        "link" => link(argv),
        _ => {
            eprintln!("rose: commande inconnue : {}", action);
            std::process::exit(1);
        }
    }
}

/// `rose llvm prog.rose [-o sortie.c] [--exe]` : transpilation vers C par le
/// transpileur intégré (`rose/src/transpile.rs`), sans dépendance LLVM ni
/// Python. Avec `--exe`, un compilateur C local est utilisé si disponible ;
/// sinon le fichier C est rendu et l'absence est expliquée.
fn llvm_codegen(argv: &[String]) -> ! {
    let mut sortie = String::new();
    let mut fichier = String::new();
    let mut veut_exe = false;
    let mut i = 2;
    while i < argv.len() {
        if argv[i] == "--exe" || argv[i] == "--compil" {
            veut_exe = true; i += 1;
        } else if (argv[i] == "-o" || argv[i] == "--sortie") && i + 1 < argv.len() {
            sortie = argv[i + 1].clone(); i += 2;
        } else if !argv[i].starts_with('-') { fichier = argv[i].clone(); i += 1; }
        else { i += 1; }
    }
    if fichier.is_empty() { eprintln!("rose: quel programme compiler ?"); std::process::exit(1); }
    if sortie.is_empty() {
        sortie = fichier.rsplit_once(['/', '\\']).map(|(_, f)| f.to_string()).unwrap_or(fichier.clone());
        if let Some(p) = sortie.rfind(".rose") { sortie.truncate(p); }
        if sortie.is_empty() { sortie = "programme".into(); }
        sortie.push_str(".c");
    }
    // Même API que `rose transpile --cible c` : un seul transpileur, en Rust.
    let code = match crate::transpile::transpiler_fichier(&fichier, crate::transpile::Cible::C) {
        Ok(c) => c,
        Err(e) => { eprintln!("rose: transpilation impossible : {e}"); std::process::exit(1); }
    };
    if let Err(e) = std::fs::write(&sortie, code.as_bytes()) {
        eprintln!("rose: impossible d'écrire {sortie} : {e}"); std::process::exit(1);
    }
    if !veut_exe {
        println!("rose: {} produit (code C)", sortie);
        std::process::exit(0);
    }
    let exe = match sortie.strip_suffix(".c") {
        Some(p) => p.to_string(),
        None => format!("{}.exe", sortie),
    };
    match trouver_compilateur_c() {
        Some(compilateur) => {
            let statut = std::process::Command::new(compilateur)
                .arg("-O2").arg(&sortie).arg("-o").arg(&exe).arg("-lm").status();
            match statut {
                Ok(s) if s.success() => println!("rose: {} produit (exécutable)", exe),
                Ok(s) => {
                    eprintln!("rose: compilation C impossible (code {}) ; {} reste disponible", s, sortie);
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("rose: compilation C impossible ({e}) ; {} reste disponible", sortie);
                    std::process::exit(1);
                }
            }
        }
        None => println!("rose: aucun compilateur C disponible ; {} produit (code C)", sortie),
    }
    std::process::exit(0);
}

/// Cherche un compilateur C déjà installé (aucun téléchargement).
fn trouver_compilateur_c() -> Option<&'static str> {
    ["cc", "gcc", "clang"].into_iter().find(|nom| {
        std::process::Command::new(nom).arg("--version").output()
            .map(|s| s.status.success()).unwrap_or(false)
    })
}

/// Refuse clairement un programme hors du sous-ensemble machine, sans jamais
/// écrire un fichier qui ressemblerait à un exécutable mais ne le serait pas.
fn refuser(raison: &str) -> ! {
    eprintln!("rose: compilation native impossible : {}", raison);
    eprintln!("rose: {}", CONSEIL_COMP);
    std::process::exit(1);
}

/// Architecture et format de sortie déduits d'une cible éventuelle
/// (`--target`). Sans indication, on reste sur la machine courante.
fn cible(target: Option<&str>) -> (crate::natif_emetteur::Architecture, bool) {
    use crate::natif_emetteur::Architecture;
    let t = target.unwrap_or("");
    let arch = if t.contains("aarch64") || t.contains("arm64") {
        Architecture::Aarch64
    } else if t.contains("x86_64") || t.contains("amd64") {
        Architecture::X86_64
    } else {
        Architecture::native()
    };
    let windows = if t.contains("windows") || t.contains("msvc") || t.contains("mingw") {
        true
    } else if t.contains("linux") || t.contains("darwin") || t.contains("macos") {
        false
    } else {
        cfg!(windows)
    };
    (arch, windows)
}

/// Nom complet du produit : `.exe` est ajouté sur Windows quand il manque.
fn nom_executable(sortie: &str, windows: bool) -> String {
    if windows && !sortie.to_ascii_lowercase().ends_with(".exe") {
        format!("{}.exe", sortie)
    } else {
        sortie.to_string()
    }
}

/// `rose orc prog.rose [-o sortie]` — alias : `rose aot`.
///
/// Analyse, abaisse en RIR, émet les octets machine puis écrit DIRECTEMENT
/// un exécutable ELF64 ou PE64. La valeur rendue par le programme (son
/// dernier affichage) devient le code de retour du processus.
fn orc(argv: &[String]) -> ! {
    let mut sortie = String::new();
    let mut fichier = String::new();
    let mut target: Option<String> = None;
    let mut i = 2;
    while i < argv.len() {
        if (argv[i] == "-o" || argv[i] == "--sortie") && i + 1 < argv.len() {
            sortie = argv[i + 1].clone(); i += 2;
        } else if argv[i] == "--target" && i + 1 < argv.len() {
            target = Some(argv[i + 1].clone()); i += 2;
        } else if argv[i].starts_with("--target=") {
            target = Some(argv[i]["--target=".len()..].to_string()); i += 1;
        } else if argv[i].starts_with('-') {
            // Options historiques sans effet sur l'émetteur intégré
            // (niveaux d'optimisation, profilage, lien implicite).
            i += 1;
        } else { fichier = argv[i].clone(); i += 1; }
    }
    if fichier.is_empty() { eprintln!("rose: quel programme compiler ?"); std::process::exit(1); }
    if sortie.is_empty() {
        sortie = fichier.rsplit_once(['/', '\\']).map(|(_, f)| f.to_string()).unwrap_or(fichier.clone());
        if let Some(p) = sortie.rfind(".rose") { sortie.truncate(p); }
        if sortie.is_empty() { sortie = "programme".into(); }
    }
    let (arch, windows) = cible(target.as_deref());
    let sortie = nom_executable(&sortie, windows);

    let mut prog = match crate::front::analyser_fichier(&fichier) {
        Ok(p) => p,
        Err(e) => { eprintln!("rose: analyse impossible : {}", e); std::process::exit(1); }
    };
    if let Err(e) = crate::front::verifier(&mut prog) {
        eprintln!("rose: programme invalide : {}", e);
        std::process::exit(1);
    }

    // Le dernier `>> expression` du programme devient la valeur de retour
    // du code machine (le `retour` n'existe qu'à l'intérieur des fonctions).
    let corps = crate::rir::abaisser_principal_sortie(&prog.instructions)
        .or_else(|| crate::rir::abaisser_principal(&prog.instructions));
    let Some(corps) = corps else {
        refuser("la valeur finale du programme ne peut pas être calculée directement");
    };
    let rend_valeur = corps.ops.iter()
        .any(|op| matches!(op, crate::rir::Op::RetourAbstrait(_)));
    if !rend_valeur {
        refuser("le programme ne rend pas de valeur calculable directement");
    }
    if !crate::natif_emetteur::peut_emettre(&corps) {
        refuser("certaines opérations demandent le moteur d'exécution");
    }
    let emis = match crate::natif_emetteur::emettre(&corps, arch) {
        Ok(e) => e,
        Err(_) => refuser("certaines opérations ne peuvent pas être traduites en code machine"),
    };
    // Sans moteur embarqué, un appel ou un repli rendrait un résultat faux.
    if !emis.appels.is_empty() || !emis.fallbacks.is_empty() {
        refuser("ce programme fait appel à des fonctions ou à des valeurs qui demandent le moteur d'exécution");
    }

    let chemin = Path::new(&sortie);
    let ecriture = if windows {
        crate::link_natif::ecrire_pe_depuis_emitted(&emis, chemin)
    } else {
        crate::link_natif::ecrire_elf_depuis_emitted(&emis, chemin)
    };
    if let Err(e) = ecriture {
        eprintln!("rose: écriture de {} impossible : {}", sortie, e);
        std::process::exit(1);
    }
    println!("rose: {} produit (exécutable natif {}, code de retour = valeur du programme)",
             sortie, arch.nom());
    std::process::exit(0);
}

/// `rose link fichier [-o sortie]` : le produit de `rose aot` est déjà un
/// exécutable complet ; cette commande ne fait que le vérifier et, si demandé,
/// le copier sous un autre nom. Aucun fichier intermédiaire n'est nécessaire.
fn link(argv: &[String]) -> ! {
    let mut objet = String::new();
    let mut sortie = String::new();
    let mut i = 2;
    while i < argv.len() {
        if (argv[i] == "-o" || argv[i] == "--sortie") && i + 1 < argv.len() {
            sortie = argv[i + 1].clone(); i += 2;
        } else if !argv[i].starts_with('-') {
            objet = argv[i].clone(); i += 1;
        } else { i += 1; }
    }
    if objet.is_empty() { eprintln!("rose: quel exécutable vérifier ?"); std::process::exit(1); }
    let octets = match std::fs::read(&objet) {
        Ok(o) => o,
        Err(e) => { eprintln!("rose: lecture de {} impossible : {}", objet, e); std::process::exit(1); }
    };
    let est_elf = octets.starts_with(b"\x7fELF");
    let est_pe = octets.starts_with(b"MZ");
    if !est_elf && !est_pe {
        eprintln!("rose: {} n'est pas un exécutable natif", objet);
        eprintln!("rose: {}", CONSEIL_COMP);
        std::process::exit(1);
    }
    if sortie.is_empty() {
        println!("rose: {} est déjà un exécutable natif", objet);
        std::process::exit(0);
    }
    if let Err(e) = std::fs::write(&sortie, &octets) {
        eprintln!("rose: écriture de {} impossible : {}", sortie, e);
        std::process::exit(1);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&sortie, std::fs::Permissions::from_mode(0o755));
    }
    println!("rose: {} produit (exécutable natif)", sortie);
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cible_choisit_architecture_et_format() {
        use crate::natif_emetteur::Architecture;
        assert_eq!(cible(Some("x86_64-unknown-linux-gnu")).0, Architecture::X86_64);
        assert_eq!(cible(Some("aarch64-unknown-linux-gnu")).0, Architecture::Aarch64);
        assert!(cible(Some("x86_64-pc-windows-msvc")).1);
        assert!(!cible(Some("aarch64-unknown-linux-gnu")).1);
        assert_eq!(nom_executable("prog", true), "prog.exe");
        assert_eq!(nom_executable("prog.exe", true), "prog.exe");
        assert_eq!(nom_executable("prog", false), "prog");
    }

    #[test]
    fn dernier_affichage_devient_valeur_de_retour() {
        let prog = crate::front::analyser("n = 7\nr = n * 6\n>> r\n").unwrap();
        let corps = crate::rir::abaisser_principal_sortie(&prog.instructions).unwrap();
        assert!(matches!(corps.ops.last(), Some(crate::rir::Op::RetourAbstrait(_))));
        assert!(crate::natif_emetteur::peut_emettre(&corps));
        let emis = crate::natif_emetteur::emettre(&corps, crate::natif_emetteur::Architecture::native())
            .unwrap();
        assert!(emis.appels.is_empty() && emis.fallbacks.is_empty());
        let octets = crate::link_natif::elf_bytes(
            &emis.code, &crate::link_natif::Options::depuis_emitted(&emis)).unwrap();
        assert!(octets.starts_with(b"\x7fELF"));
    }

    #[test]
    fn appel_de_fonction_est_refuse_sans_faux_executable() {
        let prog = crate::front::analyser(
            "fn f(n: entier) -> entier { retour n * 6 }\n>> f(7)\n").unwrap();
        let corps = crate::rir::abaisser_principal_sortie(&prog.instructions)
            .or_else(|| crate::rir::abaisser_principal(&prog.instructions)).unwrap();
        assert!(!corps.ops.iter().any(|o| matches!(o, crate::rir::Op::RetourAbstrait(_))));
    }
}
