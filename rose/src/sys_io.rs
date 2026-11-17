//! E/S et mémoire SANS `std` — jalon M5.2.
//!
//! Ce module ne contient AUCUN chemin `std::` : il ne dépend que de `core`,
//! de `alloc` et (sur Linux) de la crate `libc` déjà présente dans
//! `Cargo.toml`. Il est donc compilable dans une crate `#![no_std]` avec
//! `extern crate alloc;`.
//!
//! ⚠ Deux contraintes pour la porte « no_std + alloc seul » :
//!
//! 1. `libc = "0.2"` est déclaré avec ses features par défaut dans
//!    `Cargo.toml`, et `libc` a `default = ["std"]`. Tant que cette ligne
//!    n'est pas changée en `libc = { version = "0.2", default-features = false }`,
//!    le binaire final embarque `std` via `libc` (le module, lui, reste
//!    propre). Voir `docs/NO_STD.md`.
//! 2. Le démarrage no_std (allocateur global, point d'entrée, panic handler)
//!    n'est PAS dans ce module : il appartient au binaire. Voir `docs/NO_STD.md`.
//!
//! Architecture mesurée avant écriture : `uname -m` → `aarch64`. Les numéros
//! de syscalls utilisés viennent de `libc::SYS_*` (aarch64 : openat 56,
//! close 57, read 63, write 64, munmap 215, mmap 222) : aucun appel asm
//! en ligne n'est nécessaire, `libc::syscall` est disponible.
//!
//! Les syscalls bruts ne POSENT PAS `errno` : l'erreur est rendue directement
//! dans la valeur de retour (négative, entre -1 et -4095). On stocke donc
//! `-ret` dans `IoError::code`, qui est un vrai code errno POSIX.

extern crate alloc;

use alloc::vec::Vec;

/// Erreur d'E/S : un code errno POSIX (ou `GetLastError` sous Windows).
/// Pas de `std::io::Error` : ce type est `Copy` et `no_std`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IoError {
    /// Code d'erreur brut : `-valeur_de_retour` sur Linux, `GetLastError()`
    /// sous Windows. Le cas spécial `mmap` est documenté sur [`linux::mmap`].
    pub code: i32,
}

impl IoError {
    pub const fn new(code: i32) -> Self {
        Self { code }
    }

    pub const fn code(self) -> i32 {
        self.code
    }

    /// Nom symbolique pour les errno les plus courants (affichage sans
    /// `strerror`, donc sans `std`).
    pub const fn nom(self) -> Option<&'static str> {
        match self.code {
            1 => Some("EPERM"),
            2 => Some("ENOENT"),
            5 => Some("EIO"),
            9 => Some("EBADF"),
            12 => Some("ENOMEM"),
            13 => Some("EACCES"),
            17 => Some("EEXIST"),
            20 => Some("ENOTDIR"),
            21 => Some("EISDIR"),
            22 => Some("EINVAL"),
            28 => Some("ENOSPC"),
            38 => Some("ENOSYS"),
            _ => None,
        }
    }
}

impl core::fmt::Display for IoError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.nom() {
            Some(n) => write!(f, "{} ({})", n, self.code),
            None => write!(f, "erreur E/S {}", self.code),
        }
    }
}

/// Descripteur de fichier (Linux) ou `HANDLE` Windows (transmuté en `isize`).
#[cfg(target_os = "linux")]
pub type Handle = i32;
/// Voir [`Handle`].
#[cfg(windows)]
pub type Handle = isize;
/// Voir [`Handle`].
#[cfg(not(any(target_os = "linux", windows)))]
pub type Handle = i32;

/// `EINVAL` — utilisé pour un chemin contenant un NUL ou un argument invalide.
pub const EINVAL: i32 = 22;
/// `ENOSYS` — retourné sur les cibles sans implémentation (WASM, macOS…).
pub const ENOSYS: i32 = 38;

/// Construit un chemin NUL-terminé pour les API C, sans `std::ffi::CString`.
#[cfg(any(target_os = "linux", windows))]
fn chemin_nul(chemin: &str) -> Result<Vec<u8>, IoError> {
    if chemin.as_bytes().contains(&0) {
        return Err(IoError::new(EINVAL));
    }
    let mut v = Vec::with_capacity(chemin.len() + 1);
    v.extend_from_slice(chemin.as_bytes());
    v.push(0);
    Ok(v)
}

// ── Linux : appels système bruts ───────────────────────────────────────────

#[cfg(target_os = "linux")]
pub mod linux {
    use super::{IoError, chemin_nul};

    /// `AT_FDCWD` : les chemins relatifs sont résolus depuis le cwd.
    pub const AT_FDCWD: i32 = -100;

    pub const O_RDONLY: u32 = 0;
    pub const O_WRONLY: u32 = 1;
    pub const O_RDWR: u32 = 2;
    pub const O_CREAT: u32 = 0o100;
    pub const O_TRUNC: u32 = 0o1000;
    pub const O_APPEND: u32 = 0o2000;

    pub const PROT_READ: u32 = 0x1;
    pub const PROT_WRITE: u32 = 0x2;
    pub const PROT_EXEC: u32 = 0x4;

    pub const MAP_SHARED: u32 = 0x01;
    pub const MAP_PRIVATE: u32 = 0x02;
    pub const MAP_ANONYMOUS: u32 = 0x20;
    /// `(void *) -1` renvoyé par `mmap` en échec.
    pub const MAP_FAILED: usize = usize::MAX;
    /// Code sentinelle de [`mmap`] : `mmap` ne rend PAS d'errno, seulement
    /// `MAP_FAILED`. On choisit -1 (jamais un errno positif valide) pour
    /// distinguer ce cas d'un vrai errno.
    pub const MAP_FAILED_CODE: i32 = -1;

    #[inline]
    fn err(ret: libc::c_long) -> IoError {
        IoError::new((-ret) as i32)
    }

    /// `openat(dirfd, chemin, drapeaux, mode)` via `libc::syscall`.
    /// `chemin` doit être NUL-terminé par l'appelant C ; on le construit ici.
    pub fn openat(dirfd: i32, chemin: &str, drapeaux: u32, mode: u32) -> Result<i32, IoError> {
        let c = chemin_nul(chemin)?;
        let r = unsafe {
            libc::syscall(
                libc::SYS_openat,
                dirfd as libc::c_long,
                c.as_ptr() as libc::c_long,
                drapeaux as libc::c_long,
                mode as libc::c_long,
            )
        };
        if r < 0 { Err(err(r)) } else { Ok(r as i32) }
    }

    /// `read(fd, buf, buf.len())`. Renvoie le nombre d'octets lus (0 = EOF).
    pub fn read(fd: i32, buf: &mut [u8]) -> Result<usize, IoError> {
        let r = unsafe {
            libc::syscall(
                libc::SYS_read,
                fd as libc::c_long,
                buf.as_mut_ptr() as libc::c_long,
                buf.len() as libc::c_long,
            )
        };
        if r < 0 { Err(err(r)) } else { Ok(r as usize) }
    }

    /// `write(fd, buf, buf.len())`. Renvoie le nombre d'octets écrits (qui
    /// peut être < `buf.len()` : utiliser [`crate::sys_io::ecrire_tout`]).
    pub fn write(fd: i32, buf: &[u8]) -> Result<usize, IoError> {
        let r = unsafe {
            libc::syscall(
                libc::SYS_write,
                fd as libc::c_long,
                buf.as_ptr() as libc::c_long,
                buf.len() as libc::c_long,
            )
        };
        if r < 0 { Err(err(r)) } else { Ok(r as usize) }
    }

    /// `close(fd)`.
    pub fn close(fd: i32) -> Result<(), IoError> {
        let r = unsafe { libc::syscall(libc::SYS_close, fd as libc::c_long) };
        if r < 0 { Err(err(r)) } else { Ok(()) }
    }

    /// `mmap(addr, len, prot, flags, fd, offset)`.
    ///
    /// ⚠ `unsafe` : l'appelant doit garantir la validité de `addr`, que `len`
    /// et `offset` sont cohérents avec l'objet mappé, et que la mémoire rendue
    /// n'est pas utilisée après [`munmap`]. `offset` est un `i64` : exact sur
    /// aarch64/x86_64 (c_long 64 bits) ; les cibles 32 bits exigeraient un
    /// découpage en deux mots (non implémenté ici).
    pub unsafe fn mmap(
        addr: *mut u8,
        len: usize,
        prot: u32,
        flags: u32,
        fd: i32,
        offset: i64,
    ) -> Result<*mut u8, IoError> {
        let r = unsafe {
            libc::syscall(
                libc::SYS_mmap,
                addr as libc::c_long,
                len as libc::c_long,
                prot as libc::c_long,
                flags as libc::c_long,
                fd as libc::c_long,
                offset as libc::c_long,
            )
        };
        if r as usize == MAP_FAILED {
            return Err(IoError::new(MAP_FAILED_CODE));
        }
        if r < 0 { Err(err(r)) } else { Ok(r as *mut u8) }
    }

    /// `munmap(ptr, len)`.
    pub fn munmap(ptr: *mut u8, len: usize) -> Result<(), IoError> {
        let r = unsafe {
            libc::syscall(
                libc::SYS_munmap,
                ptr as libc::c_long,
                len as libc::c_long,
            )
        };
        if r < 0 { Err(err(r)) } else { Ok(()) }
    }
}

// ── Windows : appels Win32 bruts (non testables sur cette machine) ─────────

#[cfg(windows)]
pub mod win32 {
    // `Vec` est importé ici depuis le module parent : `alloc` n'est PAS dans
    // l'extern prelude d'une crate `std`, seul `extern crate alloc;` du parent
    // le rend visible (un chemin `alloc::…` direct échouerait en sous-module).
    use super::{IoError, Vec, chemin_nul};
    use core::ffi::c_void;

    pub const GENERIC_READ: u32 = 0x8000_0000;
    pub const GENERIC_WRITE: u32 = 0x4000_0000;
    pub const FILE_SHARE_READ: u32 = 0x0000_0001;
    pub const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    pub const CREATE_ALWAYS: u32 = 2;
    pub const OPEN_EXISTING: u32 = 3;
    pub const FILE_ATTRIBUTE_NORMAL: u32 = 0x0000_0080;
    /// `(HANDLE) -1` : échec de `CreateFileW`.
    pub const INVALID_HANDLE_VALUE: isize = -1;

    unsafe extern "system" {
        fn CreateFileW(
            nom: *const u16,
            acces: u32,
            partage: u32,
            securite: *mut c_void,
            disposition: u32,
            attributs: u32,
            modele: *mut c_void,
        ) -> *mut c_void;
        fn ReadFile(
            h: *mut c_void,
            buf: *mut c_void,
            n: u32,
            lu: *mut u32,
            chevauchement: *mut c_void,
        ) -> i32;
        fn WriteFile(
            h: *mut c_void,
            buf: *const c_void,
            n: u32,
            ecrit: *mut u32,
            chevauchement: *mut c_void,
        ) -> i32;
        fn CloseHandle(h: *mut c_void) -> i32;
        fn GetLastError() -> u32;
    }

    #[inline]
    fn err() -> IoError {
        IoError::new(unsafe { GetLastError() } as i32)
    }

    /// UTF-8 → UTF-16 NUL-terminé, sans `std`.
    fn utf16(chemin: &str) -> Result<Vec<u16>, IoError> {
        if chemin.encode_utf16().count() > u32::MAX as usize - 1 {
            return Err(IoError::new(super::EINVAL));
        }
        // `chemin_nul` vérifie l'absence de NUL dans l'UTF-8 (donc dans l'UTF-16).
        let _ = chemin_nul(chemin)?;
        let mut v: Vec<u16> = Vec::new();
        for c in chemin.encode_utf16() {
            v.push(c);
        }
        v.push(0);
        Ok(v)
    }

    /// `CreateFileW` brut. Renvoie un `HANDLE` transmuté en `isize`.
    pub fn create_file_w(chemin: &str, ecriture: bool) -> Result<isize, IoError> {
        let u = utf16(chemin)?;
        let h = unsafe {
            CreateFileW(
                u.as_ptr(),
                if ecriture { GENERIC_WRITE } else { GENERIC_READ },
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                core::ptr::null_mut(),
                if ecriture { CREATE_ALWAYS } else { OPEN_EXISTING },
                FILE_ATTRIBUTE_NORMAL,
                core::ptr::null_mut(),
            )
        };
        let h = h as isize;
        if h == INVALID_HANDLE_VALUE { Err(err()) } else { Ok(h) }
    }

    /// `ReadFile` brut.
    pub fn read_file(h: isize, buf: &mut [u8]) -> Result<usize, IoError> {
        let mut lu = 0u32;
        let n = buf.len().min(u32::MAX as usize) as u32;
        let ok = unsafe {
            ReadFile(
                h as *mut c_void,
                buf.as_mut_ptr() as *mut c_void,
                n,
                &mut lu,
                core::ptr::null_mut(),
            )
        };
        if ok == 0 { Err(err()) } else { Ok(lu as usize) }
    }

    /// `WriteFile` brut.
    pub fn write_file(h: isize, buf: &[u8]) -> Result<usize, IoError> {
        let mut ecrit = 0u32;
        let n = buf.len().min(u32::MAX as usize) as u32;
        let ok = unsafe {
            WriteFile(
                h as *mut c_void,
                buf.as_ptr() as *const c_void,
                n,
                &mut ecrit,
                core::ptr::null_mut(),
            )
        };
        if ok == 0 { Err(err()) } else { Ok(ecrit as usize) }
    }

    /// `CloseHandle` brut.
    pub fn close_handle(h: isize) -> Result<(), IoError> {
        let ok = unsafe { CloseHandle(h as *mut c_void) };
        if ok == 0 { Err(err()) } else { Ok(()) }
    }
}

// ── Façade plateforme-indépendante ─────────────────────────────────────────

/// Ouvre `chemin` en lecture (`ecriture = false`) ou en écriture
/// (`ecriture = true` : création + troncature).
pub fn ouvrir(chemin: &str, ecriture: bool) -> Result<Handle, IoError> {
    #[cfg(target_os = "linux")]
    {
        let drapeaux = linux::O_RDONLY
            | if ecriture { linux::O_WRONLY | linux::O_CREAT | linux::O_TRUNC } else { 0 };
        linux::openat(linux::AT_FDCWD, chemin, drapeaux, 0o666)
    }
    #[cfg(windows)]
    {
        win32::create_file_w(chemin, ecriture)
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = (chemin, ecriture);
        Err(IoError::new(ENOSYS))
    }
}

/// Lit au plus `buf.len()` octets. 0 = fin de fichier.
pub fn lire(h: Handle, buf: &mut [u8]) -> Result<usize, IoError> {
    #[cfg(target_os = "linux")]
    { linux::read(h, buf) }
    #[cfg(windows)]
    { win32::read_file(h, buf) }
    #[cfg(not(any(target_os = "linux", windows)))]
    { let _ = (h, buf); Err(IoError::new(ENOSYS)) }
}

/// Écrit `buf` (une seule syscall : peut être partiel).
pub fn ecrire(h: Handle, buf: &[u8]) -> Result<usize, IoError> {
    #[cfg(target_os = "linux")]
    { linux::write(h, buf) }
    #[cfg(windows)]
    { win32::write_file(h, buf) }
    #[cfg(not(any(target_os = "linux", windows)))]
    { let _ = (h, buf); Err(IoError::new(ENOSYS)) }
}

/// Écrit TOUT `buf`, en bouclant sur les écritures partielles.
pub fn ecrire_tout(h: Handle, buf: &[u8]) -> Result<(), IoError> {
    let mut pos = 0usize;
    while pos < buf.len() {
        let n = ecrire(h, &buf[pos..])?;
        if n == 0 {
            return Err(IoError::new(EIO));
        }
        pos += n;
    }
    Ok(())
}

/// `EIO` — une écriture qui n'avance pas est une erreur.
pub const EIO: i32 = 5;

/// Ferme le descripteur / handle.
pub fn fermer(h: Handle) -> Result<(), IoError> {
    #[cfg(target_os = "linux")]
    { linux::close(h) }
    #[cfg(windows)]
    { win32::close_handle(h) }
    #[cfg(not(any(target_os = "linux", windows)))]
    { let _ = h; Err(IoError::new(ENOSYS)) }
}

/// Lit un fichier entier en mémoire (remplace `std::fs::read`).
pub fn lire_fichier(chemin: &str) -> Result<Vec<u8>, IoError> {
    let h = ouvrir(chemin, false)?;
    let mut out = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = match lire(h, &mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                let _ = fermer(h);
                return Err(e);
            }
        };
        out.extend_from_slice(&buf[..n]);
    }
    fermer(h)?;
    Ok(out)
}

/// Écrit un fichier entier (remplace `std::fs::write`).
pub fn ecrire_fichier(chemin: &str, contenu: &[u8]) -> Result<(), IoError> {
    let h = ouvrir(chemin, true)?;
    let e = ecrire_tout(h, contenu);
    let f = fermer(h);
    e?;
    f?;
    Ok(())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn ecrire_puis_lire_via_syscalls_bruts() {
        let chemin = "/tmp/opencode/rose_sys_io_m52_test.bin";
        let contenu = b"Rose M5.2 - E/S sans std\n";
        ecrire_fichier(chemin, contenu).expect("écriture brute");
        let lu = lire_fichier(chemin).expect("lecture brute");
        assert_eq!(lu.as_slice(), contenu);
        // Le même contenu doit être relu via les wrappers bruts directs.
        let fd = linux::openat(linux::AT_FDCWD, chemin, linux::O_RDONLY, 0).expect("openat");
        let mut petit = [0u8; 8];
        let n = linux::read(fd, &mut petit).expect("read");
        assert_eq!(n, 8);
        assert_eq!(&petit, &contenu[..8]);
        linux::close(fd).expect("close");
    }

    #[test]
    fn mmap_anonyme_et_munmap() {
        let page = 4096usize;
        let p = unsafe {
            linux::mmap(
                core::ptr::null_mut(),
                page,
                linux::PROT_READ | linux::PROT_WRITE,
                linux::MAP_PRIVATE | linux::MAP_ANONYMOUS,
                -1,
                0,
            )
        }
        .expect("mmap anonyme");
        assert!(p as usize != linux::MAP_FAILED);
        unsafe {
            core::ptr::write_bytes(p, 0xAB, page);
            assert_eq!(*p, 0xAB);
        }
        linux::munmap(p, page).expect("munmap");
    }

    #[test]
    fn chemin_avec_nul_est_refuse() {
        assert_eq!(lire_fichier("ab\0cd").unwrap_err().code, EINVAL);
    }
}
