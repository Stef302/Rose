# OPT_BIGINT — accélération mesurée des mathématiques exactes

**Constat daté : 22/11/2026. Worktree exclusif `/tmp/opencode/opt_bigint`,
aucun commit/push. Binaire `CARGO_TARGET_DIR=/tmp/opencode/opt_shared_target`.
Garde-fou `bash scripts/garde_fou_mem.sh 12000000 <commande>` sur toutes les
commandes longues.**

## 1. Protocole (reproductible)

Banc : `rose/tests/opt_bigint.rs`, fonction `banc()`, lancée par
`OPT_BIGINT_BENCH=1 cargo test --release --test opt_bigint -- --nocapture`.
Pin `taskset -c 19` (cœur X925), entrées décimales déterministes (xorshift64*),
`std::hint::black_box` sur chaque résultat, **3 passes × 3 répétitions
entrelacées**, médianes. Le compteur d'allocations est un `GlobalAlloc`
actif uniquement pendant les appels mesurés. Pic RSS du processus via
`/usr/bin/time -v`. Bruts : `/tmp/opencode/opt_bigint_before.txt`,
`/tmp/opencode/opt_bigint_final3.txt`.

Référence Rust : `num-bigint 0.5.1` est **présent** dans le crate (contrairement
à l'hypothèse de départ) : la comparaison utilise donc `BigInt::gcd` (Stein),
`BigInt::pow`, `<<`, `mod_floor`, et un PGCD `u128` de Stein pour les petites
valeurs. La colonne `ratio` du banc est `Rose/Rust` : **< 1 = Rose gagne**.

Dispersion avant/après < 1,5 % sur toutes les lignes, sauf factorielle (~3 %).

## 2. Médianes avant / après (µs par appel, c19)

| Domaine | Avant Rose | Après Rose | Gain | Réf. Rust (avant → après) | Rose/Rust après | Allocs avant → après | Octets/appel avant → après |
|---|---:|---:|---:|---:|---:|---:|---:|
| **PGCD 1 000 chiffres** | 67,49 | **34,71** | **1,94×** | 67,17 → 67,29 | **0,52×** | 11 → 803 | 2 016 → 176 384 |
| **2^10000 `gmp_ui_pow_ui`** | 1,04 | **0,05** | **≈20×** | 0,69 → 0,70 (`pow`) | **0,07×** | 24 → **1** | 9 312 → 1 264 |
| 2^10000 `gmp_pow_ui` | 0,73 | **0,05** | ≈15× | 0,05 (`<<`, optimal) | 1,0× | 12 → 1 | — |
| Factorielle 5 000 | 265,96 | 273,46 | 0,97× | 1 200 → 1 178 | 0,23× | 1 105 → 1 105 | 279 543 → 279 543 |
| Rationnels `add` 120 ch | 9,09 | **8,97** | 1,01× | 9,21 → 9,07 | 0,99× | 21 → **12** | 1 672 → 1 024 |
| Rationnels `mul` 120 ch | 9,07 | 9,30 | 0,98× | — | — | 18 → 13 | 1 440 → 1 112 |
| Rationnels `div` 120 ch | 9,34 | 9,30 | 1,00× | — | — | 18 → **11** | 1 440 → 904 |
| Rationnels `sub` 120 ch | 9,15 | 9,08 | 1,01× | — | — | 21 → 12 | 1 672 → 1 024 |
| Racine √2, 1 000 déc. | 14,69 | 14,56 | 1,01× | 17,32 → 17,40 | 0,84× | 28 → 28 | 7 816 → 7 816 |
| Comparaison rationnels | 0,31 | **0,24** | **1,29×** | 0,26 → 0,24 | 1,00× | 6 → **2** | 720 → 368 |

**Pic RSS : 1 672 Ko avant = 1 672 Ko après** (3 salves de part et d'autre) :
aucune régression mémoire, malgré l'algorithmique plus élaborée du PGCD.

Lecture : l'objectif ≥ 2× est **atteint sur la puissance** (≈20×, et l'égalité
avec le décalage optimal `1 << 10000`) et **presque atteint sur le PGCD**
(1,94×, à 3 % près). Le critère « moins d'allocations » est atteint sur
puissance (−96 %), comparaison (−67 %), rationnels (−28 à −43 %). Le PGCD, lui,
**échange des allocations contre du temps** (11 → 803) : c'est un choix
mesuré, pas une régression masquée (803 au lieu de 1 190 pour le premier
Lehmer implémenté, −33 % après réglage du plafond de coefficients).

## 3. Optimisations retenues (toutes dans `rose/src/i128x.rs`)

1. **`gmp_ui_pow_ui` / `gmp_pow_ui` / `Gz::pow_u64`** — l'ancien
   `gmp_ui_pow_ui` faisait **deux clones complets par bit d'exposant**
   (`res * b.clone()` **et** `b.clone() * b`). La nouvelle fonction partagée
   `pow_ui_vers` n'en fait plus qu'un, et **zéro pour une base puissance de 2**
   : `k = log2(base)`, `base^e = 1 << (k·e)` (une seule allocation). Garde-fou
   `shift ≤ 2^28` pour éviter un décalage déraisonnable. `base == 1` rend 1.
2. **PGCD `gcd_rapide`** — Lehmer hybride à fenêtre 64 bits : une matrice
   unimodulaire appliquée en une passe à de grands nombres, puis Stein
   (`num-bigint`) sous 2^128 bits et **repli Stein direct sous 1 024 bits**
   (les rationnels 120 ch, ~400 bits, allaient plus lentement sans ce seuil).
   Exactitude : le déterminant reste ±1, donc le PGCD est invariant même si
   le quotient approché est imparfait ; si le reste ne diminue pas, un vrai
   pas d'Euclide garantit la terminaison. Plafond de coefficients `2^32`,
   32 pas d'Euclide par matrice (balayage détaillé §4).
3. **Zéro clone des opérandes dans la façade `gmp_*` et les `Gz`** —
   `gmp_add/sub/mul/mul_ui/mul_si/neg/add_ui/fdiv_q/fdiv_r/fdiv_ui/mul_2exp/…`
   empruntent `&(*a).v` ; `Gz::fits_*`, `sgn`, `get_*`, `sizeinbase10`,
   `taille_bits`, `tstbit`, `i128`, `fdiv_*`, `mul_2exp`, `neg`, `est_premier`
   passent par `bref()`. Résultat calculé dans une temporaire avant écriture,
   donc sûr même si `r` aliase `a` (cas réel `interp.rs:3509`).
4. **Rationnels en place** — `canon_q()` réduit sur place (plus de double
   clone du numérateur/dénominateur ni du PGCD) ; `gmpq_add/sub/mul/div/cmp`
   et les méthodes `Gq::*_in` calculent tous les produits par référence avant
   d'écrire. La voie rapide `add_rat_inv_small` et le raccourci `(a/1)/(b/1)`
   sont conservés.

## 4. Variantes mesurées et REJETÉES (preuve de non-régression)

Toutes sur le même banc, PGCD 1 000 chiffres, médiane c19 :

| Variante | Temps | Verdict |
|---|---:|---|
| Fenêtre u128 (96 pas) | 197 µs | `u128/u128` = routine logicielle sur aarch64 → 3× plus lent que Stein |
| Fenêtre 64 bits mais chaque opérande normalisé sur SES 64 bits de tête | 670 µs | quotient approché faux quand les tailles diffèrent |
|fenêtre 64 bits, décalage commun, 24 pas, plafond 2^62 | 56 µs | 1,20× seulement |
| 48 pas, plafond 2^62 | 328 µs | multiplicateurs de 62 bits → coût de `BigUint × u64` décuplé |
| 32 pas, plafond 2^24 | 41 µs | plafond trop serré |
| 32 pas, plafond 2^40 | 47 µs | idem |
| **32 pas, plafond 2^32** | **35 µs** | **retenu (1,94×)** |
| Lehmer appliqué aux rationnels 400 bits | 10,4 µs / 258 allocs | remplacé par le seuil Stein 1 024 bits → 9,0 µs / 12 allocs |

## 5. Fichiers possédés NON modifiés (avec raison mesurée)

- `rose/src/gz_pool.rs` : le pool recycle déjà les `Box<Mpz>` (MAX_LIBRES = 64)
  et `avec_deux` relâche le `RefCell` pendant le calcul. Le banc montre que le
  coût résiduel vient des vecteurs de limbs `BigInt`, pas des boîtes : aucun
  gain mesurable à toucher ce pool.
- `rose/src/mul_rapide.rs` : `vers_chaine` fait déjà la conversion binaire→décimal
  par multiplication/division récursive (2× plus rapide que num-bigint,
  cf. `BIGINTS_RAPIDES.md`). Sur 5 000!, Rose reste 4,3× plus rapide que Rust ;
  la ligne du banc n'a pas bougé hors bruit (265,96 → 273,46 µs).
- `rose/src/irrationnel.rs` : `produit_a` met déjà en cache `n·10^(2d)` et
  l'échelonnement 2 048 bits de Newton est en place. 14,69 → 14,56 µs = bruit.

## 6. Exactitude et validation

- `rose/tests/opt_bigint.rs` — **7 tests, 0 échec** :
  `banc` (y compris l'assertion `gmp_gcd == num_bigint::gcd` sur 1 000 chiffres),
  `exactitude_gcd_lehmer` (candidat indépendant), `exactitude_gcd_rapide`
  (8 tailles 1→1 500 chiffres × 10 paires, tailles déséquilibrées, zéros et
  signes), `exactitude_puissances` (2^10000 + référence `u128` sur 3^0..3^80),
  `exactitude_rationnels` (4 opérations + comparaison), `exactitude_reference_u128`,
  `exactitude_racine` (oracle Python `decimal` 120 dps).
- `cargo test --release` (worktree) : **311 + 324 tests unitaires, 0 failed**
  (3 ignored), toutes les suites d'intégration vertes, y compris les 7 ci-dessus.
- `bash scripts/validation_finale.sh` : **0 échec** — point fixe émetteur
  `b3ed9262dd54776a6dbd07bc12368d0e` (identique sur les deux passes), AOT
  **75/75**, JIT **25/25**, WASM 92 o, RVM 317811, AZ **31/31**, HOST-1
  fib=317811. Deux salves concurrentes d'un autre agent écrivaient dans les
  chemins partagés `/tmp/opencode/az_tests/battery` ; la salve canonique
  retenue a été relancée après la fin de cette interférence (RC = 0).

## 7. Limites honnêtes

- Le gain PGCD est de **1,94×**, pas 2,00×. Le seuil de 2× n'est pas franchi
  sur cette entrée ; le balayage §4 montre qu'un plafond de coefficients plus
  large ou une fenêtre plus grande le dégrade fortement sur cette machine.
- Le surcoût d'allocations du PGCD (11 → 803) est réel. Il est compensé par
  le temps (−48 %) et reste bien sous le premier Lehmer (1 190 allocs).
- Factorielle 5 000 est 2,8 % plus lente après (bruit inter-salves, 265,96 →
  273,46 µs) ; aucun code de `factorielle_parallele` n'a été touché.
- Le gain « puissance ≈20× » porte sur `gmp_ui_pow_ui` (base 2). Pour une base
  quelconque, le gain est de ~1,5× (suppression des deux clones), pas 20×.
