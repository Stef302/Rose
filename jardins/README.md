# Jardins — bibliothèques Rose écrites en Rose

## 1. Qu'est-ce qu'un jardin ?

Un **jardin** est un module de bibliothèque écrit en **Rose pur** : un fichier
`.rose` qui ne contient que des définitions de fonctions (`fn`), sans
instruction programme (`>>`), sans code Rust, sans bibliothèque C, sans wasm et
sans runtime externe. Il ne s'exécute pas seul : un programme le charge avec
`importer "jardins/<nom>.rose"` et appelle ses fonctions comme s'il les avait
écrites lui-même.

Un jardin n'est **pas** un morceau du compilateur : il ne touche ni au lexer,
ni au vérificateur de types, ni à la mémoire (`Gz`, `Gq`, `Rc`, `Box`), ni aux
opérateurs. C'est de l'algorithmique exprimée dans le langage, avec les
symboles et le calcul entier exact de Rose.

## 2. Catalogue des jardins (état au 19/11/2026)

| Jardin | Contenu | Fonctions exportées | Import |
|---|---|---|---|
| [`premiers.rose`](premiers.rose) | nombres premiers, facteurs, pgcd, sommes | `est_premier`, `nb_premiers_inferieurs`, `prochain_premier`, `facteurs_premiers`, `pgcd_jardin`, `somme_premiers` | `importer "jardins/premiers.rose"` |
| [`hachage.rose`](hachage.rose) | codes de caractères, checksum, hachage djb2 exact et modéré | `codes_caracteres`, `somme_caracteres`, `hachage_djb2`, `hachage_djb2_mod` | `importer "jardins/hachage.rose"` |
| [`texte.rose`](texte.rose) | rendu texte inspiré de Rich, en Rose pur (panneau, tableau, règle) — **sans Python** | `panneau`, `tableau_simple`, `barre`, `motif_repetit`, `ajuster` | `importer "jardins/texte.rose"` |
| [`gui.rose`](gui.rose) | fenêtres et widgets « comme tkinter », en Rose pur : `rendre` produit un HTML autonome (le navigateur est l'hôte du rendu, **aucune fenêtre Os n'est ouverte**) | `fenetre`, `bouton`, `champ`, `etiquette`, `case`, `liste`, `ajouter`, `rendre`, `evenement`, `lier_evenement`, `grille`, `empiler`, `cadrer`, `compter_widgets`, `compter_actions` | `importer "jardins/gui.rose"` |
| [`math.rose`](math.rose) | délocalisation de 34 primitives numériques de la stdlib (arithmétique, suites, abs/min/max, plancher, fraction, durée) | 40 `fn` (dont 6 helpers) — voir `rose/docs/DELOCALISATION_40.md` | `importer "jardins/math.rose"` |
| [`chaines.rose`](chaines.rose) | délocalisation de 5 primitives chaîne (`longueur`, `len`, `ordre`, `caractere`, `concatener`) — `ordre`/`caractere` sur l'ASCII imprimable uniquement | 6 `fn` (dont 1 helper) | `importer "jardins/chaines.rose"` |
| [`listes.rose`](listes.rose) | utilitaires de listes (tri, filtres, replis, aplatissement) — **hors STDLIB**, ne comptent PAS dans la délocalisation | 11 `fn` | `importer "jardins/listes.rose"` |

Les tests vivent dans `jardins/tests/` et importent le jardin au lieu d'en
copier le corps : `jardins/tests/premiers.rose` est passé de 121 à 72 lignes
quand `importer` a été introduit. Le catalogue est générable à tout moment :

```sh
scripts/rose-jardin.sh liste
```

Sortie réelle au 19/11/2026 : **7 jardins, 95 fonctions exportées**. Le jardin
`texte.rose` est documenté dans
[`rose/docs/RICH_SANS_PYTHON.md`](../rose/docs/RICH_SANS_PYTHON.md), et la
délocalisation de la stdlib dans
[`rose/docs/DELOCALISATION_40.md`](../rose/docs/DELOCALISATION_40.md).

Les tests d'équivalence des nouveaux jardins se lancent ainsi (tous passent,
constat du 19/11/2026) :

```sh
taskset -c 10 ./rose/rose jardins/tests/math.rose      # 35 primitives, 45 vérifications, 0 échec
taskset -c 10 ./rose/rose jardins/tests/chaines.rose   # 5 fonctions, 0 échec
taskset -c 10 ./rose/rose jardins/tests/listes.rose    # 19 tests hors STDLIB, 0 échec
taskset -c 10 ./rose/rose jardins/tests/premiers.rose  # 20 tests existants, 0 échec
```

Performance (constat daté du 19/11/2026) : deux jardins ont été accélérés
sans changer leur API ni leur sortie — `compter_premiers_jardin` (× 2,024)
et `longueur_jardin` (× 2,364), médianes de 9 échantillons pinés c8. Les
décomposition, variantes rejetées et garde-fous de délocalisation sont
décrits dans [`rose/docs/JARDINS_RAPIDES.md`](../rose/docs/JARDINS_RAPIDES.md).
Les jardins restent **plus lents que les primitives** sur ces deux fonctions :
le gain visé est la réduction de la surface du noyau, pas la vitesse brute.

## 3. Comment importer un jardin

```rose
importer "jardins/hachage.rose"

>> chaine(hachage_djb2("hello"))          # 210714636441
>> chaine(hachage_djb2_mod("hello", 1000)) # 441
```

Règles de résolution (vérifiées dans `rose/src/front.rs::analyser_fichier`) :

- le chemin est pris **tel quel** (relatif au répertoire courant), puis
  **à côté du fichier importateur** ;
- les imports sont résolus **récursivement** — un jardin peut importer un
  autre jardin — et chaque fichier n'est lu **qu'une fois** ;
- la vérification a lieu **après la fusion** des signatures : le programme
  qui importe voit les fonctions du jardin sans recopier leur corps ;
- deux définitions du même nom sont une erreur explicite.

Lancez le programme depuis la racine du dépôt :

```sh
./rose/rose /tmp/opencode/test_hachage.rose
```

## 4. Comment créer un jardin

L'utilitaire `scripts/rose-jardin.sh` fait le travail (bash seul, aucune
dépendance) :

```sh
scripts/rose-jardin.sh init fibonacci    # crée jardins/fibonacci.rose documenté
scripts/rose-jardin.sh verif fibonacci   # contrôle Rose pur + rose verif
scripts/rose-jardin.sh liste             # catalogue + signatures extraites du AST
scripts/rose-jardin.sh verif-tous        # vérifie tous les jardins
scripts/rose-jardin.sh aide
```

`verif` contrôle cinq choses : présence du fichier, absence de marqueurs
Rust/FFI (`extern crate`, `use std::`, `@dll`, `charger(`, …), absence
d'instruction `>>` (un jardin est une bibliothèque), imports limités aux
fichiers `.rose`, et `rose verif` dont la sortie doit être exactement
`vérifié`. Un jardin interdit est refusé avec un code non nul.

Le squelette généré par `init` contient déjà un `fn` d'exemple et les règles
en commentaire ; il passe `rose verif` dès la création.

## 5. Conventions de nommage

- **fichier** : `jardins/<sujet>.rose`, en minuscules, `snake_case`, sans
  tiret (ex. `premiers.rose`, `hachage.rose`) ; pas de chemin relatif dans le
  nom passé à `init` ;
- **fonctions** : noms français en `snake_case` (`est_premier`,
  `codes_caracteres`). Les noms natifs sont réservés au cœur : on les suffixe
  (`pgcd_jardin`) quand un jardin reprend un algorithme déjà natif, pour
  rendre la comparaison explicite ;
- **paramètres** : noms simples (`n`, `s`, `m`), pas de préfixe `$` ;
  **variables locales** : préfixe `$` (`$h`, `$i`), comme dans le cœur ;
- **en-tête** : chaque jardin commence par un commentaire indiquant qu'il est
  une bibliothèque, comment l'importer, et pourquoi il est en Rose pur ;
- **tests** : `jardins/tests/<sujet>.rose`, avec `verifier(nom, obtenu,
  attendu)` et, quand c'est possible, un cross-check contre la primitive
  native ;
- liste vide typée : Rose n'a pas (encore) de littéral `[]` typé ; on écrit
  `[0 : entier]` (le `0` donne le type d'élément, ce n'est **pas** un élément).

## 6. Jardin Rose vs bibliothèque C appelée via wasm

Les deux voies existent dans Rose, elles ne se confondent pas :

| | Jardin Rose | Bibliothèque C via wasm |
|---|---|---|
| Livrable | un fichier `.rose` | un module `.wasm` compilé (ex. `libxxhash.wasm`, 6 901 octets) |
| Appel | `importer "jardins/x.rose"` puis `x(...)` | `@dll("wasm", …)` / `@sym(…)` + `rose wasm` |
| Chaîne d'outils | aucune | clang/wasi-sdk, glue d'ABI, shims `memcpy`/`memset`, `-nostdlib` |
| Langage d'écriture | Rose | C (ou Rust→wasm) |
| Vérification | `rose verif` | parseur de sections wasme + exécution dans `wasm_lecteur.rs` |
| Calcul | entier exact illimité | types machine (`i32`, `i64`), débordement possible |
| Dépendance au runtime | aucune | interpréteur de pile wasm du moteur |

La voie C/wasm est documentée et mesurée dans
[`rose/docs/INTEROP_BIBLIOTHEQUE.md`](../rose/docs/INTEROP_BIBLIOTHEQUE.md) :
elle apporte du code C **existant** (xxHash v0.8.2, BSD-2-Clause) au prix d'un
artefact binaire, d'une glue d'ABI et d'un chargeur. Le jardin Rose apporte
l'algorithme **écrit en Rose**, sans artefact ni chargeur. Aucune des deux
voies n'est revendiquée « plus rapide que C » ici : voir les mesures datées
ci-dessous.

## 7. Avantages du jardin Rose

- **calcul exact** : les entiers sont des BigInt natifs (zéro GMP). Le djb2
  de `hachage.rose` sur `"hello"` vaut `210714636441` en exact, alors que le
  même algorithme en C tronque à `261238937` sur 32 bits ;
- **symboles Rose** : les fonctions sont des noms du langage, visibles dans
  `rose ast`, vérifiées par `rose verif`, composables par `importer` — pas des
  adresses dans une table d'import wasm ;
- **aucun runtime externe** : ni libc, ni wasmtime, ni DLL, ni GMP. Le seul
  prérequis est le binaire `rose/rose` du dépôt ;
- **portabilité du source** : un jardin est un fichier texte lisible et
  modifiable avec un éditeur, sans compilateur C ni SDK ;
- **évolutivité** : le langage peut porter de nouveaux noms de fonctions sans
  toucher au cœur Rust.

## 8. Mesures datées : pourquoi un jardin n'est pas une primitive chaude

Mesuré sur cette machine le **16/11/2026** (même algorithme : test de division
jusqu'à √n, N = 50 000, nombres premiers < N) :

| Voie | Temps |
|---|---|
| boucle nue (aucun appel) | 26 ms |
| boucle + appel **natif** `est_premier` | 51 ms |
| boucle + appel **fonction Rose triviale** | 32 ms |
| boucle + **jardin Rose** `est_premier` | 102 ms |
| **C** `-O2` (même algorithme) | 4 ms |
| **C** `-O0` (sans optimisation) | 3 ms |
| **moteur C++ Rose** (interpréteur) | 3 212 ms |

Trois faits établis par ces mesures :

1. **Le jardin Rose est ~2× plus lent que la primitive native** (102 vs 51 ms)
   — pas 50×. Le coût d'une fonction Rose est modeste parce que le JIT
   compile le corps principal (boucle) et que l'appel retombe sur
   l'interpréteur, qui cherche chaque variable PAR NOM dans une table
   (`HashMap<String, Valeur>`) à **chaque** accès.
2. **Le coût réel est dans l'interpréteur, pas dans l'appel** : la fonction
   triviale (`n % 2 == 0`) coûte à peine plus que la boucle nue (32 vs 26 ms).
3. **Rose est plus lent que le C optimisé** sur cette boucle, et plus rapide
   que son propre moteur C++ — l'écart vient de la lecture/évaluation, pas de
   l'algorithme.

**Conséquence** : un jardin Rose est fait pour l'algorithmique que le natif ne
porte pas, pas pour remplacer une primitive chaude. Ces chiffres sont un
**constat daté**, pas une garantie de performance ; les re-mesurer après tout
changement de moteur.

## 9. La règle de répartition (validée le 16/11)

| Ce qui reste dans le **cœur** (natif Rust) | Ce qui peut devenir un **jardin** |
|---|---|
| lexer, parseur, vérificateur de types | `est_premier`, `compter_premiers` |
| interpréteur et **mémoire** (`Gz`, `Gq`, `Rc`, `Box`) | `pgcd`, `ppcm`, `fibonacci` |
| **opérateurs** `!`, `!!`, `#`, `?`, `$`, préfixe `!` | `nombre_pi`, `nombre_e`, `phi`, `racine` |
| **types** : entier exact, rationnel, complexe, liste, table, chaîne | algorithmique : tri, graphes, statistiques, arithmétique modulaire |
| E/S (`afficher`, `lire_texte`, `ecrire_texte`) | générateurs aléatoires dérivés |

⚠ **Les opérateurs factoriels ne peuvent PAS être un jardin** : `n!` est une
primitive du langage, comme `+` ou `^`. En revanche, les **noms de
fonctions** (`factorielle(n)`, `primorielle(n)`, `terminal(n)`) sont des
candidats naturels : rien n'oblige le langage à porter ces noms.

## 10. Checklist pour ajouter un jardin

1. `scripts/rose-jardin.sh init mon_sujet` ;
2. remplacer le `fn` d'exemple par les vraies fonctions, documenter chaque
   fonction et sa complexité, garder l'en-tête « bibliothèque » ;
3. `scripts/rose-jardin.sh verif mon_sujet` (doit afficher `✓ JARDIN VALIDE`) ;
4. créer `jardins/tests/mon_sujet.rose` avec `verifier(...)` et un cross-check
   natif quand c'est possible ; le lancer : `./rose/rose jardins/tests/mon_sujet.rose`
   (aucun `✗`) ;
5. ajouter la ligne du jardin au tableau du catalogue (§2).

## 11. État du langage vis-à-vis des jardins

1. ~~**`importer`**~~ — **RÉSOLU le 17/11**. Un fichier commence par
   `importer "jardins/premiers.rose"` et les fonctions du jardin sont
   disponibles SANS recopier leur corps. Les imports sont résolus
   récursivement, chaque fichier n'étant lu qu'une fois, et la vérification a
   lieu APRÈS la fusion.
2. **Liste vide typée** — Rose refuse `[]` (« le type d'élément est
   introuvable » ) ; le jardin utilise `[0 : entier]`. Une annotation
   `[0 : entier]` dédiée réglerait le problème une fois pour toutes.
3. ~~**Accès par nom**~~ — **RÉSOLU le 16 et 17/11** : les corps de fonction
   ET le corps principal sont abaissés en **RIR** (`rose/src/rir.rs`) : un
   tableau plat d'opérations, des sauts numériques et des variables en
   **slots indexés**. Mesures sans JIT : le jardin des premiers passe de
   3,65 s à **0,57 s** (6×) ; une boucle nue de 2 M tours de 1 660 ms à
   **826 ms** ; `it(30000)` de 44-62 ms à **31 ms** ; le jardin avec JIT de
   50 ms à **47 ms**.
