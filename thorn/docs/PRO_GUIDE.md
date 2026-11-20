# Thorn — guide professionnel : syntaxe, tactiques, noyau

**Constat daté : 20/11/2026.** Toutes les commandes de ce document ont été
**réellement exécutées** depuis la racine `/home/clausake/Stefan/ROSE` avec le
binaire du dépôt `./rose/rose` (`Rose 0.1.6 — compilateur Rust + JIT
Cranelift`, relevé par `./rose/rose version`). Les sorties sont collées telles
quelles. Ce document est un **constat**, pas une promesse : re-mesurer après
toute modification de `rose/src/**` ou `thorn/src/**`.

Périmètre : référence complète de la syntaxe, des tactiques et du noyau, plus
**dix exemples commentés, exécutés**, dont la sortie réelle est reproduite
§7. Les limites et la comparaison avec Lean sont dans `PRO_LIMITES.md` ; la
méthode de rédaction de preuves, le débogage et l'extension de la bibliothèque
sont dans `PRO_RECETTES.md`.

---

## 1. Démarrage en trente secondes

```bash
cd /home/clausake/Stefan/ROSE
./rose/rose version
./rose/rose verif thorn/src/noyau.rose          # → vérifié
./rose/rose thorn/src/principal.rose --demo     # → 5 théorème(s), 5 réussi(s)
```

Utiliser **`./rose/rose`** (0.1.6, dans le dépôt). Le `rose` éventuellement
installé dans le `PATH` (0.1.5) ne connaît pas la commande `verif`.

**Règles d'or pour tout script Rose appelant Thorn :**

1. Préfixer les globales du programme importateur (`$ex_...`, `$nx_...`,
   `$ta_...`). Rose infère le type des variables **par nom, tous scopes
   confondus** : une globale `$r` ou `$but` entre en collision avec les locaux
   des modules importés et produit des erreurs du type
   `Ordre entier > complexe impossible`. Les modules Thorn appliquent déjà
   cette règle (`$nx_` dans `noyau.rose`, `$ta_` dans `tactiques.rose`,
   `$bl_` dans `bibliotheque.rose`, `$al_` dans `algebre.rose`, `$p_` dans
   `principal.rose`).
2. Importer ce dont on a besoin : `noyau.rose` (noyau seul), `analyse.rose`
   (parseur ; il importe lui-même `lexeur.rose`), `tactiques.rose` (importe
   `noyau.rose`), `bibliotheque.rose` / `algebre.rose` (importent
   `tactiques.rose`), `principal.rose` (pilote CLI).
3. Vérifier statiquement avant d'exécuter : `./rose/rose verif <fichier.rose>`.

---

## 2. Le modèle de données : trois tables et un contrat

Un **terme** est une table Rose avec un champ `genre`, conformément à
`thorn/SPEC.md`. Un **contexte** est une liste de tables `{nom, type}`
**rembourrée** à l'index 0 par `table()` (sentinelle). Un **résultat** est une
table `{ok, type, erreur, code, contexte, ligne, pas, ...}`.

| `genre` | Champs | Constructeur Rose | Lecture intuitive |
|---|---|---|---|
| `var` | `nom` | `noy_var(n)` | variable |
| `type` | `niveau` | `noy_type(n)` | univers `Type(n)` |
| `pi` | `nom`, `domaine`, `codomaine` | `noy_pi(n, d, c)` | `Πn:d, c` |
| `lam` | `nom`, `domaine`, `corps` | `noy_lam(n, d, c)` | `λn:d, c` |
| `app` | `fonction`, `argument` | `noy_app(f, a)` | `f a` |
| `const` | `nom`, `type` | `noy_const(n, ty)` | constante annotée |
| `constructeur` | `nom`, `type` | `noy_constructeur(n, ty)` | constructeur annoté |
| `ind` | `nom`, `params` | `noy_ind(n, p)` | type inductif (liste rembourrée) |
| `elim` | `nom`, `motif`, `branches` | `noy_elim(n, m, b)` | éliminateur |

**Point de contrat critique** (cause d'un bug réel, détaillé dans
`PRO_RECETTES.md` §4.3) : un `pi` porte
`codomaine`, un `lam` porte `corps`. Toute fonction qui parcourt les termes
doit tester le champ qu'elle lit, pas supposer que `domaine` implique
`codomaine`.

Fonctions d'accès défensives (jamais de panique sur une table vide) :
`noy_genre`, `noy_nom`, `noy_niveau`, `noy_champ`, `noy_champ_liste`,
`noy_corps`, `noy_erreur`, `noy_type_res`, `noy_code`, `noy_contexte`,
`noy_ligne_res`, `noy_pas_res`, `noy_est_ok`.

---

## 3. Syntaxe

### 3.1 Lexeur (`thorn/src/lexeur.rose`)

```
lexer(src)            -> liste<chaine>   # jetons nus
lexer_detaille(src)   -> liste<chaine>   # jetons "texte~ligne"
lexer_table(src)      # interne : {n: nb jetons, j: jetons détaillés}
```

- Identifiants : lettres ASCII, `_`, tout code Unicode ≥ 128 (`é`, `è`, `π`),
  puis lettres / chiffres / `_`.
- Nombres : suites de chiffres → jeton nombre.
- Opérateurs : `->`, `=>`, `=`, `:`, `,`, `;`, `(`, `)`, `+`, `-`, `*`, `/`,
  `[`, `]`, `{`, `}`.
- Commentaires : `#` jusqu'à la fin de ligne. Espace, tabulation et `\r`
  ignorés ; `\n` incrémente le numéro de ligne.
- Caractère inconnu → jeton `?`, **jamais d'exception** ; le parseur signale
  la ligne.

### 3.2 Précédence des termes

De la précédence la plus **faible** à la plus **forte** :

| Niveau | Écriture | Terme produit |
|---|---|---|
| 1 (droite) | `A -> B`, `A implique B` | `pi("_", A, B)` |
| 2 (gauche) | `A ou B` | `ind("ou", [A, B])` |
| 3 (gauche) | `A et B` | `ind("et", [A, B])` |
| 4 (gauche) | `A = B` | `ind("=", [A, B])` (liste **non** rembourrée côté parseur) |
| 5 (gauche) | `A + B`, `A - B` | `app(app(const("+", type(0)), A), B)` |
| 6 (préfixe) | `non A` | `ind("non", [A])` |
| 7 (gauche) | `f a b` (juxtaposition) | `app(app(f, a), b)` |
| 8 | atome | voir ci-dessous |

Atomes : identifiant → `var` ; `type` / `type N` → `type(niveau)` ; `( ... )` ;
`fn x y : T => corps` → `lam` imbriqués ; `pour_tout x y : T, corps` → `pi`
imbriqués ; `existe x : T, P` → `ind("existe", [var(x), T, P])` ; `vrai`,
`faux` → `ind("vrai", [])`, `ind("faux", [])` ; nombre → `const("N", type(0))`.

Mots réservés : `théorème` (ou `theoreme`), `preuve` (ou `preuv`, `proof`),
`soit`, `montrer`, `par`, `cas`, `fin`, `induction`, `sur`, `pour_tout`,
`existe`, `implique`, `et`, `ou`, `non`, `vrai`, `faux`, `type`, `fn`.

**Pièges mesurés :**

- `analyser(jetons)` exige une liste typée `liste<chaine>` ; le résultat brut de
  `lexer()` est typé `?` et ne peut pas lui être passé directement. Pour un
  terme isolé issu d'une chaîne, utiliser `analyser_source(src)`.
- `analyser` / `analyser_fichier` ne lèvent jamais d'exception Rose : ils
  affichent `Thorn — ligne N : <message>` et rendent un résultat **partiel**.
  Exemple réel : `Thorn — ligne 3 : tactique inattendue '?'`.
- Dans un `.thn`, seul `0` est normalisé vers le zéro de `Nat`. Le littéral `1`
  reste `const("1", type(0))` et rend
  `ÉCHEC … : type invalide — membres d'une egalite de types differents` dans
  `0 = 1`. Écrire `succ 0` ne fonctionne pas non plus : `succ` y est une
  variable libre (`variable non liee: succ`). Les preuves de démonstration
  utilisent donc des variables et des lemmes, pas des numeralia.
- `fin` est un terminateur reconnu par le **pilote** ; le parseur
  `analyser_fichier` le traite comme une tactique inattendue. Cela n'affecte
  pas le chemin d'exécution `pilote_executer`, qui découpe les jetons lui-même.

### 3.3 Grammaire des déclarations

```
fichier      := declaration*
declaration  := "théorème" nom ":" terme "preuve" tactique*
tactique     := "soit" nom+ ":" terme
              | "montrer" terme
              | "par" nom ["sur" nom+]
              | "cas" motif nom* ":" tactique*
              | ";"
```

`analyser_fichier(src)` rend une liste de tables
`{genre:"declaration", nom, type, preuve}`. Le **format réellement exécuté**
par le pilote est plus simple (voir §5).

---

## 4. Le noyau (`thorn/src/noyau.rose`)

### 4.1 API publique

| Fonction | Rôle |
|---|---|
| `verifier(t, ctx) -> Resultat` | synthèse de type, garde de 10 000 pas |
| `verifier_lim(t, ctx, limite)` | idem, garde paramétrée |
| `beta(t) -> Terme` | forme normale **β + ι**, stratégie normale |
| `convertible(a, b) -> bool` | βι-normalisation puis α-équivalence **stricte** |
| `cumulatif(a, b) -> bool` | conversion stricte OU `Type(i) ≤ Type(j), i ≤ j` |
| `bien_forme(t, ctx) -> bool` | vrai si `verifier` accepte |
| `noy_beta_lim(t, limite)` | `{terme, pas, erreur}` (comptage des pas) |
| `noy_convertible_res(a, b, limite)` | conversion avec diagnostic |
| `noy_subst(t, nom, val)` | substitution capture-évitante |
| `noy_alpha(a, b, ea, eb)` | α-équivalence à environnements parallèles |
| `noy_ctx_vide()`, `noy_ctx_ajouter(ctx, n, ty)`, `noy_ctx_type(ctx, n)` | contextes |

### 4.2 Règles implémentées

- `Type(n) : Type(n+1)` ; `n < 0` rejeté (`NIVEAU_NEGATIF`).
- `Var` : type lu dans le contexte (recherche de la fin vers le début =
  masquage) ; sinon `VAR_NON_LIE`.
- `Πx:A.B` : `A : Type(i)` et `B : Type(j)` exigés ; type rendu
  `Type(max(i,j))`. Domaine ou codomaine non univers → `UNIVERS`.
- `λx:A.t` : même contrôle sur `A` ; type rendu `Πx:A.(type de t)`.
- `App(f,a)` : le type de `f` est βι-normalisé (doit être un `Π`, sinon
  `APP_PAS_PI`) ; le type de `a` est comparé au domaine par **cumulativité
  contrôlée** ; échec → `APP_ARG` ou `CUMUL`. Résultat `codomaine[a/nom]`.
- `const` / `constructeur` : annotation vérifiée comme type bien formé
  (`ANNOTATION` sinon), puis rendue comme type du terme.
- `ind` : paramètres synthétisés (`IND_PARAM`). Les deux formes d'égalité sont
  **durcies** : `ind("=",[a,b])` exige que `a` et `b` aient des types
  convertibles ; `ind("Eq",[A,a,b])` exige `a : A` et `b : A` ; sinon
  `EQ_MAL_TYPE`. Les autres inductifs restent structurels (type rendu
  `Type(0)`, aucun contrôle de positivité).
- `elim` : motif et branches synthétisés, type rendu `Type(0)`.

### 4.3 Réduction β **et ι**

La stratégie normale essaie d'abord l'ι puis descend (fonction avant
argument, domaine avant codomaine/corps, annotations, `params`, `branches`).
Les primitives sont reconnues **par leur nom ET par la forme de leur
annotation de type** : un `const` nommé `plus` mais mal typé n'est jamais
réduit.

| Primitive | Motif | Réduction |
|---|---|---|
| `plus : Πm:Nat. Πn:Nat. Nat` | `plus zero n` | `n` |
| `plus : Πm:Nat. Πn:Nat. Nat` | `plus (succ k) n` | `succ (plus k n)` |
| `nat_rec : ΠP.Πbase.Πpas.Πm:Nat. P m` | `nat_rec P base pas zero` | `base` |
| `nat_rec : ΠP.Πbase.Πpas.Πm:Nat. P m` | `nat_rec P base pas (succ k)` | `pas k (nat_rec P base pas k)` |

Les pas d'ι sont comptés comme les pas de β (`plus (succ zero) n` = 2 pas,
mesuré par `tests_noyau.rose`). `convertible` voit donc
`plus zero n ≡ n` et `plus (succ m) n ≡ succ (plus m n)`.

**Limites de la conversion** : pas d'η, pas de `let`, pas de récursion
générale, pas de conversion avec hypothèses, cumulativité **uniquement dans
`App`**. Si la garde (10 000 pas) est épuisée, `convertible` rend `faux`
(le diagnostic est dans `noy_convertible_res`).

### 4.4 Résultats et codes

`noy_resultat_plein` est l'unique constructeur. Champs : `ok` (booléen),
`type` (Terme ou sentinelle `_absent`), `erreur` (chaine), `code` (chaine),
`contexte` (règle fautive), `ligne` (champ `ligne` du terme, 0 si absent),
`pas` (pas consommés).

Codes stables : `OK`, `GARDE_PAS`, `VAR_NON_LIE`, `NIVEAU_NEGATIF`, `UNIVERS`,
`APP_PAS_PI`, `APP_ARG`, `CUMUL`, `ANNOTATION`, `IND_PARAM`, `EQ_MAL_TYPE`,
`GENRE_INCONNU`. Tous les accesseurs sont défensifs.

Le champ `ligne` est **optionnel** sur un terme : s'il est présent et qu'une
erreur est produite sous ce terme, le numéro est remonté dans
`Resultat.ligne` (le plus interne gagne). Le parseur actuel ne fournit pas de
position, mais un outil peut le faire sans casser le noyau.

---

## 5. Tactiques et pilote

### 5.1 Le protocole d'honnêteté

Toute tactique construit un terme `t` puis appelle :

```
r   = verifier(t, ctx)
ok  = noy_est_ok(r) ET convertible(noy_type_res(r), but)
```

C'est `ta_verifie()` (`tactiques.rose`). Aucun `ok = vrai` n'est codé en dur.
Un résultat porte en plus `terme`, `buts`, `ctxs`, `regle`, et pour
`introduction` / `induction` les noms des trous.

### 5.2 Les six tactiques

| Tactique | Terme produit | Vérification réelle |
|---|---|---|
| `reflexivite(but)` | `refl A a` + `λ` pour traverser les Π | synthèse + `convertible` à chaque niveau ; exige `convertible(a,b)` |
| `symetrie(but [,ctx])` | `symm A a b h`, `h : a=b` cherché dans le contexte | synthèse + conversion ; `symm` est un terme dérivé vérifié |
| `introduction(but, ctx)` | `λx:A. ?trou`, `?trou : B` | squelette typé ; rend `buts=[B]` et `ctxs` mis à jour |
| `application(but, ctx)` | variable exacte, ou 1–2 applications d'hypothèses Π | chaque candidat passe `verifier` + `convertible` |
| `induction(but, ctx)` | `nat_rec P ?base ?pas` | squelette typé ; `induction_resolue()` vérifie le terme fermé |
| `simplifier(t)` | terme réduit | **non** soumis au noyau : conversion étendue, carburant 200 |

Alias : `refl`, `sym`, `intro`, `app`, `ind`. Variantes avec contexte :
`reflexivite_ctx`, `symetrie_ctx`. Résolutions : `introduction_resolue`,
`induction_resolue`.

### 5.3 Signature (axiomes explicites)

`Nat`, `zero`, `succ`, `plus`, `Eq`, `refl`, `eq_rec`, `nat_rec`,
`plus_0_l : Πn, 0+n = n`, `plus_s_l : Πm n, succ m + n = succ (m+n)`.
Les lemmes `symm`, `trans`, `congr_succ`, `congr_plus_l`, `plus_zero_d`,
`plus_succ_d` sont des **termes dérivés**, construits puis vérifiés. Voir
`PRO_LIMITES.md` §2 pour la liste complète (algèbre incluse).

### 5.4 Pilote CLI

```
./rose/rose thorn/src/principal.rose <fichier.thn> [--trace]
./rose/rose thorn/src/principal.rose --demo [fichiers.thn...] [--trace]
./rose/rose thorn/src/principal.rose --aide     # aussi -h, --help
```

Sortie : `OK <nom>` ou `ÉCHEC <nom> : <raison>`, puis
`N théorème(s), M réussi(s)`. Codes de sortie : **0** tout passe, **1** au
moins un échec de preuve, **2** lecture / lexage / analyse impossible ou
aucun théorème. `--demo` utilise un manifeste figé (`pilote_manifeste()`),
Rose n'ayant pas d'énumération de répertoire.

**Défaut connu et mesuré** : `--trace` plante sur les termes `lam` avec
`Rose : erreur — clé absente de la table` (rc=1). Cause exacte et contournement
dans `PRO_RECETTES.md` §4.3.

---

## 6. Installation de test reproductible (constats du 20/11/2026)

```
$ ./rose/rose thorn/tests_noyau.rose        → REUSSIS=126 ECHOUES=0 RESULTAT: OK    (rc=0)
$ ./rose/rose thorn/tests_syntaxe.rose      → OK = 8   ECHEC = 0                    (rc=0)
$ ./rose/rose thorn/tests_tactiques.rose    → TOTAL_REUSSIS=33 TOTAL_ECHOUES=0 RESULTAT: OK (rc=0)
$ ./rose/rose thorn/tests_pilote.rose       → REUSSIS=12  ECHOUES=0  RESULTAT: OK   (rc=0)
$ ./rose/rose thorn/tests_algebre.rose      → 34 lemme(s) vérifié(s), 0 échec(s)    (rc=0)
$ ./rose/rose thorn/src/bibliotheque.rose   → 10 lemme(s) vérifié(s), 0 échec(s)    (rc=0)
$ ./rose/rose thorn/src/principal.rose --demo → 5 théorème(s), 5 réussi(s)          (rc=0)
```

`./rose/rose verif <fichier.rose>` rend `vérifié` pour les sept modules
`noyau`, `lexeur`, `analyse`, `tactiques`, `bibliotheque`, `algebre`,
`principal`, et pour les dix fichiers d'exemple ci-dessous.

**Note de concordance.** Pendant la rédaction de ce document, un chantier
concurrent a enrichi `tests_noyau.rose` (88 → **126** assertions) et
`tests_tactiques.rose` (19 → **33** succès). Les nombres ci-dessus sont ceux
relevés **en fin de chantier** avec `./rose/rose` (0.1.6). Les documents
antérieurs (`NOYAU.md`, `TACTIQUES.md`, `QUALITE.md`) citent encore 88 et 19 :
ce sont des constats plus anciens, pas des contradictions. Le script
`thorn/tests_regression.sh`, lui, réutilise `rose/target/release/rose` (0.1.7)
et a rendu `17/17 cas`, `BILAN: OK`, avec `REUSSIS=126` observé. Re-mesurer
après toute modification.

---

## 7. Les dix exemples commentés, exécutés

Tous les fichiers sont dans `thorn/exemples/`. Chaque exemple a été lancé
depuis la racine avec `./rose/rose <chemin>` ; la sortie ci-dessous est la
sortie réelle, `RC` = code de sortie relevé juste après.

### Exemple 1 — le terme d'identité (`ex01_identite.rose`)

Le terme `λA:Type(0). λx:A. x` est soumis au noyau ; on compare le type
synthétisé au but `Π(A:Type(0))(x:A), A`.

```
$ ./rose/rose thorn/exemples/ex01_identite.rose
terme   = Lam(A:Type(0), Lam(x:A, x))
ok      = vrai
type    = Pi(A:Type(0), Pi(x:A, A))
code    = OK
convert = vrai
RC=0
```

*Lecture* : `verifier` a synthétisé exactement le but, `code = OK`, et
`convertible` confirme l'égalité. C'est le « hello world » du protocole
`verifier` + `convertible`.

### Exemple 2 — β et α (`ex02_beta_alpha.rose`)

`(λx:X. x) a` se réduit vers `a` ; deux `λ` dont seul le nom du lieur diffère
sont α-équivalents ; un `x` libre ne l'est pas.

```
$ ./rose/rose thorn/exemples/ex02_beta_alpha.rose
beta(App(Lam(x:X,x), a)) = a
convertible(app, a)       = vrai
alpha(Lam(x:X,x), Lam(y:X,y)) = vrai
alpha(Lam(x:X,x), Lam(y:X,x)) = faux
RC=0
```

*Lecture* : la conversion du noyau est **β + α**, sans η. La troisième ligne
montre que `noy_alpha` distingue lieur et variable libre.

### Exemple 3 — ι-réduction (`ex03_iota.rose`)

Les équations de `plus` et de `nat_rec` sont **calculées** par le noyau
(reconnaissance par nom + forme du type), et le résultat reste typé.

```
$ ./rose/rose thorn/exemples/ex03_iota.rose
beta(plus zero n)         = n
beta(plus (succ zero) n)  = App(succ, n)
convertible(0+n, n)       = vrai
convertible(succ 0+n, succ n) = vrai
beta(nat_rec P base pas zero) = zero
verifier(nat_rec ... zero) ok = vrai
son type                      = Nat
RC=0
```

*Lecture* : `β` ici inclut l'ι (§4.3). Le type `Nat` rendu après réduction
montre la préservation du type. Attention : cette ι est **étroite** — `mult`
n'est pas réduit (vérifié séparément : `beta(n × 0)` reste une `app`).

### Exemple 4 — refus et codes d'erreur (`ex04_refus.rose`)

Cinq refus typiques, chacun avec son code stable.

```
$ ./rose/rose thorn/exemples/ex04_refus.rose
convertible(0, succ 0)   = faux
code(0 = Type(0))        = EQ_MAL_TYPE
contexte                 = ind(=)
code(var inconnu)        = VAR_NON_LIE
code(Type 0 appliqué)    = APP_PAS_PI
code(Ω avec limite 0)    = GARDE_PAS
message                  = depassement de la garde de pas (0)
RC=0
```

*Lecture* : `convertible(0, succ 0) = faux` est le mécanisme Anti-fausse ;
`Ω = (λx. x x)(λx. x x)` est arrêté par la garde (code `GARDE_PAS`, message
explicite). Le `RC=0` est celui du **script de démonstration** : il affiche
des refus attendus, il ne valide pas de preuve.

### Exemple 5 — cumulativité minimale (`ex05_cumul.rose`)

`Type(i) ≤ Type(j)` si `i ≤ j`, uniquement dans `App` ; `convertible` public
reste strict.

```
$ ./rose/rose thorn/exemples/ex05_cumul.rose
convertible(Type0, Type1) = faux
cumulatif(Type0, Type1)   = vrai
cumulatif(Type1, Type0)   = faux
cumulatif(Type0, Type0)   = vrai
verifier((λX:Type1. X) Type0) ok = vrai
type synthétisé                = type
convertible(terme, but strict)  = faux
RC=0
```

*Lecture* : l'application est acceptée avec un argument « plus petit », mais
la conversion stricte échoue : ne pas confondre les deux fonctions.

### Exemple 6 — syntaxe et précédence (`ex06_syntaxe.rose`)

Le parseur seul, sans noyau. Cinq sources, cinq AST réels.

```
$ ./rose/rose thorn/exemples/ex06_syntaxe.rose
1) a ou b et c implique d
[genre => pi, nom => _, domaine => [genre => ind, nom => ou, params => [[genre => var, nom => a], [genre => ind, nom => et, params => [[genre => var, nom => b], [genre => var, nom => c]]]]], codomaine => [genre => var, nom => d]]
2) a + b = c
[genre => ind, nom => =, params => [[genre => app, fonction => [genre => app, fonction => [genre => const, nom => +, type => [genre => type, niveau => 0]], argument => [genre => var, nom => a]], argument => [genre => var, nom => b]], [genre => var, nom => c]]]
3) fn x : Nat => x
[genre => lam, nom => x, domaine => [genre => var, nom => Nat], corps => [genre => var, nom => x]]
4) pour_tout x : Nat, x = x
[genre => pi, nom => x, domaine => [genre => var, nom => Nat], codomaine => [genre => ind, nom => =, params => [[genre => var, nom => x], [genre => var, nom => x]]]]
5) existe x : Nat, x = zero
[genre => ind, nom => existe, params => [[genre => var, nom => x], [genre => var, nom => Nat], [genre => ind, nom => =, params => [[genre => var, nom => x], [genre => var, nom => zero]]]]]
RC=0
```

*Lecture* : `implique` lie le plus faiblement, puis `ou`, `et`, `=`, `+` ;
`existe` garde le nom du liant comme premier paramètre (ce n'est **pas** un
`pi`, le noyau doit le reconnaître explicitement).

### Exemple 7 — réflexivité et symétrie (`ex07_refl_sym.rose`)

```
$ ./rose/rose thorn/exemples/ex07_refl_sym.rose
reflexivite : ok    = vrai
reflexivite : regle = reflexivite/lambda
symetrie : ok          = vrai
symetrie : regle       = symetrie/symm
symetrie sans hypothese = faux
raison                  = symétrie : aucune hypothèse de type a = b
RC=0
```

*Lecture* : `reflexivite` traverse les Π et fabrique `λA. λx. refl A x` ;
`symetrie` exige une hypothèse de type exact (modulo β/α) — sans elle, échec
honnête avec une raison.

### Exemple 8 — introduction, application, simplifier (`ex08_intro_app.rose`)

```
$ ./rose/rose thorn/exemples/ex08_intro_app.rose
introduction : ok        = vrai
introduction : regle     = introduction/pi
introduction : buts      = 1
application : ok         = vrai
application : regle      = application/exact
application : terme      = x
simplifier(0 + n)        = n
convertible(0+n, n)      = vrai
RC=0
```

*Lecture* : `introduction` ouvre **un** but résiduel ; `application` trouve
`x` dans le contexte. `simplifier` réduit `0+n` vers `n`, mais sa conversion
étendue n'est **pas** garantie `convertible` pour le noyau en général — ici
la dernière ligne le vérifie explicitement sur ce cas.

### Exemple 9 — induction vérifiée (`ex09_induction.rose`)

`H : ∀m:Nat, m = m` par `nat_rec`, avec base `refl Nat 0` et pas
`λk. λih. refl Nat (succ k)`.

```
$ ./rose/rose thorn/exemples/ex09_induction.rose
induction : ok           = vrai
induction : regle        = induction/Nat
induction : nb buts      = 2
induction_resolue : ok   = vrai
induction_resolue : regle = induction/Nat+résolue
verifier(terme) : ok     = vrai
convertible(type, but)  = vrai
RC=0
```

*Lecture* : le squelette ouvre **deux** buts (base, pas) ; `induction_resolue`
substitue les branches, puis le terme **fermé** repasse par `verifier` et
`convertible`. C'est le même chemin que les preuves
`associativite_plus` / `commutativite_plus`.

### Exemple 10 — le pilote CLI, un accepté et un refusé

Fichiers : `thorn/exemples/ex10_ok.thn` et `thorn/exemples/ex10_echec.thn`.

```
$ ./rose/rose thorn/src/principal.rose thorn/exemples/ex10_ok.thn
OK ex10_refl_zero
1 théorème(s), 1 réussi(s)
RC=0

$ ./rose/rose thorn/src/principal.rose thorn/exemples/ex10_echec.thn
ÉCHEC ex10_faux : réflexivité : les deux membres diffèrent
1 théorème(s), 0 réussi(s)
RC=1

$ ./rose/rose thorn/src/principal.rose thorn/exemples/ex10_echec.thn --trace
  [trace] ex10_faux but = pi(A)(type[0] -> pi(B)(type[0] -> ind(=)))
ÉCHEC ex10_faux : réflexivité : les deux membres diffèrent
1 théorème(s), 0 réussi(s)
RC=1
```

*Lecture* : le but du refus (`∀A B:Type, A = B`) est **bien typé** — les deux
membres ont le même type `Type(0)` — mais ils ne sont pas convertibles : la
réflexivité ne peut pas fabriquer de preuve. Aucun `ok` n'est codé en dur.
Le `--trace` fonctionne ici : le but tracé ne contient aucun `lam`, et aucun
terme n'est affiché puisque la preuve échoue. Voir le bug documenté en
`PRO_RECETTES.md` §4.3.

---

## 8. Fichiers de référence

| Fichier | Contenu |
|---|---|
| `thorn/SPEC.md` | contrat des termes / contextes / API |
| `thorn/docs/NOYAU.md` | noyau, β et ι, codes, garde |
| `thorn/docs/SYNTAXE.md` | lexeur, parseur, grammaire |
| `thorn/docs/TACTIQUES.md` | six tactiques, signature, honnêteté |
| `thorn/docs/PILOTE.md` | CLI `--demo`, `--trace`, codes |
| `thorn/docs/ALGEBRE.md` | 34 lemmes d'algèbre et leurs axiomes |
| `thorn/docs/QUALITE.md` | porte de non-régression CI |
| `PRO_LIMITES.md` | prouvé / supposé / impossible, comparaison Lean |
| `PRO_RECETTES.md` | écrire, déboguer, étendre |
| `thorn/exemples/` | les dix exemples exécutés ci-dessus |
