# Helper `rose-tray` — « À l'ouverture » lance aussi le helper (vague 81)

État : **21/11/2026**. Dépôt `Stef302/Rose`. Ce chantier n'a exécuté **aucun
`git commit` et aucune publication**.

Fichiers du chantier : `rose/src/demarrage.rs`, `rose/src/setup_gui.rs`,
`rose/src/bin/rose-tray.rs`, `rose/src/velopack.rs`, `rose/docs/TRAY4.md`
(nouveau). Aucun changement du compilateur Rose (`compilateur/`).

---

## 1. Bug signalé

L'utilisateur a choisi **« À l'ouverture »** (`ModeMiseAJour::Auto`, le mode
proposé par défaut dans l'UI) et le helper tray **ne s'est pas lancé tout seul**
à la fin de l'installation.

Cause : le lancement immédiat et l'enregistrement `shell:startup` étaient liés
au **seul** prédicat `doit_demarrer_timer_discret()`, vrai uniquement pour
`ModeMiseAJour::Automatique` (« Automatique (discret) »). Or « À l'ouverture »
est **aussi** un mode automatique : il doit lancer le helper et s'enregistrer,
mais **sans** timer 6 h (c'est le launcher qui fait la lecture au démarrage
et affiche la banderole L5).

---

## 2. Correction — deux prédicats distincts

`rose/src/velopack.rs` :

| Prédicat | `Auto` (« À l'ouverture ») | `Automatique` (discret) | `Desactive` / `Manuel` |
|---|---|---|---|
| `doit_lancer_helper()` **(nouveau)** | ✅ | ✅ | ❌ |
| `doit_demarrer_timer_discret()` (inchangé) | ❌ | ✅ | ❌ |
| `doit_verifier_au_demarrage()` (L5, inchangé) | ✅ | ❌ | ❌ |

`doit_lancer_helper()` = `matches!(self, Auto | Automatique)`. Il pilote :

1. le lancement immédiat en fin d'installation (`WM_APP_FIN`) ;
2. le lancement au « Continuer » du premier écran ;
3. l'entrée `shell:startup` déposée/relue par `activer_session_tray()` ;
4. le maintien de cette entrée quand on clique une carte dans les Options.

`doit_demarrer_timer_discret()` reste **réservé au timer 6 h** : il n'a pas
été élargi, sinon « À l'ouverture » aurait forcé un timer alors que
l'utilisateur a choisi une seule lecture à l'ouverture.

### Points de code corrigés

- `setup_gui.rs` — `Action::Continuer` : `mode.doit_lancer_helper()` remplace
  `doit_demarrer_timer_discret()` ; le bras « désactivation » ne concerne plus
  que `Desactive` / `Manuel`.
- `setup_gui.rs` — `WM_APP_FIN` (fin d'installation) : même remplacement.
  C'est le point précis du bug : le binaire `rose-tray.exe` n'arrive qu'ici
  dans une installation neuve.
- `setup_gui.rs` — `lancer_helper_tray()` prend maintenant le **mode choisi**
  en paramètre et l'écrit tel quel. Avant, il écrivait toujours
  `Automatique` : choisir « À l'ouverture » écrasait donc silencieusement le
  `auto` en `automatique_discret` et cassait la banderole L5. Le timer 6 h
  n'est posé que si `mode.doit_demarrer_timer_discret()`.
- `setup_gui.rs` — `appliquer_mode_maj()` : le mode `Auto` ne retire plus
  l'entrée `shell:startup` (c'était une régression directe), et les deux modes
  automatiques la gardent ou la posent.
- `setup_gui.rs` — libellé du bouton des Options : « DÉJÀ ACTIVÉ » dès qu'un
  mode automatique est retenu.
- `rose-tray.rs` — `doit_demarrer()` utilise `doit_lancer_helper()` : le helper
  ne sort plus immédiatement en mode `Auto`. Le `SetTimer(hwnd, …)` et la
  lecture immédiate sont **conditionnés à `doit_demarrer_timer_discret()`**.
- `demarrage.rs` — documentation du garde-fou mise à jour.

### Banderole L5 — intacte

Aucune modification de `doit_verifier_au_demarrage()`, de
`verifier_au_demarrage()` ni du bras `if mode == Auto { lancer_verification_maj(h, false) }`
de `appliquer_mode_maj()`. Le mode `Auto` reste écrit `auto` dans
`config.json`. La vérification au lancement du launcher et la banderole
fonctionnent comme avant.

### Choix documenté : icône en mode « À l'ouverture »

En mode `Auto`, le helper est bien lancé et enregistré dans `shell:startup`,
mais il reste **passif** :

- l'icône **peut apparaître** (le helper reste vivant dans `GetMessageW`) ;
- **aucun timer 6 h** n'est posé ;
- **aucune requête automatique** n'est faite par le helper : la lecture du
  manifeste et l'éventuelle banderole appartiennent au launcher (L5) ;
- `verification_discrete()` reste gardé par `doit_demarrer_timer_discret()` :
  le menu « Vérifier maintenant » de l'icône est donc sans effet en mode
  `Auto`. C'est un choix de discrétion, pas une omission : la vérification
  automatique de ce mode se fait à l'ouverture, dans le launcher.

`rose-tray --disable` et le menu « Désactiver le helper » écrivent toujours
`desactive` et retirent l'entrée `shell:startup`.

---

## 3. Vérification — constats datés du 21/11/2026

| Contrôle | Résultat mesuré |
|---|---|
| `cargo build` | ✅ RC=0 (warnings préexistants uniquement) |
| `cargo build --release --bin rose` | ✅ RC=0 |
| `cargo test` (suite complète, 8 cibles) | ✅ **0 échec** : 294 lib, 301 `rose`, 6 `rose-aot`, 9 `rose-tray`, 8 `grands_nombres`, 1 `mul_cmp`, 0 ailleurs, 3 ignorés |
| Tests des 3 modes (`parser_config_reconnait_les_trois_modes_ui_et_le_mode_historique`, `helper_lance_pour_les_deux_modes_automatiques_pas_pour_les_autres`) | ✅ verts |
| `garde_fou_de_mode` + `timer_6h_reserve_au_mode_discret` (`rose-tray`) | ✅ verts : `Auto` démarre, mais ne pose pas de timer |
| Cross-compile `cargo build --release --target x86_64-pc-windows-msvc --bin rose-tray` | ✅ RC=0, `rose-tray.exe` **529 408 octets**, sha256 `82442425faad326441e8b1e8e97d08872b2d143273d4524e79969a7b3ac457b9` |
| Point fixe `b3ed9262` ×2 (avec le `rose/rose` reconstruit en release) | ✅ `rose rir compilateur/emetteur.rose` **et** `rose compilateur/emetteur.rose compilateur/emetteur.rose` → rc=0, **263 520 octets**, stderr 0, md5 `b3ed9262dd54776a6dbd07bc12368d0e` **×2**, `cmp` identique |
| Exécution réelle sur Windows | ❌ **non mesurable ici** (aucun Windows sur cette machine) |

Le cross-compile MSVC exige l'environnement de `scripts/build-windows-rebuild.sh`
(`CFLAGS_x86_64_pc_windows_msvc` pointant sur les includes xwin, `RUSTFLAGS`
avec les bibliothèques CRT/SDK, et le wrapper `lld-link`). Sans ces variables,
`build.rs` échoue sur `#include <windows.h>` — ce n'est pas une régression du
tray.

Note de méthode : une première salve de `cargo test` a montré 4 échecs
transitoires dans `png::tests` / `paint::tests`, provenant du chantier concurrent
du logo (PNG 276×343 remplacé par 184×233, tests non encore alignés). Les tests
ont été alignés entre-temps par ce chantier ; la salve finale ci-dessus est
verte sur les 8 cibles.

---

## 4. Limites Windows (héritées de TRAY3.md)

- Le `.cmd` de `shell:startup` fait clignoter une fenêtre ~100 ms au logon
  avant `start /B`. Le helper lui-même est en sous-système Windows (aucune
  console). Un raccourci `.lnk` supprimerait ce clignotement mais exigerait
  COM/IShellLinkW.
- Une entrée oubliée reste inoffensive : le `.cmd` commence par `if exist`, et
  le helper relit la config — `Desactive` / `Manuel` le font sortir immédiatement.
