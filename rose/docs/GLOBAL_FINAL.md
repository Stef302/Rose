# GLOBAL_FINAL — dernière tranche rationnelle (CONSTAT DATÉ, 20/11/2026)

**Auteur** : sous-agent `glb1`. Aucun commit, aucune publication, aucun
`rose/rose` remplacé. Un seul fichier source modifié : **`rose/src/i128x.rs`
(+56 lignes, 0 suppression)**. Binaire construit dans `/tmp/opencode/glb1_agents/`
avec `CARGO_TARGET_DIR` isolé. HEAD annoncé et final : `6af7b41` (inchangé).

---

## 0. Protocole réellement exécuté

| Élément | Mesure |
|---|---|
| Binaire témoin (arbre courant avant modification) | `/tmp/opencode/glb1_agents/rose_base`, SHA256 `b528c2f4d540fdb0d0a6d99f5e0dfdac557a05d7997bb6ce59f1cd48f08fdbc7` |
| Binaire final | `/tmp/opencode/glb1_agents/rose_fast1`, SHA256 `4f46fb43a1b24c589cd0ae1e97b714b50b6ffe6f321ab10e1588213dcebdaf4f` |
| Jumeau Rust | `/tmp/opencode/cmp2_agents/global_rs` |
| Pinage | **`taskset -c 6` ET `taskset -c 16`** (capacités 997 / 1017), disponibilité vérifiée dans `/sys/devices/system/cpu/cpu*/cpu_capacity` |
| Échantillons | **9 par implémentation et par cœur**, entrelacés en rotation avec alternance A/B à chaque répétition, médianes |
| Horloges | H2 = `millisecondes()`/`temps()` internes du programme ; H1 = mur `time.perf_counter_ns()` |
| Valeurs | 11 lignes du global vérifiées à **chaque tirage** (`global_bench.py`) ; micro-banc 1000 additions vérifié identique |
| Fenêtre | `2026-11-20T05:12Z` → `05:24Z` (les chiffres ci-dessous sont des constats à cet horodatage) |
| Bruit | global A/A final : 8–9 ms (c16), 8–9 ms (c6) ; rationnel A/A final : 784–808 µs ; médianes utilisées partout |

---

## 1. Décomposition (instrumentation temporaire, **retirée** avant livraison)

Copie temporaire de `i128x.rs` / `run.rs` / `interp.rs` avec compteurs + `Instant`
accumulés, vidés en fin d'exécution ; les trois fichiers ont été **restaurés
octet pour octet** (md5 `e12a75d…`, `1dac92a…`, `cacf5e5…`) avant la correction.
Micro-banc : `$r = 1/3 ; pour c de 1 vers 1001 { $r = $r + 1/c }`.

| Poste (1000 itérations) | c16 | c6 | % du `binop_plus` |
|---|---:|---:|---:|
| **`gcd` BigInt complet dans `canonicalize`** | **10 159 µs** (10,159 µs/it) | 10 115 µs | **94,3 %** |
| `gmpq_add` complet (gcd inclus) | 10 558 µs | 10 516 µs | 98,0 % |
| `binop_plus` (dispatch + 2 `vers_q` + `Gq::neuf` + add + `Valeur::qq`) | 10 777 µs | 10 727 µs | 100 % |
| `div` — construction de `1/c` | 173 µs (0,173 µs/it) | 174 µs | 1,6 % |
| `vers_q` ×4 (2 par addition, 2 par division) | 168 µs | 163 µs | 1,6 % |

La boucle non instrumentée mesurait **10 749 466 ns (c16) / 10 782 570 ns (c6)**.
**Goulot #1 = 10,16 ms de PGCD BigInt sur des nombres d'environ 430 chiffres
décimaux, soit ~94 % du temps de l'addition rationnelle.** Le `1/c` et le
dispatch ne coûtent que ~0,35 ms ensemble : les options (b) et (c) du programme
n'auraient pas touché au goulot.

---

## 2. Décision structurale retenue : option (a), PGCD différé à borne prouvée

`gmpq_add` essaye d'abord le cas `n/d + s/c` avec `s = ±1` et `c` tenant dans
`u64` (cas de `1/c`) ; sinon il garde **exactement** l'ancien chemin BigInt.

```
g = PGCD(d, c)        // euclidienne sur le RESTE SCALAIRE d % c, jamais de
                      // BigInt du côté petit, jamais de PGCD de grands nombres
d1 = d / g ; c1 = c / g
N  = n*c1 ± d1 ;  D = d*c1
g2 = PGCD(|N|, c)     // encore petit
si g2 > 1 : N /= g2 ; D /= g2
```

**Preuve d'exactitude** (vérifiée aussi par balayage exhaustif
`d,c ≤ 300`, `n ∈ [-d,d]`, les deux signes : **0 contre-exemple**) :
tout facteur premier commun à `N` et `D` divise `c` (les facteurs de `d`
seuls sont exclus par `PGCD(n,d)=1`), et sa multiplicité dans `D` est au plus
sa multiplicité dans `c`. Un seul PGCD sur `c` suffit donc à produire la
fraction **strictement réduite**, identique à l'ancien `canonicalize`.

Aucune mémoïsation, aucune formule fermée, aucune valeur codée en dur :
les 1000 additions sont bien effectuées une par une, en BigInt exact.
Le cas général (dénominateur de l'inverse > `u64`, signe autre que ±1,
`d ≤ 0`) retombe sur l'ancien code.

---

## 3. Gains mesurés (9 échantillons entrelacés, médianes)

### 3.1 Section rationnelle (micro-banc, 1000 additions)

| pin | avant H2 | après H2 | gain H2 | avant mur | après mur | gain mur |
|---|---:|---:|---:|---:|---:|---:|
| c16 | 10 749 466 ns | **789 743 ns** | **×13,61** | 11,939 ms | **1,922 ms** | ×6,21 |
| c6 | 10 782 570 ns | **798 127 ns** | **×13,51** | 11,874 ms | **2,044 ms** | ×5,81 |

**Porte « < 5 ms sur la section rationnelle » : ATTEINTE** (0,79–0,80 ms
interne, 1,9–2,0 ms mur).

### 3.2 Banc global complet (`bench_global.rose`, 11 valeurs identiques)

| pin | avant H2 | après H2 | avant mur | après mur | ratio mur avant → après |
|---|---:|---:|---:|---:|---:|
| c16 | 18 ms | **8 ms** | 21,496 ms | **10,860 ms** | 8,45 → **4,41** |
| c6 | 19 ms | **9 ms** | 21,221 ms | **11,296 ms** | 8,48 → **4,25** |

Contre Rust (H2 1,375 ms c16 / 1,420 ms c6 ; mur 2,461 / 2,657 ms) :
ratio interne **5,82 / 6,34**, ratio mur **4,41 / 4,25**. La porte « < ×5 »
du programme JIT2 est donc également franchie, **mais Rose reste plus de
2× plus lent que Rust sur le global complet** : l'objectif « idéalement
Rose ≤ Rust » n'est **pas** atteint.

---

## 4. Validation (preuves de sortie réelles)

| Gate | Résultat |
|---|---|
| `cargo build --release --bin rose` (target dir isolé) | **OK** |
| `cargo test --release` | **431 passed, 0 failed** (220 + 204 + 0 + 6 + 1 ; 6 ignored) |
| Jardin `jardins/tests/premiers.rose` | `── jardin/premiers : 20 tests, 0 échec ──` |
| `fib(25)` | `75025` |
| Point fixe (2 voies) | `b3ed9262dd54776a6dbd07bc12368d0e` **×2** (`rose rir …` et `rose … …`) |
| 9 domaines multi, c16 **et** c6 | **81/81** valeurs conformes par cœur ; ratios base/final 0,96–1,13 = bruit, aucune régression mesurée |
| `bench_global.rose` | **rc = 0**, 12 lignes, **11 valeurs identiques** à la référence, vérifiées à chaque tirage (36 exécutions) |
| Équivalence rationnelle aléatoire | 1620 lignes générées (400 opérations aléatoires + 20 chaînes de 60 additions), 420 valeurs de sortie **identiques** entre témoin et final |
| Correctifs antérieurs | `f9b0966` (affichage rationnel), `38a8804` (pool), `0b87f77` (garde mpz), `065a0d2` (table), `43d1884` (superinstructions) : **intacts**, `git diff` ne montre que `i128x.rs` |
| Propreté | aucun `rug`/GMP ajouté ; aucun commit ; aucune publication ; `rose/rose` inchangé (md5 `12eac460…`, daté du 19/11) |

---

## 5. État final factuel

- **Fait** : une seule fonction ajoutée (`pgcd_grand_petit`) et une voie rapide
  exacte dans `gmpq_add`, dans `rose/src/i128x.rs`. L'instrumentation de PROF
  a été retirée avant le build final.
- **Vérifié** : goulot chiffré (94 % de PGCD), porte rationnelle 0,79 ms
  (×13,6), global ×2 sur le mur, 431 tests, jardin, fib25, point fixe ×2,
  81/81 ×2 cœurs, 11 valeurs globales inchangées, 420 sorties rationnelles
  aléatoires identiques.
- **Reste** : ~7–8 ms hors rationnels dans le global : SEC1 carrés ~2 ms
  (`jit.rs`, hors périmètre), SEC5 tables ~2–3 ms, SEC2 flottants ~1 ms,
  sections 3/4/7 ~1 ms chacune, E/S bruitées. Rose global = 4,25–4,41× Rust
  au mur : la supériorité sur Rust n'est pas atteinte. Tout chiffre ci-dessus
  est un **constat daté du 20/11/2026** et doit être re-mesuré après tout
  changement de modèle, de quantification ou de topologie.
