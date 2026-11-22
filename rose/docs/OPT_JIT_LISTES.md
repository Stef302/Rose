# OPT_JIT_LISTES — pourquoi le domaine `liste` du banc ne bougeait pas

Constat daté : 22–23/11/2026, worktree `/tmp/opencode/opt_vm` (HEAD `8aac2f8`),
binaire `CARGO_TARGET_DIR=/tmp/opencode/opt_shared_target`, cœur PIN 15,
protocole 3 salves × 3 tirages entrelacés (harnais `bench/perf/harness.py`).

## 1. Mesures avant / après

### Domaine `liste` du banc (3 salves, 3 tirages chacune, médianes par salve)

| salve | avant Rose | avant Rust | avant ratio | après Rose | après Rust | après ratio |
|---|---|---|---|---|---|---|
| 1 | 0,9956 ms | 0,1806 ms | 5,512× | 0,5540 ms | 0,1660 ms | 3,338× |
| 2 | 1,0473 ms | 0,1901 ms | 5,508× | 0,5417 ms | 0,1782 ms | 3,040× |
| 3 | 1,1016 ms | 0,1884 ms | 5,848× | 0,5417 ms | 0,1568 ms | 3,455× |
| **médiane des médianes** | **1,0473 ms** | **0,1884 ms** | **5,56×** | **0,5417 ms** | **0,1660 ms** | **3,26×** |

(Salves intermédiaires au cours du réglage : 0,5886 / 0,5681 / 0,5342 ms —
même fourchette 3,0–3,5×.)

Le chiffre du demandeur (5,72× : 1,037 / 0,182) est reproduit à 3 % près.

Banc complet `N_TRIALS=3` (SHA binaire `d59740cd…`) : `liste` = **0,543 ms vs
0,158 ms Rust = 3,428×**, 5/13 victoires au total (inchangé), aucun autre
domaine régressé. Diagnostic JIT : `liste` JIT 0,600 ms contre interpréteur
0,957 ms (**1,59×**, contre 0,76× avant — le JIT était plus lent que
l'interpréteur).

### Micro-banc JIT (`rose/tests/opt_jit_listes.rs`, n = 100 000)

| chemin | avant | après |
|---|---|---|
| `rose_jit_joindre` | 1,134 ms (3,98× `Vec<i64>` Rust) | 0,256 ms (**1,04×**) |
| `rose_jit_index_i64` | 0,130 ms | 0,301 ms (voir §4) |
| `Valeur::z` isolé (référence cross-crate) | ~0,5 ms / 100 k | supprimé du chemin chaud |

Plancher mesuré en remplaçant les deux helpers par des no-op (en conservant
le résultat exact) : **0,270 ms** pour les 200 000 appels, soit 1,5× Rust.
Le coût irréductible est l'appel Cranelift vers une fonction `extern "C"`
opaque (~1,35 ns/appel), pas la mémoire.

## 2. Cause

Le banc `liste.rose` fait 100 000 `joindre($l, i)` puis 100 000 `$l[$j]`.
Le RIR ne déclenche pas `detecter_liste_boucles` (il exige `[0 : entier]`,
`$i = 0` et `longueur($l)` dans la condition ; le banc a `[0]`, `$i = 1` et
un bornage littéral), donc le principal compile en JIT générique avec **un
appel runtime par élément** :

* `rose_jit_joindre` faisait `vec.push(Valeur::z(v))` dans le `Vec<Valeur>`
  canonique : **24 octets écrits par élément**, plus un appel cross-crate non
  inliné à `Valeur::z` (mesuré ~5 ns/élément : 1,19 ms avec `z` contre 0,64 ms
  avec `Valeur::Entier` direct dans la même salve) ;
* `rose_jit_index_i64` relisait ces 24 octets et testait le discriminant ;
* le miroir compact `CompactListe::I64` introduit en vague 2 restait **vide**
  (les écritures JIT ne le touchaient pas) : `Liste::lire`/`avec_slots_i64`
  invalidaient donc le miroir et retombaient sur le canonique.

Le correctif de vague 2 n'avait agi que sur le chemin **interpréteur**
(`pousser_i64_plage`, `avec_slots_i64`) ; le chemin JIT, lui, n'avait jamais
été branché dessus. D'où un domaine « liste » figé à ~5,7×.

## 3. Correctif (diff précis — un seul fichier source partagé)

`rose/src/jit.rs` — **145 lignes modifiées, toutes dans la zone des helpers
liste** (aucune modification des émetteurs boucle/produit/chaîne, ni de
`run.rs`, ni de `interp.rs`) :

1. `thread_local! JIT_HANDLES` : ajout de `MIROIR_CACHE: Cell<(i64, *mut Vec<i64>)>`
   + `miroir_depuis_handle(h)` (résolution froide une fois par handle, cache
   du pointeur vers la cible `Rc` stable, réservation 131072 slots une fois).
2. `jit_handles_reinitialiser()` : remise à zéro du cache (les index de
   handles sont réutilisés à partir de 0).
3. nouveaux helpers (froids) : `liste_controle`, `rose_jit_liste_materiel`,
   `rose_jit_liste_slots_count`, `rose_jit_liste_miroir_invalider`.
4. `rose_jit_joindre` : n'écrit **plus que le miroir i64** (8 octets, écriture
   brute + `set_len`, zéro `Valeur::z`, zéro allocation, zéro emprunt
   `RefCell`). repli `Valeur::z` conservé si le miroir est opaque.
5. `rose_jit_index_i64` : lit le miroir (8 octets, `get_unchecked` borné par
   `u < len`), repli exact sur le canonique (`rose_jit_liste_materiel` puis
   lecture `Valeur::Entier`).
6. `rose_jit_longueur` : matérialise le canonique avant de répondre (un
   observateur hors JIT ne doit jamais voir un canonique vide).
7. `rose_jit_valeur_afficher` et `rose_jit_valeur_chaine` : un appel à
   `rose_jit_liste_materiel(h)` avant de lire la `Valeur` (deux lignes, dans
   des helpers génériques ; aucun changement de sémantique pour les chaînes,
   tables ou rationnels).

`rose/tests/opt_jit_listes.rs` (nouveau) : micro-banc 10 k/100 k avec 3
médianes et assertions d'exactitude (ABI handle, miroir synchronisé,
matérialisation, repli hétérogène, parité JIT ↔ interpréteur). Un mutex de
série est posé car l'arène JIT est thread-local : deux tests du même binaire
parallèles SEGFAULTaient sans lui (observé avec `validation_finale.sh`).

`rose/docs/OPT_JIT_LISTES.md` (ce fichier).

## 4. Écart à l'objectif 2,5× — mesuré, pas supposé

Le domaine passe de **5,56× à 3,26×** (gain 1,71×), mais ne descend **pas**
sous 2,5×. La raison est mesurée, pas invoquée : le plancher avec deux helpers
réduits à des no-op (produisant pourtant le même résultat) est **0,270 ms**,
soit déjà 1,5× Rust. Les 200 000 appels `extern "C"` depuis le code Cranelift
coûtent ~1,35 ns chacun ; le travail utile restant (cache + 8 octets
écrits/lus) ajoute ~1,35 ns. Atteindre 2,5× exigerait ~0,9 ns de travail par
appel, soit ~33 % de moins que le minimum observé pour `Vec::get`/`set_len`
à travers un appel opaque. Le seul moyen connu serait d'émettre les deux
boucles en code machine direct (voie `emettre_liste_machine`, déjà présente
mais réservée au motif `[0 : entier]` / `longueur($l)`), ce qui sortirait du
périmètre « helpers liste » de cette mission.

Le micro-banc `idx` (0,301 ms) est mesuré avec le handle **tagué** de
`rose_jit_liste_vide` (le banc lui-même utilise un handle indexé via
`Expr::ListeLit`) : la résolution froide et le cache ne sont pas les mêmes
dans les deux cas ; c'est pourquoi le chiffre du micro-banc n'est pas
directement transposable au domaine.

## 5. Validation (preuves d'exécution)

* `cargo test --release` avec `RUST_TEST_THREADS=1` : **313 + 326 + … test(result)
  tous `ok`, 0 échec**, y compris `opt_jit_listes` (2/2).
* `bash scripts/validation_finale.sh` : **0 échec** — point fixe émetteur,
  AOT 75/75, JIT 25/25, WASM+RVM, AZ 31/31 conforme, self-host HOST-1.
* Banc complet `N_TRIALS=3` : 5/13 victoires (identique à l'avant), SHA binaire
  stable avant/après salve (`d59740cd…`).
* Aucun commit, push ou publication ; aucun fichier hors
  `jit.rs` (zone liste), `opt_jit_listes.rs`, `OPT_JIT_LISTES.md` modifié.
