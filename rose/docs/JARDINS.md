# Jardins — bibliothèques de première classe en Rose

> État au **18/11/2026** — Rose 0.1.5. Ce document décrit le modèle des
> jardins du point de vue du langage. Le catalogue maintenu et la checklist de
> contribution sont dans [`jardins/README.md`](../../jardins/README.md).

## 1. Définition

Un **jardin** est une bibliothèque Rose écrite **en Rose**, distribuée comme
un simple fichier `.rose`. C'est une unité de réutilisation de première classe :
elle s'importe, se vérifie, se teste et se catalogue avec les outils du langage,
sans aucun artefact binaire.

```

jardins/premiers.rose     ← bibliothèque (Rose pur, fonctions seules)
jardins/hachage.rose      ← bibliothèque (Rose pur, fonctions seules)
jardins/tests/premiers.rose  ← programme de test, importe le jardin
```

Propriétés vérifiables :

| Propriété | Commande |
|---|---|
| Le fichier est accepté par le vérificateur | `./rose/rose verif jardins/hachage.rose` → `vérifié` |
| Le fichier est du Rose pur | `scripts/rose-jardin.sh verif hachage` |
| Les signatures exportées | `./rose/rose ast jardins/hachage.rose` puis lire les lignes `fonction` / `param` / `retour` |
| Le catalogue | `scripts/rose-jardin.sh liste` → `2 jardins, 10 fonctions exportées` |

## 2. Ce qu'un jardin EST et ce qu'il n'EST PAS

Un jardin EST :

- un ensemble de `fn` en Rose, avec les types du langage (`entier`, `booleen`,
  `chaine`, `liste<entier>`, …) ;
- un fichier chargé par `importer "jardins/<nom>.rose"`, résolu récursivement
  par `front::analyser_fichier` (une seule lecture par fichier, vérification
  après fusion des signatures) ;
- du calcul en **entier exact** (BigInt natif, zéro GMP) — aucune troncature
  à 32/64 bits.

Un jardin N'EST PAS :

- un morceau du compilateur : ni lexer, ni vérificateur de types, ni
  interpréteur, ni gestion mémoire (`Gz`, `Gq`, `Rc`, `Box`) ;
- un opérateur du langage : `n!`, `+`, `^`, `?`, `!`, `#` restent natifs ;
- un module Rust : pas de `Cargo.toml`, pas de `.rs` dans un jardin ;
- un module wasm : pas de `@dll`, pas de `@sym`, pas de glue d'ABI ;
- un programme : aucune instruction `>>` (les tests, eux, en ont).

## 3. Exemple complet : le jardin `hachage.rose`

```rose
importer "jardins/hachage.rose"

>> chaine(codes_caracteres("abc"))       # [97, 98, 99]
>> chaine(somme_caracteres("abc"))       # 294
>> chaine(hachage_djb2("hello"))         # 210714636441  (exact)
>> chaine(hachage_djb2_mod("hello", 1000)) # 441         (indice de seau)
```

`hachage_djb2` illustre l'intérêt du calcul exact : le djb2 de `"hello"` vaut
`210714636441` en Rose, alors qu'une implémentation C classique tronque à
`261238937` (modulo 2³²). Rose garde la valeur entière, sans débordement et
sans bibliothèque de grandes valeurs.

Valeurs vérifiées le 18/11/2026 par `/tmp/opencode/test_hachage.rose` :
12 assertions, 0 échec (dont `hachage_djb2("") = 5381`,
`hachage_djb2("abc") = 193485963`, `hachage_djb2("Rose") = 6384512510`,
`hachage_djb2_mod("abc", 0) = 0`).

## 4. Écrire un jardin

```sh
scripts/rose-jardin.sh init mon_sujet        # squelette documenté
scripts/rose-jardin.sh verif mon_sujet       # Rose pur + rose verif
scripts/rose-jardin.sh verif-tous            # tous les jardins d'un coup
scripts/rose-jardin.sh liste                 # catalogue + signatures
```

Le squelette généré passe `rose verif` immédiatement. Les conventions
(nommage, en-tête, liste vide typée `[0 : entier]`, tests dans
`jardins/tests/`) sont détaillées dans `jardins/README.md`, §5 et §10.

Le contrôle `verif` refuse :

- un marqueur de code Rust/FFI (`extern crate`, `use std::`, `@dll`,
  `charger(`, `appeler0(`, …) ;
- une instruction programme `>>` (un jardin n'est pas un `main`) ;
- un import qui n'est pas un fichier `.rose` ;
- tout fichier rejeté par `rose verif` (sortie autre que `vérifié`).

## 5. Pourquoi un jardin et pas une bibliothèque C via wasm ?

Rose supporte aussi l'appel de vraies bibliothèques C compilées en wasm
(`@dll("wasm", …)` / `@sym(…)`, interpréteur `rose/src/wasm_lecteur.rs`),
documenté dans [`INTEROP_BIBLIOTHEQUE.md`](INTEROP_BIBLIOTHEQUE.md) avec
xxHash v0.8.2. Ce n'est pas le même modèle :

| Jardin Rose | Bibliothèque C via wasm |
|---|---|
| source `.rose`, lisible et modifiable | artefact `.wasm` compilé |
| `importer` | `@dll` + `@sym` + chargeur |
| aucune chaîne d'outils | clang/wasi-sdk, glue ABI, shims libc |
| entier exact illimité | `i32`/`i64` machine |
| `rose verif` suffit | validation de sections wasm + exécution dans un interpréteur de pile |

Le jardin gagne en **simplicité de distribution**, **exactitude des calculs**
et **intégration dans le langage** (symboles visibles dans `rose ast`).
La voie wasm apporte l'accès à du code C **existant**. Aucune des deux n'est
déclarée plus rapide que l'autre sans mesure : les seules mesures disponibles
sont celles du 16/11 sur le jardin des premiers (voir `jardins/README.md`, §8),
qui montrent un jardin Rose ~2× plus lent que la primitive native `est_premier`
et plus lent que le C `-O2` sur cette boucle — un jardin vise l'algorithmique
absente du cœur, pas le remplacement d'une primitive chaude.

## 6. Résumé des chemins

| Fichier | Rôle |
|---|---|
| `jardins/premiers.rose` | jardin de référence (premiers, facteurs, pgcd) |
| `jardins/hachage.rose` | jardin de démonstration (djb2 exact) |
| `jardins/tests/premiers.rose` | tests du premier jardin, cross-check natif |
| `jardins/README.md` | catalogue, conventions, checklist de contribution |
| `scripts/rose-jardin.sh` | `liste` / `init` / `verif` / `verif-tous` |
| `rose/docs/JARDINS.md` | ce document (modèle du langage) |
