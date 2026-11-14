# Installeur Velopack de Rose (Windows)

Velopack **remplace WiX et NSIS** : il installe Rose dans un dossier versionné
(`%LOCALAPPDATA%\Rose\app-0.1.4\`, désigné par `current`), pose le PATH et les
raccourcis, et fournit un launcher (`Update.exe`) qui applique les mises à jour
par **deltas binaires** et revient à la version précédente si la nouvelle ne
démarre pas.

Le moteur Rust connaît le protocole Velopack sans aucune dépendance
(`rose/src/velopack.rs`) : `rose update` lit `releases.win.json`, compare la
version installée, télécharge le paquet **complet** (`Type: Full`), vérifie son
SHA-256 puis le dépose dans `packages\`. Le launcher applique le paquet au
démarrage suivant. Les arguments internes `--veloapp-updated` et
`--veloapp-obsolete` sont routés en tête de `main()` (ils ne sont jamais pris
pour un nom de fichier).

## Trois binaires Windows

| Fichier | Type | Rôle |
|---|---|---|
| `rose.exe` | PE32+ console | le moteur (interprète exact, JIT Cranelift, AOT, WASM côté module) |
| `rose-aot.exe` | PE32+ console | helper AOT LLVM ORC, téléchargé à la demande par `rose orc` |
| `Rose-win-Setup.exe` | PE32+ **GUI** | installeur autonome : fenêtre Win32 de `setup_gui`, aucun droit admin |

Les trois sont liés **statiquement** (`+crt-static`) : aucune DLL
`VCRUNTIME140` ni `api-ms-win-crt-*` n'est nécessaire sur la machine cible ;
les seules dépendances sont `kernel32` / `user32` / `gdi32` / `ntdll` /
`bcryptprimitives`, présentes sur Windows 10 et 11.

## Build depuis Linux (sans Visual Studio)

`installateur/velopack/build-linux.sh` construit la cible
`x86_64-pc-windows-msvc` avec **clang + lld-link + llvm-lib** et le SDK/UCRT
que `cargo-xwin` met en cache (`~/.cache/cargo-xwin/xwin`) :

```sh
sh installateur/velopack/build-linux.sh   # → rose.exe, rose-aot.exe, rose-setup.exe
```

`pack-linux.sh` emballe ensuite le dossier `stage` en paquet Velopack et écrit
le manifeste (Python 3 suffit, pas de CLI `vpk`) :

```sh
sh installateur/velopack/pack-linux.sh    # → Releases/Rose-0.1.4-full.nupkg + releases.win.json
```

## Build depuis Windows (assistant Tauri + CLI Velopack)

Prérequis : Windows x64, Rust MSVC (`rustup target add x86_64-pc-windows-msvc`),
Visual C++ Build Tools + Windows SDK, CLI Tauri 2 (`cargo install tauri-cli --version "^2"`)
et la CLI Velopack (`dotnet tool install -g vpk`).

```powershell
powershell -File .\build.ps1 -Version 0.1.4
```

`build.ps1` enchaîne :

1. `cargo build --release --target x86_64-pc-windows-msvc` : moteur +
   `rose-aot` + `rose-setup` (le binaire GUI, déclaré dans `rose/Cargo.toml` ;
   `build.rs` lui pose `/SUBSYSTEM:WINDOWS` + `/ENTRY:mainCRTStartup`) ;
2. `cargo tauri build --no-bundle` : l'assistant graphique WebView2
   (`installateur/tauri-installer`, commande Rust `install_app`) ;
3. `vpk pack --packId Rose --packVersion <v> --packDir .\stage --mainExe rose.exe`
   : produit `Releases\Rose-win-Setup.exe`, `Rose-<v>-full.nupkg`, `releases.win.json` ;
4. `vpk local` pour tester localement (installe dans le profil courant) ;
5. `vpk push` (ou copie manuelle des fichiers dans la release GitHub) pour publier.

## Ce qui est publié

| Fichier | Rôle |
|---|---|
| `Rose-win-Setup.exe` | installeur Velopack (deltas + rollback, sans admin) |
| `Rose-<v>-full.nupkg` | paquet complet (contient `rose.exe`, `rose-aot.exe`, `Rose-win-Setup.exe`, `payload\`) |
| `Rose-<v>-delta.nupkg` | delta depuis la version précédente (optionnel) |
| `releases.win.json` | manifeste lu par `rose update` |

## Vérifications possibles sans Windows

La voie de mise à jour est portable et testée ici (`ROSE_VELOPACK_ROOT`,
`ROSE_VELOPACK_URL` en `file://`, `ROSE_VELOPACK=1`) :

```bash
printf 'app-0.1.3' > /tmp/vpk/current
ROSE_VELOPACK_ROOT=/tmp/vpk ROSE_VELOPACK_URL=file:///tmp/vpk/releases.win.json \
  ./rose/rose update --velopack     # → paquet 0.1.4 téléchargé + SHA-256 vérifié
./rose/rose --veloapp-updated       # → code 0
```

Tests Rust : `cargo test --release velopack` (3 tests : comparaison de versions,
analyse du manifeste, détection du launcher). `installateur/velopack/verifier.sh`
joue les 6 contrôles de bout en bout.

## Remplacements

- `installateur/wix/` et `installateur/rose.nsi` sont **retirés** : Velopack est
  la seule voie Windows.
- Sous Linux et macOS, `rose update` reste sur la voie artefacts GitHub
  (`docs/telechargements/` + SHA-256) : Velopack n'a pas d'installeur natif Linux.
