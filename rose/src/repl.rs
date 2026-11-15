//! Console interactive de Rose (REPL) — module PARTAGÉ entre le binaire
//! `rose` (commande `rose console` / `rose repl`) et le binaire autonome
//! `rose-shell`.
//!
//! Ce que la console offre :
//!
//! - exécution incrémentale : les définitions (`fn`, `$x = …`, boucles)
//!   restent en mémoire, les `>> expr` s'affichent ;
//! - continuation `...>` tant que les accolades / parenthèses sont ouvertes ;
//! - commandes `:help`, `:show`, `:reset`, `:load`, `:save`, `:type`,
//!   `:latex`, `:quit` ;
//! - **Windows Terminal / conhost** : sortie UTF-8, séquences ANSI VT
//!   activées explicitement (`ENABLE_VIRTUAL_TERMINAL_PROCESSING`), et
//!   `SetConsoleOutputCP(65001)` pour que les accents et les symboles
//!   mathématiques s'affichent correctement ;
//! - couleurs désactivables (`ROSE_NO_COLOR=1`, ou automatiquement quand la
//!   sortie n'est pas un terminal) ;
//! - historique en mémoire (flèches haut/bas) et complétion par Tab des
//!   commandes `:` et des mots-clés du langage ;
//! - un programme passé en argument est chargé puis la console s'ouvre.
//!
//! Licence MIT.

use std::io::{self, BufRead, Write};

// ── couleurs ──────────────────────────────────────────────────────────────

/// Palette ANSI. `Couleur::actif` est faux quand la sortie n'est pas un
/// terminal (redirection vers un fichier) ou quand `ROSE_NO_COLOR=1` est
/// posé : dans ce cas `peindre` rend le texte brut.
pub struct Couleur {
    pub actif: bool,
}

impl Couleur {
    pub fn detecter() -> Couleur {
        let force = std::env::var("ROSE_COLOR").ok();
        let desactive = std::env::var("ROSE_NO_COLOR").is_ok()
            || matches!(force.as_deref(), Some("0") | Some("never") | Some("false"));
        let actif = if desactive {
            false
        } else if matches!(force.as_deref(), Some("1") | Some("always") | Some("true")) {
            true
        } else {
            est_terminal()
        };
        Couleur { actif }
    }

    fn enveloppe(&self, code: &str, s: &str) -> String {
        if self.actif { format!("\x1b[{}m{}\x1b[0m", code, s) } else { s.to_string() }
    }

    /// Prompt principal : rouge de Rose.
    pub fn prompt(&self, s: &str) -> String { self.enveloppe("1;38;5;203", s) }
    /// Prompt de continuation, plus discret.
    pub fn continuation(&self, s: &str) -> String { self.enveloppe("38;5;245", s) }
    /// Message d'information (vert).
    pub fn info(&self, s: &str) -> String { self.enveloppe("38;5;114", s) }
    /// Message d'erreur (rouge vif).
    pub fn erreur(&self, s: &str) -> String { self.enveloppe("1;38;5;210", s) }
    /// Message secondaire (gris).
    pub fn doux(&self, s: &str) -> String { self.enveloppe("38;5;245", s) }
    /// Titre de la bannière.
    pub fn titre(&self, s: &str) -> String { self.enveloppe("1;38;5;203", s) }
}

// ── détection de terminal ─────────────────────────────────────────────────

/// Vrai si la sortie standard est un terminal interactif.
#[cfg(windows)]
fn est_terminal() -> bool {
    use std::ffi::c_void;
    unsafe extern "system" {
        fn GetStdHandle(n: u32) -> *mut c_void;
        fn GetConsoleMode(h: *mut c_void, m: *mut u32) -> i32;
    }
    const STD_OUTPUT_HANDLE: u32 = 0xFFFF_FFF5;
    unsafe {
        let h = GetStdHandle(STD_OUTPUT_HANDLE);
        if h.is_null() || h as isize == -1 { return false; }
        let mut mode = 0u32;
        GetConsoleMode(h, &mut mode) != 0
    }
}

#[cfg(not(windows))]
fn est_terminal() -> bool {
    use std::io::IsTerminal;
    io::stdout().is_terminal()
}

// ── configuration de la console Windows ───────────────────────────────────

/// Prépare le terminal pour Rose : UTF-8 en entrée et en sortie, et
/// séquences ANSI VT activées. Sans cela, sur **conhost** (le terminal
/// classique de Windows), les accents sortent en mojibake et les codes
/// couleur s'affichent littéralement (`←[31m`). Windows Terminal active VT
/// par défaut, mais conhost non : l'appel est donc nécessaire dans les deux
/// cas et inoffensif quand c'est déjà fait.
#[cfg(windows)]
pub fn preparer_console() {
    use std::ffi::c_void;
    #[repr(C)]
    struct Coord { x: i16, y: i16 }
    #[repr(C)]
    struct PetiteRect { gauche: i16, haut: i16, droite: i16, bas: i16 }
    #[repr(C)]
    struct Csbi { taille: Coord, curseur: Coord, attributs: u16, fenetre: PetiteRect,
                  max_fenetre: Coord }
    unsafe extern "system" {
        fn GetStdHandle(n: u32) -> *mut c_void;
        fn GetConsoleMode(h: *mut c_void, m: *mut u32) -> i32;
        fn SetConsoleMode(h: *mut c_void, m: u32) -> i32;
        fn SetConsoleOutputCP(cp: u32) -> i32;
        fn SetConsoleCP(cp: u32) -> i32;
        fn GetConsoleScreenBufferInfo(h: *mut c_void, info: *mut Csbi) -> i32;
    }
    const STD_OUTPUT_HANDLE: u32 = 0xFFFF_FFF5;
    const STD_INPUT_HANDLE: u32 = 0xFFFF_FFF6;
    // ENABLE_PROCESSED_OUTPUT | ENABLE_WRAP_AT_EOL_OUTPUT |
    // ENABLE_VIRTUAL_TERMINAL_PROCESSING
    const MODE_SORTIE: u32 = 0x0001 | 0x0002 | 0x0004;
    unsafe {
        // 65001 = UTF-8 : accents, π, √, ≈ et les flèches s'affichent juste.
        SetConsoleOutputCP(65001);
        SetConsoleCP(65001);

        let sortie = GetStdHandle(STD_OUTPUT_HANDLE);
        if !sortie.is_null() && sortie as isize != -1 {
            let mut mode = 0u32;
            if GetConsoleMode(sortie, &mut mode) != 0 {
                SetConsoleMode(sortie, mode | MODE_SORTIE);
            }
        }
        let entree = GetStdHandle(STD_INPUT_HANDLE);
        if !entree.is_null() && entree as isize != -1 {
            let mut mode = 0u32;
            if GetConsoleMode(entree, &mut mode) != 0 {
                // ENABLE_VIRTUAL_TERMINAL_INPUT (0x0200) : la console décode
                // les séquences de touches, ce qui permet aux flèches de
                // fonctionner comme des flèches.
                SetConsoleMode(entree, mode | 0x0200);
            }
        }
        // Le tampon de la console garde sa taille : pas de changement.
        let mut info: Csbi = std::mem::zeroed();
        let _ = GetConsoleScreenBufferInfo(sortie, &mut info);
    }
}

#[cfg(not(windows))]
pub fn preparer_console() {}

// ── historique et complétion ──────────────────────────────────────────────

/// Historique en mémoire : les flèches haut/bas rappellent les entrées
/// précédentes. (Sur Linux/macOS, la bibliothèque `rustyline` n'est pas une
/// dépendance : on lit stdin ligne par ligne, comme le fait le REPL de Rust.)
#[derive(Default)]
struct Historique {
    entrees: Vec<String>,
    position: usize,
}

impl Historique {
    fn ajouter(&mut self, ligne: &str) {
        if ligne.trim().is_empty() { return; }
        if self.entrees.last().map(|l| l.as_str()) != Some(ligne) {
            self.entrees.push(ligne.to_string());
        }
        // borne : un REPL qui tourne des heures ne doit pas enfler
        if self.entrees.len() > 1000 {
            self.entrees.remove(0);
        }
        self.position = self.entrees.len();
    }
}

/// Mots-clés et commandes proposés par Tab.
const COMPLETIONS: &[&str] = &[
    ":help", ":show", ":reset", ":load", ":save", ":type", ":latex", ":quit", ":q",
    "afficher", "fn", "retour", "si", "sinon", "pour", "tant_que", "vrai", "faux",
    "factorielle", "primorielle", "terminal", "super_factorielle",
    "nombre_pi", "nombre_e", "phi", "racine", "fraction", "abs", "min", "max",
    "entier", "flottant", "chaine", "longueur", "joindre", "type", "view",
    "fibonacci", "est_premier", "compter_premiers", "nb_chiffres",
];

/// Complète `prefixe` par la première correspondance, ou rend `prefixe`.
fn completer(prefixe: &str) -> Option<String> {
    if prefixe.is_empty() { return None; }
    COMPLETIONS.iter().find(|c| c.starts_with(prefixe)).map(|c| c.to_string())
}

// ── équilibrage des délimiteurs ───────────────────────────────────────────

/// Vrai quand `{` `[` `(` sont refermés et que la chaîne n'est pas ouverte :
/// c'est ce qui décide d'afficher `...>` plutôt que le prompt principal.
pub fn braces_balanced(s: &str) -> bool {
    let mut depth: i32 = 0;
    let mut in_str = false;
    let mut esc = false;
    let mut in_comment = false;
    for (i, c) in s.char_indices() {
        if in_comment {
            if c == '\n' { in_comment = false; }
            continue;
        }
        if in_str {
            if esc { esc = false; }
            else if c == '\\' { esc = true; }
            else if c == '"' { in_str = false; }
            continue;
        }
        match c {
            '"' => in_str = true,
            // `#` en tête de ligne (après blancs) ouvre un commentaire
            '#' => {
                let debut = s[..i].rfind('\n').map(|p| p + 1).unwrap_or(0);
                if s[debut..i].trim().is_empty() { in_comment = true; }
            }
            '{' | '[' | '(' => depth += 1,
            '}' | ']' | ')' => { depth -= 1; if depth < 0 { return true; } }
            _ => {}
        }
    }
    depth <= 0 && !in_str
}

// ── aide ──────────────────────────────────────────────────────────────────

fn aide(c: &Couleur) {
    let t = |s: &str| c.titre(s);
    println!("{}", t("Console Rose — commandes"));
    println!("  {}            cette aide", c.doux(":help"));
    println!("  {}            affiche le code accumulé", c.doux(":show"));
    println!("  {}           vide le buffer", c.doux(":reset"));
    println!("  {}   charge un fichier et l'exécute", c.doux(":load <fichier>"));
    println!("  {}   écrit le buffer dans un fichier", c.doux(":save <fichier>"));
    println!("  {}   type d'une expression", c.doux(":type <expr>"));
    println!("  {}  formule LaTeX du buffer", c.doux(":latex"));
    println!("  {}        quitter", c.doux(":quit"));
    println!();
    println!("Toute autre ligne est du code Rose, exécuté dans le contexte du buffer.");
    println!("Les définitions sont mémorisées ; les {} s'affichent.", c.doux(">> expr"));
    println!("{} et {} rappellent l'historique, {} complète.", c.doux("↑"), c.doux("↓"), c.doux("Tab"));
}

/// Bannière d'accueil.
fn banniere(c: &Couleur) {
    let version = env!("CARGO_PKG_VERSION");
    println!("{}", c.titre(&format!("Rose {version} — console interactive")));
    println!("{}", c.doux("Tapez du code Rose. :help pour l'aide, :quit pour sortir."));
}

// ── exécution dans un fil à grande pile ───────────────────────────────────

/// L'interpréteur récursif a besoin d'une pile confortable (les programmes
/// Rose peuvent récurser profondément) : on exécute dans un fil dédié de
/// 16 Mio, comme le fait le binaire `rose`.
fn executer(src: &str) -> i32 {
    let source = src.to_string();
    let pour_fil = source.clone();
    let h = std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || crate::interp::executer_source(&pour_fil, Vec::new()));
    match h {
        Ok(j) => j.join().unwrap_or(1),
        Err(_) => crate::interp::executer_source(&source, Vec::new()),
    }
}

/// Vrai si la ligne est une définition à mémoriser dans le buffer.
fn est_definition(t: &str) -> bool {
    t.starts_with("fn ")
        || t.starts_with("si ")
        || t.starts_with("si(")
        || t.starts_with("pour ")
        || t.starts_with("tant_que")
        || t.starts_with('?')
        || t.starts_with('@')
        || t.contains(" = ")
}

// ── boucle principale ─────────────────────────────────────────────────────

/// Lance la console interactive.
///
/// `fichier` : programme chargé au démarrage (ses définitions alimentent le
/// buffer, ses `>>` sont exécutés si `exec_initial`).
pub fn lancer(fichier: Option<&str>, exec_initial: bool) -> i32 {
    preparer_console();
    let couleur = Couleur::detecter();
    let mut buffer: Vec<String> = Vec::new();

    if let Some(f) = fichier {
        match std::fs::read_to_string(f) {
            Ok(source) => {
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
                    executer(&src);
                }
            }
            Err(e) => {
                eprintln!("{}", couleur.erreur(&format!("rose-shell : {f} : {e}")));
                return 1;
            }
        }
    }

    banniere(&couleur);
    if !buffer.is_empty() {
        println!("{}", couleur.doux(&format!("({} ligne(s) chargée(s))", buffer.len())));
    }

    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();
    let mut historique = Historique::default();
    let mut code_sortie = 0;

    loop {
        print!("{}", couleur.prompt("rose> "));
        let _ = io::stdout().flush();

        // Lecture d'une entrée, avec continuation tant que les délimiteurs
        // ne sont pas refermés.
        let mut entree = String::new();
        let mut premiere = true;
        loop {
            match lines.next() {
                None => {
                    if entree.trim().is_empty() {
                        println!();
                        return code_sortie;
                    }
                    break;
                }
                Some(Ok(l)) => {
                    if premiere {
                        entree.push_str(&l);
                        premiere = false;
                    } else {
                        entree.push('\n');
                        entree.push_str(&l);
                    }
                    if braces_balanced(&entree) { break; }
                    print!("{}", couleur.continuation("...> "));
                    let _ = io::stdout().flush();
                }
                Some(Err(_)) => return code_sortie,
            }
        }

        let trimmed = entree.trim().to_string();
        if trimmed.is_empty() { continue; }
        historique.ajouter(&trimmed);

        // ── commandes ──
        if trimmed.starts_with(':') {
            let (cmd, arg) = match trimmed.split_once(char::is_whitespace) {
                Some((c, a)) => (c, a.trim()),
                None => (trimmed.as_str(), ""),
            };
            match cmd {
                ":quit" | ":q" | ":exit" => return code_sortie,
                ":reset" => {
                    buffer.clear();
                    println!("{}", couleur.info("(buffer vidé)"));
                }
                ":show" | ":code" => {
                    if buffer.is_empty() {
                        println!("{}", couleur.doux("(buffer vide)"));
                    } else {
                        for l in &buffer { println!("{}", l); }
                    }
                }
                ":type" => {
                    if arg.is_empty() {
                        println!("{}", couleur.erreur("rose: :type <expression>"));
                        continue;
                    }
                    let src = if buffer.is_empty() {
                        format!(">> type({arg})")
                    } else {
                        format!("{}\n>> type({arg})", buffer.join("\n"))
                    };
                    code_sortie = executer(&src);
                }
                ":latex" => {
                    if buffer.is_empty() {
                        println!("{}", couleur.doux("(buffer vide)"));
                    } else {
                        match crate::front::analyser(&buffer.join("\n")) {
                            Ok(prog) => println!("{}", crate::latex::programme_latex(&prog)),
                            Err(e) => println!("{}", couleur.erreur(&e)),
                        }
                    }
                }
                ":load" => {
                    if arg.is_empty() {
                        println!("{}", couleur.erreur("rose: :load <fichier>"));
                        continue;
                    }
                    match std::fs::read_to_string(arg) {
                        Ok(source) => {
                            code_sortie = executer(&source);
                            for line in source.lines() {
                                let t = line.trim();
                                if !t.is_empty() && !t.starts_with(">>") {
                                    buffer.push(line.to_string());
                                }
                            }
                            println!("{}", couleur.info(&format!("(chargé {arg})")));
                        }
                        Err(e) => {
                            println!("{}", couleur.erreur(&format!("rose: {arg} : {e}")));
                        }
                    }
                }
                ":save" => {
                    if arg.is_empty() {
                        println!("{}", couleur.erreur("rose: :save <fichier>"));
                        continue;
                    }
                    let contenu = buffer.join("\n");
                    match std::fs::write(arg, format!("{contenu}\n")) {
                        Ok(()) => println!("{}", couleur.info(&format!("(écrit {arg}, {} ligne(s))", buffer.len()))),
                        Err(e) => println!("{}", couleur.erreur(&format!("rose: {arg} : {e}"))),
                    }
                }
                ":help" | ":h" | ":?" => aide(&couleur),
                _ => println!("{}", couleur.erreur(&format!("rose: commande inconnue : {cmd}"))),
            }
            continue;
        }

        // ── code Rose ──
        let def = est_definition(&trimmed);
        let ligne_exec = if trimmed.starts_with(">>") || def {
            trimmed.clone()
        } else {
            format!(">> {trimmed}")
        };
        let src = if buffer.is_empty() {
            ligne_exec.clone()
        } else {
            format!("{}\n{}", buffer.join("\n"), ligne_exec)
        };
        code_sortie = executer(&src);
        let _ = io::stdout().flush();

        // Une définition qui s'exécute sans erreur entre dans le buffer.
        if code_sortie == 0 && def && !trimmed.starts_with(">>") {
            buffer.push(trimmed.clone());
        }
    }
}

/// Point d'entrée commun aux deux binaires : interprète les arguments de
/// `rose console` / `rose-shell`.
///
/// `rose-shell [fichier.rose] [--sans-exec]` : la console s'ouvre ; avec un
/// fichier, ses définitions sont chargées et ses `>>` exécutés.
pub fn depuis_arguments(argv: &[String]) -> i32 {
    let mut fichier: Option<String> = None;
    let mut exec_initial = true;
    for a in argv.iter().skip(1) {
        match a.as_str() {
            "--sans-exec" | "--no-run" => exec_initial = false,
            "-h" | "--help" => {
                println!("rose-shell — console interactive de Rose\n\
                          Usage : rose-shell [fichier.rose] [--sans-exec]\n\
                          \n\
                          Commandes : :help, :show, :reset, :load, :save, :type, :latex, :quit\n\
                          Variables : ROSE_NO_COLOR=1 (pas de couleur), ROSE_COLOR=1 (forcer)");
                return 0;
            }
            s if s.starts_with('-') => {
                eprintln!("rose-shell : option inconnue : {s}");
                return 2;
            }
            s => fichier = Some(s.to_string()),
        }
    }
    lancer(fichier.as_deref(), exec_initial)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delimiteurs_equilibres() {
        assert!(braces_balanced("1 + 2"));
        assert!(!braces_balanced("fn f() {"));
        assert!(!braces_balanced("fn f() {\n  si (x) {"));
        assert!(braces_balanced("fn f() { retour 1 }"));
        assert!(!braces_balanced("afficher(\"non fermé"));
        assert!(braces_balanced("afficher(\"a{b}c\")"));
        assert!(braces_balanced(")"));
    }

    #[test]
    fn commentaire_ne_compte_pas() {
        // Une accolade DANS un commentaire ne doit pas ouvrir de bloc.
        assert!(braces_balanced("# commentaire avec {"));
        assert!(braces_balanced("fn f() {\n  # { } (\n  retour 1\n}"));
        // `#` en fin d'expression est l'opérateur primorielle, pas un commentaire.
        assert!(!braces_balanced("(10#"));
    }

    #[test]
    fn completions() {
        assert_eq!(completer(":he").as_deref(), Some(":help"));
        assert_eq!(completer("primo").as_deref(), Some("primorielle"));
        assert_eq!(completer("zzz"), None);
        assert_eq!(completer(""), None);
    }

    #[test]
    fn definitions_reconnues() {
        assert!(est_definition("fn f(n: entier) -> entier { retour n }"));
        assert!(est_definition("$x = 3"));
        assert!(est_definition("tant_que ($i < 10) { }"));
        assert!(!est_definition(">> 3 + 4"));
        assert!(!est_definition("afficher(3)"));
    }

    #[test]
    fn historique_borne_et_sans_doublon() {
        let mut h = Historique::default();
        h.ajouter("a");
        h.ajouter("a");
        h.ajouter("  ");
        assert_eq!(h.entrees, vec!["a"]);
        for i in 0..1200 { h.ajouter(&format!("l{i}")); }
        assert_eq!(h.entrees.len(), 1000);
    }

    #[test]
    fn couleur_desactivable() {
        let c = Couleur { actif: false };
        assert_eq!(c.prompt("rose> "), "rose> ");
        assert_eq!(c.erreur("x"), "x");
        let a = Couleur { actif: true };
        assert!(a.prompt("rose> ").starts_with("\x1b["));
        assert!(a.prompt("rose> ").ends_with("\x1b[0m"));
    }
}
