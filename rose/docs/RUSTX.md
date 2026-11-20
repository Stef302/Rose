# RUSTX — course Rose vs Rust (subagent, 21/11/2026)

Constat daté sur le binaire isolé `/tmp/opencode/rustx_agents/rose_base`
(SHA256 `27e5fe53…`), HEAD `8f91228`. **Aucune mesure n'est citée :
toutes ont été re-mesurées sur le binaire courant.**

## Méthode

- Harnais `/tmp/opencode/rustx_agents/final_measure.py` : 21 échantillons,
  rotations appariées `c6` (Cortex-X925 3,9 GHz) / `c16`, `c19`
  (Cortex-A725 2,8 GHz), ratio calculé sur les **minimums** (robustes aux
  pics). `taskset -c` sur chaque tirage.
- ⚠ Trois processus `rose`/`rustc` d'autres agents ont tourné à 100 % CPU
  pendant une partie des salves (load 4–6). Les **ratios de la même salve**
  (base vs modifié) sont fiables ; les ratios absolus vs Rust sont bruités
  de ±0,1 et doivent être re-mesurés à jeun.

## Décomposition courante (binaire de départ, c6, `ROSE_TIMING`)

| Banc | parse | rep | transform | abaiss | prepare_jit | exec | goulot #1 |
|---|---:|---:|---:|---:|---:|---:|---|
| Suite | 40 µs | 1 | 2 | 10 | 14 | **538 µs** | overhead de préparation (~66 µs) : la boucle complexe est déjà 38 µs *plus rapide* que Rust (538 vs 558 µs) |
| Englobant | 102 µs | 5 | 13 | 3 | 37 | **1,90 ms** | S6 Monte-Carlo 1,21 ms + S7 Philox 650 µs (S4 66 µs, S5 35 µs) |
| Global | 81 µs | 11 | 5 | 8 | 44 | **2,60 ms** | flottants 1,02 ms + carrés 557 µs (tables 484 µs, RNG 264 µs) |

Ratios de départ (mins, 21 échantillons) : suite **1,12–1,13**, englobant
**1,30–1,35**, global **1,67–1,73**.

## Changement retenu (une décision structurelle)

**`rose/src/main.rs` — exécution directe sans thread.** Le moteur créait
systématiquement un thread avec 512 Mio de pile (`pthread_create` + `join`
mesurés à 23–30 µs sur un micro-bench isolé). Une détection conservative du
graphe d'appels (`programme_a_cycle_appel`) décide : sans cycle et sans
import, le programme tourne dans le *main thread* sous `catch_unwind`
(gestion de panic identique) ; avec cycle, le thread 512 Mio est gardé
(récursion profonde `it(10000)`, `fib`, jardins).

Gain apparié dans la même salve (base → modifié, 21 échantillons, mins) :
- suite : **0,91** sur c16 et c19 → ~9 % plus rapide ;
- englobant : 0,98 sur c6/c16 → ~1–2 % plus rapide.

Aucun autre fichier n'est modifié. Les tentatives suivantes ont été
**re-mesurées puis abandonnées** (aucun gain, code restauré depuis HEAD) :
scalaires déroulés à la place des tableaux dans `exec_mc` (identique :
tableaux 574 µs vs scalaires 443 µs en micro-bench, mais 1,12 ms dans le
moteur — le compilateur applique déjà SROA) ; entrelacement de 2 blocs Philox
(identique, 1,79 ms médiane).

## Blocages identifiés (pourquoi les 3 bancs ne sont pas battus)

1. **Global — formule fermée interdite.** Le jumeau Rust calcule la somme
   des carrés 1..10⁷ en **7,7 µs** (md5 de sortie identique : le compilateur
   a remplacé la boucle par `n(n+1)(2n+1)/6`), contre **557 µs** pour la
   superinstruction NEON de Rose, qui doit faire les 10⁶ additions. La
   consigne interdit explicitement « formule fermée ». Même en annulant
   tous les autres écarts (tables +200 µs, listes +61 µs), Rose reste
   ≥ 550 µs plus lent que le total Rust (~537 µs) → ratio plancher ≥ 2.
   Blocage structurel prouvé par la règle, pas une intention.

2. **Englobant et suite — constantes intégrées par rustc.** Le jumeau Rust
   a les bornes (`5`, `200000`) et le diviseur (`1000000`) en littéraux :
   LLVM déroule/vectorise (S6 = 250 µs). Mesuré avec des bornes opaques,
   le même code Rust tombe à **1,26 ms**, soit aussi lent que Rose
   (1,09 ms). Rose lit ces valeurs dans le RIR au runtime. Les reproduire
   exigerait de générer du code machine avec les constantes du *programme*
   intégrées (JIT dédié), chantier non mené à terme ici.

3. ⚠ Le jumeau Rust du global tronque (`1291990006563070912`) et utilise
   f64 (`7,8188…`) : Rose reste exact, cette sémantique n'a **pas** été
   adoptée.

## Validation (binaire courant)

- `cargo build --release` : OK. `cargo test --release` : **529 passed,
  0 failed** (6 ignored).
- Jardin `premiers.rose` : **20 tests, 0 échec**. `fib(25)` = **75025**.
- Point fixe : `rose rir compilateur/emetteur.rose` → md5
  `b3ed9262dd54776a6dbd07bc12368d0e` **×2 successifs**.
- Suite : sortie Rose = sortie Rust, md5 `675b5084632bf132f9dfb1d1a6da9770`.
- Englobant : 4 valeurs identiques `46297232.426252 / 2820 / 157158 /
  11899223598484` (F=5), sur c6/c16/c19.
- Global : rc=0, 12 lignes ; **9 valeurs identiques**, 2 divergences
  uniquement dues aux troncatures Rust documentées ci-dessus.
- Non re-joué : banc multi `/tmp/opencode/benchmulti/bench_now.py` (dossier
  supprimé) et "détail géant" (référence introuvable dans le dépôt).

## État final

| Banc | Rose battu ? | Constat |
|---|---|---|
| Suite | **Non** (1,12–1,13) | boucle déjà plus rapide, overhead non compensé |
| Englobant | **Non** (1,30–1,35) | bornes constantes Rust |
| Global | **Non** (1,67–1,73) | formule fermée LLVM interdite à Rose |

Objectif **non atteint**. Blocage prouvé pour le global ; travail restant
(JIT spécialisé sur constantes) pour les deux autres.
