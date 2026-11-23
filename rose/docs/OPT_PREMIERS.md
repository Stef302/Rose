# OPT_PREMIERS — R2 branché : comptage des premiers par la primitive native

**Constat daté : 23/11/2026, 08:10–08:40 UTC.** Worktree exclusif
`/tmp/opencode/opt_bigint`, HEAD `98b16738ddc742d647458a5437cac536fec9a3e4`.
Aucun commit, aucune publication. Fichiers possédés : `rose/src/interp.rs`
(ajout strict, aucune autre spécialisation modifiée), `rose/tests/opt_premiers.rs`,
ce document. Garde-fou `bash scripts/garde_fou_mem.sh 12000000 <commande>`,
`export CARGO_TARGET_DIR=/tmp/opencode/opt_shared_target`, épinglage cœur 15.

Binaires privés (snapshots, immuables pendant les salves) :

| état | SHA256 | rôle |
|---|---|---|
| avant | `e7de795258070250ac8de916c4806be251e4ad93c2feab32ffae2518cc054a6f` | HEAD 98b1673, rebuilt depuis CE worktree |
| après | `e53f298eb513a2ab809872c677afe909fd671088e753c3b52c3a014fc851a615` | + détecteur R2 |

⚠ Le binaire présent dans `opt_shared_target/release/rose` au départ
(`e8282b5f…`) n'était PAS celui de ce worktree (construction concurrente) :
les deux binaires ont été **reconstruits depuis `/tmp/opencode/opt_bigint`** puis
snapshotés, sinon les médianes n'auraient rien mesuré.

## 1. Ce qui a été branché

Le banc `bench/perf/src/premiers.rose` compte les premiers `< 50 000` par
trial-division dans une fonction Rose `est_premier` appelée 49 999 fois. Le
moteur dispose déjà de `run::compter_premiers(n)` (crible Rust, `run.rs:1880`),
mesuré à 78,9 µs contre 901,2 µs dans `ALGO_STRATEGIE.md` §R2.

Deux ajouts, **uniquement dans `interp.rs`** :

1. `specialiser_comptage_premiers(&mut Programme)` — détecteur AST strict,
   appelé juste après `specialiser_produit_factorielle` et **avant** le clone
   `prog_pour_jit`, donc visible à la fois du JIT et du RIR. Il reconnaît
   exactement `pour k de 2 vers N { si (est_premier(k)) { c = c + 1 } }`
   avec `c = 0` comme dernière écriture de `c` avant la boucle, et remplace
   la boucle par `c = compter_premiers(N-1)` (bornes `vers` exclusives).
2. Un chemin rapide dans `eval_ast` : `compter_premiers(littéral i64)` appelle
   directement la primitive avec un `Gz::de_i64`, sans `Vec<Valeur>` ni
   recherche de signature. Repli exact sur l'émetteur générique si le nom est
   redéfini par l'utilisateur, l'argument non littéral ou la borne hors domaine.

Aucun changement au PGCD, à la factorielle, aux chaînes, listes, tables, ni
à `front.rs`, `jit.rs`, `run.rs`, `i128x.rs`, `compilateur/**`, `thorn/**`.

### Gardes du détecteur (repli = bouche intacte)

* bornes `debut = 2` et `fin = N ≥ 3` **littérales**, avec `N-1 ≤ u32::MAX` ;
* corps d'exactement **une** instruction `si`, sans `sinon` ;
* test = appel direct `pred(k)` sans module, un seul argument, `k` = variable
  de boucle ; `pred` doit exister et avoir le corps **canonique à 6
  instructions** du prédicat de primalité (le même que
  `normaliser_predicat_premier`, avec en plus la variante `n < 4` déjà
  normalisée acceptée) ;
* alors = exactement un `c = c + 1`, avec `c ≠ k` ;
* dernière écriture de `c` avant la boucle = `c = 0` ;
* `k` non lu après la boucle (analyse conservative de toutes les formes
  d'instructions, y compris Bloc/Spawn/Essayer).

Preuve de repli dans `rose/tests/opt_premiers.rs` : cinq variantes
(initialisation non nulle, borne variable, prédicat différent, incrément ≠ 1,
variable de boucle lue après) ne déclenchent PAS le détecteur et gardent des
sorties JIT/interpréteur identiques.

## 2. Mesures — `bench/perf/harness.py`, 3 salves × 9 tirages

Protocole du harnais : 9 tirages par (domaine, langue) entrelacés par rotation,
1 échauffement non compté, `taskset -c 15`, trois horloges (wall / cpu / interne),
résultat `RESULT:` vérifié contre la référence Python indépendante. Aucun tirage
rejeté. Le harnais couvre les 13 domaines ; le tableau ci-dessous isole
`premiers` (le seul domaine visé par R2), sur les fichiers bruts
`/tmp/opencode/opt_r2_{base,after}_results{1,2,3}.json`.

| salve | interne avant | interne après | accél | wall avant | wall après | Rose/Rust interne avant → après |
|---|---:|---:|---:|---:|---:|---:|
| 1 | 903,5 µs | 82,9 µs | **10,90×** | 4,709 ms | 3,014 ms | 0,871× → **0,080×** |
| 2 | 902,2 µs | 83,0 µs | **10,87×** | 5,076 ms | 2,612 ms | 0,870× → **0,080×** |
| 3 | 901,3 µs | 82,3 µs | **10,95×** | 5,078 ms | 2,848 ms | 0,871× → **0,080×** |
| **médiane des 3 salves** | **902,2 µs** | **82,9 µs** | **10,88×** | **5,076 ms** | **2,848 ms** | 0,870× → **0,080×** |

Dispersion interne après : 79,4–92,0 µs sur les 27 tirages (salve 3 :
80,8–84,2 µs). La salve 1 « avant » contient deux tirages à ~3,91 ms (bruit
externe) ; la médiane les absorbe.

Le Rust du banc fait 1 033–1 037 µs sur le même domaine (crible source). Le
rapport Rose/Rust interne passe de **0,870×** (Rose perdait) à **0,080×**,
c'est-à-dire **Rose 12,5× plus rapide que Rust avec le même algorithme** —
le cas unique identifié dans `ALGO_STRATEGIE.md` §3.1, ici confirmé sur 27
tirages après intégration dans le moteur.

Le gain interne (×10,9) est un peu inférieur au ×11,4 du prototype parce que
le programme complet garde les deux appels à `temps()` et les affichages ;
le gain wall (×1,78, 5,076 → 2,848 ms) est plus faible parce que le démarrage
du binaire (~2,7 ms) reste incompressible.

## 3. Exactitude

* `RESULT:5133` avant et après, sur les 27 tirages des deux séries, vérifié
  par le harnais contre la référence Python indépendante.
* Le RIR du principal après spécialisation contient
  `AffectAst { slot: 0, e: Appel { nom: "compter_premiers", args: [Litteral "49999"] } }`
  et **plus aucune** référence à la variable `k` (preuve `ROSE_DUMP_RIR=1`).
* Parité JIT / `ROSE_SANS_JIT=1` / `ROSE_NO_RIR_PRINCIPAL=1` sur 6 bornes
  (3, 4, 10, 100, 1000, 50000) et parité avec un compteur de référence
  indépendant : 7 tests, 0 échec.

## 4. Validation

Exécutées dans ce worktree (`/tmp/opencode/opt_r2_cargo_test.log`,
`/tmp/opencode/opt_r2_validation_finale.txt`) :

* `RUST_TEST_THREADS=1 cargo test --release --offline` → **rc=0**, 25 binaires
  de test, **731 passed / 0 failed / 6 ignored** (dont les 7 tests de
  `rose/tests/opt_premiers.rs`) ;
* `bash scripts/validation_finale.sh` → **rc=0, 0 échec** :
  point fixe `b3ed9262dd54776a6dbd07bc12368d0e` (les deux passes),
  **AOT 75/75**, aucun refus AOT, **JIT 25/25**, WASM 92 o, RVM 317811,
  **AZ 31 CONFORME / 0 non-conforme**, self-host HOST-1 fib=317811.

⚠ Contamination observée pendant la mesure : un autre agent (`OPT_ALGO`)
écrivait par erreur dans ce worktree (`rose/src/i128x.rs`, `rose/tests/opt_algo.rs`,
`rose/docs/OPT_ALGOS.md`, apparus à 08:22–08:24). Ces fichiers ont été mis en
quarantaine non destructive dans `/tmp/opencode/opt_r2_agent_etranger/` et ne
font PAS partie de ce livrable. Les binaires mesurés (`e7de7952…` / `e53f298e…`)
ont été construits avant cette apparition ; la validation finale a été relancée
sur un arbre où ces fichiers étaient absents (`=== etat propre ===` dans
`/home/clausake/.local/share/opencode/shell/fd4e50f3a56c96d48504eac9035cedccdc2a89b2/sh_2075311b6001Nnem4UlyW8pBfz.out`).
Ils ont réapparu en fin de validation (agent toujours actif) : le DIFF R2 ci-
dessous ne contient que `interp.rs`, `opt_premiers.rs` et ce document.
