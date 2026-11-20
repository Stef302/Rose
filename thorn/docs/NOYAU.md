# Thorn — noyau de vérification de types

Implémentation en Rose du noyau décrit dans `thorn/SPEC.md`.
Fichiers livrés (mission TH4) : `thorn/src/noyau.rose`, `thorn/tests_noyau.rose`,
ce document. Aucun autre fichier n'a été modifié. Aucun commit, aucune publication.

## Vérification (commandes réellement exécutées)

```
cd /home/clausake/Stefan/ROSE
./rose/rose verif thorn/src/noyau.rose        # → vérifié
./rose/rose verif thorn/tests_noyau.rose      # → vérifié
./rose/rose thorn/tests_noyau.rose            # → REUSSIS=88, ECHOUES=0, RESULTAT: OK
```

Non-régression (rejoué après chaque bloc) : 5 preuves `.thn` (5/5), 10 lemmes de
`bibliotheque.rose` (10/0), 8 tests syntaxe (8/0), 19 tests tactiques (19/0),
12 tests pilote (12/0).

## Interface publique

| Fonction | Signature SPEC | Résultat |
|---|---|---|
| `verifier(t, ctx)` | `-> Resultat` | `{ok, type, erreur, code, contexte, ligne, pas}` |
| `verifier_lim(t, ctx, limite)` | extension | idem, avec garde paramétrée (tests de la garde) |
| `beta(t)` | `-> Terme` | forme normale (β **et ι**), stratégie normale, garde 10 000 |
| `convertible(a, b)` | `-> bool` | β+ι puis α-équivalence **stricte** (pas de cumulativité) |
| `bien_forme(t, ctx)` | `-> bool` | vrai si `verifier` accepte (et genre connu) |
| `cumulatif(a, b)` | extension | conversion stricte OU `Type(i) ≤ Type(j)` si `i ≤ j` |
| `noy_code(r)`, `noy_contexte(r)`, `noy_ligne_res(r)`, `noy_pas_res(r)` | extension | accès défensifs au Resultat |

Les champs historiques `ok`, `type`, `erreur` sont inchangés : tous les
consommateurs existants (tactiques, pilote, bibliothèque) fonctionnent sans
modification.

## Représentation

`Terme` est une table Rose avec un champ `genre`, conforme à la SPEC :
`var`, `type`, `pi`, `lam`, `app`, `const`, `ind`, `constructeur`, `elim`.
Sentinelle interne `{genre:"_absent"}` = champ manquant / variable non liée.

Un terme peut **facultativement** porter un champ `ligne` (entier). Le noyau ne
l'exige jamais ; s'il est présent et qu'une erreur est produite sous ce terme,
le numéro est remonté dans `Resultat.ligne` (le plus interne gagne). Sans ce
champ, `Resultat.ligne = 0`. C'est le mécanisme concret de « ligne/contexte »
demandé : les termes construits par le parseur actuel n'ont pas de position
source, mais un outil peut désormais en fournir une sans casser le noyau.

`Contexte` est une **liste de tables** `{nom, type}` rembourrée à l'index 0 par
`table()` ; `noy_ctx_vide()` / `noy_ctx_ajouter()` la construisent. La recherche
va de la fin vers le début (masquage). `verifier` accepte aussi une table
`{nom: Terme}` (`noy_normaliser_ctx`). Les listes `params`/`branches` sont
elles aussi rembourrées à l'index 0.

Tous les locaux sont préfixés `$nx_` : le vérificateur Rose tire le type des
variables par nom tous scopes confondus ; le préfixe évite les collisions avec
les programmes importateurs.

## Règles implémentées

- `Type(n) : Type(n+1)` ; `n < 0` rejeté (`NIVEAU_NEGATIF`).
- `Var` : type lu dans le contexte ; sinon `VAR_NON_LIE`.
- `Πx:A.B` : `A : Type(i)` et `B : Type(j)` exigés (contrôle par
  `noy_univers_res` avec la garde **de la synthèse en cours**, pas un plafond
  fixe de 1 000 ; les pas sont réinjectés) ; type rendu `Type(max(i,j))`.
  Domaine ou codomaine non univers → `UNIVERS`.
- `λx:A.t` : même contrôle de niveau sur `A` ; type rendu `Πx:A.(type de t)`.
- `App(f,a)` : le type de `f` est βι-normalisé (doit être un `Π`, sinon
  `APP_PAS_PI`) ; le type de `a` est comparé au domaine par **cumulativité
  contrôlée** (`noy_cumul_res`) ; échec → `APP_ARG` (ou `CUMUL` si les deux
  types sont des univers dans le mauvais sens). Résultat `codomaine[a/nom]`.
- `const` / `constructeur` : annotation vérifiée comme type bien formé
  (`ANNOTATION` sinon), puis rendue comme type du terme.
- `ind` : paramètres synthétisés (`IND_PARAM`), **plus contrôle de formation
  des égalités** : `ind("=",[a,b])` exige que `a` et `b` aient des types
  convertibles ; `ind("Eq",[A,a,b])` exige `a : A` et `b : A`. Échec →
  `EQ_MAL_TYPE`. La sur-permissivité historique (« a et b non typés par A »)
  est supprimée pour ces deux formes. Les autres inductifs restent
  structurels (voir Limites).
- `elim` : motif et branches synthétisés (`ELIM_*`), type rendu `Type(0)`.

### Cumulativité

Elle est **délibérément étroite** : `Type(i) ≤ Type(j)` ssi `i ≤ j`, et
uniquement dans la règle `App` (contrôle argument/domaine). Toute autre paire
de types doit être βια-convertible. La fonction publique `convertible` reste
stricte — `convertible(Type(0),Type(1)) = faux` — tandis que
`cumulatif(Type(0),Type(1)) = vrai` et `cumulatif(Type(1),Type(0)) = faux`.
Aucune sous-typage implicite n'est introduit ailleurs (pas de subsumption
générale, pas de cumulativité sur les Π).

## Réduction : β **et ι**

`noy_redex` essaie d'abord l'ι, puis applique la stratégie normale habituelle
(fonction avant argument, domaine avant codomaine/corps, descente dans les
annotations, `params` et `branches`). `noy_beta_lim(t, limite)` répète jusqu'à
forme normale en comptant ; à la limite il rend `{terme partiel, pas, erreur}`.
`beta(t)` rend le terme (signature SPEC `-> Terme`) ; c'est `verifier` qui
expose l'erreur de garde.

Les deux primitives reconnues par l'ι sont désignées **par leur nom ET par la
forme de leur annotation de type** (`noy_pi_domaine_ieme`) : un `const`
arbitraire nommé `plus` mais mal typé n'est jamais réduit (test dédié).

| Primitive | Motif | Réduction |
|---|---|---|
| `plus : Πm:Nat. Πn:Nat. Nat` | `plus zero n` | `n` |
| `plus : Πm:Nat. Πn:Nat. Nat` | `plus (succ k) n` | `succ (plus k n)` |
| `nat_rec : ΠP.Πbase.Πpas.Πm:Nat. P m` | `nat_rec P base pas zero` | `base` |
| `nat_rec : ΠP.Πbase.Πpas.Πm:Nat. P m` | `nat_rec P base pas (succ k)` | `pas k (nat_rec P base pas k)` |

Les pas d'ι sont comptés comme les pas de β (test : 2 pas pour
`plus (succ zero) n`). `convertible` voit ces équations :
`plus zero n ≡ n` et `plus (succ m) n ≡ succ (plus m n)` sont vraies. Les
constantes `plus_0_l` / `plus_s_l` de la bibliothèque restent présentes (fichier
hors périmètre TH4) mais leurs équations sont désormais **calculées** par le
noyau : elles ne sont plus nécessaires à la réduction, seulement conservées
pour les preuves existantes.

## Résultats structurés

`noy_resultat_plein` est l'unique constructeur. Chaque Resultat porte :

| Champ | Type | Sens |
|---|---|---|
| `ok` | booleen | acceptation |
| `type` | Terme | type synthétisé, ou sentinelle `_absent` |
| `erreur` | chaine | message lisible |
| `code` | chaine | identifiant stable |
| `contexte` | chaine | règle ayant échoué (`Var`, `Pi/domaine`, `App`, `ind(Eq)/b`, …) |
| `ligne` | entier | champ `ligne` du terme fautif, 0 si absent |
| `pas` | entier | pas consommés |

Codes produits : `OK`, `GARDE_PAS`, `VAR_NON_LIE`, `NIVEAU_NEGATIF`, `UNIVERS`,
`APP_PAS_PI`, `APP_ARG`, `CUMUL`, `ANNOTATION`, `IND_PARAM`, `EQ_MAL_TYPE`,
`GENRE_INCONNU` (plus `ERREUR`, repli de l'ancien `noy_err`, non utilisé par la
synthèse). Tous les accesseurs sont défensifs : une table vide, un champ absent
ou un champ de mauvais type rendent une valeur par défaut, jamais une panique.

La garde remonte désormais de trois endroits auparavant silencieux :
`noy_synth0` (entrée), `noy_univers_res` (détection d'univers, avec la limite
restante et réinjection des pas) et `noy_cumul_res` (conversion). `verifier_lim`
permet de la tester avec une petite limite ; `verifier` garde le plafond de
10 000 de la SPEC.

## Substitution et α-équivalence

`noy_subst` remplace les occurrences libres. `pi`/`lam` : le domaine est substitué,
le corps ne l'est que si le lieur n'a pas le même nom ; évitation de capture par
renommage du lieur en un nom frais (suffixe `'`) choisi pour n'apparaître nulle
part dans le terme ni dans les libres de `val`. `noy_alpha` porte deux
environnements parallèles de lieurs : un `var` lié est comparé par indice de
profondeur, un `var` libre par son nom. `convertible` βι-normalise puis compare.

## Tests (88 assertions, toutes vertes)

Le harnais `thorn/tests_noyau.rose` importe le noyau. Les 35 assertions
d'origine sont conservées à l'identique ; 53 s'y ajoutent :

1–8. Tests d'origine : identité, application/dépendance, α, capture-évitation,
erreurs, univers `Type(0):Type(1)` / `Type(2)`, garde sur Ω, contexte-table.
9. Résultats structurés : `code OK`, présence des 4 champs, `VAR_NON_LIE`,
`APP_PAS_PI`, `GENRE_INCONNU`, contexte, accesseurs sur table vide, remontée de
`ligne` (42), absence (0).
10. Garde : `verifier_lim(...,0)` → `GARDE_PAS`/contexte `synthese` ;
`noy_univers_res(Ω,5)` et `noy_convertible_res(Ω,Ω,5)` → erreur remontée, code
`GARDE_PAS`, pas ≤ 5.
11. Cumulativité : strict vs cumulatif, `Type(0)≤Type(1)`, refus de
`Type(1)≤Type(0)`, application acceptée avec type `Type(1)`, descente refusée
(`CUMUL`), domaine de `Pi` non univers (`UNIVERS`).
12. Eq typé : `=` à deux paramètres accepte deux membres de même type et refuse
des types différents (`EQ_MAL_TYPE`) ; `Eq(A,a,b)` accepte `a,b:X` et refuse un
second membre hors de `X` (`ind(Eq)/b`).
13. ι-réduction : les 4 équations (zero/succ pour `plus`, zero/succ pour
`nat_rec`), comptage des pas, garde-fous sur `const` mal typé (type et codomaine), préservation du
type (`Ind(Nat)`), convertibilité des deux équations de `plus`, `nat_rec`
vérifié avec type `P(zero) = Ind(Nat)`.

Une sonde complémentaire (hors fichiers livrés) vérifiait déjà que des termes
délibérément mal formés (`genre` numérique, `domaine` chaîne, …) rendent
`ok = faux` sans planter.

## Limites honnêtes (état après TH4)

- **`ind` / `elim` restent approximatifs pour les autres inductifs.** Leurs
  sous-termes sont vérifiés, mais le type rendu reste `Type(0)` ; pas de motifs,
  d'indices, de positivité ni de vérification de terminaison structurelle.
  Seules les deux formes d'égalité ont été durcies (contrôle des membres).
- **ι étroite.** Les motifs zero/succ sont reconnus pour `plus` et `nat_rec` par
  nom + forme du type. Ce n'est PAS un mécanisme d'éliminateur générique
  (pas de motif utilisateur, pas d'indices, pas de récursion imbriquée, pas de
  contrôle de terminaison). Un `nat_rec` appliqué à un motif non constructeur
  reste bloqué, ce qui est correct.
- **`plus_0_l` / `plus_s_l` n'ont pas été supprimés** de `tactiques.rose` /
  `bibliotheque.rose` : ces fichiers sont hors périmètre. Leur contenu
  computationnel est désormais fait par le noyau ; les preuves qui les citent
  restent valides et ont été rejouées (10/10 lemmes, 5/5 preuves).
- **Cumulativité minimale.** Uniquement `Type(i) ≤ Type(j), i ≤ j`, dans la
  règle `App`. Pas de subsumption dans les Π, les annotations ou ailleurs ;
  `convertible` public reste strict.
- **`convertible` n'utilise pas le contexte** : conversion définitionnelle pure
  (βι + α), pas de conversion avec hypothèses. Si sa garde (10 000 pas) est
  épuisée, il rend `faux` plutôt que de comparer des termes partiellement
  normalisés (le message est disponible via `noy_convertible_res`).
- **Pas d'η-équivalence, pas de `let`, pas de récursion générale**, pas de
  vérification de termination des définitions utilisateur.
- **Pas de partage de structure** : chaque constructeur alloue une table Rose ;
  les très grands termes peuvent être lents (la garde borne le temps, pas la
  mémoire).
- Le préfixage `$nx_` est une contrainte du vérificateur Rose actuel, pas un
  choix esthétique.
