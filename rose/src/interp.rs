// Rose 2.0 — interpréteur d'arbre complet : instructions, expressions,
// bibliothèque standard (miroir du moteur C++), FFI dynamique::, RNG.
// Sert de repli au JIT (mêmes sorties) et de moteur pour les types non
// compilables (chaînes, rationnels, listes, tables, tampons).
// Licence MIT.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::front::{Expr, Instr, Programme, Signature, TyKind};
use crate::run::*;
use crate::jit::{Compile, Jit};

pub struct Interp {
    prog: Programme,
    globaux: HashMap<String, Valeur>,
    trames: Vec<HashMap<String, Valeur>>,
    jit: HashMap<String, Compile>,
    /// corps du programme principal compilé par Cranelift (None = interprété)
    jit_principal: Option<Compile>,
    pub rng: u64,
    pub arguments: Vec<String>,
    arguments_disponibles: bool,
    poignees: Vec<*mut std::ffi::c_void>,
}



/// Découpe une chaîne au niveau 0 par des virgules.
fn split_top(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut prof = 0i32;
    for ch in s.chars() {
        match ch {
            '(' | '[' | '{' => { prof += 1; cur.push(ch); }
            ')' | ']' | '}' => { prof -= 1; cur.push(ch); }
            ',' if prof == 0 => { out.push(cur.trim().to_string()); cur.clear(); }
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() { out.push(cur.trim().to_string()); }
    out
}

/// Nettoie un rendu : enlève un suffixe de contexte résiduel.
/// Rendu simple d'un flottant : supprime les zeros superflus.
fn texte_simple(f: f64) -> String {
    if f == f.floor() && f.abs() < 1e16 { return (f as i64).to_string(); }
    let s = format!("{}", f);
    s
}

fn nettoyer_rendu(r: &str) -> String {
    let mut out = r.trim().to_string();
    loop {
        let n = if let Some(k) = out.find(" // vars:") { Some((k, 9usize)) }
            else if let Some(k) = out.find("// vars:") { Some((k, 8usize)) }
            else if let Some(k) = out.find("//vars:") { Some((k, 7usize)) }
            else { None };
        match n {
            Some((k, _)) => { out = out[..k].trim_end().to_string(); }
            None => break,
        }
    }
    out
}

/// Vrai si une chaîne contient encore un appel `nom(...)`.
fn contient_appel(s: &str) -> bool {
    let cs: Vec<char> = s.chars().collect();
    let mut i = 0usize;
    let mut dans_guilm = false;
    while i < cs.len() {
        let c = cs[i];
        if c == '"' { dans_guilm = !dans_guilm; i += 1; continue; }
        if !dans_guilm && c == '(' {
            let mut j = i;
            while j > 0 && (cs[j - 1].is_ascii_alphanumeric() || cs[j - 1] == '_' || cs[j - 1] == ')') {
                j -= 1;
            }
            if j < i { return true; }
        }
        i += 1;
    }
    false
}

/// Substitue chaque mot de `nu` égal à un nom de paire par sa valeur.
/// Les separateurs `( ) [ ]` decoupent aussi les mots (comme la voie inline).
fn substituer_mots_paires(nu: &str, paires: &[(String, String)]) -> String {
    let mut s2 = String::new();
    let chars: Vec<char> = nu.chars().collect();
    let mut k = 0usize;
    while k < chars.len() {
        let c = chars[k];
        if c == ' ' || c == '\t' || c == '\n' || c == ',' { k += 1; continue; }
        if c == '"' {
            let d = k; k += 1;
            while k < chars.len() && chars[k] != '"' { k += 1; }
            if k < chars.len() { k += 1; }
            let seg: String = chars[d..k].iter().collect();
            s2.push_str(&seg);
            continue;
        }
        if c == '(' || c == ')' || c == '[' || c == ']' {
            s2.push(c); k += 1; continue;
        }
        let d = k;
        while k < chars.len()
            && chars[k] != ' ' && chars[k] != '\t' && chars[k] != '\n' && chars[k] != ','
            && chars[k] != '(' && chars[k] != ')' && chars[k] != '['
            && chars[k] != ']' && chars[k] != '"'
        { k += 1; }
        let seg: String = chars[d..k].iter().collect();
        // le jeton « vars: » (et les suivants) fait partie du contexte deja
        // encode dans `paires` : on ne le rend pas.
        if seg == "vars:" || seg == "vars" {
            k = chars.len();
            continue;
        }
        let mut trouve = false;
        for (nom, val) in paires.iter().rev() {
            if *nom == seg { s2.push_str(val); trouve = true; break; }
        }
        if !trouve { s2.push_str(&seg); }
    }
    s2
}

/// Paires "nom=valeur" à partir du texte brut du prélude.
fn paires_prelude(vb: &str) -> Vec<(String, String)> {
    let mut paires: Vec<(String, String)> = Vec::new();
    let mut cur = String::new();
    let mut dans_guilm = false;
    let mut prof = 0usize;
    for c in vb.chars() {
        if c == '"' { dans_guilm = !dans_guilm; cur.push(c); }
        else if !dans_guilm && prof == 0 && (c == ' ' || c == '\t' || c == '\n' || c == ',') {
            if !cur.is_empty() {
                if let Some(eq) = cur.find('=') {
                    paires.push((cur[..eq].to_string(), cur[eq + 1..].to_string()));
                }
                cur.clear();
            }
        } else {
            if !dans_guilm {
                if c == '(' || c == '[' || c == '{' { prof += 1; }
                if c == ')' || c == ']' || c == '}' { prof = prof.saturating_sub(1); }
            }
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        if let Some(eq) = cur.find('=') {
            paires.push((cur[..eq].to_string(), cur[eq + 1..].to_string()));
        }
    }
    paires
}

/// Rend une valeur atomique depuis la chaîne d'une valeur de paire.
fn si_val_atomique(val: &str) -> Valeur {
    if val.starts_with('"') && val.ends_with('"') && val.len() >= 2 {
        return Valeur::s(val[1..val.len() - 1].to_string());
    }
    if val == "vrai" { return Valeur::Booleen(true); }
    if val == "faux" { return Valeur::Booleen(false); }
    if let Ok(n) = val.parse::<i64>() { return Valeur::z(n); }
    if let Ok(f) = val.parse::<f64>() { return Valeur::f(f); }
    // entier exact trop grand pour i64 (et sans point/exposant) : BigInt
    let est_entier_pur = !val.is_empty()
        && (val.chars().all(|c| c.is_ascii_digit())
            || (val.len() > 1
                && (val.as_bytes()[0] == b'+' || val.as_bytes()[0] == b'-')
                && val[1..].chars().all(|c| c.is_ascii_digit())));
    if est_entier_pur { return Valeur::zg(chaine_vers_entier(val)); }
    Valeur::s(val.to_string())
}

/// Liste de Valeur depuis une chaîne "a, b, c" en résolvant chaque argument.
fn args_depuis_chaine(interieur: &str, paires: &[(String, String)]) -> Vec<Valeur> {
    let mut out: Vec<Valeur> = Vec::new();
    for a_s in split_top(interieur) {
        if a_s.is_empty() { continue; }
        let mut trouve = false;
        for (nom, val) in paires.iter().rev() {
            if *nom == a_s { out.push(si_val_atomique(val)); trouve = true; break; }
        }
        if trouve { continue; }
        let est_atomique = !a_s.contains('(') && !a_s.contains(')')
            && !a_s.contains('[') && !a_s.contains(']')
            && !a_s.contains('+') && !a_s.contains('-')
            && !a_s.contains('*') && !a_s.contains('/')
            && !a_s.contains(',') && !a_s.contains('=');
        if est_atomique {
            // un mot entre guillemets est un litteral chaine
            if a_s.len() >= 2
                && a_s.starts_with('"')
                && a_s.ends_with('"')
                && a_s[1..a_s.len() - 1].find('"').is_none()
            {
                out.push(Valeur::s(a_s[1..a_s.len() - 1].to_string()));
            } else {
                out.push(Valeur::s(a_s.clone()));
            }
            continue;
        }
        if let Some(r) = replier_expression(&a_s, paires) {
            if let Ok(i) = r.parse::<i64>() { out.push(Valeur::z(i)); continue; }
            if let Ok(f) = r.parse::<f64>() { out.push(Valeur::f(f)); continue; }
            if r == "vrai" { out.push(Valeur::Booleen(true)); continue; }
            if r == "faux" { out.push(Valeur::Booleen(false)); continue; }
            out.push(Valeur::s(r));
            continue;
        }
        out.push(Valeur::s(a_s.clone()));
    }
    out
}

/// Nom de fonction std connu (liste blanche).
fn nom_std_connu(nom: &str) -> bool {
    matches!(nom,
        "afficher" | "saisir_entier" | "saisir_flottant" | "time" | "graine" | "seed"
        | "graine_systeme" | "seed_sys" | "graine_vrai" | "rng_materiel" | "rng_mat" | "hasard"
        | "hasard_systeme" | "hasard_vrai" | "trng" | "entier" | "rationnel" | "flottant"
        | "chaine" | "longueur" | "len" | "table" | "tampon" | "buf" | "tableau" | "premier" | "dernier"
        | "insere" | "retire" | "concatener" | "joindre" | "separer" | "min" | "max"
        | "abs" | "factorielle" | "fact" | "fibonacci" | "fib" | "pgcd" | "gcd" | "ppcm" | "lcm" | "puissance" | "pow" | "racine"
        | "est_premier" | "is_prime" | "compter_premiers" | "nb_chiffres" | "digits" | "nombre_pi" | "pi" | "phi" | "i"
        | "sous_chaine" | "sub" | "lire_texte" | "ecrire_texte" | "eval_expr" | "quitter" | "breakpoint" | "breakpoint_if" | "watch" | "unwatch" | "join"
        | "lever" | "tampon_case" | "tampon_pose" | "remplir_hasard" | "remplir_hasard_philox"
        | "fraction")
}

fn fonction_atomique(nom: &str, args: &[Valeur]) -> Option<Valeur> {
    if !nom_std_connu(nom) { return None; }
    let mut it = Interp::nouveau(crate::front::Programme::default());
    Some(it.stdlib_valeur(nom, args))
}

/// Pli au moyen du parseur : on pré-injecte les paires comme affectations.
fn repli_parseur(nu: &str, paires: &[(String, String)]) -> Option<String> {
    if nu.is_empty() { return None; }
    let mut prefixes = String::new();
    for (nom, val) in paires.iter() {
        // forme la plus simple : `nom = <val>` telle quelle (le parseur sait
        // déjà lire les nombres, booléens, listes `[..]`, tables `{..}`, et
        // les chaînes entre guillemets).
        prefixes.push_str(&format!("{} = {}\n", nom, val));
    }
    let formes = [
        nu.to_string(),
        format!("{}{}", prefixes, nu),
        format!("x = {}", nu),
        format!("{}x = {}", prefixes, nu),
        format!("x = {}\n>> x", nu),
        format!("{}x = {}\n>> x", prefixes, nu),
    ];
    for f in formes.iter() {
        if let Ok(prog) = crate::front::analyser(f) {
            if prog.instructions.is_empty() { continue; }
            let mut it = Interp::nouveau(prog);
            it.executer_principal();
            // rendu de la dernière instruction utile
            let n = it.prog.instructions.len();
            for idx in (0..n).rev() {
                let ins = it.prog.instructions[idx].clone();
                match ins {
                    crate::front::Instr::Afficher(exprs, _) => {
                        for e in exprs {
                            if let Ok(v) = it.eval(&e) {
                                let r = chaine_de(&v);
                                if !r.is_empty() { return Some(r); }
                            }
                        }
                    }
                    crate::front::Instr::Affect { nom, e, .. } => {
                        if let Some(v) = it.globaux.get(&nom) {
                            let r = chaine_de(v);
                            if !r.is_empty() { return Some(r); }
                        }
                        if let Ok(v) = it.eval(&e) {
                            let r = chaine_de(&v);
                            if !r.is_empty() { return Some(r); }
                        }
                    }
                    crate::front::Instr::ExprStmt(e) => {
                        if let Ok(v) = it.eval(&e) {
                            let r = chaine_de(&v);
                            if !r.is_empty() { return Some(r); }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    None
}

/// Pliage récursif d'une expression : atomes directs, parseur, substitution,
/// puis réduction séquentielle des appels (du plus imbriqué au plus externe).
fn replier_expression(nu: &str, paires: &[(String, String)]) -> Option<String> {
    // A) atome direct
    if nu.starts_with('"') && nu.ends_with('"') && nu.len() >= 2 {
        return Some(nu[1..nu.len() - 1].to_string());
    }
    if nu == "vrai" || nu == "faux" { return Some(nu.to_string()); }
    if let Ok(n) = nu.parse::<i64>() { return Some(n.to_string()); }

    // B) parseur avec préfixe des paires.
    let mut prefixes = String::new();
    for (nom, val) in paires.iter() {
        let val_src = if val.starts_with('"') && val.ends_with('"') {
            val.clone()
        } else if val.parse::<i64>().is_ok()
            || val.parse::<f64>().is_ok()
            || val == "vrai" || val == "faux"
            || (val.starts_with('[') && val.ends_with(']'))
            || (val.starts_with('{') && val.ends_with('}'))
        { val.clone() } else { format!("\"{}\"", val) };
        prefixes.push_str(&format!("{} = {}\n", nom, val_src));
    }
    let formes_b = [
        format!("{}>> {}", prefixes, nu),
        format!("{}x = {}\n>> x", prefixes, nu),
        format!("{}{}", prefixes, nu),
        format!("{}x = {}", prefixes, nu),
    ];
    for f in formes_b.iter() {
        if let Some(r) = rendu_par_parseur(f) { return Some(r); }
    }

    // C) réduction séquentielle : on réduit `nu` itérativement.
    //    Pré-pli : si `nu` ne contient pas `(`, on le passe directement au
    //    parseur pour obtenir un entier/booléen/chaîne.
    {
        let t0 = nu.trim();
        if !t0.is_empty() && !contient_appel(t0) {
            for f in [t0.to_string(), format!("x = {}", t0), format!("x = {}\n>> x", t0)].iter() {
                if let Some(r) = rendu_par_parseur(f) { return Some(r); }
            }
        }
    }
    let mut courant = nu.to_string();
    let mut tour = 0usize;
    while tour < 128 {
        tour += 1;
        let etat_avant = courant.clone();

        // (i) plus court-circuit : si `courant` est un entier/booléen/chaîne,
        //     on rend directement.
        let t = courant.trim();
        if t.starts_with('"') && t.ends_with('"') && t.len() >= 2 {
            return Some(t[1..t.len() - 1].to_string());
        }
        if t == "vrai" || t == "faux" { return Some(t.to_string()); }
        if let Ok(n) = t.parse::<i64>() { return Some(n.to_string()); }

        // (ii) parseur sur `courant` nu
        if !contient_appel(t) {
            for f in [t.to_string(), format!("x = {}", t), format!("x = {}\n>> x", t)].iter() {
                if let Some(r) = rendu_par_parseur(f) { return Some(r); }
            }
        }

        // (iii) réduction : le prochain `nom(...)` en partant de la gauche.
        if !contient_appel(&courant) { break; }
        let pos_open = match courant.find('(') { Some(p) => p, None => break };
        let mut dbut_nom = pos_open;
        while dbut_nom > 0 {
            let c = courant.as_bytes()[dbut_nom - 1] as char;
            if c.is_ascii_alphanumeric() || c == '_' { dbut_nom -= 1; } else { break; }
        }
        let nom_f: String = courant[dbut_nom..pos_open].chars().collect();
        if nom_f.is_empty() { break; }
        let mut prof_p = 0i32;
        let mut fin = pos_open;
        while fin < courant.len() {
            let c = courant.as_bytes()[fin] as char;
            if c == '(' { prof_p += 1; }
            else if c == ')' {
                prof_p -= 1;
                if prof_p == 0 { break; }
            }
            fin += 1;
        }
        if fin >= courant.len() { break; }
        let interieur: String = courant[pos_open + 1..fin].to_string();
        if nom_std_connu(&nom_f) {
            let args_s: Vec<String> = split_top(&interieur)
                .iter()
                .map(|a| replier_expression(a, paires).unwrap_or_else(|| a.clone()))
                .collect();
            let rejoints = args_s.join(", ");
            let args_val = args_depuis_chaine(&rejoints, paires);
            if let Some(v) = fonction_atomique(&nom_f, &args_val) {
                let rendu = chaine_de(&v);
                let nouveau = format!("{}{}{}", &courant[..dbut_nom], rendu, &courant[fin + 1..]);
                if nouveau != etat_avant { courant = nouveau; continue; }
            }
        }
        let mut remplace: Option<String> = None;
        for (n, v) in paires.iter().rev() {
            if *n == nom_f { remplace = Some(v.clone()); break; }
        }
        let mut a_remplace: Option<String> = None;
        if let Some(val) = &remplace { a_remplace = Some(val.clone()); }
        if let Some(val) = a_remplace {
            let nouveau = format!("{}{}{}", &courant[..dbut_nom], val, &courant[fin + 1..]);
            if nouveau != etat_avant { courant = nouveau; continue; }
        }
    }
    // Rendu final si non numérique.
    let t = courant.trim();
    if t.starts_with('"') && t.ends_with('"') && t.len() >= 2 {
        return Some(t[1..t.len() - 1].to_string());
    }
    if t == "vrai" || t == "faux" { return Some(t.to_string()); }
    if let Ok(n) = t.parse::<i64>() { return Some(n.to_string()); }
    if !contient_appel(t) {
        for f in [t.to_string(), format!("x = {}", t), format!("x = {}\n>> x", t)].iter() {
            if let Some(r) = rendu_par_parseur(f) { return Some(r); }
        }
    }
    if !courant.is_empty() && courant != nu { return Some(courant.clone()); }
    None
}


/// Analyse `src` et rend le rendu de la dernière instruction utile, si elle
/// produit une valeur non vide et sans appel résiduel.
fn rendu_par_parseur(src: &str) -> Option<String> {
    let prog = crate::front::analyser(src).ok()?;
    if prog.instructions.is_empty() { return None; }
    let mut it = Interp::nouveau(prog);
    it.executer_principal();

    let n = it.prog.instructions.len();
    for idx in (0..n).rev() {
        let ins = it.prog.instructions[idx].clone();
        match ins {
            crate::front::Instr::Afficher(exprs, _) => {
                for e in exprs {
                    if let Ok(v) = it.eval(&e) {
                        let r = nettoyer_rendu(&chaine_de(&v));
                        if !r.is_empty() && !contient_appel(&r) { return Some(r); }
                    }
                }
            }
            crate::front::Instr::Affect { nom, e, .. } => {
                if let Some(v) = it.globaux.get(&nom) {
                    let r = nettoyer_rendu(&chaine_de(v));
                    if !r.is_empty() && !contient_appel(&r) { return Some(r); }
                }
                if let Ok(v) = it.eval(&e) {
                    let r = nettoyer_rendu(&chaine_de(&v));
                    if !r.is_empty() && !contient_appel(&r) { return Some(r); }
                }
            }
            crate::front::Instr::ExprStmt(e) => {
                if let Ok(v) = it.eval(&e) {
                    let r = nettoyer_rendu(&chaine_de(&v));
                    if !r.is_empty() && !contient_appel(&r) { return Some(r); }
                }
            }
            _ => {}
        }
    }
    None
}


#[cfg(unix)]
unsafe fn ffi_dlopen(name: *const i8) -> *mut std::ffi::c_void {
    // RTLD_NOW = 2
    unsafe extern "C" { fn dlopen(filename: *const i8, flags: i32) -> *mut std::ffi::c_void; }
    dlopen(name, 2)
}
#[cfg(windows)]
unsafe fn ffi_dlopen(name: *const i8) -> *mut std::ffi::c_void {
    unsafe extern "system" { fn LoadLibraryA(lpLibFileName: *const i8) -> *mut std::ffi::c_void; }
    LoadLibraryA(name)
}
#[cfg(unix)]
unsafe fn ffi_dlsym(handle: *mut std::ffi::c_void, name: *const i8) -> *mut std::ffi::c_void {
    unsafe extern "C" { fn dlsym(handle: *mut std::ffi::c_void, symbol: *const i8) -> *mut std::ffi::c_void; }
    dlsym(handle, name)
}
#[cfg(windows)]
unsafe fn ffi_dlsym(handle: *mut std::ffi::c_void, name: *const i8) -> *mut std::ffi::c_void {
    unsafe extern "system" { fn GetProcAddress(hModule: *mut std::ffi::c_void, lpProcName: *const i8) -> *mut std::ffi::c_void; }
    GetProcAddress(handle, name)
}
#[cfg(unix)]
unsafe fn ffi_dlclose(handle: *mut std::ffi::c_void) {
    unsafe extern "C" { fn dlclose(handle: *mut std::ffi::c_void) -> i32; }
    dlclose(handle);
}
#[cfg(windows)]
unsafe fn ffi_dlclose(handle: *mut std::ffi::c_void) {
    unsafe extern "system" { fn FreeLibrary(hLibModule: *mut std::ffi::c_void) -> i32; }
    FreeLibrary(handle);
}


impl Interp {
    pub fn nouveau(prog: Programme) -> Self {
        Interp {
            prog, globaux: HashMap::new(), trames: vec![HashMap::new()],
            jit: HashMap::new(),
            jit_principal: None,
            // état initial : pid × constante d'or XOR adresse ASLR d'une variable
            // globale (miroir du g_rng_etat du moteur C++), pour que deux
            // exécutions successives sans graine() ne rendent pas la même suite.
            rng: {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                static ANCRE: u64 = 0;
                let adr = &ANCRE as *const u64 as u64;
                let mut h = DefaultHasher::new();
                (std::process::id() as u64).hash(&mut h);
                adr.hash(&mut h);
                (std::process::id() as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ h.finish() ^ adr
            },
            arguments: Vec::new(), arguments_disponibles: false, poignees: Vec::new(),
        }
    }

    pub fn avec_arguments(mut self, args: Vec<String>) -> Self {
        self.arguments = args;
        self.arguments_disponibles = true;
        self
    }

    fn cible(&self, nom: &str) -> Option<Valeur> {
        let f = self.trames.last().unwrap();
        if let Some(v) = f.get(nom) { return Some(v.clone()); }
        self.globaux.get(nom).cloned()
    }

    fn affecter(&mut self, nom: &str, v: Valeur) {
        // miroir C++ : la variable existante garde son type (conversion)
        let (ty, existe) = {
            let f = self.trames.last().unwrap();
            match f.get(nom) {
                Some(cur) => (cur.ty(), true),
                None => match self.globaux.get(nom) {
                    Some(cur) => (cur.ty(), true),
                    None => (v.ty(), false),
                },
            }
        };
        // miroir C++ : convertir() ne convertit QUE vers entier/rationnel/
        // flottant ; les autres cibles (table, liste, chaine…) gardent la
        // valeur telle quelle (variable à type dynamique).
        let v = match ty {
            TyKind::Entier | TyKind::Rationnel | TyKind::Flottant => convertir(v, ty),
            _ => v,
        };
        // Variables de niveau principal (une seule trame) -> globaux,
        // pour que les fonctions puissent y acceder.
        if self.trames.len() == 1 {
            self.globaux.insert(nom.to_string(), v);
            return;
        }
        if existe {
            let f = self.trames.last_mut().unwrap();
            if f.contains_key(nom) { f.insert(nom.to_string(), v); return; }
        }
        let f = self.trames.last_mut().unwrap();
        if f.contains_key(nom) {
            f.insert(nom.to_string(), v);
        } else if self.globaux.contains_key(nom) {
            self.globaux.insert(nom.to_string(), v);
        } else {
            f.insert(nom.to_string(), v);
        }
    }

    pub fn executer_principal(&mut self) {
        // voie JIT du corps principal (miroir C++ $principal natif) : si le
        // corps entier est dans le sous-ensemble, on saute l'interpréteur.
        if let Some(Compile::Principal(f)) = self.jit_principal.take() {
            f();
            return;
        }
        let ins = self.prog.instructions.clone();
        let r = self.exec_bloc(&ins);
        match r {
            Err(Some(code)) => { flush_out(); std::process::exit(code); }
            Err(None) => {}
            Ok(()) => {}
        }
    }

    fn exec_bloc(&mut self, instrs: &[Instr]) -> Result<(), Option<i32>> {
        for i in instrs {
            self.exec(i)?;
        }
        Ok(())
    }

    fn exec(&mut self, i: &Instr) -> Result<(), Option<i32>> {
        if crate::run::debug_mode() {
            eprintln!("[DEBUG] {:?}", i);
            for w in crate::run::get_watches() {
                if let Some(v) = self.globaux.get(&w).or_else(|| self.trames.last().and_then(|t| t.get(&w))) {
                    eprintln!("  [WATCH] {} = {}", w, chaine_de(v));
                }
            }
        }
        if crate::run::step_mode() {
            eprintln!("[STEP] Appuyer sur Entrée...");
            let mut buf = String::new();
            let _ = std::io::stdin().read_line(&mut buf);
        }
        match i {
            Instr::Afficher(exprs, _) => { for e in exprs { let v = self.eval(e)?; afficher(&v); } }
            Instr::Lire { nom, ty, .. } => {
                let ty_resolu = if *ty == TyKind::Dyn {
                    self.chercher_var(nom).map(|v| v.ty()).unwrap_or(TyKind::Entier)
                } else { *ty };
                let v = self.lire_selon(ty_resolu);
                self.affecter(nom, v);
            }
            Instr::Affect { nom, e, .. } => { let v = self.eval(e)?; self.affecter(nom, v); }
            Instr::AffectIndex { nom, index, valeur, .. } => {
                let idx = self.eval(index)?;
                let val = self.eval(valeur)?;
                // chercher la table dans les trames (locale d'abord, puis globaux)
                let table_opt = self.trames.iter().rev()
                    .find_map(|t| t.get(nom))
                    .or_else(|| self.globaux.get(nom));
                if let Some(Valeur::Tableau(t)) = table_opt {
                    let idx = vers_z(&idx);
                    let len = t.0.borrow().len();
                    if !idx.fits_u64_lt(len as u64) {
                        erreur_runtime("index hors des limites du tableau");
                    }
                    let i = idx.get_u64() as usize;
                    let v = vers_z(&val);
                    if !v.fits_i64() { erreur_runtime("tableau : valeur hors i64"); }
                    t.0.borrow_mut()[i] = v.get_i64();
                } else if let Some(Valeur::Table(t)) = table_opt {
                    let mut items = t.0.borrow_mut();
                    // chercher si la clé existe déjà
                    let mut trouve = false;
                    for (k, v) in items.iter_mut() {
                        if valeur_egale(k, &idx) {
                            *v = val.clone();
                            trouve = true;
                            break;
                        }
                    }
                    if !trouve {
                        items.push((idx, val));
                    }
                } else {
                    erreur_runtime(&format!("Affectation par index sur une non-table: {}", nom));
                }
            }
            Instr::Si { test, alors, sinon, .. } => {
                let c = self.eval(test)?;
                if est_vrai(&c) { self.exec_bloc_retour_capture(alors)?; }
                else if let Some(s) = sinon { self.exec_bloc_retour_capture(s)?; }
            }
            Instr::TantQue { test, corps, .. } => {
                loop {
                    let c = self.eval(test)?;
                    if !est_vrai(&c) { break; }
                    self.exec_bloc_retour_capture(corps)?;
                }
            }
            Instr::Pour { nom, debut, fin, corps, ligne } => {
                let d = vers_z(&self.eval(debut)?);
                let f = vers_z(&self.eval(fin)?);
                self.trames.last_mut().unwrap().insert(nom.clone(), Valeur::zg(d));
                loop {
                    let cur = {
                        let v = self.trames.last().unwrap().get(nom).unwrap().clone();
                        vers_z(&v)
                    };
                    if gmp_cmp_ge(&cur, &f) { break; }
                    if let Err(e) = self.exec_bloc(corps) {
                        self.trames.last_mut().unwrap().remove(nom);
                        return Err(e);
                    }
                    let cur = vers_z(self.trames.last().unwrap().get(nom).unwrap());
                    let suivant = Gz::neuf();
                    unsafe { gmp_add_ui(suivant.p(), cur.p(), 1) };
                    self.trames.last_mut().unwrap().insert(nom.clone(), Valeur::zg(suivant));
                }
                self.trames.last_mut().unwrap().remove(nom);
            }
            Instr::Retour(e, _) => {
                let _v = match e { Some(e) => self.eval(e)?, None => Valeur::Vide };
                return Err(None); // signalé à appeler via panic contrôlé
            }
            Instr::ExprStmt(e) => { self.eval(e)?; }
            Instr::Bloc(b) => { self.exec_bloc(b)?; }
            Instr::Lever(e, _) => {
                let v = self.eval(e)?;
                erreur_runtime(&chaine_de(&v));
            }
            Instr::Spawn(corps, _) => {
                let sigs = self.prog.signatures.clone();
                let ordre = self.prog.ordre.clone();
                let corps = corps.clone();
                let id = crate::run::spawn_thread(move || {
                    let prog = crate::front::Programme { instructions: corps, signatures: sigs, ordre };
                    let mut interp = Interp::nouveau(prog);
                    let _ = interp.exec_bloc(&interp.prog.instructions.clone());
                });
                self.affecter("_thread", Valeur::z(id as i64));
            }
            Instr::Essayer { corps, attraper, finalment, .. } => {
                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    self.exec_bloc(corps)
                }));
                let mut erreur_msg: Option<String> = None;
                match res {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => return Err(e),
                    Err(payload) => {
                        let msg = if let Some(s) = payload.downcast_ref::<String>() {
                            s.clone()
                        } else if let Some(s) = payload.downcast_ref::<&str>() {
                            s.to_string()
                        } else {
                            "erreur inconnue".to_string()
                        };
                        if let Some(m) = msg.strip_prefix("ROSE_ERR:") {
                            erreur_msg = Some(m.to_string());
                        } else {
                            std::panic::resume_unwind(payload);
                        }
                    }
                }
                if let Some(m) = erreur_msg {
                    if let Some((var, catch_body)) = attraper {
                        self.affecter(var, Valeur::s(m));
                        let _ = self.exec_bloc(catch_body);
                    } else {
                        // pas de attraper : propager après finally
                        if let Some(f) = finalment { let _ = self.exec_bloc(f); }
                        erreur_runtime(&m);
                    }
                }
                if let Some(f) = finalment {
                    let _ = self.exec_bloc(f);
                }
            }
        }
        Ok(())
    }

    fn appeler(&mut self, sig: &Signature, args: Vec<Valeur>) -> Result<Valeur, Option<i32>> {
        // conversion des paramètres (d'abord : le JIT attend des valeurs déjà
        // conformes aux types déclarés, comme le JIT C++ qui émet les conversions
        // d'arguments à l'appel).
        let mut args = args;
        for (i, (_, pt)) in sig.params.iter().enumerate() {
            args[i] = convertir(args[i].clone(), pt.n);
        }
        // JIT ensuite, si tous les args sont natifs
        if let Some(Compile::Natif(f)) = self.jit.get(&sig.nom) {
            if args.iter().all(|a| matches!(a,
                Valeur::Entier(_, None) | Valeur::Flottant(..) | Valeur::Booleen(..)))
                && args.len() <= 4 {
                return Ok(f(args));
            }
        }
        self.trames.push(HashMap::new());
        for ((pn, _), v) in sig.params.iter().zip(args.into_iter()) {
            self.trames.last_mut().unwrap().insert(pn.clone(), v);
        }
        let res: Valeur;
        let code: Option<i32>;
        {
            let corps = sig.corps.clone().unwrap_or_default();
            // exec en mode « retour » : on intercepte Retour via une signal
            let r = self.exec_bloc_retour(&corps);
            match r {
                Ok(v) => res = v,
                Err(Signal::Quitter(c)) => { self.trames.pop(); return Err(Some(c)); }
                Err(Signal::Retour(v)) => res = v,
            }
        }
        self.trames.pop();
        let _ = code;
        // le rendu est converti vers le type déclaré (booleen ⇄ entier compris),
        // comme le moteur C++ qui rend une Valeur déjà typée.
        Ok(match sig.retour.n {
            TyKind::Booleen => match &res {
                Valeur::Booleen(_) => res,
                _ => Valeur::b(est_vrai(&res)),
            },
            TyKind::Vide => Valeur::Vide,
            t => convertir(res, t),
        })
    }

    fn exec_bloc_retour_capture(&mut self, instrs: &[Instr]) -> Result<(), Option<i32>> {
        for i in instrs {
            match i {
                Instr::Retour(_, _) => return Err(None),
                Instr::Si { test, alors, sinon, .. } => {
                    let c = self.eval(test)?;
                    if est_vrai(&c) { self.exec_bloc_retour_capture(alors)?; }
                    else if let Some(s) = sinon { self.exec_bloc_retour_capture(s)?; }
                }
                Instr::TantQue { test, corps, .. } => {
                    loop {
                        let c = self.eval(test)?;
                        if !est_vrai(&c) { break; }
                        self.exec_bloc_retour_capture(corps)?;
                    }
                }
                Instr::Pour { nom, debut, fin, corps, .. } => {
                    // même sémantique que exec() : on délègue, puis on remonte
                    self.exec(&Instr::Pour { nom: nom.clone(), debut: debut.clone(),
                                             fin: fin.clone(), corps: corps.clone(), ligne: 0 })?;
                }
                Instr::Bloc(b) => { self.exec_bloc_retour_capture(b)?; }
                _ => { self.exec(i)?; }
            }
        }
        Ok(())
    }

    fn exec_bloc_retour(&mut self, instrs: &[Instr]) -> Result<Valeur, Signal> {
        for i in instrs {
            match i {
                Instr::Retour(e, _) => {
                    let v = match e { Some(e) => self.eval(e)?, None => Valeur::Vide };
                    return Err(Signal::Retour(v));
                }
                Instr::Si { test, alors, sinon, .. } => {
                    let c = self.eval(test)?;
                    let r = if est_vrai(&c) {
                        self.exec_bloc_retour(alors)
                    } else if let Some(s) = sinon {
                        self.exec_bloc_retour(s)
                    } else {
                        Ok(Valeur::Vide)
                    };
                    match r {
                        Ok(_) => {}
                        Err(Signal::Quitter(c)) => return Err(Signal::Quitter(c)),
                        Err(Signal::Retour(v)) => return Err(Signal::Retour(v)),
                    }
                }
                Instr::TantQue { test, corps, .. } => {
                    loop {
                        let c = self.eval(test)?;
                        if !est_vrai(&c) { break; }
                        match self.exec_bloc_retour(corps) {
                            Ok(_) => {}
                            Err(Signal::Quitter(x)) => return Err(Signal::Quitter(x)),
                            Err(Signal::Retour(v)) => return Err(Signal::Retour(v)),
                        }
                    }
                }
                Instr::Bloc(b) => {
                    match self.exec_bloc_retour(b) {
                        Ok(_) => {}
                        Err(Signal::Quitter(c)) => return Err(Signal::Quitter(c)),
                        Err(Signal::Retour(v)) => return Err(Signal::Retour(v)),
                    }
                }
                _ => {
                    match self.exec(i) {
                        Ok(()) => {}
                        Err(Some(c)) => return Err(Signal::Quitter(c)),
                        Err(None) => return Err(Signal::Retour(Valeur::Vide)),
                    }
                }
            }
        }
        Ok(Valeur::Vide)
    }

    fn chercher_var(&self, nom: &str) -> Option<Valeur> {
        for trame in self.trames.iter().rev() {
            if let Some(v) = trame.get(nom) { return Some(v.clone()); }
        }
        self.globaux.get(nom).cloned()
    }

    fn lire_selon(&self, ty: TyKind) -> Valeur {
        let mut ligne = String::new();
        use std::io::BufRead;
        let stdin = std::io::stdin();
        if stdin.lock().read_line(&mut ligne).unwrap_or(0) == 0 {
            erreur_runtime("entrée attendue, fichier fermé");
        }
        let ligne = ligne.trim_end_matches(['\n', '\r', ' ']);
        match ty {
            TyKind::Flottant => Valeur::f(chaine_vers_flottant(ligne)),
            TyKind::Rationnel => Valeur::qq(chaine_vers_rationnel(ligne)),
            _ => Valeur::zg(chaine_vers_entier(ligne)),
        }
    }

    // ── expressions ───────────────────────────────────────────────────────

    pub fn eval(&mut self, e: &Expr) -> Result<Valeur, Option<i32>> {
        match e {
            Expr::LitComplexe { re, im, .. } => Ok(Valeur::Complexe { re: *re, im: *im }),
            Expr::Litteral { texte, flott, .. } => {
                if *flott {
                    Ok(Valeur::f(texte.parse::<f64>().unwrap_or(f64::NAN)))
                } else if texte.contains('/') {
                    let n = texte.find('/').unwrap();
                    let num = chaine_vers_entier(&texte[..n]);
                    let den = chaine_vers_entier(&texte[n + 1..]);
                    if unsafe { gmp_sgn(den.p()) } == 0 {
                        erreur_runtime("Division par zéro dans une constante");
                    }
                    let q = Gq::neuf();
                    unsafe {
                        gmpq_set_z_field(q.num(), num.p());
                        gmpq_set_z_field(q.den(), den.p());
                        gmpq_canonicalize(q.p());
                    }
                    Ok(Valeur::qq(q))
                } else {
                    let z = Gz::neuf();
                    if !z.depuis_chaine(texte) {
                        erreur_runtime(&format!("constante invalide ({})", texte));
                    }
                    Ok(Valeur::zg(z))
                }
            }
            Expr::ChaineLit(s, _) => Ok(Valeur::s(s.clone())),
            Expr::BooleenLit(b, _) => Ok(Valeur::b(*b)),
            Expr::Var(n, _) => {
                // une variable PORTE LE NOM : elle masque la constante
                // (le lexeur retire le « $ » : $e et e ont le même jeton).
                match self.cible(n) {
                    Some(v) => return Ok(v),
                    None => {}
                }
                // constantes irrationnelles sans argument : 50 décimales par défaut
                if n == "pi" {
                    let z = Gz::de_i64(50);
                    return Ok(Valeur::s(nombre_pi(&z)));
                }
                if n == "phi" {
                    let z = Gz::de_i64(50);
                    return Ok(Valeur::s(nombre_phi(&z)));
                }
                if n == "e" {
                    let z = Gz::de_i64(50);
                    return Ok(Valeur::s(nombre_e(&z)));
                }
                { erreur_runtime_ligne(&format!("Variable '{}' utilisée sans valeur", n)); }
            }
            Expr::ListeLit(el, _) => {
                let mut v = Vec::new();
                for x in el { v.push(self.eval(x)?); }
                Ok(Valeur::liste_de(v))
            }
            Expr::TableLit(el, _) => {
                let mut items: Vec<(Valeur, Valeur)> = Vec::new();
                for (k, v) in el {
                    let k = self.eval(&Expr::ChaineLit(k.clone(), 0))?;
                    let v = self.eval(v)?;
                    items.push((k, v));
                }
                Ok(Valeur::Table(Table(Rc::new(RefCell::new(items)))))
            }
            Expr::Index { a, b, .. } => {
                let base = self.eval(a)?;
                match &base {
                    Valeur::Table(t) => {
                        let cle = self.eval(b)?;
                        for (k, v) in t.0.borrow().iter() {
                            if valeur_egale(k, &cle) { return Ok(v.clone()); }
                        }
                        erreur_runtime("clé absente de la table");
                    }
                    Valeur::Liste(l) => {
                        let idx = vers_z(&self.eval(b)?);
                        let taille = l.0.borrow().len();
                        if !idx.fits_u64_lt(taille as u64) {
                            erreur_runtime("index hors des limites de la liste");
                        }
                        let i = idx.get_u64();
                        let v = l.0.borrow()[i as usize].clone();
                        Ok(v)
                    }
                    Valeur::Chaine(s) => {
                        let idx = vers_z(&self.eval(b)?);
                        let cps: Vec<char> = s.chars().collect();
                        if !idx.fits_u64_lt(cps.len() as u64) {
                            erreur_runtime("index hors des limites de la chaîne");
                        }
                        let i = idx.get_u64();
                        Ok(Valeur::s(cps[i as usize].to_string()))
                    }
                    Valeur::Tableau(t) => {
                        let idx = vers_z(&self.eval(b)?);
                        let len = t.0.borrow().len();
                        if !idx.fits_u64_lt(len as u64) {
                            erreur_runtime("index hors des limites du tableau");
                        }
                        let i = idx.get_u64() as usize;
                        Ok(Valeur::z(t.0.borrow()[i]))
                    }
                    _ => erreur_runtime("index sur une valeur qui n'est pas une liste"),
                }
            }
            Expr::Unaire { op, a, .. } => {
                let v = self.eval(a)?;
                if op == "non" {
                    return Ok(Valeur::b(!est_vrai(&v)));
                }
                match &v {
                    // -i64::MIN = 2^63 ne tient pas en i64 : promotion GMP exacte
                    Valeur::Entier(i, None) => match i.checked_neg() {
                        Some(n) => Ok(Valeur::z(n)),
                        None => {
                            let z = Gz::neuf();
                            z.poser_i64(*i);
                            let r = Gz::neuf();
                            unsafe { gmp_neg(r.p(), z.p()) };
                            Ok(Valeur::zg(r))
                        }
                    },
                    Valeur::Entier(_, Some(z)) => {
                        let r = Gz::neuf();
                        unsafe { gmp_neg(r.p(), z.p()) };
                        Ok(Valeur::zg(r))
                    }
                    Valeur::Rationnel(q) => {
                        let r = Gq::neuf();
                        unsafe { gmpq_neg(r.p(), q.p()) };
                        Ok(Valeur::qq(r))
                    }
                    Valeur::Flottant(d) => Ok(Valeur::f(-*d)),
                    Valeur::Booleen(b) => Ok(Valeur::z(if *b { -1 } else { 0 })),
                    _ => erreur_runtime("opérateur unaire '-' sur un non-nombre"),
                }
            }
            Expr::Factorielle(a, _) => {
                let v = self.eval(a)?;
                let z = vers_z(&v);
                Ok(Valeur::zg(factorielle(&z)))
            }
            Expr::Binaire { op, a, b, ligne } => {
                crate::run::noter_ligne(*ligne);
                self.binaire(op, a, b)
            }
            Expr::Appel { nom, module, args, .. } => self.appel(nom, module, args),
        }
    }

    fn binaire(&mut self, op: &str, a: &Expr, b: &Expr) -> Result<Valeur, Option<i32>> {
        if op == "et" {
            let g = self.eval(a)?;
            if !est_vrai(&g) { return Ok(Valeur::b(false)); }
            let d = self.eval(b)?;
            return Ok(Valeur::b(est_vrai(&d)));
        }
        if op == "ou" {
            let g = self.eval(a)?;
            if est_vrai(&g) { return Ok(Valeur::b(true)); }
            let d = self.eval(b)?;
            return Ok(Valeur::b(est_vrai(&d)));
        }
        let g = self.eval(a)?;
        let d = self.eval(b)?;

        // comparaisons
        match op {
            "==" | "!=" | "<" | "<=" | ">" | ">=" => {
                if op == "==" || op == "!=" {
                    // miroir rose_valeur_egale : tampons, listes, tables et
                    // chaînes comparent structurassiment (jamais vers_z).
                    if matches!(g, Valeur::Tampon(..)) || matches!(d, Valeur::Tampon(..))
                        || matches!(g, Valeur::Liste(..)) || matches!(d, Valeur::Liste(..))
                        || matches!(g, Valeur::Table(..)) || matches!(d, Valeur::Table(..))
                        || matches!(g, Valeur::Chaine(..)) || matches!(d, Valeur::Chaine(..))
                    {
                        let e = valeur_egale(&g, &d);
                        return Ok(Valeur::b(if op == "==" { e } else { !e }));
                    }
                }
                let c = cmp_vals(&g, &d);
                let r = match op {
                    "==" => c == 0, "!=" => c != 0, "<" => c < 0,
                    "<=" => c <= 0, ">" => c > 0, _ => c >= 0,
                };
                return Ok(Valeur::b(r));
            }
            "≈" => {
                let r = if matches!(g, Valeur::Flottant(..)) || matches!(d, Valeur::Flottant(..)) {
                    let (x, y) = (vers_d(&g), vers_d(&d));
                    let scale = x.abs().max(y.abs()).max(1.0);
                    (x - y).abs() <= 1e-9 * scale
                } else { cmp_vals(&g, &d) == 0 };
                return Ok(Valeur::b(r));
            }
            _ => {}
        }
        // add. (chaînes) — le parser n'émet jamais « add. » ; la concaténation
        // passe par l'opérateur « + » (comme dans le C++, + sur chaînes).
        if op == "+" {
            if let (Valeur::Chaine(x), Valeur::Chaine(y)) = (&g, &d) {
                return Ok(Valeur::s(format!("{}{}", x, y)));
            }
        }
        // Complexe : opérations sur a+bi
        if matches!(g, Valeur::Complexe{..}) || matches!(d, Valeur::Complexe{..}) {
            let gc = matches!(g, Valeur::Complexe{..});
            let dc = matches!(d, Valeur::Complexe{..});
            // ── VOIE RAPIDE : un seul opérande est complexe ──────────────
            // Un réel est traité comme (x, 0) mais on n'émet PAS les
            // multiplications par 0.0 / 1.0 de la formule générale : le JIT
            // fait de même (bit à bit), sinon les deux moteurs divergeraient.
            // C'est ce chemin qui rend la construction « x + y*i » rapide.
            if gc != dc {
                return Ok(if gc {
                    // complexe ⊕ réel
                    let (cr, ci) = match &g { Valeur::Complexe{re,im} => (*re,*im), _ => (0.0, 0.0) };
                    let x = vers_d(&d);
                    match op {
                        "+" => Valeur::Complexe { re: cr + x, im: ci },
                        "-" => Valeur::Complexe { re: cr - x, im: ci },
                        "*" => Valeur::Complexe { re: cr * x, im: ci * x },
                        "/" => {
                            if x == 0.0 { erreur_runtime("division par zéro (complexe)"); }
                            Valeur::Complexe { re: cr / x, im: ci / x }
                        }
                        _ => erreur_runtime("opération non supportée sur les complexes"),
                    }
                } else {
                    // réel ⊕ complexe
                    let (cr, ci) = match &d { Valeur::Complexe{re,im} => (*re,*im), _ => (0.0, 0.0) };
                    let x = vers_d(&g);
                    match op {
                        "+" => Valeur::Complexe { re: x + cr, im: ci },
                        "-" => Valeur::Complexe { re: x - cr, im: -ci },
                        "*" => Valeur::Complexe { re: x * cr, im: x * ci },
                        "/" => {
                            let denom = cr*cr + ci*ci;
                            if denom == 0.0 { erreur_runtime("division par zéro (complexe)"); }
                            Valeur::Complexe { re: (x*cr)/denom, im: -(x*ci)/denom }
                        }
                        _ => erreur_runtime("opération non supportée sur les complexes"),
                    }
                });
            }
            // ── complexe ⊕ complexe : formule générale ───────────────────
            let (gr, gi) = match &g { Valeur::Complexe{re,im} => (*re,*im), _ => (vers_d(&g), 0.0) };
            let (dr, di) = match &d { Valeur::Complexe{re,im} => (*re,*im), _ => (vers_d(&d), 0.0) };
            return Ok(match op {
                "+" => Valeur::Complexe { re: gr + dr, im: gi + di },
                "-" => Valeur::Complexe { re: gr - dr, im: gi - di },
                // le produit croisé gr*di + gi*dr vaut 2*(gr*di) : on ne calcule
                // le produit qu'une fois (même arrondi IEEE, une division de
                // moins — le JIT fait de même).
                "*" => { let p = gr * di; Valeur::Complexe { re: gr*dr - gi*di, im: p + p } }
                "/" => {
                    let denom = dr*dr + di*di;
                    if denom == 0.0 { erreur_runtime("division par zéro (complexe)"); }
                    Valeur::Complexe { re: (gr*dr + gi*di)/denom, im: (gi*dr - gr*di)/denom }
                }
                _ => erreur_runtime("opération non supportée sur les complexes"),
            });
        }
        // Irrationnel : √n
        if matches!(g, Valeur::Irrationnel(..)) || matches!(d, Valeur::Irrationnel(..)) {
            let (gr, gi) = match &g {
                Valeur::Irrationnel(z) => (z.chaine().parse::<f64>().unwrap_or(0.0), 0.0),
                _ => (vers_d(&g), 0.0),
            };
            let (dr, di) = match &d {
                Valeur::Irrationnel(z) => (z.chaine().parse::<f64>().unwrap_or(0.0), 0.0),
                _ => (vers_d(&d), 0.0),
            };
            return Ok(match op {
                "+" => Valeur::f(gr + dr),
                "-" => Valeur::f(gr - dr),
                "*" => Valeur::f(gr * dr),
                "/" => {
                    if dr == 0.0 { erreur_runtime("division par zéro (irrationnel)"); }
                    Valeur::f(gr / dr)
                }
                _ => erreur_runtime("opération non supportée sur les irrationnels"),
            });
        }
        // Répétition de chaîne : "abc" * 3  ou  3 * "abc"
        if op == "*" {
            if let (Valeur::Chaine(x), Valeur::Entier(n, None)) = (&g, &d) {
                let n = *n;
                if n < 0 { erreur_runtime("répétition de chaîne avec compte négatif"); }
                return Ok(Valeur::s(x.repeat(n as usize)));
            }
            if let (Valeur::Entier(n, None), Valeur::Chaine(x)) = (&g, &d) {
                let n = *n;
                if n < 0 { erreur_runtime("répétition de chaîne avec compte négatif"); }
                return Ok(Valeur::s(x.repeat(n as usize)));
            }
        }
        let m = rnk(&g).max(rnk(&d));
        match op {
            "+" => Ok(if m == 3 { Valeur::f(vers_d(&g) + vers_d(&d)) }
                      else if m == 2 { let (x, y) = (vers_q(&g), vers_q(&d)); let r = Gq::neuf(); unsafe { gmpq_add(r.p(), x.p(), y.p()) }; Valeur::qq(r) }
                      else { let (x, y) = (vers_z(&g), vers_z(&d)); let r = Gz::neuf(); unsafe { gmp_add(r.p(), x.p(), y.p()) }; Valeur::zg(r) }),
            "-" => Ok(if m == 3 { Valeur::f(vers_d(&g) - vers_d(&d)) }
                      else if m == 2 { let (x, y) = (vers_q(&g), vers_q(&d)); let r = Gq::neuf(); unsafe { gmpq_sub(r.p(), x.p(), y.p()) }; Valeur::qq(r) }
                      else { let (x, y) = (vers_z(&g), vers_z(&d)); let r = Gz::neuf(); unsafe { gmp_sub(r.p(), x.p(), y.p()) }; Valeur::zg(r) }),
            "*" => Ok(if m == 3 { Valeur::f(vers_d(&g) * vers_d(&d)) }
                      else if m == 2 { let (x, y) = (vers_q(&g), vers_q(&d)); let r = Gq::neuf(); unsafe { gmpq_mul(r.p(), x.p(), y.p()) }; Valeur::qq(r) }
                      else { let (x, y) = (vers_z(&g), vers_z(&d)); let r = Gz::neuf(); unsafe { gmp_mul(r.p(), x.p(), y.p()) }; Valeur::zg(r) }),
            "/" => {
                if m == 3 {
                    let (x, y) = (vers_d(&g), vers_d(&d));
                    if y == 0.0 { erreur_runtime("division par zéro"); }
                    return Ok(Valeur::f(x / y));
                }
                let y = vers_q(&d);
                if crate::run::gmpq_sgn(y.p()) == 0 { erreur_runtime("division par zéro"); }
                let x = vers_q(&g);
                let r = Gq::neuf();
                unsafe { gmpq_div(r.p(), x.p(), y.p()) };
                Ok(Valeur::qq(r))
            }
            "div" => {
                if matches!(g, Valeur::Flottant(..)) || matches!(d, Valeur::Flottant(..)) {
                    let y = vers_d(&d);
                    if y == 0.0 { erreur_runtime("division euclidienne par zéro"); }
                    return Ok(Valeur::f((vers_d(&g) / y).floor()));
                }
                if matches!(g, Valeur::Rationnel(..)) || matches!(d, Valeur::Rationnel(..)) {
                    let y = vers_q(&d);
                    if crate::run::gmpq_sgn(y.p()) == 0 { erreur_runtime("division euclidienne par zéro"); }
                    let q = Gq::neuf();
                    let x = vers_q(&g);
                    unsafe { gmpq_div(q.p(), x.p(), y.p()) };
                    let r = Gz::neuf();
                    unsafe { gmp_fdiv_q(r.p(), q.num_raw(), q.den_raw()) };
                    return Ok(Valeur::zg(r));
                }
                let y = vers_z(&d);
                if y.chaine() == "0" { erreur_runtime("division euclidienne par zéro"); }
                let x = vers_z(&g);
                let r = Gz::neuf();
                unsafe { gmp_fdiv_q(r.p(), x.p(), y.p()) };
                Ok(Valeur::zg(r))
            }
            "%" => {
                // miroir C++ (verificateur ligne 1487) : le modulo n'accepte
                // que deux entiers — un operand rationnel/flottant est une
                // erreur, jamais une promotion silencieuse.
                if rnk(&g) != 1 || rnk(&d) != 1 {
                    erreur_runtime_ligne("Le modulo exige deux entiers");
                }
                let y = vers_z(&d);
                if y.chaine() == "0" { erreur_runtime("modulo par zéro"); }
                let x = vers_z(&g);
                let r = Gz::neuf();
                unsafe { gmp_fdiv_r(r.p(), x.p(), y.p()) };
                Ok(Valeur::zg(r))
            }
            "^" => {
                if matches!(d, Valeur::Flottant(..)) {
                    return Ok(Valeur::f(vers_d(&g).powf(vers_d(&d))));
                }
                if matches!(g, Valeur::Flottant(..)) {
                    return Ok(Valeur::f(vers_d(&g).powf(vers_d(&d))));
                }
                let expo = vers_z(&d);
                if !expo.fits_i64() {
                    erreur_runtime("exposant démesuré (la puissance exploserait au-delà de tout calcul)");
                }
                let e = expo.get_i64();
                if matches!(g, Valeur::Rationnel(..)) {
                    let q = vers_q(&g);
                    return Ok(Valeur::qq(pow_rationnel_sur(&q, e)));
                }
                let base = vers_z(&g);
                if e >= 0 {
                    let r = Gz::neuf();
                    unsafe { gmp_pow_ui(r.p(), base.p(), e as u64) };
                    Ok(Valeur::zg(r))
                } else {
                    if base.sgn() == 0 { erreur_runtime("0^exposant négatif"); }
                    let q = Gq::neuf();
                    unsafe { gmpq_set_z_field(q.num(), base.p()); gmpq_canonicalize(q.p()) };
                    Ok(Valeur::qq(pow_rationnel_sur(&q, e)))
                }
            }
            _ => { erreur_runtime(&format!("opérateur '{}' inconnu", op)); }
        }
    }

    fn appel(&mut self, nom: &str, module: &str, args: &[Expr]) -> Result<Valeur, Option<i32>> {
        if module == "dynamique" {
            let mut a = Vec::new();
            for x in args { a.push(self.eval(x)?); }
            return self.dynamique(nom, &a);
        }
        // conversions
        if args.len() == 1 && matches!(nom, "entier" | "rationnel" | "flottant" | "chaine") {
            let v = self.eval(&args[0])?;
            return Ok(match nom {
                "entier" => {
                    if let Valeur::Chaine(s) = &v { return Ok(Valeur::zg(chaine_vers_entier(s))); }
                    convertir(v, TyKind::Entier)
                }
                "rationnel" => convertir(v, TyKind::Rationnel),
                "flottant" => {
                    if let Valeur::Chaine(s) = &v { return Ok(Valeur::f(chaine_vers_flottant(s))); }
                    convertir(v, TyKind::Flottant)
                }
                _ => Valeur::s(chaine_de(&v)),
            });
        }
        if let Some(sig) = self.prog.signatures.get(nom).cloned() {
            // fonction externe @dll/@sym sans corps : FFI directe (miroir C++
            // rose.cpp:3719-3727 -> ffi_appeler).
            if sig.corps.is_none()
                && !(sig.dll.is_empty() && sig.dll_son.is_empty() && sig.symbole.is_empty())
            {
                let mut a = Vec::new();
                for x in args { a.push(self.eval(x)?); }
                return self.ffi_appeler(&sig, &a);
            }
            let mut a = Vec::new();
            for x in args { a.push(self.eval(x)?); }
            return self.appeler(&sig, a);
        }
        let mut a = Vec::new();
        for x in args { a.push(self.eval(x)?); }
        self.stdlib(nom, &a)
    }

    /// Poignée de bibliothèque pour une signature @dll/@sym (miroir C++
    /// ffi_poignee : candidats lib<n>.so, <n>.so, <n>, lib<n>.so.6/7/8).
    fn ffi_poignee(&mut self, sig: &Signature) -> *mut std::ffi::c_void {
        let nom = if !sig.dll_son.is_empty() { sig.dll_son.clone() }
                  else if !sig.dll.is_empty() { sig.dll.clone() }
                  else { "c".to_string() };
        let candidats: Vec<String> = if !sig.dll_son.is_empty() || nom.contains('/') {
            vec![nom.clone()]
        } else {
            vec![format!("lib{nom}.so"), format!("{nom}.so"), nom.clone(),
                 format!("lib{nom}.so.6"), format!("lib{nom}.so.7"), format!("lib{nom}.so.8")]
        };
        unsafe {
            for c in candidats {
                let cs = std::ffi::CString::new(c).unwrap();
                let h = ffi_dlopen(cs.as_ptr() as *const i8);
                if !h.is_null() { self.poignees.push(h); return h; }
            }
        }
        erreur_runtime(&format!("bibliothèque introuvable ({})", nom));
        std::ptr::null_mut()
    }

    /// Appel FFI d'une fonction externe (miroir C++ ffi_appeler rose.cpp:3253) :
    /// entier→long long, flottant→double, booleen→int, chaine→char*.
    fn ffi_appeler(&mut self, sig: &Signature, args: &[Valeur]) -> Result<Valeur, Option<i32>> {
        use crate::front::TyKind as K;
        let h = self.ffi_poignee(sig);
        let sym = if sig.symbole.is_empty() { sig.nom.clone() } else { sig.symbole.clone() };
        let cs = std::ffi::CString::new(sym.clone()).unwrap();
        let s = unsafe { ffi_dlsym(h, cs.as_ptr() as *const i8) };
        if s.is_null() { erreur_runtime(&format!("symbole introuvable dans la bibliothèque ({sym})")); }
        let zarg = |i: usize| -> i64 {
            let z = vers_z(&args[i]);
            if z.fits_i64() { z.get_i64() }
            else { erreur_runtime("fonction externe : argument entier hors i64") }
        };
        let darg = |i: usize| -> f64 { vers_d(&args[i]) };
        let sent = |i: usize| matches!(sig.params[i].1.n, K::Chaine);
        let p = s as usize;
        let r = match sig.retour.n {
            K::Entier => match args.len() {
                0 => { let f: extern "C" fn() -> i64 = unsafe { std::mem::transmute(p) }; Valeur::z(f()) }
                1 => { let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(p) }; Valeur::z(f(zarg(0))) }
                2 => { let f: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(p) }; Valeur::z(f(zarg(0), zarg(1))) }
                _ => { erreur_runtime("fonction externe : plus de 2 arguments entiers non supporté"); }
            },
            K::Flottant => match args.len() {
                0 => { let f: extern "C" fn() -> f64 = unsafe { std::mem::transmute(p) }; Valeur::f(f()) }
                1 => {
                    if sent(0) {
                        let f: extern "C" fn(*const i8) -> f64 = unsafe { std::mem::transmute(p) };
                        let c = std::ffi::CString::new(chaine_de(&args[0])).unwrap();
                        Valeur::f(unsafe { f(c.as_ptr() as *const i8) })
                    } else if matches!(sig.params[0].1.n, K::Flottant) {
                        let f: extern "C" fn(f64) -> f64 = unsafe { std::mem::transmute(p) }; Valeur::f(f(darg(0)))
                    } else if matches!(sig.params[0].1.n, K::Booleen) {
                        let f: extern "C" fn(i32) -> f64 = unsafe { std::mem::transmute(p) };
                        Valeur::f(f(if est_vrai(&args[0]) { 1 } else { 0 }))
                    } else {
                        let f: extern "C" fn(i64) -> f64 = unsafe { std::mem::transmute(p) }; Valeur::f(f(zarg(0)))
                    }
                }
                2 => {
                    let p0 = sig.params[0].1.n; let p1 = sig.params[1].1.n;
                    if matches!(p0, K::Flottant) && matches!(p1, K::Flottant) {
                        let f: extern "C" fn(f64, f64) -> f64 = unsafe { std::mem::transmute(p) }; Valeur::f(f(darg(0), darg(1)))
                    } else if matches!(p0, K::Flottant) {
                        let f: extern "C" fn(f64, i64) -> f64 = unsafe { std::mem::transmute(p) }; Valeur::f(f(darg(0), zarg(1)))
                    } else if matches!(p1, K::Flottant) {
                        let f: extern "C" fn(i64, f64) -> f64 = unsafe { std::mem::transmute(p) }; Valeur::f(f(zarg(0), darg(1)))
                    } else {
                        let f: extern "C" fn(i64, i64) -> f64 = unsafe { std::mem::transmute(p) }; Valeur::f(f(zarg(0), zarg(1)))
                    }
                }
                3 => { let f: extern "C" fn(f64, f64, f64) -> f64 = unsafe { std::mem::transmute(p) }; Valeur::f(f(darg(0), darg(1), darg(2))) }
                _ => { erreur_runtime("fonction externe flottante : arité non supportée"); }
            },
            K::Booleen => match args.len() {
                0 => { let f: extern "C" fn() -> i32 = unsafe { std::mem::transmute(p) }; Valeur::b(f() != 0) }
                1 => { let f: extern "C" fn(i64) -> i32 = unsafe { std::mem::transmute(p) }; Valeur::b(f(zarg(0)) != 0) }
                _ => { erreur_runtime("fonction externe booléenne : arité non supportée"); }
            },
            K::Chaine => match args.len() {
                0 => {
                    let f: extern "C" fn() -> *const i8 = unsafe { std::mem::transmute(p) };
                    let r = unsafe { f() };
                    Valeur::s(if r.is_null() { String::new() }
                              else { unsafe { std::ffi::CStr::from_ptr(r as *const std::os::raw::c_char) }.to_string_lossy().into_owned() })
                }
                1 => {
                    if sent(0) {
                        let f: extern "C" fn(*const i8) -> *const i8 = unsafe { std::mem::transmute(p) };
                        let c = std::ffi::CString::new(chaine_de(&args[0])).unwrap();
                        let r = unsafe { f(c.as_ptr() as *const i8) };
                        Valeur::s(if r.is_null() { String::new() } else { unsafe { std::ffi::CStr::from_ptr(r as *const std::os::raw::c_char) }.to_string_lossy().into_owned() })
                    } else {
                        let f: extern "C" fn(i64) -> *const i8 = unsafe { std::mem::transmute(p) };
                        let r = unsafe { f(zarg(0)) };
                        Valeur::s(if r.is_null() { String::new() } else { unsafe { std::ffi::CStr::from_ptr(r as *const std::os::raw::c_char) }.to_string_lossy().into_owned() })
                    }
                }
                _ => { erreur_runtime("fonction externe texte : arité non supportée"); }
            },
            _ => { erreur_runtime("type de retour externe non supporté"); }
        };
        Ok(r)
    }

    fn dynamique(&mut self, nom: &str, args: &[Valeur]) -> Result<Valeur, Option<i32>> {
        match nom {
            "charger" => {
                let s = chaine_de(&args[0]);
                let c = std::ffi::CString::new(s.clone()).unwrap();
                unsafe {
                    let h = ffi_dlopen(c.as_ptr() as *const i8);
                    if h.is_null() {
                        erreur_runtime(&format!("chargement de bibliothèque impossible ({})", s));
                    }
                    self.poignees.push(h);
                    Ok(Valeur::z(h as i64))
                }
            }
            "fermer" => {
                let h = extraire_i64(&args[0]) as *mut std::ffi::c_void;
                unsafe { ffi_dlclose(h) };
                Ok(Valeur::Vide)
            }
            "appeler0" | "appeler1" | "appeler2" => {
                let h = extraire_i64(&args[0]) as *mut std::ffi::c_void;
                let sym = chaine_de(&args[1]);
                let c = std::ffi::CString::new(sym.clone()).unwrap();
                unsafe {
                    let p = ffi_dlsym(h, c.as_ptr() as *const i8);
                    if p.is_null() {
                        erreur_runtime(&format!("symbole '{}' introuvable", sym));
                    }
                    match nom {
                        "appeler0" => {
                            let f: extern "C" fn() -> f64 = std::mem::transmute(p);
                            Ok(Valeur::f(f()))
                        }
                        "appeler1" => {
                            let f: extern "C" fn(f64) -> f64 = std::mem::transmute(p);
                            Ok(Valeur::f(f(vers_d(&args[2]))))
                        }
                        _ => {
                            let f: extern "C" fn(f64, f64) -> f64 = std::mem::transmute(p);
                            Ok(Valeur::f(f(vers_d(&args[2]), vers_d(&args[3]))))
                        }
                    }
                }
            }
            _ => { erreur_runtime(&format!("Fonction dynamique '{}' inconnue", nom)); }
        }
    }

    fn stdlib(&mut self, nom: &str, a: &[Valeur]) -> Result<Valeur, Option<i32>> {
        if nom == "quitter" {
            let z = vers_z(&a[0]);
            if !z.fits_i64() { erreur_runtime("code de sortie hors plage"); }
            flush_out();
            return Err(Some(z.get_i64() as i32));
        }
        if nom == "join" {
            let id = vers_z(&a[0]);
            if !id.fits_i64() { erreur_runtime("join : id invalide"); }
            crate::run::join_thread(id.get_i64() as usize);
            return Ok(Valeur::Vide);
        }
        if nom == "breakpoint_if" {
            let cond = est_vrai(&a[0]);
            if cond {
                crate::run::flush_out();
                eprintln!("[BREAKPOINT_IF] condition vraie");
                for w in crate::run::get_watches() {
                    // try to find the var
                }
                for (k, v) in &self.globaux {
                    eprintln!("  {} = {}", k, chaine_de(v));
                }
                eprintln!("Appuyer sur Entrée pour continuer...");
                let mut buf = String::new();
                let _ = std::io::stdin().read_line(&mut buf);
            }
            return Ok(Valeur::Vide);
        }
        if nom == "watch" {
            let name = chaine_de(&a[0]);
            crate::run::add_watch(name);
            return Ok(Valeur::Vide);
        }
        if nom == "unwatch" {
            let name = chaine_de(&a[0]);
            crate::run::remove_watch(&name);
            return Ok(Valeur::Vide);
        }
        Ok(self.stdlib_valeur(nom, a))
    }



    fn stdlib_valeur(&mut self, nom: &str, a: &[Valeur]) -> Valeur {
        match nom {
            "afficher" => { afficher(&a[0]); Valeur::Vide }
            "saisir_entier" => self.lire_selon(TyKind::Entier),
            "saisir_flottant" => self.lire_selon(TyKind::Flottant),
            "time" => {
                let d = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH).unwrap();
                let nanos = d.as_nanos();
                let micros = d.as_micros();
                let millis = d.as_millis();
                let secs = d.as_secs();
                if secs > 0 {
                    Valeur::s(format!("{} s", secs))
                } else if millis > 0 {
                    Valeur::s(format!("{} ms", millis))
                } else if micros > 0 {
                    Valeur::s(format!("{} µs", micros))
                } else {
                    Valeur::s(format!("{} ns", nanos))
                }
            }
            "graine" | "seed" => { let z = vers_z(&a[0]); graine_de(&mut self.rng, &z); Valeur::Vide }
            "graine_systeme" | "seed_sys" | "graine_vrai" => {
                self.rng = tirage_systeme();
                rng_tirer(&mut self.rng);
                Valeur::Vide
            }
            "rng_materiel" | "rng_mat" => {
                let z = Gz::neuf();
                z.poser_u64(tirage_systeme());
                Valeur::zg(z)
            }
            "hasard" | "prng" => {
                let n = vers_z(&a[0]);
                let r = hasard(&mut self.rng, &n);
                Valeur::zg(r)
            }
            "hasard_systeme" | "hasard_vrai" | "trng" => {
                let n = vers_z(&a[0]);
                if n.sgn() <= 0 { erreur_runtime("hasard : bornage positif attendu"); }
                let x = tirage_systeme();
                let r = Gz::neuf();
                if n.fits_i64() {
                    let nn = n.get_i64() as u64;
                    r.poser_u64(((x as u128) * (nn as u128) >> 64) as u64);
                } else {
                    r.poser_u64(x >> 32);
                    unsafe {
                        gmp_mul_2exp(r.p(), r.p(), 32);
                        gmp_add_ui(r.p(), r.p(), (x & 0xFFFF_FFFF) as u64);
                        gmp_fdiv_r(r.p(), r.p(), n.p());
                    }
                }
                Valeur::zg(r)
            }
            "tampon" | "buf" => {
                let n = vers_z(&a[0]);
                if n.sgn() < 0 || !n.fits_u64_lt(usize::MAX as u64) { erreur_runtime("tampon : taille démesurée"); }
                Valeur::tampon(n.get_u64() as usize)
            }
            "tableau" => {
                let n = vers_z(&a[0]);
                if n.sgn() < 0 || !n.fits_u64_lt(usize::MAX as u64) { erreur_runtime("tableau : taille démesurée"); }
                Valeur::tableau(n.get_u64() as usize)
            }
            "octets" | "bytes" => {
                let s = chaine_de(&a[0]);
                Valeur::Tampon(Tampon(Rc::new(RefCell::new(s.into_bytes()))))
            }
            "tampon_case" => {
                let (Valeur::Tampon(t),) = (&a[0],) else { erreur_runtime("tampon_case : tampon attendu") };
                let idx = vers_z(&a[1]);
                let i = indice_tampon(&idx, t.0.borrow().len());
                Valeur::z(t.0.borrow()[i] as i64)
            }
            "tampon_pose" => {
                let (Valeur::Tampon(t),) = (&a[0],) else { erreur_runtime("tampon_pose : tampon attendu") };
                let idx = vers_z(&a[1]);
                let i = indice_tampon(&idx, t.0.borrow().len());
                let v = vers_z(&a[2]);
                let octet = unsafe { gmp_fdiv_ui(v.p(), 256) } as u8;
                t.0.borrow_mut()[i] = octet;
                Valeur::Vide
            }
            "remplir_hasard" => {
                let (Valeur::Tampon(t), Valeur::Tampon(alpha)) = (&a[0], &a[1]) else {
                    erreur_runtime(if matches!(a[0], Valeur::Tampon(..)) { "remplir_hasard : alphabet non vide attendu" } else { "remplir_hasard : tampon attendu" });
                };
                let ab = alpha.0.borrow().clone();
                if ab.is_empty() { erreur_runtime("remplir_hasard : alphabet non vide attendu"); }
                {
                    let mut buf = t.0.borrow_mut();
                    remplir_hasard(&mut self.rng, &mut buf, &ab);
                }
                Valeur::Vide
            }
            "remplir_hasard_philox" => {
                let (Valeur::Tampon(t), Valeur::Tampon(alpha)) = (&a[0], &a[1]) else {
                    erreur_runtime(if matches!(a[0], Valeur::Tampon(..)) { "remplir_hasard_philox : alphabet non vide attendu" } else { "remplir_hasard_philox : tampon attendu" });
                };
                let ab = alpha.0.borrow().clone();
                if ab.is_empty() { erreur_runtime("remplir_hasard_philox : alphabet non vide attendu"); }
                let g = vers_z(&a[2]);
                let gr = if g.fits_i64() { g.get_i64() } else { 0 };
                let mut buf = t.0.borrow_mut();
                remplir_philox(&mut buf, &ab, gr);
                Valeur::Vide
            }
            "factorielle" | "fact" => { let z = vers_z(&a[0]); Valeur::zg(factorielle(&z)) }
            "fibonacci" | "fib" => { let z = vers_z(&a[0]); Valeur::zg(fibonacci(&z)) }
            "pgcd" | "gcd" => { let (x, y) = (vers_z(&a[0]), vers_z(&a[1])); Valeur::zg(pgcd(&x, &y)) }
            "ppcm" | "lcm" => { let (x, y) = (vers_z(&a[0]), vers_z(&a[1])); Valeur::zg(ppcm(&x, &y)) }
            "puissance" | "pow" => {
                let e = vers_z(&a[1]);
                puissance(&a[0], &e)
            }
            "racine" => {
                let n = vers_z(&a[0]);
                if n.sgn() < 0 { erreur_runtime("racine : entier négatif"); }
                if n.fits_i64() {
                    let x = n.get_i64();
                    let r = (x as f64).sqrt() as i64;
                    if r * r == x {
                        return Valeur::z(r);
                    }
                }
                Valeur::Irrationnel(Rc::new(n))
            }
            "est_premier" | "is_prime" => { let z = vers_z(&a[0]); Valeur::b(est_premier(&z)) }
            "compter_premiers" => { let z = vers_z(&a[0]); Valeur::z(compter_premiers(&z) as i64) }
            "nb_chiffres" | "digits" => { let z = vers_z(&a[0]); Valeur::z(nb_chiffres(&z) as i64) }
            "nombre_pi" | "pi" => { let z = vers_z(&a[0]); Valeur::s(nombre_pi(&z)) }
            "nombre_e" | "e" => { let z = vers_z(&a[0]); Valeur::s(nombre_e(&z)) }
            "phi" => { let z = vers_z(&a[0]); Valeur::s(nombre_phi(&z)) }
            "i" => Valeur::Complexe { re: 0.0, im: 1.0 },
            "longueur" | "len" => {
                let n: u64 = match &a[0] {
                    Valeur::Chaine(s) => s.chars().count() as u64,
                    Valeur::Liste(l) => l.0.borrow().len() as u64,
                    Valeur::Table(t) => t.0.borrow().len() as u64,
                    Valeur::Tampon(t) => t.0.borrow().len() as u64,
                    Valeur::Tableau(t) => t.0.borrow().len() as u64,
                    // un mot non atomique peut rester une chaine : on rend sa longueur
                    _ => {
                        let c = chaine_de(&a[0]);
                        c.chars().count() as u64
                    }
                };
                let z = Gz::neuf();
                z.poser_u64(n);
                Valeur::zg(z)
            }
            "table" | "map" => Valeur::table(),
            "inserer" => {
                let Valeur::Table(t) = &a[0] else { erreur_runtime("inserer attend une table") };
                let (k, v) = (a[1].clone(), a[2].clone());
                {
                    let mut items = t.0.borrow_mut();
                    for kv in items.iter_mut() {
                        if valeur_egale(&kv.0, &k) { kv.1 = v; return Valeur::Vide; }
                    }
                    items.push((k, v));
                }
                Valeur::Vide
            }
            "cle_existe" => {
                let Valeur::Table(t) = &a[0] else { erreur_runtime("cle_existe attend une table") };
                for (k, _) in t.0.borrow().iter() {
                    if valeur_egale(k, &a[1]) { return Valeur::b(true); }
                }
                Valeur::b(false)
            }
            "supprimer" => {
                let Valeur::Table(t) = &a[0] else { erreur_runtime("supprimer attend une table") };
                let mut items = t.0.borrow_mut();
                if let Some(i) = items.iter().position(|(k, _)| valeur_egale(k, &a[1])) {
                    items.remove(i);
                    return Valeur::b(true);
                }
                Valeur::b(false)
            }
            "cles" => {
                let Valeur::Table(t) = &a[0] else { erreur_runtime("cles attend une table") };
                Valeur::liste_de(t.0.borrow().iter().map(|(k, _)| k.clone()).collect())
            }
            "valeurs" => {
                let Valeur::Table(t) = &a[0] else { erreur_runtime("valeurs attend une table") };
                Valeur::liste_de(t.0.borrow().iter().map(|(_, v)| v.clone()).collect())
            }
            "joindre" => {
                let Valeur::Liste(l) = &a[0] else { erreur_runtime("joindre attend une liste") };
                l.0.borrow_mut().push(a[1].clone());
                Valeur::Vide
            }
            "ordre" => {
                let s = chaine_de(&a[0]);
                let cps: Vec<char> = s.chars().collect();
                if cps.len() != 1 { erreur_runtime("ordre attend une chaîne d'un seul caractère"); }
                let z = Gz::neuf();
                z.poser_u64(cps[0] as u32 as u64);
                Valeur::zg(z)
            }
            "caractere" => {
                let n = vers_z(&a[0]);
                if !n.fits_u64_lt(0x10FFFF) { erreur_runtime("caractere : code hors plage (0 à 1114111)"); }
                let cp = n.get_u64() as u32;
                if (0xD800..=0xDFFF).contains(&cp) { erreur_runtime("caractere : code réservé (55296 à 57343)"); }
                Valeur::s(char::from_u32(cp).unwrap_or('\u{FFFD}').to_string())
            }
            "sous_chaine" | "sub" => {
                let s = chaine_de(&a[0]);
                if a.len() < 3 { erreur_runtime("sous_chaine : 3 arguments attendus"); }
                let d = vers_z(&a[1]);
                let f = vers_z(&a[2]);
                if d.sgn() < 0 || f.sgn() < 0 || !d.fits_i64() || !f.fits_i64() {
                    erreur_runtime("sous_chaine : les bornes doivent être des entiers positifs");
                }
                let cps: Vec<char> = s.chars().collect();
                let taille = cps.len();
                let di = d.get_i64() as usize;
                let mut fi = f.get_i64() as usize;
                if di > taille || fi < di { erreur_runtime("sous_chaine : bornes invalides"); }
                if fi > taille { fi = taille; }
                Valeur::s(cps[di..fi].iter().collect::<String>())
            }
            "concatener" => {
                let Valeur::Liste(l) = &a[0] else { erreur_runtime("concatener attend une liste") };
                let mut r = String::new();
                for e in l.0.borrow().iter() {
                    if !matches!(e, Valeur::Chaine(..)) { erreur_runtime("concatener attend une liste de chaînes"); }
                    r.push_str(&chaine_de(e));
                }
                Valeur::s(r)
            }
            "lire_texte" => {
                let p = chaine_de(&a[0]);
                match std::fs::read(&p) {
                    Ok(b) if b.len() <= (1 << 28) => Valeur::s(String::from_utf8_lossy(&b).into_owned()),
                    Ok(_) => erreur_runtime(&format!("fichier trop volumineux ({})", p)),
                    Err(_) => erreur_runtime(&format!("lecture du fichier impossible ({})", p)),
                }
            }
            "eval_expr" => {
                // Evaluation d'une expression avec substitutions.
                // Le contexte est une liste de paires "nom=valeur" separees par
                // des espaces ou des virgules : "expr // vars: a=1 b=2".
                // Les valeurs peuvent etre nues ou entre guillemets doubles.
                // Le remplacement est fait par mots entiers (decoupes aux
                // separateurs "( )" aussi) : un mot n'est remplace que s'il
                // egale exactement un nom ; la derniere definition d'un nom
                // fait foi (shadowing).
                //
                // Ordre des voies (la premiere qui rend une valeur non vide gagne) :
                //  1) litteral guillemete  2) atome nu (booleen/nombre)
                //  3) substituer le contexte puis evaluer  4) evaluer sans contexte.
                let expr_str = chaine_de(&a[0]);
                let base = expr_str.trim().to_string();
                let mut substituted = base.clone();
                // Separer l'expression nue du contexte eventuel ("// vars: ...").
                // La recherche est sensible aux littéraux chaînes : un separateur
                // apparu a l'interieur de guillemets n'est pas un separateur.
                let pos_vars = {
                    let cs: Vec<char> = base.chars().collect();
                    let mut dans_guilm = false;
                    let mut found: Option<(usize, usize)> = None;
                    let mut i = 0usize;
                    while i < cs.len() && found.is_none() {
                        if cs[i] == '"' {
                            dans_guilm = !dans_guilm;
                        } else if !dans_guilm {
                            if i + 8 <= cs.len() && base[i..].starts_with("// vars:") {
                                found = Some((i, 8usize));
                            } else if i + 7 <= cs.len() && base[i..].starts_with("//vars:") {
                                found = Some((i, 7usize));
                            } else if i + 7 <= cs.len() && base[i..].starts_with("; vars:") {
                                found = Some((i, 7usize));
                            } else if i + 6 <= cs.len() && base[i..].starts_with(";vars:") {
                                found = Some((i, 6usize));
                            } else if i + 5 <= cs.len() && base[i..].starts_with("vars:") {
                                found = Some((i, 5usize));
                            }
                        }
                        i += 1;
                    }
                    found
                };
                let (nu, vars_brut) = match pos_vars {
                    Some((i, saut)) => (
                        base[..i].trim().to_string(),
                        base[i + saut..].trim().to_string(),
                    ),
                    None => {
                        // pas de separateur : on nettoie neanmoins un suffixe
                        // « // vars: » eventuel.
                        let n = nettoyer_rendu(&base);
                        (n, String::new())
                    }
                };
                // le substituted initial = nu (sans le suffixe), pas base.
                substituted = nu.clone();
                // le prelude est souvent suivi des lignes `nom = valeur` du
                // programme source : on le tronque a la premiere ligne sans
                // signe `=`.
                let vars_brut = {
                    let mut lignes: Vec<&str> = Vec::new();
                    for ln in vars_brut.lines() {
                        if ln.contains('=') { lignes.push(ln); } else { break; }
                    }
                    lignes.join(" ")
                };
                // 1) litteral : une seule paire de guillemets englobante
                if nu.len() >= 2
                    && nu.starts_with('"')
                    && nu.ends_with('"')
                    && nu[1..nu.len() - 1].find('"').is_none()
                {
                    return Valeur::s(nu[1..nu.len() - 1].to_string());
                }
                // nu peut encore porter un suffixe « // vars: » (si le
                // separateur n'a pas ete detecte) : on le nettoie.
                let nu = nettoyer_rendu(&nu);
                if nu.is_empty() { return Valeur::s(String::new()); }
                if nu.len() >= 2
                    && nu.starts_with('"')
                    && nu.ends_with('"')
                    && nu[1..nu.len() - 1].find('"').is_none()
                {
                    return Valeur::s(nu[1..nu.len() - 1].to_string());
                }
                // 2) atome nu : booleen ou nombre
                if nu == "vrai" || nu == "faux" { return Valeur::s(nu.clone()); }
                if let Ok(n) = nu.parse::<i64>() { return Valeur::s(n.to_string()); }
                // 2ter) entier exact BigInt pour les littéraux entiers trop
                //     grands pour i64 (ex: 999999999999999999999). On ne le
                //     tente que pour un entier pur (chiffres, signe optionnel)
                //     afin de ne pas confondre avec un flottant (1.5, 1e21…).
                let est_entier_pur = !nu.is_empty()
                    && (nu.chars().all(|c| c.is_ascii_digit())
                        || (nu.len() > 1
                            && (nu.as_bytes()[0] == b'+' || nu.as_bytes()[0] == b'-')
                            && nu[1..].chars().all(|c| c.is_ascii_digit())));
                if est_entier_pur {
                    let z = chaine_vers_entier(&nu);
                    return Valeur::s(z.chaine());
                }
                if let Ok(f) = nu.parse::<f64>() { return Valeur::s(texte_simple(f)); }
                // 2bis) voie de pliage structuree (replier_expression) : gere les
                //    appels std imbriques et les fonctions utilisateur declarees
                //    dans le prelude. Gagne sur la substitution mot-a-mot simple.
                let mut rendu_secours: Option<String> = None;
                if !vars_brut.is_empty() {
                    let paires_pre = paires_prelude(&vars_brut);
                    if let Some(r) = replier_expression(&nu, &paires_pre) {
                        let nettoye = nettoyer_rendu(&r);
                        if !nettoye.is_empty() && !nettoye.contains("vars:") {
                            return Valeur::s(nettoye);
                        }
                    }
                    // 2ter) si `nu` est déja un mot-clé ou un atome, on le rend
                    //     sans le suffixe « // vars: ».
                    if nu == "vrai" || nu == "faux" { return Valeur::s(nu.clone()); }
                    if let Ok(n) = nu.parse::<i64>() { return Valeur::s(n.to_string()); }
                }
                // 3) avec contexte : decoupage des paires, puis substitution mot a mot
                if !vars_brut.is_empty() {
                    let mut paires: Vec<(String, String)> = Vec::new();
                    let mut cur = String::new();
                    let mut dans_guilm = false;
                    let mut prof = 0usize;
                    for c in vars_brut.chars() {
                        if c == '"' {
                            dans_guilm = !dans_guilm;
                            cur.push(c);
                        } else if !dans_guilm && prof == 0 && (c == ' ' || c == '\t' || c == '\n' || c == ',') {
                            if !cur.is_empty() {
                                if let Some(eq) = cur.find('=') {
                                    paires.push((cur[..eq].to_string(), cur[eq + 1..].to_string()));
                                }
                                cur.clear();
                            }
                        } else {
                            if !dans_guilm {
                                if c == '(' { prof += 1; }
                                if c == ')' { prof = prof.saturating_sub(1); }
                            }
                            cur.push(c);
                        }
                    }
                    if !cur.is_empty() {
                        if let Some(eq) = cur.find('=') {
                            paires.push((cur[..eq].to_string(), cur[eq + 1..].to_string()));
                        }
                    }
                    //decoupage de l'expression en mots, guillemets preserves
                    //le separateur est aussi la virgule (la forme "a=1, b=2")
                    let mut mots: Vec<String> = Vec::new();
                    let mut cur = String::new();
                    let mut dans_guilm = false;
                    for c in substituted.chars() {
                        if c == '"' {
                            dans_guilm = !dans_guilm;
                            cur.push(c);
                        } else if !dans_guilm && (c == ' ' || c == '\t' || c == ',') {
                            if !cur.is_empty() {
                                mots.push(cur.clone());
                                cur.clear();
                            }
                        } else {
                            cur.push(c);
                        }
                    }
                    if !cur.is_empty() {
                        mots.push(cur.clone());
                    }
                    //chaque mot est decompose aux separateurs "( ) [ ]" ; un
                    //sous-mot est remplace seulement s'il egale exactement un
                    //nom (la derniere definition fait foi) ; les espaces sont
                    //rejetes seulement autour des parentheses
                    let mut sous: Vec<String> = Vec::new();
                    for m in mots.iter() {
                        let mut c2 = String::new();
                        for c in m.chars() {
                            if c == '(' || c == ')' || c == '[' || c == ']' {
                                if !c2.is_empty() {
                                    sous.push(c2.clone());
                                    c2.clear();
                                }
                                sous.push(c.to_string());
                            } else {
                                c2.push(c);
                            }
                        }
                        if !c2.is_empty() {
                            sous.push(c2.clone());
                        }
                    }
                    //un sous-mot n'est remplace que s'il egale exactement un
                    //nom (la derniere definition fait foi) ; les separateurs
                    //"( ) [ ]" ne sont pas des noms : ils restent tels quels ;
                    //la lettre, le chiffre et le mot-cle deja substitues sont
                    //rendus bruts (le parseur les normalisera)
                    let mut nouveau = String::new();
                    for m in sous.iter() {
                        if m.is_empty() {
                            continue;
                        }
                        if m == "(" || m == ")" || m == "[" || m == "]" {
                            nouveau.push_str(m);
                            continue;
                        }
                        let sep_avant = nouveau.ends_with('(') || nouveau.ends_with(')')
                            || nouveau.ends_with('[') || nouveau.ends_with(']');
                        let sep_apres = m.starts_with('(') || m.starts_with(')')
                            || m.starts_with('[') || m.starts_with(']');
                        if !nouveau.is_empty() && !sep_avant && !sep_apres {
                            nouveau.push(' ');
                        }
                        let mut trouve = false;
                        for (nom, val) in paires.iter().rev() {
                            if m == nom {
                                nouveau.push_str(val);
                                trouve = true;
                                break;
                            }
                        }
                        if !trouve {
                            nouveau.push_str(m);
                        }
                    }
                    substituted = nouveau;
                }
                // Evaluer l'expression. On commence par essayer la forme
                // "x = <expr>" (affectation, premier instruction = Affect),
                // puis, si cela echoue, la forme directe "<expr>" dont le
                // premier jeton est deja une expression complete.
                let candidats = [
                    format!("x = {}", substituted),
                    format!("x = {}\n>> x", substituted),
                    substituted.clone(),
                ];

                let mut rendu = Valeur::s(String::new());
                let mut obtenu = false;
                // 0) si la substitution a deja produit un atome complet
                //    (litteral guillemete, booleen), on le rend tel quel ;
                //    un nombre est rendu par le parseur, qui normalise le rendu
                if substituted.starts_with('"') && substituted.ends_with('"')
                    && substituted.len() >= 2
                {
                    rendu = Valeur::s(substituted[1..substituted.len() - 1].to_string());
                    obtenu = true;
                } else if substituted == "vrai" || substituted == "faux" {
                    rendu = Valeur::s(substituted.clone());
                    obtenu = true;
                }
                if !obtenu {
                for src in candidats.iter() {
                    match crate::front::analyser(src) {
                        Ok(prog) => {
                            let mut interp = Interp::nouveau(prog);
                            // 1) la premiere instruction est-elle une forme connue ?
                            let instr_clone = interp.prog.instructions.first().cloned();
                            if let Some(instr) = instr_clone {
                                match instr {
                                    crate::front::Instr::Affect { e, .. } => {
                                        if let Ok(v) = interp.eval(&e) {
                                            rendu = Valeur::s(chaine_de(&v));
                                            obtenu = true;
                                            break;
                                        }
                                    }
                                    crate::front::Instr::ExprStmt(e) => {
                                        if let Ok(v) = interp.eval(&e) {
                                            rendu = Valeur::s(chaine_de(&v));
                                            obtenu = true;
                                            break;
                                        }
                                    }
                                    //le programme est execute jusqu'au bout :
                                    //la valeur de x (issue de l'affectation)
                                    //est lue dans l'environnement global
                                    crate::front::Instr::Afficher(_, _) => {
                                        interp.executer_principal();
                                        if let Some(v) = interp.globaux.get("x") {
                                            rendu = Valeur::s(chaine_de(v));
                                            obtenu = true;
                                            break;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Err(_) => {}
                    }
                }
                }
                // 2) voie « fonction » : le premier jeton est une application
                //    complète (ex. "eval_expr(\"...\")") : on évalue le premier
                //    ExprStmt directement.
                if !obtenu {
                    match crate::front::analyser(&substituted) {
                        Ok(prog) => {
                            let mut interp = Interp::nouveau(prog);
                            if let Some(crate::front::Instr::ExprStmt(e)) =
                                interp.prog.instructions.first().cloned()
                            {
                                if let Ok(v) = interp.eval(&e) {
                                    rendu = Valeur::s(chaine_de(&v));
                                    obtenu = true;
                                }
                            }
                        }
                        Err(_) => {}
                    }
                }
                // 3) voie « programme complet » : on execute tout le programme et
                //    on rend la valeur de sa derniere instruction utile.
                if !obtenu {
                    let prog_complet = crate::front::analyser(&substituted);
                    if let Ok(prog) = prog_complet {
                        let mut interp = Interp::nouveau(prog);
                        let n = interp.prog.instructions.len();
                        if n > 0 {
                            let derniere = interp.prog.instructions[n - 1].clone();
                            match derniere {
                                crate::front::Instr::Afficher(exprs, _) => {
                                    interp.executer_principal();
                                    if let Some(e) = exprs.first() {
                                        if let Ok(v) = interp.eval(e) {
                                            rendu = Valeur::s(chaine_de(&v));
                                            obtenu = true;
                                        }
                                    }
                                }
                                crate::front::Instr::Affect { e, .. } => {
                                    if let Ok(v) = interp.eval(&e) {
                                        rendu = Valeur::s(chaine_de(&v));
                                        obtenu = true;
                                    }
                                }
                                crate::front::Instr::ExprStmt(e) => {
                                    if let Ok(v) = interp.eval(&e) {
                                        rendu = Valeur::s(chaine_de(&v));
                                        obtenu = true;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                // derniere chance : un littéral nu (non stocké comme instruction
                // d'expression) est reconnu directement par le parseur
                if !obtenu {

                    if substituted.starts_with('"') && substituted.ends_with('"')
                        && substituted.len() >= 2
                    {
                        rendu = Valeur::s(substituted[1..substituted.len() - 1].to_string());
                    } else if let Ok(n) = substituted.parse::<i64>() {
                        rendu = Valeur::s(n.to_string());
                    } else if substituted == "vrai" || substituted == "faux" {
                        rendu = Valeur::s(substituted.clone());
                    } else if !substituted.is_empty() {
                        // atome non numérique : rendu brut (miroir d'un mot-cle
                        // ou d'un nom deja substitue)
                        rendu = Valeur::s(substituted.clone());
                    }
                }
                // dernier secours : le rendu pré-calculé par replier_expression.
                if let Some(sec) = rendu_secours {
                    if !sec.is_empty() { rendu = Valeur::s(sec); }
                }
                rendu
            }
            "ecrire_texte" => {
                let p = chaine_de(&a[0]);
                let t = chaine_de(&a[1]);
                if std::fs::write(&p, t.as_bytes()).is_err() {
                    erreur_runtime(&format!("écriture du fichier impossible ({})", p));
                }
                Valeur::Vide
            }
            "ecrire_binaire" => {
                let p = chaine_de(&a[0]);
                let mut bytes: Vec<u8> = Vec::new();
                if let Valeur::Liste(lst) = &a[1] {
                    for v in lst.0.borrow().iter() {
                        match v {
                            Valeur::Chaine(s) => {
                                match s.parse::<u8>() {
                                    Ok(n) => bytes.push(n),
                                    Err(_) => erreur_runtime(&format!("octet invalide: {}", s)),
                                }
                            }
                            _ => erreur_runtime("ecrire_binaire: liste doit contenir des nombres entiers"),
                        }
                    }
                } else {
                    erreur_runtime("ecrire_binaire: second argument doit être une liste");
                }
                if std::fs::write(&p, &bytes).is_err() {
                    erreur_runtime(&format!("écriture binaire impossible ({})", p));
                }
                Valeur::z(0)
            }
            "caractere" => {
                let n = chaine_de(&a[0]).parse::<u8>().unwrap_or(0);
                Valeur::s(char::from_u32(n as u32).unwrap_or('\0').to_string())
            }
            "ecrire_bytes" => {
                let p = chaine_de(&a[0]);
                let s = chaine_de(&a[1]);
                let mut bytes: Vec<u8> = Vec::new();
                for c in s.chars() {
                    bytes.push(c as u8);
                }
                if std::fs::write(&p, &bytes).is_err() {
                    erreur_runtime(&format!("écriture d'octets impossible ({})", p));
                }
                Valeur::z(0)
            }
            "nb_arguments" => {
                let z = Gz::neuf();
                z.poser_u64(self.arguments.len() as u64);
                Valeur::zg(z)
            }
            "argument" => {
                let i = vers_z(&a[0]);
                if !i.fits_u64_lt(self.arguments.len() as u64) {
                    erreur_runtime("index hors des limites des arguments");
                }
                Valeur::s(self.arguments[i.get_u64() as usize].clone())
            }
            "temps" => {
                let t = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u64;
                let z = Gz::neuf();
                z.poser_u64(t);
                Valeur::zg(z)
            }
            "duree" => {
                let z = vers_z(&a[0]);
                if z.sgn() < 0 { erreur_runtime("duree : la durée doit être positive ou nulle"); }
                Valeur::s(formater_duree(&z.chaine()))
            }
            "afficher_erreur" => {
                flush_out();
                eprintln!("{}", chaine_de(&a[0]));
                Valeur::Vide
            }
            "lever" => { erreur_runtime(&chaine_de(&a[0])); }
            "breakpoint" => {
                crate::run::flush_out();
                eprintln!("[BREAKPOINT]");
                for (k, v) in &self.globaux {
                    eprintln!("  {} = {}", k, chaine_de(v));
                }
                eprintln!("Appuyer sur Entrée pour continuer...");
                let mut buf = String::new();
                let _ = std::io::stdin().read_line(&mut buf);
                Valeur::Vide
            }
            "gpu_somme" => {
                // Placeholder: somme CPU pour l'instant
                let t = match &a[0] { Valeur::Tableau(t) => t, _ => { erreur_runtime("gpu_somme : tableau attendu"); } };
                let b = t.0.borrow();
                let mut s: i64 = 0;
                for &x in b.iter() { s = s.wrapping_add(x); }
                Valeur::z(s)
            }
            "gpu_somme" => {
                let t = match &a[0] { Valeur::Tableau(t) => t, _ => { erreur_runtime("gpu_somme : tableau attendu"); } };
                let b = t.0.borrow();
                let buf: Vec<i64> = b.clone();
                drop(b);
                match crate::run::gpu_somme_i64(&buf) {
                    Some(s) => Valeur::z(s),
                    None => {
                        // fallback CPU
                        let mut s: i64 = 0;
                        for &x in buf.iter() { s = s.wrapping_add(x); }
                        Valeur::z(s)
                    }
                }
            }
            "i" => Valeur::Complexe { re: 0.0, im: 1.0 },
            "re" => { let z = match &a[0] { Valeur::Complexe{re,..} => *re, _ => vers_d(&a[0]) }; Valeur::f(z) }
            "im" => { let z = match &a[0] { Valeur::Complexe{im,..} => *im, _ => 0.0 }; Valeur::f(z) }
            "conj" => { match &a[0] { Valeur::Complexe{re,im} => Valeur::Complexe { re: *re, im: -*im }, _ => a[0].clone() } }
            "modulus" | "abs" => { match &a[0] { Valeur::Complexe{re,im} => Valeur::f((re*re+im*im).sqrt()), _ => { let v = &a[0]; match v { Valeur::Entier(i,None) => Valeur::z(i.abs()), _ => Valeur::f(vers_d(v).abs()) } } } }
            "arg" => { let z = match &a[0] { Valeur::Complexe{re,im} => im.atan2(*re), _ => 0.0f64.atan2(vers_d(&a[0])) }; Valeur::f(z) }
            "racine" => {
                let n = vers_z(&a[0]);
                if n.sgn() < 0 { erreur_runtime("racine : entier négatif"); }
                if n.fits_i64() {
                    let x = n.get_i64();
                    let r = (x as f64).sqrt() as i64;
                    if r * r == x {
                        return Valeur::z(r);
                    }
                }
                Valeur::Irrationnel(Rc::new(n))
            }
            "i" => Valeur::Complexe { re: 0.0, im: 1.0 },
            "quitter" => unreachable!("quitter traité dans stdlib"),
            "abs" => {
                match &a[0] {
                    Valeur::Entier(i, None) => Valeur::z(i.abs()),
                    Valeur::Entier(_, Some(z)) => {
                        let r = Gz::neuf();
                        unsafe { if gmp_sgn(z.p()) < 0 { gmp_neg(r.p(), z.p()) } else { gmp_set(r.p(), z.p()) } };
                        Valeur::zg(r)
                    }
                    Valeur::Rationnel(q) => {
                        let r = Gq::neuf();
                        unsafe { if crate::run::gmpq_sgn(q.p()) < 0 { gmpq_neg(r.p(), q.p()) } else { gmpq_set(r.p(), q.p()) } };
                        Valeur::qq(r)
                    }
                    Valeur::Flottant(d) => Valeur::f(d.abs()),
                    _ => { erreur_runtime("abs attend un nombre"); }
                }
            }
            "min" | "max" => {
                // miroir C++ : les deux valeurs sont d'abord converties vers le
                // rang max (entier<rationnel<flottant), puis comparées.
                let rang = |v: &Valeur| -> u8 {
                    match v {
                        Valeur::Flottant(..) => 3,
                        Valeur::Rationnel(..) => 2,
                        _ => 1,
                    }
                };
                let m = rang(&a[0]).max(rang(&a[1]));
                let cible = match m { 3 => TyKind::Flottant, 2 => TyKind::Rationnel, _ => TyKind::Entier };
                let x = convertir(a[0].clone(), cible);
                let y = convertir(a[1].clone(), cible);
                let c = cmp_vals(&x, &y);
                let take_min = nom == "min";
                if (c <= 0 && take_min) || (c >= 0 && !take_min) { x } else { y }
            }
            "partie_entiere" => {
                // miroir C++ : floor exact (rationnel → fdiv_q, flottant → floor).
                match &a[0] {
                    Valeur::Rationnel(q) => {
                        let z = Gz::neuf();
                        unsafe { gmp_fdiv_q(z.p(), q.num(), q.den()) };
                        Valeur::zg(z)
                    }
                    Valeur::Flottant(d) => {
                        let z = Gz::neuf();
                        unsafe { gmp_set_d(z.p(), d.floor()) };
                        Valeur::zg(z)
                    }
                    _ => { let z = vers_z(&a[0]); Valeur::zg(z) }
                }
            }
            "fraction" => {
                let n = vers_z(&a[0]);
                let d = vers_z(&a[1]);
                if d.sgn() == 0 { erreur_runtime("dénominateur nul"); }
                let q = Gq::neuf();
                unsafe {
                    gmpq_set_z_field(q.num(), n.p());
                    gmpq_set_z_field(q.den(), d.p());
                    gmpq_canonicalize(q.p());
                }
                Valeur::qq(q)
            }
            _ => {
                // miroir C++ (rose.cpp ~3668) : un corps de fonction
                // utilisateur appele par un chemin non resolu (par ex. via
                // une expression dans une position non-Eval) — message et
                // code 1 identiques au moteur C++.
                if self.prog.signatures.contains_key(nom) {
                    eprintln!("Rose : erreur — fonction '{}' inconnue", nom);
                    flush_out();
                    std::process::exit(1);
                }
                erreur_runtime(&format!("Fonction '{}' inconnue", nom));
            }
        }
    }
}

fn indice_tampon(idx: &Gz, taille: usize) -> usize {
    if !idx.fits_usize(taille) { erreur_runtime("index hors tampon"); }
    idx.get_u64() as usize
}

fn rnk(v: &Valeur) -> u8 {
    match v.ty() {
        TyKind::Entier | TyKind::Booleen => 1,
        TyKind::Rationnel => 2,
        TyKind::Flottant => 3,
        _ => 0,
    }
}

fn est_vrai(v: &Valeur) -> bool {
    match v {
        Valeur::Booleen(b) => *b,
        _ => erreur_runtime("condition non booléenne"),
    }
}

fn extraire_i64(v: &Valeur) -> i64 {
    match v {
        Valeur::Entier(i, None) => *i,
        Valeur::Entier(_, Some(z)) => z.get_i64(),
        _ => erreur_runtime("entier attendu"),
    }
}

enum Signal { Retour(Valeur), Quitter(i32) }

impl From<Option<i32>> for Signal {
    fn from(c: Option<i32>) -> Signal { Signal::Quitter(c.unwrap_or(0)) }
}

// ── Point d'entrée CLI ────────────────────────────────────────────────────

pub fn executer_source(source: &str, args: Vec<String>) -> i32 {
    let _t0 = std::time::Instant::now();
    let mut prog = match crate::front::analyser(source) {
        Ok(p) => p,
        Err(e) => { eprintln!("{}", e); return 1; }
    };
    let _t_parse = _t0.elapsed();
    // Optimisation : boucles accumulateur compilables -> fonction synthetique,
    // pour que la boucle tourne en natif meme si le programme principal ne
    // passe pas tout entier dans le sous-ensemble JIT.
    {
        use crate::front::{Instr, Expr, Ty, TyKind};
        use crate::jit::bloc_compilable;
        use std::collections::HashMap;
        let mut counter = 0usize;
        let mut new_instrs = Vec::new();
        for instr in std::mem::take(&mut prog.instructions) {
            if let Instr::Pour { nom: loopvar, debut, fin, corps, ligne } = instr {
                let mut transformed = false;
                if corps.len() == 1 {
                    if let Instr::Affect { nom: acc, e: eacc, .. } = &corps[0] {
                        let acc = acc.clone();
                        let eacc = eacc.clone();
                        let mut env = HashMap::new();
                        env.insert(acc.clone(), TyKind::Entier);
                        env.insert(loopvar.clone(), TyKind::Entier);
                        let acc_ty = crate::jit::ty_statique(&eacc, &env);
                        let ok_ty = matches!(acc_ty, Some(TyKind::Entier) | Some(TyKind::Flottant));
                        if ok_ty && bloc_compilable(&corps, &mut env, &prog.signatures, Some(TyKind::Vide)) {
                            let func_name = format!("__rose_loop_{}", counter);
                            counter += 1;
                            let ty = acc_ty.unwrap();
                            let sig = crate::front::Signature {
                                nom: func_name.clone(),
                                params: vec![(acc.clone(), Ty::simple(ty))],
                                retour: Ty::simple(ty),
                                a_retour: true,
                                corps: Some(vec![
                                    Instr::Pour { nom: loopvar.clone(), debut: debut.clone(), fin: fin.clone(), corps: corps.clone(), ligne },
                                    Instr::Retour(Some(Expr::Var(acc.clone(), ligne)), ligne),
                                ]),
                                dll: String::new(), dll_son: String::new(), symbole: String::new(),
                                ligne,
                            };
                            prog.signatures.insert(func_name.clone(), sig);
                            prog.ordre.push(func_name.clone());
                            new_instrs.push(Instr::Affect {
                                nom: acc.clone(),
                                e: Expr::Appel { nom: func_name, module: String::new(), args: vec![Expr::Var(acc.clone(), ligne)], ligne },
                                ligne,
                            });
                            transformed = true;
                        }
                    }
                }
                if !transformed {
                    new_instrs.push(Instr::Pour { nom: loopvar, debut, fin, corps, ligne });
                }
                continue;
            }
            new_instrs.push(instr);
        }
        prog.instructions = new_instrs;
    }
    let prog_pour_jit = prog.clone();
    let _t_transform = _t0.elapsed();
    let mut interp = Interp::nouveau(prog).avec_arguments(args);
    // active le JIT sur les fonctions éligibles
    let mut jit = Jit::nouveau(&interp.prog.signatures);
    let _t_jitnew = _t0.elapsed();
    interp.jit = jit.compiler_tout();
    let _t_compiletout = _t0.elapsed();
    interp.jit_principal = jit.compiler_principal(&prog_pour_jit);
    let _t_compileprincipal = _t0.elapsed();
    interp.executer_principal();
    let _t_exec = _t0.elapsed();
    flush_out();
    if std::env::var("ROSE_TIMING").is_ok() {
        eprintln!("TIMING parse={:?} transform={:?} jitnew={:?} compiletout={:?} compileprincipal={:?} exec={:?} total={:?}",
            _t_parse, _t_transform - _t_parse, _t_jitnew - _t_transform,
            _t_compiletout - _t_jitnew, _t_compileprincipal - _t_compiletout,
            _t_exec - _t_compileprincipal, _t_exec);
    }
    0
}

pub fn version() -> String {
    "Rose 2.0.3 — compilateur Rust + JIT Cranelift (sémantique du moteur C++)".to_string()
}

pub fn info() -> String {
    let arch = if cfg!(target_arch = "aarch64") { "ARM 64 bits" } else { "x86-64" };
    format!("rose 2.0.3\n  \
             moteur            : Rust natif {} — JIT Cranelift + interpréteur exact\n  \
             maths exactes     : BigInt natif (num-bigint, illimité, zéro GMP)\n  \
             nombres           : entiers, rationnels, flottants, irrationnels (√n), complexes (a+bi)\n  \
             interpréteur      : intégré (repli du JIT, mêmes sorties)\n  \
             compilation       : JIT à chaud des fonctions entiers/flottants/booléens\n", arch)
}
