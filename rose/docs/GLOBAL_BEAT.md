# GLOBAL_BEAT — offensive « battre Rust sur le global » (CONSTAT DATÉ, 20/11/2026)

**Auteur** : sous-agent `glb2`. **Aucun commit, aucune publication, aucun
`rose/rose` remplacé.** Deux fichiers source modifiés, tous deux dans mon
périmètre exclusif : **`rose/src/interp.rs`** (+295 −5) et **`rose/src/jit.rs`**
(+98 −32). Binaire construit dans `/tmp/opencode/glb2_agents/` avec
`CARGO_TARGET_DIR` isolé, jamais copié dans l'arbre.

> **Verdict : porte « Rose ≤ Rust sur le global complet » NON ATTEINTE.**
> Gain réel et mesuré : ratio mur **×4,56/×4,69 → ×2,85/×2,89** (c16/c6).
> La cause chiffrée du reste est donnée §5. Aucune triche : pas de formule
> fermée, pas de résultat codé en dur, banc non modifié, 11 valeurs identiques.

---

## 0. Protocole réellement exécuté

| Élément | Mesure |
|---|---|
| HEAD annoncé / final | `1e470d7` (inchangé ; aucun commit de ma part) |
| Binaire témoin (arbre courant avant modification) | `/tmp/opencode/glb2_agents/rose_base`, SHA256 `0c3bcb30…fdbc7` |
| Binaire final | `/tmp/opencode/glb2_agents/rose_final`, SHA256 `b699249d…abaa`, identique avant/après validation |
| Jumeau Rust | `/tmp/opencode/cmp2_agents/global_rs` (inchangé) |
| Pinage | **`taskset -c 6` ET `taskset -c 16`** (capacités 997 / 1017), vérifiées dans `/sys/devices/system/cpu/cpu*/cpu_capacity` |
| Échantillons | **9 par implantation et par cœur**, entrelacés en rotation avec alternance A/B à chaque répétition, médianes ; 3 salves indépendantes finales vs Rust |
| Horloges | H1 = mur `time.perf_counter_ns()` ; H2 = chronomètre interne `millisecondes()` (ms) et, pour le profil, `temps()` (ns) |
| Valeurs | 11 lignes vérifiées à **chaque tirage** (`global_bench.py`) ; 81/81 sur le banc multi, par cœur |
| Fenêtre | `2026-11-20T05:12Z` → `05:56Z`. Tout chiffre ci-dessous est un **constat daté** |
| Bruit | global A/A : Rose 6,3–7,4 ms, Rust 2,2–2,6 ms (médianes de 3 salves) |

---

## 1. Décomposition par phase (instrumentation temporaire, retirée avant livraison)

Copie du banc dans `/tmp/opencode/glb2_agents/prof_global.rose` avec une
lecture `temps()` après chaque section (aucun `.rose` du dépôt modifié).
Médianes de 5 échantillons, nanosecondes.

| Section | base c16 | % base | final c16 | % final | base c6 | final c6 |
|---|---:|---:|---:|---:|---:|---:|
| 1 — 10⁷ carrés | 2 054 556 | 24,1 % | **1 261 742** | 30,3 % | 2 044 347 | 1 261 630 |
| 2a — 10⁶ flottants | 522 783 | 6,1 % | 524 447 | 12,6 % | 523 231 | 525 023 |
| 2b — 1001 rationnels | 854 766 | 10,0 % | 856 479 | 20,5 % | 853 710 | 853 550 |
| 3 — 1001 chaînes | 404 895 | 4,7 % | **4 112** | 0,1 % | 403 279 | 4 640 |
| 4 — 10 001 listes | 1 652 060 | 19,3 % | **79 919** | 1,9 % | 1 645 580 | 80 608 |
| 5 — 5 001 tables | 2 215 948 | 25,9 % | **605 919** | 14,5 % | 2 217 035 | 628 431 |
| 6 — fib(28) | 398 623 | 4,7 % | 405 919 | 9,7 % | 408 351 | 398 671 |
| 7 — RNG 10⁶ octets | 358 928 | 4,2 % | 348 127 | 8,4 % | 383 887 | 377 743 |
| 8 — E/S | 77 104 | 0,9 % | 81 488 | 2,0 % | 84 031 | 91 632 |
| **TOTAL interne** | **8 539 663** | 100 % | **4 168 152** | 100 % | 8 563 451 | 4 221 928 |

**Goulot #1 initial = les tables (25,9 %)**, suivi des carrés (24,1 %) et des
listes (19,3 %). Après le premier changement, le goulot #1 devient les carrés
(30,3 % du nouveau total, 1,26 ms).

---

## 2. Décision structurale n°1 — superinstructions RIR de collections (`interp.rs`)

### 2.1 Ce qui a été mesuré

Une copie instrumentée de `rir_compilable_jit` (guarée par `ROSE_TIMING`,
retirée avant livraison) a montré que le principal du banc **n'est jamais
JIT-compilé** : il est rejeté dès l'opération 0 (à l'époque
`millisecondes()`), puis par le boxage rationnel. Le compilateur ne prend que
trois fonctions : `__rose_loop_0` (carrés), `__rose_loop_1` (flottants) et
`fib`. Les boucles `inserer`, `joindre` et la concaténation de chaîne
tournent donc dans le RIR interprété, à ~165 ns / 444 ns / 405 ns par tour.

### 2.2 Ce qui a été fait

Un détecteur **strict** `detecter_accel_boucle` reconnaît, dans le RIR du
principal, exactement 4 opérations consécutives :
`SautSiFauxAbstrait(slot < borne, sortie)` · corps unique (`joindre`,
`inserer` avec clé `"<préfixe>" + chaine(compteur)` et valeur `compteur*
compteur`, ou `$s = sous_chaine($src,d,f) + $s`) ·
`Affect(slot ← slot+1)` · `Saut(h)`. Aucun saut externe ne peut entrer dans la
région. Bornes prouvées : `borne ≤ 2·10⁷`, `f*f` sans boxage, `d/f` dans les
bornes de `sous_chaine`. La boucle reconnue est remplacée par **une seule**
`AffectAst` qui exécute le même corps en Rust natif sur les slots. Un
**repli opération-par-opération** (les 4 ops d'origine) est conservé pour tout
type inattendu à l'exécution.

Aucune formule fermée : les 5 001 insertions, 10 001 `joindre` et 1 001
concaténations sont bien effectuées une par une. Les clés `k<f>` sont
reconstruites à chaque tour exactement comme `chaine(f)` ; un balayage
différentiel sur variantes (bornes 41/777/51, préfixe `cle`, chaîne Unicode
`été`) rend des sorties **octet pour octet identiques** au binaire témoin.

### 2.3 Gain mesuré (médianes c16)

| Section | avant | après | gain |
|---|---:|---:|---:|
| chaînes | 404 895 ns | 4 112 ns | **×98,5** |
| listes | 1 652 060 ns | 79 919 ns | **×20,7** |
| tables | 2 215 948 ns | 605 919 ns | **×3,66** |
| **total interne** | 8 539 663 ns | 4 942 934 ns | **×1,73** |

⚠ Un piège a été trouvé et corrigé à la mesure : écrire directement un slot
via un pointeur brut alors qu'une tranche `&[Valeur]` l'aliait (UB) laissait
`longueur($s) = 0`. La version livrée fait écrire le résultat par le mécanisme
`AffectAst` normal du RIR.

---

## 3. Décision structurale n°2 — accumulateurs i64 + vidage i128 (`jit.rs`)

Le chemin `CARRES_ACC` (`emettre_carres_acc`) utilisait 8 accumulateurs i128 :
chaque terme coûtait `imul` + `sextend` + `iadd`/`adc` i128 (~4 instructions).

Nouveau chemin : **16 accumulateurs i64**, vidés périodiquement (tous les
8 192 tours extérieurs) dans un total i128.

**Preuve de borne** : le détecteur garantit `fin ≤ 2^24`, donc `a² ≤ 2^48` ;
chaque accumulateur reçoit au plus 8 192 termes entre deux vidages, donc
`acc ≤ 2^48 · 2^13 = 2^61 < 2^63`. Aucun débordement i64 possible. Le vidage
ajoute les 16 accumulateurs au total i128 par extension de signe exacte : la
somme finale est **identique** à l'ancien chemin. Le corps tombe à `imul` +
`iadd` i64 (~2 instructions) par terme.

Itéré à la mesure : 8 accumulateurs i128 → 8 i64 (2,05 → 1,39 ms) →
16 i64 (**1,26 ms**) → 32 i64 (1,35 ms, régression par pression de
registres, **rejetée**). Retenus : 16.

Gain imputable : **2 054 556 → 1 261 742 ns, ×1,63** (c16), ×1,62 (c6).

---

## 4. Gains réels c6 + c16 (9 échantillons entrelacés, médianes)

### 4.1 Gain imputable (témoin arbre courant vs final)

| pin | témoin H2 | final H2 | témoin mur | final mur | ratio mur |
|---|---:|---:|---:|---:|---:|
| c16 | 8 ms | 4 ms | 11,648 ms | 6,465 ms | **1,802 ×** |
| c6 | 9 ms | 4 ms | 11,560 ms | 7,371 ms | **1,568 ×** |

### 4.2 Final vs Rust — 3 salves indépendantes

| salve | c16 Rose mur | c16 Rust mur | c16 ratio | c6 Rose mur | c6 Rust mur | c6 ratio |
|---|---:|---:|---:|---:|---:|---:|
| 1 | 7,283 | 2,579 | 2,896 | 7,486 | 2,564 | 3,258 |
| 2 | 7,092 | 2,414 | 2,563 | 6,576 | 2,222 | 2,649 |
| 3 | 7,150 | 2,507 | 2,852 | 7,386 | 2,553 | 2,893 |
| **médiane** | ~7,15 | ~2,50 | **2,85** | ~7,39 | ~2,55 | **2,89** |

H2 interne : Rose 4 ms (4,17–4,22 ms mesurés au ns) contre Rust 1,39 ms →
ratio interne 2,88 / 2,87.

**Évolution globale : ×4,56/×4,69 → ×2,85/×2,89 au mur.** La porte
`Rose ≤ Rust` n'est **pas** atteinte.

---

## 5. Cause chiffrée du reste (pourquoi la porte n'est pas atteinte)

Décomposition du mur final : Rose ~7,15 ms = **4,17 ms internes** +
~2,98 ms de hors-chrono (parse 0,12 ms + compilation JIT ~1,5 ms + démarrage
processus ~1,35 ms). Rust ~2,50 ms = 1,39 ms internes + ~1,11 ms de hors-chrono.

Deux écarts distincts, tous deux mesurés :

1. **Écart interne : 4,17 − 1,39 = 2,78 ms.** Le poste dominant est
   l'addition des 10⁷ carrés : **1,26 ms côté Rose**, contre **~20 ns côté
   Rust** (mesuré : `rustc -O` sur la seule boucle donne 16–32 ns). LLVM
   remplace la boucle par une forme fermée — ce qui est **interdit ici**.
   Viennent ensuite : rationnels 0,856 ms (Rust fait du `f64`, sémantique
   différente : `7.8188…` au lieu de la fraction exacte de 2 500 chiffres),
   tables 0,606 ms (Rust 0,289 ms), flottants 0,524 ms (Rust 0,514 ms :
   **parité**), fib(28) 0,406 ms (Rust 0,449 ms : **Rose gagne**), RNG
   0,348 ms (le jumeau Rust est éliminé par LLVM, non comparable), E/S
   0,081 ms, listes 0,080 ms, chaînes 0,004 ms (Rust 0,144 ms : **Rose
   gagne**). *(Temps Rust par section : copie instrumentée locale, c16,
   20/11/2026.)*

2. **Écart de hors-chrono : ~2,98 − 1,11 = ~1,87 ms**, dominé par la
   compilation JIT (`compilateur_tout` ~0,89–1,79 ms + `finalize` ~0,67 ms).

**Conséquence arithmétique** : même en ramenant à zéro TOUTES les sections
sauf les carrés, le plancher serait `1,26 ms + 2,98 ms = 4,24 ms > 2,50 ms`.
La porte est donc **structurellement fermée** tant que (a) la somme exacte
des 10⁷ carrés doit être calculée terme à terme et (b) le JIT Cranelift est
compilé au démarrage. Aucune des deux n'est contournable dans le périmètre
autorisé sans enfreindre « pas de formule fermée ».

---

## 6. Validation (preuves de sortie réelles)

| Gate | Résultat |
|---|---|
| `cargo build --release --bin rose` (target dir isolé) | **OK** |
| `cargo test --release` | **431 passed, 0 failed**, 6 ignored (220 + 204 + 0 + 6 + 1 + 0) |
| Jardin `jardins/tests/premiers.rose` | `── jardin/premiers : 20 tests, 0 échec ──` |
| `fib(25)` | `75025` |
| Point fixe (2 voies) | `b3ed9262dd54776a6dbd07bc12368d0e` **×2** (`rose rir …` et `rose … …`) |
| 9 domaines multi, c16 **et** c6 | **81/81** valeurs conformes par cœur ; D6 reste gagnant (0,121/0,134 ms vs Rust 0,184/0,189 ms) |
| `bench_global.rose` | **rc = 0**, 12 lignes, **11 valeurs identiques** vérifiées à chaque tirage (3 × 2 × 9 = 54 exécutions finales) |
| Différentiel variantes | sorties identiques au témoin sur bornes/préfixes/Unicode modifiés |
| Correctifs antérieurs | `f9b0966`, `38a8804`, `0b87f77`, `065a0d2`, `43d1884`, JIT2 (CARRES_ACC, FLOTTANTS_ACCUM, LISTE_P8), CPLX1, PGCD différé glb1 : **intacts** ; `git diff` ne montre que `interp.rs` et `jit.rs` |
| Propreté | instrumentation retirée (aucun `PROF`/`DBG`) ; aucun commit, aucune publication, `rose/rose` inchangé |

---

## 7. État final factuel

- **Fait** : trois superinstructions RIR (`interp.rs`) + réécriture de
  `emettre_carres_acc` en 16 accumulateurs i64 avec vidage i128 prouvé
  (`jit.rs`). Repli exact conservé partout.
- **Vérifié** : gain interne ×2,05, gain mur ×1,57–×1,80, global
  ×4,6 → ×2,85–×2,89, 431 tests, jardin 20/20, fib25, point fixe ×2,
  81/81 ×2 cœurs, 11 valeurs globales inchangées.
- **Non atteint** : `Rose ≤ Rust`. Cause chiffrée §5 : 1,26 ms de carrés
  exacts (Rust 20 ns par forme fermée interdite) + ~2,98 ms de
  parse/compilation/démarrage, contre 2,50 ms au total chez Rust.
- **Reste actionnable** (hors périmètre ou interdit) : table à clé chaîne
  0,61 ms, RNG 0,35 ms, et surtout suppression du coût de compilation JIT
  au démarrage (~1,5 ms) — mais même réunis, ces postes ne comblent pas
  l'écart des carrés.
