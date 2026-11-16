# Rose Language Support for VS Code

Prise en charge du langage **Rose** (v0.1.5) dans Visual Studio Code :

- coloration syntaxique complète (mots-clés, types, fonctions standard,
  opérateurs, chaînes, nombres, commentaires) ;
- notations factorielles étendues colorées : `n!!`, `n!!!`, `!n` (préfixe),
  `n#`, `n?`, `n$`, ainsi que les fonctions `multifactorielle`,
  `sous_factorielle`, `derangement`, `factorial_approx` ;
- les lignes commençant par `%%` sont colorées comme annotations LaTeX
  (commentaires TeX, ignorés par le lexer) ;
- snippets pour les structures de contrôle, fonctions, tables, listes, FFI,
  et les factorielles étendues ;
- thème d'icônes Rose ;
- ligatures automatiques (optionnelles) : `<=` `>=` `!=` `=~` `_^` ;
- survol et complétion pour les mots-clés, types et fonctions standard ;
- commandes : exécuter le fichier, générer le LaTeX **complet**, générer le
  LaTeX des **lignes marquées `%%`**, ouvrir un terminal Rose.

## Les factorielles en Rose

| Écriture | Sens | Exactitude |
|---|---|---|
| `n!` | factorielle ordinaire | exacte |
| `n!!` `n!!!` | multifactorielle d'ordre k (k = nb de `!`) — `7!!` = 105, **pas** `(7!)!` | exacte |
| `!n` | sous-factorielle (dérangements, A000166) — `!5` = 44 | exacte |
| `n#` | primorielle (premiers ≤ n) | exacte |
| `n?` | terminal (Kurepa, somme des factorielles) | exacte |
| `n$` | super-factorielle (produit des factorielles) | exacte |
| `factorial_approx(n)` | Stirling √(2πn)(n/e)^n — donne la **taille**, pas le nombre | approximative, O(1) |

## Rendu LaTeX par ligne

Une ligne qui commence par `%%` est une annotation TeX :

```rose
%% $N!$ : la factorielle
$N = 100
%% $N!!$ : la multifactorielle, PAS $(N!)!$
>> $N!!
```

Le programme reste exécutable tel quel. Deux commandes :

- **Rose : générer le LaTeX du fichier** → `rose latex fichier.rose`
  (tout le programme) ;
- **Rose : générer le LaTeX des lignes marquées %%** →
  `rose latex fichier.rose --lignes` (seulement les lignes `%%`).

## Configuration

- `rose.binaryPath` : chemin vers le binaire `rose` (défaut `rose`).
- `rose.ligatures` : active les ligatures automatiques (défaut `true`).
- `rose.runInTerminal` : exécute dans un terminal intégré (défaut `true`).
- `rose.latexMarker` : marqueur de ligne LaTeX (défaut `%%`).

## Raccourcis

- `Ctrl+Alt+R` (macOS `Cmd+Alt+R`) : exécuter le fichier actif.
- `Ctrl+Alt+L` (macOS `Cmd+Alt+L`) : générer le LaTeX complet du fichier.
- `Ctrl+Alt+Shift+L` (macOS `Cmd+Alt+Shift+L`) : générer le LaTeX des lignes `%%`.
- `Ctrl+Alt+V` : valider le fichier.
- `Ctrl+Alt+C` : exécuter puis ouvrir une console.

## Installation

Voir `GUIDE_STORE.md` pour la publication sur le Marketplace.
