# Thorn — noyau de vérification de types

Implémentation en Rose du noyau décrit dans `thorn/SPEC.md`.
Fichiers livrés : `thorn/src/noyau.rose`, `thorn/tests_noyau.rose`, ce document.
Aucun autre fichier n'a été modifié. Aucun commit, aucune publication.

## Vérification

```
cd /home/clausake/Stefan/ROSE
./rose/rose verif thorn/src/noyau.rose        # → vérifié
./rose/rose verif thorn/tests_noyau.rose      # → vérifié
./rose/rose thorn/tests_noyau.rose            # → REUSSIS=35, ECHOUES=0, RESULTAT: OK
```

## Interface publique

| Fonction | Signature SPEC | Résultat |
|---|---|---|
| `verifier(t, ctx)` | `-> Resultat` | table `{ok: booleen, type: Terme, erreur: chaine}` |
| `beta(t)` | `-> Terme` | forme normale beta (stratégie normale), garde 10 000 pas |
| `convertible(a, b)` | `-> bool` | beta(a) et beta(b) puis alpha-équivalence |
| `bien_forme(t, ctx)` | `-> bool` | vrai si `verifier` accepte le terme (et que son genre est connu) |

## Représentation

`Terme` est une table Rose avec un champ `genre`, conforme à la SPEC :
`var`, `type`, `pi`, `lam`, `app`, `const`, `ind`, `constructeur`, `elim`.
Sentinelle interne `{genre:"_absent"}` = champ manquant / variable non liée.

`Contexte` est une **liste de tables** `{nom: chaine, type: Terme}`, rembourrée
à l'index 0 par `table()` (le vérificateur Rose refuse `[]`). Les entrées
vont de 1 à `longueur(ctx)-1`. `noy_ctx_vide()` et `noy_ctx_ajouter(ctx,n,ty)`
construisent un contexte sans détruire le précédent. La recherche se fait de
la fin vers le début : la liaison la plus récente masque les anciennes.
Par interopérabilité, `verifier` accepte aussi une table `{nom: Terme}` :
`noy_normaliser_ctx` la convertit.

Les champs `params` (`ind`) et `branches` (`elim`) sont des `liste<table>`
eux aussi rembourrés à l'index 0.

Tous les locaux du noyau sont préfixés `$nx_`. Raison mesurée : le
vérificateur Rose tire le type des variables **par nom, tous scopes
confondus** ; un global appelant `$c` entrait en collision avec le local
`$c` de `noy_normaliser_ctx` et faisait échouer `rose verif` du programme
importateur. Le préfixe élimine cette classe de collision.

## Règles implémentées

- `Type(n) : Type(n+1)` ; `n < 0` rejeté.
- `Var` : son type est lu dans le contexte ; sinon erreur `variable non liee: …`.
- `Πx:A.B` : `A : Type(i)` requis ; `B` vérifié sous `ctx + x:A` ;
  le type du Π est `Type(max(i,j))` où `B : Type(j)`.
- `λx:A.t` : `A : Type(i)` requis ; `t` synthétisé sous `ctx + x:A` ;
  le type rendu est `Πx:A.(type de t)`.
- `App(f,a)` : le type de `f` est beta-normalisé et doit être un `Π` ;
  le type de `a` doit être ** convertible ** au domaine ; le résultat est
  `codomaine[a/nom]` par substitution capture-évitante.
- `const` / `constructeur` : l'annotation `type` est vérifiée comme type
  bien formé, puis rendue comme type du terme.
- `ind` / `elim` : voir « Limites ».

La garde de pas (10 000) est vérifiée à l'entrée de la synthèse et à chaque
beta-normalisation. Une erreur explicite est rendue dans `Resultat.erreur`
(notamment dans la règle `App`). Aucun terme, même divergant, ne fait boucler
le vérificateur.

## Beta-réduction

`noy_redex` contrôle **un pas** en stratégie normale (le plus à gauche,
dehors) : fonction avant argument, domaine avant codomaine/corps, et
descente dans les annotations de type, `params` et `branches`.
`noy_beta_lim(t, limite)` répète ce pas jusqu'à forme normale, en comptant.
À la limite, il rend `{terme partiellement réduit, pas, erreur}` : il n'y a
ni boucle infinie ni plante.

`beta(t)` a la signature imposée par la SPEC (`-> Terme`) et ne peut donc pas
transmettre l'erreur : en cas de dépassement, il rend le terme partiellement
réduit. C'est `verifier` qui expose le message d'erreur.

## Substitution et alpha-équivalence

`noy_subst(t, nom, val)` remplace les occurrences **libres** de `nom`.
- `var` : remplacement ; `type`/sentinelle : inchangé.
- `app`, `const`, `constructeur`, `ind`, `elim` : descente récursive.
- `pi` / `lam` : le domaine est substitué (hors portée du lieur) ; le corps
  ne l'est que si le lieur n'a pas le même nom.
- **Évitation de capture** : si `nom` est libre dans le corps et que le nom
  du lieur est libre dans `val`, le lieur est renommé en un nom frais
  (suffixe `'` répété) choisi pour n'apparaître **nulle part** dans le terme
  (lieurs compris) ni parmi les variables libres de `val`. On renomme le
  corps avant de substituer. Le renommage est évité quand la substitution
  n'a pas lieu, ce qui garde les types affichés stables.

`convertible` beta-normalise les deux termes puis les compare avec
`noy_alpha`, qui porte deux environnements parallèles de lieurs
(`ea`, `eb`). Un `var` lié est comparé par son indice de profondeur dans
l'environnement (équivalence alpha), un `var` libre par son nom. Les genres,
niveaux, noms de constantes, listes de paramètres/branches sont comparés
structurellement.

## Tests (35 assertions, toutes vertes)

Le harnais est dans `thorn/tests_noyau.rose` ; il importe le noyau avec
`importer "thorn/src/noyau.rose"`. Couverture réelle :

1. `λx:X. x` accepté, type `Πx:X. X`, sans erreur.
2. Application à un argument ; application dépendante
   `(λA:Type(0). λx:A. x) X : Πx:X. X`.
3. Alpha : `λx:X.x ≡ λy:X.y ≡ λz:X.z`, lieur libre distinct, cas imbriqués.
4. Capture-évitation : `(λx:X. λy:X. x) y` donne `λy':X. y` (et non
   `λy:X. y`) ; vérification du terme et de son type ; substitution directe
   avec masquage et renommage.
5. Erreurs : application d'un `Type(0)`, variable non liée, terme mal formé,
   `bien_forme` sur terme mal formé, beta bloquée stable.
6. Univers `Type(0):Type(1)`, Pi `Type(max)` (`Type(2)`).
7. Garde de pas sur Ω : erreur explicite, pas ≤ 50.
8. Contexte fourni sous forme de table.

Une sonde complémentaire (hors fichiers livrés) a vérifié que des termes
délibérément mal formés — `genre` numérique, `nom` numérique, `domaine`
chaîne, `fonction` entier — rendent `ok = faux` sans jamais planter.

## Limites honnêtes

- **`ind` / `elim` ne sont pas des règles complètes.** Leurs sous-termes sont
  synthétisés pour validation structurelle, mais le type rendu est toujours
  `Type(0)`. Pas de motifs, d'indices, de motifs de récursion ni de
  convertibilité spécifique aux inductifs. Ne pas s'appuyer dessus pour un
  vrai système inductif.
- **Pas de cumulativité** (`Type(0)` n'est pas traité comme sous-type de
  `Type(1)`), pas d'η-équivalence, pas de types `let`, pas de récursion
  générale.
- **`convertible` n'utilise pas le contexte** : c'est une conversion
  définitionnelle pure (beta + alpha), pas une conversion avec hypothèses.
- **Garde de pas approximative dans la détection d'univers** :
  `noy_univers` utilise sa propre limite de 1 000 pas et ne réinjecte pas
  ces pas dans le compteur principal. Le plafond global de 10 000 reste
  garanti pour la beta-réduction et la synthèse elle-même.
- **En cas de dépassement de la garde dans `convertible`**, la comparaison
  porte sur des termes partiellement normalisés : le résultat peut alors
  être inexact (mais sans jamais planter). Le test de garde vérifie l'erreur
  explicite de `noy_beta_lim`, pas la sémantique dans ce régime.
- **Pas de partage de structure** : chaque constructeur alloue une nouvelle
  table Rose. Les termes très grands peuvent être lents à normaliser.
- Le préfixage `$nx_` est une contrainte du vérificateur Rose actuel, pas
  un choix esthétique : retirer le préfixe réintroduirait les collisions de
  noms avec les programmes importateurs.
