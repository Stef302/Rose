# J2 — backend PE natif (émetteur PE compilé en ELF par C0)

Chantier J2 de `rose/docs/SELFHOST_EXTENSION.md` §7 : `rose_natif_pe.rose`
(207 `fn`) émet un PE32+ valide, mais seulement **interprété par la graine
Rust**. Ce document rapporte la construction d'un émetteur PE **compilé en
code natif** : `compilateur/emetteur_pe_sel.rose`, compilé par C0 en un ELF
AArch64 autonome qui lit un RIR Rose et écrit un PE32+ AArch64.

Toutes les mesures ci-dessous sont des constats datés du 23/11/2026 dans le
worktree `/tmp/opencode/opt_bench` (HEAD `62ab54c91f1778f5c201898abf50607afbabcb8d`).
Aucun commit, push ou publication. Garde-fou systématique :
`bash scripts/garde_fou_mem.sh 12000000 <commande>`.

## 1. Comparaison des deux backends (étape 1)

| Fichier | Lignes | `fn` | Rôle |
|---|---:|---:|---|
| `compilateur/emetteur_pe.rose` | 1 591 | 94 | RIR → PE32+ AArch64 (machine G5 + writer PE + stubs Win32) |
| `compilateur/emetteur_elf.rose` | 7 393 | — | RIR → ELF64 AArch64 (machine G5 + writer ELF + runtime Linux) |
| `compilateur/rose_natif_pe.rose` | 5 719 | 207 | composite généré (front-end `emetteur.rose` + backend PE), interprété |

**Routines G5 partagées : 77** (comparaison des listes de `fn` par `comm -12`) :
`g4_pos`, `g4_lignes`, `g4_trim`, `g4_operand`, `g4_entier_ou_zero`, `g5_nu_op`,
`g5_sub`, `g5_next`, `g5_wrap`, `g5_emit`, `g5_branch`, `g5_mark`,
`g5_labelidx`, `g5_fail`, `g5_tokens`, `g5_unescape`, `g5_make_str`, `g5_noms`,
`g5_slot_of`, `g5_parse`, `g5_compile_fonction`, `g5_op`, `g5_eval`,
`g5_cible`, `g5_emit_print`, `g5_emit_alloc`, `g5_emit_len`, `g5_emit_ordre`,
`g5_emit_sous`, `g5_emit_idx`, `g5_emit_join`, `g5_emit_inser`,
`g5_emit_entier`, `g5_emit_stub0`, `g5_emit_stubs`, `g5_patch`, `g5_elf`,
`g5_load`, `g5_framesize`, `g5_cond`, `g5_func_id`, `g5_slotoff`,
`g5_tmpoff`, `g5_argoff`, `g5_listoff`, `g5_alloc_inline`, `u8le`, `u16le`,
`u32le`, `u64le` et les 29 encodeurs AArch64 (`movz`, `movk`, `addi`, `subi`,
`addr`, `subr`, `andr`, `orrr`, `eorr`, `mulr`, `sdivr`, `udivr`, `msubr`,
`cmpr`, `cmpi`, `ldroff`, `stroff`, `strboff`, `ldrboff`, `cset`, `strbio`,
`ldrbio`, `strbr`, `ldrbr`, `movzs`, `RET_I`, `SVC_I`, …).

**Routines propres au PE (17)** : `g5_pe`, `pe_z`, `pe_c`, `g5_final_rva`,
`g5_heap`, `g5_load_heap`, `g5_setheap`, `g5_emit_m`, `adrp_i`, `ldr_i`,
`movn_i`, `BR_I`, `BLR_I`, `g5_emit_tramp`, `g5_emit_ecrire_win`,
`g5_emit_lire_win`, `g5_emit_print_win`.

Le cœur `g5_parse` → `g5_compile_fonction` → `g5_patch` est donc le même :
le RIR est analysé en fonctions/slots/opérations, chaque opération est
compilée en mots AArch64 (liste `$g5_code`), les branches sont résolues par
`g5_patch()`. La divergence est en aval :

- **ELF** (`emetteur_elf.rose` ≈ l.4480-4650) : dispatch RIR, réservation des
  labels de primitives (IO, table, bigint, UTF-8, …), pointeur de tas absolu
  `0x60000000`, writer `g5_elf()` (3 en-têtes, 2 sections), `ecrire_bytes`.
- **PE** (`emetteur_pe.rose` ≈ l.1485-1591) : même dispatch mais réservation
  des trampolines d'imports, table IAT (`KERNEL32.DLL`), stubs Win64
  (`print_i64`, `ecrire_octets`, `lire_fichier`, `exit`), tas dans `.data`
  (`.data+0x200`), writer `g5_pe()` (DOS/COFF/optionnel PE32+, 3 sections
  dont `.pdata`), plus de `svc #0`.

## 2. Le problème : pourquoi le PE compilé par C0 ne sortait pas

Commande de référence (graine native `C0_opt.elf`, md5
`9fbd21d2b06efe7f1e86ac5b5730d2c8`) :

```
bash scripts/garde_fou_mem.sh 12000000 timeout 1800 \
  /tmp/opencode/C0_opt.elf compilateur/emetteur_pe.rose > emetteur_pe_nat.elf
```

L'ELF produit (`195 380` o, `ELF 64-bit LSB executable, ARM aarch64`) **segfaulte**
sur n'importe quel RIR (`file` de sortie vide, `rc=139`). Trois causes
distinctes, mesurées une par une avec des variantes instrumentées :

### 2.1 `et`/`ou` non court-circuités dans le backend G5 natif

`gdb` sur le RIR minimal : `SIGSEGV` à `pc=0x42d988`, instruction
`ldr x2,[x1]` avec `x1=0`, juste après la boucle de `g5_parse()`
(`si (nb_arguments()≥1)` → `g5_parse($src)`). La condition

```
tant_que ($i < longueur($ll) et g5_sub(g4_trim($ll[$i]), 0, 9) != "fonction ")
```

évalue `$ll[$i]` **même quand `$i == longueur($ll)`** : l'interpréteur Rust
court-circuite, le code natif émis par C0 non. Reproduit en isolé :
`si ($i < longueur($l) et longueur($l[$i]) > 0)` avec `$i=2`, `longueur($l)=2`
→ `rc=139` natif, `hors`/`apres` sous `rose/rose`. `sous_chaine` hors bornes,
lui, est sûr en natif (`""`, mesuré).

**Correctif** : `fn g5_elt_ch($l: liste<chaine>, $i: entier) -> chaine`
(retourne `""` hors bornes) utilisé dans les 4 conditions concernées
(2 boucles sur `$t`, test `neg`, boucle sur `$ll`).

### 2.2 Constantes 64 bits non représentables dans l'ABI G5

Après 2.1 : `rc=2`, aucun octet, `G5 errlen=3`, `ERR0=immédiat hors int64 : -1`.
`g5_load()` de `emetteur_pe.rose` compare à `-9223372036854775808` et
`18446744073709551616`, tous deux tronqués à `0` par l'émetteur G5 (61 bits
utiles) : tout négatif est rejeté. **Correctif** : port du `g5_load()` de
`emetteur_elf.rose` (complément à 2 par chunks ≤ 65 536, sans constante > 2^48).

### 2.3 `caractere()` = UTF-8, `longueur()` = points de code

Après 2.2 : PE de `3 965` o, `file` = « MS-DOS executable, MZ for MS-DOS ».
Hexdump : `e_lfanew` à `0x3c` vaut `c2 80 00 00` au lieu de `80 00 00 00` :
`caractere(128)` est encodé en UTF-8 (`c2 80`) par le runtime G5 natif, alors
que `ecrire_bytes` Rust écrit le code point bas (`0x80`).

Premier essai : utiliser `octet_brut()` (interception G5 `$g5_octetll`, octet
brut) dans `u8le/u16le/u32le/u64le`. Résultat : `0x80` correct, mais PE de
`3 821` o avec **235 octets de bourrage en trop** (diff `difflib` : +1 à
`0x49`, +3 à `0x16e`, +230 à `0xa24`, +1 à `0xb1e`). Cause mesurée :
`longueur(octet_brut(128))` vaut **0** en natif contre **1** sous Rust (le
`g5_emit_len` natif compte les têtes de séquence UTF-8 ; `0x80..0xBF` sont des
octets de continuation). Chaque octet brut ≥ 0x80 dans le texte fait donc
allonger le `pe_z()` de `pe_c($s, cible)`.

**Correctif retenu** : construire tout le PE avec `caractere()` (UTF-8 valide,
`longueur()` = nombre d'octets), puis convertir **au dernier moment**, juste
avant `ecrire_bytes`, avec `pe_raw()` : chaque point de code est relu par
`ordre(sous_chaine(...))` et réécrit par `octet_brut()`. `pe_join()` (jointure
équilibrée d'une liste de morceaux) évite le O(n²) de `+` sur chaînes dans le
runtime G5. Résultat natif : **octet pour octet identique** à la référence
interprétée.

## 3. Livrable

`compilateur/emetteur_pe_sel.rose` — **1 653 lignes, 98 `fn`** (les 94 de
`emetteur_pe.rose` + `octet_brut`, `g5_elt_ch`, `pe_join`, `pe_raw`). Fichier
possédé ; `emetteur_pe.rose`, `emetteur_elf.rose`, `rose_natif*.rose` et
`rose/src/**` sont intacts.

```
bash scripts/garde_fou_mem.sh 12000000 timeout 1800 \
  /tmp/opencode/C0_opt.elf compilateur/emetteur_pe_sel.rose > /tmp/opencode/j2_nat/pe_sel_final.elf
chmod +x /tmp/opencode/j2_nat/pe_sel_final.elf
/tmp/opencode/j2_nat/pe_sel_final.elf programme.rir > programme.exe
```

- émetteur natif : `ELF 64-bit LSB executable, ARM aarch64, statically linked`,
  **199 004 o**, md5 `2ed788dfc4e7ae9fd6e655f19f830528` ;
- `./rose/rose verif compilateur/emetteur_pe_sel.rose` → `vérifié` ;
- compilation `rc=0`, stderr vide (garde-fou 12 000 000 Ko) ;
- recompilé avec la graine **étendue** `C0_ext.elf` (md5
  `7bb091200cf927022eb9d86a548d2502`) : ELF de la même taille `199 004` o et
  PE `01_arith` identique (`e7b9fa82…`) — l'émetteur ne dépend pas du choix
  de graine C0 avant/après extension.

## 4. Validation du PE produit (étape 3)

Corpus : les 25 RIR générés par `./rose/rose rir` depuis
`rose/tests_aot8/programmes/*.rose`, émis **par l'ELF natif** puis comparés
aux PE émis par `./rose/rose compilateur/emetteur_pe.rose` (graine Rust) :

| Contrôle | Résultat mesuré |
|---|---|
| md5 natif = md5 interprété | **25/25** (1 s pour les 25 émissions natives) |
| `file` (`01_arith`) | `PE32+ executable (console) Aarch64, for MS Windows, 3 sections` |
| signature | `4d 5a` à `0x00` (MZ), `50 45 00 00` à `0x80` (PE), machine `64 aa` (0xAA64) |
| imports | `llvm-readobj --coff-imports` : `KERNEL32.DLL`, `GetStdHandle`, `WriteFile`, `ExitProcess`, `ReadFile`, `CreateFileA`, `CloseHandle` |
| sections | `objdump -h` : `.text` @RVA `0x1000`, `.data` @RVA `0x2000`, `.pdata` @RVA `0x403000` |
| code machine | `objdump -d` désassemble `.text` en AArch64 : prologue `sub sp,sp,#0x500`, `adrp x9,0x140002000` + `add x9,x9,#0x200` (tas `.data+0x200`), `mov x0,#0xe`, `bl` vers le stub Win64, `ret` |
| md5 PE `01_arith` | `e7b9fa82468d0e97fd9dcb99b33e47c5` (3 584 o), identique aux deux voies |

Deux programmes du corpus (`05_bigint_factorielle`, `21_primitives_const`)
produisent une sortie vide et `rc=2` **dans les deux voies** (native et
interprétée) : ce sont les lacunes déjà documentées du sous-ensemble G5
(opérateur `!` / primitives constantes), pas une divergence J2.

Le mode `--elf` de l'émetteur (writer Linux conservé) donne lui aussi un ELF
de `1 200` o, md5 `aa469057d7703445014541777e1b9946` identique aux deux voies.
Auto-émission : le RIR du propre émetteur (`2 165` lignes) donne un PE de
`77 824` o, md5 `34c9d44522c0a110d1870f6ae217b1e9` dans les deux voies ;
mesure unique (non A/B) : `7,62 s` / `1,72 Gio` en natif contre `40,71 s` /
`48 Mio` interprété.

Comparaison avec le PE de référence de `rose/tests_aot8/verifier.sh`
(`./rose/rose aot 01_arith.rose -o ref_aot.exe --target x86_64-pc-windows-msvc`) :

| | PE natif J2 | PE AOT8 de référence |
|---|---|---|
| architecture | AArch64 (0xAA64) | x86-64 |
| sections | 3 (`.text`, `.data`, `.pdata`) | 2 |
| imports KERNEL32.DLL | 6 (dont `CreateFileA`, `CloseHandle`) | 6 (dont `GetProcessHeap`, `HeapAlloc`) |
| taille `01_arith` | 3 584 o | 2 048 o |

Les deux sont des `PE32+ executable (console) … for MS Windows` avec table
d'imports `KERNEL32.DLL` valide ; l'émetteur J2 vise AArch64 et embarque ses
propres stubs Win64 (pas de `HeapAlloc`, donc pas de test `verifier.sh`
`Symbol: HeapAlloc`, qui porte sur le backend Rust x86-64). Aucune exécution
Windows n'est possible ici (Wine absent) : la validation est structurelle
(`file`, magic, `llvm-readobj`, `objdump`) et l'égalité octet pour octet avec
la voie interprétée déjà validée.

## 5. Non-régressions (étape 4)

- `./rose/rose verif compilateur/emetteur_pe.rose` → `vérifié` ;
- point fixe émetteur : `./rose/rose rir compilateur/emetteur.rose | md5sum`
  → `b3ed9262dd54776a6dbd07bc12368d0e` ;
- `RUST_TEST_THREADS=1 bash scripts/garde_fou_mem.sh 12000000 timeout 3600 bash scripts/validation_finale.sh`
  → `rc=0`, journal `/tmp/opencode/j2_nat/validation_finale.out`,
  `=== RESULTAT: 0 échec(s) ===` (314 + 327 tests Rust réussis, point fixe,
  **AOT 75/75**, JIT 25/25, WASM 92 o, RVM `317811`, AZ 31 CONFORME/0,
  HOST-1 `fib=317811`) ;
- `git status --short` dans le worktree : uniquement
  `?? compilateur/emetteur_pe_sel.rose` (et ce document).
