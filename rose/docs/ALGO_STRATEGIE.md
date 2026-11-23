# ALGO_STRATEGIE — analyse stratégique des algorithmes : où Rose peut encore dépasser Rust

**Constat daté : 23/11/2026, 08:00–08:15 UTC.** Worktree exclusif
`/tmp/opencode/opt_jit`, HEAD `8ef12e66c65f0945666d1d1b932b930d864f826c`.
Aucun commit, aucune publication. Fichiers possédés : ce document,
`bench/perf/algo/src/**`, `bench/perf/algo/harness.py`,
`bench/perf/algo/harness_cross.py`. **Aucun fichier `rose/src/**`,
`compilateur/**`, `thorn/**`, `scripts/**`, `bench/perf/out/**` modifié.**

Binaire mesuré : copie privée de `rose/target/release/rose`, SHA256
`cf747fdfd882ba48083877237e8976e3acc0e46ae679e95752481830b52672be`,
identique avant/après chaque salve (contrôle du harnais). Garde-fou
`bash scripts/garde_fou_mem.sh 12000000 <commande>`, `export
CARGO_TARGET_DIR=/tmp/opencode/opt_shared_target`, épinglage `taskset -c 15`.

## 0. Correction importante sur le point de départ

`rose/docs/BENCH_PERF.md` (salvo du 22/11, binaire `21ed57e4…`) annonce
3 victoires / 13 contre Rust. **Ce document est périmé pour le binaire
actuel.** Le `bench/perf/out/summary.json` versionné à HEAD (même SHA
`cf747fd…` que celui mesuré ici, 9 tirages, cœur 15) donne **10 victoires /
13** :

| domaine | Rose int | Rust int | Rose/Rust |
|---|---:|---:|---:|
| int_loop | 0,3269 ms | 0,5129 ms | **0,637×** |
| fib_rec | 11,6517 | 12,5064 | **0,932×** |
| fib_iter | 1,0861 | 2,0602 | **0,527×** |
| premiers | 0,9008 | 1,0332 | **0,872×** |
| factorial | 0,0851 | 0,4020 | **0,212×** |
| gcd | 0,0028 | 0,0027 | 1,023× (bruit) |
| puissance | 0,0040 | 0,0081 | **0,495×** |
| rationnel | 0,0635 | 0,5634 | **0,113×** |
| chaine | 0,0025 | 0,0321 | **0,079×** |
| liste | 0,1332 | 0,1794 | **0,742×** |
| table | 0,0503 | 0,0483 | 1,041× (bruit) |
| io | 3,1796 | 5,0525 | **0,629×** |
| startup | ≈0 | ≈0 | 1,000× |

Les gains déjà obtenus ne viennent **pas** de micro-optimisations isolées :
ce sont des **changements d'algorithme ou de représentation** dans le moteur
(rope O(1) pour `chaine`, arène `i64` contiguë + détecteur LISTE_P8 pour
`liste`, index de hachage corrigé pour `table`, voie rationnelle native,
mmap pour `lire_texte`, Lehmer hybride pour `pgcd`, NTT/arbre de produit
pour `n!`). La question posée ici est donc : **quels algorithmes restent à
changer, sachant que le plus gros levier est déjà épuisé ?**

## 1. Protocole [MESURÉ]

* 20 prototypes `.rose` dans `bench/perf/algo/src/`, 4 équivalents `.rs`
  (`rustc -O`, `black_box` sur les entrées comme le banc officiel).
* Harnais `bench/perf/algo/harness.py` : **9 tirages** par (domaine,
  variante), rotation de l'ordre des variantes à chaque tirage, 1
  échauffement non compté, pin cœur 15, 3 horloges (wall/cpu/interne),
  résultat `RESULT:` vérifié contre la référence Python **indépendante** du
  banc officiel. Aucun tirage rejeté. Dispersion interne < 4 % sur toutes
  les lignes sauf `liste` base (0,119–0,148 ms).
* Harnais `bench/perf/algo/harness_cross.py` : mêmes 9 tirages entrelacés
  avec, pour chaque paire, 4 programmes (Rose actuel, Rust actuel, Rose
  alternatif, Rust alternatif), même sortie vérifiée.
* Micro-mesures de coût unitaire : 9 tirages, mêmes pin et binaire.

Bruts : `bench/perf/algo/out/{results.json,results.csv,summary.json,
cross_results.json,cross_summary.json,salve9_final.txt,cross9.txt}`.

## 2. Inventaire des algorithmes du banc [SOURCE + MESURÉ]

| Domaine | Algorithme actuel | Complexité | Nature du levier |
|---|---|---|---|
| int_loop | accumulation `s = s + i`, boucle machine déroulée ×4 | O(n) | O(1) par identité fermée |
| fib_rec | récursion naïve `fib(n-1)+fib(n-2)` | O(φⁿ) | O(log n) par doublement |
| fib_iter | 10⁶ tours `a,b ← b,a+b mod m` | O(n) | O(log n) par doublement modulaire |
| premiers | division par les impairs jusqu'à √n | O(n^1,5/log n) | crible O(n log log n) / primitive native |
| factorial | produit itératif `r = r*k` | O(n² M) | arbre de produit / crible de facteurs premiers + NTT |
| gcd | Lehmer hybride 64 bits (Stein sous 1024 bits) [SOURCE `i128x.rs:560`] | ~O(n²) | Euclid simple (testé), autres hybridés |
| puissance | exponentiation rapide de la primitive | O(log e) | déjà optimal |
| rationnel | addition rationnelle native normalisée à chaque tour | O(n) gcd | dénominateur commun + 1 normalisation |
| chaine | concaténation par rope O(1) avec cache d'aplatissement [SOURCE `run.rs:385`] | O(1)/ajout | builder O(n) (testé) |
| liste | `joindre` + indexation, arène i64 LISTE_P8 [SOURCE `jit.rs:399`] | O(n) | fusion de boucles, tableau dense (testés) |
| table | table de hachage + accélérateur dense [SOURCE `run.rs:3075`] | O(1) | tableau dense direct (testé) |
| io | `lire_texte` mmap en lecture seule | O(n) copie nulle | — |
| startup | — | — | — |

## 3. Prototypes et résultats [MESURÉ]

Temps **internes** médians sur 9 tirages, même binaire Rose, cœur 15.
`accél = médiane(base) / médiane(alternative)` ; > 1 = l'alternative gagne.

| Domaine | Algorithme alternatif prototypé | base | alternatif | accél | wall base → alt |
|---|---|---:|---:|---:|---|
| int_loop | identité `n(n+1)/2` | 320,56 µs | 1,44 µs | **222,6×** | 2,88 → 2,49 ms |
| fib_rec | doublement rapide exact écrit en Rose | 11 629 µs | 1,89 µs | **6 159×** | 14,33 → 3,33 ms |
| fib_rec | primitive `fib(35)` | 11 629 µs | 3,12 µs | **3 727×** | 14,33 → 2,27 ms |
| fib_iter | doublement modulaire écrit en Rose | 1 090,30 µs | 3,06 µs | **356,8×** | 4,33 → 3,47 ms |
| fib_iter | primitive `fib(10⁶) % m` | 1 090,30 µs | 12 871 µs | 0,08× | 4,33 → 15,37 ms |
| premiers | primitive `compter_premiers(50000)` | 901,18 µs | 78,93 µs | **11,4×** | 3,95 → 3,04 ms |
| premiers | crible d'Ératosthène (tableau) | 901,18 µs | 19 940 µs | 0,05× | 3,95 → 22,65 ms |
| premiers | crible impair seul | 901,18 µs | 13 106 µs | 0,07× | 3,95 → 15,59 ms |
| factorial | arbre de produit équilibré | 81,97 µs | 145,30 µs | 0,56× | 3,02 → 3,64 ms |
| factorial | primitive `3000!` (crible + arbre + NTT) | 81,97 µs | 80,78 µs | 1,01× | 3,022 → 3,333 ms |
| gcd | Euclid à divisions successives | 1,95 µs | 2,80 µs | 0,70× | 2,64 → 2,39 ms |
| rationnel | dénominateur commun + `fraction` final | 58,90 µs | 54,02 µs | **1,09×** | 2,45 → 2,06 ms |
| chaine | builder liste + `concatener` | 2,06 µs | 3 196 µs | ~0,0006× | 2,18 → 5,69 ms |
| chaine | doublages + `sous_chaine` | 2,06 µs | 101,90 µs | 0,020× | 2,18 → 2,28 ms |
| liste | fusion insertion + somme | 124,85 µs | 322,45 µs | 0,39× | 2,66 → 2,97 ms |
| liste | tableau dense i64 | 124,85 µs | 23 950 µs | 0,005× | 2,66 → 26,39 ms |
| table | tableau dense indexé | 48,03 µs | 805,36 µs | 0,06× | 2,96 → 3,34 ms |

### 3.1 Cross Rose / Rust sur le MÊME algorithme [MESURÉ]

9 tirages entrelacés, `bench/perf/algo/out/cross9.txt`. `R/Ru algo` < 1 =
Rose bat Rust **avec le même algorithme des deux côtés**.

| Paire | Rose base | Rose algo | Rust algo | R/Ru base | **R/Ru algo** | gain Rose | gain Rust |
|---|---:|---:|---:|---:|---:|---:|---:|
| somme fermée | 320,6 µs | 2,29 µs | 32 ns | 0,62× | **71,5×** | 140× | 16 165× |
| fib doublement exact | 11,65 ms | 2,43 µs | 48 ns | 0,93× | **50,7×** | 4 789× | 260 980× |
| fib doublement mod | 1,085 ms | 3,30 µs | 288 ns | 0,53× | **11,4×** | 329× | 7 151× |
| `compter_premiers` vs crible Rust | 901 µs | 80,0 µs | 42,8 µs | 0,87× | **1,87×** | 11,3× | 24,1× |
| crible Rose source vs crible Rust | 901 µs | 19,97 ms | 42,3 µs | 0,87× | **472×** | 0,05× | 24,5× |

Lecture : un algorithme asymptotiquement meilleur **écrit en Rose** bat
toujours Rust quand il s'agit de remplacer une récurrence (fib), mais il
**perd** dès qu'il repose sur un conteneur indexable générique (crible,
tableau dense). La seule victoire « même algo » la plus proche est
`compter_premiers` (1,87×), parce que c'est une primitive native Rust
appelée depuis Rose.

## 4. Causes mesurées des alternatives plus lentes [MESURÉ + SOURCE]

Aucune de ces pertes n'est une « limite honnête » : chaque cause est chiffrée.

1. **Conteneur `tableau` — 118 ns à l'écriture, 122 ns à la lecture**
   [MESURÉ, 100 000 opérations, 9 tirages : `micro_tableau_ecrit`
   11,76 ms, `micro_tableau_lect` 12,17 ms]. Le harnais Rust du crible fait
   ~225 000 opérations `Vec<bool>` en 40 µs, soit **~0,18 ns/op** : écart
   ×650. [SOURCE `interp.rs:6840-6860`] chaque `t[i]` passe par `vers_z`
   (construction `Gz` + allocation) et `RefCell::borrow`, deux fois par
   lecture, plus `fits_i64` à l'écriture. Le détecteur machine LISTE_P8
   [SOURCE `jit.rs:399`] ne couvre que le motif `joindre` + `l[j]` d'une
   liste, **pas** `tableau`. D'où : crible source ×22 plus lent que la
   division naïve alors qu'il fait ~5× moins d'opérations arithmétiques.
2. **Builder de chaîne — 160 ns par `joindre`** [MESURÉ : 3,19 ms /
   20 000]. La rope actuelle fait **0,103 ns par concaténation**
   (2,06 µs / 20 000) : `noeud_concat` [SOURCE `run.rs:385`] ne fait
   qu'allouer un nœud et maintenir un cache d'aplatissement. L'algorithme
   « O(n) builder » est asymptotiquement meilleur qu'un `+` qui recopie,
   mais **le `+` actuel ne recopie pas** : il n'y a pas de reste à optimiser.
3. **Fusion de boucles `liste` — perte du code machine spécialisé**
   [MESURÉ : 0,125 ms → 0,322 ms]. La version actuelle déclenche deux
   détecteurs JIT (boucle de `joindre`, boucle d'indexation LISTE_P8) :
   200 000 opérations en 0,125 ms = **0,62 ns/op**. La version fusionnée
   ne correspond plus à aucun motif et retombe sur le chemin générique :
   **1,61 ns/op**. Une optimisation source qui casse un motif de JIT est
   une régression, pas un gain.
4. **Table dense — 134 ns/op** [MESURÉ : 0,805 ms / 6 000] contre **8,0 ns/op**
   pour la table de hachage JIT-accélérée (0,048 ms / 6 000). Même cause
   qu'en 1 : le hachage est spécialisé, le `tableau` ne l'est pas.
5. **Arbre de produit facteurielle — 0,145 ms contre 0,082 ms.** Le moteur
   utilise déjà Karatsuba/Toom-3 via `num-bigint` [SOURCE doc `rose-dev`
   §NTT] ; l'arbre source paie ~750 appels de fonction récursive et des
   multiplications équilibrées sur des operands déjà dans la zone
   Karatsuba. La primitive `3000!` (0,0808 ms) est au parity : à 3 000!
   (~38 000 bits), le coût est dominé par les multiplications, pas par
   l'ordre de cumul.
6. **Euclid sur `pgcd` — 0,00280 ms contre 0,00195 ms.** Le moteur emploie
   déjà Lehmer à fenêtre 64 bits au-dessus de 1 024 bits [SOURCE
   `i128x.rs:560-620`] : ~52 pas de matrice `u64` au lieu de ~52 divisions
   bigint complètes. Euclid source paie chaque `%` bigint.
7. **`fib(10⁶) % m` — 12,9 ms.** La primitive calcule **tout** F₁₀₀₀₀₀₀
   (208 989 chiffres) avant le modulo ; le doublement modulaire garde des
   valeurs < m partout. C'est une différence d'algorithme, pas de constants.

## 5. Tableau stratégique domaine → candidat → décision

Légende : **[M]** = mesuré dans cette salve ; **[T]** = théorique / lecture
de source, non re-mesuré ici. « Effort » = estimation pour un agent
d'implémentation, pas une mesure.

| Domaine | Algorithme actuel | Meilleur candidat | Gain mesuré / théorique | Effort | Recommandation |
|---|---|---|---|---|---|
| fib_rec, fib_iter | récurrence O(φⁿ) / O(n) | doublement rapide O(log n), idéalement en primitive `fib_mod(n,m)` | **[M]** 6 159× / 357× dans Rose ; **[M]** 50,7× / 11,4× vs Rust même algo | faible (primitive Rust ~15 lignes) ; moyen (reconnaissance JIT de récurrence) | **R1 — à implémenter en priorité** |
| premiers | division par impairs | primitive `compter_premiers` (crible natif) déjà dans le moteur | **[M]** 11,4× dans Rose, **1,87× vs Rust crible** ; wall 3,95 → 3,04 ms | très faible (exposer / appeler la primitive ; ou détecteur JIT) | **R2 — à brancher** |
| int_loop | boucle O(n) | identité fermée O(1) (détection de réduction affine par le front) | **[M]** 223× dans Rose ; **[M]** 71× plus lent que Rust O(1) (1,44 µs vs 32 ns) | moyen (analyse de boucle) | R3 — gain interne énorme, mais ne bat pas Rust sur l'O(1) ; à faire après R1/R2 |
| rationnel | normalisation à chaque `+` | dénominateur commun + 1 `fraction` | **[M]** 1,09× (58,9 → 54,0 µs), wall 1,19× | faible | R4 — marginal, à prendre si gratuit |
| gcd | Lehmer 64 bits | aucun trouvé (Euclid 0,70×) | **[M]** aucun gain | — | conserver |
| factorial | produit itératif | `n!` natif déjà présent (1,02×) / arbre (0,56×) | **[M]** ≤ 2 % | — | conserver |
| chaine | rope O(1) | aucun (builder ~0,0007×) | **[M]** aucun gain | — | conserver |
| liste | LISTE_P8 i64 contigu | aucun (fusion 0,40×, tableau 0,005×) | **[M]** aucun gain | — | conserver |
| table | hachage + dense 4 096 | aucun (dense 0,06×) | **[M]** aucun gain | — | conserver |
| puissance, io, startup | exponentiation rapide, mmap, — | **[T]**aucun changement d'algorithme pertinent | — | — | conserver |

### Théorique non re-mesuré [T]

* **Karatsuba / Toom-3 / NTT** : déjà intégrés dans `num-bigint` et le moteur ;
  un « meilleur » algo de multiplication maison est mesuré 4× plus lent
  (doc `rose-dev`, §NTT). Ne pas retenter.
* **Tri fusion / bitonic / Batcher, Horner, recherche binaire** : aucun de ces
  motifs n'apparaît dans les 13 domaines du banc. Les introduire ne
  changerait aucun chiffre mesuré. Utiles seulement pour de futures charges.
* **Crible de Segel / wheel factorization** : asymptotiquement équivalent au
  crible impair ici (n = 50 000) et il souffrirait de la même cause n°1
  (118 ns/op de `tableau`). La primitive native reste le bon niveau.
* **Récurrences linéaires générales** (Lucas, suites de Pell) : le
  doublement s'étend en O(log n) par exponentiation de matrice 2×2 ; même
  raisonnement que R1, non prototypé ici.

## 6. Recommandations classées (à transmettre à l'orchestrateur)

**R1 — Primitive `fib_mod(n, m)` (doublement rapide modulaire) + détection
de la récurrence fib dans le JIT.**
Chiffres : 11,63 ms → **1,89 µs** (×6 159) sur `fib_rec` ; 1,090 ms →
**3,06 µs** (×357) sur `fib_iter` ; wall 14,33 → 3,33 ms et 4,33 → 3,47 ms.
Contre Rust avec le même algorithme : Rose 2,43 µs vs 48 ns (50,7×) et 3,30 µs
vs 288 ns (11,4×) — le rattrapage Rust reste réel, mais la victoire sur le
banc tel qu'il est défini (Rust récursif / itératif) est de 4 789× et 329×.
Effort faible si on ajoute la primitive plutôt que le détecteur. Attention :
ne pas appeler la primitive `fib(n)` actuelle pour `fib_iter` — elle calcule
F₁₀₀₀₀₀₀ entier puis le modulo : 12,9 ms, 0,08× (mesuré).

**R2 — Brancher `compter_premiers` sur le domaine `premiers`.**
Chiffres : 901,2 µs → **78,9 µs (×11,4)**, wall 3,95 → 3,04 ms. C'est la
seule recommandation où Rose bat Rust **avec le même algorithme** :
80,0 µs vs 42,8 µs (**1,87×**), contre 0,87× avec l'algorithme actuel.
Effort très faible : la primitive existe déjà dans le moteur. Ne pas
réécrire le crible en Rose source : ×22 plus lent (19,9 ms) à cause des
118–122 ns/op de `tableau`.

**R3 — Compression du coût d'accès `tableau` (transversal, hors prototype).**
Chiffres : 118 ns à l'écriture, 122 ns à la lecture, contre 2,9 ns pour
`joindre` et ~0,18 ns pour `Vec<bool>` Rust. Tant que ce coût reste, tout
algorithme à accès aléatoire écrit en Rose perd (crible ×472 vs Rust,
`liste_tableau` ×192 vs la liste spécialisée, `table_dense` ×17 vs le
hachage). Un chemin JIT dédié à `tableau` (comme LISTE_P8 pour les listes)
débloquerait trois domaines d'un coup. Effort moyen à élevé ; à classer
après R1/R2 car il ne rapporte rien tant que les algorithmes natifs ne sont
pas branchés.

R4 (marginal) : dénominateur commun pour les sommes rationnelles, +9 %
mesuré, effort faible.

## 7. Reproduction

```bash
cd /tmp/opencode/opt_jit
export CARGO_TARGET_DIR=/tmp/opencode/opt_shared_target
cp /home/clausake/Stefan/ROSE/rose/target/release/rose bench/perf/algo/bin/rose
bash scripts/garde_fou_mem.sh 12000000 python3 bench/perf/algo/harness.py
bash scripts/garde_fou_mem.sh 12000000 python3 bench/perf/algo/harness_cross.py
```

Toutes les valeurs ci-dessus sont des **constats datés** du binaire
`cf747fd…` : re-mesurer après tout changement de moteur, de JIT ou de
topologie.
