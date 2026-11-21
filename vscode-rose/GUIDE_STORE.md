# Publier l'extension Rose sur le Visual Studio Marketplace

## 1. Prérequis
- Compte Microsoft / Azure DevOps.
- Publisher `stef302` sur https://marketplace.visualstudio.com/manage
- `npm install -g @vscode/vsce`
- PAT Azure DevOps scope Marketplace (Manage)

## 2. Publication
```bash
cd vscode-rose
vsce package
vsce login stef302
vsce publish
```

## 3. Open VSX
```bash
npm install -g ovsx
ovsx publish rose-langage-0.1.72.vsix -p <TOKEN>
```

## 4. État courant (21/11/2026 — republié)

- Version **0.1.72** : `rose-langage-0.1.72.vsix` (73 623 o, SHA-256
  `1e1bbfe586190bc53a9a32dba143c05f2536800dbf753da54c6b40e96023494f`).
  Ce hash est identique à celui du VSIX téléchargé depuis le marketplace.
- Titre marketplace neutre, **sans numéro de version** :
  « Rose — langage de programmation ». Le titre initialement souhaité
  « Rose — langage de programmation en français » a été refusé par le
  marketplace (« This extension display name is taken ») : il reste réservé
  par l'ancienne extension `stef302.rose-language` dépubliée.
- Description générale (syntaxe française, types exacts BigInt/rationnels,
  JIT/AOT, auto-hébergement, GUI native, WASM, outils VS Code) ; les
  factorielles ne sont plus qu'un exemple dans le README.
- Deux thèmes : **Rose Dark — types rouges** et **Rose Noir — Lime & Rouge**
  (fond `#0A0A0F`, accents lime `#BFFF00` et rouge `#FF3B3B`).
- Vérification après installation : palette de commandes → « Extensions:
  Show Installed Extensions » → Rose doit afficher **0.1.72**. Les deux
  thèmes se sélectionnent avec `Ctrl+K Ctrl+T`.

## 5. Historique / publication marketplace (21/11/2026)

- Le PAT Azure DevOps a permis `vsce publish` de `stef302.rose-language` **v0.1.7**.
- **Incident** : pour retirer les anciennes versions `5.10.x` (numériquement supérieures à `0.1.7`, donc affichées comme « latest »), l'extension a été dépubliée avec `vsce unpublish --force`. Le nom `rose-language` est depuis **réservé** par le marketplace : `vsce publish` refuse de le réutiliser (« already exists »), malgré une requête d'extensionquery qui ne renvoie plus rien.
- **Contournement publié** : nouvelle extension `stef302.rose-langage`, d'abord affichée « Rose — langage (édition 0.1.7) », version **0.1.7**.
- **Correction 0.1.72** : republiée sous `stef302.rose-langage` avec titre neutre, description générale et le second thème. Commande :
  `vsce publish --packagePath rose-langage-0.1.72.vsix -p <PAT>` → « DONE Published stef302.rose-langage v0.1.72. »
- Preuve : `GET https://marketplace.visualstudio.com/_apis/public/gallery/publishers/stef302/vsextensions/rose-langage/0.1.72/vspackage` renvoie un VSIX dont le SHA-256 est identique au fichier local. (La page HTML et `vsce show` peuvent rester en cache sur l'ancienne version quelques minutes : se fier au VSIX téléchargé.)
- L'ancien identifiant `stef302.rose-language` reste non disponible ; le VSIX 0.1.7 reste téléchargeable depuis le site et la release GitHub si l'utilisateur préfère l'installation manuelle.
- Le PAT est mémorisé dans `~/.config/opencode/.azdo_token` (600), jamais affiché.
