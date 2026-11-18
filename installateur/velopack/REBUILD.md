# Reconstruction des artefacts Windows — 18/11/2026

Traçabilité de la reconstruction des binaires Windows de Rose avec **tous les
commits récents** (M1–M6, point fixe, perf). Aucun fichier de `rose/src/*`,
`compilateur/*`, `Cargo.toml`, `PLAN.md` ou `main.rs` n'a été modifié.
Aucun commit Git, aucune publication GitHub.

## 1. État initial (avant reconstruction)

Artefacts `installateur/velopack/stage/` datés du **17/11 13:10**, alors que
les sources ont continué à changer jusqu'au 18/11 (commits `24daf0c`,
`c36d36d`, `95e2782`, `dc97a63`, `1b73c74`…). Reconstruction nécessaire.

| Fichier | Taille avant | MD5 avant |
|---|---:|---|
| `stage/Rose-win-Setup.exe` | 9 459 200 | `5f43abcc70d39c21b6e427d6ce6f4ee7` |
| `stage/rose.exe` | 7 791 616 | `7cb548fd9458461388ad9886855b5e1b` |
| `stage/rose-shell.exe` | 476 672 | `caa4eacdba71c0545a124a1502eb95c5` |
| `stage/payload/rose.exe` | 7 791 616 | `7cb548fd9458461388ad9886855b5e1b` |
| `Releases/Rose-0.1.4-full.nupkg` | 5 255 132 | `583600bdc30c4afa8efd49884320fc2d` |
| `Releases/Rose-0.1.5-full.nupkg` | 8 535 331 | `fb26a281aead91830829d2cad83caf86` |

## 2. Blocage rencontré et contournement

### 2.1 Échec de la chaîne normale

`sh installateur/velopack/build-linux.sh` échoue au **lien** (exit 101) :

```
lld-link: error: undefined symbol: munmap
  >>> referenced by rose::jit::ne_local::MemoireExecutable::nouveau
lld-link: error: undefined symbol: mmap
  >>> referenced by rose::natif_emetteur::MemoireExecutable::nouveau
lld-link: error: undefined symbol: mprotect
```

Cause : le commit **M3.1 (`1b73c74`, « câblage JIT »)** appelle
`mmap`/`mprotect`/`munmap` de façon inconditionnelle dans
`rose/src/natif_emetteur.rs` (réimporté dans `jit.rs` sous `ne_local`). Ces
symboles POSIX n'existent pas sur Windows et ne sont pas fournis par
`rose/src/windows_compat.c` (qui ne contient que `dlopen`/`dlsym`/`getrandom`).
Le `build-linux.sh` d'origine ne peut donc plus produire d'exe Windows avec
les sources actuelles. **Interdiction de modifier `rose/src/*` dans cette
mission** : contournement par injection au lien.

### 2.2 Shim de lien (aucune source touchée)

- `installateur/windows-compat/mmap_win.c` (NOUVEAU) : implémente
  `mmap`/`mprotect`/`munmap` avec `VirtualAlloc`/`VirtualProtect`/`VirtualFree`
  (constantes POSIX PROT_* / MAP_* conformes à `natif_emetteur.rs`). Les
  fonctions Win32 sont déclarées à la main car le cache xwin n'a pas
  `excpt.h` (donc pas de `<windows.h>` compilable ici). Noms non décorés en
  ABI MSVC x64 : résolution directe depuis `kernel32.lib`.
- `scripts/build-windows-rebuild.sh` (NOUVEAU) : compile le shim en objet COFF
  et l'injecte dans **chaque lien** via un wrapper
  `lld-link /tmp/opencode/rose-rebuild/mmap_win.obj "$@"` posé dans
  `CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER`. Le reste de l'environnement
  est identique à `build-linux.sh` (SDK/UCRT xwin, clang, lld-link, llvm-lib,
  `+crt-static`).

### 2.3 Pourquoi deux passes

`pack-res.py` embarque le `rose.exe` **présent dans `target/` au moment où il
tourne**. Une seule passe (comme dans `build-linux.sh`) embarquerait le vieux
moteur dans `Rose-win-Setup.exe`. Le script fait donc :

1. **Passe 1** : `embed-assets.py` + `pack-res.py`, puis
   `cargo build --release --target x86_64-pc-windows-msvc --bin rose --bin
   rose-shell --bin rose-setup` → moteur et shell neufs.
2. **Passe 2** : `pack-res.py` à nouveau (avec le moteur neuf) → `rose-setup.res`
   régénéré, puis `cargo build --bin rose-setup` → relink du setup avec le
   moteur neuf.

## 3. Commandes exactes exécutées

```sh
cd /home/clausake/Stefan/ROSE
export PATH="$HOME/.cargo/bin:$PATH"
sh scripts/build-windows-rebuild.sh        # → stage/ + vérifications
sh installateur/velopack/pack-linux.sh 0.1.5   # → .nupkg + releases.win.json
```

Outils vérifiés : `rustc 1.99.1`, `cargo 1.99.1`, `cargo-xwin 0.23.1`, cibles
`x86_64-pc-windows-{msvc,gnu,gnullvm}` installées, `clang`, `lld-link`,
`llvm-lib`, `llvm-rc`, `python3`, `zip`. SDK xwin présent dans
`~/.cache/cargo-xwin/xwin`.

## 4. Résultats — hashs après reconstruction

| Fichier | Taille après | MD5 après |
|---|---:|---|
| `stage/Rose-win-Setup.exe` | 9 633 792 | `4a2e42e4595b400e5cad50fef1e7e0b0` |
| `stage/rose.exe` | 7 963 648 | `3b4276dd8f55b2c068d7993cf73166b0` |
| `stage/rose-shell.exe` | 477 696 | `4c74033a27e405286ad08ab2b447cbe6` |
| `stage/payload/rose.exe` | 7 963 648 | `3b4276dd8f55b2c068d7993cf73166b0` |
| `Releases/Rose-0.1.5-full.nupkg` | 8 740 427 | `7e1495b9107aab444ab1b3ce889f29fa` |

Paquet : SHA-256 `ad418be2397d421db90e2e27a14e91250a04085fe5d402b087a73a90ba207df5`,
SHA-1 `1e52f888602dd7408d392eb6bcfca292be15773c`, taille 8 740 427 o.
Le manifeste `releases.win.json` a été régénéré en cohérence.

`file` (sortie exacte) :

```
stage/Rose-win-Setup.exe: PE32+ executable (GUI) x86-64, for MS Windows, 9 sections
stage/rose.exe:           PE32+ executable (console) x86-64, for MS Windows, 9 sections
stage/rose-shell.exe:     PE32+ executable (console) x86-64, for MS Windows, 9 sections
```

## 5. Vérifications réelles

- **`verifier-res.py rose-setup.exe`** → `RESSOURCES VALIDES` : 5 types,
  16 ressources, 7 tailles d'icônes (16→256 px), manifeste 1 474 o ASCII
  PerMonitorV2, subsystem 2 (GUI), moteur embarqué 7 963 648 o (PE console),
  compagnons `rose-shell.exe` 477 696 o, `rose.ico` 112 510 o,
  `rose_app.ico` 64 700 o, `rose_shell.ico` 113 565 o, rose PNG 276×343 RGBA,
  33 garde-fous de tracé présents.
- **Moteur embarqué = moteur livré** : `RT_RCDATA 3` du setup est
  **octet-identique** à `stage/rose.exe` (md5 `3b4276dd…`) ; `RT_RCDATA 5`
  est octet-identique à `stage/rose-shell.exe` (`4c74033a…`). La passe 2 a
  bien fonctionné.
- **Icônes par binaire** (extraction des DIB `RT_ICON` des PE, comparaison aux
  entrées des `.ico` sources) :
  - `rose.exe` ↔ `rose/rose.ico` : **6/6 tailles**, md5 identiques
    (`27884f7bd5db`, `4b726aa2cc63`, `4eeb5263463c`, `61de67f4c8ce`,
    `9f8c0820dfbb`, `b8243812c482`).
  - `rose-shell.exe` ↔ `rose/rose_shell.ico` : **7/7 tailles**, md5 identiques
    (`162efec2a2c3`, `48d245ff3d20`, `68fdef109bf8`, `6fc03d8cb1cd`,
    `ce3a75819f9a`, `d8b758633a01`, `da9e842764df`).
- **Contenu du `.nupkg`** : 10 entrées `lib/net45/...` + `sq.version` ; le
  `rose.exe` interne est octet-identique à celui du stage.
- **Déterminisme du paquet** : deux exécutions de `pack-linux.sh` donnent le
  même SHA-256 (`ad418be2…`, horodatage FIXE des entrées ZIP).
- **Syntaxe** : `sh -n` OK sur `build-linux.sh`, `pack-linux.sh`, `verifier.sh`,
  `build-windows-rebuild.sh`.
- `rose/src/assets_installeur.rs` (généré par `embed-assets.py`) est resté
  **byte-identique** à sa version précédente (md5 `a17967f96e970dcf2ea9adb902ba79be`).
- `git status` : seuls 3 chemins non suivis ont été ajoutés par cette mission :
  `installateur/windows-compat/`, `scripts/build-windows-rebuild.sh` (et
  `scripts/build-runtime-runner.sh` appartient à un autre agent, non touché).
  Aucun fichier suivi modifié, aucun commit.

## 6. Ce qui a échoué / limites

- **CLI Velopack absente** : `vpk` = « command not found » et `dotnet` =
  « command not found ». Impossible d'exécuter le `vpk pack` de `build.ps1`
  (qui produirait un `Rose-win-Setup.exe` *launcher* Velopack officiel avec
  deltas/rollback). Le paquet a été produit par la voie Python prévue
  `pack-linux.sh` ( ZIP + `sq.version` + manifeste ), comme le prévoit le
  README pour Linux. À faire sur machine Windows/outillée : `dotnet tool
  install -g vpk` puis `vpk pack …` (voir `installateur/velopack/build.ps1`).
- **Exécution Windows non testée** : environnement Linux, aucun Wine/Windows.
  Les PE sont validés structurellement (`file`, arbre `.rsrc`, sous-système,
  manifeste, DIB d'icônes) mais **pas lancés**. Ne pas affirmer que
  l'installateur fonctionne à l'exécution.
- **Shim mmap = contournement** : il rétablit la compilation avec les sources
  actuelles sans toucher à `rose/src`. La vraie correction durable est
  d'implémenter `mmap`/`mprotect`/`munmap` pour Windows directement dans
  `rose/src/windows_compat.c` (ou de garder `natif_emetteur.rs` sous
  `cfg(unix)`), ce qui permettrait de revenir au `build-linux.sh` nu et
  rendrait `scripts/build-windows-rebuild.sh` inutile.
- **Commits concurrents** : d'autres agents continuaient à committer pendant
  la reconstruction (HEAD est passé de `63ab156` à `e46c2e7`). Le dernier
  (`e46c2e7`) ajoute `rose/src/bin/rose_nostd.rs`, un nouveau binaire non
  concerné par les trois exe reconstruits (le script passe des `--bin`
  explicites). Si de nouveaux commits modifient `rose/src` après cette
  reconstruction, relancer `scripts/build-windows-rebuild.sh`.
