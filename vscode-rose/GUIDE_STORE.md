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
ovsx publish rose-language-0.1.7.vsix -p <TOKEN>
```

## 4. État courant (21/11/2026)

- Version **0.1.7** : `rose-language-0.1.7.vsix` (72 399 o, SHA-256 `44848415bd5f…83a2`).
- Disponible sur le site Rose et dans la release GitHub v0.1.7.
- **Le marketplace n'a pas été republié** par ce chantier : il manque un PAT Azure DevOps (scope Marketplace Manage). Tant que ce n'est pas fait, VS Code peut continuer à afficher l'ancienne version (0.1.3) — installer le `.vsix` depuis le site pour avoir 0.1.7.
- Vérification après installation : palette de commandes → « Extensions: Show Installed Extensions » → Rose doit afficher **0.1.7**. Le thème « Rose Dark — types rouges » se sélectionne avec `Ctrl+K Ctrl+T`.
