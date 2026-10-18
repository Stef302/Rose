"""Rose v2 — transpilateur Python 3 (entiers natifs + fractions exactes)."""
from __future__ import annotations
from .parser import (Programme, Signature, TypeRose, Litteral, ChaineLit, BooleenLit,
                     ListeLit, Var, Affectation, Binaire, Unaire, Factorielle, Appel,
                     Index, Bloc, Si, TantQue, Pour, Retour, Afficher, Lire,
                     ExprStatement, T_ENTIER, T_RATIONNEL, T_FLOTTANT, T_BOOLEEN,
                     T_CHAINE, T_VIDE)
from .lexer import ErreurRose
from .checker import promotion

PRELUDE = r'''# Généré par le compilateur Rose v2 — cible Python 3.
from fractions import Fraction
import ctypes, math, sys
try: sys.set_int_max_str_digits(200000000)
except Exception: pass

def rose_erreur(msg):
    print("Rose : erreur —", msg, file=sys.stderr); sys.exit(1)

def _fact(n):
    if n < 0: rose_erreur("factorielle d'un négatif : impossible")
    def arbre(i, j):
        if i > j: return 1
        if i == j: return i
        m = (i + j) // 2
        return arbre(i, m) * arbre(m + 1, j)
    return arbre(2, n) if n >= 2 else 1

def _fib(n):
    if n < 0: rose_erreur("fibonacci négatif : impossible")
    a, b = 0, 1
    for bit in bin(n)[2:]:
        a, b = a * (2 * b - a), a * a + b * b
        if bit == "1": a, b = b, a + b
    return a

def _pgcd(a, b): return math.gcd(int(a), int(b))
def _ppcm(a, b):
    a, b = int(a), int(b)
    return 0 if a == 0 or b == 0 else abs(a * b) // math.gcd(a, b)

def _div(a, b):
    if b == 0: rose_erreur("division par zéro")
    if isinstance(a, int) and isinstance(b, int): return Fraction(a, b)
    return a / b

def _mod(a, b):
    if b == 0: rose_erreur("modulo par zéro")
    return a % b

def _divf(a, b):
    if b == 0: rose_erreur("division euclidienne par zéro")
    if isinstance(a, float) or isinstance(b, float):
        return float(math.floor(a / b))
    q = Fraction(a) / Fraction(b)
    return q.numerator // q.denominator

def _powr(a, e):
    if isinstance(e, Fraction) and e.denominator != 1:
        if isinstance(a, int): return float(a) ** float(e)
        return a ** e
    e = int(e)
    if isinstance(a, float): return float(a) ** e
    if e < 0:
        if a == 0: rose_erreur("0^exposant négatif")
        return Fraction(a) ** e
    return a ** e

def _approxe(a, b):
    if isinstance(a, float) or isinstance(b, float):
        return abs(a - b) <= 1e-9 * max(abs(a), abs(b), 1.0)
    return a == b

def _prem(n):
    n = int(n)
    if n < 2:
        return False
    for p in (2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37):
        if n % p == 0:
            return n == p
    d, r = n - 1, 0
    while d % 2 == 0:
        d //= 2
        r += 1
    for a in (2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37):
        x = pow(a, d, n)
        if x in (1, n - 1):
            continue
        for _ in range(r - 1):
            x = x * x % n
            if x == n - 1:
                break
        else:
            return False
    return True

def _compter_premiers(n):
    n = int(n)
    if n < 2: return 0
    crible = bytearray(n + 1)
    compte = 0
    for i in range(2, n + 1):
        if not crible[i]:
            compte += 1
            if i * i <= n: crible[i*i::i] = b"\x01" * len(range(i*i, n+1, i))
    return compte

def _pi(chiffres):
    chiffres = int(chiffres)
    if chiffres <= 0 or chiffres > 2000000:
        rose_erreur("décimales de π hors limites (1..2000000)")
    S = chiffres + 40
    E = 10 ** S
    def arctan_rec(x):
        x2 = x * x
        p, terme = x, E // x
        s = terme
        k, signe = 1, -1
        while True:
            p *= x2
            terme = E // (p * (2 * k + 1))
            if terme == 0: break
            s = s - terme if signe < 0 else s + terme
            signe = -signe
            k += 1
        return s
    piE = 16 * arctan_rec(5) - 4 * arctan_rec(239)
    brut = str(piE)
    dec = brut[1:1 + chiffres]
    arrondi = len(brut) > 1 + chiffres and brut[1 + chiffres] >= "5"
    entier = int(brut[0])
    if arrondi:
        decI = int(dec) + 1 if chiffres else 0
        if len(str(decI)) > chiffres: entier += 1; decI = 0
        dec = str(decI).rjust(chiffres, "0") if chiffres else ""
    return f"{entier}.{dec}" if dec else str(entier)

def _chaine_vers_entier(s):
    s = s.strip()
    if "/" in s:
        n, d = s.split("/", 1)
        if int(d) == 0: rose_erreur("fraction de dénominateur nul")
        return int(Fraction(int(n), int(d)))
    if "." in s: return int(float(s))
    return int(s)

def _chaine_vers_flottant(s):
    s = s.strip()
    if "/" in s:
        n, d = s.split("/", 1)
        return int(n) / int(d)
    return float(s)

def _vers_entier(x):
    if isinstance(x, str): return _chaine_vers_entier(x)
    if isinstance(x, float): return int(math.trunc(x))
    if isinstance(x, Fraction): return math.trunc(x)
    return int(x)

def _vers_flottant(x):
    if isinstance(x, str): return _chaine_vers_flottant(x)
    return float(x)

def _vers_q(x):
    if isinstance(x, str): return Fraction(x)
    return Fraction(x)

def _rose_str(x):
    if isinstance(x, bool): return "vrai" if x else "faux"
    if isinstance(x, Fraction):
        return str(x.numerator) if x.denominator == 1 else _decimal_periode(x.numerator, x.denominator)
    if isinstance(x, float):
        if x == int(x) and abs(x) < 1e16: return str(int(x))
        return repr(x)
    if isinstance(x, list): return "[" + ", ".join(_rose_str(e) for e in x) + "]"
    return str(x)

def _decimal_periode(n, d):
    """Développement décimal exact avec période, comme la cible C."""
    if d == 1: return str(n)
    if len(str(abs(n))) + len(str(d)) > 500: return f"{n}/{d}"
    entier = n // d
    reste = abs(n) % d
    if reste == 0: return str(entier)
    vus = {}
    chiffres = []
    peri = -1
    k = 0
    while reste and k < 200:
        if reste in vus:
            peri = vus[reste]
            break
        vus[reste] = k
        reste *= 10
        chiffres.append(chr(48 + reste // d))
        reste %= d
        k += 1
    s = str(entier) + "." + "".join(chiffres[:peri] if peri >= 0 else chiffres)
    if peri >= 0:
        s += "[" + "".join(chiffres[peri:]) + "]"
    elif k >= 200:
        s += "…"
    return s

def _chaine(x):
    if isinstance(x, Fraction):
        return str(x.numerator) if x.denominator == 1 else f"{x.numerator}/{x.denominator}"
    return _rose_str(x)

def _afficher(x):
    print(_rose_str(x))

def _lire():
    t = sys.stdin.readline()
    if not t: rose_erreur("entrée attendue, fichier fermé")
    t = t.strip()
    if "/" in t:
        n, d = t.split("/", 1)
        return Fraction(int(n), int(d))
    if "." in t or "e" in t: return float(t)
    return int(t)

def _saisir_entier(): return _vers_entier(_lire())
def _saisir_flottant(): return _vers_flottant(_lire())
def _saisir_rationnel(): return _vers_q(_lire())

def _abs(x): return abs(x)
def _partie_entiere(x):
    """⌊x⌋ — partie entière mathématique (floor), comme la cible C."""
    return math.floor(x)
def _nb_chiffres(n):
    n = int(n)
    return 1 if n == 0 else len(str(abs(n)))
def _idx(l, i):
    i = int(i)
    if not (0 <= i < len(l)): rose_erreur("index hors des limites de la liste")
    return l[i]

def _longueur(l): return len(l)

def _charger_dll(nom, fils):
    candidats = [nom] if (nom.endswith((".so", ".dll", ".dylib")) or "/" in nom) else \
        [f"lib{nom}.so", f"{nom}.so", f"{nom}.dylib", nom,
         f"lib{nom}.so.6", f"lib{nom}.so.7", f"lib{nom}.so.8"]
    for c in candidats:
        try: return ctypes.CDLL(c)
        except OSError: pass
    rose_erreur(f"chargement de bibliothèque impossible ({nom})")

def _charger_chemin(chemin): return _charger_dll(chemin, None)

def _appeler(h, nom, *args):
    f = getattr(h, nom, None)
    if f is None: rose_erreur("symbole introuvable dans la bibliothèque")
    f.argtypes = [ctypes.c_double] * len(args)
    f.restype = ctypes.c_double
    return f(*args)
'''


def _val_litt(e: Litteral):
    if e.flottant:
        return repr(float(e.texte))
    if "/" in e.texte:
        n, d = e.texte.split("/")
        return f"Fraction({n}, {d})"
    return e.texte


class CodegenPython:
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
        corps.extend(self.lignes)
        return "\n".join(corps) + "\n"

    def _nom(self, nom: str) -> str:
        return "rose_v_" + "".join(c if (c.isascii() and c.isalnum()) or c == "_"
                                   else f"_x{ord(c):x}_" for c in nom)

    def _externe(self, sig: Signature) -> str:
        entree_map = {T_ENTIER: "int", T_FLOTTANT: "float",
                      T_BOOLEEN: "(1 if v else 0)", T_CHAINE: None}
        argtypes = {T_ENTIER: "ctypes.c_longlong", T_FLOTTANT: "ctypes.c_double",
                    T_BOOLEEN: "ctypes.c_int", T_CHAINE: "ctypes.c_char_p"}
        dll = sig.dll or "c"
        self.bibliotheques.add(sig.dll_son or dll)
        symbole = sig.symbole or sig.nom
        fparams = ", ".join(self._nom(p) for (p, _) in sig.params)
        appelle = []
        for (p, t) in sig.params:
            if t == T_BOOLEEN:
                appelle.append(f"(1 if {self._nom(p)} else 0)")
            elif t == T_CHAINE:
                appelle.append(f"{self._nom(p)}.encode()")
            else:
                appelle.append(self._nom(p))
        lignes = [
            f"_dll_{self._nom(sig.nom)} = _charger_dll({dll!r}, {sig.dll_son!r})",
            f"def {self._nom(sig.nom)}({fparams}):",
            f"    _f = _dll_{self._nom(sig.nom)}.{symbole}",
            f"    _f.argtypes = [{', '.join(argtypes[t] for (_, t) in sig.params)}]",
            f"    _f.restype = {argtypes[sig.retour]}",
        ]
        appel = f"_f({', '.join(appelle)})"
        if sig.retour == T_CHAINE:
            lignes.append(f"    _r = {appel}")
            lignes.append("    return _r.decode() if _r is not None else ''")
        elif sig.retour == T_BOOLEEN:
            lignes.append(f"    return bool({appel})")
        else:
            lignes.append(f"    return {appel}")
        return "\n".join(lignes)

    def _fonction(self, sig: Signature) -> str:
        params = ", ".join(self._nom(p) for (p, _) in sig.params)
        self.lignes = []
        for instr in sig.corps.instructions:
            self._instr(instr, 1)
        if not any(l.strip().startswith("return") for l in self.lignes):
            self.lignes.append("    return None")
        return f"def {self._nom(sig.nom)}({params}):\n" + "\n".join(self.lignes)

    def _instr(self, instr, ind: int):
        pad = "    " * ind
        if isinstance(instr, Afficher):
            self.lignes.append(f"{pad}print(_rose_str({self._expr(instr.expression)}))")
        elif isinstance(instr, Lire):
            t = getattr(instr, "type_read", T_ENTIER)
            lecteur = {T_FLOTTANT: "_saisir_flottant()", T_RATIONNEL: "_saisir_rationnel()"}.get(
                t, "_saisir_entier()")
            self.lignes.append(f"{pad}{self._nom(instr.nom)} = {lecteur}")
        elif isinstance(instr, Affectation):
            self.lignes.append(f"{pad}{self._nom(instr.nom)} = {self._expr(instr.valeur)}")
        elif isinstance(instr, ExprStatement):
            self.lignes.append(f"{pad}{self._expr(instr.expression)}")
        elif isinstance(instr, Si):
            self.lignes.append(f"{pad}if {self._expr(instr.test)}:")
            for i in instr.alors.instructions:
                self._instr(i, ind + 1)
            if instr.sinon is not None:
                self.lignes.append(f"{pad}else:")
                for i in instr.sinon.instructions:
                    self._instr(i, ind + 1)
        elif isinstance(instr, TantQue):
            self.lignes.append(f"{pad}while {self._expr(instr.test)}:")
            for i in instr.corps.instructions:
                self._instr(i, ind + 1)
        elif isinstance(instr, Pour):
            self.lignes.append(f"{pad}{self._nom(instr.variable)} = {self._expr(instr.debut)}")
            self.lignes.append(f"{pad}while {self._nom(instr.variable)} < {self._expr(instr.fin)}:")
            for i in instr.corps.instructions:
                self._instr(i, ind + 1)
            self.lignes.append(f"{pad}    {self._nom(instr.variable)} += 1")
        elif isinstance(instr, Retour):
            self.lignes.append(f"{pad}return None" if instr.valeur is None
                               else f"{pad}return {self._expr(instr.valeur)}")
        elif isinstance(instr, Bloc):
            for i in instr.instructions:
                self._instr(i, ind)
        else:
            raise ErreurRose(f"instruction non supportée en Python : {type(instr).__name__}")

    def _expr(self, e) -> str:
        if isinstance(e, Litteral):
            return _val_litt(e)
        if isinstance(e, ChaineLit):
            return repr(e.valeur)
        if isinstance(e, BooleenLit):
            return "True" if e.valeur else "False"
        if isinstance(e, Var):
            return self._nom(e.nom)
        if isinstance(e, ListeLit):
            return "[" + ", ".join(self._expr(x) for x in e.elements) + "]"
        if isinstance(e, Index):
            return f"_idx({self._expr(e.base)}, {self._expr(e.index)})"
        if isinstance(e, Unaire):
            if e.op == "non":
                return f"(not {self._expr(e.operande)})"
            return f"(-{self._expr(e.operande)})"
        if isinstance(e, Factorielle):
            return f"_fact({self._expr(e.operande)})"
        if isinstance(e, Binaire):
            return self._binaire(e)
        if isinstance(e, Appel):
            return self._appel(e)
        raise ErreurRose(f"expression non supportée en Python : {type(e).__name__}")

    def _binaire(self, e: Binaire) -> str:
        op = e.op
        g, d = self._expr(e.gauche), self._expr(e.droite)
        if op == "et":
            return f"({g} and {d})"
        if op == "ou":
            return f"({g} or {d})"
        if op == "/":
            return f"_div({g}, {d})"
        if op == "%":
            return f"_mod({g}, {d})"
        if op == "div":
            return f"_divf({g}, {d})"
        if op == "^":
            return f"_powr({g}, {d})"
        if op in ("≈", "?=", "=~"):
            return f"_approxe({g}, {d})"
        if op == "!=":
            return f"({g} != {d})"
        if op == "==":
            return f"({g} == {d})"
        return f"({g} {op} {d})"

    def _appel(self, e: Appel) -> str:
        args = [self._expr(a) for a in e.args]
        if e.module == "dynamique":
            if e.nom == "charger":
                return f"_charger_chemin({args[0]})"
            if e.nom == "fermer":
                return "None"
            return f"_appeler({args[0]}, {args[1]}, {', '.join(args[2:])})"
        nom = e.nom
        if nom in self.prog.signatures:
            return f"{self._nom(nom)}({', '.join(args)})"
        table = {"factorielle": "_fact", "fibonacci": "_fib", "pgcd": "_pgcd", "ppcm": "_ppcm",
                 "puissance": "_powr", "est_premier": "_prem", "compter_premiers": "_compter_premiers",
                 "longueur": "_longueur", "abs": "_abs", "min": "min", "max": "max",
                 "nombre_pi": "_pi", "saisir_entier": "_saisir_entier", "saisir_flottant": "_saisir_flottant",
                 "nb_chiffres": "_nb_chiffres",
                 "partie_entiere": "_partie_entiere", "entier": "_vers_entier", "flottant": "_vers_flottant",
                 "rationnel": "_vers_q", "chaine": "_chaine", "afficher": "_afficher"}
        if nom == "joindre":
            return f"{args[0]}.append({args[1]})"
        if nom == "fraction":
            return f"Fraction({args[0]}, {args[1]})"
        if nom == "puissance":
            return f"_powr({args[0]}, {args[1]})"
        if nom == "est_premier":
            return f"_prem({args[0]})"
        fn = table.get(nom)
        if fn is None:
            raise ErreurRose(f"fonction {nom!r} inconnue en génération Python")
        return f"{fn}({', '.join(args)})"


def generer_python(prog: Programme) -> str:
    return CodegenPython(prog).generer()
