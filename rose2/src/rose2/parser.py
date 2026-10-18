"""Rose v2 — AST et analyse syntaxique (descendance récursive)."""
from __future__ import annotations
from dataclasses import dataclass, field
from .lexer import Token, Tk, ErreurRose, tokeniser

# --------------------------------------------------------------------------- AST

@dataclass
class TypeRose:
    nom: str                      # entier|rationnel|flottant|booleen|chaine|liste
    element: "TypeRose|None" = None

    def __str__(self):
        return f"liste<{self.element}>" if self.nom == "liste" else self.nom

    def __eq__(self, other):
        return (isinstance(other, TypeRose) and self.nom == other.nom
                and self.element == other.element)

    def __hash__(self):
        return hash((self.nom, self.element))


T_ENTIER = TypeRose("entier")
T_RATIONNEL = TypeRose("rationnel")
T_FLOTTANT = TypeRose("flottant")
T_BOOLEEN = TypeRose("booleen")
T_CHAINE = TypeRose("chaine")
T_TABLE = TypeRose("table")
T_VALEUR = TypeRose("valeur")
T_VIDE = TypeRose("vide")


@dataclass(kw_only=True)
class Noeud:
    ligne: int = 0


@dataclass
class Litteral(Noeud):
    texte: str                    # '1234' ou '12.5' normalisé
    flottant: bool = False


@dataclass
class ChaineLit(Noeud):
    valeur: str


@dataclass
class BooleenLit(Noeud):
    valeur: bool


@dataclass
class ListeLit(Noeud):
    elements: list


@dataclass
class Var(Noeud):
    nom: str


@dataclass
class Affectation(Noeud):
    nom: str
    valeur: Noeud


@dataclass
class Binaire(Noeud):
    op: str
    gauche: Noeud
    droite: Noeud


@dataclass
class Unaire(Noeud):
    op: str                       # '-','+','non'
    operande: Noeud


@dataclass
class Factorielle(Noeud):
    operande: Noeud


@dataclass
class Appel(Noeud):
    module: str | None            # None ou 'dynamique'
    nom: str
    args: list


@dataclass
class Index(Noeud):
    base: Noeud
    index: Noeud


@dataclass
class Bloc(Noeud):
    instructions: list


@dataclass
class Si(Noeud):
    test: Noeud
    alors: Bloc
    sinon: Bloc | None


@dataclass
class TantQue(Noeud):
    test: Noeud
    corps: Bloc


@dataclass
class Pour(Noeud):
    variable: str
    debut: Noeud
    fin: Noeud                    # exclusif
    corps: Bloc


@dataclass
class Retour(Noeud):
    valeur: Noeud | None


@dataclass
class Afficher(Noeud):
    expression: Noeud


@dataclass
class Lire(Noeud):
    nom: str


@dataclass
class ExprStatement(Noeud):
    expression: Noeud


@dataclass
class Signature:
    nom: str
    params: list                  # [(nom, TypeRose)]
    retour: TypeRose | None
    corps: Bloc | None            # None = fonction externe
    dll: str | None = None        # argument de @dll
    dll_son: str | None = None    # second argument de @dll (son de liaison)
    symbole: str | None = None    # @sym
    ligne: int = 0


@dataclass
class Programme:
    signatures: dict              # nom -> Signature
    instructions: list
    ligne: int = 0


# --------------------------------------------------------------------------- Parser

NOMS_TYPES = {"entier": T_ENTIER, "rationnel": T_RATIONNEL, "flottant": T_FLOTTANT,
              "booleen": T_BOOLEEN, "chaine": T_CHAINE}

OP_COMPAR = {Tk.Inf: "<", Tk.InfEg: "<=", Tk.Sup: ">", Tk.SupEg: ">=",
             Tk.DoubleEg: "==", Tk.Diff: "!=", Tk.Environ: "≈",
             Tk.QMarkEg: "?=", Tk.TildeEg: "=~"}


class Parser:
    def __init__(self, source: str):
        self.tokens = tokeniser(source)
        self.pos = 0

    # -- utilitaires de base -------------------------------------------------
    def voir(self, d: int = 0) -> Token:
        p = self.pos + d
        return self.tokens[p if p < len(self.tokens) else -1]

    def suivant(self) -> Token:
        t = self.voir()
        if t.type != Tk.Eof:
            self.pos += 1
        return t

    def est(self, *tt: Tk) -> bool:
        return self.voir().type in tt

    def est_mot(self, texte: str) -> bool:
        t = self.voir()
        return t.type in (Tk.Ident, Tk.Mot) and t.texte == texte

    def accepter(self, *tt: Tk) -> Token | None:
        if self.est(*tt):
            return self.suivant()
        return None

    def accepter_mot(self, texte: str) -> bool:
        if self.est_mot(texte):
            self.suivant()
            return True
        return False

    def exiger(self, tt: Tk, quoi: str = "") -> Token:
        t = self.suivant()
        if t.type != tt:
            raise ErreurRose(f"Attendu {quoi or tt.name}, trouvé {t.texte!r}", t.ligne, t.colonne)
        return t

    def nom_ident(self) -> str:
        t = self.suivant()
        if t.type not in (Tk.Ident, Tk.Mot):
            raise ErreurRose(f"Attendu un identifiant, trouvé {t.texte!r}", t.ligne, t.colonne)
        return t.texte

    # -- programme -----------------------------------------------------------
    def programmer(self) -> Programme:
        prog = Programme(signatures={}, instructions=[])
        annotations: dict = {}
        while not self.est(Tk.Eof):
            if self.accepter(Tk.Semi):
                continue
            if self.est(Tk.Arobase) and self.voir(1).type in (Tk.Ident, Tk.Mot) \
                    and self.voir(1).texte in ("dll", "sym"):
                self.suivant()                       # @
                genre = self.nom_ident()
                self.exiger(Tk.LParen, "'(' après @dll/@sym")
                if genre == "dll":
                    nom_dll = self.exiger(Tk.Chaine, "nom de bibliothèque").texte
                    filiale = None
                    if self.accepter(Tk.Virgule):
                        filiale = self.exiger(Tk.Chaine, "nom de fichier").texte
                    annotations["dll"] = nom_dll
                    annotations["dll_son"] = filiale
                else:
                    annotations["sym"] = self.exiger(Tk.Chaine, "nom de symbole").texte
                self.exiger(Tk.RParen, "')'")
                continue
            if self.est_mot("fn"):
                sig = self.def_fonction(annotations)
                annotations = {}
                if sig.nom in prog.signatures:
                    raise ErreurRose(f"Fonction {sig.nom!r} définie deux fois", sig.ligne)
                prog.signatures[sig.nom] = sig
                continue
            if annotations:
                raise ErreurRose("Annotation @dll/@sym sans fonction juste après",
                                 self.voir().ligne, self.voir().colonne)
            prog.instructions.append(self.instruction())
        return prog

    def def_fonction(self, annotations: dict) -> Signature:
        tok = self.suivant()                      # 'fn'
        nom = self.nom_ident()
        self.exiger(Tk.LParen, "'('")
        params = []
        if not self.est(Tk.RParen):
            while True:
                pname = self.nom_ident()
                self.exiger(Tk.DeuxPts, "':' (nom: type)")
                ptype = self.type_rose()
                params.append((pname, ptype))
                if not self.accepter(Tk.Virgule):
                    break
        self.exiger(Tk.RParen, "')'")
        retour = None
        if self.accepter(Tk.PointDInterro):
            retour = self.type_rose()
        if self.est(Tk.LBrace):
            corps = self.bloc()
        else:
            corps = None                          # déclaration externe
            if retour is None:
                raise ErreurRose("Une fonction externe doit déclarer un type de retour", tok.ligne)
        if corps is not None and retour is None:
            retour = T_VIDE
        return Signature(nom=nom, params=params, retour=retour, corps=corps,
                         dll=annotations.get("dll"), dll_son=annotations.get("dll_son"),
                         symbole=annotations.get("sym"), ligne=tok.ligne)

    def type_rose(self) -> TypeRose:
        t = self.suivant()
        if t.type not in (Tk.Ident, Tk.Mot):
            raise ErreurRose(f"Attendu un type, trouvé {t.texte!r}", t.ligne, t.colonne)
        if t.texte == "liste":
            self.exiger(Tk.Inf, "'<' de liste<T>")
            elt = self.type_rose()
            self.exiger(Tk.Sup, "'>' de liste<T>")
            return TypeRose("liste", elt)
        if t.texte in NOMS_TYPES:
            return NOMS_TYPES[t.texte]
        raise ErreurRose(f"Type inconnu {t.texte!r}", t.ligne, t.colonne)

    # -- instructions --------------------------------------------------------
    def instruction(self) -> Noeud:
        t = self.voir()
        if t.type == Tk.LBrace:
            return self.bloc()
        if t.type == Tk.FlecheD:
            self.suivant()
            e = self.expression()
            return Afficher(ligne=t.ligne, expression=e)
        if t.type == Tk.FlecheG:
            self.suivant()
            nom = self.nom_ident()
            return Lire(ligne=t.ligne, nom=nom)
        if t.type == Tk.Question:
            self.suivant()
            test = self.parentheser()
            corps = self.corps_chevron()
            return Si(ligne=t.ligne, test=test, alors=corps, sinon=None)
        if t.type == Tk.Arobase:
            self.suivant()
            test = self.parentheser()
            corps = self.corps_chevron()
            return TantQue(ligne=t.ligne, test=test, corps=corps)
        if t.type == Tk.Ident and t.texte == "si":
            self.suivant()
            test = self.parentheser()
            alors = self.bloc()
            sinon = None
            if self.accepter_mot("sinon"):
                sinon = self.bloc() if self.est(Tk.LBrace) else Bloc(
                    instructions=[self.instruction()], ligne=self.voir().ligne)
            return Si(ligne=t.ligne, test=test, alors=alors, sinon=sinon)
        if t.type == Tk.Ident and t.texte == "tant_que":
            self.suivant()
            test = self.parentheser()
            corps = self.bloc()
            return TantQue(ligne=t.ligne, test=test, corps=corps)
        if t.type == Tk.Ident and t.texte == "pour":
            self.suivant()
            var = self.nom_ident()
            if not self.accepter_mot("de"):
                raise ErreurRose("Attendu 'de' dans pour…de…vers", self.voir().ligne, self.voir().colonne)
            debut = self.expression()
            if not self.accepter_mot("vers"):
                raise ErreurRose("Attendu 'vers' dans pour…de…vers", self.voir().ligne, self.voir().colonne)
            fin = self.expression()
            corps = self.bloc()
            return Pour(ligne=t.ligne, variable=var, debut=debut, fin=fin, corps=corps)
        if t.type == Tk.Ident and t.texte == "retour":
            self.suivant()
            if self.est(Tk.Semi, Tk.RBrace, Tk.Eof):
                return Retour(ligne=t.ligne, valeur=None)
            return Retour(ligne=t.ligne, valeur=self.expression())
        # affectation ou expression
        if t.type in (Tk.Ident, Tk.Mot) and self.voir(1).type == Tk.Affect:
            nom = self.nom_ident()
            self.suivant()                        # '='
            valeur = self.expression()
            return Affectation(ligne=t.ligne, nom=nom, valeur=valeur)
        e = self.expression()
        return ExprStatement(ligne=t.ligne, expression=e)

    def parentheser(self) -> Noeud:
        self.exiger(Tk.LParen, "'(' ")
        e = self.expression()
        self.exiger(Tk.RParen, "')'")
        return e

    def corps_chevron(self) -> Bloc:
        """Corps [ s1 ; s2 ; … ] utilisé par le sucre ? et @."""
        t = self.exiger(Tk.LBracket, "'[' de corps")
        instructions = []
        while not self.est(Tk.RBracket, Tk.Eof):
            instructions.append(self.instruction())
            if not self.accepter(Tk.Semi):
                break
        self.exiger(Tk.RBracket, "']' de corps")
        return Bloc(instructions=instructions, ligne=t.ligne)

    def bloc(self) -> Bloc:
        t = self.exiger(Tk.LBrace, "'{'")
        instructions = []
        while not self.est(Tk.RBrace, Tk.Eof):
            if self.accepter(Tk.Semi):
                continue
            instructions.append(self.instruction())
            self.accepter(Tk.Semi)
        self.exiger(Tk.RBrace, "'}'")
        return Bloc(instructions=instructions, ligne=t.ligne)

    # -- expressions (précédence) ----------------------------------------------
    def expression(self) -> Noeud:
        return self.ou_expr()

    def ou_expr(self) -> Noeud:
        g = self.et_expr()
        while self.est_mot("ou"):
            self.suivant()
            g = Binaire(ligne=self.voir().ligne, op="ou", gauche=g, droite=self.et_expr())
        return g

    def et_expr(self) -> Noeud:
        g = self.non_expr()
        while self.est_mot("et"):
            self.suivant()
            g = Binaire(ligne=self.voir().ligne, op="et", gauche=g, droite=self.non_expr())
        return g

    def non_expr(self) -> Noeud:
        if self.est_mot("non"):
            t = self.suivant()
            return Unaire(ligne=t.ligne, op="non", operande=self.non_expr())
        return self.comparaison()

    def comparaison(self) -> Noeud:
        g = self.somme()
        while self.voir().type in OP_COMPAR:
            tok = self.suivant()
            g = Binaire(ligne=tok.ligne, op=OP_COMPAR[tok.type], gauche=g, droite=self.somme())
        return g

    def somme(self) -> Noeud:
        g = self.produit()
        while self.est(Tk.Plus, Tk.Moins):
            tok = self.suivant()
            op = "+" if tok.type == Tk.Plus else "-"
            g = Binaire(ligne=tok.ligne, op=op, gauche=g, droite=self.produit())
        return g

    def produit(self) -> Noeud:
        g = self.puissance()
        while self.est(Tk.Etoile, Tk.Slash, Tk.Percent, Tk.DivEntier) \
                or self.est_mot("div"):
            tok = self.suivant()
            op = {Tk.Etoile: "*", Tk.Slash: "/", Tk.Percent: "%",
                  Tk.DivEntier: "div"}.get(tok.type, "div")
            g = Binaire(ligne=tok.ligne, op=op, gauche=g, droite=self.puissance())
        return g

    def puissance(self) -> Noeud:
        g = self.factorielle_post()
        if self.est(Tk.Chapeau):
            tok = self.suivant()
            droite = self.puissance()             # droite-associatif, accepte -exposant
            return Binaire(ligne=tok.ligne, op="^", gauche=g, droite=droite)
        return g

    def factorielle_post(self) -> Noeud:
        g = self.unaire()
        while self.est(Tk.Bang):
            tok = self.suivant()
            g = Factorielle(ligne=tok.ligne, operande=g)
        return g

    def unaire(self) -> Noeud:
        if self.est(Tk.Racine):
            t = self.suivant()
            return Binaire(ligne=t.ligne, op="^", gauche=self.unaire(),
                           droite=Litteral(ligne=t.ligne, texte="0.5", flottant=True))
        if self.est(Tk.Moins):
            t = self.suivant()
            return Unaire(ligne=t.ligne, op="-", operande=self.unaire())
        if self.est(Tk.Plus):
            self.suivant()
            return self.unaire()
        return self.postfixe()

    def postfixe(self) -> Noeud:
        g = self.atome()
        while True:
            if self.est(Tk.LParen):
                tok = self.suivant()
                if isinstance(g, Appel) and g.args is None:
                    module, nom = g.module, g.nom
                elif isinstance(g, Var):
                    module, nom = None, g.nom
                else:
                    raise ErreurRose("Seul un nom de fonction peut être appelé", tok.ligne, tok.colonne)
                args = []
                if not self.est(Tk.RParen):
                    while True:
                        args.append(self.expression())
                        if not self.accepter(Tk.Virgule):
                            break
                self.exiger(Tk.RParen, "')' de appel")
                g = Appel(ligne=tok.ligne, module=module, nom=nom, args=args)
            elif self.est(Tk.LBracket):
                tok = self.suivant()
                if self.est(Tk.DeuxPts):
                    # tranche s[:b]
                    self.suivant()
                    debut = Litteral(ligne=tok.ligne, texte="0", flottant=False)
                    if self.est(Tk.RBracket):
                        self.suivant()
                        fin = Appel(ligne=tok.ligne, module=None, nom="longueur", args=[g])
                    else:
                        fin = self.expression()
                        self.exiger(Tk.RBracket, "']' de tranche")
                    g = Appel(ligne=tok.ligne, module=None, nom="sous_chaine", args=[g, debut, fin])
                else:
                    idx = self.expression()
                    if self.est(Tk.DeuxPts):
                        # tranche s[a:b] ou s[a:]
                        self.suivant()
                        debut = idx
                        if self.est(Tk.RBracket):
                            self.suivant()
                            fin = Appel(ligne=tok.ligne, module=None, nom="longueur", args=[g])
                        else:
                            fin = self.expression()
                            self.exiger(Tk.RBracket, "']' de tranche")
                        g = Appel(ligne=tok.ligne, module=None, nom="sous_chaine", args=[g, debut, fin])
                    else:
                        self.exiger(Tk.RBracket, "']' d'index")
                        g = Index(ligne=tok.ligne, base=g, index=idx)
            else:
                return g

    def atome(self) -> Noeud:
        t = self.voir()
        if t.type == Tk.Nombre:
            self.suivant()
            return Litteral(ligne=t.ligne, texte=t.texte, flottant=False)
        if t.type == Tk.Flottant:
            self.suivant()
            return Litteral(ligne=t.ligne, texte=t.texte.replace(",", "."), flottant=True)
        if t.type == Tk.Chaine:
            self.suivant()
            return ChaineLit(ligne=t.ligne, valeur=t.texte)
        if t.type == Tk.LBracket:
            tok = self.suivant()
            elements = []
            if not self.est(Tk.RBracket):
                while True:
                    elements.append(self.expression())
                    if not self.accepter(Tk.Virgule):
                        break
            self.exiger(Tk.RBracket, "']' de liste")
            return ListeLit(ligne=tok.ligne, elements=elements)
        if t.type == Tk.LParen:
            tok = self.suivant()
            e = self.expression()
            self.exiger(Tk.RParen, "')'")
            return e
        if t.type in (Tk.Ident, Tk.Mot):
            if t.texte == "vrai":
                self.suivant()
                return BooleenLit(ligne=t.ligne, valeur=True)
            if t.texte == "faux":
                self.suivant()
                return BooleenLit(ligne=t.ligne, valeur=False)
            # module::fonction
            module = None
            if t.type in (Tk.Ident, Tk.Mot) and self.voir(1).type == Tk.DeuxPoints:
                module = t.texte
                self.suivant()
                self.suivant()
            nom = self.nom_ident()
            return Var(ligne=t.ligne, nom=nom) if module is None else Appel(
                ligne=t.ligne, module=module, nom=nom, args=None)
        raise ErreurRose(f"Attente d'une expression, trouvé {t.texte!r}", t.ligne, t.colonne)


def analyser(source: str) -> Programme:
    return Parser(source).programmer()
