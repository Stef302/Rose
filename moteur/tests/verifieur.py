#!/usr/bin/env python3
"""verifieur.py — jalon M4.2 : l'oracle de vérification à l'oeuvre.

Compare la réponse de `rose verif <f>` (oracle C++, repli de constantes +
Vérificateur) à celle de `rose rose/verifieur.rose <f>` (vérificateur
auto-hébergé en Rose), puis au binaire AOT de ce même vérificateur, sur
tout le corpus : les 53 fixtures de moteur/tests/verifs/ (rejets et
acceptations), les dorés, les roses, les bancs, le lexeur, l'analyseur —
et le vérificateur se vérifie lui-même : exigence du bootstrap M4.

Réponse de référence : « vérifié » quand l'oracle accepte (rc=0), sinon la
ligne exacte « Rose : erreur — … » que l'oracle écrit sur stderr (rc=1).
Le vérificateur Rose écrit la même ligne sur stdout.

Porte triple, même discipline qu'aux paliers M4.0/M4.1 : texte,
interprétation, binaire natif. Une seule divergence = échec.

Usage : python3 moteur/tests/verifieur.py [moteur]
"""
import glob
import os
import subprocess
import sys

RACINE = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
MOTEUR = sys.argv[1] if len(sys.argv) > 1 else os.path.join(RACINE, "moteur", "cpp", "rose")
VERIFIEUR = os.path.join(RACINE, "rose", "verifieur.rose")
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
    fs.append(VERIFIEUR)  # le vérificateur se vérifie lui-même : point fixe du jalon
    return fs


def lancer_verif(f):
    so, se, rc = lancer([MOTEUR, "verif", f])
    if rc == 0:
        return so.strip(), rc
    return se.strip(), rc


def main():
    ok, ko = 0, []
    # troisième voie : le vérificateur COMPILÉ (AOT) doit rendre la même
    # réponse que l'oracle et que sa propre interprétation.
    verifieur_bin = os.path.join("/tmp/opencode", "verifieur_m4.bin")
    p = subprocess.run([MOTEUR, "build", VERIFIEUR, "-o", verifieur_bin],
                       capture_output=True, text=True, env=ENV)
    aot_ok = p.returncode == 0
    if not aot_ok:
        ko.append((VERIFIEUR, f"vérificateur non compilable en AOT : {p.stderr.strip()[:120]}"))
    for f in corpus():
        ref, rc1 = lancer_verif(f)
        if rc1 not in (0, 1):
            ko.append((f, f"oracle lui-même échoue rc={rc1}"))
            continue
        ao, ae, rc2 = lancer([MOTEUR, VERIFIEUR, f])
        if rc2 != 0:
            ko.append((f, f"vérificateur rc={rc2} : {ae.strip()[:90]}"))
            continue
        if ao.strip() != ref:
            ko.append((f, f"interprété — attendu [{ref}] rendu [{ao.strip()}]"))
            continue
        if aot_ok:
            bo, be, rc3 = lancer([verifieur_bin, f])
            if rc3 != 0:
                ko.append((f, f"vérificateur AOT rc={rc3} : {be.strip()[:90]}"))
                continue
            if bo.strip() != ref:
                ko.append((f, f"AOT — attendu [{ref}] rendu [{bo.strip()}]"))
                continue
        ok += 1
    total = ok + len(ko)
    for f, msg in ko:
        print(f"ÉCHEC {os.path.relpath(f, RACINE)} : {msg}")
    print(f"Vérification : {ok}/{total} fichiers identiques à l'oracle, {len(ko)} divergences")
    sys.exit(1 if ko else 0)


if __name__ == "__main__":
    main()
