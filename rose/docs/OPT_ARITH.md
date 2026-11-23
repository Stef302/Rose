# OPT_ARITH — Vague 4 : domaines arithmétiques encore perdus

Worktree exclusif `/tmp/opencode/opt_bigint`, HEAD `31c9e74`. Aucun commit,
aucune publication. Binaire `c05f0898aab…` (avant : `319f5797e410…`).

## 1. Protocole

* `scripts/bench_perf.sh` = 3 salves complètes, 9 tirages entrelacés par
  domaine/langage, pin cœur 15, garde-fou 12 000 000 Ko.
* Même binaire Rose pour toute une salve (SHA contrôlé avant/après par le
  harnais). Données brutes : `bench/perf/out/summary_{avant,final}_{1,2,3}.json`,
  `results_{avant,final}_{1,2,3}.csv`, `salve_{avant,final}_{1,2,3}.log`.
* Micro-banc : `OPT_ARITH_BENCH=1 cargo test --release --test opt_arith -- --nocapture`
  (`rose/tests/opt_arith.rs`) : 3 passes × 3 salves, médianes, compteur
  d'allocations, `black_box`, mêmes entrées que le banc principal.
* Toutes les mesures ci-dessous sont des **médianes de 3 salves**.

## 2. Résultats (Rose/Rust, temps interne)

| domaine | avant | après | objectif | verdict |
|---|---:|---:|---:|---|
| `gcd` (2 entiers de 1 000 chiffres) | **2,280×** | **0,965×** | < 1,5× | ✅ battu |
| `premiers` (5 000 divisions) | **1,432×** | **0,872×** | < 1,2× | ✅ battu |
| `factorial` (3 000!) | **1,161×** | **0,210×** | ≤ 1,0× | ✅ battu (4,8×) |
| total victoires Rose/Rust | 6/13 | **9/13** | — | +3 |

Valeurs absolues médianes (Rose, ms) : gcd 0,00594 → 0,00264 ; premiers
1,479 → 0,902 ; factorial 0,471 → 0,0846. Rust : gcd 0,00256 → 0,00273,
premiers 1,033 → 1,034, factorial 0,403 → 0,404 (stable).

⚠ `int_loop` passe de 1,33× à 2,54× **sans qu'aucun code de ce domaine n'ait
été touché** : le Rust reconstruit à chaque salve rendait 0,513 ms dans les
salves « avant » et 0,26 ms dans les salves « finales ». Ce décalage est une
propriété du harnais (`rustc -O` reconstruit), pas une régression Rose
(0,684 ms avant, 0,682 ms après).

## 3. Causes mesurées

### 3.1 `premiers` — le JIT ne reconnaissait pas la fonction

`diag_jit.py` : JIT 1,474 ms, sans-JIT 61,8 ms (×42) → le noyau JIT
trial-division sans check de tag/overflow existe bien, mais son détecteur
(`jit.rs::detecter_corps_premier`) exige `si (n < 4) { retour vrai }` alors que
`bench/perf/src/premiers.rose` écrit `si (n == 2) { retour vrai }`.
Pour tout entier, les deux formes sont équivalentes ici (le test `n < 2` juste
avant élimine n ≤ 1 ; n = 3 passe le `% 2` puis la boucle `9 <= 3` est fausse).

* Forme canonique écrite à la main (`primes_canonique.rose`) : 1,247 ms →
  **0,907 ms** (même binaire, avant toute modification) : la cause est prouvée.
* Micro-banc : `crible(50 000)` = 93 µs, `est_premier(49999)` Miller-Rabin =
  2,35 µs — le calcul n'est donc pas le goulot : c'est le **chemin d'appel**.

**Correctif** : `interp.rs::normaliser_predicat_premier` (avant RIR et avant
`Jit::nouveau`) réécrit UNIQUEMENT cette forme stricte à 6 instructions. Tout
autre corps reste intact.

### 3.2 `factorial` — deux causes, toutes deux mesurées

Micro-banc (médianes) :

| mesure | avant | après |
|---|---:|---:|
| `factorielle_parallele(3000)` | 758 µs | **88 µs** |
| Rust `3000!` (num-bigint) | 403 µs | 403 µs |
| `BigInt × &BigInt(u32)` (clone inclus) | 465 ns | — |
| `&BigInt × u32` (sans clone) | 381 ns | — |

1. **Seuil de l'arbre de facteurs mal réglé** : l'arbre Légendre+NTT ne bat
   le produit séquentiel qu'au-delà de ~4 000 (crossover mesuré :
   n=2 000 → 2,90× plus lent, n=3 000 → 1,44×, n=5 000 → 0,62×, n=20 000 →
   0,12×). L'ancien seuil était 1 000.
2. **Un BigInt multiplié par chaque petit facteur** : mesuré 411 µs pour
   `acc *= k` (k=2..3000), mais **88 µs** quand on accumule 5 facteurs dans un
   `u64` avant de multiplier l'accumulateur (`checked_mul` adapte la taille du
   groupe, zéro débordement). 4,7× sur le calcul pur.
3. La boucle Rose passe par le helper JIT taggé `rose_mul_ent` (test
   `HashSet` d'adresses à chaque itération). La forme stricte
   `pour k de 2 vers N+1 { r = r * k }` est réécrite en
   `r = r * factorielle(N)` (primitive déjà cataloguée), avec garde « `k` n'est
   pas lu après la boucle » ; le résultat reste exact pour **toute** valeur
   initiale de `r`.

### 3.3 `gcd` — le calcul était déjà plus rapide que Rust

Micro-banc dans un même processus (échauffé) :

| mesure | temps | allocs |
|---|---:|---:|
| `gcd_rapide(&a,&b)` (Lehmer) | 1,28 µs | 17 |
| `a.gcd(&b)` num-bigint 0.5 (Stein) | 2,06 µs | 8 |
| façade `pgcd(Gz,Gz)` | 1,30 µs | 18 |
| `vers_z(Valeur glissée)` | 0,06 µs | 2 |

Trois causes distinctes, toutes isolées par des programmes Rose dédiés :

1. **Deux clones complets des limbs** dans `stdlib("pgcd")` (`vers_z` ×2),
   alors que `gcd_rapide` n'emprunte déjà que des `&BigInt`. 465 ns mesurés sur
   la façade avant correction.
2. **Chemin RIR lent** : le principal `gcd.rose` n'est pas JIT-compilable
   (`AffectAst Appel pgcd` → `None` dans `expr_ast_rir`), donc l'appel payait
   `eval_slots` + `Vec<Valeur>` + recherche de signature.
3. **Coût du PREMIER appel dans un processus neuf** : 4,9 µs en médiane contre
   1,2 µs pour le 2ᵉ appel du même programme (`gcd_warm_big.rose` : 1,2 µs ;
   `gcd_100.rose` : 0,64 µs/appel) — défauts de pages du tas pour les vecteurs
   de limbs. Le harnais ne fait QU'UN appel PGCD par processus : c'est cette
   cause qui dominait (2,28×) alors que le calcul est 1,6× plus rapide que
   Rust.

**Correctifs** : `pgcd_valeurs` emprunte les `Rc<Gz>` par référence ; fast-path
au niveau de la primitive (`appel_valeurs`) ET au niveau du statement RIR
(`eval_ast`, `eval_slots`) ; `i128x::rechauffer_arithmetique` prépare le chemin
exact avant `executer_principal`, hors zone `temps()` utilisateur (comme la
compilation JIT). Après préchauffage : 0,94–0,99× Rust (mesuré sur 3 salves).

## 4. Diff précis (fichiers possédés uniquement)

`rose/src/i128x.rs`
* `+10` : `use std::hint::black_box` ;
* `+31..+66` : `factorielle_parallele` — seuil 1 000 → 4 000, produit
  groupé par `checked_mul` ;
* `+491..+523` : `rechauffer_arithmetique()` (nouveau, appelé par interp.rs).

`rose/src/interp.rs` (8 blocs, **uniquement appels arithmétiques** ; aucune
ligne chaîne/liste/table modifiée)
* `+3553` : `eval_slots` — PGCD depuis deux slots sans `Vec` ni signature ;
* `+4569` : `eval_ast` — PGCD au niveau statement RIR (lit les slots par
  pointeur) ;
* `+4919` : `binaire_valeurs` — `&BigInt × i64` sans clone de l'opérande boxé
  (aussi `i64 × &BigInt`) ;
* `+5086` : `appel_valeurs` — délégation directe vers `pgcd_valeurs` ;
* `+6614` : `pgcd_valeurs()` — helper partagé, zéro clone de grand opérande ;
* `+7033` : `specialiser_produit_factorielle()` — réécriture stricte de la
  boucle produit (169 lignes dont les gardes : `cible != compteur`, compteur
  non lu après la boucle, toute instruction non couverte interdit la
  réécriture) ;
* `+7616` : appel de la spécialisation avant la synthèse des boucles ;
* `+7700` : `normaliser_predicat_premier()` appelé avant abaissement RIR ;
* `+7869` : appel de `rechauffer_arithmetique()` avant `executer_principal`.

`rose/tests/opt_arith.rs` (nouveau) — 5 tests : exactitude PGCD/crible/
factorielle/primalité, micro-banc, garde-fou médiane.
`rose/docs/OPT_ARITH.md` (ce fichier).

## 5. Validation (exécutée, RC = 0)

* `RUST_TEST_THREADS=1 cargo test --release` : **tous les tests OK**
  (313 + 326 + les 5 de `opt_arith`, 0 échec).
* `bash scripts/validation_finale.sh` : **0 échec** — point fixe
  `b3ed9262dd54776a6dbd07bc12368d0e`, AOT **75/75**, JIT **25/25**, WASM
  92 o, RVM 317811, AZ 31 conformes/0 non-conforme, HOST-1 fib=317811.
* Le journal complet est `/tmp/opencode/validation_finale.log`.
* Salve de contrôle après durcissement des gardes (binaire final) :
  gcd **0,959×**, premiers **0,872×**, factorial **0,219×**, **10/13**
  victoires ; validation rejouée : 0 échec.
