// ═══════════════════════════════════════════════════════════════════════
 // RUNTIME Win64 — « compiler-rt » PE de Rose (AOT8)
 // ═══════════════════════════════════════════════════════════════════════
 //
 // Ce module est le pendant Win64 des runtimes machine ELF
 // (`rose/runtime/{aarch64.S,x86_64.s}`). Là où l'ELF utilise des syscalls
 // Linux (`svc #0` / `syscall`), le PE utilise la table d'imports Win32 :
 // chaque routine est un petit assemblage d'instructions x86-64 qui fait un
 // appel indirect `call qword ptr [rip+disp32]` vers une entrée de l'IAT
 // (Import Address Table) patchée par le loader Windows.
 //
 // Aucune de ces routines n'est un interpréteur : ce sont de vraies
 // instructions machine émises dans le `.text` du PE par `link_natif.rs`.
 //
 // API kernel32 importée (voir `link_natif::pe_bytes`) :
 //   [0] ExitProcess      — terminer le processus (code de retour)
 //   [1] GetStdHandle     — récupérer STD_OUTPUT_HANDLE / STD_INPUT_HANDLE
 //   [2] WriteFile        — écrire les octets mémorisés sur stdout
 //   [3] ReadFile         — lire l'entrée standard (runtime E/S)
 //   [4] GetProcessHeap   — heap du processus (allocateur des valeurs taguées)
 //   [5] HeapAlloc        — allouer un bloc (listes/tables/rationnels)
 //
 // Convention Win64 : 4 arguments dans RCX/RDX/R8/R9, 32 octets de shadow
 // space réservés par l'appelant, RSP aligné sur 16 avant chaque `call`.
 // L'entrée `.text` du PE réserve 40 octets (32 de shadow + 8 d'alignement)
 // plus la table de slots : toutes les routines ci-dessous peuvent donc
 // appeler kernel32 immédiatement.
 //
 // Licence MIT.

/// Noms importés de `KERNEL32.DLL`, dans l'ordre exact de l'IAT.
pub const IMPORTS: [&str; 6] = [
    "ExitProcess",
    "GetStdHandle",
    "WriteFile",
    "ReadFile",
    "GetProcessHeap",
    "HeapAlloc",
];

/// `STD_OUTPUT_HANDLE = (DWORD)-11`.
pub const STD_OUTPUT_HANDLE: u32 = 0xFFFF_FFFB;
/// `STD_INPUT_HANDLE = (DWORD)-10`.
pub const STD_INPUT_HANDLE: u32 = 0xFFFF_FFF6;

/// Adresses absolues des entrées IAT (ImageBase fixe, PE non-PIE).
#[derive(Clone, Copy, Default, Debug)]
pub struct Iat {
    pub exit_process: u64,
    pub get_std_handle: u64,
    pub write_file: u64,
    pub read_file: u64,
    pub get_process_heap: u64,
    pub heap_alloc: u64,
}

/// `call qword ptr [rip+disp32]` vers une entrée de l'IAT.
/// `text_vaddr` est l'adresse virtuelle de début de `.text`, `v.len()` la
/// position courante dans ce même `.text`.
pub fn call_iat(v: &mut Vec<u8>, text_vaddr: u64, iat_abs: u64) {
    let off = v.len() as u64;
    v.extend_from_slice(&[0xFF, 0x15]);
    let d = iat_abs as i64 - (text_vaddr + off + 6) as i64;
    v.extend_from_slice(&(d as i32).to_le_bytes());
}

/// `mov rcx, imm32` (signe étendu) — le n° de handle Win32.
pub fn mov_ecx_imm32(v: &mut Vec<u8>, x: u32) {
    v.push(0xB9);
    v.extend_from_slice(&x.to_le_bytes());
}

/// `mov rdx, imm64` — adresse du tampon à écrire / lire.
pub fn movabs_rdx(v: &mut Vec<u8>, x: u64) {
    v.extend_from_slice(&[0x48, 0xBA]);
    v.extend_from_slice(&x.to_le_bytes());
}

/// `mov r8d, imm32` — longueur du tampon.
pub fn mov_r8d_imm32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&[0x41, 0xB8]);
    v.extend_from_slice(&x.to_le_bytes());
}

/// `mov r9, imm64` — adresse du `DWORD` qui reçoit le nombre d'octets.
pub fn movabs_r9(v: &mut Vec<u8>, x: u64) {
    v.extend_from_slice(&[0x49, 0xB9]);
    v.extend_from_slice(&x.to_le_bytes());
}

/// `mov qword ptr [rsp+32], 0` (5e argument de WriteFile/ReadFile : lpOverlapped).
pub fn stack_zero_arg5(v: &mut Vec<u8>) {
    v.extend_from_slice(&[0x48, 0xC7, 0x44, 0x24, 0x20, 0, 0, 0, 0]);
}

/// `GetStdHandle(STD_OUTPUT_HANDLE)` → handle dans RAX.
pub fn get_std_handle(v: &mut Vec<u8>, text_vaddr: u64, iat: Iat, input: bool) {
    mov_ecx_imm32(v, if input { STD_INPUT_HANDLE } else { STD_OUTPUT_HANDLE });
    call_iat(v, text_vaddr, iat.get_std_handle);
}

/// `WriteFile(handle, data, len, &written, NULL)`.
/// Le handle doit être dans RCX (juste après `get_std_handle` : on le
/// sauvegarde d'abord avec `mov rcx, rax`).
pub fn write_file(v: &mut Vec<u8>, text_vaddr: u64, iat: Iat,
                  data_addr: u64, len: u32, written_addr: u64) {
    v.extend_from_slice(&[0x48, 0x89, 0xC1]); // mov rcx, rax (handle)
    movabs_rdx(v, data_addr);                 // rdx = données
    mov_r8d_imm32(v, len);                    // r8 = longueur
    movabs_r9(v, written_addr);               // r9 = &octets écrits
    stack_zero_arg5(v);                       // [rsp+32] = NULL
    call_iat(v, text_vaddr, iat.write_file);
}

/// `ExitProcess(code)` : code dans ECX, appel via l'IAT, puis `ud2`.
pub fn exit_process(v: &mut Vec<u8>, text_vaddr: u64, iat: Iat, code_in_ecx: bool) {
    if !code_in_ecx { v.extend_from_slice(&[0x31, 0xC9]); } // xor ecx, ecx
    call_iat(v, text_vaddr, iat.exit_process);
    v.extend_from_slice(&[0x0F, 0x0B]); // ud2
}

/// Routine machine `HeapAlloc` (allouer un bloc pour une valeur composée).
/// Séquence autonome, appelable depuis le code émis :
///   GetProcessHeap() → rcx ; HeapAlloc(heap, 0, taille) → rax.
/// `taille_imm` est la taille en octets ; l'adresse de retour est dans RAX.
pub fn heap_alloc(v: &mut Vec<u8>, text_vaddr: u64, iat: Iat, taille_imm: u32) {
    call_iat(v, text_vaddr, iat.get_process_heap); // rax = heap
    v.extend_from_slice(&[0x48, 0x89, 0xC1]);      // mov rcx, rax
    v.extend_from_slice(&[0x31, 0xD2]);            // xor edx, edx (dwFlags)
    mov_r8d_imm32(v, taille_imm);                  // r8 = taille
    call_iat(v, text_vaddr, iat.heap_alloc);       // rax = pointeur
}

/// Routine machine `ReadFile` (stdin → tampon). Handled dans RCX, tampon dans
/// RDX, capacité dans R8D, adresse du compteur dans R9.
pub fn read_file(v: &mut Vec<u8>, text_vaddr: u64, iat: Iat,
                 data_addr: u64, cap: u32, written_addr: u64) {
    movabs_rdx(v, data_addr);
    mov_r8d_imm32(v, cap);
    movabs_r9(v, written_addr);
    stack_zero_arg5(v);
    call_iat(v, text_vaddr, iat.read_file);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_attendus_sont_presents() {
        assert_eq!(IMPORTS.len(), 6);
        assert!(IMPORTS.contains(&"GetStdHandle"));
        assert!(IMPORTS.contains(&"WriteFile"));
        assert!(IMPORTS.contains(&"ReadFile"));
        assert!(IMPORTS.contains(&"GetProcessHeap"));
        assert!(IMPORTS.contains(&"HeapAlloc"));
        assert!(IMPORTS.contains(&"ExitProcess"));
    }

    #[test]
    fn call_iat_encode_un_deplacement_relatif() {
        let mut v = Vec::new();
        call_iat(&mut v, 0x140001000, 0x1400020F0);
        assert_eq!(&v[..2], &[0xFF, 0x15]);
        let d = i32::from_le_bytes(v[2..6].try_into().unwrap());
        assert_eq!(0x140001000u64 + 6 + d as u64, 0x1400020F0);
    }

    #[test]
    fn sequences_win64_sont_du_code_machine() {
        let iat = Iat { exit_process: 0x1400020F0, ..Iat::default() };
        let mut v = Vec::new();
        get_std_handle(&mut v, 0x140001000, iat, false);
        write_file(&mut v, 0x140001000, iat, 0x140002150, 42, 0x140002140);
        exit_process(&mut v, 0x140001000, iat, false);
        assert!(v.len() > 40);
    }
}
