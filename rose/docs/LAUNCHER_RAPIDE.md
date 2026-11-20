# Lanceur neutre et sélecteur de versions rapide (L4)

État : **terminé et vérifié**. Dépôt `/home/clausake/Stefan/ROSE`, HEAD `df41f5d` au départ.
Jour : 21/11/2026 (les horodatages machine ci-dessous sont en UTC, 20/11 vers 20 h).
Fichiers modifiés : `rose/src/setup_gui.rs`, `rose/src/setup_core.rs`,
`rose/src/velopack.rs` et le présent document. Aucun commit, aucune publication.

## 1. Neutralité du lanceur

Le lanceur est un **lanceur + installeur** : il peut installer n'importe quelle version
publiée sur `Stef302/Rose`. Il n'a pas d'identité de version.

| Avant | Après |
|---|---|
| Barre de titre : `Rose 0.1.7 — Programme d'installation` | `Rose — Lanceur` (2 emplacements : création et chaque `rafraichir`) |
| Pastille : `v0.1.7 · x64` | `LANCEUR · x64` |
| Sous-titre : `— Programme d'installation` | `— Lanceur` |
| Titre de section : `Versions` | `Sélecteur de versions` |
| Carte « installation détectée » : `Version … — identique au paquet…` | `Une installation existe déjà — choisissez une version dans le sélecteur.` |
| Écran Bienvenue : `version 0.1.5 dans …` | `Une installation existante a été détectée dans …` |
| Écran Terminé : `Rose 0.1.7 est installé` | `Rose est installé.` (numéro supprimé, reste le dossier et `rose --version`) |
| Registre `HKCU\Software\Rose\Version = <version du launcher>` | Clé supprimée (aucune lecture dans le code) |
| Profil Windows Terminal `tabTitle: "Rose <version du launcher>"` | `tabTitle: "Rose"` |
| Repli `version_installee().unwrap_or(env!("CARGO_PKG_VERSION"))` (4 endroits : 2 dans le GUI, 2 dans Velopack) | `unwrap_or_default()` : on ne fabrique jamais une version |

Le numéro de version n'apparaît plus que comme **donnée dynamique** :
- liste affichée dans le sélecteur (API GitHub, ou cache de cette API) ;
- titre de release affiché sur la même ligne.

Audit factuel (production uniquement, avant `#[cfg(test)]`) :

```
setup_gui.rs  : 0 occurrence de x.y.z ou CARGO_PKG_VERSION
setup_core.rs : 0 occurrence
velopack.rs   : 0 occurrence
```

Les fixtures de tests conservent volontairement des tags littéraux (`v0.1.5`, …) :
ce sont des données de test, jamais du texte produit.

## 2. Mécanisme de cache (rapidité)

### Fichier

`%LOCALAPPDATA%\Rose\versions-cache.json` (ou `ROSE_VELOPACK_ROOT` sous les tests) :

```json
{"horodatage":"1732200000","brut":"[ {\"tag_name\":\"v…\", …} ]"}
```

`brut` est la **réponse brute de l'API GitHub**, échappée en JSON. Le cache ne contient
donc aucune version inventée : il ne rejoue que ce que GitHub a déjà renvoyé.
S'il est illisible, vide ou ne donne aucune release installable, il est ignoré
(`None`) et aucun catalogue n'est fabriqué.

### API ajoutée

- `velopack::ecrire_cache_versions(json)` → écrit après chaque réussite de `lister_versions()` ;
- `velopack::lire_cache_versions()` → `(horodatage, versions)` ou `None` ;
- `velopack::cache_versions_perime(cache)` → vrai après **TTL = 300 s** ;
- `velopack::formater_date_cache(horodatage)` → `2026-11-20 20:09 UTC` ;
- `setup_core::launcher_catalogue_cache()` → catalogue affichable **sans réseau** ;
- `setup_core::launcher_cache_a_actualiser()` → décide du rafraîchissement.

### Parcours UI

1. `ouvrir_versions()` → `charger_catalogue_versions(h, force = false)`.
2. Le cache est lu et le catalogue posé dans `ETAT` **avant tout appel réseau**, puis
   `rafraichir(h)` peint immédiatement la liste. Le bandeau porte `Liste du <date>`,
   ou `Liste du <date> — liste périmée, actualisation en cours`.
3. Un thread appelle `setup_core::launcher_catalogue()` **uniquement** si le cache est
   absent/périmé (ou si l'utilisateur clique « Rafraîchir » → `force = true`).
   Il ne touche jamais à `ETAT` : il dépose le résultat dans un `Mutex`, puis poste
   `WM_APP_VERSIONS` ; la boucle de messages intègre le résultat et repeint.
   Le fil de messages n'est donc jamais bloqué, même hors ligne.
4. Si le rafraîchissement échoue et qu'un cache existe, la liste du cache reste
   affichée avec `hors_ligne = true` et un message explicite ; l'action « Installer »
   est désactivée, la bascule vers une version déjà présente reste possible.

### Timeout réseau

- `curl -sSL --fail --connect-timeout 2 --max-time 3` plus un surveillant
  `try_wait()` / `kill()` à **3 000 ms** (sortie redirigée vers un fichier temporaire :
  `Command::output()` bloquerait sur les > 100 Kio de la réponse GitHub).
- `gh api` n'est plus utilisé pour la liste : il n'a pas d'option `--timeout`
  (`gh api --help` vérifié) et l'API est publique. Cela supprime ~350 ms de démarrage.

## 3. Mesures réelles (5 tirs, temps jusqu'au premier affichage)

Harnais : `/tmp/opencode/launch4_agents/harness` (avant) et
`.../harness_after` (après), project compilés contre la vraie bibliothèque `rose`,
profil `ROSE_VELOPACK_ROOT` isolé, binaire `target/release/rose` reconstruit.
Méthode : chronomètre `Instant` autour de l'appel qui précède le premier peinture
de la liste (`launcher_catalogue()` avant, `launcher_catalogue_cache()` après).

| Série | t1 | t2 | t3 | t4 | t5 | médiane |
|---|---:|---:|---:|---:|---:|---:|
| **Avant** (appel synchrone GitHub) | 581,650 | 637,495 | 517,262 | 590,177 | 561,173 | **581,650 ms** |
| **Après, cache chaud** (ouverture) | 0,508 | 0,420 | 0,415 | 0,400 | 0,400 | **0,415 ms** |
| **Après, sans cache** (1re ouverture) | 485,431 | 202,029 | 204,388 | 203,173 | 203,626 | **203,173 ms** |
| **Après, rafraîchissement de fond** | 202,866 | 202,904 | 203,047 | 201,891 | 202,032 | **202,866 ms** (hors chemin d'affichage) |

- Objectif < 100 ms avec cache chaud : **atteint**, avec ~240× de marge sur la cible
  et ~1 400× sur l'avant (médiane 581,65 ms → 0,415 ms).
- Le rafraîchissement de fond (~203 ms) se fait **après** l'affichage : il n'entre
  pas dans le temps d'attente de l'utilisateur.

### Cas hors ligne (mesurés)

| Scénario | Résultat |
|---|---|
| API refusée (`127.0.0.1:9`), cache présent | 5 versions affichées, `hors_ligne = true`, message `GitHub inaccessible (curl code exit status: 7) : liste du 2026-11-20 20:03 UTC déjà récupérée… Le téléchargement est impossible…` (20,9 ms) |
| API qui accepte mais ne répond jamais | **3 007 ms** puis même message clair, cached list conservée ; l'UI était déjà affichée |
| Cache corrompu (`"brut":""`) | `lire_cache_versions()` = `None`, aucun catalogue inventé |

## 4. Validation (commandes et preuves)

| Contrôle | Résultat |
|---|---|
| `cargo build --release` (cible native aarch64) | OK |
| `cargo check --release --target x86_64-pc-windows-gnu` (cible du GUI) | RC 0 — le code Win32 modifié compile |
| `cargo test --release` | **515 passés, 0 échec** (258 + 242 + 6 + 8 + 1 ; 3 + 3 + 1 ignorés, tests réseau/réels préexistants) |
| Tests velopack existants | verts, y compris `launcher_selection_et_cache` étendu (cache frais/périmé/corrompu, échappement, date) |
| Jardin `jardins/tests/premiers.rose` | `20 tests, 0 échec` |
| `fib(25)` | `75025` |
| Point fixe (2 voies) | `b3ed9262dd54776a6dbd07bc12368d0e` ×2 |
| 9 domaines, c16 et c6, 9 tirages | `81/81` + `81/81` = **162/162** valeurs identiques |
| Audit versions littérales en production | 0/0/0 dans les 3 fichiers |

Le binaire Linux ne peut pas exécuter `setup_gui.rs` (`#[cfg(windows)]`) : le chemin
GUI est validé par compilation croisée, et toute la logique de données est validée
par les tests et les harnais ci-dessus.
