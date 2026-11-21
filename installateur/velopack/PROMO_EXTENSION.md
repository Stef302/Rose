# Message post-install — extension VS Code (Rose)

Ce fichier porte le **message de promotion de l'extension VS Code** à afficher à la
fin de l'installation Windows (installeur Velopack / `Rose-win-Setup.exe`), après le
téléchargement de Rose.

## Message à afficher (texte exact, en français)

```
Astuce : installez l'extension Rose pour VS Code
Coloration, complétion, diagnostics, exécution et rendu LaTeX pour vos .rose.
https://marketplace.visualstudio.com/items?itemName=stef302.rose-langage
```

Lien Marketplace : <https://marketplace.visualstudio.com/items?itemName=stef302.rose-langage>

Variante courte, si l'espace manque dans la fenêtre :

```
Astuce : installez l'extension Rose pour VS Code — https://marketplace.visualstudio.com/items?itemName=stef302.rose-langage
```

Une seconde voie, sans compte Microsoft, est le VSIX hébergé sur le site :
`https://stef302.github.io/Rose/telechargements/rose-language-0.1.7.vsix`
(installation manuelle : `code --install-extension rose-language-0.1.7.vsix`).

## Pourquoi ce fichier et pas l'interface

Le texte des écrans de l'installeur est codé en dur dans
`rose/src/setup_gui.rs` (fenêtre Win32 GDI : écran « Terminé », boutons et
libellés). Ce fichier est **hors périmètre de ce chantier** (accès interdit) :
l'intégration UI du message post-install doit être faite par l'orchestrateur.

À faire par l'orchestrateur dans `setup_gui.rs` : sur l'écran final (« Terminé »),
ajouter un contrôle texte avec le libellé ci-dessus et un lien cliquable
(`ShellExecuteW` avec le URL Marketplace, ou un bouton « OUVRIR LE MARKETPLACE »).
Les géométries et libellés se vérifient avec
`python3 installateur/velopack/previsualiser.py` (grille 1120 × 780, couleurs GDI).

## Vérification du lien

```bash
curl -s -o /dev/null -w '%{http_code}\n' -L \
  'https://marketplace.visualstudio.com/items?itemName=stef302.rose-langage'
# → 200
```

Voir aussi `rose/docs/PROMO.md` (liens du site, vérifications et reste à intégrer)
et `vscode-rose/GUIDE_STORE.md` pour l'état réel de la publication Marketplace.
