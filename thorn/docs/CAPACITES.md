# Thorn — ce qu'il a dans le ventre (inventaire vérifiable)

Document créé le **22/11/2026** par la mission « scripts de capacités + comparaison Lean ».
Fichier possédé par cette mission : `thorn/docs/CAPACITES.md` (nouveau).
Aucun commit, aucune publication, aucune modification de `thorn/src/**`,
`thorn/tests_*.rose`, `thorn/docs/MATHLIB_ROADMAP.md`, `thorn/docs/BENCH_LEAN.md`,
`thorn/exemples/**`, `compilateur/**`, `rose/src/**`, `scripts/**`.

**Règle de vérité.** Tout ce qui est marqué **MESURÉ ICI** a été obtenu le 22/11/2026
sur cette machine avec les commandes exactes reproduites ci-dessous, et peut être
rejoué par `bash thorn/scripts/capacites.sh`. Ce qui est marqué **CITÉ** vient d'un
document du dépôt et n'est pas une mesure nouvelle. Le script
`bash thorn/scripts/verifier_proprete.sh` rejoue les cas et vérifie que chaque
ligne `EXTRAIT: …` de l'annexe est réellement produite.

---

## 0. Environnement (MESURÉ ICI, 22/11/2026)

```
$ ./rose/rose version
Rose 0.1.73 — langage de programmation en français (maths exactes, JIT, AOT, compilation autonome)

$ nproc
20

$ uptime
load average: 2.25, 2.61, 2.45        # un coeur environ pris par C2 (self-hosting)
```

Le binaire du dépôt (`./rose/rose`) est impératif : le `rose` du PATH est une autre
version. Les mesures ont été faites **séquentiellement** (un seul process Rose à la
fois) pour ne pas perturber C2. Aucune commande de ce document ne dépasse 60 s :
la plus lourde, `tests_algebre`, a été chronométrée à **29,69 s / 5 561 040 Ko**
avec garde-fou mémoire (voir §10).

Catalogue complet : `$ bash thorn/scripts/capacites.sh` → **15 cas, 15 OK, 0 ÉCHEC**.

---

## 1. Termes et types dépendants (Π) — le terme d'identité

**Code** (`thorn/exemples/ex01_identite.rose`, extrait) :

```rose
$e1_but  = noy_pi("A", noy_type(0), noy_pi("x", noy_var("A"), noy_var("A")))
$e1_term = noy_lam("A", noy_type(0), noy_lam("x", noy_var("A"), noy_var("x")))
$e1_r    = verifier($e1_term, noy_ctx_vide())
>> "ok      = " + chaine(noy_est_ok($e1_r))
>> "type    = " + m(noy_type_res($e1_r))
>> "convert = " + chaine(convertible(noy_type_res($e1_r), $e1_but))
```

**Commande** : `$ ./rose/rose thorn/exemples/ex01_identite.rose`

**Sortie réelle** :

```
terme   = Lam(A:Type(0), Lam(x:A, x))
ok      = vrai
type    = Pi(A:Type(0), Pi(x:A, A))
code    = OK
convert = vrai
```

**Limite** : un seul univers (`Type(0)`) dans le terme ; la cumulativité est une
fonction Rose à part (§4), pas une règle du noyau.

---

## 2. β-réduction et α-équivalence

**Code** (`thorn/exemples/ex02_beta_alpha.rose`, extrait) :

```rose
$e2_app = noy_app(noy_lam("x", noy_var("X"), noy_var("x")), noy_var("a"))
>> "beta(App(Lam(x:X,x), a)) = " + m(beta($e2_app))
$e2_l1 = noy_lam("x", noy_var("X"), noy_var("x"))
$e2_l2 = noy_lam("y", noy_var("X"), noy_var("y"))
>> "alpha(Lam(x:X,x), Lam(y:X,y)) = " + chaine(convertible($e2_l1, $e2_l2))
```

**Commande** : `$ ./rose/rose thorn/exemples/ex02_beta_alpha.rose`

**Sortie réelle** :

```
beta(App(Lam(x:X,x), a)) = a
convertible(app, a)       = vrai
alpha(Lam(x:X,x), Lam(y:X,y)) = vrai
alpha(Lam(x:X,x), Lam(y:X,x)) = faux
```

**Limite** : β + α seulement. Le noyau ne fait **pas** d'ι-réduction : `plus zero n`
est traité par l'exemple §3 comme une réécriture, pas comme une réduction du noyau.

---

## 3. ι / réécriture et `nat_rec`

**Code** (`thorn/exemples/ex03_iota.rose`, extrait) :

```rose
>> "beta(plus zero n)         = " + m(beta(ta_plus(ta_zero(), noy_var("n"))))
>> "convertible(0+n, n)       = " + chaine(convertible(...))
```

**Commande** : `$ ./rose/rose thorn/exemples/ex03_iota.rose`

**Sortie réelle** :

```
beta(plus zero n)         = n
beta(plus (succ zero) n)  = App(succ, n)
convertible(0+n, n)       = vrai
convertible(succ 0+n, succ n) = vrai
beta(nat_rec P base pas zero) = zero
verifier(nat_rec ... zero) ok = vrai
son type                      = Nat
```

**Limite** : les équations `plus_0_l` / `plus_s_l` sont des **axiomes de signature**.
Leur emploi est contrôlé (`convertible` porte le juge final), mais ce n'est pas une
démonstration from scratch des lois de l'addition.

---

## 4. Cumulativité (limitée)

**Commande** : `$ ./rose/rose thorn/exemples/ex05_cumul.rose`

**Sortie réelle** :

```
convertible(Type0, Type1) = faux
cumulatif(Type0, Type1)   = vrai
cumulatif(Type1, Type0)   = faux
cumulatif(Type0, Type0)   = vrai
verifier((λX:Type1. X) Type0) ok = vrai
type synthétisé                = type
convertible(terme, but strict)  = faux
```

**Limite** : `cumulatif` est un prédicat Rose distinct de `convertible` ; un terme
accepté par cumulativité n'est pas convertible à son but strict.

---

## 5. Syntaxe française → AST

**Commande** : `$ ./rose/rose thorn/exemples/ex06_syntaxe.rose`

**Sortie réelle** (extraits) :

```
1) a ou b et c implique d
[genre => pi, nom => _, domaine => [genre => ind, nom => ou, ...], codomaine => [genre => var, nom => d]]
3) fn x : Nat => x
[genre => lam, nom => x, domaine => [genre => var, nom => Nat], corps => [genre => var, nom => x]]
4) pour_tout x : Nat, x = x
[genre => pi, nom => x, domaine => [genre => var, nom => Nat], codomaine => [genre => ind, nom => =, ...]]
5) existe x : Nat, x = zero
[genre => ind, nom => existe, ...]
```

**Limite** : l'analyse produit l'AST ; elle ne juge pas de la validité (c'est le
noyau qui le fait, §7).

---

## 6. Tactiques : réflexivité, symétrie, introduction, application, induction

**Commandes** :

```
$ ./rose/rose thorn/exemples/ex07_refl_sym.rose
$ ./rose/rose thorn/exemples/ex08_intro_app.rose
$ ./rose/rose thorn/exemples/ex09_induction.rose
```

**Sorties réelles** (lignes significatives) :

```
reflexivite : ok    = vrai
reflexivite : regle = reflexivite/lambda
symetrie : ok          = vrai
symetrie : regle       = symetrie/symm
symetrie sans hypothese = faux
raison                  = symétrie : aucune hypothèse de type a = b

introduction : ok        = vrai
introduction : regle     = introduction/pi
application : ok         = vrai
application : regle      = application/exact
simplifier(0 + n)        = n

induction : ok           = vrai
induction : regle        = induction/Nat
induction : nb buts      = 2
induction_resolue : ok   = vrai
verifier(terme) : ok     = vrai
convertible(type, but)  = vrai
```

**Limite** : l'induction est sur `Nat` uniquement ; le terme final repasse
toujours par `verifier` + `convertible` (aucun `OK` codé en dur).

---

## 7. Noyau : ce qu'il REFUSE (et ses codes d'erreur)

**Commande** : `$ ./rose/rose thorn/exemples/ex04_refus.rose`

**Sortie réelle** :

```
convertible(0, succ 0)   = faux
code(0 = Type(0))        = EQ_MAL_TYPE
contexte                 = ind(=)
code(var inconnu)        = VAR_NON_LIE
code(Type 0 appliqué)    = APP_PAS_PI
code(Ω avec limite 0)    = GARDE_PAS
message                  = depassement de la garde de pas (0)
```

**Limite** : la garde de pas borne la β-réduction ; c'est une protection, pas un
contrôle de terminaison général.

**Refus côté pilote** : `$ ./rose/rose thorn/src/principal.rose thorn/exemples/ex10_echec.thn`

```
ÉCHEC ex10_faux : réflexivité : les deux membres diffèrent
1 théorème(s), 0 réussi(s)
```

(rc=1 attendu : c'est une preuve fausse volontaire.)

---

## 8. Pilote `.thn` — la preuve d'associativité, ligne à ligne

**Commande** : `$ ./rose/rose thorn/src/principal.rose thorn/preuves/associativite.thn`

**Sortie réelle** :

```
OK associativite_plus
1 théorème(s), 1 réussi(s)
```

**Code** (extrait de `thorn/preuves/associativite.thn`, non modifié) :

```thorn
théorème associativite_plus :
  pour_tout m : Nat, pour_tout n : Nat, pour_tout p : Nat,
    (m + n) + p = m + (n + p)
preuve
  induction m
  cas zero :
    introduction n
    introduction p
    appliquer trans (congr_plus_l (plus 0 n) n p (plus_0_l n))
                    (symm (plus_0_l (plus n p)))
  ...
fin
```

**Coût (MESURÉ ICI, 3 répétitions)** : 1,19 / 1,20 / 1,33 s ; **248 124 Ko** au
pic (médiane 247 736 Ko dans la comparaison §COMPARAISON_LEAN). C'est le chiffre
« 248 Mo sur associativité » annoncé dans la demande.

**Limite** : `--trace` casse sur les buts à plusieurs Π (`clé absente de la table`),
mesuré dans `BENCH_LEAN.md` §1.3 ; le terme d'induction reste observable via
`ex09_induction.rose` (§6).

---

## 9. Hiérarchie et instances (M0), puis ordres et décidabilité (M1)

**Commande** : `$ ./rose/rose thorn/exemples/ex11_mathlib.rose`

**Sortie réelle** (MESURÉ ICI, 9,77 s / 3 740 972 Ko avec garde-fou) :

```
Thorn — exemple 11 : preuve generique reutilisable (hiérarchie + instances)
registre : 3 instance(s)
OK assoc4[Nat/+/0]
OK assoc4[Nat/*/1]
OK id_unique_d[Nat/+/0]
OK id_unique_d[Nat/*/1]
OK elaboration/Classe(Add,Nat) 0 1 : Nat
OK refus/Classe(Groupe,Nat) sans instance (attendu)
6 contrôle(s) réussi(s), 0 échec(s)
RESULTAT: OK
```

**Limite** : la synthèse d'instance est une **pré-élaboration** Rose (recherche
linéaire dans un registre), pas une typeclass dans le noyau.

**Ordres + décision + refus** — fichier généré par le catalogue :

```
$ ./rose/rose /tmp/opencode/thorn_capacites/demo_ordres.rose
OK ordre/refl[Nat]
OK ordre/trans[Nat]
OK [décision] decider/1 ≤ 2 (vrai)
OK [décision] decider/3 ≤ 1 (faux)
OK [refus] 0≤0 n'est pas 0≤1
OK=5 ECHEC=0
```

**Limite** : `Dec(a ≤ b)` est calculé pour des **numéraux fermés** ; pas de
décidabilité générique pour des variables.

---

## 10. Chiffres — distinguer le cité du mesuré

| Fait | Nature | Détail |
|---|---|---|
| **346 contrôles verts cumulés** | **CITÉ** (`thorn/docs/MATHLIB_ROADMAP.md`, lignes 364 et 492) | « 296 + 50 » : 296 avant le jalon M1, +50 contrôles d'ordres |
| 165 contrôles `tests_noyau` | **MESURÉ ICI** | rejoué : `REUSSIS=165 ECHOUES=0` |
| 50 contrôles `tests_ordres` | **MESURÉ ICI** | rejoué : `50 contrôle(s) réussi(s), 0 échec(s)` |
| 33 contrôles `tests_tactiques` | **MESURÉ ICI** | `REUSSIS=19` + `REUSSIS_SUP=14` |
| 12 contrôles `tests_pilote` | **MESURÉ ICI** | `REUSSIS=12 ECHOUES=0` |
| 8 contrôles `tests_syntaxe` | **MESURÉ ICI** | `OK = 8 ECHEC = 0` |
| 34 lemmes `tests_algebre` | **MESURÉ ICI** (cette mission, 29,69 s) | `34 lemme(s) vérifié(s), 0 échec(s)` |
| 19 contrôles `tests_hierarchie` | **CITÉ** (`BENCH_LEAN.md` §3.2, 22/11) | suite non rejouée ici (≈135 s > 60 s, C2 occupe un coeur) |
| 10 contrôles `tests_instances` | **CITÉ** (`BENCH_LEAN.md` §3.2, 22/11) | suite non rejouée ici (≈35 s) |
| 248 Mo sur associativité | **MESURÉ ICI** | 1,19 / 1,20 / 1,33 s ; 247 588–248 888 Ko |
| 30 s sur `tests_algebre` | **MESURÉ ICI** | 29,69 s / 5 561 040 Ko |

Re-comptage suite par suite ci-dessus : **331 contrôles** sur 8 suites, auxquels
s'ajoutent les 5 preuves `thorn/preuves/*.thn` (= 336). Le total de 346 est le
**cumul annoncé** par la feuille de route ; je ne le présente pas comme un résultat
de cette mission et je ne tente pas de le faire coïncider artificiellement.

---

## 11. Ce que Thorn ne sait pas faire (limites structurantes)

1. Univers unique (`Type(0)`), pas de cumulativité dans le noyau, pas d'η.
2. β + α seulement ; pas d'ι, pas de quotient, pas de contrôle de termination.
3. `plus_0_l`, `plus_s_l`, `nat_rec` : axiomes de signature.
4. Instances = pré-élaboration linéaire (pas de typeclasses, pas de priorité).
5. Décidabilité sur numéraux fermés seulement.
6. `--trace` en échec sur buts à plusieurs Π.
7. Aucun cache incrémental : chaque lancement reconstruit tout (d'où 30 s sur
   `tests_algebre` malgré ~0,3 s sur un terme isolé).
8. Comparaison détaillée, chiffres à l'appui : `thorn/docs/COMPARAISON_LEAN.md`.

---

## Annexe — extraits vérifiables par `verifier_proprete.sh`

```
EXTRAIT: ok      = vrai
EXTRAIT: alpha(Lam(x:X,x), Lam(y:X,y)) = vrai
EXTRAIT: convertible(0+n, n)       = vrai
EXTRAIT: cumulatif(Type0, Type1)   = vrai
EXTRAIT: [genre => pi
EXTRAIT: symetrie : ok          = vrai
EXTRAIT: introduction : ok        = vrai
EXTRAIT: convertible(type, but)  = vrai
EXTRAIT: code(Ω avec limite 0)    = GARDE_PAS
EXTRAIT: OK identite
EXTRAIT: OK associativite_plus
EXTRAIT: 1 théorème(s), 0 réussi(s)
EXTRAIT: 6 contrôle(s) réussi(s), 0 échec(s)
EXTRAIT: OK=5 ECHEC=0
EXTRAIT: REUSSIS=165
```
