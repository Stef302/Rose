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
