# Association des fichiers `.rose` sur Windows

**Date :** 19/11/2026 · **Dépôt :** `Stef302/Rose` (Rose 0.1.5, HEAD `477736f`)
**Fichiers possédés :** `rose/src/assoc_windows.rs`, ce document, `rose/docs/assoc_windows.patch`.

> ⚠ **Honnêteté :** aucune machine Windows n'était disponible. Tout ce qui est
> décrit ici est **vérifié statiquement depuis Linux** (compilation `rustc`, 11 tests
> unitaires, sortie réelle de `--dry-run`). **L'effet dans l'Explorateur Windows
> (menu clic droit, icône, « toujours ouvrir avec ») n'a PAS été testé.** Voir §6.

---

## 0. Besoin utilisateur

> « je voudrais quand je clique droit sur un .rose je peux l'ouvrir avec
> rose.exe classique ou le shell rose pour directement l'exécuter sans rentrer
> de commande, et quand j'aurai mis toujours ouvrir avec je clique ça exécute
> direct »

Réponse : deux verbes sur le ProgID, `open` (verbe **par défaut**, donc retenu
par « toujours ouvrir avec ») et `console` :

| Verbe | Libellé FR | Libellé EN | Commande |
|---|---|---|---|
| `open` (défaut) | `Exécuter avec Rose` | `Run with Rose` | `"<exe>\rose.exe" run "%1"` |
| `console` | `Éditer avec le shell Rose` | `Edit with Rose shell` | `"<exe>\rose.exe" console "%1"` |

`run` et l'invocation directe `rose fichier.rose` sont identiques (CLI actuelle),
donc le double-clic exécute bien le programme. `console` ouvre le fichier dans
le REPL (`rose console fichier.rose`).

---

## 1. Entrées de registre exactes

Préfixe `P` = `HKCU\Software\Classes` (défaut, `--user`) ou
`HKLM\SOFTWARE\Classes` (`--system`, **nécessite une élévation**).
`E` = chemin du moteur, par défaut détecté (voir §2), typiquement
`C:\...\bin\rose.exe` (installation classique) ou
`%LOCALAPPDATA%\Rose\current\rose.exe` (Velopack).

| Clé | Nom de valeur | Donnée `REG_SZ` |
|---|---|---|
| `P\.rose` | *(par défaut)* | `Rose.Source.1` |
| `P\Rose.Source.1` | *(par défaut)* | `Fichier source Rose` / `Rose source file` |
| `P\Rose.Source.1` | `FriendlyName` | idem |
| `P\Rose.Source.1` | `Description` | `Fichier source Rose — exécuter ou éditer avec le shell Rose` / `Rose source file — run or edit in the Rose shell` |
| `P\Rose.Source.1\DefaultIcon` | *(par défaut)* | `"E,0"` |
| `P\Rose.Source.1\shell\open` | *(par défaut)* | `Exécuter avec Rose` / `Run with Rose` |
| `P\Rose.Source.1\shell\open\command` | *(par défaut)* | `"E" run "%1"` |
| `P\Rose.Source.1\shell\console` | *(par défaut)* | `Éditer avec le shell Rose` / `Edit with Rose shell` |
| `P\Rose.Source.1\shell\console\command` | *(par défaut)* | `"E" console "%1"` |

**9 entrées au total.** `--remove` supprime 2 clés
(`P\.rose` et `P\Rose.Source.1`, avec `reg delete /f` = valeurs et sous-clés).

`DefaultIcon` pointe `rose.exe,0` comme demandé. ⚠ **Conflit documenté :**
l'installeur classique (`setup_core::poser_association`) pose déjà
`DefaultIcon` sur `bin\rose.ico,0` (icône de *document*, correction du 17/11)
et utilise le ProgID `Rose.File`. Voir §4.

Quoting des lignes affichées : convention **`cmd.exe`** — un argument contenant
un espace ou un guillemet est entouré de `"`, et les guillemets internes sont
**doublés** (`""`). Ainsi `"E" run "%1"` s'affiche
`"""E"" run ""%1""` et est directement copiable dans une invite de commandes.
`std::process::Command`, lui, reçoit les arguments BRUTS (un seul argv par
donnée), sans ce doublement.

---

## 2. Commande CLI `rose assoc`

Syntaxe (une fois le patch §3 appliqué) :

```
rose assoc [--user|--system] [--lang fr|en] [--exe chemin] [--dry-run]
rose assoc --remove [--user|--system] [--dry-run]
```

- `--user` (défaut) → `HKCU` ; `--system`/`-s` → `HKLM` (élévation/admin requise).
- `--lang fr|en` → libellés du menu contextuel (défaut : `fr`).
- `--exe <chemin>` → force le moteur (tests, dry-run reproductible).
- `--dry-run`/`-n` → **imprime** les commandes `reg add`, n'écrit rien,
  **fonctionne sur Linux et macOS**.
- `--remove` → `reg delete` des deux clés.
- Sans `--dry-run`, hors Windows : message explicite
  `rose assoc : Windows uniquement (registre HKCU/HKLM). Rien n'a été modifié.`
  et code de sortie 0.
- Sur Windows, `reg.exe` est lancé avec `CREATE_NO_WINDOW` (pas de fenêtre
  noire depuis l'installateur GUI), puis `SHChangeNotify(SHCNE_ASSOCCHANGED)`
  force l'Explorateur à relire les associations. Échec de `reg` → code 1.

Détection du moteur (`chemin_rose`) : 1. `--exe` ; 2. `rose.exe` **à côté**
de l'exécutable courant (cas de `rose-setup.exe`, qui doit associer le moteur,
pas l'installateur) ; 3. l'exécutable courant lui-même.

### Sortie réelle de `--dry-run` (exécutée ici, sous Linux)

Commande : `/tmp/opencode/assoc_dryrun assoc --dry-run --exe 'C:\Program Files\Rose\rose.exe'`

```
rose assoc — dry-run (aucune écriture), moteur : C:\Program Files\Rose\rose.exe
[dry-run] reg add HKCU\Software\Classes\.rose /ve /t REG_SZ /d Rose.Source.1 /f
[dry-run] reg add HKCU\Software\Classes\Rose.Source.1 /ve /t REG_SZ /d "Fichier source Rose" /f
[dry-run] reg add HKCU\Software\Classes\Rose.Source.1 /v FriendlyName /t REG_SZ /d "Fichier source Rose" /f
[dry-run] reg add HKCU\Software\Classes\Rose.Source.1 /v Description /t REG_SZ /d "Fichier source Rose — exécuter ou éditer avec le shell Rose" /f
[dry-run] reg add HKCU\Software\Classes\Rose.Source.1\DefaultIcon /ve /t REG_SZ /d """C:\Program Files\Rose\rose.exe,0""" /f
[dry-run] reg add HKCU\Software\Classes\Rose.Source.1\shell\open /ve /t REG_SZ /d "Exécuter avec Rose" /f
[dry-run] reg add HKCU\Software\Classes\Rose.Source.1\shell\open\command /ve /t REG_SZ /d """C:\Program Files\Rose\rose.exe"" run ""%1""" /f
[dry-run] reg add HKCU\Software\Classes\Rose.Source.1\shell\console /ve /t REG_SZ /d "Éditer avec le shell Rose" /f
[dry-run] reg add HKCU\Software\Classes\Rose.Source.1\shell\console\command /ve /t REG_SZ /d """C:\Program Files\Rose\rose.exe"" console ""%1""" /f
rose assoc — 9 commande(s), portée HKCU — rien n'a été modifié.
```

Variante `--system --lang en` (mêmes clés, préfixe `HKLM\SOFTWARE\Classes`,
libellés `Run with Rose` / `Edit with Rose shell`) également exécutée ; variante
`--remove --dry-run` :

```
[dry-run] reg delete HKCU\Software\Classes\.rose /f
[dry-run] reg delete HKCU\Software\Classes\Rose.Source.1 /f
```

Ces 9 + 2 chaînes sont **figées dans le test**
`tests::dry_run_hkcu_fr_lignes_exactes` et
`tests::remove_genere_deux_reg_delete`.

---

## 3. Patch proposé (NON appliqué)

Fichier : `rose/docs/assoc_windows.patch`. **Aucun fichier interdit n'a été
modifié** (`main.rs`, `lib.rs`, `Cargo.toml`, `PLAN.md`, `jit.rs`, `interp.rs`,
`run.rs`, `wasm_lecteur.rs`, `rosevm.rs` sont intacts).

Le patch contient 4 changements :

1. **`rose/src/main.rs`** — `mod assoc_windows;` (à côté de `mod console_win;`) ;
   3 lignes dans `usage()` ; bras
   `"assoc" => { std::process::exit(assoc_windows::execute(&argv)); }` dans le
   `match action` ; hook post-install dans `install_rose()` **après**
   `setup_core::installer()`, pointant `<prefixe>\bin\rose.exe`.
2. **`rose/src/lib.rs`** — `pub mod assoc_windows;` (API publique + tests).
3. **`rose/src/velopack.rs`** — dans `appliquer_paquets()`, juste après
   l'écriture de `current`, rappel de
   `crate::assoc_windows::appliquer_silencieuse(Some(&racine.join("current").join("rose.exe")))`.

Vérification du patch : `patch -p1 --dry-run < rose/docs/assoc_windows.patch`
→ `checking file rose/src/main.rs` / `lib.rs` / `velopack.rs`, **aucune erreur**.

### Pourquoi le pointage Velopack est `current\rose.exe`

Velopack installe dans `app-<version>\` et fait pointer `current\` vers la
version active (voir `velopack.rs`). Associer `app-0.1.5\rose.exe` casserait
l'association à la mise à jour suivante. Associer `current\rose.exe` la rend
stable — d'où le rappel après `appliquer_paquets()`. La mise à jour par le
**launcher** (`Update.exe`, chemin `is_update_launcher`) ne réécrit pas le
registre, mais `current` reste le même chemin : l'association reste valide.

---

## 4. Conflit avec l'association existante (`Rose.File`) — à trancher

Constat lu dans `rose/src/setup_core.rs` (non modifié) :

- `poser_association()` (l. 1894) écrit déjà :
  `HKCU\Software\Classes\.rose` → `Rose.File`,
  `Rose.File\shell\open\command` → `"<bin>\rose.exe" "%1"` (sans `run`, ce qui
  marche aussi : `rose fichier.rose` ≡ `rose run fichier.rose`),
  `Rose.File\DefaultIcon` → `<bin>\rose.ico,0`,
  `Rose.File\FriendlyTypeName` → `Fichier source Rose`.
- Le désinstallateur (l. ~1137) supprime `.rose` et `Rose.File`, **pas**
  `Rose.Source.1`.

Conséquence si le patch est appliqué tel quel : `.rose` pointe finalement sur
`Rose.Source.1` (notre hook est après l'installeur), le vieux `Rose.File` reste
orphelin mais inactif, et la désinstallation laisse `Rose.Source.1` orphelin
(l'association elle-même disparaît bien, `.rose` étant supprimé).

**Recommandation (non appliquée, setup_core.rs n'est pas à moi) :** ajouter une
ligne de `reg delete HKCU\Software\Classes\Rose.Source.1 /f` à côté de celle de
`Rose.File` dans le désinstallateur, et — si l'on veut l'icône de document —
remplacer notre `DefaultIcon` `rose.exe,0` par `bin\rose.ico,0`. Le cahier des
charges demandant explicitement `rose.exe,0`, le module implémente `rose.exe,0` ;
le choix inverse se fait en une ligne dans `entrees_reg()`.

---

## 5. Vérification statique depuis Linux — PREUVES

Commandes réellement exécutées le 19/11/2026 :

```
$ rustc --test rose/src/assoc_windows.rs -o /tmp/opencode/assoc_test
$ /tmp/opencode/assoc_test
running 11 tests
... 11 passed; 0 failed ...

$ /tmp/opencode/assoc_dryrun assoc --dry-run --exe 'C:\Program Files\Rose\rose.exe'
   → 9 lignes reg add (voir §2)

$ patch -p1 --dry-run < rose/docs/assoc_windows.patch
   → 3 fichiers OK

$ patch -p1 < rose/docs/assoc_windows.patch          # dans une COPIE isolée
$ cargo check --release --bin rose                   → Finished, 0 erreur
$ cargo test --release                               → 333 passés, 0 échec
     dont les 11 `assoc_windows::tests::*` (lib + bin)
$ ./target/release/rose assoc --dry-run --exe 'C:\Program Files\Rose\rose.exe'
   → les 9 lignes reg add de §2 (binaire RÉEL, pas un harnais)

$ cargo build --release --bin rose     → voir §7
$ cargo test --release                 → voir §7 (dépôt NON patché)
```

Le module est **std uniquement** (aucun crate externe, aucun appel à un autre
module du moteur) : c'est ce qui permet de le compiler avec `rustc` seul, sans
attendre que `main.rs`/`lib.rs` déclarent `mod assoc_windows;`.

Le test `verification_statique_des_chaines_et_verbes` lit le propre source du
module via `include_str!("assoc_windows.rs")` et vérifie la présence de :
`HKCU\Software\Classes`, `HKLM\SOFTWARE\Classes`, `Rose.Source.1`,
`DefaultIcon`, `shell\open\command`, `shell\console\command`, `\"%1\"`,
`run `, `console `, `Exécuter avec Rose`, `Éditer avec le shell Rose`,
`Run with Rose`, `Edit with Rose shell`, `reg add`, `reg delete`.

---

## 6. PROUVÉ vs RESTE À TESTER SUR WINDOWS RÉEL

**PROUVÉ ici (Linux, commandes + sorties ci-dessus) :**
- les 9 entrées et leurs données exactes (test `entrees_reg_hkcu_fr_exactes`) ;
- les 9 lignes `reg add` et les 2 `reg delete` (tests figés + sortie réelle) ;
- portée `HKLM`, libellés EN, unicité des deux verbes, rejet des options
  inconnues, no-op hors Windows (11 tests, 0 échec) ;
- le patch s'applique proprement (`patch --dry-run`) ;
- compilation du module avec `rustc 1.99.1`.

**NON PROUVÉ — à tester sur une machine Windows réelle :**
- que l'Explorateur affiche bien les deux verbes au clic droit ;
- que « toujours ouvrir avec » Rose exécute le `.rose` au simple clic
  (dépend du verbe par défaut choisi par Windows) ;
- que l'icône `rose.exe,0` s'affiche (et le cas échéant le cache d'icônes) ;
- que `--system` fonctionne sous un compte administrateur élevé ;
- que `reg.exe` est accessible en environnement verrouillé (politique de
  groupe — `setup_core::diagnostiquer_path` traite déjà ce cas) ;
- que `SHChangeNotify` suffit à rafraîchir l'Explorateur ;
- le cycle complet installer → association → mise à jour Velopack → association.

Aucune de ces affirmations n'est présentée comme testée.

---

## 7. État final des validations du dépôt (19/11/2026, constant)

- `cargo build --release --bin rose` → `Finished release profile ... in 8.26s`,
  **0 erreur** (warnings préexistants uniquement : `dlopen`, `getaddrinfo`,
  noms non snake_case, `paint.rs`, etc. — aucun lié à `assoc_windows.rs`, qui
  n'est pas encore déclaré dans `main.rs`/`lib.rs`).
- `cargo test --release` → **311 tests passés, 0 échec**
  (160 + 144 + 6 + 1, plus 3 × 0 tests de doc). ⚠ La 1re tentative a échoué
  avec `E0061` (2 erreurs) pendant qu'un autre agent modifiait une source
  interdite ; la 2e, après stabilisation, est verte. Constat daté, pas consigne.
- **Les 11 tests de `assoc_windows.rs` ne sont PAS inclus** dans ce 311 :
  le module n'est déclaré nulle part (patch non appliqué). Ils sont exécutés
  séparément par `rustc --test` (§5) — 11 passés, 0 échec.
- Dans la **copie patchée** (§5), `cargo check --release --bin rose` finit sans
  erreur et `cargo test --release` donne **333 passés, 0 échec**
  (171 + 155 + 6 + 1) : les 11 tests du module y sont bien inclus, deux fois
  (lib + bin). Le binaire patché exécute réellement `rose assoc --dry-run` et
  affiche les 9 `reg add` attendus ; sans `--dry-run` sous Linux il affiche le
  message « Windows uniquement ». Cela PROUVE le câblage CLI **dans une copie**,
  pas dans le dépôt livré (où le patch reste non appliqué).
- Jardin : `── jardin/premiers : 20 tests, 0 échec ──` avec `rose/rose` ET
  `rose/target/release/rose`.
- `fib(25)` → `75025` avec les deux binaires (fichier régénéré sous un nom
  distinct : `/tmp/opencode/fib25.rose` avait été réécrit entre-temps par un
  autre agent — me garder de recopier une preuve périmée).
- Point fixe : `bdbef63681608a0b22a5637539e9d738` **deux fois** (MD5 identiques
  de `rose rir compilateur/emetteur.rose` et de
  `rose compilateur/emetteur.rose compilateur/emetteur.rose`) avec les deux
  binaires.

Aucun de ces résultats n'implique un test sur Windows : voir §6.

---

## 8. Fichiers livrés

- `rose/src/assoc_windows.rs` — module autonome, 11 tests, `--dry-run`, hook
  d'installation silencieuse, `CREATE_NO_WINDOW`, `SHChangeNotify`.
- `rose/docs/ASSOCIATION_WINDOWS.md` — ce document.
- `rose/docs/assoc_windows.patch` — patch proposé, non appliqué, vérifié.
