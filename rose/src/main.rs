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
  rose version             numéro de version
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
    let argv: Vec<String> = std::env::args().collect();
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
            if let Some(helper) = auto_llvm::trouver_helper() {
                auto_llvm::lance_depuis(&helper, action, argv.as_slice());
            }
            eprintln!("rose: la chaîne AOT LLVM n'est pas embarquée dans ce binaire.");
            eprintln!("      le binaire peut télécharger le helper tout seul (commande 'rose orc')");
            eprintln!("      ou : cargo build --release --features llvm");
            std::process::exit(1);
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
    if std::env::var("ROSE_TIMING").is_ok() {
        eprintln!("TIMING-MAIN pret={:?} post_spawn={:?}", _t_pret, _t_post);
    }
    std::process::exit(rc);
}

