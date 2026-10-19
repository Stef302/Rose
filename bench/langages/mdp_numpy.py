#!/usr/bin/env python3
# Le programme originel de la comparaison : le script numpy d'un
# utilisateur spontanés — np.random.choice sur la liste des caractères.
# Affiche seulement les millisecondes sur stdout (pour bench.sh).
import sys, time
import numpy as np

n = int(sys.argv[1]) if len(sys.argv) > 1 else 1_000_000_000
alpha = np.frombuffer(b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_",
                      dtype=np.uint8)
t0 = time.time()
buf = np.random.default_rng(2026).choice(alpha, size=n)
ms = int((time.time() - t0) * 1000)
print(ms)
