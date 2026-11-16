// ═══════════════════════════════════════════════════════════════════════
// `rose latex --apercu` — LA RENDU VISUEL D'UN PROGRAMME
// ═══════════════════════════════════════════════════════════════════════
//
// ⚠⚠⚠ POURQUOI CET OUTIL. L'utilisateur a demandé « un programme qui contient
// du latex pour voir dans ton output ce que ça donne », et il croyait que
// `$x$` suffisait pour marquer une ligne. Ce n'est PAS le cas : le marqueur
// est `%%` EN DÉBUT DE LIGNE (voir `latex::MARQUEUR_LIGNE`), et une ligne
// `$x$` seule n'est PAS du Rose valide (elle serait lue comme une variable).
//
// ⚠⚠ Ce module rend donc un APERÇU VISIBLE — pas du LaTeX brut. Il ne dépend
// d'AUCUNE bibliothèque : il traduit les commandes LaTeX qu'émet le module
// `latex` en caractères Unicode. C'est ce qui permet de VÉRIFIER le rendu
// dans un terminal, sans LaTeX installé.
//
// ⚠⚠ Le rendu est VOLONTAIREMENT simple (pas un moteur TeX) : il couvre les
// commandes que Rose émet réellement (`\frac`, `\sqrt`, `\operatorname`,
// `\mathrm`, `\sum`, `\langle`, `\mathbf`…). Toute commande inconnue est
// conservée telle quelle entre `« »` pour qu'on la VOIE au lieu de la perdre.

/// Rend une chaîne LaTeX en texte Unicode lisible.
///
/// Exemples :
///   `\frac{1}{3}`            → `1⁄3` (numérateur sur dénominateur)
///   `\sqrt{2}`               → `√2`
///   `\operatorname{f}(n)`    → `f(n)`
///   `\sum_{k = 1}^{99} …`    → `Σ[k=1…99] …`
///   `\langle … \rangle`      → `⟨ … ⟩`
pub fn rendre(latex: &str) -> String {
    let cps: Vec<char> = latex.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    // ⚠ Mode MATHÉMATIQUES : `$…$` est la notation du contenu, pas une
    // commande. On RETIRE les délimiteurs (ils ne sont pas du texte) et on
    // rend le contenu. C'est ce que l'utilisateur attendait en écrivant `$x$`.
    let mut maths = false;
    while i < cps.len() {
        let c = cps[i];
        if c == '$' {
            maths = !maths;
            i += 1;
            continue;
        }
        if c != '\\' {
            // ⚠ `^` et `_` : exposant et indice, rendus en exposant Unicode
            // quand c'est possible (chiffres et lettres simples).
            if (c == '^' || c == '_') && i + 1 < cps.len() {
                let (groupe, suivant) = lire_groupe(&cps, i + 1);
                let texte = rendre(&groupe);
                out.push_str(&exposant_ou_indice(&texte, c == '^'));
                i = suivant;
                continue;
            }
            out.push(c);
            i += 1;
            continue;
        }
        // Commande LaTeX : on lit le nom.
        let mut j = i + 1;
        let mut nom = String::new();
        while j < cps.len() && cps[j].is_ascii_alphabetic() {
            nom.push(cps[j]);
            j += 1;
        }
        if nom.is_empty() {
            // `\\` (saut de ligne), `\{`, `\%`, `\$`, `\_`…
            if j < cps.len() {
                match cps[j] {
                    '\\' => out.push('\n'),
                    '{' | '}' | '%' | '$' | '_' | '&' | '#' => out.push(cps[j]),
                    ' ' => out.push(' '),
                    autre => { out.push('«'); out.push('\\'); out.push(autre); out.push('»'); }
                }
                i = j + 1;
            } else {
                i = j;
            }
            continue;
        }
        i = j;
        match nom.as_str() {
            "frac" => {
                let (num, k1) = lire_groupe(&cps, i);
                let (den, k2) = lire_groupe(&cps, k1);
                out.push_str(&format!("{}/{}", rendre(&num), rendre(&den)));
                i = k2;
            }
            "sqrt" => {
                let (arg, k1) = lire_groupe(&cps, i);
                out.push('√');
                let r = rendre(&arg);
                // ⚠ Parenthèses si l'argument est composé (√(1 + x)).
                if r.chars().count() > 1 && !r.chars().all(|c| c.is_alphanumeric()) {
                    out.push('(');
                    out.push_str(&r);
                    out.push(')');
                } else {
                    out.push_str(&r);
                }
                i = k1;
            }
            "operatorname" | "mathrm" | "mathit" | "mathbf" | "mathtt" | "text" => {
                let (arg, k1) = lire_groupe(&cps, i);
                out.push_str(&rendre(&arg));
                i = k1;
            }
            "sum" => {
                out.push('Σ');
                // Indice / exposant éventuels : `\sum_{k=0}^{\infty}`.
                if i < cps.len() && cps[i] == '_' {
                    let (g, k1) = lire_groupe(&cps, i + 1);
                    out.push_str(&format!("[{}", rendre(&g)));
                    i = k1;
                    if i < cps.len() && cps[i] == '^' {
                        let (g2, k2) = lire_groupe(&cps, i + 1);
                        out.push_str(&format!("…{}]", rendre(&g2)));
                        i = k2;
                    } else {
                        out.push(']');
                    }
                }
            }
            "langle" => out.push('⟨'),
            "rangle" => out.push('⟩'),
            "le" | "leq" => out.push('≤'),
            "ge" | "geq" => out.push('≥'),
            "ne" | "neq" => out.push('≠'),
            "cdot" => out.push('·'),
            "times" => out.push('×'),
            "div" => out.push('÷'),
            "pm" => out.push('±'),
            "infty" => out.push('∞'),
            "pi" => out.push('π'),
            "varphi" | "phi" => out.push('φ'),
            "approx" => out.push('≈'),
            "bmod" => out.push_str(" mod "),
            "left" | "right" | "big" | "Big" => {
                // `\left( … \right)` : le délimiteur suit la commande.
                if i < cps.len() {
                    let d = cps[i];
                    if d != '.' {
                        out.push(match d { '(' => '(', ')' => ')', '[' => '[', ']' => ']',
                                           '|' => '|', _ => d });
                    }
                    i += 1;
                }
            }
            "sin" | "cos" | "tan" | "ln" | "log" | "exp" | "arctan" | "arcsin"
            | "arccos" | "Re" | "Im" => {
                out.push_str(&nom.to_lowercase());
            }
            "overline" => {
                let (arg, k1) = lire_groupe(&cps, i);
                out.push_str(&rendre(&arg));
                out.push('\u{0305}'); // barre combinante (conjugué)
                i = k1;
            }
            "textbackslash" => out.push('\\'),
            "textasciicircum" => out.push('^'),
            "textasciitilde" => out.push('~'),
            "quad" | "qquad" | "," | ";" | ":" | "!" => out.push(' '),
            autre => {
                // ⚠ Commande inconnue : on l'AFFICHE, on ne la perd pas.
                out.push('«');
                out.push('\\');
                out.push_str(autre);
                out.push('»');
            }
        }
    }
    out
}

/// Lit un groupe `{…}` (ou un seul caractère si ce n'est pas un groupe).
/// Rend (contenu, index après le groupe).
fn lire_groupe(cps: &[char], debut: usize) -> (String, usize) {
    if debut >= cps.len() { return (String::new(), debut); }
    if cps[debut] != '{' { return (cps[debut].to_string(), debut + 1); }
    let mut profondeur = 0i32;
    let mut i = debut;
    let mut contenu = String::new();
    while i < cps.len() {
        match cps[i] {
            '{' => {
                profondeur += 1;
                if profondeur > 1 { contenu.push('{'); }
            }
            '}' => {
                profondeur -= 1;
                if profondeur == 0 { return (contenu, i + 1); }
                contenu.push('}');
            }
            c => contenu.push(c),
        }
        i += 1;
    }
    (contenu, cps.len())
}

/// Rend un exposant ou un indice : en chiffres Unicode quand c'est possible,
/// sinon en `^(…)` / `_(…)`.
fn exposant_ou_indice(texte: &str, exposant: bool) -> String {
    let simple = texte.chars().all(|c| c.is_ascii_digit() || c == '-');
    if simple {
        let mut out = String::new();
        for c in texte.chars() {
            let u = if exposant {
                match c {
                    '0' => '⁰', '1' => '¹', '2' => '²', '3' => '³', '4' => '⁴',
                    '5' => '⁵', '6' => '⁶', '7' => '⁷', '8' => '⁸', '9' => '⁹',
                    '-' => '⁻', autre => autre,
                }
            } else {
                match c {
                    '0' => '₀', '1' => '₁', '2' => '₂', '3' => '₃', '4' => '₄',
                    '5' => '₅', '6' => '₆', '7' => '₇', '8' => '₈', '9' => '₉',
                    '-' => '₋', autre => autre,
                }
            };
            out.push(u);
        }
        return out;
    }
    format!("{}{}{}", if exposant { "^(" } else { "_(" }, texte, ")")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractions_et_racines() {
        assert_eq!(rendre("\\frac{1}{3}"), "1/3");
        assert_eq!(rendre("\\frac{1 + \\sqrt{5}}{2}"), "1 + √5/2");
        assert_eq!(rendre("\\sqrt{2}"), "√2");
        assert_eq!(rendre("\\sqrt{1 + x}"), "√(1 + x)");
    }

    #[test]
    fn operateurs_et_symboles() {
        assert_eq!(rendre("\\operatorname{afficher}(n!)"), "afficher(n!)");
        assert_eq!(rendre("\\varphi"), "φ");
        assert_eq!(rendre("\\pi"), "π");
        assert_eq!(rendre("a \\leq b \\neq c"), "a ≤ b ≠ c");
        assert_eq!(rendre("\\left[1, 2\\right]"), "[1, 2]");
        assert_eq!(rendre("\\mathrm{vrai}"), "vrai");
    }

    #[test]
    fn puissances_et_sommes() {
        assert_eq!(rendre("2^{100}"), "2¹⁰⁰");
        assert_eq!(rendre("n^{a + b}"), "n^(a + b)");
        assert_eq!(rendre("\\sum_{k = 1}^{99} k"), "Σ[k = 1…99] k");
        assert_eq!(rendre("\\langle x = 1\\rangle"), "⟨ x = 1⟩");
    }

    #[test]
    fn commande_inconnue_est_visible() {
        // ⚠ On ne perd RIEN : une commande non gérée est affichée.
        assert!(rendre("\\inconnue{x}").contains("«\\inconnue»"));
    }

    #[test]
    fn delimiteurs_maths_retires() {
        // ⚠ `$x$` n'est pas une commande : ce sont les délimiteurs des
        // mathématiques. On rend le CONTENU.
        assert_eq!(rendre("$x^2 + y^2$"), "x² + y²");
        assert_eq!(rendre("$\\frac{1}{3}$"), "1/3");
        assert_eq!(rendre("prix $5 et 6$ unités"), "prix 5 et 6 unités");
    }

    #[test]
    fn saut_de_ligne() {
        assert_eq!(rendre("a \\\\ b"), "a \n b");
    }
}
