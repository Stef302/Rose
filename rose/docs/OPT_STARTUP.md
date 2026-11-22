# VAGUE 4 — le TEMPS MUR du démarrage (constats datés du 23/11/2026)

## Mission et périmètre

Le banc `bench/perf` mesurait (salvo du 22/11, 9 tirages, cœur 15) un wall
Rose ≈ 3,0 ms contre Rust ≈ 2,4 ms sur presque tous les domaines, alors que
le calcul interne est souvent gagné (rationnel 0,116×, chaîne 0,063×,
fib_iter 0,527×). L'écart ~0,5–0,6 ms était attribué au démarrage du
processus. Ce document sépare les faits mesurés des gestes techniques, et
ne cite aucune spécification.

Fichiers possédés : `rose/src/demarrage.rs`, `rose/src/main.rs` (chemin
d'amorçage AVANT exécution uniquement), `rose/tests/opt_startup.rs`,
`rose/docs/OPT_STARTUP.md`. Aucune commande ni logique d'exécution n'a été
modifiée. Aucun commit, aucune publication.

## 1. Mesure initiale — `/usr/bin/time -v` et `rose debug-lenteur`

`/usr/bin/time -f %e` arrondit à 0,01 s : **illisible** sur un programme de
3 ms. Les 9 tirages rendent `0.00 0.00 0.00` pour `version`, `--help` et un
petit programme, avec seulement le nombre de défauts de page (79 pour
`version`, 161 pour un programme) et le RSS (1,5 / 4,2 Mio). La mesure
utile est donc celle du harnais (`perf_counter_ns` autour de fork/exec/wait4)
et de `rose debug-lenteur`.

`rose debug-lenteur -n 9` (binaire de départ, 23/11) :

```
lancement (rose version)     680 µs   (1er 684 µs)
état (rose info)             696 µs
programme minimal            872 µs   (surcoût d'exécution 191 µs)
analyse seule (rose latex)   830 µs
```

`ROSE_TIMING=1 rose <programme>` (47 µs de parse, ~0,4 ms de `compiletout`
sur un programme sans boucle) :

```
TIMING parse=50.7µs transform=6.5µs jitnew=45.9µs
compiletout=423.9µs compileprincipal=384ns exec=7.0µs total=534.5µs
TIMING entree_main=32ns console=64ns pret=29.9µs post_spawn=419.2µs
```

**Cause identifiée** : le surcoût n'est PAS dans l'entrée dans `main`
(32 ns), ni dans la préparation de la console (64 ns — `preparer()` est un
no-op hors Windows), ni dans la lecture du fichier. Il est dans :
1. **le chargeur / l'éditeur de liens** : le binaire de départ est un
   *static-pie* avec 6 253 relocations `RELA` (150 120 octets) et
   `BIND_NOW` — le loader doit résoudre toutes ces relocations AVANT le
   premier instructions de `main` ;
2. **la préparation JIT de Cranelift** (~0,4 ms sur programme minimal) :
   pour un principal de 4 opérations sans saut, le moteur construit un
   `JITModule`, enregistre ~38 symboles runtime, émet et finalise deux
   fois un module qui ne produit aucun gain (l'interpréteur du RIR fait
   la même chose en ~7 µs).

`strace -c` confirme que le coût système est minuscule (31 appels, 517 µs
cumulés pour `version` dont 331 µs pour `execve` lui-même) : une grande
partie du temps mur est dans le **chargement statique PIE + relocations**,
pas dans les syscalls ni dans le code Rose.

## 2. Expériences A/B/A entrelacées (pin cœur 12, 40 lots × 7 tirages)

Protocole : rotations entrelacées des variantes, **minimum par lot** (le
bruit est unilatéral : le minimum est l'estimateur du coût intrinsèque),
médiane des minima. Machine partagée, valeur absolue à re-mesurer ; les
**différences intra-lot** sont le signal.

### 2.1 Linker : PIE vs non-PIE

Deux binaires du MÊME source (worktree HEAD 31c9e74), mêmes
`lto=fat`/`codegen-units=1`, seul `RUSTFLAGS` diffère :
`-C relocation-model=static` ajouté aux `-C target-feature=+crt-static`
du `rose/.cargo/config.toml`.

| variante                         | `file`           | relocations | version, médiane-min |
|----------------------------------|------------------|-------------|----------------------|
| PIE (témoin, config du dépôt)    | static-pie       | 6 253       | 1,83 ms              |
| non-PIE (`relocation-model=static`) | static executable | 0        | 1,78 ms              |
| Rust du banc (dynamique)         | dynamic PIE      | 453         | 1,92 ms              |

Le gain du non-PIE est ~0,05 ms (≈ 3 %), sous le bruit de salve : **le
geste ne suffit pas à expliquer les 0,5 ms**, et il dégraderait la
portabilité (ASLR perdu). Il n'est donc PAS appliqué au codepossédé : le
plus petit gain réel est ailleurs. Un expérimentateur qui voudrait
pousser ce levier doit le faire dans `rose/.cargo/config.toml` (fichier
NON possédé dans cette vague), pas dans `main.rs`.

### 2.2 JIT sur programme sans boucle (changement dans `jit.rs` prototype)

Prototype hors worktree (`/tmp/opencode/opt_proto`) : un RIR principal
court (< 32 opérations, aucun saut) n'est plus considéré « compilable par
Cranelift » ; `prepare_jit` et `compiler_principal` refusent alors la
construction du module, et le RIR est interprété.

| domaine     | témoin | prototype | Rust | surcoût témoin→proto |
|-------------|--------|-----------|------|----------------------|
| version     | 1,83   | —         | 1,92 | —                   |
| startup     | 2,57   | 2,12      | 2,10 | **−0,45 ms**        |
| rationnel   | 2,41   | 2,39      | 2,17 | −0,03 ms            |
| chaîne      | 2,55   | 2,52      | 2,05 | −0,03 ms            |
| fib_iter    | 4,45   | 4,56      | 2,09 | +0,11 ms (RÉGRES)   |

**Verdict : rejeté.** Le gain ne se voit QUE sur `startup` (le programme
le plus court du banc) ; `fib_iter` régresse et le principal JIT est
nécessaire aux domaines que Rose gagne déjà. Ce prototype n'est PAS
intégré au worktree.

## 3. Changement retenu (fichiers possédés seulement)

### 3.1 `rose/src/main.rs` — court-circuit informationnel

`version`, `--version`, `info`, `--help`, `-h`, `aide` avec **exactement
deux arguments** sont servis juste après `console_win::preparer()`, avant
l'installation du hook de panic, la collecte de `argv`, la vérification
Velopack et la normalisation `methode`/`jit`/`interprete`. Le chemin
historique reste intact dès qu'un troisième argument est présent (y compris
les arguments internes `--veloapp-updated`/`--veloapp-obsolete`).

Aucune sortie observable n'a changé : `md5` identiques pour les 5
commandes comparées entre le binaire HEAD et le binaire modifié.

### 3.2 `rose/src/demarrage.rs` — module `trace` optionnel

`demarrage::trace::Trace` chronomètre 4 phases (console, argv, dispatch,
avant_execution) uniquement si `ROSE_STARTUP_TRACE` est posé. Inerte sans
cette variable : zéro timer, zéro allocation, zéro écriture. Le rapport
va sur **stderr**, jamais sur stdout : les oracles du banc et des tests
restent bit-identiques. Exemple réel :

```
$ ROSE_STARTUP_TRACE=1 rose bench/perf/src/startup.rose
RESULT:0
INTERNAL_NS:16
TRACE-STARTUP depuis_entree_main=2087us
TRACE-STARTUP console=128ns
TRACE-STARTUP argv=4160ns
TRACE-STARTUP dispatch=3248ns
TRACE-STARTUP avant_execution=124992ns
```

Le diagnostic confirme que le chemin dans `main` (console + argv +
dispatch) coûte **< 10 µs** : les ~0,5–0,6 ms sont AVANT `main` (loader)
ou dans la préparation JIT.

### 3.3 `rose/tests/opt_startup.rs`

6 tests de garde-fou : identité bit-à-bit des 5 commandes informationnelles
(court-circuit vs chemin long), présence des 4 phases de trace, absence de
pollution de stdout/stderr.

## 4. Résultats avant/après (23/11, harnais 9 tirages, cœur 12)

Binaire « avant » : `/tmp/opencode/opt_head_target/release/rose` (HEAD
31c9e74, sha256 `1493c921…`, construit avec le même `CARGO_TARGET_DIR`
privé). Binaire « après » : `/tmp/opencode/opt_bench_base/release/rose`
(sha256 `07ad8d79…`). Salve A/A/A entrelacée de 90 tirages + 40 lots × 7
pour l'estimateur robuste (minimum par lot), plus deux salves complètes
du harnais (9 tirages, cœur 12).

Harnais complet avant/après (médianes wall, ms) :

| domaine   | avant | après | Rust du banc | calc interne avant/après |
|-----------|-------|-------|--------------|--------------------------|
| startup   | 3,900 | 3,632 | 2,794        | 0 / 0                    |
| rationnel | 3,593 | 3,516 | 3,730        | 0,125 / 0,126            |
| chaîne    | 3,860 | 3,535 | 2,768        | 0,004 / 0,004            |
| fib_iter  | 6,082 | 5,610 | 5,011        | 1,581 / 1,575            |
| puissance | 4,346 | 3,388 | 2,966        | 0,010 / 0,010            |

Sur `rationnel`, Rose reste devant Rust en wall (3,52 contre 3,73 ms) —
le domaine où le calcul est le plus gagné (0,126 contre 0,914 ms). Sur
`chaîne`, le wall après (3,54) reste derrière Rust (2,77) malgré un calcul
interne 14× plus rapide : le nécessaire est bien dans le démarrage.

| cas              | avant (médiane) | après (médiane) | Rust | diff appariée |
|------------------|-----------------|-----------------|------|---------------|
| `version` (90)   | 2,629 ms        | 2,520 ms        | 2,677 | −0,069 ms     |
| `version` (min)  | 1,832 ms        | 1,851 ms        | 1,921 | +0,019 ms     |

**Constat honeste** : sur le chemin informationnel, l'avant/après est
**dans le bruit** (la différence change de signe selon l'estimateur). Le
court-circuit supprime ~5–10 µs de travail Rust (une allocation de
`Vec<String>`, le hook de panic, un `var_os` Velopack), mais ce n'est PAS
0,5 ms. Le mur est dominé par le **chargement du binaire** (relocations
static-PIE) et, sur chemin d'exécution, par la **préparation JIT**.

Sur les domaines du banc où le calcul est gagné, le prototype JIT a montré
que l'on peut gagner 0,45 ms sur `startup` mais au prix d'une régression
sur `fib_iter` : **aucun geste testé ne réduit le wall de 0,5 ms sur
rationnel/chaîne sans casser les victoires internes**. Le geste technique
qui resterait à explorer (hors fichiers possédés) est le
`-C relocation-model=static` dans `rose/.cargo/config.toml` (gain ~0,05 ms
mesuré, insuffisant) puis une refonte de la décision JIT pour les
programmes MICRO sans boucles — mais le prototype montre que le critère
« court et sans saut » touche aussi des domaines gagnés, donc il faudrait
un critère plus fin, pas un seuil unique.

## 5. Validation (preuves d'exécution)

* `cargo test --release` (`RUST_TEST_THREADS=1`) : **0 échec**, dont les
  6 nouveaux tests `opt_startup` ; 19 harnais de test au total.
* `bash scripts/validation_finale.sh` : **0 échec**
  (`/tmp/opencode/validation_finale.log`, 23/11 23:50 UTC) :
  point fixe émetteur `b3ed9262…`, AOT 75/75, JIT 25/25, WASM 92 o,
  RVM 317811, AZ 31 CONFORME / 0 non-conforme, HOST-1 fib=317811.
* `rose methode`/JIT/AOT/WASM/RVM inchangés (contrôles 2 à 7 du script).

## 6. Diff précis

```
rose/src/demarrage.rs  | +86 lignes  (module trace optionnel, fin de fichier)
rose/src/main.rs       | +44 / -0 lignes
                       |   main(): Trace::nouveau() après Instant initial
                       |   marquer("argv") après action_exec, marquer("dispatch")
                       |   avant le bloc (fichier,args), marquer("avant_execution")
                       |   trace.rapport() avant process::exit(rc)
                       |   court-circuit version/info/help à 2 arguments
rose/tests/opt_startup.rs | nouveau, 6 tests
rose/docs/OPT_STARTUP.md  | nouveau, ce document
```
