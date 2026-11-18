# Banc de comparaison des moteurs d'exécution Rose — constat daté

**Date : 19/11/2026, 00:30 CET. Dépôt `Stef302/Rose`, HEAD `477736f`.
Binaire mesuré : `rose/rose` (5 128 936 octets, ELF aarch64 static-pie).
Machine : 20 CPU, épinglage `taskset -c 7` pour TOUTES les mesures.**

> **Toute mesure est un constat daté, pas une consigne.** Les valeurs ci-dessous
> ont été produites par `bench/backends/run_backends.sh`, pas recopiées d'un
> README. Un autre agent utilisant un autre cœur, ou un changement de moteur,
> de quantification de compilation ou de charge système, invalide ces chiffres.

---

## 1. Fichiers livrés

| Fichier | Rôle |
|---|---|
| `bench/backends/bench_backends.rose` | Le banc, **écrit en Rose**. 5 charges, chronométrage interne `temps()`, 6 valeurs déterministes. Passe `./rose/rose verif`. |
| `bench/backends/bench_backends_core.rose` | Companion AOT : **corps de `charge_arith()` et `fib()` copiés à l'identique**, sans `temps()` ni chaîne/liste/table. Imprime 2 entiers. Passe `verif`. |
| `bench/backends/run_backends.sh` | Point d'entrée du harnais. |
| `bench/backends/run_backends.py` | Pilote : 1 échauffement non compté + **9 échantillons entrelacés par rotation**, pin `taskset -c 7`, médianes/min/max, vérification de la valeur à chaque échantillon. |

Aucun fichier `*.rs`, `Cargo.toml`, `PLAN.md` ni doc d'un autre agent n'a été
modifié. Aucun commit, aucune publication.

### Jeu de charges (constantes dans le `.rose`)

`N_ARITH=2 000 000`, `N_RECUR=30`, `N_CHAINE=8 000`, `N_LISTE=15 000`,
`N_TABLE=8 000`. `pour x vers n` étant exclusif en Rose, les boucles font
`n-1` itérations.

> ⚠ Piège Rose rencontré et corrigé : `i` est l'unité imaginaire (`i*i = -1`).
> Un compteur nommé `i` donnait `arith = -1 999 999` et des clés de table `"ki"`.
> Tous les compteurs du banc s'appellent `k`.

---

## 2. Protocole de mesure — deux horloges, jamais confondues

- **Horloge murale externe** : `time.perf_counter()` dans le pilote Python,
  autour du processus entier (démarrage du moteur inclus). C'est la colonne
  « mur ». Elle sert au **classement des backends**.
- **Horloge interne Rose** : `temps()` (nanoseconds depuis l'époque Unix),
  appelé dans le `.rose` juste avant et juste après chaque charge. C'est le
  tableau « détail des horloges internes ». **Jamais additionné au temps mur.**

Entrelacement : `rotation r` exécute les backends actifs dans l'ordre décalé
`[r:] + [:r]`, donc chaque moteur est mesuré une fois par rotation, réparti
sur des instants différents. 9 rotations = 9 échantillons par backend.

Vérification : à **chaque** échantillon, les 5 valeurs `RESULT`, le `CHECKSUM`,
et (pour les AOT) les 2 valeurs du noyau sont comparés aux constantes
attendues. Un écart lève une exception et retire le backend des mesures.

---

## 3. Tableau comparatif réel (médianes de 9 échantillons)

| backend | mur médian | mur min | mur max | arith int | recur int | chaîne int | liste int | table int |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| JIT Cranelift (défaut) | **26.504 ms** | 26.356 | 26.755 | 13.965 ms | 1.040 ms | 2.875 ms | 2.034 ms | 2.702 ms |
| Interpréteur (`ROSE_SANS_JIT=1`) | 343.387 ms | 342.434 | 348.614 | 54.908 ms | 277.930 ms | 2.887 ms | 1.928 ms | 2.638 ms |
| Exécutable autonome (`rose comp`) | 27.709 ms | 27.604 | 27.911 | 14.000 ms | 1.053 ms | 2.781 ms | 2.018 ms | 2.675 ms |
| AOT LLVM ORC (`rose orc --link`) — **noyau entier seul** | 4.271 ms | 4.059 | 4.463 | n/a | n/a | n/a | n/a | n/a |
| AOT LLVM classique (`rose llvm`) — **noyau entier seul** | 278.445 ms | 278.283 | 279.985 | n/a | n/a | n/a | n/a | n/a |

### Détail des horloges internes Rose (`temps()`, médiane [min..max] en ms)

| backend | arith | recur | chaîne | liste | table |
|---|---:|---:|---:|---:|---:|
| JIT Cranelift | 13.965 [13.930..13.998] | 1.040 [1.028..1.064] | 2.875 [2.810..2.932] | 2.034 [2.000..2.079] | 2.702 [2.674..2.735] |
| Interpréteur | 54.908 [54.798..55.570] | 277.930 [277.234..283.397] | 2.887 [2.842..2.944] | 1.928 [1.919..1.979] | 2.638 [2.611..2.682] |
| `rose comp` | 14.000 [13.957..14.025] | 1.053 [1.032..1.081] | 2.781 [2.753..2.827] | 2.018 [1.992..2.078] | 2.675 [2.636..2.716] |

Sortie brute intégrale : `/tmp/opencode/bback/harness_final2.txt` (reproductible
par la commande §4).

---

## 4. Commandes exactes

Toutes depuis la racine `/home/clausake/Stefan/ROSE` (impératif pour
`rose llvm` : le binaire pose `PYTHONPATH=archive/rose2_old_python/src`
**relatif** ; lancé depuis un autre répertoire il échoue avec
`ModuleNotFoundError: No module named 'rose2'`).

```bash
cd /home/clausake/Stefan/ROSE

#oracle de syntaxe
./rose/rose verif bench/backends/bench_backends.rose
./rose/rose verif bench/backends/bench_backends_core.rose

#harnais complet (9 échantillons entrelacés, pin CPU 7)
taskset -c 7 ./bench/backends/run_backends.sh

#backends complets — commandes unitaires
taskset -c 7 ./rose/rose bench/backends/bench_backends.rose
taskset -c 7 env ROSE_SANS_JIT=1 ./rose/rose bench/backends/bench_backends.rose
taskset -c 7 ./rose/rose comp bench/backends/bench_backends.rose -o /tmp/opencode/bback/bench_comp
taskset -c 7 /tmp/opencode/bback/bench_comp

#backends AOT — noyau entier uniquement
taskset -c 7 ./rose/rose orc bench/backends/bench_backends_core.rose -o /tmp/opencode/bback/bench_orc --link
taskset -c 7 /tmp/opencode/bback/bench_orc
taskset -c 7 ./rose/rose llvm bench/backends/bench_backends_core.rose -o /tmp/opencode/bback/bench_llvm
taskset -c 7 /tmp/opencode/bback/bench_llvm

#backends échoués — reproduction des diagnostics
taskset -c 7 env ROSE_NO_RIR=1 ./rose/rose bench/backends/bench_backends.rose
taskset -c 7 env ROSE_NATIF=1 ROSE_SANS_JIT=1 ./rose/rose bench/backends/bench_backends.rose
./rose/rose rir bench/backends/bench_backends.rose > /tmp/opencode/bback/full.rir
taskset -c 7 ./rose/rose compilateur/emetteur_elf.rose /tmp/opencode/bback/full.rir /tmp/opencode/bback/full.elf
taskset -c 7 ./rose/rose bc bench/backends/bench_backends.rose -o /tmp/opencode/bback/b.rosebc
```

`--link` est **obligatoire** pour `rose orc` : sans lui la commande annonce
« produit » mais n'écrit qu'un `.ll` (IR LLVM texte) et aucun exécutable.

---

## 5. Vérifications d'égalité des résultats

Jeu complet — même sortie sous JIT, interpréteur et `rose comp`, à chaque
échantillon :

```
arith     = 2666664666667000000
recur     = 832040
chaine    = 23997
liste     = 22500
table     = 8000
CHECKSUM  = 2666664666667886537
```

Noyau AOT — même sortie sous JIT, interpréteur, `orc` et `llvm` :

```
2666664666667000000
832040
```

Rapport du harnais : `✓ AUCUNE DIVERGENCE entre backends actifs` (un seul
tuple observé par famille de backends).

---

## 6. Backends échoués ou indisponibles — documentés, pas masqués

### 6.1 `ROSE_NO_RIR=1` (chemin « RIR désactivé ») — ÉCHEC de correction

Commande unitaire : `ROSE_NO_RIR=1 ./rose/rose bench/backends/bench_backends.rose`.

Sortie réelle (jeu complet) :

```
=== BENCH BACKENDS ROSE v1 ===
TAILLES 2000000 30 8000 15000 8000
\sum_{k = 1}^{n - 1} \langle s = s + k \cdot k\rangle
RESULT arith 2666664666667000000 4662564 ns (4.662 ms)
RESULT recur 832040 2791609 ns (2.791 ms)
RESULT chaine 3 10623 ns (10.623 µs)
Rose : erreur — index hors des limites de la liste
```

Deux anomalies : (a) une ligne LaTeX parasite `\sum_{k = 1}^{n - 1} ...`
apparaît sur stdout ; (b) `chaine` vaut **3** au lieu de 23997 (la boucle
n'accumule pas), puis le chargement de liste lève
`index hors des limites de la liste`. **Backend exclu du classement.** Ce n'est
pas un problème du banc : les mêmes corps de charge passent sous JIT,
interpréteur et `comp`.

### 6.2 `ROSE_NATIF=1 ROSE_SANS_JIT=1` (émetteur machine natif) — ÉCHEC de correction

Commande unitaire : `ROSE_NATIF=1 ROSE_SANS_JIT=1 ./rose/rose bench/backends/bench_backends.rose`.

Jeu complet : `RESULT chaine 75 != 23997`, `rc=1` au contrôle du harnais.
Jeu réduit (`... rapide`), sortie réelle :

```
RESULT arith 333283335000 42656 ns (42.656 µs)
RESULT recur 2584 1795044533610603919 ns (498623.481 h)
RESULT chaine 39 6048 ns (6.048 µs)      # attendu 147
RESULT liste 18 5216 ns (5.216 µs)       # attendu 150
RESULT table 10 19008 ns (19.008 µs)     # attendu 50
CHECKSUM 2657                            # attendu 333283337931
```

Les entiers sont bons, mais chaîne/liste/table sont faux et `temps()` renvoie
des nanoseconds absurdes (≈1,8×10¹⁸ ns ≈ 498 623 h) — l'horloge interne ne
survit pas à cette voie. **Backend exclu du classement.**

Note : `ROSE_NATIF=1` **seul** (sans `ROSE_SANS_JIT=1`) donne les bonnes valeurs,
mais c'est trivialement le chemin Cranelift : d'après `rose/src/interp.rs`,
`activer_emetteur_natif()` ne remplace que les fonctions que Cranelift n'a pas
compilées. Ce n'est donc **pas** un backend distinct mesurable ici.

### 6.3 Émetteur ELF auto-hébergé (`compilateur/emetteur_elf.rose`) — sous-ensemble insuffisant

```
$ ./rose/rose rir bench/backends/bench_backends.rose > full.rir
$ ./rose/rose compilateur/emetteur_elf.rose full.rir full.elf
emetteur_elf G4 : RIR hors sous-ensemble (fonctions, appels, bin, sauts : runtime requis)
rc=2
```

La chaîne **elle-même fonctionne** : sur `>> 42`, elle produit un ELF de 132
octets dont le code de sortie est 42 (vérifié le 19/11). Mais le G4 refuse
fonctions, appels, binaires et sauts : le banc en comporte. **Aucune mesure
inventée**, backend documenté indisponible pour cette charge.

### 6.4 RVM `.rosebc` — câblage CLI absent

```
$ ./rose/rose bc bench/backends/bench_backends.rose -o b.rosebc
rose: fichier introuvable : bc        # rc=1
```

Idem pour `rose vm` et `rose rvm`. `rose/docs/ROSE_VM.md` (18/11) l'énonce
explicitement : module `rose/src/rosevm.rs` livré et testé (8 tests), **câblage
CLI non fait**. Patch nécessaire (hors de mon périmètre, pas appliqué) : ajouter
dans `main.rs`/`lib.rs` `rose bc prog.rose [-o prog.rosebc]` (analyse →
`rir::abaisser` → `verifier_sous_ensemble` → `ecrire_bytecode`) et
`rose run prog.rosebc` (`lire_bytecode` → `executer_sandbox`, affichage de
`hote.sortie`). Le sandbox du bytecode interdit E/S, appels et FFI : même une
fois câblé, il ne pourra pas exécuter le banc complet (qui appelle `temps()`,
les chaînes, listes et tables) — seulement un sous-ensemble arithmétique borné.
**Aucune mesure RVM rapportée.**

### 6.5 AOT `rose llvm` / `rose orc` — limitation de compilation, pas un échec

Ni l'un ni l'autre ne compile `temps()` (ORC : panic interne
`Found IntValue(...) but expected the FloatValue variant` sur `%phi_s` ;
LLVM : `fonction 'temps' inconnue en génération C`). ORC rejette en plus les
littéraux chaîne, listes et tables ; LLVM rejette `sous_chaine` et `table`.
Ils exécutent donc `bench_backends_core.rose` (mêmes corps arithmétiques et
récursifs), mesurés au seul chronomètre mural. Leurs lignes « int » sont `n/a`
parce qu'aucune horloge interne n'est disponible sur ces chemins — **pas
parce qu'elles ont été omises par commodité**.

---

## 7. Lecture honnête des écarts

- **AOT gagne sur la boucle entière** : sur le *noyau* (2×10⁶ carrés + fib(30)),
  ORC fait 4.271 ms en médiane, contre 26.504 ms au JIT pour le *jeu complet*
  — **ces deux nombres ne sont pas comparables directement**, le JIT paie en
  plus 3 charges chaîne/liste/table. La comparaison AOT-la-plus-rapide porte
  sur ORC (4.271 ms) vs LLVM classique (278.445 ms) sur le **même noyau** :
  ORC est ~65× plus rapide. Constat brut, sans extrapolation.
- **JIT gagne sur la récursion** : 1.040 ms contre 277.930 ms à
  l'interpréteur, soit ~267×. C'est l'écart dominant du banc.
- **Interpréteur dernier sur le jeu complet** : 343.387 ms contre 26.504 ms,
  soit ~13×. Mais il est **légèrement plus rapide que le JIT** sur chaîne
  (2.887 vs 2.875 ms, à peine), liste (1.928 vs 2.034 ms, ~5 %) et table
  (2.638 vs 2.702 ms, ~2,4 %) : ces opérations ne sont pas JIT-compilées et
  ne paient pas la surcharge de dispatch JIT. Ne pas conclure « le JIT gagne
  partout ».
- **`rose comp` ≈ JIT** (27.709 vs 26.504 ms, ~4,5 %) : l'exécutable autonome
  embarque le même moteur JIT ; son intérêt est l'absence d'installation, pas
  la vitesse.
- **Bruit** : les écarts internes sur une même charge sont de l'ordre de
  1 à 2 % (ex. arith JIT 13.930–13.998 ms), sauf `recur` interpréteur
  (277.234–283.397 ms, ~2 %). Les ratios cités sont des constats sur 9
  échantillons, pas des lois physiques.

---

## 8. État final factuel

Fait : 4 fichiers créés dans `bench/backends/`, 1 doc créée. Les deux
programmes Rose passent `verif` et produisent des valeurs déterministes
identiques sur tous les backends retenus. Le harnais a été **exécuté trois fois**
(la dernière le 19/11/2026 00:30 CET), 9 échantillons entrelacés pinés CPU 7,
tableau et vérifications reproduits ci-dessus.

Vérifié : égalité des valeurs (5 charges + checksum) entre JIT, interpréteur et
`comp` ; égalité du noyau entre JIT, interpréteur, `orc` et `llvm` ; échecs
reproductibles de `ROSE_NO_RIR`, `ROSE_NATIF+ROSE_SANS_JIT`, self-host G4 et RVM.

Reste : câblage CLI RVM (hors périmètre), correction des deux divergences de
correction `ROSE_NO_RIR` / `ROSE_NATIF+SANS_JIT` (hors périmètre : moteur `*.rs`),
extension de l'émetteur auto-hébergé au-delà du sous-ensemble G4 (hors périmètre).
Aucun de ces points n'a été masqué ni présenté comme une mesure.
