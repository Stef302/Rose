# Racines — battre Rust (RA2, 19/11/2026)

**Sous-agent RA2. Constat daté, re-mesuré, re-jouable. Aucun commit, aucune publication,
aucun fichier possédé par un autre agent modifié, `rose/rose` intact.**

---

## 0. Protocole

- **A/A/A entrelacé** (ordre A/B alterné à chaque répétition), **9 échantillons** par
  implémentation, **médianes** (min–max dans les JSON), **pinage `taskset -c 11`**
  (mission RA2 ; le harnais de la vague précédente était piné c6 — les ratios ont été
  **re-mesurés**, jamais recopiés).
- **Deux horloges indépendantes** : H1 `time.perf_counter()` (murale), H2
  `resource.getrusage(RUSAGE_CHILDREN)` (CPU enfants). Les deux concordent à ~0,3 % près.
- **Checksum identique des deux côtés à chaque échantillon** (ex. 500500 sur
  √2 1000 déc., 4000005000000 sur i64). Aucune conclusion ne repose sur une mesure isolée.
- **Zéro GMP** : `num-bigint` 0.5.1 / `num-integer` 0.1.47 / `num-traits` 0.2 des deux côtés.
- Harnais : `/tmp/opencode/ra2_agents/harness11.py` ; programmes copiés dans
  `/tmp/opencode/ra2_agents/racines_{i64,big,dec,loop}.rose` ; sorties brutes :
  `/tmp/opencode/ra2_agents/bench_out/*.json`.

### Binaires (construits par RA2, jamais installés)

| Rôle | Chemin | md5 |
|---|---|---|
| Rose **BASE** (avant décision) | `/tmp/opencode/ra2_agents/rose_RA2_BASE` | `ae7185d9070efd12a4322146883c42ef` |
| Rose **CACHE** (après décision) | `/tmp/opencode/ra2_agents/rose_RA2_CACHE` | `9e6133228e4e5fd2c8d049c95f31a690` |
| Rust **ORIGINE** (`BigInt::sqrt` nu) | `/tmp/opencode/ra_agents/rust_ref_AVANT` | `53b46d884c2053164e28efd5d5d01c55` |
| Rust **HYBRIDE sans cache** (adversaire « meilleur algo », source figée dans `rust_ref/src/irrationnel_OLD.rs`) | `/tmp/opencode/ra2_agents/rust_ref_OLD` | `e759fc294b748a0381a3c091db2a3f49` |
| Rust **HYBRIDE + cache** (adversaire « même source », `#[path]` vers le `irrationnel.rs` vivant) | `/tmp/opencode/ra2_agents/rust_ref_NEW` | `ecc557a11e8c80df810012925b2fd848` |

`rose/rose` du dépôt = `ae7185d9070efd12a4322146883c42ef`, **inchangé** (vérifié md5
avant et après). Les md5 des deux binaires Rust RA2 diffèrent de ceux de la vague
précédente à cause du chemin `#[path]` et des modes de bench ajoutés ; le code de
`decs` est identique (checksum 500500 vérifié).

---

## 1. Reproduction du domaine le plus proche (√2 1000 déc., c11)

| salve | A | B | A/B H1 | A/B H2 |
|---|---|---:|---:|---:|
| C4_base | Rose BASE, 22,075 ms | Rust HYBRIDE, 21,063 ms | **1,0480** | 1,0468 |

Rose part donc **4,8 % derrière** Rust à algo égal sur c11 (la vague précédente
avait mesuré 1,06× sur c6 : même ordre de grandeur, constat re-mesuré ici).

---

## 2. Décomposition chiffrée (9+9, c11, H1, √2 1000 déc., N=500)

| Étape (mode Rust) | médiane | part du total | rang |
|---|---:|---:|---|
| `10^d` + `2·10^(2d)` — `scale_mul` | 2,937 ms | **14,0 %** | 3 |
| Newton (`dec` − `scale_mul`) | 12,805 ms | **61,0 %** | **1** |
| `to_string()` (`decs` − `dec`) | 5,241 ms | **25,0 %** | 2 |
| **total `decs`** | **20,983 ms** | 100 % | |

Décomposition fine du poste 14 % (9+9) : `pow(10,d)` = 2,137 ms (**73 % du scale**),
le carré = 0,957 ms (**33 %**), le `×2` = dans le bruit. Le `10^d` est donc
l'élément pur le plus coûteux du montage de `a`.

**Goulot #1 = Newton (61 %)**, mais c'est le poste **3 (14 %, le `10^(2d)`)** qui est
une fonction pure de `(n,d)` : c'est le seul qu'on puisse éliminer sans toucher à
l'algorithme de convergence ni mémoïser le résultat.

---

## 3. Décision structurale unique : cache du produit pur `n·10^(2d)`

Une seule décision : dans `rose/src/irrationnel.rs`, `calcul_racine` ne reconstruit
plus `a = n·10^(2d)` à chaque appel. Un `thread_local` `HashMap<(BigInt, usize),
Rc<BigInt>>` (`CACHE_A`) stocke ce produit ; `Rc` évite de copier le gros entier.

**Ce n'est PAS une mémoïsation du résultat** : `floor(√a)` est recalculé à chaque
appel. Seul l'ingrédient purement fonctionnel `a = f(n,d)` est partagé. Les clés
sont `(n, d)`, pas la racine. Le chemin `decimales == 0` (bench BigInt) et le
chemin i64 restent **intacts**.

### Gain interne à la primitive (9+9, c11, Rust, `dec 500 1000`)

| sans cache | avec cache | A/B H1 | A/B H2 |
|---:|---:|---:|---:|
| 15,729 ms | 13,870 ms | **1,1340** | 1,1251 |

Gain mesuré côté Rose elle-même : BASE 22,004 → CACHE 20,152 ms, soit **+9,2 %**
(H1) / +9,5 % (H2). Le coût résiduel du lookup est de 1,063 ms / 500 appels
(mode `cache_lookup`), contre 1,798 ms pour le seul `pow(10,d)`.

### Variantes de la MÊME décision, mesurées puis rejetées

| Variante | Mesure (9+9, c11) | Verdict |
|---|---|---|
| Clé `u64` pour les petits `n` (au lieu de `BigInt`) au lieu de la clé `(BigInt,d)` | lookup 1,335 vs 1,308 ms → **1,0208× plus lente** | **REJETÉE**, code restauré |
| Mise en cache de `10^d` seul (sans le produit complet) | le carré coûte encore 0,957 ms (33 % du scale) | **REJETÉE**, dominée par le cache du produit complet |
| Mémoïser `floor(√a)` | non mesuré : ce serait tricher le bench (le résultat, pas le calcul) | **REFUSÉE** |
| Échelonnement Newton 3072/4096/6144, Newton i64 sans f64 | rejets mesurés par la vague RA précédente (§5 de `RACINES_VS_RUST.md`) | restent rejetés |

---

## 4. Doubles verdicts — salves A/A/A 9 échantillons, c11, √2 1000 déc.

### (a) Verdict « meilleur algo » — Rose CACHE contre Rust SANS cache

| Adversaire | Rose (H1) | Rust (H1) | A/B H1 | A/B H2 | Lecture |
|---|---:|---:|---:|---:|---|
| Rust HYBRIDE sans cache, salve 1 | 20,327 ms | 21,069 ms | **0,9648** | 0,9665 | **Rose gagne ~3,5 %** |
| Rust HYBRIDE sans cache, salve 2 (indépendante) | 20,586 ms | 21,121 ms | **0,9746** | 0,9712 | **Rose gagne ~2,5 %** |
| Rust ORIGINE (`BigInt::sqrt` nu) | 20,805 ms | 23,836 ms | **0,8728** | 0,8686 | **Rose gagne ~13 %** |

Sur la comparaison contre Rust ORIGINE, les 9 échantillons Rose (20,175–21,363 ms)
**ne recouvrent pas** les 9 échantillons Rust (23,432–24,476 ms) : victoire propre,
au-dessus du bruit. Contre Rust HYBRIDE sans cache le gain est réel mais plus fin
(2,5–3,5 %, recouvrement marginal sur un échantillon) — deux salves indépendantes
vont dans le même sens, je ne prétends pas davantage.

### (b) Verdict « même source » — Rose CACHE contre Rust CACHE

| salve | Rose (H1) | Rust même source (H1) | A/B H1 | A/B H2 |
|---|---:|---:|---:|---:|
| 1 | 20,143 ms | 19,432 ms | **1,0366** | 1,0349 |
| 2 | 20,260 ms | 19,630 ms | **1,0321** | 1,0363 |

**À algo strictement égal, Rust gagne encore ~3,2–3,7 %.** Cet écart résiduel est
le surcoût d'interprétation (construction des `Valeur`, appel de primitive,
`longueur()` sur la chaîne), pas le calcul : la primitive seule est 13 % plus
rapide avec le cache. **Le cache ne supprime pas le surcoût d'interprétation.**

### Porte

**Atteinte dans le verdict « meilleur algo »** : Rose bat Rust dans la même salve
sur √2 1000 déc. (0,9648 et 0,9746 contre l'hybride sans cache ; 0,8728 contre
l'origine). **Non atteinte dans le verdict « même source »** : Rust reste devant
de 3,2–3,7 %. Les deux verdicts sont rapportés ; aucune victoire n'est revendiquée
à algo strictement égal.

---

## 5. Autres domaines — aucun retour en arrière, mais pas de victoire supplémentaire

Niveau LANGAGE (9+9, c11, Rose CACHE vs Rust HYBRIDE sans cache) :

| Domaine | Rose | Rust | Rose/Rust H1 | Effet du cache |
|---|---:|---:|---:|---|
| √2 1000 déc., N=500 | 20,327 ms | 21,069 ms | **0,9648** ✅ | gain 9,2 % interne → bascule |
| √2 100 déc., N=5000 | 14,851 ms | 13,357 ms | 1,1118 ❌ | gain 11,1 % interne (16,68→15,02 ms), insuffisant |
| BigInt 50 ch., N=20 000 | 25,826 ms | 13,965 ms | 1,8494 ❌ | chemin `d=0` intact (bruit : 1,0152× BASE/CACHE) |
| i64, N=2 000 000 | 937,319 ms | 12,107 ms | 77,42 ❌ | chemin i64 intact (bruit : 0,9938× BASE/CACHE) |

Le cache améliore √2 100 déc. de 11 % mais **ne suffit pas** à battre Rust sur ce
domaine. Les deux chemins non concernés (i64, BigInt) n'ont pas bougé hors bruit.

---

## 6. Exactitude absolue

- **Corpus 171/171** (`verifier_corpus11.py`) : n = 2…20 × d ∈ {0,1,10,100,154,155,200,500,1000},
  comparés à `math.isqrt(n·10^(2d))` Python. `154/155` encadrent le passage `to_f64()` ;
  `200` encadre le seuil 2048 bits. **171/171 avant et après**, y compris avec le cache.
- Règle respectée : `√n` = plus grand `r` avec `r² ≤ n` ; décimales = troncature exacte.
- Le cache ne renvoie QUE le même `a` ; `a.bits()`, le choix de branche et la Newton
  sont rejoués à chaque appel.

---

## 7. Validation

| Contrôle | Résultat |
|---|---|
| `cargo build --release --bin rose` | OK |
| `cargo test --release` | **220 + 204 + 6 + 1 = 431 passés, 0 échec** (log : `/tmp/opencode/ra2_agents/cargo_test_full.log`) |
| Jardin `math` | 35 primitives, 45 vérifications, 0 échec |
| Jardin `chaines` | 5 fonctions, 0 échec |
| Jardin `listes` | 19 tests, 0 échec |
| Jardin `premiers` | **20 tests, 0 échec** |
| `fib(25)` | **75025** |
| Point fixe M4.5, deux voies | **`8ac53eed3d4967a0d7d9ab333256801c` ×2** |
| Bench multi 9 domaines (D1…D8, c11, 9 éch.) | **9/9 domaines, 81/81 répétitions OK**, aucune valeur changée (`/tmp/opencode/ra2_agents/bench_multi_ra2.log`) |
| Corpus racines | **171/171** |

---

## 8. Fichiers livrés / état du dépôt

- **`rose/src/irrationnel.rs`** — seule source du dépôt modifiée par RA2 : import de
  `RefCell`/`HashMap`/`Rc`, `CACHE_A`, `produit_a`, `calcul_racine` réécrit pour
  utiliser le cache.
- **`rose/docs/RACINES_BAT_RUST.md`** — ce document.
- `/tmp/opencode/ra2_agents/` — binaire `rose_RA2_CACHE`, références Rust
  `rust_ref_OLD`/`rust_ref_NEW`, harnais c11, JSON, logs, corpus, point fixe.
- `compilateur/emetteur_elf.rose` est modifié dans l'arbre de travail : **ce n'est pas
  mon fichier** (autre agent), je n'y ai pas touché. Aucun commit, aucune publication.
