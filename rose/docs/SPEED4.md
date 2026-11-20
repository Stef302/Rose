# SPEED4 — tables + RNG du banc global (CONSTAT DATÉ)

Sous-agent `speed4`. **Aucun commit, aucune publication, `rose/rose` non remplacé**
(binaire intact : mtime 19/11, SHA256 `2143e996…b265e076f9b894be689f1293aad9c`).
Un seul fichier source livré modifié : **`rose/src/run.rs`** (134 insertions,
23 suppressions selon `git diff --stat`), plus ce document. `interp.rs`, `jit.rs`, `i128x.rs`, `front.rs`,
`rir.rs`, `main.rs`, `Cargo.toml`, `PLAN.md` et les docs des autres agents n'ont
**pas** été touchés. Tous les correctifs antérieurs de `run.rs` sont préservés
(rationnel `f9b0966`, pool de listes `38a8804`, garde mpz `0b87f77`, `array_part`
`065a0d2`, `VAL1` `e4f25cf`) : le diff ne porte que sur 5 zones (§3).

> **Verdict : porte « diviser tables+RNG par 2 » NON ATTEINTE.**
> Gain réel mesuré : **−29,5 % c16 / −28,3 % c6** sur la somme
> `tables + RNG` (salves entrelacées 12 échantillons/cœur/binaire, médianes et
> différences appariées). La somme passe de **969 à 675 µs (c16)** et de
> **1 044 à 749 µs (c6)** ; la porte exigeait ≤485 µs (c16) / ≤522 µs (c6).
> Il manque donc **~190 µs (c16) / ~226 µs (c6)**. Cause chiffrée et prouvée §5 :
> le plancher inévitable du poste « tables » dans le périmètre est ~365 µs,
> dont ~145 µs de `format!` dans `interp.rs` (interdit) et ~165 µs d'allocations
> de clés chaîne que l'API `Valeur` impose.

**Horloge de la machine : 20/11/2026 ~11:30–14:00 UTC** (la consigne annonce
21/11). Tout chiffre ci-dessous est un **constat daté**, à re-mesurer après tout
changement de binaire, de pinage ou de topologie.

---

## 1. Protocole réellement exécuté

| Élément | Mesure |
|---|---|
| HEAD annoncé / HEAD au départ des mesures | `6e8f6db` / `6e8f6db` |
| Arbre source isolé | `/tmp/opencode/speed4_agents/src_base` = copie de l'arbre au départ (taille 175 Mio, `target` exclus) |
| Binaire témoin | `/tmp/opencode/speed4_agents/rose_base`, SHA256 `e8fc22ff201c292bc3c47f64bdd37a919d8a74f8deb48ce5f1fdd6f7c54167c4` |
| Binaire final | `/tmp/opencode/speed4_agents/rose_deliver`, SHA256 `431ac0d7f09cb6e271d36b877516374eed156be731cb20696557ed7fdda654ac` |
| Construction | `CARGO_TARGET_DIR` dédié pour chaque binaire ; A et B viennent du **même arbre figé**, seul `run.rs` diffère. Aucun binaire copié dans `rose/rose`. |
| Pinage | **`taskset -c 6` ET `-c 16`**, cœurs physiques isolés re-vérifiés (`thread_siblings_list` = `6` et `16`) |
| Échantillons | **12 par binaire et par cœur**, rotation entrelacée `A/B/B/A` puis `B/A/A/B`, médianes **et** différences appariées ; 2 salves complètes c6 + c16 |
| Horloge interne | primitRose `temps()` (`cntvct_el0`-compatible via `temps_unix_nanos()`), relevée avant/après chaque section dans une COPIE du banc (`/tmp/opencode/speed4_agents/phase_bench.rose`) — **aucun `.rose` du dépôt modifié** |
| Horloge des micro-sondes | `cntvct_el0` (fréquence re-mesurée : `cntfrq_el0` = 1 000 000 000 Hz) |
| Valeurs | 11 valeurs du global vérifiées **à chaque tirage** ; 81/81 sur le contrôle multi (9 programmes × 9 tirages, c6 **et** c16) |
| Bruit | dérive forte entre salves (RNG témoin lu de 342 à 393 µs sur c6) : la décision repose sur les **différences appariées entrelacées**, pas sur les médianes marginales. Une valeur aberrante du poste « rationnels » (3,5 ms au lieu de 0,5 ms) est apparue dans une salve c16, sans effet sur `tables+RNG`. |

Harnais : `/tmp/opencode/speed4_agents/{measure.py,bench_multi.py}` ; banc
instrumenté : `phase_bench.rose` (structure du banc original strictement
conservée — une première copie avec la chaîne source en littéral désactivait la
superinstruction GLB2 et donnait 488 µs au lieu de 4 µs : **corrigée avant toute
mesure retenue**).

---

## 2. Décomposition par phase et goulot

Médianes c16, 12 échantillons (ns) ; les deux dernières colonnes sont le gain
imputable au changement.

| Section | témoin c16 | final c16 | témoin c6 | final c6 | Δ c16 | Δ c6 |
|---|---:|---:|---:|---:|---:|---:|
| 1 — 10⁷ carrés | 730 279 | 733 575 | 729 654 | 733 862 | +3 296 | +4 208 |
| 2a — 10⁶ flottants | 513 471 | 513 471 | 513 727 | 516 792 | 0 | +3 064 |
| 2b — 1 001 rationnels | 3 497 386* | 2 506 300* | 497 975 | 498 903 | bruit | +928 |
| 3 — 1 001 chaînes | 2 616 | 2 440 | 3 104 | 2 656 | −176 | −448 |
| 4 — 10 001 listes | 82 376 | 81 616 | 88 360 | 87 528 | −760 | −832 |
| **5 — 5 001 tables** | **627 495** | **500 519** | **661 526** | **539 879** | **−126 976** | **−121 648** |
| 6 — fib(28) | 960 | 1 032 | 1 328 | 1 320 | +72 | −8 |
| **7 — RNG 10⁶ octets** | **341 912** | **174 168** | **382 888** | **208 776** | **−167 744** | **−174 112** |
| 8 — E/S | 46 472 | 50 776 | 72 344 | 71 344 | +4 304 | −1 000 |
| **TOTAL tables+RNG** | **969 406** | **674 686** | **1 044 414** | **748 655** | **−294 720** | **−295 759** |

`*` valeur aberrante d'une salve, non retenue pour la décision (§1).

**Goulot #1 du périmètre = les tables (64,7 % de la somme c16)**, devant le RNG
(35,3 %). Le RNG a cédé le plus (−49 % c16, −45 % c6) ; les tables n'ont cédé
que −20 % c16 / −18 % c6.

### 2.1 Où vont réellement les ~627 µs de tables (sondes d'élimination)

Binaires construits depuis le même arbre, une seule partie de `Table::inserer`
court-circuitée à la fois, médianes de 5 tirs c16 (phase 5) :

| Variante (sonde, hors livrable) | µs | Coût imputable |
|---|---:|---|
| `inserer` = no-op | ~200 | **appelant seul** : `format!("k{}", i)` (`interp.rs`) + `Valeur::s` + `Valeur::z` |
| `inserer` = `h()` + `k_table()` puis retour | ~213 | +11 µs = hachage direct (déjà optimisé) |
| + `items.push((Valeur::Vide, Valeur::Vide))` | ~260 | +47 µs = `Vec::push` nu |
| + `items.push((cle, val))` réelles | ~425 | **+165 µs = rétention des 5 000 `Valeur::Chaine`** (allocations `String` + `Rc<ChaineRep>` qui ne peuvent pas être réutilisées) |
| + index `HashMap` complet (v4) | ~530 | +105 µs = `entry`/insertion |
| + `index.reserve(8192)` (v5 = livré) | ~500 | −21 µs (13 rehashes supprimés) |

Une seconde sonde (`Valeur::s` remplacé par un clone d'`Rc` statique dans le
binaire no-op) fait passer 200 → 145 µs : **~55 µs de l'appelant sont dans
`Valeur::s`/`ChaineRep::plein` (run.rs), ~145 µs sont le `format!` de
`interp.rs` (interdit)**.

### 2.2 Où vont réellement les ~342 µs de RNG

| Mesure | µs / 10⁶ octets |
|---|---:|
| `graine` + `tampon(10⁶)` seuls (`remplir_hasard` no-op) | ~6 |
| RNG scalaire Rose (témoin) | ~342 |
| Rose avec boucle d'octets remplacée par des XOR (tirages conservés) | ~317 |
| Rose après NEON (livré) | ~174 |
| Micro-jumeau C, boucle scalaire équivalente | ~285 |
| Micro-jumeau C, NEON `vqtbl4q_u8` (md5 du tampon identique) | ~174 |
| Micro-jumeau C, 500 000 `mix` splitmix64 seuls | ~161 pour **4×** 125 000 tirages, soit ~40 µs pour les 125 000 tirages du banc |

Conclusion : après optimisation, le RNG est à son plancher de distribution
d'octets (~174 µs, identique au jumeau C) ; le calcul splitmix lui-même n'est
plus que ~40 µs.

---

## 3. Changements livrés (tous dans `rose/src/run.rs`)

1. **Hachage direct des chaînes, sans allocation** (`Table::h`, branche
   `Valeur::Chaine`) : `octets_empruntes()` quand la feuille est contiguë
   (`Plein`/`FichierMmap`), repli sur `aplatir()` pour une rope concaténée.
   Même FNV-1a sur les mêmes octets → hash bit-identique, donc aucun changement
   d'index. **Mesuré : −99 µs c16 (633 → 534 µs, apparié médian −95,8 µs).**
2. **Insertion à emprunt unique et hash unique** (`Table::inserer`) : un seul
   `borrow_mut()`, `k_table` calculé une fois, `HashMap::entry` remplace
   `trouver_h` (get) + `contains_key` + `insert`. L'ordre des validations
   `valeur_egale` (tableau dense → index → collisions → fallback) est conservé.
   **Mesuré : −8 µs c16.**
3. **Réservation de l'index** (`Table::neuf`) : `index.reserve(8192)` au lieu
   d'un `HashMap` vide qui grandit 13 fois. **Mesuré : −21 µs c16.**
4. **NEON pour le remplissage** (`remplir_hasard`, `aarch64`, alphabets
   puissances de 2 ≤ 64) : table de 64 octets chargée une fois dans
   `uint8x16x4_t`, `vandq_u8` + `vqtbl4q_u8` + `vst1q_u8` (4 écritures de 16
   octets par bloc de 64). Huit tirages par bloc, calculés par deux appels à
   `rng_tirer_x4` : les quatre états `s+C…s+4C` sont d'abord additionnés, puis
   les mixages entrelacés ; **suite RNG et état final bit-identiques** (vérifié
   par `rng_diff.rose` : 256 octets + deux tirages + alphabet de 5, diff vide).
   La queue non multiple de 64 et les alphabets > 64 retombent sur l'ancienne
   voie scalaire. **Mesuré : −168 µs c16 / −174 µs c6.**
5. `ChaineRep::plein` : `is_ascii()` → `len()` au lieu de `chars().count()`.
   Gain non mesurable isolément (conservé : sans risque, sémantique identique
   pour l'UTF-8 valide).

### Tentatives mesurées puis REJETÉES (code retiré du livrable)

| Tentative | Mesure | Décision |
|---|---|---|
| ILP-4 scalar seul dans la boucle 32 octets (`rng_tirer_x4`) | 351 759 → 352 063 ns (bruit) : LLVM interleave déjà les 4 mixages | conservé **uniquement** comme fournisseur des 8 tirages NEON |
| NEON + pointeur brut de sortie au lieu de `buf.as_mut_ptr().add(i)` | +8 µs (bruit) | retiré |
| NEON + `rep_mut_nv()` (emprunt `RefCell` supprimé) | +8 µs (bruit), unsafe inutile | retiré, `borrow_mut()` conservé |
| `index.reserve(16384)` au lieu de 8192 | ~−10 µs dans une salve, non reproductible | 8192 conservé |

---

## 4. Gains réels c6 + c16 — imputable

Salves entrelacées 12 × 2 cœurs, somme `tables+RNG` :

| Salve | témoin c16 | final c16 | ratio | témoin c6 | final c6 | ratio |
|---|---:|---:|---:|---:|---:|---:|
| décision (12 éch.) | 969 406 | 674 686 | **0,696** | 1 044 414 | 748 655 | **0,717** |
| différence appariée médiane | — | **−295 880** | — | — | **−291 648** | — |

**Aucune salve n'a atteint la division par 2.** Les quatre phases hors périmètre
(carrés, flottants, rationnels, E/S) sont inchangées dans les limites du bruit :
aucun gain n'a été pris hors de `run.rs`.

---

## 5. Cause chiffrée de la non-atteinte

Il manque ~190 µs (c16) / ~226 µs (c6). Le plancher démontré du poste tables,
dans le fichier possédé, est :

```
format! dans interp.rs (interdit)              ~145 µs   (sonde no-op sans alloc Rc)
Valeur::s / ChaineRep::plein (run.rs, inévitable) ~55 µs   (diff 200 → 145 µs)
rétention des 5000 clés chaîne (allocations      ~165 µs   (sonde push Vide → push réel)
  String + Rc<ChaineRep>, API Valeur figée)
Vec::push nu                                    ~47 µs
index HashMap après reserve                     ~85 µs
hachage direct                                  ~11 µs
                                             ─────────
plancher table observable                      ≈ 508 µs   (mesuré 500 µs)
```

Le RNG, lui, est déjà à ~174 µs, **identique au micro-jumeau C NEON** : il ne
reste aucun gain structural dans `run.rs` sans changer la suite splitmix
(interdite : les valeurs doivent rester identiques).

**Plancher arithmétique : 508 + 174 ≈ 682 µs > 485 µs visés (c16)**, alors que
le témoin vaut 969 µs. Même en ramenant à zéro l'index, le `Vec::push` et le
hachage (~143 µs de plus), le plancher resterait ~539 µs, toujours au-dessus de
la porte. Les ~145 µs de `format!` sont dans `interp.rs` (formellement interdit)
et les ~165 µs de rétention sont imposés par la représentation de
`Valeur::Chaine` partagée avec `interp.rs`/`front.rs`/`rir.rs` (interdits) :
changer cette représentation modifierait la drop-glue de `Valeur`, ce qui a
déjà fait régresser D5/D6 de 4 à 10 % le 18/11 (mesuré, cf. commentaire en tête
de `run.rs`).

Ce sont des blocages physiques dans le périmètre autorisé, pas un renoncement
prématuré : le goulot a été attaqué, mesuré, décomposé par sondes, et un gain
réel de ~30 % a été obtenu.

---

## 6. Validation (preuves de sortie réelles, binaire `rose_deliver`)

```
cargo build --release --bin rose (dépôt, CARGO_TARGET_DIR dédié)   rc=0
cargo test --release (dépôt, target dédié)   rc=0 ; 515 passed, 0 failed
   (499 attendus ; 16 tests de plus dus aux agents concurrents)
jardins/tests/premiers.rose      ── jardin/premiers : 20 tests, 0 échec ──
fib(25)                          75025
point fixe (2 voies)             b3ed9262dd54776a6dbd07bc12368d0e ×2
                                 (rose rir compilateur/emetteur.rose /
                                  rose compilateur/emetteur.rose compilateur/emetteur.rose ;
                                  stderr 0 octet des deux côtés)
9 domaines multi, c6, 9 tirages  81/81 OK (sorties octet pour octet = base)
9 domaines multi, c16, 9 tirages 81/81 OK (sorties octet pour octet = base)
bench_global.rose                rc=0, 12 lignes, 11 valeurs IDENTIQUES à la base,
                                 stderr 0 octet (vérifié c16 et c6)
detail géant demo_geant.rose     rc=0, propriétés dynamiques affichées,
                                 stderr 0 octet
rng_diff.rose (256 + 100 octets, 2 graines)  sortie IDENTIQUE à la base
```

Le binaire compilé depuis le **dépôt courant** (qui contient aussi les
modifications concurrentes de `front.rs` et d'autres agents) donne les mêmes
11 valeurs du global et le même point fixe `b3ed9262…` ×2.

---

## 7. État final factuel

- **Fait** : 5 changements mesurés dans `run.rs` seul (hachage de chaîne sans
  allocation, insertion à emprunt/hash uniques, réserve de l'index, remplissage
  NEON avec ILP-4, comptage ASCII), instrumentation de profilage **retirée** du
  livrable (`grep -c 'SPEED4 PROF\|sp4::' rose/src/run.rs = 0`).
- **Vérifié** : tables+RNG −29,5 % c16 / −28,3 % c6 en différences appariées ;
  RNG ÷2 (342 → 174 µs) ; 515 tests verts, jardin 20/20, fib25, point fixe ×2,
  81/81 ×2 cœurs, 11 valeurs globales, tampon RNG bit-identique.
- **Non atteint** : division par 2 de la somme. Reste chiffré ~190 µs (c16) /
  ~226 µs (c6), dont ~145 µs de `format!` dans `interp.rs` (interdit) et ~165 µs
  d'allocations de clés imposées par `Valeur::Chaine` (dont dépendent les fichiers
  interdits). Aucune triche : pas de formule fermée, pas de constante du banc,
  pas de banc modifié, mêmes valeurs vérifiées à chaque tirage.
- **Propreté** : aucun commit, aucune publication, aucun binaire installé dans
  `rose/rose`, aucun autre fichier source du dépôt modifié par ce sous-agent.
