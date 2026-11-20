# PUBLICATION — publier une release Rose en une seule commande

**Constat daté au 21/11/2026.** Outil livré : `scripts/publier-release.py`.
Il remplace la procédure manuelle en 7 étapes de `SITE_RELEASES.md` par une
seule invocation, de la vérification du compte GitHub jusqu'à la vérification
du nupkg téléchargé.

## Commande exacte

```bash
cd /home/clausake/Stefan/ROSE
python3 scripts/publier-release.py <version> "<titre>" <fichier_notes> [--dry-run] [--no-build]
```

Exemple réel (safety) :

```bash
python3 scripts/publier-release.py 0.1.72 "Rose v0.1.72 — correctifs" notes.md --dry-run
```

Publication réelle (après vérification du dry-run) :

```bash
python3 scripts/publier-release.py 0.1.72 "Rose v0.1.72 — correctifs" notes.md
```

Options :

| Option | Effet |
|---|---|
| `--dry-run` | Exécute les vérifications (1, 2, 9, 10 en lecture seule) et **simule** 3–8. Aucune écriture, aucun build, aucun commit, aucune release. |
| `--no-build` | Saute l'étape 5 et exige que les artefacts soient déjà présents. Utile pour republier les mêmes binaires sous une nouvelle version. |
| `--force` | Autorise l'écrasement d'une release GitHub existante (suppression puis recréation). Sans cette option, l'outil s'arrête immédiatement. |

## Prérequis

- Python 3.9+ (stdlib uniquement ; aucun `pip install`).
- Jeton GitHub dans `~/.config/opencode/.gh_token` (profil **Stef302**).
  Le script appelle `GET /user` et refuse tout autre `login` **avant toute
  action distante**. Le jeton n'est jamais affiché : les erreurs sont filtrées
  et les commandes simulées montrent `<jeton masqué>`.
- `gh` CLI (`gh release create`, `gh release download`, `gh release delete`),
  `git` sur la branche `main`, `cargo`, `sh`, `python3`.
- Chaîne de build utilisable : `sh scripts/build-windows-rebuild.sh`,
  `sh installateur/velopack/pack-linux.sh <version>`, cible Rust
  `wasm32-unknown-unknown` (`rustup target add wasm32-unknown-unknown`).
- `docs/telechargements/releases.json` existant ou non : l'outil utilise le
  schéma de SITE_RELEASES.md et conserve les métadonnées historiques déjà
  publiées. Les empreintes sont **toujours recalculées** sur les fichiers
  réellement présents.
- Fichier de notes : une ligne par puce, précédée de `-`, `*`, `•` ou
  `1.`. Les titres Markdown (`# …`) sont ignorés. Si aucune puce n'est
  trouvée, toutes les lignes non vides servent de puces.

## Les 10 étapes (et ce qu'elles modifient)

| # | Étape | Effet |
|---|---|---|
| 1 | Compte GitHub | Lecture seule de `/user`. Arrêt immédiat si `login != Stef302`. |
| 2 | Version | Accepte `x.y.z` et `x.y.zz` (`0.1.7`, `0.1.72`), compatible `comparer_versions`. |
| 3 | Cargo | `rose/Cargo.toml` (section `[package]`) et `rose/Cargo.lock` (`[[package]] name = "rose"`). Édition directe déterministe, sans `cargo check`. |
| 4 | Marqueurs d'état courant | `README.md` (badge shields.io `version-…`), `docs/index.html` (avant `#page-changelog` seulement : `MOTEUR NATIF — V…`, badge, `stat-num`, `releases/tag/v…`, `v… — actuelle`, `Rust v…`), `wikimiroir/Home.md` (`versioned as **…**`), `wikimiroir/CLI.md` (`"Rose …"`). **L'historique n'est pas touché.** |
| 5 | Builds | `build-windows-rebuild.sh`, puis `pack-linux.sh <version>`, puis `cargo build --release --target wasm32-unknown-unknown --lib` avec `CARGO_TARGET_DIR=<rose/target>`. Arrêt sur `rc != 0`, avec les 25 dernières lignes de sortie. |
| 6 | Artefacts | Copie de `installateur/velopack/Releases/Rose-<v>-full.nupkg`, `releases.win.json`, du stage (`Rose-win-Setup.exe`, `rose.exe`, `rose-shell.exe`) et du wasm (`rose.wasm` → `rose-wasm.wasm`) dans `docs/telechargements/` ; écriture de `VERSION` ; recalcul de `SHA256SUMS` sur tous les fichiers du dossier sauf `SHA256SUMS` et `releases.json`. |
| 7 | Manifeste | Régénère `docs/telechargements/releases.json` depuis les fichiers présents : `{version,tag,titre,date,changelog,assets:[{nom,url,sha256,taille}]}`. URL = `https://stef302.github.io/Rose/telechargements/…`, **jamais GitHub**. Aucun hash inventé. Les versions historiques gardent leurs assets initiaux, mais seulement s'ils existent encore sur disque. |
| 8 | Site | Insère un article en tête de `#page-changelog` (version, date du jour, titre, puces) et un événement en tête de `#page-releases` (boutons avec vrais SHA-256, mention `ACTUELLE` déplacée). Idempotent : une version déjà présente n'est pas dupliquée. |
| 9 | Git | `git add --` sur les seuls fichiers de la release, `git commit -m "chore(release): publier v<version>"`, `git push origin main`. Refuse toute branche autre que `main`. Les modifications d'autres agents ne sont pas ajoutées. |
| 10 | GitHub | Crée `v<version>` avec `gh release create … --title … --notes-file …` ; refuse d'écraser sans `--force`. Vérifie par API : tag présent, nombre d'assets, puis télécharge le nupkg et compare son SHA-256 au fichier local. |

**Date** : l'étape 8 et le manifeste utilisent `datetime.date.today()` de la
machine (date du jour réelle), pas une date codée en dur.

## Procédure `--dry-run`

1. Écrire les notes dans un fichier temporaire hors dépôt, par exemple
   `/tmp/opencode/notes-0.1.72.md`.
2. Lancer la commande avec `--dry-run` (exemple ci-dessus).
3. Lire le rapport : il affiche, pour chaque étape, les changements calculés,
   les commandes qui seraient exécutées, la liste des artefacts présents ou
   absents, le nombre de versions/assets du manifeste et l'aperçu de l'entrée
   courante.
4. Vérifier que rien n'a bougé :

   ```bash
   git diff --stat
   git status --porcelain=v1
   ```

   Seuls les fichiers d'autres agents (le dépôt est partagé) peuvent y
   apparaître ; aucun fichier de la release ne doit être modifié.
5. Corriger les notes ou les artefacts, relancer le dry-run, puis seulement
   lancer la commande sans `--dry-run`.

Note : en dry-run, les étapes 5–8 sont **simulées** — les builds ne tournent
pas et aucun fichier n'est écrit, y compris dans `docs/telechargements/`.
C'est ce qui permet d'utiliser une version fictive (`0.1.72`) sans toucher au
dépôt. Les étapes 1, 2, 9 et 10 font leurs vérifications réelles en lecture
seule (API GitHub, branche git).

## Ce que l'outil NE fait PAS

- Il ne modifie ni `rose/src/**`, ni `compilateur/**`, ni `.github/**`, ni
  `Cargo.toml` à la racine, ni `PLAN.md`, ni les docs des autres agents.
- Il ne réécrit pas manuellement le site : `docs/index.html` n'est modifié
  que par les étapes 4 et 8, de façon automatique et idempotente.
- Il ne publie pas GitHub Pages : le push sur `main` déclenche la publication
  du site par le dépôt. Aucune action Pages directe.
- Il ne reconstruit pas `rose-aarch64-linux` ni le VSIX : l'étape 5 ne lance
  que les trois builds prescrits (Windows, nupkg, wasm). Les fichiers déjà
  présents sont conservés et leurs empreintes recalculées.
- Il ne devine pas de hash, de taille, de date de release GitHub ni d'asset
  absent : tout est lu sur disque ou dans le manifeste existant.
- Il ne touche pas aux modifications non commitées des autres agents et ne
  les inclut pas dans son commit.
- Il n'écrase pas une release existante sans `--force`.

## Récupération si une étape échoue à mi-chemin

Chaque étape vérifie son propre résultat et s'arrête avec un code non nul et
un message précis. L'état du dépôt est alors celui de la dernière étape
réussie ; aucune publication distante n'a eu lieu avant l'étape 10.

| Échec | État | Récupération |
|---|---|---|
| 1 (mauvais compte) | Rien | Remplacer le jeton dans `~/.config/opencode/.gh_token` par celui de **Stef302**, relancer. |
| 2 (version) | Rien | Corriger l'argument, relancer. |
| 3–4 (édition) | Fichiers texte partiellement mis à jour (idempotents) | Relancer la même commande : les motifs déjà à jour sont reconnus et inchangés. |
| 5 (build) | Textes mis à jour, artefacts absents ou partiels | Lire les 25 dernières lignes affichées, corriger la chaîne de build, relancer avec `--no-build` si les artefacts sont bons, sinon relancer normalement. |
| 6 (copie) | `docs/telechargements/` partiellement copié | Relancer ; `shutil.copy2` et l'écriture de `VERSION`/`SHA256SUMS` sont idempotents. |
| 7 (manifeste) | Fichiers présents, manifeste inchangé | Vérifier `python3 -m json.tool docs/telechargements/releases.json`, corriger le fichier fautif, relancer. |
| 8 (HTML) | Manifeste à jour, site partiel | Relancer : la vérification `event-ver">V<version>` empêche tout doublon. |
| 9 (git) | Commit local éventuellement fait, push manqué | `git status`, puis `git push origin main` ; ne pas refaire la release. |
| 10 (release) | Code/site poussés, release absente ou incomplète | Relancer avec `--no-build` (les artefacts sont déjà sur disque) ; si la release existe mais est incomplète, utiliser `--force` pour la recréer. |

En cas de doute, relancer avec `--no-build` : les étapes 3–4 et 6–8 sont
idempotentes, l'étape 9 ne recommite rien si le message est identique et
l'étape 10 refuse d'écraser sans `--force`.

## Vérification après publication

```bash
curl -s https://stef302.github.io/Rose/telechargements/releases.json \
  | python3 -m json.tool > /dev/null && echo "manifeste servi : JSON valide"
cd docs/telechargements && sha256sum -c SHA256SUMS | tail -5
```

Le script lui-même a déjà vérifié, via l'API GitHub, le tag, le nombre
d'assets et l'égalité du SHA-256 du nupkg téléchargé avec le fichier local.
