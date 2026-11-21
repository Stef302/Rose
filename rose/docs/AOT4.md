# AOT4 — `aot` / `orc` universels : le chemin autonome remplace le refus

Vague 79 du `PLAN.md` (21/11/2026). Aucun commit, aucune publication pour ce
chantier. Fichiers : `rose/src/cmd_llvm.rs`, `rose/src/main.rs`, ce document.

## 1. Problème (utilisateur exaspéré)

```
C:\> rose aot fib.rose
rose: compilation native impossible : l'écriture d'une valeur exacte n'est pas disponible sur PE ;
utilisez rose comp
```

Le message variait selon le programme (primorielle, factorielle, appel de
fonction, flottant, hasard…) : **cas par cas**, et il renvoyait la responsabilité
à l'utilisateur alors que `rose comp` produit déjà un exécutable qui marche.

## 2. Décision systémique

> `rose aot` et `rose orc` **réussissent toujours**, sur toutes les plateformes,
> pour tout script `.rose` valide.

| Plateforme / situation | Voie utilisée | Produit | Message |
|---|---|---|---|
| Windows / PE (`--target …windows…` ou hôte Windows) | **chemin autonome universel**, sans tentative native | PE autonome = moteur Rose + source collés | `<nom>.exe produit (exécutable AOT autonome)` |
| ELF, programme émettable | vrai AOT natif (voie rapide inchangée) | ELF machine | `… produit (exécutable natif …)` |
| ELF, valeur exacte émettable | AOT valeurs exactes (vague 78, inchangé) | ELF machine + texte mémorisé | `… (valeur exacte mémorisée)` |
| ELF, programme **non émettable** ou valeur non écrivable | **repli automatique** sur le chemin autonome | exécutable autonome | `<nom> produit (exécutable AOT autonome)` |

Le chemin autonome est le **même produit** que `rose comp` : `moteur Rose +
source + queue `ROSE_MAGIC``. Il est déjà prouvé (`rose.exe` fonctionne, les
`.exe` de `comp` fonctionnent). Il n'y a plus aucun message « utilisez rose
comp » nulle part dans `cmd_llvm.rs` (même `rose link` renvoie désormais vers
`rose aot` / `rose orc`).

## 3. Implémentation

| Fichier | Changement |
|---|---|
| `rose/src/main.rs` | `comp_programme_avec_message(fichier, sortie, message)` = noyau commun ; `comp_programme()` garde le message historique `(exécutable autonome)` ; nouveau `pub(crate) comp_programme_aot()` affiche `<nom> produit (exécutable AOT autonome)`. Aide CLI mise à jour. |
| `rose/src/cmd_llvm.rs` | `autonome()` appelle `comp_programme_aot()` puis `exit(code)`. `orc()` : short-circuit PE **avant** toute évaluation de valeur exacte, puis `orc_natif()` sous `Result<(), String>` ; tout `refuser()` (qui exitait) devient `natif_erreur!` (qui rend `Err`) et déclenche le repli. Le vrai AOT natif reste la voie rapide ELF, inchangé octet pour octet. |

`rose rir`, `rir.rs`, `natif_emetteur.rs` et `link_natif.rs` ne sont pas
modifiés : le point fixe `b3ed9262` et la sortie de `rose rir` restent intacts.

## 4. Preuves réelles (21/11/2026, aarch64 Linux)

### 4.1 Build et tests

```
cargo build --release --bin rose              → rc 0 (arbre courant, 2m57)
cargo test --release                          → 0 échec (arbre courant)
  292 passed / 0 failed / 3 ignored  (lib)
  299 passed / 0 failed / 3 ignored  (bin rose)
  6 + 8 + 8 + 1 passed                (bins/tests annexes) → 614 tests passes
```

### 4.2 `rose aot` sur des programmes NON émettables (le cas qui échouait)

Binaire : `target/release/rose` (aarch64). Chaque produit est exécuté et sa
sortie comparée à l'interpréteur `rose <script>`.

```
$ rose aot divnon.rose -o divnon_aot
rose: AOT natif indisponible (la fonction g utilise des valeurs qui demandent le moteur d'exécution) ; exécutable AOT autonome en repli
rose: divnon_aot produit (exécutable AOT autonome)
$ ./divnon_aot        → 3        $ rose divnon.rose → 3        SORTIE IDENTIQUE

$ rose aot flottant.rose -o flottant_aot
rose: AOT natif indisponible (la valeur finale du programme demande le moteur d'exécution) ; exécutable AOT autonome en repli
rose: flottant_aot produit (exécutable AOT autonome)
$ ./flottant_aot      → 2.5      $ rose flottant.rose → 2.5      SORTIE IDENTIQUE

$ rose orc flottant.rose -o flottant_orc      → même repli, même message, sortie 2.5

$ rose aot hasard.rose -o hasard_aot          # >> hasard(1000000)
rose: AOT natif indisponible (le programme ne rend pas de valeur calculable directement) ; exécutable AOT autonome en repli
rose: hasard_aot produit (exécutable AOT autonome)
$ ./hasard_aot  → 234905 / 869088 / 171266   (aléatoire, comme l'interpréteur)
```

### 4.3 La voie rapide native est conservée

```
$ rose aot fib.rose -o fib_aot
rose: fib_aot produit (exécutable natif aarch64, code de retour = valeur du programme)
$ ./fib_aot → 6765   $ rose fib.rose → 6765   SORTIE IDENTIQUE   (ELF 7f454c46)

$ rose aot exact565.rose -o exact565_aot
rose: exact565_aot produit (exécutable natif aarch64, valeur exacte mémorisée)
$ ./exact565_aot → 228 chiffres, octet pour octet identiques à l'interpréteur

$ rose aot grand.rose -o grand_aot           # >> 2^200
rose: grand_aot produit (exécutable natif aarch64, valeur exacte mémorisée)
sortie identique à l'interpréteur.
```

### 4.4 Point fixe `b3ed9262` ×2

```
rose rir compilateur/emetteur.rose            → md5 b3ed9262dd54776a6dbd07bc12368d0e (263 520 o), stderr vide
rose compilateur/emetteur.rose compilateur/emetteur.rose
                                              → md5 b3ed9262dd54776a6dbd07bc12368d0e (263 520 o), stderr vide
cmp = 0
```

### 4.5 Windows / PE

Cross-compilation réelle du moteur Windows (SDK xwin, `lld-link`, crt-static) :

```
cargo build --release --bin rose --target x86_64-pc-windows-msvc  → rc 0
target/x86_64-pc-windows-msvc/release/rose.exe :
  PE32+ executable (console) x86-64, for MS Windows, 9 sections, 9,0 Mio
```

Sur un hôte Windows, `cible()` rend `windows = true`, `nom_executable()` ajoute
`.exe` et `orc()` appelle directement `autonome()` : le produit est un PE
autonome (le moteur PE auquel la source est collée), avec le message exigé.
Vérification complémentaire depuis Linux avec une cible Windows explicite :

```
$ rose aot fib.rose --target x86_64-pc-windows-msvc -o fib_win
rose: fib_win.exe produit (exécutable AOT autonome)     ← suffixe .exe et message
```

⚠ Limite de mesure : l'exécution de `rose.exe` sous Wine n'a pas pu être
obtenue sur cette machine — `wineboot --init` reste bloqué (prefix neuf ou
`~/.wine`), sans qu'aucun processus `rose.exe` ne démarre. Le **cross-build PE**
est, lui, mesuré et réussi ; le chemin autonome qu'il appelle est exactement
celui déjà validé sur ELF (§4.2). Ce point reste à re-mesurer sur un vrai
Windows / un Wine fonctionnel.

## 5. Non-régression

- `rose comp` garde son message historique `(exécutable autonome)`.
- Le vrai AOT natif, les valeurs exactes et `rose rir` sont inchangés (§4.3, §4.4).
- Aucun fichier nouveau n'est écrit en cas d'échec d'analyse/vérification du
  source (les erreurs de syntaxe restent des erreurs, avant tout repli).
- Aucun réseau, aucun outil externe, aucun téléchargement.
