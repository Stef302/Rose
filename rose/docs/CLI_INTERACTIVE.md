# CLI interactive — choisir sa méthode au lieu de deviner les commandes

État : **21/11/2026**. Dépôt `Stef302/Rose`. **Aucun commit, aucune publication.**

Fichiers modifiés : `rose/src/main.rs`, `rose/src/repl.rs`,
`rose/tests/grands_nombres.rs` (une seule ligne : l'invite du REPL a changé
de `rose> ` à `rose[jit]> `). Nouveau document : celui-ci.

---

## 1. Le problème

L'utilisateur disait : *« faut clarifier les commandes car je ne comprends pas
les diff entre run comp etc, tu dois faire ça de façon interactive avec la
méthode que je veux »*.

Avant, il fallait déjà connaître le vocabulaire (`run`, `comp`, `aot`, `orc`,
`bc`, `rvm`, `transpile`) pour choisir. Le défaut (`rose prog.rose`) faisait
du JIT sans le dire. Maintenant **la question est posée**, en français, et
chaque choix est explicitement relié à une commande déjà présente dans le
moteur (rien n'a été réimplémenté).

---

## 2. `rose methode` — le menu (sortie réelle)

```
$ echo '2' | rose methode prog.rose
Comment voulez-vous exécuter votre programme ?
  1) Interprété       — démarrage immédiat, RIR mis en cache
  2) JIT              — compile à chaud les boucles (rapide après warmup)
  3) AOT natif        — binaire natif compilé à l'avance (le plus rapide)
  4) Exécutable autonome — moteur+source dans un .exe (universel, ~5 Mo)
  5) Transpiler       — générer du JS / Python / C lisible
  6) Bytecode RVM     — .rosebc exécuté dans la JVM Rose
Votre choix [1-6] : (méthode choisie : JIT)
bonjour depuis Rose
```

`rose --methode` est un synonyme. Si le fichier n'est pas fourni en argument,
le menu le demande après le choix. Une entrée vide (pipe épuisé) choisit le
défaut JIT sans boucler ; un numéro invalide relance la question.

### Correspondance des six choix

| Choix | Commande équivalente | Ce qui sort |
|---|---|---|
| 1 Interprété | `rose interprete prog.rose` | résultat à l'écran, sans accélération |
| 2 JIT | `rose jit prog.rose` (= défaut) | résultat à l'écran, accélération des boucles |
| 3 AOT natif | `rose aot prog.rose -o sortie` | binaire natif ; **refuse** si non traduisible |
| 4 Exécutable autonome | `rose comp prog.rose -o sortie` | exécutable moteur + source (universel) |
| 5 Transpiler | `rose transpile --cible js\|py\|c` | fichier lisible `.js` / `.py` / `.c` |
| 6 Bytecode RVM | `rose bc` puis `rose rvm` | `.rosebc` exécuté dans la machine virtuelle |

Les choix 3 et 6 ont des limites réelles et le disent : un programme avec
`afficher(...)` n'est pas accepté par le bytecode sandboxé, et un programme
qui ne rend pas de valeur calculable n'est pas accepté en AOT. Dans ces cas
Rose refuse explicitement et n'écrit aucun faux binaire — c'est le comportement
déjà documenté, pas une régression.

---

## 3. Lignes d'aide claires + `rose help <commande>`

L'aide générale donne une ligne par commande, en français simple, avec sa
limite :

```
rose methode [prog.rose]       menu en français : choisissez COMMENT exécuter votre programme
rose help <commande>           explication détaillée et un exemple pour une commande
rose prog.rose [args…]         exécute votre programme (mode accéléré habituel)
rose run prog.rose [args…]     idem, avec le mot « run » ; interpréteur et accélération
rose jit prog.rose [args…]     Choix explicite : accélère les boucles après un court échauffement
rose interprete prog.rose      Choix explicite : démarre tout de suite, sans accélération
rose comp prog.rose [-o sortie]   exécutable autonome prêt à l'emploi (moteur et source réunis) ; suffixe .exe ajouté sur Windows
rose aot prog.rose [-o sortie] binaire natif le plus rapide, compilé à l'avance ; refuse les programmes trop complexes
rose transpile prog.rose --cible js|py|c [-o sortie]   traduit en JavaScript, Python ou C lisible ; produit du texte, ne lance rien
rose bc prog.rose [-o sortie]   écrit un fichier de bytecode .rosebc ; ne l'exécute pas
rose rvm fichier.rosebc [classe] [methode] [args…]   exécute un .rosebc dans la machine virtuelle Rose
```

`rose help <commande>` donne trois paragraphes : **Ce que ça fait**, **Limite**,
**Exemple**. Sortie réelle pour `rose help jit` :

```
rose jit prog.rose [args…]

Ce que ça fait : exécute votre programme en accélérant les boucles pendant
le déroulement. Les premières fois sont un peu lentes (échauffement), puis
ça va très vite. C'est identique au défaut de « rose prog.rose », mais vous
le choisissez explicitement.

Limite : l'accélération n'a pas le temps d'agir sur un programme très court
; dans ce cas « rose interprete » démarre plus vite. Aucun fichier n'est
produit.

Exemple :
  rose jit exemples/fibonacci.rose
```

Neuf commandes sont documentées finement : `methode`, `run`, `jit`,
`interprete`, `comp`, `aot`, `transpile`, `bc`, `rvm` (plus `latex`, `llvm`,
`console`). Un test unitaire vérifie que chacune contient bien les trois
paragraphes et aucun nom d'outil externe.

### Alias explicite

`rose jit prog.rose` est le nouveau nom explicite du défaut. `rose interprete
prog.rose` est son contraire (variable interne `ROSE_SANS_JIT`, déjà lue par
le moteur). Les deux passent par exactement le même bloc d'exécution que
`rose run` : seule la façon de le demander change, pas le résultat.

---

## 4. REPL — `:methode jit|aot|interprete`

La méthode courante s'affiche dans l'invite. Sortie réelle :

```
Tapez du code Rose. :help pour l'aide, :quit pour sortir.
Méthode courante : jit — changez avec :methode aot | interprete.
↑/↓ historique · Tab complète · Ctrl-C annule · Ctrl-L efface
rose[jit]> :methode
(méthode courante : jit)
Choix : :methode jit | aot | interprete
rose[jit]> :methode aot
(méthode : aot — invitation mise à jour)
rose[aot]> :methode jit
(méthode : jit — invitation mise à jour)
rose[jit]> :methode interprete
(méthode : interprete — invitation mise à jour)
rose[interprete]>
```

`:methode` sans argument affiche la méthode courante. Les accents sont
acceptés (`:methode interprété`). Un choix inconnu rend un message d'erreur
sans rien changer. `:method` est un alias, et `:methode` est proposé par la
complétion Tab et par `:help`.

Côté moteur : `jit` retire `ROSE_SANS_JIT` et `ROSE_AOT`, `interprete` pose
`ROSE_SANS_JIT`, `aot` pose `ROSE_AOT=1`. Ces variables sont relues à chaque
exécution : le changement prend effet immédiatement, sans redémarrer la
console.

---

## 5. Vérifications réelles (21/11/2026)

| Contrôle | Résultat mesuré |
|---|---|
| `cargo build --release --bin rose` | **OK** (`Finished release profile`) |
| `cargo test --release` | **0 échec** — 297 + 310 + 6 + 9 + 8 + 1 tests OK, 0 failed, 3 ignored par suite |
| `echo '2' \| rose methode prog.rose` | menu affiché, `(méthode choisie : JIT)`, `bonjour depuis Rose`, **rc=0** |
| Point fixe voie 1 : `rose rir compilateur/emetteur.rose` | `b3ed9262dd54776a6dbd07bc12368d0e` (263 520 octets) |
| Point fixe voie 2 : `rose compilateur/emetteur.rose compilateur/emetteur.rose` | `b3ed9262dd54776a6dbd07bc12368d0e` (263 520 octets), **identique ×2** |
| Les 6 choix du menu | testés un par un : 1/2 rendent le résultat, 3 produit un ELF natif sur un programme calculable, 4 produit l'exécutable autonome, 5 produit `prog.js`, 6 produit le `.rosebc` et l'exécute |

Note sur le point fixe : la valeur `b3ed9262…` est bien celle mesurée ce jour
sur les deux voies. `rose/docs/POINT_FIXE_2026_11_19.md` cite une valeur plus
récente (`8ac53eed…`) datée du 19/11 : conformément à la règle du dépôt, un
constat est daté et se re-mesure après tout changement de
`compilateur/emetteur.rose`. La mesure ci-dessus est celle du binaire
recompilé pendant cette tâche.

Un test d'intégration (`tests/grands_nombres.rs`) retirait le préfixe `rose> `
des lignes du REPL ; il accepte désormais aussi `rose[jit]> `,
`rose[aot]> ` et `rose[interprete]> `. Aucun autre test ne dépendait de
l'ancienne invite.

---

## 6. Ce qui n'a PAS changé

- aucune modification de `interp.rs`, `jit.rs`, `cmd_llvm.rs`, `transpile.rs`
  ni de `compilateur/emetteur.rose` ;
- les commandes historiques gardent leur sémantique (`aot` = `orc`, `bc`
  n'exécute pas, `rvm` exécute, `comp` n'est pas un AOT) ;
- la bannière, `rose --version` et `rose info` restent neutres (aucun nom
  d'outil de construction).
