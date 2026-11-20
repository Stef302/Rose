# LAUNCH2 — sélecteur de versions dans l'interface du launcher

Date des mesures : **21/11/2026**. Périmètre respecté :
`rose/src/setup_gui.rs` (modifié) et `rose/docs/SELECTEUR_UI.md` (créé).
Aucune modification de `velopack.rs`, `setup_core.rs`, `front.rs`, `run.rs`,
`jit.rs`, `interp.rs`, `Cargo.toml`, `PLAN.md` ni d'un doc d'un autre agent.
Aucun commit, aucune publication de ma part.

> ⚠ **Constat d'environnement** : un autre agent a incorporé une partie de ces
> modifications dans le commit `0d817d0` (« Logo et texte superposés corrigés
> (LOGO1) ») pendant la mission. Je n'ai ni commité ni publié. Mon diff
> résiduel sur `setup_gui.rs` ne porte que sur 4 ajustements (message hors
> ligne, cast `i32`, mise à jour de l'état de navigation, actions du pied de
> page, initialisation de l'état) — voir § 7.

---

## 1. Écran ajouté

`Vue::Versions` est le **8ᵉ variant** de l'énumération (index `7`), ajouté
**en fin**, après `MisesAJour` (index `6`). Les cinq étapes du panneau latéral
gardent leurs index `0..4` : le test `enumeration_vue_garde_les_index_des_cinq_etapes`
le vérifie explicitement. Aucune étape ne s'allume sur les deux nouveaux écrans
(le panneau compare `vue as usize` à `0..4`).

**Accès** : un bouton `VERSIONS` est dessiné en haut à droite de la zone de
contenu sur le **premier écran (`MisesAJour`)** et sur **`Options`**. Sa zone
cliquable est remise à zéro à chaque tracé : il n'est actif que sur ces deux
vues. `Retour` ramène à la vue d'origine (mémorisée dans `version_retour`).

`Vue::Versions` n'existait pas dans l'énumération d'origine ; toutes les
correspondances exhaustives (`boutons`, titre/sous-titre, tracé de contenu) ont
été étendues.

## 2. Ce qui s'affiche réellement

De haut en bas, dans la zone de contenu :

1. **Bandeau d'information** — `cat.message` tel quel, en texte gras. Rouge vif
   si `hors_ligne`, vert lime sinon. Hors ligne, le préfixe `HORS LIGNE — ` est
   ajouté et la phrase « Le téléchargement est impossible… » est garantie même
   si le backend ne l'a pas dite (sans répétition). Le message n'est **jamais**
   vide, y compris si la liste est vide.
2. **En-tête de liste** — `"<N> version(s) · flèches ↑/↓ ou clic pour naviguer"`
   (N vient du catalogue) et deux flèches `▲` / `▼` pour le défilement souris.
3. **Une ligne par `EntreeCatalogue`**, triée par version décroissante :
   - ligne 1 : `version  titre  (date)` — date ISO tronquée à 10 caractères,
     ou **« date inconnue »** si le champ est vide ;
   - ligne 2 : badges construits depuis les drapeaux :
     `[Déjà installée]  [Manifeste en cache]  [Paquet disponible]`, ou
     `Aucun artefact installable — aucune action` si aucun drapeau n'est posé.
     `en_cache` est volontairement libellé **« manifeste en cache »** et non
     « prêt à installer » : le `.nupkg` est supprimé après extraction (contrat
     LAUNCH1 § 2.1) ;
   - la ligne sélectionnée a un filet **vert lime vif** et son texte est marqué.
   - deux boutons dans la ligne : `INSTALLER` / `RÉINSTALLER` et `UTILISER`.
4. **Bandeau de statut** sous la liste : `statut.message` du backend après
   chaque action, en rouge si échec, en vert sinon. Le même message est aussi
   dans la barre d'état du bas. **Aucune action n'est silencieuse.**
5. **Pied de page** : `RETOUR` (gauche) et `RAFRAÎCHIR` (droite, libellé
   « Rafraîchir » passé en majuscules avant tracé Kare) — `Rafraîchir` rappelle
   `launcher_catalogue()`.

Aucune version littérale n'est écrite dans le code de production ; les seules
chaînes de version sont dans les fixtures des tests (vérifié :
`git diff | grep '^+' | grep -E '0\.1\.'` → aucune occurrence).

**Données réelles attendues (mesure API du 21/11, lecture seule)** :
`GET https://api.github.com/repos/Stef302/Rose/releases?per_page=100` →
**HTTP 200, 7 releases**, dont **4 retenues** par `analyser_releases` :

| tag | installable | ce que l'UI propose |
|---|---|---|
| `v0.1.6` | oui | `INSTALLER` (ou `RÉINSTALLER` si `app-0.1.6` existe) |
| `v0.1.5` | oui | idem |
| `v0.1.4` | oui | idem |
| `v0.1.3` | non (legacy `Rose-Setup-0.1.3.exe`) | rien, sauf `UTILISER` si `app-0.1.3` existe |

`v0.1.2`, `v0.1.1`, `v0.1.0` n'ont aucun asset Windows reconnu et ne sont pas
affichées. Le nombre exact de lignes et les badges `Déjà installée` /
`Manifeste en cache` dépendent de l'état disque de la machine Windows
(`LOCALAPPDATA`, dossiers `app-*` et manifeste `releases.win.json`) ; je ne
prétends pas les avoir observés ici.

## 3. Ce qui est cliquable / action réelle

| Zone | Condition | Appel réel |
|---|---|---|
| bouton `VERSIONS` (MisesAJour / Options) | toujours dessiné | `launcher_catalogue()` à l'entrée de l'écran |
| clic sur une ligne | `nb > 0` | sélection de l'index, repeint |
| `INSTALLER` / `RÉINSTALLER` | `installable && !hors_ligne` | `launcher_installer_version(tag)` |
| `UTILISER` | `deja_installee` | `launcher_selectionner_version(tag)` |
| `▲` / `▼` | liste non vide | défilement d'une ligne, sélection maintenue visible |
| `RETOUR` | toujours | retour à la vue d'origine |
| `RAFRAÎCHIR` | toujours | `launcher_catalogue()` à nouveau |

**Clavier** (ajout d'un `WM_KEYDOWN`, le gestionnaire n'en avait aucun) :
`↑`/`↓` ligne, `PgUp`/`PgDn` page, `Début`/`Fin` extrémités, `Entrée` action
autorisée (bascule prioritaire, sinon installation), `Échap` = `Retour`. Les
autres touches passent à `DefWindowProcW`. Le hit-test existant (`ZONES`,
`ZONES_MAJ`, `zone_titre`) est intact : les nouvelles zones sont lues en
plus, avec la même fonction pure `zones_ligne_version` pour le tracé et le clic.

Après un succès, `version_installee` est remplacée par `statut.version`, la
sélection est repositionnée sur cette version, le catalogue est reconstruit,
**puis** `statut.message` est réécrit pour rester visible (le rechargement ne
doit pas écraser le résultat de l'action).

## 4. Ce qui est bloqué (et pourquoi)

- `installable = false && deja_installee = false` → boutons grisés, aucun
  appel backend (contrat § 2.1, ligne 4 du tableau).
- `hors_ligne = true` → **`INSTALLER` désactivé pour toutes les lignes**, même
  si `installable = true` ; le bouton affiche `HORS LIGNE`. Seule la bascule
  (`deja_installee`) reste cliquable, sans réseau.
- `en_cache = true` seul **n'active pas** `UTILISER` : cette action exige
  `deja_installee` (présence de `app-<version>`).
- Clic sur un bouton désactivé : aucun appel, aucun message (pas de succès
  simulé).
- Catalogue non encore chargé (`None`) : ligne « Chargement du catalogue… »,
  aucune action.
- `launcher_installer_version` refuse aussi côté backend un tag absent du
  catalogue ou non installable ; son `Refuse` est affiché comme échec.

## 5. Tests ajoutés

9 tests dans `#[cfg(test)] mod tests_versions` de `setup_gui.rs` (fixtures
uniquement) : rendu de ligne (version/titre/date), date vide, badges des trois
drapeaux + libellé non trompeur, les 4 lignes du tableau d'actions, désactivation
`INSTALLER` hors ligne, bandeau explicite/jamais vide, bornes de `layout_liste`,
déterminisme et non-chevauchement de `zones_ligne_version`, non-régression des
index de `Vue`/`ETAPES`.

**Fait important et non masqué** : `setup_gui.rs` est `#![cfg(windows)]` et
`lib.rs` ne l'inclut que sous Windows. Ces 9 tests **ne tournent donc pas**
dans `cargo test` Linux (ils y sont invisibles, pas « passés »). Je les ai
exécutés réellement en extrayant **verbatim** les blocs source (enum `Vue`,
`ETAPES`, fonctions pures, module de tests) vers
`/tmp/opencode/launch2_agents/harness_versions.rs` (`gen_harness.py`), compilé
avec `rustc --test` et lancé sous Linux :

```
running 13 tests   # 9 LAUNCH2 + 4 LOGO1 (geometrie_marque)
test result: ok. 13 passed; 0 failed
```

Les 4 tests de géométrie de LOGO1 y sont inclus et passent : je n'ai touché ni
`geometrie_marque`, ni `GeometrieMarque`, ni `dessiner_rose` (vérifié par
`git diff | grep -E 'geometrie_marque|…'` → aucune occurrence).

## 6. Preuves de compilation et de non-régression

| Contrôle | Résultat mesuré |
|---|---|
| `CARGO_TARGET_DIR=/tmp/opencode/launch2_agents cargo build --release --bin rose` | **OK** (1 m 32 s) |
| `CARGO_TARGET_DIR=/tmp/opencode/launch2_agents cargo test --release` | **515 passed, 0 failed, 6 ignored** |
| `cargo check --target x86_64-pc-windows-gnu --tests` (copie isolée `wincheck`, `windows_compat.c` stubé : ni mingw ni linker Windows ici) | **EXIT 0** — `setup_gui.rs` et ses tests typent |
| Harnais source verbatim (`rustc --test`) | **13/13 OK** |
| Jardin `jardins/tests/premiers.rose` | **20 tests, 0 échec** |
| `/tmp/opencode/fib25.rose` | **75025** |
| Point fixe `rir` ↔ front-end (`emetteur.rose`) | `b3ed9262dd54776a6dbd07bc12368d0e` **×2**, 263 520 o |
| Banc 9 domaines × 9 tirages × c6/c16 (`multi_valeurs.py`, `FIN` = binaire isolé final) | **c16 81/81, c6 81/81, TOTAL 162/162**, `rc=0` |
| API GitHub (lecture seule) | **HTTP 200**, 7 releases, 4 retenues |

Le banc 9 domaines mesure l'identité des **valeurs**, pas des temps : aucun
gain de performance n'est revendiqué. Le moteur (`front.rs`, `jit.rs`,
`interp.rs`, `run.rs`) n'a pas été modifié par LAUNCH2 ; `front.rs` est modifié
dans l'arbre de travail par un **autre agent**, je ne l'ai ni lu en détail ni
touché.

## 7. Limites — ce que je n'ai PAS pu prouver

- **Aucune capture d'écran** de la fenêtre Windows : il n'y a pas de session
  Windows ici, et le binaire `Rose-win-Setup.exe` n'a pas été **lié** (ni
  `x86_64-w64-mingw32-gcc`, ni SDK MSVC installés). La preuve UI est donc :
  (a) typage Windows de `setup_gui.rs` + ses tests, (b) exécution réelle des
  fonctions pures de rendu/navigation, (c) relecture du code de tracé. Je ne
  prétends pas avoir vu le sélecteur s'afficher.
- `launcher_catalogue()` est appelé **synchroneusement** à l'entrée et sur
  `Rafraîchir`. Un accès GitHub lent peut figer brièvement la fenêtre (un
  message « Chargement… » est peint avant l'appel). Aucun thread n'a été
  ajouté : c'est un choix de périmètre, pas une négligence de l'action
  d'installation, qui reste elle aussi synchrone.
- Le diff résiduel de cette mission sur `setup_gui.rs` (après incorporation
  dans `0d817d0`) porte sur : bandeau hors ligne garanti, cast `i32` de
  `layout_liste`, synchronisation `version_visible`/`version_depart` avec le
  tracé, curseur main sur les nouvelles zones, 5 bras d'action, initialisation
  des 6 champs d'état. Le gros de l'écran était déjà dans l'arbre au moment
  de la incorporation.
