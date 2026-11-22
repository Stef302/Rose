# VAGUE 2 — Boucles entières : suppression des checks dans la boucle chaude

**Date des mesures** : 22/11/2026. **Base** : HEAD `0f9c28b` (avant toute modification).
**Fichiers possédés** : `rose/src/jit.rs`, `rose/src/rir.rs`, `rose/tests/opt_boucles.rs`,
`rose/docs/OPT_BOUCLLES.md`. Aucun commit, aucune publication.

## 1. Protocole de mesure

* Harnais : `bench/perf/harness.py`, programmes `bench/perf/src/{int_loop,fib_iter,premiers,table}.rose`.
* Deux épinglages : `taskset -c 15` (cœur X925, conditions du banc parent : `int_loop 1,437 ms`
  avant optimisation) et `taskset -c 2` (cœur little, machine big.LITTLE).
* A/B/A entrelacé, 5 tirages (3 tirages de confirmation), base et nouveau dans la **même
  salve**, Rust compilé dans la même salve, médianes ; chaque tirage vérifie `RESULT:` contre
  la référence Python indépendante du harnais (`/tmp/opencode/vague2_ab.py`).
* Binaire de base reconstruit depuis `git archive HEAD rose` dans
  `/tmp/opencode/vague2/base_target/release/rose` ; binaire optimisé dans
  `/tmp/opencode/vague2/target/release/rose`.
* ⚠ Le répertoire `CARGO_TARGET_DIR=/tmp/opencode/opt_shared_target` est partagé avec les
  autres agents : un `cargo build` concurrent y réécrit `release/rose` avec un autre arbre
  source (constaté : binaire sans les symboles de cette vague alors que le build avait
  réussi). Le binaire de cette vague a donc été construit dans un répertoire cible dédié,
  puis comparé au binaire de base construit de la même manière.

## 2. Diagnostic — causes identifiées dans le code généré

### 2.1 CLIF avant optimisation (`ROSE_DUMP_IR=1`, `int_loop.rose`)

La boucle générique émet, **par itération**, deux chaînes de vérification :

```
block4(v18, v19, v116, v118):        ; v18 = compteur, v116 = $s, v19/v118 = drapeaux
    v24 = icmp slt v18, v23          ; test i < 1000001
    v27 = select v24, 1, 0           ; booléen → i64
    v30 = icmp ne v27, 0             ; re-test du booléen
    brif v30, block5, block8
block5:
    v40 = iadd v35, v38              ; $s + i
    v41 = bxor v40, v35 ; v42 = bxor v40, v38 ; v43 = band ...
    v44 = icmp sge v43, 0            ; test de débordement signé
    v45 = band v36, v39 ; v46 = icmp ne v45, 0   ; drapeaux « non boxé »
    v47 = band v46, v44
    brif v47, block12, block13       ; block13 = helper rose_add_ent
block6:
    v69 = iadd v63, v68              ; i + 1 — MÊME chaîne de checks
    ... brif v76, block15, block16   ; block16 = helper rose_add_ent
```

Désassemblage du code JIT avant optimisation (`ROSE_DUMP_CODE`, `objdump -D -b binary -m
aarch64`) : la boucle occupe `0x190..0x290`, soit **~40 instructions et deux `blr` vers
`rose_add_ent`** par tour (chemins lents) — `and/cmp/cset/tst/b.ne` pour les drapeaux,
`add/eor/eor/and/cmp/cset` pour l'overflow, plus les `select` du test.

**Cause 1** — le test de boucle `SautSiFauxAbstrait` sur `i < B` passe par
`expr_eabstrait_rir`, qui matérialise un `select` puis un `icmp ne`, au lieu d'un `brif`
direct sur `icmp slt`.

**Cause 2** — chaque `+` entier émet un test de drapeau **et** un test d'overflow avec
repli GMP ; les drapeaux arrivent comme paramètres de bloc (phis), donc `and/cmp/cset`
sont rejoués à chaque tour même quand ils sont constants.

**Cause 3 (fib_iter)** — le `% 1_000_000_007` est émis en division magique
(`smulh / add / asr / lsr / add / msub / add`) avec de plus **quatre `mov/movk` de
constante rematérialisés à chaque tour** (Cranelift ne remonte pas les `iconst` 64 bits),
un test de division par zéro et une correction de signe alors que les deux opérandes sont
prouvés positifs et strictement inférieurs au diviseur.

La voie interpréteur (`ROSE_SANS_JIT=1`) était 40× (int_loop) à 57× (fib_iter) plus lente :
le code généré existait bien, il était sous-optimal, pas absent.

## 3. Modifications

### 3.1 `rose/src/rir.rs` — détecteur de boucle nue

* `BoucleNu { test, back, ctr, borne, garder, mod_rapide }`, `ModRapide`,
  `BOUCLE_NU_BORNE_MAX = 2^20`, `BOUCLE_NU_VALEUR_MAX = 2^40`.
* `boucle_nu(corps)` reconnaît un `pour` dont le test est `compteur < borne` (borne
  littérale ≤ 2^20), le compteur initialisé par un littéral et **écrit une seule fois**
  dans le corps (`+1`), et dont le corps ne contient que des `Affect` arithmétiques
  bornées (`+`, `-`, `% m` avec `0 < m ≤ 2^30`, recopies). `Mul`, `Div`, appels,
  négation et littéraux hors `±2^40` sont refusés.
* `ModRapide` n'est retenu que si les deux opérandes du `%` sont dans `[0,m)` aux
  entrées (valeurs littérales connues) **et** si chaque réécriture d'un opérande est une
  recopie depuis un slot de l'ensemble `{a, b, résultat}`. L'invariant `0 ≤ x,y < m`
  est donc préservé à chaque tour.
* Preuve d'absence de débordement : au garde, `|v| ≤ 2^40`, `B ≤ 2^20` ; un accumulateur
  `acc = acc ± (compteur | petit littéral)` reste sous `2^40 + B² ≤ 2^41` ; une somme
  alimentant un `% m` est sous `2^41` puis son reste sous `2^30` ; le compteur reste sous
  `2^20`. Tout tient dans un i64 : aucun boxage n'est nécessaire.

### 3.2 `rose/src/jit.rs` — émission derrière garde

* `Emetteur.sans_checks` : dans la boucle nu, `expr_eabstrait_rir` émet un `iadd`/`isub`
  nu, pose `tag_tmp = 0` et `unboxed_tmp = 1`, sans test d'overflow ni helper.
* `emettre_boucle_nu` émet **un seul garde** à l'entrée : drapeau `unboxed` et
  `−2^40 ≤ v ≤ 2^40` pour chaque slot lu. Si le garde échoue, un `brif` saute sur la
  **boucle générique vérifiée inchangée** (même code, mêmes helpers GMP) : la sémantique
  est conservée pour tout programme, y compris les débordements réels.
* Le test de la boucle rapide est un `icmp slt` + `brif` direct (cause 1).
* `Emetteur.lits_hoistes` matérialise les littéraux du corps dans le préheader
  (cause 3, 4 `movk` économisés par tour sur fib_iter). ⚠ `header` n'est scellé
  qu'après le saut arrière, sinon `use_var` résout le compteur une fois pour toutes
  et la boucle ne termine pas (mesuré, puis corrigé).
* `% m` prouvé : `somme = x+y` ; `soust = somme−m` ; `select(soust < 0, somme, soust)`
  — 4 instructions au lieu de la division magique et de la correction de signe.
* Instrumentation conservée sous variables d'environnement : `ROSE_DUMP_RIR` (ops +
  détection), `ROSE_DUMP_IR` (CLIF du principal, y compris voie RIR), `ROSE_DUMP_CODE`.

### 3.3 `rose/tests/opt_boucles.rs` (nouveau)

6 tests : activation effective de la voie nu (`VAGUE2 boucle-nu` dans la trace),
résultats exacts, parité JIT ↔ interpréteur sur bornes et accumulateurs variables,
parité sur trois modules de `%`, repli quand le garde ne peut pas prouver (borne >
2^20 et valeur > 2^40), et test unitaire du détecteur (`BoucleNu`, `ModRapide`).

## 4. Résultats — médianes A/B entrelacées

### 4.1 Épingle cœur 15 (conditions du banc parent, 5 tirages)

| Domaine | base (ms) | optimisé (ms) | Rust (ms) | gain base/optimisé | ratio base/Rust | ratio opt/Rust |
|---|---:|---:|---:|---:|---:|---:|
| `int_loop` | 1,437 | **0,682** | 0,264 | **2,11×** | 5,45× | **2,59×** |
| `fib_iter` | 3,620 | **1,087** | 2,059 | **3,33×** | 1,76× | **0,53×** |
| `premiers` | 1,481 | 1,480 | 1,032 | 1,00× | 1,43× | 1,43× |
| `table`    | 0,052 | 0,052 | 0,049 | 0,99× | 1,06× | 1,07× |

### 4.2 Épingle cœur 2 (5 tirages)

| Domaine | base (ms) | optimisé (ms) | Rust (ms) | gain | ratio base/Rust | ratio opt/Rust |
|---|---:|---:|---:|---:|---:|---:|
| `int_loop` | 2,944 | **1,080** | 0,722 | **2,73×** | 4,08× | **1,50×** |
| `fib_iter` | 4,768 | **1,579** | 2,557 | **3,02×** | 1,86× | **0,62×** |
| `premiers` | 3,151 | 3,225 | 1,585 | 1,00× | 1,99× | 2,04× |
| `table`    | 0,104 | 0,099 | 0,093 | 1,05× | 1,12× | 1,08× |

Sans JIT, sur cœur 2 : `int_loop` 117,8 ms (109× le JIT optimisé), `fib_iter` 272,6 ms
(173×). `premiers` et `table` restent au même niveau (le détecteur ne retient pas leurs
boucles, qui contiennent des appels ; aucune régression de codegen).

**Objectifs** : `int_loop` ≥ 2× (5,54 → ≤ 2,8× Rust) : **2,11× de gain, 2,59× Rust** ;
`fib_iter` ≥ 1,3× : **3,33× de gain, Rose 1,9× plus rapide que Rust**.

### 4.3 Désassemblage après optimisation

`int_loop` : boucle rapide `0x2f4..0x318`, **8 instructions**
(`mov / add x0,x0,x1 / add x5,x1,#1 / mov / mov / mov / b`), plus le test
`cmp x0,#0xf4241 ; b.lt 0x2f4`. Avant : ~40 instructions et deux `blr` par tour.

`fib_iter` : plus aucun `smulh/msub` dans la boucle ; le corps est
`add / sub / cmp / csel` pour le modulo, `iadd` pour le compteur et deux recopies.

## 5. Validation

* `cargo test --release` : **313 + 326 + 6 + 9 + 8 + 2 + 1 + 7 + 6 + 2 + 2 + 2**,
  **0 échec** (le premier lancement a rencontré un `Text file busy` sur un ELF AOT écrit
  par un autre agent ; le relancement est vert).
* `bash scripts/validation_finale.sh` : **0 échec** — point fixe émetteur (md5
  `b3ed9262…`), AOT 75/75, **JIT 25/25**, WASM + RVM 317811, batterie AZ 31/31
  conforme, self-host HOST-1 fib=317811.
* `cargo test --release --test opt_jit parite_25_programmes` : **ok** (24 programmes
  comparables ; `20_hasard` est exclu par conception).
* `cargo test --release --test opt_boucles` : **6/6**.

Toutes les mesures ci-dessus sont des constats datés de la salve indiquée, pas des
consignes : re-mesurer après tout changement de quantification, de topologie ou de
binaire.
