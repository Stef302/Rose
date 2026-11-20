# Thorn — présentation pour un utilisateur qui ne connaît pas le projet

Document écrit le 21/11/2026. Il décrit l'état **mesuré** de Thorn à cette date.
Les modules `thorn/src/tactiques.rose`, `thorn/src/principal.rose` et les
preuves de `thorn/preuves/` sont en cours de correction par un autre chantier
en parallèle (les fichiers `.slg` ont été renommés en `.thn` pendant la
rédaction) : les sorties ci-dessous sont des constats datés, à re-mesurer
après tout changement de module.

---

## 1. Qu'est-ce que Thorn ?

Thorn est un **assistant de démonstration expérimental**, écrit entièrement en
[Rose](https://github.com/Stef302/Rose), **sans aucune dépendance externe**
(pas de Python, pas de Coq, pas de bibliothèque C). C'est un projet distinct de
Rose : Thorn est un *consommateur* de Rose, il ne modifie pas `rose/src/**`.

Ce n'est **ni un compilateur, ni un calculateur**. C'est un **vérificateur de
preuves** : on lui donne une affirmation et une preuve, et il répond si la
preuve est valide d'après son noyau de types dépendants.

Comparing with un compilateur : un compilateur produit du code exécutable ;
Thorn produit un **terme de preuve** et contrôle son type. Comparing with une
calculatrice : une calculatrice évalue `1 + 1` ; Thorn ne calcule pas `plus`,
il vérifie qu'un terme de type `Eq Nat (plus 1 1) 2` est bien typé — et encore,
seulement dans la mesure où les équations de `plus` lui sont données comme
axiomes de signature (voir §4).

## 2. Ce qu'il fait concrètement

On écrit une déclaration :

```
théorème <nom> : <type du but>
preuve <tactique ou terme>
fin
```

Thorn construit alors un **terme de preuve** (un arbre de syntaxe Rose décrit
dans `SPEC.md`) et le soumet au **noyau** :

1. `verifier(terme, contexte)` fait une **synthèse de type** et rend
   `{ok, type, erreur}` ;
2. `convertible(type_synthétisé, but)` compare le type obtenu au but annoncé
   (β-réduction complète + équivalence α).

Le verdict est `OK <nom>` ou `ÉCHEC <nom> : <raison>`. Une preuve n'est acceptée
que si **les deux** étapes passent. Il n'y a aucun raccourci : un `ok` n'est
jamais une valeur `vrai` écrite en dur dans les démonstrations.

### Ce que Thorn apporte

- **Certitude formelle** : chaque terme produit est réellement typé par le
  noyau ; les axiomes utilisés sont explicites et documentés.
- **Syntaxe française** : `théorème`, `preuve`, `soit`, `montrer`, `par`, `cas`,
  `fin`, `pour_tout`, `existe`, `implique`, `et`, `ou`, `non`, `vrai`, `faux`.
- **Transparence** : chaque tactique rend une table qui contient le `terme`
  construit et la `regle` employée ; l'option `--trace` du pilote affiche ces
  termes. On peut toujours demander « quel terme as-tu fabriqué ? ».
- **Zéro dépendance** : tout tient dans cinq fichiers Rose plus le binaire
  `./rose/rose` du dépôt.

## 3. Syntaxe complète

### 3.1 Gabarit d'une déclaration

```
théorème nom_du_theoreme : <type du but>
preuve
  soit x y : T           # introduit des hypothèses / variables
  montrer <but restant>  # déclare le but courant
  par <tactique>         # reflexivite | symetrie | introduction
                         # | application | induction [sur x] | simplifier
  cas zero :             # branche d'induction
    par reflexivite
  cas succ n :
    par simplifier
fin                      # terminateur du bloc (pilote)
```

Le lexeur accepte les accents (`théorème`) et aussi `theoreme` ; `preuve`
admet les alias `preuv` et `proof`. Les tactiques ont des alias courts :
`refl`, `sym`, `intro`, `app`, `ind`.

Ànoter, mesuré le 21/11 : `fin` est un terminateur reconnu par le **pilote**
(découpage des jetons), mais le **parseur** `analyser_fichier` le traite comme
une tactique inattendue et affiche
`Thorn — ligne N : tactique inattendue 'fin'` tout en rendant la déclaration
partielle. Les exemples exécutables du §3.3 passent donc par les fonctions Rose
du noyau et des tactiques, pas par un fichier `.thn` contenant `fin`.

### 3.2 Termes et précédence

De la précédence **la plus faible à la plus forte** :

| Niveau | Écriture | Terme produit |
|---|---|---|
| 1 (droite) | `A -> B`, `A implique B` | `pi("_", A, B)` |
| 2 (gauche) | `A ou B` | `ind("ou", [A, B])` |
| 3 (gauche) | `A et B` | `ind("et", [A, B])` |
| 4 (gauche) | `A = B` | `ind("=", [A, B])` |
| 5 (gauche) | `A + B`, `A - B` | `app(app(const("+", type(0)), A), B)` |
| 6 (préfixe) | `non A` | `ind("non", [A])` |
| 7 (gauche) | `f a b` (juxtaposition) | `app(app(f, a), b)` |
| 8 | atome | `var`, `type [N]`, `(...)`, `fn x : T => c`, `pour_tout x : T, c`, `existe x : T, P`, `vrai`, `faux`, nombre |

Mots réservés : `théorème`, `preuve`, `soit`, `montrer`, `par`, `cas`, `fin`,
`induction`, `sur`, `pour_tout`, `existe`, `implique`, `et`, `ou`, `non`,
`vrai`, `faux`, `type`, `fn`. Un caractère inconnu donne le jeton `?`, jamais
d'exception ; le parseur signale la ligne.

**Piège Rose, mesuré** : les variables globales d'un programme importateur
doivent être préfixées (ex. `$ex_but`, `$bl_terme`). Rose infère le type des
variables **par nom, tous scopes confondus** : une globale `$r` ou `$but`
entre en collision avec les locaux des modules importés et produit des erreurs
du type `Ordre entier > complexe impossible`. Les exemples ci-dessous utilisent
tous le préfixe `$ex_`.

### 3.3 Cinq exemples copiables, avec leur sortie réelle

Pour chaque bloc : copiez-le dans un fichier (par exemple
`/tmp/opencode/exN.rose`), placez-vous à la racine du dépôt, puis lancez
`./rose/rose /tmp/opencode/exN.rose`. Les sorties ci-dessous ont été obtenues
exactement ainsi le 21/11/2026.

**Exemple 1 — identité** : `λA:Type(0). λx:A. x`, le terme de preuve de
`Π(A:type)(x:A), A`.

```rose
importer "thorn/src/tactiques.rose"
$ex_but = noy_pi("A", noy_type(0), noy_pi("x", noy_var("A"), noy_var("A")))
$ex_terme = noy_lam("A", noy_type(0), noy_lam("x", noy_var("A"), noy_var("x")))
$ex_r = verifier($ex_terme, ta_ctx())
>> "OK=" + chaine(noy_est_ok($ex_r))
>> "convertible=" + chaine(convertible(noy_type_res($ex_r), $ex_but))
```

Sortie réelle :

```
OK=vrai
convertible=vrai
```

**Exemple 2 — réflexivité** : la tactique `reflexivite` fabrique
`λA. λx. refl A x` en traversant les Π.

```rose
importer "thorn/src/tactiques.rose"
$ex_A = noy_var("A")
$ex_x = noy_var("x")
$ex_but = noy_pi("A", noy_type(0), noy_pi("x", $ex_A, ta_eq($ex_A, $ex_x, $ex_x)))
$ex_r = reflexivite($ex_but)
>> "OK=" + chaine(ta_boo($ex_r, "ok"))
>> "regle=" + ta_txt($ex_r, "regle")
```

Sortie réelle :

```
OK=vrai
regle=reflexivite/lambda
```

**Exemple 3 — symétrie** : avec `h : 0 = succ 0` dans le contexte, la
tactique `symetrie` construit `symm Nat 0 (succ 0) h`.

```rose
importer "thorn/src/tactiques.rose"
$ex_ctx = noy_ctx_ajouter(ta_ctx(), "h", ta_eq(ta_nat(), ta_zero(), ta_succ(ta_zero())))
$ex_but = ta_eq(ta_nat(), ta_succ(ta_zero()), ta_zero())
$ex_r = symetrie_ctx($ex_but, $ex_ctx)
>> "OK=" + chaine(ta_boo($ex_r, "ok"))
>> "regle=" + ta_txt($ex_r, "regle")
```

Sortie réelle :

```
OK=vrai
regle=symetrie/symm
```

**Exemple 4 — induction sur Nat** : `nat_rec P base pas` où `base` et `pas`
sont fournis explicitement, puis vérifiés par `induction_resolue`.

```rose
importer "thorn/src/tactiques.rose"
$ex_but = noy_pi("m", ta_nat(), ta_eq(ta_nat(), noy_var("m"), noy_var("m")))
$ex_base = ta_ap2(ta_refl_symbole(), ta_nat(), ta_zero())
$ex_pas = noy_lam("k", ta_nat(), noy_lam("ih", ta_eq(ta_nat(), noy_var("k"), noy_var("k")), ta_ap2(ta_refl_symbole(), ta_nat(), ta_succ(noy_var("k")))))
$ex_r = induction_resolue($ex_but, ta_ctx(), $ex_base, $ex_pas)
>> "OK=" + chaine(ta_boo($ex_r, "ok"))
>> "regle=" + ta_txt($ex_r, "regle")
```

Sortie réelle :

```
OK=vrai
regle=induction/Nat+résolue
```

**Exemple 5 — précédence des opérateurs** : le parseur seul, sans noyau.
`implique` est le plus faible, puis `ou`, puis `et`, puis `=`, puis `+`.

```rose
importer "thorn/src/analyse.rose"
>> chaine(analyser_source("a ou b et c implique d"))
>> chaine(analyser_source("a + b = c"))
```

Sortie réelle (deux lignes) :

```
[genre => pi, nom => _, domaine => [genre => ind, nom => ou, params => [[genre => var, nom => a], [genre => ind, nom => et, params => [[genre => var, nom => b], [genre => var, nom => c]]]]], codomaine => [genre => var, nom => d]]
[genre => ind, nom => =, params => [[genre => app, fonction => [genre => app, fonction => [genre => const, nom => +, type => [genre => type, niveau => 0]], argument => [genre => var, nom => a]], argument => [genre => var, nom => b]], [genre => var, nom => c]]]
```

Lecture : la première ligne est `Π_, (a ou (b et c)) -> d` ; la deuxième est
`(a + b) = c` — l'addition lie plus fort que l'égalité.

## 4. Ce que Thorn ne fait PAS encore (honnêteté)

Ces limites sont documentées dans les modules et vérifiables dans le code ;
elles ne sont pas des défauts cachés.

1. **Pas d'ι-réduction dans le noyau.** `beta` et `convertible` ne font que β + α.
   Les équations `0 + n = n` et `succ m + n = succ (m + n)` sont donc des
   constantes de signature (`plus_0_l`, `plus_s_l`), pas des calculs.
   `simplifier` applique ces règles localement mais son résultat n'est **pas**
   garanti convertible au terme initial pour le noyau.
2. **Pas de termination ni de positivité.** `nat_rec` et `eq_rec` sont des
   axiomes dont le type encode le principe d'élimination ; le noyau vérifie les
   types des applications, pas la cohérence du type inductif.
3. **Égalité sur-permissive.** Le noyau valide structurellement le terme
   `ind` qui représente `a = b` mais ne vérifie pas que `a` et `b` ont
   effectivement le même type.
4. **Univers unique `Type(0)`.** Pas de polymorphisme d'univers, pas de
   cumulativité (`Type(0)` n'est pas un sous-type de `Type(1)`), pas d'η.
5. **Induction et recherche limitées.** Les buts résiduels sont rendus sous
   forme de trous typés ; les branches sont fournies explicitement par le code
   Rose. `symetrie` exige une hypothèse de type exact (modulo β/α) ;
   `application` essaie une ou deux applications, sans unification générale.
6. **Preuves de démonstration partiellement cassées.** Le chantier TH1 corrige
   en parallèle les preuves de `thorn/preuves/` le 21/11 (les `.slg` y ont été
   renommés en `.thn` pendant cette rédaction). Dernier constat mesuré :
   `./rose/rose thorn/src/principal.rose --demo` rend
   `5 théorème(s), 0 réussi(s)` avec, par exemple,
   `ÉCHEC identite : terme de preuve invalide — variable non liee: ?` et
   `ÉCHEC reflexivite : tactique reflexivite : index hors des limites de la liste`.
   État transitoire, à re-mesurer. **Ne pas prétendre que ces preuves passent.**
   Ce qui passe réellement, ce sont les termes construits et vérifiés par
   `tests_noyau.rose`, `tests_syntaxe.rose`, `tests_tactiques.rose` et
   `bibliotheque.rose`.

## 5. Architecture : les cinq modules et qui appelle qui

| Module | Rôle | API publique (essentielle) | Importe |
|---|---|---|---|
| `thorn/src/noyau.rose` | noyau de types dépendants | `verifier`, `beta`, `convertible`, `bien_forme`, `noy_*` | rien |
| `thorn/src/lexeur.rose` | découpage en jetons, numéros de ligne | `lexer`, `lexer_detaille`, `lexer_table` (interne) | rien |
| `thorn/src/analyse.rose` | parseur des termes et des déclarations | `analyser`, `analyser_source`, `analyser_fichier`, constructeurs `t_*` | `lexeur.rose` |
| `thorn/src/tactiques.rose` | signature, tactiques, lemmes dérivés | `reflexivite`, `symetrie`, `introduction`, `application`, `induction`, `simplifier`, `ta_*` | `noyau.rose` |
| `thorn/src/principal.rose` | pilote CLI (fichiers `.thn`) | `pilote_executer`, `pilote_demo`, `pilote_main` | `tactiques`, `analyse`, `noyau`, `lexeur` |
| `thorn/src/bibliotheque.rose` *(nouveau)* | lemmes vérifiés, consommateur pur | `bl_test`, `bl_*` | `tactiques.rose` |

Chaîne d'appel :

```
principal.rose
   ├── tactiques.rose ──> noyau.rose  (verifier / beta / convertible)
   ├── analyse.rose ────> lexeur.rose (lexer_table pour analyser_source)
   ├── noyau.rose
   └── lexeur.rose

bibliotheque.rose ──> tactiques.rose ──> noyau.rose
```

**Pourquoi l'ordre d'import est imposé.** `analyse.rose` appelle
`lexer_table`, défini dans `lexeur.rose` : sans cet import,
`analyser_source` échoue avec `Fonction 'lexer_table' inconnue`. Mesure du
21/11 : avec

```
importer "thorn/src/lexeur.rose"
importer "thorn/src/analyse.rose"
```

dans cet ordre, l'appel à `analyser_source("f x")` rend précisément cette
erreur ; avec `importer "thorn/src/analyse.rose"` seul, cela marche. Le pilote
importe donc `tactiques`, `analyse`, `noyau`, `lexeur` **dans cet ordre** ;
`bibliotheque.rose` importe seulement `tactiques`, qui tire le noyau
transitivement.

## 6. Comment lancer (depuis la racine du dépôt)

```bash
cd /home/clausake/Stefan/ROSE

# Vérifier le nouveau fichier de lemmes
./rose/rose verif thorn/src/bibliotheque.rose          # → vérifié
./rose/rose thorn/src/bibliotheque.rose                 # → 10 lemmes vérifiés, 0 échec

# Vérifier les cinq modules
./rose/rose verif thorn/src/noyau.rose
./rose/rose verif thorn/src/lexeur.rose
./rose/rose verif thorn/src/analyse.rose
./rose/rose verif thorn/src/tactiques.rose
./rose/rose verif thorn/src/principal.rose

# Lancer les suites de tests (sorties mesurées le 21/11)
./rose/rose thorn/tests_noyau.rose        # → REUSSIS=35, ECHOUES=0, RESULTAT: OK
./rose/rose thorn/tests_syntaxe.rose      # → OK = 8   ECHEC = 0
./rose/rose thorn/tests_tactiques.rose    # → REUSSIS=19, ECHOUES=0, RESULTAT: OK

# Pilote (en cours de correction — ne pas attendre que les .slg passent)
./rose/rose thorn/src/principal.rose --aide
```

Utiliser `./rose/rose` (0.1.6, présent dans le dépôt) : le `rose` du PATH
(0.1.5) ne connaît pas la commande `verif`.

## 7. La bibliothèque de lemmes ajoutée

`thorn/src/bibliotheque.rose` est un nouveau fichier, consommateur lecteur des
modules existants. Il construit **dix lemmes**, chacun soumis personnellement à
`verifier()` puis `convertible()` via la fonction `bl_test` ; aucun résultat
`ok` n'est codé en dur.

| # | Lemme | Construction |
|---|---|---|
| 1 | `refl_succ` | `λn. refl Nat (succ n)` |
| 2 | `zero_plus_succ` | `λn. plus_0_l (succ n)` |
| 3 | `succ_plus_zero` | `λn. trans (plus_s_l n zero) (congr_succ (plus_zero_d n))` |
| 4 | `symm` | `eq_rec T x y (λu, u = x) (refl T x) h` |
| 5 | `trans` | `eq_rec T y z (λu, x = u) h1 h2` |
| 6 | `congr_succ` | `eq_rec` avec `P u = succ r = succ u` |
| 7 | `congr_plus_l` | `eq_rec` avec `P u = r+p = u+p` |
| 8 | `plus_zero_d` | `nat_rec`, base `plus_0_l zero`, pas `trans (plus_s_l k zero) (congr_succ ih)` |
| 9 | `plus_assoc` | induction sur `m`, branches explicites |
| 10 | `plus_comm` | induction sur `m`, branches explicites |

Sortie réelle de `./rose/rose thorn/src/bibliotheque.rose` le 21/11/2026 :

```
Thorn — bibliothèque de lemmes (vérification par le noyau)
OK refl_succ
OK zero_plus_succ
OK succ_plus_zero
OK symm
OK trans
OK congr_succ
OK congr_plus_l
OK plus_zero_d
OK plus_assoc
OK plus_comm
10 lemme(s) vérifié(s), 0 échec(s)
RESULTAT: OK
```

## 8. Fichiers de référence

- Spécification : `thorn/SPEC.md`
- Noyau : `thorn/docs/NOYAU.md`
- Syntaxe : `thorn/docs/SYNTAXE.md`
- Tactiques : `thorn/docs/TACTIQUES.md`
- Pilote : `thorn/docs/PILOTE.md`
- Présentation : ce fichier
- Bibliothèque : `thorn/src/bibliotheque.rose`

Aucun commit et aucune publication n'ont été faits pour ce chantier.
