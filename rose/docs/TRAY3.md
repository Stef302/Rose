# Helper `rose-tray` — icône PE + activation sans clic (vague 78)

État : **21/11/2026**. Dépôt `Stef302/Rose`. Ce chantier n'a exécuté **aucun
`git commit` et aucune publication**.

Fichiers du chantier : `rose/src/bin/rose-tray.rs`, `rose/rose-tray.rc`
(nouveau), `rose/build.rs`, `rose/src/demarrage.rs` (nouveau), `rose/src/lib.rs`,
`rose/src/setup_gui.rs`, `rose/docs/TRAY3.md` (nouveau), `.gitignore`.
`rose/src/setup_core.rs` est inchangé.

---

## 1. Problème initial

L'utilisateur rapportait : *« le tray n'a aucune icône sur son exe, faut lui
mettre aussi pour l'activer, j'ai cliqué dessus, c'est pas à moi de le faire
normalement. »*

Deux défauts distincts, confirmés par lecture du code et par le binaire Windows :

1. **`rose-tray.exe` n'avait AUCUNE icône dans son PE.** La section `.rsrc`
   n'existait pas (`llvm-objdump -h rose-tray.exe` ne montrait que `.text`,
   `.rdata`, …). Le seul chargement d'icône était un `LoadImageW` *au runtime*
   d'un `rose.ico` externe — et les chemins candidats pouvaient tous être
   absents. Résultat : icône générique Windows, et `explorer.exe` affichait un
   exe « vierge ».
2. **L'activation restait à la charge de l'utilisateur.** Le mode
   « Automatique (discret) » écrivait la config et essayait de lancer le
   helper, mais :
   - au premier écran « Continuer », le helper **n'était pas encore installé**
     (il n'arrive dans `bin\` qu'à la fin de l'installation) → lancement
     différé, jamais rattrapé ;
   - aucun Lanceur au démarrage n'était créé → après réouverture de session,
     plus rien ne se relançait.

---

## 2. Icône embarquée dans les ressources PE

### Mécanisme

`rose/rose-tray.rc` :

```
1 ICON "rose.ico"
```

`rose/build.rs` l'ajoute à la même boucle que `rose.rc` / `rose-shell.rc` :

- compilation `.rc` → `rose/rose-tray.res` (COFF, `llvm-windres`/`windres`) ;
- lien : `cargo:rustc-link-arg-bin=rose-tray=rose-tray.res`.

C'est exactement le résultat visé par `installateur/velopack/pack-res.py`
pour `rose-setup` : **RT_ICON (id 3)** + **RT_GROUP_ICON (id 14)**. Ici on
laisse le compilateur `.rc` découper l'ICO en six tailles (16/32/48/64/128/256)
et construire le groupe — le script `pack-res.py` faisait la même chose à la
main parce qu'il devait *aussi* embarquer la police et les binaires ; le
helper n'a besoin que de l'icône.

⚠ Quelle icône : `rose/rose.ico` = **LA ROSE**, le logo de base de
l'application. Pas `installateur/assets/rose.ico` (document `.rose`), pas
`rose_shell.ico` (console), pas `rose_setup.ico` (boîte d'installation).

### Chargement au runtime

`charger_icone()` essaie maintenant dans cet ordre :

1. **`LoadIconW(GetModuleHandleW(NULL), MAKEINTRESOURCE(1))`** — la ressource
   `RT_GROUP_ICON 1` embarquée. C'est la source normale : elle ne dépend
   d'aucun fichier externe.
2. `LoadImageW(..., LR_LOADFROMFILE | LR_SHARED)` sur `rose.ico` à côté de
   l'exe, `bin\rose.ico`, puis le répertoire courant et son `bin\` (repli pour
   les anciens exe sans ressources).
3. `IDI_APPLICATION` en dernier recours (garantit un `HICON` non nul pour
   `Shell_NotifyIconW`).

Chaque étape est journalisée dans `%TEMP%\rose-tray.log`.

### Preuve (cross-compilation `x86_64-pc-windows-msvc`)

```
$ llvm-objdump -h rose-tray.exe | grep .rsrc
  7 .rsrc  0000fe58  0000000140077000  DATA
$ llvm-readobj --coff-resources rose-tray.exe
  Total Number of Resources: 7
  Type: ICON (ID 3) [ ID 1..6, Language 1033, DataSize 691..36382 ]
  Type: GROUP_ICON (ID 14) [ ID 1, Language 1033, DataSize 90 ]
```

Avant le chantier, la même commande ne montrait **aucune** ligne `.rsrc` et
`llvm-readobj` rendait `Resources [Total Number of Resources: 0]`.

---

## 3. Activation automatique — plus aucun clic à faire

### 3.1 Au choix du mode

`setup_gui.rs` :

- `Action::Continuer` (premier écran) appelle déjà `lancer_helper_tray(h)`
  quand le mode est `Automatique`. **Correction** : quand le mode n'est pas
  discret, on appelle maintenant `retirer_demarrage_tray()` — une entrée
  héritée d'une config précédente est effacée.
- `lancer_helper_tray()` écrit la config, garde le timer L6 cohérent, puis
  appelle `activer_session_tray()`.
- `appliquer_mode_maj()` retire l'entrée dès que l'utilisateur quitte le mode
  discret.

### 3.2 À la fin de l'installation (le vrai correctif)

`WM_APP_FIN`, branche « installation réussie et non désinstallation » :

```rust
if crate::velopack::lire_config().mode.doit_demarrer_timer_discret() {
    if let Some(chemin) = chemin_helper_tray() {
        let _ = activer_session_tray(&chemin);
    }
}
```

C'est là que `rose-tray.exe` vient d'être copié dans `bin\`. L'entrée
`shell:startup` est posée et le helper est lancé immédiatement, **sans aucun
clic supplémentaire**. `TRAY_DEJA_LANCE` (AtomicBool) garantit qu'un seul
processus est lancé même si le bouton des Options et la fin d'installation
appellent la même fonction.

### 3.3 Entrée `shell:startup`

`rose/src/demarrage.rs`, nouveau module sans dépendance externe :

| Élément | Valeur |
|---|---|
| Dossier | `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup` |
| Repli | `%USERPROFILE%\AppData\Roaming\…` |
| Fichier | `rose-tray.cmd` |
| Droits | **aucun** : tout est sous le profil de l'utilisateur courant, rien dans HKLM |
| Contenu | 4 lignes ASCII : `@echo off`, un commentaire, `if exist "<exe>" start "" /B "<exe>"`, `exit /B 0` |

Le fichier est **relu après écriture** (`reussi = true` seulement si le
contenu lu est identique). Il est idempotent : relancer l'activation ne crée
pas de doublon.

Le lanceur est passif : `if exist` évite une erreur si le helper a été
désinstallé, et le helper relit la config au démarrage — une entrée oubliée
est donc inoffensive (le helper sort tout de suite si le mode n'est plus
`automatique_discret`).

### 3.4 Désactivation

Trois chemins mènent à la même fonction `demarrage::desactiver()` :

1. menu contextuel **« Désactiver le helper »** (clic droit sur l'icône) ;
2. **`rose-tray --disable`** (ligne de commande, aucun clic) — écrit
   `{"mode": "desactive"}` puis supprime le lanceur, puis sort ;
3. choix « Désactivée » / « À l'ouverture » dans le setup, et désinstallation.

La suppression est vérifiée par relecture (`!chemin.exists()`).
`rose-tray --disable` ne touche ni à un service (il n'y en a pas), ni au
registre machine.

---

## 4. Chemin exact et vérification sur Windows

### Vérifier l'icône du PE

```powershell
# PowerShell / Terminal dans le dossier de l'artefact
$exe = "$env:LOCALAPPDATA\Programs\Rose\bin\rose-tray.exe"
Get-Item $exe | Select-Object Length, LastWriteTime
# Icône visible dans l'Explorateur : l'onglet Propriétés de rose-tray.exe
# doit montrer LA ROSE, pas une icône vierge.
# Les ressources NotePad/Resource Hacker : RT_ICON 1..6 + RT_GROUP_ICON 1.
```

### Vérifier l'entrée Démarrage

```powershell
$startup = "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Startup"
Get-ChildItem $startup | Where-Object Name -eq 'rose-tray.cmd'
Get-Content "$startup\rose-tray.cmd"
# ou, plus court : Win+R → shell:startup → le fichier rose-tray.cmd doit être là
```

La ligne `if exist "C:\…\bin\rose-tray.exe" start "" /B "C:\…\bin\rose-tray.exe"`
doit pointer vers le **vrai** binaire installé.

### Vérifier que le helper tourne

```powershell
Get-Process rose-tray -ErrorAction SilentlyContinue
Get-Content "$env:TEMP\rose-tray.log" -Tail 30
```

Attendu dans le journal :

```
démarrage : lanceur écrit C:\Users\...\Startup\rose-tray.cmd
démarrage : lanceur RELU conforme (session suivante couverte)
icône : RT_GROUP_ICON 1 embarquée dans le PE (rose-tray.rc) chargée
helper : lancement DETACHED_PROCESS|CREATE_NO_WINDOW de C:\...\bin\rose-tray.exe
helper : spawn OK (processus détaché)
tray actif : icône + timer 6 h posés, entrée dans GetMessageW
```

### Vérifier la désactivation

```powershell
& "$env:LOCALAPPDATA\Programs\Rose\bin\rose-tray.exe" --disable
Test-Path "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Startup\rose-tray.cmd"   # False
Get-Content "$env:APPDATA\Rose\config.json"                                            # "desactive"
```

Puis redémarrer la session : aucun `rose-tray.exe`, aucun clignotement de
console, aucune icône.

---

## 5. Preuves produites ici (Linux, 21/11/2026)

| Contrôle | Résultat |
|---|---|
| `cargo build --release --bin rose-tray` | ✅ `Finished release profile` |
| `cargo test --release --bin rose-tray` | ✅ **8 passés / 0 échec** (4 historiques + 4 nouveaux) |
| `cargo t --release` (= `cargo test --bins`, alias du dépôt) | ✅ **294 passés / 0 échec / 3 ignorés** dans le binaire `rose`, 0 / 6 / 8 pour les autres binaires |
| Cross-compile `cargo build --release --target x86_64-pc-windows-msvc --bin rose-tray` | ✅ RC=0, `rose-tray.exe` 529 920 octets |
| Ressources PE | ✅ `.rsrc` présent ; `llvm-readobj --coff-resources` : `Type: ICON (ID 3)` 6 entrées + `Type: GROUP_ICON (ID 14)` 1 entrée, langue 1033 |
| `.res` généré | ✅ `rose/rose-tray.res` 65 508 octets, avec les mêmes types `3, 14` que le test de `pack-res.py` |
| Point fixe ×2 | ✅ voir §6 |
| Exécution réelle sur Windows | ❌ **non mesurable ici** (aucun Windows sur cette machine). Le comportement du tray, l'affichage réel de l'icône dans la zone de notification et le clignotement du `.cmd` au logon restent à constater chez l'utilisateur. |

---

## 6. Point fixe — constat daté

Deux compilations du même programme, deuxième avec le compilateur en source :

```
sha256 / md5 attendu : b3ed9262dd54776a6dbd07bc12368d0e  (263 520 octets)
```

Mesuré le 21/11/2026 depuis la racine du dépôt, avec `rose/rose` reconstruit
par `cargo build --release` :

```
$ ./rose/rose rir compilateur/emetteur.rose
    rc=0, 263 520 octets, stderr 0 octet
    md5 = b3ed9262dd54776a6dbd07bc12368d0e
$ ./rose/rose compilateur/emetteur.rose compilateur/emetteur.rose
    rc=0, 263 520 octets, stderr 0 octet
    md5 = b3ed9262dd54776a6dbd07bc12368d0e
$ cmp des deux sorties → IDENTIQUE
```

Source `compilateur/emetteur.rose` : md5 `390335d29fb9cee368b3456dd892e377`.
Aucun changement du compilateur dans ce chantier (seuls l'emballage Windows,
le helper et le setup ont été touchés) : le point fixe reste vert.

---

## 7. Limites Windows

- **Console au logon** : Windows exécute le `.cmd` dans une fenêtre qui
  clignote ~100 ms avant `start /B`. Le helper lui-même est en sous-système
  Windows (aucune console). Un vrai raccourci `.lnk` supprimerait ce
  clignotement, mais exigerait COM/IShellLinkW (~100 lignes de FFI non
  testables ici) : un `.cmd` a été retenu, comme autorisé par la demande.
- **Zone des icônes cachées** : gérée par Windows ; l'icône peut être masquée
  ou déplacée par une mise à jour de la barre des tâches. Le helper ne peut
  pas forcer son affichage.
- **Helper déjà en cours** : `--disable` et le menu retirent l'entrée et
  écrivent `desactive`, mais ne tuent pas un autre processus déjà lancé (pas
  d'IPC). Il sortira au prochain clic « Quitter »/« Désactiver » ou à la
  fermeture de session — et ne reviendra pas au logon.
- **Profil verrouillé** : si `%APPDATA%` est en lecture seule, l'entrée ne
  peut pas être écrite ; le cas est journalisé et `reussi = false` — le
  message de statut ne prétend pas que le démarrage auto est actif.
- Aucun service, aucune tâche planifiée, aucune clé HKLM, aucune élévation.

⚠ Ce document est un constat daté : re-mesurer après tout changement de code,
de cible ou de stratégie d'installation.
