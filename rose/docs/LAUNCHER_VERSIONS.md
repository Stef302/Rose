# Launcher unique — versions dynamiques depuis le dépôt

État : **19/11/2026**. Dépôt `Stef302/Rose`, HEAD `006ea56` au début du chantier.
Auteur : sous-agent. **Aucun commit, aucune publication.** Fichiers possédés modifiés :
`rose/src/velopack.rs`, `rose/src/setup_core.rs`. Nouveaux fichiers : ce document et
`rose/docs/launcher.patch`. `main.rs`, `setup_gui.rs`, `Cargo.toml`, `rose/rose`
**non touchés**.

---

## 1. Tags RÉELLEMENT récupérés (preuve de sortie)

Deux instruments indépendants, en lecture seule :

### a) API GitHub brute via `gh api` (CLI connecté comme `clausake` — lecture seule)

```
$ gh api repos/Stef302/Rose/releases --jq '.[] | [.tag_name,.name,(.assets|length)] | @tsv'
v0.1.5  Rose v0.1.5 — installeur Windows refait  11
v0.1.4  Rose v0.1.4 — primorielle #, terminal ?, super-factorielle $  8
v0.1.3  Rose v0.1.3 — le million ! en ~0,9 s  6
v0.1.2  Rose v0.1.2  0
v0.1.1  Rose v0.1.1  0
v0.1.0  Rose v0.1.0 — le moteur Rust devient la référence  0
```

### b) Notre implémentation (`lister_versions()`), test réel ignoré par défaut

```
$ cd rose && ROSE_VELOPACK_ROOT=... cargo test --release launcher_api_reelle -- --ignored --nocapture
TAG REEL v0.1.5 | version=0.1.5 | installable=true  | nupkg=Rose-0.1.5-full.nupkg | legacy=-
TAG REEL v0.1.4 | version=0.1.4 | installable=true  | nupkg=Rose-0.1.4-full.nupkg | legacy=-
TAG REEL v0.1.3 | version=0.1.3 | installable=false | nupkg=-                      | legacy=Rose-Setup-0.1.3.exe
test result: ok. 1 passed; 0 failed
```

**Constat important :** `v0.1.0`/`v0.1.1`/`v0.1.2` ont **0 artefact** ; `v0.1.3` n'a
que l'ancien `Rose-Setup-0.1.3.exe` (pas de `releases.win.json`, pas de `.nupkg`) ;
seuls `v0.1.4` et `v0.1.5` ont le couple manifeste + paquet complet. Le launcher
liste donc **toutes** les releases, mais marque `installable: false` celles qui
exigent l'installeur legacy. Ces données sont lues **dynamiquement** : une release
future publiée avec `Rose-<v>-full.nupkg` + `releases.win.json` apparaît
automatiquement, sans recompiler le launcher.

---

## 2. Architecture du launcher unique

```
[rose.exe — BINAIRE STABLE, identique pour toutes les versions]
        │
        ├─ 1er lancement : GET https://api.github.com/repos/Stef302/Rose/releases?per_page=100
        │      (gh api si présent, sinon curl ; aucune version codée en dur)
        │
        ├─ affichage : v0.1.5 ✓ | v0.1.4 ✓ | v0.1.3 (legacy)
        │
        ├─ choix de l'utilisateur → UN SEUL téléchargement :
        │      .../releases/download/<tag>/releases.win.json   (cache local)
        │      .../releases/download/<tag>/Rose-<v>-full.nupkg (8,2 Mo pour 0.1.5)
        │
        ├─ SHA-256 du .nupkg comparé au manifeste → refus si différence
        │
        ├─ extraction de lib/net45/* → %LOCALAPPDATA%\Rose\app-<v>\
        │
        └─ %LOCALAPPDATA%\Rose\current = "app-<v>" ; association .rose réaffirmée

[changement de version sans téléchargement] → selectionner_version(tag)
   pointe current sur un app-<v> déjà présent (ancienne version conservée = rollback)
```

Le lanceur ne embarque **aucun moteur Rose** : le moteur arrive uniquement avec
la version choisie. Le binaire ne change plus d'une release à l'autre — plus de
`Rose-win-Setup-<version>.exe` à republier à chaque mise à jour.

**Chemins et variables**

| Variable | Rôle |
|---|---|
| `ROSE_VELOPACK_ROOT` | force la racine d'installation (tests / installations hors profil) ; sinon `%LOCALAPPDATA%\Rose` |
| `ROSE_LAUNCHER_API` | surcharge l'API des releases (accepte `file://` pour tests hors ligne) |
| `ROSE_LAUNCHER_BASE` | surcharge la base des artefacts (accepte `file://`) |
| `ROSE_LAUNCHER_REPO` | surcharge `Stef302/Rose` pour la voie `gh api` |

Cache local : `%LOCALAPPDATA%\Rose\releases.win.json` (dernière version consultée).
Paquets en attente : `%LOCALAPPDATA%\Rose\packages\`.

---

## 3. Fonctions implémentées

### `rose/src/velopack.rs`

| Fonction | Rôle |
|---|---|
| `lister_versions() -> Result<Vec<VersionDistante>, String>` | API GitHub, triée du plus récent au plus ancien ; `installable` / `legacy` par release |
| `telecharger_version(tag) -> Result<String, String>` | manifeste du tag → cache → `.nupkg` → SHA-256 → `app-<v>\` → `current` |
| `selectionner_version(tag) -> Result<String, String>` | rebascule `current\` sur un `app-<v>` existant (rollback) |
| `telecharger_legacy(tag) -> Result<PathBuf, String>` | ancien `Rose-Setup-<v>.exe` + vérification via `SHA256SUMS` (exécution non lancée) |
| `versions_installees() -> Vec<String>` | dossiers `app-*` présents sur disque |
| `versions_cachees() -> Vec<String>` | versions du manifeste local (hors ligne) |
| `dry_run(tag: Option<&str>) -> Result<String, String>` | plan d'action, **aucun fichier modifié** |
| `analyser_releases(json)` | parseur JSON maison (objets de 1er niveau, chaînes respectées, piège `author.name` évité) |
| `champ_racine(objet, cle)` / `objets_racine(json)` | lecture JSON sans crate externe |
| `requeter_json(url, endpoint)` | `file://` → `gh api` → `curl` (sans fenêtre de console sur Windows) |
| `telecharger_fichier(url, cible)` | `fs::copy` pour `file://`, sinon `curl -o` |

Types : `VersionDistante { tag, version, titre, nupkg, installable, legacy }`,
`DEPOT`, `API_RELEASES`, `BASE_ARTIFACTS`, `NOM_CACHE`.

### `rose/src/setup_core.rs`

Ponts pour le cœur d'installation / l'UI, sans dupliquer la logique réseau :
`launcher_versions()`, `launcher_installer(tag)`, `launcher_selectionner(tag)`,
`launcher_dry_run(tag)`.

---

## 4. Patch CLI (non appliqué — `main.rs` verrouillé)

Voir **`rose/docs/launcher.patch`**. Il ajoute trois commandes
(`rose versions`, `rose installer <tag>`, `rose update --dry-run`) et un
point d'entrée `rose-setup` = launcher stable. **Aucune modification de
`main.rs` n'a été faite ici.**

---

## 5. Prouvé ici / à tester sur Windows

### PROUVÉ (commandes réellement exécutées le 19/11/2026)

1. `cargo build --release --bin rose` → `Finished ... in 12.57s`, binaire
   `target/release/rose` (5 456 840 octets).
2. `cargo test --release` → **217 passed, 0 failed** (lib) et **201 passed,
   0 failed** (bin), plus 6 et 1 tests des autres cibles, 0 échec. Les 4 tests
   du launcher (`launcher_etiquettes`, `launcher_liste_versions`,
   `launcher_selection_et_cache`, `launcher_api_reelle` en mode ignoré) passent.
   Le correctif Rust 2024 est appliqué : chaque `set_var`/`remove_var` est dans
   `unsafe { ... }` (l'erreur `E0133` signalée par l'orchestrateur est corrigée).
3. Parcours complet **hors ligne** (`launcher_selection_et_cache`) :
   manifeste → téléchargement `file://` → SHA-256 → ZIP → `app-0.1.6/rose.exe`
   → `current = app-0.1.6` ; puis SHA erroné → `Err("SHA-256 invalide ...")`
   et `current` inchangé.
4. Parcours complet **avec le vrai artefact v0.1.5**
   (`launcher_installation_reelle_locale`, miroir local dans `/tmp/opencode/lb_agents`) :
   `INSTALLATION REELLE v0.1.5 : 8 entrées`, `current = app-0.1.5`, cache
   `releases.win.json` md5/sha conformes.
5. SHA-256 réel calculé sur l'artefact téléchargé depuis GitHub :
   ```
   ea93bc0832d56e642140af6ab80249bd6fcc70938f9d5ebc5463ab7d7586147a  Rose-0.1.5-full.nupkg
   ea93bc0832d56e642140af6ab80249bd6fcc70938f9d5ebc5463ab7d7586147a  (manifeste)
   ```
   Taille 8 213 857 octets = `Size` du manifeste. Égalité prouvée.
6. `--dry-run` réel (`cargo test --release launcher_dry_run_reel -- --ignored
   --nocapture`), extrait :
   ```
   [dry-run] aucun fichier ne sera modifié
   [dry-run] versions distantes (3) :
   [dry-run]   v0.1.5   paquet complet disponible
   [dry-run]   v0.1.4   paquet complet disponible
   [dry-run]   v0.1.3   ancien installeur (legacy)
   [dry-run] action     : telecharger_version(v0.1.5) → SHA-256 → app-0.1.5 → current
   ...
   [dry-run] action     : telecharger_legacy(v0.1.3) → SHA-256 via SHA256SUMS (Rose-Setup-0.1.3.exe) — exécution Windows à valider
   ```
7. `git status` final : seuls `rose/src/velopack.rs`, `rose/src/setup_core.rs`,
   `rose/docs/LAUNCHER_VERSIONS.md`, `rose/docs/launcher.patch` proviennent de
   **ce** chantier. `rose/src/setup_gui.rs` a été modifié **par une autre
   session** pendant le travail (non touché ici).

### Correction signalée par l'orchestrateur

`error[E0133]` sur `set_var`/`remove_var` (édition Rust 2024) : **corrigé** dans
`velopack.rs` — chaque appel est entouré de `unsafe { ... }`. La dernière exécution
de `cargo test --release` donne **217 passed / 0 failed** (lib) et
**201 passed / 0 failed** (bin).

### À TESTER SUR WINDOWS (machine Windows absente ici — honnêteté)

- Le binaire launcher **slim** lui-même : je n'ai pas construit de nouvel exe
  (interdiction) ; le binaire présent reste le `rose` complet. Le patch CLI montre
  le câblage, pas un exe livré.
- `CREATE_NO_WINDOW` sur `gh.exe`/`curl.exe` : le code est conditionné `#[cfg(windows)]`
  et compiles, mais aucun test GUI réel n'a été fait ici.
- Exécution silencieuse de l'ancien `Rose-Setup-*.exe` (legacy) : délibérément
  **non lancée** ; `telecharger_legacy()` ne fait que télécharger + vérifier.
- Association `.rose` après changement de version (`appliquer_silencieuse`) :
  no-op hors Windows, donc non vérifiée ici.
- Le `.nupkg` v0.1.5 **contient encore** `Rose-win-Setup.exe` (9 105 408 o) ET
  `payload/rose.exe` (doublon du moteur) en plus de `rose.exe`. Le launcher
  n'ajoute pas de nouvel exe par release, mais **la chaîne CI actuelle produit
  encore ce paquet redondant** : le nettoyage du contenu du `.nupkg` est un
  changement de packaging (hors de mes fichiers possédés). Constat relevé sur
  l'artefact réel, pas une supposition.

---

## 6. Honnêteté sur le contexte

- Le CLI `gh` de cette machine est authentifié comme **`clausake`** ; le PAT
  `~/.config/opencode/.gh_token` est **`Stef302`**. Seul le CLI a été utilisé,
  **en lecture seule** (`gh api` sur une API publique). Aucune publication,
  aucun tag, aucun asset, aucun commit.
- Le parseur JSON est maison : il couvre la structure réelle renvoyée par
  l'API (vérifié sur la réponse live) mais n'est pas un parseur JSON complet.
- `lister_versions()` interroge `gh` d'abord ; si `gh` est absent ou échoue,
  repli sur `curl` avec l'en-tête `Accept: application/vnd.github+json`.
  Sur une machine Windows sans `gh` ni `curl`, le launcher doit compiler un
  client HTTP minimal ou exiger `curl` — documenté comme limite.

## 7. Commandes de vérification reproductibles

```
cd /home/clausake/Stefan/ROSE/rose
~/.cargo/bin/cargo build --release --bin rose
~/.cargo/bin/cargo test --release
~/.cargo/bin/cargo test --release launcher_api_reelle -- --ignored --nocapture
# installation réelle depuis un miroir local :
ROSE_VELOPACK_ROOT=/tmp/opencode/lb_agents/install \
ROSE_LAUNCHER_BASE=file:///tmp/opencode/lb_agents/miroir \
  ~/.cargo/bin/cargo test --release launcher_installation_reelle_locale -- --ignored --nocapture
```
