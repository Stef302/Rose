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
ovsx publish rose-language-5.10.1.vsix -p <TOKEN>
```
