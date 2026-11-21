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
ni élévation de privilège** : le lancement est laissé à l'utilisateur (par
exemple un raccourci dans `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup`).

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

## 7. Vérifications (21/11/2026, CONSTATS DATÉS)

| Contrôle | Résultat |
|---|---|
| `cargo build --release --bin rose-tray` | ✅ `Finished release profile` |
| `cargo test --release` | ✅ **0 échec** (tous les targets : 275 / 262 / 6 / 4 / 8 / 1 tests OK, 3 ignorés) |
| Point fixe `b3ed9262` ×2 | ✅ `rose rir compilateur/emetteur.rose` **et** `rose compilateur/emetteur.rose compilateur/emetteur.rose` → `b3ed9262dd54776a6dbd07bc12368d0e`, **263 520 octets**, stderr 0 octet, `cmp` identique (source `390335d29fb9cee368b3456dd892e377`) |
| Version du paquet | `0.1.72` inchangée |
| Commit | aucun (modifications laissées dans l'arbre de travail) |

⚠ Ces mesures sont des constats datés : à re-mesurer après tout changement de
code, de quantification ou de topologie.
