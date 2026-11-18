# G3 — plan d'exécution CONCRET pour rendre le moteur Rose complet `no_std`

Statut au **18/11/2026**. Ce document est une **feuille de route avec première
étape réellement validée**, pas une constatation de réussite globale.

⚠️ **Périmètre** : ce sous-agent n'a créé que ce fichier. Aucun fichier source,
`Cargo.toml`, `PLAN.md` ni fichier d'un autre agent n'a été modifié ; aucun
commit n'a été fait. Les expériences ont été faites **uniquement sous
`/tmp/opencode/`**. ⚠️ **Ce document ne promet PAS « Rose sans Rust »** : c'est
le jalon **G4** (binaire émis par le compilateur Rose), hors de portée ici.

Environnement de toutes les mesures : `rustc 1.99.1 (58cf8f9c3 2026-10-13)`,
`cargo 1.99.1 (5f94df478 2026-08-27)`, `uname -m` = `aarch64`, cache crates
`~/.cargo/registry/cache`, builds `--offline`.

Convention de citation : les sorties sont reproduites telles quelles ; `...`
marque une ligne abrégée pour la lisibilité (adresse de bibliothèque, chemin
de crate). Les logs complets sont conservés dans les harnais `/tmp/opencode/`
et dans `/tmp/opencode/m53_tree_*.txt`.

---

## 1. Ce qui est PROUVÉ par exécution (preuves reproductibles)

### 1.1 `libc` : le `std` est réinjecté par Cranelift, pas par `Cargo.toml`

Commande demandée, sortie exacte (`cd /home/clausake/Stefan/ROSE/rose`,
`export PATH="/home/clausake/.cargo/bin:$PATH"`) :

```
$ cargo tree --offline -e features -i libc
libc v0.2.191
└── rose v0.1.5 (/home/clausake/Stefan/ROSE/rose)
    ├── rose feature "default" (command-line)
    └── rose feature "native"
        └── rose feature "default" (command-line)
├── libc feature "default"
│   ├── cranelift-jit v0.137.1
│   │   └── cranelift-jit feature "default"
│   │       └── cranelift v0.137.1
│   │           ├── cranelift feature "frontend"
│   │           │   └── rose v0.1.5 (/home/clausake/Stefan/ROSE/rose) (*)
│   │           ├── cranelift feature "jit"
│   │           │   └── rose v0.1.5 (/home/clausake/Stefan/ROSE/rose) (*)
│   │           ├── cranelift feature "module"
│   │           │   └── rose v0.1.5 (/home/clausake/Stefan/ROSE/rose) (*)
│   │           └── cranelift feature "native"
│   │               └── rose v0.1.5 (/home/clausake/Stefan/ROSE/rose) (*)
│   ├── memmap2 v0.9.12
│   │   └── memmap2 feature "default"
│   │       └── cranelift-jit v0.137.1 (*)
│   ├── region v3.0.2
│   │   └── region feature "default"
│   │       └── cranelift-jit v0.137.1 (*)
│   └── wasmtime-internal-jit-icache-coherence v50.0.1
│       └── wasmtime-internal-jit-icache-coherence feature "default"
│           └── cranelift-jit v0.137.1 (*)
└── libc feature "std"
    └── libc feature "default" (*)
RC=0
```

**Lecture** : `rose/Cargo.toml:57` contient **déjà**
`libc = { version = "0.2", default-features = false }` (correction appliquée par
l'orchestrateur depuis la checklist du 17/11). Pourtant `libc feature
"default"` → `libc feature "std"` est **actif**, uniquement à cause de la chaîne
`cranelift → cranelift-jit → {memmap2, region, wasmtime-internal-jit-icache-coherence}`.
Conclusion **prouvée** : la porte G3 (`libc features=[]`) est **impossible tant
que G2 n'a pas retiré Cranelift**. Modifier `Cargo.toml` seul ne suffit pas.

### 1.2 `num-*` et Cranelift dans le graphe

```
$ cargo tree --offline | grep -E 'num-|cranelift'
├── cranelift v0.137.1
│   ├── cranelift-codegen v0.137.1
│   │   ├── cranelift-assembler-x64 v0.137.1
│   │   │   └── cranelift-assembler-x64-meta v0.137.1
│   │   │       └── cranelift-srcgen v0.137.1
│   │   ├── cranelift-bforest v0.137.1
│   │   │   ├── cranelift-entity v0.137.1
│   │   │   │   ├── cranelift-bitset v0.137.1
│   │   ├── cranelift-bitset v0.137.1 (*)
│   │   ├── cranelift-codegen-shared v0.137.1
│   │   ├── cranelift-control v0.137.1
│   │   ├── cranelift-entity v0.137.1 (*)
│   │   ├── cranelift-codegen-meta v0.137.1
│   │   │   ├── cranelift-assembler-x64-meta v0.137.1 (*)
│   │   │   ├── cranelift-codegen-shared v0.137.1
│   │   │   ├── cranelift-srcgen v0.137.1
│   │   └── cranelift-isle v0.137.1
│   ├── cranelift-frontend v0.137.1
│   │   ├── cranelift-codegen v0.137.1 (*)
│   ├── cranelift-jit v0.137.1
│   │   ├── cranelift-codegen v0.137.1 (*)
│   │   ├── cranelift-control v0.137.1 (*)
│   │   ├── cranelift-entity v0.137.1 (*)
│   │   ├── cranelift-module v0.137.1
│   │   │   ├── cranelift-codegen v0.137.1 (*)
│   │   │   ├── cranelift-control v0.137.1 (*)
│   │   ├── cranelift-native v0.137.1
│   │   │   ├── cranelift-codegen v0.137.1 (*)
│   ├── cranelift-module v0.137.1 (*)
│   └── cranelift-native v0.137.1 (*)
├── num-bigint v0.5.1
│   ├── num-integer v0.1.47
│   │   └── num-traits v0.2.19
│   └── num-traits v0.2.19 (*)
├── num-integer v0.1.47 (*)
└── num-traits v0.2.19 (*)
$ ... | grep -cE 'num-|cranelift'      → 36
```

Origine exacte des features `std` :

```
$ cargo tree --offline -e features -i num-bigint
num-bigint v0.5.1
├── num-bigint feature "default"
│   └── rose v0.1.5 (/home/clausake/Stefan/ROSE/rose)
│       ├── rose feature "default" (command-line)
│       └── rose feature "native"
│           └── rose feature "default" (command-line)
└── num-bigint feature "std"
    └── num-bigint feature "default" (*)

$ cargo tree --offline -e features -i num-traits
num-traits v0.2.19
├── num-traits feature "default"
│   └── rose v0.1.5 (/home/clausake/Stefan/ROSE/rose)
│       ├── rose feature "default" (command-line)
│       └── rose feature "native"
│           └── rose feature "default" (command-line)
├── num-traits feature "i128"
│   ├── num-bigint v0.5.1
│   │   ├── num-bigint feature "default"
│   │   │   └── rose v0.1.5 (/home/clausake/Stefan/ROSE/rose) (*)
│   │   └── num-bigint feature "std"
│   │       └── num-bigint feature "default" (*)
│   └── num-integer v0.1.47
│       ├── num-integer feature "default"
│       │   └── rose v0.1.5 (/home/clausake/Stefan/ROSE/rose) (*)
│       ├── num-integer feature "i128"
│       │   └── num-bigint v0.5.1 (*)
│       └── num-integer feature "std"
│           ├── num-bigint feature "std" (*)
│           └── num-integer feature "default" (*)
└── num-traits feature "std"
    ├── num-bigint feature "std" (*)
    ├── num-integer feature "std" (*)
    └── num-traits feature "default" (*)
```

Lecture : `default` et `std` viennent de Rose ; `i128` est une feature utile
à conserver (elle est demandée par `num-bigint` et `num-integer`).

`Cargo.toml:50,51,56` = `num-bigint = "0.5"`, `num-integer = "0.1"`,
`num-traits = "0.2"` : les trois features `default` (= `std`) sont activées par
Rose. Le levier est donc `default-features = false` sur ces trois lignes — et
**ce levier est validé en 1.3**.

### 1.3 ✅ LEVIER VALIDÉ : `num-bigint = { version="0.5", default-features=false }` compile et tourne en `no_std`

Harnais réel : **`/tmp/opencode/rose_num_bigint_nostd_min/`**
(`Cargo.toml` : `num-bigint`/`num-integer`/`num-traits`/`libc` tous
`default-features = false` ; `src/main.rs` : `#![no_std]`, `#![no_main]`,
`extern crate alloc;`, allocateur `malloc`/`free` de `libc`, `#[panic_handler]`,
stub `rust_eh_personality`, calcul `2^256 - 1` **sans `format!`**).

```
$ cd /tmp/opencode/rose_num_bigint_nostd_min && cargo build --release --offline
   Compiling autocfg v1.5.1
   Compiling libc v0.2.191
   Compiling num-traits v0.2.19
   Compiling num-integer v0.1.47
   Compiling num-bigint v0.5.1
   Compiling rose_num_bigint_nostd v0.1.5
    Finished `release` profile [optimized] target(s) in 1.28s
BUILD_RC=0

$ ./target/release/rose_num_bigint_nostd
OK num-bigint no_std (sans format!): bits=256
RUN_RC=0

$ cargo tree --offline -f '{p} features=[{f}]'
rose_num_bigint_nostd v0.1.5 (...) features=[]
├── libc v0.2.191 features=[]
├── num-bigint v0.5.1 features=[]
│   ├── num-integer v0.1.47 features=[i128]
│   │   └── num-traits v0.2.19 features=[i128]
│   │       [build-dependencies]
│   │       └── autocfg v1.5.1 features=[]
│   └── num-traits v0.2.19 features=[i128] (*)
├── num-integer v0.1.47 features=[i128] (*)
└── num-traits v0.2.19 features=[i128] (*)

$ file target/release/rose_num_bigint_nostd
ELF 64-bit LSB pie executable, ARM aarch64, ... with debug_info, not stripped
$ ldd target/release/rose_num_bigint_nostd
	linux-vdso.so.1 (...)
	libc.so.6 => /lib/aarch64-linux-gnu/libc.so.6 (...)
	/lib/ld-linux-aarch64.so.1 (...)
$ nm -C target/release/rose_num_bigint_nostd | grep -E '(^|[^A-Za-z0-9_])std::' | wc -l → 0
$ nm     target/release/rose_num_bigint_nostd | grep -cE '_?_ZN3std'            → 0
```

**Deux pièges mesurés, à ne pas rejouer** :

1. **`alloc::format!` casse le lien en `panic="abort"`.** Variante identique
   avec `format!("{}", n)` (`/tmp/opencode/rose_num_bigint_nostd/`) :
   ```
   /usr/bin/ld: liballoc-*.rlib(...): in function `alloc::fmt::format::format_inner':
       undefined reference to `_Unwind_Resume'
   collect2: error: ld returned 1 exit status
   BUILD_RC=101
   ```
   Ajout de `#[no_mangle] pub extern "C" fn _Unwind_Resume() -> ! { loop {} }`
   → `BUILD_RC=0`, exécution : `OK num-bigint no_std: chiffres=78`. Le défaut
   vient du `liballoc` **précompilé** du sysroot, pas de `num-bigint`.
2. **Témoin négatif (contrôle A/B)** : même harnais avec `num-bigint = "0.5"`
   (features par défaut) → **échec** :
   ```
   error[E0152]: found duplicate lang item `panic_impl`
     = note: the lang item is first defined in crate `std` (which `num_bigint` depends on)
   BUILD_RC=101
   $ cargo tree -f '{p} features=[{f}]'
   ├── num-bigint v0.5.1 features=[default,std]
   │   ├── num-integer v0.1.47 features=[i128,std]
   │   └── num-traits v0.2.19 features=[i128,std]
   ```
   Donc `default-features = false` est **nécessaire et suffisant** pour
   `num-bigint` : c'est une première étape G3 réellement validée.

### 1.4 ❌ `i128x.rs` isolé avec `num-*` sans std : ÉCHEC (erreur exacte)

Harnais réel : **`/tmp/opencode/rose_i128x_nostd/`**. Il inclut
`#[path = ".../src/i128x.rs"] mod i128x;` **et** `mul_rapide.rs` (dépendance
directe de `i128x`), stub de `crate::run::erreur_runtime`, `num-*` sans std.

```
$ cd /tmp/opencode/rose_i128x_nostd && cargo build --release --offline
   Compiling num-bigint v0.5.1 ... (OK)
   Compiling rose_i128x_nostd v0.1.5
error[E0433]: cannot find module or crate `std` in this scope
   --> /home/clausake/Stefan/ROSE/rose/src/mul_rapide.rs:763:13
    |
763 |         use std::sync::atomic::Ordering::{AcqRel, Acquire, Relaxed, Release};
    |             ^^^ use of unresolved module or unlinked crate `std`
error[E0433]: cannot find module or crate `std` in this scope
   --> /home/clausake/Stefan/ROSE/rose/src/mul_rapide.rs:1534:13
1534 |         use std::sync::Arc;
error[E0433]: cannot find module or crate `std` in this scope
   --> /home/clausake/Stefan/ROSE/rose/src/mul_rapide.rs:2016:9
2016 |     use std::collections::HashMap;
error[E0433]: cannot find module or crate `std` in this scope
  --> /home/clausake/Stefan/ROSE/rose/src/i128x.rs:9:5
9 | use std::cmp::Ordering;
error[E0433]: cannot find module or crate `std` in this scope
   --> /home/clausake/Stefan/ROSE/rose/src/i128x.rs:211:5
211 | use std::collections::BinaryHeap;
error: cannot find macro `format` in this scope
   --> /home/clausake/Stefan/ROSE/rose/src/i128x.rs:629:56
629 |         if d == BigInt::one() { n.to_string() } else { format!("{}/{}", n, d) }
BUILD_RC=101
error: could not compile `rose_i128x_nostd` (bin "rose_i128x_nostd")
       due to 270 previous errors; 3 warnings emitted
```

Répartition exacte des 270 erreurs (sortie complète conservée dans
`/tmp/opencode/rose_i128x_nostd/build_output.txt`) :

| Catégorie | Compte |
|---|---|
| `E0425` (`Vec`, `Box`, `String` du prelude manquants) | 125 |
| `E0433` (`std::` non résolu) | 107 |
| `E0599` (`to_string`, `f64::fract`, `to_u64` : traits hors scope) | 6 |
| `cannot find macro` (`vec!` ×30, `format!` ×2) | 32 |
| **Total** | **270** (= 125 + 107 + 6 + 32) |

Les diagnostics citent `i128x.rs` 77 fois et `mul_rapide.rs` 195 fois (272
références : certains diagnostics citent aussi un fichier du sysroot).

**Verdict PROUVÉ** : `i128x.rs` + `mul_rapide.rs` **ne compilent PAS** en
`no_std` même avec `num-bigint` sans std. Les crates `num-*` compilent (voir
1.3) : ce sont **les deux modules Rose** qui bloquent. Correctifs **ESTIMÉS**
(non testés) : `use alloc::{vec::Vec, vec, boxed::Box, string::{String,ToString}, format, collections::BinaryHeap}` ;
`std::cmp`/`std::mem` → `core::cmp`/`core::mem` ; `std::sync::OnceLock`
(`mul_rapide.rs:593`) → table calculée à l'amorçage ou `static mut` synchronisé
; `std::thread::scope`/`available_parallelism` → repli séquentiel ou façade
pthread via `sys_io`/`libc` ; `std::sync::{Mutex,Condvar,atomic}` → futex
`libc::syscall` ; `HashMap` → `alloc::collections::BTreeMap` ; `f64::fract()` →
`num_traits::float::FloatCore`. ⚠ Le repli séquentiel changerait les perfs de
`n!` : à **re-mesurer**, jamais promettre le « 1 000 000 ! en 200 ms » sans test.

### 1.5 Repère déjà existant, re-mesuré

`src/bin/rose_nostd.rs` (non câblé dans `Cargo.toml`) démontre un binaire
`#![no_std]` complet avec `sys_io` et un micro-bytecode. Harnais
`/tmp/opencode/rose_nostd_demo/` re-joué le 18/11 :

```
$ ./target/release/rose_nostd 50
30414093201713378043612608166064768844377641568960512000000000000
RUN_RC=0
```

Ce n'est **pas** le moteur : ni `front.rs`, ni `interp.rs`, ni `run.rs`,
ni `i128x.rs`, ni `mul_rapide.rs` n'y sont inclus (voir `BINAIRE_NO_STD.md`).

---

## 2. Ordre d'exécution RÉEL G2 → G3

G2 est un **prérequis dur** : la preuve 1.1 montre que Cranelift seul
réactive `libc feature "std"`. G3 ne doit pas être joué avant G2.

### G2 — retirer Cranelift (détenu par d'autres agents ; ESTIMÉ)

| # | Fichiers à modifier | Action | Porte de sortie |
|---|---|---|---|
| G2.1 | `rose/src/jit.rs`, `rose/src/natif_emetteur.rs` | Câbler le backend natif (`natif_emetteur`) comme chemin machine principal ; conserver le chemin Cranelift seulement sous une feature de secours temporaire | `fib machine = 75025` mesuré (ESTIMÉ) |
| G2.2 | `rose/Cargo.toml:61` (`cranelift = {...}`), `rose/src/jit.rs`, `Cargo.lock` | Supprimer la dépendance et tous les `use cranelift`/types Cranelift ; régénérer le lock | `cargo tree -e features -i libc` ne montre plus QUE `rose` + `libc features=[]` (PROUVÉ que c'est la cause) ; `strings … \| grep -ci cranelift` 117→0 ; `nm … \| grep -ci cranelift` 487→0 (cibles ESTIMÉES) |
| G2.3 | `rose/src/natif_emetteur.rs` (23 `std::`), `rose/src/link_natif.rs` (19) | `std::fmt`→`core::fmt`, `std::io`→`sys_io`, `std::path::Path`→comparaison d'octets | `grep -n 'std::'` = 0 dans ces deux fichiers (hors commentaires) |

### G3 — moteur complet `no_std` + `alloc` (après G2)

| # | Fichiers à modifier | Action | Porte de sortie | Statut |
|---|---|---|---|---|
| G3.1 | `rose/Cargo.toml:50,51,56` | `num-bigint = { version="0.5", default-features=false }`, idem `num-integer`/`num-traits` ; ajouter `[[bin]] rose_nostd` (sans `required-features`) | `cargo tree -f '{p} features=[{f}]'` → `num-bigint features=[]`, `num-traits features=[i128]` sans `std` | **PROUVÉ** en isolateur (§1.3), non appliqué au dépôt |
| G3.2 | `rose/src/sys_io.rs` | Ajouter à `sys_io::linux` : `fchmod`, `mkdir`, `stat`, `unlink`, `rename`, `readlink`, `clock_gettime` (SYS 113 aarch64) + façade `lire_ligne_stdin` + allocateur mmap | Harnais `no_std` isolé qui appelle chaque nouveau syscall et compare octet pour octet | ESTIMÉ (module de base PROUVÉ le 17/11) |
| G3.3 | `rose/src/i128x.rs`, `rose/src/mul_rapide.rs` | Appliquer la liste de correctifs §1.4 ; remplacer `std::thread` par un repli séquentiel **mesuré** | Le harnais `/tmp/opencode/rose_i128x_nostd/` doit compiler et afficher `OK i128x no_std` ; puis re-mesurer `n!` | ÉCHEC actuel PROUVÉ (270 erreurs), correctifs ESTIMÉS |
| G3.4 | `rose/src/run.rs` (`temps_unix_nanos`, 168-169), `rose/src/lib.rs` (197 `catch_unwind`, 291 `Any`) | `SystemTime::now()`→`sys_io::linux::clock_gettime` ; remplacer `catch_unwind` par une propagation `Result` (le binaire `no_std` exige `panic="abort"`, incompatible avec `catch_unwind`) | `cargo check` sans `std` sur la lib seule ; `essayer`/`attraper` garde la même sémantique sur le corpus doré | ESTIMÉ — **blocage architectural** |
| G3.5 | `rose/src/interp.rs` (161 `std::` re-mesurés), `rose/src/main.rs` (67) | Porter `std::fs`/`std::io`/`std::env`/`std::process`/`std::thread` vers `sys_io` + pointeurs `argv` (cf. checklist §3.1-3.2) | `grep -o 'std::' \| wc -l` = 0 hors commentaires, fichier par fichier | ESTIMÉ (inventaire PROUVÉ par re-mesure) |
| G3.6 | `rose/src/main.rs`, `rose/src/lib.rs` | Amorçage : `#![no_std]`/`#![no_main]`, `#[global_allocator]` (mmap), `#[panic_handler]`, `extern "C" fn main`, stub `rust_eh_personality` (déjà dans `rose_nostd.rs`) | Binaire non stripé qui démarre et exécute un programme Rose réel | ESTIMÉ (mécanique PROUVÉE sur `rose_nostd`) |
| G3.7 | `rose/Cargo.toml` (profil), outillage de validation | Build non stripé + porte corrigée §3 | Toutes les mesures de la §3 à 0 ; oracle G0 inchangé | ESTIMÉ |

**Comptage `std::` re-mesuré le 18/11/2026** (diffère de la checklist du
17/11, les fichiers ayant bougé) : `interp.rs` 161, `setup_core.rs` 143,
`setup_gui.rs` 117, `jit.rs` 104, `run.rs` 69, `main.rs` 67,
`mul_rapide.rs` 42, `i128x.rs` 15, `lib.rs` 4, `sys_io.rs` 6 (commentaires),
`bin/rose_nostd.rs` 6 (commentaires). Stratégie conseillée : introduire une
feature `nostd` avec `#![cfg_attr(feature = "nostd", no_std)]` dans `lib.rs`
pour garder un build std de contrôle à chaque étape, puis ne basculer
`main.rs` qu'une fois la lib propre.

---

## 3. Porte de validation G3 — VERSION CORRIGÉE

La porte de `NO_STD_CHECKLIST.md` §6 contient une erreur : elle exige
`strings … | grep -ciE 'library/std|rustc/'` = 0. **Mesuré sur un harnais
propre** (§1.3) : `library/std` = **0** mais `rustc/` = **47**, ce sont des
chemins de debug `/rustc/<hash>/library/{core,alloc}/src/…` — pas du `std`.
La porte doit donc être :

```bash
export PATH="/home/clausake/.cargo/bin:$PATH"
cd /home/clausake/Stefan/ROSE/rose

# 1. build NON STRIPÉ (le profil release-dbg existe déjà : Cargo.toml:76-79)
cargo build --profile release-dbg
file target/release-dbg/rose
nm -C target/release-dbg/rose | wc -l        # DOIT être > 0

# 2. absence de std — regex ANCRÉE (piège "nostd::" du §4.2 de la checklist)
nm -C target/release-dbg/rose | grep -E '(^|[^A-Za-z0-9_])std::' | wc -l   # → 0
nm     target/release-dbg/rose | grep -cE '_?_ZN3std'                      # → 0
strings target/release-dbg/rose | grep -ci 'library/std'                  # → 0
# NE PAS utiliser `rustc/` seul : 47 faux positifs dans un harnais no_std sain.

# 3. absence de Cranelift (G2)
nm -C target/release-dbg/rose | grep -ci cranelift                         # → 0

# 4. graphe de features
cargo tree -f '{p} features=[{f}]' | grep -E 'libc|num-'
# → libc features=[], num-bigint features=[], num-traits features=[i128]

# 5. exécution / oracle
ldd target/release-dbg/rose          # libc.so.6 seul, ou statique sans libstd Rust
./target/release-dbg/rose --version  # Rose 0.1.5, sortie identique à l'oracle G0
```

Refuser la porte si `file` dit `stripped`, si `nm` répond « no symbols », si un
comptage est non nul, ou si la sortie diffère de l'oracle. ⚠ `panic="abort"`
doit rester **confiné au binaire autonome** ; le moteur live garde `unwind`
tant que `catch_unwind` n'a pas été remplacé (G3.4).

---

## 4. Récapitulatif PROUVÉ / ESTIMÉ

| Fait | Statut |
|---|---|
| `libc default/std` réactivé uniquement par la chaîne Cranelift (1.1) | **PROUVÉ** (`cargo tree -e features -i libc`, RC 0) |
| `num-bigint`/`num-integer`/`num-traits` activent `std` depuis `Cargo.toml` (1.2) | **PROUVÉ** (`cargo tree -e features -i …`) |
| `num-bigint 0.5` sans std compile, lie et s'exécute en `no_std` (1.3) | **PROUVÉ** (build RC 0, run RC 0, `nm` 0, `ldd` libc seul) |
| Avec features par défaut, `std` revient et casse le `panic_handler` (1.3) | **PROUVÉ** (E0152, contrôle A/B) |
| `format!` sans stub `_Unwind_Resume` casse le lien, pas num-bigint (1.3) | **PROUVÉ** (undefined reference, puis RC 0 avec stub) |
| `i128x.rs` + `mul_rapide.rs` échouent en `no_std` (1.4) | **PROUVÉ** (270 erreurs, log conservé) |
| Correctifs précis pour `i128x`/`mul_rapide`, G2.1-G2.3, G3.2-G3.7 | **ESTIMÉ** (non appliqués, non testés) |
| « Rose entier compile no_std » | **NON PROUVÉ** — cible, pas résultat |
| « Rose sans Rust » | **HORS PORTÉE** — jalon G4 |

---

## 5. Limites

* Un seul fichier du dépôt a été écrit : `rose/docs/NO_STD_MOTEUR.md`. Les
  harnais sous `/tmp/opencode/` ne sont pas des livrables du dépôt. Aucun
  commit.
* Ce plan suppose G0 (point fixe M4.5) et G1 (link natif M3.2) validés par
  les agents concernés ; il ne les reprend pas.
* Aucune dépendance GMP n'est introduite, aucun jardin Rust n'est créé, la
  version annoncée reste 0.1.5, le dépôt reste `Stef302/Rose`.
* Le passage de `interp.rs`/`lib.rs` au `no_std` change la sémantique des
  erreurs runtime (`catch_unwind` → `Result`) : c'est le plus gros risque du
  chantier et il doit être validé sur l'oracle avant toute porte `nm`.
