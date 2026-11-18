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

---

## Addendum — après fermeture du goulot chaîne (18/11, 15:40Z)

Binaire `c05d72e5574a8279326aa2f3b8bc24f88b8b68df1abc2e1219d2aaca6725a4fb`,
2 salves A/A de 9 échantillons rejouées indépendamment (c19), mêmes harnais.

| # | Domaine | Avant (ce2188) | Après rope (c05d72) | Ratio Rust |
|---|---|---:|---:|---:|
| 1 | `fib(35)` | 11,600 | 11,553 – 11,595 | 0,91 × ✅ |
| 2 | Boucle 1..10⁶ | 8,743 | 9,045 – 9,054 | 34,9 × (**+3 % régression**) |
| 3 | Premiers < 50 000 | 9,453 | 9,323 – 9,339 | 9,4 × |
| 4 | Factorielle 5 000 | 1,484 | 1,480 – 1,482 | 0,23 × ✅ |
| 4b | PGCD 1 000 chiffres | 6,735 | 6,708 – 6,721 | 45,5 × |
| 5 | Chaîne 20 000 car. | 6,160 | **3,294 – 3,321** | 233 × (**×1,87**) |
| 6 | Liste 100 000 entiers | 38,591 | 38,124 – 38,126 | 203 × |
| 7 | Table 3 000 clés | 6,332 | 6,283 – 6,286 | 66,6 × |
| 8 | E/S 10 Mo | 13,490 | 14,564 – 14,822 (bruité) | 2,85 × |

Toujours **2 domaines / 9** battant Rust. Chaîne : O(n²) → O(n) **prouvé**
(N = 5k/10k/20k/40k → 0,82/1,65/3,30/6,57 ms, ×2,01/×2,00/×1,99).
Détails, variantes rejetées et exactitude : `CHAINE_O1.md`.

**Porte chaîne < 1 ms NON atteinte** : plancher ~1,7–2,2 ms de synchronisation
slots↔globaux dans `rir.rs` (hors périmètre de l'agent). Régression D2 de 3 %
reproductible, non isolée (apparaît dès que le code d'internage est présent).
Constat daté, pas consigne.

---

## Addendum — après E/S zéro recopie (18/11, 15:55Z)

Binaire `aa0f8c7ed5e6ab98cd07bca69337571b45f7ba2438ad4513f41a992becce6ee0`.
2 salves A/A indépendantes (9 échantillons, c15 **et** c19).

| # | Domaine | Avant | Après | Ratio Rust (c15 / c19) |
|---|---|---:|---:|---:|
| 1 | `fib(35)` | 11,55 | 11,64 / 11,56 | 0,918 / 0,911 ✅ |
| 2 | Boucle 1..10⁶ | 9,05 | 9,03 / 9,05 | 34,4 / 34,9 |
| 3 | Premiers < 50 000 | 9,34 | 9,35 (stable) | 9,4 |
| 4 | Factorielle 5 000 | 1,48 | 1,56 (bruit) | 0,23 ✅ |
| 4b | PGCD 1 000 chiffres | 6,72 | 6,72 (stable) | 45,5 |
| 5 | Chaîne 20 000 car. | 3,29 | 3,29 / 3,28 | 235 / 234 |
| 6 | Liste 100 000 entiers | 38,12 | 38,06 / 38,05 | 203 / 209 |
| 7 | Table 3 000 clés | 6,29 | 6,34 / 6,33 | 66,6 / 67,2 |
| 8 | E/S 10 Mo | 14,76 | **6,05 / 5,95** | **0,998 / 0,992** ≈ |

**Bilan : 2 victoires nettes** (fib35, factorielle) **+ 1 domaine au niveau de
Rust** (E/S, écarts < 2 % = bruit, pas une victoire revendiquée). Les 6 autres
domaines restent en retard.

Décomposition mesurée du coût E/S avant correction : `fs::read` 2 143 µs,
`from_utf8_lossy().into_owned()` **6 043 µs**, `aplatir()` **1 869 µs**,
`fs::write` 4 083 µs — les deux recopies représentaient 54,8 % du total.
Détails : `ES_ZERO_COPY.md`. Constat daté, pas consigne.

---

## Addendum — après table native (18/11, 17:50Z)

Binaire `6e3ff795456d3f9bcf01c74dcb5a443c65e969c29ee8515b18b6497a5dacf801`.
2 salves A/A indépendantes (9 échantillons, c19 et c15).

| # | Domaine | Avant | Après (c19 / c15) | Ratio Rust |
|---|---|---:|---:|---:|
| 1 | `fib(35)` | 11,56 | 11,65 / 11,56 | 0,92 / 0,91 ✅ |
| 2 | Boucle 1..10⁶ | 9,05 | 9,30 / 9,25 | 35,8 / 35,9 (**+2,8 %**) |
| 3 | Premiers < 50 000 | 9,34 | 9,42 / 9,46 | 9,4 (**+1 %**) |
| 4 | Factorielle 5 000 | 1,56 | 1,47 / — | 0,23 ✅ |
| 4b | PGCD 1 000 chiffres | 6,72 | 6,71 / — | 45,4 |
| 5 | Chaîne 20 000 car. | 3,29 | 3,34 / 3,33 | 232 / 226 (**+1,4 %**) |
| 6 | Liste 100 000 entiers | 38,12 | 38,66 / 38,73 | 214 / 206 (**+1,7 %**) |
| 7 | Table 3 000 clés | 6,33 | **1,386 / 1,385** | **14,8 / 14,7** (−78 %) |
| 8 | E/S 10 Mo | 5,95 | 6,17 / 4,99 (bruit) | 1,01 / 0,98 ≈ |

**Toujours 2 victoires nettes + 1 domaine au niveau de Rust.** Table :
9 000 000 appels à `valeur_egale` (recherche linéaire) remplacés par un index
de hachage O(1) → ×4,59, ratio Rust 67 → 14,7. Porte < 1 ms non atteinte
(plancher 1,1 ms de dispatch interprète). Régressions faibles mais
reproductibles (1–2,8 %) sur D2/D3/D5/D6, documentées. Exactitude : diff vide
sur 43 + 8 sorties vs binaire de départ. Détails : `TABLE_NATIVE.md`.

---

## Addendum — après JIT boucle i64 (18/11, 18:45Z)

Binaire `2c46dd2deca2db92349cfb47b9aa50cc950c0785f8e14ba75ae79483f8e5493c`.
2 salves A/A indépendantes (9 échantillons, c19 et c15).

| # | Domaine | Avant | Après (c19 / c15) | Ratio Rust |
|---|---|---:|---:|---:|
| 1 | `fib(35)` | 11,62 | 11,62 / 11,79 | 0,91 / 0,93 ✅ |
| 2 | Boucle 1..10⁶ | 9,30 | **1,294 / 1,297** | **5,00 / 4,93** (−86 %) |
| 3 | Premiers < 50 000 | 9,42 | 9,51 / 9,46 | 9,49 / 9,44 |
| 4 | Factorielle 5 000 | 1,47 | 1,43 / 1,43 | 0,22 ✅ |
| 4b | PGCD 1 000 chiffres | 6,71 | 6,72 / — | 45,4 |
| 5 | Chaîne 20 000 car. | 3,33 | 3,33 / 3,32 | 238 / 236 |
| 6 | Liste 100 000 entiers | 38,66 | 38,52 / 38,54 | 210 / 206 |
| 7 | Table 3 000 clés | 1,386 | 1,383 / 1,378 | 14,2 / 14,5 |
| 8 | E/S 10 Mo | 6,07 | 6,07 / 6,00 (bruit) | 0,99 / 0,98 ≈ |

**Toujours 2 victoires nettes + 1 au niveau de Rust.** La boucle passe de
×35,8 à ×5,0 sans régression systématique ailleurs. La prémisse initiale
(« checks tag/overflow de `jit.rs:2966` ») était **fausse** : le dump code a
montré 2 appels `rose_add_ent` par itération (~8 ns des 9,3 ns) via
`expr_eabstrait_rir`. Détails et deux bugs d'exactitude corrigés :
`JIT_BOUCLE_I64.md`. Constat daté, pas consigne.
