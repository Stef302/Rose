# PUBLICATION_ROSE.md — publier une release depuis Rose

**Date :** 21/11/2026. **Dépôt :** `Stef302/Rose`.
HEAD initial du chantier : `8f91228` ; HEAD courant `8821bdd` (commits
d'agents parallèles, auxquels ce chantier n'a pas participé).
**Fichiers concernés :** `scripts/publier-release.rose` et
`rose/docs/PUBLICATION_ROSE.md`, créés par ce chantier puis versionnés par un
agent parallèle ; l'arbre de travail contient **les versions de ce chantier**.
`scripts/publier-release.py` (outil Python de REL1), `rose/src/**`,
`Cargo.toml`, `PLAN.md` et les autres docs n'ont **pas** été modifiés ici.

> Note d'honnêteté : un autre agent a travaillé en parallèle sur le même
> fichier. Sa première version (569 lignes) passait `verif` mais **échouait à
> l'exécution** (`si ($CONF["dry"])` : type `?` au lieu de `booleen`, ligne 535) ;
> il l'a ensuite corrigée et commitée sous `8a5884e` (578 lignes, dry-run RC=0).
> La version retenue dans l'arbre de travail est celle de **ce** chantier (301
> lignes), parce qu'elle utilise `curl` pour l'étape 1 — consigne explicite —
> alors que la version commitée utilise `gh api user`. Les deux ont été testées ;
> aucune n'a été simulée. Les autres fichiers modifiés simultanément
> (`rose/src/interp.rs`, `rose/src/run.rs`, `vscode-rose/**`, …) appartiennent à
> d'autres chantiers et n'ont pas été touchés ici.

---

## 1. But

Auto-héberger l'outil de publication : piloter les **dix étapes** de
`scripts/publier-release.py` depuis un programme **écrit en Rose**, en passant
par la FFI `@dll`/`@sym` vers libc `system()`.

```
./rose/rose scripts/publier-release.rose <version> "<titre>" <fichier_notes> \
    [--dry-run] [--no-build] [--force]
```

À lancer **depuis la racine du dépôt** (les commandesshell sont relatives).

---

## 2. Ce qui fonctionne réellement (preuves d'exécution)

### 2.1 Vérification statique

```
$ ./rose/rose verif scripts/publier-release.rose
vérifié
RC=0
```

⚠ `verif` **ne détecte pas toutes les erreurs de typage** : une version
concurrent du même fichier passait `verif` mais échouait à l'exécution sur
`si ($CONF["dry"])` (valeur de table de type `?`, pas `booleen`). Le test
d'exécution est donc indispensable — voir 2.4.

### 2.2 Dry-run réel (aucune écriture, aucun appel réseau)

Commande exécutée :

```
./rose/rose scripts/publier-release.rose 0.1.72 "Rose v0.1.72 — test" \
    /tmp/opencode/notes-release-test.md --dry-run
```

Sortie **complète** (RC=0) :

```
╔═══════════════════════════════════════════════════════════╗
║  DRY-RUN — aucune écriture, aucun build, aucune publication ║
╚═══════════════════════════════════════════════════════════╝

=== Étape 1/10 : vérification du compte GitHub ===
  ✓ jeton lu dans /home/clausake/.config/opencode/.gh_token (jamais affiché)
    [SIMULÉ] curl -sS --max-time 20 -H "Authorization: Bearer $(cat /home/clausake/.config/opencode/.gh_token)" -H "Accept: application/vnd.github+json" -H "User-Agent: rose-publier-release" https://api.github.com/user > <capture> ; grep login == Stef302
    dry-run : aucun appel réseau (contrairement au dry-run Python)

=== Étape 2/10 : validation du format de version ===
  ✓ version validée : 0.1.72
  ✓ notes lues : /tmp/opencode/notes-release-test.md

=== Étape 3/10 : mise à jour de Cargo.toml / Cargo.lock + marqueurs ===
    [SIMULÉ] python3 -c 'import sys,datetime as dt,importlib.util; s=importlib.util.spec_from_file_location("pub","scripts/publier-release.py"); m=importlib.util.module_from_spec(s); s.loader.exec_module(m); r=m.Rapport(False); v=m.valider_version(sys.argv[1],r); a=m.version_courante(); n=m.lire_notes(sys.argv[2]); m.maj_cargo(a,v,r,False); m.maj_marqueurs(a,v,r,False)' "0.1.72" "/tmp/opencode/notes-release-test.md"

=== Étape 5/10 : construction des artefacts ===
    [SIMULÉ] sh scripts/build-windows-rebuild.sh
    [SIMULÉ] sh installateur/velopack/pack-linux.sh 0.1.72
    [SIMULÉ] CARGO_TARGET_DIR=rose/target cargo build --release --target wasm32-unknown-unknown --lib --manifest-path rose/Cargo.toml

=== Étape 6/10 : copie des artefacts, SHA256SUMS, manifeste, CHANGELOG/RELEASES ===
    [SIMULÉ] python3 -c 'import sys,datetime as dt,importlib.util; s=importlib.util.spec_from_file_location("pub","scripts/publier-release.py"); m=importlib.util.module_from_spec(s); s.loader.exec_module(m); r=m.Rapport(False); v=m.valider_version(sys.argv[1],r); m.copier_artefacts(v,r,False); n=m.lire_notes(sys.argv[2]); man=m.regenerer_manifeste(v,sys.argv[3],n,dt.date.today(),r,False); m.maj_html(v,sys.argv[3],n,dt.date.today(),man,r,False)' "0.1.72" "/tmp/opencode/notes-release-test.md" "Rose v0.1.72 — test"

=== Étape 9/10 : commit et push sur main ===
    [SIMULÉ] git rev-parse --abbrev-ref HEAD | tr -d '\n' ; vérification branche == main
    [SIMULÉ] git add -- <fichiers de la release>
    [SIMULÉ] git commit -m "chore(release): publier v0.1.72"
    [SIMULÉ] git push origin main

=== Étape 10/10 : création et vérification de la release GitHub ===
    [SIMULÉ] GH_TOKEN=$(cat /home/clausake/.config/opencode/.gh_token) gh release create v0.1.72 --title "Rose v0.1.72 — test" --notes-file "/tmp/opencode/notes-release-test.md" docs/telechargements/Rose-0.1.72-full.nupkg docs/telechargements/releases.win.json docs/telechargements/Rose-win-Setup.exe docs/telechargements/rose.exe docs/telechargements/rose-shell.exe docs/telechargements/rose-wasm.wasm
    [SIMULÉ] GH_TOKEN=$(cat /home/clausake/.config/opencode/.gh_token) gh release view v0.1.72 --json assets --jq '.assets|length' | tr -d '\n'
    [SIMULÉ] GH_TOKEN=$(cat /home/clausake/.config/opencode/.gh_token) gh release download v0.1.72 --pattern 'Rose-0.1.72-full.nupkg' --dir /tmp/opencode/rose-release-ffi --clobber && sha256sum docs/telechargements/Rose-0.1.72-full.nupkg /tmp/opencode/rose-release-ffi/Rose-0.1.72-full.nupkg | awk '{print $1}' | sort -u | wc -l | tr -d ' '  (doit rendre 1)
    vérification API : tag présent, nombre d'assets, SHA-256 du nupkg téléchargé

╔═══════════════════════════════════════════════════════════╗
║  RAPPORT DRY-RUN — simulation terminée (10 étapes)        ║
╚═══════════════════════════════════════════════════════════╝
Aucun fichier du dépôt modifié, aucun build, aucun commit, aucune release.
```

Vérification du jeton : `tok in sortie` = **faux** pour les deux dry-runs
(Rose et Python). Le jeton n'est jamais imprimé.

### 2.3 Étape 1 en mode RÉEL (seule étape distante exécutée pendant ce chantier)

Sur une copie tronquée du programme (arrêt volontaire juste après l'étape 1,
avant toute écriture de release) :

```
=== Étape 1/10 : vérification du compte GitHub ===
  ✓ jeton lu dans /home/clausake/.config/opencode/.gh_token (jamais affiché)
  ✓ compte vérifié : Stef302 (sortie curl capturée, jeton non affiché)
TEST ETAPE 1 TERMINE — arret volontaire avant publication
RC=0
```

Détail mesuré : `rc.txt` = `0`, `user.json` contient `"login":"Stef302"`.
⚠ GitHub renvoie le JSON **sans espace** après les deux-points : le motif
`grep` doit être `'\"login\"[[:space:]]*:[[:space:]]*\"Stef302\"'` ; un motif
avec un espace fixe échoue (bug réel trouvé et corrigé pendant ce chantier).

### 2.4 Chemin `executer` réel et refus sur échec

```
$ echo EXECUTION_REELLE_OK
EXECUTION_REELLE_OK
  ✓ rc=0 : echo EXECUTION_REELLE_OK
TEST SUCCES TERMINE            → RC=0

$ echo NE_DOIT_PAS_APPARAITRE ; false
NE_DOIT_PAS_APPARAITRE
  ✗ échec (rc=1) — journal complet : …
ÉCHEC : TEST: arret sur echec confirme → RC=1
```

Le rapport s'arrête bien sur le premier échec (`quitter(1)`).

### 2.5 Non-régression

- `cargo test --release` dans `rose/` : **529 passed, 0 failed**, 6 ignored
  (7 cibles : 265 + 249 + 0 + 6 + 8 + 1 + 0). Le chiffre **516 attendu
  n'a pas été reproduit** : c'est le constat réel du 21/11, à re-mesurer.
- Point fixe **×2** (binaire `rose/rose` commité) :
  - `./rose/rose rir compilateur/emetteur.rose` → `b3ed9262dd54776a6dbd07bc12368d0e`, 263 520 o ;
  - `./rose/rose compilateur/emetteur.rose compilateur/emetteur.rose` → `b3ed9262dd54776a6dbd07bc12368d0e`.

---

## 3. La limite FFI — exactement

### 3.1 `system()` ne rend qu'un code

Le pont FFI de Rose (`rose/src/interp.rs`, `ffi_appeler`, ~l. 4991-5079)
mappe les types Rose vers des signatures C. Deux faits mesurés dans le code :

1. Pour un **retour `entier`** (l. 5015-5020), TOUS les arguments sont
   convertis par `zarg()` = `vers_z()`. Une chaîne passe donc par
   `conversion vers entier impossible (chaine)` — reproduction réelle :

   ```
   @dll("c") @sym("system")
   fn sh(cmd: chaine) -> entier
   >> sh("echo BONJOUR_SYSTEM")
   Rose : erreur — conversion vers entier impossible (chaine: "echo BONJOUR_SYSTEM")
   ```

2. Le seul cas qui transmet un `char*` est un **retour `flottant`** (l. 5024 :
   `sent(0)` → `extern "C" fn(*const i8) -> f64`).

### 3.2 Contournement utilisé (et pourquoi il est honnête)

```rose
@dll("c") @sym("system")
fn sh(cmd: chaine) -> flottant
```

La commande est bien exécutée (preuve : création de fichier, 2.3). **La valeur
flottante rendue n'est jamais utilisée** : `system()` rend un `int` dans `%eax`,
pas un `double` dans `%xmm0` ; le lire comme un flottant serait du hasard.
Le vrai code de retour et la sortie sont capturés **par le shell lui-même** :

```
{ <commande> ; } > journal-complet.txt 2>&1
printf %s $? > rc.txt
tail -n 25 journal-complet.txt > journal.txt
```

Puis Rose lit `rc.txt` et `journal.txt` avec `lire_texte()`. Ce n'est donc **pas une simulation** : sortie et code de retour sont réels, mais transportés par
fichier. Le fichier `journal-complet.txt` garde toute la sortie en cas d'échec.

### 3.3 Ce que Rose NE fournit PAS aujourd'hui

- pas de `popen()` utilisable : il rend `FILE*` (pointeur) et le moteur n'a pas
  de type pointeur FFI ; le déclarer `-> chaine` ferait lire `%eax` comme une
  adresse (comportement indéfini) ;
- aucun appel système natif « exécuter et capturer » dans la bibliothèque
  standard (`std::process::Command::output()` n'est pas exposé) ;
- `@dll("libc")` est **faux** : le chargeur essaie `liblibc.so`, `libc.so`,
  `libc.so.6`… Il faut `@dll("c")` (le chargeur essaie `libc.so`, `c.so`, `c`,
  `libc.so.6`…). C'est vérifié dans `ffi_poignee` (l. 4972-4977).

### 3.4 Pont natif minimal proposé — **NON IMPLÉMENTÉ ICI** (interdit)

À ajouter plus tard dans `rose/src/**` par un chantier autorisé. Ce serait une
fonction de bibliothèque standard, pas une FFI :

```rust
// esquisse, non appliquée
"systeme" | "executer" => {
    let cmd = chaine_de(&a[0]);
    let out = std::process::Command::new("/bin/sh")
        .arg("-c").arg(cmd).output()
        .map_err(|e| erreur_runtime(&format!("systeme : {e}")))?;
    let t = table();
    t.inserer("rc", Valeur::z(out.status.code().unwrap_or(-1) as i64));
    t.inserer("sortie", Valeur::s(String::from_utf8_lossy(&out.stdout).into_owned()));
    t.inserer("erreur", Valeur::s(String::from_utf8_lossy(&out.stderr).into_owned()));
    t
}
```

Usage rêvé : `let r = systeme("git status") ; afficher(r["sortie"])`. Cela
supprimerait les fichiers temporaires, le `tail` et le contournement `flottant`.
**Ce chantier ne l'a pas fait** : `rose/src/**` était interdit.

---

## 4. Sécurité du jeton

- Lecture par `lire_texte($TOKEN_PATH)` (jamais affichée) ;
- les commandes affichées contiennent `$(cat /home/…/.gh_token)` : le **chemin**
  du jeton apparaît, **jamais sa valeur** ;
- aucun message d'erreur n'imprime la réponse brute de `/user` ; en cas de
  refus, seul le compte attendu `Stef302` est nommé ;
- comparaison programmée : valeur du jeton absente des deux sorties dry-run.

---

## 5. Comparaison honnête avec le dry-run Python

| Point | Python (`publier-release.py`) | Rose (`publier-release.rose`) |
|---|---|---|
| Étapes couvertes | 10 (headers 1…10) | 10, mais groupées : 3+4 et 6+7+8 partagent un appel `python3` |
| Étape 1 | **vrai appel** `/user` même en dry-run | **simulée** (exigence : aucun appel distant en dry-run) |
| Version courante / tailles / présence des artefacts | affichées | non affichées en dry-run (aucune commande locale exécutée) |
| Builds | simulés | simulés (mêmes 3 commandes) |
| Manifest + CHANGELOG/RELEASES | logique Python interne | **importe le module Python existant** et appelle ses fonctions (aucun code dupliqué, fichier non modifié) |
| git / gh | simulés | simulés (mêmes intentions, `--force` simulé aussi) |
| Jeton | jamais affiché | jamais affiché |
| Fichier modifié en dry-run | non | non |

Écart assumé : le dry-run Rose est **plus strict** sur le réseau et **moins
verbeux** sur les changements de fichiers. Il ne prétend pas être une copie
octet pour octet du rapport Python.

---

## 6. Limites connues et conditions d'usage

1. **CWD = racine du dépôt** (les chemins sont relatifs ; pas de `getcwd` FFI).
2. **Titre et chemin de notes sans guillemet double** (interpolés dans les
   commandes shell) ; à durcir avant une utilisation hostile.
3. `--force` suppression avant création : en mode réel, un échec de
   suppression est **averti** puis la création est tentée (la release existante
   fera alors échouer `gh create`) — comportement volontaire, pas un succès
   simulé.
4. `--dry-run` n'appelle aucune commande, y compris locales : la présence des
   artefacts n'est donc pas vérifiée dans ce mode.
5. Le nombre d'assets attendu est **6** (les six de `sources_artefacts`).
6. Dossier de travail FFI : `/tmp/opencode/rose-release-ffi` (doit être
   accessible en écriture en mode réel).
7. La logique de fichiers est déléguée au module Python REL1 : l'outil Rose
   n'est donc **pas indépendant de Python** — il est auto-hébergé dans son
   *pilotage*, pas dans l'écriture des fichiers. C'est un choix explicite
   (réutilisation d'une logique vérifiée) et non une omission.

---

## 7. État final factuel

- **Fait :** `scripts/publier-release.rose` existe (301 lignes), `verif` →
  `vérifié`, dry-run réel → RC=0, étape 1 réelle → compte `Stef302` confirmé,
  refus sur échec → RC=1.
- **Fait :** la sortie de `system()` est récupérée (code + texte) via fichiers ;
  la valeur flottante de la FFI est ignorée.
- **Fait :** aucun commit, aucune release, aucune modification de fichier
  existant appartenant à un autre chantier n'a été effectuée ici.
- **Non fait :** pont natif `systeme()` (interdit), publication réelle
  (interdite), test du chemin complet en mode réel (aurait publié).
- **Mesuré, pas promis :** 529 tests Cargo réussis / 0 échec ; point fixe
  `b3ed9262…` identique sur les deux voies.
