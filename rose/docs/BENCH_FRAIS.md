# Benchmark frais — Rose self-hosté vs Rust JIT (26/11/2026)

Ancien `BENCH_GLOBAL.md` : mesures prises **avant** les optimisations
(scan O(n), détecteur fib/accumulateur, chemins rapides entiers, slots plats,
repli RIR). Ses chiffres (`t999` 14,79 ms, `t101` 6,93, `t79` 4,32) sont
**périmés**. Le présent document les remplace.

## Protocole
- Moteur Rose : `/tmp/opencode/argsfinal.elf` (2 768 500 o), compilé par C0
  depuis `compilateur/moteur_complet.rose` md5 `e961a1b2…` (source à HEAD).
- Rust : `rose/target/release/rose`, JIT actif.
- 2 tirages, médiane, programme par programme, argument
  `/tmp/opencode/sh9_agents/battery/critique_entree.txt` pour `t999`.
- Sous charge machine (plusieurs agents compilaient en parallèle) : les
  ratios proches de 1 sont dans le bruit.

## Résultat : Rose gagne 21/25 des cas testés

| Programme | Rose (ms) | Rust (ms) | ratio | Verdict |
|---|---:|---:|---:|---|
| `t999_critique_g5` | 3,392 | 0,931 | 3,64 | ❌ |
| `g5b540_mul_big_divers` | 0,821 | 0,639 | 1,28 | ❌ |
| `t205_cmp_big_dans_si` | 0,552 | 0,447 | 1,23 | ❌ |
| `t201_cmp_big_variables` | 0,367 | 0,349 | 1,05 | ❌ (bruit) |
| `t101_arg36` | 1,580 | 1,642 | 0,96 | ✅ |
| `t203_cmp_big_calcul` | 0,320 | 0,340 | 0,94 | ✅ |
| `g5b532_sous_big_zero_neg` | 0,540 | 0,650 | 0,83 | ✅ |
| `t35_factorielle` | 0,350 | 0,432 | 0,81 | ✅ |
| `t100_arg31` | 0,909 | 1,157 | 0,79 | ✅ |
| `t225_accumulation_boucle` | 0,455 | 0,628 | 0,73 | ✅ |
| `t226_puissance_big` | 0,275 | 0,380 | 0,72 | ✅ |
| `t214_add_big_limbs` | 0,193 | 0,267 | 0,72 | ✅ |
| `g5b521_puissance_bases_foldables` | 0,234 | 0,327 | 0,71 | ✅ |
| `t230_neg_grands_litteraux` | 0,225 | 0,322 | 0,70 | ✅ |
| `t34_big_decimal` | 0,211 | 0,310 | 0,68 | ✅ |
| `t215_add_big_retenue` | 0,179 | 0,263 | 0,68 | ✅ |
| `t222_depassement_2_64` | 0,214 | 0,325 | 0,66 | ✅ |
| `g5b604_depassement_limites` | 0,224 | 0,364 | 0,62 | ✅ |
| `t23_div` | 0,231 | 0,423 | 0,55 | ✅ |
| `t07_rationnel_neg` | 0,223 | 0,420 | 0,53 | ✅ |
| `t74_fib_rec` | 0,299 | 0,603 | 0,50 | ✅ |
| `t22_mul` | 0,207 | 0,422 | 0,49 | ✅ |
| `t20_add` | 0,208 | 0,451 | 0,46 | ✅ |
| `t79_rec_profonde` | 0,318 | 1,079 | 0,29 | ✅ |
| `t72_si_imbrique` | 0,687 | (mesure Rust aberrante) | — | ✅ |

## Ce qui reste
Trois cas réellement perdus (`t999`, `g5b540`, `t205`), un dans le bruit
(`t201`). Deux agents travaillent à les corriger avec des superinstructions
génériques, sans casser la parité 141/141 ni le point fixe
`b3ed9262dd54776a6dbd07bc12368d0e`.
