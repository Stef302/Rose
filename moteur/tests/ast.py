#!/usr/bin/env python3
"""ast.py — jalon M4.1 : l'oracle d'arbre syntaxique à l'oeuvre.

Compare la sortie de `rose ast <f>` (oracle C++, Parser + ast_programme)
à celle de `rose rose/analyseur.rose <f>` (analyseur auto-hébergé en Rose)
puis au binaire AOT de ce même analyseur, sur tout le corpus : les dorés,
les roses, les bancs et les deux fichiers de rose/ — l'analyseur se lit
lui-même : exigence du bootstrap M4.

Porte triple, même discipline qu'au palier M4.0 (lexeur.py) : texte,
interprétation, binaire natif. Une seule divergence = échec.

Usage : python3 moteur/tests/ast.py [moteur]
"""
import glob
import os
import subprocess
import sys

RACINE = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
MOTEUR = sys.argv[1] if len(sys.argv) > 1 else os.path.join(RACINE, "moteur", "cpp", "rose")
ANALYSEUR = os.path.join(RACINE, "rose", "analyseur.rose")
LEXEUR = os.path.join(RACINE, "rose", "lexeur.rose")
ENV = dict(os.environ, ROSE_GMP="/tmp/opencode/gmp-linux")


def lancer(cmd):
    p = subprocess.run(cmd, capture_output=True, text=True, env=ENV)
    return p.stdout, p.stderr, p.returncode


def corpus():
    fs = []
    fs += sorted(glob.glob(os.path.join(RACINE, "moteur", "tests", "dorés", "*.rose")))
    fs += sorted(glob.glob(os.path.join(RACINE, "moteur", "tests", "roses", "*.rose")))
    fs += sorted(glob.glob(os.path.join(RACINE, "moteur", "tests", "bench", "*.rose")))
    fs += sorted(glob.glob("/tmp/opencode/aot_divergence_*.rose"))
    fs += sorted(glob.glob("/tmp/opencode/*divergence*.rose"))
    fs.append(LEXEUR)      # l'analyseur lit le lexeur
    fs.append(ANALYSEUR)   # l'analyseur se lit lui-même : point fixe du jalon
    return fs


def premiere_ligne_divergente(a, b):
    a, b = a.splitlines(), b.splitlines()
    for i in range(max(len(a), len(b))):
        xa = a[i] if i < len(a) else "<fin>"
        xb = b[i] if i < len(b) else "<fin>"
        if xa != xb:
            return f"ligne {i + 1} : [{xa}] vs [{xb}]"
    return "contenu identique mais bytes distincts"


def main():
    ok, ko = 0, []
    # troisième voie : l'analyseur COMPILÉ (AOT) doit rendre la même sortie
    # que l'oracle et que sa propre interprétation.
    analyseur_bin = os.path.join("/tmp/opencode", "analyseur_m4.bin")
    p = subprocess.run([MOTEUR, "build", ANALYSEUR, "-o", analyseur_bin],
                       capture_output=True, text=True, env=ENV)
    aot_ok = p.returncode == 0
    if not aot_ok:
        ko.append((ANALYSEUR, f"analyseur non compilable en AOT : {p.stderr.strip()[:120]}"))
    for f in corpus():
        so, se, rc1 = lancer([MOTEUR, "ast", f])
        ao, ae, rc2 = lancer([MOTEUR, ANALYSEUR, f])
        if rc1 != 0:
            ko.append((f, f"oracle lui-même échoue rc={rc1} : {se.strip()[:90]}"))
            continue
        if rc2 != 0:
            ko.append((f, f"analyseur rc={rc2} : {ae.strip()[:90]}"))
            continue
        if so != ao:
            ko.append((f, "interprété — " + premiere_ligne_divergente(so, ao)))
            continue
        if aot_ok:
            bo, be, rc3 = lancer([analyseur_bin, f])
            if rc3 != 0:
                ko.append((f, f"analyseur AOT rc={rc3} : {be.strip()[:90]}"))
                continue
            if bo != so:
                ko.append((f, "AOT — " + premiere_ligne_divergente(so, bo)))
                continue
        ok += 1
    total = ok + len(ko)
    for f, msg in ko:
        print(f"ÉCHEC {os.path.relpath(f, RACINE)} : {msg}")
    print(f"Arbre : {ok}/{total} fichiers identiques à l'oracle, {len(ko)} divergences")
    sys.exit(1 if ko else 0)


if __name__ == "__main__":
    main()
