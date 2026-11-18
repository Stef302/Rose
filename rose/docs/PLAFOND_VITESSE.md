# Plafond de vitesse réel du moteur Rose — mesures & plafond théorique

Horodatage de la salve de référence : **2026-11-18T13:28:57+01:00**, binaire `rose/rose` SHA256 **`8a4ac955dced44dab318ca6eebbe309af4f3e8f0480b5bb3c2ae76fae48d6eab`** (hash vérifié identique avant et après la salve). Salves exploratoires antérieures : 13:20:20 et 13:21:58. Machine : **aarch64**, 20 cœurs hétérogènes (X925 = cpu 5–9 et 15–19 à 3,9 GHz ; A725 = cpu 0–4 et 10–14 à 2,8 GHz), `governor=performance`. Toutes les mesures sont **pinées sur le cœur X925 15** via `taskset -c 15`, sauf la variante explicitement « 20 cœurs ».

Règle AGENTS.md appliquée : **mesurer, jamais citer une spec** ; 9 échantillons par mesure, A/A/A entrelacé, médianes, rc et valeur vérifiés à chaque échantillon ; démarrage et exécution chronométrés **séparément**.

⚠ **Constat de concurrence, important pour la reproductibilité** : un autre agent a modifié `rose/src/{jit.rs,lib.rs,mul_rapide.rs}` et reconstruit `rose/rose` **deux fois pendant cette mission** (13:24:12 → `d05a3062…`, 13:27:37 → `8a4ac955…`, après le binaire `72d8cc…` des benchs de 12:31), soit trois binaires distincts observés, notamment en ajoutant un chemin rapide i64 pour la **soustraction** dans Cranelift. Les chiffres de ce document sont ceux de la **dernière salve propre**, post-rebuild, avec hash contrôlé avant/après. Un constat ancien (ex. « fib(35) = 208 ms » de `BENCH.md`) n'est donc plus valable : il est **divisé par ~4** sur le binaire actuel. Ce document ne modifie aucun autre fichier du dépôt et ne fait aucun commit.

## 1. Démarrage vs exécution

Deux horloges distinctes, jamais confondues :

| Instrument | Ce qu'il mesure | Médiane |
|---|---|---:|
| `python3 perf_counter_ns` autour de `subprocess.run` | processus complet (fork/exec + parsing + calcul + affichage, **+ ~0,8 ms de surcoût du harnais Python**) | **1,057 ms** (`vide.rose`), **1,062 ms** (`petit.rose`) |
| `rose debug-lenteur -n 9` (Rust `Command`) | lancement du processus | 229 µs (`rose version`), 348 µs (programme minimal), 435 µs (`petit.rose`) |
| `ROSE_TIMING=1` | `pret` après entrée dans `main` | 7–14 µs |
| `ROSE_TIMING=1` | `parse` d'un programme de 186 octets | 30–54 µs |

Commandes :

```bash
for i in $(seq 1 9); do taskset -c 15 /usr/bin/time -f %e ./rose/rose /tmp/opencode/plafond/vide.rose; done
# -> 0.00 × 9 : %e n'a qu'une résolution de 10 ms, INUTILE pour un démarrage < 1 ms
python3 /tmp/opencode/plafond/measure_plafond.py
taskset -c 15 ./rose/rose debug-lenteur -n 9 /tmp/opencode/plafond/petit.rose
```

**Constat** : le démarrage réel du moteur est de l'ordre de **0,2 ms** (lancement + entrée dans `main` + parsing). Le 1,057 ms du harnais Python inclut le surcoût de `subprocess`. L'écart entre programme vide et `>> 2+3` est de **0,005 ms** sur l'horloge Python et de **~118–205 µs** sur l'horloge `debug-lenteur` : sur un calcul trivial, **le démarrage est l'essentiel du temps total**. Le `%e` de `/usr/bin/time` rend `0.00` partout et ne prouve rien : c'est un piège de résolution, pas un démarrage nul.

## 2. Plafond de l'émetteur machine natif (M3.1) vs JIT vs interpréteur

`boucle.rose` : fonction `somme(n)` avec boucle `s = s + k` sur 1 000 000 d'itérations, chronométrée par `temps()` **à l'intérieur** du programme (affichage exclu). Même entrée, même cœur, même sortie vérifiée (`499999500000`) aux 27 échantillons (9 × 3).

| Chemin | Commande | Interne médian | Wall médian | n |
|---|---|---:|---:|---:|
| **Émetteur natif M3.1** | `ROSE_SANS_JIT=1 ROSE_NATIF=1` | **3,459 ms** | 4,735 ms | 9 |
| JIT Cranelift (défaut) | aucun env | 13,848 ms | 15,130 ms | 9 |
| Interpréteur RIR | `ROSE_SANS_JIT=1` | 14,510 ms | 15,591 ms | 9 |

Min–max internes : natif 3,454–3,481 ms ; JIT 13,842–14,056 ms ; interpréteur 14,473–14,639 ms (dispersion < 1,5 % partout).

**Le chemin natif M3.1 est le plus rapide : 4,00× devant le JIT Cranelift et 4,19× devant l'interpréteur.** Le JIT, lui, n'est que **1,048×** plus rapide que l'interpréteur sur cette boucle d'additions.

Vérification du câblage (`ROSE_TIMING=1`) :

- JIT : `TIMING-JIT eligibles=["somme"]`, `emit=20,1 µs`, `finalize=221 µs` → Cranelift compile bien `somme` ;
- interpréteur : aucune ligne `TIMING-JIT` → fonction interprétée ;
- natif : aucune ligne `TIMING-JIT`, mais `exec` passe de 14,5 ms à 3,5 ms → c'est bien l'émetteur M3.1 installé par `activer_emetteur_natif()` ;
- `ROSE_NATIF=1` **seul** (sans `ROSE_SANS_JIT`) laisse le JIT prioritaire (13,9 ms) : l'émetteur ne remplace jamais une fonction déjà compilée par Cranelift.

### Plancher machine de cette boucle (MESURÉ)

Même calcul en C, piné sur le même cœur 15 :

| Version | Médiane (9 échantillons) |
|---|---:|
| `gcc -O2` (`n` lu de `argv` pour empêcher le repli en `n(n-1)/2`) | **0,256463 ms** |
| `gcc -O0` | 1,824 ms |

Plancher théorique **calculé** : une dépendance `s += k` (addition i64 dépendante, latence 1 cycle) par itération → 10⁶ cycles / 3,9 GHz = **0,2564 ms**. Le C `-O2` mesure 0,256463 ms, soit **1,00× le plancher** : c'est le plancher machine réel, pas une estimation.

| Chemin | Rapport au plancher (0,2564 ms) |
|---|---:|
| C `-O2` | 1,0× |
| **Rose natif M3.1** | **13,5×** |
| Rose JIT Cranelift | 54,0× |
| Rose interprété | 56,6× |

⚠ Deux pièges de benchmark rencontrés et corrigés : le C `-O2` donnait d'abord `0,000032 ms` (compilateur avait remplacé la boucle par la formule fermée) — `n` vient maintenant de `argv` et la sortie est vérifiée ; le premier `memcpy` de la mesure de bande passante avait été éliminé (167 773 Go/s farfelu) — un checksum `volatile` force maintenant les accès.

### Contre-exemple : la récursion

`fib(35)` (résultat vérifié `9227465`), 9 échantillons JIT sur le binaire final :

| Chemin | Médiane interne |
|---|---:|
| **JIT Cranelift (binaire 13:27)** | **51,30 ms** |
| JIT Cranelift (`BENCH.md`, binaire 12:31) | 210,25 ms (constat daté, périmé) |
| Émetteur natif M3.1 (5 échantillons, binaire 13:24) | 12 027 ms |
| Rust `-O` (`BENCH_COMPARATIF.md`) | 12,63 ms (constat daté) |

La soustraction rapide ajoutée par l'agent concurrent divise le JIT par **~4,1**. Mais l'émetteur natif, vainqueur sur la boucle, est ici **234× plus lent que le JIT** : son `AppelRir` délègue chaque appel récursif à l'interpréteur via trampoline. **Le natif M3.1 n'est pas un remplacement universel du JIT** : il gagne sur le code entier droit, perd sur la récursion.

## 3. Grand entier : `factorielle(1000000)`

`fact1m_natif.rose` : `$d=temps(); $r=factorielle(1000000); $f=temps(); >> longueur(chaine($r)); >> $f-$d`. La conversion décimale est **après** `$f` : elle n'entre pas dans le temps interne, mais entre dans le temps processus. Sortie vérifiée : **5 565 709 chiffres** aux 18 échantillons.

| Mode | Cœurs | Interne médian | Wall médian | Max RSS (`/usr/bin/time -v`) |
|---|---|---:|---:|---:|
| **Non piné** | 20 | **196,15 ms** | 577,87 ms | 226 568 – 233 504 Kio (~222 Mo) |
| Piné `taskset -c 15` | 1 | 1 026,76 ms | 3 198,94 ms | 73 984 Kio (~72 Mo) |

Échantillons internes (ms) — non pinés : 196,6 / 198,7 / 194,3 / 194,6 / 183,0 / 209,4 / 201,5 / 209,7 / 209,6 (salve 13:21) ; 196,2 (salve finale 13:28). Pinés : 1 016–1 023 ms (salve 13:21), 1 026,8 ms (salve finale).

**Comparaison au chiffre historique** : `CAPACITES.md:48,97` et `README.md:76,468` annoncent « 5 565 709 chiffres en 200 ms » (constat du 18/11). **Confirmé** : 196,15 ms médian, à condition de **ne pas pinner** — la primitive est volontairement parallèle (`i128x::factorielle_parallele`, LPT + arbre Huffman + NTT). Le pinage impose un facteur **5,24×** de ralentissement, loin des 20× d'un parallélisme parfait : Amdahl + contention mémoire sur la dernière fusion.

Le temps **processus** (577,87 ms) est ~2,9× le temps de calcul (196,15 ms) : ~**382 ms** de conversion décimale de 5,5 M chiffres. C'est cohérent avec le « total 565 ms » de `README.md:468`. Ce n'est pas du calcul : c'est un convertisseur.

Référence hors catégorie : la boucle Rose naïve `$r = $r * $k` de 2 à 10⁶, 3 échantillons pinés : internes **90,84 / 90,95 / 106,90 s** (médiane **90,95 s**), wall 93,00 / 93,12 / 109,07 s, RSS 73 684 Kio. Dispersion due aux tâches concurrentes de cette machine. Soit ~**464× plus lente** que la primitive (90,95 / 0,19615). Les deux chemins ne font pas le même travail : la primitive utilise Légendre + arbre Huffman + NTT.

## 4. Plafond théorique — calcul explicite

Légende : **[M]** = mesuré cette session ; **[E]** = estimé/calculé, formule donnée ; **[H]** = chiffre historique daté dans la source, non re-mesuré ici.

### 4.1 Bande passante [M]

| Mesure | Résultat |
|---|---:|
| Copie séquentielle 512 Mio, 1 cœur piné (lecture+écriture) | **28,86 Go/s** |
| Lecture séquentielle 512 Mio, 1 cœur piné | **17,73 Go/s** |
| Copie 20 threads, agrégé | **129,7 Go/s** |

`/proc/buddyinfo` (constat du 18/11) : plus gros bloc Normal = 2¹¹ pages = 8 Mio, aucun bloc 2¹²/2¹³. Aucune valeur de spec n'est citée : ce sont les trois nombres ci-dessus qui servent de référence.

### 4.2 Taille du résultat [E]

`1 000 000!` = 5 565 709 chiffres **[M]**.
bits = 5 565 709 × log₂(10) = **18 488 885 bits** = 2,31 Mo = **2,20 MiB**.

### 4.3 Plancher I/O strict d'une multiplication BigInt [E]

Dernière fusion de l'arbre (opérandes **[H]** `i128x.rs:183`, 18/11) : 7 727 004 et 10 761 882 bits = 0,92 + 1,28 MiB ; résultat 2,20 MiB.

> Trafic minimal = lire A + lire B + écrire C = **4,41 MiB**
> à 129,7 Go/s (20 cœurs) → **0,0356 ms** ; à 28,86 Go/s (1 cœur) → **0,160 ms**

**[H]** la source annonce 350 ms pour cette dernière fusion → **~9 800× au-dessus du plancher I/O**. Conclusion : **la bande passante brute n'est pas le facteur limitant d'une multiplication isolée** ; c'est le travail arithmétique et le nombre de passes de la NTT.

### 4.4 Plancher trafic NTT de la dernière fusion [E]

- limbs u32 = (7 727 004 + 10 761 882) / 32 = **577 778** → NTT de taille **N = 2²⁰ = 1 048 576** ;
- coefficient u64 (module 62 bits, 2 modules) = 8 octets → **8,0 MiB par tableau** ;
- 20 étages × (lecture 8 MiB + écriture 8 MiB) = **320 MiB par transformée** ;
- 2 modules × (2 transformées directes + 1 inverse) = **6 transformées** → **1,88 GiB** de trafic.

> 1,88 GiB / 129,7 Go/s (20 cœurs) = **15,5 ms**
> 1,88 GiB / 28,86 Go/s (1 cœur) = **69,8 ms**

**[H]** 350 ms / 15,5 ms = **22,5×** : même en comptant toutes les passes NTT, l'écart reste d'un ordre de grandeur. Il vient des multiplies modulaires Montgomery (u128), des passes de CRT/retenue, et du fait que la dernière fusion est **sérielle du point de vue de l'arbre** (elle utilise les 20 cœurs dans la NTT, mais il n'y a qu'une fusion à ce niveau).

### 4.5 Plancher trafic NTT de TOUTE la factorielle [E]

L'arbre Huffman conserve la somme des bits du produit à chaque niveau : ~577 778 limbs restent à traiter par niveau, sur ~**17 niveaux** (log₂ de ~78 500 facteurs premiers). En bornant le nombre moyen d'étages de transformée entre 10 et 20 selon le niveau :

> trafic estimé **8,3 – 11,3 Go** → **64 – 87 ms** à 129,7 Go/s (20 cœurs), **286 – 392 ms** à 28,86 Go/s (1 cœur).

**[M]** mesuré : **196,15 ms** (20 cœurs), **1 026,76 ms** (1 cœur).

**[M]/[E]** : 196,15 / 87 = **2,25×** sur 20 cœurs ; 1 026,76 / 392 = **2,62×** sur 1 cœur. Les deux rapports sont proches : **le calcul de 1 000 000! se tient à ~2,5× du plancher de trafic mémoire de son propre algorithme NTT**. Ce n'est pas « proche du plancher machine absolu » (le plancher I/O strict est à 0,036 ms, ~5 500× en dessous), mais c'est le bon ordre de grandeur pour un NTT qui fait ~6 passages complets par niveau.

### 4.6 Conversion décimale [M] + [E]

Wall − interne = **382 ms** (20 cœurs), **2 172 ms** (1 cœur). Le résultat ne fait que 2,20 MiB : un passage lecture+écriture à 129,7 Go/s coûterait **0,036 ms** **[E]**. La conversion fait donc l'équivalent d'~**10 700 passages** de données **[E]** — ce n'est pas un coût mémoire, c'est le coût des divisions répétées par blocs de chiffres. C'est la vraie cible d'optimisation du temps **total** perçu.

## 5. Verdict honnête

| Cas | Distance au plancher | Verdict |
|---|---|---|
| `2 + 3`, programme vide | démarrage ~0,2 ms domine | **Rose est au plancher** : le calcul vaut ~0,02 ms. Optimiser le calcul ici n'aurait aucun effet perceptible. |
| Boucle entière 1M, **émetteur natif M3.1** | 13,5× le plancher (0,2564 ms) | **Loin du plancher machine**, mais **4,0× mieux que le JIT**. L'émetteur travaille sur slots mémoire avec vérifications : il n'atteint pas le C `-O2` registre-à-registre. C'est le meilleur chemin Rose mesuré sur du code entier droit. |
| Boucle entière 1M, **JIT Cranelift** | 54,0× le plancher | **Très loin.** Le JIT n'est que 1,048× plus rapide que l'interpréteur : son ABI/cadre de pile et ses appels runtime coûtent plus que le calcul. |
| `fib(35)` récursif, JIT | 51,30 ms ; Rust `-O` 12,63 ms | **Loin mais en net progrès** : l'écart est passé de 16,5× (BENCH.md, binaire 12:31) à **~4,1×** grâce au chemin rapide de soustraction ajouté à 13:27. Le primitif natif `fibonacci()` (0,010 ms interne) bat tout, mais avec **un autre algorithme** (doublement rapide). |
| `fib(35)`, émetteur natif M3.1 | 12 027 ms = **234× le JIT** | **Contre-exemple décisif** : `AppelRir` délègue la récursion à l'interpréteur. Le natif n'est pas un remplacement du JIT, seulement un accélérateur de corps droits. |
| `factorielle(1e6)`, primitive native | 196,15 ms vs 64–87 ms estimés (trafic NTT) | **Même ordre de grandeur** (~2,3×) : proche du plancher de son propre algorithme. Le parallélisme 20 cœurs gagne 5,24× ; le goulot est la dernière fusion et la bande passante agrégée, pas le crible. |
| `factorielle(1e6)`, total perçu | 577,87 ms wall | **Loin** : ~382 ms (66 %) sont de la conversion décimale, pas du calcul. Le calcul est au plancher de son algo, mais l'utilisateur attend ~3× plus longtemps. |
| `factorielle(1e6)`, boucle Rose naïve | 90,95 s (médiane) | **Très loin** (~464× la primitive) : l'absence d'algorithme spécialisé (Légendre + Huffman + NTT) domine tout. |

**Réponse à « Rose va plus vite en prenant compte l'ensemble de sa structure »** : oui, **à condition d'utiliser la structure** — la primitive parallèle et l'émetteur natif M3.1 sont respectivement 464× et 4× plus rapides que les chemins naïfs sur leurs cas favorables, et le JIT vient de gagner 4,1× sur `fib(35)`. Non, si l'on juge le seul JIT sur une boucle entière serrée : il reste à 54× du plancher machine et à 5 % devant l'interpréteur. Le goulot n'est pas dans l'algorithme bigint (la primitive est à 2,3× de son plancher NTT) ; il est dans **(a)** l'ABI du JIT sur petits types machine, **(b)** le démarrage (~0,2 ms, inévitable sur petits programmes), **(c)** la conversion décimale (~382 ms sur 1 000 000!), **(d)** le fait que l'émetteur natif n'est activé qu'explicitement (`ROSE_NATIF=1`) et jamais à la place d'une fonction déjà compilée par Cranelift, et **(e)** le fait qu'il régresse sur la récursion.

## 6. Reproduction

Tous les fichiers de test et sorties brutes sont sous `/tmp/opencode/plafond/` (cette mission n'a modifié aucun fichier du dépôt sauf ce document ; les modifications concurrentes signalées en tête ne sont pas les siennes) :

```bash
cd /tmp/opencode/plafond
python3 remeasure.py          # salve finale : hash avant/après + startup + 3 chemins + fact
python3 measure_plafond.py    # démarrage + 3 chemins, 9 échantillons entrelacés
python3 measure_fact.py       # factorielle 1e6, piné vs 20 cœurs
python3 calcul_plafond.py     # tous les calculs de plafond, entrées [M] explicites
gcc -O2 -o loopbench_o2 loopbench.c && taskset -c 15 ./loopbench_o2 1000000
gcc -O0 -o loopbench_o0 loopbench.c && taskset -c 15 ./loopbench_o0 1000000
gcc -O2 -o membench membench.c && taskset -c 15 ./membench
gcc -O2 -pthread -o membench_mt membench_mt.c && ./membench_mt 20 20
taskset -c 15 /usr/bin/time -v /home/clausake/Stefan/ROSE/rose/rose fact1m_natif.rose
```

Données brutes : `results_final.json` (salve de référence), `results_plafond.json`, `results_fact.json`, `fact_loop_3.txt`.

Toute mesure est un **constat daté** : re-mesurer après tout changement de binaire — ce document en est la preuve, le binaire ayant été reconstruit deux fois pendant les salves.
