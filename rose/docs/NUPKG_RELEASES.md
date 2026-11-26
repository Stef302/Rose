# Publier un `.nupkg` Velopack avec chaque release Rose

## Constat (26/11/2026)

Les releases `v0.1.77`, `v0.1.78` et `v0.1.79` avaient 4 assets (`rose.exe`,
`rose-shell.exe`, `rose-tray.exe`, `Rose-win-Setup.exe`) mais **aucun
`Rose-<version>-full.nupkg`**. Le launcher installé lit pourtant
`releases.win.json` (par défaut
`https://github.com/Stef302/Rose/releases/latest/download/releases.win.json`),
puis télécharge le paquet complet dont le nom figure dans le manifeste
(`rose/src/velopack.rs::check_and_update`). Sans nupkg, la mise à jour échoue
au téléchargement.

Deux causes distinctes :

1. Les releases manuelles récentes n'ont pas appelé
   `installateur/velopack/pack-linux.sh` (étape 5 de
   `scripts/publier-release.py`, qui enchaîne build Windows → pack → WASM).
2. `pack-linux.sh` **écrase** `releases.win.json` avec une seule entrée (la
   version qu'il vient d'empaqueter) : même en le lançant, l'historique du
   manifeste est perdu à chaque release.

La correction est donc **procédurale** : empaqueter, puis régénérer le
manifeste depuis les fichiers réellement présents, puis publier les deux sur
la release.

## Format attendu

### Paquet `Rose-<version>-full.nupkg`

Archive ZIP (décompression `deflate`) contenant :

- `lib/net45/...` : les binaires PE du `stage/` (`rose.exe`, `rose-shell.exe`,
  `rose-tray.exe`, `Rose-win-Setup.exe`, `payload/`, …) ; c'est ce dossier que
  `velopack.rs::extraire_net45` recopie dans `app-<version>\` ;
- `sq.version` à la racine : nuspec XML avec `<id>Rose</id>` et
  `<version><version></version>` — c'est la source de vérité de la version
  lue par `velopack.rs::lire_sq_version`.

`pack-linux.sh` fixe la date de chaque entrée ZIP à `2026-01-01 00:00:00` :
deux exécutions sur le même `stage/` donnent donc le **même SHA-256**.

### Manifeste `releases.win.json`

Tableau JSON, une entrée par version, **du plus récent au plus ancien** :

```json
{
  "PackageId": "Rose-0.1.79-full.nupkg",
  "Version": "0.1.79",
  "Type": "Full",
  "FileName": "Rose-0.1.79-full.nupkg",
  "SHA1": "<hex>",
  "SHA256": "<hex>",
  "SHA512": "<base64>",
  "Size": 1831254
}
```

`Type` vaut `Full` (pas de delta). `SHA1`/`SHA256` sont hexadécimales,
`SHA512` est en base 64, `Size` est le nombre d'octets. Le parseur maison
(`Manifeste::analyser`) lit ces clés telles quelles ; `paquet_a_jour` choisit
la version `Full` la plus haute strictement supérieure à la version installée.

**Aucune empreinte ni taille ne doit être écrite à la main** : tout est
calculé sur les octets des fichiers du dossier `Releases/`.

## Procédure complète (à chaque release `vX.Y.Z`)

Toutes les commandes se lancent depuis la racine du dépôt.

### 1. Vérifier le compte (jamais `clausake`)

```sh
export GH_TOKEN="$(cat ~/.config/opencode/.gh_token)"
gh api user -q .login          # doit afficher Stef302
```

### 2. Construire les artefacts Windows puis empaqueter

```sh
sh installateur/velopack/build-linux.sh          # si le stage est périmé
sh installateur/velopack/pack-linux.sh X.Y.Z     # → Releases/Rose-X.Y.Z-full.nupkg
```

Le script exige `installateur/velopack/stage/rose.exe`. `stage/` et
`Releases/` sont **gitignorés** : ce sont des artefacts de build, pas du code.

### 3. Régénérer le manifeste (toutes les versions voulues)

```sh
python3 installateur/velopack/regenerer-manifeste.py 0.1.76 X.Y.Z
# ou, sans argument, pour inclure TOUS les nupkg présents :
python3 installateur/velopack/regenerer-manifeste.py
```

Cette étape est indispensable : elle remplace l'entrée unique écrite par
`pack-linux.sh` par la liste complète, et écrit **deux copies identiques** :

- `installateur/velopack/Releases/releases.win.json` (source, gitignorée) ;
- `docs/telechargements/releases.win.json` (copie suivie, publiée sur le site).

### 4. Miroir site et sommes de contrôle

```sh
cp -p installateur/velopack/Releases/Rose-X.Y.Z-full.nupkg docs/telechargements/
# recalculer docs/telechargements/SHA256SUMS (voir scripts/publier-release.py, étape 6)
```

Sans cette copie, le manifeste du site annonce un paquet que le site ne sert
pas : `velopack.rs` construit l'URL du nupkg en remplaçant le dernier segment
de l'URL du manifeste par `FileName`.

### 5. Uploader sur la release GitHub

```sh
gh release upload vX.Y.Z \
  installateur/velopack/Releases/Rose-X.Y.Z-full.nupkg \
  installateur/velopack/Releases/releases.win.json \
  --clobber
```

`--clobber` rend l'opération idempotente (un manifeste peut déjà exister sur
la release). Si l'API d'upload renvoie un 404 transitoire, réessayer : c'est
un incident déjà observé, pas une absence de droits.

### 6. Valider

```sh
gh release view vX.Y.Z --json assets           # nupkg + releases.win.json présents
gh release download vX.Y.Z --pattern '*full.nupkg' --pattern 'releases.win.json' --dir /tmp/verif --clobber
sha256sum /tmp/verif/Rose-X.Y.Z-full.nupkg     # == SHA256 du manifeste
cmp /tmp/verif/releases.win.json installateur/velopack/Releases/releases.win.json
python3 -c 'import json; m=json.load(open("installateur/velopack/Releases/releases.win.json")); print([e["Version"] for e in m])'
```

Le contrôle de bout en bout côté moteur reste
`sh installateur/velopack/verifier.sh` (12 contrôles) et
`cargo test --release velopack`.

## État corrigé au 26/11/2026

Les trois nupkg manquants ont été produits depuis le `stage/` présent, puis
uploadés avec le manifeste mis à jour sur `v0.1.77`, `v0.1.78` et `v0.1.79`.
Le manifeste contient 4 entrées (0.1.76 → 0.1.79) :

| Version | Taille (octets) | SHA-256 |
|---|---:|---|
| 0.1.79 | 1 831 254 | `c13c4e88d995373a009fa99f4680b0670d0f7cfab09db7e6e66fd2ca34d59672` |
| 0.1.78 | 1 831 254 | `997c1765bfa07802fcb4c60fc2f42b4858e65c48db3bd1d93e1ac9ca2e7ee7a9` |
| 0.1.77 | 1 831 253 | `db02d7afc4a3918cab8074bc95ba0f56d17d1ae69c08b7ba6726d9ef5161970d` |
| 0.1.76 | 11 413 518 | `c18573081550b09612fa2456f355eb78a7bab9e41dabba85c64b635c5d3fe491` |

Les tailles plus petites des trois nouveaux paquets reflètent le `stage/`
courant (artefacts PE self-hostés du 26/11), et non une perte de contenu :
chaque paquet contient bien ses 16 entrées `lib/net45/` et son `sq.version`.

Le manifeste historique de `v0.1.76` n'a pas été écrasé : il décrivait
correctement l'unique paquet disponible à cette date. Le chemin par défaut
du launcher passe toujours par `/releases/latest/`, donc par `v0.1.79`.
