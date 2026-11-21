//! Commandes de compilation de la ligne de commande :
//! `rose llvm` (transpilation vers C), `rose orc` / `rose aot`
//! (exécutable machine natif) et `rose link` (vérification d'un produit).
//!
//! Le chemin natif utilise UNIQUEMENT les modules intégrés au binaire :
//!   front (analyse) → rir (abaissement) → natif_emetteur (octets machine)
//!   → link_natif (ELF64 / PE64 exécutable).
//! Aucun programme tiers, aucun téléchargement, aucun réseau.
//!
//! `rose llvm` garde sa sémantique historique : le transpileur intégré
//! (`src/transpile.rs`) produit du code C. Cette partie n'est pas modifiée.
//!
//! Licence MIT.

use crate::front::{Expr, Instr};
use std::path::Path;

/// Conseil unique quand le programme ne peut pas devenir un exécutable natif.
/// Aucun nom d'outil externe n'apparaît dans les messages utilisateur.
const CONSEIL_COMP: &str = "utilisez `rose comp`, qui produit un exécutable autonome";

/// Exécute l'action demandée. Ne rend jamais (termine le processus).
pub fn execute(action: &str, argv: &[String]) -> ! {
    match action {
        "llvm" => llvm_codegen(argv),
        "orc" | "aot" => orc(argv),
        "link" => link(argv),
        _ => {
            eprintln!("rose: commande inconnue : {}", action);
            std::process::exit(1);
        }
    }
}

/// `rose llvm prog.rose [-o sortie.c] [--exe]` : transpilation vers C par le
/// transpileur intégré (`rose/src/transpile.rs`), sans dépendance LLVM ni
/// Python. Avec `--exe`, un compilateur C local est utilisé si disponible ;
/// sinon le fichier C est rendu et l'absence est expliquée.
fn llvm_codegen(argv: &[String]) -> ! {
    let mut sortie = String::new();
    let mut fichier = String::new();
    let mut veut_exe = false;
    let mut i = 2;
    while i < argv.len() {
        if argv[i] == "--exe" || argv[i] == "--compil" {
            veut_exe = true; i += 1;
        } else if (argv[i] == "-o" || argv[i] == "--sortie") && i + 1 < argv.len() {
            sortie = argv[i + 1].clone(); i += 2;
        } else if !argv[i].starts_with('-') { fichier = argv[i].clone(); i += 1; }
        else { i += 1; }
    }
    if fichier.is_empty() { eprintln!("rose: quel programme compiler ?"); std::process::exit(1); }
    if sortie.is_empty() {
        sortie = fichier.rsplit_once(['/', '\\']).map(|(_, f)| f.to_string()).unwrap_or(fichier.clone());
        if let Some(p) = sortie.rfind(".rose") { sortie.truncate(p); }
        if sortie.is_empty() { sortie = "programme".into(); }
        sortie.push_str(".c");
    }
    // Même API que `rose transpile --cible c` : un seul transpileur, en Rust.
    let code = match crate::transpile::transpiler_fichier(&fichier, crate::transpile::Cible::C) {
        Ok(c) => c,
        Err(e) => { eprintln!("rose: transpilation impossible : {e}"); std::process::exit(1); }
    };
    if let Err(e) = std::fs::write(&sortie, code.as_bytes()) {
        eprintln!("rose: impossible d'écrire {sortie} : {e}"); std::process::exit(1);
    }
    if !veut_exe {
        println!("rose: {} produit (code C)", sortie);
        std::process::exit(0);
    }
    let exe = match sortie.strip_suffix(".c") {
        Some(p) => p.to_string(),
        None => format!("{}.exe", sortie),
    };
    match trouver_compilateur_c() {
        Some(compilateur) => {
            let statut = std::process::Command::new(compilateur)
                .arg("-O2").arg(&sortie).arg("-o").arg(&exe).arg("-lm").status();
            match statut {
                Ok(s) if s.success() => println!("rose: {} produit (exécutable)", exe),
                Ok(s) => {
                    eprintln!("rose: compilation C impossible (code {}) ; {} reste disponible", s, sortie);
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("rose: compilation C impossible ({e}) ; {} reste disponible", sortie);
                    std::process::exit(1);
                }
            }
        }
        None => println!("rose: aucun compilateur C disponible ; {} produit (code C)", sortie),
    }
    std::process::exit(0);
}

/// Cherche un compilateur C déjà installé (aucun téléchargement).
fn trouver_compilateur_c() -> Option<&'static str> {
    ["cc", "gcc", "clang"].into_iter().find(|nom| {
        std::process::Command::new(nom).arg("--version").output()
            .map(|s| s.status.success()).unwrap_or(false)
    })
}

/// Refuse clairement un programme hors du sous-ensemble machine, sans jamais
/// écrire un fichier qui ressemblerait à un exécutable mais ne le serait pas.
fn refuser(raison: &str) -> ! {
    eprintln!("rose: compilation native impossible : {}", raison);
    eprintln!("rose: {}", CONSEIL_COMP);
    std::process::exit(1);
}

/// Architecture et format de sortie déduits d'une cible éventuelle
/// (`--target`). Sans indication, on reste sur la machine courante.
fn cible(target: Option<&str>) -> (crate::natif_emetteur::Architecture, bool) {
    use crate::natif_emetteur::Architecture;
    let t = target.unwrap_or("");
    let arch = if t.contains("aarch64") || t.contains("arm64") {
        Architecture::Aarch64
    } else if t.contains("x86_64") || t.contains("amd64") {
        Architecture::X86_64
    } else {
        Architecture::native()
    };
    let windows = if t.contains("windows") || t.contains("msvc") || t.contains("mingw") {
        true
    } else if t.contains("linux") || t.contains("darwin") || t.contains("macos") {
        false
    } else {
        cfg!(windows)
    };
    (arch, windows)
}

/// Nom complet du produit : `.exe` est ajouté sur Windows quand il manque.
fn nom_executable(sortie: &str, windows: bool) -> String {
    if windows && !sortie.to_ascii_lowercase().ends_with(".exe") {
        format!("{}.exe", sortie)
    } else {
        sortie.to_string()
    }
}

// ═══════════════════════════════════════════════════════════════════════
// ABAISSEMENT SPÉCIFIQUE AOT — linéarisation des appels
// ═══════════════════════════════════════════════════════════════════════
//
// ⚠ `rir.rs` N'EST PAS MODIFIÉ : `rose rir` (et le point fixe b3ed9262)
// garde exactement sa sortie. Cette passe ne sert qu'à `rose aot`.
//
// `Eabstrait::AppelRir` n'accepte que des arguments qui sont des VARIABLES.
// Or `f(n-1)` passe un calcul, et `f(n-1) + f(n-2)` n'est donc pas abstractible.
// On insuffle ici des temporaires AVANT l'abaissement :
//
//   retour f(n-1) + f(n-2)
//   → t0 = n-1 ; t1 = f(t0) ; t2 = n-2 ; t3 = f(t2) ; retour t1 + t3
//
// Tout devient alors abstractible, donc émettable en code machine.

struct Lineariseur<'a> {
    /// Fonctions Rose qui auront un corps RIR (et donc un corps machine).
    connus: &'a std::collections::HashSet<String>,
    /// Tous les noms de variables déjà présents (évite toute collision).
    utilises: std::collections::HashSet<String>,
    compteur: usize,
}

impl<'a> Lineariseur<'a> {
    fn nouveau(connus: &'a std::collections::HashSet<String>) -> Self {
        Lineariseur { connus, utilises: std::collections::HashSet::new(), compteur: 0 }
    }

    fn collecter_bloc(bloc: &[Instr], noms: &mut std::collections::HashSet<String>) {
        for i in bloc {
            match i {
                Instr::Affect { nom, .. } | Instr::AffectIndex { nom, .. }
                | Instr::Lire { nom, .. } | Instr::Pour { nom, .. } => { noms.insert(nom.clone()); }
                Instr::Si { alors, sinon, .. } => {
                    Self::collecter_bloc(alors, noms);
                    if let Some(s) = sinon { Self::collecter_bloc(s, noms); }
                }
                Instr::TantQue { corps, .. } | Instr::Pour { corps, .. } | Instr::Bloc(corps)
                | Instr::Spawn(corps, _) => Self::collecter_bloc(corps, noms),
                Instr::Essayer { corps, attraper, finalment, .. } => {
                    Self::collecter_bloc(corps, noms);
                    if let Some((_, b)) = attraper { Self::collecter_bloc(b, noms); }
                    if let Some(b) = finalment { Self::collecter_bloc(b, noms); }
                }
                _ => {}
            }
        }
    }

    fn temporaire(&mut self) -> String {
        loop {
            let n = format!("aotv{}", self.compteur);
            self.compteur += 1;
            if self.utilises.insert(n.clone()) { return n; }
        }
    }

    /// Réécrit une expression ; les temporaires nécessaires sont ajoutés à
    /// `pre` (à insérer AVANT l'instruction courante).
    fn expr(&mut self, e: &Expr, pre: &mut Vec<Instr>) -> Expr {
        use crate::front::Expr as E;
        match e {
            E::Appel { nom, module, args, ligne } => {
                let args: Vec<Expr> = args.iter().map(|a| self.expr(a, pre)).collect();
                let directs = module.is_empty() && self.connus.contains(nom.as_str());
                let mut fin = Vec::with_capacity(args.len());
                for a in args {
                    if directs && !matches!(a, E::Var(..)) {
                        let t = self.temporaire();
                        pre.push(Instr::Affect { nom: t.clone(), e: a, ligne: *ligne });
                        fin.push(E::Var(t, *ligne));
                    } else {
                        fin.push(a);
                    }
                }
                E::Appel { nom: nom.clone(), module: module.clone(), args: fin, ligne: *ligne }
            }
            E::Binaire { op, a, b, ligne } => E::Binaire {
                op: op.clone(), a: Box::new(self.expr(a, pre)),
                b: Box::new(self.expr(b, pre)), ligne: *ligne,
            },
            E::Unaire { op, a, ligne } => E::Unaire {
                op: op.clone(), a: Box::new(self.expr(a, pre)), ligne: *ligne,
            },
            E::Index { a, b, ligne } => E::Index {
                a: Box::new(self.expr(a, pre)), b: Box::new(self.expr(b, pre)), ligne: *ligne,
            },
            E::ListeLit(v, ligne) => E::ListeLit(
                v.iter().map(|x| self.expr(x, pre)).collect(), *ligne),
            E::TableLit(v, ligne) => E::TableLit(
                v.iter().map(|(k, x)| (k.clone(), self.expr(x, pre))).collect(), *ligne),
            E::Factorielle(a, l) => E::Factorielle(Box::new(self.expr(a, pre)), *l),
            E::MultiFactorielle(a, k, l) => E::MultiFactorielle(Box::new(self.expr(a, pre)), *k, *l),
            E::SousFactorielle(a, l) => E::SousFactorielle(Box::new(self.expr(a, pre)), *l),
            E::Primorielle(a, l) => E::Primorielle(Box::new(self.expr(a, pre)), *l),
            E::Terminal(a, l) => E::Terminal(Box::new(self.expr(a, pre)), *l),
            E::SuperFactorielle(a, l) => E::SuperFactorielle(Box::new(self.expr(a, pre)), *l),
            autre => autre.clone(),
        }
    }

    fn bloc(&mut self, bloc: &[Instr]) -> Vec<Instr> {
        use crate::front::Instr as I;
        let mut out = Vec::with_capacity(bloc.len());
        for i in bloc {
            let mut pre: Vec<Instr> = Vec::new();
            let ni = match i {
                I::Affect { nom, e, ligne } => I::Affect {
                    nom: nom.clone(), e: self.expr(e, &mut pre), ligne: *ligne,
                },
                I::AffectIndex { nom, index, valeur, ligne } => {
                    let index = self.expr(index, &mut pre);
                    let valeur = self.expr(valeur, &mut pre);
                    I::AffectIndex { nom: nom.clone(), index, valeur, ligne: *ligne }
                }
                I::Afficher(es, ligne) => I::Afficher(
                    es.iter().map(|e| self.expr(e, &mut pre)).collect(), *ligne),
                I::Si { test, alors, sinon, ligne } => {
                    let alors = self.bloc(alors);
                    let sinon = sinon.as_ref().map(|s| self.bloc(s));
                    let test = self.expr(test, &mut pre);
                    I::Si { test, alors, sinon, ligne: *ligne }
                }
                I::TantQue { test, corps, ligne } => {
                    let corps = self.bloc(corps);
                    let test = self.expr(test, &mut pre);
                    I::TantQue { test, corps, ligne: *ligne }
                }
                I::Pour { nom, debut, fin, corps, ligne } => {
                    let corps = self.bloc(corps);
                    let debut = self.expr(debut, &mut pre);
                    let fin = self.expr(fin, &mut pre);
                    I::Pour { nom: nom.clone(), debut, fin, corps, ligne: *ligne }
                }
                I::Retour(e, ligne) => I::Retour(e.as_ref().map(|x| self.expr(x, &mut pre)), *ligne),
                I::ExprStmt(e) => I::ExprStmt(self.expr(e, &mut pre)),
                I::Lever(e, ligne) => I::Lever(self.expr(e, &mut pre), *ligne),
                I::Bloc(b) => I::Bloc(self.bloc(b)),
                autre => autre.clone(),
            };
            out.extend(pre);
            out.push(ni);
        }
        out
    }
}

/// Vrai si un corps RIR est émettable SANS la moindre trampoline (mode AOT).
fn corps_emettable_aot(corps: &crate::rir::CorpsRir) -> bool {
    use crate::rir::{Eabstrait, Oper};
    fn expr(e: &Eabstrait) -> bool {
        match e {
            Eabstrait::Slot(..) | Eabstrait::Entier(_) => true,
            Eabstrait::Binaire { op, a, b } => {
                matches!(op, Oper::Add | Oper::Sub | Oper::Mul | Oper::Lt
                          | Oper::Le | Oper::Gt | Oper::Ge | Oper::Eq | Oper::Ne)
                    && expr(a) && expr(b)
            }
            Eabstrait::Neg(a) | Eabstrait::Non(a) => expr(a),
            Eabstrait::AppelRir { args, .. } => {
                args.iter().all(|a| matches!(a, Expr::Var(..)))
            }
            _ => false,
        }
    }
    use crate::rir::Op;
    corps.ops.iter().all(|op| match op {
        Op::Affect { expr: e, .. } | Op::RetourAbstrait(e)
        | Op::SautSiFauxAbstrait { test: e, .. } => expr(e),
        Op::Saut { .. } => true,
        _ => false,
    })
}

/// Noms des fonctions appelées par un corps RIR (appels directs uniquement).
fn appels_du_corps(corps: &crate::rir::CorpsRir, sortie: &mut Vec<String>) {
    use crate::rir::Eabstrait;
    fn visite(e: &Eabstrait, sortie: &mut Vec<String>) {
        match e {
            Eabstrait::Binaire { a, b, .. } => { visite(a, sortie); visite(b, sortie); }
            Eabstrait::Neg(a) | Eabstrait::Non(a) => visite(a, sortie),
            Eabstrait::AppelRir { nom, .. } => sortie.push(nom.clone()),
            _ => {}
        }
    }
    use crate::rir::Op;
    for op in &corps.ops {
        match op {
            Op::Affect { expr, .. } | Op::RetourAbstrait(expr)
            | Op::SautSiFauxAbstrait { test: expr, .. } => visite(expr, sortie),
            _ => {}
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// AOT « VALEURS EXACTES » (vague 78, AOT3)
// ═══════════════════════════════════════════════════════════════════════
//
// `>> 565#` fait 228 chiffres : le RIR historique ne sait représenter qu'un
// i64 et refuse le programme. Or l'expression est un TERME CONSTANT : sa
// valeur est entièrement déterminée à la compilation. On la calcule donc
// UNE fois avec l'interpréteur Rose (le même `eval` que `rose comp` / `rose
// prog.rose`), puis on mémorise son texte décimal dans le binaire natif ; le
// stub l'écrit sur stdout par un `write` syscall.
//
// Cela couvre aussi `>> 100!`, `>> 20?`, `>> 30$`, `>> !10`, `>> 7!!`…
// Aucun changement au RIR pour les autres programmes : la voie historique
// est inchangée, octet pour octet.

/// Calcule la valeur d'un programme réduit à `>> <terme constant>`.
///
/// Rend `(texte_avec_nouvelle_ligne, parties_basses_i64)` ou `None` dès que
/// le programme n'est pas exactement ce cas (l'appelant retombe alors sur le
/// chemin RIR historique). Le texte est construit exactement comme
/// `run::afficher` le ferait, pour garantir l'égalité octet pour octet avec
/// l'interpréteur.
fn valeur_constante_aot(prog: &crate::front::Programme) -> Option<(String, i64)> {
    if prog.instructions.len() != 1 { return None; }
    let expr = match &prog.instructions[0] {
        Instr::Afficher(es, _) if es.len() == 1 && crate::rir::est_constant(&es[0]) => &es[0],
        _ => return None,
    };
    let valeur = crate::interp::Interp::nouveau(prog.clone()).eval(expr).ok()?;
    let (texte, bas) = match &valeur {
        crate::run::Valeur::Entier(n, None) => (n.to_string(), *n),
        crate::run::Valeur::Entier(_, Some(z)) => {
            use num_traits::ToPrimitive;
            // Les 64 bits bas donnent le code de retour du processus. Il est
            // tronqué pour les grands entiers (comme le faisait déjà le stub
            // i64), mais le texte complet, lui, est exact.
            let bas = (&z.v() & num_bigint::BigInt::from(u64::MAX))
                .to_u64().unwrap_or(0) as i64;
            (z.chaine(), bas)
        }
        // Seuls les entiers sont des « valeurs exactes » au sens AOT3.
        _ => return None,
    };
    Some((format!("{}\n", texte), bas))
}

/// `rose orc prog.rose [-o sortie]` — alias : `rose aot`.
///
/// Analyse, abaisse en RIR, émet les octets machine puis écrit DIRECTEMENT
/// un exécutable ELF64 ou PE64. La valeur rendue par le programme (son
/// dernier affichage) devient le code de retour du processus.
///
/// Toutes les fonctions Rose du programme sont compilées dans le MÊME
/// segment : les appels (y compris récursifs) deviennent des appels directs,
/// sans runtime externe ni trampoline.
fn orc(argv: &[String]) -> ! {
    let mut sortie = String::new();
    let mut fichier = String::new();
    let mut target: Option<String> = None;
    let mut i = 2;
    while i < argv.len() {
        if (argv[i] == "-o" || argv[i] == "--sortie") && i + 1 < argv.len() {
            sortie = argv[i + 1].clone(); i += 2;
        } else if argv[i] == "--target" && i + 1 < argv.len() {
            target = Some(argv[i + 1].clone()); i += 2;
        } else if argv[i].starts_with("--target=") {
            target = Some(argv[i]["--target=".len()..].to_string()); i += 1;
        } else if argv[i].starts_with('-') {
            // Options historiques sans effet sur l'émetteur intégré
            // (niveaux d'optimisation, profilage, lien implicite).
            i += 1;
        } else { fichier = argv[i].clone(); i += 1; }
    }
    if fichier.is_empty() { eprintln!("rose: quel programme compiler ?"); std::process::exit(1); }
    if sortie.is_empty() {
        sortie = fichier.rsplit_once(['/', '\\']).map(|(_, f)| f.to_string()).unwrap_or(fichier.clone());
        if let Some(p) = sortie.rfind(".rose") { sortie.truncate(p); }
        if sortie.is_empty() { sortie = "programme".into(); }
    }
    let (arch, windows) = cible(target.as_deref());
    let sortie = nom_executable(&sortie, windows);

    let mut prog = match crate::front::analyser_fichier(&fichier) {
        Ok(p) => p,
        Err(e) => { eprintln!("rose: analyse impossible : {}", e); std::process::exit(1); }
    };
    if let Err(e) = crate::front::verifier(&mut prog) {
        eprintln!("rose: programme invalide : {}", e);
        std::process::exit(1);
    }

    // ── AOT « valeurs exactes » (vague 78) ──────────────────────────────
    // `>> 565#` est un terme constant : l'interpréteur le calcule ici, une
    // seule fois, et le binaire natif mémorise son texte décimal. On tente
    // cette voie AVANT tout abaissement RIR, afin que `rose rir` et le
    // point fixe b3ed9262 restent strictement inchangés.
    if let Some((texte, bas)) = valeur_constante_aot(&prog) {
        if windows {
            refuser("l'écriture d'une valeur exacte n'est pas disponible sur PE ; utilisez `rose comp` sur Windows");
        }
        let Some(corps) = crate::rir::abaisser_principal_valeur_exacte(&prog.instructions, bas)
        else { refuser("la valeur finale du programme ne peut pas être calculée directement") };
        let programme = match crate::natif_emetteur::emettre_valeur_exacte(
            &corps, arch, texte.as_bytes())
        {
            Ok(p) => p,
            Err(e) => refuser(&format!("traduction en code machine impossible : {}", e)),
        };
        debug_assert!(programme.fallbacks.is_empty() && programme.appels.is_empty());
        let mut options = crate::link_natif::Options::nouveau(arch);
        options.n_slots_total = programme.n_slots_total;
        options.sortie_exacte = Some(texte.into_bytes());
        let chemin = Path::new(&sortie);
        if let Err(e) = crate::link_natif::ecrire_elf(&programme.code, &options, chemin) {
            eprintln!("rose: écriture de {} impossible : {}", sortie, e);
            std::process::exit(1);
        }
        println!("rose: {} produit (exécutable natif {}, valeur exacte mémorisée)",
                 sortie, arch.nom());
        std::process::exit(0);
    }

    // Fonctions Rose qui auront un corps machine. `declarer_rir` doit être
    // appelé AVANT l'abaissement pour que `abstraire` produise des AppelRir.
    let candidats: Vec<String> = prog.ordre.iter()
        .filter(|n| prog.signatures.get(*n).map_or(false, |s| s.corps.is_some()))
        .cloned().collect();
    let connus: std::collections::HashSet<String> = candidats.iter().cloned().collect();
    let mut lin = Lineariseur::nouveau(&connus);
    for sig in prog.signatures.values() {
        for (p, _) in &sig.params { lin.utilises.insert(p.clone()); }
    }
    for nom in &prog.ordre {
        if let Some(sig) = prog.signatures.get(nom) {
            if let Some(corps) = sig.corps.as_ref() {
                Lineariseur::collecter_bloc(corps, &mut lin.utilises);
            }
        }
    }
    Lineariseur::collecter_bloc(&prog.instructions, &mut lin.utilises);
    crate::rir::declarer_rir(&candidats);

    // Le dernier `>> expression` devient la valeur de retour du code machine.
    let principaux = lin.bloc(&prog.instructions);
    let principal = crate::rir::abaisser_principal_sortie(&principaux)
        .or_else(|| crate::rir::abaisser_principal(&principaux));
    let Some(principal) = principal else {
        refuser("la valeur finale du programme ne peut pas être calculée directement");
    };
    if !principal.ops.iter().any(|op| matches!(op, crate::rir::Op::RetourAbstrait(_))) {
        refuser("le programme ne rend pas de valeur calculable directement");
    }
    if !corps_emettable_aot(&principal) {
        refuser("la valeur finale du programme demande le moteur d'exécution");
    }

    // Toutes les fonctions sont compilées dans le même segment. Une fonction
    // non émettable est écartée ; elle ne bloque la compilation QUE si elle
    // est effectivement appelée depuis le programme principal.
    // Une fonction qui lit/écrit une globale du principal ne peut pas être
    // émise avec un simple pointeur de trame local : on la refuse proprement
    // plutôt que de produire un binaire qui lirait un slot non initialisé.
    let mut noms_principaux: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    Lineariseur::collecter_bloc(&prog.instructions, &mut noms_principaux);
    let globaux_principaux: Vec<String> = noms_principaux.into_iter().collect();

    let mut fonctions: Vec<(String, crate::rir::CorpsRir)> =
        vec![("$principal".to_string(), principal)];
    let mut refusees: std::collections::HashSet<String> = std::collections::HashSet::new();
    for nom in &prog.ordre {
        let Some(sig) = prog.signatures.get(nom) else { continue };
        let Some(corps) = sig.corps.as_ref() else { continue };
        let corps_lineaire = lin.bloc(corps);
        let params: Vec<String> = sig.params.iter().map(|(n, _)| n.clone()).collect();
        match crate::rir::abaisser(&corps_lineaire, &params, &globaux_principaux) {
            Some(rir) if corps_emettable_aot(&rir) && rir.globaux.is_empty() => {
                fonctions.push((nom.clone(), rir))
            }
            _ => { refusees.insert(nom.clone()); }
        }
    }

    // Fermeture transitive des appels : les fonctions inatteignables sont
    // simplement omises du binaire (elles ne coûtent rien à personne).
    let mut a_visiter: Vec<String> = vec!["$principal".to_string()];
    let mut atteintes: std::collections::HashSet<String> = std::collections::HashSet::new();
    while let Some(nom) = a_visiter.pop() {
        if !atteintes.insert(nom.clone()) { continue; }
        if refusees.contains(&nom) {
            refuser(&format!("la fonction {} utilise des valeurs qui demandent le moteur d'exécution", nom));
        }
        if let Some((_, c)) = fonctions.iter().find(|(n, _)| n == &nom) {
            let mut appels = Vec::new();
            appels_du_corps(c, &mut appels);
            a_visiter.extend(appels);
        } else {
            refuser(&format!("la fonction {} appelée par le programme n'est pas émettable", nom));
        }
    }
    fonctions.retain(|(n, _)| atteintes.contains(n));

    let programme = match crate::natif_emetteur::emettre_programme(&fonctions, arch) {
        Ok(p) => p,
        Err(e) => refuser(&format!("traduction en code machine impossible : {}", e)),
    };
    if !programme.fallbacks.is_empty() {
        refuser("certaines opérations demandent le moteur d'exécution");
    }
    let mut options = crate::link_natif::Options::nouveau(arch);
    options.n_slots_total = programme.n_slots_total;
    // ELF : la valeur complète est ALSO écrite sur stdout (le code de retour
    // POSIX est limité à 8 bits). Le PE garde son stub d'entrée historique.
    options.afficher_valeur = !windows;

    let chemin = Path::new(&sortie);
    let ecriture = if windows {
        crate::link_natif::ecrire_pe(&programme.code, &options, chemin)
    } else {
        crate::link_natif::ecrire_elf(&programme.code, &options, chemin)
    };
    if let Err(e) = ecriture {
        eprintln!("rose: écriture de {} impossible : {}", sortie, e);
        std::process::exit(1);
    }
    println!("rose: {} produit (exécutable natif {}, code de retour = valeur du programme)",
             sortie, arch.nom());
    std::process::exit(0);
}

/// `rose link fichier [-o sortie]` : le produit de `rose aot` est déjà un
/// exécutable complet ; cette commande ne fait que le vérifier et, si demandé,
/// le copier sous un autre nom. Aucun fichier intermédiaire n'est nécessaire.
fn link(argv: &[String]) -> ! {
    let mut objet = String::new();
    let mut sortie = String::new();
    let mut i = 2;
    while i < argv.len() {
        if (argv[i] == "-o" || argv[i] == "--sortie") && i + 1 < argv.len() {
            sortie = argv[i + 1].clone(); i += 2;
        } else if !argv[i].starts_with('-') {
            objet = argv[i].clone(); i += 1;
        } else { i += 1; }
    }
    if objet.is_empty() { eprintln!("rose: quel exécutable vérifier ?"); std::process::exit(1); }
    let octets = match std::fs::read(&objet) {
        Ok(o) => o,
        Err(e) => { eprintln!("rose: lecture de {} impossible : {}", objet, e); std::process::exit(1); }
    };
    let est_elf = octets.starts_with(b"\x7fELF");
    let est_pe = octets.starts_with(b"MZ");
    if !est_elf && !est_pe {
        eprintln!("rose: {} n'est pas un exécutable natif", objet);
        eprintln!("rose: {}", CONSEIL_COMP);
        std::process::exit(1);
    }
    if sortie.is_empty() {
        println!("rose: {} est déjà un exécutable natif", objet);
        std::process::exit(0);
    }
    if let Err(e) = std::fs::write(&sortie, &octets) {
        eprintln!("rose: écriture de {} impossible : {}", sortie, e);
        std::process::exit(1);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&sortie, std::fs::Permissions::from_mode(0o755));
    }
    println!("rose: {} produit (exécutable natif)", sortie);
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cible_choisit_architecture_et_format() {
        use crate::natif_emetteur::Architecture;
        assert_eq!(cible(Some("x86_64-unknown-linux-gnu")).0, Architecture::X86_64);
        assert_eq!(cible(Some("aarch64-unknown-linux-gnu")).0, Architecture::Aarch64);
        assert!(cible(Some("x86_64-pc-windows-msvc")).1);
        assert!(!cible(Some("aarch64-unknown-linux-gnu")).1);
        assert_eq!(nom_executable("prog", true), "prog.exe");
        assert_eq!(nom_executable("prog.exe", true), "prog.exe");
        assert_eq!(nom_executable("prog", false), "prog");
    }

    #[test]
    fn dernier_affichage_devient_valeur_de_retour() {
        let prog = crate::front::analyser("n = 7\nr = n * 6\n>> r\n").unwrap();
        let corps = crate::rir::abaisser_principal_sortie(&prog.instructions).unwrap();
        assert!(matches!(corps.ops.last(), Some(crate::rir::Op::RetourAbstrait(_))));
        assert!(crate::natif_emetteur::peut_emettre(&corps));
        let emis = crate::natif_emetteur::emettre(&corps, crate::natif_emetteur::Architecture::native())
            .unwrap();
        assert!(emis.appels.is_empty() && emis.fallbacks.is_empty());
        let octets = crate::link_natif::elf_bytes(
            &emis.code, &crate::link_natif::Options::depuis_emitted(&emis)).unwrap();
        assert!(octets.starts_with(b"\x7fELF"));
    }

    #[test]
    fn linearisation_aot_rend_les_appels_emettables() {
        let prog = crate::front::analyser(
            "fn f(n: entier) -> entier {\n  si (n < 2) { retour n }\n  retour f(n-1) + f(n-2)\n}\n>> f(4)\n"
        ).unwrap();
        let connus: std::collections::HashSet<String> =
            ["f".to_string()].into_iter().collect();
        let mut lin = Lineariseur::nouveau(&connus);
        Lineariseur::collecter_bloc(&prog.instructions, &mut lin.utilises);
        let principal = lin.bloc(&prog.instructions);
        // ⚠ Le principal linéarisé contient maintenant un `AppelRir` avec un
        // argument VARIABLE (le temporaire), donc émettable sans runtime.
        crate::rir::declarer_rir(&["f".to_string()]);
        let rir = crate::rir::abaisser_principal_sortie(&principal).unwrap();
        assert!(matches!(rir.ops.last(),
            Some(crate::rir::Op::RetourAbstrait(crate::rir::Eabstrait::AppelRir { nom, .. }))
            if nom == "f"));
        assert!(corps_emettable_aot(&rir));
    }

    #[test]
    fn appel_de_fonction_est_refuse_sans_faux_executable() {
        let prog = crate::front::analyser(
            "fn f(n: entier) -> entier { retour n * 6 }\n>> f(7)\n").unwrap();
        let corps = crate::rir::abaisser_principal_sortie(&prog.instructions)
            .or_else(|| crate::rir::abaisser_principal(&prog.instructions)).unwrap();
        assert!(!corps.ops.iter().any(|o| matches!(o, crate::rir::Op::RetourAbstrait(_))));
    }
}
