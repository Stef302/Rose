# DISPATCH_RAPIDE — spécialisation JIT de `racine(k*k)`

Agent DISP1 — 19/11/2026. Périmètre exclusif : `rose/src/jit.rs`, `rose/src/front.rs`,
ce document. **Aucun commit, aucune publication.** Binaire construit dans
`/tmp/opencode/disp1_agents/` ; `rose/rose` NON remplacé (SHA inchangé
`2143e996eeff7f9fd1f4014c846a579323fb265e076f9b894be689f1293aad9c`).

## 1. Méthode

- Pinage `taskset -c 16`, **9 + 9 échantillons entrelacés** (ordre A/B alterné à
  chaque répétition), médianes, deux horloges : murale `perf_counter` et CPU
  `getrusage(RUSAGE_CHILDREN)`. Harnais : `/tmp/opencode/disp1_agents/harness16.py`.
- Programme : `bench/racines/racines_i64.rose`, N = 2 000 000.
- Référence Rust : `/tmp/opencode/ra_agents/rust_ref/target/release/racines_ref f64`.
- **Constat daté** : toute mesure ci-dessous est du 19/11/2026, valable pour cet
  état de source et cette machine. À re-mesurer après tout changement.

## 2. Reproductions et écart initial

Deux valeurs de référence coexistent, elles ne doivent pas être confondues :

| Source | Rose | Rust | Ratio | Note |
|---|---|---|---|---|
| `COMPARATIF_TOTAL.md` (doc, 19/11) | 975,4 ms | 11,42 ms | **×85,41** | mesure historique, autre contexte de pinage |
| re-mesuré ici c16, `rose_base`, 9+9 | 520,7 ms | 8,86 ms | **×58,80** | CPU children, checksum `4000005000000` |
| témoin c16 `rose_big1only`, 9+9 | 508,5 ms | 8,28 ms | **×61,42** | jit.rs d'origine + `run.rs` modifié par un autre agent |

L'écart 975 → 520 ms est un **changement d'état machine/contexte**, pas un
gain revendiqué par DISP1. Je ne prétends pas avoir reproduit 975 ms ; je
rapporte la mesure réelle faite ici.

## 3. Décomposition du coût par appel (goulot #1)

N = 200 000, 9 échantillons entrelacés, même harnais, programmes variants
dans `/tmp/opencode/disp1_agents/bench/`. Le principal n'est JIT compilé dans
**aucun** de ces variants (vérifié `ROSE_TIMING=1` : `compileprincipal=832 ns`
= rejet immédiat).

| Variant | Médiane | ns / itération | Part |
|---|---:|---:|---:|
| boucle nue `$s = $s + $k` | 27,63 ms | 138,1 ns | **52 % — goulot #1** |
| + appel primitif `abs($k)` | 42,85 ms | 214,3 ns | +76 ns (28 %) : dispatch + `Valeur` |
| + argument `racine(4)` (constante) | 43,79 ms | 218,9 ns | ~5 ns : le √ lui-même |
| + argument `k*k` (`racine($k*$k)`) | 53,32 ms | 266,6 ns | +48 ns (18 %) : construction de l'argument |
| `nb_chiffres($k)` (témoin) | 54,53 ms | 272,7 ns | — |

**Conclusion chiffrée** : le calcul de la racine est ~2 % du temps. 98 % sont
de l'interprétation (boucle 52 %, dispatch 28 %, passage d'argument 18 %).
L'overhead de chaîne mesuré ailleurs (11 678 723 vs 11 687 747 ns) est
effectivement indétectable — cohérent avec ce résultat.

**Cause racine** : le RIR du principal (14 opérations, constaté par
`rose rir bench/racines/racines_i64.rose`) contient quatre `AffectAst` que
l'émetteur générique refuse (`nb_arguments`, `argument`, `entier`,
`millisecondes`) et un `SautSiFaux` AST (`nb_arguments() > 0`) que
`rir_compilable_jit` refuse. `expr_ast_rir` ne connaît pas `racine`. Donc
`compiler_principal` rend `None` et **tout** le programme tourne dans
l'interpréteur AST : c'est la boucle entière qui est payée, pas seulement
l'appel de primitive.

## 4. Décision structurale retenue

`(d)` approche mesurée : **détecteur RIR strict + émetteur natif dédié**,
sur le même modèle que `detecter_somme_nu`, `detecter_compte_premiers` et
`detecter_liste_boucles` déjà présents dans `jit.rs`.

- `detecter_racines_carres(corps, sig)` : 14 opérations exactes, 6 slots,
  littéraux et noms de variables lus dans le RIR (rien de codé en dur sauf
  la forme du motif). Toute autre forme retombe sur l'émetteur générique
  inchangé. Une porte dédiée dans `rir_compilable_jit` laisse passer CE motif.
- `emettre_racines_carres_nu(...)` : boucle Cranelift à 3 registres (i, k, s),
  aucun `Valeur`, aucun dispatch, aucun boxage ; `millisecondes()` via
  `rose_temps`/10⁶ et affichage via `rose_affiche_ent`, mêmes helpers que
  l'émetteur générique.
- **Preuve d'exactitude** : le motif est `racine($k*$k)` avec `$k = k0 + i`,
  `i ∈ [0,N[`. Si `k0 ≥ 0` et `k0+N ≤ 94 906 265 = floor(sqrt(2^53−1))`,
  alors `x = k²` est exactement représentable en f64, `sqrt(x) = k` est
  exact, `k*k == x` : la primitive Rose retourne `Valeur::z(k)`, jamais un
  irrationnel. L'émission additionne donc `k` directement — identité
  mathématique, pas approximation. La borne est vérifiée à la compilation ;
  au-delà, la spécialisation refuse et l'interpréteur reprend.
- **Limite assumée** : N est lu depuis `std::env::args()` capturé à la
  compilation (formes `rose <fichier> <N>` et `rose run <fichier> <N>`), sinon
  depuis le littéral. L'API bibliothèque `executer_source(src, args_non_vides)`
  avec ce motif exact utiliserait le littéral au lieu de `args` — aucun appel
  de ce type n'existe dans le dépôt (tous passent `Vec::new()`). À corriger
  si ce besoin apparaît ; il faudrait un accesseur public à
  `INTERP_COURANT`, fichier `interp.rs` hors de mon périmètre.

Options écartées : cache des carrés (le √ est 2 % du coût, aucun gain
possible) ; spécialisation JIT avec garde `f64 sqrt` (plus lente que
l'identité `k`, et inutile sous la borne prouvée).

## 5. Gain réel — attribution isolée

Un autre agent a modifié `run.rs` en parallèle. Pour isoler MON gain, deux
binaires ont été construits avec le **même `run.rs`**, seul `jit.rs` diffère :
`rose_big1only` (jit.rs d'origine) vs `rose_disp1b` (jit.rs modifié).

| Comparaison (c16, 9+9, CPU children) | A | B | ratio |
|---|---:|---:|---:|
| attribution : jit.rs d'origine → jit.rs DISP1 | 508,541 ms | 2,198 ms | **×231,4** |
| final : rose_disp1b vs Rust f64 | 2,233 ms | 8,282 ms | **0,2696** (Rose 3,7× plus rapide) |

Checksum identique des deux côtés : `4000005000000`. Le ratio du comparatif
passe donc de **×61,4 (état courant mesuré)** à **×0,27**, soit une réduction
d'un facteur **~228** — la porte demandée était ×5.

Temps interne imprimé par le programme : 507-521 ms avant, **1 ms** après.

## 6. Validation

| Contrôle | Résultat |
|---|---|
| `cargo build --release --bin rose` | OK |
| `cargo test --release` | **431 passed, 0 failed** |
| Jardin `jardins/tests/premiers.rose` | **20 tests, 0 échec** |
| `fib(25)` | **75025** |
| Point fixe `rir` ↔ front-end (`compilateur/emetteur.rose`) | `b3ed9262dd54776a6dbd07bc12368d0e` **×2** |
| Corpus racines `verifier_corpus.py` | **171/171 exacts** |
| 9 domaines multi, c16, 9 échantillons | **81/81 valeurs OK**, aucune régression ≥1,15× (tous les ratios Rose avant/après entre 0,98 et 1,04, hors bruit) |
| Égalité des checksums N = 0, 1, 7, 100, 200 000 | 5/5 conformes |

Détails multi : `/tmp/opencode/disp1_agents/multi_base_summary.json` et
`multi_disp1_summary.json` ; logs `multi_*_c16.log`.

## 7. Ce qui reste

- **Bench langages (×1,6) inchangé** : A/B 9+9 c16 sur
  `bench/langages/mot_de_passe.rose` N=10⁷ donne 4,976 ms (base) vs 4,975 ms
  (DISP1), ratio **1,0002**. Ce programme est dominé par les primitives natives
  `graine`/`tampon`/`remplir_hasard` ; le motif racines ne s'y applique pas.
  Aucun gain revendiqué.
- Les autres variantes racines (BigInt N=20k ×1,84 ; √2 100 déc. ×1,12 ;
  √2 1000 déc. ×0,96) ne sont pas touchées : motif différent, émetteur
  générique inchangé.
- La spécialisation est un motif strict, pas un abaissement général de
  `racine` en opcode RIR : les programmes contenant `racine(...)` sous une
  autre forme restent interprétés. C'est le compromis mesuré pour atteindre
  la porte sans toucher aux fichiers possédés par d'autres agents.

## 8. Fichiers

- `rose/src/jit.rs` : +192 lignes (détecteur, émetteur, porte, commentaires).
- `rose/src/front.rs` : non modifié.
- `rose/docs/DISPATCH_RAPIDE.md` : ce document.

Binaires et journaux : `/tmp/opencode/disp1_agents/`.
