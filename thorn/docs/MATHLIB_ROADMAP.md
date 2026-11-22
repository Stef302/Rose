# Thorn — MATHLIB_ROADMAP : vers un équivalent de mathlib adapté à Thorn

**Constat daté : 22/11/2026.** Tous les chiffres de ce document ont été
re-mesurés ce jour avec `./rose/rose` **0.1.73** depuis
`/home/clausake/Stefan/ROSE`. Ce sont des constats, pas des promesses.
Fichiers livrés par ce chantier (tous NOUVEAUX, aucun fichier existant modifié,
aucun commit, aucune publication) :

| Fichier | Rôle |
|---|---|
| `thorn/src/hierarchie.rose` | Jalon 1 : Magma → Semigroupe → Monoid → Groupe, lois, registre, lemmes génériques, réécriture |
| `thorn/tests_hierarchie.rose` | Rejeu du jalon 1 (19 contrôles) |
| `thorn/src/instances.rose` | Prototype : synthèse d'instances par pré-élaboration (zéro changement du noyau) |
| `thorn/tests_instances.rose` | Rejeu du prototype (10 contrôles) |
| `thorn/docs/MATHLIB_ROADMAP.md` | Ce document |

---

## 1. État des lieux réel — chiffres mesurés

### 1.1 Avant le chantier (ligne de base, 22/11/2026)

| Commande | Sortie réelle | rc |
|---|---|---|
| `./rose/rose thorn/tests_noyau.rose` | `REUSSIS=165`, `ECHOUES=0`, `RESULTAT: OK` | 0 |
| `./rose/rose thorn/tests_syntaxe.rose` | `OK = 8   ECHEC = 0` | 0 |
| `./rose/rose thorn/tests_tactiques.rose` | `REUSSIS=19`, `REUSSIS_SUP=14`, `TOTAL_REUSSIS=33`, `TOTAL_ECHOUES=0` | 0 |
| `./rose/rose thorn/tests_pilote.rose` | `REUSSIS=12`, `ECHOUES=0`, `RESULTAT: OK` | 0 |
| `./rose/rose thorn/tests_algebre.rose` | `34 lemme(s) vérifié(s), 0 échec(s)`, `RESULTAT: OK` (154 s) | 0 |
| `./rose/rose thorn/src/bibliotheque.rose` | `10 lemme(s) vérifié(s), 0 échec(s)` | 0 |
| `./rose/rose thorn/src/principal.rose --demo` | `5 théorème(s), 5 réussi(s)` | 0 |
| `bash thorn/tests_regression.sh` | `cas exécutés : 17 / 17`, `réussis : 17`, `échoués : 0`, `BILAN: OK` | 0 |

Total de la ligne de base : **165 + 8 + 33 + 12 + 34 + 10 + 5 = 267 contrôles verts**,
plus la porte de non-régression 17/17.

### 1.2 Ce qui existe réellement (inventaire)

**Noyau** (`noyau.rose`, 1 586 lignes) : 9 genres de termes (`var`, `type`, `pi`,
`lam`, `app`, `const`, `ind`, `constructeur`, `elim`) ; contexte = liste de tables
rembourrée à l'index 0 ; `verifier`/`verifier_lim` (garde 10 000 pas),
`beta` = β + ι (stratégie normale), `convertible` = βι + α stricte,
`cumulatif` = `Type(i) ≤ Type(j)` **uniquement dans `App`** ; `Resultat` structuré
(`ok`, `type`, `erreur`, `code`, `contexte`, `ligne`, `pas`) ; formation d'égalité
durcie (`=` à 2 paramètres, `Eq` à 3) ; registre interne fermé de 3 inductifs
(`Nat`, `Bool`, `List`) avec ι pour `plus`, `not`, `nat_rec`, `bool_rec`,
`list_rec` (reconnus par **nom + structure de type**, jamais par nom seul).

**Tactiques** (`tactiques.rose`, 1 397 lignes) : `reflexivite`, `symetrie`,
`introduction`, `application`, `exact`, `apply` (déduction arrière, unification
légère, profondeur 3, 6 Π/tête), `induction`, `simplifier` (conversion étendue
non garantie `convertible`) ; lemmes dérivés vérifiés `symm`, `trans`,
`congr_succ`, `congr_plus_l`, `plus_zero_d`, `plus_succ_d`.

**Bibliothèque / algèbre** : 10 lemmes (`bibliotheque.rose`) ; 34 lemmes
(`algebre.rose` : 11 sur `+`, 12 sur `×`, 8 sur `≤`, 3 sur `succ`), dont
l'associativité, la commutativité de `+`, les deux distributivités et
l'associativité de `×`. Axiomes explicites : équations de `×`, `≤` et ses 5
lois, injectivité de `succ`, `succ ≠ 0`, `Faux`.

**Pilote / syntaxe** : lexeur français, parseur à 8 niveaux de précédence,
`--demo` (5 preuves), contre-exemple `0 = 1` rejeté (preuve fausse refusée).

### 1.3 Ce qui manque pour un « mathlib »

1. Aucune hiérarchie de classes, aucune instance publique, aucun lemme
   générique réutilisable : les 34 lemmes d'algèbre sont spécialisés à `Nat`.
2. Aucune synthèse d'instance, aucun argument implicite au niveau noyau.
3. Ordres/topologie/analyse absents en tant que structures (seules 8 lois sur
   `≤` postulées dans `algebre.rose`).
4. Pas de groupes, d'entiers relatifs, de soustraction, de division.
5. Unification générale absente ; pas de `simp`, pas de réécriture par lemmes.
6. Inductifs utilisateurs, positivité, terminaison : absents (registre fermé).
7. Univers : `Type(n)` existe dans le noyau, mais toute la bibliothèque est à
   `Type(0)` et `ind` rend toujours `Type(0)` → pas de polymorphisme réel.
8. Pas d'η, de `let`, de définitions récursives vérifiées.
9. Documentation éparse et parfois contradictoire : `TACTIQUES.md` et
   `QUALITE.md` décrivent un noyau sans ι alors que `NOYAU.md` et le code
   mesuré ont ι (3 inductifs). Ce document fait foi pour le 22/11/2026.

---

## 2. Analyse — qu'est-ce qui rend mathlib solide, et comment l'adapter

### 2.1 Les piliers de mathlib

| Pilier | Mécanisme Lean/mathlib | Équivalent Thorn réaliste |
|---|---|---|
| Hiérarchie typeclassée | `class Semigroup`, `Monoid extends Semigroup`, instances | Table Rose de champs `{nom, car, op, e, inv, assoc, …}` + registre (cf. `noy_registre()`) |
| Instances canoniques | `[instMulNat]`, synthèse par l'élaborateur | Registre public `mg_registre()` + passe de pré-élaboration `th_resoudre` |
| Lemmes génériques | `theorem mul_assoc [Semigroup G] (a b c : G) : …` | Fonction Rose `mg_lem_assoc4(inst)` qui **construit le terme** et le soumet à `verifier` |
| Champs = preuves | les lois sont des champs de la classe | les champs contiennent des **termes de preuve vérifiés**, pas des noms opaques |
| Automation | `simp`, `omega`, `linarith`, `aesop` | `mg_rewriter` (eq_rec + motif) puis boucle de réécriture avec carburant |
| Notation / implicites | `+`, `*``, `@` | termes Rose construits explicitement ; marqueurs de classe pré-élaborés |
| Cohérence du noyau | petit, audité, termes reconstructibles | déjà vrai : tout `Resultat` porte `terme`/`regle`, tout `OK` passe par `verifier` + `convertible` |
| Échelle | milliers de lemmes, conventions de nommage | corpus encore minuscule : c'est le travail des jalons suivants |

### 2.2 Le choix de conception du jalon 1

Thorn n'a **pas** de typeclasses dans son noyau, et n'en a pas besoin pour
commencer : un enregistrement Rose *est* une classe, et une fonction Rose
paramétrée par cet enregistrement *est* un lemme générique.

- **Loi = terme, pas nom.** Le champ `assoc` de l'instance `Nat/+/0` est le
  terme de preuve déjà vérifié `al_lem_plus_assoc()["terme"]`, pas une constante
  opaque. Le lemme générique l'applique directement ; le noyau vérifie
  l'application complète. Aucun axiome nouveau n'est introduit pour les
  instances concrètes.
- **Instance abstraite = section.** `GroupeAbstrait` déclare `A : Type(0)`,
  `op`, `e`, `inv` et les cinq lois dans un contexte `liste<table>` de 9 entrées.
  Les lemmes de groupe sont vérifiés dans ce contexte : c'est l'équivalent
  d'une section Lean avec variables de classe, mais sans toucher au noyau.
- **Registre déterministe.** `mg_registre()` contient 3 instances ; la
  recherche (`mg_registre_trouver`) est par nom, la synthèse de classe
  (`th_resoudre`) est par `(classe, porteur convertible)`, avec priorité
  d'ordre de liste. Pas de résolution de contraintes : c'est un choix
  délibéré, documenté comme limite.
- **Réécriture vérifiée.** `mg_rewriter` construit
  `eq_rec A u v (λz. Eq A C[z] b) preuve h` et **re-vérifie** le résultat
  contre le but calculé. Aucun `ok` n'est codé en dur.

### 2.3 Obstacles concrets du noyau actuel (mesurés, pas supposés)

1. **Pas d'arguments implicites.** Tout est explicite : `symm` prend
   `(A, a, b, h)` même quand `A` est déterminé par `h`. Le prototype
   `instances.rose` masque l'argument d'instance, mais pas les implicites
   ordinaires.
2. **Pas d'unification générale.** `apply` (tactiques.rose) lie des méta à des
   sous-termes du but, profondeur 3, 6 Π par tête. Un lemme générique doit
   donc recevoir ses arguments explicitement.
3. **Pas de classes dans `verifier`.** Une classe est une valeur Rose, pas un
   terme du noyau : l'élaboration doit avoir lieu *avant* `verifier`.
4. **`convertible` ignore le contexte** : βι + α pure ; aucune conversion par
   hypothèse, pas d'η.
5. **`ind`/`elim` approximatifs** hors des 3 inductifs du registre : type rendu
   `Type(0)`, pas de positivité, pas de terminaison, pas d'indices.
6. **Registre ι fermé** (3 entrées construites localement). Ajouter un
   inductif utilisateur exigerait de rendre le registre paramétrable.
7. **Univers plafonnés en pratique** : `ind` rend `Type(0)` quoi qu'il arrive ;
   une classe sur `A : Type(1)` n'est pas représentable aujourd'hui.
8. **Ordre des lieurs des lemmes existants non uniforme** : par exemple
   `al_lem_mult_assoc` a type `Πc a b`, alors que la signature canonique est
   `Πx y z`. Le jalon 1 le normalise par un wrapper λ dans
   `mg_inst_nat_mult` — preuve que le futur registre doit stocker des
   *descripteurs de forme*, pas seulement des noms.

---

## 3. Plan par jalons (ordre de dépendance)

```
M0  FONDATIONS  (LIVRÉ)  structures + registre + lemmes génériques + réécriture
 │                       + prototype de synthèse d'instances
 ├──> M1  ORDRES ET DÉCIDABILITÉ
 │       Preorder / PartialOrder / LinearOrder en enregistrements ; lemmes
 │       génériques (trans, antisymm, monotonicité) ; `décider (a ≤ b)` ;
 │       recycler les 8 lemmas de `≤` de algebre.rose comme lois certifiées.
 │       Cible mesurable : ≥ 25 lemmes génériques vérifiés, 3 instances.
 ├──> M2  SEMI-ANNEAUX ET ANNEAUX
 │       AddMonoid/MulMonoid/Distrib/Semiring/Ring ; instances Nat, puis
 │       entiers relatifs comme paire (Nat, Nat) quotientée par une relation
 │       vérifiée ; normaliseur de polynômes par réécriture (carburant).
 │       Cible : ≥ 60 lemmes génériques, ≥ 4 instances, 0 axiome nouveau pour Nat.
 ├──> M3  TOPOLOGIE / ANALYSE
 │       Filters, métriques, convergence comme enregistrements ; dépend de M1
 │       (ordre décidable) et M2 (corps ordonné). Nécessite un changement de
 │       noyau (§5.B) pour les définitions récursives vérifiées.
 └──> M4  AUTOMATISATION
         Boucle `simp`-like sur un registre de lemmes `[simp]` alimenté par
         `mg_rewriter`, avec carburant et contrôle final `convertible` ;
         réflexion arithmétique (`omega`-like) pour les fragments Nat.
         Cible : 100 réécritures vérifiées en < 60 s.
 M5  ERGONOMIE  transversale : marqueurs d'implicites intégrés au parseur,
     notation, diagnostics. Dépend de M0–M2.
```

Règle de passage d'un jalon : **chaque lemme est soumis à `verifier` puis
`convertible`**, aucun compteur n'est écrit en dur, et la porte
`bash thorn/tests_regression.sh` reste à 17/17.

---

## 4. Jalon 1 livré — M0 FONDATIONS

### 4.1 Contenu

`thorn/src/hierarchie.rose` fournit :

- **4 constructeurs de structures** : `mg_magma`, `mg_semigroupe`, `mg_monoid`,
  `mg_groupe`, avec prédicats `mg_est_*` et accesseurs défensifs.
- **6 lemmes génériques** (un terme construit une fois, valable pour toute
  instance) :
  1. `mg_lem_assoc4` — `((a*b)*c)*d = a*(b*(c*d))` ;
  2. `mg_lem_id_unique_d` — unicité de l'identité à droite ;
  3. `mg_lem_id_unique_g` — unicité de l'identité à gauche ;
  4. `mg_lem_inv_unique` — `b*a = e → a*b = e → b = inv a` ;
  5. `mg_lem_inv_inv` — `inv (inv a) = a`, dérivé du précédent ;
  6. `mg_lem_cancel_g` — `a*b = a*c → b = c`, par congruences `eq_rec`.
- **3 instances publiques** dans `mg_registre()` : `Nat/+/0` et `Nat/*/1`
  (lois = termes **déjà vérifiés** dans `algebre.rose`), `GroupeAbstrait`
  (lois = constantes de signature déclarées dans un contexte de 9 entrées).
- **2 tactiques d'assistance** : `mg_rewriter` (réécriture du membre gauche)
  et `mg_rewriter_droite` (membre droit), fondées sur `eq_rec` + motif, avec
  re-vérification du terme produit contre le but calculé.
- **Un prototype de synthèse d'instance** dans `thorn/src/instances.rose` :
  marqueur `Classe(C, A)`, résolution `(classe, porteur)` dans le registre,
  `th_applique` qui élaboré puis vérifie.

### 4.2 Preuves d'exécution (commandes réellement lancées le 22/11/2026)

```
$ ./rose/rose verif thorn/src/hierarchie.rose          → vérifié
$ ./rose/rose verif thorn/tests_hierarchie.rose        → vérifié
$ ./rose/rose thorn/tests_hierarchie.rose
Thorn — jalon 1 : hiérarchie algébrique vérifiée par le noyau
registre : 3 instance(s) publique(s)
OK [structure] registre/Nat/+/0 retrouve
OK [structure] structure/Nat/+/0 est Magma+Semigroupe+Monoid
OK [structure] structure/Nat/*/1 est Monoid
OK [structure] structure/GroupeAbstrait est Groupe
OK [structure] structure/niveau Monoid pour Nat/+/0
OK [structure] structure/niveau Groupe pour GroupeAbstrait
OK [structure] registre/nom inconnu -> absent
OK assoc4[Nat/+/0]
OK assoc4[Nat/*/1]
OK id_unique_d[Nat/+/0]
OK id_unique_g[Nat/+/0]
OK id_unique_d[Nat/*/1]
OK id_unique_g[Nat/*/1]
contexte abstrait : 9 declaration(s)
OK inv_unique[GroupeAbstrait]
OK inv_inv[GroupeAbstrait]
OK cancel_g[GroupeAbstrait]
OK [rw] rw/gauche 1*n -> n
OK [rw] rw/droite 1*n -> n
OK [structure] rw/refus u absent (attendu)
19 contrôle(s) réussi(s), 0 échec(s)
RESULTAT: OK                                    (rc=0, 132 s mesurées)

$ ./rose/rose verif thorn/src/instances.rose            → vérifié
$ ./rose/rose verif thorn/tests_instances.rose          → vérifié
$ ./rose/rose thorn/tests_instances.rose
OK marqueur/structure
OK marqueur/classe lue
OK marqueur/porteur lu
OK resoudre/Add(Nat) -> Nat/+/0
OK resoudre/Mul(Nat) -> Nat/*/1
OK resoudre/Groupe(Nat) -> absent
OK resoudre/classe inconnue -> absent
OK elaboration/Add(Nat) 0 1 typé Nat
OK elaboration/Mul(Nat) 1 1 typé Nat
OK elaboration/refus instance absente (attendu)
10 contrôle(s) réussi(s), 0 échec(s)
RESULTAT: OK                                    (rc=0, 34 s mesurées)
```

### 4.3 Avant / après

| Indicateur | Avant | Après |
|---|---|---|
| Structures algébriques réutilisables | 0 | 4 niveaux, 3 instances publiques |
| Lemmes génériques | 0 | 6 (vérifiés par le noyau) |
| Réécriture vérifiée | 0 | 2 tactiques (`eq_rec`), + 1 refus attendu |
| Synthèse d'instance | 0 | 1 prototype (10 contrôles) |
| Contrôles verts Thorn (hors porte) | 267 | **296** (267 + 19 + 10) |
| Porte de non-régression | 17/17 | **17/17, inchangée** |

Non-régression re-mesurée **après** ajout : `bash thorn/tests_regression.sh`
→ `cas exécutés : 17 / 17`, `réussis : 17`, `échoués : 0`,
`BILAN: OK — aucune régression`, compteur `REUSSIS=165` inchangé.

---

## 5. Limites techniques FRANCHES et changements de noyau minimaux

### 5.A Ce qui est impossible avec le noyau actuel, et pourquoi

| Limite | Cause précise dans le noyau | Conséquence |
|---|---|---|
| Typeclasses *dans* `verifier` | `Terme` n'a pas de genre `classe` ; `App` ne consulte aucun registre d'instances | La synthèse doit être faite **avant** `verifier` (prototype `instances.rose`) |
| Arguments implicites | Aucun méta au niveau noyau, aucun marquage d'implicite dans `pi` | Chaque argument est explicite dans le terme construit |
| Unification générale | `ta_unifie` ne résout pas d'équations, profondeur 3, 6 Π/tête | Pas de `exact?`, pas d'inférence de lemme à partir du but seul |
| Inductifs utilisateurs | `noy_registre()` est local, fermé, 3 entrées ; `elim` rend toujours `Type(0)` | Pas de `inductive` Thorn, pas de positivité, pas de terminaison |
| Récursivité vérifiée | `nat_rec`/`eq_rec` sont des axiomes ; aucune garde de terminaison | Pas de définitions utilisateur calculables au-delà des 3 inductifs |
| Polymorphisme d'univers réel | `ind` rend `Type(0)` ; cumulativité bornée à `App` | Les structures sont limitées à des porteurs de `Type(0)` |
| η, `let`, conversion par hypothèses | `noy_alpha`/`noy_beta_lim` ne les implémentent pas | Certaines preuves naturelles exigent des détours (`eq_rec`) |
| Cohérence des instances | Deux instances `Monoid Nat` peuvent coexister dans le registre | Aucune garantie qu'elles donnent la même opération ni les mêmes lois |

### 5.B Changement minimal proposé — ET PROTOTYPÉ

**Le changement minimal est de NE PAS changer le noyau.** C'est le point
important : l'encodage choisi montre que la majeure partie d'un « mathlib »
est faisable avec le noyau tel quel, pour peu que l'on ajoute une **passe de
pré-élaboration** devant `verifier`. Cette passe est livrée et testée dans
`thorn/src/instances.rose` / `thorn/tests_instances.rose` (10/10) :

- un marqueur `ind("Classe", [const(C, Type(0)), A])` est un terme **déjà bien
  formé** pour le noyau (ses paramètres synthétisent un type) ;
- `th_resoudre` trouve l'instance par `(classe, porteur convertible)` ;
- `th_applique` remplace le marqueur par l'opération de l'instance, construit
  l'application, puis **soumet le tout à `verifier` + `convertible`**.

Le noyau ne voit que des termes ordinaires : aucune règle de typage nouvelle,
aucune confiance supplémentaire. C'est le protocole le plus sûr et le moins
coûteux à valider.

**Ce que ce prototype ne couvre pas** (à faire dans M5, avec un vrai changement
si nécessaire) : implicites ordinaires, instances locales, priorités
configurationnelles, contraintes, instances dépendantes d'un paramètre de
type, surcharge. Le jour où l'on voudra que `verifier` résolve lui-même une
classe, le changement minimal sera : (1) ajouter un genre `classe` à `Terme`
avec une annotation `ΠA:Type(i), Type(j)`, (2) dans la règle `App`, quand le
domaine est une classe, consulter un registre passé en paramètre de
`verifier_lim` (au lieu du registre clos actuel), (3) garder `convertible`
inchangé. **Ce variant n'est PAS implémenté** : il modifierait `noyau.rose` et
donc la porte de non-régression ; il est proposé comme jalon M5, pas livré ici.

**Pour M3 (analyse)**, le changement minimal sera de rendre `noy_registre()`
paramétrable (registre passé à `beta_lim`) et d'ajouter un contrôle de
terminaison structurelle pour les définitions utilisateur. Là encore : non
implémenté dans ce jalon, et non prétendu.

### 5.C Ce que ce jalon ne livre PAS

- Pas d'ordres typeclassés (M1), pas d'anneaux au sens `Ring` (M2), pas de
  topologie ni d'analyse (M3).
- Pas de `simp`/`omega` : `mg_rewriter` est une assistance contrôlée, pas une
  boucle d'automatisation.
- `GroupeAbstrait` a des **lois supposées** : ses lemmes sont vérifiés, mais
  l'existence d'un tel groupe n'est pas démontrée. Aucune instance concrète de
  groupe n'est fournie (il n'y a pas d'entiers relatifs dans Thorn).
- Les 2 instances `Nat/+/0` et `Nat/*/1` héritent des axiomes déjà documentés
  de `algebre.rose` (`succ_inj_ax`, `le_*`, équations de `×`) : le jalon 1 ne
  les a pas supprimés et ne prétend pas les avoir démontrés.
- Performance : 132 s pour 19 contrôles ; les termes de preuve de groupe sont
  volumineux et le noyau n'a pas de partage de structure. C'est un plafond
  concret pour l'échelle visée par M4.

---

## 6. Honnêteté

Tous les `OK` ci-dessus sont calculés par `verifier()` + `convertible()` du
noyau, ou, pour les prédicats de structure, par des tests Rose explicitement
étiquetés `[structure]`. Aucun `ok = vrai` n'est codé en dur pour valider une
preuve. Les trois références historiques qui décrivent un noyau « sans ι »
(`TACTIQUES.md`, `QUALITE.md`, `PRESENTATION.md`, `PRO_LIMITES.md` partiel)
sont des constats antérieurs ; le code mesuré du 22/11 et `NOYAU.md` font foi.
Après toute modification de `thorn/src/**`, de `rose/src/**` ou de la
topologie de compilation, re-mesurer : ne pas recopier les chiffres de ce
document.

---

## 7. Prochaine action : M1 — ORDRES ET DÉCIDABILITÉ

Le jalon suivant est déjà cadré et peut être attaqué sans attendre :

1. `thorn/src/ordre.rose` : enregistrements `Preorder`, `PartialOrder`,
   `LinearOrder` (champs de lois = termes vérifiés), avec une instance
   certifiée réutilisant les 8 lemmes de `≤` de `algebre.rose`.
2. Corpus générique : `le_trans`, `le_antisymm`, monotonie de `+` et de `×`,
   `le_succ_mono`, `le_add_droite` — écrits une fois sur l'enregistrement,
   puis instanciés sur `Nat` et sur le produit `Nat × Nat`.
3. `décider (a ≤ b)` : relation décidable construite par récursion sur `Nat`
   via `nat_rec` (le noyau a déjà l'ι pour `nat_rec` et `plus`), avec preuve
   vérifiée de l'équivalence « le booléen est vrai ↔ a ≤ b ».
4. `thorn/tests_ordre.rose` : objectif ≥ 25 contrôles verts, porte 17/17
   inchangée, intégration au registre `mg_registre()` sans modifier le noyau.

Ce jalon ne demande **aucun** changement de noyau : il réutilise le schéma
M0 (enregistrement + lois-certificats + lemmes génériques + pré-élaboration).
