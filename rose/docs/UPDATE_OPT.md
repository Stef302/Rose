# Mise à jour automatique OPTIONNELLE et discrète (L6)

État : **21/11/2026**. Dépôt `Stef302/Rose`, HEAD `6673096` au début du chantier
(« PLAN : vague 72 »), `ea061eb` en fin de chantier. Agent L6. **Ce sous-agent
n'a exécuté aucun `git commit` et aucune publication.**

**Constat de traçabilité :** un commit `ea061ebb…` (« Update auto L6 : tests
timer ajustés au comportement réel du cache », signé `Stef302`, 21/11 10:43:17)
est apparu dans l'historique **pendant** le chantier ; il contient l'essentiel
des modifications de `velopack.rs` (271 insertions). Le test structurel
additionnel (`timer_discret_structure_une_seule_requete`) et tous les changements
de `setup_gui.rs` restent **non commités** dans l'arbre de travail. Ce n'est pas
une action de ce sous-agent.

Fichiers possédés modifiés : `rose/src/velopack.rs`, `rose/src/setup_gui.rs`,
ce document. `rose/src/setup_core.rs` est possédé mais **inchangé** (les ponts
`launcher_*` vers `velopack` suffisaient). Aucun autre fichier du dépôt n'a été
touché par ce chantier (les autres modifications visibles dans `git status`
appartiennent à des chantiers concurrents).

---

## 1. Ce qui est livré

1. **Un réglage « Vérification des mises à jour » à 3 choix**, accessible sur le
   premier écran **et** dans les **Options** du launcher :

   | Libellé affiché | Valeur `config.json` | Défaut ? | Comportement |
   |---|---|---|---|
   | **Désactivée** | `"desactive"` | ✅ **oui** | Aucun timer, aucun appel réseau, zéro CPU. |
   | **À l'ouverture** | `"auto"` | non | Banderole L5 du premier écran (single thread, single lecture). |
   | **Automatique (discret)** | `"automatique_discret"` | non ❌ | **NOUVEAU** : UN timer Windows à 6 h, une pastille discrète. |

   Le mode discret est donc **jamais activé sans action explicite** de
   l'utilisateur. L'ancien mode `"manuel"` reste accepté à la lecture d'une
   config déjà écrite (il est ramené à « Désactivée » pour l'affichage) : aucune
   migration n'est imposée.

2. **Source de vérité = le SITE Rose**, pas GitHub :
   `https://stef302.github.io/Rose/telechargements/releases.json`.
   Le manifeste est lu avec le cache local horodaté (`site-releases-cache.json`,
   TTL 300 s) et **une seule requête HTTP bornée à 3 s** (`--connect-timeout 2`,
   `--max-time 3`). Le tick du timer **ne fait aucun repli GitHub** (ce serait
   une 2ᵉ requête) et **ne télécharge rien**. Le repli GitHub reste en place pour
   les actions explicites de l'UI (bouton « Vérifier », sélecteur de versions).

3. **Aucun polling** : un seul `SetTimer(h, TIMER_MAJ_DISCRETE, 21_600_000, …)`
   (6 heures), posé uniquement dans le mode discret. À chaque tick : **un** thread
   qui fait **une** lecture du manifeste, pose le texte de la pastille, poste
   `WM_APP_MAJ_DISCRETE`, puis **meurt**. Aucune boucle, aucun `sleep`, aucune
   icône cachée, aucun service Windows, aucune tâche planifiée, aucune fenêtre
   intrusive. `KillTimer` est appelé dès que l'utilisateur quitte le mode et à la
   fermeture de la fenêtre.

4. **Notification discrète** : un point lime dans la barre d'état + une ligne
   `● Nouvelle version disponible : X → Y` à droite de la barre. Pas de balloon,
   pas de dialogue modal, pas de fenêtre supplémentaire.

5. **Persistance** : `ecrire_config` est appelé **immédiatement** au clic d'un
   choix (Options ou premier écran). Le fichier est relu au démarrage
   (`lire_config()`) pour poser le timer. Choisir « Désactivée » retire le timer
   **dans la foulée**, sans attendre un redémarrage.

---

## 2. Où vit la logique

| Responsabilité | Fichier | Symbole |
|---|---|---|
| Les 3 libellés (source unique) | `velopack.rs` | `ModeMiseAJour::options_ui()` |
| Décision « timer ou pas » | `velopack.rs` | `ModeMiseAJour::doit_demarrer_timer_discret()` |
| Intervalle 6 h + garde-fous | `velopack.rs` | `INTERVALLE_TIMER_DISCRET_MS`, `intervalle_timer_discret_valide()` |
| Un tick = une lecture site | `velopack.rs` | `verification_discrete()` → `verification_discrete_depuis()` |
| Décision d'afficher la pastille | `velopack.rs` | `doit_notifier_discret()` |
| Timer Win32 (unique) | `setup_gui.rs` | `demarrer_timer_maj_discret()`, `arreter_timer_maj_discret()` |
| Thread unique du tick | `setup_gui.rs` | `lancer_verification_discrete()` |
| Application d'un choix | `setup_gui.rs` | `appliquer_mode_maj()` |
| Pastille dans la barre | `setup_gui.rs` | `pastille_discrete()` + tracé de la barre d'état |

`setup_gui.rs` est `#![cfg(windows)]` : il ne se compile pas sur cette machine
Linux. Sa logique a donc été (a) vérifiée par **compilation croisée** Win32
(`cargo check --bin rose-setup --target x86_64-pc-windows-msvc`, avec un stub
`windows.h` pour le `.c` du `build.rs`) et (b) **prouvée structurellement** par
des tests unitaires qui lisent le source (`include_str!`) et comptent les
`thread::spawn`, `SetTimer`, `KillTimer`, `loop`, `sleep`, `requeter_json`.

---

## 3. Coût annoncé et comment il est prouvé

| Exigence | Preuve réellement exécutée |
|---|---|
| Timer à 6 h | `INTERVALLE_TIMER_DISCRET_MS == 21_600_000` ; `intervalle_timer_discret_valide(60_000) == false` ; un seul `SetTimer(h, TIMER_MAJ_DISCRETE, …)` dans le source (`timer_discret_intervalle_long_et_borne`, `timer_discret_structure_sans_polling`). |
| Zéro requête sur 1 minute | Aucun tick possible en moins de 6 h : 0 requête par construction. Le garde-fou de mode renvoie `Ok(None)` **avant** tout disque/réseau dans les 3 autres modes (`timer_discret_garde_fou_de_mode`). Le tick contient exactement **1** `requeter_json(...)`, aucun `lister_versions_source()` / `lister_versions()` (`timer_discret_structure_une_seule_requete`). Le 2ᵉ tick est servi par le cache (`SourceVersions::CacheSite`) : **0 requête HTTP supplémentaire** (`timer_discret_une_lecture_site_puis_cache`). |
| ~0 % CPU sur 1 minute | Un seul thread, lancé **une fois par tick**, qui meurt après une lecture bornée à 3 s. Aucun `loop {`, `while true`, `sleep(`, `Sleep(`, `SetTimer` dans le corps du tick (test structurel). Entre deux ticks : **aucun thread en cours**, le fil de messages Windows dort dans `GetMessage`. |
| Pas d'icône cachée | Aucun appel `Shell_NotifyIcon` / registre / tâche planifiée n'a été ajouté ; la notification est un tracé GDI dans la barre d'état existante. |

### Limite de mesure (importante, non dissimulée)

**Aucun outil Windows n'est disponible sur cette machine Linux** : pas de
Gestionnaire des tâches, pas de `GetProcessTimes`/Performance Counters, pas de
`netstat` sur le processus cible. Je n'ai donc **pas mesuré** « 0,0 % CPU sur
60 s » ni « 0 paquet réseau sur 60 s » avec un instrument Windows : ce serait
une affirmation simulée. Ce qui est **mesuré** ici, ce sont les **propriétés de
la logique** (tests unitaires ci-dessus) qui rendent ces deux nombres
structurels : intervalle 6 h, un seul spawn par tick, un seul appel HTTP, cache
au second tick, aucun repli dans le tick.

Procédure à exécuter **sur Windows** pour clore la mesure (non exécutée ici) :
1. lancer `rose-setup.exe`, choisir « Automatique (discret) », laisser la
   fenêtre ouverte sans interaction ;
2. Gestionnaire des tâches → onglet Détails → colonne « Processeur » de
   `rose-setup.exe` : lire **~0 %** sur 60 s (hors le pic < 3 s au tick) ;
3. `netstat -ano | findstr <PID>` ou Resource Monitor → onglet Réseau :
   **0 connexion** sur 60 s (hors le pic du tick, et uniquement si le cache TTL
   est périmé) ;
4. avancer l'horloge de 6 h (oupatcher temporairement la constante en build de
   diagnostic) : observer **une seule** connexion vers
   `stef302.github.io:443`, puis le retour à 0.

---

## 4. Résultats de vérification réellement exécutés (21/11/2026)

| Contrôle | Commande | Constat |
|---|---|---|
| Build Linux | `cargo build` (features par défaut) | `Finished`, **exit 0** |
| Tests Linux | `cargo test` (tous les harnais) | **275 + 259 + 6 + 8 + 1 = 549 tests, 0 échec** (3 ignorés, tous réseau/miroir). Le harnais lib passe de 270 (avant chantier) à **275** : **5 tests L6** ajoutés. |
| Tests ciblés | `cargo test --lib velopack` | **27 passed, 0 failed**, 3 ignored (22 avant chantier) |
| Compile croisée GUI | `cargo check --bin rose-setup --target x86_64-pc-windows-msvc` (stub `windows.h`) | `Finished`, **exit 0** |
| Point fixe | `./rose/rose rir compilateur/emetteur.rose` ×2 **et** binaire rebuild ×2 | md5 `b3ed9262dd54776a6dbd07bc12368d0e` **×4**, 263 520 o, `cmp` = 0, stderr 0 o — **intact** |
| 9 domaines | 9 programmes `multi_*` × 9 tirages, comparaison octet pour octet au binaire témoin, `taskset -c 6` **et** `-c 16` | **81/81 + 81/81**, aucune divergence |
| L5 (banderole) | `bandeau_ne_ment_jamais`, `mesure_bandeau_cache_froid_et_chaud`, `verification_au_demarrage_sans_polling`, `config_persistance_et_effet_au_demarrage` (pas 6) | **tous verts** : le mode « À l'ouverture » déclenche toujours la banderole et le thread unique, sans polling |

**Récriture du défaut — à connaître :** le défaut passe de `"auto"` à
`"desactive"`. La banderole L5 n'apparaît donc plus **par défaut sur une machine
neuve**, mais elle fonctionne à l'identique dès que l'utilisateur choisit
« À l'ouverture » (contrôle L5 vert ci-dessus). C'est la conséquence directe et
demandée de « Désactivée (défaut) » ; le binaire n'a perdu aucune capacité.

---

## 5. Fichiers touchés / non touchés

- `rose/src/velopack.rs` : enum à 4 valeurs (3 UI + 1 historique), défaut
  `Desactive`, constantes et garde-fous du timer, `verification_discrete*`,
  mise à jour des tests (27 verts).
- `rose/src/setup_gui.rs` : `SetTimer`/`KillTimer` uniques, thread du tick,
  `WM_APP_MAJ_DISCRETE`, pastille, 3 choix dans les Options et sur le premier
  écran, `appliquer_mode_maj`.
- `rose/docs/UPDATE_OPT.md` : ce document (nouveau).
- `rose/src/setup_core.rs` : **inchangé**.
- Aucun commit, aucun tag, aucune release, aucune publication, aucun fichier
  hors de ce périmètre.
