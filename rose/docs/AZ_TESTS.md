# AZ_TESTS — extension de la batterie self-host aux nouveaux bigints

Constat mesuré, **20/11/2026 22:10 UTC**. Sous-agent `az-tests`. Dépôt
`/home/clausake/Stefan/ROSE` (`Stefan/ROSE`). Fichiers livrés, tous dans les
chemins possédés : **31 programmes `tests_selfhost/t2*.rose`**,
**`tests_selfhost/lancer_az.sh`**, ce document. **Aucune modification de
`rose/src/**`, `compilateur/**` ou d'un autre fichier existant ; aucun commit,
aucune publication.**

## 1. Protocole

`tests_selfhost/lancer_az.sh` rejoue, pour chaque `tests_selfhost/t2[0-9][0-9]_*.rose` :

```
ROSE_SANS_JIT=1 rose p.rose        > p.nat       # référence sémantique
rose rir p.rose                    > p.rir       # dump RIR, 2 fois
rose <émetteur_figé> p.rir         > p.elf       # ELF émis, 2 fois
chmod +x p.elf && ./p.elf          > p.elfout    # sortie ELF, 2 fois
```

L'émetteur est **copié avant la salve** (`AZ_EMETTEUR`, défaut
`compilateur/emetteur_elf.rose`), parce qu'un autre agent le modifie pendant
les mesures. Comparaisons **octet pour octet** (`cmp`) : `nat/ELF`, `RIR1/RIR2`,
`ELF1/ELF2`, `sortie1/sortie2`. Timeouts 25 s (interprétation, dump, exécution
ELF) et 90 s (émission). Classes :

- **CONFORME** — 4 rc = 0 et 4 comparaisons égales ;
- **DIVERGENT** — 4 rc = 0 mais au moins une comparaison échoue ;
- **BLOQUÉ** — rc ≠ 0, timeout, artefact manquant ou sortie non reproductible.

Le runner n'omet aucun rc : `resultats.tsv` contient une ligne par programme
avec les 4 rc, les 8 md5 et les 4 résultats de `cmp` ; `rapport.txt` imprime
chaque blocage et les sorties NAT/ELF des divergences.

## 2. Chiffres réels

Salve de référence (état courant à 22:10:04 UTC) : **31 programmes,
17 CONFORMES, 14 DIVERGENTS, 0 BLOQUÉ**.

| Mesure | Valeur |
|---|---|
| Émetteur figé | `compilateur/emetteur_elf.rose`, md5 `a515b9626eaee92489639f8c72db7c20` |
| Binaire Rose | `rose/rose`, md5 `12eac46007cf0fe9e1aec2dff6fbf665` |
| Programmes | 31 |
| CONFORMES | **17** |
| DIVERGENTS | **14** |
| BLOQUÉS | **0** |
| Comparaisons NAT/ELF égales | 17/31 |
| RIR reproductible octet pour octet | 31/31 |
| ELF reproductible octet pour octet | 31/31 |
| Sortie ELF reproductible octet pour octet | 31/31 |
| Durée de la salve | 52 s |
| Artefacts | `/tmp/opencode/az_tests/courant/{rapport.txt,resultats.tsv,*.nat,*.rir,*.elf,*.elfout}` |

Par catégorie (salve de référence) :

| Catégorie | Tests | Conformes | Divergents | Bloqués |
|---|---|---|---|---|
| Comparaisons bigint (`t200`–`t205`) | 6 | 6 | 0 | 0 |
| Opérations mixtes (`t210`–`t215`) | 6 | 3 | 3 | 0 |
| Débordements (`t220`–`t226`) | 7 | 5 | 2 | 0 |
| Négatifs (`t230`–`t233`) | 4 | 2 | 2 | 0 |
| Rationnels calculés (`t240`–`t243`) | 4 | 1 | 3 | 0 |
| Unicode (`t250`–`t253`) | 4 | 0 | 4 | 0 |
| **Total** | **31** | **17** | **14** | **0** |

### 2.1 Reproductibilité et volatilité de l'émetteur

L'émetteur a été modifié par d'autres agents **pendant** ce chantier. Chaque
salve est donc datée et liée à un md5 ; ne pas comparer ces chiffres sans
re-mesurer. Séquence réellement observée :

| UTC | md5 émetteur | Tests | C | D | B |
|---|---|---|---|---|---|
| 21:57:15 | `4e6a1923…` | 30 | 9 | 21 | 0 |
| 21:58:41 | `4e6a1923…` | 31 | 8 | 23 | 0 |
| 21:59:47 | `c1c65340…` | 31 | 8 | 23 | 0 |
| 22:00:37 | `48527587…` | 31 | 8 | 23 | 0 |
| 22:02:51 | `cc673f86…` | 31 | 8 | 15 | **8** (bus errors/segfaults ELF sur les comparaisons) |
| 22:03:43 | `b225747f…` | 31 | 16 | 15 | 0 |
| 22:07:03 / 22:07:56 / 22:09:05 | `b225747f…` (snapshot épinglé) | 31 | 16 | 15 | 0 |
| **22:10:04** | **`a515b962…`** | **31** | **17** | **14** | **0** |

Trois salves consécutives sur le snapshot épinglé `b225747f…` ont produit des
**classes identiques et des sorties NAT/ELF octet pour octet identiques**
(31/31). Les deux salves `a515b962…` et `b225747f…` ne diffèrent que par
`t232_neg_add_mixte`, corrigé entre les deux (DIVERGENT → CONFORME). Le md5 de
l'émetteur était encore `a515b962…` à 22:10:10 UTC, à la rédaction.

## 3. Divergences restantes (entrée / sortie réelles)

Toutes ont rc = 0 des deux côtés ; seul le contenu des octets diffère.
`NAT` = `ROSE_SANS_JIT=1 rose p.rose`, `ELF` = binaire émis exécuté.

### 3.1 Addition bigint — 1

**`t212_add_big_signes`** — entrée : `$p = 18446744073709551616`,
`$n = -18446744073709551616`, puis
`>> (-9223372036854775808) + $p`.

```
NAT : 9223372036854775808
ELF : -9223372036854775808
```

Les lignes `$p+$n`, `$n+$p`, `$n+(-1)`, `(-1)+$n` sont conformes ; seul le cas
« grand positif + grand négatif de même effectif de limbs » garde un signerr
sur l'ELF courant. Avec le snapshot précédent `b225747f…`, la même ligne
donnait `-999999990776627963145224192` : le défaut a changé de forme, il n'a
pas disparu.

### 3.2 Soustraction bigint — 2

**`t214_moins_big`** — entrée : `$a = 18446744073709551617`, `$b = 1`.

| Entrée | NAT | ELF |
|---|---|---|
| `>> $a - $b` | `18446744073709551616` | `-` |
| `>> $b - $a` | `-18446744073709551616` | `-201330688` |
| `>> $a - 18446744073709551616` | `1` | `-6` |

`>> $a - $a` donne `0` des deux côtés.

**`t231_neg_unaires`** — entrée : `$a = 18446744073709551616`.

| Entrée | NAT | ELF |
|---|---|---|
| `>> -$a` | `-18446744073709551616` | `-18446744073709551616` (conforme) |
| `>> -(-$a)` | `18446744073709551616` | `18446744073709551616` (conforme) |
| `>> 0 - $a` | `-18446744073709551616` | `-201330689` |

La négation unaire est émise correctement ; c'est bien l'opérateur binaire `-`
qui retombe sur le chemin 64 bits.

### 3.3 Produit / quotient / modulo bigint — 1

**`t215_mul_div_mod_big`** — entrée : `$a = 18446744073709551616`, `$b = 2`.

| Entrée | NAT | ELF |
|---|---|---|
| `>> $a * $b` | `36893488147419103232` | `402661376` |
| `>> $a / $b` | `9223372036854775808` | `100665344` |
| `>> $a % $b` | `0` | `0` (conforme) |
| `>> $a * 0` | `0` | `0` (conforme) |

### 3.4 abs / min / max sur bigint — 1

**`t233_neg_abs_minmax`** — entrée : `$a = -18446744073709551616`,
`$b = 18446744073709551616`.

| Entrée | NAT | ELF |
|---|---|---|
| `>> abs($a)` | `1.8446744073709552e+19` | `201330688` |
| `>> min($a, $b)` | `-18446744073709551616` | `201330688` |
| `>> max($a, $b)` | `18446744073709551616` | `201330694` |
| `>> abs($a + 1)` | `1.8446744073709552e+19` | `201330704` |

Note d'honnêteté : `abs(grand)` affiche un flottant côté NAT (comportement du
compilateur Rose, pas de l'ELF). Le désaccord mesuré porte sur les octets réels,
sans préjuger de quelle sortie est « la bonne » pour `abs`.

### 3.5 Rationnels calculés — 3

**`t240_rat_big_calcul`** — `$a = 100000000000000000000`, `$b = 7` :

| Entrée | NAT | ELF |
|---|---|---|
| `>> $a / $b` | `14285714285714285714.[285714]` | `28761526` |
| `>> $a % $b` | `2` | `6` |

**`t241_rat_neg_calcul`** — `$a = -7`, `$b = 3` :

| Entrée | NAT | ELF |
|---|---|---|
| `>> $a / $b` | `-3.[3]` | `-2` |
| `>> $b / $a` | `-1.[428571]` | `0` |
| `>> $a % $b` | `2` | `2` (conforme) |
| `>> $b % $a` | `-4` | `-4` (conforme) |

**`t243_rat_somme_calcul`** — `$a=1`, `$b=3`, `$c=2`, `$d=3`, puis
`$x = 100000000000000000000 + 1`, `$y = 3` :

| Entrée | NAT | ELF |
|---|---|---|
| `>> $a/$b + $c/$d` | `1` | `0` |
| `>> $x / $y` | `33333333333333333333.[6]` | `67110229` |

Les rationnels **littéraux** (`t242` : `100000000000000000000/7`,
`18446744073709551616/3`, `-100000000000000000000/7`, `(10^20)/(10^19)`) sont
conformes : le compilateur les replie avant émission. Seuls les quotients
calculés à l'exécution divergent.

### 3.6 Puissance non repliée — 2

**`t222_depassement_2_64`** — ligne `>> 2^65` : NAT `36893488147419103232`,
ELF `0`. Les autres lignes (`2^64`, `2^64-1`, `2^64+1`, `2^64+2^64`) sont
conformes.

**`t226_puissance_big`** :

| Entrée | NAT | ELF |
|---|---|---|
| `>> 2^65` | `36893488147419103232` | `0` |
| `>> 2^100` | `1267650600228229401496703205376` | `0` |
| `>> (2^64)^2` | `340282366920938463463374607431768211456` | identique (conforme) |
| `>> (10^20)^2` | `10000000000000000000000000000000000000000` | identique (conforme) |
| `>> 18446744073709551616^2` | `340282366920938463463374607431768211456` | identique (conforme) |

Preuve du mécanisme : `rose rir` laisse `2^65` sous forme
`(bin ^ entier 2 entier 65)` (le repli natif ne se fait pas au-delà de 2^64),
alors que `(2^64)^2` est replié en `entier 340282366920938463463374607431768211456`.

### 3.7 Unicode indexé par octets — 4

**`t250_unicode_chaines`** :

| Entrée | NAT | ELF |
|---|---|---|
| `>> "été"` | `été` | `\xe9 t \xe9` (octets `e9 74 e9`, non UTF-8) |
| `>> "中文"` | `中文` | `-` + `\x87` (octets `2d 87`) |
| `>> "🚀🌕"` | `🚀🌕` | `\x80` + `\x15` (octets `80 15`) |
| `>> longueur("été")` | `3` | `3` (conforme) |
| `>> longueur("🚀")` | `1` | `1` (conforme) |

**`t251_unicode_sous`** — `$s = "été radieux"` :

| Entrée | NAT | ELF |
|---|---|---|
| `>> longueur($s)` | `11` | `11` (conforme) |
| `>> sous_chaine($s, 0, 3)` | `été` | `\xe9 t \xe9` |
| `>> sous_chaine($s, 4, 6)` | `ra` | `ra` (conforme) |
| `>> sous_chaine($s, 0, 1)` | `é` | `\xe9` |

**`t252_unicode_ordre_car`** :

| Entrée | NAT | ELF |
|---|---|---|
| `>> ordre("é")` | `233` | `233` (conforme) |
| `>> ordre("中")` | `20013` | `45` |
| `>> ordre("🚀")` | `128640` | `128` |
| `>> caractere(233)` | `é` | `\xe9` |
| `>> caractere(20013)` | `中` | `-` |
| `>> caractere(128640)` | `🚀` | `\x80` |

**`t253_unicode_comparaison`** :

| Entrée | NAT | ELF |
|---|---|---|
| `>> "é" == "é"` | `vrai` | `vrai` (conforme) |
| `>> "é" < "è"` | `faux` | `vrai` |
| `>> "été" + "🚀"` | `été🚀` | `\xe9 t \xe9 \x80` |
| `>> "中" != "文"` | `vrai` | `vrai` (conforme) |

Preuve octet pour octet (première ligne de `t250`) :
NAT `c3 a9 74 c3 a9 0a`, ELF `e9 74 e9 0a` — l'ELF écrit un octet par point
de code au lieu de l'UTF-8. md5 des sorties complètes :
NAT `666147dace67764d7531f54fa2548916`,
ELF `ae5de6eb5d4a213b9da75bc7bcbd7f2d`.

## 4. Classement par cause

| Cause (chemin de l'émetteur figé `a515b962…`) | Divergents | Tests |
|---|---|---|
| `unicode_octets_pas_points_de_code` — `g5_emit_print`/`g5_emit_stubs` écrivent l'octet de poids faible du point de code, `ordre`/`caractere` tronqués à 8 bits | **4** | t250, t251, t252, t253 |
| `division_calculee_ancien_chemin_64bits` — `>>` et `%` sur valeurs non littérales passent par `asr3`+`sdivr` 64 bits, sans gestion du tag 3 | **3** | t240, t241, t243 |
| `puissance_big_non_repliee_non_emise` — `bin ^` non replié reste émis comme une boucle `mulr` sur des entiers tagués | **2** | t222, t226 |
| `soustraction_bigint_ancien_chemin_64bits` — `g5_eval` émet `subr(0,1,0)` (l. 1114) sans branche tag 3 | **2** | t214, t231 |
| `addition_bigadd_signes_opposes` — `g5_emit_bigadd` (l. 2822) a un défaut de signe/effectif sur un couple de grands de signes opposés | **1** | t212 |
| `mul_div_mod_bigint_ancien_chemin_64bits` — `mulr` (l. 1115), `sdivr` (l. 1116), modulo 64 bits | **1** | t215 |
| `abs_minmax_bigint` — `g5_emit_abs`/`min`/`max` (l. 2658/2669/2680) font `asr3` puis `lsl3` sur le pointeur tagué | **1** | t233 |
| **Total** | **14** | |

Familles : encodage Unicode 4 ; arithmétique bigint hors addition 5 ;
rationnels calculés 3 ; puissance 2.

Ce classement est une **attribution de cause** : les étiquettes sont positionnées
par le programme testé (nom de fichier) et confirmées par la forme des sorties
et la lecture du chemin d'émission dans le snapshot. Aucun fichier source n'a
été modifié pour le vérifier davantage.

## 5. Ce qui est désormais CONFORME (régression non constatée)

Avec le snapshot `a515b962…`, les 6 tests de comparaison de grands entiers
(littéraux, variables, négatifs, calculés, seuil ±2^59, tests dans `si`) sont
**tous conformes** : la routine `g5_emit_cmp_value`/`g5_emit_bigcmp` ajoutée
entre les salves 21:59 et 22:10 a corrigé les 6 divergences mesurées au départ.
Sont aussi conformes : débordements 2^59 → 2^63, littéraux 2^64/10^29/10^39,
promotion à l'exécution (`t224`), accumulation en boucle au-delà de 2^64
(`t225` : `30000000000000000000` des deux côtés), additions multi-limbes et
retenues (`t210`, `t211`, `t213`), négation unaire, grands littéraux négatifs,
rationnels littéraux (`t242`), `longueur` Unicode.

## 6. Rejouer

```
cd /home/clausake/Stefan/ROSE
./tests_selfhost/lancer_az.sh                       # 31 tests, ~50 s
./tests_selfhost/lancer_az.sh cmp                   # filtre sur le nom
AZ_EMETTEUR=/tmp/opencode/az_tests/emetteur_b225747f.rose \
AZ_OUT=/tmp/opencode/az_tests/rejouer \
  ./tests_selfhost/lancer_az.sh                     # snapshot épinglé
```

Rapport lisible : `$AZ_OUT/rapport.txt` (défaut
`/tmp/opencode/az_tests/battery/rapport.txt`) ; détail machine :
`$AZ_OUT/resultats.tsv` (31 lignes + en-tête). Le script figé à `b225747f…` est
`/tmp/opencode/az_tests/emetteur_b225747f.rose` (md5 vérifié) ; il permet de
rejouer exactement la salve de reproductibilité. La batterie SH9 historique
(80 programmes inchangés) reste rejouable avec `tests_selfhost/lancer.sh`.

## 7. État final

- 31 nouveaux `.rose`, un runner dédié, ce document : livrés et exécutés.
- Chiffres rapportés : **17 CONFORMES / 14 DIVERGENTS / 0 BLOQUÉ** sur le
  snapshot émetteur `a515b962…` ; 16/15/0 sur le snapshot reproductible
  `b225747f…` (3 salves identiques octet pour octet).
- Aucune modification de l'émetteur, du compilateur, des sources Rust, d'un
  document existant ; aucun commit, aucune publication.
- Limite factuelle : l'émetteur continue de changer sous d'autres agents ;
  toute nouvelle salve doit être datée avec son md5. Ce document n'affirme pas
  que les 14 divergences sont des bugs du seul émetteur : `abs(grand)` et
  l'ordre de tri Unicode ont aussi des comportements natifs particuliers,
  explicitement signalés ci-dessus.
