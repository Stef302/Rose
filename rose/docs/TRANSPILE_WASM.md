# Transpilation Rose → WebAssembly (module `.wasm` binaire)

**Constat daté : 21/11/2026. Aucun commit, aucune publication.**
**Fichiers possédés par ce chantier : `rose/src/transpile.rs`,
`rose/src/main.rs` (5 points d'entrée CLI/help seulement), ce document.**
⚠ Un **autre chantier non commité** modifiait simultanément `natif_emetteur.rs`,
`link_natif.rs`, `rosevm.rs`, `jit.rs`, `lib.rs`, `cmd_llvm.rs` et
`runtime_natif.rs` dans le même arbre de travail. Ces changements sont
**étrangers** à la transpilation WASM ; voir §6 pour l'état du build.

---

## 1. Ce qui est livré

`Cible::Wasm` s'ajoute à `Js`, `Py`, `C`. Contrairement aux trois autres,
**la sortie n'est pas du texte mais un module binaire `.wasm`** :

```
rose transpile prog.rose --cible wasm -o prog.wasm
```

Le module exporte une seule fonction :

```
rose_main() -> i64
```

Sa valeur est celle de la **dernière instruction `>>`** du programme (plus
précisément : chaque `>>` écrit sa valeur dans un local `__result`, et
`rose_main` rend ce local). Un programme sans `>>` rend `0`.

### API Rust

```rust
pub enum Cible { Js, Py, C, Wasm }          // + parser/extension/nom

// Texte (inchangé pour JS / Python / C) :
pub fn transpiler_source(source, cible) -> Result<String, String>;
pub fn transpiler_fichier(chemin, cible) -> Result<String, String>;
pub fn transpiler_programme(prog, cible)   -> Result<String, String>;
// ⚠ transpiler_* avec Cible::Wasm renvoie une Err explicite : le binaire ne
//    peut pas être représenté par un String.

// Binaire (nouveau) :
pub fn transpiler_source_wasm(source)      -> Result<Vec<u8>, String>;
pub fn transpiler_fichier_wasm(chemin)     -> Result<Vec<u8>, String>;
pub fn transpiler_programme_wasm(prog)     -> Result<Vec<u8>, String>;
pub fn transpiler_fichier_octets(chemin, cible) -> Result<Vec<u8>, String>;
```

`transpiler_fichier_octets` est le **point d'entrée unique de la CLI** : il
renvoie des octets pour les quatre cibles (texte encodé en UTF-8 pour
js/py/c, module binaire pour wasm). Les deux sites CLI (`rose transpile …`
et `rose methode transpile …`) l'utilisent. Sans `-o` en cible wasm, le
fichier `.wasm` est écrit à côté du programme et annoncé (on n'imprime jamais
d'octets bruts sur la console).

---

## 2. Émetteur : zéro dépendance

Comme `natif_emetteur.rs` écrit ELF/PE, l'émetteur WASM écrit les octets à la
main dans `transpile.rs` : magic `\0asm`, version `1`, LEB128 non signé
(`uleb`) et signé (`sleb`), sections et opcodes. **Aucune crate wasm, aucun
`.wat`, aucun runtime n'est une dépendance du moteur.**

Module produit, dans l'ordre :

| Section | ID | Contenu |
|---|---|---|
| Type | 1 | `(param i64…) -> i64` pour `rose_main` puis chaque fonction Rose |
| Function | 3 | un type par fonction, dans l'ordre des index |
| Export | 7 | `rose_main` (kind `func`, index 0) |
| Code | 10 | corps : `[taille][vec des locaux i64][opcodes][end]` |

Conventions d'émission :

- **Index 0 = `rose_main`** ; les fonctions Rose sont 1, 2, … dans
  `prog.ordre`. Un appel Rose devient `call <index>`.
- Toutes les valeurs sont des **i64**. Comparaisons WASM rendant un `i32`,
  on ajoute systématiquement `i64.extend_i32_u`.
- `si` utilise `i32.wrap_i64` sur la condition (WASM `if` exige un i32).
- `tant_que` = `block` + `loop` + `br_if 1` + `br 0`.
- `pour k de A vers B` = borne **exclusive** (comme l'interpréteur) : A dans
  `k`, B dans un local caché, test `k >= B`, incrément de 1.
- Les noms de variables Rose sont tous des locaux i64. ⚠ Les **paramètres
  WASM occupent les index 0..k-1** ; les locaux déclarés viennent après
  (`base = k`). C'est un piège corrigé pendant la validation.
- Les locaux temporaires portent un nom commençant par un caractère de
  contrôle (`\u{1}wN`) : collision impossible avec un identifiant Rose.
- `non`/`et`/`ou` normalisent leurs opérandes en booléen `x != 0`.
- `%` implémente le **modulo euclidien de Rose** (signe du diviseur, comme le
  `rMod` du préambule JS), pas le `i64.rem_s` brut.

---

## 3. Sous-ensemble couvert (et vérifié)

| Construction | Émission | Testé |
|---|---|---|
| `>> 7*6` | `i64.const 42`, retour via `__result` | ✅ 42 |
| `+`, `-`, `*` | `i64.add/sub/mul` | ✅ 42 / 42 / 42 |
| Littéraux signés, `-x`, `+x` | `i64.const` (LEB128 signé), `0 - x` | ✅ `-7`, `20` |
| `<`, `<=`, `>`, `>=`, `==`, `!=` | comparaisons i64 + extension u32→i64 | ✅ 1/0 |
| `et`, `ou`, `non` | normalisation booléenne + `i32.and/or/eqz` | ✅ |
| Variables, affectations | `local.get` / `local.set` | ✅ 42 |
| `si` / `sinon` | `if (void)` | ✅ |
| `tant_que` | `block` + `loop` | ✅ 10 |
| `pour … de … vers …` | borne exclusive, local caché | ✅ 10 |
| Fonctions, récursion, `retour` | `call` / `return` | ✅ `carre(9)=81`, `fib(10)=55` |
| `div`, `%` euclidien | `i64.div_s`, correction de signe | ✅ 42, `-7 % 3 = 2` |
| Booléens | `i64.const 0/1` | ✅ |

### Refus explicites (`Err`, jamais de silence)

Rationnels/flottants (`1 / 2`), complexes (`2 * i`), chaînes, listes,
tables, indexation, lecture clavier (`<<`), `spawn`, `essayer`/`attraper`,
`lever`, `Instr::Rir`, appels de module, primitives de la bibliothèque
(`factorielle`, `fibonacci`, `pgcd`, … — seules les **fonctions définies en
Rose** sont appelables), `^`, `≈`, et tout entier hors de l'intervalle i64.

`>>` n'émet **aucune sortie** dans le module : il ne fait que mémoriser une
valeur. C'est un choix ABI, pas une omission ; le module ne dépend d'aucun
runtime d'affichage.

---

## 4. Validation effectuée (preuves)

1. **`cargo test` complet dans un arbre propre issu de `HEAD`** avec uniquement
   `transpile.rs` + les 5 points CLI remplacés :
   `301 + 314 + … passed; 0 failed` (voir §6 pour l'arbre de travail courant).
2. **25 modules générés, validés et exécutés par Node.js 26** (`new
   WebAssembly.Module` = validation stricte, puis `rose_main()`) : **25/25
   OK**, dont `>> 7*6 → 42`, `carre(9) → 81`, `fib(10) → 55`,
   `-7 % 3 → 2`, `pour … → 10`, fonctions à 2 paramètres (`add(20,22)=42`),
   locaux dans une fonction (`f(6)=15`), appels imbriqués (`f(4)=17`),
   `si` sans `sinon`, combinaisons de comparaisons.
3. **CLI de bout en bout** :
   `rose transpile prog.rose --cible wasm -o prog.wasm` → 48 octets,
   `WebAssembly.Module` valide, `rose_main() = 42n`.
4. **Non-régression JS/Python/C** : `smoke.rose` (`carre(7)`) transpilé dans
   les 4 cibles ; `node smoke.js` → 49, `python3 smoke.py` → 49,
   `gcc -O2 smoke.c -o smoke_c -lm && ./smoke_c` → 49, module wasm valide.
5. **Point fixe `b3ed9262` ×2** avec le binaire compilé depuis l'arbre propre :
   `rose rir compilateur/emetteur.rose` deux fois → 263 520 octets,
   md5 `b3ed9262dd54776a6dbd07bc12368d0e` **deux fois**, `cmp` = 0, stderr 0.

---

## 5. Comment rejouer la validation WASM

```bash
# 1. Production du module
echo '>> 7*6' > /tmp/prog.rose
rose transpile /tmp/prog.rose --cible wasm -o /tmp/prog.wasm

# 2. Validation + exécution (Node >= 14 ; i64 => BigInt)
node -e '
const fs=require("fs");
const m=new WebAssembly.Module(fs.readFileSync("/tmp/prog.wasm"));
const i=new WebAssembly.Instance(m);
const r=i.exports.rose_main();
console.log("rose_main() =", r);
if (r !== 42n) process.exit(1);
'
```

Sans Node : `wasm-tools validate prog.wasm` puis
`wasmtime prog.wasm --invoke rose_main` si ces outils sont installés (ils ne
le sont **pas** sur cette machine : `command -v` → absent — c'est Node qui a
servi de validateur).

---

## 6. État du build dans l'arbre de travail au moment du rapport

⚠ **Le dépôt `/home/clausake/Stefan/ROSE` est modifié en parallèle par un
autre chantier** (`runtime_natif.rs` AOT6, `natif_emetteur.rs`, `link_natif.rs`,
`rosevm.rs`, `jit.rs`, `lib.rs`, `cmd_llvm.rs`). Pendant ce travail,
`cargo check` y a successivement signalé des erreurs **dans ces fichiers**
(types `Architecture` incompatibles, argument manquant à `stub_depart`,
`mod runtime_natif` absent de `main.rs`, `Ty`/`TyKind` dans `rosevm.rs`), et
les fichiers changeaient entre deux lectures. **Aucune de ces erreurs ne
vient du code WASM** (`transpile.rs` compile et ses 12 tests passent).

Pour ne pas dépendre de ce chantier mouvant, toute la validation ci-dessus a
été faite dans `/tmp/opencode/rose-wasm-verify` (arbre `git archive HEAD` +
ce `transpile.rs` + les 5 remplacements CLI). **Aucun commit, aucune publication,
aucune suppression de fichier du chantier concurrent** ; seul `mod runtime_natif;`
a été ajouté au `main.rs` courant pour tenter de débloquer son build.

**État final mesuré dans le dépôt réel (16:35)** : `cargo check` et
`cargo build --release --bin rose` passent (0 erreur) ; le binaire release
produit bien `prog.wasm` et Node exécute `rose_main() = 42n`. `cargo test`
complet y donne **305 passés, 2 échecs** — tous deux dans
`link_natif::tests` (`stub_avec_affichage_est_plus_long_et_contient_write`,
`elf_avec_valeur_exacte_ecrit_le_texte_memorise`), c'est-à-dire le runtime
natif AOT6 simultané, pas la transpilation. Dans l'arbre propre ces deux
tests passent (301 + 314 tests, 0 échec). Les 12 tests `transpile::tests`
passent dans les deux arbres.

---

## 7. Ce qui reste hors périmètre

- Nombres rationnels/flottants, BigInt au-delà de 64 bits, chaînes, listes,
  tables (la cible C est elle-même limitée au noyau numérique).
- Primitives de la bibliothèque standard (elles ont des preuves complexes en
  JavaScript/Python/C ; les réécrire en opcodes i64 n'était pas demandé).
- Import/export d'autres modules WASM, mémoire, tables, traps personnalisés.
- L'interopérabilité avec `wasm.rs` / `wasm_lecteur.rs` / `wasm_runtime.rs`
  (qui **lisent/appellent** des modules) n'a pas été modifiée : ce chantier
  n'a ajouté que l'**émission**.
