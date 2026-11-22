# OPT_JIT_CHAINES — vague 3 : pourquoi le banc chaîne ne bougeait pas

**Constat daté : 22/11/2026**, worktree exclusif `/tmp/opencode/opt_tables`
(HEAD `8aac2f8`), `CARGO_TARGET_DIR=/tmp/opencode/opt_shared_target`, pin cœur 15.
Toutes les mesures ci-dessous ont été **exécutées** (3 salves `bench/perf` de
9 tirages entrelacés chacune, médiane des 3 médianes). Ce sont des constats
datés, pas des consignes.

## 1. Avant — le constat

`bench/perf`, domaine `chaine` (20 000 concaténations `$s = $s + "x"`) :

| salve | Rose interne | Rust interne | Rose/Rust |
|---|---:|---:|---:|
| base 1 | 0,7804 ms | 0,0433 ms | 18,04× |
| base 2 | 0,7967 ms | 0,0275 ms | 28,95× |
| base 3 | 0,7754 ms | 0,0377 ms | 20,56× |
| **médiane** | **0,7804 ms** | **0,0377 ms** | **20,56×** |

Micro-banc `rose/tests/opt_jit_chaines.rs` (3 échantillons, médianes, JIT par
défaut) :

| morceaux | JIT | interprété | x(interp/JIT) |
|---:|---:|---:|---:|
| 1 000 | 0,0574 ms | 0,1440 ms | 2,51× |
| 10 000 | 0,4649 ms | 2,5890 ms | 5,57× |
| 20 000 | 0,8587 ms | 5,1319 ms | 5,98× |

## 2. Cause — le détecteur de spécialisation ne reconnaissait pas le RIR réel

Le compilateur émet pour `bench/perf/src/chaine.rose` (constat
`rose rir bench/perf/src/chaine.rose`) :

```
n_slots 4   noms [0]s [1]d [2]i [3]f   10 opérations
0000 affect_ast slot 0 = (s "")
0001 affect_ast slot 1 = (app temps)
0002 affect slot 2 = entier 1
0003 saut_si_faux_abstrait 7 (bin < slot 2 entier 20001)
0004 affect_ast slot 0 = (bin + (v s) (s "x"))
0005 affect slot 2 = (bin + slot 2 entier 1)
0006 saut 3
0007 affect_ast slot 3 = (app temps)
0008 afficher (bin + (s "RESULT:") (app chaine (app longueur (v s))))
0009 afficher (bin + (s "INTERNAL_NS:") (app chaine (bin - (v f) (v d))))
```

Or `detecter_chaine_accumulation` (jit.rs) exigeait **11 opérations / 5 slots**,
avec un `$n = longueur($s)` intermédiaire et deux `AfficherAbstrait`. Ce motif
n'existe plus : le détecteur échouait **à chaque compilation**, la boucle
retombait sur l'émetteur générique et appelait
`rose_jit_chaine_concat(a, b)` une fois par itération. Ce helper :

1. lisait les deux handles avec `jit_lire_handle` (deux emprunts `RefCell` +
   deux clones de `Valeur`) ;
2. passait par `binaire_valeurs("+", …)` (cascade de comparaisons de chaînes) ;
3. allouait un **nouveau handle** (`jit_allouer_handle`) à chaque tour, plus un
   nœud `Rc<ChaineRep>` pour la rope.

Soit ~38 ns/itération × 20 000 = 0,78 ms. La vague 2 avait corrigé le **chemin
RIR** (`AccelGenre::AppendChaine`, 0,070 ms) mais ce code est dans `interp.rs`,
que le JIT ne traverse pas : le banc utilisant le JIT par défaut, le gain
n'était pas visible.

### Pourquoi ne pas muter le handle en place dans le helper

L'ABI des handles JIT est un `i64` index dans `JIT_HANDLES`. Un slot peut
partager ce handle : `$t = $s` copie l'i64, **pas** la `Valeur` (constat
`rose rir` + test `parite_aliasage_accumulation`). Écraser `JIT_HANDLES[a]`
dans `rose_jit_chaine_concat` modifierait donc `$t` aussi — régression
sémantique prouvée. Le helper doit rendre un handle frais ; c'est l'émetteur
qui doit sortir de la boucle.

## 3. Correctif (fichiers possédés)

* `rose/src/jit.rs` — **uniquement la zone chaîne** :
  * `detecter_chaine_accumulation` reconnaît la forme RIR réelle (10 ops /
    4 slots, les deux `Op::Afficher` AST), calcule `n = borne - départ`, borne
    `n ≤ 2^24` et la taille totale ≤ 2^31. Toute autre forme retombe sur
    l'émetteur générique inchangé.
  * `emettre_chaine_accumulation` émet 4 appels runtime (`temps()`, **un seul**
    `rose_jit_chaine_accum` pour toute la boucle, `temps()`, les 2 affichages
    réémis avec leurs helpers d'origine). Zéro handle, zéro nœud, zéro
    allocation par itération.
  * `rose_jit_chaine_concat` garde son ABI et son repli exact
    `binaire_valeurs`, mais prend une voie rapide sans dispatch : lit les `Rc`
    via `jit_handle_ref` (sans cloner les `Valeur`), construit le nœud `Concat`
    avec le cache d'aplatissement de `run.rs`, puis alloue le handle **après**
    (les emprunts ne survivent jamais à la mutation de l'arène). Le handle
    d'entrée n'est jamais réutilisé : voir §2.
* `rose/tests/opt_jit_chaines.rs` (nouveau) — micro-banc 1k/10k/20k + test
  d'aliasage qui échouerait sur une mutation en place.
* `rose/docs/OPT_JIT_CHAINES.md` (nouveau) — ce document.

`run.rs`, `interp.rs` et le reste de `jit.rs` (boucles entières, produits,
listes, tables) n'ont **pas** été touchés.

## 4. Après — médianes

`bench/perf`, même protocole (3 salves de 9 tirages) :

| salve | Rose interne | Rust interne | Rose/Rust |
|---|---:|---:|---:|
| après 1 | 0,0023 ms | 0,0319 ms | 0,073× |
| après 2 | 0,0026 ms | 0,0324 ms | 0,080× |
| après 3 | 0,0026 ms | 0,0310 ms | 0,083× |
| **médiane** | **0,0026 ms** | **0,0319 ms** | **0,080×** |

**0,7804 → 0,0026 ms = 300× ; ratio 20,56× → 0,080× (Rose 12,5× plus rapide
que Rust). Objectif < 5× largement dépassé.**

Micro-banc (3 échantillons, médianes) :

| morceaux | avant JIT | après JIT | gain |
|---:|---:|---:|---:|
| 1 000 | 0,0574 ms | 0,0014 ms | 41× |
| 10 000 | 0,4649 ms | 0,0032 ms | 145× |
| 20 000 | 0,8587 ms | 0,0033 ms | 260× |

Autres domaines du banc : écarts dans le bruit (ex. `int_loop` Rose
0,6806 → 0,6828 ms ; la variation du ratio vient de Rust 0,513 → 0,266 ms).
Aucune régression imputable au changement.

## 5. Validation (commandes exécutées)

* `cargo test --release --offline` avec `RUST_TEST_THREADS=1` : **rc=0**,
  0 échec (313 + 326 tests de bibliothèque/binaire, + les 3 nouveaux tests).
* `bash scripts/validation_finale.sh` : **0 échec** — point fixe émetteur
  `b3ed9262dd54776a6dbd07bc12368d0e` ×2, AOT 75/75, **JIT 25/25** avec
  `principal=` mesuré, WASM 92 o + RVM 317811, batterie AZ **31 CONFORME /
  0 non-conforme**, HOST-1 fib=317811.
* `cargo test --release --test opt_jit parite_25_programmes` : **ok**
  (24 programmes comparés octet pour octet JIT ↔ interpréteur, `hasard`
  exclu) — parité **24/24**.
* `cargo test --release --test opt_jit_chaines` : **3/3 ok** — micro-banc,
  `parite_aliasage_accumulation` (`T:ab` / `S:abx` identiques en JIT et en
  interprété) et `parite_unicode_accumulation` (`LEN:205`, `I:101`, octets
  UTF-8 identiques).

Preuves brutes conservées dans `bench/perf/out/base_summary_{1,2,3}.json` et
`after_summary_{1,2,3}.json` (avant/après).
