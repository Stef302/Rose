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

## Chaîne de build (Windows uniquement)

Prérequis : Windows x64, Rust MSVC (`rustup target add x86_64-pc-windows-msvc`),
Visual C++ Build Tools + Windows SDK, CLI Tauri 2 (`cargo install tauri-cli --version "^2"`)
et la CLI Velopack (`dotnet tool install -g vpk`).

```powershell
# 1. le binaire du moteur (embarqué dans le payload)
cd rose
cargo build --release --target x86_64-pc-windows-msvc
cd ..\installateur\velopack
powershell -File .\build.ps1 -Version 0.1.4
```

`build.ps1` enchaîne :

1. `cargo tauri build --no-bundle --target x86_64-pc-windows-msvc` : l'assistant
   graphique (`installateur/tauri-installer`, HTML + backend Rust `install_app`) ;
2. `vpk pack --packId Rose --packVersion <v> --packDir .\stage --mainExe rose.exe`
   : produit `Releases\Rose-win-Setup.exe`, `Rose-<v>-full.nupkg`, `releases.win.json` ;
3. `vpk local` pour tester localement (installe dans le profil courant) ;
4. `vpk push` (ou copie manuelle des fichiers dans la release GitHub) pour publier.

## Ce qui est publié

| Fichier | Rôle |
|---|---|
| `Rose-win-Setup.exe` | installeur Velopack (deltas + rollback, sans admin) |
| `Rose-<v>-full.nupkg` | paquet complet |
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
analyse du manifeste, détection du launcher).

## Remplacements

- `installateur/wix/` et `installateur/rose.nsi` sont **retirés** : Velopack est
  la seule voie Windows.
- Sous Linux et macOS, `rose update` reste sur la voie artefacts GitHub
  (`docs/telechargements/` + SHA-256) : Velopack n'a pas d'installeur natif Linux.
