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
/// JVM Rose : bytecode `.rosebc`, classes, méthodes et sandbox.
mod rosevm;
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
mod transpile;
#[allow(dead_code)]
mod wasm_lecteur;
#[allow(dead_code)]
mod wasm_runtime;
mod paint;
mod console_win;
mod assoc_windows;
mod gui_native;
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

/// Aide du programme. La version vient du `Cargo.toml` — elle était en
/// dur (« Rose 2.0 »), d'où un `rose` installé en 0.1.5 qui s'annonçait
/// « Rose 2.0 » (mesuré par l'utilisateur).
/// Présentation NEUTRE : le nom de l'outil ayant construit Rose n'apparaît
/// jamais dans l'interface. `JIT`/`AOT` sont des concepts du produit.
fn usage() -> String {
    format!(
        "Rose {} — langage de programmation en français (maths exactes, JIT, AOT, compilation autonome)\n\
Usage :
  rose prog.rose [args…]   exécuter un programme Rose (interpréteur + JIT)
  rose run prog.rose       identique
  rose comp prog.rose [-o sortie]   exécutable autonome prêt à l'emploi (ajoute .exe sur Windows)
  rose latex prog.rose              formule LaTeX du programme entier
  rose latex prog.rose --apercu     idem, rendu VISIBLE (Unicode, sans LaTeX)
  rose latex prog.rose --lignes     seulement les lignes marquees %% (LaTeX brut)
  rose latex prog.rose --lignes --apercu   les lignes %% en rendu visible
  rose tokens prog.rose             liste les jetons (oracle : ligne:colonne type texte)
  rose ast prog.rose                arbre syntaxique (oracle, S-expressions)
  rose rir prog.rose                représentation intermédiaire (RIR)
  rose bc prog.rose [-o sortie]     bytecode JVM Rose .rosebc (classe v2)
  rose rvm fichier.rosebc [classe] [methode] [args...]   exécuter dans la JVM Rose sandboxée
  rose transpile prog.rose --cible js|py|c [-o sortie]   traduction lisible (JavaScript, Python, C)
  rose verif prog.rose              vérifie le programme (oracle : « vérifié » ou message)
  rose llvm prog.rose [-o sortie] [--exe]   génère un fichier C (--exe : exécutable si possible)
  rose aot prog.rose [-o sortie] [-O1|-O2|-O3]   binaire natif optimisé, compilé à l'avance (optionnel). Alias de orc.
  rose orc prog.rose [-o sortie] [-O1|-O2|-O3]   idem, compilation en mémoire (optionnelle)
  rose link objet.o [-o sortie]   lie un objet natif en exécutable
  rose info                état du moteur
  rose console [fichier.rose]   console interactive (REPL) — :help, :more [N] (30 chiffres de plus)
  rose repl [fichier.rose]      identique
  rose debug-lenteur [-n N] [prog.rose]   OÙ VA LE TEMPS ? lancement, analyse, exécution
  rose wasm lib.wasm       fonctions exportées d'un module WebAssembly (pont @dll(\"wasm\", …))
  rose update              installe/met a jour Rose (64 bits, interface console)
  rose assoc [--user|--system] [--lang fr|en]   associer .rose à rose.exe (Windows)
  rose assoc --remove [--user|--system]         retirer l'association .rose
  rose assoc --dry-run                          afficher les commandes d'association sans les appliquer
Note : `rose comp` suffit pour 99 % des usages (exécutable autonome). `aot`, `orc`, `link` et `llvm` sont des commandes avancées.
",
        env!("CARGO_PKG_VERSION")
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

/// Nom de sortie de `rose comp` : `.exe` est ajouté sur Windows quand il
/// manque. Le drapeau `windows` est passé en argument (au lieu de lire
/// `cfg!(windows)` ici) pour que la règle soit testable sous Linux.
/// Un `-o sortie` explicite est honoré : seul le suffixe manquant est ajouté.
fn nom_sortie_comp(sortie: &str, windows: bool) -> String {
    if windows && !sortie.to_ascii_lowercase().ends_with(".exe") {
        format!("{}.exe", sortie)
    } else {
        sortie.to_string()
    }
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

/// ✔ DECISION STRUCTURALE (21/11) — le programme a-t-il un cycle d'appels ?
/// Un cycle = recursion possible = besoin de la pile de 512 Mo. Sans cycle,
/// l'execution dans le main thread (avec `catch_unwind`) evite un
/// `pthread_create` + `join` de ~25 us mesures. Conservatif : toute ambiguite
/// (accolades dans une chaine, graphe non resolu) rend `true` = thread garde.
fn programme_a_cycle_appel(source: &str) -> bool {
    let mut noms: Vec<String> = Vec::new();
    for lig in source.lines() {
        let t = lig.trim_start();
        if let Some(r) = t.strip_prefix("fn ") {
            let n: String = r.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
            if !n.is_empty() { noms.push(n); }
        }
    }
    if noms.is_empty() { return false; }
    // corps de chaque fonction : entre le '{' suivant la signature et son '}' correspondant.
    let mut graphe: Vec<Vec<usize>> = vec![Vec::new(); noms.len()];
    for (i, nom) in noms.iter().enumerate() {
        let motif = format!("fn {}(", nom);
        let mut pos = 0usize;
        let mut trouve = false;
        while let Some(k) = source[pos..].find(&motif) {
            let debut = pos + k;
            if let Some(rel) = source[debut..].find('{') {
                let od = debut + rel;
                let mut prof = 0i32;
                let mut fin = source.len();
                for (j, c) in source[od..].char_indices() {
                    match c {
                        '{' => prof += 1,
                        '}' => { prof -= 1; if prof == 0 { fin = od + j + 1; break; } }
                        _ => {}
                    }
                }
                if prof != 0 { return true; } // ambiguite -> thread
                trouve = true;
                let corps = &source[od..fin];
                for (j, m) in noms.iter().enumerate() {
                    if corps.contains(&format!("{}(", m)) { graphe[i].push(j); }
                }
            }
            pos = debut + motif.len();
        }
        if !trouve { return true; } // signature non resolute -> thread
    }
    // DFS cycle
    let mut couleur = vec![0u8; noms.len()];
    fn dfs(u: usize, g: &[Vec<usize>], c: &mut [u8]) -> bool {
        c[u] = 1;
        for &v in &g[u] {
            if c[v] == 1 { return true; }
            if c[v] == 0 && dfs(v, g, c) { return true; }
        }
        c[u] = 2;
        false
    }
    for i in 0..noms.len() {
        if couleur[i] == 0 && dfs(i, &graphe, &mut couleur) { return true; }
    }
    false
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
    // « orc » et « link » exigent une chaîne AOT locale : soit elle est
    // embarquée dans ce binaire, soit un helper est présent à côté de
    // rose.exe (ou dans le cache/PATH). AUCUN téléchargement, AUCUNE
    // compilation à la demande : si rien n'est installé, le message
    // oriente vers `rose comp` (autonome) ou vers une installation locale.
    // `aot` est l'alias EXPLICITE de `orc` demandé par l'utilisateur.
    let action = if action == "aot" { "orc" } else { action };
    if matches!(action, "orc" | "link") {
        #[cfg(feature = "llvm")]
        cmd_llvm::execute(action, argv.as_slice());
        #[cfg(not(feature = "llvm"))]
        auto_llvm::lance(action, argv.as_slice());
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
        "bc" => {
            if argv.len() < 3 { eprintln!("rose bc : quel programme ?"); std::process::exit(1); }
            match front::analyser_fichier(&argv[2]) {
                Ok(prog) => match rir::abaisser_principal(&prog.instructions) {
                    Some(corps) => {
                        if let Err(e) = rosevm::verifier_sous_ensemble(&corps) {
                            eprintln!("rose bc : {}", e); std::process::exit(1);
                        }
                        // Le programme principal n'a pas de paramètres : on
                        // déclare arité 0 (plutôt que tous les slots) pour que
                        // `rose rvm fichier.rosebc` marche sans argument.
                        let classe = match rosevm::promouvoir_corps_avec_parametres(
                            &corps, "<principal>", 0) {
                            Ok(c) => c,
                            Err(e) => { eprintln!("rose bc : {}", e); std::process::exit(1); }
                        };
                        let octets = rosevm::ecrire_classe(&classe);
                        let chemin = if argv.get(3).map(String::as_str) == Some("-o") {
                            argv.get(4).map(String::as_str).unwrap_or(&argv[2]).to_string()
                        } else if argv.len() >= 4 && !argv[3].starts_with('-') {
                            argv[3].clone()
                        } else {
                            format!("{}.rosebc", argv[2])
                        };
                        if let Err(e) = std::fs::write(&chemin, &octets) {
                            eprintln!("rose bc : écriture impossible : {}", e); std::process::exit(1);
                        }
                        println!("rose bc : {} octets -> {}", octets.len(), chemin);
                        std::process::exit(0);
                    }
                    None => { eprintln!("rose bc : corps principal non abaissable"); std::process::exit(1); }
                },
                Err(e) => { eprintln!("{}", e); std::process::exit(1); }
            }
        }
        "rvm" => {
            if argv.len() < 3 { eprintln!("rose rvm : quel fichier .rosebc ?"); std::process::exit(2); }
            let octets = match std::fs::read(&argv[2]) {
                Ok(o) => o,
                Err(e) => { eprintln!("rose rvm : lecture impossible : {}", e); std::process::exit(1); }
            };
            let classe = argv.get(3).map(String::as_str).unwrap_or("Principal");
            let methode = argv.get(4).map(String::as_str).unwrap_or("<principal>");
            let arguments: Vec<rosevm::Valeur> = argv.iter().skip(5)
                .filter_map(|s| s.parse::<i64>().ok()).map(rosevm::Valeur::z).collect();
            let mut hote = rosevm::HoteBc::neuf(rosevm::GcPool::LIMITE_DEFAUT);
            let limites = rosevm::LimitesJvm::default();
            match rosevm::executer_fichier(&octets, classe, methode, &arguments, &mut hote, &limites) {
                Ok(v) => {
                    print!("{}", hote.sortie);
                    println!("{}", rosevm::chaine_publique(&v));
                    std::process::exit(0);
                }
                Err(e) => { eprintln!("rose rvm : {}", e); std::process::exit(1); }
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
        "transpile" => {
            // Rose -> JS / Python / C. Aucun runtime externe n'est appelé :
            // Node/python3/gcc ne servent qu'à VALIDER la sortie, jamais à
            // exécuter Rose. Ce module est un produit de sortie du moteur.
            if argv.len() < 3 { eprintln!("rose transpile : quel programme ?"); std::process::exit(1); }
            let mut cible = String::new();
            let mut sortie: Option<String> = None;
            let mut fichier = String::new();
            let mut i = 2;
            while i < argv.len() {
                match argv[i].as_str() {
                    "--cible" | "-t" if i + 1 < argv.len() => { cible = argv[i + 1].clone(); i += 2; }
                    "-o" | "--sortie" if i + 1 < argv.len() => { sortie = Some(argv[i + 1].clone()); i += 2; }
                    x if x.starts_with("--cible=") => { cible = x[8..].to_string(); i += 1; }
                    x if x.starts_with('-') => { i += 1; }
                    x => { if fichier.is_empty() { fichier = x.to_string(); } i += 1; }
                }
            }
            if fichier.is_empty() { eprintln!("rose transpile : quel programme ?"); std::process::exit(1); }
            let cible = match transpile::Cible::parser(&cible) {
                Ok(c) => c,
                Err(e) => { eprintln!("{}", e); std::process::exit(1); }
            };
            match transpile::transpiler_fichier(&fichier, cible) {
                Ok(code) => match sortie {
                    Some(p) => match std::fs::write(&p, code.as_bytes()) {
                        Ok(()) => println!("rose: {} produit (transpilation {})", p, cible.nom()),
                        Err(e) => { eprintln!("rose: ne peut écrire {} ({})", p, e); std::process::exit(1); }
                    },
                    None => print!("{}", code),
                },
                Err(e) => { eprintln!("{}", e); std::process::exit(1); }
            }
            std::process::exit(0);
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
            sortie = nom_sortie_comp(&sortie, cfg!(windows));
            std::process::exit(comp_programme(&fichier, &sortie));
        }
        "assoc" => {
            std::process::exit(assoc_windows::execute(&argv));
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
    // ⚠ CLOSE1 — une seule lecture du source sur le chemin courant.
    //
    // `interp::executer_fichier` relit le fichier avec `read_to_string` avant
    // de l'analyser. Quand le programme n'a AUCUN import, `origine` ne sert
    // qu'à résoudre les imports (front::analyser_fichier_rec_source) : le
    // chemin `interp::executer_source(source, args)` est donc exactement le
    // même pipeline (analyser_sans_verifier + verifier), sans la seconde
    // lecture disque. Le test `contains("importer")` est CONSERVATIF : un
    // faux positif (mot dans un commentaire) garde l'ancien chemin, jamais
    // l'inverse — une vraie directive `importer` est toujours écrite en
    // toutes lettres dans le source.
    let sans_imports = !source.contains("importer");
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
    enum Tache {
        Source(String, Vec<String>),
        Fichier(String, Vec<String>),
    }
    // DECISION STRUCTURALE (21/11) : pas de cycle d'appels et pas d'imports
    // => execution DIRECTE dans le main thread. Le `pthread_create`+`join`
    // (25 us mesures) est supprime ; `catch_unwind` garde la gestion de panic.
    let exec_direct = sans_imports && !programme_a_cycle_appel(&source)
        && std::env::var("ROSE_FORCE_THREAD").is_err();
    let tache = if sans_imports {
        Tache::Source(source, args)
    } else {
        Tache::Fichier(fichier.clone(), args)
    };
    let mut rc_direct = None;
    let h = if exec_direct {
        rc_direct = Some(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match &tache {
            Tache::Source(s, a) => interp::executer_source(s, a.clone()),
            Tache::Fichier(f, a) => interp::executer_fichier(f, a.clone()),
        })));
        None
    } else {
        Some(std::thread::Builder::new()
            .stack_size(512 * 1024 * 1024)
            .spawn(move || match tache {
                Tache::Source(s, a) => interp::executer_source(&s, a),
                Tache::Fichier(f, a) => interp::executer_fichier(&f, a),
            }))
    };
    let _t_post = _t_start.elapsed();
    let rc = if exec_direct {
        match rc_direct.expect("catch_unwind direct") {
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
        }
    } else { match h.expect("thread") {
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
        Err(_) => {
            // Repli (création de fil impossible) : les valeurs ont été
            // déplacées dans `tache`, on relit le fichier et on reconstruit
            // les arguments exactement comme l'ancien chemin.
            let a: Vec<String> = if action == "run" {
                std::env::args().skip(3).collect()
            } else {
                std::env::args().skip(2).collect()
            };
            if sans_imports {
                match std::fs::read_to_string(&fichier) {
                    Ok(s) => interp::executer_source(&s, a),
                    Err(_) => 1,
                }
            } else {
                interp::executer_fichier(&fichier, a)
            }
        }
    } };
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
    // Association .rose : double-clic = exécution, clic droit = shell Rose.
    // HKCU, sans élévation ; silencieux si reg.exe est indisponible.
    // ⚠ `poser_association()` (setup_core) a écrit .rose -> Rose.File ;
    //    notre hook réécrit .rose -> Rose.Source.1 et doit rester APRÈS.
    let exe_installe = std::path::PathBuf::from(&prefixe)
        .join("bin")
        .join(if cfg!(windows) { "rose.exe" } else { "rose" });
    if let Err(e) = assoc_windows::appliquer_silencieuse(Some(&exe_installe)) {
        eprintln!("Association .rose : {e}");
    }
}

fn couleur(s: &str, code: &str) -> String {
    format!("\x1b[{}m{}\x1b[0m", code, s)
}

#[cfg(test)]
mod tests_cli_clair {
    use super::*;

    #[test]
    fn comp_extension_exe_windows_seulement() {
        // Linux : le nom explicite est conservé tel quel.
        assert_eq!(nom_sortie_comp("main", false), "main");
        assert_eq!(nom_sortie_comp("sortie.exe", false), "sortie.exe");
        // Windows : .exe ajouté s'il manque, jamais doublé.
        assert_eq!(nom_sortie_comp("main", true), "main.exe");
        assert_eq!(nom_sortie_comp("main.exe", true), "main.exe");
        assert_eq!(nom_sortie_comp("MAIN.EXE", true), "MAIN.EXE");
        // -o explicite : honoré, avec ajout du suffixe manquant sous Windows.
        assert_eq!(nom_sortie_comp("sortie", true), "sortie.exe");
        assert_eq!(nom_sortie_comp("sortie.bin", true), "sortie.bin.exe");
    }

    #[test]
    fn interface_neutre_sans_outil_de_construction() {
        let textes = [usage(), crate::interp::version(), crate::interp::info()];
        for t in &textes {
            let bas = t.to_ascii_lowercase();
            assert!(!bas.contains("rust"), "mention interdite dans : {}", t.lines().next().unwrap_or(""));
            assert!(!bas.contains("cargo"), "mention interdite dans : {}", t.lines().next().unwrap_or(""));
            assert!(!bas.contains("cranelift"), "mention interdite dans : {}", t.lines().next().unwrap_or(""));
        }
    }

    #[test]
    fn aide_sans_chiffres_de_taille_ni_noms_d_outils() {
        let u = usage().to_ascii_lowercase();
        // L'aide décrit des USAGES, jamais l'implémentation ni les outils
        // employés pour construire ou compiler Rose.
        for interdit in [
            "8,8 mo", "8.8 mo", "mo)", "octets", "clang", "gcc",
            "cranelift", "reg add", "outil externe", "outils externes",
        ] {
            assert!(!u.contains(interdit), "mention interdite dans l'aide : {}", interdit);
        }
    }

    #[test]
    fn banniere_et_aide_presentent_le_langage() {
        let u = usage();
        assert!(u.starts_with(&format!(
            "Rose {} — langage de programmation en français (maths exactes, JIT, AOT, compilation autonome)",
            env!("CARGO_PKG_VERSION")
        )));
        assert!(u.contains("99 % des usages"));
        assert!(u.contains("rose comp prog.rose"));
        assert!(u.contains("rose aot prog.rose"));
        assert!(u.contains("rose llvm prog.rose"));
        let v = crate::interp::version();
        assert!(v.starts_with(&format!(
            "Rose {} — langage de programmation en français",
            env!("CARGO_PKG_VERSION")
        )));
        assert!(crate::interp::info().contains("langage de programmation en français"));
    }
}
