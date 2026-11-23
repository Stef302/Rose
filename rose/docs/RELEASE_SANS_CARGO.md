# RELEASE SANS CARGO — chaîne de release auto-hébergée C0

**Constat daté** : 2026-11-23T22:38Z (23:38 heure locale CET). Worktree exclusif
`/tmp/opencode/rel_rel1`, HEAD `cdd57af3eca9db944e79fb092b6c2f6d7fe37802`.
Aucun commit, aucun push. Aucun fichier `rose/src/**`, `compilateur/**` ni
`Cargo.toml` modifié. Tout chiffre ci-dessous est une sortie réellement
exécutée ; re-mesurer après tout changement de binaire C0, de source composé
ou de release.

## 1. Ce que la release Rust utilise réellement

`scripts/build-windows-rebuild.sh` (3 appels `cargo build --release
--target x86_64-pc-windows-msvc`, lignes 61/64/70 avant modification)
construit quatre exe. La chaîne de publication utilise le moteur Rust pour :

| Bloc | Où (source lue) | Rôle dans la release | Dépendance Rust |
|---|---|---|---|
| CLI `rose` | `rose/src/main.rs` (~2 200 l.) | `methode`, `run`, `jit`, `interprete`, `comp`, `aot`/`orc`/`link`/`llvm`, `transpile`, `latex`, `bc`, `rvm`, `wasm`, `console`/`repl`, `version`, `info`, `help`, `verif`, `rir`, `assoc`, `update`/`install`, `debug-lenteur` | std + modules Rust |
| JIT chaud | `rose/src/jit.rs` (11 089 l.) | accélération des boucles pendant l'exécution | **Cranelift 0.137** (feature `native`) |
| AOT natif | `aot_llvm_orc.rs` (1 135 l.), `cmd_llvm.rs` (1 199 l.), `link_natif.rs` (1 194 l.), `elf_linker.rs`, `macho_writer.rs` | `rose aot`/`orc` : binaire natif compilé à l'avance, `rose link` | LLVM ORC / Inkwell + writers ELF/PE/Mach-O |
| Bytecode + VM | `rosevm.rs` (3 982 l.) | `rose bc` (snapshot ROSEBC v3) puis `rose rvm` (exécution en sandbox) | std |
| WASM | `wasm.rs`, `wasm_runtime.rs`, `wasm_lecteur.rs` (2 325 l.) | `rose wasm <lib.wasm>` (fonctions exportées), `transpile --cible wasm` | lecteur/runtime Rust |
| Console | `repl.rs` (1 411 l.) | `rose console` : `:help`, `:methode`, `:more [N]`, `:save` | std |
| Inférence graphique | `gui_native.rs` (1 074 l.), `setup_gui.rs` (4 648 l.), `setup_core.rs` (2 740 l.) | fenêtre Win32 de `rose-setup.exe` | Win32 Rust |
| Launcher / tray | `src/bin/rose-shell.rs` (308 l.), `rose-tray.rs` (670 l.) | "shell" (~300 Ko) qui délègue à `rose.exe` ; helper icône + timer 6 h | Win32 Rust |
| Ressources installeur | `installateur/velopack/pack-res.py`, `embed-assets.py`, `verifier-res.py` | RCDATA/ICON/FONT/MANIFEST embarqués dans `rose-setup.exe` | `llvm-rc` + `lld-link` |

Le binaire Rust embarqué dans la release annonce `0.1.73` (`Cargo.toml` du
worktree) ; la chaîne vise v0.1.76. Le moteur auto-hébergé annonce la même
constante `0.1.73` : ce n'est pas une divergence de comportement, mais un
champ de version à régénérer avec la release (voir §5).

## 2. Ce que le moteur C0 couvre — mesuré

Chaîne exécutée : `C0 compilateur/moteur_complet.rose > moteur.elf`, avec
`C0 = /tmp/opencode/C0_h.elf`, md5 `2db168b907b4a874712857d3957fd078`, sous
`bash scripts/garde_fou_mem.sh 12000000 …` (pic RSS < 12 000 000 Ko).

- source composé : `compilateur/moteur_complet.rose`, 469 824 o,
  md5 `65d12b384070ec34ea45f0a4d8d07712` ;
- artefact : `moteur.elf`, **1 902 136 o**, magic `7f454c4602010100`,
  machine aarch64 (183), md5 `9344caf808074eb283b2ec86dafb78af`, stderr 0 o ;
  reproduction bit-identique de `/tmp/opencode/moteur_h.elf` ;
- 12 programmes `tests_selfhost/*.rose` exécutés : **12/12 rc=0**, et
  **12/12 sorties octet-identiques** au binaire Rust
  `/home/clausake/Stefan/ROSE/rose/target/release/rose` lancé avec
  `ROSE_SANS_JIT=1` (`t01_entiers`, `t02_bool`, `t03_chaine_ascii`,
  `t04_chaine_unicode`, `t05_chaine_sous`, `t06_rationnel_lit`,
  `t09_flottant`, `t20_add`, `g5b604`, `g5b608`, `g5b609`, `g5b610`).
- sous-commandes effectivement implémentées dans le point d'entrée du composé
  (vérifiées par exécution, pas par lecture de la doc) : lecture + vérification
  + RIR + exécution, `run`/`jit`/`interprete`/`methode`, `verif`, `rir`,
  `tokens`, `ast`, `latex`, `transpile --cible wasm`, `bc`, `comp`, `version`,
  `help`, `info`.
- le corpus plus large reste à **133/141 conformes, 0 erreur Rose sur
  programme valide** (`rose/docs/G5_UNICODE.md`, bilan vague 97, commit
  `cdd57af`) — chiffre documenté, pas re-mesuré intégralement ici.

Le lanceur `rose` produit par `build-sans-cargo.sh` transmet ces commandes à
l'ELF et **refuse explicitement** (rc=3, message sur stderr) les commandes non
implémentées, au lieu de les simuler.

## 3. Ce qui manque encore, cause technique, geste

| Commande / artefact | Cause technique précise | Geste pour 100 % |
|---|---|---|
| `jit` accélération réelle | Le composé exécute le RIR interprété ; `compile_rir`/`call_fn` n'émettent aucun code machine chaud. `jit` n'est qu'un alias d'exécution. | Brancher l'émetteur machine G5 (`compilateur/emetteur_x64.rose` / noyau de `emetteur_elf.rose`) sur le chemin RIR, avec trampoline d'appel et garde bigint, puis mesurer A/A/A face à Cranelift. |
| `aot`, `orc`, `link`, `llvm` | Aucun writer LLVM ORC, aucun lieur d'objets, aucun émetteur C dans `moteur_complet.rose` ; `rose aot` y est lu comme un nom de fichier. | Composer `emetteur_rust.rose`/`emetteur_cpp.rose` + writer ELF/PE/Mach-O dans le composé, au lieu de `cmd_llvm.rs`. |
| `console` / `repl`, `rvm` | `repl_rose.rose` est composé (56 `fn rep_*`) mais son point d'entrée a été retiré lors de l'assemblage (`sans_main` dans `build_moteur_sans_rust.sh`) ; `rosevm.rs` n'a aucun équivalent Rose. | Réintroduire un point d'entrée REPL dans le pilote du composé ; écrire le chargeur ROSEBC v3 et la boucle de trames en Rose. |
| `transpile js/py/c` | Seul `sw_module()` (cible wasm) est composé ; les émetteurs texte ne sont pas appelés. | Composer `emetteur_rust.rose`/`emetteur_cpp.rose` et ajouter les trois branches `--cible`. |
| `comp` non équivalent | `mc_elf_gabarit()` ne patche qu'**une constante** extraite du RIR : ELF de **8 908 o**, ou une seule ligne (`0`) après `chmod +x`, contre **5 457 034 o** et 4 lignes pour `rose comp` Rust sur `t20_add.rose`. | Faire embarquer le source/RIR complet au lieu de la constante, ou appeler l'émetteur ELF G5 sur le RIR ; benchmarker la sortie octet pour octet. |
| `transpile --cible wasm` non identique | `sw_module()` écrit des LEB128 surlongs (`c3 b9 00`, `c2 80 …`) : **82 o** contre **72 o** côté Rust ; la longueur du corps est `0x23` contre `0x22`. | Corriger l'encodeur LEB128 du composé pour n'émettre que la forme minimale, puis comparer md5 des modules. |
| `bc` non identique | 157 o des deux côtés mais les champs `0x0c` (12) et les u32 de ligne/index présents côté Rust sont remplacés par `0x01` et absents côté C0 (`sb_module()`). | Aligners `sb_module()` sur le snapshot v3 : écrire les champs de ligne/index, puis `cmp -s` des deux `.rosebc`. |
| `Rose-win-Setup.exe` | La fenêtre Win32 (`setup_gui.rs`) et la table de ressources (`pack-res.py` → `llvm-rc` → `lld-link`) n'ont aucun équivalent en Rose ; C0 n'a ni writer PE ni lieur. | Écrire un writer PE32+ x86_64 + table de ressources en Rose (le noyau aarch64 existe dans `compilateur/emetteur_pe.rose`, mais cible `IMAGE_FILE_MACHINE_ARM64`), puis lier dans `build-sans-cargo.sh`. |
| `rose.exe` (x86_64-pc-windows-msvc) | C0 émet **uniquement** de l'ELF64 aarch64 (machine=183) ; l'émetteur PE Rose existant est ARM64, pas x86_64, et exige une graine Rose. | Porter `emetteur_pe.rose` au machine type x86_64 + IAT MSVCRT, puis l'exécuter depuis C0. |
| `rose-shell.exe`, `rose-tray.exe`, `payload/rose.exe` | Dépendent de `rose.exe` PE (shell) ou sont du code Win32 Rust pur (tray : icône, timer 6 h, lecture manifeste). | Réécrire ces trois lanceurs en Rose ciblant PE x86_64, puis arrêter de copier les `.exe` de `target/`. |
| Champ de version | `cli_banniere()`/`cli_info()` du composé portent `0.1.73` en dur. | Régénérer le pilote avec la version de la release au moment du tag. |

## 4. Reproduction (preuves d'exécution)

```sh
cd /tmp/opencode/rel_rel1
# 1) release autonome seule
bash scripts/build-sans-cargo.sh --c0 /tmp/opencode/C0_h.elf \
  --sortie /tmp/opencode/release-sans-cargo \
  --reference /home/clausake/Stefan/ROSE/rose/target/release/rose
# → ELF 1 902 136 o md5 9344caf… ; 12/12 rc=0 ; 12 conformes / 0 divergents

# 2) chemin de release Windows en mode --selfhost (cargo non appelé)
ROSE_REFERENCE=/home/clausake/Stefan/ROSE/rose/target/release/rose \
  sh scripts/build-windows-rebuild.sh --selfhost
# → stage : moteur.elf, rose, MANIFESTE.txt, resultats.tsv,
#   NON_COUVERT_SELFHOST.txt ; `rose aot …` → rc=3 (refus explicite)

# 3) chemin par défaut, INCHANGÉ (release v0.1.76)
sh scripts/build-windows-rebuild.sh
```

Sorties réelles du mode `--selfhost` (23/11 22:36Z UTC, 23:36 CET) :

```
=== VÉRIFICATION ARTEFACTS SELFHOST ===
ELF 64-bit LSB executable, ARM aarch64
  moteur.elf     1902136 o  9344caf808074eb283b2ec86dafb78af
  rose               832 o  9889900f43eea0499413002723fb1478
  commande non couverte aot : rc=3 (3 attendu)
=== ARTEFACTS WINDOWS ABSENTS (documentés) ===
  Rose-win-Setup.exe rose.exe rose-shell.exe rose-tray.exe payload/rose.exe
```

Le script `build-windows-rebuild.sh` garde `cargo build` **par défaut** :
`--selfhost` est une option explicite, donc la release v0.1.76 n'est pas
cassée. Aucun appel à `cargo`, `rustc`, `clang`, `lld-link`, `llvm-rc` ou
`pack-res.py` n'est fait dans la branche `--selfhost`.

## 5. Jalon exact pour 100 %

Le self-hosting de la **chaîne de release** est atteint quand, avec
`sh scripts/build-windows-rebuild.sh --selfhost` **sans** `--cargo`, le stage
contient les cinq artefacts (`Rose-win-Setup.exe`, `rose.exe`,
`rose-shell.exe`, `rose-tray.exe`, `payload/rose.exe`) avec magic PE
`MZ`/`PE\0\0`, machine `0x8664`, tailles et md5 journalisés, et que
`verifier-res.py` passe sur le setup. Ordre des gestes :

1. porter `compilateur/emetteur_pe.rose` de `IMAGE_FILE_MACHINE_ARM64` à
   `0x8664` (IAT MSVCRT/x86_64 au lieu de KERNEL32/Win64) ;
2. composer cet émetteur dans le pilote C0 aux côtés de `moteur_complet.rose`,
   puis produire `rose.exe` depuis le RIR du composé (pas de `cargo`) ;
3. réécrire `rose-shell.rs`/`rose-tray.rs` en Rose ciblant le même PE et
   remplacer les trois `cargo build` par ces trois émissions ;
4. ajouter le writer de ressources Win32 (RCDATA/ICON/FONT/MANIFEST) en Rose
   pour que `verifier-res.py` valide le setup sans `llvm-rc` ;
5. basculer le défaut de `build-windows-rebuild.sh` sur ce chemin, garder
   l'ancien sous `--cargo`, re-mesurer les 12 programmes + les cinq artefacts,
   puis aligner la constante de version sur le tag de release.

Les écarts fonctionnels `jit`/`aot`/REPL/RVM/comp/wasm/bc de la section 3
sont des jalons parallèles : ils ne bloquent pas la production des cinq
artefacts, mais doivent être fermés avant de pouvoir retirer le moteur Rust
de la distribution (étape 5).
