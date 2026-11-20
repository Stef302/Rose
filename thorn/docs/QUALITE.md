# Thorn — infrastructure de qualité et garanties

**Constat daté : 21/11/2026.** Ce document décrit ce que la porte de
non-régression `thorn/tests_regression.sh` et le workflow
`.github/workflows/thorn.yml` prouvent réellement — et ce qu'ils ne prouvent
pas. Il ne promet rien au-delà de ce qui est mesuré.

## 1. Fichiers livrés

| Fichier | Rôle |
|---|---|
| `thorn/tests_regression.sh` | Unique source de vérité. Joue toutes les suites Thorn, vérifie codes de sortie et chaînes, dresse un bilan, rend un code non nul si un seul cas échoue. |
| `.github/workflows/thorn.yml` | Job `ubuntu-latest` : checkout neuf, Rust minimal, `cargo build --release --bin rose`, puis `bash thorn/tests_regression.sh`. Aucun `continue-on-error`, aucun filtre `paths`. |
| `thorn/docs/QUALITE.md` | Ce document. |

Rien d'autre n'est modifié : aucun `rose/src/**`, aucun `thorn/src/**`
existant, aucun test existant, aucun `Cargo.toml`, aucun `PLAN.md`, aucun doc
d'un autre agent. Aucun commit, aucune publication.

## 2. Lancer localement

Depuis n'importe où (le script se place seul à la racine du dépôt) :

```bash
bash thorn/tests_regression.sh
```

Le script réutilise `rose/target/release/rose` s'il est exécutable (smoke
`rose version`), sinon il compile `cargo build --release --bin rose`. Il force
alors `RUSTFLAGS=""` : le `+crt-static` de `rose/.cargo/config.toml` produit
sur x86_64 un static-pie que le chargeur du runner CI ne démarre pas. Sur
cette machine (aarch64) le binaire statique fonctionne et est réutilisé.

Commandes jouées une à une, pour diagnostiquer :

```bash
./rose/rose verif thorn/src/{lexeur,analyse,noyau,tactiques,bibliotheque,principal}.rose
./rose/rose thorn/tests_noyau.rose
./rose/rose thorn/tests_syntaxe.rose
./rose/rose thorn/tests_tactiques.rose
./rose/rose thorn/tests_pilote.rose
./rose/rose thorn/src/bibliotheque.rose
./rose/rose thorn/src/principal.rose --demo
./rose/rose thorn/src/principal.rose --aide
# le contre-exemple est écrit par le script dans un mktemp -d, puis effacé
```

Exigences : `bash`, `cargo`/`rustc` uniquement si le binaire doit être
reconstruit, `grep`/`sed`/`mktemp` (coreutils). Aucune dépendance réseau.

## 3. Périmètre de garantie

### 3.1 Ce que la CI prouve

Depuis un clone neuf, sur `ubuntu-latest` (x86_64) :

1. **Compilabilité** : la chaîne Rust stable `--profile minimal` suffit à
   produire `rose --bin rose`. Un fichier source déclaré manquant casserait
   le build.
2. **Vérification statique des 6 modules** : `rose verif` rend `vérifié` et
   rc=0 pour `lexeur`, `analyse`, `noyau`, `tactiques`, `bibliotheque`,
   `principal`.
3. **Suites Thorn** : `tests_noyau` (verdict `RESULTAT: OK`, `ECHOUES=0`),
   `tests_syntaxe` (`OK = 8   ECHEC = 0`), `tests_tactiques` (`REUSSIS=19`,
   `ECHOUES=0`), `tests_pilote` (`REUSSIS=12`, `ECHOUES=0`) — chacune avec
   rc=0.
4. **Bibliothèque** : les 10 lemmes sont acceptés par le noyau
   (`10 lemme(s) vérifié(s), 0 échec(s)`, `RESULTAT: OK`).
5. **Pilote** : `--demo` valide les 5 preuves du manifeste
   (`5 théorème(s), 5 réussi(s)`, aucune chaîne `ÉCHEC`) ; `--aide` répond.
6. **Anti-preuve-du-faux** : un fichier `.thn` affirmant `0 = 1` avec
   `preuve reflexivite` est **rejeté** (rc=1, `ÉCHEC faux`, `0 réussi(s)`,
   aucune ligne `OK faux`). C'est le test le plus important : si le noyau
   commençait à accepter une preuve fausse, la CI devient rouge.
7. **Honnêteté du harnais** : la liste `PREVUS` est comparée à la liste des
   cas réellement exécutés. Un cas retiré du script, ou un compteur de suite
   tombant sous son plancher, rend la CI rouge. Le bilan affiche le nombre
   de cas exécutés/attendus et le nombre de cas réussis/échoués.

### 3.2 Ce que la CI ne prouve pas

- **Pas de correction de Rose.** Elle prouve que le compilateur Rust se
  compile et exécute ces scripts, pas qu'il est exempt de bug. Les 515 tests
  Rust (`cargo test --release`) sont couverts par `.github/workflows/ci.yml`,
  pas par ce workflow-ci.
- **Pas de solidité méta-théorique de Thorn.** Les axiomes de la §5 sont
  assumés ; la CI vérifie qu'ils sont utilisés, pas qu'ils sont cohérents.
- **Pas d'ι-réduction, pas de contrôle de terminaison ou de positivité** pour
  `nat_rec`. Le noyau vérifie des types d'applications, pas une sémantique
  récursive.
- **Pas d'univers multiples** : tout est à `Type(0)`.
- **Pas de validation des `.thn` hors manifeste** : `--demo` ne joue que les
  5 fichiers énumérés dans `pilote_manifeste()`. Un nouveau fichier de preuve
  n'est pas couvert tant qu'il n'y est pas ajouté.
- **Pas de couverture exhaustive des tactiques** : `symetrie`, `introduction`,
  `application`, `induction` sont exercées par les démos et les tests, mais
  il n'y a ni unification générale, ni recherche automatique.
- **Pas de garantie sur `simplifier`** : sa conversion étendue n'est pas
  vérifiée `convertible` par le noyau.
- **Pas de `--trace`** : l'option plante sur les termes `lam` (§5). Elle
  n'est donc pas testée par la porte, et ne doit pas être présentée comme
  fonctionnelle.
- **Pas de multi-plateforme** : un seul job, `ubuntu-latest`. Les chemins
  Windows/macOS/aarch64 ne sont pas couverts ici.
- **Pas de performance, de mémoire, de déterminisme binaire ou de docs** :
  hors périmètre. Les points fixes du compilateur sont mesurés par
  `scripts/selfhost_total.sh` (ci-dessous), pas par cette CI.

### 3.3 Vérifications hors CI effectuées à la livraison

Constat du 21/11/2026, re-mesurables :

| Contrôle | Résultat |
|---|---|
| `bash -n thorn/tests_regression.sh` | rc=0 |
| `bash thorn/tests_regression.sh` | 17/17 cas verts, `BILAN: OK`, rc=0 |
| Injection d'une régression (motif `RESULTAT: OK` remplacé par un motif faux dans une copie temporaire) | 4 cas rouges, `BILAN: ECHEC`, rc=1 — la porte n'est pas vacue |
| Parse YAML du workflow (`python3 -c 'import yaml…'`) | valide, 1 job, `runs-on: ubuntu-latest` |
| `cargo test --release` (dans `rose/`) | 515 passed, 0 failed, 3 ignored, 0 filtered |
| Point fixe `b3ed9262dd54776a6dbd07bc12368d0e` (`rose rir compilateur/emetteur.rose`, ×2) | md5 identiques ×2, 263 520 o, `cmp` octet-pour-octet OK |

Le point fixe est mesuré sur `compilateur/emetteur.rose` (source non
modifiée) — pas sur `compilateur/emetteur_elf.rose`, qui était modifié dans
l'arbre de travail au moment de la mesure.

## 4. Politique de non-régression

1. **Rouge = bloquant.** Aucun `|| true`, aucun test sauté, aucun
   `continue-on-error`. Le job échoue si et seulement si le script échoue.
2. **Un cas ajouté = une entrée dans `PREVUS`.** Le script compare `FAITS` à
   `PREVUS` : sans cette entrée, un cas qui disparaît ne serait pas signalé.
3. **Chaînes épinglées quand elles sont stables** (`RESULTAT: OK`, `ECHOUES=0`,
   `OK = 8   ECHEC = 0`, `REUSSIS=19`, `REUSSIS=12`, `10 lemme(s)…`,
   `5 théorème(s), 5 réussi(s)`, `vérifié`, `Thorn — pilote de démonstration…`).
   Une modification de l'une d'elles doit être deliberate et expliquée.
4. **Plancher pour la suite qui grandit.** `tests_noyau` est vérifié avec un
   plancher de 35 cas (valeur livrée initialement) et non un compte exact :
   au 21/11 la suite est passée de 35 à 87 puis 88 cas au fil des ajouts
   `iota`/`nat_rec` d'un autre chantier concurrent. Le nombre **observé** est
   affiché dans le bilan. Baisser
   sous 35 = régression. Les autres suites restent épinglées.
5. **Le contre-exemple doit continuer à échouer.** Son libellé de raison
   dépend du noyau (il a déjà changé) ; on n'épingle donc que le verdict
   `ÉCHEC faux` et le bilan `0 réussi(s)`. Toute modification qui ferait
   accepter `0 = 1` casse la porte (vérifié : rc=1 aujourd'hui).
6. **Un changement de types/API Thorn doit être accompagné** d'une mise à jour
   du script et de ce document dans le même lot.

## 5. Limites connues (documentées et mesurées)

1. **`--trace` est cassé sur les termes `lam`.** Mesuré le 21/11 sur les 5
   preuves du manifeste :

   ```
   $ ./rose/rose thorn/src/principal.rose thorn/preuves/identite.thn --trace
   Rose : erreur — clé absente de la table
   $ echo $?
   1
   ```

   Idem pour `reflexivite`, `symetrie`, `associativite`, `commutativite`.
   Sans `--trace`, les mêmes fichiers rendent `OK …` et rc=0. Les exemples
   de trace dans `thorn/docs/EXEMPLE_CONCRET.md` décrivent un état antérieur
   et ne sont pas reproductibles sur le binaire courant.

2. **Axiomes explicites, non démontrés** (`thorn/docs/TACTIQUES.md` §3) :
   `Nat`, `zero`, `succ`, `plus`, `Eq`, `refl`, `eq_rec`, `nat_rec`,
   `plus_0_l : Πn, Eq (plus zero n) n`,
   `plus_s_l : Πm n, Eq (plus (succ m) n) (succ (plus m n))`.
   Ce sont des constantes de signature assumées. En revanche `symm`, `trans`,
   `congr_succ`, `congr_plus_l`, `plus_zero_d`, `plus_succ_d` sont des termes
   dérivés construits puis vérifiés par le noyau.

3. **Pas de ι-réduction** : `beta`/`convertible` ne font que β + α. Les
   équations de `plus` sont donc des axiomes, pas des calculs. `simplifier`
   applique une passe locale (carburant 200) dont le résultat n'est pas
   garanti convertible au terme initial pour le noyau.

4. **Univers unique** : tout est à `Type(0)`. Pas de polymorphisme
   d'univers, pas de cumulativité (`Type(0)` n'est pas sous-type de
   `Type(1)`), pas d'η.

5. **Formation de `Eq` sur-permissive** : le noyau vérifie que les paramètres
   synthétisent un type, mais pas que les deux membres d'un `Eq A a b` ont
   effectivement le type `A`.

6. **Induction simplifiée** : `nat_rec` encode le principe d'élimination
   dépendant et ses applications sont typées, mais il n'y a aucun contrôle
   de terminaison, de positivité ou de cohérence des constructeurs.

7. **Recherche limitée** : `symetrie` exige une hypothèse de type exact
   (modulo β/α) ; `application` essaye une correspondance exacte puis 1 ou 2
   applications ; ni unification générale, ni synthèse d'arguments.

## 6. Honnêteté

Les nombres de la §3.3 sont des **constats datés**, pas des promesses. Après
toute modification de `rose/src/**`, de `thorn/src/**`, des suites de tests ou
de la topologie de compilation, re-mesurer : les relire ici sans les rejouer
serait faux.
