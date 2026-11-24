# PAYLOAD_C0 — `payload/rose.exe` compilé par C0, fin du recours au nupkg 0.1.76

**Constat mesuré le 24/11/2026.** Ce document remplace, pour le payload, le
paragraphe « ce qui reste » de `BUILD_SELFHOST_FINAL.md` : dans le chemin
`--selfhost`, **plus aucun octet du binaire signé extrait de
`docs/telechargements/Rose-0.1.76-full.nupkg` (`lib/net45/rose.exe`) n'est copié
ni embarqué**. `rose.exe`, `payload/rose.exe` et la ressource `RT_RCDATA 3` du
setup sont le **même** PE32+ console produit par C0 depuis
`compilateur/moteur_complet.rose` (RIR C0 → PE C0).

## 1. Avant : d'où venait le payload

Chaîne `--selfhost` d'avant correctif (`scripts/build-sans-cargo.sh`, option
`--windows`) :

```python
# ancien code, ligne ~330
z = zipfile.ZipFile(root / "docs/telechargements/Rose-0.1.76-full.nupkg")
engine = z.read("lib/net45/rose.exe")          # 9 753 600 o, signé
(out / "rose-engine.exe").write_bytes(engine)
...
cp "$ASSETS/rose-engine.exe" "$WIN/rose.exe.engine"
cp "$ASSETS/rose-engine.exe" "$WIN/payload/rose.exe"   # ← le recours
add(10, 3, out / "rose-engine.exe")                    # ← et dans le setup
```

Mesures du stage livré (`/tmp/opencode/release-sans-cargo`, rapport
`BUILD_SELFHOST_FINAL.md` du 24/11/2026 06:51 UTC) :

| fichier | taille (o) | md5 | origine |
|---|---:|---|---|
| `rose.exe` | 1 413 632 | `9396d0bffa2caaef168e2f5a7bf918ac` | RIR C0 → PE C0 (moteur complet) |
| `payload/rose.exe` | 9 753 600 | `02528a069c185bcedaffd626aff3ecee` | `lib/net45/rose.exe` du nupkg 0.1.76, tel quel |
| `assets/rose-engine.exe` | 9 753 600 | `02528a069c185bcedaffd626aff3ecee` | idem |
| `RCDATA 3` du setup | 9 753 600 | `02528a069c185bcedaffd626aff3ecee` | idem |
| `Rose-win-Setup.exe` | 10 476 032 | `d160c6f80e16037a3d10665ae60bc447` | générateur Rose, 17 ressources |

Le vérificateur binaire du script n'exigeait pour le payload que
`sous-système == 3` (console) et `taille >= 1 000 000` : le binaire signé du
nupkg satisfaisait ce contrôle, d'où son maintien alors que `rose.exe` était
déjà produit par C0. `verifier-res.py` contrôle la même chose pour le
`RCDATA 3` (PE console > 1 Mio) — jamais une signature ni un md5 figé.

## 2. Correctif

`scripts/build-sans-cargo.sh` (seule la branche `--windows` ; le chemin cargo
par défaut de `build-windows-rebuild.sh` n'est pas touché) :

1. **Plus de repli.** La RIR du moteur complet puis l'émission PE par
   l'émetteur C0 sont obligatoires : un échec journalise `BLOQUÉ, aucun repli`
   et quitte en erreur. Le pilote entier `>> 73` (stub 2 560 o) et la copie du
   nupkg sont supprimés de cette chaîne.
2. **Validation du moteur C0 avant emploi** : `MZ`, `PE\0\0`, `machine
   0x8664`, `magic 0x20B`, `sous-système 3`, `taille >= 1 000 000`.
3. **Payload** : `cp "$WIN/rose.exe" "$WIN/payload/rose.exe"` (plus
   `rose.exe.engine`, trace identique).
4. **RCDATA 3** : `add(10, 3, win / "rose.exe")` — 17 ressources inchangées.
5. **Deux garde-fous d'identité** : la vérification des 5 artefacts exige
   `md5(payload) == md5(rose.exe)`, et une étape 6b extrait le `RCDATA 3` de
   `Rose-win-Setup.exe` (via les fonctions de `verifier-res.py`) pour exiger
   `md5(RCDATA 3) == md5(rose.exe) == md5(payload)`. Le rapport est écrit dans
   `PAYLOAD_ORIGINE.txt`.
6. L'extraction ZIP du nupkg et l'import `zipfile` ont été retirés du script :
   aucune lecture de `Rose-0.1.76-full.nupkg` ne subsiste dans le chemin
   `--windows`.

`compilateur/pe_stub_ressources.rose` n'a pas eu besoin de modification : il
lit déjà chaque ressource par chemin + taille STAT (`argument(5+4*i)`,
`argument(6+4*i)`) et n'a aucune taille codée en dur pour le `RCDATA 3`.

## 3. Preuve de l'origine C0

Chaîne complète, sans cargo (re-mesurée par le build final ci-dessous) :

- C0 : `/tmp/opencode/C0_h.elf`, md5 `2db168b907b4a874712857d3957fd078` ;
- source : `compilateur/moteur_complet.rose`, md5
  `e5bc586cfddc46c2d32338a52a51c2b4` ;
- `moteur.elf` : ELF64 aarch64, 1 965 712 o, md5
  `5e9256a664ac4dfd0c54bc5fec473489` ;
- émetteur : `compilateur/emetteur_pe_x64.rose` compilé par C0 en
  `emetteur_pe_x64.elf` (225 652 o, md5 `fcf9b8a6e4e444ab5bb08d7f617bca25`) ;
- RIR complet : 795 791 o, puis `rose_complet.exe` = `rose.exe` (même md5),
  PE32+ console x86-64, 3 sections `.text`, `.data`, `.pdata`, 12 imports
  `KERNEL32.DLL`.

Le md5 `9396d0bffa2caaef168e2f5a7bf918ac` du payload après correctif est donc
**le même que celui du `rose.exe` issu de l'émetteur C0** mesuré dans
`BUILD_SELFHOST_FINAL.md` — et **différent** de `02528a069c…` (nupkg). Le
`rose.exe` du build précédent reste disponible dans
`/tmp/opencode/release-sans-cargo/` comme étalon de comparaison.

## 4. Rejeu rapide des étapes post-C0 (mesuré)

En réutilisant les ELF déjà compilés par C0 du build précédent, le nouveau
`setup.tsv` pointe le `RCDATA 3` sur `rose.exe` (1 413 632 o) au lieu de
`assets/rose-engine.exe` (9 753 600 o) ; le générateur Rose produit alors un
setup de **2 136 064 o** (10 476 032 − 8 339 968).

```
$ python3 installateur/velopack/verifier-res.py /tmp/opencode/payload_smoke/rose-setup.exe
rose-setup.exe : section .rsrc de 2133504 o (PE lié, OffsetToData = RVA)
  racine : 5 types
  17 ressources :
    ...
    type 10 nom 3  1413632 o
    ...
  groupe d'icônes : 7 tailles (16x16, 24x24, 32x32, 48x48, 64x64, 128x128, 256x256)
  manifeste : 1474 o, ASCII, PerMonitorV2
  subsystem : 2 (GUI)
  moteur embarqué : 1413632 o (PE console)
  rose-shell.exe     124416 o
  rose.ico           112510 o
  rose_app.ico        64700 o
  rose_shell.ico     113565 o
  rose PNG      31391 o (184x233, RGBA)
  tracé : 33 garde-fous présents
RESSOURCES VALIDES

$ python3 <bloc 6b extrait de build-sans-cargo.sh> ...
rose.exe (C0)      :  1413632 o  md5=9396d0bffa2caaef168e2f5a7bf918ac
payload/rose.exe   :  1413632 o  md5=9396d0bffa2caaef168e2f5a7bf918ac
RCDATA 3 du setup  :  1413632 o  md5=9396d0bffa2caaef168e2f5a7bf918ac
PREUVE : payload/rose.exe = RCDATA 3 = rose.exe = moteur complet C0 (RIR -> PE)
```

## 5. Preuve bout en bout (build complet, 24/11/2026 08:00 → 08:37 UTC)

Commande exacte, sous garde-fou mémoire :

```sh
cd /tmp/opencode/az_payload
ROSE_SELFHOST_OUT=/tmp/opencode/payload_c0_out \
  bash scripts/garde_fou_mem.sh 12000000 timeout 3000 \
  sh scripts/build-windows-rebuild.sh --selfhost
# journal : /tmp/opencode/payload_c0_build.log
# shell : sh_20c5b5c39001m0tTWFx4rrRXWp
```

**rc = 0** (`/tmp/opencode/payload_c0_build.rc` = `rc=0`). Zéro ligne
`ÉCHEC`/`BLOQUÉ`/`Traceback`/`ERREUR` dans le journal. La seule occurrence de
`cargo` dans le journal est la bannière « AUCUNE commande cargo » ; aucune
lecture du nupkg 0.1.76, aucun `rose-engine.exe` dans `assets/`.

Étapes mesurées : C0 `2db168b9…` + `moteur_complet.rose` `e5bc586c…` →
`moteur.elf` 1 965 712 o `5e9256a6…` ; 12/12 tests rc=0 ; émetteur PE
225 652 o `fcf9b8a6…` ; RIR complet 795 791 o ; `rose.exe` émis en PE.
`WINDOWS_TENTATIVES.txt` contient ` rose.exe = moteur complet émis en PE` et
**aucune** ligne `repli sur stub` ni `>> 73`.

### Les 5 artefacts (`WINDOWS_ARTIFACTS.tsv`)

| artefact | taille (o) | md5 | machine | magic | sections | verdict |
|---|---:|---|---|---|---|---|
| `Rose-win-Setup.exe` | 2 136 064 | `474fb456853e2f496a30230d43f5e8c4` | 0x8664 | 0x20B | `.text,.data,.rsrc` | OK |
| `rose.exe` | 1 413 632 | `9396d0bffa2caaef168e2f5a7bf918ac` | 0x8664 | 0x20B | `.text,.data,.pdata` | OK |
| `rose-shell.exe` | 124 416 | `c955c6608b0913f7fa541d74cd9b457c` | 0x8664 | 0x20B | `.text,.data,.rsrc` | OK |
| `rose-tray.exe` | 125 952 | `1cbc358a85eb4a8b55c0b12e251782ce` | 0x8664 | 0x20B | `.text,.data,.rsrc` | OK |
| `payload/rose.exe` | 1 413 632 | `9396d0bffa2caaef168e2f5a7bf918ac` | 0x8664 | 0x20B | `moteur-console-reel` | OK |

`5/5 ARTEFACTS PE32+ x86-64 VALIDES`. `file -b payload/rose.exe` =
`PE32+ executable (console) x86-64, for MS Windows, 3 sections` ;
`cmp rose.exe payload/rose.exe` → rc 0. Le stage
`installateur/velopack/stage/payload/rose.exe` a le même md5.

### Ressources (`VERIFIER_RES.txt`)

`python3 installateur/velopack/verifier-res.py Rose-win-Setup.exe` →
**RESSOURCES VALIDES** : 5 types, **17 ressources**, `RT_RCDATA 3` =
**1 413 632 o**, `moteur embarqué : 1413632 o (PE console)`, 7 tailles
d'icônes (16→256 px), manifeste PerMonitorV2 (1 474 o), **33 garde-fous
présents**, `rose-shell.exe`/`rose.ico`/`rose_app.ico`/`rose_shell.ico` et
PNG RGBA 184×233. Setup en sous-système 2 (GUI), moteur embarqué en
sous-système 3 (console).

### Identité (`PAYLOAD_ORIGINE.txt`)

```
rose.exe (C0)      :  1413632 o  md5=9396d0bffa2caaef168e2f5a7bf918ac
payload/rose.exe   :  1413632 o  md5=9396d0bffa2caaef168e2f5a7bf918ac
RCDATA 3 du setup  :  1413632 o  md5=9396d0bffa2caaef168e2f5a7bf918ac
PREUVE : payload/rose.exe = RCDATA 3 = rose.exe = moteur complet C0 (RIR -> PE)
```

`assets/setup.tsv` ligne 3 : `10  3  /tmp/opencode/payload_c0_out/rose.exe  1413632`.

### Avant / après récapitulatif

| | avant | après |
|---|---|---|
| `payload/rose.exe` | 9 753 600 o, `02528a06…` (nupkg signé) | **1 413 632 o, `9396d0bf…` (C0)** |
| `RCDATA 3` du setup | 9 753 600 o, nupkg | **1 413 632 o, C0** |
| `Rose-win-Setup.exe` | 10 476 032 o, `d160c6f8…` | **2 136 064 o, `474fb456…`** |
| `assets/rose-engine.exe` | présent (copie du nupkg) | **absent** |
| recours au nupkg | 2 copies (payload + RCDATA 3) | **0** |

## 6. Non-régression du chemin cargo et des validations

`git status --short` ne montre que `scripts/build-sans-cargo.sh` (modifié) et
`rose/docs/PAYLOAD_C0.md` (nouveau). `scripts/build-windows-rebuild.sh` n'a pas
été touché : le défaut (`MODE=cargo`, lignes 138+) reste inchangé, seule la
branche `--selfhost` appelle le script modifié. `scripts/validation_finale.sh`,
`scripts/verifier_parite_selfhost.sh` et `scripts/verif_corpus.sh` n'appellent
ni `build-sans-cargo.sh`, ni ne lisent `payload/` : les contrôles « validation
0 échec » et « 141/141 » ne dépendent d'aucun fichier modifié ici.

Un texte du stage reste périmé et contredit par les mesures ci-dessus :
`NON_COUVERT_SELFHOST.txt`, écrit tel quel par `build-windows-rebuild.sh`
(fichier non modifié ici : il décrit encore des stubs 2,5 Ko). Le
`MANIFESTE.txt` du build ci-dessus contenait lui aussi une phrase héritée
(« moteur console publié ») : le texte correspondant de
`scripts/build-sans-cargo.sh` a été corrigé **après** ce build, en commentaire
uniquement — aucun artefact n'en dépend. Les fichiers faits foi sont
`WINDOWS_TENTATIVES.txt`, `WINDOWS_ARTIFACTS.tsv`, `VERIFIER_RES.txt`,
`PAYLOAD_ORIGINE.txt` et les 5 PE.
