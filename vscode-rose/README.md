# Rose Language Support for VS Code

Prise en charge du langage **Rose** (v5.10) dans Visual Studio Code :

- coloration syntaxique complète (mots-clés, types, fonctions standard, opérateurs, chaînes, nombres, commentaires) ;
- snippets pour les structures de contrôle, fonctions, tables, listes, FFI ;
- thème d'icônes Rose ;
- ligatures automatiques (optionnelles) : `<=` `>=` `!=` `=~` `_^` ;
- survol et complétion pour les mots-clés, types et fonctions standard ;
- commandes : exécuter le fichier, générer le LaTeX, ouvrir un terminal Rose.

## Configuration

- `rose.binaryPath` : chemin vers le binaire `rose` (défaut `rose`).
- `rose.ligatures` : active les ligatures automatiques (défaut `true`).
- `rose.runInTerminal` : exécute dans un terminal intégré (défaut `true`).

## Raccourcis

- `Ctrl+Alt+R` (macOS `Cmd+Alt+R`) : exécuter le fichier actif.
- `Ctrl+Alt+L` (macOS `Cmd+Alt+L`) : générer le LaTeX du fichier actif.

## Installation

Voir `GUIDE_STORE.md` pour la publication sur le Marketplace.
