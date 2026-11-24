# PE32+ x86-64 natif — émetteur compilé par C0 sans cargo (jalon §5.1)

**Constat daté : 24/11/2026.** Worktree exclusif `/tmp/opencode/pe_x64`, HEAD
`a8914cb2f40cc996806a54d3dd310064ea2351dc`. Aucun commit, aucun push, aucun
fichier `rose/src/**`, `link_natif.rs`, `scripts/build-windows-rebuild.sh`,
`emetteur.rose` ou `emetteur_elf.rose` modifié. Garde-fou systématique :
`bash scripts/garde_fou_mem.sh 12000000 <commande>` (pic RSS < 12 000 000 Ko).

Ce chantier porte l'émetteur PE Rose du machine type ARM64 (`0xAA64`) au machine
type **AMD64 (`0x8664`)** et le rend **auto-exécutable sans Rust** : compilé par
C0 en ELF AArch64, il lit un RIR Rose et écrit un PE32+ x86-64 valide dont le
stub affiche un entier puis appelle `ExitProcess`. C'est le socle du jalon §5.1
de `rose/docs/RELEASE_SANS_CARGO.md` (ligne 125 : « porter
`compilateur/emetteur_pe.rose` … à `0x8664` »). La composition de cet émetteur
dans le pilote C0 pour produire le **vrai `rose.exe`** (§5.1 étape 2) reste une
étape ultérieure : ici l'artefact produit est le PE de démonstration
`>> 47` / `fib(10)` / somme, pas l'image complète du moteur.

## 1. Fichiers possédés

| Fichier | Lignes | `fn` | md5 | Rôle |
|---|---:|---:|---|---|
| `compilateur/emetteur_pe_x64.rose` | 928 | 57 | `84a4880292b0397f95f6123eebd8d988` | émetteur RIR → PE32+ AMD64, version native C0 |
| `compilateur/tests_pe_x64.rose` | 129 | 5 | `2915434ad51ee74992eb9df3e1cac29f` | module de 13 assertions (concaténé à l'émetteur) |
| `rose/docs/PE_X64_NATIF.md` | ce document | — | — | preuves et structure |

Base : `compilateur/emetteur_x64.rose` (836 lignes, 53 `fn`, déjà validé en
mode interprété par `rose/docs/PE_X64.md`) + `compilateur/emetteur_pe_sel.rose`
pour les correctifs natifs. Les deux fichiers sources existants sont **intacts**.

## 2. Chaîne exécutée

```sh
cd /tmp/opencode/pe_x64
C0=/tmp/opencode/C0_h.elf            # md5 2db168b907b4a874712857d3957fd078
ROSE=/home/clausake/Stefan/ROSE/rose/target/release/rose   # graine Rust, lecture seule

# 1) source RIR (ici >> 47)
printf '>> 47\n' > prog.rose
$ROSE rir prog.rose > prog.rir

# 2) émetteur compilé par C0, SANS cargo
bash scripts/garde_fou_mem.sh 12000000 timeout 900 \
  $C0 compilateur/emetteur_pe_x64.rose > emetteur_pe_x64_nat.elf
chmod +x emetteur_pe_x64_nat.elf

# 3) PE émis par l'ELF natif
./emetteur_pe_x64_nat.elf prog.rir > prog.exe

# 4) référence interprétée (même source, graine Rust, JIT désactivé :
#    le correctif x_imm64 déclenche un bug du JIT de la graine, voir §3)
ROSE_SANS_JIT=1 $ROSE interprete compilateur/emetteur_pe_x64.rose prog.rir > prog.interp.exe
cmp prog.exe prog.interp.exe         # rc=0
```

L'émetteur natif : **127 360 o**, `ELF 64-bit LSB executable, ARM aarch64,
statically linked`, md5 `e02511616b2c26ed1e2d5365eb375fa0` ; compilation
`rc=0`, stderr vide.

Les PE émis (tous **2 560 o**) :

| RIR source | md5 natif = md5 interprété | `file` |
|---|---|---|
| `>> 47` | `66cee8060a9fc6cb76fd0d4f8d4282f4` | `PE32+ executable (console) x86-64, for MS Windows, 3 sections` |
| `fib(10)` | `48b20203cbbceb5836c76db6754cd976` | idem |
| somme `pour i de 0..10` | `3f3b4cb712349a0246dd1635193e13ac` | idem |

Ces trois md5 reproduisent exactement ceux documentés dans `rose/docs/PE_X64.md`
§3.4 pour l'émetteur interprété : le port natif est **bit-identique**, pas
seulement « structurellement proche ».

## 3. Les quatre correctifs du port natif

Le backend G5 généré par C0 n'a pas exactement les mêmes sémantiques que
l'interpréteur Rust (diagnostics détaillés dans `rose/docs/J2_PE_NATIF.md` §2).
Quatre écarts ont été mesurés puis corrigés :

1. **`et`/`ou` non court-circuités** — `tant_que ($i < longueur($ll) et
   $ll[$i] …)` déréférence `$ll[$i]` hors bornes en fin de boucle (segfault
   `rc=139`, sortie 0 o sur l'émetteur x64 d'origine compilé par C0).
   Correctif : `x_elt_ch($l, $i)` (retourne `""` hors bornes) utilisé dans les
   deux conditions concernées (`x_eval` et `x_parse`).
2. **`partie_entiere(negatif / 256)` tronque vers zéro en natif** — un `rel32`
   de `-44` était émis `d4 00 00 00` au lieu de `d4 ff ff ff`
   (diff `cmp -l` sur 3 octets, mesuré avant correction). Correctif :
   `u32le()` normalise `$u = $v % 4294967296` puis `$u += 4294967296` si
   négatif, avant la décomposition en 4 octets. Le désassemblage final montre
   bien `e9 d4 ff ff ff` (jmp en arrière) dans le stub `print_i64`.
3. **`caractere()` = UTF-8 sous le backend natif** — `ecrire_bytes` écrit
   alors `c2 80` là où l'interpréteur écrit `0x80`, et `longueur()` dénombre
   des points de code. Correctif : tout le PE est construit avec `caractere()`
   (UTF-8 valide), puis `pe_raw()` convertit chaque point de code en octet brut
   via `octet_brut()` juste avant `ecrire_bytes` ; `pe_join()` (jointure
   équilibrée) évite le O(n²) d'une accumulation `+` octet par octet.

4. **Constante 2^64 tronquée dans `x_imm64()`** — l'encodeur d'origine
   (`% 18446744073709551616`) émet un entier négatif sur 8 octets incorrects en
   natif : `>> -47` donnait `movabsq $0xd1` au lieu de `movabsq $-0x2f`
   (md5 natif `79386153…` ≠ md5 interprété `21174056…`). Mesure complémentaire
   sur un programme témoin compilé par C0 : `18446744073709551616 % 1000000007`
   et `(4294967296 * 4294967296) % 1000000007` rendent **0** en natif contre
   **582344008** dans la graine. Correctif : complément à 2 par chunks 16 bits,
   sans constante > 2^48. Le PE de `>> -47` est alors émis `d1 ff ff ff ff ff
   ff ff` dans les deux voies.
   **Limite de la graine** : ce correctif déclenche un bug du **JIT de la
   graine Rust** (`Rose : erreur — index sur une valeur qui n'est pas une
   liste`, reproductible ×3 en modes `jit`/`run`) — un bug de compilation JIT
   du *programme émetteur*, pas du PE émis. La voie interprétée utilisée pour
   les comparaisons est donc `ROSE_SANS_JIT=1 rose interprete` (rc=0) ; le C0
   natif, lui, fonctionne sans aucune de ces options. Plusieurs formulations
   alternatives ont été mesurées : dès que `x_imm64()` appelle une fonction
   auxiliaire ou contient une boucle imbriquée, c'est cette fois le backend C0
   qui segfaulte (`rc=139`) ; la version retenue (chunks, une seule boucle) est
   la seule qui réussisse en natif.

## 4. Validation statique du PE (preuves d'exécution)

Sur `>> 47` (`t47b.x64.exe`) :

| Commande | Résultat mesuré |
|---|---|
| `file` | `PE32+ executable (console) x86-64, for MS Windows, 3 sections` |
| `llvm-readobj --file-headers` | `Machine: IMAGE_FILE_MACHINE_AMD64 (0x8664)`, `Magic: 0x20B`, `AddressOfEntryPoint: 0x1000`, `ImageBase: 0x140000000`, `SectionAlignment: 4096`, `FileAlignment: 512`, `Subsystem: CUI (0x3)`, `ExceptionTableRVA: 0x3000`, `ExceptionTableSize: 0xC` |
| `llvm-readobj --coff-imports` | `KERNEL32.DLL` → `GetStdHandle (0)`, `WriteFile (0)`, `ExitProcess (0)` ; `ImportLookupTableRVA: 0x2060`, `ImportAddressTableRVA: 0x2080` |
| `llvm-readobj --sections` | `.text` RVA `0x1000` (VS `0x10A`), `.data` RVA `0x2000`, `.pdata` RVA `0x3000` (VS `0x24`) |
| `llvm-readobj --unwind` | `RuntimeFunction { Start 0x140001000, End 0x14000110A, UnwindInfo 0x140003018, Version 1, PrologSize 11, UnwindCodeCount 4, PUSH_NONVOL reg=RBP, ALLOC_LARGE size=688 }` |
| `llvm-objdump --triple=x86_64-pc-windows-msvc -d` | `pushq %rbp` / `movq %rsp,%rbp` / `subq $0x2b0,%rsp` / `movabsq $0x2f,%rax` / `callq <print_i64>` / `xorl %ecx,%ecx` / `callq *IAT(ExitProcess)` |

Excodec machine pertinent (`print_i64`, convention MS x64) :

```
1400010d0: b9 f5 ff ff ff        movl $0xfffffff5, %ecx   # STD_OUTPUT_HANDLE = -11
1400010d5: ff 15 a5 0f 00 00     callq *0x140002080       # GetStdHandle via IAT
1400010e9: 49 8b d2              movq %r10, %rdx          # tampon
1400010ec: 4d 8b c1              movq %r9, %r8            # longueur
1400010ef: 4c 8d 8d 68 ff ff ff  leaq -0x98(%rbp), %r9    # &written
1400010f6: 48 c7 44 24 20 00…    movq $0, 0x20(%rsp)      # 5e arg = NULL (shadow space)
1400010ff: ff 15 83 0f 00 00     callq *0x140002088       # WriteFile via IAT
```

`.pdata` contient un `RUNTIME_FUNCTION` de 12 octets, un terminateur nul et une
`UNWIND_INFO` (version 1, prologue 11, 4 codes : `UWOP_PUSH_NONVOL rbp` +
`UWOP_ALLOC_LARGE 688`). `llvm-readobj --unwind` affiche ensuite deux entrées
parasites dues à son analyseur COFF qui parcourt le reste de la section au-delà
de `ExceptionTableSize=0xC` (déjà documenté dans `PE_X64.md` §4) — la table
réelle est correcte.

### Comparaison avec le PE de référence de `rose/tests_aot8/verifier.sh`

Référence générée par
`$ROSE aot prog.rose -o ref_aot.exe --target x86_64-pc-windows-msvc` (2 048 o,
md5 `22fbf72e5d29fc464d2ef3aa37cc206d`) :

| | PE natif x64 Rose | PE AOT8 de référence |
|---|---|---|
| `file` | PE32+ console x86-64, 3 sections | PE32+ console x86-64, 2 sections |
| Machine / Magic | `0x8664` / `0x20B` | `0x8664` / `0x20B` |
| EntryPoint / ImageBase | `0x1000` / `0x140000000` | `0x1000` / `0x140000000` |
| Sections | `.text`, `.data`, `.pdata` | `.text`, `.data` |
| Exception table | RVA `0x3000`, taille `0xC` | absente (`0x0`) |
| Imports KERNEL32 | `GetStdHandle`, `WriteFile`, `ExitProcess` | `ExitProcess`, `GetStdHandle`, `WriteFile`, `ReadFile`, `GetProcessHeap`, `HeapAlloc` |
| Taille | 2 560 o | 2 048 o |

Les deux sont des PE32+ AMD64 valides avec table d'imports `KERNEL32.DLL` ; la
différence d'imports vient du runtime : l'émetteur Rose embarque son propre
`print_i64` (pas de `HeapAlloc`) et fournit une table d'exceptions que le
backend Rust n'émet pas. Le test `verifier.sh` (`grep 'Symbol: HeapAlloc'`) porte
sur le backend Rust, pas sur cet émetteur.

## 5. Exécution sous Wine — impossible ici (cause mesurée)

| Vérification | Résultat |
|---|---|
| `uname -m` | `aarch64` |
| `wine --version` | `wine-9.0 (Ubuntu 9.0~repack-4build3)` + « wine32 is missing » |
| `file -L /usr/lib/wine/wine64` | `ELF 64-bit LSB pie executable, ARM aarch64` — **binaire hôte**, pas un wine x86-64 |
| `command -v wine64` | absent ; `command -v wine32` absent |
| `timeout 45 wine prog.exe` (x64) | `rc=124`, stdout vide, stderr = message wine32 |
| `timeout 45 WINEARCH=win64 wine prog.exe` | `rc=124`, idem |
| `timeout 30 /usr/lib/wine/wine64 prog.exe` | `rc=124`, fixmes SEH `unknown CFA opcode 2d`, aucun stdout |
| `timeout 45 wine arm64_01.exe` (PE AArch64 témoin) | `rc=124` également — Wine ne fonctionne pas du tout ici |
| `command -v qemu-x86_64-static` | présent, mais émulateur **Linux-user** : ne charge pas un PE |

Donc : Wine est installé mais **inutilisable sur cet hôte aarch64** (pas de
WoW64, pas de wine32/i386). Aucun PE n'a été exécuté ; la validation est
**statique** (`file`, `llvm-readobj`, `llvm-objdump`) et par égalité octet pour
octet avec la voie interprétée. Aucune sortie « 47 » n'est revendiquée comme
ayant été observée à l'exécution ici : c'est la valeur encodée dans le stub,
confirmée par le désassemblage (`movabsq $0x2f, %rax`) et par le moteur Rose.

## 6. Corpus et tests

- **25 RIR** générés depuis `rose/tests_aot8/programmes/*.rose`, émis par les
  deux voies (native C0 et interprétée) : **11/11 programmes couverts
  bit-identiques, 0 divergence** ; 14 programmes hors sous-ensemble G5
  échouent **de la même manière** dans les deux voies (rc=2, messages
  `op bin non couverte : ^`, `forme non couverte : fact/s/lst`, `appel non
  résolu : table/hasard`) — pas une régression du port.
- **13/13 assertions** de `compilateur/tests_pe_x64.rose` passent dans les deux
  moteurs, avec des sorties **octet-identiques** :
  `ROSE_SANS_JIT=1 rose interprete` (tests_prog.rose) → `TOTAL 13/13` ;
  `C0 tests_prog.rose > tests.elf && ./tests.elf` → `TOTAL 13/13`.
  Elles couvrent `u32le(-44) = d4 ff ff ff`, `x_elt_ch` hors bornes,
  `x_imm64(-47) = d1 ff ff ff ff ff ff ff`, et les champs MZ/PE/machine/0x20B/
  sections de `x_pe()`.
- `rose verif compilateur/emetteur_pe_x64.rose` → `vérifié`.

## 7. Non-régressions (étape 4)

Vérifié après le port, sur le même worktree :

| Contrôle | Résultat |
|---|---|
| `rose verif compilateur/emetteur_pe.rose` (PE ARM64) | `vérifié` |
| `rose verif compilateur/emetteur_pe_sel.rose` (PE ARM64 natif) | `vérifié` |
| PE ARM64 émis par l'ELF natif C0 (`emetteur_pe_sel.rose`) | `file` = `PE32+ executable (console) Aarch64, 3 sections`, machine `0xAA64`, imports KERNEL32 ×6, md5 `e7b9fa82468d0e97fd9dcb99b33e47c5` (égal au constat J2) |
| Point fixe `rose rir compilateur/emetteur.rose \| md5sum` ×2 | `b3ed9262dd54776a6dbd07bc12368d0e` (stable) |
| `validation_finale.sh` | voir §8 |
| `git status --short` | uniquement les 2 nouveaux fichiers `.rose` + ce document + le dossier temporaire `.build_pe_x64/` (artefacts non commités). Aucun fichier suivi modifié. |

## 8. Résultat `validation_finale.sh`

Commande : `RUST_TEST_THREADS=1 bash scripts/garde_fou_mem.sh 12000000 timeout
3600 bash scripts/validation_finale.sh` → **rc=0**,
`=== RESULTAT: 0 échec(s) ===` (journal `.build_pe_x64/validation_finale2.out`).
Détail mesuré : build Rust + tests `314 passed / 0 failed`, `327 passed /
0 failed` ; point fixe `b3ed9262…` ×2 ; AOT `75 OK, 0 ECHEC` ; JIT `25/25` ;
WASM 92 o ; RVM `317811` ; AZ `31 CONFORME, 0 non-conforme` ; HOST-1
`fib=317811`.

Note d'environnement : le premier passage avait rendu `rc=1` avec
`AZ 0 CONFORME, 31 non-conforme`. Cause **prouvée** par le journal
`/tmp/opencode/vf_az.log` : `tests_selfhost/lancer_az.sh` exécute
`$ROOT/rose/rose`, absent de ce worktree (les autres étapes utilisent `$B`, la
graine externe), d'où 31 × `timeout: failed to run command … No such file or
directory`. Un lien symbolique gitignoré `rose/rose` → la graine
`/home/clausake/Stefan/ROSE/rose/target/release/rose` a été posé dans le
worktree, puis le passage complet a été rejoué : **0 échec**. Ce lien n'est pas
un fichier suivi et n'est pas une modification du dépôt.
