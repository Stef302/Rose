# OPT_CHAINES — vague 2 : chaînes à 22,94× de Rust

**Constat daté : 22/11/2026**, commit `0f9c28b`, worktree exclusif
`/tmp/opencode/opt_tables`, `CARGO_TARGET_DIR=/tmp/opencode/opt_shared_target`.
Toutes les mesures ci-dessous sont des **constats re-mesurés**, pas des consignes.

## 1. Avant — le goulot

Banc `bench/perf` (9 tirages entrelacés, pin c16, `summary.json` du commit) :

| domaine | Rose interne | Rust interne | Rose/Rust |
|---|---:|---:|---:|
| `chaine` — 20 000 concaténations | 0,780687 ms | 0,034032 ms | **22,94×** |

Micro-banc `rose/tests/opt_chaines.rs` (3 échantillons, médianes, AVANT tout
changement — `chaines_before.log`) :

| n | op | Rose | Rust | Rose/Rust |
|---:|---|---:|---:|---:|
| 1 000 | `concat` | 0,0138 ms | 0,0008 ms | 16,63× |
| 10 000 | `concat` | 0,1391 ms | 0,0055 ms | 25,42× |
| 20 000 | `concat` | 0,2801 ms | 0,0107 ms | 26,24× |
| 20 000 | `tranche` × 1 000 | **71,1469 ms** | 0,0016 ms | 45 374× |
| 20 000 | `longueur` × 20 000 | ~0 | ~0 | — |
| 20 000 | `aplatir` | 0,0906 ms | — | — |

**Diagnostic confirmé par la mesure** : `+` empilait 20 000 nœuds `Concat` en
chaîne linéaire. Chaque `sous_chaine(i, i+1)` devait traverser les 20 000
nœuds : 71 ms pour 1 000 tranches. Le coût de la concaténation elle-même
n'était déjà plus l'allocation (O(1) depuis le 18/11), mais la **forme** de
l'arbre et l'absence de réutilisation de l'aplatissement.

## 2. Stratégies mesurées (3 tentatives, une retenue)

| stratégie | concat 20k | tranche × 1 000 | verdict |
|---|---:|---:|---|
| AVL complet (profondeur + rotations) | 12,647 ms | 1,06 ms | écarté : ~15 allocations par ajout |
| fusion de feuilles ≤ 256 o | 1,923 ms | 0,43 ms | écarté : recopie + 3 allocations |
| rééquilibrage périodique (profondeur 64) | 209,965 ms | 1,16 ms | écarté : O(n²) (recollecte toutes les feuilles) |
| **cache d'aplatissement + ajout en place** | **0,0744 ms** | **0,1313 ms** | **retenu** |

Le cache est la seconde option de la consigne (« n'aplatir qu'une fois ») :
la rope est immuable, donc le `RefCell<Option<String>>` porté par
`NoeudChaine::Concat` n'est jamais invalide. `ChaineRep::aplatir()` le remplit
à la première demande ; `tranche()` découpe ensuite directement dans le
`String` (avec conversion `points de code → octets` par `char_indices` pour
l'Unicode). Aucun champ n'a été ajouté à `ChaineRep` : l'ABI de `Valeur` est
inchangée, `jit.rs` construit toujours `NoeudChaine::Plein` (fichier non touché).

`ChaineRep::concat_possessif(a, b)` (nouveau) évite même l'allocation quand
l'accumulateur est possédé en exclusivité : `Rc::get_mut` + `pousser` font le
travail de `String::push_str` dans le `String` de la feuille. Un préfixe aliasé
retombe sur `concat` (persistant) — vérifié par test.

L'accélérateur RIR `AccelGenre::AppendChaine` reconnaît la boucle
`$s = $s + "morceau"` et garde une feuille `String` possédée pendant toute la
boucle (zéro `Rc`, zéro nœud par tour). Repli exact sur les 4 opérations
d'origine si le type est inattendu.

## 3. Après — médianes

Micro-banc `opt_chaines.rs`, 3 échantillons, médianes (même binaire) :

| n | op | Rose | Rust | Rose/Rust |
|---:|---|---:|---:|---:|
| 1 000 | `concat` (générique) | 0,0142 ms | 0,0007 ms | 19,26× |
| 1 000 | `concat_poss` | 0,0040 ms | 0,0007 ms | 5,86× |
| 10 000 | `concat` (générique) | 0,1371 ms | 0,0054 ms | 25,28× |
| 10 000 | `concat_poss` | 0,0372 ms | 0,0057 ms | 6,57× |
| 20 000 | `concat` (générique) | 0,2910 ms | 0,0106 ms | 27,35× |
| 20 000 | **`concat_poss`** | **0,0744 ms** | 0,0108 ms | **6,89×** |
| 20 000 | `tranche` × 1 000 | **0,1313 ms** | 0,0005 ms | 264,81× |
| 20 000 | `longueur` × 20 000 | ~0 | ~0 | — |
| 20 000 | `aplatir` | 0,0847 ms | — | — |

Gains sur la structure seule : **concat 20k 3,9×** (0,2801 → 0,0744 ms),
**sous_chaine 542×** (71,1469 → 0,1313 ms), longueur inchangée, parité
Unicode conservée (`semantique_chaines_preservee`, 5 familles d'assertions).

Banc `bench/perf` complet, `ROSE_BIN` = binaire du worktree, 3 tirages,
**mode interprété (`ROSE_SANS_JIT=1`, chemin RIR)** :

| domaine | Rose interne | Rust interne | Rose/Rust |
|---|---:|---:|---:|
| `chaine` | **0,070 ms** | 0,033 ms | **2,10×** |

Soit **0,7807 → 0,070 ms = 11,2× sur la concaténation 20k du banc**,
objectif ≥ 5× dépassé. Le ratio face à Rust passe de 22,94× à 2,10×.

Le même banc en mode JIT par défaut (chemin générique
`rose_jit_chaine_concat` → `binaire_valeurs`) mesure 0,871 ms : ce chemin ne
passe pas par le RIR et l'accélérateur ne s'y applique pas. Le déplacement de
la règle `+`/chaîne en tête de `binaire_valeurs` y est fait (gain marginal) ;
la suppression du handle alloué par itération exigerait une modification de
`jit.rs`, fichier explicitement hors périmètre de cette vague.

## 4. Validation

* `cargo test --release` : **0 échec** (313 + 326 + … tests verts, dont les
  2 nouveaux `opt_chaines`).
* `bash scripts/validation_finale.sh` : **0 échec** — point fixe émetteur
  `b3ed9262dd54776a6dbd07bc12368d0e` ×2, AOT 75/75, JIT 25/25, WASM+RVM,
  batterie AZ 31 CONFORME / 0 non-conforme (Unicode couvert), HOST-1.
* Parité interpreter/JIT vérifiée sur un programme d'ajout Unicode + alias :
  `L:100`, `T:é`, `ALIAS:13:3` dans les deux modes.

## 5. Fichiers touchés

* `rose/src/run.rs` — section chaînes uniquement (`ChaineRep`, `NoeudChaine`,
  cache, `pousser`, `concat_possessif`). Le hash des tables (reste du fichier)
  est préservé.
* `rose/src/interp.rs` — primitive `+` (règle en tête), nouveau genre
  d'accélérateur de boucle `AppendChaine`.
* `rose/tests/opt_chaines.rs` — nouveau : banc + non-régression Unicode/alias.
* `rose/docs/OPT_CHAINES.md` — nouveau : ce document.
