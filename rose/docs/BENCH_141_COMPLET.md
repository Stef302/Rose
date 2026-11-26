# BENCH_141_COMPLET — benchmark intégral des 141 programmes

Constat **daté et mesuré**, pas une promesse. Worktree exclusif `/tmp/opencode/v12bench` (HEAD `1d5d9d4`, release v0.1.82), sans commit ni push. Données brutes : `/tmp/opencode/bench141_final.tsv` — 141 lignes, `rose_ms<TAB>rust_ms<TAB>nom`, aucune ligne fabriquée, aucun cas écarté.

## 1. Protocole et fiabilité

- Moteur Rose : `/home/clausake/Stefan/ROSE/installateur/velopack/stage/moteur.elf` (md5 `e01362d5d0faed908a26feea136efc51`).
- Rust : `/home/clausake/Stefan/ROSE/rose/target/release/rose` (JIT actif).
- 141 sources `tests_selfhost/*.rose` (byte-identiques dans le worktree et le dépôt principal).
- Argument `/tmp/opencode/sh9_agents/battery/critique_entree.txt` pour `t80_*`, `t81_*`, `t999_*` uniquement.
- **Médiane de 3 tirages** par programme et par moteur, chronométrée autour de `fork/exec/wait`, stdout/stderr au `/dev/null`, temps en ms. Paire Rose/Rust mesurée consécutivement.
- Écriture ligne par ligne avec `flush()` immédiat, par lots de 20, reprise possible (le script relit le TSV pour déduire les noms déjà faits) : `/tmp/opencode/bench141_resume.py`.
- Garde-fou : tout tirage dépassant 5 s est tué et enregistré `TIMEOUT` (jamais converti en chiffre).

### Pourquoi le pass initial a été re-joué

L’exécution interrompue à 71/141 avait laissé 71 mesures en tirage unique. Un contrôle par médiane de 3 sur les mêmes binaires a montré qu’elles n’étaient **pas reproductibles** : `g5b511_rat_calc_negatif` y affichait 2,381 ms chez Rose contre 0,418 ms chez Rust (ratio 5,70), puis 1,310 ms contre 1,301 ms (ratio 1,01). Ces écarts venaient du régime de charge, pas du moteur. Les 71 lignes d’origine sont conservées telles quelles dans `/tmp/opencode/bench141_final.tsv.mixed-20261126`. Le présent document utilise **exclusivement** le pass de confirmation : les 141 programmes re-mesurés de bout en bout dans un seul régime, le même jour.

Contrôle de cohérence du pass retenu : temps médians Rose 1,296 ms sur 1..71 contre 1,284 ms sur 72..141 ; Rust 1,299 ms contre 1,295 ms. Les deux moitiés sont homogènes.

## 2. Résumé

| Métrique | Valeur |
|---|---:|
| Programmes mesurés | 141 / 141 |
| Rose gagne | **83 / 141** |
| Rust gagne | **58 / 141** |
| Égalités | 0 |
| Non-terminaison | 1 (`t224_promotion_execution`, Rust) |
| Σ temps Rose (141 cas, ms) | 183.348 |
| Σ temps Rust (140 cas terminés, ms) | 1325.609 |
| Ratio agrégé ΣRose/ΣRust sur les 140 cas terminés | **0.137** (Rose 7.28× plus rapide en somme) |
| Ratio agrégé vrai sur 141 (Rust = ∞ sur t224) | **0** |
| Ratio agrégé borne basse (t224 Rust compté au plafond 5 s) | 0.0290 |
| Médiane des ratios par programme (140 cas) | 0.992 |

Deux lectures distinctes, toutes deux réelles :

- **Par programme** : Rose gagne 83 cas sur 141, Rust 58. La médiane des ratios est 0.992, c’est-à-dire un quasi-égalité (Rose environ 1 % plus rapide en typique).
- **En somme** : ratio agrégé 0.137, mais il est dominé par deux cas Rust pathologiques et indépendants de la charge : `t72_si_imbrique` (Rust 1116.293 ms puis **segfault**, reproduit à 1091/1083/1080 ms en tirages isolés) et `t224_promotion_execution` (Rust ne termine jamais). La victoire de Rose en somme n’est donc **pas** une victoire sur la majorité des programmes.

Le gain réel de Rose est ailleurs : sur les programmes à appels imbriqués / récursion profonde / 36 arguments (`t100`, `t101`, `t108`, `t64`, `t75`, `t78`, `t79`, `t7b`) le ratio se situe autour de 0,38 — Rust passe ~3,4 ms contre ~1,3 ms pour Rose (surcoût de compilation JIT).

## 3. Tableau complet (141)

| # | Programme | Rose ms | Rust ms | ratio Rose/Rust | Verdict |
|---:|---|---:|---:|---:|---|
| 1 | `g5b501_unicode_latin1` | 1.287 | 1.230 | 1.046 | ❌ Rust |
| 2 | `g5b502_unicode_grec_cyrillique` | 1.331 | 1.356 | 0.982 | ✅ Rose |
| 3 | `g5b503_unicode_cjk` | 1.678 | 1.412 | 1.188 | ❌ Rust |
| 4 | `g5b504_unicode_astral` | 1.594 | 1.776 | 0.898 | ✅ Rose |
| 5 | `g5b505_unicode_compare_concat` | 1.430 | 1.444 | 0.990 | ✅ Rose |
| 6 | `g5b510_rat_calc_positif` | 1.307 | 1.359 | 0.962 | ✅ Rose |
| 7 | `g5b511_rat_calc_negatif` | 1.253 | 1.313 | 0.954 | ✅ Rose |
| 8 | `g5b512_rat_calc_somme` | 1.304 | 1.330 | 0.980 | ✅ Rose |
| 9 | `g5b513_rat_calc_boucle` | 1.269 | 1.482 | 0.856 | ✅ Rose |
| 10 | `g5b520_puissance_2_exposants` | 1.342 | 7.571 | 0.177 | ✅ Rose |
| 11 | `g5b521_puissance_bases_foldables` | 1.377 | 1.286 | 1.071 | ❌ Rust |
| 12 | `g5b522_puissance_encoding` | 1.328 | 3.356 | 0.396 | ✅ Rose |
| 13 | `g5b530_sous_big_divers` | 1.315 | 1.387 | 0.948 | ✅ Rose |
| 14 | `g5b531_sous_big_chaine` | 1.258 | 1.322 | 0.952 | ✅ Rose |
| 15 | `g5b532_sous_big_zero_neg` | 1.279 | 1.168 | 1.095 | ❌ Rust |
| 16 | `g5b540_mul_big_divers` | 1.262 | 1.227 | 1.029 | ❌ Rust |
| 17 | `g5b541_divmod_big_divers` | 1.180 | 1.278 | 0.923 | ✅ Rose |
| 18 | `g5b550_abs_big` | 1.277 | 1.299 | 0.983 | ✅ Rose |
| 19 | `g5b551_minmax_big` | 1.304 | 1.314 | 0.992 | ✅ Rose |
| 20 | `g5b560_add_signes_opposes` | 1.290 | 1.210 | 1.066 | ❌ Rust |
| 21 | `g5b601_cmp_big_litteraux` | 1.352 | 1.307 | 1.034 | ❌ Rust |
| 22 | `g5b602_add_big_retenue` | 1.281 | 1.322 | 0.969 | ✅ Rose |
| 23 | `g5b603_add_big_boucle` | 1.285 | 1.278 | 1.005 | ❌ Rust |
| 24 | `g5b604_depassement_limites` | 1.299 | 1.290 | 1.007 | ❌ Rust |
| 25 | `g5b605_neg_unaire_grand` | 1.276 | 1.297 | 0.984 | ✅ Rose |
| 26 | `g5b606_rat_litteraux` | 1.269 | 1.439 | 0.882 | ✅ Rose |
| 27 | `g5b607_longueur_ascii_unicode` | 1.340 | 1.345 | 0.996 | ✅ Rose |
| 28 | `g5b608_arithmetique_base` | 1.335 | 1.286 | 1.038 | ❌ Rust |
| 29 | `g5b609_logique_condition` | 1.332 | 1.309 | 1.018 | ❌ Rust |
| 30 | `g5b610_chaines_ascii` | 1.282 | 1.322 | 0.970 | ✅ Rose |
| 31 | `t01_entiers` | 1.316 | 1.299 | 1.013 | ❌ Rust |
| 32 | `t02_bool` | 1.362 | 1.333 | 1.022 | ❌ Rust |
| 33 | `t03_chaine_ascii` | 1.274 | 1.275 | 0.999 | ✅ Rose |
| 34 | `t04_chaine_unicode` | 1.340 | 1.302 | 1.029 | ❌ Rust |
| 35 | `t05_chaine_sous` | 1.276 | 1.311 | 0.973 | ✅ Rose |
| 36 | `t06_rationnel_lit` | 1.243 | 1.277 | 0.973 | ✅ Rose |
| 37 | `t07_rationnel_neg` | 1.142 | 1.276 | 0.895 | ✅ Rose |
| 38 | `t08_rationnel_calc` | 1.267 | 1.302 | 0.973 | ✅ Rose |
| 39 | `t09_flottant` | 1.374 | 1.304 | 1.054 | ❌ Rust |
| 40 | `t100_arg31` | 1.279 | 3.392 | 0.377 | ✅ Rose |
| 41 | `t101_arg36` | 1.354 | 3.489 | 0.388 | ✅ Rose |
| 42 | `t102_bool_arg4` | 1.347 | 1.244 | 1.083 | ❌ Rust |
| 43 | `t104_surcharge_arith` | 1.402 | 1.280 | 1.095 | ❌ Rust |
| 44 | `t105_surcharge_arite` | 1.368 | 1.350 | 1.013 | ❌ Rust |
| 45 | `t106_hof_param` | 1.300 | 1.224 | 1.062 | ❌ Rust |
| 46 | `t107_hof_var` | 1.342 | 1.339 | 1.002 | ❌ Rust |
| 47 | `t108_appels_imbriques` | 1.302 | 3.384 | 0.385 | ✅ Rose |
| 48 | `t10_liste_lit` | 1.319 | 1.226 | 1.076 | ❌ Rust |
| 49 | `t11_liste_mut` | 1.258 | 1.349 | 0.933 | ✅ Rose |
| 50 | `t12_table` | 1.330 | 1.287 | 1.033 | ❌ Rust |
| 51 | `t13_table_entier` | 1.360 | 1.342 | 1.013 | ❌ Rust |
| 52 | `t14_type` | 1.301 | 1.315 | 0.989 | ✅ Rose |
| 53 | `t15_chaine_bool` | 1.332 | 1.292 | 1.031 | ❌ Rust |
| 54 | `t200_cmp_big_litteraux` | 1.403 | 1.217 | 1.153 | ❌ Rust |
| 55 | `t201_cmp_big_variables` | 1.211 | 1.223 | 0.990 | ✅ Rose |
| 56 | `t202_cmp_big_negatifs` | 1.268 | 1.243 | 1.020 | ❌ Rust |
| 57 | `t203_cmp_big_calcul` | 1.234 | 1.276 | 0.967 | ✅ Rose |
| 58 | `t204_cmp_big_seuil` | 1.227 | 1.215 | 1.010 | ❌ Rust |
| 59 | `t205_cmp_big_dans_si` | 1.221 | 1.248 | 0.978 | ✅ Rose |
| 60 | `t20_add` | 1.274 | 1.317 | 0.967 | ✅ Rose |
| 61 | `t210_add_big_melange` | 1.397 | 1.281 | 1.091 | ❌ Rust |
| 62 | `t211_add_big_limbs` | 1.296 | 1.276 | 1.016 | ❌ Rust |
| 63 | `t212_add_big_signes` | 1.252 | 1.313 | 0.954 | ✅ Rose |
| 64 | `t213_add_big_retenue` | 1.196 | 1.243 | 0.962 | ✅ Rose |
| 65 | `t214_moins_big` | 1.316 | 1.293 | 1.018 | ❌ Rust |
| 66 | `t215_mul_div_mod_big` | 1.287 | 1.296 | 0.993 | ✅ Rose |
| 67 | `t21_sous` | 1.294 | 1.275 | 1.015 | ❌ Rust |
| 68 | `t220_depassement_2_59` | 1.251 | 1.201 | 1.042 | ❌ Rust |
| 69 | `t221_depassement_2_60_63` | 1.268 | 1.286 | 0.986 | ✅ Rose |
| 70 | `t222_depassement_2_64` | 1.258 | 1.291 | 0.974 | ✅ Rose |
| 71 | `t223_decimal_geant` | 1.230 | 1.299 | 0.947 | ✅ Rose |
| 72 | `t224_promotion_execution` | 1.267 | TIMEOUT | ∞ | ✅ Rose (Rust ne termine pas) |
| 73 | `t225_accumulation_boucle` | 1.354 | 1.264 | 1.071 | ❌ Rust |
| 74 | `t226_puissance_big` | 1.225 | 1.358 | 0.902 | ✅ Rose |
| 75 | `t22_mul` | 1.240 | 1.274 | 0.973 | ✅ Rose |
| 76 | `t230_neg_grands_litteraux` | 1.207 | 1.268 | 0.952 | ✅ Rose |
| 77 | `t231_neg_unaires` | 1.234 | 1.252 | 0.986 | ✅ Rose |
| 78 | `t232_neg_add_mixte` | 1.250 | 1.273 | 0.982 | ✅ Rose |
| 79 | `t233_neg_abs_minmax` | 1.253 | 1.260 | 0.994 | ✅ Rose |
| 80 | `t23_div` | 1.263 | 1.280 | 0.987 | ✅ Rose |
| 81 | `t240_rat_big_calcul` | 1.271 | 1.141 | 1.114 | ❌ Rust |
| 82 | `t241_rat_neg_calcul` | 1.268 | 1.316 | 0.964 | ✅ Rose |
| 83 | `t242_rat_litteraux_grands` | 1.345 | 1.242 | 1.083 | ❌ Rust |
| 84 | `t243_rat_somme_calcul` | 1.367 | 1.220 | 1.120 | ❌ Rust |
| 85 | `t24_mod` | 1.309 | 1.319 | 0.992 | ✅ Rose |
| 86 | `t250_unicode_chaines` | 1.294 | 1.305 | 0.992 | ✅ Rose |
| 87 | `t251_unicode_sous` | 1.284 | 1.247 | 1.030 | ❌ Rust |
| 88 | `t252_unicode_ordre_car` | 1.197 | 1.314 | 0.911 | ✅ Rose |
| 89 | `t253_unicode_comparaison` | 1.258 | 1.312 | 0.959 | ✅ Rose |
| 90 | `t25_puissance` | 1.304 | 1.346 | 0.969 | ✅ Rose |
| 91 | `t26_negation` | 1.279 | 1.271 | 1.006 | ❌ Rust |
| 92 | `t27_comparaisons` | 1.316 | 1.303 | 1.010 | ❌ Rust |
| 93 | `t28_bool_ops` | 1.278 | 1.285 | 0.995 | ✅ Rose |
| 94 | `t29_precedence` | 1.282 | 1.286 | 0.997 | ✅ Rose |
| 95 | `t30_abs_minmax` | 1.259 | 1.262 | 0.998 | ✅ Rose |
| 96 | `t31_partie_entiere` | 1.257 | 1.239 | 1.015 | ❌ Rust |
| 97 | `t32_big_2_63` | 1.240 | 1.251 | 0.991 | ✅ Rose |
| 98 | `t33_big_2_64` | 1.239 | 1.270 | 0.976 | ✅ Rose |
| 99 | `t34_big_decimal` | 1.306 | 1.289 | 1.013 | ❌ Rust |
| 100 | `t35_factorielle` | 1.253 | 1.302 | 0.962 | ✅ Rose |
| 101 | `t40_chaine_egale` | 1.348 | 1.328 | 1.015 | ❌ Rust |
| 102 | `t41_unicode_sous` | 1.278 | 1.295 | 0.987 | ✅ Rose |
| 103 | `t42_escape` | 1.362 | 1.314 | 1.037 | ❌ Rust |
| 104 | `t43_chaine_longue` | 1.370 | 1.233 | 1.111 | ❌ Rust |
| 105 | `t44_ordre` | 1.287 | 1.297 | 0.992 | ✅ Rose |
| 106 | `t45_sous_compare` | 1.322 | 1.298 | 1.018 | ❌ Rust |
| 107 | `t46_chaine_entier` | 1.308 | 1.353 | 0.967 | ✅ Rose |
| 108 | `t47_concat_diverse` | 1.270 | 1.332 | 0.953 | ✅ Rose |
| 109 | `t50_liste_vide` | 1.354 | 1.373 | 0.986 | ✅ Rose |
| 110 | `t51_liste_idx` | 1.316 | 1.230 | 1.070 | ❌ Rust |
| 111 | `t52_liste_croissance` | 1.318 | 1.316 | 1.002 | ❌ Rust |
| 112 | `t53_liste_alias` | 1.226 | 1.239 | 0.990 | ✅ Rose |
| 113 | `t54_liste_chaine` | 1.250 | 1.328 | 0.941 | ✅ Rose |
| 114 | `t55_liste_imbriquee` | 1.222 | 1.223 | 0.999 | ✅ Rose |
| 115 | `t60_table_bool` | 1.333 | 1.244 | 1.072 | ❌ Rust |
| 116 | `t61_table_multi` | 1.285 | 1.197 | 1.074 | ❌ Rust |
| 117 | `t63_table_chaines_cle` | 1.290 | 1.238 | 1.042 | ❌ Rust |
| 118 | `t64_table_dans_fonction` | 1.335 | 3.379 | 0.395 | ✅ Rose |
| 119 | `t70_pour` | 1.286 | 1.335 | 0.963 | ✅ Rose |
| 120 | `t71_tant_que` | 1.385 | 1.315 | 1.053 | ❌ Rust |
| 121 | `t72_si_imbrique` | 1.461 | 1116.293 | 0.0013 | ✅ Rose |
| 122 | `t73_fact_rec` | 1.353 | 3.336 | 0.406 | ✅ Rose |
| 123 | `t74_fib_rec` | 1.300 | 1.294 | 1.005 | ❌ Rust |
| 124 | `t75_pgcd_rec` | 1.269 | 3.342 | 0.380 | ✅ Rose |
| 125 | `t76_rec_mutuelle` | 1.358 | 1.262 | 1.076 | ❌ Rust |
| 126 | `t77_pour_imbrique` | 1.281 | 1.271 | 1.008 | ❌ Rust |
| 127 | `t78_retours_multiples` | 1.264 | 3.361 | 0.376 | ✅ Rose |
| 128 | `t79_rec_profonde` | 1.281 | 3.349 | 0.383 | ✅ Rose |
| 129 | `t7a_boucle_chaine` | 1.297 | 1.352 | 0.959 | ✅ Rose |
| 130 | `t7b_si_dans_boucle` | 1.292 | 3.407 | 0.379 | ✅ Rose |
| 131 | `t80_nb_arguments` | 1.324 | 1.320 | 1.003 | ❌ Rust |
| 132 | `t81_argument` | 1.266 | 1.283 | 0.987 | ✅ Rose |
| 133 | `t82_argument_hors` | 1.269 | 1.384 | 0.917 | ✅ Rose |
| 134 | `t83_lire_texte` | 1.291 | 1.294 | 0.998 | ✅ Rose |
| 135 | `t84_ecrire_texte` | 1.268 | 1.276 | 0.994 | ✅ Rose |
| 136 | `t85_ecrire_bytes` | 1.161 | 1.150 | 1.010 | ❌ Rust |
| 137 | `t86_ecrire_puis_lire` | 1.453 | 1.276 | 1.139 | ❌ Rust |
| 138 | `t90_graine` | 1.296 | 1.387 | 0.934 | ✅ Rose |
| 139 | `t91_prng` | 1.276 | 1.353 | 0.943 | ✅ Rose |
| 140 | `t92_hasard_bornes` | 1.386 | 1.348 | 1.028 | ❌ Rust |
| 141 | `t999_critique_g5` | 1.371 | 3.355 | 0.409 | ✅ Rose |

## 4. Top 15 des pires cas pour Rose

Tri par ratio Rose/Rust décroissant. **Aucun écart ne dépasse 1,19×** : dans ce régime de mesure, ce sont des écarts de bruit, pas des régressions structurelles. La cause indiquée est le chemin de code traversé, déduit du source du test — pas une certitude sur le moteur.

| # | Programme | Rose ms | Rust ms | ratio | Chemin de code traversé |
|---:|---|---:|---:|---:|---|
| 1 | `g5b503_unicode_cjk` | 1.678 | 1.412 | 1.19 | Comparaison/concaténation Unicode CJK : décodage de codepoints sur plusieurs octets. |
| 2 | `t200_cmp_big_litteraux` | 1.403 | 1.217 | 1.15 | Comparaisons de grands littéraux : Rose matérialise les bigints puis compare génériquement. |
| 3 | `t86_ecrire_puis_lire` | 1.453 | 1.276 | 1.14 | Aller-retour écriture/lecture de fichier : dominé par les appels système, écart dans le bruit. |
| 4 | `t243_rat_somme_calcul` | 1.367 | 1.220 | 1.12 | Somme de rationnels grands : normalisation/PGCD sur dénominateurs communs. |
| 5 | `t240_rat_big_calcul` | 1.271 | 1.141 | 1.11 | Calcul rationnel sur grands entiers : même chemin de réduction. |
| 6 | `t43_chaine_longue` | 1.370 | 1.233 | 1.11 | Chaîne longue : allocation et concaténation répétées. |
| 7 | `t104_surcharge_arith` | 1.402 | 1.280 | 1.10 | Détection de surcharge arithmétique : les deux moteurs refusent, seul le chemin d’erreur est chronométré. |
| 8 | `g5b532_sous_big_zero_neg` | 1.279 | 1.168 | 1.10 | Soustraction bigint avec zéro négatif : gestion du signe sur petit effectif. |
| 9 | `t210_add_big_melange` | 1.397 | 1.281 | 1.09 | Addition bigint de types mélangés : conversions de tag. |
| 10 | `t242_rat_litteraux_grands` | 1.345 | 1.242 | 1.08 | Littéraux rationnels grands : analyse et normalisation au chargement. |
| 11 | `t102_bool_arg4` | 1.347 | 1.244 | 1.08 | Booléens passés en argument : surcoût générique sur programme court. |
| 12 | `t76_rec_mutuelle` | 1.358 | 1.262 | 1.08 | Récursion mutuelle : plusieurs appels de fonction. |
| 13 | `t10_liste_lit` | 1.319 | 1.226 | 1.08 | Liste littérale : construction et indexation. |
| 14 | `t61_table_multi` | 1.285 | 1.197 | 1.07 | Table à multiples clés : insertions/recherches. |
| 15 | `t60_table_bool` | 1.333 | 1.244 | 1.07 | Table à clés booléennes : hachage de petites clés. |

Hors classement : `t224_promotion_execution` — Rose 1.267 ms, Rust **TIMEOUT** (non-terminaison, vérifiée à 35 s). C’est le cas qui avait coupé l’exécution précédente à 71/141 ; il compte comme une victoire Rose, pas comme un cas écarté.

## 5. Familles où Rose perd

**Aucune famille n’a un ratio médian supérieur à 1,10** dans le pass retenu. Les anciennes « pertes nettes » annoncées sur les rationnels `g5b51x` (ratio 3,83) et le divmod `g5b54x` (2,66) étaient des artefacts du régime de charge du pass interrompu ; leurs ratios fiables sont ci-dessous.

| Famille citée | n | ratio médian fiable | Détail |
|---|---:|---:|---|
| rationnels `g5b51x` | 4 | 0.958 | `g5b510_rat_calc_positif` 0.96, `g5b511_rat_calc_negatif` 0.95, `g5b512_rat_calc_somme` 0.98, `g5b513_rat_calc_boucle` 0.86 |
| divmod big `g5b54x` | 2 | 0.976 | `g5b540_mul_big_divers` 1.03, `g5b541_divmod_big_divers` 0.92 |
| comparaisons big `t20x` | 7 | 0.990 | `t200_cmp_big_litteraux` 1.15, `t201_cmp_big_variables` 0.99, `t202_cmp_big_negatifs` 1.02, `t203_cmp_big_calcul` 0.97, `t204_cmp_big_seuil` 1.01, `t205_cmp_big_dans_si` 0.98, `t20_add` 0.97 |
| additions big `t21x` | 7 | 1.015 | `t210_add_big_melange` 1.09, `t211_add_big_limbs` 1.02, `t212_add_big_signes` 0.95, `t213_add_big_retenue` 0.96, `t214_moins_big` 1.02, `t215_mul_div_mod_big` 0.99, `t21_sous` 1.01 |
| rationnels big `t24x` | 5 | 1.083 | `t240_rat_big_calcul` 1.11, `t241_rat_neg_calcul` 0.96, `t242_rat_litteraux_grands` 1.08, `t243_rat_somme_calcul` 1.12, `t24_mod` 0.99 |
| big `g5b60x` | 9 | 1.005 | `g5b601_cmp_big_litteraux` 1.03, `g5b602_add_big_retenue` 0.97, `g5b603_add_big_boucle` 1.01, `g5b604_depassement_limites` 1.01, `g5b605_neg_unaire_grand` 0.98, `g5b606_rat_litteraux` 0.88, `g5b607_longueur_ascii_unicode` 1.00, `g5b608_arithmetique_base` 1.04, `g5b609_logique_condition` 1.02 |
| unicode `g5b50x` | 5 | 0.990 | `g5b501_unicode_latin1` 1.05, `g5b502_unicode_grec_cyrillique` 0.98, `g5b503_unicode_cjk` 1.19, `g5b504_unicode_astral` 0.90, `g5b505_unicode_compare_concat` 0.99 |
| puissances `g5b52x` | 3 | 0.396 | `g5b520_puissance_2_exposants` 0.18, `g5b521_puissance_bases_foldables` 1.07, `g5b522_puissance_encoding` 0.40 |

Cas isolés dépassant 1,10 (tous dans le bruit) : `g5b503_unicode_cjk` 1.19 ; `t200_cmp_big_litteraux` 1.15 ; `t86_ecrire_puis_lire` 1.14 ; `t243_rat_somme_calcul` 1.12 ; `t240_rat_big_calcul` 1.11 ; `t43_chaine_longue` 1.11.

À l’inverse, les familles où Rose gagne nettement : appels/récursion `t100`/`t101`/`t108`/`t64`/`t75`/`t78`/`t79`/`t7b` (~0,38), puissances `g5b52x` (médiane 0,396, tirée par `g5b520_puissance_2_exposants` 0,18).

## 6. Contrôles

- 141 lignes, 141 noms uniques, aucun nom manquant, aucun doublon (vérifié par `comm`/`uniq`).
- md5 moteur : `e01362d5d0faed908a26feea136efc51` (conforme à la consigne).
- Un seul `TIMEOUT`, explicitement marqué, jamais converti en nombre.
- Aucun cas favorable sélectionné : les 141 noms sont ceux de `tests_selfhost/*.rose` triés, sans filtre.
- Aucun fichier du worktree modifié hormis ce document.

