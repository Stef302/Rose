# COMPARATIF_TOTAL — « comparatif en tout point » (CONSTAT DATÉ, 19/11/2026)

**Auteur** : sous-agent `cmp2`. **Aucun commit, aucune publication. `rose/rose` intact,
aucun `.rs`, `Cargo.toml`, `PLAN.md` ni doc d'un autre agent modifié.** Binaire construit
dans `/tmp/opencode/cmp2_agents/`, jamais installé dans l'arbre.

---

## Mise à jour datée — 21/11/2026 (HEAD poussé `902fbbf`)

Les paragraphes datés 19/11 (§0–§9) et la mise à jour 20/11 juste dessous sont
**conservés comme historique**, pas effacés. Les chiffres ci-dessous sont des
**constats datés**, cités avec leur source (commit ou doc d'agent) et leur date :
aucun n'est une règle permanente, et aucun n'autorise à écrire « Rose bat partout ».

### Self-hosting — passage majeur : PARTIEL → ATTENT (émetteur)

Le verdict « PARTIEL, émetteur ELF en échec fonctionnel » du §6 (19/11) est **périmé**.

- `0e2d844` « Self-hosting atteint : le compilateur généré reproduit le RIR exact »
  (20/11) : l'émetteur auto-hébergé produit un ELF de **534 780 octets**, md5
  `ed699457a9b8cd0ea69a53bbae78fd39`.
- `e0f5744` « Preuve : self-hosting reproductible depuis l'arbre commité » (20/11) :
  chaîne complète rejouée depuis le fichier commité, md5 `ed699457…` reproduit à
  l'identique. L'ELF émis lit une **source `.rose`** et rend le dump RIR
  `b3ed9262dd54776a6dbd07bc12368d0e` (263 520 octets) **octet pour octet identique**
  à la référence.
- Source détaillée : `rose/docs/SELF_HOST_SH3.md`, §1 et « Réémission indépendante
  (vérification orchestrateur, 20/11/2026) ».

**Précision obligatoire** : c'est **l'émetteur** qui est auto-hébergé, et reproductible
depuis l'arbre commité. Le **binaire `rose/rose` complet — interpréteur, JIT, runtime —
exige toujours `cargo build`** et n'est pas auto-hébergé. L'entrée de l'ELF émis est une
source `.rose` ; lui passer un dump RIR donne `rc=139` (comportement attendu, déjà noté
par SH2, pas une régression). Le §6 ci-dessous reste vrai pour le 19/11.

### Banc principal 9 domaines — 9/9 gagné (constat 20/11, réétiqueté daté)

Source : `rose/docs/ETAT_VICTOIRE_9_DOMAINES.md` (20/11, commit `dee9378`), binaire
SHA256 `a2f98fe93799e2faa69a21a486bcd4b7bc7def976d7b62181c0c6c95ca04ff3c`,
banc `bench_now.py`, `taskset -c 19` puis `-c 6`, 9 échantillons entrelacés, deux
horloges, sortie vérifiée à chaque tirage. Ratios Rose/Rust (calcul seul, médianes ;
**< 1 = Rose plus rapide**) :

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

Ces ratios **contredisent le tableau §1 et la §4 du 19/11** (D6 liste à 1,698 ; D7
table à 0,988 ≈) : sur le binaire du 20/11, liste et table sont gagnées. Les ratios
c11 du §1 ne sont pas l'état courant. Les 9 ratios **ne couvrent pas** les autres bancs
(global, englobant, racines spécialisées, langages) : ne pas en déduire une victoire
universelle.

### Global — ×414 → ×2,85 → ×2,88 / ×2,57 (final GLB3, 20/11)

- ×414 : §1 ligne 15 (constat 19/11, c11).
- ×2,85 : `10b9df4` + doc `bbf1af0` (20/11) ; le tableau §2.4 ci-dessous reste celui du 19/11.
- **×2,88 (c16) / ×2,57 (c6)** : état final du programme global, `DEMARRAGE.md` §3.4
  (sous-agent `glb3`, 20/11, commit `951bb12`). **Rust reste devant.**

Détails glb3 (`DEMARRAGE.md`, 20/11) : carrés **1,26 → 0,76 ms** (superinstruction NEON
`umlal`, ÷1,66) ; compilation Cranelift `compiletout` **777 → 337 µs** sur c16 (÷2,30),
841 → 357 µs sur c6 (÷2,36) ; pré-exécution ÷1,98 (c16) / ÷2,04 (c6). Le poste complet
« mur − interne » n'est **pas** divisé par 2 (×1,41 c16 / ×1,22 c6).
Détail glb1 (`GLOBAL_FINAL.md`, 20/11, `4aad1d8`) : section rationnelle **10,75 → 0,79 ms**
(÷13,6) ; à ce stade le global complet restait 4,25–4,41× Rust au mur.

**Réserve sémantique majeure, maintenue** (`bbf1af0`, mise à jour 20/11 de ce doc,
réjouée) : `bench_global_rs` rend ligne 1 `1291990006563070912` (**débordement `i64`**)
et ligne 3 `7.818804193883677` (**`f64`**), tandis que Rose rend
`333333383333335000000` et la fraction exacte. La comparaison est **non équivalente** :
battre Rust exigerait d'adopter ses erreurs numériques — **refusé**. Le gain réel
×414 → ×2,88/×2,57 porte sur un travail qui n'est pas le même. La ligne 3 du §2.4
(« Rose `0` ») est elle-même périmée (`f9b0966` + `4aad1d8`).

### Suite complexe — ×203 → ×1,86 → ×1,15 → écart réduit, porte NON atteinte

- ×203 : §1 ligne 18 (constat 19/11).
- ×1,86 : CPLX1 (déjà consigné dans la mise à jour 20/11 ci-dessous).
- **×1,15 (c6) / ×1,13 (c16)** : `SUITE_BEAT.md` (sous-agent `sui1`, 20/11, `745bb9c`),
  base 2,166 → v3 1,598 ms sur c6 contre Rust 1,388 ms, 60 échantillons A/A/A ;
  **Rose ≤ Rust NON atteint**.
- **CLOSE1** (`8c31a38`, `CLOSURE.md`, 20/11) : LTO + lecture source unique réduisent
  l'écart résiduel de ~57 à 80 µs ; la meilleure recette (PGO) reste à **+131 à +146 µs**
  CPU sur les trois cœurs. **Porte NON atteinte.**
- Le titre du commit `8c31a38` (« écart suite ×1,19→×0,9 sur une partie ») est
  **imprécis** : aucune salve n'a montré Rose ≤ Rust. Rectifié par `ef7464e` et en tête
  de `CLOSURE.md`.

### Englobant — ×109 → ×2,49 → ×1,40–1,62, porte NON atteinte

- ×109 : §1 ligne 16 (constat 19/11).
- ×2,49 : mise à jour 20/11 de ce doc (constat INT2).
- **×1,40 (c19) / fourchette ×1,40–1,62 sur 4 salves** : `ENGBEAT.md` (sous-agent `eng1`,
  20/11). Rose final 1,381 ms c19 / 1,676 ms c6 contre Rust 0,990 / 1,036 ms.
  Superinstruction IEEE S4 : 764 → 14,4 µs. **Porte Rose ≤ Rust NON atteinte.**

### Ce qui reste valide et ce qui est périmé ici

- **Défaites documentées maintenues** : global, suite complexe et englobant
  (**Rust devant dans les trois cas**), racines spécialisées, langages, D1 fib35 au
  binaire 19/11, D6/D7 au binaire c11 du 19/11. Aucune réécriture en « Rose bat partout ».
- **Périmés comme état courant** : le §1 (pinage c11, ratios 19/11), le §2.4 (global,
  ligne 3 à `0`), le §6 (self-hosting PARTIEL / émetteur en échec), le §3 et la §4
  (D6 perte), et la ligne « ×2,85 / ×2,89 » de la mise à jour 20/11 pour le global final.
- Le binaire du 20/11 (`a2f98fe9…`) diffère du binaire c11 (`f4b4f729…`) de la salve
  19/11 : ne pas mélanger les époques ni les cœurs.

---

## Mise à jour datée — 20/11/2026 (constat rejoué, remplace les chiffres périmés)

Les valeurs ci-dessous ont été **re-mesurées** après les commits `5cfc9db` (liste),
`065a0d2` (table), `a44dcb5` (complexes), `43d1884` (englobant), `4aad1d8` (rationnels)
et `10b9df4` (tables/listes/chaînes/carrés). Les tableaux des §2.4 et suivants sont
**périmés** ; ils sont conservés comme historique, pas comme état courant.

| Banc | Avant | Maintenant | Verdict honnête |
|---|---:|---:|---|
| Banc principal, 9 domaines | 7/9 gagnés | **9/9 gagnés** (ratios 0,21–0,92, c6 et c19) | victoires moteur sur source identique |
| Suite complexe | ×203 | **×1,86** | presque à égalité, Rust devant |
| Englobant | ×109 | **×2,49** | Rust devant (mesures c19) |
| Global (8 domaines) | ×414 | **×2,85 / ×2,89** (mur c16/c6) | Rust devant, **comparaison non équivalente** (voir plus bas) |

**Global — divergence sémantique confirmée par exécution directe le 20/11** :
`bench_global_rs` rend ligne 1 `1291990006563070912` (**débordement `i64`**) et ligne 3
`7.818804193883677` (**`f64`**), tandis que Rose rend `333333383333335000000` et la
fraction exacte sur ~800 chiffres. Le rapport ×2,85 mesure donc un travail **différent** :
Rose calcule exact, Rust tronque. La ligne 3 du §2.4 qui annonçait Rose `0` est **périmée**
(corrige_display rationnel `f9b0966` + voie rapide `4aad1d8`). **Battre Rust sur ce banc
exigerait d'accepter ses erreurs numériques — refusé.** Le gain réel reste ×414 → ×2,85.

**Structure du resteGlobal** (GLB2, 20/11) : mur Rose ~7,15 ms = 4,17 ms internes +
~2,98 ms parse/JIT/démarrage ; Rust ~2,50 ms = 1,39 + 1,11. Le jumeau Rust compile
`total += i*i` sur 10⁷ itérations en ~20 ns par LLVM (forme fermée sur induction
polynomiale), contre 1,26 ms chez Rose qui exécute la boucle. Plancher arithmétique
même en annulant tout le reste : 4,24 ms > 2,50 ms. La porte est structurellement
fermée **sans** adopter soit la forme fermée, soit la sémantique tronquée.

---

## 0. Protocole — ce qui a réellement été exécuté

| Élément | Valeur mesurée |
|---|---|
| HEAD annoncé par la mission | `e0ec2871d273d9de8df7546f0675a9aef5517aad` |
| HEAD à la fin des mesures | `f564545e5fd86e9de24442c767f0ca97ff20549c` (3 commits d'autres agents sont tombés pendant la session). **`git diff e0ec287..HEAD -- rose/src` = vide** : aucun `.rs` n'a changé, le moteur mesuré est le même. Seuls `compilateur/*.rose` et des docs ont bougé. |
| Binaire Rose utilisé | `/tmp/opencode/cmp2_agents/rose_cmp2`, `cargo build --release --bin rose` depuis l'arbre courant, copié. SHA256 `f4b4f729e2f267f9d9c79a0e1a40c7154c3bde43201ddc7648cd16bef9a5ec65`, vérifié avant/après toutes les mesures. |
| Pinage | **`taskset -c 11`** (mission). ❗ **c11 = Cortex-A725, `cpu_capacity=731`** (cœur « petit », max 2,808 GHz). Les documents antérieurs pinés c16 mesurent un **Cortex-X925, `cpu_capacity=1017`** (max 3,9 GHz). A/A sur `fib(35)` : **31,0 ms sur c11 contre 11,7 ms sur c16** → les chiffres de ce document **ne sont pas comparables** à ceux de `COMPARATIF_FINAL.md`. |
| Échantillons | **9 par implantation et par comparaison**, entrelacés par rotation (et alternance A/B à chaque répétition pour les racines). Aucune mesure isolated. |
| Horloges | **deux, toujours étiquetées** : H1 = `time.perf_counter()` autour du sous-processus (mur) ; H2 = « calcul seul » = `temps()`/`Instant`/`perf_counter` interne du programme **ou** `getrusage(RUSAGE_CHILDREN)` pour les bancs sans horloge interne Rose (racines, englobant, suite). |
| Valeurs | **vérifiées à chaque tirage** : multi 81/81, racines 18/18 par comparaison (90/90), global 11 lignes ×3 ×9 = 297 contrôles, englobant 4 valeurs ×4 ×9 = 144, langages 4×9 = 36, suite 1 f64 ×4 ×9 = 36, backends 9/9 par backend (63). |
| Ratios | `Rose / adversaire` ; **< 1 = Rose plus rapide**. Médianes, min–max entre parenthèses. |
| Fenêtre | `2026-11-19T21:32:09Z` → `2026-11-19T21:39:12Z` (benches) ; validations jusqu'à `22:45Z`. |
| Règle des deux verdicts | **(a) même source** = même programme/même algorithme des deux côtés → mesure le **moteur**. **(b) meilleur algo** = on nomme l'algorithme le plus rapide même s'il diffère. Une victoire de **primitive native** est comptée **à part**, jamais comme victoire « même source ». |

**Ce document est un constat daté** : re-mesurer après tout changement de moteur, de binaire,
de noyau de pinage ou de topologie. Aucune victoire n'est revendiquée sans mesure rejouée ici.

---

## 1. Tableau synthétique unique — 18 lignes, 9 échantillons, c11

H2 partout : interne programme quand les 4 langages l'exposent, sinon CPU `getrusage`.
`(min–max)` sur la médiane Rose uniquement quand l'espace le permet ; le détail par
implémentation est au §2.

| # | Banc / domaine | H2 (ms) Rose méd | H2 (ms) adversaire méd | Rose/Rust | (a) même source | (b) meilleur algo (nommé) | Nature | Qui gagne |
|---:|---|---:|---:|---:|---|---|---|---|
| 1 | **D1 `fib(35)`** (multi) | 30,940 (30,906–30,988) | 27,913 Rust · 21,232 C | **1,108** ❌ | **perte** : même récursion naïve des deux côtés | perte ; primitive `fibonacci()` **0,014 ms** = **bibliothèque, comptée à part** | moteur (codegen récursion) | **Rust**, et **C aussi** |
| 2 | **D2 boucle 10⁶** (multi) | 0,219 (0,218–0,228) | 0,562 Rust · 0,376 C | **0,389** ✅ | **victoire** : source identique, le JIT réassocie | **victoire** : 8 accumulateurs + déroulage ×8, **auto-appliqués, source inchangée** | **moteur + algo** | **Rose** (H2) ; H1 murale Rust 2,833 < Rose 3,357 |
| 3 | **D3 premiers 50k** (multi) | 1,370 (1,363–1,430) | 1,552 Rust · 1,502 C | **0,883** ✅ | **victoire** : même division d'essai | perte vs crible natif `compter_premiers()` **0,132 ms** = **bibliothèque, à part** | moteur (6 `udiv` supprimés) | **Rose** |
| 4 | **D4 `5000!`** (multi) | 2,206 (2,203–2,220) | 12,122 Rust · 0,604 Python | **0,182** ✅ | **victoire** : même produit itératif | perte vs arbre Rose **0,592 ms** et `math.factorial` **0,604 ms** | moteur (chemin bigint) | **Rose vs Rust/C** ; **Python gagne** |
| 5 | **D4b PGCD 1000 ch.** (multi) | 0,179 (0,177–0,185) | 0,284 Rust (Stein) | **0,629** ✅ | **N/A** : Euclid (Rose) ≠ Stein (Rust) | **victoire** : Stein primitif Rose **0,099 ms** < Stein Rust 0,284 ; Python `math.gcd` **0,018 ms** bat tout | **bibliothèque** | **Rose vs Rust** ; Python gagne |
| 6 | **D5 chaîne 20k** (multi) | 0,021 (0,017–0,057) | 0,024 Rust (0,023–0,031) | **0,869** ⚠ | **victoire fragile, non revendiquée** : plages superposées ; H1 murale **Rose 2,186 > Rust 1,441 (1,52)** | même, dans le bruit | moteur (JIT `str::repeat`) | **indécis** |
| 7 | **D6 liste 100k** (multi) | 0,487 (0,463–0,511) | 0,287 Rust · 0,276 C | **1,698** ❌ | **perte nette** | **perte**, même avec le chemin inline spécialisé | aucun (90 % page faults, §4) | **Rust et C** |
| 8 | **D7 table 3000** (multi) | 0,171 (0,163–0,193) | 0,173 Rust HashMap | **0,988** ≈ | **égalité dans le bruit** (plages superposées) | le hachage gagne : `Vec assoc` Rust 1,944 ms, `Vec assoc` C 1,875 ms | bibliothèque | **égalité** |
| 9 | **D8 E/S 10 Mo** (multi) | 5,433 (5,083–5,969) | 7,446 Rust · 7,588 C | **0,730** ✅ | **victoire** : `lire/ecrire_texte` vs `fs::read/write` | **victoire**, pas de meilleur algo revendiqué | **bibliothèque / syscall** (mmap zéro recopie) | **Rose** |
| 10 | **Racines √2 1000 déc., N=500, OLD sans cache** | 20,040 (19,919–20,508) | 20,942 Rust | **0,957** ✅ | **N/A** : algo différent (cache Rose) | **victoire** : cache du produit pur `n·10^(2d)` | **bibliothèque** (cache, pas mémoïsation du résultat) | **Rose** |
| 11 | **Racines √2 1000 déc., N=500, NEW avec cache** | 20,112 (19,915–20,302) | 19,284 Rust | **1,043** ❌ | **perte** : même algo des deux côtés | perte | surcoût d'interprétation annule le cache | **Rust** |
| 12 | **Racines i64, N=2 000 000** | 975,364 (930,150–986,186) | 11,420 Rust | **85,41** ❌ | **perte** | perte | 1 appel de primitive interprété par itération | **Rust** |
| 13 | **Racines BigInt, N=20 000** | 25,659 (25,416–25,939) | 13,921 Rust | **1,843** ❌ | **perte** | perte | chemin `d=0` (pas de cache) | **Rust** |
| 14 | **Racines √2 100 déc., N=5 000** | 14,704 (14,510–14,908) | 13,096 Rust | **1,123** ❌ | **perte** | perte | cache +7,9 % interne côté Rust, insuffisant | **Rust** |
| 15 | **Global, 8 domaines** | 1 088,000 (H1 mur : 1 092,961) | 2,631 Rust (H1 : 4,570) | **413,6** ❌ (mur **239,2**) | **perte** : structure identique, sémantique numérique différente | perte | **bibliothèque** : BigInt/rationnel exacts vs `i64`/`f64` | **Rust** ; Rose bat Python (1 616,108) de **1,49×** en mur |
| 16 | **Englobant, 4 sections** (source réparée, §2.5) | 181,635 (180,716–182,531) | 2,154 Rust · 3,303 C++ | **84,33** ❌ | **perte** : 4 valeurs **identiques** partout (parité prouvée) | perte | moteur (boucles scalaires + RNG + Philox) | **Rust, C++** ; Rose bat Python (479,555) **2,64×** |
| 17 | **Langages `remplir_hasard`, N=10⁷** | 8,000 (7–8) | 5,000 Rust · 6,000 C++ | **1,600** ❌ | **perte** : même splitmix64, 4 tirages/lot | perte | moteur (dispatch de la primitive) | **Rust, C++** ; Rose bat `rand()` glibc (162) **20×** |
| 18 | **Suite complexe 10⁶** | 772,429 (769,571–787,844) | 2,876 Rust · 2,908 C++ | **268,6** ❌ | **perte** : mêmes formules, f64 **bit-égal** | perte | moteur (type complexe construit par itération) | **Rust, C++** ; Rose bat Python (547,506) **1,41×** |

**Bilan brut des ratios Rose/Rust < 1** : **7 lignes** (2, 3, 4, 5, 6⚠, 8≈, 10)
dont **1 indécise (D6/D5 bruit)** et **1 à algo différent (D4b)** ; **10 défaites** ;
**1 égalité** (D7). Sur les 15 lignes où le verdict « même source » est applicable :
**5 victoires (4 robustes + D5 fragile), 10 défaites**. Sur les 18 lignes « meilleur algo » :
**4 victoires Rose** (2, 5, 9, 10), **1 égalité** (8), **13 défaites** ; **3 victoires de
bibliothèque supplémentaires comptées à part** (1 : `fibonacci`, 3 : `compter_premiers`,
4 : `factorielle`).

---

## 2. Tableaux par banc

### 2.1 Multi — `/tmp/opencode/benchmulti/bench.py` (9 domaines)

`PIN_CPU=11 NSAMP=9 python3 /tmp/opencode/cmp2_agents/bench_cmp2.py`.
Chaque cellule = médiane (min–max) en ms. H2 = horloge interne du programme.

| Domaine | Implémentation | H2 méd (min–max) | H1 mur méd (min–max) | H2 / Rose |
|---|---|---:|---:|---:|
| D1 fib35 | **Rose JIT source** | 30,940 (30,906–30,988) | 33,114 (32,659–34,115) | 1,000 |
| | Rust -O | 27,913 (27,796–27,952) | 29,498 (28,885–30,530) | 0,902 |
| | Python récursif | 1 239,610 (1 238,991–1 244,088) | 1 259,505 | 40,065 |
| | C -O2 | 21,232 (21,217–21,290) | 22,503 | 0,686 |
| | Rose `fibonacci()` | 0,014 (0,013–0,014) | 2,507 | 0,00045 |
| D2 boucle 10⁶ | **Rose JIT source** | 0,219 (0,218–0,228) | 3,357 (2,081–4,068) | 1,000 |
| | Rust -O | 0,562 (0,562–0,631) | 2,833 | 2,572 |
| | Python | 40,289 | 58,793 | 184,3 |
| | C -O2 | 0,376 (0,364–0,384) | 2,346 | 1,720 |
| | Rose natif M3.1 | 4,602 | 7,076 | 21,05 |
| D3 premiers 50k | **Rose JIT source** | 1,370 (1,363–1,430) | 4,899 | 1,000 |
| | Rust -O | 1,552 | 3,174 | 1,133 |
| | Python division | 31,360 | 50,407 | 22,88 |
| | C -O2 | 1,502 | 2,873 | 1,096 |
| | Rose crible natif | 0,132 (0,129–0,139) | 1,910 | 0,096 |
| D4 5000! | **Rose JIT boucle** | 2,206 (2,203–2,220) | 6,177 | 1,000 |
| | Rust bigint maison | 12,122 | 13,320 | 5,495 |
| | `math.factorial` | 0,604 | 26,022 | 0,274 |
| | C bigint maison | 12,097 | 13,257 | 5,484 |
| | Rose arbre natif | 0,592 | 4,261 | 0,268 |
| D4b PGCD 1000 ch. | **Rose Euclid interprété** | 0,179 (0,177–0,185) | 1,802 | 1,000 |
| | Rose `pgcd()` Stein | 0,099 | 1,718 | 0,555 |
| | Rust Stein maison | 0,284 | 1,697 | 1,589 |
| | Python `math.gcd` | 0,018 | 18,328 | 0,099 |
| | C Stein maison | 10,038 | 11,319 | 56,22 |
| D5 chaîne 20k | **Rose `+`** | 0,021 (0,017–0,057) | 2,186 (1,590–3,053) | 1,000 |
| | Rust `push_str` | 0,024 (0,023–0,031) | 1,441 | 1,151 |
| | Python `+=` | 0,880 | 19,731 | 42,71 |
| | C `strcat` | 4,815 | 6,080 | 233,6 |
| D6 liste 100k | **Rose joindre/index** | 0,487 (0,463–0,511) | 3,656 | 1,000 |
| | Rust `Vec` | 0,287 | 1,823 | 0,589 |
| | Python `append` | 3,496 | 23,409 | 7,180 |
| | C tableau `malloc` | 0,276 | 2,229 | 0,567 |
| D7 table 3000 | **Rose table** | 0,171 (0,163–0,193) | 3,654 | 1,000 |
| | Rust `HashMap` | 0,173 (0,167–0,185) | 2,488 | 1,012 |
| | Rust `Vec assoc` | 1,944 | 4,086 | 11,40 |
| | Python `dict` | 0,479 | 20,402 | 2,807 |
| | Python liste assoc | 147,220 | 165,844 | 863,1 |
| | C assoc linéaire | 1,875 | 3,960 | 10,99 |
| D8 E/S 10 Mo | **Rose lire/ecrire_texte** | 5,433 (5,083–5,969) | 8,318 | 1,000 |
| | Rust `fs::read/write` | 7,446 (6,040–8,411) | 9,765 | 1,370 |
| | Python `open` | 6,933 | 27,355 | 1,276 |
| | C `fread/fwrite` | 7,588 | 9,211 | 1,397 |

Résultats vérifiés : 81/81 (`9227465`, `499999500000`, `5133`, `16326`, `500`, `20000`,
`4999950000`, `3000`, `10485760`). Horodatage : `21:32:09.841830Z` → `21:32:26.941795Z`.
CSV/JSON : `/tmp/opencode/cmp2_agents/cmp2_results.csv`, `cmp2_summary.json`.

### 2.2 Racines — `bench/racines/harness.py` (copie c11 + vérification stricte)

Adversaires figés : `rust_ref_OLD` (md5 `46c284009acd47d8cb828fffe36ec822`, sans cache)
et `rust_ref_NEW` (md5 `ecc557a11e8c80df810012925b2fd848`, avec cache). Alternance A/B
à chaque tirage, checksums vérifiés **18/18 par comparaison**.

| Comparaison | Rose H1 mur méd (min–max) | Rust H1 mur méd | A/B mur | Rose H2 CPU méd | Rust H2 CPU méd | A/B CPU |
|---|---:|---:|---:|---:|---:|---:|
| A `decs 500 1000` OLD | 20,164 (20,005–20,795) | 21,042 | **0,9583** | 20,040 | 20,942 | **0,9569** |
| B `decs 500 1000` NEW | 20,284 (20,061–20,537) | 19,448 | **1,0430** | 20,112 | 19,284 | **1,0429** |
| C1 `i64 2000000` | 975,789 (930,348–986,578) | 11,549 | **84,49** | 975,364 | 11,420 | **85,41** |
| C2 `big 20000` | 25,758 (25,487–26,035) | 14,001 | **1,840** | 25,659 | 13,921 | **1,843** |
| C3 `decs 5000 100` | 14,758 (14,571–15,105) | 13,226 | **1,116** | 14,704 | 13,096 | **1,123** |

Checksums contrôlés : `500500`, `4000005000000`, `500000`, `505000` côté Rose ;
`500500 1000`, `4000005000000 100`, `500000 100`, `505000 100` côté Rust.
Le gain du cache est **7,9 % côté Rust** (20,942 → 19,284) et **nul côté Rose**
(20,040 → 20,112) : le surcoût d'interprétation absorbe exactement le gain de bibliothèque.

### 2.3 Backends — `bench/backends/run_backends.sh` (pilote Python c11)

7 backends construits, 1 échauffement non compté, 9 échantillons entrelacés par rotation.
`CHECKSUM 2666664666667886537` commun et 6 valeurs identiques partout, 9/9 par backend.

| Backend | Mur méd (min–max) ms | arith int | recur int | chaîne int | liste int | table int |
|---|---:|---:|---:|---:|---:|---:|
| **JIT Cranelift (défaut)** | 47,198 (46,778–48,210) | 25,048 | 2,785 | 4,990 | 4,137 | 5,215 |
| Interpréteur `ROSE_SANS_JIT=1` | 651,853 (650,744–653,096) | 134,612 | 500,418 | 4,844 | 3,697 | 5,006 |
| `ROSE_NO_RIR=1` | 46,990 (46,723–47,235) | 25,078 | 2,789 | 5,142 | 4,088 | 5,223 |
| Émetteur natif `ROSE_NATIF=1` | 665,867 (665,662–671,401) | **10,218** | 637,795 | 5,246 | 4,102 | 5,289 |
| `rose comp` (exécutable autonome) | 48,364 (47,886–50,789) | 25,078 | 2,786 | 4,923 | 3,921 | 5,106 |
| AOT LLVM ORC `--link` (noyau seul) | **6,054** (5,851–7,050) | n/a | n/a | n/a | n/a | n/a |
| AOT LLVM classique (noyau seul) | 515,257 (512,539–517,181) | n/a | n/a | n/a | n/a | n/a |

Deux sondes **non mesurées** : `selfhost` et `rvm` — voir §5 et §6. Détails des horloges
internes (médiane [min..max] ms) dans `/tmp/opencode/cmp2_agents/backends_run.log`.

### 2.4 Global — `bench/global/bench_global.rose` (8 domaines)

`build_global_c11.py` : 3 implémentations ×9, rotation, **11 lignes de valeurs vérifiées
à chaque tirage** (la 12ᵉ est un temps). Le jumeau **C++ n'a pas pu être rejoué** :
`rose_runtime.cpp` exige GMP/gmpxx, absents (`/tmp/opencode/gmp-linux` inexistant,
`ldconfig -p | grep libgmp` vide, aucun `gmpxx.h` sur la machine) — `g++ …` échoue sur
`undefined reference to __gmpq_set` etc. **Aucun temps C++ n'est inventé.**

| Implémentation | H1 mur méd (min–max) ms | H2 interne méd (min–max) ms | H1 / Rose | H2 / Rose |
|---|---:|---:|---:|---:|
| **Rose JIT** | 1 092,961 (1 092,262–1 105,483) | 1 088,000 (1 088–1 100) | 1,000 | 1,000 |
| Rust -O | 4,570 (4,062–5,606) | 2,631 (2,582–2,725) | **0,004** | **0,002** |
| Python3 | 1 648,951 (1 622,704–1 684,564) | 1 616,108 (1 594,970–1 653,267) | 1,509 | 1,485 |

❗ **Divergences de valeurs observées, non masquées** (constats rejoués 9×) :
* ligne 1 : Rose `333333383333335000000` (exact) — Rust `1291990006563070912`
  (**débordement `i64`** volontaire du jumeau) — Python `333333383333335000000`.
* ligne 3 (somme des `1/c`) : Rose **`0`** — Rust et Python `7.818804193883677`.
  Reproduction courte : `rose /tmp/opencode/cmp2_agents/probe_rat.rose` imprime `0.[3]`
  puis `0` après 1001 itérations ; le seuil observé se situe entre N=50 (affichage
  rationnel complet) et N=100 (`0`). Le type reste `rationnel`. **Je ne conclus pas à
  une erreur de calcul interne** (non mesuré) : je constate une **divergence d'affichage**
  du rationnel sur ce banc.
* Le 239× (mur) n'est donc **pas un verdict de moteur pur** : il inclut une différence
  de sémantique numérique (BigInt/rationnel exacts contre `i64`/`f64`).

### 2.5 Englobant — `bench/englobant` (4 sections, 4 moteurs)

❗ **Le source commité ne tourne plus sur le moteur courant** :
`rose bench_englobant.rose 4096 1` →
`Rose : erreur — [Ligne 60, Colonne 0] La variable de boucle 'i6' existe déjà`
(idem `rose/rose`, binaire antérieur). Cause : `$i6 = 0` (l.54) et `$i7 = 0` (l.84)
sont des pré-affectations de variables ensuite utilisées comme variables de `pour`.
Reproduit aussi avec `pour i … ; pour i …` minimal.

Copie minimale réparée **uniquement dans mon répertoire**
(`/tmp/opencode/cmp2_agents/bench_englobant_fixed.rose`) : **2 lignes supprimées**,
rien d'autre. `diff -u` exact :

```
-$i6 = 0
-$i7 = 0
```

Mesures sur cette copie, N=4096, F=1, 9 échantillons, c11. Parité des 4 valeurs
(`46297232.426252`, `564`, `157158`, `11899223598484`) **144/144**.

| Moteur | H1 mur méd (min–max) ms | H2 CPU méd (min–max) ms | temps interne méd ms |
|---|---:|---:|---:|
| **Rose JIT (copie réparée)** | 181,834 (181,046–182,863) | 181,635 (180,716–182,531) | — (source sans horloge) |
| C++ `-O3 -ffp-contract=off` | 3,553 (3,244–4,579) | 3,303 | 1,530 |
| Rust `-O3 target-cpu=native` | 3,166 (2,068–4,173) | **2,154** | 0,980 |
| Python3 | 479,862 (476,611–482,292) | 479,555 | 460,012 |

Note : le script `run_englobant.sh` du dépôt ne peut pas être utilisé tel quel avec le
moteur Rust : il appelle `rose build`, commande absente de `rose/rose` (`rose: fichier
introuvable : build`). Le harnais Python écrit pour l'occasion ne modification aucun
fichier du dépôt et rejoue les mêmes sources C++/Rust/Python.

### 2.6 Langages — `bench/langages/bench.sh` (mot de passe, N=10⁷)

Le défaut du script (10⁹, ~1 Go par tirage) est hors périmètre. 4 moteurs ×9,
longueur du tampon vérifiée 36/36.

| Moteur | H1 mur méd (min–max) ms | H2 CPU méd ms | H2 interne méd (min–max) ms |
|---|---:|---:|---:|
| **Rose JIT** | 10,338 (9,963–11,240) | 10,056 | 8,0 (7–8) |
| C++ splitmix64 | 8,838 (8,389–9,511) | 8,325 | 6,0 |
| Rust -O splitmix64 | 7,616 (7,428–8,343) | 7,271 | 5,0 |
| C `rand()` glibc | 163,988 (163,641–164,695) | 163,767 | 162,0 |

Annexe hors protocole (la sortie ne contient que des ms, aucune valeur vérifiable) :
`python3 bench/langages/mdp_numpy.py 10000000` = **61 ms** pour 1 tirage.

### 2.7 Suite — `bench/suite/complexe.rose` (noyau complexe 10⁶)

`float(ligne1) == 0.9999999999860738` vérifié 36/36 (C++ imprime `…07381`, même f64).
La 2ᵉ ligne n'est pas comparée : Rose `333333833333500000` (BigInt exact) contre
`333333.3333…` (f64) chez les jumeaux.

| Moteur | H1 mur méd (min–max) ms | H2 CPU méd (min–max) ms |
|---|---:|---:|
| **Rose JIT** | 772,812 (769,820–788,108) | 772,429 (769,571–787,844) |
| C++ -O3 | 3,150 (2,776–4,584) | 2,908 |
| Rust -O | 3,858 (2,824–4,151) | 2,876 |
| Python3 | 547,724 (531,278–563,222) | 547,506 |

Les autres noyaux de `bench/suite/*.rose` (fibonacci, factorielles, sieve, matrix,
montecarlo, philox, power, reduction, sumsquares) et `bench/dores/**` (tests de
syntaxe, pas des bancs chronométrés) n'ont **pas** été rejoués : périmètre de cette
salve limité aux bancs avec jumeaux comparables + les bancs nommés par la mission.

---

## 3. Victoires (chiffrées, sans extrapolation)

| Victoire | Mesure c11 | Verdict | Pourquoi c'est réel |
|---|---|---|---|
| **D2 boucle 10⁶** | 0,219 vs 0,562 ms (×2,57) | (a) + (b) | Source strictement identique ; 8 accumulateurs + déroulage ×8 appliqués par le JIT. **Seule victoire « moteur + algo » incontestable de la salve.** |
| **D3 premiers 50k** | 1,370 vs 1,552 ms (×1,13) | (a) | Même division d'essai des deux côtés. |
| **D4 5000!** | 2,206 vs 12,122 ms (×5,49) | (a) | Même produit itératif ; bigint natif `num-bigint`, zéro GMP. |
| **D4b PGCD 1000 ch.** | 0,099 (Stein Rose) vs 0,284 (Stein Rust) | (b) seulement | Euclid ≠ Stein : **pas** une victoire « même source ». |
| **D8 E/S 10 Mo** | 5,433 vs 7,446 ms (×1,37) | (a) + (b) | Même paire `lire_texte`/`ecrire_texte` ; Rose bat aussi C (7,588) et Python (6,933). |
| **Racines √2 OLD** | 20,040 vs 20,942 ms (×1,05) | (b) seulement | Cache du produit pur ; Rodolphe : le même algo avec cache (NEW) est **perdu** 1,043. |
| **Primitives, comptées à part** | `fibonacci` 0,014 · `compter_premiers` 0,132 · `factorielle` (arbre) 0,592 · `pgcd` Stein 0,099 ms | bibliothèque | Ce sont des **victoires de bibliothèque**, pas des victoires du moteur sur une source identique. |
| **Contre Python / C** | Global : Rose 1,49× plus rapide que Python · Englobant : 2,64× · Suite : 1,41× · Langages : `rand()` glibc battu 20× · D3/D8 : C battu | — | Honnête mais sans rapport avec « battre Rust ». |

---

## 4. Défaites non masquées — causes chiffrées

| Défaite | Écart mesuré | Cause (chiffrée quand disponible) |
|---|---|---|
| **D6 liste 100k** | 0,487 − 0,287 = **0,200 ms** (1,698×) | Décomposition mesurée par un autre agent (`rose/docs/VITESSE_FINALE.md`, constat 19/11, c16) : **~288 000 ns de page faults sur ~319 000 ns = 90 %** ; `Valeur::Entier` fait **24 octets** contre 8 pour `Vec<i64>` → 2,4 Mio d'écrits, ~586 pages touchées contre ~196. Les 200 000 appels `extern "C"` du comparatif précédent sont **obsolètes** (8 appels au total, 19 instructions/4 éléments). 9 pistes testées, **toutes rejetées à la mesure** (premier toucher 320 977 ns vs pré-fauté 32 832 ns ; `MAP_POPULATE` dans le bruit ; pré-fautage hors fenêtre +324 µs de coût mur). |
| **Racines i64 2M** | 975,364 vs 11,420 ms (**85,4×**) | Un appel de primitive interprété par itération, aucun chemin i64 intégré dans la boucle Rose. |
| **Global 8 domaines** | 1 088 vs 2,631 ms (**414×**) | 10⁷ itérations de BigInt exact + cumul rationnel ; le jumeau Rust fait du `i64` (déborde) et du `f64`. Pour le jumeau Python, Rose gagne 1,49× : le coût est dans la sémantique exacte, pas dans l'interpréteur. |
| **Englobant** | 181,635 vs 2,154 ms CPU (**84,3×**) | Mêmes formules et mêmes 4 valeurs ; le coût est dans l'exécution interprétée des 4 sections (2×10⁵ tirages `hasard()` + Philox 8 192 blocs par passe ×5). |
| **Suite complexe** | 772,429 vs 2,876 ms CPU (**268,6×**) | f64 bit-égal ; chaque itération construit un `Valeur::Complexe` dans le chemin interprété. La signature `calc(n: entier) -> flottant` ne suffit pas à faire tomber ce coût. |
| **D1 fib35** | 30,940 vs 27,913 ms (**1,108×**) | Même récursion ; codegen/prologue JIT plus coûteux que `rustc -O`. **C -O2 (21,232 ms) bat aussi Rose de 1,46×.** |
| **Langages** | 8 vs 5 ms interne (**1,60×**) | Même splitmix64, 4 tirages/lot ; 3 ms d'écart, dispatch de la primitive. |
| **Racines BigInt / √2 100 déc.** | 1,843× / 1,123× | Chemins sans cache ; le cache n'apporte rien côté Rose (20,040 → 20,112), alors qu'il apporte 7,9 % côté Rust. |
| **D5 chaîne / D7 table** | 0,869× / 0,988× | **Non déclarées victoires** : plages superposées (D5 Rose 0,017–0,057 contre Rust 0,023–0,031 ; et H1 mur Rose 2,186 > Rust 1,441). D7 = égalité dans le bruit. |

---

## 5. Backends — JIT vs AOT vs interpréteur vs RVM

| Backend | Disponible | Fait réel mesuré | Limite |
|---|---|---|---|
| **JIT Cranelift** | ✅ | 47,198 ms mur ; 25,048 ms arith ; 2,785 ms récursion | Référence. |
| **Interpréteur `ROSE_SANS_JIT=1`** | ✅ | 651,853 ms mur (**13,8× le JIT**) ; récursion 500,418 ms (**180× le JIT**) | Le JIT est indispensable sur la récursion. |
| **RIR désactivé `ROSE_NO_RIR=1`** | ✅ | 46,990 ms mur (**0,996× le JIT**) : aucun gain mesurable ici | Le RIR n'apporte rien sur ce jeu-ci, hors D2 (boucle spécialisée). |
| **Émetteur natif `ROSE_NATIF=1`** | ✅ | 665,867 ms mur ; **arith 10,218 ms < JIT 25,048** ; récursion 637,795 ms | Bat le JIT sur l'arithmétique pure mais **perd 229× sur la récursion**. |
| **`rose comp` (autonome)** | ✅ | 48,364 ms mur, mêmes valeurs | ~1,02× le JIT : pas de gain de vitesse, gain d'autonomie. |
| **AOT LLVM ORC `--link`** | ✅ (noyau seul) | **6,054 ms mur** → **7,8× plus rapide que le JIT** sur le noyau arith+récursion | Ne compile ni `temps()`, ni chaîne/liste/table : **PAS comparable au jeu complet**. |
| **AOT LLVM classique** | ✅ (noyau seul) | 515,257 ms mur | **85× plus lent qu'ORC** sur ce noyau. |
| **RVM `.rosebc` (`rose bc`)** | ❌ | `rc=1 — rose bc : bytecode Rose : opération \`AST\` refusée par la sandbox` | Aucune mesure inventée. |
| **Émetteur ELF auto-hébergé** | ❌ | voir §6 | Aucune mesure inventée. |

---

## 6. Self-hosting — verdict courant : **PARTIEL**, point de défaillance exact

| Porte | État mesuré ici | Preuve |
|---|---|---|
| Point fixe `rir` ↔ front-end Rose | ✅ **atteint ×2** | `md5(rose rir compilateur/emetteur.rose)` = `md5(rose compilateur/emetteur.rose compilateur/emetteur.rose)` (deux voies, même sortie). **Deux valeurs datées** : `8ac53eed3d4967a0d7d9ab333256801c` à `21:31Z` avant modification du fichier par un autre agent ; `b3ed9262dd54776a6dbd07bc12368d0e` après (`compilateur/emetteur.rose` md5 `390335d29fb9cee368b3456dd892e377`, modifié à `22:35+01:00`). Le changement vient du `.rose`, **pas du moteur**. |
| Auto-hébergement du front-end | ✅ | Le point fixe ci-dessus prouve l'identité de sortie du front-end, pas un exécutable. |
| Chaîne LLVM complète en source Rose | ❌ | `rose/rose build …` n'existe pas (`rose: fichier introuvable : build`) ; seule la voie Rust `orc`/`llvm`/`comp` fonctionne. |
| RVM auto-hébergé | ❌ | `rose bc …` → `rc=1 — opération \`AST\` refusée par la sandbox` (rejoué ici). |
| Émetteur ELF auto-hébergé | ❌ **point de défaillance exact** | `compilateur/emetteur_elf.rose` est un **travail non committé d'un autre agent** pendant cette session. (i) Avec l'ancienne forme 3 arguments du harnais backends, `rc=0` mais **aucun fichier ELF n'est produit** — les octets ELF partent sur `stdout` (le sondeur ne vérifiait que `rc`). (ii) Avec la forme actuelle documentée en tête de fichier, `rose compilateur/emetteur_elf.rose bench/backends/bench_backends.rose -o /tmp/opencode/cmp2_agents/self3.elf` → **`rc=1`, `Rose : erreur — Fonction 'chaine' inconnue`**, 0 octet écrit. (iii) Avec le dump RIR complet de `emetteur.rose` (263 520 octets), l'émission **n'a pas terminé en 180 s**. |
| Exécution d'un ELF auto-écrit | ❌ | Aucun ELF produit ce jour → aucune exécution revendiquée. Le doc d'un autre agent (`G5_IO.md`) documente `rc=139` sur boucle d'allocation ~2,8 M appels, `table`/`inserer`/`cle_existe` encore stubs, UTF-8 (points de code) vs octets non unifié, et >8 arguments écrasant `x8`/`x9` — **je le cite comme constat antérieur, pas comme ma mesure**. |

**Verdict** : **PARTIEL** — front-end auto-hébergé (point fixe), **0 % du produit livré sans Rust**.
Le point de défaillance mesuré aujourd'hui est **l'émetteur ELF Rose lui-même**, avant
même l'exécution : il ne produit pas de fichier avec l'ancienne signature et échoue sur
`Fonction 'chaine' inconnue` avec la nouvelle. Je n'ai **rien modifié** à ce fichier.

---

## 7. Validations

```
# 1) Point fixe ×2 — 21:31Z (avant modification de emetteur.rose par un autre agent)
R=/tmp/opencode/cmp2_agents/rose_cmp2
$R rir compilateur/emetteur.rose 2>&1 | md5sum      # 8ac53eed3d4967a0d7d9ab333256801c
$R compilateur/emetteur.rose compilateur/emetteur.rose 2>&1 | md5sum
                                                   # 8ac53eed3d4967a0d7d9ab333256801c

# 2) Point fixe ×2 — après modification (constat actualisé)
$R rir compilateur/emetteur.rose 2>&1 | md5sum      # b3ed9262dd54776a6dbd07bc12368d0e
$R compilateur/emetteur.rose compilateur/emetteur.rose 2>&1 | md5sum
                                                   # b3ed9262dd54776a6dbd07bc12368d0e

# 3) Suite de tests — 431 / 0 échec (à 21:39Z, arbre e0ec287)
cd rose && export PATH="$HOME/.cargo/bin:$PATH" RUSTFLAGS="" CARGO_PROFILE_RELEASE_STRIP=none
cargo test --release
# test result: ok. 220 passed; 0 failed; 3 ignored     (lib)
# test result: ok. 204 passed; 0 failed; 3 ignored     (bin)
# test result: ok. 0 passed; 0 failed
# test result: ok. 6 passed; 0 failed
# test result: ok. 1 passed; 0 failed                  → 431 passed, 0 failed
# test result: ok. 0 passed; 0 failed

# 4) Intégrité du binaire, après toutes les mesures
sha256sum /tmp/opencode/cmp2_agents/rose_cmp2
# f4b4f729e2f267f9d9c79a0e1a40c7154c3bde43201ddc7648cd16bef9a5ec65
```

---

## 8. Commandes rejouables exactes

```bash
# 0) Binaire isolé (ne JAMAIS copier sur rose/rose)
cd /home/clausake/Stefan/ROSE
export PATH="$HOME/.cargo/bin:$PATH"
mkdir -p /tmp/opencode/cmp2_agents
(cd rose && cargo build --release --bin rose)
cp rose/target/release/rose /tmp/opencode/cmp2_agents/rose_cmp2
sha256sum /tmp/opencode/cmp2_agents/rose_cmp2

# 1) Multi — 9 domaines
cd /tmp/opencode/cmp2_agents
PIN_CPU=11 NSAMP=9 python3 bench_cmp2.py | tee cmp2_multi_run.log
# → cmp2_results.csv, cmp2_summary.json

# 2) Racines — 5 comparaisons, 9 échantillons alternés, checksums obligatoires
cd /home/clausake/Stefan/ROSE
R=/tmp/opencode/cmp2_agents/rose_cmp2
OLD=/tmp/opencode/ra2_agents/rust_ref_OLD
NEW=/tmp/opencode/ra2_agents/rust_ref_NEW
H="python3 /tmp/opencode/cmp2_agents/harness_c11.py"
$H --label A_dec1000_OLD --n 9 --expect-a 500500 --expect-b '500500 1000' \
   --a "$R" bench/racines/racines_dec.rose 500 1000 --b "$OLD" decs 500 1000 \
   --json /tmp/opencode/cmp2_agents/rac_a_old.json
$H --label B_dec1000_NEW --n 9 --expect-a 500500 --expect-b '500500 1000' \
   --a "$R" bench/racines/racines_dec.rose 500 1000 --b "$NEW" decs 500 1000 \
   --json /tmp/opencode/cmp2_agents/rac_b_new.json
$H --label C1_i64 --n 9 --expect-a 4000005000000 --expect-b '4000005000000 100' \
   --a "$R" bench/racines/racines_i64.rose 2000000 --b "$OLD" f64 2000000 \
   --json /tmp/opencode/cmp2_agents/rac_c1.json
$H --label C2_big --n 9 --expect-a 500000 --expect-b '500000 100' \
   --a "$R" bench/racines/racines_big.rose 20000 --b "$OLD" big 20000 \
   --json /tmp/opencode/cmp2_agents/rac_c2.json
$H --label C3_dec100 --n 9 --expect-a 505000 --expect-b '505000 100' \
   --a "$R" bench/racines/racines_dec.rose 5000 100 --b "$OLD" decs 5000 100 \
   --json /tmp/opencode/cmp2_agents/rac_c3.json

# 3) Backends
python3 /tmp/opencode/cmp2_agents/run_backends_c11.py | tee /tmp/opencode/cmp2_agents/backends_run.log

# 4) Global
export PATH="$HOME/.cargo/bin:$PATH"
rustc -O /home/clausake/Stefan/ROSE/bench/global/bench_global.rs -o /tmp/opencode/cmp2_agents/global_rs
python3 /tmp/opencode/cmp2_agents/bench_global_c11.py | tee /tmp/opencode/cmp2_agents/global_run.log

# 5) Englobant (source réparée : copie + 2 suppressions, §2.5)
cd /home/clausake/Stefan/ROSE/bench/englobant
g++ -O3 -march=native -ffp-contract=off -std=c++17 -pthread bench_englobant.cpp \
    -o /tmp/opencode/cmp2_agents/eng_cpp
rustc -C opt-level=3 -C target-cpu=native -C overflow-checks=off \
    -C debug-assertions=off bench_englobant.rs -o /tmp/opencode/cmp2_agents/eng_rs
python3 /tmp/opencode/cmp2_agents/bench_englobant_c11.py | tee /tmp/opencode/cmp2_agents/englobant_run.log

# 6) Langages, N=10^7
cd /home/clausake/Stefan/ROSE/bench/langages
g++ -O2 -march=native -std=c++20 mdp_splitmix.cpp -o /tmp/opencode/cmp2_agents/mdp_splitmix
gcc -O2 -march=native -x c mdp_rand.c -o /tmp/opencode/cmp2_agents/mdp_rand_c
rustc -O mdp.rs -o /tmp/opencode/cmp2_agents/mdp_rs
python3 /tmp/opencode/cmp2_agents/bench_langages_c11.py | tee /tmp/opencode/cmp2_agents/langages_run.log

# 7) Suite complexe
cd /home/clausake/Stefan/ROSE/bench/suite
g++ -O3 complexe_cpp.cpp -o /tmp/opencode/cmp2_agents/suite_cpp
rustc -O complexe_rs.rs -o /tmp/opencode/cmp2_agents/suite_rs
python3 /tmp/opencode/cmp2_agents/bench_suite_c11.py | tee /tmp/opencode/cmp2_agents/suite_run.log

# 8) Validations (§7)
```

Tous les harnais, journaux, CSV et JSON sont dans `/tmp/opencode/cmp2_agents/`.
`bench/**` n'a **pas** été modifié ; seule la copie réparée de l'englobant y est créée.

---

## 9. Limites — à lire avant toute citation

1. **c11 est un cœur A725 (capacity 731), pas un X925 (1017)**. Toutes les valeurs ici
   sont ~1,5–2,7× plus lentes que les documents pinés c15/c16. Ne pas les mélanger.
2. **L'arbre a bougé pendant la salve** (3 commits d'autres agents, `e0ec287` → `f564545`).
   Aucun `.rs` n'a changé (`git diff e0ec287..HEAD -- rose/src` vide) : le moteur binaire
   est le même, mais `compilateur/emetteur.rose` a changé, donc le point fixe a changé de
   hash (§6).
3. **D5 et D7 sont indécises** (bruit) ; elles ne comptent pas comme victoires robustes.
4. **Global n'est pas un verdict de moteur pur** (BigInt/rationnel vs `i64`/`f64`, et
   divergence d'affichage du rationnel documentée §2.4).
5. **Le C++ du banc global n'a pas été rejoué** (GMP/gmpxx absents) ; aucun temps inventé.
6. **L'englobant commité est cassé** ; les mesures portent sur une copie réparée de 2
   lignes, dont le diff est publié §2.5. Le moteur n'a pas été modifié.
7. **AOT ORC/llvm ne compilent que le noyau** : 6,054 ms n'est pas comparable au jeu
   complet des 47,198 ms du JIT.
8. **Self-hosting PARTIEL**, émetteur ELF en échec fonctionnel mesuré (§6) ; document
   écrit sans toucher au fichier WIP d'un autre agent.
9. Aucun commit, aucune publication, aucun remplacement de `rose/rose`, aucune modification
   de `.rs`, `Cargo.toml`, `PLAN.md` ou doc d'un autre agent.
