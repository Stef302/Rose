---
name: Rose
description: Écrire, corriger et faire tourner des programmes dans le langage Rose (langage compilé en français — entiers exacts illimités, rationnels, complexes, constantes irrationnelles π/e/φ, LaTeX automatique, JIT Cranelift, AOT LLVM ORC, WASM). Utiliser ce skill dès qu'il s'agit de coder en Rose (.rose), d'utiliser la CLI rose (run/latex/comp/orc/link/info), ou de développer dans le dépôt ROSE.
metadata:
  opencode/autoinvoke: true
---

# Développer en Rose

Rose est un langage de programmation **en français**, typé fort dynamiquement,
à arithmétique **exacte** (BigInt natif, zéro GMP), compilé par un moteur Rust
(interprète · JIT Cranelift · AOT LLVM ORC · WASM).

Avant d'écrire du Rose, lis la référence complète :

- `references/syntaxe.md` — **toute la syntaxe** : types, instructions,
  opérateurs, bibliothèque standard, exemples.
- `references/cli.md` — **la ligne de commande** : commandes, REPL, AOT,
  installation.

L'exemple vivant de toute la syntaxe : `rose/exemples/tour.rose`.

## Règle d'or : vérifier avec le moteur

Le binaire de test est `rose/rose` (et sa copie `rose-20`, un fichier à la racine).
**Toujours tester chaque programme écrit** avant de le conclure :

```bash
cd rose && ./rose programme.rose          # exécute (JIT + formules LaTeX)
./rose latex programme.rose               # rendu LaTeX du programme entier
./rose info                               # état du moteur
```

Après une modification du moteur Rust (`rose/src/`) :

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cd rose && cargo build --release
cp target/release/rose ../rose/rose && cp target/release/rose ../rose-20
cargo test --release                      # tests attendus verts
bash ../bench/suite/run_parite_jit.sh     # parité JIT ↔ interprète
```

Sans ce `cp`, on teste un binaire périmé. Sans le `PATH`, cargo ne trouve
pas rustc.

## Les 10 pièges qui cassent un programme Rose

1. **`i` minuscule = l'imaginaire** (`3 + 4i`, `i * i` → `-1`). Le même
   lexème sert de variable : dans `pour i de …`, `i` devient la VARIABLE
   entière. Hors boucle `pour`, nomme tes compteurs autrement (`$compteur`).
2. **`pour j de A vers B` est EXCLUSIF** : `pour j de 0 vers 3` → 0, 1, 2.
3. **Corps raccourcis `? (cond) [ a ; b ]` et `@ (cond) [ a ; b ]`** :
   instructions séparées par `;`, JAMAIS `,`.
4. **`/` = division exacte** (`7 / 2` → `3.5`) ; division entière = `÷`.
5. **Listes homogènes** : `[1, "a"]` est refusé, un seul type par liste.
6. **Le type se fige à la première affectation** : `$s = "x"` puis
   `$s = 5` = erreur.
7. **`<< x` (saisie)** : variable inconnue → lit un entier ; pour un
   flottant, créer `$x = 0.0` avant.
8. **Types de fonctions FACULTATIFS** : `fn carre(x) { retour x * x }` est
   valable. Les types déclarés (`entier`/`flottant`/`booleen`) activent la
   voie rapide JIT/AOT ; les autres fonctions restent interprétées.
9. **`concatener` prend une LISTE** : `concatener(["ab","cd"])` → "abcd".
   `joindre($l, x)` AJOUTE x en fin de liste (mutateur, ne retourne rien).
10. **Backtick interdit** dans les identifiants ; accents et Unicode permis
    (`⋅ ≤ ≥ ≠ ≈ →` fonctionnent).

## Formules LaTeX (comportement de sortie)

Chaque `>> expr` affiche `formule_latex   résultat` (JIT et interprète, à
l'octet près) :

```
>> 3 + 4i      →  3 + 4\mathrm{i}   5
>> modulus($z) →  \left|z\right|   5
>> pi          →  π = 4\cdot\arctan(1/5) - \arctan(1/239)   π ≈ 3.14159…
```

`rose latex prog.rose` rend le programme entier. Signature typée →
`\operatorname{fib}(n \in \mathbb{Z}) \to \mathbb{Z}` ; signature allégée →
`\operatorname{carre}(x)` (pas de `∈ ?` ni de `→ ?` parasites).

## Modes d'exécution

| Commande | Rôle |
|---|---|
| `rose prog.rose` | interprète/JIT (défaut) |
| `ROSE_AOT=1 rose run prog.rose` | AOT LLVM ORC si helper dispo (parité Rust) |
| `rose latex prog.rose` | LaTeX du programme entier |
| `rose comp prog.rose -o sortie` | exécutable autonome |
| `rose orc prog.rose -o prog.o` puis `gcc -O3 prog.o -o prog -lm` | AOT LLVM |
| `rose link prog.o -o prog` | lier un objet ELF |
| `rose info` / `rose version` | diagnostic |

Env : `ROSE_SANS_JIT=1` (interprète pur), `ROSE_TIMING=1`, `ROSE_DUMP_IR=1`,
`ROSE_ORC_DUMP_IR=1`, `ROSE_AOT_URL`/`ROSE_AOT_SHA256` (miroir helper).

## Build LLVM / WASM (côté développeur seulement — jamais demandé à
l'utilisateur web)

- LLVM (`--features llvm`) : sortir `rose/.cargo/config.toml` du chemin
  (le `crt-static` casse les proc-macros inkwell), puis
  `LLVM_SYS_181_PREFIX=/usr/lib/llvm-18 cargo build --release --features llvm`,
  et RESTAURER le config. Recopier ensuite `target/release/rose-aot` vers
  `~/.cache/rose/rose-aot` ET `~/.cache/rose/rose-aot-v5.8.0-orc-fonctions`.
- WASM : `cargo build --profile wasm-release --target wasm32-unknown-unknown
  --no-default-features` puis `cp target/…/rose.wasm web/` et
  `node web/test-wasm.mjs`. Le `rose.wasm` commité dans `web/` doit rester
  autonome : la page web ne doit JAMAIS exiger cargo ni node.

## Référentiels dans ce dépôt

- `README.md` — référence de syntaxe + bancs + architecture.
- `rose/exemples/tour.rose` — tour complet de la syntaxe (sortie attendue :
  `rose/exemples/tour_sortie.txt`).
- `rose/exemples/exemple_complexe.rose`, `exemple_irrationnels.rose`,
  `exemple_fonctions.rose`, `ffi_libm.rose`, `performance.rose`.
- `examples/` — premiers programmes (arithmétique, fibonacci, flottants).
- `rose/tests/porte_unique.py` — suite de conformité (53 programmes).
- `bench/suite/run_bench_intercale.sh` — banc complexe (Rose JIT / Rose AOT
  / C++ / Rust, 40 tours alternés).
- `docs/WASM.md` — ABI et limitations WASM.

## Installation

- `rose update` : installe / met à jour Rose (64 bits natif). Détecte une
  installation existante, propose réinstaller / réparer / désinstaller.
- **Velopack est LA voie Windows** (WiX et NSIS sont retirés). Le module
  `rose/src/velopack.rs` reproduit le protocole sans dépendance :
  `rose update --velopack` (ou `ROSE_VELOPACK=1`, ou `VELOCABI` posé par le
  launcher) lit `releases.win.json`, compare `current` → `app-<version>`,
  télécharge le paquet complet, vérifie le SHA-256 et le dépose dans
  `packages\` ; le launcher applique au démarrage suivant (deltas + rollback).
  Les arguments internes `--veloapp-updated` / `--veloapp-obsolete` sont routés
  en tête de `main()` (jamais pris pour un fichier).
- Chaîne de build : `installateur/velopack/build.ps1` (Windows) —
  `cargo tauri build` → `vpk pack` → `vpk local` / `vpk push`.
  Vérifiable sans Windows : `sh installateur/velopack/verifier.sh ./rose/rose`
  (6 contrôles : téléchargement, dépôt, à-jour, SHA-256 falsifié, launcher ×2)
  et `cargo test --release velopack`.
  Variables de test : `ROSE_VELOPACK_ROOT` (racine d'installation),
  `ROSE_VELOPACK_URL` (manifeste `file://` ou HTTP).
- **Trois exécutables Windows, liés statiquement** (`+crt-static`, aucune DLL
  `VCRUNTIME140` / `api-ms-win-crt-*` requise) :
  - `rose.exe` — moteur (PE32+ console) ;
  - `rose-aot.exe` — helper AOT LLVM ORC ;
  - `rose-shell.exe` — console interactive autonome (REPL, `rose/src/repl.rs`,
    partagée avec `rose console` : UNE implémentation, deux points d'entrée) ;
  - `Rose-win-Setup.exe` — installeur **GUI** (`rose/src/bin/rose-setup.rs`,
    `setup_gui` Win32 ; `build.rs` pose `/SUBSYSTEM:WINDOWS` +
    `/ENTRY:mainCRTStartup` pour ce binôme SEULEMENT).
  Le moteur `rose.exe` et `rose-aot.exe` sont EMBARQUÉS dans le paquet.
- **Interface de l'installeur — pièges vérifiés** (maquette de référence) :
  - **plein écran** : `WM_SIZING` imposait le ratio 1120:780 ; une fenêtre
    maximisée sur un écran 16:9 devenait plus haute que l'écran et la barre
    d'état passait sous le bord inférieur. L'état `maximise` (suivi dans
    `WM_SIZE`) neutralise le ratio ; `WM_GETMINMAXINFO` borne le minimum ;
  - **barre de titre custom** : `WS_POPUP | WS_THICKFRAME | WS_SYSMENU` +
    `WS_MINIMIZEBOX | WS_MAXIMIZEBOX`, jamais `WS_CAPTION` (sinon la barre
    Windows 11 revient). `WM_NCHITTEST` rend HTCAPTION pour la zone de
    déplacement, HTRIGHT/HTBOTTOM/HTBOTTOMRIGHT pour les bords ;
  - **DPI** : manifeste `PerMonitorV2` embarqué en `RT_MANIFEST` (type 24,
    id 1) — lld-link REFUSE `/MANIFESTINPUT` dès qu'un `.res` est lié
    (« more than one resource obj file not allowed ») ; + double-bufferisation
    (`CreateCompatibleBitmap` + `BitBlt`) et `ANTIALIASED_QUALITY` (4) ;
  - **icône** : le groupe est `RT_GROUP_ICON` (type 14, id 1), PAS `RT_ICON`
    (type 1). Charger `IMAGE_ICON` id 1 laissait la fenêtre sans icône ;
    poser aussi `SetClassLongPtrW(GCLP_HICON)` ;
  - **Kare** est une police d'AFFICHAGE : ses minuscules dessinent des
    capitales (« Options » → « PTIONS »). Ne l'utiliser que pour la marque,
    les titres de section et les boutons, EN MAJUSCULES ; Segoe UI pour le
    texte courant. Elle n'a pas `▸` : ne pas mettre de flèche dans les libellés ;
  - GDI lit un `COLORREF` `0x00BBGGRR` : écrire `0xE62F1E` pour le rouge
    affiche RGB(0x1E,0x2F,0xE6), un BLEU. Toujours passer par
    `const fn couleur(0xRRGGBB)` de `setup_gui.rs` (tout l'installeur était
    bleu avant cette conversion) ;
  - l'échelle du tracé est `min(largeur/1120, hauteur/780)` ;
  - les cartes doivent être assez hautes pour leur contenu (la carte
    « Paquet à installer » débordait de 8 px) ;
  - la rose embarquée a son fond noir rendu TRANSPARENT (`embed-assets.py`)
    et ses pixels sont prémultipliés pour `AlphaBlend` (`AC_SRC_ALPHA`) ;
  - `contour()` trace une vraie bordure (crayon + `NULL_BRUSH`), pas un
    aplat : un pinceau plein masquait le fond des cartes ;
  - la section `.rsrc` exige **un seul nœud par type** : les six icônes
    doivent être six NOMS sous le seul `RT_ICON` (type 3), pas six types
    répétés ; `NumberOfNamedEntries` = 0 ; les entrées de langue pointent
    une entrée de données SANS le bit 31 ; une entrée de données fait
    16 octets. Contrôle : `python3 installateur/velopack/verifier-res.py`.
- **Builds Windows depuis Linux** (clang + lld-link + llvm-lib + SDK/UCRT de
  `cargo-xwin` dans `~/.cache/cargo-xwin/xwin`) :
  `sh installateur/velopack/build-linux.sh` puis
  `sh installateur/velopack/pack-linux.sh` (paquet `.nupkg` + manifeste
  `releases.win.json`, Python 3, sans CLI `vpk`).
  La cible mingw est INUTILISABLE ici (pas de `dlltool`) : rester en msvc.
  Pas de Wine : l'interface se contrôle par
  `python3 installateur/velopack/previsualiser.py` (cinq vues PNG, textes
  réels, couleurs GDI exactes, contrôle mesuré des boutons).
- Sous Linux/macOS, `rose update` simple reste sur la voie artefacts GitHub
  (`docs/telechargements/` + SHA-256) : Velopack n'a pas d'installeur Linux.

## Console / shell — pièges vérifiés

- `repl::preparer_console()` DOIT être appelé au démarrage : sans
  `SetConsoleOutputCP(65001)` les accents sortent en mojibake sur conhost, et
  sans `ENABLE_VIRTUAL_TERMINAL_PROCESSING` les séquences ANSI s'affichent
  littéralement (`←[1;38;5;203m`). Windows Terminal active VT par défaut,
  conhost non.
- La détection de terminal est `GetConsoleMode(GetStdHandle(STD_OUTPUT_HANDLE))`
  sous Windows et `std::io::IsTerminal` ailleurs ; `ROSE_NO_COLOR=1` désactive,
  `ROSE_COLOR=1` force.
- L'interpréteur est récursif : exécuter dans un fil de 16 Mio
  (`repl::executer`), sinon les programmes profonds débordent la pile.
- `braces_balanced` ignore les `#` de commentaire en tête de ligne ET les
  chaînes : un `{` dans un commentaire ne doit pas ouvrir de continuation.
  Attention, `10#` en fin d'expression reste l'opérateur primorielle.
- Un binaire `#[path]` (comme `rose-shell.rs`) doit déclarer TOUS les modules
  atteints transitivement : `interp.rs` appelle `crate::auto_llvm`, oublier
  `mod auto_llvm` casse la compilation.
- Éditeur de ligne : sur Windows on lit `ReadConsoleInputW` en mode brut
  (retirer `ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_MOUSE_INPUT |
  ENABLE_QUICK_EDIT_MODE`, garder `ENABLE_PROCESSED_INPUT` pour Ctrl-C).
  On restaure TOUJOURS le mode d'origine en sortie. `WriteConsoleW` pour
  écrire (Unicode complet). Nested `fn` items n'héritent PAS de l'unsafety :
  les marquer `unsafe fn` et appeler depuis un bloc `unsafe`.
- Un double-clic sur l'exe ferme la console dès la sortie : détecter via
  `GetConsoleProcessList` (1 seul PID) + titre vide, puis attendre Entrée.
  `ROSE_SHELL_PERSIST=0` désactive.
- Centrage d'un cadre en console : compter des CARACTÈRES, jamais `{:<44}`
  sur une chaîne UTF-8 (bordure décalée).

## Notation factorielle (v0.1.x)

| Écriture | Sens | Sortie |
|---|---|---|
| `n!` | factorielle exacte (multi-cœur, facteurs premiers) | entier |
| `n!!` `n!!!` `n!!!!` | **multifactorielle** d'ordre k (k = nb de `!`) : `7!!` → **105** (PAS `(7!)!`) | entier |
| `!n` | **sous-factorielle** (dérangements, A000166) : `!5` → 44 (≠ Kurepa, qui est `n?`) | entier |
| `n#` | **primorielle** : produit des premiers ≤ n (`10#` → 210) | entier |
| `n?` | **terminal** : somme des factorielles 1!+…+n! (`5?` → 154) | entier |
| `n$` | **super-factorielle** : produit 1!·2!·…·n! (`3$` → 12) | entier |
| `rapide(n)` / `factorielle_rapide(n)` | factorielle, voie rapide — `1000000!` en ~0,9 s | entier |
| `approx(n)` | **Stirling** (approximatif, O(1)) : `approx(100)` → `9.332622e157` — ⚠ PAS exact (100! a 158 chiffres) | chaîne `{:.6}e{}` |
| `primorielle(n)` / `terminal(n)` / `super_factorielle(n)` | alias fonctions de `n#` / `n?` / `n$` | entier |
| `multifactorielle(n, k)` | alias de `n!!` (k=2), `n!!!` (k=3)… quand k est calculé | entier |
| `sous_factorielle(n)` / `derangement(n)` | alias de `!n` | entier |
| `view(x)` | visionneuse paginée façon nano (Espace / b / q) | — |
| `type(x)` | nom du type (`entier`, `rationnel`, …) | chaine |

⚠ Cette note disait « `!!` / `!?` sont RETIRÉS » : c'est PÉRIMÉ. `!!`,
`!!!`… sont désormais les multifactorielles, et `!n` en préfixe est la
sous-factorielle (voir la section « Factorielles étendues » plus bas).
Lexage des postfixes (front.rs) :
- `#` est la primorielle s'il suit immédiatement un caractère non-blanc ;
  en début de ligne ou après un blanc il reste le commentaire ;
- `?` est le terminal SAUF en début d'instruction suivi de `(` à blanc près
  (début de fichier, après `\n`, `;` ou `>`) → c'est le si compact `? (c) [ … ]` ;
- `$` est la super-factorielle s'il n'est PAS suivi d'une lettre ; suivi d'une
  lettre il ouvre une variable (`$x`).

Piège : le `#[derive(Clone)]` de `Gz` (i128x.rs, pointeur nu `*mut Mpz`) ne
clone QUE le pointeur — utiliser `clone_val()` sinon double-free/segfault.

## Installeur Windows : ressources et « Cette application ne peut pas s'exécuter »

- **`OffsetToData` d'un PE est une RVA, pas un offset de section.** Le script
  qui écrivait un objet COFF `.rsrc` à la main n'émettait aucune relocation :
  le lieur laissait `0x2e8`, Windows le résolvait dans `.text` et lisait du
  CODE MACHINE à la place du manifeste → refus au lancement. Symptôme :
  `pe.get_data(OffsetToData, Size)` sur `RT_MANIFEST` commence par des octets
  d'instructions x64 au lieu de `<?xml`. **Diagnostic rapide** :
  `python3 installateur/velopack/verifier-res.py rose-setup.exe` (convertit les
  RVA en offsets et exige que chaque ressource tombe dans `.rsrc`).
- **Ne pas écrire l'objet COFF à la main.** `llvm-rc` produit le `.res` MSVC
  canonique que `lld-link` lie correctement (il calcule les RVAs lui-même).
  `pack-res.py` génère un `.rc` temporaire et appelle `llvm-rc`.
- **`llvm-rc` NE découpe PAS un `.ico`.** Donné à `RT_ICON`, il embarque le
  fichier entier comme UNE icône, sans `RT_GROUP_ICON` : aucune icône
  utilisable. Il faut extraire chaque taille (`RT_ICON 1..n`) et écrire le
  `RT_GROUP_ICON 1` à la main.
- **⚠⚠ Une entrée `RT_ICON` doit contenir le DIB BRUT, pas un `.ico`.**
  Écrire un `.ico` d'un seul élément (en-tête `00 00 01 00` + entrée + image)
  produit une ressource qui EXISTE — arbre valide, groupe valide, tailles
  concordantes — mais que Windows ne décode JAMAIS : la fenêtre s'ouvre sans
  icône, l'Explorateur affiche l'icône générique. C'est le piège qui a fait
  rater l'icône deux fois alors que tous les contrôles passaient.
  Donner `e["donnees"]` (les octets de l'image seuls) à `llvm-rc`.
- **Contrôle qui l'attrape** : chaque `RT_ICON` doit commencer par un
  BITMAPINFOHEADER (`biSize` 12/40/108/124) ou la signature PNG
  (`89 50 4E 47`), JAMAIS par `00 00 01 00`. Voir `verifier-res.py`.
- **Taille d'une icône dans le groupe** = `len(donnees)` quand on donne le
  DIB brut (le `+22` ne valait que pour l'ancien conteneur `.ico`).
- **Deux icônes différentes** : `setup.ico` = la ROSE de marque (rouge et
  verte) pour la fenêtre et le setup ; `rose.ico` = icône de TYPE DE FICHIER
  (document marqué « .ROSE »). Ne pas les confondre.
- **`IMAGE_SECTION_HEADER` fait 40 octets** : 8s + 7×I + 2×H + I. Oublier
  `PointerToRelocations` décale tous les champs suivants.

## `rose-aot.exe` : un stub INUTILE, à ne pas livrer

- `rose/src/bin/rose-aot.rs` est compilé **sans** la feature `llvm` par le
  script de build : il pèse 432 Ko et ne peut RIEN compiler. Le VRAI helper
  (2,1 Mo, avec LLVM) est dans `~/.cache/rose/rose-aot-v5.10.0`, téléchargé
  par `rose.exe` au premier `rose orc` / `rose link`.
- Le livrer dans l'installation donnait l'illusion que la chaîne AOT était
  disponible. `installer()` ne le pose plus, et `pack-res.py` ne l'embarque
  plus (l'id `RT_RCDATA 4` est laissé LIBRE, ne pas le réutiliser).
- `LLVM-C.dll` (90 Mo, `installateur/staging/`) n'est référencé par AUCUN
  code : supprimé. Il ne servait à rien depuis que le helper embarque LLVM.

## Console : compter les COLONNES, pas les caractères

- `redessiner` (éditeur de ligne Win32) place le curseur à
  `invite.chars().count() + curseur`. Or l'invite est COLORÉE : elle contient
  des séquences ANSI (`\x1b[1;38;5;203mrose> \x1b[0m`) qui n'occupent
  **aucune** colonne mais comptent dans `chars().count()`. Résultat mesuré :
  le curseur était ~20 colonnes trop à droite — un « gros décalage » entre
  le `>` et le texte saisi.
- Utiliser `colonnes()` (fonction module, testée) : elle ignore les
  séquences CSI.

## PNG : ZLIB, pas DEFLATE brut (le logo invisible)

- Les IDAT d'un PNG contiennent un flux **ZLIB** (en-tête 2 octets `78 DA`,
  puis DEFLATE). `crate::inflate::decompresser()` attend du **DEFLATE brut**.
 -feeding des IDAT à `decompresser` consomme l'en-tête comme des données :
  le flux part en vrille et le décodeur rend `None`.
- Conséquence mesurée : la rose de l'installeur ne s'affichait NI dans la
  barre de titre, NI au-dessus de « ROSE » — `dessiner_rose` rendait `false`
  et l'appel était **silencieusement** ignoré. Aucun message, aucune
  erreur : juste un logo absent.
- Utiliser `crate::inflate::decompresser_zlib()`.
- Le décodeur vit dans `rose/src/png.rs` (module propre) pour être TESTABLE
  sous Linux : `setup_gui` n'est compilé que sur Windows, un bug y resterait
  invisible. Trois tests vérifient que le PNG réel se décode et contient des
  pixels opaques rouges ET verts (une image entièrement transparente ne
  s'afficherait jamais).

## PATH : diffuser le changement d'environnement

- Écrire `HKCU\Environment\Path` ne met PAS à jour les applications
  DÉJÀ lancées : elles gardent l'environnement du démarrage. Mesuré :
  `rose` marchait dans cmd (ouvert après l'installation) mais VS Code
  disait « rose n'est pas reconnu » (lancé avant).
- Diffuser `WM_SETTINGCHANGE` / `"Environment"` via `SendMessageTimeoutW`
  (`HWND_BROADCAST`, `SMTO_ABORTIFHUNG`, 3 s).

## Icônes : deux icônes, deux usages

- `rose.ico` = icône de TYPE DE FICHIER (document « .ROSE ») → association
  `Rose.File\DefaultIcon`, et `SHChangeNotify(SHCNE_ASSOCCHANGED)` pour que
  l'Explorateur relise le cache.
- `rose_app.ico` = la ROSE de marque → profil Windows Terminal, barre des
  tâches, raccourcis.
- Se tromper donnait un document dans le menu des onglets de Windows
  Terminal (mesuré).

## Icônes : TROIS usages, trois fichiers

- `rose.ico` = icône de TYPE DE FICHIER (document « .ROSE ») → association
  `Rose.File\DefaultIcon`, et `SHChangeNotify(SHCNE_ASSOCCHANGED)` pour que
  l'Explorateur relise le cache.
- `rose_app.ico` = la ROSE de marque → profil Windows Terminal, barre des
  tâches, raccourcis.
- `rose.exe` embarque la ROSE DE MARQUE (`rose.rc` : `1 ICON "setup.ico"`),
  parce que c'est son icône dans l'Explorateur, dans la barre des tâches et
  — surtout — sur la FENÊTRE DE CONSOLE qui s'ouvre au double-clic sur un
  `.rose`. Avant, c'était le document : la console affichait un document.

## Console ouverte par double-clic : icône et attente

- `rose/src/console_win.rs` :
  - `preparer()` pose le titre (« Rose 0.1.5 ») et l'icône de la fenêtre
    via `PostMessageW` + `WM_SETICON` (le groupe embarqué dans `rose.exe`) ;
  - ⚠ **JAMAIS `SendMessageW` sur la fenêtre de console** : elle appartient à
    `conhost.exe`, et un envoi SYNCHRONE depuis le programme console peut se
    bloquer (chacun attend le verrou que l'autre détient) — le programme ne
    rend plus la main et l'utilisateur ne voit qu'une fenêtre VIDE, avec les
    seules bordures (mesuré). `PostMessageW` ne bloque jamais ; l'icône est
    en outre posée depuis un fil séparé ;
  - `attendre_fermeture()` attend une frappe quand la console n'appartient
    qu'à nous (`GetConsoleProcessList` renvoie 1 seul PID). Sans ça, la
    fenêtre se fermait avant qu'on ait pu lire la sortie.
- `ROSE_NO_PAUSE=1` désactive l'attente (scripts, CI).

## GDI+ : ATTENTION à l'ordre des canaux

- `COLORREF` GDI = `0x00BBGGRR` ; `ARGB` GDI+ = `0xAARRGGBB`. Passer la
  valeur telle quelle **échange rouge et bleu** : le lime `0x9ACD32`
  s'affichait en CYAN `0x32CD9A` (mesuré). Toujours passer par
  `argb_depuis_colorref()`.
- `GdipDeleteImage` n'existe pas : c'est `GdipDisposeImage`.
- ⚠⚠ **CORRECTION : la rose se dessine en GDI pur (`CreateDIBSection` +
  `AlphaBlend`), PAS en GDI+.** L'ancienne note affirmait l'inverse ; elle
  était fausse. `GdipCreateBitmapFromScan0` avec un `scan0` pointant sur un
  `Vec` Rust laisse GDI+ référencer — voire libérer — un tampon qui nous
  appartient : double libération à la destruction du `Vec`, donc CRASH de
  l'installeur pendant le dessin. C'est une des causes du symptôme « fenêtre
  avec ses seules bordures ».
  Le chemin qui marche : DIB 32 bits top-down (`biHeight` négatif), pixels
  BGRA **prémultipliés** (`c = c * a / 255`), puis `AlphaBlend` avec
  `AC_SRC_ALPHA` (blend_op 0, blend_flags 0, constant_alpha 255,
  alpha_format 1) depuis un DC mémoire.
- GDI+ ne sert QUE pour les FORMES (cercles, coins arrondis, anticrénelage),
  où aucun tampon n'est partagé — et chaque appel doit avoir son repli GDI.

## Shell : un LANCEUR, pas une copie du moteur

- `rose-shell.exe` ne doit PAS recompiler le moteur (`#[path]` sur front,
  interp, run, jit, latex…) : le `.exe` faisait 6,9 Mo, le langage était
  embarqué DEUX fois dans l'installation. Il localise `rose.exe` (`ROSE_EXE`,
  puis à côté de lui, puis `PATH`) et lui délègue `rose console`.
- Un test `le_shell_n_embarque_pas_le_moteur` garde la taille sous 4 Mo en
  release : c'est le garde-fou contre un retour à l'ancienne architecture.

## Console Windows muette : le layout de `INPUT_RECORD`

- **`INPUT_RECORD`** = `EventType` WORD (2) + padding (2) + union (16) = 20 o.
  **`KEY_EVENT_RECORD`** = `bKeyDown` BOOL (4), `wRepeatCount` WORD,
  `wVirtualKeyCode` WORD, `wVirtualScanCode` WORD, `UnicodeChar` WORD,
  `dwControlKeyState` DWORD. Un `Cle` mal ordonné lit `code` à l'offset 4 au
  lieu de 10 : la bannière s'affiche mais AUCUNE touche ne réagit (mesuré).
  `bKeyDown` est un BOOL séparé, pas un bit de `dwControlKeyState`.
- **Ne PAS activer `ENABLE_VIRTUAL_TERMINAL_INPUT` (0x0200)** quand on lit
  avec `ReadConsoleInputW` : l'entrée VT traduit les touches en séquences
  d'échappement, que `ReadConsoleInputW` ne reçoit pas. C'est la seconde
  cause du clavier mort. `preparer_console` doit le RETIRER.
- Profil Windows Terminal : déposer
  `%LOCALAPPDATA%\Microsoft\Windows Terminal\Fragments\Rose\rose.json`
  (`commandline`, `icon`, `startingDirectory`, `tabTitle`). Windows Terminal
  fusionne automatiquement ces fragments : ne jamais éditer `settings.json`.

## CI et binaires de test (pièges mesurés le 15/11)

- **`cargo test` ment sur x86_64.** `rose/.cargo/config.toml` force
  `-C target-feature=+crt-static` : le binaire de test devient un `static-pie`
  que le runner GitHub x86_64 (rustc 1.100) **ne démarre pas** → SIGSEGV
  immédiat, avant le premier test, sans aucun message. En local (aarch64) le
  même binaire tourne : le bug est invisible.
  Parade CI : `RUSTFLAGS=""` + `CARGO_PROFILE_RELEASE_STRIP=none` sur l'étape
  de test. Les binaires PUBLIÉS (`docs/telechargements/rose-aarch64-linux`)
  sont dynamiques et ne sont pas concernés.
- **`--target <host>` change le chemin du harnais** mais ne corrige pas le
  SIGSEGV : le cdylib n'a pas de point d'entrée. Utiliser `cargo t`
  (alias `--bins` dans `.cargo/config.toml`) : `src/main.rs` déclare les
  mêmes `mod` que `lib.rs`, donc les 52 tests de la lib y tournent.
- **Un commentaire YAML contenant ` : `** casse le workflow (GitHub le
  signale comme « .github/workflows/ci.yml failure », sans logs). Vérifier
  avec `python3 -c "import yaml; yaml.safe_load(open(...))"` AVANT de pousser ;
  `sed -n 's/^host: //p'` dans un `run:` non quoté en est un exemple réel.
- **`.gitignore` : `bin/` sans ancre** correspondait à `rose/src/bin/` :
  `rose-aot.rs` et `rose-shell.rs` n'ont jamais été versionnés, et un clone
  neuf de `main` ne compilait pas. La règle doit être `/bin/`.

## Fenêtre de l'installeur « transparente » puis qui disparaît — les 6 causes

Symptôme mesuré : « la fenêtre s'ouvre avec ses seules BORDURES, sans
intérieur, puis disparaît au bout d'un moment ». Ce n'était PAS un bug de
ressources : six défauts cumulés, tous dans `setup_gui.rs`.

1. **`WM_PAINT` appelait `dessiner` SANS isolation.** Une panique (indice hors
   bornes, allocation) déroulait la pile du fil de messages : le processus
   mourait pendant le premier repeint. Windows avait déjà créé la fenêtre → on
   n'en voyait que le cadre, puis elle disparaissait avec le processus.
   Correction : `std::panic::catch_unwind` AUTOUR de `dessiner` **et** dans le
   gestionnaire `WM_PAINT` (double ceinture).
2. **`BeginPaint` sans `EndPaint` en cas de panique.** Le DC de mise à jour
   restait verrouillé : toutes les invalidations suivantes étaient ignorées, la
   fenêtre ne se repeignait plus JAMAIS. `EndPaint` doit être appelé
   INCONDITIONNELLEMENT (le tracé détaillé est dans une fonction séparée).
3. **Tout le tracé passait par un DC mémoire** copié à la fin : un plantage en
   route laissait l'écran VIDE, même sans fond. On peint le FOND sur `dc_ecran`
   AVANT toute décoration.
4. **`GetMessageW` rend -1 en cas d'erreur** ; la boucle `while … > 0` sortait
   sans diagnostic et le processus se terminait proprement → « disparaît ».
   Distinguer 0 (`WM_QUIT`) de -1 (erreur) ; en cas d'erreur, journaliser et
   laisser la fenêtre ouverte. Utiliser aussi `SW_SHOWNORMAL` (1), pas
   `SW_SHOW` (5), et `SetForegroundWindow` : sinon la fenêtre s'ouvre DERRIÈRE.
5. **`ZONES_TITRE` vide au premier `WM_NCHITTEST`** (`[0,0,0]`) : `zone_titre`
   rendait 3 = FERMER pour TOUTE la barre de titre. Un simple clic dans la
   barre fermait l'installeur. Les zones doivent être recalculées si elles ne
   sont pas cohérentes avec la largeur courante.
6. **`WM_LBUTTONUP` en zone de déplacement tombait dans `_ => {}`** puis testait
   les zones du pied de page : un clic sur la barre de titre pouvait déclencher
   « Annuler » (fermeture). `zone_titre == 0` doit sortir immédiatement.

Gardes-fous vérifiés statiquement par `verifier-res.py` (10 motifs exigés dans
le source). Sous Linux la fenêtre ne peut pas être exécutée : c'est le SEUL
contrôle possible, avec `previsualiser.py`.

## Contenu de la fenêtre invisible : hauteurs de cartes écrites en dur

- Les cartes avaient des hauteurs CONSTANTES sur la grille 1120 × 780
  (`s(150)`, `s(186)`, licence = tout l'espace restant). Dès que la fenêtre
  était plus petite, la carte était plus basse que son texte et `DrawText`
  n'en dessinait que la première ligne — la fenêtre paraissait VIDE.
- Règle : la hauteur d'une carte suit son TEXTE MESURÉ (`mesurer_texte` +
  `paint::hauteur_texte` + `paint::hauteur_carte`). `paint.rs` est un module
  PUR, testé sous Linux — c'est le seul moyen de vérifier ces calculs sans
  Windows.
- `points_ecrits` a un plancher de **13 px** (et non 9) : à 9 pt sur une
  fenêtre réduite, la police tombait à 9 px, illisible.
- Ajouter `DT_END_ELLIPSIS` (0x8000) partout où le texte peut dépasser :
  `DrawText` TRONQUE net sinon, sans « … ».
- `CreateFontW` peut rendre NULL : `SelectObject(dc, NULL)` renvoie NULL et
  `DrawTextW` dessine avec la police de stock (textes GÉANTS qui recouvrent
  tout). Toujours tester `creer_police()` avant de dessiner.
- Le repli GDI de `cercle` et `arrondi` ne doit JAMAIS être sauté : sans lui,
  les pastilles, les cartes et les boutons sont ABSENTS quand GDI+ ne
  s'initialise pas.

## Logo de l'installeur : la bande de fond semi-opaque

- `logo-rose.png` a un fond sombre dont l'**alpha varie de 0 à ~165** (ce n'est
  PAS du noir pur). L'ancien `embed-assets.py` ne rendait transparents que les
  pixels dont les trois canaux étaient sous 40, SANS regarder l'alpha : une
  bande de fond semi-opaque subsistait autour de la rose.
- La boîte englobante « alpha > 8 » englobait alors TOUT le visuel
  (273..1126 × 62..902) au lieu de la fleur (424..951 × 230..902), et la rose
  était collée au coin d'un carré presque transparent → logo invisible.
- Critère correct : transparent si `alpha <= 8` **ou** si le pixel est sombre.
- **PAS de canevas carré** : la rose avec sa tige fait **138 × 172**. La
  centrer sur un carré ajoutait deux bandes transparentes et le logo
  n'occupait qu'une fraction de la place prévue. Garder les proportions et
  mettre à l'échelle sur le plus grand côté (`dessiner_rose` fait de même).
- Tests dans `png.rs` : dimensions (138, 172), marges ≤ 4 px, et
  `sombres_opaques == 0` (aucun pixel noir opaque = bande nettoyée).

## Glyphes absents : carrés vides au lieu du texte

- `⚿` (U+26BF) n'existe dans AUCUNE police Windows courante : le badge
  « MODE UTILISATEUR » sortait avec un carré vide. Kare ne couvre pas `✓`
  non plus.
- `GetGlyphIndicesW` rend `GDI_ERROR` (0xFFFFFFFF) pour un caractère non
  couvert : `famille_couvre()` teste chaque caractère et retombe sur Segoe UI.
- Ne jamais supposer qu'`AddFontMemResourceEx` a réussi : `kare_disponible()`
  demande à GDI le glyphe réel de « R » ; sinon tout retombe sur Segoe UI
  (au lieu d'un « ROSE » en Segoe sans que personne ne le remarque).

## Factorielles étendues : multifactorielles et sous-factorielle

- **`n!!` n'est PAS `(n!)!`.** C'est la **multifactorielle** d'ordre k (k =
  nombre de `!`) : `n!⁽ᵏ⁾ = ∏_{i=0}^{⌊(n−1)/k⌋} (n − i·k)`.
  `7!!` = 7·5·3·1 = **105**, alors que `(7!)!` = 5040! = 16 686 chiffres.
  Valeurs : `8!!`=384, `8!!!`=80, `9!!!`=162, `11!!!!`=231, `0!!`=`1!!`=1.
- **Le lexer doit compter les `!` consécutifs en UN token** (`Tk::MultiFacto(k)`).
  L'ancien code émettait deux `Tk::Bang` et le parser appliquait la
  factorielle deux fois — c'est exactement le piège de la notation.
- **`!n` préfixe = SOUS-FACTORIELLE** (dérangements, A000166) :
  `!0`=1, `!1`=0, `!2`=1, `!3`=2, `!4`=9, `!5`=44. Récurrence
  `!n = n·!(n−1) + (−1)^n`.
- ⚠⚠ **`!n` est AMBIGU en mathématiques** : ce symbole désigne aussi la
  **factorielle gauche de Kurepa** (A003422 : 0, 1, 2, 4, 10, 34, 154…), qui
  vaut `∑_{k=0}^{n−1} k!`. Rose réserve `!n` aux dérangements et garde
  `n?` (terminal) pour Kurepa — les deux sont donc accessibles sans
  ambiguïté. Ne pas « corriger » `!n` en Kurepa : cela casserait `n?`.
- **La décision postfixe/préfixe se prend sur le DERNIER TOKEN ÉMIS**, jamais
  sur le caractère précédent. Mesuré : `5 !` (blanc avant `!`) était lu comme
  préfixe, le `!` attendait un opérande et avalait la ligne suivante
  (« Attente d'une expression, trouvé '>>' »).
- `terminal` (`n?`) était typé `Dyn` **même sur un entier** : `f(0?)` était
  refusé (« Argument de f : ? reçu, entier attendu »). Le type suit
  maintenant l'opérande : entier → `Entier`, sinon `Dyn`.
- Fonctions nommées : `multifactorielle(n, k)` et `sous_factorielle(n)` /
  `derangement(n)` — utiles quand k est calculé.
- Bench et vérification : `bench/suite/factorielles.rose` (temps + nombre de
  chiffres, sans imprimer les valeurs) et
  `bench/suite/verifier_factorielles.rose` (58 comparaisons à des constantes
  OEIS). ⚠ `n$` explose (∏k! ≈ n!^(n+1)/2) : à N=5000 elle fait 38 millions
  de chiffres en ~15 s ; à N=20000 le bench dépasse plusieurs minutes. Elle
  est donc mesurée sur un N séparé, plus petit.

## Trois factorielles : exacte, approximative, « rapide »

Ne pas confondre — c'est une source d'erreur classique :

| Écriture | Résultat | Coût | Usage |
|---|---|---|---|
| `n!` | entier **exact**, tous les chiffres | arbre de produit, O(n log²n) | résultat final |
| `factorial_approx(n)` (`approx`) | `"m.eE"` — **Stirling** √(2πn)(n/e)^n + série asymptotique `1/(12n) − 1/(360n³) + 1/(1260n⁵)` | O(1) | **taille** seulement (`100000!` ≈ `2.824229e456573`) |
| `factorielle_rapide(n)` (`rapide`, `factorial_rapide`) | entier exact, même moteur crible+arbre | = `n!` | alias explicite |

- ⚠ `factorial_approx` **n'est pas exact** : `factorial_approx(100)` =
  `9.332622e157` (14 caractères) alors que `100!` a **158 chiffres**. Un test
  le vérifie (`interp::tests::factorial_approx_n_est_pas_le_nombre_exact`).
- `factorial_approx(0)` et `(1)` rendent `"1"`, pas `"1.000000e0"`.

## Rose N'UTILISE PAS GMP — ce qui la rend rapide

⚠⚠ **Rose ne dépend jamais de GMP.** Tout est en Rust pur avec `num-bigint`.
GMP sert UNIQUEMENT de référence de comparaison dans `bench/suite/fact_gmp.c`
et `bench/suite/fact_rs.rs` (rug).

| Optimisation | Gain mesuré |
|---|---|
| Suppression des clones d'opérandes | `n!` 41 → 25 ms |
| Seuil de parallélisation de l'arbre (500 000 bits) | `n#` 2 → 1 ms |
| **NTT à 2 modules de 62 bits** (> 16,4 M bits) | `1 000 000!` 3 797 → **906 ms** |

⚠⚠⚠ **LE FAIT QUI CHANGE TOUT** : `num-bigint` fait DÉJÀ école → Half-Karatsuba
→ Karatsuba (≤256 limbs) → **Toom-3 en place**. Un Toom-3 maison est inutile
(4× plus lent que le sien). Ce qui lui manque est la **FFT**.

- **Seuil NTT : 16 384 000 bits (512 000 limbs).** ⚠ La bascule ISOLÉE est à
  256 000 limbs, mais dans un ARBRE de produit le seuil 8,19 M bits rendait
  `1 000 000!` PLUS LENT (1074 ms contre 913). Toujours mesurer sur l'arbre réel.
- **2 modules de 62 bits, pas 3 de 30** : mesuré 1,6× plus rapide
  (498 ms contre 790 à 512 000 limbs). Modules vérifiés premiers, racines
  testées : `(2850325311931809793, 5)` et `(3874380531890126849, 3)`.
- ⚠ **Racine primitive à VÉRIFIER** : `(2013265921, 5)` est faux
  (`5^((m−1)/2) ≡ 1`), il faut 31. Tester `g^((m−1)/2) ≡ −1`.
- ⚠ **Toom-3 degré 4** : les 4 points {0,1,−1,2} ne déterminent qu'un degré
  ≤ 3 ; retirer `w4·x⁴`, interpoler, puis le rajouter.
- ⚠ **2-adicité** : pour une NTT de longueur 2^k, il faut `m−1 ≡ 0 mod 2^k`.
  `2^61−1` a une 2-adicité de 1 → INUTILISABLE (piège classique).

### Pièges Windows — icônes et PATH (mesurés le 16/11)

**Icônes : ⚠⚠⚠ DEUX fichiers `rose.ico` DIFFÉRENTS (vérifié visuellement).**
- `rose/rose.ico` (6 tailles, **64 700 o**, md5 `2ea01977…`) = **LA ROSE
  SEULE** (low-poly, rouge, tige verte). C'est le LOGO DE BASE.
- `installateur/assets/rose.ico` (6 tailles, **112 510 o**, md5
  `306732b5…`) = **UN DOCUMENT « .ROSE »**. C'est le TYPE DE FICHIER.

Rôles :
- `rose/rose.ico` → **rose.exe** (moteur) ET `rose_app.ico` dans `bin\`
  (raccourcis, profil Windows Terminal).
- `installateur/assets/rose.ico` → `rose.ico` dans `bin\` (type `.rose`).
- `rose_shell.ico` (7 tailles, 113 565 o, fenêtre de terminal) →
  **rose-shell.exe** uniquement.
- `rose_setup.ico` → **rose-setup.exe** (la boîte d'installation).

⚠⚠ ERREUR À NE PAS REFAIRE : j'ai posé une copie `rose_base.ico` de
`installateur/assets/rose.ico` sur `rose.exe` — donc le DOCUMENT sur le
moteur, deux fois faux. `rose_base.ico` est supprimé.

Vérification : `llvm-readobj --coff-resources <f>.res` → `rose.res` = 6
icônes de **691/1908/3547/5647/16423/36382 o** (tailles de `rose/rose.ico`) ;
`rose-shell.res` = 7. Dans `rose-setup.res`, RCDATA **6 = 112 510 o**
(document) et **7 = 64 700 o** (la rose) — s'ils sont égaux, c'est faux.

⚠ `build.rs` doit déclarer `rerun-if-changed` sur les `.ico` : sans cela,
remplacer une icône ne reconstruit PAS le `.res` (cache périmé mesuré).

**`rose-aot` : PAS un binaire d'installation.** C'est le helper LLVM
(~libLLVM) que `rose.exe` télécharge à la demande dans `~/.cache/rose`
(`auto_llvm.rs`). Le stub construit sans `--features llvm` (480 Ko) ne peut
RIEN compiler : il était livré dans le stage alors que `setup_core` ne
l'installe pas = fichier mort et trompeur. **Retiré de `build-linux.sh`.**
La feature `llvm` ne compile pas sur cette machine
(`inkwell_internals` : « cannot produce proc-macro for this target »).
- `llvm-windres rose/rose.rc --target=pe-x86-64 -O coff -o x.res` compile
  sous Linux (rc=0, 113 300 o) : la vérification des `.rc` est possible sans
  Windows. En revanche la cible `x86_64-pc-windows-msvc` NE COMPILE PAS ici
  (`windows_compat.c` → `cc: unrecognized option '-m64'`).

**PATH : trois bugs qui font échouer `rose` après installation.**
1. LECTURE : `ligne.split_whitespace()` puis `parts[2..].join(" ")`
   reconstruit de travers un chemin avec espace
   (`C:\Program Files\Rose\bin`). → couper sur le TYPE
   (`REG_EXPAND_SZ` / `REG_SZ`), jamais sur les espaces.
2. FILTRE : `contains("\rose")` retirait AUSSI le bon dossier. → comparer
   les derniers éléments normalisés.
3. SUR-FILTRAGE : `avant == "rose"` retirait un dossier Rose LÉGITIME dès
   qu'on installait ailleurs. Mesuré : gardés = `['C:\Windows\System32']`
   seulement, le bon dossier PERDU. → ne retirer que si le dossier n'a plus
   `rose.exe` / `rose` (Rose mort), ou s'il est le dossier courant.
4. Le même bug de LECTURE existait dans `desinstaller()` : corrigé aussi.
5. Après écriture, `verifier_path()` RELIT le registre et CONFIRME la
   présence de `bin\` : écrire la clé ne prouve rien.
6. Écrire `HKCU\Environment\Path` ne met PAS à jour les processus déjà
   lancés (VS Code ouvert avant l'installation dit « rose non reconnu »).
   Le code diffuse `WM_SETTINGCHANGE` ; pour les shells déjà ouverts il faut
   relancer la session ou `set PATH=%PATH%;<bin>`.

### Ce qui a été essayé et ABANDONNÉ (ne pas retenter sans preuve)

- **RÉDUCTION DE BARRETT** : essayée pour remplacer `u128 % u64` dans la NTT.
  REJETÉE : `μ = u64::MAX/m` ne couvre pas un produit de 124 bits (test rouge,
  valeurs énormes). Mesuré par ailleurs : `% u128` n'est que **1,8×** plus
  lent qu'un `% u64` (3,0 ms contre 1,7 ms pour 1M opérations) — ce n'est PAS
  le goulot de la NTT.
- **MONTGOMERY** : rejeté par calcul — deux multiplications 64×64 + décalage
  coûtent plus que le `% u128` qu'il évite, vu le facteur 1,8 ci-dessus.
- **UNE SEULE TRANSFORMÉE PAR MODULE** (c = a + i·b, i² = −1) : DEUX formes
  essayées, toutes deux FAUSSES (tests rouges) :
  1. « c ← c² puis NTT inverse, partie imaginaire » : la reconstruction par
     `c[(n−k)%n]` est fausse ;
  2. « symétrie hermitienne A[k] = (C[k]+C[n−k])/2 » : **ne s'applique pas** —
     la symétrie hermitienne suppose des coefficients RÉELS, alors qu'ici x
     et y sont des entiers mod m (le corps fini n'a pas de conjugaison).
  On garde donc 6 NTT (3 par module) : c'est incompressible.
- **PARALLÉLISATION DES PUISSANCES p^e** (factorielle) : mesuré 904–941 ms
  avec, 906–920 ms sans — aucun gain net. Le temps est dans l'ARBRE de
  produit, pas dans le calcul des p^e.
- **SEUIL NTT BALAYÉ** (1 000 000!, mesuré) : 2,05 M bits → 1568 ms ;
  4,1 M → 1226 ; 6,14 M → 1024 ; 9,2 M → 1053 ; 16,4 M → **907** ;
  32,7 M → 900–941 (variance) ; ∞ → 912. Le meilleur stable est **16,4 M**.


- **Inverse de Newton pour la division** : désormais CORRECT
  (`div_rem_newton`, testé 40→1000 limbs contre num-bigint) MAIS **25× PLUS
  LENT** que num-bigint à toutes les tailles mesurées :
  40 limbs → 2,9 µs (Newton gagne), 100 → 113 µs vs 4,6 µs,
  1000 → 3,92 ms vs 169 µs, 20000 → 491 ms vs 18,2 ms.
  → `div_rem` délègue à `num-bigint`. La version Newton est conservée
  (`#[allow(dead_code)]`) pour un futur chantier « division en place ».
- **Les pièges de l'inverse de Newton** (trois essais ratés avant) :
  1. `v ← v + (B^p − d·v)·v/B^p` **NE CONVERGE PAS** (l'écart ne bouge pas).
     La bonne itération est `v ← ⌊v·(2·B^p − d·v)/B^p⌋`.
  2. Tronquer `d·v` à p limbs **fausse le quotient** : produit COMPLET.
  3. Le ré-échelonnage entre paliers est `(m2−m)` LIMBS, pas `2(m2−m)`.
  4. L'amorçage `⌊(B²−1)/d_top⌋ · B^(n−1)` (le décalage est indispensable).
  5. Une échelle FIXE (sans paliers) est exacte mais **catastrophique** :
     40,9 s pour la conversion de 1M! contre 2,87 s.
- **Burnikel-Ziegler** : récursion infinie quand le demi-limb ne décroît plus.
- **Conversion décimale maison** : `num-bigint` fait déjà le même découpage
  récursif (`to_radix_digits_le_divide_and_conquer`) ; notre version est plus
  lente (3,07 s contre 2,88 s). ⚠ Piège : diviser par 10⁹ à chaque niveau fait
  600 000 niveaux de récursion → **SIGABRT** (pile).
- Le goulot réel de la conversion est la **division**. L'inverse de Newton
  est maintenant CORRECT mais plus LENT que Knuth de num-bigint (25×) :
  le chemin pour battre GMP passe par une division EN PLACE, pas par Newton.

## Bench comparatif entre langages

`bench/suite/bench_factorielles.sh [N]` compare Rose (num-bigint), Rose
(+GMP), GMP/C (`bench/suite/fact_gmp.c`) et Python sur les mêmes opérations.
Ordre de grandeur à N = 100 000 : Rose bat Python de 3× à 60× et Rust pur de
1,4× ; reste à ~2× de GMP/C. À `1 000 000!` : Rose 906 ms, Rust pur 2 231 ms,
GMP/C 173 ms.

## Marqueur LaTeX par ligne : `%%`

- Une ligne qui **commence** par `%%` (à blanc près) est une annotation TeX,
  ignorée par le lexer comme un commentaire — le programme reste EXÉCUTABLE.
- `rose latex prog.rose` rend TOUT le programme ; `rose latex prog.rose
  --lignes` (ou `-l`) ne rend QUE les lignes `%%`, séparées par `\\`.
- ⚠ Le lexer doit reconnaître `%%` EN DÉBUT DE LIGNE (`i == 0` ou précédé de
  `\n`) : `%` est l'opérateur modulo en Rose, et sans ce cas le programme est
  rejeté (« Attente d'une expression, trouvé '%' »).
- La super-factorielle `n$` s'affiche `\operatorname{sf}(n)` en LaTeX : `$`
  est le délimiteur des maths, `3\$` se lirait comme un prix.

## Style

- Messages et commentaires du moteur en français ; noms de fonctions Rust
  du moteur en français également (`expr_type`, `deplier_i_rec`…).
- Un programme Rose exemple se termine par ses sorties vérifiées : mets à
  jour `tour_sortie.txt` quand `tour.rose` change.
- Commit en français, préfixe conventional (`feat:`, `fix:`, `perf:`,
  `docs:`, `bench:`, `build:`).
