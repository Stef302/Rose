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

---

## 5. Jalons (état à la rédaction)

| Contrôle | Résultat mesuré |
|---|---|
| `./rose/rose verif compilateur/emetteur_elf.rose` | `vérifié`, rc=0 |
| `./rose/rose rir compilateur/emetteur.rose \| md5sum` | `b3ed9262dd54776a6dbd07bc12368d0e` (identique au départ) |
| `t999_critique_g5` (avec l'argument fourni) | **CONFORME**, 20/20 lignes, stdout+stderr octet pour octet, rc=0 des deux côtés |
| md5 émetteur après | `0c37cbc5489b8f3cbd003f6c489ffd52` (départ `7742556a…`) |
| F1 vs F2 (auto-régénération) | voir §7 — **en cours / à re-mesurer** |

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
- Comparaison, soustraction (`-`), multiplication (`*`), division (`/`),
  modulo et puissance sur des grands entiers **variables** restent sur
  l'ancien chemin 64 bits. L'addition signée est la seule opération
  arithmétique nouvelle.
- Le scratch de promotion/conversion est une zone fixe (0x60005000 /
  0x60006000). Un nombre de limbs dépassant la place disponible écraserait
  une zone voisine ; les tests vont jusqu'à 10 limbs.
- Aucun test de concurrence entre les(appels) imbriqués de bigadd et
  l'impression n'a été fait au-delà des programmes ci-dessus.

---

## 7. F1 / F2

Chaîne rejouée sur l'état final :
```
./rose/rose rir compilateur/emetteur_elf.rose > /tmp/opencode/bigint/f1f2/e_final.rir   # 348 821 o, 909c4d1b…
./rose/rose compilateur/emetteur_elf.rose e_final.rir > F1_final.elf
./F1_final.elf e_final.rir > F2_final.elf
cmp F1_final.elf F2_final.elf
```
Première salve (`e.rir`, 349 001 o, généré AVANT l'optimisation des sauts) :
`F1.elf` émis par le natif en **14 min 56 s**, rc=0, 604 092 o, md5
`67283c07b3816de036705d7c8bc4b018`, stderr vide. Le RIR ayant changé
ensuite (180 o de moins), cette salve ne valide que la cohérence interne de
l'émetteur, pas l'état final du source. Résultat F2 et salve finale : à
compléter après mesure.
