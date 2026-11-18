# Autonomie complète de Rose — route G2 → G3 → G4, état réel et portes vérifiables

Date des constats : **18/11/2026**. Branche `main`, HEAD lu `785b38c` au moment de la
rédaction (`PLAN.md` peut bouger : le relire avant d'agir). Ce document est un
**guide**, pas une constatation de réussite : il dit ce qui est prouvé, ce qui reste,
et la commande qui tranche à chaque porte.

⚠️ **Périmètre** : ce sous-agent n'a écrit que `rose/docs/AUTONOMIE_COMPLETE.md`.
Aucun fichier source, `Cargo.toml`, `PLAN.md` ou fichier d'un autre agent n'a été
modifié. Aucun commit, aucune publication. Aucune dépendance GMP n'est introduite
(`PLAN.md:241-242`, `312`), la release reste **0.1.5** (`PLAN.md:318`), le dépôt reste
**Stef302/Rose**.

**Sources citées** : `NO_STD_MOTEUR.md`, `PRODUIT_SANS_RUST.md`, `NO_STD_CHECKLIST.md`,
`BINAIRE_NO_STD.md`, `AUTOHOST_G4.md`, `rose/Cargo.toml`, `PLAN.md`.

**Convention** : **PROUVÉ** = commande exécutée pour ce guide ou log conservé cité ;
**CONSTAT DATÉ** = état mesuré à une date, à re-mesurer après tout changement ;
**ESTIMÉ** = jugement non mesuré, jamais une promesse.

---

## 0. Carte des portes — état réel au 18/11/2026

| Porte | Contenu | État réel | Preuve / source |
|---|---|---|---|
| **G0** | M4.5 — `rose-2` émet le même RIR octet pour octet que `rose-1` | ✅ **FAIT** | `PLAN.md:50` : `M4.5 point fixe de bootstrap ← FAIT (1f7cd3c, md5 identique 83449e53…)` ; maintenu `PLAN.md:440` : `Point fixe M4.5 maintenu md5 bdbef636…` (deux mesures datées, md5 différents selon le corpus) |
| **G1** | M3.2 — ELF/PE écrits à la main, sans gcc/clang/ld | ✅ **FAIT (avec limite)** | `PLAN.md:44` : `M3.2 link natif (ELF/PE) sans gcc ← FAIT (e554a24+aa52b8c, ELF rend 49, 4 tests)`. Limite : `PRODUIT_SANS_RUST.md:97-100` documente un trampoline **stub qui rend 0** (programmes à runtime refusés) et des `std::fmt`/`std::path` résiduels (`link_natif.rs:25-26`) |
| **G2** | M3.1 câblé comme chemin machine principal **+ retrait de Cranelift** | ⬜ **NON FAIT** | `Cargo.toml:65` contient encore `cranelift = { version = "0.137", … }` ; `cargo tree -e features -i libc` re-joué ci-dessous réactive `libc feature "std"` uniquement par Cranelift |
| **G3** | Moteur Rose entier en `no_std` + `alloc` | ⬜ **NON FAIT** | `i128x.rs` + `mul_rapide.rs` = **270 erreurs** en `no_std` (`NO_STD_MOTEUR.md:241-278`) ; `num-*` encore en features par défaut (`Cargo.toml:54-56`) ; **783 symboles `std` ancrés mesurés dans le binaire `release-dbg` non stripé** (§1.3, commande 4) |
| **G4** | Binaire produit par le compilateur Rose, sans moteur Rust | ⬜ **PARTIEL** | `AUTOHOST_G4.md:97-106` : stub ELF aarch64 prouvé (12/12), **G4 complet NON atteint** — le moteur `rose/rose` exécute encore l'émetteur |

⚠️ `PRODUIT_SANS_RUST.md` (17/11) décrivait G0 comme bloqué (`PRODUIT_SANS_RUST.md:15-19`) :
c'était **vrai à cette date**. `PLAN.md` (18/11) le marque FAIT. C'est un **constat daté**,
pas une vérité permanente : re-mesurer le point fixe md5 avant toute publication.

---

## 1. Partie 1 — État actuel

### 1.1 Ce qui est déjà autonome (PROUVÉ)

| Brique | Fait mesuré | Citation |
|---|---|---|
| **Arithmétique exacte sans GMP** | `i128x.rs` : façade `gmp_*` **maison** sur `BigInt` (pas d'appel GMP) ; `mul_rapide.rs` : NTT Montgomery + arbre de produit équilibré. `1 000 000!` = 5 565 709 chiffres en 200 ms. | `i128x.rs:1-11`, `i128x.rs:660-836`, `mul_rapide.rs:1-3`, `PLAN.md:244-250` ; règle `PLAN.md:241-242` |
| **`sys_io` sans std** | Module compilé **et exécuté** dans une crate réellement `#![no_std]` : `OK sys_io no_std: 51 octets lus, ecrits et verifies`, `libc features=[]`, `nm` ancré = 0. Aucune correction du module n'a été nécessaire. | `NO_STD_CHECKLIST.md:21-67` ; harnais `/tmp/opencode/rose_sys_io_nostd/` |
| **Binaire `no_std` de démonstration** | `#![no_std]` + `alloc`, `argv` sans `std::env`, `sys_io` seul, calcul exact maison sur limbs : `50!` = 65 chiffres, 10 valeurs croisées avec `python3 math.factorial`. | `BINAIRE_NO_STD.md:1-10`, `39-53`, `126-147` ; `rose/src/bin/rose_nostd.rs`. ⚠ **non câblé** : `Cargo.toml:48-51` (conflit `panic=abort`) |
| **Pont wasm zéro dépendance** | `wasm_lecteur.rs` (1 391 lignes, zéro dépendance), `@dll("wasm", …)`, ABI C. `rose wasm lib.wasm` liste les exports. | `PLAN.md:274-282` (commit `115e705`) ; `WASM_FFI.md` |
| **Émetteur machine maison** | `natif_emetteur.rs` traduit le RIR en octets x86-64/aarch64, **sans LLVM ni Cranelift**, 6/6 tests, 1,57 ms (porte ≤ 3 ms). | `PLAN.md:43`, `161-167`, `379` ; module déclaré dans `lib.rs` |
| **Writers ELF / PE / Mach-O** | `link_natif.rs` : ELF64/PE64 écrits à la main (M3.2, ELF rend 49) ; `macho_writer.rs` : structure Mach-O prouvée ; `emetteur_elf.rose` : ELF aarch64 réel écrit **en Rose**. | `PLAN.md:44`, `392`, `478`, `485` ; `AUTOHOST_G4.md:7-58` |
| **`libc` sans feature `std` (déclaration)** | `Cargo.toml:61` = `libc = { version = "0.2", default-features = false }` : la correction M5.3 est appliquée. ⚠ Déclaration ≠ graphe réel (voir 1.2). | `Cargo.toml:61` ; `NO_STD_CHECKLIST.md:71-93` |

### 1.2 Ce qui n'est PAS encore autonome (PROUVÉ)

| Blocage | Mesure | Citation |
|---|---|---|
| **Le binaire livré est Rust + Cranelift + std** | `rose --version` → `Rose 0.1.5 — compilateur Rust + JIT Cranelift (sémantique exacte)`. `file` → `static-pie linked, … stripped`. Le `statically linked` **ne veut pas dire** « sans std » : std est lié **en statique** à l'intérieur. | `PRODUIT_SANS_RUST.md:143-146`, `208-214` ; re-mesuré ci-dessous §1.3 |
| **`std` dans la rlib non stripée** | `nm librose.rlib \| grep -c '3std'` → **868** ; `grep -ci 'std'` → **886** ; `cranelift` → **487** ; `5alloc` → 2029 ; `4core` → 2583. **Dans le binaire `release-dbg` non stripé : `std::` ancré = 783, `3std` = 969, `library/std` = 586, `cranelift` = 2064.** | `PRODUIT_SANS_RUST.md:184-201` ; re-mesuré ci-dessous §1.3, commandes 2 et 4 |
| **Cranelift réinjecte `libc/std`** | `cargo tree --offline -e features -i libc` : `libc feature "default"` → `libc feature "std"` active, uniquement via `cranelift → cranelift-jit → {memmap2, region, wasmtime-internal-jit-icache-coherence}`. **Modifier `Cargo.toml` seul ne suffit pas** : G2 précède G3. | `NO_STD_MOTEUR.md:25-69` ; re-joué ci-dessous §1.3 |
| **`num-*` ramènent `std`** | `Cargo.toml:54,55,60` = `num-bigint = "0.5"`, `num-integer = "0.1"`, `num-traits = "0.2"` : features `default` (= `std`) actives par Rose. | `NO_STD_MOTEUR.md:114-160` ; `PRODUIT_SANS_RUST.md:46-49` |
| **`i128x` / `mul_rapide` ne compilent pas en `no_std`** | 270 erreurs : 125 `E0425` (prelude), 107 `E0433` (`std::`), 6 `E0599`, 32 macros manquantes. Les crates `num-*` compilent (§1.3 de `NO_STD_MOTEUR.md`) : ce sont **les deux modules Rose** qui bloquent. | `NO_STD_MOTEUR.md:234-293` |
| **`catch_unwind` / `Any` — blocage architectural** | `lib.rs:197` (`std::panic::catch_unwind`), `lib.rs:291` (`dyn std::any::Any`), `run.rs:168-169` (`SystemTime::now`). `panic="abort"` (exigé par le binaire `no_std`) est **incompatible** avec `catch_unwind`. | `NO_STD_CHECKLIST.md:169-189`, `NO_STD_MOTEUR.md:332` ; `Cargo.toml:88-97` |
| **Portée du code à porter** | Comptage `std::` re-mesuré le 18/11 : `interp.rs` 161, `setup_core.rs` 143, `setup_gui.rs` 117, `jit.rs` 104, `run.rs` 69, `main.rs` 67, `mul_rapide.rs` 42, `i128x.rs` 15, `lib.rs` 4. | `NO_STD_MOTEUR.md:337-344` |
| **G4 incomplet** | L'émetteur Rose refuse `app `, `appel`, `saut`, `(bin` et toute seconde fonction (rc=2) ; pas de `_start`/trampolines, pas de runtime bigints, pas de PE, `e_shoff=0`. | `AUTOHOST_G4.md:49-95`, `103-106` |

### 1.3 Mesures re-jouées pour CE guide (sorties exactes)

Environnement : `cd /home/clausake/Stefan/ROSE/rose`, `export PATH="/home/clausake/.cargo/bin:$PATH"`,
`rustc 1.99.1`, `cargo 1.99.1`, `uname -m = aarch64`, builds `--offline`.

**Commande 1 — le graphe de features (preuve que G2 précède G3) :**

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
│   │           │   └── rose v0.1.5 (...) (*)
│   │           ├── cranelift feature "jit"
│   │           │   └── rose v0.1.5 (...) (*)
│   │           ├── cranelift feature "module"
│   │           │   └── rose v0.1.5 (...) (*)
│   │           └── cranelift feature "native"
│   │               └── rose v0.1.5 (...) (*)
│   ├── memmap2 v0.9.12      └── memmap2 feature "default" (*)
│   ├── region v3.0.2        └── region feature "default" (*)
│   └── wasmtime-internal-jit-icache-coherence v50.0.1
│       └── ... feature "default" (*)
└── libc feature "std"
    └── libc feature "default" (*)
RC=0
```
(les `(...)` abrègent le chemin et le `( * )` les répétitions ; sortie complète identique
à `NO_STD_MOTEUR.md:32-60`). **Lecture** : `Cargo.toml:61` est déjà corrigé, mais
`libc feature "std"` reste **actif** : seule la chaîne Cranelift le réinjecte.

**Commande 2 — le binaire livré, strip et ses symboles :**

```
$ file target/release/rose
target/release/rose: ELF 64-bit LSB pie executable, ARM aarch64, version 1 (SYSV), static-pie linked, BuildID[sha1]=01a689c59c2f81335586e57045ef91ffde8ac891, for GNU/Linux 3.7.0, stripped
$ nm target/release/rose
nm: target/release/rose: no symbols
$ ./target/release/rose --version
Rose 0.1.5 — compilateur Rust + JIT Cranelift (sémantique exacte)
$ nm target/release/librose.rlib | grep -c '3std'      → 868
$ nm target/release/librose.rlib | grep -ci 'std'      → 886
$ nm target/release/librose.rlib | grep -c 'cranelift' → 487
```
Le `0` que donnerait `nm … | grep -c 'std::'` sur ce binaire est un **artefact de strip**,
jamais une preuve (`PRODUIT_SANS_RUST.md:163-167`, `NO_STD_CHECKLIST.md:195-211`).
Le BuildID diffère de celui de `PRODUIT_SANS_RUST.md:160` (`613315bf…`) : le binaire a
été reconstruit le 18/11 13:31 ; c'est un **constat daté**.

**Commande 3 — témoin positif, binaire NON stripé (démo no_std) :**

```
$ file /tmp/opencode/rose_nostd_demo/target/release/rose_nostd
... dynamically linked, ... with debug_info, not stripped
$ nm -C /tmp/opencode/rose_nostd_demo/target/release/rose_nostd | wc -l                     → 61
$ nm -C /tmp/opencode/rose_nostd_demo/target/release/rose_nostd | grep -E '(^|[^A-Za-z0-9_])std::' | wc -l → 0
```
`nm` voit bien 61 symboles : la porte est **valide**. C'est la forme que devra avoir la
porte du moteur complet, pas l'artefact `no symbols`.

**Commande 4 — le VRAI binaire du moteur, build `release-dbg` non stripé (exécuté pour ce guide) :**

```
$ cargo build --profile release-dbg --offline
    Finished `release-dbg` profile [optimized + debuginfo] target(s) in 36.28s
BUILD_RC=0                      (227 warnings, principalement `non_snake_case` + messages de ld)

$ file target/release-dbg/rose
target/release-dbg/rose: ELF 64-bit LSB pie executable, ARM aarch64, version 1 (GNU/Linux), static-pie linked, BuildID[sha1]=faea7baeb4c6865b627ad4b473270609cac09bb4, for GNU/Linux 3.7.0, with debug_info, not stripped

$ nm -C target/release-dbg/rose | wc -l                                          → 11112
$ nm -C target/release-dbg/rose | grep -E '(^|[^A-Za-z0-9_])std::' | wc -l        → 783
$ nm     target/release-dbg/rose | grep -c '3std'                                → 969
$ nm     target/release-dbg/rose | grep -cE '_?_ZN3std'                          → 0   ⚠ FAUX NÉGATIF
$ strings target/release-dbg/rose | grep -c 'library/std'                       → 586
$ strings target/release-dbg/rose | grep -c 'rustc/'                            → 722
$ nm -C target/release-dbg/rose | grep -ci cranelift                            → 2064
$ strings target/release-dbg/rose | grep -ci cranelift                          → 45787
$ ldd target/release-dbg/rose                                                    → statically linked
$ ./target/release-dbg/rose --version
Rose 0.1.5 — compilateur Rust + JIT Cranelift (sémantique exacte)
```

**Deux leçons mesurées ici, à insérer dans toute porte :**

1. Le motif `_?_ZN3std` de `NO_STD_CHECKLIST.md:223` et `NO_STD_MOTEUR.md:367` donne
   **0 alors que 783 symboles `std` sont présents** : rustc 1.99.1 utilise le
   **mangling Rust v0** (`…3std…`), pas l'ancien `_ZN3std…`. Le motif valide est
   `grep -c '3std'` (969 ici). Une porte qui n'utiliserait que `_ZN3std` accepterait
   à tort un binaire plein de `std`.
2. `strings | grep -ci cranelift` = **45 787** sur le build avec debug info : les
   chemins de source et les métadonnées DWARF gonflent le comptage. La porte G2 doit
   porter sur `nm -C | grep -ci cranelift` (2064 ici) **et** sur le graphe `cargo tree`,
   pas sur un seul comptage `strings` non qualifié. Le chiffre 117 de
   `PRODUIT_SANS_RUST.md:179` était mesuré sur le binaire **stripé** : ce ne sont pas
   la même mesure, ne pas les comparer.

Cette mesure **remplace** l'artefact de strip : le moteur livré contient bien `std` et
Cranelift, prouvés cette fois sur un binaire où `nm` voit 11 112 symboles.

---

## 2. Partie 2 — Route G2 → G3 → G4 en étapes vérifiables

### 2.0 Protocole de mesure — six règles à ne pas enfreindre

1. **Build non stripé** : `cargo build --profile release-dbg` (profil déjà présent,
   `Cargo.toml:80-83`, `strip = false`) ou `cargo rustc -- -C strip=none`. Vérifier
   `file` (pas `stripped`) et `nm BIN | wc -l > 0` **avant** tout comptage. Piège mesuré :
   sur le binaire stripé, `nm` répond « no symbols » et `grep -c` donne 0
   (`PRODUIT_SANS_RUST.md:163-167`, `NO_STD_CHECKLIST.md:195-211`).
2. **Regex ancrée, ET motif de mangling à jour** : `nm -C BIN | grep -E '(^|[^A-Za-z0-9_])std::' | wc -l`
   **et** `nm BIN | grep -c '3std'`. Le `grep -c 'std::'` non ancré donne un faux positif
   réel : `rose_nostd::factorielle_limbs` contient la sous-chaîne `std::`
   (`BINAIRE_NO_STD.md:183-194` ; mesure re-jouée : **1** au lieu de 0). Inversement,
   `grep -cE '_?_ZN3std'` (motif des docs du 17/11) est un **faux négatif** mesuré sur le
   build `release-dbg` : **0** alors que `std::` ancré = 783 et `3std` = 969. Ne jamais
   utiliser `_ZN3std` seul comme porte.
3. **`library/std` seul, pas `rustc/`** : sur un harnais `no_std` sain,
   `strings | grep -ciE 'library/std|rustc/'` donne **46-47** faux positifs — ce sont des
   chemins DWARF `/rustc/<hash>/library/{core,alloc,compiler-builtins}/src/…`
   (`NO_STD_MOTEUR.md:350-353`, `BINAIRE_NO_STD.md:212-228`). `strings | grep -c 'library/std'` doit valoir 0.
4. **Témoin négatif A/A** : garder le build std de contrôle avec la feature `nostd`
   (`#![cfg_attr(feature = "nostd", no_std)]` dans `lib.rs`) tant que G3 n'est pas fini,
   et comparer l'oracle G0 à chaque étape (`NO_STD_MOTEUR.md:341-344`).
5. **Feature unification** : une seule dépendance qui active `std` le réactive pour
   tout le graphe. Après chaque changement : `cargo tree -f '{p} features=[{f}]' | grep -E 'libc|num-'`
   (`NO_STD_CHECKLIST.md:87-93`).
6. **Oracle avant vitesse** : ne jamais annoncer un gain de temps avant d'avoir
   re-mesuré ; `NO_STD_MOTEUR.md:292-293` demande explicitement de re-mesurer `n!`
   si le repli séquentiel remplace `std::thread`.

### 2.1 G2 — retirer Cranelift (PRÉREQUIS DUR, avant toute porte `nm` du moteur)

| Étape | Action | Commande de mesure | Attendu |
|---|---|---|---|
| **G2.1** | Câbler `natif_emetteur` comme chemin machine principal ; garder Cranelift en feature de secours temporaire (`NO_STD_MOTEUR.md:321`) | test API + `fib machine` | `fib machine = 75025`, 6/6 tests, ≤ 3 ms (`PLAN.md:161-167`, `394`) — **ESTIMÉ** tant que non re-joué |
| **G2.2** | Supprimer `Cargo.toml:65`, les `use cranelift`/types Cranelift dans `jit.rs`/`natif_emetteur.rs`, régénérer `Cargo.lock` | `cargo tree --offline -e features -i libc` | plus de branche `libc feature "default"/"std"` : seulement `libc v0.2.191 └── rose` ; `cargo tree -f '{p} features=[{f}]' \| grep libc` → `features=[]` |
| **G2.3** | Nettoyer `natif_emetteur.rs` (23 `std::`), `link_natif.rs` (19) : `std::fmt`→`core::fmt`, `std::io`→`sys_io`, `std::path::Path`→comparaison d'octets (`NO_STD_MOTEUR.md:323`) | `nm -C target/release-dbg/rose \| grep -ci cranelift` ; `strings … \| grep -ci cranelift` | **0** et **0** (avant, mesuré sur `release-dbg` : **2064 symboles / 45 787 chaînes**, §1.3 commande 4 ; les 487/117 de `PRODUIT_SANS_RUST.md:179,194` venaient d'autres binaires) |

⚠ Piège : G2.2 sans G2.1 casse `rose orc`/le JIT livré. ⚠ `inkwell` (`Cargo.toml:62`) est
optionnel (feature `llvm`) : vérifier qu'aucune porte ne le réactive par surprise.

### 2.2 G3 — moteur complet `no_std` + `alloc` (après G2)

| Étape | Action | Commande de mesure | Attendu / état |
|---|---|---|---|
| **G3.1** | `Cargo.toml:54,55,60` : `default-features = false` sur `num-bigint`/`num-integer`/`num-traits` ; garder la feature `i128` | `cargo tree -f '{p} features=[{f}]' \| grep -E 'num-'` | `num-bigint features=[]`, `num-traits features=[i128]`, **aucun `std`**. **PROUVÉ en isolateur** (`NO_STD_MOTEUR.md:162-232`), non appliqué au dépôt |
| **G3.2** | `sys_io::linux` : `fchmod`, `mkdir`, `stat`, `unlink`, `rename`, `readlink`, `clock_gettime` (SYS 113 aarch64) + façade `lire_ligne_stdin` + allocateur mmap | harnais `no_std` isolé qui appelle chaque syscall et compare octet pour octet | module de base **PROUVÉ** (`NO_STD_CHECKLIST.md:21-67`), extensions **ESTIMÉES** |
| **G3.3** | Porter `i128x.rs`/`mul_rapide.rs` : `alloc::{vec::Vec, vec, boxed::Box, string::{String,ToString}, format, collections::BinaryHeap}`, `std::cmp`→`core::cmp`, `HashMap`→`BTreeMap`, `OnceLock`→table à l'amorçage, `thread`→repli séquentiel ou façade pthread via `libc` | le harnais `/tmp/opencode/rose_i128x_nostd/` doit compiler et afficher `OK i128x no_std`, puis re-mesurer `n!` | **270 erreurs PROUVÉES** ; correctifs **ESTIMÉS** (`NO_STD_MOTEUR.md:283-293`) |
| **G3.4** | `run.rs:168-169` `SystemTime::now()`→`sys_io::linux::clock_gettime` ; `lib.rs:197/291` `catch_unwind`/`Any`→propagation `Result` | `cargo check` sans `std` sur la lib seule ; `essayer`/`attraper` sur le corpus doré | **blocage architectural ESTIMÉ** (`NO_STD_MOTEUR.md:332`) — le plus gros risque sémantique (`NO_STD_MOTEUR.md:415-417`) |
| **G3.5** | Porter `main.rs`, `interp.rs`, `setup_core.rs`, `setup_gui.rs`, `run.rs` vers `sys_io` + pointeurs `argv` | `grep -o 'std::' fichier \| wc -l` = 0 hors commentaires, fichier par fichier | inventaire **PROUVÉ** (§1.2), portage **ESTIMÉ** |
| **G3.6** | Amorçage du VRAI binaire : `#![no_std]`, `#![no_main]`, `#[global_allocator]` mmap, `#[panic_handler]`, `extern "C" fn main`, stub `rust_eh_personality` | binaire non stripé qui démarre et exécute un programme Rose réel | mécanique **PROUVÉE** sur `rose_nostd.rs` et dans `/tmp/opencode/rose_nostd_demo/` (`BINAIRE_NO_STD.md:41-53`) |
| **G3.7** | Profil non stripé + porte complète | voir §2.4 | **ESTIMÉ** |

**Pièges G3 déjà payés, à ne pas rejouer :**
- `alloc::format!` casse le lien en `panic="abort"` : `undefined reference to _Unwind_Resume`
  (liballoc **précompilé** du sysroot, pas `num-bigint`). Stub
  `#[no_mangle] pub extern "C" fn _Unwind_Resume() -> ! { loop {} }`, puis RC=0
  (`NO_STD_MOTEUR.md:209-219`).
- `rust_eh_personality` : référence DWARF de `liballoc`, **même avec `panic="abort"`** ;
  stub `#[unsafe(no_mangle)] pub extern "C" fn rust_eh_personality() {}`
  (`NO_STD_CHECKLIST.md:228-247`).
- `panic="abort"` doit rester **confiné au binaire autonome** : le moteur live garde
  `unwind` tant que `catch_unwind` n'a pas été remplacé (`NO_STD_MOTEUR.md:384-386`,
  `Cargo.toml:88-97`). Le binaire démo n'est pas câblé précisément pour ça (`Cargo.toml:48-51`).
- `std` n'est **pas** la crate `libc` : un binaire `#![no_std]` peut appeler
  `libc::malloc` sans réintroduire `std` (`NO_STD_CHECKLIST.md:270-274`).
- Numéros de syscall codés en dur pour **aarch64** (`openat=56`, `close=57`, `read=63`,
  `write=64`, `munmap=215`, `mmap=222`, `clock_gettime=113`) : à confronter à `libc::*`
  pour toute nouvelle cible (`NO_STD_CHECKLIST.md:262-268`).

### 2.3 G4 — binaire émis par le compilateur Rose

| Étape | Action | Commande de mesure | Attendu / état |
|---|---|---|---|
| **G4.0** | **Déjà fait** : `emetteur_elf.rose` lit un dump RIR texte et écrit un ELF64 `ET_EXEC` aarch64 réel | `./rose/rose rir /tmp/p47.rose > /tmp/p47.rir` ; `./rose/rose compilateur/emetteur_elf.rose /tmp/p47.rir > /tmp/test.elf` ; `chmod +x && /tmp/test.elf; echo $?` | **47**, `file` = ELF statique, 12 valeurs octet-identiques (`AUTOHOST_G4.md:14-58`). ⚠ sous-ensemble minuscule |
| **G4.1** | Porter en Rose `link_natif::stub_depart` (réservation `n_slots*8`, `base=sp`, `tramp`, `bl`) et l'abaissement `bin`/`affect`/`retour_abstrait` en AAPCS64 | RIR contenant `bin`, `appel`, `saut` doit produire un ELF qui tourne, au lieu de `rc=2` | aujourd'hui **refusé explicitement** (`AUTOHOST_G4.md:60-62`, `80-85`) — **ESTIMÉ** |
| **G4.2** | Intégrer le runtime : `eval_fallback`, bigints exacts **sans GMP** | mêmes programmes que l'oracle G0, sortie à l'octet près | `link_natif.rs` rend 0 sur les programmes à runtime (`AUTOHOST_G4.md:86-88`) — **ESTIMÉ** |
| **G4.3** | Sortie PE64 / Windows et structure ELF complète (`.data`, `.rodata`, symboles) | `readelf -S` non vide, PE vérifié | `e_shoff=0` aujourd'hui (`AUTOHOST_G4.md:89-95`) — **ESTIMÉ** |
| **G4.4** | Porte finale : le binaire auto-hébergé remplace le binaire livré | script §2.4 | `rose --version` rendu **par le binaire émis par Rose** ; `strings` sans `rustc/`/`library/std`/`cranelift` ; `ldd` sans bibliothèque Rust ; `file` = ELF exécutable (`PLAN.md:265`, `PRODUIT_SANS_RUST.md:258-265`) |

### 2.4 Script de porte unique (à exécuter tel quel, après chaque jalon)

```bash
export PATH="/home/clausake/.cargo/bin:$PATH"
cd /home/clausake/Stefan/ROSE/rose

# 0. build NON stripé — sinon tous les comptages ci-dessous sont des artefacts
cargo build --profile release-dbg --offline
file target/release-dbg/rose                 # ne doit PAS dire "stripped"
nm -C target/release-dbg/rose | wc -l        # DOIT être > 0

# 1. std — trois mesures croisées, regex ANCRÉE
nm -C target/release-dbg/rose | grep -E '(^|[^A-Za-z0-9_])std::' | wc -l   # → 0
nm     target/release-dbg/rose | grep -c '3std'                            # → 0  (Rust v0)
# ⚠ NE PAS utiliser `grep -cE '_?_ZN3std'` : faux négatif mesuré (0 avec 783 std)
strings target/release-dbg/rose | grep -c 'library/std'                   # → 0
# NE PAS utiliser `rustc/` seul (faux positifs DWARF), ni grep non ancré

# 2. Cranelift (G2) — condition de validité de la porte 1
nm -C target/release-dbg/rose | grep -ci cranelift                         # → 0
# `strings | grep -ci cranelift` est gonflé par la DWARF (45 787 avant G2) :
# l'exiger à 0 aussi, mais ne PAS le comparer au 117 mesuré sur le binaire stripé

# 3. graphe de features (piège de feature unification)
cargo tree --offline -e features -i libc
# → aucune branche libc feature "default"/"std"
cargo tree -f '{p} features=[{f}]' | grep -E 'libc|num-'
# → libc features=[], num-bigint features=[], num-traits features=[i128]

# 4. dépendances dynamiques + oracle
ldd target/release-dbg/rose          # libc.so.6 seul, ou statique sans libstd
./target/release-dbg/rose --version  # Rose 0.1.5, identique à l'oracle G0
./target/release-dbg/rose <programme_du_corpus>   # sortie identique à l'oracle
nm target/release-dbg/librose.rlib | grep -c '3std'       # témoin secondaire → 0
```

Refuser la porte si `file` dit `stripped`, si `nm` répond « no symbols », si un comptage
est non nul, si `ldd` montre une bibliothèque Rust, ou si la sortie diffère de l'oracle
(`NO_STD_MOTEUR.md:383-386`, `NO_STD_CHECKLIST.md:333-335`).

✅ Le `cargo build --profile release-dbg --offline` a été **exécuté pour ce guide**
(RC=0, 36,28 s) : les valeurs **de départ** citées en commentaire (45 787, 783, 2064, …)
sont réellement observées avant tout travail G2/G3/G4. Les `→ 0` restent la **cible**,
pas un résultat.

---

## 3. Partie 3 — Ce que ça change pour l'utilisateur

### 3.1 Ce que ça CHANGE

- **Un seul binaire, zéro dépendance de runtime** : ni Rust std, ni GMP, ni JIT
  Cranelift, ni linker externe, ni runtime WASM obligatoire. Le produit final est
  émis par le compilateur Rose (`PLAN.md:262-265`). `ldd` ne montre plus que `libc.so.6`,
  ou rien de Rust.
- **Démarrage plus léger** : suppression du std statique, de la chaîne
  `cranelift-jit → {memmap2, region, icache-coherence}` et de l'amorçage Rust. C'est un
  **mécanisme attendu**, pas un chiffre : la valeur doit être re-mesurée avec
  `PLAFOND_VITESSE.md` comme référence (démarrage/natif/JIT/interp déjà mesurés,
  `PLAN.md:485`). Ne pas annoncer de gain avant mesure.
- **Distribution universelle** : une cible wasm sans dépendance (pont maison
  `wasm_lecteur.rs`, `PLAN.md:274-282`, guide `UNIVERSEL_WASM.md`) + writers ELF/PE/Mach-O
  (`PLAN.md:44`, `478`). Un seul artefact théoriquement déployable partout là où un
  binaire ou un module wasm tourne.
- **Auto-hébergement réel** : le compilateur se reproduit (G0, `PLAN.md:50`) et, à G4,
  se compile lui-même sans que le moteur Rust soit livré — exactement le chemin Zig
  décrit `PLAN.md:26-27`, `181-187`.

### 3.2 Ce que ça NE change PAS

- **Le langage reste le même** : mêmes opérateurs symboliques `!`, `!!`, `!!!`, `#`, `?`,
  `$`, préfixe `!` (`PLAN.md:12-15`, `314-316`).
- **La syntaxe reste française** : `si`, `tant_que`, `afficher`, `fonction`, `var`, etc. ;
  la sortie d'exécution reste la **valeur seule**, LaTeX seulement via `rose latex` ou
  `%%` (`PLAN.md:316`).
- **L'arithmétique exacte reste identique** : entiers illimités, rationnels réduits,
  `38? = 741` (`PLAN.md:317`), aucune perte en virgule flottante.
- **GMP reste interdit** comme dépendance (`PLAN.md:241-242`, `312`) : « sans Rust » ne
  doit jamais devenir « avec GMP ».
- **Release 0.1.5** inchangée (`PLAN.md:318`), dépôt **Stef302/Rose** inchangé.
- **Le moteur Rust reste dans l'arbre de source** comme amorce de bootstrap, il sort
  seulement du produit livré (`PRODUIT_SANS_RUST.md:264-265`).

---

## 4. Verdict honnête — temps et étapes (ESTIMÉ, non mesuré)

Aucune date n'est promise. Les fourchettes ci-dessous sont des **ordres de grandeur
ESTIMÉS**, hypothèse : un agent travaillant à plein temps sur le chantier, sans
régression à corriger. Elles ne sont **pas** des mesures et doivent être révisées après
chaque porte.

| Porte | Étapes | Temps ESTIMÉ | Incertitude principale |
|---|---|---|---|
| G2 | 3 (G2.1 → G2.3) | quelques heures à **~2 jours** | câblage `natif_emetteur` sans casser les 150 oracles ; retrait de Cranelift dans `Cargo.lock` |
| G3 | 7 (G3.1 → G3.7) | **~1 à 3 semaines** | G3.4 `catch_unwind` → `Result` (sémantique des erreurs runtime) et G3.3 (270 erreurs, perfs `n!` à re-mesurer) |
| G4 | 4 (G4.1 → G4.4) | **~plusieurs semaines à quelques mois** | abaissement RIR général en Rose + runtime bigints sans GMP ; G4.0 ne couvre qu'un stub `mov/svc` |
| **Total G2→G4** | **14 étapes** | **~plusieurs semaines à quelques mois ESTIMÉS** | le queue est G4, pas G2 |

**Verdict** : avec les moyens actuels, le produit livré reste dépendant de Rust
(preuve §1.3). Le chemin est **réel et balisé** — chaque jalon a une commande qui tranche
(§2.4) — mais il n'existe **aucune mesure** permettant d'affirmer que G3 ou G4 seront
atteints dans un délai donné. Les deux limites que l'utilisateur veut franchir
(« autonomie » et « vitesse ») ne se franchissent pas ensemble : G2-G3 donnent
l'autonomie, la vitesse doit être **re-mesurée** à chaque porte, et `PLAN.md:485` montre
déjà que Rose reste 4,05× derrière Rust AOT sur `fib(35)` malgré les gains mesurés.
L'autonomie n'implique donc pas automatiquement « plus rapide que Rust ».

---

## 5. Limites de ce guide

* Ce document ne modifie aucun source, aucun `Cargo.toml`, aucun `PLAN.md` : il ne
  **réalise** aucune des étapes G2/G3/G4. Aucun commit.
* Les sorts `cargo tree`, `file`, `nm`, `rose --version` du §1.3 ont été **exécutés** pour
  ce guide ; les autres chiffres sont cités aux documents d'origine avec leur date.
* Le build `release-dbg` du §2.4 a été **réellement exécuté** (RC=0) et ses comptages
  sont au §1.3 « Commande 4 » : la porte « nm du binaire complet » est donc **mesurée**
  à l'état initial. Elle devra être **re-jouée après chaque étape** G2/G3/G4 pour
  montrer la progression vers 0.
* État mesuré le **18/11/2026** : toute modification de `jit.rs`/`rir.rs` (agent I1), de
  `Cargo.toml` ou de `PLAN.md` rend ces constats périmés. Re-mesurer avant d'agir.
* Aucun jardin en Rust, aucune dépendance GMP, release 0.1.5, GitHub `Stef302/Rose`.
