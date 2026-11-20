# VER1 — références de version 0.1.5 → 0.1.6 (état courant vs historique)

> **Constat daté au 21/11/2026**, dépôt `Stef302/Rose`, HEAD `6e8f6db` au
> démarrage du chantier. **Aucun commit, aucune publication, aucun tag créé.**
>
> Périmètre autorisé : `README.md`, `docs/**`, `wikimiroir/**`, `vscode-rose/**`,
> et le présent fichier `rose/docs/VERSION_016.md` (nouveau).
>
> Hors périmètre, non modifiés : `rose/src/**`, `rose/tests/**`, `jardins/**`,
> `Cargo.toml`, `PLAN.md`, `scripts/**`, `installateur/**`, `.opencode/**`,
> `rose/rose-setup.manifest` et `rose/docs/*.md` autres que celui-ci.

## Règle appliquée

- **(a) État courant affiché** — badge de version, « version actuelle »,
  sélecteur, stats du site, liens de release courants, description du moteur,
  README : **doit être 0.1.6**.
- **(b) Historique daté** — changelogs, notes archivées, exemples de
  compatibilité entre versions, noms d'artefacts réellement publiés en 0.1.5,
  identité de l'extension VS Code : **reste 0.1.5**.
- En cas de doute : conserver et documenter. Aucun hash n'a été inventé.

## Preuves d'état (mesurées au 21/11/2026)

- `rose/Cargo.toml:3` (lecture seule) : `version = "0.1.6"`.
- `docs/telechargements/VERSION` : `0.1.6` (déjà à jour avant VER1, inchangé).
- `docs/telechargements/releases.win.json` : `0.1.6` (déjà à jour avant VER1,
  inchangé). Son SHA-256 déclaré `e194f979…117e9` a été recalculé sur
  `Rose-0.1.6-full.nupkg` présent : **identique**.
- Commit `b10f5ae` (19/11/2026) : publication de `Rose-0.1.6-full.nupkg`
  (2 988 307 o) et du manifeste 0.1.6 ; `SHA256SUMS` n'avait pas été resynchronisé.
- API GitHub en lecture seule, token `~/.config/opencode/.gh_token` :
  `GET /user` → **HTTP 200**, `login: Stef302` (compte conforme).
  - `GET /repos/Stef302/Rose/git/ref/tags/v0.1.6` → **HTTP 200**,
    `ref: refs/tags/v0.1.6`, objet `155ea4a26278c45f3c7092fab817c94ce04c73df`.
  - `GET /repos/Stef302/Rose/releases/tags/v0.1.6` → **HTTP 200**,
    `tag_name=v0.1.6`, `name=Rose v0.1.6 — launcher Rose`, publié le
    `2026-11-19T13:54:57Z`. Assets (7) : `releases.win.json`,
    `Rose-0.1.6-full.nupkg`, `rose-aarch64-linux`, `rose-setup.exe`,
    `rose-shell.exe`, `rose-wasm.wasm`, `rose.exe`.
  - `GET /repos/Stef302/Rose/releases/latest` → **HTTP 200**, `tag_name=v0.1.6`.
  - `GET /repos/Stef302/Rose/releases/tags/v0.1.5` → **HTTP 200**,
    `name=Rose v0.1.5 — installeur Windows refait`, 11 assets dont
    `rose-skill.zip`, `Rose-win-Setup.exe`, `rose-language-0.1.5.vsix`.
  - **Conclusion** : le lien `.../releases/tag/v0.1.6` pointe un tag réel,
    aucun lien 0.1.6 ne pointe vers un tag inexistant. Aucun VSIX 0.1.6
    n'existe dans la release 0.1.6, ni dans le dépôt.

## Tableau complet — occurrence / action / raison

Numéros de ligne **avant** modification (source : `grep -n` initial).

| Occurrence | Classe | Action | Raison |
|---|---|---|---|
| `README.md:9` badge shields.io `version-0.1.5` | (a) | **Changé** → `version-0.1.6` | Badge de version affiché = état courant. |
| `README.md:746` « `rose.exe` 0.1.5 et `rose-shell.exe` d'une autre version » | (b) | **Gardé** 0.1.5 | Exemple illustratif de désynchronisation entre deux versions, pas une annonce de version courante (mission : historique → garder). |
| `README.md:820` « ZIP publié dans la release `v0.1.5` » | (b) | **Gardé** 0.1.5 | Vérifié par API : `rose-skill.zip` est bien un asset de v0.1.5 (11 assets) et **n'est pas** dans v0.1.6 (7 assets). Écrire 0.1.6 rendrait l'affirmation fausse. |
| `docs/index.html:487` `MOTEUR NATIF — V0.1.5` | (a) | **Changé** → `V0.1.6` | Bandeau d'accueil, état courant affiché. |
| `docs/index.html:504` badge `v0.1.5` | (a) | **Changé** → `v0.1.6` | Badge de version de la page. |
| `docs/index.html:520` stats `0.1.5` | (a) | **Changé** → `0.1.6` | Statistique « LANGAGE ROSE — MOTEUR RUST » = version courante. |
| `docs/index.html:641` lien `releases/tag/v0.1.5` | (a) | **Changé** → `releases/tag/v0.1.6` | Lien de release courant ; tag vérifié HTTP 200 par API. |
| `docs/index.html:742` `<option value="15" selected>v0.1.5 — actuelle</option>` | (a) | **Changé** : ajout de `<option value="16" selected>v0.1.6 — actuelle</option>`, 0.1.5 conservé en option historique sans « actuelle » ; `setDocVer()` adapté (`ver >= 16` = actuelle, `ver === 15` = historique) | Sélecteur de version = état courant. Le filtre `data-min` (max 14) reste inchangé : les deux versions affichent tout le contenu. |
| `docs/index.html:783` « moteur (Rust v0.1.5, licence MIT) » | (a) | **Changé** → `v0.1.6` | Description du moteur courant dans la doc. |
| `docs/index.html:955` « fonctions du moteur v0.1.5 » | (a) | **Changé** → `v0.1.6` | Description du moteur courant. |
| `docs/index.html:1006` « moteur (Rust v0.1.5) » | (a) | **Changé** → `v0.1.6` | Description du moteur courant. |
| `docs/index.html:1041` `<h3>Moteur Rust (v0.1.5)</h3>` | (a) | **Changé** → `v0.1.6` | Titre de la carte du moteur courant. |
| `docs/index.html:1111` `<span class="event-ver">V0.1.5</span>` | (b) | **Gardé** 0.1.5 | Entrée du **changelog**, datée « 14 NOVEMBRE 2026 », section `HISTORIQUE`. |
| `docs/index.html:1396` `searchData` « Changelog v0.1.5 — installeur Windows refait » | (b) | **Gardé** 0.1.5 | Entrée de recherche pointant l'entrée historique ci-dessus. |
| `docs/telechargements/SHA256SUMS:3` `Rose-0.1.5-full.nupkg` (`99b0c22d…`) | (b) | **Gardé** | Fichier réellement présent (8 103 198 o), hash recalculé conforme. Checksum d'un artefact historique téléchargeable. |
| `docs/telechargements/SHA256SUMS:7` `rose-language-0.1.5.vsix` (`3f17dd0d…`) | (b) | **Gardé** | VSIX réellement présent (69 392 o), hash conforme. Aucun VSIX 0.1.6 n'existe. |
| `docs/telechargements/SHA256SUMS` (nouvelle ligne) `Rose-0.1.6-full.nupkg` | (a) | **Ajouté** `e194f9793d20547844d51c5b221a5e6854cd8d3f83570730efb3682d625117e9` | Hash **calculé par `sha256sum`** sur le fichier présent (2 988 307 o), identique à `releases.win.json`. L'artefact courant doit figurer dans la somme de contrôle. |
| `docs/telechargements/SHA256SUMS:5` `releases.win.json` (`cc2fca4b…`) | — | **Corrigé** → `ae4b6f670065b86407c3888140c87221a083e8874b0f7c8a2e53dbcf4b2a1e48` | Hash déclaré périmé : le commit `b10f5ae` a changé le manifeste en 0.1.6 sans resynchroniser `SHA256SUMS`. Nouvelle valeur calculée sur le fichier réel. Aucun hash inventé. |
| `wikimiroir/Home.md:8` « versioned as **0.1.5** » | (a) | **Changé** → `0.1.6` | Déclaration de version courante du projet. |
| `wikimiroir/CLI.md:48` titre affiché « Rose 0.1.5 » | (a) | **Changé** → « Rose 0.1.6 » | Le titre de la console est produit dynamiquement par `format!("Rose {} — console interactive", env!("CARGO_PKG_VERSION"))` (lecture de `rose/src/repl.rs` et `console_win.rs`) et `rose/Cargo.toml` = 0.1.6. Le nom du profil Windows Terminal, lui, reste « Rose » (sans version). |
| `wikimiroir/Install.md:3` « versioned **0.1.5** everywhere: engine, Windows installer, VS Code extension and WebAssembly module » | (a) | **Reformulé** → `0.1.6` pour moteur / installateur / WASM, avec mention explicite « The VS Code extension is still published as 0.1.5 » | L'affirmation « partout 0.1.5 » est devenue fausse pour le moteur ; elle reste vraie pour l'extension, non republiée. |
| `wikimiroir/Install.md:33,37` `rose-language-0.1.5.vsix` | (b) | **Gardé** 0.1.5 | Nom réel du VSIX publié ; aucun artefact 0.1.6 (API + dépôt). |
| `vscode-rose/package.json:4` description « langage Rose (v0.1.5) » | (b) | **Gardé** 0.1.5 | Version de l'extension VS Code, non republiée en 0.1.6. |
| `vscode-rose/package.json:5` `"version": "0.1.5"` | (b) | **Gardé** 0.1.5 | Identité du paquet VSIX réel `rose-language-0.1.5.vsix`. Passer à 0.1.6 sans artefact publierait une version inexistante. |
| `vscode-rose/README.md:3` « Rose (v0.1.5) dans VS Code » | (b) | **Gardé** 0.1.5 | Cohérent avec `package.json` et le VSIX publié. |

## Vérification des hashes

`sha256sum` recalculé sur les 10 fichiers présents dans `docs/telechargements/`,
puis `sha256sum -c SHA256SUMS` → **10 × `OK`, `rc=0`** (avant correction,
`releases.win.json` était `FAILED`). Valeurs calculées :

| Fichier | SHA-256 calculé |
|---|---|
| `Rose-win-Setup.exe` | `fc623cc27b5df1f599d9f87010991e059470e8d42b9f723f3b703dc26fd8dd9e` |
| `rose-shell.exe` | `df4b9ca07bc162b4542d6ee343b185fcc13aebe3d20bdc819ff71ba88180ff65` |
| `Rose-0.1.5-full.nupkg` | `99b0c22d29ff435f24cc83b3c93f0f7a012ee23ed4ba44dfbd4c6d96dbbaafdb` |
| `Rose-0.1.6-full.nupkg` | `e194f9793d20547844d51c5b221a5e6854cd8d3f83570730efb3682d625117e9` |
| `releases.win.json` | `ae4b6f670065b86407c3888140c87221a083e8874b0f7c8a2e53dbcf4b2a1e48` |
| `rose-skill.zip` | `ba36afa247911300eb4b5479bc0d17cedd07b750baa4bf4b07f528a5271948ec` |
| `rose-wasm.wasm` | `cc3bb23f6f31131f778230b5d9a8f4ac50e8828db56a7d752a6428053e23ca07` |
| `rose-language-0.1.5.vsix` | `3f17dd0d0a3c5e71fe1037e8f775d629181f863ceed127f031e37d47747b5e8c` |
| `rose-aarch64-linux` | `5156144574749ea7ffb337ea92e05e500a4a8af75cf9ca407db7ea89fe968531` |
| `rose-aot-aarch64-linux` | `42e48fa074b113d587a7c9134adfbf33c58308a065448167f3c460feef384494` |

## Grep final des `0.1.5` restants (périmètre VER1) et justification

| Occurrence restante | Justification |
|---|---|
| `README.md:746` | Exemple de désynchronisation (b). |
| `README.md:820` | Asset `rose-skill.zip` réellement en v0.1.5 seulement (b). |
| `docs/index.html:743` (ex-742) | Option historique du sélecteur, plus « actuelle ». |
| `docs/index.html:1112` (ex-1111) | Changelog daté du 14/11/2026. |
| `docs/index.html:1371` | Note JS pour l'option historique 0.1.5. |
| `docs/index.html:1398` (ex-1396) | Entrée de recherche du changelog historique. |
| `docs/telechargements/SHA256SUMS:3` | Checksum du nupkg 0.1.5 présent. |
| `docs/telechargements/SHA256SUMS:8` | Checksum du VSIX 0.1.5 présent. |
| `wikimiroir/Install.md:4` | Extension VS Code encore publiée en 0.1.5. |
| `wikimiroir/Install.md:33` | Nom du VSIX réel. |
| `wikimiroir/Install.md:37` | Commande d'installation du VSIX réel. |
| `vscode-rose/README.md:3` | Version de l'extension publiée. |
| `vscode-rose/package.json:4` | Description de l'extension publiée. |
| `vscode-rose/package.json:5` | Version du paquet VSIX publié. |

## Validation complémentaire

- JSON relus par `python3 -c 'json.load(...)'` : `vscode-rose/package.json` et
  `docs/telechargements/releases.win.json` → **OK**.
- `git status --short` à lire avec prudence : `rose/src/setup_gui.rs` a été
  modifié **pendant** ce chantier (horodatage `2026-11-20 13:23:33 +0100`,
  après la dernière modification VER1 à `13:22:58`) par un processus concurrent
  autre que VER1. **VER1 n'a ni lu pour écriture, ni modifié ce fichier** ;
  il est hors périmètre et laissé tel quel. L'arbre git était propre au
  démarrage de VER1.
- Fichiers modifiés par VER1 : `README.md`, `docs/index.html`,
  `docs/telechargements/SHA256SUMS`, `wikimiroir/CLI.md`, `wikimiroir/Home.md`,
  `wikimiroir/Install.md`, + création du présent fichier.
