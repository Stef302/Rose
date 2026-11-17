# Jardins — bibliothèques Rose écrites en Rose

Un **jardin** est un module de bibliothèque écrit en Rose pur : il ne
dépend d'aucun code Rust, il est chargé par un programme qui le demande.

```
jardins/
├── premiers.rose        # nombres premiers, facteurs, pgcd (Rose pur)
├── tests/premiers.rose  # tests + cross-check contre les primitives natives
└── README.md
```

## La règle de répartition (validée le 16/11)

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

## Pourquoi un jardin Rose est PLUS LENT qu'une primitive native

Mesuré sur cette machine (même algorithme : test de division jusqu'à √n,
N = 50 000, nombres premiers < N) :

| Voie | Temps |
|---|---|
| boucle nue (aucun appel) | 26 ms |
| boucle + appel **natif** `est_premier` | 51 ms |
| boucle + appel **fonction Rose triviale** | 32 ms |
| boucle + **jardin Rose** `est_premier` | 102 ms |
| **C** `-O2` (même algorithme) | 4 ms |
| **C** `-O0` (sans optimisation) | 3 ms |
| **moteur C++ Rose** (interpréteur) | 3 212 ms |

Trois faits :

1. **Le jardin Rose est ~2× plus lent que la primitive native** (102 vs 51 ms)
   — pas 50×. Le coût d'une fonction Rose est modeste parce que le JIT
   compile le corps principal (boucle) et que l'appel retombe sur
   l'interpréteur, qui cherche chaque variable PAR NOM dans une table
   (`HashMap<String, Valeur>`) à **chaque** accès.
2. **Le coût réel est dans l'interpréteur, pas dans l'appel** : la fonction
   triviale (`n % 2 == 0`) coûte à peine plus que la boucle nue (32 vs 26 ms).
   Ce qui coûte, c'est de refaire le travail de l'interpréteur pour **chaque
   instruction** du corps (si, tant_que, affectation).
3. **Rose est 50× plus lent que le C optimisé** (201 vs 4 ms sur N = 100 000)
   mais **16× plus rapide que son propre moteur C++** (3 212 ms) — et le C
   `-O0` montre que l'écart est dû à la lecture/évaluation, pas à
   l'algorithme.

**Conséquence** : un jardin Rose est parfait pour l'algorithmique que le
natif ne porte pas, mais il ne remplace PAS une primitive chaude comme
`est_premier`. Si `est_premier` devient un jardin, il faut garder la
primitive native comme accélérateur (ou l'accepter 2× plus lent).

## Ce qui manque au langage pour que les jardins vivent

1. ~~**`importer`**~~ — **RÉSOLU le 17/11**. Un fichier commence par
   `importer "jardins/premiers.rose"` et les fonctions du jardin sont
   disponibles SANS recopier leur corps. Les imports sont résolus
   récursivement (un jardin peut importer un autre jardin), chaque fichier
   n'étant lu qu'une fois, et la vérification a lieu APRÈS la fusion.
   `jardins/tests/premiers.rose` est passé de 121 à **72 lignes** en
   remplaçant la copie du jardin par une seule ligne d'import.
2. **Liste vide typée** — Rose refuse `[]` (« le type d'élément est
   introuvable ») ; le jardin doit rembourrer à l'index 0 puis dépouiller.
   Une annotation `[0 : entier]` réglerait le problème une fois pour toutes.
3. ~~**Accès par nom**~~ — **RÉSOLU le 16 et 17/11** : les corps de fonction
   ET le corps principal sont abaissés en **RIR** (`rose/src/rir.rs`) : un
   tableau plat d'opérations, des sauts numériques et des variables en
   **slots indexés**. C'est la forme d'exécution « comme Zig ».
   Mesures : sans JIT, le jardin des premiers passe de 3,65 s à **0,57 s**
   (6×) ; une boucle nue de 2 M de tours au niveau principal de 1 660 ms à
   **826 ms** ; `it(30000)` (récursion) de 44-62 ms à **31 ms** ; le jardin
   avec JIT de 50 ms à **47 ms**.

## Ajouter un jardin

1. créer `jardins/mon_jardin.rose` (fonctions seulement, aucun `>>`) ;
2. créer `jardins/tests/mon_jardin.rose` avec `verifier(...)` et, quand
   c'est possible, un cross-check contre la primitive native ;
3. exécuter : `rose jardins/tests/mon_jardin.rose` (doit afficher 0 « ✗ »).
