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
`references/syntaxe.md` (à la racine du dépôt : section
« 📖 Syntaxe Rose — référence complète » du `README.md`).
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

## Style

- Messages et commentaires du moteur en français ; noms de fonctions Rust
  du moteur en français également (`expr_type`, `deplier_i_rec`…).
- Un programme Rose exemple se termine par ses sorties vérifiées : mets à
  jour `tour_sortie.txt` quand `tour.rose` change.
- Commit en français, préfixe conventional (`feat:`, `fix:`, `perf:`,
  `docs:`, `bench:`, `build:`).
