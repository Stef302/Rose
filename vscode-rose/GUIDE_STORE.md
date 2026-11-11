# Publier l'extension Rose sur le Visual Studio Marketplace

## 1. Prérequis

- Compte Microsoft / Azure DevOps.
- Un **publisher** créé sur https://marketplace.visualstudio.com/manage (ex. `stef302`).
- Node.js + `vsce` : `npm install -g @vscode/vsce`.
- Un PAT (Personal Access Token) Azure DevOps avec le scope **Marketplace (Manage)**.

## 2. Vérifications locales

```bash
cd vscode-rose
vsce package                  # produit rose-language-5.10.1.vsix
code --install-extension rose-language-5.10.1.vsix   # test local
```

## 3. Connexion au Marketplace

```bash
vsce login stef302
vsce ls-publishers
```

## 4. Publication

```bash
vsce publish
```

## 5. Open VSX (VSCodium, Cursor, Gitpod…)

```bash
npm install -g ovsx
ovsx publish rose-language-5.10.1.vsix -p <OVSX_TOKEN>
```

## 6. Checklist

- [ ] `vsce package` sans erreur.
- [ ] Installation locale OK.
- [ ] `icon` = logo_exe.png (256×256).
- [ ] `repository.url` valide.
- [ ] `LICENSE` présent.
- [ ] README.md à jour.
EOF