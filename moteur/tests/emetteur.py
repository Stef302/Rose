#!/usr/bin/env python3
"""emetteur.py — jalon M4.3 : l'oracle RIR à l'oeuvre.

Compare la réponse de `rose rir <f>` (oracle C++, repli de constantes +
Vérificateur + EmetteurRir) à celle de `rose rose/emetteur.rose <f>`
(émetteur auto-hébergé en Rose), puis au binaire AOT de ce même émetteur,
sur tout le corpus : les 53 fixtures de moteur/tests/verifs/, les dorés,
les roses, les bancs, le lexeur, l'analyseur, le vérificateur — et
l'émetteur se dump lui-même : le compilateur se compile-t-il lui-même ?

Réponse de référence : le texte RIR complet quand l'oracle accepte (rc=0),
sinon la ligne exacte « Rose : erreur — … » qu'il écrit sur stderr (rc=1) —
les rejets partagent donc la réponse de verifieur.py, l'émetteur rejoue le
vérificateur avant d'émettre. L'émetteur Rose écrit la même sortie sur
stdout dans les deux cas.

Porte triple, même discipline qu'aux paliers M4.0/M4.1/M4.2 : texte,
interprétation, binaire natif. Une seule divergence = échec.

Usage : python3 moteur/tests/emetteur.py [moteur]
"""
import glob
import os
import subprocess
import sys

RACINE = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
MOTEUR = sys.argv[1] if len(sys.argv) > 1 else os.path.join(RACINE, "moteur", "cpp", "rose")
EMETTEUR = os.path.join(RACINE, "rose", "emetteur.rose")
ENV = dict(os.environ, ROSE_GMP="/tmp/opencode/gmp-linux")


def lancer(cmd):
    p = subprocess.run(cmd, capture_output=True, text=True, env=ENV)
    return p.stdout, p.stderr, p.returncode


def corpus():
    fs = []
    fs += sorted(glob.glob(os.path.join(RACINE, "moteur", "tests", "verifs", "*.rose")))
    fs += sorted(glob.glob(os.path.join(RACINE, "moteur", "tests", "dorés", "*.rose")))
    fs += sorted(glob.glob(os.path.join(RACINE, "moteur", "tests", "roses", "*.rose")))
    fs += sorted(glob.glob(os.path.join(RACINE, "moteur", "tests", "bench", "*.rose")))
    fs.append(os.path.join(RACINE, "rose", "lexeur.rose"))
    fs.append(os.path.join(RACINE, "rose", "analyseur.rose"))
    fs.append(os.path.join(RACINE, "rose", "verifieur.rose"))
    fs.append(EMETTEUR)  # l'émetteur se dump lui-même : point fixe du jalon
    return fs


def lancer_rir(f):
    so, se, rc = lancer([MOTEUR, "rir", f])
    if rc == 0:
        return so.strip(), rc
    return se.strip(), rc


def main():
    ok, ko = 0, []
    # troisième voie : l'émetteur COMPILÉ (AOT) doit rendre la même réponse
    # que l'oracle et que sa propre interprétation.
    emetteur_bin = os.path.join("/tmp/opencode", "emetteur_m43.bin")
    p = subprocess.run([MOTEUR, "build", EMETTEUR, "-o", emetteur_bin],
                       capture_output=True, text=True, env=ENV)
    aot_ok = p.returncode == 0
    if not aot_ok:
        ko.append((EMETTEUR, f"émetteur non compilable en AOT : {p.stderr.strip()[:120]}"))
    for f in corpus():
        ref, rc1 = lancer_rir(f)
        if rc1 not in (0, 1):
            ko.append((f, f"oracle lui-même échoue rc={rc1}"))
            continue
        ao, ae, rc2 = lancer([MOTEUR, EMETTEUR, f])
        if rc2 != 0:
            ko.append((f, f"émetteur rc={rc2} : {ae.strip()[:90]}"))
            continue
        if ao.strip() != ref:
            ko.append((f, f"interprété — attendu [{ref[:200]}] rendu [{ao.strip()[:200]}]"))
            continue
        if aot_ok:
            bo, be, rc3 = lancer([emetteur_bin, f])
            if rc3 != 0:
                ko.append((f, f"émetteur AOT rc={rc3} : {be.strip()[:90]}"))
                continue
            if bo.strip() != ref:
                ko.append((f, f"AOT — attendu [{ref[:200]}] rendu [{bo.strip()[:200]}]"))
                continue
        ok += 1
    total = ok + len(ko)
    for f, msg in ko:
        print(f"ÉCHEC {os.path.relpath(f, RACINE)} : {msg}")
    print(f"Émission : {ok}/{total} fichiers identiques à l'oracle, {len(ko)} divergences")
    sys.exit(1 if ko else 0)


if __name__ == "__main__":
    main()
