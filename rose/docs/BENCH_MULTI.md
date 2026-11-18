# Benchmark multi-domaines étendu — Rose / Rust / Python / C

**Horodatage de la salve de décision** : **2026-11-18T12:48:14Z → 12:48:27Z** (UTC). Une salve identique de contrôle a été jouée à 12:47:11Z → 12:47:24Z : les conclusions sont les mêmes, les écarts restent dans la dispersion annoncée.

**Binaire** : `/home/clausake/Stefan/ROSE/rose/rose`, SHA256 `8a4ac955dced44dab318ca6eebbe309af4f3e8f0480b5bb3c2ae76fae48d6eab`, vérifié **identique avant et après** la salve. Rose 0.1.5. `rustc 1.99.1` (`~/.cargo/bin/rustc -O`), Python 3.12.3, gcc 13.3.0 (`-O2`).

**Machine** : aarch64 GB10, 20 cœurs hétérogènes (X925 3,9 GHz = cœurs 5–9 et 15–19 ; A725 2,8 GHz), `governor=performance`. Toutes les exécutions sont **pinées sur le cœur X925 19** via `taskset -c 19` (le cœur 6 était occupé à 100 % par un autre agent au moment de la salve ; le cœur 19 était à ~4 %).

**Périmètre** : ce document est le **seul** fichier du dépôt modifié par cette mission. Aucun commit. Sources, binaires, sorties brutes et harnais sous `/tmp/opencode/benchmulti/`.

---

## Protocole

- **9 échantillons par implémentation**, entrelacés **par rotation de l'ordre** (A/B/C/D… puis B/C/D/A…), dans une seule salve, sur le même cœur. Médianes rapportées, avec min–max interne.
- **Deux horloges distinctes, jamais confondues** :
  1. `wall` : `time.perf_counter_ns()` autour de `subprocess.run` — démarrage + parsing + compilation + calcul + affichage (+ ~0,8 ms de surcoût du harnais Python) ;
  2. `interne` : `temps()` Rose (ns), `Instant` Rust, `perf_counter_ns()` Python, `clock_gettime(CLOCK_MONOTONIC)` C — **le calcul seul**, affichage exclu.
- **Résultat vérifié à chaque échantillon** (rc = 0 et valeur attendue) : `9227465`, `499999500000`, `5133`, `16326`, `500`, `20000`, `4999950000`, `3000`, `10485760`. **Aucun échantillon rejeté.**
- **Mêmes entrées, même cœur, même valeur de sortie.** Les conversions décimales (`longueur(chaine(...))`, `len(str(...))`, `to_string().len()`) sont **hors zone chronométrée** dans les quatre langages pour les domaines 4 et 4b.
- **Anti-optimisation** : en Rust, `n` de la boucle vient de `argv` et `std::hint::black_box(k)` est appliqué dans le corps (un `for k in 0..n { s += k }` seul est remplacé par la formule fermée en 64 ns, mesuré) ; en C, `n` est `volatile`. Le plancher mesuré est ~0,262 ms, conforme au `0,256463 ms` de `PLAFOND_VITESSE.md` : la boucle est bien exécutée.
- **Dispersion** : < 2 % sur la plupart des cellules. Deux exceptions honnêtes : l'E/S (domaine 8, 14–60 % de dispersion : bruit de page-cache — mais les plages Rose 13,96–17,37 ms et Rust 5,66–6,44 ms **ne se recouvrent pas**, donc le verdict tient) et les temps internes de ~10 µs (primitives, où le bruit relatif est élevé mais l'écart avec Rust est de 3 ordres de grandeur).

---

## Tableau résumé — médianes du calcul seul (`interne`)

« Rose » = le code Rose écrit dans le langage (JIT pour les domaines numériques, **interpréteur** pour chaînes/listes/tables/E/S — voir § Domaines non applicables). « Meilleur Rose » = primitive ou backend natif disponible dans le moteur.

| # | Domaine | Rose | Rust `-O` | Python | C `-O2` | Rose/Rust | Meilleur Rose | Verdict (source contre source) |
|---|---|---:|---:|---:|---:|---:|---:|---|
| 1 | `fib(35)` récursif | 51,329 ms | 12,708 ms | 590,114 ms | 9,157 ms | **4,04 ×** | 0,010 ms (natif) | **Perdu** |
| 2 | Boucle 1..10⁶ | 13,850 ms | 0,262 ms | 20,724 ms | 0,264 ms | **52,85 ×** | 3,459 ms (M3.1) | **Perdu** |
| 3 | Premiers < 50 000 | 26,988 ms | 1,000 ms | 17,541 ms | 0,967 ms | **26,98 ×** | 0,090 ms (crible) | **Perdu** |
| 4 | Factorielle 5 000 | **1,471 ms** | 6,561 ms | 0,378 ms | 8,466 ms | **0,22 ×** | 0,342 ms (arbre) | **Gagné (4,46 ×)** |
| 4b | PGCD 1 000 chiffres | 7,511 ms (Euclid) | 0,148 ms | 0,011 ms | 6,491 ms | **50,76 ×** | **0,050 ms** (pgcd Stein) | **Perdu** ; gagné avec la primitive |
| 5 | Chaîne 20 000 car. | 14,326 ms | 0,014 ms | 0,415 ms | 2,005 ms | **1007 ×** | — | **Perdu** |
| 6 | Liste 100 000 entiers | 238,335 ms | 0,202 ms | 2,115 ms | 0,201 ms | **1181 ×** | — | **Perdu** |
| 7 | Table 3 000 clés | 188,139 ms | 0,097 ms | 0,271 ms | 1,209 ms | **1946 ×** | — | **Perdu** |
| 8 | E/S 10 Mo | 14,150 ms | 5,946 ms | 5,781 ms | 5,932 ms | **2,38 ×** | — | **Perdu** |

Le ratio est `temps Rose / temps Rust` : **< 1 = Rose gagne**, > 1 = Rust gagne.

---

## Détail par domaine

### 1. Récursion — fib(35) = 9 227 465

| Implémentation | wall médian | interne médian | min–max interne | Ratio / Rose |
|---|---:|---:|---:|---:|
| Rose JIT (source) | 52,879 ms | 51,329 ms | 51,313 – 51,363 ms | 1,000 |
| Rust -O | 13,896 ms | 12,708 ms | 12,659 – 12,774 ms | 0,248 × |
| Python (récursif) | 600,938 ms | 590,114 ms | 589,622 – 590,472 ms | 11,497 × |
| C -O2 | 10,169 ms | 9,157 ms | 9,143 – 9,191 ms | 0,178 × |
| Rose primitif `fibonacci` | 1,261 ms | 0,010 ms | 0,007 – 0,011 ms | 0,000 × |

**Perdu** (Rust 4,04 × plus rapide ; C 5,61 ×). Le gain historique 208 → 51 ms du chemin rapide de soustraction est confirmé sur le binaire actuel. Le primitif `fibonacci()` fait du doublement rapide O(log n) : **algorithme différent**, hors catégorie. Rust `-O` réécrit en boucle (~2 × moins d'appels, voir `VITESSE_VS_RUST.md §2`) : même source, pas exactement le même machine-code.

### 2. Boucle entière — somme 0..999999 = 499 999 500 000

| Implémentation | wall médian | interne médian | min–max interne | Ratio / Rose |
|---|---:|---:|---:|---:|
| Rose JIT (source) | 15,126 ms | 13,850 ms | 13,793 – 13,878 ms | 1,000 |
| Rust -O | 1,078 ms | 0,262 ms | 0,257 – 0,266 ms | 0,019 × |
| Python | 30,521 ms | 20,724 ms | 20,617 – 20,749 ms | 1,496 × |
| C -O2 | 1,114 ms | 0,264 ms | 0,262 – 0,275 ms | 0,019 × |
| Rose natif M3.1 (`ROSE_NATIF=1 ROSE_SANS_JIT=1`) | 4,713 ms | 3,459 ms | 3,452 – 3,490 ms | 0,250 × |

**Perdu** (Rust/C 52,8 × ; le meilleur chemin Rose, l'émetteur M3.1, perd encore 13,2 ×). **Rose bat Python ici** (1,49 ×) : la boucle Python interprétée paie l'objet `int` à chaque itération, alors que le JIT Cranelift travaille sur des slots i64. L'émetteur M3.1 gagne 4,0 × sur le JIT, confirmant `PLAFOND_VITESSE.md §2`.

### 3. Primalité — premiers < 50 000, division jusqu'à √n = 5 133

| Implémentation | wall médian | interne médian | min–max interne | Ratio / Rose |
|---|---:|---:|---:|---:|
| Rose JIT (source) | 28,422 ms | 26,988 ms | 26,921 – 27,118 ms | 1,000 |
| Rust -O | 2,183 ms | 1,000 ms | 0,994 – 1,003 ms | 0,037 × |
| Python (division) | 27,973 ms | 17,541 ms | 17,456 – 17,766 ms | 0,650 × |
| C -O2 | 1,844 ms | 0,967 ms | 0,964 – 0,971 ms | 0,036 × |
| Rose primitif `compter_premiers` (crible) | 1,036 ms | 0,090 ms | 0,085 – 0,094 ms | 0,003 × |

**Perdu** (Rust 27,0 × ; **Python gagne aussi**, 1,54 ×, le modulo Python étant un C rapide appelé depuis une boucle génératrice). Le crible natif est 11,1 × plus rapide que Rust — mais c'est un **algorithme différent** (crible d'Ératosthène au lieu de la division par les impairs).

### 4. Bigint — factorielle exacte 5 000 = 16 326 chiffres

| Implémentation | wall médian | interne médian | min–max interne | Ratio / Rose |
|---|---:|---:|---:|---:|
| Rose JIT (boucle `r *= k`) | 4,929 ms | 1,471 ms | 1,460 – 1,495 ms | 1,000 |
| Rust -O (bigint maison base 10⁹) | 7,414 ms | 6,561 ms | 6,519 – 6,584 ms | 4,459 × |
| Python (`math.factorial`, C) | 14,820 ms | 0,378 ms | 0,377 – 0,431 ms | 0,257 × |
| C -O2 (bigint maison base 10⁹) | 9,247 ms | 8,466 ms | 8,451 – 8,501 ms | 5,754 × |
| Rose primitif (arbre de produit) | 2,881 ms | 0,342 ms | 0,326 – 0,369 ms | 0,232 × |

**Gagné** : Rose JIT **4,46 × plus rapide que le Rust pur naïf**, même produit itératif, mêmes entrées. C'est la seule victoire du code Rose source contre Rust sur les 8 domaines. La cause n'est pas le langage mais la **bibliothèque bigint embarquée** (`num-bigint`) face à un bigint de fortune base 10⁹ : Rose bat aussi C (5,75 ×). Python/C gagnent avec `math.factorial` (implémentation C spécialisée, algorithme différent). Le primitif Rose (arbre de produit) bat tout le monde sur le calcul mais avec un autre algorithme.

### 4b. Grands nombres — PGCD de deux entiers de 1 000 chiffres = 500 chiffres

Entrées : deux entiers de **1 000 chiffres** engendrés avec `random.seed(3021)` puis figés dans `gcd_inputs.txt` ; PGCD attendu = **500 chiffres** (vérifié avec `math.gcd`). Rose `pgcd` et Rust/C maison implémentent **Stein** (vérifié dans `num-bigint-0.4.8/src/biguint.rs:241,247` : `// Stein's algorithm`) ; Python `math.gcd` utilise Euclid/Lehmer en C.

| Implémentation | wall médian | interne médian | min–max interne | Ratio / Rose Euclid |
|---|---:|---:|---:|---:|
| Rose (Euclid interprété `tant_que ... %`) | 8,607 ms | 7,511 ms | 7,489 – 7,523 ms | 1,000 |
| Rose primitif `pgcd` (Stein) | 1,052 ms | **0,050 ms** | 0,049 – 0,055 ms | 0,007 × |
| Rust -O (Stein maison, base 2³²) | 1,023 ms | 0,148 ms | 0,147 – 0,152 ms | 0,020 × |
| Python (`math.gcd`) | 9,673 ms | 0,011 ms | 0,010 – 0,011 ms | 0,001 × |
| C -O2 (Stein maison, base 10) | 7,362 ms | 6,491 ms | 6,461 – 6,509 ms | 0,864 × |

**Perdu** avec le code Rose écrit dans le langage (l'Euclid interprété est 50,8 × plus lent que Rust, et 683 × plus lent que Python). **Gagné avec la primitive** : `pgcd` (Stein sur `num-bigint`) est **2,96 × plus rapide que le Stein maison en Rust** — ici **le même algorithme**, donc une victoire de bibliothèque, pas d'algorithme. Le C est 44 × plus lent que le Rust uniquement parce que son bigint est en base 10 et le Rust en base 2³² (détail d'implémentation, pas une propriété du langage).

### 5. Chaînes — 20 000 concaténations `s = s + "x"` + `longueur(s)`

| Implémentation | wall médian | interne médian | min–max interne | Ratio / Rose |
|---|---:|---:|---:|---:|
| Rose (interpréteur, `+`) | 15,339 ms | 14,326 ms | 14,281 – 14,352 ms | 1,000 |
| Rust -O (`String::push_str`) | 1,121 ms | 0,014 ms | 0,014 – 0,017 ms | 0,001 × |
| Python (`+=`) | 10,782 ms | 0,415 ms | 0,413 – 0,424 ms | 0,029 × |
| C -O2 (`strcat`) | 2,854 ms | 2,005 ms | 1,991 – 2,294 ms | 0,140 × |

**Perdu** (Rust 1 007 ×, Python 34,5 ×, C 7,1 ×). Trois effets : (a) `+` sur chaîne recopie la chaîne à chaque itération en Rose **et** en C (`strcat`), d'où O(n²) — Rust `push_str` et Python `+=` (optimisé CPython quand le refcount vaut 1) sont amortis O(n) ; (b) chaque concaténation Rose passe par le dispatch interprété avec des `Valeur`/`Gz` boxés ; (c) `longueur()` Rose re-scanne les UTF-8 (O(n)) alors que `len()` Python est O(1). Le chemin efficace existe en Rose (`concatener(liste)`, O(n)) mais il change l'algorithme de construction ; il n'a pas été substitué ici pour ne pas tricher.

### 6. Listes — 100 000 entiers, `joindre` puis parcours indexé

| Implémentation | wall médian | interne médian | min–max interne | Ratio / Rose |
|---|---:|---:|---:|---:|
| Rose (`joindre` + `$l[$j]`) | 240,094 ms | 238,335 ms | 237,173 – 240,177 ms | 1,000 |
| Rust -O (`Vec<u64>`) | 1,469 ms | 0,202 ms | 0,183 – 0,221 ms | 0,001 × |
| Python (`list.append`) | 13,686 ms | 2,115 ms | 1,922 – 2,280 ms | 0,009 × |
| C -O2 (tableau `malloc`) | 1,153 ms | 0,201 ms | 0,190 – 0,219 ms | 0,001 × |

**Perdu** (Rust/C 1 181 ×, Python 113 ×). `joindre` est bien un `Vec::push` O(1), mais chaque élément Rose est une `Valeur` boxée contenant un bigint `Gz` avec Rc/RefCell : ~2,38 µs/élément contre ~2 ns en Rust/C. Le JIT Cranelift **ne compile pas ce domaine** (pas de type liste dans le sous-ensemble JIT) : tout tourne dans l'interpréteur.

### 7. Table — 3 000 insertions + 3 000 recherches

| Implémentation | wall médian | interne médian | min–max interne | Ratio / Rose |
|---|---:|---:|---:|---:|
| Rose (`table()`, liste d'association) | 189,360 ms | 188,139 ms | 187,975 – 188,202 ms | 1,000 |
| Rust -O (`HashMap`) | 1,031 ms | 0,097 ms | 0,094 – 0,106 ms | 0,001 × |
| Rust -O (`Vec<(u64,u64)>` linéaire) | 2,087 ms | 1,220 ms | 1,209 – 1,260 ms | 0,006 × |
| Python (`dict`) | 10,695 ms | 0,271 ms | 0,252 – 0,283 ms | 0,001 × |
| Python (`list` de tuples linéaire) | 93,374 ms | 83,048 ms | 82,823 – 83,425 ms | 0,441 × |
| C -O2 (liste d'association linéaire) | 2,472 ms | 1,209 ms | 1,205 – 1,261 ms | 0,006 × |

**Perdu** partout. Découverte importante : **`table()` / `map()` de Rose n'est pas une table de hachage** mais `Rc<RefCell<Vec<(Valeur,Valeur)>>>` avec `inserer`/`cle_existe` en **scan linéaire** (`run.rs:32`, `interp.rs:3488-3510`). Les 3 000 insertions + 3 000 recherches font ~9 millions de comparaisons de clés bigint. Deux effets séparés : la **structure** coûte ×12,6 (Rust Vec assoc 1,220 ms vs HashMap 0,097 ms) et le **surcoût interpréteur + clés bigint** coûte ×154 (Rose 188,1 ms vs Rust Vec assoc 1,220 ms). Le total ×1 946 contre HashMap n'est donc pas seulement « Rose est lent » : c'est « Rose n'a pas de hachage natif ».

### 8. E/S — lecture puis écriture d'un fichier de 10 Mo

Même fichier source `in_10m.txt` (10 485 760 octets ASCII), déjà en cache page ; chaque implémentation écrit dans son propre fichier de sortie.

| Implémentation | wall médian | interne médian | min–max interne | Ratio / Rose |
|---|---:|---:|---:|---:|
| Rose (`lire_texte` + `ecrire_texte`) | 17,470 ms | 14,150 ms | 13,960 – 17,373 ms | 1,000 |
| Rust -O (`fs::read`/`fs::write`) | 7,189 ms | 5,946 ms | 5,661 – 6,441 ms | 0,420 × |
| Python (`open`/`read`/`write`) | 17,490 ms | 5,781 ms | 5,501 – 6,130 ms | 0,409 × |
| C -O2 (`fread`/`fwrite`) | 7,463 ms | 5,932 ms | 5,792 – 8,590 ms | 0,419 × |

**Perdu** (Rust/C/Python ~2,4 × plus rapides sur le calcul). `lire_texte` fait `String::from_utf8_lossy` (validation UTF-8 intégrale + copie) avant l'écriture, ce que Rust/Python/C évitent sur des octets. Le **wall** raconte une histoire différente : Python (17,49 ms) paraît aussi lent que Rose (17,47 ms) alors que son calcul est 2,4 × plus rapide — les ~9,7 ms de démarrage Python masquent le gain. C'est l'illustration directe de la règle « vitesse du moteur ≠ vitesse d'exécution ».

---

## Domaines non applicables / précisions

Aucun des 8 domaines demandés n'est totalement « non applicable » : **Rose possède bien** les chaînes, listes, tables et primitives E/S (`lire_texte`, `ecrire_texte`, `ecrire_binaire`), vérifié dans `front.rs:649-674` et exécuté ici. Deux réserves doivent cependant être énoncées :

1. **JIT non applicable** aux domaines 5, 6, 7, 8 (et au Euclid bigint du 4b) : le JIT Cranelift ne couvre que les entiers/flottants/booléens. Ces mesures Rose sont donc **interprétées**, pas JIT. Les comparer à Rust `-O` mesure l'interpréteur, pas le JIT — c'est le bon objet à mesurer puisque c'est ce qui exécute réellement ces programmes.
2. **Vraie table de hachage non applicable en Rose** : `table()` est une liste d'association linéaire. Le domaine 7 a donc été mesuré avec ce qui existe, et complété par des variantes « liste d'association » en Rust/Python/C pour séparer le coût de structure du coût de langage. Aucune table de hachage Rose n'a été inventée.

---

## VERDICT

### Verdict strict — même code source, mêmes entrées, mêmes algorithmes

**Rose bat Rust dans 1 domaine sur 8 ; il perd dans 7** (le domaine 4b compte comme une défaite supplémentaire si on le dénombre séparément : 1 victoire / 8 défaites sur 9 cas).

- **Victoire unique** : factorielle 5 000 (1,471 ms contre 6,561 ms, **4,46 ×**), grâce à la bibliothèque bigint embarquée.
- **Défaites** : récursion (4,04 ×), boucle entière (52,8 ×), primalité (27,0 ×), PGCD écrit en Rose (50,8 ×), chaînes (1 007 ×), listes (1 181 ×), tables (1 946 ×), E/S (2,38 ×).

### Verdict « meilleur chemin Rose disponible » (primitives incluses)

**Rose bat Rust dans 4 domaines sur 9, perd dans 5** — mais **2 de ces 4 victoires changent d'algorithme** et ne prouvent pas que Rose exécute le même code plus vite :

| Domaine | Meilleur Rose | Rust | Rapport | Nature de la victoire |
|---|---:|---:|---:|---|
| fib(35) | 0,010 ms | 12,708 ms | 1 271 × | **Algorithme différent** (doublement rapide) |
| premiers 50 000 | 0,090 ms | 1,000 ms | 11,1 × | **Algorithme différent** (crible) |
| factorielle 5 000 | 0,342 ms | 6,561 ms | 19,2 × | Bibliothèque + arbre de produit |
| PGCD 1 000 ch. | 0,050 ms | 0,148 ms | 2,96 × | **Même algorithme Stein**, bibliothèque `num-bigint` |

Les 5 défaites restantes : boucle (M3.1 3,459 ms vs 0,262 ms, 13,2 ×), chaînes, listes, tables, E/S.

### Pourquoi — synthèse mécanique

1. **Sur le calcul entier machine, un JIT ne bat pas un AOT optimisé.** ABI, cadres de pile, helpers runtime et (sur la récursion) deux fois plus d'appels : 4 × sur fib, 27 × sur la primalité, 53 × sur la boucle. Ce n'est pas un bug, c'est l'écart normal JIT/AOT.
2. **Rose gagne uniquement là où la bibliothèque numérique embarquée fait le travail** : bigint (`num-bigint`, zéro GMP) sur factorielle et PGCD. Et encore, face à un bigint maison naïf : `math.factorial` de Python (C) bat la boucle Rose 3,9 ×, et `math.gcd` bat le PGCD écrit en Rose 683 ×.
3. **Les types non numériques sont interprétés, pas JIT**, avec des valeurs boxées (`Valeur`/`Gz`/Rc/RefCell) : ×1 000 à ×2 000 sur chaînes, listes et tables. C'est le mur principal du « je veux tout ».
4. **La structure de données compte autant que le langage** : pas de hachage natif (table = scan linéaire) et concaténation de chaînes O(n²) expliquent une part massive des écarts.
5. **Le démarrage domine les petits calculs et fausse les comparaisons au wall** : 1,5 ms Rose, 1,0 ms Rust, ~9,5 ms Python, ~0,7 ms C. Sur `pgcd`, le calcul Rose vaut 0,050 ms mais le processus 1,052 ms (95 % de démarrage) ; Python vaut 0,011 ms de calcul mais 9,673 ms de processus (99,9 % de démarrage). Juger au wall conclurait à tort que « Rose est 9 × plus rapide que Python sur le PGCD ».
6. **Les primitives natives Rose sont une autre catégorie** : 10 µs sur fib, 90 µs sur les premiers, 50 µs sur le PGCD. Elles battent tout, mais au prix d'un algorithme différent (sauf PGCD, où l'algorithme est le même). Ce sont des victoires du **moteur**, pas du **code Rose**.

**Réponse courte à « je veux tout, pas un domaine »** : Rose n'est pas universellement plus rapide. Il gagne sur les bigints face à un Rust sans bibliothèque, et sur les primitives embarquées ; il perd sur tout le calcul machine et sur tous les types non numériques dès qu'on compare au même algorithme. Toute affirmation du type « Rose bat Rust » doit préciser **quel domaine, quel chemin (source vs primitive), et quelle horloge**.

---

## Reproduction

```bash
cd /tmp/opencode/benchmulti
# binaires
/home/clausake/.cargo/bin/rustc -O -o multi_rs multi.rs
gcc -O2 -o multi_c multi.c
# salve complète : 9 échantillons entrelacés, pinés
PIN_CPU=19 python3 bench.py > bench_out.txt 2>&1
```

Sources : `r_*.rose` (12 fichiers), `multi.rs`, `multi.py`, `multi.c`. Données : `gcd_inputs.txt`, `in_10m.txt`. Sorties : `bench_out.txt`, `multi_results.csv` (9 domaines × 4–6 implémentations × 9 échantillons), `multi_summary.json`. Vérification binaire : `sha256sum /home/clausake/Stefan/ROSE/rose/rose`.

Toute mesure est un **constat daté** : re-mesurer après tout changement de binaire, de topologie ou d'état machine (le cœur 19 n'était pas le même que celui des documents précédents, et un autre agent occupait le cœur 6 pendant la salve).

**Constat de concurrence** : des modifications concurrentes de `rose/src/jit.rs` et un nouveau `rose/src/rosevm.rs` sont apparus à 12:49Z, **après** la fin de la salve (12:48:27Z). Elles ne sont pas de cette mission et n'ont pas été touchées. Le binaire `rose/rose` est resté à l'hash `8a4ac955…` (mtime 12:31:54Z) : les chiffres ci-dessus décrivent **ce binaire-là**. Si un rebuild intègre ces sources, re-mesurer — en particulier `fib(35)`, dont le chemin AST de la soustraction est précisément la zone modifiée.
