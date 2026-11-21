# Promotion de l'extension VS Code après le téléchargement de Rose

Chantier du 21/11/2026. Objectif : après le téléchargement de ROSE, proposer
l'extension **Rose Language Support** (v0.1.7) à l'utilisateur, sur le site et
dans l'installeur Windows. Aucun commit ni publication n'a été fait ici :
l'orchestrateur publie le site.

## 1. Où sont les liens

| Emplacement | Contenu |
|---|---|
| `docs/index.html`, carte `#extension-vscode` (section `#installer`, juste après les liens de téléchargement) | Bouton **INSTALLER DEPUIS LE MARKETPLACE ↗** + bouton **TÉLÉCHARGER LE .VSIX · 72 Ko**, description (coloration, complétion, diagnostics, exécution, LaTeX), thème « Rose Dark — types rouges », aperçu et astuce post-install. |
| `installateur/velopack/PROMO_EXTENSION.md` | Message texte « Astuce : installez l'extension Rose pour VS Code » + lien Marketplace, variante courte, VSIX de secours, et consigne d'intégration UI (hors périmètre). |
| `installateur/velopack/README.md` | Renvoi vers ce message post-install. |
| `vscode-rose/GUIDE_STORE.md` | État réel de la publication : l'identifiant publié est **`stef302.rose-langage`** (édition 0.1.7). L'ancien `stef302.rose-language` a été dépublié puis réservé par le Marketplace — ne pas l'utiliser. |

Liens utilisés :

- Marketplace : <https://marketplace.visualstudio.com/items?itemName=stef302.rose-langage>
- VSIX hébergé sur le site : <https://stef302.github.io/Rose/telechargements/rose-language-0.1.7.vsix>
  (fichier source : `docs/telechargements/rose-language-0.1.7.vsix`,
  SHA-256 `44848415bd5f7e99407a37936236cc8fcddc0870619e2cbf1ac53235105c83a2`)
- Empreintes : <https://stef302.github.io/Rose/telechargements/SHA256SUMS>
- Installation manuelle : `code --install-extension rose-language-0.1.7.vsix`

## 2. Comment vérifier que les liens répondent (curl, 21/11/2026)

```bash
curl -s -o /dev/null -w '%{http_code} %{size_download}o\n' -L --max-time 20 \
  'https://marketplace.visualstudio.com/items?itemName=stef302.rose-langage'
curl -s -o /dev/null -w '%{http_code} %{size_download}o\n' -L --max-time 20 \
  'https://stef302.github.io/Rose/telechargements/rose-language-0.1.7.vsix'
curl -s -o /dev/null -w '%{http_code} %{size_download}o\n' -L --max-time 20 \
  'https://stef302.github.io/Rose/telechargements/SHA256SUMS'
```

Résultats mesurés : Marketplace **200** (page dynamique : ~78 477–78 478 o selon
la réponse), VSIX **200 / 72 399 o**, SHA256SUMS **200 / 1 342 o**.

Deux précautions :

- Une page Marketplace est une application monopage : un **200 ne prouve pas à lui
  seul que l'extension existe** (la coquille répond même pour un identifiant
  inconnu). Vérifier aussi le contenu, par exemple
  `curl -s -L '<url>' | grep -oiE 'rose-langage' | wc -l` → 14 occurrences
  mesurées, et/ou une requête `extensionquery` (voir `vscode-rose/GUIDE_STORE.md`,
  § 5 : l'édition `stef302.rose-langage` 0.1.7 y est vérifiée).
- Le VSIX doit être comparé à l'empreinte du `SHA256SUMS` publié, pas seulement
  au code HTTP : `curl -s '<url_vsix>' | sha256sum` doit rendre
  `44848415…c83a2`.

Contrôle du HTML après modification (section ajoutée sans toucher aux autres) :

```bash
python3 -c "from html.parser import HTMLParser; import pathlib; p=HTMLParser(); p.feed(pathlib.Path('docs/index.html').read_text()); print('HTML analysé sans erreur')"
grep -c 'id="extension-vscode"' docs/index.html   # → 1
grep -c '<section class="page"' docs/index.html       # → 6 (inchangé avant/après)
grep -c 'section class="page active"' docs/index.html # → 1 (la page d'accueil)
```

## 3. Ce qui reste à intégrer dans le launcher (hors périmètre)

Le message post-install n'est **pas** encore affiché par l'installeur : le texte
des écrans Vitesse/Velopack est codé en dur dans `rose/src/setup_gui.rs`, fichier
interdit à ce chantier. Reste à faire par l'orchestrateur :

1. Sur l'écran final (« Terminé ») de `setup_gui.rs`, ajouter le libellé de
   `installateur/velopack/PROMO_EXTENSION.md` et un lien cliquable vers le
   Marketplace (`ShellExecuteW`, ou bouton « OUVRIR LE MARKETPLACE »).
2. Prévoir la variante courte si la fenêtre 1120 × 780 manque de place, puis
   revérifier les géométrie/libellés avec
   `python3 installateur/velopack/previsualiser.py`.
3. Reconstruire l'installeur (`sh installateur/velopack/build-linux.sh`,
   `sh installateur/velopack/pack-linux.sh`) et publier depuis le profil GitHub
   **Stef302** — jamais `clausake`.

## Périmètre de ce chantier

Modifiés : `docs/index.html` (une carte ajoutée), `installateur/velopack/README.md`,
`installateur/velopack/PROMO_EXTENSION.md` (nouveau), `rose/docs/PROMO.md` (nouveau).
Non touchés : `rose/src/**`, `compilateur/**`, `vscode-rose/**`, `PLAN.md`,
ainsi que les sections RELEASES, CHANGELOG et Documentation du site.
