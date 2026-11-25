# BENCH_GLOBAL — benchmark global des 141 programmes (Rose self-hosté vs Rust JIT)

**Constat daté** : mesuré le 2026-11-25T22:17:19.742824+00:00 (UTC). Worktree exclusif `/tmp/opencode/v6perf`, HEAD `7085f79`, aucun commit ni push.

## 1. Protocole réellement exécuté

| Élément | Mesure |
|---|---|
| Moteur Rose | moteur self-hosté **compilé par C0** depuis `compilateur/moteur_complet.rose` à HEAD `7085f79` : ELF aarch64 `2615540` octets, md5 `bab13df633e53d15b1ee181029a717c2`, rendu exécutable (755) |
| C0 | `/tmp/opencode/C0_i.elf`, md5 `2db168b907b4a874712857d3957fd078` ; compilation sous garde-fou `scripts/garde_fou_mem.sh 12000000` (rc=0) |
| Moteur Rust JIT | `/home/clausake/Stefan/ROSE/rose/rose`, md5 `7614a534b9411917ef9cd49a834014a8`, **JIT actif** (`ROSE_SANS_JIT` absent de l'environnement) |
| Corpus | les **141** sources `tests_selfhost/*.rose`, avec les fixtures du harnais de parité `/tmp/opencode/harnais_corpus141.sh` (`/tmp/opencode/sh9_agents/battery/`, `/tmp/aotwork/entree.txt`), argument `critique_entree.txt` pour `t80_*`, `t81_*` et `t999_*` |
| Pin CPU | `taskset -c 18` (toute la salve, sans autre processus sur ce cœur) |
| Échantillons | **3 salves indépendantes × 5 tirages** par moteur et par programme, entrelacés A/B avec **alternance de l'ordre** à chaque tirage, 1 échauffement ; temps **mur** `perf_counter_ns` ; valeur reportée = médiane des 3 médianes |
| Contrôle bruit | A/A : même protocole avec le **même binaire Rust des deux côtés** (2 salves × 5) |
| Parité | `rc` + stdout + stderr du moteur C0 comparés à l'oracle canonique (**Rust interprète**, `ROSE_SANS_JIT=1`) à chaque tirage ; le Rust JIT est comparé séparément |
| Timeout | 60 s par tirage |

⚠ Le Rust **JIT** affiche les booléens en `1`/`0` alors que l'oracle interprète et le moteur C0 affichent `vrai`/`faux` : 8 programmes ont un md5 JIT différent. Ce n'est pas une divergence de calcul — la parité est validée contre l'oracle canonique du harnais de corpus. Le JIT reste le moteur **chronométré**.

Une salve préliminaire où l'ELF C0 n'était pas encore marqué exécutable (rc=126 de `timeout`) a été **écartée** ; le binaire a été passé en 755 puis toutes les mesures ci-dessous ont été refaites.

Verdict : ✅ = Rose ≤ Rust (Rose gagne), ❌ = Rust < Rose (Rust gagne). `ratio` = temps Rose / temps Rust, < 1 = Rose plus rapide.

## 2. Résumé

| Métrique | Valeur |
|---|---:|
| Parité rc + stdout vs oracle canonique | **141 / 141** |
| Parité stricte (rc + stdout + stderr) | 139 / 141 (2 BLOQUES : `t107_hof_var`, `t55_liste_imbriquee`) |
| Rose gagne (✅) | **78 / 141** |
| Rust gagne (❌) | **63 / 141** |
| Somme des médianes Rose | 174.12 ms |
| Somme des médianes Rust | 150.76 ms |
| Ratio global (sommes) | **1.155 ×** (Rust plus rapide) |
| Médiane des ratios par programme | 0.978 × |
| Contrôle A/A (même binaire) | 72 / 141 « victoires » ; médiane 0.9948 ; étendue 0.797–1.530 |
| Point fixe émetteur (2 voies) | `b3ed9262dd54776a6dbd07bc12368d0e` ×2 |

**Lecture honnête** : Rose gagne sur 78 petits programmes (médiane par programme 0.978), mais **Rust gagne en agrégat** (1.155 ×) à cause d'un petit nombre de cas très coûteux `t999`, `t101`, `t79`. Le contrôle A/A rend 72/141 victoires avec une médiane de 0.9948 : les écarts proches de 1 ne sont pas significatifs sous la charge de la machine (plusieurs agents compilaient en parallèle).

Sensibilité mesurée (salves valides, même source compilée par C0) :

| pin / charge | salves | Rose gagne | ratio des sommes |
|---|---:|---:|---:|
| cœur 5 (charge ~12) | 1 × 5 | 71 / 141 | 1.037 |
| cœur 15 (voisin occupé) | 3 × 5 | 117 / 141 | 0.840 |
| **cœur 18 (salve finale, retenue)** | **3 × 5** | **78 / 141** | **1.155** |
| cœur 18, A/A même binaire | 2 × 5 | 72 / 141 | 1.004 |

## 3. Les 10 pires cas pour Rose

| # | programme | Rose (ms) | Rust (ms) | ratio | % du temps Rose | cause |
|---:|---|---:|---:|---:|---:|---|
| 1 | `t999_critique_g5` | 14.789 | 1.604 | **9.22 ×** | 8.5 % | 20 primitives G5 enchaînées (lire_texte, argument, sous_chaine, ordre, caractere, table…), aucune superinstruction |
| 2 | `t101_arg36` | 6.929 | 2.276 | **3.04 ×** | 4.0 % | appel de fonction à 36 arguments (allocation + conversion du vecteur d'arguments) |
| 3 | `t79_rec_profonde` | 4.324 | 1.841 | **2.35 ×** | 2.5 % | récursion utilisateur somme(100) : chaque appel repasse par le dispatch RIR |
| 4 | `t100_arg31` | 3.130 | 1.733 | **1.81 ×** | 1.8 % | appel de fonction à 31 arguments |
| 5 | `g5b532_sous_big_zero_neg` | 1.455 | 0.805 | **1.81 ×** | 0.8 % | soustraction bigint depuis zéro et avec grands négatifs (boxage num-bigint) |
| 6 | `g5b540_mul_big_divers` | 1.036 | 0.610 | **1.70 ×** | 0.6 % | multiplication de grands entiers (boxage + repli hors i64) |
| 7 | `t72_si_imbrique` | 1.432 | 0.849 | **1.69 ×** | 0.8 % | si imbriqués : aucun saut de branche compilé dans le RIR self-host |
| 8 | `t205_cmp_big_dans_si` | 1.827 | 1.098 | **1.66 ×** | 1.0 % | comparaison bigint à l'intérieur d'un si (deux boxages + branchement) |
| 9 | `t225_accumulation_boucle` | 2.338 | 1.453 | **1.61 ×** | 1.3 % | accumulation en boucle sur valeur boxée, pas d'accumulateur i64 dédié |
| 10 | `t201_cmp_big_variables` | 1.292 | 0.803 | **1.61 ×** | 0.7 % | comparaison de grands entiers issus de variables |

Ratios par salve pour ces 10 cas (stabilité du signal) :

| programme | salve 1 | salve 2 | salve 3 | A/A même binaire |
|---|---:|---:|---:|---:|
| `t999_critique_g5` | 10.32 | 7.21 | 9.22 | 1.13 |
| `t101_arg36` | 3.01 | 3.09 | 2.94 | 0.92 |
| `t79_rec_profonde` | 2.43 | 2.35 | 2.29 | 0.98 |
| `t100_arg31` | 2.24 | 1.42 | 1.73 | 1.09 |
| `g5b532_sous_big_zero_neg` | 1.32 | 1.81 | 1.35 | 1.11 |
| `g5b540_mul_big_divers` | 1.70 | 1.84 | 2.11 | 1.06 |
| `t72_si_imbrique` | 1.59 | 1.90 | 1.60 | 0.98 |
| `t205_cmp_big_dans_si` | 1.66 | 2.37 | 1.25 | 0.93 |
| `t225_accumulation_boucle` | 2.21 | 2.16 | 1.43 | 0.87 |
| `t201_cmp_big_variables` | 1.93 | 1.06 | 1.61 | 0.84 |

## 4. Optimisations à faire pour battre Rust partout (classées par impact)

| Rang | Poste | Part du temps Rose | Mesure | Cause | Action |
|---:|---|---:|---|---|---|
| 1 | **t999 — primitives G5 enchaînées** | 8.5 % (1 programme, 14.8 ms) | Rose 14.79 ms, Rust 1.60 ms, **9.22 ×** ; 3 salves cohérentes | une vingtaine de primitives (`lire_texte`, `argument`, `sous_chaine`, `ordre`, `caractere`, `chaine`, `table`, `inserer`, …) traversent le dispatch de l'interpréteur self-host à chaque appel | émettre une **superinstruction RIR** pour la séquence de critique G5, ou réutiliser le RIR compilé d'une exécution à l'autre (cache disque) ; cible < 2 ms |
| 2 | **appels à 31/36 arguments** | 9.2 % (8 programmes, 16.1 ms) | t101 3.04 ×, t100 1.81 × | le moteur self-host alloue et convertit un vecteur de paramètres pour chaque appel | passer les arguments par **slots plats indexés** (l'index direct slot→globale existe déjà dans HEAD, l'étendre au passage d'arguments) |
| 3 | **récursion utilisateur** | 8.9 % (10 programmes, 15.6 ms) | t79 2.35 × ; la famille pèse 8.9 % du temps Rose | seul le motif fib95 est détecté ; `somme(n)` retombe sur un appel RIR complet à chaque niveau | généraliser la détection de récurrence structurelle (motif accumulateur) du JIT Rust au moteur self-host |
| 4 | **bigint / comparaisons / rationnels** | 22.3 % (31 programmes, 38.8 ms) | médiane 1.181 × sur 31 programmes | grands entiers boxés (num-bigint) et comparaisons hors i64 dans le RIR interprété | superinstructions `cmp_big`, `add_big`, `divmod_big` + garde de plage i64 avant boxage |
| 5 | **backend G5 (g5b*)** | 19.7 % (30 programmes, 34.3 ms) | médiane 1.188 × sur 30 programmes | mêmes primitives arithmétiques G5 (sous_big, rat_calc, abs_big, puissance) que le poste précédent | factoriser ces primitives dans le noyau et partager les superinstructions avec t200-243 |
| 6 | **chaînes / unicode** | 4.2 % (8 programmes, 7.4 ms) | médiane 0.952 × sur 8 programmes | `longueur`/`sous_chaine`/`ordre` recalculent un index UTF-8 à chaque appel | index UTF-8 en une passe + superinstructions pour concaténer/tronquer |
| 7 | **listes / tables** | 2.7 % (6 programmes, 4.6 ms) | médianes 0.867 × et 0.977 × | `joindre`/`inserer` passent par le dispatch RIR | superinstructions de collections déjà prouvées dans le JIT Rust (GLOBAL_BEAT.md §2) à porter au self-host |

À l'inverse, **ne pas toucher** aux chemins où Rose est déjà largement devant : `t74_fib_rec` 0.53 ×, `t70_pour` 0.57 ×, `t25_puissance` 0.61 × (détection fib95 et boucles déjà optimisées en HEAD).

## 5. Tableau complet (141 programmes)

Tri par nom. `md5_sortie` = md5 du stdout du moteur C0 (= stdout de l'oracle canonique Rust interprète, octet pour octet).

| # | nom | md5_sortie | temps Rose (ms) | temps Rust JIT (ms) | ratio | verdict |
|---:|---|---|---:|---:|---:|:---:|
| 1 | `g5b501_unicode_latin1` | `6bb52cd2e6e69a0081f14ac9b060bb47` | 0.962 | 0.982 | 0.980 | ✅ |
| 2 | `g5b502_unicode_grec_cyrillique` | `7f590170a7fa8c5f302ca7846b0e5c63` | 0.946 | 0.922 | 1.026 | ❌ |
| 3 | `g5b503_unicode_cjk` | `e109fd2d0358bcdaf2f6c3487d0f84d8` | 0.972 | 1.069 | 0.909 | ✅ |
| 4 | `g5b504_unicode_astral` | `6057aa9954ac11248c5725fe2fcceaca` | 1.105 | 1.045 | 1.057 | ❌ |
| 5 | `g5b505_unicode_compare_concat` | `77f33c0ac4f466a403b9cc33f271b695` | 0.773 | 0.848 | 0.912 | ✅ |
| 6 | `g5b510_rat_calc_positif` | `45e2c37851a41b562ec29605fca2ad07` | 1.153 | 1.026 | 1.123 | ❌ |
| 7 | `g5b511_rat_calc_negatif` | `551d40c35151d7f0562f14696b11a705` | 1.290 | 0.872 | 1.479 | ❌ |
| 8 | `g5b512_rat_calc_somme` | `b5dcfd5b90d1b97595e432895c60cd69` | 1.181 | 1.047 | 1.129 | ❌ |
| 9 | `g5b513_rat_calc_boucle` | `1f1c3030583f53b7e0b15aa424fd444a` | 1.464 | 1.015 | 1.442 | ❌ |
| 10 | `g5b520_puissance_2_exposants` | `d596766b2e3032e0c6c025607b7c67fb` | 1.217 | 0.941 | 1.293 | ❌ |
| 11 | `g5b521_puissance_bases_foldables` | `1fe636ee517b6f548bce2609721aa488` | 0.839 | 0.849 | 0.988 | ✅ |
| 12 | `g5b522_puissance_encoding` | `3f7468bc9025959f10bb4f4581023140` | 1.292 | 1.838 | 0.703 | ✅ |
| 13 | `g5b530_sous_big_divers` | `10f6922d3af37050b749d9ea12119fef` | 1.213 | 1.016 | 1.194 | ❌ |
| 14 | `g5b531_sous_big_chaine` | `0ea0b20cf5563f47e47c3797996acedf` | 1.165 | 0.966 | 1.206 | ❌ |
| 15 | `g5b532_sous_big_zero_neg` | `014465ba1a7097c610154fd8cf41c549` | 1.455 | 0.805 | 1.806 | ❌ |
| 16 | `g5b540_mul_big_divers` | `29115b0a1ad05a0b7380bd0dcc6b0bb6` | 1.036 | 0.610 | 1.698 | ❌ |
| 17 | `g5b541_divmod_big_divers` | `67b2f5e71897b5aaf62c8f4c9f06d4d4` | 1.119 | 0.883 | 1.266 | ❌ |
| 18 | `g5b550_abs_big` | `9981391d6330659cfd47ad43547fd85d` | 0.989 | 0.837 | 1.183 | ❌ |
| 19 | `g5b551_minmax_big` | `816494d703978a7555a6cdbf7e49b88e` | 1.128 | 0.824 | 1.369 | ❌ |
| 20 | `g5b560_add_signes_opposes` | `87dafe906ada86994a8889b6a8c10e8d` | 1.382 | 1.016 | 1.359 | ❌ |
| 21 | `g5b601_cmp_big_litteraux` | `957f054d99ec0c315d056f12266bb304` | 1.414 | 1.118 | 1.265 | ❌ |
| 22 | `g5b602_add_big_retenue` | `36b96487c5483724673a51eb09fbb828` | 1.092 | 0.833 | 1.311 | ❌ |
| 23 | `g5b603_add_big_boucle` | `61d6d72d48a4610855a46308a5eeef85` | 2.096 | 1.450 | 1.445 | ❌ |
| 24 | `g5b604_depassement_limites` | `fcd295f405ff234e53a5073c99cf4c43` | 1.041 | 1.073 | 0.970 | ✅ |
| 25 | `g5b605_neg_unaire_grand` | `a5edf9a01aec0a7ba6af056e0c1226f4` | 0.862 | 1.038 | 0.831 | ✅ |
| 26 | `g5b606_rat_litteraux` | `4b0b9e11b528eb28d986e89c0eac731b` | 1.159 | 0.992 | 1.168 | ❌ |
| 27 | `g5b607_longueur_ascii_unicode` | `c4cd0bd0a2aed5e3256ffdb54dee1f1b` | 0.626 | 0.602 | 1.040 | ❌ |
| 28 | `g5b608_arithmetique_base` | `0dee3572fad4fb75cf617fed318a9445` | 1.045 | 0.931 | 1.121 | ❌ |
| 29 | `g5b609_logique_condition` | `16d65bce30874955891c6b7f29ecc400` | 1.137 | 0.897 | 1.268 | ❌ |
| 30 | `g5b610_chaines_ascii` | `b4688abd6c1e24bba1ff37b63c60ff4f` | 1.151 | 0.931 | 1.236 | ❌ |
| 31 | `t01_entiers` | `25114f087551f65a7442d5c849623f8f` | 0.900 | 1.118 | 0.805 | ✅ |
| 32 | `t02_bool` | `522afbf84e6eaf396986ca12c55d74f9` | 1.179 | 2.194 | 0.537 | ✅ |
| 33 | `t03_chaine_ascii` | `afe1b17de0095f112a313fd1e43bcd20` | 1.204 | 1.045 | 1.152 | ❌ |
| 34 | `t04_chaine_unicode` | `3ce71432e5c88564d56ac7e66440d53c` | 0.602 | 0.717 | 0.840 | ✅ |
| 35 | `t05_chaine_sous` | `c5ce48df26d99c2e6bedb4ef18239a8d` | 0.812 | 0.865 | 0.939 | ✅ |
| 36 | `t06_rationnel_lit` | `05c2476b3825447912c0e557bf6a845c` | 0.854 | 0.935 | 0.913 | ✅ |
| 37 | `t07_rationnel_neg` | `1bd95cf65f2ade6f1329051222102cf0` | 0.780 | 0.996 | 0.783 | ✅ |
| 38 | `t08_rationnel_calc` | `4aff31ac47fe115ead80567a263299d7` | 0.977 | 0.967 | 1.011 | ❌ |
| 39 | `t09_flottant` | `7e162b74a321fd72875f430c897765d3` | 0.958 | 1.386 | 0.691 | ✅ |
| 40 | `t100_arg31` | `4f89a0f6c113ae3ff279af1e6c6286bb` | 3.130 | 1.733 | 1.807 | ❌ |
| 41 | `t101_arg36` | `971658bc2f5bdee5660844a83b5bf0a2` | 6.929 | 2.276 | 3.044 | ❌ |
| 42 | `t102_bool_arg4` | `6c443c6d56a3bf9136a2bd62943195ff` | 1.142 | 1.206 | 0.947 | ✅ |
| 43 | `t104_surcharge_arith` | `d41d8cd98f00b204e9800998ecf8427e` | 0.755 | 0.682 | 1.107 | ❌ |
| 44 | `t105_surcharge_arite` | `d41d8cd98f00b204e9800998ecf8427e` | 0.722 | 0.950 | 0.760 | ✅ |
| 45 | `t106_hof_param` | `d41d8cd98f00b204e9800998ecf8427e` | 1.031 | 1.159 | 0.889 | ✅ |
| 46 | `t107_hof_var` | `d41d8cd98f00b204e9800998ecf8427e` | 0.861 | 0.858 | 1.002 | ❌ |
| 47 | `t108_appels_imbriques` | `0a933f8cd3160bc81e292fa6d8c16067` | 1.508 | 1.784 | 0.846 | ✅ |
| 48 | `t10_liste_lit` | `1a2222974e5976ad968e2bcac2534ea5` | 0.931 | 1.063 | 0.876 | ✅ |
| 49 | `t11_liste_mut` | `24f6e6d78ba285f5c3c90539890a1924` | 0.604 | 0.820 | 0.737 | ✅ |
| 50 | `t12_table` | `a076fc49d56fb50af0b2071cf909931c` | 1.068 | 1.024 | 1.043 | ❌ |
| 51 | `t13_table_entier` | `17c8d0433bf3339ab49bd02af2ec59aa` | 0.796 | 0.927 | 0.859 | ✅ |
| 52 | `t14_type` | `8a3da5f7c130f651b812d9dc4e8e5ea8` | 0.554 | 0.700 | 0.792 | ✅ |
| 53 | `t15_chaine_bool` | `a455cc989c1909d0e8b424fc069ea5e9` | 0.678 | 0.766 | 0.885 | ✅ |
| 54 | `t200_cmp_big_litteraux` | `35ee609de5df078a9fde8476ce5e6eda` | 1.706 | 1.238 | 1.378 | ❌ |
| 55 | `t201_cmp_big_variables` | `e7854a4fd0a14b6b5dda30c51bdadc65` | 1.292 | 0.803 | 1.608 | ❌ |
| 56 | `t202_cmp_big_negatifs` | `c64c49b42ba1830507ab5f48dd970842` | 1.749 | 1.224 | 1.429 | ❌ |
| 57 | `t203_cmp_big_calcul` | `957f054d99ec0c315d056f12266bb304` | 1.465 | 1.096 | 1.336 | ❌ |
| 58 | `t204_cmp_big_seuil` | `c64c49b42ba1830507ab5f48dd970842` | 1.598 | 1.156 | 1.383 | ❌ |
| 59 | `t205_cmp_big_dans_si` | `2a773165cc39d9533da87def36542d58` | 1.827 | 1.098 | 1.664 | ❌ |
| 60 | `t20_add` | `13d3fafd4cb07bb96cbb76a95b96edbf` | 0.984 | 1.292 | 0.761 | ✅ |
| 61 | `t210_add_big_melange` | `a8be43765d93c209eb3aa63c947a3cb8` | 1.828 | 1.310 | 1.395 | ❌ |
| 62 | `t211_add_big_limbs` | `5d2586a6caf84509cf312cc73ef6a26c` | 1.328 | 1.019 | 1.303 | ❌ |
| 63 | `t212_add_big_signes` | `685c008250cfaea2907750460fc3390f` | 1.646 | 1.025 | 1.605 | ❌ |
| 64 | `t213_add_big_retenue` | `2e2873d6a7ba3102f169a0ec0b647b74` | 0.815 | 0.921 | 0.885 | ✅ |
| 65 | `t214_moins_big` | `c4dc54cfbbcc6a8fdca022755b124658` | 1.110 | 0.864 | 1.284 | ❌ |
| 66 | `t215_mul_div_mod_big` | `4c1ac504a02c3008de31d0ea38387d9d` | 0.968 | 0.866 | 1.119 | ❌ |
| 67 | `t21_sous` | `a83d9a5e10fab5493babb8008d1f1b35` | 0.815 | 1.109 | 0.735 | ✅ |
| 68 | `t220_depassement_2_59` | `27057b425b28c95ab0037f1ea01715b5` | 0.827 | 1.181 | 0.700 | ✅ |
| 69 | `t221_depassement_2_60_63` | `8013d6e113736baafec9d3bcc5d7fca8` | 0.995 | 0.968 | 1.028 | ❌ |
| 70 | `t222_depassement_2_64` | `74f187d7f4608c01c6fd8ba7d33c01fe` | 1.075 | 1.115 | 0.964 | ✅ |
| 71 | `t223_decimal_geant` | `f314f21a4d00ec39c391b7a0e07a93b0` | 0.969 | 1.161 | 0.835 | ✅ |
| 72 | `t224_promotion_execution` | `7b0ecc9e225211f38864e4c899057873` | 1.530 | 1.500 | 1.020 | ❌ |
| 73 | `t225_accumulation_boucle` | `3c25947575f43a010e7ef5a247faf05e` | 2.338 | 1.453 | 1.609 | ❌ |
| 74 | `t226_puissance_big` | `8bd4b2ceb590d52184b118a620b4aea9` | 1.237 | 1.175 | 1.053 | ❌ |
| 75 | `t22_mul` | `298179b8ab46f68d4628093a83edff34` | 0.898 | 1.181 | 0.760 | ✅ |
| 76 | `t230_neg_grands_litteraux` | `35f79985b91fafcfdd5de35c066297b2` | 0.870 | 0.966 | 0.901 | ✅ |
| 77 | `t231_neg_unaires` | `00167592cc2b3594b652c157394a697d` | 1.063 | 1.067 | 0.996 | ✅ |
| 78 | `t232_neg_add_mixte` | `037fff4fd73b908a4697b2a872f8dfc6` | 1.549 | 1.312 | 1.181 | ❌ |
| 79 | `t233_neg_abs_minmax` | `3abe3e0c648179af41fbac75efb0baed` | 1.224 | 0.808 | 1.515 | ❌ |
| 80 | `t23_div` | `9c2308508bdf7a656cb999d95eb88f7c` | 0.642 | 0.786 | 0.817 | ✅ |
| 81 | `t240_rat_big_calcul` | `38e8445b231d7a523b7889bd533fb38d` | 0.827 | 0.830 | 0.997 | ✅ |
| 82 | `t241_rat_neg_calcul` | `7dd13c08ba193de364c75b1d8d2b9291` | 1.335 | 0.890 | 1.499 | ❌ |
| 83 | `t242_rat_litteraux_grands` | `4b0b9e11b528eb28d986e89c0eac731b` | 0.840 | 0.606 | 1.387 | ❌ |
| 84 | `t243_rat_somme_calcul` | `838e7d86eda84fc2deab46916578b163` | 1.268 | 0.980 | 1.294 | ❌ |
| 85 | `t24_mod` | `1390ba802cbe697a0b847bf1bfdb32a5` | 1.030 | 1.485 | 0.693 | ✅ |
| 86 | `t250_unicode_chaines` | `666147dace67764d7531f54fa2548916` | 0.949 | 0.928 | 1.023 | ❌ |
| 87 | `t251_unicode_sous` | `619fb93225e0d087aa5315e452122dd8` | 0.870 | 1.113 | 0.782 | ✅ |
| 88 | `t252_unicode_ordre_car` | `6285c30c784f631bdbf45ee194c7b032` | 0.873 | 1.092 | 0.800 | ✅ |
| 89 | `t253_unicode_comparaison` | `b376b334b6ac204f1a03fd408ea3b2a4` | 0.869 | 0.909 | 0.957 | ✅ |
| 90 | `t25_puissance` | `edd72a0fdace0b6b50de18ce2ae5ebcd` | 0.823 | 1.355 | 0.608 | ✅ |
| 91 | `t26_negation` | `c2fd4113c53c6328272700101409b03f` | 0.798 | 1.193 | 0.669 | ✅ |
| 92 | `t27_comparaisons` | `c73a751a777ecb9b99aa2ba73b8ca7a7` | 1.167 | 1.405 | 0.830 | ✅ |
| 93 | `t28_bool_ops` | `db4406c6e9c85a64219261be4acf705c` | 0.826 | 0.969 | 0.852 | ✅ |
| 94 | `t29_precedence` | `0f341bcaae759ebc55b3ec03b2a89cc9` | 0.885 | 1.002 | 0.883 | ✅ |
| 95 | `t30_abs_minmax` | `f37c1780a519ca588535f27963185a20` | 0.705 | 0.860 | 0.820 | ✅ |
| 96 | `t31_partie_entiere` | `c70f70f6860be1b42e7900e2ca8e3c83` | 0.573 | 0.907 | 0.632 | ✅ |
| 97 | `t32_big_2_63` | `ea6da0995286361ef754ae9d66e63faa` | 0.561 | 0.758 | 0.739 | ✅ |
| 98 | `t33_big_2_64` | `017cd27a67d47eb7ae9ab6148a9b5aad` | 0.693 | 0.801 | 0.864 | ✅ |
| 99 | `t34_big_decimal` | `8b2ff35a57828ce0d447a5b93c97effc` | 0.684 | 0.616 | 1.111 | ❌ |
| 100 | `t35_factorielle` | `70b70293196a8e3461c5b7ba7e5d4690` | 0.804 | 0.810 | 0.993 | ✅ |
| 101 | `t40_chaine_egale` | `d0255ada46f712bb2ce1268c1d9d130f` | 0.885 | 0.942 | 0.939 | ✅ |
| 102 | `t41_unicode_sous` | `0ee794507e85b3d97c459836bb8a2408` | 0.862 | 0.854 | 1.010 | ❌ |
| 103 | `t42_escape` | `5ffd26daad94c89ec48167a1a0f53a10` | 0.830 | 0.938 | 0.885 | ✅ |
| 104 | `t43_chaine_longue` | `63fc7b13d6f2ba791916b03a5b84a344` | 1.346 | 1.026 | 1.312 | ❌ |
| 105 | `t44_ordre` | `e8fd1ea6e3ce472364ca9a093d1bc1f7` | 0.842 | 0.909 | 0.926 | ✅ |
| 106 | `t45_sous_compare` | `17c8d0433bf3339ab49bd02af2ec59aa` | 0.903 | 0.650 | 1.389 | ❌ |
| 107 | `t46_chaine_entier` | `81e36544a17398c747cd5df481825d16` | 0.806 | 0.927 | 0.870 | ✅ |
| 108 | `t47_concat_diverse` | `0f8f787546eb04fd29502009c9f30b98` | 0.887 | 0.918 | 0.966 | ✅ |
| 109 | `t50_liste_vide` | `897316929176464ebc9ad085f31e7284` | 0.873 | 0.994 | 0.878 | ✅ |
| 110 | `t51_liste_idx` | `dc07c38a28631afbd18fb4c3a83d1f51` | 0.634 | 0.651 | 0.973 | ✅ |
| 111 | `t52_liste_croissance` | `8e4ca22a4bcde6e86e41978f2603408b` | 0.970 | 0.714 | 1.359 | ❌ |
| 112 | `t53_liste_alias` | `64ccc2acebb62671bae26801215ca1a8` | 0.783 | 0.881 | 0.888 | ✅ |
| 113 | `t54_liste_chaine` | `42e2f42480bcc2183dffa691299861a1` | 1.267 | 1.321 | 0.959 | ✅ |
| 114 | `t55_liste_imbriquee` | `d41d8cd98f00b204e9800998ecf8427e` | 1.122 | 1.238 | 0.906 | ✅ |
| 115 | `t60_table_bool` | `17c8d0433bf3339ab49bd02af2ec59aa` | 1.046 | 1.244 | 0.841 | ✅ |
| 116 | `t61_table_multi` | `ed1d0fa93292e8cb074c7358e460737b` | 1.071 | 0.831 | 1.288 | ❌ |
| 117 | `t63_table_chaines_cle` | `17c8d0433bf3339ab49bd02af2ec59aa` | 0.910 | 0.937 | 0.972 | ✅ |
| 118 | `t64_table_dans_fonction` | `17c8d0433bf3339ab49bd02af2ec59aa` | 1.139 | 1.160 | 0.982 | ✅ |
| 119 | `t70_pour` | `93e50d72ab294091a81d828459019eba` | 1.069 | 1.875 | 0.570 | ✅ |
| 120 | `t71_tant_que` | `84bc3da1b3e33a18e8d5e1bdd7a18d7a` | 0.941 | 1.071 | 0.878 | ✅ |
| 121 | `t72_si_imbrique` | `5f9e425f2dfdcb50cbcaec67d240ce1f` | 1.432 | 0.849 | 1.687 | ❌ |
| 122 | `t73_fact_rec` | `d7d4b9836c7a22c0d42c79a9edaa3500` | 1.199 | 1.527 | 0.785 | ✅ |
| 123 | `t74_fib_rec` | `2a9162c5b23f01510318db17a039da21` | 1.081 | 2.059 | 0.525 | ✅ |
| 124 | `t75_pgcd_rec` | `ea6277c6af3a990c405bfd33938f4418` | 1.291 | 1.332 | 0.970 | ✅ |
| 125 | `t76_rec_mutuelle` | `b039fea1170960b0c2c8185db7f9b8e7` | 1.533 | 1.581 | 0.970 | ✅ |
| 126 | `t77_pour_imbrique` | `cf4278314ef8e4b996e1b798d8eb92cf` | 1.447 | 1.341 | 1.079 | ❌ |
| 127 | `t78_retours_multiples` | `9589b0df4c660ecea0936cbb3a304af9` | 1.266 | 1.406 | 0.901 | ✅ |
| 128 | `t79_rec_profonde` | `9fdc03fef9ff73d752cddfe000401e74` | 4.324 | 1.841 | 2.349 | ❌ |
| 129 | `t7a_boucle_chaine` | `857f2261690a2305dba03062e778a73b` | 1.181 | 1.079 | 1.095 | ❌ |
| 130 | `t7b_si_dans_boucle` | `31d30eea8d0968d6458e0ad0027c9f80` | 1.408 | 1.720 | 0.818 | ✅ |
| 131 | `t80_nb_arguments` | `b026324c6904b2a9cb4b88d6d61c81d1` | 1.217 | 1.051 | 1.158 | ❌ |
| 132 | `t81_argument` | `9dfbff9b5e168944b245adad41e459d6` | 0.879 | 0.945 | 0.930 | ✅ |
| 133 | `t82_argument_hors` | `d41d8cd98f00b204e9800998ecf8427e` | 0.788 | 0.971 | 0.812 | ✅ |
| 134 | `t83_lire_texte` | `fdcc7f5855245c58ab3e26cdaeffc519` | 0.633 | 0.867 | 0.730 | ✅ |
| 135 | `t84_ecrire_texte` | `c26a617131afa3701e8c3ccf243dd3f1` | 0.841 | 0.873 | 0.963 | ✅ |
| 136 | `t85_ecrire_bytes` | `6d0b865b7d33c81b43fabaf044a35f76` | 0.790 | 0.939 | 0.842 | ✅ |
| 137 | `t86_ecrire_puis_lire` | `6d7fce9fee471194aa8b5b6e47267f03` | 1.006 | 1.054 | 0.954 | ✅ |
| 138 | `t90_graine` | `1cebe5f222e2019c6e840547a47b3222` | 0.881 | 0.901 | 0.978 | ✅ |
| 139 | `t91_prng` | `4d76f847f56a59d0a864183113a0093e` | 0.943 | 1.065 | 0.886 | ✅ |
| 140 | `t92_hasard_bornes` | `0b1f9e68de138bf0330234e73af91769` | 1.000 | 1.022 | 0.978 | ✅ |
| 141 | `t999_critique_g5` | `de94136cd3a3e6a2e8ee2f38be9cf7ce` | 14.789 | 1.604 | 9.219 | ❌ |

## 6. Validation

- **Parité 141/141** sur `rc` + stdout entre le moteur C0 compilé à HEAD et l'oracle canonique (`ROSE_SANS_JIT=1`) avec les fixtures du harnais, vérifiée à chaque tirage des 3 salves.
- **Parité stricte 139/141** (rc + stdout + stderr) : 2 BLOQUES uniquement, `t107_hof_var`, `t55_liste_imbriquee`, où les deux moteurs refusent (rc=1, stdout vide) mais avec un diagnostic stderr différent ; ce sont les BLOQUES historiques documentés dans `DIAG_141.md`, non corrigés dans ce worktree de benchmark (interdiction de modifier `moteur_complet.rose`).
- **Point fixe** : `rir compilateur/emetteur.rose` et `compilateur/emetteur.rose compilateur/emetteur.rose` rendent tous deux le md5 `b3ed9262dd54776a6dbd07bc12368d0e` (263 520 octets), rc=0 des deux côtés.
- **Aucun fichier interdit modifié** : seul `rose/docs/BENCH_GLOBAL.md` est ajouté au worktree ; `compilateur/moteur_complet.rose`, `compilateur/emetteur*.rose` et `rose/src/**` sont intacts.
- Données brutes : `/tmp/opencode/v6perf_bench/results.json` (A/B) et `/tmp/opencode/v6perf_bench/aa_rust.json` (A/A), 15 tirages par programme et par moteur en A/B.
