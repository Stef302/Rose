# G3 — checklist « no_std + alloc seul »

Statut au 17/11/2026 : **préparation concrète faite pour `sys_io` seul**. Rose
entier **ne compile PAS** encore en `no_std` : ce document est la feuille de
route, pas la constatation d'une réussite globale.

Portes du document `PRODUIT_SANS_RUST.md` §3 :

| Porte | Contenu | État |
|-------|---------|------|
| G0 | M4.5 — compilateur auto-hébergé, point fixe octet pour octet | ✅ fait |
| G1 | M3.2 — écrivain ELF/PE natif sans gcc/clang/ld | ✅ fait |
| G2 | M3.1 câblé + retrait de Cranelift (117 chaînes / 487 symboles) | ⬜ à faire |
| G3 | `no_std` + `alloc` seul dans Rose | ⬜ **cible de cette checklist** |
| G4 | binaire émis par le compilateur Rose | ⬜ à faire |

---

## 1. Ce qui est PROUVÉ (M5.3, 17/11/2026)

`rose/src/sys_io.rs` (504 lignes + note d'en-tête M5.3) compile **et tourne**
dans une crate réellement `#![no_std]` dont l'unique dépendance est
`libc = { version = "0.2", default-features = false }`.

Harnais isolé : `/tmp/opencode/rose_sys_io_nostd/`
(`Cargo.toml`, `src/main.rs` avec `#[path = "/home/clausake/Stefan/ROSE/rose/src/sys_io.rs"] mod sys_io;`).

Commandes et sorties exactes :

```
$ export PATH="/home/clausake/.cargo/bin:$PATH"
$ cd /tmp/opencode/rose_sys_io_nostd && cargo build --release --offline
    Finished `release` profile [optimized] target(s) in 0.16s
BUILD_RC=0

$ ./target/release/rose_sys_io_nostd
OK sys_io no_std: 51 octets lus, ecrits et verifies
RUN_RC=0

$ cargo tree --offline -f '{p} features=[{f}]'
rose_sys_io_nostd v0.1.5 (/tmp/opencode/rose_sys_io_nostd) features=[]
└── libc v0.2.191 features=[]          ← AUCUNE feature `std`

$ file target/release/rose_sys_io_nostd
ELF 64-bit LSB pie executable, ARM aarch64, ... dynamically linked,
... with debug_info, not stripped

$ ldd target/release/rose_sys_io_nostd
    linux-vdso.so.1 (...)
    libc.so.6 => /lib/aarch64-linux-gnu/libc.so.6 (...)
    /lib/ld-linux-aarch64.so.1 (...)
$ ldd ... | grep -c libstd  → 0
$ readelf -d ... | grep NEEDED
  (NEEDED) Shared library: [libc.so.6]

$ nm -C target/release/rose_sys_io_nostd | grep -E '(^|[^A-Za-z0-9_])std::' | wc -l
0
$ nm target/release/rose_sys_io_nostd | grep -cE '_?_ZN3std'
0
```

Le harnais exerce : `ecrire_fichier` → `lire_fichier` (51 octets, comparaison
octet pour octet), `openat`/`read`/`close` bruts, `mmap` anonyme 4096 octets +
`munmap`. Fichier produit : `/tmp/opencode/rose_sys_io_no_std_test.bin` (51 o).

**Aucune correction de `sys_io.rs` n'a été nécessaire** : le module était déjà
propre. Seul un paragraphe de constat M5.3 a été ajouté à son en-tête.

---

## 2. Changement `Cargo.toml` à faire (par l'orchestrateur, APRÈS ce rapport)

Fichier : `rose/Cargo.toml`, **ligne 52**.

```toml
# AVANT
libc = "0.2"

# APRÈS
libc = { version = "0.2", default-features = false }
```

Raison : `libc` a `default = ["std"]` ; tant que la ligne n'est pas changée, le
binaire final embarque `std` **même si `sys_io.rs` est propre**. Ce changement
n'a PAS été fait ici (fichier interdit à ce sous-agent).

⚠ **Feature unification** : une seule autre dépendance qui active la feature
`std` de `libc` réactive `std` pour tout le graphe. Vérifier après coup avec :

```
cargo tree -f '{p} features=[{f}]' | grep libc
# attendu : └── libc v0.2.191 features=[]
```

---

## 3. Blocages restants — lignes EXACTES (non modifiées)

Relevés par lecture de `src/main.rs`, `src/lib.rs`, `src/interp.rs` le
17/11/2026. Les numéros ci-dessous sont ceux du fichier **actuel** ; ils
dérivent de quelques lignes par rapport à `NO_STD.md` §4.2-4.3 (ex.
`main.rs:612` y était annoncé, c'est `614` aujourd'hui) — toujours re-vérifier
par `grep -n` avant d'éditer.

### 3.1 `src/main.rs` — 6 `std::fs`, 3 `std::io::` + 2 `io::` (via `use std::io::{self,…}`), 1 `std::time`

`grep -c` confirme : `std::fs` = 6, `std::time` = 1, `std::env` = 10,
`std::process` = 40.

| Ligne | Usage | Équivalent `sys_io` |
|-------|-------|---------------------|
| 42 | `use std::io::Read;` | supprimer (trait de `read_to_string`) |
| 45 | `std::fs::File::open(chemin)` | `sys_io::ouvrir(chemin, false)` |
| 46 | `f.read_to_string(source)` | `sys_io::lire_fichier` + `String::from_utf8` |
| 554 | `std::fs::read(&chemin)` (`lire_bundle`) | `sys_io::lire_fichier` |
| 587 | `std::fs::read(c)` (`comp_programme`) | `sys_io::lire_fichier` |
| 591 | `std::fs::File::create(sortie)` | manque : `openat` avec `O_WRONLY\|O_CREAT\|O_TRUNC` (déjà dans `linux`) |
| 595 | `use std::io::Write;` | supprimer |
| 596 | `out.write_all(&moteur)` | `sys_io::ecrire_tout` |
| 597 | `out.write_all(source.as_bytes())` | `sys_io::ecrire_tout` |
| 602 | `out.write_all(&q)` | `sys_io::ecrire_tout` |
| 606 | `use std::os::unix::fs::PermissionsExt;` | supprimer |
| 607 | `std::fs::set_permissions(sortie, ...0o755)` | manque : `linux::fchmod` |
| 614 | `std::time::Instant::now()` | manque : `linux::clock_gettime` (SYS 113 sur aarch64) |
| 619 | `_t_start.elapsed()` | idem |
| 624 | `_t_start.elapsed()` | idem |
| 905 | `std::fs::read(&fichier)` (`wasm`) | `sys_io::lire_fichier` |
| 988 | `_t_start.elapsed()` | idem |
| 1005 | `_t_start.elapsed()` | idem |
| 1053 | `use std::io::{self, Write, BufRead};` | supprimer |
| 1108 | `io::stdout().flush()` | inutile si sortie par `write(2)` brut |
| 1110 | `io::stdin().lock().read_line(&mut buf)` | manque : lecture stdin par `sys_io::lire(0, …)` |

Autres `std` de `main.rs` hors périmètre strict mais bloquants pour G3 :
`std::env` (234, 548, 636, 979, 982, 1026, 1055, 1064-1066…), `std::process`
(40 occurrences : `exit`, `Command`, `Stdio`), `std::thread` (650, 1002),
`std::panic::set_hook` (628).

### 3.2 `src/interp.rs` — 12 `std::fs`, 5 `std::io`, 0 `std::time` direct

| Ligne | Usage |
|-------|-------|
| 595 | `std::fs::read(chemin)` — chargeur WASM |
| 868 | `std::io::stdin().read_line(&mut buf)` (STEP) |
| 1575 | `use std::io::BufRead;` (`lire_selon`) |
| 1576 | `std::io::stdin()` (`lire_selon`) |
| 3283 | `std::io::stdin().read_line` (pause) |
| 3309 | commentaire `std::time::SystemTime` (pas de code) |
| 3312 | `crate::run::temps_unix_nanos()` — **indirect** `std::time` |
| 3572 | `std::fs::read(&p)` (`lire_texte`) |
| 3991 | `std::fs::write(&p, t.as_bytes())` (`ecrire_texte`) |
| 4014 | `std::fs::write(&p, &bytes)` (`ecrire_binaire`) |
| 4030 | `std::fs::write(&p, &bytes)` (`ecrire_bytes`) |
| 4048 | `crate::run::temps_unix_nanos()` — indirect |
| 4056 | `crate::run::temps_unix_nanos()` — indirect |
| 4078 | `std::io::stdin().read_line` (BREAKPOINT) |
| 4463 | `std::fs::create_dir_all(&cache_dir)` |
| 4486 | `std::fs::create_dir_all(&tmp)` |
| 4490 | `std::fs::write(&src_path, source)` |
| 4507 | `std::fs::copy(&bin_path, &cache_bin)` |
| 4510 | `use std::os::unix::fs::PermissionsExt;` |
| 4511 | `std::fs::set_permissions(&cache_bin, ...0o755)` |
| 4551 | `std::fs::read_to_string(chemin)` (voie AOT) |
| 4555 | `std::fs::read_to_string(chemin)` (`executer_fichier`) |

`create_dir_all`, `copy`, `set_permissions` n'ont **aucun** équivalent dans
`sys_io` : à ajouter (`mkdir`, `stat`, `rename`/`copy`, `fchmod`).

### 3.3 `src/lib.rs` — AUCUN `std::fs`/`std::io`/`std::time`

Rien à porter pour ces trois familles. Deux autres usages `std` subsistent et
devront être traités dans le chantier global (hors périmètre de cette mission) :
- ligne 197 : `std::panic::catch_unwind(std::panic::AssertUnwindSafe(...))`
- ligne 291 : `payload: &(dyn std::any::Any + Send)`

### 3.4 Dépendance transitoire à traiter

`src/run.rs:165-175` — `temps_unix_nanos()` utilise
`std::time::SystemTime::now()` (168) et `std::time::UNIX_EPOCH` (169). C'est la
source des trois appels indirects de `interp.rs`. Ajouter dans `sys_io::linux` :

```rust
pub fn clock_gettime(horloge: i32, ts: &mut libc::timespec) -> Result<(), IoError>;
```

Hors périmètre strict mais à recenser : `debug_lenteur.rs:27`,
`setup_gui.rs:2903` (`std::time`), et les 21 autres fichiers contenant `std::`
(`PRODUIT_SANS_RUST.md` §2.1 : 23 fichiers au total, `interp.rs` 143 occurrences,
`setup_core.rs` 138, `setup_gui.rs` 109, `jit.rs` 86, `run.rs` 62).

---

## 4. Pièges — à ne jamais rejouer

### 4.1 Le strip fausse `nm` (déjà tombé dans M5.3)

```
$ file rose-20
... static-pie linked, ... stripped
$ nm rose-20
nm: rose-20: no symbols
$ nm rose-20 | grep -c 'std::'
0        ← ARTÉFACT, pas une preuve
```

Démonstration complémentaire : une copie **strippée** du harnais no_std donne
`nm: …: no symbols` et `grep -c 'std::'` = 0, alors qu'elle s'exécute toujours
(`OK sys_io no_std: 51 octets…`). Le `0` ne mesure que l'absence de table de
symboles. **Toute porte `nm` exige un binaire NON stripé** (`strip = false` dans
le profil, ou `cargo rustc -- -C strip=none`) et une vérification préalable que
`nm` répond autre chose que « no symbols ».

### 4.2 `grep -c 'std::'` non ancré = faux positifs

Sur le harnais, `nm -C … | grep -c 'std::'` renvoie **2** : ce sont
`rose_sys_io_no**std::**ecrire_u64` et `rose_sys_io_no**std::**openat` — le nom
de crate se termine par `nostd`, donc `::` produit la sous-chaîne `std::`.
Mesures correctes :

```
nm -C BIN | grep -E '(^|[^A-Za-z0-9_])std::' | wc -l     # → 0
nm -C BIN | grep -P '(?<![A-Za-z0-9_])std::' | wc -l     # → 0
nm     BIN | grep -cE '_?_ZN3std'                        # → 0 (manglé)
```

Toujours croiser les trois, plus `ldd` et `strings | grep -ci 'library/std\|rustc/'`.

### 4.3 `rust_eh_personality` — erreur de lien réelle, même en `panic = "abort"`

Premier build du harnais (avant ajout du stub) :

```
/usr/bin/ld: …/liballoc-*.rlib(…):(.data.DW.ref.rust_eh_personality
[DW.ref.rust_eh_personality]+0x0): undefined reference to `rust_eh_personality'
collect2: error: ld returned 1 exit status
```

Le `liballoc` **précompilé du sysroot** référence ce symbole dans une section
DWARF, indépendamment du mode panic. Correctif minimal :

```rust
#[no_mangle]
pub extern "C" fn rust_eh_personality() {}
```

(avec `#[unsafe(no_mangle)]` si l'édition 2024 l'exige). Vérifié présent dans le
binaire : `0000000000001c34 T rust_eh_personality`.

### 4.4 Allocateur global obligatoire

`sys_io.rs` utilise `Vec` (`chemin_nul`, `lire_fichier`), donc `alloc`. Il ne
fournit **pas** d'allocateur : c'est au binaire de poser `#[global_allocator]`.
Le harnais utilise `malloc`/`free` de `libc` (disponibles sans feature `std`) ;
la production doit porter l'allocateur mmap décrit dans `NO_STD.md` §4.5.

### 4.5 Amorçage complet du binaire

`#![no_std]` + `#![no_main]` + `#[global_allocator]` + `#[panic_handler]` +
`#[no_mangle] extern "C" fn main()` + stub `rust_eh_personality`. Le harnais
est un exemple fonctionnel de ces cinq éléments.

### 4.6 Numéros de syscall / portabilité

`sys_io::linux` est écrit pour **aarch64** (`SYS_openat=56`, `close=57`,
`read=63`, `write=64`, `munmap=215`, `mmap=222`, `clock_gettime=113`). Vérifié :
`uname -m` → `aarch64` sur cette machine. `mmap(offset: i64)` n'est pas découpé
pour les cibles 32 bits. Les constantes `MAP_ANONYMOUS`, `O_*` sont codées en
dur dans le module — à confronter à `libc::*` si une nouvelle cible apparaît.

### 4.7 `std` n'est PAS la crate `libc`

Rust `std` ne dépend pas de la crate `libc` (il lie `libc.so` en C). Un binaire
`#![no_std]` peut donc appeler `libc::malloc` sans réintroduire `std` : c'est ce
que fait le harnais, et `ldd` ne montre que `libc.so.6`.

---

## 5. Checklist ordonnée pour G3

- [x] **M5.2** — `sys_io.rs` fournit open/read/write/close + mmap/munmap, sans
      `std::` (déjà fait).
- [x] **M5.3 préparation** — preuve no_std du module seul (§1).
- [ ] **G2 d'abord** — retirer Cranelift (487 symboles) ; sinon le `nm` final
      restera pollué et la porte G3 ne voudra rien dire.
- [ ] **Changer `Cargo.toml:52`** en `libc = { version = "0.2", default-features = false }`
      (§2) ; vérifier `cargo tree -f '{p} features=[{f}]'` → `libc … features=[]`.
- [ ] **Ajouter à `sys_io::linux`** les syscalls manquants : `fchmod`, `mkdir`,
      `stat`, `unlink`, `rename`, `readlink`, `clock_gettime` ; ajouter une
      façade `lire_ligne_stdin`.
- [ ] **Porter `main.rs`** : les 6 `std::fs`, 5 `std::io`, 1 `std::time` (§3.1),
      puis `std::env`/`std::process`/`std::thread`/`std::panic`.
- [ ] **Porter `interp.rs`** : les 12 `std::fs`, 5 `std::io` (§3.2).
- [ ] **Porter `run.rs:168-169`** vers `clock_gettime` ; traiter `lib.rs:197` et
      `lib.rs:291`.
- [ ] **Traiter les 23 fichiers** contenant `std::` (`PRODUIT_SANS_RUST.md` §2.1).
- [ ] **Amorçage du binaire** : les 5 éléments du §4.5.
- [ ] **Profil de build non stripé** : `strip = false` (piège §4.1).
- [ ] **Porte de validation** (§6) : `nm` + `ldd` + exécution.

---

## 6. Porte de validation de G3 (à exécuter telle quelle)

```bash
export PATH="/home/clausake/.cargo/bin:$PATH"
cd /home/clausake/Stefan/ROSE/rose

# 1. build NON stripé — vérifier d'abord que nm VOIT des symboles
cargo build --release
file target/release/rose
nm -C target/release/rose | wc -l          # DOIT être > 0, sinon porte invalide

# 2. absence de std — trois mesures croisées, regex ANCRÉE (piège §4.2)
nm -C target/release/rose | grep -E '(^|[^A-Za-z0-9_])std::' | wc -l   # → 0
nm target/release/rose     | grep -cE '_?_ZN3std'                      # → 0
strings target/release/rose | grep -ciE 'library/std|rustc/'            # → 0

# 3. absence de Cranelift (G2)
nm -C target/release/rose | grep -i cranelift | wc -l                   # → 0

# 4. dépendances dynamiques
ldd target/release/rose
# attendu : libc.so.6 seul, OU « statically linked » sans libstd

# 5. comportement — l'oracle doit rester identique
./target/release/rose --version             # → Rose 0.1.5
./target/release/rose examples/....rose     # sortie identique à l'oracle G0

# 6. témoin secondaire : la rlib non stripée
nm -C target/release/librose.rlib | grep -E '(^|[^A-Za-z0-9_])std::' | wc -l  # → 0
```

Refuser la porte si : `file` dit `stripped`, si `nm` répond « no symbols », si
un des comptages est non nul, si `ldd` montre une bibliothèque Rust, ou si la
sortie diffère de l'oracle.

---

## 7. Limite de portée de ce rapport

Ce sous-agent **ne prétend PAS** que Rose entier compile en `no_std`. Il
affirme seulement, preuves à l'appui (§1), que **`sys_io.rs` seul** compile et
s'exécute avec `libc` sans feature `std`, et que les blocages restants sont
inventoriés aux §2-§3. Le changement `Cargo.toml` n'a **pas** été appliqué et
aucun commit n'a été fait.
