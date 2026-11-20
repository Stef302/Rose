# Sélecteur de versions du launcher — contrat d'API pour l'interface (LAUNCH2)

**Mission LAUNCH1 (backend uniquement).** Fichiers modifiés :
`rose/src/velopack.rs`, `rose/src/setup_core.rs` et le présent document.
**Aucune modification** de `setup_gui.rs`, `run.rs`, `front.rs`, `rir.rs`,
`main.rs`, `Cargo.toml` ni `PLAN.md`. Aucun commit, aucune publication.

Date des mesures : **21/11/2026**. Binaire isolé de validation :
`/tmp/opencode/launch1_agents/release/rose`.

---

## 1. Principe

Le launcher ne contient **aucune version en dur**. Il n'interroge qu'une
source : l'API publique des releases GitHub
`GET https://api.github.com/repos/Stef302/Rose/releases?per_page=100`
(`velopack::lister_versions()`, lecture seule — `gh api` si disponible, sinon
`curl`). Le catalogue fusionne cette réponse avec l'état disque local
(`versions_installees()` = dossiers `app-<v>`, `versions_cachees()` =
manifeste `releases.win.json`). Les seules versions littérales du code se
trouvent dans les **fixtures de tests**.

---

## 2. API publique à brancher dans l'UI

Trois fonctions, toutes dans `crate::setup_core` (ponts fins vers
`crate::velopack` où vit la logique) :

```rust
pub fn launcher_catalogue() -> crate::velopack::CatalogueVersions;
pub fn launcher_installer_version(tag: &str) -> crate::velopack::StatutVersion;
pub fn launcher_selectionner_version(tag: &str) -> crate::velopack::StatutVersion;
```

Les quatre ponts historiques (`launcher_versions`, `launcher_installer`,
`launcher_selectionner`, `launcher_dry_run`) restent inchangés pour ne pas
casser les appelants existants.

### 2.1 `launcher_catalogue()` — à appeler en entrant dans `Vue::MisesAJour`

Ne renvoie **jamais `Err`**. Même si GitHub est inaccessible, l'UI reçoit un
catalogue exploitable avec `hors_ligne = true` et un message explicite.

```rust
pub struct CatalogueVersions {
    pub versions: Vec<EntreeCatalogue>, // TRIÉES par version DÉCROISSANTE
    pub hors_ligne: bool,               // true = GitHub n'a pas répondu
    pub message: String,                // toujours renseigné, même si liste vide
}

pub struct EntreeCatalogue {
    pub tag: String,            // "v0.1.6" (ou la version seule si entrée locale)
    pub version: String,        // "0.1.6" — sans préfixe v, sert au tri et aux dossiers
    pub titre: String,          // titre GitHub, ou "Version locale <v>"
    pub date: String,           // ISO-8601 « 2026-11-19T13:54:57Z », vide si inconnue
    pub installable: bool,      // un paquet complet <v>-full.nupkg est annoncé en ligne
    pub en_cache: bool,         // manifeste releases.win.json déjà sur disque
    pub deja_installee: bool,   // dossier app-<v> présent : bascule sans réseau possible
}
```

Tri : `comparer_versions` sur `version`, segments numériques, décroissant.
Dédup : une seule entrée par version (l'entrée distante gagne sur une entrée
locale ou un doublon de tag). Le tri est stable.

**Sémantique des actions côté UI** (à respecter) :

| `installable` | `deja_installee` | Action à proposer |
|---|---|---|
| `true` | `false` | « Installer » → `launcher_installer_version(tag)` |
| `true` | `true` | « Réinstaller » → `launcher_installer_version(tag)`, ou « Utiliser » → `launcher_selectionner_version(tag)` |
| `false` | `true` | « Utiliser » → `launcher_selectionner_version(tag)` (ancien installeur legacy ou version locale) |
| `false` | `false` | Afficher grisé, sans action (aucune artefact installable) |

`en_cache` signifie **seulement** que le manifeste est connu localement : le
`.nupkg` est supprimé après extraction. Ce n'est PAS une garantie de
réinstallation hors ligne. Ne pas afficher « prêt à installer » sur cette base.

### 2.2 `launcher_installer_version(tag)` — téléchargement + SHA-256

```rust
pub struct StatutVersion {
    pub etat: EtatOperation,  // Reussi | Refuse | Echec
    pub version: String,      // version sans préfixe v
    pub message: String,      // phrase courte, directement affichable
}
pub enum EtatOperation { Reussi, Refuse, Echec }
impl StatutVersion { pub fn ok(&self) -> bool { /* etat == Reussi */ } }
```

Déroulement :
1. `Refuse` si le tag est **absent du catalogue** ou si l'entrée n'est pas
   `installable`. Ce refus intervient avant tout téléchargement.
2. Sinon `velopack::telecharger_version(tag)` : manifeste du tag, paquet
   complet, **SHA-256 vérifié** (hash absent du manifeste = refus ; hash
   différent = refus + suppression du fichier téléchargé), extraction vers
   `app-<version>`, écriture de `current`.
3. `Reussi` avec la version installée, ou `Echec` avec le message d'erreur
   (réseau, hash, disque). L'UI n'a qu'un seul canal à gérer.

### 2.3 `launcher_selectionner_version(tag)` — bascule sans réseau

Même type de retour. Vérifie le dossier `app-<version>` sur disque, écrit
`current`, met à jour l'association `.rose` (silencieusement). Aucun appel
réseau, aucun téléchargement. `Refuse` si le dossier n'existe pas.

---

## 3. Mode hors ligne

`launcher_catalogue()` appelle `lister_versions()`. Si celle-ci échoue
(API inaccessible, réponse vide), `assembler_catalogue` construit le catalogue
**uniquement depuis l'état disque** via `catalogue_local()` :

- `hors_ligne = true` ;
- `message` contient la cause (ex. `GitHub inaccessible (HTTP 000) : voici les
  versions déjà présentes sur cet appareil. Le téléchargement sera impossible
  tant que l'API ne répond pas.`) ;
- les entrées viennent des dossiers `app-*` et du manifeste en cache, triées,
  toutes avec `installable = false`, `date = ""` et le bon drapeau
  `deja_installee` / `en_cache` ;
- si **rien** n'est présent sur disque, la liste est vide MAIS `message`
  n'est jamais vide : l'UI affiche toujours l'explication, jamais une liste
  vide muette.

Conséquence pour l'UI : si `hors_ligne`, proposer uniquement la bascule
(`deja_installee`) et désactiver « Installer », avec le `message` en bandeau.

---

## 4. Preuve API GitHub réelle (lecture seule)

Deux instruments, même salve du 21/11/2026 :

1. `curl -H 'Accept: application/vnd.github+json'
   https://api.github.com/repos/Stef302/Rose/releases?per_page=100` →
   **HTTP 200**, **7 releases** brutes : `v0.1.6`, `v0.1.5`, `v0.1.4`,
   `v0.1.3`, `v0.1.2`, `v0.1.1`, `v0.1.0` (dates `published_at` de
   `2026-11-13T23:13:21Z` à `2026-11-19T13:54:57Z`). Les trois dernières
   (`v0.1.2`, `v0.1.1`, `v0.1.0`) n'ont **aucun asset**.
2. Test réel `cargo test --release launcher_api_reelle -- --ignored
   --nocapture` → **1 passed**, sortie de `lister_versions()` :
   **4 releases Windows** retenues :

   | tag | version | installable | artefact |
   |---|---|---|---|
   | `v0.1.6` | 0.1.6 | true | `Rose-0.1.6-full.nupkg` |
   | `v0.1.5` | 0.1.5 | true | `Rose-0.1.5-full.nupkg` |
   | `v0.1.4` | 0.1.4 | true | `Rose-0.1.4-full.nupkg` |
   | `v0.1.3` | 0.1.3 | false | `Rose-Setup-0.1.3.exe` (legacy) |

Compte GitHub vérifié avant toute lecture : `GH_TOKEN=$(cat
~/.config/opencode/.gh_token) gh api user -q .login` → **`Stef302`**. Le CLI
`gh` nu est connecté comme `clausake` : c'est pourquoi la preuve tokenisée
passe par `GH_TOKEN` et que l'API est de toute façon **publique**. Aucune
écriture, aucun tag, aucune release, aucun asset n'a été créé ou modifié.

---

## 5. Validation (binaire isolé)

| Contrôle | Résultat mesuré |
|---|---|
| `CARGO_TARGET_DIR=/tmp/opencode/launch1_agents cargo build --release --bin rose` | **OK** |
| `cargo test --release` | **515 passed, 0 failed, 3 ignored** (258 + 242 + 6 + 8 + 1) |
| Jardin `jardins/tests/premiers.rose` | **20 tests, 0 échec** |
| `fib(25)` | **75025** |
| Point fixe `rir` ↔ front-end | `b3ed9262dd54776a6dbd07bc12368d0e` **×2** (md5 égaux) |
| Banc 9 domaines × 9 tirages × c6/c16 (`multi_valeurs.py`, base `speed1_agents/rose_base`) | voir § 7 |

Une salve intermédiaire a affiché un échec isolé de
`gui_native::tests::machine_a_etats_simulee` (`left: 1, right: 0` sur un
compteur de fenêtres global, concurrent avec les autres tests du même module).
Ce test, dans un fichier **non modifié par LAUNCH1**, passe 3/3 isolément et
la suite complète est repassée à **0 échec** deux fois ensuite. Je ne le
présente pas comme un échec de cette mission, mais je le consigne.

**Écart factuel avec la consigne** : la consigne annonçait `499` tests. Le
comptage réel après nos ajouts est **515** (`0` échec). Les 8 tests ajoutés par
LAUNCH1 sont 6 dans `velopack::tests` et 2 dans `setup_core::tests` ; le
différentiel restant vient d'ajouts concurrents d'autres agents dans le même
arbre de travail. Aucun test n'a été supprimé ni désactivé.

---

## 6. Ce qui reste à brancher (LAUNCH2, `setup_gui.rs`)

1. Dans `Vue::MisesAJour` (déjà **premier écran** de l'énumération, ajouté en
   fin pour ne pas décaler `ETAPES`), appeler `launcher_catalogue()` une fois
   à l'entrée et stocker le résultat dans `Etat` (par exemple
   `catalogue: Option<CatalogueVersions>` + index de sélection).
2. Afficher une ligne par `EntreeCatalogue` : `version`, `titre`, `date`
   (ou « date inconnue » si vide), et un badge d'état à partir des trois
   drapeaux. Afficher `catalogue.message` en bandeau, avec un style distinct
   si `hors_ligne`.
3. Sur clic « Installer » appeler `launcher_installer_version(tag)`, sur
   « Utiliser » appeler `launcher_selectionner_version(tag)` ; écrire
   `statut.message` dans `etat.statut`, avec `etat.occupe` pendant l'action.
   Ne rien proposer pour `installable = false && deja_installee = false`.
4. Après succès, rafraîchir `etat.version_installee` (comme le fait déjà
   l'écran à la ligne ~2816) et reconstruire le catalogue pour refléter le
   nouveau `app-<version>` / `current`.

Aucun de ces branchements n'a été fait dans LAUNCH1 : `setup_gui.rs` est
intact de notre part.

---

## 7. Résultat du banc 9 domaines

Harnais `/tmp/opencode/val1_agents/multi_valeurs.py` recopié dans
`/tmp/opencode/launch1_agents/`, avec `FIN` pointant sur le binaire isolé
**final** (`sha256 1b4b458be999913daf772dec…`) et `BASE` sur
`/tmp/opencode/speed1_agents/rose_base`. 9 programmes × 9 tirages × 2 cœurs :

- `debut 2026-11-20T12:38:02Z`
- `c16: 81/81 VALEURS identiques`
- `c6: 81/81 VALEURS identiques`
- `TOTAL: 162/162`, `rc=0`, `fin 12:40:23Z`
- log : `/tmp/opencode/launch1_agents/multi_valeurs_final.log`

Une première salve (12:33Z, binaire juste avant le dernier ajustement du
message hors ligne) avait également donné 162/162. Aucun fichier du moteur
(`jit.rs`, `rir.rs`, `front.rs`, `interp.rs`, …) n'a été modifié par LAUNCH1 :
le seul changement porte sur les deux modules du launcher et un document,
donc aucun chemin de performance du moteur n'est affecté. Ce banc mesure
l'identité des **valeurs** de sortie, pas des temps : je ne revendique aucun
gain ni aucune mesure de performance nouvelle.
