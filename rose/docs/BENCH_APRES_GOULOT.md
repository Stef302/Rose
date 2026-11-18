# Constatt daté — bench multi après fermeture du goulot #1

**Horodatage** : 2026-11-18T15:29:52Z → 15:30:01Z (salve 1) et
15:30:28Z → 15:30:37Z (salve 2 de contrôle). Deux salves A/A de 9 échantillons
entrelacés, pinées sur le cœur X925 19, même harnais que `BENCH_MULTI.md`.

**Binaire** : `rose/rose`, SHA256 `ce2188308eab1f43846f2d95aa7e42135ced6661f62992189557bf7620fb6c31`,
vérifié identique avant et après les deux salves. Ce binaire contient : protocole
de trame unique (N1), RIR slots + `appel_rir_slots` (A2), inline de l'API valeurs
(L1), décodage wasm unique (L3). Rose 0.1.5, `rustc 1.99.1 -O`, C `gcc -O2`.

**Protocole** : identique à `BENCH_MULTI.md` — médianes de 9 échantillons,
deux horloges distinctes (interne = calcul seul), valeur de sortie vérifiée à
chaque échantillon, anti-optimisation Rust (`black_box`) et C (`volatile`).
Les deux salves sont convergées à 1–5 % près sur tous les domaines.

---

## Résultat (calcul seul, médianes — salve 2)

| # | Domaine | Rose (ms) | Rust -O (ms) | Rose/Rust | Ancien (12:48Z) | Gain |
|---|---|---:|---:|---:|---:|---:|
| 1 | `fib(35)` récursif | 11,600 | 12,736 | **0,91 ×** ✅ | 51,329 | 4,43 × |
| 2 | Boucle 1..10⁶ | 8,743 | 0,261 | 33,56 × | 13,850 | 1,58 × |
| 3 | Premiers < 50 000 | 9,453 | 1,002 | 9,44 × | 26,988 | 2,86 × |
| 4 | Factorielle 5 000 | 1,484 | 6,538 | **0,23 ×** ✅ | 1,471 | ~1 (stable) |
| 4b | PGCD 1 000 chiffres | 6,735 | 0,148 | 45,55 × | 7,511 | 1,12 × |
| 5 | Chaîne 20 000 car. | 6,160 | 0,014 | 439,02 × | 14,326 | 2,33 × |
| 6 | Liste 100 000 entiers | 38,591 | 0,186 | 207,52 × | 238,335 | 6,17 × |
| 7 | Table 3 000 clés | 6,332 | 0,094 | 67,22 × | 188,139 | **29,72 ×** |
| 8 | E/S 10 Mo | 13,490 | 4,733 | 2,85 × | 14,150 | 1,05 × |

Ratio < 1 = Rose gagne. **Rose bat maintenant Rust sur 2 domaines / 9**
(contre 1/8 au constat de 12:48Z) : `fib(35)` et factorielle. Aucun de ces deux
résultats n'est généralisé aux autres domaines.

---

## Ce qui a bougé, et pourquoi (architecture, pas micro-opt)

- **Goulot #1 fermé** (N1, `TRAME_UNIQUE.md`) : le ratio V30/V0 est passé de
  8,337 → 0,993 sur `joindre`. Conséquence directe : domaine 6 (liste) ×6,17,
  domaine 7 (table) ×29,72 — ce sont les programmes qui matérialisaient le plus
  de trames `HashMap` par opération.
- **RIR slots** (A2, `ARCHI_INTERP.md`) : `fib(35)` ×4,43 ; la récursion ne
  reconstruit plus une trame complète à chaque appel.
- **API valeurs en ligne** (L1, `ARCHI_VALEURS.md`) : domaine 7 (table).
- **Décodage wasm unique** (L3, `ARCHI_WASM_PONT.md`) : ×2,375 sur le pont,
  mais aucun domaine du bench ne passe par le pont wasm — gain réel mais hors
  périmètre de ce tableau.

## Ce qui reste (constats, pas des promesses)

- Le domaine 5 (chaîne, ×439) reste le plus éloigné : concaténation O(n²) dans
  le chemin interprété.
- Le domaine 6 (liste, ×207) : `joindre` coûte encore ~138 ns/op (objectif
  secondaire < 100 ns non atteint).
- Le domaine 8 (E/S, ×2,85) : recopie de tampon.
- Les domaines 2, 3, 4b restent à 9–46 × de Rust avec la même source.

**Règle** : ce document est un constat daté. Re-mesurer après tout changement
de moteur, de quantification ou de topologie. Ne pas citer ces chiffres comme
une consigne.

---

## Preuves rejouables

```
cd /tmp/opencode/benchmulti
PIN_CPU=19 NSAMP=9 python3 bench.py
# multi_summary.json → summary.<domaine>.<impl>.int_med (ns)
sha256sum /home/clausake/Stefan/ROSE/rose/rose
# ce2188308eab1f43846f2d95aa7e42135ced6661f62992189557bf7620fb6c31
```

Données brutes : `/tmp/opencode/benchmulti/bench_after_goulot.log`,
`bench_after_goulot_ctrl.log`, `multi_results.csv`, `multi_summary.json`.
