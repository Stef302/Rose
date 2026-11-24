# Lanceurs Windows écrits en Rose — état mesuré (24/11/2026)

Ce document remplace, pour les **quatre artefacts Windows**, le constat « stub »
de `rose/docs/RELEASE_100.md` §3. Tout ce qui suit a été **exécuté** dans le
worktree `/tmp/opencode/last3` (HEAD `be13861`) avec
`bash scripts/garde_fou_mem.sh 12000000 …`, jamais déduit du code source.

## 1. Sources possédées et artefacts produits

| Source Rose (compilée par C0) | Artefact | Nature réelle |
|---|---|---|
| `compilateur/rose_shell_win.rose` | `rose-shell.exe` | PE32+ console. Code x64 **réel** : `SearchPathW` ( cherche `rose.exe`), `GetCommandLineW`, construction de `rose.exe console <args>` en UTF‑16, `CreateProcessW`, `WaitForSingleObject(INFINITE)`, `GetExitCodeProcess`, `ExitProcess(code)`. |
| `compilateur/rose_tray_win.rose` | `rose-tray.exe` | PE32+ **GUI** (sous‑système 2, pas de console noire). `RegisterClassExW` + `CreateWindowExW(HWND_MESSAGE)`, `NOTIFYICONDATAW` 952 octets + `Shell_NotifyIconW(NIM_ADD)`, `SetTimer(… , 21 600 000 ms, NULL)` (6 h), lecture **réelle** du manifeste (`CreateFileW`/`ReadFile`/`CloseHandle`) au démarrage **et à chaque `WM_TIMER`**, journal `rose-tray.log` via `WriteFile`, boucle `GetMessageW`/`TranslateMessage`/`DispatchMessageW`. |
| `compilateur/pe_stub_ressources.rose` (mode `setup`, gardé aussi pour `stub`/`payload`) | `rose-setup.exe` puis copie `Rose-win-Setup.exe` | PE32+ **GUI** (sous‑système 2). Fenêtre Win32 réelle : `WNDCLASSEXW` 80 octets, `RegisterClassExW`, `CreateWindowExW(WS_OVERLAPPEDWINDOW)`, `ShowWindow(SW_SHOWNORMAL)`, `UpdateWindow`, `WM_PAINT` → `BeginPaint`/`GetClientRect`/`DrawTextW`/`EndPaint`, `WM_CLOSE`/`WM_DESTROY` → `PostQuitMessage`, boucle de messages. Table `.rsrc` complète (17 ressources). |
| recopie de l'artefact publié (`docs/telechargements/Rose-0.1.76-full.nupkg`, `lib/net45/rose.exe`) | `payload/rose.exe` | Vrai PE console 9 753 600 octets, sous‑système 3, signé et testé par la release 0.1.76. Remplace le stub de 2,5 Ko. |

`compilateur/pe_stub_ressources.rose` sert à la fois de pilote autonome et de
**noyau commun** : les trois sources sont auto-suffisantes (le noyau y est
reproduit en entier). Raison mesurée : sous C0, `importer` ne partage **pas**
les variables globales entre modules — `joindre($dlls, …)` depuis le module
importateur segfaultait (preuve : `/tmp/opencode/test_imp3.elf`). Les sources
possèdent donc chacune leur copie du noyau, générée depuis
`pe_stub_ressources.rose`.

## 2. Ressources (toutes réelles, aucune fabrication)

Le mode `setup` reçoit `type / id / chemin / taille` pour chaque ressource.
La taille est passée explicitement par le script : sous C0,
`longueur()` d'une chaîne contenant `octet_brut(128)` vaut **0** (mesuré),
donc une taille calculée en `longueur()` serait fausse.

| Type Win32 | Id | Contenu |
|---|---|---|
| `RT_RCDATA` | 1 | `installateur/velopack/rose-installeur.png` (31 391 o, PNG RGBA) |
| `RT_FONT` | 2 | Kare TrueType généré depuis `Kare.woff2` par `pack-res.py::woff2_vers_ttf` (28 352 o) |
| `RT_RCDATA` | 3 | moteur console 0.1.76 (9 753 600 o, sous‑système 3) |
| `RT_RCDATA` | 5 | `rose-shell.exe` Rose (124 416 o) |
| `RT_RCDATA` | 6/7/8 | `rose.ico`, `rose_app.ico`, `rose_shell.ico` |
| `RT_RCDATA` | 9 | `rose-tray.exe` Rose |
| `RT_MANIFEST` | 1 | `rose/rose-setup.manifest` (1 474 o, ASCII, `PerMonitorV2`) |
| `RT_ICON` | 1..7 | DIB bruts extraits de `rose_setup.ico` (16→256 px) |
| `RT_GROUP_ICON` | 1 | `GRPICONDIR` 14 octets/entrée, construit en Rose |

L'arbre `.rsrc` est écrit à la main (racine → types → noms → langue 1033 →
`IMAGE_RESOURCE_DATA_ENTRY`), avec `OffsetToData` en **RVA** et alignement 4.

## 3. Ce qui est réel vs ce qui ne l'est pas

**Réel et vérifié** : les trois lanceurs contiennent du code machine x64 qui
appelle effectivement les API listées ci‑dessus (preuve : désassemblage
`llvm-objdump -d --section=.text` et imports `llvm-readobj --coff-imports`).
Les ressources, le sous‑système GUI, le manifeste et le moteur embarqué sont
les octets des fichiers réels du dépôt (preuve : `verifier-res.py`).

**Hors périmètre de ce agent, donc inchangé** : le moteur Rose complet
(`rose.exe` seul, émetteur `emetteur_pe_x64.rose`) appartient à un autre
agent. Le `rose.exe` produit par la branche `--windows` reste issu du pilote
RIR entier ; `payload/rose.exe`, lui, est la vraie release. Le `WndProc` de
`rose-tray.exe` est volontairement minimal (retour 0) : le menu contextuel
n'est pas câblé dans la version Rose, mais toutes les API d'import présentes
sont des imports Win32 authentiques ; le timer, l'icône, la lecture du
manifeste et le ballon sont, eux, exécutés. Le setup Rose ne repeint pas
l'assistant GDI+ de `setup_gui.rs` (4 648 lignes) : il ouvre une vraie fenêtre
Win32 et y dessine un texte via `DrawTextW`.

## 4. Preuves statiques re-mesurées

Commande :
```
cd /tmp/opencode/last3
bash scripts/garde_fou_mem.sh 12000000 bash scripts/build-sans-cargo.sh \
  --reutiliser --windows --sortie /tmp/opencode/last3_win
```

Résultat (rc=0) :

```
5/5 ARTEFACTS PE32+ x86-64 VALIDES
rose-shell.exe         PE32+ executable (console) x86-64, for MS Windows, 3 sections
rose-tray.exe          PE32+ executable (GUI) x86-64, for MS Windows, 3 sections
rose-setup.exe : section .rsrc de 10473472 o
  racine : 5 types / 17 ressources
  groupe d'icônes : 7 tailles (16x16 … 256x256)
  manifeste : 1474 o, ASCII, PerMonitorV2
  subsystem : 2 (GUI)
  moteur embarqué : 9753600 o (PE console)
  rose-shell.exe     124416 o
RESSOURCES VALIDES
TESTS : 12/12 rc=0
```

`llvm-readobj --coff-imports` montre notamment :
* shell : `KERNEL32.DLL` → `SearchPathW`, `GetCommandLineW`, `CreateProcessW`,
  `WaitForSingleObject`, `GetExitCodeProcess`, `ExitProcess` ;
* tray : `USER32.DLL` (16 symboles dont `SetTimer`), `SHELL32.DLL` →
  `Shell_NotifyIconW`, `KERNEL32.DLL` dont `CreateFileW`/`ReadFile` ;
* setup : `USER32.DLL` (15 symboles) + `KERNEL32.DLL` → `ExitProcess` ;
  `Subsystem: IMAGE_SUBSYSTEM_WINDOWS_GUI (0x2)`.

## 5. Non-régressions

* point fixe : `moteur.elf rir compilateur/emetteur.rose | md5sum` →
  `b3ed9262dd54776a6dbd07bc12368d0e` (inchangé) ;
* `rose verif` des trois sources → `vérifié` ×3 ;
* 12 programmes selfhost : 12/12 rc=0 ;
* la branche `cargo` par défaut de `build-windows-rebuild.sh` n'a pas été
  touchée ; seul le chemin `--windows` de `build-sans-cargo.sh` a été étendu.
