# t999_critique_g5 — cause réelle et brique générique anti-O(n²)

## Cause (mesurée, pas seulement documentée)

`rose/docs/BENCH_GLOBAL.md` attribuait les 14,79 ms de `t999_critique_g5`
aux « 20 primitives G5 enchaînées, aucune superinstruction ». Des variations
sur la source (ELF self-hosté de HEAD `faaf18c`, `taskset -c 18`, médiane de
11 tirages) montrent une autre cause dominante :

| variation | médiane |
|---|---:|
| `>> 1` seul | 0,73 ms |
| t999 complet | 13,9 ms |
| t999 sans `fn t36` (36 paramètres) | 3,6 ms |
| déclaration seule de `fn t36` (appel retiré) | 3,0 ms |
| `fn f()` → `fn f(36 paramètres)` | 0,72 → 2,35 ms |
| un seul paramètre long (10 → 800 caractères) | 0,76 → 6,38 ms |
| t999 + 3000 caractères de commentaire (RIR inchangé) | 13,7 → 30,0 ms |

Sous le backend G5, `s[$i]` et `sous_chaine(s, i, j)` **rebalayent la chaîne
depuis son début** (indices en points de code, descripteur en octets UTF-8).
Toute boucle caractère par caractère est donc O(n²). Les passe concernées
à chaque lancement sont `mc_hashes` (source), `mcce_fold_source`,
`g4_pos` (recherche `importer`), `parse_rir` + `tok_line` (dump RIR),
`mc_const_programme` et `mc_fold_rir_powers`. Les primitives G5 ne sont
qu'une part du temps ; la très longue ligne `noms` des 36 paramètres et le
dump RIR de ~3 Ko sont le vrai goulot.

## Optimisation générique (aucun code spécifique à t999)

Trois helpers dans `compilateur/moteur_complet.rose` (couche CLI) :

- `mc_scan_pos(s, mot, dep)` — recherche de sous-chaîne par blocs de 64
  points de code, avec chevauchement de `longueur(mot)-1` ;
- `mc_scan_lignes(s)` — découpe en lignes par `mc_scan_pos`, sans balayage
  octet par octet ;
- `mc_scan_caracteres(s)` — extraction des caractères par blocs de 64.

Ils remplacent les boucles O(n²) de `g4_pos`, `tok_line`, `parse_rir`,
`mc_hashes`, `mcce_src_lines`, `mcce_fold`, `mc_const_programme` et
`mc_fold_rir_powers`. Coût O(n²/64 + 32n) au lieu de O(n²), avec la même
sémantique de points de code (Unicode inclus). Aucune optimisation
existante n'est supprimée ni court-circuitée ; les algorithmes, les clés de
cache et les sorties sont inchangés.

## Résultats

`rose verif compilateur/moteur_complet.rose` : `vérifié` (rc=0).
C0 (`/tmp/opencode/C0_i.elf`) → ELF aarch64 `2726680` octets, magic
`7f454c46`, machine `183`, rc=0, md5 `8e646de11b76ad0f8ea7004700586283`.

`t999`, 9 échantillons entrelacés A/B (`taskset -c 18`, ordre alterné) :

| moteur | médiane |
|---|---:|
| avant (ELF HEAD faaf18c) | 9,583 ms |
| après (même source + brique) | **2,365 ms** |
| Rust JIT | 1,485 ms |

Gain 4,05× ; après/Rust = 1,59× (objectif < 2 ms atteint).
stdout identique : md5 `de94136cd3a3e6a2e8ee2f38be9cf7ce`, rc=0.

Parité corpus 141 : **141/141** sur rc + stdout (139 CONFORMES rc+stdout+
stderr, 2 BLOQUES historiques `t107_hof_var` / `t55_liste_imbriquee`,
0 DIVERGENT) — classification strictement identique à l'ELF « avant ».

Point fixe : `rose rir compilateur/emetteur.rose` et
`rose compilateur/emetteur.rose compilateur/emetteur.rose` rendent tous deux
`b3ed9262dd54776a6dbd07bc12368d0e` (263 520 octets) ; le moteur self-hosté
après correction rend aussi `b3ed9262…` sur le dump de 264 Ko.
