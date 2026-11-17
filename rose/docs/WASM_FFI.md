# WebAssembly — les bibliothèques d'autres langages, sans leur runtime

⚠ **Le problème** : chaque langage a ses bibliothèques écrites dans son
propre langage, et les exécuter exige SON runtime (Python a CPython, JS a
V8, Rust a sa stdlib…). Unifier tout ça en important des binaires natifs
(`.so`/`.dll`) ne marche pas : chaque format binaire est lié à une ABI, une
architecture, un système.

⚠ **La réponse de Rose** : le **WebAssembly** est le seul format binaire que
TOUS les compilateurs modernes savent produire et que l'on peut exécuter
**sans runtime de langage** :

| Source | Commande |
|---|---|
| C / C++ | `clang --target=wasm32 -nostdlib -Wl,--no-entry -Wl,--export=f lib.c -o lib.wasm` |
| Rust | `cargo build --target wasm32-unknown-unknown --release` |
| Zig | `zig build-lib -target wasm32-freestanding -dynamic lib.zig` |
| Emscripten | `emcc lib.c -s STANDALONE_WASM -s EXPORTED_FUNCTIONS='["_f"]'` |

Le module est **autonome** (pas d'import, pas de `start`) : Rose l'exécute
avec son propre interpréteur WebAssembly (`rose/src/wasm_lecteur.rs`), qui
n'ajoute **aucune dépendance** — pas de `wasmtime`, pas de `wasmer`, pas de
`wasmi`.

## Utilisation depuis Rose

```rose
@dll("wasm", "lib.wasm")
@sym("carre")
fn carre(x: entier) -> entier

@dll("wasm", "lib.wasm")
@sym("pgcd")
fn pgcd_wasm(a: entier, b: entier) -> entier

@dll("wasm", "lib.wasm")
@sym("demi")
fn demi_wasm(x: flottant) -> flottant

carre(7)          # 49
pgcd_wasm(48, 18) # 6
demi_wasm(5)      # 2.5
```

⚠ `@dll("wasm", "lib.wasm")` : le premier argument est le mot `wasm`, le
second le FICHIER. Le module est chargé UNE fois (une seule instance, avec
son état : mémoire linéaire et globals), puis chaque `@sym` devient un appel
direct à la fonction exportée.

## La convention des types (ABI C, wasm32)

| Type Rose | Type wasm | Remarque |
|---|---|---|
| `entier` | `i32` | ⚠ 32 bits : un module wasm ne voit que `i32`/`i64` ; Rose convertit |
| `booleen` | `i32` | 0 = faux, 1 = vrai |
| `flottant` | `f64` | le `double` de C |
| `chaine` | `i32` | ⚠ POINTEUR dans la mémoire linéaire : les octets y sont copiés puis suivis d'un zéro (`char*` de C) |

⚠ Le retour suit le type déclaré dans le `fn` Rose : `-> flottant` lit un
`f64`, `-> chaine` relit une chaîne C (jusqu'au zéro), tout le reste est lu
comme `i32`.

⚠ Pour un module compilé en C avec `-nostdlib`, la zone libre est le HAUT de
la mémoire linéaire : Rose y écrit les chaînes (après 8 octets de marge). Un
module qui exporte son propre `malloc` doit recevoir un pointeur explicite
(argument `entier`).

## Ce que l'interpréteur supporte

| Catégorie | Opcodes |
|---|---|
| types | `i32`, `i64`, `f32`, `f64` (pas de `v128`) |
| contrôle | `block`, `loop`, `if`/`else`, `br`, `br_if`, `br_table`, `return`, `call`, `drop`, `select` |
| mémoire | `load`/`store` (toutes tailles), `memory.size`, `memory.grow` |
| arithmétique | `const`, `add`/`sub`/`mul`/`div`/`rem` (signés et non signés), et/ou/xor, décalages, rotations, comparaisons, conversions, `clz`/`ctz`/`popcnt`, `abs`/`neg`/`ceil`/`floor`/`trunc`/`nearest`/`sqrt` |
| globals | `get`/`set` (mutables compris) |

⚠ **Refusé PROPREMENT** (message clair, pas de plantage) : les sections
`start` et d'**import** (le module doit être autonome), `call_indirect`, les
exceptions, les SIMD, les threads.

## ⚠⚠⚠ Deux pièges d'implémentation (mesurés, à ne pas réintroduire)

1. **Les cibles des blocs sont pré-calculées** (`decouper`) : un `br` ne doit
   JAMAIS re-scanner le corps pour retrouver la fin du bloc — le coût d'un
   saut deviendrait proportionnel à la taille de la fonction, donc
   quadratique dans une boucle chaude.
2. **Après un saut, la pile est REMISE À NIVEAU** (`entree + nr`) : la
   spécification wasm impose que la pile revienne à la hauteur d'entrée du
   bloc visé. Sans ça, chaque tour de boucle laisse ses valeurs (mesuré :
   `pgcd` sortait de sa boucle avec une pile vide).
3. **Les opérations entières sont dans une TABLE (`op_i32`/`op_i64`), pas
   dans un `match` de bornes** : un `match` ordonné avec une borne plus large
   que ses cas avale les opcodes suivants (mesuré : `0x6C` tombait dans
   `count_ones`).

## Vérifier un module sans l'exécuter

```
rose wasm lib.wasm      # liste les fonctions exportées (nom, types)
```

## Ce qui n'est PAS encore fait

- ⚠ Les **tableaux** (`tableau`, `liste`) ne sont pas passés en mémoire :
  seuls les scalaires et les chaînes le sont. Un module qui prend un `int*`
  se sert d'un `entier` (adresse) que Rose remplit avec `ecrire_memoire` —
  à exposer comme primitive quand le besoin apparaîtra.
- ⚠ Pas de `call_indirect` (les modules autonomes d'un compilateur C n'en
  produisent pas ; Rust peut en produire pour les `fn` passés en argument).
- ⚠ La performance de l'interpréteur est celle d'un interpréteur : un module
  wasm reste **plus lent** que du code natif. C'est un pont de
  COMPATIBILITÉ, pas une voie de vitesse.
