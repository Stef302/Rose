# M3.2 — Link natif : ELF64 / PE64 sans gcc, clang ni linker

Fichier : `rose/src/link_natif.rs` (déclaré dans `lib.rs` sous
`#[cfg(not(target_arch = "wasm32"))]` ; le câblage CLI reste à la charge de
l'orchestrateur).

Objectif du plan : « `./prog` produit par `rose build` tourne sur une machine
sans gcc ni clang ». On écrit donc les octets du format directement en Rust :
**aucun appel à `cc`/`ld`/`gcc`, aucune dépendance ajoutée, aucun GMP, aucun
jardin Rust**. L'entrée est le code machine de M3.1
(`natif_emetteur::emettre`, `Emitted::code`).

---

## 1. API publique

```rust
pub use crate::natif_emetteur::Architecture; // Aarch64 | X86_64

pub const ADRESSE_BASE: u64 = 0x0040_0000;

#[derive(Clone, Debug)]
pub struct Options {
    pub architecture: Architecture,
    pub adresse_base: u64,        // alignée sur 0x1000
    pub n_slots_total: usize,     // table des slots i64
    pub nom_code: String,         // symbole machine (défaut "rose_code")
}
impl Options {
    pub fn nouveau(architecture: Architecture) -> Self;
    pub fn depuis_emitted(emitted: &Emitted) -> Self; // reprend arch + slots
}
impl Default for Options; // architecture native

#[derive(Debug)]
pub enum ErreurLien {
    Io(std::io::Error), CodeVide, RuntimeRequis, Depassement(String),
}

pub fn elf_bytes(code: &[u8], options: &Options) -> Result<Vec<u8>, ErreurLien>;
pub fn ecrire_elf(code: &[u8], options: &Options, chemin: &Path) -> Result<(), ErreurLien>;
pub fn ecrire_elf_depuis_emitted(emitted: &Emitted, chemin: &Path) -> Result<(), ErreurLien>;

pub fn pe_bytes(code: &[u8], options: &Options) -> Result<Vec<u8>, ErreurLien>;
pub fn ecrire_pe(code: &[u8], options: &Options, chemin: &Path) -> Result<(), ErreurLien>;
pub fn ecrire_pe_depuis_emitted(emitted: &Emitted, chemin: &Path) -> Result<(), ErreurLien>;
```

`ecrire_*` pose les permissions `0755` (unix). `*_depuis_emitted` refuse avec
`ErreurLien::RuntimeRequis` dès que `Emitted::appels` ou `Emitted::fallbacks`
n'est pas vide : le stub de trampoline rend `0`, il ne sait pas encore appeler
l'interpréteur Rose.

### Utilisation minimale

```rust
use rose::link_natif::{ecrire_elf_depuis_emitted, Architecture, Options};
use rose::natif_emetteur::emettre;
use rose::rir::{CorpsRir, Eabstrait, Op};
use std::path::Path;

let corps = CorpsRir {
    ops: vec![Op::RetourAbstrait(Eabstrait::Entier("49".into()))],
    noms: vec![], globaux: vec![], n_slots: 0,
};
let emitted = emettre(&corps, Architecture::Aarch64).unwrap();
ecrire_elf_depuis_emitted(&emitted, Path::new("./prog")).unwrap();
// $ ./prog ; echo $?  → 49
```

---

## 2. Format ELF64 produit

Un seul **ET_EXEC non-PIE, statique**, un seul `PT_LOAD` (RWX) Couvrant tout le
fichier. Aucune relocation, aucun interpréteur dynamique, aucune section `.bss`,
aucun appel à `ld`.

| champ | valeur |
|---|---|
| `e_ident` | `\x7fELF`, class 2, data 1, version 1, OSABI 0 |
| `e_type` / `e_machine` | `ET_EXEC` / `EM_AARCH64` (183) ou `EM_X86_64` (62) |
| `e_entry` | `adresse_base + 0x100` = `_start` |
| `e_phoff` / `e_phnum` | 64 / 1 |
| `e_shoff` / `e_shnum` / `e_shstrndx` | calculés / 7 / 6 |
| `PT_LOAD` | offset 0, vaddr = base, filesz = memsz = taille fichier, align 0x1000 |

Sections (ordre dans la table) : `[0] NULL`, `1 .text`, `2 .rodata`,
`3 .data`, `4 .symtab`, `5 .strtab`, `6 .shstrtab`. La `.symtab` contient
`_start` (FUNC, `.text`), `rose_code` (FUNC, `.text`) et `rose_tramp` (OBJECT,
`.data`, 24 octets) ; `sh_link = 5`, `sh_info = 1`.

### Stub `_start` (ajouté devant le code émis)

Le code de M3.1 a la signature `i64 f(base: *mut i64, tramp: *const Trampoline)`.
Le stub fournit ces deux arguments puis transforme le retour en code de sortie :

```
aarch64                          x86-64 (SysV)
sub sp, sp, #frame               sub rsp, frame
mov x0, sp        ; base         mov rdi, rsp      ; base
adr x1, tramp                    lea rsi, [rip+d]  ; tramp
bl rose_code                     call rose_code
mov x8, #93  ; __NR_exit         mov rdi, rax      ; code de retour
svc #0                           mov eax, #60 ; __NR_exit
                                 syscall
```

`frame = align16(n_slots_total * 8)` (SP reste aligné sur 16). Le trampoline
embarqué dans `.data` est `[stub_retourne_0, stub_retourne_0, 0]`, soit
`mov x0,#0 ; ret` (aarch64) ou `xor eax,eax ; ret` (x86-64).

### Limites

- `frame <= 4095` sur aarch64 (`sub sp,#imm12`) — soit au plus ~511 slots ;
  au-delà : `ErreurLien::Depassement`.
- Base non-PIE : un seul programme chargé à `0x400000` ; pas de ASLR, pas de
  PIE, pas de relocations (volontaire : c'est ce qui supprime le linker).
- Une seule fonction émise par exécutable (pas encore de multi-corps ni de
  résolution inter-fonctions).
- `AppelRir` / `eval_fallback` → `RuntimeRequis` (runtime Rose embarqué à faire).
- L'écrivain x86-64 ELF est impliqué mais **non exécuté ici** (hôte aarch64).

---

## 3. Format PE64 produit (imports + stub Win64)

Le writer n'écrit plus un simple squelette : il produit un PE32+ dont la table
d'imports est réelle et dont le point d'entrée est un stub Win64 complet.

### En-têtes

| champ | valeur |
|---|---|
| DOS | `MZ`, `e_lfanew = 0x80`, stub `Rose PE64\0` en `0x40` |
| signature | `PE\0\0` en `0x80` |
| COFF `Machine` | `0xAA64` (**ARM64**, pas `0xA641` qui est ARM64EC) ou `0x8664` (AMD64) |
| COFF | 2 sections, `SizeOfOptionalHeader = 0xF0`, characteristics `0x0022` |
| optionnel | **PE32+** (`0x20B`), `ImageBase = 0x14_0000_0000` |
| `AddressOfEntryPoint` | **`0x1000` = le stub**, pas le code émis (correction d'un bug : l'ancien writer sautait directement dans le code, sans prologue) |
| alignements | section `0x1000`, fichier `0x200`, `SizeOfHeaders = 0x400`, `SizeOfImage = 0x3000` |
| sous-système | CUI (3), 16 data directories |

`NumberOfRvaAndSizes = 16` et deux entrées sont renseignées (les autres
restent à zéro) :

- `[1] IMAGE_DIRECTORY_ENTRY_IMPORT` → RVA `0x2020`, taille `40` ;
- `[12] IMAGE_DIRECTORY_ENTRY_IAT` → RVA `0x2070`, taille `16`.

### Table d'imports (dans `.data`)

| RVA | contenu |
|---|---|
| `0x2000` | table de trampoline : 3 × u64 (`appel_rir`, `eval_fallback`, `ctx`) |
| `0x2020` | `IMAGE_IMPORT_DESCRIPTOR` : ILT `0x2060`, Name `0x2080`, FirstThunk `0x2070` |
| `0x2034` | descripteur nul (terminateur) |
| `0x2050` | `IMAGE_IMPORT_BY_NAME` : Hint `0` + `"ExitProcess\0"` |
| `0x2060` | ILT : `0x2050`, puis `0` (entrées PE32+ de 8 octets) |
| `0x2070` | IAT : `0x2050`, puis `0` (patchée par le loader au chargement) |
| `0x2080` | `"KERNEL32.DLL\0"` |

Le loader résout `ExitProcess` et écrit son adresse dans l'IAT en `0x140002070`.
Le stub appelle cette case via `call [rip+d32]` (x86-64) ou `adr x9 / ldr x9,[x9] /
blr x9` (ARM64) : **aucun appel à `dlsym`/`GetProcAddress` ni à un linker**.

### Stub d'entrée Win64

Les deux architectures réservent la **shadow space de 32 octets** exigée par
l'ABI Microsoft et gardent la pile alignée sur 16 avant chaque appel. Le stub
pose `base` et `tramp` dans les registres attendus par le code émis de M3.1
(`rdi`/`rsi` ou `x0`/`x1`), appelle le code, puis passe `eax`/`w0` à
`ExitProcess` et s'arrête sur `ud2`/`brk #0` si elle revenait jamais.

```
x86-64 (Microsoft x64)                  aarch64 (Windows ARM64)
sub rsp, 40+frame   ; 32 shadow + 8      sub sp, sp, #(32+frame) ; home space
mov rdi, rsp        ; base               mov x0, sp              ; base
movabs rsi, tramp                        adr x1, tramp
call rose_code                           bl rose_code
mov ecx, eax        ; uExitCode          (x0 contient déjà w0)
add rsp, frame      ; → shadow space     add sp, sp, #frame      ; → home space
call [rip+d]        ; IAT ExitProcess    adr x9, IAT ; ldr x9,[x9] ; blr x9
ud2                                      brk #0
```

`frame = align16(n_slots_total × 8)`. Sur x86-64, `40 + frame` et `frame` sont
encodés en `imm8` si ≤ 127, sinon en `imm32` (le cas du programme test,
`frame = 128`). Sur ARM64 la somme `32 + frame` doit tenir dans l'`imm12`
(≤ 4095), d'où une borne explicite `ErreurLien::Depassement`.

### Ce qui reste non prouvé

**L'exécution sous Windows n'a pas été testée** : la machine de développement
est Linux/aarch64. Ce qui est démontré ici est la *structure* (analyseurs PE +
désassemblage + assertions octet par octet) et la cohérence de l'ABI Win64 du
stub. Le comportement réel du loader Windows (résolution de l'import, stack
alignment acceptée, code de sortie observé) reste à valider sur une machine
Windows — voir la procédure du §7.

---

## 4. Preuves réelles (exécuté le 17/11/2026, hôte Linux/aarch64)

### 4.1 Exécution ELF aarch64 — **prouvée**

```
$ uname -m
aarch64

$ cd rose && export PATH="$HOME/.cargo/bin:$PATH"
$ cargo test --lib link_natif -- --nocapture
running 7 tests
test link_natif::tests::runtime_requis_est_refuse ... ok
test link_natif::tests::entete_elf_aarch64_est_valide ... ok
test link_natif::tests::elf_aarch64_retourne_49_en_reel ... ok
test link_natif::tests::pe_arm64_machine_et_stub_blr ... ok
test link_natif::tests::pe_ecrit_fichiers_pour_outils_externes ... ok
test link_natif::tests::pe_import_et_stub_win64_sont_complets ... ok
test link_natif::tests::squelette_pe_a_la_bonne_structure ... ok
ELF exécuté : /tmp/rose_m3_2_prog.elf → code 49
test result: ok. 7 passed; 0 failed

$ file /tmp/rose_m3_2_prog.elf
ELF 64-bit LSB executable, ARM aarch64, version 1 (SYSV), statically linked, not stripped

$ readelf -h /tmp/rose_m3_2_prog.elf | grep -E 'Type:|Machine:|Entry point'
  Type:                              EXEC (Executable file)
  Machine:                           AArch64
  Entry point address:               0x400100

$ /tmp/rose_m3_2_prog.elf; echo $?
49
```

Le programme testé est `retour 49` abaissé en RIR
(`RetourAbstrait(Entier("49"))`) puis émis par `natif_emetteur` — ce n'est pas
un octet machine codé en dur dans le test de link.

### 4.2 Structure PE64 — **prouvée** ; exécution Windows — **non prouvée**

Les fichiers sont écrits par le test `pe_ecrit_fichiers_pour_outils_externes`
dans `/tmp/opencode/` (2048 octets chacun).

```
$ file rose_m3_2_pe_x86_64.exe rose_m3_2_pe_arm64.exe
rose_m3_2_pe_x86_64.exe: PE32+ executable (console) x86-64, for MS Windows, 2 sections
rose_m3_2_pe_arm64.exe:  PE32+ executable (console) Aarch64, for MS Windows, 2 sections

$ xxd -l 4 rose_m3_2_pe_x86_64.exe
00000000: 4d5a 0000                 MZ..
$ xxd -s 0x80 -l 12 rose_m3_2_pe_x86_64.exe
00000080: 5045 0000 6486 0200 0000 0000  PE..d.......

$ objdump -x rose_m3_2_pe_arm64.exe | head -45
rose_m3_2_pe_arm64.exe:     file format pei-aarch64-little
architecture: aarch64, flags 0x0000012f:
start address 0x0000000140001000
Magic                020b (PE32+)
AddressOfEntryPoint  0000000000001000
ImageBase            0000000140000000
NumberOfRvaAndSizes  00000010
Entry 1 0000000000002020 00000028 Import Directory [parts of .idata]
Entry c 0000000000002070 00000010 Import Address Table Directory

$ objdump -x rose_m3_2_pe_arm64.exe | sed -n '/The Import Tables/,+10p'
There is an import table in .data at 0x140002020
The Import Tables (interpreted .data section contents)
 vma:            Hint    Time      Forward  DLL       First
                 Table   Stamp     Chain    Name      Thunk
 00002020  00002060 00000000 00000000 00002080 00002070
    DLL Name: KERNEL32.DLL
    vma:  Hint/Ord Member-Name Bound-To
    2050      0  ExitProcess
 00002034  00000000 00000000 00000000 00000000 00000000

$ objdump -d -M no-aliases rose_m3_2_pe_arm64.exe | sed -n '6,16p'
   140001000: d10283ff  sub  sp, sp, #0xa0        ; 32 home + 128 frame
   140001004: 910003e0  add  x0, sp, #0x0         ; base
   140001008: 10000261  adr  x1, 0x140001054      ; tramp
   14000100c: 94000006  bl   0x140001024          ; rose_code
   140001010: 910203ff  add  sp, sp, #0x80        ; → home space
   140001014: 100082e9  adr  x9, 0x140002070      ; IAT
   140001018: f9400129  ldr  x9, [x9]
   14000101c: d63f0120  blr  x9                   ; ExitProcess
   140001020: d4200000  brk  #0x0
```

GNU `objdump` de cette machine n'a pas de backend PE x86-64 (`file format not
recognized`) : on utilise `llvm-readobj` / `llvm-objdump` pour ce variant.

```
$ llvm-readobj --file-headers --sections --coff-imports rose_m3_2_pe_x86_64.exe
Format: COFF-x86-64
  Machine: IMAGE_FILE_MACHINE_AMD64 (0x8664)
  AddressOfEntryPoint: 0x1000
  ImportTableRVA: 0x2020
  ImportTableSize: 0x28
  IATRVA: 0x2070
  IATSize: 0x10
  Name: .text (VirtualSize 0x53, VA 0x1000, CODE|EXECUTE|READ)
  Name: .data (VirtualSize 0x200, VA 0x2000, INITIALIZED_DATA|READ|WRITE)
Import {
  Name: KERNEL32.DLL
  ImportLookupTableRVA: 0x2060
  ImportAddressTableRVA: 0x2070
  Symbol: ExitProcess (0)
}

$ llvm-objdump -d --no-show-raw-insn rose_m3_2_pe_x86_64.exe | sed -n '5,16p'
0000000140001000 <.text>:
140001000:  subq   $0xa8, %rsp              ; 32 shadow + 8 align + 128 frame
140001007:  movq   %rsp, %rdi               ; base
14000100a:  movabsq $0x140001050, %rsi      ; tramp
140001014:  callq  0x14000102a              ; rose_code
140001019:  movl   %eax, %ecx               ; uExitCode
14000101b:  addq   $0x80, %rsp              ; → shadow space
140001022:  callq  *0x1048(%rip)            # 0x140002070  (IAT ExitProcess)
140001028:  ud2

$ readelf -h rose_m3_2_pe_x86_64.exe
readelf: Error: Not an ELF file - it has the wrong magic bytes at the start
```

Le test `pe_import_et_stub_win64_sont_complets` vérifie en plus, octet par
octet : `MZ`, `e_lfanew = 0x80`, `PE\0\0`, machine `0x8664`, magic `0x20B`,
`AddressOfEntryPoint == 0x1000`, les deux data directories, les champs du
descripteur d'import, le descripteur nul, `"ExitProcess\0"`, `"KERNEL32.DLL\0"`,
les entrées ILT/IAT, et la séquence complète du stub y compris le `disp32`
du `call [rip+d]` qui vise bien `0x140002070`.

**Aucun de ces outils ne prouve que le PE s'exécute sous Windows** : ils
n'analysent que les octets. Aucun PE n'a été lancé.

---

## 5. Câblage (orchestrateur)

La déclaration existe déjà dans `rose/src/lib.rs` (juste après M3.1) :

```rust
/// M3.2 — écrivains ELF64/PE64 sans gcc, clang ni linker externe.
#[cfg(not(target_arch = "wasm32"))]
pub mod link_natif;
```

Côté CLI (`main.rs`, non modifié ici), l'appel typique :

```rust
use rose::link_natif::{ecrire_elf_depuis_emitted, ecrire_pe_depuis_emitted};
// après abaissement RIR + `natif_emetteur::emettre(&corps, Architecture::native())`
ecrire_elf_depuis_emitted(&emitted, std::path::Path::new(&sortie))?;
// sur Windows, le writer PE impose l'ABI Win64 :
ecrire_pe_depuis_emitted(&emitted, std::path::Path::new(&sortie))?;
// repli conseillé si Err(ErreurLien::RuntimeRequis) : garder la voie JIT/interprétée
```

---

## 6. Ce qui reste

1. **Exécution Windows à tester** : structure PE prouvée, mais aucun lancement
   sous Windows n'a été fait (voir §7 pour la procédure exacte).
2. **ABI du code émis (M3.1)** : les appels de trampoline restent SysV ; voir §8.
3. **Runtime Rose embarqué** : brancher `appel_rir` / `eval_fallback` sur
   l'interpréteur compilé dans l'exécutable, pour lever `RuntimeRequis`.
4. **Multi-fonctions** : plusieurs corps RIR + résolution des appels internes
   par symboles (la `.symtab` est déjà le bon point d'accroche).
5. **PIE / ASLR** : relocations `R_AARCH64_RELATIVE` si l'on veut charger à
   une adresse quelconque.

---

## 7. Valider le PE sur une machine Windows (procédure à exécuter là-bas)

Cette procédure n'a **pas** été exécutée ici (hôte Linux/aarch64). Sur Windows
x64, copier `rose_m3_2_pe_x86_64.exe` puis, dans `cmd` :

```
dumpbin /headers rose_m3_2_pe_x86_64.exe
    → machine 0x8664, magic PE32+, AddressOfEntryPoint 1000,
      Import Directory 2020, IAT 2070
dumpbin /imports rose_m3_2_pe_x86_64.exe
    → KERNEL32.DLL : ExitProcess
rose_m3_2_pe_x86_64.exe
echo %ERRORLEVEL%
    → 49  (preuve que le loader a résolu ExitProcess et que le code a tourné)
```

Équivalent PowerShell (sorties structurées) :

```powershell
$env:PATH = "$env:PATH;C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\<ver>\bin\Hostx64\x64"
dumpbin /imports .\rose_m3_2_pe_x86_64.exe
.\rose_m3_2_pe_x86_64.exe
$LASTEXITCODE          # doit valoir 49
```

Sur ARM64 Windows (machine ARM64 ou émulation), utiliser
`rose_m3_2_pe_arm64.exe` : `dumpbin /headers` doit indiquer
`0xAA64 (Aarch64)` ; le comportement attendu est le même code de sortie 49.
Si l'exécution échoue, regarder en premier : (a) le code de sortie réel,
(b) `dumpbin /imports` (résolution de `KERNEL32.DLL!ExitProcess`), (c) un
éventuel décalage de pile — le stub réserve 32 o de shadow + 8 o d'alignement
sur x64, 32 o de home space sur ARM64.

Ce qui constituera une **preuve d'exécution** : observer `49` comme code de
retour du processus. Tant que cette commande n'a pas été lancée, l'état exact
est « structure valide, exécution non testée ».

---

## 8. ABI : SysV/AAPCS64 vs Win64 — ce qui est corrigé et ce qui bloque encore

### 8.1 Différences (x86-64)

| | SysV AMD64 (ELF, Linux/macOS) | Microsoft x64 (PE, Windows) |
|---|---|---|
| arguments entiers 1..4 | `rdi`, `rsi`, `rdx`, `rcx` | **`rcx`, `rdx`, `r8`, `r9`** |
| arguments 5+ | `r8`, `r9`, puis pile | **tous sur la pile**, après 32 o de shadow |
| shadow space | aucune | **32 octets réservés par l'appelant**, même avec < 4 args |
| alignement pile | RSP ≡ 0 (mod 16) avant `call` | identique (callee voit RSP ≡ 8) |
| red zone | 128 o sous RSP utilisables | **aucune** |
| sauvegardés | `rbx`, `rbp`, `r12`–`r15` | `rbx`, `rbp`, **`rdi`, `rsi`**, `r12`–`r15`, `xmm6`–`15` |
| retour | `rax` | `rax` |

ARM64 : AAPCS64 et Windows ARM64 partagent `x0`–`x7`, mais Windows exige
**32 octets de home space** et une pile alignée sur 16 ; `x8` y est le registre
de résultat indirect (l'émetteur utilise `x9` comme scratch, ce qui l'évite —
correct).

### 8.2 Corrigé dans `link_natif.rs` (ce chantier)

Le **stub d'entrée** est paramétré par `Abi::{SysV, Win64}` :
`elf_bytes` utilise SysV (syscall `exit`), `pe_bytes` impose Win64 (shadow
space, alignement 16, `ExitProcess` via l'IAT, `ud2`/`brk` final). L'ancien
PE avait trois défauts structurels, tous corrigés : `Machine = 0xA641`
(ARM64EC) au lieu de `0xAA64`, `AddressOfEntryPoint` pointant sur le code émis
au lieu du stub, et aucune table d'imports.

L'interface stub → corps émis reste volontairement `rdi`/`rsi` (x86-64) et
`x0`/`x1` (ARM64) : c'est le stub qui les pose explicitement, et son propre
prologue est le seul appelant du corps. Un programme sans `AppelRir` ni
`eval_fallback` (ceux que `verifier_sans_runtime` accepte) n'appelle aucune
fonction Win64 depuis le corps : **le stub Win64 est donc cohérent avec le
corps tel quel**.

### 8.3 Bloque encore — hors périmètre (`natif_emetteur.rs`, M3.1)

Dès que le corps appelle la trampoline, l'écart d'ABI devient réel et ne peut
pas être comblé depuis `link_natif.rs` :

- `Builder::appel_runtime` (x86-64) charge les 4 arguments dans
  `rdi, rsi, rdx, rcx` (`ARG_REGS = [7,6,2,1]`) et le nom dans `r8` via
  `lea r8,[rip]`. En Win64 il faut `rcx, rdx, r8, r9`, le 5e argument (nom) sur
  la pile à `[rsp+32]`, et **réserver 32 octets de shadow space** avant le
  `call rax`. Les pointeurs de la table `Trampoline` sont `extern "C"` : sous
  MSVC x64 cela signifie Win64, donc l'appel actuel transmettrait des
  ordures aux 4 premiers paramètres.
- `Builder::fallback_runtime` (x86-64) pose `rdi = ctx`, `rsi = id`,
  `rdx = base` : en Win64 il faut `rcx`, `rdx`, `r8` + shadow space.
- Le prologue de l'émetteur sauvegarde bien `rbp`/`rbx`/`r12` (tous
  callee-saved en Win64), mais il ne réserve pas la shadow space pour ses
  propres appels.
- Sur ARM64 les registres sont compatibles, mais le `blr x9` de la trampoline
  doit lui aussi être entouré de 32 octets de home space et d'un SP aligné.

Correctif structurel à faire dans `natif_emetteur.rs` (fichier de l'agent
M3.1, non modifié ici) : rendre l'émission des appels de trampoline
dépendante d'un `Abi`, avec une variante Win64 qui réserve la shadow space et
remappe les registres. Tant que ce n'est pas fait, `AppelRir` et
`eval_fallback` restent refusés par `ecrire_*_depuis_emitted`
(`ErreurLien::RuntimeRequis`) — ce qui évite de produire un binaire
silencieusement faux.
