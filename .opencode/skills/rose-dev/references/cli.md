# 🖥️ Rose en ligne de commande — référence

## Commandes

```
rose prog.rose [args…]          interpréter / JIT un programme Rose
rose run prog.rose              identique
rose comp prog.rose [-o sortie] exécutable autonome (moteur + source collé)
rose latex prog.rose            formule LaTeX du programme entier
rose llvm prog.rose [-o sortie] AOT LLVM classique (transpilation C + clang/gcc)
rose orc prog.rose [-o sortie] [-O1|-O2|-O3] [--pgo-generate|--pgo-use] [--link]
rose link objet.o [-o sortie]   lier un objet ELF en binaire exécutable
rose info                      état du moteur (version, architecture, moteur)
rose console [fichier.rose]    console interactive (REPL)
rose repl [fichier.rose]       identique
rose update                    installe ou met à jour Rose (64 bits)
rose --version                 « Rose <version> — compilateur Rust + JIT Cranelift »
rose --help                    aide complète
```

## La console interactive (REPL)

Elle est partagée par `rose console` et par `rose-shell.exe` — une seule
implémentation (`rose/src/repl.rs`).

Commandes (préfixées de `:`) :

| Commande | Effet |
|---|---|
| `:help` | liste des commandes |
| `:show` | affiche les variables définies |
| `:reset` | vide la mémoire |
| `:load fichier.rose` | charge un programme |
| `:save fichier.rose` | écrit le contenu courant |
| `:type expr` | type d'une expression |
| `:latex` | le programme courant en LaTeX |
| `:quit` (`:q`) | sort |

Édition de ligne réelle (console brute `ReadConsoleInputW`, sans dépendance) :

| Touche | Effet |
|---|---|
| ← / → | déplace le curseur |
| Home / End | début / fin de ligne |
| Retour arrière / Suppr | édite au milieu de la ligne |
| ↑ / ↓ | historique (le brouillon en cours est conservé) |
| Tab | complète le mot sous le curseur |
| Ctrl-C | annule la saisie (`^C`) |
| Ctrl-L | efface l'écran |
| Ctrl-Z | termine |
| Échap | efface la ligne |

Variables d'environnement : `ROSE_NO_COLOR=1` (pas de couleur),
`ROSE_COLOR=1` (forcer les couleurs).

## AOT LLVM « à la demande »

La chaîne AOT (`rose orc`, `rose link`) exige LLVM. Le moteur la cherche
seule :

1. le helper `rose-aot` est cherché à côté de l'exécutable, dans
   `~/.cache/rose/`, puis dans le `PATH` ;
2. s'il est absent, il est **téléchargé** depuis la release du projet,
   vérifié par **SHA-256**, installé dans le cache, puis exécuté.

Variables : `ROSE_AOT_URL` et `ROSE_AOT_SHA256` (miroir interne).

## 📦 Installation

### Windows — installeur Velopack (remplace WiX et NSIS)

`Rose-win-Setup.exe` est l'installeur **Velopack** (assistant Tauri + paquets
`.nupkg`). Il remplace définitivement les installeurs WiX et NSIS.

- Installation **versionnée** dans `%LOCALAPPDATA%\Rose\app-<version>\` :
  le launcher `Update.exe` applique les mises à jour par **deltas binaires** et
  **revient à la version précédente** si la nouvelle ne démarre pas.
- **Aucun droit administrateur** : tout est écrit dans le profil utilisateur
  (`HKCU`, `%LOCALAPPDATA%`), compatible avec les stratégies d'entreprise qui
  verrouillent `HKLM`.
- Le moteur `rose.exe` et le shell `rose-shell.exe` sont **embarqués** dans le
  paquet ; l'installeur pose le PATH et les raccourcis, la désinstallation est
  gérée par Velopack. Le helper AOT LLVM n'est **pas** embarqué : `rose.exe`
  le télécharge au premier `rose orc` / `rose link`.
- **Trois exécutables Windows**, liés **statiquement** (aucune DLL
  `VCRUNTIME140` ni `api-ms-win-crt-*` requise, Windows 10/11 les lance
  directement) :
  - `rose.exe` — le moteur (console, PE32+ x86-64) ;
  - `rose-shell.exe` — le lanceur de la console interactive (délègue à `rose.exe`) ;
  - `Rose-win-Setup.exe` — l'installeur **GUI** (fenêtre Win32 dessinée à la
    main, `rose/src/bin/rose-setup.rs` + `rose/src/setup_gui.rs`) :
    - **fenêtre entièrement personnalisée** : `WS_POPUP` + `WS_THICKFRAME`,
      sans aucun cadre Windows 11 — barre de titre, boutons réduire / agrandir
      / fermer et bords de redimensionnement dessinés au GDI ;
    - gabarit de 1120 × 780 **mis à l'échelle de la fenêtre** (aucun texte
      rogné, aucun bouton coupé) ; en plein écran le ratio n'est plus imposé,
      sinon la barre d'état sortait sous le bord inférieur de l'écran ;
    - **manifeste `PerMonitorV2`** embarqué (`RT_MANIFEST` dans le `.rsrc`) :
      sans lui Windows étire la fenêtre en bitmap sur les écrans à 125 / 150 %,
      d'où le rendu pixélisé ; double-bufferisation + `ANTIALIASED_QUALITY` ;
    - **branding Kare** : marque « Rose », titres de section et libellés de
      boutons en Kare majuscule (police d'affichage), texte courant en Segoe UI ;
    - la rose et la police Kare embarquées dans une section `.rsrc`
      (PNG + TrueType + icônes 16 → 256 px + manifeste,
      `installateur/velopack/pack-res.py`) ; le groupe d'icônes `RT_GROUP_ICON`
      est posé sur la fenêtre, la classe et la barre des tâches ;
    - couleurs du site, et recherche automatique du moteur à poser
      (`payload\rose.exe`, `rose.exe`, ou l'installeur lui-même s'il a été
      renommé).
    Rendu de contrôle sous Linux : `installateur/velopack/previsualiser.py`
    (aperçu PNG des cinq vues, textes réels, couleurs GDI exactes, contrôle
    mesuré des boutons et des cartes) ; ressources validées par
    `installateur/velopack/verifier-res.py` (arbre `.rsrc`, PNG, police,
    icônes, groupe d'icônes et manifeste).
- **Builds produits depuis cette machine Linux** (`clang` + `lld-link` +
  SDK/UCRT de `cargo-xwin`), pas seulement sur Windows :
  `sh installateur/velopack/build-linux.sh` puis
  `sh installateur/velopack/pack-linux.sh` (paquet Velopack + manifeste).
- Chaîne de build Windows complète : `installateur/velopack/build.ps1` —
  `cargo tauri build` puis `vpk pack` (voir `installateur/velopack/README.md`).
- La voie de mise à jour est vérifiable **sans Windows** :
  `sh installateur/velopack/verifier.sh` (6/6).

### Windows / Linux — depuis le moteur

```bash
rose update              # menu : mettre à jour / réinstaller / réparer / désinstaller
rose update --force      # retélécharge même si la version est déjà à jour
rose update --velopack   # Windows : passe par Velopack (delta + rollback)
```

**« Mettre à jour »** interroge l'API GitHub (`releases/latest`), compare
la version publiée à celle installée, **télécharge l'artefact** de la
plateforme, **vérifie son SHA-256** contre `SHA256SUMS`, puis remplace le
binaire installé.

**Velopack (Windows, la seule voie)** : pour les installations faites par
l'installeur Velopack (variable `VELOCABI` posée par le launcher), `rose update`
bascule automatiquement sur Velopack — il lit `releases.win.json`, compare la
version installée (`current` → `app-<version>`), télécharge le paquet complet,
**vérifie son SHA-256** et le dépose dans `packages\` ; le launcher applique la
mise à jour au démarrage suivant avec **rollback automatique** si la nouvelle
version casse. Les arguments internes `--veloapp-updated` / `--veloapp-obsolete`
sont routés en tête de `main()`. La chaîne de build est
`installateur/velopack/build.ps1` (`cargo tauri build` → `vpk pack` →
`vpk local` pour tester, `vpk push` pour publier).
Velopack n'ayant pas d'installeur natif Linux, `rose update` simple reste
sous Linux sur la voie artefacts GitHub (`docs/telechargements/` + SHA-256).

Même logique en console ; sur Linux : `~/.local/bin/rose` (ou `/usr/local`),
PATH dans `~/.bashrc` / `~/.profile`, type MIME `.rose` enregistré.

---
