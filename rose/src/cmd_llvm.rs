//! Commandes LLVM de la ligne de commande (`rose llvm`, `rose orc`,
//! `rose link`).
//!
//! Ce module est partagé par le binaire principal et par un helper AOT
//! éventuellement installé localement (aucun téléchargement automatique).
//!
//! Note : la commande `llvm` (transpilation vers C) utilise le transpileur
//! intégré (`src/transpile.rs`) : aucun processus externe, donc disponible
//! même dans le binaire léger. L'option `--exe` peut, si un compilateur C est
//! présent, produire un exécutable ; sinon le fichier C est rendu tel quel.
//! Seules `orc` et `link` exigent une chaîne AOT locale.
//! Aucun message utilisateur ne mentionne d'outil de compilation.
//!
//! Licence MIT.

/// Exécute l'action LLVM demandée. Ne rend jamais (termine le processus).
pub fn execute(action: &str, argv: &[String]) -> ! {
    match action {
        "llvm" => llvm_codegen(argv),
        "orc" => orc(argv),
        "link" => link(argv),
        _ => {
            eprintln!("rose: commande LLVM inconnue : {}", action);
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

/// `rose orc prog.rose -o sortie.ll` : compilation AOT par LLVM ORC.
fn orc(argv: &[String]) -> ! {
    #[cfg(not(feature = "llvm"))]
    {
        let _ = argv;
        eprintln!("rose: {}", crate::auto_llvm::MESSAGE_AOT_INDISPONIBLE);
        std::process::exit(1);
    }
    #[cfg(feature = "llvm")]
    {
        let mut sortie = String::new();
        let mut fichier = String::new();
        let mut opt_level = crate::aot_llvm_orc::OrcOptLevel::O2;
        let mut pgo_mode = crate::aot_llvm_orc::PgoMode::None;
        let mut target_triple: Option<String> = None;
        let mut auto_link = false;
        let mut i = 2;
        while i < argv.len() {
            if argv[i] == "-O1" { opt_level = crate::aot_llvm_orc::OrcOptLevel::O1; i += 1; }
            else if argv[i] == "-O2" { opt_level = crate::aot_llvm_orc::OrcOptLevel::O2; i += 1; }
            else if argv[i] == "-O3" { opt_level = crate::aot_llvm_orc::OrcOptLevel::O3; i += 1; }
            else if argv[i] == "--pgo-generate" { pgo_mode = crate::aot_llvm_orc::PgoMode::Generate; i += 1; }
            else if argv[i] == "--pgo-use" { pgo_mode = crate::aot_llvm_orc::PgoMode::Use; i += 1; }
            else if argv[i] == "--link" { auto_link = true; i += 1; }
            else if argv[i] == "--target" && i + 1 < argv.len() { target_triple = Some(argv[i + 1].clone()); i += 2; }
            else if (argv[i] == "-o" || argv[i] == "--sortie") && i + 1 < argv.len() {
                sortie = argv[i + 1].clone(); i += 2;
            } else if !argv[i].starts_with('-') { fichier = argv[i].clone(); i += 1; }
            else { i += 1; }
        }
        if fichier.is_empty() { eprintln!("rose: quel programme compiler ?"); std::process::exit(1); }
        if sortie.is_empty() {
            sortie = fichier.rsplit_once(['/', '\\']).map(|(_, f)| f.to_string()).unwrap_or(fichier.clone());
            if let Some(p) = sortie.rfind(".rose") { sortie.truncate(p); }
            if sortie.is_empty() { sortie = "programme".into(); }
        }
        if auto_link && !sortie.ends_with(".o") {
            sortie = format!("{}.o", sortie);
        }
        match crate::aot_llvm_orc::run_orc(&fichier, &sortie, opt_level, pgo_mode, target_triple.as_deref()) {
            Ok(()) => {
                if auto_link {
                    let objet = sortie.clone();
                    let exe = if let Some(p) = sortie.rfind(".o") {
                        sortie[..p].to_string()
                    } else {
                        format!("{}.exe", sortie)
                    };
                    match crate::elf_linker::link_elf(&objet, &exe) {
                        Ok(()) => println!("rose: {} produit (binaire exécutable lié)", exe),
                        Err(e) => { eprintln!("rose: échec du lien ELF : {}", e); std::process::exit(1); }
                    }
                } else {
                    println!("rose: {} produit (AOT LLVM ORC, niveau {:?}, PGO {:?})", sortie, opt_level, pgo_mode);
                }
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("rose: échec AOT LLVM ORC : {}", e);
                std::process::exit(1);
            }
        }
    }
}

/// `rose link prog.o -o prog` : lien ELF (pas de LLVM nécessaire).
fn link(argv: &[String]) -> ! {
    #[cfg(not(feature = "llvm"))]
    {
        let _ = argv;
        eprintln!("rose: {}", crate::auto_llvm::MESSAGE_AOT_INDISPONIBLE);
        std::process::exit(1);
    }
    #[cfg(feature = "llvm")]
    {
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
        if objet.is_empty() { eprintln!("rose: quel objet linker ?"); std::process::exit(1); }
        if sortie.is_empty() {
            sortie = objet.rsplit_once(['/', '\\']).map(|(_, f)| f.to_string()).unwrap_or(objet.clone());
            if let Some(p) = sortie.rfind(".o") { sortie.truncate(p); }
            if sortie.is_empty() { sortie = "programme".into(); }
        }
        match crate::elf_linker::link_elf(&objet, &sortie) {
            Ok(()) => {}
            Err(e) => { eprintln!("rose: échec du lien ELF : {}", e); std::process::exit(1); }
        }
        std::process::exit(0);
    }
}
