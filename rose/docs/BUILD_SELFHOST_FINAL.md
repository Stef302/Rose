# Build `--selfhost` final — release Windows 100 % sans cargo

**Constat daté : 24/11/2026, 06:08:38 → 06:45:58 UTC** (vérification des artefacts
06:51 UTC). Re-mesurable ; aucune conclusion n'est tirée d'un document antérieur.

## Commande exacte et résultat

```sh
bash scripts/garde_fou_mem.sh 12000000 timeout 3000 \
  sh scripts/build-windows-rebuild.sh --selfhost
# journal : /tmp/opencode/selfhost_final2.log
# shell de fond : sh_20c2ba68d001uXRjvKVjWeCnyR
```

- **rc = 0**, lu dans la sortie du shell de fond
  `/home/clausake/.local/share/opencode/shell/fd4e50f3a56c96d48504eac9035cedccdc2a89b2/sh_20c2ba68d001uXRjvKVjWeCnyR.out` (`rc=0`, ligne 6).
- Dernière ligne du journal :
  `-rw-rw-r-- 1 clausake clausake  1413632 Nov 24 07:45 rose.exe`
  (listage final `=== STAGE ===`, les heures du journal sont en heure locale +0100).
- Aucune ligne d'échec dans le journal (`ÉCHEC`, `BLOQUÉ`, `Traceback`, `ERREUR` : 0 occurrence).
- Étapes datées : C0 `moteur_complet.rose` → `moteur.elf` 06:17:32 UTC ; 12/12 tests rc=0
  06:17:54 ; 4 ELF auxiliaires compilés par C0 06:18:00–06:18:08 ; RIR complet
  06:23:46 (795 791 o) ; émission PE 06:23:46 → 06:45:57 (~1 331 s, sous le délai
  2 400 s corrigé en `ccc2aae`) ; setup + vérifications 06:45:58. Durée totale ~37 min 20 s.
- Première ligne du journal : `=== MODE SELFHOST : compilation par C0, AUCUNE commande cargo ===`.
  Aucun `cargo`, `rustc`, `clang`, `lld-link` ni `llvm-rc` n'a été lancé dans cette chaîne
  (les seules occurrences de « cargo » dans le journal sont les textes de titre du script).

## Les 5 artefacts (stage `installateur/velopack/stage/`)

Mesures directes : `file -b`, `stat`, `md5sum`, `llvm-readobj`, et
`WINDOWS_ARTIFACTS.tsv` / `LLVM_READOJ.txt` du stage.

| artefact | taille (o) | md5 | machine | magic PE | sections | `file -b` |
|---|---:|---|---|---|---|---|
| `rose.exe` | 1 413 632 | `9396d0bffa2caaef168e2f5a7bf918ac` | 0x8664 | 0x20B | 3 : `.text`, `.data`, `.pdata` | PE32+ executable (console) x86-64 |
| `rose-shell.exe` | 124 416 | `c955c6608b0913f7fa541d74cd9b457c` | 0x8664 | 0x20B | 3 : `.text`, `.data`, `.rsrc` | PE32+ executable (console) x86-64 |
| `rose-tray.exe` | 125 952 | `1cbc358a85eb4a8b55c0b12e251782ce` | 0x8664 | 0x20B | 3 : `.text`, `.data`, `.rsrc` | PE32+ executable (GUI) x86-64 |
| `Rose-win-Setup.exe` | 10 476 032 | `d160c6f80e16037a3d10665ae60bc447` | 0x8664 | 0x20B | 3 : `.text`, `.data`, `.rsrc` | PE32+ executable (GUI) x86-64 |
| `payload/rose.exe` | 9 753 600 | `02528a069c185bcedaffd626aff3ecee` | 0x8664 | 0x20B | 9 (`moteur-console-reel`, du nupkg) | PE32+ executable (console) x86-64 |

Journal : `5/5 ARTEFACTS PE32+ x86-64 VALIDES` (deux fois : dans
`build-sans-cargo.sh`, puis sur le stage par `build-windows-rebuild.sh`).

`llvm-readobj --file-headers --sections --coff-imports` sur **`rose.exe`** confirme
`Machine: IMAGE_FILE_MACHINE_AMD64 (0x8664)`, `Magic: 0x20B`, 3 sections et
**12 imports** `KERNEL32.DLL` : `GetStdHandle`, `WriteFile`, `ExitProcess`,
`GetProcessHeap`, `HeapAlloc`, `HeapFree`, `CreateFileA`, `ReadFile`,
`CloseHandle`, `GetFileSize`, `GetCommandLineA`, `GetTickCount64`.

`python3 installateur/velopack/verifier-res.py Rose-win-Setup.exe` (sur le stage) →
**RESSOURCES VALIDES** : 5 types, 17 ressources, 7 tailles d'icônes
(16→256 px), manifeste PerMonitorV2 (1 474 o), 33 garde-fous présents,
moteur embarqué 9 753 600 o, `rose-shell.exe`, `rose-tray.exe` et les 3 icônes
embarquées. `VERIFIER_RES.txt` et `LLVM_READOJ.txt` sont copiés dans le stage.

## Preuve que `rose.exe` est compilé par C0 depuis le moteur complet

Chaîne complète, sans compilation Rust :

1. `COMPILATION C0 : /tmp/opencode/C0_h.elf (2db168b907b4a874712857d3957fd078) <- moteur_complet.rose (e5bc586cfddc46c2d32338a52a51c2b4)`.
2. `moteur.elf` = ELF64 aarch64, 1 965 712 o, md5 `5e9256a664ac4dfd0c54bc5fec473489`.
3. Émetteur PE x64 compilé par C0 depuis `compilateur/emetteur_pe_x64.rose` :
   `emetteur_pe_x64.elf`, 225 652 o, md5 `fcf9b8a6e4e444ab5bb08d7f617bca25`
   (commit `b45970f` : `x_eval`/`x_op` étendus, 23 primitives, tas x64).
4. `WINDOWS_TENTATIVES.txt` du stage, lignes 7–9 :
   `TENTATIVE RIR moteur complet : …/moteur.elf rir moteur_complet.rose`,
   `RIR COMPLET obtenu : 795791 octets`,
   ` rose.exe = moteur complet émis en PE`.
5. `MANIFESTE.txt` : `origine rose.exe : moteur complet (RIR C0 -> PE C0)`.
6. `/tmp/opencode/release-sans-cargo/rose_complet.exe` et le `rose.exe` livré ont
   le **même md5** (`9396d0bf…`) et `cmp` rend 0 : le PE livré est bien la sortie
   de l'émetteur C0, pas une copie de release.
7. Aucun repli sur stub n'a eu lieu : la branche `>> 73` du script n'a pas été
   exécutée (absente de `WINDOWS_TENTATIVES.txt`).

⚠ Ne pas confondre avec `/tmp/opencode/last2_rose_final.exe`
(md5 `8e73e1679930c3565d5600ac7765cba5`, 1 413 120 o) : c'est un artefact
**antérieur**, de 512 o plus petit et différent octet par octet ; il n'est pas celui
livré par ce build (1 413 632 o, `9396d0bf…`).

## Verdict vrai

**Oui, au niveau des artefacts : la distribution Windows complète (5 PE32+
x86-64) a été produite sans cargo, avec un `rose.exe` issu du RIR complet du
moteur compilé puis émis en PE par C0.** Les lanceurs `rose-shell.exe`,
`rose-tray.exe` et le setup sont du code Win32 réel écrit en Rose (commit
`e2ee006`), le setup embarque de vraies ressources validées, et
`payload/rose.exe` est le moteur console réel du nupkg 0.1.76.

**Ce qui reste / limites exactes :**

- Aucune exécution sous Windows n'a été faite ici : la preuve est statique
  (magic, machine, sections, imports, ressources, hash), pas un test de lancement.
- `payload/rose.exe` n'est **pas** compilé par C0 : c'est le moteur console
  signé du nupkg 0.1.76, embarqué tel quel comme RCDATA 10/3.
- Le moteur auto-hébergé Linux `moteur.elf` couvre 12 programmes de test
  (`TESTS : 12/12 rc=0`) et **refuse explicitement** (rc=3)
  `aot|orc|link|llvm|console|repl|rvm|wasm` : JIT Cranelift et AOT/LLVM ORC ne
  sont pas dans le chemin sans cargo.
- Deux textes du stage sont **périmés et contredits par les mesures ci-dessus** :
  `NON_COUVERT_SELFHOST.txt` (écrit tel quel par `build-windows-rebuild.sh`,
  encore rédigé pour les anciens stubs) et la dernière ligne du `MANIFESTE.txt`
  (« moteur complet hors périmètre (autre agent) »). Le `WINDOWS_TENTATIVES.txt`
  et les 5 artefacts font foi.
- Aucun commit ni push effectué par ce rapport ; seul ce document a été ajouté.
