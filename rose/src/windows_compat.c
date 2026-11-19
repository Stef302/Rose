/* ═══════════════════════════════════════════════════════════════════════════
   Compatibilité Windows (MinGW) — stubs des symboles POSIX absents de
   Windows : dlopen/dlsym/dlclose (FFI) et getrandom (RNG).
   
   Ce fichier est compilé uniquement pour les cibles Windows (voir build.rs).
   ═══════════════════════════════════════════════════════════════════════════ */

#include <windows.h>
#include <stdint.h>
#include <stddef.h>

#ifndef _SSIZE_T_DEFINED
typedef intptr_t ssize_t;
#define _SSIZE_T_DEFINED
#endif

/* ── FFI dlopen / dlsym / dlclose ── */
/* Mappé sur LoadLibraryA / GetProcAddress / FreeLibrary (Win32 API). */

void *dlopen(const char *filename, int mode) {
    (void)mode;
    return (void *)LoadLibraryA(filename);
}

void *dlsym(void *handle, const char *name) {
    return (void *)GetProcAddress((HMODULE)handle, name);
}

int dlclose(void *handle) {
    return FreeLibrary((HMODULE)handle) ? 0 : -1;
}

/* ── getrandom ── */
/* Signature POSIX : ssize_t getrandom(void *buf, size_t len, unsigned int flags) */
/* Sous Windows, on utilise SystemFunction036 (RtlGenRandom) de advapi32.dll. */

typedef BOOLEAN (WINAPI *RtlGenRandom_t)(PVOID, ULONG);

ssize_t getrandom(void *buf, size_t len, unsigned int flags) {
    (void)flags;
    static RtlGenRandom_t pRtlGenRandom = NULL;
    if (!pRtlGenRandom) {
        HMODULE h = LoadLibraryA("advapi32.dll");
        if (h) {
            pRtlGenRandom = (RtlGenRandom_t)GetProcAddress(h, "SystemFunction036");
        }
    }
    if (pRtlGenRandom && pRtlGenRandom(buf, (ULONG)len)) {
        return (ssize_t)len;
    }
    return -1;
}


/* ── mmap / mprotect / munmap ──────────────────────────────────────────────
   Certains modules (JIT machine natif) appellent ces symboles POSIX même
   quand le binaire Windows ne les utilise pas : fournir des équivalents
   Win32 évite des symboles non résolus à l'édition de liens. */

void *mmap(void *addr, size_t len, int prot, int flags, int fd, long long off) {
    (void)addr; (void)flags; (void)fd; (void)off;
    DWORD p = (prot & 2) ? PAGE_READWRITE : PAGE_EXECUTE_READ;
    return VirtualAlloc(NULL, len, MEM_COMMIT | MEM_RESERVE, p);
}

int mprotect(void *addr, size_t len, int prot) {
    DWORD old = 0;
    DWORD p = (prot & 4) ? PAGE_EXECUTE_READ : PAGE_READWRITE;
    return VirtualProtect(addr, len, p, &old) ? 0 : -1;
}

int munmap(void *addr, size_t len) {
    (void)len;
    return VirtualFree(addr, 0, MEM_RELEASE) ? 0 : -1;
}
