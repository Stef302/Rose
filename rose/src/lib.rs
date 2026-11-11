// Rose 2.0 — le moteur Rose devient une BIBLIOTHÈQUE.
//
// Ce fichier est le point d'entrée de la crate `rose` en tant que
// bibliothèque : il expose le moteur (analyse, exécution, maths exactes,
// rendu LaTeX) sous forme d'API publique, utilisable par
//
//   * le binaire natif `src/main.rs` (qui continue de fonctionner tel quel :
//     il déclare ses propres modules, la bibliothèque est simplement
//     construite en parallèle) ;
//   * une compilation WebAssembly (`cargo build --release --target
//     wasm32-unknown-unknown --lib`), pour exécuter des programmes Rose
//     dans le navigateur ou dans un runtime sans système de fichiers ;
//   * tout autre programme Rust qui veut embarquer Rose.
//
// Organisation :
//
//   front        lexeur + parseur + vérificateur  (AST Rose)
//   i128x        arithmétique exacte native (BigInt maison, zéro GMP)
//   irrationnel  constantes symboliques exactes (π, e, φ, √n)
//   interp       interpréteur (moteur d'exécution)
//   run          valeurs, bibliothèque standard, sortie, erreurs runtime
//   latex        rendu LaTeX de l'AST
//   sha256       empreinte SHA-256 (autonomie de la crate)
//   jit          JIT Cranelift — NATIF UNIQUEMENT (shim no-op en WASM)
//   wasm         interface C ABI pour le navigateur (sans wasm-bindgen)
//
// Le JIT Cranelift émet du CODE MACHINE NATIF : il est impossible de
// l'embarquer dans un module WebAssembly. En WASM, `jit` est remplacé par un
// shim dont l'API est strictement identique : l'interpréteur compile à
// l'identique et le programme s'exécute en mode interprété (mêmes résultats,
// mêmes messages d'erreur — seule la vitesse change).
//
// Licence MIT.

#![allow(unsafe_op_in_unsafe_fn)]

pub mod front;
pub mod i128x;
pub mod irrationnel;
pub mod interp;
pub mod run;
pub mod latex;
pub mod sha256;

/// JIT Cranelift — disponible uniquement sur les cibles natives.
#[cfg(not(target_arch = "wasm32"))]
pub mod jit;

/// JIT Cranelift — ABSENT en WebAssembly (le JIT émet du code machine natif).
/// Shim d'API STRICTEMENT identique à `src/jit.rs` : `interp.rs` compile à
/// l'identique, sans une seule adaptation, et le programme tourne en mode
/// interprété.
#[cfg(target_arch = "wasm32")]
pub mod jit {
    use crate::front::{Expr, Instr, Programme, Signature, TyKind};
    use crate::run::Valeur;
    use std::collections::HashMap;

    /// Même forme que `jit::Compile` : une fonction compilée, ou le repli
    /// interprété. En WASM rien n'est jamais compilé : la table reste vide.
    #[allow(dead_code)]
    pub enum Compile {
        Natif(Box<dyn Fn(Vec<Valeur>) -> Valeur + Send + Sync>),
        Principal(Box<dyn Fn() + Send + Sync>),
        Interprete,
    }

    /// Aucun type n'est « compilable » en WASM : tout passe par l'interpréteur.
    pub fn compilable(_ty: TyKind) -> bool { false }

    /// Type statique d'une expression : inconnu en WASM (l'analyse de types
    /// du JIT n'a pas lieu d'être).
    pub fn ty_statique(_e: &Expr, _env: &HashMap<String, TyKind>) -> Option<TyKind> { None }

    /// Une fonction utilisateur n'est jamais compilée en WASM.
    pub fn fonction_compilable(_sig: &Signature, _prog: &HashMap<String, Signature>) -> bool { false }

    /// Un bloc d'instructions n'est jamais compilé en WASM.
    pub fn bloc_compilable(
        _instrs: &[Instr],
        _env: &mut HashMap<String, TyKind>,
        _prog: &HashMap<String, Signature>,
        _attendu: Option<TyKind>,
    ) -> bool {
        false
    }

    /// Vide dediscussion sur le pipeline (aucun code machine émis).
    pub fn pipeline_flush_si_threads() {}

    /// Le « JIT » WASM : une coquille vide, toujours opérationnelle.
    pub struct Jit;

    impl Jit {
        pub fn nouveau(_signatures: &HashMap<String, Signature>) -> Jit { Jit }

        /// Aucune fonction n'est compilée : la table est vide et l'appelant
        /// reste sur la voie interprétée.
        pub fn compiler_tout(&mut self) -> HashMap<String, Compile> { HashMap::new() }

        /// Le corps principal n'est jamais compilé.
        pub fn compiler_principal(&mut self, _prog: &Programme) -> Option<Compile> { None }
    }
}

/// Interface C ABI pour le navigateur (WebAssembly). Utilisable aussi en
/// natif (tests, outillage) : les fonctions `extern "C"` y sont exportées
/// telles quelles.
pub mod wasm;

// ── API de haut niveau ────────────────────────────────────────────────────
//
// Ces trois fonctions encapsulent tout ce qu'il faut pour faire tourner un
// programme Rose : analyse, exécution, capture de la sortie. Elles n'ont
// AUCUNE dépendance à un système de fichiers : la sortie du programme est
// capturée en mémoire (voir `run::set_sink` / `run::take_sink`).

/// Exécute un programme Rose et renvoie TOUT ce qu'il a écrit (stdout).
///
/// Les erreurs ne font pas échouer l'appel : elles sont rendues DANS la
/// chaîne renvoyée, au format du moteur natif (« Rose : erreur — … »).
/// Un programme valide renvoie simplement sa sortie.
pub fn rose_executer(source: &str) -> String {
    // La sortie est capturée : en WASM il n'y a pas de stdout, et l'on veut
    // un rendu exact même quand le programme écrit beaucoup.
    run::set_sink(true);
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        interp::executer_source(source, Vec::new())
    }));
    let sortie = run::take_sink().unwrap_or_default();
    match res {
        // Le programme a réussi : sa sortie est le résultat.
        Ok(0) => sortie,
        // L'interpréteur a refusé le programme (vérificateur) : le message
        // d'origine est écrit sur stderr, qui n'existe pas en WASM — on
        // ré-analyse le source pour rendre un message lisible (le message du
        // front-end porte déjà le préfixe « Rose : erreur — »).
        Ok(code) => {
            let msg = message_analyse(source)
                .unwrap_or_else(|| format!("Rose : erreur — le programme n'a pas pu être exécuté (code {})", code));
            let mut s = sortie;
            s.push_str(&msg);
            s.push('\n');
            s
        }
        // Panique runtime (« ROSE_ERR:… ») : elle est convertie en message.
        Err(payload) => {
            let mut s = sortie;
            s.push_str(&format!("Rose : erreur — {}\n", message_panique(&payload)));
            s
        }
    }
}

/// Analyse un programme Rose sans l'exécuter.
///
/// `Ok(resume)` : le source est syntaxiquement valide (le résumé décrit le
/// programme : nombre d'instructions, fonctions, état de la vérification).
/// `Err(msg)` : le source est invalide (message du parseur, au format du
/// moteur : « Rose : erreur — [Ligne L, Colonne C] … »).
///
/// L'état de la vérification est reported DANS le résumé (jamais comme une
/// erreur) : la réparation post-repli du moteur (« $i = $i + 1 ») peut
/// corriger certaines alertes que le vérificateur voit avant réparation.
pub fn rose_analyser(source: &str) -> Result<String, String> {
    let mut prog = front::analyser(source)?;
    let verification = match front::verifier(&mut prog) {
        Ok(()) => "vérification : OK".to_string(),
        // La vérification est signalée SANS faire échouer l'analyse : la
        // réparation post-repli du moteur (« $i = $i + 1 ») peut corriger
        // certaines alertes que le vérificateur voit avant réparation.
        Err(e) => format!("vérification : {}", e),
    };
    let mut fonctions: Vec<String> = prog.ordre.iter().map(|n| {
        let nparams = prog.signatures.get(n).map(|s| s.params.len()).unwrap_or(0);
        format!("{}({})", n, nparams)
    }).collect();
    fonctions.sort();
    Ok(format!(
        "programme valide — {} instruction(s), {} fonction(s){}\n{}",
        prog.instructions.len(),
        prog.ordre.len(),
        if fonctions.is_empty() { String::new() } else { format!(" : {}", fonctions.join(", ")) },
        verification
    ))
}

/// Rend le programme Rose sous forme LaTeX (équations, sommes, fractions…).
///
/// En cas d'erreur d'analyse, la chaîne renvoyée contient le message
/// d'erreur (au format du moteur : « Rose : erreur — … ») au lieu de la
/// formule.
pub fn rose_latex(source: &str) -> String {
    match front::analyser(source) {
        // Le saut de ligne final est PARTIE de la sortie : la commande
        // `rose latex` et l'API WebAssembly rendent ainsi le MÊME texte,
        // au caractère près.
        Ok(prog) => {
            let mut s = latex::programme_latex(&prog);
            s.push('\n');
            s
        }
        Err(e) => e,
    }
}

/// Message d'analyse (pour `rose_executer` quand l'interpréteur refuse le
/// programme) : le message du parseur ou, à défaut, celui du vérificateur.
fn message_analyse(source: &str) -> Option<String> {
    match front::analyser(source) {
        Err(e) => Some(e),
        Ok(mut prog) => match front::verifier(&mut prog) {
            Err(e) => Some(e),
            Ok(()) => None,
        },
    }
}

/// Extrait le message d'une panique : le moteur signale ses erreurs runtime
/// par `panic!("ROSE_ERR:<message>")` (voir `run::erreur_runtime`).
pub fn message_panique(payload: &(dyn std::any::Any + Send)) -> String {
    let brut = if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else {
        "erreur inconnue".to_string()
    };
    match brut.strip_prefix("ROSE_ERR:") {
        Some(m) => m.to_string(),
        None => brut,
    }
}
