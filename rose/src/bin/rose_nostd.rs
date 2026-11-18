//! M5.3 / G3 — binaire de démonstration RÉELLEMENT `#![no_std]` + `alloc`.
//!
//! ⚠⚠ CE N'EST PAS LE MOTEUR ROSE SANS RUST, NI « ROSE SANS RUST ». ⚠⚠
//!
//! Ce fichier est une **démonstration no_std d'un calcul exact** : il prouve
//! qu'un exécutable peut (a) démarrer sans `std`, (b) lire un argument via
//! le point d'entrée C `main(argc, argv)` — aucun `std::env`, (c) écrire sur
//! stdout UNIQUEMENT via `sys_io` (syscalls bruts), (d) exécuter un
//! **micro-bytecode encodé en dur** et (e) produire une factorielle exacte
//! sur des limbs `u32`, sans GMP et sans `num-bigint`.
//!
//! Ce qu'il NE fait PAS : il n'inclut ni le frontal (`front.rs`), ni
//! l'interpréteur (`interp.rs`), ni le runtime (`run.rs`), ni `i128x.rs`,
//! ni `mul_rapide.rs`. Le moteur Rose COMPLET ne compile toujours PAS en
//! `no_std` (voir `docs/NO_STD_CHECKLIST.md` §3). Les 4 opcodes ci-dessous
//! sont un « programme pré-analysé encodé en dur » SCHÉMATIQUE, pas le
//! bytecode de Rose. Voir `docs/BINAIRE_NO_STD.md`.
//!
//! Pourquoi un calcul maison au lieu de `i128x`/`mul_rapide` : ces deux
//! modules importent `num_bigint`/`num_integer`/`num_traits` et utilisent
//! `std::thread`, `std::sync::OnceLock`, `std::collections` et
//! `crate::run::erreur_runtime`. Ils ne compilent donc PAS dans une crate
//! `#![no_std]`. La factorielle est ici calculée par multiplications
//! successives sur un nombre en base 2^32 (limbs little-endian), avec
//! conversion décimale par divisions répétées par 10^9 — exact pour
//! n! tant que la mémoire suit (n = 50 tient en 7 limbs).
//!
//! Amorçage no_std complet (pièges documentés dans NO_STD_CHECKLIST.md §4) :
//!   `#![no_std]` + `#![no_main]` + `#[global_allocator]` +
//!   `#[panic_handler]` + `extern "C" fn main` + `rust_eh_personality`.
//!
//! Validation (harnais isolé `/tmp/opencode/rose_nostd_demo/`) :
//!   ./target/release/rose_nostd 50
//!   nm -C BIN | grep -E '(^|[^A-Za-z0-9_])std::' | wc -l   # → 0
//!   nm     BIN | grep -cE '_?_ZN3std'                      # → 0
//!   ldd BIN                                                # libc.so.6 seul

#![no_std]
#![no_main]

extern crate alloc;

// Chemin RELATIF : depuis src/bin/rose_nostd.rs, `../sys_io.rs` = src/sys_io.rs.
// L'orchestrateur n'a qu'à ajouter un `[[bin]]` dans Cargo.toml.
#[path = "../sys_io.rs"]
#[allow(dead_code)] // le module entier est inclus ; seul ecrire_tout est utilisé ici
mod sys_io;

use alloc::alloc::{GlobalAlloc, Layout};
use alloc::vec::Vec;
use core::panic::PanicInfo;

// ── Amorçage ───────────────────────────────────────────────────────────────

/// Allocateur global minimal : `malloc`/`free` de libc, présents même sans
/// la feature `std` de la crate `libc` (NO_STD_CHECKLIST.md §4.7).
struct MallocAlloc;

unsafe impl GlobalAlloc for MallocAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe {
            let n = if layout.size() == 0 { 1 } else { layout.size() };
            libc::malloc(n) as *mut u8
        }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        unsafe { libc::free(ptr as *mut libc::c_void) }
    }
}

#[global_allocator]
static ALLOC: MallocAlloc = MallocAlloc;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    let _ = sys_io::ecrire_tout(2, b"PANIC rose_nostd\n");
    unsafe { libc::syscall(libc::SYS_exit_group, 101isize as libc::c_long) };
    loop {}
}

/// `liballoc` précompilé référence ce symbole en DWARF même en panic=abort.
#[unsafe(no_mangle)]
pub extern "C" fn rust_eh_personality() {}

// ── Micro-bytecode : « programme pré-analysé encodé en dur » ──────────────

const OP_LIRE_ARG: u8 = 1; // argv[1] → registre R0 (entier u64)
const OP_FACT: u8 = 2; // R0 → registre exact R1 (limbs base 2^32)
const OP_ECRIRE: u8 = 3; // R1 → stdout en décimal
const OP_HALT: u8 = 0;

/// Programme de démonstration. Ce n'est PAS un bytecode Rose : ce sont 4
/// opcodes SCHÉMATIQUES qui montrent la forme « moteur exécute un programme ».
const PROGRAMME: &[u8] = &[OP_LIRE_ARG, OP_FACT, OP_ECRIRE, OP_HALT];

/// État de la machine : deux registres seulement.
struct Machine {
    /// R0 — entier courant (l'argument décodé).
    r0: u64,
    /// R1 — entier exact, limbs little-endian en base 2^32.
    r1: Vec<u32>,
}

// ── Utilitaires de E/S et d'arithmétique exacte (sans std) ─────────────────

/// Écrit un u64 en décimal (pour les codes d'erreur, sans `format!`).
fn ecrire_u64(fd: i32, mut n: u64) {
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    if n == 0 {
        i -= 1;
        buf[i] = b'0';
    }
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    let _ = sys_io::ecrire_tout(fd, &buf[i..]);
}

/// Lit l'argument `index` du argv C (NUL-terminé) sans `std::env::args`.
unsafe fn arg_bytes(argv: *const *const u8, index: usize) -> Option<&'static [u8]> {
    unsafe {
        let p = *argv.add(index);
        if p.is_null() {
            return None;
        }
        let mut n = 0usize;
        while *p.add(n) != 0 {
            n += 1;
        }
        Some(core::slice::from_raw_parts(p, n))
    }
}

/// Parse un entier décimal ASCII en u64, avec détection de débordement.
fn parser_u64(s: &[u8]) -> Option<u64> {
    if s.is_empty() {
        return None;
    }
    let mut n = 0u64;
    for &c in s {
        if !c.is_ascii_digit() {
            return None;
        }
        n = n.checked_mul(10)?.checked_add((c - b'0') as u64)?;
    }
    Some(n)
}

/// Factorielle exacte par multiplications successives sur des limbs u32.
fn factorielle_limbs(n: u64) -> Vec<u32> {
    let mut v: Vec<u32> = Vec::new();
    v.push(1);
    let mut k: u64 = 2;
    while k <= n {
        let mut retenue: u64 = 0;
        for limb in v.iter_mut() {
            let p = (*limb as u64) * k + retenue;
            *limb = p as u32;
            retenue = p >> 32;
        }
        while retenue > 0 {
            v.push(retenue as u32);
            retenue >>= 32;
        }
        k += 1;
    }
    v
}

/// Écrit un entier en base 2^32 sous forme décimale : divisions répétées
/// par 10^9 (9 chiffres par groupe, rembourrés à gauche pour les groupes
/// sauf le plus significatif). `rem << 32` tient en u64 car rem < 10^9.
fn ecrire_exact_decimal(fd: i32, v: &[u32]) {
    let mut travail = v.to_vec();
    let mut groupes: Vec<u32> = Vec::new();
    while travail.iter().any(|&x| x != 0) {
        let mut reste: u64 = 0;
        for limb in travail.iter_mut().rev() {
            let courant = (reste << 32) | (*limb as u64);
            *limb = (courant / 1_000_000_000) as u32;
            reste = courant % 1_000_000_000;
        }
        groupes.push(reste as u32);
    }

    let dernier = groupes.len() - 1;
    ecrire_u64(fd, groupes[dernier] as u64);
    let mut g = dernier;
    while g > 0 {
        g -= 1;
        let mut x = groupes[g];
        let mut buf = [b'0'; 9];
        for i in (0..9).rev() {
            buf[i] = b'0' + (x % 10) as u8;
            x /= 10;
        }
        let _ = sys_io::ecrire_tout(fd, &buf);
    }
    let _ = sys_io::ecrire_tout(fd, b"\n");
}

fn erreur(message: &[u8], code: i32) -> i32 {
    let _ = sys_io::ecrire_tout(2, message);
    let _ = sys_io::ecrire_tout(2, b"\n");
    code
}

// ── Point d'entrée : la « boucle d'interprétation » du micro-bytecode ──────

#[unsafe(no_mangle)]
pub extern "C" fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut machine = Machine { r0: 0, r1: Vec::new() };
    let mut pc = 0usize;

    while pc < PROGRAMME.len() {
        match PROGRAMME[pc] {
            OP_LIRE_ARG => {
                if argc < 2 {
                    return erreur(b"usage: rose_nostd <entier>", 2);
                }
                let octets = match unsafe { arg_bytes(argv, 1) } {
                    Some(s) => s,
                    None => return erreur(b"argv[1] absent", 2),
                };
                machine.r0 = match parser_u64(octets) {
                    Some(n) => n,
                    None => return erreur(b"argument invalide (entier decimal attendu)", 2),
                };
                pc += 1;
            }
            OP_FACT => {
                machine.r1 = factorielle_limbs(machine.r0);
                pc += 1;
            }
            OP_ECRIRE => {
                ecrire_exact_decimal(1, &machine.r1);
                pc += 1;
            }
            OP_HALT => return 0,
            _ => return erreur(b"opcode inconnu", 3),
        }
    }
    0
}
