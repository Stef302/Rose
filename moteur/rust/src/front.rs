// ═══════════ Rose — moteur Rust : lexeur, AST, parseur, repli, vérificateur ═══════════
// Port fidèle du pipeline du moteur C++ (mêmes messages, mêmes positions).
#![allow(dead_code)]
use crate::ffi::{Q, Z};
use crate::float::texte_flottant;

#[derive(Clone)]
pub struct Erreur {
    pub message: String,
    pub ligne: i64,
    pub colonne: i64,
}

impl Erreur {
    pub fn new(m: impl Into<String>) -> Erreur {
        Erreur { message: m.into(), ligne: 0, colonne: 0 }
    }
    pub fn loc(m: impl Into<String>, ligne: i64, colonne: i64) -> Erreur {
        Erreur { message: m.into(), ligne, colonne }
    }
    pub fn ligne_seule(m: impl Into<String>, ligne: i64) -> Erreur {
        Erreur { message: m.into(), ligne, colonne: 0 }
    }
    pub fn texte(&self) -> String {
        if self.ligne != 0 {
            format!("[Ligne {}, Colonne {}] {}", self.ligne, self.colonne, self.message)
        } else {
            self.message.clone()
        }
    }
}
pub type R<T> = std::result::Result<T, Erreur>;

/* ── types ─────────────────────────────────────────────────────────── */

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TyN { Vide = 0, Entier = 1, Rationnel = 2, Flottant = 3, Booleen = 4, Chaine = 5, Liste = 6, Table = 7, Valeur = 8, Tampon = 9 }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Ty { pub n: TyN, pub elt: TyN }

impl Ty {
    pub fn nouveau(n: TyN) -> Ty { Ty { n, elt: TyN::Vide } }
    pub fn liste(elt: TyN) -> Ty { Ty { n: TyN::Liste, elt } }
    pub fn nom(&self) -> String {
        match self.n {
            TyN::Entier => "entier".into(),
            TyN::Rationnel => "rationnel".into(),
            TyN::Flottant => "flottant".into(),
            TyN::Booleen => "booleen".into(),
            TyN::Chaine => "chaine".into(),
            TyN::Vide => "vide".into(),
            TyN::Liste => format!("liste<{}>", Ty::nouveau(self.elt).nom()),
            TyN::Table => "table".into(),
            TyN::Tampon => "tampon".into(),
            TyN::Valeur => "valeur".into(),
        }
    }
}

/* ── jetons ────────────────────────────────────────────────────────── */

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tk {
    Eof, Ident, Mot, Nombre, Flottant, Chaine,
    Plus, Moins, Etoile, Slash, DeuxPts, DeuxPoints, Percent, Chapeau, Bang,
    Point, Virgule, DivEntier, Affect, LParen, RParen, LBrace, RBrace,
    LBracket, RBracket, Semi, FlecheD, FlecheG, Tilde, Arobase, PointDInterro,
    Question, Inf, Sup, InfEg, SupEg, DoubleEg, Diff, Environ, QMarkEg, TildeEg, Racine,
}

#[derive(Clone)]
pub struct Token { pub type_: Tk, pub texte: String, pub ligne: i64, pub colonne: i64 }

fn est_mot_cle(s: &str) -> bool {
    matches!(s, "fn" | "retour" | "si" | "sinon" | "tant_que" | "pour" | "de" | "vers"
        | "vrai" | "faux" | "et" | "ou" | "non" | "div"
        | "entier" | "rationnel" | "flottant" | "booleen" | "chaine" | "liste" | "table")
}
fn cp_lettre(c: char) -> bool { c.is_alphabetic() || c == '_' }
fn cp_ident_suite(c: char) -> bool { cp_lettre(c) || c.is_numeric() }

pub fn tokeniser(source: &str) -> R<Vec<Token>> {
    let cs: Vec<char> = source.chars().collect();
    let n = cs.len();
    let mut tokens: Vec<Token> = Vec::new();
    let (mut i, mut ligne, mut col): (usize, i64, i64) = (0, 1, 1);
    macro_rules! avancer {
        ($k:expr) => {{
            for _ in 0..$k {
                if i < n && cs[i] == '\n' { ligne += 1; col = 1; } else { col += 1; }
                i += 1;
            }
        }};
    }
    while i < n {
        let c = cs[i];
        if c == ' ' || c == '\t' || c == '\r' || c == '\n' { avancer!(1); continue; }
        if c == '#' || (c == '/' && i + 1 < n && cs[i + 1] == '/') {
            while i < n && cs[i] != '\n' { avancer!(1); }
            continue;
        }
        if c == '"' || c == '\'' {
            let quote = c;
            let (dl, dc) = (ligne, col);
            avancer!(1);
            let mut buf = String::new();
            loop {
                if i >= n { return Err(Erreur::loc("Chaîne non terminée", dl, dc)); }
                if cs[i] == '\\' {
                    if i + 1 >= n {
                        return Err(Erreur::loc("Échappement en fin de fichier", ligne, col));
                    }
                    match cs[i + 1] {
                        'n' => buf.push('\n'),
                        't' => buf.push('\t'),
                        '\\' | '"' | '\'' => buf.push(cs[i + 1]),
                        _ => { buf.push('\\'); buf.push(cs[i + 1]); }
                    }
                    avancer!(2);
                    continue;
                }
                if cs[i] == quote { avancer!(1); break; }
                buf.push(cs[i]);
                avancer!(1);
            }
            tokens.push(Token { type_: Tk::Chaine, texte: buf, ligne: dl, colonne: dc });
            continue;
        }
        if c.is_ascii_digit() {
            let (dl, dc) = (ligne, col);
            let debut = i;
            while i < n && cs[i].is_ascii_digit() { avancer!(1); }
            let mut flott = false;
            if i + 1 < n && cs[i] == '.' && cs[i + 1].is_ascii_digit() {
                flott = true;
                avancer!(1);
                while i < n && cs[i].is_ascii_digit() { avancer!(1); }
            }
            let texte: String = cs[debut..i].iter().collect();
            tokens.push(Token { type_: if flott { Tk::Flottant } else { Tk::Nombre }, texte, ligne: dl, colonne: dc });
            continue;
        }
        if c == 'π' {   // constante 3,14159… (flottant)
            tokens.push(Token { type_: Tk::Flottant, texte: "3.141592653589793".into(), ligne, colonne: col });
            avancer!(1);
            continue;
        }
        if c == '²' || c == '³' {   // x² ≡ x^2
            tokens.push(Token { type_: Tk::Chapeau, texte: "^".into(), ligne, colonne: col });
            tokens.push(Token { type_: Tk::Nombre,
                texte: if c == '²' { "2".into() } else { "3".into() }, ligne, colonne: col });
            avancer!(1);
            continue;
        }
        if c == '$' || cp_lettre(c) {
            let (dl, dc) = (ligne, col);
            let debut = i;
            let mut dollar = false;
            if c == '$' { dollar = true; avancer!(1); }
            if i >= n || !cp_lettre(cs[i]) {
                return Err(Erreur::loc("Attente d'un nom de variable après '$'", dl, dc));
            }
            while i < n && cp_ident_suite(cs[i]) { avancer!(1); }
            let texte: String = cs[debut..i].iter().collect();
            let nu = if dollar { &texte[1..] } else { &texte[..] };
            let tt = if dollar || est_mot_cle(nu) { Tk::Ident } else { Tk::Mot };
            tokens.push(Token { type_: tt, texte: nu.to_string(), ligne: dl, colonne: dc });
            continue;
        }
        if i + 1 < n {
            let (a, b) = (cs[i], cs[i + 1]);
            let paire = match (a, b) {
                ('<', '=') => Some(Tk::InfEg),
                ('>', '=') => Some(Tk::SupEg),
                ('=', '=') => Some(Tk::DoubleEg),
                ('!', '=') | ('/', '=') => Some(Tk::Diff),
                ('?', '=') => Some(Tk::QMarkEg),
                ('=', '~') | ('~', '=') => Some(Tk::TildeEg),
                _ => None,
            };
            if let Some(tt) = paire {
                tokens.push(Token { type_: tt, texte: format!("{}{}", a, b), ligne, colonne: col });
                avancer!(2);
                continue;
            }
        }
        if i + 1 < n && cs[i] == '>' && cs[i + 1] == '>' {
            tokens.push(Token { type_: Tk::FlecheD, texte: ">>".into(), ligne, colonne: col });
            avancer!(2);
            continue;
        }
        if i + 1 < n && cs[i] == '<' && cs[i + 1] == '<' {
            tokens.push(Token { type_: Tk::FlecheG, texte: "<<".into(), ligne, colonne: col });
            avancer!(2);
            continue;
        }
        if i + 1 < n && cs[i] == '-' && cs[i + 1] == '>' {
            tokens.push(Token { type_: Tk::PointDInterro, texte: "->".into(), ligne, colonne: col });
            avancer!(2);
            continue;
        }
        if c == '=' {
            tokens.push(Token { type_: Tk::Affect, texte: "=".into(), ligne, colonne: col });
            avancer!(1);
            continue;
        }
        if i + 1 < n && cs[i] == ':' && cs[i + 1] == ':' {
            tokens.push(Token { type_: Tk::DeuxPoints, texte: "::".into(), ligne, colonne: col });
            avancer!(2);
            continue;
        }
        let uni = match c {
            '≈' => Some(Tk::Environ), '≤' => Some(Tk::InfEg), '≥' => Some(Tk::SupEg),
            '≠' => Some(Tk::Diff), '×' | '⋅' | '·' => Some(Tk::Etoile), '÷' => Some(Tk::DivEntier),
            '←' => Some(Tk::Affect), '→' => Some(Tk::PointDInterro), '√' => Some(Tk::Racine),
            _ => None,
        };
        if let Some(tt) = uni {
            tokens.push(Token { type_: tt, texte: c.to_string(), ligne, colonne: col });
            avancer!(1);
            continue;
        }
        let car = match c {
            '+' => Some(Tk::Plus), '-' => Some(Tk::Moins), '*' => Some(Tk::Etoile),
            '/' => Some(Tk::Slash), ':' => Some(Tk::DeuxPts), '%' => Some(Tk::Percent),
            '^' => Some(Tk::Chapeau), '!' => Some(Tk::Bang), '.' => Some(Tk::Point),
            ',' => Some(Tk::Virgule), '?' => Some(Tk::Question), '(' => Some(Tk::LParen),
            ')' => Some(Tk::RParen), '{' => Some(Tk::LBrace), '}' => Some(Tk::RBrace),
            '[' => Some(Tk::LBracket), ']' => Some(Tk::RBracket), ';' => Some(Tk::Semi),
            '<' => Some(Tk::Inf), '>' => Some(Tk::Sup), '~' => Some(Tk::Tilde),
            '@' => Some(Tk::Arobase),
            _ => None,
        };
        if let Some(tt) = car {
            tokens.push(Token { type_: tt, texte: c.to_string(), ligne, colonne: col });
            avancer!(1);
            continue;
        }
        return Err(Erreur::loc(format!("Caractère inattendu '{}'", c), ligne, col));
    }
    tokens.push(Token { type_: Tk::Eof, texte: String::new(), ligne, colonne: col });
    Ok(tokens)
}

/* ── AST ───────────────────────────────────────────────────────────── */

#[derive(Clone)]
pub enum E {
    Litteral { texte: String, flott: bool, ligne: i64, typ: Ty },
    ChaineLit { valeur: String, ligne: i64 },
    BooleenLit { valeur: bool, ligne: i64 },
    ListeLit { elements: Vec<E>, ligne: i64, elt: TyN },
    Var { nom: String, ligne: i64 },
    Unaire { op: String, operande: Box<E>, ligne: i64 },
    Fact { operande: Box<E>, ligne: i64 },
    Binaire { op: String, gauche: Box<E>, droite: Box<E>, ligne: i64, typ: Ty },
    Appel { module: String, nom: String, args: Vec<E>, ligne: i64, typ: Ty },
    Index { base: Box<E>, index: Box<E>, ligne: i64, typ: Ty },
}

impl E {
    fn set_type(&mut self, t: Ty) {
        match self {
            E::Litteral { typ, .. } => *typ = t,
            E::Binaire { typ, .. } => *typ = t,
            E::Appel { typ, .. } => *typ = t,
            E::Index { typ, .. } => *typ = t,
            E::ListeLit { elt, .. } => *elt = t.elt,
            _ => {}
        }
    }
}

#[derive(Clone)]
pub enum I {
    Afficher { expr: E, ligne: i64 },
    Lire { nom: String, ligne: i64, typ_read: Ty },
    Affect { nom: String, expr: E, ligne: i64 },
    Si { test: E, alors: Vec<I>, sinon: Option<Vec<I>>, ligne: i64 },
    TantQue { test: E, corps: Vec<I>, ligne: i64 },
    Pour { var: String, debut: E, fin: E, corps: Vec<I>, ligne: i64 },
    Retour { expr: Option<E>, ligne: i64 },
    Expr { expr: E, ligne: i64 },
    Bloc { instrs: Vec<I>, ligne: i64 },
}

#[derive(Clone)]
pub struct Signature {
    pub nom: String,
    pub params: Vec<(String, Ty)>,
    pub retour: Ty,
    pub corps: Option<Vec<I>>,
    pub dll: String,
    pub dll_son: String,
    pub symbole: String,
    pub ligne: i64,
}

#[derive(Clone, Default)]
pub struct Programme {
    pub signatures: std::collections::BTreeMap<String, Signature>,
    pub instructions: Vec<I>,
}

#[derive(Default, Clone)]
pub struct Ann { pub dll: String, pub dll_son: String, pub sym: String, pub active: bool }

/* ── parseur ───────────────────────────────────────────────────────── */

pub struct Parser { tokens: Vec<Token>, pos: usize }

impl Parser {
    fn voir(&self, d: usize) -> Token {
        let p = (self.pos + d).min(self.tokens.len() - 1);
        self.tokens[p].clone()
    }
    fn suivant(&mut self) -> Token {
        let t = self.voir(0);
        if t.type_ != Tk::Eof { self.pos += 1; }
        t
    }
    fn est(&self, ks: &[Tk]) -> bool { ks.contains(&self.voir(0).type_) }
    fn est_nom(&self, texte: &str) -> bool {
        let t = self.voir(0);
        (t.type_ == Tk::Ident || t.type_ == Tk::Mot) && t.texte == texte
    }
    fn accepter(&mut self, ks: &[Tk]) -> bool {
        if self.est(ks) { self.suivant(); true } else { false }
    }
    fn accepter_nom(&mut self, texte: &str) -> bool {
        if self.est_nom(texte) { self.suivant(); true } else { false }
    }
    fn exiger(&mut self, tt: Tk, quoi: &str) -> R<Token> {
        let t = self.suivant();
        if t.type_ != tt {
            return Err(Erreur::loc(format!("Attendu {}, trouvé '{}'", quoi, t.texte), t.ligne, t.colonne));
        }
        Ok(t)
    }
    fn nom_ident(&mut self) -> R<String> {
        let t = self.suivant();
        if t.type_ != Tk::Ident && t.type_ != Tk::Mot {
            return Err(Erreur::loc(format!("Attendu un identifiant, trouvé '{}'", t.texte), t.ligne, t.colonne));
        }
        Ok(t.texte)
    }

    pub fn nouveau(source: &str) -> R<Parser> {
        Ok(Parser { tokens: tokeniser(source)?, pos: 0 })
    }

    pub fn programmer(&mut self) -> R<Programme> {
        let mut prog = Programme::default();
        let mut ann = Ann::default();
        while self.voir(0).type_ != Tk::Eof {
            if self.accepter(&[Tk::Semi]) { continue; }
            let t = self.voir(0);
            if t.type_ == Tk::Arobase
                && (self.voir(1).type_ == Tk::Ident || self.voir(1).type_ == Tk::Mot)
                && (self.voir(1).texte == "dll" || self.voir(1).texte == "sym")
            {
                self.suivant();
                let genre = self.nom_ident()?;
                self.exiger(Tk::LParen, "'(' après @dll/@sym")?;
                if genre == "dll" {
                    ann.dll = self.exiger(Tk::Chaine, "nom de bibliothèque")?.texte;
                    if self.accepter(&[Tk::Virgule]) {
                        ann.dll_son = self.exiger(Tk::Chaine, "nom de fichier")?.texte;
                    }
                } else {
                    ann.sym = self.exiger(Tk::Chaine, "nom de symbole")?.texte;
                }
                self.exiger(Tk::RParen, "')'")?;
                ann.active = true;
                continue;
            }
            if self.est_nom("fn") {
                let sig = self.def_fonction(&mut ann)?;
                ann = Ann::default();
                if prog.signatures.contains_key(&sig.nom) {
                    return Err(Erreur::ligne_seule(
                        format!("Fonction '{}' définie deux fois", sig.nom), sig.ligne));
                }
                prog.signatures.insert(sig.nom.clone(), sig);
                continue;
            }
            if ann.active {
                return Err(Erreur::loc("Annotation @dll/@sym sans fonction juste après", t.ligne, t.colonne));
            }
            prog.instructions.push(self.instruction()?);
        }
        Ok(prog)
    }

    fn def_fonction(&mut self, ann: &mut Ann) -> R<Signature> {
        let tok = self.suivant();
        let nom = self.nom_ident()?;
        self.exiger(Tk::LParen, "'('")?;
        let mut params: Vec<(String, Ty)> = Vec::new();
        if !self.est(&[Tk::RParen]) {
            loop {
                let pname = self.nom_ident()?;
                self.exiger(Tk::DeuxPts, "':' (nom: type)")?;
                let pt = self.type_rose()?;
                params.push((pname, pt));
                if !self.accepter(&[Tk::Virgule]) { break; }
            }
        }
        self.exiger(Tk::RParen, "')'")?;
        let mut retour = Ty::nouveau(TyN::Vide);
        let mut a_retour = false;
        if self.accepter(&[Tk::PointDInterro]) { retour = self.type_rose()?; a_retour = true; }
        let corps = if self.est(&[Tk::LBrace]) { Some(self.bloc()?) } else { None };
        if corps.is_none() && !a_retour {
            return Err(Erreur::ligne_seule(
                "Une fonction externe doit déclarer un type de retour", tok.ligne));
        }
        Ok(Signature {
            nom, params, retour, corps,
            dll: ann.dll.clone(), dll_son: ann.dll_son.clone(), symbole: ann.sym.clone(),
            ligne: tok.ligne,
        })
    }

    fn type_rose(&mut self) -> R<Ty> {
        let t = self.suivant();
        if t.type_ != Tk::Ident && t.type_ != Tk::Mot {
            return Err(Erreur::loc(format!("Attendu un type, trouvé '{}'", t.texte), t.ligne, t.colonne));
        }
        if t.texte == "liste" {
            self.exiger(Tk::Inf, "'<' de liste<T>")?;
            let elt = self.type_rose()?;
            self.exiger(Tk::Sup, "'>' de liste<T>")?;
            return Ok(Ty::liste(elt.n));
        }
        match t.texte.as_str() {
            "entier" => Ok(Ty::nouveau(TyN::Entier)),
            "rationnel" => Ok(Ty::nouveau(TyN::Rationnel)),
            "flottant" => Ok(Ty::nouveau(TyN::Flottant)),
            "booleen" => Ok(Ty::nouveau(TyN::Booleen)),
            "chaine" => Ok(Ty::nouveau(TyN::Chaine)),
            "table" => Ok(Ty::nouveau(TyN::Table)),
            _ => Err(Erreur::loc(format!("Type inconnu '{}'", t.texte), t.ligne, t.colonne)),
        }
    }

    fn instruction(&mut self) -> R<I> {
        let t = self.voir(0);
        let ligne = t.ligne;
        match t.type_ {
            Tk::LBrace => return Ok(I::Bloc { instrs: self.bloc()?, ligne }),
            Tk::FlecheD => { self.suivant(); return Ok(I::Afficher { expr: self.expression()?, ligne }); }
            Tk::FlecheG => {
                self.suivant();
                return Ok(I::Lire { nom: self.nom_ident()?, ligne, typ_read: Ty::nouveau(TyN::Entier) });
            }
            Tk::Question => {
                self.suivant();
                let test = self.parentheser()?;
                let alors = self.corps_chevron()?;
                return Ok(I::Si { test, alors, sinon: None, ligne });
            }
            Tk::Arobase => {
                self.suivant();
                let test = self.parentheser()?;
                let corps = self.corps_chevron()?;
                return Ok(I::TantQue { test, corps, ligne });
            }
            _ => {}
        }
        if t.type_ == Tk::Ident && t.texte == "si" {
            self.suivant();
            let test = self.parentheser()?;
            let alors = self.bloc()?;
            let mut sinon = None;
            if self.accepter_nom("sinon") {
                sinon = Some(if self.est(&[Tk::LBrace]) { self.bloc()? } else { vec![self.instruction()?] });
            }
            return Ok(I::Si { test, alors, sinon, ligne });
        }
        if t.type_ == Tk::Ident && t.texte == "tant_que" {
            self.suivant();
            return Ok(I::TantQue { test: self.parentheser()?, corps: self.bloc()?, ligne });
        }
        if t.type_ == Tk::Ident && t.texte == "pour" {
            self.suivant();
            let var = self.nom_ident()?;
            if !self.accepter_nom("de") {
                return Err(Erreur::loc("Attendu 'de' dans pour…de…vers", self.voir(0).ligne, self.voir(0).colonne));
            }
            let debut = self.expression()?;
            if !self.accepter_nom("vers") {
                return Err(Erreur::loc("Attendu 'vers' dans pour…de…vers", self.voir(0).ligne, self.voir(0).colonne));
            }
            let fin = self.expression()?;
            let corps = self.bloc()?;
            return Ok(I::Pour { var, debut, fin, corps, ligne });
        }
        if t.type_ == Tk::Ident && t.texte == "retour" {
            self.suivant();
            let expr = if self.est(&[Tk::Semi, Tk::RBrace, Tk::Eof]) {
                None
            } else {
                Some(self.expression()?)
            };
            return Ok(I::Retour { expr, ligne });
        }
        if (t.type_ == Tk::Ident || t.type_ == Tk::Mot) && self.voir(1).type_ == Tk::Affect {
            let nom = self.nom_ident()?;
            self.suivant();
            let expr = self.expression()?;
            return Ok(I::Affect { nom, expr, ligne });
        }
        let expr = self.expression()?;
        Ok(I::Expr { expr, ligne })
    }

    fn parentheser(&mut self) -> R<E> {
        self.exiger(Tk::LParen, "'(' ")?;
        let e = self.expression()?;
        self.exiger(Tk::RParen, "')'")?;
        Ok(e)
    }

    fn corps_chevron(&mut self) -> R<Vec<I>> {
        self.exiger(Tk::LBracket, "'[' de corps")?;
        let mut instrs = Vec::new();
        while !self.est(&[Tk::RBracket, Tk::Eof]) {
            instrs.push(self.instruction()?);
            if !self.accepter(&[Tk::Semi]) { break; }
        }
        self.exiger(Tk::RBracket, "']' de corps")?;
        Ok(instrs)
    }

    fn bloc(&mut self) -> R<Vec<I>> {
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

    fn expression(&mut self) -> R<E> { self.ou_expr() }

    fn ou_expr(&mut self) -> R<E> {
        let mut g = self.et_expr()?;
        while self.est_nom("ou") {
            self.suivant();
            let ligne = self.voir(0).ligne;
            let d = self.et_expr()?;
            g = E::Binaire { op: "ou".into(), gauche: Box::new(g), droite: Box::new(d), ligne, typ: Ty::nouveau(TyN::Vide) };
        }
        Ok(g)
    }
    fn et_expr(&mut self) -> R<E> {
        let mut g = self.non_expr()?;
        while self.est_nom("et") {
            self.suivant();
            let ligne = self.voir(0).ligne;
            let d = self.non_expr()?;
            g = E::Binaire { op: "et".into(), gauche: Box::new(g), droite: Box::new(d), ligne, typ: Ty::nouveau(TyN::Vide) };
        }
        Ok(g)
    }
    fn non_expr(&mut self) -> R<E> {
        if self.est_nom("non") {
            let t = self.suivant();
            let e = self.non_expr()?;
            return Ok(E::Unaire { op: "non".into(), operande: Box::new(e), ligne: t.ligne });
        }
        self.comparaison()
    }
    fn comparaison(&mut self) -> R<E> {
        let mut g = self.somme()?;
        loop {
            let t = self.voir(0);
            let op = match t.type_ {
                Tk::Inf => "<", Tk::InfEg => "<=", Tk::Sup => ">", Tk::SupEg => ">=",
                Tk::DoubleEg => "==", Tk::Diff => "!=", Tk::Environ => "≈",
                Tk::QMarkEg => "?=", Tk::TildeEg => "=~",
                _ => break,
            };
            self.suivant();
            let d = self.somme()?;
            g = E::Binaire { op: op.into(), gauche: Box::new(g), droite: Box::new(d), ligne: t.ligne, typ: Ty::nouveau(TyN::Vide) };
        }
        Ok(g)
    }
    fn somme(&mut self) -> R<E> {
        let mut g = self.produit()?;
        while self.est(&[Tk::Plus, Tk::Moins]) {
            let t = self.suivant();
            let op = if t.type_ == Tk::Plus { "+" } else { "-" };
            let d = self.produit()?;
            g = E::Binaire { op: op.into(), gauche: Box::new(g), droite: Box::new(d), ligne: t.ligne, typ: Ty::nouveau(TyN::Vide) };
        }
        Ok(g)
    }
    fn produit(&mut self) -> R<E> {
        let mut g = self.puissance()?;
        loop {
            let t = self.voir(0);
            let op = match t.type_ {
                Tk::Etoile => "*",
                Tk::Slash => "/",
                Tk::Percent => "%",
                Tk::DivEntier => "div",
                _ if self.est_nom("div") => "div",
                _ => break,
            };
            self.suivant();
            let d = self.puissance()?;
            g = E::Binaire { op: op.into(), gauche: Box::new(g), droite: Box::new(d), ligne: t.ligne, typ: Ty::nouveau(TyN::Vide) };
        }
        Ok(g)
    }
    fn puissance(&mut self) -> R<E> {
        let g = self.factorielle_post()?;
        if self.est(&[Tk::Chapeau]) {
            let t = self.suivant();
            let d = self.puissance()?;
            return Ok(E::Binaire { op: "^".into(), gauche: Box::new(g), droite: Box::new(d), ligne: t.ligne, typ: Ty::nouveau(TyN::Vide) });
        }
        Ok(g)
    }
    fn factorielle_post(&mut self) -> R<E> {
        let mut g = self.unaire()?;
        while self.est(&[Tk::Bang]) {
            let t = self.suivant();
            g = E::Fact { operande: Box::new(g), ligne: t.ligne };
        }
        Ok(g)
    }
    fn unaire(&mut self) -> R<E> {
        if self.est(&[Tk::Racine]) {
            let t = self.suivant();
            let e = self.unaire()?;
            return Ok(E::Binaire {
                op: "^".into(),
                gauche: Box::new(e),
                droite: Box::new(E::Litteral { texte: "0.5".into(), flott: true,
                    ligne: t.ligne, typ: Ty::nouveau(TyN::Vide) }),
                ligne: t.ligne, typ: Ty::nouveau(TyN::Vide),
            });
        }
        if self.est(&[Tk::Moins]) {
            let t = self.suivant();
            let e = self.unaire()?;
            return Ok(E::Unaire { op: "-".into(), operande: Box::new(e), ligne: t.ligne });
        }
        if self.est(&[Tk::Plus]) { self.suivant(); return self.unaire(); }
        self.postfixe()
    }
    fn litt_entier(&self, v: i64, ligne: i64) -> E {
        E::Litteral { texte: v.to_string(), flott: false, ligne, typ: Ty::nouveau(TyN::Vide) }
    }
    fn postfixe(&mut self) -> R<E> {
        let mut g = self.atome()?;
        loop {
            if self.est(&[Tk::LParen]) {
                let t = self.suivant();
                let (module, nom) = match &g {
                    E::Appel { module, nom, args, .. } if args.is_empty() && !module.is_empty() => {
                        (module.clone(), nom.clone())
                    }
                    E::Var { nom, .. } => (String::new(), nom.clone()),
                    _ => return Err(Erreur::loc("Seul un nom de fonction peut être appelé", t.ligne, t.colonne)),
                };
                let mut args = Vec::new();
                if !self.est(&[Tk::RParen]) {
                    loop {
                        args.push(self.expression()?);
                        if !self.accepter(&[Tk::Virgule]) { break; }
                    }
                }
                self.exiger(Tk::RParen, "')' de appel")?;
                g = E::Appel { module, nom, args, ligne: t.ligne, typ: Ty::nouveau(TyN::Vide) };
                continue;
            }
            if self.est(&[Tk::LBracket]) {
                let t = self.suivant();
                if self.est(&[Tk::DeuxPts]) {
                    // tranche s[:b]
                    self.suivant();
                    let debut = self.litt_entier(0, t.ligne);
                    let fin = if self.est(&[Tk::RBracket]) {
                        let lng = E::Appel { module: String::new(), nom: "longueur".into(), args: vec![g.clone()], ligne: t.ligne, typ: Ty::nouveau(TyN::Vide) };
                        lng
                    } else {
                        self.expression()?
                    };
                    self.exiger(Tk::RBracket, "']' de tranche")?;
                    let mut appel = E::Appel { module: String::new(), nom: "sous_chaine".into(), args: vec![g.clone(), debut, fin], ligne: t.ligne, typ: Ty::nouveau(TyN::Vide) };
                    g = appel;
                    continue;
                }
                let ix = self.expression()?;
                if self.est(&[Tk::DeuxPts]) {
                    // tranche s[a:b] ou s[a:]
                    self.suivant();
                    let debut = ix;
                    let fin = if self.est(&[Tk::RBracket]) {
                        E::Appel { module: String::new(), nom: "longueur".into(), args: vec![g.clone()], ligne: t.ligne, typ: Ty::nouveau(TyN::Vide) }
                    } else {
                        self.expression()?
                    };
                    self.exiger(Tk::RBracket, "']' de tranche")?;
                    let appel = E::Appel { module: String::new(), nom: "sous_chaine".into(), args: vec![g.clone(), debut, fin], ligne: t.ligne, typ: Ty::nouveau(TyN::Vide) };
                    g = appel;
                    continue;
                }
                self.exiger(Tk::RBracket, "']' d'index")?;
                g = E::Index { base: Box::new(g), index: Box::new(ix), ligne: t.ligne, typ: Ty::nouveau(TyN::Vide) };
                continue;
            }
            return Ok(g);
        }
    }
    fn atome(&mut self) -> R<E> {
        let t = self.voir(0).clone();
        match t.type_ {
            Tk::Nombre => {
                self.suivant();
                Ok(E::Litteral { texte: t.texte, flott: false, ligne: t.ligne, typ: Ty::nouveau(TyN::Vide) })
            }
            Tk::Flottant => {
                self.suivant();
                Ok(E::Litteral { texte: t.texte, flott: true, ligne: t.ligne, typ: Ty::nouveau(TyN::Vide) })
            }
            Tk::Chaine => {
                self.suivant();
                Ok(E::ChaineLit { valeur: t.texte, ligne: t.ligne })
            }
            Tk::LBracket => {
                self.suivant();
                let mut elements = Vec::new();
                if !self.est(&[Tk::RBracket]) {
                    loop {
                        elements.push(self.expression()?);
                        if !self.accepter(&[Tk::Virgule]) { break; }
                    }
                }
                self.exiger(Tk::RBracket, "']' de liste")?;
                Ok(E::ListeLit { elements, ligne: t.ligne, elt: TyN::Vide })
            }
            Tk::LParen => {
                self.suivant();
                let e = self.expression()?;
                self.exiger(Tk::RParen, "')'")?;
                Ok(e)
            }
            Tk::Ident | Tk::Mot => {
                if t.texte == "vrai" { self.suivant(); return Ok(E::BooleenLit { valeur: true, ligne: t.ligne }); }
                if t.texte == "faux" { self.suivant(); return Ok(E::BooleenLit { valeur: false, ligne: t.ligne }); }
                let mut module = String::new();
                if self.voir(1).type_ == Tk::DeuxPoints {
                    module = t.texte.clone();
                    self.suivant();
                    self.suivant();
                }
                let nom = self.nom_ident()?;
                if module.is_empty() {
                    Ok(E::Var { nom, ligne: t.ligne })
                } else {
                    Ok(E::Appel { module, nom, args: Vec::new(), ligne: t.ligne, typ: Ty::nouveau(TyN::Vide) })
                }
            }
            _ => Err(Erreur::loc(format!("Attente d'une expression, trouvé '{}'", t.texte), t.ligne, t.colonne)),
        }
    }
}

pub fn analyser(source: &str) -> R<Programme> {
    Parser::nouveau(source)?.programmer()
}

/* ── repli de constantes ───────────────────────────────────────────── */

fn ligne_de(e: &E) -> i64 {
    match e {
        E::Litteral { ligne, .. } | E::ChaineLit { ligne, .. } | E::BooleenLit { ligne, .. }
        | E::ListeLit { ligne, .. } | E::Var { ligne, .. } | E::Unaire { ligne, .. }
        | E::Fact { ligne, .. } | E::Binaire { ligne, .. } | E::Appel { ligne, .. }
        | E::Index { ligne, .. } => *ligne,
    }
}

fn lit_info(e: &E) -> Option<(bool, String)> {
    if let E::Litteral { texte, flott, .. } = e { Some((*flott, texte.clone())) } else { None }
}

fn valeur_q(texte: &str) -> Option<Q> { Q::depuis_texte(texte) }

fn litt_depuis_q(q: &Q, ligne: i64) -> E {
    let mut q = q.clone();
    q.canon();
    let num = q.numerateur();
    let den = q.denominateur();
    let (texte, typ) = if den.cmp(&Z::i(1)) == 0 {
        (num.texte(), Ty::nouveau(TyN::Entier))
    } else {
        (format!("{}/{}", num.texte(), den.texte()), Ty::nouveau(TyN::Rationnel))
    };
    E::Litteral { texte, flott: false, ligne, typ }
}

fn litt_depuis_z(z: &Z, ligne: i64) -> E {
    E::Litteral { texte: z.texte(), flott: false, ligne, typ: Ty::nouveau(TyN::Entier) }
}

fn fact_arbre(i: u64, j: u64) -> Z {
    if i > j { return Z::i(1); }
    if i == j { return Z::u(i); }
    let m = i + (j - i) / 2;
    fact_arbre(i, m).mul(&fact_arbre(m + 1, j))
}

fn float_litteral(t: &str) -> Option<f64> {
    let a = crate::float::prefis_flottant(t)?;
    match t.find('/') {
        Some(i) => { let b = crate::float::prefis_flottant(&t[i + 1..])?; Some(a / b) }
        None => Some(a),
    }
}

fn repli_binaire(op: &str, gf: bool, gt: &str, df: bool, dt: &str, ligne: i64) -> R<Option<E>> {
    if gf || df {
        // valeur flottante d'un littéral : préfixe strtod, et « p/q » →
        // double(p)/double(q) (miroir de vers_d — « 1/4 » vaut 0.25, pas 1.0)
        let a: f64 = match float_litteral(gt) { Some(v) => v, None => return Ok(None) };
        let b: f64 = match float_litteral(dt) { Some(v) => v, None => return Ok(None) };
        let r = match op {
            "+" => a + b,
            "-" => a - b,
            "*" => a * b,
            "/" => { if b == 0.0 { return Ok(None); } a / b }
            "^" => a.powf(b),
            _ => return Ok(None),
        };
        if r.is_nan() || r.is_infinite() { return Ok(None); }
        return Ok(Some(E::Litteral {
            texte: texte_flottant(r), flott: true, ligne, typ: Ty::nouveau(TyN::Flottant) }));
    }
    if matches!(op, "≈" | "?=" | "=~" | "==" | "!=" | "%") { return Ok(None); }
    let a = match valeur_q(gt) { Some(q) => q, None => return Ok(None) };
    let b = match valeur_q(dt) { Some(q) => q, None => return Ok(None) };
    match op {
        "+" => Ok(Some(litt_depuis_q(&a.add(&b), ligne))),
        "-" => Ok(Some(litt_depuis_q(&a.sub(&b), ligne))),
        "*" => Ok(Some(litt_depuis_q(&a.mul(&b), ligne))),
        "/" => {
            if b.sgn() == 0 {
                return Err(Erreur::ligne_seule("Division par zéro dans une constante", ligne));
            }
            Ok(Some(litt_depuis_q(&a.div(&b), ligne)))
        }
        "^" => {
            if b.denominateur().cmp(&Z::i(1)) != 0 { return Ok(None); }
            let expo = b.numerateur();
            if expo.sgn() < 0 && a.sgn() == 0 {
                return Err(Erreur::ligne_seule("0^négatif dans une constante", ligne));
            }
            if !expo.tient_i64() { return Ok(None); }
            let e = expo.en_i64();
            let (num, den) = if e >= 0 {
                (a.numerateur().pow_u(e as u64), a.denominateur().pow_u(e as u64))
            } else {
                let k = (-(e as i128)) as u64;
                (a.denominateur().pow_u(k), a.numerateur().pow_u(k))
            };
            let mut r = Q::avec_num_den(&num, &den);
            r.canon();
            Ok(Some(litt_depuis_q(&r, ligne)))
        }
        "div" => {
            if b.sgn() == 0 {
                return Err(Erreur::ligne_seule("division par zéro dans une constante", ligne));
            }
            let q = a.div(&b);
            Ok(Some(litt_depuis_z(&q.numerateur().fdiv_q(&q.denominateur()), ligne)))
        }
        _ => Ok(None),
    }
}

pub fn replier(e: E) -> R<E> {
    let ligne = ligne_de(&e);
    match e {
        E::ListeLit { elements, ligne, elt } => Ok(E::ListeLit {
            elements: elements.into_iter().map(replier).collect::<R<Vec<_>>>()?, ligne, elt }),
        E::Unaire { op, operande, ligne } => {
            let operande = replier(*operande)?;
            if op == "-" {
                if let Some((flott, texte)) = lit_info(&operande) {
                    if flott {
                        if let Ok(d) = texte.parse::<f64>() {
                            return Ok(E::Litteral {
                                texte: texte_flottant(-d), flott: true, ligne,
                                typ: Ty::nouveau(TyN::Flottant) });
                        }
                    } else if let Some(q) = valeur_q(&texte) {
                        return Ok(litt_depuis_q(&q.neg(), ligne));
                    }
                }
            }
            Ok(E::Unaire { op, operande: Box::new(operande), ligne })
        }
        E::Fact { operande, ligne } => {
            let operande = replier(*operande)?;
            if let Some((false, texte)) = lit_info(&operande) {
                if !texte.contains('/') {
                    if let Some(n) = Z::depuis_texte(&texte) {
                        if n.sgn() >= 0 && n.cmp(&Z::u(20000)) <= 0 {
                            if n.cmp(&Z::u(2)) < 0 {
                                return Ok(litt_depuis_z(&Z::i(1), ligne));
                            }
                            return Ok(litt_depuis_z(&fact_arbre(2, n.en_u64()), ligne));
                        }
                    }
                }
            }
            Ok(E::Fact { operande: Box::new(operande), ligne })
        }
        E::Binaire { op, gauche, droite, ligne, typ } => {
            let gauche = replier(*gauche)?;
            let droite = replier(*droite)?;
            if let (Some((gf, gt)), Some((df, dt))) = (lit_info(&gauche), lit_info(&droite)) {
                if let Some(r) = repli_binaire(&op, gf, &gt, df, &dt, ligne)? {
                    return Ok(r);
                }
            }
            Ok(E::Binaire { op, gauche: Box::new(gauche), droite: Box::new(droite), ligne, typ })
        }
        E::Appel { module, nom, args, ligne, typ } => Ok(E::Appel {
            module, nom, args: args.into_iter().map(replier).collect::<R<Vec<_>>>()?, ligne, typ }),
        E::Index { base, index, ligne, typ } => Ok(E::Index {
            base: Box::new(replier(*base)?), index: Box::new(replier(*index)?), ligne, typ }),
        autre => Ok(autre),
    }
}

pub fn replier_instr(i: I) -> R<I> {
    Ok(match i {
        I::Afficher { expr, ligne } => I::Afficher { expr: replier(expr)?, ligne },
        I::Affect { nom, expr, ligne } => I::Affect { nom, expr: replier(expr)?, ligne },
        I::Retour { expr, ligne } => I::Retour { expr: expr.map(replier).transpose()?, ligne },
        I::Expr { expr, ligne } => I::Expr { expr: replier(expr)?, ligne },
        I::Si { test, alors, sinon, ligne } => I::Si {
            test: replier(test)?,
            alors: alors.into_iter().map(replier_instr).collect::<R<Vec<_>>>()?,
            sinon: sinon.map(|s| s.into_iter().map(replier_instr).collect::<R<Vec<_>>>()).transpose()?,
            ligne,
        },
        I::TantQue { test, corps, ligne } => I::TantQue {
            test: replier(test)?,
            corps: corps.into_iter().map(replier_instr).collect::<R<Vec<_>>>()?,
            ligne,
        },
        I::Pour { var, debut, fin, corps, ligne } => I::Pour {
            var, debut: replier(debut)?, fin: replier(fin)?,
            corps: corps.into_iter().map(replier_instr).collect::<R<Vec<_>>>()?,
            ligne,
        },
        I::Bloc { instrs, ligne } => I::Bloc {
            instrs: instrs.into_iter().map(replier_instr).collect::<R<Vec<_>>>()?, ligne },
        autre => autre,
    })
}

pub fn replier_constants(prog: &mut Programme) -> R<()> {
    for sig in prog.signatures.values_mut() {
        if let Some(c) = &mut sig.corps {
            *c = std::mem::take(c).into_iter().map(replier_instr).collect::<R<Vec<_>>>()?;
        }
    }
    prog.instructions = std::mem::take(&mut prog.instructions)
        .into_iter().map(replier_instr).collect::<R<Vec<_>>>()?;
    Ok(())
}

/* ── vérificateur de types ─────────────────────────────────────────── */

fn rang(t: Ty) -> i32 {
    match t.n {
        TyN::Booleen => 0, TyN::Entier => 1, TyN::Rationnel => 2, TyN::Flottant => 3,
        _ => -1,
    }
}
fn est_numerique(t: Ty) -> bool {
    matches!(t.n, TyN::Entier | TyN::Rationnel | TyN::Flottant)
}
fn promotion(a: Ty, b: Ty) -> Ty {
    let r = rang(a).max(rang(b));
    match r {
        0 | 1 => Ty::nouveau(TyN::Entier),
        2 => Ty::nouveau(TyN::Rationnel),
        _ => Ty::nouveau(TyN::Flottant),
    }
}
fn droite_moins(e: &E) -> bool { matches!(e, E::Unaire { op, .. } if op == "-") }

pub struct EntreeStd {
    pub nom: &'static str,
    pub args: &'static [&'static str],
    pub retour: &'static str,
}

pub const STDLIB: &[EntreeStd] = &[
    EntreeStd { nom: "afficher", args: &["n"], retour: "vide" },
    EntreeStd { nom: "saisir_entier", args: &[], retour: "entier" },
    EntreeStd { nom: "saisir_flottant", args: &[], retour: "flottant" },
    EntreeStd { nom: "factorielle", args: &["entier"], retour: "entier" },
    EntreeStd { nom: "fibonacci", args: &["entier"], retour: "entier" },
    EntreeStd { nom: "pgcd", args: &["entier", "entier"], retour: "entier" },
    EntreeStd { nom: "ppcm", args: &["entier", "entier"], retour: "entier" },
    EntreeStd { nom: "puissance", args: &["num", "entier"], retour: "entier" },
    EntreeStd { nom: "est_premier", args: &["entier"], retour: "booleen" },
    EntreeStd { nom: "compter_premiers", args: &["entier"], retour: "entier" },
    EntreeStd { nom: "nb_chiffres", args: &["entier"], retour: "entier" },
    EntreeStd { nom: "nombre_pi", args: &["entier"], retour: "chaine" },
    EntreeStd { nom: "longueur", args: &["liste_ou_chaine_ou_table"], retour: "entier" },
    EntreeStd { nom: "table", args: &[], retour: "table" },
    EntreeStd { nom: "inserer", args: &["table", "tout", "tout"], retour: "vide" },
    EntreeStd { nom: "cle_existe", args: &["table", "tout"], retour: "booleen" },
    EntreeStd { nom: "supprimer", args: &["table", "tout"], retour: "booleen" },
    EntreeStd { nom: "cles", args: &["table"], retour: "MIXTE" },
    EntreeStd { nom: "valeurs", args: &["table"], retour: "MIXTE" },
    EntreeStd { nom: "joindre", args: &["liste", "liste_elt"], retour: "vide" },
    EntreeStd { nom: "ordre", args: &["chaine"], retour: "entier" },
    EntreeStd { nom: "caractere", args: &["entier"], retour: "chaine" },
    EntreeStd { nom: "sous_chaine", args: &["chaine", "entier", "entier"], retour: "chaine" },
    EntreeStd { nom: "concatener", args: &["liste"], retour: "chaine" },
    EntreeStd { nom: "lire_texte", args: &["chaine"], retour: "chaine" },
    EntreeStd { nom: "ecrire_texte", args: &["chaine", "chaine"], retour: "vide" },
    EntreeStd { nom: "nb_arguments", args: &[], retour: "entier" },
    EntreeStd { nom: "argument", args: &["entier"], retour: "chaine" },
    EntreeStd { nom: "afficher_erreur", args: &["tout"], retour: "vide" },
    EntreeStd { nom: "quitter", args: &["entier"], retour: "vide" },
    EntreeStd { nom: "abs", args: &["num"], retour: "MIXTE" },
    EntreeStd { nom: "min", args: &["num", "num"], retour: "NUM_MAX" },
    EntreeStd { nom: "max", args: &["num", "num"], retour: "NUM_MAX" },
    EntreeStd { nom: "partie_entiere", args: &["num"], retour: "entier" },
    EntreeStd { nom: "fraction", args: &["entier", "entier"], retour: "rationnel" },
    // aléatoire reproductible et tampons d'octets
    EntreeStd { nom: "graine", args: &["entier"], retour: "vide" },
    EntreeStd { nom: "hasard", args: &["entier"], retour: "entier" },
    EntreeStd { nom: "tampon", args: &["entier"], retour: "tampon" },
    EntreeStd { nom: "octets", args: &["chaine"], retour: "tampon" },
    EntreeStd { nom: "tampon_case", args: &["tampon", "entier"], retour: "entier" },
    EntreeStd { nom: "tampon_pose", args: &["tampon", "entier", "entier"], retour: "vide" },
    EntreeStd { nom: "remplir_hasard", args: &["tampon", "tampon"], retour: "vide" },
    EntreeStd { nom: "millisecondes", args: &[], retour: "entier" },
    EntreeStd { nom: "temps", args: &[], retour: "entier" },
    EntreeStd { nom: "duree", args: &["entier"], retour: "chaine" },
];

pub const DYNAMIQUE: &[EntreeStd] = &[
    EntreeStd { nom: "charger", args: &["chaine"], retour: "entier" },
    EntreeStd { nom: "fermer", args: &["entier"], retour: "vide" },
    EntreeStd { nom: "appeler0", args: &["entier", "chaine"], retour: "flottant" },
    EntreeStd { nom: "appeler1", args: &["entier", "chaine", "num"], retour: "flottant" },
    EntreeStd { nom: "appeler2", args: &["entier", "chaine", "num", "num"], retour: "flottant" },
];

/* Les temporaires internes du moteur portent la forme « t<chiffres> » :
   un utilisateur ne peut pas déclarer un tel nom (collision d'emplacements). */
fn verifier_nom_reserve(nom: &str, ligne: i64) -> R<()> {
    let b = nom.as_bytes();
    if b.len() > 1 && b[0] == b't' && b[1..].iter().all(|c| c.is_ascii_digit()) {
        return Err(Erreur::ligne_seule(
            format!("Le nom '{}' est réservé par le moteur", nom), ligne));
    }
    Ok(())
}

fn ty_nom(s: &str) -> Ty {    match s {
        "entier" => Ty::nouveau(TyN::Entier),
        "rationnel" => Ty::nouveau(TyN::Rationnel),
        "flottant" => Ty::nouveau(TyN::Flottant),
        "booleen" => Ty::nouveau(TyN::Booleen),
        "chaine" => Ty::nouveau(TyN::Chaine),
        "table" => Ty::nouveau(TyN::Table),
        "tampon" => Ty::nouveau(TyN::Tampon),
        _ => Ty::nouveau(TyN::Vide),
    }
}

fn chercher_std(liste: &'static [EntreeStd], nom: &str) -> Option<&'static EntreeStd> {
    liste.iter().find(|e| e.nom == nom)
}

type Env = std::collections::HashMap<String, Ty>;

pub struct Verificateur {
    pub signatures: std::collections::BTreeMap<String, Signature>,
    pub assume_retour: Ty,
    pub a_assume: bool,
}

impl Verificateur {
    pub fn verifier(prog: &mut Programme) -> R<()> {
        let mut v = Verificateur {
            signatures: std::mem::take(&mut prog.signatures),
            assume_retour: Ty::nouveau(TyN::Vide),
            a_assume: false,
        };
        let mut res = Ok(());
        for k in v.signatures.keys().cloned().collect::<Vec<_>>() {
            let mut sig = v.signatures.get(&k).unwrap().clone();
            res = v.verifier_signature(&mut sig);
            if res.is_ok() { v.signatures.insert(k, sig); }
            if res.is_err() { break; }
        }
        if res.is_ok() {
            let mut globales: Env = Default::default();
            let mut instrs = std::mem::take(&mut prog.instructions);
            for i in instrs.iter_mut() {
                res = v.instruction(i, &mut globales);
                if res.is_err() { break; }
            }
            prog.instructions = instrs;
        }
        prog.signatures = v.signatures;
        res
    }

    fn verifier_signature(&mut self, sig: &mut Signature) -> R<()> {
        if sig.corps.is_none() {
            for t in sig.params.iter().map(|p| p.1).chain(std::iter::once(sig.retour)) {
                if !matches!(t.n, TyN::Entier | TyN::Flottant | TyN::Booleen | TyN::Chaine) {
                    return Err(Erreur::ligne_seule(format!(
                        "Type {} non supporté aux frontières d'une fonction externe \
                         (utilisez entier/flottant/booleen/chaine)", t.nom()), sig.ligne));
                }
            }
            return Ok(());
        }
        let mut locals: Env = Default::default();
        for (p, t) in &sig.params {
            verifier_nom_reserve(p, sig.ligne)?;
            locals.insert(p.clone(), *t);
        }
        self.assume_retour = sig.retour;
        self.a_assume = true;
        let mut res = Ok(());
        if let Some(corps) = sig.corps.as_mut() {
            for i in corps.iter_mut() {
                res = self.instruction(i, &mut locals);
                if res.is_err() { break; }
            }
        }
        self.a_assume = false;
        res
    }

    fn bloc(&mut self, b: &mut [I], env: &mut Env) -> R<()> {
        for i in b.iter_mut() {
            self.instruction(i, env)?;
        }
        Ok(())
    }

    fn exiger_booleen(&self, t: Ty, ligne: i64, quoi: &str) -> R<()> {
        if t.n != TyN::Booleen {
            return Err(Erreur::ligne_seule(
                format!("La condition de {} doit être booléenne, trouvé {}", quoi, t.nom()), ligne));
        }
        Ok(())
    }

    fn instruction(&mut self, instr: &mut I, env: &mut Env) -> R<()> {
        match instr {
            I::Afficher { expr, ligne } => {
                let t = self.expression(expr, env)?;
                if t.n == TyN::Vide {
                    return Err(Erreur::ligne_seule("Rien à afficher ici", *ligne));
                }
            }
            I::Lire { nom, ligne, typ_read } => {
                if let Some(t) = env.get(nom).copied() {
                    if !matches!(t.n, TyN::Entier | TyN::Flottant | TyN::Rationnel) {
                        return Err(Erreur::ligne_seule(format!(
                            "Ne peut pas lire dans une variable {} (entier/flottant attendus)",
                            t.nom()), *ligne));
                    }
                    *typ_read = t;
                } else {
                    verifier_nom_reserve(nom, *ligne)?;
                    env.insert(nom.clone(), Ty::nouveau(TyN::Entier));
                    *typ_read = Ty::nouveau(TyN::Entier);
                }
            }
            I::Affect { nom, expr, ligne } => {
                let t = self.expression(expr, env)?;
                if t.n == TyN::Vide {
                    return Err(Erreur::ligne_seule("Impossible d'affecter une valeur sans type", *ligne));
                }
                let mut cible = t;
                if let Some(ex) = env.get(nom).copied() {
                    cible = ex;
                    if cible.n == TyN::Valeur || t.n == TyN::Valeur {
                        // dynamique
                    } else if t.n == TyN::Entier && ex.n == TyN::Rationnel {
                        // acceptable
                    } else if t != ex {
                        if !(promotion(t, ex) == ex && est_numerique(t) && est_numerique(ex)) {
                            return Err(Erreur::ligne_seule(format!(
                                "'{}' est {}, on ne peut pas y stocker du {}", nom, ex.nom(), t.nom()), *ligne));
                        }
                    }
                }
                verifier_nom_reserve(nom, *ligne)?;
                env.insert(nom.clone(), cible);
            }
            I::Si { test, alors, sinon, ligne } => {
                let t = self.expression(test, env)?;
                self.exiger_booleen(t, *ligne, "si")?;
                self.bloc(alors, env)?;
                if let Some(s) = sinon { self.bloc(s, env)?; }
            }
            I::TantQue { test, corps, ligne } => {
                let t = self.expression(test, env)?;
                self.exiger_booleen(t, *ligne, "tant_que")?;
                self.bloc(corps, env)?;
            }
            I::Pour { var, debut, fin, corps, ligne } => {
                if env.contains_key(var) {
                    return Err(Erreur::ligne_seule(
                        format!("La variable de boucle '{}' existe déjà", var), *ligne));
                }
                let td = self.expression(debut, env)?;
                let tf = self.expression(fin, env)?;
                if td.n != TyN::Entier || tf.n != TyN::Entier {
                    return Err(Erreur::ligne_seule("pour…de…vers exige des bornes entières", *ligne));
                }
                verifier_nom_reserve(var, *ligne)?;
                env.insert(var.clone(), Ty::nouveau(TyN::Entier));
                self.bloc(corps, env)?;
            }
            I::Retour { expr, ligne } => {
                if !self.a_assume {
                    return Err(Erreur::ligne_seule("'retour' hors de fonction", *ligne));
                }
                match expr {
                    None => {
                        if self.assume_retour.n != TyN::Vide {
                            return Err(Erreur::ligne_seule(format!(
                                "La fonction doit retourner un {}", self.assume_retour.nom()), *ligne));
                        }
                    }
                    Some(e) => {
                        let t = self.expression(e, env)?;
                        let att = self.assume_retour;
                        if t != att && !(est_numerique(t) && est_numerique(att)
                            && promotion(t, att) == att) {
                            return Err(Erreur::ligne_seule(format!(
                                "La fonction retourne {}, attendu {}", t.nom(), att.nom()), *ligne));
                        }
                    }
                }
            }
            I::Expr { expr, .. } => { self.expression(expr, env)?; }
            I::Bloc { instrs, .. } => { self.bloc(instrs, env)?; }
        }
        Ok(())
    }

    pub fn expression(&mut self, e: &mut E, env: &mut Env) -> R<Ty> {
        let t = self._expression(e, env)?;
        e.set_type(t);
        Ok(t)
    }

    fn _expression(&mut self, e: &mut E, env: &mut Env) -> R<Ty> {
        match e {
            E::Litteral { texte, flott, .. } => Ok(if *flott {
                Ty::nouveau(TyN::Flottant)
            } else if texte.contains('/') {
                Ty::nouveau(TyN::Rationnel)
            } else {
                Ty::nouveau(TyN::Entier)
            }),
            E::ChaineLit { .. } => Ok(Ty::nouveau(TyN::Chaine)),
            E::BooleenLit { .. } => Ok(Ty::nouveau(TyN::Booleen)),
            E::Var { nom, ligne } => {
                if let Some(t) = env.get(nom) {
                    Ok(*t)
                } else {
                    Err(Erreur::ligne_seule(format!("Variable '{}' utilisée sans valeur", nom), *ligne))
                }
            }
            E::ListeLit { elements, ligne, .. } => {
                if elements.is_empty() {
                    return Err(Erreur::ligne_seule("Liste vide : le type d'élément est introuvable", *ligne));
                }
                let mut t_elt = self.expression(&mut elements[0], env)?;
                for el in elements[1..].iter_mut() {
                    let t2 = self.expression(el, env)?;
                    if t2 != t_elt {
                        if est_numerique(t2) && est_numerique(t_elt) {
                            t_elt = promotion(t2, t_elt);
                        } else {
                            return Err(Erreur::ligne_seule(format!(
                                "Liste hétérogène : {} puis {}", t_elt.nom(), t2.nom()), *ligne));
                        }
                    }
                }
                if t_elt.n == TyN::Liste {
                    return Err(Erreur::ligne_seule("Listes de listes non supportées pour l'instant", *ligne));
                }
                Ok(Ty::liste(t_elt.n))
            }
            E::Index { base, index, ligne, .. } => {
                let tb = self.expression(base, env)?;
                let ti = self.expression(index, env)?;
                if tb.n != TyN::Liste && tb.n != TyN::Chaine && tb.n != TyN::Table && tb.n != TyN::Valeur {
                    return Err(Erreur::ligne_seule(format!(
                        "Index sur un {}, une liste est attendue", tb.nom()), *ligne));
                }
                if tb.n == TyN::Table || tb.n == TyN::Valeur {
                    if tb.n == TyN::Table && ti.n != TyN::Entier && ti.n != TyN::Rationnel && ti.n != TyN::Flottant
                        && ti.n != TyN::Chaine && ti.n != TyN::Booleen {
                        // type de clé invalide
                    }
                    return Ok(Ty { n: TyN::Valeur, elt: TyN::Vide });
                }
                if ti.n != TyN::Entier {
                    return Err(Erreur::ligne_seule(if tb.n == TyN::Chaine {
                        "Index d'une chaîne doit être entier"
                    } else {
                        "Index d'une liste doit être entier"
                    }, *ligne));
                }
                Ok(if tb.n == TyN::Chaine { Ty::nouveau(TyN::Chaine) } else { Ty::nouveau(tb.elt) })
            }
            E::Unaire { op, operande, ligne } => {
                let t = self.expression(operande, env)?;
                if op == "non" {
                    self.exiger_booleen(t, *ligne, "non")?;
                    return Ok(Ty::nouveau(TyN::Booleen));
                }
                if !est_numerique(t) {
                    return Err(Erreur::ligne_seule(format!("Unaire {} sur un {}", op, t.nom()), *ligne));
                }
                Ok(t)
            }
            E::Fact { operande, ligne } => {
                let t = self.expression(operande, env)?;
                if t.n != TyN::Entier {
                    return Err(Erreur::ligne_seule(
                        format!("Factorielle attend un entier, trouvé {}", t.nom()), *ligne));
                }
                Ok(Ty::nouveau(TyN::Entier))
            }
            E::Binaire { op, gauche, droite, ligne, .. } => {
                let op = op.clone();
                self.binaire(&op, gauche, droite, *ligne, env)
            }
            E::Appel { module, nom, args, ligne, .. } => {
                let module = module.clone();
                let nom = nom.clone();
                self.appel(&module, &nom, args, *ligne, env)
            }
        }
    }

    fn binaire(&mut self, op: &str, g: &mut E, d: &mut E, ligne: i64, env: &mut Env) -> R<Ty> {
        let tg = self.expression(g, env)?;
        let td = self.expression(d, env)?;
        let r = if op == "et" || op == "ou" {
            self.exiger_booleen(tg, ligne, "et/ou")?;
            self.exiger_booleen(td, ligne, "et/ou")?;
            Ty::nouveau(TyN::Booleen)
        } else if matches!(op, "==" | "!=" | "≈" | "?=" | "=~") {
            if tg == td || (est_numerique(tg) && est_numerique(td)) {
                Ty::nouveau(TyN::Booleen)
            } else {
                return Err(Erreur::ligne_seule(
                    format!("Comparaison {} {} {} impossible", tg.nom(), op, td.nom()), ligne));
            }
        } else if matches!(op, "<" | "<=" | ">" | ">=") {
            if (est_numerique(tg) && est_numerique(td)) || (tg.n == TyN::Chaine && td.n == TyN::Chaine) {
                Ty::nouveau(TyN::Booleen)
            } else {
                return Err(Erreur::ligne_seule(
                    format!("Ordre {} {} {} impossible", tg.nom(), op, td.nom()), ligne));
            }
        } else if op == "+" {
            if tg.n == TyN::Chaine && td.n == TyN::Chaine {
                Ty::nouveau(TyN::Chaine)
            } else if est_numerique(tg) && est_numerique(td) {
                promotion(tg, td)
            } else {
                return Err(Erreur::ligne_seule(
                    format!("Addition {} + {} impossible", tg.nom(), td.nom()), ligne));
            }
        } else if op == "-" || op == "*" {
            if est_numerique(tg) && est_numerique(td) {
                promotion(tg, td)
            } else {
                return Err(Erreur::ligne_seule(
                    format!("Opération {} {} {} impossible", tg.nom(), op, td.nom()), ligne));
            }
        } else if op == "/" {
            if est_numerique(tg) && est_numerique(td) {
                let r = promotion(tg, td);
                if r.n == TyN::Entier { Ty::nouveau(TyN::Rationnel) } else { r }
            } else {
                return Err(Erreur::ligne_seule(
                    format!("Division {} / {} impossible", tg.nom(), td.nom()), ligne));
            }
        } else if op == "%" {
            if tg.n == TyN::Entier && td.n == TyN::Entier {
                Ty::nouveau(TyN::Entier)
            } else {
                return Err(Erreur::ligne_seule("Le modulo exige deux entiers", ligne));
            }
        } else if op == "div" {
            if !(est_numerique(tg) || tg.n == TyN::Booleen) {
                return Err(Erreur::ligne_seule(
                    format!("div attend un nombre à gauche, trouvé {}", tg.nom()), ligne));
            }
            if !(est_numerique(td) || td.n == TyN::Booleen) {
                return Err(Erreur::ligne_seule(
                    format!("div attend un nombre à droite, trouvé {}", td.nom()), ligne));
            }
            if td.n == TyN::Flottant || tg.n == TyN::Flottant {
                Ty::nouveau(TyN::Flottant)
            } else {
                Ty::nouveau(TyN::Entier)
            }
        } else if op == "^" {
            if !est_numerique(tg) {
                return Err(Erreur::ligne_seule(format!("Puissance sur une base {}", tg.nom()), ligne));
            }
            if td.n == TyN::Flottant {
                Ty::nouveau(TyN::Flottant)
            } else if matches!(td.n, TyN::Entier | TyN::Rationnel | TyN::Booleen) {
                if droite_moins(d) {
                    if tg.n == TyN::Entier { Ty::nouveau(TyN::Rationnel) } else { tg }
                } else if tg.n == TyN::Entier {
                    Ty::nouveau(TyN::Entier)
                } else {
                    tg
                }
            } else {
                return Err(Erreur::ligne_seule(format!("Exposant {} impossible", td.nom()), ligne));
            }
        } else {
            return Err(Erreur::ligne_seule(format!("Opérateur '{}' inconnu", op), ligne));
        };
        Ok(r)
    }

    fn verifier_args(&mut self, args: &mut [E], sig: &'static EntreeStd, nom: &str,
                     ligne: i64, env: &mut Env) -> R<()> {
        if args.len() != sig.args.len() {
            return Err(Erreur::ligne_seule(format!("{} attend {} argument(s), reçu {}",
                nom, sig.args.len(), args.len()), ligne));
        }
        let mut t_elt = Ty::nouveau(TyN::Vide);
        let mut a_elt = false;
        for (a, t_att) in args.iter_mut().zip(sig.args.iter()) {
            let ta = self.expression(a, env)?;
            match *t_att {
                "chaine" => {
                    if ta.n != TyN::Chaine {
                        return Err(Erreur::ligne_seule(format!("{}: une chaine est attendue", nom), ligne));
                    }
                }
                "num" => {
                    if !est_numerique(ta) {
                        return Err(Erreur::ligne_seule(
                            format!("{}: un nombre est attendu, reçu {}", nom, ta.nom()), ligne));
                    }
                }
                "entier" => {
                    if ta.n != TyN::Entier {
                        return Err(Erreur::ligne_seule(
                            format!("{}: un entier est attendu, reçu {}", nom, ta.nom()), ligne));
                    }
                }
                "liste" => {
                    if ta.n != TyN::Liste {
                        return Err(Erreur::ligne_seule(
                            format!("{}: une liste est attendue, reçu {}", nom, ta.nom()), ligne));
                    }
                    t_elt = Ty::nouveau(ta.elt);   // type de l'ÉLÉMENT, pas de la liste
                    a_elt = true;
                }
                "liste_elt" => {
                    if a_elt && t_elt.n != TyN::Vide && ta != t_elt
                        && !(est_numerique(ta) && est_numerique(t_elt) && promotion(ta, t_elt) == t_elt) {
                        return Err(Erreur::ligne_seule(format!(
                            "{}: élément {} ne correspond pas à la liste {}", nom, ta.nom(), t_elt.nom()), ligne));
                    }
                }
                "liste_ou_chaine" => {
                    if ta.n != TyN::Liste && ta.n != TyN::Chaine && ta.n != TyN::Table {
                        return Err(Erreur::ligne_seule(
                            format!("{}: une liste est attendue, reçu {}", nom, ta.nom()), ligne));
                    }
                }
                "liste_ou_chaine_ou_table" => {
                    if ta.n != TyN::Liste && ta.n != TyN::Chaine && ta.n != TyN::Table
                        && ta.n != TyN::Tampon {
                        return Err(Erreur::ligne_seule(format!(
                            "{}: une liste, une chaîne ou une table est attendue, reçu {}", nom, ta.nom()), ligne));
                    }
                }
                "tampon" => {
                    if ta.n != TyN::Tampon {
                        return Err(Erreur::ligne_seule(
                            format!("{}: un tampon est attendu, reçu {}", nom, ta.nom()), ligne));
                    }
                }
                "tout" => { /* tout type accepté (afficher_erreur) */ }
                _ => {}
            }
        }
        Ok(())
    }

    fn appel(&mut self, module: &str, nom: &str, args: &mut [E], ligne: i64, env: &mut Env) -> R<Ty> {
        if module == "dynamique" {
            let sig = chercher_std(DYNAMIQUE, nom).ok_or_else(|| Erreur::ligne_seule(format!(
                "Fonction dynamique::{} inconnue (disponibles: charger, fermer, \
                 appeler0, appeler1, appeler2)", nom), ligne))?;
            self.verifier_args(args, sig, &format!("dynamique::{}", nom), ligne, env)?;
            return Ok(ty_nom(sig.retour));
        }
        if args.len() == 1 && matches!(nom, "entier" | "rationnel" | "flottant" | "chaine") {
            let cible = ty_nom(nom);
            let t = self.expression(&mut args[0], env)?;
            if cible.n == TyN::Chaine {
                return Ok(Ty::nouveau(TyN::Chaine));
            }
            if t.n == TyN::Chaine {
                if cible.n == TyN::Entier { return Ok(Ty::nouveau(TyN::Entier)); }
                if cible.n == TyN::Flottant { return Ok(Ty::nouveau(TyN::Flottant)); }
                return Err(Erreur::ligne_seule(
                    "Conversion chaine → rationnel non disponible, passez par flottant", ligne));
            }
            if !est_numerique(t) {
                return Err(Erreur::ligne_seule(
                    format!("Conversion {} → {} impossible", t.nom(), cible.nom()), ligne));
            }
            return Ok(cible);
        }
        if let Some(sig) = self.signatures.get(nom) {
            let sig = sig.clone();
            if args.len() != sig.params.len() {
                return Err(Erreur::ligne_seule(format!("{} attend {} argument(s), reçu {}",
                    sig.nom, sig.params.len(), args.len()), ligne));
            }
            for (a, (pnom, t_att)) in args.iter_mut().zip(sig.params.iter()) {
                let ta = self.expression(a, env)?;
                let _ = pnom;
                if ta != *t_att && !(est_numerique(ta) && est_numerique(*t_att)
                    && promotion(ta, *t_att) == *t_att) {
                    return Err(Erreur::ligne_seule(format!("Argument de {}: {} reçu, {} attendu",
                        sig.nom, ta.nom(), t_att.nom()), ligne));
                }
            }
            return Ok(sig.retour);
        }
        let sd = chercher_std(STDLIB, nom)
            .ok_or_else(|| Erreur::ligne_seule(format!("Fonction '{}' inconnue", nom), ligne))?;
        self.verifier_args(args, sd, nom, ligne, env)?;
        if sd.retour == "MIXTE" {
            let t = self.expression(&mut args[0], env)?;
            return Ok(if t.n == TyN::Booleen { Ty::nouveau(TyN::Entier) } else { t });
        }
        if sd.retour == "NUM_MAX" {
            let t1 = self.expression(&mut args[0], env)?;
            let t2 = self.expression(&mut args[1], env)?;
            return Ok(promotion(t1, t2));
        }
        Ok(ty_nom(sd.retour))
    }
}
