# PE32+ x86-64 en Rose pur — `compilateur/emetteur_x64.rose`

**Constat daté : 19/11/2026.** Dépôt `Stefan/ROSE`, HEAD `e0ec287`, Rose 0.1.6.
Complément de `compilateur/emetteur_pe.rose` (PE32+ **AArch64**). Cible :
**x86-64** (`x86_64-pc-windows-msvc` / `x86_64-pc-windows-gnu`). Jamais i686.

## 1. Ce qui a été construit

Un seul fichier Rose pur, `compilateur/emetteur_x64.rose` (~830 lignes) :

1. **Analyseur RIR** (`x_parse`, `x_tokens`, `x_noms`, `x_slot_of`) — lit la
   sortie de `rose rir`.
2. **Évaluateur d'expressions** (`x_eval`) — entiers, variables/slots, `bin`
   (`+ - * / %`, `== != < <= > >=`, `et`/`ou`), `app` (appels utilisateurs,
   récursifs), `neg`/`uni`/`non`, littéraux booléens.
3. **Opérations RIR** (`x_op`) — `affect slot`, `expr`, `saut`,
   `saut_si_faux`, `retour`, `afficher`.
4. **Backend machine x86-64** — encodeur de bytes et helpers REX/ModRM ;
   conventions : **RAX** = valeur d'expression, **R10** = second opérande,
   **RCX/RDX/R8/R9** = arguments (ABI Win64), **RBP** = pointeur de cadre,
   scratch indexé par profondeur d'expression (aucun `push` d'expression).
   Prologue `push rbp ; mov rbp,rsp ; sub rsp,688` (frame 16-alignée),
   épilogue `mov rsp,rbp ; pop rbp ; ret`.
5. **Stub `print_i64`** — conversion décimale dans un tampon pile,
   `GetStdHandle(-11)` puis `WriteFile(h, buf, len, &written, NULL)` ; le
   5ᵉ argument est déposé à `[rsp+32]`, comme l'exige l'ABI Win64.
6. **Writer PE32+ complet** — DOS, COFF `Machine=0x8664`, OptionalHeader
   `Magic=0x20B`, `DllCharacteristics=0x8160`, sections `.text`, `.data`
   (descripteur d'import + ILT + IAT + 3 `IMAGE_IMPORT_BY_NAME` + nom de DLL),
   `.pdata` (RUNTIME_FUNCTION + UNWIND_INFO), appels d'import via
   `call qword ptr [rip+disp32]` patchés après calcul des RVA.

## 2. Chaîne de compilation

```bat
rose rir prog.rose > prog.rir
rose compilateur/emetteur_x64.rose prog.rir > prog.exe
prog.exe
```

## 3. Validation réelle (structurelle — aucune exécution x64 ici)

Binaire de test construit pour l'occasion :
`/tmp/opencode/x64_agents/rose` (`cargo build --release --bin rose`,
`CARGO_TARGET_DIR=/tmp/opencode/x64_agents/target`, rc=0, 33,12 s).
**`rose/rose` n'a PAS été remplacé.**

### 3.1 Vérification Rose

```
$ rose verif compilateur/emetteur_x64.rose
vérifié
```

### 3.2 Trois PE x86-64 générés

Programmes sources : `>> 47`, `fib(10)` (récursion), somme `pour i de 0 vers 10`.

| fichier | `file` |
|---|---|
| `rir_test1.x64.exe` | PE32+ executable (console) x86-64, for MS Windows, 3 sections |
| `rir_test2.x64.exe` | PE32+ executable (console) x86-64, for MS Windows, 3 sections |
| `rir_test3.x64.exe` | PE32+ executable (console) x86-64, for MS Windows, 3 sections |

`llvm-readobj --file-headers` sur les trois :

```
AddressSize: 64bit
Machine: IMAGE_FILE_MACHINE_AMD64 (0x8664)
AddressOfEntryPoint: 0x1000
ImageBase: 0x140000000
ExceptionTableRVA: 0x3000
ExceptionTableSize: 0xC
```

`llvm-readobj --coff-imports` sur les trois :

```
Name: KERNEL32.DLL
Symbol: GetStdHandle (0)
Symbol: WriteFile (0)
Symbol: ExitProcess (0)
```

`llvm-readobj --unwind` reconnaît la table d'exceptions :

```
RuntimeFunction {
  StartAddress:  (0x140001000)
  EndAddress:    (0x1400011FE)
  UnwindInfoAddress: (0x140003018)
  UnwindInfo {
    Version: 1, PrologSize: 11, UnwindCodeCount: 4
    UnwindCodes [
      0x01: PUSH_NONVOL reg=RBP
      0x0B: ALLOC_LARGE size=688
    ]
  }
}
```

### 3.3 Désassemblage

⚠ Le `objdump` GNU de CETTE machine est une build **aarch64-only**
(`objdump --info` ne contient aucun `pei-x86-64`) : `objdump -d -m i386:x86-64`
échoue avec « file format not recognized ». Le désassemblage a donc été fait
avec `llvm-objdump --triple=x86_64-pc-windows-msvc` (même architecture cible).
Sur une machine GNU objdump x86, la commande équivalente est :

```
objdump -d -m i386:x86-64 -b pe-x86-64 prog.exe
```

Extrait réel (programme `>> 47`) :

```
140001000: 55                    pushq  %rbp
140001001: 48 89 e5              movq   %rsp, %rbp
140001004: 48 81 ec b0 02 00 00  subq   $0x2b0, %rsp
14000100b: 48 b8 2f 00 ...       movabsq $0x2f, %rax      ; 47
140001015: e8 09 00 00 00        callq  <print_i64>
14000101a: 31 c9                 xorl   %ecx, %ecx
14000101c: ff 15 6e 10 00 00     callq  *0x140002090      ; ExitProcess IAT
140001022: c3                    retq
...
1400010a0: 49 f7 f0              divq   %r8               ; conversion décimale
1400010aa: 41 88 12              movb   %dl, (%r10)
1400010d5: ff 15 a5 0f 00 00     callq  *0x140002080      ; GetStdHandle IAT
1400010ff: ff 15 83 0f 00 00     callq  *0x140002088      ; WriteFile IAT
```

Le programme `fib(10)` montre bien les appels récursifs
(`callq <fib+…>`), `setl`, `subq`, `addq`, et le retour via `retq`.
La boucle somme montre `jmp` en arrière vers le test et `je` vers
l'`afficher`.

### 3.4 Déterminisme

Deux générations indépendantes, md5 identiques :

```
rir_test1  66cee8060a9fc6cb76fd0d4f8d4282f4  ×2
rir_test2  48b20203cbbceb5836c76db6754cd976  ×2
rir_test3  3f3b4cb712349a0246dd1635193e13ac  ×2
```

### 3.5 Non-régression (binaire construit ici)

- `cargo test --release` → **220 + 204 + 0 + 6 + 1 + 0 = 431 passed, 0 failed**,
  6 ignorés, rc=0 (`/tmp/opencode/x64_agents/cargo_test.log`).
- `jardins/tests/premiers.rose` → `── jardin/premiers : 20 tests, 0 échec ──`.
- Point fixe M4.5, **mesuré avec le binaire construit dans
  `/tmp/opencode/x64_agents/`** :

  ```
  b3ed9262dd54776a6dbd07bc12368d0e  fpA.rir   (rose rir compilateur/emetteur.rose)
  b3ed9262dd54776a6dbd07bc12368d0e  fpB.rir
  cmp_rc = 0
  ```

  **Écart honnête** : la valeur `8ac53eed3d4967a0d7d9ab333256801c` citée
  dans `PE_WIN32.md` est un **constat daté** d'un binaire d'un autre agent
  (`pe2_agents`) dans un état antérieur du dépôt. Le binaire construit
  aujourd'hui à HEAD `e0ec287` donne `b3ed9262…`, de façon stable ×2. Ce
  chiffre n'est pas une régression : c'est une mesure locale re-faite, pas
  une recopie de note.
- Pour info, le RIR du nouvel émetteur est lui aussi stable :
  `f669e807ca02522b47cd48fed12f09ee` ×2.

## 4. Limites exactes — ne pas prétendre davantage

1. **Aucun PE x86-64 n'a été exécuté ici.** Wine 11.12 présent sur cette
   machine est compilé en **aarch64** : il ne peut pas lancer un PE AMD64
   (aucun WoW64). La validation est **structurelle** (`file`, `llvm-readobj`,
   désassemblage), pas fonctionnelle. Les sorties attendues sur Windows x64
   sont 47 / 55 / 45 (valeurs confirmées côté moteur Rose, pas par le PE).
2. **Sous-ensemble couvert** : entiers 64 bits signés, slots, binops arithmétiques
   et comparaisons, appels/récursion, sauts, affichage d'entier.
   **Non couvert** : chaînes, listes, tables, flottants, rationnels, nombres
   complexes, primorielle/sous-factorielle, ffi, E/S fichier, exceptions
   utilizateur, plus de 4 arguments de fonction, `retour` dans `$principal`
   (la fin du programme passe par `ExitProcess`).
3. Le débordement arithmétique est celui de l'entier 64 bits (pas de BigInt) ;
   un programme dépassant `2^63` ne produit pas la même valeur que Rose.
4. `llvm-readobj --unwind` affiche, après la vraie `RuntimeFunction`, deux
   entrées parasites dues à son analyseur COFF qui parcourt le reste de la
   section `.pdata` au-delà de `ExceptionTableSize=0xC`. La table réelle est
   correcte (une entrée de 12 octets + un terminateur nul + UNWIND_INFO à
   l'offset 0x18) ; `ExceptionTableRVA=0x3000` est bien non nul.
5. Le writer n'émet **aucune relocation** : l'image doit être chargée à son
   `ImageBase` préféré (`0x140000000`) ou rebasé par le chargeur via
   `DYNAMIC_BASE` (0x8160). Les appels d'import et les sauts sont
   PC-relatifs, donc insensibles au décalage.

## 5. Test sur Windows x64 (à faire par l'utilisateur)

Copier `compilateur/emetteur_x64.rose` et les trois `.rose` dans un dossier,
puis dans `cmd.exe` (ou PowerShell) :

```bat
rose rir t47.rose        > t47.rir
rose compilateur/emetteur_x64.rose t47.rir  > t47.exe
t47.exe
REM attendu : 47

rose rir fib.rose        > fib.rir
rose compilateur/emetteur_x64.rose fib.rir  > fib.exe
fib.exe
REM attendu : 55

rose rir somme.rose      > somme.rir
rose compilateur/emetteur_x64.rose somme.rir  > somme.exe
somme.exe
REM attendu : 45
```

Vérification complémentaire sous Windows (si `dumpbin` disponible) :

```bat
dumpbin /headers t47.exe
REM Machine : 0x8664 (x64), Exception Table RVA 0x3000
dumpbin /imports t47.exe
REM KERNEL32.DLL : GetStdHandle, WriteFile, ExitProcess
```

## 6. Fichiers possédés

- `compilateur/emetteur_x64.rose` — créé, vérifié, non commité.
- `rose/docs/PE_X64.md` — ce document.

Aucun autre fichier du dépôt n'a été modifié par ce chantier
(`emetteur_pe.rose`, `emetteur_elf.rose`, `run.rs`, `PLAN.md` intacts).
