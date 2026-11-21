# UI tray/setup — vague 77 (UITRAY)

État : **21/11/2026**. Dépôt `Stef302/Rose`. **Aucun commit, aucune publication.**

Périmètre initial : `rose/src/bin/rose-tray.rs`, `rose/src/setup_gui.rs`,
`rose/docs/UI_TRAY.md` (nouveau). Le diagnostic a montré que le helper n'était
jamais livré : quatre fichiers de build/packaging ont dû être corrigés en plus
(voir §4), sinon le bouton du setup ne pouvait qu'annoncer « fichier
introuvable ».

---

## 1. Icône absente de la zone de notification

### Causes trouvées

**(a) Le bouton « Continuer » ne lançait pas le helper.** Le choix
« Automatique (discret) » n'écrivait que `config.json` puis passait à
`Vue::Bienvenue`. Le lancement n'existait que dans le bouton dédié de l'écran
Options (`ZONE_TRAY_ACTIVER`). Sur la capture Windows, l'utilisateur avait
donc configuré le mode discret, cliqué « Continuer », et **aucun
`rose-tray.exe` ne démarrait** : seule l'icône verte du task manager était
visible. C'est la cause principale.

**(b) `Shell_NotifyIconW` était vérifié mais jamais journalisé.** Le retour
était bien testé (`notifier()` → bool, `NIM_ADD` contrôlé dans `demarrer()`),
mais le helper est un programme GUI sans console : un échec restait totalement
invisible. Idem pour `LoadImageW`, `RegisterClassExW` et `CreateWindowExW`.

**(c) `rose.ico` n'était pas trouvé : non.** Les chemins étaient déjà bons
(`rose.ico`, `bin/rose.ico`, repli `IDI_APPLICATION`) — mais aucun message ne
le disait.

**(d) Le binaire n'existait pas dans la livraison.** `rose-tray.exe` n'était
ni construit par `scripts/build-windows-rebuild.sh`, ni embarqué dans
`rose-setup.res`, ni installé par `setup_core::installer()`, ni présent dans
le `.nupkg` publié. Même avec le bouton corrigé, `chemin_helper_tray()` ne
pouvait donc rien trouver.

### Correctifs

- `setup_gui.rs`, bras `Action::Continuer` : si le mode choisi est
  `doit_demarrer_timer_discret()`, on appelle `lancer_helper_tray(h)` juste
  après la persistance. Le helper part en
  `DETACHED_PROCESS (0x8) | CREATE_NO_WINDOW (0x08000000)` : il survit à la
  fermeture du launcher, sans console.
- `setup_gui.rs`, `lancer_helper_tray()` : messages visibles dans la barre
  d'état, « **Helper démarré** : icône Rose dans les icônes cachées. » ou
  « **ÉCHEC helper : impossible de démarrer** — … ». Le nom Windows
  `rose-tray.exe` (et non `rose-tray`, ni le binaire Linux
  `rose-tray-aarch64-linux`) est le seul accepté par `chemin_helper_tray()`.
- `rose-tray.rs` : journal en ajout dans `%TEMP%\rose-tray.log` (sans
  dépendance, sans console). Sont journalisés : le mode lu au départ, chaque
  chemin d'icône testé, le repli `IDI_APPLICATION`, le résultat de
  `Shell_NotifyIconW` avec `GetLastError()`, les échecs de
  `RegisterClassExW`/`CreateWindowExW`/`SetTimer`, et la mise en place
  effective de l'icône + du timer. Le repli `LoadIcon(IDI_APPLICATION)` déjà
  présent est désormais prouvé par le journal.

---

## 2. Doublon « Automatique (discret) »

Dans l'écran Mises à jour, la carte sélectionnée affichait le libellé et sa
description (source unique `velopack::ModeMiseAJour::options_ui()`), puis la
ligne de statut verte répétait :

> Automatique (discret) : un timer toutes les 6 h, une pastille si une version
> est disponible.

**Correction** : la carte est conservée (c'est le choix interactif) ; le bras
`ModeMiseAJour::Automatique` de la ligne de statut rend maintenant une chaîne
vide. Cette ligne reste réservée aux résultats d'action (vérification,
téléchargement, erreur). Le timer et le helper ne sont pas touchés.

---

## 3. Tige de la rose coupée

### Mesure

Le PNG embarqué (`installateur/velopack/rose-installeur.png`, 276 × 343,
RGBA) a été décodé avec le décodeur maison : bornes opaques
**x 6..270, y 6..343**. La tige va donc bien au dernier pixel ;
`bornes_opaques` ne la coupe pas, et la réduction par moyenne de surface
conserve des pixels opaques dans sa dernière ligne (alpha 212 mesuré).

La cause était géométrique : `geometrie_marque()` dimensionnait la boîte sur
le ratio du **PNG natif** (343/276) alors que `dessiner_rose()` met à l'échelle
le **contenu opaque** (337/264, plus élancé). La boîte était donc trop courte
de quelques pixels et la pointe anti-aliasée (alpha 127 sur la dernière ligne)
tombait sur le bord exact du rectangle `AlphaBlend`.

### Correctifs

- `RATIO_CONTENU_ROSE = 337.0 / 264.0` : `hauteur_voulue` suit le contenu
  réellement dessiné (140 px au lieu de 137 pour une boîte de 110 px).
- `marge_basse_logo()` réserve 1 à 3 px **sous la tige** dans le rectangle de
  destination ; le contenu est dessiné un peu plus haut, sans être rogné ni
  étiré. La marge reste toujours inférieure à l'espace entre le bas du logo et
  le haut de la cellule du titre (vérifié sur 144 combinaisons
  taille × DPI de la grille de tests).
- Aucune hauteur de carte modifiée : `place_logo` n'était jamais le facteur
  limitant sur la grille (le plus petit rapport observé était 63/63).

---

## 4. Chaîne de build / packaging (ajout nécessaire)

| Fichier | Changement |
|---|---|
| `scripts/build-windows-rebuild.sh` | construit `--bin rose-tray`, copie `rose-tray.exe` dans le stage, l'ajoute aux contrôles et md5 |
| `installateur/velopack/pack-res.py` | `RCDATA_TRAY = 9`, embarque `rose-tray.exe` (ignoré proprement s'il est absent) |
| `rose/src/setup_gui.rs` | `COMPAGNONS` passe à 5 entrées avec `(9, "rose-tray.exe")` |
| `rose/src/setup_core.rs` | `installer()` copie le helper dans `bin\` (id de ressource 9) |
| `scripts/publier-release.py` | `rose-tray.exe` ajouté aux génériques et aux artefacts de release |

Preuve : `verifier-res.py` sur le `rose-setup.exe` relinké liste
`type 10 nom 9 = 444 928 o` (le PE du helper), RCDATA ids `[1, 3, 5, 6, 7, 8, 9]`.

---

## 5. Vérifications

### Réussies ici (Linux, 21/11)

- `cargo t --release` : **279 + 6 + 4 tests, 0 échec** (3 ignorés).
- Cross-build Windows : `cargo build --release --target x86_64-pc-windows-msvc
  --bin rose-setup --bin rose-tray` → `Finished`, 0 erreur (avertissements
  PDB `LNK4099` sans effet).
- `cargo test --release --target x86_64-pc-windows-msvc --no-run` : les cinq
  harnais de tests Windows, dont celui de `setup_gui`, **compilent**.
- Tests ajoutés : journalisation et garde-fous du helper (`rose-tray.rs`),
  tige présente dans le PNG et la réduction, géométrie sans chevauchement avec
  marge basse, absence de doublon, « Continuer » qui lance le helper,
  helper embarqué sous son nom Windows (`setup_gui.rs`).
- Point fixe `b3ed9262dd54776a6dbd07bc12368d0e` **×2** :
  `rose rir compilateur/emetteur.rose` et
  `rose compilateur/emetteur.rose compilateur/emetteur.rose` → 263 520 o,
  md5 identiques, `cmp` octet pour octet, stderr vide.
- Replica Python des formules de géométrie : 144 combinaisons
  (7 tailles × 4 DPI, grille étendue) sans assertion en échec.

### Non vérifiable visuellement ici

Sous Linux, `setup_gui` est `#[cfg(windows)]` : **les tests de géométrie ne
peuvent pas être exécutés**, seulement compilés. Wine est présent mais son
préfixe reste bloqué dans `wineboot --init` (rc=124 après 60 s, `system.reg`
vide) : ni capture du launcher, ni exécution des harnais Windows n'ont pu être
produites. En conséquence :

- l'apparition réelle de l'icône dans la zone des icônes cachées,
- la couleur/le message exact affiché par le setup,
- le rendu final de la tige sur un écran Windows

restent à confirmer par l'utilisateur sur la machine Windows. Le journal
`%TEMP%\rose-tray.log` donne désormais la preuve locale du chemin exact
(`NIM_ADD`, code `GetLastError`, icône utilisée) dès le premier lancement.
