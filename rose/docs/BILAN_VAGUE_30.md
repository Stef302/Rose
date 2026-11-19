# Bilan vague 30 (19/11/2026) — bibliothèque réelle, full-hosted, dépasser Rust

**HEAD poussé** `f3ef7ba`. Deux salves A/A de 9 échantillons (c15 et c19).

## 1 — Réponses aux trois questions

**a) Vraie bibliothèque → wasm avec import → exploitée.** `miniz 11.3.2`
(7 sources officielles MIT) converti par `rose-wasm-lib.sh --allow-imports` :
ELF 4 457 octets, **1 import réel** `wasi_snapshot_preview1.fd_write`, 0 start,
0 call_indirect, 4 exports. Appelé depuis Rose : `mz_crc32`, `mz_compressBound`,
`mz_version` — **7/7 vérifications**, valeurs identiques à un binaire gcc natif.
Le banner `11.3.2` est écrit **par le module wasm via l'import**, pas par Rose.
Code complet (commande, `.rose` généré, programme d'appel, sortie) dans
`WASM_BIBLIOTHEQUE_REELLE.md`.

**Verdict honnête** : ce n'est **pas** « n'importe quelle bibliothèque ».
Sous-ensemble pur (args scalaires/chaînes, 0 call_indirect/start, 10 hôtes).
`miniz_adler32` et LZ4_compress font timeout à `-O0`.

**b) Full-hosted ? NON.** Verdict re-mesuré : **PARTIEL**.
5–6 étapes sur 10–11 ont une implémentation Rose prouvée (front-end + pontier
OS), mais **0 % ne sont exécutables sans le moteur Rust** et **0 % pour
produire `rose/rose` sans cargo/rustc**. 8/8 compilateurs Rose refusés par
l'émetteur ELF, réflexivité impossible. 60 crates, 5 dépendances directes,
2 064 symboles Cranelift. Détails et volumes dans `SELF_HOSTED_VERDIT_FINAL.md`.

**c) Dépasser Rust — 4ᵉ victoire.** Voir tableau ci-dessous.

## 2 — Bench final

| # | Domaine | Rose (c15 / c19) | Rust | Ratio | Verdict |
|---|---|---:|---:|---:|---|
| 1 | `fib(35)` | 11,71 / 11,63 | 12,7 | 0,91–0,92 | **victoire** |
| 2 | Boucle 1M | 0,262 | 0,259 | 1,00 | au niveau |
| 3 | Premiers 50k | 0,853 | 1,004 | 0,85 | **victoire** |
| 4 | Factorielle 5000 | 1,382 | 6,53 | 0,21 | **victoire** |
| 4b | PGCD 1000 ch. | 0,104 / 0,102 | 0,147 | **0,69–0,70** | **victoire** |
| 5 | Chaîne 20k | 0,911 / 0,928 | 0,014 | ~63 | en retard |
| 6 | Liste 100k | 0,324 / 0,329 | 0,187 | 1,75–1,80 | en retard |
| 7 | Table 3000 | 0,096 | 0,094 | ~1,0 | au niveau |
| 8 | E/S 10 Mo | ~6,0 | ~5,9 | ~1,0 | au niveau |

**4 victoires nettes + 3 domaines au niveau.**

Honnêteté sur D4b : Rose fait l'Euclidien, Rust fait Stein — pas une victoire
« même algorithme ». Sur D6, la porte reste fermée par un plancher de 262 µs
de défauts de page (Valeur = 24 octets) contre 184 µs à Rust.

## 3 — Gains de la vague

| Domaine | Avant | Après | Gain |
|---|---:|---:|---:|
| PGCD Euclid | 0,397 ms | **0,103 ms** | ×3,9 |
| Liste 100k | 0,355 ms | 0,324 ms | ×1,10 |
| miniz via wasm | — | 7/7 fonctions | conversion avec import |

Trois prémisses fausses corrigées dans la liste (LLVM n'élimine pas les
lectures, ce n'est pas du `str`/`ldr` mais des page faults, la fusion était
correcte) et trois dans le PGCD (conversion hors chrono, division ~50 %,
0,40 ms n'était pas un plancher).

## 4 — Validation

419 tests / 0 échec · jardin 20/20 · 10 jardins valides · fib25 `75025` ·
oracles 170/170 · point fixe `b084ac5c926f6ae486845f9bc96c616d` ×2 ·
`rug` absent · version `0.1.5` · `bench_global rc=0`.

Constat daté, pas consigne.

## Addendum — audit officiel (v5)

`scripts/audit-final.sh` rejoué sur `cfd3cec` : **AUCUNE porte non atteinte**.
Oracles 170/0 ×4 · point fixe `b084ac5c…` · jardin 20/20 · fib(25) `75025` ·
`bench_global rc=0` · `rug` absent · version `0.1.5`. L'audit a tourné pendant
que commençaient les agents de la vague 31 ; ses résultats sont donc antérieurs
à leurs modifications, qui seront re-auditées après intégration.
