# GUIDE_GEANTS — nombres géants « façon Wolfram » dans Rose

**Constat du 21/11/2026**, vérifié sur le binaire compilé depuis l'arbre de
travail (câblage INT1 inclus). Les sorties ci-dessous sont des sorties réelles
copiées depuis l'exécution, pas des promesses. Les temps sont des mesures
datées : à re-mesurer après tout changement de code ou de machine.

Rose affiche désormais les très grands nombres comme un calculateur : **30
chiffres puis `…`**, avec une commande pour en demander plus, un `detail()`
typé qui ne calcule que les propriétés demandées, et un bouton « Plus de
chiffres » dans une fenêtre native.

---

## 1. Démarrage en trois lignes

Créez `exemple.rose` :

```rose
>> entier_geant_factorielle(100)
```

Exécutez :

```bash
rose exemple.rose
```

Sortie réelle, sur une seule ligne :

```
933262154439441526816992388562…
```

`entier_geant_factorielle(100)` ne matérialise **pas** 100! : il décrit « 100! »
et ne calcule que les 30 chiffres affichés. Le chemin historique `factorielle(100)`
lui, calcule toujours le nombre exact (158 chiffres) — il n'a pas changé.

---

## 2. Syntaxe exacte

| Appel | Ce que ça rend | Erreur si… |
|---|---|---|
| `entier_geant_factorielle(n)` | une valeur symbolique paresseuse décrivant `n!` | `n < 0` |
| `chiffres(expr, n)` | une **chaîne** avec exactement les `n` premiers chiffres : ni signe, ni séparateur, ni `…`, ni remplissage | `n < 0` ; `n` omis → 30 |
| `plus_de_chiffres(expr, n)` | la chaîne cumulée des chiffres de **ce même objet**, terminée par `…` | `n < 0` ; `expr` n'est pas un entier géant symbolique ; `n` omis → 30 |
| `detail(expr)` | une **table Rose** de propriétés pertinentes pour le type | — |
| `afficher_detail(detail(expr))` | le bloc lisible, 2 espaces d'indentation, libellé aligné sur 28 caractères, séparateur ` : ` | l'argument n'est pas une table |

`n` peut être bien plus grand qu'un `i64` : `10^37` est accepté (le moteur
stocke `n` en `BigInt`). Les appels `chiffres` et `plus_de_chiffres` sont
réitérables sur la même valeur.

---

## 3. La limite des 30 chiffres

Par défaut, une valeur géante s'affiche en **30 chiffres suivis de `…`** :

```rose
>> entier_geant_factorielle(100)
>> entier_geant_factorielle(10^37)
```

```
933262154439441526816992388562…
901820028144300502340101362711…
```

Quand le nombre tient en 30 chiffres ou moins, aucun `…` n'apparaît :
`>> entier_geant_factorielle(10)` rend `3628800`.

Pour voir au-delà de 30 chiffres dans un programme, utilisez `chiffres`
(choix explicite) ou `plus_de_chiffres` (cumul) :

```rose
$o = entier_geant_factorielle(100)
>> chiffres($o, 30)
>> plus_de_chiffres($o, 30)
>> chiffres($o, 158)
```

```
933262154439441526816992388562
933262154439441526816992388562667004907159682643816214685929…
93326215443944152681699238856266700490715968264381621468592963895217599993229915608941463976156518286253697920827223758251185210916864000000000000000000000000
```

Le préfixe des 30 premiers chiffres est **stable** : demander 30 chiffres de
plus ne le change jamais.

---

## 4. `detail()` : trois exemples copiables

`>> detail(x)` rend la table compacte Rose. Pour le bloc lisible, appelez
`afficher_detail(...)` **comme instruction**, sans `>>` :

```rose
afficher_detail(detail(entier_geant_factorielle(100)))
```

```
  Puissance de 10              : 10^157.9700036547157883739149248029898481939453
  Premiers chiffres            : 933262154439441526816992388562…
  Longueur                     : 158 decimal digits
  Zeros finaux                 : 24
  Derniers non nuls            : 210916864
```

Mille fois plus gros, même coût d'affichage court :

```rose
afficher_detail(detail(entier_geant_factorielle(1000000)))
```

```
  Puissance de 10              : 10^5565708.9171867185426132211333662504586361080781
  Premiers chiffres            : 826393168833124006237664610317…
  Longueur                     : 5565709 decimal digits
  Zeros finaux                 : 249998
  Derniers non nuls            : 58412544
```

Et l'extrême, `10^37` :

```rose
afficher_detail(detail(entier_geant_factorielle(10^37)))
```

```
  Puissance de 10              : 10^365657055180967481723488710810833949195.9551198761423918636379657598490494581248
  Premiers chiffres            : 901820028144300502340101362711…
  Longueur                     : 365657055180967481723488710810833949196 decimal digits
  Zeros finaux                 : 2499999999999999999999999999999999990
  Derniers non nuls            : 467172352
```

`detail()` est **dynamique** : il n'affiche que les propriétés du type. Un
entier premier n'a pas de « zéros finaux », un rationnel n'a pas de
« longueur en chiffres décimaux », une liste n'a pas de propriétés
arithmétiques. Aucune ligne n'est inventée pour remplir un gabarit.

---

## 5. Instantané contre coûteux

Mesures réelles du 21/11/2026, binaire `release`, 3 à 5 lancements, temps de
démarrage du processus inclus sauf indication :

| Commande | Temps observé | Pourquoi |
|---|---|---|
| `>> entier_geant_factorielle(10^37)` | **2,2 à 3,4 ms** médian (salves de 3 mesures, calcul seul) | `log10` en précision arbitraire + 70 chiffres de tête, jamais `n!` |
| `>> entier_geant_factorielle(1000000)` | ~5 ms avec démarrage | idem |
| `afficher_detail(detail(entier_geant_factorielle(10^37)))` | **531 ms** (5 lancements) | `detail()` calcule aussi les derniers chiffres non nuls (produit sur 5^9) |
| `>> factorielle(100000)` (chemin exact) | ~51 ms | calcule réellement les 456 574 chiffres |
| `>> factorielle(10^37)` | erreur immédiate `factorielle trop grande` | le chemin exact refuse `n > 0xFFFF_FFFF` |

Règle pratique : **afficher est instantané, `detail()` est rapide mais pas
gratuit pour un `n` gigantesque**. Ne demandez pas `detail(10^37)` dans une
boucle.

---

## 6. REPL : `:more`, `:plus`, `:digits`

Dans la console interactive (`rose console`) :

```
rose> entier_geant_factorielle(100)
933262154439441526816992388562…
rose> :more
933262154439441526816992388562667004907159682643816214685929…
rose> :plus
933262154439441526816992388562667004907159682643816214685929638952175999932299156089414639…
rose> :more 10
9332621544394415268169923885626670049071596826438162146859296389521759999322991560894146397615651828…
rose> :reset
(buffer vidé)
rose> :more
rose: :more nécessite d'abord detail(expr) (résultat non symbolique)
```

- `:more` ajoute 30 chiffres ; `:more N` ajoute `N` chiffres.
- `:plus` et `:digits` sont des alias de `:more`.
- `:reset` vide le buffer et remet le compteur à zéro.
- Sans résultat symbolique préalable, le message d'erreur ci-dessus est
  affiché, en rouge dans un vrai terminal, et le buffer n'est pas modifié.
- `:help` liste `:more` ; la complétion Tab propose `:mo` → `:more`.

---

## 7. Le bouton « Plus de chiffres » (GUI native)

Toute l'interface est décrite en Rose dans `jardins/gui_native.rose`, fonction
`grand_nombre_fenetre(expression)`. Aucun libellé n'est codé en Rust.

Programme headless (sans écran) prouvant la fenêtre et le bouton :

```rose
importer "jardins/gui_native.rose"
$f = grand_nombre_fenetre("entier_geant_factorielle(100)")
>> decrire_grand_nombre($f)
>> geometrie_boutons($f)
```

Sortie réelle, tronquée aux lignes utiles :

```
FENETRE|Grands nombres : entier_geant_factorielle(100)|520|240|widgets=6|actions=1
W|champ|chiffres|12|16
W|bouton|Plus de chiffres|12|56
W|case|10|12|96
W|case|30|72|96
W|case|100|132|96
W|case|1000|200|96
EXPRESSION|entier_geant_factorielle(100)
CHIFFRES|
PAS|30
CLICS|0
LECTURE_SEULE|1
VALEUR_CHAMP|
ACTION_PLUS|1|plus_de_chiffres

B|1|Plus de chiffres|12|56|160|28
```

Le pas est exclusif (10 / 30 / 100 / 1000, 30 par défaut). Le pont natif
accepte la géométrie produite par le jardin ; un clic peut être simulé sans
serveur X :

```rose
$h = fenetre_creer_simulee("Grands nombres", 520, 240)
fenetre_boutons_creer($h, geometrie_boutons($f))
bouton_clic_simuler($h, 1)
>> bouton_attendre_clic_limite($h, 20)
```

Sortie réelle : `CREE|1`, `NB|1`, `CLIC|`, `ATT|1` — le bouton d'identifiant 1
a bien été reçu. La production des chiffres reste la affaire de VAL1 :
le jardin met à jour le champ et le compteur, il ne calcule rien.

---

## 8. Avertissements honnêtes

1. **`factorielle(n)` reste le chemin exact.** Il refuse
   `n > 0xFFFF_FFFF` avec `factorielle trop grande`. Utilisez
   `entier_geant_factorielle(n)` pour les géants.
2. **Compilateur auto-hébergé : pas de bigints.** L'ABI G5 du compilateur écrit
   en Rose limite les entiers à 64 bits : `18446744073709551616` (`2^64`) y
   devient `0` et les grands décimaux sont tronqués. Le **moteur** Rose, lui,
   calcule exactement. Ne jugez pas l'un avec l'autre.
3. **Derniers chiffres non nuls : 9 au maximum.** `detail()` plafonne à
   9 chiffres (`210916864` pour 100!, `467172352` pour `(10^37)!`), alors que
   le document de design `DETAIL_UX.md` en décrivait 16. C'est une limite
   délibérée du noyau (au-delà, le calcul retombe sur un produit direct O(n),
   intraitable pour `10^37`), pas un oubli.
4. **`afficher_detail(...)` s'écrit en instruction.** Précédé de `>>`, il
   rend `Rose : erreur — Rien à afficher ici` : la fonction écrit ses lignes
   puis rend une valeur vide.
5. **`chiffres` et `plus_de_chiffres` refusent les pas négatifs.**
   `chiffres(x, 0)` rend la chaîne vide ; `plus_de_chiffres` n'accepte qu'une
   valeur symbolique et rend une erreur explicite sur un entier ordinaire.
6. **Constat daté.** Les temps et sorties de ce guide ont été mesurés le
   21/11/2026 ; re-vérifiez-les après toute modification du moteur, de la
   quantification ou de la machine.
