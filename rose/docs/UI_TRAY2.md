# UI tray/setup — vague 78 (UITRAY2)

État : **21/11/2026**. Dépôt `Stef302/Rose`. **Aucun commit, aucune publication.**

Périmètre de ce chantier : `rose/src/bin/rose-tray.rs`, `rose/src/setup_gui.rs`,
`rose/src/setup_core.rs`, `rose/src/paint.rs` (un test de non-régression) et
ce document. Les autres fichiers modifiés dans le dépôt pendant ce chantier
(`rir.rs`, `link_natif.rs`, `natif_emetteur.rs`, `cmd_llvm.rs`) ne l'ont PAS été
par ce chantier et n'ont pas été touchés.

Signalement utilisateur (release v0.1.72, Windows 11) : (1) « rose-tray.exe
introuvable » au clic sur Continuer en mode *Automatique (discret)*, alors que
le binaire est livré ; (2) aucune icône dans la zone de notification ;
(3) description de la 3ᵉ carte coupée en bas ; (4) tige de la rose coupée.

---

## 1. Message « introuvable » et lancement réel du helper

### Cause

`lancer_helper_tray()` (setup_gui) écrivait la configuration, cherchait le
binaire, et en cas d'absence affichait « *rose-tray.exe introuvable à côté du
launcher* ». Or :

- le binaire est **embarqué** (RCDATA 9) et **livré** (stage racine, `bin\`
  après installation) — le mot « introuvable » était donc faux ;
- aucun chemin testé n'était journalisé côté setup : impossible de savoir
  lequel avait échoué ;
- `velopack::dossier_racine().join("current")` était traité comme un DOSSIER
  alors que `current` est un **fichier** contenant le nom de version
  (`app-<version>`) : cette branche ne pouvait jamais trouver le helper.

### Correctifs

- `setup_core.rs` : `MESSAGE_HELPER_PROCHAIN_DEMARRAGE` = **« Le helper sera
  actif au prochain démarrage / après réouverture de session. »**. Le message
  affiché est `Mode discret enregistré. <ce texte>` ; plus aucune occurrence
  de « introuvable » dans le code de production du setup.
- `setup_core.rs` : `NOM_HELPER_TRAY = "rose-tray.exe"` (nom Windows exact,
  jamais `rose-tray` / artefact Linux), `candidats_helper_tray()` (fonction
  PURE : même nom + `bin\` + `payload\`, déduplication) et
  `trouver_helper_tray()` qui cherche, dans l'ordre : exe courant, répertoire
  de travail, `app-<version>` et `app-<version>\bin`, racine Velopack et
  `racine\bin`. **Chaque chemin est journalisé présent/absent**, ainsi que le
  binaire retenu.
- `setup_gui.rs::chemin_helper_tray()` délègue à `trouver_helper_tray()` ;
  `lancer_helper_tray()` journalise le lancement, les indicateurs
  `DETACHED_PROCESS (0x8) | CREATE_NO_WINDOW (0x08000000)` (inchangés et
  vérifiés) et le résultat de `spawn()`.
- Journal PARTAGÉ : le setup et le helper écrivent tous deux dans
  `%TEMP%\rose-tray.log` (même fichier, en ajout).

⚠ Périmètre de la promesse : `docs/TRAY.md` §7 exclut volontairement tout
auto-démarrage (ni service, ni tâche planifiée, ni clé de registre). « Au
prochain démarrage / après réouverture de session » signifie donc : la config
est persistée et le helper est relancé au prochain lancement du
setup/launcher (ou via le bouton des Options). Le helper lit cette config au
démarrage et ne s'active que pour le mode discret.

---

## 2. Icône absente de la zone de notification

`rose-tray.rs` : `Shell_NotifyIconW(NIM_ADD)` est déjà appelé avec un `HICON`
non nul, `NIF_MESSAGE | NIF_ICON | NIF_TIP`, retour vérifié et
`GetLastError()` journalisé en cas d'échec. La lacune restante était la
recherche de `rose.ico` : seulement le dossier de l'exe et `bin\`.

- `candidats_icone()` (fonction PURE, hors module Win32, testable sur Linux)
  impose l'ordre : `CurrentExe` → `rose.ico`, `bin\rose.ico`, puis
  répertoire courant → `rose.ico`, `bin\rose.ico`.
- `charger_icone()` journalise **chaque** chemin testé (présent/absent), le
  chargement `LoadImageW(..., IMAGE_ICON, 16, 16, LR_LOADFROMFILE|LR_SHARED)`
  (repli taille système avec `LR_DEFAULTSIZE`), son `GetLastError()`, puis le
  repli final `IDI_APPLICATION` — de sorte qu'un `HICON` nul n'est jamais
  passé à `NIM_ADD`.
- `NIM_ADD` / `NIM_MODIFY` / `NIM_DELETE` restent vérifiés et journalisés avec
  le code d'erreur Win32.

Comportement attendu sur Windows 11 : l'icône Rose (ou, à défaut de `rose.ico`,
l'icône système) apparaît dans la **zone des icônes cachées** de la barre des
tâches ; un clic droit ouvre « Vérifier maintenant / Désactiver le helper /
Quitter ».

---

## 3. Description coupée dans l'écran « Mises à jour »

`setup_gui.rs`, vue `Vue::MisesAJour` : la hauteur de carte était la constante
`h_opt = s(84)`. Sur fenêtre étroite, la description de « Automatique
(discret) » (« Un seul timer Windows toutes les 6 heures… ») occupe trois
lignes et était tronquée en bas de carte.

- Les hauteurs de titre et de description sont désormais **mesurées** avec
  `crate::paint::hauteur_texte()` et la vraie police via `mesurer()`.
- La hauteur commune des 3 cartes vaut
  `max(84, 10 + h_titre + 4 + h_description + 10)` (grille régulière).
- Le rectangle de la description commence sous le titre **mesuré** (plus de
  décalage fixe `s(40)`) et s'étend sur toute sa hauteur mesurée.
- Test de non-régression dans `paint.rs` : avec la vraie chaîne de
  `options_ui()` et une largeur de 320 px, `h_opt > 84` (mesuré).

---

## 4. Tige de la rose coupée

- `bornes_opaques()` est inchangée et reste correcte : sur le PNG embarqué
  (276 × 343) les bornes mesurées sont **x 6..270, y 6..343** (264 × 337).
  `RATIO_CONTENU_ROSE = 337/264` est le ratio du contenu opaque, pas celui du
  PNG natif (343/276).
- `marge_basse_logo()` réserve désormais **au moins 4 px** (4..8 selon la
  hauteur, jamais plus que la boîte) contre 1..3 px auparavant : la pointe
  anti-aliasée ne tombe plus sur le bord exact de `AlphaBlend`.
- `geometrie_marque()` ne fait plus `.min(343)` sur la hauteur (clamp sur la
  hauteur du PNG natif, qui contient du vide) et plafonne la largeur à 264 px
  (largeur du contenu) : aucune réduction ne peut couper la tige ; le ratio
  du contenu est conservé et la marge basse est réservée dans la boîte.
- Tests géométriques mis à jour : grille 7 tailles × 4 DPI, marge ≥ 4 px,
  contenu ≤ 264 × 337, pointe opaque présente dans la dernière ligne de la
  réduction.

---

## 5. Vérifications exécutées (constats datés)

| Commande | Résultat |
|---|---|
| `cargo build --release --bin rose` | **OK** — `Finished release` |
| `cargo test --release` | **OK, 0 échec** (286 + 291 + 6 + 5 + 8 + 1 tests, 0 failed) |
| `cargo t --release` (`test --bins`, alias sûr) | **OK, 0 échec** (288 + 6 + 5 passed, 3 ignored) |
| Cross-build Windows `cargo build --release --target x86_64-pc-windows-msvc --bin rose --bin rose-tray --bin rose-setup` (SDK xwin + `lld-link`, environnement de `scripts/build-windows-rebuild.sh`) | **OK** — `rose-tray.exe` PE32+ GUI, 450 560 o ; `rose-setup.exe` 11 686 400 o |
| `cargo check --release --tests --target x86_64-pc-windows-msvc` | **OK** — les tests Windows (setup_gui, rose-tray) typent |
| `./rose/rose rir compilateur/emetteur.rose` ×2 | **point fixe `b3ed9262dd54776a6dbd07bc12368d0e` ×2**, 263 520 o, `cmp` identique, stderr 0 octet |

Le cross-build direct `--target x86_64-pc-windows-gnu` échoue ici pour une
raison préexistante et hors périmètre (`windows.h`/`dlltool` MinGW absents) ;
la voie MSVC/xwin, elle, réussit.

## 6. Limites Windows — à confirmer chez l'utilisateur

- Aucune capture d'écran Windows n'est possible sur cette machine (pas de
  session graphique Windows ; Wine ne rend pas la zone de notification). Les
  corrections de rendu (icône, carte, tige) sont **vérifiées par le code,
  les tests géométriques et le typage Windows**, pas par une capture.
- Les exécutables Windows ne peuvent pas être lancés ici : le `spawn()`
  détaché, l'apparition de l'icône et son emplacement exact dans la zone des
  icônes cachées restent à constater sur le PC de l'utilisateur.
- En cas de doute, lire `%TEMP%\rose-tray.log` : il contient les chemins
  testés côté setup et helper, le `GetLastError()` de `LoadImageW` /
  `Shell_NotifyIconW`, le résultat du `spawn()` et l'entrée dans
  `GetMessageW`.
