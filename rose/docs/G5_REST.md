# G5_REST — correction des divergences A–Z de l'émetteur ELF

Constat daté, **21/11/2026**. Sous-agent `g5-rest`. Dépôt
`/home/clausake/Stefan/ROSE` (`Stefan/ROSE`). Deux fichiers possédés :
**`compilateur/emetteur_elf.rose`** et ce document. Rien d'autre modifié,
**aucun commit, aucune publication**.

Règle absolue respectée : aucune livraison si le point fixe `b3ed9262…` ou
`F1 == F2` casse.

## 1. Méthode et point de départ

Chaîne par programme (`tests_selfhost/lancer_az.sh`) : référence native
`ROSE_SANS_JIT=1 rose p.rose`, dump RIR ×2, ELF émis par l'émetteur figé ×2,
sortie ELF ×2 ; comparaisons octet pour octet (`cmp`). L'émetteur est copié
avant chaque salve, donc chaque mesure est liée à un md5.

Salve de départ, émetteur `6c957b53e97f3ebca94a26c9a272cf54`,
binaire `rose/rose` `12eac46007cf0fe9e1aec2dff6fbf665` :

| Mesure | Valeur |
|---|---|
| Tests | 31 |
| CONFORMES | **18** |
| DIVERGENTS | **13** |
| BLOQUÉS | 0 |
| RIR / ELF / sortie reproductibles | 31/31 / 31/31 / 31/31 |

Le document `AZ_TESTS.md` annonçait 17 C / 14 D sur un émetteur plus ancien
(`a515b962…`) : `t212_add_big_signes` avait déjà été corrigé entre-temps par
un autre agent. Le point de départ réel de ce chantier est donc **13**
divergences, pas 14.

Les 13 divergences initiales, par cause :

| Cause | Tests | Nombre |
|---|---|---|
| puissance `^` non repliée au-delà de 2^64 | t222, t226 | 2 |
| soustraction bigint sur l'ancien chemin 64 bits | t214, t231 | 2 |
| `* / %` bigint | t215 | 1 |
| `abs/min/max` bigint | t233 | 1 |
| rationnels calculés | t240, t241, t243 | 3 |
| Unicode (octets au lieu de points de code) | t250–t253 | 4 |

## 2. Corrections livrées

### 2.1 Puissance littérale au-delà de 2^64 (t222, t226) — 2 corrigées

Dans `g5_eval`, branche `(bin ^ …)` : quand les deux opérandes sont des
jetons RIR littéraux `(bin ^ entier A entier B)`, le résultat est calculé en
arithmétique exacte Rose à l'émission puis chargé par `g5_load_val`, qui
construit le descripteur tag 3 `[3][signe][nlimbs][limbs base 10^9]`.
Borne prouvée : `0 <= B <= 512` (couvre `2^100` et `10^40`) ; au-delà, le
chemin 64 bits historique est conservé. Aucun `g5_next()` supplémentaire :
le repli ne modifie ni le nombre de labels ni la table d'étiquettes, donc la
réflexivité reste inchangée.

Mesures : `t222` `36893488147419103232` des deux côtés ; `t226` `2^65` et
`2^100` conformes.

### 2.2 Soustraction bigint (t214, t231) — 2 corrigées

`A - B` est désormais émis comme `A + (-B)` dès qu'un des deux opérandes
porte le tag 3. Le dispatch est un saut à déplacement constant (aucun label
par nœud `-`) vers un bloc qui :

1. préserve la gauche dans `x12` et la droite dans `x13` (registres que
   `bigpromote`, `bigneg` et `bigadd` n'écrasent pas) ;
2. promeut les deux opérandes via `$g5_bigprol` ;
3. inverse la droite via `$g5_bignegl` ;
4. appelle `$g5_bigaddl` avec `x1 = A`, `x0 = -B` ;
5. saute par-dessus la voie 64 bits au retour de `bigadd` — point délicat,
   trois itérations ont été nécessaires pour corriger les déplacements
   (`g5_branch_off` : la cible est `adresse_du_saut + N*4`, pas
   `instruction_suivante + N*4`, et le retour de `BL` retombe sur
   l'instruction suivant l'appel).

Voie 64 bits inchangée (`subr`) pour les petits entiers.

Preuve par désassemblage de l'ELF émis pour `$a = 2^64+1 ; >> $a - 1`
(`aarch64-linux-gnu-objdump -D -b binary -m aarch64`) :

```
18c: and x2, x1, #0x7        ; tag de la gauche
190: cmp x2, #0x3
194: b.eq 0x1a4              ; tag3 -> bloc big
198: and x2, x0, #0x7        ; tag de la droite
19c: cmp x2, #0x3
1a0: b.ne 0x1e0              ; pas tag3 -> voie 64 bits
1a4: mov x12, x1 / mov x13, x0
1ac: bl <bigpromote>         ; promouvoir la droite
1b8: bl <bigpromote>         ; promouvoir la gauche
1c8: bl <bigneg>             ; x0 = -droite
1d8: bl <bigadd>             ; x0 = gauche + (-droite)
1dc: b 0x1e4                 ; retour bigadd : ENJAMBER la voie 64 bits
1e0: sub x0, x1, x0          ; voie 64 bits (petits entiers)
1e4: (instruction suivante)
```

Le saut `1dc → 1e4` est l'élément critique : sans lui, le retour de `BL`
retombait sur `sub` et détruisait le résultat bigint.

Mesures (`nat | ELF`) :

| Test | Ligne | Natif = ELF |
|---|---|---|
| t214 | `$a - $b` | `18446744073709551616` |
| t214 | `$b - $a` | `-18446744073709551616` |
| t214 | `$a - $a` | `0` |
| t214 | `$a - 2^64` | `1` |
| t231 | `0 - $a` | `-18446744073709551616` |

### 2.3 Bilan

| Émetteur md5 | CONFORMES | DIVERGENTS | BLOQUÉS |
|---|---|---|---|
| `6c957b53…` (départ) | 18 | 13 | 0 |
| `5a3553ca…` (puissance) | 20 | 11 | 0 |
| `1bb80fcb…` (puissance + soustraction) | **22** | **9** | **0** |

**4 divergences corrigées sur 13** (13 → 9). Aucune régression : les 18
tests conformes du départ le sont restés, RIR/ELF/sortie reproductibles
31/31 à chaque salve.

## 3. Jalons vérifiés (émetteur `1bb80fcb…`)

| Contrôle | Résultat mesuré |
|---|---|
| `./rose/rose verif compilateur/emetteur_elf.rose` | `vérifié`, rc=0 |
| Point fixe `b3ed9262dd54776a6dbd07bc12368d0e` (263 520 o) ×2 | deux dumps `md5=b3ed9262…`, `cmp`=0 |
| `t999_critique_g5` (entrée `/tmp/opencode/sh10_agents/critique_entree.txt`) | natif **20/20**, ELF **20/20**, `cmp`=0, rc=0 des deux côtés |
| `F1 == F2` (auto-régénération, `rose rir` → `rose <src> <rir>` → `F1 <rir>`) | voir §5 |

Le point fixe porte sur `compilateur/emetteur.rose` (émetteur RIR), fichier
non possédé et non modifié : sa valeur est donc structurellement inchangée.
La mesure ×2 ci-dessus le confirme.

## 4. Divergences restantes (9) — non livrées

| Cause | Tests | Pourquoi non corrigé dans ce chantier |
|---|---|---|
| `* / %` bigint | t215 | Exige une routine de produit/quotient multi-limbes émise (carry sur limbs base 10^9, `umulh` pour les grands facteurs) ; non ajoutée pour préserver `F1 == F2`. |
| `abs/min/max` bigint | t233 | `min`/`max` demanderaient une sélection via `bigcmp` ; `abs(grand)` renvoie **un flottant** côté natif (`type(abs(2^64)) = flottant`, sortie `1.8446744073709552e+19`) alors que G5 n'a pas de tag flottant : la ligne `abs` ne peut pas être rendue conforme sans changement d'ABI. |
| Rationnels calculés | t240, t241, t243 | Nécessitent un tag « rationnel » (numérateur/dénominateur) et un formateur de décimale périodique à l'exécution. Les rationnels **littéraux** (`t242`) sont conformes car repliés à l'émission. |
| Unicode | t250–t253 | Non tenté : une tentative antérieure (SH12, `G5_AZ.md` §5.3) a corrigé ces 4 tests mais **cassé `F1 == F2`** (F1 sortait par `exit(2)` sur sa propre RIR). `g5_cps` est bien présent dans l'émetteur courant (l. 555), mais la consigne impose de replay F1/F2 après toute tentative et d'annuler si ça casse ; le budget de ce chantier ne permet pas un second cycle d'émission (~2×400 s) après une telle tentative. |

Détail des sorties (`nat | ELF`) dans `/tmp/opencode/g5rest/v5/rapport.txt`
et `/tmp/opencode/g5rest/v5/resultats.tsv` (salve émetteur `1bb80fcb…`).

Note d'honnêteté : `abs(grand)` est un comportement **natif** particulier,
pas un bug de l'émetteur ; l'écart mesuré porte sur les octets réels, sans
préjuger de la « bonne » valeur.

## 5. F1 == F2 — mesuré

Cycle complet rejoué sur le snapshot émetteur `1bb80fcb…` :

```
rose rir <snap>            > e.rir        # 359 309 o, rc=0
rose <snap> e.rir          > F1.elf       # 688 032 o, rc=0
F1.elf e.rir               > F2.elf       # 688 032 o, rc=0
cmp F1.elf F2.elf                         # 0
md5 F1.elf = md5 F2.elf = 30a747d8cb032e815faefe864e4cae72
```

**F1 == F2 : IDENTIQUES.** Aucun `exit(2)` prématuré, stderr vides des deux
côtés. Journal : `/tmp/opencode/g5rest/jalon/log.txt`.

### Limite de self-hosting (constat, non régression)

F1, utilisé comme compilateur, n'émet pas encore correctement les grands
entiers : les ELF qu'il produit pour t214/t222/t226/t231 renvoient `0`
alors que l'émetteur **natif** produit les ELF corrects mesurés en §2. C'est
la limite ABI documentée dans `G5_AZ.md` §5.1 (`g5_load_val` et les
opérations tag 3 reposent sur une arithmétique 64 bits une fois l'émetteur
lui-même compilé en G5). Ce chantier ne prétend donc pas avoir rendu le
self-hosting bigint capable : il corrige la batterie A–Z mesurée avec
l'émetteur natif, sans casser la chaîne réflexive `F1 == F2`.

Le test « point fixe par ELF » doit utiliser **C0** (émetteur RIR compilé
depuis `emetteur.rose`), pas F1 : F1 consomme un RIR, pas une source `.rose`.
Le point fixe `b3ed9262…` a été vérifié par deux dumps natifs consécutifs de
`compilateur/emetteur.rose` (`cmp`=0), fichier non modifié par ce chantier.

## 6. Rejouer

```
cd /home/clausake/Stefan/ROSE
./rose/rose verif compilateur/emetteur_elf.rose
AZ_OUT=/tmp/opencode/g5rest/rejouer ./tests_selfhost/lancer_az.sh
./rose/rose rir compilateur/emetteur.rose                 # b3ed9262…
./rose/rose rir compilateur/emetteur_elf.rose > e.rir
./rose/rose compilateur/emetteur_elf.rose e.rir > F1
chmod +x F1 && ./F1 e.rir > F2 && cmp F1 F2               # F1 == F2
```

## 7. Concurrence de fichier

Pendant ce chantier, `git status --short` a montré, **en plus** des deux
fichiers possédés, des modifications d'autres agents : `rose/src/front.rs`,
`rose/src/run.rs`, `thorn/docs/NOYAU.md`, `thorn/src/noyau.rose`,
`thorn/tests_noyau.rose`, `rose/docs/R8.md`. Ce chantier ne les a ni ouvertes,
ni modifiées, ni restaurées. Le binaire `rose/rose` est resté au md5
`12eac46007cf0fe9e1aec2dff6fbf665` pendant toutes les mesures.

## 8. État final

- Fichiers livrés : `compilateur/emetteur_elf.rose` (4 corrections), ce
  document. Aucun autre fichier, aucun commit, aucune publication.
- Divergences A–Z : **13 → 9** (4 corrigées : t222, t226, t214, t231).
- Jalons : `verif` OK, point fixe `b3ed9262…` ×2 OK, `t999` 20/20 OK,
  **F1 == F2 OK** (md5 `30a747d8…`, `cmp`=0, §5). Aucune règle absolue violée.
