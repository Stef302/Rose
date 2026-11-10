//! Commandes LLVM de la ligne de commande (`rose llvm`, `rose orc`,
//! `rose link`).
//!
//! Ce module est partagé par le binaire principal et par le helper AOT
//! (`rose-aot`) que le binaire principal télécharge à la demande quand la
//! chaîne LLVM n'y est pas embarquée.
//!
//! Note : la commande `llvm` (transpilation vers C via le codeur Python)
//! n'a PAS besoin de LLVM — elle est donc disponible même dans le binaire
//! léger. Seules `orc` et `link` exigent LLVM.
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

/// `rose llvm prog.rose -o sortie` : transpilation vers C (codeur Python),
/// sans dépendance LLVM.
fn llvm_codegen(argv: &[String]) -> ! {
    let mut sortie = String::new();
    let mut fichier = String::new();
    let mut i = 2;
    while i < argv.len() {
        if (argv[i] == "-o" || argv[i] == "--sortie") && i + 1 < argv.len() {
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
    let srcdir = "archive/rose2_old_python/src";
    let st = std::process::Command::new("python3")
        .args(["-m", "rose2.driver", "build", &fichier, "--cible", "c", "-o", &sortie])
        .env("PYTHONPATH", srcdir)
        .status();
    if !st.map(|s| s.success()).unwrap_or(false) {
        eprintln!("rose: échec de l'AOT LLVM (python3/codegen_c)"); std::process::exit(1);
    }
    println!("rose: {} produit (AOT LLVM)", sortie);
    std::process::exit(0);
}

/// `rose orc prog.rose -o sortie.ll` : compilation AOT par LLVM ORC.
fn orc(argv: &[String]) -> ! {
    #[cfg(not(feature = "llvm"))]
    {
        let _ = argv;
        eprintln!("rose: la chaîne AOT LLVM n'est pas embarquée dans ce binaire.");
        eprintln!("      le binaire peut télécharger le helper tout seul (commande 'rose orc')");
        eprintln!("      ou : cargo build --release --features llvm");
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
        eprintln!("rose: le lieur ELF n'est pas embarqué dans ce binaire.");
        eprintln!("      le binaire peut télécharger le helper tout seul (commande 'rose link')");
        eprintln!("      ou : cargo build --release --features llvm");
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
