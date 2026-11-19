# JIT_OFFENSIVE — deux portes, un seul fichier source (CONSTAT DATÉ, 19/11/2026)

**Auteur** : sous-agent `jit2`. **Aucun commit, aucune publication, aucun
`rose/rose` remplacé.** Périmètre source possédé : **`rose/src/jit.rs` uniquement**
(+495 −2 lignes) et ce document. Tous les binaires sont construits dans
`/tmp/opencode/jit2_agents/` avec `CARGO_TARGET_DIR` dédié.

> ⚠ Des agents concurrents ont modifié `rose/src/run.rs` et `rose/src/interp.rs`
> pendant la session. Toutes les comparaisons A/B finales ont donc été rejouées
> avec un **témoin construit sur l'arbre courant**, mes trois spécialisations
> désactivées (`rose_ctrl2`) contre le même arbre avec mes spécialisations
> actives (`rose_final`). Le gain mesuré est donc bien imputable à `jit.rs`.

---

## 0. Protocole réellement exécuté

| Élément | Mesure |
|---|---|
| HEAD annoncé / HEAD final | `f5a82ad` (inchangé ; aucun commit de ma part) |
| Binaire témoin (spécialisations OFF, arbre courant) | `/tmp/opencode/jit2_agents/rose_ctrl2`, SHA256 `b90f682bb989ac89aa68d3a047d9b3f3d73699a4c5e08e10770f0a260a385a28` |
| Binaire final (spécialisations ON, arbre courant) | `/tmp/opencode/jit2_agents/rose_final`, SHA256 `caf64f833743deda1cd6f76e9213d84ae665d11892a44f358e6378516cb14f36` |
| Binaire du début de session (référence historique) | `rose_base`, SHA256 `790a59b38a4ae0999c3c202935c13d711c8d13515ba91496011be687e38d5645` |
| Jumeaux Rust | `/tmp/opencode/cmp2_agents/global_rs` (global), `/tmp/opencode/benchmulti/multi_rs` (D6) |
| Pinage | **`taskset -c 16` ET `taskset -c 6`**, deux Cortex-X925 (`cpu_capacity` 1017 / 997), disponibilité vérifiée |
| Échantillons | **9 par implémentation et par cœur**, entrelacés en rotation avec alternance A/B à chaque répétition, médianes |
| Horloges | H2 = chronomètre interne du programme (`millisecondes()` pour le global, `temps()` en ns pour D6) ; H1 = mur `perf_counter` |
| Valeurs | vérifiées à **chaque tirage** : global 11 lignes ×3 implémentations ; D6 `4999950000` ; multi 81/81 par cœur |
| Fenêtre de mesure | `2026-11-19T22:42Z` → `22:47Z` (les chiffres ci-dessous sont des constats à cet horodatage) |
| Bruit | global A/A : 182–184 ms (témoin), 18–19 ms (final) ; D6 A : 113–190 µs (final), 309–426 µs (témoin) — médianes utilisées partout |

---

## 1. Porte 2 — D6 liste 100 k : **ATTEINTE**

### 1.1 Ce qui a été changé

`emettre_liste_machine` (aarch64 brut) écrivait **3 mots de 8 octets par élément**
dans un `Vec<Valeur>` (24 o/élément, 2,4 Mio), alors que le jumeau Rust écrit
0,8 Mio. Le détecteur strict `detecter_liste_boucles` (16 opérations RIR
exactes) **n'a pas été touché** : il prouve que `$l` n'est utilisé que par
`longueur($l)` et `$l[$j]` à l'intérieur de la même fonction, donc que la liste
n'a aucune observabilité hors de la boucle.

Nouveau chemin `LISTE_P8` :
- `emettre_liste_machine_p8(lig)` : 4 `str` de 8 octets par tour (pas = 8),
  4 accumulations de la valeur prouvée `i`, aucun appel dans la boucle ;
- `rose_jit_i64_buffer(n)` : alloue un `Vec<i64>` exactement dimensionné,
  gardé vivant dans une arène thread-local `JIT_I64_BUFFERS`, libérée par
  `jit_handles_reinitialiser()` ;
- le JIT ne construit plus aucun `Valeur::Entier` pour les 100 000 éléments.

Aucun `Valeur` intermédiaire, aucune formule fermée, aucun résultat codé en dur,
bench non modifié, somme `4999950000` inchangée.

### 1.2 Résultat (9 échantillons entrelacés, médianes, H2)

| pin | témoin Rose | final Rose | Rust | ratio témoin | **ratio final** |
|---|---:|---:|---:|---:|---:|
| c16 | 312 414 ns | **116 287 ns** | 183 471 ns | 1,695 × | **0,634 ×** ✅ |
| c6  | 368 190 ns | **118 943 ns** | 187 967 ns | 1,948 × | **0,633 ×** ✅ |

Gain imputable à `jit.rs` : **×2,69 (c16) / ×3,10 (c6)**.

### 1.3 Confirmation dans le banc multi officiel (`benchmulti/bench.py`, 9 tirages)

| pin | Rose (interne méd) | Rust | C | xRose (Rust) | tirages valides |
|---|---:|---:|---:|---:|---:|
| c16 | 0,124 ms | 0,188 ms | 0,204 ms | **0,658** ✅ | 81/81 |
| c6  | 0,154 ms | 0,194 ms | 0,239 ms | **0,794** ✅ | 81/81 |

**Porte D6 atteinte : Rose ≤ Rust dans le banc officiel, sur les deux cœurs.**

---

## 2. Porte 1 — banc global : gain massif, **porte < ×5 NON atteinte**

### 2.1 Ce qui a été changé

La section 1 est une boucle synthétique `__rose_loop_0(total: entier) -> entier`
dont le RIR exact est :
`a=1 ; tant que a<10000001 { total = total + a*a ; a=a+1 } ; retour total`.
Le chemin générique boxait `total` dès que la somme dépassait i64
(≈ a = 3,02×10⁶) : ~7 millions d'appels `rose_add_ent`.

Nouveau chemin `CARRES_ACC` (`detecter_carres_acc` + `emettre_carres_acc`) :
- **8 accumulateurs i128 indépendants** dans des registres Cranelift, un carré
  par accumulateur tous les 8 termes → chaîne critique divisée par 8 ;
- `fin ≤ 2^24` garantit `a² ≤ 2^48` (chaque carré tient dans un i64) ;
- la somme finale (≈ 3,33×10²⁰) est **boxée UNE SEULE fois** à la fin, via
  les helpers déjà prouvés `rose_mul_ent`/`rose_add_ent` (2³²×2³² = 2⁶⁴, × hi, + lo) ;
- repli bigint générique conservé pour un accumulateur initial négatif ;
- aucun appel runtime dans la boucle, aucune formule fermée (on additionne
  bien les 10⁷ carrés un par un).

Bonus parallèle : `FLOTTANTS_ACCUM` (`x += c/b`) supprime le contrôle bigint
du compteur entier (≈10 instructions/itération) ; il n'apporte que ~1 ms sur
le global, mais la sortie flottante reste bit pour bit identique (aucune
réassociation des additions).

### 2.2 Résultat (9 échantillons entrelacés, médianes)

| pin | témoin Rose H2 | final Rose H2 | Rust H2 | témoin mur | final mur | Rust mur | ratio mur témoin → **final** |
|---|---:|---:|---:|---:|---:|---:|---:|
| c16 | 183 ms | **18 ms** | 1,400 ms | 185,8 ms | **21,6 ms** | 2,450 ms | 71,9 → **8,80** |
| c6  | 182 ms | **19 ms** | 1,418 ms | 184,6 ms | **21,4 ms** | 2,616 ms | 75,4 → **8,19** |

Gain imputable à `jit.rs` : **×10,2 (c16) / ×9,6 (c6)** sur H2, ×8,6 sur le mur.

Décomposition H2 sur c16 (copie temporaire du banc avec chronomètres par
section, non modifiée dans le dépôt) :

| section | témoin | final |
|---|---:|---:|
| 1 — 10⁷ carrés (`total += a*a`) | 165 ms | **2 ms** |
| 2 — flottants + rationnels | 13 ms | 12–13 ms |
| 3–8 — chaînes, listes, tables, fib, RNG, E/S | 4–7 ms | 4–7 ms |

### 2.3 Cause chiffrée du résidu (mesurée, pas supposée)

Le temps restant est **hors de mon fichier** :

| Poste | Mesure (`temps()` ns, c16) | Fichier |
|---|---:|---|
| boucle rationnelle `r = 1/3 ; pour c de 1 vers 1001 { r = r + 1/c }` | **10 640 000 ns** (3 tirages : 10 678 536 / 10 604 169 / 10 638 681) | interpréteur + `i128x.rs` (Gq::add_in + canonicalize à chaque itération) |
| affichage de la fraction (~2 500 chiffres) | 7 000 ns | idem |
| table 5 000 / listes 10 001 / E/S | ~4 000 000 ns | interpréteur / runtime |
| JIT désactivé sur la boucle rationnelle | 10–11 ms (identique) | confirme que le JIT n'y intervient pas |

Même en supprimant **tout** le reste (2 ms de carrés + ~5 ms de sections 3–8),
les 10,6 ms de rationnels seuls donnent un ratio mur ≥ 4,3 — mais avec les
5 ms annexes le ratio mesuré reste **8,2–8,8**. La porte « < ×5 » n'est donc
**pas atteinte**. Il faudrait agir sur `Gq::add_in`/`canonicalize` ou sur
l'interpréteur (réduction différée des rationnels), fichiers interdits à ce
sous-agent.

---

## 3. Validation exécutée (preuves de sortie réelles)

| Gate | Résultat |
|---|---|
| `cargo build --release --bin rose` | **OK** (binaire `caf64f83…`, compilation avec `CARGO_TARGET_DIR` isolé) |
| `cargo test --release` | **431 passed, 0 failed**, 6 ignored (220 + 204 + 6 + 1) |
| Jardin `jardins/tests/premiers.rose` | `── jardin/premiers : 20 tests, 0 échec ──` |
| `fib(25)` | `75025` |
| Point fixe (2 voies) | `b3ed9262dd54776a6dbd07bc12368d0e` **×2** (`rose rir` et `rose compilateur/emetteur.rose`) |
| 9 domaines multi | **81/81** valeurs conformes sur c16 **et** c6 |
| `bench_global.rose` | **rc = 0**, 12 lignes, **11 valeurs identiques** au témoin |
| Pinage | c6 **et** c16, 9 échantillons entrelacés, valeurs vérifiées à chaque tirage |

---

## 4. État final factuel

- **Fait** : trois spécialisations dans `jit.rs` (CARRES_ACC 8×i128 avec boxing
  unique, FLOTTANTS_ACCUM sans garde, LISTE_P8 à pas de 8 octets) + un correctif
  de compatibilité `Valeur: Drop` sur `table_du_handle` (emprunt + clone de `Rc`,
  sémantique identique). Aucun autre fichier modifié par ce sous-agent.
- **Vérifié** : D6 Rose ≤ Rust (0,63×) sur c6 et c16, dans le harnais dédié
  **et** dans le bench multi officiel ; global ×72 → ×8,2–8,8 ; 431 tests,
  jardin 20/20, fib25, point fixe ×2, 11 valeurs globales identiques.
- **Reste** : porte 1 « < ×5 » non atteinte. Cause chiffrée : 10,6 ms de
  rationnels dans l'interpréteur + ~5 ms de sections annexes, hors
  `rose/src/jit.rs`. Aucune triche (formule fermée, résultat codé en dur, bench
  modifié) n'a été utilisée ; toutes les valeurs de sortie sont inchangées.
