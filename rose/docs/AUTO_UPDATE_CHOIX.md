# Choix du mode de mise à jour — premier écran

**Constat daté : 19/11/2026.** Agent RE2, dépôt `Stef302/Rose`, HEAD de départ
`5665bf7`. Ce document décrit uniquement ce qui a été ÉCRIT et VÉRIFIÉ ici.
Aucune capture d'écran Windows n'a été prise ni inventée : ce machine est Linux,
aucun écran Windows n'a été observé.

---

## 1. Ce qui a été ajouté

### 1.1 Premier écran (avant toute installation)

`rose/src/setup_gui.rs` : nouvelle vue `Vue::MisesAJour`, **premier écran**
affiché par `rose-setup` (`lancer()` : `vue: Vue::MisesAJour`), avant
`Bienvenue`, `Licence MIT`, `Options`, `Installation`, `Terminé`.

La vue est ajoutée **en fin d'énumération** (`Path` reste 5, `MisesAJour` = 6)
pour ne pas décaler les valeurs numériques des cinq étapes : le panneau latéral
compare `etat.vue as usize` à l'index d'étape. Aucune étape n'est donc
incorrectement cochée ; le nouveau pré-écran n'apparaît pas dans la colonne
des cinq étapes d'installation (comme l'outil PATH).

Trois cartes radio cliquables, avec le thème Rose :

| Option | Valeur `config.json` | Comportement |
|---|---|---|
| **Auto au démarrage** (recommandé) | `"auto"` | Vérifie les versions à chaque lancement ; **ne télécharge qu'après accord** (bouton « Télécharger »). |
| **Manuel** | `"manuel"` | Aucune vérification automatique ; seulement `rose update` ou le bouton « Vérifier les mises à jour ». |
| **Désactivées** | `"desactive"` | Aucune vérification, y compris `rose update` (message explicite). |

Détails d'implémentation :

- indicateur de sélection = cercle + **coche antialiasée** (`coche()`, le correctif
  GDI+ de LA, avec repli GDI) en `VERT_LIME_VIF` (`0x9CFF00`) ;
- accent des cartes : `VERT_LIME_VIF` pour la carte choisie, `LIGNE_FORTE`
  pour les autres ; le bandeau d'accent `ROUGE_VIF` (`0xFF2D1A`) reste le motif
  des cartes et le bouton principal « Continuer » est `ROUGE_VIF` (pied de page) ;
- pied de page : **« Annuler »** (Vert Lime) / **« Continuer »** (`Action::Continuer`,
  rouge vif) — « Continuer » appelle `velopack::ecrire_config` **avant**
  de passer à `Vue::Bienvenue` ;
- un clic sur une carte change le mode affiché ; un clic sur le bouton de la
  ligne de statut lance soit une vérification explicite, soit le téléchargement
  consenti, **dans un fil de fond** (aucun blocage de l'interface) ;
- ⚠ **aucun service Windows, aucune tâche planifiée, aucune clé HKLM** n'est
  créé. Le choix est un simple fichier sous le profil utilisateur.

### 1.2 Persistance

`rose/src/velopack.rs` (parseur JSON maison déjà présent, `valeur()`, aucun
crate externe) :

```rust
pub enum ModeMiseAJour { Auto, Manuel, Desactive }
pub struct ConfigMiseAJour { pub mode: ModeMiseAJour }
pub const MODE_PAR_DEFAUT: ModeMiseAJour = ModeMiseAJour::Auto;
pub fn chemin_config() -> PathBuf
pub fn parser_config(json: &str) -> ConfigMiseAJour
pub fn lire_config() -> ConfigMiseAJour
pub fn ecrire_config(mode: ModeMiseAJour) -> Result<PathBuf, String>
```

Emplacement : **`%APPDATA%\Rose\config.json`** sur Windows (équivalent HKCU,
aucun droit administrateur). `ROSE_CONFIG_DIR` redirige le dossier (tests /
profils nomades) ; à défaut d'APPDATA, repli sur la racine Velopack
(`ROSE_VELOPACK_ROOT` / `%LOCALAPPDATA%\Rose`), puis sur `config.json` relatif.

Format (3 lignes, `"mode"` seul champ) :

```json
{
  "mode": "auto"
}
```

Valeurs acceptées : `auto`/`automatique`/`startup`, `manuel`/`manual`,
`desactive`/`désactivé`/`off`/`none` (insensible à la casse, Unicode —
`to_lowercase()` ; mesuré : `to_ascii_lowercase()` rejetait « DÉSACTIVÉ »).
**Toute valeur absente, invalide, ou fichier illisible → `Auto`** (jamais
d'erreur fatale) : un défaut « désactivé » casserait `rose update` sur machine
neuve.

### 1.3 Effet réel au démarrage

Tout est dans `velopack.rs` (les ponts `setup_core::launcher_*` existants
restent inchangés) :

- `doit_verifier_au_demarrage()` → `true` **uniquement** en mode `Auto` ;
- `verifier_au_demarrage()` → en `Auto`, lit le manifeste et renvoie
  `Some(version)` **sans rien télécharger** ; en `Manuel`/`Désactivé`, renvoie
  `Ok(None)` **sans aucun appel réseau** ;
- `verifier_manifeste()` → vérification explicite du bouton « Vérifier les
  mises à jour », refusée en mode `Désactivé` ;
- `check_and_update(silencieux)` → **garde-fou de mode ajouté** : en
  `Désactivé`, retour immédiat `Err("mises à jour DÉSACTIVÉES … même depuis
  « rose update » …")`, **avant** toute lecture du manifeste. C'est le point
  d'appel de `rose update` (`src/main.rs`, non possédé par cet agent, donc
  inchangé) : le message explicite remonte à l'utilisateur. En `Manuel`
  l'appel explicite est autorisé (c'est l'action de l'utilisateur) ; en `Auto`
  il correspond au téléchargement **consenti** par le bouton de l'UI.

Câblage GUI : `lancer()` appelle `doit_verifier_au_demarrage()` puis
`lancer_verification_maj(hwnd, false)` ; le résultat est affiché dans la ligne
de statut du premier écran via `WM_APP_MAJ`. « Télécharger » est le seul
chemin qui appelle `check_and_update` depuis cet écran.

⚠ Limite factuelle : `src/main.rs` (possédé par RE1) préfixe l'erreur par
« Velopack : … » et ajoute « — repli sur l'installeur classique. » alors qu'il
fait `return` juste après ; le texte de refus explicite est bien affiché, mais
le suffixe « repli » est une formulation héritée que cet agent ne pouvait pas
modifier.

---

## 2. Preuves exécutées (19/11/2026, sur cette machine Linux)

### 2.1 Validation Rust

Commandes dans `/home/clausake/Stefan/ROSE/rose` avec
`export PATH="$HOME/.cargo/bin:$PATH"` :

```
cargo check --release --offline     → Finished `release` profile, EXIT 0
cargo test --release --offline      → 0 échec (détail ci-dessous)
```

Sortie exacte de `cargo test --release --offline` (grep des lignes de résultat) :

```
running 223 tests → test result: ok. 220 passed; 0 failed; 3 ignored (lib)
running 207 tests → test result: ok. 204 passed; 0 failed; 3 ignored (bin rose)
running   6 tests → test result: ok.   6 passed; 0 failed
test result: ok. 1 passed; 0 failed  (test d'intégration)
```

Les 3 tests ignorés sont les tests réseau préexistants (`#[ignore]`), pas
des tests de cette mission.

### 2.2 Tests de persistance (nouveaux, dans `velopack.rs`)

- `parser_config_reconnait_les_trois_modes` — les 3 valeurs canoniques + alias
  (`DÉSACTIVÉ`, `MANUAL`, `Automatic`).
- `parser_config_valeurs_invalides_retombent_sur_le_defaut` — `""`, `{}`,
  texte libre, `{"mode":"inconnu"}`, `{"mode":42}`, `{"autre":"auto"}`,
  `{"mode":null}` → tous `Auto`.
- `config_persistance_et_effet_au_demarrage` — **un seul test pose
  `ROSE_CONFIG_DIR` / `ROSE_VELOPACK_URL`** (les tests Rust sont parallèles ;
  il ne touche PAS `ROSE_VELOPACK_ROOT`, déjà utilisé par
  `launcher_selection_et_cache` — une collision mesurée a donné
  « LOCALAPPDATA indisponible » et a été corrigée) :
  1. absence de fichier → défaut `Auto` ;
  2. aller-retour `ecrire_config` → fichier contient
     `"mode": "<valeur>"` → `lire_config` renvoie le même mode, pour les 3 ;
  3. écriture invalide → relecture tolérante sur `Auto` ;
  4. **Manuel** : `doit_verifier_au_demarrage() == false`,
     `verifier_au_demarrage() == Ok(None)` (zéro appel ; manifeste absent),
     mais `verifier_manifeste()` et `check_and_update()` **tentent** la lecture
     (Erreur locale attendue) → la vérification explicite est autorisée ;
  5. **Désactivé** : les trois entrées renvoient le message avec
     « DÉSACTIVÉ » et « rose update », sans toucher au manifeste ;
  6. **Auto** : `doit_verifier_au_demarrage() == true` et
     `verifier_au_demarrage()` tente effectivement la lecture.

Commande ciblée : `cargo test --release --offline config_`
→ `test result: ok. 3 passed; 0 failed`.

### 2.3 Type-check de l'interface Windows (harnais isolé)

Le module `setup_gui` est `#[cfg(windows)]` : il est exclu de la compilation
Linux. Comme LA, type-check hors dépôt avec un harnais qui inclut le **vrai**
`setup_gui.rs` et de vrais `paint.rs` / `png.rs` / `inflate.rs`, `setup_core`
et `velopack` stubés (API publique identique) :

```
/tmp/opencode/re2_agents/harness.rs
cd /tmp/opencode/re2_agents && export PATH="$HOME/.cargo/bin:$PATH" && \
CARGO_PKG_VERSION=0.1.5 rustc --edition 2024 --target x86_64-pc-windows-gnu \
  --crate-type lib --emit=metadata -A warnings -o harness_after.rmeta harness.rs
→ EXIT 0 (harness_after.rmeta produit)
```

(La variable `CARGO_PKG_VERSION` doit être fournie à `rustc` : sans elle,
6 erreurs `environment variable not defined` — corrigé dans la commande.)

Le **vrai `velopack.rs`**, lui, est compilé et testé sur Linux par Cargo
(il n'est pas `#[cfg(windows)]`).

---

## 3. Ce qui est prouvé vs ce qui reste à tester sur Windows réel

**Prouvé ici :**

- les 3 options, le premier écran, le thème, les zones de clic, le fil de fond
  et le câblage `WM_APP_MAJ` sont dans le fichier et **type-checkent sur la
  cible Windows** (métadonnées, sans éditeur de liens) ;
- le format de config, son emplacement, les défauts, les cas invalides et
  l'effet des 3 modes sont **exécutés et passent** en tests Linux ;
- `cargo check --release` et `cargo test --release` : 0 échec.

**À tester sur une vraie machine Windows (non fait ici, aucun écran observé) :**

- rendu visuel réel des 3 cartes, de la coche antialiasée GDI+ (et de son repli
  GDI), des couleurs `0xFF2D1A` / `0x9CFF00` et de la mise en page à
  125 % / 150 % et après redimensionnement ;
- clic souris effectif sur les cartes et sur « Vérifier / Télécharger » ;
- présence réelle du fichier `%APPDATA%\Rose\config.json` après « Continuer »
  et relecture au second lancement ;
- déclenchement réel du `curl` de vérification au démarrage en mode `Auto`,
  et son absence en `Manuel` / `Désactivé` (ici remplacé par un manifeste
  `file://` absent) ;
- comportement du message de refus de `rose update` en mode `Désactivé` dans
  le flux complet de `src/main.rs`.

---

## 4. Fichiers touchés

- `rose/src/velopack.rs` — config, décision de démarrage, garde-fou de mode,
  3 tests.
- `rose/src/setup_gui.rs` — premier écran `Vue::MisesAJour`, 3 cartes,
  bouton « Continuer », fil de vérification/téléchargement, `WM_APP_MAJ`.
- `rose/docs/AUTO_UPDATE_CHOIX.md` — ce document.

`rose/src/setup_core.rs` n'a **pas** été modifié : ses ponts `launcher_*`
déjà présents suffisent, la décision de mode ayant été placée dans `velopack`
(une seule source de vérité). `src/main.rs`, `Cargo.toml`, `PLAN.md`,
`docs/index.html` et les docs des autres agents n'ont pas été touchés.

Aucun commit, aucune publication, aucun remplacement de `rose/rose` : le
binaire de la mission est construit à part dans `/tmp/opencode/re2_agents/`.
