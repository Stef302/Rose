# VER3 — version 0.1.73 : bump systémique du marqueur courant

> **Constat daté au 21/11/2026**, dépôt `Stef302/Rose`, HEAD `c25d9b3`
> (« PLAN : vague 79 — 0.1.73 systémique ») à l'ouverture du chantier.
> **VER3 n'a exécuté ni commit, ni tag, ni publication.**
>
> Périmètre possédé : `rose/Cargo.toml` (ligne `version` uniquement),
> `rose/Cargo.lock` (bloc `[[package]] name = "rose"`), `README.md`,
> `docs/index.html`, `wikimiroir/{Home,CLI}.md`,
> `docs/telechargements/releases.json`, et le présent
> `rose/docs/VERSION_073.md` (nouveau).
>
> Hors périmètre, non modifiés : les **binaires** de `docs/telechargements/**`
> et leurs empreintes (`SHA256SUMS`, `sha256`/`taille` des assets — copie et
> régénération réservées à l'orchestrateur), `releases.win.json`,
> `rose/src/**`, `rose/build.rs`, `rose/rose-tray.rc`, `PLAN.md`, `scripts/**`,
> `installateur/**`, `vscode-rose/**` et les autres `rose/docs/*.md`.

---

## 1. Règle (a) / (b)

- **(a) État courant → 0.1.73** : version Cargo, badge README, bandeau,
  badge et statistique du site, lien de release, descriptions du moteur,
  sélecteur de documentation (option `selected` « actuelle »).
- **(b) Historique → 0.1.72 et antérieures conservées telles quelles** :
  option historique du sélecteur, note JS `setDocVer()`, entrée 0.1.72 du
  manifeste, artefacts VS Code déjà publiés, fixtures de test du launcher,
  plans et comptes rendus datés.

La convention `0.1.7x` reste celle de `VERSION_072.md` :
`0.1.73 > 0.1.72 > 0.1.7 > 0.1.6` (comparateur `comparer_versions`,
`rose/src/velopack.rs`). Aucun tri lexical ni lecture « 0.1.7 + 3 ».

## 2. Marqueurs modifiés — avant → après

| Occurrence | Classe | Avant | Après |
|---|---|---|---|
| `rose/Cargo.toml:3` | (a) | `version = "0.1.72"` | `version = "0.1.73"` |
| `rose/Cargo.lock:477` (`[[package]] rose`) | (a) | `0.1.72` | `0.1.73` |
| `README.md:9` badge shields.io | (a) | `version-0.1.72` | `version-0.1.73` |
| `docs/index.html:494` bandeau | (a) | `MOTEUR NATIF — V0.1.72` | `V0.1.73` |
| `docs/index.html:511` badge | (a) | `v0.1.72` | `v0.1.73` |
| `docs/index.html:527` statistique | (a) | `0.1.72` | `0.1.73` |
| `docs/index.html:648` lien de release | (a) | `releases/tag/v0.1.72` | `v0.1.73` |
| `docs/index.html:781` sélecteur | (a) | `value="17" selected` `v0.1.72 — actuelle` | `value="18" selected` `v0.1.73 — actuelle` + `value="17">v0.1.72` conservée |
| `docs/index.html:825` doc | (a) | `Rust v0.1.72, licence MIT` | `v0.1.73` |
| `docs/index.html:997` doc | (a) | `moteur v0.1.72` | `v0.1.73` |
| `docs/index.html:1048` doc | (a) | `Le moteur (Rust v0.1.72)` | `v0.1.73` |
| `docs/index.html:1083` doc | (a) | `<h3>Moteur Rust (v0.1.72)</h3>` | `v0.1.73` |
| `docs/index.html:1508` `setDocVer()` | (a) | `ver >= 17` = actuelle | `ver >= 18` = actuelle ; `ver === 17` = 0.1.72 (historique) |
| `wikimiroir/Home.md:8` | (a) | `versioned as **0.1.72**` | `**0.1.73**` |
| `wikimiroir/CLI.md:48` titre console | (a) | `Rose 0.1.72` | `Rose 0.1.73` |

Aucun autre `0.1.72` du périmètre n'a été réécrit : les trois occurrences
restantes de `docs/index.html` (sélecteur historique, note `setDocVer()`,
mention « versions antérieures » de la nouvelle entrée de changelog) et
l'entrée historique de `releases.json` sont **explicitement conservées**.

## 3. Sélecteur et changelog du site

- Nouvelle option `value="18" selected` : `v0.1.73 — actuelle`. L'ancienne
  option `selected` devient `value="17">v0.1.72` (historique) ; `0.1.7`
  (`value="16"`) et les suivantes restent inchangées.
- `setDocVer()` : `ver >= 18` → « Version actuelle » ; nouveau cas
  `ver === 17` → « Version 0.1.72 (historique) » ; `ver === 16` → 0.1.7.
  Les filtres `data-min` ne sont pas modifiés.
- Chronologie **Changelog** (`#page-changelog`) : nouvelle entrée de tête
  `V0.1.73`, 21 NOVEMBRE 2026, marqueur `ACTUELLE` — AOT/ORC universel PE,
  helper tray autonome avec icône, logo à tige entière, bump de version.
  L'historique (V0.1.5 et antérieures) est laissé en place.

## 4. Manifeste `releases.json`

Nouvelle entrée de tête : `version`/`tag` **0.1.73**, `titre` aligné sur le
changelog du site, `date` `2026-11-21`, asset versionné
`Rose-0.1.73-full.nupkg` (URL `…/Rose-0.1.73-full.nupkg`). L'entrée **0.1.72**
est conservée en historique, ainsi que 0.1.7 / 0.1.6 / 0.1.5.

Les `sha256` et `taille` des assets sont **délibérément inchangés** : ils
décrivent les binaires actuellement présents ; les artefacts 0.1.73 seront
copiés par l'orchestrateur, qui régénérera les empreintes et
`SHA256SUMS`. Aucun binaire n'a été touché.

## 5. Vérification (commandes réellement exécutées)

- **grep des marqueurs courants** : `git grep` sur les 7 fichiers du
  périmètre — 20 occurrences de `0.1.73` (état courant) et aucune occurrence
  courante résiduelle de `0.1.72` ; les 9 occurrences restantes de `0.1.72`
  sont toutes historiques (sélecteur, `setDocVer()`, note de changelog,
  entrée 0.1.72 du manifeste).
- **`cargo build`** (arbre principal, après correction concurrente ci-dessous)
  : **rc=0, 0 erreur**, `Finished \`dev\` profile ... in 8.30 s`. Warnings
  préexistants uniquement.
- **`cargo t`** (alias `test --bins`, évite le SIGSEGV du cdylib) dans
  l'arbre principal : **rc=0, 0 échec**, 4 cibles `test result: ok` —
  `294 passed; 0 failed; 3 ignored` (binaire `rose`), `6 passed`
  (`rose-shell`), `8 passed` (`rose-setup`), `0 passed` (doc-tests). Soit
  **308 tests réussis, 0 échec**.
- **Contexte de la vérification** : au premier contrôle, l'arbre principal
  échouait sur 4 `E0599` (`ResultatSession::new` introuvable,
  `rose/src/demarrage.rs` l.108, 136, 163, 176) : fichier **nouveau, non
  commité, appartenant à un autre agent** (vague 79, hors périmètre VER3),
  avec des éditions simultanées de `lib.rs`, `cmd_llvm.rs`, `main.rs`,
  `paint.rs` et `setup_gui.rs`. Pour prouver que le bump n'introduit aucune
  erreur, build et tests ont d'abord été **rejoués dans un worktree détaché
  à HEAD (`c25d9b3`) avec uniquement les 7 fichiers du périmètre VER3
  appliqués** et un `CARGO_TARGET_DIR` dédié : `cargo build` rc=0, `cargo t`
  rc=0 (302 tests réussis, 0 échec). L'agent concurrent ayant ensuite corrigé
  son fichier, le contrôle final a été refait **dans l'arbre principal** :
  build et tests verts, résultats ci-dessus.
- **Point fixe `b3ed9262` ×2** — depuis la racine du dépôt, avec `rose/rose` :
  - `./rose/rose rir compilateur/emetteur.rose` → `rc=0`, md5
    `b3ed9262dd54776a6dbd07bc12368d0e`, 263 520 o, stderr 0 o ;
  - `./rose/rose compilateur/emetteur.rose compilateur/emetteur.rose` →
    `rc=0`, md5 `b3ed9262dd54776a6dbd07bc12368d0e`, 263 520 o, stderr 0 o.
  **Deux dumps identiques au même hash attendu (`cmp` = 0, octet pour octet).**
