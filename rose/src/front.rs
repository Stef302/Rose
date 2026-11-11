// Rose 2.0 — front-end fidèle au moteur C++ (moteur/cpp/src/rose.cpp).
// Lexeur, AST, vérificateur, repli de constantes — messages identiques.
// Licence MIT.

use std::collections::HashMap;

pub type Err = String;

pub fn fmt_err(msg: &str, l: usize, c: usize) -> Err {
    // préfixe d'affichage par le CLI, comme le moteur C++ :
    // « Rose : erreur — … » sur stderr.
    let corps = if l == 0 { msg.to_string() }
    else { format!("[Ligne {}, Colonne {}] {}", l, c, msg) };
    format!("Rose : erreur — {}", corps)
}

// ── Types ─────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TyKind { Vide, Entier, Rationnel, Flottant, Booleen, Chaine, Liste, Table, Tampon, Tableau, Complexe, Irrationnel, Dyn }

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Ty { pub n: TyKind, pub elt: TyKind }

impl Ty {
    pub fn vide() -> Ty { Ty { n: TyKind::Vide, elt: TyKind::Dyn } }
    pub fn simple(n: TyKind) -> Ty { Ty { n, elt: TyKind::Dyn } }
    pub fn liste(elt: TyKind) -> Ty { Ty { n: TyKind::Liste, elt } }
    #[allow(dead_code)]
    pub fn nom(&self) -> String {
        match self.n {
            TyKind::Vide => "vide".into(),
            TyKind::Entier => "entier".into(),
            TyKind::Rationnel => "rationnel".into(),
            TyKind::Flottant => "flottant".into(),
            TyKind::Booleen => "booleen".into(),
            TyKind::Chaine => "chaine".into(),
            TyKind::Liste => format!("liste<{}>", Ty::simple(self.elt).nom()),
            TyKind::Table => "table".into(),
            TyKind::Tampon => "tampon".into(),
            TyKind::Tableau => "tableau".into(),
            TyKind::Complexe => "complexe".into(),
            TyKind::Irrationnel => "irrationnel".into(),
            TyKind::Dyn => "?".into(),
        }
    }
}

fn rang(t: &Ty) -> u8 {
    match t.n {
        TyKind::Vide | TyKind::Entier | TyKind::Booleen => 1,
        TyKind::Rationnel => 2,
        TyKind::Flottant => 3,
        TyKind::Complexe => 4,
        _ => 5,
    }
}

pub fn promotion(a: &Ty, b: &Ty) -> Ty {
    if a.n == TyKind::Complexe || b.n == TyKind::Complexe { return Ty::simple(TyKind::Complexe); }
    match rang(a).max(rang(b)) {
        0 | 1 => Ty::simple(TyKind::Entier),
        2 => Ty::simple(TyKind::Rationnel),
        _ => Ty::simple(TyKind::Flottant),
    }
}

// ── Tokens ────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tk {
    Eof, Ident, Mot, Nombre, Flottant, Chaine,
    Plus, Moins, Etoile, Slash, Pourcent, Chapeau, Bang,
    DivEntier, Affect, LParen, RParen, LBrace, RBrace, LBracket, RBracket,
    Semi, Virgule, DeuxPts, DeuxPoints, FlecheD, FlecheG,
    PointDInterro,
    Inf, Sup, InfEg, SupEg, DoubleEg, Diff, Environ, Racine, Tilde, Arobase, Question,
}

#[derive(Clone, Debug)]
pub struct Token { pub tk: Tk, pub texte: String, pub ligne: usize, pub col: usize }

fn est_mot_cle(s: &str) -> bool {
    matches!(s,
        "fn" | "fonction" | "retour" | "si" | "sinon" | "tant_que" | "pour" | "de" | "vers"
        | "vrai" | "faux" | "et" | "ou" | "non" | "div"
        | "entier" | "rationnel" | "flottant" | "booleen" | "chaine" | "liste" | "table"
                | "π" | "φ" | "i" | "time")
}

fn cp_lettre(c: char) -> bool {
    if (c as u32) < 0x80 { return c.is_ascii_alphabetic() || c == '_'; }
    let u = c as u32;
    (0xC0..=0xD6).contains(&u) || (0xD8..=0xF6).contains(&u) || (0xF8..=0x2FF).contains(&u)
        || (0x370..=0x4FF).contains(&u) || (0x1E00..=0x1FFF).contains(&u)
}
fn cp_ident_suite(c: char) -> bool {
    if (c as u32) < 0x80 { return c.is_ascii_alphanumeric() || c == '_'; }
    cp_lettre(c)
}

fn paire(a: char, b: char) -> Option<Tk> {
    Some(match (a, b) {
        ('<', '=') => Tk::InfEg,
        ('>', '=') => Tk::SupEg,
        ('=', '=') => Tk::DoubleEg,
        ('!', '=') => Tk::Diff,
        ('/', '=') => Tk::Diff,
        ('?', '=') => Tk::Environ,
        ('=', '~') => Tk::Environ,
        ('~', '=') => Tk::Environ,
        _ => return None,
    })
}

fn uni(c: char) -> Option<Tk> {
    Some(match c {
        '≈' => Tk::Environ,
        '≤' => Tk::InfEg,
        '≥' => Tk::SupEg,
        '≠' => Tk::Diff,
        '×' | '⋅' | '·' => Tk::Etoile,
        '÷' => Tk::DivEntier,
        '←' => Tk::Affect,
        '→' => Tk::PointDInterro,
        '√' => Tk::Racine,
        'π' => Tk::Mot,
        'φ' => Tk::Mot,
        _ => return None,
    })
}

fn car(c: char) -> Option<Tk> {
    Some(match c {
        '+' => Tk::Plus, '-' => Tk::Moins, '*' => Tk::Etoile, '/' => Tk::Slash,
        ':' => Tk::DeuxPts, '%' => Tk::Pourcent, '^' => Tk::Chapeau, '!' => Tk::Bang,
        ',' => Tk::Virgule, '?' => Tk::Question,
        '(' => Tk::LParen, ')' => Tk::RParen, '{' => Tk::LBrace, '}' => Tk::RBrace,
        '[' => Tk::LBracket, ']' => Tk::RBracket, ';' => Tk::Semi,
        '<' => Tk::Inf, '>' => Tk::Sup, '~' => Tk::Tilde, '@' => Tk::Arobase,
        _ => return None,
    })
}

pub fn tokeniser(source: &str) -> Result<Vec<Token>, Err> {
    let cps: Vec<char> = source.chars().collect();
    let n = cps.len();
    let mut tk: Vec<Token> = Vec::new();
    let (mut i, mut ligne, mut col): (usize, usize, usize) = (0, 1, 1);

    macro_rules! err {
        ($msg:expr, $l:expr, $c:expr) => { return Err(fmt_err(&$msg, $l, $c)) };
    }

    while i < n {
        let c = cps[i];
        if c == ' ' || c == '\t' || c == '\r' || c == '\n' { i += 1; if c == '\n' { ligne += 1; col = 1; } else { col += 1; } continue; }
        if c == '#' || (c == '/' && i + 1 < n && cps[i + 1] == '/') {
            while i < n && cps[i] != '\n' { i += 1; } continue;
        }
        if c == '"' || c == '\'' {
            let quote = c;
            let (dl, dc) = (ligne, col);
            i += 1; col += 1;
            let mut buf = String::new();
            loop {
                if i >= n { err!("Chaîne non terminée".to_string(), dl, dc); }
                let d = cps[i];
                if d == '\\' {
                    if i + 1 >= n { err!("Échappement en fin de fichier".to_string(), ligne, col); }
                    let nx = cps[i + 1];
                    match nx {
                        'n' => buf.push('\n'),
                        't' => buf.push('\t'),
                        '\\' | '"' | '\'' => buf.push(nx),
                        other => { buf.push('\\'); buf.push(other); }
                    }
                    i += 2; col += 2; continue;
                }
                if d == quote { i += 1; col += 1; break; }
                buf.push(d); i += 1; col += 1;
            }
            tk.push(Token { tk: Tk::Chaine, texte: buf, ligne: dl, col: dc });
            continue;
        }
        if c.is_ascii_digit() {
            let (dl, dc) = (ligne, col);
            let debut = i;
            while i < n && cps[i].is_ascii_digit() { i += 1; col += 1; }
            let mut flott = false;
            if i + 1 < n && cps[i] == '.' && cps[i + 1].is_ascii_digit() {
                flott = true;
                i += 1; col += 1;
                while i < n && cps[i].is_ascii_digit() { i += 1; col += 1; }
            }
            let texte: String = cps[debut..i].iter().collect();
            tk.push(Token { tk: if flott { Tk::Flottant } else { Tk::Nombre }, texte, ligne: dl, col: dc });
            continue;
        }
        if c == 'π' {
            tk.push(Token { tk: Tk::Mot, texte: "π".into(), ligne, col });
            i += 1; col += 1; continue;
        }
        if c == 'φ' {
            tk.push(Token { tk: Tk::Mot, texte: "φ".into(), ligne, col });
            i += 1; col += 1; continue;
        }
        if c == '$' || cp_lettre(c) {
            let (dl, dc) = (ligne, col);
            let debut = i;
            let mut dollar = false;
            if c == '$' { dollar = true; i += 1; col += 1; }
            if i >= n || !cp_lettre(cps[i]) {
                err!("Attente d'un nom de variable après '$'".to_string(), dl, dc);
            }
            while i < n && cp_ident_suite(cps[i]) { i += 1; col += 1; }
            let texte: String = cps[debut..i].iter().collect();
            if dollar {
                // le C++ n'interdit pas les mots-cles apres '$' ($de, $si sont des
                // noms valides) — miroir strict, aucun controle ici.
                let nu = texte[1..].to_string();
                tk.push(Token { tk: Tk::Ident, texte: nu, ligne: dl, col: dc });
            } else {
                tk.push(Token { tk: Tk::Mot, texte, ligne: dl, col: dc });
            }
            continue;
        }
        if i + 1 < n {
            if let Some(t) = paire(c, cps[i + 1]) {
                let texte: String = cps[i..i + 2].iter().collect();
                tk.push(Token { tk: t, texte, ligne, col });
                i += 2; col += 2; continue;
            }
        }
        if c == '>' && i + 1 < n && cps[i + 1] == '>' {
            tk.push(Token { tk: Tk::FlecheD, texte: ">>".into(), ligne, col });
            i += 2; col += 2; continue;
        }
        if c == '<' && i + 1 < n && cps[i + 1] == '<' {
            tk.push(Token { tk: Tk::FlecheG, texte: "<<".into(), ligne, col });
            i += 2; col += 2; continue;
        }
        if c == '-' && i + 1 < n && cps[i + 1] == '>' {
            // '->' de signature : émis comme deux tokens (comme le C++, qui a un
            // token PointDInterro ; on garde '-' puis '>' et le parser accepte)
            tk.push(Token { tk: Tk::Moins, texte: "-".into(), ligne, col });
            tk.push(Token { tk: Tk::Sup, texte: ">".into(), ligne, col: col + 1 });
            i += 2; col += 2; continue;
        }
        if c == '=' { tk.push(Token { tk: Tk::Affect, texte: "=".into(), ligne, col }); i += 1; col += 1; continue; }
        if c == ':' && i + 1 < n && cps[i + 1] == ':' {
            tk.push(Token { tk: Tk::DeuxPoints, texte: "::".into(), ligne, col });
            i += 2; col += 2; continue;
        }
        if c == '²' || c == '³' {
            tk.push(Token { tk: Tk::Chapeau, texte: "^".into(), ligne, col });
            tk.push(Token { tk: Tk::Nombre, texte: if c == '²' { "2".into() } else { "3".into() }, ligne, col });
            i += 1; col += 1; continue;
        }
        if let Some(t) = uni(c) {
            tk.push(Token { tk: t, texte: c.to_string(), ligne, col });
            i += 1; col += 1; continue;
        }
        if let Some(t) = car(c) {
            tk.push(Token { tk: t, texte: c.to_string(), ligne, col });
            i += 1; col += 1; continue;
        }
        err!(format!("Caractère inattendu '{}'", c), ligne, col);
    }
    tk.push(Token { tk: Tk::Eof, texte: String::new(), ligne, col });
    crate::run::noter_ligne(ligne.max(1));
    Ok(tk)
}

// ── AST ───────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub enum Expr {
    Litteral { texte: String, flott: bool, rationnel: bool, neg: bool, ty: TyKind, ligne: usize },
    LitComplexe { re: f64, im: f64, ligne: usize },
    ChaineLit(String, usize),
    BooleenLit(bool, usize),
    Var(String, usize),
    Appel { nom: String, module: String, args: Vec<Expr>, ligne: usize },
    Binaire { op: String, a: Box<Expr>, b: Box<Expr>, ligne: usize },
    Unaire { op: String, a: Box<Expr>, ligne: usize },
    Factorielle(Box<Expr>, usize),
    Index { a: Box<Expr>, b: Box<Expr>, ligne: usize },
    ListeLit(Vec<Expr>, usize),
    TableLit(Vec<(String, Expr)>, usize),
}

/// « i » est un mot réservé replié en 0+1i par le lexeur ; mais dans une
/// condition de boucle (« tant_que (i < n) »), c'est le nom d'une VARIABLE.
/// On dé-replie le littéral imaginaire pur en variable « i » lorsque l'autre
/// côté de la comparaison est un nombre : un vrai usage de l'imaginaire ne se
/// compare jamais avec « < ».
pub fn est_lit_i(e: &Expr) -> bool {
    matches!(e, Expr::LitComplexe { re, im, .. } if *re == 0.0 && *im == 1.0)
}

pub fn est_surement_nombre(e: &Expr) -> bool {
    match e {
        Expr::Litteral { .. } => true,
        Expr::Var(n, _) => !matches!(n.as_str(), "pi" | "phi" | "e"),
        _ => false,
    }
}


pub fn deplier_i_test(e: &mut Expr) {
    if let Expr::Binaire { op, a, b, .. } = e {
        let ordre = matches!(op.as_str(), "<" | "<=" | ">" | ">=");
        let compare_i = (est_lit_i(a) && est_surement_nombre(b))
            || (est_lit_i(b) && est_surement_nombre(a));
        if ordre && compare_i {
            // La variable de boucle s'appelle « i » : tout littéral imaginaire
            // pur de la condition est en réalité cette variable.
            deplier_i_rec(a);
            deplier_i_rec(b);
        }
    }
}

pub fn deplier_i_rec(e: &mut Expr) {
    match e {
        // dans le contexte d'une boucle dont « i » est le compteur entier,
        // le littéral 0+1i (jeton « i » replié par le lexeur) est la VARIABLE.
        Expr::LitComplexe { re, im, ligne } if *re == 0.0 && *im == 1.0 => {
            *e = Expr::Var("i".into(), *ligne);
        }
        Expr::Binaire { a, b, .. } | Expr::Index { a, b, .. } => { deplier_i_rec(a); deplier_i_rec(b); }
        Expr::Unaire { a, .. } | Expr::Factorielle(a, _) => deplier_i_rec(a),
        Expr::ListeLit(v, _) => { for x in v { deplier_i_rec(x); } }
        Expr::Appel { args, .. } => { for x in args { deplier_i_rec(x); } }
        Expr::TableLit(v, _) => { for (_, x) in v { deplier_i_rec(x); } }
        _ => {}
    }
}

/// Ligne d'un noeud d'expression (miroir de e->ligne du C++) : sert aux
/// messages d'erreur du vérificateur et du repli de constantes.
pub fn ligne_expr(e: &Expr) -> usize {
    match e {
        Expr::Litteral { ligne, .. } => *ligne,
        Expr::LitComplexe { ligne, .. } => *ligne,
        Expr::ChaineLit(_, l) | Expr::BooleenLit(_, l) | Expr::Var(_, l) => *l,
        Expr::Appel { ligne, .. } | Expr::Binaire { ligne, .. } => *ligne,
        Expr::Unaire { ligne, .. } | Expr::Index { ligne, .. } => *ligne,
        Expr::Factorielle(_, l) | Expr::ListeLit(_, l) => *l,
        Expr::TableLit(_, l) => *l,
    }
}

#[derive(Clone, Debug)]
pub enum Instr {
    Afficher(Vec<Expr>, usize),
    Lire { nom: String, ty: TyKind, ligne: usize },
    Affect { nom: String, e: Expr, ligne: usize },
    AffectIndex { nom: String, index: Expr, valeur: Expr, ligne: usize },
    Si { test: Expr, alors: Vec<Instr>, sinon: Option<Vec<Instr>>, ligne: usize },
    TantQue { test: Expr, corps: Vec<Instr>, ligne: usize },
    Pour { nom: String, debut: Expr, fin: Expr, corps: Vec<Instr>, ligne: usize },
    Retour(Option<Expr>, usize),
    ExprStmt(Expr),
    Bloc(Vec<Instr>),
    Spawn(Vec<Instr>, usize),
    Essayer { corps: Vec<Instr>, attraper: Option<(String, Vec<Instr>)>, finalment: Option<Vec<Instr>>, ligne: usize },
    Lever(Expr, usize),
}

#[derive(Clone, Debug)]
pub struct Signature {
    pub nom: String,
    pub params: Vec<(String, Ty)>,
    pub retour: Ty,
    #[allow(dead_code)]
    pub a_retour: bool,
    pub corps: Option<Vec<Instr>>,
    #[allow(dead_code)]
    pub dll: String,
    #[allow(dead_code)]
    pub dll_son: String,
    #[allow(dead_code)]
    pub symbole: String,
    pub ligne: usize,
}

#[derive(Debug, Clone)]
pub struct Programme {
    pub signatures: HashMap<String, Signature>,
    pub ordre: Vec<String>,
    pub instructions: Vec<Instr>,
}

impl Default for Programme {
    fn default() -> Self {
        Programme { signatures: HashMap::new(), ordre: Vec::new(), instructions: Vec::new() }
    }
}

// ── Bibliothèque standard (table STDLIB du C++) ───────────────────────────

pub struct EntreeStd { pub nom: &'static str, pub args: &'static [&'static str], pub retour: &'static str }

pub static STDLIB: &[EntreeStd] = &[
    EntreeStd { nom: "afficher", args: &["n"], retour: "vide" },
    EntreeStd { nom: "saisir_entier", args: &[], retour: "entier" },
    EntreeStd { nom: "saisir_flottant", args: &[], retour: "flottant" },
    EntreeStd { nom: "factorielle", args: &["entier"], retour: "entier" },
EntreeStd { nom: "fact", args: &["entier"], retour: "entier" },
    EntreeStd { nom: "fibonacci", args: &["entier"], retour: "entier" },
    EntreeStd { nom: "fib", args: &["entier"], retour: "entier" },
    EntreeStd { nom: "pgcd", args: &["entier", "entier"], retour: "entier" },
EntreeStd { nom: "gcd", args: &["entier", "entier"], retour: "entier" },
    EntreeStd { nom: "ppcm", args: &["entier", "entier"], retour: "entier" },
EntreeStd { nom: "lcm", args: &["entier", "entier"], retour: "entier" },
    EntreeStd { nom: "puissance", args: &["num", "entier"], retour: "entier" },
EntreeStd { nom: "pow", args: &["num", "entier"], retour: "entier" },
    EntreeStd { nom: "racine", args: &["entier"], retour: "irrationnel" },
    EntreeStd { nom: "est_premier", args: &["entier"], retour: "booleen" },
EntreeStd { nom: "is_prime", args: &["entier"], retour: "booleen" },
    EntreeStd { nom: "compter_premiers", args: &["entier"], retour: "entier" },
    EntreeStd { nom: "nb_chiffres", args: &["entier"], retour: "entier" },
EntreeStd { nom: "digits", args: &["entier"], retour: "entier" },
    EntreeStd { nom: "nombre_pi", args: &["entier"], retour: "chaine" },
    EntreeStd { nom: "pi", args: &["entier"], retour: "chaine" },
    EntreeStd { nom: "nombre_e", args: &["entier"], retour: "chaine" },
    EntreeStd { nom: "e", args: &["entier"], retour: "chaine" },
    EntreeStd { nom: "phi", args: &["entier"], retour: "chaine" },
    EntreeStd { nom: "__i", args: &[], retour: "complexe" },
    EntreeStd { nom: "i", args: &[], retour: "complexe" },
    EntreeStd { nom: "re", args: &["complexe"], retour: "flottant" },
    EntreeStd { nom: "im", args: &["complexe"], retour: "flottant" },
    EntreeStd { nom: "conj", args: &["complexe"], retour: "complexe" },
    EntreeStd { nom: "arg", args: &["complexe"], retour: "flottant" },
    EntreeStd { nom: "modulus", args: &["complexe"], retour: "flottant" },
    EntreeStd { nom: "longueur", args: &["liste_ou_chaine_ou_table"], retour: "entier" },
EntreeStd { nom: "len", args: &["liste_ou_chaine_ou_table"], retour: "entier" },
    EntreeStd { nom: "table", args: &[], retour: "table" },
EntreeStd { nom: "map", args: &[], retour: "map" },
    EntreeStd { nom: "inserer", args: &["table", "tout", "tout"], retour: "vide" },
    EntreeStd { nom: "cle_existe", args: &["table", "tout"], retour: "booleen" },
    EntreeStd { nom: "supprimer", args: &["table", "tout"], retour: "booleen" },
    EntreeStd { nom: "cles", args: &["table"], retour: "MIXTE" },
    EntreeStd { nom: "valeurs", args: &["table"], retour: "MIXTE" },
    EntreeStd { nom: "joindre", args: &["liste", "liste_elt"], retour: "vide" },
    EntreeStd { nom: "ordre", args: &["chaine"], retour: "entier" },
    EntreeStd { nom: "caractere", args: &["entier"], retour: "chaine" },
    EntreeStd { nom: "sous_chaine", args: &["chaine", "entier", "entier"], retour: "chaine" },
EntreeStd { nom: "sub", args: &["chaine", "entier", "entier"], retour: "chaine" },
    EntreeStd { nom: "concatener", args: &["liste"], retour: "chaine" },
    EntreeStd { nom: "lire_texte", args: &["chaine"], retour: "chaine" },
    EntreeStd { nom: "eval_expr", args: &["chaine"], retour: "chaine" },
    EntreeStd { nom: "ecrire_texte", args: &["chaine", "chaine"], retour: "vide" },
    EntreeStd { nom: "ecrire_binaire", args: &["chaine", "liste"], retour: "vide" },
    EntreeStd { nom: "ecrire_bytes", args: &["chaine", "chaine"], retour: "vide" },
    EntreeStd { nom: "nb_arguments", args: &[], retour: "entier" },
    EntreeStd { nom: "argument", args: &["entier"], retour: "chaine" },
    EntreeStd { nom: "temps", args: &[], retour: "entier" },
    EntreeStd { nom: "duree", args: &["entier"], retour: "chaine" },
    EntreeStd { nom: "afficher_erreur", args: &["tout"], retour: "vide" },
    EntreeStd { nom: "quitter", args: &["entier"], retour: "vide" },
    EntreeStd { nom: "breakpoint", args: &[], retour: "vide" },
    EntreeStd { nom: "breakpoint_if", args: &["booleen"], retour: "vide" },
    EntreeStd { nom: "watch", args: &["chaine"], retour: "vide" },
    EntreeStd { nom: "unwatch", args: &["chaine"], retour: "vide" },
    EntreeStd { nom: "join", args: &["entier"], retour: "vide" },
    EntreeStd { nom: "abs", args: &["num"], retour: "MIXTE" },
    EntreeStd { nom: "min", args: &["num", "num"], retour: "NUM_MAX" },
    EntreeStd { nom: "max", args: &["num", "num"], retour: "NUM_MAX" },
    EntreeStd { nom: "partie_entiere", args: &["num"], retour: "entier" },
    EntreeStd { nom: "fraction", args: &["entier", "entier"], retour: "rationnel" },
    EntreeStd { nom: "graine", args: &["entier"], retour: "vide" },
EntreeStd { nom: "seed", args: &["entier"], retour: "vide" },
    EntreeStd { nom: "graine_systeme", args: &[], retour: "vide" },
    EntreeStd { nom: "graine_vrai", args: &[], retour: "vide" },
    EntreeStd { nom: "hasard", args: &["entier"], retour: "entier" },
    EntreeStd { nom: "prng", args: &["entier"], retour: "entier" },
    EntreeStd { nom: "hasard_systeme", args: &["entier"], retour: "entier" },
    EntreeStd { nom: "trng", args: &["entier"], retour: "entier" },
    EntreeStd { nom: "hasard_vrai", args: &["entier"], retour: "entier" },
    EntreeStd { nom: "rng_materiel", args: &[], retour: "entier" },
    EntreeStd { nom: "rng_mat", args: &[], retour: "entier" },
    EntreeStd { nom: "tampon", args: &["entier"], retour: "tampon" },
    EntreeStd { nom: "tableau", args: &["entier"], retour: "tableau" },
    EntreeStd { nom: "gpu_somme", args: &["tableau"], retour: "entier" },
EntreeStd { nom: "buf", args: &["entier"], retour: "buf" },
    EntreeStd { nom: "octets", args: &["chaine"], retour: "tampon" },
EntreeStd { nom: "bytes", args: &["chaine"], retour: "tampon" },
    EntreeStd { nom: "tampon_case", args: &["tampon", "entier"], retour: "entier" },
    EntreeStd { nom: "tampon_pose", args: &["tampon", "entier", "entier"], retour: "vide" },
    EntreeStd { nom: "remplir_hasard", args: &["tampon", "tampon"], retour: "vide" },
    EntreeStd { nom: "remplir_hasard_philox", args: &["tampon", "tampon", "entier"], retour: "vide" },
    EntreeStd { nom: "millisecondes", args: &[], retour: "entier" },
    EntreeStd { nom: "time", args: &[], retour: "chaine" },
EntreeStd { nom: "ms", args: &[], retour: "entier" },
];

pub fn chercher_std(nom: &str) -> Option<&'static EntreeStd> {
    STDLIB.iter().find(|e| e.nom == nom)
}

pub fn est_std(nom: &str) -> bool {
    chercher_std(nom).is_some()
        || matches!(nom, "charger" | "fermer" | "appeler0" | "appeler1" | "appeler2")
}

pub fn ty_du_nom(s: &str) -> Ty {
    match s {
        "entier" => Ty::simple(TyKind::Entier),
        "rationnel" => Ty::simple(TyKind::Rationnel),
        "flottant" => Ty::simple(TyKind::Flottant),
        "booleen" => Ty::simple(TyKind::Booleen),
        "chaine" => Ty::simple(TyKind::Chaine),
        "table" => Ty::simple(TyKind::Table),
        "tampon" => Ty::simple(TyKind::Tampon),
        "tableau" => Ty::simple(TyKind::Tableau),
        "complexe" => Ty::simple(TyKind::Complexe),
        "irrationnel" => Ty::simple(TyKind::Irrationnel),
        _ => Ty::vide(),
    }
}

// ── Analyseur ─────────────────────────────────────────────────────────────

pub struct Parser { tk: Vec<Token>, pos: usize }

impl Parser {
    pub fn nouveau(tk: Vec<Token>) -> Self { Parser { tk, pos: 0 } }

    fn voir(&self, d: usize) -> &Token {
        let p = (self.pos + d).min(self.tk.len() - 1);
        &self.tk[p]
    }
    fn revenir(&mut self) { if self.pos > 0 { self.pos -= 1; } }
    fn suivant(&mut self) -> Token {
        let t = self.voir(0).clone();
        if t.tk != Tk::Eof { self.pos += 1; }
        t
    }
    fn est(&self, tt: &[Tk]) -> bool { tt.contains(&self.voir(0).tk) }
    fn est_mot(&self, texte: &str) -> bool {
        let t = self.voir(0);
        (t.tk == Tk::Ident || t.tk == Tk::Mot) && t.texte == texte
    }
    fn accepter(&mut self, tt: &[Tk]) -> bool {
        if self.est(tt) { self.suivant(); true } else { false }
    }
    fn accepter_mot(&mut self, texte: &str) -> bool {
        if self.est_mot(texte) { self.suivant(); true } else { false }
    }
    fn exiger(&mut self, tt: Tk, quoi: &str) -> Result<Token, Err> {
        let t = self.suivant();
        if t.tk != tt {
            return Err(fmt_err(&format!("Attendu {}, trouvé '{}'", quoi, t.texte), t.ligne, t.col));
        }
        Ok(t)
    }
    fn nom_ident(&mut self) -> Result<String, Err> {
        let t = self.suivant();
        if t.tk != Tk::Ident && t.tk != Tk::Mot {
            return Err(fmt_err(&format!("Attendu un identifiant, trouvé '{}'", t.texte), t.ligne, t.col));
        }
        Ok(t.texte)
    }

    pub fn programmer(&mut self) -> Result<Programme, Err> {
        self.programme_int()
    }
    fn programme_int(&mut self) -> Result<Programme, Err> {
        let mut prog = Programme::default();
        let mut ann_dll = String::new();
        let mut ann_dll_son = String::new();
        let mut ann_sym = String::new();
        let mut ann_actif = false;
        while !self.est(&[Tk::Eof]) {
            if self.accepter(&[Tk::Semi]) { continue; }
            {
                let t0 = self.voir(0);
                let t1 = self.voir(1);
                if t0.tk == Tk::Arobase && (t1.tk == Tk::Ident || t1.tk == Tk::Mot)
                    && (t1.texte == "dll" || t1.texte == "sym") {
                    self.suivant();
                    let genre = self.nom_ident()?;
                    self.exiger(Tk::LParen, "'(' après @dll/@sym")?;
                    if genre == "dll" {
                        ann_dll = self.exiger(Tk::Chaine, "nom de bibliothèque")?.texte;
                        if self.accepter(&[Tk::Virgule]) {
                            ann_dll_son = self.exiger(Tk::Chaine, "nom de fichier")?.texte;
                        }
                    } else {
                        ann_sym = self.exiger(Tk::Chaine, "nom de symbole")?.texte;
                    }
                    self.exiger(Tk::RParen, "')'")?;
                    ann_actif = true;
                    continue;
                }
            }
            if self.est_mot("fn") || self.est_mot("fonction") {
                let sig = self.def_fonction(&mut ann_dll, &mut ann_dll_son, &mut ann_sym, &mut ann_actif)?;
                if prog.signatures.contains_key(&sig.nom) {
                    return Err(fmt_err(&format!("Fonction '{}' définie deux fois", sig.nom), sig.ligne, 0));
                }
                prog.ordre.push(sig.nom.clone());
                prog.signatures.insert(sig.nom.clone(), sig);
                continue;
            }
            if ann_actif {
                let t = self.voir(0);
                return Err(fmt_err("Annotation @dll/@sym sans fonction juste après", t.ligne, t.col));
            }
            let avant = self.pos;
            let instr = self.instruction()?;
            if self.pos <= avant {
                let t = &self.tk[avant.min(self.tk.len()-1)];
                return Err(fmt_err(&format!("jeton inattendu '{}' (ligne {})", t.texte, t.ligne), t.ligne, t.col));
            }
            prog.instructions.push(instr);
        }
        Ok(prog)
    }

    #[allow(clippy::too_many_arguments)]
    fn def_fonction(&mut self, ann_dll: &mut String, ann_dll_son: &mut String,
                    ann_sym: &mut String, ann_actif: &mut bool) -> Result<Signature, Err> {
        let tok = self.suivant(); // 'fn'
        let nom = self.nom_ident()?;
        self.exiger(Tk::LParen, "'('")?;
        let mut params = Vec::new();
        if !self.est(&[Tk::RParen]) {
            loop {
                let pname = self.nom_ident()?;
                // Type OPTIONNEL : « fn f(n) » (sans « : type ») est accepté,
                // le type est alors dynamique (Dyn) — le moteur le résout à
                // l'exécution. C'est la forme allégée demandée pour les
                // fonctions simples.
                let pt = if self.accepter(&[Tk::DeuxPts]) {
                    self.type_rose()?
                } else {
                    Ty::simple(TyKind::Dyn)
                };
                params.push((pname, pt));
                if !self.accepter(&[Tk::Virgule]) { break; }
            }
        }
        self.exiger(Tk::RParen, "')'")?;
        // Type de retour OPTIONNEL : « fn f(x) { ... } » (sans « -> type »)
        // est accepté — le type de retour est alors dynamique (Dyn) et la
        // valeur rendue est convertie au type du site d'appel, comme pour les
        // paramètres sans type déclaré. C'est la forme allégée demandée
        // pour les fonctions simples.
        let mut retour = Ty::simple(TyKind::Vide);
        let mut a_retour = false;
        // '->' (émis comme Moins + Sup, '→' ou '←'…)
        if (self.voir(0).tk == Tk::Moins && self.voir(1).tk == Tk::Sup)
            || self.voir(0).tk == Tk::PointDInterro {
            if self.voir(0).tk == Tk::Moins { self.suivant(); }
            self.suivant();
            retour = self.type_rose()?;
            a_retour = true;
        }
        let corps = if self.est(&[Tk::LBrace]) {
            Some(self.bloc()?)
        } else {
            if !a_retour {
                return Err(fmt_err("Une fonction externe doit déclarer un type de retour", tok.ligne, 0));
            }
            None
        };
        // C++ ne force a_retour qu'EN ABSENCE de corps déclaré (rose.cpp:617,
        // dans la branche « corps ») ; avec un corps, a_retour reste faux tant
        // que « -> type » est absent — le vérificateur s'en sert pour refuser
        // « retour » hors de fonction et exiger le type de retour déclaré.
        if corps.is_none() && !a_retour { retour = Ty::simple(TyKind::Vide); a_retour = true; }
        // Fonction avec corps mais sans type de retour déclaré : on passe
        // le retour en Dyn (type dynamique) pour que « retour expr » soit
        // accepté sans contrôle strict (le type est résolu à l'exécution).
        if corps.is_some() && !a_retour { retour = Ty::simple(TyKind::Dyn); }
        let sig = Signature {
            nom, params, retour, a_retour, corps,
            dll: std::mem::take(ann_dll), dll_son: std::mem::take(ann_dll_son),
            symbole: std::mem::take(ann_sym), ligne: tok.ligne,
        };
        *ann_actif = false;
        Ok(sig)
    }

    fn type_rose(&mut self) -> Result<Ty, Err> {
        let t = self.suivant();
        if t.tk != Tk::Ident && t.tk != Tk::Mot {
            return Err(fmt_err(&format!("Attendu un type, trouvé '{}'", t.texte), t.ligne, t.col));
        }
        match t.texte.as_str() {
            "liste" => {
                self.exiger(Tk::Inf, "'<' de liste<T>")?;
                let elt = self.type_rose()?;
                self.exiger(Tk::Sup, "'>' de liste<T>")?;
                Ok(Ty::liste(elt.n))
            }
            "entier" => Ok(Ty::simple(TyKind::Entier)),
            "rationnel" => Ok(Ty::simple(TyKind::Rationnel)),
            "flottant" => Ok(Ty::simple(TyKind::Flottant)),
            "booleen" => Ok(Ty::simple(TyKind::Booleen)),
            "chaine" => Ok(Ty::simple(TyKind::Chaine)),
            "table" => Ok(Ty::simple(TyKind::Table)),
            "complexe" => Ok(Ty::simple(TyKind::Complexe)),
            "irrationnel" => Ok(Ty::simple(TyKind::Irrationnel)),
            other => Err(fmt_err(&format!("Type inconnu '{}'", other), t.ligne, t.col)),
        }
    }

    fn instruction(&mut self) -> Result<Instr, Err> {
        let t = self.voir(0).clone();
        match t.tk {
            Tk::LBrace => return Ok(Instr::Bloc(self.bloc()?)),
            Tk::FlecheD => {
                let lg = self.voir(0).ligne; self.suivant();
                let mut exprs = vec![self.expression()?];
                while self.accepter(&[Tk::Virgule]) {
                    exprs.push(self.expression()?);
                }
                return Ok(Instr::Afficher(exprs, lg))
            }
            Tk::FlecheG => {
                self.suivant();
                let nom = self.nom_ident()?;
                return Ok(Instr::Lire { nom, ty: TyKind::Dyn, ligne: t.ligne });
            }
            Tk::Question => {
                self.suivant();
                let mut test = self.parentheser()?;
                // « ? (i < 2) » ne dé-replie PAS le test : hors compteur
                // déclaré, 0+1i reste le VRAI imaginaire (le raccourci n'a
                // pas de variable de boucle en vue).
                let alors = self.corps_chevron()?;
                return Ok(Instr::Si { test, alors, sinon: None, ligne: t.ligne });
            }
            Tk::Arobase => {
                self.suivant();
                let test = self.parentheser()?;
                let corps = self.corps_chevron()?;
                return Ok(Instr::TantQue { test, corps, ligne: t.ligne });
            }
            _ => {}
        }
        if t.tk == Tk::Ident || t.tk == Tk::Mot {
            // affectation par index sur table : « t["cle"] = 42 »
            if self.voir(1).tk == Tk::LBracket {
                let nom = self.nom_ident()?;
                self.suivant(); // '['
                let index = self.expression()?;
                self.exiger(Tk::RBracket, "']'");
                self.exiger(Tk::Affect, "'='");
                let valeur = self.expression()?;
                return Ok(Instr::AffectIndex { nom, index, valeur, ligne: t.ligne });
            }
            // affectation d'un mot non dollar : « b = a % b » (comme le C++,
            // qui tokenise « b » en Mot puis le traite comme une variable).
            if self.voir(1).tk == Tk::Affect {
                let nom = self.nom_ident()?;
                self.suivant(); // '='
                let e = self.expression()?;
                return Ok(Instr::Affect { nom, e, ligne: t.ligne });
            }
            match t.texte.as_str() {
                "si" => {
                    self.suivant();
                    let test = self.parentheser()?;
                    let alors = self.bloc()?;
                    let mut sinon = None;
                    if self.accepter_mot("sinon") {
                        sinon = Some(if self.est(&[Tk::LBrace]) { self.bloc()? }
                                     else { vec![self.instruction()?] });
                    }
                    return Ok(Instr::Si { test, alors, sinon, ligne: t.ligne });
                }
                "tant_que" => {
                    self.suivant();
                    let mut test = self.parentheser()?;
                    // si la condition compare « i » à un nombre, « i » est une
                    // VARIABLE de boucle (le lexeur l'a déjà replié en 0+1i) :
                    // on le dé-replie pour que le vérificateur le voie comme
                    // un entier. Les vrais usages de l'imaginaire (« z = 2*i »,
                    // « re(i) ») ne comparent pas « i » avec « < » : ils
                    // gardent la constante.
                    deplier_i_test(&mut test);
                    let corps = self.bloc()?;
                    return Ok(Instr::TantQue { test, corps, ligne: t.ligne });
                }
                "pour" => {
                    self.suivant();
                    let nom = self.nom_ident()?;
                    if !self.accepter_mot("de") {
                        let v = self.voir(0);
                        return Err(fmt_err("Attendu 'de' dans pour…de…vers", v.ligne, v.col));
                    }
                    let mut debut = self.expression()?;
                    if !self.accepter_mot("vers") {
                        let v = self.voir(0);
                        return Err(fmt_err("Attendu 'vers' dans pour…de…vers", v.ligne, v.col));
                    }
                    let mut fin = self.expression()?;
                    // même règle que « tant_que » : dans une borne de boucle,
                    // « i » est la VARIABLE de boucle, pas l'imaginaire.
                    if nom == "i" { deplier_i_rec(&mut debut); deplier_i_rec(&mut fin); }
                    let corps = self.bloc()?;
                    return Ok(Instr::Pour { nom, debut, fin, corps, ligne: t.ligne });
                }
                "spawn" => {
                    self.suivant();
                    let corps = self.bloc()?;
                    return Ok(Instr::Spawn(corps, t.ligne));
                }
                "retour" => {
                    let lg_retour = t.ligne;
                    self.suivant();
                    let e = if self.est(&[Tk::Semi, Tk::RBrace, Tk::Eof]) { None }
                            else { Some(self.expression()?) };
                    return Ok(Instr::Retour(e, lg_retour));
                }
                "lever" => {
                    self.suivant();
                    let e = self.expression()?;
                    return Ok(Instr::Lever(e, t.ligne));
                }
                "essayer" => {
                    self.suivant();
                    let corps = self.bloc()?;
                    let mut finalment = None;
                    let mut attraper = None;
                    if self.accepter_mot("attraper") {
                        self.exiger(Tk::LParen, "'('")?;
                        let var = self.nom_ident()?;
                        self.exiger(Tk::RParen, "')'")?;
                        let c = self.bloc()?;
                        attraper = Some((var, c));
                    }
                    if self.accepter_mot("finalement") {
                        finalment = Some(self.bloc()?);
                    }
                    return Ok(Instr::Essayer { corps, attraper, finalment, ligne: t.ligne });
                }
                "var" => {
                    // « var x = expr » : déclaration d variable, équivalente
                    // à une affectation nue (miroir de l'émetteur ELF qui
                    // traite « var » comme une affectation).
                    self.suivant(); // consume 'var'
                    let nom = self.nom_ident()?;
                    self.exiger(Tk::Affect, "'='");
                    let e = self.expression()?;
                    return Ok(Instr::Affect { nom, e, ligne: t.ligne });
                }
                _ => {}
            }
            if self.voir(1).tk == Tk::Affect {
                let nom = self.nom_ident()?;
                self.suivant(); // '='
                let e = self.expression()?;
                return Ok(Instr::Affect { nom, e, ligne: t.ligne });
            }
        }
        Ok(Instr::ExprStmt(self.expression()?))
    }

    fn parentheser(&mut self) -> Result<Expr, Err> {
        self.exiger(Tk::LParen, "'(' ")?;
        let e = self.expression()?;
        self.exiger(Tk::RParen, "')'")?;
        Ok(e)
    }

    fn corps_chevron(&mut self) -> Result<Vec<Instr>, Err> {
        self.exiger(Tk::LBracket, "'[' de corps")?;
        let mut instrs = Vec::new();
        while !self.est(&[Tk::RBracket, Tk::Eof]) {
            instrs.push(self.instruction()?);
            if !self.accepter(&[Tk::Semi]) { break; }
        }
        self.exiger(Tk::RBracket, "']' de corps")?;
        Ok(instrs)
    }

    fn bloc(&mut self) -> Result<Vec<Instr>, Err> {
        self.exiger(Tk::LBrace, "'{'")?;
        let mut instrs = Vec::new();
        while !self.est(&[Tk::RBrace, Tk::Eof]) {
            if self.accepter(&[Tk::Semi]) { continue; }
            instrs.push(self.instruction()?);
            self.accepter(&[Tk::Semi]);
        }
        self.exiger(Tk::RBrace, "'}'")?;
        Ok(instrs)
    }

    pub fn expression(&mut self) -> Result<Expr, Err> { self.ou_expr() }

    fn ou_expr(&mut self) -> Result<Expr, Err> {
        let mut g = self.et_expr()?;
        while self.est_mot("ou") {
            let ligne = self.voir(0).ligne;
            self.suivant();
            let b = self.et_expr()?;
            g = Expr::Binaire { op: "ou".into(), a: Box::new(g), b: Box::new(b), ligne };
        }
        Ok(g)
    }
    fn et_expr(&mut self) -> Result<Expr, Err> {
        let mut g = self.non_expr()?;
        while self.est_mot("et") {
            let ligne = self.voir(0).ligne;
            self.suivant();
            let b = self.non_expr()?;
            g = Expr::Binaire { op: "et".into(), a: Box::new(g), b: Box::new(b), ligne };
        }
        Ok(g)
    }
    fn non_expr(&mut self) -> Result<Expr, Err> {
        if self.est_mot("non") {
            let ligne = self.voir(0).ligne;
            self.suivant();
            let a = self.non_expr()?;
            return Ok(Expr::Unaire { op: "non".into(), a: Box::new(a), ligne });
        }
        self.comparaison()
    }
    fn comparaison(&mut self) -> Result<Expr, Err> {
        let mut g = self.somme()?;
        loop {
            // Ne consommer le jeton qu'apres l'avoir reconnu comme operateur
            // de comparaison : « revenir() » d'une position ne suffit pas quand
            // le courant est le premier '>' d'un « >> » (Afficher) — le repli
            // retombait sur le second '>' et somme() rendait une expression
            // vide, bouclant a l'infini dans le programme principal.
            let tok = self.voir(0).clone();
            let op = match tok.tk {
                Tk::Inf => "<", Tk::InfEg => "<=", Tk::Sup => ">", Tk::SupEg => ">=",
                Tk::DoubleEg => "==", Tk::Diff => "!=", Tk::Environ => "≈",
                _ => break,
            };
            self.suivant();
            let b = self.somme()?;
            g = Expr::Binaire { op: op.into(), a: Box::new(g), b: Box::new(b), ligne: tok.ligne };
        }
        Ok(g)
    }
    fn somme(&mut self) -> Result<Expr, Err> {
        let mut g = self.produit()?;
        while self.est(&[Tk::Plus, Tk::Moins]) {
            let tok = self.suivant();
            let op = if tok.tk == Tk::Plus { "+" } else { "-" };
            let b = self.produit()?;
            g = Expr::Binaire { op: op.into(), a: Box::new(g), b: Box::new(b), ligne: tok.ligne };
        }
        Ok(g)
    }
    fn produit(&mut self) -> Result<Expr, Err> {
        let mut g = self.puissance()?;
        loop {
            let op = if self.est(&[Tk::Etoile]) { "*" }
                else if self.est(&[Tk::Slash]) { "/" }
                else if self.est(&[Tk::Pourcent]) { "%" }
                else if self.est(&[Tk::DivEntier]) { "div" }
                else if self.est_mot("div") { self.suivant(); "div" }
                else { break };
            let ligne = self.voir(0).ligne;
            if self.est(&[Tk::Etoile, Tk::Slash, Tk::Pourcent, Tk::DivEntier]) { self.suivant(); }
            let b = self.puissance()?;
            g = Expr::Binaire { op: op.into(), a: Box::new(g), b: Box::new(b), ligne };
        }
        Ok(g)
    }
    fn puissance(&mut self) -> Result<Expr, Err> {
        let g = self.factorielle_post()?;
        if self.est(&[Tk::Chapeau]) {
            let ligne = self.voir(0).ligne;
            self.suivant();
            let b = self.puissance()?;
            return Ok(Expr::Binaire { op: "^".into(), a: Box::new(g), b: Box::new(b), ligne });
        }
        Ok(g)
    }
    fn factorielle_post(&mut self) -> Result<Expr, Err> {
        let mut g = self.unaire()?;
        while self.est(&[Tk::Bang]) {
            let ligne = self.voir(0).ligne;
            self.suivant();
            g = Expr::Factorielle(Box::new(g), ligne);
        }
        Ok(g)
    }
    fn unaire(&mut self) -> Result<Expr, Err> {
        if self.est(&[Tk::Racine]) {
            let ligne = self.voir(0).ligne;
            self.suivant();
            let a = self.unaire()?;
            return Ok(Expr::Binaire {
                op: "^".into(), a: Box::new(a), ligne,
                b: Box::new(Expr::Litteral { ligne, texte: "0.5".into(), flott: true, rationnel: false, neg: false, ty: TyKind::Dyn }),
            });
        }
        if self.est(&[Tk::Moins]) {
            let ligne = self.voir(0).ligne;
            self.suivant();
            let a = self.unaire()?;
            return Ok(Expr::Unaire { op: "-".into(), a: Box::new(a), ligne });
        }
        if self.est(&[Tk::Plus]) { self.suivant(); return self.unaire(); }
        self.postfixe()
    }
    fn postfixe(&mut self) -> Result<Expr, Err> {
        let mut g = self.atome()?;
        loop {
            if self.est(&[Tk::LParen]) {
                let tok = self.suivant();
                let (module, nom) = match &g {
                    Expr::Appel { nom, module, args, .. } if args.is_empty() => (module.clone(), nom.clone()),
                    Expr::Var(n, _) => (String::new(), n.clone()),
                    _ => {
                        return Err(fmt_err("Seul un nom de fonction peut être appelé", tok.ligne, tok.col));
                    }
                };
                let mut args = Vec::new();
                if !self.est(&[Tk::RParen]) {
                    loop {
                        args.push(self.expression()?);
                        if !self.accepter(&[Tk::Virgule]) { break; }
                    }
                }
                self.exiger(Tk::RParen, "')' de appel")?;
                g = Expr::Appel { nom, module, args, ligne: tok.ligne };
                continue;
            }
            if self.est(&[Tk::LBracket]) {
                let ligne = self.voir(0).ligne;
                let _tok = self.suivant();
                if self.est(&[Tk::DeuxPts]) {
                    self.suivant();
                    let debut = Expr::Litteral { ligne, texte: "0".into(), flott: false, rationnel: false, neg: false, ty: TyKind::Dyn };
                    let fin = if self.est(&[Tk::RBracket]) { None } else { Some(self.expression()?) };
                    self.exiger(Tk::RBracket, "']' de tranche")?;
                    let args = match fin {
                        Some(f) => vec![g, debut, f],
                        None => {
                            let mut l = vec![g.clone(), debut];
                            l.push(Expr::Appel { nom: "longueur".into(), module: String::new(), args: vec![g], ligne });
                            l
                        }
                    };
                    g = Expr::Appel { nom: "sous_chaine".into(), module: String::new(), args, ligne };
                    continue;
                }
                let idx = self.expression()?;
                if self.est(&[Tk::DeuxPts]) {
                    self.suivant();
                    let debut = idx;
                    let fin = if self.est(&[Tk::RBracket]) {
                        Expr::Appel { nom: "longueur".into(), module: String::new(), args: vec![g.clone()], ligne }
                    } else { self.expression()? };
                    self.exiger(Tk::RBracket, "']' de tranche")?;
                    g = Expr::Appel { nom: "sous_chaine".into(), module: String::new(), args: vec![g, debut, fin], ligne };
                    continue;
                }
                self.exiger(Tk::RBracket, "']' d'index")?;
                g = Expr::Index { a: Box::new(g), b: Box::new(idx), ligne };
                continue;
            }
            if self.est(&[Tk::Bang]) {
                let lg = self.voir(0).ligne;
                self.suivant();
                g = Expr::Appel { nom: "factorielle".into(), module: String::new(), args: vec![g], ligne: lg };
                continue;
            }
            return Ok(g);
        }
    }
    fn atome(&mut self) -> Result<Expr, Err> {
        let t = self.voir(0).clone();
        match t.tk {
            Tk::Nombre => {
                self.suivant();
                // suffixe imaginaire : « 4i » ou « 4.5i » (pas d'opérateur entre)
                if self.voir(0).tk == Tk::Mot && self.voir(0).texte == "i" {
                    self.suivant();
                    let im: f64 = t.texte.parse().unwrap_or(0.0);
                    return Ok(Expr::LitComplexe { re: 0.0, im, ligne: t.ligne });
                }
                Ok(Expr::Litteral { ligne: t.ligne, texte: t.texte, flott: false, rationnel: false, neg: false, ty: TyKind::Dyn })
            }
            Tk::Flottant => { self.suivant(); Ok(Expr::Litteral { ligne: t.ligne, texte: t.texte, flott: true, rationnel: false, neg: false, ty: TyKind::Dyn }) }
            Tk::Chaine => { self.suivant(); Ok(Expr::ChaineLit(t.texte, t.ligne)) }
            Tk::LBracket => {
                self.suivant();
                let mut elems = Vec::new();
                if !self.est(&[Tk::RBracket]) {
                    loop {
                        elems.push(self.expression()?);
                        if !self.accepter(&[Tk::Virgule]) { break; }
                    }
                }
                self.exiger(Tk::RBracket, "']' de liste")?;
                Ok(Expr::ListeLit(elems, t.ligne))
            }
            Tk::LBrace => {
                self.suivant();
                let mut elems = Vec::new();
                if !self.est(&[Tk::RBrace]) {
                    loop {
                        let cle = self.nom_ident()?;
                        self.exiger(Tk::DeuxPoints, "':' dans table")?;
                        let val = self.expression()?;
                        elems.push((cle, val));
                        if !self.accepter(&[Tk::Virgule]) { break; }
                    }
                }
                self.exiger(Tk::RBrace, "'}' de table")?;
                Ok(Expr::TableLit(elems, t.ligne))
            }
            Tk::LParen => {
                self.suivant();
                let e = self.expression()?;
                self.exiger(Tk::RParen, "')'")?;
                Ok(e)
            }
            Tk::Ident | Tk::Mot => {
                if t.texte == "vrai" { self.suivant(); return Ok(Expr::BooleenLit(true, t.ligne)); }
                if t.texte == "faux" { self.suivant(); return Ok(Expr::BooleenLit(false, t.ligne)); }
                if t.texte == "i" { self.suivant(); return Ok(Expr::LitComplexe { re: 0.0, im: 1.0, ligne: t.ligne }); }
                if t.texte == "π" {
                    self.suivant();
                    return Ok(Expr::Appel { nom: "pi".into(), module: String::new(), args: vec![Expr::Litteral { ligne: t.ligne, texte: "50".into(), flott: false, rationnel: false, neg: false, ty: TyKind::Dyn }], ligne: t.ligne });
                }
                if t.texte == "φ" {
                    self.suivant();
                    return Ok(Expr::Appel { nom: "phi".into(), module: String::new(), args: vec![Expr::Litteral { ligne: t.ligne, texte: "50".into(), flott: false, rationnel: false, neg: false, ty: TyKind::Dyn }], ligne: t.ligne });
                }
                let mut module = String::new();
                if self.voir(1).tk == Tk::DeuxPoints {
                    module = t.texte.clone();
                    self.suivant(); self.suivant();
                }
                let nom = self.nom_ident()?;
                if module.is_empty() { Ok(Expr::Var(nom, t.ligne)) }
                else { Ok(Expr::Appel { nom, module, args: Vec::new(), ligne: t.ligne }) }
            }
            _ => Err(fmt_err(&format!("Attente d'une expression, trouvé '{}'", t.texte), t.ligne, t.col)),
        }
    }
}

// ── Vérificateur (miroir de Verificateur du C++) ─────────────────────────

fn nom_fonction_connue(prog: &Programme, module: &str, nom: &str) -> bool {
    if !module.is_empty() { return module == "dynamique"; }
    prog.signatures.contains_key(nom)
        || est_std(nom)
        || (nom == "entier" || nom == "rationnel" || nom == "flottant" || nom == "chaine")
}

use std::cell::RefCell;
thread_local! {
    static GLOBALS: RefCell<HashMap<String, Ty>> = RefCell::new(HashMap::new());
}

pub fn verifier(prog: &mut Programme) -> Result<(), Err> {
    // pré-passe : collecte les types des variables globales déclarées au
    // niveau supérieur, pour qu'elles soient disponibles dans les corps de
    // fonction.
    let mut globals: HashMap<String, Ty> = HashMap::new();
    for i in &prog.instructions {
        match i {
            Instr::Affect { nom, e, ligne } => {
                if let Ok(t) = expr_type(prog, e, &mut globals) {
                    if t.n != TyKind::Vide {
                        let cible = globals.get(nom).cloned().unwrap_or(t);
                        if nom == "i" && cible.n == TyKind::Entier {
                            I_ENTIER.with(|c| c.set(true));
                        }
                        globals.insert(nom.clone(), cible);
                    }
                }
            }
            Instr::AffectIndex { nom, index, valeur, ligne } => {
                // affectation par index : « t["cle"] = 42 »
                if let Ok(t) = expr_type(prog, valeur, &mut globals) {
                    if t.n != TyKind::Vide {
                        let _ = expr_type(prog, index, &mut globals);
                        let cible = globals.get(nom).cloned().unwrap_or(Ty::simple(TyKind::Table));
                        globals.insert(nom.clone(), cible);
                    }
                }
            }
            Instr::Lire { nom, ty, ligne } => {
                let t_final = globals.get(nom).cloned().unwrap_or_else(|| Ty::simple(TyKind::Entier));
                globals.insert(nom.clone(), t_final);
            }
            _ => {}
        }
    }

    GLOBALS.with(|g| *g.borrow_mut() = globals.clone());
    for (nom, sig) in &prog.signatures {
        let sig = prog.signatures.get(nom).unwrap().clone();
        if sig.corps.is_none() {
            let frontieres: Vec<&Ty> = sig.params.iter().map(|(_, t)| t)
                .chain(std::iter::once(&sig.retour)).collect();
            for t in frontieres {
                if !matches!(t.n, TyKind::Entier | TyKind::Flottant | TyKind::Booleen | TyKind::Chaine) {
                    return Err(fmt_err(&format!(
                        "Type {} non supporté aux frontières d'une fonction externe \
                         (utilisez entier/flottant/booleen/chaine)", t.nom()), sig.ligne, 0));
                }
            }
        }
        for (pn, _) in &sig.params { verifier_nom_reserve(pn, 0)?; }
        let mut env: HashMap<String, Ty> = HashMap::new();
        for (pn, pt) in &sig.params { env.insert(pn.clone(), pt.clone()); }
        if let Some(corps) = &sig.corps {
            verifier_bloc(prog, corps, &mut env, &sig.retour, true)?;
        }
    }
    let mut env: HashMap<String, Ty> = HashMap::new();
    verifier_bloc(prog, &prog.instructions.clone(), &mut env, &Ty::vide(), false)?;
    GLOBALS.with(|g| g.borrow_mut().clear());
    Ok(())
}

fn verifier_nom_reserve(nom: &str, ligne: usize) -> Result<(), Err> {
    // miroir C++ verifier_nom_reserve : « t1 », « t2 »… sont des temporaires
    // du JIT, interdits comme noms de variable.
    let b = nom.as_bytes();
    if b.len() > 1 && b[0] == b't' && b[1..].iter().all(|c| c.is_ascii_digit()) {
        return Err(fmt_err(&format!("Le nom '{}' est réservé par le moteur", nom), ligne, 0));
    }
    Ok(())
}

fn verifier_bloc(prog: &Programme, instrs: &[Instr], env: &mut HashMap<String, Ty>,
                 retour: &Ty, en_fonction: bool) -> Result<(), Err> {
    for (position, i) in instrs.iter().enumerate() {
        match i {
            Instr::Afficher(exprs, lg) => {
                for e in exprs {
                    let t = expr_type(prog, e, env)?;
                    if t.n == TyKind::Vide {
                        return Err(fmt_err("Rien à afficher ici", *lg, 0));
                    }
                }
            }
            Instr::Lire { nom, ty, ligne } => {
                // miroir C++ IK::Lire (rose.cpp:1280-1293) : variable inconnue ->
                // entier ; variable existante -> entier/flottant/rationnel sinon erreur.
                if let Some(t) = env.get(nom) {
                    if !matches!(t.n, TyKind::Entier | TyKind::Flottant | TyKind::Rationnel) {
                        return Err(fmt_err(&format!(
                            "Ne peut pas lire dans une variable {} (entier/flottant attendus)",
                            nom_pour_msg(t)), *ligne, 0));
                    }
                } else {
                    verifier_nom_reserve(nom, *ligne)?;
                }
                // le C++ typer la lue en ENTIER si inconnue, sinon garde le type
                // existant (rose.cpp:1283-1292 : instr->type_read = T_ENTIER / existant)
                let t_final = env.get(nom).cloned().unwrap_or_else(|| Ty::simple(TyKind::Entier));
                env.insert(nom.clone(), t_final);
            }
            Instr::Affect { nom, e, ligne } => {
                // miroir C++ instruction() IK::Affectation : le nom réservé est
                // vérifié APRÈS l'expression ; et « valeur sans type » rejeté.
                // Si la variable « i » est déjà un entier (compteur de boucle),
                // tout littéral imaginaire pur de l'expression est en réalité
                // cette variable : le lexeur replie « i » en 0+1i avant que
                // le contexte soit connu.
                let e = {
                    let i_est_entier = nom == "i"
                        && (env.get(nom).map(|t| t.n == TyKind::Entier).unwrap_or(false)
                            || (nom == "i" && !env.contains_key(nom)));
                    if i_est_entier && porte_boucle_i(instrs, position) {
                        let mut e2 = (*e).clone();
                        deplier_i_rec(&mut e2);
                        e2
                    } else { (*e).clone() }
                };
                let t = expr_type(prog, &e, env)?;
                if t.n == TyKind::Vide {
                    return Err(fmt_err("Impossible d'affecter une valeur sans type", *ligne, 0));
                }
                // affectation dans une variable existante : contrôle de domaine
                let mut cible = t.clone();
                // si la variable est un global, on la traite comme un shadow
                // (pas de contrôle de type, comme le moteur C++)
                let is_global = GLOBALS.with(|g| g.borrow().contains_key(nom));
                let existante = if is_global { None } else { env.get(nom).cloned() };
                if let Some(ex) = &existante {
                    cible = ex.clone();
                    if !(ex.n == TyKind::Dyn || t.n == TyKind::Dyn
                         || (t.n == TyKind::Entier && ex.n == TyKind::Rationnel))
                    {
                        if !(t == *ex) {
                            if !(promotion(&t, ex) == *ex && est_numerique(&t) && est_numerique(ex)) {
                                return Err(fmt_err(&format!(
                                    "'{}' est {}, on ne peut pas y stocker du {}",
                                    nom, ex.nom(), t.nom()), *ligne, 0));
                            }
                        }
                    }
                }
                verifier_nom_reserve(nom, *ligne)?;
                env.insert(nom.clone(), cible);
            }
            Instr::AffectIndex { nom, index, valeur, ligne } => {
                // « t["cle"] = 42 » : la valeur doit avoir un type, l'index
                // doit être utilisable (entier ou chaîne pour une table).
                let t = expr_type(prog, valeur, env)?;
                if t.n == TyKind::Vide {
                    return Err(fmt_err("Impossible d'affecter une valeur sans type", *ligne, 0));
                }
                let _ = expr_type(prog, index, env)?;
                // la table doit exister (ou être créée)
                let existante = env.get(nom).cloned();
                if let Some(ex) = &existante {
                    if ex.n != TyKind::Table && ex.n != TyKind::Tableau && ex.n != TyKind::Dyn {
                        return Err(fmt_err(&format!(
                            "'{}' est {}, on ne peut pas y affecter par index",
                            nom, ex.nom()), *ligne, 0));
                    }
                }
                verifier_nom_reserve(nom, *ligne)?;
                if existante.is_none() {
                    // type inconnu : on ne peut pas deviner, laisser Dyn
                    // (le type sera résolu à l'exécution)
                }
            }
            Instr::Si { test, alors, sinon, ligne } => {
                let t = expr_type(prog, test, env)?;
                exiger_booleen(&t, *ligne, "si")?;
                verifier_bloc(prog, alors, env, retour, en_fonction)?;
                if let Some(s) = sinon { verifier_bloc(prog, s, env, retour, en_fonction)?; }
            }
            Instr::TantQue { test, corps, ligne } => {
                let t = expr_type(prog, test, env)?;
                exiger_booleen(&t, *ligne, "tant_que")?;
                verifier_bloc(prog, corps, env, retour, en_fonction)?;
            }
            Instr::Pour { nom, debut, fin, corps, ligne } => {
                if env.contains_key(nom) {
                    return Err(fmt_err(&format!("La variable de boucle '{}' existe déjà", nom), *ligne, 0));
                }
                let td = expr_type(prog, debut, env)?;
                let tf = expr_type(prog, fin, env)?;
                if td.n != TyKind::Entier || tf.n != TyKind::Entier {
                    return Err(fmt_err("pour…de…vers exige des bornes entières", *ligne, 0));
                }
                verifier_nom_reserve(nom, *ligne)?;
                env.insert(nom.clone(), Ty::simple(TyKind::Entier));
                verifier_bloc(prog, corps, env, retour, en_fonction)?;
            }
            Instr::Retour(e, lg) => {
                // miroir C++ instruction() IK::Retour : « retour » hors de
                // fonction, et « La fonction doit retourner un X » si le corps
                // omet le retour attendu.

                if !en_fonction {
                    return Err(fmt_err("'retour' hors de fonction", *lg, 0));
                }
                match e {
                    // miroir de l'affectation : dans « fn f(n: entier) -> entier {
                    // …; retour i } », le « i » replié en 0+1i par le lexeur est
                    // la variable de boucle entière, pas l'imaginaire.
                    Some(Expr::LitComplexe { re, im, ligne })
                        if env.get("i").map(|t| t.n == TyKind::Entier).unwrap_or(false)
                            && *re == 0.0 && *im == 1.0 =>
                    {
                        let e2 = Expr::Var("i".into(), *ligne);
                        let t = expr_type(prog, &e2, env)?;
                        if !(t == *retour || (est_numerique(&t) && est_numerique(retour)
                                              && promotion(&t, retour) == *retour)
                            || retour.n == TyKind::Dyn)
                        {
                            return Err(fmt_err(&format!(
                                "La fonction retourne {}, attendu {}", t.nom(), retour.nom()),
                                *lg, 0));
                        }
                    }
                    // C++ ne signale « La fonction doit retourner un X » que
                    // sur un « retour » SANS expression (rose.cpp:1346-1348) ;
                    // un corps qui omet tout retour n'est pas vérifié.
                    None => {
                        if retour.n != TyKind::Vide {
                            return Err(fmt_err(
                                &format!("La fonction doit retourner un {}", retour.nom()),
                                *lg, 0));
                        }
                    }
                    Some(e) => {
                        let t = expr_type(prog, e, env)?;
                        if !(t == *retour || (est_numerique(&t) && est_numerique(retour)
                                              && promotion(&t, retour) == *retour)
                            || retour.n == TyKind::Dyn)
                        {
                            return Err(fmt_err(&format!(
                                "La fonction retourne {}, attendu {}", t.nom(), retour.nom()),
                                *lg, 0));
                        }
                    }
                }
            }
            Instr::ExprStmt(e) => { expr_type(prog, e, env)?; }
            Instr::Bloc(b) => verifier_bloc(prog, b, env, retour, en_fonction)?,
            Instr::Spawn(b, _) => { verifier_bloc(prog, b, env, retour, en_fonction)?; env.insert("_thread".to_string(), Ty::simple(TyKind::Entier)); }
            Instr::Essayer { corps, attraper, finalment, .. } => {
                verifier_bloc(prog, corps, env, retour, en_fonction)?;
                if let Some((var, c)) = attraper {
                    let mut env2 = env.clone();
                    env2.insert(var.clone(), Ty::simple(TyKind::Chaine));
                    verifier_bloc(prog, c, &mut env2, retour, en_fonction)?;
                }
                if let Some(f) = finalment { verifier_bloc(prog, f, env, retour, en_fonction)?; }
            }
            Instr::Lever(e, _) => { expr_type(prog, e, env)?; }
        }
    }
    Ok(())
}

/// Vrai si l'affectation courante (repérée par sa position dans le bloc) est
/// l'incrément du compteur d'une boucle « tant_que (i < n) » : la condition
/// compare « i » et l'affectation est la dernière du corps. Dans ce contexte,
/// le littéral 1+1i ou 0+1i est la VARIABLE i, pas l'imaginaire.
fn porte_boucle_i(instrs: &[Instr], position: usize) -> bool {
    match instrs.get(position) {
        Some(Instr::Affect { nom, .. }) if nom == "i" => {}
        _ => return false,
    }
    // l'affectation doit être la dernière du bloc (cas typique du corps de
    // boucle « $i = $i + 1 ») — dans un autre ordre, on garde l'imaginaire.
    position + 1 == instrs.len()
}

fn est_numerique(t: &Ty) -> bool {
    matches!(t.n, TyKind::Entier | TyKind::Rationnel | TyKind::Flottant)
}

fn droite_unaire_moins(e: &Expr) -> bool {
    matches!(e, Expr::Unaire { op, .. } if op == "-")
}

fn exiger_booleen(t: &Ty, ligne: usize, quoi: &str) -> Result<(), Err> {
    if t.n != TyKind::Booleen {
        return Err(fmt_err(&format!("La condition de {} doit être booléenne, trouvé {}", quoi, t.nom()), ligne, 0));
    }
    Ok(())
}

/// Miroir exact de Verificateur::verifier_args (rose.cpp:1206-1244).
/// Chaque attendu : "entier", "flottant", "num", "booleen", "chaine", "liste",
/// "table", "tampon", "liste_ou_chaine_ou_table", "liste_elt", "tout".
/// Miroir exact de Verificateur::verifier_args (rose.cpp:1514-1551).
fn verifier_type_std(nom: &str, attendus: &[&str], args: &[Expr],
                     env: &mut HashMap<String, Ty>, prog: &Programme,
                     ligne: usize) -> Result<(), Err> {
    let mut t_elt = Ty::vide();
    let mut a_elt = false;
    for (i, a) in args.iter().enumerate() {
        let ta = expr_type(prog, a, env)?;
        match attendus[i] {
            "chaine" => {
                if ta.n != TyKind::Chaine {
                    return Err(fmt_err(&format!("{}: une chaine est attendue", nom), ligne, 0));
                }
            }
            "num" => {
                if !est_numerique(&ta) {
                    return Err(fmt_err(&format!("{}: un nombre est attendu, reçu {}",
                        nom, nom_pour_msg(&ta)), ligne, 0));
                }
            }
            "entier" => {
                if ta.n != TyKind::Entier {
                    return Err(fmt_err(&format!("{}: un entier est attendu, reçu {}",
                        nom, nom_pour_msg(&ta)), ligne, 0));
                }
            }
            "flottant" => {
                if ta.n != TyKind::Flottant {
                    return Err(fmt_err(&format!("{}: un flottant est attendu, reçu {}",
                        nom, nom_pour_msg(&ta)), ligne, 0));
                }
            }
            "booleen" => {
                if ta.n != TyKind::Booleen {
                    return Err(fmt_err(&format!("{}: un booleen est attendu, reçu {}",
                        nom, nom_pour_msg(&ta)), ligne, 0));
                }
            }
            "liste" => {
                if ta.n != TyKind::Liste {
                    return Err(fmt_err(&format!("{}: une liste est attendue, reçu {}",
                        nom, nom_pour_msg(&ta)), ligne, 0));
                }
                t_elt = Ty::simple(ta.elt); a_elt = true;   // type de l'ÉLÉMENT, pas de la liste
            }
            "tampon" => {
                if ta.n != TyKind::Tampon {
                    return Err(fmt_err(&format!("{}: un tampon est attendu, reçu {}",
                        nom, nom_pour_msg(&ta)), ligne, 0));
                }
            }
            "liste_ou_chaine" | "liste_ou_chaine_ou_table" => {
                if !matches!(ta.n, TyKind::Liste | TyKind::Chaine | TyKind::Table | TyKind::Tampon | TyKind::Tableau) {
                    return Err(fmt_err(&format!(
                        "{}: une liste, une chaîne ou une table est attendue, reçu {}",
                        nom, nom_pour_msg(&ta)), ligne, 0));
                }
            }
            "liste_elt" => {
                if a_elt && t_elt.n != TyKind::Vide && ta != t_elt
                    && !(est_numerique(&ta) && est_numerique(&t_elt)
                         && promotion(&ta, &t_elt) == t_elt)
                {
                    return Err(fmt_err(&format!("{}: élément {} ne correspond pas à la liste {}",
                        nom, nom_pour_msg(&ta), nom_pour_msg(&t_elt)), ligne, 0));
                }
            }
            "table" => {
                // le C++ n'a PAS de branche « table » dans verifier_args :
                // l'appel passe la vérification statique et l'erreur
                // « <nom> attend une table » est levée à l'EXÉCUTION, sans
                // ligne (rose.cpp:2616-2643). Même comportement ici.
            }
            _ => { /* "tout" et inconnu : tout type est accepté (afficher_erreur) */ }
        }
    }
    Ok(())
}

fn nom_pour_msg(t: &Ty) -> String {
    // le C++ affiche T_VALEUR « ? » dans les messages du vérificateur ;
    // côté rose-20 c'est Dyn, et une liste d'élément Dyn garde liste<?>
    match t.n {
        TyKind::Dyn => "?".into(),
        TyKind::Liste if t.elt == TyKind::Dyn => "list<?>".into(),
        _ => t.nom(),
    }
}

fn ty_du_texte(s: &str) -> Ty {
    match s {
        "vide" => Ty::vide(),
        "entier" => Ty::simple(TyKind::Entier),
        "rationnel" => Ty::simple(TyKind::Rationnel),
        "flottant" => Ty::simple(TyKind::Flottant),
        "booleen" => Ty::simple(TyKind::Booleen),
        "chaine" => Ty::simple(TyKind::Chaine),
        "table" => Ty::simple(TyKind::Table),
        "tampon" => Ty::simple(TyKind::Tampon),
        "tableau" => Ty::simple(TyKind::Tableau),
        "complexe" => Ty::simple(TyKind::Complexe),
        "irrationnel" => Ty::simple(TyKind::Irrationnel),
        _ => Ty::vide(),
    }
}

fn expr_type(prog: &Programme, e: &Expr, env: &mut HashMap<String, Ty>) -> Result<Ty, Err> {
    match e {
        Expr::Litteral { flott, texte, .. } => {
            if *flott { return Ok(Ty::simple(TyKind::Flottant)); }
            if texte.contains('/') { return Ok(Ty::simple(TyKind::Rationnel)); }
            Ok(Ty::simple(TyKind::Entier))
        }
        Expr::LitComplexe { .. } => Ok(Ty::simple(TyKind::Complexe)),
        Expr::ChaineLit(..) => Ok(Ty::simple(TyKind::Chaine)),
        Expr::BooleenLit(..) => Ok(Ty::simple(TyKind::Booleen)),
        Expr::Var(n, l) => {
            // une variable locale PORTE LE NOM : elle masque la constante
            // (le lexeur retire le « $ », donc $e et e ont le même jeton ;
            // sans ce test, `$e = 5` serait écrasé par la constante d'Euler).
            if let Some(t) = env.get(n).cloned() { return Ok(t); }
            if let Some(t) = GLOBALS.with(|g| g.borrow().get(n).cloned()) { return Ok(t); }
            // constantes irrationnelles sans argument : pi, phi, e → chaine
            if matches!(n.as_str(), "pi" | "phi" | "e") {
                return Ok(Ty::simple(TyKind::Chaine));
            }
            Err(fmt_err(&format!("Variable '{}' utilisée sans valeur", n), *l, 0))
        }
        Expr::ListeLit(el, ligne) => {
            // miroir C++ NK::ListeLit (rose.cpp:1393-1407)
            let ligne = *ligne;
            if el.is_empty() {
                return Err(fmt_err("Liste vide : le type d'élément est introuvable", ligne, 0));
            }
            let mut t_elt = expr_type(prog, &el[0], env)?;
            for x in &el[1..] {
                let t2 = expr_type(prog, x, env)?;
                if t2 != t_elt {
                    if est_numerique(&t2) && est_numerique(&t_elt) { t_elt = promotion(&t2, &t_elt); }
                    else {
                        return Err(fmt_err(&format!("Liste hétérogène : {} puis {}",
                            nom_pour_msg(&t_elt), nom_pour_msg(&t2)), ligne, 0));
                    }
                }
            }
            if t_elt.n == TyKind::Liste {
                return Err(fmt_err("Listes de listes non supportées pour l'instant", ligne, 0));
            }
            Ok(Ty::liste(t_elt.n))
        }
        Expr::TableLit(el, ligne) => {
            // miroir C++ NK::TableLit (rose.cpp:1408-1420)
            let ligne = *ligne;
            Ok(Ty::simple(TyKind::Table))
        }
        Expr::Unaire { op, a, ligne } => {
            // miroir C++ NK::Unaire
            let t = expr_type(prog, a, env)?;
            if op == "non" {
                exiger_booleen(&t, *ligne, "non")?;
                return Ok(Ty::simple(TyKind::Booleen));
            }
            if t.n == TyKind::Dyn {
                // type dynamique : on laisse passer, résolu à l'exécution.
                return Ok(Ty::simple(TyKind::Dyn));
            }
            if !est_numerique(&t) {
                return Err(fmt_err(&format!("Unaire {} sur un {}", op, nom_pour_msg(&t)), *ligne, 0));
            }
            Ok(t)
        }
        Expr::Factorielle(a, ligne) => {
            let t = expr_type(prog, a, env)?;
            if t.n != TyKind::Entier {
                return Err(fmt_err(&format!("Factorielle attend un entier, trouvé {}", nom_pour_msg(&t)), *ligne, 0));
            }
            Ok(Ty::simple(TyKind::Entier))
        }
        Expr::Index { a, b, ligne } => {
            // miroir C++ NK::Index (rose.cpp:1409-1424)
            let ligne = *ligne;
            let tb = expr_type(prog, a, env)?;
            let ti = expr_type(prog, b, env)?;
            if !(matches!(tb.n, TyKind::Liste | TyKind::Chaine | TyKind::Table | TyKind::Tableau | TyKind::Dyn)) {
                return Err(fmt_err(&format!("Index sur un {}, une liste est attendue", nom_pour_msg(&tb)), ligne, 0));
            }
            if tb.n == TyKind::Table || tb.n == TyKind::Dyn {
                if tb.n == TyKind::Table
                    && !(matches!(ti.n, TyKind::Entier | TyKind::Rationnel | TyKind::Flottant
                                  | TyKind::Chaine | TyKind::Booleen))
                {
                    return Err(fmt_err("Index d'une table doit être un texte ou un nombre", ligne, 0));
                }
                return Ok(Ty { n: TyKind::Dyn, elt: TyKind::Dyn });   // T_VALEUR côté C++
            }
            if ti.n != TyKind::Entier {
                return Err(fmt_err(if tb.n == TyKind::Chaine { "Index d'une chaîne doit être entier" }
                                   else if tb.n == TyKind::Tableau { "Index d'un tableau doit être entier" }
                                   else { "Index d'une liste doit être entier" }, ligne, 0));
            }
            Ok(if tb.n == TyKind::Chaine { Ty::simple(TyKind::Chaine) }
               else if tb.n == TyKind::Tableau { Ty::simple(TyKind::Entier) }
               else { Ty::simple(tb.elt) })
        }
        Expr::Binaire { op, a, b, ligne } => {
            // miroir exact Verificateur::binaire (rose.cpp:1450-1510)
            let ligne = *ligne;
            let ta = expr_type(prog, a, env)?;
            let tb = expr_type(prog, b, env)?;
            let na = nom_pour_msg(&ta); let nb = nom_pour_msg(&tb);
            match op.as_str() {
                "et" | "ou" => {
                    exiger_booleen(&ta, ligne, "et/ou")?;
                    exiger_booleen(&tb, ligne, "et/ou")?;
                    return Ok(Ty::simple(TyKind::Booleen));
                }
                "==" | "!=" | "≈" | "?=" | "=~" => {
                    if ta == tb || (est_numerique(&ta) && est_numerique(&tb))
                        || ta.n == TyKind::Dyn || tb.n == TyKind::Dyn
                    {
                        return Ok(Ty::simple(TyKind::Booleen));
                    }
                    return Err(fmt_err(&format!("Comparaison {} {} {} impossible", na, op, nb), ligne, 0));
                }
                "<" | "<=" | ">" | ">=" => {
                    if (est_numerique(&ta) && est_numerique(&tb))
                        || (ta.n == TyKind::Chaine && tb.n == TyKind::Chaine)
                    {
                        return Ok(Ty::simple(TyKind::Booleen));
                    }
                    // comparaison impliquant un paramètre sans type déclaré
                    // (Dyn) : acceptée, le type est résolu à l'exécution.
                    if ta.n == TyKind::Dyn || tb.n == TyKind::Dyn {
                        return Ok(Ty::simple(TyKind::Booleen));
                    }
                    return Err(fmt_err(&format!("Ordre {} {} {} impossible", na, op, nb), ligne, 0));
                }
                "+" => {
                    if ta.n == TyKind::Chaine && tb.n == TyKind::Chaine { return Ok(Ty::simple(TyKind::Chaine)); }
                    if ta.n == TyKind::Complexe || tb.n == TyKind::Complexe { return Ok(Ty::simple(TyKind::Complexe)); }
                    if ta.n == TyKind::Irrationnel || tb.n == TyKind::Irrationnel { return Ok(Ty::simple(TyKind::Irrationnel)); }
                    if ta.n == TyKind::Dyn || tb.n == TyKind::Dyn {
                        if est_numerique(&ta) && est_numerique(&tb) { return Ok(promotion(&ta, &tb)); }
                        return Ok(Ty::simple(TyKind::Dyn));
                    }
                    if est_numerique(&ta) && est_numerique(&tb) { return Ok(promotion(&ta, &tb)); }
                    return Err(fmt_err(&format!("Addition {} + {} impossible", na, nb), ligne, 0));
                }
                "-" | "*" => {
                    if ta.n == TyKind::Complexe || tb.n == TyKind::Complexe { return Ok(Ty::simple(TyKind::Complexe)); }
                    if ta.n == TyKind::Irrationnel || tb.n == TyKind::Irrationnel { return Ok(Ty::simple(TyKind::Irrationnel)); }
                    if op == "*" && ta.n == TyKind::Chaine && tb.n == TyKind::Entier { return Ok(Ty::simple(TyKind::Chaine)); }
                    if op == "*" && ta.n == TyKind::Entier && tb.n == TyKind::Chaine { return Ok(Ty::simple(TyKind::Chaine)); }
                    // type dynamique (paramètre sans type déclaré) : on laisse
                    // passer, le type est résolu à l'exécution (Dyn).
                    if ta.n == TyKind::Dyn || tb.n == TyKind::Dyn {
                        if est_numerique(&ta) && est_numerique(&tb) { return Ok(promotion(&ta, &tb)); }
                        // Dyn × Dyn ou Dyn × X : on renvoie Dyn pour laisser
                        // l'interpréteur / le JIT résoudre à l'exécution.
                        return Ok(Ty::simple(TyKind::Dyn));
                    }
                    if est_numerique(&ta) && est_numerique(&tb) { return Ok(promotion(&ta, &tb)); }
                    return Err(fmt_err(&format!("Opération {} {} {} impossible", na, op, nb), ligne, 0));
                }
                "/" => {
                    if ta.n == TyKind::Complexe || tb.n == TyKind::Complexe { return Ok(Ty::simple(TyKind::Complexe)); }
                    if ta.n == TyKind::Irrationnel || tb.n == TyKind::Irrationnel { return Ok(Ty::simple(TyKind::Irrationnel)); }
                    if ta.n == TyKind::Dyn || tb.n == TyKind::Dyn {
                        if est_numerique(&ta) && est_numerique(&tb) {
                            let r = promotion(&ta, &tb);
                            if r.n == TyKind::Entier { return Ok(Ty::simple(TyKind::Rationnel)); }
                            return Ok(r);
                        }
                        return Ok(Ty::simple(TyKind::Dyn));
                    }
                    if est_numerique(&ta) && est_numerique(&tb) {
                        let r = promotion(&ta, &tb);
                        // division exacte : entier/entier -> rationnel
                        if r.n == TyKind::Entier { return Ok(Ty::simple(TyKind::Rationnel)); }
                        return Ok(r);
                    }
                    return Err(fmt_err(&format!("Division {} / {} impossible", na, nb), ligne, 0));
                }
                "%" => {
                    if ta.n == TyKind::Entier && tb.n == TyKind::Entier { return Ok(Ty::simple(TyKind::Entier)); }
                    return Err(fmt_err("Le modulo exige deux entiers", ligne, 0));
                }
                "div" => {
                    if !(est_numerique(&ta) || ta.n == TyKind::Booleen) {
                        return Err(fmt_err(&format!("div attend un nombre à gauche, trouvé {}", na), ligne, 0));
                    }
                    if !(est_numerique(&tb) || tb.n == TyKind::Booleen) {
                        return Err(fmt_err(&format!("div attend un nombre à droite, trouvé {}", nb), ligne, 0));
                    }
                    if tb.n == TyKind::Flottant || ta.n == TyKind::Flottant { return Ok(Ty::simple(TyKind::Flottant)); }
                    return Ok(Ty::simple(TyKind::Entier));
                }
                "^" => {
                    if !est_numerique(&ta) {
                        return Err(fmt_err(&format!("Puissance sur une base {}", na), ligne, 0));
                    }
                    if tb.n == TyKind::Flottant { return Ok(Ty::simple(TyKind::Flottant)); }
                    if matches!(tb.n, TyKind::Entier | TyKind::Rationnel | TyKind::Booleen) {
                        if droite_unaire_moins(b) {
                            if ta.n == TyKind::Entier { return Ok(Ty::simple(TyKind::Rationnel)); }
                            return Ok(ta);
                        }
                        return Ok(if ta.n == TyKind::Entier { Ty::simple(TyKind::Entier) } else { ta });
                    }
                    return Err(fmt_err(&format!("Exposant {} impossible", nb), ligne, 0));
                }
                _ => return Err(fmt_err(&format!("Opérateur '{}' inconnu", op), ligne, 0)),
            }
        }
        Expr::Appel { nom, module, args, ligne } => {
            let ligne = *ligne;
            if !nom_fonction_connue(prog, module, nom) {
                return Err(fmt_err(&format!("Fonction '{}' inconnue", nom), ligne, 0));
            }

            // conversions à un argument — miroir C++ rose.cpp:1564-1579
            if args.len() == 1 && matches!(nom.as_str(), "entier" | "rationnel" | "flottant" | "chaine") {
                let cible = match nom.as_str() {
                    "entier" => Ty::simple(TyKind::Entier),
                    "rationnel" => Ty::simple(TyKind::Rationnel),
                    "flottant" => Ty::simple(TyKind::Flottant),
                    _ => Ty::simple(TyKind::Chaine),
                };
                let t = expr_type(prog, &args[0], env)?;
                if cible.n == TyKind::Chaine { return Ok(Ty::simple(TyKind::Chaine)); }
                if t.n == TyKind::Chaine {
                    if cible.n == TyKind::Entier { return Ok(Ty::simple(TyKind::Entier)); }
                    if cible.n == TyKind::Flottant { return Ok(Ty::simple(TyKind::Flottant)); }
                    return Err(fmt_err("Conversion chaine → rationnel non disponible, passez par flottant", ligne, 0));
                }
                if !est_numerique(&t) {
                    return Err(fmt_err(&format!("Conversion {} → {} impossible",
                        nom_pour_msg(&t), cible.nom()), ligne, 0));
                }
                return Ok(cible);
            }
            if let Some(sig) = prog.signatures.get(nom).cloned() {
                if args.len() != sig.params.len() {
                    return Err(fmt_err(&format!("{} attend {} argument(s), reçu {}",
                        nom, sig.params.len(), args.len()), ligne, 0));
                }
                // miroir C++ rose.cpp:1589-1596 : le type de chaque argument est
                // comparé au paramètre (promotion numérique admise dans le sens
                // ta -> t_att uniquement).
                for (i, a) in args.iter().enumerate() {
                    let ta = expr_type(prog, a, env)?;
                    let t_att = &sig.params[i].1;
                    if &ta != t_att && !(est_numerique(&ta) && est_numerique(t_att)
                                         && promotion(&ta, t_att) == *t_att)
                        && t_att.n != TyKind::Dyn {
                        // paramètre sans type déclaré (Dyn) : accepte n'importe
                        // quel argument — le type est résolu à l'exécution.
                        return Err(fmt_err(&format!("Argument de {}: {} reçu, {} attendu",
                            nom, nom_pour_msg(&ta), t_att.nom()), ligne, 0));
                    }
                }
                return Ok(sig.retour.clone());
            }
            if !module.is_empty() {
                // miroir C++ rose.cpp:1555-1562
                if !matches!(nom.as_str(), "charger" | "fermer" | "appeler0" | "appeler1" | "appeler2") {
                    return Err(fmt_err(&format!(
                        "Fonction dynamique::{} inconnue \
                         (disponibles: charger, fermer, appeler0, appeler1, appeler2)", nom), ligne, 0));
                }
                for a in args { expr_type(prog, a, env)?; }
                let ret = match nom.as_str() {
                    "charger" => Ty::simple(TyKind::Entier),
                    "fermer" => Ty::vide(),
                    _ => Ty::simple(TyKind::Flottant),
                };
                return Ok(ret);
            }
            let sd = chercher_std(nom).unwrap();
            if args.len() != sd.args.len() {
                return Err(fmt_err(&format!("{} attend {} argument(s), reçu {}", nom, sd.args.len(), args.len()), ligne, 0));
            }
            // miroir Verificateur::verifier_args : le TYPE de chaque argument est
            // vérifié contre la table STDLIB (les messages exacts viennent du C++)
            verifier_type_std(nom, sd.args, args, env, prog, ligne)?;
            match sd.retour {
                "MIXTE" => {
                    let t = expr_type(prog, &args[0], env)?;
                    Ok(if t.n == TyKind::Booleen { Ty::simple(TyKind::Entier) } else { t })
                }
                "NUM_MAX" => {
                    let t1 = expr_type(prog, &args[0], env)?;
                    let t2 = expr_type(prog, &args[1], env)?;
                    Ok(promotion(&t1, &t2))
                }
                other => Ok(ty_du_nom(other)),
            }
        }
    }
}

// ── Repli de constantes (miroir de replier() du C++) ──────────────────────

// Calcule a op b sur la chaîne de littéral, comme le C++ : flottants en
// double, sinon rationnels exacts sur des i128 bornés ; si le nombre dépasse
// i128, on laisse l'expression telle quelle (évaluée à l'exécution, même
// résultat exact).
/// Replie une opération binaire sur des littéraux numériques quand au moins
/// un opérande est un littéral complexe (i, 4i, 3+4i...). Retourne None si
/// l'opération n'est pas repliable (opérateur non supporté, opérandes non
/// littéraux, division par zéro).
fn replier_binaire_complexe(op: &str, a: &Expr, b: &Expr, ligne: usize) -> Option<Expr> {
    fn cplx(e: &Expr) -> Option<(f64, f64)> {
        match e {
            Expr::LitComplexe { re, im, .. } => Some((*re, *im)),
            Expr::Litteral { texte, flott, .. } => {
                let v: f64 = if *flott {
                    texte.parse().ok()?
                } else {
                    texte.parse::<i64>().ok()? as f64
                };
                Some((v, 0.0))
            }
            _ => None,
        }
    }
    let (ar, ai) = cplx(a)?;
    let (br, bi) = cplx(b)?;
    let (re, im) = match op {
        "+" => (ar + br, ai + bi),
        "-" => (ar - br, ai - bi),
        "*" => (ar * br - ai * bi, ar * bi + ai * br),
        "/" => {
            let denom = br * br + bi * bi;
            if denom == 0.0 { return None; }
            ((ar * br + ai * bi) / denom, (ai * br - ar * bi) / denom)
        }
        _ => return None,
    };
    // résultat purement réel → littéral réel (entier quand possible)
    if im == 0.0 {
        if re == re.floor() && re.abs() < 1e15 && (re as i64 as f64) == re {
            return Some(Expr::Litteral { texte: (re as i64).to_string(), flott: false, rationnel: false, neg: false, ty: TyKind::Entier, ligne });
        }
        return Some(Expr::Litteral { texte: texte_flottant(re), flott: true, rationnel: false, neg: false, ty: TyKind::Flottant, ligne });
    }
    Some(Expr::LitComplexe { re, im, ligne })
}

pub fn replier(e: &Expr) -> Expr {
    match e {
        Expr::Binaire { op, a, b, ligne } => {
            let ligne = *ligne;
            let a = replier(a);
            let b = replier(b);
            if let (Expr::Litteral { texte: ta, flott: fa, .. },
                    Expr::Litteral { texte: tb, flott: fb, .. }) = (&a, &b) {
                // si les deux littéraux sont des entiers exacts (pas de '.'
                // ni d'exposant), on garde la voie rationnelle (exacte).
                let les_deux_ents = !ta.contains('.') && !ta.contains('e') && !ta.contains('E')
                    && !tb.contains('.') && !tb.contains('e') && !tb.contains('E');
                if (fa.clone() || fb.clone()) && !les_deux_ents {
                    let va = lit_d(ta, *fa);
                    let vb = lit_d(tb, *fb);
                    let r = match op.as_str() {
                        "+" => Some(va + vb), "-" => Some(va - vb), "*" => Some(va * vb),
                        // miroir du C++ (fold_bin) : une division flottante par zéro au
                        // repli → return nullptr → on NE replie pas ; l'erreur « division
                        // par zéro » n'est levée qu'au runtime, via binaire().
                        "/" => { if vb == 0.0 { None } else { Some(va / vb) } }
                        "^" => Some(va.powf(vb)),
                        _ => None,
                    };
                    if let Some(r) = r {
                        if !r.is_nan() && !r.is_infinite() {
                            return Expr::Litteral { texte: texte_flottant(r), flott: true, rationnel: false, neg: false, ty: TyKind::Flottant, ligne };
                        }
                        return a_or(e.clone(), &a, &b, op);
                    }
                    return a_or(e.clone(), &a, &b, op);
                }
                // rationnel / entier exact
                if !matches!(op.as_str(), "==" | "!=" | "%" | "<" | "<=" | ">" | ">=") {
                    if let (Some(qa), Some(qb)) = (lit_q(ta), lit_q(tb)) {
                        use num_bigint::BigInt;
                        let zero = BigInt::from(0);
                        let un = BigInt::from(1);
                        let r: Option<(BigInt, BigInt)> = match op.as_str() {
                            "+" => {
                                Some(((&qa.0 * &qb.1) + (&qb.0 * &qa.1), &qa.1 * &qb.1))
                            }
                            "-" => {
                                Some(((&qa.0 * &qb.1) - (&qb.0 * &qa.1), &qa.1 * &qb.1))
                            }
                            "*" => Some(((&qa.0 * &qb.0), (&qa.1 * &qb.1))),
                            "/" => {
                                if qb.0 == zero { crate::run::erreur_front("Division par zéro dans une constante", ligne, 0); }
                                else { Some(((&qa.0 * &qb.1), (&qa.1 * &qb.0))) }
                            }
                            "div" => {
                                if qb.0 == zero { crate::run::erreur_front("division par zéro dans une constante", ligne, 0); }
                                else {
                                    let n = &qa.0 * &qb.1;
                                    let d = &qa.1 * &qb.0;
                                    let mut q = &n / &d;
                                    if (&n % &d) != zero && ((n.sign() != d.sign())) { q = q - BigInt::from(1); }
                                    Some((q, un.clone()))
                                }
                            }
                            "^" => {
                                // exposant petit
                                if qb.1 != un { None }
                                else {
                                    let e_i = qb.0.to_string().parse::<i64>().ok();
                                    match e_i {
                                        Some(e) if e >= 0 && e <= 64 => {
                                            let mut acc = BigInt::from(1);
                                            for _ in 0..e { acc = acc * &qa.0; }
                                            Some((acc, un.clone()))
                                        }
                                        Some(e) if e < 0 && qa.0 == zero => {
                                            crate::run::erreur_front("0^négatif dans une constante", ligne, 0);
                                        }
                                        _ => None,
                                    }
                                }
                            }
                            _ => None,
                        };
                        if let Some((mut n, mut d)) = r {
                            if d < zero { n = -n; d = -d; }
                            let g = {
                                let an = if n < zero { -n.clone() } else { n.clone() };
                                let ad = if d < zero { -d.clone() } else { d.clone() };
                                pgcd128(an, ad)
                            };
                            let g = if g == zero { un.clone() } else { g };
                            n = &n / &g; d = &d / &g;
                            let texte = if d == un { n.to_string() } else { format!("{}/{}", n, d) };
                            return Expr::Litteral { texte, flott: false, rationnel: d != un, neg: false,
                                ty: if d == un { TyKind::Entier } else { TyKind::Rationnel }, ligne };
                        }
                    }
                }
            }
            // repli des binaires sur complexes (littéraux imaginaires : i, 4i, ...)
            if let Some(r) = replier_binaire_complexe(op, &a, &b, ligne) {
                return r;
            }
            Expr::Binaire { op: op.clone(), a: Box::new(a), b: Box::new(b), ligne }
        }
        Expr::Unaire { op, a, ligne } => {            let ligne = *ligne;
            let a = replier(a);
            if op == "-" {
                if let Expr::Litteral { texte, flott, rationnel, neg, ty, .. } = &a {
                    let t = if texte.starts_with('-') { texte[1..].to_string() } else { format!("-{}", texte) };
                    return Expr::Litteral { texte: t, flott: *flott, rationnel: *rationnel, neg: !neg, ty: *ty, ligne };
                }
            }
            Expr::Unaire { op: op.clone(), a: Box::new(a), ligne }
        }
        Expr::Index { a, b, ligne } => Expr::Index { a: Box::new(replier(a)), b: Box::new(replier(b)), ligne: *ligne },
        Expr::Appel { nom, module, args, ligne }
            if module.is_empty() && (nom == "i" || nom == "__i") && args.is_empty() =>
        {
            Expr::LitComplexe { re: 0.0, im: 1.0, ligne: *ligne }
        }
        Expr::Appel { nom, module, args, ligne } => Expr::Appel {
            nom: nom.clone(), module: module.clone(), args: args.iter().map(replier).collect(), ligne: *ligne },
        Expr::ListeLit(l, ligne) => Expr::ListeLit(l.iter().map(replier).collect(), *ligne),
        Expr::TableLit(el, ligne) => {
            let el: Vec<(String, Expr)> = el.iter().map(|(k, v)| (k.clone(), replier(v))).collect();
            Expr::TableLit(el, *ligne)
        }
        _ => e.clone(),
    }
}

fn a_or(e: Expr, _a: &Expr, _b: &Expr, _op: &str) -> Expr {
    // flottant infini/nan : pas de repli, on garde l'opération
    e
}

fn lit_d(t: &str, _flott: bool) -> f64 {
    // « p/q » : strtod s'arrête à '/', on divise par le dénominateur (miroir C++)
    let d: f64 = t.parse().unwrap_or(f64::NAN);
    if let Some(p) = t.find('/') {
        let a: f64 = t[..p].parse().unwrap_or(f64::NAN);
        let b: f64 = t[p + 1..].parse().unwrap_or(f64::NAN);
        return a / b;
    }
    d
}

fn lit_q(t: &str) -> Option<(num_bigint::BigInt, num_bigint::BigInt)> {
    use num_bigint::BigInt;
    if let Some(p) = t.find('/') {
        let n: BigInt = t[..p].trim().parse().ok()?;
        let d: BigInt = t[p + 1..].trim().parse().ok()?;
        if d == BigInt::from(0) { return None; }
        Some((n, d))
    } else {
        let n: BigInt = t.trim().parse().ok()?;
        Some((n, BigInt::from(1)))
    }
}

fn pgcd128(a: num_bigint::BigInt, b: num_bigint::BigInt) -> num_bigint::BigInt {
    use num_bigint::BigInt;
    let mut a = a; let mut b = b;
    while b != BigInt::from(0) { let t = b.clone(); b = &a % &b; a = t; }
    a
}

// Formatteur « plus court fidèle » : miroir de texte_flottant (C++ %.*g).
pub fn texte_flottant(d: f64) -> String {
    if d == 0.0 { return if d.is_sign_negative() { "-0.0".into() } else { "0.0".into() }; }
    for p in 1..=17usize {
        let s = format!("{:.*e}", p - 1, d);
        // construit le texte %g correspondant à cette précision
        let g = format_g(d, p);
        if g.parse::<f64>() == Ok(d) {
            let mut out = g;
            if !out.contains('.') && !out.contains('e') && !out.contains("inf") && !out.contains("nan") {
                out.push_str(".0");
            }
            let _ = s;
            return out;
        }
    }
    let mut out = format_g(d, 17);
    if !out.contains('.') && !out.contains('e') { out.push_str(".0"); }
    out
}

// %.*g à la C : mantisse en p chiffres significatifs, notation scientifique
// si expo < -4 ou expo >= p.
fn format_g(d: f64, p: usize) -> String {
    if d.is_nan() { return "nan".into(); }
    if d.is_infinite() { return if d > 0.0 { "inf".into() } else { "-inf".into() }; }
    // L'arrondi correct vient du printf C ; on s'appuie sur un formatage Rust
    // équivalent puis on normalise (zéros superflus, séparateur d'expo).
    let m = format!("{:.*e}", p - 1, d); // ex : 1.23000e1
    let (mant, expo) = m.split_once('e').unwrap();
    let expo: i32 = expo.parse().unwrap();
    let digits: String = mant.chars().filter(|c| c.is_ascii_digit()).collect();
    let digits = digits.trim_end_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    let sign = if d < 0.0 || (d == 0.0 && d.is_sign_negative()) { "-" } else { "" };
    let nd = digits.len() as i32;
    if expo < -4 || expo >= p as i32 {
        // notation scientifique : d.dddde±X (expo >= 2 chiffres → +XX)
        let mut s = String::from(sign);
        s.push_str(&digits[0..1]);
        if digits.len() > 1 { s.push('.'); s.push_str(&digits[1..]); }
        let ea = expo.unsigned_abs();
        if expo < 0 || ea >= 10 { s.push('e'); } else { s.push('e'); }
        s.push(if expo < 0 { '-' } else { '+' });
        s.push_str(&ea.to_string());
        s
    } else if expo < 0 {
        let mut s = String::from(sign);
        s.push('0');
        s.push('.');
        for _ in 0..(-expo - 1) { s.push('0'); }
        s.push_str(digits);
        let _ = nd;
        s
    } else {
        let ip = expo as usize + 1;
        let mut s = String::from(sign);
        if digits.len() <= ip {
            s.push_str(digits);
            s.push_str(&"0".repeat(ip - digits.len()));
        } else {
            s.push_str(&digits[..ip]);
            s.push('.');
            s.push_str(&digits[ip..]);
        }
        s
    }
}

// ── Point d'entrée ────────────────────────────────────────────────────────

pub fn analyser(source: &str) -> Result<Programme, Err> {
    let tk = tokeniser(source)?;
    let mut p = Parser::nouveau(tk);
    let mut prog = p.programmer()?;
    // Repli de constantes AVANT la verification : le lexeme « i » est plie en
    // litteral imaginaire 0+1i par le lexeur, et le repli peut le fusionner
    // (« $i = 0 + i » -> 0+1i, « $i + 1 » -> 1+1i) alors que « i » est le
    // COMPTEUR ENTIER. On replie donc d'abord, on repare la signature du
    // compteur, puis on verifie : c'est le seul ordre qui donne la meme
    // decision aux voies « comp », JIT et interpreteur (parite avec le C++,
    // qui suit « i » par un etat de l'interpreteur).
    // repli de constantes sur les instructions
    prog.instructions = prog.instructions.iter().map(|i| replier_instr(i)).collect();
    for nom in prog.ordre.clone() {
        let sig = prog.signatures.get_mut(&nom).unwrap();
        if let Some(c) = &sig.corps {
            sig.corps = Some(c.iter().map(|i| replier_instr(i)).collect());
        }
    }
    // Le repli de constantes a pu FUSIONNER des litteraux en 0+1i / k+ki alors
    // que « i » est le COMPTEUR ENTIER du programme (« $i = 0 + i », le test
    // « @ (i < n) », l'increment « $i = $i + 1 »). Le parser ne peut pas le
    // voir sans borne de boucle en vue ; le C++ suit « i » par un etat de
    // l'interpreteur, on le reproduit par une pre-passe qui decide une fois
    // pour toutes si « i » est un entier. Sans cette passe, « comp » rejette
    // un programme que l'execution accepte (divergence des voies).
    let (i_compteur, i_boucle) = {
        // deux drapeaux : « i est entier » (compteur) et « i est bouclé »
        // (seule situation où le repli peut avoir écrasé un incrément).
        fn dans(b: &[Instr]) -> (bool, bool) {
            let mut a = (false, false);
            for i in b {
                let x = match i {
                    Instr::Pour { nom, corps, .. } => {
                        let c = dans(corps);
                        (nom == "i" || c.0, nom == "i" || c.1)
                    }
                    Instr::TantQue { test, corps, .. } => {
                        let c = dans(corps);
                        (est_cote_i(test) || c.0, est_cote_i(test) || c.1)
                    }
                    Instr::Si { test, alors, sinon, .. } => {
                        let c = dans(alors);
                        let d = sinon.as_ref().map(|s| dans(s)).unwrap_or((false, false));
                        (est_cote_i(test) || c.0 || d.0, est_cote_i(test) || c.1 || d.1)
                    }
                    Instr::Bloc(bb) => dans(bb),
                    Instr::Affect { nom, e, .. } => {
                        if nom != "i" { (false, false) }
                        else if matches!(e, Expr::Binaire { op, a, .. } if {
                                (op == "+" || op == "-") && matches!(&**a, Expr::Var(x, _) if x == "i")
                            }) {
                            // « $i = $i + 1 » : compteur ET bouclé
                            (true, true)
                        } else {
                            (matches!(e, Expr::Litteral { flott: false, rationnel: false, .. }), false)
                        }
                    }
                    _ => (false, false),
                };
                a.0 |= x.0; a.1 |= x.1;
            }
            a
        }
        fn dans0(b: &[Instr]) -> bool { let (a, _) = dans(b); a }
        let _ = dans0;
        let mut r = dans(&prog.instructions);
        for n in &prog.ordre {
            if let Some(c) = prog.signatures.get(n).and_then(|s| s.corps.as_deref()) {
                let x = dans(c); r.0 |= x.0; r.1 |= x.1;
            }
        }
        r
    };
    // le drapeau est partage par tout le processus (miroir de l'etat de
    // l'interpreteur C++) : les portes d'execution de interp.rs le lisent.
    I_COMPTEUR.with(|c| c.set(i_compteur || c.get()));
    I_BBOUCLE.with(|c| c.set(i_boucle || c.get()));
    {
        fn rep(i: &Instr, i_entier: bool) -> Instr {
            match i {
                // l'increment ecrase par le repli (« $i = k+ki ») : la signature
                // re == im != 0 n'a jamais d'usage legitime de l'imaginaire.
                Instr::Affect { nom, e, ligne } if nom == "i" && matches!(e, Expr::LitComplexe { re, im, .. } if *re == *im && *re != 0.0) => {
                    Instr::Affect { nom: nom.clone(),
                        e: Expr::Binaire { op: "+".into(),
                            a: Box::new(Expr::Var("i".into(), *ligne)),
                            b: Box::new(Expr::Litteral { texte: { let Expr::LitComplexe { re, .. } = &*e else { unreachable!() }; re.to_string() }, flott: false, rationnel: false, neg: false, ty: TyKind::Dyn, ligne: *ligne }),
                            ligne: *ligne },
                        ligne: *ligne }
                }
                Instr::Affect { nom, e, ligne } if nom == "i" && i_entier => match e {
                    Expr::LitComplexe { re, im, ligne: l } if *im == *re && *re != 0.0 => {
                        Instr::Affect { nom: nom.clone(),
                            e: Expr::Binaire { op: "+".into(),
                                a: Box::new(Expr::Var("i".into(), *l)),
                                b: Box::new(Expr::Litteral { texte: re.to_string(), flott: false, rationnel: false, neg: false, ty: TyKind::Dyn, ligne: *l }),
                                ligne: *l },
                            ligne: *ligne }
                    }
                    Expr::LitComplexe { re, im, ligne: l } if *re == 0.0 && *im == 1.0 => {
                        Instr::Affect { nom: nom.clone(), e: Expr::Var("i".into(), *l), ligne: *ligne }
                    }
                    _ => Instr::Affect { nom: nom.clone(), e: e.clone(), ligne: *ligne },
                },
                Instr::Si { test, alors, sinon, ligne } => Instr::Si {
                    test: reparer_test_i(test, i_entier),
                    alors: alors.iter().map(|x| rep(x, i_entier)).collect(),
                    sinon: sinon.as_ref().map(|s| s.iter().map(|x| rep(x, i_entier)).collect()),
                    ligne: *ligne },
                Instr::TantQue { test, corps, ligne } => {
                    // corps de « tant_que (i < n) » : les 0+1i du corps sont le
                    // COMPTEUR (miroir du dé-repli que le parser fait pour
                    // « tant_que », que le raccourci « @ » ne traverse pas).
                    if est_boucle_i_repar_test(test) {
                        let t2 = { let mut x = test.clone(); deplier_i_rec(&mut x); x };
                        let c2: Vec<Instr> = corps.iter().map(|x| rep(x, true)).collect();
                        let c2 = c2.into_iter().map(|x| deplier_corps_i(&x)).collect();
                        Instr::TantQue { test: t2, corps: c2, ligne: *ligne }
                    } else {
                        Instr::TantQue {
                            test: reparer_test_i(test, i_entier),
                            corps: corps.iter().map(|x| rep(x, i_entier)).collect(), ligne: *ligne }
                    }
                }
                Instr::Pour { nom, debut, fin, corps, ligne } => {
                    // « pour i de … » : le mot « i » des bornes ET du corps est la
                    // VARIABLE de boucle, jamais l'imaginaire 0+1i. Sans ce
                    // dé-repli, « >> i » affichait le littéral au lieu du compteur.
                    if nom == "i" {
                        let mut d2 = debut.clone();
                        let mut f2 = fin.clone();
                        deplier_i_rec(&mut d2);
                        deplier_i_rec(&mut f2);
                        let c2: Vec<Instr> = corps.iter().map(|x| rep(x, true)).collect();
                        let c2 = c2.into_iter().map(|x| deplier_corps_i(&x)).collect();
                        Instr::Pour { nom: nom.clone(), debut: d2, fin: f2, corps: c2, ligne: *ligne }
                    } else {
                        Instr::Pour { nom: nom.clone(), debut: debut.clone(), fin: fin.clone(),
                            corps: corps.iter().map(|x| rep(x, i_entier)).collect(), ligne: *ligne }
                    }
                }
                Instr::Bloc(b) => Instr::Bloc(b.iter().map(|x| rep(x, i_entier)).collect()),
                other => other.clone(),
            }
        }
        fn est_boucle_i_repar_test(test: &Expr) -> bool {
            if let Expr::Binaire { op, a, b, .. } = test {
                if matches!(op.as_str(), "<" | "<=" | ">" | ">=") {
                    return est_cote_i(a) || est_cote_i(b);
                }
            }
            false
        }
        fn reparer_test_i(test: &Expr, i_entier: bool) -> Expr {
            if let Expr::Binaire { op, a, b, .. } = test {
                if matches!(op.as_str(), "<" | "<=" | ">" | ">=") {
                    let l = est_cote_i(a);
                    let r = est_cote_i(b);
                    let est_i = (l && !r && est_surement_nombre(b))
                        || (r && !l && est_surement_nombre(a))
                        || (l && r && i_entier);
                    if est_i {
                        let mut t2 = test.clone();
                        deplier_i_rec(&mut t2);
                        return t2;
                    }
                }
            }
            test.clone()
        }
        prog.instructions = prog.instructions.iter().map(|x| rep(x, i_compteur)).collect();
        for nom in prog.ordre.clone() {
            let sig = prog.signatures.get_mut(&nom).unwrap();
            if let Some(c) = &sig.corps {
                let cc = c.clone();
                sig.corps = Some(cc.iter().map(|x| rep(x, i_compteur)).collect());
            }
        }
        verifier(&mut prog)?;
    }
    Ok(prog)
}

thread_local! {
    /// « i » est le COMPTEUR ENTIER du programme courant (decide par analyser,
    /// see i_compteur). Les portes d'execution de interp.rs s'y conforment pour
    /// que « $i = 3+3i » reste un VRAI complexe quand « i » n'est pas un
    /// compteur, et redevienne l'increment ecrase quand il l'est.
    static I_COMPTEUR: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// « i » est BOUCLÉ (pour i de …, tant_que (i < n), ou $i = $i + k) :
    /// seule situation où le repli de constantes a pu écraser un incrément.
    static I_BBOUCLE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// « i » est le compteur d'une boucle du programme courant.
pub fn i_est_boucle() -> bool { I_BBOUCLE.with(|c| c.get()) }

pub fn i_est_compteur() -> bool { I_COMPTEUR.with(|c| c.get()) }

/// Marque « i » comme compteur entier pour tout le processus. appele par
/// analyser() et par la reparation d'executer_source : les deux voies doivent
/// donner la meme decision au JIT comme a l'interpreteur.
pub fn marquer_i_compteur() { I_COMPTEUR.with(|c| c.set(true)); }

/// « i » est verrouille comme entier par le verificateur du programme courant
/// (la pre-passe de verifier() a trouve « $i = <entier> »). Les portes
/// d'execution de interp.rs l'admettent comme second signe du compteur :
/// analyser() peut ne pas l'avoir vu quand le verificateur est rappele sur un
/// AST deja repare.
pub fn i_est_entier_verifie() -> bool { I_ENTIER.with(|c| c.get()) }

thread_local! {
    static I_ENTIER: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// idem, version visible de l'interpréteur : la réparation de
/// executer_source l'applique aux corps de « tant_que (i < n) ».
pub fn deplier_corps_instr(i: &Instr) -> Instr { deplier_corps_i(i) }

/// dans le corps de « pour i de … », les 0+1i portés par un affichage ou une
/// instruction d'expression sont la VARIABLE de boucle.
fn deplier_corps_i(i: &Instr) -> Instr {
    match i {
        Instr::Afficher(es, l) => {
            let v: Vec<Expr> = es.iter().map(|e| { let mut x = e.clone(); deplier_i_rec(&mut x); x }).collect();
            Instr::Afficher(v, *l)
        }
        Instr::ExprStmt(e) => { let mut x = e.clone(); deplier_i_rec(&mut x); Instr::ExprStmt(x) }
        Instr::Si { test, alors, sinon, ligne } => Instr::Si {
            test: { let mut x = test.clone(); deplier_i_rec(&mut x); x },
            alors: alors.iter().map(deplier_corps_i).collect(),
            sinon: sinon.as_ref().map(|ss| ss.iter().map(deplier_corps_i).collect()),
            ligne: *ligne },
        Instr::TantQue { test, corps, ligne } => Instr::TantQue {
            test: { let mut x = test.clone(); deplier_i_rec(&mut x); x },
            corps: corps.iter().map(deplier_corps_i).collect(), ligne: *ligne },
        Instr::Bloc(b) => Instr::Bloc(b.iter().map(deplier_corps_i).collect()),
        other => other.clone(),
    }
}

/// cote d'un comparateur qui porte le compteur « i » : soit Var("i") deja
/// de-replie par le parser (« tant_que »), soit le 0+1i du lexeme « i ».
pub fn est_cote_i(e: &Expr) -> bool {
    match e {
        Expr::Var(x, _) => x == "i",
        Expr::LitComplexe { re, im, .. } => *re == 0.0 && *im == 1.0,
        _ => false,
    }
}

pub fn replier_instr(i: &Instr) -> Instr {
    match i {
        Instr::Afficher(exprs, l) => Instr::Afficher(exprs.iter().map(replier).collect(), *l),
        Instr::Lire { nom, ty, ligne } => Instr::Lire { nom: nom.clone(), ty: *ty, ligne: *ligne },
        Instr::Affect { nom, e, ligne } => Instr::Affect { nom: nom.clone(), e: replier(e), ligne: *ligne },
        Instr::AffectIndex { nom, index, valeur, ligne } => {
            Instr::AffectIndex { nom: nom.clone(), index: replier(index), valeur: replier(valeur), ligne: *ligne }
        },
        Instr::Si { test, alors, sinon, ligne } => Instr::Si {
            test: replier(test),
            alors: alors.iter().map(replier_instr).collect(),
            sinon: sinon.as_ref().map(|s| s.iter().map(replier_instr).collect()),
            ligne: *ligne,
        },
        Instr::TantQue { test, corps, ligne } => Instr::TantQue {
            test: replier(test),
            corps: corps.iter().map(replier_instr).collect(),
            ligne: *ligne,
        },
        Instr::Pour { nom, debut, fin, corps, ligne } => Instr::Pour {
            nom: nom.clone(),
            debut: replier(debut),
            fin: replier(fin),
            corps: corps.iter().map(replier_instr).collect(),
            ligne: *ligne,
        },
        Instr::Essayer { corps, attraper, finalment, ligne } => Instr::Essayer {
            corps: corps.iter().map(replier_instr).collect(),
            attraper: attraper.as_ref().map(|(v, c)| (v.clone(), c.iter().map(replier_instr).collect())),
            finalment: finalment.as_ref().map(|f| f.iter().map(replier_instr).collect()),
            ligne: *ligne,
        },
        Instr::Lever(e, l) => Instr::Lever(replier(e), *l),
        Instr::Retour(e, l) => Instr::Retour(e.as_ref().map(replier), *l),
        Instr::ExprStmt(e) => Instr::ExprStmt(replier(e)),
        Instr::Bloc(b) => Instr::Bloc(b.iter().map(replier_instr).collect()),
        Instr::Spawn(b, l) => Instr::Spawn(b.iter().map(replier_instr).collect(), *l),
    }
}

fn t_ligne(tk: &[Token], pos: usize) -> usize {
    tk.get(pos.min(tk.len()-1)).map(|t| t.ligne).unwrap_or(0)
}
