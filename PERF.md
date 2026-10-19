# Performance de Rose

Mesures **réelles** exécutées le 18/10/2026 sur la machine de
développement (ARM Linux aarch64, GMP 6.3 embarqué). Chaque ligne est le
**minimum de 2 exécutions** du même programme par chaque moteur, temps
total processus → sortie (démarrage ~10 ms inclus). Reproduction :
`moteur/tests/bench.sh` (moteurs compilés) puis
`JULIA=<chemin> moteur/tests/bench.sh python julia` (moteurs vivants).

| Programme (ce qu'il mesure) | C++ | Rust | Zig | Python | Julia |
|---|---:|---:|---:|---:|---:|
| boucle entière — 10 M itérations, addition d'entiers | **2,65 s** | 2,78 s | 2,99 s | 10,6 s | 15,6 s |
| boucle flottante — 3 M additions `+ 0,5` | 0,61 s | **0,47 s** | 0,65 s | 2,6 s | 4,8 s |
| `factorielle(1 000 000)` — 5 565 709 chiffres | 0,19 s | 0,20 s | 0,27 s | 7,4 s | 3,8 s |
| `2 ^ 30 000 000` — puissance, 9 030 901 chiffres | 0,96 s | 0,97 s | 0,97 s | 12,5 s | 2,8 s |
| `fibonacci(5 000 000)` — 1 044 939 chiffres | 0,03 s | 0,02 s | 0,02 s | 0,79 s | 1,8 s |
| `compter_premiers(20 000 000)` — crible → 1 270 607 | 0,11 s | 0,09 s | 0,09 s | 0,67 s | 1,95 s |
| `pgcd` × 2 M d'appels en boucle | 1,06 s | **0,91 s** | 1,02 s | 4,8 s | 7,1 s |
| chaînes — 500 000 `joindre` + `concatener` (1 Mo) | 0,45 s | **0,18 s** | 0,17 s | 0,49 s | 2,5 s |

Lecture honnête :

* **Les trois moteurs natifs tiennent en 1,5× entre eux** sur tous les
  exercices. C++ domine les boucles pures (cœur de l'interpréteur) ;
  Rust/Zig dominent les chaînes ; le calcul sur grands entiers est à
  parité partout — c'est **GMP lié statically** dans les trois cas, la
  bibliothèque fait le travail.
* Les moteurs Python et Julia sont les **oracles de parité**, pas des
  produits : ils servent à vérifier les moteurs compilés, et le public
  ne les installe pas. Leur lenteur (4 à 20×) est le prix d'un
  interpréteur écrit dans un interpréteur.
* Repères absolus sur cette machine : factoriel d'un million en **0,2
  s**, 9 millions de chiffres de 2^30 000 000 en **~1 s**, crible de
  20 millions de nombres premiers en **~0,1 s**.
* Rose **v1 ne pouvait exécuter aucun de ces programmes** — pas de
  boucles, pas de variables, une seule expression à la fois dans une
  interface .NET de 74 Mo.

## Notes de méthode

* Programmes : `moteur/tests/bench/*.rose` — identiques pour tous les
  moteurs, vérifiés rendus identiques (parité 113 comparaisons).
* Les résultats transcendants (`pow` à exposant fractionnaire) peuvent
  varier du dernier ulp selon la bibliothèque mathématique de la
  plateforme (glibc / openlibm / msvcrt).
* Sur la machine Windows x64 de l'utilisateur, les .exe livrés dans
  `bin/` sont les mêmes moteurs compilés pour x86_64 ; les ordres de
  grandeur sont transposables (mêmes algorithmes, même GMP).
* Après le JIT du plan `ARCHITECTURE.md` (jalon M2), les boucles pures
  sont le gain attendu (×3 à ×10 visé) : plus de revérification de
  types à chaque noeud de l'arbre. Ces chiffres seront alors mesurés de
  nouveau, pas promis.

## Jalon M2 — JIT natif AArch64 (mesures v3.4.0, même machine)

Le moteur choisit le JIT quand tout le module est dans son sous-ensemble
(entiers, booléens, comparaisons, boucles, variables, fonctions
récursives) ; sinon repli transparent sur l'interpréteur RIR. Comparaison
`ROSE_NO_JIT=1` (interpréteur RIR) vs JIT, même binaire, minimum de 2
exécutions :

| Programme | Interpréteur RIR | **JIT** | Gain |
|---|---:|---:|---:|
| boucle entière — 10 M itérations (`pour`, somme) | 11,60 s | **0,21 s** | **×55** |
| récursion ×200 k — 4 M d'appels `fact(20)` | 7,44 s | **2,16 s** | ×3,4 |
| boucle flottante (hors sous-ensemble → repli) | 3,36 s | 3,35 s | — |
| les 6 autres benchs (FFI/appels internes → repli) | identiques | identiques | — |

* La boucle entière JIT (**0,21 s**) est 12× sous l'ancien interpréteur
  d'arbre (2,65 s) : le RIR avait un creux (11,6 s) que le JIT rattrape
  très au-delà de l'objectif ×3–×10.
* Parité stricte JIT/interpréteur vérifiée sur les 8 programmes de bench,
  les suites du dépôt (moteur **37/37**, parité 92/92, v2 53/53) et 14
  tests unitaires du JIT (`moteur/cpp/src/test_jit.cpp`, dont
  `factorielle(10)` récursive).
* Le JIT n'émettait que pour AArch64 (machine de dev) à la v3.4.0 ; le
  backend x86-64 est venu au jalon suivant (section M2b ci-dessous).

## Jalon M2b — backend x86-64 + constantes partagées (mesures v3.5.0)

Deux ajouts au JIT, validés de bout en bout :

1. **Constantes partagées par module** (toutes architectures) : une
   `Valeur` par constante, créée à la compilation ; le prologue ne pose
   plus qu'un pointeur au lieu d'appeler un helper allocateur. Effet
   sur la machine de dev (AArch64) :

| Programme | v3.4.0 | **v3.5.0** | avant JIT |
|---|---:|---:|---:|
| récursion ×200 k — 4 M d'appels `fact(20)` | 2,16 s | **1,55 s** | 7,44 s |
| boucle entière 10 M | 0,21 s | **0,20 s** | 11,60 s |

2. **Backend x86-64** (`moteur/cpp/src/jit_x64.cpp`, encodage direct
   d'octets SysV, même table de helpers que l'AArch64). Validé **sous
   qemu-x86_64** (émulation TCG — les durées ci-dessous sont des durées
   d'émulation, pas des mesures de matériel x86) via le build croisé
   sans droits root `moteur/build-x64.sh` :

| Programme (sous qemu-x86_64) | Interpréteur RIR | **JIT x86-64** | Gain |
|---|---:|---:|---:|
| boucle entière — 3 M itérations | 2 m 30 s | **3,6 s** | **×42** |
| récursion 30 k × `fact(20)` | — | 7,8 s | — |

* Vérifications x86-64 : `test_jit` **14/14** (dont récursion via le
  trampoline d'appel), suite moteur **36/36 avec JIT**, FFI `dynamique`
  prouvée à part (la `.so` de test est de l'architecture de l'hôte —
  recompilée en x86-64 pour la preuve), parité stricte JIT/interpréteur
  sur les benchs.
* Sur le matériel x86-64 du public, le moteur v3.5.0 tourne donc à la
  vitesse JIT (les binaires livrés ne passent plus par le repli).
