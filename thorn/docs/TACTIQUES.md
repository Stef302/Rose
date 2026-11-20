# Thorn — tactiques (PROV3) : ce qui est réel, ce qui est simplifié

Fichiers livrés : `thorn/src/tactiques.rose`, `thorn/tests_tactiques.rose`,
`thorn/preuves/{identite,reflexivite,associativite,commutativite}.slg`,
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
Sortie réelle (`./rose/rose thorn/tests_tactiques.rose`) : **19 réussis / 0 échec**.

Les quatre preuves `.slg` correspondent à des termes effectivement vérifiés :

- `identite.slg` → `λA. λx. x` : `demo_identite()` — OK ;
- `reflexivite.slg` → `λA. λx. refl A x` : `demo_reflexivite()` — OK ;
- `associativite.slg` → `nat_rec P base pas` : `demo_associativite()` — OK ;
- `commutativite.slg` → `nat_rec P base pas` : `demo_commutativite()` — OK.

`rose verif thorn/src/tactiques.rose` → `vérifié` ; idem pour `tests_tactiques.rose`.

**Les fichiers `.slg` ne sont pas encore analysés** : le lexeur est en PROV2 et le
pilote `thorn <fichier.slg>` en PROV4, qui ne sont pas livrés ici. Ce sont des
spécifications déclarantives ; leur contenu est vérifié par les constructions Rose
équivalentes de `tests_tactiques.rose`. Ne pas les considérer comme exécutables
aujourd'hui.

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
6. **Recherche limitée.** `symetrie` exige une hypothèse de type exact (modulo β/α).
   `application` cherche une correspondance exacte puis 1 ou 2 applications ; il n'y
   a ni unification générale, ni synthèse d'arguments.
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

Aucune tactique ne rend un `ok` sans terme soumis au noyau. Aucune démo n'est une
valeur `vrai` codée en dur. Les seules concessions sont les axiomes de la §3 et
les limites de la §5, toutes explicites. `rose verif` passe ; les 19 cas de test
ont été exécutés et leur sortie réelle est reproduite ci-dessus.
