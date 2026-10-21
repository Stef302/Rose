# Tester Rose sur votre machine

## En une commande

```console
$ ./tester.sh
```

Le script :
1. teste les trois binaires autonomes livrés dans `bin/` ;
2. les **reconstruit automatiquement** s'ils sont inexécutables ici
   (les binaires livrés ont été compilés sur une machine ARM64 — une
   machine x86-64 doit les recompiler, c'est normal et prévu) ;
3. joue les trois suites de validation et annonce *« Tout est vert »*
   ou liste précisément ce qui manque.

Résultat attendu (tous les outils présents) :

```
── Moteurs autonomes ──────────────────────────────────
  ✓ C++   : Rose 3.14.0 — moteur natif C++/GMP
  ✓ Rust  : Rose 3.14.0 — moteur natif Rust/GMP
  ✓ Zig   : Rose 3.14.0 — moteur natif Zig/GMP
── Moteurs vivants ────────────────────────────────────
  ✓ Python : Rose 3.14.0 — moteur Python 3 (int/Fraction/float64)
  ✓ Julia  : Rose 2.0.0 — moteur Julia (BigInt / Rational exacts)
── Suites de validation ───────────────────────────────
  ✓ suite moteur (moteur C++) : Tests moteur : 25 réussis, 0 échoués
  ✓ suite moteur (Rust)       : Tests moteur : 25 réussis, 0 échoués
  ✓ parité des moteurs        : Parité : 89 comparaisons, 0 divergences
  ✓ suite v2 (147 cas)        : Tests : 147 réussis, 0 échoués

Tout est vert — Rose est opérationnel sur cette machine.
```

Chaque suite est autonome : les programmes de test et les dorés
(résultats attendus, produits par la référence) vivent dans le dépôt
(`moteur/tests/dorés/`), rien n'est récupéré sur Internet.

## Sur Windows 10/11 (x64) — en une commande

```powershell
PS> powershell -ExecutionPolicy Bypass -File .\tester.ps1
```

`bin/` contient trois exécutables **Windows x64 autonomes**, compilés
depuis ce dépôt, chacun dans son langage fondateur :

| fichier | moteur | contenu |
|---------|--------|---------|
| `bin\rose-win64.exe`     | C++  | vérificateur + FFI + `comp` (bundle) + GMP lié statiquement |
| `bin\rose-rs-win64.exe`  | Rust | vérificateur + FFI + `comp` + GMP |
| `bin\rose-zig-win64.exe` | Zig  | interpréteur complet + GMP |

**Aucune installation** : leurs seules dépendances sont `KERNEL32.dll`
et l'UCRT (`api-ms-win-crt-*.dll`), fournies par Windows 10/11 eux-
mêmes. `tester.ps1` rejoue les dorés **octet par octet**, les onze
« roses à tester » de `moteur\tests\roses\` (symboles, mixtes, tables,
puissances, boucles, chaînes UTF-8, grands entiers, flottants, récursion,
erreurs runtime — voir `MANIFEST.md`), le CRLF des
fichiers édités sous Windows, la récursion profonde, le bundle
`comp → .exe` et la FFI vers `msvcrt.dll` — et annonce la couleur.

Usage quotidien :

```powershell
PS> .\bin\rose-win64.exe prog.rose                     # interpréter
PS> .\bin\rose-win64.exe comp prog.rose                # → prog.exe autonome
PS> .\prog.exe                                         # seul, sans rien d'autre
PS> .\bin\rose-win64.exe version
```

Remarques Windows :

- Les exécutables ne sont pas signés numériquement : SmartScreen ou
  l'antivirus peuvent demander « Exécuter quand même » à la première
  ouverture (c'est le sort de tout binaire compilé soi-même).
- Windows 32 bits (x86 pur) n'est pas visé — tout Windows récent est
  x64.
- Console : le moteur bascule la console en UTF-8 (accents et notation
  périodique `0.[3]` s'affichent correctement).
- Les moteurs « vivants » Python et Julia fonctionnent aussi sous
  Windows si vous installez Python 3 / Julia vous-mêmes :
  `python moteur\python\engine.py prog.rose`,
  `julia moteur\julia\engine.jl prog.rose`.
- Pour **recompiler** les `.exe` vous-mêmes (facultatif) :
  `moteur/build-win64.sh` documente la chaîne croisée (Zig + GMP
  Windows + cible Rust gnullvm) ; elle tourne sous Linux ou MSYS2.

## Utiliser Rose directement

```console
$ bin/rose prog.rose                      # interpréter
$ bin/rose comp prog.rose -o prog         # binaire autonome (programme scellé dans le moteur)
$ ./prog                                  # tourne sans rien d'autre que la libc
$ bin/rose info                           # moteur + dépendances vues
```

`bin/rose` (C++), `bin/rose-rs` (Rust), `bin/rose-zig` (Zig) sont le
**même langage, la même sémantique, les mêmes messages** — vérifiés au
caractère près par `moteur/tests/parite.py`. Le moteur C++ est la
référence ; le vérificateur de types, la FFI (`@dll`, `dynamique::`) et
`rose comp` (bundle autonome) existent sur C++ et Rust.

## Prérequis de reconstruction (uniquement si les binaires livrés ne
## tournent pas sur votre architecture)

| moteur | outil | GMP | remarque |
|--------|-------|-----|----------|
| C++    | `g++` (C++17) | paquet `libgmp-dev` (ou `GMP_DIR=/préfixe`) | `make -C moteur/cpp` |
| Rust   | `rustc` (std seule, aucun crate) | `libgmp.a` cherché automatiquement | `moteur/rust/build.sh` |
| Zig    | `zig` ≥ 0.14 | `libgmp.a` | voir commande dans `moteur/README.md` |
| Python | `python3` (stdlib seule) | — | `python3 moteur/python/engine.py prog.rose` |
| Julia  | `julia` ≥ 1.9 | — | `julia moteur/julia/engine.jl prog.rose` |

Deux chemins suffisent pour la chaîne complète Debian/Ubuntu :

```console
$ sudo apt install g++ libgmp-dev python3        # moteur C++ + suites
$ # facultatif : rustc, zig, julia pour les autres moteurs
```

Sans GMP, seul le moteur C++ (et Rust/Zig) est indisponible : les
suites restent jouables avec les moteurs vivants.

### Si `tester.sh` ne reconstruit pas tout seul

```console
$ make -C moteur/cpp                      # GMP système (paquet)
$ make -C moteur/cpp GMP_DIR=/opt/gmp     # GMP installé ailleurs
$ moteur/rust/build.sh                    # GMP statique ; GMP_LIB=/chemin/libgmp.a au besoin
$ cd moteur/zig && zig build-exe src/main.zig -O ReleaseSafe -lc \
      /usr/lib/x86_64-linux-gnu/libgmp.a -lm -femit-bin=rose
```

## Ce que les suites prouvent

- **suite moteur (37 cas)** — les sorties du moteur reproduisent au
  caractère près les dorés de la cible C de Rose v2 : arithmétique
  exacte (entiers/rationnels sans limite via GMP), flottants aller‑retour
  le plus court, périodiques (`1/3 → 0.[3]`, `-1/3 → -1.[3]`),
  `partie_entiere` = plancher, stdlib complète, liste, lecture clavier
  typée, erreurs runtime (texte + code 1), FFI `@dll` (libm) et
  `dynamique::`, bundle `rose comp` autonome, CLI (version/info/aide).
- **parité (117 comparaisons, dont 89 sans Julia)** — Rust, Zig, Python, Julia comparés au
  C++ sur 19 programmes (récursion profonde incluse) : stdout, stderr et
  code de retour identiques.
- **suite v2 (147 cas)** — la chaîne d'export polyglotte (C/GMP, Python,
  Julia, Rust, Zig) reste verte ; les cibles sans outilchain sont
  sautées proprement.

## Limites connues (documentées, pas des régressions)

- **Zig** : pas de vérificateur de types statique (erreurs à
  l'exécution), pas de FFI ni de `rose comp` — le moteur est autonome
  sur le langage évalué.
- **Python / Julia** (moteurs vivants) : pas de repli de constantes
  flottantes « texte → strtod » — les expressions comme
  `-8.0 ^ (1/3)` y donnent la valeur runtime vraie (`non_nombre`), là où
  le repli C++ lit le préfixe `1/3` en `1.0`. La `parite.py` documente
  ce cas réservé aux moteurs à compilateur.
- **Julia** : la récursion profonde compte sur la pile du processus —
  lancer `ulimit -s 524288` avant `compte(20000)`. Les moteurs
  autonomes n'ont pas ce besoin (pile dédiée 512 Mo interne).
- Les binaires de `bin/` sont liés à l'architecture de la machine qui
  les a produits (ici ARM64) ; `tester.sh` rebasculera vers une
  reconstruction locale sur une autre architecture.

## Arborescence

    tester.sh              auto-test complet de cette machine (Linux/ARM64/x86)
    tester.ps1             auto-test Windows x64 (les trois .exe)
    bin/                   binaires autonomes validés — Linux (rose, rose-rs,
                           rose-zig) et Windows x64 (rose-win64.exe,
                           rose-rs-win64.exe, rose-zig-win64.exe)
    moteur/                les cinq moteurs + suites 37 cas et parité
      README.md            tout sur les moteurs (construction, validation, sémantique)
      build-win64.sh       reconstruction croisée des .exe Windows
      cpp/ rust/ zig/      moteurs compilés ; python/ julia/ moteurs vivants
      tests/               courir.py (37 cas), parite.py, dorés/
    rose2/                 chaîne d'export polyglotte (outil secondaire) + SPEC.md
    Rose language.zip      archive d'origine (v1) — intouchée
