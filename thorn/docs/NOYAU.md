# Thorn — noyau de vérification de types

Implémentation en Rose du noyau décrit dans `thorn/SPEC.md`.
Fichiers livrés : `thorn/src/noyau.rose`, `thorn/tests_noyau.rose`, ce document.
Aucun autre fichier n'a été modifié. Aucun commit, aucune publication.

Itération en cours (Thorn pro) : généralisation des éliminateurs en **registre
de descripteurs d'inductifs** (`noy_registre`) et ajout d'un troisième inductif,
`List` avec `nil`/`cons` et l'éliminateur dépendant `list_rec`. Les 126 tests
antérieurs sont conservés ; 39 s'y ajoutent (165 au total).

## Vérification (commandes réellement exécutées)

```
cd /home/clausake/Stefan/ROSE
./rose/rose verif thorn/src/noyau.rose        # → vérifié
./rose/rose verif thorn/tests_noyau.rose      # → vérifié
./rose/rose thorn/tests_noyau.rose            # → REUSSIS=165, ECHOUES=0, RESULTAT: OK
```

Non-régression (rejoué après chaque bloc) : 5 preuves `.thn` (5/5), 10 lemmes de
`bibliotheque.rose` (10/0), 34 lemmes d'`algebre.rose` (34/0), 8 tests syntaxe
(8/0), 33 tests tactiques (19 + 14, 0 échec), 12 tests pilote (12/0). Non-régression
globale : `bash thorn/tests_regression.sh` → 17/17 cas, BILAN: OK.

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
| `noy_est_bool`, `noy_est_true`, `noy_est_false`, `noy_est_motive_bool`, `noy_bool_rec_valide`, `noy_not_valide` | extension | contrôle de structure du second inductif (Bool) |
| `noy_registre()`, `noy_registre_trouver_elim(reg, nom)`, `noy_registre_trouver_constr(desc, nom)` | extension | registre interne des 3 inductifs (Nat/Bool/List) |
| `noy_est_ind_nom`, `noy_resultat_est_ind`, `noy_registre_motive`, `noy_registre_elim_valide` | extension | prédicats génériques de contrôle de structure |

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
  est supprimée pour ces deux formes. `ind("Bool",[])` est reconnu comme
  type de donnée (annotation des constructeurs `true` / `false`). Les autres
  inductifs restent structurels (voir Limites).
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

Les primitives reconnues par l'ι sont désignées **par leur nom ET par la
forme de leur annotation de type** : les éliminateurs par le registre
(`noy_registre_elim_valide`, `noy_resultat_est_ind`), les fonctions calculées
`plus` / `not` par les prédicats dédiés (`noy_bool_rec_valide` / `noy_not_valide`
/ `noy_est_motive_bool`, conservés) et `noy_pi_domaine_ieme`. Un `const`
arbitraire nommé `plus`, `bool_rec`, `list_rec` ou `not` mais mal typé n'est
jamais réduit (tests dédiés).

| Primitive | Motif | Réduction |
|---|---|---|
| `plus : Πm:Nat. Πn:Nat. Nat` | `plus zero n` | `n` |
| `plus : Πm:Nat. Πn:Nat. Nat` | `plus (succ k) n` | `succ (plus k n)` |
| `nat_rec : ΠP.Πbase.Πpas.Πm:Nat. P m` | `nat_rec P base pas zero` | `base` |
| `nat_rec : ΠP.Πbase.Πpas.Πm:Nat. P m` | `nat_rec P base pas (succ k)` | `pas k (nat_rec P base pas k)` |
| `not : Πb:Bool. Bool` | `not true` | `false` |
| `not : Πb:Bool. Bool` | `not false` | `true` |
| `bool_rec : ΠP:(Πb:Bool.Type).Πt:(P true).Πf:(P false).Πb:Bool. P b` | `bool_rec P t f true` | `t` |
| `bool_rec : ΠP:(Πb:Bool.Type).Πt:(P true).Πf:(P false).Πb:Bool. P b` | `bool_rec P t f false` | `f` |
| `list_rec : ΠP.Πbase.Πstep.Πl:List(A). P l` | `list_rec P base step nil` | `base` |
| `list_rec : ΠP.Πbase.Πstep.Πl:List(A). P l` | `list_rec P base step (cons a l)` | `step a l (list_rec P base step l)` |

Les pas d'ι sont comptés comme les pas de β (test : 2 pas pour
`plus (succ zero) n`). `convertible` voit ces équations :
`plus zero n ≡ n` et `plus (succ m) n ≡ succ (plus m n)` sont vraies. Les
constantes `plus_0_l` / `plus_s_l` de la bibliothèque restent présentes (fichier
hors périmètre TH4) mais leurs équations sont désormais **calculées** par le
noyau : elles ne sont plus nécessaires à la réduction, seulement conservées
pour les preuves existantes.

### Second inductif : Bool

`Bool` est le deuxième inductif à disposer d'une ι-réduction (après `Nat`).
Ses constructeurs `true` / `false` sont des `const` dont l'annotation doit être
`Ind(Bool)` ; un `true` annoté autrement n'est pas reconnu. L'éliminateur
dépendant `bool_rec` porte une annotation à **quatre flèches**, contrôlée par
`noy_bool_rec_valide` :

1. un motif `P` bien formé (`Πb:Bool. Type(i)`, vérifié par
   `noy_est_motive_bool`) ;
2. une branche `t` pour `true` ;
3. une branche `f` pour `false` ;
4. un scrutinee `b` dont le domaine annoncé est `Ind(Bool)`.

Le résultat de `bool_rec` est la branche choisie, **sans reconstruction de
terme arbitraire**. La négation `not` reconstruit le constructeur opposé en
réutilisant l'annotation `Ind(Bool)` du scrutinee. Un scrutinee variable reste
bloqué (correct) tout en restant vérifiable. L'ι est aussi déclenchée sous un
lieur et à travers une application imbriquée (`not (not true) → true`,
`bool_rec … (not false) → t`). Les quatre équations Bool du tableau sont
comptées comme un pas chacune et vues par `convertible`.

### Registre de descripteurs d'inductifs — troisième inductif : List

Les éliminateurs `nat_rec` et `bool_rec`, autrefois **câblés** dans `noy_iota`,
sont désormais décrits par la **même table de registre** qu'un troisième
inductif, `List`. Chaque entrée porte le nom du type (`ind`), le nom de
l'éliminateur (`elim`) et la liste de ses constructeurs ; chaque constructeur
porte son nom, l'index de la branche à rendre dans l'épine d'application, son
arité et l'index de son argument récursif (`0` = non récursif) :

| Inductif | Éliminateur | Constructeurs (branche, arité, arg. réc.) |
|---|---|---|
| `Nat` | `nat_rec` | `zero` (3, 0, —), `succ` (2, 1, 1) |
| `Bool` | `bool_rec` | `true` (3, 0, —), `false` (2, 0, —) |
| `List` | `list_rec` | `nil` (3, 0, —), `cons` (2, 2, 1) |

Convention commune : `ΠP.Πbase.Πstep.Πx:T. P x`, avec l'épine
`args[4]=P, args[3]=base, args[2]=step, args[1]=scrutinee`. Pour un
constructeur récursif, le pas d'ι est
`step <args du constructeur> (elim P base step <arg récursif>)`.

Le registre est **interne et fermé** (trois entrées, construites localement —
Rose ne permet pas d'indexer une globale par une variable dans une fonction).
Il ne s'agit PAS d'un mécanisme d'inductifs utilisateurs : pas de motif
utilisateur, pas d'indices, pas de positivité ni de contrôle de terminaison.
Il factorise trois éliminateurs déjà présents et ajoute `List(A)` avec ses
constructeurs `nil : List(A)` et `cons : Πa:A. Πl:List(A). List(A)`.

Le déclenchement exige toujours **nom + structure** :
`noy_registre_elim_valide` impose quatre flèches, un motif `Πx:Ind(T).Type(i)`,
deux branches, un scrutinee `Ind(T)` et un résultat présent ;
`noy_resultat_est_ind` vérifie que le **type de retour** du constructeur est
bien `Ind(T)` (direct pour `nil`/`zero`/`true`/`false`, codomaine final d'une
chaîne de Π pour `succ`/`cons`). Un `const` homonyme mal annoté ne se réduit
jamais (tests dédiés). Un scrutinee variable reste bloqué, ce qui est correct.

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

## Tests (165 assertions, toutes vertes)

Le harnais `thorn/tests_noyau.rose` importe le noyau. Les 35 assertions
d'origine sont conservées à l'identique ; 53 s'y ajoutent pour TH4, 38 pour
le second inductif (Bool), puis 39 pour le registre et le troisième inductif
(List) :

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
14. Bool : les six équations ι (`bool_rec` sur true/false, `not` sur
true/false), comptage des pas (1, 1, 1, 2), imbrication (`not (not true)`,
`bool_rec … (not false)`), conversion, réduction sous λ, trois garde-fous
(nom sans structure, constructeur mal annoté), scrutinee variable bloqué mais
vérifiable, préservation du type (`Ind(Nat)` / `Ind(Bool)`), et les six
prédicats de contrôle de structure testés directement.
15. Registre et List : les trois entrées retrouvées par nom (`Nat`/`Bool`/
`List`), nom inconnu → sentinelle, descripteurs de `nil`/`cons` (arité,
branche, récursivité), validation générique de l'éliminateur, `resultat_est_ind`
sur `cons`/`nil`/`Type(0)`, ι de `list_rec` sur `nil` (1 pas), sur
`cons a l` (variable bloquée), sur `cons a (cons b nil)` → `succ (succ zero)`,
convertibilité, trois garde-fous (nom sans structure, constructeur mal annoté,
scrutinee `Nat`), préservation du type `Ind(Nat)`, et bien-formation de
`List`, `nil`, `cons`, `list_rec`.

Une sonde complémentaire (hors fichiers livrés) vérifiait déjà que des termes
délibérément mal formés (`genre` numérique, `domaine` chaîne, …) rendent
`ok = faux` sans planter.

## Limites honnêtes (état actuel — registre des 3 inductifs)

- **`ind` / `elim` restent approximatifs pour les autres inductifs.** Leurs
  sous-termes sont vérifiés, mais le type rendu reste `Type(0)` ; pas de motifs,
  d'indices, de positivité ni de vérification de terminaison structurelle.
  Seules les deux formes d'égalité ont été durcies (contrôle des membres), et
  les trois inductifs du registre (`Nat`, `Bool`, `List`) sont reconnus comme
  types de donnée pour l'annotation de leurs constructeurs.
- **ι toujours spécialisée, mais pilotée par un registre interne.** Les motifs
  sont reconnus pour **trois** inductifs : `Nat` (`plus`, `nat_rec`), `Bool`
  (`not`, `bool_rec`) et `List` (`list_rec`). Les éliminateurs dépendants
  (`nat_rec`/`bool_rec`/`list_rec`) ne sont plus câblés dans `noy_iota` : ils
  sont décrits dans `noy_registre()` et réduits par un mécanisme unique. Les
  fonctions calculées `plus` et `not` restent, elles, spécialisées (elles ne
  sont pas des éliminateurs d'inductif). Ceci n'est **pas** un mécanisme
  d'inductifs utilisateurs : le registre est fermé (3 entrées), sans motif
  utilisateur, sans indices, sans récursion imbriquée sur plusieurs
  constructeurs récursifs, sans contrôle de terminaison ni de positivité. Un
  éliminateur appliqué à un scrutinee non constructeur reste bloqué, ce qui est
  correct.
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
