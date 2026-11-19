# Racines : Rose vs Rust — mesure, décomposition, optimisation

**Spécialiste RA — 19/11/2026. Constat daté, re-mesurable. Aucun commit, aucune publication.**

---

## 0. Protocole (commun à toutes les mesures)

- **Méthode** : A/A/A entrelacé (ordre A/B alterné à chaque répétition), **9 échantillons** par
  implémentation, **médianes** (min–max joints), pinage `taskset -c 6`, mêmes valeurs d'entrée,
  même checksum vérifié à chaque échantillon.
- **Deux horloges indépendantes** : H1 = `time.perf_counter()` (murale, autour du sous-processus) ;
  H2 = `resource.getrusage(RUSAGE_CHILDREN).ru_utime + ru_stime` (CPUenfants). Les deux horloges
  donnent des ratios concordants à ~0,5 % près (ex. C4b : 0,9351 vs 0,9393) — la mesure est propre.
- **Binaire Rose final** : `/tmp/opencode/ra_agents/rose_FINAL`, md5 `f08e8954f5c35c73528777efdbea1cb7`
  (reconstruit en fin de mission après qu'un autre agent a committé `e3f0ea4` et modifié des fichiers
  partagés ; mesures de contrôle G1/G2 re-jouées dessus, résultats identiques à ±0,3 %).
  **AVANT** : `rose_AVANT`, md5 `3080b36a79f76646f3524b0fab4b77df`. **`rose/rose` du dépôt n'a jamais
  été remplacé** (md5 inchangé `3d72c37163eeaa03e19c5262c06dc914`). Je n'ai fait aucun commit.
- **Référence Rust** : `/tmp/opencode/ra_agents/rust_ref_AVANT` md5 `53b46d884c2053164e28efd5d5d01c55`
  (algorithme d'origine) et `rust_ref_APRES` md5 `8f536e47aa81dbe7ee0ae86e3055efe7` (hybride).
  `rustc 1.99.1`, `num-bigint 0.5.1`, `num-integer 0.1.47`, **zéro GMP**.
- **Harnais** : `bench/racines/harness.py` ; programmes `racines_{i64,big,dec,loop}.rose` ;
  re-jouable via `bash bench/racines/run.sh` et `bash bench/racines/run_avant_apres.sh`.
- Bruit observé sur des mesures isolées : jusqu'à 80 % (ex. `dec 2000 100` lu à 2,29 puis 4,16 ms).
  **Aucune conclusion de ce document ne repose sur une mesure isolée.**

---

## 1. Deux prémisses vérifiées — l'une fausse, l'autre vraie

**Fausse (corrigée dans `irrationnel.rs`)** : le commentaire historique affirmait que
`num-bigint::sqrt` utilisait Karatsuba en `O(M(n)·log n)`. Lecture du code source de la dépendance
(`num-bigint-0.5.1/src/biguint.rs:455` + `num-integer-0.1.47/src/roots.rs`) : c'est **Newton avec
division** `s ← (s + a/s) >> 1`, estimation initiale `f64` quand `a` tient, sinon rééchelonnement
**1024 bits**. Le coût est donc dominé par des divisions pleine précision, pas par Karatsuba.

**Vraie (vérifiée)** : le chemin i64 `(x as f64).sqrt() as i64` suivi de `r*r==x` est exact.
Sonde Rust : 30 005 carrés parfaits autour de chaque puissance de 2 (2²⁷…2³²) → **0 erreur** ;
2 000 000 carrés aléatoires → **0 erreur**. Le correctif « Newton i64 » n'était donc pas nécessaire
pour l'exactitude — et il est plus lent (§5).

---

## 2. Décomposition chiffrée (où va le temps)

Médianes 9 échantillons, même binaire Rust hybride, H2 CPU :

| Étape | d=100, N=5000 | part | d=1000, N=500 | part |
|---|---:|---:|---:|---:|
| `10^(2d)` + `n·scale²` (sans sqrt) — `scale_mul` | 1,370 ms | 21 % | 1,675 ms | 13 % |
| + Newton (≈ `calcul_racine`) — `dec` | 6,504 ms | → **79 % sqrt** | 9,756 ms | → **62 % sqrt** |
| + `to_string()` — `tostring` | — | — | 12,840 ms | **+24 % chaîne** |
| **total primitive + chaîne** | — | — | **12,840 ms** | 100 % |

Côté Rose, la boucle interprétée coûte à elle seule **255,5 ms / 2 M itérations** (`racines_loop.rose`)
contre **508,6 ms** avec `racine()` (`racines_i64.rose`) : ~128 ns/itération de boucle, ~126 ns/itération
d'appel de primitive. Le surcoût d'interprétation est donc la cause dominante sur les petits travaux,
et il s'amortit quand le travail par appel croît (ratio Rose/Rust 53,4× en i64 → 1,05× à 1000 décimales).

---

## 3. (a) Résultats AVEC L'ALGORITHME D'ORIGINE

Algorithmes d'origine : i64 = `(x as f64).sqrt() as i64` + `r*r==x` ; BigInt/décimales =
`num_bigint::BigInt::sqrt()` (Newton num-integer, estimation f64 / échelonnement 1024).

### 3.1 Niveau langage — Rose (source Rose) vs Rust (appel direct), même salve

| Cas | Charge | Rose méd. (H1) | Rust méd. (H1) | Rose/Rust (H1) | Rose/Rust (H2) |
|---|---|---:|---:|---:|---:|
| C1 | √ carrés i64, N=2 000 000 | 505,99 ms | 9,75 ms | **51,90 ×** | 53,24 × |
| C2 | √ BigInt 50 chiffres, N=20 000 | 20,31 ms | 7,86 ms | **2,58 ×** | 2,65 × |
| C3 | √2 à 100 déc., N=5 000 | 10,87 ms | 8,13 ms | **1,34 ×** | 1,34 × |
| C4 | √2 à 1000 déc., N=500 | 15,69 ms | 14,81 ms | **1,06 ×** | 1,06 × |

Verdict : **Rust gagne les 4 cas à algo égal.** Rose ne bat Rust sur aucun domaine.

---

## 4. (b) Résultats AVEC LE MEILLEUR ALGORITHME — nommé précisément

**Algorithme retenu (hybride, `rose/src/irrationnel.rs::calcul_racine`)** :
- si `a.bits() <= 2048` → `BigInt::sqrt()` **d'origine** (Newton num-integer) ;
- si `a.bits() > 2048` → **Newton à échelonnement 2048 bits** : estimation initiale
  `(a >> scale).sqrt() << root_scale` avec `root_scale = ⌈(bits−2048)/2⌉`, puis le *même* `fixpoint`
  que num-bigint (`s ← (s+a/s)>>1`, phases croissante puis décroissante, saturation à `2^max_bits`).
  Le résultat reste exactement `floor(√a)`, ce n'est **pas** une approximation.

### 4.1 Avant / Après dans la même salve (9 échantillons, c6, 2 horloges)

| Cas | AVANT méd. (H1 / H2) | APRÈS méd. (H1 / H2) | Rapport AVANT/APRÈS (H1 / H2) |
|---|---:|---:|---:|
| B1 i64, 2 M | 506,26 / 505,72 ms | 506,21 / 505,94 ms | 1,0001 / 0,9996 — **aucun changement** (chemin i64 intact) |
| B2 BigInt 50 ch., 20 k | 19,89 / 19,79 ms | 19,78 / 19,65 ms | 1,0055 / 1,0073 — bruit |
| B3 √2 100 déc., 5 k | 10,43 / 10,33 ms | 10,47 / 10,41 ms | 0,9956 / 0,9928 — bruit (seuil non franchi) |
| **B4 √2 1000 déc., 500** | **15,26 / 15,13 ms** | **13,83 / 13,75 ms** | **1,1032 / 1,1000 — +10 % réels** |
| B5 √2 200 déc., 1000 | 4,66 / 4,59 ms | 4,73 / 4,60 ms | 0,9861 / 0,9967 — bruit (1329 bits < 2048) |

Sur B4, les 9 échantillons APRÈS (13,65–14,29 ms) ne recouvrent pas les 9 AVANT (15,08–15,43 ms) :
le gain est au-dessus du bruit. **Re-joué sur le binaire final** (G1) : AVANT 15,14 ms vs FINAL
13,73 ms → **1,1026 / 1,0996** (H1/H2), soit +10 % confirmés. Contre Rust hybride (G2) :
13,75 vs 12,92 ms → 1,0642 / 1,0614.

### 4.2 Porte « battre Rust » — deux comparaisons honnêtes, même salve

| Comparaison | A méd. (H1) | B méd. (H1) | A/B (H1 / H2) | Verdict |
|---|---:|---:|---:|---|
| **C4b** Rose hybride **vs Rust algo d'origine**, √2 1000 déc. | 13,75 ms | 14,70 ms | **0,9351 / 0,9393** | **Rose gagne (~6,5 %)** |
| **C4c** Rose hybride **vs Rust hybride**, √2 1000 déc. | 13,91 ms | 13,12 ms | 1,0609 / 1,0611 | Rust gagne (~6 %) |
| **D2** Rose final vs Rust hybride (après accès BigInt direct) | 13,73 ms | 13,08 ms | 1,0499 / 1,0513 | Rust gagne (~5 %) |

**Lecture honnête** : Rose ne bat Rust que lorsque l'adversaire garde l'algorithme dépassé. À
algorithme égal, l'écart résiduel de ~5 % est le surcoût d'interprétation (mesuré §2), pas le calcul.
La porte « 2 cas sur 3 » n'est **pas atteinte** ; le meilleur écart en faveur de Rose est **0,935 ×**
(C4b), et le meilleur écart défavorable est **53,4 ×** (F1, i64).

### 4.3 Niveau langage avec le binaire final (algo égal)

| Cas | Rose méd. (H1) | Rust méd. (H1) | Rose/Rust H1 / H2 |
|---|---:|---:|---:|
| F1 i64 2 M | 508,88 ms | 9,52 ms | 53,43 / 54,42 |
| F2 BigInt 20 k | 15,07 ms | 7,84 ms | 1,92 / 1,93 |
| F3 √2 100 déc. 5 k | 10,16 ms | 7,96 ms | 1,28 / 1,28 |
| D2 √2 1000 déc. 500 | 13,73 ms | 13,08 ms | 1,05 / 1,05 |

---

## 5. Décision et rejets (tous mesurés, A/A/A 9 échantillons)

| Candidat | Mesure | Décision |
|---|---|---|
| **Newton i64 sans f64** (`isqrt_u64`) pour le chemin exact | A1 : f64 9,86 ms vs Newton 28,04 ms sur 2 M → **f64 2,85× plus vite** ; et il est déjà exact (§1) | **REJETÉ** |
| **Newton échelonné 2048 appliqué partout** | A2 d=100 : 6,68 → 6,87 ms (**candidat 3 % plus lent**) ; A4 BigInt 50 ch. : 7,65 → 8,12 ms (**6 % plus lent**) | **REJETÉ** pour les petits |
| **Newton échelonné 2048 pour `bits>2048` seulement** | A3 d=1000 : 11,40 → 10,01 ms (**+14 %**) ; A5 avec chaîne : 14,71 → 13,37 ms (**+10 %**) ; B4 Rose : **+10 %** | **RETENU** |
| Échelonnement 3072 / 4096 / 6144 bits | d=1000 : 13,0 / 11,7 / 17,4 ms pour 400 it. contre 7,08 ms à 2048 → nettement pire | **REJETÉ** |
| Accès BigInt direct `Gz::bref()` au lieu de `chaine().parse()` dans la primitive | D1 : 13,98 → 13,96 ms (**0,15 %**, dans le bruit). Hypothèse « ~1,6 µs/appel » **infirmée** | Conservé (gratuit, sans gain mesurable) |

---

## 6. Exactitude absolue

- **Corpus** (`bench/racines/verifier_corpus.py`) : n = 2…20 × d ∈ {0,1,10,100,154,155,200,500,1000}
  = **171 cas**, comparés à `math.isqrt(n·10^(2d))` Python : **171/171 exacts** avant *et* après.
  Les points d=154/155 encadrent le passage `to_f64()` de num-bigint ; d=200 encadre le seuil 2048 bits.
- **i64** : 30 005 carrés autour des 2ᵖ + 2 × 10⁶ carrés aléatoires → 0 erreur.
- **Règle respectée** : `√n` = plus grand `r` avec `r² ≤ n` ; décimales = troncature exacte, pas un arrondi.
- **Zéro GMP** : `num-bigint` uniquement, y compris dans le binaire de référence.

---

## 7. Validation (binaire final `rose_APRES2`)

| Contrôle | Résultat |
|---|---|
| `cargo build --release --bin rose` | OK |
| `cargo test --release` | **214 + 198 + 6 + 1 = 419 passés, 0 échec** |
| Jardin `premiers` | **20 tests, 0 échec** |
| `fib(25)` | **75025** |
| Point fixe M4.5 (2 md5) | `b084ac5c926f6ae486845f9bc96c616d` = `b084ac5c926f6ae486845f9bc96c616d` — **ATTEINT** |
| Bench multi 9 domaines (D1…D8, c6, 9 éch.) | **9/9 domaines, 9/9 échantillons OK**, aucune valeur changée |
| Régression D1/D4 (interp.rs) | D1 11,655 → 11,598 ms ; D4 1,394 → 1,400 ms (bruit) — **aucune régression** |
| Corpus racines | 171/171 |

---

## 8. Fichiers livrés

- `rose/src/irrationnel.rs` — correction de la prémisse Karatsuba + hybride 2048 bits.
- `rose/src/interp.rs` — primitive `racine_decimales` / `sqrt_dec` (accès direct au BigInt).
- `rose/src/front.rs` — déclaration STDLIB de la primitive.
- `bench/racines/` — `harness.py`, `run.sh`, `run_avant_apres.sh`, `verifier_corpus.py`,
  `racines_{i64,big,dec,loop}.rose`.
- Sorties brutes : `/tmp/opencode/ra_agents/bench_out/*.json` et `/tmp/opencode/ra_agents/*.log`.
