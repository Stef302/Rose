# Thorn — algèbre élémentaire vérifiée (TH5)

Fichiers livrés : `thorn/src/algebre.rose`, `thorn/tests_algebre.rose`, ce document.
Aucun fichier existant n'a été modifié. Aucun commit, aucune publication.

## 1. Méthode de vérification (identique à TACTIQUES.md)

Chaque lemme construit un **terme** `t` (table Rose au format `SPEC.md`), puis :

```
r   = verifier(t, ta_ctx())                 # synthèse par le noyau
ok  = noy_est_ok(r) ET convertible(noy_type_res(r), but)
```

C'est exactement `al_test()` dans `algebre.rose`, appelé par `al_lance()` depuis
`tests_algebre.rose`. Aucun `ok = vrai` n'est codé en dur : le `OK / ECHEC`
affiché est calculé par le noyau (`verifier` + `convertible`). Les preuves par
induction passent par `induction()` / `induction_resolue()` de `tactiques.rose`,
qui typent réellement la base et le pas avant de soumettre le terme fermé.

## 2. Résultat réel

`./rose/rose verif thorn/src/algebre.rose` → `vérifié`
`./rose/rose verif thorn/tests_algebre.rose` → `vérifié`
`./rose/rose thorn/tests_algebre.rose` → **33 lemme(s) vérifié(s), 0 échec(s)**,
`RESULTAT: OK` (exécution réelle, ~2 min 13 s sur cette machine : les termes
de preuve de la distributivité et de l'associativité de × sont volumineux).

## 3. Axiomatique explicite — ce qui est SUPPOSÉ

Le noyau n'a ni ι-réduction pour les inductifs, ni récursion, ni règle de
discriminateur pour `succ`. Les éléments suivants sont donc des **constantes de
signature** dont seul le type est déclaré ; ils ne sont PAS démontrés ici :

### Déjà présents dans `tactiques.rose` (non dupliqués)
- `Nat` (type inductif `ind`), `zero : Nat`, `succ : Nat -> Nat`, `plus : Nat -> Nat -> Nat` ;
- `Eq A a b` (`ind("=", [a,b])`), `refl`, `eq_rec`, `nat_rec` ;
- `plus_0_l : Πn, 0 + n = n` et `plus_s_l : Πm n, succ m + n = succ (m + n)`
  (équations computationnelles de `+`, non calculables par le noyau).

### Nouveaux dans `algebre.rose`
- `mult : Nat -> Nat -> Nat` (noté `×`) ;
- `mult_0_r : Πn, n × 0 = 0` — équation computationnelle de × (récursion sur le **second** argument) ;
- `mult_s_r : Πn m, n × succ m = (n × m) + n` — seconde équation de × ;
- `le : Nat -> Nat -> Type(0)` (noté `≤`) ;
- `le_refl_ax : Πn, n ≤ n`, `le_trans_ax : Πn m p, n ≤ m -> m ≤ p -> n ≤ p`,
  `le_antisymm_ax : Πn m, n ≤ m -> m ≤ n -> n = m`,
  `le_zero_ax : Πn, 0 ≤ n`,
  `le_plus_ax : Πn m p, n ≤ m -> n + p ≤ m + p` ;
- `succ_inj_ax : Πn m, succ n = succ m -> n = m`,
  `succ_ne_zero_ax : Πn, succ n = 0 -> Faux`, `Faux : Type(0)`.

**Conséquence honnête** : les cinq propriétés de `≤` (réflexivité, transitivité,
antisymétrie, minimalité de 0, compatibilité avec `+`) et les deux propriétés de
`succ` (injectivité, `succ ≠ 0`) sont des **axiomes** ; les lemmes correspondants
du §4 sont des applications vérifiées de ces axiomes, pas des démonstrations à
partir d'une définition de `≤`. La compatibilité à gauche, `le_succ_mono` et
`le_add_droite` sont en revanche dérivées (par `eq_rec` et les lemmes de `+`).

**Ce qui est réellement DÉMONTRÉ par induction vérifiée** : toutes les lois de ×
(zéro à gauche, successeur à gauche, identité à gauche, distributivité à
droite/gauche, associativité, absorption), les lois de `+` listées ci-dessous,
`succ_commute`, les cancellations et `plus_assoc4`. Les congruuences de `×` et
de `≤` sont des termes `eq_rec` vérifiés, pas des axiomes.

## 4. Les 33 lemmes vérifiés

### Addition (10)
| Lemme | Énoncé (notation lisible) | Nature |
|---|---|---|
| `plus_zero_d` | `Πn, n + 0 = n` | démontré par induction (terme propre, indépendant de `bibliotheque.rose`) |
| `plus_assoc` | `Πm n p, (m+n)+p = m+(n+p)` | démontré par induction (terme propre) |
| `succ_commute` | `Πn m, succ n + m = n + succ m` | démontré (`plus_s_l` + `plus_succ_d`) |
| `plus_un_d` | `Πn, n + 1 = succ n` | démontré |
| `plus_un_g` | `Πn, 1 + n = succ n` | démontré |
| `plus_deux_d` | `Πn, n + 2 = succ (succ n)` | démontré |
| `plus_congr_d` | `Πa b p, a = b -> a+p = b+p` | `eq_rec` vérifié |
| `plus_cancel_d` | `Πp n m, n+p = m+p -> n = m` | démontré par induction + injectivité de succ |
| `plus_cancel_g` | `Πp n m, p+n = p+m -> n = m` | démontré par induction + injectivité de succ |
| `plus_assoc4` | `Πa b c d, ((a+b)+c)+d = a+(b+(c+d))` | démontré |

La **commutativité** `plus_comm` existe déjà dans `bibliotheque.rose` : elle
n'est PAS dupliquée dans la liste. Son terme est reconstruit en interne
(`al_term_plus_comm_interne()`, lui aussi vérifié) uniquement comme outil de
preuve de `mult_succ_g`. L'identité à gauche `0+n = n` est l'axiome `plus_0_l`
déjà présent et n'est pas non plus comptée comme nouveau lemme.

### Multiplication (12)
| Lemme | Énoncé | Nature |
|---|---|---|
| `mult_zero_d` | `Πn, n × 0 = 0` | application de l'équation `mult_0_r` |
| `mult_succ_d` | `Πn m, n × succ m = (n×m)+n` | application de l'équation `mult_s_r` |
| `mult_zero_g` | `Πn, 0 × n = 0` | démontré par induction |
| `mult_succ_g` | `Πn m, succ n × m = (n×m)+m` | démontré par induction |
| `mult_congr_g` | `Πa b p, a=b -> a×p = b×p` | `eq_rec` vérifié |
| `mult_congr_d` | `Πa b p, a=b -> p×a = p×b` | `eq_rec` vérifié |
| `mult_un_d` | `Πn, n × 1 = n` | démontré |
| `mult_un_g` | `Πn, 1 × n = n` | démontré |
| `mult_distrib_g` | `Πc a b, a×(b+c) = (a×b)+(a×c)` | démontré par induction |
| `mult_distrib_d` | `Πc a b, (a+b)×c = (a×c)+(b×c)` | démontré par induction |
| `mult_assoc` | `Πc a b, (a×b)×c = a×(b×c)` | démontré par induction (using `mult_distrib_g`) |
| `mult_zero_absorb` | `Πn m, (n×0)×m = 0` | démontré |

### Ordre (8)
| Lemme | Énoncé | Nature |
|---|---|---|
| `le_refl` | `Πn, n ≤ n` | application de l'axiomatique de ≤ |
| `le_trans` | `Πn m p, n≤m -> m≤p -> n≤p` | application de l'axiomatique de ≤ |
| `le_antisymm` | `Πn m, n≤m -> m≤n -> n=m` | application de l'axiomatique de ≤ |
| `le_zero_min` | `Πn, 0 ≤ n` | application de l'axiomatique de ≤ |
| `le_plus_compatible_d` | `Πn m p, n≤m -> n+p ≤ m+p` | application de l'axiomatique de ≤ |
| `le_plus_compatible_g` | `Πn m p, n≤m -> p+n ≤ p+m` | dérivé (`eq_rec` + `plus_comm`) |
| `le_succ_mono` | `Πn m, n≤m -> succ n ≤ succ m` | dérivé (`+1` + `eq_rec`) |
| `le_add_droite` | `Πn p, n ≤ n+p` | dérivé (`0≤p` + compatibilité) |

### Succ (3)
| Lemme | Énoncé | Nature |
|---|---|---|
| `succ_inj` | `Πn m, succ n = succ m -> n = m` | application de l'axiomatique de succ |
| `succ_ne_zero` | `Πn, succ n = 0 -> Faux` | application de l'axiomatique de succ |
| `zero_ne_succ` | `Πn, 0 = succ n -> Faux` | dérivé (`symm` + `succ_inj`-axiom) |

## 5. Périmètre honnête — ce qui N'EST PAS fourni

- **Pas de soustraction**, pas de prédécesseur, pas de `n - m`. Les cancellations
  sont des implications (`n+p = m+p -> n=m`), pas une opération.
- **Pas de division**, pas de quotient, pas de reste, pas de PGCD.
- **Pas de nombres négatifs**, pas d'entiers relatifs, pas de groupe additif.
  Le monoïde additif est `(Nat, +, 0)` ; il n'y a pas d'inverses.
- **Pas de grandeur ni de comparaison décidable** : `le` est une relation
  postulée, pas un calcul booléen ; aucun `décider n ≤ m`.
- **Pas de commutativité de ×** : elle n'est pas demandée ici et n'est pas
  prouvée (elle suivrait des lemmes ci-dessus, mais n'est pas livrée).
- **Univers unique** `Type(0)`, pas de polymorphisme d'univers.
- Les opérations restent des constantes de signature : le noyau ne calcule
  `0+n`, `succ m+n`, `n×0` ou `n×succ m` que via les axiomes de signature,
  jamais par ι-réduction. `convertible` ne fait que β + α.

## 6. Compatibilité avec les suites existantes

`algebre.rose` importe uniquement `tactiques.rose` (donc `noyau.rose`) ; il ne
modifie ni `noyau.rose`, ni `bibliotheque.rose`, ni `tactiques.rose`, ni
`analyse.rose`, ni `rose/src/**`. Les suites `tests_noyau.rose`,
`tests_syntaxe.rose`, `tests_tactiques.rose`, `tests_pilote.rose` et le rejeu des
10 lemmes de `bibliotheque.rose` restent inchangés et verts.
