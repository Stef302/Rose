# Benchmark comparatif — Rose / Rust pur / Python / primitive native

Horodatage des mesures : **2026-11-18T12:03:06Z → 12:04:52Z** (salves pinées : 12:04:02Z → 12:04:10Z ; expérience jardin approfondie : 12:04:43Z).
Machine : **aarch64**, 20 cœurs hétérogènes (10 Cortex-X925 `cpu_capacity≈1017`, 10 Cortex-A725 `cpu_capacity≈718`), `governor=performance`. Toutes les mesures citées dans les tableaux sont **pinées sur le cœur 15** (X925) via `taskset -c 15` : sans pinage, l'ordonnanceur baladait les processus entre gros et petits cœurs et produisait des distributions **bimodales** (ex. `fib` Rust pur : 12,6 ms sur X925 contre 36,4 ms sur A725 ; factorielle Rose JIT : 1,50 ms / 2,45 ms). Voir `results.csv` (non piné) vs `results_pinned.csv` (piné).

Binaire : `rose/rose` SHA256 `72d8cc31089e80ef92717ac1…`, Rose 0.1.5, commit `210ad03`, branche `main`. `rustc 1.99.1`, `python3 3.12.3`.

## Protocole

- **9 échantillons** par implémentation, **entrelacés par rotation de l'ordre** (A/A/A …) dans une même salve ; 25 échantillons pour l'expérience « 40 % jardins ». Médianes, min et max rapportés.
- Deux horloges **distinctes et non confondues** :
  1. **processus complet** (`wall`) : `time.perf_counter_ns()` autour de `subprocess.run` — inclut le démarrage, le parsing, la compilation JIT et l'affichage ;
  2. **exécution interne** (`interne`) : `temps()` Rose (nanosecondes), `std::time::Instant` Rust, `time.perf_counter_ns()` Python — chronomètre posé autour du **calcul seul**, affichage exclu.
- Résultat vérifié à chaque échantillon (rc = 0 et sortie attendue) : `9227465`, `5133`, `16326`. Aucun échantillon rejeté.
- **Même entrée, même machine, même cœur.** Convertisseurs décimaux (`longueur(chaine(...))`, `str(...)`, `to_string()` du limb de tête) placés **hors** de la zone chronométrée dans les trois langages.

### ⚠ Avertissement sur la quatrième colonne

« Rose primitif natif » = le binaire Rose appelant une primitive **écrite en Rust dans le moteur** (`fibonacci`, `compter_premiers`, `factorielle`, définies dans `rose/src/run.rs`). **Ce ne sont PAS les mêmes algorithmes** :

| Primitives natives Rose | Algorithme réel | Ligne source |
|---|---|---|
| `fibonacci(35)` | doublement rapide O(log n), multiplications de bigints | `run.rs:784` |
| `compter_premiers(50000)` | crible d'Ératosthène | `run.rs:821` |
| `factorielle(5000)` | facteurs premiers (Legendre) + arbre de produit équilibré, parallèle | `run.rs:775` → `i128x.rs:27` |

La comparaison **« même algorithme »** est donc celle des trois premières colonnes (Rose JIT / Rust pur / Python). La quatrième colonne mesure **le meilleur code natif disponible dans le moteur**, pas une accélération du même algorithme. Ne pas en conclure « Rose bat Rust partout ».

## 1. Fibonacci récursif — `fib(35)` (9227465)

Même récursion naïve `f(n)=f(n-1)+f(n-2)` dans Rose, Rust et Python.

| Implémentation | wall médian (ms) | interne médian (ms) | min–max interne | rapport interne / Rose JIT |
|---|---:|---:|---:|---:|
| **Rose JIT (source Rose)** | 209,686 | **207,982** | 207,851 – 208,138 | 1,00 |
| Rust pur (`rustc -O`) | 13,691 | **12,633** | 12,577 – 12,673 | **0,06** (Rust 16,5× plus rapide) |
| Python (récursif) | 601,465 | **590,498** | 589,984 – 591,072 | 2,84 (Rose 2,84× plus rapide que Python) |
| Rose primitif natif (doublement rapide) | 1,202 | **0,010** | 0,008 – 0,010 | 0,00005 — **algorithme différent** |

Overhead de démarrage (médiane `wall − interne`) : Rose 1,70 ms ; Rust 1,06 ms ; Python 10,97 ms ; primitif Rose 1,19 ms.

## 2. Comptage des premiers < 50000 — division jusqu'à √n (5133)

Même test de division impaire jusqu'à √n dans Rose, Rust et Python.

| Implémentation | wall médian (ms) | interne médian (ms) | min–max interne | rapport interne / Rose JIT |
|---|---:|---:|---:|---:|
| **Rose JIT (source Rose)** | 28,591 | **26,984** | 26,930 – 27,025 | 1,00 |
| Rust pur (`rustc -O`) | 2,095 | **1,003** | 1,000 – 1,007 | **0,04** (Rust 26,9× plus rapide) |
| Python (division) | 27,594 | **17,584** | 17,517 – 17,717 | 0,65 (**Python 1,53× plus rapide que Rose JIT**) |
| Rose primitif natif (crible) | 1,305 | **0,085** | 0,082 – 0,087 | 0,003 — **algorithme différent** |

Overhead de démarrage : Rose 1,61 ms ; Rust 1,09 ms ; Python 10,01 ms ; primitif Rose 1,22 ms.

## 3. Factorielle exacte N = 5000 (16326 chiffres)

Même produit `r = 1; pour k de 2 vers 5000: r *= k` dans Rose et Rust (bigint maison base 10⁹, aucune dépendance, car 5000! ne tient pas dans `i128`) ; Python utilise `math.factorial` comme demandé.

| Implémentation | wall médian (ms) | interne médian (ms) | min–max interne | rapport interne / Rose JIT |
|---|---:|---:|---:|---:|
| **Rose JIT (boucle Rose)** | 4,844 | **1,498** | 1,489 – 1,555 | 1,00 |
| Rust pur (bigint maison) | 8,777 | **7,811** | 7,774 – 7,849 | **5,22** (Rose JIT 5,22× plus rapide que Rust pur naïf) |
| Python (`math.factorial`, C) | 14,899 | **0,383** | 0,379 – 0,390 | 0,26 (Python 3,91× plus rapide — **implémentation C, algorithme différent**) |
| Rose primitif natif (arbre de produit) | 2,718 | **0,338** | 0,327 – 0,356 | 0,23 — **algorithme différent** |

Overhead de démarrage : Rose 3,35 ms ; Rust 0,97 ms ; Python 14,52 ms ; primitif Rose 2,38 ms.

## 4. Expérience « 40 % délocalisé en jardins »

Deux tâches sur **les mêmes 49998 appels** à `est_premier` :
- **A (60 %)** : `k ∈ [2, 30000)` — reste en Rose direct ;
- **B (40 %)** : `k ∈ [30000, 50000)` — déplacée dans `garden_b.rose`, chargée par `importer`.

Les deux versions produisent le même résultat (5133). `est_premier` reste défini dans le programme principal et est visible depuis le jardin (fusion textuelle au parsing, vérifiée expérimentalement) : **aucun code dupliqué**, seul le calcul B est délocalisé.

Protocole renforcé : **25 échantillons entrelacés**, pinés sur le cœur 15 (12:04:43Z).

| Version | wall médian (ms) | min–max wall | interne médian (ms) | min–max interne |
|---|---:|---:|---:|---:|
| Tout en Rose direct | 4,317 | 3,235 – 4,643 | **1,792** | 1,781 – 1,835 |
| 40 % en jardin importé | 4,317 | 3,132 – 4,722 | **1,790** | 1,781 – 1,837 |
| **Rapport jardin / direct** | **1,000** | — | **0,999** | — |

Coût pur de l'`importer`, isolé sur un programme trivial (15 échantillons) : `0,778 ms` sans import contre `0,828 ms` avec import, soit **+0,05 ms** de parsing, noyé dans le bruit du programme complet (la médiane « 3,647 ms vs 4,228 ms » vue sur 9 échantillons était un artefact de bruit : elle disparaît à 25 échantillons).

**Réponse : délocaliser 40 % en jardin ne change rien, ni en bien ni en mal** (`×1,000` sur les deux horloges). Explication mécanique : `importer` est une **fusion textuelle au moment du parsing** ; les fonctions du jardin entrent dans le même programme et sont compilées par le **même JIT Cranelift** que les fonctions directes. Il n'y a **ni dispatch à l'exécution, ni couche d'ABI, ni pénalité d'appel** — seulement ~0,05 ms de lecture/parsing du fichier supplémentaire au démarrage. Le jardin est un **outil d'organisation du code**, pas un levier de performance (ni un frein).

## Interprétation — où Rose gagne, où Rust/Python gagnent, et pourquoi

1. **Rust pur gagne nettement sur le calcul entier machine** : 16,5× sur la récursion fib et 26,9× sur le comptage de premiers. Le JIT Cranelift compile bien le sous-ensemble entier/ booléen, mais le code généré et l'ABI d'appel restent plus coûteux que `rustc -O` sur des boucles et récursions serrées. Ce n'est pas un défaut de compilation à la volée : c'est l'écart normal entre un JIT et un compilateur AOT optimisé sur du code machine simple.
2. **Rose JIT gagne contre le Rust pur sur les bigints naïfs** (5,22× sur 5000!). Raison : le moteur Rose porte une arithmétique bigint native (`num-bigint`, zéro GMP) avec des chemins multipliés optimisés, alors que le « Rust pur » de référence n'a aucune bibliothèque bigint dans `rustc` seul — le bigint maison base 10⁹ est volontairement naïf. Ce n'est donc **pas** une victoire du langage sur Rust, mais une victoire de la **bibliothèque numérique embarquée** sur une implémentation de fortune.
3. **Python gagne là où sa bibliothèque C est spécialisée** : `math.factorial` est 3,9× plus rapide que la boucle Rose, et le comptage de premiers en Python est 1,53× plus rapide que le JIT Rose. En revanche Python est **2,84× plus lent** que Rose sur la récursion fib (appels de fonction Python interprétés). Il ne faut donc pas dire « Python est lent » ni « Rose bat Python » : cela dépend de si le travail est dans l'interpréteur ou dans une primitive C.
4. **Le démarrage domine les petits calculs, et il est distinct du calcul**. Sur 5000!, le calcul Rose ne coûte que 1,50 ms mais le **processus complet** coûte 4,84 ms : ~69 % du temps est du démarrage (parsing + compilation JIT). Python paie 10–15 ms de démarrage : sur `math.factorial(5000)` le calcul est 0,38 ms mais le processus fait 14,9 ms, soit **~39× le temps de calcul**. C'est l'illustration directe de la règle AGENTS.md : **vitesse du moteur ≠ vitesse d'exécution**. Un bench qui ne rapporte que le wall time conclurait à tort que « Python est 39× plus lent que le calcul ».
5. **Les primitives natives du moteur Rose sont dans une autre catégorie** : 10 µs sur fib (doublement rapide), 85 µs sur les premiers (crible), 338 µs sur 5000! (arbre de produit). Elles battent tout le reste, **mais au prix d'un algorithme différent** — elles ne prouvent pas que Rose exécute *le même code* plus vite. C'est la distinction entre « moteur optimisé » et « code Rose optimisé ».
6. **Bruit de mesure** : sans pinage, les mêmes programmes donnent des distributions bimodales dues aux 10 cœurs X925 et 10 cœurs A725 de la GB10. Toute conclusion tirée d'une salve non pinée (notamment un écart de 10–15 % sur le jardin) est invalide ; c'est pourquoi les tableaux ci-dessus sont pinés.

## Verdict honnête (une phrase par cas)

- **fib(35) récursif** : **Rust gagne** (16,5×), Python perd (2,84× plus lent que Rose), le natif Rose gagne écrasamment mais avec un autre algorithme.
- **premiers < 50000 par division** : **Rust gagne** (26,9×), **Python gagne aussi** (1,53×) face au JIT Rose, le crible natif est hors catégorie.
- **factorielle 5000** : **Rose JIT gagne face au Rust pur naïf** (5,2×), **Python/C gagne** (3,9×), le primitif Rose gagne (4,4×).
- **40 % en jardins** : **aucun effet** (`×1,000`) — organisation du code, pas performance.

## Reproduction

Tous les fichiers de test sont sous `/tmp/opencode/rosebench/` (aucun fichier du dépôt modifié sauf ce document).

```bash
cd /tmp/opencode/rosebench
~/.cargo/bin/rustc -O -o pure_rs pure.rs
PIN_CPU=15 python3 measure.py      # 4 groupes × 9 échantillons entrelacés
python3 garden_deep.py             # expérience jardin, 25 échantillons, cœur 15
```

Sorties brutes conservées : `measure_out.txt` (salve libre), `measure_out_pinned.txt` (salve pinée), `garden_deep_out.txt`, données `results.csv`, `results_pinned.csv`. Sources des programmes : `rose_fib.rose`, `rose_primes.rose`, `rose_fact.rose`, `rose_native_{fib,primes,fact}.rose`, `pure.rs`, `impl.py`, `mix_direct.rose`, `mix_garden.rose`, `garden_b.rose`.

## Contexte

Voir `BENCH.md` (mesures Rose internes), `CAPACITES.md`, `AUDIT_FINAL.md`. Toute mesure est un **constat daté** : re-mesurer après tout changement de binaire, de quantification ou de topologie.
