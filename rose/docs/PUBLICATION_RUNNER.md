# Publication de l'artefact runner wasmtime — release v0.1.5

Horodatage de la publication (UTC) : **2026-11-18T10:03:18Z**

## Résumé

L'artefact binaire du runner wasmtime a été publié sur la release **v0.1.5** du
dépôt **Stef302/Rose**. Publication **réelle** (mode `--exec`, confirmation `OUI`
explicite), effectuée via `scripts/publier-runtime.sh`. Aucun autre fichier n'a été
publié, aucun fichier source `.rs`, `Cargo.toml`, `PLAN.md` ni fichier d'un autre
agent n'a été modifié.

## Vérifications effectuées

| Contrôle | Commande | Résultat |
| --- | --- | --- |
| Compte GitHub | `GH_TOKEN="$(cat ~/.config/opencode/.gh_token)" gh api user -q .login` | `Stef302` — conforme (jamais `clausake`) |
| Artefact local | `ls -la` + `sha256sum /tmp/opencode/rose-runner-final1/rose-wasmtime-runner-v0.1.5-aarch64-linux` | présent, exécutable, 6 493 120 octets |
| SHA avant publication | `sha256sum` | `c7b2bf3c2b85e72ec37192f2ba855bd98de0c144288c9cf04aa63f11880739d1` |
| Release avant publication | `gh release view v0.1.5 --repo Stef302/Rose` | 10 assets, **aucun runner** → pas de doublon |
| Dry-run | `bash scripts/publier-runtime.sh <artefact>` (avec `ROSE_WASM_RUNTIME_SHA256`) | compte OK, cohérence SHA OK, rien publié |
| Publication | `printf 'OUI\n' \| bash scripts/publier-runtime.sh <artefact> --exec` | `gh release upload` réussi |
| Release après publication | `gh release view v0.1.5 --repo Stef302/Rose` | **11 assets**, runner présent, `state: uploaded` |
| URL de téléchargement | `curl -sIL` | `302` → `200`, `content-length: 6493120` |
| SHA après publication (aller-retour) | `curl -sL <URL> \| sha256sum` | `c7b2bf3c2b85e72ec37192f2ba855bd98de0c144288c9cf04aa63f11880739d1` — identique |

Le token GitHub n'a été lu que depuis `~/.config/opencode/.gh_token` via la
variable d'environnement `GH_TOKEN`; il n'a été écrit dans aucun fichier, ni dans
le dépôt, ni dans ce rapport.

## Artefact publié

- **Nom de l'asset** : `rose-wasmtime-runner-v0.1.5-aarch64-linux`
- **Taille** : 6 493 120 octets
- **SHA-256 (avant = après)** : `c7b2bf3c2b85e72ec37192f2ba855bd98de0c144288c9cf04aa63f11880739d1`
- **URL de téléchargement** :
  https://github.com/Stef302/Rose/releases/download/v0.1.5/rose-wasmtime-runner-v0.1.5-aarch64-linux
- **Page de la release** : https://github.com/Stef302/Rose/releases/tag/v0.1.5

## État final de la release v0.1.5

Release publique, non brouillon, non pré-release, auteur `Stef302`.
Assets (11) : `releases.win.json`, `Rose-0.1.5-full.nupkg`, `rose-aarch64-linux`,
`rose-aot-aarch64-linux`, `rose-language-0.1.5.vsix`, `rose-shell.exe`,
`rose-skill.zip`, `rose-wasm.wasm`, **`rose-wasmtime-runner-v0.1.5-aarch64-linux`**,
`Rose-win-Setup.exe`, `SHA256SUMS`.

Le runner est désormais téléchargeable et son empreinte vérifiée de bout en bout.
La table `sha256_attendu()` dans `wasm_runtime.rs` (fichier d'un autre agent) doit
être renseignée avec l'empreinte ci-dessus par l'orchestrateur — hors de ce chantier.
