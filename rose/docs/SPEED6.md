# SPEED6 — rétention des chaînes et index de table (CONSTAT DATÉ)

Sous-agent `speed6`. **Aucun commit, aucune publication, binaire `rose/rose` du dépôt
non remplacé.** Deux fichiers touchés au total : **`rose/src/run.rs`** (117 insertions,
53 suppressions) et ce document. `interp.rs`, `jit.rs`, `i128x.rs`, `front.rs`, `rir.rs`,
`main.rs`, `Cargo.toml`, `PLAN.md` et les docs des autres agents n'ont **pas** été
modifiés. Les correctifs antérieurs sont préservés (rationnel `f9b0966`, pool `38a8804`,
garde mpz `0b87f77`, `array_part` `065a0d2`, VAL1 `e4f25cf`, SPEED4 `f133796`).

> **Verdict : porte « diviser rétention + index (250 µs) par 2 » NON ATTEINTE.**
> Gain réel mesuré sur la phase tables du banc global : **−87,8 µs c16 / −85,9 µs c6
> (ratio 0,825 / 0,832)**, soit **35 % des 250 µs visés** (il fallait −125 µs).
> Sur la somme `tables + RNG` : **−88,3 µs c16 / −93,8 µs c6** (ratios 0,869 / 0,867).
> Cause chiffrée et bloquée par le périmètre : §5. **Aucun chiffre n'a été pris
> hors de `run.rs`.**

**Horloge : 20/11/2026 ~18:35–19:20 UTC+1** (la consigne annonce 21/11). Tout chiffre
ci-dessous est un constat daté, à re-mesurer après tout changement de binaire,
d'arbre source, de pinage ou de topologie. L'arbre a bougé pendant la mission
(HEAD annoncé `cadcdb8`, HEAD en fin de mesure `8bc15ca`, autres agents actifs sur
`interp.rs`) : **toutes les mesures A/B ont été faites sur un arbre isolé figé au
départ**, témoin et candidat construits depuis le même arbre, seul `run.rs` diffère.

---

## 1. Protocole réellement exécuté

| Élément | Mesure |
|---|---|
| Arbre source figé | `/tmp/opencode/speed6_agents/src_base` (copie de l'arbre au départ, `target` exclu) |
| Témoin A | `/tmp/opencode/speed6_agents/rose_base`, SHA256 `10cf8722889f…ea0f114c6f2737990e0a84aa38e` |
| Candidat B | `/tmp/opencode/speed6_agents/rose_cand`, SHA256 `337b438c9d03…c4fc3bb90eb905b14122cb24b745e525b81a740ceeb0aed` |
| Construction | `CARGO_TARGET_DIR` dédié par binaire ; aucun binaire copié dans `rose/rose` |
| Pinage | `taskset -c 6` **et** `-c 16`, mesurés sur la même machine (aarch64, `cntvct_el0` = 1 GHz) |
| Échantillons | **12 par binaire et par cœur**, rotation entrelacée A/B/B/A puis B/A/A/B, médianes **et** différences appariées, deux salves complètes c6 + c16 |
| Horloge interne | primitive Rose `temps()` (banc instrumenté copié hors dépôt, structure du banc strictement conservée) |
| Horloge des sondes | `cntvct_el0` via `mrs`, fréquence re-mesurée à 1 000 000 000 Hz |
| Valeurs | 11 valeurs du global vérifiées à chaque tirage ; 81/81 sur le contrôle multi (9 programmes × 9 tirages) c6 **et** c16 |
| Bruit | dérive forte entre salves (phase tables du témoin lue de 482 à 512 µs) : la décision repose sur les **différences appariées entrelacées**, pas sur les médianes marginales |

Harnais : `/tmp/opencode/speed6_agents/{measure.py,paired.py,probe_probe.py,validate_multi.py}`
et bancs instrumentés `phase_bench.rose`, `probe_table.rose`, `probe_table2.rose`,
`probe_table3.rose` — tous hors dépôt. Aucun `.rose` du dépôt n'a été modifié.

---

## 2. Mesure 1 — où va réellement la rétention (sondes temporaires, retirées)

Cinq binaires-sondes ont été construits depuis le même arbre figé, en court-circuitant
une seule partie de `Table::inserer` ou de `Valeur::s` à la fois (`black_box` pour
empêcher LLVM d'éliminer le `format!` de l'appelant). Banc `probe_table2.rose` :
`table()` hors chrono, 5 001 insertions `inserer($t, "k"+chaine($f), $f*$f)` chronométrées.
Médianes de 12 à 15 tirs c16 (ns) :

| Sonde (hors livrable) | Médiane | Lecture |
|---|---:|---|
| témoin, `inserer` complet | 481 663 | référence |
| `inserer` = no-op, `Valeur::s` réel | 209 792 | **appelant seul** = `format!` (interp.rs) + `Valeur::s` + `Valeur::z` |
| `inserer` = no-op, `Valeur::s` → `Rc` statique | 143 792 | **plancher appelant** ~144 µs : `format!` + boucle + `Valeur::z` |
| `inserer` = no-op, `Valeur::s` → `Rc` pré-alloués distincts | 145 727 | idem, contrôle de la sonde (±2 µs) |
| `inserer` complet, `Rc` pré-alloués distincts | 251 760 | insertion avec clés déjà allouées contiguës |

Deux déductions chiffrées :

1. **`Valeur::s` / `ChaineRep::plein` coûte ~66 µs** (209 792 − 143 792) pour 5 000 clés :
   c'est l'allocation `Rc<ChaineRep>` qui ne peut pas être réutilisée.
2. **L'inséreur coûte 272 µs avec de vraies clés mais 106 µs avec les mêmes clés
   pré-allouées en bloc** (481 663−209 792 contre 251 760−145 727). L'écart de
   **166 µs est la rétention mesurée** : ce n'est pas seulement le coût CPU des
   `malloc`, c'est surtout l'empreinte de 10 000 blocs vivants imbriqués (`String`
   + `Rc`) qui dégrade caches et TLB pendant les 5 000 insertions.

Décomposition complémentaire (médianes c16, mêmes sondes) :

| Poste | µs |
|---|---:|
| appelant `format!` + `Valeur::s` + `Valeur::z` | ~210 |
| `Vec::push` nu (mesuré par `push(Vide)`) | ~47 |
| index `HashMap` après `reserve(8192)` | ~53 |
| hachage direct des octets | ~11 |
| rétention / localité des 5 000 clés | **~166** |

---

## 3. Changements livrés (tous dans `rose/src/run.rs`)

1. **Feuille de chaîne courte en ligne** — nouveau variant `NoeudChaine::PleinCourt
   { buf: [u8; 16], len: u8 }`. `ChaineRep::plein` copie les octets ASCII ≤ 16 dans
   le `Rc` lui-même : **un seul bloc heap vivant par clé au lieu de deux**. Le `String`
   produit par `format!` est libéré immédiatement ; sa taille d'enum reste 32 octets
   (le nouveau variant tient dans la place déjà occupée par `String`), donc la layout de `Valeur` ne
   change **pas** — le piège de la drop-glue signalé en tête de `run.rs` est évité.
   `octets_empruntes`, `aplatir`, `cmp_rep`, `egale_rep`, `Debug` et le `Drop`
   itératif gèrent le variant. Les octets non utilisés du tampon sont laissés à zéro :
   seule la tranche `[..len]` est lue, donc le hachage FNV et l'égalité sont bit-identiques.
   **Mesuré : −75 µs sur la phase tables** (486 → 406 µs, apparié).
   Coût assumé : l'appelant passe de 210 à 227 µs (+17 µs, libération du `String`
   désormais inside `Valeur::s`) — ce coût est inclus dans le gain net ci-dessus.
2. **Index plat à adressage ouvert** — `IndexRapide { hs: Vec<u64>, ps: Vec<u32>, n }`
   remplace `HashMap<u64, usize, HashU64>` (deux tableaux contigus, 96 Kio au lieu de
   ~136 Kio pour 8 192 buckets ; plus de finaliseur sur le h, une seule sonde en
   moyenne, croissance à 70 % de charge). Les collisions de h restent résolues par
   `valeur_egale` et la liste `collisions` inchangée ; `trouver_h`, `inserer_i64`,
   `contient_i64`, `reindexer` et `retirer` utilisent la même source de vérité.
   **`retirer` reconstruit désormais l'index depuis `items`** (déjà O(n) auparavant
   à cause des décalages de positions). **Mesuré : −14 µs** (406 → 392 µs).

`HashU64` est conservé (`#[allow(dead_code)]`) : le retirer proprement n'était pas
nécessaire et toucher à d'autres zones aurait élargi le diff inutilement.

### Tentatives mesurées puis REJETÉES (code retiré du livrable)

| Tentative | Mesure c16 (phase tables) | Décision |
|---|---:|---|
| `items.reserve(8192)` au lieu de 4096 | 403 vs 406 µs (~1 µs, bruit) | retiré |
| h FNV précalculé dans les octets libres de `PleinCourt` | 400 vs 393 µs (**+6 µs**) | retiré : le calcul dans `plein` coûte plus que la lecture dans `h` |
| index direct 1 slot sans sonde linéaire | **607 µs** (collisions de slots → O(n²) dans `collisions`) | retiré |
| accélérateur dense « suffixe décimal » des clés chaîne | 390 vs 397 µs (~7 µs) | retiré : complexité et surface de bug sans gain décisif |
| libérer le `String` **avant** l'allocation du `Rc` | −2 µs (bruit) | retiré |
| `rep_mut_nv()` (emprunt `RefCell` supprimé) dans `inserer` | −1,4 µs (bruit) | retiré, aucun `unsafe` ajouté |

---

## 4. Gains réels c6 + c16 — salves appariées 12 × 2 cœurs

Médianes (ns), même arbre figé, seul `run.rs` diffère :

| Phase | témoin c16 | candidat c16 | Δ c16 | témoin c6 | candidat c6 | Δ c6 |
|---|---:|---:|---:|---:|---:|---:|
| carrés | 723 463 | 726 295 | +2 832 | 720 455 | 723 783 | +3 328 |
| flottants | 516 391 | 516 471 | +80 | 517 943 | 516 791 | −1 152 |
| rationnels | 493 720 | 495 999 | +2 280 | 499 727 | 498 368 | −1 360 |
| chaînes | 2 328 | 2 448 | +120 | 2 888 | 2 992 | +104 |
| listes | 84 088 | 85 200 | +1 112 | 88 032 | 89 272 | +1 240 |
| **tables** | **502 056** | **414 287** | **−87 768** | **512 551** | **426 696** | **−85 855** |
| fib(28) | 928 | 912 | −16 | 1 312 | 1 400 | +88 |
| RNG | 172 128 | 171 576 | −552 | 190 896 | 182 936 | −7 960 |
| E/S | 55 176 | 59 296 | +4 120 | 73 992 | 76 400 | +2 408 |
| **TABLES + RNG** | **674 184** | **585 862** | **−88 321** | **703 446** | **609 632** | **−93 814** |

Différences appariées médianes : **−88 216 ns c16** (min −328 671, max −75 728) et
**−93 392 ns c6** (min −246 112, max −38 000). Ratios `tables` : **0,825 c16 / 0,832 c6**.
Les phases hors périmètre sont inchangées dans le bruit : aucun gain n'a été pris hors
de `run.rs`. La régression apparente des E/S (+4,1 µs c16) est du bruit de salve
(même code, variations de 2 à 4 µs entre tirages du même binaire).

---

## 5. Cause chiffrée de la non-atteinte — blocage de périmètre, pas renoncement

Il manque **~37 µs** pour atteindre −125 µs. Deux sondes bornent ce qui restait
accessible :

**(a) Le plancher appelant est de ~144 µs** (`inserer` no-op + `Valeur::s` statique,
§2). Cette somme est presque entièrement le `format!("{}{}", prefixe, i)` de
`interp.rs` (`exec_boucle`, ligne ~3031), **formellement interdit**. Même en
annulant toute la rétention et tout l'index, ce plancher seul dépasse déjà le
budget cible de 125 µs si l'on y inclut l'allocation du `format!` comptée dans
les 165 µs de rétention par la sonde SPEED4 « push(Vide) → push(clé réelle) ».

**(b) Supprimer totalement la rétention rapporterait ~99 µs de plus** — mais au prix
d'une rupture sémantique. Sonde : dans `inserer`, remplacer la clé `Valeur::Chaine`
par `Valeur::Entier((h >> 1) as i64, None)` avant le `push`. Médiane appariée
c16 : **398 095 → 300 031 ns, soit −99,1 µs** (9 paires, min −144 175, max −86 193).
Ce gain n'est **pas livrable** :

- `cles()` lit directement `t.0.borrow().items.iter().map(|(k, _)| k.clone())`
  dans `interp.rs` (ligne ~5478) : les clés rendues seraient des entiers au lieu
  de chaînes. L'API `Table` cesserait d'être sémantiquement identique.
- Stocker un nouveau variant compact dans `Valeur` casserait les ~25 sites
  `match Valeur::Chaine` de `interp.rs`/`jit.rs`/`repl.rs`, tous hors périmètre
  (ex. `TyKind::Chaine => matches!(args[i], Valeur::Chaine(..))` ligne 2264,
  `concatener` ligne 5524). La persistance d'un `Rc<ChaineRep>` distinct par clé est donc
  imposée par le contrat observable, pas par un choix d'implantation.

Après le livrable, le coût restant se décompose ainsi (c16, estimations par
différences de sondes) : rétention résiduelle **~100 µs** (un `Rc` vivant par clé,
irréductible sans (b)) + index **~39 µs** (déjà divisé par ~2,2 par rapport aux
~85 µs de SPEED4) + appelant ~227 µs + push ~47 µs + hash ~11 µs. Le poste cible
`rétention + index` passe donc d'environ **250 µs à ~139 µs** (−111 µs, −44 % par
les sondes ; −88 µs, −35 % par la mesure de phase appariée retenue). **La porte
−50 % n'est pas atteinte.**

---

## 6. Validation exécutée (preuves de sortie)

| Gate | Résultat observé |
|---|---|
| `cargo build --release --bin rose` (arbre isolé figé) | **rc=0** |
| `cargo build --release --bin rose` (arbre du dépôt avec le patch livré, `CARGO_TARGET_DIR` dédié) | **rc=0** |
| `cargo test --release` (arbre isolé figé) | **515 passed, 0 failed, 6 ignored** |
| jardin `jardins/tests/premiers.rose` | `20 tests, 0 échec` |
| `fib(25)` | `75025` |
| point fixe `b3ed9262…` ×2 | `md5(rir emetteur.rose)` = `md5(front emetteur.rose)` = `b3ed9262dd54776a6dbd07bc12368d0e` |
| détail géant (`demo_geant.rose`) | longueur `365657055180967481723488710810833949196`, zeros finaux `2499999999999999999999999999999999990`, derniers non nuls `467172352` |
| 9 domaines × 9 tirages c6 **et** c16 | **81/81 + 81/81 = 162/162** sorties identiques au témoin |
| `bench_global.rose` ×3, dossier temporaire | **rc=0**, 11 valeurs identiques au témoin à chaque tirage |
| banc principal / phase tables | 5 000 entrées, `$tab["k2500"]` = `6250000` à chaque tirage, c6 et c16 |

---

## 7. État final factuel

- Fait : deux optimisations semantic-preserving livrées dans `run.rs` ; phase tables
  −17 % c16 / −17 % c6 ; tables+RNG −13 % c16 / −13 % c6.
- Vérifié : build, 515 tests, jardin, fib, point fixe, détail géant, 162/162 multi,
  global 11 valeurs ×3, A/A/A entrelacé 12 échantillons sur c6 et c16.
- Non atteint : division par 2 de `rétention + index`. Il manque ~37 µs sur la mesure
  de phase. Blocage borné : ~144 µs de `format!` dans `interp.rs` (interdit) et
  l'obligation de conserver un `Valeur::Chaine` par clé visible par `cles()`
  (interp.rs non modifiable). Aucun chiffre de ce document ne provient d'une spec
  ou d'un README : tout est issu de sondes exécutées et de salves appariées.
- Aucun commit, aucune publication, aucun binaire installé dans `rose/rose`.
