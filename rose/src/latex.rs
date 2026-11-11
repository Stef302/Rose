// Rose 2.0 — rendu LaTeX de l'AST.
// Transforme l'AST de `front.rs` (expressions, instructions, programme) en
// LaTeX mathématique : `\frac{1}{2}`, `\sqrt{2}`, `\sum_{j = 0}^{9}`, etc.
//
// Aucune dépendance externe : tout est fabriqué par `format!`.
//
// Utilisation (une fois le module branché dans `main.rs`) :
//     let tex = crate::latex::programme_latex(&prog);
//     println!("{}", tex);
//
// Les conventions adoptées :
//   * `\left|`/`\right|`, `\Re{}`, `\Im{}`, `\overline{}` pour le complexe ;
//   * `\pi`, `\varphi`, `\mathrm{e}` pour les constantes ;
//   * `\cdot` pour la multiplication, `\bmod` pour le modulo, `\frac{}{}`
//     pour la division ;
//   * les parenthèses ne sont ajoutées que si la priorité l'exige
//     (`+`/`-` < `*`/`/`/`%` < `^`, `^` associatif à droite).
//
// Licence MIT.

// Le module expose une API publique (expr_latex, instr_latex, programme_latex)
// qui n'est pas encore appelée par `main.rs` : sans cet allow, le linter
// `dead_code` signalerait chaque fonction tant que le module n'est pas
// branché.
#![allow(dead_code)]

use crate::front::{Expr, Instr, Programme};

// ── Priorités ─────────────────────────────────────────────────────────────

/// Échelle des priorités : plus le nombre est grand, plus l'expression
/// « colle » aux voisines. Elle est calquée sur celle du parseur Rose :
/// `ou` < `et` < comparaison < `+`/`-` < `*`/`/`/`%` < `^` < atome.
const P_OU: u8 = 0;
const P_ET: u8 = 1;
const P_COMPARAISON: u8 = 2;
const P_ADDITIF: u8 = 3;
const P_MULTIPLICATIF: u8 = 4;
const P_PUISSANCE: u8 = 5;
const P_ATOME: u8 = 6;

/// Priorité d'une expression : plus le nombre est grand, plus l'expression
/// « colle » aux voisines.
fn priorite(e: &Expr) -> u8 {
    match e {
        Expr::Binaire { op, .. } => match op.as_str() {
            "ou" => P_OU,
            "et" => P_ET,
            "<" | ">" | "<=" | ">=" | "=" | "==" | "!=" | "≈" => P_COMPARAISON,
            "+" | "-" => P_ADDITIF,
            "*" | "/" | "%" | "div" => P_MULTIPLICATIF,
            "^" => P_PUISSANCE,
            _ => P_ADDITIF,
        },
        // Le moins unaire et la factorielle « collent » comme une puissance :
        // `(-a)^{b}` et `(a!)^{b}` doivent rester parenthésés pour que le
        // LaTeX garde exactement le sens de l'AST (`-a^{b}` se lirait
        // `-(a^{b})`).
        Expr::Unaire { .. } | Expr::Factorielle(..) => P_PUISSANCE,
        Expr::Litteral { .. }
        | Expr::LitComplexe { .. }
        | Expr::ChaineLit(..)
        | Expr::BooleenLit(..)
        | Expr::Var(..)
        | Expr::Appel { .. }
        | Expr::Index { .. }
        | Expr::ListeLit(..)
        | Expr::TableLit(..) => P_ATOME,
    }
}

/// Rend une expression ; `min` est la priorité minimale exigée par le
/// parent. Les parenthèses sont ajoutées seulement si l'expression est
/// moins prioritaire que `min`.
fn expr_avec_priorite(e: &Expr, min: u8) -> String {
    let s = expr_brut(e);
    if priorite(e) < min { format!("({})", s) } else { s }
}

// ── Expressions ───────────────────────────────────────────────────────────

/// Rend une expression en LaTeX mathématique.
/// Une expression est « composée » si elle n'est pas un atome pur
/// (littéral, variable, booléen, chaîne, complexe). C'est le critère
/// utilisé par l'interpréteur et le JIT pour décider si la formule LaTeX
/// apporte une information que la seule valeur ne donne pas :
///   * `>> 42`          → atome → on affiche « 42 » ;
///   * `>> 1 + 2`       → composé → « 1 + 2   3 » ;
///   * `>> min(1, 2.5)` → composé → « \operatorname{min}(1, 2.5)   1 ».
/// Les deux voies (JIT et interpréteur) utilisent le MÊME prédicat
/// purement syntaxique → parité garantie sans coordination runtime.
pub fn formule_composee(e: &Expr) -> bool {
    match e {
        Expr::Litteral { .. } | Expr::LitComplexe { .. }
        | Expr::Var(..) | Expr::BooleenLit(..) | Expr::ChaineLit(..) => false,
        _ => true,
    }
}

pub fn expr_latex(e: &Expr) -> String {
    expr_latex_ctx(e, false)
}

/// Rend une expression en LaTeX, en KNOWLEDGE du fait que « i » est une
/// VARIABLE entière du programme (et non la constante imaginaire).
///
/// Le lexer replie le mot « i » en littéral complexe 0+1i AVANT que le
/// contexte soit connu (Rose partage le jeton entre la variable et la
/// constante). Sans cette information, « $i = 2 » s'afficherait
/// « \mathrm{i} » — la constante imaginaire — au lieu de la variable.
/// `i_est_variable` vient de l'interpréteur/JIT, qui sait si le programme
/// possède une variable entière « i ».
pub fn expr_latex_ctx(e: &Expr, i_est_variable: bool) -> String {
    if i_est_variable {
        // On dé-replie le littéral 0+1i en variable « i » : c'est le
        // même traitement que front::deplier_i_rec (le compteur de boucle).
        let mut copie = e.clone();
        crate::front::deplier_i_rec(&mut copie);
        return expr_latex(&copie);
    }
    expr_latex_interne(e)
}

/// Implémentation interne (sans dé-repli de « i »).
fn expr_latex_interne(e: &Expr) -> String {
    expr_avec_priorite(e, P_OU)
}

/// Rendu « nu » d'une expression (sans parenthèses ajoutées par le parent).
fn expr_brut(e: &Expr) -> String {
    match e {
        Expr::Litteral { texte, flott, rationnel, neg, .. } => litteral_latex(texte, *flott, *rationnel, *neg),
        Expr::LitComplexe { re, im, .. } => complexe_latex(*re, *im),
        Expr::ChaineLit(s, _) => format!("\\text{{{}}}", echapper_texte(s)),
        Expr::BooleenLit(v, _) => if *v { "\\mathrm{vrai}" } else { "\\mathrm{faux}" }.to_string(),
        Expr::Var(nom, _) => nom_variable(nom),
        Expr::Appel { nom, args, .. } => appel_latex(nom, args),
        Expr::Binaire { op, a, b, .. } => binaire_latex(op, a, b),
        Expr::Unaire { op, a, .. } => unaire_latex(op, a),
        Expr::Factorielle(a, _) => format!("{}!", expr_avec_priorite(a, P_ATOME)),
        Expr::Index { a, b, .. } => format!("{}_{{{}}}", expr_avec_priorite(a, P_ATOME), expr_brut(b)),
        Expr::ListeLit(elems, _) => {
            let inner: Vec<String> = elems.iter().map(expr_latex).collect();
            format!("\\left[{} \\right]", inner.join(", "))
        }
        Expr::TableLit(paires, _) => {
            let inner: Vec<String> = paires
                .iter()
                .map(|(cle, val)| format!("\\mathtt{{{}}} = {}", echapper_texte(cle), expr_latex(val)))
                .collect();
            format!("\\left\\{{{} \\right\\}}", inner.join(", "))
        }
    }
}

/// Littéral numérique : entier, flottant ou rationnel `p/q`.
fn litteral_latex(texte: &str, flott: bool, rationnel: bool, neg: bool) -> String {
    let mut s = String::new();
    if neg { s.push('-'); }
    if rationnel || (!flott && texte.contains('/')) {
        let (num, den) = texte.split_once('/').unwrap_or((texte, "1"));
        s.push_str(&format!("\\frac{{{}}}{{{}}}", num.trim(), den.trim()));
    } else if flott {
        // le point décimal est normalisé (Rose écrit « 2.5 »)
        s.push_str(&texte.replace(',', "."));
    } else {
        s.push_str(texte);
    }
    s
}

/// Littéral complexe `re + im·i`.
fn complexe_latex(re: f64, im: f64) -> String {
    let re_null = re == 0.0;
    let im_null = im == 0.0;
    if re_null && im_null { return "0".to_string(); }
    if re_null {
        if im == 1.0 { return "\\mathrm{i}".to_string(); }
        return format!("{}\\mathrm{{i}}", nombre_f64(im));
    }
    if im_null { return nombre_f64(re); }
    if im == 1.0 { return format!("{} + \\mathrm{{i}}", nombre_f64(re)); }
    if im == -1.0 { return format!("{} - \\mathrm{{i}}", nombre_f64(re)); }
    format!("{} + {}\\mathrm{{i}}", nombre_f64(re), nombre_f64(im))
}

/// Nom d'une variable : les constantes mathématiques ont leur forme LaTeX.
fn nom_variable(nom: &str) -> String {
    match nom {
        "pi" | "π" => "\\pi".to_string(),
        "phi" | "φ" => "\\varphi".to_string(),
        "e" => "\\mathrm{e}".to_string(),
        autre => autre.to_string(),
    }
}

/// Échappe les caractères specials de LaTeX dans un texte.
fn echapper_texte(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\textbackslash{}"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '$' => out.push_str("\\$"),
            '&' => out.push_str("\\&"),
            '#' => out.push_str("\\#"),
            '%' => out.push_str("\\%"),
            '_' => out.push_str("\\_"),
            '^' => out.push_str("\\textasciicircum{}"),
            '~' => out.push_str("\\textasciitilde{}"),
            autre => out.push(autre),
        }
    }
    out
}

/// Formate un `f64` sans partie décimale si la valeur est entière.
fn nombre_f64(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 { return format!("{}", v as i64); }
    format!("{}", v)
}

/// Opération binaire.
///
/// Les priorités minimales exigées aux opérandes suivent l'associativité :
/// à gauche (`a - b - c` = `(a - b) - c`) l'opérande de droite doit être
/// parenthésé s'il a la même priorité ; à droite (`a^{b^{c}}`) c'est
/// l'opérande de gauche qui doit l'être.
fn binaire_latex(op: &str, a: &Expr, b: &Expr) -> String {
    match op {
        "+" => format!(
            "{} + {}",
            expr_avec_priorite(a, P_ADDITIF),
            expr_avec_priorite(b, P_MULTIPLICATIF)
        ),
        "-" => format!(
            "{} - {}",
            expr_avec_priorite(a, P_ADDITIF),
            expr_avec_priorite(b, P_MULTIPLICATIF)
        ),
        "*" => format!(
            "{} \\cdot {}",
            expr_avec_priorite(a, P_MULTIPLICATIF),
            expr_avec_priorite(b, P_PUISSANCE)
        ),
        // `\frac{}{}` groupe déjà ses arguments : aucune parenthèse nécessaire.
        "/" => format!("\\frac{{{}}}{{{}}}", expr_brut(a), expr_brut(b)),
        "%" => format!(
            "{} \\bmod {}",
            expr_avec_priorite(a, P_MULTIPLICATIF),
            expr_avec_priorite(b, P_PUISSANCE)
        ),
        "div" => format!(
            "{} \\operatorname{{div}} {}",
            expr_avec_priorite(a, P_MULTIPLICATIF),
            expr_avec_priorite(b, P_PUISSANCE)
        ),
        // associativité à droite : `a^{b^{c}}`
        "^" => format!(
            "{}^{{{}}}",
            expr_avec_priorite(a, P_ATOME),
            expr_avec_priorite(b, P_PUISSANCE)
        ),
        "<" => format!(
            "{} < {}",
            expr_avec_priorite(a, P_COMPARAISON),
            expr_avec_priorite(b, P_ADDITIF)
        ),
        ">" => format!(
            "{} > {}",
            expr_avec_priorite(a, P_COMPARAISON),
            expr_avec_priorite(b, P_ADDITIF)
        ),
        "<=" => format!(
            "{} \\leq {}",
            expr_avec_priorite(a, P_COMPARAISON),
            expr_avec_priorite(b, P_ADDITIF)
        ),
        ">=" => format!(
            "{} \\geq {}",
            expr_avec_priorite(a, P_COMPARAISON),
            expr_avec_priorite(b, P_ADDITIF)
        ),
        "=" | "==" => format!(
            "{} = {}",
            expr_avec_priorite(a, P_COMPARAISON),
            expr_avec_priorite(b, P_ADDITIF)
        ),
        "!=" => format!(
            "{} \\neq {}",
            expr_avec_priorite(a, P_COMPARAISON),
            expr_avec_priorite(b, P_ADDITIF)
        ),
        "≈" => format!(
            "{} \\approx {}",
            expr_avec_priorite(a, P_COMPARAISON),
            expr_avec_priorite(b, P_ADDITIF)
        ),
        "et" => format!(
            "{} \\land {}",
            expr_avec_priorite(a, P_ET),
            expr_avec_priorite(b, P_COMPARAISON)
        ),
        "ou" => format!(
            "{} \\lor {}",
            expr_avec_priorite(a, P_OU),
            expr_avec_priorite(b, P_ET)
        ),
        autre => format!(
            "\\operatorname{{{}}}({}, {})",
            autre,
            expr_brut(a),
            expr_brut(b)
        ),
    }
}

/// Opération unaire (`-`, `non`).
fn unaire_latex(op: &str, a: &Expr) -> String {
    match op {
        "-" => format!("-{}", expr_avec_priorite(a, P_ATOME)),
        "+" => expr_avec_priorite(a, P_ATOME),
        "non" => format!("\\neg {}", expr_avec_priorite(a, P_ATOME)),
        autre => format!("\\operatorname{{{}}}({})", autre, expr_brut(a)),
    }
}

/// Appel de fonction : fonctions mathématiques connues, sinon `\operatorname`.
fn appel_latex(nom: &str, args: &[Expr]) -> String {
    let a0 = args.first().map(expr_brut).unwrap_or_default();
    match nom {
        "sin" => format!("\\sin({})", a0),
        "cos" => format!("\\cos({})", a0),
        "tan" => format!("\\tan({})", a0),
        "asin" => format!("\\arcsin({})", a0),
        "acos" => format!("\\arccos({})", a0),
        "atan" | "arctan" => format!("\\arctan({})", a0),
        "exp" => format!("\\exp({})", a0),
        "ln" => format!("\\ln({})", a0),
        "log" => format!("\\log({})", a0),
        "racine" | "sqrt" => format!("\\sqrt{{{}}}", a0),
        "modulus" | "abs" => format!("\\left|{}\\right|", a0),
        "re" => format!("\\Re{{{}}}", a0),
        "im" => format!("\\Im{{{}}}", a0),
        "conj" => format!("\\overline{{{}}}", a0),
        "i" => "\\mathrm{i}".to_string(),
        // constantes : le lexeur transforme « π » et « φ » en appels
        "pi" => "\\pi".to_string(),
        "phi" => "\\varphi".to_string(),
        _ => {
            let inner: Vec<String> = args.iter().map(expr_brut).collect();
            format!("\\operatorname{{{}}}({})", nom, inner.join(", "))
        }
    }
}

// ── Instructions ──────────────────────────────────────────────────────────

/// Rend une instruction en LaTeX.
pub fn instr_latex(i: &Instr) -> String {
    match i {
        Instr::Afficher(exprs, _) => {
            let inner: Vec<String> = exprs.iter().map(expr_latex).collect();
            format!("\\mathrm{{afficher}}\\left({}\\right)", inner.join(", "))
        }
        Instr::Affect { nom, e, .. } => format!("{} = {}", nom_variable(nom), expr_latex(e)),
        Instr::Si { test, alors, sinon, .. } => {
            let mut s = format!(
                "\\mathbf{{si}}\\ \\langle {}\\rangle\\ \\mathbf{{alors}}\\ \\langle {}\\rangle",
                expr_latex(test),
                bloc_latex(alors)
            );
            if let Some(sinon) = sinon {
                s.push_str(&format!(
                    "\\ \\mathbf{{sinon}}\\ \\langle {}\\rangle",
                    bloc_latex(sinon)
                ));
            }
            s
        }
        Instr::TantQue { test, corps, .. } => format!(
            "\\mathbf{{tant que}}\\ \\langle {}\\rangle\\ \\mathbf{{faire}}\\ \\langle {}\\rangle",
            expr_latex(test),
            bloc_latex(corps)
        ),
        Instr::Pour { nom, debut, fin, corps, .. } => format!(
            "\\sum_{{{} = {}}}^{{{}}} \\langle {}\\rangle",
            nom,
            expr_latex(debut),
            borne_fin_latex(fin),
            bloc_latex(corps)
        ),
        Instr::Retour(Some(e), _) => format!("\\mathbf{{retour}}\\ \\langle {}\\rangle", expr_latex(e)),
        Instr::Retour(None, _) => "\\mathbf{retour}".to_string(),
        Instr::ExprStmt(e) => expr_latex(e),
        Instr::Bloc(b) => bloc_latex(b),
        Instr::Lire { nom, .. } => format!("\\mathrm{{lire}}\\ \\langle {}\\rangle", nom),
        Instr::AffectIndex { nom, index, valeur, .. } => format!(
            "{}_{{{}}} = {}",
            nom_variable(nom),
            expr_latex(index),
            expr_latex(valeur)
        ),
        Instr::Spawn(corps, _) => format!("\\mathrm{{spawn}}\\ \\langle {}\\rangle", bloc_latex(corps)),
        Instr::Essayer { corps, attraper, finalment, .. } => {
            let mut s = format!("\\mathrm{{essayer}}\\ \\langle {}\\rangle", bloc_latex(corps));
            if let Some((var, c)) = attraper {
                s.push_str(&format!(
                    "\\ \\mathrm{{attraper}}\\ \\langle \\mathtt{{{}}} : {}\\rangle",
                    echapper_texte(var),
                    bloc_latex(c)
                ));
            }
            if let Some(c) = finalment {
                s.push_str(&format!("\\ \\mathrm{{finalement}}\\ \\langle {}\\rangle", bloc_latex(c)));
            }
            s
        }
        Instr::Lever(e, _) => format!("\\mathrm{{lever}}\\ \\langle {}\\rangle", expr_latex(e)),
    }
}

/// Rendu d'un bloc : les instructions sont séparées par « ; ».
fn bloc_latex(instrs: &[Instr]) -> String {
    instrs.iter().map(instr_latex).collect::<Vec<String>>().join("; ")
}

/// Borne finale d'une boucle « pour » : la borne Rose est exclusive, donc on
/// rend `fin - 1` (calculé quand `fin` est un entier littéral).
fn borne_fin_latex(fin: &Expr) -> String {
    if let Expr::Litteral { texte, flott, rationnel, neg, .. } = fin {
        if !*flott && !*rationnel {
            if let Ok(v) = texte.parse::<i64>() {
                let v = if *neg { -v } else { v };
                return (v - 1).to_string();
            }
        }
    }
    format!("{} - 1", expr_latex(fin))
}

// ── Programme ─────────────────────────────────────────────────────────────

/// Rend un programme Rose en LaTeX : une instruction par ligne, séparées par
/// un saut de ligne LaTeX (`\\`).
pub fn programme_latex(p: &Programme) -> String {
    let mut s = String::new();
    // 1) les fonctions définies par le programme, dans l'ordre de
    //    définition : c'est la partie du programme qui porte les CALCULS.
    for nom in &p.ordre {
        if let Some(sig) = p.signatures.get(nom) {
            s.push_str(&fonction_latex(nom, sig));
            s.push('\n');
        }
    }
    // 2) le corps principal, instruction par instruction.
    let n = p.instructions.len();
    for (i, instr) in p.instructions.iter().enumerate() {
        if i > 0 { s.push('\n'); }
        s.push_str(&instr_latex(instr));
        if i + 1 < n { s.push_str(" \\\\"); }
    }
    s
}

/// Rend une fonction du programme : signature typée puis corps.
fn fonction_latex(nom: &str, sig: &crate::front::Signature) -> String {
    // Le type est FACULTATIF : un paramètre sans type déclaré (Dyn) et un
    // retour non déclaré (Vide ou Dyn) sont omis du rendu — on lit
    // « carre(x) » plutôt que « carre(x ∈ ?) → ? ».
    let params: Vec<String> = sig.params.iter()
        .map(|(n, t)| {
            if t.n == crate::front::TyKind::Dyn {
                n.clone()
            } else {
                format!("{} \\in {}", n, type_math_latex(t))
            }
        })
        .collect();
    let rendu = !(sig.retour.n == crate::front::TyKind::Vide
        || sig.retour.n == crate::front::TyKind::Dyn);
    let mut s = if rendu {
        format!(
            "\\operatorname{{{}}}({}) \\to {}",
            nom,
            params.join(", "),
            type_math_latex(&sig.retour)
        )
    } else {
        format!("\\operatorname{{{}}}({})", nom, params.join(", "))
    };
    match &sig.corps {
        Some(corps) => {
            s.push_str(" \\ \\langle ");
            s.push_str(&bloc_latex(corps));
            s.push_str(" \\rangle");
        }
        None => {
            // déclaration externe (FFI) : pas de corps à rendre.
            s.push_str(" \\ \\text{(fonction externe)}");
        }
    }
    s
}

/// Rend un type comme un ENSEMBLE mathematique : « \\mathbb{Z} »,
/// « \\mathbb{R} », « \\mathbb{Q} », « \\mathbb{C} », etc.
/// C'est la forme demandee pour les signatures de fonctions : on lit
/// « fib(n \\in \\mathbb{Z}) \\to \\mathbb{Z} » plutot que
/// « fib(n : entier) \\to entier ».
fn type_math_latex(t: &crate::front::Ty) -> String {
    use crate::front::TyKind;
    match t.n {
        TyKind::Entier => "\\mathbb{Z}".to_string(),
        TyKind::Rationnel => "\\mathbb{Q}".to_string(),
        TyKind::Flottant => "\\mathbb{R}".to_string(),
        TyKind::Booleen => "\\mathbb{B}".to_string(),
        TyKind::Chaine => "\\mathrm{chaine}".to_string(),
        TyKind::Complexe => "\\mathbb{C}".to_string(),
        TyKind::Irrationnel => "\\mathbb{I}".to_string(),
        TyKind::Liste => "\\mathrm{liste}".to_string(),
        TyKind::Table => "\\mathrm{table}".to_string(),
        TyKind::Tableau => "\\mathrm{tableau}".to_string(),
        TyKind::Tampon => "\\mathrm{tampon}".to_string(),
        TyKind::Vide => "\\mathrm{vide}".to_string(),
        TyKind::Dyn => "?".to_string(),
    }
}

/// Nom LaTeX d'un type Rose.
fn type_latex(t: &crate::front::Ty) -> String {
    use crate::front::TyKind;
    let base = match t.n {
        TyKind::Entier => "\\mathrm{entier}".to_string(),
        TyKind::Rationnel => "\\mathrm{rationnel}".to_string(),
        TyKind::Flottant => "\\mathrm{flottant}".to_string(),
        TyKind::Booleen => "\\mathrm{booleen}".to_string(),
        TyKind::Chaine => "\\mathrm{chaine}".to_string(),
        TyKind::Complexe => "\\mathrm{complexe}".to_string(),
        TyKind::Irrationnel => "\\mathrm{irrationnel}".to_string(),
        TyKind::Liste => "\\mathrm{liste}".to_string(),
        TyKind::Table => "\\mathrm{table}".to_string(),
        TyKind::Tableau => "\\mathrm{tableau}".to_string(),
        TyKind::Tampon => "\\mathrm{tampon}".to_string(),
        TyKind::Vide => "\\mathrm{vide}".to_string(),
        TyKind::Dyn => "?".to_string(),
    };
    match t.elt {
        TyKind::Vide | TyKind::Dyn => base,
        _ => format!("\\mathrm{{{}}}(\\mathrm{{{}}})", "liste", base),
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::front::TyKind;

    // ── Fabriques d'expressions ──────────────────────────────────────────

    fn lit(texte: &str) -> Expr {
        Expr::Litteral { texte: texte.into(), flott: false, rationnel: false, neg: false, ty: TyKind::Dyn, ligne: 1 }
    }
    fn lit_flott(texte: &str) -> Expr {
        Expr::Litteral { texte: texte.into(), flott: true, rationnel: false, neg: false, ty: TyKind::Flottant, ligne: 1 }
    }
    fn lit_rationnel(num: &str, den: &str) -> Expr {
        Expr::Litteral {
            texte: format!("{}/{}", num, den),
            flott: false,
            rationnel: true,
            neg: false,
            ty: TyKind::Rationnel,
            ligne: 1,
        }
    }
    fn var(nom: &str) -> Expr { Expr::Var(nom.into(), 1) }
    fn bin(op: &str, a: Expr, b: Expr) -> Expr {
        Expr::Binaire { op: op.into(), a: Box::new(a), b: Box::new(b), ligne: 1 }
    }
    fn una(op: &str, a: Expr) -> Expr {
        Expr::Unaire { op: op.into(), a: Box::new(a), ligne: 1 }
    }
    fn app(nom: &str, args: Vec<Expr>) -> Expr {
        Expr::Appel { nom: nom.into(), module: String::new(), args, ligne: 1 }
    }

    // ── Littéraux ────────────────────────────────────────────────────────

    #[test]
    fn litteral_entier() {
        assert_eq!(expr_latex(&lit("42")), "42");
    }

    #[test]
    fn litteral_flottant() {
        assert_eq!(expr_latex(&lit_flott("2.5")), "2.5");
    }

    #[test]
    fn litteral_rationnel() {
        assert_eq!(expr_latex(&lit_rationnel("3", "4")), "\\frac{3}{4}");
    }

    #[test]
    fn litteral_negatif() {
        let e = Expr::Litteral { texte: "7".into(), flott: false, rationnel: false, neg: true, ty: TyKind::Entier, ligne: 1 };
        assert_eq!(expr_latex(&e), "-7");
    }

    #[test]
    fn constantes_mathematiques() {
        assert_eq!(expr_latex(&var("z")), "z");
        assert_eq!(expr_latex(&var("pi")), "\\pi");
        assert_eq!(expr_latex(&var("phi")), "\\varphi");
        assert_eq!(expr_latex(&var("e")), "\\mathrm{e}");
    }

    #[test]
    fn complexe() {
        let e = Expr::LitComplexe { re: 3.0, im: 4.0, ligne: 1 };
        assert_eq!(expr_latex(&e), "3 + 4\\mathrm{i}");
        let pur = Expr::LitComplexe { re: 0.0, im: 1.0, ligne: 1 };
        assert_eq!(expr_latex(&pur), "\\mathrm{i}");
        let reelle = Expr::LitComplexe { re: 2.0, im: 0.0, ligne: 1 };
        assert_eq!(expr_latex(&reelle), "2");
    }

    // ── Opérations ───────────────────────────────────────────────────────

    #[test]
    fn somme_et_produit() {
        assert_eq!(expr_latex(&bin("+", lit("1"), lit("2"))), "1 + 2");
        assert_eq!(expr_latex(&bin("*", var("a"), var("b"))), "a \\cdot b");
        assert_eq!(expr_latex(&bin("-", var("a"), var("b"))), "a - b");
    }

    #[test]
    fn division_et_modulo() {
        assert_eq!(expr_latex(&bin("/", lit("1"), lit("2"))), "\\frac{1}{2}");
        assert_eq!(expr_latex(&bin("%", lit("7"), lit("2"))), "7 \\bmod 2");
    }

    #[test]
    fn puissance() {
        assert_eq!(expr_latex(&bin("^", var("x"), lit("2"))), "x^{2}");
        // associativité à droite : a^(b^c)
        assert_eq!(
            expr_latex(&bin("^", var("a"), bin("^", var("b"), var("c")))),
            "a^{b^{c}}"
        );
    }

    #[test]
    fn parentheses_seulement_si_necessaire() {
        // pas de parenthèses redondantes
        assert_eq!(expr_latex(&bin("+", lit("1"), lit("2"))), "1 + 2");
        assert_eq!(expr_latex(&bin("*", var("a"), var("b"))), "a \\cdot b");
        // parenthèses autour d'une sous-expression moins prioritaire
        assert_eq!(
            expr_latex(&bin("*", bin("+", var("a"), var("b")), var("c"))),
            "(a + b) \\cdot c"
        );
        assert_eq!(
            expr_latex(&bin("+", bin("*", var("a"), var("b")), var("c"))),
            "a \\cdot b + c"
        );
        // soustraction : le membre de droite doit être parenthésé
        assert_eq!(
            expr_latex(&bin("-", var("a"), bin("-", var("b"), var("c")))),
            "a - (b - c)"
        );
    }

    #[test]
    fn unaire_et_factorielle() {
        assert_eq!(expr_latex(&una("-", var("a"))), "-a");
        assert_eq!(expr_latex(&una("-", bin("+", var("a"), var("b")))), "-(a + b)");
        assert_eq!(expr_latex(&una("non", var("p"))), "\\neg p");
        assert_eq!(
            expr_latex(&Expr::Factorielle(Box::new(lit("5")), 1)),
            "5!"
        );
    }

    #[test]
    fn operateurs_logiques_et_comparaisons() {
        assert_eq!(
            expr_latex(&bin("<", var("x"), lit("5"))),
            "x < 5"
        );
        assert_eq!(
            expr_latex(&bin("<=", var("x"), lit("5"))),
            "x \\leq 5"
        );
        assert_eq!(
            expr_latex(&bin("!=", var("x"), lit("5"))),
            "x \\neq 5"
        );
        // « et » est associatif à gauche : pas de parenthèses redondantes
        let e = bin("et", bin("et", lit("1"), lit("2")), lit("3"));
        assert_eq!(expr_latex(&e), "1 \\land 2 \\land 3");
        // « ou » : l'opérande de droite, de même priorité, est parenthésé
        let o = bin("ou", lit("1"), bin("ou", lit("2"), lit("3")));
        assert_eq!(expr_latex(&o), "1 \\lor (2 \\lor 3)");
    }

    #[test]
    fn puissance_associative_a_droite() {
        // a^(b^c) : pas de parenthèses
        assert_eq!(
            expr_latex(&bin("^", var("a"), bin("^", var("b"), var("c")))),
            "a^{b^{c}}"
        );
        // (a^b)^c : parenthèses obligatoires
        assert_eq!(
            expr_latex(&bin("^", bin("^", var("a"), var("b")), var("c"))),
            "(a^{b})^{c}"
        );
        // (-a)^b : le moins unaire doit rester groupé
        assert_eq!(
            expr_latex(&bin("^", una("-", var("a")), var("b"))),
            "(-a)^{b}"
        );
    }

    #[test]
    fn appels_mathematiques() {
        assert_eq!(expr_latex(&app("sin", vec![var("x")])), "\\sin(x)");
        assert_eq!(expr_latex(&app("racine", vec![lit("2")])), "\\sqrt{2}");
        assert_eq!(expr_latex(&app("sqrt", vec![lit("2")])), "\\sqrt{2}");
        assert_eq!(expr_latex(&app("modulus", vec![var("z")])), "\\left|z\\right|");
        assert_eq!(expr_latex(&app("re", vec![var("z")])), "\\Re{z}");
        assert_eq!(expr_latex(&app("im", vec![var("z")])), "\\Im{z}");
        assert_eq!(expr_latex(&app("conj", vec![var("z")])), "\\overline{z}");
        assert_eq!(expr_latex(&app("i", vec![])), "\\mathrm{i}");
        // fonction inconnue : `\operatorname`
        assert_eq!(
            expr_latex(&app("ma_fonction", vec![lit("1"), lit("2")])),
            "\\operatorname{ma_fonction}(1, 2)"
        );
    }

    // ── Instructions ─────────────────────────────────────────────────────

    #[test]
    fn afficher_et_affectation() {
        assert_eq!(
            instr_latex(&Instr::Afficher(vec![var("x")], 1)),
            "\\mathrm{afficher}\\left(x\\right)"
        );
        assert_eq!(
            instr_latex(&Instr::Affect { nom: "x".into(), e: lit("42"), ligne: 1 }),
            "x = 42"
        );
    }

    #[test]
    fn instruction_si() {
        let i = Instr::Si {
            test: bin("<", var("x"), lit("5")),
            alors: vec![Instr::Affect { nom: "x".into(), e: lit("1"), ligne: 1 }],
            sinon: None,
            ligne: 1,
        };
        assert_eq!(
            instr_latex(&i),
            "\\mathbf{si}\\ \\langle x < 5\\rangle\\ \\mathbf{alors}\\ \\langle x = 1\\rangle"
        );
    }

    #[test]
    fn instruction_tant_que() {
        let i = Instr::TantQue {
            test: bin("<", var("x"), lit("10")),
            corps: vec![Instr::Affect {
                nom: "x".into(),
                e: bin("+", var("x"), lit("1")),
                ligne: 1,
            }],
            ligne: 1,
        };
        assert_eq!(
            instr_latex(&i),
            "\\mathbf{tant que}\\ \\langle x < 10\\rangle\\ \\mathbf{faire}\\ \\langle x = x + 1\\rangle"
        );
    }

    #[test]
    fn instruction_pour_forme_somme() {
        let i = Instr::Pour {
            nom: "j".into(),
            debut: lit("0"),
            fin: lit("10"),
            corps: vec![Instr::Affect {
                nom: "s".into(),
                e: bin("+", var("s"), var("j")),
                ligne: 1,
            }],
            ligne: 1,
        };
        assert_eq!(
            instr_latex(&i),
            "\\sum_{j = 0}^{9} \\langle s = s + j\\rangle"
        );
    }

    #[test]
    fn instruction_pour_borne_non_litterale() {
        let i = Instr::Pour {
            nom: "i".into(),
            debut: lit("0"),
            fin: var("n"),
            corps: vec![Instr::ExprStmt(var("i"))],
            ligne: 1,
        };
        assert_eq!(
            instr_latex(&i),
            "\\sum_{i = 0}^{n - 1} \\langle i\\rangle"
        );
    }

    #[test]
    fn bloc_et_retour() {
        let b = Instr::Bloc(vec![
            Instr::Affect { nom: "a".into(), e: lit("1"), ligne: 1 },
            Instr::Affect { nom: "b".into(), e: lit("2"), ligne: 1 },
        ]);
        assert_eq!(instr_latex(&b), "a = 1; b = 2");
        assert_eq!(
            instr_latex(&Instr::Retour(Some(lit("0")), 1)),
            "\\mathbf{retour}\\ \\langle 0\\rangle"
        );
    }

    #[test]
    fn programme_complet() {
        let mut p = Programme::default();
        p.instructions.push(Instr::Affect { nom: "x".into(), e: lit("1"), ligne: 1 });
        p.instructions.push(Instr::Affect { nom: "y".into(), e: lit("2"), ligne: 2 });
        assert_eq!(programme_latex(&p), "x = 1 \\\\\ny = 2");
    }

    #[test]
    fn instructions_secondaires() {
        // index, lecture, levée : rendus non nuls et lisibles
        assert_eq!(
            instr_latex(&Instr::AffectIndex {
                nom: "t".into(),
                index: lit("0"),
                valeur: lit("7"),
                ligne: 1,
            }),
            "t_{0} = 7"
        );
        assert_eq!(
            instr_latex(&Instr::Lire { nom: "n".into(), ty: TyKind::Entier, ligne: 1 }),
            "\\mathrm{lire}\\ \\langle n\\rangle"
        );
        assert_eq!(
            instr_latex(&Instr::Lever(lit("1"), 1)),
            "\\mathrm{lever}\\ \\langle 1\\rangle"
        );
        // expression seule
        assert_eq!(instr_latex(&Instr::ExprStmt(var("x"))), "x");
    }

    #[test]
    fn listes_et_tables() {
        let l = Expr::ListeLit(vec![lit("1"), lit("2")], 1);
        assert_eq!(expr_latex(&l), "\\left[1, 2 \\right]");
        let t = Expr::TableLit(vec![("a".to_string(), lit("1"))], 1);
        assert_eq!(expr_latex(&t), "\\left\\{\\mathtt{a} = 1 \\right\\}");
    }
}
