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

## Jalon M2.5 — arithmétique machine et réservoirs (mesures v3.5.x)

Le JIT baseline appelait GMP pour chaque opération, même sur `1 + 1`. Trois
changements invisibles sémantiquement (l'exactitude arbitraire est garantie
par le repli GMP au débordement — vérifié sur les bornes `LONG_MIN`, `2^63`,
division plancher négative, avec parité stricte JIT/interpréteur) :

1. **Fast-path entier machine** dans les helpers partagés : quand les
   opérandes tiennent dans un `long`, l'opération se fait en arithmétique
   machine (membres de `__mpz_struct` lus directement, zéro appel libgmp,
   débordement détecté par le compilateur) ; sinon GMP.
2. **Réservoir d'objets `Valeur`** : les objets rendus par les frames et
   les résultats d'appels sont recyclés au lieu d'être détruits (leur
   capacité GMP interne se réutilise).
3. **Réservoir de frames** par fonction dans le trampoline d'appel.

| Programme (AArch64, JIT) | v3.5.0 | **M2.5** | cumul vs interpréteur |
|---|---:|---:|---:|
| récursion ×200 k — 4 M d'appels `fact(20)` | 1,55 s | **0,16 s** | **×46** |
| boucle entière — 10 M itérations | 0,20 s | **0,15 s** | ×77 |

* Sous qemu-x86_64 (émulation), la boucle 3 M passe de 3,6 s à 2,3 s et
  `test_jit` reste 14/14 — le matériel x86-64 réel fera mieux encore.
* Suites après changement : moteur 37/37, parité 92/92, v2 53/53,
  `test_jit` 14/14 sur les deux architectures.

## Jalon M2.6 — alias de constantes SSA et test booléen inline (v3.5.2)

L'émission RIR copie chaque constante dans un temp avant de s'en servir.
Or une temp SSA n'est écrite qu'une fois : le JIT rend maintenant le temp
**alias du slot de la constante** (lecture directe, copie supprimée), et le
`br.cond` lit le champ booléen de la `Valeur` sur place au lieu d'appeler
`jit_test_b`. Les constantes partagées retournées par une fonction sont
protégées contre le réservoir d'objets par `jit_free_valeur`.

| Programme (AArch64, JIT) | M2.5 | **M2.6** |
|---|---:|---:|
| boucle entière — 10 M itérations | 0,15 s | **0,12 s** |
| récursion ×200 k — 4 M d'appels | 0,16 s | **0,15 s** |

Sous qemu-x86_64 : boucle 3 M 2,3 s → **1,55 s** ; `test_jit` 14/14 des
deux côtés, suites revues à **38/38** (ARM) et 37/37 (x64 sous émulation,
ffi_dyn sauté).

## Jalon M2.7-M2.8 — arithmétique inline et fusion producteur→affect (v3.5.3)

Trois suppressions d'appels par itération, à sémantique constante (tout cas
hors patron retombe sur l'appel helper précédent) :

1. **Arithmétique machine inline** : `add/sub/mul` sur entiers positifs d'un
   seul membre (membres GMP lus/écrits sur place, débordement → repli appel) ;
   `cmp` comparant deux positifs d'un membre écrit le booléen directement
   dans l'objet destination (le `bool_into` disparaît).
2. **Fusion producteur→affect** : quand une temp SSA n'est lue que par
   l'`affect` qui suit son producteur, le producteur écrit directement dans
   la variable (`$s = $s + 1` devient une écriture en place) et l'affect est
   supprimé. Les helpers concernés sont tous en `_into` (lecture avant
   écriture, aliasing dst==opérande toléré par GMP).
3. La boucle entière type ne coûte plus **aucun appel** par itération dès
   que les compteurs restent des positifs d'un seul membre.

| Programme (AArch64, JIT) | M2.6 | **M2.8** |
|---|---:|---:|
| boucle entière — 10 M itérations | 0,12 s | **0,07 s** |
| récursion ×200 k — 4 M d'appels | 0,15 s | **0,12 s** |

Sous qemu-x86-64 : boucle 3 M 1,55 s → **1,16 s**. Depuis l'interpréteur
RIR d'origine : boucle ×165.

Validation : suites 38/38 (ARM) et 37/37 (x64 sous émulation), parité
92/92, v2 53/53, `test_jit` 14/14 ×2, bornes machine identiques sur les
deux architectures, et stress « constante partagée retournée par un appel »
(aliasing + fusion combinés) correct × 200 k tours des deux côtés.

## Jalon M2.10 — inlining RIR des appels feuille (v3.6.0)

Les appels aux fonctions sans appel interne (feuilles) sont désormais
**inlinés dans l'appelant par transformation RIR**, avant l'émission :
copies d'arguments vers des slots frais, blocs du callee renommés et
renumérotés, chaque `retour` remplacé par « affect vers le slot de l'appelant
puis saut vers la reprise » (le bloc de l'appel est scindé à cet effet).
Le wrapper `jit_call` (allocation de trame, memset, nettoyage, allers-retours
réservoirs) disparaît entièrement de la boucle. La transformation étant
purement RIR, les deux backends (AArch64 et x86-64) la partagent sans
aucune modification d'encodage. Conditions de sécurité : callee sans `call`
(exclut donc la récursion), sans `retour` de type vide, terminators complets,
aucune branche interne vers le bloc d'entrée ; si l'inlinage dépasse la
capacité de trame, la fonction est recompilée normalement.

| Programme (AArch64, JIT) | v3.5.3 | **v3.6.0** |
|---|---:|---:|
| appels — 2 M d'appels feuille | 0,05 s | **0,03 s** |
| boucle entière — 10 M itérations | 0,07 s | 0,07 s |
| récursion ×200 k (non inlinable) | 0,12 s | 0,12 s |

Sous qemu-x86-64 : 2 M d'appels feuille en 1,2 s. Validation : suites
39/39 (ARM) et 38/38 (x64 sous émulation), parité 95/95, v2 53/53,
`test_jit` 14/14 ×2, bornes machine identiques ×2. Nouveau banc
`moteur/tests/bench/appels.rose`.

### Correction au passage — émetteur RIR (antérieure au JIT)

Le stress de l'inlining a révélé un vieux défaut de l'émetteur RIR : les noms
de blocs `si`/`tant_que`/`pour` étaient dérivés du seul **numéro de ligne** —
deux structures sur une même ligne physique (ex.
`si (a) { si (b) { retour 1 } retour 2 }`) produisaient des blocs jumeaux et
les cibles de branchement se mélangeaient : boucle infinie au JIT, **fausses
valeurs déjà à l'interpréteur**. S'y ajoutait un défaut de continuation : le
corps `si` était émis dans un bloc fixe au lieu du curseur `fn.blocs.back()`
utilisé par `tant_que`/`pour`, si bien que la suite d'un bloc après un `si`
imbriqué restait collée après le `br.cond` (morte) et le bloc « fin » interne
choisait sur un voisin. Corrigé par : compteur global dans les noms de blocs,
curseur mobile pour `alors`/`sinon`, et dump RIR sous `ROSE_RIR_DUMP=1`.
Doré de régression `t_si_imbr` (six valeurs vérifiées à la main) et trois
programmes de parité ajoutés.

## Jalon M2.11 — aléatoire natif et tampons d'octets (v3.7.0, porté v3.8.0)

Briques qui manquaient au langage pour les charges « vectorielles » :
`graine(n)` / `hasard(n)` (splitmix64 reproductible, identique sur toutes les
architectures), type **tampon** (buffer d'octets contigu, passé par référence),
`octets`/`tampon_case`/`tampon_pose`/`chaine(tampon)`, et `remplir_hasard(t,
alphabet)` qui exécute la boucle de tirage **dans le moteur** (masque exact
pour les alphabets puissance de deux avec recyclage de 10 caractères par
tirage, réduction de Lemire sinon — χ² = 56,1 sur 10 M de caractères, ddl 63).
Le modèle est celui de numpy : le langage lance un appel, le C fait la boucle.

Benchmark mot de passe (génération de N caractères dans a-zA-Z0-9-_, AArch64) :

| N | numpy 2.5 (même machine) | Rose v3.6 (FFI + liste de caractères) | **Rose v3.7** |
|---:|---:|---:|---:|
| 1 M | 12 ms | 3,4 s | **< 10 ms** |
| 10 M | 107 ms | ~34 s | **0,01 s** |
| 100 M | 1,1 s | ~5,7 min | **0,07 s** |
| 1 000 M | 17,0 s | ~40 min **et ~300 Go de RAM** (impossible) | **0,66 s, 1 Go de RAM** |

À comparer du C++ équivalent (~0,65 s) : parité. Le JIT n'intervient pas
(le module reste à l'interpréteur) : tout le coût est dans l'appel natif.

Au passage, piège latent fermé : les variables nommées `t<chiffres>`
collidaient avec les temporaires internes du RIR (mêmes emplacements →
corruption mémoire, bus error). Le vérificateur refuse désormais ces noms, et
un `static_assert(sizeof(Valeur))` garde les deux définitions synchronisées
(rose.cpp et rir.h) qui avaient dérivé pendant ce chantier.

## Jalon M2.12 — JIT des flottants (v3.9.0)

Le JIT ne connaissait que l'entier et le booléen : `$s = $s + 0.5` faisait
tomber **tout le module à l'interpréteur** (1,16 µs par itération). Le
flottant est pourtant le type le plus simple de Rose — un `double` dans la
Valeur, sans GMP ni sentinelle : c'est le candidat naturel du déboxage, et
il devient la première opération nativement émise par le JIT.

Côté AArch64 : `LDR D`/`STR D`/`FADD`/`FSUB`/`FMUL` sur `add.f`/`sub.f`/
`mul.f` avec garde « dst null ou opérande non flottant → helper C++ » (le
même chemin lent sert d'allocateur et de conversion tolérante, miroir exact
de `vers_d`). `div.f`, `divf.f`, `pow.f`, `neg.f` et les six comparaisons
`cmp.*.f` passent par des helpers dédiés (zéros de division = messages de
l'interpréteur, NaN comparé comme `cmp_vals`, jamais via `==`). Côté x86-64,
miroir strict en SSE2 : `movsd`/`addsd`/`subsd`/`mulsd` vérifiés par
`llvm-mc --show-encoding`. Les constantes flottantes sont matérialisées une
fois par module (round-trip `%g` exact via `strtod`) ; `affect`, `afficher`,
les paramètres d'appel (`jit_call`) et la copie des résultats acceptent
désormais le type flottant.

Mesures (même machine, minimum de passes) :

| Bench | v3.8.0 (interpréteur, module non compilable) | **v3.9.0 (JIT)** |
|---|---:|---:|
| boucle_flottante (3 M × +0,5) AArch64 | 3,48 s | **0,02 s (×170)** |
| boucle_flottante, qemu-x86_64 | 144,75 s | **0,99 s (×146)** |

Un seul garde-fou de sémantique interdit l'approximation : chaque opération
inline porte sa garde et retombe sur le helper C++ qui reproduit exactement
l'interpréteur (conversions tolérantes, erreurs, formatage). Vérifié doré
`t_flottant` (24 valeurs écrites à la main, dont 1,1 cumulé, 1/2⁴⁰ et π⋅r²),
suite 42/42 ARM, 41/41 croisée qemu, parité 110/110 (2 programmes flottants
neufs × C++/Rust/Python), test_jit 14/14, et les bancs entiers inchangés.

## Jalon M2.13 — builtins numériques natifs dans le JIT (v3.10.0)

Un seul `pgcd(k, ...)` dans une boucle suffisait à renvoyer tout le module à
l'interpréteur : l'appel de builtin n'existait pas côté JIT. Le pont est
minimal et donc nécessairement fidèle : le code émis range les arguments
(pointeurs de Valeur) sur la pile et appelle le **helper 50**,
`rose_appel_builtin`, qui appelle **la même fonction `stdlib()` que
l'interpréteur** — mêmes valeurs, mêmes erreurs, aucun duplicata.

Liste blanche (types d'arguments et de retour purement numériques, pour que
le JIT sache copier le résultat dans son slot) : `factorielle`, `fibonacci`,
`pgcd`, `ppcm`, `puissance`, `est_premier`, `compter_premiers`,
`nb_chiffres`, `hasard`, plus `abs`/`min`/`max` dont le type suit le contexte.
Les builtins qui renvoient vide ou manipulent listes et chaînes
(`joindre`, `concatener`, `longueur`, `argument`…) restent hors liste : le
module retombe à l'interpréteur, comportement inchangé d'avant.

| Bench | v3.9.0 (module non compilable) | **v3.10.0** |
|---|---:|---:|
| pgcd (2 M appels dans une boucle) | 4,43 s | **0,59 s (×7,5)** |
| factorielle | 0,21 s | 0,20 s (désormais JITé) |

Doré `t_builtin` (14 valeurs, somme de boucle 68 490 rec recoupée en Python),
suites 43/43 + 42/42 croisé qemu, parité 113/110 avec un programme builtins
neuf (les quatre moteurs d'accord), test_jit 14/14.

Suite logique (M2.14) : le même pont appliqué aux listes et chaînes —
`joindre` (résultat vide → appel sans copie), `longueur`, `concatener`,
constantes de chaîne, puis les ops `liste.*` — pour débloquer la famille
`chaines` (1,19 s) et au-delà.

## Jalon M2.14 — listes et chaînes natives dans le JIT (v3.11.0)

Le pont builtins de M2.13 s'étend aux structures : `joindre`, `concatener`,
`longueur`, `ordre`, `caractere`, `sous_chaine` passent par le même helper 50
(la stdlib reçoit les **Valeur réelles** des slots — ses shared_ptr donnent
l'aliasing d'origine par construction), et les ops RIR `liste.new`,
`liste.push`, `liste.get`, `liste.len`, `index` deviennent natives via des
helpers miroirs copiés de l'interpréteur RIR (`rose_index` est même défini
dans rose.cpp et appelé en helper 56 : code identique, pas une copie).

Détails d'émission notables :
- `liste.push` en énoncé est émis **sans destination** par l'émetteur RIR :
  l'appel seul suffit, aucun slot touché dans ce cas ;
- `joindre` renvoie vide : appelé puis libéré, le slot de destination n'est
  écrit que si le vérificateur prouve que le temp n'est jamais lu ;
- `concatener` et compagnie écrivent le **vrai type** dans le slot
  (`slot_ty = S`) quel que soit le `dest_ty` demandé par l'émetteur (U en
  position d'argument) : ce sont les lecteurs qui consultent `slot_ty` ;
- affectation depuis un slot de type inconnu (liste) = **partage du pointeur**
  — la copie shallow de l'interpréteur partageait déjà les shared_ptr
  internes, les deux moteurs ont donc exactement le même aliasing ;
- `afficher` un type non suivi statiquement (liste, table, rationnel, vide)
  appelle le helper 57 `rose_afficher_jit` = fonction `afficher()` de
  l'interpréteur : plus de repli de module pour un simple affichage de liste.

| Bench | v3.10.0 | **v3.11.0** |
|---|---:|---:|
| chaines (500 k `joindre` + concaténation 1 Mo) | 1,19 s | **0,34 s (×3,5)** |

Piège rencontré (x64) : `call_helper` attend ses arguments en P0/P1 — après
`call_helper(50)` le résultat est en RAX ; libérer sans recopier RAX→P0
libérait un registre poubelle (SIGSEGV). AArch64 l'avait gratis (X0 =
argument ET résultat). Pièges de constante : les constantes de chaîne
arrivent **sans préfixe** (`name = texte brut`, contrairement aux numériques
en « @c… »), et `-3 < 0` faisait classer `concatener` dans la branche
« dynamique » par un `else if (ty_fixe < 0)` — les cas particuliers se testent
AVANT les comparaisons d'ordre.

Doré `t_liste` (12 valeurs : new/push/get/index/len/joindre/concatener/
longueur/affectation aliasée), suites 44/44 ARM + 43/43 croisé qemu,
parité 116/116 avec un programme listes/chaînes neuf (les quatre moteurs
d'accord), test_jit 14/14, bancs sans régression.

## Jalon M3.0 — premier compilateur AOT (`rose build`) (v3.12.0-dev)

`rose comp` n'est qu'un packager (moteur + source). `rose build` est le
premier vrai compilateur AOT de Rose : il parse le source, émet le RIR,
transpile le RIR en C++, et compile avec `g++ -O3 -march=native`. Le
résultat est un exécutable natif autonome qui link le runtime Rose
(`rose_runtime.cpp` : RNG, tampons, affichage, temps).

Premier résultat mesuré sur `mot_de_passe.rose` (1 G car.) :

| Moteur | Temps |
|---|---:|
| Rust (`-O`) | ~0,38 s |
| C++ twin (`-O3 -march=native`) | ~0,32 s |
| **Rose AOT (`rose build`)** | **~0,39 s** |
| Rose JIT+interpréteur | ~0,48 s |

Rose AOT est désormais **compétitif avec Rust** (écart dans la variance
thermique). Le transpileur gère pour l'instant les types entier/chaîne/
tampon et les ops arithmétiques, appels builtins, branches, pour-boucles ;
les flottants, listes et fonctions utilisateur sont à étendre.

Fichiers neufs : `transpiler.h`, `transpiler.cpp`, `rose_runtime.h`,
`rose_runtime.cpp`. Commande : `rose build prog.rose [-o sortie]`.

## Jalon M3.2 — transpileur complet : flottants, listes, grands entiers, fonctions utilisateur (v3.12.0-dev)

`rose build` gère désormais :

- **Flottants** : `cst.f`/`add.f`/`sub.f`/`mul.f`/`div.f`/`neg.f`, affichage,
  variantes flottantes des builtins numériques ;
- **Listes et chaînes** : `liste.new`/`liste.push`/`index`/`liste.len`,
  builtins `joindre`/`concatener`/`longueur` ; type runtime `rose_liste`
  et type RIR `RirListe` ;
- **Grands entiers GMP** : type runtime `rose_gd` (`mpz_class`) et type RIR
  `RirGd`. Factorielle en arbre de produits, Fibonacci en doublement rapide
  — algorithmes **identiques à l'interprète —, plus `rose_puissance_gd` et
  `rose_nb_chiffres_gd`. Les constantes entières hors int64 sont typées
  `rose_gd` (l'émetteur RIR plie `2 ^ 30000000` en une constante géante).
  Deux rustines de bench supprimées (factorielle qui renvoyait un nombre de
  chiffres, `nb_chiffres` identité au-delà de 1000) : la sémantique AOT est
  désormais fidèle à l'interprète ;
- **Fonctions utilisateur** : `fn` émises en fonctions C++ (types de
  paramètre et de retour issus du RIR), dispatch des appels, `ret` avec
  valeur ;
- **Comparaisons et boucles** : `pour`/`tant_que` validés (gotos étiquetés).

Mesures AOT (les 10 bancs compilent, sortie identique à l'interprète) :

| Banc | AOT | Moteur JIT+interprète |
|---|---:|---:|
| appels (fonction user, 2 M itérations) | 0,002 s | 0,036 s |
| chaines (concat 500 k) | 0,019 s | 0,34 s |
| factorielle (10⁶ !, GMP) | 0,168 s | — |
| fibonacci (F(5·10⁶), 1 M chiffres) | 0,032 s | — |
| mot_de_passe (1 G car.) | 0,42–0,47 s | 0,48 s |
| puissance (2³⁰⁰⁰⁰⁰⁰⁰) | 0,296 s | — |
| pgcd / premiers / boucles | 0,001–0,103 s | — |

Suites dorées 44/44, parité 116/116, test_jit 14/14, build par défaut (-O2)
revérifié sur les bancs.

Reste côté transpileur : `div.z`/`rem.z` sur `rose_gd`, fonctions
utilisateurs sur listes/chaînes, exécution des suites dorées en AOT.

## Jalon M3.3 — fidélité sémantique AOT : les dorés parlent (v3.12.0-dev)

Nouvelle suite `moteur/tests/aot.py` : chaque doré est compilé par
`rose build`, exécuté, et comparé au doré C **caractère par caractère**.
Résultat : **9 réussis, 0 échoué, 1 limite, 0 non compilable**.

Corrections de fidélité au transpileur et au runtime :

- **Rationnels exacts** : type `rose_qd` (mpq_class), ops `cst.q`/`div.q`,
  écriture avec périodes `0.8[3]` (miroir de `ecri`), builtin `fraction`,
  comparaison exacte mixte, `cmp.approx` (≈ tolérance 1e-9 relative) ;
- **Divisions euclidiennes à la Rose** : `divf`/`rem.z` en plancher
  (`mpz_fdiv`), variantes entiers et grands entiers — le C de `%` n'y
  suffisait plus (-7 % 3 = 2) ;
- **Flottants fidèles** : `entier()` = troncature (mpz_set_d),
  `partie_entiere` = plancher, `non`/`et`/`ou` (ops `not.b`/`and.b`/`or.b`),
  affichage en représentation la plus courte qui relit juste ;
- **RNG commun** : `hasard` partage enfin l'état global de `graine`
  (le runtime avait un état privé — séquences divergentes) ;
- **Listes hétéroclites** : poussée typée (entier/grand entier/rationnel/
  flottant→texte fidèle), lecture d'élément selon le type de destination,
  affichage `[a, b, c]`, index dans les chaînes typé en chaîne ;
- **`nombre_pi`** porté (formule de Machin, 2 M de décimales max) ;
- **Grandes marges machine** : constantes |v| ≥ 2⁶² promues `rose_gd` —
  le doré `machine` (LONG_MIN ± 1, a·a) passe en exact ;
- **Inférence** : promotion des globals par contamination, `ret` avec
  valeur, appels de fonctions utilisateur, paramètres éclipsant les globals,
  comparaison des tampons par contenu.

Limite assumée et tracée : `signature` échoue sur `facto_iteratif(30)` —
accumulateur débordant int64 depuis de petites valeurs, indétectable à la
compilation statique. La promotion runtime (entier taggué ou redémarrage
spéculatif de fonction) est un jalon ultérieur ; tout le reste de la sortie
est identique.

Suites : dorés 44/44, parité 116/116, test_jit vert, les 10 bancs compilent
et sortent comme l'interprète (mot_de_passe AOT 0,46-0,47 s, stable).

## Jalon M2.15 — déboxage des entiers machine dans le JIT (v3.12.0-dev)

La vérité est sortie du dump désassemblé : depuis M2.7, les chemins « inline »
ne s'activaient **jamais**. Les vérifications lisaient les GMP à des offsets
de Valeur hypothétiques (`padding` et `_mp_alloc` au lieu de `_mp_size` et
`_mp_d`) : le test « taille == +1 » échouait toujours et **toutes** les
itérations retombaient sur les appels helper — les dorés restaient verts, le
gain mesuré à l'époque (M2.8) venait en réalité de la seule fusion. Pire : si
le padding d'un objet recyclé avait valu 1, le chemin calculait des pointeurs
au lieu des valeurs (bombe à retardement désamorcée au passage).

M2.15 répare et active le déboxage : les offsets sont **calculés** par
`offsetof` sur `__mpz_struct` (`_mp_alloc`+8, `_mp_size`+12, `_mp_d`+16 dans
le `Valeur`), et l'arithmétique `add/sub/mul` comme les comparaisons
`cmp.*.z` travaillent l'entier machine **en place** dans le mpz embarqué :
chargement du membre via `_mp_d` avec vérification (positif d'un membre,
< 2^63), addition/soustraction/multiplication natives, détection de
débordement (`b.vs`/`jo`, `smulh`), écriture en place du membre et de la
taille, capacité `_mp_alloc` vérifiée avant écriture. Tout le reste —
négatifs, multi-membres, débordements, allocation — retombe sur les **mêmes
helpers** qu'avant : sémantique identique, dorés inchangés, y compris
`machine` (LONG_MIN ± 1, a·a exacts par repli GMP). Miroir x86-64 à l'identique.

| Programme (AArch64, JIT) | helpers | **inline** |
|---|---:|---:|
| boucle entière | 0,073 s | **0,035 s** (×2,1) |
| boucle flottante (compteur entier) | 0,027 s | **0,010 s** (×2,7) |
| appels ×2 M | 0,035 s | **0,026 s** (×1,35) |
| factorielle | 0,186 s | **0,172 s** |
| premiers | 0,057 s | **0,052 s** |
| pgcd / mot_de_passe / chaînes | neutre | (dominé helpers) |

Même binaire, A/B par `ROSE_NO_INLINE=1` (garde de mesure conservée).
Trempage de vérification : débordements répétés (2⁷⁰), LONG_MAX+1,
LONG_MAX², LONG_MIN−1, accumulateur traversant 2⁶³ — sortie identique au
calcul exact Python et au mode sans inline. Suites : dorés 44/44, parité
116/116, test_jit vert, suite croisée x86-64 sous qemu. Le build croisé
s'est vu ajouter les TU transpileur/runtime (rose.cpp référence
`transpiler_rir` depuis M3 — `build-x64.sh` était resté à trois TU).

Accessoire de livraison : `rose build` ne connaît plus aucun chemin
absolu — le runtime est cherché à côté de l'exécutable (`ROSE_HOME` en
override), le compilateur, les drapeaux et GMP se pilotent par
`ROSE_CXX`, `ROSE_CXXFLAGS`, `ROSE_GMP`. Vérifié avec le binaire déplacé
dans un répertoire témoin.

## Jalon M2.16 — chemin machine des builtins numériques (v3.12.0-dev)

Le pont JIT → `rose_appel_builtin` coûtait **~140 ns par argument recopié**
(vector de `Valeur` profonds, mpz à chaque passage) plus ~200 ns de base
(construction de nom, distribution, allocation du résultat) : une boucle
`min($k, 3)` sur 2 M d'itérations valait 0,97 s contre 0,014 s sans appel.

`rose_appel_builtin` tente d'abord le **chemin machine int64** pour
`abs`, `min`, `max`, `pgcd`, `ppcm`, `puissance`, `factorielle`,
`fibonacci`, `nb_chiffres` quand les arguments sont des entiers tenant en
int64 : Euclide machine, exponentiation par carrés avec
`__builtin_mul_overflow`, boucles factorielle/fibonacci à additions
vérifiées, chiffres par divisions décimales (128 bits pour |LONG_MIN|).
Tout cas de sortie du domaine — grand entier, `LONG_MIN` sur abs/pgcd/ppcm,
débordement d'un seul produit, `21!`, `fib(93)`, exposant négatif, types
mixtes — **retombe sur le chemin générique exact** (mêmes codes stdlib) :
le résultat ne dépend jamais du chemin.

| Banc / micro (2 M d'appels, AArch64) | avant | **après** |
|---|---:|---:|
| min (micro) | 0,973 s | **0,083 s** (×11,7) |
| abs (micro) | 0,355 s | **0,079 s** |
| pgcd | 0,577 s | **0,180 s** (×3,2) |
| factorielle / fibonacci / puissance | inchangés (hors domaine, repli exact) | |

Trempage de fidélité : 34 valeurs limites (abs(LONG_MIN), pgcd(LONG_MIN,·),
ppcm(2⁶²,4), 2¹⁰⁰⁰-ish via 10^40, min/max mixtes gd, 0!, fib(0..93),
puissance(0,0), puissances négatives → rationnel…) — sortie **identique au
calcul exact Python**. Dorés 44/44 (dont t_builtin qui brasse les deux
chemins), parité 116/116, test_jit, AOT 9/10+1 limite.

### M2.16 bis — dispatch AOT grands entiers et fidélité `nb_chiffres`

Le programme-témoin `fastb.rose` (34 valeurs limites) refusait de compiler
en AOT (`rose_min(rose_int)` recevant un grand entier) : le transpileur
reçoit des variantes `rose_pgcd_gd`/`rose_ppcm_gd`/`rose_abs_gd`/
`rose_min_gd`/`rose_max_gd` (mpz exacts, arguments int64 convertis),
`séty_of` reconnaît enfin les constantes entières promues (corrige du même
coup les dispatch `cmp`/`divf`/`rem` sur ces constantes), et les deux
inféreurs propagent `gd` à travers les appels.

Le tiers-voie a aussi démasqué un écart de fidélité : l'interprète
compte les chiffres avec `mpz_sizeinbase` (GMP rend ±1 près de la
vérité : `nb_chiffres(10¹⁸−1) = 19`, `nb_chiffres(−2⁶³) = 20`). La boucle
à vrais chiffres du runtime AOT et le chemin rapide M2.16 divergeaient
tous les deux — or la sémantique de référence **est** l'interprète.
`nb_chiffres` AOT (entier et grand entier) imite maintenant littéralement
l'interprète (abs puis sizeinbase) et `nb_chiffres` sort du chemin rapide
JIT (retour au stdlib exact). `fastb.rose` : interprète = JIT = AOT sur
les 34 lignes ; dorés 44/44, parité 116/116, AOT 9/10+1 limite.

### M2.16 ter — croisée x86-64 verte, REX.R et outils d'audit

Le test croisé `test_jit-x64` (sous qemu/TCG) a démasqué un défaut
d'émission x64 introduced avec M2.15 : le `test r,r` du chemin inline
(débordement de l'addition) encodait REX avec le seul bit B pour
r8..r15, produisant `test r8, rax` au lieu de `test r8, r8` — ZF ne
regardait plus la somme, l'instruction `cmove` qui en dépend écrasait le
résultat, et `boucle.somme` tournait en boucle infinie. Le `lea r, [rsp]`
souffrait du même champ reg sans REX.R. Corrigés (REX |= R pour le champ
reg) ; le backend AArch64 n'a pas d'équivalent (test et lea n'y ont pas de
champ reg décalé). Leçon : un backend non natif ne devient « validé » que
sous émulation — le doré de boucle l'a attrapé là où l'empreinte de
boucle_entier sur ARM était restée verte.

Outils d'audit ajoutés (sans effet sur les moteurs) :
- `bench/bench_stat.py` — chauffe + N mesurées, min/médiane/p10/p90/étal.
  et vérification d'empreinte : le dispersion ARM sur les bancs courts est
  de 5 à 18 %, le seul min est trompeur.
- `tests/aot.py` réécrit en **couverture sémantique** (35 cas : dorés,
  stdin, argv, fichiers, quitter, erreurs, rejets) avec catalogue
  d'écarts connus (fidélité des erreurs/effects AOT = jalon M3.3bis) ;
  un échec hors catalogue est une régression bloquante.
- `ARCHITECTURE.md` : section hypothèses structurales (offsets GMP via
  offsetof, règles de propriété des Valeur, ABI/plateformes prouvées ou
  non).

Croisée finale : test_jit-x64 14/14, dorés 43/43 sous qemu. AOT : 11
fidèles + 1 rejet sur 35 cas, 1 limite, 5 non compilables, 17 écarts
catalogués, 0 régression.

### M3.3 bis — fidélité des erreurs et des effets AOT (audit)

La couverture sémantique mesurée (35 cas) a catalogué 17 écarts : presque
tous venaient de deux trous structurels. Côté compilable, `rose build`
laissait fuir les `ErreurRose` du front-end en « terminate called » — le
try/catch du mode `comp` est appliqué au mode `build` : rejet poli, même
message que l'interprète, code 1 (8 cas). Côté runtime AOT, les builtins
hors liste n'étaient pas émis du tout (commentaire dans le C++ généré,
valeur par défaut silencieuse) — le pire kind d'écart, désormais comblé :

- effets du monde : `lire` (stdin typé entier/flottant/rationnel/chaîne,
  conversions miroir de l'interprète, « tolérant » = troncature vers zéro),
  `quitter` (exit réel), `lire_texte`/`ecrire_texte` (erreurs avec chemin) ;
- chaînes UTF-8 : `longueur` compte des caractères (plus d'octets — la
  régression `v3_chaines` l'a justement démasquée), `sous_chaine` et
  l'index découpent en caractères, `caractere` avec contrôles de plage ;
- erreurs runtime aux messages exacts de l'interprète, format unifié
  « Rose : erreur — … » : ordre mono-caractère, bornes de `sous_chaine`,
  index hors liste/chaîne, `entier("abc")`, factorielle négative,
  « modulo par zéro », « division euclidienne par zéro », « division par zéro ».

Mesure après correction : **15 fidèles + 16 rejets corrects sur 35 cas,
0 écart catalogué, 0 régression**. Restent 3 lacunes de fonctionnalités
non compilables (notation symbolique, mixtes rationnel⊛flottant, tables)
et 1 limite assumée (`signature`, promotion overflow→GMP = M3.4).
Croisée x86-64 finale sous qemu : test_jit complet + dorés 43/43.
Bancs inchangés (empreintes 10/10) : la fidélité n'a rien coûté au JIT.

### M3.4 — rose_ent : l'entier AOT ne déborde plus

Jusqu'ici les slots entiers d'un binaire AOT étaient de simples `long long` :
`acc ⋅ i` qui dépasse 2⁶³ rendait un résultat faux en silence — c'était la
limite `signature`. Les slots deviennent **`rose_ent`**, la copie AOT de
l'entier de l'interprète : mot machine tant que ça tient, promotion GMP dès
l'overflow, rétrogradation quand un résultat revient dans int64. Les
opérateurs `+ − × < ==` sont surchargés avec garde machine (additions en
`__int128`, comparaisons directes) — l'émission brute du transpileur ne
change pas d'un octet.

Le coût réel est dans les angles, pas dans l'arithmétique :

- **`rose_int` est l'alias de `int64_t`** (= `long` ici) : un wrapper
  `f(rose_ent)` qui rappelle `f((rose_int)x)` se rappelle LUI-MÊME à l'infini.
  Premier symptôme : `rose_nombre_pi` en récursion jusqu'au SEGV (PAC masque
  le débordement de pile en faute de segment). Les surcharges de domaine
  primitif passent désormais par des types distincts.
- **`rose_ent + int` était ambigu** (constructeur `rose_ent(int64_t)` contre
  constructeur `mpz(long)` : deux conversions utilisateur de même rang).
  Surcharges exactes `(rose_ent, long long)` — et l'interdiction stricte
  d'un constructeur `rose_ent(double)`, qui rendrait tout littéral ambigu.
- **`double × rose_ent`** : avec surcharges `(long long, rose_ent)`, g++
  tronque le double avant la multiplication (2 au lieu de 3 dans
  `t_flottant`). La conversion vers `double` est **explicite**, et le
  transpileur enrobe lui-même les opérandes entiers des opérations
  flottantes, comparaisons, paramètres et retours (`rose_vers_double`).
- **`gd = ent`** : la conversion mpz implicite réveillait le machinery de
  templates de gmpxx (ambiguïtés, UB). Conversion explicite + enrobage
  `rose_vers_mpz(...)` par le transpileur (affectations, arguments gd,
  retours, destination d'appel).
- **Récursion profonde** : les cadres portant des `rose_ent`, le golden
  `machine` (`it(100000)`) déborde la pile de 8 Mo. Le transpileur détecte
  les cycles d'appels et `main` lève alors sa propre pile (512 Mo,
  `setrlimit` + `execv` de soi-même, avant toute sortie). Les programmes
  non récursifs ne paient rien.

Mesures. Bancs JIT : empreintes 10/10 inchangées (M3.4 ne touche pas la
voiture rapide). Bancs **AOT** (build -O3 -march=native, médiane N=3,
chauffe 1) : pgcd 0,112 (JIT 0,174, ×1,6), puissance 0,286 (JIT 0,916,
×3,2), mot_de_passe 0,462 (JIT 0,483), factorielle 0,168, fibonacci 0,017,
premiers 0,053 — empreintes toutes exactes. Coût payé : boucle_entier
0,103 contre 0,035 en JIT (garde d'overflow dans la boucle la plus serrée)
et appels 0,058 contre 0,024 ; c'est le prix de la correction, le JIT reste
le chemin de vitesse.

Mesure de couverture : `aot.py` passe à **16 fidèles + 16 rejets corrects
sur 35 cas, 0 limite, 0 écart catalogué, 0 régression** — `signature` et
`machine` sont devenus fidèles. Restent trois lacunes de fonctionnalité
non compilables : notation symbolique, mixtes rationnel⊛flottant, tables.
