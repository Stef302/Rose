#!/usr/bin/env python3
"""Corpus d'exactitude absolue pour les racines (RA 19/11).

Compare `racine_decimales(n,d)` de Rose à `math.isqrt(n*10^(2d))` Python
(référence indépendante), pour n=2..20 et plusieurs profondeurs dont le
seuil de bascule 2048 bits. Zéro approximation, zéro GMP.
"""
import math
import subprocess
import sys

ROSE = sys.argv[1] if len(sys.argv) > 1 else "/home/clausake/Stefan/ROSE/rose/target/release/rose"
NS = list(range(2, 21))
DS = [0, 1, 10, 100, 154, 155, 200, 500, 1000]

prog = "\n".join(f">> racine_decimales({n}, {d})" for n in NS for d in DS) + "\n"
path = "/tmp/opencode/ra_agents/corpus_racines.rose"
with open(path, "w") as f:
    f.write(prog)

p = subprocess.run(["taskset", "-c", "6", ROSE, path], capture_output=True, text=True)
if p.returncode != 0:
    print("ÉCHEC lancement Rose:", p.stderr[-500:])
    sys.exit(2)
lignes = [l for l in p.stdout.splitlines() if l.strip()]
attendu = [(n, d, str(math.isqrt(n * 10 ** (2 * d)))) for n in NS for d in DS]
if len(lignes) != len(attendu):
    print(f"ÉCHEC nombre de lignes : {len(lignes)} au lieu de {len(attendu)}")
    sys.exit(2)

bad = 0
for (n, d, ref), got in zip(attendu, lignes):
    if got != ref:
        bad += 1
        if bad <= 5:
            print(f"MISMATCH n={n} d={d}\n  attendu {ref[:80]}… ({len(ref)} chiffres)\n  obtenu  {got[:80]}… ({len(got)} chiffres)")
print(f"corpus : {len(attendu)-bad}/{len(attendu)} exacts  (n=2..20, d={DS})")
sys.exit(1 if bad else 0)
