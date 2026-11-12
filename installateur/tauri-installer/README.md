# Installeur HTML en Tauri

Ce dossier contient le squelette d'un exécutable Windows x64 qui affiche une
interface HTML avec Tauri. Le build vise un EXE autonome côté interface, sans
générer de package NSIS ou MSI.

## Ce qui est préparé

- Par défaut, le build embarque `..\exe.html`.
- La fenêtre Tauri est sans barre native et démarre maximisée, mais pas en plein
  écran exclusif. Les barres, la barre des tâches et les boutons réduire/agrandir
  dessinés dans le HTML restent donc visibles et gardent leur comportement.
- Une feuille `theme.css` placée à côté du HTML est copiée si elle existe. Le
  HTML doit la référencer. `installer.js` et `installer.json` voisins sont aussi
  copiés s'ils existent.
- Le backend Rust de [main.rs](./src-tauri/src/main.rs) expose `installer_info`
  et `install_app`. Cette dernière commande copie le contenu de `payload` vers
  le dossier utilisateur choisi, sans demander les droits administrateur.
- [build.ps1](./build.ps1) prépare le HTML puis compile la cible
  `x86_64-pc-windows-msvc`.

## Limites actuelles

Le [exe.html](../exe.html) fourni est un **simulateur** : ses écrans de
progression et de fin ne déclenchent pas la commande Rust. L'EXE affichera donc
la maquette, mais n'installera pas encore Rose. Il faut relier les actions HTML
à `window.__TAURI__.core.invoke("install_app", { destination: ... })` pour
activer la copie du payload.

La modification du PATH, les raccourcis, les associations de fichiers et la
désinstallation ne sont pas implémentés. Le build produit l'application
d'installation, pas encore un installeur Rose complet.

## Prérequis et build

Sur Windows, installer Visual Studio C++ Build Tools et le Windows SDK, le
toolchain Rust MSVC, le runtime WebView2, ainsi que la CLI Tauri. Installer la
CLI avec le toolchain MSVC :

```powershell
rustup run stable-x86_64-pc-windows-msvc cargo install tauri-cli --version "^2"
```

Depuis ce dossier, lancer :

```powershell
.\build.ps1
```

Pour choisir un autre HTML et sa feuille de thème :

```powershell
.\build.ps1 -HtmlPath .\ui\index.html -ThemePath .\ui\theme.css
```

Le résultat attendu est `dist\HtmlInstaller.exe` accompagné de
`dist\payload\` : les deux doivent rester côte à côte. Le HTML est intégré à
l'EXE ; les fichiers à installer sont fournis séparément dans `payload`.

## État du build dans cet environnement

Aucun EXE n'a été généré ici : la CLI Tauri (`cargo-tauri`) et le linker MSVC
(`link.exe`) ne sont pas disponibles, et l'exécution du script PowerShell a été
bloquée par l'AuthorizationManager.
