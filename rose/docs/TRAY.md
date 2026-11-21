# Helper ARRIÈRE-PLAN Windows — `rose-tray` (vague 74)

État : **21/11/2026**. Dépôt `Stef302/Rose`. Ce chantier n'a exécuté **aucun
`git commit` et aucune publication**.

Fichiers du chantier : `rose/src/bin/rose-tray.rs` (nouveau), la section
`[[bin]] name = "rose-tray"` de `rose/Cargo.toml` (ajoutée, version `0.1.72`
intacte), ce document. Aucun autre fichier n'a été modifié par ce chantier.

---

## 1. Ce qui est livré

Un binaire léger, **processus séparé**, qui n'embarque pas le moteur Rose,
n'ouvre aucune fenêtre visible et n'a **aucune dépendance Rust supplémentaire**.

| Élément | Comportement |
|---|---|
| **Icône** | Une icône dans la **zone des icônes cachées** de Windows (`Shell_NotifyIconW`, `NIM_ADD`). `rose.ico` (ou `bin/rose.ico`) à côté de l'exe ; à défaut l'icône système `IDI_APPLICATION`. Aucun `DestroyIcon` (`LR_SHARED`). |
| **Timer** | **Un seul** `SetTimer(hwnd, 1, 21_600_000, NULL)` — 6 heures. Aucun autre timer, aucun polling. |
| **Tick** | **Une** lecture du manifeste du **site** via `rose::velopack::verification_discrete()` (lecture seule, cache TTL 300 s, curl borné à 3 s). **Jamais GitHub**, jamais de téléchargement, jamais de 2ᵉ requête. Le thread du tick meurt aussitôt. |
| **Notification** | Ballon discret `NIF_INFO` (`NIIF_INFO \| NIIF_NOSOUND`), uniquement si une version réellement plus récente est annoncée. Aucune fenêtre, aucun son. |
| **Menu clic droit** | Trois entrées, persistées dans `config.json` ( voir §3 ). |
| **Linux / macOS** | Message clair + `exit 0` : aucun tray n'existe, aucun timer, aucun réseau, aucun processus. |

### Menu contextuel (clic **droit** uniquement)

1. **Vérifier maintenant** → déclenche immédiatement la même lecture unique du
   manifeste site (un drapeau `AtomicBool` interdit tout empilement si l'on
   clique plusieurs fois).
2. **Désactiver le helper** → écrit `{"mode": "desactive"}` via
   `velopack::ecrire_config(ModeMiseAJour::Desactive)`, retire l'icône et quitte.
   Le prochain lancement ne posera **ni icône, ni timer, ni requête**.
3. **Quitter** → retire l'icône (`NIM_DELETE`) et quitte **sans modifier la
   configuration** : le choix « Automatique (discret) » reste persistant.

Le clic gauche ne fait rien (pas de vérification accidentelle).

---

## 2. Démarrage et garde-fou

Le helper ne fait quelque chose **que** si le mode configuré est
`automatique_discret` (choix explicite « Automatique (discret) » des Options) :
`ModeMiseAJour::doit_demarrer_timer_discret()`. Les trois autres modes
(`desactive`, `auto`, `manuel`) ne posent ni icône, ni timer, ni requête.

Le défaut reste **`desactive`** : le helper n'est jamais actif sans action de
l'utilisateur. Il n'y a **ni service Windows, ni tâche planifiée, ni clé HKLM,
ni élévation de privilège** : depuis la vague 78, le setup dépose un petit
`rose-tray.cmd` dans `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup`
(`shell:startup`) dès que le mode discret est choisi, et le supprime à la
désactivation. Détails, chemins exacts et vérification : **`docs/TRAY3.md`**.

---

## 3. Persistance

Le choix est un simple JSON sous le profil utilisateur :

- Windows : `%APPDATA%\Rose\config.json` ;
- surcharge de diagnostic : `ROSE_CONFIG_DIR`.

Valeurs : `"automatique_discret"` (le helper tourne), `"desactive"` (le helper
ne démarre pas). Les écritures passent exclusivement par les fonctions
publiques de `rose::velopack` (`lire_config`, `ecrire_config`) : aucune
réécriture maison du fichier, aucun autre module touché.

---

## 4. Consommation — preuve structurelle

**Zéro CPU entre les ticks.** Le fil principal est bloqué dans `GetMessageW` :
il ne consomme rien tant qu'aucun message (timer, clic, fin) n'arrive. La
structure est vérifiée par le test `structure_un_timer_une_requete_aucun_polling` :

- exactement **1** appel à `SetTimer(hwnd, ID_TIMER, …)` ;
- exactement **1** point d'appel `Shell_NotifyIconW(action, d)` (utilisé pour
  `NIM_ADD`, `NIM_MODIFY` du ballon et `NIM_DELETE`) ;
- exactement **1** `std::thread::spawn` (le tick, qui meurt aussitôt) ;
- **aucun** `loop {`, aucun `thread::sleep`, aucun appel à
  `lister_versions_source` (le repli GitHub) ;
- `INTERVALLE_TIMER_DISCRET_MS == 21_600_000` et
  `intervalle_timer_discret_valide(...)`.

> Le test lit sa propre source avec `include_str!` : les aiguilles de recherche
> sont donc assemblées à l'exécution et le code examiné est tranché avant
> `#[cfg(test)]`, sinon les motifs se compteraient eux-mêmes (mesuré : 2 au
> lieu de 1).

---

## 5. Appels Win32 — aucune dépendance

Tout est déclaré dans le fichier via `extern "system"` :

- `user32` : `RegisterClassExW`, `CreateWindowExW` (fenêtre **message-only**,
  `HWND_MESSAGE`), `GetMessageW`, `TranslateMessage`, `DispatchMessageW`,
  `PostMessageW`, `PostQuitMessage`, `SetTimer`, `KillTimer`, `DestroyWindow`,
  `GetCursorPos`, `SetForegroundWindow`, `CreatePopupMenu`, `AppendMenuW`,
  `TrackPopupMenu`, `DestroyMenu`, `LoadIconW`, `LoadImageW`, `GetModuleHandleW`,
  `DefWindowProcW` ;
- `shell32` : `Shell_NotifyIconW`.

Les structures `WNDCLASSEXW`, `MSG`, `NOTIFYICONDATAW` (version V3, 952 octets
sur x64, ballon `NIF_INFO` inclus) sont déclarées en `#[repr(C)]` dans le
fichier. `#![cfg_attr(windows, windows_subsystem = "windows")]` supprime la
console noire.

---

## 6. Limites Windows

- La zone des icônes cachées est **gérée par Windows** : l'icône peut être
  masquée par l'utilisateur ou déplacée par les mises à jour de la barre des
  tâches ; le helper ne peut pas forcer son affichage.
- Nécessite **Windows 10/11** (présence de `user32.dll` / `shell32.dll` et de
  `curl.exe` pour la lecture HTTP de `velopack`). Aucun droit administrateur.
- Le timer est porté par la **session utilisateur** : si la session est fermée,
  le helper s'arrête ; un tick manqué n'est pas « rattrapé » en rafale (pas de
  replay, pas de rattrapage).
- Le manifeste site est la **seule** source du tick : si le site est
  injoignable, le tick échoue silencieusement (aucun ballon d'erreur, aucun
  repli GitHub) — c'est volontaire pour rester discrètement à 1 requête.
- Le ballon est **informatif** : il n'installe rien. Le téléchargement reste
  une action explicite via le launcher / `rose update`.
- Sur Linux/macOS, le binaire se contente d'afficher un message et de sortir :
  il n'y a pas d'équivalent de la zone des icônes cachées.

---

## 7. Activation depuis le launcher (bouton des Options)

L'utilisateur lance `rose-tray.exe` une fois — raccourci, ou **bouton
« Activer la vérification en arrière-plan » ajouté dans les Options** du
launcher (`setup_gui.rs`, `lancer_helper_tray`). Ce bouton :

1. écrit le mode `automatique_discret` via `velopack::ecrire_config` (le helper
   le relit au démarrage — c'est ce garde-fou qui rend la désactivation
   persistante) ;
2. garde le timer L6 du launcher cohérent avec le même mode ;
3. lance `rose-tray.exe` en **`DETACHED_PROCESS | CREATE_NO_WINDOW`** : aucun
   parent, aucune console, aucune fenêtre, et le helper survit à la fermeture
   du launcher.

Le chemin est cherché à côté de l'exe courant, dans `bin\`, `payload\` ou dans
l'installation Velopack (`current\`). S'il est absent, aucune fenêtre
d'erreur : une simple ligne sous le bouton indique qu'il faut copier le
binaire. Depuis la vague 78, un lanceur `rose-tray.cmd` est aussi créé dans
le dossier Démarrage de l'utilisateur (`shell:startup`) et retiré par
`rose-tray --disable` ou le menu « Désactiver » — toujours **sans service,
sans tâche planifiée, sans clé de registre et sans droits admin**. Voir
`docs/TRAY3.md`.

---

## 8. Artefacts de release — À FAIRE PAR L'ORCHESTRATEUR

Ce chantier historique **ne modifiait aucun script de build**. Depuis lors,
`scripts/build-windows-rebuild.sh` copie bien `rose-tray.exe` et les icônes
dans le stage ; `rose-tray.exe` porte désormais son icône dans son PE (voir
`docs/TRAY3.md`). La liste ci-dessous reste la checklist à re-vérifier pour
chaque release Windows :

1. copier **`rose-tray.exe`** (produit par `cargo build --release --bin
   rose-tray`, présent dans `rose/target/release/`) à côté de `rose.exe`
   **et/ou** dans le sous-dossier `bin\` du stage Velopack ;
2. copier **`rose.ico`** au même endroit (ou dans `bin\`) : c'est l'icône
   affichée dans la zone des icônes cachées ; sans elle, le helper retombe sur
   l'icône système, mais continue à tourner ;
3. ajouter ces deux fichiers au manifeste de release / aux checksums SHA-256
   au même titre que `rose.exe` et `rose-shell.exe` ;
4. vérifier après installation que `chemin_helper_tray()` trouve bien le
   binaire (bouton des Options → ligne « Helper lancé : icône Rose dans les
   icônes cachées. »).

Sans ces copies, le helper compile et se lance à la main depuis leur dossier,
mais le bouton affiche « `rose-tray.exe` introuvable » : c'est un choix
délibéré — aucun script de publication n'a été touché ici.

---

## 9. Vérifications (21/11/2026, CONSTATS DATÉS)

| Contrôle | Résultat |
|---|---|
| `cargo build --release --bin rose-tray` | ✅ `Finished release profile` (binaire 332 520 octets) |
| `cargo t --release` (alias du dépôt = `cargo test --release --bins` ; le `cargo test` nu charge le cdylib comme harnais et n'est pas la voie du projet) | ✅ salle propre : **0 échec** — 275 (lib) / 262 (binaire `rose`) / 0 (`rose-setup`) / 6 (`rose-shell`) / 4 (`rose-tray`) / 8 / 1, **3 ignorés**, RC=0. ⚠ Re-mesuré ensuite **pendant qu'un agent concurrent modifiait `main.rs`** : 1 échec **hors périmètre tray**, `tests_cli_clair::aide_sans_chiffres_de_taille_ni_noms_d_outils` (« mention interdite dans l'aide : python », `src/main.rs:1596`). Vérifications ciblées rejouées à la fin, **toutes vertes** : `cargo test --release --lib` → 275/0, `--bin rose-tray` → 4/0, `--lib velopack::tests::timer_discret` → 5/0. |
| `cargo check --release --bin rose-tray --target x86_64-pc-windows-msvc` | ✅ RC=0 (avec le stub `windows.h` de `/tmp/opencode/winstub` pour `build.rs`) |
| `cargo check --release --bin rose-setup --target x86_64-pc-windows-msvc` | ✅ RC=0 (le bouton des Options se compile bien sur Windows) |
| Cross-lien complet Windows | ❌ non obtenu ici, pour une raison **préexistante hors périmètre** : cible GNU → `mprotect` défini deux fois (`windows_compat.o` vs `libgcc.a`) ; `cargo xwin` → `build.rs` transmet des `-I/imsvc` que `clang --target` rejette. Le typage Windows est en revanche intégralement vérifié par les deux `cargo check` ci-dessus. |
| Point fixe `b3ed9262` ×2 | ✅ `rose rir compilateur/emetteur.rose` **et** `rose compilateur/emetteur.rose compilateur/emetteur.rose` → `b3ed9262dd54776a6dbd07bc12368d0e`, **263 520 octets**, stderr 0 octet, `cmp` identique (source `390335d29fb9cee368b3456dd892e377`) |
| L6 (3 modes) | ✅ inchangé : les tests `parser_config_reconnait_les_trois_modes_ui_et_le_mode_historique`, `timer_discret_garde_fou_de_mode`, `timer_discret_intervalle_long_et_borne`, `timer_discret_structure_sans_polling`, `timer_discret_structure_une_seule_requete` passent ; `velopack.rs` n'a pas été modifié. |
| Version du paquet | `0.1.72` inchangée (seule la section `[[bin]]` de `Cargo.toml` a été ajoutée par ce chantier) |
| Commit | aucun de la part de ce sous-agent (l'orchestrateur a intégré les fichiers dans l'arbre) |

### Nuance de mesure — où passe le CPU

Le helper n'a **ni boucle active ni attente active** : son unique boucle est
`GetMessageW`, qui met le fil au repos jusqu'à un message. Le tick est un
thread unique qui meurt après **une** requête. En revanche, la fonction
`velopack` réutilisée pour borner `curl` à 3 s contient, **en elle**, une
attente `try_wait` avec un `sleep` de 20 ms — elle ne tourne que pendant ces
3 s, une fois par tick, et n'existe pas entre les ticks. C'est une propriété
du code existant, pas une boucle du helper ; le test structurel porte bien sur
`rose-tray.rs` uniquement. Aucun instrument Windows n'étant disponible sur
cette machine Linux, « 0 % CPU sur 60 s » n'a **pas** été mesuré au
Gestionnaire des tâches : c'est une garantie structurelle, pas une mesure
d'horloge (voir UPDATE_OPT.md pour la procédure Windows).

⚠ Ces mesures sont des constats datés : à re-mesurer après tout changement de
code, de quantification ou de topologie.
