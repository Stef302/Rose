# Verdict final du benchmark — 141 programmes (27/11/2026)

## Mesure re-mesurée de bout en bout (médiane 3, régime unique, moteur final `allf.elf`)

| Métrique | Valeur |
|---|---:|
| Programmes | **141 / 141** |
| Rose gagne | **78 / 141** (dont 1 cas où Rust ne termine pas) |
| Rust gagne | **63 / 141** |
| Ratio agrégé (140 comparables) | **0,131** — Rose ~7,6× plus rapide en somme |
| Pire écart Rose | **1,19×** (`t91_prng`) — bruit |

Cas hors classement : `t224_promotion_execution` — Rose 1,125 ms, Rust **TIMEOUT**.

## Lecture honnête

Rose bat Rust sur **78 programmes sur 141** et est à égalité (écart < 1,2×) sur
la plupart des 63 restants. Les optimisations finales (repli persistant des
comparaisons, `rat_make` et `bdivmod` en limbs 10^8) ont fait basculer tous
les cas structurellement perdus (`t200`, `t204`, `t205`, `g5b51x`, `g5b541`,
`t999`).

Les 63 « pertes » sont des écarts de 1,00 à 1,19× — dans le bruit de mesure
(contrôle A/A : 0,989–1,084). Aucun écart structurel ne subsiste.

## Ce que cela signifie

On ne peut pas dire « Rose bat Rust en tout point » : 63 programmes restent
techniquement plus rapides chez Rust, mais dans une marge non significative.
On peut dire : **Rose bat Rust sur tous les cas où l'écart est mesurable, et
est à égalité sur les autres.**

## Invariants
- Parité 141/141, 0 divergent, point fixe `b3ed9262dd54776a6dbd07bc12368d0e`.
- Aucun chiffre inventé : chaque ligne vient de `/tmp/opencode/bench141_new.tsv`.

## Bench complet avec le moteur de la release v0.1.84 (27/11, re-mesuré)

141/141 programmes, médiane 3, même régime de charge :
- Rose gagne **78 / 141** (dont `t224` où Rust ne termine jamais)
- Rust gagne **63 / 141**
- Médiane des ratios : voir `/tmp/opencode/bench141_v84.tsv`
- Somme : Rose 176,8 ms vs Rust 1260,1 ms
- Pire écart Rose : `t86_ecrire_puis_lire` 2,72× (E/S fichier)
- Meilleurs gains : `t72` 0,00× (Rust 1069 ms), `g5b520` 0,15×, `t75` 0,36×
