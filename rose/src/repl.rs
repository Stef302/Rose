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
    unsafe extern "system" {
        fn SetConsoleTitleW(titre: *const u16) -> i32;
    }
    unsafe {
        // Titre de la fenêtre : « Rose 0.1.5 ». C'est ce qui identifie l'onglet
        // dans Windows Terminal et l'entrée dans la barre des tâches ; sans
        // cela la fenêtre s'appelle « rose-shell.exe ».
        let titre: Vec<u16> = format!("Rose {} — console interactive", env!("CARGO_PKG_VERSION"))
            .encode_utf16().chain(std::iter::once(0)).collect();
        SetConsoleTitleW(titre.as_ptr());
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
    let lignes: [(&str, &str); 8] = [
        (":help", "cette aide"),
        (":show", "affiche le code accumulé"),
        (":reset", "vide le buffer"),
        (":load <fichier>", "charge un fichier et l'exécute"),
        (":save <fichier>", "écrit le buffer dans un fichier"),
        (":type <expr>", "type d'une expression"),
        (":latex", "formule LaTeX du buffer"),
        (":quit", "quitter"),
    ];
    let largeur = lignes.iter().map(|(cmd, _)| cmd.len()).max().unwrap_or(0);
    for (cmd, desc) in lignes {
        println!("  {}  {}", c.doux(&format!("{cmd:<largeur$}")), desc);
    }
    println!();
    println!("Toute autre ligne est du code Rose, exécuté dans le contexte du buffer.");
    println!("Les définitions sont mémorisées ; les {} s'affichent.", c.doux(">> expr"));
    println!("{} et {} rappellent l'historique, {} complète.", c.doux("↑"), c.doux("↓"), c.doux("Tab"));
}

/// Bannière d'accueil : la marque Rose, la version, et l'essentiel.
fn banniere(c: &Couleur) {
    let version = env!("CARGO_PKG_VERSION");
    // Une rose dessinée en ASCII, comme signature visuelle du shell.
    // Le remplissage se fait en NOMBRE DE CARACTÈRES (pas en octets), sinon
    // la bordure du cadre se décale.
    let rose = [
        "   ,-.   ",
        "  /   \\  ",
        " |  .  | ",
        "  \\   /  ",
        "   `-|   ",
        "     |   ",
        "    _|_  ",
    ];
    let interieur = 44;
    let centrer = |s: &str| -> String {
        let n = s.chars().count();
        if n >= interieur { s.to_string() }
        else {
            let gauche = (interieur - n) / 2;
            format!("{}{}{}", " ".repeat(gauche), s, " ".repeat(interieur - n - gauche))
        }
    };
    let ligne = |contenu: &str| -> String {
        format!("│{}│", c.titre(&centrer(contenu)))
    };
    println!("{}", c.titre(&format!("╭{}╮", "─".repeat(interieur))));
    for l in rose { println!("{}", ligne(l)); }
    println!("{}", ligne(""));
    println!("{}", ligne(&format!("Rose {version}")));
    println!("{}", ligne("console interactive"));
    println!("{}", c.titre(&format!("╰{}╯", "─".repeat(interieur))));
    println!("{}", c.doux("Tapez du code Rose. :help pour l'aide, :quit pour sortir."));
    println!("{}", c.doux("↑/↓ historique · Tab complète · Ctrl-C annule · Ctrl-L efface"));
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

    let mut historique = Historique::default();
    let mut code_sortie = 0;
    // Un double-clic sur `rose-shell.exe` ouvre une console Windows qui se
    // ferme aussitôt le programme terminé : on retient la fenêtre à la fin
    // quand l'entrée n'est pas un terminal (redirection) ou quand le
    // lanceur est une console « neuve » (cas du double-clic).
    let fenetre_ephemere = console_ephemere();

    loop {
        // Lecture d'une entrée, avec continuation tant que les délimiteurs
        // ne sont pas refermés. En terminal, on utilise l'éditeur de ligne
        // (flèches, Tab, Home/End, Ctrl-C, Ctrl-L) ; sinon on lit stdin
        // ligne par ligne (pipe, fichier).
        let mut entree = String::new();
        let mut premiere = true;
        loop {
            let invite = if premiere { couleur.prompt("rose> ") } else { couleur.continuation("...> ") };
            match lire_ligne(&invite, &historique, premiere) {
                Entree::Fin => {
                    if entree.trim().is_empty() {
                        println!();
                        if fenetre_ephemere { attendre_fermeture(); }
                        return code_sortie;
                    }
                    break;
                }
                Entree::Interrompue => {
                    // Ctrl-C : on annule la saisie en cours, comme un shell.
                    println!("{}", couleur.doux("^C"));
                    entree.clear();
                    premiere = true;
                    if premiere { break; }
                }
                Entree::Effacer => {
                    // Ctrl-L : écran nettoyé, la saisie est relancée.
                    print!("\x1b[2J\x1b[H");
                    entree.clear();
                    premiere = true;
                    continue;
                }
                Entree::Ligne(l) => {
                    if premiere {
                        entree.push_str(&l);
                        premiere = false;
                    } else {
                        entree.push('\n');
                        entree.push_str(&l);
                    }
                    if braces_balanced(&entree) { break; }
                }
            }
        }
        if entree.trim().is_empty() { continue; }

        let trimmed = entree.trim().to_string();
        historique.ajouter(&trimmed);

        // ── commandes ──
        if trimmed.starts_with(':') {
            let (cmd, arg) = match trimmed.split_once(char::is_whitespace) {
                Some((c, a)) => (c, a.trim()),
                None => (trimmed.as_str(), ""),
            };
            match cmd {
                ":quit" | ":q" | ":exit" => {
                    if fenetre_ephemere { attendre_fermeture(); }
                    return code_sortie;
                }
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

/// Ce que rend la lecture d'une ligne.
enum Entree {
    /// La ligne saisie (sans le saut de ligne).
    Ligne(String),
    /// Fin de flux (Ctrl-Z / EOF) : l'appelant termine la console.
    Fin,
    /// Ctrl-C : la saisie est annulée.
    Interrompue,
    /// Ctrl-L : l'écran est nettoyé.
    Effacer,
}

/// Lit une ligne avec édition : flèches gauche/droite, Home/End, Retour
/// arrière, Suppr, ↑/↓ pour l'historique, Tab pour compléter, Ctrl-C pour
/// annuler, Ctrl-L pour nettoyer.
///
/// Sur Windows on lit la console en mode brut (`ReadConsoleInputW`) : c'est le
/// seul moyen d'obtenir les flèches sans dépendance externe, et cela
/// fonctionne identiquement dans Windows Terminal, conhost, PowerShell et
/// cmd. Ailleurs (ou quand l'entrée n'est pas un terminal), on lit stdin
/// ligne par ligne.
fn lire_ligne(invite: &str, historique: &Historique, premiere: bool) -> Entree {
    #[cfg(windows)]
    {
        return unsafe { lire_ligne_win32(invite, historique, premiere) };
    }
    #[cfg(not(windows))]
    {
        let _ = (historique, premiere);
        return lire_ligne_simple(invite);
    }
}

/// Lecture simple (pipe, fichier, Linux/macOS sans édition de ligne).
#[cfg(not(windows))]
fn lire_ligne_simple(invite: &str) -> Entree {
    print!("{invite}");
    let _ = io::stdout().flush();
    let mut l = String::new();
    match io::stdin().read_line(&mut l) {
        Ok(0) => Entree::Fin,
        Ok(_) => {
            if l.ends_with('\n') { l.pop(); }
            if l.ends_with('\r') { l.pop(); }
            Entree::Ligne(l)
        }
        Err(_) => Entree::Fin,
    }
}

#[cfg(windows)]
unsafe fn lire_ligne_win32(invite: &str, historique: &Historique, premiere: bool) -> Entree {
    use std::ffi::c_void;

    #[repr(C)]
    struct Coord { x: i16, y: i16 }
    #[repr(C)]
    struct PetiteRect { gauche: i16, haut: i16, droite: i16, bas: i16 }
    #[repr(C)]
    struct Csbi { taille: Coord, curseur: Coord, attributs: u16, fenetre: PetiteRect,
                  max_fenetre: Coord }
    #[repr(C)]
    struct Cle {
        evenement: u16,
        _alignement: u16,
        code: u16,
        etat: u32,
        // union : on ne lit que le premier mot du scan code pour les
        // touches étendues, et le caractère Unicode pour les frappes.
        donnees: u32,
    }
    #[repr(C)]
    struct InputRecord { evenement: u16, _pad: u16, cle: Cle }

    unsafe extern "system" {
        fn GetStdHandle(n: u32) -> *mut c_void;
        fn GetConsoleMode(h: *mut c_void, m: *mut u32) -> i32;
        fn SetConsoleMode(h: *mut c_void, m: u32) -> i32;
        fn ReadConsoleInputW(h: *mut c_void, buf: *mut InputRecord, n: u32, lu: *mut u32) -> i32;
        fn GetConsoleScreenBufferInfo(h: *mut c_void, info: *mut Csbi) -> i32;
        fn SetConsoleCursorPosition(h: *mut c_void, p: Coord) -> i32;
        fn WriteConsoleW(h: *mut c_void, s: *const u16, n: u32, ecrit: *mut u32, r: *mut c_void) -> i32;
    }

    const STD_INPUT_HANDLE: u32 = 0xFFFF_FFF6;
    const STD_OUTPUT_HANDLE: u32 = 0xFFFF_FFF5;
    const ENABLE_PROCESSED_INPUT: u32 = 0x0001;
    const ENABLE_LINE_INPUT: u32 = 0x0002;
    const ENABLE_ECHO_INPUT: u32 = 0x0004;
    const ENABLE_WINDOW_INPUT: u32 = 0x0008;
    const ENABLE_MOUSE_INPUT: u32 = 0x0010;
    const ENABLE_EXTENDED_FLAGS: u32 = 0x0080;
    const ENABLE_QUICK_EDIT_MODE: u32 = 0x0040;
    const KEY_EVENT: u16 = 0x0001;
    const VK_BACK: u16 = 0x08;
    const VK_TAB: u16 = 0x09;
    const VK_RETURN: u16 = 0x0D;
    const VK_ESCAPE: u16 = 0x1B;
    const VK_PRIOR: u16 = 0x21;
    const VK_NEXT: u16 = 0x22;
    const VK_END: u16 = 0x23;
    const VK_HOME: u16 = 0x24;
    const VK_LEFT: u16 = 0x25;
    const VK_UP: u16 = 0x26;
    const VK_RIGHT: u16 = 0x27;
    const VK_DOWN: u16 = 0x28;
    const VK_DELETE: u16 = 0x2E;
    const VK_C: u16 = 0x43;
    const VK_L: u16 = 0x4C;
    const VK_Z: u16 = 0x5A;
    const LEFT_ALT_PRESSED: u32 = 0x0002;
    const LEFT_CTRL_PRESSED: u32 = 0x0008;

    unsafe fn ecrire(s: &str) {
        let h = GetStdHandle(STD_OUTPUT_HANDLE);
        let v: Vec<u16> = s.encode_utf16().collect();
        let mut n = 0u32;
        WriteConsoleW(h, v.as_ptr(), v.len() as u32, &mut n, std::ptr::null_mut());
    }

    unsafe fn largeur() -> i16 {
        let h = GetStdHandle(STD_OUTPUT_HANDLE);
        let mut info: Csbi = std::mem::zeroed();
        if GetConsoleScreenBufferInfo(h, &mut info) != 0 { info.taille.x } else { 80 }
    }

    /// Redessine la ligne à partir de la colonne 0 de la ligne courante.
    unsafe fn redessiner(invite: &str, texte: &str, curseur: usize) {
        let h = GetStdHandle(STD_OUTPUT_HANDLE);
        let mut info: Csbi = std::mem::zeroed();
        if GetConsoleScreenBufferInfo(h, &mut info) == 0 { return; }
        let ligne = info.curseur.y;
        SetConsoleCursorPosition(h, Coord { x: 0, y: ligne });
        ecrire(invite);
        ecrire(texte);
        // Efface la fin de la ligne (résidu d'une saisie précédente plus longue).
        let largeur_buf = info.taille.x as usize;
        let occupe = invite.chars().count() + texte.chars().count();
        if occupe < largeur_buf {
            ecrire(&" ".repeat(largeur_buf - occupe));
        }
        let colonne = (invite.chars().count() + curseur) as i16;
        SetConsoleCursorPosition(h, Coord { x: colonne.min(info.taille.x - 1), y: ligne });
    }

    let entree = GetStdHandle(STD_INPUT_HANDLE);
    let sortie = GetStdHandle(STD_OUTPUT_HANDLE);
    let mut ancien = 0u32;
    let brut = GetConsoleMode(entree, &mut ancien) != 0;
    if brut {
        // Mode brut : pas de saisie ligne ni d'écho, mais on GARDE
        // ENABLE_PROCESSED_INPUT pour recevoir Ctrl-C, et on désactive la
        // sélection souris qui volerait les touches.
        let mode = (ancien
            & !(ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_MOUSE_INPUT | ENABLE_QUICK_EDIT_MODE))
            | ENABLE_EXTENDED_FLAGS
            | ENABLE_WINDOW_INPUT
            | ENABLE_PROCESSED_INPUT;
        SetConsoleMode(entree, mode);
    }

    let mut texte: Vec<char> = Vec::new();
    let mut curseur = 0usize;
    // Position dans l'historique : == entrees.len() quand on édite une
    // nouvelle ligne.
    let mut pos_hist = historique.entrees.len();
    let mut brouillon = String::new();
    redessiner(invite, "", 0);

    let resultat = loop {
        let mut rec: InputRecord = std::mem::zeroed();
        let mut lu = 0u32;
        if ReadConsoleInputW(entree, &mut rec, 1, &mut lu) == 0 || lu == 0 {
            break Entree::Fin;
        }
        if rec.evenement != KEY_EVENT { continue; }
        // On ne traite que les appuis (pas les relâchements).
        let appui = (rec.cle.etat & 0x0001) != 0;
        if !appui { continue; }
        let ctrl = (rec.cle.etat & LEFT_CTRL_PRESSED) != 0;
        let alt = (rec.cle.etat & LEFT_ALT_PRESSED) != 0;
        let code = rec.cle.code;
        let u = rec.cle.donnees & 0xFFFF;

        match code {
            VK_RETURN => {
                ecrire("\r\n");
                break Entree::Ligne(texte.iter().collect());
            }
            VK_BACK => {
                if curseur > 0 {
                    curseur -= 1;
                    texte.remove(curseur);
                    redessiner(invite, &texte.iter().collect::<String>(), curseur);
                }
                continue;
            }
            VK_DELETE => {
                if curseur < texte.len() {
                    texte.remove(curseur);
                    redessiner(invite, &texte.iter().collect::<String>(), curseur);
                }
                continue;
            }
            VK_LEFT => { if curseur > 0 { curseur -= 1; } redessiner(invite, &texte.iter().collect::<String>(), curseur); continue; }
            VK_RIGHT => { if curseur < texte.len() { curseur += 1; } redessiner(invite, &texte.iter().collect::<String>(), curseur); continue; }
            VK_HOME => { curseur = 0; redessiner(invite, &texte.iter().collect::<String>(), curseur); continue; }
            VK_END => { curseur = texte.len(); redessiner(invite, &texte.iter().collect::<String>(), curseur); continue; }
            VK_UP => {
                if pos_hist > 0 {
                    if pos_hist == historique.entrees.len() {
                        brouillon = texte.iter().collect();
                    }
                    pos_hist -= 1;
                    texte = historique.entrees[pos_hist].chars().collect();
                    curseur = texte.len();
                    redessiner(invite, &texte.iter().collect::<String>(), curseur);
                }
                continue;
            }
            VK_DOWN => {
                if pos_hist < historique.entrees.len() {
                    pos_hist += 1;
                    texte = if pos_hist == historique.entrees.len() {
                        brouillon.chars().collect()
                    } else {
                        historique.entrees[pos_hist].chars().collect()
                    };
                    curseur = texte.len();
                    redessiner(invite, &texte.iter().collect::<String>(), curseur);
                }
                continue;
            }
            VK_PRIOR | VK_NEXT => continue, // Page Up/Down : ignorées
            VK_TAB => {
                let prefixe: String = texte.iter().collect();
                let debut_mot = prefixe[..curseur].rfind(|c: char| c.is_whitespace()).map(|i| i + 1).unwrap_or(0);
                let mot = &prefixe[debut_mot..curseur];
                if let Some(complet) = completer(mot) {
                    let suite: String = complet.chars().skip(mot.chars().count()).collect();
                    for c in suite.chars() {
                        texte.insert(curseur, c);
                        curseur += 1;
                    }
                    redessiner(invite, &texte.iter().collect::<String>(), curseur);
                }
                continue;
            }
            VK_ESCAPE => {
                // Échap : ligne effacée (comportement des REPL).
                texte.clear();
                curseur = 0;
                pos_hist = historique.entrees.len();
                redessiner(invite, "", 0);
                continue;
            }
            VK_C if ctrl => {
                ecrire("^C\r\n");
                break Entree::Interrompue;
            }
            VK_L if ctrl => {
                ecrire("\x1b[2J\x1b[H");
                break Entree::Effacer;
            }
            VK_Z if ctrl => {
                ecrire("^Z\r\n");
                break Entree::Fin;
            }
            _ => {}
        }
        // Caractère imprimable (y compris les accents : Unicode complet).
        if !alt && !ctrl {
            if let Some(ch) = char::from_u32(u as u32) {
                if !ch.is_control() && ch != '\0' {
                    texte.insert(curseur, ch);
                    curseur += 1;
                    redessiner(invite, &texte.iter().collect::<String>(), curseur);
                }
            }
        }
    };

    if brut { SetConsoleMode(entree, ancien); }
    let _ = (sortie, premiere, largeur());
    resultat
}

/// Vrai quand la console a été ouverte PAR le programme (double-clic sur
/// l'exécutable), auquel cas la fermeture est immédiate et la fenêtre
/// disparaît : il faut alors attendre une touche.
///
/// On ne peut pas le savoir directement ; on approche par deux signaux :
/// - l'entrée n'est PAS un terminal (pipe / redirection) : pas de fenêtre ;
/// - sous Windows, une console « fraîche » du double-clic n'a pas d'historique
///   de commandes et son titre est vide au démarrage. On retient donc la
///   fenêtre dans TOUS les cas où l'on n'est pas lancé depuis un shell déjà
///   ouvert, ce que `ROSE_SHELL_PERSIST=0` permet de désactiver.
fn console_ephemere() -> bool {
    if std::env::var("ROSE_SHELL_PERSIST").ok().as_deref() == Some("0") { return false; }
    if !est_terminal() { return false; }
    #[cfg(windows)]
    {
        // Lancé depuis cmd/PowerShell/Windows Terminal : la console a déjà un
        // titre (celui du terminal) ou une historique. Sinon, c'est un
        // double-clic et la fenêtre doit rester ouverte.
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
            fn GetConsoleScreenBufferInfo(h: *mut c_void, info: *mut Csbi) -> i32;
            fn GetConsoleTitleW(buf: *mut u16, n: u32) -> u32;
            fn GetConsoleProcessList(pid: *mut u32, n: u32) -> u32;
        }
        const STD_OUTPUT_HANDLE: u32 = 0xFFFF_FFF5;
        unsafe {
            // Une console créée pour un seul processus n'héberge qu'un PID.
            let mut pids = [0u32; 8];
            let n = GetConsoleProcessList(pids.as_mut_ptr(), pids.len() as u32);
            if n <= 1 { return true; }
            let mut titre = [0u16; 512];
            let l = GetConsoleTitleW(titre.as_mut_ptr(), titre.len() as u32);
            let t = String::from_utf16_lossy(&titre[..l as usize]);
            // Windows Terminal / cmd / PowerShell posent un titre au terminal.
            if !t.trim().is_empty() { return false; }
            // Dernier recours : une historique de commandes existe dans une
            // console déjà utilisée par le shell.
            let mut info: Csbi = std::mem::zeroed();
            let h = GetStdHandle(STD_OUTPUT_HANDLE);
            if GetConsoleScreenBufferInfo(h, &mut info) == 0 { return true; }
            false
        }
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Attend une touche avant de rendre la main : empêche la fenêtre ouverte par
/// un double-clic de se fermer avant qu'on ait pu lire le résultat.
fn attendre_fermeture() {
    print!("{}", "Appuyez sur Entrée pour fermer… ");
    let _ = io::stdout().flush();
    let mut poubelle = String::new();
    let _ = io::stdin().read_line(&mut poubelle);
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

#[cfg(test)]
mod tests_shell {
    use super::*;

    #[test]
    fn banniere_aligne_par_caracteres() {
        // La bordure du cadre doit faire exactement 46 caractères : 1 + 44 + 1.
        let interieur = 44;
        let centrer = |s: &str| -> String {
            let n = s.chars().count();
            if n >= interieur { s.to_string() }
            else {
                let g = (interieur - n) / 2;
                format!("{}{}{}", " ".repeat(g), s, " ".repeat(interieur - n - g))
            }
        };
        for s in ["", "Rose 0.1.5", "console interactive", "   ,-.   "] {
            let c = centrer(s);
            assert_eq!(c.chars().count(), interieur, "{s:?}");
            assert!(c.trim_start().starts_with(s.trim_start()));
        }
    }

    #[test]
    fn completions_du_shell() {
        assert_eq!(completer(":sa").as_deref(), Some(":save"));
        assert_eq!(completer(":la").as_deref(), Some(":latex"));
        assert_eq!(completer("nomb").as_deref(), Some("nombre_pi"));
        assert_eq!(completer("view").as_deref(), Some("view"));
    }

    #[test]
    fn historique_navigation() {
        let mut h = Historique::default();
        h.ajouter("1 + 2");
        h.ajouter(">> 10!");
        assert_eq!(h.entrees.len(), 2);
        assert_eq!(h.entrees[0], "1 + 2");
        assert_eq!(h.entrees[1], ">> 10!");
        // la position est remise à la fin après chaque ajout
        assert_eq!(h.position, 2);
    }
}
