# Thorn — limites, axiomes et comparaison honnête avec Lean / Mathlib

**Constat daté : 20/11/2026.** Ce document distingue trois choses que les
documents de présentation mélangent souvent : (1) ce que Thorn **prouve**
réellement, (2) ce qu'il **suppose** (axiomes et confiance), (3) ce qu'il **ne
peut pas faire**. La dernière section compare avec Lean 4 / Mathlib sans
complaisance dans les deux sens. Toutes les commandes citées ont été
exécutées depuis `/home/clausake/Stefan/ROSE` avec `./rose/rose` (0.1.6).

---

## 1. Ce que Thorn prouve — exactement

### 1.1 Le jugement vérifié

Thorn ne prouve pas « une proposition est vraie » au sens sémantique. Il
établit un **jugement de typage** :

```
verifier(t, ctx) rend {ok=vrai, type=T, code=OK}
ET  convertible(T, but) rend vrai
```

Autrement dit : *le terme `t` est bien typé dans `ctx`, et son type `T` est
égal au but annoncé modulo βι + α-équivalence* (avec la cumulativité
`Type(i) ≤ Type(j)` dans la seule règle `App`). C'est `ta_verifie()` dans
`tactiques.rose`, `bl_test()` dans `bibliotheque.rose`, `al_test()` dans
`algebre.rose`. Aucun `ok` n'est une valeur `vrai` écrite en dur.

Le terme résultant est **toujours reconstruisible** : chaque `Resultat` porte
un champ `terme` et une `regle`. Le protocole est donc auditable ligne à ligne.

### 1.2 Ce que cela donne concrètement (mesuré)

```
$ ./rose/rose thorn/src/principal.rose --demo
=== thorn/preuves/identite.thn ===
OK identite
1 théorème(s), 1 réussi(s)
...
5 théorème(s), 5 réussi(s)
RC=0
```

```
$ ./rose/rose thorn/src/bibliotheque.rose
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
RC=0
```

```
$ ./rose/rose thorn/tests_algebre.rose
34 lemme(s) vérifié(s), 0 échec(s)
RESULTAT: OK
RC=0
```

```
$ ./rose/rose thorn/tests_noyau.rose
REUSSIS=126
ECHOUES=0
RESULTAT: OK
RC=0
```

Ces nombres sont des **constats datés** : à re-mesurer après tout changement.

### 1.3 Le test anti-preuve-du-faux

La propriété la plus importante d'un vérificateur est de **refuser** ce qui
n'est pas prouvable. Thorn le fait, et c'est testé dans la porte de
non-régression (`thorn/tests_regression.sh`, documentée dans `QUALITE.md`) :

```
$ ./rose/rose thorn/src/principal.rose thorn/exemples/ex10_echec.thn
ÉCHEC ex10_faux : réflexivité : les deux membres diffèrent
1 théorème(s), 0 réussi(s)
RC=1
```

Le but (`∀A B : Type, A = B`) est bien typé, mais les deux membres ne sont pas
convertibles : la tactique échoue, aucun terme n'est accepté. Un fichier
affirmant `0 = 1` est également rejeté (pour une autre raison : les membres
n'ont pas le même type après analyse).

### 1.4 Ce qu'un `ÉCHEC` ne prouve PAS

Un échec signifie **« cette tactique n'a pas produit de terme vérifié »**, pas
« la proposition est fausse ». Les tactiques sont incomplètes : `symetrie`
exige une hypothèse de type exact, `application` essaie une ou deux
applications, il n'y a ni unification générale ni recherche automatique. Un
`ÉCHEC` est donc un fait sur le **programme de preuve**, pas un théorème
d'impossibilité.

---

## 2. Ce que Thorn suppose — axiomes et confiance

### 2.1 Constantes de signature du noyau et des tactiques

| Constante | Type déclaré | Statut réel |
|---|---|---|
| `Nat` | type inductif `ind`, params vides | **supposé** (type rendu `Type(0)`, aucune déclaration d'inductif par l'utilisateur) |
| `zero : Nat` | constante annotée | **supposé** |
| `succ : Nat -> Nat` | constante annotée | **supposé** (pas de contrôle de discriminateur dans le noyau) |
| `plus : Nat -> Nat -> Nat` | constante annotée | **supposé**, mais ses deux équations sont désormais **calculées** par l'ι du noyau |
| `Eq A a b` | `ind("=",[a,b])` / `ind("Eq",[A,a,b])` | formation **vérifiée** pour ces deux formes (types des membres contrôlés) |
| `refl : Π(A:type)(x:A), x = x` | constante annotée | **supposé** |
| `eq_rec`, `nat_rec` | principes d'élimination | **supposés** (types vérifiés à l'application ; aucune terminaison / positivité) |
| `plus_0_l : Πn, 0+n = n` | constante annotée | **supposé** comme constante, mais **redondant** pour le calcul depuis l'ι : conservé pour les preuves existantes |
| `plus_s_l : Πm n, succ m + n = succ (m+n)` | idem | idem |

`plus_0_l` et `plus_s_l` sont encore cités par les preuves de démonstration et
par la bibliothèque. Leur contenu computationnel est fait par le noyau, mais
**la constante reste un axiome** : Thorn ne prétend pas l'avoir démontrée.

### 2.2 Axiomes supplémentaires de `algebre.rose`

| Constante | Rôle | Statut |
|---|---|---|
| `mult : Nat -> Nat -> Nat` | multiplication | **supposé** ; l'ι du noyau ne la réduit **pas** (mesuré : `beta(n × 0)` reste une `app`) |
| `mult_0_r : Πn, n × 0 = 0` | équation computationnelle de × | **supposé** |
| `mult_s_r : Πn m, n × succ m = (n×m) + n` | seconde équation de × | **supposé** |
| `le : Nat -> Nat -> Type(0)` | ordre | **supposé**, relation postulée, pas un calcul booléen |
| `le_refl_ax`, `le_trans_ax`, `le_antisymm_ax`, `le_zero_ax`, `le_plus_ax` | propriétés de `≤` | **supposées** |
| `succ_inj_ax : Πn m, succ n = succ m -> n = m` | injectivité | **supposé** |
| `succ_ne_zero_ax : Πn, succ n = 0 -> Faux` | discrimination | **supposé** |
| `Faux : Type(0)` | absurde | **supposé** |

En revanche, `symm`, `trans`, `congr_succ`, `congr_plus_l`, `plus_zero_d`,
`plus_succ_d`, ainsi que les lois d'addition, de multiplication et les
congruences de `algebre.rose`, sont des **termes dérivés vérifiés** par le
noyau (pas des axiomes). La liste précise figure dans `ALGEBRE.md` §3–4.

### 2.3 Confiance dans l'implémentation

- Le noyau est **écrit en Rose** (`thorn/src/noyau.rose`, ~1 350 lignes). Sa
  correction n'est pas démontrée dans un méta-modèle : on lui fait confiance,
  comme on fait confiance à tout vérificateur implémenté.
- Rose lui-même (compilateur Rust + JIT) est un prédicat de confiance ; la
  CI `ci.yml` couvre ses 515 tests Rust, mais Thorn n'hérite d'aucune preuve
  de correction formelle de Rose.
- Les axiomes ci-dessus sont supposés **cohérents**. S'ils ne l'étaient pas,
  `Faux` serait dérivable et le vérificateur accepterait n'importe quoi. Thorn
  documente cette hypothèse ; il ne la prouve pas.

---

## 3. Ce que Thorn ne peut pas faire

### 3.1 Noyau

- **Univers unique en pratique.** Le noyau sait `Type(n)` pour `n ≥ 0`, mais
  toute la bibliothèque est à `Type(0)`. Pas de polymorphisme d'univers, pas
  de cumulativité générale (uniquement `Type(i) ≤ Type(j), i ≤ j` dans `App`),
  pas de subsumption dans les Π ni les annotations.
- **Pas d'η-équivalence**, pas de `let`, pas de récursion générale, pas de
  vérification de terminaison des définitions utilisateur.
- **ι étroite.** Seuls `plus` et `nat_rec` sont reconnus, par **nom + forme du
  type**. Un `plus` mal typé n'est pas réduit (correct) ; un `mult` bien typé
  ne l'est pas non plus (limite). Pas de motifs utilisateur, pas d'indices,
  pas de récursion imbriquée.
- **`ind` / `elim` approximatifs** pour les inductifs autres que les deux
  formes d'égalité : sous-termes vérifiés, mais type rendu `Type(0)`, sans
  positivité ni cohérence des constructeurs.
- **`convertible` sans contexte** : conversion définitionnelle pure, aucune
  conversion avec hypothèses. Garde à 10 000 pas ; épuisée, elle rend `faux`
  plutôt qu'une comparaison partielle.
- **Pas de partage de structure** : chaque constructeur alloue une table. La
  garde borne le temps, pas la mémoire.

### 3.2 Syntaxe et élaboration

- Pas de notation Unicode `∧`, `∨`, `→` ; seulement les mots français et
  `->`, `=`, `+`, `-`. Pas de priorités personnalisées.
- `existe` est un `ind` et non un `pi` : le noyau doit le reconnaître
  explicitement (ce n'est pas encore le cas).
- Aucun élaborateur, aucune unification générale, aucune synthèse
  d'arguments implicites au niveau noyau (le pilote a une élaboration locale
  `ta_elab` pour `trans h1 h2`, pas un mécanisme général).
- Seuls les caractères « inconnus » deviennent `?` ; il n'y a pas de
  diagnostic riche ni de surlignage de position (le champ `ligne` existe mais
  le parseur ne le remplit pas).

### 3.3 Tactiques et bibliothèque

- `application` essaie une correspondance exacte puis 1 ou 2 applications ;
  `symetrie` exige une hypothèse de type exact modulo β/α.
- `simplifier` applique une passe locale (carburant 200) dont le résultat
  n'est **pas** garanti `convertible` au terme initial pour le noyau.
- Pas de soustraction, de prédécesseur, de division, de nombres négatifs, de
  groupe, de grandeur décidable, de quotient ni de choix.
- Pas de commutativité de `×` livrée (elle suivrait des lemmes présents, mais
  n'est pas fournie).
- `--demo` ne joue que les cinq fichiers du manifeste : un nouveau `.thn`
  n'est pas couvert tant qu'il n'y est pas ajouté.

### 3.4 Outil et qualité

- **`--trace` est cassé sur les termes `lam`** (bug mesuré, voir
  `PRO_RECETTES.md` §4.3). Il n'est donc pas couvert par la porte de
  non-régression.
- Une seule plateforme CI (`ubuntu-latest`) ; pas de macOS / Windows, pas de
  mesure de performance ni de mémoire, pas de reproductibilité binaire
  vérifiée par cette CI.
- Aucune garantie de cohérence des axiomes, aucune preuve méta-théorique de
  correction du noyau.

---

## 4. Comparaison honnête avec Lean 4 / Mathlib

### 4.1 Tableau comparatif

| Axe | Thorn (état mesuré) | Lean 4 / Mathlib |
|---|---|---|
| Nature | Assistant de démonstration expérimental, écrit en Rose, zéro dépendance | Assistant de preuve généraliste, noyau CIC, écosystème Rust/Lean |
| Langage de définition | Français (`théorème`, `preuve`, `pour_tout`, `implique`…) | Anglais (`theorem`, `by`, `forall`, `->`…) |
| Noyau | Synthèse de type + conversion βι + α, ~1 350 lignes de Rose | CIC prédicatif, `Prop` impredicatif, noyau petit et très audité |
| Univers | `Type(n)` supporté ; **toute la bibliothèque à `Type(0)`**, cumulativité seulement dans `App` | Univers polymorphes (`u`, `v`), cumulativité générale, max/imax |
| Inductifs | Termes `ind` **supposés** ; deux formes d'égalité durcies ; rien d'autre | Déclarations `inductive` par l'utilisateur, **strict positivity** et indices vérifiés |
| Récursion | `nat_rec` / `eq_rec` axiomes ; aucune terminaison vérifiée | Récursion structurelle et bien-fondée avec **contrôle de terminaison** |
| Conversion | β + ι étroite (`plus`, `nat_rec`) + α ; pas d'η, pas de conversion par hypothèses | βδιζ + η (selon réglage), réduction des définitions et des projections |
| Égalité | `Eq` formation vérifiée ; pas de proof irrelevance, pas de quotient | Égalité identitaire, proof irrelevance pour `Prop`, quotient et `Classical.choice` (axiomes assumés) |
| Élaboration | Aucun élaborateur ; les arguments sont fournis explicitement (élaboration locale partielle dans le pilote) | Élaborateur puissant, inférence, **typeclasses**, implicites, coercion |
| Tactiques | Six tactiques artisanales, recherche très limitée | `simp`, `omega`, `linarith`, `aesop`, `grind`, etc., automatisation massive |
| Bibliothèque | 10 lemmes (`bibliotheque`) + 34 (`algebre`) + 5 preuves de démo | Des milliers de fichiers, analysis, algebra, topology, categories… |
| Transparence | Chaque résultat porte le terme et la règle ; `--trace` (en panne sur `lam`) | `set_option pp.all true`, `show_term`, `trace.Meta.Tactic.simp`… |
| Preuve de correction du noyau | Aucune | Aucune non plus au sens « noyau certifié » ; confiance dans une implémentation petite et auditée |

### 4.2 Ce que Lean a et que Thorn n'a pas (et pourquoi c'est un écart réel)

1. **Univers et prédicativité.** Un `Nat : Type` et un polymorphe `List α` ne
   posent aucun problème à Lean ; dans Thorn, l'encodage de Church exigerait
   `Nat : Type(1)` passé comme `X : Type(0)`, ce que le noyau prédicatif
   refuse (documenté dans `TACTIQUES.md` §5).
2. **Définitions vérifiées.** Lean vérifie la terminaison d'une fonction
   récursive et la positivité d'un inductif. Thorn **assume** `nat_rec`,
   `mult_0_r`, `le_*`, `succ_inj_ax` : il vérifie des types d'applications,
   pas une sémantique récursive.
3. **Automatisation.** `by omega` résout des systèmes arithmétiques entiers ;
   Thorn exige que chaque branche d'induction soit fournie explicitement en
   Rose.
4. **Écosystème.** Mathlib fournit des milliers de théorèmes réutilisables ;
   Thorn fournit une quarantaine de lemmes sur `Nat`, `+`, `×`, `≤`.

### 4.3 Ce que Thorn a et que Lean n'a pas (sans complaisance)

1. **Syntaxe française et zéro dépendance** : tout tient dans sept fichiers
   Rose et un binaire ; aucun gestionnaire de paquets, aucun runtime externe.
2. **Traçabilité complète du terme produit** (hors bug `--trace` sur les
   `lam`) : chaque tactique rend `{terme, regle, buts, ctxs}`. C'est un
   excellent outil pédagogique pour *voir* ce qu'est une preuve.
3. **Petitesse auditable** : le noyau tient en ~1 350 lignes ; on peut le lire
   d'une séance, ce qui n'est pas le cas d'un noyau de production complet.
4. **Intégration Rose** : Thorn est un consommateur de Rose et peut être
   appelé depuis un programme Rose ordinaire (exemples 1–9 du `PRO_GUIDE.md`).

### 4.4 Ce que les deux systèmes supposent

La différence n'est pas « Lean prouve tout, Thorn suppose tout ». Lean
assume également des axiomes (`Classical.choice`, `propext`, quotients) et la
correction de son noyau. La différence est de **discipline et d'échelle** :
Lean contrôle les définitions utilisateur (terminaison, positivité) et a un
noyau minimal très scruté ; Thorn laisse ces contrôles au programmeur et
documente ses axiomes dans `TACTIQUES.md` / `ALGEBRE.md`. Un utilisateur de
Thorn doit donc lire la §2 avant de croire un `OK`.

---

## 5. Limites connues et mesurées (à re-vérifier)

| Limite | Preuve / commande |
|---|---|
| `--trace` plante sur `lam` | `./rose/rose thorn/src/principal.rose thorn/preuves/identite.thn --trace` → `Rose : erreur — clé absente de la table`, rc=1 |
| `--trace` fonctionne sur un but sans `lam` | `./rose/rose thorn/src/principal.rose thorn/exemples/ex10_echec.thn --trace` → ligne `[trace] … but = pi(A)(… -> ind(=))`, rc=1 (échec attendu de la preuve) |
| `1` n'est pas un `Nat` dans un `.thn` | `0 = 1` → `ÉCHEC … : type invalide — membres d'une egalite de types differents` |
| `succ` libre dans un `.thn` | `0 = succ 0` → `ÉCHEC … : variable non liee: succ` |
| `convertible` strict | `convertible(Type(0), Type(1)) = faux` mais `cumulatif(Type(0), Type(1)) = vrai` (exemple 5) |
| ι ne touche pas `mult` | `beta(n × 0)` reste une `app` (sonde `./rose/rose` sur `algebre.rose`) |
| formation d'égalité durcie | `0 = Type(0)` → code `EQ_MAL_TYPE` (exemple 4) |
| garde | `verifier_lim(Ω, ctx, 0)` → `GARDE_PAS` (exemple 4) |

---

## 6. Verdict

Thorn est un **vérificateur de termes typés** honnête et auditable, pas un
assistant de démonstration de niveau production. Ce qu'il prouve est réel :
les 5 preuves de démonstration, les 10 lemmes de bibliothèque et les 34 lemmes
d'algèbre passent par `verifier` + `convertible`, et une preuve fausse est
refusée. Ce qu'il suppose est explicite : une dizaine de constantes de
signature et la correction de Rose. Ce qu'il ne peut pas faire est massif :
univers, inductifs génériques, récursion vérifiée, automatisation, bibliothèque.
La comparaison avec Lean n'est pas flatteuse pour Thorn — et c'est précisément
pour cela que ses limites sont documentées ici plutôt que masquées.
