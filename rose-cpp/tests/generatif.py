#!/usr/bin/env python3
"""Tests différentiels génératifs — v3.13 : interprète == JIT == AOT.

Le générateur produit des programmes Rose VALIDES et TYPÉS à partir d'une
grammaire sûre (arithmétique exacte, rationnels, flottants, chaînes,
booléens, si/pour/tant_que, fonctions déclarées, duree littérale). Chaque
programme est lancé de trois façons et les sorties sont comparées OCTET
pour OCTET :

  1. interprète pur   (ROSE_NO_JIT=1)   — la spec
  2. moteur complet   (JIT actif)       — doit reproduire la spec
  3. binaire AOT      (rose build + run)— doit reproduire la spec

Un rejet poli de l'AOT (fonction hors transpileur) est COMPTÉ, pas puni :
c'est la couverture, pas une régression. Une divergence de SORTIE entre
les trois voies est une régression réelle — le programme fautif et sa
graine sont imprimés tels quels pour reproduction.

Usage : python3 generatif.py [--n 100] [--graine 0]
      ROSE_MOTEUR=... pour cibler un autre binaire C++.
"""
import argparse
import os
import random
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
MOTEUR = os.environ.get("ROSE_MOTEUR") or os.path.join(HERE, "..", "cpp", "rose")
ENV = dict(os.environ)
if "ROSE_GMP" not in ENV and os.path.exists("/tmp/opencode/gmp-linux/include"):
    ENV["ROSE_GMP"] = "/tmp/opencode/gmp-linux"
TMP = tempfile.mkdtemp(prefix="rose-gen-")

# ── lexique ─────────────────────────────────────────────────────────────
NOMS_VAR = ["alpha", "beta", "gamma", "delta", "epsil", "zeta", "eta", "theta"]
CHAINES = ["rose", "petale", "épine", "jardin", "a", "abracadabra", "ü", "bonjour"]
# (les noms en t-chiffre sont réservés par le moteur — proscrits ici)


class Gen:
    """Générateur typé : chaque expression rend (texte, type) avec type dans
    I entier, Q rationnel, F flottant, S chaîne, B booléen — exactement les
    règles de promotion du langage (F domine Q, Q domine I)."""

    def __init__(self, rnd):
        self.rnd = rnd
        self.vars = {}          # nom -> type
        self.fns = []           # [(nom, nb_params)]
        self.lignes = []

    # ── littéraux ──────────────────────────────────────────────────────
    def lit_I(self):
        r = self.rnd.random()
        if r < 0.12:
            return str(self.rnd.randint(10 ** self.rnd.randint(12, 18),
                                        10 ** self.rnd.randint(19, 24)))   # GMP
        if r < 0.2:
            return str(-self.rnd.randint(1, 999))
        return str(self.rnd.randint(0, 60))

    def lit_F(self):
        v = self.rnd.uniform(-50, 50)
        return f"{v:.4f}"

    def denom(self):
        # dénominateur : rarement 0 → les trois moteurs doivent rejeter pareil
        return self.rnd.randint(1, 9) if self.rnd.random() < 0.94 else 0

    # ── expressions ────────────────────────────────────────────────────
    def expr(self, veut, prof):
        if prof <= 0:
            return (self.atome(veut), veut)
        choix = self.rnd.random()
        if choix < 0.30:
            return (self.atome(veut), veut)
        if veut in ("I", "Q", "F"):
            return self.arith(veut, prof)
        if veut == "B":
            return self.booleen(prof)
        if veut == "S":
            return self.chaine_expr(prof)
        return self.atome(veut)

    def atome(self, veut):
        candidats = [n for n, t in self.vars.items() if t == veut]
        if candidats and self.rnd.random() < 0.45:
            return self.rnd.choice(candidats)
        return self.lit_pour(veut)

    def lit_pour(self, veut):
        if veut == "I":
            return self.lit_I()
        if veut == "Q":
            return f"{self.rnd.randint(-30, 30)} / {self.rnd.randint(1, 17)}"
        if veut == "F":
            return self.lit_F()
        if veut == "S":
            return '"' + self.rnd.choice(CHAINES) + '"'
        return self.rnd.choice(["vrai", "faux"])

    def arith(self, veut, prof):
        r = self.rnd.random()
        # promotion : un F ailleurs → F ; Q ailleurs → Q ; sinon I
        if veut == "F":
            g = lambda: self.expr(self.rnd.choice(["F", "I", "Q"]), prof - 1)
            a, ta = g(); b, tb = g()
            if r < 0.12:
                return f"abs({a})", "F" if ta == "F" else ("Q" if ta == "Q" else "F")
            op = self.rnd.choice(["+", "-", "*", "/", "^"])
            return f"({a} {op} {b})", "F"
        if veut == "Q":
            a, ta = self.expr(self.rnd.choice(["Q", "I", "Q"]), prof - 1)
            b, tb = self.expr(self.rnd.choice(["Q", "I"]), prof - 1)
            if r < 0.18:
                return f"({a} div {b})", "I" if ta != "Q" and tb != "Q" else "Q"
            if r < 0.30:
                return f"({a} % {b})", "I" if ta != "Q" and tb != "Q" else "Q"
            if r < 0.40:
                e = self.rnd.randint(-2, 4)
                return f"({a} ^ {e})", "Q"
            if r < 0.5:
                return (f"pgcd({self.expr('I', prof - 1)[0]}, "
                        f"{self.expr('I', prof - 1)[0]})", "I")
            if r < 0.56:
                return (f"ppcm({self.expr('I', prof - 1)[0]}, "
                        f"{self.expr('I', prof - 1)[0]})", "I")
            if r < 0.62:
                return f"abs({a})", "Q" if ta == "Q" else "I"
            if r < 0.68:
                n = self.rnd.randint(0, 12)
                return f"factorielle({n})", "I"
            if r < 0.74:
                n = self.rnd.randint(0, 40)
                return f"fibonacci({n})", "I"
            if r < 0.8:
                return f"nb_chiffres({self.expr('I', prof - 1)[0]})", "I"
            if r < 0.86:
                m = self.expr("I", prof - 1)[0]; x = self.expr("I", prof - 1)[0]
                return f"{self.rnd.choice(['min', 'max'])}({m}, {x})", "I"
            op = self.rnd.choice(["+", "-", "*", "/"])
            return f"({a} {op} {b})", ("Q" if (ta == "Q" or tb == "Q" or op == "/") else "I")
        # veut == "I"
        if r < 0.16:
            a, ta = self.expr("I", prof - 1)
            b, tb = self.expr("I", prof - 1)
            return f"({a} div {b})", "I"
        if r < 0.32:
            a, ta = self.expr("I", prof - 1)
            b, tb = self.expr("I", prof - 1)
            return f"({a} % {b})", "I"
        if r < 0.44:
            a, _ = self.expr("I", prof - 1)
            e = self.rnd.randint(0, 4)
            return f"({a} ^ {e})", "I"
        if r < 0.52:
            return (f"pgcd({self.expr('I', prof - 1)[0]}, "
                    f"{self.expr('I', prof - 1)[0]})", "I")
        if r < 0.58:
            return (f"ppcm({self.expr('I', prof - 1)[0]}, "
                    f"{self.expr('I', prof - 1)[0]})", "I")
        if r < 0.64:
            return f"abs({self.expr('I', prof - 1)[0]})", "I"
        if r < 0.70:
            return f"factorielle({self.rnd.randint(0, 15)})", "I"
        if r < 0.76:
            return f"fibonacci({self.rnd.randint(0, 60)})", "I"
        if r < 0.82:
            return f"nb_chiffres({self.expr('I', prof - 1)[0]})", "I"
        if r < 0.88:
            return f"{self.rnd.choice(['min', 'max'])}({self.expr('I', prof - 1)[0]}, {self.expr('I', prof - 1)[0]})", "I"
        if r < 0.94:
            s = self.rnd.choice(CHAINES)
            d = self.rnd.randint(0, max(0, len(s) - 1))
            f = self.rnd.randint(d, len(s))
            return f"longueur(\"{s}\")", "I"
        return self.lit_I(), "I"

    def booleen(self, prof):
        r = self.rnd.random()
        if r < 0.42:
            num = ["I", "Q", "F"]
            t = self.rnd.choice(num)
            a, _ = self.expr(t, prof - 1); b, _ = self.expr(t, prof - 1)
            op = self.rnd.choice(["==", "!=", "<", "<=", ">", ">="])
            return f"({a} {op} {b})", "B"
        if r < 0.52:
            a, _ = self.expr("S", prof - 1); b, _ = self.expr("S", prof - 1)
            return f"({a} {self.rnd.choice(['==', '!='])} {b})", "B"
        if r < 0.62:
            return f"est_premier({self.expr('I', prof - 1)[0]})", "B"
        if r < 0.72:
            a, _ = self.booleen(prof - 1); b, _ = self.booleen(prof - 1)
            return f"({a} {self.rnd.choice(['et', 'ou'])} {b})", "B"
        if r < 0.8:
            a, _ = self.booleen(prof - 1)
            return f"(non {a})", "B"
        return self.rnd.choice(["vrai", "faux"]), "B"

    def chaine_expr(self, prof):
        r = self.rnd.random()
        if r < 0.45:
            a, _ = self.expr("I", prof - 1)
            return f"chaine({a})", "S"
        if r < 0.7:
            s = self.rnd.choice(CHAINES)
            if len(s) >= 2:
                d = self.rnd.randint(0, len(s) - 2)
                f = self.rnd.randint(d + 1, len(s))
                return f'sous_chaine("{s}", {d}, {f})', "S"
            return f'"{s}"', "S"
        return '"' + self.rnd.choice(CHAINES) + '"', "S"

    # ── instructions ───────────────────────────────────────────────────
    def bloc(self, profondeur, budget, boucle_var=None):
        rendu = []
        for _ in range(self.rnd.randint(1, budget)):
            rendu += self.instr(profondeur, boucle_var)
        return rendu

    def instr(self, profondeur, boucle_var=None):
        r = self.rnd.random()
        if r < 0.30 or profondeur > 2:
            # affectation
            veut = self.rnd.choice(["I", "Q", "F", "S", "B"])
            connus = [n for n, t in self.vars.items() if t == veut]
            if connus and self.rnd.random() < 0.5:
                nom = self.rnd.choice(connus)
            else:
                libre = [n for n in NOMS_VAR if n not in self.vars]
                if not libre:
                    nom = f"v{len(self.vars)}"
                else:
                    nom = self.rnd.choice(libre)
            e, t = self.expr(veut, profondeur + 1)
            self.vars[nom] = t
            return [f"${nom} = {e}"]
        if r < 0.45:
            # sortie
            veut = self.rnd.choice(["I", "Q", "F", "S", "B", "Q", "I"])
            e, _ = self.expr(veut, profondeur + 1)
            return [f">> {e}"]
        if r < 0.50:
            # duree littérale (déterministe, v3.13)
            n = self.rnd.choice([0, 1, 42, 999, 1000, 1234567, 59_000_000_000,
                                 3_600_000_000_000, 73849237489234723423])
            return [f">> duree({n})"]
        if r < 0.62 and profondeur <= 2:
            # si / sinon
            c, _ = self.booleen(2)
            out = [f"si ({c}) {{"]
            out += ["  " + l for l in self.bloc(profondeur + 1, 2, boucle_var)]
            if self.rnd.random() < 0.6:
                c2, _ = self.booleen(2)
                out.append(f"}} sinon si ({c2}) {{")
                out += ["  " + l for l in self.bloc(profondeur + 1, 2, boucle_var)]
            if self.rnd.random() < 0.5:
                out.append("} sinon {")
                out += ["  " + l for l in self.bloc(profondeur + 1, 2, boucle_var)]
            out.append("}")
            return out
        if r < 0.74 and profondeur <= 1:
            # pour borné
            v = self.rnd.choice(["k", "m", "p"])
            a = self.rnd.randint(0, 5)
            b = a + self.rnd.randint(1, 12)
            nom = self.rnd.choice([n for n in NOMS_VAR if n not in self.vars] or ["acc"])
            self.vars[f"somme_{nom}"] = "I"
            out = [f"$somme_{nom} = 0", f"pour {v} de {a} vers {b} {{",
                   f"  $somme_{nom} = $somme_{nom} + {v}", "}",
                   f">> $somme_{nom}"]
            return out
        if r < 0.82 and profondeur <= 1:
            # tant_que qui termine : compteur décroissant
            nom = self.rnd.choice([n for n in NOMS_VAR if n not in self.vars] or ["c"])
            debut = self.rnd.randint(1, 6)
            out = [f"${nom} = {debut}", f"tant_que (${nom} > 0) {{",
                   f"  ${nom} = ${nom} - 1"]
            self.vars[nom] = "I"
            out.append("}")
            out.append(f">> ${nom}")
            return out
        if r < 0.90 and self.fns:
            nom, np = self.rnd.choice(self.fns)
            args = ", ".join(self.expr("I", 1)[0] for _ in range(np))
            return [f">> {nom}({args})"]
        # sortie simple
        e, _ = self.expr("I", profondeur + 1)
        return [f">> {e}"]

    def fonction(self):
        nom = f"fleur{len(self.fns)}"
        np = self.rnd.randint(1, 2)
        params = [f"p{i}: entier" for i in range(np)]
        corps = []
        if self.rnd.random() < 0.5:
            corps.append(f"  $local = p0")
            e = "$local * 2 + 1"
        elif self.rnd.random() < 0.5:
            corps.append("  si (p0 > p1) {")
            corps.append("    retour p0 - p1")
            corps.append("  }")
            e = "p1 - p0"
        else:
            e = f"pgcd(p0, {self.rnd.randint(2, 60)}) + p0"
        corps.append(f"  retour {e}")
        sig = f"fn {nom}({', '.join(params)}) → entier {{\n" + "\n".join(corps) + "\n}"
        self.fns.append((nom, np))
        return [sig]

    def programme(self):
        lignes = []
        for _ in range(self.rnd.randint(0, 2)):
            lignes += self.fonction()
        for _ in range(self.rnd.randint(6, 14)):
            lignes += self.instr(1)
        return "\n".join(lignes) + "\n"


# ── exécution des trois voies ───────────────────────────────────────────
def lancer(cmd, env=None):
    try:
        p = subprocess.run(cmd, capture_output=True, timeout=20,
                           env=env or ENV, cwd=TMP)
        return (p.returncode, p.stdout.decode("utf-8", "replace"),
                p.stderr.decode("utf-8", "replace"))
    except subprocess.TimeoutExpired:
        return -99, "<EXPIRÉ>", ""


def programme_test(graine):
    g = Gen(random.Random(graine))
    src = g.programme()
    chemin = os.path.join(TMP, f"g{graine}.rose")
    with open(chemin, "w", encoding="utf-8") as f:
        f.write(src)
    rc_i, out_i, err_i = lancer([MOTEUR, chemin], env={**ENV, "ROSE_NO_JIT": "1"})
    rc_j, out_j, err_j = lancer([MOTEUR, chemin])
    if out_j != out_i or rc_j != rc_i or err_j != err_i:
        chemin2 = f"/tmp/opencode/jit_divergence_{graine}.rose"
        with open(chemin2, "w", encoding="utf-8") as f2:
            f2.write(src)
        return "DIVERGENCE-JIT", src, f"jit rc={rc_j} vs interp rc={rc_i} — figé {chemin2}"
    exe = os.path.join(TMP, f"g{graine}.exe")
    rc_b, out_b, err_b = lancer([MOTEUR, "build", chemin, "-o", exe])
    if rc_b != 0:
        motif = ((out_b + err_b).strip().splitlines() or ["<vide>"])[-1][:110]
        if "rose build:" in motif:
            # le checker a accepté, g++ a échoué : violation du contrat
            # « émettre ou rejeter poliment » — programme figé pour enquête.
            chemin2 = f"/tmp/opencode/aot_echec_{graine}.rose"
            with open(chemin2, "w", encoding="utf-8") as f2:
                f2.write(src)
            return "ECHEC-GPP", src, chemin2
        return "REFUS-AOT", src, motif
    rc_a, out_a, err_a = lancer([exe])
    if rc_i != 0:
        # erreur runtime : l'AOT doit rejeter aussi (même code, même message)
        if rc_a == 0:
            chemin2 = f"/tmp/opencode/aot_divergence_{graine}.rose"
            with open(chemin2, "w", encoding="utf-8") as f2:
                f2.write(src)
            return "DIVERGENCE-AOT", src, f"interprète rejette, AOT calcule — figé {chemin2}"
        if (out_a, err_a) != (out_i, err_i):
            chemin2 = f"/tmp/opencode/aot_divergence_{graine}.rose"
            with open(chemin2, "w", encoding="utf-8") as f2:
                f2.write(src)
            return "DIVERGENCE-AOT", src, f"message runtime divergent — figé {chemin2}"
        return "OK", src, ""
    if rc_a != 0 or out_a != out_i:
        chemin2 = f"/tmp/opencode/aot_divergence_{graine}.rose"
        with open(chemin2, "w", encoding="utf-8") as f2:
            f2.write(src)
        return "DIVERGENCE-AOT", src, f"aot rc={rc_a} vs interp rc={rc_i} — figé {chemin2}"
    return "OK", src, ""


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--n", type=int, default=100)
    ap.add_argument("--graine", type=int, default=0)
    args = ap.parse_args()

    stats = {"OK": 0, "REFUS-AOT": 0, "ECHEC-GPP": 0, "DIVERGENCE-JIT": 0,
             "DIVERGENCE-AOT": 0, "DIVERS": 0}
    motifs = {}
    for i in range(args.n):
        try:
            etat, src, det = programme_test(args.graine * 100000 + i)
        except Exception as e:                      # harness lui-même
            etat, src, det = "DIVERS", "", repr(e)
        stats[etat] += 1
        if etat == "REFUS-AOT":
            motifs[det] = motifs.get(det, 0) + 1
        if (etat.startswith("DIVERGENCE") or etat == "DIVERS"
                or etat == "ECHEC-GPP"):
            print(f"\n══ {etat} (graine {args.graine * 100000 + i}) : {det}")
            print(src)
    tot = sum(stats.values())
    print(f"\nDifférentiel génératif : {tot} programmes — "
          f"{stats['OK']} identiques, {stats['REFUS-AOT']} rejets AOT, "
          f"{stats['ECHEC-GPP']} échecs g++, "
          f"{stats['DIVERGENCE-JIT']} divergences JIT, "
          f"{stats['DIVERGENCE-AOT']} divergences AOT, {stats['DIVERS']} incidents")
    if motifs:
        print("Couverture AOT — motifs de rejet :")
        for m, c in sorted(motifs.items(), key=lambda kv: -kv[1]):
            print(f"  {c:4d}  {m}")
    return 0 if stats["DIVERGENCE-JIT"] == 0 and stats["DIVERGENCE-AOT"] == 0 \
        and stats["DIVERS"] == 0 and stats["ECHEC-GPP"] == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
