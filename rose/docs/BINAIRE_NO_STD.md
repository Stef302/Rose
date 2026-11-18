# G3 / M5.3 — binaire de démonstration RÉELLEMENT `no_std`

Statut au 18/11/2026 : **réussi, preuves à l'appui**, pour une démonstration
d'un calcul exact exécuté par un micro-bytecode dans un binaire `#![no_std]`.

⚠️ **Ce document ne dit PAS que Rose entier compile en `no_std`.** Il ne dit
PAS non plus que « Rose fonctionne sans Rust ». Il atteste un fait précis et
mesurable : un exécutable `#![no_std]` + `alloc` peut démarrer, lire `argv`
sans `std::env`, écrire sur stdout via `sys_io` seul, exécuter un petit
programme encodé en dur et produire une factorielle exacte sur des limbs.

---

## 1. Ce qui a été livré

| Fichier | Rôle |
|---------|------|
| `rose/src/bin/rose_nostd.rs` | binaire `#![no_std]` + `alloc` (unique fichier source ajouté) |
| `rose/docs/BINAIRE_NO_STD.md` | ce document |

Aucun autre fichier du dépôt n'a été modifié. En particulier `Cargo.toml`,
`main.rs`, `lib.rs`, `interp.rs`, `run.rs`, `PLAN.md` sont intacts. Le câblage
`[[bin]]` dans `rose/Cargo.toml` est à faire par l'orchestrateur :

```toml
[[bin]]
name = "rose_nostd"
path = "src/bin/rose_nostd.rs"
# PAS de required-features = ["native"] : ce binaire ne dépend d'aucune
# feature du moteur. Il utilise uniquement libc (déjà sans std par défaut
# depuis la correction M5.3, voir NO_STD_CHECKLIST.md §2).
```

Le binaire inclut `sys_io.rs` par `#[path = "../sys_io.rs"]` : depuis
`src/bin/rose_nostd.rs`, ce chemin désigne bien `src/sys_io.rs`.

---

## 2. Ce que fait le binaire

### 2.1 Amorçage `no_std` complet

Les cinq éléments du piège `NO_STD_CHECKLIST.md` §4.5 sont présents :

- `#![no_std]` + `#![no_main]` ;
- `#[global_allocator]` : `malloc`/`free` de `libc` (disponibles sans la
  feature `std` de la crate `libc`, cf. §4.7 de la checklist) ;
- `#[panic_handler]` : écrit sur fd 2 puis `exit_group(101)` ;
- `#[unsafe(no_mangle)] extern "C" fn main(argc, argv) -> i32` : point
  d'entrée C, **aucun `std::env`** — `argv` est parcouru à la main
  (`arg_bytes`, pointeurs NUL-terminés) ;
- `#[unsafe(no_mangle)] pub extern "C" fn rust_eh_personality() {}` : stub
  indispensable, cf. §4.3 de la checklist (référence DWARF de `liballoc`).

`edition = "2024"` est utilisée dans le harnais pour être exactement dans la
même configuration que le crate `rose` (attributs `unsafe(...)` acceptés).

### 2.2 Micro-bytecode — « programme pré-analysé encodé en dur »

```rust
const OP_HALT: u8 = 0;
const OP_LIRE_ARG: u8 = 1;  // argv[1] → registre R0 (u64)
const OP_FACT: u8 = 2;      // R0 → registre R1 (entier exact, limbs base 2^32)
const OP_ECRIRE: u8 = 3;    // R1 → stdout en décimal
const PROGRAMME: &[u8] = &[OP_LIRE_ARG, OP_FACT, OP_ECRIRE, OP_HALT];
```

La boucle `while pc < PROGRAMME.len() { match PROGRAMME[pc] { … } }` est une
vraie boucle d'interprétation : elle décode des opcodes et dispatchte. Ce
n'est **pas** le bytecode de Rose (pas de frontal, pas de parser, pas
d'`interp.rs`, pas de `run.rs`) : ces 4 opcodes sont schématiques et servent
uniquement à montrer la forme « un moteur exécute un programme ».

### 2.3 Calcul exact maison (et pourquoi pas `i128x` / `mul_rapide`)

La consigne demandait d'essayer `i128x`/`mul_rapide` d'abord. Verdict par
lecture des sources (non modifiées) :

- `src/i128x.rs` : `use num_bigint`, `num_integer`, `num_traits`,
  `std::cmp::Ordering`, `std::collections::BinaryHeap`,
  `std::thread::available_parallelism`, `std::thread::scope`, et
  `crate::run::erreur_runtime` ;
- `src/mul_rapide.rs` : `use num_bigint::{BigInt, BigUint, Sign}`,
  `num_integer`, `num_traits`, `std::sync::OnceLock`, `std::sync::Mutex`,
  `std::sync::Condvar`, `std::sync::Arc`, `std::thread::scope`.

→ Ces deux modules **ne compilent pas en `#![no_std]`** en l'état. On ne
peut pas les inclure dans ce binaire sans modifying des fichiers interdits.

Le binaire implémente donc un calcul exact minimal :

- `factorielle_limbs(n)` : nombre en base 2^32, limbs little-endian dans un
  `Vec<u32>`, multiplications successives par `k = 2..=n` avec retenue `u64` ;
- `ecrire_exact_decimal` : divisions répétées par 10^9, 9 chiffres par
  groupe avec rembourrage à gauche (`reste << 32` tient en `u64` car
  `reste < 10^9`) ;
- aucune dépendance externe, aucun GMP, aucun `num-bigint`.

`n = 50` tient en 7 limbs ; `n = 500` a aussi été vérifié (1135 chiffres).

---

## 3. Preuves — sorties exactes

Harnais isolé : `/tmp/opencode/rose_nostd_demo/`

```
/tmp/opencode/rose_nostd_demo/
├── Cargo.toml                 # edition 2024, libc sans features, strip=false, panic=abort
├── src/sys_io.rs              # copie exacte de rose/src/sys_io.rs (non modifiée)
└── src/bin/rose_nostd.rs      # copie exacte du fichier livré
```

### 3.1 Build

```
$ export PATH="/home/clausake/.cargo/bin:$PATH"
$ cd /tmp/opencode/rose_nostd_demo && cargo build --release --offline
   Compiling rose_nostd v0.1.5 (/tmp/opencode/rose_nostd_demo)
    Finished `release` profile [optimized] target(s) in 0.13s
BUILD_RC=0
```

Aucun warning.

### 3.2 Exécution — valeur exacte

```
$ ./target/release/rose_nostd 50
30414093201713378043612608166064768844377641568960512000000000000
RUN_RC=0

$ ATTENDU=30414093201713378043612608166064768844377641568960512000000000000
$ OBTENU=$(./target/release/rose_nostd 50)
$ [ "$OBTENU" = "$ATTENDU" ] && echo EGALITE_EXACTE=OUI
EGALITE_EXACTE=OUI
nb_chiffres=65
```

Oracle croisé `python3 math.factorial` (10 valeurs) :

```
n=0 OK (1 chiffres)     n=1 OK (1 chiffres)     n=2 OK (1 chiffres)
n=5 OK (3 chiffres)     n=10 OK (7 chiffres)    n=20 OK (19 chiffres)
n=30 OK (33 chiffres)   n=50 OK (65 chiffres)   n=100 OK (158 chiffres)
n=500 OK (1135 chiffres)
```

Cas d'erreur (terminaison propre) :

```
$ ./target/release/rose_nostd
usage: rose_nostd <entier>
RC=2
$ ./target/release/rose_nostd abc
argument invalide (entier decimal attendu)
RC=2
```

### 3.3 `nm` — zéro symbole `std` (binaire NON stripé)

```
$ file target/release/rose_nostd
ELF 64-bit LSB pie executable, ARM aarch64, version 1 (SYSV), dynamically linked,
interpreter /lib/ld-linux-aarch64.so.1, ... with debug_info, not stripped

$ nm -C target/release/rose_nostd | wc -l
61                                   ← > 0 : la porte nm est VALIDE

$ nm -C BIN | grep -E '(^|[^A-Za-z0-9_])std::' | wc -l
0
$ nm -C BIN | grep -P '(?<![A-Za-z0-9_])std::' | wc -l
0
$ nm     BIN | grep -cE '_?_ZN3std'
0
$ nm -D -C BIN | grep -E '(^|[^A-Za-z0-9_])std::' | wc -l
0
$ nm BIN | grep rust_eh_personality
0000000000020008 V DW.ref.rust_eh_personality
0000000000001608 T rust_eh_personality
```

**Piège rejoué et vérifié** (`NO_STD_CHECKLIST.md` §4.2) : le `grep` NON
ancré donne **1** :

```
$ nm -C BIN | grep -c 'std::'
1
$ nm -C BIN | grep 'std::'
0000000000000840 t rose_nostd::factorielle_limbs
```

C'est `rose_nostd::` (nom de crate finissant par `nostd`) qui produit la
sous-chaîne `std::` — faux positif. La regex ancrée est obligatoire.

### 3.4 `ldd` / `readelf` / `cargo tree`

```
$ ldd target/release/rose_nostd
    linux-vdso.so.1 (0x0000fd106db4a000)
    libc.so.6 => /lib/aarch64-linux-gnu/libc.so.6 (0x0000fd106d910000)
    /lib/ld-linux-aarch64.so.1 (0x0000fd106db00000)

$ readelf -d BIN | grep NEEDED
 0x0000000000000001 (NEEDED)  Shared library: [libc.so.6]

$ cargo tree --offline -f '{p} features=[{f}]'
rose_nostd v0.1.5 (/tmp/opencode/rose_nostd_demo) features=[]
└── libc v0.2.191 features=[]          ← AUCUNE feature std
```

### 3.5 Piège `strings` — documenté pour éviter un faux positif

```
$ strings BIN | grep -ciE 'library/std|rustc/'
46
$ strings BIN | grep -c 'library/std'
0
$ strings BIN | grep -c 'rustc/'
46
```

Les 46 occurrences de `rustc/` sont des **chemins DWARF du sysroot** vers
`/rustc/<hash>/library/{core,alloc,compiler-builtins}/src/…`, pas vers
`library/std`. Le motif combiné `library/std|rustc/` de la checklist §6
doit donc être lu avec discernement sur un binaire non stripé : c'est
`library/std` seul qui est le marqueur de `std`, et il vaut **0** ici.
Les symboles (`nm`) et `ldd` restent les preuves primaires.

---

## 4. Ce que CE binaire PROUVE

1. Un exécutable Rust `#![no_std]` + `alloc` démarre et termine proprement sur
   cette machine (`aarch64`, Rust 1.99.1) avec un point d'entrée C.
2. `rose/src/sys_io.rs` suffit à écrire sur stdout/stderr : aucun `std::io`,
   aucun `std::fs`, aucun `std::env`.
3. Le graphe de dépendances peut être limité à `libc` **sans feature `std`**
   (`cargo tree` → `features=[]`).
4. Un binaire non stripé peut avoir **0 symbole `std`** (regex ancrée,
   trois mesures croisées) et `ldd` = **libc.so.6 seul**.
5. Un **calcul exact** (factorielle, 65 chiffres pour n=50) est réalisable
   sans GMP ni `num-bigint`, avec un allocateur `malloc`/`free`.
6. Une boucle d'interprétation de bytecode fonctionne en `no_std` — sur un
   micro-bytecode de 4 opcodes.

## 5. Ce que CE binaire NE PROUVE PAS

1. **Il n'exécute aucun programme Rose réel.** Le `PROGRAMME` est encodé en
   dur (4 opcodes schématiques). Il n'y a ni lexeur, ni parser, ni AST, ni
   `front.rs`, ni `rir`, ni les opcodes de `interp.rs`.
2. **Le moteur Rose complet ne compile toujours pas en `no_std`.** Les
   blocages inventoriés dans `NO_STD_CHECKLIST.md` §2-§3 restent entiers :
   `main.rs` (std::fs/io/time/env/process/thread/panic), `interp.rs`
   (12 `std::fs`, 5 `std::io`), `run.rs` (`SystemTime`), `lib.rs`
   (`catch_unwind`, `Any`), `i128x.rs`/`mul_rapide.rs` (num-* + std::thread),
   et la feature unification via `num-*`/`cranelift`.
3. **Ce n'est pas « Rose sans Rust » ni « un binaire émis par le compilateur
   Rose ».** C'est un binaire Rust compilé par `rustc`, avec un calcul exact
   maison. La porte G4 (« binaire émis par le compilateur Rose ») n'est pas
   concernée.
4. Il ne prouve pas l'absence de `std` dans le futur binaire `rose` complet :
   G2 (retrait de Cranelift, 487 symboles) doit être fait **avant** pour que
   la porte `nm` du moteur complet ait un sens.
5. Il ne teste pas les E/S fichier du moteur : seul `ecrire_tout` de
   `sys_io.rs` est exercé (le reste du module est inclus mais inutilisé —
   d'où le `#[allow(dead_code)]` sur la déclaration de module).
6. Limite d'échelle : factorielle par `Vec<u32>` croissant, sans l'arbre de
   produits ni la NTT de `mul_rapide.rs`. Au-delà de quelques milliers,
   c'est trop lent pour être utile — la démonstration s'arrête à l'exactitude.

---

## 6. Chemin restant vers G4

```
G3 (ce document)  ✅ binaire no_std de démonstration (calcul exact maison)
   │
   ├─ G2 d'abord : retirer Cranelift du binaire rose (487 symboles)
   ├─ libc sans std : déjà fait dans Cargo.toml par l'orchestrateur (M5.3)
   ├─ ajouter à sys_io::linux : fchmod, mkdir, stat, unlink, rename,
   │  readlink, clock_gettime + façade lire_ligne_stdin
   ├─ porter main.rs (6 std::fs, 5 std::io, 1 std::time, std::env/process/
   │  thread/panic) — NO_STD_CHECKLIST.md §3.1
   ├─ porter interp.rs (12 std::fs, 5 std::io) — §3.2
   ├─ porter run.rs:168-169 (SystemTime → clock_gettime), lib.rs:197/291
   │  (catch_unwind/Any), puis i128x.rs et mul_rapide.rs
   ├─ amorçage du VRAI binaire rose (les 5 éléments du §4.5) + strip=false
   └─ porte §6 de NO_STD_CHECKLIST.md : nm + ldd + oracle G0 identique
        │
G4  ✅/⬜ binaire ÉMIS par le compilateur Rose (pas encore abordé ici)
```

Ordre impératif : **G2 avant la porte `nm` du moteur complet**, sinon les
487 symboles Cranelift rendent la mesure ininterprétable. Le présent binaire
n'a aucun symbole Cranelift puisqu'il n'a aucune dépendance Cranelift.

---

## 7. Limite de portée

Ce rapport atteste uniquement les mesures de la section 3, reproductibles
dans `/tmp/opencode/rose_nostd_demo/`. Aucun commit n'a été fait. Aucun
fichier interdit n'a été touché. Le câblage `[[bin]]` et toute modification
de `Cargo.toml` restent à l'orchestrateur.
