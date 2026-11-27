# SETUP_DOWNLOADER — `Rose-win-Setup.exe` est un DOWNLOADER / UPDATER

Constat **daté et mesuré**, pas une promesse. Worktree exclusif
`/tmp/opencode/v18dl` (HEAD `2eecd62`), sans commit ni push.

## Correction de compréhension (cause du faux départ)

Le setup Windows **n'est pas un installateur qui copie des fichiers locaux**.
C'est un **downloader / updater** : l'utilisateur le télécharge **une seule
fois** depuis le launcher, puis il interroge le dépôt, télécharge la version
courante, synchronise les mises à jour et relance Rose. Aucune installation
locale de fichiers préexistants ne doit être codée dans le stub.

L'agent `v17setup` avait pris la fausse piste de la copie locale de fichiers ;
il a été arrêté. Ce document fixe le flux réel.

## Le flux exact (clic → API → download → sync → lancement)

```
clic sur Rose-win-Setup.exe  (stub Rose, mode `setup`)
  │  GetModuleFileNameW(NULL, buf, 260)
  │  PathRemoveFileSpecW(buf)          → dossier du setup
  │  lstrcatW(buf, L"\\rose.exe")       → <dossier>\rose.exe  (chemin ABSOLU)
  └  ShellExecuteW(NULL, L"open", buf,
                   L"update --velopack --setup", NULL, SW_SHOWNORMAL)
       │
       ▼  rose.exe (binaire Rose réel, console)
  install_rose()  →  velopack::check_and_update_consenti(true)
       │  1. GET https://api.github.com/repos/Stef302/Rose/releases?per_page=100
       │     (API PUBLIQUE, sans jeton ; curl -sSL --fail, 3 s max,
       │      CREATE_NO_WINDOW — aucune fenêtre noire parasite)
       │  2. analyser_releases() : assets RÉELS du tableau `assets` ;
       │     plus récente version > version installée (`current`/`app-*`),
       │     jamais `CARGO_PKG_VERSION`. Aucune version codée en dur.
       │  3. GET .../releases/download/<tag>/releases.win.json
       │     (manifeste DU TAG, pas celui de `/latest` qui n'a qu'une entrée)
       │  4. GET .../releases/download/<tag>/Rose-<v>-full.nupkg
       │     → curl -o %LOCALAPPDATA%\Rose\packages\<nom>
       │  5. SHA-256 du fichier == champ `SHA256` du manifeste ;
       │     sinon le paquet est SUPPRIMÉ et rien n'est installé.
       │  6. appliquer_paquets() : extraction ZIP `lib/net45/*` vers
       │     app-<v>\ , écriture de `current` = `app-<v>`
       │     (l'ancienne version reste sur disque = rollback manuel).
       └  7. relancer_apres_setup() : spawn de `current\rose.exe console`
```

Étapes (a) à (f) de la consigne : **(a)** API GitHub ✅, **(b)** affichage de
la liste des versions ⚠️ (voir « Reste à faire »), **(c)** `.nupkg` de la
dernière ✅, **(d)** hash SHA-256 ✅, **(e)** extraction / `current` ✅,
**(f)** relance ✅ (côté Rose, pas côté stub).

### Pourquoi l'option (a) — invocation de `rose update` — a été choisie

Le stub est du **code machine nu émis par C0** (`compilateur/pe_stub_ressources.rose`) :
il n'a ni runtime, ni TLS, ni parseur JSON, ni décompresseur ZIP. L'option (b)
(WinHTTP / `URLDownloadToFile` dans le stub) exigerait d'émettre à la main le
handshake HTTPS, la validation du certificat, le parseur de la réponse GitHub,
le ZIP Deflate et la gestion d'erreur — une surface énorme, dupliquée, et
**invérifiable ici** (aucun Windows pour exécuter le PE).

L'option (a) réutilise `rose/src/velopack.rs`, qui fait **déjà exactement** ce
qu'il faut (découverte dynamique, assets GitHub, SHA-256, extraction, rollback)
et qui est testé sous Linux avec des fixtures `file://`. Un seul chemin de
téléchargement, une seule source de vérité, aucune dépendance ajoutée.

### Le piège du garde-fou de mode

`check_and_update()` refuse toute vérification quand
`%APPDATA%\Rose\config.json` est en mode `desactive` (le **défaut**). Un
downloadeur fraîchement téléchargé tomberait donc sur « mises à jour
DÉSACTIVÉES » avant même d'avoir téléchargé quoi que ce soit.

Correction : `velopack::check_and_update_consenti()` appelle directement
`verifier_et_telecharger()` **sans lire ni écrire `config.json`**. Le clic sur
le setup EST le consentement explicite. Le garde-fou reste **intact** pour
`rose update` normal, la banderole d'ouverture et le timer 6 h. Test de non
régression : `velopack::tests::config_persistance_et_effet_au_demarrage`
vérifie que `check_and_update(true)` est bloqué en mode désactivé alors que
`check_and_update_consenti(true)` traverse bien le garde-fou (et échoue ici
seulement parce que le manifeste de test est absent).

## Fichiers modifiés

| Fichier | Changement |
|---|---|
| `compilateur/pe_stub_ressources.rose` | `setup_gui()` : 4 imports Win32 (`GetModuleFileNameW`, `lstrcatW`, `PathRemoveFileSpecW`, `ShellExecuteW`), 3 chaînes UTF-16 + tampon 260 WCHAR, séquence de lancement du downloader, texte du `DrawTextW` remplacé. |
| `rose/src/velopack.rs` | `check_and_update_consenti()` + `mettre_a_jour_depuis_setup()` (consentement explicite) ; test de non régression ; test réel d'API rendu indépendant de la version courante. |
| `rose/src/main.rs` | `--setup` / `--downloader` dans `install_rose()` : voie consentie, puis `relancer_apres_setup()` (`current\rose.exe console`). `rose update` ordinaire garde le garde-fou de mode. |
| `rose/docs/UPDATE_DYNAMIQUE.md` | sortie attendue du test réel devenue dynamique (la dernière release était v0.1.86 le 27/11/2026). |
| `rose/docs/SETUP_DOWNLOADER.md` | ce document. |

## Réellement implémenté / reste à faire

| Élément | État |
|---|---|
| Clic → lancement de `rose update --velopack --setup` | ✅ émis en machine code (IAT 17–20) |
| Découverte via l'API GitHub publique, sans jeton | ✅ `lister_versions()` |
| Assets lus dans le tableau `assets` (noms réels) | ✅ `analyser_releases()` |
| Téléchargement direct depuis `github.com/.../releases/download/<tag>/` | ✅ `urls_paquet()` + `curl` sans fenêtre |
| Manifeste DU TAG (pas `/latest`) | ✅ `urls_paquet()` |
| SHA-256 vérifié, paquet refusé sinon | ✅ `verifier_et_telecharger()` |
| Extraction `lib/net45/` + `current`, rollback manuel | ✅ `appliquer_paquets()` |
| Relance de la version installée | ✅ `relancer_apres_setup()` |
| Consentement explicite du clic sans toucher `config.json` | ✅ `check_and_update_consenti()` |
| **Liste des versions affichée dans la fenêtre du stub** | ❌ reste : le stub n'a qu'un `DrawTextW` statique. La liste complète vit dans `rose/src/setup_gui.rs` (4 648 l.) et dans `velopack::lister_versions_source()`. |
| **Barre de progression / messages d'erreur dans la fenêtre** | ❌ reste : le vrai statut sort dans la console de `rose.exe` lancée par `ShellExecuteW`. |
| **Attente du fils côté stub** | ❌ reste : le stub garde sa boucle de messages ; il ne fait pas `WaitForSingleObject`. La relance est assurée côté Rose. |
| **Bootstrap si le setup est téléchargé SEUL** | ⚠️ condition : le stub lance `rose.exe` **à côté de lui** ; s'il n'y a ni `rose.exe` voisin ni Rose installé, `ShellExecuteW` échoue silencieusement. Le fallback (extraire le moteur `RT_RCDATA 3` embarqué, ou tenter `%LOCALAPPDATA%\Rose\current\rose.exe`) n'est PAS implémenté — extraction = écriture locale, volontairement exclue par la consigne. |
| **Exécution du PE sur Windows** | ❌ non vérifiable ici (Linux, aucun Wine) : validation statique uniquement (C0, imports, chaînes, `.rsrc`). |

## Checklist de vérification POST-build

⚠️ **Toute régénération du setup invalidate les preuves précédentes.** Ne jamais
déclarer « le setup est bon » en relisant le code source ou un hash ancien :
rejouer la chaîne complète **après** chaque régénération.

1. **Recompiler le stub depuis la source modifiée** (le binaire C0 n'est pas
   une donnée figée) :
   `scripts/build-sans-cargo.sh --windows` (chemin complet), ou directement
   `C0 pe_stub_ressources.rose > pe_stub.elf` puis
   `pe_stub.elf rose-setup.exe setup <N> <args…>` et copie vers
   `Rose-win-Setup.exe`.
   *Note d'environnement* : sous `qemu-aarch64-static`, donner un **nom court**
   à l'ELF généré (`a.elf`) — avec un nom long, le runtime C0 échoue
   (`rc=1`) avant même d'émettre le PE ; ce n'est PAS un défaut du source.
2. **Imports** : `llvm-readobj --coff-imports Rose-win-Setup.exe` doit montrer
   `USER32.DLL`, `KERNEL32.DLL` (dont `GetModuleFileNameW`, `lstrcatW`),
   `SHLWAPI.DLL!PathRemoveFileSpecW` et `SHELL32.DLL!ShellExecuteW`.
3. **Chaînes UTF-16** présentes dans le PE : `update --velopack --setup`,
   `\rose.exe`, `open` (contrôle `python3` + `encode('utf-16le')`).
4. **Désassemblage** : dans `.text`, la suite
   `GetModuleFileNameW → PathRemoveFileSpecW → lstrcatW → ShellExecuteW`
   doit être visible juste après `UpdateWindow`, avec `SW_SHOWNORMAL = 1`
   dans `[rsp+0x28]`.
5. **Ressources** : `python3 installateur/velopack/verifier-res.py
   Rose-win-Setup.exe` → `RESSOURCES VALIDES` (17 ressources, 7 tailles
   d'icônes, manifeste PerMonitorV2). Un .rsrc correct n'implique PAS un PE
   lançable : les étapes 2–4 sont obligatoires.
6. **Artefacts** : bloc « VÉRIFICATION DES 5 ARTEFACTS WINDOWS » du script →
   `5/5 ARTEFACTS PE32+ x86-64 VALIDES`, payload identique à `rose.exe` C0.
7. **Rust** : `cargo check --release --bin rose --manifest-path rose/Cargo.toml`
   → `Finished`, rc 0 ; `cargo test --release --bin rose velopack::tests` →
   31 passés, 0 échec.
8. **Réseau réel (lecture seule)** :
   `cargo test --release rose_update_voit_v0_1_79_reel -- --ignored --nocapture`
   → `ROSE UPDATE VOIT : 0.1.76 → <dernière> … source GitHub (repli)`.
   Vérifier aussi que l'asset `Rose-<v>-full.nupkg` et `releases.win.json`
   existent sur le tag annoncé (`curl -I` sur l'API / le download).
9. **Non-régression moteur** : parité self-host **141/141**, point fixe
   `b3ed9262dd54776a6dbd07bc12368d0e` (263 520 o) inchangé.
10. **Pas de fausse piste** : le stub ne contient aucun appel de copie de
    fichier (`CopyFile*`, `fs::copy` sur des fichiers locaux) ; les seules
    écritures sont celles du paquet **téléchargé** (extraction ZIP + `current`).

## Preuves de cette session (27–28/11/2026)

| Contrôle | Résultat mesuré |
|---|---|
| Compilation C0 du stub modifié | `qemu-aarch64-static C0_141.elf pe_stub_ressources.rose` → rc 0, ELF 103 464 o, stderr 0 |
| Génération PE (17 ressources réelles) | `pe_stub.elf → full_setup.exe setup 17 …` → rc 0, 2 873 856 o, `PE32+ executable (GUI) x86-64, 3 sections` |
| Imports | `llvm-readobj --coff-imports` : `SHLWAPI.DLL!PathRemoveFileSpecW`, `SHELL32.DLL!ShellExecuteW`, `KERNEL32.DLL!{GetModuleFileNameW,lstrcatW}` ✅ |
| Désassemblage `.text` | séquence `GetModuleFileNameW → PathRemoveFileSpecW → lstrcatW → ShellExecuteW` après `UpdateWindow`, `[rsp+0x28] = 1` (SW_SHOWNORMAL) ✅ |
| Chaînes UTF-16 | `update --velopack --setup`, `\rose.exe`, `open` présentes dans le PE ✅ |
| Ressources | `verifier-res.py` → `RESSOURCES VALIDES` (17, 7 icônes, PerMonitorV2) |
| Test de non-régression du garde-fou | `config_persistance_et_effet_au_demarrage` : `check_and_update` bloqué en `desactive`, `check_and_update_consenti` traversé ✅ |
| Parcours hors ligne complet (`--setup`, config `desactive`) | `Rose 9.9.9 installé`, `current = app-9.9.9`, `rose.exe` extrait du nupkg téléchargé ✅ ; contrôle `rose update --velopack` sans `--setup` → refus « DÉSACTIVÉES », `current` absent ✅ |
| API réelle | `ROSE UPDATE VOIT : 0.1.76 → 0.1.86 (release v0.1.86, source GitHub (repli))` ; manifeste + nupkg du tag en `200`, SHA-256 mesuré `b4906c25…c4ca56` = SHA-256 du manifeste ✅ |
| Test unitaire | `velopack::tests` → 31 passés, 0 échec (4 ignorés, dont le test réseau) |
| `cargo check --release --bin rose` | `Finished`, rc 0 |
| Parité self-host | `tests=141 CONFORMES=141 DIVERGENTS=0 BLOQUES=0`, `VERDICT … RC=0` |
| Point fixe | `rose rir compilateur/emetteur.rose` → `b3ed9262dd54776a6dbd07bc12368d0e`, 263 520 o ✅ |

⚠️ Un premier lancement du harnais de parité a auto-découvert un ELF self-hosté
plus ancien (`moteur_fmt2.elf`, compilation antérieure) et a rapporté 28 divergences ; ce
résultat est **écarté** comme faux négatif. La mesure retenue utilise
`PARITE_MOTEUR=/tmp/opencode/moteur_141.elf`, compilé depuis le MÊME composite
suivi `compilateur/moteur_complet.rose` (md5 `154152f78b1d61d80c8a125f7b5b26bb`,
inchangé par ce chantier), avec l'oracle Rust recompilé depuis ce worktree.
