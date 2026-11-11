# Publier l'extension Rose sur le Visual Studio Marketplace

## 1. Prérequis

- Compte Microsoft / Azure DevOps.
- Un **publisher** créé sur https://marketplace.visualstudio.com/manage (ex. `stef302`).
- Node.js + `vsce` : `npm install -g @vscode/vsce`.
- Un PAT (Personal Access Token) Azure DevOps avec le scope **Marketplace (Manage)**.

## 2. Vérifications locales

```bash
cd vscode-rose
npm install -g @vscode/vsce   # si pas déjà fait
vsce package                  # produit rose-language-5.10.0.vsix
code --install-extension rose-language-5.10.0.vsix   # test local
```

## 3. Connexion au Marketplace

```bash
vsce login stef302
# coller le PAT quand demandé
vsce ls-publishers           # vérifie l'authentification
```

## 4. Publication

```bash
# depuis vscode-rose/
vsce publish                 # incrémente patch automatiquement si version déjà prise
# ou version explicite :
vsce publish 5.10.0
```

Le Marketplace détecte `package.json` (name, displayName, description, icon, repository, license).

## 5. Open VSX (VSCodium, Cursor, Gitpod…)

```bash
npm install -g ovsx
ovsx create-namespace stef302   # une fois
ovsx publish rose-language-5.10.0.vsix -p <OVSX_TOKEN>
```

Token Open VSX : https://open-vsx.org/user-settings/tokens

## 6. Mise à jour

1. Modifier le code / la grammaire.
2. Bumper `version` dans `package.json`.
3. `vsce package` puis `vsce publish` (ou `ovsx publish`).

## 7. Checklist avant publication

- [ ] `vsce package` sans erreur.
- [ ] Installation locale OK, coloration visible sur un `.rose`.
- [ ] `icon` pointe vers un PNG ≥ 128×128.
- [ ] `repository.url` valide et public.
- [ ] `LICENSE` présent.
- [ ] README.md à jour.
- [ ] Pas de secrets / chemins absolus dans le paquet.

## 8. Dépannage

- Erreur « Missing publisher name » : renseigner `publisher` dans `package.json`.
- Erreur « README.md not found » : vsce attend un README à la racine (présent).
- Erreur PAT : régénérer le token avec scope Marketplace Manage.
- Signature : `vsce` peut demander `--allow-star-activation` ; éviter les `*` dans `activationEvents`.
EOF