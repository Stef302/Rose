#!/usr/bin/env python3
"""Rapport statistique des bancs (Phase « industrialiser les mesures »).

Pour chaque programme de bench/ : N exécutions mesurées après exécutions de
chauffe, et rapporte min, médiane, p10/p90 et l'écart-type relatif — le min
seul masque la dispersion, la médiane seule masque les meilleurs cas. Le
résultat attendu (empreinte) est vérifié sur la première exécution mesurée :
un chrono sans correction ne vaut rien.

Usage : bench_stat.py [nom ...]   (défaut : tous les bancs)
       ROSE_MOTEUR=... pour un moteur donné ; ROSE_REP=5 pour les répétitions.
"""
import math
import os
import subprocess
import sys
import time

ICI = os.path.dirname(os.path.abspath(__file__))
MOTEUR = os.environ.get("ROSE_MOTEUR") or os.path.join(
    os.path.dirname(os.path.abspath(__file__)), "..", "..", "cpp", "rose")
REP = int(os.environ.get("ROSE_REP", "11"))
CHAUFFE = int(os.environ.get("ROSE_CHAUFFE", "2"))

# Empreintes de référence (identiques à l'historique PERF — un résultat
# différent invalide la mesure).
EMPREINTES = {
    "appels": "2000013000000", "boucle_entier": "9999999",
    "boucle_flottante": "1499999.5", "chaines": "1000000",
    "factorielle": "5565709", "fibonacci": "1044939",
    "mot_de_passe": "1000000000", "pgcd": "16106831",
    "premiers": "1270607", "puissance": "9030901",
}


def un_exec(nom):
    debut = time.perf_counter()
    p = subprocess.run([MOTEUR, os.path.join(ICI, nom + ".rose")],
                       capture_output=True, text=True)
    return time.perf_counter() - debut, p.stdout.strip().splitlines()[:1]


def pct(trie, q):
    i = q * (len(trie) - 1)
    lo, hi = int(math.floor(i)), int(math.ceil(i))
    return trie[lo] + (trie[hi] - trie[lo]) * (i - lo)


noms = sys.argv[1:] or sorted(EMPREINTES)
print(f"moteur : {MOTEUR}")
print(f"répétitions : {CHAUFFE} chauffe + {REP} mesurées\n")
print(f"{'banc':<18}{'min':>9}{'médiane':>9}{'p10':>9}{'p90':>9}{'étal.':>8}  empreinte")
mauvais = 0
for nom in noms:
    for _ in range(CHAUFFE):
        un_exec(nom)
    temps, empreinte = [], []
    for _ in range(REP):
        t, e = un_exec(nom)
        temps.append(t)
        empreinte = e
    trie = sorted(temps)
    med = pct(trie, 0.5)
    moyenne = sum(trie) / len(trie)
    ecart = math.sqrt(sum((t - moyenne) ** 2 for t in trie) / len(trie))
    ok = empreinte and empreinte[0] == EMPREINTES[nom]
    if not ok:
        mauvais += 1
    print(f"{nom:<18}{trie[0]:>9.3f}{med:>9.3f}{pct(trie, 0.1):>9.3f}"
          f"{pct(trie, 0.9):>9.3f}{(ecart / moyenne * 100):>7.1f}%"
          f"  {'ok' if ok else 'DIVERGENTE ' + str(empreinte)}")

sys.exit(1 if mauvais else 0)
