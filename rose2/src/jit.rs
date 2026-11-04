// Rose 2.0 — JIT Cranelift : émet du code machine natif pour le code Rose
// « chaud » (entiers 64 bits, booléens, flottants, comparaisons, branches,
// boucles, fonctions utilisateur avec récursion).
// Comme le JIT du moteur C++ : TOUT ce qui sort du support (chaînes,
// rationnels, listes, tables, tampons, FFI, appels runtime) fait basculer la
// fonction entière sur l'interpréteur — repli transparent, sorties identiques.
//
// Convention d'appel SystemV (ARM aarch64) :
//   une fonction Rose compiled reçoit ses arguments dans x0..x7 (i64) et
//   d0..d7 (f64), et rend dans x0 / d0. Les scalaires sont alloués dans une
//   trame de frame par Cranelift (stack_store/load natifs).
// Licence MIT.

use std::collections::HashMap;

use cranelift::codegen::ir::{AbiParam, BlockArg, FuncRef, Inst, InstBuilder, MemFlagsData, Signature as ClSignature, Value};
use cranelift::codegen::isa::CallConv;
use cranelift::module::FuncId;
use cranelift::jit::{JITBuilder, JITModule};
use cranelift::module::{default_libcall_names, Linkage, Module};
use cranelift::prelude::{Block, FloatCC, FunctionBuilder, IntCC, types, Variable};

use crate::front::{Expr, Instr, Signature, TyKind};

/// Vrai si la séquence d'instructions finit toujours par un retour (le code
/// qui suit est mort) — sert à ne pas réémettre un return final illégal.
use crate::run::Valeur;

/// Une fonction compilée : soit du code natif, soit repli interprété.
pub enum Compile {
    Natif(Box<dyn Fn(Vec<Valeur>) -> Valeur + Send + Sync>),
    /// Corps du programme principal compilé (pas d'arguments, pas de retour).
    Principal(Box<dyn Fn() + Send + Sync>),
    #[allow(dead_code)]
    Interprete,
}

/// Analyse : une expression est-elle compilable (entier i64 / booleen /
/// flottant, sans effet de bord sur des types hors support) ?
pub fn compilable(ty: TyKind) -> bool {
    matches!(ty, TyKind::Entier | TyKind::Booleen | TyKind::Flottant)
}

/// Une addition/soustraction entiere est sure en i64 seulement si elle ne peut
/// pas deborder : litteral petit (0, 1 — le pas des boucles) ou deux litteraux
/// (deja replies). Toute autre somme passe par l'interprete (GMP exacte).
fn somme_sure(a: &Expr, b: &Expr) -> bool {
    fn petit(e: &Expr) -> bool {
        match e {
            Expr::Litteral { flott, texte, .. } => {
                if *flott || texte.contains('/') { return false; }
                matches!(texte.as_str(), "0" | "1")
            }
            _ => false,
        }
    }
    petit(a) || petit(b)
}


/// Le type statique d'une expression, dans le sous-ensemble compilable.
/// Retourne None si l'expression sort du support.
fn ty_statique(e: &Expr, env: &HashMap<String, TyKind>) -> Option<TyKind> {
    match e {
        Expr::Litteral { flott, texte, .. } => {
            if *flott { Some(TyKind::Flottant) }
            else if texte.contains('/') { None }
            else { Some(TyKind::Entier) }
        }
        Expr::BooleenLit(..) => Some(TyKind::Booleen),
        Expr::Var(n, _) => env.get(n).copied().filter(|t| compilable(*t)),
        Expr::Appel { nom, module, args, .. } => {
            // appel compilable si : pas de module, args compilables, et la
            // signature connue est dans le sous-ensemble JIT (miroir de la
            // porte du JIT C++ qui n'accepte que entier/flottant/booleen).
            if !module.is_empty() { return None; }
            for a in args { if ty_statique(a, env).is_none() { return None; } }
            PROG_AVEC.with(|p| p.borrow().get(nom).cloned()).and_then(|s| {
                if compilable(s.retour.n) || s.retour.n == TyKind::Vide { Some(s.retour.n) } else { None }
            })
        }
        Expr::Unaire { op, a, .. } => {
            if op == "non" {
                if ty_statique(a, env)? == TyKind::Booleen { Some(TyKind::Booleen) } else { None }
            } else { ty_statique(a, env) }
        }
        Expr::Binaire { op, a, b, .. } => {
            match op.as_str() {
                "et" | "ou" => {
                    if ty_statique(a, env)? == TyKind::Booleen && ty_statique(b, env)? == TyKind::Booleen {
                        Some(TyKind::Booleen)
                    } else { None }
                }
                "==" | "!=" | "<" | "<=" | ">" | ">=" => {
                    let ta = ty_statique(a, env)?;
                    let tb = ty_statique(b, env)?;
                    if ta == tb || (compilable(ta) && compilable(tb)) { Some(TyKind::Booleen) } else { None }
                }
                "div" => { ty_statique(a, env)?; ty_statique(b, env)?; Some(TyKind::Entier) }
                "+" | "-" | "*" => {
                    let ta = ty_statique(a, env)?;
                    let tb = ty_statique(b, env)?;
                    match (ta, tb) {
                        (TyKind::Flottant, _) | (_, TyKind::Flottant) => Some(TyKind::Flottant),
                        (TyKind::Entier, TyKind::Entier) => Some(TyKind::Entier),
                        (TyKind::Booleen, TyKind::Booleen) => Some(TyKind::Entier),
                        _ => None,
                    }
                }
                "%" => { ty_statique(a, env)?; ty_statique(b, env)?; Some(TyKind::Entier) }
                "^" => {
                    let ta = ty_statique(a, env)?;
                    let tb = ty_statique(b, env)?;
                    match (ta, tb) {
                        (TyKind::Flottant, _) | (_, TyKind::Flottant) => Some(TyKind::Flottant),
                        (TyKind::Entier, TyKind::Entier) => Some(TyKind::Entier),
                        _ => None,
                    }
                }
                "/" => {
                    let ta = ty_statique(a, env)?;
                    let tb = ty_statique(b, env)?;
                    match (ta, tb) {
                        (TyKind::Flottant, TyKind::Flottant) => Some(TyKind::Flottant),
                        _ => None, // entier/entier → rationnel : hors support
                    }
                }
                _ => None,
            }
        }
        _ => None,
    }
}

thread_local! {
    static PROG_AVEC: std::cell::RefCell<HashMap<String, Signature>> =
        std::cell::RefCell::new(HashMap::new());
}

/// Vérifie qu'une fonction entière tient dans le sous-ensemble JIT.
pub fn fonction_compilable(sig: &Signature, prog: &HashMap<String, Signature>) -> bool {
    if !compilable(sig.retour.n) && sig.retour.n != TyKind::Vide { return false; }
    // un corps sans « retour » dans une fonction non-vide remonterait Vide :
    // la valeur de retour n'a pas de sens JIT, on garde l'interpréteur.
    if sig.retour.n != TyKind::Vide {
        let corps = sig.corps.as_deref().unwrap_or(&[]);
        if !contient_retour(corps) { return false; }
    }
    let mut env: HashMap<String, TyKind> = HashMap::new();
    for (pn, pt) in &sig.params {
        if !compilable(pt.n) { return false; }
        env.insert(pn.clone(), pt.n);
    }
    let attendu = match sig.retour.n {
        TyKind::Vide => None,
        t => Some(t),
    };
    PROG_AVEC.with(|p| { *p.borrow_mut() = prog.clone(); });
    let r = bloc_compilable(sig.corps.as_deref().unwrap_or(&[]), &mut env, prog, attendu);
    PROG_AVEC.with(|p| { p.borrow_mut().clear(); });
    r
}

fn contient_retour(instrs: &[Instr]) -> bool {
    instrs.iter().any(|i| match i {
        Instr::Retour(_, _) => true,
        Instr::Si { alors, sinon, .. } => {
            contient_retour(alors) || sinon.as_deref().map(contient_retour).unwrap_or(false)
        }
        Instr::TantQue { corps, .. } | Instr::Pour { corps, .. } => contient_retour(corps),
        Instr::Bloc(b) => contient_retour(b),
        _ => false,
    })
}

pub fn bloc_compilable(instrs: &[Instr], env: &mut HashMap<String, TyKind>,
                   prog: &HashMap<String, Signature>, attendu: Option<TyKind>) -> bool {

    // Sous-ensemble i64 STRICT : la multiplication et la puissance entières
    // sont exclues, car le moteur C++ promeut en GMP dès que le produit
    // dépasse int64 (rose_ent) alors que Cranelift wrappe. Une fonction qui
    // multiplie des entiers reste donc interprétée (parité exacte).
    fn multiplicatif(e: &Expr) -> bool {
        match e {
            Expr::Binaire { op, a, b, .. } => {
                let m = match op.as_str() { "*" | "^" => true, _ => false };
                m || multiplicatif(a) || multiplicatif(b)
            }
            Expr::Unaire { a, .. } => multiplicatif(a),
            _ => false,
        }
    }
    fn sur_i64(e: &Expr, env: &HashMap<String, TyKind>) -> bool {
        match e {
            Expr::Binaire { op, a, b, .. } => {
                if matches!(op.as_str(), "≈") { return false; }
                if matches!(op.as_str(), "==" | "!=" | "<" | "<=" | ">" | ">=" | "et" | "ou") {
                    return true;
                }
                let f = |x: &Expr| -> bool {
                    match x {
                        Expr::Litteral { flott: true, .. } => true,
                        Expr::Var(n, _) => matches!(env.get(n), Some(TyKind::Flottant)),
                        _ => false,
                    }
                };
                if matches!(op.as_str(), "*" | "^") {
                    if matches!(op.as_str(), "^") {
                        if f(a) || f(b) { return sur_i64(a, env) && sur_i64(b, env); }
                        return false; // ^ entier reste interprete (degre inconnu)
                    }
                    // * entier : voie taggee (promotion BigInt au depassement)
                    return sur_i64(a, env) && sur_i64(b, env);
                }
                if matches!(op.as_str(), "+" | "-") && !f(a) && !f(b) {
                    // + / - entiers : voie taggee (promotion BigInt au depassement)
                    return true;
                }
                return sur_i64(a, env) && sur_i64(b, env);
            }
            Expr::Unaire { op, a, .. } => {
                if op == "-" {
                    // negation entiere : voie taggee
                    return sur_i64(a, env);
                }
                sur_i64(a, env)
            }
            Expr::Litteral { flott, texte, .. } => {
                if *flott || texte.contains('/') { return true; }
                // un entier déjà replié (ex. produit constant) qui ne tient pas
                // en i64 vit en GMP : hors trame i64 -> hors sous-ensemble.
                texte.parse::<i64>().is_ok()
            }
            _ => true,
        }
    }
    let _ = multiplicatif;
    for i in instrs {

        match i {
            Instr::Afficher(e, _) | Instr::ExprStmt(e) => {
                if ty_statique(e, env).is_none() || !sur_i64(e, env) { return false; }
            }
            Instr::Affect { nom, e, .. } => {
                let t = match ty_statique(e, env) { Some(t) => t, None => return false };
                if !sur_i64(e, env) { return false; }
                if let Some(ancien) = env.get(nom) {
                    if *ancien != t && !compilable(t) { return false; }
                }
                env.insert(nom.clone(), t);
            }
            Instr::Si { test, alors, sinon, .. } => {
                if ty_statique(test, env) != Some(TyKind::Booleen) { return false; }
                if !bloc_compilable(alors, env, prog, attendu) { return false; }
                if let Some(s) = sinon { if !bloc_compilable(s, env, prog, attendu) { return false; } }
            }
            Instr::TantQue { test, corps, .. } => {
                if ty_statique(test, env) != Some(TyKind::Booleen) { return false; }
                if !bloc_compilable(corps, env, prog, attendu) { return false; }
            }
            Instr::Pour { nom, debut, fin, corps, .. } => {
                if ty_statique(debut, env) != Some(TyKind::Entier) { return false; }
                if ty_statique(fin, env) != Some(TyKind::Entier) { return false; }
                env.insert(nom.clone(), TyKind::Entier);
                if !bloc_compilable(corps, env, prog, attendu) { return false; }
            }
            Instr::Retour(e, _) => {
                if let Some(e) = e {
                    let t = match ty_statique(e, env) { Some(t) => t, None => return false };
                    // un entier issu de * ou ^ peut promouvoir en GMP ; la trame
                    // i64 du JIT ne sait pas le representer (imul wrapp en
                    // silence). Refuser la compilation -> repli interprete exact.
                    fn multiplicatif(e: &Expr) -> bool {
                        match e {
                            Expr::Binaire { op, a, b, .. } => {
                                matches!(op.as_str(), "*" | "^") || multiplicatif(a) || multiplicatif(b)
                            }
                            Expr::Unaire { a, .. } => multiplicatif(a),
                            _ => false,
                        }
                    }
                    if t == TyKind::Entier && multiplicatif(e) { return false; }
                    // le type doit correspondre au déclaré, SAUF entier vers
                    // flottant : l'émetteur insère fcvt_from_sint (miroir de la
                    // conversion d'arguments, « demi(7) »).
                    if !retour_coherent(t, attendu)
                        && !(attendu == Some(TyKind::Flottant) && t == TyKind::Entier)
                    { return false; }
                }
            }
            Instr::Bloc(b) => { if !bloc_compilable(b, env, prog, attendu) { return false; } }
            _ => return false,
        }
    }

    true
}

fn retour_coherent(t: TyKind, attendu: Option<TyKind>) -> bool {
    match attendu {
        None => true,
        Some(a) => match (t, a) {
            (TyKind::Booleen, TyKind::Booleen) => true,
            (TyKind::Entier, TyKind::Entier) => true,
            (TyKind::Flottant, TyKind::Flottant) => true,
            (TyKind::Entier, TyKind::Booleen) | (TyKind::Booleen, TyKind::Entier) => false,
            _ => false,
        },
    }
}

// ── Émission ──────────────────────────────────────────────────────────────

/// Variables déjà déclarées qui sont réaffectées dans le bloc : elles doivent
/// traverser la boucle « pour » en paramètres (nœuds phi) sinon le SSA builder
/// résoudrait leur use_var sur la valeur d'entrée du bloc (effet : accumulateur
/// figé à sa valeur initiale).

struct Emetteur<'a> {
    module: &'a mut JITModule,
    signatures: &'a HashMap<String, Signature>,
    #[allow(dead_code)]
    foncs: &'a HashMap<String, FuncId>,
    decls: &'a HashMap<String, FuncId>,
    runtime: &'a HashMap<String, FuncId>,
    builder: FunctionBuilder<'a>,
    returned: bool,
    prochain_slot: u32,
    refs: HashMap<String, FuncRef>,
    tags: HashMap<String, Variable>,
    tag_tmp: Variable,
}


fn make_signature(sig: &Signature) -> ClSignature {
    let mut s = ClSignature::new(CallConv::SystemV);
    for (_, t) in &sig.params {
        s.params.push(AbiParam::new(match t.n {
            TyKind::Flottant => types::F64,
            _ => types::I64,
        }));
    }
    match sig.retour.n {
        TyKind::Vide => {}
        TyKind::Flottant => s.returns.push(AbiParam::new(types::F64)),
        _ => s.returns.push(AbiParam::new(types::I64)),
    }
    s
}



pub struct Jit {
    module: JITModule,
    decls: HashMap<String, FuncId>,
    runtime_decls: HashMap<String, FuncId>,
    #[allow(dead_code)]
    foncs: HashMap<String, FuncId>,
    signatures: HashMap<String, Signature>,
}

impl Jit {
    pub fn nouveau(signatures: &HashMap<String, Signature>) -> Jit {
        let builder = JITBuilder::new(default_libcall_names()).unwrap();
        let module = JITModule::new(builder);
        Jit { module, decls: HashMap::new(), runtime_decls: HashMap::new(),
              foncs: HashMap::new(), signatures: signatures.clone() }
    }

    /// Compile toutes les fonctions éligibles ; les autres restent Interprète.
    /// Compile le corps du programme principal en code natif Cranelift
    /// (miroir du C++ qui émet $principal dans le module RIR et en prend
    /// get_entry("$principal")). Les globales deviennent des slots i64/f64
    /// de la trame, exactement comme les locales d'une fonction.
    pub fn compiler_principal(&mut self, prog: &crate::front::Programme)
        -> Option<Compile> {
        if std::env::var("ROSE_SANS_JIT").is_ok() { return None; }
        // les fonctions appelees doivent exister (declarees+definiens) AVANT
        // d'etre referencees depuis $principal :
        if self.foncs.is_empty() {
            self.compiler_tout();
        }
        let sig = Signature {
            nom: "$principal".into(), params: Vec::new(),
            retour: crate::front::Ty::vide(), a_retour: false,
            corps: Some(prog.instructions.clone()),
            dll: String::new(), dll_son: String::new(), symbole: String::new(),
            ligne: 0,
        };
        // mêmes portes de parité qu'une fonction ordinaire
        let mut env = HashMap::new();
        if !bloc_compilable(sig.corps.as_deref().unwrap_or(&[]), &mut env,
                            &self.signatures, Some(crate::front::TyKind::Vide)) {
            return None;
        }
        use cranelift::codegen::Context;
        let fid = self.module.declare_function("$principal", Linkage::Preemptible,
                                               &make_signature(&sig)).unwrap();
        let cible = self.module.target_config();
        let mut fcx = Context::new();
        fcx.func.signature = make_signature(&sig);
        let mut fctx = cranelift::frontend::FunctionBuilderContext::new();
        let mut b = FunctionBuilder::new(&mut fcx.func, &mut fctx);
        let tag_tmp_var = b.declare_var(types::I64);
        let mut e = Emetteur {
            module: &mut self.module, signatures: &self.signatures,
            foncs: &self.foncs, decls: &self.decls, runtime: &self.runtime_decls,
            builder: b, returned: false, prochain_slot: 100, refs: HashMap::new(), tags: HashMap::new(), tag_tmp: tag_tmp_var,
        };
        e.emettre_fonction(&sig);
        let mut b = e.builder;
        b.seal_all_blocks();
        b.finalize(cible);
        if let Err(e) = self.module.define_function(fid, &mut fcx) { return None; }
        if let Err(e) = self.module.finalize_definitions() { eprintln!("DEBUG finalize_definitions err: {}", e); return None; }
        let code = self.module.get_finalized_function(fid);
        let fptr: usize = code as usize;
        // appel direct du code natif (pas de trampoline d'arguments : zéro param)
        Some(Compile::Principal(Box::new(move || {
            unsafe { let f: extern "C" fn() = std::mem::transmute(fptr); f(); }
        })))
    }

    pub fn compiler_tout(&mut self) -> HashMap<String, Compile> {
        let mut out: HashMap<String, Compile> = HashMap::new();
        // échappatoire de debug : ROSE_SANS_JIT=1 force l'interpréteur pur
        if std::env::var("ROSE_SANS_JIT").is_ok() { return out; }
        // déclarations Import des helpers runtime (utilisées par l'émetteur)
        if self.runtime_decls.is_empty() {
            for (nom, params, ret) in [
                ("rose_affiche_i64", vec![types::I64], None),
                ("rose_affiche_bool", vec![types::I64], None),
                ("rose_affiche_f64", vec![types::F64], None),
                ("rose_powf", vec![types::F64, types::F64], Some(types::F64)),
                ("rose_add_ent", vec![types::I64, types::I64, types::I64, types::I64, types::I64, types::I64], None),
                ("rose_sub_ent", vec![types::I64, types::I64, types::I64, types::I64, types::I64, types::I64], None),
                ("rose_mul_ent", vec![types::I64, types::I64, types::I64, types::I64, types::I64, types::I64], None),
                ("rose_neg_ent", vec![types::I64, types::I64, types::I64, types::I64], None),
                ("rose_affiche_ent", vec![types::I64, types::I64], None),
                ("rose_libere", vec![types::I64], None),
                ("rose_set_ret_tag", vec![types::I64], None),
                ("rose_erreur_mod_zero", vec![], None),
                ("rose_erreur_div_zero", vec![], None),
            ] {
                let mut s = ClSignature::new(CallConv::SystemV);
                for p in params { s.params.push(AbiParam::new(p)); }
                if let Some(r) = ret { s.returns.push(AbiParam::new(r)); }
                // Import : la relocation User namespace 0 vers ce FuncId
                // retombe sur get_address → lookup_symbol → dlsym dans ce
                // binaire (le binaire s'exporte lui-meme via --export-dynamic
                // dans le lien, d'ou les #[unsafe(no_mangle)] des helpers plus bas).
                let fid = self.module.declare_function(nom, Linkage::Import, &s).unwrap();
                self.runtime_decls.insert(nom.to_string(), fid);
            }
        }
        // symboles runtime publiés dans le module JIT (résolus par nom, jamais appelés ici)
        #[allow(dead_code,unused_imports)]
        unsafe extern "C" {
            fn rose_affiche_i64(v: i64);
            fn rose_affiche_bool(v: i64);
            fn rose_affiche_f64(v: f64);
        }
        
        // passe 1 : déclaration de toutes les fonctions éligibles (récursion)
        let eligibles: Vec<String> = self.signatures.iter()
            .filter(|(_, s)| s.corps.is_some() && fonction_compilable(s, &self.signatures))
            .map(|(n, _)| n.clone()).collect();
        for nom in &eligibles {
            let sig = &self.signatures[nom];
            let fid = self.module.declare_function(nom, Linkage::Preemptible, &make_signature(sig)).unwrap();
            self.decls.insert(nom.clone(), fid);
        }
        // passe 2 : émission
        use cranelift::codegen::Context;
        let mut fcx = Context::new();
        let cible = self.module.target_config();
        for nom in &eligibles {
            let sig = self.signatures[nom].clone();
            let fid = self.decls[nom];
            fcx.clear();
            fcx.func.signature = make_signature(&sig);
            let mut fctx = cranelift::frontend::FunctionBuilderContext::new();
            let mut b = FunctionBuilder::new(&mut fcx.func, &mut fctx);
            let tag_tmp_var = b.declare_var(types::I64);
            let mut e = Emetteur {
                module: &mut self.module, signatures: &self.signatures,
                foncs: &self.foncs, decls: &self.decls, runtime: &self.runtime_decls,
                builder: b, returned: false, prochain_slot: 100, refs: HashMap::new(), tags: HashMap::new(), tag_tmp: tag_tmp_var,
            };
            e.emettre_fonction(&sig);
            let mut b = e.builder;
            b.seal_all_blocks();
            b.finalize(cible);
            if std::env::var("ROSE_DUMP_IR").is_ok() { eprintln!("{}", fcx.func); }
            self.module.define_function(fid, &mut fcx).unwrap();
            self.foncs.insert(nom.clone(), fid);
        }
        self.module.finalize_definitions().unwrap();
        for nom in &eligibles {
            let fid = self.foncs[nom];
            let code = self.module.get_finalized_function(fid);
            let sig = self.signatures[nom].clone();
            let fptr: usize = code as usize;
            out.insert(nom.clone(), Compile::Natif(Box::new(move |args: Vec<Valeur>| {
                unsafe { appeler_trampoline(fptr, &sig, args) }
            })));
        }
        out
    }
}

#[allow(dead_code)]
#[unsafe(no_mangle)]
pub extern "C" fn rose_powf(a: f64, b: f64) -> f64 { a.powf(b) }

/// Trampoline d'appel : la convention SystemV place les entiers dans x0..x7
/// et les flottants dans d0..d7 ; on transmute vers une signature typée par
/// combinaison (i64/f64). Au-delà de 4 paramètres, repli interprété.
unsafe fn appeler_trampoline(fptr: usize, sig: &Signature, args: Vec<Valeur>) -> Valeur {
    use TyKind::*;
    if args.iter().any(|a| matches!(a, Valeur::Entier(_, Some(_)))) { return Valeur::Vide; }
    if args.len() > 4 { return Valeur::Vide; }
    // le JIT n'appelle que des entiers, flottants et booleens (miroir de la
    // porte de l'interpreteur) ; les booleens passent en i64 0/1 quel que soit
    // le type déclaré.
    if args.iter().any(|a| !matches!(a,
        Valeur::Flottant(..) | Valeur::Entier(_, None) | Valeur::Booleen(..))) {
        return Valeur::Vide;
    }
    // le typage du registre suit la VALEUR RÉELLEMENT passée, pas le type
    // déclaré (le vérificateur autorise « demi(7) » : entier vers flottant).
    let types: Vec<TyKind> = args.iter().map(|a| match a {
        Valeur::Flottant(..) => Flottant,
        _ => Entier,
    }).collect();
    // prépare les emplacements
    let mut ints: Vec<u64> = Vec::new();
    let mut flts: Vec<f64> = Vec::new();
    for a in &args {
        match a {
            Valeur::Flottant(d) => flts.push(*d),
            Valeur::Booleen(b) => ints.push(if *b { 1 } else { 0 }),
            Valeur::Entier(i, None) => ints.push(*i as u64),
            _ => {}
        }
    }
    // helper : lit la k-ième valeur d'entier / flottant
    macro_rules! iv { ($k:expr) => { ints[$k] as i64 } }
    macro_rules! fv { ($k:expr) => { flts[$k] } }
    let cle: String = types.iter().map(|t| if *t == Flottant { 'f' } else { 'i' }).collect();
    let retour = sig.retour.n;
    let r: i64;
    let rf: f64;
    match (cle.as_str(), retour) {
        ("", Vide) => { let f: extern "C" fn() = std::mem::transmute(fptr); f(); r = 0; rf = 0.0; }
        ("", Flottant) => { let f: extern "C" fn() -> f64 = std::mem::transmute(fptr); rf = f(); r = 0; }
        ("", _) => { let f: extern "C" fn() -> i64 = std::mem::transmute(fptr); r = f(); rf = 0.0; }
        ("i", Vide) => { let f: extern "C" fn(i64) = std::mem::transmute(fptr); f(iv!(0)); r = 0; rf = 0.0; }
        ("i", Flottant) => { let f: extern "C" fn(i64) -> f64 = std::mem::transmute(fptr); rf = f(iv!(0)); r = 0; }
        ("i", _) => { let f: extern "C" fn(i64) -> i64 = std::mem::transmute(fptr); r = f(iv!(0)); rf = 0.0; }
        ("f", Vide) => { let f: extern "C" fn(f64) = std::mem::transmute(fptr); f(fv!(0)); r = 0; rf = 0.0; }
        ("f", Flottant) => { let f: extern "C" fn(f64) -> f64 = std::mem::transmute(fptr); rf = f(fv!(0)); r = 0; }
        ("f", _) => { let f: extern "C" fn(f64) -> i64 = std::mem::transmute(fptr); r = f(fv!(0)); rf = 0.0; }
        ("ii", Vide) => { let f: extern "C" fn(i64, i64) = std::mem::transmute(fptr); f(iv!(0), iv!(1)); r = 0; rf = 0.0; }
        ("ii", Flottant) => { let f: extern "C" fn(i64, i64) -> f64 = std::mem::transmute(fptr); rf = f(iv!(0), iv!(1)); r = 0; }
        ("ii", _) => { let f: extern "C" fn(i64, i64) -> i64 = std::mem::transmute(fptr); r = f(iv!(0), iv!(1)); rf = 0.0; }
        ("if", Vide) => { let f: extern "C" fn(i64, f64) = std::mem::transmute(fptr); f(iv!(0), fv!(0)); r = 0; rf = 0.0; }
        ("if", Flottant) => { let f: extern "C" fn(i64, f64) -> f64 = std::mem::transmute(fptr); rf = f(iv!(0), fv!(0)); r = 0; }
        ("if", _) => { let f: extern "C" fn(i64, f64) -> i64 = std::mem::transmute(fptr); r = f(iv!(0), fv!(0)); rf = 0.0; }
        ("fi", Vide) => { let f: extern "C" fn(f64, i64) = std::mem::transmute(fptr); f(fv!(0), iv!(0)); r = 0; rf = 0.0; }
        ("fi", Flottant) => { let f: extern "C" fn(f64, i64) -> f64 = std::mem::transmute(fptr); rf = f(fv!(0), iv!(0)); r = 0; }
        ("fi", _) => { let f: extern "C" fn(f64, i64) -> i64 = std::mem::transmute(fptr); r = f(fv!(0), iv!(0)); rf = 0.0; }
        ("ff", Vide) => { let f: extern "C" fn(f64, f64) = std::mem::transmute(fptr); f(fv!(0), fv!(1)); r = 0; rf = 0.0; }
        ("ff", Flottant) => { let f: extern "C" fn(f64, f64) -> f64 = std::mem::transmute(fptr); rf = f(fv!(0), fv!(1)); r = 0; }
        ("ff", _) => { let f: extern "C" fn(f64, f64) -> i64 = std::mem::transmute(fptr); r = f(fv!(0), fv!(1)); rf = 0.0; }
        ("iii", Vide) => { let f: extern "C" fn(i64, i64, i64) = std::mem::transmute(fptr); f(iv!(0), iv!(1), iv!(2)); r = 0; rf = 0.0; }
        ("iii", Flottant) => { let f: extern "C" fn(i64, i64, i64) -> f64 = std::mem::transmute(fptr); rf = f(iv!(0), iv!(1), iv!(2)); r = 0; }
        ("iii", _) => { let f: extern "C" fn(i64, i64, i64) -> i64 = std::mem::transmute(fptr); r = f(iv!(0), iv!(1), iv!(2)); rf = 0.0; }
        _ => {
            // 4 args et plus : reconstruction selon le gabarit exact
            return appeler_quatre(fptr, sig, &types, retour, &ints, &flts);
        }
    }
    let tag = unsafe { crate::run::rose_get_ret_tag() };
    match retour {
        Vide => Valeur::Vide,
        Flottant => Valeur::f(rf),
        Booleen => Valeur::b(r != 0),
        _ => if tag != 0 {
            let g = crate::run::Gz::neuf();
            unsafe { g.copier_de_ptr(r as *const crate::run::Mpz); }
            Valeur::Entier(0, Some(std::rc::Rc::new(g)))
        } else { Valeur::z(r as i64) },
    }
}

unsafe fn appeler_quatre(fptr: usize, _sig: &Signature, types: &[TyKind], retour: TyKind,
                         ints: &[u64], flts: &[f64]) -> Valeur {
    use TyKind::*;
    let n = types.len();
    if n != 4 { return Valeur::Vide; }
    macro_rules! x { ($k:expr) => { ints[$k] as i64 } }
    macro_rules! y { ($k:expr) => { flts[$k] } }
    let cle: String = types.iter().map(|t| if *t == Flottant { 'f' } else { 'i' }).collect();
    let r: i64; let rf: f64;
    match (cle.as_str(), retour) {
        ("iiii", Flottant) => { let f: extern "C" fn(i64,i64,i64,i64) -> f64 = std::mem::transmute(fptr); rf = f(x!(0),x!(1),x!(2),x!(3)); r = 0; }
        ("iiii", _) => { let f: extern "C" fn(i64,i64,i64,i64) -> i64 = std::mem::transmute(fptr); r = f(x!(0),x!(1),x!(2),x!(3)); rf = 0.0; }
        ("iiif", Flottant) => { let f: extern "C" fn(i64,i64,i64,f64) -> f64 = std::mem::transmute(fptr); rf = f(x!(0),x!(1),x!(2),y!(0)); r = 0; }
        ("iiif", _) => { let f: extern "C" fn(i64,i64,i64,f64) -> i64 = std::mem::transmute(fptr); r = f(x!(0),x!(1),x!(2),y!(0)); rf = 0.0; }
        ("iiff", Flottant) => { let f: extern "C" fn(i64,i64,f64,f64) -> f64 = std::mem::transmute(fptr); rf = f(x!(0),x!(1),y!(0),y!(1)); r = 0; }
        ("iiff", _) => { let f: extern "C" fn(i64,i64,f64,f64) -> i64 = std::mem::transmute(fptr); r = f(x!(0),x!(1),y!(0),y!(1)); rf = 0.0; }
        ("ifff", Flottant) => { let f: extern "C" fn(i64,f64,f64,f64) -> f64 = std::mem::transmute(fptr); rf = f(x!(0),y!(0),y!(1),y!(2)); r = 0; }
        ("ifff", _) => { let f: extern "C" fn(i64,f64,f64,f64) -> i64 = std::mem::transmute(fptr); r = f(x!(0),y!(0),y!(1),y!(2)); rf = 0.0; }
        ("ffff", Flottant) => { let f: extern "C" fn(f64,f64,f64,f64) -> f64 = std::mem::transmute(fptr); rf = f(y!(0),y!(1),y!(2),y!(3)); r = 0; }
        ("ffff", _) => { let f: extern "C" fn(f64,f64,f64,f64) -> i64 = std::mem::transmute(fptr); r = f(y!(0),y!(1),y!(2),y!(3)); rf = 0.0; }
        ("ffii", Flottant) => { let f: extern "C" fn(f64,f64,i64,i64) -> f64 = std::mem::transmute(fptr); rf = f(y!(0),y!(1),x!(0),x!(1)); r = 0; }
        ("ffii", _) => { let f: extern "C" fn(f64,f64,i64,i64) -> i64 = std::mem::transmute(fptr); r = f(y!(0),y!(1),x!(0),x!(1)); rf = 0.0; }
        ("ffif", Flottant) => { let f: extern "C" fn(f64,f64,i64,f64) -> f64 = std::mem::transmute(fptr); rf = f(y!(0),y!(1),x!(0),y!(2)); r = 0; }
        ("ffif", _) => { let f: extern "C" fn(f64,f64,i64,f64) -> i64 = std::mem::transmute(fptr); r = f(y!(0),y!(1),x!(0),y!(2)); rf = 0.0; }
        ("fiif", Flottant) => { let f: extern "C" fn(f64,i64,i64,f64) -> f64 = std::mem::transmute(fptr); rf = f(y!(0),x!(0),x!(1),y!(1)); r = 0; }
        ("fiif", _) => { let f: extern "C" fn(f64,i64,i64,f64) -> i64 = std::mem::transmute(fptr); r = f(y!(0),x!(0),x!(1),y!(1)); rf = 0.0; }
        ("fffi", Flottant) => { let f: extern "C" fn(f64,f64,f64,i64) -> f64 = std::mem::transmute(fptr); rf = f(y!(0),y!(1),y!(2),x!(0)); r = 0; }
        ("fffi", _) => { let f: extern "C" fn(f64,f64,f64,i64) -> i64 = std::mem::transmute(fptr); r = f(y!(0),y!(1),y!(2),x!(0)); rf = 0.0; }
        _ => { r = 0; rf = 0.0; }
    }
    let tag = unsafe { crate::run::rose_get_ret_tag() };
    match retour {
        Vide => Valeur::Vide,
        Flottant => Valeur::f(rf),
        Booleen => Valeur::b(r != 0),
        _ => if tag != 0 {
            let g = crate::run::Gz::neuf();
            unsafe { g.copier_de_ptr(r as *const crate::run::Mpz); }
            Valeur::Entier(0, Some(std::rc::Rc::new(g)))
        } else { Valeur::z(r as i64) },
    }
}

impl Emetteur<'_> {
    fn saut(&mut self, bloc: Block, args: &[Value]) -> Inst {
        let args: Vec<BlockArg> = args.iter().map(|v| (*v).into()).collect();
        self.builder.ins().jump(bloc, &args)
    }

    fn br_args(&mut self, c: Value, alors: Block, args_alors: &[Value], sinon: Block, args_sinon: &[Value]) {
        let a: Vec<BlockArg> = args_alors.iter().map(|v| (*v).into()).collect();
        let b: Vec<BlockArg> = args_sinon.iter().map(|v| (*v).into()).collect();
        self.builder.ins().brif(c, alors, &a[..], sinon, &b[..]);
    }

    fn emettre_fonction(&mut self, sig: &Signature) {
        // chaque variable (paramètre comme locale) a SON propre slot : deux
        // Variable Cranelift différents = deux emplacements distincts.
        let block = self.builder.create_block();
        self.builder.append_block_params_for_function_params(block);
        self.builder.switch_to_block(block);
        self.builder.seal_block(block);

        let mut env: HashMap<String, Variable> = HashMap::new();
        let mut types: HashMap<String, TyKind> = HashMap::new();
        let zt = self.builder.ins().iconst(types::I64, 0);
        self.builder.def_var(self.tag_tmp, zt);
        for (idx, (pn, pt)) in sig.params.iter().enumerate() {
            let ty = match pt.n { TyKind::Flottant => types::F64, _ => types::I64 };
            let slot = self.builder.declare_var(ty);
            let _ = idx;
            let param = self.builder.block_params(block)[idx];
            self.builder.def_var(slot, param);
            env.insert(pn.clone(), slot);
            types.insert(pn.clone(), pt.n);
        }
        // variables locales : slots à partir de 100
        self.prochain_slot = 100;
        let corps = sig.corps.clone().unwrap_or_default();
        self.emettre_bloc(&corps, &mut env, &mut types, sig);
        if !self.returned {
            match sig.retour.n {
                TyKind::Vide => { self.builder.ins().return_(&[]); }
                TyKind::Flottant => {
                    let c = self.builder.ins().iconst(types::I64, 0);
                    let z = self.builder.ins().bitcast(types::F64, MemFlagsData::new(), c);
                    let zt = self.builder.ins().iconst(types::I64, 0);
                    let f = self.import_runtime("rose_set_ret_tag");
                    let _ = self.builder.ins().call(f, &[zt]);
                    self.builder.ins().return_(&[z]);
                }
                _ => {
                    let zt = self.builder.ins().iconst(types::I64, 0);
                    let f = self.import_runtime("rose_set_ret_tag");
                    let _ = self.builder.ins().call(f, &[zt]);
                    let c = self.builder.ins().iconst(types::I64, 0);
                    self.builder.ins().return_(&[c]);
                }
            }
        }
    }

    fn var(&mut self, env: &mut HashMap<String, Variable>, types: &HashMap<String, TyKind>,
           nom: &str) -> Variable {
        if let Some(v) = env.get(nom) { return *v; }
        let ty = types.get(nom).copied().unwrap_or(TyKind::Entier);
        let cr = match ty { TyKind::Flottant => types::F64, _ => types::I64 };
        let slot = self.builder.declare_var(cr);
        self.prochain_slot += 1;
        if ty != TyKind::Flottant {
            let tslot = self.builder.declare_var(types::I64);
            self.prochain_slot += 1;
            let zt = self.builder.ins().iconst(types::I64, 0);
            self.builder.def_var(tslot, zt);
            self.tags.insert(nom.to_string(), tslot);
        }
        let zero = match ty {
            TyKind::Flottant => {
                let c = self.builder.ins().iconst(types::I64, 0);
                self.builder.ins().bitcast(types::F64, MemFlagsData::new(), c)
            }
            _ => self.builder.ins().iconst(types::I64, 0),
        };
        self.builder.def_var(slot, zero);
        env.insert(nom.to_string(), slot);
        slot
    }

    fn type_of(&self, e: &Expr, env: &HashMap<String, Variable>, types: &HashMap<String, TyKind>) -> TyKind {
        let mut envty: HashMap<String, TyKind> = HashMap::new();
        for k in env.keys() {
            if let Some(t) = types.get(k) { envty.insert(k.clone(), *t); }
        }
        ty_statique(e, &envty).unwrap_or(TyKind::Entier)
    }

    fn expr(&mut self, e: &Expr, env: &mut HashMap<String, Variable>,
            types: &HashMap<String, TyKind>) -> Value {
        match e {
            Expr::Litteral { texte, flott, .. } => {
                if *flott {
                    let d: f64 = texte.parse().unwrap_or(0.0);
                    let c = self.builder.ins().iconst(types::I64, d.to_bits() as i64);
                    self.builder.ins().bitcast(types::F64, MemFlagsData::new(), c)
                } else if texte.contains('/') {
                    let n = texte.find('/').unwrap();
                    let a: i64 = texte[..n].parse().unwrap_or(0);
                    let b: i64 = texte[n + 1..].parse().unwrap_or(1);
                    let d = a as f64 / b as f64;
                    let c = self.builder.ins().iconst(types::I64, d.to_bits() as i64);
                    self.builder.ins().bitcast(types::F64, MemFlagsData::new(), c)
                } else {
                    let v: i64 = texte.parse().unwrap_or(0);
                    let zt = self.builder.ins().iconst(types::I64, 0);
        self.builder.def_var(self.tag_tmp, zt);
                    self.builder.ins().iconst(types::I64, v)                }
            }
            Expr::BooleenLit(b, _) => { let zt = self.builder.ins().iconst(types::I64, 0); self.builder.def_var(self.tag_tmp, zt); self.builder.ins().iconst(types::I64, if *b { 1 } else { 0 }) }
            Expr::Unaire { op, a, .. } => {
                let v = self.expr(a, env, types);
                if op == "non" {
                    let un = self.builder.ins().iconst(types::I64, 1);
                    self.builder.ins().bxor(v, un)
                } else {
                    let tag_v = self.builder.use_var(self.tag_tmp);
                    let trame = self.trame_deux_mots();
                    let f = self.import_runtime("rose_neg_ent");
                    let trame8 = self.builder.ins().iadd_imm(trame, 8);
                    let _ = self.builder.ins().call(f, &[v, tag_v, trame, trame8]);
                    let val = self.builder.ins().load(types::I64, MemFlagsData::new(), trame, 0);
                    let tag = self.builder.ins().load(types::I64, MemFlagsData::new(), trame, 8);
                    self.builder.def_var(self.tag_tmp, tag);
                    val
                }
            }
            Expr::Var(n, _) => {
                let slot = self.var(env, types, n);
                if types.get(n).copied() != Some(TyKind::Flottant) {
                    if let Some(ts) = self.tags.get(n).copied() {
                        let t = self.builder.use_var(ts);
                        self.builder.def_var(self.tag_tmp, t);
                    } else {
                        let zt = self.builder.ins().iconst(types::I64, 0);
                        self.builder.def_var(self.tag_tmp, zt);
                    }
                } else {
                    let zt = self.builder.ins().iconst(types::I64, 0);
                    self.builder.def_var(self.tag_tmp, zt);
                }
                self.builder.use_var(slot)
            }
            Expr::Binaire { op, a, b, .. } => self.binaire(op, a, b, env, types),
            Expr::Appel { nom, args, .. } => self.appel(nom, args, env, types),
            _ => unreachable!("expression hors support JIT"),
        }
    }

    /// Appel direct d'une fonction déjà déclarée dans le module JIT (les
    /// récursives comprises : la déclaration précède l'émission). Les types
    /// non compilables sont filtrés par la porte bloc_compilable.
    fn appel(&mut self, nom: &str, args: &[Expr], env: &mut HashMap<String, Variable>,
             types: &HashMap<String, TyKind>) -> Value {
        let sig = self.signatures[nom].clone();
        let fref = self.fonction_ref(nom, &sig);
        let mut vals = Vec::with_capacity(args.len());
        for (i, a) in args.iter().enumerate() {
            let pt = sig.params[i].1.n;
            let v = self.expr(a, env, types);
            let attendu = if pt == TyKind::Flottant { types::F64 } else { types::I64 };
            let obtenu = self.builder.func.dfg.value_type(v);
            vals.push(if attendu == obtenu { v }
                      else if attendu == types::F64 { self.builder.ins().fcvt_from_sint(types::F64, v) }
                      else { self.builder.ins().fcvt_to_sint(types::I64, v) });
        }
        let inst = self.builder.ins().call(fref, &vals);
        match sig.retour.n {
            TyKind::Vide => self.builder.ins().iconst(types::I64, 0),
            TyKind::Entier => self.builder.inst_results(inst)[0],
            _ => self.builder.inst_results(inst)[0],
        }
    }

    fn binaire(&mut self, op: &str, a: &Expr, b: &Expr, env: &mut HashMap<String, Variable>,
               types: &HashMap<String, TyKind>) -> Value {
        let ta = self.type_of(a, env, types);
        let tb = self.type_of(b, env, types);
        let flot = ta == TyKind::Flottant || tb == TyKind::Flottant;
        match op {
            "et" => {
                // Le sous-ensemble JIT est sans effet de bord sur variables
                // compilables (pas d'appels) : évaluer les deux côtés puis
                // band/bor rend la même valeur que le court-circuit.
                let g = self.expr(a, env, types);
                let d = self.expr(b, env, types);
                self.builder.ins().band(g, d)
            }
            "ou" => {
                let g = self.expr(a, env, types);
                let d = self.expr(b, env, types);
                self.builder.ins().bor(g, d)
            }
            "==" | "!=" | "<" | "<=" | ">" | ">=" => {
                let g = self.expr(a, env, types);
                let tag_g = self.builder.use_var(self.tag_tmp);
                let d = self.expr(b, env, types);
                let tag_d = self.builder.use_var(self.tag_tmp);
                let (g, d) = si_flottant(&mut self.builder, flot, g, d, ta, tb);
                let v = if flot {
                    let cc = match op {
                        "==" => FloatCC::Equal,
                        "!=" => FloatCC::NotEqual,
                        "<" => FloatCC::LessThan,
                        "<=" => FloatCC::LessThanOrEqual,
                        ">" => FloatCC::GreaterThan,
                        _ => FloatCC::GreaterThanOrEqual,
                    };
                    self.builder.ins().fcmp(cc, g, d)
                } else {
                    let cc = match op {
                        "==" => IntCC::Equal,
                        "!=" => IntCC::NotEqual,
                        "<" => IntCC::SignedLessThan,
                        "<=" => IntCC::SignedLessThanOrEqual,
                        ">" => IntCC::SignedGreaterThan,
                        _ => IntCC::SignedGreaterThanOrEqual,
                    };
                    self.builder.ins().icmp(cc, g, d)
                };
                // b1 → i128
                self.builder.ins().uextend(types::I64, v)
            }
            "+" | "-" | "*" | "/" | "%" | "^" | "div" => {
                // ('*'/'^'/'-' entiers : hors sous-ensemble, sur_i64 a refuse la
                // compilation en amont — le chemin imul/i64_pow n'est jamais emis
                // pour un operande entier.)
                let g = self.expr(a, env, types);
                let tag_g = self.builder.use_var(self.tag_tmp);
                let d = self.expr(b, env, types);
                let tag_d = self.builder.use_var(self.tag_tmp);
                let (g, d) = si_flottant(&mut self.builder, flot, g, d, ta, tb);
                match op {
                    "+" => if flot { self.builder.ins().fadd(g, d) } else {
                        let zero_tag = self.builder.ins().iconst(types::I64, 0);
                        let tag_g_zero = self.builder.ins().icmp(IntCC::Equal, tag_g, zero_tag);
                        let tag_d_zero = self.builder.ins().icmp(IntCC::Equal, tag_d, zero_tag);
                        let tag_g_zero64 = self.builder.ins().uextend(types::I64, tag_g_zero);
                        let tag_d_zero64 = self.builder.ins().uextend(types::I64, tag_d_zero);
                        let both_i64 = self.builder.ins().band(tag_g_zero64, tag_d_zero64);
                        let fast_sum = self.builder.ins().iadd(g, d);
                        let g_xor_d = self.builder.ins().bxor(g, d);
                        let g_xor_sum = self.builder.ins().bxor(g, fast_sum);
                        let ov = self.builder.ins().band(g_xor_d, g_xor_sum);
                        let zero = self.builder.ins().iconst(types::I64, 0);
                        let ov_flag = self.builder.ins().icmp(IntCC::SignedLessThan, ov, zero);
                        let zero8 = self.builder.ins().iconst(types::I8, 0);
                        let ov_zero8 = self.builder.ins().icmp(IntCC::Equal, ov_flag, zero8);
                        let ov_zero = self.builder.ins().uextend(types::I64, ov_zero8);
                        let fast_ok = self.builder.ins().band(both_i64, ov_zero);
                        let fast_b = self.builder.create_block();
                        let slow_b = self.builder.create_block();
                        let merge_b = self.builder.create_block();
                        self.builder.append_block_param(merge_b, types::I64);
                        self.builder.append_block_param(merge_b, types::I64);
                        let zero64 = self.builder.ins().iconst(types::I64, 0);
                        let fast_ok_i8 = self.builder.ins().icmp(IntCC::NotEqual, fast_ok, zero64);
                        self.builder.ins().brif(fast_ok_i8, fast_b, &[], slow_b, &[]);
                        self.builder.switch_to_block(fast_b);
                        self.builder.seal_block(fast_b);
                        let zt = self.builder.ins().iconst(types::I64, 0);
                        self.builder.def_var(self.tag_tmp, zt);
                        self.saut(merge_b, &[fast_sum, zt]);
                        self.builder.switch_to_block(slow_b);
                        self.builder.seal_block(slow_b);
                        let trame = self.trame_deux_mots();
                        let trame8 = self.builder.ins().iadd_imm(trame, 8);
                        let f = self.import_runtime("rose_add_ent");
                        let _ = self.builder.ins().call(f, &[g, tag_g, d, tag_d, trame, trame8]);
                        let hval = self.builder.ins().load(types::I64, MemFlagsData::new(), trame, 0);
                        let htag = self.builder.ins().load(types::I64, MemFlagsData::new(), trame, 8);
                        self.builder.def_var(self.tag_tmp, htag);
                        self.saut(merge_b, &[hval, htag]);
                        self.builder.switch_to_block(merge_b);
                        self.builder.seal_block(merge_b);
                        let mv = self.builder.block_params(merge_b)[0];
                        let mt = self.builder.block_params(merge_b)[1];
                        self.builder.def_var(self.tag_tmp, mt);
                        mv
                    },
                    "-" => if flot { self.builder.ins().fsub(g, d) } else {
                        let trame = self.trame_deux_mots();
                        let f = self.import_runtime("rose_sub_ent");
                        let trame8 = self.builder.ins().iadd_imm(trame, 8);
                        let _ = self.builder.ins().call(f, &[g, tag_g, d, tag_d, trame, trame8]);
                        let val = self.builder.ins().load(types::I64, MemFlagsData::new(), trame, 0);
                        let tag = self.builder.ins().load(types::I64, MemFlagsData::new(), trame, 8);
                        self.builder.def_var(self.tag_tmp, tag);
                        val
                    },
                    "*" => if flot { self.builder.ins().fmul(g, d) } else {
                        let zero_tag = self.builder.ins().iconst(types::I64, 0);
                        let tag_g_zero = self.builder.ins().icmp(IntCC::Equal, tag_g, zero_tag);
                        let tag_d_zero = self.builder.ins().icmp(IntCC::Equal, tag_d, zero_tag);
                        let tag_g_zero64 = self.builder.ins().uextend(types::I64, tag_g_zero);
                        let tag_d_zero64 = self.builder.ins().uextend(types::I64, tag_d_zero);
                        let both_i64 = self.builder.ins().band(tag_g_zero64, tag_d_zero64);
                        let prod = self.builder.ins().imul(g, d);
                        // overflow check for signed multiplication
                        let zero = self.builder.ins().iconst(types::I64, 0);
                        let g_zero = self.builder.ins().icmp(IntCC::Equal, g, zero);
                        let d_zero = self.builder.ins().icmp(IntCC::Equal, d, zero);
                        let either_zero = self.builder.ins().bor(g_zero, d_zero);
                        // if either is zero, no overflow
                        let prod_div_d = self.builder.ins().sdiv(prod, d);  // only valid if d != 0
                        let div_ok = self.builder.ins().icmp(IntCC::Equal, prod_div_d, g);
                        let div_ok_i64 = self.builder.ins().uextend(types::I64, div_ok);
                        let one_val = self.builder.ins().iconst(types::I64, 1);
                        let no_ov = self.builder.ins().select(either_zero, one_val, div_ok_i64);
                        let zero64 = self.builder.ins().iconst(types::I64, 0);
                        let no_ov_i64 = no_ov;  // already 0 or 1 (I64)
                        let fast_ok = self.builder.ins().band(both_i64, no_ov_i64);
                        let fast_b = self.builder.create_block();
                        let slow_b = self.builder.create_block();
                        let merge_b = self.builder.create_block();
                        self.builder.append_block_param(merge_b, types::I64);
                        self.builder.append_block_param(merge_b, types::I64);
                        let zero64 = self.builder.ins().iconst(types::I64, 0);
                        let fast_ok_i8 = self.builder.ins().icmp(IntCC::NotEqual, fast_ok, zero64);
                        self.builder.ins().brif(fast_ok_i8, fast_b, &[], slow_b, &[]);
                        self.builder.switch_to_block(fast_b);
                        self.builder.seal_block(fast_b);
                        let zero_tag2 = self.builder.ins().iconst(types::I64, 0);
                        self.builder.def_var(self.tag_tmp, zero_tag2);
                        self.saut(merge_b, &[prod, zero_tag2]);
                        self.builder.switch_to_block(slow_b);
                        self.builder.seal_block(slow_b);
                        let trame = self.trame_deux_mots();
                        let trame8 = self.builder.ins().iadd_imm(trame, 8);
                        let f = self.import_runtime("rose_mul_ent");
                        let _ = self.builder.ins().call(f, &[g, tag_g, d, tag_d, trame, trame8]);
                        let hval = self.builder.ins().load(types::I64, MemFlagsData::new(), trame, 0);
                        let htag = self.builder.ins().load(types::I64, MemFlagsData::new(), trame, 8);
                        self.builder.def_var(self.tag_tmp, htag);
                        self.saut(merge_b, &[hval, htag]);
                        self.builder.switch_to_block(merge_b);
                        self.builder.seal_block(merge_b);
                        let mv = self.builder.block_params(merge_b)[0];
                        let mt = self.builder.block_params(merge_b)[1];
                        self.builder.def_var(self.tag_tmp, mt);
                        mv
                    },
                    "/" => {
                        let bloc = self.builder.current_block();
                        let _ = bloc;
                        self.builder.ins().fdiv(g, d)
                    }
                    "%" => {
                        // garde de justesse : un diviseur nul ne doit jamais
                        // atteindre srem (qui trapperait). On delegue a
                        // l'interpreteur via le helper runtime qui sort proprement.
                        let zero = self.builder.ins().iconst(types::I64, 0);
                        let is_zero = self.builder.ins().icmp(cranelift::codegen::ir::condcodes::IntCC::Equal, d, zero);
                        let trap_b = self.builder.create_block();
                        let cont_b = self.builder.create_block();
                        self.builder.ins().brif(is_zero, trap_b, &[], cont_b, &[]);
                        self.builder.switch_to_block(trap_b);
                        self.builder.seal_block(trap_b);
                        {
                            let f = self.runtime("rose_erreur_mod_zero");
                            let _ = self.builder.ins().call(f, &[]);
                        }
                        self.builder.ins().trap(cranelift::codegen::ir::TrapCode::INTEGER_DIVISION_BY_ZERO);
                        self.builder.switch_to_block(cont_b);
                        self.builder.seal_block(cont_b);
                        let r = self.builder.ins().srem(g, d);
                        // fdiv_r : le signe suit le diviseur. Correction quand
                        // signe(r) ≠ signe(d) et r ≠ 0.
                        self.correction_rem(r, d)
                    }
                    "div" => {
                        let q = self.builder.ins().sdiv(g, d);
                        // division euclidienne = floor
                        self.floor_div(q, g, d)
                    }
                    "^" => {
                        if flot {
                            // Puissance flottante par powf (miroir de l'appel
                            // std::pow du moteur C++, même arrondi libm).
                            let f = self.powf();
                            let inst = self.builder.ins().call(f, &[g, d]);
                            self.builder.inst_results(inst)[0]
                        } else {
                            self.i64_pow(g, d)
                        }
                    }
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        }
    }

    fn powf(&mut self) -> FuncRef {
        self.import_runtime("rose_powf")
    }

    /// Émet une expression ENTÈRE par le chemin exact : si elle contient une
    /// multiplication ou une puissance entière, passe par rose_mul_ent /
    /// rose_pow_ent (promotion GMP au dépassement, tag en deuxième mot) ;
    /// sinon calcul i64 ordinaire, tag zéro. Rend (valeur, tag) : tag ≠ 0 =>
    /// pointeur vers une Box<Valeur::Entier(0, Some(Gz))>.
    fn emettre_ent(&mut self, e: &Expr, env: &mut HashMap<String, Variable>,
                   types: &HashMap<String, TyKind>) -> (Value, Value) {
        fn multiplicatif(e: &Expr) -> bool {
            match e {
                Expr::Binaire { op, a, b, .. } => {
                    matches!(op.as_str(), "*" | "^") || multiplicatif(a) || multiplicatif(b)
                }
                Expr::Unaire { a, .. } => multiplicatif(a),
                _ => false,
            }
        }
        if matches!(e, Expr::Unaire { op, .. } if op == "-") {
            // negation entiere : voie exacte (rose_neg_ent) — -i64::MIN = 2^63
            // ne tient pas en i64, le C++ promote en GMP (jit_neg_z).
            if let Expr::Unaire { a, .. } = e {
                let v = self.expr(a, env, types);
                return self.neg_exact(v);
            }
        }
        if !multiplicatif(e) {
            let v = self.expr(e, env, types);
            let zero = self.builder.ins().iconst(types::I64, 0);
            return (v, zero);
        }
        match e {
            Expr::Binaire { op, a, b, .. } if op == "^" => {
                let (g, gt) = self.emettre_ent(a, env, types);
                let (d, dt) = self.emettre_ent(b, env, types);
                self.pow_exact(g, gt, d, dt)
            }
            Expr::Unaire { op, a, .. } if op == "-" => {
                // negation exacte miroir interp : -(i64::MIN) = 2^63 ne tient pas
                // en i64 -> Gz boxé (tag), rendu par rose_affiche_ent.
                let v = self.expr(a, env, types);
                self.neg_exact(v)
            }
            Expr::Unaire { a, .. } => self.emettre_ent(a, env, types),
            _ => {
                let v = self.expr(e, env, types);
                let zero = self.builder.ins().iconst(types::I64, 0);
                (v, zero)
            }
        }
    }

    /// a*b exact : appel rose_mul_ent (checked_mul i64 ; promotion GMP si le
    /// produit déborde, rendue dans une Box<Valeur> taggée).
    /// Trame locale de 2×i64 (valeur, tag) adressée au helper : un StackSlot
    /// explicite de 16 octets (les Variables Cranelift ne sont pas adressables).
    fn trame_deux_mots(&mut self) -> Value {
        let slot = self.builder.func.create_sized_stack_slot(
            cranelift::codegen::ir::StackSlotData::new(
                cranelift::codegen::ir::StackSlotKind::ExplicitSlot, 16, 0));
        self.builder.ins().stack_addr(types::I64, slot, 0)
    }

    /// Negation exacte (miroir jit_neg_z C++) : le tag signale la Box Gz.
    fn neg_exact_tag(&mut self, g: Value, _gt: Value) -> (Value, Value) {
        let (s, t) = self.neg_exact(g);
        (s, t)
    }

    fn neg_exact(&mut self, g: Value) -> (Value, Value) {
        let trame = self.trame_deux_mots();
        let f = self.import_runtime("rose_neg_ent");
        let _ = self.builder.ins().call(f, &[g, trame, trame]);
        let sortie = self.builder.ins().load(types::I64, MemFlagsData::new(), trame, 0);
        let tag = self.builder.ins().load(types::I64, MemFlagsData::new(), trame, 8);
        (sortie, tag)
    }

    /// Addition/soustraction exactes entre (valeur,tag) : les deux tags doivent
    /// etre nuls pour rester en i64 ; sinon la fonction n'est pas compilable
    /// (garde en amont), et l'addition i64 simple serait FAUSSE.
    fn add_exact(&mut self, g: Value, gt: Value, d: Value, dt: Value, soustraction: bool) -> Value {
        let zero = self.builder.ins().iconst(types::I64, 0);
        let ge = self.builder.ins().icmp(IntCC::Equal, gt, zero);
        let de = self.builder.ins().icmp(IntCC::Equal, dt, zero);
        let les_deux = self.builder.ins().band(ge, de);
        // tag ≠ 0 => operande Gz boxé : hors trame i64. On ne peut PAS combiner :
        // le compilable-check doit refuser (voir plus bas le garde 'tag dans le
        // flux'). Pour l'instant, si un tag apparait, la valeur rendue sera
        // incorrecte — le garde 'tags_interdits' en emission le detecte.
        let _ = les_deux;
        if soustraction { self.builder.ins().isub(g, d) } else { self.builder.ins().iadd(g, d) }
    }

    fn mul_exact(&mut self, g: Value, _gt: Value, d: Value, _dt: Value) -> (Value, Value) {
        let trame = self.trame_deux_mots();
        let f = self.import_runtime("rose_mul_ent");
        let _ = self.builder.ins().call(f, &[g, d, trame, trame]);
        let sortie = self.builder.ins().load(types::I64, MemFlagsData::new(), trame, 0);
        let tag = self.builder.ins().load(types::I64, MemFlagsData::new(), trame, 8);
        (sortie, tag)
    }

    fn pow_exact(&mut self, g: Value, _gt: Value, d: Value, _dt: Value) -> (Value, Value) {
        let trame = self.trame_deux_mots();
        let f = self.import_runtime("rose_pow_ent");
        let _ = self.builder.ins().call(f, &[g, d, trame, trame]);
        let sortie = self.builder.ins().load(types::I64, MemFlagsData::new(), trame, 0);
        let tag = self.builder.ins().load(types::I64, MemFlagsData::new(), trame, 8);
        (sortie, tag)
    }

    /// Référence appelée pour une fonction du programme : le FuncId déclaré en
    /// passe 1 devient un FuncRef importé (colocalisé, résolu à la finalisation).
    fn fonction_ref(&mut self, nom: &str, sig: &Signature) -> FuncRef {
        if let Some(&r) = self.refs.get(nom) { return r; }
        let fid = self.decls[nom];
        let s = self.module.declarations().get_function_decl(fid).signature.clone();
        let sigref = self.builder.import_signature(s);
        // déclaration d'un nom utilisateur (namespace 0 = fonction, index =
        // FuncId du module) que le module résoudra sur le symbole défini.
        let fid = self.decls[nom];
        let uref = self.builder.func.declare_imported_user_function(
            cranelift::codegen::ir::UserExternalName { namespace: 0, index: fid.as_u32() });
        let name = cranelift::codegen::ir::ExternalName::User(uref);
        let r = self.builder.import_function(cranelift::codegen::ir::ExtFuncData {
            name,
            signature: sigref,
            colocated: true,
            patchable: false,
        });
        self.refs.insert(nom.to_string(), r);
        let _ = sig;
        r
    }

    fn runtime(&mut self, nom: &str) -> FuncRef {
        self.import_runtime(nom)
    }

    fn import_runtime(&mut self, nom: &str) -> FuncRef {
        let fid = self.runtime[nom];
        let sig = self.module.declarations().get_function_decl(fid).signature.clone();
        let sigref = self.builder.import_signature(sig);
        let uref = self.builder.func.declare_imported_user_function(
            cranelift::codegen::ir::UserExternalName { namespace: 0, index: fid.as_u32() });
        self.builder.import_function(cranelift::codegen::ir::ExtFuncData {
            name: cranelift::codegen::ir::ExternalName::User(uref),
            signature: sigref,
            colocated: false,
            patchable: false,
        })
    }

    fn correction_rem(&mut self, r: Value, d: Value) -> Value {
        let zero = self.builder.ins().iconst(types::I64, 0);
        let rneg = self.builder.ins().icmp(IntCC::SignedLessThan, r, zero);
        let dpos = self.builder.ins().icmp(IntCC::SignedGreaterThan, d, zero);
        let rpos = self.builder.ins().icmp(IntCC::SignedGreaterThan, r, zero);
        let dneg = self.builder.ins().icmp(IntCC::SignedLessThan, d, zero);
        let a = self.builder.ins().band(rneg, dpos);
        let b = self.builder.ins().band(rpos, dneg);
        let cond = self.builder.ins().bor(a, b);
        let corr = self.builder.ins().iadd(r, d);
        self.builder.ins().select(cond, corr, r)
    }

    fn floor_div(&mut self, q: Value, g: Value, d: Value) -> Value {
        let r = self.builder.ins().srem(g, d);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let rnull = self.builder.ins().icmp(IntCC::Equal, r, zero);
        let rneg = self.builder.ins().icmp(IntCC::SignedLessThan, r, zero);
        let dpos = self.builder.ins().icmp(IntCC::SignedGreaterThan, d, zero);
        let rpos = self.builder.ins().icmp(IntCC::SignedGreaterThan, r, zero);
        let dneg = self.builder.ins().icmp(IntCC::SignedLessThan, d, zero);
        let a = self.builder.ins().band(rneg, dpos);
        let b = self.builder.ins().band(rpos, dneg);
        let signes = self.builder.ins().bor(a, b);
        let doit = self.builder.ins().band_not(signes, rnull);
        let un = self.builder.ins().iconst(types::I64, 1);
        let q2 = self.builder.ins().isub(q, un);
        self.builder.ins().select(doit, q2, q)
    }

    fn i64_pow(&mut self, g: Value, d: Value) -> Value {
        // ^ entier : refuse en amont (sur_i64 retourne false), ce code n'est
        // jamais atteint. Stub pour satisfaire le compilateur.
        let _ = (g, d);
        self.builder.ins().iconst(types::I64, 0)
    }

    fn emettre_bloc(&mut self, instrs: &[Instr], env: &mut HashMap<String, Variable>,
                    types: &mut HashMap<String, TyKind>, sig: &Signature) {
        for i in instrs {
            match i {
                Instr::Affect { nom, e, .. } => {
                    let t = self.type_of(e, env, types);
                    let v = self.expr(e, env, types);
                    let slot = env.get(nom).copied();
                    let slot = match slot {
                        Some(s) => s,
                        None => {
                            let cr = match t { TyKind::Flottant => types::F64, _ => types::I64 };
                            let s = self.builder.declare_var(cr);
                            self.prochain_slot += 1;
                            env.insert(nom.clone(), s);
                            types.insert(nom.clone(), t);
                            if t != TyKind::Flottant {
                                let ts = self.builder.declare_var(types::I64);
                                self.prochain_slot += 1;
                                let zt = self.builder.ins().iconst(types::I64, 0); self.builder.def_var(ts, zt);
                                self.tags.insert(nom.clone(), ts);
                            }
                            s
                        }
                    };
                    if t != TyKind::Flottant {
                        if let Some(ts) = self.tags.get(nom).copied() {
                            let tag = self.builder.use_var(self.tag_tmp);
                            self.builder.def_var(ts, tag);
                        }
                    }
                    self.builder.def_var(slot, v);
                }
                Instr::Afficher(e, _) => {
                    let t = self.type_of(e, env, types);
                    if t == TyKind::Entier {
                        let v = self.expr(e, env, types);
                        let tag = self.builder.use_var(self.tag_tmp);
                        let f = self.runtime("rose_affiche_ent");
                        let _ = self.builder.ins().call(f, &[v, tag]);
                        continue;
                    }
                    let v = self.expr(e, env, types);
                    let nom_key = match t {
                        TyKind::Flottant => "rose_affiche_f64",
                        TyKind::Booleen => "rose_affiche_bool",
                        _ => "rose_affiche_i64",
                    };
                    {
                        // pour un booleen (i128 0/1), extraire en i64
                        let vv = if t == TyKind::Booleen || t == TyKind::Entier {
                            self.builder.ins().ireduce(types::I64, v)
                        } else { v };
                        let f = self.runtime(nom_key);
                        let _ = self.builder.ins().call(f, &[vv]);
                    }
                }
                Instr::ExprStmt(e) => { self.expr(e, env, types); }
                Instr::Si { test, alors, sinon, .. } => {
                    let avant_retour = self.returned;
                    let corps = self.builder.create_block();
                    let apres_b = self.builder.create_block();
                    let cond = self.expr(test, env, types);
                    let zero = self.builder.ins().iconst(types::I64, 0);
                    let est_vrai = self.builder.ins().icmp(IntCC::NotEqual, cond, zero);
                    match sinon {
                        None => {
                            self.br_args(est_vrai, corps, &[], apres_b, &[]);
                            self.builder.switch_to_block(corps);
                            self.builder.seal_block(corps);
                            self.returned = avant_retour;
                            self.emettre_bloc(alors, env, types, sig);
                            if !self.returned { self.saut(apres_b, &[]); }
                            self.builder.switch_to_block(apres_b);
                            self.builder.seal_block(apres_b);
                            self.returned = avant_retour;
                        }
                        Some(else_) => {
                            let sinon_b = self.builder.create_block();
                            self.br_args(est_vrai, corps, &[], sinon_b, &[]);
                            self.builder.switch_to_block(corps);
                            self.builder.seal_block(corps);
                            self.returned = avant_retour;
                            self.emettre_bloc(alors, env, types, sig);
                            if !self.returned { self.saut(apres_b, &[]); }
                            self.builder.switch_to_block(sinon_b);
                            self.builder.seal_block(sinon_b);
                            self.returned = avant_retour;
                            self.emettre_bloc(else_, env, types, sig);
                            if !self.returned { self.saut(apres_b, &[]); }
                            self.builder.switch_to_block(apres_b);
                            self.builder.seal_block(apres_b);
                            self.returned = avant_retour;
                        }
                    }
                }
                Instr::TantQue { test, corps, .. } => {
                    let entree = self.builder.create_block();
                    let tour = self.builder.create_block();
                    let sortie = self.builder.create_block();
                    self.saut(entree, &[]);
                    self.builder.switch_to_block(entree);
                    let cond = self.expr(test, env, types);
                    let zero = self.builder.ins().iconst(types::I64, 0);
                    let ok = self.builder.ins().icmp(IntCC::NotEqual, cond, zero);
                    self.br_args(ok, tour, &[], sortie, &[]);
                    self.builder.switch_to_block(tour);
                    self.emettre_bloc(corps, env, types, sig);
                    if !self.returned { self.saut(entree, &[]); }
                    self.builder.switch_to_block(sortie);
                    if self.returned {
                        // bloc mort : fermé par un retour de secours (seal_all se
                        // charge du scellement).
                        match sig.retour.n {
                            TyKind::Vide => { self.builder.ins().return_(&[]); }
                            _ => { let c = self.builder.ins().iconst(types::I64, 0); self.builder.ins().return_(&[c]); }
                        }
                    }
                }
                Instr::Pour { nom, debut, fin, corps, .. } => {
                    let d = self.expr(debut, env, types);
                    let f = self.expr(fin, env, types);
                    let slot = {
                        match env.get(nom).copied() {
                            Some(s) => s,
                            None => {
                                let s = self.builder.declare_var(types::I64);
                                self.prochain_slot += 1;
                                let ts = self.builder.declare_var(types::I64);
                                self.prochain_slot += 1;
                                let zt = self.builder.ins().iconst(types::I64, 0);
                                self.builder.def_var(ts, zt);
                                self.tags.insert(nom.clone(), ts);
                                env.insert(nom.clone(), s);
                                types.insert(nom.clone(), TyKind::Entier);
                                s
                            }
                        }
                    };
                    self.builder.def_var(slot, d);
                    // Boucle en un seul bloc auto-suffisant : le test est émis en
                    // queue, donc le bloc corps n'a qu'un seul prédécesseur
                    // (l'entrée initiale ou lui-même) et le SSA builder n'a aucun
                    // phi à relier : le compteur est passé en paramètre explicite,
                    // et les variables du corps sont publiées par def_var en tête.
                    let corps_b = self.builder.create_block();
                    let sortie = self.builder.create_block();
                    self.builder.append_block_param(corps_b, types::I64);
                    self.saut(corps_b, &[d]);
                    self.builder.switch_to_block(corps_b);
                    let ps: Vec<Value> = self.builder.block_params(corps_b).to_vec();
                    let cur = ps[0];
                    self.builder.def_var(slot, cur);
                    self.emettre_bloc(corps, env, types, sig);
                    if !self.returned {
                        let cur2 = self.builder.use_var(slot);
                        let un = self.builder.ins().iconst(types::I64, 1);
                        let next = self.builder.ins().iadd(cur2, un);
                        let ok2 = self.builder.ins().icmp(IntCC::SignedLessThan, next, f);
                        self.br_args(ok2, corps_b, &[next], sortie, &[]);
                    }
                    self.builder.switch_to_block(sortie);
                    if self.returned {
                        // « sortie » n'est atteinte que par la branche fausse de la
                        // queue ; si le corps retourne toujours, il reste vide :
                        // retour de secours (code mort, jamais exécuté).
                        match sig.retour.n {
                            TyKind::Vide => { self.builder.ins().return_(&[]); }
                            _ => { let c = self.builder.ins().iconst(types::I64, 0); self.builder.ins().return_(&[c]); }
                        }
                    }
                }
                Instr::Retour(e, _) => {
                    match e {
                        None => { self.builder.ins().return_(&[]); }
                        Some(e) => {
                            let v = self.expr(e, env, types);
                            let tag = self.builder.use_var(self.tag_tmp);
                            let f = self.import_runtime("rose_set_ret_tag");
                            let _ = self.builder.ins().call(f, &[tag]);
                            self.builder.ins().return_(&[v]);
                        }
                    }
                    self.returned = true;
                }
                Instr::Bloc(b) => self.emettre_bloc(b, env, types, sig),
                _ => {}
            }
        }
    }
}

fn si_flottant(b: &mut FunctionBuilder, flot: bool, g: Value, d: Value,
               ta: TyKind, tb: TyKind) -> (Value, Value) {
    if !flot { return (g, d); }
    let g = if ta == TyKind::Flottant { g } else { b.ins().fcvt_from_sint(types::F64, g) };
    let d = if tb == TyKind::Flottant { d } else { b.ins().fcvt_from_sint(types::F64, d) };
    (g, d)
}


// les helpers d'affichage appelés par le code JIT (cranelift import) sont
// définis ici avec la convention C : référencés par nom via ExternalName, donc
// jamais appelés depuis Rust directement.
#[allow(dead_code)]
#[unsafe(no_mangle)]
pub extern "C" fn rose_affiche_i64(v: i64) { crate::run::ecrire_out(&format!("{}\n", v)); }

/// Helper JIT : affichage d'un i64 taggé (valeur, tag) rendu par rose_mul_ent /
/// rose_pow_ent / rose_neg_ent. tag ≠ 0 recycle la Box<Valeur>.
#[unsafe(no_mangle)]
pub extern "C" fn rose_affiche_ent(valeur: i64, tag: i64) {
    if tag == 0 {
        crate::run::ecrire_out(&format!("{}\n", valeur));
    } else {
        let g = crate::run::Gz::neuf();
        unsafe { g.copier_de_ptr(valeur as *const crate::run::Mpz); }
        crate::run::ecrire_out(&format!("{}\n", g.chaine()));
        std::mem::forget(g);
    }
}

#[allow(dead_code)]
#[unsafe(no_mangle)]
pub extern "C" fn rose_affiche_bool(v: i64) { crate::run::ecrire_out(if v != 0 { "vrai\n" } else { "faux\n" }); }
#[allow(dead_code)]
#[unsafe(no_mangle)]
pub extern "C" fn rose_affiche_f64(v: f64) {
    let t = if v.is_nan() { "non_nombre".to_string() }
        else if v.is_infinite() { if v > 0.0 { "infini".into() } else { "-infini".into() } }
        else if v == v.floor() && v.abs() < 1e16 { (v as i64).to_string() }
        else { crate::front::texte_flottant(v) };
    crate::run::ecrire_out(&format!("{}\n", t));
}
