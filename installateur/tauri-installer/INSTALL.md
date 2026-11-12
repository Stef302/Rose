# Installeur Rose Tauri - Guide de build Windows

## Prérequis (sur Windows)

1. **Rust + Rustup** : https://rustup.rs/
2. **Cible Windows MSVC** :
   ```powershell
   rustup target add x86_64-pc-windows-msvc
   ```
3. **CLI Tauri** :
   ```powershell
   cargo install tauri-cli --version "^2"
   ```
4. **Microsoft Visual C++ Build Tools** (pour `link.exe`) :
   - https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022
   - Sélectionner "C++ build tools" + "Windows 11 SDK"
5. **WebView2 Runtime** : déjà présent sur Windows 10/11 modernes

## Build de l'installeur

```powershell
# Depuis ce dossier (installateur/tauri-installer/)
.\build.ps1
```

Le script va :
1. Copier le HTML, CSS et JS dans `ui/`
2. Compiler avec `cargo tauri build --no-bundle --target x86_64-pc-windows-msvc`
3. Produire `dist/Rose-Setup.exe` + `dist/payload/`

## Résultat

- `dist/Rose-Setup.exe` : l'installeur graphique (WebView2)
- `dist/payload/rose.exe` : le moteur Rose embarqué
- Les deux doivent rester côte à côte

## Payload

Le dossier `payload/` contient `rose.exe` (le moteur Rose compilé pour Windows).
Lors de l'installation, il est copié vers le dossier choisi par l'utilisateur.

## Problèmes connus

- `cargo tauri build` nécessite la CLI Tauri installée globalement
- Le linker MSVC (`link.exe`) est requis pour produire un EXE Windows
- WebView2 Runtime doit être installé sur la machine cible (déjà présent sur Win10/11 récentes)
