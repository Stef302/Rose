# Rose Language Support for VS Code

Prise en charge du langage **Rose** (v0.1.7) dans Visual Studio Code :

- coloration syntaxique complète (mots-clés, types, fonctions standard,
  opérateurs, chaînes, nombres, commentaires) ;
- thème sombre intégré **Rose Dark — types rouges** : les types Rose restent
  en rouge vif `#FF4D4D` au lieu de bleu marine (voir § « Thème rouge ») ;
- primitives récentes reconnues : `entier_geant_factorielle`, `detail`,
  `chiffres`, `plus_de_chiffres`, `afficher_detail` ;
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

## Thème rouge

Le scope des types Rose est `storage.type.rose` (et `entity.name.type.rose`
pour les types nommés). Aucun thème n'était fourni : le rendu dépendait du
thème VS Code de l'utilisateur, et beaucoup affichaient les types en bleu
marine. L'extension fournit désormais le thème **Rose Dark — types rouges**
(`themes/rose-dark.json`) : les types y sont colorés en rouge vif `#FF4D4D`
sur fond sombre `#14141A` (contraste WCAG 5,61:1).

Activation : `Ctrl+K Ctrl+T` (Palette de commandes → *Preferences: Color
Theme*), puis choisir **Rose Dark — types rouges**. Le thème ne s'applique
que s'il est sélectionné : l'extension ne force pas le thème de l'utilisateur.

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
