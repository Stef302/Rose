"""Rose v2 — backend Zig (types machine i64/f64).

Comme Rust : intégrité et vitesse sur valeurs machine ; refus explicite des
fonctionnalités arbitrairement précises (elles relèvent de C/GMP, Python, Julia).
"""
from __future__ import annotations
from .parser import (Programme, Signature, TypeRose, Litteral, ChaineLit, BooleenLit,
                     ListeLit, Var, Affectation, Binaire, Unaire, Factorielle, Appel,
                     Index, Bloc, Si, TantQue, Pour, Retour, Afficher, Lire,
                     ExprStatement, T_ENTIER, T_RATIONNEL, T_FLOTTANT, T_BOOLEEN,
                     T_CHAINE, T_VIDE)
from .lexer import ErreurRose

I64_MIN, I64_MAX = -(2 ** 63), 2 ** 63 - 1

PRELUDE = r"""// Généré par le compilateur Rose v2 — cible Zig.
const std = @import("std");

fn roseErreur(msg: []const u8) noreturn {
    std.debug.print("Rose : erreur — {s}\n", .{msg});
    std.process.exit(1);
}

fn lireLigne() []const u8 {
    const stdin = std.io.getStdIn().reader();
    var tampon: [4096]u8 = undefined;
    const ligne = stdin.readUntilDelimiterOrEof(&tampon, '\n')
        catch roseErreur("entrée attendue, fichier fermé")
        orelse roseErreur("entrée attendue, fichier fermé");
    return std.mem.trim(u8, ligne, " \r");
}

fn fl(x: anytype) f64 {
    if (@TypeOf(x) == bool) return @floatFromInt(@intFromBool(x)) else return @floatFromInt(x);
}

fn roseLireEntier() i64 {
    const t = lireLigne();
    if (std.mem.indexOfScalar(u8, t, '/') != null) {
        var p = std.mem.splitScalar(u8, t, '/');
        const n = std.fmt.parseFloat(f64, p.first()) catch roseErreur("entier invalide");
        const d = std.fmt.parseFloat(f64, p.rest()) catch roseErreur("entier invalide");
        if (d == 0) roseErreur("division par zéro");
        return @intFromFloat(@trunc(n / d));
    }
    if (std.mem.indexOfScalar(u8, t, '.') != null or std.mem.indexOfScalar(u8, t, 'e') != null) {
        const v = std.fmt.parseFloat(f64, t) catch roseErreur("entier invalide");
        return @intFromFloat(@trunc(v));
    }
    return std.fmt.parseInt(i64, t, 10) catch roseErreur("entier invalide");
}
fn roseLireFlottant() f64 {
    const t = lireLigne();
    if (std.mem.indexOfScalar(u8, t, '/') != null) {
        var p = std.mem.splitScalar(u8, t, '/');
        const n = std.fmt.parseFloat(f64, p.first()) catch roseErreur("flottant invalide");
        const d = std.fmt.parseFloat(f64, p.rest()) catch roseErreur("flottant invalide");
        if (d == 0) roseErreur("division par zéro");
        return n / d;
    }
    return std.fmt.parseFloat(f64, t) catch roseErreur("flottant invalide");
}

fn divI(a: i64, b: i64) i64 {
    if (b == 0) roseErreur("division euclidienne par zéro");
    return @divFloor(a, b);
}
fn divf(a: f64, b: f64) f64 {
    if (b == 0) roseErreur("division euclidienne par zéro");
    return @floor(a / b);
}
fn dive(a: f64, b: f64) f64 {
    if (b == 0) roseErreur("division par zéro");
    return a / b;
}
fn modI(a: i64, b: i64) i64 {
    if (b == 0) roseErreur("modulo par zéro");
    const r = @rem(a, b);
    return if (r != 0 and ((r < 0) != (b < 0))) r + b else r;
}
fn verifI64(v: i128) i64 {
    const mini: i128 = std.math.minInt(i64);
    const maxi: i128 = std.math.maxInt(i64);
    if (v > maxi or v < mini) roseErreur("dépassement i64 : la cible Zig est à précision machine (utilise C/GMP)");
    return @intCast(v);
}
fn powrBase(a: i64, e: i64) i64 {
    if (e < 0) roseErreur("exposant négatif : la cible Zig (i64) ne rend pas de rationnel (utilise la cible C)");
    var resultat: i128 = 1;
    var base: i128 = a;
    var expo: u64 = @intCast(e);
    while (expo > 0) {
        if (expo & 1 == 1) {
            resultat *= base;
            _ = verifI64(resultat);
        }
        expo >>= 1;
        if (expo > 0) {
            base *= base;
            if (@abs(base) > (@as(i128, 1) << 100)) _ = verifI64(base);
        }
    }
    return verifI64(resultat);
}
fn facteur(n: i64) i64 {
    if (n < 0) roseErreur("factorielle d'un négatif : impossible");
    var r: i128 = 1;
    var k: i64 = 2;
    while (k <= n) : (k += 1) {
        r *= k;
        if (r > std.math.maxInt(i64)) roseErreur("dépassement i64 (utilise la cible C/GMP)");
    }
    return @intCast(r);
}
fn fibo(n: i64) i64 {
    if (n < 0) roseErreur("fibonacci négatif : impossible");
    var a: i128 = 0;
    var b: i128 = 1;
    if (n > 0) {
        var trouve = false;
        var p: u6 = 63;
        while (true) {
            if (!trouve and ((n >> p) & 1) == 1) trouve = true;
            if (trouve) {
                const t1 = a * (2 * b - a);
                const t2 = a * a + b * b;
                if (t1 > std.math.maxInt(i64) or t2 > std.math.maxInt(i64))
                    roseErreur("dépassement i64 (utilise la cible C/GMP)");
                if ((n >> p) & 1 == 1) { a = t2; b = t1 + t2; } else { a = t1; b = t2; }
            }
            if (p == 0) break;
            p -= 1;
        }
    }
    return @intCast(a);
}
fn pgcd(a: i64, b: i64) i64 {
    var x = if (a < 0) -a else a;
    var y = if (b < 0) -b else b;
    while (y != 0) {
        const t = y;
        y = @rem(x, y);
        x = t;
    }
    return x;
}
fn ppcm(a: i64, b: i64) i64 {
    if (a == 0 or b == 0) return 0;
    const g = pgcd(a, b);
    const q = @divTrunc(if (a < 0) -a else a, g);
    const q2 = if (q < 0) -q else q;
    const b2 = if (b < 0) -b else b;
    return q2 * b2;
}
fn estPremier(n: i64) bool {
    if (n < 2) return false;
    const petits = [_]i64{ 2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37 };
    for (petits) |p| {
        if (@rem(n, p) == 0) return n == p;
    }
    var d: u64 = @intCast(n - 1);
    var r: u32 = 0;
    while (d % 2 == 0) : (d /= 2) r += 1;
    const nu: u64 = @intCast(n);
    const bases = [_]u64{ 2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37 };
    outer: for (bases) |a| {
        var x = powmod(a, d, nu);
        if (x == 1 or x == nu - 1) continue;
        var j: u32 = 1;
        while (j < r) : (j += 1) {
            x = mulmod(x, x, nu);
            if (x == nu - 1) continue :outer;
        }
        return false;
    }
    return true;
}
fn mulmod(a: u64, b: u64, m: u64) u64 {
    return @intCast(@mod(@as(u128, a) * @as(u128, b), @as(u128, m)));
}
fn powmod(base0: u64, expo0: u64, m: u64) u64 {
    var base = base0 % m;
    var expo = expo0;
    var r: u64 = 1;
    while (expo > 0) {
        if (expo & 1 == 1) r = mulmod(r, base, m);
        base = mulmod(base, base, m);
        expo >>= 1;
    }
    return r;
}
fn compterPremiers(n: i64) i64 {
    if (n < 2) return 0;
    const ni: usize = @intCast(n);
    var gpa = std.heap.page_allocator;
    const crible = gpa.alloc(bool, ni + 1) catch roseErreur("mémoire insuffisante pour le crible");
    defer gpa.free(crible);
    @memset(crible, false);
    var compte: i64 = 0;
    var i: usize = 2;
    while (i <= ni) : (i += 1) {
        if (!crible[i]) {
            compte += 1;
            if (@as(u64, i) * @as(u64, i) <= ni) {
                var j = i * i;
                while (j <= ni) : (j += i) crible[j] = true;
            }
        }
    }
    return compte;
}
fn approche(a: f64, b: f64) bool {
    return @abs(a - b) <= 1e-9 * @max(@max(@abs(a), @abs(b)), 1.0);
}
fn nbChiffres(n: i64) i64 {
    if (n == 0) return 1;
    var v = if (n < 0) n * -1 else n;
    var c: i64 = 0;
    while (v != 0) : (c += 1) v = @divTrunc(v, 10);
    return c;
}
fn afficherF64(d: f64) void {
    const w = std.io.getStdOut().writer();
    if (std.math.isNan(d)) { w.print("non_nombre\n", .{}) catch return; }
    w.print("{d}\n", .{d}) catch {};
}
fn afficherI64(v: i64) void {
    const w = std.io.getStdOut().writer();
    w.print("{d}\n", .{v}) catch {};
}
fn afficherBooleen(v: bool) void {
    const w = std.io.getStdOut().writer();
    w.print("{s}\n", .{if (v) "vrai" else "faux"}) catch {};
}
"""


class CodegenZig:
    def __init__(self, prog: Programme):
        self.prog = prog
        self.lignes: list[str] = []
        self.declaire: dict[str, TypeRose] = {}
        self.bibliotheques: set[str] = set()
        self.externes: list[str] = []
        self.fonction: Signature | None = None

    def generer(self) -> str:
        for sig in self.prog.signatures.values():
            if sig.corps is None:
                self._externe(sig)
        corps: list[str] = [PRELUDE]
        corps.extend(self.externes)
        for sig in self.prog.signatures.values():
            if sig.corps is not None:
                corps.append(self._fonction(sig))
        self.lignes, self.declaire, self.fonction = [], {}, None
        for instr in self.prog.instructions:
            self._instr(instr, 1)
        pre = self._pre_declarations(1)
        corps.append("pub fn main() void {")
        corps.extend(pre)
        corps.extend(self.lignes)
        corps.append("}")
        return "\n".join(corps) + "\n"

    def _nom(self, nom: str) -> str:
        base = "".join(c if (c.isascii() and c.isalnum()) or c == "_" else f"_x{ord(c):x}_"
                       for c in nom)
        return "rose_v_" + base

    @staticmethod
    def _type_zig(t: TypeRose) -> str:
        return {T_ENTIER: "i64", T_FLOTTANT: "f64", T_BOOLEEN: "bool"}.get(t, "i64")

    def _pre_declarations(self, ind: int, exclus: set[str] = frozenset()) -> list[str]:
        pad = "    " * ind
        out = []
        for nom, t in self.declaire.items():
            if nom in exclus:
                continue
            ty = self._type_zig(t)
            defaut = "0.0" if ty == "f64" else ("false" if ty == "bool" else "0")
            out.append(f"{pad}var {self._nom(nom)}: {ty} = {defaut};")
        return out

    def _externe(self, sig: Signature):
        zs = {T_ENTIER: "i64", T_FLOTTANT: "f64", T_BOOLEEN: "c_int", T_CHAINE: "?[*:0]const u8"}
        if sig.retour == T_CHAINE or any(t == T_CHAINE for (_, t) in sig.params):
            raise ErreurRose("la cible Zig ne passe pas de chaînes aux fonctions externes "
                             "(utilise la cible C)")
        params = ", ".join(f"{self._nom(p)}: {zs[t]}" for (p, t) in sig.params)
        symbole = sig.symbole or sig.nom
        self.externes.append(f"extern fn {symbole}({params}) {zs[sig.retour]};")
        if sig.dll and sig.dll not in ("c", "m"):
            self.bibliotheques.add(sig.dll)

    def _fonction(self, sig: Signature) -> str:
        self.lignes, self.declaire = [], {}
        self.fonction = sig
        for instr in sig.corps.instructions:
            self._instr(instr, 1)
        params = ", ".join(f"p_{self._nom(p)}: {self._type_zig(t)}" for (p, t) in sig.params)
        ret = self._type_zig(sig.retour) if sig.retour != T_VIDE else "void"
        # les paramètres sont immuables en Zig : on les recopie dans des variables locales
        recopies = [f"    var {self._nom(p)} = p_{self._nom(p)};" for (p, _) in sig.params]
        pre = self._pre_declarations(1, {p for (p, _) in sig.params})
        return (f"fn {self._nom(sig.nom)}({params}) {ret} {{\n"
                + "\n".join(recopies + pre + self.lignes)
                + ("\n" if ret != "void" else "") + "}")

    def _instr(self, instr, ind: int):
        pad = "    " * ind
        if isinstance(instr, Afficher):
            t = instr.expression.type
            ref = self._expr(instr.expression)
            if t == T_FLOTTANT:
                self.lignes.append(f"{pad}afficherF64({ref});")
            elif t == T_BOOLEEN:
                self.lignes.append(f"{pad}afficherBooleen({ref});")
            else:
                self.lignes.append(f"{pad}afficherI64({ref});")
        elif isinstance(instr, Lire):
            nom = self._nom(instr.nom)
            t = getattr(instr, "type_read", T_ENTIER)
            self.declaire.setdefault(instr.nom, t)
            if t == T_FLOTTANT:
                self.lignes.append(f"{pad}{nom} = roseLireFlottant();")
            else:
                self.lignes.append(f"{pad}{nom} = roseLireEntier();")
        elif isinstance(instr, Affectation):
            if instr.nom not in self.declaire:
                self.declaire[instr.nom] = instr.valeur.type
            self.lignes.append(f"{pad}{self._nom(instr.nom)} = {self._expr(instr.valeur)};")
        elif isinstance(instr, ExprStatement):
            self.lignes.append(f"{pad}_ = {self._expr(instr.expression)};")
        elif isinstance(instr, Si):
            self.lignes.append(f"{pad}if ({self._expr(instr.test)}) {{")
            for i in instr.alors.instructions:
                self._instr(i, ind + 1)
            if instr.sinon is not None:
                self.lignes.append(f"{pad}}} else {{")
                for i in instr.sinon.instructions:
                    self._instr(i, ind + 1)
            self.lignes.append(f"{pad}}}")
        elif isinstance(instr, TantQue):
            self.lignes.append(f"{pad}while ({self._expr(instr.test)}) {{")
            for i in instr.corps.instructions:
                self._instr(i, ind + 1)
            self.lignes.append(f"{pad}}}")
        elif isinstance(instr, Pour):
            var = self._nom(instr.variable)
            self.declaire.setdefault(instr.variable, T_ENTIER)
            self.lignes.append(f"{pad}{var} = {self._expr(instr.debut)};")
            self.lignes.append(f"{pad}while ({var} < {self._expr(instr.fin)}) : ({var} += 1) {{")
            for i in instr.corps.instructions:
                self._instr(i, ind + 1)
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
            raise ErreurRose(f"instruction non supportée en Zig : {type(instr).__name__}")

    def _expr(self, e) -> str:
        if isinstance(e, Litteral):
            if e.flottant:
                v = float(e.texte)
                return f"{v!r}"
            if "/" in e.texte:
                raise ErreurRose("valeur rationnelle : la cible Zig (i64) est à précision "
                                 "machine (utilise les cibles C, Python ou Julia)")
            v = int(e.texte)
            if not (I64_MIN <= v <= I64_MAX):
                raise ErreurRose(f"valeur {v} hors i64 : la cible Zig est à précision machine "
                                 f"(utilise les cibles C, Python ou Julia)")
            return str(v)
        if isinstance(e, ChaineLit):
            raise ErreurRose("chaînes non supportées par la cible Zig (utilise C, Python ou Julia)")
        if isinstance(e, BooleenLit):
            return "true" if e.valeur else "false"
        if isinstance(e, Var):
            return self._nom(e.nom)
        if isinstance(e, (ListeLit, Index)):
            raise ErreurRose("listes non supportées par la cible Zig (utilise C, Python ou Julia)")
        if isinstance(e, Unaire):
            if e.op == "non":
                return f"(!({self._expr(e.operande)}))"
            return f"(-({self._expr(e.operande)}))"
        if isinstance(e, Factorielle):
            return f"facteur({self._expr(e.operande)})"
        if isinstance(e, Binaire):
            return self._binaire(e)
        if isinstance(e, Appel):
            return self._appel(e)
        raise ErreurRose(f"expression non supportée en Zig : {type(e).__name__}")

    def _flottant(self, e: Binaire) -> bool:
        return e.gauche.type == T_FLOTTANT or e.droite.type == T_FLOTTANT or e.type == T_FLOTTANT

    def _binaire(self, e: Binaire) -> str:
        op = e.op
        g, d = self._expr(e.gauche), self._expr(e.droite)
        floaty = self._flottant(e)
        if floaty:
            # conversion des opérandes non flottantes en f64
            gc = g if (isinstance(e.gauche, Litteral) and e.gauche.flottant) or e.gauche.type == T_FLOTTANT else f"fl({g})"
            dc = d if (isinstance(e.droite, Litteral) and e.droite.flottant) or e.droite.type == T_FLOTTANT else f"fl({d})"
            if op in ("+", "-", "*"):
                return f"({gc} {op} {dc})"
            if op == "/":
                return f"dive({gc}, {dc})"
            if op == "div":
                return f"divf({gc}, {dc})"
            if op == "^":
                return f"std.math.pow(f64, {gc}, {dc})"
            if op in ("≈", "?=", "=~"):
                return f"approche({gc}, {dc})"
            if op in ("<", "<=", ">", ">=", "==", "!="):
                return f"({gc} {op} {dc})"
        if op == "et":
            return f"({g} and {d})"
        if op == "ou":
            return f"({g} or {d})"
        if op == "/" or op == "div":
            return f"divI({g}, {d})"
        if op == "%":
            return f"modI({g}, {d})"
        if op == "^":
            return f"powrBase({g}, {d})"
        if op in ("≈", "?=", "=~"):
            return f"({g} == {d})"
        return f"({g} {op} {d})"

    def _appel(self, e: Appel) -> str:
        if e.module == "dynamique":
            raise ErreurRose("FFI dynamique non supportée par la cible Zig (utilise la cible C)")
        args = [self._expr(a) for a in e.args]
        sig = self.prog.signatures.get(e.nom)
        if sig is not None:
            return f"{self._nom(e.nom)}({', '.join(args)})"
        if e.nom == "abs":
            return f"@abs({args[0]})"
        if e.nom == "min":
            return f"@min({args[0]}, {args[1]})"
        if e.nom == "max":
            return f"@max({args[0]}, {args[1]})"
        if e.nom == "puissance":
            if e.args[0].type == T_FLOTTANT:
                return f"std.math.pow(f64, {args[0]}, @floatFromInt({args[1]}))"
            return f"powrBase({args[0]}, {args[1]})"
        if e.nom == "partie_entiere":
            if e.args[0].type == T_FLOTTANT:
                return f"@as(i64, @floor({args[0]}))"
            return args[0]
        if e.nom == "flottant":
            return f"@as(f64, {args[0]})" if e.args[0].type == T_ENTIER else args[0]
        if e.nom == "entier":
            return f"@as(i64, @floor({args[0]}))" if e.args[0].type == T_FLOTTANT else args[0]
        if e.nom == "afficher":
            t = e.args[0].type
            return f"afficherF64({args[0]})" if t == T_FLOTTANT else (
                f"afficherBooleen({args[0]})" if t == T_BOOLEEN else f"afficherI64({args[0]})")
        table = {"factorielle": "facteur", "fibonacci": "fibo", "pgcd": "pgcd", "ppcm": "ppcm",
                 "est_premier": "estPremier", "compter_premiers": "compterPremiers",
                 "nb_chiffres": "nbChiffres"}
        fn = table.get(e.nom)
        if fn is None:
            raise ErreurRose(f"fonction {e.nom!r} non supportée par la cible Zig")
        return f"{fn}({', '.join(args)})"


def generer_zig(prog: Programme) -> str:
    return CodegenZig(prog).generer()
