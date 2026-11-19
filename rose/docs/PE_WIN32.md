# PE Win32 exécutable — backend KERNEL32 dans `emetteur_pe.rose`

**Constat daté : 19/11/2026, mis à jour en fin de soirée après diagnostic Wine.**
Agent PE2 (vague 37). Aucun commit, aucune publication, aucun fichier du dépôt
modifié hors `compilateur/emetteur_pe.rose` et ce document. `emetteur_elf.rose`
est possédé par un autre agent et n'a pas été touché.

⚠ **Mise à jour décisive** : l'utilisateur a installé Wine et l'orchestrateur a
compilé **Wine 11.12 depuis les sources** avec clang
(`/tmp/opencode/winebuild/build64/wine`). Le PE Rose a d'abord été refusé
(`Bad EXE format`, erreur 193) : cause = absence de table d'exceptions ARM64.
Après ajout de `.pdata`, **les PE Rose s'exécutent réellement sous Wine 11.12 :
`47`, `55`, `55`**. Voir section « Diagnostic Wine ».

## Objectif et résultat

Rendre le PE32+ Aarch64 produit par Rose **réellement exécutable sous Windows**
au lieu d'un conteneur structurellement valide mais empli de syscalls Linux.

**Réalisé** : les deux `svc #0` du G5 (`write`=64, `exit`=93) sont remplacés par
des appels à six fonctions KERNEL32.DLL via la table d'imports. Le tas G5 n'est
plus à l'adresse absolue Linux `0x60000000` mais dans une zone RW de l'image,
adressée en PC-relatif. Le mode `--elf` du même fichier conserve le backend
Linux d'origine : `emetteur_pe.rose` est autonome.

## Diagnostic Wine 11.12 — le vrai blocage et le correctif

### Symptôme reproduit

```
$ wine /tmp/opencode/launcher.exe C:/t47.exe
launching C:/t47.exe
error Bad EXE format for %1. (os error 193)
```

Le lanceur Rust de référence (PE ARM64 MSVC) tourne, lui, sous le même Wine :
le chargeur rejette donc bien **notre image**, pas Wine.

### Différence d'en-tête décisive

| Champ | PE de référence (Rust ARM64 MSVC) | PE Rose avant correctif |
|---|---|---|
| `ExceptionTableRVA` | `0x37000` | **`0x0`** |
| `ExceptionTableSize` | `0x1340` | **`0x0`** |
| section `.pdata` | présente (VirtualSize `0xA60`) | **absente** |
| `DllCharacteristics` | `0x8160` | `0x0` |

Sur ARM64, la table d'exceptions n'est pas un luxe : le chargeur exige au
moins un `RUNTIME_FUNCTION` valide pour accepter l'image.

### Correctif apporté à `g5_pe()`

1. **Troisième section `.pdata`** (RVA = `.data` + sa taille virtuelle, alignée
   page ; 512 octets bruts, `INIT|READ`), avec :
   - `RUNTIME_FUNCTION` (12 octets) : `BeginAddress = 0x1000`,
     `EndAddress = 0x1000 + text_len`, `UnwindData = RVA(.pdata)+16` ;
   - `UNWIND_INFO` minimal (4 octets `01 00 00 00`) : version 1, flags 0,
     prolog 0, CountOfCodes 0, frame 0.
2. **DataDirectory[3]** (EXCEPTION) renseigné : `ExceptionTableRVA = RVA(.pdata)`,
   `ExceptionTableSize = 12`.
3. **`DllCharacteristics = 0x8160`** (DYNAMIC_BASE | HIGH_ENTROPY_VA |
   NX_COMPAT | TERMINAL_SERVER_AWARE), comme la référence.
4. NumberOfSections 2 → 3 ; SizeOfImage recalculé.

Contrôle `llvm-readobj --file-headers t47.exe` après correctif :

```
SectionCount: 3
IMAGE_DLL_CHARACTERISTICS_DYNAMIC_BASE (0x40)
IMAGE_DLL_CHARACTERISTICS_HIGH_ENTROPY_VA (0x20)
IMAGE_DLL_CHARACTERISTICS_NX_COMPAT (0x100)
IMAGE_DLL_CHARACTERISTICS_TERMINAL_SERVER_AWARE (0x8000)
ExceptionTableRVA: 0x403000
ExceptionTableSize: 0xC
```

Contenu réel de `.pdata` (t47) : `RUNTIME_FUNCTION = (0x1000, 0x155C, 0x403010)`,
`UNWIND_INFO = 01 00 00 00`.

## Backend Win32

### Table d'imports (précisément six symboles)

`llvm-readobj --coff-imports` résout, sur **chaque** PE généré :

```
Name: KERNEL32.DLL
ImportLookupTableRVA: 0x<data_rva>+0x60
ImportAddressTableRVA: 0x<data_rva>+0x98
  GetStdHandle, WriteFile, ExitProcess, ReadFile, CreateFileA, CloseHandle
```

Les deux tables (ILT et IAT) se terminent par une entrée nulle — sans elle,
`llvm-readobj` échoue avec `RVA … for import symbol name not found` et le
chargeur Windows lirait la table jusqu' dans les noms de fonctions.

### Trampolines d'import

Chaque entrée de l'IAT est appelée par un trampoline de trois instructions :

```
adrp x17, page(IAT[i])      ; PC-relatif
ldr  x16, [x17, #off]       ; entrée résolue par le loader
br   x16                    ; APPEL TAIL : x30 intact
```

Appelé par `bl <trampoline>`, `br x16` saute dans KERNEL32, qui revient
directement à l'appelant : **aucun sauvegarde de LR n'est nécessaire** au point
d'appel. `x16`/`x17` sont les registres scratch AArch64 de l'ABI Windows ;
`x18` (TLS) n'est jamais utilisé par le G5.

### Stubs Win64 émis

| Stub | Appels | ABI |
|---|---|---|
| `print_i64` | `GetStdHandle(-11)` + `WriteFile` | x0 = entier signé 64 bits ; cadre 96 o (home space 32 o, LR en `[sp,#40]`, chiffres en `[sp,#64..95]`) |
| `ecrire_octets` | `GetStdHandle(-11)` + `WriteFile` | x0 = descripteur G5 `[tag][len][octets…]` ; retour = octets écrits |
| `lire_fichier` | `CreateFileA` + `ReadFile` + `CloseHandle` | x0 = nom C, x1 = tampon, x2 = taille max ; retour = octets lus |
| `exit` | `ExitProcess` | x0 = 0 |

Les appels Win64 réservent 32 octets de home space et gardent `sp` aligné sur
16. `x19`/`x20` (callee-saved) portent le handle et la longueur.

### Tas

`g5_setheap()` émet `adrp xRd, page(.data) ; add xRd, xRd, #0x200` : le tas est
à `image_base + data_rva + 0x200`, avec 4 Mio réservés (VirtualSize de `.data`,
remplis de zéros par le chargeur ; 512 octets seulement sur disque). Le RVA de
`.data` est recalculé après émission du code, et les immédiats ADRP sont
patchés dans `g5_patch()` (métadonnées `A|<rd>|<offset>`). Conséquence : aucun
chevauchement de sections même pour `self.exe` (`.text` = 0x11E10 octets,
`.data` déplacée à RVA 0x13000), et l'image reste correcte si le chargeur la
déplace (pas d'adresse absolue pour le tas ni pour l'IAT).

⚠ Piège Rose rencontré et contourné : `longueur($g5_code)` lue au niveau
supérieur du dispatch renvoie une valeur périmée (10) alors que la même lecture
**dans une fonction** renvoie la bonne valeur (386 / 18308). Le calcul final du
RVA passe donc par `fn g5_final_rva()`.

⚠ Deuxième piège réel trouvé par exécution : `strboff/ldroff` encodent un
**offset depuis sp**, pas une adresse. Écrire `addi(x7, sp, 64)` puis
`strboff(x, 7)` adresse `2*sp+64` → SIGSEGV. Les offsets de chiffres sont donc
des immediats (`movz(x7, 64)`), pas des pointeurs.

## Preuves

### 1. `rose verif` et six PE

```
rose verif compilateur/emetteur_pe.rose  -> vérifié
rose compilateur/emetteur_pe.rose <prog>.rir > <prog>.exe
```

| programme | taille .exe | VirtualSize .text | RVA .data | RVA .pdata | `file` |
|---|---|---|---|---|---|
| `>> 47` | 3 584 o | 0x55C | 0x2000 | 0x403000 | PE32+ console Aarch64, 3 sections |
| `fib(10)` | 4 096 o | 0x608 | 0x2000 | 0x403000 | idem |
| `somme(10)` | 3 584 o | 0x5F4 | 0x2000 | 0x403000 | idem |
| `>> -47/0/1234567890123` | 3 584 o | 0x580 | 0x2000 | 0x403000 | idem |
| `>> "ABC"` | 3 584 o | 0x5A0 | 0x2000 | 0x403000 | idem |
| `self.exe` (émetteur se compilant) | 75 776 o | 0x11E10 | 0x13000 | 0x414000 | idem |

`llvm-readobj --coff-imports` : **rc=0 et 6 symboles** pour les six.
`llvm-readobj --file-headers` : Machine `0xAA64`, Magic `0x20B`, entry `0x1000`,
ImageBase `0x140000000`, `ExceptionTableRVA != 0` sur les six.

### 2. Plus aucun syscall Linux

Balayage octet par octet de `0xD4000001` (`svc #0`) :

| PE | `svc #0` | `br x16` (trampolines) |
|---|---|---|
| t47 | **0** | 6 |
| fib | **0** | 6 |
| somme | **0** | 6 |
| self | **0** | 6 |

Les 6 `br x16` correspondent exactement aux 6 trampolines.

`objdump` sur `fib.exe` (adresses virtuelles PE) :

```
140001008: adrp x9, 0x140002000 ; add x9,x9,#0x200 ; tas à 0x140002200
140001030: bl  0x140001490     ; print_i64
140001034: bl  0x1400015b4     ; exit
...
140001578: mov  x0, #0xfff5    ; STD_OUTPUT_HANDLE = -11
140001588: bl  0x1400015c0     ; trampoline GetStdHandle
1400015a4: bl  0x1400015cc     ; trampoline WriteFile
1400015c0: adrp x17, 0x140002000 ; ldr x16,[x17,#152] ; br x16
1400015d8: adrp x17, 0x140002000 ; ldr x16,[x17,#168] ; br x16  <- ExitProcess
```

### 3. Exécution RÉELLE sous Wine 11.12 (chargeur PE complet)

Commande rejouée (Wine compilé depuis les sources par l'orchestrateur) :

```
cd /tmp/opencode/winebuild/build64
WINEARCH=win64 WINEPREFIX=/tmp/opencode/wine_new WINEDEBUG=-all \
  ./wine /tmp/opencode/launcher.exe 'C:/t47.exe'
```

| PE | sortie Wine | statut |
|---|---|---|
| `t47.exe` (`>> 47`) | `47` | `status exit code: 0` |
| `fib.exe` (`fib(10)`) | `55` | `status exit code: 0` |
| `somme.exe` (`somme(10)`) | `55` | `status exit code: 0` |
| `neg.exe` | `-4701234567890123` | `status exit code: 0` |
| `chaine.exe` | `5368717840` (pointeur du descripteur) | `status exit code: 0` |
| `self.exe` | *(rien — E/S G5 encore stubées)* | `status exit code: 0` |

Le `status exit code: 0` prouve que `ExitProcess(0)` a été réellement appelé via
l'IAT et que l'image a été chargée sans erreur. Ce n'est **pas** Windows natif :
c'est Wine 11.12, chargeur PE complet avec KERNEL32 réel.

### 3b. Complément : exécution sous qemu + shims

Le `.text` du PE a aussi été extrait et exécuté sous `qemu-aarch64-static` dans
un ELF harnais qui branche l'IAT sur des shims `GetStdHandle`/`WriteFile`/
`ExitProcess` (écrivant via le syscall Linux `write`) : `47`, `55`, `55`, rc=0.
Harnais : `/tmp/opencode/pe2_agents/mk_harness.py` (hors dépôt). Cette voie
restait disponible avant Wine ; elle n'est plus la seule preuve d'exécution.

### 4. Différence `.text` PE vs `.text` ELF (backend Linux du même fichier)

Comparaison des mots 32 bits, après normalisation (a) des charges de tas —
`adrp+add` côté PE, `movz+movk…` côté ELF — et (b) des immédiats de
branchement :

| programme | mots PE / ELF | groupes tas normalisés | préfixe opcode identique |
|---|---|---|---|
| t47 | 343 / 256 | 7 / 7 | 192 / 249 |
| fib | 386 / 299 | 7 / 7 | 235 / 292 |
| somme | 381 / 294 | 7 / 7 | 230 / 287 |

La première divergence survient **au début du stub d'affichage** : le PE
alloue un cadre Win64 de 96 octets et appelle l'IAT, l'ELF garde le `svc #0`
d'origine. Les corps de fonctions G5 sont donc opcode-identiques ; seuls
l'adressage du tas, les déplacements de `bl`, le stub d'affichage/sortie et les
six trampolines diffèrent. L'ELF produit par `--elf` contient bien 2 `svc #0`
(write + exit) : le backend Linux est préservé.

### 5. Non-régression

- point fixe `8ac53eed3d4967a0d7d9ab333256801c` ×2, `cmp_rc=0` (binaire
  reconstruit dans `/tmp/opencode/pe2_agents/rose`) ;
- `cargo test --release` : **220 + 204 + 0 + 6 + 1 + 0 = 431 passés, 0 échec** ;
- jardin `jardins/tests/premiers.rose` : **20 tests, 0 échec**.

Ces trois contrôlent le moteur/émetteur Rust et `compilateur/emetteur.rose` :
aucun d'eux n'a été modifié par ce correctif (seul `g5_pe()` de
`emetteur_pe.rose` a changé depuis la première version).

## Test sur Windows ARM64 natif (recommandé, non fait ici)

Le chargement PE complet a été validé sous **Wine 11.12**, pas sous Windows
natif. Pour confirmer sur une machine **Windows ARM64**, copier `fib.exe` puis,
dans `cmd.exe` :

```
fib.exe
```

La console doit afficher `55`. `dumpbin /imports fib.exe` doit lister les six
symboles KERNEL32 ; `dumpbin /disasm` ne doit montrer aucun `svc` ;
`dumpbin /headers fib.exe` doit montrer `ExceptionTableRVA != 0`.

## Limites — honnêteté absolue

1. **Aucun PE n'a été exécuté sous Windows natif ici.** L'exécution réelle a été
   faite sous **Wine 11.12** (chargeur PE + KERNEL32), pas sous Windows. Wine
   n'a pas été modifié : c'est le binaire compilé par l'orchestrateur.
2. `self.exe` est une **preuve d'auto-compilation structurelle** : l'émetteur
   sait compiler son propre RIR en PE valide, et ce PE **se charge sous Wine**
   (`status exit code: 0`). Fonctionnellement, ses primitives d'entrée
   (`lire_texte`) restent les stubs neutres du G5 : il ne relira pas son fichier
   d'entrée. La self-hosting *fonctionnelle* (E/S réelles dans l'émetteur
   auto-compilé) n'est pas réalisée ici.
3. `>> "ABC"` affiche le pointeur du descripteur, pas les octets : le stub
   `print_i64` du PE G5 ne sait pas afficher les chaînes (limite préexistante
   du G5 de `emetteur_pe.rose` ; l'autre agent a ajouté l'impression de chaînes
   dans `emetteur_elf.rose`, non fusionné ici, conformément à la consigne).
4. `DllCharacteristics=0x8160` active ASLR, mais l'émetteur ne produit **aucune
   section `.reloc`**. Les accès tas/IAT et les sauts sont PC-relatifs/RVA, donc
   le chargement sous Wine a réussi ; si un chargeur déplaçait l'image,
   d'éventuelles adresses absolues résiduelles ne seraient pas réparées.
5. Le `UNWIND_INFO` est volontairement minimal (0 code de déroulement) : il
   suffit à faire accepter l'image par le chargeur, mais il ne décrit pas les
   prologues réels — le débogage/déroulement d'exceptions natives serait
   incomplet.
6. Les fichiers de validation et les binaires générés vivent dans
   `/tmp/opencode/pe2_agents/`, pas dans le dépôt.
