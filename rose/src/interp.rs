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
        match i {
            Instr::Afficher(e, _) => { let v = self.eval(e)?; afficher(&v); }
            Instr::Lire { nom, ty, .. } => {
                let ty_resolu = if *ty == TyKind::Dyn {
                    self.chercher_var(nom).map(|v| v.ty()).unwrap_or(TyKind::Entier)
                } else { *ty };
                let v = self.lire_selon(ty_resolu);
                self.affecter(nom, v);
            }
            Instr::Affect { nom, e, .. } => { let v = self.eval(e)?; self.affecter(nom, v); }
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
            Expr::Var(n, _) => match self.cible(n) {
                Some(v) => Ok(v),
                None => { erreur_runtime_ligne(&format!("Variable '{}' utilisée sans valeur", n)); }
            },
            Expr::ListeLit(el, _) => {
                let mut v = Vec::new();
                for x in el { v.push(self.eval(x)?); }
                Ok(Valeur::liste_de(v))
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
                let h = dlopen(cs.as_ptr() as *const i8, 2);
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
        let s = unsafe { dlsym(h, cs.as_ptr() as *const i8) };
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
                    let h = dlopen(c.as_ptr() as *const i8, 2);
                    if h.is_null() {
                        erreur_runtime(&format!("chargement de bibliothèque impossible ({})", s));
                    }
                    self.poignees.push(h);
                    Ok(Valeur::z(h as i64))
                }
            }
            "fermer" => {
                let h = extraire_i64(&args[0]) as *mut std::ffi::c_void;
                unsafe { dlclose(h) };
                Ok(Valeur::Vide)
            }
            "appeler0" | "appeler1" | "appeler2" => {
                let h = extraire_i64(&args[0]) as *mut std::ffi::c_void;
                let sym = chaine_de(&args[1]);
                let c = std::ffi::CString::new(sym.clone()).unwrap();
                unsafe {
                    let p = dlsym(h, c.as_ptr() as *const i8);
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
        Ok(self.stdlib_valeur(nom, a))
    }

    fn stdlib_valeur(&mut self, nom: &str, a: &[Valeur]) -> Valeur {
        match nom {
            "afficher" => { afficher(&a[0]); Valeur::Vide }
            "saisir_entier" => self.lire_selon(TyKind::Entier),
            "saisir_flottant" => self.lire_selon(TyKind::Flottant),
            "millisecondes" => {
                let t = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
                let z = Gz::neuf();
                z.poser_u64(t);
                Valeur::zg(z)
            }
            "graine" => { let z = vers_z(&a[0]); graine_de(&mut self.rng, &z); Valeur::Vide }
            "graine_systeme" | "graine_vrai" => {
                self.rng = tirage_systeme();
                rng_tirer(&mut self.rng);
                Valeur::Vide
            }
            "rng_materiel" | "rng_mat" => {
                let z = Gz::neuf();
                z.poser_u64(tirage_systeme());
                Valeur::zg(z)
            }
            "hasard" => {
                let n = vers_z(&a[0]);
                let r = hasard(&mut self.rng, &n);
                Valeur::zg(r)
            }
            "hasard_systeme" | "hasard_vrai" => {
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
            "tampon" => {
                let n = vers_z(&a[0]);
                if n.sgn() < 0 || !n.fits_u64_lt(usize::MAX as u64) { erreur_runtime("tampon : taille démesurée"); }
                Valeur::tampon(n.get_u64() as usize)
            }
            "octets" => {
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
            "factorielle" => { let z = vers_z(&a[0]); Valeur::zg(factorielle(&z)) }
            "fibonacci" => { let z = vers_z(&a[0]); Valeur::zg(fibonacci(&z)) }
            "pgcd" => { let (x, y) = (vers_z(&a[0]), vers_z(&a[1])); Valeur::zg(pgcd(&x, &y)) }
            "ppcm" => { let (x, y) = (vers_z(&a[0]), vers_z(&a[1])); Valeur::zg(ppcm(&x, &y)) }
            "puissance" => {
                let e = vers_z(&a[1]);
                puissance(&a[0], &e)
            }
            "est_premier" => { let z = vers_z(&a[0]); Valeur::b(est_premier(&z)) }
            "compter_premiers" => { let z = vers_z(&a[0]); Valeur::z(compter_premiers(&z) as i64) }
            "nb_chiffres" => { let z = vers_z(&a[0]); Valeur::z(nb_chiffres(&z) as i64) }
            "nombre_pi" => { let z = vers_z(&a[0]); Valeur::s(nombre_pi(&z)) }
            "longueur" => {
                let n: u64 = match &a[0] {
                    Valeur::Chaine(s) => s.chars().count() as u64,
                    Valeur::Liste(l) => l.0.borrow().len() as u64,
                    Valeur::Table(t) => t.0.borrow().len() as u64,
                    Valeur::Tampon(t) => t.0.borrow().len() as u64,
                    _ => erreur_runtime("longueur attend une liste, chaîne, table ou tampon"),
                };
                let z = Gz::neuf();
                z.poser_u64(n);
                Valeur::zg(z)
            }
            "table" => Valeur::table(),
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
            "sous_chaine" => {
                let s = chaine_de(&a[0]);
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
                //separer l'expression nue du contexte eventuel
                let pos_vars = if let Some(i) = base.find("// vars:") {
                    Some((i, 8usize))
                } else if let Some(i) = base.find("; vars:") {
                    Some((i, 7usize))
                } else {
                    None
                };
                let (nu, vars_brut) = match pos_vars {
                    Some((i, saut)) => (
                        base[..i].trim().to_string(),
                        base[i + saut..].trim().to_string(),
                    ),
                    None => (base.clone(), String::new()),
                };
                // 1) litteral : une seule paire de guillemets englobante
                if nu.len() >= 2
                    && nu.starts_with('"')
                    && nu.ends_with('"')
                    && nu[1..nu.len() - 1].find('"').is_none()
                {
                    return Valeur::s(nu[1..nu.len() - 1].to_string());
                }
                // 2) atome nu : booleen ou nombre
                if nu == "vrai" || nu == "faux" || nu.parse::<i64>().is_ok() {
                    return Valeur::s(nu.clone());
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
                                crate::front::Instr::Afficher(e, _) => {
                                    interp.executer_principal();
                                    if let Ok(v) = interp.eval(&e) {
                                        rendu = Valeur::s(chaine_de(&v));
                                        obtenu = true;
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
    let mut prog = match crate::front::analyser(source) {
        Ok(p) => p,
        Err(e) => { eprintln!("{}", e); return 1; }
    };
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
    let mut interp = Interp::nouveau(prog).avec_arguments(args);
    // active le JIT sur les fonctions éligibles
    let mut jit = Jit::nouveau(&interp.prog.signatures);
    interp.jit = jit.compiler_tout();
    interp.jit_principal = jit.compiler_principal(&prog_pour_jit);
    interp.executer_principal();
    flush_out();
    0
}

pub fn version() -> String {
    "Rose 2.0 — compilateur Rust + JIT Cranelift (sémantique du moteur C++)".to_string()
}

pub fn info() -> String {
    let arch = if cfg!(target_arch = "aarch64") { "ARM 64 bits" } else { "x86-64" };
    format!("rose 2.0\n  \
             moteur            : Rust natif {} — JIT Cranelift + interpréteur exact\n  \
             maths exactes     : BigInt natif (num-bigint, illimité, zéro GMP)\n  \
             interpréteur      : intégré (repli du JIT, mêmes sorties)\n  \
             compilation       : JIT à chaud des fonctions entiers/flottants/booléens\n", arch)
}
