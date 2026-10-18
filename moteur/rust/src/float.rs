// ═══════════ Rose — moteur Rust : flottants fidèles (équivalent snprintf %g) ═══════════

/// Rendu "%g" de précision p (chiffres significatifs), identique à glibc.
fn g_prec(p: usize, d: f64) -> String {
    if !d.is_finite() {
        if d.is_nan() { return "nan".into(); }
        return if d > 0.0 { "inf".into() } else { "-inf".into() };
    }
    let brut = format!("{:.*e}", p.saturating_sub(1), d);
    let (mant, expo) = brut.split_once('e').unwrap();
    let negatif = mant.starts_with('-');
    let mut chiffres: String = mant.trim_start_matches('-').chars().filter(|c| *c != '.').collect();
    let x: i32 = expo.parse().unwrap_or(0);
    // %g supprime les zéros de fin du mantisse
    while chiffres.len() > 1 && chiffres.ends_with('0') {
        chiffres.pop();
    }
    let d = chiffres.as_str();
    let signe = if negatif { "-" } else { "" };
    if x < -4 || x >= p as i32 {
        let m = if d.len() > 1 { format!("{}.{}", &d[..1], &d[1..]) } else { d.to_string() };
        let ax = x.unsigned_abs();
        format!("{}{}e{}{:02}", signe, m, if x < 0 { "-" } else { "+" }, ax)
    } else if x >= 0 {
        let v: Vec<char> = d.chars().collect();
        let n = (x + 1) as usize;
        let mut entier: String = v[..n.min(v.len())].iter().collect();
        while entier.len() < n { entier.push('0'); }
        if n < v.len() {
            format!("{}{}.{}", signe, entier, v[n..].iter().collect::<String>())
        } else {
            format!("{}{}", signe, entier)
        }
    } else {
        let zeros = "0".repeat((-x - 1) as usize);
        format!("{}0.{}{}", signe, zeros, d)
    }
}

fn strtod_fidele(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok()
}

/// Texte le plus court qui fait l'aller-retour, comme snprintf("%.*g") p=1..17,
/// avec ".0" ajouté si aucune marque décimale n'apparaît (comportement du moteur C++).
pub fn texte_flottant(d: f64) -> String {
    if !d.is_finite() {
        return if d.is_nan() { "nan".into() }
               else if d > 0.0 { "inf".into() } else { "-inf".into() };
    }
    for p in 1..=17usize {
        let s = g_prec(p, d);
        if strtod_fidele(&s) == Some(d) {
            if !s.contains('.') && !s.contains('e') && !s.contains("inf") && !s.contains("nan") {
                return s + ".0";
            }
            return s;
        }
    }
    g_prec(17, d)
}

/// Parseur type strtod : préfixe numérique valide le plus long ; inf/nan
/// acceptés comme en C ; « infini » (sans signe) accepté mot à mot.
pub fn lire_flottant_strtod(ch: &str) -> Option<f64> {
    let t = ch.trim_start();
    let (sign, rest) = if let Some(r) = t.strip_prefix('-') {
        (-1.0f64, &t[1..])
    } else if let Some(r) = t.strip_prefix('+') {
        (1.0f64, &t[1..])
    } else {
        (1.0f64, t)
    };
    let rr = rest.to_ascii_lowercase();
    if rr.starts_with("inf") { return Some(sign * f64::INFINITY); }
    if rr.starts_with("nan") { return Some(f64::NAN); }
    let cs: Vec<char> = rest.chars().collect();
    let mut i = 0usize;
    while i < cs.len() && cs[i].is_ascii_digit() { i += 1; }
    let mut av_point = i > 0;
    if i < cs.len() && cs[i] == '.' {
        i += 1;
        let a = i;
        while i < cs.len() && cs[i].is_ascii_digit() { i += 1; }
        if i > a { av_point = true; }
    }
    if !av_point {
        if t == "infini" { return Some(f64::INFINITY); }
        return None;
    }
    if i < cs.len() && (cs[i] == 'e' || cs[i] == 'E') {
        let save = i;
        i += 1;
        if i < cs.len() && (cs[i] == '+' || cs[i] == '-') { i += 1; }
        let a = i;
        while i < cs.len() && cs[i].is_ascii_digit() { i += 1; }
        if i == a { i = save; }
    }
    let decal = t.len() - rest.len();
    t[..decal + i].parse::<f64>().ok()
}

/// Préfixe numérique pour la lecture flottant de rose (strtod : s'arrête au
/// premier caractère invalide au lieu d'échouer).
pub fn prefis_flottant(ch: &str) -> Option<f64> {
    lire_flottant_strtod(ch)
}
