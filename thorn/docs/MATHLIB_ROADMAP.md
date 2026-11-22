# Thorn — MATHLIB_ROADMAP : vers un équivalent de mathlib adapté à Thorn

**Constat daté : 22/11/2026 (M0 + M1).** Tous les chiffres de ce document ont
été re-mesurés ce jour avec `./rose/rose` **0.1.73** depuis
`/home/clausake/Stefan/ROSE`. Ce sont des constats, pas des promesses.
Fichiers livrés par ce chantier (tous NOUVEAUX, aucun fichier existant modifié,
aucun commit, aucune publication) :

| Fichier | Rôle |
|---|---|
| `thorn/src/hierarchie.rose` | Jalon M0 : Magma → Semigroupe → Monoid → Groupe, lois, registre, lemmes génériques, réécriture |
| `thorn/tests_hierarchie.rose` | Rejeu du jalon M0 (19 contrôles) |
| `thorn/src/instances.rose` | Prototype M0 : synthèse d'instances par pré-élaboration (zéro changement du noyau) |
| `thorn/tests_instances.rose` | Rejeu du prototype (10 contrôles) |
| `thorn/src/ordres.rose` | Jalon M1 : Preorder / PartialOrder / LinearOrder / Lattice, 35 lemmes génériques, décidabilité calculée |
| `thorn/tests_ordres.rose` | Rejeu du jalon M1 (50 contrôles) |
| `thorn/docs/MATHLIB_ROADMAP.md` | Ce document (M0 + M1) |

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
   `≤` postulées dans `algebre.rose`). **M1 comble la partie « ordres »** :
   §4bis ; topologie et analyse restent absentes.
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
 ├──> M1  ORDRES ET DÉCIDABILITÉ  (LIVRÉ — voir §4bis)
 │       Preorder / PartialOrder / LinearOrder en enregistrements ; lemmes
 │       génériques (trans, antisymm, monotonicité) ; `décider (a ≤ b)` ;
 │       recycler les 8 lemmas de `≤` de algebre.rose comme lois certifiées.
 │       Cible mesurable : ≥ 25 lemmes génériques vérifiés, 3 instances.
 │       RÉSULTAT : 35 lemmes génériques, 4 instances, 50/50 contrôles.
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

## 4bis. Jalon M1 livré — ORDRES ET DÉCIDABILITÉ

### 4bis.1 Contenu réel

`thorn/src/ordres.rose` (importe `instances.rose`, donc toute la chaîne M0) fournit :

- **4 constructeurs de structures** : `ord_preorder`, `ord_partial`, `ord_linear`,
  `ord_lattice`, avec prédicats (`ord_est_preorder`/`ord_est_partial`/
  `ord_est_linear`/`ord_est_lattice`) et accesseurs défensifs. Une `LinearOrder`
  a une loi `total : Πa b, Dec (a ≤ b)` — la totalité est **decidable**, pas un
  drapeau.
- **Champs de loi = termes vérifiés** : les instances Nat utilisent
  `al_lem_le_refl`, `al_lem_le_trans`, `al_lem_le_antisymm` (termes déjà passés
  par `verifier`/`convertible` dans `tests_algebre.rose`). Les structures
  `OrdreAbstrait` et `LatticeAbstrait` sont des sections : lois = constantes de
  signature dans un contexte de noyau de 6 puis 14 entrées, exactement comme
  `GroupeAbstrait` en M0.
- **4 instances publiques** dans `ord_registre()` :
  `Nat/≤/Preorder`, `Nat/≤/PartialOrder`, `Nat/≤/LinearOrder`, et
  `Nat/≥/PartialOrder` (ordre dual, lois dérivées des lemmes vérifiés, sans
  axiome nouveau). `ord_registre_complet()` = registre M0 (3 instances) + registre
  M1 (4 instances) = **7 instances**, sans modifier `hierarchie.rose`.
- **35 lemmes génériques** (`grep -c '^fn ord_lem_' thorn/src/ordres.rose` = 35),
  répartis : ordre (réflexivité, transitivité, chaînes de 4 et 5, antisymétrie,
  symétrie de l'antisymétrie, transport par égalité, trans avec égalité à
  gauche/droite, réflexivité d'égalité, décidabilité) ; treillis (idempotence,
  commutativité, associativité de `min`/`max`, absorption, `min ≤ max`,
  `min_eq_left`, `max_eq_right`, monotonie de `min`/`max`, bornes inférieure et
  supérieure).
- **Décidabilité calculée** : `ord_decide_nat(a, b)` calcule la comparaison de
  deux numéraux Nat fermés (Rose), puis construit un **terme de noyau vérifié**
  de type `Dec (a ≤ b)` : `oui` + preuve vérifiée si vrai, `non` + fonction de
  réfutation vérifiée si faux. Famille de signature : `Dec : ΠP:Type(0), Type(0)`,
  `oui : ΠP, P → Dec P`, `non : ΠP, (P → Faux) → Dec P`. **Deux équations de
  signature sont ajoutées, toutes deux documentées** :
  `le_succ_pas_ax : Πn, succ n ≤ n → Faux` (nécessaire parce que `le` est une
  constante opaque dans `algebre.rose` : les 5 lois de l'ordre ne contredisent
  pas `succ n ≤ n`) ;
  `le_total_ax : Πa b, Dec (a ≤ b)` (champ `total` de l'instance
  `Nat/≤/LinearOrder`). Cause technique de ce second axiome : produire un TERME
  de noyau de type `Πa b, Dec (a ≤ b)` pour des variables `a b` exigerait une
  induction dans le terme avec réfutation de `succ n ≤ n` pour tout `n`, alors
  que `ord_decide_nat` ne calcule que sur des numéraux **fermés** (le noyau a
  l'ι pour `nat_rec`/`plus`, mais la bibliothèque n'a pas de lemme de décidabilité
  inductif réutilisable). Le protocole livré est le même que M0 pour
  `GroupeAbstrait` : axiome de signature explicite + lemmes dérivés vérifiés.
  Aucun changement de noyau.

### 4bis.2 Preuves d'exécution (commandes réellement lancées le 22/11/2026)

```
$ ./rose/rose verif thorn/src/ordres.rose            → vérifié
$ ./rose/rose verif thorn/tests_ordres.rose          → vérifié
$ ./rose/rose thorn/tests_ordres.rose
Thorn — jalon M1 : ordres et décidabilité vérifiés par le noyau
registre d'ordres : 4 instance(s)
registre complet M0+M1 : 7 instance(s)
OK [structure] ... (9 contrôles de structure)
OK loi/refl[Nat] / loi/trans[Nat] / loi/antisymm[Nat] / loi/total[Nat]
OK gen/* (17 contrôles, contexte abstrait d'ordre)
OK lat/* (15 contrôles, contexte abstrait de treillis)
OK [décision] decider/1 ≤ 2 (vrai)
OK [décision] decider/2 ≤ 2 (vrai)
OK [décision] decider/3 ≤ 1 (faux)
OK [décision] decider/0 ≤ 3 (vrai)
OK [refus] refus/0≤0 n'est pas 0≤1
50 contrôle(s) réussi(s), 0 échec(s)
RESULTAT: OK                                    (rc=0, 49,30 s mesurées)
```

Le contrôle `[refus]` est une preuve fausse (`le_refl 0` présentée pour
`0 ≤ 1`) rejetée par `convertible` : le bilan n'est donc pas codé en dur. Si un
lemme est rejeté par le noyau, `ord_test` affiche le message d'erreur du noyau,
incrémente le compteur d'échecs, et `thorn/tests_ordres.rose` lève une erreur
réelle via `quitter(1)` (code de sortie 1) — vérifié après ajout : 50/50, rc=0.
Le rejeu précédent, avant `quitter`, donnait 49,10 s ; le rejeu final, après
relecture des commentaires, a donné **47,84 s**, 50/50, rc=0.

### 4bis.3 Avant / après

| Indicateur | Avant M1 (M0 livré) | Après M1 |
|---|---|---|
| Structures d'ordre | 0 | 4 niveaux, 2 sections abstraites |
| Instances d'ordre publiques | 0 | 4 (dont 1 duale), registre complet 7 |
| Lemmes génériques vérifiés | 6 (algèbre) | 6 + **35 = 41** |
| `Dec (a ≤ b)` calculé et vérifié | 0 | 4 décisions fermées + 1 refus |
| Contrôles verts Thorn (hors porte) | 296 | **346** (296 + 50) |
| Porte de non-régression | 17/17, REUSSIS=165 | **17/17, REUSSIS=165 inchangé** |
| Changements du noyau | 0 | **0** (2 équations de signature documentées) |

Porte re-mesurée **après** ajout : `bash thorn/tests_regression.sh` →
`cas exécutés : 17 / 17 attendus`, `réussis : 17`, `échoués : 0`,
`compteurs observés : REUSSIS=165`, `BILAN: OK — aucune régression`
(rc=0, 19,31 s, 19,90 s puis 19,31 s mesurées, la dernière fois **après** toutes
les modifications, y compris `quitter`). La mesure de la même porte **avant** ajout,
au début de la session et sur une machine chargée, donnait 570,58 s ; les deux
mesures ne sont pas comparables (les fichiers joués par la porte sont
identiques, seules la charge et les caches ont changé) — ce sont les
**compteurs** qui font foi.

Suites M0 rejouées **après** ajout (aucune régression) :
`./rose/rose thorn/tests_hierarchie.rose` → `19 contrôle(s) réussi(s), 0 échec(s)`,
`RESULTAT: OK` (rc=0, 137,07 s) ; `./rose/rose thorn/tests_instances.rose` →
`10 contrôle(s) réussi(s), 0 échec(s)`, `RESULTAT: OK` (rc=0, 34,27 s) ;
`./rose/rose thorn/tests_algebre.rose` → `34 lemme(s) vérifié(s), 0 échec(s)`,
`RESULTAT: OK` (rc=0, 155,61 s). Soit **63 contrôles M0 verts** rejoués après M1.
Les six `./rose/rose verif` des modules M0/M1 (hierarchie, instances, ordres,
tests_hierarchie, tests_instances, tests_ordres) rendent tous `vérifié`.

### 4bis.4 Cause technique des 6 échecs initiaux (tous corrigés)

Le premier rejeu donnait 44 réussis / 6 échecs ; chaque échec avait une cause
précise, aucune n'a été masquée :

1. `LatticeAbstrait est un treillis` — `ord_lattice` étend `PartialOrder`, pas
   `LinearOrder` : le prédicat `ord_est_linear` était donc faux. Corrigé en
   vérifiant `ord_niveau == "Lattice"`.
2. `gen/le_of_eq_symm` — motif `eq_rec` inversé : pour `h : b = a`, le motif
   doit être `λz. le z b` avec prémisse `refl b`, pas `λz. le a z`.
3. `gen/antisymm[Nat/dual]` — les domaines des λ de l'instance duale utilisaient
   `le a b` au lieu de la relation duale `le b a`.
4. `lat/max_comm` — `max_le(a, b, max b a)` réclame `le a (max b a)` et
   `le b (max b a)` : ce sont `le_max_r(b,a)` et `le_max_l(b,a)`, pas les
   deux `le_max_*` de `(a,b)`.
5. `lat/max_assoc` — la borne `le a (max (max a b) c)` exigeait une transitivité
   `a → max a b → max (max a b) c`.
6. `lat/max_monotone` — la prémisse `h : a ≤ b` n'était pas utilisée ; le
   terme passé à `max_le` avait le mauvais domaine.

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
| Décidabilité d'une relation opaque (`M1`) | `le` est une constante de signature ; aucune ι-réduction ne calcule `succ n ≤ n`, et aucun lemme inductif ne donne `Dec (a ≤ b)` pour des variables | 2 équations de signature documentées §4bis.1 : `le_succ_pas_ax` (cas faux fermé) et `le_total_ax` (champ `total`) ; le calcul effectif reste vérifié pour les numéraux fermés |

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

- **Les ordres typeclassés sont désormais livrés (M1, §4bis)** ; il reste :
  pas d'anneaux au sens `Ring` (M2), pas de topologie ni d'analyse (M3).
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

## 7. Prochaine action : M2 — SEMI-ANNEAUX ET ANNEAUX

M1 est livré et vérifié (§4bis) : 35 lemmes génériques, 4 instances d'ordre,
50/50 contrôles, 346 contrôles verts cumulés, porte 17/17 inchangée. Le jalon
suivant est **M2**, cadré avec les dépendances exactes ci-dessous.

### 7.1 Dépendances exactes (toutes disponibles à ce jour)

| Dépendance | Fournie par | Emploi précis dans M2 |
|---|---|---|
| `mg_magma`, `mg_semigroupe`, `mg_monoid`, accesseurs `mg_*_de`, `mg_registre_trouver` | `thorn/src/hierarchie.rose` (M0) | Base des structures `AddMonoid`, `MulMonoid` ; `Semiring = AddMonoid + MulMonoid + Distrib` |
| Lois certifiées `al_lem_plus_assoc`, `al_lem_plus_zero_[ld]`, `al_lem_mult_assoc`, `al_lem_mult_un_[gd]` | `thorn/src/algebre.rose` (termes vérifiés) | Champs de loi des instances `Nat/+/0` et `Nat/*/1`, sans axiome nouveau |
| Distributivités vérifiées `al_lem_mult_distrib_g`, `al_lem_mult_distrib_d` | `thorn/src/algebre.rose` | Champ `distrib` de l'instance `Semiring Nat` |
| `al_lem_mult_zero_d`, `al_lem_mult_zero_g`, `al_lem_mult_zero_absorb` | `thorn/src/algebre.rose` | Lois `zero_mul` / `mul_zero` du semi-anneau |
| `mg_rewriter` / `mg_rewriter_droite` (eq_rec, re-vérifiés) | M0 | Normaliseur de polynômes par réécriture à carburant |
| `mg_champ`, `mg_loi`, `mg_coerce_tab`, `mg_compter`, `mg_lance` | M0 | Schéma de construction des instances et du rejeu |
| `ord_partial`, `ord_linear`, `ord_lem_trans`, `ord_lem_antisymm`, `ord_lem_total_dec` | `thorn/src/ordres.rose` (M1) | `OrderedRing` / `LinearOrderedRing` : compatibilité `≤` avec `+` et `×` |
| `ord_registre_complet()` (7 instances) | M1 | Registre unique anneaux + ordres, sans doublon de nom |
| ι de `plus` et `nat_rec`, `Bool`, `eq_rec`, `convertible` βι+α | `thorn/src/noyau.rose` | Calcul des polynômes et des décisions sur les coefficients Nat |

### 7.2 Livrables visés et cible mesurable

1. `thorn/src/anneaux.rose` : `AddMonoid`, `MulMonoid`, `Distrib`, `Semiring`,
   `Ring` (champs de loi = termes vérifiés) ; instance `Nat` sans axiome nouveau.
2. `thorn/src/entiers.rose` : `ℤ` comme paire `(Nat, Nat)` quotientée par
   `a - b ~ c - d ↔ a + d = c + b` ; `neg`, `add`, `mul`, et les lois de
   `Ring`. Le noyau ne connaît que `Nat`, `Bool`, `List` (registre ι fermé) :
   **aucun nouvel inductif n'est nécessaire**, la paire est un terme `Nat × Nat`
   encodé, et le quotient est une relation de signature (à documenter comme
   axiome explicite, au même titre que `le_succ_pas_ax` en M1).
3. Normaliseur de polynômes : `mg_rewriter` avec carburant décroissant, chaque
   réécriture re-vérifiée par `verifier` + `convertible`.
4. `thorn/tests_anneaux.rose` : cible ≥ 60 lemmes génériques, ≥ 4 instances,
   0 axiome nouveau pour Nat ; porte 17/17 et `REUSSIS=165` inchangées ;
   les suites M0 (19/0, 10/0) et M1 (50/0) doivent rester vertes.

### 7.3 Ce que M2 ne demandera PAS

Aucun changement de `noyau.rose` : comme en M0 et M1, une classe est une table
Rose, une loi est un terme de preuve, et la synthèse est une pré-élaboration.
Le passage à des ℤ « natifs » (inductif utilisateur, récursivité vérifiée,
univers `Type(1)`) reste repoussé à M3/M5 et exigerait un changement de noyau
décrit en §5.B — il n'est pas présupposé ici.

### 7.4 Règle de passage

Chaque lemme est soumis à `verifier` puis `convertible` ; aucun compteur n'est
écrit en dur ; `bash thorn/tests_regression.sh` reste à 17/17 avec
`REUSSIS=165`. Après toute modification de `thorn/src/**`, re-mesurer les
chiffres de ce document : ils sont des constats datés, pas des consignes.
