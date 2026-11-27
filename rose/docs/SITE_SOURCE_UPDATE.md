# SITE_SOURCE_UPDATE — le site Rose, source primaire de découverte des releases

**Constat daté aux 27–28/11/2026.** Worktree exclusif `/tmp/opencode/v19site`,
HEAD `2eecd62`. Aucun commit, aucun push, aucune publication effectuée par ce
chantier. Ce document décrit l'ordre RÉEL des sources après correction.

## 1. Pourquoi le site d'abord

Le manifeste `https://stef302.github.io/Rose/telechargements/releases.json`
est servi par GitHub Pages : **aucun quota d'API, aucune authentification,
aucune limite de requêtes par heure**. L'API publique GitHub
(`/repos/Stef302/Rose/releases`) est limitée (60 requêtes/heure par IP sans
jeton) et peut devenir indisponible au moment précis où un utilisateur ouvre
le launcher. Le site est donc la source de vérité ; GitHub n'est qu'un repli.

Le launcher lit le manifeste une fois à l'ouverture : cache local horodaté,
TTL **300 s**, une seule requête bornée à **3 s** sinon (`requeter_json`).
Le mode discret (timer 6 h) ne fait qu'UNE lecture site par tick, sans repli
GitHub, pour éviter deux requêtes.

## 2. Ordre exact des sources (`decouvrir_versions_update`)

| Ordre | Source | Déclenchement |
|---|---|---|
| 1 | `ROSE_VELOPACK_URL` (manifeste `file://` ou HTTP imposé) | surcharge explicite, compatibilité/tests |
| 2 | `ROSE_LAUNCHER_API` (URL API imposée) | surcharge explicite, diagnostic/tests |
| 3 | **SITE ROSE** — `lister_versions_site()` via `ROSE_LAUNCHER_SITE_MANIFESTE` ou l'URL Pages | source par défaut ; cache frais d'abord |
| 4 | API GitHub publique — `lister_versions()` | uniquement si le site échoue |
| 5 | Manifeste local `<racine>/releases.win.json`, puis `url_manifeste()` | dernier recours hors ligne |

La source réellement utilisée est annoncée par `SourceVersions` :
`site Rose`, `site Rose (cache)`, `GitHub (repli)`, `appareil`. Le repli
n'est jamais silencieux : les causes d'échec du site et de GitHub sont
conservées dans le message d'erreur final.

Le parseur `analyser_manifeste_site` accepte les trois formes : liste nue
`[…]`, enveloppe `{"releases":[…]}` et **enveloppe réelle du site
`{"versions":[…]}`** (c'était le point bloquant : le manifeste publié utilise
`versions` et l'analyseur ne connaissait que `releases`). Un paquet n'est
marqué `installable` que si son SHA-256 est annoncé.

## 3. Régénérer le manifeste (toutes les versions réelles)

Le fichier `docs/telechargements/releases.json` passe de **8 versions**
(0.1.76 → 0.1.5) à **23 versions** : les 18 versions demandées
**0.1.5 → 0.1.86** y sont toutes, plus les 5 releases GitHub antérieures
(0.1.0 → 0.1.4) pour la parité complète. Les nupkg 0.1.81 → 0.1.86 (et
0.1.4) ont été copiés depuis `installateur/velopack/Releases/` du dépôt
principal vers `docs/telechargements/`. **Aucun hash n'est inventé** : chaque
`sha256`/`taille` est calculé sur le fichier réellement présent ; les titres,
dates et changelogs viennent de l'API GitHub publique.

```bash
# Depuis la racine du dépôt (branche main pour une publication réelle)
python3 scripts/sync-site.py --regenerer
python3 scripts/sync-site.py --verifier-local
python3 scripts/sync-site.py --verifier-parite          # 23/23 attendu
```

`--api-json /chemin/rose-releases.json` permet de régénérer SANS réseau à
partir d'une réponse GitHub déjà reçue (aucune donnée fabriquée).

## 4. Commande de synchronisation post-release

À chaque release, le nupkg, `releases.json` et `releases.win.json` doivent
partir sur GitHub **et** sur Pages. Le chemin normal est la commande unique :

```bash
python3 scripts/publier-release.py <version> "<titre>" notes.md
```

Elle enchaîne les 10 étapes historiques, puis l'**étape 11** ajoutée :
`scripts/sync-site.py --verifier-local --verifier-parite --verifier-site`
(6 essais espacés de 30 s pour laisser Pages se déployer). Un écart sur le
contenu servi est un **échec** de la publication, pas un avertissement.

Si l'on publie seulement le site (par exemple après une correction du
manifeste), la commande exacte est :

```bash
python3 scripts/sync-site.py --regenerer
python3 scripts/sync-site.py --publier                  # git add/commit/push -> Pages
python3 scripts/sync-site.py --verifier-parite --verifier-site --attendre 30 --essais 6
```

`--verifier-site` ne se contente pas du code HTTP : il télécharge chaque
fichier et compare son SHA-256 à celui du fichier local. `--dry-run` simule
sans écrire.

## 5. Checklist de vérification (preuves exécutées le 27/11/2026)

| Contrôle | Commande | Résultat mesuré |
|---|---|---|
| Compilation | `cargo check --release --bin rose --manifest-path rose/Cargo.toml` (garde-fou 12 Go) | `Finished release … in 12.08s`, 0 erreur |
| Tests du module | `cargo test --release --manifest-path rose/Cargo.toml --lib velopack::tests -- --test-threads=1` | **33 passed, 0 failed, 4 ignored** |
| Découverte hors ligne | `ROSE_LAUNCHER_SITE_MANIFESTE=file:///…/releases.json cargo run --release --example verifier_source_update …` | `source=site Rose`, `versions=23`, `derniere=0.1.86` |
| Hashes locaux | `python3 scripts/sync-site.py --verifier-local` | 23 versions, 27 assets, 18 paquets Velopack ; `sha256sum -c SHA256SUMS` : 28/28 OK |
| Parité GitHub ↔ site | `python3 scripts/sync-site.py --verifier-parite` | **23/23** releases GitHub annoncées sur le site |
| Parité self-host | `PARITE_MOTEUR=… bash scripts/verifier_parite_selfhost.sh` (garde-fou 12 Go) | **141/141 CONFORMES**, 0 divergent, 0 bloqué, RC=0 |
| Point fixe | `rose rir compilateur/emetteur.rose` et `rose compilateur/emetteur.rose compilateur/emetteur.rose`, `md5sum` | `b3ed9262dd54776a6dbd07bc12368d0e` deux fois |

## 6. Limites connues et non masquées

- **Divergence 0.1.5 (historique, mesurée)** : le site sert
  `Rose-0.1.5-full.nupkg` de 8 103 198 o (`99b0c22d29ff…`) tandis que
  `releases.win.json` décrit le build Velopack de 8 740 427 o
  (`ad418be2397d…`). Les deux fichiers existent réellement ; rien n'est
  écrasé ni inventé. Le contrôle signale l'écart sans échouer et la liste
  d'exceptions est documentée dans `scripts/sync-site.py`.
- `Rose-0.1.0` à `Rose-0.1.3` sont annoncées **sans binaire** sur le site
  (`installable: false`) : leur nupkg n'est présent ni dans
  `docs/telechargements/` ni dans `installateur/velopack/Releases/`. Aucune
  URL de téléchargement n'est fabriquée.
- La section HTML `#page-releases` de `docs/index.html` n'a pas été touchée
  (fichier d'un autre chantier) : la source de découverte machine est le
  manifeste, qui contient bien toutes les versions. Le contrôle porte sur le
  manifeste et les fichiers servis, pas sur la mise en page.

## 7. Fichiers modifiés par ce chantier

- `rose/src/velopack.rs` — ordre site → GitHub → local, enveloppe `versions`
  acceptée, tests de structure et de découverte hors ligne.
- `rose/examples/verifier_source_update.rs` (nouveau) — diagnostic hors ligne.
- `docs/telechargements/releases.json` — 8 → 23 versions, hashes réels.
- `docs/telechargements/SHA256SUMS` — recalculé (28 lignes).
- `docs/telechargements/Rose-0.1.{4,81,82,83,84,85,86}-full.nupkg` — copiés
  depuis `installateur/velopack/Releases/`.
- `scripts/sync-site.py` (nouveau) — régénération, contrôle local, parité,
  publication Pages et contrôle du contenu servi.
- `scripts/publier-release.py` — étape 11 de contrôle du site.
- `rose/docs/SITE_SOURCE_UPDATE.md` (nouveau) — ce document.
