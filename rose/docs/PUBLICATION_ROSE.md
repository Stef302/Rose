# Publication Rose — automatisateur écrit en Rose

## 1. Fichiers et interface

- `scripts/publier-release.rose` (nouveau) : automatisateur des 10 étapes de
  publication, écrit en Rose.
- `rose/docs/PUBLICATION_ROSE.md` (ce document) : limites FFI et pont natif
  minimal proposé.

Aucun autre fichier du dépôt n'a été modifié par ce chantier. Aucun commit,
aucune publication réelle, aucune modification de `rose/src/**`.

```
./rose/rose scripts/publier-release.rose <version> "<titre>" <fichier_notes> [--dry-run]
```

Exemple réellement exécuté :

```
./rose/rose scripts/publier-release.rose 0.1.72 "Rose v0.1.72 — correctifs FFI" /tmp/opencode/notes-release.md --dry-run
```

`<fichier_notes>` est un fichier de puces (`-`, `*`, `•` ou `1.`), comme dans
`scripts/publier-release.py`.

### Les 10 étapes

| # | Étape | Moyen utilisé depuis Rose |
|---|-------|---------------------------|
| 1 | Compte GitHub `Stef302` | `gh api user` via FFI, PAT lu dans `~/.config/opencode/.gh_token` par `$(cat …)` (jamais affiché) |
| 2 | Validation `x.y.z` / `x.y.zz` | pur Rose (`valider_version`) |
| 3 | Bump `Cargo.toml` / `Cargo.lock` | `sed -i` via FFI |
| 4 | Marqueurs README / index.html / wiki | `sed -i` via FFI |
| 5 | Builds Windows, Velopack, WASM | `sh` / `cargo` via FFI |
| 6 | Copie artefacts, `VERSION`, `SHA256SUMS` | `cp`, `find`, `sha256sum` via FFI |
| 7 | `releases.json` | pont Python (voir §5) via FFI |
| 8 | CHANGELOG + RELEASES dans `docs/index.html` | pont Python via FFI |
| 9 | `git add` / `commit` / `push`, contrôle branche `main` | `git` via FFI |
| 10 | `gh release create`, vérification tag/assets, SHA-256 du nupkg | `gh`, `sha256sum` via FFI |

En `--dry-run` : **aucune commande distante, aucun fichier du dépôt modifié,
aucun build**. Les 28 commandes et 10 changements sont affichés `[SIMULÉ]` ;
le dossier scratch `/tmp/opencode/rose_release` n'est même pas créé.

## 2. Ce qui a été vérifié réellement

1. `./rose/rose verif scripts/publier-release.rose` → **`vérifié`**, code 0.
2. Le dry-run complet ci-dessus → **code 0**, rapport final :
   `Changements simulés : 10`, `Commandes simulées : 28`,
   `Aucun fichier du dépôt modifié, aucun commit, aucune release créée.`
3. `git status --short` avant/après : seul `scripts/publier-release.rose` a
   été ajouté par ce chantier (les autres entrées étaient déjà présentes).
4. Sonde FFI isolée (sans publication) : `sh` lancé via
   `appeler_sh()` avec redirection vers un script temporaire a rendu les codes
   **0 / 7 / 0**, la sortie capturée (`SORTIE-CAPTUREE`, `ERREUR-CAPTUREE`) et
   `git rev-parse --abbrev-ref HEAD` → `main`.
5. Les appels `lire_texte`, `ecrire_texte`, `sous_chaine`, `trouver`,
   `remplacer`, tables mutables partagées par les fonctions, `argument()` /
   `nb_arguments()` et `quitter()` ont été testés séparément avec le moteur.

**Non exécuté volontairement** : le chemin réel (sans `--dry-run`). Aucune
publication, aucun commit, aucun push n'a été effectué. Le chemin réel est
écrit et vérifié syntaxiquement, mais pas éprouvé en conditions réelles.

## 3. La limite FFI — precise

### 3.1 `@dll("libc")` ne charge pas libc sur cette machine

`ffi_poignee()` (`rose/src/interp.rs`) essaie, pour un nom `N` sans `/` :
`libN.so`, `N.so`, `N`, `libN.so.6`, `libN.so.7`, `libN.so.8`.

- `@dll("libc")` cherche `liblibc.so`, `libc.so`, `libc`, `liblibc.so.6…` →
  **échec** (« bibliothèque introuvable (libc) »). Le fichier réel est
  `libc.so.6`.
- La forme littérale suggérée `@dll("libc", "system")` est **aussi fausse** :
  le deuxième chaîne est `dll_son` (« nom de fichier »), donc le chargeur
  fait `dlopen("system")` → échec.
- La forme qui marche est donc :

```rose
@dll("c")
@sym("system")
fn appeler_sh(cmd: chaine) -> flottant
```

### 3.2 Un retour `entier` avec un argument `chaine` est impossible aujourd'hui

`ffi_appeler()` convertit une chaîne en `char*` (`sent(i)`) **uniquement**
dans les branches de retour `Flottant` et `Chaine`. La branche `Entier` fait
`f(zarg(0))` sans tester `sent(0)` : elle essaie de convertir la chaîne en
entier. Mesuré :

```
@dll("c") @sym("system")
fn systeme(cmd: chaine) -> entier
>> systeme("true")
Rose : erreur — conversion vers entier impossible (chaine: "true")
```

Il est donc **impossible** d'écrire `fn systeme(cmd: chaine) -> entier` et
de lire `rc == 0` directement. Le script déclare un retour `flottant`
(l'argument `chaine` est alors bien passé en `char*`), **ignore** cette
valeur (XMM0 n'a aucun rapport avec le `int` rendu par `system()`), et
récupère le vrai statut par redirection shell.

### 3.3 `system()` ne rend pas la sortie

`system()` ne rend qu'un statut d'attente encodé (`0`, `256`, `512`, …).
Ni stdout ni stderr ne sont exposés comme valeur Rose. La sortie est
**indirectement** capturable en la redirigeant vers un fichier lu avec
`lire_texte` — c'est ce que fait `executer_reel()` :

```rose
$script = "#!/bin/sh\n( " + cmd + " ) > " + $LOG_FICHIER +
          " 2>&1\nprintf %s $? > " + $RC_FICHIER + "\n"
ecrire_texte($SH_FICHIER, $script)
appeler_sh("sh " + $SH_FICHIER)
$rc = nettoyer(lire_texte($RC_FICHIER))
```

Le code de retour est `"0"` en succès ; toute autre valeur déclenche
`echouer()` après affichage du log.

### 3.4 Conséquence sur l'interface demandée

L'interface `./rose/rose scripts/publier-release.rose … [--dry-run]` est
respectée. Le message « `@dll("libc","system")` » est interprété comme
« appeler `system()` de libc via `@dll`/`@sym` » : la forme exacte utilisée
est `@dll("c") @sym("system")`, seule forme qui charge réellement libc ici
(§3.1). Aucune modification de `rose/src/**` n'a été faite.

## 4. Pont natif minimal proposé (NON implémenté)

Deux correctifs possibles, dans `rose/src/**` — **non appliqués ici**.

### Option A — correction ciblée de la FFI existante (la plus petite)

Dans `ffi_appeler()`, branche `K::Entier`, cas `1` argument, honorer `sent(0)`
comme le font déjà les branches `Flottant` et `Chaine` :

```rust
1 => {
    if sent(0) {
        let f: extern "C" fn(*const i8) -> i64 = unsafe { std::mem::transmute(p) };
        let c = std::ffi::CString::new(chaine_de(&args[0])).unwrap();
        Valeur::z(f(c.as_ptr() as *const i8) & 0xffff_ffff) // int C → bas 32 bits
    } else {
        let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(p) };
        Valeur::z(f(zarg(0)))
    }
}
```

Le masquage sur 32 bits évite de lire les bits hauts non définis de RAX
quand le C ne rend qu'un `int`. Après cela,
`fn systeme(cmd: chaine) -> entier` fonctionne et le script peut lire le
code directement (la récupération de la sortie par fichier reste utile).

### Option B — builtin dédié, sans FFI et sans guessing

Ajouter une primitive typée, par exemple :

```
r = systeme("git status --short")   # table : r["code"] (entier), r["sortie"] (chaine)
```

Implémentation Rust minimale : `std::process::Command::new("sh").arg("-c")`
avec `.output()` (capture stdout+stderr), retour d'une `Valeur::Table` avec
`code = status.code().unwrap_or(-1)` et `sortie = String::from_utf8_lossy`.
Cela supprime le script temporaire, les redirections et le bricolage de
code de retour, et rend la capture de sortie explicite.

### Utiles aussi (pour retirer le pont Python décrit en §5)

- `fichier_existe(chemin) -> booleen` (utilisé pour les sondes PRESENT/ABSENT
  en dry-run) ;
- `sha256(chemin) -> chaine`, `date_iso() -> chaine`, `taille(chemin) -> entier` ;
- un minimum JSON (`json_lire` / `json_ecrire`) ou une primitive
  `maj_manifeste_release(...)`.

Aucune de ces pistes n'a été touchée dans ce chantier.

## 5. Limites fonctionnelles connues de l'automatisateur

1. **Étapes 7 et 8 déléguées**. Rose n'a ni JSON, ni date, ni HTML escaping.
   En mode réel, le script génère dans `/tmp/opencode/rose_release/pont.py`
   un petit adaptateur qui importe le module existant
   `scripts/publier-release.py` et appelle `regenerer_manifeste()` puis
   `maj_html()`. L'orchestration, les contrôles et le dry-run restent en
   Rose ; ces deux transformations textuelles sont les seules déléguées.
2. **Chemins codés en dur** pour cette machine (`/home/clausake/Stefan/ROSE`,
   `/home/clausake/.config/opencode/.gh_token`). Aucun `getenv()`/`home()`
   n'est exposé au langage. À paramétrer avant un usage ailleurs.
3. **Dry-run sans sonde disque** : faute de `fichier_existe()`, la liste des
   artefacts est affichée comme « attendus » et non PAS/ABSENT comme dans le
   script Python. Aucun fichier n'est lu/modifié pour ces sondes.
4. **Marqueurs (étape 4)** : `sed` applique un remplacement textuel de la
   version avant la section `id="page-changelog"` ; c'est une approximation
   des expressions régulières bornées du script Python. Le pont Python
   (étape 8) réécrit ensuite correctement les sections CHANGELOG/RELEASES.
5. **Interface réduite** : ni `--no-build`, ni `--force` (contrairement au
   script Python). Une release existante fait échouer l'étape 10.
6. **Ordre des étapes** : l'étape 10 n'ajoute qu'un seul asset (le nupkg)
   à la release GitHub ; le script Python passe tous les assets du manifeste.
   À étendre si nécessaire.
7. **Le chemin réel n'a pas été exécuté** (§2) : aucun test de publication
   n'a été fait, conformément à la consigne.

## 6. Commandes de référence

```bash
cd /home/clausake/Stefan/ROSE

# Vérification statique
./rose/rose verif scripts/publier-release.rose          # → vérifié

# Dry-run (aucun effet de bord)
./rose/rose scripts/publier-release.rose 0.1.72 \
  "Rose v0.1.72 — correctifs FFI" /tmp/opencode/notes-release.md --dry-run

# Vérification de la non-modification du dépôt
git status --short
```
