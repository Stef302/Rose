"""Rose v2 — transpilateur Julia (BigInt / Rational{BigInt} / Float64)."""
from __future__ import annotations
from .parser import (Programme, Signature, TypeRose, Litteral, ChaineLit, BooleenLit,
                     ListeLit, Var, Affectation, Binaire, Unaire, Factorielle, Appel,
                     Index, Bloc, Si, TantQue, Pour, Retour, Afficher, Lire,
                     ExprStatement, T_ENTIER, T_RATIONNEL, T_FLOTTANT, T_BOOLEEN,
                     T_CHAINE, T_VIDE)
from .lexer import ErreurRose

PRELUDE = r"""# Généré par le compilateur Rose v2 — cible Julia 1.10.
import Libdl

rose_erreur(msg) = (println(stderr, "Rose : erreur — ", msg); exit(1))

_fact(n::Integer) = n < 0 ? rose_erreur("factorielle d'un négatif : impossible") :
    n <= 1 ? big(1) : _arbre(BigInt(2), BigInt(n))
function _arbre(i, j)
    i > j && return BigInt(1)
    i == j && return i
    m = div(i + j, 2)
    return _arbre(i, m) * _arbre(m + 1, j)
end

function _fib(n::Integer)
    n < 0 && rose_erreur("fibonacci négatif : impossible")
    a, b = BigInt(0), BigInt(1)
    for c in string(BigInt(n), base = 2)
        a, b = a * (2b - a), a * a + b * b
        if c == '1'; a, b = b, a + b; end
    end
    return a
end

_pgcd(a, b) = gcd(BigInt(a), BigInt(b))
_ppcm(a, b) = lcm(BigInt(a), BigInt(b))

_mulmod(a, b, m) = mod(BigInt(a) * BigInt(b), m)
_powmod(b0, e0, m) = powermod(BigInt(b0), BigInt(e0), BigInt(m))
function _prem(n::Integer)
    ni = BigInt(n)
    ni < 2 && return false
    for p in (2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37)
        if mod(ni, p) == 0; return ni == p; end
    end
    d = ni - 1; r = 0
    while mod(d, 2) == 0; d = div(d, 2); r += 1; end
    for a in (2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37)
        x = _powmod(a, d, ni)
        (x == 1 || x == ni - 1) && continue
        temoin = false
        for _ in 1:(r - 1)
            x = _mulmod(x, x, ni)
            if x == ni - 1; temoin = true; break; end
        end
        temoin || return false
    end
    return true
end
_prem(n) = _prem(BigInt(n))

function _div(a, b)
    b == 0 && rose_erreur("division par zéro")
    if a isa Integer && b isa Integer
        return Rational{BigInt}(BigInt(a), BigInt(b))
    end
    return float(a) / float(b)
end
_mod(a, b) = (b == 0 ? rose_erreur("modulo par zéro") : mod(a, b))

function _divf(a, b)
    b == 0 && rose_erreur("division euclidienne par zéro")
    (a isa AbstractFloat || b isa AbstractFloat) && return Float64(floor(a / b))
    qa = a isa Rational ? Rational{BigInt}(numerator(a), denominator(a)) : Rational{BigInt}(BigInt(a), 1)
    qb = b isa Rational ? Rational{BigInt}(numerator(b), denominator(b)) : Rational{BigInt}(BigInt(b), 1)
    q = qa / qb                                    # Rationnel{BigInt} exact
    return fld(numerator(q), denominator(q))
end

function _powr(a, e)
    e isa AbstractFloat && return float(a) ^ float(e)
    if e isa Rational && denominator(e) != 1
        return float(a) ^ float(e)
    end
    ei = Int(e)
    if a isa AbstractFloat
        return a ^ ei
    end
    if ei < 0
        a == 0 && rose_erreur("0^exposant négatif")
        return Rational{BigInt}(BigInt(a))^ei
    end
    if a isa Integer
        return BigInt(a)^ei
    end
    return a^ei
end

_approche(a, b) = (a isa AbstractFloat || b isa AbstractFloat) ?
    abs(a - b) <= 1e-9 * max(abs(a), abs(b), 1.0) : a == b

function _compter_premiers(n::Integer)
    n < 2 && return BigInt(0)
    n > typemax(Int) - 1 && rose_erreur("crible trop grand")
    ni = Int(n)
    crible = falses(ni + 1)
    compte = BigInt(0)
    for i in 2:ni
        if !crible[i]
            compte += 1
            i <= sqrt(ni) && (crible[i*i:i:ni] .= true)
        end
    end
    return compte
end

function _pi(chiffres::Integer)
    (chiffres <= 0 || chiffres > 2000000) && rose_erreur("décimales de π hors limites (1..2000000)")
    S = Int(chiffres) + 40
    E = big(10)^S
    function arctan_rec(x)
        x2 = BigInt(x)^2
        p = BigInt(x)
        terme = div(E, p)
        s = terme
        k, signe = 1, -1
        while true
            p *= x2
            terme = div(E, p * (2k + 1))
            terme == 0 && break
            s = signe < 0 ? s - terme : s + terme
            signe = -signe
            k += 1
        end
        return s
    end
    piE = 16 * arctan_rec(5) - 4 * arctan_rec(239)
    brut = string(piE)
    dec = brut[2:min(end, 1 + Int(chiffres))]
    arrondir = length(brut) > 1 + Int(chiffres) && brut[2 + Int(chiffres)] >= '5'
    entier = parse(BigInt, string(brut[1]))
    if arrondir
        d = parse(BigInt, dec) + 1
        ds = string(d)
        if length(ds) > length(dec); entier += 1; ds = "0"; end
        dec = lpad(ds, length(dec), '0')
    end
    return "$(entier).$(dec)"
end

_vers_entier(x::AbstractString) = (occursin("/", x) ?
    div(parse(BigInt, split(x, "/")[1]), parse(BigInt, split(x, "/")[2])) :
    occursin(".", x) ? BigInt(trunc(parse(Float64, x))) : parse(BigInt, x))
_vers_entier(x::AbstractFloat) = BigInt(trunc(x))
_vers_entier(x::Rational) = BigInt(div(numerator(x), denominator(x)))
_vers_entier(x::Integer) = BigInt(x)

_vers_flottant(x::AbstractString) = occursin("/", x) ?
    parse(BigInt, split(x, "/")[1]) / parse(BigInt, split(x, "/")[2]) : parse(Float64, x)
_vers_flottant(x) = Float64(x)

_vers_q(x::AbstractString) = occursin("/", x) ?
    Rational{BigInt}(parse(BigInt, split(x, "/")[1]), parse(BigInt, split(x, "/")[2])) :
    Rational{BigInt}(BigInt(trunc(parse(Float64, x) * 10^15), 10^15))
_vers_q(x::Real) = Rational{BigInt}(BigInt(x), 1)
_vers_q(x::Rational) = x

_rose_str(x::Bool) = x ? "vrai" : "faux"
function _rose_str(x::AbstractFloat)
    isnan(x) && return "non_nombre"
    isinf(x) && return x < 0 ? "-infini" : "infini"
    (x == floor(x) && abs(x) < 1e16) && return string(Int(x))
    return string(x)
end
_rose_str(x::Rational) = _periode(numerator(x), denominator(x))
_rose_str(x::AbstractVector) = "[" * join((_rose_str(e) for e in x), ", ") * "]"
_chaine(x::Rational) = denominator(x) == 1 ? string(numerator(x)) : "$(numerator(x))/$(denominator(x))"
_chaine(x) = _rose_str(x)
_rose_str(x) = string(x)
_afficher(x) = println(_rose_str(x))

function _periode(n::BigInt, d::BigInt)
    d == 1 && return string(n)
    if length(string(abs(n))) + length(string(d)) > 500; return "$(n)/$(d)"; end
    entier = div(n, d)
    reste = mod(n, d)
    reste == 0 && return string(entier)
    digits = Char[]
    vus = Dict{BigInt, Int}()
    peri = -1
    k = 0
    while reste != 0 && k < 200
        if haskey(vus, reste); peri = vus[reste]; break; end
        vus[reste] = k
        reste *= 10
        push!(digits, '0' + div(reste, d))
        reste = mod(reste, d)
        k += 1
    end
    s = string(entier) * "." * join(digits[1:(peri >= 0 ? peri : k)])
    peri >= 0 && (s *= "[" * join(digits[(peri + 1):end]) * "]")
    (peri < 0 && k >= 200) && (s *= "…")
    return s
end

function _lire()
    t = readline(stdin)
    eof(stdin) && t == "" && rose_erreur("entrée attendue, fichier fermé")
    t = strip(t)
    occursin("/", t) && return Rational{BigInt}(parse(BigInt, split(t, "/")[1]), parse(BigInt, split(t, "/")[2]))
    occursin(".", t) && return parse(Float64, t)
    return parse(BigInt, t)
end
_saisir_entier() = _vers_entier(_lire())
_saisir_flottant() = _vers_flottant(_lire())

_idx(l, i) = (i < 0 && rose_erreur("index hors des limites de la liste");
    i >= length(l) && rose_erreur("index hors des limites de la liste"); l[Int(i) + 1])
_partie_entiere(x::Integer) = BigInt(x)
_partie_entiere(x::Rational) = fld(BigInt(numerator(x)), BigInt(denominator(x)))
_partie_entiere(x::AbstractFloat) = BigInt(floor(x))
_nb_chiffres(n::Integer) = n == 0 ? BigInt(1) : BigInt(length(string(n < 0 ? -n : n)))
_nb_chiffres(n) = _nb_chiffres(BigInt(n))

function _charger_dll(nom)
    extensions = any(s -> endswith(nom, s), (".so", ".dll", ".dylib")) || occursin("/", nom)
    candidats = extensions ? [nom] :
        ["lib$(nom).so", "$(nom).so", nom, "lib$(nom).so.6", "lib$(nom).so.7", "lib$(nom).so.8"]
    for c in candidats
        h = Libdl.dlopen(c, Libdl.RTLD_LAZY; throw_error=false)
        h !== nothing && return h
    end
    rose_erreur("chargement de bibliothèque impossible ($(nom))")
end

function _symbole(h, nom)
    p = Libdl.dlsym(h, String(nom); throw_error=false)
    p === C_NULL && rose_erreur("symbole introuvable dans la bibliothèque")
    return p
end

function _appeler(h, nom, args...)
    p = Libdl.dlsym(h, String(nom); throw_error=false)
    p === C_NULL && rose_erreur("symbole introuvable dans la bibliothèque")
    if length(args) == 0
        return ccall(p, Cdouble, ())
    elseif length(args) == 1
        return ccall(p, Cdouble, (Cdouble,), Float64(args[1]))
    elseif length(args) == 2
        return ccall(p, Cdouble, (Cdouble, Cdouble), Float64(args[1]), Float64(args[2]))
    else
        rose_erreur("trop d'arguments pour l'appel dynamique")
    end
end
"""


def _val_litt(e: Litteral) -> str:
    if e.flottant:
        return repr(float(e.texte))
    if "/" in e.texte:
        n, d = e.texte.split("/")
        return f"Rational{{BigInt}}({n}, {d})"
    return f"BigInt({e.texte})"


class CodegenJulia:
    def __init__(self, prog: Programme):
        self.prog = prog
        self.lignes: list[str] = []
        self.bibliotheques: set[str] = set()

    def generer(self) -> str:
        corps: list[str] = [PRELUDE]
        for sig in self.prog.signatures.values():
            if sig.corps is None:
                corps.append(self._externe(sig))
        for sig in self.prog.signatures.values():
            if sig.corps is not None:
                corps.append(self._fonction(sig))
        self.lignes = []
        for instr in self.prog.instructions:
            self._instr(instr, 0)
        corps.append("# --- programme principal ---")
        corps.append("let")
        corps.extend("    " + l for l in self.lignes)
        corps.append("end")
        return "\n".join(corps) + "\n"

    def _nom(self, nom: str) -> str:
        return "rose_v_" + "".join(
            c if (c.isascii() and c.isalnum()) or c == "_" else f"_x{ord(c):x}_" for c in nom)

    def _externe(self, sig: Signature) -> str:
        jl_arg = {T_ENTIER: "Clong", T_FLOTTANT: "Cdouble", T_BOOLEEN: "Cint", T_CHAINE: "Cstring"}
        jl_ret = {T_ENTIER: "Clong", T_FLOTTANT: "Cdouble", T_BOOLEEN: "Cint", T_CHAINE: "Cstring"}
        dll = sig.dll_son or sig.dll or "c"
        self.bibliotheques.add(sig.dll_son or sig.dll or "c")
        symbole = sig.symbole or sig.nom
        fparams = ", ".join(f"{self._nom(p)}::Real" if t in (T_ENTIER, T_FLOTTANT, T_BOOLEEN)
                            else f"{self._nom(p)}::String" for (p, t) in sig.params)
        conversion = {T_ENTIER: "Clong({n})", T_FLOTTANT: "Cdouble({n})",
                      T_BOOLEEN: "Cint({n} ? 1 : 0)", T_CHAINE: "Cstring({n})"}
        appelle = ", ".join(conversion[t].format(n=self._nom(p)) for (p, t) in sig.params)
        types_args = "(" + "".join(jl_arg[t] + ", " for (_, t) in sig.params) + ")"
        ccall = (f"ccall(_symbole(handle_{self._nom(sig.nom)}, \"{symbole}\"), "
                 f"{jl_ret[sig.retour]}, {types_args}, {appelle})")
        if sig.retour == T_ENTIER:
            ccall = f"BigInt({ccall})"
        elif sig.retour == T_BOOLEEN:
            ccall = f"({ccall}) != 0"
        elif sig.retour == T_CHAINE:
            ccall = f"String(unsafe_string({ccall}))"
        return (f"const handle_{self._nom(sig.nom)} = _charger_dll(\"{dll}\")\n"
                f"function {self._nom(sig.nom)}({fparams})\n"
                f"    return {ccall}\nend")

    def _fonction(self, sig: Signature) -> str:
        params = ", ".join(self._nom(p) for (p, _) in sig.params)
        self.lignes = []
        for instr in sig.corps.instructions:
            self._instr(instr, 1)
        if not any(l.strip().startswith("return") for l in self.lignes):
            self.lignes.append("    nothing")
        return f"function {self._nom(sig.nom)}({params})\n" + "\n".join(self.lignes) + "\nend"

    def _instr(self, instr, ind: int):
        pad = "    " * ind
        if isinstance(instr, Afficher):
            self.lignes.append(f"{pad}println(_rose_str({self._expr(instr.expression)}))")
        elif isinstance(instr, Lire):
            t = getattr(instr, "type_read", T_ENTIER)
            lecteur = {T_FLOTTANT: "_saisir_flottant()", T_RATIONNEL: "_vers_q(_lire())"}.get(
                t, "_saisir_entier()")
            self.lignes.append(f"{pad}{self._nom(instr.nom)} = {lecteur}")
        elif isinstance(instr, Affectation):
            self.lignes.append(f"{pad}{self._nom(instr.nom)} = {self._expr(instr.valeur)}")
        elif isinstance(instr, ExprStatement):
            self.lignes.append(f"{pad}(_ = {self._expr(instr.expression)}); nothing")
        elif isinstance(instr, Si):
            self.lignes.append(f"{pad}if {self._expr(instr.test)}")
            for i in instr.alors.instructions:
                self._instr(i, ind + 1)
            if instr.sinon is not None:
                self.lignes.append(f"{pad}else")
                for i in instr.sinon.instructions:
                    self._instr(i, ind + 1)
            self.lignes.append(f"{pad}end")
        elif isinstance(instr, TantQue):
            self.lignes.append(f"{pad}while {self._expr(instr.test)}")
            for i in instr.corps.instructions:
                self._instr(i, ind + 1)
            self.lignes.append(f"{pad}end")
        elif isinstance(instr, Pour):
            var = self._nom(instr.variable)
            self.lignes.append(f"{pad}{var} = {self._expr(instr.debut)}")
            self.lignes.append(f"{pad}while {var} < {self._expr(instr.fin)}")
            for i in instr.corps.instructions:
                self._instr(i, ind + 1)
            self.lignes.append(f"{pad}    {var} += 1")
            self.lignes.append(f"{pad}end")
        elif isinstance(instr, Retour):
            self.lignes.append(f"{pad}return nothing" if instr.valeur is None
                               else f"{pad}return {self._expr(instr.valeur)}")
        elif isinstance(instr, Bloc):
            for i in instr.instructions:
                self._instr(i, ind)
        else:
            raise ErreurRose(f"instruction non supportée en Julia : {type(instr).__name__}")

    def _lignes_noms(self, v):
        return set()

    def _expr(self, e) -> str:
        if isinstance(e, Litteral):
            return _val_litt(e)
        if isinstance(e, ChaineLit):
            return '"' + e.valeur.replace("\\", "\\\\").replace('"', "\\\"") + '"'
        if isinstance(e, BooleenLit):
            return "true" if e.valeur else "false"
        if isinstance(e, Var):
            return self._nom(e.nom)
        if isinstance(e, ListeLit):
            return "[" + ", ".join(self._expr(x) for x in e.elements) + "]"
        if isinstance(e, Index):
            return f"_idx({self._expr(e.base)}, {self._expr(e.index)})"
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
        raise ErreurRose(f"expression non supportée en Julia : {type(e).__name__}")

    def _binaire(self, e: Binaire) -> str:
        op = e.op
        g, d = self._expr(e.gauche), self._expr(e.droite)
        if op == "et":
            return f"({g} && {d})"
        if op == "ou":
            return f"({g} || {d})"
        if op == "/":
            return f"_div({g}, {d})"
        if op == "%":
            return f"_mod({g}, {d})"
        if op == "div":
            return f"_divf({g}, {d})"
        if op == "^":
            return f"_powr({g}, {d})"
        if op in ("≈", "?=", "=~"):
            return f"_approche({g}, {d})"
        if op == "+":
            if e.gauche.type == T_CHAINE and e.droite.type == T_CHAINE:
                return f"({g} * {d})"
            return f"({g} + {d})"
        return f"({g} {op} {d})"

    def _appel(self, e: Appel) -> str:
        args = [self._expr(a) for a in e.args]
        if e.module == "dynamique":
            if e.nom == "charger":
                return f"_charger_dll({args[0]})"
            if e.nom == "fermer":
                return "nothing"
            return f"_appeler({args[0]}, {args[1]}, {', '.join(args[2:])})"
        nom = e.nom
        if nom in self.prog.signatures:
            return f"{self._nom(nom)}({', '.join(args)})"
        table = {"factorielle": "_fact", "fibonacci": "_fib", "pgcd": "_pgcd", "ppcm": "_ppcm",
                 "est_premier": "_prem", "compter_premiers": "_compter_premiers",
                 "nb_chiffres": "_nb_chiffres",
                 "abs": "abs", "min": "min", "max": "max", "nombre_pi": "_pi",
                 "saisir_entier": "_saisir_entier", "saisir_flottant": "_saisir_flottant",
                 "partie_entiere": "_partie_entiere", "entier": "_vers_entier",
                 "flottant": "_vers_flottant", "rationnel": "_vers_q", "chaine": "_chaine",
                 "afficher": "_afficher", "longueur": "length"}
        if nom == "joindre":
            return f"push!({args[0]}, {args[1]})"
        if nom == "fraction":
            return f"Rational{{BigInt}}({args[0]}, {args[1]})"
        if nom == "puissance":
            return f"_powr({args[0]}, {args[1]})"
        fn = table.get(nom)
        if fn is None:
            raise ErreurRose(f"fonction {nom!r} inconnue en génération Julia")
        return f"{fn}({', '.join(args)})"


def generer_julia(prog: Programme) -> str:
    return CodegenJulia(prog).generer()
