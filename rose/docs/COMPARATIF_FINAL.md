# COMPARATIF_FINAL — Rose / Rust, domaine par domaine (CONSTAT DATÉ, 19/11/2026)

**Auteur** : sous-agent `cmp1`. **Aucun commit, aucune publication, aucun `.rs`,
`Cargo.toml`, `PLAN.md` ni doc d'un autre agent modifié. `rose/rose` intact.**

---

## 0. Protocole — ce qui a été réellement exécuté

| Élément | Valeur mesurée |
|---|---|
| HEAD | `a9b54098f6b16c74b6e2519e266059a31a564a5e` (arbre propre au départ) |
| Binaire Rose utilisé | `/tmp/opencode/cmp1_agents/rose_cmp1`, construit par `cargo build --release --bin rose` à partir de l'arbre courant, puis copié. **JAMAIS installé dans `rose/rose`.** |
| SHA256 binaire | `2143e996eeff7f9fd1f4014c846a579323fb265e076f9b894be689f1293aad9c` (identique à `rose/rose`, vérifié avant/après) |
| Pinage | `taskset -c 16` (mission), coeur disponible vérifié par `taskset -c 16 echo c16 ok` |
| Échantillons | **9 par implementation**, entrelacés par rotation (ordre A/B alterné aussi pour les racines) |
| Horloges | **deux** : H1 = `time.perf_counter()` (murale) ; H2 = temps interne du programme (`temps()`/`Instant`, « calcul seul ») ; pour les racines H2 = `getrusage(RUSAGE_CHILDREN)` |
| Valeurs de sortie | **vérifiées à chaque échantillon** : multi 81/81 répétitions OK, racines 9/9 par comparaison, backends 9/9 par backend |
| Ratios | `Rose / Rust` ; **< 1 = Rose plus rapide**. Médianes ; min–max entre parenthèses |
| Fenêtre multi | `2026-11-19T19:13:30.906104+00:00` → `19:13:39.594324+00:00` |

**Règle des deux verdicts appliquée** (telle que définie dans `RELEASE_0_1_6.md`,
`RACINES_BAT_RUST.md`, `BOUCLE_BAT_RUST.md`) :

- **(a) « même source »** = même programme / même algorithme des deux côtés : mesure la qualité du **moteur**.
- **(b) « meilleur algorithme »** = on nomme l'algorithme le plus rapide, même s'il diffère. Une victoire **de bibliothèque** (primitive native Rose) est comptée **à part**, jamais comme une victoire « même source ».

**Ce document est un constat daté** : re-mesurer après tout changement de moteur, de quantification ou de topologie. Aucune victoire n'est revendiquée sans mesure rejouée ci-dessous.

---

## 1. Tableau unique — calcul seul (H2), médianes de 9 échantillons, c16

| Domaine | Rose int méd (min–max) | Rust int méd (min–max) | Rose/Rust | (a) Verdict « même source » | (b) Verdict « meilleur algo » (algo nommé) | Nature de la victoire | Rust encore devant ? |
|---|---:|---:|---:|---|---|---|---|
| **D1 `fib(35)`** | 11,567 ms (11,431–12,144) | 12,686 ms (12,632–13,357) | **0,912** ✅ | ✅ **victoire** : même récursion naïve des deux côtés, gain par le moteur | même algo ; le **doublement rapide** O(log n) n'existe que comme primitive Rose `fibonacci()` : 0,011 ms — **bibliothèque, comptée à part** | **moteur** (JIT `b.cs` + slots RIR) | ⚠ **C `-O2` = 9,153 ms (0,791) bat Rose** |
| **D2 boucle 1..10⁶** | 0,100 ms (0,099–0,101) | 0,260 ms (0,257–0,268) | **0,384** ✅ | ❌ **pas de victoire** : même algo itératif, déjà au plancher physique 0,2564 ms (≈1,00) | ✅ **victoire — réassociation en 8 accumulateurs indépendants + déroulage ×8** (source Rose inchangée) | **algorithme** (auto-appliqué par le JIT) | non |
| **D3 premiers 50k** | 0,851 ms (0,846–0,891) | 1,000 ms (0,998–1,003) | **0,851** ✅ | ✅ **victoire** : même division d'essai des deux côtés | même algo ; le **crible** `compter_premiers()` fait 0,085 ms — **bibliothèque, comptée à part** | **moteur** (6 `udiv` supprimés, produits magiques) | non (Rose bat aussi C 0,966 ms) |
| **D4 factorielle 5000** | 1,412 ms (1,403–1,430) | 6,529 ms (6,519–6,564) | **0,216** ✅ | ✅ **victoire** : même produit itératif des deux côtés | même algo ; l'**arbre de produits** `factorielle()` fait 0,369 ms — **bibliothèque, comptée à part** | **moteur** (chemin bigint JIT) | non vs Rust et C ; ⚠ **Python `math.factorial` 0,386 ms bat Rose ×3,7** |
| **D4b PGCD 1000 chiffres** | 0,100 ms (0,098–0,105) *Euclid interprété* | 0,148 ms (0,146–0,194) *Stein maison* | **0,676** ✅ | ❌ **non applicable** : algorithms différents (Euclid côté Rose, Stein côté Rust) | ✅ **victoire « meilleur algo »** ; l'algo le plus rapide est **Stein** : primitif `pgcd()` 0,051 ms (**0,345×** Rust/Stein) — **bibliothèque, comptée à part** | **moteur** (test zéro direct, 93,4 % des conversions decimales supprimées) + **bibliothèque** | non |
| **D5 chaîne 20 000 car.** | 0,011 ms (0,009–0,042) | 0,017 ms (0,014–0,019) | **0,647** ✅ | ✅ **victoire** : source `$s = $s + "x"` inchangée ; spécialisation JIT `CHAINE_ACCUM` | `"x" * 20000` mesuré ~6,3 µs (~19 % plus rapide, **dans le bruit, NON adopté** — changerait ce que D5 mesure) | **moteur** (JIT `str::repeat`) | ⚠ H1 murale **défavorable** : Rose 1,172 ms > Rust 0,952 ms (bruit de démarrage) |
| **D6 liste 100 000** | 0,374 ms (0,330–0,419) | 0,186 ms (0,180–0,198) | **2,011** ❌ | ❌ **perte** (Rust ×2,0) | ❌ **perte** : même avec la boucle inline spécialisée (0,370 ms), Rust reste ×1,99 devant | — (aucune victoire) | ✅ **OUI — Rust devant ×2,0 ; C 0,212 ms aussi devant** |
| **D7 table 3 000 clés** | 0,102 ms (0,093–0,153) | 0,097 ms (0,094–0,102) *HashMap* | **1,052** ≈ | ≈ **égalité dans le bruit ici** (recouvrement des plages) ; salves serrées documentées 0,937–0,968, artifact multi ~1,088 | l'algo le plus rapide est le **hachage** : `Vec assoc` linéaire de Rust = 1,212 ms (×11,8 plus lent) | **moteur / bibliothèque** (partie directe 4 096 entrées, `Vec<u32>` 16 Kio) | ⚠ Rust marginalement devant dans **cette** salve (1,05×, non significatif) |
| **D8 E/S 10 Mo** | 4,421 ms (4,144–5,630) | 7,083 ms (6,649–7,635) | **0,624** ✅ | ✅ **victoire** : même `lire_texte`/`ecrire_texte` vs `fs::read/write` | même algo ; pas de meilleur algo revendiqué | **bibliothèque / syscall** (mmap zéro recopie) | non (Rose bat C 6,999 ms, Python 6,915 ms) |
| **Racines √2 1000 déc., N=500** (H1 murale) | 12,776 ms (12,447–13,403) | 13,031 ms (12,840–13,221) *Rust SANS cache* | **0,980** ✅ | ❌ **perte** : contre Rust **avec** cache, Rose 12,890 ms vs 12,333 ms = **1,045** (H2 CPU : 1,038) | ✅ **victoire « meilleur algo »** : cache du produit pur `n·10^(2d)` (`thread_local HashMap<(BigInt,usize),Rc<BigInt>>`), +9,2 % interne | **bibliothèque** (cache, pas mémoïsation du résultat) | ✅ **OUI au même algo (1,04×)** ; sous-domaines : i64 56,2×, BigInt 1,91×, √2 100 déc. 1,14× |

**Bilan honnête de cette salve** : **7 victoires mesurées** (D1, D2, D3, D4, D4b, D5, D8)
dont **5 sont des victoires « même source, même algo »** au sens strict (D1, D3, D4, D5,
D8) ; **D2 est une victoire d'algorithme** (8 accumulateurs, auto-appliquée par le JIT,
source inchangée) et **D4b une victoire contre un algorithme différent** (Euclid vs Stein —
le gain de bibliothèque réel est Stein 0,051 ms). **Rust reste devant sur D6 (×2,0),
sur les racines à algo strictement égal (×1,04) et marginalement sur D7 (×1,05, bruit).**
**C reste devant Rose sur D1 (0,791). Python reste devant Rose sur D4 (0,273).**

---

## 2. Domaines où Rust gagne encore — mesures brutes (aucune dissimulation)

| Comparaison | Rose | Rust (ou adversaire) | Ratio | Nature de l'écart |
|---|---:|---:|---:|---|
| D6 liste 100k, **même source** | 0,374 ms | 0,186 ms | **2,011 ×** | ~200 000 appels `extern "C"` + deux boucles `pour` à 1,28 ns/it (256 µs) contre 189 µs à LLVM ; ~305 µs de `str`/`ldr` observables que le JIT doit émettre |
| Racines √2 1000 déc., **même source** (cache des deux côtés) | 12,890 ms (12,402–13,248) | 12,333 ms (12,093–12,579) | **1,045 ×** (CPU 1,038) | surcoût d'interprétation : construction des `Valeur`, appel de primitive, `longueur()` sur la chaîne — la primitive seule est **13 % plus rapide** avec le cache |
| Racines i64, N=2 000 000 | 516,829 ms (514,413–532,408) | 9,198 ms (8,464–10,519) | **56,19 ×** | chemin i64 intact par le cache ; un appel de primitive interprété par itération |
| Racines BigInt 50 ch., N=20 000 | 14,948 ms (14,655–15,279) | 7,828 ms (7,652–7,939) | **1,91 ×** | chemin `d = 0` intact (bruit BASE/CACHE 1,015×) |
| Racines √2 100 déc., N=5 000 | 8,901 ms (8,795–9,263) | 7,805 ms (7,681–8,060) | **1,14 ×** | cache +11 % interne mais insuffisant |
| D7 table, cette salve | 0,102 ms | 0,097 ms | **1,052 ×** | artifact de trous ~100 ms du harnais multi, documenté ; salves serrées 0,937–0,968 ailleurs |
| D1 fib contre **C `-O2`** | 11,567 ms | 9,153 ms | **1,264 ×** | prologue / codegen C plus léger — **Rose ne bat pas C sur fib** |
| D4 fact contre **Python** | 1,412 ms | 0,386 ms | **3,66 ×** | Python appelle un produit arbre/implémentation C efficace |

---

## 3. Nouveaux gains non encore intégrés au comparatif global — état mesuré

| Gain | Ce qui est réellement mesuré ici | Ce qui n'est PAS démontré |
|---|---|---|
| **Table directe (TA2)** | `Vec<u32>` 4 096 entrées / 16 Kio ; recherche −33 % ; D7 à 0,102 ms (ratio 1,052 dans le harnais multi, 0,937–0,968 en salves serrées documentées) | pas de 4ᵉ victoire nette revendiquée : le multi reste à ~1,0–1,09 |
| **Cache racines (RA2)** | `n·10^(2d)` en cache, +9,2 % interne ; verdict « meilleur algo » **0,980** ici (0,965/0,975 documenté) | verdict « même source » **perdu 1,045** ici (1,033/1,037 documenté) — aucune victoire à algo égal |
| **ABI G5 (tag 3 bits bas : `n<<3`, `ptr|1`, `ptr|2`)** | SIGSEGV d'origine supprimé ; `fib(10)`→55 et `BASE`→1 rc=0 ; émission ELF rc=0 | **aucun gain de vitesse mesuré** dans ces benches ; réflexivité **non atteinte** (ELF émis mais exécution rc=139 sur boucle d'allocation ~2,8 M appels) |
| **E/S syscalls G5-IO (IO1)** | `nb_arguments`, `argument`, `lire_texte`, `ecrire_bytes`/`ecrire_texte` **vraiment émis dans l'ELF** via `svc #0` ; sortie ELF = Rose natif octet pour octet (md5 égal) | gain de **complétude du compilateur**, pas de vitesse du moteur interprété ; ELF du compilateur : rc=139, 0 octet de dump même à 4 Gio de tas ; UTF-8 (points de code) vs octets non unifié ; `table`/`inserer`/`cle_existe` restent des stubs |

---

## 4. Ce qui reste à faire pour être plus rapide en tout point (causes chiffrées)

1. **D6 liste — écart 0,188 ms (2,011×)**. Cause mesurée : ~200 000 appels `extern "C"` et deux boucles `pour` à 1,28 ns/it (256 µs) contre 189 µs à LLVM ; le JIT doit émettre ~305 µs de `str`/`ldr` observables (les sources Rust font aussi `v.iter().fold`). **À faire** : codegen inline des boucles listes — explicitement hors de la recette de voie typée actuelle. Rejets déjà mesurés : `resize` 0,866 ms, gabarit `memset` 0,53–0,57 ms, `dmb sy` 0,69 ms (régression), fusion de boucles (résultat faux 11 249 925 000).
2. **Racines à algo égal — écart 4,5 % (1,045×)**. La primitive avec cache est **13 % plus rapide** que sans ; le surcoût d'interprétation annule le gain. **À faire** : supprimer la construction des `Valeur` / l'appel de primitive / `longueur()` sur la chaîne dans le chemin racines. Sous-domaines non concernés par le cache : i64 **56,2×**, BigInt **1,91×**, √2 100 déc. **1,14×**.
3. **D7 table — écart résiduel 0,005 ms (1,052× ici)**. Artifact de trous ~100 ms du harnais multi (documenté) ; salves serrées 0,937–0,968. **À faire** : éliminer le dispatch interprète restant sur le chemin général de `cle_existe`.
4. **D1 fib contre C — écart 2,414 ms (1,264×)**. Rose bat Rust (0,912) mais pas C. **À faire** : réduire le prologue du codegen JIT (le gain RIR slots est déjà acquis sur la récursion elle-même).
5. **D4 factorielle contre Python — écart 1,026 ms (3,66×)**. Le primitif arbre de produits Rose fait déjà 0,369 ms. **À faire** : rendre l'arbre de produits accessible au chemin JIT sans changer la source du bench.
6. **D5 chaîne — plancher de synchronisation**. Ratios internes bruités (Rose 0,009–0,042 ms, Rust 0,014–0,019 ms) et H1 murale défavorable (1,172 vs 0,952 ms). Plancher ~1,7–2,2 ms de synchronisation slots↔globaux documenté dans `rir.rs`, **hors périmètre de l'agent**. **À faire** : supprimer ce plancher puis re-mesurer sur deux horloges avant de revendiquer une 8ᵉ victoire.
7. **Réflexivité G5 — obstacle fonctionnel, pas de vitesse**. Boucle d'allocation ~2,8 M appels dans la tokenisation (tas épuisé à 64 Mio, 512 Mio et 4 Gio), sémantique UTF-8 vs octets, `table`/`inserer`/`cle_existe` stubs, appels à > 8 arguments écrasent `x8`/`x9`. **À faire** : corriger ces quatre points avant de parler de self-hosting.

---

## 5. Rejouabilité — commandes et sorties exactes

### 5.1 Bench multi (c16, 9 échantillons, binaire isolé)

```
cd /tmp/opencode/cmp1_agents
sha256sum rose_cmp1
# 2143e996eeff7f9fd1f4014c846a579323fb265e076f9b894be689f1293aad9c  rose_cmp1
PIN_CPU=16 NSAMP=9 python3 bench_cmp1.py | tee cmp1_multi_run.log
# HORODATAGE DEBUT : 2026-11-19T19:13:30.906104+00:00
# PIN_CPU=16  N=9  rose_sha256=2143e996...  /tmp/opencode/cmp1_agents/rose_cmp1
# ... 9/9 "rep OK" pour CHACUN des 9 domaines (81/81) ...
# FIN HORODATAGE : 2026-11-19T19:13:39.594324+00:00
# CSV: /tmp/opencode/cmp1_agents/cmp1_results.csv
# JSON: /tmp/opencode/cmp1_agents/cmp1_summary.json
```

`bench_cmp1.py` est une copie de `/tmp/opencode/benchmulti/bench.py` avec **seules**
les lignes `ROSE` et les chemins de sortie modifiés ; `HERE` reste le dossier des
sources et des binaires Rust/C partagés. Le harnais lève une exception si une
valeur de sortie diffère de l'attendu : **aucune n'a différé** (ex. `9227465` pour
fib, `3000` pour table, `10485760` pour E/S).

### 5.2 Racines — double verdict (c16, 9 échantillons, alternance A/B)

```
cd /home/clausake/Stefan/ROSE
HARN="python3 /tmp/opencode/cmp1_agents/harness_c16.py"
ROSE=/tmp/opencode/cmp1_agents/rose_cmp1
OLD=/tmp/opencode/ra2_agents/rust_ref_OLD    # md5 46c284009acd47d8cb828fffe36ec822 (sans cache)
NEW=/tmp/opencode/ra2_agents/rust_ref_NEW    # md5 ecc557a11e8c80df810012925b2fd848 (avec cache)

$HARN --label 'RAC_A_meilleur_algo_dec1000_old' --n 9 \
  --a "$ROSE" bench/racines/racines_dec.rose 500 1000 --b "$OLD" decs 500 1000
# checksums: A='500500 | 12'  B='decs 500 12131722 500500 1000'
# [murale] A médiane 12,776 ms / B 13,031 ms → A/B = 0,9805  (A plus vite)
# [CPU    ] A médiane 12,694 ms / B 12,937 ms → A/B = 0,9812  (A plus vite)

$HARN --label 'RAC_B_meme_source_dec1000_new' --n 9 \
  --a "$ROSE" bench/racines/racines_dec.rose 500 1000 --b "$NEW" decs 500 1000
# checksums: A='500500 | 12'  B='decs 500 11369179 500500 1000'
# [murale] A médiane 12,890 ms / B 12,333 ms → A/B = 1,0451  (B plus vite)
# [CPU    ] A médiane 12,745 ms / B 12,278 ms → A/B = 1,0380  (B plus vite)

# Sous-domaines (même harnais, adversaire OLD) :
# C1 i64 2M    : 516,829 ms vs 9,198 ms  → 56,1900 ×
# C2 big 20k   :  14,948 ms vs 7,828 ms  →  1,9095 ×
# C3 dec100 5k :   8,901 ms vs 7,805 ms  →  1,1405 ×
```

`harness_c16.py` est une copie de `bench/racines/harness.py` avec **unique**
`PIN = ["taskset","-c","16"]`. Les checksums sont identiques des deux côtés à
chaque échantillon (`500500`, `4000005000000`, `505000`).

### 5.3 Backends Rose (c16, 9 échantillons)

```
python3 /tmp/opencode/cmp1_agents/run_backends_c16.py | tee /tmp/opencode/cmp1_agents/backends_run.log
# 7 backends construits : jit, interp, norir, nat, comp, orc, llvm (aucun échec)
# 9/9 échantillons vérifiés pour chacun ; CHECKSUM 2666664666667886537 commun
# mur médian : jit 23,379 ms · interp 347,017 · norir 23,579 · nat 329,914
#              comp 24,402 · orc 3,262 (noyau seul) · llvm 277,973 (noyau seul)
# interne arith : nat 8,438 ms < jit 11,839 ms  (l'émetteur natif bat le JIT sur arith pur)
# SONDES : selfhost rc=0 mais ELF non exécuté dans ce harnais ; rvm rc=1 (AST refusé)
```

`run_backends_c16.py` est une copie de `bench/backends/run_backends.py` avec `ROOT`,
`ROSE` et `PIN` ajustés, plus `errors="replace"` pour que la sonde `selfhost`
(qui écrit des octets ELF sur stdout) ne fasse pas échouer le harnais.

### 5.4 Validations

```
cd /home/clausake/Stefan/ROSE
R=/tmp/opencode/cmp1_agents/rose_cmp1
M1=$($R rir compilateur/emetteur.rose 2>&1 | md5sum | awk '{print $1}')
M2=$($R compilateur/emetteur.rose compilateur/emetteur.rose 2>&1 | md5sum | awk '{print $1}')
echo "$M1 / $M2"
# 8ac53eed3d4967a0d7d9ab333256801c / 8ac53eed3d4967a0d7d9ab333256801c
# → POINT FIXE ×2 ATTEINT

cd rose && export PATH="$HOME/.cargo/bin:$PATH" && cargo test --release
# test result: ok. 220 passed; 0 failed; 3 ignored      (lib)
# test result: ok. 204 passed; 0 failed; 3 ignored      (bin)
# test result: ok. 6 passed; 0 failed                   (doc/intégration)
# test result: ok. 1 passed; 0 failed
# → 431 passed, 0 failed
```

---

## 6. Règle de lecture

- Toutes les valeurs ci-dessus sont des **constats datés du 19/11/2026, pin c16, binaire `2143e996…`**. Elles ne sont pas des consignes.
- Les ratios « même source » et « meilleur algo » **ne doivent pas être confondus** : D2 est gagné par l'algorithme (8 accumulateurs), D4b par un algorithme différent (et le vrai gain de bibliothèque est Stein 0,051 ms), les racines sont gagnées « meilleur algo » mais perdues « même source ».
- Les primitives natives (`fibonacci`, `compter_premiers`, `factorielle`, `pgcd`) sont des **victoires de bibliothèque comptées à part**, pas des victoires « même source ».
- Aucune porte non mesurée n'est présentée comme atteinte : self-hosting **PARTIEL**, réflexivité G5 **non atteinte**, D6/Racines-à-algo-égal/D7 **perdus ou au niveau**.
