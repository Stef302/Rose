# OPT_ALGOS — dépassement des algorithmes arithmétiques (23/11/2026)

Worktree `opt_bigint`, HEAD de départ `8ef12e66`. Banc : `rose/tests/opt_algo.rs`
(`OPT_ALGO_BENCH=1 RUST_TEST_THREADS=1 cargo test --release --test opt_algo -- --nocapture`),
pin CPU 15, 3 salves entrelacées, médianes. Aucun chiffre n'est cité sans mesure.

## 1. État de l'art trouvé dans le code (avant toute modification)

| Domaine | Implantation réelle | Fichier |
|---|---|---|
| Multiplication BigInt | `num-bigint` (école → Karatsuba → **Toom-3 en place**) sous 520 000 bits ; au-delà NTT 2 modules de 62 bits, parallèle (`mul_rapide::mul_par`). Les routines maison école/Karatsuba/Toom-3 existent mais sont **mesurées plus lentes** que `num-bigint` (7,6 ms vs 1,8 ms à 8 000 limbs) et servent surtout de référence de test. | `mul_rapide.rs:96-169` |
| Division | inverse de Newton au-delà de 100 limbs, école en dessous (`div_rem_newton`). | `mul_rapide.rs:1773-1898` |
| PGCD | Lehmer hybride : Stein `num-bigint` sous 1 024 bits, sinon matrices unimodulaires à fenêtre u64 (32 étapes, coefficients ≤ 2^32). | `i128x.rs:591-640` |
| Puissance | exponentiation binaire ; raccourci O(1) par décalage pour les bases puissances de 2 (`2^20000` passe par là). | `i128x.rs:746-769`, `i128x.rs:955-971` |
| Factorielle | n ≤ 4 000 : produit séquentiel groupé par facteurs u64 ; au-delà : Légendre + crible + `p^e` parallèles + arbre de produit Huffman, NTT pour la fusion finale. | `i128x.rs:28-131`, `i128x.rs:345-392` |
| Primalité | Miller–Rabin : 12 bases pour `BigInt` ; voie u64 sans allocation avec **12 bases** (2,3,5,…,37) + crible de ces 12 petits facteurs. | `i128x.rs:1295-1365` |

Donc : multiplication, division, factorielle et PGCD étaient déjà à un niveau « GMP-like ».
Les deux cibles réellement perfectibles mesurées sont la **primalité u64** (12 témoins
redondants) et la **puissance** (binaire, alors qu'une fenêtre k-aire divise le nombre
de multiplications pleines).

## 2. Algorithmes RETENUS (gains ≥ 1,3×, parité bit-à-bit vérifiée)

### 2.1 Primalité u64 — 7 témoins déterministes de Sinclair

`TEMOINS_MR_U64 = [2, 325, 9375, 28178, 450775, 9780504, 1795265022]` : jeu
**déterministe pour tout n < 2^64**, remplace les 12 bases 2..37. Même crible de
petits facteurs, même `mulmod` en u128, même nombre d'exponentiations par
témoin. Le chemin `BigInt` (> 64 bits) **garde ses 12 bases** : on ne réduit pas
la certitude d'un test déjà probabiliste.

| Mesure (pin CPU 15, médiane de 3) | 12 bases (avant) | 7 bases (après) | Gain |
|---|---:|---:|---:|
| 256 u64 sans petits facteurs | 184,29 µs (rejoué 182,65) | 125,54 µs (rejoué 125,66) | **1,47×** (rejoué 1,45×) |
| 0..50 000 | 8 410,96 µs | 5 176,73 µs | **1,62×** |
| surface intégrée `Gz::est_premier` (256 u64, allocation Gz incluse) | 179,18 µs | 127,60 µs | **1,40×** |

Parité : tous les n de 0 à 100 000, 400 u64 aléatoires, `u64::MAX`, `2^62`,
3_825_123_056_546_413_051 (Carmichael) — 0 divergence.

### 2.2 Puissance — exponentiation k-aire à fenêtre

`pow_fenetre` : table `tab[i] = base^i` pour `i < 2^w` construite par carrés,
puis consommation de l'exposant en fenêtres de `w` bits (`r ← r^(2^w)·tab[d]`).
`w = 1` (binaire) si `e < 256`, `w = 2` jusqu'à 2^16, `w = 4` au-delà.
Raccourci « base puissance de 2 = un seul décalage » conservé.

| Mesure (base 123 456 789, médiane de 3) | binaire (avant) | fenêtre | Gain |
|---|---:|---:|---:|
| e = 5 000 | 499,50 µs | 347,68 µs (w=3) | **1,44×** |
| e = 20 000 | 4 169,91 µs | 2 867,46 µs (w=4) | **1,46×** |
| surface intégrée `Gz::pow_u64`, e = 5 000 | 498,93 µs | 348,31 µs | **1,43×** |
| surface intégrée `Gz::pow_u64`, e = 20 000 | 4 170,69 µs | 2 869,22 µs | **1,45×** |

Balayage w = 2..6 : w ≥ 5 est plus lent (table plus coûteuse que l'économie de
fenêtres). Parité : 200 bases signées aléatoires × exposants 0..12 000, 16
exposants limites (0, 1, 2^8, 2^16, …), façades `gmp_ui_pow_ui` et `Gz::pow_u64`
— 0 divergence.

## 3. CANDIDATS REJETÉS (chiffres mesurés, pas d'à-peu-près)

### 3.1 PGCD — Lehmer à fenêtre large / garde de Jebelean

Coefficients i128 portés de 2^32 à 2^48/2^62, 64–96 étapes u64 par matrice,
avec et sans garde `y ≥ 2^k` (Jebelean : arrêt dès que les 64 bits de tête ne
déterminent plus le quotient des grands nombres). Entrée : deux entiers de
1 000 chiffres, identiques au banc `bench/perf`.

| Variante | µs | Verdict |
|---|---:|---|
| actuel (2^32, 32 étapes) | 30,42 | référence |
| copie 2^32 / 32 (détail : `BigUint::from(u128)` au lieu de `as u64`) | 31,71 | −4 % |
| 2^48 / 64 | 69,73 | **2,3× plus lent** |
| 2^62 / 64 | 308,93 | **10× plus lent** |
| 2^62 / 96 | 309,22 | **10× plus lent** |
| garde 2^40 / 32 / y≥2^32 | 32,25 | plus lent |
| garde 2^48 / 32 / y≥2^32 | 32,24 | plus lent |
| garde 2^62 / 96 / y≥2^32 | 32,25 | plus lent |
| garde 2^62 / 96 / y≥2^40 | 40,19 | plus lent |

Explication mesurée : au-delà d'une trentaine d'étapes, les quotients estimés
sur les 64 bits de tête divergent de ceux des grands nombres ; la matrice ne
réduit plus `v`, le repli `u % v` est déclenché, et on paie les applications
BigInt pour rien. La fenêtre u64 de 32 étapes est le point optimal sur cette
machine (aarch64, division matérielle u64). Le PGCD reste inchangé.

### 3.2 Factorielle — arbre binaire et regroupement u128 (régime n ≤ 4 000)

| n | groupes u64 (actuel) | groupes u128 | arbre binaire (Borwein, sans Legendre) |
|---:|---:|---:|---:|
| 1 000 | 6,43 µs | 10,64 µs | 11,30 µs |
| 2 000 | 31,36 µs | 45,98 µs | 37,67 µs |
| 3 000 | 79,94 µs | 110,09 µs | 75,98 µs (1,05×) |
| 4 000 | 151,37 µs | 209,54 µs | 132,09 µs (1,15×) |

L'arbre ne dépasse 1,3× nulle part ; le regroupement u128 est **1,4× plus lent**
(`BigInt *= u128` coûte plus cher que `BigInt *= u64` malgré deux fois moins de
passes). Rien n'est intégré.

### 3.3 Multiplication / division

Déjà évaluées et documentées dans `mul_rapide.rs` : Toom-3 maison 4× plus lent
que `num-bigint`, NTT maison battu par `num-bigint` sous 256 000 limbs, seuil
d'arbre réel balayé (520 000 bits optimal). Aucun nouveau candidat (Toom-4,
NTT à 3 modules) n'a pu battre ces seuils dans ce chantier : on ne touche pas à
`mul_rapide.rs`.

## 4. Exactitude et validation

- `cargo test --release --test opt_algo` : 6/6 tests, dont la parité bit-à-bit
  des deux algorithmes intégrés sur des centaines de valeurs.
- `RUST_TEST_THREADS=1 cargo test --release` : 314 + 327 tests, 0 échec
  (journal `/tmp/opencode/opt_algo_validation.txt`).
- `bash scripts/validation_finale.sh` (avec `RUST_TEST_THREADS=1`) :
  **0 échec** ; point fixe `b3ed9262dd54776a6dbd07bc12368d0e`, AOT 75/75, JIT 25/25,
  WASM/RVM/HOST-1 OK.
- Fichiers modifiés : `rose/src/i128x.rs` (2 algorithmes), `rose/tests/opt_algo.rs`
  (banc + parité), `rose/docs/OPT_ALGOS.md` (ce document). Aucune modification de
  `interp.rs`, `jit.rs`, `run.rs`, `main.rs`, `compilateur/**`, `thorn/**`.

## 5. Incident de worktree (traçabilité)

Pendant la validation, un processus concurrent a fait `git reset --hard 98b1673`
sur ce worktree (reflog 08:12:15), supprimant les fichiers non commités. Le HEAD
est donc passé de `8ef12e66` à `98b1673`. Les trois fichiers ont été restaurés à
l'identique depuis les copies de travail et les mesures rejouées ; le diff final
ne porte QUE sur ces trois fichiers (voir rapport). Le chantier n'a pas été
interrompu.
