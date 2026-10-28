#!/usr/bin/env python3
"""lexeur.py — jalon M4.0 : l'oracle de lexage à l'oeuvre.

Compare la sortie de `rose tokens <f>` (oracle C++, tokeniser()) à celle
de `rose rose/lexeur.rose <f>` (lexeur auto-hébergé en Rose) sur tout le
corpus : 46 dorés, 12 roses, 10 bancs, fixtures génératives gelées.
Une seule divergence = échec. C'est la semelle du bootstrap M4 : le jour
où l'analyseur en Rose produira le RIR de l'oracle, ce test sera le
premier maillon prouvable de la chaîne auto-hébergée.

Usage : python3 moteur/tests/lexeur.py [moteur]
"""
import glob
import os
import subprocess
import sys

RACINE = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
MOTEUR = sys.argv[1] if len(sys.argv) > 1 else os.path.join(RACINE, "moteur", "cpp", "rose")
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
    fs.append(LEXEUR)  # le lexeur se lexelui-même : exigence du bootstrap
    return fs


def main():
    ok, ko = 0, []
    # v3.14.1 — troisième voie : le lexeur COMPILÉ (AOT) doit rendre la même
    # sortie que l'oracle et que sa propre interprétation. C'est la porte
    # triple du bootstrap M4 : texte, exécution, binaire natif.
    lexeur_bin = os.path.join(TMPDIR := "/tmp/opencode", "lexeur_m4.bin")
    p = subprocess.run([MOTEUR, "build", LEXEUR, "-o", lexeur_bin],
                       capture_output=True, text=True, env=ENV)
    aot_ok = p.returncode == 0
    if not aot_ok:
        ko.append((LEXEUR, f"lexeur non compilable en AOT : {p.stderr.strip()[:120]}"))
    for f in corpus():
        so, se, rc1 = lancer([MOTEUR, "tokens", f])
        lo, le, rc2 = lancer([MOTEUR, LEXEUR, f])
        if rc1 != 0:
            ko.append((f, f"oracle lui-même échoue rc={rc1} : {se.strip()[:90]}"))
            continue
        if rc2 != 0:
            ko.append((f, f"lexeur rc={rc2} : {le.strip()[:90]}"))
            continue
        if so != lo:
            # première ligne divergente pour le diagnostic
            a, b = so.splitlines(), lo.splitlines()
            for i in range(max(len(a), len(b))):
                xa = a[i] if i < len(a) else "<fin>"
                xb = b[i] if i < len(b) else "<fin>"
                if xa != xb:
                    ko.append((f, f"ligne {i + 1} : oracle [{xa}] vs lexeur [{xb}]"))
                    break
            continue
        if aot_ok:
            ao, ae, rc3 = lancer([lexeur_bin, f])
            if rc3 != 0:
                ko.append((f, f"lexeur AOT rc={rc3} : {ae.strip()[:90]}"))
                continue
            if ao != so:
                a, b = so.splitlines(), ao.splitlines()
                for i in range(max(len(a), len(b))):
                    xa = a[i] if i < len(a) else "<fin>"
                    xb = b[i] if i < len(b) else "<fin>"
                    if xa != xb:
                        ko.append((f, f"AOT ligne {i + 1} : oracle [{xa}] vs binaire [{xb}]"))
                        break
                continue
        ok += 1
    total = ok + len(ko)
    for f, msg in ko:
        print(f"ÉCHEC {os.path.relpath(f, RACINE)} : {msg}")
    print(f"Lexage : {ok}/{total} fichiers identiques à l'oracle, {len(ko)} divergences")
    sys.exit(1 if ko else 0)


if __name__ == "__main__":
    main()
