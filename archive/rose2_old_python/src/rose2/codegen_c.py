"""Rose v2 — génération de code C ciblant GMP.

Chaque expression est abaissée en (lignes_préfixe, référence, type).
Les temporaires mpz_t/mpq_t sont déclarés à l'entrée de fonction et relâchés à
la sortie : pas de fuite accumulée dans les boucles.
Les variables du programme principal sont globales au fichier (visibles des
fonctions, ombrageable par les paramètres/affectations locales).
"""
from __future__ import annotations
from .parser import (Programme, Signature, TypeRose, Litteral, ChaineLit, BooleenLit,
                     ListeLit, Var, Affectation, Binaire, Unaire, Factorielle, Appel,
                     Index, Bloc, Si, TantQue, Pour, Retour, Afficher, Lire,
                     ExprStatement, T_ENTIER, T_RATIONNEL, T_FLOTTANT, T_BOOLEEN,
                     T_CHAINE, T_VIDE)
from .lexer import ErreurRose
from .checker import promotion
from .runtime_c import RUNTIME_C

CODE_ELEM = {T_ENTIER: "1", T_RATIONNEL: "2", T_FLOTTANT: "3"}


def nom_c(nom: str) -> str:
    sortie = []
    for c in nom:
        if c.isascii() and (c.isalnum() or c == "_"):
            sortie.append(c)
        else:
            sortie.append(f"_x{ord(c):x}_")
    r = "".join(sortie)
    return ("_" + r) if r and r[0].isdigit() else (r or "_v")


class CodegenC:
    def __init__(self, prog: Programme, globaux: dict):
        self.prog = prog
        self.globaux = globaux                 # nom -> TypeRose (sortie du vérificateur)
        self.protos: list[str] = []
        self.externes: list[str] = []
        self.corps: list[str] = []
        self.decls_globales: list[str] = []
        self.inits_globales: list[str] = []
        self.bibliotheques: set[str] = set()
        self._compteur = 0
        self.decls: list[str] = []
        self.clears: list[str] = []
        self.env: dict[str, tuple[str, TypeRose]] = {}
        self.genv: dict[str, tuple[str, TypeRose]] = {}
        self.fonction: Signature | None = None

    # -- infra ---------------------------------------------------------------
    def _alloue(self, ident: str, t: TypeRose, globaux: bool = False) -> str:
        if t == T_ENTIER:
            self.decls.append(f"mpz_t {ident}; mpz_init({ident});")
            self.clears.append(f"mpz_clear({ident});")
        elif t == T_RATIONNEL:
            self.decls.append(f"mpq_t {ident}; mpq_init({ident});")
            self.clears.append(f"mpq_clear({ident});")
        elif t == T_FLOTTANT:
            self.decls.append(f"double {ident} = 0;")
        elif t == T_BOOLEEN:
            self.decls.append(f"int {ident} = 0;")
        elif t == T_CHAINE:
            self.decls.append(f"char* {ident} = (char*)\"\";")
        elif t.nom == "liste":
            self.decls.append(f"RoseListe* {ident} = NULL;")
        return ident

    def nouveau_temp(self, t: TypeRose) -> str:
        self._compteur += 1
        return self._alloue(f"_Rt{self._compteur}", t)

    def nouveau_var(self, nom: str, t: TypeRose) -> str:
        self._compteur += 1
        return self._alloue(f"rose_v_{nom_c(nom)}_{self._compteur}", t)

    def assurer_variable(self, nom: str, t: TypeRose) -> str:
        """Crée la variable locale si nécessaire, renvoie (ref, type) en l'enregistrant."""
        return self.nouveau_var(nom, t)

    # -- conversions -----------------------------------------------------------
    def vers_double(self, ref: str, t: TypeRose, lignes: list[str]) -> str:
        if t == T_FLOTTANT:
            return f"({ref})"
        if t == T_ENTIER:
            return f"mpz_get_d({ref})"
        if t == T_RATIONNEL:
            return f"mpq_get_d({ref})"
        if t == T_BOOLEEN:
            return f"(double)({ref})"
        raise ErreurRose(f"conversion vers flottant impossible depuis {t}")

    def vers_q(self, ref: str, t: TypeRose, lignes: list[str]) -> str:
        if t == T_RATIONNEL:
            return ref
        if t in (T_ENTIER, T_BOOLEEN):
            q = self.nouveau_temp(T_RATIONNEL)
            lignes.append(f"rose_vers_q({q}, {self.vers_z(ref, t, lignes)});")
            return q
        if t == T_FLOTTANT:
            q = self.nouveau_temp(T_RATIONNEL)
            lignes.append(f"rose_q_de_d({q}, {ref});")
            return q
        raise ErreurRose(f"conversion vers rationnel impossible depuis {t}")

    def vers_z(self, ref: str, t: TypeRose, lignes: list[str]) -> str:
        if t == T_ENTIER:
            return ref
        if t == T_BOOLEEN:
            z = self.nouveau_temp(T_ENTIER)
            lignes.append(f"mpz_set_ui({z}, (unsigned long)({ref}));")
            return z
        raise ErreurRose(f"conversion vers entier impossible depuis {t}")

    def vers_chaine(self, ref: str, t: TypeRose, lignes: list[str]) -> str:
        t_dest = self.nouveau_temp(T_CHAINE)
        if t == T_ENTIER:
            lignes.append(f"{t_dest} = rose_chaine_entier({ref});")
        elif t == T_RATIONNEL:
            lignes.append(f"{t_dest} = rose_chaine_rationnel({ref});")
        elif t == T_FLOTTANT:
            lignes.append(f"{t_dest} = rose_chaine_flottant({ref});")
        elif t == T_BOOLEEN:
            lignes.append(f"{t_dest} = rose_chaine_booleen({ref});")
        elif t == T_CHAINE:
            lignes.append(f"{t_dest} = rose_strdup({ref});")
        else:
            raise ErreurRose(f"conversion chaine impossible depuis {t}")
        return t_dest

    # -- programme ---------------------------------------------------------------
    def generer(self) -> tuple[str, set[str]]:
        for sig in self.prog.signatures.values():
            if sig.corps is None:
                self.emettre_prototype_externe(sig)
        # variables globales du programme principal
        for nom, t in self.globaux.items():
            ident = f"rose_gv_{nom_c(nom)}"
            if t == T_ENTIER:
                self.decls_globales.append(f"static mpz_t {ident};")
                self.inits_globales.append(f"mpz_init(rose_gv_{nom_c(nom)});")
            elif t == T_RATIONNEL:
                self.decls_globales.append(f"static mpq_t {ident};")
                self.inits_globales.append(f"mpq_init(rose_gv_{nom_c(nom)});")
            elif t == T_FLOTTANT:
                self.decls_globales.append(f"static double {ident} = 0;")
            elif t == T_BOOLEEN:
                self.decls_globales.append(f"static int {ident} = 0;")
            elif t == T_CHAINE:
                self.decls_globales.append(f"static char* {ident} = (char*)\"\";")
            elif t.nom == "liste":
                self.decls_globales.append(f"static RoseListe* {ident} = NULL;")
            self.genv[nom] = (ident, t)
        for sig in self.prog.signatures.values():
            self.protos.append(self.signe_prototype(sig))
        for sig in self.prog.signatures.values():
            if sig.corps is not None:
                self.corps.append(self.emettre_fonction(sig))
        corps_main = self.emettre_corps(None, self.prog.instructions)
        self.corps.append("static void rose_programme(void) {\n" + corps_main + "\n}")
        source = ("/* Généré par le compilateur Rose v2 — ne pas éditer à la main. */\n"
                  "#include <stdint.h>\n"
                  + RUNTIME_C + "\n"
                  + "\n".join(self.externes) + "\n"
                  + "\n".join(self.decls_globales) + "\n"
                  + "\n".join(self.protos) + "\n"
                  + "\n".join(self.corps) + "\n"
                  "int main(void) {\n"
                  + "\n".join("    " + i for i in self.inits_globales) + "\n"
                  "    rose_programme();\n    return 0;\n}\n")
        return source, self.bibliotheques

    def signe_retour(self, t: TypeRose) -> str:
        return {T_ENTIER: "mpz_ptr", T_RATIONNEL: "mpq_ptr", T_FLOTTANT: "double",
                T_BOOLEEN: "int", T_CHAINE: "char*", T_VIDE: "void"}.get(
            t, "RoseListe*" if t.nom == "liste" else "void")

    def type_param(self, t: TypeRose) -> str:
        return {T_ENTIER: "const mpz_t", T_RATIONNEL: "const mpq_t", T_FLOTTANT: "double",
                T_BOOLEEN: "int", T_CHAINE: "const char*", T_VIDE: "void"}.get(
            t, "const RoseListe*" if t.nom == "liste" else "void")

    def type_sortie(self, t: TypeRose) -> str:
        """mpz_t/mpq_t sont des tableaux : passés « par référence » naturellement.
        Les scalaires C (double/int/char*) exigent un pointeur de sortie."""
        return {T_ENTIER: "mpz_t", T_RATIONNEL: "mpq_t", T_FLOTTANT: "double *",
                T_BOOLEEN: "int *", T_CHAINE: "char **"}.get(
            t, "RoseListe **" if t.nom == "liste" else "void")

    def signe_prototype(self, sig: Signature) -> str:
        params = [self.type_sortie(sig.retour) + " __res"] if sig.retour not in (None, T_VIDE) else []
        params += [f"{self.type_param(t)} {nom_c(p)}" for (p, t) in sig.params]
        return f"static void rose_fn_{nom_c(sig.nom)}({', '.join(params)});"

    def emettre_prototype_externe(self, sig: Signature):
        mapping_c = {T_ENTIER: "long long", T_FLOTTANT: "double", T_BOOLEEN: "int",
                     T_CHAINE: "const char*"}
        cret = mapping_c[sig.retour]
        params = ", ".join(mapping_c[t] for (_, t) in sig.params) or "void"
        symbole = sig.symbole or sig.nom
        self.externes.append(f"extern {cret} {symbole}({params});")
        if sig.dll:
            self.bibliotheques.add(sig.dll_son or sig.dll)

    # -- corps -------------------------------------------------------------------
    def emettre_fonction(self, sig: Signature) -> str:
        self._compteur, self.decls, self.clears = 0, [], []
        self.fonction = sig
        self.env = dict(self.genv)
        for (p, t) in sig.params:
            self.env[p] = (nom_c(p), t)
        lignes = []
        for instr in sig.corps.instructions:
            self.emettre_instruction(instr, lignes)
        ret = sig.retour
        params = [f"{self.type_sortie(ret)} __res"] if ret not in (None, T_VIDE) else []
        params += [f"{self.type_param(t)} {nom_c(p)}" for (p, t) in sig.params]
        entete_c = f"static void rose_fn_{nom_c(sig.nom)}({', '.join(params)}) {{"
        return "\n".join([entete_c]
                         + ["    " + d for d in self.decls]
                         + ["    " + l for l in lignes]
                         + ["_sortie:", "    (void)0;"]
                         + ["    " + c for c in self.clears]
                         + ["}"])

    def emettre_corps(self, sig, instructions: list) -> str:
        self._compteur, self.decls, self.clears = 0, [], []
        self.fonction = None
        self.env = dict(self.genv)
        lignes = []
        for instr in instructions:
            self.emettre_instruction(instr, lignes)
        return "\n".join(["    " + d for d in self.decls] + ["    " + l for l in lignes])

    # -- variables ------------------------------------------------------------------
    def cible_de(self, nom: str) -> tuple[str, TypeRose]:
        e = self.env.get(nom) or self.genv.get(nom)
        if e is None:
            raise ErreurRose(f"variable {nom!r} introuvable en génération C")
        return e

    def type_de(self, nom: str) -> TypeRose | None:
        e = self.env.get(nom) or self.genv.get(nom)
        return e[1] if e else None

    def emettre_instruction(self, instr, lignes: list[str]):
        if isinstance(instr, Afficher):
            ref, t = self.ecrire(instr.expression, lignes)
            fn = {T_ENTIER: "rose_afficher_entier", T_RATIONNEL: "rose_afficher_rationnel",
                  T_FLOTTANT: "rose_afficher_flottant", T_BOOLEEN: "rose_afficher_booleen",
                  T_CHAINE: "rose_afficher_chaine"}.get(t)
            if fn is None:
                if t.nom == "liste":
                    lignes.append(f"if ({ref}) rose_afficher_liste({ref}); else puts(\"[]\");")
                else:
                    raise ErreurRose(f"affichage de {t} non supporté")
            else:
                lignes.append(f"{fn}({ref});")
        elif isinstance(instr, Lire):
            if instr.nom not in self.env and instr.nom not in self.genv:
                raise ErreurRose(f"variable {instr.nom!r} à lire sans type connu")
            var, t = self.cible_de(instr.nom)
            if t == T_ENTIER:
                lignes.append(f"rose_lire_entier({var});")
            elif t == T_RATIONNEL:
                lignes.append(f"rose_lire_rationnel({var});")
            elif t == T_FLOTTANT:
                lignes.append(f"{var} = rose_lire_flottant();")
            else:
                raise ErreurRose(f"lecture vers {t} impossible")
        elif isinstance(instr, Affectation):
            self.affecter(instr.nom, instr.valeur, lignes)
        elif isinstance(instr, ExprStatement):
            self.ecrire(instr.expression, lignes)
        elif isinstance(instr, Si):
            tref, _ = self.ecrire(instr.test, lignes)
            lignes.append(f"if ({tref}) {{")
            bloc = []
            for i in instr.alors.instructions:
                self.emettre_instruction(i, bloc)
            lignes.extend("    " + b for b in bloc)
            if instr.sinon is not None:
                lignes.append("} else {")
                bloc = []
                for i in instr.sinon.instructions:
                    self.emettre_instruction(i, bloc)
                lignes.extend("    " + b for b in bloc)
            lignes.append("}")
        elif isinstance(instr, TantQue):
            tref, _ = self.ecrire(instr.test, lignes)
            lignes.append(f"while ({tref}) {{")
            bloc = []
            for i in instr.corps.instructions:
                self.emettre_instruction(i, bloc)
            lignes.extend("    " + b for b in bloc)
            lignes.append("}")
        elif isinstance(instr, Pour):
            lo_ref, lo_t = self.ecrire(instr.debut, lignes)
            hi_ref, hi_t = self.ecrire(instr.fin, lignes)
            lo_z = self.vers_z(lo_ref, lo_t, lignes)
            hi_z = self.vers_z(hi_ref, hi_t, lignes)
            var = self.nouveau_var(instr.variable, T_ENTIER)   # locale à la portée
            self.env[instr.variable] = (var, T_ENTIER)
            lignes.append(f"for (mpz_set({var}, {lo_z}); mpz_cmp({var}, {hi_z}) < 0;"
                          f" mpz_add_ui({var}, {var}, 1)) {{")
            bloc = []
            for i in instr.corps.instructions:
                self.emettre_instruction(i, bloc)
            lignes.extend("    " + b for b in bloc)
            lignes.append("}")
        elif isinstance(instr, Retour):
            if self.fonction is None:
                raise ErreurRose("'retour' hors de fonction")
            if instr.valeur is not None:
                ref, t = self.ecrire(instr.valeur, lignes)
                ret = self.fonction.retour
                if ret == T_ENTIER:
                    lignes.append(f"mpz_set(__res, {self.vers_z(ref, t, lignes)});")
                elif ret == T_RATIONNEL:
                    lignes.append(f"mpq_set(__res, {self.vers_q(ref, t, lignes)});")
                elif ret == T_FLOTTANT:
                    lignes.append(f"*__res = {self.vers_double(ref, t, lignes)};")
                elif ret == T_BOOLEEN:
                    lignes.append(f"*__res = ({ref}) ? 1 : 0;")
                else:
                    lignes.append(f"*__res = {ref};")
            lignes.append("goto _sortie;")
        elif isinstance(instr, Bloc):
            for i in instr.instructions:
                self.emettre_instruction(i, lignes)
        else:
            raise ErreurRose(f"instruction non supportée en C : {type(instr).__name__}")

    def affecter(self, nom: str, e, lignes: list[str]):
        ref, t = self.ecrire(e, lignes)
        existante = self.env.get(nom) or self.genv.get(nom)
        if existante is None:
            var = self.nouveau_var(nom, t)
            self.env[nom] = (var, t)
            existante = (var, t)
        var, t_cible = existante
        if t_cible == T_ENTIER:
            lignes.append(f"mpz_set({var}, {self.vers_z(ref, t, lignes)});")
        elif t_cible == T_RATIONNEL:
            lignes.append(f"mpq_set({var}, {self.vers_q(ref, t, lignes)});")
        elif t_cible == T_FLOTTANT:
            lignes.append(f"{var} = {self.vers_double(ref, t, lignes)};")
        elif t_cible == T_BOOLEEN:
            lignes.append(f"{var} = ({ref}) ? 1 : 0;")
        else:
            lignes.append(f"{var} = {ref};")

    # -- expressions ------------------------------------------------------------------
    def ecrire(self, e, lignes: list[str]) -> tuple[str, TypeRose]:
        t: TypeRose = getattr(e, "type", None)
        if isinstance(e, Litteral):
            return self._litteral(e, lignes)
        if isinstance(e, ChaineLit):
            return self._chaine_litt(e.valeur), T_CHAINE
        if isinstance(e, BooleenLit):
            return ("1" if e.valeur else "0"), T_BOOLEEN
        if isinstance(e, Var):
            return self.cible_de(e.nom)
        if isinstance(e, ListeLit):
            return self._liste(e, lignes)
        if isinstance(e, Index):
            return self._index(e, lignes)
        if isinstance(e, Unaire):
            return self._unaire(e, lignes, t)
        if isinstance(e, Factorielle):
            ref, tref = self.ecrire(e.operande, lignes)
            dest = self.nouveau_temp(T_ENTIER)
            lignes.append(f"rose_factorielle({dest}, {self.vers_z(ref, tref, lignes)});")
            return dest, T_ENTIER
        if isinstance(e, Binaire):
            return self._binaire(e, lignes, t)
        if isinstance(e, Appel):
            return self._appel(e, lignes, t)
        raise ErreurRose(f"expression non supportée en C : {type(e).__name__}")

    def _chaine_litt(self, s: str) -> str:
        echappe = s.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n").replace("\t", "\\t")
        return f'"{echappe}"'

    def _litteral(self, e: Litteral, lignes: list[str]):
        if e.flottant:
            return f"({float(e.texte):.17e})", T_FLOTTANT
        if "/" in e.texte:
            q = self.nouveau_temp(T_RATIONNEL)
            lignes.append(f'mpq_set_str({q}, "{e.texte}", 10);')
            lignes.append(f"mpq_canonicalize({q});")
            return q, T_RATIONNEL
        z = self.nouveau_temp(T_ENTIER)
        lignes.append(f'mpz_set_str({z}, "{e.texte}", 10);')
        return z, T_ENTIER

    def _liste(self, e: ListeLit, lignes: list[str]):
        t_elt: TypeRose = e.type.element
        dest = self.nouveau_temp(e.type)
        lignes.append(f"{dest} = rose_liste_vide({CODE_ELEM.get(t_elt, '1')});")
        for elt in e.elements:
            ref, t = self.ecrire(elt, lignes)
            if t_elt == T_FLOTTANT:
                lignes.append(f"rose_liste_ajoute_flottant({dest}, {self.vers_double(ref, t, lignes)});")
            elif t_elt == T_RATIONNEL:
                lignes.append(f"rose_liste_ajoute_rationnel({dest}, {self.vers_q(ref, t, lignes)});")
            else:
                lignes.append(f"rose_liste_ajoute_entier({dest}, {self.vers_z(ref, t, lignes)});")
        return dest, e.type

    def _index(self, e: Index, lignes: list[str]):
        bref, bt = self.ecrire(e.base, lignes)
        iref, it = self.ecrire(e.index, lignes)
        if it != T_ENTIER:
            iref = self.vers_z(iref, it, lignes)
        idx = f"((size_t)mpz_get_ui({iref}))"
        t_elt: TypeRose = e.base.type.element
        if t_elt == T_FLOTTANT:
            return f"rose_liste_lire_flottant({bref}, {idx})", T_FLOTTANT
        dest = self.nouveau_temp(t_elt)
        if t_elt == T_RATIONNEL:
            lignes.append(f"rose_liste_lire_rationnel({bref}, {idx}, {dest});")
        else:
            lignes.append(f"rose_liste_lire_entier({bref}, {idx}, {dest});")
        return dest, t_elt

    def _unaire(self, e: Unaire, lignes: list[str], t: TypeRose):
        ref, tref = self.ecrire(e.operande, lignes)
        if e.op == "non":
            return f"!({ref})", T_BOOLEEN
        if tref == T_FLOTTANT or t == T_FLOTTANT:
            return f"-({self.vers_double(ref, tref, lignes)})", T_FLOTTANT
        if t == T_RATIONNEL:
            dest = self.nouveau_temp(T_RATIONNEL)
            lignes.append(f"mpq_neg({dest}, {self.vers_q(ref, tref, lignes)});")
            return dest, T_RATIONNEL
        dest = self.nouveau_temp(T_ENTIER)
        lignes.append(f"mpz_neg({dest}, {self.vers_z(ref, tref, lignes)});")
        return dest, T_ENTIER

    def _op_c(self, op: str) -> str:
        return {"<": "<", "<=": "<=", ">": ">", ">=": ">="}[op]

    def _comparaison(self, e: Binaire, lignes: list[str]):
        g, tg = self.ecrire(e.gauche, lignes)
        d, td = self.ecrire(e.droite, lignes)
        op = e.op
        egalite = op in ("==", "!=", "≈", "?=", "=~")
        if egalite and tg == T_CHAINE and td == T_CHAINE:
            expr = "(strcmp(%s, %s) == 0)" % (g, d)
            return (f"!{expr}" if op == "!=" else expr), T_BOOLEEN
        if tg == T_CHAINE and td == T_CHAINE:
            return f"(strcmp({g}, {d}) {self._op_c(op)} 0)", T_BOOLEEN
        if egalite and op != "≈" and tg.nom == "liste" and td.nom == "liste":
            dest = self.nouveau_temp(T_BOOLEEN)
            lignes.append(f"{dest} = ({g} && {d}) ? rose_liste_egales({g}, {d}) : ({g} == {d});")
            return (f"!{dest}" if op == "!=" else dest), T_BOOLEEN
        if tg == T_FLOTTANT or td == T_FLOTTANT:
            a = self.vers_double(g, tg, lignes)
            b = self.vers_double(d, td, lignes)
            if op == "≈":
                dest = self.nouveau_temp(T_BOOLEEN)
                lignes.append(f"{dest} = (fabs(({a}) - ({b})) <= 1e-9 * "
                              f"fmax(fabs({a}), fmax(fabs({b}), 1.0))) ? 1 : 0;")
                return dest, T_BOOLEEN
            if egalite:
                expr = f"(({a}) == ({b}))" if op != "!=" else f"(({a}) != ({b}))"
            else:
                expr = f"(({a}) {self._op_c(op)} ({b}))"
            return expr, T_BOOLEEN
        commun = promotion(tg, td) if (tg in (T_ENTIER, T_RATIONNEL, T_BOOLEEN)
                                       and td in (T_ENTIER, T_RATIONNEL, T_BOOLEEN)) else None
        if commun == T_RATIONNEL:
            a, b = self.vers_q(g, tg, lignes), self.vers_q(d, td, lignes)
            base, fonc = "mpq_cmp(%s, %s)", None
            if egalite:
                expr = ("(mpq_cmp(%s, %s) == 0)" % (a, b)) if op != "!=" else ("(mpq_cmp(%s, %s) != 0)" % (a, b))
                if op in ("≈", "?=", "=~"):
                    expr = "(mpq_cmp(%s, %s) == 0)" % (a, b)
            else:
                expr = ("(mpq_cmp(%s, %s) %s 0)" % (a, b, self._op_c(op)))
            return expr, T_BOOLEEN
        if commun is not None:
            a, b = self.vers_z(g, tg, lignes), self.vers_z(d, td, lignes)
            if egalite:
                expr = ("(mpz_cmp(%s, %s) == 0)" % (a, b)) if op != "!=" else ("(mpz_cmp(%s, %s) != 0)" % (a, b))
                if op in ("≈", "?=", "=~"):
                    expr = "(mpz_cmp(%s, %s) == 0)" % (a, b)
            else:
                expr = ("(mpz_cmp(%s, %s) %s 0)" % (a, b, self._op_c(op)))
            return expr, T_BOOLEEN
        raise ErreurRose(f"comparaison {tg} {op} {td} non générable")

    def _binaire(self, e: Binaire, lignes: list[str], t: TypeRose):
        op = e.op
        if op in ("et", "ou"):
            g, _ = self.ecrire(e.gauche, lignes)
            d, _ = self.ecrire(e.droite, lignes)
            return f"(({g}) {'&&' if op == 'et' else '||'} ({d}))", T_BOOLEEN
        if op in ("<", "<=", ">", ">=", "==", "!=", "≈", "?=", "=~"):
            return self._comparaison(e, lignes)
        g, tg = self.ecrire(e.gauche, lignes)
        d, td = self.ecrire(e.droite, lignes)
        if op == "+" and tg == T_CHAINE and td == T_CHAINE:
            dest = self.nouveau_temp(T_CHAINE)
            lignes.append(f"{dest} = rose_chaine_concat({g}, {d});")
            return dest, T_CHAINE
        if op == "div":
            if t == T_FLOTTANT:
                a = self.vers_double(g, tg, lignes)
                b = self.vers_double(d, td, lignes)
                return f"(floor(rose_div_d({a}, {b})))", T_FLOTTANT
            a, b = self.vers_q(g, tg, lignes), self.vers_q(d, td, lignes)
            dest = self.nouveau_temp(T_ENTIER)
            lignes.append(f"rose_div_q_floor({dest}, {a}, {b});")
            return dest, T_ENTIER
        if t == T_FLOTTANT:
            a = self.vers_double(g, tg, lignes)
            b = self.vers_double(d, td, lignes)
            code = {"+": f"({a} + {b})", "-": f"({a} - {b})", "*": f"({a} * {b})",
                    "/": f"rose_div_d({a}, {b})", "^": f"pow({a}, {b})"}[op]
            return f"({code})", T_FLOTTANT
        if t == T_RATIONNEL:
            a, b = self.vers_q(g, tg, lignes), self.vers_q(d, td, lignes)
            dest = self.nouveau_temp(T_RATIONNEL)
            if op == "+":
                lignes.append(f"mpq_add({dest}, {a}, {b});")
            elif op == "-":
                lignes.append(f"mpq_sub({dest}, {a}, {b});")
            elif op == "*":
                lignes.append(f"mpq_mul({dest}, {a}, {b});")
            elif op == "/":
                lignes.append(f"rose_q_div({dest}, {a}, {b});")
            elif op == "^":
                ez = self.vers_z(d, td, lignes)
                lignes.append(f"rose_pow_rationnel({dest}, {a}, {ez});")
            else:
                raise ErreurRose(f"opérateur {op} sur rationnel non supporté")
            return dest, T_RATIONNEL
        a, b = self.vers_z(g, tg, lignes), self.vers_z(d, td, lignes)
        dest = self.nouveau_temp(T_ENTIER)
        if op == "+":
            lignes.append(f"mpz_add({dest}, {a}, {b});")
        elif op == "-":
            lignes.append(f"mpz_sub({dest}, {a}, {b});")
        elif op == "*":
            lignes.append(f"mpz_mul({dest}, {a}, {b});")
        elif op == "%":
            lignes.append(f"rose_mod({dest}, {a}, {b});")
        elif op == "^":
            lignes.append(f"rose_pow_entier({dest}, {a}, {b});")
        else:
            raise ErreurRose(f"opérateur {op!r} non supporté en entier")
        return dest, T_ENTIER

    def _appel(self, e: Appel, lignes: list[str], t: TypeRose):
        if e.module == "dynamique":
            return self._dynamique(e, lignes, t)
        sig = self.prog.signatures.get(e.nom)
        if sig is not None and sig.corps is None:
            return self._externe(e, sig, lignes, t)
        refs, types = [], []
        for arg in e.args:
            r, ta = self.ecrire(arg, lignes)
            refs.append(r)
            types.append(ta)
        if sig is not None:
            return self._appel_utilisateur(e.nom, sig, refs, types, lignes, t)
        return self._appel_stdlib(e.nom, refs, types, e, lignes, t)

    def _appel_utilisateur(self, nom, sig, refs, types, lignes, t):
        dest = self.nouveau_temp(t) if t != T_VIDE else None
        args = []
        for arg, ta, (_, t_att) in zip(refs, types, sig.params):
            if t_att == T_FLOTTANT:
                args.append(self.vers_double(arg, ta, lignes))
            elif t_att == T_RATIONNEL:
                args.append(self.vers_q(arg, ta, lignes))
            elif t_att == T_ENTIER:
                args.append(self.vers_z(arg, ta, lignes))
            elif t_att == T_BOOLEEN:
                args.append(f"(({arg}) ? 1 : 0)")
            else:
                args.append(arg)
        appels = ([] if dest is None else
                  [f"&{dest}" if (t in (T_FLOTTANT, T_BOOLEEN, T_CHAINE) or t.nom == "liste")
                   else dest]) + args
        lignes.append(f"rose_fn_{nom_c(nom)}({', '.join(appels)});")
        return (dest, t) if t != T_VIDE else ("0", T_VIDE)

    def _z(self, i, refs, types, lignes):
        return self.vers_z(refs[i], types[i], lignes)

    def _appel_stdlib(self, nom, refs, types, e: Appel, lignes, t):
        g = refs[0] if refs else None
        tg = types[0] if types else None
        if nom == "afficher":
            fn = {T_ENTIER: "rose_afficher_entier", T_RATIONNEL: "rose_afficher_rationnel",
                  T_FLOTTANT: "rose_afficher_flottant", T_BOOLEEN: "rose_afficher_booleen",
                  T_CHAINE: "rose_afficher_chaine"}.get(tg)
            lignes.append(f"{fn or 'rose_afficher_liste'}({g});")
            return "0", T_VIDE
        if nom == "saisir_entier":
            dest = self.nouveau_temp(T_ENTIER)
            lignes.append(f"rose_lire_entier({dest});")
            return dest, T_ENTIER
        if nom == "saisir_flottant":
            return "rose_lire_flottant()", T_FLOTTANT
        if nom == "factorielle":
            dest = self.nouveau_temp(T_ENTIER)
            lignes.append(f"rose_factorielle({dest}, {self._z(0, refs, types, lignes)});")
            return dest, T_ENTIER
        if nom == "fibonacci":
            dest = self.nouveau_temp(T_ENTIER)
            lignes.append(f"rose_fibonacci({dest}, {self._z(0, refs, types, lignes)});")
            return dest, T_ENTIER
        if nom == "pgcd":
            dest = self.nouveau_temp(T_ENTIER)
            lignes.append(f"mpz_gcd({dest}, {self._z(0, refs, types, lignes)},"
                          f" {self._z(1, refs, types, lignes)});")
            return dest, T_ENTIER
        if nom == "ppcm":
            dest = self.nouveau_temp(T_ENTIER)
            lignes.append(f"mpz_lcm({dest}, {self._z(0, refs, types, lignes)},"
                          f" {self._z(1, refs, types, lignes)});")
            return dest, T_ENTIER
        if nom == "puissance":
            ez = self._z(1, refs, types, lignes)
            if tg == T_FLOTTANT:
                return (f"pow({self.vers_double(g, tg, lignes)}, mpz_get_si({ez}))", T_FLOTTANT)
            dest = self.nouveau_temp(t)
            if t == T_RATIONNEL:
                lignes.append(f"rose_pow_rationnel({dest}, {self.vers_q(g, tg, lignes)}, {ez});")
            else:
                lignes.append(f"rose_pow_entier({dest}, {self._z(0, refs, types, lignes)}, {ez});")
            return dest, t
        if nom == "est_premier":
            dest = self.nouveau_temp(T_BOOLEEN)
            lignes.append(f"{dest} = rose_est_premier({self._z(0, refs, types, lignes)});")
            return dest, T_BOOLEEN
        if nom == "compter_premiers":
            dest = self.nouveau_temp(T_ENTIER)
            lignes.append(f"rose_compter_premiers({dest}, {self._z(0, refs, types, lignes)});")
            return dest, T_ENTIER
        if nom == "nb_chiffres":
            dest = self.nouveau_temp(T_ENTIER)
            lignes.append(f"rose_nb_chiffres({dest}, {self._z(0, refs, types, lignes)});")
            return dest, T_ENTIER
        if nom == "nombre_pi":
            dest = self.nouveau_temp(T_CHAINE)
            lignes.append(f"{dest} = rose_pi({self._z(0, refs, types, lignes)});")
            return dest, T_CHAINE
        if nom == "longueur":
            dest = self.nouveau_temp(T_ENTIER)
            lignes.append(f"mpz_set_ui({dest}, ({g} ? {g}->n : 0));")
            return dest, T_ENTIER
        if nom == "joindre":
            t_liste: TypeRose = e.args[0].type
            t_elt = t_liste.element
            if t_elt == T_FLOTTANT:
                lignes.append(f"rose_liste_ajoute_flottant({g},"
                              f" {self.vers_double(refs[1], types[1], lignes)});")
            elif t_elt == T_RATIONNEL:
                lignes.append(f"rose_liste_ajoute_rationnel({g},"
                              f" {self.vers_q(refs[1], types[1], lignes)});")
            else:
                lignes.append(f"rose_liste_ajoute_entier({g},"
                              f" {self.vers_z(refs[1], types[1], lignes)});")
            return "0", T_VIDE
        if nom == "abs":
            if tg == T_FLOTTANT:
                return f"fabs({g})", T_FLOTTANT
            dest = self.nouveau_temp(t)
            if t == T_RATIONNEL:
                lignes.append(f"mpq_abs({dest}, {self.vers_q(g, tg, lignes)});")
            else:
                lignes.append(f"mpz_abs({dest}, {self._z(0, refs, types, lignes)});")
            return dest, t
        if nom in ("min", "max"):
            if t == T_FLOTTANT:
                a = self.vers_double(g, tg, lignes)
                b = self.vers_double(refs[1], types[1], lignes)
                comp = "<" if nom == "min" else ">"
                return f"(({a}) {comp} ({b}) ? {a} : {b})", T_FLOTTANT
            if t == T_RATIONNEL:
                a, b = self.vers_q(g, tg, lignes), self.vers_q(refs[1], types[1], lignes)
                cmpf, copier = "mpq_cmp", "mpq_set"
            else:
                a, b = self._z(0, refs, types, lignes), self._z(1, refs, types, lignes)
                cmpf, copier = "mpz_cmp", "mpz_set"
            dest = self.nouveau_temp(t)
            cond = "<" if nom == "min" else ">"
            lignes.append(f"{copier}({dest}, {a});")
            lignes.append(f"if ({cmpf}({b}, {a}) {cond} 0) {copier}({dest}, {b});")
            return dest, t
        if nom == "partie_entiere":
            dest = self.nouveau_temp(T_ENTIER)
            if tg == T_RATIONNEL:
                lignes.append(f"rose_partie_entiere({dest}, {g});")
            elif tg == T_FLOTTANT:
                lignes.append(f"mpz_set_d({dest}, floor({g}));")
            else:
                lignes.append(f"mpz_set({dest}, {g});")
            return dest, T_ENTIER
        if nom == "fraction":
            dest = self.nouveau_temp(T_RATIONNEL)
            lignes.append(f"rose_fraction({dest}, {self._z(0, refs, types, lignes)},"
                          f" {self._z(1, refs, types, lignes)});")
            return dest, T_RATIONNEL
        if nom == "entier":
            dest = self.nouveau_temp(T_ENTIER)
            if tg == T_CHAINE:
                lignes.append(f"rose_chaine_vers_entier({dest}, {g});")
            elif tg == T_RATIONNEL:
                lignes.append(f"rose_entier_de_q({dest}, {g});")
            elif tg == T_FLOTTANT:
                lignes.append(f"mpz_set_d({dest}, floor({g}));")
            else:
                lignes.append(f"mpz_set({dest}, {g});")
            return dest, T_ENTIER
        if nom == "rationnel":
            dest = self.nouveau_temp(T_RATIONNEL)
            if tg == T_FLOTTANT:
                lignes.append(f"rose_q_de_d({dest}, {g});")
            else:
                lignes.append(f"rose_vers_q({dest}, {self._z(0, refs, types, lignes)});")
            return dest, T_RATIONNEL
        if nom == "flottant":
            return self.vers_double(g, tg, lignes), T_FLOTTANT
        if nom == "chaine":
            return self.vers_chaine(g, tg, lignes), T_CHAINE
        raise ErreurRose(f"fonction {nom!r} inconnue en génération C")

    def _externe(self, e: Appel, sig: Signature, lignes, t: TypeRose):
        symbole = sig.symbole or sig.nom
        args = []
        for arg, (_, t_att) in zip(e.args, sig.params):
            r, ta = self.ecrire(arg, lignes)
            if t_att == T_ENTIER:
                args.append(f"(long long)mpz_get_si({self.vers_z(r, ta, lignes)})")
            elif t_att == T_FLOTTANT:
                args.append(self.vers_double(r, ta, lignes))
            elif t_att == T_BOOLEEN:
                args.append(f"(({r}) ? 1 : 0)")
            else:
                args.append(r)
        appel = f"{symbole}({', '.join(args)})"
        if t == T_ENTIER:
            dest = self.nouveau_temp(T_ENTIER)
            lignes.append(f"mpz_set_si({dest}, (long long)({appel}));")
            return dest, T_ENTIER
        if t == T_FLOTTANT:
            return f"({appel})", T_FLOTTANT
        if t == T_BOOLEEN:
            dest = self.nouveau_temp(T_BOOLEEN)
            lignes.append(f"{dest} = ({appel}) ? 1 : 0;")
            return dest, T_BOOLEEN
        if t == T_CHAINE:
            dest = self.nouveau_temp(T_CHAINE)
            lignes.append(f"{dest} = rose_strdup({appel});")
            return dest, T_CHAINE
        raise ErreurRose(f"type de retour externe {t} non supporté")

    def _dynamique(self, e: Appel, lignes, t: TypeRose):
        if e.nom == "charger":
            h_ref, _ = self.ecrire(e.args[0], lignes)
            dest = self.nouveau_temp(T_ENTIER)
            lignes.append(f"mpz_set_ui({dest}, (unsigned long)(size_t)rose_dyn_charger({h_ref}));")
            return dest, T_ENTIER
        if e.nom == "fermer":
            h_ref, ht = self.ecrire(e.args[0], lignes)
            lignes.append(f"rose_dyn_fermer((void*)(size_t)mpz_get_ui({self.vers_z(h_ref, ht, lignes)}));")
            return "0", T_VIDE
        h_ref, ht = self.ecrire(e.args[0], lignes)
        h = f"(void*)(size_t)mpz_get_ui({self.vers_z(h_ref, ht, lignes)})"
        n_ref, _ = self.ecrire(e.args[1], lignes)
        if e.nom == "appeler0":
            return f"rose_dyn_appeler0({h}, {n_ref})", T_FLOTTANT
        if e.nom == "appeler1":
            a_ref, at = self.ecrire(e.args[2], lignes)
            return f"rose_dyn_appeler1({h}, {n_ref}, {self.vers_double(a_ref, at, lignes)})", T_FLOTTANT
        if e.nom == "appeler2":
            a_ref, at = self.ecrire(e.args[2], lignes)
            b_ref, bt = self.ecrire(e.args[3], lignes)
            return (f"rose_dyn_appeler2({h}, {n_ref}, {self.vers_double(a_ref, at, lignes)},"
                    f" {self.vers_double(b_ref, bt, lignes)})", T_FLOTTANT)
        raise ErreurRose(f"dynamique::{e.nom} inconnu")


def generer_c(prog: Programme, globaux: dict) -> tuple[str, set[str]]:
    return CodegenC(prog, globaux).generer()
