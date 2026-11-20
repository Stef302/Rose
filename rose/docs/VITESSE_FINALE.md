# VITESSE_FINALE — attaque « dépasser Rust » (CONSTAT DATÉ, 19/11/2026)

**Auteur** : sous-agent `vit`. Aucun commit, aucune publication. Binaire construit
dans `/tmp/opencode/vit_agents/`, **jamais installé dans `rose/rose`**.
Fichiers possédés : `rose/src/run.rs`, `rose/src/interp.rs`, ce document.
**Aucun de ces fichiers n'est modifié à la fin du chantier** : toutes les
variantes testées ont été soit rejetées à la mesure, soit restaurées.

---

## Mise à jour datée — 21/11/2026 (constats 20/11 ; historique 19/11 conservé)

Le corps de ce document (§0–§5) est un **constat du 19/11**, binaire
`f4b4f729e2f267f9d9c79a0e1a40c7154c3bde43201ddc7648cd16bef9a5ec65`, pinage c16/c6.
Le binaire mesuré depuis est
`a2f98fe93799e2faa69a21a486bcd4b7bc7def976d7b62181c0c6c95ca04ff3c`
(constat 20/11, `rose/docs/ETAT_VICTOIRE_9_DOMAINES.md`, commit `dee9378`,
9 échantillons entrelacés, horloge « calcul seul »). Tout ce qui suit est **daté** ;
aucune conclusion d'époque n'est présentée comme définitive, et « Rose bat partout »
n'est pas écrit.

### Banc principal 9 domaines — 9/9 gagné (20/11)

| Domaine | c19 | c6 |
|---|---:|---:|
| `fib(35)` | 0,92 | 0,91 |
| Boucle 1..10⁶ | 0,39 | 0,39 |
| Premiers < 50 000 | 0,85 | 0,85 |
| Factorielle 5000 | 0,21 | 0,21 |
| PGCD 1000 chiffres | 0,66 | 0,69 |
| Chaîne 20 000 | 0,61 | 0,78 |
| Liste 100 000 | 0,64 | 0,72 |
| Table 3000 | 0,69 | 0,65 |
| E/S 10 Mo | 0,63 | 0,61 |

**Conséquence directe sur ce document** : le §1 (D6 ≈ 288 µs de page faults sur
319 µs, « porte D6 < 0,186 ms non atteinte »), le §3 (D6 1,841 c16 / 2,039 c6,
D7 1,043 c6 ❌) et le §5 (« ce qu'il faudrait pour battre Rust sur D6 ») décrivent
le **binaire du 19/11**. Sur le binaire du 20/11, **D6 est gagné** (0,64 c19 / 0,72 c6)
et D7 aussi (0,69 c19 / 0,65 c6). Ce sont des constats de catégories différentes
(binaire et pinage changés), pas une réfutation de la décomposition page-fault
mesurée ici : celle-ci reste valable **pour le binaire `f4b4f729…` et le chemin D6
d'alors**, et doit être re-mesurée avant d'expliquer le D6 actuel.

Les deux variantes testées puis rejetées (§2.3 : `plein_ascii`, `longueur` → `z`) ont
été restaurées : aucun changement de `run.rs`/`interp.rs` de ce chantier n'est
revendiqué comme cause du 9/9 du 20/11. Le gain de pool de `INNOVATION_VITESSE.md`
(2ᵉ exécution en processus persistant) n'est pas non plus la cause de ce 9/9.

### Autres bancs — cohérence avec COMPARATIF_TOTAL et INNOVATION_VITESSE

- **Global** : ×414 → ×2,85 → **×2,88 (c16) / ×2,57 (c6)** final GLB3 (`DEMARRAGE.md`,
  20/11, `951bb12`) ; carrés 1,26 → 0,76 ms (NEON), compilation Cranelift 777 → 337 µs
  (÷2,3) ; rationnels 10,75 → 0,79 ms (GLB1, `4aad1d8`). Réserve sémantique maintenue :
  `bench_global_rs` rend `1291990006563070912` (débordement `i64`) et
  `7.818804193883677` (`f64`) quand Rose rend `333333383333335000000` et la fraction
  exacte — comparaison **non équivalente** ; battre Rust exigerait d'adopter ces
  erreurs, **refusé**. **Rust reste devant.**
- **Suite complexe** : ×203 → ×1,86 → **×1,15** (SUI1, `SUITE_BEAT.md`, `745bb9c`) →
  écart réduit par CLOSE1 (`8c31a38`, LTO + lecture source unique, `CLOSURE.md`).
  **Porte Rose ≤ Rust NON atteinte** ; titre de `8c31a38` rectifié par `ef7464e`.
- **Englobant** : ×109 → ×2,49 → **×1,40–1,62** (ENG1, `ENGBEAT.md`, 20/11, `12c786c`).
  **Porte NON atteinte.**
- **Self-hosting** : **ATTENT pour l'émetteur** (`0e2d844`, reproductibilité `e0f5744`,
  `SELF_HOST_SH3.md`, 20/11) : ELF 534 780 o, md5 `ed699457…`, lit un `.rose`, reproduit
  le RIR `b3ed9262…` octet pour octet. Le **binaire `rose/rose` complet (interpréteur,
  JIT, runtime) exige toujours `cargo build`** : pas d'auto-hébergement du produit livré.

### Ce qui reste une défaite documentée

Global, suite complexe et englobant restent **perdus face à Rust** (Rust devant dans
les trois cas) ; la réserve sémantique du global interdit de compter son gain apparent
comme une victoire de moteur. Le point fixe `b3ed9262…` listé en §4 reste le bon,
mais cette entrée de validation doit être lue avec la précision ci-dessus : point
fixe de l'émetteur ≠ auto-hébergement du binaire complet. Aucun
« Rose bat partout ».

---

## 0. Protocole réellement exécuté

| Élément | Mesure |
|---|---|
| HEAD au départ | `e0ec2871d273d9de8df7546f0675a9aef5517aad` |
| Binaire isolé | `/tmp/opencode/vit_agents/rose_base` = `cargo build --release --bin rose` depuis l'arbre courant ; SHA256 `f4b4f729e2f267f9d9c79a0e1a40c7154c3bde43201ddc7648cd16bef9a5ec65` |
| `rose/rose` (intact) | SHA256 `2143e996eeff7f9fd1f4014c846a579323fb265e076f9b894be689f1293aad9c` — **différent du binaire construit** : le binaire installé est périmé par rapport aux sources |
| Binaires construits | `rose_base` (source d'origine), `rose_r1` (variante racines), `rose_final` (= `rose_base` après restauration, SHA identique) |
| Pinage | `taskset -c 16` ET `taskset -c 6`, disponibilité vérifiée par `taskset -c … echo` |
| Échantillons | 9 par implémentation et par cœur, entrelacés ordre A/B puis B/A (rotation), médianes |
| Horloges | interne `temps()`/`millisecondes()` du programme + murale `perf_counter_ns` + CPU `getrusage` |
| Valeurs | contrôlées à chaque échantillon : `4999950000` (D6), `500500` (racines), 81/81 pour le multi |
| ⚠ `target/` partagé | D'autres agents construisent dans le même `rose/target/` ; un SHA transitoire `7e9c8b5b…` y a été observé puis remplacé par `f4b4f729…` après reconstruction. **Toutes les mesures de ce document utilisent la copie isolée** `/tmp/opencode/vit_agents/rose_base` / `rose_final` (SHA constant `f4b4f729…`) |

### ⚠ Correction du chiffre de mission

Le comparatif `COMPARATIF_FINAL.md` annonçait D6 à **0,374 ms (×2,011)** avec le
binaire `2143e996…`. Ce binaire est **périmé** : reconstruit depuis HEAD `e0ec287`,
le même programme donne **0,319 ms (c16, harnais focalisé)** — le gain de
`LISTE_INLINE` (boucle fusionnée déjà présente dans `jit.rs`) est dans l'arbre.
L'écart réel à combattre est donc **×1,72 (focalisé c16)** / **×1,84 (multi c16)**,
pas ×2,01. Tous les chiffres ci-dessous sont re-mesurés.

---

## 1. D6 — décomposition chiffrée du goulot

### 1.1 Ce que fait réellement le chemin D6 à HEAD

`r_list.rose` est spécialisé par `LISTE_INLINE` (`jit.rs`, hors de mon périmètre).
Le principal machine n'émet **aucun appel `extern "C"` par élément** : 8 appels
au total (`rose_temps` ×2, `rose_jit_liste_vide`, `_reserve`, `_meta`, `_set_len`,
`rose_affiche_ent` ×2). La boucle fusionnée émet 19 instructions pour 4 éléments
(475 000 instructions pour 100 000 éléments). La cause « ~200 000 appels
`extern "C"` » du comparatif est **obsolète** et n'a pas pu être reproduite.

### 1.2 Mesure des page faults dans la région chronométrée

`/usr/bin/time -v` (processus entier, 1 échantillon chacun) :

| Exécutable | page faults mineurs | RSS max |
|---|---:|---:|
| Rose `r_list.rose` | **802** | 6 760 Kio |
| Rust `multi_rs list` | **272** | 2 500 Kio |

Différence : **530 faults**, cohérente avec la représentation : `Valeur::Entier`
occupe **24 octets** (`layout_valeur_entier_nu` l'impose) contre 8 octets pour
`Vec<i64>` côté Rust → 2,4 Mio écrits contre 0,8 Mio, ~586 pages touchées contre ~196.

### 1.3 Microbench C isolé — imputabilité du temps

`/tmp/opencode/vit_agents/pf.c`, mêmes 100 000 éléments, même motif d'écriture,
`gcc -O2`, `taskset -c 16`, 9 échantillons, médianes (ns) :

| Variante | médiane | min | max |
|---|---:|---:|---:|
| `rose` (24 o/él., mmap paresseux) | **320 977** | 310 369 | 503 698 |
| `rose_pre` (24 o/él., pages pré-fautées hors chrono) | **32 832** | 32 368 | 79 249 |
| `rust` (8 o/él., mmap paresseux) | 141 905 | 140 513 | 148 128 |
| `rust_pre` (8 o/él., pré-fauté) | 38 689 | 38 560 | 41 761 |

Le microbench « rose paresseux » (320 977 ns) reproduit le temps Rose réel
(319 329 ns) **à 0,5 % près**. Décomposition :

| Poste | ns | % du D6 Rose |
|---|---:|---:|
| **Page faults (premier toucher des pages)** | **≈ 288 000** | **≈ 90 %** |
| Boucle machine + 8 appels helper + prologue + épilogue | ≈ 32 800 | ≈ 10 % |

**Goulot #1 = 90 % page faults**, pas les appels extern ni les `str`/`ldr`.

### 1.4 Décision structurale et rejets mesurés

| Approche | Résultat mesuré | Décision |
|---|---|---|
| (a) inline sans `extern "C"` | Déjà en place dans `jit.rs` (0 appel/élément) ; 8 appels au total | **déjà fait**, hors de mon périmètre |
| (b) fusion écriture+lecture en registres | Déjà en place : la somme accumule la valeur prouvée `j` au lieu d'un `ldr` | **déjà fait**, hors de mon périmètre |
| (c) spécialisation à preuve de bornes | Déjà en place (`detecter_liste_boucles`, 16 ops RIR strictes) | **déjà fait**, hors de mon périmètre |
| `MAP_POPULATE` sur l'allocation (coût **total** dans la fenêtre) | `pf2.c` : 349 601 ns vs 357 745 ns paresseux — dans le bruit | **rejeté** (aucun gain) |
| Pré-fautage manuel dans la fenêtre | `pf2.c` `rose_touch` : 427 586 ns — **régression de 70 µs** | **rejeté** |
| Pré-fautage hors fenêtre (arène `mmap` + touch au démarrage) | Le touch de 3,1 Mio coûte **611 858 ns** (`pf3.c`) pour 288 000 ns économisés : **+324 µs de coût mur par exécution** | **rejeté** (déplacement de coût, pas gain réel) |
| Réduire `Valeur` de 24 à 16 octets | `layout_valeur_entier_nu()` refuse la spécialisation si `size_of::<Valeur>() != 24` → retour au chemin Cranelift (~0,9 ms mesuré historiquement) | **rejeté** (nécessite `jit.rs`, interdit) |
| Omettre le 3ᵉ mot (`Option<Rc>` = zéro) | Ne réduit **aucun** page fault (granularité 4 Kio, même stride 24 o) | **rejeté** par construction |

**Décision finale : aucun changement de code adopté.** Avec les fichiers dont je
dispose (`run.rs`, `interp.rs`), il n'existe pas de levier permettant d'agir sur
le stride de 24 octets ni sur l'émetteur machine. La porte D6 < 0,186 ms n'est
**pas atteinte**. Cause chiffrée : 288 µs de faults sur 319 µs, structure
`Valeur`/émetteur dans `jit.rs`.

### 1.5 Temps D6 avant/après — 9 échantillons, c16 ET c6

Harnais focalisé `/tmp/opencode/vit_agents/d6.py`, binaire `rose_base` (identique
à `rose_final`), ordre A/B alterné, valeur contrôlée à chaque tirage :

| c16 — échantillons Rose (ns) | 512514, 367474, 330865, 317905, 319329, 310625, 308897, 308945, 376721 |
|---|---|
| **médiane Rose c16** | **319 329 ns (0,319 ms)** — min 308 897 / max 512 514 |
| **médiane Rust c16** | 186 032 ns — min 180 016 / max 202 945 |
| **ratio c16** | **1,7165 ×** |
| c6 — échantillons Rose (ns) | 522930, 353153, 328689, 339569, 359042, 336705, 323441, 334161, 326529 |
| **médiane Rose c6** | **336 705 ns (0,337 ms)** — min 323 441 / max 522 930 |
| **médiane Rust c6** | 187 440 ns — min 181 921 / max 195 105 |
| **ratio c6** | **1,7963 ×** |

Dans le harnais multi (plus bruité, trous documentés) : c16 **0,337 ms (×1,841)**,
c6 **0,394 ms (×2,039)**. Aucun « après » amélioré n'existe : avant = après =
binaire `f4b4f729…`, par absence de changement retenu.

---

## 2. Racines √2 1000 décimales — même méthode

### 2.1 Ligne de base re-mesurée (harnais `/tmp/opencode/vit_agents/rac.py`, 9+9)

| Pin | Horloge | Rose | Rust (avec cache) | Rose/Rust | checksum |
|---|---|---:|---:|---:|---|
| c16 | murale | 12,856 ms | 12,382 ms | **1,0383** | `500500` des deux côtés |
| c16 | CPU | 12,669 ms | 12,310 ms | **1,0292** | idem |
| c6 | murale | 12,579 ms | 12,152 ms | **1,0351** | idem |
| c6 | CPU | 12,521 ms | 12,105 ms | **1,0344** | idem |

### 2.2 Decomposition du surcoût interprété

Microbenches Rose isolés (`temps()`, 9 échantillons, c16, `N=500`) :

| Programme | médiane | par itération |
|---|---:|---:|
| boucle vide (`$s = $s + 1`) | 656 ns | 1,3 ns |
| 500 × `longueur("abc")` | 83 920 ns | **168 ns** (dispatch + `Valeur`) |
| 500 × `racine_decimales(2,1000)`, résultat ignoré | 11 687 747 ns | 23,4 µs |
| 500 × `longueur(racine_decimales(2,1000))` | 11 678 723 ns | 23,4 µs |

Le coût de `longueur` + conversion chaîne est **indétectable** (−9 024 ns sur
11,7 ms, dans le bruit). Le temps est entièrement dans `calcul_racine`
(`irrationnel.rs`, **hors de mon périmètre**) ; l'écart avec Rust est 0,33 ms
interne (11,66 vs 11,33 ms), soit 0,66 µs/itération, dont seulement 0,168 µs
est de l'overhead de dispatch mesurable.

### 2.3 Variantes testées puis rejetées (preuve réelle, pas d'intention)

| Variante (`run.rs` + `interp.rs`) | Effet mesuré | Décision |
|---|---|---|
| `ChaineRep::plein_ascii` + `Valeur::s_ascii` (1001 `chars()` économisés par appel, chiffres ASCII prouvés) | aucun gain : `rac_full` base = 11 678 723 ns (microbench) ; ratio racines de la variante **1,0528** c16 / **1,0347** c6 contre **1,0383** / **1,0351** pour la base — dans le bruit | **rejetée** |
| `longueur` : `Valeur::z(n as i64)` au lieu de `Gz::neuf()` + `zg_rapide` (sémantique identique pour n ≤ i64::MAX) | idem, aucun gain isolable | **rejetée** |

Les deux ont été **restaurées**. Le binaire final est octet pour octet le binaire
de base (SHA `f4b4f729…`). Aucun gain revendiqué sur les racines.

### 2.4 Double verdict

L'algorithme n'a **pas été modifié** (aucun changement retenu) : le double verdict
de `RELEASE_0_1_6.md` n'a pas lieu d'être invoqué. Verdict « même source » :
**défaite confirmée** 1,0383 (mural c16) / 1,0292 (CPU c16). Verdict « meilleur
algo » (cache du produit `n·10^(2d)`, déjà intégré) : **victoire de bibliothèque
déjà documentée** par ailleurs, non re-mesurée comme telle ici.

---

## 3. Tableau des 9 domaines (binaire final, 9 échantillons, médianes internes)

Harnais `/tmp/opencode/vit_agents/bench_vit_base.py`, 81/81 valeurs contrôlées
à chaque salve (c16 et c6). `< 1 = Rose plus rapide`.

| Domaine | Rose c16 (µs) | Rust c16 (µs) | Ratio c16 | Rose c6 (µs) | Rust c6 (µs) | Ratio c6 |
|---|---:|---:|---:|---:|---:|---:|
| D1 fib(35) | 11 540,2 | 12 681,7 | **0,910** ✅ | 11 659,4 | 12 720,3 | **0,917** ✅ |
| D2 boucle 10⁶ | 98,9 | 262,8 | **0,376** ✅ | 100,6 | 263,7 | **0,382** ✅ |
| D3 premiers 50k | 852,1 | 1 002,1 | **0,850** ✅ | 852,0 | 1 002,2 | **0,850** ✅ |
| D4 fact 5000 | 1 405,1 | 6 536,5 | **0,215** ✅ | 1 410,0 | 6 533,1 | **0,216** ✅ |
| D4b PGCD 1000 ch. | 98,8 | — (Rust Stein « -O » non étiqueté dans ce harnais) | — | 103,0 | — | — |
| D5 chaîne 20k | 10,5 | 14,0 | **0,751** ✅ | 11,9 | 14,4 | **0,829** ✅ |
| D6 liste 100k | 337,4 | 183,2 | **1,841** ❌ | 394,1 | 193,3 | **2,039** ❌ |
| D7 table 3000 | 92,0 | 93,3 | **0,986** ✅ | 100,1 | 95,9 | **1,043** ❌ (bruit) |
| D8 E/S 10 Mo | 3 038,6 | 5 428,8 | **0,560** ✅ | 3 305,8 | 6 007,0 | **0,550** ✅ |

Aucune régression introduite : les ratios sont ceux de l'arbre d'origine, re-mesurés.
D4b est un domaine où l'adversaire du harnais est Stein (comparatif : 0,676) ; le
harnais multi ne l'étiquette pas `Rust -O`, d'où le tiret — aucune valeur n'est
inventée.

---

## 4. Validation de l'arbre final

| Contrôle | Sortie réelle |
|---|---|
| `cargo build --release --bin rose` | `Finished release …` ; SHA `f4b4f729…` |
| `cargo test --release` | **220 + 204 + 0 + 6 + 1 = 431 passed, 0 failed** (3+3 ignorés) — `/tmp/opencode/vit_agents/cargo_test.log` |
| Jardin `jardins/tests/premiers.rose` | `── jardin/premiers : 20 tests, 0 échec ──` → **20/20** |
| fib25 | `75025` |
| Point fixe avec le fichier **HEAD** `compilateur/emetteur.rose` | `8ac53eed3d4967a0d7d9ab333256801c` / `8ac53eed3d4967a0d7d9ab333256801c` → **×2** ✅ |
| ⚠ Point fixe avec le fichier **de travail** | `b3ed9262dd54776a6dbd07bc12368d0e` ×2 : le ×2 tient toujours, mais le hash diffère parce qu'un **autre agent** a ajouté 5 lignes à `compilateur/emetteur.rose` (hors de mon périmètre, non modifié par moi) |
| `bench_global.rose` | `rc=0`, sortie `6250000 / 317811 / 1000000 / bench global Rose / 437` |
| Multi 9 domaines | 81/81 valeurs exactes par salve, c16 et c6 |
| `git status` pour mes fichiers | `run.rs`, `interp.rs` : **aucune modification** ; aucun commit, aucune publication ; `rose/rose` intact |

---

## 5. Ce qu'il faudrait pour réellement battre Rust sur D6

1. **Assouplir `layout_valeur_entier_nu()` (`jit.rs`)** pour spécialiser aussi une
   `Valeur` 16 octets, ou ajouter un mode liste « i64 nu » où la boucle n'écrit
   que 8 octets/élément : 196 pages au lieu de 586, soit −288 µs mesurables.
2. **Ou** faire émettre un `str` de zéro omis + un pas de 16 octets avec un
   `Valeur` restructuré — nécessite `jit.rs` **et** `run.rs` de concert.
3. Ne **pas** tenter le pré-fault : 611 858 ns payés pour 288 000 ns gagnés
   (mesuré), ni `MAP_POPULATE` (aucun gain mesuré).

**Bilan honnête : aucun domaine perdu n'a été gagné lors de ce chantier.**
La contribution mesurable est une décomposition chiffrée (90 % page faults sur
D6) et la **correction** d'un chiffre de comparatif périmé (0,374 ms → 0,319 ms
au binaire reconstruit). Toutes les pistes ouvertes dans mes fichiers ont été
testées puis rejetées à la mesure, et l'arbre a été restauré.
