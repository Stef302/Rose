# PE Win32 exécutable — backend KERNEL32 dans `emetteur_pe.rose`

**Constat daté : 19/11/2026.** Agent PE2 (vague 37). Aucun commit, aucune
publication, aucun fichier du dépôt modifié hors
`compilateur/emetteur_pe.rose` et ce document. `emetteur_elf.rose` est possédé
par un autre agent et n'a pas été touché.

## Objectif et résultat

Rendre le PE32+ Aarch64 produit par Rose **réellement exécutable sous Windows**
au lieu d'un conteneur structurellement valide mais empli de syscalls Linux.

**Réalisé** : les deux `svc #0` du G5 (`write`=64, `exit`=93) sont remplacés par
des appels à six fonctions KERNEL32.DLL via la table d'imports. Le tas G5 n'est
plus à l'adresse absolue Linux `0x60000000` mais dans une zone RW de l'image,
adressée en PC-relatif. Le mode `--elf` du même fichier conserve le backend
Linux d'origine : `emetteur_pe.rose` est autonome.

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

### 1. `rose verif` et trois PE

```
rose verif compilateur/emetteur_pe.rose  -> vérifié
rose compilateur/emetteur_pe.rose <prog>.rir > <prog>.exe
```

| programme | taille .exe | VirtualSize .text | RVA .data | `file` |
|---|---|---|---|---|
| `>> 47` | 3 072 o | 0x55C | 0x2000 | PE32+ console Aarch64, 2 sections |
| `fib(10)` | 3 584 o | 0x608 | 0x2000 | idem |
| `somme(10)` | 3 072 o | 0x5F4 | 0x2000 | idem |
| `self.exe` (émetteur se compilant) | 75 264 o | 0x11E10 | 0x13000 | idem |

`llvm-readobj --coff-imports` : **rc=0 et 6 symboles** pour les quatre.
`llvm-readobj --file-headers` : Machine `0xAA64`, Magic `0x20B`, entry `0x1000`,
ImageBase `0x140000000`.

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

### 3. Exécution RÉELLE des instructions émises (sous qemu, avec shims)

Wine est absent et `sudo` demande un mot de passe : **aucune exécution Windows
n'a eu lieu ici**. En revanche, le `.text` du PE a été extrait et exécuté sous
`qemu-aarch64-static` dans un ELF harnais qui branche l'IAT sur des shims
`GetStdHandle`/`WriteFile`/`ExitProcess` (écrivant via le syscall Linux
`write`). Ce n'est **pas** Windows : c'est une exécution réelle du code machine
émis, avec des fonctions hôtes factices.

```
t47      -> 47        (rc=0)
fib(10)  -> 55        (rc=0)
somme(10)-> 55        (rc=0)
>> -47 / 0 / 1234567890123 -> -4701234567890123 (rc=0, concaténés : le print
                              G5 n'ajoute pas de retour à la ligne, comportement
                              préexistant)
```

Le harnais est `/tmp/opencode/pe2_agents/mk_harness.py` (hors dépôt).

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

## Test à faire sur Windows ARM64 (à faire par l'utilisateur)

Copier `self.exe` (ou un PE d'exemple) sur une machine **Windows ARM64**, puis
dans `cmd.exe` :

```
self.exe
```

Pour un programme `fib.exe` contenant `>> fib(10)`, la console doit afficher
`55`. Vérification complémentaire : `dumpbin /imports fib.exe` doit lister les
six symboles KERNEL32 ci-dessus, et `dumpbin /disasm` ne doit montrer aucun
`svc`.

## Limites — honnêteté absolue

1. **Aucun PE n'a été exécuté sous Windows ici.** Wine absent, sudo verrouillé.
   Seule l'exécution sous qemu + shims a été faite (section 3).
2. `self.exe` est une **preuve structurelle d'auto-compilation** : l'émetteur
   sait compiler son propre RIR en PE valide. Fonctionnellement, ses primitives
   d'entrée (`lire_texte`) restent les stubs neutres du G5 : lancé sur Windows,
   il ne relira pas son fichier d'entrée. La self-hosting *fonctionnelle*
   (E/S réelles dans l'émetteur auto-compilé) n'est pas réalisée ici.
3. `>> "ABC"` affiche le pointeur du descripteur, pas les octets : le stub
   `print_i64` du PE G5 ne sait pas afficher les chaînes (limite préexistante
   du G5 de `emetteur_pe.rose` ; l'autre agent a ajouté l'impression de chaînes
   dans `emetteur_elf.rose`, non fusionné ici, conformément à la consigne).
4. L'émetteur n'ajoute pas de section de relocations `.reloc`. Les accès tas/IAT sont
   PC-relatifs et `DllCharacteristics=0` (pas de `DYNAMIC_BASE`), donc l'image
   charge à `0x140000000` ; si Windows la déplace malgré tout, les sauts
   internes restent valides mais d'éventuelles adresses absolues résiduelles
   (hors tas/IAT) ne seraient pas réparées.
5. Les fichiers de validation et les binaires générés vivent dans
   `/tmp/opencode/pe2_agents/`, pas dans le dépôt.
