# BENCH_PERF — mesure indépendante Rose / Rust / C / Python

**Date de la salve** : 2026-11-22T18:45:25Z → 18:45:39Z (salvo 1), salve de contrôle identique juste après (salvo 2). **Toutes les valeurs ci-dessous sont des constats datés**, pas des consignes.

**Binaire mesuré** : `/home/clausake/Stefan/ROSE/rose/target/release/rose`, SHA256 `21ed57e4be7c765f09635a3600dfc0fb1e540bb05d5ea924503a13bf1cb980b2`, **identique avant et après les deux salves** (vérifié par le harnais). Arbre principal lu uniquement ; aucun fichier `rose/src/**` ni `compilateur/**` modifié.

**Compilateurs** [MESURÉ] : `rustc 1.99.1` (`~/.cargo/bin/rustc -O`, dépendances `num-bigint 0.4.8` + `num-rational 0.4.2` hors ligne), `gcc 13.3.0 -O2`, `python3 3.12.3`. GMP absent (`gmp.h` introuvable) : le bigint C est un bigint **maison base 10⁹** (`bench/perf/src/bigint.h`), pas une bibliothèque équivalente à `num-bigint` — les temps C des domaines bigint ne doivent pas être lus comme « C optimizeur de bibliothèque ».

**Machine** [MESURÉ] : aarch64 GB10, 20 cœurs hétérogènes, `governor=performance`, 13 domaines × 4 langages pinés sur le **cœur X925 15** (`taskset -c 15`, ~98 % inactif au moment de la salve ; le cœur 19 était occupé par un autre agent).

## Étiquettes utilisées

- **[MESURÉ]** : sortie brute produite par ce harnais (fichiers `bench/perf/out/`).
- **[CITÉ]** : affirmation lue dans un document préexistant, non reproduite ici.
- **[SOURCE]** : lecture du code moteur, pas une mesure de performance.

## Protocole [MESURÉ]

- **9 tirages** par (domaine, langage) dans chaque salve, **entrelacés par rotation de l'ordre des langages**, dans une seule salve ; une salve de contrôle identique a été rejouée. Médianes rapportées. Aucun tirage rejeté.
- **Trois horloges distinctes, jamais confondues** :
  1. `wall` = `perf_counter_ns` autour de `fork/exec/wait4` — démarrage + parsing + compilation + calcul + affichage ;
  2. `cpu` = `ru_utime + ru_stime` du fils récupérés par `wait4` ;
  3. `interne` = chronomètre posé par le programme autour du **calcul seul** (`temps()` Rose, `Instant` Rust, `clock_gettime(CLOCK_MONOTONIC)` C, `perf_counter_ns` Python).
- **Pic RSS** = `ru_maxrss` du fils via `wait4`, jamais `free -h` ni agrégat. Médiane Rose **12,2 MiB** sur tous les domaines, **12,9 MiB** sur E/S ; Rust 12,2 MiB.
- **Vérification d'identicité** : à chaque tirage, la ligne `RESULT:` des 4 langages doit être égale à une référence recalculée par Python (`math.factorial`, `math.gcd`, `fractions.Fraction`, etc.), et pour E/S le fichier produit doit avoir le même SHA256 que l'entrée (10 485 760 octets, `53ad9e07…`). Les erreurs lèvent une exception : aucune mesure « approximative » n'est acceptée.
- `cpu/wall` médian [MESURÉ] : Rose 1,007 ; Rust 1,009 ; C 1,008 ; Python 1,000 (programmes mono-thread, donc les deux horloges se recoupent — elles restent rapportées séparément, comme demandé).

## Tableau principal [MESURÉ] — salvo 2, médianes du calcul seul (`interne`)

`R/Ru = temps Rose / temps Rust` : **< 1 = Rose gagne**. Même format derrière : `R/C`, `R/Py`. `wall` = médiane du processus complet.

| Domaine | Rose int (ms) | Rust int (ms) | C int (ms) | Python int (ms) | R/Ru | R/C | R/Py | Rose wall | Rust wall | Verdict vs Rust |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|:---:|
| int_loop — somme 1..10⁶ | 1,434 | 0,259 | 0,257 | 48,633 | 5,532 | 5,590 | 0,029 | 4,303 | 2,515 | **Perdu** |
| fib_rec — fib(35) récursif | 11,599 | 12,492 | 8,923 | 589,960 | **0,929** | 1,300 | 0,020 | 14,766 | 14,921 | **Gagné** |
| fib_iter — 10⁶ tours mod 1e9+7 | 3,609 | 2,064 | 2,085 | 75,127 | 1,749 | 1,731 | 0,048 | 6,361 | 3,607 | **Perdu** |
| premiers — < 50 000 par division | 1,479 | 1,033 | 0,946 | 18,025 | 1,431 | 1,563 | 0,082 | 4,058 | 2,701 | **Perdu** |
| factorial — 3000! itératif | 0,463 | 0,404 | 2,854 | 1,217 | 1,144 | 0,162 | 0,380 | 4,126 | 2,981 | **Perdu** |
| gcd — 2 × 1000 chiffres (Stein) | 0,006 | 0,002 | 0,007 | 0,002 | 2,252 | 0,759 | 2,424 | 2,043 | 1,654 | **Perdu** |
| puissance — 2^20000 | 0,012 | 0,010 | 0,268 | 0,020 | 1,161 | 0,044 | 0,586 | 2,756 | 2,300 | **Perdu** |
| rationnel — Σ1/k, k=2..200 | 0,064 | 0,560 | 3,918 | 0,145 | **0,115** | 0,016 | 0,445 | 2,497 | 2,363 | **Gagné** |
| chaine — 20 000 concaténations | 0,739 | 0,031 | 0,048 | 2,901 | 23,713 | 15,522 | 0,255 | 3,715 | 2,209 | **Perdu** |
| liste — 100 000 insertions + somme | 1,043 | 0,183 | 0,147 | 9,336 | 5,704 | 7,103 | 0,112 | 4,166 | 2,226 | **Perdu** |
| table — 3000 insertions + lookups | 0,051 | 0,048 | 1,189 | 0,469 | 1,055 | 0,043 | 0,108 | 2,905 | 2,120 | **Perdu (au bruit près)** |
| io — lecture + écriture 10 Mio | 2,904 | 4,748 | 4,615 | 4,912 | **0,612** | 0,629 | 0,591 | 5,730 | 8,072 | **Gagné** |
| startup — programme minimal | ~0 | ~0 | ~0 | ~0 | ≈1,000 | ≈ | ≈ | 2,482 | 2,276 | **≈ (égalité des temps internes)** |

### Verdicts chiffrés [MESURÉ]

- **Contre Rust (calcul seul)** : **3 gagnés** (fib_rec 0,929× ; rationnel 0,115× ; E/S 0,612×), **9 perdus**, **1 égalité dégénérée** (startup, temps internes à 0 ns de part et d'autre).
- **Contre Rust (wall)** : **3 gagnés** (fib_rec, gcd, E/S), **10 perdus** — le démarrage Rose (2,48 ms) coûte ~0,2–0,4 ms de plus que Rust/C (2,28 / 2,18 ms) et efface deux victoires internes.
- **Contre C (calcul seul)** : **6 gagnés** (factorial 0,162× ; gcd 0,759× ; puissance 0,044× ; rationnel 0,016× ; table 0,043× ; E/S 0,629×), **6 perdus**, 1 égalité dégénérée. Les « victoires » bigint C sont à relativiser : GMP est absent, le C utilise un bigint naïf.
- **Contre Python (calcul seul)** : **12 gagnés / 1 égalité** ; mais en wall Python paie ~10–14 ms de démarrage, donc ce Score ne mesure pas « Rose bat Python », seulement « le JIT bat l'interpréteur CPython ».

### Dispersion et pièges de mesure [MESURÉ]

- **`int_loop` Rust est bimodal** : 0,258–0,543 ms d'une salve à l'autre (salvo 1 : 7 tirages à ~0,514 ms, 2 à ~0,263 ms ; salvo 2 : inverse). Le ratio Rose/Rust est donc **2,8× à 5,5×** selon le mode ; Rose perd dans tous les cas. Aucune conclusion de 10–15 % n'est recevable sur ce domaine.
- **E/S** : Rose 2,267–2,942 ms, Rust 4,104–5,082 ms sur 18 tirages cumulés : les plages **ne se recouvrent pas**, donc la victoire E/S tient malgré le bruit de page-cache.
- `chaine` Rose 0,729–0,827 ms (salvo 1) / 0,739 (médiane salvo 2) — stable. `table` 0,049–0,056 ms : l'écart de 5 % avec Rust est du même ordre que la dispersion ; **ne pas en conclure une victoire**.
- `gcd`, `puissance`, `startup` : temps de l'ordre de la microseconde ou moins, dispersion relative élevée ; ratios à lire comme ordre de grandeur.

## Diagnostic JIT vs interpréteur [MESURÉ]

5 tirages par point, mêmes sources, `ROSE_SANS_JIT=1` contre défaut, pin cœur 15 (`bench/perf/out/diag_jit.json`) :

| Domaine | Rose avec JIT (ms) | Rose sans JIT (ms) | x (sans/JIT) |
|---|---:|---:|---:|
| int_loop | 1,428 | 59,089 | 41,4 |
| fib_rec | 11,571 | 3 503,201 | 302,8 |
| fib_iter | 3,614 | 142,847 | 39,5 |
| premiers | 1,478 | 61,004 | 41,3 |
| chaine | 0,878 | 2,560 | 2,9 |
| liste | 1,193 | 15,509 | 13,0 |
| table | 0,060 | 5,705 | 94,4 |
| rationnel | 0,067 | 0,066 | 0,99 |

**Conclusion [MESURÉ]** : les domaines perdus `int_loop`, `fib_iter`, `premiers`, `chaine`, `liste`, `table` sont **déjà accélérés par le JIT** (×2,9 à ×94). Le problème n'est donc pas « l'interpréteur tourne », c'est **la qualité/portée du code généré** pour ces types. `rationnel` passe par une primitive native, pas par le JIT (ratio 0,99).

## Causes identifiées [MESURÉ + SOURCE + CITÉ]

1. **Chaînes (23,7×)** [MESURÉ] : `s = s + "x"` 20 000 fois. [CITÉ `BENCH_MULTI.md`] l'ancienne mesure était 1 007× ; le JIT de concaténation a réduit l'écart d'un facteur ~42, mais `push_str` Rust reste amorti O(n) alors que chaque `+` Rose recopie. [SOURCE `interp.rs`] la voie `concatener(liste)` existe, mais elle changerait l'algorithme, donc elle n'a pas été substituée.
2. **Listes (5,7×)** [MESURÉ] : `joindre` + indexation sur 100 000 éléments. [CITÉ `BENCH_MULTI.md`] « chaque élément est une `Valeur` boxée contenant un `Gz` avec Rc/RefCell ». Le diagnostic JIT montre un gain ×13, donc le JIT touche ce domaine, mais le surcoût par élément reste ~6× celui de `Vec<u64>`.
3. **Boucles entières (5,5× ; fib_iter 1,75× ; premiers 1,43×)** [MESURÉ] : le JIT Cranelift exécute les slots i64, mais le code généré est 2,8–5,5× plus lent que `rustc -O` sur la somme, 1,75× sur le Fibonacci itératif et 1,43× sur la division impaire. Diagnostic : sans JIT ces charges sont 39–41× plus lentes, donc le JIT fonctionne ; c'est son code qui est moins optimisé (pas de réécriture de boucle équivalente à LLVM).
4. **Bigint de petite taille (gcd 2,25× ; puissance 1,16× ; factorial 1,14×)** [MESURÉ] : écarts réels mais sur 2–460 µs. [SOURCE `run.rs`] `pgcd`/`puissance` sont des primitives `num-bigint` ; ici Rust utilise la **même bibliothèque** `num-bigint 0.4.8`, donc l'écart est du surcoût d'appel/conversion de primitive, pas de la bibliothèque.
5. **Démarrage** [MESURÉ] : Rose 2,48 ms, Rust 2,28 ms, C 2,18 ms, Python 10,57 ms (médianes `wall` du domaine `startup`). ~0,2–0,3 ms de surcoût Rose sur **chaque** invocation ; c'est le goulot numéro 1 en temps mur pour les petits calculs.
6. **E/S gagnée (0,61×)** [SOURCE `interp.rs:5718`] : `lire_texte` utilise `ChaineRep::depuis_fichier` avec **mmap en lecture seule**, zéro recopie, validation UTF-8 conservée. C'est une optimisation de bibliothèque, pas une propriété du langage.
7. **Tables presque au niveau (1,06×)** [SOURCE `run.rs:3075`] : `TableRep` contient désormais `array_part` (accélérateur dense 4 096 entrées) + `index: IndexRapide` haché + `fallback`. La table n'est **plus** une liste d'association linéaire.
8. **Rationnels gagnés (0,115×)** [MESURÉ] : 0,064 ms contre 0,560 ms pour `Ratio<BigInt>` Rust, 3,918 ms pour le rationnel maison C et 0,145 ms pour `fractions.Fraction`. [SOURCE] voie native `rationnel` du moteur, confirmée par le diagnostic JIT (×0,99).

## Confrontation aux trois documents contradictoires

| Document | [CITÉ] | [MESURÉ] aujourd'hui (binaire `21ed57…`) | Verdict |
|---|---|---|---|
| `BENCH_MULTI.md` (18/11, binaire `8a4ac9…`) | 1 victoire / 8 ; tables **1946×**, chaînes 1007×, listes 1181×, boucle 52,85×, fib 4,04×, E/S 2,38× perdues | 3 victoires / 13 ; tables **1,06×**, chaînes 23,7×, listes 5,7×, boucle 5,5×, fib **0,93×**, E/S **0,61×** gagnées | **Non reproductible** : binaire différent, tables et chaînes radicalement améliorées. Le chiffre 1946× est caduc. |
| `BILAN_VAGUE_35.md` (19/11) | « 7 victoires » : fib 0,92× · fact 0,21× · premiers 0,85× · PGCD 0,70× · chaîne 0,59× · boucle 0,38× · E/S 0,62× ; table ~1,0× | fib 0,93× ✅ · E/S 0,61× ✅ · table ~1,06× ✅ **reproduits** ; fact 1,14× ❌ · premiers 1,43× ❌ · PGCD 2,25× ❌ · chaîne 23,7× ❌ · boucle 5,5× ❌ | **Partiellement reproductible** (3/7). Le harnais historique ayant disparu, la convention de ratio des 4 autres cas ne peut pas être vérifiée ; ils sont **non confirmés**, pas « faux par principe ». |
| `BENCH_COMPARATIF.md` (18/11) | « Ne pas conclure Rose bat Rust partout » | 3 gagnés / 9 perdus / 1 égalité | **Confirmé** : Rose ne bat pas Rust partout. |

**Attention convention** : les ratios de `BILAN_VAGUE_35` sont écrits « Rose/Rust < 1 = victoire », identiques en apparence à ceux d'ici, mais ses valeurs (ex. chaîne 0,59×) sont incompatibles avec une mesure sur le binaire actuel. Ne pas les additionner aux présents ni les présenter comme validés.

## Top 5 des goulots à attaquer (classés) [MESURÉ + priorisation]

Classement par **levier mesuré** = ampleur du ratio × occurrence dans les charges réelles × coût en temps mur ; pas par intention.

1. **Concaténation de chaînes** — 23,7× vs Rust, 0,739 ms de calcul, wall 3,72 vs 2,21 ms. Le plus gros écart et un motif ubiquitous (journalisation, sérialisation). Piste : étendre le chemin JIT de concaténation amorti (le diagnostic montre ×2,9 seulement entre JIT et interpréteur).
2. **Conteneurs de listes** — 5,7× vs Rust, 1,043 ms, wall 4,17 vs 2,23 ms. Piste : représenter les listes d'entiers en slots i64 contigus dans le JIT plutôt qu'en `Valeur`/`Gz` boxés (`interp.rs`/`run.rs`).
3. **Codegen JIT des boucles entières** — somme 5,5× (2,8× en mode lent), fib_itératif 1,75×, premiers 1,43× ; 1,43–3,61 ms par charge. Piste : réécriture de boucle/induction et réduction de l'ABI d'appel dans `jit.rs`. Traiter en un seul chantier : ces trois domaines partagent la même cause.
4. **Démarrage processus** — +0,2 à +0,3 ms sur **les 13 domaines** (wall Rose 2,48 ms vs C 2,18 ms). Piste : réduire parsing + initialisation JIT au lancement ; c'est le seul levier qui améliore le wall même là où le calcul est déjà gagné.
5. **Surcoût des primitives bigint** — gcd 2,25×, puissance 1,16×, factorial 1,14× (6–463 µs). Faible impact absolu mais écart systématique face à la **même** bibliothèque `num-bigint` côté Rust ; piste : réduire les conversions `Valeur` ↔ `Gz` autour des primitives.

**À protéger, pas à attaquer** [MESURÉ] : rationnel (0,115×), E/S mmap (0,612×), fib récursif (0,929×). **Déjà au niveau** [MESURÉ] : tables (1,055×, dans le bruit).

## Reproduction [MESURÉ]

```bash
cd /tmp/opencode/opt_bench
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR=/tmp/opencode/opt_shared_target
bash scripts/bench_perf.sh                 # 9 tirages entrelacés, garde-fou 12 Go
N_TRIALS=9 bash scripts/bench_perf.sh      # identique (rejoué pour le salvo 2)
python3 bench/perf/diag_jit.py             # diagnostic JIT, 5 tirages/point
python3 bench/perf/check_once.py           # vérification d'un tirage par domaine
```

Le script compile les sources de `bench/perf/src/` (Rust `-O`, C `-O2`, bigint Rust via `cargo --offline`), génère l'entrée E/S de 10 Mio, vérifie toutes les sorties, puis écrit :

- `bench/perf/out/results.json` et `results.csv` — données brutes (wall, cpu, RSS, interne, 9 tirages × 13 domaines × 4 langages) ;
- `bench/perf/out/summary.json` — médianes ;
- `bench/perf/out/results_salvo1.json`, `results_salvo2.json` — les deux salves ;
- `bench/perf/out/diag_jit.json` — diagnostic JIT ;
- `bench/perf/out/salvo_*.txt` — transcriptions des salves ; `out/logs/` — stderr par tirage.

`harness.py` lève une exception si un tirage échoue, si une sortie diffère de la référence ou si le binaire Rose change pendant la salve : aucune valeur n'est produite silencieusement. Sources : `bench/perf/src/*.{rose,rs,c,py}`, `bigint.h`, `Cargo.toml`.
