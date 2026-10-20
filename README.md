# Rose — langage de mathématiques autonome

Rose est un langage de calcul mathématique : entiers et rationnels
**exacts sans limite** (GMP), décimaux périodiques (`1/3 → 0.[3]`),
flottants fidèles, fonctions utilisateur, listes, FFI, et un mode
compilation qui scelle un programme dans un **binaire autonome unique**
— aucune dépendance d'exécution.

À partir de la **v3**, le langage possède aussi les briques de
l'auto-hébergement : indexation de chaînes Unicode par caractères
(`s[i]`, `ordre`, `caractere`, `sous_chaine`), constructeur linéaire
`concatener`, lecture/écriture de fichiers, arguments de ligne de
commande et sortie d'erreur contrôlée (`quitter`). La **v3.2** ajoute la
notation symbolique : le code s'écrit avec les symboles mathématiques —
`⋅ × ÷ ≤ ≥ ≠ ≈ √ ² ³ π ← →` — et unifie l'arithmétique mixte
rationnel⊛flottant. Les mesures de vitesse sont dans `PERF.md`. Le plan
complet — IR, JIT, LLVM, compilateur écrit en Rose — est dans
`ARCHITECTURE.md`.

## Démarrer

```console
$ ./tester.sh          # auto-test complet de la machine (voir TESTER.md)
$ bin/rose prog.rose   # interpréter un programme Rose
$ bin/rose comp prog.rose -o prog && ./prog   # binaire autonome
```

Sous **Windows 10/11 x64**, aucun prérequis : le moteur officiel
`bin\rose-win64.exe` (et les oracles `rose-rs-win64.exe`,
`rose-zig-win64.exe`) tournent tels quels (seules dépendances : les
DLL livrées avec Windows) ; `powershell -File .\tester.ps1` valide le
tout.

## Contenu

| chemin | rôle |
|--------|------|
| `ARCHITECTURE.md` | **cap du projet** : un moteur officiel + des oracles, feuille de route IR → JIT → LLVM → auto-hébergement |
| `DEPENDANCES.md` | **inventaire des dépendances externes** : surface GMP mesurée (44+15 fonctions), route vers le bignum natif `rose_mpz`/`rose_mpq` et l'affranchissement des autres liens |
| `TESTER.md` | **guide du test final** sur votre machine (Linux **et Windows**) |
| `bin/` | moteur officiel `rose` / `rose-win64.exe` (C++) ; les exécutables Rust/Zig livrés sont les **oracles** du contrat de parité |
| `moteur/` | le moteur officiel (C++) et les quatre oracles (Rust, Zig, Python, Julia), suites de validation, spec de l'IR (`IR.md`) — détails dans `moteur/README.md` |
| `rose2/` | chaîne d'export polyglotte (C/GMP, Python, Julia, Rust, Zig) — outil secondaire ; spécification dans `rose2/SPEC.md` |
| `Rose language.zip` | archive d'origine (v1), conservée intacte |

Un langage, **un moteur produit** (C++), quatre **validateurs** : Rust,
Zig, Python et Julia sont affectés à des tâches de contrôle (parité,
spécification vivante) et prouvent **identiques au caractère près**
chaque évolution du moteur (`moteur/tests/parite.py` : 117 comparaisons,
0 divergence). Le chemin complet — IR, JIT natif, backend LLVM,
compilateur écrit en Rose (auto-hébergement) — est tracé dans
`ARCHITECTURE.md`.

## Exemple

```rose
>> 1/3                    # 0.[3] — période détectée
>> nombre_pi(30)          # 3.141592653589793238462643383280
>> fibonacci(200)          # 42 chiffres, exact
>> partie_entiere(0-7/2)   # -4  (plancher)
```

    $ bin/rose exemple.rose
=======
# Rose
An experimental programming language
