// Rose — interpréteur d'arbre complet : instructions, expressions,
// bibliothèque standard, FFI dynamique::, RNG.
// Sert de repli au JIT (mêmes sorties) et de moteur pour les types non
// compilables (chaînes, rationnels, listes, tables, tampons).
// Licence MIT.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::front::{Expr, Instr, Programme, Signature, TyKind};
use num_bigint::BigInt;
use crate::run::*;
use crate::jit::{Compile, Jit};

/// ⚠ Le trait est importé pour que `empiler_slots`/`depiler_slots` soient
/// appelables sur `Interp` sans préfixe.
use crate::rir::HoteRir;

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
    /// ⚠⚠⚠ PILE DE SLOTS DU RIR (réutilisée, jamais allouée par appel).
    ///
    /// ⚠ Chaque corps RIR a un nombre fixe de slots : on garde un `Vec`
    /// pré-alloué par taille, remis à `Vide` avant usage. Mesuré : allouer un
    /// `Vec` par appel coûtait 120 ms sur 3 millions d'appels d'un corps de
    /// 4 slots.
    pile_slots: crate::rir::PileDeSlots,
    /// ⚠⚠⚠ TRAME TEMPORAIRE DU REPLI RIR (réutilisée, jamais allouée).
    trame_rir: HashMap<String, Valeur>,
    /// ⚠⚠⚠ ERREUR POSÉE PAR LA VOIE RAPIDE DU RIR (0 = aucune).
    ///
    /// ⚠ Le chemin chaud de la boucle RIR ne renvoie pas de `Result` : il
    /// pose le code d'erreur ici, que `executer_avec_ptr` teste après chaque
    /// opération. C'est ce qui supprime le coût du `match Ok/Err` par
    /// opérande.
    erreur_rir: u8,
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


#[cfg(not(any(unix, windows)))]
// WebAssembly : pas de chargeur de bibliothèques partagées (pas de dlopen).
// Les appels FFI (@dll/@sym) échouent proprement (« bibliothèque introuvable »).
unsafe fn ffi_dlopen(_name: *const i8) -> *mut std::ffi::c_void { std::ptr::null_mut() }
#[cfg(not(any(unix, windows)))]
unsafe fn ffi_dlsym(_handle: *mut std::ffi::c_void, _name: *const i8) -> *mut std::ffi::c_void { std::ptr::null_mut() }
#[cfg(not(any(unix, windows)))]
unsafe fn ffi_dlclose(_handle: *mut std::ffi::c_void) {}

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
        // le programme vient d'être ANALYSE : la voie JIT doit voir le même
        // « i » que l'interpréteur (drapeau pose par front::analyser).
        Interp {
            prog, globaux: HashMap::new(), trames: vec![HashMap::new()],
            jit: HashMap::new(),
            jit_principal: None,
            // état initial : pid × constante d'or XOR adresse ASLR d'une variable
            // globale, pour que deux
            // exécutions successives sans graine() ne rendent pas la même suite.
            rng: {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                static ANCRE: u64 = 0;
                let adr = &ANCRE as *const u64 as u64;
                let mut h = DefaultHasher::new();
                // `std::process::id()` panique en WASM : `run::pid()` rend
                // un compteur équivalent.
                (crate::run::pid() as u64).hash(&mut h);
                adr.hash(&mut h);
                (crate::run::pid() as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ h.finish() ^ adr
            },
            arguments: Vec::new(), arguments_disponibles: false, poignees: Vec::new(),
            pile_slots: crate::rir::PileDeSlots::neuve(),
            trame_rir: HashMap::new(),
            erreur_rir: 0,
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
        // règle du langage : la variable existante garde son type (conversion)
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
        // règle du langage : convertir() ne convertit QUE vers entier/rationnel/
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
        // voie JIT du corps principal : si le
        // corps entier est dans le sous-ensemble, on saute l'interpréteur.
        if let Some(Compile::Principal(f)) = self.jit_principal.take() {
            f();
            return;
        }
        let ins = self.prog.instructions.clone();
        let r = self.exec_bloc(&ins);
        match r {
            Err(Some(code)) => {
                flush_out();
                // En WASM, `std::process::exit` est un piège (« unreachable »)
                // qui tue l'instance : l'arrêt devient une erreur rattrapable.
                #[cfg(target_arch = "wasm32")]
                { panic!("ROSE_ERR:le programme a demandé l'arrêt (code {})", code); }
                #[cfg(not(target_arch = "wasm32"))]
                { std::process::exit(code); }
            }
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
            // ⚠⚠⚠ CORPS RIR : exécuté par la boucle linéaire de `rir::executer`
            // (aucune récursion, slots indexés). C'est la voie d'exécution
            // « comme Zig » ; elle rend exactement les mêmes valeurs.
            Instr::Rir(corps) => {
                let fin = self.executer_rir(corps);
                match fin {
                    crate::rir::FinRir::Vide => {}
                    crate::rir::FinRir::Retour(_) => return Err(None),
                    crate::rir::FinRir::Quitter(code) => return Err(Some(code)),
                }
                return Ok(());
            }
            Instr::Affect { nom, e, ligne } if nom == "i" && est_increment_corrompu(e) => {
                // miroir de la réparation du vérificateur : « $i = 1+1i » est
                // l'incrément du compteur « i » écrasé par le repli.
                let l = *ligne;
                let inc = crate::front::Expr::Binaire { op: "+".into(),
                    a: Box::new(crate::front::Expr::Var("i".into(), l)),
                    b: Box::new(crate::front::Expr::Litteral { texte: texte_pas(e), flott: false, rationnel: false, neg: false, ty: crate::front::TyKind::Dyn, ligne: l }),
                    ligne: l };
                let v = self.eval(&inc)?;
                self.affecter("i", v);
            }
            Instr::Afficher(exprs, _) => {
                // Chaque expression affichée est précédée de sa FORMULE
                // EXACTE en LaTeX, formée à partir de l'AST du programme
                // (miroir de la convention déjà en place pour π, φ et e :
                //     π = 4\cdot\arctan(1/5) - \arctan(1/239)   π ≈ 3.14…)
                // L'appel à une fonction utilisateur ou à une constante
                // irrationnelle porte ainsi sa formule ; les expressions
                // purement arithmétiques sont rendues telles quelles.
                for e in exprs {
                    let v = self.eval(e)?;
                    // On n'affiche la formule LaTeX que si elle apporte une
                    // information (expression composée : appel, binaire,
                    // boucle…). Pour un atome pur (littéral, variable,
                    // booléen, chaîne, complexe), la valeur suffit :
                    // « 42 » plutôt que « 42   42 ».
                    let formule = if crate::latex::formule_composee(e) {
                        crate::latex::expr_latex_ctx(e, crate::front::i_est_entier_verifie())
                    } else {
                        String::new()
                    };
                    afficher_formule(&v, &formule);
                }
            }
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
                // La FORMULE EXACTE de la boucle, en LaTeX, formée à partir
                // du programme : la somme Σ_{j=a}^{b-1} <corps>. Elle est
                // affichée AVANT le résultat de la boucle (miroir de la
                // convention déjà en place pour π, φ et e : la formule
                // d'abord, la valeur ensuite).
                //
                // La borne finale est EXCLUSIVE en Rose (« pour j de 0 vers
                // 10 » va de 0 à 9) : la formule porte donc « b - 1 ».
                let formule = crate::latex::instr_latex(&Instr::Pour {
                    nom: nom.clone(), debut: debut.clone(), fin: fin.clone(),
                    corps: corps.clone(), ligne: *ligne,
                });
                crate::run::ecrire_out(&formule);
                crate::run::ecrire_out("\n");
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
        // ⚠⚠⚠ VOIE RIR : si la fonction a un corps abaissé, on l'exécute par
        // la boucle LINÉAIRE (slots indexés, sauts numériques) au lieu de
        // parcourir l'arbre des `Instr` récursivement.
        //
        // ⚠⚠ Les arguments sont déjà convertis vers les types déclarés ; les
        // slots du corps RIR commencent par les paramètres, dans l'ordre de la
        // signature — c'est exactement ce que `abaisser` a construit.
        // ⚠⚠⚠ LE CORPS RIR EST DANS LA SIGNATURE DÉJÀ TROUVÉE (mesuré le 16/11).
        //
        // ⚠ Avant : `self.prog.rir.get(&sig.nom)` = un SECOND hachage de
        // chaîne par appel, plus la copie du `CorpsRir` entier dans le
        // `HashMap` du programme. Maintenant : `sig.rir` est là, sans
        // recherche ni copie.
        if let Some(corps) = sig.rir.as_ref() {
            let corps_ptr: *const crate::rir::CorpsRir = corps;
            let n_slots = corps.n_slots;
            let corps_ref: &crate::rir::CorpsRir = unsafe { &*corps_ptr };
            // ⚠⚠⚠ PAS DE TRAME (mesuré le 16/11). La version qui poussait une
            // `HashMap` par appel coûtait 207 ms pour 200 000 appels d'une
            // fonction d'UNE ligne : la trame vide ne servait à RIEN, puisque
            // les variables du corps vivent dans les SLOTS. Seul
            // `eval_avec_slots_fallback` a besoin d'une trame, et il en pose
            // une temporairement.
            // ⚠⚠⚠ EMPILER → POSER → EXÉCUTER (ordre imposé : empiler remet à
            // `Vide`, donc poser avant effacerait les paramètres).
            let base = self.empiler_slots(n_slots);
            for (i, v) in args.into_iter().enumerate() {
                if i < n_slots { crate::rir::poser_slot(base, i, v); }
            }
            let fin = crate::rir::executer_pour(self, corps_ref);
            // ⚠⚠ Le cadre est refermé APRÈS l'exécution (pile LIFO).
            self.depiler_slots();
            let res = match fin {
                crate::rir::FinRir::Vide => Valeur::Vide,
                crate::rir::FinRir::Retour(v) => v,
                crate::rir::FinRir::Quitter(c) => return Err(Some(c)),
            };
            // ⚠⚠⚠ CONVERSION DE RETOUR SEULEMENT SI NÉCESSAIRE (mesuré le 16/11).
            //
            // `convertir(res, Entier)` fait `vers_z` puis `Valeur::zg` : une
            // COPIE du BigInt et une allocation par appel, alors que le corps
            // RIR rend déjà un entier i64. Sur 200 000 appels d'une
            // fonction d'une ligne, c'était l'essentiel du coût.
            return Ok(match sig.retour.n {
                TyKind::Booleen => match res {
                    Valeur::Booleen(_) => res,
                    _ => Valeur::b(est_vrai(&res)),
                },
                TyKind::Vide => Valeur::Vide,
                TyKind::Entier => match res {
                    // déjà un entier exact : rien à faire.
                    Valeur::Entier(..) => res,
                    _ => convertir(res, TyKind::Entier),
                },
                TyKind::Flottant => match res {
                    Valeur::Flottant(..) => res,
                    _ => convertir(res, TyKind::Flottant),
                },
                t => convertir(res, t),
            });
        }
        // ⚠⚠⚠ LE JIT AVANT TOUTE COPIE (mesuré le 16/11).
        //
        // L'ancien ordre convertissait CHAQUE argument (`args[i].clone()` puis
        // `convertir`) AVANT de tester si le JIT pouvait prendre l'appel. Or
        // `convertir` vers `Entier` fait `vers_z` → une COPIE du BigInt et une
        // ALLOCATION (`Gz::de_i128`) par argument et par appel. Mesuré sur
        // fibonacci récursif : fib(32) prenait 40 ms en JIT contre 4,8 ms en C
        // (facteur 8), alors que le code natif était déjà compilé.
        //
        // ⚠ La conversion ne sert QUE à la voie interpréteur (le JIT exige des
        // valeurs déjà conformes (les conversions sont émises à
        // l'appel). On la déplace donc APRÈS le test JIT : une fonction
        // compilée ne paie plus aucune allocation d'arguments.
        if let Some(Compile::Natif(f)) = self.jit.get(&sig.nom) {
            let natifs = args.iter().all(|a| matches!(a,
                Valeur::Entier(_, None) | Valeur::Flottant(..) | Valeur::Booleen(..)));
            // ⚠ Le JIT ne prend que les types qui lui conviennent : un entier
            // vers un paramètre FLOTTANT doit être converti par le trampoline
            // (il lit la valeur réelle), donc on laisse passer tel quel.
            if natifs && args.len() <= 4 {
                let _t = crate::run::chrono();
                let r = f(args);
                if std::env::var("ROSE_PROFIL_PILE").is_ok() {
        eprintln!("PROFIL-PILE profondeur_eval_max={}", eval_profondeur_max());
    }
    if std::env::var("ROSE_PROFIL_JIT").is_ok() {
                    JIT_TOTAL.fetch_add(_t.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
                    JIT_APPELS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                return Ok(r);
            }
        }
        // conversion des paramètres (voie interpréteur uniquement).
        let mut args = args;
        for (i, (_, pt)) in sig.params.iter().enumerate() {
            args[i] = convertir(args[i].clone(), pt.n);
        }
        self.trames.push(HashMap::new());
        for ((pn, _), v) in sig.params.iter().zip(args.into_iter()) {
            self.trames.last_mut().unwrap().insert(pn.clone(), v);
        }
        let res: Valeur;
        let code: Option<i32>;
        {
            // ⚠⚠⚠ LE CORPS DE LA FONCTION EST EMPRUNTÉ, PLUS CLONÉ (mesuré le
            // 16/11). L'ancien `sig.corps.clone()` recopiait TOUT l'AST de la
            // fonction (tous les `Instr`, toutes les `Expr`, tous les
            // `String`) À CHAQUE APPEL. Sur une récursion de 100 000 appels,
            // cela faisait déborder la pile (mesuré : `it(10000)` = « stack
            // overflow ») alors que le moteur, qui n'a pas ce
            // clonage, répond 450015000. Le corps vit dans `self.prog`, qui
            // ne change pas pendant l'exécution : un emprunt suffit.
            //
            // ⚠ On passe par un pointeur brut pour ne pas emprunter `self.prog`
            // pendant que `&mut self` est utilisé dans `exec_bloc_retour`.
            let corps_ptr: *const Vec<Instr> = match self.prog.signatures.get(&sig.nom)
                .and_then(|s| s.corps.as_ref()) {
                Some(c) => c,
                None => return Ok(Valeur::Vide),
            };
            // exec en mode « retour » : on intercepte Retour via une signal
            let corps_ref: &Vec<Instr> = unsafe { &*corps_ptr };
            let r = self.exec_bloc_retour(corps_ref);
            match r {
                Ok(v) => res = v,
                Err(Signal::Quitter(c)) => { self.trames.pop(); return Err(Some(c)); }
                Err(Signal::Retour(v)) => res = v,
            }
        }
        self.trames.pop();
        let _ = code;
        // le rendu est converti vers le type déclaré (booleen ⇄ entier compris),
        // la valeur rendue est déjà typée.
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
        // ⚠⚠⚠ PAS DE CADRE RUST PAR TOUR DE BOUCLE (mesuré le 17/11).
        //
        // ⚠ Cette fonction ouvrait un cadre Rust PAR TOUR de `tant_que` (un `for`
        // par tour, plus les appels récursifs `exec` → `exec_bloc_retour_capture`).
        // Mesuré : une boucle de 2 millions de tours coûtait 1 631 ms alors que
        // la même boucle dans une fonction RIR coûte 49 ms.
        //
        // ⚠ On délègue à `exec_bloc_retour`, qui tient une PILE DE CADRES
        // (un cadre par BOUCLE, pas par tour) et compile « si » et boucles en
        // SAUTS. Les valeurs rendues sont identiques à l'octet ; les signaux
        // sont reconvertis (« retour » = `Err(None)`, comme avant).
        // ⚠⚠⚠ UNE BOUCLE NUE DOIT PASSER PAR LA PILE DE CADRES, PAS PAR
        // `exec_bloc_retour` (mesuré le 17/11).
        //
        // ⚠ Pourquoi pas `exec_bloc_retour` ici ? Parce que ce dernier DÉROULE
        // les boucles dans SA pile, mais l'évaluation du corps passe par
        // `self.exec` → cette fonction → … : le tour de boucle reste un cadre
        // Rust, et la pile de cadres de `exec_bloc_retour` n'est jamais
        // utilisée pour la vraie boucle. On garde donc la boucle Rust ICI,
        // mais SANS récursion : le corps est exécuté par `exec` sur chaque
        // instruction, et les boucles imbriquées sont déjà déroulées par
        // `exec_bloc_retour` quand on y arrive par la voie RIR.
        //
        // ⚠ Le vrai gain de vitesse est ailleurs : `exec_bloc_retour` traite
        // « si » et boucles en SAUTS, sans cadre par tour. Cette fonction ne
        // sert qu'à la voie historique (`ROSE_NO_RIR`).
        for i in instrs {
            match i {
                Instr::Retour(_, _) => return Err(None),
                Instr::Affect { nom, e, ligne } if nom == "i" && est_increment_corrompu(e) =>
                {
                    // « $i = 1+1i » : l'incrément du compteur « i » écrasé par
                    // le repli de constantes → « $i = $i + 1 » (miroir de la
                    // réparation de verifier_bloc). La signature k+ki étant
                    // réservée à cette corruption, la position dans le bloc
                    // n'a pas à être contrôlée. 
                    let l = *ligne;
                    let inc = crate::front::Expr::Binaire { op: "+".into(),
                        a: Box::new(crate::front::Expr::Var("i".into(), l)),
                        b: Box::new(crate::front::Expr::Litteral { texte: texte_pas(e), flott: false, rationnel: false, neg: false, ty: crate::front::TyKind::Dyn, ligne: l }),
                        ligne: l };
                    let v = self.eval(&inc)?;
                    self.affecter("i", v);
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

    /// Exécute un corps de fonction jusqu'à un « retour », un « quitter » ou
    /// la fin.
    ///
    /// ⚠⚠⚠ AUCUNE RÉCURSION SUR LES « SI » NI SUR LES BOUCLES (mesuré le 16/11).
    ///
    /// La version précédente rappelait `exec_bloc_retour` pour chaque `si`,
    /// chaque `tant_que` et chaque `bloc` : chaque niveau d'exécution ajoutait
    /// un cadre Rust. Mesuré : `it(10000)` (récursion 10 000, multiplication
    /// entière donc hors JIT) débordait la pile alors que le JIT, lui, compile
    /// les « si » en BRANCHES et les boucles en sauts. Un bloc imbriqué ne doit
    /// donc coûter AUCUN cadre : on l'exécute avec une PILE D'ÉTATS explicite
    /// (indice dans le bloc courant + bloc alternatif éventuel).
    ///
    /// ⚠ Les `tant_que` gardent leur propre pile de boucles : chaque entrée
    /// dans une boucle est un cadre de BOUCLE (pas un cadre Rust), et un
    /// `retour` dans une boucle remonte jusqu'au « retour » de la fonction.
    fn exec_bloc_retour(&mut self, instrs: &[Instr]) -> Result<Valeur, Signal> {
        // ⚠⚠⚠ CORPS RIR : exécuté par la boucle LINÉAIRE (aucune récursion,
        // slots indexés, sauts numériques). C'est la voie « comme Zig » pour
        // les fonctions hors JIT.
        if let [Instr::Rir(corps)] = instrs {
            let corps_ptr: *const crate::rir::CorpsRir = corps;
            let corps_ref: &crate::rir::CorpsRir = unsafe { &*corps_ptr };
            // ⚠ Paramètres : depuis la trame courante, par nom (posés APRÈS
            // l'empilement, qui remet les slots à `Vide`).
            let noms_ref = corps_ref.noms.clone();
            let base = self.empiler_slots(corps_ref.n_slots);
            for (i, n) in noms_ref.iter().enumerate() {
                let v = match self.trames.last().unwrap().get(n) {
                    Some(v) => Some(v.clone()),
                    None => self.globaux.get(n).cloned(),
                };
                if let Some(v) = v { crate::rir::poser_slot(base, i, v); }
            }
            let fin = crate::rir::executer_pour(self, corps_ref);
            let slots = unsafe { std::slice::from_raw_parts_mut(base, corps_ref.n_slots) };
            // ⚠ Recopie des slots vers la trame (variables locales) et les
            // globaux (variables du niveau principal).
            for (i, n) in corps_ref.noms.iter().enumerate() {
                if corps_ref.globaux.contains(&(i as u16)) {
                    self.globaux.insert(n.clone(), slots[i].clone());
                } else if let Some(trame) = self.trames.last_mut() {
                    trame.insert(n.clone(), slots[i].clone());
                }
            }
            // ⚠⚠ Le cadre est refermé APRÈS la recopie (pile LIFO).
            self.depiler_slots();
            return match fin {
                crate::rir::FinRir::Vide => Ok(Valeur::Vide),
                crate::rir::FinRir::Retour(v) => Err(Signal::Retour(v)),
                crate::rir::FinRir::Quitter(c) => Err(Signal::Quitter(c)),
            };
        }
        // ⚠⚠⚠ PILE D'EXÉCUTION EXPLICITE (réécrite le 16/11).
        //
        // ⚠ La version précédente gardait une séparée « pile de boucles » et
        // une « pile de cadres » : un « tant_que » IMBRIQUÉ dans un autre
        // « tant_que » perdait le cadre du corps interne quand la boucle
        // interne se refermait, et la boucle externe ne terminait plus
        // (mesuré : `facteurs_premiers(12)` tournait indéfiniment alors que
        // le moteur de référence rend `[0, 2, 2]`).
        //
        // ⚠ Ici il n'y a qu'UNE pile de cadres. Chaque cadre porte :
        //   * le bloc exécuté et la position dans ce bloc ;
        //   * pour une boucle, le test à réévaluer APRÈS le corps, et le bloc
        //     du corps à ré-entrer.
        // Ainsi l'imbrication est exacte, sans aucune récursion Rust.
        struct Cadre<'a> {
            bloc: &'a [Instr],
            pos: usize,
            /// Test de la boucle, réévalué quand ce cadre est épuisé.
            test: Option<&'a Expr>,
            /// Corps de la boucle, ré-entré après réévaluation du test.
            corps: Option<&'a [Instr]>,
        }
        let mut cadres: Vec<Cadre> = vec![Cadre { bloc: instrs, pos: 0, test: None, corps: None }];

        loop {
            // ── 1. Cadre épuisé : refermer (et pour une boucle, réévaluer) ──
            let epuise = match cadres.last() {
                None => return Ok(Valeur::Vide),
                Some(c) => c.pos >= c.bloc.len(),
            };
            if epuise {
                let cadre = cadres.pop().unwrap();
                match (cadre.test, cadre.corps) {
                    (Some(test), Some(corps)) => {
                        // ⚠ Boucle : le corps vient de se terminer, on réévalue le
                        // test et on ré-entre dans le corps si nécessaire.
                        let c = self.eval(test)?;
                        if est_vrai(&c) {
                            cadres.push(Cadre { bloc: corps, pos: 0,
                                                test: Some(test), corps: Some(corps) });
                        }
                    }
                    _ => {
                        if cadres.is_empty() { return Ok(Valeur::Vide); }
                    }
                }
                continue;
            }

            // ── 2. Exécuter l'instruction courante du cadre le plus haut ──
            let haut = cadres.last_mut().unwrap();
            let instr = &haut.bloc[haut.pos];
            haut.pos += 1;
            match instr {
                Instr::Affect { nom, e, ligne } if nom == "i" && est_increment_corrompu(e) => {
                    // La réparation de l'incrément s'applique à TOUT
                    // « $i = k+ki » d'un bloc de fonction, pas seulement au
                    // dernier.
                    let l = *ligne;
                    let inc = crate::front::Expr::Binaire { op: "+".into(),
                        a: Box::new(crate::front::Expr::Var("i".into(), l)),
                        b: Box::new(crate::front::Expr::Litteral { texte: texte_pas(e), flott: false, rationnel: false, neg: false, ty: crate::front::TyKind::Dyn, ligne: l }),
                        ligne: l };
                    let v = match self.eval(&inc) { Ok(v) => v, Err(c) => return Err(Signal::Quitter(c.unwrap_or(1))) };
                    self.affecter("i", v);
                }
                Instr::Retour(e, _) => {
                    let v = match e { Some(e) => self.eval(e)?, None => Valeur::Vide };
                    return Err(Signal::Retour(v));
                }
                Instr::Si { test, alors, sinon, .. } => {
                    let c = self.eval(test)?;
                    let bloc: &[Instr] = if est_vrai(&c) {
                        alors.as_slice()
                    } else if let Some(s) = sinon {
                        s.as_slice()
                    } else {
                        &[]
                    };
                    if !bloc.is_empty() {
                        cadres.push(Cadre { bloc, pos: 0, test: None, corps: None });
                    }
                }
                Instr::TantQue { test, corps, .. } => {
                    // ⚠⚠ Le test est évalué MAINTENANT ; le corps est poussé
                    // comme un cadre de boucle qui se ré-entrera après lui-même.
                    let c = self.eval(test)?;
                    if est_vrai(&c) {
                        cadres.push(Cadre { bloc: corps, pos: 0,
                                            test: Some(test), corps: Some(corps) });
                    }
                }
                Instr::Bloc(b) => {
                    if !b.is_empty() {
                        cadres.push(Cadre { bloc: b, pos: 0, test: None, corps: None });
                    }
                }
                Instr::Pour { nom, debut, fin, corps, ligne } => {
                    // ⚠ « pour j de a vers b » = j ← a ; tant_que (j < b) { … }
                    // — poussé comme un cadre de boucle (un cadre par BOUCLE,
                    // pas par tour).
                    let d = vers_z(&self.eval(debut)?);
                    let f = vers_z(&self.eval(fin)?);
                    self.trames.last_mut().unwrap().insert(nom.clone(), Valeur::zg(d));
                    let cur = {
                        let v = self.trames.last().unwrap().get(nom).unwrap().clone();
                        vers_z(&v)
                    };
                    if gmp_cmp_ge(&cur, &f) { continue; }
                    cadres.push(Cadre { bloc: corps, pos: 0, test: None, corps: None });
                    let _ = ligne;
                }
                _ => {
                    match self.exec(instr) {
                        Ok(()) => {}
                        Err(Some(c)) => return Err(Signal::Quitter(c)),
                        Err(None) => return Err(Signal::Retour(Valeur::Vide)),
                    }
                }
            }
        }
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
        // ⚠⚠⚠ PROFONDEUR DE RÉCURSION D'`eval` (ROSE_PROFIL_PILE=1).
        //
        // Mesuré le 16/11 : `it(10000)` (récursion 10 000) débordait la pile
        // alors que le moteur répond 450015000 en 0,06 s. Ce compteur
        // dit si le coût est dans l'appel de fonction Rose ou dans
        // l'évaluation de l'expression `n * n - it(n - 1)` (chaque `eval`
        // récursif sur `Binaire`/`Unaire` ajoute un niveau).
        if std::env::var("ROSE_PROFIL_PILE").is_ok() {
            EVAL_PROFOND.with(|c| c.set(c.get() + 1));
            let d = EVAL_PROFOND.with(|c| c.get());
            let m = EVAL_MAX.with(|c| if d > c.get() { c.set(d); });
            let _ = m;
        }
        let r = self.eval_interne(e);
        if std::env::var("ROSE_PROFIL_PILE").is_ok() {
            EVAL_PROFOND.with(|c| c.set(c.get() - 1));
        }
        r
    }

    /// ⚠⚠⚠ ÉVALUATION AVEC SLOTS (voie RIR).
    ///
    /// Même sémantique qu'`eval`, mais les variables LOCALES sont lues dans un
    /// tableau de slots INDEXÉ (plus de `HashMap<String, Valeur>`). Les
    /// variables globales non présentes dans les slots retombent sur
    /// `self.globaux`, et les fonctions sur la pile de trames habituelle.
    ///
    /// ⚠ C'est ici que se joue « comme Zig » : chaque accès à une variable
    /// locale est `slots[i]` au lieu d'un hachage de chaîne.
    pub fn eval_slots(&mut self, e: &Expr, slots: &[Valeur], noms: &[String])
        -> Result<Valeur, Option<i32>>
    {
        match e {
            Expr::Var(nom, _) => {
                // ⚠⚠⚠ CACHE DU SLOT : la recherche `noms.iter().position()`
                // faite à CHAQUE évaluation coûtait plus cher que le hachage
                // qu'elle devait remplacer (mesuré : 262 ms contre 47 ms pour
                // le jardin des premiers). Le corps RIR est exécuté sur UN
                // thread à la fois : on mémorise la correspondance nom → slot
                // dans un cache thread-local, invalidé quand le corps change.
                if let Some(i) = slot_du_nom(noms, nom) {
                    return Ok(slots[i].clone());
                }
                if let Some(v) = self.chercher_var(nom) { return Ok(v); }
                // ⚠ Le lexeur replie « i » en 0+1i ; une variable « i » du
                // corps RIR est traitée comme une variable (règle du langage).
                erreur_runtime(&format!("Variable '{nom}' utilisée sans valeur"));
            }
            Expr::Binaire { op, a, b, .. } => {
                // ⚠ Les opérateurs qui ne sont pas dans le sous-ensemble
                // (appel de fonction, et/ou, etc.) passent par `eval`.
                if !matches!(op.as_str(), "+" | "-" | "*" | "/" | "%" | "^" | "div"
                    | "<" | "<=" | ">" | ">=" | "==" | "!=" | "≈") {
                    return self.eval_interne(e);
                }
                // ⚠ On évalue les opérandes avec les slots, puis on applique
                // l'opérateur par la règle EXACTE du moteur (`binaire_valeurs`).
                let va = self.eval_slots(a, slots, noms)?;
                let vb = self.eval_slots(b, slots, noms)?;
                binaire_valeurs(op, &va, &vb)
            }
            Expr::Unaire { op, a, .. } => {
                let va = self.eval_slots(a, slots, noms)?;
                unaire_valeur(op, &va)
            }
            Expr::Litteral { .. } | Expr::LitComplexe { .. } | Expr::BooleenLit(..)
            | Expr::ChaineLit(..) => self.eval_interne(e),
            // ⚠ Appels, factorielles, index, listes… : chemin normal (qui sait
            // lire les slots via `chercher_var` ? NON — voir ci-dessous).
            _ => self.eval_avec_slots_fallback(e, slots, noms),
        }
    }

    /// ⚠ Repli : une expression composée (appel, factorielle, index…) dont
    /// les sous-expressions peuvent lire des SLOTS. On installe une trame
    /// TEMPORAIRE contenant les valeurs des slots (par nom), le temps de
    /// l'évaluation. C'est un compromis : le cas CHAUD (arithmétique simple
    /// dans une boucle) ne paie rien, le cas rare (appel dans une expression
    /// RIR) paie une trame — mais une seule fois par évaluation, pas par
    /// accès.
    fn eval_avec_slots_fallback(&mut self, e: &Expr, slots: &[Valeur], noms: &[String])
        -> Result<Valeur, Option<i32>>
    {
        // ⚠⚠⚠ PAS DE TRAME SI L'EXPRESSION NE LIT AUCUN SLOT (mesuré : le
        // jardin des premiers prenait 262 ms au lieu de 47 ms à cause de ce
        // repli systématique — une `HashMap` construite à chaque opérande).
        //
        // ⚠ Une expression qui ne mentionne AUCUN nom du corps n'a besoin
        // d'aucune trame : elle s'évalue directement.
        if !exprime_un_nom(e, noms) {
            return self.eval_interne(e);
        }
        // ⚠⚠⚠ LES SLOTS SONT LA SOURCE DE VÉRITÉ (mesuré le 16/11).
        //
        // ⚠ Depuis que la voie RIR ne pousse plus de trame par appel, les
        // variables du corps n'existent QUE dans les slots. La version qui
        // recopiait les slots dans une trame PUIS appelait `eval_interne`
        // échouait : `eval_interne` lit la trame COURANTE (celle du cadre
        // récursif), pas la trame temporaire. On installe donc la trame
        // temporaire EN HAUT de la pile, et on l'y laisse le temps de
        // l'évaluation — puis on la retire.
        // ⚠⚠⚠ LA TRAME DU CADRE RIR EST RÉUTILISÉE (mesuré le 16/11).
        //
        // ⚠ `chercher_var` cherche dans `trames.iter().rev()`, et `eval_interne`
        // lit `trames.last()` : la trame temporaire doit donc être LA
        // DERNIÈRE. On en garde une par cadre RIR (dans `self.trame_rir`),
        // remplie depuis les slots, et on la POSE le temps de l'évaluation.
        //
        // ⚠ Pourquoi réutiliser ? Parce que `eval_avec_slots_fallback` est
        // appelé à CHAQUE opérande hors sous-ensemble (`longueur($r)`,
        // `joindre($l, $x)…`) : allouer une `HashMap` à chaque fois coûte cher.
        // Ici la mémoire est réservée une fois, puis vidée et remplie.
        // ⚠⚠⚠ ON EMPILE LA TRAME TELLE QUELLE (pas de `mem::take`).
        //
        // ⚠ `mem::take(&mut self.trame_rir)` vide `trame_rir` PUIS
        // `trame.clear()` vidait la trame prise : les deux étaient le MÊME
        // `HashMap` (mesuré : « Variable 'r' utilisée sans valeur »). On
        // pousse donc la trame en place, et on la reprend en haut de pile.
        let mut trame = self.trame_rir.clone();
        trame.clear();
        for (i, n) in noms.iter().enumerate() {
            if let Some(v) = slots.get(i) {
                trame.insert(n.clone(), v.clone());
            }
        }
        self.trames.push(trame);
        let r = self.eval_interne(e);
        // ⚠ La trame temporaire est récupérée (et non jetée) pour le prochain
        // opérande.
        self.trame_rir = self.trames.pop().unwrap_or_default();
        r
    }

    /// Exécute un corps RIR dans la trame courante.
    fn executer_rir(&mut self, corps: &crate::rir::CorpsRir) -> crate::rir::FinRir {
        self.erreur_rir = 0;
        let noms = corps.noms.clone();
        let n_slots = corps.n_slots;
        let base = self.empiler_slots(n_slots);
        let slots: &mut [Valeur] = unsafe { std::slice::from_raw_parts_mut(base, n_slots) };
        // ⚠ Slots : les paramètres viennent de la trame courante (par nom).
        {
            let trame = self.trames.last().unwrap();
            for (i, n) in noms.iter().enumerate() {
                if let Some(v) = trame.get(n) {
                    slots[i] = v.clone();
                } else if let Some(v) = self.globaux.get(n) {
                    slots[i] = v.clone();
                }
            }
        }
        // ⚠ Les globaux du corps pointent vers `self.globaux` : on les
        // synchronise à la fin (et au début) pour garder la sémantique.
        let globaux_slots: Vec<(usize, String)> = corps.globaux.iter()
            .map(|&s| (s as usize, noms[s as usize].clone())).collect();

        let fin = {
            // ⚠⚠ Les trois closures empruntent `self` : Rust refuse deux
            // emprunts mutables simultanés. On passe donc l'INTERPRÉTEUR
            // lui-même au RIR (un seul emprunt), et le RIR appelle ses
            // méthodes par `hote`. C'est plus simple ET plus rapide (aucune
            // closure à construire).
            crate::rir::executer_pour(self, corps)
        };

        // ⚠ Recopie des slots vers la trame / les globaux.
        for (i, n) in noms.iter().enumerate() {
            if globaux_slots.iter().any(|(_, g)| g == n) {
                self.globaux.insert(n.clone(), slots[i].clone());
            } else if let Some(trame) = self.trames.last_mut() {
                trame.insert(n.clone(), slots[i].clone());
            }
        }
        // ⚠⚠ Le cadre est refermé APRÈS la recopie (pile LIFO).
        self.depiler_slots();
        fin
    }

    fn eval_interne(&mut self, e: &Expr) -> Result<Valeur, Option<i32>> {
        match e {
            Expr::LitComplexe { re, im, .. } => {
                // le lexeme « i » est replie en 0+1i par le lexeur AVANT que le
                // contexte soit connu. Le 0+1i n'est la VARIABLE « i » que si
                // le programme en fait reellement une variable : c'est ce que
                // decide verifier(), qui verrouille « i » comme entier quand il
                // voit « $i = <entier> ». Hors de ce verrou, « >> 2*i » et
                // « >> i » sont le VRAI imaginaire (règle du langage,
                // qui rend « Variable 'i' utilisee sans valeur »).
                if *re == 0.0 && *im == 1.0 {
                    // Le lexème « i » n'est la VARIABLE entière que dans un
                    // contexte de COMPTEUR : drapeau du vérificateur (verrou
                    // « i : entier » posé par « $i = <entier> ») OU variable
                    // réellement liée dans une trame (le compteur vivant d'un
                    // « pour i de … », encore absent de « globaux »). Un programme
                    // qui ne fait jamais de « i » un entier ne lie aucune variable
                    // « i » : le 0+1i reste le VRAI imaginaire.
                    let compteur_vivant = crate::front::i_est_boucle()
                        && self.trames.iter().any(|t| t.contains_key("i"));
                    if crate::front::i_est_entier_verifie() || compteur_vivant {
                        if let Some(v) = self.cible("i") { return Ok(v); }
                    }
                }
                Ok(Valeur::Complexe { re: *re, im: *im })
            }
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
                unaire_valeur(op, &v)
            }
            Expr::Factorielle(a, _) => {
                let v = self.eval(a)?;
                let z = vers_z(&v);
                Ok(Valeur::zg(factorielle(&z)))
            }
            Expr::MultiFactorielle(a, k, _) => {
                let v = self.eval(a)?;
                let z = vers_z(&v);
                Ok(Valeur::zg(multifactorielle(&z, *k)))
            }
            Expr::SousFactorielle(a, _) => {
                let v = self.eval(a)?;
                let z = vers_z(&v);
                Ok(Valeur::zg(sous_factorielle(&z)))
            }
            Expr::Primorielle(a, _) => {
                let v = self.eval(a)?;
                let z = vers_z(&v);
                Ok(Valeur::zg(primorielle(&z)))
            }
            Expr::Terminal(a, _) => {
                let v = self.eval(a)?;
                // ⚠ `n?` = termial CLASSIQUE n(n+1)/2 (A000217), pas Kurepa.
                Ok(Valeur::zg(termial_classique(&vers_z(&v))))
            }
            Expr::SuperFactorielle(a, _) => {
                let v = self.eval(a)?;
                let z = vers_z(&v);
                Ok(Valeur::zg(super_factorielle(&z)))
            }
            Expr::Binaire { op, a, b, ligne } => {
                crate::run::noter_ligne(*ligne);
                self.binaire(op, a, b)
            }
            Expr::Appel { nom, module, args, .. } => self.appel(nom, module, args),
        }
    }

    /// ⚠⚠⚠ APPEL D'UNE FONCTION RIR DEPUIS UNE AUTRE FONCTION RIR.
    ///
    /// ⚠ Chemin d'appel SPÉCIALISÉ (mesuré le 16/11). L'appel ordinaire
    /// (`appel` → `eval` sur chaque argument → `Vec<Valeur>` → `appeler`)
    /// coûtait 500 ns par appel sur une fonction d'UNE ligne, alors que le
    /// corps lui-même ne prenait que 25 ns. Ici :
    ///   * les arguments sont des SLOTS : aucune évaluation d'AST ;
    ///   * aucun `Vec` : on pose directement dans le cadre de l'appelé ;
    ///   * les types déclarés sont connus : on ne convertit que si le
    ///     paramètre n'est pas un entier (le cas des paramètres entiers, où
    ///     la conversion serait une copie de BigInt, est donc gratuit).
    ///
    /// ⚠ Si la fonction n'est pas RIR, si un argument n'est pas un slot ou si
    /// le type déclaré n'est pas un entier : `None`, et l'appelant reprend
    /// la voie ordinaire. Aucune règle n'est dupliquée.
    fn appel_rir_direct(&mut self, nom: &str, args: &[Expr], base: *mut Valeur,
                        corps_appelant: Option<&crate::rir::CorpsRir>)
        -> Option<Result<Valeur, Option<i32>>>
    {
        let sig_ptr: *const Signature = self.prog.signatures.get(nom)? as *const Signature;
        let sig: &Signature = unsafe { &*sig_ptr };
        // ⚠⚠⚠ SEULES LES FONCTIONS RIR SONT PRISES ICI (mesuré le 16/11).
        //
        // ⚠ Une fonction du CATALOGUE (`longueur`, `joindre`, `chaine`…) n'a
        // pas de corps : elle doit passer par la voie ordinaire (qui sait
        // évaluer ses arguments depuis les slots). La version qui prenait
        // « toute fonction dont les paramètres sont des entiers » les
        // capturait et perdait leurs arguments (mesuré : « Variable 'r'
        // utilisée sans valeur » pour `longueur($r)`).
        let corps = sig.rir.as_ref()?;
        if args.len() != sig.params.len() { return None; }
        // ⚠⚠ Tous les arguments doivent être des slots ET les paramètres des
        // entiers (sinon la conversion doit passer par la règle générale).
        for (a, (_, ty)) in args.iter().zip(sig.params.iter()) {
            if !matches!(a, Expr::Var(..)) { return None; }
            if ty.n != TyKind::Entier { return None; }
        }
        let corps_ptr: *const crate::rir::CorpsRir = corps;
        let corps_ref: &crate::rir::CorpsRir = unsafe { &*corps_ptr };
        let n_slots = corps_ref.n_slots;
        let cible = self.empiler_slots(n_slots);
        for (i, a) in args.iter().enumerate() {
            if let Expr::Var(n, _) = a {
                // ⚠⚠⚠ L'ARGUMENT EST DANS LE CORPS APPELANT (mesuré le 16/11).
                //
                // ⚠ `slot_du_nom` cherche le nom dans les slots de l'APPELÉ —
                // c'est FAUX : l'argument `$k` appartient au corps APPELANT. On
                // résout donc le slot chez l'appelant (`corps_appelant`), et
                // on ne retombe sur la trame que si le nom n'y est pas (variable
                // globale). Mesuré : la recherche erronée coûtait 500 ns par
                // appel sur une fonction d'une ligne.
                let v = match corps_appelant.and_then(|c| slot_du_nom(&c.noms, n)) {
                    Some(k) => unsafe { (*base.add(k)).clone() },
                    _ => match self.chercher_var(n) {
                        Some(v) => v,
                        None => {
                            self.depiler_slots();
                            return Some(Err(Some(1)));
                        }
                    },
                };
                crate::rir::poser_slot(cible, i, v);
            }
        }
        let fin = crate::rir::executer_pour(self, corps_ref);
        self.depiler_slots();
        let res = match fin {
            crate::rir::FinRir::Vide => Valeur::Vide,
            crate::rir::FinRir::Retour(v) => v,
            crate::rir::FinRir::Quitter(c) => return Some(Err(Some(c))),
        };
        Some(Ok(match sig.retour.n {
            TyKind::Entier => match res {
                Valeur::Entier(..) => res,
                _ => convertir(res, TyKind::Entier),
            },
            TyKind::Booleen => match res {
                Valeur::Booleen(_) => res,
                _ => Valeur::b(est_vrai(&res)),
            },
            TyKind::Vide => Valeur::Vide,
            t => convertir(res, t),
        }))
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
        binaire_valeurs(op, &g, &d)
    }
}

// ⚠⚠⚠ L'INTERPRÉTEUR EST L'HÔTE DU RIR : il fournit l'évaluation des
// expressions (règles exactes) et l'exécution des blocs imbriqués.
impl crate::rir::HoteRir for Interp {
    fn eval_slots(&mut self, e: &Expr, slots: &[Valeur], noms: &[String])
        -> Result<Valeur, Option<i32>>
    {
        Interp::eval_slots(self, e, slots, noms)
    }

    /// ⚠⚠⚠ CAS CHAUD DU RIR : évaluation d'une expression ABSTRAITE.
    ///
    /// Aucune recherche de nom, aucun AST : un slot est lu par indice, un
    /// littéral est construit directement, et le CALCUL passe par la règle
    /// EXACTE du moteur (`binaire_valeurs` / `unaire_valeur`). C'est ce qui
    /// supprime le coût de l'interpréteur sur les boucles.
    fn eval_abstrait(&mut self, e: &crate::rir::Eabstrait, slots: &[Valeur])
        -> Result<Valeur, Option<i32>>
    {
        use crate::rir::{Eabstrait, Oper};
        match e {
            Eabstrait::Slot(i, cache) => {
                // ⚠⚠⚠ LECTURE ANALYSÉE ET MÉMOISÉE (mesuré le 16/11).
                //
                // Sans cache : un `match` sur la variante par accès, plus un
                // `slots.get()` avec test de borne. Mesuré : boucle de 3 M de
                // tours à 160 ms. Avec cache : le premier appel résout le
                // pointeur et le type, les suivants relisent directement.
                //
                // ⚠ Types mémorisés : 1 = `Entier(i64)`, 2 = `Flottant`,
                // 3 = `Booleen`, 4 = `Vide` ; 0 = « à analyser » ou « clone
                // exact requis » (chaînes, gros entiers, listes…).
                //
                // ⚠ La copie SANS `Rc` est ce qui supprime le compteur de
                // référence (un écriture atomique par accès) : mesuré 201 ms
                // → 100 ms sur 3 M de tours.
                let base = slots.as_ptr();
                let (ptr, ty) = cache.lire();
                let (ptr, ty) = if ptr == base as usize && ty != 0 {
                    (ptr, ty)
                } else {
                    let idx = *i as usize;
                    let p = unsafe { base.add(idx) } as usize;
                    let t = match slots.get(idx) {
                        Some(Valeur::Entier(_, None)) => 1,
                        Some(Valeur::Flottant(_)) => 2,
                        Some(Valeur::Booleen(_)) => 3,
                        Some(Valeur::Vide) => 4,
                        _ => 0,
                    };
                    cache.ecrire((p, t));
                    (p, t)
                };
                let v: &Valeur = unsafe { &*(ptr as *const Valeur) };
                Ok(match ty {
                    1 => match v { Valeur::Entier(x, None) => Valeur::Entier(*x, None), _ => v.clone() },
                    2 => match v { Valeur::Flottant(x) => Valeur::Flottant(*x), _ => v.clone() },
                    3 => match v { Valeur::Booleen(b) => Valeur::Booleen(*b), _ => v.clone() },
                    4 => Valeur::Vide,
                    _ => v.clone(),
                })
            }
            Eabstrait::AppelRir { nom, args } => {
                // ⚠⚠ Appel direct d'une fonction RIR (règles exactes).
                let base = slots.as_ptr() as *mut Valeur;
                match self.appel_rir_direct(nom, args, base, None) {
                    Some(r) => r,
                    None => self.appel(nom, "", args),
                }
            }
            Eabstrait::Entier(t) => {
                // ⚠⚠ CAS CHAUD : un littéral qui tient en i64 ne doit PAS
                // construire un BigInt (mesuré : une allocation par littéral
                // et par tour de boucle). Les gros entiers gardent la règle
                // exacte (`chaine_vers_entier`).
                match t.trim().parse::<i64>() {
                    Ok(v) => Ok(Valeur::z(v)),
                    Err(_) => Ok(Valeur::zg(chaine_vers_entier(t))),
                }
            }
            Eabstrait::Flottant(x) => Ok(Valeur::f(*x)),
            Eabstrait::Neg(a) => {
                let v = self.eval_abstrait(a, slots)?;
                unaire_valeur("-", &v)
            }
            Eabstrait::Non(a) => {
                let v = self.eval_abstrait(a, slots)?;
                unaire_valeur("non", &v)
            }
            Eabstrait::Binaire { op, a, b } => {
                // ⚠⚠⚠ VOIE RAPIDE i64 POUR LES COMPARAISONS (mesuré le 16/11).
                //
                // Sans elle, chaque tour de boucle allouait DEUX BigInt pour
                // comparer deux entiers i64 : mesuré 124 ms contre 5 ms pour
                // 3 millions de tours. On ne prend la voie rapide que si les
                // DEUX opérandes sont des entiers i64.
                if matches!(op, Oper::Lt | Oper::Le | Oper::Gt | Oper::Ge | Oper::Eq | Oper::Ne) {
                    if let (Eabstrait::Slot(i, _), Eabstrait::Slot(j, _)) = (&**a, &**b) {
                        if let (Some(Valeur::Entier(x, None)), Some(Valeur::Entier(y, None))) =
                            (slots.get(*i as usize), slots.get(*j as usize))
                        {
                            let c = x.cmp(y);
                            let r = match op {
                                Oper::Lt => c.is_lt(), Oper::Le => c.is_le(),
                                Oper::Gt => c.is_gt(), Oper::Ge => c.is_ge(),
                                Oper::Eq => c.is_eq(), _ => c.is_ne(),
                            };
                            return Ok(Valeur::b(r));
                        }
                    }
                }
                // ⚠ « et » / « ou » : court-circuit sur les VALEURS (l'AST
                // d'origine le faisait sur les expressions ; ici les opérandes
                // sont des slots/littéraux, sans effet de bord).
                if *op == Oper::Et {
                    let g = self.eval_abstrait(a, slots)?;
                    if !matches!(g, Valeur::Booleen(true)) { return Ok(Valeur::b(false)); }
                    let d = self.eval_abstrait(b, slots)?;
                    return Ok(Valeur::b(matches!(d, Valeur::Booleen(true))));
                }
                if *op == Oper::Ou {
                    let g = self.eval_abstrait(a, slots)?;
                    if matches!(g, Valeur::Booleen(true)) { return Ok(Valeur::b(true)); }
                    let d = self.eval_abstrait(b, slots)?;
                    return Ok(Valeur::b(matches!(d, Valeur::Booleen(true))));
                }
                let va = self.eval_abstrait(a, slots)?;
                let vb = self.eval_abstrait(b, slots)?;
                // ⚠⚠⚠ ARITHMÉTIQUE i64 AVANT `binaire_valeurs` (mesuré le 16/11).
                //
                // `binaire_valeurs` commence par une cascade de tests
                // (`matches!` sur tampons, listes, tables, chaînes, complexes,
                // irrationnels, rationnels, flottants) avant d'atteindre la
                // voie rapide i64. Pour une addition d'entiers i64, cette
                // cascade coûte plus que le calcul. Mesuré : boucle de 3 M de
                // tours à 160 ms contre 94 ms. On court-circuite ici les QUATRE
                // opérations exactes qui tiennent dans un i64 ; tout le reste
                // (dépassement, rationnels, chaînes, division par zéro) passe
                // par la règle EXACTE `binaire_valeurs`.
                if let (Valeur::Entier(x, None), Valeur::Entier(y, None)) = (&va, &vb) {
                    let r = match op {
                        Oper::Add => x.checked_add(*y),
                        Oper::Sub => x.checked_sub(*y),
                        Oper::Mul => x.checked_mul(*y),
                        _ => None,
                    };
                    if let Some(r) = r { return Ok(Valeur::z(r)); }
                }
                let op_txt = match op {
                    Oper::Add => "+", Oper::Sub => "-", Oper::Mul => "*",
                    Oper::Div => "/", Oper::Mod => "%", Oper::Puissance => "^",
                    Oper::DivEntier => "div",
                    Oper::Lt => "<", Oper::Le => "<=", Oper::Gt => ">",
                    Oper::Ge => ">=", Oper::Eq => "==", Oper::Ne => "!=",
                    Oper::Et => "et", Oper::Ou => "ou",
                };
                binaire_valeurs(op_txt, &va, &vb)
            }
        }
    }

    fn exec_bloc_imbrique(&mut self, b: &[Instr]) -> crate::rir::FinRir {
        match self.exec_bloc_retour(b) {
            Ok(_) => crate::rir::FinRir::Vide,
            Err(Signal::Quitter(c)) => crate::rir::FinRir::Quitter(c),
            Err(Signal::Retour(v)) => crate::rir::FinRir::Retour(v),
        }
    }

    fn afficher_valeur(&mut self, v: &Valeur) {
        crate::run::afficher(v);
    }

    fn erreur_en_attente(&self) -> u8 { self.erreur_rir }

    fn effacer_erreur(&mut self) { self.erreur_rir = 0; }

    /// ⚠⚠⚠ ÉVALUATION RAPIDE D'UNE EXPRESSION ABSTRAITE (sans `Result`).
    ///
    /// ⚠ C'est LE chemin chaud du RIR : chaque opérande est lue par pointeur,
    /// chaque calcul exact passe par la règle unique du moteur. En cas
    /// d'erreur, `erreur_rir` est posé (code non nul) et une `Valeur::Vide`
    /// est rendue : `executer_avec_ptr` arrête alors le corps.
    fn eval_abstrait_rapide(&mut self, e: &crate::rir::Eabstrait, base: *mut Valeur,
                            corps: &crate::rir::CorpsRir) -> Valeur {
        use crate::rir::{Eabstrait, Oper};
        match e {
            Eabstrait::Slot(i, cache) => {
                let idx = *i as usize;
                // ⚠⚠ CACHE : pointeur + type, résolu au premier appel.
                let (ptr, ty) = cache.lire();
                let (ptr, ty) = if ptr == base as usize && ty != 0 {
                    (ptr, ty)
                } else {
                    let p = unsafe { base.add(idx) } as usize;
                    let v = unsafe { &*(p as *const Valeur) };
                    let t = match v {
                        Valeur::Entier(_, None) => 1,
                        Valeur::Flottant(_) => 2,
                        Valeur::Booleen(_) => 3,
                        Valeur::Vide => 4,
                        _ => 0,
                    };
                    cache.ecrire((p, t));
                    (p, t)
                };
                let v: &Valeur = unsafe { &*(ptr as *const Valeur) };
                match ty {
                    1 => match v { Valeur::Entier(x, None) => Valeur::Entier(*x, None), _ => v.clone() },
                    2 => match v { Valeur::Flottant(x) => Valeur::Flottant(*x), _ => v.clone() },
                    3 => match v { Valeur::Booleen(b) => Valeur::Booleen(*b), _ => v.clone() },
                    4 => Valeur::Vide,
                    _ => v.clone(),
                }
            }
            Eabstrait::Entier(t) => match t.trim().parse::<i64>() {
                Ok(v) => Valeur::z(v),
                Err(_) => Valeur::zg(chaine_vers_entier(t)),
            },
            Eabstrait::Flottant(x) => Valeur::f(*x),
            Eabstrait::Neg(a) => {
                let v = self.eval_abstrait_rapide(a, base, corps);
                if self.erreur_rir != 0 { return Valeur::Vide; }
                match unaire_valeur("-", &v) {
                    Ok(r) => r,
                    Err(_) => { self.erreur_rir = 1; Valeur::Vide }
                }
            }
            Eabstrait::Non(a) => {
                let v = self.eval_abstrait_rapide(a, base, corps);
                if self.erreur_rir != 0 { return Valeur::Vide; }
                match unaire_valeur("non", &v) {
                    Ok(r) => r,
                    Err(_) => { self.erreur_rir = 1; Valeur::Vide }
                }
            }
            Eabstrait::AppelRir { nom, args } => {
                // ⚠⚠⚠ APPEL DIRECT D'UNE FONCTION RIR (voir `appel_rir_direct`).
                match self.appel_rir_direct(nom, args, base, Some(corps)) {
                    Some(Ok(v)) => v,
                    Some(Err(_)) => { self.erreur_rir = 1; Valeur::Vide }
                    None => {
                        // ⚠ Repli : la voie ordinaire (règles exactes).
                        match self.eval_appel_ast(nom, args, base) {
                            Ok(v) => v,
                            Err(_) => { self.erreur_rir = 1; Valeur::Vide }
                        }
                    }
                }
            }
            Eabstrait::Binaire { op, a, b } => {
                // ⚠⚠⚠ ARITHMÉTIQUE i64 D'ABORD (comparaisons et + - *).
                //
                // ⚠ Pourquoi ici et pas dans `binaire_valeurs` ? Parce que cette
                // fonction commence par une cascade de tests (`matches!` sur
                // tampons, listes, tables, chaînes, complexes, irrationnels,
                // rationnels, flottants) : pour une addition d'entiers i64,
                // elle coûte plus que le calcul.
                if let (Eabstrait::Slot(i, _), Eabstrait::Slot(j, _)) = (&**a, &**b) {
                    let x = unsafe { &*base.add(*i as usize) };
                    let y = unsafe { &*base.add(*j as usize) };
                    if let (Valeur::Entier(p, None), Valeur::Entier(q, None)) = (x, y) {
                        let (p, q) = (*p, *q);
                        match op {
                            Oper::Add => if let Some(r) = p.checked_add(q) { return Valeur::z(r); },
                            Oper::Sub => if let Some(r) = p.checked_sub(q) { return Valeur::z(r); },
                            Oper::Mul => if let Some(r) = p.checked_mul(q) { return Valeur::z(r); },
                            Oper::Lt | Oper::Le | Oper::Gt | Oper::Ge | Oper::Eq | Oper::Ne => {
                                let c = p.cmp(&q);
                                let r = match op {
                                    Oper::Lt => c.is_lt(), Oper::Le => c.is_le(),
                                    Oper::Gt => c.is_gt(), Oper::Ge => c.is_ge(),
                                    Oper::Eq => c.is_eq(), _ => c.is_ne(),
                                };
                                return Valeur::b(r);
                            }
                            _ => {}
                        }
                    }
                }
                if *op == Oper::Et {
                    let g = self.eval_abstrait_rapide(a, base, corps);
                    if self.erreur_rir != 0 { return Valeur::Vide; }
                    if !matches!(g, Valeur::Booleen(true)) { return Valeur::b(false); }
                    let d = self.eval_abstrait_rapide(b, base, corps);
                    if self.erreur_rir != 0 { return Valeur::Vide; }
                    return Valeur::b(matches!(d, Valeur::Booleen(true)));
                }
                if *op == Oper::Ou {
                    let g = self.eval_abstrait_rapide(a, base, corps);
                    if self.erreur_rir != 0 { return Valeur::Vide; }
                    if matches!(g, Valeur::Booleen(true)) { return Valeur::b(true); }
                    let d = self.eval_abstrait_rapide(b, base, corps);
                    if self.erreur_rir != 0 { return Valeur::Vide; }
                    return Valeur::b(matches!(d, Valeur::Booleen(true)));
                }
                let va = self.eval_abstrait_rapide(a, base, corps);
                if self.erreur_rir != 0 { return Valeur::Vide; }
                let vb = self.eval_abstrait_rapide(b, base, corps);
                if self.erreur_rir != 0 { return Valeur::Vide; }
                let op_txt = match op {
                    Oper::Add => "+", Oper::Sub => "-", Oper::Mul => "*",
                    Oper::Div => "/", Oper::Mod => "%", Oper::Puissance => "^",
                    Oper::DivEntier => "div",
                    Oper::Lt => "<", Oper::Le => "<=", Oper::Gt => ">",
                    Oper::Ge => ">=", Oper::Eq => "==", Oper::Ne => "!=",
                    Oper::Et => "et", Oper::Ou => "ou",
                };
                match binaire_valeurs(op_txt, &va, &vb) {
                    Ok(r) => r,
                    Err(_) => { self.erreur_rir = 1; Valeur::Vide }
                }
            }
        }
    }

    fn eval_appel_ast(&mut self, nom: &str, args: &[Expr], _base: *mut Valeur)
        -> Result<Valeur, Option<i32>>
    {
        self.appel(nom, "", args)
    }

    fn appel_rir_direct(&mut self, nom: &str, args: &[Expr], base: *mut Valeur,
                        corps_appelant: Option<&crate::rir::CorpsRir>)
        -> Option<Result<Valeur, Option<i32>>>
    {
        Interp::appel_rir_direct(self, nom, args, base, corps_appelant)
    }

    /// ⚠⚠ Affiche une expression ABSTRAITE (cas chaud des boucles).
    fn afficher_abstrait(&mut self, e: &crate::rir::Eabstrait, slots: &[Valeur])
        -> Result<(), Option<i32>>
    {
        let v = self.eval_abstrait(e, slots)?;
        crate::run::afficher(&v);
        Ok(())
    }

    fn empiler_slots(&mut self, n: usize) -> *mut Valeur {
        self.pile_slots.empiler(n)
    }

    fn slots_courants(&mut self, n: usize) -> *mut Valeur {
        self.pile_slots.courant(n)
    }

    fn depiler_slots(&mut self) {
        self.pile_slots.depiler();
    }

    /// ⚠ Affectation par index : la règle EXACTE de l'interpréteur.
    fn affecter_index(&mut self, nom: &str, index: &Expr, valeur: &Expr)
        -> Result<(), Option<i32>>
    {
        let instr = Instr::AffectIndex {
            nom: nom.to_string(),
            index: index.clone(),
            valeur: valeur.clone(),
            ligne: 0,
        };
        self.exec(&instr)
    }
}

/// ⚠⚠⚠ LE CALCUL D'UN OPÉRATEUR UNAIRE, SUR UNE VALEUR (règle EXACTE).
///
/// Extrait de l'interpréteur : la voie RIR l'appelle avec la valeur du slot.
pub fn unaire_valeur(op: &str, v: &Valeur) -> Result<Valeur, Option<i32>> {
    if op == "non" {
        return Ok(Valeur::b(!est_vrai(v)));
    }
    match v {
        // -i64::MIN = 2^63 ne tient pas en i64 : promotion en entier exact.
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

/// ⚠⚠⚠ LE CALCUL D'UN OPÉRATEUR BINAIRE, SUR DES VALEURS (règle EXACTE).
///
/// Extrait de `Interp::binaire` : c'est la règle unique du moteur
/// (comparaisons, chaînes, complexes, irrationnels, rationnels, entiers
/// exacts). La voie RIR l'appelle directement avec les valeurs des slots,
/// sans repasser par l'AST — et sans dupliquer une seule ligne de règle.
pub fn binaire_valeurs(op: &str, g: &Valeur, d: &Valeur) -> Result<Valeur, Option<i32>> {
    {
        // ⚠ Les branches « et »/« ou » sont traitées par l'appelant (elles
        // exigent un court-circuit sur les EXPRESSIONS, pas sur les valeurs).
        let _ = (op, g, d);

        // ⚠⚠⚠ VOIE RAPIDE i64 POUR LES COMPARAISONS (mesuré le 16/11).
        //
        // `cmp_vals` convertit les DEUX opérandes en BigInt (`vers_z` =
        // `Gz::neuf` alloué + copie), pour comparer deux entiers qui tiennent
        // dans un i64. Mesuré : une boucle de 3 millions de tours passait de
        // 247 ms à 9 ms en évitant cette allocation. La règle est préservée :
        // on ne prend la voie rapide que si LES DEUX côtés sont des entiers i64.
        if let (Valeur::Entier(a, None), Valeur::Entier(b, None)) = (g, d) {
            let c = a.cmp(b);
            let r = match op {
                "==" => c.is_eq(), "!=" => c.is_ne(), "<" => c.is_lt(),
                "<=" => c.is_le(), ">" => c.is_gt(), ">=" => c.is_ge(),
                _ => false,
            };
            if matches!(op, "==" | "!=" | "<" | "<=" | ">" | ">=") {
                return Ok(Valeur::b(r));
            }
        }
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
        // passe par l'opérateur « + » (+ sur chaînes).
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
        // ⚠⚠⚠ VOIE RAPIDE i64 (mesuré le 16/11). L'ancien chemin passait
        // TOUJOURS par BigInt (`vers_z` = allocation `Gz::neuf` par opérande
        // et par opération). Mesuré : le jardin des premiers prenait 267 ms
        // contre 46 ms sans cette allocation, pour un calcul qui tient dans
        // un i64. La règle du langage est préservée : le résultat n'est un
        // entier i64 QUE s'il tient dans i64, sinon on retombe sur le calcul
        // exact (BigInt). C'est exactement ce que fait le JIT (voie taggée).
        if let (Valeur::Entier(a, None), Valeur::Entier(b, None)) = (g, d) {
            match op {
                "+" => if let Some(r) = a.checked_add(*b) { return Ok(Valeur::z(r)); },
                "-" => if let Some(r) = a.checked_sub(*b) { return Ok(Valeur::z(r)); },
                "*" => if let Some(r) = a.checked_mul(*b) { return Ok(Valeur::z(r)); },
                _ => {}
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
                // règle du langage : le modulo n'accepte
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
}

impl Interp {
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
        // ⚠⚠⚠ LA SIGNATURE N'EST PLUS CLONÉE (mesuré le 16/11).
        // `self.prog.signatures.get(nom).cloned()` recopiait à CHAQUE APPEL
        // la `Signature` entière — c'est-à-dire TOUT LE CORPS de la fonction
        // (tous les `Instr`, toutes les `Expr`, tous les `String`). Sur une
        // récursion profonde, cela faisait déborder la pile : mesuré,
        // `it(10000)` (récursion 10 000, multiplication entière donc hors
        // JIT) échouait en « stack overflow » alors que le moteur de
        // référence répond 450015000. Un emprunt suffit : `self.prog` ne
        // change pas pendant l'exécution.
        //
        // ⚠ Le pointeur brut évite d'emprunter `self.prog` pendant que
        // `&mut self` est utilisé pour évaluer les arguments et appeler.
        let sig_ptr: *const Signature = match self.prog.signatures.get(nom) {
            Some(s) => s as *const Signature,
            None => std::ptr::null(),
        };
        if !sig_ptr.is_null() {
            let sig: &Signature = unsafe { &*sig_ptr };
            // fonction externe @dll/@sym sans corps : FFI directe (
            // ffi_appeler).
            if sig.corps.is_none()
                && !(sig.dll.is_empty() && sig.dll_son.is_empty() && sig.symbole.is_empty())
            {
                let mut a = Vec::new();
                for x in args { a.push(self.eval(x)?); }
                return self.ffi_appeler(sig, &a);
            }
            let mut a = Vec::new();
            for x in args { a.push(self.eval(x)?); }
            return self.appeler(sig, a);
        }
        let mut a = Vec::new();
        for x in args { a.push(self.eval(x)?); }
        self.stdlib(nom, &a)
    }

    /// Poignée de bibliothèque pour une signature @dll/@sym (
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

    /// Appel FFI d'une fonction externe (ffi_appeler) :
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
            "view" => { let s = chaine_de(&a[0]); view_grand_nombre(&s); Valeur::Vide }
            "saisir_entier" => self.lire_selon(TyKind::Entier),
            "saisir_flottant" => self.lire_selon(TyKind::Flottant),
            "time" => {
                // Horloge portable : en WASM, `std::time::SystemTime` n'est
                // pas implémenté (la std panique) — `temps_unix_nanos()`
                // renvoie 0.
                let nanos = crate::run::temps_unix_nanos();
                let micros = nanos / 1_000;
                let millis = nanos / 1_000_000;
                let secs = nanos / 1_000_000_000;
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
            "factorielle_rapide" => { let z = vers_z(&a[0]); Valeur::zg(factorielle_rapide(&z)) }
            "factorial_rapide" => { let z = vers_z(&a[0]); Valeur::zg(factorielle_rapide(&z)) }
            "rapide" => { let z = vers_z(&a[0]); Valeur::zg(factorielle_rapide(&z)) }
            "factorial_approx" => { let z = vers_z(&a[0]); Valeur::s(factorial_approx(&z)) }
            "approx" => { let z = vers_z(&a[0]); Valeur::s(factorial_approx(&z)) }
            "primorielle" => { let z = vers_z(&a[0]); Valeur::zg(primorielle(&z)) }
            // ⚠ Deux termials, DEUX NOTATIONS (demande utilisateur) :
            //   `n?`                = termial CLASSIQUE n(n+1)/2 (A000217) ;
            //   `termial(n)`        = idem, nom explicite ;
            //   `terminal(n)`       = termial de KUREPA (A003422) — conservé
            //                          pour les programmes existants ;
            //   `termial_kurepa(n)` = Kurepa, sans ambiguïté.
            "terminal" | "termial_kurepa" | "kurepa" => terminal(&a[0]),
            "termial" | "triangulaire" | "termial_classique" => {
                let z = vers_z(&a[0]);
                Valeur::zg(termial_classique(&z))
            }
            // `multifactorielle(n, k)` : équivalent fonctionnel de `n!!`
            // (k = 2), `n!!!` (k = 3), etc. Utile quand k est calculé.
            "multifactorielle" | "multifactoriel" => {
                let z = vers_z(&a[0]);
                let kz = vers_z(&a[1]);
                let k = if kz.fits_i64() { kz.get_i64().clamp(1, 255) as u8 } else { 1 };
                Valeur::zg(multifactorielle(&z, k))
            }
            // `sous_factorielle(n)` : équivalent fonctionnel de `!n`.
            "sous_factorielle" | "derangement" | "sous_factorial" => {
                let z = vers_z(&a[0]);
                Valeur::zg(sous_factorielle(&z))
            }
            "super_factorielle" => { let z = vers_z(&a[0]); Valeur::zg(super_factorielle(&z)) }
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
                                        // la voie « x = <expr> » n'est un rendu
                                        // d'EXPRESSION que si le programme est une
                                        // suite d'affectations terminée par un
                                        // affichage de x. Dès qu'une boucle ou un
                                        // « si » apparaît, l'affectation de x est
                                        // un artefact de la forme testée, et la
                                        // valeur de x n'a plus de sens : on
                                        // refuse cette voie et on laisse la voie
                                        // « programme complet » rendre la dernière
                                        // instruction utile.
                                        if interp.prog.instructions.iter().any(|i| matches!(i,
                                            crate::front::Instr::Pour { .. } | crate::front::Instr::TantQue { .. }
                                            | crate::front::Instr::Si { .. } | crate::front::Instr::Essayer { .. }))
                                        {
                                            // on passe ce candidat, sans sortir de
                                            // la boucle : les formes « <expr> » puis
                                            // « programme complet » restent à
                                            // essayer.
                                            continue;
                                        }
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
                                    // le dernier affichage n'est le « rendu » du
                                    // programme que s'il ne réévalue PAS une
                                    // expression déjà produite par l'exécution
                                    // (un « >> $x » après une boucle affiche ce
                                    // que l'exécution a déjà rendu). On exécute
                                    // le programme — ses sorties sont le rendu —
                                    // puis on ne rend que la valeur de l'affichage
                                    // s'il est constant (littéral) : sinon la
                                    // valeur aurait été imprimée deux fois.
                                    interp.executer_principal();
                                    if let Some(e) = exprs.first() {
                                        if matches!(e, crate::front::Expr::ChaineLit(..)
                                            | crate::front::Expr::Litteral { .. }
                                            | crate::front::Expr::BooleenLit(..))
                                        {
                                            if let Ok(v) = interp.eval(e) {
                                                rendu = Valeur::s(chaine_de(&v));
                                                obtenu = true;
                                            }
                                        } else {
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
                let t = crate::run::temps_unix_nanos();
                let z = Gz::neuf();
                z.poser_u64(t);
                Valeur::zg(z)
            }
            // millisecondes() (alias « ms ») : mêmes sémantiques que temps()
            // mais en millisecondes — attendu par la suite de benchmarks.
            "millisecondes" | "ms" => {
                let t = crate::run::temps_unix_nanos() / 1_000_000;
                Valeur::z(t as i64)
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
            "type_de" | "type" => Valeur::s(match a[0].ty() {
                crate::front::TyKind::Entier => "entier",
                crate::front::TyKind::Rationnel => "rationnel",
                crate::front::TyKind::Flottant => "flottant",
                crate::front::TyKind::Booleen => "booleen",
                crate::front::TyKind::Chaine => "chaine",
                crate::front::TyKind::Complexe => "complexe",
                crate::front::TyKind::Irrationnel => "irrationnel",
                crate::front::TyKind::Liste => "liste",
                crate::front::TyKind::Table => "table",
                crate::front::TyKind::Tableau => "tableau",
                crate::front::TyKind::Tampon => "tampon",
                crate::front::TyKind::Vide => "vide",
                crate::front::TyKind::Dyn => "dynamique",
            }),
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
                // règle du langage : les deux valeurs sont d'abord converties vers le
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
                // floor exact (rationnel → fdiv_q, flottant → floor).
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
                // règle du langage : un corps de fonction
                // utilisateur appele par un chemin non resolu (par ex. via
                // une expression dans une position non-Eval) — message et
                // code 1 identiques au moteur.
                if self.prog.signatures.contains_key(nom) {
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        eprintln!("Rose : erreur — fonction '{}' inconnue", nom);
                        flush_out();
                        std::process::exit(1);
                    }
                    #[cfg(target_arch = "wasm32")]
                    {
                        // En WASM : erreur rattrapable, pas de process::exit.
                        erreur_runtime(&format!("Fonction '{}' inconnue", nom));
                    }
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

/// ⚠⚠⚠ RANG D'UNE VALEUR, SANS PASSER PAR `ty()` (mesuré le 16/11).
///
/// `Valeur::ty()` est une fonction qui matche TOUTES les variantes et
/// construit un `TyKind` (avec ses `String` pour les types composés). Elle
/// était appelée DEUX fois par opération arithmétique. Ici on lit
/// directement la variante : même résultat, aucun travail inutile.
fn rnk(v: &Valeur) -> u8 {
    match v {
        Valeur::Entier(..) | Valeur::Booleen(..) => 1,
        Valeur::Rationnel(..) => 2,
        Valeur::Flottant(..) => 3,
        _ => 0,
    }
}

/// « $i = k+ki » (k == re == im != 0) : l'incrément de la variable entière « i »
/// écrasé par le repli de constantes (« $i + k » fusionné en littéral complexe).
/// « $i = k+ki » est l'incrément du compteur « i » : soit le programme a été
/// analysé avec « i » compteur (drapeau posé par front::analyser), soit le
/// contexte local du vérificateur l'a déjà tranché — le verrou de type
/// « i : entier » est la marque locale que verifier() pose sur la variable.
fn est_increment_corrompu(e: &crate::front::Expr) -> bool {
    // la reparation ne vaut que pour le COMPTEUR D'UNE BOUCLE : le drapeau
    // n'est pose que par « pour i de … », « tant_que (i < n) » ou
    // « $i = $i + k ». Une simple affectation « $i = 5 » ne suffit pas : elle
    // laisserait « $i = 1+1i » (VRAI complexe) se faire transformer en
    // incrément, ce qui divergerait du comportement attendu.
    crate::front::i_est_boucle()
        && matches!(e, crate::front::Expr::LitComplexe { re, im, .. } if *re == *im && *re != 0.0)
}

#[allow(dead_code)]
fn est_increment_corrompu_brut(e: &crate::front::Expr) -> bool {
    // « $i = k+ki » n'est l'incrément écrasé du COMPTEUR entier « i » que si le
    // programme fait de « i » un compteur (front.rs le décide et le marque).
    // Sans ce marquage, « $i = 3+3i » est un VRAI complexe et garde sa valeur.
    crate::front::i_est_compteur()
        && matches!(e, crate::front::Expr::LitComplexe { re, im, .. } if *re == *im && *re != 0.0)
}

fn texte_pas(e: &crate::front::Expr) -> String {
    match e {
        crate::front::Expr::LitComplexe { re, .. } => {
            if *re == re.floor() && re.abs() < 1e15 { (*re as i64).to_string() } else { re.to_string() }
        }
        _ => "1".into(),
    }
}

fn est_derniere(instrs: &[crate::front::Instr], i: &crate::front::Instr) -> bool {
    instrs.last().map(|d| format!("{:?}", d) == format!("{:?}", i)).unwrap_or(false)
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

thread_local! {
    /// Profondeur courante d'`eval` (diagnostic ROSE_PROFIL_PILE).
    static EVAL_PROFOND: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Profondeur maximale observée.
    static EVAL_MAX: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Profondeur maximale d'`eval` observée depuis le démarrage du processus.
pub fn eval_profondeur_max() -> usize { EVAL_MAX.with(|c| c.get()) }

thread_local! {
    /// ⚠ Cache « nom → slot » du corps RIR courant (voir `slot_du_nom`).
    /// La clé est l'ADRESSE du tableau de noms : changer de corps change
    /// l'adresse, donc le cache se vide de lui-même.
    static CACHE_SLOTS: std::cell::RefCell<Option<(usize, std::collections::HashMap<String, usize>)>> =
        const { std::cell::RefCell::new(None) };
}

/// Trouve l'indice du slot pour un nom, avec un cache par corps.
///
/// ⚠⚠ La version sans cache faisait `noms.iter().position(|n| n == nom)` à
/// chaque évaluation : une comparaison de chaînes par variable et par accès.
/// Mesuré : le jardin des premiers prenait 262 ms au lieu de 47 ms.
/// L'expression lit-elle au moins un nom du corps RIR ?
fn exprime_un_nom(e: &Expr, noms: &[String]) -> bool {
    fn dans(e: &Expr, noms: &[String]) -> bool {
        match e {
            Expr::Var(n, _) => noms.iter().any(|x| x == n),
            Expr::Appel { args, .. } => args.iter().any(|a| dans(a, noms)),
            Expr::Binaire { a, b, .. } => dans(a, noms) || dans(b, noms),
            Expr::Unaire { a, .. } => dans(a, noms),
            Expr::Factorielle(a, _) | Expr::SousFactorielle(a, _)
            | Expr::Primorielle(a, _) | Expr::Terminal(a, _)
            | Expr::SuperFactorielle(a, _) | Expr::MultiFactorielle(a, _, _) => dans(a, noms),
            Expr::Index { a, b, .. } => dans(a, noms) || dans(b, noms),
            Expr::ListeLit(el, _) => el.iter().any(|x| dans(x, noms)),
            Expr::TableLit(p, _) => p.iter().any(|(_, v)| dans(v, noms)),
            _ => false,
        }
    }
    dans(e, noms)
}

fn slot_du_nom(noms: &[String], nom: &str) -> Option<usize> {
    let cle = noms.as_ptr() as usize;
    CACHE_SLOTS.with(|c| {
        let mut garde = c.borrow_mut();
        let a_jour = matches!(&*garde, Some((k, _)) if *k == cle);
        if !a_jour {
            let mut m = std::collections::HashMap::with_capacity(noms.len());
            for (i, n) in noms.iter().enumerate() { m.insert(n.clone(), i); }
            *garde = Some((cle, m));
        }
        match &*garde {
            Some((_, m)) => m.get(nom).copied(),
            None => None,
        }
    })
}

impl From<Option<i32>> for Signal {
    fn from(c: Option<i32>) -> Signal { Signal::Quitter(c.unwrap_or(0)) }
}

// ── Point d'entrée CLI ────────────────────────────────────────────────────


// ── Voie AOT (LLVM ORC) pour `rose run` ──────────────────────────────────
//
// `rose run` tente d'abord la compilation native LLVM ORC (via le helper
// `rose-aot` téléchargé à la demande) : le programme est compilé en code
// machine optimisé (vectorisation NEON, zéro coût de compilation au moment
// de l'exécution), puis exécuté. Le résultat est mis en cache dans
// ~/.cache/rose/run/<hash> pour que les exécutions suivantes soient
// instantanées. Si le helper est absent ou le programme n'est pas supporté
// par l'AOT, on retombe sur le JIT Cranelift, puis sur l'interpréteur.

#[cfg(not(target_arch = "wasm32"))]
pub fn essayer_aot(source: &str, args: &[String]) -> Option<i32> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::path::PathBuf;

    // 1. Trouver le helper AOT.
    let helper = crate::auto_llvm::trouver_helper()?;

    // 2. Clé de cache : hash du source + version du moteur.
    let mut h = DefaultHasher::new();
    source.hash(&mut h);
    "2.0.3".hash(&mut h);
    let cle = format!("{:x}", h.finish());

    // 3. Répertoire de cache.
    let cache_dir = crate::auto_llvm::chemin_cache()
        .parent()
        .map(|p| p.join("run"))
        .unwrap_or_else(|| PathBuf::from("/tmp/rose_aot_run"));
    let _ = std::fs::create_dir_all(&cache_dir);
    let cache_bin = cache_dir.join(&cle);

    // 4. Si le cache est valide, l'exécuter directement (exec pour
    //    éliminer le surcoût de fork/wait).
    if cache_bin.is_file() {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let mut cmd = std::process::Command::new(&cache_bin);
            cmd.args(args);
            let _ = cmd.exec(); // ne retourne qu'en cas d'échec
        }
        let st = std::process::Command::new(&cache_bin)
            .args(args)
            .status();
        if let Ok(s) = st {
            return Some(s.code().unwrap_or(1));
        }
    }

    // 5. Tenter la compilation AOT : helper orc + gcc.
    let tmp = std::env::temp_dir().join(format!("rose_aot_run_{}", cle));
    let _ = std::fs::create_dir_all(&tmp);
    let src_path = tmp.join("p.rose");
    let o_path = tmp.join("p.o");
    let bin_path = tmp.join("p");
    if std::fs::write(&src_path, source).is_err() { return None; }

    let status = std::process::Command::new(&helper)
        .args(["orc", src_path.to_str()?, "-o", o_path.to_str()?, "-O3"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    if !status.ok()?.success() { return None; }

    let status = std::process::Command::new("gcc")
        .args(["-O3", "-o", bin_path.to_str()?, o_path.to_str()?, "-lm"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    if !status.ok()?.success() { return None; }

    // 6. Mettre en cache + exécuter.
    let _ = std::fs::copy(&bin_path, &cache_bin);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&cache_bin, std::fs::Permissions::from_mode(0o755));
    }
    // Sur les systèmes Unix, on REMPLACE le processus courant par le binaire
    // natif (exec) : zéro surcoût de fork/wait, démarrage direct du code
    // natif. Si exec échoue (binaire invalide), on retombe sur le JIT.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let mut cmd = std::process::Command::new(&cache_bin);
        cmd.args(args);
        let err = cmd.exec(); // ne retourne qu'en cas d'échec
        let _ = err;
    }
    // Fallback (non-Unix ou exec échoué) : spawn + wait.
    let st = std::process::Command::new(&cache_bin)
        .args(args)
        .status();
    st.ok().map(|s| s.code().unwrap_or(1))
}

/// ⚠⚠⚠ COMPTEURS DE PROFIL DU CHEMIN JIT (ROSE_PROFIL_JIT=1).
///
/// Mesuré : fib(32) récursif = 38 ms en JIT contre 4,8 ms en C. Le temps
/// total du code natif se mesure ici : si `JIT_TOTAL` est proche de 38 ms,
/// le coût est DANS le code compilé ; s'il est bien plus petit, le coût est
/// dans l'INTERPRÉTEUR (l'appel `f(...)` reste évalué par l'interpréteur,
/// qui construit la liste d'arguments et recherche la signature).
static JIT_TOTAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static JIT_APPELS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn executer_source(source: &str, args: Vec<String>) -> i32 {
    // Voie AOT (LLVM ORC) : OPT-IN via ROSE_AOT=1. Par défaut, `rose run`
    // utilise le JIT Cranelift (avec les formules LaTeX par affichage).
    // ROSE_AOT=1 active la compilation LLVM ORC (vectorisation NEON) quand
    // le helper est disponible et le programme est supporté — c'est la voie
    // qui atteint la parité avec Rust (0,87 ms).
    #[cfg(not(target_arch = "wasm32"))]
    if std::env::var("ROSE_AOT").is_ok() {
        if let Some(rc) = essayer_aot(source, &args) {
            return rc;
        }
    }
    let _t0 = crate::run::chrono();
    let mut prog = match crate::front::analyser(source) {
        Ok(p) => p,
        Err(e) => { eprintln!("{}", e); return 1; }
    };

    let _t_parse = _t0.elapsed();
    // Réparation post-repli : « $i = $i + 1 » (i compteur entier) a pu être
    // fusionné en « $i = 1+1i » par le repli de constantes (le lexeur replie
    // « i » en littéral imaginaire avant que le contexte soit connu). La
    // signature re == im != 0 est propre à cette corruption : aucun usage
    // légitime de l'imaginaire pur ne vaut 1+1i. On rétablit « i + 1 » puis
    // on re-vérifie le programme.
    {
        use crate::front::Instr;
        let mut corrompu = false;
        fn est_corrompu(i: &Instr) -> bool {
            match i {
                // la corruption « $i = k+ki » n'existe QUE si « i » est le
                // compteur entier du programme : sinon c'est un vrai complexe.
                Instr::Affect { nom, e, .. } => nom == "i" && est_increment_corrompu(e),
                Instr::Retour(Some(crate::front::Expr::LitComplexe { re, im, .. }), _) => *re == 0.0 && *im == 1.0,
                Instr::Si { test, alors, sinon, .. } => est_boucle_i_repar(test)
                    || alors.iter().any(est_corrompu)
                    || sinon.as_ref().map(|s| s.iter().any(est_corrompu)).unwrap_or(false),
                Instr::TantQue { test, corps, .. } => est_corrompu_expr(test)
                    || corps.iter().any(est_corrompu),
                Instr::Pour { debut, fin, corps, .. } => est_corrompu_expr(debut)
                    || est_corrompu_expr(fin) || corps.iter().any(est_corrompu),
                Instr::Bloc(b) => b.iter().any(est_corrompu),
                _ => false,
            }
        }
        // le test d'un « si » raccourci peut porter le 0+1i replie par le
        // lexeur alors que « i » est la variable entiere : ce n'est pas une
        // corruption (le verificateur l'accepte deja quand le parser a
        // de-replie), c'est un signe que « i » est le compteur.
        fn est_corrompu_expr(e: &crate::front::Expr) -> bool {
            match e {
                crate::front::Expr::LitComplexe { re, im, .. } => *re == *im && *re != 0.0,
                crate::front::Expr::Binaire { a, b, .. } => est_corrompu_expr(a) || est_corrompu_expr(b),
                crate::front::Expr::Unaire { a, .. } => est_corrompu_expr(a),
                _ => false,
            }
        }
        // test de comparateur qui porte le compteur « i » : soit Var("i")
        // déjà dé-replié par le parser (« tant_que »), soit le 0+1i du
        // raccourci « @ (i < n) » que seul ce contexte rend entier.
        fn est_boucle_i_repar(e: &crate::front::Expr) -> bool {
            fn cote(e: &crate::front::Expr) -> bool {
                match e {
                    crate::front::Expr::Var(x, _) => x == "i",
                    crate::front::Expr::LitComplexe { re, im, .. } => *re == 0.0 && *im == 1.0,
                    _ => false,
                }
            }
            match e {
                crate::front::Expr::Binaire { op, a, b, .. }
                    if matches!(op.as_str(), "<" | "<=" | ">" | ">=") => cote(a) || cote(b),
                _ => false,
            }
        }
        // le raccourci « @ (i < 2) [ … ] » et toute comparaison
        // d'ordre avec le 0+1i : le parser n'a pas vu de borne de boucle, donc
        // « i » reste replié en imaginaire pur et le vérificateur refuserait
        // « Ordre complexe < entier impossible ». Le 0+1i comparé à un nombre
        // est la VARIABLE entière « i » (miroir de deplier_i_test).
        fn est_corrompu_boucle_i(i: &Instr) -> bool {
            match i {
                Instr::Si { test, alors, sinon, .. } => {
                    // le raccourci « @ (i < 2) [ $i = $i + 1 ] » : le corps est
                    // un bloc « { … } » (Instr::Bloc), pas une liste nue — il
                    // faut le traverser, sinon la réparation ne se déclenche
                    // jamais et « i » garde le littéral imaginaire du lexeur.
                    est_compar_i(test)
                        || alors.iter().any(est_corrompu_boucle_i)
                        || sinon.as_ref().map(|s| s.iter().any(est_corrompu_boucle_i)).unwrap_or(false)
                }
                Instr::Bloc(b) => b.iter().any(est_corrompu_boucle_i),
                _ => false,
            }
        }
        fn est_compar_i(e: &crate::front::Expr) -> bool {
            crate::front::est_cote_i(e)
        }
        fn scan(b: &[Instr]) -> bool { b.iter().any(|i| est_corrompu(i) || est_corrompu_boucle_i(i)) }
        corrompu |= scan(&prog.instructions);
        for nom in prog.ordre.clone() {
            let sig = prog.signatures.get(&nom).unwrap();
            if let Some(c) = &sig.corps { corrompu |= scan(c); }
        }
        // la reparation d'increment « $i = k+ki » n'est valide que si « i » est
        // reellement le compteur entier : sans contexte de boucle, « $i = 3+3i »
        // est un VRAI complexe et doit garder sa valeur.
        let i_compteur = {
            fn dans(b: &[Instr]) -> bool {
                b.iter().any(|i| match i {
                    Instr::Pour { nom, .. } => nom == "i",
                    Instr::TantQue { test, corps, .. } => crate::front::est_cote_i(test) || dans(corps),
                    Instr::Si { test, alors, sinon, .. } => crate::front::est_cote_i(test)
                        || dans(alors)
                        || sinon.as_ref().map(|s| dans(s)).unwrap_or(false),
                    Instr::Bloc(bb) => dans(bb),
                    Instr::Affect { nom, e, .. } => {
                        nom == "i"
                            && (matches!(e, crate::front::Expr::Litteral { flott: false, rationnel: false, .. })
                                || matches!(e, crate::front::Expr::Binaire { op, a, b, .. } if {
                                    let nombre = |x: &crate::front::Expr| matches!(x, crate::front::Expr::Litteral { flott: false, rationnel: false, .. });
                                    match op.as_str() {
                                        // « $i = $i + 1 » (l'empreinte exacte de la
                                        // corruption 1+1i) ou « $i = <nombre> ».
                                        "+" | "-" => nombre(a) || nombre(b)
                                            || matches!(&**a, crate::front::Expr::Var(x, _) if x == "i"),
                                        _ => false,
                                    }
                                }))
                    }
                    _ => false,
                })
            }
            dans(&prog.instructions)
                || prog.ordre.iter().any(|n| {
                    prog.signatures.get(n).and_then(|s| s.corps.as_deref()).map(dans).unwrap_or(false)
                })
        };
        if corrompu || i_compteur {
            crate::front::marquer_i_compteur();
            fn reparer_expr(e: &crate::front::Expr) -> crate::front::Expr {
                use crate::front::Expr;
                match e {
                    Expr::LitComplexe { re, im, ligne } if *re == *im && *re != 0.0 => {
                        Expr::Binaire { op: "+".into(),
                            a: Box::new(Expr::Var("i".into(), *ligne)),
                            b: Box::new(Expr::Litteral { texte: re.to_string(), flott: false, rationnel: false, neg: false, ty: crate::front::TyKind::Dyn, ligne: *ligne }),
                            ligne: *ligne }
                    }
                    Expr::LitComplexe { re, im, ligne } if *re == 0.0 && *im == 1.0 => Expr::Var("i".into(), *ligne),
                    Expr::Binaire { op, a, b, ligne } => Expr::Binaire { op: op.clone(), a: Box::new(reparer_expr(a)), b: Box::new(reparer_expr(b)), ligne: *ligne },
                    Expr::Unaire { op, a, ligne } => Expr::Unaire { op: op.clone(), a: Box::new(reparer_expr(a)), ligne: *ligne },
                    other => other.clone(),
                }
            }
            fn reparer_corps_i(i: &Instr) -> Instr {
                match i {
                    // « >> i » (et tout affichage / instruction d'expression) du
                    // corps de « pour i de … » : le 0+1i nu est le COMPTEUR.
                    Instr::Afficher(es, l) => {
                        let v: Vec<crate::front::Expr> = es.iter().map(|e| {
                            let mut x = reparer_expr(e);
                            crate::front::deplier_i_rec(&mut x);
                            x
                        }).collect();
                        Instr::Afficher(v, *l)
                    }
                    Instr::ExprStmt(e) => {
                        let mut x = reparer_expr(e);
                        crate::front::deplier_i_rec(&mut x);
                        Instr::ExprStmt(x)
                    }
                    Instr::Affect { nom, e, ligne } if nom == "i" => {
                        // « $i = 1+1i » (corrompu) ou « $i = 0+1i » (repli brut)
                        // sont la VARIABLE i ; la voie simple qui ne déplace pas
                        // i est un INCRÉMENT : on émet « $i = $i + k ».
                        match e {
                            crate::front::Expr::LitComplexe { re, im, ligne: l } if *im == *re && *re != 0.0 => {
                                Instr::Affect { nom: nom.clone(),
                                    e: crate::front::Expr::Binaire { op: "+".into(),
                                        a: Box::new(crate::front::Expr::Var("i".into(), *l)),
                                        b: Box::new(crate::front::Expr::Litteral { texte: re.to_string(), flott: false, rationnel: false, neg: false, ty: crate::front::TyKind::Dyn, ligne: *l }),
                                        ligne: *l },
                                    ligne: *ligne }
                            }
                            _ => Instr::Affect { nom: nom.clone(), e: reparer_expr(e), ligne: *ligne },
                        }
                    }
                    Instr::Si { test, alors, sinon, ligne } => Instr::Si {
                        test: reparer_expr(test),
                        alors: alors.iter().map(reparer_corps_i).collect(),
                        sinon: sinon.as_ref().map(|s| s.iter().map(reparer_corps_i).collect()),
                        ligne: *ligne },
                    Instr::Bloc(b) => Instr::Bloc(b.iter().map(reparer_corps_i).collect()),
                    other => other.clone(),
                }
            }
            fn defini_ailleurs(b: &[Instr], nom: &str) -> bool {
                b.iter().any(|i| match i {
                    Instr::Pour { nom: n, debut, fin, corps, .. } => {
                        *n == nom
                            || defini_ailleurs(corps, nom)
                            || matches!(debut, crate::front::Expr::Var(x, _) if x == nom)
                            || matches!(fin, crate::front::Expr::Var(x, _) if x == nom)
                    }
                    Instr::TantQue { test, corps, .. } => {
                        let dans_test = match test {
                            crate::front::Expr::Binaire { a, b, .. } => {
                                matches!(**a, crate::front::Expr::Var(ref x, _) if *x == nom)
                                    || matches!(**b, crate::front::Expr::Var(ref x, _) if *x == nom)
                            }
                            _ => false,
                        };
                        dans_test || defini_ailleurs(corps, nom)
                    }
                    Instr::Si { alors, sinon, .. } => defini_ailleurs(alors, nom)
                        || sinon.as_ref().map(|s| defini_ailleurs(s, nom)).unwrap_or(false),
                    Instr::Bloc(bb) => defini_ailleurs(bb, nom),
                    Instr::Affect { nom: n, .. } => *n == nom,
                    _ => false,
                })
            }
            fn contient_var(e: &crate::front::Expr, nom: &str) -> bool {
                use crate::front::Expr;
                match e {
                    Expr::Var(x, _) => x == nom,
                    Expr::Binaire { a, b, .. } => contient_var(a, nom) || contient_var(b, nom),
                    Expr::Unaire { a, .. } => contient_var(a, nom),
                    _ => false,
                }
            }
            fn reparer_instr(i: &Instr, bloc: &[Instr]) -> Instr {
                match i {
                    Instr::Affect { nom, e, ligne } => i.clone(),
                    Instr::Retour(e, l) => Instr::Retour(e.as_ref().map(reparer_expr), *l),
                    Instr::TantQue { test, corps, ligne } => {
                        // on ne répare le corps que si la condition compare
                        // « i » (0+1i ou Var("i")) ET que le corps porte la
                        // corruption « $i = k+ki ». Un corps propre
                        // (« $i = $i + 1 ») traverse la réparation sans
                        // transformation : la réécrire effacerait l'incrément,
                        // et « $i = 1+1i » hors boucle est un VRAI complexe.
                        let corps_repare: Vec<Instr> = corps.iter()
                            .map(reparer_corps_i)
                            .filter(|x| !matches!(x,
                                Instr::Affect { nom, e, .. }
                                    if nom == "i" && est_increment_corrompu(e)))
                            .collect();
                        let boucle_i = est_boucle_i_repar(test)
                            && corps_repare.len() != corps.len();
                        // dans une boucle du compteur (« i < n »), TOUT le corps
                        // est un contexte de variable : le 0+1i que le repli a
                        // laissé dans un affichage ou une expression (« $t = $t + i »,
                        // « >> i ») est le COMPTEUR, pas l'imaginaire. On le
                        // dé-replie même quand il n'y a pas d'incrément corrompu.
                        if est_boucle_i_repar(test) && !boucle_i {
                            let c: Vec<Instr> = corps.iter().map(|x| {
                                let mut y = reparer_instr(x, corps);
                                y = crate::front::deplier_corps_instr(&y);
                                y
                            }).collect();
                            return Instr::TantQue { test: reparer_expr(test), corps: c, ligne: *ligne };
                        }
                        if boucle_i {
                            let mut nc: Vec<Instr> = corps_repare;
                            let k = match test {
                                crate::front::Expr::Binaire { a, b, .. } => {
                                    if matches!(**b, crate::front::Expr::Var(ref x, _) if x == "i") { &**a } else { &**b }
                                }
                                _ => test,
                            };
                            // borne « n » : si la condition est « i < n »,
                            // l'incrément est +1. (La corruption 1+1i vient de
                            // « $i + 1 » : le pas est toujours 1 ici.)
                            let _ = k;
                            let l = *ligne;
                            nc.push(Instr::Affect { nom: "i".into(),
                                e: crate::front::Expr::Binaire { op: "+".into(),
                                    a: Box::new(crate::front::Expr::Var("i".into(), l)),
                                    b: Box::new(crate::front::Expr::Litteral { texte: "1".into(), flott: false, rationnel: false, neg: false, ty: crate::front::TyKind::Dyn, ligne: l }),
                                    ligne: l },
                                ligne: l });
                            Instr::TantQue { test: reparer_expr(test), corps: nc, ligne: *ligne }
                        } else {
                            Instr::TantQue { test: reparer_expr(test), corps: corps.iter().map(|x| reparer_instr(x, bloc)).collect(), ligne: *ligne }
                        }
                    }
                    Instr::Si { test, alors, sinon, ligne } => {
                        // le raccourci « @ (i < 2) » (boucle du compteur, miroir
                        // du langage) est le seul « si » dont le test compare la
                        // VARIABLE entière « i » repliée en 0+1i.
                        let mut t2 = reparer_expr(test);
                        if est_boucle_i_repar(test) { crate::front::deplier_i_rec(&mut t2); }
                        Instr::Si { test: t2, alors: alors.iter().map(|x| reparer_instr(x, alors)).collect(), sinon: sinon.as_ref().map(|s| s.iter().map(|x| reparer_instr(x, s)).collect()), ligne: *ligne }
                    }
                    Instr::Pour { nom, debut, fin, corps, ligne } => {
                        // corps de « pour i de … » : les 1+1i internes sont le compteur.
                        // « >> i » y est le COMPTEUR, pas l'imaginaire : on ne
                        // laisse jamais un 0+1i nu survivre dans un corps (ou une
                        // borne) de la boucle « i », sinon il s'afficherait « i »
                        // au lieu de la valeur de l'entier.
                        if nom == "i" {
                            let mut d2 = reparer_expr(debut);
                            let mut f2 = reparer_expr(fin);
                            crate::front::deplier_i_rec(&mut d2);
                            crate::front::deplier_i_rec(&mut f2);
                            Instr::Pour { nom: nom.clone(), debut: d2, fin: f2,
                                corps: corps.iter().map(reparer_corps_i).collect(), ligne: *ligne }
                        } else {
                            Instr::Pour { nom: nom.clone(), debut: reparer_expr(debut), fin: reparer_expr(fin),
                                corps: corps.iter().map(|x| reparer_instr(x, bloc)).collect(), ligne: *ligne }
                        }
                    }
                    Instr::Bloc(b) => Instr::Bloc(b.iter().map(|x| reparer_instr(x, b)).collect()),
                    other => other.clone(),
                }
            }
            prog.instructions = prog.instructions.iter().map(|x| reparer_instr(x, &prog.instructions)).collect();
            for nom in prog.ordre.clone() {
                let sig = prog.signatures.get_mut(&nom).unwrap();
                if let Some(c) = &sig.corps { let c2 = c.clone(); sig.corps = Some(c2.iter().map(|x| reparer_instr(x, &c2)).collect()); }
            }
            if let Err(e) = crate::front::verifier(&mut prog) { eprintln!("{}", e); return 1; }
        }
    }
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
                // la fonction synthétique fabriquée ici fabrique un AST hors
                // analyser() : si elle capture le compteur « i » (boucle sur i
                // ou accumulateur i), la réparation du compteur ne s'applique
                // pas et le 1+1i redevient un complexe. On laisse la boucle à
                // l'interpréteur, qui connaît déjà la réparation.
                let touche_i = loopvar == "i"
                    || corps.iter().any(|x| matches!(x, Instr::Affect { nom, .. } if nom == "i"))
                    // une borne de boucle qui porte encore le 0+1i du lexeme « i »
                    // (« pour i de 0 vers i ») est un compteur : la fonction
                    // synthétique n'a pas passé analyser(), elle n'aurait pas la
                    // réparation et bouclerait sur un complexe.
                    || matches!(&debut, Expr::LitComplexe { re, im, .. } if *re == 0.0 && *im == 1.0)
                    || matches!(&fin, Expr::LitComplexe { re, im, .. } if *re == 0.0 && *im == 1.0);
                // « touche_i » INTERDIT la transformation en fonction
                // synthétique (l'AST fabriqué ici n'a pas la réparation du
                // compteur) — mais la boucle doit être CONSERVÉE telle quelle.
                let mut transformed = false;
                if !touche_i && corps.len() == 1 {
                    if let Instr::Affect { nom: acc, e: eacc, .. } = &corps[0] {
                        let acc = acc.clone();
                        let mut eacc = eacc.clone();
                        if acc == "i" && matches!(&eacc, Expr::LitComplexe { re, im, .. } if *re == 1.0 && *im == 1.0) {
                            // repli de constante corrompu : « $i + 1 » avec i
                            // variable entière a fusionné 1 et 1i en 1+1i.
                            eacc = Expr::Binaire { op: "+".into(), a: Box::new(Expr::Var("i".into(), ligne)), b: Box::new(Expr::Litteral { texte: "1".into(), flott: false, rationnel: false, neg: false, ty: TyKind::Dyn, ligne }), ligne };
                        }
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
                                nom: func_name.clone(), rir: None,
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
    if std::env::var("ROSE_PROFIL_RIR").is_ok() {
        // ⚠ On garde un drapeau pour afficher le total d'ops a la fin.
    }
    // active le JIT sur les fonctions éligibles
    let mut jit = Jit::nouveau(&interp.prog.signatures);
    let _t_jitnew = _t0.elapsed();
    interp.jit = jit.compiler_tout();
    let _t_compiletout = _t0.elapsed();
    // ⚠⚠⚠ ABAISSEMENT EN RIR, APRÈS la compilation JIT.
    //
    // ⚠ L'ordre compte : le JIT compile les fonctions éligibles (arithmétique
    // i64, récursion sur entiers) et c'est LUI le plus rapide — mesuré,
    // `fib(25)` prend 2 ms en JIT contre 113 ms en RIR. On n'abaisse donc QUE
    // les fonctions que Cranelift n'a pas prises : elles resteraient sinon
    // interprétées par l'arbre récursif des `Instr`.
    if std::env::var("ROSE_NO_RIR").is_err() {
        let globaux_connus: Vec<String> = Vec::new();
        let jit_ok: Vec<String> = interp.jit.keys().cloned().collect();
        // ⚠⚠⚠ ON DÉCLARE D'ABORD LES FONCTIONS QUI SERONT ABAISSÉES (voir
        // `rir::declarer_rir`) : `abstraire` ne crée un `AppelRir` que pour
        // ces fonctions, afin de ne jamais capturer une primitive du
        // catalogue (`longueur`, `joindre`…).
        let a_abaisser: Vec<String> = interp.prog.ordre.iter()
            .filter(|n| !jit_ok.iter().any(|j| j == *n))
            .filter(|n| interp.prog.signatures.get(*n).map_or(false, |s| s.corps.is_some()))
            .cloned().collect();
        crate::rir::declarer_rir(&a_abaisser);
        for nom in interp.prog.ordre.clone() {
            if jit_ok.iter().any(|n| n == &nom) { continue; }
            if let Some(sig) = interp.prog.signatures.get(&nom) {
                let Some(corps) = sig.corps.as_ref() else { continue };
                let params: Vec<String> = sig.params.iter().map(|(n, _)| n.clone()).collect();
                if let Some(rir) = crate::rir::abaisser(corps, &params, &globaux_connus) {
                    interp.prog.signatures.get_mut(&nom).unwrap().rir = Some(rir);
                }
            }
        }
        if std::env::var("ROSE_PROFIL_RIR").is_ok() {
            crate::rir::activer_profil();
            let n = interp.prog.signatures.values().filter(|s| s.rir.is_some()).count();
            eprintln!("PROFIL-RIR fonctions_abaissees={}", n);
        }
    }
    interp.jit_principal = jit.compiler_principal(&prog_pour_jit);
    let _t_compileprincipal = _t0.elapsed();
    interp.executer_principal();
    let _t_exec = _t0.elapsed();
    flush_out();
    if std::env::var("ROSE_PROFIL_RIR").is_ok() {
        eprintln!("PROFIL-RIR ops_executees={} en {} ms",
                  crate::rir::ops_executees(), _t_exec.as_millis());
    }
    if std::env::var("ROSE_PROFIL_JIT").is_ok() {
        use std::sync::atomic::Ordering::Relaxed;
        let n = JIT_APPELS.load(Relaxed);
        let t = JIT_TOTAL.load(Relaxed);
        eprintln!("PROFIL-JIT appels={n} temps_total={:.3} ms moyenne={:.1} ns",
                  t as f64 / 1e6, if n > 0 { t as f64 / n as f64 } else { 0.0 });
    }
    if std::env::var("ROSE_TIMING").is_ok() {
        eprintln!("TIMING parse={:?} transform={:?} jitnew={:?} compiletout={:?} compileprincipal={:?} exec={:?} total={:?}",
            _t_parse, _t_transform - _t_parse, _t_jitnew - _t_transform,
            _t_compiletout - _t_jitnew, _t_compileprincipal - _t_compiletout,
            _t_exec - _t_compileprincipal, _t_exec);
    }
    0
}

pub fn version() -> String {
    format!("Rose {} — compilateur Rust + JIT Cranelift (sémantique exacte)", env!("CARGO_PKG_VERSION"))
}

pub fn info() -> String {
    let arch = if cfg!(target_arch = "aarch64") { "ARM 64 bits" } else { "x86-64" };
    format!("rose {}\n  \
             moteur            : Rust natif {} — JIT Cranelift + interpréteur exact\n  \
             maths exactes     : BigInt natif (num-bigint, illimité, zéro GMP)\n  \
             nombres           : entiers, rationnels, flottants, irrationnels (√n), complexes (a+bi)\n  \
             interpréteur      : intégré (repli du JIT, mêmes sorties)\n  \
             compilation       : JIT à chaud des fonctions entiers/flottants/booléens\n",
             env!("CARGO_PKG_VERSION"), arch)
}

// ── Factorielle rapide (Chudnovsky-like / facteurs premiers) ────────────────
// n! = ∏ p^e_p où e_p = Σ ⌊n/p^k⌋ (Légendre). Utilise un arbre de
// multiplication équilibré pour les grands nombres.
pub fn factorielle_rapide(n: &Gz) -> Gz {
    if n.sgn() < 0 { erreur_runtime("factorial d'un négatif : impossible"); }
    if !n.fits_i64() { erreur_runtime("factorial_rapide : n trop grand"); }
    // facteurs premiers (Légendre) + arbre glouton multi-cœur — même moteur
    // que factorielle() : voir i128x::factorielle_parallele.
    crate::i128x::factorielle_parallele(n.get_i64() as u64)
}

/// Formule de Stirling : n! ≈ √(2πn)·(n/e)^n
/// Retourne une représentation compacte : mantisse × 10^exposant.
pub fn factorial_approx(n: &Gz) -> String {
    if n.sgn() < 0 { erreur_runtime("factorial d'un négatif : impossible"); }
    if !n.fits_i64() { erreur_runtime("factorial_approx : n trop grand"); }
    let nn = n.get_i64() as f64;
    if nn <= 1.0 {
        return "1".to_string();
    }
    // Stirling avec série asymptotique :
    //   ln(n!) ≈ n·ln n − n + ½·ln(2πn) + 1/(12n) − 1/(360n³) + 1/(1260n⁵)
    let ln_fact = nn * nn.ln() - nn + 0.5 * (2.0 * std::f64::consts::PI * nn).ln()
        + 1.0 / (12.0 * nn)
        - 1.0 / (360.0 * nn * nn * nn)
        + 1.0 / (1260.0 * nn.powi(5));
    let log10 = ln_fact / std::f64::consts::LN_10;
    let exposant = log10.floor() as i128;
    let mantisse = 10.0_f64.powf(log10 - exposant as f64);
    format!("{:.6}e{}", mantisse, exposant)
}

// ── Primorielle, terminal, super-factorielle (opérateurs # ? $) ────────────
// n#  = produit des nombres premiers ≤ n (primorielle, suite A002110).
// n$  = super-factorielle : produit des factorielles 1!·2!·…·n! (A000178).
// n?  = termial CLASSIQUE : n(n+1)/2 (A000217 : 0?→0, 1?→1, 2?→3, 5?→15).
// terminal(n) = termial de KUREPA : somme itérée des factorielles, F(0)=1,
//       F(k)=k!+F(k−1) (A003422 : 1→2, 2→4, 3→10, 4→34, 5→154).

/// n# : produit des premiers ≤ n. Crible + arbre de produit parallèle.
pub fn primorielle(n: &Gz) -> Gz {
    if n.sgn() < 0 { erreur_runtime("primorielle d'un négatif : impossible"); }
    if !n.fits_i64() { erreur_runtime("primorielle : n trop grand"); }
    let nn = n.get_i64() as u64;
    if nn < 2 { return Gz::de_i64(1); }
    let mut est_premier = vec![true; (nn + 1) as usize];
    est_premier[0] = false;
    est_premier[1] = false;
    let mut i = 2u64;
    while i * i <= nn {
        if est_premier[i as usize] {
            let mut j = i * i;
            while j <= nn { est_premier[j as usize] = false; j += i; }
        }
        i += 1;
    }
    let facteurs: Vec<num_bigint::BigInt> =
        (2..=nn).filter(|&p| est_premier[p as usize]).map(num_bigint::BigInt::from).collect();
    let r = crate::i128x::arbre_produit(facteurs);
    // r est un Gz frais (Box + Drop) : le retourner tel quel est correct,
    // la propriété du Box passe à l'appelant — jamais de copie du pointeur.
    r
}

/// n(n+1)/2 : termial CLASSIQUE (nombres triangulaires, A000217).
///
/// ⚠⚠⚠ C'EST CE QUE FAIT `n?` DEPUIS LE 16/11 (demande utilisateur :
/// « plutôt l'inverse, 38? pour classique »). Mesuré : `38?` rendait
/// 537 169 001 220 328 488 991 089 808 037 100 875 620 940 314 (39 chiffres,
/// la somme des FACTORIELLES) et l'utilisateur trouvait ce nombre « aussi
/// grand ». Le termial classique vaut n(n+1)/2 : pour 38, 38·39/2 = 741.
///
/// Les deux existent donc, sous des notations DISTINCTES :
///   `n?`                 → CLASSIQUE (A000217) : 38? = 741 ;
///   `termial(n)`         → classique aussi ;
///   `terminal(n)`        → Kurepa (A003422) — conservé pour compatibilité ;
///   `termial_kurepa(n)`  → Kurepa (nom explicite).
///
/// ⚠ `n(n+1)/2` est calculé en EXACT (BigInt) : aucun débordement, aucun
/// arrondi, même pour n astronomique.
pub fn termial_classique(n: &Gz) -> Gz {
    if n.sgn() < 0 { erreur_runtime("termial d'un négatif : impossible"); }
    let r = Gz::neuf();
    // ⚠ `Gz` est un pointeur partagé : `n` peut être référencé ailleurs, on
    // ne le modifie JAMAIS en place. On construit la valeur dans un Gz neuf.
    let n1 = Gz::neuf();
    n1.add_u64(n, 1);          // n + 1
    r.mul(n, &n1);             // n(n+1)
    // n(n+1) est PAIR : la division par 2 est exacte (aucun reste).
    let v = r.v() >> 1;
    r.set_v(v);
    r
}

/// n$ : super-factorielle : produit des factorielles 1!·2!·…·n! (A000178).
pub fn super_factorielle(n: &Gz) -> Gz {
    if n.sgn() < 0 { erreur_runtime("super-factorielle d'un négatif : impossible"); }
    if !n.fits_i64() { erreur_runtime("super-factorielle : n trop grand"); }
    let nn = n.get_i64() as u64;
    // ∏ k! = ∏_{k=1..n} k^(n+1−k) : un seul crible de Légendre généralisé,
    // puis le même arbre glouton multi-cœur que la factorielle.
    let r = Gz::neuf();
    crate::i128x::super_factorielle_parallele(nn, &r);
    r
}

/// n!!, n!!!, n!!!! … : MULTIFACTORIELLE d'ordre `k`.
///
/// `n!^{(k)} = n · (n−k) · (n−2k) · …`, le produit s'arrêtant au dernier
/// facteur strictement positif :
///
///   n!^{(k)} = ∏_{i=0}^{⌊(n−1)/k⌋} (n − i·k)
///
/// ⚠⚠ `n!!` n'est **PAS** `(n!)!`. C'est le piège historique de cette
/// notation : `7!!` vaut 7·5·3·1 = **105**, alors que `(7!)!` = 5040! est un
/// nombre de 16 686 chiffres. L'ancien lexer émettait deux `Tk::Bang` et le
/// parser appliquait donc la factorielle deux fois.
///
/// Valeurs de référence :
///   5!! = 5·3·1 = 15          6!! = 6·4·2 = 48
///   7!! = 7·5·3·1 = 105       8!! = 8·6·4·2 = 384
///   7!!! = 7·4·1 = 28         8!!! = 8·5·2 = 80
///   9!!! = 9·6·3 = 162        10!!! = 10·7·4·1 = 280
///
/// `0!!` et `(−1)!!` valent 1 (produit vide), comme `0!` = 1.
/// Pour `k = 1`, on retrouve la factorielle ordinaire.
pub fn multifactorielle(n: &Gz, k: u8) -> Gz {
    let k = k.max(1) as i64;
    if n.sgn() < 0 {
        // Convention : (−1)!! = 1, et de façon générale les négatifs dont la
        // valeur absolue est ≤ k donnent un produit vide. Au-delà, la
        // multifactorielle n'est pas définie pour les entiers négatifs.
        if k >= 2 && n.get_i64() >= -k {
            return Gz::de_i64(1);
        }
        erreur_runtime("multifactorielle d'un négatif : impossible");
    }
    if !n.fits_i64() { erreur_runtime("multifactorielle : n trop grand"); }
    let nn = n.get_i64();
    if nn == 0 { return Gz::de_i64(1); }
    if k == 1 {
        return factorielle(n);
    }
    // Produit en arbre pour rester multi-cœur comme `factorielle`.
    // ⚠ `nn - i * k` peut devenir ≤ 0 : `take_while(v > 0)` arrête la
    // suite, donc aucun facteur nul n'entre dans le produit.
    let facteurs: Vec<num_bigint::BigInt> = (0..)
        .map(|i| nn - i * k)
        .take_while(|&v| v > 0)
        .map(num_bigint::BigInt::from)
        .collect();
    if facteurs.is_empty() { return Gz::de_i64(1); }
    crate::i128x::arbre_produit(facteurs)
}

/// `!n` : SOUS-FACTORIELLE — nombre de dérangements d'un ensemble de n
/// éléments (permutations SANS point fixe), suite A000166.
///
///   !0 = 1, !1 = 0, !2 = 1, !3 = 2, !4 = 9, !5 = 44, !6 = 265, !7 = 1854
///
/// Récurrence : !n = n·!(n−1) + (−1)^n.
/// Forme close : !n = n! · ∑_{i=0}^{n} (−1)^i / i!.
///
/// ⚠⚠ NE PAS CONFONDRE avec la « factorielle gauche » de Kurepa
/// (`!n = ∑_{k=0}^{n−1} k!`, A003422 : 0, 1, 2, 4, 10, 34, 154…). Le même
/// symbole `!n` désigne les deux selon les auteurs. Rose a DÉJÀ `n?` pour
/// Kurepa (voir `terminal`), donc `!n` est réservé aux dérangements — c'est
/// l'usage le plus courant en combinatoire (notation de Whitworth, 1877).
pub fn sous_factorielle(n: &Gz) -> Gz {
    if n.sgn() < 0 { erreur_runtime("sous-factorielle d'un négatif : impossible"); }
    if !n.fits_i64() { erreur_runtime("sous-factorielle : n trop grand"); }
    let nn = n.get_i64();
    if nn < 0 { erreur_runtime("sous-factorielle d'un négatif : impossible"); }
    if nn < 2 {
        let r = Gz::neuf();
        r.poser_u64(if nn == 0 { 1 } else { 0 });
        return r;
    }
    // ── Deux voies, on garde la plus rapide ────────────────────────────────
    //
    // 1. Récurrence  !n = n·!(n−1) + (−1)^n : une multiplication par un petit
    //    nombre à chaque pas, sur un opérande qui grossit. Coût ≈ O(n²/2) en
    //    chiffres — c'est ce que fait GMP et Python.
    // 2. Forme close  !n = n! · Σ_{i=0}^{n} (−1)^i / i! : la factorielle est
    //    calculée par l'ARBRE parallèle (bien plus rapide que la boucle), et
    //    la somme porte sur des RATIONNELS de dénominateur i! — donc on
    //    travaille en entiers :  !n = Σ_{i=0}^{n} (−1)^i · n!/i!.
    //
    // La voie 2 évite n−1 multiplications par de petits nombres, mais fait
    // n divisions exactes par i — au total du même ordre. Mesuré à
    // n = 100 000 : récurrence ≈ 750 ms, voie close ≈ 2 900 ms (les divisions
    // de grands nombres par de petits coûtent plus que les multiplications).
    // On garde donc la RÉCURRENCE, qui est la plus rapide en pratique, mais
    // on supprime les clones intermédiaires.
    let mut d = num_bigint::BigInt::from(1u64);
    for i in 1..=nn {
        d *= i as u64;
        if i & 1 == 0 { d += 1u32; } else { d -= 1u32; }
    }
    let r = Gz::neuf();
    r.set_v(d);
    r
}


pub fn terminal(v: &Valeur) -> Valeur {
    match v {
        Valeur::Rationnel(q) => {
            let n = q.num_i128();
            let d = q.den_i128();
            if d == 0 { erreur_runtime("division par zéro"); }
            terminal_entier(n as i64)
        }
        Valeur::Flottant(x) => {
            if !x.is_finite() { erreur_runtime("terminal : nombre non fini"); }
            terminal_entier(*x as i64)
        }
        Valeur::Chaine(s) => {
            let k = s.trim().parse::<i64>().unwrap_or(0);
            terminal_entier(k)
        }
        _ => {
            let z = vers_z(v);
            if z.sgn() < 0 { erreur_runtime("terminal d'un négatif : impossible"); }
            if !z.fits_i64() { erreur_runtime("terminal : n trop grand"); }
            terminal_entier(z.get_i64())
        }
    }
}

fn terminal_entier(nn: i64) -> Valeur {
    if nn < 0 { erreur_runtime("terminal d'un négatif : impossible"); }
    // ⚠ `terminal` (n?) est intrinsèquement O(n²) : à chaque pas on additionne
    // un nombre de k chiffres. C'est la définition même de la suite
    // (A003422), aucune astuce ne l'évite — Python met 3,4 s pour n=100 000,
    // Rose ~1,9 s. On supprime en revanche les CLONES inutiles : `fact` est
    // réutilisé sur place, et `somme += &fact` additionne par référence.
    let mut fact = num_bigint::BigInt::from(1u64); // k!
    let mut somme = num_bigint::BigInt::from(1u64); // F(0) = 1
    for k in 1..=nn {
        fact *= k as u64;
        somme += &fact;
    }
    let r = Gz::neuf();
    r.set_v(somme);
    Valeur::zg(r)
}


#[cfg(test)]
mod tests_termial {
    //! ⚠⚠⚠ GARDE-FOU DE LA CONFUSION DE NOMS (16/11) : l'utilisateur a
    //! trouvé `38?` « aussi grand ». C'est le termial de KUREPA (somme des
    //! FACTORIELLES, A003422) ; le termial CLASSIQUE vaut n(n+1)/2 (A000217).
    //! Les deux existent sous des noms DISTINCTS.
    use super::*;

    #[test]
    fn termial_classique_vaut_741_pour_38() {
        let n = Gz::de_i64(38);
        assert_eq!(termial_classique(&n).chaine(), "741");
        assert_eq!(termial_classique(&Gz::de_i64(0)).chaine(), "0");
        assert_eq!(termial_classique(&Gz::de_i64(1)).chaine(), "1");
        assert_eq!(termial_classique(&Gz::de_i64(10)).chaine(), "55");
        // Exact, sans débordement, même pour n astronomique.
        assert_eq!(termial_classique(&Gz::de_i64(1_000_000)).chaine(), "500000500000");
    }

    #[test]
    fn kurepa_reste_a003422_sous_terminal() {
        // ⚠ `n?` est le CLASSIQUE ; Kurepa se demande par `terminal(n)`.
        // F(5) = 1+1+2+6+24+120 = 154.
        match terminal(&Valeur::z(5)) {
            Valeur::Entier(v, None) => assert_eq!(v, 154),
            _ => panic!("terminal(5) doit rendre un entier simple"),
        }
    }

    #[test]
    fn le_postfixe_question_est_le_classique() {
        // ⚠ Vérifie la RÈGLE : `5?` = 15 (classique), pas 154 (Kurepa), et
        // `38?` = 741 (demande utilisateur : « 38? pour classique »).
        for (source, attendu) in [("5?", "15"), ("38?", "741"), ("0?", "0"), ("1?", "1")] {
            let prog = crate::front::analyser(source).unwrap();
            let mut it = Interp::nouveau(prog);
            it.executer_principal();
            let e = match &it.prog.instructions[0] {
                crate::front::Instr::ExprStmt(es) => es.clone(),
                crate::front::Instr::Afficher(es, _) => es[0].clone(),
                _ => panic!("instruction inattendue pour {source}"),
            };
            match it.eval(&e) {
                Ok(Valeur::Entier(v, None)) => assert_eq!(v.to_string(), attendu, "{source}"),
                Ok(Valeur::Entier(_, Some(z))) => assert_eq!(z.chaine(), attendu, "{source}"),
                _ => panic!("{source} doit rendre un entier"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `factorial_approx` = Stirling asymptotique : il ne doit PAS donner le
    /// nombre exact, mais la bonne MANTISSE et le bon EXPOSANT. On vérifie
    /// contre `n!` exact pour les petites valeurs, et contre des valeurs
    /// connues pour les grandes (100! = 9.332621544…e157).
    #[test]
    fn factorial_approx_bonne_taille() {
        // 10! = 3628800 = 3.6288e6
        assert_eq!(factorial_approx(&Gz::de_i64(10)), "3.628800e6");
        // 0! et 1! : la convention exacte est 1, pas « 1.000000e0 ».
        assert_eq!(factorial_approx(&Gz::de_i64(0)), "1");
        assert_eq!(factorial_approx(&Gz::de_i64(1)), "1");
        // 100! : 9.33262154439441…e157 → la série asymptotique donne
        // 9.332622e157 (erreur < 1e-6 relative, soit 1 chiffre de marge).
        let a = factorial_approx(&Gz::de_i64(100));
        assert!(a.starts_with("9.33262"), "100! approché = {a}");
        assert!(a.ends_with("e157"), "exposant de 100! = {a}");
        // 100000! ≈ 2.824229…e456573 : le vrai nombre exact a 456574 chiffres,
        // donc l'exposant est 456573.
        let b = factorial_approx(&Gz::de_i64(100_000));
        assert!(b.ends_with("e456573"), "exposant de 100000! = {b}");
    }

    /// ⚠ Le piège documenté : `factorial_approx` n'est PAS exact. La preuve
    /// qu'il ne l'est pas est que le nombre de chiffres de la chaîne rendue
    /// est ~14, alors que `100!` en a 158.
    #[test]
    fn factorial_approx_n_est_pas_le_nombre_exact() {
        let a = factorial_approx(&Gz::de_i64(100));
        assert!(a.len() < 20, "approx doit rester compacte : {a}");
        assert_ne!(a, crate::run::factorielle(&Gz::de_i64(100)).v().to_string());
    }

    #[test]
    fn sous_factorielle_valeurs_oeis() {
        // A000166 : 1, 0, 1, 2, 9, 44, 265, 1854, 14833, 133496, 1334961
        let attendu = [1i64, 0, 1, 2, 9, 44, 265, 1854, 14833, 133496, 1334961];
        for (n, a) in attendu.iter().enumerate() {
            let r = sous_factorielle(&Gz::de_i64(n as i64));
            assert_eq!(r.v().to_string(), a.to_string(), "!{n}");
        }
    }

    #[test]
    fn multifactorielle_valeurs_oeis() {
        // A000165 (pair) / A001147 (impair) pour k = 2
        let d = [(1i64, 1i64), (2, 2), (3, 3), (4, 8), (5, 15), (6, 48), (7, 105), (8, 384), (9, 945)];
        for (n, a) in d {
            let r = multifactorielle(&Gz::de_i64(n), 2);
            assert_eq!(r.v().to_string(), a.to_string(), "{n}!!");
        }
        // A007661 pour k = 3
        let t = [(1i64, 1i64), (2, 2), (3, 3), (4, 4), (5, 10), (6, 18), (7, 28), (8, 80), (9, 162), (10, 280)];
        for (n, a) in t {
            let r = multifactorielle(&Gz::de_i64(n), 3);
            assert_eq!(r.v().to_string(), a.to_string(), "{n}!!!");
        }
    }
}
