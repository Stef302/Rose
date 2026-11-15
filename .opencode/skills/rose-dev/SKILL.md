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
| `n#` | **primorielle** : produit des premiers ≤ n (`10#` → 210) | entier |
| `n?` | **terminal** : somme des factorielles 1!+…+n! (`5?` → 154) | entier |
| `n$` | **super-factorielle** : produit 1!·2!·…·n! (`3$` → 12) | entier |
| `rapide(n)` / `factorielle_rapide(n)` | factorielle, voie rapide — `1000000!` en ~0,9 s | entier |
| `approx(n)` | Stirling complet, série `1/12n − 1/360n³ + 1/1260n⁵` | chaîne `{:.6}e{}` |
| `primorielle(n)` / `terminal(n)` / `super_factorielle(n)` | alias fonctions de `n#` / `n?` / `n$` | entier |
| `view(x)` | visionneuse paginée façon nano (Espace / b / q) | — |
| `type(x)` | nom du type (`entier`, `rationnel`, …) | chaine |

Les opérateurs analogues `!!` / `!?` sont RETIRÉS (reportés) : `!` reste la
seule factorielle postfixe. Lexage des nouveaux postfixes (front.rs) :
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

## GDI+ : ATTENTION à l'ordre des canaux

- `COLORREF` GDI = `0x00BBGGRR` ; `ARGB` GDI+ = `0xAARRGGBB`. Passer la
  valeur telle quelle **échange rouge et bleu** : le lime `0x9ACD32`
  s'affichait en CYAN `0x32CD9A` (mesuré). Toujours passer par
  `argb_depuis_colorref()`.
- `GdipDeleteImage` n'existe pas : c'est `GdipDisposeImage`.
- `AlphaBlend` sur un DC mémoire ne rend RIEN : la rose n'apparaissait pas
  (ni barre de titre, ni panneau). Utiliser GDI+
  (`GdipCreateBitmapFromScan0` + `GdipDrawImageRectI`), qui gère l'alpha et
  la mise à l'échelle.

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

## Style

- Messages et commentaires du moteur en français ; noms de fonctions Rust
  du moteur en français également (`expr_type`, `deplier_i_rec`…).
- Un programme Rose exemple se termine par ses sorties vérifiées : mets à
  jour `tour_sortie.txt` quand `tour.rose` change.
- Commit en français, préfixe conventional (`feat:`, `fix:`, `perf:`,
  `docs:`, `bench:`, `build:`).
