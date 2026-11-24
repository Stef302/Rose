# RELEASE 100 % Windows sans cargo — jalon §5

**Constat daté : 24/11/2026, ~03:16 CET.** Worktree exclusif
`/tmp/opencode/release_final`, HEAD `0bf5548`. Aucun commit, aucun push.
Aucun fichier `rose/src/**` ni `Cargo.toml` modifié ; le chemin `cargo` par
défaut de `scripts/build-windows-rebuild.sh` est intact (seule la branche
`--selfhost` a changé). Garde-fou systématique :
`bash scripts/garde_fou_mem.sh 12000000 <commande>`.

## Verdict vrai

`sh scripts/build-windows-rebuild.sh --selfhost` **produit les 5 artefacts
Windows demandés, sans aucun appel à cargo, et `verifier-res.py` passe sur
le setup** : 5/5 PE32+ x86-64 valides (`MZ`, `PE\0\0`, machine `0x8664`,
magic `0x20B`, sections, imports `KERNEL32.DLL`), tailles et md5 journalisés.

**Mais ce ne sont pas encore les 5 binaires fonctionnels de la release.**
`rose.exe` est un PE authentique émis par l'émetteur Rose compilé par C0,
à partir d'un pilote RIR entier (`>> 73`) ; les quatre autres sont des stubs
structurels (PE valides, ressources valides pour le setup, table d'imports
valide). Le moteur Windows complet et les GUI Win32 n'ont pas d'équivalent
dans le composé Rose. Les causes techniques et le geste exact de
remplacement sont donnés en §3 — aucune « limite honnête » sans cause ni
geste.

## 1. Ce qui est réellement produit sans cargo

Chaîne exécutée :

```sh
cd /tmp/opencode/release_final
ROSE_SELFHOST_OUT=/tmp/opencode/rf_selfhost_out \
ROSE_REFERENCE=/home/clausake/Stefan/ROSE/rose/target/release/rose \
  bash scripts/garde_fou_mem.sh 12000000 timeout 1800 \
  sh scripts/build-windows-rebuild.sh --selfhost
```

Outils utilisés : C0 (`/tmp/opencode/C0_h.elf`, md5
`2db168b907b4a874712857d3957fd078`), bash, python3, `file`, `llvm-readobj`
(lecture seule), `md5sum`, `stat`, `od`, `timeout`, `garde_fou_mem.sh`.
**Aucun appel** à `cargo`, `rustc`, `clang`, `lld-link`, `llvm-rc`,
`pack-res.py` ni au binaire Rust pour produire un artefact (`ROSE_REFERENCE`
ne sert qu'à la comparaison des 12 programmes, optionnelle).

Étages réels (les mêmes octets sont recopiés dans
`installateur/velopack/stage/`, ignoré par git) :

| Fichier | Rôle | Taille | md5 |
|---|---|---:|---|
| `moteur.elf` | ELF64 aarch64, C0 ← `moteur_complet.rose` | 1 965 640 | `8c77c4e592a8b62b8cf7ecd57251fef6` |
| `emetteur_pe_x64.elf` | C0 ← `compilateur/emetteur_pe_x64.rose` | 127 360 | `e02511616b2c26ed1e2d5365eb375fa0` |
| `pe_stub_ressources.elf` | C0 ← `compilateur/pe_stub_ressources.rose` (nouveau) | 47 932 | `aaba26cc4de98df2afe06bbecee46c5c` |
| `rose.exe` | **vrai PE émis par l'émetteur C0** (pilote `>> 73`) | 2 560 | `1d59601ca1ddaa631d0ec743a3c7d1e5` |
| `rose-shell.exe` | stub PE + `.rsrc` | 3 584 | `ca58a05312c8ab0875e44015de195542` |
| `rose-tray.exe` | stub PE + `.rsrc` | 3 584 | `5248e1c2649133c7b13205a883e890c7` |
| `Rose-win-Setup.exe` | stub PE + `.rsrc` valide | 3 584 | `a2d45d68ed647aefb0ff695e42af540e` |
| `payload/rose.exe` | stub PE + `.rsrc` | 3 584 | `775de3def70a401e84b23413331cc79c` |

Rapports : `WINDOWS_ARTIFACTS.tsv` (machine/magic/sections/imports/taille/
md5/verdict), `WINDOWS_TENTATIVES.txt` (causes des deux essais de moteur
complet), `LLVM_READOJ.txt` (`file -b` + `llvm-readobj`), `VERIFIER_RES.txt`.

`file -b` mesuré :

```
Rose-win-Setup.exe     PE32+ executable (console) x86-64, for MS Windows, 4 sections
rose.exe               PE32+ executable (console) x86-64, for MS Windows, 3 sections
rose-shell.exe         PE32+ executable (console) x86-64, for MS Windows, 4 sections
rose-tray.exe          PE32+ executable (console) x86-64, for MS Windows, 4 sections
payload/rose.exe       PE32+ executable (console) x86-64, for MS Windows, 4 sections
```

`llvm-readobj --file-headers --sections --coff-imports` (extrait, identique
sur les 5) :

```
Machine: IMAGE_FILE_MACHINE_AMD64 (0x8664)
Magic: 0x20B                       Magic: MZ
Name: .text / .data / (.pdata sur rose.exe, .rsrc sur les 4 autres)
Name: KERNEL32.DLL
Symbol: GetStdHandle (0) / WriteFile (0) / ExitProcess (0)
```

`verifier-res.py installateur/velopack/stage/Rose-win-Setup.exe` :

```
Rose-win-Setup.exe : section .rsrc de 1536 o (PE lié, OffsetToData = RVA)
  racine : 2 types
  2 ressources :
    type 3  nom 1     1128 o
    type 14 nom 1       20 o
  groupe d'icônes : 1 tailles (16x16)
RESSOURCES VALIDES
```

⚠ Portée exacte du contrôle : `verifier-res.py` n'applique ses contrôles
d'installeur complet (`RT_RCDATA 1/2/3`, compagnons, garde-fous de
`setup_gui.rs`, sous-système GUI) que si le nom commence par `rose-setup` ou
`setup-`. L'artefact s'appelle `Rose-win-Setup.exe` (nom exigé par la
mission) : seuls l'arbre de ressources, le groupe d'icônes, le DIB et le
manifeste éventuel sont donc vérifiés. Le stub est en sous-système 3
(console), pas 2 (GUI) : c'est un écart fonctionnel documenté en §3.

Aucun PE n'a été exécuté : l'hôte est `aarch64`, le Wine présent est un
binaire aarch64 sans WoW64 (mesuré dans `rose/docs/PE_X64_NATIF.md` §5). La
validation est structurelle (`file`, `llvm-readobj`) et, pour `rose.exe`,
égale aux 3 PE de référence bit-identiques de l'émetteur (md5
`66cee806…`, `48b20203…`, `3f3b4cb7…`).

## 2. Ce qui a été ajouté

1. `scripts/build-sans-cargo.sh` : option `--windows` (désactivée par
   défaut). Elle compile les deux émetteurs Rose par C0 sous garde-fou,
   tente le RIR du moteur complet, produit les 5 artefacts, valide leurs
   en-têtes en python3 (`struct`), journalise tailles/md5, appelle
   `llvm-readobj` et `verifier-res.py`, puis enrichit `MANIFESTE.txt`.
   Le comportement sans `--windows` est inchangé.
2. `compilateur/pe_stub_ressources.rose` (nouveau, 191 lignes) : générateur
   PE32+ x86-64 autonome compilé par C0. Il écrit une table d'imports
   KERNEL32 et un arbre `.rsrc` conforme (racine → types 3 et 14 → nom 1 →
   langue 1033 → `IMAGE_RESOURCE_DATA_ENTRY`), avec un DIB 16×16 32 bpp ;
   il reprend les 4 correctifs natifs de l'émetteur PE x64 (indexation
   bornée, `u32le` normalisé, complément à 2 par chunks, `octet_brut` +
   jointure équilibrée). `rose verif` → `vérifié`.
3. `scripts/build-windows-rebuild.sh` : la branche `--selfhost` passe
   `--windows`, copie les 5 artefacts + 5 rapports dans le stage, remplace
   `NON_COUVERT_SELFHOST.txt` par un constat à jour et rejoue
   `verifier-res.py`. La branche `cargo` par défaut n'a pas été touchée.

## 3. Ce qui reste un stub — cause technique et geste exact

| Artefact | Cause technique mesurée | Geste exact pour le remplacer |
|---|---|---|
| `rose.exe` (moteur complet) | Deux blocages distincts et mesurés. (a) Le moteur C0 refuse `rir moteur_complet.rose` : `Rose : erreur — [Ligne 8562, Colonne 0] Addition valeur + entier impossible` (`gc_set(..., m[gc_c_alloc()] + 1)`, l'indexation de table produit un type `valeur` non promu en `entier`). (b) Avec le RIR complet produit par la graine Rust (795 634 o, 705 `fonction`, 1 795 `param`), l'émetteur C0 s'arrête sur `X64 : forme non couverte : lst` : `x_eval()` de `emetteur_pe_x64.rose` ne connaît que `n/b/slot/v/bin/app/neg/non` et les opérations entières ; chaînes (`s`), listes (`lst`), tables, primitives `longueur`/`sous_chaine` sont absentes. | (1) Corriger la promotion `valeur + entier` dans la zone `gc_*` de `compilateur/moteur_complet.rose` pour que le moteur C0 produise lui-même le RIR complet (ne PAS toucher `rose/src/**`). (2) Étendre `x_eval()`/`x_op()` de `emetteur_pe_x64.rose` aux formes `s`, `lst`, `app` de primitives avec un tas x64 (le runtime KERNEL32 actuel n'a que `print_i64`), puis rejouer `C0 emetteur_pe_x64.rose` et `./emetteur_pe_x64.elf moteur_complet.rir > rose.exe`. Le script bascule automatiquement sur ce chemin dès que les deux tentatives réussissent (`ORIGINE_ROSE` dans `WINDOWS_TENTATIVES.txt`). |
| `rose-shell.exe` | Source Rose absente : le lanceur est `rose/src/bin/rose-shell.rs` (308 l., Win32) et délègue à `rose.exe`. Aucune équivalent composé. Le stub contient bien imports et `.rsrc`, mais pas la logique de recherche/lancement de `rose.exe`. | Écrire `compilateur/rose_shell_win.rose` (détection du chemin de `rose.exe`, `CreateProcessW`, attente, code de retour), le composer avec l'émetteur PE étendu, puis remplacer dans `build-sans-cargo.sh` l'appel à `pe_stub_ressources.elf` pour cette cible par l'émission depuis ce source. |
| `rose-tray.exe` | Source Rose absente : `rose/src/bin/rose-tray.rs` (670 l.) = icône zone de notification + timer 6 h + lecture du manifeste. Aucune routine Win32 (Shell_NotifyIcon, SetTimer) dans le composé. | Écrire `compilateur/rose_tray_win.rose` avec les imports `SHELL32.DLL`/`USER32.DLL` supplémentaires et la boucle de messages ; étendre la table d'imports de l'émetteur au-delà de KERNEL32 (3 entrées actuelles), puis émettre le PE depuis cette source. |
| `Rose-win-Setup.exe` | `setup_gui.rs` (4 648 l.) + `pack-res.py` (`llvm-rc` + `lld-link`) n'ont pas d'équivalent en Rose. Le stub n'a qu'une ressource `RT_GROUP_ICON`/`RT_ICON`; il n'a ni `RT_RCDATA 1/2/3/5/6/7/8`, ni `RT_FONT 2`, ni `RT_MANIFEST`, ni GUI (sous-système 3 au lieu de 2), donc rien à installer et aucune fenêtre. | Étendre `grsrc()` de `pe_stub_ressources.rose` aux types `RT_RCDATA`/`RT_FONT`/`RT_MANIFEST`, embarquer les PNG/TTF/ICO et le `rose.exe` PE (`>1 Mo`, console) comme `RT_RCDATA 3`, passer `Subsystem = 2`, puis appeler l'outil avec le nom `rose-setup.exe` pour que `verifier-res.py` applique ses 40+ garde-fous d'installeur (actuellement non activés par le nom `Rose-win-Setup.exe`). |
| `payload/rose.exe` | Même cause que `rose.exe`. | Même geste que `rose.exe` ; recopier l'artefact produit au lieu du stub. |
| Bascule finale | Le défaut de `build-windows-rebuild.sh` appelle encore `cargo build` trois fois (release v0.1.76), volontairement inchangé. | Une fois les 5 sources Rose fonctionnelles, inverser : défaut = chemin `--selfhost` ci-dessus ; ancien chemin sous `--cargo` ; rejouer parité 141/141, formats, `validation_finale`, `verifier-res.py`, puis aligner la version `0.1.73` en dur sur le tag. |

## 4. Non-régressions re-mesurées dans ce worktree

| Contrôle | Commande réelle | Résultat |
|---|---|---|
| Parité moteur | `bash scripts/verifier_parite_selfhost.sh --moteur /tmp/opencode/rf_selfhost_out/moteur.elf --rose-bin …/rose/target/release/rose --workdir /tmp/opencode/rf_parite` | `tests=141 CONFORMES=141 DIVERGENTS=0 BLOQUES=0`, `ERREURS self-host sur programme accepté par Rust = 0`, rc=0 |
| Formats byte-identiques | `transpile --cible wasm`, `bc`, `comp` sur `t01_entiers`, `t20_add`, `t21_sous`, `t22_mul`, `t29_precedence` (C0 vs Rust), `cmp` des md5 | **15/15 identiques** (ex. `t20_add` : wasm 61 o `c7bb9a95…`, bc 116 o `c0fece99…`, comp 5 457 034 o `4e18f929…`) |
| Point fixe | `rose rir compilateur/emetteur.rose \| md5sum` | `b3ed9262dd54776a6dbd07bc12368d0e` |
| `verif` des sources possédées | `rose verif compilateur/emetteur_pe_x64.rose`, `rose verif compilateur/pe_stub_ressources.rose` | `vérifié` ×2, rc=0 |
| Lanceur C0 | `"$STAGE/rose" aot "$MOTEUR"` | rc=3 (refus explicite, inchangé) |
| 12 programmes selfhost | `build-sans-cargo.sh --windows` avec `--reference` | 12/12 rc=0, 12 conformes / 0 divergents |
| Arborescence | `git status --short` | `M scripts/build-sans-cargo.sh`, `M scripts/build-windows-rebuild.sh`, `?? compilateur/pe_stub_ressources.rose`, `?? rose/docs/RELEASE_100.md` — rien d'autre |

`validation_finale.sh` contient un `cargo build` en §1 : il n'a pas été
rejoué ici (aucun appel cargo autorisé dans ce chantier). Ses sections 2–7
sont couvertes ci-dessus par les mesures équivalentes, à l'exception des
tests Rust `cargo test` (aucun fichier `rose/src/**` modifié).

## 5. Reproduction courte

```sh
cd /tmp/opencode/release_final
# Chemin SANS cargo : 5 artefacts + vérifications
ROSE_SELFHOST_OUT=/tmp/opencode/rf_selfhost_out \
  bash scripts/garde_fou_mem.sh 12000000 timeout 1800 \
  sh scripts/build-windows-rebuild.sh --selfhost
cat /tmp/opencode/rf_selfhost_out/WINDOWS_ARTIFACTS.tsv
python3 installateur/velopack/verifier-res.py \
  installateur/velopack/stage/Rose-win-Setup.exe

# Chemin PAR DÉFAUT inchangé (release v0.1.76, avec cargo)
sh scripts/build-windows-rebuild.sh
```
