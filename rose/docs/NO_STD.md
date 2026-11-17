# M5.2 — E/S et mémoire sans `std`

Jalon M5.2 du plan : remplacer les accès disque/temps de `std` par des appels
système directs, pour préparer une porte « `no_std` + `alloc` seul ».

**État : module livré, NON câblé dans Rose.** Conformément à la mission, seuls
`src/sys_io.rs` et `docs/NO_STD.md` ont été créés. `main.rs`, `lib.rs` et
`Cargo.toml` n'ont pas été touchés. Le câblage est décrit à la fin de ce
document, avec les numéros de lignes mesurés.

---

## 1. Faits mesurés avant écriture

| Fait | Commande | Résultat |
|---|---|---|
| Architecture | `uname -m` | `aarch64` |
| Version du projet | `Cargo.toml:3` | `0.1.5` |
| Dépendance `libc` | `Cargo.toml:52` | `libc = "0.2"` → résolue `0.2.191` dans `Cargo.lock` |
| `libc::syscall` | source `libc-0.2.191/src/unix/linux_like/linux/mod.rs:3623` | `pub fn syscall(num: c_long, ...) -> c_long` |
| Numéros de syscalls aarch64 | `.../gnu/b64/aarch64/mod.rs` | `SYS_openat=56`, `SYS_close=57`, `SYS_read=63`, `SYS_write=64`, `SYS_munmap=215`, `SYS_mmap=222` |
| `AT_FDCWD` | `.../unix/linux_like/mod.rs:1213` | `-100` |

`libc::syscall` étant disponible, **aucun asm en ligne n'a été nécessaire**
(l'approche asm resterait de rechange si `libc` devait être retiré : `svc #0`
sur aarch64, `syscall` sur x86_64, avec `x8`/`rax` = numéro de syscall — non
implémenté ici).

⚠ `libc` a `default = ["std"]` (`libc-0.2.191/Cargo.toml:201`). Le module
`sys_io.rs` est propre (aucun `std::`), mais tant que la ligne de
`Cargo.toml` n'est pas modifiée, `libc` ramène `std` dans l'édition de lien.
Voir §4.

---

## 2. API publique de `src/sys_io.rs` (signatures exactes)

### Types et constantes communs

```rust
pub struct IoError { pub code: i32 }          // Copy, Eq, Display
impl IoError {
    pub const fn new(code: i32) -> Self
    pub const fn code(self) -> i32
    pub const fn nom(self) -> Option<&'static str>
}
pub const EINVAL: i32 = 22;
pub const EIO: i32 = 5;
pub const ENOSYS: i32 = 38;

#[cfg(target_os = "linux")] pub type Handle = i32;
#[cfg(windows)]             pub type Handle = isize;
#[cfg(other)]               pub type Handle = i32;
```

`IoError::code` est un errno POSIX (`-valeur_de_retour` du syscall brut ;
les syscalls bruts ne posent pas `errno`). Sous Windows c'est `GetLastError()`.

### Linux — `pub mod linux` (appels système bruts)

```rust
pub const AT_FDCWD: i32 = -100;
pub const O_RDONLY: u32 = 0;  pub const O_WRONLY: u32 = 1;  pub const O_RDWR: u32 = 2;
pub const O_CREAT: u32 = 0o100;  pub const O_TRUNC: u32 = 0o1000;  pub const O_APPEND: u32 = 0o2000;
pub const PROT_READ: u32 = 0x1;  pub const PROT_WRITE: u32 = 0x2;  pub const PROT_EXEC: u32 = 0x4;
pub const MAP_SHARED: u32 = 0x01;  pub const MAP_PRIVATE: u32 = 0x02;  pub const MAP_ANONYMOUS: u32 = 0x20;
pub const MAP_FAILED: usize = usize::MAX;   pub const MAP_FAILED_CODE: i32 = -1;

pub fn openat(dirfd: i32, chemin: &str, drapeaux: u32, mode: u32) -> Result<i32, IoError>;
pub fn read(fd: i32, buf: &mut [u8]) -> Result<usize, IoError>;
pub fn write(fd: i32, buf: &[u8]) -> Result<usize, IoError>;
pub fn close(fd: i32) -> Result<(), IoError>;
pub unsafe fn mmap(addr: *mut u8, len: usize, prot: u32, flags: u32,
                   fd: i32, offset: i64) -> Result<*mut u8, IoError>;
pub fn munmap(ptr: *mut u8, len: usize) -> Result<(), IoError>;
```

Notes :
- `openat` construit lui-même le chemin NUL-terminé (pas de `CString`).
- `mmap` ne renvoie PAS d'errno, seulement `MAP_FAILED` `(void*)-1` : le code
  sentinelle `MAP_FAILED_CODE = -1` distingue ce cas d'un vrai errno.
- `offset: i64` est exact sur aarch64/x86_64 (`c_long` 64 bits) ; un port
  32 bits exigerait le découpage en deux mots.

### Windows — `pub mod win32` (`#[cfg(windows)]`, non testable en exécution ici)

```rust
pub const GENERIC_READ: u32 = 0x8000_0000;  pub const GENERIC_WRITE: u32 = 0x4000_0000;
pub const FILE_SHARE_READ: u32 = 1;  pub const FILE_SHARE_WRITE: u32 = 2;
pub const CREATE_ALWAYS: u32 = 2;  pub const OPEN_EXISTING: u32 = 3;
pub const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;
pub const INVALID_HANDLE_VALUE: isize = -1;

pub fn create_file_w(chemin: &str, ecriture: bool) -> Result<isize, IoError>;  // CreateFileW
pub fn read_file(h: isize, buf: &mut [u8]) -> Result<usize, IoError>;          // ReadFile
pub fn write_file(h: isize, buf: &[u8]) -> Result<usize, IoError>;             // WriteFile
pub fn close_handle(h: isize) -> Result<(), IoError>;                         // CloseHandle
```

Chemins UTF-8 → UTF-16 NUL-terminés sans `std`. Le `HANDLE` est transporté
comme `isize`. Erreurs = `GetLastError()`.

### Façade commune (remplace `std::fs` / `std::io`)

```rust
pub fn ouvrir(chemin: &str, ecriture: bool) -> Result<Handle, IoError>;
pub fn lire(h: Handle, buf: &mut [u8]) -> Result<usize, IoError>;
pub fn ecrire(h: Handle, buf: &[u8]) -> Result<usize, IoError>;
pub fn ecrire_tout(h: Handle, buf: &[u8]) -> Result<(), IoError>;
pub fn fermer(h: Handle) -> Result<(), IoError>;
pub fn lire_fichier(chemin: &str) -> Result<Vec<u8>, IoError>;
pub fn ecrire_fichier(chemin: &str, contenu: &[u8]) -> Result<(), IoError>;
```

Sur les cibles sans implémentation (WASM, macOS…), la façade renvoie
`ENOSYS` : déclarer `pub mod sys_io;` dans `lib.rs` ne casse pas le build WASM.

---

## 3. Preuves d'exécution réellement obtenues

Toutes les commandes ont été **exécutées** le 17/11/2026 ; `PATH` inclut
`$HOME/.cargo/bin` (rustc/cargo 1.99.1 ne sont pas dans le PATH par défaut).

1. **`cd rose && cargo check --bin rose`** → `Finished dev profile` (214
   avertissements préexistants, 0 erreur). ⚠ Ce module n'étant pas encore
   déclaré dans `lib.rs`/`main.rs`, cette commande ne le compile PAS : elle
   prouve seulement que Rose n'a pas été cassé.

2. **Crate temporaire `/tmp/opencode/rose_sys_io_check`** (`libc = "0.2"`,
   module inclus par `#[path = "/home/clausake/Stefan/ROSE/rose/src/sys_io.rs"]`) :
   - `cargo run` → `lu 35 octets : Rose M5.2 : E/S par syscalls bruts`,
     `troncature OK : [88]`, code 0 ;
   - `cargo test` → `test result: ok. 3 passed; 0 failed` (écriture+lecture
     via wrappers bruts, mmap/munmap anonyme, rejet d'un chemin contenant NUL) ;
   - `cargo check --target x86_64-pc-windows-gnu` → `Finished` : le code
     `cfg(windows)` TYPE-VÉRIFIE (exécution non testable ici) ;
   - `cargo check --target wasm32-unknown-unknown` → `Finished` : les stubs
     `ENOSYS` compilent.

3. **Crate temporaire `/tmp/opencode/rose_nostd_check`** — LA preuve de la
   porte : `#![no_std] #![no_main]`, `libc = { version = "0.2", default-features = false }`,
   allocateur global maison (1 mmap par allocation, `unsafe impl GlobalAlloc`),
   `#[panic_handler]`, stub `rust_eh_personality`, et le même `sys_io.rs` inclus
   par `#[path]` :
   - `cargo build` → succès ;
   - exécution → `OK: no_std + alloc lit/ecrit un fichier via syscalls bruts`,
     `EXIT=0` ;
   - fichier produit lu en hexadécimal (`od -An -tx1 -c`) : contenu exact ;
   - `nm <binaire> | grep -c 'std::'` → **0** ; `ldd` → `libc.so.6` seulement
     (runtime C + bindings libc, aucun `std` Rust).

---

## 4. Ce qui manque pour la porte `no_std` COMPLÈTE dans Rose

Le module M5.2 couvre **les E/S fichier et la mémoire**, pas tout le moteur.
État réel relevé par `grep` dans `rose/src/` :

### 4.1 À modifier dans `Cargo.toml` (ligne 52 — NON fait ici)

```toml
# avant
libc = "0.2"
# après
libc = { version = "0.2", default-features = false }
```
Sans cela, `libc` active sa feature `std` par défaut et l'édition de lien
embarque `std` même si Rose est `#![no_std]`.

### 4.2 À ajouter dans `src/lib.rs` (après la ligne 68 `pub mod console_win;`)

```rust
/// E/S et mémoire sans std (jalon M5.2).
pub mod sys_io;
```
Puis, pour une vraie crate no_std, en haut du fichier (aux alentours de la
ligne 35 où se trouve déjà `#![allow(unsafe_op_in_unsafe_fn)]`) :

```rust
#![no_std]
extern crate alloc;
```
⚠ **Ce dernier changement n'est pas réalisable tel quel aujourd'hui** : le
moteur entier utilise `std` (`HashMap`, `String`, `Box`, `format!`,
`catch_unwind`, threads, `std::env`…). Il doit être précédé d'un chantier
séparé (M5.3+). Le câblage de `pub mod sys_io;` SEUL, lui, est immédiat et
ne casse rien (validé sur cible wasm dans le crate temporaire).

### 4.3 À remplacer dans `src/main.rs` (lignes mesurées)

```rust
// ligne 40
use std::io::Read;                      // → supprimer
// lignes 42-47
fn lire(source: &mut String, chemin: &str) -> bool {
    match std::fs::File::open(chemin) {  // → sys_io::ouvrir(chemin, false)
        Ok(mut f) => f.read_to_string(source).is_ok(),
        Err(_) => false,
    }
}
```
Remplacement proposé (garde le même contrat `bool`) :
```rust
fn lire(source: &mut String, chemin: &str) -> bool {
    match sys_io::lire_fichier(chemin) {
        Ok(octets) => match core::str::from_utf8(&octets) {
            Ok(s) => { source.push_str(s); true }
            Err(_) => false,
        },
        Err(_) => false,
    }
}
```
Autres appels `std::fs` dans `main.rs` : lignes **552, 585, 589, 605, 853**
(`read`, `File::create`, `set_permissions`). `File::create`/`set_permissions`
n'ont pas encore d'équivalent dans `sys_io.rs` : il manque `fchmod`/`open`
avec `O_CREAT` déjà fourni (la permission 0o755 passera par un futur
`linux::fchmod`).

### 4.4 Horloge : PAS couverte par M5.2

`std::time` reste utilisé (`main.rs:612`, `debug_lenteur.rs:27`, `run.rs:143-172`,
`setup_gui.rs:2903`). `run.rs` a déjà une abstraction `Chrono` avec shim WASM :
c'est le point d'entrée naturel pour brancher `clock_gettime` (syscall aarch64
`SYS_clock_gettime = 113`, vérifié dans `libc-0.2.191/.../gnu/b64/aarch64/mod.rs:661` ;
403 est la valeur x86_64) plus tard. À ajouter alors dans `linux` :
```rust
pub fn clock_gettime(horloge: i32, ts: &mut libc::timespec) -> Result<(), IoError>;
```

### 4.5 Amorçage no_std du binaire (hors de ce module)

Le temp crate de preuve montre ce qu'il faut, **à porter dans le binaire Rose
le moment venu** :
- `#[global_allocator]` (dans le temp crate : un mmap par allocation ;
  production : allocateur plus économe) ;
- `#[panic_handler]` ;
- point d'entrée `#[unsafe(no_mangle)] extern "C" fn main()` avec `#![no_main]` ;
- stub `#[unsafe(no_mangle)] extern "C" fn rust_eh_personality() {}` — mesuré
  nécessaire : `liballoc` précompilé référence ce symbole même en
  `panic = "abort"`, sans lui l'édition de lien échoue
  (`undefined reference to rust_eh_personality`).

---

## 5. Limites

- Windows : validation **type uniquement** (`cargo check --target
  x86_64-pc-windows-gnu`) ; aucune exécution Win32 sur cette machine aarch64.
- `mmap(offset: i64)` : non décomposé pour les cibles 32 bits.
- Pas de `stat`, `fchmod`, `unlink`, `mkdir`, `rename`, `readlink` : l'installeur
  et le chargeur WASM (`setup_core.rs`, `velopack.rs`, `wasm_runtime.rs`,
  `interp.rs`) en ont besoin ; M5.2 livre seulement open/read/write/close et
  mmap/munmap.
- `lire_fichier` lit par blocs de 8192 octets sans prédimensionnement (`stat`)
  : correct mais non optimal pour les gros bundles.
- Aucun commit git effectué (conforme à la consigne).
