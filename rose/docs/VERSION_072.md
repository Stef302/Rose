# VER2 — version 0.1.72 : convention 0.1.7x et launcher version-neutre

> **Constat daté au 21/11/2026**, dépôt `Stef302/Rose`, HEAD `7a5c069` au
> démarrage du chantier. **VER2 n'a exécuté ni commit, ni publication, ni tag.**
> L'orchestrateur a intégré les marqueurs de version dans le commit `772d4af`
> (« Version 0.1.72 : bump intermédiaire partout, site sans 'MOTEUR RUST' »)
> pendant le chantier ; ce document, nouveau, reste non commité.
>
> Périmètre possédé : `rose/Cargo.toml` (ligne `version` uniquement),
> `rose/Cargo.lock`, `README.md`, `docs/**` (hors `docs/telechargements/*`
> binaires), `wikimiroir/**`, et le présent fichier `rose/docs/VERSION_072.md`
> (nouveau).
>
> Hors périmètre, non modifiés : `docs/telechargements/**` (artefacts et
> checksums — publication réservée à l'orchestrateur), `rose/src/**` — dont
> `rose/src/auto_llvm.rs`, déjà modifié par un autre agent à l'ouverture —,
> `rose/tests/**`, `PLAN.md`, `scripts/**`, `installateur/**`, et les autres
> `rose/docs/*.md`.

---

## 1. La convention `0.1.7x`

**`0.1.7x` désigne une mise à jour intermédiaire réelle de la lignée `0.1.7`.**
Le `x` n'est **pas** un chiffre de la notation `0.1.7` : `0.1.72` est une
version **supérieure** à `0.1.7` et doit être comparée comme telle.

- `0.1.7` = version majeure de la lignée, publiée le 20/11/2026, conservée
  **telle quelle** dans l'historique et le changelog (elle n'est pas réécrite
  en 0.1.72).
- `0.1.72` = première mise à jour intermédiaire de cette lignée. C'est la
  **version courante** affichée par les marqueurs d'état du site, du README et
  du wiki, et la version du crate Cargo. Les suivantes (`0.1.73`, …) suivront
  la même règle.
- Conséquence de comparaison : `0.1.72 > 0.1.7 > 0.1.6`. Le code du launcher
  l'implémente déjà : `comparer_versions("0.1.72", "0.1.7") == Greater` et
  `comparer_versions("0.1.71", "0.1.72") == Less` (`rose/src/velopack.rs`,
  tests `comparer_versions` — le tri décroissant du sélecteur donne
  `["0.1.72", "0.1.7", "0.1.3"]`). Un simple tri lexical ou une lecture
  « 0.1.7 + 2 » serait faux.

### Règle (a) / (b) appliquée

- **(a) État courant affiché** — badge, bandeau d'accueil, statistique,
  sélecteur avec mention « actuelle », liens de release courants, descriptions
  du moteur dans la documentation, README, wiki, version Cargo : **0.1.72**.
- **(b) Historique daté** — changelog du site, notes archivées, noms
  d'artefacts réellement publiés, identité des extensions VS Code déjà
  publiées : **reste 0.1.7 / 0.1.6 / 0.1.5**. En cas de doute : conserver et
  documenter. Aucun hash n'a été inventé.

---

## 2. Launcher version-neutre, sélecteur dynamique

Le launcher ne porte **aucun numéro de version codé en dur** dans son
interface : la version installée est lue dans l'**état local** (`current` /
dossiers `app-<v>`), jamais dans `CARGO_PKG_VERSION`
(`rose/src/velopack.rs` : « *Version réellement installée (état local, pas
`CARGO_PKG_VERSION`)* », « *JAMAIS de `CARGO_PKG_VERSION`* »).

Le **sélecteur de versions** est alimenté dynamiquement, par ordre de priorité :

| Ordre | Source | Rôle |
|---|---|---|
| 1 | Manifeste du site `https://stef302.github.io/Rose/telechargements/releases.json` | source de vérité, cache frais puis une requête |
| 2 | API GitHub `repos/Stef302/Rose/releases` | repli si le manifeste est absent/invalide |
| 3 | État local (`versions-cache.json`, `site-releases-cache.json`, dossiers `app-<v>`) | hors ligne explicite |

La banderole `Nouvelle version disponible : 0.1.7 → 0.1.72` est donc calculée
à partir de deux données réelles ; elle s'active **sans recompiler le launcher**
dès que le manifeste publie `0.1.72`. Le site reste la source officielle des
releases ; GitHub n'est qu'un miroir (`docs/index.html`, section Releases).

Le sélecteur de version de la **documentation** (`docs/index.html`, `#docVer`)
suit la même logique de marquage : l'option `v0.1.72` est `selected` et porte
seule la mention « actuelle » ; `v0.1.7` est conservée en option historique,
et `setDocVer()` a été adapté (`ver >= 17` = actuelle, `ver === 16` =
historique). Le filtre `data-min` reste inchangé : les deux versions affichent
tout le contenu.

---

## 3. Marqueurs modifiés — avant → après

| Occurrence | Classe | Avant | Après |
|---|---|---|---|
| `rose/Cargo.toml:3` | (a) | `version = "0.1.7"` | `version = "0.1.72"` |
| `rose/Cargo.lock:477` (`[[package]] rose`) | (a) | `version = "0.1.7"` | `version = "0.1.72"` |
| `README.md:9` badge shields.io | (a) | `version-0.1.7` | `version-0.1.72` |
| `docs/index.html:494` bandeau d'accueil | (a) | `MOTEUR NATIF — V0.1.7` | `MOTEUR NATIF — V0.1.72` |
| `docs/index.html:511` badge | (a) | `v0.1.7` | `v0.1.72` |
| `docs/index.html:527` statistique | (a) | `0.1.7` | `0.1.72` |
| `docs/index.html:648` lien de release | (a) | `releases/tag/v0.1.7` | `releases/tag/v0.1.72` |
| `docs/index.html:781` sélecteur | (a) | `<option value="16" selected>v0.1.7 — actuelle</option>` | `<option value="17" selected>v0.1.72 — actuelle</option>` + `<option value="16">v0.1.7</option>` conservée |
| `docs/index.html:824` doc | (a) | `Le moteur (Rust v0.1.7, licence MIT)` | `v0.1.72` |
| `docs/index.html:996` doc | (a) | `fonctions du moteur v0.1.7` | `v0.1.72` |
| `docs/index.html:1047` doc | (a) | `Le moteur (Rust v0.1.7)` | `v0.1.72` |
| `docs/index.html:1082` doc | (a) | `<h3>Moteur Rust (v0.1.7)</h3>` | `v0.1.72` |
| `docs/index.html:1507-1509` `setDocVer()` | (a) | `ver >= 16` = actuelle | `ver >= 17` = actuelle, `ver === 16` = historique |
| `wikimiroir/Home.md:8` | (a) | `versioned as **0.1.7**` | `versioned as **0.1.72** (an intermediate update of the 0.1.7 line)` |
| `wikimiroir/CLI.md:48` titre de la console | (a) | `Rose 0.1.7` | `Rose 0.1.72` |

Le numéro de ligne de `Cargo.lock` est celui du bloc `[[package]] name =
"rose"` ; aucun autre `0.1.7` du lockfile n'a été touché.

---

## 4. `0.1.7` restants dans le périmètre (tous justifiés)

| Occurrence | Raison de conservation |
|---|---|
| `docs/index.html:659`, `681`, `683` | Identité de l'extension VS Code **déjà publiée** (`rose-language-0.1.7.vsix`, Marketplace `0.1.7`). Artefact binaire non possédé : la republier relève de l'orchestrateur. |
| `docs/index.html:782`, `1509` | Option historique du sélecteur et sa note JS (`v0.1.7`, plus « actuelle »). |
| `docs/index.html:1320`, `1324`, `1340` | Entrée du **changelog** datée « 20 NOVEMBRE 2026 » et son artefact `Rose-0.1.7-full.nupkg` : historique, explicitement hors périmètre. |
| `rose/src/velopack.rs`, `rose/src/setup_gui.rs`, `rose/src/setup_core.rs` (fixtures de test `v0.1.6` / `0.1.7`) | Tests du launcher : `velopack` et `setup_gui` utilisent des fixtures `0.1.6`. Non modifiés, pour ne pas les casser. |

---

## 5. Vérification (commandes réellement exécutées)

- **`cargo build`** (`~/.cargo/bin/cargo build`, depuis `rose/`) : **succès**,
  `Finished \`dev\` profile ... in 7.94s`, `rc=0`. Uniquement des avertissements
  préexistants (`dead_code` dans `paint.rs`, `linker_messages` sur `dlopen` /
  glibc statique) ; **0 erreur**.
- **`cargo test` — passage 1** (manifeste complet, avant l'édition concurrente
  décrite ci-dessous) : **`rc=0`, `0 failed` sur toutes les cibles**, dont
  `rose-shell` 6/6, `grands_nombres` 8/8, `mul_cmp` 1/1 et les doc-tests 0/0.
  Les tests `velopack` / `setup_gui` avec fixtures `0.1.6` sont passés.
- **`cargo test` — passage final** (après création du fichier TRAY) :
  bibliothèque `275 passed; 0 failed; 3 ignored`, second binaire d'unités
  `262 passed; 0 failed; 3 ignored`, `rose-shell` `6 passed; 0 failed`,
  doc-tests `0 failed` — soit **543 tests réussis, 0 échec sur tout le code
  préexistant**. Le seul échec du passage final est
  `tests::structure_un_timer_une_requete_aucun_polling` dans le **nouveau**
  `rose/src/bin/rose-tray.rs:418` (`left: 2, right: 1`), c'est-à-dire le travail
  **en cours de l'agent TRAY**, fichier hors périmètre de VER2 et non existant
  lors du passage 1. VER2 n'a ni lu pour écriture ni modifié ce fichier, ni
  l'entrée `[[bin]] name = "rose-tray"` ajoutée simultanément à `Cargo.toml`
  (sa ligne `version = "0.1.72"` est restée intacte).
  Un passage intermédiaire avait aussi échoué en **résolution de cible**
  (`can't find bin 'rose-tray' ...`) pendant la fenêtre où la déclaration
  existait sans le fichier source — même cause concurrente.
- **`cargo test` — passage de clôture** (après correction du test par l'agent
  TRAY) : **`rc=0`, 8 cibles `test result: ok`, 556 tests réussis, 0 échec,
  6 ignorés** (`275 + 262 + 0 + 6 + 4 + 8 + 1 + 0`). C'est l'état final vérifié.
- **Point fixe `b3ed9262` ×2** — exécuté depuis la racine du dépôt avec le
  binaire `rose/rose` :
  - `rose/rose rir compilateur/emetteur.rose` → `rc=0`, md5
    `b3ed9262dd54776a6dbd07bc12368d0e`, 263 520 o, stderr 0 o ;
  - `rose/rose compilateur/emetteur.rose compilateur/emetteur.rose` → `rc=0`,
    md5 `b3ed9262dd54776a6dbd07bc12368d0e`, 263 520 o, stderr 0 o.
  **Deux dumps identiques au même hash attendu (`cmp` octet pour octet).**
