# EXTENSION2 — extension VS Code neutre, thème noir/lime/rouge, republication

Chantier EX1 de la vague 72 (21/11/2026). Fichiers possédés :
`vscode-rose/**` et `rose/docs/EXTENSION2.md`. Aucun commit, aucune
publication hors marketplace.

## 1. Titre neutre — CORRIGÉ

- État initial sur le marketplace (`stef302.rose-langage`) :
  `displayName = "Rose — langage (édition 0.1.7)"` (vérifié par
  `extensionquery` : `displayName`, `shortDescription`, `versions`).
- Titre souhaité littéralement : « Rose — langage de programmation en français ».
  **Refusé par le marketplace** : `ERROR This extension display name is taken.`
  Ce titre reste réservé par l'ancienne extension `stef302.rose-language`
  dépubliée (conflit documenté dans `vscode-rose/GUIDE_STORE.md`).
- Titre publié retenu : **« Rose — langage de programmation »** — neutre,
  **aucun numéro de version**. La version n'apparaît que dans le champ
  `version` (`0.1.72`).

## 2. Description générale — RÉÉCRITE

`description` (192 caractères, sous la limite marketplace de 200) :

> Langage en français : syntaxe française, types exacts (BigInt, rationnels),
> compilation JIT/AOT, auto-hébergement, GUI native, WASM, outils VS Code
> (coloration, diagnostics, exécution, LaTeX).

`vscode-rose/README.md` réécrit dans le même esprit : présentation générale
du langage d'abord, factorielles reléguées au rang d'exemple parmi d'autres
dans un tableau, ajout des deux thèmes et des commandes.

## 3. Thèmes — DEUX DÉCLARÉS

`contributes.themes` contient désormais :

| id | label | fichier |
|---|---|---|
| `rose-dark` | Rose Dark — types rouges | `themes/rose-dark.json` (inchangé) |
| `rose-noir` | **Rose Noir — Lime & Rouge** | `themes/rose-noir.json` (nouveau) |

Nouveau thème : fond `editor.background = #0A0A0F`, accents lime `#BFFF00`
(curseur, mots-clés, focus, barre d'état, boutons, liens), rouge vif
`#FF3B3B` (types Rose en gras, opérateurs, bordures actives). 7 règles
`tokenColors`, 28 couleurs d'interface.

Validation JSON : `package.json`, les deux thèmes, snippets, grammaire et
configuration sont valides. (`rose-icons.json` porte un BOM UTF-8
préexistant, lu correctement avec `utf-8-sig` ; inchangé.)

## 4. VSIX — CONSTRUIT ET VÉRIFIÉ

- Fichier : `vscode-rose/rose-langage-0.1.72.vsix`
- Taille : **73 623 octets**
- SHA-256 : **`1e1bbfe586190bc53a9a32dba143c05f2536800dbf753da54c6b40e96023494f`**
- `vsce package` : OK — 16 fichiers, dont les deux thèmes et le README réécrit.
- Contenu vérifié dans le ZIP :
  - `extension/package.json` → `name=rose-langage`, `publisher=stef302`,
    `version=0.1.72`, titre neutre, description générale, 2 thèmes ;
  - `extension/themes/rose-dark.json` et `extension/themes/rose-noir.json`
    présents et JSON-valides ;
  - `extension/readme.md` → concepts généraux + Rose Noir présents.

## 5. PUBLICATION — RÉUSSIE

Commande (PAT lu depuis `~/.config/opencode/.azdo_token`, 600, jamais affiché) :

```bash
cd vscode-rose
PAT="$(tr -d '\r\n' < ~/.config/opencode/.azdo_token)"
vsce publish --packagePath rose-langage-0.1.72.vsix -p "$PAT"
```

Sortie :

```
INFO  Publishing 'stef302.rose-langage v0.1.72'...
INFO  Extension URL: https://marketplace.visualstudio.com/items?itemName=stef302.rose-langage
DONE  Published stef302.rose-langage v0.1.72.
```

Preuve indépendante : le VSIX téléchargé depuis le marketplace

```
https://marketplace.visualstudio.com/_apis/public/gallery/publishers/stef302/vsextensions/rose-langage/0.1.72/vspackage
```

( gzip ; décompressé ) a le **même SHA-256** que le fichier local, et son
`package.json` contient le titre neutre, la description générale et les deux
thèmes. La page HTML du marketplace affiche aussi le nouveau titre et la
nouvelle description, sans « édition 0.1.7 ».

Note : `vsce show stef302.rose-langage` et l'endpoint `extensionquery`
peuvent continuer à renvoyer le vieux titre/0.1.7 pendant quelques minutes
(cache des réplicas de lecture) ; la preuve retenue est le VSIX 0.1.72
téléchargé et son hash.

## État final

- [x] Titre neutre sans numéro de version (variante disponible la plus proche)
- [x] Description générale ; factorielles = exemple parmi d'autres ; README à jour
- [x] Second thème « Rose Noir — Lime & Rouge », deux thèmes déclarés, JSON valides
- [x] `vsce package` réussi, VSIX vérifié (taille + hash + contenu)
- [x] Publication `stef302.rose-langage` v0.1.72 réussie et prouvée par téléchargement
