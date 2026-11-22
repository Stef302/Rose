// ═══════════════════════════════════════════════════════════════════════════
// TRANSPILATION Rose → JavaScript (ES2020) / Python 3 / C99 / WebAssembly
// ═══════════════════════════════════════════════════════════════════════════
//
// ⚠ CE MODULE EST UN PRODUIT DE SORTIE, PAS UNE DÉPENDANCE DU MOTEUR.
// Node.js et python3 ne sont jamais appelés ici : ils servent UNIQUEMENT à
// valider le code émis (voir docs/TRANSPILATION.md et docs/TRANSPILE_WASM.md).
// Le moteur Rose reste autonome, sans GMP externe, sans Python embarqué et
// sans Node embarqué.
//
// ⚠ `js` / `py` / `c` produisent du TEXTE (le marcheur `Emetteur<S: Syntaxe>`
// partagé). `wasm` produit des OCTETS : un module binaire `.wasm` écrit à la
// main (magic, sections, LEB128, opcodes), sans dépendance externe — voir la
// section « CIBLE WEBASSEMBLY » plus bas. Les deux voies partagent l'AST et
// `front::analyser`, mais pas le backend.
//
// ARCHITECTURE COMMUNE
//   `transpiler_source` / `transpiler_fichier` → `front::analyser` (AST) →
//   `Emetteur<S: Syntaxe>` : UN seul marcheur d'AST, zéro duplication du
//   contrôle (si / tant_que / pour / fonctions / affichage). Chaque cible
//   fournit uniquement sa `Syntaxe` (petites briques d'expression + gabarits
//   d'instructions) et son PRÉAMBULE runtime émis en tête du fichier.
//
// SOUS-ENSEMBLE COUVERT (identique pour JS et Python, C plus limité) :
//   • littéraux entiers (BigInt en JS, int en Python, int64_t en C),
//     rationnels (RQ / fractions.Fraction / struct RQ), flottants, booléens,
//     chaînes ;
//   • + - * / (rationnel exact) % (modulo euclidien) div (division plancher)
//     ^ (puissance, exposant négatif → rationnel) ≈, comparaisons, et/ou/non ;
//   • factorielle `!`, multifactorielle `!!`, primorielle `#`, termial `?`,
//     super-factorielle `$`, sous-factorielle `!n` ;
//   • variables, listes, tables (création / indexation / affectation par
//     index), `longueur`, `sous_chaine` ;
//   • `si`/`sinon`, `tant_que`, `pour … de … vers …` (borne finale EXCLUSIVE,
//     exactement comme l'interpréteur), `retour`, `>>` ;
//   • fonctions définies en Rose (récursion comprise) et un catalogue
//     explicite de primitives (voir `runtime_name`).
//
// REFUS EXPLICITES (jamais de silence : `Err` porte la raison) : E/S clavier,
// `spawn`, `essayer`/`attraper`, `lever`, complexes/irrationnels, appels de
// module, primitives hors catalogue, blocs `Instr::Rir` (déjà abaissé).
// Les listes/tables sont refusées en C (voir doc, écarts sémantiques).
//
// Licence MIT.

use std::collections::{HashMap, HashSet};

use crate::front::{Expr, Instr, Programme};

/// Cible de transpilation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cible {
    /// JavaScript ES2020 — Node.js et navigateurs.
    Js,
    /// Python 3 — sortie lisible, sans dépendance autre que `fractions` (stdlib).
    Py,
    /// C99 — noyau numérique (entiers 64 bits), sortie lisible.
    C,
    /// WebAssembly — module BINAIRE `.wasm`, exportant `rose_main() -> i64`.
    /// Sous-ensemble i64 : voir docs/TRANSPILE_WASM.md.
    Wasm,
}

impl Cible {
    pub fn nom(self) -> &'static str {
        match self {
            Cible::Js => "js", Cible::Py => "py", Cible::C => "c",
            Cible::Wasm => "wasm",
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            Cible::Js => "js", Cible::Py => "py", Cible::C => "c",
            Cible::Wasm => "wasm",
        }
    }
    pub fn parser(s: &str) -> Result<Cible, String> {
        match s.to_ascii_lowercase().as_str() {
            "js" | "javascript" | "node" => Ok(Cible::Js),
            "py" | "python" | "python3" => Ok(Cible::Py),
            "c" | "c99" => Ok(Cible::C),
            "wasm" | "wasm32" | "webassembly" => Ok(Cible::Wasm),
            _ => Err(format!(
                "rose: cible de transpilation inconnue '{}' (attendu js|py|c|wasm)", s)),
        }
    }
}

/// Transpile un SOURCE Rose (déjà chargé en mémoire) vers une cible.
pub fn transpiler_source(source: &str, cible: Cible) -> Result<String, String> {
    let prog = crate::front::analyser(source)?;
    transpiler_programme(&prog, cible)
}

/// Transpile un FICHIER Rose et ses `importer` vers une cible.
pub fn transpiler_fichier(chemin: &str, cible: Cible) -> Result<String, String> {
    let prog = crate::front::analyser_fichier(chemin)?;
    transpiler_programme(&prog, cible)
}

/// Transpile un AST Rose déjà analysé vers du TEXTE.
///
/// ⚠ `Cible::Wasm` est BINAIRE : cette fonction refuse cette cible et renvoie
/// une erreur explicite. Utiliser `transpiler_programme_wasm` / `transpiler_fichier_octets`.
pub fn transpiler_programme(prog: &Programme, cible: Cible) -> Result<String, String> {
    match cible {
        Cible::Js => Emetteur::nouveau(Js).programme(prog),
        Cible::Py => Emetteur::nouveau(Py).programme(prog),
        Cible::C => Emetteur::nouveau(C).programme(prog),
        Cible::Wasm => Err(
            "rose: transpile — wasm est un module binaire : utiliser transpiler_programme_wasm ou `rose transpile --cible wasm -o sortie.wasm`".into()),
    }
}

/// Transpile un SOURCE Rose vers un module WebAssembly binaire.
pub fn transpiler_source_wasm(source: &str) -> Result<Vec<u8>, String> {
    let prog = crate::front::analyser(source)?;
    transpiler_programme_wasm(&prog)
}

/// Transpile un FICHIER Rose (et ses `importer`) vers un module WebAssembly binaire.
pub fn transpiler_fichier_wasm(chemin: &str) -> Result<Vec<u8>, String> {
    let prog = crate::front::analyser_fichier(chemin)?;
    transpiler_programme_wasm(&prog)
}

/// Point d'entrée UNIQUE utilisé par la CLI : renvoie toujours des octets.
/// JS / Python / C sont du texte (`String::into_bytes`), WASM est binaire.
pub fn transpiler_fichier_octets(chemin: &str, cible: Cible) -> Result<Vec<u8>, String> {
    match cible {
        Cible::Wasm => transpiler_fichier_wasm(chemin),
        _ => transpiler_fichier(chemin, cible).map(|s| s.into_bytes()),
    }
}

// ── Gabarits communs ───────────────────────────────────────────────────────

fn indent(lignes: &[String]) -> Vec<String> {
    lignes.iter().map(|l| if l.is_empty() { String::new() } else { format!("  {}", l) }).collect()
}

fn corps_vide(lignes: &[String], pass: &str) -> Vec<String> {
    if lignes.is_empty() { vec![pass.to_string()] } else { lignes.to_vec() }
}

/// Primitives du runtime émis dans le préambule. Le NOM est le même dans les
/// trois cibles : c'est le contrat commun entre le marcheur et les backends.
fn runtime_name(nom: &str) -> Option<&'static str> {
    Some(match nom {
        "print" => "rPrint",
        "fact" => "rFact",
        "mfact" => "rMFact",
        "prim" => "rPrim",
        "term" => "rTerm",
        "sfact" => "rSfact",
        "der" => "rDer",
        "fib" => "rFib",
        "gcd" => "rGcd",
        "lcm" => "rLcm",
        "abs" => "rAbs",
        "min" => "rMin",
        "max" => "rMax",
        "len" => "rLen",
        "subch" => "rSubch",
        "get" => "rGet",
        "set" => "rSet",
        "has" => "rHas",
        "del" => "rDel",
        "keys" => "rKeys",
        "values" => "rValues",
        "list" => "rList",
        "table" => "rTable",
        "tablefrom" => "rTableFrom",
        _ => return None,
    })
}

/// Nom Rose d'une primitive de la bibliothèque standard → nom interne.
fn primitive_rose(nom: &str) -> Option<&'static str> {
    Some(match nom {
        "afficher" | "afficher_erreur" => "print",
        "table" | "map" => "table",
        "factorielle" | "fact" => "fact",
        "multifactorielle" | "multifactoriel" => "mfact",
        "primorielle" => "prim",
        "triangulaire" | "termial" | "termial_classique" => "term",
        "super_factorielle" => "sfact",
        "sous_factorielle" | "sous_factorial" | "derangement" => "der",
        "fibonacci" | "fib" => "fib",
        "pgcd" | "gcd" => "gcd",
        "ppcm" | "lcm" => "lcm",
        "abs" => "abs",
        "min" => "min",
        "max" => "max",
        "longueur" | "len" => "len",
        "sous_chaine" | "sub" => "subch",
        "inserer" => "set",
        "cle_existe" => "has",
        "supprimer" => "del",
        "cles" => "keys",
        "valeurs" => "values",
        _ => return None,
    })
}

/// Échappement d'une chaîne Rose vers un littéral `"…"` (JS / Python / C).
fn chaine_echappee(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\x00"),
            c if c.is_control() => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Identifiant sûr pour la cible. Préfixe `v_` : aucun risque de collision
/// avec les primitives du préambule (`r…`) ni avec les mots réservés.
fn ident(n: &str) -> String {
    let mut s = String::from("v_");
    for c in n.chars() {
        if c.is_ascii_alphanumeric() || c == '_' { s.push(c); }
        else { s.push_str(&format!("_u{:x}_", c as u32)); }
    }
    if s.len() == 2 { s.push('_'); }
    s
}

/// Noms affectés / lus dans un bloc : déclarés une seule fois en tête.
fn collecter_noms(instrs: &[Instr], noms: &mut Vec<String>) {
    for i in instrs {
        match i {
            Instr::Affect { nom, .. } | Instr::AffectIndex { nom, .. }
            | Instr::Lire { nom, .. } => {
                if !noms.iter().any(|x| x == nom) { noms.push(nom.clone()); }
            }
            Instr::Pour { nom, corps, .. } => {
                if !noms.iter().any(|x| x == nom) { noms.push(nom.clone()); }
                collecter_noms(corps, noms);
            }
            Instr::Si { alors, sinon, .. } => {
                collecter_noms(alors, noms);
                if let Some(s) = sinon { collecter_noms(s, noms); }
            }
            Instr::TantQue { corps, .. } | Instr::Bloc(corps)
            | Instr::Spawn(corps, _) => collecter_noms(corps, noms),
            Instr::Essayer { corps, attraper, finalment, .. } => {
                collecter_noms(corps, noms);
                if let Some((_, c)) = attraper { collecter_noms(c, noms); }
                if let Some(c) = finalment { collecter_noms(c, noms); }
            }
            _ => {}
        }
    }
}

/// Le contrat que chaque cible implémente. Tout le contrôle (si/boucles/
/// fonction) est écrit UNE fois dans `Emetteur`, indépendamment de la cible.
trait Syntaxe {
    fn prelude(&self) -> String;
    fn commentaire(&self, s: &str) -> String;
    fn ident(&self, n: &str) -> String { ident(n) }
    fn litteral(&self, texte: &str, flott: bool, rationnel: bool) -> Result<String, String>;
    fn chaine(&self, s: &str) -> String { chaine_echappee(s) }
    fn booleen(&self, b: bool) -> String;
    fn binaire(&self, op: &str, a: &str, b: &str) -> Result<String, String>;
    fn unaire(&self, op: &str, a: &str) -> Result<String, String>;
    fn primitive(&self, nom: &str, args: &[String]) -> Result<String, String>;
    fn affect(&self, nom: &str, val: &str) -> String;
    fn declarations(&self, noms: &[String]) -> Vec<String>;
    fn expression_stmt(&self, e: &str) -> String;
    fn afficher(&self, e: &str) -> Vec<String>;
    fn si(&self, test: &str, alors: &[String], sinon: Option<&[String]>) -> Vec<String>;
    fn tant_que(&self, test: &str, corps: &[String]) -> Vec<String>;
    fn pour(&self, nom: &str, debut: &str, fin: &str, corps: &[String], t1: &str, t2: &str) -> Vec<String>;
    fn retour(&self, e: Option<&str>) -> Vec<String>;
    fn fonction(&self, nom: &str, params: &[String], corps: &[String]) -> Vec<String>;
    fn principale(&self, corps: &[String]) -> Vec<String>;
    /// `pass` pour Python (corps vide interdit), chaîne vide ailleurs.
    fn pass(&self) -> Option<&'static str> { None }
    /// Le test d'un `si` / `tant_que` est-il déjà un booléen de la cible ?
    fn test_bool(&self, e: &str) -> String { e.to_string() }
}

struct Emetteur<S: Syntaxe> {
    syn: S,
    tmp: usize,
    fonctions: HashSet<String>,
}

impl<S: Syntaxe> Emetteur<S> {
    fn nouveau(syn: S) -> Self { Emetteur { syn, tmp: 0, fonctions: HashSet::new() } }
    fn tmp(&mut self) -> String { self.tmp += 1; format!("__r{}", self.tmp) }

    fn programme(&mut self, prog: &Programme) -> Result<String, String> {
        self.fonctions = prog.ordre.iter().cloned().collect();
        let mut out = String::new();
        out.push_str(&self.syn.prelude());
        out.push_str(&self.syn.commentaire("Fonctions définies en Rose"));
        out.push('\n');
        for nom in &prog.ordre {
            let sig = prog.signatures.get(nom)
                .ok_or_else(|| format!("rose: transpile — fonction '{}' introuvable", nom))?;
            let corps_rose = sig.corps.as_ref()
                .ok_or_else(|| format!("rose: transpile — fonction externe '{}' non transpilable", nom))?;
            let lignes = self.corps(corps_rose, true)?;
            let params: Vec<String> = sig.params.iter().map(|(n, _)| self.syn.ident(n)).collect();
            for l in self.syn.fonction(&self.syn.ident(nom), &params, &lignes) {
                out.push_str(&l); out.push('\n');
            }
            out.push('\n');
        }
        out.push_str(&self.syn.commentaire("Programme principal"));
        out.push('\n');
        let lignes = self.corps(&prog.instructions, true)?;
        for l in self.syn.principale(&lignes) { out.push_str(&l); out.push('\n'); }
        Ok(out)
    }

    fn corps(&mut self, instrs: &[Instr], declarer: bool) -> Result<Vec<String>, String> {
        let mut out = Vec::new();
        if declarer {
            let mut noms = Vec::new();
            collecter_noms(instrs, &mut noms);
            let ids: Vec<String> = noms.iter().map(|n| self.syn.ident(n)).collect();
            out.extend(self.syn.declarations(&ids));
        }
        for i in instrs { out.extend(self.instr(i)?); }
        Ok(out)
    }

    fn instr(&mut self, i: &Instr) -> Result<Vec<String>, String> {
        match i {
            Instr::Afficher(es, _) => {
                let mut out = Vec::new();
                for e in es {
                    let x = self.expr(e)?;
                    out.extend(self.syn.afficher(&x));
                }
                Ok(out)
            }
            Instr::Affect { nom, e, .. } => {
                let v = self.expr(e)?;
                Ok(vec![self.syn.affect(&self.syn.ident(nom), &v)])
            }
            Instr::AffectIndex { nom, index, valeur, .. } => {
                let c = self.syn.ident(nom);
                let i = self.expr(index)?;
                let v = self.expr(valeur)?;
                let call = self.syn.primitive("set", &[c, i, v])?;
                Ok(vec![self.syn.expression_stmt(&call)])
            }
            Instr::Si { test, alors, sinon, .. } => {
                let x = self.expr(test)?;
                let t = self.syn.test_bool(&x);
                let a = self.corps(alors, false)?;
                let s = match sinon { Some(b) => Some(self.corps(b, false)?), None => None };
                Ok(self.syn.si(&t, &a, s.as_deref()))
            }
            Instr::TantQue { test, corps, .. } => {
                let x = self.expr(test)?;
                let t = self.syn.test_bool(&x);
                let c = self.corps(corps, false)?;
                Ok(self.syn.tant_que(&t, &c))
            }
            Instr::Pour { nom, debut, fin, corps, .. } => {
                let d = self.expr(debut)?;
                let f = self.expr(fin)?;
                let c = self.corps(corps, false)?;
                let t1 = self.tmp();
                let t2 = self.tmp();
                Ok(self.syn.pour(&self.syn.ident(nom), &d, &f, &c, &t1, &t2))
            }
            Instr::Retour(e, _) => {
                let v = match e { Some(x) => Some(self.expr(x)?), None => None };
                Ok(self.syn.retour(v.as_deref()))
            }
            Instr::ExprStmt(e) => {
                let v = self.expr(e)?;
                Ok(vec![self.syn.expression_stmt(&v)])
            }
            Instr::Bloc(b) => self.corps(b, false),
            Instr::Lire { .. } => Err("rose: transpile — lecture clavier (`<<`) non supportée".into()),
            Instr::Spawn(..) => Err("rose: transpile — `spawn` non supporté".into()),
            Instr::Essayer { .. } => Err("rose: transpile — `essayer`/`attraper` non supporté".into()),
            Instr::Lever(..) => Err("rose: transpile — `lever` non supporté".into()),
            Instr::Rir(..) => Err("rose: transpile — corps RIR déjà abaissé non supporté".into()),
        }
    }

    fn expr(&mut self, e: &Expr) -> Result<String, String> {
        match e {
            Expr::Litteral { texte, flott, rationnel, .. } => self.syn.litteral(texte, *flott, *rationnel),
            Expr::ChaineLit(s, _) => Ok(self.syn.chaine(s)),
            Expr::BooleenLit(b, _) => Ok(self.syn.booleen(*b)),
            Expr::Var(n, _) => Ok(self.syn.ident(n)),
            Expr::Binaire { op, a, b, .. } => {
                let x = self.expr(a)?;
                let y = self.expr(b)?;
                self.syn.binaire(op, &x, &y)
            }
            Expr::Unaire { op, a, .. } => {
                let x = self.expr(a)?;
                self.syn.unaire(op, &x)
            }
            Expr::Appel { nom, module, args, ligne } => {
                if !module.is_empty() {
                    return Err(format!("rose: transpile — appel de module '{}' non supporté (ligne {})", module, ligne));
                }
                let mut a = Vec::new();
                for x in args { a.push(self.expr(x)?); }
                if self.fonctions.contains(nom) {
                    let f = self.syn.ident(nom);
                    return Ok(format!("{}({})", f, a.join(", ")));
                }
                let p = primitive_rose(nom).ok_or_else(|| format!(
                    "rose: transpile — primitive '{}' non couverte (ligne {})", nom, ligne))?;
                self.syn.primitive(p, &a)
            }
            Expr::Factorielle(a, _) => {
                let x = self.expr(a)?;
                self.syn.primitive("fact", &[x])
            }
            Expr::MultiFactorielle(a, k, _) => {
                let x = self.expr(a)?;
                let kk = self.syn.litteral(&k.to_string(), false, false)?;
                self.syn.primitive("mfact", &[x, kk])
            }
            Expr::Primorielle(a, _) => {
                let x = self.expr(a)?;
                self.syn.primitive("prim", &[x])
            }
            Expr::Terminal(a, _) => {
                let x = self.expr(a)?;
                self.syn.primitive("term", &[x])
            }
            Expr::SuperFactorielle(a, _) => {
                let x = self.expr(a)?;
                self.syn.primitive("sfact", &[x])
            }
            Expr::SousFactorielle(a, _) => {
                let x = self.expr(a)?;
                self.syn.primitive("der", &[x])
            }
            Expr::Index { a, b, .. } => {
                let x = self.expr(a)?;
                let y = self.expr(b)?;
                self.syn.primitive("get", &[x, y])
            }
            Expr::ListeLit(v, _) => {
                let mut a = Vec::new();
                for x in v { a.push(self.expr(x)?); }
                self.syn.primitive("list", &a)
            }
            Expr::ListeVideTypee(_, _) => self.syn.primitive("list", &[]),
            Expr::TableLit(v, _) => {
                // Rose ne stocke que des CLÉS TEXTE dans un littéral de table.
                // On émet les deux listes puis un appel `rTableFrom` : un
                // littéral de table reste une EXPRESSION, pas une suite
                // d'instructions (piège corrigé le 19/11).
                let mut cles = Vec::new();
                let mut vals = Vec::new();
                for (k, x) in v {
                    cles.push(self.syn.chaine(k));
                    vals.push(self.expr(x)?);
                }
                let ks = self.syn.primitive("list", &cles)?;
                let vs = self.syn.primitive("list", &vals)?;
                self.syn.primitive("tablefrom", &[ks, vs])
            }
            Expr::LitComplexe { .. } => Err("rose: transpile — complexes non supportés".into()),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CIBLE JAVASCRIPT (ES2020)
// ═══════════════════════════════════════════════════════════════════════════

struct Js;

const JS_PRELUDE: &str = r##"// Rose -> JavaScript ES2020 — préambule émis par `rose transpile --cible js`.
// Aucune dépendance. Fonctionne sous Node.js >= 14 et dans tout navigateur moderne.
"use strict";
class RQ {
  constructor(n, d) {
    n = BigInt(n); d = BigInt(d);
    if (d === 0n) throw new Error("division par zéro");
    if (d < 0n) { n = -n; d = -d; }
    let a = n < 0n ? -n : n, b = d;
    while (b !== 0n) { const t = a % b; a = b; b = t; }
    if (a > 1n) { n = n / a; d = d / a; }
    this.n = n; this.d = d;
  }
}
function qOf(x) { return x instanceof RQ ? x : new RQ(BigInt(x), 1n); }
function fOf(x) { return x instanceof RQ ? Number(x.n) / Number(x.d) : Number(x); }
function isF(x) { return typeof x === "number"; }
function rAdd(a, b) {
  if (isF(a) || isF(b)) return fOf(a) + fOf(b);
  if (a instanceof RQ || b instanceof RQ) { const x = qOf(a), y = qOf(b); return new RQ(x.n * y.d + y.n * x.d, x.d * y.d); }
  if (typeof a === "string" && typeof b === "string") return a + b;
  return BigInt(a) + BigInt(b);
}
function rSub(a, b) {
  if (isF(a) || isF(b)) return fOf(a) - fOf(b);
  if (a instanceof RQ || b instanceof RQ) { const x = qOf(a), y = qOf(b); return new RQ(x.n * y.d - y.n * x.d, x.d * y.d); }
  return BigInt(a) - BigInt(b);
}
function rMul(a, b) {
  if (isF(a) || isF(b)) return fOf(a) * fOf(b);
  if (a instanceof RQ || b instanceof RQ) { const x = qOf(a), y = qOf(b); return new RQ(x.n * y.n, x.d * y.d); }
  if (typeof a === "string" && BigInt(b) >= 0n) return a.repeat(Number(BigInt(b)));
  if (typeof b === "string" && BigInt(a) >= 0n) return b.repeat(Number(BigInt(a)));
  return BigInt(a) * BigInt(b);
}
function rDiv(a, b) {
  if (isF(a) || isF(b)) { const y = fOf(b); if (y === 0) throw new Error("division par zéro"); return fOf(a) / y; }
  const y = qOf(b); if (y.n === 0n) throw new Error("division par zéro");
  const x = qOf(a); return new RQ(x.n * y.d, x.d * y.n);
}
function rMod(a, b) {
  if (isF(a) || isF(b) || a instanceof RQ || b instanceof RQ) throw new Error("Le modulo exige deux entiers");
  const x = BigInt(a), y = BigInt(b);
  if (y === 0n) throw new Error("modulo par zéro");
  let r = x % y;
  if (r !== 0n && ((r < 0n) !== (y < 0n))) r += y;
  return r;
}
function rDivEnt(a, b) {
  if (isF(a) || isF(b)) { const y = fOf(b); if (y === 0) throw new Error("division euclidienne par zéro"); return Math.floor(fOf(a) / y); }
  const x = BigInt(a), y = BigInt(b);
  if (y === 0n) throw new Error("division euclidienne par zéro");
  let q = x / y; if (x % y !== 0n && ((x < 0n) !== (y < 0n))) q -= 1n;
  return q;
}
function rPow(a, b) {
  if (isF(a) || isF(b)) return Math.pow(fOf(a), fOf(b));
  const e = BigInt(b);
  if (a instanceof RQ) {
    const q = a, p = e < 0n ? -e : e;
    if (e >= 0n) return new RQ(q.n ** p, q.d ** p);
    if (q.n === 0n) throw new Error("0^exposant négatif");
    return new RQ(q.d ** p, q.n ** p);
  }
  const x = BigInt(a);
  if (e >= 0n) return x ** e;
  if (x === 0n) throw new Error("0^exposant négatif");
  return new RQ(1n, x ** (-e));
}
function rNeg(a) { if (isF(a)) return -a; if (a instanceof RQ) return new RQ(-a.n, a.d); return -BigInt(a); }
function rCmp(a, b) {
  if (isF(a) || isF(b)) { const x = fOf(a), y = fOf(b); return x < y ? -1 : x > y ? 1 : 0; }
  if (a instanceof RQ || b instanceof RQ) { const x = qOf(a), y = qOf(b); const l = x.n * y.d, r = y.n * x.d; return l < r ? -1 : l > r ? 1 : 0; }
  if (typeof a === "string" && typeof b === "string") return a < b ? -1 : a > b ? 1 : 0;
  const x = BigInt(a), y = BigInt(b); return x < y ? -1 : x > y ? 1 : 0;
}
function rLt(a,b){ return rCmp(a,b) < 0; }
function rLe(a,b){ return rCmp(a,b) <= 0; }
function rGt(a,b){ return rCmp(a,b) > 0; }
function rGe(a,b){ return rCmp(a,b) >= 0; }
function rEq(a,b){ return rCmp(a,b) === 0; }
function rNe(a,b){ return rCmp(a,b) !== 0; }
function rApprox(a, b) {
  if (isF(a) || isF(b)) { const x = fOf(a), y = fOf(b), s = Math.max(Math.abs(x), Math.abs(y), 1); return Math.abs(x - y) <= 1e-9 * s; }
  return rEq(a, b);
}
function rRatStr(n, d, per) {
  if (d === 0n) return "0";
  if (!per) return n + "/" + d;
  if (d === 1n) return n.toString();
  const na = n < 0n ? -n : n, da = d < 0n ? -d : d;
  if (na.toString().length + da.toString().length > 500) return n + "/" + d;
  let ent = n / da; if (n % da !== 0n && ((n < 0n) !== (da < 0n))) ent -= 1n;
  let reste = na % da, digits = "", peri = -1;
  const pos = new Map();
  while (reste !== 0n && digits.length < 200) {
    const k = reste.toString();
    if (pos.has(k)) { peri = pos.get(k); break; }
    pos.set(k, digits.length);
    reste *= 10n;
    const ch = reste / da; reste %= da;
    digits += ch.toString();
  }
  let out = ent.toString();
  if (digits.length) {
    out += ".";
    out += peri >= 0 ? digits.slice(0, peri) : digits;
    if (peri >= 0) out += "[" + digits.slice(peri) + "]";
    else if (digits.length >= 200) out += "…";
  }
  return out;
}
function rFloatStr(x) {
  if (Number.isNaN(x)) return "non_nombre";
  if (!isFinite(x)) return x > 0 ? "infini" : "-infini";
  if (x === Math.floor(x) && Math.abs(x) < 1e16) return x.toString();
  return x.toString().replace(/e([+-])(\d)$/, "e$10$2").replace(/e\+(\d)$/, "e+0$1").replace(/e-(\d)$/, "e-0$1");
}
class RTable {
  constructor() { this.items = []; }
  static key(k) {
    const t = (typeof k === "string") ? "s" : (k instanceof RQ ? "q" : (typeof k === "bigint" ? "i" : (typeof k === "boolean" ? "b" : "f")));
    return t + ":" + (k instanceof RQ ? k.n + "/" + k.d : rStr(k, false));
  }
  set(k, v) {
    const key = RTable.key(k);
    for (const p of this.items) if (p[0] === key) { p[1] = v; p[2] = k; return; }
    this.items.push([key, v, k]);
  }
  get(k) { const key = RTable.key(k); for (const p of this.items) if (p[0] === key) return p[1]; throw new Error("clé absente"); }
  has(k) { const key = RTable.key(k); return this.items.some(p => p[0] === key); }
  del(k) { const key = RTable.key(k); const n = this.items.length; this.items = this.items.filter(p => p[0] !== key); return n !== this.items.length; }
  keys() { return this.items.map(p => p[2]); }
  values() { return this.items.map(p => p[1]); }
}
function rStr(v, per) {
  if (v === null || v === undefined) return "";
  if (typeof v === "boolean") return v ? "vrai" : "faux";
  if (isF(v)) return rFloatStr(v);
  if (v instanceof RQ) return rRatStr(v.n, v.d, per);
  if (typeof v === "bigint") return v.toString();
  if (typeof v === "string") return v;
  if (Array.isArray(v)) return "[" + v.map(x => rStr(x, true)).join(", ") + "]";
  if (v instanceof RTable) return "[" + v.items.map(p => rStr(p[2], false) + " => " + rStr(p[1], per)).join(", ") + "]";
  return String(v);
}
function rPrint(v) { console.log(rStr(v, true)); }
function rList(...xs) { return xs; }
function rTable() { return new RTable(); }
function rTableFrom(keys, vals) { const t = new RTable(); for (let i = 0; i < keys.length; i++) t.set(keys[i], vals[i]); return t; }
function rGet(a, b) {
  if (a instanceof RTable) return a.get(b);
  const i = Number(BigInt(b));
  if (i < 0) throw new Error("index négatif");
  return a[i];
}
function rSet(a, b, v) {
  if (a instanceof RTable) { a.set(b, v); return; }
  const i = Number(BigInt(b));
  if (i < 0) throw new Error("index négatif");
  a[i] = v;
}
function rHas(a, b) { return a instanceof RTable && a.has(b); }
function rDel(a, b) { if (a instanceof RTable) return a.del(b); return false; }
function rKeys(a) { return a instanceof RTable ? a.keys() : []; }
function rValues(a) { return a instanceof RTable ? a.values() : []; }
function rLen(a) {
  if (a instanceof RTable) return BigInt(a.items.length);
  return BigInt(a.length);
}
function rSubch(s, a, b) { return String(s).slice(Number(BigInt(a)), Number(BigInt(b))); }
function rGcd(x, y) { let a = BigInt(x) < 0n ? -BigInt(x) : BigInt(x), b = BigInt(y) < 0n ? -BigInt(y) : BigInt(y); while (b) { const t = a % b; a = b; b = t; } return a; }
function rLcm(x, y) { const a = BigInt(x), b = BigInt(y); return a === 0n || b === 0n ? 0n : (a / rGcd(a, b)) * b; }
function rAbs(x) { if (isF(x)) return Math.abs(x); if (x instanceof RQ) return new RQ(x.n < 0n ? -x.n : x.n, x.d); const n = BigInt(x); return n < 0n ? -n : n; }
function rMin(a, b) { return rCmp(a, b) <= 0 ? a : b; }
function rMax(a, b) { return rCmp(a, b) >= 0 ? a : b; }
function rFact(x) { let n = BigInt(x), r = 1n; for (let i = 2n; i <= n; i++) r *= i; return r; }
function rMFact(x, k) { let n = BigInt(x), p = BigInt(k), r = 1n; if (p < 1n) throw new Error("multifactorielle d'ordre < 1"); while (n > 0n) { r *= n; n -= p; } return r; }
function rPrim(x) { let n = BigInt(x), r = 1n; for (let i = 2n; i <= n; i++) if (n % i === 0n) r *= i; return r; }
function rTerm(x) { const n = BigInt(x); return n * (n + 1n) / 2n; }
function rSfact(x) { const n = BigInt(x); let r = 1n, f = 1n; for (let i = 1n; i <= n; i++) { f *= i; r *= f; } return r; }
function rDer(x) {
  const n = BigInt(x); if (n === 0n) return 1n; if (n === 1n) return 0n;
  let a = 1n, b = 0n;
  for (let i = 2n; i <= n; i++) { const c = (i - 1n) * (a + b); a = b; b = c; }
  return b;
}
function rFib(x) { const n = BigInt(x); if (n < 2n) return n; let a = 0n, b = 1n; for (let i = 2n; i <= n; i++) { const c = a + b; a = b; b = c; } return b; }
"##;

impl Syntaxe for Js {
    fn prelude(&self) -> String { JS_PRELUDE.to_string() }
    fn commentaire(&self, s: &str) -> String { format!("// {}", s) }
    fn litteral(&self, texte: &str, flott: bool, rationnel: bool) -> Result<String, String> {
        if flott { return Ok(texte.to_string()); }
        if rationnel || texte.contains('/') {
            let (n, d) = texte.split_once('/').unwrap_or((texte, "1"));
            return Ok(format!("new RQ({}n, {}n)", if n.is_empty() { "0" } else { n }, d));
        }
        Ok(format!("{}n", texte))
    }
    fn booleen(&self, b: bool) -> String { if b { "true".into() } else { "false".into() } }
    fn binaire(&self, op: &str, a: &str, b: &str) -> Result<String, String> {
        let f = match op {
            "+" => "rAdd", "-" => "rSub", "*" => "rMul", "/" => "rDiv", "%" => "rMod",
            "div" => "rDivEnt", "^" => "rPow", "<" => "rLt", "<=" => "rLe", ">" => "rGt",
            ">=" => "rGe", "==" => "rEq", "!=" => "rNe", "≈" => "rApprox",
            "et" => return Ok(format!("({} && {})", a, b)),
            "ou" => return Ok(format!("({} || {})", a, b)),
            _ => return Err(format!("rose: transpile js — opérateur '{}' non couvert", op)),
        };
        Ok(format!("{}({}, {})", f, a, b))
    }
    fn unaire(&self, op: &str, a: &str) -> Result<String, String> {
        Ok(match op {
            "-" => format!("rNeg({})", a),
            "non" => format!("(!{})", a),
            _ => return Err(format!("rose: transpile js — opérateur unaire '{}' non couvert", op)),
        })
    }
    fn primitive(&self, nom: &str, args: &[String]) -> Result<String, String> {
        let f = runtime_name(nom).ok_or_else(|| format!("rose: transpile js — primitive '{}' non couverte", nom))?;
        Ok(format!("{}({})", f, args.join(", ")))
    }
    fn affect(&self, n: &str, v: &str) -> String { format!("{} = {};", n, v) }
    fn declarations(&self, noms: &[String]) -> Vec<String> {
        if noms.is_empty() { vec![] } else { vec![format!("let {};", noms.join(", "))] }
    }
    fn expression_stmt(&self, e: &str) -> String { format!("{};", e) }
    fn afficher(&self, e: &str) -> Vec<String> { vec![format!("rPrint({});", e)] }
    fn si(&self, t: &str, a: &[String], s: Option<&[String]>) -> Vec<String> {
        let mut v = vec![format!("if ({}) {{", t)];
        v.extend(indent(a));
        if let Some(s) = s { v.push("} else {".into()); v.extend(indent(s)); }
        v.push("}".into()); v
    }
    fn tant_que(&self, t: &str, c: &[String]) -> Vec<String> {
        let mut v = vec![format!("while ({}) {{", t)];
        v.extend(indent(c)); v.push("}".into()); v
    }
    fn pour(&self, n: &str, d: &str, f: &str, c: &[String], t1: &str, t2: &str) -> Vec<String> {
        let mut v = vec![format!("let {} = {}, {} = {};", t1, d, t2, f),
                         format!("for ({} = {}; rLt({}, {}); {} = rAdd({}, 1n)) {{", n, t1, n, t2, n, n)];
        v.extend(indent(c)); v.push("}".into()); v
    }
    fn retour(&self, e: Option<&str>) -> Vec<String> {
        vec![match e { Some(x) => format!("return {};", x), None => "return;".into() }]
    }
    fn fonction(&self, n: &str, p: &[String], c: &[String]) -> Vec<String> {
        let mut v = vec![format!("function {}({}) {{", n, p.join(", "))];
        v.extend(indent(c)); v.push("}".into()); v
    }
    fn principale(&self, c: &[String]) -> Vec<String> { c.to_vec() }
}

// ═══════════════════════════════════════════════════════════════════════════
// CIBLE PYTHON 3
// ═══════════════════════════════════════════════════════════════════════════

struct Py;

const PY_PRELUDE: &str = r##"# Rose -> Python 3 — préambule émis par `rose transpile --cible py`.
# Aucune dépendance tierce : seule la bibliothèque standard `fractions` est utilisée.
from fractions import Fraction


class RTable:
    """Table Rose : paires (clé, valeur) ordonnées."""
    def __init__(self):
        self.items = []

    @staticmethod
    def key(k):
        if isinstance(k, bool):
            t = "b"
        elif isinstance(k, int):
            t = "i"
        elif isinstance(k, Fraction):
            t = "q"
        elif isinstance(k, str):
            t = "s"
        else:
            t = "f"
        return t + ":" + (str(k.numerator) + "/" + str(k.denominator) if isinstance(k, Fraction) else rStr(k, False))

    def set(self, k, v):
        key = RTable.key(k)
        for p in self.items:
            if p[0] == key:
                p[1], p[2] = v, k
                return
        self.items.append([key, v, k])

    def get(self, k):
        key = RTable.key(k)
        for p in self.items:
            if p[0] == key:
                return p[1]
        raise KeyError("clé absente")

    def has(self, k):
        key = RTable.key(k)
        return any(p[0] == key for p in self.items)

    def delete(self, k):
        key = RTable.key(k)
        n = len(self.items)
        self.items = [p for p in self.items if p[0] != key]
        return len(self.items) != n


def rAdd(a, b):
    if isinstance(a, float) or isinstance(b, float):
        return float(a) + float(b)
    if isinstance(a, Fraction) or isinstance(b, Fraction):
        return Fraction(a) + Fraction(b)
    if isinstance(a, str) and isinstance(b, str):
        return a + b
    return int(a) + int(b)


def rSub(a, b):
    if isinstance(a, float) or isinstance(b, float):
        return float(a) - float(b)
    if isinstance(a, Fraction) or isinstance(b, Fraction):
        return Fraction(a) - Fraction(b)
    return int(a) - int(b)


def rMul(a, b):
    if isinstance(a, float) or isinstance(b, float):
        return float(a) * float(b)
    if isinstance(a, Fraction) or isinstance(b, Fraction):
        return Fraction(a) * Fraction(b)
    if isinstance(a, str) and isinstance(b, int) and b >= 0:
        return a * b
    if isinstance(b, str) and isinstance(a, int) and a >= 0:
        return b * a
    return int(a) * int(b)


def rDiv(a, b):
    if isinstance(a, float) or isinstance(b, float):
        y = float(b)
        if y == 0.0:
            raise ZeroDivisionError("division par zéro")
        return float(a) / y
    y = Fraction(b)
    if y == 0:
        raise ZeroDivisionError("division par zéro")
    return Fraction(a) / y


def rMod(a, b):
    # Rose: le modulo exige deux entiers ; Python: le reste suit le signe du
    # DIVISEUR, comme gmp_fdiv_r.
    if isinstance(a, (float, Fraction)) or isinstance(b, (float, Fraction)):
        raise ValueError("Le modulo exige deux entiers")
    return int(a) % int(b)


def rDivEnt(a, b):
    if isinstance(a, Fraction) or isinstance(b, Fraction):
        y = Fraction(b)
        if y == 0:
            raise ZeroDivisionError("division euclidienne par zéro")
        return Fraction(a) // y
    if isinstance(a, float) or isinstance(b, float):
        y = float(b)
        if y == 0.0:
            raise ZeroDivisionError("division euclidienne par zéro")
        return float(__import__("math").floor(float(a) / y))
    return int(a) // int(b)


def rPow(a, b):
    if isinstance(a, float) or isinstance(b, float):
        return float(a) ** float(b)
    e = int(b)
    if isinstance(a, Fraction):
        return Fraction(a) ** e
    x = int(a)
    if e >= 0:
        return x ** e
    if x == 0:
        raise ZeroDivisionError("0^exposant négatif")
    return Fraction(1, x ** (-e))


def rNeg(a):
    if isinstance(a, bool):
        return -int(a)
    return -a


def rCmp(a, b):
    if isinstance(a, float) or isinstance(b, float) or isinstance(a, Fraction) or isinstance(b, Fraction):
        x, y = float(a), float(b)
    elif isinstance(a, str) and isinstance(b, str):
        x, y = a, b
    else:
        x, y = int(a), int(b)
    return -1 if x < y else (1 if x > y else 0)


def rLt(a, b): return rCmp(a, b) < 0
def rLe(a, b): return rCmp(a, b) <= 0
def rGt(a, b): return rCmp(a, b) > 0
def rGe(a, b): return rCmp(a, b) >= 0
def rEq(a, b): return rCmp(a, b) == 0
def rNe(a, b): return rCmp(a, b) != 0


def rApprox(a, b):
    if isinstance(a, float) or isinstance(b, float):
        x, y = float(a), float(b)
        s = max(abs(x), abs(y), 1.0)
        return abs(x - y) <= 1e-9 * s
    return rEq(a, b)


def rRatStr(n, d, per):
    if d == 0:
        return "0"
    if not per:
        return str(n) + "/" + str(d)
    if d == 1:
        return str(n)
    na, da = abs(n), abs(d)
    if len(str(na)) + len(str(da)) > 500:
        return str(n) + "/" + str(d)
    # Python: // et % sont DEJA euclidiens (plancher) — aucun ajustement.
    ent = n // da
    reste, digits, peri, pos = na % da, "", -1, {}
    while reste != 0 and len(digits) < 200:
        if reste in pos:
            peri = pos[reste]
            break
        pos[reste] = len(digits)
        reste *= 10
        ch, reste = divmod(reste, da)
        digits += str(ch)
    out = str(ent)
    if digits:
        out += "."
        out += digits[:peri] if peri >= 0 else digits
        if peri >= 0:
            out += "[" + digits[peri:] + "]"
        elif len(digits) >= 200:
            out += "…"
    return out


def rFloatStr(x):
    if x != x:
        return "non_nombre"
    if x in (float("inf"), float("-inf")):
        return "infini" if x > 0 else "-infini"
    if x == int(x) and abs(x) < 1e16:
        return str(int(x))
    return repr(x)


def rStr(v, per=True):
    if v is None:
        return ""
    if isinstance(v, bool):
        return "vrai" if v else "faux"
    if isinstance(v, float):
        return rFloatStr(v)
    if isinstance(v, Fraction):
        return rRatStr(v.numerator, v.denominator, per)
    if isinstance(v, int):
        return str(v)
    if isinstance(v, str):
        return v
    if isinstance(v, list):
        return "[" + ", ".join(rStr(x, True) for x in v) + "]"
    if isinstance(v, RTable):
        return "[" + ", ".join(rStr(p[2], False) + " => " + rStr(p[1], per) for p in v.items) + "]"
    return str(v)


def rPrint(v):
    print(rStr(v, True))


def rList(*xs):
    return list(xs)


def rTable():
    return RTable()


def rTableFrom(keys, vals):
    t = RTable()
    for k, v in zip(keys, vals):
        t.set(k, v)
    return t


def rGet(a, b):
    if isinstance(a, RTable):
        return a.get(b)
    i = int(b)
    if i < 0:
        raise IndexError("index négatif")
    return a[i]


def rSet(a, b, v):
    if isinstance(a, RTable):
        a.set(b, v)
        return
    i = int(b)
    if i < 0:
        raise IndexError("index négatif")
    a[i] = v


def rHas(a, b):
    return a.has(b) if isinstance(a, RTable) else False


def rDel(a, b):
    return a.delete(b) if isinstance(a, RTable) else False


def rKeys(a):
    return a.keys() if isinstance(a, RTable) else []


def rValues(a):
    return a.values() if isinstance(a, RTable) else []


def rLen(a):
    return len(a.items) if isinstance(a, RTable) else len(a)


def rSubch(s, a, b):
    return str(s)[int(a):int(b)]


def rGcd(x, y):
    a, b = abs(int(x)), abs(int(y))
    while b:
        a, b = b, a % b
    return a


def rLcm(x, y):
    a, b = int(x), int(y)
    return 0 if a == 0 or b == 0 else (a // rGcd(a, b)) * b


def rAbs(x):
    return abs(x)


def rMin(a, b):
    return a if rCmp(a, b) <= 0 else b


def rMax(a, b):
    return a if rCmp(a, b) >= 0 else b


def rFact(x):
    n, r = int(x), 1
    for i in range(2, n + 1):
        r *= i
    return r


def rMFact(x, k):
    n, p, r = int(x), int(k), 1
    if p < 1:
        raise ValueError("multifactorielle d'ordre < 1")
    while n > 0:
        r *= n
        n -= p
    return r


def rPrim(x):
    n, r = int(x), 1
    for i in range(2, n + 1):
        if n % i == 0:
            r *= i
    return r


def rTerm(x):
    n = int(x)
    return n * (n + 1) // 2


def rSfact(x):
    n, r, f = int(x), 1, 1
    for i in range(1, n + 1):
        f *= i
        r *= f
    return r


def rDer(x):
    n = int(x)
    if n == 0:
        return 1
    if n == 1:
        return 0
    a, b = 1, 0
    for i in range(2, n + 1):
        a, b = b, (i - 1) * (a + b)
    return b


def rFib(x):
    n = int(x)
    if n < 2:
        return n
    a, b = 0, 1
    for _ in range(2, n + 1):
        a, b = b, a + b
    return b


"##;

impl Syntaxe for Py {
    fn prelude(&self) -> String { PY_PRELUDE.to_string() }
    fn commentaire(&self, s: &str) -> String { format!("# {}", s) }
    fn pass(&self) -> Option<&'static str> { Some("pass") }
    fn litteral(&self, texte: &str, flott: bool, rationnel: bool) -> Result<String, String> {
        if flott { return Ok(texte.to_string()); }
        if rationnel || texte.contains('/') {
            let (n, d) = texte.split_once('/').unwrap_or((texte, "1"));
            return Ok(format!("Fraction({}, {})", if n.is_empty() { "0" } else { n }, d));
        }
        Ok(texte.to_string())
    }
    fn booleen(&self, b: bool) -> String { if b { "True".into() } else { "False".into() } }
    fn binaire(&self, op: &str, a: &str, b: &str) -> Result<String, String> {
        let f = match op {
            "+" => "rAdd", "-" => "rSub", "*" => "rMul", "/" => "rDiv", "%" => "rMod",
            "div" => "rDivEnt", "^" => "rPow", "<" => "rLt", "<=" => "rLe", ">" => "rGt",
            ">=" => "rGe", "==" => "rEq", "!=" => "rNe", "≈" => "rApprox",
            "et" => return Ok(format!("({} and {})", a, b)),
            "ou" => return Ok(format!("({} or {})", a, b)),
            _ => return Err(format!("rose: transpile py — opérateur '{}' non couvert", op)),
        };
        Ok(format!("{}({}, {})", f, a, b))
    }
    fn unaire(&self, op: &str, a: &str) -> Result<String, String> {
        Ok(match op {
            "-" => format!("rNeg({})", a),
            "non" => format!("(not {})", a),
            _ => return Err(format!("rose: transpile py — opérateur unaire '{}' non couvert", op)),
        })
    }
    fn primitive(&self, nom: &str, args: &[String]) -> Result<String, String> {
        let f = runtime_name(nom).ok_or_else(|| format!("rose: transpile py — primitive '{}' non couverte", nom))?;
        let f = if f == "rDel" { "rDel" } else { f };
        Ok(format!("{}({})", f, args.join(", ")))
    }
    fn affect(&self, n: &str, v: &str) -> String { format!("{} = {}", n, v) }
    fn declarations(&self, _noms: &[String]) -> Vec<String> { vec![] }
    fn expression_stmt(&self, e: &str) -> String { e.to_string() }
    fn afficher(&self, e: &str) -> Vec<String> { vec![format!("rPrint({})", e)] }
    fn si(&self, t: &str, a: &[String], s: Option<&[String]>) -> Vec<String> {
        let mut v = vec![format!("if {}:", t)];
        v.extend(indent(&corps_vide(a, "pass")));
        if let Some(s) = s { v.push("else:".into()); v.extend(indent(&corps_vide(s, "pass"))); }
        v
    }
    fn tant_que(&self, t: &str, c: &[String]) -> Vec<String> {
        let mut v = vec![format!("while {}:", t)];
        v.extend(indent(&corps_vide(c, "pass"))); v
    }
    fn pour(&self, n: &str, d: &str, f: &str, c: &[String], t1: &str, t2: &str) -> Vec<String> {
        let mut v = vec![format!("{} = {}", t1, d), format!("{} = {}", t2, f),
                         format!("{} = {}", n, t1), format!("while rLt({}, {}):", n, t2)];
        let mut corps = corps_vide(c, "pass");
        corps.push(format!("{} = rAdd({}, 1)", n, n));
        v.extend(indent(&corps)); v
    }
    fn retour(&self, e: Option<&str>) -> Vec<String> {
        vec![match e { Some(x) => format!("return {}", x), None => "return".into() }]
    }
    fn fonction(&self, n: &str, p: &[String], c: &[String]) -> Vec<String> {
        let mut v = vec![format!("def {}({}):", n, p.join(", "))];
        v.extend(indent(&corps_vide(c, "pass"))); v
    }
    fn principale(&self, c: &[String]) -> Vec<String> { c.to_vec() }
}

// ═══════════════════════════════════════════════════════════════════════════
// CIBLE C99 — noyau numérique (sous-ensemble volontairement plus étroit)
// ═══════════════════════════════════════════════════════════════════════════

struct C;

const C_PRELUDE: &str = r##"/* Rose -> C99 — préambule émis par `rose transpile --cible c`.
   Valeurs taguées RV : entiers int64, rationnels exacts, flottants, booléens,
   chaînes. Aucune dépendance externe (pas de GMP). */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <math.h>

typedef struct { int64_t n, d; } RQ;
typedef enum { RV_VIDE, RV_INT, RV_RAT, RV_FLT, RV_BOOL, RV_STR } RType;
typedef struct { RType t; int64_t i; RQ q; double f; int b; char *s; } RV;

static RV rvVoid(void) { RV r; memset(&r, 0, sizeof r); r.t = RV_VIDE; return r; }
static RV rvInt(int64_t x) { RV r; memset(&r, 0, sizeof r); r.t = RV_INT; r.i = x; return r; }
static RV rvFlt(double x) { RV r; memset(&r, 0, sizeof r); r.t = RV_FLT; r.f = x; return r; }
static RV rvBool(int x) { RV r; memset(&r, 0, sizeof r); r.t = RV_BOOL; r.b = x ? 1 : 0; return r; }
static char *rDup(const char *s) { size_t n = strlen(s) + 1; char *p = malloc(n); memcpy(p, s, n); return p; }
static RV rvStr(const char *s) { RV r; memset(&r, 0, sizeof r); r.t = RV_STR; r.s = rDup(s); return r; }
static int64_t rgcd(int64_t a, int64_t b) { if (a < 0) a = -a; if (b < 0) b = -b; while (b) { int64_t t = a % b; a = b; b = t; } return a; }
static RV rvRat(int64_t n, int64_t d) {
  RV r; memset(&r, 0, sizeof r);
  if (d == 0) { fputs("division par zero\n", stderr); exit(1); }
  if (d < 0) { n = -n; d = -d; }
  int64_t g = rgcd(n, d); if (g > 1) { n /= g; d /= g; }
  r.t = RV_RAT; r.q.n = n; r.q.d = d; return r;
}
static int rvTruth(RV a) {
  switch (a.t) {
    case RV_VIDE: return 0;
    case RV_INT: return a.i != 0;
    case RV_RAT: return a.q.n != 0;
    case RV_FLT: return a.f != 0.0;
    case RV_BOOL: return a.b;
    case RV_STR: return a.s && a.s[0];
  }
  return 0;
}
static double rvNum(RV a) {
  switch (a.t) {
    case RV_INT: return (double)a.i;
    case RV_RAT: return (double)a.q.n / (double)a.q.d;
    case RV_FLT: return a.f;
    case RV_BOOL: return (double)a.b;
    default: return 0.0;
  }
}
static int64_t rvIntOf(RV a) { return a.t == RV_RAT && a.q.d == 1 ? a.q.n : a.i; }
static RQ rvQ(RV a) { RQ q; q.n = (a.t == RV_RAT) ? a.q.n : rvIntOf(a); q.d = (a.t == RV_RAT) ? a.q.d : 1; return q; }
static int rCmp(RV a, RV b) {
  if (a.t == RV_FLT || b.t == RV_FLT || a.t == RV_RAT || b.t == RV_RAT) {
    double x = rvNum(a), y = rvNum(b); return x < y ? -1 : x > y ? 1 : 0;
  }
  if (a.t == RV_STR && b.t == RV_STR) { int c = strcmp(a.s, b.s); return c < 0 ? -1 : c > 0 ? 1 : 0; }
  int64_t x = rvIntOf(a), y = rvIntOf(b); return x < y ? -1 : x > y ? 1 : 0;
}
static RV rAdd(RV a, RV b) {
  if (a.t == RV_FLT || b.t == RV_FLT) return rvFlt(rvNum(a) + rvNum(b));
  if (a.t == RV_RAT || b.t == RV_RAT) { RQ x = rvQ(a), y = rvQ(b); return rvRat(x.n * y.d + y.n * x.d, x.d * y.d); }
  if (a.t == RV_STR && b.t == RV_STR) { size_t n = strlen(a.s) + strlen(b.s) + 1; char *s = malloc(n); snprintf(s, n, "%s%s", a.s, b.s); return rvStr(s); }
  return rvInt(rvIntOf(a) + rvIntOf(b));
}
static RV rSub(RV a, RV b) {
  if (a.t == RV_FLT || b.t == RV_FLT) return rvFlt(rvNum(a) - rvNum(b));
  if (a.t == RV_RAT || b.t == RV_RAT) { RQ x = rvQ(a), y = rvQ(b); return rvRat(x.n * y.d - y.n * x.d, x.d * y.d); }
  return rvInt(rvIntOf(a) - rvIntOf(b));
}
static RV rMul(RV a, RV b) {
  if (a.t == RV_FLT || b.t == RV_FLT) return rvFlt(rvNum(a) * rvNum(b));
  if (a.t == RV_RAT || b.t == RV_RAT) { RQ x = rvQ(a), y = rvQ(b); return rvRat(x.n * y.n, x.d * y.d); }
  return rvInt(rvIntOf(a) * rvIntOf(b));
}
static RV rDiv(RV a, RV b) {
  if (a.t == RV_FLT || b.t == RV_FLT) { double y = rvNum(b); if (y == 0) { fputs("division par zero\n", stderr); exit(1); } return rvFlt(rvNum(a) / y); }
  RQ y = rvQ(b); if (y.n == 0) { fputs("division par zero\n", stderr); exit(1); }
  RQ x = rvQ(a); return rvRat(x.n * y.d, x.d * y.n);
}
static RV rMod(RV a, RV b) {
  if (a.t == RV_FLT || b.t == RV_FLT || a.t == RV_RAT || b.t == RV_RAT) { fputs("Le modulo exige deux entiers\n", stderr); exit(1); }
  int64_t x = rvIntOf(a), y = rvIntOf(b);
  if (!y) { fputs("modulo par zero\n", stderr); exit(1); }
  int64_t r = x % y; if (r && ((r < 0) != (y < 0))) r += y; return rvInt(r);
}
static RV rDivEnt(RV a, RV b) {
  if (a.t == RV_RAT || b.t == RV_RAT) {
    RQ x = rvQ(a), y = rvQ(b);
    if (y.n == 0) { fputs("division par zero\n", stderr); exit(1); }
    int64_t num = x.n * y.d, den = x.d * y.n; if (den < 0) { num = -num; den = -den; }
    int64_t q = num / den; if (num % den && ((num < 0) != (den < 0))) q -= 1;
    return rvInt(q);
  }
  if (a.t == RV_FLT || b.t == RV_FLT) { double y = rvNum(b); if (y == 0) { fputs("division par zero\n", stderr); exit(1); } return rvFlt(floor(rvNum(a) / y)); }
  int64_t x = rvIntOf(a), y = rvIntOf(b);
  if (!y) { fputs("division par zero\n", stderr); exit(1); }
  int64_t q = x / y; if (x % y && ((x < 0) != (y < 0))) q -= 1; return rvInt(q);
}
static RV rPow(RV a, RV b) {
  if (a.t == RV_FLT || b.t == RV_FLT) return rvFlt(pow(rvNum(a), rvNum(b)));
  int64_t e = rvIntOf(b);
  if (a.t == RV_RAT) {
    int64_t p = e < 0 ? -e : e;
    int64_t num = 1, den = 1;
    for (int64_t i = 0; i < p; i++) { num *= a.q.n; den *= a.q.d; }
    if (e >= 0) return rvRat(num, den);
    if (a.q.n == 0) { fputs("0^exposant negatif\n", stderr); exit(1); }
    return rvRat(den, num);
  }
  int64_t x = rvIntOf(a);
  if (e >= 0) { int64_t r = 1; for (int64_t i = 0; i < e; i++) r *= x; return rvInt(r); }
  if (x == 0) { fputs("0^exposant negatif\n", stderr); exit(1); }
  int64_t p = 1; for (int64_t i = 0; i < -e; i++) p *= x; return rvRat(1, p);
}
static RV rNeg(RV a) {
  if (a.t == RV_FLT) return rvFlt(-a.f);
  if (a.t == RV_RAT) return rvRat(-a.q.n, a.q.d);
  return rvInt(-rvIntOf(a));
}
static RV rLt(RV a, RV b) { return rvBool(rCmp(a, b) < 0); }
static RV rLe(RV a, RV b) { return rvBool(rCmp(a, b) <= 0); }
static RV rGt(RV a, RV b) { return rvBool(rCmp(a, b) > 0); }
static RV rGe(RV a, RV b) { return rvBool(rCmp(a, b) >= 0); }
static RV rEq(RV a, RV b) { return rvBool(rCmp(a, b) == 0); }
static RV rNe(RV a, RV b) { return rvBool(rCmp(a, b) != 0); }
static RV rApprox(RV a, RV b) {
  if (a.t == RV_FLT || b.t == RV_FLT) {
    double x = rvNum(a), y = rvNum(b), s = fmax(fabs(x), fmax(fabs(y), 1.0));
    return rvBool(fabs(x - y) <= 1e-9 * s);
  }
  return rEq(a, b);
}
static void rRatStr(RQ q, char *out, size_t cap) {
  if (q.d == 1) { snprintf(out, cap, "%lld", (long long)q.n); return; }
  int64_t na = q.n < 0 ? -q.n : q.n, da = q.d < 0 ? -q.d : q.d;
  int64_t ent = q.n / da;
  if (q.n % da && ((q.n < 0) != (da < 0))) ent -= 1;
  int64_t reste = na % da;
  char digits[201]; int nd = 0, peri = -1; int64_t vus[200]; int pos[200];
  while (reste && nd < 200) {
    int j = 0; while (j < nd && vus[j] != reste) j++;
    if (j < nd) { peri = pos[j]; break; }
    vus[nd] = reste; pos[nd] = nd;
    reste *= 10; int64_t ch = reste / da; reste %= da;
    digits[nd++] = (char)('0' + ch);
  }
  size_t k = (size_t)snprintf(out, cap, "%lld", (long long)ent);
  if (nd && k + (size_t)nd + 4 < cap) {
    out[k++] = '.';
    int fin = peri >= 0 ? peri : nd;
    for (int j = 0; j < fin; j++) out[k++] = digits[j];
    if (peri >= 0) { out[k++] = '['; for (int j = peri; j < nd; j++) out[k++] = digits[j]; out[k++] = ']'; }
    out[k] = 0;
  }
}
static void rStr(RV v, int per, char *out, size_t cap) {
  switch (v.t) {
    case RV_VIDE: out[0] = 0; break;
    case RV_INT: snprintf(out, cap, "%lld", (long long)v.i); break;
    case RV_RAT: if (per) rRatStr(v.q, out, cap); else snprintf(out, cap, "%lld/%lld", (long long)v.q.n, (long long)v.q.d); break;
    case RV_FLT: if (v.f == floor(v.f) && fabs(v.f) < 1e16) snprintf(out, cap, "%.0f", v.f); else snprintf(out, cap, "%.17g", v.f); break;
    case RV_BOOL: snprintf(out, cap, "%s", v.b ? "vrai" : "faux"); break;
    case RV_STR: snprintf(out, cap, "%s", v.s ? v.s : ""); break;
  }
}
static void rPrint(RV v) { char buf[4096]; rStr(v, 1, buf, sizeof buf); puts(buf); }
static RV rAbs(RV a) { if (a.t == RV_FLT) return rvFlt(fabs(a.f)); if (a.t == RV_RAT) return rvRat(a.q.n < 0 ? -a.q.n : a.q.n, a.q.d); int64_t x = rvIntOf(a); return rvInt(x < 0 ? -x : x); }
static RV rMin(RV a, RV b) { return rCmp(a, b) <= 0 ? a : b; }
static RV rMax(RV a, RV b) { return rCmp(a, b) >= 0 ? a : b; }
static RV rGcd(RV a, RV b) { return rvInt(rgcd(rvIntOf(a), rvIntOf(b))); }
static RV rLcm(RV a, RV b) { int64_t x = rvIntOf(a), y = rvIntOf(b); return rvInt(x == 0 || y == 0 ? 0 : (x / rgcd(x, y)) * y); }
static RV rFact(RV a) { int64_t n = rvIntOf(a), r = 1; for (int64_t i = 2; i <= n; i++) r *= i; return rvInt(r); }
static RV rMFact(RV a, RV k) { int64_t n = rvIntOf(a), p = rvIntOf(k), r = 1; if (p < 1) { fputs("ordre < 1\n", stderr); exit(1); } while (n > 0) { r *= n; n -= p; } return rvInt(r); }
static RV rPrim(RV a) { int64_t n = rvIntOf(a), r = 1; for (int64_t i = 2; i <= n; i++) if (n % i == 0) r *= i; return rvInt(r); }
static RV rTerm(RV a) { int64_t n = rvIntOf(a); return rvInt(n * (n + 1) / 2); }
static RV rSfact(RV a) { int64_t n = rvIntOf(a), r = 1, f = 1; for (int64_t i = 1; i <= n; i++) { f *= i; r *= f; } return rvInt(r); }
static RV rDer(RV a) { int64_t n = rvIntOf(a); if (n == 0) return rvInt(1); if (n == 1) return rvInt(0); int64_t x = 1, y = 0; for (int64_t i = 2; i <= n; i++) { int64_t z = (i - 1) * (x + y); x = y; y = z; } return rvInt(y); }
static RV rFib(RV a) { int64_t n = rvIntOf(a); if (n < 2) return rvInt(n); int64_t x = 0, y = 1; for (int64_t i = 2; i <= n; i++) { int64_t z = x + y; x = y; y = z; } return rvInt(y); }
static RV rLen(RV a) { (void)a; return rvInt(0); }
static RV rSubch(RV a, RV b, RV e) { (void)a; (void)b; (void)e; return rvStr(""); }
"##;

impl Syntaxe for C {
    fn prelude(&self) -> String { C_PRELUDE.to_string() }
    fn commentaire(&self, s: &str) -> String { format!("/* {} */", s) }
    fn chaine(&self, s: &str) -> String { format!("rvStr({})", chaine_echappee(s)) }
    fn litteral(&self, texte: &str, flott: bool, rationnel: bool) -> Result<String, String> {
        if flott { return Ok(format!("rvFlt({})", texte)); }
        if rationnel || texte.contains('/') {
            return Err("rose: transpile c — littéraux rationnels non supportés (utiliser js ou py)".into());
        }
        Ok(format!("rvInt((int64_t)({}))", texte))
    }
    fn booleen(&self, b: bool) -> String { format!("rvBool({})", if b { 1 } else { 0 }) }
    fn binaire(&self, op: &str, a: &str, b: &str) -> Result<String, String> {
        let f = match op {
            "+" => "rAdd", "-" => "rSub", "*" => "rMul", "/" => "rDiv", "%" => "rMod",
            "div" => "rDivEnt", "^" => "rPow", "<" => "rLt", "<=" => "rLe", ">" => "rGt",
            ">=" => "rGe", "==" => "rEq", "!=" => "rNe", "≈" => "rApprox",
            "et" => return Ok(format!("rvBool(rvTruth({}) && rvTruth({}))", a, b)),
            "ou" => return Ok(format!("rvBool(rvTruth({}) || rvTruth({}))", a, b)),
            _ => return Err(format!("rose: transpile c — opérateur '{}' non couvert", op)),
        };
        Ok(format!("{}({}, {})", f, a, b))
    }
    fn unaire(&self, op: &str, a: &str) -> Result<String, String> {
        Ok(match op {
            "-" => format!("rNeg({})", a),
            "non" => format!("rvBool(!rvTruth({}))", a),
            _ => return Err(format!("rose: transpile c — opérateur unaire '{}' non couvert", op)),
        })
    }
    fn primitive(&self, nom: &str, args: &[String]) -> Result<String, String> {
        if matches!(nom, "list" | "table" | "tablefrom" | "get" | "set" | "has" | "del" | "keys" | "values") {
            return Err(format!("rose: transpile c — listes/tables non supportées (`{}`, utiliser js ou py)", nom));
        }
        let f = runtime_name(nom).ok_or_else(|| format!("rose: transpile c — primitive '{}' non couverte", nom))?;
        Ok(format!("{}({})", f, args.join(", ")))
    }
    fn affect(&self, n: &str, v: &str) -> String { format!("{} = {};", n, v) }
    fn declarations(&self, noms: &[String]) -> Vec<String> {
        noms.iter().map(|n| format!("RV {} = rvVoid();", n)).collect()
    }
    fn test_bool(&self, e: &str) -> String { format!("rvTruth({})", e) }
    fn expression_stmt(&self, e: &str) -> String { format!("{};", e) }
    fn afficher(&self, e: &str) -> Vec<String> { vec![format!("rPrint({});", e)] }
    fn si(&self, t: &str, a: &[String], s: Option<&[String]>) -> Vec<String> {
        let mut v = vec![format!("if ({}) {{", t)];
        v.extend(indent(a));
        if let Some(s) = s { v.push("} else {".into()); v.extend(indent(s)); }
        v.push("}".into()); v
    }
    fn tant_que(&self, t: &str, c: &[String]) -> Vec<String> {
        let mut v = vec![format!("while ({}) {{", t)];
        v.extend(indent(c)); v.push("}".into()); v
    }
    fn pour(&self, n: &str, d: &str, f: &str, c: &[String], t1: &str, t2: &str) -> Vec<String> {
        let mut v = vec![format!("RV {} = {}, {} = {};", t1, d, t2, f),
                         format!("for ({} = {}; rvTruth(rLt({}, {})); {} = rAdd({}, rvInt(1))) {{", n, t1, n, t2, n, n)];
        v.extend(indent(c)); v.push("}".into()); v
    }
    fn retour(&self, e: Option<&str>) -> Vec<String> {
        vec![match e { Some(x) => format!("return {};", x), None => "return rvVoid();".into() }]
    }
    fn fonction(&self, n: &str, p: &[String], c: &[String]) -> Vec<String> {
        let mut v = vec![format!("RV {}({}) {{", n,
            p.iter().map(|x| format!("RV {}", x)).collect::<Vec<_>>().join(", "))];
        v.extend(indent(c));
        v.push("return rvVoid();".into()); v.push("}".into()); v
    }
    fn principale(&self, c: &[String]) -> Vec<String> {
        let mut v = vec!["int main(void) {".to_string()];
        v.extend(indent(c));
        v.push("return 0;".into()); v.push("}".into()); v
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CIBLE WEBASSEMBLY — émetteur binaire, ZÉRO dépendance externe
// ═══════════════════════════════════════════════════════════════════════════
//
// Écrit exactement comme `natif_emetteur` écrit ELF/PE : octets, LEB128,
// sections et opcodes à la main. Aucune crate wasm, aucun `.wat`, aucun
// runtime : le module produit est directement chargeable par
// `WebAssembly.Module` (Node) ou wasmtime.
//
// ABI : `rose_main() -> i64`, exportée (index 0). Les fonctions Rose deviennent
// des fonctions WASM `(i64…) -> i64` appelables par index. `>> expr` mémorise
// sa valeur dans un local `__result` ; c'est cette valeur que `rose_main`
// rend. Tout ce qui n'est pas couvert renvoie `Err` (jamais de silence). Le
// détail du sous-ensemble est dans `docs/TRANSPILE_WASM.md`.

// Opcodes WASM utilisés (core spec 1.0).
const OP_BLOCK: u8 = 0x02;
const OP_LOOP: u8 = 0x03;
const OP_IF: u8 = 0x04;
const OP_ELSE: u8 = 0x05;
const OP_END: u8 = 0x0B;
const OP_BR: u8 = 0x0C;
const OP_BR_IF: u8 = 0x0D;
const OP_RETURN: u8 = 0x0F;
const OP_CALL: u8 = 0x10;
const OP_DROP: u8 = 0x1A;
const OP_LOCAL_GET: u8 = 0x20;
const OP_LOCAL_SET: u8 = 0x21;
const OP_I64_CONST: u8 = 0x42;
const TY_I64: u8 = 0x7E;
const TY_VOID: u8 = 0x40;
const OP_I32_EQZ: u8 = 0x45;
const OP_I32_AND: u8 = 0x71;
const OP_I32_OR: u8 = 0x72;
const OP_I32_XOR: u8 = 0x73;
const OP_I64_EQZ: u8 = 0x50;
const OP_I64_EQ: u8 = 0x51;
const OP_I64_NE: u8 = 0x52;
const OP_I64_LT_S: u8 = 0x53;
const OP_I64_LE_S: u8 = 0x57;
const OP_I64_GT_S: u8 = 0x55;
const OP_I64_GE_S: u8 = 0x59;
const OP_I64_ADD: u8 = 0x7C;
const OP_I64_SUB: u8 = 0x7D;
const OP_I64_MUL: u8 = 0x7E;
const OP_I64_DIV_S: u8 = 0x7F;
const OP_I64_REM_S: u8 = 0x81;
const OP_I32_WRAP_I64: u8 = 0xA7;
const OP_I64_EXTEND_I32_U: u8 = 0xAD;

/// Entier non signé en LEB128 (longueurs, index, counts).
fn uleb(mut v: u64, out: &mut Vec<u8>) {
    loop {
        let mut b = (v & 0x7F) as u8;
        v >>= 7;
        if v != 0 { b |= 0x80; }
        out.push(b);
        if v == 0 { break; }
    }
}

/// Entier signé en LEB128 (constantes i64).
fn sleb(mut v: i64, out: &mut Vec<u8>) {
    loop {
        let mut b = (v as u8) & 0x7F;
        v >>= 7;
        let fini = (v == 0 && (b & 0x40) == 0) || (v == -1 && (b & 0x40) != 0);
        if !fini { b |= 0x80; }
        out.push(b);
        if fini { break; }
    }
}

/// Une section WASM : id, taille (LEB128), contenu.
fn section_wasm(id: u8, contenu: &[u8], out: &mut Vec<u8>) {
    out.push(id);
    uleb(contenu.len() as u64, out);
    out.extend_from_slice(contenu);
}

/// Chaîne WASM : taille en octets (LEB128) puis UTF-8 brut.
fn chaine_wasm(s: &str, out: &mut Vec<u8>) {
    uleb(s.len() as u64, out);
    out.extend_from_slice(s.as_bytes());
}

/// Type de fonction : `(param i64…) -> i64`.
fn type_fonction_wasm(params: usize, out: &mut Vec<u8>) {
    out.push(0x60); // functype
    uleb(params as u64, out);
    for _ in 0..params { out.push(TY_I64); }
    uleb(1, out);
    out.push(TY_I64);
}

struct WasmEmetteur<'a> {
    prog: &'a Programme,
    /// Noms Rose des fonctions, dans l'ordre ; leur index WASM est `i + 1`
    /// (l'index 0 étant réservé à `rose_main`).
    ordre: Vec<String>,
    index: HashMap<String, u32>,
    /// État de la fonction en cours de compilation.
    /// ⚠ En WASM les PARAMÈTRES occupent les premiers index de locaux (0..k-1)
    /// et les locaux déclarés viennent ENSUITE : `base` = nombre de paramètres.
    locals: Vec<String>,
    local_idx: HashMap<String, u32>,
    base: u32,
    result_idx: u32,
    hidden: u32,
}

impl<'a> WasmEmetteur<'a> {
    fn nouveau(prog: &'a Programme) -> Result<Self, String> {
        let mut index = HashMap::new();
        for (i, nom) in prog.ordre.iter().enumerate() {
            let sig = prog.signatures.get(nom)
                .ok_or_else(|| format!("rose: transpile wasm — fonction '{}' introuvable", nom))?;
            if sig.corps.is_none() {
                return Err(format!("rose: transpile wasm — fonction externe '{}' non transpilable", nom));
            }
            index.insert(nom.clone(), (i as u32) + 1);
        }
        Ok(WasmEmetteur {
            prog, ordre: prog.ordre.clone(), index,
            locals: Vec::new(), local_idx: HashMap::new(),
            base: 0, result_idx: 0, hidden: 0,
        })
    }

    /// Un local i64 supplémentaire (les variables Rose sont toutes des i64).
    /// Les paramètres sont déjà mappés sur 0..base et ne sont PAS redéclarés.
    fn local(&mut self, nom: &str) -> u32 {
        if let Some(i) = self.local_idx.get(nom) { return *i; }
        let i = self.base + self.locals.len() as u32;
        self.locals.push(nom.to_string());
        self.local_idx.insert(nom.to_string(), i);
        i
    }

    /// Local temporaire dont le nom ne peut PAS apparaître dans un programme
    /// Rose (caractère de contrôle en tête) : zéro collision garantie.
    fn hidden(&mut self) -> u32 {
        self.hidden += 1;
        let nom = format!("\u{1}w{}", self.hidden);
        self.local(&nom)
    }

    fn local_get(&self, i: u32, out: &mut Vec<u8>) {
        out.push(OP_LOCAL_GET);
        uleb(i as u64, out);
    }

    fn local_set(&mut self, nom: &str, out: &mut Vec<u8>) {
        let i = self.local(nom);
        out.push(OP_LOCAL_SET);
        uleb(i as u64, out);
    }

    fn const_i64(v: i64, out: &mut Vec<u8>) {
        out.push(OP_I64_CONST);
        sleb(v, out);
    }

    /// Corps d'une fonction : déclaration des locaux, instructions, valeur
    /// de retour (le local `__result`, ou 0), puis `end`.
    fn compiler_fonction(&mut self, params: &[String], instrs: &'a [Instr]) -> Result<Vec<u8>, String> {
        self.locals.clear();
        self.local_idx.clear();
        self.hidden = 0;
        self.base = params.len() as u32;
        // Les paramètres SONT les locaux 0..base (règle WASM), pas des locaux
        // déclarés : ne pas les ajouter à `self.locals`.
        for (i, p) in params.iter().enumerate() {
            self.local_idx.insert(p.clone(), i as u32);
        }
        self.result_idx = self.local("__result");

        let mut code = Vec::new();
        self.instrs(instrs, &mut code)?;
        self.local_get(self.result_idx, &mut code); // laisse exactement un i64

        // Un corps WASM = [taille du corps][vec des groupes de locaux][code][end].
        let mut contenu = Vec::new();
        uleb(1, &mut contenu); // UN groupe de locaux…
        uleb(self.locals.len() as u64, &mut contenu); // …de N i64
        contenu.push(TY_I64);
        contenu.extend_from_slice(&code);
        contenu.push(OP_END);

        let mut corps = Vec::new();
        uleb(contenu.len() as u64, &mut corps);
        corps.extend_from_slice(&contenu);
        Ok(corps)
    }

    fn instrs(&mut self, instrs: &[Instr], out: &mut Vec<u8>) -> Result<(), String> {
        for i in instrs { self.instr(i, out)?; }
        Ok(())
    }

    fn instr(&mut self, i: &Instr, out: &mut Vec<u8>) -> Result<(), String> {
        match i {
            Instr::Afficher(es, _) => {
                // `>>` ne produit PAS de sortie dans un module WASM : il
                // mémorise la valeur, qui devient le résultat de rose_main.
                for e in es {
                    self.expr(e, out)?;
                    let r = self.result_idx;
                    out.push(OP_LOCAL_SET);
                    uleb(r as u64, out);
                }
                Ok(())
            }
            Instr::Affect { nom, e, .. } => {
                self.expr(e, out)?;
                self.local_set(nom, out);
                Ok(())
            }
            Instr::ExprStmt(e) => { self.expr(e, out)?; out.push(OP_DROP); Ok(()) }
            Instr::Retour(e, _) => {
                match e {
                    Some(x) => self.expr(x, out)?,
                    None => Self::const_i64(0, out),
                }
                out.push(OP_RETURN);
                Ok(())
            }
            Instr::Bloc(b) => self.instrs(b, out),
            Instr::Si { test, alors, sinon, .. } => {
                self.expr(test, out)?;          // i64
                out.push(OP_I32_WRAP_I64);      // `if` veut un i32
                out.push(OP_IF);
                out.push(TY_VOID);
                self.instrs(alors, out)?;
                if let Some(s) = sinon {
                    out.push(OP_ELSE);
                    self.instrs(s, out)?;
                }
                out.push(OP_END);
                Ok(())
            }
            Instr::TantQue { test, corps, .. } => {
                out.push(OP_BLOCK); out.push(TY_VOID);
                out.push(OP_LOOP); out.push(TY_VOID);
                self.expr(test, out)?;
                out.push(OP_I32_WRAP_I64);
                out.push(OP_I32_EQZ);           // 1 si la condition est fausse
                out.push(OP_BR_IF);
                uleb(1, out);                   // sortir du BLOCK
                self.instrs(corps, out)?;
                out.push(OP_BR);
                uleb(0, out);                   // reboucler
                out.push(OP_END);               // loop
                out.push(OP_END);               // block
                Ok(())
            }
            Instr::Pour { nom, debut, fin, corps, .. } => {
                // Borne finale EXCLUSIVE, exactement comme l'interpréteur.
                self.expr(debut, out)?;
                self.local_set(nom, out);
                self.expr(fin, out)?;
                let borne = self.hidden();
                out.push(OP_LOCAL_SET);
                uleb(borne as u64, out);
                out.push(OP_BLOCK); out.push(TY_VOID);
                out.push(OP_LOOP); out.push(TY_VOID);
                out.push(OP_LOCAL_GET); uleb(self.local(nom) as u64, out);
                out.push(OP_LOCAL_GET); uleb(borne as u64, out);
                out.push(OP_I64_GE_S);          // i>=borne ? (i32)
                out.push(OP_BR_IF);
                uleb(1, out);
                self.instrs(corps, out)?;
                out.push(OP_LOCAL_GET); uleb(self.local(nom) as u64, out);
                Self::const_i64(1, out);
                out.push(OP_I64_ADD);
                out.push(OP_LOCAL_SET); uleb(self.local(nom) as u64, out);
                out.push(OP_BR);
                uleb(0, out);
                out.push(OP_END);
                out.push(OP_END);
                Ok(())
            }
            Instr::AffectIndex { .. } => Err("rose: transpile wasm — affectation par index non couverte".into()),
            Instr::Lire { .. } => Err("rose: transpile wasm — lecture clavier (`<<`) non couverte".into()),
            Instr::Spawn(..) => Err("rose: transpile wasm — `spawn` non couvert".into()),
            Instr::Essayer { .. } => Err("rose: transpile wasm — `essayer`/`attraper` non couvert".into()),
            Instr::Lever(..) => Err("rose: transpile wasm — `lever` non couvert".into()),
            Instr::Rir(..) => Err("rose: transpile wasm — corps RIR déjà abaissé non couvert".into()),
        }
    }

    fn expr(&mut self, e: &Expr, out: &mut Vec<u8>) -> Result<(), String> {
        match e {
            Expr::Litteral { texte, flott, rationnel, neg, ligne, .. } => {
                if *flott || *rationnel || texte.contains('/') || texte.contains('.') {
                    return Err(format!(
                        "rose: transpile wasm — '{}' n'est pas un entier i64 (ligne {})", texte, ligne));
                }
                let t = texte.replace('_', "");
                let mut v: i128 = t.parse::<i128>().map_err(|_| format!(
                    "rose: transpile wasm — littéral '{}' illisible (ligne {})", texte, ligne))?;
                // Le front-end met SOUVENT le signe DANS `texte` et pose aussi
                // `neg = true` après repli d'un unaire : ne pas négatif deux fois.
                if *neg && v >= 0 { v = -v; }
                let v = i64::try_from(v).map_err(|_| format!(
                    "rose: transpile wasm — '{}' hors de l'intervalle i64 (ligne {})", texte, ligne))?;
                Self::const_i64(v, out);
                Ok(())
            }
            Expr::BooleenLit(b, _) => {
                Self::const_i64(if *b { 1 } else { 0 }, out);
                Ok(())
            }
            Expr::Var(n, _) => { let i = self.local(n); self.local_get(i, out); Ok(()) }
            Expr::Binaire { op, a, b, .. } => self.binaire(op, a, b, out),
            Expr::Unaire { op, a, .. } => match op.as_str() {
                "-" => { Self::const_i64(0, out); self.expr(a, out)?; out.push(OP_I64_SUB); Ok(()) }
                "+" => self.expr(a, out),
                "non" => {
                    self.expr(a, out)?;
                    out.push(OP_I64_EQZ);
                    out.push(OP_I64_EXTEND_I32_U);
                    Ok(())
                }
                _ => Err(format!("rose: transpile wasm — opérateur unaire '{}' non couvert", op)),
            },
            Expr::Appel { nom, module, args, ligne } => {
                if !module.is_empty() {
                    return Err(format!("rose: transpile wasm — appel de module '{}' non couvert (ligne {})", module, ligne));
                }
                let idx = match self.index.get(nom) {
                    Some(i) => *i,
                    None => return Err(format!(
                        "rose: transpile wasm — fonction/primitive '{}' non couverte (ligne {})", nom, ligne)),
                };
                for x in args { self.expr(x, out)?; }
                out.push(OP_CALL);
                uleb(idx as u64, out);
                Ok(())
            }
            Expr::LitComplexe { .. } => Err("rose: transpile wasm — complexes non couverts".into()),
            Expr::ChaineLit(..) => Err("rose: transpile wasm — chaînes non couvertes".into()),
            Expr::Index { .. } | Expr::ListeLit(..) | Expr::ListeVideTypee(..) | Expr::TableLit(..) => {
                Err("rose: transpile wasm — listes/tables non couvertes".into())
            }
            Expr::Factorielle(..) | Expr::MultiFactorielle(..) | Expr::SousFactorielle(..)
            | Expr::Primorielle(..) | Expr::Terminal(..) | Expr::SuperFactorielle(..) => {
                Err("rose: transpile wasm — notations factorielles non couvertes".into())
            }
        }
    }

    fn binaire(&mut self, op: &str, a: &Expr, b: &Expr, out: &mut Vec<u8>) -> Result<(), String> {
        match op {
            "+" | "-" | "*" | "div" | "÷" => {
                self.expr(a, out)?;
                self.expr(b, out)?;
                out.push(match op {
                    "+" => OP_I64_ADD,
                    "-" => OP_I64_SUB,
                    "*" => OP_I64_MUL,
                    _ => OP_I64_DIV_S,
                });
                Ok(())
            }
            "%" => self.modulo(a, b, out),
            "<" | "<=" | ">" | ">=" | "==" | "!=" => {
                self.expr(a, out)?;
                self.expr(b, out)?;
                out.push(match op {
                    "<" => OP_I64_LT_S,
                    "<=" => OP_I64_LE_S,
                    ">" => OP_I64_GT_S,
                    ">=" => OP_I64_GE_S,
                    "==" => OP_I64_EQ,
                    _ => OP_I64_NE,
                });
                // Les comparaisons WASM rendent un i32 ; Rose veut un i64.
                out.push(OP_I64_EXTEND_I32_U);
                Ok(())
            }
            "et" | "ou" => {
                // Normalisation en booléen : (x != 0) ; puis i32.and / i32.or.
                self.expr(a, out)?;
                out.push(OP_I64_EQZ); out.push(OP_I32_EQZ);
                self.expr(b, out)?;
                out.push(OP_I64_EQZ); out.push(OP_I32_EQZ);
                out.push(if op == "et" { OP_I32_AND } else { OP_I32_OR });
                out.push(OP_I64_EXTEND_I32_U);
                Ok(())
            }
            _ => Err(format!("rose: transpile wasm — opérateur '{}' non couvert", op)),
        }
    }

    /// Modulo EUCLIDIEN de Rose : le résultat a le signe du DIVISEUR, comme
    /// dans le préambule JS (`rMod`). `i64.rem_s` seul a le signe du dividende.
    fn modulo(&mut self, a: &Expr, b: &Expr, out: &mut Vec<u8>) -> Result<(), String> {
        let ha = self.hidden();
        let hb = self.hidden();
        let hr = self.hidden();
        self.expr(a, out)?;
        out.push(OP_LOCAL_SET); uleb(ha as u64, out);
        self.expr(b, out)?;
        out.push(OP_LOCAL_SET); uleb(hb as u64, out);
        out.push(OP_LOCAL_GET); uleb(ha as u64, out);
        out.push(OP_LOCAL_GET); uleb(hb as u64, out);
        out.push(OP_I64_REM_S);
        out.push(OP_LOCAL_SET); uleb(hr as u64, out);

        // condition = (r != 0) && ((r < 0) XOR (b < 0))
        out.push(OP_LOCAL_GET); uleb(hr as u64, out);
        out.push(OP_I64_EQZ); out.push(OP_I32_EQZ);
        out.push(OP_LOCAL_GET); uleb(hr as u64, out);
        Self::const_i64(0, out);
        out.push(OP_I64_LT_S);
        out.push(OP_LOCAL_GET); uleb(hb as u64, out);
        Self::const_i64(0, out);
        out.push(OP_I64_LT_S);
        out.push(OP_I32_XOR);
        out.push(OP_I32_AND);
        out.push(OP_IF); out.push(TY_I64);
        out.push(OP_LOCAL_GET); uleb(hr as u64, out);
        out.push(OP_LOCAL_GET); uleb(hb as u64, out);
        out.push(OP_I64_ADD);
        out.push(OP_ELSE);
        out.push(OP_LOCAL_GET); uleb(hr as u64, out);
        out.push(OP_END);
        Ok(())
    }

    /// Assemble le module : header, types, functions, exports, code.
    fn module(&mut self) -> Result<Vec<u8>, String> {
        // OPT_VM — zéro copie des corps : `self.prog` vit `'a`, donc les
        // tranches d'instructions restent valides pendant que `self` est
        // emprunté de façon mutable pour compiler. L'ancien code clonait TOUT l'AST
        // (principal + chaque fonction) avant d'émettre le moindre octet.
        let main_instrs: &'a [Instr] = &self.prog.instructions;
        let mut specs: Vec<(Vec<String>, &'a [Instr])> = Vec::with_capacity(self.ordre.len());
        for nom in &self.ordre {
            let sig = self.prog.signatures.get(nom)
                .ok_or_else(|| format!("rose: transpile wasm — fonction '{}' introuvable", nom))?;
            let corps = sig.corps.as_ref()
                .ok_or_else(|| format!("rose: transpile wasm — fonction '{}' sans corps", nom))?;
            specs.push((sig.params.iter().map(|(n, _)| n.clone()).collect(), corps.as_slice()));
        }

        let mut corps_fonctions = Vec::with_capacity(specs.len() + 1);
        corps_fonctions.push(self.compiler_fonction(&[], main_instrs)?); // index 0
        for (params, corps) in &specs {
            corps_fonctions.push(self.compiler_fonction(params, corps)?);
        }

        let nb = specs.len() + 1;
        let mut types = Vec::new();
        uleb(nb as u64, &mut types);
        type_fonction_wasm(0, &mut types);
        for (p, _) in &specs { type_fonction_wasm(p.len(), &mut types); }

        let mut fonctions = Vec::new();
        uleb(nb as u64, &mut fonctions);
        for i in 0..nb { uleb(i as u64, &mut fonctions); }

        let mut exports = Vec::new();
        uleb(1, &mut exports);
        chaine_wasm("rose_main", &mut exports);
        exports.push(0x00); // kind = func
        uleb(0, &mut exports); // index 0

        let mut code = Vec::new();
        uleb(corps_fonctions.len() as u64, &mut code);
        for c in &corps_fonctions { code.extend_from_slice(c); }

        let mut out = Vec::new();
        out.extend_from_slice(&[0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00]);
        section_wasm(1, &types, &mut out);
        section_wasm(3, &fonctions, &mut out);
        section_wasm(7, &exports, &mut out);
        section_wasm(10, &code, &mut out);
        Ok(out)
    }
}

/// Transpile un AST Rose déjà analysé vers un module WebAssembly binaire.
pub fn transpiler_programme_wasm(prog: &Programme) -> Result<Vec<u8>, String> {
    WasmEmetteur::nouveau(prog)?.module()
}

// ── Tests unitaires (sans node/python : ils ne sont pas des dépendances) ───

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = r#"
fn fib(n) {
  si (n < 2) { retour n }
  retour fib(n - 1) + fib(n - 2)
}
>> fib(10)
>> 7 div 2
>> -7 % 3
>> 2 ^ 10
>> 1 / 3
>> "ab" + "cd"
$s = 0
pour k de 0 vers 5 {
  $s = $s + $k
}
>> $s
"#;

    #[test]
    fn cibles_parsees() {
        assert_eq!(Cible::parser("js").unwrap(), Cible::Js);
        assert_eq!(Cible::parser("PYTHON").unwrap(), Cible::Py);
        assert_eq!(Cible::parser("c99").unwrap(), Cible::C);
        assert!(Cible::parser("rust").is_err());
    }

    #[test]
    fn emet_js_executable() {
        let code = transpiler_source(BASE, Cible::Js).unwrap();
        assert!(code.contains("class RQ"));
        assert!(code.contains("function v_fib(v_n)"));
        assert!(code.contains("for (v_k = __r"));
        assert!(code.contains("rPrint(new RQ(1n, 3n));"));
        assert!(code.ends_with('\n'));
    }

    #[test]
    fn emet_python_executable() {
        let code = transpiler_source(BASE, Cible::Py).unwrap();
        assert!(code.contains("from fractions import Fraction"));
        assert!(code.contains("def v_fib(v_n):"));
        assert!(code.contains("while rLt(v_k"));
        assert!(code.contains("Fraction(1, 3)"));
    }

    #[test]
    fn emet_c_executable() {
        let src = "fn carre(x) { retour x * x }\n>> carre(7)\n";
        let code = transpiler_source(src, Cible::C).unwrap();
        assert!(code.contains("typedef struct { RType t"));
        assert!(code.contains("RV v_carre(RV v_x)"));
        assert!(code.contains("int main(void)"));
    }

    #[test]
    fn refuse_hors_sous_ensemble() {
        // complexes
        assert!(transpiler_source(">> 2 * i\n", Cible::Js).is_err());
        // lecture clavier
        assert!(transpiler_source("<< x\n", Cible::Js).is_err());
        // rational en C
        assert!(transpiler_source(">> 1 / 2\n", Cible::C).is_err());
        // listes en C
        assert!(transpiler_source(">> [1, 2]\n", Cible::C).is_err());
    }

    #[test]
    fn listes_et_tables_js_py() {
        let src = r#"
$t = table()
inserer($t, "a", 1)
>> $t
>> [1, 2, 3]
>> longueur([1, 2, 3])
"#;
        let js = transpiler_source(src, Cible::Js).unwrap();
        assert!(js.contains("rTable()"));
        assert!(js.contains("rSet("));
        assert!(js.contains("rList("));
        let py = transpiler_source(src, Cible::Py).unwrap();
        assert!(py.contains("RTable()"));
        assert!(py.contains("rList("));
    }

    #[test]
    fn table_litterale_est_une_expression() {
        let src = ">> {a:: 1, b:: 2}\n";
        let js = transpiler_source(src, Cible::Js).unwrap();
        assert!(js.contains("rTableFrom("), "table littérale perdue en JS");
        let py = transpiler_source(src, Cible::Py).unwrap();
        assert!(py.contains("rTableFrom("), "table littérale perdue en Python");
    }

    #[test]
    fn noms_stables() {
        assert_eq!(ident("s"), "v_s");
        assert_eq!(ident("π"), "v__u3c0_");
        assert_eq!(ident("x2"), "v_x2");
    }

    // ── WASM ──────────────────────────────────────────────────────────────

    #[test]
    fn cible_wasm_parsee() {
        assert_eq!(Cible::parser("wasm").unwrap(), Cible::Wasm);
        assert_eq!(Cible::parser("WASM32").unwrap(), Cible::Wasm);
        assert_eq!(Cible::parser("webassembly").unwrap(), Cible::Wasm);
        assert_eq!(Cible::Wasm.nom(), "wasm");
        assert_eq!(Cible::Wasm.extension(), "wasm");
        // La voie texte refuse explicitement la cible binaire.
        assert!(transpiler_source(">> 42\n", Cible::Wasm).is_err());
    }

    #[test]
    fn module_wasm_header_et_sections() {
        let octets = transpiler_source_wasm(">> 7 * 6\n").unwrap();
        assert_eq!(&octets[..8], &[0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00]);
        // Après le header : type (1), function (3), export (7), code (10).
        let mut ids = Vec::new();
        let mut p = 8;
        while p < octets.len() {
            ids.push(octets[p]);
            p += 1;
            let (_n, q) = lire_uleb(&octets, p);
            p = q + _n as usize;
        }
        assert_eq!(ids, vec![1, 3, 7, 10]);
        assert!(p == octets.len(), "section qui déborde du module");
    }

    #[test]
    fn module_wasm_exporte_rose_main() {
        let octets = transpiler_source_wasm(">> 42\n").unwrap();
        let chaine = b"rose_main";
        assert!(octets.windows(chaine.len()).any(|w| w == chaine));
    }

    #[test]
    fn wasm_refuse_hors_sous_ensemble() {
        assert!(transpiler_source_wasm(">> 1 / 2\n").is_err());     // rationnel
        assert!(transpiler_source_wasm(">> 2 * i\n").is_err());     // complexe
        assert!(transpiler_source_wasm(">> \"abc\"\n").is_err());   // chaîne
        assert!(transpiler_source_wasm("<< x\n").is_err());        // clavier
        assert!(transpiler_source_wasm(">> [1, 2]\n").is_err());   // liste
        assert!(transpiler_source_wasm(">> factorielle(5)\n").is_err());
    }

    /// Petit lecteur de test : lit un ULEB128 et rend (valeur, index suivant).
    fn lire_uleb(o: &[u8], mut p: usize) -> (u64, usize) {
        let mut v = 0u64;
        let mut dec = 0;
        loop {
            let b = o[p];
            p += 1;
            v |= ((b & 0x7F) as u64) << dec;
            dec += 7;
            if b & 0x80 == 0 { break; }
        }
        (v, p)
    }
}
