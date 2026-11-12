// Rose 2.0 — compilateur Rose en Rust + JIT Cranelift.
// Front-end fidèle au moteur C++ (lexeur, vérificateur, repli de constantes),
// interpréteur exact (arithmétique native i128, zéro dépendance) et JIT
// natif Cranelift pour le code chaud.
// Les sorties sont identiques au caractère près à `bin/rose` (moteur C++).
//
// Use of this source code is governed by an MIT-style license.

#![allow(unsafe_op_in_unsafe_fn)]

mod front;
mod i128x;
mod irrationnel;
mod interp;
mod jit;
mod run;
mod latex;
mod auto_llvm;
mod cmd_llvm;
mod sha256;
mod setup_core;
#[cfg(feature = "llvm")]
mod aot_llvm_orc;
#[cfg(feature = "llvm")]
mod elf_linker;

use std::io::Read;

fn lire(source: &mut String, chemin: &str) -> bool {
    match std::fs::File::open(chemin) {
        Ok(mut f) => f.read_to_string(source).is_ok(),
        Err(_) => false,
    }
}

const USAGE: &str = "\
Rose 2.0 — compilateur Rust + Cranelift JIT + LLVM ORC AOT (maths exactes BigInt natif, zéro GMP)
Usage :
  rose prog.rose [args…]   interpréter / JIT un programme Rose
  rose run prog.rose       identique
  rose comp prog.rose [-o sortie]   exécutable autonome (moteur + source collé)
  rose latex prog.rose              formule LaTeX du programme entier
  rose llvm prog.rose [-o sortie]   AOT LLVM classique (transpilation C + clang/gcc)
  rose orc prog.rose [-o sortie] [-O1|-O2|-O3] [--pgo-generate|--pgo-use] [--link]   AOT LLVM ORC (in-process, inkwell)
  rose link objet.o [-o sortie]                                               Lier un objet ELF en binaire exécutable
  rose info                état du moteur
  rose console [fichier.rose]   console interactive (REPL) — tapez des commandes Rose
  rose repl [fichier.rose]      identique
  rose update              installe/met a jour Rose (64 bits, interface console)
";

/* ── mode bundle : programme collé dans le moteur (miroir C++ rose.cpp:4344) ── */
const ROSE_MAGIC: [u8; 8] = [b'R', b'O', b'S', b'E', b'B', b'1', 0x01, 0x02];

fn chemin_moteur() -> Option<String> {
    std::env::current_exe().ok().and_then(|p| p.to_str().map(|s| s.to_string()))
}

/// Si l'exécutable courant porte une queue bundle, en retourne le source.
fn lire_bundle() -> Option<String> {
    let chemin = chemin_moteur()?;
    let octets = std::fs::read(&chemin).ok()?;
    if octets.len() < 24 { return None; }
    let q = &octets[octets.len() - 16..];
    if q[8..] != ROSE_MAGIC { return None; }
    let mut ln: u64 = 0;
    for i in 0..8 { ln |= (q[i] as u64) << (8 * i); }
    let ln = ln as usize;
    if ln == 0 || ln > octets.len() - 16 { return None; }
    let src = &octets[octets.len() - 16 - ln..octets.len() - 16];
    let mut s = String::from_utf8_lossy(src).into_owned();
    // normaliser fins de ligne (miroir normaliser_fins_de_ligne)
    if s.contains("\r\n") { s = s.replace("\r\n", "\n"); }
    if s.contains('\r') { s = s.replace('\r', "\n"); }
    Some(s)
}

/// comp : valide le source, copie le moteur puis le source, puis la queue.
fn comp_programme(fichier: &str, sortie: &str) -> i32 {
    let mut source = String::new();
    if !lire(&mut source, fichier) {
        eprintln!("rose: fichier introuvable : {}", fichier);
        return 1;
    }
    // valide avant d'embarquer (analyse + repli + vérification)
    match front::analyser(&source) {
        Ok(mut prog) => {
            if let Err(e) = front::verifier(&mut prog) {
                eprintln!("Rose : erreur — {}", e);
                return 1;
            }
        }
        Err(e) => { eprintln!("Rose : erreur — {}", e); return 1; }
    }
    let moteur = match chemin_moteur().and_then(|c| std::fs::read(c).ok()) {
        Some(m) => m,
        None => { eprintln!("rose: moteur illisible"); return 1; }
    };
    let mut out = std::fs::File::create(sortie).unwrap_or_else(|_| {
        eprintln!("rose: ne peut écrire {}", sortie);
        std::process::exit(1);
    });
    use std::io::Write;
    let _ = out.write_all(&moteur);
    let _ = out.write_all(source.as_bytes());
    let ln = source.len() as u64;
    let mut q = [0u8; 16];
    for i in 0..8 { q[i] = ((ln >> (8 * i)) & 0xFF) as u8; }
    q[8..].copy_from_slice(&ROSE_MAGIC);
    let _ = out.write_all(&q);
    drop(out);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(sortie, std::fs::Permissions::from_mode(0o755));
    }
    println!("rose: {} produit (exécutable autonome)", sortie);
    0
}

fn main() {
    let _t_start = std::time::Instant::now();
    // Hook de panic silencieux pour les erreurs Rose récupérables (ROSE_ERR:),
    // afin d'éviter le bruit « thread panicked » sur stderr. Les vrais bugs
    // (panics non ROSE_ERR) sont toujours affichés.
    std::panic::set_hook(Box::new(|info| {
        let msg = info.payload().downcast_ref::<String>()
            .map(|s| s.as_str())
            .or_else(|| info.payload().downcast_ref::<&str>().copied())
            .unwrap_or("");
        if msg.starts_with("ROSE_ERR:") { return; }
        eprintln!("{}", info);
    }));
    let mut argv: Vec<String> = std::env::args().collect();
    let console_apres = argv.iter().any(|a| a == "--console");
    argv.retain(|a| a != "--console");
    if argv.len() < 2 {
        // aucun argument : si l'exécutable porte un bundle, l'exécuter (mode
        // autonome, miroir du 'comp' C++ — rose.cpp:4783 lire_bundle).
        if let Some(source) = lire_bundle() {
            let source_t = source.clone();
            let h = std::thread::Builder::new().stack_size(8 * 1024 * 1024)
                .spawn(move || interp::executer_source(&source_t, Vec::new()));
            let rc = match h { Ok(j) => j.join().unwrap_or(1), Err(_) => interp::executer_source(&source, Vec::new()) };
            std::process::exit(rc);
        }
        eprint!("{}", USAGE);
        std::process::exit(1);
    }
    let action = argv[1].as_str();
    // La commande « llvm » (transpilation vers C) n'a pas besoin de LLVM :
    // elle est disponible meme dans le binaire leger.
    if action == "llvm" {
        cmd_llvm::execute("llvm", argv.as_slice());
    }
    // « orc » et « link » exigent LLVM : soit il est embarque, soit le
    // binaire le cherche tout seul (telechargement du helper AOT).
    // IMPORTANT : lancer le helper AVANT le message d'absence — l'ancien
    // code imprimait « non embarquee » puis sortait, ce qui rendait le
    // telechargement a la demande inatteignable dans le binaire leger.
    if matches!(action, "orc" | "link") {
        #[cfg(feature = "llvm")]
        cmd_llvm::execute(action, argv.as_slice());
        #[cfg(not(feature = "llvm"))]
        {
            // 1. helper déjà présent (cache ou à côté de l'exécutable) ?
            if let Some(helper) = auto_llvm::trouver_helper() {
                auto_llvm::lance_depuis(&helper, action, argv.as_slice());
            }
            // 2. sinon : téléchargement depuis la release GitHub (ou miroir)
            match auto_llvm::installer_helper() {
                Ok(helper) => auto_llvm::lance_depuis(&helper, action, argv.as_slice()),
                Err(e) => {
                    eprintln!("rose: la chaîne AOT LLVM n'est pas disponible.");
                    eprintln!("      {}", e);
                    eprintln!("      autre solution : cargo build --release --features llvm");
                    std::process::exit(1);
                }
            }
        }
    }
    match action {
        "version" | "--version" => { println!("{}", interp::version()); return; }
        "info" => { print!("{}", interp::info()); return; }
        "--help" | "-h" | "aide" => { print!("{}", USAGE); return; }
        "latex" => {
            // RENDU LaTeX DU PROGRAMME ENTIER : c'est la synthèse de ce
            // qu'on a programmé. La commande `run` montre, instruction par
            // instruction, la formule de chaque affichage ; celle-ci donne la
            // formule GLOBALE du programme.
            if argv.len() < 3 { eprintln!("rose: quel programme rendre ?"); std::process::exit(1); }
            let mut source = String::new();
            if !lire(&mut source, &argv[2]) {
                eprintln!("rose: fichier introuvable : {}", argv[2]);
                std::process::exit(1);
            }
            let prog = match front::analyser(&source) {
                Ok(p) => p,
                Err(e) => { eprintln!("Rose : erreur — {}", e); std::process::exit(1); }
            };
            print!("{}", latex::programme_latex(&prog));
            println!();
            return;
        }
        "comp" => {
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
            std::process::exit(comp_programme(&fichier, &sortie));
        }
        "console" | "repl" => {
            let fichier = if argv.len() >= 3 { Some(argv[2].as_str()) } else { None };
            console_mode(fichier, true);
            return;
        }
        "update" | "install" => {
            install_rose();
            return;
        }
        _ => {}
    }



    let (fichier, args) = if action == "run" {
        if argv.len() < 3 { eprintln!("rose: quel programme interpréter ?"); std::process::exit(1); }
        (argv[2].clone(), argv[3..].to_vec())
    } else {
        (argv[1].clone(), argv[2..].to_vec())
    };
    let mut source = String::new();
    if !lire(&mut source, &fichier) {
        eprintln!("rose: fichier introuvable : {}", fichier);
        std::process::exit(1);
    }
    // le moteur C++ execute le pipeline dans un thread a pile de 512 Mo
    // (rose.cpp:4745-4747) ; la recursion profonde de l'interpreteur Rust en a
    // autant. Repli en ligne si le thread ne peut etre cree.
    if argv.iter().any(|a| a == "--debug") || std::env::var("ROSE_DEBUG").is_ok() {
        crate::run::set_debug_mode(true);
    }
    if argv.iter().any(|a| a == "--step") || std::env::var("ROSE_STEP").is_ok() {
        crate::run::set_step_mode(true);
    }
    let source_t = source.clone();
    let args_t = args.clone();
    let _t_pret = _t_start.elapsed();
    let h = std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(move || interp::executer_source(&source_t, args_t));
    let _t_post = _t_start.elapsed();
    let rc = match h {
        Ok(j) => match j.join() {
            Ok(r) => r,
            Err(payload) => {
                let msg = if let Some(s) = payload.downcast_ref::<String>() { s.clone() }
                          else if let Some(s) = payload.downcast_ref::<&str>() { s.to_string() }
                          else { "erreur inconnue".to_string() };
                if let Some(m) = msg.strip_prefix("ROSE_ERR:") {
                    eprintln!("Rose : erreur — {}", m);
                } else {
                    eprintln!("Rose : panic — {}", msg);
                }
                1
            }
        },
        Err(_) => interp::executer_source(&source, args),
    };
    if console_apres {
        console_mode(Some(&fichier), false);
    }
    if std::env::var("ROSE_TIMING").is_ok() {
        eprintln!("TIMING-MAIN pret={:?} post_spawn={:?}", _t_pret, _t_post);
    }
    std::process::exit(rc);
}


/* ── console interactive (REPL) ──────────────────────────────────────── */

fn console_mode(fichier: Option<&str>, exec_initial: bool) {
    use std::io::{self, BufRead, Write};
    let mut buffer: Vec<String> = Vec::new();
    if let Some(f) = fichier {
        let mut source = String::new();
        if lire(&mut source, f) {
            // séparer définitions et affichages
            let mut defs: Vec<String> = Vec::new();
            let mut affs: Vec<String> = Vec::new();
            for line in source.lines() {
                let t = line.trim();
                if t.is_empty() { defs.push(line.to_string()); continue; }
                if t.starts_with(">>") { affs.push(line.to_string()); }
                else { defs.push(line.to_string()); }
            }
            buffer = defs;
            if exec_initial {
                let src = if affs.is_empty() {
                    buffer.join("\n")
                } else {
                    format!("{}\n{}", buffer.join("\n"), affs.join("\n"))
                };
                let h = std::thread::Builder::new().stack_size(8 * 1024 * 1024)
                    .spawn(move || crate::interp::executer_source(&src, Vec::new()));
                if let Ok(j) = h { let _ = j.join(); }
            }
        } else {
            eprintln!("rose: fichier introuvable : {}", f);
        }
    }
    println!("Console Rose. Tapez du code Rose, ou :help pour l'aide, :quit pour sortir.");
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();
    loop {
        print!("rose> ");
        let _ = io::stdout().flush();
        let mut entry = String::new();
        let mut first = true;
        loop {
            match lines.next() {
                None => {
                    if entry.trim().is_empty() { return; }
                    break;
                }
                Some(Ok(l)) => {
                    if first { entry.push_str(&l); first = false; }
                    else { entry.push('\n'); entry.push_str(&l); }
                    if braces_balanced(&entry) { break; }
                    print!("...> ");
                    let _ = io::stdout().flush();
                }
                Some(Err(_)) => return,
            }
        }
        let trimmed = entry.trim().to_string();
        if trimmed.is_empty() { continue; }
        if trimmed.starts_with(':') {
            let parts: Vec<&str> = trimmed.splitn(2, ' ').collect();
            match parts[0] {
                ":quit" | ":q" | ":exit" => return,
                ":reset" => { buffer.clear(); println!("(buffer vidé)"); }
                ":show" | ":code" => {
                    if buffer.is_empty() { println!("(buffer vide)"); }
                    else { for l in &buffer { println!("{}", l); } }
                }
                ":type" => {
                    if parts.len() < 2 { eprintln!("rose: :type <expr>"); continue; }
                    let expr = parts[1].trim();
                    let src = if buffer.is_empty() {
                        format!(">> type_de({})", expr)
                    } else {
                        format!("{}
>> type_de({})", buffer.join("\n"), expr)
                    };
                    let h = std::thread::Builder::new().stack_size(8 * 1024 * 1024)
                        .spawn(move || crate::interp::executer_source(&src, Vec::new()));
                    if let Ok(j) = h { let _ = j.join(); }
                    let _ = io::stdout().flush();
                }
                ":help" => {
                    println!("Commandes :");
                    println!("  :help            cette aide");
                    println!("  :show            affiche le code accumulé");
                    println!("  :reset           vide le buffer");
                    println!("  :load <fichier>  charge un fichier (et l'exécute)");
                    println!("  :quit            quitter");
                    println!("Toute autre ligne est du code Rose exécuté dans le contexte du buffer.");
                    println!("Les définitions sont mémorisées ; les '>> expr' s'affichent.");
                }
                ":load" => {
                    if parts.len() < 2 { eprintln!("rose: :load <fichier>"); continue; }
                    let f = parts[1].trim();
                    let mut source = String::new();
                    if lire(&mut source, f) {
                        let source_clone = source.clone();
                        let h = std::thread::Builder::new().stack_size(8 * 1024 * 1024)
                            .spawn(move || crate::interp::executer_source(&source_clone, Vec::new()));
                        if let Ok(j) = h { let _ = j.join(); }
                        for line in source.lines() {
                            let t = line.trim();
                            if !t.is_empty() && !t.starts_with(">>") {
                                buffer.push(line.to_string());
                            }
                        }
                        println!("(chargé {})", f);
                    } else {
                        eprintln!("rose: fichier introuvable : {}", f);
                    }
                }
                _ => eprintln!("rose: commande inconnue : {}", parts[0]),
            }
            continue;
        }
        let est_def = trimmed.starts_with("fn ")
            || trimmed.starts_with("si ")
            || trimmed.starts_with("si(")
            || trimmed.starts_with("pour ")
            || trimmed.starts_with("tant_que")
            || trimmed.starts_with("?")
            || trimmed.starts_with("@")
            || trimmed.contains(" = ");
        let ligne_exec = if trimmed.starts_with(">>") || est_def {
            trimmed.clone()
        } else {
            format!(">> {}", trimmed)
        };
        let src = if buffer.is_empty() {
            ligne_exec.clone()
        } else {
            format!("{}\n{}", buffer.join("\n"), ligne_exec)
        };
        let src_clone = src.clone();
        let h = std::thread::Builder::new().stack_size(8 * 1024 * 1024)
            .spawn(move || crate::interp::executer_source(&src_clone, Vec::new()));
        let rc = match h {
            Ok(j) => j.join().unwrap_or(1),
            Err(_) => crate::interp::executer_source(&src, Vec::new()),
        };
        let _ = io::stdout().flush();
        if rc == 0 && est_def && !trimmed.starts_with(">>") {
            buffer.push(trimmed.clone());
        }
    }
}

fn braces_balanced(s: &str) -> bool {
    let mut depth: i32 = 0;
    let mut in_str = false;
    let mut esc = false;
    for c in s.chars() {
        if in_str {
            if esc { esc = false; }
            else if c == '\\' { esc = true; }
            else if c == '"' { in_str = false; }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' | '[' | '(' => depth += 1,
            '}' | ']' | ')' => { depth -= 1; if depth < 0 { return true; } }
            _ => {}
        }
    }
    depth <= 0
}

/* ── installeur intégré (rose install) ──────────────────────────────── */
/* 64 bits natif, interface console colorée, pas de NSIS. */

fn install_rose() {
    use std::io::{self, Write, BufRead};
    // « rose update --force » : retélécharge même si la version est à jour
    let force = std::env::args().any(|a| a == "--force" || a == "-f");

    println!("{}", couleur("╔══════════════════════════════════════╗", "36"));
    println!("{}", couleur("║   Rose — installation / mise à jour      ║", "36"));
    println!("{}", couleur("╚══════════════════════════════════════╝", "36"));
    println!();

    let existant = setup_core::installation_existante();
    let prefixe = match existant {
        Some(ref p) => {
            println!("{} {}", couleur("Installation existante :", "33"), p);
            println!();
            println!("  1) Mettre à jour depuis GitHub (dernière release)");
            println!("  2) Réinstaller (écraser tous les fichiers)");
            println!("  3) Réparer (réinstaller les fichiers manquants)");
            println!("  4) Désinstaller");
            println!("  5) Annuler");
            print!("Choix [1-5] : ");
            let _ = io::stdout().flush();
            let mut buf = String::new();
            let _ = io::stdin().lock().read_line(&mut buf);
            match buf.trim() {
                "1" => {
                    println!("Recherche de la dernière release…");
                    for l in setup_core::mettre_a_jour(p, force) {
                        println!("{}", l);
                    }
                    return;
                }
                "4" => {
                    for l in setup_core::desinstaller(p) {
                        println!("{}", l);
                    }
                    return;
                }
                "5" => { println!("Annulé."); return; }
                _ => p.clone(),
            }
        }
        None => {
            let p = setup_core::prefixe_defaut();
            println!("Installation dans : {}", p);
            p
        }
    };

    for l in setup_core::installer(&prefixe, None) {
        println!("{}", l);
    }
    println!();
    println!("{}", couleur(&format!("Rose {} est installé.", env!("CARGO_PKG_VERSION")), "32"));
    println!("  Dossier : {}", prefixe);
    println!();
    println!("Ouvrez un nouveau terminal et tapez : rose --version");
}

fn couleur(s: &str, code: &str) -> String {
    format!("\x1b[{}m{}\x1b[0m", code, s)
}
