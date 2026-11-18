# Vérification indépendante des artefacts Windows reconstruits

**Date : 18/11/2026 — 10:03:18 UTC / 11:03:18 CET** (toutes les commandes
ci-dessous ont été exécutées à cette date sur l'hôte **Linux**).

Mission : re-vérifier **sans modifier le code source** les artefacts reconstruits
par l'agent D2 (commit `bbda07d`). Aucun fichier autre que
`rose/docs/VERIF_WINDOWS.md` n'a été écrit dans le dépôt. **Aucun commit, aucune
publication GitHub.**

Périmètre : cet agent est un **vérificateur indépendant**. Les hashs attendus
proviennent de l'énoncé de mission / de `installateur/velopack/REBUILD.md` ; les
sorties ci-dessous sont celles réellement obtenues, recalculées à partir des
fichiers sur disque.

---

## 1. Identification de l'état du dépôt

```
$ cd /home/clausake/Stefan/ROSE
$ pwd
/home/clausake/Stefan/ROSE
$ git rev-parse --abbrev-ref HEAD
main
$ git log -1 --oneline
9e18cef PLAN : 150/150 sur les 4 oracles (sans-faute corpus élargi)
$ git merge-base --is-ancestor bbda07d HEAD && echo "YES bbda07d is ancestor"
YES bbda07d is ancestor
```

Le commit de reconstruction `bbda07d` est bien un ancêtre de `main` (`9e18cef`).

---

## 2. Empreintes recalculées (objectif 1)

Commandes réelles : `md5sum …` et `sha256sum …` sur les fichiers du dépôt.

| Artefact | Algorithme | Valeur attendue | Valeur mesurée | Verdict |
|---|---|---|---|---|
| `installateur/velopack/stage/rose.exe` | MD5 | `3b4276dd8f55b2c068d7993cf73166b0` | `3b4276dd8f55b2c068d7993cf73166b0` | ✅ |
| `installateur/velopack/stage/rose-shell.exe` | MD5 | `4c74033a27e405286ad08ab2b447cbe6` | `4c74033a27e405286ad08ab2b447cbe6` | ✅ |
| `installateur/velopack/stage/Rose-win-Setup.exe` | MD5 | `4a2e42e4595b400e5cad50fef1e7e0b0` | `4a2e42e4595b400e5cad50fef1e7e0b0` | ✅ |
| `installateur/velopack/Releases/Rose-0.1.5-full.nupkg` | SHA-256 | `ad418be2397d421db90e2e27a14e91250a04085fe5d402b087a73a90ba207df5` | `ad418be2397d421db90e2e27a14e91250a04085fe5d402b087a73a90ba207df5` | ✅ |

Sortie brute (horodatage ci-dessus) :

```
$ md5sum installateur/velopack/stage/rose.exe installateur/velopack/stage/rose-shell.exe installateur/velopack/stage/Rose-win-Setup.exe
3b4276dd8f55b2c068d7993cf73166b0  installateur/velopack/stage/rose.exe
4c74033a27e405286ad08ab2b447cbe6  installateur/velopack/stage/rose-shell.exe
4a2e42e4595b400e5cad50fef1e7e0b0  installateur/velopack/stage/Rose-win-Setup.exe

$ sha256sum installateur/velopack/Releases/Rose-0.1.5-full.nupkg
ad418be2397d421db90e2e27a14e91250a04085fe5d402b087a73a90ba207df5  installateur/velopack/Releases/Rose-0.1.5-full.nupkg
```

Contrôles croisés supplémentaires (valeurs réellement mesurées) :

```
$ md5sum installateur/velopack/Releases/Rose-0.1.5-full.nupkg
7e1495b9107aab444ab1b3ce889f29fa  installateur/velopack/Releases/Rose-0.1.5-full.nupkg
$ md5sum installateur/velopack/stage/payload/rose.exe
3b4276dd8f55b2c068d7993cf73166b0  installateur/velopack/stage/payload/rose.exe
$ md5sum rose/target/x86_64-pc-windows-msvc/release/rose-setup.exe
4a2e42e4595b400e5cad50fef1e7e0b0  rose/target/x86_64-pc-windows-msvc/release/rose-setup.exe
$ cmp -s installateur/velopack/stage/Rose-win-Setup.exe rose/target/x86_64-pc-windows-msvc/release/rose-setup.exe && echo "IDENTIQUES (cmp rc=0)"
IDENTIQUES (cmp rc=0)
```

Le paquet `Releases/Rose-0.1.5-full.nupkg` porte le MD5 `7e1495b9…`
(celui annoncé dans `REBUILD.md`) et le SHA-256 `ad418be2…` (celui annoncé dans
l'énoncé de mission). Les deux sont cohérents : ce sont deux fonctions de hash
différentes du **même fichier**. Le manifeste publié confirme la valeur :

```
$ cat installateur/velopack/Releases/releases.win.json
[
  {
    "PackageId": "Rose-0.1.5-full.nupkg",
    "Version": "0.1.5",
    "Type": "Full",
    "FileName": "Rose-0.1.5-full.nupkg",
    "SHA1": "1e52f888602dd7408d392eb6bcfca292be15773c",
    "SHA256": "ad418be2397d421db90e2e27a14e91250a04085fe5d402b087a73a90ba207df5",
    "SHA512": "tFlQwsvY2Hor6+WNM/oR/zM8nFCKnHJ0lRee9IPQdsnhpyy2tJR9zl9YI2XlUzhE8GLD6ncTczQQysUQecIqZw==",
    "Size": 8740427
  }
]
$ sha1sum installateur/velopack/Releases/Rose-0.1.5-full.nupkg
1e52f888602dd7408d392eb6bcfca292be15773c  installateur/velopack/Releases/Rose-0.1.5-full.nupkg
$ openssl dgst -sha512 -binary installateur/velopack/Releases/Rose-0.1.5-full.nupkg | base64 -w0
tFlQwsvY2Hor6+WNM/oR/zM8nFCKnHJ0lRee9IPQdsnhpyy2tJR9zl9YI2XlUzhE8GLD6ncTczQQysUQecIqZw==
```

➡️ Version annoncée : **0.1.5** (`releases.win.json`, champ `Version`).

---

## 3. `file` sur chaque exécutable (objectif 2)

```
$ file installateur/velopack/stage/rose.exe installateur/velopack/stage/rose-shell.exe installateur/velopack/stage/Rose-win-Setup.exe installateur/velopack/Releases/Rose-0.1.5-full.nupkg
installateur/velopack/stage/rose.exe:                 PE32+ executable (console) x86-64, for MS Windows, 9 sections
installateur/velopack/stage/rose-shell.exe:           PE32+ executable (console) x86-64, for MS Windows, 9 sections
installateur/velopack/stage/Rose-win-Setup.exe:       PE32+ executable (GUI) x86-64, for MS Windows, 9 sections
installateur/velopack/Releases/Rose-0.1.5-full.nupkg: Zip archive data, at least v2.0 to extract, compression method=deflate
```

Les trois exe sont des **PE32+ x86-64 valides**, avec les sous-systèmes attendus :
`console` pour le moteur et le shell, `GUI` pour le setup. Le nupkg est bien une
archive ZIP.

---

## 4. `verifier-res.py` — sorties exactes (objectif 2)

Outil réel : `installateur/velopack/verifier-res.py` (chemin confirmé par
`find`). Il valide l'arbre des ressources `.rsrc` d'un PE lié : types, noms,
langues 1033, alignement, signature des icônes DIB, manifeste ASCII
PerMonitorV2, moteur embarqué, compagnons, garde-fous de tracé.

### 4.1 Appel par défaut — ÉCHEC ATTENDU (limite de l'outil)

```
$ python3 installateur/velopack/verifier-res.py
Traceback (most recent call last):
  File "/home/clausake/Stefan/ROSE/installateur/velopack/verifier-res.py", line 394, in <module>
    main()
  File "/home/clausake/Stefan/ROSE/installateur/velopack/verifier-res.py", line 123, in main
    rsrc = section_rsrc(donnees)
  File "/home/clausake/Stefan/ROSE/installateur/velopack/verifier-res.py", line 97, in section_rsrc
    raise AssertionError("verifier-res.py ne sait pas relire un .res directement ; "
AssertionError: verifier-res.py ne sait pas relire un .res directement ; utiliser l'EXE lié
rc=1
```

L'appel sans argument cible `rose/rose-setup.res` (un `.res` MSVC brut) que
l'outil refuse explicitement de relire. **Ce n'est pas un défaut des artefacts** :
la documentation de l'outil indique « utiliser l'EXE lié ». Les vérifications
utiles portent donc sur les exe.

### 4.2 Setup — contrôle complet

```
$ python3 installateur/velopack/verifier-res.py installateur/velopack/stage/Rose-win-Setup.exe
Rose-win-Setup.exe : section .rsrc de 8939520 o (PE lié, OffsetToData = RVA)
  racine : 5 types
  16 ressources :
    type 3  nom 1     1128 o
    type 3  nom 2     2440 o
    type 3  nom 3     4264 o
    type 3  nom 4     9640 o
    type 3  nom 5    16936 o
    type 3  nom 6    67624 o
    type 3  nom 7    14278 o
    type 8  nom 2    28352 o
    type 10 nom 1    59812 o
    type 10 nom 3  7963648 o
    type 10 nom 5   477696 o
    type 10 nom 6   112510 o
    type 10 nom 7    64700 o
    type 10 nom 8   113565 o
    type 14 nom 1      104 o
    type 24 nom 1     1474 o
  groupe d'icônes : 7 tailles (16x16, 24x24, 32x32, 48x48, 64x64, 128x128, 256x256)
  manifeste : 1474 o, ASCII, PerMonitorV2
RESSOURCES VALIDES
rc=0
```

Note factuelle : l'outil n'affiche pas les lignes « subsystem », « moteur
embarqué », « compagnons », « rose PNG » et « tracé » quand le nom de fichier
est `Rose-win-Setup.exe` — ses tests les déclenchent uniquement pour un nom
commençant par `rose-setup` ou `setup-`. Pour exercer **tous** les contrôles,
j'ai aussi appelé l'outil sur le fichier `rose-setup.exe` de `target/`, qui est
**octet-identique** au setup du stage (preuve `cmp` en §2) :

```
$ python3 installateur/velopack/verifier-res.py rose/target/x86_64-pc-windows-msvc/release/rose-setup.exe
rose-setup.exe : section .rsrc de 8939520 o (PE lié, OffsetToData = RVA)
  racine : 5 types
  16 ressources :
    [les mêmes 16 lignes que ci-dessus]
  groupe d'icônes : 7 tailles (16x16, 24x24, 32x32, 48x48, 64x64, 128x128, 256x256)
  manifeste : 1474 o, ASCII, PerMonitorV2
  subsystem : 2 (GUI)
  moteur embarqué : 7963648 o (PE console)
  rose-shell.exe     477696 o
  rose.ico           112510 o
  rose_app.ico        64700 o
  rose_shell.ico     113565 o
  rose PNG      59812 o (276x343, RGBA)
  tracé : 33 garde-fous présents
RESSOURCES VALIDES
rc=0
```

### 4.3 Moteur et shell seuls

```
$ python3 installateur/velopack/verifier-res.py installateur/velopack/stage/rose.exe
rose.exe : section .rsrc de 65536 o (PE lié, OffsetToData = RVA)
  racine : 2 types
  7 ressources :
    type 3  nom 1      691 o
    type 3  nom 2     1908 o
    type 3  nom 3     3547 o
    type 3  nom 4     5647 o
    type 3  nom 5    16423 o
    type 3  nom 6    36382 o
    type 14 nom 1       90 o
  groupe d'icônes : 6 tailles (16x16, 32x32, 48x48, 64x64, 128x128, 256x256)
RESSOURCES VALIDES
rc=0

$ python3 installateur/velopack/verifier-res.py installateur/velopack/stage/rose-shell.exe
rose-shell.exe : section .rsrc de 114176 o (PE lié, OffsetToData = RVA)
  racine : 2 types
  8 ressources :
    type 3  nom 1     1128 o
    type 3  nom 2     2440 o
    type 3  nom 3     4264 o
    type 3  nom 4     9640 o
    type 3  nom 5    16936 o
    type 3  nom 6    67624 o
    type 3  nom 7    11415 o
    type 14 nom 1      104 o
  groupe d'icônes : 7 tailles (16x16, 24x24, 32x32, 48x48, 64x64, 128x128, 256x256)
RESSOURCES VALIDES
rc=0
```

**Bilan : `RESSOURCES VALIDES` (rc=0) dans les 4 cas PE pertinents.**

---

## 5. Icône correcte par exécutable (objectif 3)

Vérification **indépendante** : j'ai écrit un script temporaire hors dépôt
(`/tmp/opencode/verif_indep.py`) qui réutilise uniquement les primitives
d'extraction de `verifier-res.py` (`sections_pe`, `section_rsrc`, `entrees`,
`rva_vers_offset`) mais **reconstruit lui-même** la comparaison. Méthode :

1. extraire chaque entrée `RT_ICON` du PE (DIB ou PNG) ;
2. lire les blocs image du `.ico` source (répertoire + offsets) ;
3. comparer les octets et afficher les MD5 des deux côtés.

```
$ python3 /tmp/opencode/verif_indep.py
… (section 1 et 2, voir §6 ci-dessous) …

3) ICONES PAR BINAIRE

--- rose.exe <-> rose.ico ---
  RT_ICON dans le PE : 6 entrees (ids [1, 2, 3, 4, 5, 6])
  entrees dans le .ico : 6 (16x16, 32x32, 48x48, 64x64, 128x128, 256x256)
  #1 16x16     ico=4eeb5263463ce5363ee753f835713b28  pe=4eeb5263463ce5363ee753f835713b28  IDENTIQUE
  #2 32x32     ico=9f8c0820dfbbb0b6bd3620254e7ee9f0  pe=9f8c0820dfbbb0b6bd3620254e7ee9f0  IDENTIQUE
  #3 48x48     ico=61de67f4c8ce7fed66e838a7b45efa57  pe=61de67f4c8ce7fed66e838a7b45efa57  IDENTIQUE
  #4 64x64     ico=27884f7bd5db9a1f928d6562588c7c3e  pe=27884f7bd5db9a1f928d6562588c7c3e  IDENTIQUE
  #5 128x128   ico=b8243812c48228d39b0cce50f1bd651c  pe=b8243812c48228d39b0cce50f1bd651c  IDENTIQUE
  #6 256x256   ico=4b726aa2cc63a442834e3029568742f2  pe=4b726aa2cc63a442834e3029568742f2  IDENTIQUE
  RESULTAT : TOUTES IDENTIQUES (6/6)

--- rose-shell.exe <-> rose_shell.ico ---
  RT_ICON dans le PE : 7 entrees (ids [1, 2, 3, 4, 5, 6, 7])
  entrees dans le .ico : 7 (16x16, 24x24, 32x32, 48x48, 64x64, 128x128, 256x256)
  #1 16x16     ico=162efec2a2c3f8a715cc432e39c8c307  pe=162efec2a2c3f8a715cc432e39c8c307  IDENTIQUE
  #2 24x24     ico=48d245ff3d203d757b24489577b7dbaf  pe=48d245ff3d203d757b24489577b7dbaf  IDENTIQUE
  #3 32x32     ico=6fc03d8cb1cd77de6055ddda6e86a752  pe=6fc03d8cb1cd77de6055ddda6e86a752  IDENTIQUE
  #4 48x48     ico=68fdef109bf807fa8628016743b29e29  pe=68fdef109bf807fa8628016743b29e29  IDENTIQUE
  #5 64x64     ico=da9e842764dfbe87465b354611081002  pe=da9e842764dfbe87465b354611081002  IDENTIQUE
  #6 128x128   ico=ce3a75819f9adc802e842f3cdaa50651  pe=ce3a75819f9adc802e842f3cdaa50651  IDENTIQUE
  #7 256x256   ico=d8b758633a016608ff33b2e59f00756b  pe=d8b758633a016608ff33b2e59f00756b  IDENTIQUE
  RESULTAT : TOUTES IDENTIQUES (7/7)

RESULTAT GLOBAL ICÔNES : PASS
rc=0
```

✅ `rose.exe` embarque bien les 6 images de `rose.ico` ; `rose-shell.exe` embarque
bien les 7 images de `rose_shell.ico`. Aucune inversion des deux icônes.

---

## 6. Moteur embarqué octet-identique (objectif 4)

Même script, extraction de `RT_RCDATA 3` (moteur) et `RT_RCDATA 5` (shell
compagnon) du setup puis comparaison octet par octet avec les fichiers du stage :

```
$ python3 /tmp/opencode/verif_indep.py
======================================================================
1) MOTEUR EMBARQUE DANS LE SETUP vs stage/rose.exe
  RT_RCDATA 3 : 7963648 o, md5 3b4276dd8f55b2c068d7993cf73166b0
  stage/rose.exe : 7963648 o, md5 3b4276dd8f55b2c068d7993cf73166b0
  OCTET-IDENTIQUE : True
  RT_RCDATA 5 : 477696 o, md5 4c74033a27e405286ad08ab2b447cbe6
  stage/rose-shell.exe : 477696 o, md5 4c74033a27e405286ad08ab2b447cbe6
  COMPAGNON SHELL OCTET-IDENTIQUE : True

======================================================================
2) NUPKG : fichiers internes vs stage
  lib/net45/Rose-win-Setup.exe            9633792 o md5 4a2e42e4595b400e5cad50fef1e7e0b0  identique au stage/Rose-win-Setup.exe : True
  lib/net45/rose.exe                      7963648 o md5 3b4276dd8f55b2c068d7993cf73166b0  identique au stage/rose.exe : True
  lib/net45/rose-shell.exe                 477696 o md5 4c74033a27e405286ad08ab2b447cbe6  identique au stage/rose-shell.exe : True
  lib/net45/payload/rose.exe              7963648 o md5 3b4276dd8f55b2c068d7993cf73166b0  identique au stage/rose.exe : True
```

✅ Le moteur embarqué dans `Rose-win-Setup.exe` est **octet-identique** à
`stage/rose.exe` (`cmp` logique via comparaison des bytes, True). La « passe 2 »
de reconstruction a bien fonctionné. Les 4 exe du nupkg sont également
octet-identiques à leurs homologues du stage.

Contenu complet du nupkg (`unzip -l`, horodatage ZIP fixe 2026-01-01) :

```
$ unzip -l installateur/velopack/Releases/Rose-0.1.5-full.nupkg
     1064  2026-01-01 00:00   lib/net45/LICENSE
  9633792  2026-01-01 00:00   lib/net45/Rose-win-Setup.exe
   477696  2026-01-01 00:00   lib/net45/rose-shell.exe
  7963648  2026-01-01 00:00   lib/net45/rose.exe
   112510  2026-01-01 00:00   lib/net45/rose.ico
    64700  2026-01-01 00:00   lib/net45/rose_app.ico
   116428  2026-01-01 00:00   lib/net45/rose_setup.ico
   113565  2026-01-01 00:00   lib/net45/rose_shell.ico
  7963648  2026-01-01 00:00   lib/net45/payload/rose.exe
      200  2026-01-01 00:00   sq.version
---------                     -------
 26447251                     10 files
```

---

## 7. Limites — ce qui N'EST PAS prouvé (objectif 5)

**Je suis sous Linux. Je n'ai PAS lancé ces exécutables, ni l'installateur, ni
aucun code Windows. Les affirmations ci-dessus sont exclusivement
structurelles.**

### Structurellement prouvé (preuves ci-dessus)

- Les 3 exe sont des PE32+ x86-64 avec sous-systèmes cohérents (console/console/GUI).
- Les 4 hashs attendus sont reproduits à l'octet près.
- L'arbre des ressources `.rsrc` est valide et lisible selon `verifier-res.py` :
  racine → types → noms → langue 1033 → données, offsets alignés et dans la
  section, signature des DIB d'icônes correcte (ni conteneur `.ico` parasite, ni
  en-tête invalide), manifeste ASCII `PerMonitorV2`, 33 garde-fous de tracé
  présents dans `setup_gui.rs`.
- Les icônes sont les bonnes (6/6 et 7/7 identiques aux `.ico` sources).
- Le moteur embarqué est octet-identique au moteur livré ; le paquet contient
  les mêmes exe que le stage.

### NON prouvé — nécessite une machine Windows

1. **Lancement réel de `Rose-win-Setup.exe`** : l'exécution du GUI, l'affichage
   de la fenêtre, le tracé effectif (fond, coins arrondis, logo, police),
   l'absence de clignotement et l'absence de fenêtre vide ne peuvent pas être
   constatés ici. Les 33 garde-fous sont une vérification **statique du code**,
   pas une preuve d'exécution.
2. **Installation réelle** : copie des fichiers dans `%LOCALAPPDATA%`,
   création des raccourcis, associations `.rose`, désinstallation, roll-back.
   Ces chemins ne sont exercés par aucun test sous Linux.
3. **Code de sortie et dialogues d'erreur** : le célèbre « Cette application ne
   peut pas s'exécuter sur votre PC » (manifeste / sous-système refusé par le
   chargeur Windows) est précisément ce que `verifier-res.py` cherche à
   prévenir, mais **seul Windows peut le confirmer**.
4. **Outil Velopack officiel absent** : `vpk` = « command not found » et
   `dotnet` = « command not found » sur cet hôte. Le setup livré est celui
   produit par la voie Python (`pack-linux.sh`), pas un *launcher* Velopack
   officiel avec deltas/rollback. À faire sur machine outillée :
   `dotnet tool install -g vpk` puis `vpk pack …` (voir
   `installateur/velopack/build.ps1`).
5. **Aucun Wine/Windows dans l'environnement** : aucune émulation n'a été
   utilisée ; aucun `.exe` n'a été exécuté.

### Procédure à exécuter sur machine Windows (à faire par un opérateur)

```powershell
# 1) Contrôle structurel avec les outils MSVC (dumpbin)
dumpbin /headers Rose-win-Setup.exe | findstr /i "subsystem machine"
#    → attendu : machine = x64, subsystem = 2 (GUI)
dumpbin /headers rose.exe | findstr /i "subsystem machine"
#    → attendu : machine = x64, subsystem = 3 (console)
dumpbin /resources Rose-win-Setup.exe | findstr /i "RT_RCDATA RT_ICON"
#    → vérifier la présence des ressources 1,3,5,6,7,8

# 2) Lancement réel et code de sortie
.\Rose-win-Setup.exe
echo %ERRORLEVEL%
#    → 0 attendu ; tout autre code = échec à investiguer

# 3) Après installation : lancer le moteur et le shell
rose.exe --version
echo %ERRORLEVEL%
rose-shell.exe
echo %ERRORLEVEL%

# 4) Contrôle visuel
#    - la fenêtre du setup doit être PEINTE (pas de simple bordure transparente)
#    - le logo rose doit apparaître, les boutons doivent répondre sans clignoter
#    - l'icône de rose.exe = rose.ico ; celle de rose-shell.exe = rose_shell.ico
#      (Windows Terminal : onglet Rose = icône console, pas icône app)
```

---

## 8. Règles du projet — vérifiées

- **GMP jamais dépendance** : `grep -niE 'gmp|rug' rose/Cargo.toml rose/Cargo.lock`
  → **aucune occurrence**. Les mentions de « GMP » dans `rose/src/*` sont des
  commentaires comparatifs (« sans GMP », « à la manière de GMP »), pas une
  dépendance. ✅
- **Release 0.1.5** : `releases.win.json` → `"Version": "0.1.5"` ; nupkg
  `Rose-0.1.5-full.nupkg`. ✅
- **GitHub = Stef302/Rose** : aucune opération GitHub effectuée par cet agent
  (aucun commit, aucun push, aucune publication). ✅
- **Aucune modification de code source** : seul `rose/docs/VERIF_WINDOWS.md` a
  été créé, hors dépôt source (et le script temporaire `/tmp/opencode/verif_indep.py`
  est **en dehors du dépôt**). ✅

Note de traçabilité : pendant cette vérification (11:03–11:05 CET), `git status`
a montré des modifications **concurrentes** dans `rose/src/wasm_runtime.rs`,
`rose/docs/WASM_RUNTIME.md` et `scripts/audit-final.sh` (mtime 11:03:22–11:05:05).
Elles proviennent d'autres agents travaillant sur le dépôt en parallèle et
**ne sont pas issues de cette mission** ; ce vérificateur n'y a pas touché.
Le `git status` initial de cette session (10:03 UTC) était propre.

---

## Conclusion factuelle

Les 4 artefacts reconstruits passent **tous les contrôles structurels exécutables
sous Linux** : hashs conformes, PE valides, ressources valides, bonnes icônes,
moteur embarqué octet-identique, paquet cohérent, version 0.1.5, zéro GMP.
**Aucune affirmation d'exécution sous Windows n'est faite** : le lancement,
l'installation et le code de sortie restent à valider sur machine Windows
selon la procédure du §7.
