# Rose — moteurs natifs (binaires autonomes)

`rose` est **LE** compilateur-interpréteur du langage Rose : un binaire
unique qui embarque tout le moteur — lexeur, parseur, vérificateur de
types, repli de constantes exactes, interpréteur, bibliothèque
mathématique (GMP liée **statiquement** : entiers et rationnels sans
limite) et la FFI (`dlopen`).

**Pour exécuter un programme Rose, il ne faut rien d'autre que ce
binaire.** Ni Python, ni rustc, ni zig, ni julia, ni gcc. Le langage
d'écriture n'est qu'une fondation de construction, jamais une dépendance
d'exécution.

> **Répartition des rôles** (`../ARCHITECTURE.md`) : le moteur C++ est
> le produit officiel et le futur hôte de l'IR/JIT/LLVM ; Rust et Zig
> sont des **oracles** (secondes implémentations qui prouvent la
> parité), Python la **spécification vivante**, Julia l'**oracle
> exact**. L'IR commune qui unifiera les backends est spécifiée dans
> [`IR.md`](IR.md).

## Les moteurs

Le même langage, la même sémantique, les mêmes messages — validés
**identiques au caractère près** (`tests/parite.py`, 117 comparaisons, 0
divergence). Le moteur C++ est la référence ; chaque autre moteur doit
s'y conformer.

| moteur | dossier | nature | vérificateur | FFI `@dll`/`dynamique::` | `rose comp` (bundle) |
|--------|---------|--------|--------------|--------------------------|----------------------|
| **C++** (référence) | `cpp/` | binaire autonome, GMP statique | ✓ | ✓ | ✓ |
| **Rust** | `rust/` | binaire autonome, GMP statique (bindings `extern`) | ✓ | ✓ | ✓ (interopérable avec C++) |
| **Zig** | `zig/` | binaire autonome, GMP liée (`@extern`) | — (erreurs à l'exécution) | — (message d'erreur clair) | — |
| **Python** | `python/` | moteur vivant (python3 seul) | via v2 | ✓ (ctypes) | — |
| **Julia** | `julia/` | moteur vivant pur Julia | — (erreurs à l'exécution) | ✓ (ccall) | — |

Les binaires validés sont livrés dans `../bin/` :
`rose` (C++), `rose-rs` (Rust), `rose-zig` (Zig) pour Linux, et
`rose-win64.exe` / `rose-rs-win64.exe` / `rose-zig-win64.exe` pour
Windows x64 — ces derniers ne dépendent que de l'UCRT, fournie par
Windows 10/11 eux-mêmes (`../tester.ps1` les valide sur place).

Le moteur Zig applique la même grammaire, la même évaluation exacte
(GMP), les mêmes messages runtime ; il lève ses erreurs syntaxiques à
l'exécution plutôt qu'à la compilation et refuse proprement les modules
dynamiques. Les moteurs vivants Python/Julia évaluent les constants au
sens runtime (sans repli de constantes flottantes « texte → strtod ») :
seul un cas pathological comme `>> -8.0 ^ (1/3)` diffère (le repli C++
lit le préfixe « 1/3 » en 1.0 ; le runtime donne la valeur vraie, `non_nombre`).

## Utilisation

```console
$ rose prog.rose                    # interpréter (mode par défaut)
$ rose run prog.rose                # identique
$ rose comp prog.rose -o prog       # produire un exécutable autonome (C++/Rust)
$ rose info                         # état du moteur et des dépendances
$ rose version
```

`rose comp` colle le programme dans une copie du moteur (format
`moteur + source + taille + magique ROSEB1`) : le fichier produit est un
seul exécutable, dépendant uniquement de la libc, déplaçable sur toute
machine de même architecture. Les binaires C++ et Rust produisent des
bundles interopérables (un bundle fait par `rose` tourne avec `rose-rs`
comme moteur, et réciproquement).

## Modes

| commande              | rôle                                             | dépendances |
|-----------------------|--------------------------------------------------|-------------|
| interpréteur          | `rose prog.rose` — exécution immédiate           | aucune      |
| compilateur           | `rose comp prog.rose` — binaire autonome         | aucune      |
| export polyglotte     | `rose2/bin/rose run prog.rose --cible X` (C, Python, Julia, Rust, Zig) | toolchain de la cible |

L'export polyglotte (chaîne v2, dans `../rose2/`) reste disponible pour
publier du code dans d'autres langages ; le moteur natif est le mode
principal : il offre le même langage, les mêmes valeurs exactes, et
s'exécute sans aucune installation.

## Construction

```console
$ make                              # cpp/ : g++ + GMP (headers + .a), GMP_DIR surchargeable
$ cd rust && ./build.sh             # rustc seul ; GMP liée statiquement (bindings extern)
$ cd zig  && zig build-exe src/main.zig -O ReleaseSafe -lc \
      <GMP>/lib/libgmp.a -lm -femit-bin=rose     # zig 0.14 + GMP
$ python3 python/engine.py prog.rose             # moteur vivant Python
$ julia julia/engine.jl prog.rose                # moteur vivant Julia
```

Depuis l'environnement de développement :
`GMP_DIR=/tmp/opencode/toolchain/gmp-install make`.

### Versions Windows x64 (compilation croisée)

`build-win64.sh` rejoue la chaîne complète : GMP croisé compilé par Zig
(avec ses cache-vars COFF), archive COFF reconstruite au `llvm-ar`
(l'index « ar » GNU du build croisé est refusé par lld-link), puis les
trois moteurs — C++ et Zig nativement par Zig, Rust via la cible
`x86_64-pc-windows-gnullvm` liée par le même Zig.

Un piège d'ABI est verrouillé par le code : sous Windows, `long` fait
**32 bits** (LLP64). Les conversions entier↔64 bits passent donc par
`mpz_import`/`mpz_export` d'un mot de 8 octets (jamais
`mpz_get_si`/`mpz_set_ui`, qui tronqueraient silencieusement), et les
`mpz_*_ui` à exposant 64 bits ont un repli strictement équivalent
(élévation par carrés, doublage rapide pour `fibonacci`). C'est ce qui
garde les sorties identiques au caractère entre Linux et Windows.

## Validation

- `tests/courir.py` — suite moteur (37 cas : dorés t_exact/t_machine/
  signature/alias, FFI, bundle, erreurs, CLI). Moteur testé par
  `ROSE_MOTEUR=<chemin>` (C++ et Rust : 37/37).
- `tests/parite.py` — comparateur au caractère contre le C++ sur 21
  programmes auto-portés (arithmétique exacte, périodiques, %g, stdlib,
  récursion, listes, symboles, mixtes, erreurs runtime…). Rust, Zig,
  Python, Julia : **PARFAIT** (117 comparaisons, 0 divergence).
- `tests/bench.sh` — mesure de performance des moteurs sur les mêmes
  programmes (`tests/bench/`) ; résultats dans `../PERF.md`.

```console
$ python3 tests/courir.py
Tests moteur : 37 réussis, 0 échoués
$ python3 tests/parite.py
— rust    : PARFAIT
— zig     : PARFAIT
— python  : PARFAIT
— julia   : PARFAIT
```

## Organisation

    cpp/src/rose.cpp      moteur complet (un seul fichier, Makefile statique)
    rust/src/*.rs         moteur jumeau (ffi/float/front/run/main)
    zig/src/main.zig      moteur Zig (un fichier, GMP @extern)
    python/engine.py      moteur vivant Python 3 (stdlib seule)
    julia/engine.jl       moteur vivant Julia (aucune dépendance externe)
    tests/courir.py       suite moteur 37 cas
    tests/parite.py       comparateur multi-moteurs
    tests/bench.sh        mesure de performance (avec tests/bench/)

## Notes de comportement (alignées sur SPEC.md)

- Affichage des flottants : aller‑retour **le plus court fidèle**
  (17 chiffres si nécessaire) — un flottant affiché se relit exactement.
- `partie_entiere(x)` = **plancher** ⌊x⌋ partout (‑3 pour ‑2.5).
- Affichage d'un rationnel négatif : partie entière en plancher, chiffres
  sur |numérateur| (`-1/3` → `-1.[3]`, `-7/2` → `-4.5`) — convention de la
  référence, identique sur les cinq moteurs.
- `chaine(x)` rend exactement le texte que `>> x` afficherait.
- Fraction lue dans une variable entière (`<< $n` avec `7/2`) :
  troncature vers zéro → `3` ; dans une variable `rationnel` : valeur
  exacte.
- `^` avec un exposant rationnel et une base flottante : `pow()` libm
  (l'exposant n'est pas tronqué).
- Entier à puissance entière négative (exposant variable négatif) :
  résultat **rationnel exact** (0.5 pour `2 ^ -$e`), comme la cible
  Python ; la cible C, à typage statique, refuse ce cas à l'exécution.
- `≈` / `?=` / `=~` : tolérance relative 1e‑9 quand un flottant est
  impliqué, égalité exacte sinon (comme les autres cibles).
- Récursion profonde : les moteurs autonomes tournent sur une pile
  dédiée de 512 Mo (100 000 niveaux vérifiés). Le moteur Julia compte sur
  la pile du processus : lancer `ulimit -s 524288` avant un programme
  très récursif.
