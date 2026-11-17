# M3.2 — Link natif : ELF64 / PE64 sans gcc, clang ni linker

Fichier : `rose/src/link_natif.rs` (nouveau, **non déclaré** dans `lib.rs` /
`main.rs` — le câblage est à la charge de l'orchestrateur).

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

## 3. Format PE64 produit (squelette)

Structure cohérente, écrite à la main :

- DOS header `MZ` + `e_lfanew = 0x80` + petit stub ;
- signature `PE\0\0` ;
- COFF : `Machine = 0xA641` (ARM64) ou `0x8664` (AMD64), 2 sections,
  `SizeOfOptionalHeader = 0xF0`, characteristics `0x0022` ;
- optional header **PE32+** (`0x20B`) : image base `0x140000000`, entry RVA
  `0x1000 + taille_stub`, section alignment `0x1000`, file alignment `0x200`,
  sous-système CUI (3), 16 data directories à zéro ;
- sections `.text` (RVA `0x1000`, `CODE|EXECUTE|READ`) et `.data`
  (RVA `0x2000`, `INITIALIZED_DATA|READ|WRITE`) ; trampoline 24 octets dans
  `.data`.

**Limite bloquante pour l'exécution** : le stub d'entrée appelle le code émis
puis fait `ret`. Le loader Windows n'attend pas de `ret` : il manque un appel
à `ExitProcess` (table d'imports `kernel32.dll`) et la gestion des conventions
d'appel Win64 pour la voie x86-64 (le code de M3.1 est SysV : rdi/rsi). Le
writer est donc **structurellement valide et type-vérifié**, mais non
exécutable en l'état. Test automatique : en-têtes et noms de sections uniquement.

---

## 4. Preuve d'exécution réelle (exécuté le 17/11/2026, hôte aarch64)

```
$ uname -m
aarch64

$ cargo check --lib                       # dans une copie de contrôle avec la ligne pub mod ajoutée
    Finished `dev` profile ...             # 0 erreur, 0 avertissement de link_natif

$ cargo test --lib link_natif -- --nocapture
running 4 tests
test link_natif::tests::runtime_requis_est_refuse ... ok
test link_natif::tests::squelette_pe_a_la_bonne_structure ... ok
test link_natif::tests::entete_elf_aarch64_est_valide ... ok
ELF exécuté : /tmp/opencode/rose_m3_2_prog.elf → code 49
test link_natif::tests::elf_aarch64_retourne_49_en_reel ... ok
test result: ok. 4 passed; 0 failed

$ file rose_m3_2_prog.elf
ELF 64-bit LSB executable, ARM aarch64, version 1 (SYSV), statically linked, not stripped

$ readelf -h rose_m3_2_prog.elf | grep -E 'Type:|Machine:|Entry point'
  Type:                              EXEC (Executable file)
  Machine:                           AArch64
  Entry point address:               0x400100

$ ./rose_m3_2_prog.elf; echo $?
49
```

Le programme testé est `retour 49` abaissé en RIR (`RetourAbstrait(Entier("49"))`)
puis émis par `natif_emetteur` — ce n'est pas un octet machine codé en dur dans
le test de link.

---

## 5. Lignes à cibler pour le câblage (orchestrateur)

Dans `rose/src/lib.rs`, juste après le bloc M3.1 :

```rust
/// M3.2 — écrivains ELF64/PE64 sans gcc, clang ni linker.
#[cfg(not(target_arch = "wasm32"))]
pub mod link_natif;
```

Côté CLI (`main.rs`, non modifié ici), l'appel typique :

```rust
use rose::link_natif::ecrire_elf_depuis_emitted;
// après abaissement RIR + `natif_emetteur::emettre(&corps, Architecture::native())`
ecrire_elf_depuis_emitted(&emitted, std::path::Path::new(&sortie))?;
// repli conseillé si Err(ErreurLien::RuntimeRequis) : garder la voie JIT/interprétée
```

---

## 6. Ce qui reste

1. **Runtime Rose embarqué** : brancher `appel_rir` / `eval_fallback` sur
   l'interpréteur compilé dans l'exécutable (section `.data` + tableau de
   noms), pour lever `RuntimeRequis`.
2. **PE exécutable** : table d'imports `kernel32!ExitProcess`, adaptation ABI
   Win64, test sous Windows.
3. **Multi-fonctions** : plusieurs corps RIR + résolution des appels internes
   par symboles (la `.symtab` est déjà le bon point d'accroche).
4. **PIE / ASLR** : relocations `R_AARCH64_RELATIVE` si l'on veut charger à
   une adresse quelconque.
