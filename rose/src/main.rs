// Rose — compilateur Rose en Rust + JIT Cranelift.
// Front-end du langage (lexeur, vérificateur, repli de constantes),
// interpréteur exact (arithmétique native i128, zéro dépendance) et JIT
// natif Cranelift pour le code chaud.
// Les sorties sont identiques au caractère près entre l'interpréteur et le JIT.
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
mod debug_lenteur;
mod apercu_latex;
mod auto_llvm;
mod cmd_llvm;
mod sha256;
mod repl;
mod setup_core;
mod velopack;
mod inflate;
mod png;
mod mul_rapide;
mod rir;
#[allow(dead_code)]
mod wasm_lecteur;
mod paint;
mod console_win;
mod setup_gui;
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

/// Aide du programme. La version vient du `Cargo.toml` — elle était en
/// dur (« Rose 2.0 »), d'où un `rose` installé en 0.1.5 qui s'annonçait
/// « Rose 2.0 » (mesuré par l'utilisateur).
fn usage() -> String {
    format!(
        "Rose {} — compilateur Rust + Cranelift JIT + LLVM ORC AOT (maths exactes BigInt natif, zéro GMP)\n\
Usage :
  rose prog.rose [args…]   interpréter / JIT un programme Rose
  rose run prog.rose       identique
  rose comp prog.rose [-o sortie]   exécutable autonome (moteur + source collé)
  rose latex prog.rose              formule LaTeX du programme entier
  rose latex prog.rose --apercu     idem, rendu VISIBLE (Unicode, sans LaTeX)
  rose latex prog.rose --lignes     seulement les lignes marquees %% (LaTeX brut)
  rose latex prog.rose --lignes --apercu   les lignes %% en rendu visible
  rose llvm prog.rose [-o sortie]   AOT LLVM classique (transpilation C + clang/gcc)
  rose orc prog.rose [-o sortie] [-O1|-O2|-O3] [--pgo-generate|--pgo-use] [--link]   AOT LLVM ORC (in-process, inkwell)
  rose link objet.o [-o sortie]                                               Lier un objet ELF en binaire exécutable
  rose info                état du moteur
  rose console [fichier.rose]   console interactive (REPL) — tapez des commandes Rose
  rose repl [fichier.rose]      identique
  rose debug-lenteur [-n N] [prog.rose]   OÙ VA LE TEMPS ? lancement, analyse, exécution
  rose wasm lib.wasm       fonctions exportées d'un module WebAssembly (pont @dll(\"wasm\", …))
  rose update              installe/met a jour Rose (64 bits, interface console)
",
        env!("CARGO_PKG_VERSION")
    )
}

/* ── mode bundle : programme collé dans le moteur  ── */
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
    // ⚠⚠ MESURE : le temps passé dans la préparation de la console
    // (`SetConsoleTitleW` + fil d'icône) est ventilé à part quand
    // `ROSE_TIMING` est posé. C'est le suspect n°2 de la « lenteur avant
    // la première sortie » signalée sur Windows.
    let _t_entree = _t_start.elapsed();
    // Titre et icône de la fenêtre de console (Windows). Doit être fait
    // AVANT toute écriture : le double-clic sur un `.rose` ouvre cette
    // fenêtre, et elle doit porter la rose de marque.
    console_win::preparer();
    let _t_console = _t_start.elapsed();
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
    // Velopack : le launcher relance l'application avec un argument interne
    // (`--veloapp-updated`, `--veloapp-obsolete`) — il n'est PAS un nom de
    // fichier, on le route avant toute autre action.
    if velopack::is_update_launcher(&argv) {
        std::process::exit(velopack::run_update_launcher(&argv));
    }
    if argv.len() < 2 {
        // aucun argument : si l'exécutable porte un bundle, l'exécuter (mode
        // autonome — lire_bundle).
        if let Some(source) = lire_bundle() {
            let source_t = source.clone();
            let h = std::thread::Builder::new().stack_size(8 * 1024 * 1024)
                .spawn(move || interp::executer_source(&source_t, Vec::new()));
            let rc = match h { Ok(j) => j.join().unwrap_or(1), Err(_) => interp::executer_source(&source, Vec::new()) };
            std::process::exit(rc);
        }
        eprint!("{}", usage());
        std::process::exit(1);
    }
    let action = argv[1].as_str();
    // ⚠ `debug-lenteur` : diagnostic du temps de lancement / analyse /
    // exécution. Il ne dépend de RIEN (std seul) et se place AVANT toute
    // initialisation lourde.
    if action == "debug-lenteur" || action == "lenteur" {
        std::process::exit(debug_lenteur::execute(&argv));
    }
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
        "--help" | "-h" | "aide" => { print!("{}", usage()); return; }
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
            // `--lignes` : ne rendre QUE les lignes marquées `%%`.
            //
            // ⚠ On ne passe PAS par l'analyseur : une ligne `%% $N!$` n'est
            // pas du code Rose valide, et le programme peut être incomplet.
            // Le marqueur est une ANNOTATION, pas une instruction — le
            // fichier reste exécutable tel quel, l'analyseur ignore `%%`
            // comme un commentaire (TeX) s'il le rencontre.
            //
            // ⚠⚠ LA SORTIE COMPORTE UN `\\` APRÈS CHAQUE LIGNE, Y COMPRIS LA
            // DERNIÈRE (16/11). L'ancienne version le séparait seulement
            // ENTRE les lignes : la dernière marque se retrouvait collée à
            // la suivante (`$n!$` sans `\\`), donc invisible dans un
            // `align`/`gather` LaTeX. Les tests unitaires l'avaient figé
            // (« sans séparateur final ») — c'est le COMPORTEMENT qui était
            // faux, pas le test.
            // ⚠ `--apercu` : rendu VISIBLE (Unicode) au lieu du LaTeX brut.
            // C'est la réponse à « un programme qui contient du latex pour
            // voir dans ton output ce que ça donne » : on vérifie le rendu
            // sans LaTeX installé, et toute commande non gérée s'affiche
            // entre « » au lieu d'être perdue.
            let apercu = argv.iter().any(|a| a == "--apercu" || a == "-a");
            if argv.iter().any(|a| a == "--lignes" || a == "-l") {
                let texte = latex::programme_latex_lignes(&source);
                if apercu {
                    for ligne in texte.lines() {
                        let l = ligne.trim_end_matches(" \\\\").trim_end();
                        print!("{}\n", apercu_latex::rendre(l));
                    }
                } else {
                    print!("{}", texte);
                }
                return;
            }
            let prog = match front::analyser(&source) {
                Ok(p) => p,
                Err(e) => { eprintln!("Rose : erreur — {}", e); std::process::exit(1); }
            };
            let texte = latex::programme_latex(&prog);
            if apercu {
                for ligne in texte.lines() {
                    let l = ligne.trim_end_matches(" \\\\").trim_end();
                    if l.is_empty() { continue; }
                    print!("{}\n", apercu_latex::rendre(l));
                }
                return;
            }
            print!("{}", texte);
            println!();
            return;
        }
        "wasm" => {
            // ⚠ Inspecte un module WebAssembly SANS l'exécuter : les fonctions
            // exportées et leurs types (pour écrire les `@sym`/`fn` Rose).
            let fichier = match argv.get(2) {
                Some(f) => f.clone(),
                None => { eprintln!("rose wasm : quel module ? (rose wasm lib.wasm)"); std::process::exit(1); }
            };
            let octets = match std::fs::read(&fichier) {
                Ok(o) => o,
                Err(e) => { eprintln!("rose wasm : {fichier} illisible ({e})"); std::process::exit(1); }
            };
            let module = match wasm_lecteur::charger(&octets) {
                Ok(m) => m,
                Err(e) => { eprintln!("rose wasm : {fichier} : {e}"); std::process::exit(1); }
            };
            let inst = wasm_lecteur::InstanceWasm::nouveau(std::sync::Arc::new(module));
            let mut n = 0;
            for (nom, params, resultats) in inst.exports_fonctions() {
                let ty = |t: wasm_lecteur::TyWasm| match t {
                    wasm_lecteur::TyWasm::I32 => "entier",
                    wasm_lecteur::TyWasm::I64 => "entier64",
                    wasm_lecteur::TyWasm::F32 => "flottant32",
                    wasm_lecteur::TyWasm::F64 => "flottant",
                };
                let ps: Vec<String> = params.iter().enumerate()
                    .map(|(i, t)| format!("a{i}: {}", ty(*t))).collect();
                let r = resultats.first().map(|t| ty(*t)).unwrap_or("vide");
                println!("@dll(\"wasm\", \"{fichier}\")");
                println!("@sym(\"{nom}\")");
                println!("fn {nom}({}) -> {r}", ps.join(", "));
                println!();
                n += 1;
            }
            eprintln!("{n} fonction(s) exportée(s)");
            std::process::exit(0);
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
            std::process::exit(repl::lancer(fichier, true));
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
    // le pipeline s'execute dans un thread a pile de 512 Mo
    // ; la recursion profonde de l'interpreteur Rust en a
    // autant. Repli en ligne si le thread ne peut etre cree.
    if argv.iter().any(|a| a == "--debug") || std::env::var("ROSE_DEBUG").is_ok() {
        crate::run::set_debug_mode(true);
    }
    if argv.iter().any(|a| a == "--step") || std::env::var("ROSE_STEP").is_ok() {
        crate::run::set_step_mode(true);
    }
    let source_t = source.clone();
    let args_t = args.clone();
    let fichier_t = fichier.clone();
    let _t_pret = _t_start.elapsed();
    // ⚠⚠⚠ 512 Mo DE PILE, PAS 8 Mo (mesuré le 16/11).
    //
    // Le commentaire d'origine dit « pile de 512 Mo :
    // une pile de 512 Mo ». La constante, elle, valait 8 Mo : mesuré,
    // `it(10000)` (récursion 10 000, multiplication entière donc hors JIT)
    // débordait la pile vers 4 300 niveaux, alors que le moteur de
    // référence répond 450015000 pour la même récursion. Une pile de 512 Mo
    // comme la pile de référence couvre les récursions profondes que le JIT ne peut pas
    // prendre (corps avec multiplication/puissance = BigInt exact).
    //
    // ⚠ Les 512 Mo sont RÉSERVÉS (mmap), pas engagés : aucune mémoire n'est
    // touchée tant que la récursion n'y descend pas.
    // ⚠⚠⚠ L'EXÉCUTION PASSE PAR LE FICHIER (imports résolus) (mesuré le 17/11).
    let h = std::thread::Builder::new()
        .stack_size(512 * 1024 * 1024)
        .spawn(move || interp::executer_fichier(&fichier_t, args_t));
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
        Err(_) => interp::executer_fichier(&fichier, args),
    };
    if console_apres {
        repl::lancer(Some(&fichier), false);
    }
    if std::env::var("ROSE_TIMING").is_ok() {
        // ⚠ Ventilation COMPLÈTE : entrée dans main, préparation console,
        // lecture du fichier, spawn du fil d'exécution, total.
        eprintln!("TIMING entree_main={:?} console={:?} pret={:?} post_spawn={:?}",
                  _t_entree, _t_console, _t_pret, _t_post);
    }
    // ⚠ La fenêtre de console ouverte par un DOUBLE-CLIC sur un `.rose` se
    // ferme avec le programme : l'utilisateur n'a pas le temps de lire la
    // sortie (« la fenêtre apparaît et disparaît »). On attend une frappe
    // quand la console n'appartient qu'à nous.
    // En mode REPL, c'est le REPL qui attend (à sa sortie) : on ne fait pas
    // attendre deux fois.
    if !console_apres {
        console_win::attendre_fermeture();
    }
    std::process::exit(rc);
}


/* ── console interactive : voir src/repl.rs ─────────────────────────── */
// `rose console` et le binaire autonome `rose-shell` partagent le même
// module `repl` : une seule implémentation, deux points d'entrée.

/* ── installeur intégré (rose install) ──────────────────────────────── */
/* 64 bits natif, interface console colorée, pas de NSIS. */

fn install_rose() {
    use std::io::{self, Write, BufRead};
    // « rose update --force » : retélécharge même si la version est à jour
    let force = std::env::args().any(|a| a == "--force" || a == "-f");

    // Windows : mise à jour via Velopack (delta + rollback) — activation
    // explicite : « rose update --velopack » ou ROSE_VELOPACK=1. Les builds
    // installés par Velopack portent la variable VELOCABI (posée par le
    // launcher) et basculent automatiquement.
    // (le module est portable : la voie Velopack est vérifiable sous Linux
    // avec ROSE_VELOPACK_URL pointant sur un manifeste local `file://`).
    {
        let args: Vec<String> = std::env::args().collect();
        let veut_velopack = std::env::var("ROSE_VELOPACK").is_ok()
            || std::env::var("VELOCABI").is_ok()
            || args.iter().any(|a| a == "--velopack");
        if veut_velopack {
            println!("Mise à jour Velopack…");
            match velopack::check_and_update(true) {
                Ok(Some(v)) => {
                    // Sans launcher Velopack (installation faite à la main par
                    // Rose-win-Setup.exe), on applique NOUS-MÊMES le paquet :
                    // sinon le fichier reste dans packages\ et rien ne change.
                    match velopack::appliquer_paquets() {
                        Ok(Some(appliquee)) => println!(
                            "Rose {appliquee} installé — relancez « rose » (l'ancienne version reste en place pour un rollback)."
                        ),
                        Ok(None) => println!(
                            "Paquet {v} déposé — le launcher Velopack l'appliquera au prochain démarrage."
                        ),
                        Err(e) => eprintln!("Application du paquet : {e}"),
                    }
                }
                Ok(None) => println!("Rose est déjà à jour."),
                Err(e) => eprintln!("Velopack : {e} — repli sur l'installeur classique."),
            }
            return;
        }
    }

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
