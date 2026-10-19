# Architecture — de cinq moteurs à l'auto-hébergement

## Le principe

Un langage n'a qu'**un moteur produit**. Les autres implémentations ne
disparaissent pas : elles sont **affectées à des tâches** (oracles,
spécification vivante, amorce de bootstrap) puis sorties de la
livraison. C'est exactement le chemin de Rust : un compilateur OCaml
stage0 a compilé le premier rustc écrit en Rust, qui s'est recompilé
lui-même — et OCaml a disparu du produit. Le moteur C++ de Rose joue
le rôle d'OCaml : l'**amorce**.

## Répartition des tâches (M0 — effet immédiat)

| moteur | tâche affectée | livré dans `bin/` ? |
|--------|----------------|---------------------|
| **C++**  | **MOTEUR OFFICIEL** — référence sémantique, futur hôte de l'IR, du JIT et du backend LLVM | **oui** (`rose`, `rose-win64.exe`) |
| Rust   | **ORACLE** — seconde implémentation indépendante (vérificateur, bundle, CLI) ; la parité prouve que le moteur officiel ne dérive pas | non (outil de test, reconstruit par `tester.sh`) |
| Zig    | **ORACLE** — évaluation seule ; attrape les biais de structure de données des deux autres | non (idem) |
| Python | **SPÉCIFICATION VIVANTE** — la sémantique lisible + producteur des dorés v2 | non (déjà outil) |
| Julia  | **ORACLE exact** — types arithmétiques natifs, sans code C | non (déjà outil) |

La parité (113 comparaisons, 0 divergence) devient le **contrat de
non-régression du moteur officiel** : les oracles vivent dans
`moteur/`, ils ne sont plus présentés comme des produits alternatives.
Message utilisateur : « un langage, un moteur, des validateurs ».

## Feuille de route du moteur officiel

### M1 — IR : le centre de gravité
Tout passe par une représentation intermédiaire unique, **RIR**
(spécifique dans `moteur/IR.md`) : vérificateur → RIR → (interpréteur
RIR | JIT | LLVM). L'interpréteur d'arbre actuel devient le repli de
débug ; la parité et les 147 cas v2 restent le filet.

### M2 — JIT natif (vitesse interactive + bundles)
Backend JIT émettant du code machine x86-64 et AArch64. asmjit fut le
candidat C++ pressenti (bibliothèque d'assemblage, ~300 Ko), mais son
API v1.24 a divergé des notes initiales : le moteur émet donc les
opcodes **directement** (deux petits assembleurs intégrés, zéro
dépendance externe — le binaire autonome reste petit). Les appels GMP
deviennent des appels C directs depuis le code émis ; le bundle
`rose comp` gagne un mode natif compilé.

> **État M2 (v3.5.0)** : JIT opérationnel sur **deux architectures**, sans
> asmjit — les assembleurs sont intégrés au moteur : `moteur/cpp/src/jit.cpp`
> émet les opcodes AArch64, `moteur/cpp/src/jit_x64.cpp` les octets x86-64
> (même table de helpers C++, mêmes slots de frame, mêmes règles de
> porte ; asmjit abandonné, son API v1.24 ayant divergé). Supporté :
> entiers (arithmétique complète GMP, grande précision), booléens,
> comparaisons, branches et boucles (`si/tant_que/pour`), variables,
> constantes **partagées par module** (une `Valeur` créée à la
> compilation, le prologue ne pose qu'un pointeur — zéro allocation par
> appel), fonctions utilisateur avec récursion. Les opérations hors
> support (chaînes, rationnels, flottants, tableaux, FFI, `lire`)
> basculent **le module entier** sur l'interpréteur RIR — repli
> transparent, sorties identiques (parité vérifiée sur les 8 programmes
> de bench + suites). Mesures AArch64 : boucle entière 10 M = **0,21 s
> en JIT** contre 12,2 s en interpréteur ; 4 M d'appels récursifs
> 1,55 s. x86-64 validé **de bout en bout sous qemu-x86_64** (build
> croisé sans root par `moteur/build-x64.sh`, suite 36/36 avec JIT,
> ×42 vs interpréteur même sous émulation — détails dans `PERF.md`).
> Reste : mode natif compilé pour le bundle `rose comp`, chaîne de
> production Windows x86-64.

### M3 — AOT LLVM : `rose build`
Backend **LLVM** (API C) pour la compilation native complète :
`rose build prog.rose -o prog` produit un binaire sans interpréteur,
optimisé (inlining, arithmétique vectorisée). LLVM n'est utilisé
qu'**au moment de compiler** : le binaire produit n'embarque que la
petite runtime Rose + GMP → toujours autonome. Le moteur lui-même
grossit (LLVM statique) — c'est un outil de développement, pas le
programme compilé.

### M4 — Auto-hébergement
Écrire le compilateur **en Rose** (`rose/compilateur.rose` : lexeur,
analyseur, vérificateur, émission RIR). Protocole de bootstrap en
cinq étapes, chacune prouvable :

1. le moteur C++ (amorce) exécute `compilateur.rose` → produit `rose-1` ;
2. `rose-1` compile `compilateur.rose` → `rose-2` ;
3. **point fixe** : `rose-2` byte-identique à `rose-1` (ou sémantiquement
   équivalent + mêmes sorties sur les dorés) → le compilateur se
   maintient lui-même ;
4. le moteur C++ régresse au rôle d'**amorce de bootstrap** (comme
   stage0 avant, plus personne ne le développe d'abord) ;
5. le JIT/LLVM passent dans le compilateur auto-hébergé à son tour.

### M5 — Standard library en Rose
`prelude.rose` (listes, tri, chaînes, tables, tests unitaires écrits
en Rose) compilé par le moteur auto-hébergé : le langage se décrit
et se teste par lui-même de bout en bout.

## JIT ou LLVM ? Les deux, mais pas au même étage

| critère | JIT (asmjit) | backend LLVM |
|---------|--------------|--------------|
| qualité du code généré | bonne (spécialisation de traces à la LuaJIT possible ensuite) | maximale (optimiseurs mondiaux) |
| taille du moteur | +300 Ko | +~100 Mo statique (ou lié au système) |
| architectures | x86-64, AArch64 | tout ce que LLVM connaît |
| usage | REPL, `run`, bundles petits | `rose build` publication native |
| risque | à écrire/vérifier | lourdeur de build, mais éprouvé |

L'IR commune (M1) rend les deux non exclusifs — c'est l'ordre qui
change : JIT d'abord (M2) car il garde le binaire autonome et sert le
bootstrap ; LLVM ensuite (M3) pour la puissance de publication.

## Audit de langage : ce qu'il manque à Rose pour écrire son compilateur

**Fait (v3)** — le noyau de contrôle était déjà là (fonctions récursives,
`tant_que`, `pour`, listes, chaînes concaténables, `joindre`, appel nu en
instruction). Six briques sont arrivées, chacune validée par les cinq
moteurs (parité) puis spécifiée dans `rose2/SPEC.md` (section v3) :

1. **indexation de chaînes** `s[i]` en caractères Unicode ✅ ;
2. **codes de caractères** `ordre(...)` / `caractere(...)` ✅ ;
3. **constructeur linéaire** `concatener(liste_de_chaines)` ✅ ;
4. **E/S fichiers** `lire_texte` / `ecrire_texte` ✅ ;
5. **arguments CLI** `nb_arguments()` / `argument(i)` ✅ ;
6. **sortie d'erreur** `afficher_erreur(...)` / `quitter(code)` ✅.

**Fait (v3.2)** — **notation symbolique** (`⋅ × ÷ ≤ ≥ ≠ ≈ √ ² ³ π ← →`
au lexeur, table identique sur les cinq moteurs) et **arithmétique mixte
rationnel⊛flottant unifiée** : repli de constantes ≡ exécution
(`1/4 + 0.5` → `0.75` partout), modulo réservé aux entiers, exposants
rationnels tronqués — spécifiée dans `rose2/SPEC.md`, dorés `t_symboles`
et `t_mixtes`, parité **113 comparaisons, 0 divergence** ✅. Mesures de
vitesse des trois moteurs natifs : `PERF.md`.

**Fait (v3.1)** — **tables** : `table()`, `$t[cle]`, `inserer`, `cle_existe`,
`supprimer`, `cles`, `valeurs`, `longueur`, affichage `["a" => 1]`,
égalité indépendante de l'ordre, tables imbriquées — spécifiée dans
`rose2/SPEC.md`, doré `t_tables`, parité **117 comparaisons, 0 divergence**
✅.

**Fait (v3.3)** — **tranches** `s[a:b]`, `s[:b]`, `s[a:]`, `s[:]`
(sucre syntaxique pour `sous_chaine`, mêmes bornes en caractères) —
spécifiée dans `rose2/SPEC.md` ✅.

**Fait (M1a+M1b)** — **IR RIR** : structures (types, opérandes, instructions,
blocs, fonctions, module), émetteur AST→RIR, dump texte canonique
(`rose rir prog.rose`), interpréteur RIR fonctionnel (`rose rir-run
prog.rose`) — vérifié sur `fact(5)` → 120, slices, tables, boucles,
récursion. Le moteur a désormais **deux chemins d'exécution** (arbre +
RIR). Les dorés d'IR sont la prochaine étape (M1c).

**Fait (M1c)** : dorés d'IR (`rirs/`, 23 cas, `rose rir` → texte
canonique comparé). **Fait (M1d)** : le moteur n'exécute plus que le
RIR (l'arbre n'intervient plus qu'avant l'émission). **Fait (M1e)** :
le repli RIR couvre le langage entier — FFI `@dll/@sym` et
`dynamique::` (partagée avec l'arbre par `ffi_appeler`), lecture typée
`<<`, comparaisons `≈ / ?= / =~` (`cmp.approx`), `et/ou`, division
exacte `/` contre euclidienne `div` (conforme à IR.md), disposition des
blocs par curseur (la suite d'un `si` dans une boucle est atteignable) —
la suite moteur est **verte (37/37)**. **Fait (M2)** : JIT en
production sur AArch64 **et** x86-64 (encodeurs intégrés `jit.cpp` /
`jit_x64.cpp`, table de helpers commune ; voir l'encadré « État M2 »
ci-dessus) ; le bundle `rose comp` en mode natif compilé reste à
faire. Ces briques sont propres aux moteurs : les cibles d'export
`rose2` les refusent avec un message explicite (la chaîne d'export
reste l'outil secondaire).

## Ce que le plan ne casse pas

- Le contrat « binaire autonome, aucune dépendance d'exécution » est
  renforcé : JIT = binaire petit ; LLVM = dépendance du compilateur,
  jamais du programme compilé.
- Les dorés, la parité et les suites restent la vérité pendant toute
  la migration : à chaque jalon, `tester.sh` (et `tester.ps1` sur
  Windows) doit être vert.
