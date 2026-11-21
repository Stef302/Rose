# G5B — batterie indépendante de validation de l'émetteur ELF

Constat mesuré, **21/11/2026 08:17 UTC** (3 salves concordantes). Sous-agent `G5B`. Dépôt
`/home/clausake/Stefan/ROSE` (`Stefan/ROSE`). Fichiers livrés, tous dans les
chemins possédés : **30 programmes `tests_selfhost/g5b5*.rose` /
`g5b6*.rose`**, **`tests_selfhost/lancer_g5b.sh`**, ce document. **Aucune
modification de `rose/src/**`, `compilateur/**`, de l'émetteur ou d'un autre
fichier existant ; aucun commit, aucune publication.**

Objectif : fournir à l'agent **G5A** un juge indépendant, avec des valeurs
**différentes** de celles de `rose/docs/AZ_TESTS.md`, pour vérifier que les
14 divergences documentées sont réellement corrigées de façon générale et non
pas seulement sur les exemples de la batterie AZ.

## 1. Protocole

`tests_selfhost/lancer_g5b.sh [filtre]` rejoue, pour chaque programme :

```
ROSE_SANS_JIT=1 rose/target/release/rose p.rose   > p.nat    # référence sémantique
rose/target/release/rose rir p.rose                > p.rir    # dump RIR, 2 fois
rose/target/release/rose <emetteur_figé> p.rir     > p.elf    # ELF émis, 2 fois
chmod +x p.elf && ./p.elf                          > p.elfout # sortie ELF, 2 fois
```

L'émetteur est **copié avant la salve** (`G5B_EMETTEUR`, défaut
`compilateur/emetteur_elf.rose`) : aucun fichier source n'est écrit, et un
autre agent peut modifier l'émetteur original pendant le mesurage sans
bouger le snapshot. Comparaisons **octet pour octet** (`cmp`) : `nat/ELF`,
`RIR1/RIR2`, `ELF1/ELF2`, `sortie1/sortie2`. Timeouts 25 s (interprétation,
dump, exécution ELF) et 90 s (émission). Classes :

- **CONFORME** — 4 rc = 0 et 4 comparaisons égales ;
- **DIVERGENT** — 4 rc = 0 mais `nat != ELF` ;
- **BLOQUÉ** — rc ≠ 0, timeout, artefact manquant ou sortie non reproductible.

Le runner n'omet aucun rc : `resultats.tsv` contient une ligne par programme
avec les 4 rc, les 8 md5 et les 4 résultats de `cmp` ; `rapport.txt` imprime
chaque blocage et les sorties NAT/ELF des divergences (12 premières lignes).

### Binaire et émetteur mesurés

| Élément | Valeur |
|---|---|
| Binaire Rose | `rose/target/release/rose`, md5 `3b27d46fc48e280a3b890aa71286802f` |
| Binaire AZ (pour mémoire) | `rose/rose`, md5 `12eac46007cf0fe9e1aec2dff6fbf665` |
| Émetteur figé | `compilateur/emetteur_elf.rose`, md5 `6c957b53e97f3ebca94a26c9a272cf54` |
| Émetteur AZ (pour mémoire) | md5 `a515b9626eaee92489639f8c72db7c20` |

L'émetteur courant n'est **pas** celui de la salve AZ : il a changé depuis
(la batterie AZ s'arrêtait à `a515b962…`). Toute nouvelle salve doit donc être
datée avec son md5.

## 2. Chiffres réels

Salve canonique (21/11/2026 08:17:19 UTC, durée 52 s) : **30 programmes,
12 CONFORMES, 17 DIVERGENTS, 1 BLOQUÉ**. Deux salves antérieures à
08:14:37 et 08:15:59 UTC ont donné exactement les mêmes classes.

| Mesure | Valeur |
|---|---|
| Programmes | 30 (20 ciblant les 14 divergences + 10 non-régression) |
| CONFORMES | **12** |
| DIVERGENTS | **17** |
| BLOQUÉS | **1** |
| Comparaisons NAT/ELF égales | 12/30 |
| RIR reproductible octet pour octet | 30/30 |
| ELF reproductible octet pour octet | 30/30 |
| Sortie ELF reproductible octet pour octet | 30/30 |
| Artefacts | `/tmp/opencode/g5b_tests/battery/{rapport.txt,resultats.tsv,*.nat,*.rir,*.elf,*.elfout}` |

### Par catégorie

| Catégorie | Tests | Conformes | Divergents | Bloqués |
|---|---|---|---|---|
| Unicode | 5 | 0 | 5 | 0 |
| Rationnel calculé | 4 | 0 | 4 | 0 |
| Puissance bigint | 3 | 1 | 2 | 0 |
| Soustraction bigint | 3 | 0 | 3 | 0 |
| Mul/div/mod bigint | 2 | 0 | 1 | 1 |
| abs/min/max bigint | 2 | 0 | 2 | 0 |
| Addition signes opposés | 1 | 1 | 0 | 0 |
| Non-régression | 10 | 10 | 0 | 0 |
| **Total** | **30** | **12** | **17** | **1** |

### Couverture des 14 divergences AZ

| # | Divergence documentée (AZ §4) | Tests G5B | Résultat |
|---|---|---|---|
| 1 | `unicode_octets_pas_points_de_code` (4 tests AZ) | g5b501–g5b505 | 5 divergents |
| 2 | `division_calculee_ancien_chemin_64bits` (3) | g5b510–g5b513 | 4 divergents |
| 3 | `puissance_big_non_repliee_non_emise` (2) | g5b520–g5b522 | 2 divergents + 1 conforme (témoin replié) |
| 4 | `soustraction_bigint_ancien_chemin_64bits` (2) | g5b530–g5b532 | 3 divergents |
| 5 | `addition_bigadd_signes_opposes` (1) | g5b560 | **conforme** (AZ t212 divergeait) |
| 6 | `mul_div_mod_bigint_ancien_chemin_64bits` (1) | g5b540, g5b541 | 1 divergent + 1 bloqué (segfault) |
| 7 | `abs_minmax_bigint` (1) | g5b550, g5b551 | 2 divergents |

Les 7 familles de causes sont donc toutes couvertes, avec des valeurs neuves.
Le point 5 est une **amélioration observée** par rapport à AZ : avec
l'émetteur courant, l'addition de grands entiers de signes opposés est devenue
conforme sur les valeurs testées ici comme sur celles d'AZ (`t212` rejoué en
contrôle). Les 6 autres familles restent divergentes avec de nouveaux
opérandes.

## 3. Divergences — entrées/sorties réelles

Toutes ont rc = 0 des deux côtés ; seul le contenu des octets diffère.
`NAT` = `ROSE_SANS_JIT=1 rose/target/release/rose p.rose`,
`ELF` = binaire émis exécuté.

### 3.1 Unicode (5 divergents)

Notation : `\uFFFD` dans les transcriptions ci-dessous représente un **octet
brut invalide** tel que le terminal l'affiche, et non la séquence UTF-8
`ef bf bd` du caractère U+FFFD. Les hexdumps donnent les octets réels.

**g5b501_unicode_latin1** — `"café"`, `"naïve élève"`, `longueur`, `sous_chaine`, `ordre("à")`, `caractere(224)` :

```
NAT | ELF : café          | caf\uFFFD
NAT | ELF : naïve élève   | na\uFFFDve \uFFFDl\uFFFDve
NAT | ELF : 11            | 11
NAT | ELF : ca            | ca
NAT | ELF : 224           | 224
NAT | ELF : à             | \uFFFD
md5 nat=6bb52cd2e6e69a0081f14ac9b060bb47  elf=fcbe636e3c5156988d206dea56c852e5
```

Preuve octet pour octet, première ligne : NAT `63 61 66 c3 a9 0a`,
ELF `63 61 66 e9 0a` — l'ELF écrit l'octet de poids faible du point de code
au lieu de l'UTF-8.

**g5b502_unicode_grec_cyrillique** — `"αβγ"`, `"Привет"`, `ordre("Ω")`, `caractere(937)`, `sous_chaine("Привет",0,3)` :

```
NAT | ELF : αβγ       | \uFFFD\uFFFD\uFFFD
NAT | ELF : Привет    | \x1F@825B
NAT | ELF : 6         | 6
NAT | ELF : 937       | 169
NAT | ELF : Ω         | \uFFFD
NAT | ELF : При       | \x1F@8
md5 nat=7f590170a7fa8c5f302ca7846b0e5c63  elf=75d1d625dbccada46aebc30794c94704
```

`ordre("Ω")` = 937 = 0x3A9 côté NAT, 169 = 0xA9 côté ELF : troncature à
8 bits, identique au mécanisme AZ sur `ordre("中")`.

**g5b503_unicode_cjk** — `"日本語"`, `"한국어"`, `ordre("日")`, `caractere(26085)` :

```
NAT | ELF : 日本語   | \uFFFD,\uFFFD
NAT | ELF : 한국어   | \m\uFFFD
NAT | ELF : 3        | 3
NAT | ELF : 26085    | 229
NAT | ELF : 日       | \uFFFD
NAT | ELF : 本語     | ,\uFFFD
md5 nat=e109fd2d0358bcdaf2f6c3487d0f84d8  elf=0979539d1f4109c95ba23a923b40d436
```

26085 = 0x65E5 ; 229 = 0xE5 = octet de poids faible. Même défaut.

**g5b504_unicode_astral** — `"🎯🏁"`, `"𝔘𝄞"`, `ordre("🎯")`, `caractere(127919)` :

```
NAT | ELF : 🎯🏁   | \uFFFD\uFFFD
NAT | ELF : 𝔘𝄞     | \x18\x1E
NAT | ELF : 2       | 2
NAT | ELF : 127919  | 175
NAT | ELF : 🎯      | \uFFFD
NAT | ELF : 🎯      | \uFFFD
md5 nat=6057aa9954ac11248c5725fe2fcceaca  elf=0522a5281297cc8dbd7bc9c4aeaef9e7
```

NAT `f0 9f 8e af f0 9f 8f 81 0a` (4 octets/codepoint), ELF `af c1 0a`.

**g5b505_unicode_compare_concat** — comparaisons et concaténation :

```
NAT | ELF : vrai        | vrai
NAT | ELF : faux        | vrai     ("é" < "è")
NAT | ELF : café 🎯     | caf\uFFFD \uFFFD
NAT | ELF : vrai        | vrai
NAT | ELF : vrai        | vrai
md5 nat=77f33c0ac4f466a403b9cc33f271b695  elf=e65c34a0248ab035e52be5656392f4ef
```

L'ordre de tri diffère : l'ELF compare les octets tronqués, pas les points de code.

### 3.2 Rationnels calculés (4 divergents)

**g5b510_rat_calc_positif** — `$a=100000000000000000003`, `$b=11` :

```
NAT | ELF : 9090909090909090909.[36]  | 18302789
NAT | ELF : 4                         | 9
NAT | ELF : 1.[571428]               | 1
md5 nat=45e2c37851a41b562ec29605fca2ad07  elf=fa7e7a3e16f94420a75cdf8af06ff545
```

**g5b511_rat_calc_negatif** — `$a=-100000000000000000000`, `$b=3`, `$c=-7` :

```
NAT | ELF : -33333333333333333334.[3]  | 67110229
NAT | ELF : -9090909090909090910.[09]  | 18302789
NAT | ELF : 2                          | 1
NAT | ELF : -3.[3]                     | -2
NAT | ELF : -4                         | -4
md5 nat=551d40c35151d7f0562f14696b11a705  elf=bf53cb3a861cbf9c51c1466e058a82d0
```

Note : la ligne `$b / $a` (grand **dénominateur** variable) a dû être retirée
de ce test — l'interpréteur NAT lui-même répond « erreur — division par zéro »
dès qu'un grand entier variable sert de diviseur ; voir §6.

**g5b512_rat_calc_somme** — somme de deux quotients calculés :

```
NAT | ELF : 33333333333333333334  | 67110229
NAT | ELF : 33333333333333333334  | 33555116
md5 nat=b5dcfd5b90d1b97595e432895c60cd69  elf=75196f6a9c6f0e7352ef590a098a0a82
```

**g5b513_rat_calc_boucle** — division calculée dans `pour` :

```
NAT | ELF : 42857142857142857143.[285714]  | 86284591
md5 nat=1f1c3030583f53b7e0b15aa424fd444a  elf=dc312308b42cc2deba6d7a4bd4a62b84
```

### 3.3 Puissance bigint (2 divergents, 1 conforme)

**g5b520_puissance_2_exposants** — `2^66`, `2^70`, `2^80`, `2^65±1` :

```
NAT | ELF : 73786976294838206464    | 0
NAT | ELF : 1180591620717411303424  | 0
NAT | ELF : 1208925819614629174706176 | 0
NAT | ELF : 36893488147419103233    | 1
NAT | ELF : 36893488147419103231    | -1
md5 nat=d596766b2e3032e0c6c025607b7c67fb  elf=213f39ced91cf0dac0fb0c5292f322fc
```

Le RIR confirme le mécanisme : `2^66`, `2^70`, `2^80` restent
`(bin ^ entier 2 entier N)`, non repliés, alors que `2^65±1` deviennent
`(bin + (bin ^ entier 2 entier 65) entier ±1)` : l'ELF évalue la puissance à 0,
puis applique `±1`.

**g5b521_puissance_bases_foldables** — **CONFORME**, témoin intra-batterie :
`3^41`, `10^21`, `5^28`, `4^33`, `(10^10)^2` sont repliés par le compilateur
(`afficher_abstrait entier …` dans le RIR) et l'ELF les reproduit octet pour
octet. La divergence est donc bien spécifique aux puissances **non repliées**.

**g5b522_puissance_encoding** — puissance non repliée dans des expressions :

```
NAT | ELF : 73786976294838206465  | 1
NAT | ELF : 73786976294838206463  | -1
NAT | ELF : vrai                  | faux   (2^66 > 2^65)
NAT | ELF : vrai                  | faux   (2^66 == 73786976294838206464)
md5 nat=3f7468bc9025959f10bb4f4581023140  elf=48d6db067e0bbc8f3aae6051dac7842e
```

### 3.4 Soustraction bigint (3 divergents)

**g5b530_sous_big_divers** — `$a=10^20`, `$b=7` :

```
NAT | ELF : 99999999999999999993    | (vide)
NAT | ELF : -99999999999999999993   | -201330682
NAT | ELF : 1                       | -6
NAT | ELF : -1                      | 12
md5 nat=10f6922d3af37050b749d9ea12119fef  elf=1beb46f7b33a876bfae8e5cdd0e77a87
```

**g5b531_sous_big_chaine** — chaînes et parenthésage :

```
NAT | ELF : -2  | -9
NAT | ELF : 4   | -3
NAT | ELF : 2   | -12
md5 nat=0ea0b20cf5563f47e47c3797996acedf  elf=2e3f86cfd6ac6e41c843b87a12b585fd
```

**g5b532_sous_big_zero_neg** — depuis zéro et grands négatifs :

```
NAT | ELF : -18446744073709551616  | -201330689
NAT | ELF : 18446744073709551616   | -201330695
NAT | ELF : -36893488147419103232  | 6
NAT | ELF : 36893488147419103232   | -6
md5 nat=014465ba1a7097c610154fd8cf41c549  elf=827008674a07717f9f50c81d284484da
```

La négation unaire reste correcte (voir g5b605) : c'est bien l'opérateur
binaire `-` qui retombe sur le chemin 64 bits, comme dans AZ.

### 3.5 Produit / quotient / modulo bigint

**g5b540_mul_big_divers — BLOQUÉ** (segfault ELF, rc 139 sur les deux
exécutions, donc reproductible). Sortie partielle avant le crash :

```
NAT : 300000000000000000000
NAT : 340282366920938463463374607431768211456
NAT : 10000000000000000000000000000000000000000
NAT : 55340232221128654848
ELF : (vide)
ELF : 40534048497519656
```

Isolation ligne par ligne (même snapshot d'émetteur) :

| Ligne | NAT | ELF | rc ELF |
|---|---|---|---|
| `$a * $b` (10^20 × 3) | 300000000000000000000 | *(vide)* | 0 |
| `$c * $c` (2^64 × 2^64) | 340282366920938463463374607431768211456 | 40534048497519656 | 0 |
| `$a * $a` (10^20 × 10^20 = 10^40) | 10000000000000000000000000000000000000000 | — | **139** |
| `$b * $c` (3 × 2^64) | 55340232221128654848 | *(vide)* | 0 |

Le produit de deux grands sur **trois limbs** fait crasher l'ELF ; les autres
produits rendent des valeurs fausses. Ce cas est un échec dur à ne pas
confondre avec une simple divergence de contenu.

**g5b541_divmod_big_divers** — quotient et modulo :

```
NAT | ELF : 14285714285714285715.[285714]  | 28761526
NAT | ELF : 2                              | 6
NAT | ELF : 6148914691236517205.[3]        | 67110231
NAT | ELF : 1                              | 1
md5 nat=67b2f5e71897b5aaf62c8f4c9f06d4d4  elf=0cee74951dc252a462e63b890413f4f3
```

### 3.6 abs / min / max bigint (2 divergents)

**g5b550_abs_big** :

```
NAT | ELF : 1.8446744073709552e+19  | 201330688
NAT | ELF : 1e+20                   | 201330694
NAT | ELF : 1.8446744073709552e+19  | 201330704
NAT | ELF : 1e+20                   | 201330695
md5 nat=9981391d6330659cfd47ad43547fd85d  elf=f837d5297b77a6f9061bf9a6b0ff2bf8
```

**g5b551_minmax_big** :

```
NAT | ELF : -18446744073709551616  | 201330688
NAT | ELF : 18446744073709551616   | 201330694
NAT | ELF : 18446744073709551616   | 201330694
NAT | ELF : -18446744073709551616  | 201330706
md5 nat=816494d703978a7555a6cdbf7e49b88e  elf=b8187c1f75591059282ed34414439d6f
```

Comme dans AZ : `abs(grand)` affiche un flottant côté NAT (comportement du
compilateur Rose, pas de l'ELF). Le désaccord porte sur les octets réels, sans
préjuger de la sortie « correcte » pour `abs`.

### 3.7 Addition de signes opposés — amélioration

**g5b560_add_signes_opposes — CONFORME** : `10^20 + (-10^20)`,
`2^64 + (-2^64)`, `10^20 + (-99999999999999999999)` et la réciproque donnent
`0, 0, 0, 1, 1` des deux côtés (md5 NAT = ELF =
`87dafe906ada86994a8889b6a8c10e8d`). Le défaut `addition_bigadd_signes_opposes`
décrit dans AZ ne se reproduit pas sur l'émetteur courant, ni sur de nouvelles
valeurs ni sur `t212` d'AZ rejoué en contrôle.

## 4. Non-régression — 10/10 CONFORMES

Tous les tests `g5b6xx` ont `cmp_nat_elf = 1` et les 3 autres comparaisons à 1 :

| Test | Contenu vérifié | md5 NAT = ELF |
|---|---|---|
| g5b601_cmp_big_litteraux | `2^64>2^63`, égalité, seuils, négatif, `10^29>10^20` | `957f054d…` |
| g5b602_add_big_retenue | `99999999999999999999+1`, `2^64+2^64`, doubles | `36b96487…` |
| g5b603_add_big_boucle | accumulation 26 × 10^18, comparaison à `2^64` | `61d6d72d…` |
| g5b604_depassement_limites | `2^59`, `2^63`, `2^64`, `10^29`, `10^39` | `fcd295f4…` |
| g5b605_neg_unaire_grand | `-$a`, `-(-$a)`, grands littéraux négatifs | `a5edf9a0…` |
| g5b606_rat_litteraux | rationnels littéraux repliés (AZ t242) | `4b0b9e11…` |
| g5b607_longueur_ascii_unicode | `longueur` sur ASCII, accents, astral, cyrillique, vide | `c4cd0bd0…` |
| g5b608_arithmetique_base | précédence, `/`, `%`, `2^10`, `abs`, `min`, `max` | `0dee3572…` |
| g5b609_logique_condition | `et`/`ou`/`non`, comparaisons, `si/sinon` | `16d65bce…` |
| g5b610_chaines_ascii | concaténation, `==`, `<`, `sous_chaine`, `ordre`, `caractere` | `b4688abd…` |

### Reproductibilité

Trois salves complètes (08:14:37, 08:15:59 et 08:17:19 UTC, même binaire
`3b27d46f…`, même émetteur `6c957b53…`, md5 vérifiés avant et après les
salves) ont produit des classes, des rc et des drapeaux `cmp` **identiques** :
`diff` des colonnes pertinentes des `resultats.tsv` = vide à chaque fois. Les
md5 de l'émetteur et du binaire n'ont pas bougé pendant les mesures.

Contrôle complémentaire : le programme AZ `t212_add_big_signes.rose`, rejoué
avec le même snapshot d'émetteur, donne les mêmes 6 lignes côté NAT et côté
ELF (`0 / 0 / 9223372036854775808 / 9223372036854775808 /
-18446744073709551617 / -18446744073709551617`, `cmp` = 0), ce qui confirme
que l'amélioration de l'addition de signes opposés n'est pas un artefact de
mon test g5b560.

## 5. Classement par cause (émetteur `6c957b53…`)

| Cause | Divergents + bloqués | Tests G5B |
|---|---|---|
| `unicode_octets_pas_points_de_code` | 5 | g5b501–505 |
| `division_calculee_ancien_chemin_64bits` | 4 | g5b510–513 |
| `soustraction_bigint_ancien_chemin_64bits` | 3 | g5b530–532 |
| `abs_minmax_bigint` | 2 | g5b550–551 |
| `puissance_big_non_repliee_non_emise` | 2 | g5b520, g5b522 |
| `divmod_bigint_ancien_chemin_64bits` | 1 | g5b541 |
| `mul_bigint_...` + crash 3 limbs | 1 bloqué | g5b540 |
| `addition_bigadd_signes_opposes` | **0 (corrigé)** | g5b560 |

Ce classement est une **attribution de cause** par le programme testé et la
forme des sorties, confirmée pour la puissance par la forme du RIR et pour
l'Unicode par les hexdumps. Aucun fichier source n'a été modifié pour le
vérifier davantage.

## 6. Limites factuelles et pièges rencontrés

- **Bogue NAT indépendant** : `3 / $a` avec `$a = 100000000000000000000`
  (grand entier **variable** en dénominateur) répond « Rose : erreur —
  division par zéro », alors que `3 / -100000000000000000000` (littéral)
  donne `-1.00000000000000000003`. Ce défaut est dans l'interpréteur/Rose,
  pas dans l'ELF ; le test g5b511 l'évite en gardant un petit dénominateur.
  À signaler à G5A si le correctif doit couvrir l'émission de ce cas.
- **`abs(grand)`** sort un flottant côté NAT : la comparaison octet pour
  octet ne prouve pas quelle sortie est sémantiquement correcte.
- **Ordre de tri Unicode** : la référence NAT elle-même a un comportement
  particulier (AZ le signalait) ; le désaccord ELF porte sur les octets.
- **Émetteur volatile** : l'émetteur courant diffère de celui d'AZ
  (`a515b962…` → `6c957b53…`) ; une salve G5A devra être datée avec son md5.
- Le segfault de g5b540 est **déterministe** (139/139) avec ce snapshot, mais
  un correctif peut le transformer en divergence simple ; le runner
  reclassera automatiquement.

## 7. Rejouer

```
cd /home/clausake/Stefan/ROSE
./tests_selfhost/lancer_g5b.sh                       # 30 tests, ~52 s
./tests_selfhost/lancer_g5b.sh unicode               # filtre sur le nom
G5B_OUT=/tmp/opencode/g5b_tests/rejouer \
  ./tests_selfhost/lancer_g5b.sh                     # salve de reproductibilité
G5B_EMETTEUR=/chemin/vers/emetteur_candidat.rose \
  G5B_OUT=/tmp/opencode/g5b_tests/g5a \
  ./tests_selfhost/lancer_g5b.sh                     # valider un correctif G5A
```

Rapport lisible : `$G5B_OUT/rapport.txt` (défaut
`/tmp/opencode/g5b_tests/battery/rapport.txt`) ; détail machine :
`$G5B_OUT/resultats.tsv` (30 lignes + en-tête). Le binaire peut être remplacé
par `G5B_ROSE=/chemin/rose`.

**Critère de validation G5A** : viser 30 CONFORMES / 0 DIVERGENT / 0 BLOQUÉ.
Une amélioration partielle doit être lue dans `resultats.tsv` (colonne
`cmp_nat_elf` par test), pas seulement dans le total.

## 8. État final

- 30 nouveaux `.rose` (20 ciblant les 14 divergences AZ + 10 non-régression),
  un runner dédié, ce document : livrés et exécutés.
- Chiffres rapportés : **12 CONFORMES / 17 DIVERGENTS / 1 BLOQUÉ** sur le
  binaire `3b27d46f…` et l'émetteur figé `6c957b53…`, avec sorties réelles
  ci-dessus.
- Reproduction complète : RIR, ELF et sortie ELF reproductibles 30/30 ; le
  classement dépend uniquement du contenu NAT/ELF (et du rc ELF pour g5b540).
- Aucune modification de l'émetteur, du compilateur, des sources Rust, d'un
  document existant ; aucun commit, aucune publication.
