# Preuve d'exécution des PE Windows produits par C0 — état mesuré

Date : 2026-11-24. Hôte : `aarch64`, noyau Linux, worktree `/tmp/opencode/fin_wine`
(HEAD `0d0ebb3`). Aucun commit/push. Toutes les commandes ont été passées sous le
garde-fou `bash scripts/garde_fou_mem.sh 12000000 <commande>`.

Artefact cible : `rose.exe`, **1 413 632 octets**, md5
`9396d0bffa2caaef168e2f5a7bf918ac` (copies mesurées :
`/tmp/opencode/payload_c0_out/rose.exe`,
`/home/clausake/Stefan/ROSE/installateur/velopack/stage/rose.exe`).
`file` : `PE32+ executable (console) x86-64, for MS Windows, 3 sections`.

## 1. Inventaire réel des outils

| Outil | Résultat mesuré |
|---|---|
| `wine` | `/usr/bin/wine` → script `/usr/bin/wine-stable`, wine 9.0 Ubuntu |
| `wine64` (PATH) | **absent** ; `/usr/lib/wine/wine64` existe mais est **AArch64** (`readelf -h` : `Machine: AArch64`) |
| `wine32` | absent (`wine --version` : « it looks like wine32 is missing ») |
| `qemu-x86_64` | **absent** du PATH |
| `qemu-x86_64-static` | présent, ELF aarch64 statique |
| `box64` | `/usr/local/bin/box64`, arm64 v0.4.5 — émule des **ELF Linux** x86-64 |
| `box86`, `fex` | absents |
| `qemu-system-x86_64` | présent, mais **aucune image Windows** trouvée (`*.qcow2/*.img/*.iso/*.vhd/*.vmdk` : seuls des OS maison `mon_os64`) |
| Docker | 17 images présentes, **aucune image Windows** |
| `llvm-objdump`, `objdump`, `pefile` (Python) | présents |

Conclusion d'inventaire : l'hôte n'a **aucun** lanceur de PE x86-64 prêt à l'emploi.
Wine local est un Wine ARM64 (il exécuterait des PE ARM64, pas ce PE x86-64), et
les émulateurs user-space présents n'acceptent que des ELF Linux.

## 2. Essais directs (échecs attendus, mesurés)

```
wine rose.exe --help                    → rc=124 (timeout 15 s, 0 octet stdout)
wineboot -u (WINEPREFIX isolé)          → rc=124 après 240 s
   trace WINEDEBUG=+seh,+process,+loaddll sur 30 s :
   27 239 825 lignes, boucle sur
   "fixme:seh:execute_cfa_instructions ... unknown CFA opcode 2d"
box64 rose.exe --help                   → rc=255
   "[BOX64] Error: .../rose.exe is not an executable file."
qemu-x86_64-static rose.exe --help      → rc=1, 0 octet stdout, 0 octet stderr
   (qemu-user attend un ELF Linux, pas un PE)
```

Le `wine` fourni par la distribution n'a donc pas même pu initialiser un préfixe ;
ce n'est pas un problème propre à `rose.exe`.

## 3. Montage d'un Wine **amd64** réel sous qemu-x86_64 (contournement sans root)

Aucun paquet amd64 n'est installé sur l'hôte, mais les `.deb` Ubuntu noble sont
téléchargeables et extractibles sans root dans le worktree. Paquets extraits :
`wine64:amd64`, `libwine:amd64`, `libc6:amd64`, `libunwind8`, `liblzma5`,
`zlib1g`, `libz-mingw-w64`, `libgcc-s1`, `libfreetype6`, `libfontconfig1`,
`libexpat1`. Charges nécessaires mesurées une par une :

* `QEMU_LD_PREFIX=<root>` et `qemu-x86_64-static -L <root>` pour l'éditeur de liens ;
* `lib64/ld-linux-x86-64.so.2` et `lib/x86_64-linux-gnu` symlinkés dans le root ;
* `zlib1.dll` copié dans le dossier des DLL Wine (sinon `kernel32.dll`/`user32.dll`
  échouent à s'importer) ;
* `wineboot.exe` lancé **par son chemin PE réel** (le wrapper `wineboot` du paquet
  cherche un builtin `aa64` et échoue) ;
* les 726 DLL builtins du dossier Wine symlinkées dans
  `<prefix>/drive_c/windows/system32` (sans cela `kernel32.dll` n'est pas trouvé
  pour un exe tiers).

Résultat mesuré : `wine --version` → `wine-9.0 (Ubuntu 9.0~repack-4build3)` ;
`wineboot.exe -u` → **rc=0**.

### Témoin positif — le banc exécute bien un vrai PE x86-64

```
qemu-x86_64-static -L <root> <root>/usr/lib/wine/wine64 \
  <root>/usr/lib/x86_64-linux-gnu/wine/x86_64-windows/cmd.exe \
  /c echo CONTROL_OK
→ rc=0, stdout = "CONTROL_OK"
```

Donc le montage Wine-amd64/qqemu n'est pas une simulation : il charge, reloge et
exécute du code machine x86-64 issu d'un PE, et transmet sa sortie stdout.

## 4. Exécution de `rose.exe` et comparaison à `rose aot` sur 5 programmes

Commande unique (variables `<root>`, `<prefix>` ci-dessus) :

```
QEMU_LD_PREFIX=<root> WINEPREFIX=<prefix> WINEDEBUG=-all \
  qemu-x86_64-static -L <root> <root>/usr/lib/wine/wine64 \
  /tmp/opencode/payload_c0_out/rose.exe <programme.rose>
```

Cinq programmes `.rose` écrits dans le worktree (`>> 2 + 2`, `>> 6 * 7`,
`>> 100 / 4`, `>> 2 ^ 10`, `>> 1/3 + 1/6`). Référence native :
`bench/perf/algo/bin/rose aot pN.rose` (rc=0, « produit »), puis exécution du
binaire aarch64 généré.

| Programme | `rose aot` → sortie du binaire natif | `rose.exe` sous Wine-amd64/qemu |
|---|---|---|
| `p1` `>> 2 + 2` | `4` (rc=4) | **rc=1, stdout 0 octet** |
| `p2` `>> 6 * 7` | `42` (rc=42) | **rc=1, stdout 0 octet** |
| `p3` `>> 100 / 4` | `25` (rc=25) | **rc=1, stdout 0 octet** |
| `p4` `>> 2 ^ 10` | `1024` (rc=0) | **rc=1, stdout 0 octet** |
| `p5` `>> 1/3 + 1/6` | `0.5` (rc=0) | **rc=1, stdout 0 octet** |

Même résultat avec `aot <fichier>` en argument, avec `--help`, `help`, `info`,
`version` ou sans argument : sortie standard vide, rc=1. Le `stderr` des 5 cas ne
contient que 175 octets du même avertissement Wine sur FreeType que dans le
témoin `cmd.exe` — aucune donnée Rose.

Ce qui s'est réellement passé dans le PE (trace `WINEDEBUG=+file,+process,+heap`) :
le loader Wine mappe bien `rose.exe` (`map_image_into_view ... rose.exe`), puis on
voit une longue suite de `RtlAllocateHeap(..., size 0x18, ...)`, ce qui correspond
exactement au prologue désassemblé (`GetProcessHeap`, `HeapAlloc(0, 0x18)`).
Avec `WINEDEBUG=+heap` ou `+seh`, qemu finit par `Segmentation fault` (signal 11)
après cette suite d'allocations ; sans debug, Wine termine avec rc=1. **Aucun
appel à `WriteFile` n'a été tracé**, donc aucune sortie Rose n'a été produite.

Explication plausible mais **non prouvée comme cause unique** : le code généré
contient 4256 appels directs à l'adresse d'entrée `0x140001000` ; le premier,
en `0x14000106f`, est inconditionnel juste après les allocations. Un tel appel
réentrant est cohérent avec la suite d'allocations puis l'épuisement de pile
observés. Je n'affirme pas en avoir identifié formellement la cause (le mode
debug modifie le comportement de Wine/qemu).

## 5. Preuves statiques vérifiables (alternative réelle, distincte d'une exécution)

* `llvm-objdump -d --section=.text rose.exe` → **rc=0**, 214 924 lignes,
  **0 octet d'erreur sur stderr**, **0** marqueur `(bad)`/`invalid`/`error`.
  Le `.text` entier (0x158608 octets) est donc du x86-64 décodable de bout en bout.
* Section table : `.text` 0x158608 @0x140001000, `.data` 0x400, `.pdata` 0x24 ;
  `Machine` 0x8664, `AddressOfEntryPoint` 0x1000, `Subsystem` 3 (CUI).
* Table d'imports : **12/12** noms présents dans le `KERNEL32.dll` de Wine
  (dont `HeapAlloc`, simple forwarder vers `NTDLL.RtlAllocateHeap`) :
  `CloseHandle CreateFileA ExitProcess GetCommandLineA GetFileSize
  GetProcessHeap GetStdHandle GetTickCount64 HeapAlloc HeapFree ReadFile
  WriteFile`. Aucun import non résolu mesuré.
* Le témoin `cmd.exe` prouve que le chargeur et les imports Wine fonctionnent
  dans le même banc ; l'échec est donc propre à `rose.exe`.
* *Non fait* : comparaison bit-à-bit avec un PE Rust équivalent exécutable ailleurs.
  Une telle comparaison prouverait l'identité des octets, **pas** l'exécution,
  et n'a pas été réalisée ici — je ne la présente donc pas comme une preuve.

## 6. Ce qui est PROUVÉ / ce qui ne l'est PAS

**Prouvé par exécution mesurée**

1. Le banc Wine-amd64 sous qemu exécute réellement des PE x86-64 (`CONTROL_OK`).
2. `rose.exe` est reconnu, mappé et commencé par ce banc ; des appels Win32
   (`GetProcessHeap`/`HeapAlloc`) issus de son code s'exécutent.
3. Lancé comme `rose aot` sur 5 programmes, il ne produit **aucune** sortie
   (0 octet) et termine avec rc=1, à chaque fois.
4. Le binaire aarch64 issu de `rose aot`, lui, produit les 5 valeurs attendues.

**Pas prouvé**

1. Aucune sortie de moteur Rose (`4`, `42`, `25`, `1024`, `0.5`, bannière
   `--help`, etc.) n'a été obtenue depuis le PE.
2. Aucune parité fonctionnelle `rose.exe` ↔ `rose aot` n'a été observée ; la
   comparaison porte sur l'absence de sortie du PE, pas sur une réussite.
3. Rien ne démontre que `rose.exe` exécute le RIR C0 complet : le désassemblage
   montre 4256 appels réentrants à l'entrée, et l'exécution se termine sans
   écriture. La structure PE est valide, **la sémantique n'est pas démontrée**.
4. Aucun des autres artefacts Windows (shell/tray/setup/payload) n'a été exécuté.
5. Aucune exécution sur Windows natif n'a eu lieu (aucune image/VM Windows ici).

Aucun code de sortie, aucune sortie stdout et aucun résultat de comparaison
n'ont été inventés : tout ce qui figure ci-dessus est issu de commandes réellement
exécutées et de fichiers réellement lus.

## 7. Reproduction

```
cd /tmp/opencode/fin_wine
bash scripts/garde_fou_mem.sh 12000000 \
  env PE=/tmp/opencode/payload_c0_out/rose.exe \
      bash scripts/tester_pe_windows.sh
```

Le script `scripts/tester_pe_windows.sh` réinventorie les outils, teste
wine/box64/qemu, construit le root Wine-amd64 si nécessaire, joue le témoin
`cmd.exe`, compare les 5 programmes et imprime les contrôles statiques. Il
télécharge les `.deb` listés au §3 et les extraits dans `.preuve_exec_win/`
(worktree uniquement). Si un root Wine-amd64 existe déjà, le passer avec
`WINE_ROOT=/chemin/...` évite le téléchargement.
