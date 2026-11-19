# G5 → PE64 : writer PE32+ aarch64 en Rose pur

**Constat daté du 19/11/2026.** Binaire utilisé : binaire construit pour ce
chantier dans `/tmp/opencode/pe1_agents/rose_engine` (copie de
`cargo build --release` faite à 16:36, `Rose 0.1.6`). Le binaire livré
`rose/rose` n'a **pas** été remplacé.

## 1. Périmètre — ce qui a été écrit, ce qui n'a pas été touché

Fichier créé (le seul fichier source modifié par ce chantier) :

- `compilateur/emetteur_pe.rose` — **41 971 octets**, `./rose/rose verif
  compilateur/emetteur_pe.rose` → `vérifié`.

Fichier créé pour la documentation : `rose/docs/G5_PE.md` (ce fichier).

Non modifiés (vérifié `git diff --stat` = vide sur ces deux chemins) :

- `compilateur/emetteur_elf.rose` (possédé par SH1) — lu, jamais écrit ;
- `rose/src/link_natif.rs` — lu (modèle du writer), jamais écrit.

⚠ À la clôture, `git status --short` montrait aussi `M rose/src/interp.rs` et
`M rose/src/run.rs` (modifiés à 16:46 par d'autres agents concurrents, **pas**
par ce chantier). Aucune écriture de ce chantier ne les concerne.

`emetteur_pe.rose` contient :

1. le **cœur G5 repris à l'identique** de `emetteur_elf.rose` (lignes 1–1105) :
   helpers `u8le/u16le/u32le/u64le`, `g4_*`, analyseur RIR `g5_parse`,
   compilation `g5_compile_fonction`, runtime `g5_emit_*`, résolution de sauts
   `g5_patch`, dispatch ;
2. `g5_pe()` — le writer PE32+ écrit en Rose pur ;
3. `g5_elf()` — le writer ELF d'origine, recopié, pour le mode `--elf` ;
4. le dispatch : sortie **PE par défaut**, sortie **ELF si `--elf`** est passé
   en second argument.

Rose n'a pas de littéral hexadécimal (`0xAA64` est lu comme la variable
`xAA64`) : toutes les constantes PE sont écrites en décimal dans le source.

## 2. Structure PE produite

Modèle : `rose/src/link_natif.rs::pe_bytes` (machine ARM64, `SizeOfOptionalHeader
= 0xF0`, `.text`,ILT/IAT, nom `Rose PE64`). Constante `PE_IMAGE_BASE =
0x140000000`. Le code machine est **exactement** celui de G5 (aucune
réécriture) : vérifié octet pour octet contre le ELF (§4).

| Champ | Valeur | Preuve (`llvm-readobj --file-headers`) |
|---|---|---|
| Signature DOS | `MZ`, `e_lfanew = 0x80` | `Magic: MZ`, `AddressOfNewExeHeader: 128` |
| Commentaire DOS | `Rose PE64\0` à `0x40` | octets `52 6f 73 65 20 50 45 36 34 00` |
| Signature PE | `PE\0\0` à `0x80` | `Format: COFF-ARM64` |
| Machine | `0xAA64` (ARM64 natif) | `Machine: IMAGE_FILE_MACHINE_ARM64 (0xAA64)` |
| Nombre de sections | 2 | `SectionCount: 2` |
| `SizeOfOptionalHeader` | `0xF0` = 240 | `OptionalHeaderSize: 240` |
| Caractéristiques COFF | `0x22` | `EXECUTABLE_IMAGE`, `LARGE_ADDRESS_AWARE` |
| Magic optional | `0x20B` (PE32+) | `Magic: 0x20B` |
| `AddressOfEntryPoint` | `0x1000` = début de `.text` | `AddressOfEntryPoint: 0x1000` |
| `BaseOfCode` | `0x1000` | `BaseOfCode: 0x1000` |
| `ImageBase` | `0x140000000` | `ImageBase: 0x140000000` |
| Alignements | section `0x1000`, fichier `0x200` | `SectionAlignment: 4096`, `FileAlignment: 512` |
| `SizeOfImage` / `SizeOfHeaders` | `0x3000` / `0x400` | `SizeOfImage: 12288`, `SizeOfHeaders: 1024` |
| Sous-système | 3 (CUI) | `IMAGE_SUBSYSTEM_WINDOWS_CUI (0x3)` |
| Répertoires | 16, dont IMPORT et IAT | `NumberOfRvaAndSize: 16` |
| IMPORT | RVA `0x2020`, taille `0x28` | `ImportTableRVA: 0x2020`, `ImportTableSize: 0x28` |
| IAT | RVA `0x2070`, taille `0x10` | `ImportAddressTableRVA: 0x2070` |

Sections (`llvm-readobj --sections`) :

- `.text` — `VirtualAddress 0x1000`, `VirtualSize` = 4 × nombre de mots G5,
  `SizeOfRawData` aligné sur 512, `PointerToRawData 0x400`,
  caractéristiques `0x60000020` (`CODE|EXECUTE|READ`) ;
- `.data` — `VirtualAddress 0x2000`, `VirtualSize/SizeOfRawData = 0x200`,
  `PointerToRawData` = `0x400 + SizeOfRawData(.text)`, caractéristiques
  `0xC0000040` (`INITIALIZED_DATA|READ|WRITE`).

Table d'imports (`llvm-readobj --coff-imports`) — identique dans les 3 PE :

```
Import {
  Name: KERNEL32.DLL
  ImportLookupTableRVA: 0x2060
  ImportAddressTableRVA: 0x2070
  Symbol: ExitProcess (0)
}
```

Layout de `.data` (offsets relatifs à RVA `0x2000`) : `0x00` table de
trampoline réservée (3 × u64), `0x20` descripteur d'import + descripteur nul,
`0x50` `IMAGE_IMPORT_BY_NAME` (`Hint=0`, `"ExitProcess\0"`), `0x60` ILT,
`0x70` IAT, `0x80` `"KERNEL32.DLL\0"`. md5 de la section `.data` identique
pour les trois programmes : `e02cf1a40bdf…`.

`llvm-readobj --coff-resources` rend `Resources [ ]` (aucune ressource,
attendu : le writer n'émet pas de `.rsrc`).

## 3. Chaîne rejouée — 3 programmes

```
$R rir prog.rose          > prog.rir
$R compilateur/emetteur_pe.rose prog.rir > prog.exe
```

`$R` = `/tmp/opencode/pe1_agents/rose_engine`. `rc=0` pour les 6 commandes,
stderr vide.

| Programme | RIR (o) | ELF G5 (o) | PE (o) | `file` | `.text` VirtualSize | `.text` Raw | `.data` file offset |
|---|---:|---:|---:|---|---:|---:|---:|
| `>> 47` | 100 | 1 196 | 2 560 | PE32+ console Aarch64, 2 sections | `0x3FC` (1 020) | `0x400` | `0x800` |
| `fib(10)` récursif | 353 | 1 372 | 3 072 | idem | `0x4AC` (1 196) | `0x600` | `0xA00` |
| somme `pour i de 1 vers 15` → 105 | 359 | 1 296 | 3 072 | idem | `0x460` (1 120) | `0x600` | `0xA00` |

Tous les autres champs d'en-tête sont identiques entre les trois PE : seuls
`SizeOfCode`, `VirtualSize`/`SizeOfRawData` de `.text` et
`PointerToRawData` de `.data` varient (conséquence directe de la taille du
code machine).

Les ELF de référence exécutés sous Linux donnent bien `47`, `55`, `105`
(`rc=0`) — la chaîne G5 amont est donc saine ; le PE contient le même code.

## 4. Preuve que le writer PE transporte le code G5 non modifié

Comparaison octet pour octet (Python, dans `/tmp/opencode/pe1_agents`) : code
ELF = `elf[176:]`, code PE = `pe[0x400 : 0x400 + VirtualSize]`.

```
t_bin elf_code_len 1020 pe_text_VS 1020 identique True
t_fib elf_code_len 1196 pe_text_VS 1196 identique True
t_sum elf_code_len 1120 pe_text_VS 1120 identique True
```

## 5. Bonus : mode `--elf` dans le nouveau fichier

```
$R compilateur/emetteur_pe.rose prog.rir --elf > prog.mode_elf
$R compilateur/emetteur_elf.rose prog.rir        > prog.elf_ref
```

`cmp -s` → `oui` pour les trois programmes ; tailles 1 196 / 1 372 / 1 296
octets, identiques à la voie historique. `emetteur_elf.rose` n'a pas été
modifié pour autant : le mode est entièrement dans `emetteur_pe.rose`.

## 6. Validation non-régression (constats du 19/11/2026)

- `./rose/rose verif compilateur/emetteur_pe.rose` → `vérifié` (rc=0).
- Point fixe M4.5, deux exécutions avec le binaire construit ici :

  ```
  b084ac5c926f6ae486845f9bc96c616d  fpA.rir
  b084ac5c926f6ae486845f9bc96c616d  fpB.rir
  cmp_rc=0
  ```

  La voie `./rose/rose rir compilateur/emetteur.rose` donne le même md5.
- `cargo test --release` : `220 + 204 + 6 + 1 = 431 passed, 0 failed`
  (3 + 3 ignorés), rc=0.
- `jardins/tests/premiers.rose` : `── jardin/premiers : 20 tests, 0 échec ──`.
- `git diff --stat -- compilateur/emetteur_elf.rose rose/src/link_natif.rs` →
  vide.

## 7. Limites exactes — ne pas prétendre davantage

1. **Aucune exécution Windows n'a eu lieu.** Il n'y a pas de Windows, pas de
   Wine, pas d'émulateur ARM64 Windows sur cette machine. La validation est
   **structurelle** (`file`, `llvm-readobj`, `objdump`) et **binaire** (code
   `.text` identique à l'ELF qui, lui, s'exécute sous Linux). Le PE n'a jamais
   été lancé.
2. Le code machine G5 émis aujourd'hui utilise des **syscalls Linux**
   (`movz x8,#93` + `svc #0` pour `exit`, `svc #0` pour `write`) et une tas
   absolue à `0x60000000`. Ce n'est **pas** un ABI Windows : un tel PE, même
   structurellement valide, ne produirait pas la sortie attendue sous Windows
   sans adaptation du stub d'entrée (cf. `link_natif.rs::stub_depart`, ABI
   Win64 + `ExitProcess` via l'IAT). La table d'imports est émise et reconnue
   par `llvm-readobj`, mais le code émis ne l'appelle pas.
3. Un port x86-64 de G5 n'a pas été fait : il n'était pas requis. Le PE est
   **PE32+ aarch64** (`0xAA64`), ce qui est une cible Windows valide.
4. Aucune signature, aucun manifeste, aucune ressource, aucune relocation,
   aucun `.pdata`/`.edata` : writer minimal volontaire. Ces champs sont à zéro
   et vérifiés comme tels.
5. Le point d'entrée pointe directement sur le premier mot G5 (pas de stub
   Win64), conformément au choix 2. Le reste des champs suit le modèle Rust.

## 8. Commandes de rejeu

```sh
cd /home/clausake/Stefan/ROSE
R=/tmp/opencode/pe1_agents/rose_engine
$R rir /tmp/opencode/pe1_agents/t_fib.rose > /tmp/opencode/pe1_agents/t_fib.rir
$R compilateur/emetteur_pe.rose /tmp/opencode/pe1_agents/t_fib.rir \
   > /tmp/opencode/pe1_agents/t_fib.exe
file /tmp/opencode/pe1_agents/t_fib.exe
llvm-readobj --file-headers --sections --coff-imports \
  /tmp/opencode/pe1_agents/t_fib.exe
./rose/rose verif compilateur/emetteur_pe.rose
```
