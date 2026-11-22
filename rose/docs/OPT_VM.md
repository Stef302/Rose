# OPT_VM — accélération des backends alternatifs (RVM / transpilation)

**Constat daté : 22/11/2026. Worktree exclusif `/tmp/opencode/opt_vm`, aucun commit,
aucune publication.** `CARGO_TARGET_DIR=/tmp/opencode/opt_shared_target`,
toutes les compilations longues sous `scripts/garde_fou_mem.sh 12000000`.

## 1. Ce qui a été optimisé (deux chemins)

### A. Exécution RVM : le pipeline JIT du snapshot (≥ 200× sur fib)

`rosevm::executer_snapshot` reconstruisait le programme depuis le snapshot,
abaissait les fonctions en RIR, puis appelait **uniquement**
`Interp::nouveau(prog).executer_principal()`. Or `Interp::nouveau` ne prépare
aucun JIT : toute la RVM tournait en interpréteur, y compris `fib(28)`
(~1,6 million d'appels). Le chemin `rose run` (`interp::executer_source_analyse`)
compile le principal et les fonctions avec Cranelift.

La RVM applique maintenant le **même pipeline** : `jit::prepare_jit` →
`Jit::nouveau` → `compiler_principal` → `compiler_tout`, le principal compilé
étant appelé directement avec `poser_interp_courant(&mut interp)` (le code
compilé peut déléguer à l'interpréteur). Deux garde-fous :

* la voie JIT n'est prise **que si toutes les fonctions ont été réellement
  compilées** (`Compile::Natif` / `Compile::Principal`), sinon le code compilé
  déléguerait à un interpréteur sans table JIT installée ;
* compiler coûte ~0,5 ms : on ne déclenche le JIT que si une fonction est
  **récursive** (fib, factorielle récursive…). Les programmes sans récursion
  (boucles courtes, tables) gardent exactement l'ancien chemin RIR, sans
  régression (mesuré : `boucles` 41 µs avant / 40 µs après ; `table` 11,6 µs /
  10,6 µs).

Le cas irrationnel (`pi`/`phi`/`e`) reste en AST pur, comme avant.
Le pointeur d'interpréteur est remis à nul après l'exécution, y compris en cas
de panique.

### B. Émission des modules : zéro copie d'AST + déduplication linéaire

* `transpile::WasmEmetteur::module` clonait **tout l'AST** (principal + chaque
  fonction) avant d'émettre un octet. Les tranches sont maintenant empruntées
  à `prog: &'a Programme` ; les corps de fonctions passent par `&'a [Instr]`.
  Aucun sémantique changée, modules bit-à-bit identiques (taille inchangée).
* `rosevm::ecrire_snapshot` reconstruisait la liste des noms avec
  `Vec::contains` en O(n²) ; une table `HashSet` sérielle les doublons en O(n).
  Le cas normal (`ordre.len() == signatures.len()`) garde un simple
  `clone()` : le HashSet n'est alloué que si les deux vues divergent
  (un HashSet coûte ~100 ns sur un programme d'une fonction).

## 2. Méthode de mesure

Harnais `rose/tests/opt_vm.rs`, **dans le processus**, avec
`std::hint::black_box`. Quatre scénarios : `fib` (fib(28)), `boucles`
(`somme(2000)`), `table` (3 insertions + indexation), `grand` (120 functions,
~1 400 instructions, 27 804 octets de snapshot).

* émission (`bc_emission`, `transpile_*`) : lots de 200 appels (100 pour les
  transpileurs), 5 échantillons, médiane divisée par le lot ;
* lecture du snapshot : lots de 200 ;
* exécution RVM : 3 appels réels après un échauffement, médiane ;
* **appariement A/B/A** : référence (`git checkout` des deux sources), 3 runs, puis
  sources optimisées restaurées, 3 runs, même machine et même harnais. Les
  commandes sont passées sous le garde-fou mémoire ; `cargo test --release
  --test opt_vm --no-run` a été relancé à chaque bascule.

## 3. Résultats — médianes de 3 runs (nanosecondes)

| scénario / métrique | avant | après | gain | taille avant → après |
|---|---:|---:|---:|---|
| **fib / rvm_execution** | **124 341 937** | **577 072** | **215,5×** | 7 → 7 o de sortie |
| **grand / transpile_wasm** | **219 197** | **103 585** | **2,12×** | 5 388 → 5 388 o |
| grand / bc_emission | 23 048 | 15 266 | 1,51× | 27 804 → 27 804 o |
| boucles / transpile_wasm | 1 861 | 1 288 | 1,44× | 107 → 107 o |
| table / transpile_wasm | 984 | 117 | (erreur précoce, non significatif) | refus identique |
| boucles / rvm_execution | 40 992 | 39 568 | 1,04× | 8 → 8 o |
| table / rvm_execution | 11 600 | 10 608 | 1,09× | 2 → 2 o |
| grand / rvm_execution | 343 760 | 339 376 | 1,01× | 2 → 2 o |
| boucles / bc_emission | 178 | 175 | 1,02× | 301 → 301 o |
| fib / transpile_js | 3 110 | 1 782 | (bruit : même code) | 9 464 → 9 464 o |
| grand / transpile_js | 190 802 | 193 565 | 0,99× | 28 436 → 28 436 o |
| grand / transpile_py | 183 069 | 185 575 | 0,99× | 22 676 → 22 676 o |
| grand / transpile_c | 249 570 | 252 339 | 0,99× | 43 650 → 43 650 o |
| grand / bc_lecture | 173 798 | 177 660 | 0,98× | — |

Objectif ≥ 2× atteint sur **deux** chemins : exécution RVM (215×) et émission
WASM (2,12×). Les chemins non modifiés (JS/Python/C, lecture de snapshot,
exécution non récursive) restent à ±2 % près, aucun modules de sortie n'a
changé de taille.

Variabilité constatée sur `fib/rvm_execution` après optimisation : 0,577 ms à
1,50 ms selon l'état du cache JIT et la charge de la machine (plusieurs agents
compilent sur le même nœud) ; même la valeur la plus lente reste **83×** plus
rapide que la référence de 124 ms. Les micro-mesures (< 2 µs) peuvent varier
de ±40 % d'une salve à l'autre : c'est pourquoi les comparaisons ci-dessus
sont appariées et que seul l'écart ≥ 2× est revendiqué comme gain.

Taille des modules : snapshot fib 292 o, WASM fib 92 o ; WASM `grand`
5 388 o ; JS/Python/C inchangés (9 464 / 8 724 / 9 992 o pour fib).

## 4. Vérifications réelles

* `cargo test --release` (shared target) : **rc=0**, 311 + 324 + 6 + 9 + 8 + 2
  + 1 + 2 tests OK (dont les tests de parité snapshot ↔ interpréteur de
  `rosevm.rs` et les 2 tests de `tests/opt_vm.rs`). Un premier passage a
  échoué sur `link_natif::tests::elf_aarch64_retourne_49_en_reel` avec
  `ExecutableFileBusy` (un autre agent exécutait le même fichier ELF temporaire)
  ; le passage suivant a réussi — aucune modification de `link_natif.rs`.
* `bash scripts/validation_finale.sh` : **rc=0, `=== RESULTAT: 0 échec(s) ===`**
  (point fixe émetteur `b3ed9262…`, AOT 75/75, JIT 25/25, WASM 92 o,
  RVM 317811, AZ 31 CONFORME, HOST-1 fib=317811).
  Deux corrections d'environnement ont été nécessaires, **aucune dans le code
  du moteur** : le fixture `/tmp/opencode/fib28.rose` utilisait une syntaxe
  invalide (`si n < 2` sans parenthèses) et a été réécrit en `si (n < 2)` ;
  le binaire attendu par la batterie AZ (`rose/rose`, ignoré par git) a été
  copié depuis `rose/target/release/rose`.
* `scripts/validation_finale.sh` a été rendu robuste au passage de la batterie
  AZ de 31 à 47 tests (vu pendant la session) : le contrôle exige désormais
  **au moins 31 CONFORME et zéro non-conforme**, au lieu d'un cardinal figé à
  31. Aucune autre ligne du script n'a changé.
* Sorties : `rose bc examples/fibonacci.rose` puis `rose rvm` → `317811` ;
  module WASM d'un programme à fonctions + boucles → `rose_main()` = **42**
  vérifié par Node 26 (`WebAssembly.instantiate(...).instance.exports.rose_main()`).

## 5. Fichiers touchés

* `rose/src/rosevm.rs` — pipeline JIT de `executer_snapshot`, détection de
  récursion, `ecrire_snapshot` sans O(n²).
* `rose/src/transpile.rs` — `WasmEmetteur::module` sans copie d'AST.
* `rose/tests/opt_vm.rs` (nouveau) — harnais de mesure + tests de sortie.
* `rose/docs/OPT_VM.md` (nouveau) — ce document.
* `scripts/validation_finale.sh` — une ligne, contrôle AZ non figé (outillage
  partagé, documenté ci-dessus).
