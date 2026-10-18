"""Rose v2 — analyse lexicale."""
from __future__ import annotations
from dataclasses import dataclass
from enum import Enum, auto


class Tk(Enum):
    Eof = auto(); Ident = auto(); Nombre = auto(); Flottant = auto(); Chaine = auto()
    Plus = auto(); Moins = auto(); Etoile = auto(); Slash = auto(); DeuxPts = auto()
    Percent = auto(); Chapeau = auto(); Bang = auto(); Point = auto(); Virgule = auto()
    DivEntier = auto()
    Affect = auto(); LParen = auto(); RParen = auto(); LBrace = auto(); RBrace = auto()
    LBracket = auto(); RBracket = auto(); Semi = auto(); DeuxPoints = auto()
    FlecheD = auto(); FlecheG = auto(); Tilde = auto(); Arobase = auto(); PointDInterro = auto()
    Question = auto()
    Inf = auto(); Sup = auto(); InfEg = auto(); SupEg = auto(); DoubleEg = auto(); Diff = auto()
    Environ = auto(); QMarkEg = auto(); TildeEg = auto(); Vers = auto(); Mot = auto()
    Racine = auto()


MOTS_CLES = {
    "fn", "retour", "si", "sinon", "tant_que", "pour", "de", "vers",
    "vrai", "faux", "et", "ou", "non", "div",
    "entier", "rationnel", "flottant", "booleen", "chaine", "liste",
}

SYMB_OPR = {
    "<=": Tk.InfEg, ">=": Tk.SupEg, "==": Tk.DoubleEg, "!=": Tk.Diff, "/=": Tk.Diff,
    "≈": Tk.Environ, "?=": Tk.QMarkEg, "=~": Tk.TildeEg, "~=": Tk.TildeEg,
    "≤": Tk.InfEg, "≥": Tk.SupEg, "≠": Tk.Diff,
    "×": Tk.Etoile, "⋅": Tk.Etoile, "·": Tk.Etoile, "÷": Tk.DivEntier,
}

CAR_OPR = {
    "+": Tk.Plus, "-": Tk.Moins, "*": Tk.Etoile, "/": Tk.Slash, ":": Tk.DeuxPts,
    "%": Tk.Percent, "^": Tk.Chapeau, "!": Tk.Bang, ".": Tk.Point, ",": Tk.Virgule,
    "?": Tk.Question,
    "(": Tk.LParen, ")": Tk.RParen, "{": Tk.LBrace, "}": Tk.RBrace,
    "[": Tk.LBracket, "]": Tk.RBracket, ";": Tk.Semi, "<": Tk.Inf, ">": Tk.Sup,
    "~": Tk.Tilde, "@": Tk.Arobase,
    "←": Tk.Affect, "→": Tk.PointDInterro, "√": Tk.Racine,
}
# opérateurs monocaractères non-ASCII (≤ ≥ ≠ × ⋅ · ≈)
SYMB_UNIQUES = {k: v for k, v in SYMB_OPR.items() if len(k) == 1}


@dataclass
class Token:
    type: Tk
    texte: str
    ligne: int
    colonne: int

    def __repr__(self):
        return f"{self.type.name}({self.texte!r}@L{self.ligne})"


class ErreurRose(Exception):
    """Erreur de compilation Rose, localisée."""
    def __init__(self, message: str, ligne: int = 0, colonne: int = 0):
        super().__init__(message)
        self.message = message
        self.ligne = ligne
        self.colonne = colonne

    def __str__(self):
        if self.ligne:
            return f"[Ligne {self.ligne}, Colonne {self.colonne}] {self.message}"
        return self.message


def est_lettre(c: str) -> bool:
    return c.isalpha() or c == "_"


def est_chiffre(c: str) -> bool:
    return "0" <= c <= "9"   # isdigit() accepterai ² ³ ２ — hors langage


def est_ident_continue(c: str) -> bool:
    return c.isalnum() or c == "_"


def tokeniser(source: str) -> list[Token]:
    tokens: list[Token] = []
    i, n = 0, len(source)
    ligne, col_ligne = 1, 1
    # position de début de ligne pour recalculer les colonnes
    def advance(chars: int):
        nonlocal i, ligne, col_ligne
        for _ in range(chars):
            if i < n and source[i] == "\n":
                ligne += 1
                col_ligne = 1
            else:
                col_ligne += 1
            i += 1

    while i < n:
        c = source[i]
        # blancs et commentaires
        if c in " \t\r\n":
            advance(1)
            continue
        if c == "#" or (c == "/" and i + 1 < n and source[i + 1] == "/"):
            while i < n and source[i] != "\n":
                advance(1)
            continue
        if c == '"' or c == "'":
            quote = c
            debut_l, debut_c = ligne, col_ligne
            advance(1)
            buf = []
            while True:
                if i >= n:
                    raise ErreurRose("Chaîne non terminée", debut_l, debut_c)
                if source[i] == "\\":
                    if i + 1 >= n:
                        raise ErreurRose("Échappement en fin de fichier", ligne, col_ligne)
                    nxt = source[i + 1]
                    buf.append({"n": "\n", "t": "\t", "\\": "\\", '"': '"', "'": "'"}.get(nxt, "\\" + nxt))
                    advance(2)
                    continue
                if source[i] == quote:
                    advance(1)
                    break
                buf.append(source[i])
                advance(1)
            tokens.append(Token(Tk.Chaine, "".join(buf), debut_l, debut_c))
            continue
        # nombres : 1234, 12.5 (la virgule reste un séparateur, comme dans Rose v1)
        if est_chiffre(c):
            debut_l, debut_c = ligne, col_ligne
            debut = i
            while i < n and est_chiffre(source[i]):
                advance(1)
            est_flottant = False
            if i + 1 < n and source[i] == "." and est_chiffre(source[i + 1]):
                est_flottant = True
                advance(1)
                while i < n and est_chiffre(source[i]):
                    advance(1)
            texte = source[debut:i]
            tokens.append(Token(Tk.Flottant if est_flottant else Tk.Nombre, texte, debut_l, debut_c))
            continue
        # identifiants et mots-clés, avec préfixe $ optionnel
        if c == "π":   # constante flottante 3,14159…
            tokens.append(Token(Tk.Flottant, "3.141592653589793", ligne, col_ligne))
            advance(1)
            continue
        if c in "²³":   # x² ≡ x^2
            tokens.append(Token(Tk.Chapeau, "^", ligne, col_ligne))
            tokens.append(Token(Tk.Nombre, "2" if c == "²" else "3", ligne, col_ligne))
            advance(1)
            continue
        if c == "$" or est_lettre(c):
            debut_l, debut_c = ligne, col_ligne
            debut = i
            if c == "$":
                advance(1)
            if i >= n or not est_lettre(source[i]):
                raise ErreurRose("Attente d'un nom de variable après '$'", debut_l, debut_c)
            while i < n and est_ident_continue(source[i]):
                advance(1)
            texte = source[debut:i]
            nu = texte[1:] if texte.startswith("$") else texte
            if nu in ("fn", "retour", "si", "sinon", "tant_que", "pour", "de", "vers",
                      "vrai", "faux", "et", "ou", "non", "div", "entier", "rationnel",
                      "flottant", "booleen", "chaine", "liste") or texte.startswith("$"):
                tokens.append(Token(Tk.Ident, nu if texte.startswith("$") else texte, debut_l, debut_c))
            else:
                tokens.append(Token(Tk.Mot, texte, debut_l, debut_c))
            continue
        # symboles
        couple = source[i:i + 2]
        if couple in SYMB_OPR:
            tt = SYMB_OPR[couple]
            tok = Token(tt, couple, ligne, col_ligne)
            advance(2)
            tokens.append(tok)
            continue
        if c == ">" and i + 1 < n and source[i + 1] == ">":
            tok = Token(Tk.FlecheD, ">>", ligne, col_ligne)
            advance(2)
            tokens.append(tok)
            continue
        if c == "<" and i + 1 < n and source[i + 1] == "<":
            tok = Token(Tk.FlecheG, "<<", ligne, col_ligne)
            advance(2)
            tokens.append(tok)
            continue
        if c == "-" and i + 1 < n and source[i + 1] == ">":
            tok = Token(Tk.PointDInterro, "->", ligne, col_ligne)
            advance(2)
            tokens.append(tok)
            continue
        if c == "=":
            tok = Token(Tk.Affect, "=", ligne, col_ligne)
            advance(1)
            tokens.append(tok)
            continue
        if c == ":" and i + 1 < n and source[i + 1] == ":":
            tok = Token(Tk.DeuxPoints, "::", ligne, col_ligne)
            advance(2)
            tokens.append(tok)
            continue
        if c in SYMB_UNIQUES:
            tok = Token(SYMB_UNIQUES[c], c, ligne, col_ligne)
            advance(1)
            tokens.append(tok)
            continue
        if c in CAR_OPR:
            tok = Token(CAR_OPR[c], c, ligne, col_ligne)
            advance(1)
            tokens.append(tok)
            continue
        raise ErreurRose(f"Caractère inattendu {c!r}", ligne, col_ligne)

    tokens.append(Token(Tk.Eof, "", ligne, col_ligne))
    return tokens
