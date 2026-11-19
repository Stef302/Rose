# Pont WebAssembly : imports hôtes + convertisseur tous langages

**Constat daté : 19/11/2026.** Repository `Stef302/Rose`, HEAD de départ `477736f`,
Rose 0.1.5. Toutes les commandes ci-dessous ont été exécutées ; les sorties sont
celles obtenues, pas des citations. Pinage `taskset -c 8` pour toutes les mesures.

---

## 1. Ce qui a changé

### 1.1 `rose/src/wasm_lecteur.rs` — la section d'import est lue

Avant : `charger()` rejetait la section 2 avec
`section d'import : un module autonome est attendu`.

Après :

* la section 2 est **décodée** (func / table / memory / global) ;
* les **imports de fonction** sont résolus au chargement vers un **registre
  d'hôtes** `nom module + nom champ → fn(&mut InstanceWasm, &[ValWasm])` ;
* les **imports de mémoire** sont reliés à la mémoire linéaire que Rose alloue
  pour l'instance (`env.memory`, toute paire module/nom de genre memory) ;
* les imports table/global sont refusés avec un message explicite ;
* l'espace des indices de fonctions respecte la spec : un import de fonction
  occupe l'indice `0..N-1`, les fonctions définies commencent à `N`
  (`nb_imports_fonction`). `call`, les exports et `appeler()` passent par
  `ModuleWasm::signature_de(idx)` qui gère ce décalage.

Corrigés au passage (nécessaires aux modules WASI réels, mesurés) :

* `lire_offset()` accepte `global.get N` (utilisé par `__data_end`) ;
* les sections `elem`/`data` acceptent l'octet de drapeaux bulk-memory
  (0 actif / 1 passif / 2 actif avec index / 3 déclaratif), avec détection
  automatique de l'ancien format sans drapeau ;
* `i64.load` (0x29) et `i64.load8/16/32` (0x30..0x35) poussent désormais un
  `ValWasm::I64` au lieu d'un `I32` tronqué. Mesure : sans ce correctif, un
  iovec WASI avait son `len` réduit à 0 et `fd_write` écrivait 0 octet.

Les modules 0-import existants gardent le même chemin de décodage unique au
chargement (handles, caches `code`/`blocs_info`/`tables_info` inchangés).

### 1.2 Registre d'hôtes — API publique

```rust
pub type HoteFonction = fn(&mut InstanceWasm, &[ValWasm]) -> Result<Vec<ValWasm>, WasmErr>;
pub fn enregistrer_hote(module: &str, nom: &str, f: HoteFonction);
pub fn resoudre_hote(module: &str, nom: &str) -> Option<HoteFonction>;
pub fn hotes_integres() -> Vec<(String, String)>;
```

Un hôte enregistré **après** le chargement d'un module n'est pas visible par ce
module : la résolution a lieu dans `charger()`. Enregistrer avant.

### 1.3 `scripts/rose-wasm-lib.sh` — détection par extension

L'interface CLI est inchangée (`--export`, `--out`, `--dir`, `--buf`, `--header`,
`-h`). Une seule option est ajoutée : **`--allow-imports`**, qui autorise les
imports (le comportement C historique exige toujours 0 import par défaut).
La vérification imprime désormais `imports_liste=...` et valide chaque import
contre le registre du pont ; le `.rose` généré contient la liste des imports
résolus.

---

## 2. Liste EXACTE des imports supportés

### 2.1 Fonctions hôtes intégrées

| Module | Nom | Comportement réel |
|---|---|---|
| `env` | `log` | Affiche sur stdout. `(i32)` → `[wasm] N` ; `(i32,i32)` → chaîne mémoire `(ptr,len)` ; `(i32,i32,i32)` → `(fd,ptr,len)`, fd ignoré. |
| `env` | `ecrire` | Idem, sans retour à la ligne. |
| `wasi_snapshot_preview1` | `fd_write` | Lit les iovecs `(ptr,len)` depuis la mémoire wasm, écrit sur stdout (fd 1) ou stderr (fd 2), dépose le nombre d'octets à `nwritten`, rend `0` (errno WASI). |
| `wasi_snapshot_preview1` | `fd_read` | **Sandbox** : ne lit jamais stdin (pas de blocage), dépose `0` à `nread`, rend `0`. |
| `wasi_snapshot_preview1` | `proc_exit` | Ne tue PAS le processus Rose : rend une erreur propre `proc_exit(N) demandé par le module wasm`. |
| `wasi_snapshot_preview1` | `random_get` | Remplit `(ptr,len)` avec un xorshift64 non cryptographique griné par l'horloge, rend `0`. |
| `wasi_unstable` | `fd_write`, `fd_read`, `proc_exit`, `random_get` | Alias des quatre précédents (anciens clang wasi-sdk). |

Total : **10** entrées de fonction (6 noms `wasi_snapshot_preview1`/`env` + 4 alias).
Les valeurs de retour sont adaptées à la signature déclarée (tronquées ou
complétées par des zéros, puis converties au type wasm attendu).

### 2.2 Mémoire

Tout import de **genre memory** est accepté (pas seulement `env.memory`), à
condition qu'il n'y en ait qu'un et qu'aucune mémoire ne soit définie par le
module. Rose alloue `min` pages de 64 Kio ; `InstanceWasm::nouveau` charge les
segments de données à l'offset résolu.

### 2.3 Comportement des autres imports

```text
$ ./rose/rose wasm /tmp/opencode/wasm_imports/import_inconnu.wasm
rose wasm : ... : import non résolu : env.inconnu — hôtes disponibles : env.log, env.ecrire, wasi_snapshot_preview1.fd_write, wasi_snapshot_preview1.fd_read, wasi_snapshot_preview1.proc_exit, wasi_snapshot_preview1.random_get, wasi_unstable.fd_write, wasi_unstable.fd_read, wasi_unstable.proc_exit, wasi_unstable.random_get
```

* import de **table** → `import de table MOD.NOM : non supporté` ;
* import de **global** → `import de global MOD.NOM : non supporté` ;
* type de fonction invalide → `import MOD.NOM : type N inconnu` ;
* plusieurs mémoires importées, ou mémoire importée **et** définie → erreur
  explicite. Aucun crash, aucun `panic!` : tout passe par `WasmErr`.

Le convertisseur applique le même contrôle avant d'écrire le `.wasm` :

```text
ERREUR: imports non resolus par le pont Rose: env.inconnu (fonction)
```

---

## 3. Preuves d'exécution (fonctions hôtes)

### 3.1 `env.log` depuis un module C compilé avec un import

```sh
CLANG=/home/clausake/.local/wasi-sdk/bin/clang
SYSROOT=/home/clausake/.local/wasi-sdk/share/wasi-sysroot
"$CLANG" --target=wasm32-wasi --sysroot="$SYSROOT" -O0 -nostdlib \
   -Wl,--no-entry -Wl,--allow-undefined -Wl,--export-memory \
   -Wl,--export=tester_import import_log.c -o import_log.wasm
```

Source : `__attribute__((import_module("env"), import_name("log"))) extern void log_i32(int);`
`llvm-objdump` confirme `call 0` (l'import) depuis `tester_import` (index 1).

```text
$ ./rose/rose /tmp/opencode/wasm_imports/demo_import.rose
[wasm] 20      ← sortie de la fonction hôte Rose
41             ← tester_import(20) = 20*2+1
```

### 3.2 Les quatre imports WASI

Module C `import_wasi.c` important `fd_write`, `fd_read`, `random_get`,
`proc_exit` ; quatre exports.

```text
$ ./rose/rose /tmp/opencode/wasm_imports/demo_wasi.rose
Bonjour depuis wasi fd_write      ← fd_write(1, ...)
356                               ← rc=0, ecrit=29, 'B'=66 => 29*10+66
0                                 ← fd_read : 0 octet disponible
0                                 ← random_get : errno 0

$ ./rose/rose /tmp/opencode/wasm_imports/demo_exit.rose
Rose : erreur — appel WebAssembly : ... : wasi_snapshot_preview1.proc_exit : proc_exit(7) demandé par le module wasm
```

### 3.3 Mémoire importée

```sh
"$CLANG" ... -Wl,--import-memory -Xlinker --import-memory=env,memory ...
```

Le module importe `env.memory` (2 pages) **et** `env.log`.

```text
$ ./rose/rose /tmp/opencode/wasm_imports/demo_memory.rose
[wasm] 7
107
```

---

## 4. Langages réellement convertis

Toutes les conversions passent par `scripts/rose-wasm-lib.sh`, puis le `.rose`
généré est importé et appelé depuis Rose.

### 4.1 C — préservation exacte

```sh
bash scripts/rose-wasm-lib.sh \
  --export 'xxh32_str=XXH32(chaine,entier,entier)->entier' \
  --out libxxhash_test /tmp/opencode/interop/xxhash.c
```

```text
WASM_CHECK imports=0 start=0 call_indirect=0 memory_exported=yes
==> OK: ./libxxhash_test.wasm  (3283 octets)
```

Appel : `46947589`, `852579327` (identiques à la baseline du 18/11).
FNV : `440920331`, `text_len("abc")=3` (identiques).

### 4.2 C++

```sh
bash scripts/rose-wasm-lib.sh --export 'carre_cpp=carre_cpp(entier)->entier' \
  --out libcpp calc.cpp
```

```text
==> compilation C++: .../clang++ ... -O0 ... -> ./libcpp.wasm
WASM_CHECK imports=0 start=0 call_indirect=0 memory_exported=yes
carre_cpp(9) -> 84
```

Contraintes : clang++ wasi-sdk, `-fno-exceptions -fno-rtti`, glue et shim
compilés en C ; la fonction réelle doit être `extern "C"`.

### 4.3 Rust

```sh
bash scripts/rose-wasm-lib.sh --export 'triangle_rs=triangle(entier)->entier' \
  --out librust lib.rs
```

```text
==> compilation Rust: /home/clausake/.cargo/bin/rustc --target wasm32-unknown-unknown ...
WASM_CHECK imports=0 start=0 call_indirect=0 memory_exported=yes
==> OK: ./librust.wasm  (627777 octets)
triangle_rs(10) -> 55
```

Cibles installées mesurées : `wasm32-unknown-unknown`, `wasm32-wasip1` ;
rustc 1.99.1. Le script génère une racine de crate `#![no_std] #![no_main]`
avec `#[panic_handler]`, `-C overflow-checks=off -C panic=abort`.

**Limite mesurée** : avec `std`, le module contient **14 `call_indirect`**
(refusés par le pont) et l'appel se bloque avec les vérifications de débordement
activées. Le code converti doit donc être du calcul pur (`core`) et exposer
`#[no_mangle] pub extern "C" fn`. Le nom d'export wasm vient du `#[no_mangle]`
du source : il doit être **identique** au nom à gauche de `=` dans `--export`.

### 4.4 WAT / WAST

Pas de `wat2wasm` sur la machine (`command -v wat2wasm` → rien). Le script
contient un encodeur binaire Python intégré (sous-ensemble documenté : module,
func avec param/result/local/export, import func/memory, memory, export, data,
const i32/i64/f64, appels, blocs, arithmétique i32/i64, load/store,
memory.size/grow).

```sh
bash scripts/rose-wasm-lib.sh --export 'double_wat=double(entier)->entier' \
  --allow-imports --out libwat demo_wat2.wat
```

```text
WAT_ENCODE ok: 91 octets, 1 fonction(s), 1 import(s)
WASM_CHECK imports=1 start=0 call_indirect=0 memory_exported=yes
exports=double_wat
imports_liste=env.log(kind=0)
[wasm] 42
84
```

`.wast` avec assertions n'est pas supporté : message
`un seul (module ...) est attendu (.wast avec assertions refuse)`.

### 4.5 Zig — non converti (outil absent)

`command -v zig` → rien. Le script refuse avec la commande d'installation
précise et le drapeau `ZIG=/chemin/zig`. Aucun module Zig n'a été produit :
**non revendiqué**.

---

## 5. Limites honnêtes

* **Python** : refusé. Cython/mypyc génèrent du C qui dépend de `Python.h`
  (imports non résolus, runtime CPython requis). CPython n'est PAS importé dans
  le produit. La preuve explorée plus tôt reste vraie : Rich entier ne se
  convertit pas.
* **Go** : refusé. Le compilateur Go produit un wasm avec runtime/GC et de
  nombreux imports WASI non résolus. TinyGo `-target=wasi` pourrait aider mais
  n'est pas installé : non mesuré.
* **JavaScript** : refusé. Emscripten absent ; son runtime JS n'est pas un
  module wasm pur.
* **Zig** : message d'installation, aucune conversion mesurée.
* **Rust std** : 14 `call_indirect` mesurés → refusé par le pont (calcul pur
  requis).
* **Rust avec boucles** : une fonction `while` simple compilée en `loop i32`
  (bloc typé avec résultat) fait **bloquer l'appel** (mesuré, `timeout 20` →
  exit 124) : l'interpréteur ne gère pas les résultats de blocs typés. Les
  fonctions Rust converties doivent être sans boucle (formules arithmétiques) ;
  utiliser C/C++/WAT pour les boucles. Le passage `chaine` (*const u8) est
  généré, mais non validé sur une telle fonction à cause de cette limite.
* **call_indirect en général**, sections `start`, SIMD, threads, exceptions :
  toujours hors support.
* `fd_read` est un stub sandbox (0 octet), `random_get` n'est pas
  cryptographique, `proc_exit` est une erreur Rose et non un `exit()` système.
* Les WAT complexes (globals, tables, éléments, expressions d'offset
  `global.get`, SIMD) sont hors du petit encodeur intégré.

---

## 6. Non-régression — constats mesurés le 19/11/2026

Binaire reconstruit puis copié dans `rose/rose` avant mesure
(`md5 33ec2e3b587a5c1842e3a874dd3013ef` au moment des mesures, vérifié avant
et après ; d'autres agents compilant en parallèle, le hash doit être re-vérifié
avant toute nouvelle mesure).

| Contrôle | Résultat |
|---|---|
| `cargo build --release --bin rose` | `Finished`, 0 erreur |
| jardin `jardins/tests/premiers.rose` | `20 tests, 0 échec` |
| fib(25) | `75025` |
| point fixe `rose rir compilateur/emetteur.rose \| md5sum` | `bdbef63681608a0b22a5637539e9d738` |
| point fixe `rose compilateur/emetteur.rose compilateur/emetteur.rose \| md5sum` | `bdbef63681608a0b22a5637539e9d738` |
| xxHash (`""`, `"abc"`) | `46947589`, `852579327` |
| FNV-1a (`"abc"`), `text_len` | `440920331`, `3` |
| `cargo test --release` | 160 + 144 + 6 + 1 = **311 passed, 0 failed** (3 cibles vides) |

### 6.1 Mesure de performance — constat NON comparatif

Banc internalisé (`temps()` = ns, 1000 appels `xxh32_str("abc",3,0)` par
échantillon), 6 échantillons consécutifs, même binaire, pinage `taskset -c 8` :

```text
3769655  3772551  3803032  3845751  3899447  4076390   (ns)
```

Médiane (moyenne des deux valeurs centrales) : **3 824 391,5 ns / 1000 appels**
soit **3,824 µs par appel wasm**. Ce chiffre est un constat daté, pas une
comparaison : le binaire antérieur n'a pas été conservé, donc toute affirmation
« aucune régression de vitesse » serait une citation, pas une mesure. Le chemin
chaud (décodage unique au chargement, caches dans `Fonction`) est
structurellement inchangé ; les nouveaux champs de `ModuleWasm` ne sont lus
qu'au chargement et lors d'un `call`.

---

## 7. Fichiers possédés / modifiés

* `rose/src/wasm_lecteur.rs` — registre d'hôtes, imports, correctifs WASI.
* `scripts/rose-wasm-lib.sh` — détection par extension, `--allow-imports`,
  encodeur WAT, bridages Rust/C++/Zig, vérification des imports.
* `rose/docs/WASM_IMPORTS_UNIVERSEL.md` — ce document.

Aucun autre `.rs`, aucun `Cargo.toml`, aucun commit, aucune publication.
