# Transpilation Rose → JavaScript / Python / C

**Constat daté : 19/11/2026. HEAD `477736f` au début de la mission,
`6ce279e` à la fin (commit CONCURRENT d'un autre agent — aucun commit de cet
agent, rien publié). Rose 0.1.5.**
**Fichiers possédés par cet agent : `rose/src/transpile.rs` (1511 lignes),
ce document, `rose/docs/transpile.patch`. Aucun autre fichier du dépôt n'a été
modifié — en particulier `main.rs`, `lib.rs`, `Cargo.toml`, `PLAN.md` sont
intacts.**

---

## 1. Ce qui est livré

Un module Rust `rose/src/transpile.rs` qui transforme un **AST Rose** (issu de
`front::analyser` / `front::analyser_fichier`) en code source lisible et
exécutable pour trois cibles :

| Cible | Sortie | Runtime émis | Dépendance du moteur Rose |
|---|---|---|---|
| `js` | JavaScript **ES2020** (Node.js ≥ 14 et navigateurs) | classes/fonctions `RQ`, `rAdd`, `rPrint`… en tête de fichier | **aucune** |
| `py` | Python 3, sortie lisible | mêmes noms `rAdd`, `rPrint`… + `fractions.Fraction` (stdlib) | **aucune** |
| `c` | C99, valeurs taguées `RV` | `rAdd`, `rPrint`… + `struct RQ` (pas de GMP) | **aucune** |

⚠ **Node.js, python3 et gcc ne sont jamais appelés par le moteur.** Ce sont
des **outils de test** utilisés ci-dessous pour exécuter la sortie et comparer
son résultat à `./rose/rose`. Le moteur Rose n'a aucune dépendance nouvelle.

### Architecture commune (un seul marcheur d'AST)

```
transpiler_source / transpiler_fichier / transpiler_programme
        │  front::analyser  →  Programme (AST)
        ▼
Emetteur<S: Syntaxe>   ← contrôle écrit UNE seule fois :
   instructions : Afficher, Affect, AffectIndex, Si, TantQue, Pour,
                  Retour, ExprStmt, Bloc
   expressions  : 20 variantes de Expr, postfixes, appels
        │  chaque cible fournit seulement sa `Syntaxe` (briques
        │  d'expression + gabarits d'instructions) et son PRÉAMBULE
        ▼
   JS_PRELUDE / PY_PRELUDE / C_PRELUDE
```

API publique :

```rust
pub enum Cible { Js, Py, C }
pub fn transpiler_source(source: &str, cible: Cible) -> Result<String, String>;
pub fn transpiler_fichier(chemin: &str, cible: Cible) -> Result<String, String>;
pub fn transpiler_programme(prog: &Programme, cible: Cible) -> Result<String, String>;
```

---

## 2. Sous-ensemble exact couvert

### JS et Python (même couverture)

- **Valeurs** : entiers exacts (`BigInt` en JS, `int` en Python — au-delà de
  `i64` inclus), rationnels exacts (`RQ` / `Fraction`), flottants IEEE-754,
  booléens (`vrai`/`faux` à l'affichage), chaînes, listes, tables.
- **Opérateurs** : `+ - * / % div ^ et ou non ≈`, `< <= > >= == !=`.
  - `/` sur deux entiers produit un **rationnel exact** (jamais un flottant),
    comme dans le moteur.
  - `%` = modulo euclidien (signe du diviseur) ; `div` = division plancher.
  - `^` avec exposant négatif produit un rationnel.
- **Postfixes/préfixes** : `n!` (factorielle), `n!!` (multifactorielle d'ordre
  k), `n#` (primorielle), `n?` (termial classique), `n$` (super-factorielle),
  `!n` (sous-factorielle / dérangements).
- **Containers** : littéraux `[…]` et `{clé:: valeur, …}` (la syntaxe de table
  de ce build est `::`, voir 4), indexation `t[k]`, affectation par index sur
  **table** (`t[k] = v`), `longueur`/`len`, `sous_chaine`.
- **Contrôle** : `si`/`sinon`, `tant_que`,
  `pour x de a vers b` (**borne finale excluse**, `a ≤ x < b`, pas de pas
  hors 1, exactement comme `interp.rs`), `retour`, `>>` (une ligne par
  expression).
- **Fonctions** : définitions en Rose, récursion comprise, corps avec
  variables locales ; les identifiants sont préfixés `v_` (aucune collision
  avec le préambule `r…`).
- **Primitives couvertes** (catalogue explicite ; toute autre fonction est
  **refusée avec un message**, jamais traduite silencieusement) :
  `afficher`, `factorielle`/`fact`, `fibonacci`/`fib`, `pgcd`/`gcd`,
  `ppcm`/`lcm`, `abs`, `min`, `max`, `longueur`/`len`, `sous_chaine`/`sub`,
  `triangulaire`/`termial`/`termial_classique`, `primorielle`,
  `super_factorielle`, `sous_factorielle`/`derangement`, `multifactorielle`,
  `inserer`, `cle_existe`, `supprimer`, `cles`, `valeurs`, `table`, `map`.

### C (sous-ensemble plus étroit — documenté, pas prétendu complet)

Mêmes opérateurs, contrôles, fonctions et postfixes **numériques** que ci-dessus,
plus chaînes et concaténation, **mais** :

- entiers `int64_t` : **débordement non détecté** (pas de GMP, par consigne) ;
- **aucun littéral rationnel**, aucune liste/table, aucune indexation ;
- `longueur`/`sous_chaine`, `cles`/`valeurs` refused (stubs retirés) ;
- flottants rendus avec `%.17g` (formatage différent de Rose au-delà des cas
  simples).

Le C existant `rose llvm prog.rose -o sortie` (`src/cmd_llvm.rs`) passe par le
codeur Python externe de `archive/rose2_old_python/src`. Le nouveau
`--cible c` **ne refactorise pas ce chemin** (module verrouillé par un autre
agent) : il fournit un émetteur Rust indépendant, sans python3, avec la même
interface conceptuelle (`transpiler_*`). Les deux chemins coexistent.

---

## 3. Refus explicites (aucun comportement silencieux)

`transpiler_*` rend `Err(msg)` pour : complexes / irrationnels, lecture clavier
`<<`, `spawn`, `essayer`/`attraper`/`finalement`, `lever`, corps `Instr::Rir`
(déjà abaissé), appels qualifiés par un module, primitives hors catalogue,
opérateurs non couverts, et — pour le C — rationnels/listes/tables.

---

## 4. Écarts sémantiques documentés (honnêteté)

| Point | JS | Python | C |
|---|---|---|---|
| Entiers > 2⁶³ | exacts (`BigInt`) | exacts (`int`) | **débordement** |
| Rationnels | `RQ` exact, décimale périodique `[…]` identique à Rose pour les petites valeurs ; au-delà de 500 chiffres cumulés, rendu `n/d` comme Rose | `Fraction`, même rendu | refusés en littéral ; issus de `/` mais rendu décimal limité à 200 chiffres |
| Flottants | `Number` : le rendu non entier peut différer de `texte_flottant` de Rose (exposants) | `repr()` : idem | `%.17g` : idem |
| Affichage `>>` | `console.log` (une ligne par expression) ; `vrai`/`faux`, listes `[a, b]`, tables `[k => v]` | `print` ; mêmes règles | `puts` ; mêmes règles pour les valeurs couvertes |
| Clés de table | égalité par `(type, rendu texte)` : correct pour clés texte/entier, approximation pour clés complexes | idem | tables refusées |
| Chaînes | indexation par unité UTF-16 (Rose indexe des caractères) | indexation Python (caractères) | chaînes non indexables |
| Portée | déclaration `let` unique en tête de fonction (pas de portée de bloc Rose) | pas de déclaration | déclaration `RV` en tête |
| Table littérale | `{clé:: val}` → `rTableFrom` ; la ponctuation `::` est celle du parseur de ce build (`{a: 1}` est refusé par le moteur lui-même) | idem | refusée |
| E/S, concurrence, exceptions, FFI, modules | **non transpilés** | idem | idem |

---

## 5. Interface CLI proposée (patch, NON appliqué au dépôt)

`rose/docs/transpile.patch` (71 lignes) ajoute :

1. `pub mod transpile;` dans `rose/src/lib.rs` ;
2. `mod transpile;` dans `rose/src/main.rs` ;
3. une ligne d'aide `rose transpile prog.rose --cible js|py|c [-o sortie]` ;
4. le bras de dispatch `"transpile"` (parse `--cible`/`-t`/`--cible=`,
   `-o`/`--sortie`, écriture fichier ou stdout, messages d'erreur).

Vérifications réelles du patch (19/11/2026) :

```
$ cd /tmp/opencode/patchwork/check && patch -p1 --dry-run < rose/docs/transpile.patch
checking file rose/src/lib.rs
checking file rose/src/main.rs
DRY-RUN OK

$ cd /tmp/opencode/patchfull && patch -p1 < rose/docs/transpile.patch
patching file rose/src/lib.rs
patching file rose/src/main.rs
$ cd rose && cargo build --release --bin rose
    Finished `release` profile [optimized] target(s) in 46.27s
$ cargo test --release --lib transpile
running 8 tests ... test result: ok. 8 passed; 0 failed
```

Le harnais de test (hors dépôt, `/tmp/opencode/transpile-harness`) câble le
module dans une **copie** de `lib.rs` ; c'est ainsi que les 8 tests unitaires
ont été exécutés sans toucher au `lib.rs` du dépôt.

---

## 6. Round-trip réel (commandes et sorties exactes)

Harnais : `/tmp/opencode/transpile-harness/target/release/harness <src.rose> <js|py|c> <sortie>`.
Moteur de référence : `/home/clausake/Stefan/ROSE/rose/rose`.

### 6.1 Programme `base.rose` (fib, div/mod/puissance, rationnels périodiques, chaînes, boucles, factorielles, listes, tables)

```
$ ./rose/rose /tmp/opencode/rt/base.rose
55
3
-4
2
1024
0.[3]
3.[142857]
0.125
-1.[3]
1.5
3
vrai
abcd
10
3
2
1
720
6
120
105
741
44
[1, 2, 3]
[a => 1]
2

$ harness base.rose js base.js && node base.js > base.js.out
$ harness base.rose py base.py && python3 base.py > base.py.out
$ diff -u rose.out base.js.out        # AUCUNE DIFFÉRENCE
$ diff -u rose.out base.py.out        # AUCUNE DIFFÉRENCE
```

### 6.2 Programme `full.rose` (max2, rationnels négatifs, `div` signé, `et/ou/non`, ≈, indexation table, table littérale, `cle_existe`, 30!, grand entier)

```
$ ./rose/rose /tmp/opencode/rt/full.rose
9
2.25
4
3
-4
-2
faux
faux
vrai
vrai
[1, 2, 3]
2
3
onj
[x => 1, y => 2]
1
[x => 1, y => 2, z => 7]
vrai
265252859812191058636308480000000
1
1
1000000

$ harness full.rose js full.js && node full.js > full.js.out
$ harness full.rose py full.py && python3 full.py > full.py.out
$ diff -u full.rose.out full.js.out   # AUCUNE DIFFÉRENCE
$ diff -u full.rose.out full.py.out   # AUCUNE DIFFÉRENCE
```

### 6.3 Grands entiers / puissances rationnelles

```
$ ./rose/rose /tmp/opencode/rt/big.rose
75025
1267650600228229401496703205376
12.[703]
0.[142857]
-1.[714285]
$ node big.js | diff -u big.rose.out -    # AUCUNE DIFFÉRENCE
$ python3 big.py | diff -u big.rose.out - # AUCUNE DIFFÉRENCE
```

### 6.4 Cible C (`cbase.rose`, sans rationnels ni containers)

```
$ harness cbase.rose c cbase.c
$ gcc -std=c99 -O2 -o cbase cbase.c -lm
$ ./rose/rose cbase.rose > crose.out && ./cbase > c.out
$ diff -u crose.out c.out              # AUCUNE DIFFÉRENCE
```

### 6.5 Via le CLI patché (copie hors dépôt)

```
$ /tmp/opencode/patchfull/rose/target/release/rose transpile base.rose --cible js -o cli.js
rose: /tmp/opencode/rt/cli.js produit (transpilation js)
$ node cli.js | diff -u rose.out -       # AUCUNE DIFFÉRENCE
$ /tmp/opencode/patchfull/rose/target/release/rose transpile base.rose --cible py -o cli.py
rose: /tmp/opencode/rt/cli.py produit (transpilation py)
$ python3 cli.py | diff -u rose.out -    # AUCUNE DIFFÉRENCE
$ /tmp/opencode/patchfull/rose/target/release/rose transpile base.rose --cible rust
rose: cible de transpilation inconnue 'rust' (attendu js|py|c)    (rc=1)
```

---

## 7. Validation globale (non transpilation, garde-fous du dépôt)

Exécuté le 19/11/2026 avec `taskset -c 6` (pinage demandé), sur l'arbre au
HEAD `477736f` (avant le commit concurrent `6ce279e`) :

```
$ cargo build --release --bin rose         Finished release ... in 28.17s
$ cargo test --release
running 160 tests ... ok. 160 passed; 0 failed
running 144 tests ... ok. 144 passed; 0 failed
running 6 tests   ... ok. 6 passed; 0 failed
running 1 test    ... ok. 1 passed; 0 failed

$ ./rose/rose jardins/tests/premiers.rose | tail -1
── jardin/premiers : 20 tests, 0 échec ──
$ printf '>> fibonacci(25)\n' > fib25.rose && ./rose/rose fib25.rose
75025
$ ./rose/rose rir compilateur/emetteur.rose | md5sum
bdbef63681608a0b22a5637539e9d738  -
$ ./rose/rose compilateur/emetteur.rose compilateur/emetteur.rose | md5sum
bdbef63681608a0b22a5637539e9d738  -
```

---

## 8. État des lieux factuel

- **Fait** : module `transpile.rs` (3 backends, 8 tests unitaires verts), doc,
  patch CLI vérifié (`patch --dry-run`, build du CLI patché, round-trip via le
  CLI patché).
- **Vérifié** : parité octet pour octet des sorties Rose vs node/python3/gcc sur
  4 programmes couvrant le sous-ensemble annoncé ; garde-fous du dépôt verts.
- **Limite assumée** : couverture partielle (voir §2–4) — pas de complexes,
  E/S, exceptions, concurrence, modules ; C limité aux entiers 64 bits sans
  containers. Le patch CLI n'a **pas** été appliqué au dépôt (main.rs/lib.rs
  verrouillés) : il est livré sous forme de fichier patch.
