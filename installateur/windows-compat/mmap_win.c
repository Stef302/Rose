/* Compat Windows pour les appels POSIX mmap/munmap/mprotect émis par
 * rose/src/natif_emetteur.rs (module M3.1 « émetteur machine natif »).
 *
 * CONTEXTE — ce fichier est un SHIM DE LIEN, pas une modification de Rose.
 * Les commits récents (M3.1, 1b73c74) appellent mmap/mprotect/munmap de façon
 * inconditionnelle dans `natif_emetteur.rs`. Ces symboles n'existent pas sur
 * Windows et `rose/src/windows_compat.c` ne les fournit pas : le cross-build
 * x86_64-pc-windows-msvc échoue au lien (undefined symbol: mmap/munmap/mprotect).
 * Ce shim les implémente avec VirtualAlloc/VirtualProtect/VirtualFree, sans
 * toucher aux sources de rose/src (interdit dans cette mission).
 *
 * Les fonctions Win32 sont DÉCLARÉES À LA MAIN (pas de <windows.h>) : le cache
 * xwin ne contient pas excpt.h, donc windows.h ne se compile pas ici. Sur
 * x86_64 Windows (MSVC ABI) les noms ne sont pas décorés : ces déclarations
 * se résolvent directement depuis kernel32.lib.
 *
 * Constantes POSIX attendues par natif_emetteur.rs :
 *   PROT_READ 0x1  PROT_WRITE 0x2  PROT_EXEC 0x4
 *   MAP_PRIVATE 0x2  MAP_ANONYMOUS 0x20
 *   MAP_FAILED = (void*)-1
 */

typedef unsigned long long size_t_rc;   /* SIZE_T sur x86_64 = 64 bits */
typedef unsigned int DWORD_rc;          /* DWORD = 32 bits */
typedef int BOOL_rc;

extern void *VirtualAlloc(void *lpAddress, size_t_rc dwSize,
                          DWORD_rc flAllocationType, DWORD_rc flProtect);
extern BOOL_rc VirtualProtect(void *lpAddress, size_t_rc dwSize,
                              DWORD_rc flNewProtect, DWORD_rc *lpflOldProtect);
extern BOOL_rc VirtualFree(void *lpAddress, size_t_rc dwSize,
                           DWORD_rc dwFreeType);

#define PROT_READ  0x1
#define PROT_WRITE 0x2
#define PROT_EXEC  0x4

#define MEM_COMMIT   0x00001000
#define MEM_RESERVE  0x00002000
#define MEM_RELEASE  0x00008000

#define PAGE_NOACCESS          0x01
#define PAGE_READONLY          0x02
#define PAGE_READWRITE         0x04
#define PAGE_EXECUTE           0x10
#define PAGE_EXECUTE_READ      0x20
#define PAGE_EXECUTE_READWRITE 0x40

/* Traduit des bits de protection POSIX en une protection de page Win32. */
static DWORD_rc protection_win(int prot) {
    if ((prot & PROT_READ) && (prot & PROT_WRITE) && (prot & PROT_EXEC))
        return PAGE_EXECUTE_READWRITE;
    if ((prot & PROT_READ) && (prot & PROT_WRITE))
        return PAGE_READWRITE;
    if ((prot & PROT_READ) && (prot & PROT_EXEC))
        return PAGE_EXECUTE_READ;
    if (prot & PROT_EXEC)
        return PAGE_EXECUTE;
    if (prot & PROT_WRITE)
        return PAGE_READWRITE;
    if (prot & PROT_READ)
        return PAGE_READONLY;
    return PAGE_NOACCESS;
}

/* Signature alignée sur `unsafe extern "C" fn mmap(addr, len, prot, flags,
 * fd, offset) -> *mut c_void` de natif_emetteur.rs (offset est un i64). */
void *mmap(void *addr, size_t_rc len, int prot, int flags, int fd,
           long long offset) {
    (void)addr;    /* adresse suggérée : on laisse Windows choisir */
    (void)flags;   /* MAP_PRIVATE|MAP_ANONYMOUS seulement dans Rose */
    (void)fd;      /* -1 pour anonyme */
    (void)offset;  /* 0 pour anonyme */
    if (len == 0)
        return (void *)-1;
    void *p = VirtualAlloc((void *)0, len, MEM_COMMIT | MEM_RESERVE,
                           protection_win(prot));
    return p ? p : (void *)-1;   /* MAP_FAILED */
}

int mprotect(void *addr, size_t_rc len, int prot) {
    DWORD_rc ancienne = 0;
    return VirtualProtect(addr, len, protection_win(prot), &ancienne) ? 0 : -1;
}

int munmap(void *addr, size_t_rc len) {
    (void)len;   /* MEM_RELEASE exige dwSize = 0 */
    if (!addr || addr == (void *)-1)
        return -1;
    return VirtualFree(addr, 0, MEM_RELEASE) ? 0 : -1;
}
