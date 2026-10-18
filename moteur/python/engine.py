#!/usr/bin/env python3
"""Rose — moteur en Python 3 (pipeline complet).

Réutilise le frontend officiel (lexer, parser, repli de constantes,
vérificateur de types — stdlib Python seule) et ÉVALUE l'arbre
directement : entiers Python arbitraires, Fraction exacte, float64.
Les helpers d'affichage et de calcul sont ceux des targets v2, mot pour
mot : la sortie est identique au caractère près aux autres moteurs.

Usage :  python3 engine.py prog.rose      (ou: run prog.rose)
         python3 engine.py version | info
"""
from __future__ import annotations
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                "..", "..", "rose2", "src"))
from fractions import Fraction
import ctypes, math
try:
    sys.set_int_max_str_digits(200000000)
except Exception:
    pass
sys.setrecursionlimit(2_000_000)

from rose2.parser import (analyser, Litteral, ChaineLit, BooleenLit, ListeLit, Var,
                          Affectation, Binaire, Unaire, Factorielle, Appel, Index,
                          Si, TantQue, Pour, Retour, Afficher, Lire, ExprStatement,
                          T_ENTIER, T_RATIONNEL, T_FLOTTANT)
from rose2.checker import Verificateur, replier_constants
from rose2.lexer import ErreurRose

# ── runtime (identique aux targets v2 — parité caractère à caractère) ──

PROG_ARGS: list = []   # argv vus par le programme Rose (v3)

class QuitterSignal(Exception):
    def __init__(self, code):
        super().__init__()
        self.code = code

def rose_erreur(msg):
    print("Rose : erreur —", msg, file=sys.stderr)
    sys.stdout.flush()
    sys.exit(1)

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

_POW = ctypes.CDLL(None).pow          # pow() de la libm du processus, comme le moteur C++
_POW.restype = ctypes.c_double
_POW.argtypes = [ctypes.c_double, ctypes.c_double]

def _fpow(a, e): return _POW(float(a), float(e))

def _powr(a, e):
    # miroir du moteur C++ : exposant flottant → pow ; base flottante → pow ;
    # sinon exposant entier/rationnel tronqué (vers_z = plancher) puis puissance exacte
    if isinstance(e, float): return _fpow(a, e)
    if isinstance(a, float): return _fpow(a, float(e))
    e = _vers_entier(e)
    if e > 2**63 - 1 or e < -2**63:
        rose_erreur("exposant démesuré (la puissance exploserait au-delà de tout calcul)")
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

def _entier_strict(s):
    t = s[1:] if s[:1] in "+-" else s
    if not t or not all("0" <= c <= "9" for c in t): raise ValueError(s)
    return int(s)

def _strtod_prefix(t):
    i = 1 if t[:1] in "+-" else 0
    j = i
    while j < len(t) and t[j].isdigit(): j += 1
    eu = j > i
    if j < len(t) and t[j] == ".":
        j += 1
        k = j
        while j < len(t) and t[j].isdigit(): j += 1
        eu = eu or j > k
    if not eu:
        low = t.lower()
        if low.startswith("inf"): return float("-inf") if t[:1] == "-" else float("inf")
        if low.startswith("nan"): return float("nan")
        if t == "infini": return float("inf")
        return None
    if j < len(t) and t[j] in "eE":
        m = j + 1
        if m < len(t) and t[m] in "+-": m += 1
        k = m
        while m < len(t) and t[m].isdigit(): m += 1
        if m > k: j = m
    try: return float(t[:j])
    except ValueError: return None

def _chaine_vers_entier(s):
    try: return _entier_strict(s)
    except ValueError: pass
    if "/" in s:
        n, d = s.split("/", 1)
        try:
            ni, di = _entier_strict(n), _entier_strict(d)
            if di != 0: return ni // di              # mpz_fdiv_q : plancher
        except ValueError: pass
        rose_erreur("conversion chaine → entier impossible")
    if "." in s:
        try: return _entier_strict(s.split(".", 1)[0])
        except ValueError: pass
    rose_erreur("conversion chaine → entier impossible")

def _chaine_vers_flottant(s):
    s = s.strip()
    if "/" in s:
        n, d = s.split("/", 1)
        return int(n) / int(d)
    return float(s)

def _vers_entier(x):
    if isinstance(x, str): return _chaine_vers_entier(x)
    if isinstance(x, float): return int(math.trunc(x))
    if isinstance(x, Fraction): return x.numerator // x.denominator
    return int(x)

def _conv_t(v, t):
    if t == "entier":
        return v if isinstance(v, int) else _vers_entier(v)
    if t == "rationnel":
        return v if isinstance(v, Fraction) else Fraction(v)
    if t == "flottant":
        return v if isinstance(v, float) else float(v)
    return v

def _vers_flottant(x):
    if isinstance(x, str): return _chaine_vers_flottant(x)
    return float(x)

def _vers_q(x):
    if isinstance(x, str): return Fraction(x)
    return Fraction(x)

def _rose_str(x):
    if isinstance(x, bool): return "vrai" if x else "faux"
    if isinstance(x, int): return str(x)
    if isinstance(x, Fraction):
        return str(x.numerator) if x.denominator == 1 else _decimal_periode(x.numerator, x.denominator)
    if isinstance(x, float):
        if x != x: return "non_nombre"
        if x in (float("inf"), float("-inf")): return "infini" if x > 0 else "-infini"
        if x == int(x) and abs(x) < 1e16: return str(int(x))
        return repr(x)
    if isinstance(x, list): return "[" + ", ".join(_rose_str(e) for e in x) + "]"
    if isinstance(x, Table): return "[" + ", ".join(_rose_str(k) + " => " + _rose_str(v) for k, v in x.items()) + "]"
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

class Table:
    __slots__ = ("_items",)
    def __init__(self):
        self._items = []
    def items(self):
        return self._items
    def __len__(self):
        return len(self._items)
    def __eq__(self, other):
        if not isinstance(other, Table): return False
        if len(self._items) != len(other._items): return False
        for k, v in self._items:
            if not any(k == k2 and v == v2 for k2, v2 in other._items): return False
        return True
    def __ne__(self, other):
        return not self.__eq__(other)

def _afficher(x):
    sys.stdout.flush()
    print(_rose_str(x))

def _lire():
    t = sys.stdin.readline()
    if not t: rose_erreur("entrée attendue, fichier fermé")
    return t.rstrip(" \r\n")

def _lire_entier():
    t = _lire()
    if "/" in t:
        n, d = t.split("/", 1)
        try: ni, di = _entier_strict(n), _entier_strict(d)
        except ValueError: rose_erreur("nombre entier invalide")
        if di == 0: rose_erreur("nombre entier invalide")
        return math.trunc(Fraction(ni, di))          # mpz_tdiv_q : troncature
    if "." in t:
        d = _strtod_prefix(t)
        if d is None: rose_erreur("nombre entier invalide")
        return int(math.trunc(d))
    try: return _entier_strict(t)
    except ValueError: rose_erreur("nombre entier invalide")

def _lire_flottant():
    d = _strtod_prefix(_lire())
    if d is None: rose_erreur("flottant invalide")
    return d

def _lire_rationnel():
    t = _lire()
    try: return Fraction(t)
    except ValueError: rose_erreur("rationnel invalide")

def _saisir_entier(): return _lire_entier()
def _saisir_flottant(): return _lire_flottant()
def _saisir_rationnel(): return _lire_rationnel()

def _abs(x): return abs(x)
def _partie_entiere(x):
    """⌊x⌋ — partie entière mathématique (floor), comme la cible C."""
    if isinstance(x, float):
        if x != x or x in (float("inf"), float("-inf")): return x
        return math.floor(x)
    return math.floor(x)
def _nb_chiffres(n):
    n = int(n)
    return 1 if n == 0 else len(str(abs(n)))
def _idx(l, i):
    if isinstance(l, Table):
        for k, v in l.items():
            if k == i: return v
        rose_erreur("clé absente de la table")
    i = int(i)
    quoi = "chaîne" if isinstance(l, str) else "liste"
    if not (0 <= i < len(l)): rose_erreur(f"index hors des limites de la {quoi}")
    return l[i]
def _longueur(l): return len(l)

def _ordre(c):
    if not isinstance(c, str) or len(c) != 1:
        rose_erreur("ordre attend une chaîne d'un seul caractère")
    return ord(c)

def _caractere(n):
    n = int(n)
    if n < 0 or n > 0x10FFFF:
        rose_erreur("caractere : code hors plage (0 à 1114111)")
    if 0xD800 <= n <= 0xDFFF:
        rose_erreur("caractere : code réservé (55296 à 57343)")
    return chr(n)

def _sous_chaine(s, d, f):
    s = s if isinstance(s, str) else ""
    d = int(d); f = int(f)
    if d < 0 or f < 0 or d > 0xFFFFFFFFFFFFFFFF or f > 0xFFFFFFFFFFFFFFFF:
        rose_erreur("sous_chaine : les bornes doivent être des entiers positifs")
    taille = len(s)
    if d > taille or f < d: rose_erreur("sous_chaine : bornes invalides")
    if f > taille: f = taille
    return s[d:f]

def _concatener(l):
    if not isinstance(l, list): rose_erreur("concatener attend une liste")
    for e in l:
        if not isinstance(e, str): rose_erreur("concatener attend une liste de chaînes")
    return "".join(l)

def _lire_texte(chemin):
    chemin = chemin if isinstance(chemin, str) else ""
    try:
        with open(chemin, "rb") as fh:
            brut = fh.read()
    except OSError:
        rose_erreur(f"lecture du fichier impossible ({chemin})")
    if len(brut) > (1 << 28): rose_erreur(f"fichier trop volumineux ({chemin})")
    return brut.decode("utf-8", "surrogateescape")

def _ecrire_texte(chemin, contenu):
    chemin = chemin if isinstance(chemin, str) else ""
    contenu = contenu if isinstance(contenu, str) else ""
    try:
        with open(chemin, "wb") as fh:
            fh.write(contenu.encode("utf-8", "surrogateescape"))
    except OSError:
        rose_erreur(f"écriture du fichier impossible ({chemin})")
    return None

def _nb_arguments(): return len(PROG_ARGS)

def _argument(i):
    i = int(i)
    if not (0 <= i < len(PROG_ARGS)):
        rose_erreur("index hors des limites des arguments")
    return PROG_ARGS[i]

def _afficher_erreur(v):
    print(_rose_str(v), file=sys.stderr)
    sys.stderr.flush()
    return None

def _quitter(c):
    c = int(c)
    if not (-2147483648 <= c <= 2147483647):
        rose_erreur("code de sortie hors plage")
    sys.stdout.flush(); sys.stderr.flush()
    raise QuitterSignal(c)

def _charger_dll(nom):
    candidats = [nom] if (nom.endswith((".so", ".dll", ".dylib")) or "/" in nom) else \
        [f"lib{nom}.so", f"{nom}.so", f"{nom}.dylib", nom,
         f"lib{nom}.so.6", f"lib{nom}.so.7", f"lib{nom}.so.8"]
    for c in candidats:
        try: return ctypes.CDLL(c)
        except OSError: pass
    rose_erreur(f"chargement de bibliothèque impossible ({nom})")

def _charger_chemin(chemin): return _charger_dll(chemin)

def _appeler(h, nom, *args):
    f = getattr(h, nom, None)
    if f is None: rose_erreur(f"symbole introuvable dans la bibliothèque ({nom})")
    f.argtypes = [ctypes.c_longlong if isinstance(a, int) and not isinstance(a, bool)
                  else ctypes.c_double for a in args]
    f.restype = ctypes.c_double
    return f(*args)

# ── interpréteur ─────────────────────────────────────────────────────

class RetourSignale(Exception):
    def __init__(self, valeur):
        self.valeur = valeur

ARGTYPES = {T_ENTIER: ctypes.c_longlong, T_FLOTTANT: ctypes.c_double,
            "booleen": ctypes.c_int, "chaine": ctypes.c_char_p}
RETOURS = {"booleen": ctypes.c_int, "chaine": ctypes.c_char_p}

class Interp:
    def __init__(self, prog):
        self.prog = prog
        self.globaux = {}
        self.trames = [{}]
        self.dlls = {}

    def cible(self, nom):
        f = self.trames[-1]
        if nom in f: return f
        if nom in self.globaux: return self.globaux
        return None

    def affecter(self, nom, v):
        c = self.cible(nom)
        if c is not None and nom in c:
            cur = c[nom]
            if isinstance(cur, bool): pass
            elif isinstance(cur, float): v = float(v)
            elif isinstance(cur, Fraction): v = Fraction(v)
            elif isinstance(cur, int): v = _conv_t(v, "entier")
            c[nom] = v
        elif c is not None:
            c[nom] = v
        else:
            self.trames[-1][nom] = v

    def lire_var(self, nom, type_read):
        v = (_saisir_flottant() if type_read == T_FLOTTANT else
             _saisir_rationnel() if type_read == T_RATIONNEL else _saisir_entier())
        self.affecter(nom, v)

    def externe(self, sig, args):
        dll = sig.dll_son or sig.dll or "c"
        if dll not in self.dlls:
            self.dlls[dll] = _charger_dll(dll)
        h = self.dlls[dll]
        f = getattr(h, sig.symbole or sig.nom, None)
        if f is None:
            rose_erreur(f"symbole introuvable dans la bibliothèque ({sig.symbole or sig.nom})")
        types = [t.nom for (_, t) in sig.params]
        f.argtypes = [ARGTYPES.get(t, ctypes.c_longlong) for t in types]
        f.restype = RETOURS.get(sig.retour.nom,
                                ctypes.c_double if sig.retour.nom == "flottant"
                                else ctypes.c_longlong)
        passes = []
        for a, t in zip(args, types):
            if t == "booleen": passes.append(1 if a else 0)
            elif t == "chaine": passes.append(a.encode())
            elif t == "flottant": passes.append(float(a))
            else: passes.append(int(a))
        r = f(*passes)
        rn = sig.retour.nom
        if rn == "chaine": return r.decode() if r is not None else ""
        if rn == "booleen": return bool(r)
        if rn == "flottant": return float(r)
        return int(r)

    def appeler(self, sig, args):
        if sig.corps is None:
            return self.externe(sig, args)
        self.trames.append({p: _conv_t(a, t.nom) for (p, t), a in zip(sig.params, args)})
        try:
            self.bloc(sig.corps)
            return None
        except RetourSignale as r:
            return _conv_t(r.valeur, sig.retour.nom)
        finally:
            self.trames.pop()

    # ── instructions ──────────────────────────────────────────────
    def bloc(self, b):
        for i in b.instructions:
            self.instruction(i)

    def instruction(self, i):
        if isinstance(i, Afficher):
            _afficher(self.eval(i.expression)); return
        if isinstance(i, Lire):
            self.lire_var(i.nom, getattr(i, "type_read", T_ENTIER)); return
        if isinstance(i, Affectation):
            self.affecter(i.nom, self.eval(i.valeur)); return
        if isinstance(i, Si):
            if self.eval(i.test): self.bloc(i.alors)
            elif i.sinon is not None: self.bloc(i.sinon)
            return
        if isinstance(i, TantQue):
            while self.eval(i.test): self.bloc(i.corps)
            return
        if isinstance(i, Pour):
            debut, fin = int(self.eval(i.debut)), int(self.eval(i.fin))
            k = debut
            self.trames[-1][i.variable] = k
            try:
                while k < fin:
                    self.trames[-1][i.variable] = k
                    self.bloc(i.corps)
                    k = int(self.trames[-1][i.variable]) + 1
            finally:
                self.trames[-1].pop(i.variable, None)
            return
        if isinstance(i, Retour):
            raise RetourSignale(None if i.valeur is None else self.eval(i.valeur))
        if isinstance(i, ExprStatement):
            self.eval(i.expression); return
        rose_erreur(f"instruction inconnue ({type(i).__name__})")

    # ── expressions ───────────────────────────────────────────────
    def eval(self, e):
        if isinstance(e, Litteral):
            if e.flottant: return float(e.texte)
            if "/" in e.texte:
                n, d = e.texte.split("/")
                return Fraction(int(n), int(d))
            return int(e.texte)
        if isinstance(e, ChaineLit): return e.valeur
        if isinstance(e, BooleenLit): return e.valeur
        if isinstance(e, Var):
            c = self.cible(e.nom)
            if c is None: rose_erreur(f"variable '{e.nom}' utilisée sans valeur")
            return c[e.nom]
        if isinstance(e, ListeLit): return [self.eval(x) for x in e.elements]
        if isinstance(e, Affectation):
            v = self.eval(e.valeur); self.affecter(e.nom, v); return v
        if isinstance(e, Unaire):
            v = self.eval(e.operande)
            if e.op == "non": return not v
            if e.op == "+": return v
            return -v
        if isinstance(e, Factorielle): return _fact(int(self.eval(e.operande)))
        if isinstance(e, Index):
            return _idx(self.eval(e.base), self.eval(e.index))
        if isinstance(e, Binaire): return self.binaire(e)
        if isinstance(e, Appel): return self.appel(e)
        rose_erreur(f"expression inconnue ({type(e).__name__})")

    def binaire(self, e):
        op = e.op
        if op == "et": return self.eval(e.gauche) and self.eval(e.droite)
        if op == "ou": return self.eval(e.gauche) or self.eval(e.droite)
        g, d = self.eval(e.gauche), self.eval(e.droite)
        if op == "/": return _div(g, d)
        if op == "%": return _mod(g, d)
        if op == "div": return _divf(g, d)
        if op == "^": return _powr(g, d)
        if op in ("≈", "?=", "=~"): return _approxe(g, d)
        if op == "+": return g + d
        if op == "-": return g - d
        if op == "*": return g * d
        if op == "==": return g == d
        if op == "!=": return g != d
        if op == "<": return g < d
        if op == "<=": return g <= d
        if op == ">": return g > d
        if op == ">=": return g >= d
        rose_erreur(f"opérateur '{op}' inconnu")

    def appel(self, e):
        args = [self.eval(a) for a in e.args]
        if e.module == "dynamique":
            if e.nom == "charger": return _charger_chemin(args[0])
            if e.nom == "fermer": return None
            return _appeler(args[0], args[1], *args[2:])
        sig = self.prog.signatures.get(e.nom)
        if sig is not None:
            return self.appeler(sig, args)
        nom = e.nom
        if nom == "joindre": args[0].append(args[1]); return None
        if nom == "fraction": return Fraction(int(args[0]), int(args[1]))
        table = {"factorielle": _fact, "fibonacci": _fib, "pgcd": _pgcd, "ppcm": _ppcm,
                 "puissance": _powr, "est_premier": _prem, "compter_premiers": _compter_premiers,
                 "longueur": _longueur, "abs": _abs, "min": min, "max": max,
                 "nombre_pi": _pi, "saisir_entier": _saisir_entier,
                 "saisir_flottant": _saisir_flottant, "nb_chiffres": _nb_chiffres,
                 "partie_entiere": _partie_entiere, "entier": _vers_entier,
                 "flottant": _vers_flottant, "rationnel": _vers_q, "chaine": _chaine,
                 "afficher": _afficher,
                 "ordre": _ordre, "caractere": _caractere, "sous_chaine": _sous_chaine,
                 "concatener": _concatener, "lire_texte": _lire_texte,
                 "ecrire_texte": _ecrire_texte, "nb_arguments": _nb_arguments,
                 "argument": _argument, "afficher_erreur": _afficher_erreur,
                 "quitter": _quitter,
                 "table": _table, "inserer": _inserer, "cle_existe": _cle_existe,
                 "supprimer": _supprimer, "cles": _cles, "valeurs": _valeurs}
        fn = table.get(nom)
        if fn is None:
            rose_erreur(f"fonction '{nom}' inconnue")
        return fn(*args)

    def executer(self):
        for i in self.prog.instructions:
            self.instruction(i)
        sys.stdout.flush()

def _table():
    return Table()

def _inserer(t, cle, valeur):
    if not isinstance(t, Table): rose_erreur("inserer attend une table")
    for i, (k, v) in enumerate(t.items()):
        if k == cle:
            t.items()[i] = (cle, valeur)
            return None
    t.items().append((cle, valeur))
    return None

def _cle_existe(t, cle):
    if not isinstance(t, Table): rose_erreur("cle_existe attend une table")
    return any(k == cle for k, v in t.items())

def _supprimer(t, cle):
    if not isinstance(t, Table): rose_erreur("supprimer attend une table")
    for i, (k, v) in enumerate(t.items()):
        if k == cle:
            del t.items()[i]
            return True
    return False

def _cles(t):
    if not isinstance(t, Table): rose_erreur("cles attend une table")
    return [k for k, v in t.items()]

def _valeurs(t):
    if not isinstance(t, Table): rose_erreur("valeurs attend une table")
    return [v for k, v in t.items()]

# ── point d'entrée ───────────────────────────────────────────────────

def executer_source(source: str) -> int:
    try:
        prog = analyser(source)
        replier_constants(prog)
        Verificateur(prog).verifier()
        Interp(prog).executer()
    except ErreurRose as e:
        sys.stdout.flush()
        print("Rose : erreur —", e, file=sys.stderr)
        return 1
    except QuitterSignal as q:
        sys.stdout.flush(); sys.stderr.flush()
        return q.code
    except RetourSignale:
        sys.stdout.flush()
        print("Rose : erreur — 'retour' hors de fonction", file=sys.stderr)
        return 1
    except RecursionError:
        sys.stdout.flush()
        print("Rose : erreur — récursion trop profonde", file=sys.stderr)
        return 1
    return 0

def _fil(source, boite):
    try:
        boite.append(executer_source(source))
    except SystemExit as s:
        boite.append(s.code if isinstance(s.code, int) else 1)

def main(argv):
    if len(argv) < 2 or argv[1] in ("-h", "--help", "aide"):
        print(__doc__.strip()); return 0
    if argv[1] == "version":
        print("Rose 2.0.0 — moteur Python 3 (int/Fraction/float64)"); return 0
    if argv[1] == "info":
        print("rose 2.0.0 — moteur Python")
        print("  entiers    : précision arbitraire native")
        print("  rationnels : Fraction exacte")
        print("  flottants  : float64, aller-retour le plus court (repr)")
        print("  FFI        : ctypes (dlopen)")
        return 0
    args = argv[2:] if argv[1] == "run" else argv[1:]
    fichier = next((a for a in args if not a.startswith("-")), None)
    if not fichier:
        print("rose: quel programme interpréter ?", file=sys.stderr); return 1
    PROG_ARGS.extend(args[args.index(fichier) + 1:])
    try:
        with open(fichier, encoding="utf-8") as f:
            source = f.read()
    except OSError:
        print(f"rose: fichier introuvable : {fichier}", file=sys.stderr); return 1
    # grande pile pour la récursion profonde, comme le moteur natif
    boite = []
    try:
        threading.stack_size(512 * 1024 * 1024)
        t = threading.Thread(target=_fil, args=(source, boite))
        t.start(); t.join()
        return boite[0] if boite else 1
    except (ValueError, RuntimeError):
        return executer_source(source)

import threading

if __name__ == "__main__":
    sys.exit(main(sys.argv))
