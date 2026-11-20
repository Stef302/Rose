# Thorn — tactiques (PROV3) : ce qui est réel, ce qui est simplifié

Fichiers livrés : `thorn/src/tactiques.rose`, `thorn/tests_tactiques.rose`,
`thorn/preuves/{identite,reflexivite,associativite,commutativite}.thn`,
ce document. Aucun autre fichier du dépôt n'a été modifié. Aucun commit, aucune publication.

## 1. Comment un terme est déclaré « vérifié »

Le noyau PROV1 (`thorn/src/noyau.rose`, importé par `tactiques.rose`) expose
`verifier(t, ctx)` qui fait une **synthèse de type** : il rend `{ok, type, erreur}`.
Il ne compare pas `t` à un but. La vérification d'une tactique est donc :

```
r   = verifier(terme, ctx)
ok  = noy_est_ok(r) ET convertible(noy_type_res(r), but)
```

C'est la fonction `ta_verifie()`. **Toutes** les tactiques passent par elle.
Aucun vérificateur maison n'est utilisé (contrairement à l'ancien `tactiques.rose`,
qui portait son propre `sl_verif` et n'appelait pas le noyau).

Le contexte est une `liste<table>` rembourrée (index 0 sentinelle), exactement
comme dans le noyau. Les termes sont des tables avec champ `genre` (contrat SPEC.md).

## 2. Les six tactiques

| Tactique | Terme produit | Ce qui est vérifié par le noyau |
|---|---|---|
| `reflexivite(but)` | `refl A a`, puis des `λ` pour traverser les Π | à chaque niveau : synthèse + `convertible(type, but)` ; exige `convertible(a, b)` pour un but `Eq A a b` |
| `symetrie(but [,ctx])` | `symm A a b h` où `h : Eq A a b` est cherché dans le contexte | synthèse + conversion ; `symm` lui-même est un terme dérivé vérifié séparément |
| `introduction(but, ctx)` | `λx:A. ?trou` avec `?trou : B` dans un contexte méta étendu | le squelette est typé par le noyau dans ce contexte ; retourne `buts=[B]` et `ctxs` mis à jour avec `x:A` |
| `application(but, ctx)` | une variable exacte, ou 1–2 applications d'hypothèses Π | chaque candidat est soumis à `verifier` + `convertible` avant d'être accepté |
| `exact(but, ctx)` *(nouveau)* | une variable du contexte dont le type est convertible au but | synthèse + `convertible` ; refuse si aucune hypothèse ne convient |
| `apply(but, ctx)` *(nouveau)* | application complète construite par déduction arrière (voir §4b) | chaque terme candidat est soumis à `verifier` + `convertible` ; un méta non résolu est rejeté |
| `induction(but, ctx)` | `nat_rec P ?base ?pas` avec les trous typés | le squelette est typé ; `induction_resolue()` vérifie le terme fermé après substitution des branches |
| `simplifier(t)` | terme réduit | **non** soumis à `verifier` : voir §5 |

`refl : Π(A:type)(x:A), x = x` est une constante de signature. `symm : x=y -> y=x`
est un **terme dérivé** de `eq_rec`, lui aussi vérifié par le noyau (`ta_terme_symm()`).

## 3. Signature (axiomes explicites)

Le noyau n'a pas de ι-réduction pour les inductifs et pas de règle de récursion.
Les constantes suivantes sont donc des axiomes de signature, avec leur type déclaré :

- `Nat` (type inductif `ind`), `zero : Nat`, `succ : Nat -> Nat`,
  `plus : Nat -> Nat -> Nat` ;
- `Eq A a b` (type inductif `ind` à 3 paramètres) ;
- `refl`, `eq_rec`, `nat_rec` (principes de réflexivité et d'élimination) ;
- `plus_0_l : Πn, Eq (plus zero n) n` et
  `plus_s_l : Πm n, Eq (plus (succ m) n) (succ (plus m n))` :
  **équations computationnelles de `plus`**, non dérivables par le noyau.

Les lemmes `symm`, `trans`, `congr_succ`, `congr_plus_l`, `plus_zero_d`, `plus_succ_d`
ne sont PAS des axiomes : leurs **termes** sont construits dans `tactiques.rose` et
vérifiés par le noyau dans `demo_lemmes()`. Leurs constantes opaques ne sont
utilisées dans les démos qu'après ce contrôle (usage = application d'un théorème
déjà vérifié, pas une preuve codée en dur).

## 4. Démonstrations vérifiées

`tests_tactiques.rose` construit chaque terme, appelle le noyau et affiche OK/ÉCHEC.
Sortie réelle (`./rose/rose thorn/tests_tactiques.rose`) :
**19 réussis / 0 échec** pour la suite historique, **14 réussis / 0 échec** pour
la suite supplémentaire de la capacité `exact`/`apply`, soit
**33 réussis / 0 échec** au total. Les compteurs sont affichés séparément :
`REUSSIS=19`, `REUSSIS_SUP=14`, `TOTAL_REUSSIS=33`. Les 19 cas historiques et
leurs libellés n'ont pas été modifiés ; le garde-fou `tests_regression.sh`
(qui épingle `REUSSIS=19`) reste donc vert sans être touché.

Les quatre preuves `.thn` correspondent à des termes effectivement vérifiés :

- `identite.thn` → `λA. λx. x` : `demo_identite()` — OK ;
- `reflexivite.thn` → `λA. λx. refl A x` : `demo_reflexivite()` — OK ;
- `associativite.thn` → `nat_rec P base pas` : `demo_associativite()` — OK ;
- `commutativite.thn` → `nat_rec P base pas` : `demo_commutativite()` — OK.

`rose verif thorn/src/tactiques.rose` → `vérifié` ; idem pour `tests_tactiques.rose`.

**Les fichiers `.thn` ne sont pas encore analysés** : le lexeur est en PROV2 et le
pilote `thorn <fichier.thn>` en PROV4, qui ne sont pas livrés ici. Ce sont des
spécifications déclarantives ; leur contenu est vérifié par les constructions Rose
équivalentes de `tests_tactiques.rose`. Ne pas les considérer comme exécutables
aujourd'hui.

### 4b. Capacité réelle ajoutée : `exact` / `apply` avec recherche transitive

`application` (§2) restait superficielle : une correspondance exacte, puis au
plus 1 ou 2 applications dont les arguments devaient déjà être des entrées du
contexte. La nouvelle capacité fait une **déduction arrière** :

1. `exact(but, ctx)` cherche une hypothèse dont le type est convertible au but.
2. `apply(but, ctx)` considère chaque « tête » (hypothèse du contexte **ou** lemme
   vérifié : `symm`, `trans`, `congr_succ`, `plus_0_l`, …). Si son type est une
   chaîne de Π, les lieurs sont remplacés par des **méta-variables fraîches**.
3. Une **unification légère** (`ta_unifie`) compare la conclusion ainsi
   instanciée au but. Un méta se lie à un sous-terme entier de la cible ; tout
   le reste est une comparaison structurelle (β appliqué des deux côtés).
   Il y a un contrôle d'occurrence et les liaisons sont copiées avant chaque
   essai, pour qu'un échec partiel ne corrompe pas les liaisons acquises.
4. Avant de résoudre les prémisses, un méta déjà lié reçoit le **type synthétisé
   par le noyau** pour son terme. C'est ce qui infère un argument implicite :
   par exemple le `A` de `symm : Π(A)(a)(b), a=b -> b=a` est déduit du fait que
   `a` est `zero`, dont le type synthétisé est `Nat`.
5. Chaque prémisse encore ouverte est résolue **récursivement** par `apply`
   (profondeur bornée à 3, au plus 6 Π par tête), puis l'application complète
   est construite et soumise à `ta_verifie()` = `verifier()` + `convertible()`.

Ce qui est réellement démontré par les 14 tests supplémentaires :

- une chaîne `f 0 h0` puis `f (succ 0) (f 0 h0)` : `f` est appliqué **deux fois**
  (le test compte les occurrences de `f` en tête dans le terme produit), ce que
  l'ancienne recherche à 1–2 pas ne savait pas faire ;
- l'inférence de l'argument implicite de `symm` ;
- l'emploi de `plus_0_l` et `congr_succ` comme lemmes vérifiés, pas comme
  valeurs admises ;
- deux **refus attendus** : `exact` sur un but absent du contexte et `apply`
  sans aucune preuve possible. La tactique rend `ok = faux` avec une raison ;
- chaque terme accepté est **re-vérifié une seconde fois** par un contrôle
  indépendant (`ta_verifie` appliqué au champ `terme`).

Aucun terme n'est accepté sur la seule foi d'une correspondance de structure :
c'est toujours le noyau qui synthétise son type et qui le compare au but.

## 5. Partie expérimentale / simplifiée — lisible sans complaisance

1. **Induction : réel.** Le type de `nat_rec` encode le principe d'élimination
   dépendant : `Π(P:Πm:Nat,type), P zero -> (Πn, P n -> P (succ n)) -> Πm, P m`.
   Le noyau vérifie **réellement**, via sa règle `App` et `convertible`, que la
   branche de base a le type `P zero` et que la branche de pas a le type
   `Π n, P n -> P (succ n)`. `induction_resolue()` contrôle le terme fermé.
2. **Induction : simplifié.** `nat_rec` reste un axiome : aucun contrôle de
   terminaison, de positivité ou de cohérence des constructeurs. Le noyau ne sait
   pas qu'il s'agit d'un vrai type inductif ; il ne check que les types des
   applications.
3. **Inductifs approximés.** `Nat` et `Eq` sont des termes `ind` : le noyau vérifie
   que leurs paramètres synthétisent un type, mais **pas** que les deux membres
   d'un `Eq A a b` ont effectivement le type `A`. La formation de `Eq` est donc
   sur-permissive.
4. **Pas de ι-réduction dans le noyau.** `beta`/`convertible` ne font que β + α.
   C'est pourquoi `0+n -> n` et `succ m+n -> succ(m+n)` sont des axiomes
   (`plus_0_l`, `plus_s_l`) au lieu d'être calculés. Une addition réellement
   récursive n'a pas pu être définie : l'encodage de Church exigerait
   `Nat : Type(1)` passé comme `X : Type(0)`, ce que le noyau prédicatif refuse.
5. **`simplifier` va au-delà du noyau.** Il applique `beta` du noyau puis une passe
   locale de réduction des constructeurs pour `plus` (carburant 200). Le résultat
   n'est **pas** garanti `convertible` au terme initial pour le noyau : c'est une
   conversion étendue, distincte de celle du noyau. Elle n'est pas utilisée pour
   valider les démos.
6. **Recherche : ancienne limite, nouvelle capacité, limites restantes.**
   `symetrie` exige toujours une hypothèse de type exact (modulo β/α) et
   `application` reste limitée à 1 ou 2 applications : ces deux tactiques
   historiques n'ont pas été unifiées (consigne explicite). La nouvelle
   capacité `apply` (§4b) fait la recherche transitive et l'inférence
   d'arguments implicites, mais **ce n'est pas une unification générale** :
   - les méta-variables ne se lient qu'à des sous-termes du but ou de types
     déjà synthétisés ; il n'y a ni résolution d'équations, ni types
     dépendants résolus, ni recherche de méta dans le but lui-même ;
   - une prémisse qui contient encore un méta libre n'est pas résolue (le
     candidat est alors rejeté) : l'intermédiaire d'une chaîne transitive doit
     être déterminé par la conclusion ou par le type d'un argument connu ;
   - profondeur 3 et 6 Π par tête : au-delà, la tactique échoue proprement
     au lieu de boucler ;
   - les lemmes utilisés comme têtes sont ceux dont le terme a été vérifié
     dans `demo_lemmes()`. Aucune constante opaque n'est acceptée sans que
     le noyau vérifie l'application complète.
7. **Pilotage manuel.** Les tactiques qui ouvrent des buts (`introduction`,
   `induction`) rendent des trous typés et un contexte mis à jour ; elles ne résolvent
   pas les buts résiduelles seules. Les branches des démos sont fournies
   explicitement par le code Rose.
8. **Univers unique.** Tout est écrit à `Type(0)` ; pas de polymorphisme
   d'univers, pas de cumulativité.
9. **`Resultat` étendu.** En plus de `{ok, type, erreur}` (SPEC.md), les résultats
   portent `terme`, `buts`, `ctxs` (table d'indices), `regle`, et pour
   `introduction`/`induction` les noms des trous. `ctxs` est une table d'indices
   et non une `liste<liste<table>>` (Rose ne type pas les listes imbriquées).

## 6. Honnêteté

Aucune tactique ne rend un `ok` sans terme soumis au noyau — y compris `exact` et
`apply`, qui passent tous deux par `ta_verifie()` (`verifier()` puis
`convertible()`). Aucune démo n'est une valeur `vrai` codée en dur. Les seules
concessions sont les axiomes de la §3 et les limites de la §5, toutes explicites,
notamment le fait que l'unification de `apply` est légère et non générale.
`rose verif` passe ; les 19 cas historiques et les 14 cas supplémentaires ont
été exécutés et leur sortie réelle est reproduite ci-dessus.
