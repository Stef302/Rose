# Extension VS Code — Rose Language Support

Version de l'extension : **0.1.7** (`vscode-rose/package.json`,
`vscode-rose/README.md`, VSIX `rose-language-0.1.7.vsix`).
Le moteur Rose porte aussi le numéro `0.1.7` (`rose/Cargo.toml`).

## Installation

Trois voies, du plus local au plus distant :

1. **Depuis le VSIX local** (aucun compte requis) :
   `code --install-extension vscode-rose/rose-language-0.1.7.vsix`
   (ou, dans VS Code : *Extensions* → `...` → *Install from VSIX…*).
2. **Depuis le Marketplace** : chercher `Rose — langage de programmation en
   français`, éditeur `stef302`, puis installer.
3. **Depuis Open VSX** (VSCodium) : chercher le même éditeur/extension.

Le fichier `.rose` est alors associé au langage `rose` : coloration, snippets,
diagnostics, complétion, survol, définition, symboles, signature, formatage,
pliure et commandes `Rose : …`. Voir `vscode-rose/README.md` pour la liste
complète des commandes et raccourcis.

## Nouveautés de la 0.1.7

- **Thème sombre intégré `Rose Dark — types rouges`** : les types Rose restent
  en rouge vif.
- **Primitives récentes ajoutées à la coloration et à la complétion**
  (noms exacts relevés dans `rose/src/interp.rs`, fonction `nom_std_connu`) :
  - `entier_geant_factorielle(n)` — entier géant paresseux, factorielle
    symbolique de `n` (`n ≥ 0`) ;
  - `detail(x)` — table des propriétés calculées selon le type de `x` ;
  - `afficher_detail(detail(x))` — rendu multi-lignes lisible de cette table ;
  - `chiffres(expr, n = 30)` — les `n` premiers chiffres décimaux, en chaîne ;
  - `plus_de_chiffres(expr, n = 30)` — demande `n` chiffres supplémentaires à
    un entier géant symbolique.
- Mise à jour des marqueurs de version dans `package.json` (`version` et
  `description`) et `README.md`.

`aot` n'a **pas** été ajouté comme primitive : c'est un mécanisme interne
(`essayer_aot` dans `interp.rs`) déclenché automatiquement, et aucun appel
`aot(...)` ni `@aot` n'existe dans les sources `.rose` du dépôt. Les mots-clés
Thorn (`théorème`, `preuve`, `pour_tout`, `existe`, `implique`, `induction`,
`cas`, `soit`, `montrer`, `par`, `sur`, …) n'ont **pas** été ajoutés non plus :
Thorn est un langage distinct (fichiers `.thn`, lexeur propre dans
`thorn/src/lexeur.rose`) ; plusieurs de ses mots (`type`, `fn`, `et`, `ou`,
`non`, `vrai`, `faux`) sont déjà des mots-clés Rose et les ajouter au scope
`source.rose` produirait des colorations fausses dans les fichiers `.rose`.

## Le thème rouge

Le scope des types dans la grammaire est `storage.type.rose` ; les types nommés
utilisent `entity.name.type.rose`. L'extension 0.1.5 ne fournissait **aucun**
thème (`contributes.themes` absent) : le rendu dépendait entièrement du thème
VS Code de l'utilisateur, et de nombreux thèmes affichent ces scopes en bleu
marine. La 0.1.7 fournit `vscode-rose/themes/rose-dark.json`, déclaré dans
`contributes.themes` sous l'identifiant `rose-dark`, libellé
**« Rose Dark — types rouges »** :

- `storage.type.rose` et `entity.name.type.rose` → `#FF4D4D` (rouge vif) ;
- fond éditeur `#14141A` ;
- contraste WCAG calculé `#FF4D4D` sur `#14141A` : **5,61:1** (au-dessus du
  seuil AA 4,5:1 pour du texte normal).

Activation : `Ctrl+K Ctrl+T` (macOS `Cmd+K Cmd+T`) ou Palette de commandes →
*Preferences: Color Theme*, puis sélectionner **Rose Dark — types rouges**.

**Limitation** : le thème ne s'applique **que si l'utilisateur le sélectionne**.
L'extension ne modifie pas le thème actif et ne peut pas imposer une couleur
de token : avec un autre thème, les types reprennent la couleur définie par ce
thème. Le rouge `#FF4D4D` est garanti uniquement avec « Rose Dark — types
rouges » actif.

## Vérifications effectuées (2026-11-20)

- `python3 -m json.tool` : `package.json`, `syntaxes/rose.tmLanguage.json` et
  `themes/rose-dark.json` = JSON valides. (`rose-icons.json`, préexistant et
  non modifié, contient un BOM UTF-8.)
- `node --check extension.js` : OK.
- `vsce ls` : 13 fichiers, dont `themes/rose-dark.json`.
- `vsce package` : **OK**, `rose-language-0.1.7.vsix`, 72 399 octets,
  SHA-256 `44848415bd5f7e99407a37936236cc8fcddc0870619e2cbf1ac53235105c83a2`.
  Archive ZIP valide (15 entrées, préfixe `extension/`).
- `cargo test --release` : **529 réussis / 0 échec / 6 ignorés**, rc=0. Le
  chiffre `517` cité dans certaines consignes est périmé : l'arbre courant en
  compte 529 (valeur déjà relevée dans `rose/docs/PUBLICATION_ROSE.md`).
- Point fixe `b3ed9262` ×2, via `./rose/target/release/rose` :
  `rir compilateur/emetteur.rose` → 263 520 octets, md5
  `b3ed9262dd54776a6dbd07bc12368d0e` ; et
  `compilateur/emetteur.rose compilateur/emetteur.rose` → 263 520 octets,
  md5 identique.
- Périmètre de cette session : seuls `vscode-rose/**` et ce fichier ont été
  modifiés ; **aucun commit ni publication n'a été effectué par cette session**.
  À titre de constat, une autre session a commité les fichiers `vscode-rose/**`
  (`534c936`) puis publié le VSIX dans `docs/telechargements/` (`807c741`) ;
  le hash du fichier publié est identique à celui produit ici
  (`44848415bd5f7e99407a37936236cc8fcddc0870619e2cbf1ac53235105c83a2`).
  Aucun fichier du site n'a été modifié par cette session.
