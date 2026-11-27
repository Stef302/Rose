# Découverte dynamique des releases — `rose update`

## Le problème (cause exacte)

Un launcher installé une fois lisait **`releases.win.json`**, un manifeste
Velopack statique embarqué dans l'installation (et servi par
`/releases/latest/download/`). Ce manifeste ne contient **qu'une seule
entrée** — la version du build qui l'a produit (par exemple `0.1.76`).

Conséquence : un launcher installé à la v0.1.76 ne pouvait jamais voir les
releases publiées ensuite (v0.1.77, v0.1.78, v0.1.79), et demandait à
l'utilisateur de télécharger un nouveau launcher à chaque fois. Deux causes
s'additionnaient :

1. `check_and_update()` / `verifier_manifeste()` lisaient le manifeste
   statique au lieu d'interroger le dépôt ;
2. l'analyseur de l'API GitHub ne reconnaissait qu'un paquet nommé
   `Rose-<v>-full.nupkg` ou un installeur nommé `Rose-Setup-<v>.exe`. Or les
   releases v0.1.77 → v0.1.79 portent un installeur **générique**
   (`Rose-win-Setup.exe`) sans `.nupkg` : elles étaient donc écartées même
   quand l'API était interrogée.

Vérification faite le 26/11/2026 sur l'API publique : 16 releases, dont
v0.1.79 (`published_at` 2026-11-26T09:42:06Z), contre **1 entrée** dans le
manifeste local.

## La solution

`rose update` découvre désormais les releases **à l'exécution**, depuis
l'API publique :

```
GET https://api.github.com/repos/Stef302/Rose/releases?per_page=100
```

- **Aucune authentification** : API publique en lecture seule, `curl` présent
  sur Windows 10/11 (aucun crate externe, aucun `gh`, aucun jeton).
- **Aucun nouveau launcher** : le binaire installé interroge l'API himself.
- **Parseur JSON maison** : réutilisation de `objets_racine`,
  `champ_racine`, `tableau_sous_cle`, `comparer_versions` (déjà dans
  `rose/src/velopack.rs`). Aucune dépendance ajoutée.
- **Assets génériques reconnus** : les noms sont lus dans le tableau
  `assets` (pas par sous-chaîne, pour ne pas confondre un titre ou un
  changelog avec un fichier). `Rose-win-Setup.exe` marque une release
  Windows `installable: false` mais **annoncée**.
- **Comparaison sémantique** : `comparer_versions` (`0.1.79 > 0.1.76`,
  `0.1.72 > 0.1.7`). La version installée vient toujours de l'état local
  (`current` / `app-*`), jamais de `CARGO_PKG_VERSION`.

### Ordre des sources (`decouvrir_versions_update`)

| Ordre | Source | Quand |
|---|---|---|
| 1 | `ROSE_VELOPACK_URL` | Surcharge explicite (diagnostic, tests `file://`). Compatibilité ascendante : on lit exactement ce manifeste. |
| 2 | `ROSE_LAUNCHER_API` | URL d'API de diagnostic (tests `file://`). |
| 3 | **API GitHub publique** | Chemin normal, sans jeton, borné à 3 s (`DELAI_RESEAU_MS`). |
| 4 | **Manifeste local** `<racine>/releases.win.json`, puis `url_manifeste()` | Repli si le réseau est indisponible. |

`SourceVersions` indique toujours d'où vient réellement la liste
(`GitHub` ou `Local`) ; une erreur des sources n'est jamais masquée.

### Téléchargement

Quand la dernière version annoncée a un `.nupkg`, `rose update` lit le
manifeste **du tag** (`<base>/<tag>/releases.win.json`), et non celui de
`/latest` (qui ne connaît qu'une version) ; il vérifie le SHA-256, dépose le
paquet dans `packages\`, puis `appliquer_paquets()` l'installe. Aucun binaire
n'est accepté sans SHA-256.

Quand la release n'a **pas** de `.nupkg` (cas v0.1.77 → v0.1.79), la version
est annoncée mais **rien n'est téléchargé** : aucun `.exe` non vérifié n'est
accepté. Le message indique la version, la release et l'artefact proposé.
Le lanceur déjà installé reste utilisable.

## Commandes

```sh
# Vérification / mise à jour par le chemin Velopack
rose update --velopack

# Forcer la voie Velopack depuis un profil non installé
ROSE_VELOPACK=1 rose update

# Simuler hors ligne (manifeste local imposé)
ROSE_VELOPACK_ROOT=/tmp/vpk ROSE_VELOPACK_URL=file:///tmp/vpk/releases.win.json \
  rose update --velopack

# Simuler l'API avec une réponse GitHub enregistrée
ROSE_LAUNCHER_API=file:///tmp/vpk/api.json rose update --velopack

# Preuve automatisée, en lecture seule, sur l'API réelle
cargo test --release rose_update_voit_v0_1_79_reel -- --ignored --nocapture
# → ROSE UPDATE VOIT : 0.1.76 → <dernière release> (ex. v0.1.86 le 27/11/2026, source GitHub (repli))
# Le test ne code PAS la dernière version en dur : il vérifie qu'elle est
# strictement supérieure à 0.1.76 et qu'elle publie un .nupkg.
# « GitHub (repli) » est le libellé de SourceVersions::GitHub : GitHub est la
# source de repli par rapport au manifeste du site (libelle() dans velopack.rs).

# Liste complète (dry-run, lecture seule)
cargo test --release launcher_dry_run_reel -- --ignored --nocapture
```

Le garde-fou de mode est conservé : en mode **« Désactivée »**, toute
vérification — y compris `rose update` — est refusée avec un message
explicite (`docs/AUTO_UPDATE_CHOIX.md`). Les modes « À l'ouverture » et
« Automatique (discret) » gardent leur comportement (timer 6 h, une seule
requête par tick).

## Repli et limites

- **Hors réseau** : le manifeste local est utilisé ; il peut être plus ancien
  que l'API, c'est le prix du mode hors ligne. Aucune version n'est inventée.
- **Cache** : la réponse brute GitHub est conservée dans
  `versions-cache.json` (TTL 300 s) ; l'UI l'affiche immédiatement puis la
  rafraîchit. `rose update` interroge l'API à chaque appel explicite.
- **Releases sans artefact Windows** : ignorées (rien à proposer).
- **Releases avec installeur générique seulement** : annoncées, non
  installables par `packages\` ; migration manuelle via `Rose-win-Setup.exe`.
- **SHA-256 absent** : paquet refusé, jamais installé.
- **Source site** : l'UI du launcher garde le manifeste du site comme source
  prioritaire (`lister_versions_source`) ; `rose update`, lui, interroge
  directement GitHub, conformément à cette correction.

## Conséquence sur `scripts/publier-release.py` et le manifeste

`releases.win.json` **n'est plus la source de vérité pour la découverte** :
sa régénération à chaque release n'est plus nécessaire pour qu'un ancien
launcher voie une nouvelle version. Le script `scripts/publier-release.py`
reste inchangé ; il continue de produire le manifeste via
`installateur/velopack/pack-linux.sh` pour les releases qui publient un
`.nupkg` (compatibilité des installations existantes et repli hors ligne).
Il n'est plus besoin de republier un launcher ni de mettre à jour ce
manifeste pour annoncer une release : la publication GitHub suffit.

## Fichiers concernés

- `rose/src/velopack.rs` — découverte dynamique, assets génériques, repli,
  tests (dont le test réel `rose_update_voit_v0_1_79_reel`).
- `rose/docs/UPDATE_DYNAMIQUE.md` — ce document.

Aucun changement n'affecte le compilateur Rose, les 141 sources de parité
self-hosting ni le point fixe `b3ed9262`.
