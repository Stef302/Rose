# SITE_RELEASES — le site, source officielle des releases de Rose

**Constat daté au 21/11/2026.** Ce document décrit le dispositif livré par
l'agent SITE1 : une section `RELEASES` sur le site `stef302.github.io/Rose/`
et un manifeste machine-readable que le launcher consomme.

## Principe

- **Le site est la source de vérité des releases.** Les binaires sont servis
  depuis `docs/telechargements/`, publiés par GitHub Pages à
  `https://stef302.github.io/Rose/telechargements/`.
- **GitHub reste un miroir** (référence historique des tags, notes et assets).
  Les boutons de téléchargement de la section `RELEASES` ne pointent **jamais**
  vers GitHub.
- **Le launcher lit le manifeste** `docs/telechargements/releases.json`
  (URL publique : `https://stef302.github.io/Rose/telechargements/releases.json`).
  C'est ce fichier, pas l'API GitHub, qui fait foi pour la liste des versions,
  leurs artefacts, leurs tailles et leurs empreintes SHA-256.

Fichiers concernés :

| Fichier | Rôle |
|---|---|
| `docs/index.html` | onglet « RELEASES » + page `#page-releases` (navigation, changelogs, boutons de téléchargement vers le site) |
| `docs/telechargements/releases.json` | manifeste lu par le launcher |
| `docs/telechargements/SHA256SUMS` | empreintes de contrôle de tous les fichiers du dossier |
| `rose/docs/SITE_RELEASES.md` | le présent document |

## Schéma du manifeste

```json
{
  "source": "site",
  "genere": "2026-11-21",
  "versions": [
    {
      "version": "0.1.7",
      "tag": "v0.1.7",
      "titre": "Rose v0.1.7 — lanceur définitif",
      "date": "2026-11-20",
      "changelog": ["…", "…"],
      "assets": [
        {
          "nom": "rose.exe",
          "url": "https://stef302.github.io/Rose/telechargements/rose.exe",
          "sha256": "e9ca6c47a2077e5141d82907375513a9d1cd172ab218b3b07abd186f450b3566",
          "taille": 9230848
        }
      ]
    }
  ]
}
```

Champs :

| Champ | Type | Règle |
|---|---|---|
| `source` | chaîne | toujours `"site"` |
| `genere` | chaîne | date de génération `AAAA-MM-JJ` |
| `versions[]` | tableau | du plus récent au plus ancien |
| `version` | chaîne | numéro sans `v` (`0.1.7`) |
| `tag` | chaîne | tag GitHub (`v0.1.7`) |
| `titre` | chaîne | titre réel de la release GitHub |
| `date` | chaîne | date de publication ISO `AAAA-MM-JJ` (issue de `published_at`) |
| `changelog` | tableau de chaînes | liste de changements, fidèles aux notes de release |
| `assets[]` | tableau | fichiers **réellement présents** dans `docs/telechargements/` |
| `assets[].nom` | chaîne | nom de fichier exact, tel qu'il existe sur disque |
| `assets[].url` | chaîne | URL **HTTPS vers ce site**, jamais `github.com` / `raw.githubusercontent.com` |
| `assets[].sha256` | chaîne | 64 hex minuscules, calculé avec `sha256sum` sur le fichier local |
| `assets[].taille` | entier | taille en octets du fichier local |

État au 21/11/2026 : **3 versions, 12 assets**, tous présents sur disque et
tous vérifiés (`sha256sum -c` : 12/12 OK).

| Version | Date | Titre | Assets |
|---|---|---|---|
| 0.1.7 | 2026-11-20 | Rose v0.1.7 — lanceur définitif | 7 (les 7 assets de la release GitHub, tailles conformes) |
| 0.1.6 | 2026-11-19 | Rose v0.1.6 — launcher Rose | 1 (`Rose-0.1.6-full.nupkg` ; les autres fichiers portent le nom générique de la version courante et ne sont donc pas attribuables) |
| 0.1.5 | 2026-11-14 | Rose v0.1.5 — installeur Windows refait | 4 (nupkg, `Rose-win-Setup.exe`, VSIX, skill zip) |

Règle d'attribution : n'est listé qu'un fichier **présent sur disque**, dont le
nom ou l'appartenance à la release GitHub de ce tag prouve l'appartenance à la
version. Les fichiers au nom générique (`rose.exe`, `rose-wasm.wasm`,
`rose-aarch64-linux`, `rose-setup.exe`, `rose-shell.exe`) ne sont listés que
pour la version courante (0.1.7), où leurs tailles correspondent exactement aux
assets de la release. Rien n'est inventé : un asset absent du site n'est pas
annoncé comme disponible.

## Ajouter une release (procédure)

1. Déposer les binaires dans `docs/telechargements/` (les fichiers doivent
   exister réellement, avec leur nom définitif).
2. Récupérer les métadonnées réelles de la release :
   ```bash
   curl -s "https://api.github.com/repos/Stef302/Rose/releases?per_page=10" \
     -o /tmp/opencode/rose_releases.json
   ```
   Lire `tag_name`, `name`, `published_at`, `assets[].name`, `assets[].size`.
3. Calculer les empreintes et les tailles **sur le fichier local** :
   ```bash
   cd /home/clausake/Stefan/ROSE/docs/telechargements
   sha256sum Rose-0.1.8-full.nupkg rose.exe
   stat -c '%s %n' Rose-0.1.8-full.nupkg rose.exe
   ```
4. Ajouter l'entrée en tête de `versions` dans `releases.json` (champs du
   tableau ci-dessus). Ne jamais mettre d'URL GitHub dans `assets[].url`.
5. Mettre à jour `SHA256SUMS` si de nouveaux fichiers sont déposés :
   ```bash
   cd /home/clausake/Stefan/ROSE/docs/telechargements
   sha256sum * > SHA256SUMS.tmp && mv SHA256SUMS.tmp SHA256SUMS
   ```
   (Ne pas inclure `SHA256SUMS` lui-même dans la liste si l'on veut éviter une
   empreinte auto-référente.)
6. Ajouter l'événement correspondant dans la page `#page-releases` de
   `docs/index.html` : `event-ver`, `event-date`, `event-title`, liste
   `ul/li`, puis les boutons `.btn` avec `href="telechargements/<fichier>"` et
   le SHA-256 complet dans l'attribut `title`. Mettre à jour la mention
   « ACTUELLE ».
7. Ne rien toucher d'autre : `rose/src/**`, `compilateur/**`, `PLAN.md` et les
   docs des autres agents sont hors périmètre. Pas de commit, pas de
   publication depuis cet agent.

## Commandes de vérification

```bash
cd /home/clausake/Stefan/ROSE/docs/telechargements

# 1. JSON valide + cohérence des champs
python3 -c "
import json, os
d = json.load(open('releases.json', encoding='utf-8'))
assert d['source'] == 'site'
for v in d['versions']:
    for a in v['assets']:
        assert os.path.isfile(a['nom']), a['nom']
        assert os.path.getsize(a['nom']) == a['taille'], a['nom']
        assert 'github.com' not in a['url'] and a['url'].startswith('https://stef302.github.io/Rose/telechargements/')
print('JSON valide,', len(d['versions']), 'versions,', sum(len(v['assets']) for v in d['versions']), 'assets')
"

# 2. Fiche de contrôle générée depuis le manifeste, puis vérifiée
python3 -c "
import json
d = json.load(open('releases.json', encoding='utf-8'))
with open('/tmp/opencode/manifest.sha256', 'w') as f:
    for v in d['versions']:
        for a in v['assets']:
            f.write(a['sha256'] + '  ' + a['nom'] + '\n')
"
sha256sum -c /tmp/opencode/manifest.sha256

# 3. Empreintes globales du dossier
sha256sum -c SHA256SUMS 2>&1 | grep -v '^SHA256SUMS: FAILED open or read' || true

# 4. Section HTML présente, sans lien de téléchargement GitHub
cd /home/clausake/Stefan/ROSE
grep -n 'data-page="releases"\|id="page-releases"' docs/index.html
python3 -c "
import re
s = open('docs/index.html', encoding='utf-8').read()
sec = s[s.index('id=\"page-releases\"'):s.index('</main>')]
hrefs = re.findall(r'href=\"([^\"]+)\"', sec)
dl = [h for h in hrefs if h.endswith(('.exe','.nupkg','.wasm','.zip','.vsix','.json','-linux'))]
assert not any('github' in h for h in dl), dl
print('Liens de téléchargement de la section :', len(dl), '| aucun ne pointe vers GitHub')
"

# 5. Après publication par l'orchestrateur seulement :
curl -s https://stef302.github.io/Rose/telechargements/releases.json | python3 -m json.tool >/dev/null && echo "manifeste servi : JSON valide"
```

## Ce que ce dispositif NE fait PAS

- Il ne modifie pas le code du launcher (agent L5) : il fournit seulement le
  contrat de données que le launcher lit.
- Il ne publie rien : la publication GitHub Pages est du ressort de
  l'orchestrateur.
- Il ne garantit pas que les anciens binaires au nom générique soient encore
  présents sur le site : seuls les fichiers réellement présents sont listés.
