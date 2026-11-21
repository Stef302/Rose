# Rose — prise en charge du langage dans VS Code

Extension Visual Studio Code pour **Rose**, un langage de programmation en
français. Rose mise sur une syntaxe lisible en français, des types exacts
(entiers géants `BigInt`, rationnels, nombres complexes, constantes
irrationnelles), une compilation JIT (Cranelift) et AOT (LLVM ORC), une sortie
LaTeX automatique, l'auto-hébergement, une interface graphique native
(GUI native), une cible WASM et des outils VS Code complets.

## Ce que l'extension apporte

- **Coloration syntaxique** complète : mots-clés, types, fonctions standard,
  opérateurs, chaînes, nombres et commentaires.
- **Deux thèmes sombres** intégrés :
  - **Rose Dark — types rouges** : types Rose en rouge vif `#FF4D4D` sur fond
    `#14141A` ;
  - **Rose Noir — Lime & Rouge** : fond noir profond `#0A0A0F`, accents lime
    `#BFFF00` et rouge vif `#FF3B3B`.
- **Snippets** pour les structures de contrôle, fonctions, tables, listes, FFI
  et les notations numériques étendues.
- **Thème d'icônes** Rose.
- **Ligatures automatiques** (optionnelles) : `<=` `>=` `!=` `=~` `_^`.
- **Survol et complétion** pour les mots-clés, types et fonctions standard.
- **Diagnostics en temps réel**, navigation et formatage.
- **Exécution** du fichier actif dans un terminal intégré.
- **Rendu LaTeX** : du programme entier ou des seules lignes marquées `%%`.
- **Commandes** : exécuter le fichier, générer le LaTeX complet, générer le
  LaTeX des lignes marquées `%%`, ouvrir un terminal Rose, valider le fichier.

## Le langage en un exemple

```rose
$N = 100
>> $N
```

Rose reconnaît aussi bien les entiers géants que les rationnels, les complexes
ou les constantes symboliques ; le même programme peut être interprété, compilé
à la volée (JIT), compilé en natif (AOT), exporté en WASM ou rendu en LaTeX.

Les factorielles ne sont qu'un exemple parmi d'autres notations prises en
charge :

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

## Thèmes

Le scope des types Rose est `storage.type.rose` (et `entity.name.type.rose`
pour les types nommés), ce qui permet aux thèmes de les colorer
spécifiquement. L'extension fournit deux thèmes :

- **Rose Dark — types rouges** (`themes/rose-dark.json`) : types `#FF4D4D`,
  mots-clés violets, fond `#14141A` (contraste WCAG 5,61:1).
- **Rose Noir — Lime & Rouge** (`themes/rose-noir.json`) : fond noir profond
  `#0A0A0F`, types et opérateurs rouge vif `#FF3B3B`, mots-clés et accents
  lime `#BFFF00`.

Activation : `Ctrl+K Ctrl+T` (Palette de commandes → *Preferences: Color
Theme*), puis choisir l'un des deux thèmes. L'extension ne force pas le thème
de l'utilisateur.

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

Depuis le marketplace : `stef302.rose-langage`. Sinon, installer le `.vsix`
depuis le site Rose ou la release GitHub. Voir `GUIDE_STORE.md` pour la
publication sur le Marketplace.
