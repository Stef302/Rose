"""Rose v2 — vérification et inférence de types, repli de constantes.

Annote chaque noeud d'expression avec .type (TypeRose) et remonte les erreurs
en français avec numéro de ligne. Collecte aussi les « fonctionnalités »
utilisées par le programme (rationnel, chaine, liste, dynamique) pour que le
conducteur refuse une cible incapable de les offrir.
"""
from __future__ import annotations
from fractions import Fraction
from math import isqrt
from .lexer import ErreurRose
from .parser import (Programme, Signature, TypeRose, Litteral, ChaineLit, BooleenLit,
                     ListeLit, Var, Affectation, Binaire, Unaire, Factorielle, Appel,
                     Index, Bloc, Si, TantQue, Pour, Retour, Afficher, Lire,
                     ExprStatement, T_ENTIER, T_RATIONNEL, T_FLOTTANT, T_BOOLEEN,
                     T_CHAINE, T_TABLE, T_VALEUR, T_VIDE)

RANG = {T_BOOLEEN: 0, T_ENTIER: 1, T_RATIONNEL: 2, T_FLOTTANT: 3}
NUMERIQUES = (T_ENTIER, T_RATIONNEL, T_FLOTTANT)

# signature des fonctions standard : (liste d'attendus, retour)
# 'num' = numérique, 'entier' = entier requis, 'liste' = liste de T (T unifié), 'n' = nombre
STDLIB = {
    "afficher":            (["n"], T_VIDE),
    "saisir_entier":       ([], T_ENTIER),
    "saisir_flottant":     ([], T_FLOTTANT),
    "factorielle":         (["entier"], T_ENTIER),
    "fibonacci":           (["entier"], T_ENTIER),
    "pgcd":                (["entier", "entier"], T_ENTIER),
    "ppcm":                (["entier", "entier"], T_ENTIER),
    "puissance":           (["num", "entier"], T_ENTIER),
    "est_premier":         (["entier"], T_BOOLEEN),
    "compter_premiers":    (["entier"], T_ENTIER),
    "nb_chiffres":         (["entier"], T_ENTIER),
    "nombre_pi":           (["entier"], T_CHAINE),
    "longueur":            (["liste_ou_chaine_ou_table"], T_ENTIER),
    "table":               ([], T_TABLE),
    "inserer":             (["table", "tout", "tout"], T_VIDE),
    "cle_existe":          (["table", "tout"], T_BOOLEEN),
    "supprimer":           (["table", "tout"], T_BOOLEEN),
    "cles":                (["table"], T_VALEUR),
    "valeurs":             (["table"], T_VALEUR),
    "joindre":             (["liste", "liste_elt"], T_VIDE),
    "ordre":               (["chaine"], T_ENTIER),
    "caractere":           (["entier"], T_CHAINE),
    "temps":               ([], T_ENTIER),
    "duree":               (["entier"], T_CHAINE),
    "sous_chaine":         (["chaine", "entier", "entier"], T_CHAINE),
    "concatener":          (["liste"], T_CHAINE),
    "lire_texte":          (["chaine"], T_CHAINE),
    "ecrire_texte":        (["chaine", "chaine"], T_VIDE),
    "nb_arguments":        ([], T_ENTIER),
    "argument":            (["entier"], T_CHAINE),
    "afficher_erreur":     (["tout"], T_VIDE),
    "quitter":             (["entier"], T_VIDE),
    "abs":                 (["num"], "MIXTE"),
    "min":                 (["num", "num"], "NUM_MAX"),
    "max":                 (["num", "num"], "NUM_MAX"),
    "partie_entiere":      (["num"], T_ENTIER),
    "fraction":            (["entier", "entier"], T_RATIONNEL),
}
CASTS = {"entier": T_ENTIER, "rationnel": T_RATIONNEL, "flottant": T_FLOTTANT,
         "chaine": T_CHAINE}

DYNAMIQUE = {
    "charger":  (["chaine"], T_ENTIER),
    "fermer":   (["entier"], T_VIDE),
    "appeler0": (["entier", "chaine"], T_FLOTTANT),
    "appeler1": (["entier", "chaine", "num"], T_FLOTTANT),
    "appeler2": (["entier", "chaine", "num", "num"], T_FLOTTANT),
}


def promotion(a: TypeRose, b: TypeRose) -> TypeRose:
    if a == T_VALEUR or b == T_VALEUR:
        return T_VALEUR
    ra, rb = RANG[a], RANG[b]
    r = max(ra, rb)
    return {0: T_ENTIER, 1: T_ENTIER, 2: T_RATIONNEL, 3: T_FLOTTANT}[r]


def est_numerique(t: TypeRose) -> bool:
    return t in NUMERIQUES


class Verificateur:
    def __init__(self, prog: Programme):
        self.prog = prog
        self.fonction_en_cours: Signature | None = None
        self.locals: dict[str, TypeRose] = {}
        self.globales: dict[str, TypeRose] = {}
        self.fonctionnalites: set[str] = set()
        self.assume_retour: TypeRose | None = None

    # -- helpers -------------------------------------------------------------
    def defaut(self, t: TypeRose) -> None:
        if t == T_RATIONNEL:
            self.fonctionnalites.add("rationnel")
        elif t == T_CHAINE:
            self.fonctionnalites.add("chaine")
        elif t.nom == "liste":
            self.fonctionnalites.add("liste")

    def verifier(self):
        for sig in self.prog.signatures.values():
            self.verifier_signature(sig)
        self.fonction_en_cours = None
        self.locals = {}
        self.assume_retour = None
        for instr in self.prog.instructions:
            self.instruction(instr, self.globales)

    def verifier_signature(self, sig: Signature):
        if sig.corps is None:                      # externe
            for (_, t) in sig.params + [("?", sig.retour)]:
                if t not in (T_ENTIER, T_FLOTTANT, T_BOOLEEN, T_CHAINE):
                    raise ErreurRose(
                        f"Type {t} non supporté aux frontières d'une fonction externe "
                        f"(utilisez entier/flottant/booleen/chaine)", sig.ligne)
                self.defaut(t)
            return
        self.fonction_en_cours = sig
        self.locals = {nom: t for (nom, t) in sig.params}
        self.assume_retour = sig.retour
        for t in (t for (_, t) in sig.params):
            self.defaut(t)
        self.defaut(sig.retour)
        self.bloc(sig.corps, self.locals, dans_fonction=True)
        self.fonction_en_cours = None
        self.assume_retour = None

    # -- instructions --------------------------------------------------------
    def bloc(self, b: Bloc, env: dict, dans_fonction: bool):
        for instr in b.instructions:
            self.instruction(instr, env)

    def instruction(self, instr, env: dict):
        if isinstance(instr, Afficher):
            t = self.expression(instr.expression, env)
            if t == T_VIDE:
                raise ErreurRose("Rien à afficher ici", instr.ligne)
            self.defaut(t)
        elif isinstance(instr, Lire):
            t = env.get(instr.nom)
            if t is None:
                env[instr.nom] = T_ENTIER
                self.fonctionnalites.add("lecture")
            elif t not in (T_ENTIER, T_FLOTTANT, T_RATIONNEL):
                raise ErreurRose(f"Ne peut pas lire dans une variable {t} (entier/flottant attendus)", instr.ligne)
            instr.type_read = t if t is not None else T_ENTIER
        elif isinstance(instr, Affectation):
            t = self.expression(instr.valeur, env)
            if t == T_VIDE:
                raise ErreurRose("Impossible d'affecter une valeur sans type", instr.ligne)
            existante = env.get(instr.nom) or self.globales.get(instr.nom)
            if existante is not None:
                cible = existante
                if cible == T_VALEUR or t == T_VALEUR:
                    pass  # valeur dynamique : aucun contrôle
                elif t == T_ENTIER and cible == T_RATIONNEL:
                    pass                              # entier acceptable dans un rationnel
                elif t != cible:
                    if promotion(t, cible) == cible and t in NUMERIQUES and cible in NUMERIQUES:
                        pass                          # promotion numérique autorisée
                    else:
                        raise ErreurRose(
                            f"{instr.nom!r} est {existante}, on ne peut pas y stocker du {t}", instr.ligne)
            else:
                cible = t
            env[instr.nom] = cible
            self.defaut(cible)
        elif isinstance(instr, Si):
            self.exiger_booleen(self.expression(instr.test, env), instr.ligne, "si")
            self.bloc(instr.alors, env, False)
            if instr.sinon is not None:
                self.bloc(instr.sinon, env, False)
        elif isinstance(instr, TantQue):
            self.exiger_booleen(self.expression(instr.test, env), instr.ligne, "tant_que")
            self.bloc(instr.corps, env, False)
        elif isinstance(instr, Pour):
            if instr.variable in env:
                raise ErreurRose(f"La variable de boucle {instr.variable!r} existe déjà", instr.ligne)
            td = self.expression(instr.debut, env)
            tf = self.expression(instr.fin, env)
            if td != T_ENTIER or tf != T_ENTIER:
                raise ErreurRose("pour…de…vers exige des bornes entières", instr.ligne)
            env[instr.variable] = T_ENTIER
            self.bloc(instr.corps, env, False)
        elif isinstance(instr, Retour):
            if self.assume_retour is None:
                raise ErreurRose("'retour' hors de fonction", instr.ligne)
            if instr.valeur is None:
                if self.assume_retour != T_VIDE:
                    raise ErreurRose(f"La fonction doit retourner un {self.assume_retour}", instr.ligne)
            else:
                t = self.expression(instr.valeur, env)
                if t != self.assume_retour and not (t in NUMERIQUES and self.assume_retour in NUMERIQUES
                                                    and promotion(t, self.assume_retour) == self.assume_retour):
                    raise ErreurRose(f"La fonction retourne {t}, attendu {self.assume_retour}", instr.ligne)
        elif isinstance(instr, ExprStatement):
            self.expression(instr.expression, env)
        elif isinstance(instr, Bloc):
            self.bloc(instr, env, False)
        else:
            raise ErreurRose(f"Instruction inconnue {type(instr).__name__}", instr.ligne)

    def exiger_booleen(self, t: TypeRose, ligne: int, quoi: str):
        if t != T_BOOLEEN:
            raise ErreurRose(f"La condition de {quoi} doit être booléenne, trouvé {t}", ligne)

    # -- expressions ---------------------------------------------------------
    def expression(self, e, env: dict) -> TypeRose:
        t = self._expression(e, env)
        e.type = t
        return t

    def _expression(self, e, env: dict) -> TypeRose:
        if isinstance(e, Litteral):
            if not e.flottant and "/" in e.texte:      # littéral rationnel replié "p/q"
                self.defaut(T_RATIONNEL)
                return T_RATIONNEL
            return T_FLOTTANT if e.flottant else T_ENTIER
        if isinstance(e, ChaineLit):
            return T_CHAINE
        if isinstance(e, BooleenLit):
            return T_BOOLEEN
        if isinstance(e, Var):
            t = env.get(e.nom) or self.globales.get(e.nom)
            if t is None:
                raise ErreurRose(f"Variable {e.nom!r} utilisée sans valeur", e.ligne)
            return t
        if isinstance(e, ListeLit):
            if not e.elements:
                raise ErreurRose("Liste vide : le type d'élément est introuvable", e.ligne)
            t_elt = self.expression(e.elements[0], env)
            for autre in e.elements[1:]:
                t2 = self.expression(autre, env)
                if t2 != t_elt:
                    if t2 in NUMERIQUES and t_elt in NUMERIQUES:
                        t_elt = promotion(t2, t_elt)
                    else:
                        raise ErreurRose(f"Liste hétérogène : {t_elt} puis {t2}", e.ligne)
            if t_elt.nom == "liste":
                raise ErreurRose("Listes de listes non supportées pour l'instant", e.ligne)
            return TypeRose("liste", t_elt)
        if isinstance(e, Index):
            tb = self.expression(e.base, env)
            ti = self.expression(e.index, env)
            if tb.nom not in ("liste", "chaine", "table", "valeur"):
                raise ErreurRose(f"Index sur un {tb}, une liste est attendue", e.ligne)
            if tb.nom in ("table", "valeur"):
                if tb.nom == "table" and ti not in (T_ENTIER, T_RATIONNEL, T_FLOTTANT, T_CHAINE, T_BOOLEEN):
                    raise ErreurRose("Index d'une table doit être un texte ou un nombre", e.ligne)
                return T_VALEUR
            if ti != T_ENTIER:
                raise ErreurRose("Index d'une chaîne doit être entier" if tb.nom == "chaine"
                                 else "Index d'une liste doit être entier", e.ligne)
            return tb.element if tb.nom == "liste" else T_CHAINE
        if isinstance(e, Unaire):
            t = self.expression(e.operande, env)
            if e.op == "non":
                self.exiger_booleen(t, e.ligne, "non")
                return T_BOOLEEN
            if not est_numerique(t):
                raise ErreurRose(f"Unaire {e.op} sur un {t}", e.ligne)
            return t
        if isinstance(e, Factorielle):
            t = self.expression(e.operande, env)
            if t != T_ENTIER:
                raise ErreurRose(f"Factorielle attend un entier, trouvé {t}", e.ligne)
            return T_ENTIER
        if isinstance(e, Binaire):
            return self.binaire(e, env)
        if isinstance(e, Appel):
            return self.appel(e, env)
        raise ErreurRose(f"Expression inconnue {type(e).__name__}", e.ligne)

    def binaire(self, e: Binaire, env: dict) -> TypeRose:
        op = e.op
        tg = self.expression(e.gauche, env)
        td = self.expression(e.droite, env)
        if op in ("et", "ou"):
            self.exiger_booleen(tg, e.ligne, "et/ou")
            self.exiger_booleen(td, e.ligne, "et/ou")
            return T_BOOLEEN
        if op == "==":
            if tg == td or (tg in NUMERIQUES and td in NUMERIQUES):
                return T_BOOLEEN
            raise ErreurRose(f"Comparaison {tg} == {td} impossible", e.ligne)
        if op in ("!=", "≈", "?=", "=~"):
            if op == "≈":
                self.fonctionnalites.add("approximation")
            if tg == td or (tg in NUMERIQUES and td in NUMERIQUES):
                return T_BOOLEEN
            raise ErreurRose(f"Comparaison {tg} {op} {td} impossible", e.ligne)
        if op in ("<", "<=", ">", ">="):
            if (tg in NUMERIQUES and td in NUMERIQUES) or (tg == T_CHAINE and td == T_CHAINE):
                return T_BOOLEEN
            raise ErreurRose(f"Ordre {tg} {op} {td} impossible", e.ligne)
        if op == "+":
            if tg == T_CHAINE and td == T_CHAINE:
                return T_CHAINE
            if est_numerique(tg) and est_numerique(td):
                return promotion(tg, td)
            raise ErreurRose(f"Addition {tg} + {td} impossible", e.ligne)
        if op in ("-", "*"):
            if est_numerique(tg) and est_numerique(td):
                return promotion(tg, td)
            raise ErreurRose(f"Opération {tg} {op} {td} impossible", e.ligne)
        if op == "/":
            if est_numerique(tg) and est_numerique(td):
                r = promotion(tg, td)
                if r == T_ENTIER:
                    self.fonctionnalites.add("rationnel")   # division exacte → rationnel
                    return T_RATIONNEL
                return r
            raise ErreurRose(f"Division {tg} / {td} impossible", e.ligne)
        if op == "%":
            if tg == T_ENTIER and td == T_ENTIER:
                return T_ENTIER
            raise ErreurRose("Le modulo exige deux entiers", e.ligne)
        if op == "div":
            if not (est_numerique(tg) or tg == T_BOOLEEN):
                raise ErreurRose(f"div attend un nombre à gauche, trouvé {tg}", e.ligne)
            if not (est_numerique(td) or td == T_BOOLEEN):
                raise ErreurRose(f"div attend un nombre à droite, trouvé {td}", e.ligne)
            if td == T_FLOTTANT or tg == T_FLOTTANT:
                return T_FLOTTANT
            return T_ENTIER
        if op == "^":
            if not est_numerique(tg):
                raise ErreurRose(f"Puissance sur une base {tg}", e.ligne)
            if td == T_FLOTTANT:
                return T_FLOTTANT
            if td in (T_ENTIER, T_RATIONNEL, T_BOOLEEN):
                expo_negatif = isinstance(e.droite, Unaire) and isinstance(e.droite.operande, Litteral) \
                    or (isinstance(e.droite, Litteral) and False)
                if isinstance(e.droite, Unaire) and e.droite.op == "-":
                    if tg == T_ENTIER:
                        self.fonctionnalites.add("rationnel")
                        return T_RATIONNEL
                    return tg
                return T_ENTIER if tg == T_ENTIER else tg
            raise ErreurRose(f"Exposant {td} impossible", e.ligne)
        raise ErreurRose(f"Opérateur {op!r} inconnu", e.ligne)

    def appel(self, e: Appel, env: dict) -> TypeRose:
        if e.module == "dynamique":
            self.fonctionnalites.add("dynamique")
            sig = DYNAMIQUE.get(e.nom)
            if sig is None:
                raise ErreurRose(f"Fonction dynamique::{e.nom} inconnue "
                                 f"(disponibles: {', '.join(DYNAMIQUE)})", e.ligne)
            self.verifier_args(e, sig, env, f"dynamique::{e.nom}")
            return sig[1]
        if e.nom in CASTS and len(e.args) == 1:
            cible = CASTS[e.nom]
            t = self.expression(e.args[0], env)
            if cible == T_CHAINE:
                self.fonctionnalites.add("chaine")
                return T_CHAINE
            if t == T_CHAINE:
                if cible == T_ENTIER:
                    return T_ENTIER
                if cible == T_FLOTTANT:
                    return T_FLOTTANT
                raise ErreurRose("Conversion chaine → rationnel non disponible, passez par flottant", e.ligne)
            if not est_numerique(t):
                raise ErreurRose(f"Conversion {t} → {cible} impossible", e.ligne)
            if cible == T_RATIONNEL:
                self.fonctionnalites.add("rationnel")
            return cible
        sig = self.prog.signatures.get(e.nom)
        whence = "std"
        if sig is None:
            std = STDLIB.get(e.nom)
            if std is None:
                raise ErreurRose(f"Fonction {e.nom!r} inconnue", e.ligne)
            whence = "stdlib"
            retour = self.verifier_stdlib(e, std, env)
            return retour
        if sig.retour is None:
            raise ErreurRose(f"La fonction externe {e.nom!r} n'a pas de corps", e.ligne)
        attends = [t for (_, t) in sig.params]
        if len(e.args) != len(attends):
            raise ErreurRose(f"{e.nom} attend {len(attends)} argument(s), reçu {len(e.args)}", e.ligne)
        for arg, t_att in zip(e.args, attends):
            ta = self.expression(arg, env)
            if ta != t_att and not (ta in NUMERIQUES and t_att in NUMERIQUES
                                     and promotion(ta, t_att) == t_att):
                raise ErreurRose(f"Argument de {e.nom}: {ta} reçu, {t_att} attendu", e.ligne)
        return sig.retour if sig.retour is not None else T_VIDE

    def verifier_args(self, e, sig, env, nom):
        attends = sig[0]
        if len(e.args) != len(attends):
            raise ErreurRose(f"{nom} attend {len(attends)} argument(s), reçu {len(e.args)}", e.ligne)
        for arg, t_att in zip(e.args, attends):
            ta = self.expression(arg, env)
            if t_att == "chaine":
                if ta != T_CHAINE:
                    raise ErreurRose(f"{nom}: une chaine est attendue", e.ligne)
            elif t_att == "num":
                if not est_numerique(ta):
                    raise ErreurRose(f"{nom}: un nombre est attendu, reçu {ta}", e.ligne)
            elif t_att == "entier":
                if ta != T_ENTIER:
                    raise ErreurRose(f"{nom}: un entier est attendu, reçu {ta}", e.ligne)

    def verifier_stdlib(self, e, sig, env) -> TypeRose:
        attends, retour = sig
        if len(e.args) != len(attends):
            raise ErreurRose(f"{e.nom} attend {len(attends)} argument(s), reçu {len(e.args)}", e.ligne)
        t_elt = None
        for arg, t_att in zip(e.args, attends):
            ta = self.expression(arg, env)
            if t_att == "entier":
                if ta != T_ENTIER:
                    raise ErreurRose(f"{e.nom}: un entier est attendu, reçu {ta}", e.ligne)
            elif t_att == "num":
                if not est_numerique(ta):
                    raise ErreurRose(f"{e.nom}: un nombre est attendu, reçu {ta}", e.ligne)
            elif t_att == "liste":
                if ta.nom != "liste":
                    raise ErreurRose(f"{e.nom}: une liste est attendue, reçu {ta}", e.ligne)
                t_elt = ta.element
            elif t_att == "liste_elt":
                if t_elt is not None and ta != t_elt and not (ta in NUMERIQUES and t_elt in NUMERIQUES
                                                              and promotion(ta, t_elt) == t_elt):
                    raise ErreurRose(f"{e.nom}: élément {ta} ne correspond pas à la liste {t_elt}", e.ligne)
            elif t_att in ("liste_ou_chaine", "liste_ou_chaine_ou_table"):
                if ta.nom not in ("liste", "chaine", "table"):
                    raise ErreurRose(f"{e.nom}: une liste, une chaîne ou une table est attendue, reçu {ta}", e.ligne)
            elif t_att == "tout":
                pass  # tout type accepté (afficher_erreur)
        if retour == "MIXTE":
            t = self.expression(e.args[0], env)
            return T_ENTIER if t == T_BOOLEEN else t
        if retour == "NUM_MAX":
            t1 = self.expression(e.args[0], env)
            t2 = self.expression(e.args[1], env)
            return promotion(t1, t2)
        self.defaut(retour)
        return retour


def replier_constants(prog: Programme) -> int:
    """Repli de constantes exact (entiers/rationnels/flottants littéraux)."""
    nb = 0

    def replier(e):
        nonlocal nb
        for champ in ("gauche", "droite", "operande", "expression", "test", "valeur",
                      "debut", "fin", "index", "base"):
            v = getattr(e, champ, None)
            if v is not None and hasattr(v, "__dict__"):
                setattr(e, champ, replier(v))
        if isinstance(e, (Afficher, Lire, ExprStatement, Retour, Affectation)):
            pass
        if isinstance(e, ListeLit):
            e.elements = [replier(x) for x in e.elements]
        if isinstance(e, Binaire):
            g, d = e.gauche, e.droite
            if isinstance(g, Litteral) and isinstance(d, Litteral):
                r = _fold_bin(e.op, g, d, e.ligne)
                if r is not None:
                    nb += 1
                    return r
        if isinstance(e, Unaire) and e.op == "-" and isinstance(e.operande, Litteral):
            li = e.operande
            if not li.flottant:
                return Litteral(ligne=e.ligne, texte=str(-int(li.texte)), flottant=False)
            return Litteral(ligne=e.ligne, texte=str(-float(li.texte)), flottant=True)
        if isinstance(e, Factorielle) and isinstance(e.operande, Litteral) and not e.operande.flottant:
            n = int(e.operande.texte)
            if n >= 0 and n <= 20000:
                nb += 1
                return Litteral(ligne=e.ligne, texte=str(_fact(n)), flottant=False)
        return e

    def walk_instr(i):
        if isinstance(i, Afficher):
            i.expression = replier(i.expression)
        elif isinstance(i, ExprStatement):
            i.expression = replier(i.expression)
        elif isinstance(i, Affectation):
            i.valeur = replier(i.valeur)
        elif isinstance(i, Retour):
            if i.valeur is not None:
                i.valeur = replier(i.valeur)
        elif isinstance(i, Si):
            i.test = replier(i.test)
            _bloc(i.alors); _bloc(i.sinon)
        elif isinstance(i, TantQue):
            i.test = replier(i.test)
            _bloc(i.corps)
        elif isinstance(i, Pour):
            i.debut = replier(i.debut); i.fin = replier(i.fin)
            _bloc(i.corps)
        elif isinstance(i, Bloc):
            _bloc(i)
        elif isinstance(i, (Lire,)):
            pass

    def _bloc(b):
        if b is not None:
            for instr in b.instructions:
                walk_instr(instr)

    for sig in prog.signatures.values():
        if sig.corps is not None:
            _bloc(sig.corps)
    for instr in prog.instructions:
        walk_instr(instr)
    return nb


def _fact(n: int) -> int:
    return _fact_arbre(2, n) if n >= 2 else 1


def _fact_arbre(i, j):
    if i > j:
        return 1
    if i == j:
        return i
    m = (i + j) // 2
    return _fact_arbre(i, m) * _fact_arbre(m + 1, j)


def _valeure(lit: Litteral):
    return Fraction(lit.texte) if not lit.flottant else float(lit.texte)


def _fl_litteral(t: str) -> float:
    if "/" in t:
        n, q = t.split("/", 1)
        return float(n) / float(q)
    return float(t)


def _fold_bin(op: str, g: Litteral, d: Litteral, ligne: int):
    if g.flottant or d.flottant:
        try:                                  # « p/q » replié possible d'un côté :
            a, b = _fl_litteral(g.texte), _fl_litteral(d.texte)   # double(p)/double(q)
        except Exception:
            return None
        try:
            r = {"+": a + b, "-": a - b, "*": a * b, "/": a / b, "^": a ** b, "%": None}[op]
        except Exception:
            return None
        if r is None or isinstance(r, complex) or r != r or r in (float("inf"), float("-inf")):
            return None                              # NaN / infini / complexe : voir runtime
        return Litteral(ligne=ligne, texte=_texte_flottant(r), flottant=True)
    a, b = Fraction(g.texte), Fraction(d.texte)
    try:
        if op == "+":
            r = a + b
        elif op == "-":
            r = a - b
        elif op == "*":
            r = a * b
        elif op == "/":
            if b == 0:
                raise ErreurRose("Division par zéro dans une constante", ligne)
            r = a / b
        elif op == "^":
            if b.denominator != 1:
                return None
            if a == 0 and b < 0:
                raise ErreurRose("0^négatif dans une constante", ligne)
            if b > 2**63 - 1 or b < -(2**63):
                return None          # miroir mpz_fits_slong_p : trop grand → réservé à l'exécution
            r = a ** int(b)
        elif op == "div":
            if b == 0:
                raise ErreurRose("division par zéro dans une constante", ligne)
            q = a / b
            r = Fraction(q.numerator // q.denominator)     # partie entière (plafond bas)
        else:
            return None
    except ZeroDivisionError:
        return None
    if r.denominator == 1:
        return Litteral(ligne=ligne, texte=str(r.numerator), flottant=False)
    return Litteral(ligne=ligne, texte=f"{r.numerator}/{r.denominator}", flottant=False)


def _texte_flottant(x: float) -> str:
    t = repr(x)
    return t
