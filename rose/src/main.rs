// Rose — compilateur Rose en Rust + JIT Cranelift.
// Front-end du langage (lexeur, vérificateur, repli de constantes),
// interpréteur exact (arithmétique native i128, zéro dépendance) et JIT
// natif Cranelift pour le code chaud.
// Les sorties sont identiques au caractère près entre l'interpréteur et le JIT.
//
// Use of this source code is governed by an MIT-style license.

#![allow(unsafe_op_in_unsafe_fn)]

mod front;
mod i128x;
mod irrationnel;
mod interp;
/// G2 : le vrai JIT Cranelift uniquement avec la feature `jit` (= défaut).
/// Sinon, le shim autonome ci-dessous (émetteur M3.1 + repli interpréteur).
#[cfg(feature = "jit")]
mod jit;
#[cfg(not(feature = "jit"))]
mod jit {
    use std::collections::HashMap;

    use crate::front::{Expr, Instr, Programme, Signature, TyKind};
    use crate::rir::CorpsRir;
    use crate::run::Valeur;

    // Même émetteur M3.1 que `jit.rs`, déclaré au niveau crate sous `jit_ne`.
    use crate::jit_ne as ne;

    pub enum Compile {
        Natif(Box<dyn Fn(Vec<Valeur>) -> Valeur + Send + Sync>),
        #[allow(dead_code)]
        Principal(Box<dyn Fn() + Send + Sync>),
        #[allow(dead_code)]
        Interprete,
    }

    pub fn compilable(_ty: TyKind) -> bool { false }
    pub fn ty_statique(_e: &Expr, _env: &HashMap<String, TyKind>) -> Option<TyKind> { None }
    pub fn fonction_compilable(_sig: &Signature, _prog: &HashMap<String, Signature>) -> bool { false }
    pub fn bloc_compilable(
        _instrs: &[Instr],
        _env: &mut HashMap<String, TyKind>,
        _prog: &HashMap<String, Signature>,
        _attendu: Option<TyKind>,
    ) -> bool { false }
    pub fn pipeline_flush_si_threads() {}

    fn natif_type_ok(t: TyKind) -> bool { matches!(t, TyKind::Entier | TyKind::Booleen) }

    /// Jamais appelés (les fonctions concernées replient sur l'interpréteur).
    extern "C" fn pas_d_appel(_: i64, _: i64, _: i64, _: i64, _: *const u8) -> i64 { 0 }
    extern "C" fn pas_de_fallback(_: *mut std::ffi::c_void, _: u32, _: *mut i64) -> i64 { 0 }

    struct Natif {
        mem: std::sync::Arc<ne::MemoireExecutable>,
        n_slots: usize,
        n_slots_total: usize,
        retour: TyKind,
    }

    impl Natif {
        fn nouveau(corps: &CorpsRir, sig: &Signature) -> Option<Self> {
            if sig.params.iter().any(|(_, t)| !natif_type_ok(t.n)) { return None; }
            // ⚠⚠⚠ K2 : UNE FONCTION SANS ANNOTATION DE RETOUR (`-> Vide`) ne
            // doit JAMAIS être émise en code machine ici. `appeler()` rendrait
            // `Valeur::Vide` pour un slot que l'interpréteur convertit ensuite
            // vers `Entier` → « conversion vers entier impossible (type Vide,
            // val="", ligne=N) » (bug reproduit le 18/11 sur le jardin). Ces
            // procédures restent donc TOUJOURS à l'interpréteur (marqueur
            // `Compile::Interprete` ci-dessous).
            if !natif_type_ok(sig.retour.n) { return None; }
            if !ne::peut_emettre(corps) { return None; }
            let e = ne::emettre(corps, ne::Architecture::native()).ok()?;
            // Sans runtime embarqué : AppelRir / fallbacks → interpréteur.
            if !e.appels.is_empty() || !e.fallbacks.is_empty() { return None; }
            let mem = std::sync::Arc::new(ne::MemoireExecutable::nouveau(&e.code).ok()?);
            Some(Natif { mem, n_slots: e.n_slots, n_slots_total: e.n_slots_total, retour: sig.retour.n })
        }

        fn appeler(&self, args: Vec<Valeur>) -> Valeur {
            if args.len() > self.n_slots { return Valeur::Vide; }
            let mut slots = vec![0i64; self.n_slots_total];
            for (i, a) in args.iter().enumerate() {
                slots[i] = match a {
                    Valeur::Entier(v, None) => *v,
                    Valeur::Booleen(b) => if *b { 1 } else { 0 },
                    Valeur::Flottant(d) => d.to_bits() as i64,
                    _ => return Valeur::Vide,
                };
            }
            let tramp = ne::Trampoline {
                appel_rir: pas_d_appel,
                eval_fallback: pas_de_fallback,
                ctx: std::ptr::null_mut(),
            };
            let r = unsafe { self.mem.appeler(slots.as_mut_ptr(), &tramp) };
            match self.retour {
                TyKind::Vide => Valeur::Vide,
                TyKind::Booleen => Valeur::b(r != 0),
                TyKind::Flottant => Valeur::f(f64::from_bits(r as u64)),
                _ => Valeur::z(r),
            }
        }

        fn dans_compile(self) -> Compile {
            Compile::Natif(Box::new(move |args| self.appeler(args)))
        }
    }

    pub fn compiler_natif(corps: &CorpsRir, sig: &Signature) -> Option<Compile> {
        Natif::nouveau(corps, sig).map(|n| n.dans_compile())
    }

    pub struct Jit {
        signatures: HashMap<String, Signature>,
    }

    impl Jit {
        pub fn nouveau(signatures: &HashMap<String, Signature>) -> Jit {
            Jit { signatures: signatures.clone() }
        }

        pub fn declarer_delegues(&mut self) {}

        pub fn compiler_principal(&mut self, _prog: &Programme) -> Option<Compile> {
            // Sans Cranelift, le RIR principal sature la pile sur les programmes
            // profonds : on force la voie interpréteur AST (voir lib.rs).
            // SAFETY: appelé avant le démarrage de l'interpréteur, aucun thread
            // concurrent ne lit l'environnement.
            unsafe { std::env::set_var("ROSE_NO_RIR_PRINCIPAL", "1"); }
            None
        }

        pub fn compiler_tout(&mut self) -> HashMap<String, Compile> {
            let mut table = HashMap::new();
            for (nom, sig) in &self.signatures {
                if let Some(corps) = sig.rir.as_ref() {
                    if let Some(c) = compiler_natif(corps, sig) {
                        table.insert(nom.clone(), c);
                        continue;
                    }
                }
                // ⚠⚠⚠ K2 — CE MARQUEUR EST LA CORRECTION DU REPLI, NE PAS LE
                // RETIRER. Sans Cranelift, les appels RIR n'ont ni trampoline ni
                // délégués : la voie RIR rend des valeurs fausses (mesuré :
                // `nb_premiers_inferieurs(100)` = 4 au lieu de 25) puis un
                // `Valeur::Vide` converti vers entier (erreur « ligne=90 »).
                // Marquer la fonction `Interprete` la garde dans `self.jit` :
                // `appeler()` saute alors la voie RIR et l'interpréteur AST fait
                // TOUT le travail que l'émetteur ne compile pas.
                table.insert(nom.clone(), Compile::Interprete);
            }
            if std::env::var("ROSE_G2_PROF").is_ok() {
                // Diagnostic HONNÊTE : ne compter que les vraies `Compile::Natif`
                // (l'ancien compteur affichait la taille de la table, donc
                // « 7 fonction(s) sur 7 » alors qu'aucune n'était émise).
                let mut noms: Vec<&String> = table.iter()
                    .filter(|(_, c)| matches!(c, Compile::Natif(_)))
                    .map(|(n, _)| n)
                    .collect();
                noms.sort();
                eprintln!("G2 natif : {} fonction(s) sur {} — {:?}", noms.len(), self.signatures.len(), noms);
            }
            table
        }
    }
}
mod run;
mod latex;
mod debug_lenteur;
mod apercu_latex;
mod auto_llvm;
mod cmd_llvm;
mod sha256;
mod repl;
mod setup_core;
mod velopack;
mod inflate;
mod png;
mod mul_rapide;
mod rir;
#[allow(dead_code)]
mod wasm_lecteur;
#[allow(dead_code)]
mod wasm_runtime;
mod paint;
mod console_win;
mod setup_gui;
#[cfg(feature = "llvm")]
mod aot_llvm_orc;
#[cfg(feature = "llvm")]
mod elf_linker;

/// G2 — émetteur machine M3.1 pour le shim `jit` autonome (sans Cranelift).
/// Déclaré au niveau `crate` pour que `#[path]` soit résolu depuis `src/`,
/// comme la copie équivalente dans `lib.rs`.
#[cfg(not(feature = "jit"))]
#[path = "natif_emetteur.rs"]
mod jit_ne;

use std::io::Read;

fn lire(source: &mut String, chemin: &str) -> bool {
    match std::fs::File::open(chemin) {
        Ok(mut f) => f.read_to_string(source).is_ok(),
        Err(_) => false,
    }
}

/// Nom du moteur affiché dans l'aide : dépend de la feature `jit`.
#[cfg(feature = "jit")]
const MOTEUR: &str = "JIT Cranelift";
#[cfg(not(feature = "jit"))]
const MOTEUR: &str = "émetteur machine natif autonome";

/// Aide du programme. La version vient du `Cargo.toml` — elle était en
/// dur (« Rose 2.0 »), d'où un `rose` installé en 0.1.5 qui s'annonçait
/// « Rose 2.0 » (mesuré par l'utilisateur).
fn usage() -> String {
    format!(
        "Rose {} — compilateur Rust + {} + LLVM ORC AOT (maths exactes BigInt natif, zéro GMP)\n\
Usage :
  rose prog.rose [args…]   interpréter / JIT un programme Rose
  rose run prog.rose       identique
  rose comp prog.rose [-o sortie]   exécutable autonome (moteur + source collé)
  rose latex prog.rose              formule LaTeX du programme entier
  rose latex prog.rose --apercu     idem, rendu VISIBLE (Unicode, sans LaTeX)
  rose latex prog.rose --lignes     seulement les lignes marquees %% (LaTeX brut)
  rose latex prog.rose --lignes --apercu   les lignes %% en rendu visible
  rose tokens prog.rose             liste les jetons (oracle : ligne:colonne type texte)
  rose ast prog.rose                arbre syntaxique (oracle, S-expressions)
  rose rir prog.rose                représentation intermédiaire (RIR)
  rose verif prog.rose              vérifie le programme (oracle : « vérifié » ou message)
  rose llvm prog.rose [-o sortie]   AOT LLVM classique (transpilation C + clang/gcc)
  rose orc prog.rose [-o sortie] [-O1|-O2|-O3] [--pgo-generate|--pgo-use] [--link]   AOT LLVM ORC (in-process, inkwell)
  rose link objet.o [-o sortie]                                               Lier un objet ELF en binaire exécutable
  rose info                état du moteur
  rose console [fichier.rose]   console interactive (REPL) — tapez des commandes Rose
  rose repl [fichier.rose]      identique
  rose debug-lenteur [-n N] [prog.rose]   OÙ VA LE TEMPS ? lancement, analyse, exécution
  rose wasm lib.wasm       fonctions exportées d'un module WebAssembly (pont @dll(\"wasm\", …))
  rose update              installe/met a jour Rose (64 bits, interface console)
",
        env!("CARGO_PKG_VERSION"),
        MOTEUR
    )
}

// ── Oracle RIR (M4.4) ─────────────────────────────────────────────────────

fn rir_oper(o: &rir::Oper) -> &'static str {
    match o {
        rir::Oper::Add => "+", rir::Oper::Sub => "-", rir::Oper::Mul => "*",
        rir::Oper::Div => "/", rir::Oper::Mod => "%", rir::Oper::Puissance => "^",
        rir::Oper::DivEntier => "div",
        rir::Oper::Lt => "<", rir::Oper::Le => "<=", rir::Oper::Gt => ">",
        rir::Oper::Ge => ">=", rir::Oper::Eq => "==", rir::Oper::Ne => "!=",
        rir::Oper::Et => "et", rir::Oper::Ou => "ou",
    }
}

fn rir_eabstrait(e: &rir::Eabstrait, out: &mut String) {
    use rir::Eabstrait;
    match e {
        Eabstrait::Slot(s, _) => { out.push_str("slot "); out.push_str(&s.to_string()); }
        Eabstrait::Entier(t) => { out.push_str("entier "); out.push_str(t); }
        Eabstrait::Flottant(d) => { out.push_str("flottant "); out.push_str(&d.to_string()); }
        Eabstrait::Neg(a) => { out.push_str("(neg "); rir_eabstrait(a, out); out.push(')'); }
        Eabstrait::Non(a) => { out.push_str("(non "); rir_eabstrait(a, out); out.push(')'); }
        Eabstrait::Binaire { op, a, b } => {
            out.push_str("(bin ");
            out.push_str(rir_oper(op));
            out.push(' ');
            rir_eabstrait(a, out);
            out.push(' ');
            rir_eabstrait(b, out);
            out.push(')');
        }
        Eabstrait::AppelRir { nom, args } => {
            out.push_str("(appel_rir ");
            out.push_str(nom);
            for a in args { out.push(' '); ast_expr(a, out); }
            out.push(')');
        }
    }
}

fn rir_op(op: &rir::Op, idx: usize, out: &mut String) {
    use rir::Op;
    out.push_str(&format!("{:04} ", idx));
    match op {
        Op::Affect { slot, expr } => {
            out.push_str(&format!("affect slot {} = ", slot));
            rir_eabstrait(expr, out);
            out.push('\n');
        }
        Op::AffectAst { slot, e, .. } => {
            out.push_str(&format!("affect_ast slot {} = ", slot));
            ast_expr(e, out);
            out.push('\n');
        }
        Op::AffectIndex { slot: _, nom, index, valeur } => {
            out.push_str(&format!("affect_index {}[", nom));
            ast_expr(index, out);
            out.push_str("] = ");
            ast_expr(valeur, out);
            out.push('\n');
        }
        Op::Afficher(exprs) => {
            out.push_str("afficher");
            for e in exprs { out.push(' '); ast_expr(e, out); }
            out.push('\n');
        }
        Op::AfficherAbstrait(e) => {
            out.push_str("afficher_abstrait ");
            rir_eabstrait(e, out);
            out.push('\n');
        }
        Op::Lire { slot, ty } => {
            out.push_str(&format!("lire slot {} {:?}", slot, ty));
            out.push('\n');
        }
        Op::Saut { cible } => { out.push_str(&format!("saut {}\n", cible)); }
        Op::SautSiFaux { test, cible } => {
            out.push_str(&format!("saut_si_faux {} ", cible));
            ast_expr(test, out);
            out.push('\n');
        }
        Op::SautSiFauxAbstrait { test, cible } => {
            out.push_str(&format!("saut_si_faux_abstrait {} ", cible));
            rir_eabstrait(test, out);
            out.push('\n');
        }
        Op::Retour(e) => {
            out.push_str("retour");
            if let Some(ex) = e { out.push(' '); ast_expr(ex, out); }
            out.push('\n');
        }
        Op::RetourAbstrait(e) => {
            out.push_str("retour_abstrait ");
            rir_eabstrait(e, out);
            out.push('\n');
        }
        Op::ExprStmt(e) => {
            out.push_str("expr ");
            ast_expr(e, out);
            out.push('\n');
        }
        Op::Lever(e) => {
            out.push_str("lever ");
            ast_expr(e, out);
            out.push('\n');
        }
        Op::Bloc(instrs) => {
            out.push_str(&format!("bloc ({} instrs)\n", instrs.len()));
        }
    }
}

fn rir_corps_dump(nom: &str, corps: &rir::CorpsRir, params: &[(String, front::Ty)], retour: &front::Ty, out: &mut String) {
    out.push_str(&format!("fonction {}\n", nom));
    for (pn, pt) in params {
        out.push_str(&format!("  param {} {}\n", pn, pt.nom()));
    }
    out.push_str(&format!("  retour {}\n", retour.nom()));
    out.push_str(&format!("  n_slots {}\n", corps.n_slots));
    out.push_str("  noms");
    for (i, n) in corps.noms.iter().enumerate() {
        out.push_str(&format!(" [{}]{}", i, n));
    }
    out.push('\n');
    if !corps.globaux.is_empty() {
        out.push_str("  globaux");
        for g in &corps.globaux { out.push_str(&format!(" {}", g)); }
        out.push('\n');
    }
    out.push_str("  ops\n");
    for (i, op) in corps.ops.iter().enumerate() {
        out.push_str("    ");
        rir_op(op, i, out);
    }
}

fn rir_programme_dump(prog: &mut front::Programme) -> String {
    // Abaissement RIR comme pour l'exécution (M1.2)
    let globaux_connus: Vec<String> = Vec::new();
    let candidats: Vec<String> = prog.ordre.iter()
        .filter(|n| prog.signatures.get(*n).map_or(false, |s| s.corps.is_some()))
        .cloned().collect();
    rir::declarer_rir(&candidats);
    for nom in prog.ordre.clone() {
        if let Some(sig) = prog.signatures.get(&nom) {
            let Some(corps) = sig.corps.as_ref() else { continue; };
            let params: Vec<String> = sig.params.iter().map(|(n, _)| n.clone()).collect();
            if let Some(r) = rir::abaisser(corps, &params, &globaux_connus) {
                prog.signatures.get_mut(&nom).unwrap().rir = Some(r);
            }
        }
    }
    let principal_rir = if std::env::var("ROSE_NO_RIR_PRINCIPAL").is_err() {
        rir::abaisser_principal(&prog.instructions)
    } else { None };

    let mut out = String::from("rir\n");
    let mut noms: Vec<String> = prog.signatures.keys().cloned().collect();
    noms.sort();
    for nom in noms {
        let s = &prog.signatures[&nom];
        if let Some(ref r) = s.rir {
            rir_corps_dump(&nom, r, &s.params, &s.retour, &mut out);
        } else {
            out.push_str(&format!("fonction {} (sans RIR)\n", nom));
        }
    }
    if let Some(r) = principal_rir {
        let vide = front::Ty::vide();
        rir_corps_dump("$principal", &r, &[], &vide, &mut out);
    } else {
        out.push_str("$principal (sans RIR)\n");
    }
    out
}

// ── Oracle AST (M4.2) ─────────────────────────────────────────────────────

fn ast_echapper(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

fn ast_type_nom(t: &front::Ty) -> String {
    t.nom()
}

fn ast_expr(e: &front::Expr, out: &mut String) {
    use front::Expr;
    match e {
        Expr::Litteral { texte, flott, rationnel, neg, .. } => {
            if *flott || *rationnel {
                out.push_str("(f ");
            } else {
                out.push_str("(n ");
            }
            if *neg { out.push('-'); }
            out.push_str(texte);
            out.push(')');
        }
        Expr::LitComplexe { re, im, .. } => {
            // extension Rust : littéral complexe
            out.push_str(&format!("(c {} {})", re, im));
        }
        Expr::ChaineLit(s, _) => {
            out.push_str("(s ");
            out.push_str(&ast_echapper(s));
            out.push(')');
        }
        Expr::BooleenLit(v, _) => {
            out.push_str(if *v { "(b vraie)" } else { "(b faux)" });
        }
        Expr::Var(n, _) => {
            out.push_str("(v ");
            out.push_str(n);
            out.push(')');
        }
        Expr::Appel { nom, module, args, .. } => {
            out.push_str("(app ");
            if !module.is_empty() {
                out.push_str(module);
                out.push_str("::");
            }
            out.push_str(nom);
            for a in args {
                out.push(' ');
                ast_expr(a, out);
            }
            out.push(')');
        }
        Expr::Index { a, b, .. } => {
            out.push_str("(idx ");
            ast_expr(a, out);
            out.push(' ');
            ast_expr(b, out);
            out.push(')');
        }
        Expr::Binaire { op, a, b, .. } => {
            out.push_str("(bin ");
            out.push_str(op);
            out.push(' ');
            ast_expr(a, out);
            out.push(' ');
            ast_expr(b, out);
            out.push(')');
        }
        Expr::Unaire { op, a, .. } => {
            out.push_str("(uni ");
            out.push_str(op);
            out.push(' ');
            ast_expr(a, out);
            out.push(')');
        }
        Expr::Factorielle(a, _) => { out.push_str("(fact "); ast_expr(a, out); out.push(')'); }
        Expr::MultiFactorielle(a, k, _) => {
            out.push_str("(mfact ");
            out.push_str(&k.to_string());
            out.push(' ');
            ast_expr(a, out);
            out.push(')');
        }
        Expr::SousFactorielle(a, _) => { out.push_str("(sfact "); ast_expr(a, out); out.push(')'); }
        Expr::Primorielle(a, _) => { out.push_str("(prim "); ast_expr(a, out); out.push(')'); }
        Expr::Terminal(a, _) => { out.push_str("(term "); ast_expr(a, out); out.push(')'); }
        Expr::SuperFactorielle(a, _) => { out.push_str("(super "); ast_expr(a, out); out.push(')'); }
        Expr::ListeLit(elems, _) => {
            out.push_str("(lst");
            for e in elems { out.push(' '); ast_expr(e, out); }
            out.push(')');
        }
        Expr::ListeVideTypee(ty, _) => {
            // extension : liste vide typée
            out.push_str("(lstvide ");
            out.push_str(&front::Ty::simple(*ty).nom());
            out.push(')');
        }
        Expr::TableLit(paires, _) => {
            out.push_str("(tbl");
            for (k, v) in paires {
                out.push(' ');
                out.push_str(&ast_echapper(k));
                out.push(' ');
                ast_expr(v, out);
            }
            out.push(')');
        }
    }
}

fn ast_ind(n: usize) -> String { "  ".repeat(n) }

fn ast_instrs(instrs: &[front::Instr], ind: usize, out: &mut String) {
    for i in instrs { ast_instr(i, ind, out); }
}

fn ast_instr(i: &front::Instr, ind: usize, out: &mut String) {
    use front::Instr;
    let p = ast_ind(ind);
    match i {
        Instr::Afficher(args, _) => {
            out.push_str(&p);
            out.push_str("afficher");
            for a in args { out.push(' '); ast_expr(a, out); }
            out.push('\n');
        }
        Instr::Lire { nom, .. } => {
            out.push_str(&p);
            out.push_str("lire ");
            out.push_str(nom);
            out.push('\n');
        }
        Instr::Affect { nom, e, .. } => {
            out.push_str(&p);
            out.push_str("affect ");
            out.push_str(nom);
            out.push(' ');
            ast_expr(e, out);
            out.push('\n');
        }
        Instr::AffectIndex { nom, index, valeur, .. } => {
            out.push_str(&p);
            out.push_str("affect_index ");
            out.push_str(nom);
            out.push(' ');
            ast_expr(index, out);
            out.push(' ');
            ast_expr(valeur, out);
            out.push('\n');
        }
        Instr::Si { test, alors, sinon, .. } => {
            out.push_str(&p);
            out.push_str("si ");
            ast_expr(test, out);
            out.push('\n');
            out.push_str(&p);
            out.push_str("alors\n");
            ast_instrs(alors, ind + 1, out);
            if let Some(s) = sinon {
                out.push_str(&p);
                out.push_str("sinon\n");
                ast_instrs(s, ind + 1, out);
            }
        }
        Instr::TantQue { test, corps, .. } => {
            out.push_str(&p);
            out.push_str("tant_que ");
            ast_expr(test, out);
            out.push('\n');
            ast_instrs(corps, ind + 1, out);
        }
        Instr::Pour { nom, debut, fin, corps, .. } => {
            out.push_str(&p);
            out.push_str("pour ");
            out.push_str(nom);
            out.push(' ');
            ast_expr(debut, out);
            out.push(' ');
            ast_expr(fin, out);
            out.push('\n');
            ast_instrs(corps, ind + 1, out);
        }
        Instr::Retour(e, _) => {
            out.push_str(&p);
            out.push_str("retour");
            if let Some(ex) = e { out.push(' '); ast_expr(ex, out); }
            out.push('\n');
        }
        Instr::ExprStmt(e) => {
            out.push_str(&p);
            out.push_str("expr ");
            ast_expr(e, out);
            out.push('\n');
        }
        Instr::Bloc(b) => {
            out.push_str(&p);
            out.push_str("bloc\n");
            ast_instrs(b, ind + 1, out);
        }
        Instr::Spawn(b, _) => {
            out.push_str(&p);
            out.push_str("spawn\n");
            ast_instrs(b, ind + 1, out);
        }
        Instr::Essayer { corps, attraper, finalment, .. } => {
            out.push_str(&p);
            out.push_str("essayer\n");
            ast_instrs(corps, ind + 1, out);
            if let Some((nom, b)) = attraper {
                out.push_str(&p);
                out.push_str("attraper ");
                out.push_str(nom);
                out.push('\n');
                ast_instrs(b, ind + 1, out);
            }
            if let Some(b) = finalment {
                out.push_str(&p);
                out.push_str("finalement\n");
                ast_instrs(b, ind + 1, out);
            }
        }
        Instr::Lever(e, _) => {
            out.push_str(&p);
            out.push_str("lever ");
            ast_expr(e, out);
            out.push('\n');
        }
        Instr::Rir(_) => {
            out.push_str(&p);
            out.push_str("rir\n");
        }
    }
}

fn ast_programme(prog: &front::Programme) -> String {
    let mut out = String::from("programme\n");
    // signatures triées par nom (ordre déterministe)
    let mut noms: Vec<&String> = prog.signatures.keys().collect();
    noms.sort();
    for nom in noms {
        let s = &prog.signatures[nom];
        out.push_str("fonction ");
        out.push_str(&s.nom);
        out.push('\n');
        if s.corps.is_none() {
            out.push_str("externe ");
            out.push_str(&ast_echapper(&s.dll));
            out.push(' ');
            out.push_str(&ast_echapper(&s.dll_son));
            out.push(' ');
            out.push_str(&ast_echapper(&s.symbole));
            out.push('\n');
        }
        for (pn, pt) in &s.params {
            out.push_str("param ");
            out.push_str(pn);
            out.push(' ');
            out.push_str(&ast_type_nom(pt));
            out.push('\n');
        }
        out.push_str("retour ");
        out.push_str(&ast_type_nom(&s.retour));
        out.push('\n');
        if let Some(corps) = &s.corps {
            ast_instrs(corps, 1, &mut out);
        }
    }
    ast_instrs(&prog.instructions, 0, &mut out);
    out
}

/* ── mode bundle : programme collé dans le moteur  ── */
const ROSE_MAGIC: [u8; 8] = [b'R', b'O', b'S', b'E', b'B', b'1', 0x01, 0x02];

fn chemin_moteur() -> Option<String> {
    std::env::current_exe().ok().and_then(|p| p.to_str().map(|s| s.to_string()))
}

/// Si l'exécutable courant porte une queue bundle, en retourne le source.
fn lire_bundle() -> Option<String> {
    let chemin = chemin_moteur()?;
    let octets = std::fs::read(&chemin).ok()?;
    if octets.len() < 24 { return None; }
    let q = &octets[octets.len() - 16..];
    if q[8..] != ROSE_MAGIC { return None; }
    let mut ln: u64 = 0;
    for i in 0..8 { ln |= (q[i] as u64) << (8 * i); }
    let ln = ln as usize;
    if ln == 0 || ln > octets.len() - 16 { return None; }
    let src = &octets[octets.len() - 16 - ln..octets.len() - 16];
    let mut s = String::from_utf8_lossy(src).into_owned();
    // normaliser fins de ligne (miroir normaliser_fins_de_ligne)
    if s.contains("\r\n") { s = s.replace("\r\n", "\n"); }
    if s.contains('\r') { s = s.replace('\r', "\n"); }
    Some(s)
}

/// comp : valide le source, copie le moteur puis le source, puis la queue.
fn comp_programme(fichier: &str, sortie: &str) -> i32 {
    let mut source = String::new();
    if !lire(&mut source, fichier) {
        eprintln!("rose: fichier introuvable : {}", fichier);
        return 1;
    }
    // valide avant d'embarquer (analyse + repli + vérification)
    match front::analyser(&source) {
        Ok(mut prog) => {
            if let Err(e) = front::verifier(&mut prog) {
                eprintln!("Rose : erreur — {}", e);
                return 1;
            }
        }
        Err(e) => { eprintln!("Rose : erreur — {}", e); return 1; }
    }
    let moteur = match chemin_moteur().and_then(|c| std::fs::read(c).ok()) {
        Some(m) => m,
        None => { eprintln!("rose: moteur illisible"); return 1; }
    };
    let mut out = std::fs::File::create(sortie).unwrap_or_else(|_| {
        eprintln!("rose: ne peut écrire {}", sortie);
        std::process::exit(1);
    });
    use std::io::Write;
    let _ = out.write_all(&moteur);
    let _ = out.write_all(source.as_bytes());
    let ln = source.len() as u64;
    let mut q = [0u8; 16];
    for i in 0..8 { q[i] = ((ln >> (8 * i)) & 0xFF) as u8; }
    q[8..].copy_from_slice(&ROSE_MAGIC);
    let _ = out.write_all(&q);
    drop(out);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(sortie, std::fs::Permissions::from_mode(0o755));
    }
    println!("rose: {} produit (exécutable autonome)", sortie);
    0
}

fn main() {
    let _t_start = std::time::Instant::now();
    // ⚠⚠ MESURE : le temps passé dans la préparation de la console
    // (`SetConsoleTitleW` + fil d'icône) est ventilé à part quand
    // `ROSE_TIMING` est posé. C'est le suspect n°2 de la « lenteur avant
    // la première sortie » signalée sur Windows.
    let _t_entree = _t_start.elapsed();
    // Titre et icône de la fenêtre de console (Windows). Doit être fait
    // AVANT toute écriture : le double-clic sur un `.rose` ouvre cette
    // fenêtre, et elle doit porter la rose de marque.
    console_win::preparer();
    let _t_console = _t_start.elapsed();
    // Hook de panic silencieux pour les erreurs Rose récupérables (ROSE_ERR:),
    // afin d'éviter le bruit « thread panicked » sur stderr. Les vrais bugs
    // (panics non ROSE_ERR) sont toujours affichés.
    std::panic::set_hook(Box::new(|info| {
        let msg = info.payload().downcast_ref::<String>()
            .map(|s| s.as_str())
            .or_else(|| info.payload().downcast_ref::<&str>().copied())
            .unwrap_or("");
        if msg.starts_with("ROSE_ERR:") { return; }
        eprintln!("{}", info);
    }));
    let mut argv: Vec<String> = std::env::args().collect();
    let console_apres = argv.iter().any(|a| a == "--console");
    argv.retain(|a| a != "--console");
    // Velopack : le launcher relance l'application avec un argument interne
    // (`--veloapp-updated`, `--veloapp-obsolete`) — il n'est PAS un nom de
    // fichier, on le route avant toute autre action.
    if velopack::is_update_launcher(&argv) {
        std::process::exit(velopack::run_update_launcher(&argv));
    }
    if argv.len() < 2 {
        // aucun argument : si l'exécutable porte un bundle, l'exécuter (mode
        // autonome — lire_bundle).
        if let Some(source) = lire_bundle() {
            let source_t = source.clone();
            let h = std::thread::Builder::new().stack_size(8 * 1024 * 1024)
                .spawn(move || interp::executer_source(&source_t, Vec::new()));
            let rc = match h { Ok(j) => j.join().unwrap_or(1), Err(_) => interp::executer_source(&source, Vec::new()) };
            std::process::exit(rc);
        }
        eprint!("{}", usage());
        std::process::exit(1);
    }
    let action = argv[1].as_str();
    // ⚠ `debug-lenteur` : diagnostic du temps de lancement / analyse /
    // exécution. Il ne dépend de RIEN (std seul) et se place AVANT toute
    // initialisation lourde.
    if action == "debug-lenteur" || action == "lenteur" {
        std::process::exit(debug_lenteur::execute(&argv));
    }
    // La commande « llvm » (transpilation vers C) n'a pas besoin de LLVM :
    // elle est disponible meme dans le binaire leger.
    if action == "llvm" {
        cmd_llvm::execute("llvm", argv.as_slice());
    }
    // « orc » et « link » exigent LLVM : soit il est embarque, soit le
    // binaire le cherche tout seul (telechargement du helper AOT).
    // IMPORTANT : lancer le helper AVANT le message d'absence — l'ancien
    // code imprimait « non embarquee » puis sortait, ce qui rendait le
    // telechargement a la demande inatteignable dans le binaire leger.
    if matches!(action, "orc" | "link") {
        #[cfg(feature = "llvm")]
        cmd_llvm::execute(action, argv.as_slice());
        #[cfg(not(feature = "llvm"))]
        {
            // 1. helper déjà présent (cache ou à côté de l'exécutable) ?
            if let Some(helper) = auto_llvm::trouver_helper() {
                auto_llvm::lance_depuis(&helper, action, argv.as_slice());
            }
            // 2. sinon : téléchargement depuis la release GitHub (ou miroir)
            match auto_llvm::installer_helper() {
                Ok(helper) => auto_llvm::lance_depuis(&helper, action, argv.as_slice()),
                Err(e) => {
                    eprintln!("rose: la chaîne AOT LLVM n'est pas disponible.");
                    eprintln!("      {}", e);
                    eprintln!("      autre solution : cargo build --release --features llvm");
                    std::process::exit(1);
                }
            }
        }
    }
    match action {
        "version" | "--version" => { println!("{}", interp::version()); return; }
        "info" => { print!("{}", interp::info()); return; }
        "--help" | "-h" | "aide" => { print!("{}", usage()); return; }
        "latex" => {
            // RENDU LaTeX DU PROGRAMME ENTIER : c'est la synthèse de ce
            // qu'on a programmé. La commande `run` montre, instruction par
            // instruction, la formule de chaque affichage ; celle-ci donne la
            // formule GLOBALE du programme.
            if argv.len() < 3 { eprintln!("rose: quel programme rendre ?"); std::process::exit(1); }
            let mut source = String::new();
            if !lire(&mut source, &argv[2]) {
                eprintln!("rose: fichier introuvable : {}", argv[2]);
                std::process::exit(1);
            }
            // `--lignes` : ne rendre QUE les lignes marquées `%%`.
            //
            // ⚠ On ne passe PAS par l'analyseur : une ligne `%% $N!$` n'est
            // pas du code Rose valide, et le programme peut être incomplet.
            // Le marqueur est une ANNOTATION, pas une instruction — le
            // fichier reste exécutable tel quel, l'analyseur ignore `%%`
            // comme un commentaire (TeX) s'il le rencontre.
            //
            // ⚠⚠ LA SORTIE COMPORTE UN `\\` APRÈS CHAQUE LIGNE, Y COMPRIS LA
            // DERNIÈRE (16/11). L'ancienne version le séparait seulement
            // ENTRE les lignes : la dernière marque se retrouvait collée à
            // la suivante (`$n!$` sans `\\`), donc invisible dans un
            // `align`/`gather` LaTeX. Les tests unitaires l'avaient figé
            // (« sans séparateur final ») — c'est le COMPORTEMENT qui était
            // faux, pas le test.
            // ⚠ `--apercu` : rendu VISIBLE (Unicode) au lieu du LaTeX brut.
            // C'est la réponse à « un programme qui contient du latex pour
            // voir dans ton output ce que ça donne » : on vérifie le rendu
            // sans LaTeX installé, et toute commande non gérée s'affiche
            // entre « » au lieu d'être perdue.
            let apercu = argv.iter().any(|a| a == "--apercu" || a == "-a");
            if argv.iter().any(|a| a == "--lignes" || a == "-l") {
                let texte = latex::programme_latex_lignes(&source);
                if apercu {
                    for ligne in texte.lines() {
                        let l = ligne.trim_end_matches(" \\\\").trim_end();
                        print!("{}\n", apercu_latex::rendre(l));
                    }
                } else {
                    print!("{}", texte);
                }
                return;
            }
            let prog = match front::analyser(&source) {
                Ok(p) => p,
                Err(e) => { eprintln!("Rose : erreur — {}", e); std::process::exit(1); }
            };
            let texte = latex::programme_latex(&prog);
            if apercu {
                for ligne in texte.lines() {
                    let l = ligne.trim_end_matches(" \\\\").trim_end();
                    if l.is_empty() { continue; }
                    print!("{}\n", apercu_latex::rendre(l));
                }
                return;
            }
            print!("{}", texte);
            println!();
            return;
        }
        "tokens" => {
            if argv.len() < 3 { eprintln!("rose tokens : quel programme ?"); std::process::exit(1); }
            let mut source = String::new();
            if !lire(&mut source, &argv[2]) {
                eprintln!("rose: fichier introuvable : {}", argv[2]);
                std::process::exit(1);
            }
            let toks = match front::tokeniser(&source) {
                Ok(t) => t,
                Err(e) => { eprintln!("{}", e); std::process::exit(1); }
            };
            fn tk_nom_cpp(t: &front::Tk) -> String {
                use front::Tk;
                match t {
                    Tk::Eof => "eof".into(), Tk::Ident => "ident".into(), Tk::Mot => "mot".into(),
                    Tk::Nombre => "nombre".into(), Tk::Flottant => "flottant".into(), Tk::Chaine => "chaine".into(),
                    Tk::Plus => "+".into(), Tk::Moins => "-".into(), Tk::Etoile => "*".into(),
                    Tk::Slash => "/".into(), Tk::Pourcent => "%".into(), Tk::Chapeau => "^".into(),
                    Tk::Bang => "!".into(), Tk::Diese => "#".into(), Tk::Terminal => "?".into(),
                    Tk::SuperFacto => "$".into(), Tk::MultiFacto(_) => "!".into(), Tk::SousFacto => "!".into(),
                    Tk::DivEntier => "div".into(), Tk::Affect => "=".into(), Tk::LParen => "(".into(),
                    Tk::RParen => ")".into(), Tk::LBrace => "{".into(), Tk::RBrace => "}".into(),
                    Tk::LBracket => "[".into(), Tk::RBracket => "]".into(), Tk::Semi => ";".into(),
                    Tk::Virgule => ",".into(), Tk::DeuxPts => ":".into(), Tk::DeuxPoints => "::".into(),
                    Tk::FlecheD => ">>".into(), Tk::FlecheG => "<<".into(), Tk::PointDInterro => "->".into(),
                    Tk::Inf => "<".into(), Tk::Sup => ">".into(), Tk::InfEg => "<=".into(),
                    Tk::SupEg => ">=".into(), Tk::DoubleEg => "==".into(), Tk::Diff => "!=".into(),
                    Tk::Environ => "≈".into(), Tk::Racine => "racine".into(), Tk::Tilde => "~".into(),
                    Tk::Arobase => "@".into(), Tk::Question => "?".into(),
                }
            }
            fn echapper_token(s: &str) -> String {
                let mut o = String::with_capacity(s.len());
                for c in s.chars() {
                    match c {
                        '\\' => o.push_str("\\\\"),
                        '\n' => o.push_str("\\n"),
                        '\t' => o.push_str("\\t"),
                        '\r' => o.push_str("\\r"),
                        '\x1f' => o.push_str("\\u241f"),
                        _ => o.push(c),
                    }
                }
                o
            }
            for t in &toks {
                // M4.1 oracle : ligne:colonne type texte (format C++ tk_nom)
                println!("{}:{} {} {}", t.ligne, t.col, tk_nom_cpp(&t.tk), echapper_token(&t.texte));
            }
            std::process::exit(0);
        }
        "ast" => {
            if argv.len() < 3 { eprintln!("rose ast : quel programme ?"); std::process::exit(1); }
            let mut source = String::new();
            if !lire(&mut source, &argv[2]) {
                eprintln!("rose: fichier introuvable : {}", argv[2]);
                std::process::exit(1);
            }
            let toks = match front::tokeniser(&source) {
                Ok(t) => t,
                Err(e) => { eprintln!("{}", e); std::process::exit(1); }
            };
            let prog = match front::Parser::nouveau(toks).programmer() {
                Ok(p) => p,
                Err(e) => { eprintln!("{}", e); std::process::exit(1); }
            };
            print!("{}", ast_programme(&prog));
            std::process::exit(0);
        }
        "rir" => {
            if argv.len() < 3 { eprintln!("rose rir : quel programme ?"); std::process::exit(1); }
            match front::analyser_fichier(&argv[2]) {
                Ok(mut prog) => {
                    print!("{}", rir_programme_dump(&mut prog));
                    std::process::exit(0);
                }
                Err(e) => { eprintln!("{}", e); std::process::exit(1); }
            }
        }
        "verif" => {
            if argv.len() < 3 { eprintln!("rose verif : quel programme ?"); std::process::exit(1); }
            // Résolution des imports par rapport au fichier.
            match front::analyser_fichier(&argv[2]) {
                Ok(_) => { println!("vérifié"); std::process::exit(0); }
                Err(e) => { eprintln!("{}", e); std::process::exit(1); }
            }
        }
        "wasm" => {
            // ⚠ M6.1 : inspecte un module WebAssembly SANS l'exécuter (défaut).
            // M6.2 : --runtime officiel / --install-runtime / --export / --args.
            let mut runtime_officiel = false;
            let mut fichier_opt: Option<String> = None;
            let mut export: Option<String> = None;
            let mut arguments: Vec<i64> = Vec::new();
            let mut i = 2;
            while i < argv.len() {
                match argv[i].as_str() {
                    "--runtime" if i + 1 < argv.len() => {
                        runtime_officiel = matches!(argv[i + 1].as_str(),
                            "officiel" | "official" | "wasmtime" | "wasmer");
                        i += 2;
                    }
                    "--install-runtime" if i + 1 < argv.len() => {
                        match wasm_runtime::RuntimeOfficiel::depuis_str(&argv[i + 1]) {
                            Some(rt) => match wasm_runtime::installer(rt) {
                                Ok(p) => println!("rose wasm : runtime installé : {}", p.display()),
                                Err(e) => { eprintln!("rose wasm : {e}"); std::process::exit(1); }
                            },
                            None => { eprintln!("rose wasm : runtime inconnu"); std::process::exit(1); }
                        }
                        std::process::exit(0);
                    }
                    "--export" if i + 1 < argv.len() => { export = Some(argv[i + 1].clone()); i += 2; }
                    "--args" if i + 1 < argv.len() => {
                        match argv[i + 1].parse::<i64>() {
                            Ok(v) => arguments.push(v),
                            Err(_) => { eprintln!("rose wasm : argument invalide : {}", argv[i + 1]); std::process::exit(1); }
                        }
                        i += 2;
                    }
                    a if a.starts_with('-') => { i += 1; }
                    a => { fichier_opt = Some(a.to_string()); i += 1; }
                }
            }
            let fichier = match fichier_opt {
                Some(f) => f,
                None => { eprintln!("rose wasm : quel module ? (rose wasm lib.wasm)"); std::process::exit(1); }
            };

            if let Some(nom_export) = export {
                let choix = if runtime_officiel { Some(wasm_runtime::RuntimeOfficiel::Wasmtime) } else { None };
                match wasm_runtime::executer_export(
                    std::path::Path::new(&fichier), &nom_export, &arguments, choix
                ) {
                    Ok(r) => {
                        for v in &r.valeurs { println!("{v}"); }
                        if let Some(motif) = &r.repli { eprintln!("rose wasm : {motif}"); }
                    }
                    Err(e) => { eprintln!("rose wasm : {e}"); std::process::exit(1); }
                }
                std::process::exit(0);
            }

            // Inspection par défaut (M6.1) : fonctions exportées et leurs types.
            let octets = match std::fs::read(&fichier) {
                Ok(o) => o,
                Err(e) => { eprintln!("rose wasm : {fichier} illisible ({e})"); std::process::exit(1); }
            };
            let module = match wasm_lecteur::charger(&octets) {
                Ok(m) => m,
                Err(e) => { eprintln!("rose wasm : {fichier} : {e}"); std::process::exit(1); }
            };
            let inst = wasm_lecteur::InstanceWasm::nouveau(std::sync::Arc::new(module));
            let mut n = 0;
            for (nom, params, resultats) in inst.exports_fonctions() {
                let ty = |t: wasm_lecteur::TyWasm| match t {
                    wasm_lecteur::TyWasm::I32 => "entier",
                    wasm_lecteur::TyWasm::I64 => "entier64",
                    wasm_lecteur::TyWasm::F32 => "flottant32",
                    wasm_lecteur::TyWasm::F64 => "flottant",
                };
                let ps: Vec<String> = params.iter().enumerate()
                    .map(|(i, t)| format!("a{i}: {}", ty(*t))).collect();
                let r = resultats.first().map(|t| ty(*t)).unwrap_or("vide");
                println!("@dll(\"wasm\", \"{fichier}\")");
                println!("@sym(\"{nom}\")");
                println!("fn {nom}({}) -> {r}", ps.join(", "));
                println!();
                n += 1;
            }
            eprintln!("{n} fonction(s) exportée(s)");
            std::process::exit(0);
        }
        "comp" => {
            let mut sortie = String::new();
            let mut fichier = String::new();
            let mut i = 2;
            while i < argv.len() {
                if (argv[i] == "-o" || argv[i] == "--sortie") && i + 1 < argv.len() {
                    sortie = argv[i + 1].clone(); i += 2;
                } else if !argv[i].starts_with('-') { fichier = argv[i].clone(); i += 1; }
                else { i += 1; }
            }
            if fichier.is_empty() { eprintln!("rose: quel programme compiler ?"); std::process::exit(1); }
            if sortie.is_empty() {
                sortie = fichier.rsplit_once(['/', '\\']).map(|(_, f)| f.to_string()).unwrap_or(fichier.clone());
                if let Some(p) = sortie.rfind(".rose") { sortie.truncate(p); }
                if sortie.is_empty() { sortie = "programme".into(); }
            }
            std::process::exit(comp_programme(&fichier, &sortie));
        }
        "console" | "repl" => {
            let fichier = if argv.len() >= 3 { Some(argv[2].as_str()) } else { None };
            std::process::exit(repl::lancer(fichier, true));
        }
        "update" | "install" => {
            install_rose();
            return;
        }
        _ => {}
    }



    let (fichier, args) = if action == "run" {
        if argv.len() < 3 { eprintln!("rose: quel programme interpréter ?"); std::process::exit(1); }
        (argv[2].clone(), argv[3..].to_vec())
    } else {
        (argv[1].clone(), argv[2..].to_vec())
    };
    let mut source = String::new();
    if !lire(&mut source, &fichier) {
        eprintln!("rose: fichier introuvable : {}", fichier);
        std::process::exit(1);
    }
    // le pipeline s'execute dans un thread a pile de 512 Mo
    // ; la recursion profonde de l'interpreteur Rust en a
    // autant. Repli en ligne si le thread ne peut etre cree.
    if argv.iter().any(|a| a == "--debug") || std::env::var("ROSE_DEBUG").is_ok() {
        crate::run::set_debug_mode(true);
    }
    if argv.iter().any(|a| a == "--step") || std::env::var("ROSE_STEP").is_ok() {
        crate::run::set_step_mode(true);
    }
    let source_t = source.clone();
    let args_t = args.clone();
    let fichier_t = fichier.clone();
    let _t_pret = _t_start.elapsed();
    // ⚠⚠⚠ 512 Mo DE PILE, PAS 8 Mo (mesuré le 16/11).
    //
    // Le commentaire d'origine dit « pile de 512 Mo :
    // une pile de 512 Mo ». La constante, elle, valait 8 Mo : mesuré,
    // `it(10000)` (récursion 10 000, multiplication entière donc hors JIT)
    // débordait la pile vers 4 300 niveaux, alors que le moteur de
    // référence répond 450015000 pour la même récursion. Une pile de 512 Mo
    // comme la pile de référence couvre les récursions profondes que le JIT ne peut pas
    // prendre (corps avec multiplication/puissance = BigInt exact).
    //
    // ⚠ Les 512 Mo sont RÉSERVÉS (mmap), pas engagés : aucune mémoire n'est
    // touchée tant que la récursion n'y descend pas.
    // ⚠⚠⚠ L'EXÉCUTION PASSE PAR LE FICHIER (imports résolus) (mesuré le 17/11).
    let h = std::thread::Builder::new()
        .stack_size(512 * 1024 * 1024)
        .spawn(move || interp::executer_fichier(&fichier_t, args_t));
    let _t_post = _t_start.elapsed();
    let rc = match h {
        Ok(j) => match j.join() {
            Ok(r) => r,
            Err(payload) => {
                let msg = if let Some(s) = payload.downcast_ref::<String>() { s.clone() }
                          else if let Some(s) = payload.downcast_ref::<&str>() { s.to_string() }
                          else { "erreur inconnue".to_string() };
                if let Some(m) = msg.strip_prefix("ROSE_ERR:") {
                    eprintln!("Rose : erreur — {}", m);
                } else {
                    eprintln!("Rose : panic — {}", msg);
                }
                1
            }
        },
        Err(_) => interp::executer_fichier(&fichier, args),
    };
    if console_apres {
        repl::lancer(Some(&fichier), false);
    }
    if std::env::var("ROSE_TIMING").is_ok() {
        // ⚠ Ventilation COMPLÈTE : entrée dans main, préparation console,
        // lecture du fichier, spawn du fil d'exécution, total.
        eprintln!("TIMING entree_main={:?} console={:?} pret={:?} post_spawn={:?}",
                  _t_entree, _t_console, _t_pret, _t_post);
    }
    // ⚠ La fenêtre de console ouverte par un DOUBLE-CLIC sur un `.rose` se
    // ferme avec le programme : l'utilisateur n'a pas le temps de lire la
    // sortie (« la fenêtre apparaît et disparaît »). On attend une frappe
    // quand la console n'appartient qu'à nous.
    // En mode REPL, c'est le REPL qui attend (à sa sortie) : on ne fait pas
    // attendre deux fois.
    if !console_apres {
        console_win::attendre_fermeture();
    }
    std::process::exit(rc);
}


/* ── console interactive : voir src/repl.rs ─────────────────────────── */
// `rose console` et le binaire autonome `rose-shell` partagent le même
// module `repl` : une seule implémentation, deux points d'entrée.

/* ── installeur intégré (rose install) ──────────────────────────────── */
/* 64 bits natif, interface console colorée, pas de NSIS. */

fn install_rose() {
    use std::io::{self, Write, BufRead};
    // « rose update --force » : retélécharge même si la version est à jour
    let force = std::env::args().any(|a| a == "--force" || a == "-f");

    // Windows : mise à jour via Velopack (delta + rollback) — activation
    // explicite : « rose update --velopack » ou ROSE_VELOPACK=1. Les builds
    // installés par Velopack portent la variable VELOCABI (posée par le
    // launcher) et basculent automatiquement.
    // (le module est portable : la voie Velopack est vérifiable sous Linux
    // avec ROSE_VELOPACK_URL pointant sur un manifeste local `file://`).
    {
        let args: Vec<String> = std::env::args().collect();
        let veut_velopack = std::env::var("ROSE_VELOPACK").is_ok()
            || std::env::var("VELOCABI").is_ok()
            || args.iter().any(|a| a == "--velopack");
        if veut_velopack {
            println!("Mise à jour Velopack…");
            match velopack::check_and_update(true) {
                Ok(Some(v)) => {
                    // Sans launcher Velopack (installation faite à la main par
                    // Rose-win-Setup.exe), on applique NOUS-MÊMES le paquet :
                    // sinon le fichier reste dans packages\ et rien ne change.
                    match velopack::appliquer_paquets() {
                        Ok(Some(appliquee)) => println!(
                            "Rose {appliquee} installé — relancez « rose » (l'ancienne version reste en place pour un rollback)."
                        ),
                        Ok(None) => println!(
                            "Paquet {v} déposé — le launcher Velopack l'appliquera au prochain démarrage."
                        ),
                        Err(e) => eprintln!("Application du paquet : {e}"),
                    }
                }
                Ok(None) => println!("Rose est déjà à jour."),
                Err(e) => eprintln!("Velopack : {e} — repli sur l'installeur classique."),
            }
            return;
        }
    }

    println!("{}", couleur("╔══════════════════════════════════════╗", "36"));
    println!("{}", couleur("║   Rose — installation / mise à jour      ║", "36"));
    println!("{}", couleur("╚══════════════════════════════════════╝", "36"));
    println!();

    let existant = setup_core::installation_existante();
    let prefixe = match existant {
        Some(ref p) => {
            println!("{} {}", couleur("Installation existante :", "33"), p);
            println!();
            println!("  1) Mettre à jour depuis GitHub (dernière release)");
            println!("  2) Réinstaller (écraser tous les fichiers)");
            println!("  3) Réparer (réinstaller les fichiers manquants)");
            println!("  4) Désinstaller");
            println!("  5) Annuler");
            print!("Choix [1-5] : ");
            let _ = io::stdout().flush();
            let mut buf = String::new();
            let _ = io::stdin().lock().read_line(&mut buf);
            match buf.trim() {
                "1" => {
                    println!("Recherche de la dernière release…");
                    for l in setup_core::mettre_a_jour(p, force) {
                        println!("{}", l);
                    }
                    return;
                }
                "4" => {
                    for l in setup_core::desinstaller(p) {
                        println!("{}", l);
                    }
                    return;
                }
                "5" => { println!("Annulé."); return; }
                _ => p.clone(),
            }
        }
        None => {
            let p = setup_core::prefixe_defaut();
            println!("Installation dans : {}", p);
            p
        }
    };

    for l in setup_core::installer(&prefixe, None) {
        println!("{}", l);
    }
    println!();
    println!("{}", couleur(&format!("Rose {} est installé.", env!("CARGO_PKG_VERSION")), "32"));
    println!("  Dossier : {}", prefixe);
    println!();
    println!("Ouvrez un nouveau terminal et tapez : rose --version");
}

fn couleur(s: &str, code: &str) -> String {
    format!("\x1b[{}m{}\x1b[0m", code, s)
}
