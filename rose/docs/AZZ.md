# AZZ — grands entiers dans le runtime G5 (constat mesuré, 20/11/2026)

Sous-agent `az3`. Dépôt `/home/clausake/Stefan/ROSE` (`Stefan/ROSE`).
Deux fichiers possédés/écrits par ce chantier : `compilateur/emetteur_elf.rose`
et ce document. **Aucun commit, aucune publication.** Les autres fichiers
modifiés visibles dans `git status` (`rose/src/velopack.rs`,
`thorn/src/algebre.rose`) appartiennent à d'autres agents : non touchés.

Artefacts de mesure : `/tmp/opencode/bigint/` (tests, ELF, désassemblages,
batterie, logs F1/F2).

---

## 1. Problème de départ (reproduit)

`G5_AZ.md` §5.1 documente le blocage : un entier G5 est `n<<3` dans un mot
64 bits (≈61 bits utiles), donc `>> 18446744073709551616` (2^64) rend `0`
dans l'ELF alors que le natif rend la valeur exacte.

Mesure initiale rejouée le 20/11 (avant modification, émetteur md5
`7742556ae483aee5e02a1458ce326c83`) :

| | stdout | rc |
|---|---|---|
| natif `ROSE_SANS_JIT=1 ./rose/rose run t_big.rose` | `18446744073709551616` | 0 |
| ELF émis par l'émetteur G5 | `0` | 0 |

---

## 2. Representation retenue

Tag **3** (les tags 0=entier/booléen, 1=chaîne, 2=liste étaient seuls utilisés).
Descripteur 8-octets-aligné, pointeur tagué `orr #1` puis `orr #2` :

```
+0  tag = 3
+8  signe (0 = positif, 1 = négatif)
+16 nlimbs
+24 limb[0]   (base 10^9, 0..999999999, stocké sur 8 octets)
+32 limb[1]
...            (limb[0] = poids faible)
```

La base 10^9 évite toute division 128 bits : l'addition et la conversion
décimale restent des opérations 64 bits natives (`udiv`/`msub`, base 10 et
base 10^9). Le seuil de promotion est ±2^59 (et non 2^60) : au-delà, `n<<3`
déborde le mot signé ; 2^59 reste comparable sous l'émetteur auto-exécuté.

Routines émises dans l'ELF :

- `g5_emit_bigpromote` — petit entier tagué → descripteur tag 3 (identité si
  déjà grand). Scratch 0x60005000.
- `g5_emit_bigadd` — addition **signée** (même signe → addition des
  magnitudes avec retenue ; signes opposés → comparaison puis soustraction
  avec emprunt et élagage des limbs nuls de tête ; zéro → signe 0).
- `g5_load_val` — au-delà du seuil, construit le descripteur à l'émission
  (décomposition décimale par le natif Rose, qui a les entiers exacts).
- `g5_emit_print` — branche tag 3 : copie des limbs, 9 chiffres par limb
  dans le tampon décroissant 0x60006000, retrait des zéros de tête, signe,
  puis `write(1,…)` et le saut de ligne habituel.
- `g5_eval` `bin +` — test des tags 0/3, promotion de l'autre opérande,
  appel de `g5_emit_bigadd`. Le chemin chaîne et le chemin entier court
  restent inchangés (un saut explicite contourne le bloc big au retour de
  la concaténation). Les sauts locaux utilisent `g5_branch_off` (déplacement
  constant 17/14/17/12) et **non** `g5_next()` : 1244 `+` dans le RIR
  auto-hébergé × 2 labels auraient ajouté 2488 labels à `g5_patch`
  (recherche linéaire) et rendu l'émission de F1 prohibitive (> 600 s mesuré
  avant correction). Après correction, aucun label par `+`.

---

## 3. Résultats réels (natif vs ELF, octet pour octet)

Programme `/tmp/opencode/bigint/t_big.rose` = `>> 18446744073709551616` :

| | stdout | rc |
|---|---|---|
| natif | `18446744073709551616` | 0 |
| ELF **après** | `18446744073709551616` | 0 |

`cmp` natif/ELF = 0, stderr vides des deux côtés.

Autres programmes comparés (tous `cmp`=0, rc égaux) :

| fichier | contenu vérifié |
|---|---|
| `t_all.rose` | 2^64, décimal géant 10^29, 2^64 négatif, additions repliées à l'émission |
| `t_slot.rose` | grands littéraux stockés dans des slots (`>> $a`, `>> $b`) |
| `t_var.rose` | 9 additions **à l'exécution** : big+petit, big+big, big+(-big), petit+big, négatif+1, décimal géant+… |
| `t_mix.rose` | signes opposés : `-10^18+1`, `10^18+(-1)`, `-10^18+2`, `-10^18+10^18` |
| `t_small.rose` | régression `+`-entier et concaténation de chaînes inchangées |
| `t_edge.rose` | emprunt multi-limbes, deux grands négatifs, retenue multi-limbes, valeur à 6 limbs, résultat nul |

Sortie de `t_var.rose` (identique des deux côtés) :
```
18446744073709551617
36893488147419103232
0
-18446744073709551615
18446744073709551617
36893488147419103233
246913578024691357802469135780
123456789030792422974944119506
```

### Défauts trouvés et corrigés par désassemblage (`objdump`, `gdb`)

1. `movz` utilisé directement avec la constante 32 bits 10^9 → instruction
   invalide (`saddwb z2.d,…` lu dans l'ELF) ; remplacé par `g5_load` qui
   décompose en chunks 16 bits (`movz`+`movk`).
2. Trois branches inversées (`b.lt` au lieu de `b.ge`) dans les tests
   `i >= nA` / `i >= nB` : les limbs étaient zéro aux mauvais index.
3. Ordre d'écriture des limbs inversé (petits limbs en tête) et zéros de
   tête non retirés.
4. Off-by-one dans la recherche du limb de poids fort de la soustraction
   (`n` pas `n-1` pas depuis `&limb[0]`) : le limb de tête était élagué une
   fois de trop (`-10^18+1` rendait `-999999999`).
5. Comparaison des magnitudes de signes opposés parcourue du limb **faible**
   au fort : `2^64 + (-2^63)` donnait `-999999990776627963145224192` au lieu
   de `9223372036854775808` (709551616 < 854775808 au limb 0, alors que
   c'est le limb 2 qui tranche : 18 > 9). Corrigé en comparant à partir du
   limb de poids fort (labels +33/+34, réservation portée à 35). Deux pièges
   supplémentaires dans cette boucle : le compteur de recherche (`x10`)
   écrasait le signe de B, puis le limb de B (`x10`) l'écrasait aussi —
   d'où `-2^63 + 2^64 = -2^63`. Le compteur utilise `x6`, le limb B `x7`.

Preuves : `objdump -D -b binary -m aarch64 --adjust-vma=0x400000 <elf>` et
points d'arrêt gdb sur l'entrée de `bigadd` (`x8/x10/x11/x12` = signes et
effectifs) puis sur le retour (`x14` descripteur, `x13` nlimbs).

---

## 4. Batterie SH9 (80 programmes, `tests_selfhost/`)

Exécutée le 20/11 avec l'émetteur modifié (`/tmp/opencode/bigint/battery_new/`).
Comparaison natif (`ROSE_SANS_JIT=1`) vs ELF, octet pour octet.

| | avant (G5_AZ.md §4) | après |
|---|---|---|
| CONFORMES | 66 | **67** |
| DIVERGENTS | 7 | 4 (+1 émission en échec, voir ci-dessous) |
| BLOQUÉS | 7 | 7 |

Corrigés : **t32** (`2^62`), **t33** (`2^64`), **t34** (décimal géant) —
tous trois DIVERGENT → CONFORME. Le compilateur Rose replie `2^64` en
littéral `entier 18446744073709551616` dans le RIR, donc aucun opérateur
`^` n'est à émettre pour ce cas.

Divergences restantes, **inchangées** : t04/t41/t44 (Unicode) et t08
(rationnel calculé). `t35_factorielle` reste bloqué à l'émission
(`G5 : forme parenthésée non couverte : fact`). `t81_argument` /
`t999_critique_g5` nécessitent un argument : classés « bloqués » par le
runner simplifié sans argument, mais rejoués correctement avec
`/tmp/opencode/sh9_agents/battery/critique_entree.txt` (voir §5).

### 4.1 Batterie AZ « nouveaux bigints » (31 programmes, fournie par l'orchestrateur)

`./tests_selfhost/lancer_az.sh` (artefacts `/tmp/opencode/az_tests/battery/`),
émetteur figé md5 `6c957b53…` : **18 CONFORMES / 13 DIVERGENTS / 0 BLOQUÉS**
(16/15 avant correction de l'ordre de comparaison, 17/14 après première
correction). RIR×2, ELF×2 et sorties×8 identiques dans 31/31 cas. Conformes :
6 comparaisons sur littéraux géants (repliées par le compilateur), 5
débordements de seuil, opérations mixtes dont **toutes les additions signées**
(t212/t232 corrigés), etc. Divergences = précisément les limites de la §6 :
soustraction, `*`/`/`/`%`, `-` unaire, puissance non repliée, rationnels
calculés, Unicode.

---

---

## 5. Jalons (état à la rédaction)

| Contrôle | Résultat mesuré |
|---|---|
| `./rose/rose verif compilateur/emetteur_elf.rose` | `vérifié`, rc=0 |
| `./rose/rose rir compilateur/emetteur.rose \| md5sum` | `b3ed9262dd54776a6dbd07bc12368d0e` (identique au départ) |
| `t999_critique_g5` (avec l'argument fourni) | **CONFORME**, 20/20 lignes, stdout+stderr octet pour octet, rc=0 des deux côtés |
| md5 émetteur après (état livré) | `6c957b53e97f3ebca94a26c9a272cf54` (départ `7742556a…`) |
| F1 vs F2 (auto-régénération, état livré) | **IDENTIQUES** : 674 672 o, md5 `cb6b049f…`, `cmp`=0 (§8.7) |

Le point fixe documenté dans `G5_AZ.md` (dump de `emetteur.rose` par l'ELF
auto-émis = RIR natif `b3ed9262…`) n'est **pas** garanti par ce chantier :
les routines supplémentaires changent le dump réflexif de l'émetteur. Le
jalon demandé ici porte sur le RIR natif de `emetteur.rose` (inchangé) et
sur F1==F2, pas sur ce dump. À re-vérifier avant toute livraison.

---

## 6. Limites ABI — NON livrées (pas de succès simulé)

- `entier("18446744073709551616")` **à l'exécution** reste faux :
  `g5_emit_entier` accumule en 64 bits puis `<<3`. Le littéral décimal
  source est corrigé à l'émission (le natif Rose calcule les limbs), mais
  un programme auto-exécuté (F1) qui compilerait un texte contenant un
  géant devrait d'abord disposer d'un `entier()` multi-limbes. Non tenté.
- `chaine(grand_entier)` n'est pas converti (seul `>>` l'est).
- ~~Comparaison~~ **implémentée en §8** (les 6 opérateurs sur descripteurs tag
  3). Restent sur l'ancien chemin 64 bits : soustraction (`-`), multiplication
  (`*`), division (`/`), modulo et puissance sur des grands entiers
  **variables**. L'addition signée et la négation copie sont les seules
  opérations arithmétiques nouvelles.
- Le scratch de promotion/conversion est une zone fixe (0x60005000 /
  0x60006000). Un nombre de limbs dépassant la place disponible écraserait
  une zone voisine ; les tests vont jusqu'à 10 limbs.
- Aucun test de concurrence entre les(appels) imbriqués de bigadd et
  l'impression n'a été fait au-delà des programmes ci-dessus.

---

## 7. F1 / F2

### 7.1 Première salve (RIR `e.rir`, avant optimisation des sauts)

`F1.elf` émis par le natif en **14 min 56 s**, rc=0, 604 092 o, md5
`67283c07b3816de036705d7c8bc4b018`, stderr vide. Le RIR ayant changé
ensuite (180 o de moins), cette salve ne valide que la cohérence interne.

### 7.2 Blocage trouvé : tas 32 Gio insuffisant

`./F1.elf e.rir` (F1 s'auto-compilant) rendait **rc=139 SIGSEGV** en 15 s.
Diagnostic gdb : pointeur libre à `0x860000000`, exactement la fin du
mapping RW de 32 Gio (identifié dans le header ELF, `p_memsz=0x800000000`).
Preuve que ce n'est pas un bug de sémantique mais une limite de taille :
- le **F1 de référence** `/tmp/opencode/sh12_agents/F1_final.elf` (avant ce
  chantier) réussit sur son propre RIR, avec pointeur final `0x715b1cf18`
  = 28,34 Gio ;
- un patch du seul champ `p_memsz` de F1.elf à 64 Gio repousse le SIGSEGV à
  `0x105ffffff8`, puis à 128 Gio il **réussit** : pointeur final
  `0x1296320880` ≈ 79,7 Gio, rc=0, 604 092 o ;
- le ELF produit par F1 patché est **octet pour octet identique à F1.elf**
  (`cmp`=0, md5 `67283c07…` des deux côtés) : F1==F2 est donc vrai dès que
  le tas est assez grand.

Conséquence : le mapping RW de `g5_elf()` passe de 32 Gio (`34359738368`) à
**128 Gio** (`137438953472`), toujours virtuel/paresseux. Le pointeur à
~80 Gio reste sous les ~110 Gio disponibles mesurés par `free -h`.

### 7.3 Salve intermédiaire (mapping 128 Gio, avant comparaison des limbs) — RÉUSSIE

```
./rose/rose rir compilateur/emetteur_elf.rose > e_v2.rir        # 348 822 o, b776a875…
./rose/rose compilateur/emetteur_elf.rose e_v2.rir > F1_v2.elf  # 14 min 57 s, rc=0
./F1_v2.elf e_v2.rir > F2_v2.elf                                # 22,8 s, rc=0
cmp F1_v2.elf F2_v2.elf                                         # rc=0
```
F1 = F2 : **604 060 octets**, md5 `021bcdb0c437335c38e722443892d794`
des deux côtés, stderr vides, mapping RW 128 Gio identique. Jalons verts.

---

## 8. Comparaison des grands entiers (vague 69, 20/11/2026)

Sous-agent `cmp-az`. Deux fichiers possédés : `compilateur/emetteur_elf.rose`
et ce document. Aucun commit, aucune publication. Artefacts :
`/tmp/opencode/bigint/` (tests, RIR, ELF, sorties, log F1/F2).

### 8.1 Défaut reproduit

`100000000000000000000 < 18446744073709551616` rendait **vrai** en ELF alors
que le natif rend **faux** : `g5_eval` comparait les pointeurs tagués
(`cmpr x1,x0`), donc l'ordre des adresses d'allocation des descripteurs, pas
les valeurs. Le dispatch `< > <= >=` passait par la même voie 3-instructions
que les petits entiers.

### 8.2 Routines ajoutées

- `g5_emit_bigneg($lbl)` — négation. Petit entier : `0-x`. Tag 3 : ALLOUE une
  copie du descripteur `[3][signe][nlimbs][limbs…]` avec le signe inversé
  (XOR 1) ; l'original n'est jamais muté (test d'alias §8.4). `x0/x1/x2/x9`
  sont clobberés par `g5_alloc_inline` : le descripteur source, `nlimbs` et le
  signe sont préservés dans `x6/x7/x8` avant l'allocation. 4 labels (+1..+3).
  Branché sur les formes RIR `(neg …)` et `(uni neg …)` : `-$a` avec `$a`
  grand fonctionne désormais.
- `g5_emit_bigcmp($lbl)` — compare deux valeurs taguées. Les deux opérandes
  sont promus par `bigpromote` (chacun alloue son propre descripteur sur le
  tas). Comparaison du **signe** d'abord (0=positif, 1=négatif : un signe plus
  petit signifie une valeur PLUS GRANDE), puis des **magnitudes des limbs du
  poids fort `n-1` vers le faible `0`** — un limb absent (`i >= nlimbs`) est
  traité comme zéro. Si les deux signes sont négatifs, le signe du résultat
  est inversé. Sortie : `x0 = -1 / 0 / +1` (brut, non tagué) = signe de
  (gauche − droite). L'appelant applique alors `cmpi x0,#0 ; cset x0,<cond> ;
  lsl x0,#3`. 13 labels (+1..+12).
  Pièges corrigés pendant l'implémentation (preuves par `objdump`/`gdb`) :
  1. `bigcmp` est une routine-feuille appelée par `BL` et fait elle-même deux
     `BL` imbriqués vers `bigpromote` : `x30` (LR) doit être sauvegardé sur la
     PILE (`sub sp,#16 ; str x30,[sp]`), pas dans `x1` (que
     `g5_alloc_inline` clobbre) ;
  2. `x15` sert d'INDEX de limb (`n-1 .. 0`, comparé à `nlimbs`), le
     déplacement en octets `8*(n-1)` va dans `x2` : les confondre faisait
     lire des limbs hors descripteur et rendait 0 pour tout.

### 8.3 Dispatch des 6 opérateurs, sans label par expression

`g5_emit_cmp_value($c)` (nouvelle fonction d'émission, appelée depuis `g5_eval`
pour `== != < > <= >=`) teste le tag 3 par **déplacement constant**
(`g5_branch_off`, exactement comme le chemin `+` de la vague précédente) :
- les comparaisons d'entiers courantes (1491 dans le RIR auto-hébergé) gardent
  le chemin d'origine `cmpr/cset/lsl`, aucun label ni appel ajouté ;
- le chemin grand entier fait un seul `BL` vers le label FIXE `$g5_bigcmpl`.
Aucun `g5_next()` par expression de comparaison. La branche `==`/`!=`
(pré-triée pour l'égalité de chaînes) enjambe désormais 14 instructions au
lieu de 3 (`g5_branch_off(0, 15)`).

### 8.4 Résultats réels (natif vs ELF, `cmp` octet pour octet)

| fichier | contenu | résultat |
|---|---|---|
| `t_ops.rose` | **6/6 opérateurs** `< > == != <= >=` sur grands entiers | cmp=0, 6 lignes |
| `t_mix10.rose` | **10 cas mixtes** (big/big, big/petit, petit/big, négatifs, `+$a+1`, `-$a`) | cmp=0, 10 lignes |
| `t_wide.rose` | 40 cas (poids 1→3 limbs, zéro, signes, runtime add) | cmp=0, 40 lignes |
| `t_deep.rose` | 12 cas à 5–6 limbs (10^49), signes opposés | cmp=0, 12 lignes |
| `t_alias.rose` | `-$a` ne mute pas `$a` ; `$a + (-$a) == 0` | cmp=0, 5 lignes |
| `t_reg.rose` | régression `==/!=` chaîne, petits entiers, `+`, `%`, `-5<0` | cmp=0, 15 lignes |

Tous : `stderr` vides des deux côtés, rc égaux. Exemple (`t_mix10`, identique
des deux côtés) : `vrai vrai vrai vrai faux vrai vrai vrai vrai vrai`.

### 8.5 Jalons

| Contrôle | Résultat |
|---|---|
| `./rose/rose verif compilateur/emetteur_elf.rose` | `vérifié`, rc=0 |
| point fixe `b3ed9262` (RIR natif `emetteur.rose`, ×2) | identique, `b3ed9262dd54776a6dbd07bc12368d0e` |
| F1 == F2 (auto-régénération, md5 identiques) | **RÉUSSI** — 674 672 o, `cb6b049f…` (F2==F3 aussi), voir §8.6 |
| md5 émetteur source | `6c957b53e97f3ebca94a26c9a272cf54` (départ `4e6a1923…`) |

### 8.6 F1 / F2 — RÉUSSI (salve FINALE du 20/11, log `/tmp/opencode/bigint/f1f2_v2.log`)

```
./rose/rose rir compilateur/emetteur_elf.rose > e_cur.rir        # 357 567 o, 27518aa6…
./rose/rose compilateur/emetteur_elf.rose e_cur.rir > F1_cur.elf # 17 min 01 s, rc=0
./F1_cur.elf e_cur.rir > F2_cur.elf                              # 23 s, rc=0
cmp F1_cur.elf F2_cur.elf                                        # rc=0
./F2_cur.elf e_cur.rir > F3_cur.elf                              # 23 s, rc=0 (2e salve)
cmp F2_cur.elf F3_cur.elf                                        # rc=0
```

F1 = F2 = F3 : **674 672 octets**, md5 `cb6b049fdf6e5183792a9e76267ac1ec`
des trois côtés, stderr vides, mapping RW 128 Gio inchangé. La croissance de
604 060 → 674 672 octets correspond aux ~16 400 instructions supplémentaires
(14 au lieu de 3 par comparaison × 1491), exactement reproduites par
l'auto-compilation : le point fixe est préservé.

Note de chronologie : une première salve (md5 `db321542…`, 673 700 o) avait
été jouée sur l'émetteur tel qu'au début du chantier. L'orchestrateur a
ensuite corrigé `bigadd` dans le MÊME fichier possédé (comparaison par limb de
poids fort, préservation du signe de B dans x10 — voir le diff `git diff` sur
`g5_emit_bigadd`) ; la salve finale ci-dessus a été rejouée sur la version
STABLE `6c957b53…` (md5 relevé au début ET à la fin de la salve, identique).
Les 9 programmes de §8.4 ont tous été rejoués sur cette version finale :
`cmp`=0 partout. Tous les jalons sont verts.

### 8.7 Salve finale — état livré (source md5 `6c957b53…`)

Les §7.3 et §8.6 ont été mesurés sur des états antérieurs (avant correction
de l'ordre de comparaison des limbs et préservation du signe de B). L'état
livré a été rejoué intégralement :
```
./rose/rose rir compilateur/emetteur_elf.rose > e_v4.rir          # 357 567 o, 27518aa6…
./rose/rose compilateur/emetteur_elf.rose e_v4.rir > F1_v4.elf    # 17 min 02 s, rc=0
./F1_v4.elf e_v4.rir > F2_v4.elf                                  # 23,3 s, rc=0
cmp F1_v4.elf F2_v4.elf                                           # rc=0
```
F1 = F2 : **674 672 octets**, md5 `cb6b049fdf6e5183792a9e76267ac1ec`
des deux côtés, stderr vides. C'est le point de contrôle de l'état livré.
Jalons re-vérifiés sur cet état : `verif` = `vérifié`, RIR natif de
`emetteur.rose` = `b3ed9262…`, t999 = 20/20 conforme, AZ = 18/31 conformes,
`>> 18446744073709551616` natif = ELF.
