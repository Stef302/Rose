# SELFHOST_EXTENSION — extension du self-hosting et inventaire exact (vague OPT-VM, 23/11/2026)

Worktree exclusif `/tmp/opencode/opt_vm`, HEAD `8ef12e66c65f0945666d1d1b932b930d864f826c`.
**Aucun commit, aucune publication, aucun fichier `rose/src/**` ou `thorn/**` modifié.**
Fichiers possédés : `scripts/generer_rose_natif.py`, `compilateur/rose_natif.rose`,
`compilateur/rose_natif_pe.rose`, `scripts/build-sans-rust.sh`, ce document.

## 1. Ce que « self-hosté » veut dire ici (précision demandée)

Le binaire distribué reste produit par `cargo build` : **72 606 lignes** de
`rose/src/*.rs` (`wc -l rose/src/*.rs | tail -1`). Ce que le self-hosting a
déjà remplacé, ce n’est **pas** ce binaire, mais **une chaîne de compilation** :

```
source Rose  --[front-end compilateur/emetteur.rose]-->  RIR
RIR          --[backend compilateur/emetteur_elf.rose]->  ELF64 aarch64 autonome
```

Les deux sources Rose sont composées par `scripts/generer_rose_natif.py` en
`compilateur/rose_natif.rose` (ELF) et `compilateur/rose_natif_pe.rose` (PE).
Le point fixe `C0 = C1 = C2` (md5 `9fbd21d2b06efe7f1e86ac5b5730d2c8`) prouve
que **ce compilateur-là** se recompile lui-même, bit pour bit. Il ne prouve
rien sur le moteur Rust (JIT, AOT, WASM, RVM, CLI, GUI), qui reste nécessaire
pour produire et pour exécuter le binaire `rose`.

## 2. Inventaire mesuré : Rust vs Rose auto-hébergé

Légende : **OK** = chemin Rose autonome mesuré équivalent ; **PARTIEL** = le
chemin existe mais ne couvre pas tout le périmètre Rust ; **ABSENT** = aucune
routine Rose correspondante dans le composite.

| Capacité | Chemin Rust (`rose/src/…`, binaire 5,3 Mio) | Chemin Rose auto-hébergé | État |
|---|---|---|---|
| Analyse syntaxique + RIR | `main.rs` → `front.rs`, `rir.rs` (`rose rir`, `rose ast`, `rose tokens`) | `compilateur/emetteur.rose` embarqué dans `rose_natif.rose` (`em_source_rir()`) | **OK** (point fixe `b3ed9262` : `rose rir compilateur/emetteur.rose` → `b3ed9262dd54776a6dbd07bc12368d0e`) |
| Vérification statique | `main.rs` → `verif.rs` (`rose verif`) | `compilateur/verifieur.rose` existe (79 `fn`) mais **n’est pas câblé** dans le composite | **ABSENT** dans C0 |
| Compilation source → ELF aarch64 autonome | `comp.rs` / `natif_emetteur.rs` (`rose comp`) | `emetteur.rose` + `emetteur_elf.rose` → `rose_natif.rose` → C0 | **OK** (corpus 166 sources = 141 + 25, voir §5) |
| Compilation source → PE32+ aarch64 | `link_natif.rs`, `main.rs` | `rose_natif_pe.rose` (backend `emetteur_pe.rose`) | **PARTIEL** : PE valide mesuré (`file` = PE32+ AArch64, 3 sections) mais **émis par la graine Rust**, pas par C0 (C0 n’émet que de l’ELF) |
| Exécution JIT à chaud (Cranelift) | `jit.rs`, `run.rs`, `interp.rs` (`rose prog.rose`, `rose jit`) | aucun : le runtime G5 n’a ni `mmap` ni `mprotect`, seulement `openat/read/write/close/exit/fstat/getrandom` | **ABSENT** |
| AOT natif rapide (runtime `.S` embarqué) | `natif_emetteur.rs`, `aot_llvm_orc.rs`, `rose/runtime/{aarch64,x86_64}.bin` (`rose aot`) | chemin distinct : `emetteur_elf.rose` émet son propre runtime à `0x60000000+` ; les blobs Rust utilisent des absolus `0x402000`/`0x403000` | **ABSENT** |
| Interpréteur / REPL / console | `interp.rs`, `repl.rs` (`rose run`, `rose console`, `rose interprete`) | aucun : le composite est un compilateur mono-shot `source → stdout` | **ABSENT** |
| Bytecode + machine virtuelle RVM | `rosevm.rs` (`rose bc`, `rose rvm`) | aucun | **ABSENT** |
| Transpilation JS / Python / C / WASM | `transpile.rs`, `wasm.rs` (`rose transpile --cible …`) | sources Rose `emetteur_cpp.rose`, `emetteur_rust.rose`, `emetteur_x64.rose` présentes mais **non composées** dans C0 | **PARTIEL** |
| Mathématiques exactes (entiers illimités, rationnels, Unicode) | `i128x.rs`, `interp.rs`, `jit.rs` | front-end + backend G5 : compilation OK partout ; **exécution** conforme sur la majorité du corpus, divergences documentées §5 | **PARTIEL** |
| Auto-compilation du compilateur | `cargo build` (outil externe) | C0 → C1 → C2, md5 identique `9fbd21d2…` (avant extension), rejoué §6 | **OK** |
| CLI complète (`latex`, `tokens`, `ast`, `info`, `update`, `assoc`, `debug-lenteur`, `link`…) | `main.rs`, `cmd_llvm.rs`, … (`rose --help`) | aucune de ces commandes dans le composite | **ABSENT** |

Commandes de mesure : `rose --help`, `rose info`, `wc -l rose/src/*.rs | tail -1`,
`grep -c '^fn ' compilateur/*.rose`, `./rose/rose rir compilateur/emetteur.rose | md5sum`,
`./rose/rose verif compilateur/rose_natif{,_pe}.rose`, `file` sur un PE émis.

## 3. Extension réalisée : trois capacités absentes du front-end généré

Aucune source interdite n’a été modifiée : les correctifs sont injectés par
`scripts/generer_rose_natif.py` **dans la copie générée uniquement**, exactement
comme le mécanisme HOST-2 existant (`eval_expr`, `octet_brut`).

| # | Capacité ajoutée | Injection générateur | Cause technique de l’absence | Programme témoin |
|---|---|---|---|---|
| 1 | `t["cle"] + "-" + t["autre"]` (table + concaténation) | `injecte_front_valeur()` : `valeur` (type de lecture de table) routé comme le Dyn `?` de `front.rs:2044` dans l’opérateur `+` | `check_expr` de `emetteur.rose` type la lecture de table `valeur|vide`, mais l’opérateur `+` n’acceptait que `?` : `Addition valeur + chaine impossible`. Le composite retombait alors dans sa queue « vieux compilateur » (inatteignable par construction) et émettait un pseudo-ELF de 55 o (« rose: prog produit… ») au lieu d’un binaire. | `rose/tests_aot8/programmes/24_compose.rose` |
| 2 | primitive native `type(x)` | `STD_SIG_EXTENSION` : signature `chaine|tout` | Le backend G5 émet déjà `$g5_typel` (`emetteur_elf.rose` ≈ l.2041) mais le `std_sig` Rose n’exposait que l’alias `type_de`. | `tests_selfhost/t14_type.rose` |
| 3 | primitive native `prng(n)` | `STD_SIG_EXTENSION` : signature `entier|entier` | Le backend G5 émet déjà `$g5_hasardl` pour `hasard` **ou** `prng`, mais le `std_sig` Rose n’exposait que `hasard`. | `tests_selfhost/t91_prng.rose` |

Ces trois programmes échouaient à la compilation avec **C0 = 9fbd21d2…**
(message exact dans le TSV avant). Après regeneration et re-compilation de C0
par lui-même (§6), ils compilent **et s’exécutent en donnant exactement la
sortie de référence**.

## 4. Régénération et validation des composites

```
export CARGO_TARGET_DIR=/tmp/opencode/opt_shared_target
python3 scripts/generer_rose_natif.py
# généré rose_natif.rose (442 568 o) / rose_natif_pe.rose (213 372 o)
./rose/rose verif compilateur/rose_natif.rose      -> vérifié, rc=0
./rose/rose verif compilateur/rose_natif_pe.rose   -> vérifié, rc=0
./rose/rose rir compilateur/emetteur.rose | md5sum -> b3ed9262dd54776a6dbd07bc12368d0e (inchangé)
```

`b3ed9262` porte sur **emetteur.rose**, qui n’a pas été touché : l’identité du
front-end d’origine est préservée, les correctifs vivent dans le composite.

A/B du générateur (preuve que l’écart vient bien des injections) :
`RNAT_EXT_VALEUR=0 RNAT_OUT_ELF=/tmp/opencode/rn_noext.rose python3 scripts/generer_rose_natif.py elf`
→ 442 450 o, md5 `32fd82ab6838a5acb0d3cc05657272cd`, `rose verif` = `vérifié` ;
le `diff` contre le composite livré (442 568 o, md5 `c52a35b26facb13e20493f900819d839`)
ne montre que la ligne de l’opérateur `+` (et son commentaire).

Témoins compilés par le **composite final interprété par la graine** (avant même
C0′, pour isoler le correctif) :

```
./rose/rose compilateur/rose_natif.rose tests_selfhost/t14_type.rose       -> ELF ; entier|chaine|liste|booleen (identique référence)
./rose/rose compilateur/rose_natif.rose tests_selfhost/t91_prng.rose       -> ELF ; 0|9 (identique référence)
./rose/rose compilateur/rose_natif.rose rose/tests_aot8/programmes/24_compose.rose -> ELF ; rose-aot|2 (identique référence)
```

Le composite PE régénéré bénéficie des mêmes injections :
`./rose/rose compilateur/rose_natif_pe.rose rose/tests_aot8/programmes/24_compose.rose`
→ 4 608 o, `file` = `PE32+ executable (console) Aarch64, for MS Windows, 3 sections`.

Batterie AZ relancée sur cet état : `AZ_OUT=/tmp/opencode/ext_az bash
tests_selfhost/lancer_az.sh` → **`CONFORME 31`, 0 divergent, 0 bloqué** (rc=0).

## 5. Corpus compilé sans Rust — mesures avant/après

Méthode : `ext_harness.py` (hors dépôt) appelle uniquement `C0 source.rose >
out.elf`, vérifie `magic ELF` + machine `183` (AArch64), exécute l’ELF, puis
compare à la sortie de l’interpréteur Rust **uniquement comme oracle de test**
(jamais dans la chaîne de compilation). 166 sources : 141 `tests_selfhost/` +
25 `rose/tests_aot8/programmes/`.

| Compilateur | Conformes (oracle strict) | Artefacts ELF exécutables (`build-sans-rust.sh --verifier`) |
|---|---|---|
| C0 initial (md5 `9fbd21d2…`) | **92 / 166** (74/141 selfhost, 18/25 aot8) | **152 / 166** (11 `INVALIDE`, 3 `EXEC_ECHEC`) |
| C0 étendu (md5 `7bb09120…`) | **95 / 166** | **155 / 166** (8 `INVALIDE`, 3 `EXEC_ECHEC`) |

`ext_compare.py /tmp/opencode/ext_before.tsv /tmp/opencode/ext_after.tsv` :
`GAINS 3 ['tests_selfhost:t14_type', 'tests_selfhost:t91_prng', 'programmes:24_compose']`,
`PERTES 0 []`, `reste non conforme: 71` (35 bigint/rationnel, 28 autres
divergences d’exécution, 8 échecs de compilation).

C0 étendu = **1 603 744 o**, magic `7f454c4602010100`, produit par
`C0_initial compilateur/rose_natif.rose` en 436 s avec le garde-fou 12 000 000 Ko.
Aucun appel à `cargo`, `rose/rose` ou à un compilateur externe dans cette étape.

`scripts/build-sans-rust.sh` a été rendu réellement utilisable : il exécute C0
avec CWD = dossier de sortie (le repli « vieux compilateur » ne pollue plus le
dépôt avec un fichier `prog`), détecte le non-ELF par le magic, rapporte la
cause exacte **telle que C0 l’écrit** (`rose: prog produit (binaire ELF autonome, …)`
pour un rejet du front-end, `compilation rc=2` pour un avortement G5), écrit un
TSV optionnel (`--rapport`) et peut exécuter les ELF produits (`--verifier`).
Le diagnostic front-end détaillé (`Addition valeur + chaine impossible`, etc.)
s’obtient à part, **hors chaîne de compilation**, avec
`./rose/rose compilateur/emetteur.rose <source>`. La comparaison à un binaire
Rust n’est **jamais** activée par défaut (`--reference` est explicite).

Deux critères distincts, à ne pas confondre :

- `--verifier` du script = **ELF valide + exécution rc=0** : **152/166** avant
  extension (11 `INVALIDE`, 3 `EXEC_ECHEC`) ;
- oracle strict du harness = **ELF + rc=0 + sortie identique à la référence** :
  **92/166** avant extension (63 divergences d’exécution, 4 références qui
  paniquent, 1 sortie aléatoire).

Détail des échecs restants et causes exactes : voir §8.

## 6. Chaîne C0′ → C1′ → C2′ (self-hosting préservé après extension)

Le composite ayant changé, l’ancien point fixe `9fbd21d2…` décrit le compilateur
*d’avant* extension. La propriété à re démontrer est que le **nouveau**
compilateur se recompile lui-même. Chaîne jouée avec le garde-fou 12 000 000 Ko :

```
C0' = C0_initial(compilateur/rose_natif.rose)    # 06:59:43 -> 07:06:59 UTC, 436 s
C1' = C0'(compilateur/rose_natif.rose)           # 07:08:17 -> 07:16:39 UTC, 502 s
C2' = C1'(compilateur/rose_natif.rose)           # 07:16:39 -> 07:24:50 UTC, 491 s
cmp C1' C2'                                       # rc=0
```

Résultats : `C0'`, `C1'` et `C2'` ont **tous les trois** la même taille
**1 603 744 o**, le même magic `7f454c4602010100` et le même md5
`7bb091200cf927022eb9d86a548d2502`. `cmp C1' C2'` → rc=0, journal
`/tmp/opencode/chain_ext.log` : `POINT FIXE ETENDU OK`. Le point fixe est
donc atteint dès la première itération et confirmé à la seconde.

**Conséquence précise** : `9fbd21d2…` reste le point fixe du compilateur
*d’avant* ce chantier (reproductible depuis le `HEAD 8ef12e6` du composite,
avant les injections) ; `7bb09120…` est le
point fixe du compilateur *étendu*. Le self-hosting n’a pas été perdu, il a
changé de valeur — ce qui est attendu dès qu’on modifie la source compilée.

## 9. Validation finale

```
export CARGO_TARGET_DIR=/tmp/opencode/opt_shared_target RUST_TEST_THREADS=1
bash scripts/garde_fou_mem.sh 12000000 bash scripts/validation_finale.sh
# -> /tmp/opencode/ext_validation2_result.txt : === RESULTAT: 0 échec(s) === (rc=0)
```

Contenus mesurés : tests Rust `rc=0` (314 + 327 passés), point fixe émetteur
`b3ed9262dd54776a6dbd07bc12368d0e` inchangé (`OK point fixe`), AOT 75/75,
JIT Cranelift 25/25, WASM 92 o, RVM `317811`, AZ **31 CONFORME / 0 non-conforme**,
HOST-1 `fib=317811`.

Le premier passage, **sans** `RUST_TEST_THREADS=1`, a donné `1 échec` :
`cargo test --release` s’est terminé par `signal: 11, SIGSEGV` dans
`rose/tests/opt_jit_listes.rs`. Ce n’est pas une régression de ce chantier :
le fichier de test documente lui-même (l.15 et l.30) que l’arène JIT est
thread-local et que la suite doit tourner avec `RUST_TEST_THREADS=1`
(« SIGSEGV observé sans ce verrou »). Le passage avec cette variable est
`0 échec` (journal `/tmp/opencode/validation_finale.log`, dernière ligne
`=== RESULTAT: 0 échec(s) ===` à 07:18:03 UTC, rc=0, copie figée dans
`/tmp/opencode/ext_validation2_result.txt` (le log global `validation_finale.log`
est partagé entre agents et peut être réécris entre deux lectures)). Aucun fichier de test ni
`rose/src/**` n’a été modifié.

## 10. Rejouer le chantier

```bash
cd /tmp/opencode/opt_vm
export CARGO_TARGET_DIR=/tmp/opencode/opt_shared_target
python3 scripts/generer_rose_natif.py
./rose/rose verif compilateur/rose_natif.rose
./rose/rose verif compilateur/rose_natif_pe.rose
./rose/rose rir compilateur/emetteur.rose | md5sum          # b3ed9262…

# C0' depuis C0 initial (ELF md5 9fbd21d2… présent dans /tmp/opencode)
bash scripts/garde_fou_mem.sh 12000000 timeout 1200 \
  /tmp/opencode/C0_opt.elf compilateur/rose_natif.rose > /tmp/opencode/C0_ext.elf
chmod +x /tmp/opencode/C0_ext.elf

# corpus Sans Rust, avec causes exactes
bash scripts/garde_fou_mem.sh 12000000 bash scripts/build-sans-rust.sh \
  --compilateur /tmp/opencode/C0_ext.elf \
  --sortie /tmp/opencode/ext_build_after --verifier \
  --rapport /tmp/opencode/ext_build_after.tsv \
  tests_selfhost/*.rose rose/tests_aot8/programmes/*.rose
# -> 155/166 artefacts ELF natifs (8 INVALIDE, 3 EXEC_ECHEC)

# validation globale
RUST_TEST_THREADS=1 bash scripts/garde_fou_mem.sh 12000000 \
  bash scripts/validation_finale.sh                       # 0 échec
```

Toutes les mesures de ce document sont des constats datés du 23/11/2026 dans
ce worktree ; elles doivent être re-mesurées après tout changement de
composite, de graine Rust ou de topologie.

## 7. Ce qui manque exactement pour remplacer `cargo` — jalons, pas de promesse

**Nous ne promettons pas le remplacement de `cargo` pour la release.** Le
binaire livré reste `cargo build --release` (72 606 lignes Rust, 5 391 440 o).
Ce chantier a remplacé **une chaîne de compilation** (`source → RIR → ELF`),
pas l’outil de build, ni le moteur d’exécution, ni l’emballage release.

Écart mesuré, pas estimé :

- le composite `rose_natif.rose` = **284 `fn` Rose**, 442 568 o ; le moteur
  Rust = **72 606 lignes**, dont `jit.rs`, `interp.rs`, `rosevm.rs`,
  `transpile.rs`, `wasm.rs`, `natif_emetteur.rs`, `repl.rs`, `main.rs` ;
- les seuls syscalls émis par le runtime G5 sont `x8 ∈ {0, 56, 57, 63, 64, 80,
  93}` (`read`, `openat`, `close`, `getrandom`, `write`, `fstat`, `exit`),
  relevés par `grep -oE 'movz\(8, [0-9]+\)' compilateur/emetteur_elf.rose |
  sort | uniq -c`. **Ni `mmap` (222), ni `mprotect` (226)** : un JIT qui
  compiles puis saute dans le code produit est impossible tel quel ;
- l’espace des globales G5 est **512 × 8 octets à `0x60004000`**
  (`emetteur_elf.rose` ≈ l.1159 et 4133). Toute composition de plus de modules
  doit envelopper ses variables dans des `fn`, comme le front-end déjà fait ;
- `rose --help` expose 30+ sous-commandes (`tokens`, `ast`, `rir`, `verif`,
  `latex`, `console`, `bc`, `rvm`, `transpile`, `aot`, `link`, `update`,
  `assoc`, `debug-lenteur`, `wasm`, `info`…) : **une seule** est dans le
  composite (compiler une source en ELF).

Jalons minimaux, dans l’ordre où ils deviennent mesurables :

1. **J1 — vérification Sans Rust** (petit, adjacent à ce chantier) : envelopper
   `compilateur/verifieur.rose` (79 `fn`) dans une `fn` du composite pour
   rester sous les 512 globales, puis exposer `--verif`. Contrôle : 166 sources
   vérifiées sans appeler `rose/rose`.
2. **J2 — multi-backendsnatifs** : compiler `rose_natif_pe.rose` (207 `fn`) en
   un ELF autonome qui émet du PE. Aujourd’hui le PE est valide (`file` = PE32+
   AArch64, 3 sections, 4 096 o mesurés) mais **seulement interprété par la
   graine Rust** ; il manque un émetteur ELF sachant produire un backend PE.
3. **J3 — exécution sans Rust** : écrire en Rose un interpréteur du RIR complet.
   Le composite n’émet qu’un **sous-ensemble G5** du RIR (les nœuds HOF,
   valeurs composées, `eval_expr`, etc. sont traités par l’ancien compilateur
   inatteignable). Contrôle : exécuter les 166 sources sans moteur Rust.
4. **J4 — JIT** : une fois J3 disponible, soit ajouter `mmap`/`mprotect` à
   l’émetteur G5 (chantier `emetteur_elf.rose`, interdit ici), soit faire du
   AOT à la volée + exécution d’un ELF temporaire. Contrôle : reproduire les
   25/25 `principal=` de `validation_finale.sh` sans `jit.rs`.
5. **J5 — WASM / RVM / transpilation** : **zéro ligne** de sérialiseur dans le
   composite (`.rosebc`, table d’opcodes, VM, émetteurs JS/Py/C/WASM). Les
   sources `emetteur_cpp.rose`, `emetteur_rust.rose`, `emetteur_x64.rose`
   existent mais ne sont pas composées. Contrôle : `--cible wasm|bc` sans Rust.
6. **J6 — le vrai remplacement de `cargo`** : porter le **moteur** (JIT
   Cranelift, GC, exactitudes bigint/rationnel, REPL, CLI) en Rose, puis le
   compiler avec C0. Tant que ce jalon n’est pas fait, `cargo` reste requis
   pour le binaire, les tests Rust, la CI et les installeurs.

## 8. Causes exactes des échecs restants (mesure avant/après)

Sur 166 sources, **71 restent non conformes après extension** (8 échecs de
compilation + 63 ELF émis mais sortie divergente). Aucune cause n’est une
« limite d’ambition ».

**8.1 — 8 échecs de compilation (sortie non ELF), causes mesurées :**

| Cause exacte (message) | Programmes | Nature |
|---|---|---|
| `Fonction 'fn' inconnue` / `Variable 'carre' utilisée sans valeur` | `t106_hof_param`, `t107_hof_var` | fonctions d’ordre supérieur : le front-end Rose et le RIR G5 n’ont ni valeur fonction ni closures |
| `Argument de f: chaine reçu, entier attendu` / `f attend 1 argument(s), reçu 2` | `t104_surcharge_arith`, `t105_surcharge_arite` | surcharge par type/arité absente de la table des `fn` (une seule entrée par nom) |
| `Listes de listes non supportées pour l’instant` | `t55_liste_imbriquee` | `check_expr` refuse `BASE($t_elt) == "liste"` (emetteur.rose ≈ l.1549) |
| `rc=2` sans message, RIR produit mais backend G5 avorte | `t35_factorielle`, `05_bigint_factorielle`, `21_primitives_const` | opérateur `!` (factorielle) et primitives constantes non couvertes par `g5_eval`; `g5_fail` → `quitter(2)` |

Les trois premières lignes étaient **11** avant extension ; `t14_type`,
`t91_prng` et `24_compose` ont rejoint la colonne OK.

**8.2 — 63 ELF valides mais sortie divergente :** le compilateur a bien fait
son travail (machine AArch64, exécution rc=0 dans 59 cas sur 63) ; la
divergence est dans le **runtime G5 émis**, pas dans le front-end. Les
familles mesurées :

- comparaisons et opérations sur entiers au-delà du tag 3 (`t20*`, `t21*`,
  `t22*`, `t23*`, `g5b5*`) : valeurs rendues comme `0x0400000000000007`
  (tag « big » posé sur un petit résultat) ou tronquées à 64 bits au lieu
  des décimales exactes (`2^100` rend `0`, `t223_decimal_geant` rend
  `248789718555101906` au lieu de `123456789012345678901234567890`) ;
- rationnels calculés en boucle (`g5b510`–`g5b513`, `t240`–`t243`) :
  `1/3` rend `38898550215390939.[428571]` au lieu de `14285714285714285714.[285714]` ;
- flottants (`t09_flottant`, `programmes:08_flottant`) : `1.5` rend `1`,
  `3.75` rend `3` — le G5 n’a pas de chemin flottant (`flottant` est mappé sur
  `g5_entierl`) ;
- `20_hasard` : sortie aléatoire, comparaison stricte non reproductible par
  construction (ELF `88` vs référence `58` dans la salve mesurée) ;
- 4 cas où **la référence Rust panique** (`t100_arg31`, `t101_arg36`,
  `t72_si_imbrique`, `t999_critique_g5` : `src/jit.rs:4037` / `:9436`) et où
  l’ELF auto-hébergé rend une valeur plausible : ils sont comptés « NO » par
  l’oracle strict mais ne sont pas des régressions du compilateur Rose.

Correctif de ces familles = chantier **emetteur_elf.rose / runtime G5**
(fichier interdit ici), ou port du calcul exact dans un interpréteur RIR
écrit en Rose (jalon J3).
