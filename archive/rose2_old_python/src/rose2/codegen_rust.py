"""Rose v2 — backend Rust (types machine i64/f64, sans GMP).

Ce backend vise la vitesse et l'intégrité mémoire sur valeurs machine. Il
refuse à la compilation (message explicite) tout programme utilisant
rationnel, chaîne, liste ou FFI dynamique — ces fonctionnalités sont
réservées aux cibles C/Python/Julia.
"""
from __future__ import annotations
from .parser import (Programme, Signature, TypeRose, Litteral, ChaineLit, BooleenLit,
                     ListeLit, Var, Affectation, Binaire, Unaire, Factorielle, Appel,
                     Index, Bloc, Si, TantQue, Pour, Retour, Afficher, Lire,
                     ExprStatement, T_ENTIER, T_RATIONNEL, T_FLOTTANT, T_BOOLEEN,
                     T_CHAINE, T_VIDE)
from .lexer import ErreurRose

I64_MIN, I64_MAX = -(2 ** 63), 2 ** 63 - 1

PRELUDE = r"""// Généré par le compilateur Rose v2 — cible Rust (types machine).
#![allow(unused_assignments, unused_mut, unused_variables, unused_parens, non_snake_case)]
use std::io;

fn rose_erreur(msg: &str) -> ! {
    eprintln!("Rose : erreur — {}", msg);
    std::process::exit(1);
}

fn rose_lire() -> String {
    let mut tampon = String::new();
    match io::stdin().read_line(&mut tampon) {
        Ok(0) | Err(_) => rose_erreur("entrée attendue, fichier fermé"),
        Ok(_) => tampon.trim().to_string(),
    }
}
fn rose_lire_entier() -> i64 {
    let t = rose_lire();
    if let Some(pos) = t.find('/') {
        let n: f64 = t[..pos].trim().parse().unwrap_or_else(|_| rose_erreur("entier invalide"));
        let d: f64 = t[pos + 1..].trim().parse().unwrap_or_else(|_| rose_erreur("entier invalide"));
        if d == 0.0 { rose_erreur("division par zéro"); }
        return (n / d).trunc() as i64;
    }
    if t.contains('.') || t.contains('e') {
        let v: f64 = t.parse().unwrap_or_else(|_| rose_erreur("entier invalide"));
        return v.trunc() as i64;
    }
    t.parse::<i64>().unwrap_or_else(|_| rose_erreur("entier invalide"))
}
fn rose_lire_flottant() -> f64 {
    let t = rose_lire();
    if let Some(pos) = t.find('/') {
        let n: f64 = t[..pos].trim().parse().unwrap_or_else(|_| rose_erreur("flottant invalide"));
        let d: f64 = t[pos + 1..].trim().parse().unwrap_or_else(|_| rose_erreur("flottant invalide"));
        if d == 0.0 { rose_erreur("division par zéro"); }
        return n / d;
    }
    t.parse::<f64>().unwrap_or_else(|_| rose_erreur("flottant invalide"))
}

fn _div(a: i64, b: i64) -> i64 {
    if b == 0 { rose_erreur("division par zéro"); }
    a.wrapping_div(b)
}
fn _div_e(a: i64, b: i64) -> i64 {
    if b == 0 { rose_erreur("division euclidienne par zéro"); }
    a.div_euclid(b)
}
fn _divf(a: f64, b: f64) -> f64 {
    if b == 0.0 { rose_erreur("division euclidienne par zéro"); }
    (a / b).floor()
}
fn _dive(a: f64, b: f64) -> f64 {
    if b == 0.0 { rose_erreur("division par zéro"); }
    a / b
}
fn _mod(a: i64, b: i64) -> i64 {
    if b == 0 { rose_erreur("modulo par zéro"); }
    let r = a % b;
    if r != 0 && (r < 0) != (b < 0) { r + b } else { r }
}
fn _verif_i64(v: i128) -> i64 {
    if v > i64::MAX as i128 || v < i64::MIN as i128 {
        rose_erreur("dépassement i64 : la cible Rust est à précision machine (utilise C/GMP)");
    }
    v as i64
}
fn _powr_base(a: i64, e: i64) -> i64 {
    if e < 0 { rose_erreur("exposant négatif : la cible Rust (i64) ne rend pas de rationnel (utilise la cible C)"); }
    let mut resultat: i128 = 1;
    let mut base = a as i128;
    let mut expo = e as u64;
    while expo > 0 {
        if expo & 1 == 1 {
            resultat *= base;
            _verif_i64(resultat);
        }
        expo >>= 1;
        if expo > 0 {
            base = base * base;
            if base.abs() > (1i128 << 100) { _verif_i64(base); }
        }
    }
    _verif_i64(resultat)
}
fn _fact(n: i64) -> i64 {
    if n < 0 { rose_erreur("factorielle d'un négatif : impossible"); }
    let mut r: i128 = 1;
    for k in 2..=(n as i128) {
        r *= k;
        if r > i64::MAX as i128 { rose_erreur("dépassement i64 (utilise la cible C/GMP)"); }
    }
    r as i64
}
fn _fib(n: i64) -> i64 {
    if n < 0 { rose_erreur("fibonacci négatif : impossible"); }
    let (mut a, mut b): (i128, i128) = (0, 1);
    if n > 0 {
        for i in (0..(64 - n.leading_zeros())).rev() {
            let t1 = a * (2 * b - a);
            let t2 = a * a + b * b;
            if t1 > i64::MAX as i128 || t2 > i64::MAX as i128 {
                rose_erreur("dépassement i64 (utilise la cible C/GMP)");
            }
            if (n >> i) & 1 == 1 { a = t2; b = t1 + t2; } else { a = t1; b = t2; }
        }
    }
    a as i64
}
fn _pgcd(a: i64, b: i64) -> i64 {
    fn euclide(x: i64, y: i64) -> i64 { if y == 0 { x.abs() } else { euclide(y, x % y) } }
    euclide(a, b)
}
fn _ppcm(a: i64, b: i64) -> i64 {
    if a == 0 || b == 0 { return 0; }
    (a / _pgcd(a, b)).abs().wrapping_mul(b.abs())
}
fn _mulmod(a: u64, b: u64, m: u64) -> u64 {
    ((a as u128 * b as u128) % m as u128) as u64
}
fn _powmod(mut base: u64, mut expo: u64, m: u64) -> u64 {
    let mut r: u64 = 1;
    base %= m;
    while expo > 0 {
        if expo & 1 == 1 { r = _mulmod(r, base, m); }
        base = _mulmod(base, base, m);
        expo >>= 1;
    }
    r
}
fn _prem(n: i64) -> bool {
    if n < 2 { return false; }
    for p in [2i64, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37] {
        if n % p == 0 { return n == p; }
    }
    let (un_u, nu) = ((n - 1) as u64, n as u64);
    let mut d = un_u; let mut r = 0u32;
    while d % 2 == 0 { d /= 2; r += 1; }
    for a in [2u64, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37] {
        let mut x = _powmod(a, d, nu);
        if x == 1 || x == nu - 1 { continue; }
        let mut temoin = false;
        for _ in 0..r - 1 {
            x = _mulmod(x, x, nu);
            if x == nu - 1 { temoin = true; break; }
        }
        if !temoin { return false; }
    }
    true
}
fn _compter_premiers(n: i64) -> i64 {
    if n < 2 { return 0; }
    let ni = n as usize;
    let mut crible = vec![false; ni + 1];
    let mut compte: i64 = 0;
    for i in 2..=ni {
        if !crible[i] {
            compte += 1;
            if (i as u64) * (i as u64) <= ni as u64 {
                let mut j = i * i;
                while j <= ni { crible[j] = true; j += i; }
            }
        }
    }
    compte
}
fn _approxe(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0)
}
fn _nb_chiffres(n: i64) -> i64 {
    if n == 0 { return 1; }
    let mut v = if n < 0 { n.wrapping_neg() } else { n };
    let mut c: i64 = 0;
    while v != 0 { v /= 10; c += 1; }
    c
}
fn _afficher_f64(d: f64) {
    if d.is_nan() { println!("non_nombre"); } else { println!("{}", d); }
}
"""


class CodegenRust:
    def __init__(self, prog: Programme):
        self.prog = prog
        self.lignes: list[str] = []
        self.declaire: set[str] = set()
        self.types_variable: dict[str, TypeRose] = {}
        self.bibliotheques: set[str] = set()
        self.externes: list[str] = []
        self.fonction: Signature | None = None

    # -- programme -----------------------------------------------------------
    def generer(self) -> str:
        for sig in self.prog.signatures.values():
            if sig.corps is None:
                self._externe(sig)
        corps: list[str] = [PRELUDE]
        corps.extend(self.externes)
        for sig in self.prog.signatures.values():
            if sig.corps is not None:
                corps.append(self._fonction(sig))
        self.lignes, self.declaire, self.types_variable, self.fonction = [], set(), {}, None
        for instr in self.prog.instructions:
            self._instr(instr, 1)
        corps.append("fn main() {")
        corps.extend(self._pre_declarations(1))
        corps.extend(self.lignes)
        corps.append("}")
        return "\n".join(corps) + "\n"

    def _nom(self, nom: str) -> str:
        return "rose_v_" + "".join(
            c if (c.isascii() and c.isalnum()) or c == "_" else f"_x{ord(c):x}_" for c in nom)

    @staticmethod
    def _type_rust(t: TypeRose) -> str:
        return {T_ENTIER: "i64", T_FLOTTANT: "f64", T_BOOLEEN: "bool"}.get(t, "i64")

    def _defaut(self, ty: str) -> str:
        return "0.0" if ty == "f64" else ("false" if ty == "bool" else "0")

    def _pre_declarations(self, ind: int, exclus: set[str] = frozenset()) -> list[str]:
        pad = "    " * ind
        return [f"{pad}let mut {nom}: {self._type_rust(self.types_variable.get(nom, T_ENTIER))}"
                f" = {self._defaut(self._type_rust(self.types_variable.get(nom, T_ENTIER)))};"
                for nom in sorted(self.declaire - set(exclus))]

    def _externe(self, sig: Signature):
        rs = {T_ENTIER: "i64", T_FLOTTANT: "f64", T_BOOLEEN: "i32", T_CHAINE: "*const i8"}
        if sig.retour == T_CHAINE or any(t == T_CHAINE for (_, t) in sig.params):
            raise ErreurRose("la cible Rust ne passe pas de chaînes aux fonctions externes "
                             "(utilise la cible C)")
        params = ", ".join(f"{self._nom(p)}: {rs[t]}" for (p, t) in sig.params)
        symbole = sig.symbole or sig.nom
        self.externes.append(f'extern "C" {{ fn {symbole}({params}) -> {rs[sig.retour]}; }}')
        if sig.dll and sig.dll not in ("c", "m"):
            self.bibliotheques.add(sig.dll)

    def _fonction(self, sig: Signature) -> str:
        self.lignes, self.declaire = [], set()
        self.types_variable = {p: t for (p, t) in sig.params}
        self.fonction = sig
        for instr in sig.corps.instructions:
            self._instr(instr, 1)
        params = ", ".join(f"mut {self._nom(p)}: {self._type_rust(t)}" for (p, t) in sig.params)
        ret = self._type_rust(sig.retour) if sig.retour != T_VIDE else "()"
        pre = self._pre_declarations(1, {self._nom(p) for (p, _) in sig.params})
        finit_par_retour = bool(self.lignes) and self.lignes[-1].strip().startswith("return")
        queue = "" if (sig.retour == T_VIDE or finit_par_retour) else f"    {self._defaut(ret)}\n"
        return (f"fn {self._nom(sig.nom)}({params}) -> {ret} {{\n"
                + "\n".join(pre + self.lignes) + "\n" + queue + "}")

    # -- instructions -----------------------------------------------------------
    def _instr(self, instr, ind: int):
        pad = "    " * ind
        if isinstance(instr, Afficher):
            t = instr.expression.type
            ref = self._expr(instr.expression)
            if t == T_FLOTTANT:
                self.lignes.append(f"{pad}_afficher_f64({ref});")
            elif t == T_BOOLEEN:
                self.lignes.append(f'{pad}println!("{{}}", if {ref} {{ "vrai" }} else {{ "faux" }});')
            else:
                self.lignes.append(f'{pad}println!("{{}}", {ref});')
        elif isinstance(instr, Lire):
            nom = self._nom(instr.nom)
            self.declaire.add(nom)
            t = getattr(instr, "type_read", T_ENTIER)
            self.types_variable.setdefault(nom, t)
            if t == T_FLOTTANT:
                self.lignes.append(f"{pad}{nom} = rose_lire_flottant();")
            else:
                self.lignes.append(f"{pad}{nom} = rose_lire_entier();")
        elif isinstance(instr, Affectation):
            nom = self._nom(instr.nom)
            if nom not in self.declaire:
                self.declaire.add(nom)
                self.types_variable.setdefault(nom, instr.valeur.type)
            self.lignes.append(f"{pad}{nom} = {self._expr(instr.valeur)};")
        elif isinstance(instr, ExprStatement):
            self.lignes.append(f"{pad}let _ = {self._expr(instr.expression)};")
        elif isinstance(instr, Si):
            self.lignes.append(f"{pad}if {self._expr(instr.test)} {{")
            for i in instr.alors.instructions:
                self._instr(i, ind + 1)
            if instr.sinon is not None:
                self.lignes.append(f"{pad}}} else {{")
                for i in instr.sinon.instructions:
                    self._instr(i, ind + 1)
            self.lignes.append(f"{pad}}}")
        elif isinstance(instr, TantQue):
            self.lignes.append(f"{pad}while {self._expr(instr.test)} {{")
            for i in instr.corps.instructions:
                self._instr(i, ind + 1)
            self.lignes.append(f"{pad}}}")
        elif isinstance(instr, Pour):
            var = self._nom(instr.variable)
            self.declaire.add(var)
            self.types_variable.setdefault(instr.variable, T_ENTIER)
            self.lignes.append(f"{pad}{var} = {self._expr(instr.debut)};")
            self.lignes.append(f"{pad}while {var} < {self._expr(instr.fin)} {{")
            for i in instr.corps.instructions:
                self._instr(i, ind + 1)
            self.lignes.append(f"{pad}    {var} += 1;")
            self.lignes.append(f"{pad}}}")
        elif isinstance(instr, Retour):
            if instr.valeur is None:
                self.lignes.append(f"{pad}return;")
            else:
                self.lignes.append(f"{pad}return {self._expr(instr.valeur)};")
        elif isinstance(instr, Bloc):
            for i in instr.instructions:
                self._instr(i, ind)
        else:
            raise ErreurRose(f"instruction non supportée en Rust : {type(instr).__name__}")

    # -- expressions -----------------------------------------------------------
    def _expr(self, e) -> str:
        if isinstance(e, Litteral):
            if e.flottant:
                return repr(float(e.texte)) + "f64"
            if "/" in e.texte:
                raise ErreurRose("valeur rationnelle : la cible Rust (i64) est à précision "
                                 "machine (utilise les cibles C, Python ou Julia)")
            v = int(e.texte)
            if not (I64_MIN <= v <= I64_MAX):
                raise ErreurRose(f"valeur {v} hors i64 : la cible Rust est à précision machine "
                                 f"(utilise les cibles C, Python ou Julia)")
            return f"{v}i64"
        if isinstance(e, ChaineLit):
            raise ErreurRose("chaînes non supportées par la cible Rust (utilise C, Python ou Julia)")
        if isinstance(e, BooleenLit):
            return "true" if e.valeur else "false"
        if isinstance(e, Var):
            return self._nom(e.nom)
        if isinstance(e, (ListeLit, Index)):
            raise ErreurRose("listes non supportées par la cible Rust (utilise C, Python ou Julia)")
        if isinstance(e, Unaire):
            if e.op == "non":
                return f"(!({self._expr(e.operande)}))"
            return f"(-({self._expr(e.operande)}))"
        if isinstance(e, Factorielle):
            return f"_fact({self._expr(e.operande)})"
        if isinstance(e, Binaire):
            return self._binaire(e)
        if isinstance(e, Appel):
            return self._appel(e)
        raise ErreurRose(f"expression non supportée en Rust : {type(e).__name__}")

    def _flottant(self, e: Binaire) -> bool:
        return e.gauche.type == T_FLOTTANT or e.droite.type == T_FLOTTANT or e.type == T_FLOTTANT

    def _binaire(self, e: Binaire) -> str:
        op = e.op
        g, d = self._expr(e.gauche), self._expr(e.droite)
        floaty = self._flottant(e)
        if floaty and op in ("+", "-", "*"):
            return f"({g} as f64 {op} {d} as f64)"
        if floaty and op in ("<", "<=", ">", ">=", "==", "!="):
            return f"({g} as f64 {op} {d} as f64)"
        if op == "et":
            return f"({g} && {d})"
        if op == "ou":
            return f"({g} || {d})"
        if op == "/":
            return f"_dive({g} as f64, {d} as f64)" if floaty else f"_div({g}, {d})"
        if op == "div":
            return f"_divf({g} as f64, {d} as f64)" if floaty else f"_div_e({g}, {d})"
        if op == "%":
            return f"_mod({g}, {d})"
        if op == "^":
            return f"(({g} as f64).powf({d} as f64))" if floaty else f"_powr_base({g}, {d})"
        if op in ("≈", "?=", "=~"):
            if floaty:
                return f"_approxe({g} as f64, {d} as f64)"
            return f"({g} == {d})"
        return f"({g} {op} {d})"

    def _appel(self, e: Appel) -> str:
        if e.module == "dynamique":
            raise ErreurRose("FFI dynamique non supportée par la cible Rust (utilise la cible C)")
        args = [self._expr(a) for a in e.args]
        sig = self.prog.signatures.get(e.nom)
        if sig is not None:
            if sig.retour == T_VIDE:
                return f"({self._nom(e.nom)}({', '.join(args)}))"
            return f"{self._nom(e.nom)}({', '.join(args)})"
        if e.nom == "abs":
            return f"({args[0]}).abs()"
        if e.nom in ("min", "max"):
            f = "min" if e.nom == "min" else "max"
            return f"({args[0]}).{f}({args[1]})"
        if e.nom == "puissance":
            if e.args[0].type == T_FLOTTANT:
                return f"({args[0]}).powi(({args[1]}) as i32)"
            return f"_powr_base({args[0]}, {args[1]})"
        if e.nom == "partie_entiere":
            return f"({args[0]}).floor() as i64" if e.args[0].type == T_FLOTTANT else args[0]
        if e.nom == "flottant":
            return f"({args[0]}) as f64"
        if e.nom == "entier":
            return f"({args[0]}) as i64" if e.args[0].type == T_FLOTTANT else args[0]
        if e.nom == "afficher":
            t = e.args[0].type
            if t == T_FLOTTANT:
                return f"_afficher_f64({args[0]})"
            if t == T_BOOLEEN:
                return f'println!("{{}}", if {args[0]} {{ "vrai" }} else {{ "faux" }})'
            return f'println!("{{}}", {args[0]})'
        table = {"factorielle": "_fact", "fibonacci": "_fib", "pgcd": "_pgcd", "ppcm": "_ppcm",
                 "est_premier": "_prem", "compter_premiers": "_compter_premiers", "nb_chiffres": "_nb_chiffres"}
        fn = table.get(e.nom)
        if fn is None:
            raise ErreurRose(f"fonction {e.nom!r} non supportée par la cible Rust")
        return f"{fn}({', '.join(args)})"


def generer_rust(prog: Programme) -> str:
    return CodegenRust(prog).generer()
