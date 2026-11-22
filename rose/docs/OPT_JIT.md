# OPT-JIT — accélérer et étendre le JIT Cranelift (constats datés, 22/11/2026)

Ce document ne cite aucune spec ni README : **chaque chiffre ci-dessous a été
mesuré** sur cette machine (ASUS GX10 / GB10, `aarch64`, 10 cœurs Cortex-X925
+ 10 Cortex-A725, Linux, `cargo 1.99.1`, profil `release`).

## 1. Protocole de mesure

* Micro-banc intégré : `rose/tests/opt_jit.rs`
  (`cargo test --release --test opt_jit -- --nocapture`). Sept charges
  (fib 21, factorielle 20, boucle 100 000, `2^3000`, concaténation 500,
  table 500, primes 3000). Chaque charge est lancée **3 fois** en mode JIT
  et 3 fois en mode interpréteur (`ROSE_SANS_JIT=1`) ; on relève les champs
  `exec` et `compiletout` du `TIMING` et on prend la **médiane**.
* A/B avant/après : 21 paires entrelacées A/B/B/A, **épinglées sur un cœur**
  (`taskset -c 2`), médianes des différences appariées. L'épinglage est
  indispensable : sans lui, la même charge mesure 145 µs sur X925 et 291 µs
  sur A725. Une salve non épinglée a d'ailleurs fait croire à tort qu'un
  changement de `opt_level` gagnait 2× (voir §5).
* Binaire « avant » : source HEAD reconstruite dans le même environnement ;
  binaire « après » : sources de ce chantier. Sorties comparées octet pour
  octet dans chaque paire.

### Médianes du micro-banc (3 runs, après optimisation, cœur épinglé)

| charge      | JIT exec  | interpréteur exec | accélération | JIT compile |
|-------------|----------:|------------------:|-------------:|------------:|
| fib(21)     |   54,8 µs |        8 300,7 µs |      151,4 × |   1 163,9 µs |
| fact(20)    |   33,4 µs |           53,5 µs |        1,6 × |     712,0 µs |
| boucle 1e5  |  295,5 µs |       11 594,7 µs |       39,2 × |     757,6 µs |
| 2^3000      |   85,9 µs |           85,9 µs |        1,0 × |       0,1 µs (pas de JIT) |
| chaîne 500  |   52,5 µs |          195,6 µs |        3,7 × |     857,4 µs |
| table 500   |   62,0 µs |          476,1 µs |        7,7 × |     834,8 µs |
| primes 3000 |  657,4 µs |       26 256,3 µs |       39,9 × |   1 227,7 µs |

`compiletout` est le temps entre `Jit::nouveau` et l'entrée dans
`executer_principal` : il inclut l'émission, `define_function` et les deux
`finalize_definitions`. Sur un programme à exécution unique, il domine ;
les gains ci-dessous portent donc sur `exec`, pas sur le temps total d'un
processus minuscule.

## 2. Goulots identifiés

1. **`*` entier passait systématiquement par le helper boxé `rose_mul_ent`**
   (6 arguments, deux emplacements de pile, test du tag, promotion GMP).
   Dans une boucle imbriquée, cet appel est payé à chaque itération : c'est
   le coût dominant du banc `primes` (`d * d <= n`).
2. **Le lexeur replie le compteur `i` en imaginaire.** `i * 2` devient
   `LitComplexe { re: 0, im: 2 }` ; `front::deplier_i_rec` ne répare que
   `0+1i`. La porte du JIT (`valeur_nue_ast`) refuse cette valeur, donc
   `inserer(t, i, i*2)` restait **entièrement interprété**, alors que le même
   programme avec un compteur `k` passait par Cranelift (37 µs contre 260 µs
   mesurés). C'est un manque de couverture, pas une lenteur du code généré.
3. Conversions handle ↔ `Valeur`, appels aux 8 helpers natifs pour valeurs
   boxées, et absence d'inlining inter-fonctions : présents, mais **non
   dominants** sur ces bancs (voir §5 : aucune amélioration mesurable en
   jouant sur les drapeaux Cranelift).

## 3. Optimisations livrées

### A. Produit entier en code direct avec test de débordement exact

`rose/src/jit.rs`, branche `Oper::Mul` de `expr_eabstrait_rir`.

* Quand les deux opérandes portent le drapeau « entier nu » (`unboxed_tmp`),
  on émet `imul` + `smulhi` + `sshr(p, 63)` : le produit 128 bits tient dans
  un i64 exactement si les 64 bits haut valent l'extension de signe du bas.
  Aucun cas limite (y compris `i64::MIN × −1`) n'est laissé au hasard.
* Sinon (opérande boxé OU débordement), on bascule sur le MÊME helper
  `rose_mul_ent` qu'avant : la promotion GMP reste exacte.
* Vérifications complémentaires : `-3e9 × 3e9`, `5e9 × 2e9` (promotion
  bigint), `i64::MAX−1 × 2`, `3037000500²`, `3 × 7` — toutes identiques à
  l'interpréteur.

**Mesuré (21 paires, épinglé)** : `primes` 1 240,2 → 614,8 µs (**2,02 ×**),
compilation 1 110 → 1 178 µs (+6 %, plus de code émis). `boucle`, `fib`,
`chaine`, `factorielle` : 1,00 × (aucune régression).

### B. Couverture : réparation du compteur `i` dans l'abaissement RIR

`rose/src/rir.rs`, `reparer_compteur_i_expr` / `reparer_compteur_i_op`.

* La réécriture est **limitée au corps d'un `Instr::Pour { nom: "i" }`** :
  `LitComplexe { re: 0, im: k }` (k entier exact) devient `Var("i") * k`,
  ou `Var("i")` si k = 1. Un imaginaire légitime hors boucle `i` n'est jamais
  touché ; aucun autre nom de variable n'est codé en dur.
* Elle est appliquée aux AST déjà portés par les opérations RIR, **sans
  cloner l'AST du programme** (un clone complet avait été écarté).
* Le principal redevient `principal=cranelift voie=rir` sur ce programme,
  et `valeur_nue_ast` reconnaît `i*2` comme `ValeurNu::Produit`.

**Mesuré (21 paires, épinglé)** : table 500 insertions, `exec`
443,2 → 53,6 µs (**8,28 ×**), compilation 0,2 → 799,1 µs (le JIT est désormais
activé). À 5 000 insertions : 20 216,5 → 768,7 µs (**26,3 ×**), soit un total
compile+exec de 20,2 ms → 1,65 ms (12,2 ×). Le seuil de rentabilité d'une
exécution unique est d'environ 1 000 insertions.

## 4. Validation

* `cargo test --release` (cible partagée) : **311 + 324 + 6 + 9 + 8 + 2 + 1 + 2
  tests, 0 échec**.
* `bash scripts/validation_finale.sh` : **0 échec** (avec `RUST_TEST_THREADS=1`
  pour éviter un `ExecutableFileBusy` intermittent, sur cette machine partagée,
  dans le test préexistant `link_natif::tests::elf_avec_valeur_exacte_ecrit_le_texte_memorise`
  — sans rapport avec le JIT) — point fixe émetteur
  (`b3ed9262…`), AOT 75/75, JIT 25/25 (`principal=`), WASM 92 o, RVM 317811,
  AZ 31/31, HOST-1 fib=317811.
* Parité JIT ↔ interpréteur : **24/24** programmes de `tests_aot8/programmes`
  octet pour octet ; `20_hasard.rose` exclu parce que sa sortie est aléatoire
  par conception. Test automatisé dans `rose/tests/opt_jit.rs`.
* Couverture maintenue : **25/25** avec `principal=` (contrôle de
  `validation_finale.sh`) ; **18/25** avec `principal=cranelift`, nombre
  identique à celui mesuré avant le chantier (les 7 autres n'ont aucun
  travail JIT et tournent en RIR interprété). Le programme `table` du banc
  est passé de l'interpréteur au JIT sans perdre un seul `principal=`.

## 5. Tentatives écartées (avec cause mesurée)

* **`opt_level=speed_and_size` global** : salves non épinglées laissaient
  croire à 2× sur `boucle`, puis le résultat s'inversait d'une salve à
  l'autre. La cause est l'ordonnanceur big.LITTLE (X925 vs A725), pas le
  réglage. Épinglé, 15 échantillons : `boucle` 290,6 vs 288,6 µs, `primes`
  1 241 vs 1 239 µs — **aucune différence**. Code de diagnostic retiré.
* **`enable_alias_analysis=false` / `unwind_info=false`** (`ROSE_JIT_FLAGS`) :
  15 paires épinglées, écarts ≤ 50 µs sur la compilation et ≤ 5 µs sur
  l'exécution, signes contradictoires — **aucun gain**. Retiré.
* **Renseigner les paramètres numériques dans `natures_du_corps_sig`** pour
  compiler `n * f(n−1)` : la porte AST accepte alors la fonction, mais le
  comptage retombe à 18/25 `principal=cranelift`, deux programmes rationnels
  finissent par un `panic` de destructeur TLS, et `f(21)` reste faux.
  **Reverté** ; la note historique (« fib/pgcd paniquent ») reste vraie.
* **Mise en cache de `signatures_inferées`** entre `prepare_jit` et
  `Jit::nouveau` : coût réel mesuré 0,4–1,5 µs — inférieur au bruit, sans
  intérêt.

## 6. Limites connues (hors périmètre des fichiers possédés)

* `f(21)` (factorielle récursive appelée depuis un principal JIT) rend `0`
  au lieu de `51090942171709440000`. Cause : `f` n'est pas éligible, elle est
  déléguée ; `crate::interp::appeler_delegue` (interp.rs, **non possédé**)
  convertit la `Valeur` en i64 et mappe `Valeur::Entier(_, Some(_))` sur `0`.
  Correctif minimal : exposer dans `interp.rs` une variante
  `appeler_delegue_valeur(...) -> Valeur`, puis faire boxer le résultat par
  un helper `jit.rs` et tester son tag au point d'appel. Aucun test de la
  batterie 25 ne couvre ce cas ; la parité 24/24 reste vraie.
* Les boucles JIT dont le résultat grandit au-delà de l'i64
  (`a = a * a` sur plusieurs itérations) produisent des valeurs erronées
  **déjà sur HEAD** (avant comme après ce chantier). Cause probable : le tag
  de boxage d'un slot n'est pas suivi d'une itération à l'autre dans la voie
  RIR (`slot_unboxed` n'est calculé que statiquement). Correctif minimal :
  propager le drapeau « nu » de sortie vers le slot dans l'émetteur RIR.
* Aucune optimisation cross-fonctions / inlining Cranelift n'a été activée :
  les appels JIT→JIT utilisent déjà `colocated: true`, et les fonctions
  restées interprétées passent par `rose_appel_interpreteur` (ABI 4 arguments
  + nom), dont le coût n'est pas dominant sur ces bancs.
