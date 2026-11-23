# OPT_INTLOOP_FINAL — dernier domaine perdu : `int_loop`

Worktree exclusif `/tmp/opencode/opt_jit`, HEAD `8886c1d`. Aucun commit,
aucune publication. Sources possédées : `rose/src/jit.rs` (voie boucle entière
uniquement), `rose/tests/opt_intloop_final.rs`, ce document.

Binaire final : `e8282b5f033010…` (départ : `3af377d3729ceb2f…`).
Rust du banc : `d03a5015e6aaf8c7…`, **le même avant et après toute salve**.

## 1. Le chiffre annoncé (2,574×) était un artefact de mesure

### 1.1 Protocole anti-variance

* `scripts/bench_perf.sh` : 3 salves complètes, **9 tirages entrelacés** par
  domaine/langage (rotation Rose→Rust→C→Python), un échauffement par
  (domaine, langage) non compté, tous pinés `taskset -c 15`, garde-fou
  12 000 000 Ko.
* SHA256 du binaire Rose **et** du binaire Rust relevés avant et après chaque
  salve (`/tmp/opencode/opt_intloop_mesure.sh`, données brutes
  `/tmp/opencode/{results,summary}_{base,unroll}_{1,2,3}.json`).
* Le harnais vérifie `RESULT:` et `INTERNAL_NS:` à chaque tirage : aucun
  tirage n'est rejeté, donc les médianes portent sur 9 échantillons réels.
* Toutes les valeurs ci-dessous sont des temps **internes** (chronomètre
  posé par le programme autour du calcul seul), jamais du wall-time.

### 1.2 Vraies médianes — binaire de départ

| salve | Rose (ms) | Rust (ms) | Rose/Rust | SHA Rose | SHA Rust |
|---|---:|---:|---:|---|---|
| 1 | 0,6828 | 0,5186 | 1,3166× | stable | stable |
| 2 | 0,6840 | 0,5132 | 1,3327× | stable | stable |
| 3 | 0,6847 | 0,5137 | 1,3330× | stable | stable |
| **médiane des 3** | **0,6840** | **0,5136** | **1,3327×** | — | — |

Rose est extrêmement stable (0,663–0,706 ms sur 27 tirages). Rust ne l'est
pas : **0,258–0,264 ms** ou **0,513–0,521 ms**, avec un écart exact d'un
facteur 2,0004, **à binaire identique et à l'intérieur d'une même salve**.
Exemple (salve 3, 9 tirages Rust) : `0,518 0,514 0,259 0,259 0,515 0,513 0,518 0,519 0,261`.

Le 2,574× de `bench/perf/out/summary.json` venait d'une salve où la médiane
Rust était tombée à 0,265 ms (majorité de tirages « rapides »). Avec la
médiane de 3 salves, l'objectif < 1,5× était **déjà atteint avant toute
modification** (1,333×). Le correctif ci-dessous rend le résultat robuste
même contre le mode rapide de Rust.

## 2. Cause chiffrée de la bimodalité Rust

Désassemblage du corps chaud de `bench/perf/bin/int_loop` (`objdump -d`,
`_RNvCs1ybTRqDAqh9_8int_loop4main`, 0x8dd8–0x8dfc) :

```
8de4: stur x11, [x29, #-16]   ; black_box(k) -> [sp]
8de8: subs x8, x8, #1
8dec: add  x11, x11, #1
8df0: ldur x12, [x29, #-16]   ; relecture de [sp]
8df4: add  x9, x12, x9        ; s += k
8df8: str  x9, [sp, #24]      ; black_box(s) -> [sp+24]
8dfc: b.ne 8de4
```

`black_box` est donc un aller-retour mémoire **par itération**, avec une
dépendance store→load sur `[sp]`. Deux expériences isolent la cause :

| expérience (20 tirages, c15) | médiane | rapides (<0,4 ms) | lents |
|---|---:|---:|---:|
| `taskset -c 15` (harnais normal) | 0,5134 ms | 9 | 11 |
| `setarch -R taskset -c 15` (ASLR désactivé) | **0,2705 ms** | **20** | 0 |
| `taskset -c 10` (cœur A725, pas X925) | 0,6410 ms | 0 | 20 |

Test complémentaire à ASLR désactivé, en faisant varier la taille de
l'environnement (donc l'alignement de `sp`, `/tmp/opencode/rust_bimodal.py`)
:

```
pad  0..24 octets : 0,513–0,517 ms    (6 tirages chacun)
pad 32..56 octets : 0,258–0,260 ms
pad 64..256       : 0,513–0,517 ms    (période 64 octets)
```

**Cause** : l'ASLR randomise le bit 5 de l'adresse de la pile. Selon sa
valeur, la chaîne `stur [sp] → ldur [sp] → add → str [sp+24]` est transférée
en 1 cycle ou en 2 cycles par itération sur le Cortex-X925 à 3,9 GHz ;
10⁶ itérations donnent exactement 0,258 ms ou 0,513 ms. Ce n'est **ni** une
variation de binaire (SHA constant), **ni** une régression Rose (stable à
0,684 ms), **ni** du bruit de fréquence (le rapport est exactement 2).

## 3. Analyse du code JIT de la boucle (avant correctif)

`ROSE_DUMP_RIR=1` confirme la voie :
`VAGUE2 boucle-nu test=3 back=6 ctr=2 borne=1000001 garder=[0,2]`.

`ROSE_DUMP_CODE` + `objdump -D -b binary -m aarch64` : noyau à **13
instructions par itération**, dont 6 `mov` de permutation :

```
1c4: mov w1,#16961 ; movk w1,#0xf,lsl#16   ; borne rematérialisée
1cc: cmp x0,x1 ; mov x3,x0 ; b.lt 2f4
2f4: mov x0,x27 ; mov x2,x26 ; mov x23,#1 ; mov x1,x3
304: add x5,x1,#1 ; add x0,x0,x1 ; mov x27,x0 ; mov x24,x23 ; mov x0,x5
318: b 1c4
```

Le CLIF `ROSE_DUMP_IR=1` montre 4 phis de bloc (`ctr`, `acc`, et deux
drapeaux `unboxed` constants à 1) et des `iconst` morts dans le corps.

### Tentatives CLIF mesurées puis REJETÉES

| tentative | médiane Rose (3 salves) | verdict |
|---|---:|---|
| supprimer les `def_var(slot_unboxed)` dans la boucle nu | 0,7256 ms | +6,1 % — rejeté |
| ne pas lire `use_var(slot_unboxed)` dans `Eabstrait::Slot` nu | 0,7227 ms | +5,7 % — rejeté |
| hoister l'`iconst` de la borne dans le préheader | objdump **identique** | Cranelift rematérialise la constante — rejeté |

L'allocateur de Cranelift est sensible à la moindre modification de la forme
SSA : les deux réductions « évidentes » du nombre d'instructions rendent la
boucle plus lente. Le déroulage, lui, réduit le nombre **d'itérations du
noyau compilé** et donc amortit ces permutations.

## 4. Correctif retenu : déroulage ×4 sous garde strict

`rose/src/jit.rs` — **107 lignes ajoutées, aucune ligne existante modifiée**
dans les autres voies :

1. `motif_acc_plus_ctr_unroll(corps, b)` : détecteur STRICT, exige
   exactement 2 `Affect` dans le corps, `acc = acc + ctr` puis
   `ctr = ctr + 1` (sens des opérandes compris). Tout autre motif (fib avec
   `% m`, deux accumulateurs, soustraction, borne > 2²⁰) reste sur
   l'émetteur nu générique ou sur l'émetteur vérifié — le garde
   `rir::boucle_nu` inchangé continue de prouver l'absence de boxage et de
   débordement.
2. Émetteur dédié : 4 `iadd` d'accumulation + 3 `add` d'index + 1 `add` de
   compteur + 1 comparaison + 1 branche, soit ~2,5 instructions par
   itération source (contre 13), avec **2 seuls phis** (`i`, `acc`) au lieu
   de 4. Boucle résiduelle de 0 à 3 tours pour les restes. Les slots sont
   réécrits et le tag remis à zéro à la sortie, donc le code AST suivant
   (`temps()`, `chaine()`, affichage) voit exactement les mêmes valeurs.

Code machine final du noyau ×4 (0x320–0x350) : 13 instructions pour 4 tours.

### Résultats (3 salves, même protocole)

| salve | Rose (ms) | Rust (ms) | Rose/Rust | SHA Rose | SHA Rust |
|---|---:|---:|---:|---|---|
| 1 | 0,3206 | 0,5129 | **0,6250×** | stable | stable |
| 2 | 0,3206 | 0,5136 | **0,6241×** | stable | stable |
| 3 | 0,3272 | 0,2664 | **1,2283×** | stable | stable |
| **médiane des 3** | **0,3206** | **0,5136** | **0,6250×** | — | — |

* Accélération Rose : **0,6840 → 0,3206 ms = 2,13×**.
* Pire cas mesuré (Rose 0,321 / Rust mode rapide 0,258) : **1,24×**, toujours
  sous l'objectif 1,5×. Le résultat ne dépend plus du tirage de l'ASLR.
* Rose est plus stable après correction : 0,320–0,333 ms sur 27 tirages.

## 5. Validation

* `RUST_TEST_THREADS=1 cargo test --release` (garde-fou 12 Go) :
  **327 tests unitaires + tous les tests d'intégration, 0 échec**, dont
  `opt_intloop_final` **4/4** (`/tmp/opencode/opt_intloop_cargo_test.log`).
* `bash scripts/validation_finale.sh` (garde-fou 12 Go) : **0 échec**,
  AOT 75/75, **JIT 25/25 Cranelift**, point fixe, WASM/RVM, AZ 31 conforme,
  self-host HOST-1 (`/tmp/opencode/opt_intloop_validation.log`).
* `cargo test --release --test opt_jit parite_25_programmes` : **ok** —
  parité JIT ↔ interpréteur **24/24** (`20_hasard` exclu).
* Tests dédiés : borne du banc, restes 1..9 (tous les `n mod 4`), départs
  non canoniques (`i=3`, `acc=10`, `acc=-100`, `acc=-35`), repli hors garde
  (borne 2²⁰+4) et corps non strict, chacun comparé à l'interpréteur.

## 6. Diff précis

`rose/src/jit.rs` — **+107 / −0**, deux blocs :

1. avant `impl Emetteur` : la fonction `motif_acc_plus_ctr_unroll` (25 lignes
   de commentaires + gardes) ;
2. dans `emettre_boucle_nu`, juste après `self.lits_hoistes = Some(hoistes)`
   et avant la création du `header` générique : le bloc `if let Some(acc) = …`
   (82 lignes) qui émet `h4/b4/hr/br/fin` et retourne. Le chemin générique
   qui suit est **byte pour byte identique à HEAD**.

`rose/tests/opt_intloop_final.rs` — nouveau, 124 lignes, 4 tests.

`rose/docs/OPT_INTLOOP_FINAL.md` — nouveau, ce document.

Aucune modification de `rir.rs`, `run.rs`, des autres boucles (table,
produit, chaîne, liste) ni des helpers.
