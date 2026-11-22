# Vague OPT_LISTES — `slots i64` contigus pour les listes d'entiers

**Constat daté : 22/11/2026, 20:59–21:20 UTC.** Toutes les mesures ci-dessous
sont des constats à cet horodatage, pas des consignes : re-mesurer après tout
changement de moteur, de JIT, de quantification ou de topologie. Pinage
`taskset -c 15`, 5 échantillons entrelacés base/final pour le programme
complet, 3 échantillons par point et médianes pour le micro-banc interne.
Aucune mesure concurrente lourde pendant les salves.

- Arbre de départ : worktree `/tmp/opencode/opt_vm`, HEAD `0f9c28b`
  (`Banc après optimisations : 4/13 victoires`).
- Binaire de base : `/tmp/opencode/rose_base_bin`, SHA-256
  `a898bbd3378aa508f290e43ca008abf1444b0f241a3152d895cb83580ac137ac`
  (reconstruit depuis les sources `run.rs`/`interp.rs` d'origine, dans un
  `CARGO_TARGET_DIR` séparé).
- Périmètre : **`rose/src/run.rs` (partie LISTE uniquement),
  `rose/src/interp.rs` (primitives `joindre`/`idx`/`longueur` + boucles
  accélérées de liste), `rose/tests/opt_listes.rs` (nouveau), ce doc
  (nouveau)**. Aucun `.rs` hors périmètre, aucun `Cargo.toml`,
  aucun `jit.rs`, aucun commit, aucune publication.
- Conflit parallèle : un autre agent modifie `run.rs` (chaînes) et
  `interp.rs` (`+`, `sous_chaine`) dans SON worktree. Les hunks de cette
  mission sont tous dans les zones LISTE ; aucun hunk ne touche une chaîne.

---

## 1. Diagnostic — où est réellement le coût

Le bench `bench/perf/src/liste.rose` fait 100 000 `joindre` puis 100 000
lectures indexées. Sa RIR est :

```
0000 l = (lst (n 0))              0008 j = 0
0001 d = temps()                  0009 saut_si_faux 13 (j < 100001)
0002 i = 1                        0010 s = s + l[j]
0003 saut_si_faux 7 (i < 100001)  0011 j = j + 1
0004 joindre(l, i)                0012 saut 9
0005 i = i + 1                    0013 f = temps()
0006 saut 3                       0014/15 afficher RESULT / INTERNAL_NS
0007 s = 0
```

Deux constats mesurés qui corrigent la prémisse initiale :

1. **La voie JIT et la voie interprétée sont très différentes.** Instrumentation
   (`ROSE_PROBE_ACCEL`, retirée du binaire final) : avec JIT, `exec_boucle`
   n'est **jamais** appelé ; sans JIT, il l'est deux fois. La boucle `joindre`
   du JIT passe par `rose_jit_joindre` (dans `jit.rs`, hors périmètre), celle
   de l'interpréteur par `AccelGenre::Liste`.
2. **Le goulot de la voie interprétée est `idx`, pas `joindre`.**
   Décomposition `probe_idx.rose`, médianes de 5, c15 :

   | Phase | JIT base | interpréteur base |
   |---|---:|---:|
   | 100 k `joindre` | 0,707 ms | 0,707 ms |
   | 100 k `l[j]` + somme | 0,279 ms | **14,079 ms** |
   | programme complet | 1,026 ms | **14,925 ms** |

   Le coût interprété de l'index est de **141 ns/élément**, contre 2,8 ns
   sous JIT : `indexer_valeur` allouait un `Gz` (`vers_z`) pour la clé, puis
   lisait et clonait une `Valeur` de 24 octets dans le `Vec` canonique.

---

## 2. Décision — un miroir compact, jamais une représentation concurrente

`Valeur::Entier(v, None)` occupe 24 octets dans le `Vec` canonique ; 16 octets
(deux mots de glue/discriminant) sont identiques pour tous les éléments. Le
miroir `CompactListe::I64(Vec<i64>)` ne garde que les `i64` : **8 octets/élément**.

⚠ **Le `Vec<Valeur>` canonique reste la source de vérité unique.** Le JIT y
écrit directement (`liste_vec_nv`, `rose_jit_joindre`), et `chaine_de_p` /
`valeur_egale` le lisent. Le miroir est un accélérateur de lecture, pas une
copie divergente : il est invalidé (`Opac`) dès qu'un élément n'est pas un
entier nu, et toute lecture vérifie `miroir.len() == canonique.len()` avant de
s'en servir. En cas de doute on relit le canonique. Aucune synchronisation,
aucune conversion, aucune divergence possible.

**ABI Valeur préservée** : `Liste` passe de 8 à 16 octets (deux `Rc`), mais
`size_of::<Valeur>() == 24` est **asserté** dans `opt_listes.rs` et vérifié
par `cargo test --release`. `jit.rs` n'a pas été modifié.

### Optimisations retenues, toutes mesurées

| # | Changement | Fichier | Gain mesuré |
|---|---|---|---|
| 1 | Miroir `Vec<i64>` + `Liste::lire` (8 octets lus au lieu de 24) | `run.rs` | idx direct 1,20× ; avec la superinstruction : **476×** |
| 2 | Superinstruction `ListeSomme` : `$s = $s + $liste[$j]` reconnue dans le RIR et exécutée en Rust natif sur le miroir | `interp.rs` | 14,079 → 0,0296 ms |
| 3 | Boucle `joindre` accélérée remplit le miroir dans la même passe (`pousser_i64_plage`) | `run.rs`/`interp.rs` | aucune régression (0,707 → 0,669 ms) |
| 4 | `indexer_valeur` : clé `i64` nu directe, plus d'allocation `Gz` | `interp.rs` | compris dans #1/#2 |
| 5 | `longueur` O(1) sur le miroir | `interp.rs` | 0,341 → 0,137 ms (2,49×) |

### Repli exact

Le `ListeSomme` retombe sur l'exécution opération-par-opération si : la liste
est hétérogène, le miroir est désynchronisé (écriture JIT directe), la borne
dépasse la longueur, l'accumulateur n'est pas un `i64` nu, ou la somme déborde
(`checked_add`). Le repli réécrit l'accumulateur ; la valeur lue après repli
est renvoyée pour que l'`AffectAst` appelant ne l'écrase pas avec `Vide`.

---

## 3. Résultats — programme complet (5 entrelacés, c15)

| Voie | base | final | speedup | résultat |
|---|---:|---:|---:|---|
| **JIT** (`bench/perf` tel quel) | 1,026 ms | 1,028 ms | 1,00× | `5000050000` |
| **Interpréteur** (`ROSE_SANS_JIT=1`) | 14,925 ms | **0,755 ms** | **19,76×** | `5000050000` |

Échantillons interpréteur : base `15,933 / 15,166 / 14,900 / 14,925 / 14,839`,
final `0,755 / 0,786 / 0,809 / 0,727 / 0,735` ms. Même résultat octet pour
octet aux 10 tirages. Objectif **≥2× sur construction/idx 100k : atteint**
(19,76× sur le programme qui fait exactement ces deux opérations).

Décomposition finale de la phase idx : **14,079 → 0,0296 ms = 476×**.
Le coût résiduel du programme interprété (0,755 ms) est presque entièrement
dans les 100 000 `joindre` (0,669 ms, inchangé : le canonique 24 octets reste
écrit, par contrat avec le JIT).

**Voie JIT inchangée — constat factuel, pas une excuse.** Les helpers
`rose_jit_joindre` / `rose_jit_index_i64` sont définis dans `jit.rs`
(appartenant à un autre agent) et écrivent/lisent directement le `Vec<Valeur>`
canonique. Ce mission n'a pas modifié `jit.rs` (consigne explicite). Pour que
la voie JIT profite du miroir, il faut que ces helpers appellent
`Liste::pousser_i64` / `Liste::avec_slots_i64` — une intégration `jit.rs`
hors périmètre. Le gain mesuré ci-dessus porte sur la voie interprétée, qui
est celle que les fichiers possédés (`run.rs` LISTE, `interp.rs` primitives)
pilotent réellement.

---

## 4. Micro-banc interne (3 médianes, `cargo test --release --test opt_listes`)

Comparaison du comportement d'avant optimisation (colonne `canonique` : miroir
volontairement laissé vide) et du comportement final (`compact`), dans le
MÊME binaire, contre `Vec<i64>` Rust :

| n | opération | canonique | compact | gain | Rust | compact/Rust |
|---:|---|---:|---:|---:|---:|---:|
| 10 k | construction | 0,046 | 0,024 ms | 1,92× | 0,008 | 0,33 |
| 10 k | idx i64 | 0,088 | 0,073 ms | 1,21× | 0,005 | 0,07 |
| 10 k | longueur | 0,034 | 0,014 ms | 2,43× | 0,005 | 0,38 |
| 100 k | construction | 0,398 | 0,248 ms | 1,60× | 0,126 | 0,51 |
| 100 k | idx i64 | 0,886 | 0,741 ms | 1,20× | 0,051 | 0,07 |
| 100 k | longueur | 0,341 | 0,137 ms | **2,49×** | 0,051 | 0,38 |
| 100 k | somme indexée | 1,424 | 1,277 ms | 1,12× | 0,026 | 0,02 |

Dans ce micro-banc, l'appel à `indexer_valeur` (construction de la `Valeur`
clé, deux emprunts `RefCell`) domine largement la lecture mémoire : le gain
de la representation compacte seule est donc ~1,2×, et c'est la
superinstruction RIR (#2) qui débloque le ×476 du programme réel.

---

## 5. Exactitude et validation (preuves d'exécution)

`export CARGO_TARGET_DIR=/tmp/opencode/opt_shared_target`,
garde-fou `bash scripts/garde_fou_mem.sh 12000000 …`.

- `cargo test --release` : **313/0, 326/0, 0/0, 6/0, 1/0, 0/0**, rc=0.
- `cargo test --release --test opt_listes -- --nocapture --test-threads=1` :
  **2 passed ; 0 failed** (`bench_listes`, `semantique_liste_preservee`).
- `bash scripts/validation_finale.sh` : **0 échec**. Détail :
  tests rc=0 ; point fixe `b3ed9262dd54776a6dbd07bc12368d0e` ×2 ;
  AOT **75/75**, zéro refus ; JIT **25/25** Cranelift ; WASM 92 o ;
  RVM `317811` ; AZ **31 CONFORME / 0 non-conforme** ;
  HOST-1 fib `317811`.
- `opt_listes.rs::semantique_liste_preservee` vérifie : liste hétérogène
  (entiers / chaîne / booléen / 9 007 199 254 740 993), index hors limites
  signalé, alias partagé, écriture directe dans le canonique (miroir
  désynchronisé → repli sur le canonique), ajout après désynchronisation,
  et `size_of::<Valeur>() == 24`.
- Le programme du bench donne `RESULT:5000050000` sur les 10 tirages A/B,
  en mode JIT comme en mode interprété.

---

## 6. Artefacts et diffusion

- Binaire de base : `/tmp/opencode/rose_base_bin` (sha ci-dessus).
- Sources de base : `/tmp/opencode/run.rs.BASE`, `/tmp/opencode/interp.rs.BASE`.
- Harnais : `rose/tests/opt_listes.rs` ; sonde `probe_idx.rose` dans
  `/tmp/opencode/`.
- Aucun commit, aucune publication, aucun push, aucun fichier modifié hors
  des 4 fichiers annoncés.

## 7. Pour l'orchestrateur — diff précis à intégrer

Le diff complet est `git diff -- rose/src/run.rs rose/src/interp.rs` plus le
fichier nouveau `rose/tests/opt_listes.rs`. Il est **additif** dans les zones
LISTE : aucun hunk ne touche une chaîne (`ChaineRep`, `concatener`,
`sous_chaine`, opérateur `+` sur chaînes) ni une table (`TableRep`). Les
seules lignes MODIFIÉES hors ajout sont :

1. `run.rs:86` — `pub struct Liste(Rc<...>)` → `Liste(Rc<...>, Rc<RefCell<CompactListe>>)`
   + le bloc `impl Liste` qui suit (nouveau).
2. `run.rs` `Valeur::liste()` — délègue à `Liste::neuf()`.
3. `run.rs` `Valeur::liste_de()` — construit le miroir en fin de fonction.
4. `interp.rs` `enum AccelGenre` — ajout de `ListeSomme`.
5. `interp.rs` `detecter_accel_boucle` — nouvelle branche `+` / `Index`.
6. `interp.rs` `preparer` (remplacement des boucles) — `cible` = `slot_acc`
   pour `ListeSomme`.
7. `interp.rs` `exec_boucle` — `AccelGenre::Liste` utilise
   `pousser_i64_plage` ; nouveau bras `ListeSomme`.
8. `interp.rs` primitive `longueur` — `Valeur::Liste(_) => longueur_valeur(…)`.
9. `interp.rs` primitive `joindre` — délègue à `joindre_liste`.
10. `interp.rs` — ajout de `joindre_liste`, `longueur_valeur`, passage de
    `indexer_valeur` en `pub` + voie rapide clé i64.

Un hunk concurrent de l'autre agent sur `interp.rs` dans la branche `Chaine`
de `detecter_accel_boucle` est inséré **juste après** le hunk #5 (notre
branche `ListeSomme` précède la branche `Chaine` sans la modifier).
