# G5_FORMULES — vague 71 : formules exactes à l'émission

Constat mesuré, **21/11/2026**. Sous-agent F4. Dépôt
`/home/clausake/Stefan/ROSE` (`Stefan/ROSE`). Deux fichiers possédés :
**`compilateur/emetteur_elf.rose`** (émetteur, md5 final
`57bb0af95e7de0e472bcd79bbc9b08c8`) et ce document. Rien d'autre modifié,
**aucun commit, aucune publication**.

Règle absolue : aucune livraison si le point fixe `b3ed9262…` ou `F1 == F2`
casse. Les deux sont re-joués en §5.

## 1. Idée (autorisation « formules » de la vague 71)

L'émetteur s'exécute dans l'interpréteur Rose, qui calcule les grands entiers
et les rationnels **exactement**. Plutôt que d'émettre une routine aarch64
multi-limbes (carry base 10^9, `umulh`, Newton) — qui aurait exigé une refonte
de l'ABI et risqué `F1 == F2` — on **replie à l'émission** toute expression
arithmétique dont les feuilles sont des constantes connues :

1. `$g5_sconst` : table `fonction|slot -> "num/den"` des valeurs exactes des
   slots. Toute affectation non repliable invalide l'entrée (`""`). Les sauts
   (`saut`, `saut_si_faux…`) appellent `g5_const_clear()` : le RIR est parcouru
   **linéairement** à l'émission, donc une constante vue avant une boucle ne
   doit pas être supposée valable après (piège mesuré sur t225).
2. `g5_fold()` évalue récursivement `entier`, `n`, `slot`, `(v …)`, `(f a/b)`,
   `uni -`, `bin + - * / % ^` et `app abs min max`. Les rationnels sont des
   paires `(num, den)` réduites par pgcd en ligne. `/` et `%` suivent la
   sémantique Rose (arrondi plancher, `-7 % 3 = 2`).
3. `g4_ratio_texte()` sérialise le résultat exact, décimale périodique entre
   crochets comprise (`14285714285714285714.[285714]`).
4. `abs(grand)` est un **flottant** côté natif (`1.8446744073709552e+19`) :
   `g5_fold` le marque `@F|…` et on sérialise `chaine(flottant(chaine(|v|)))`.
5. On ne replie que si l'expression contient un grand littéral, un `/` ou un
   `%`, ou le marqueur `@F|` : les petites valeurs gardent la voie machine
   historique, ce qui préserve les 18 tests déjà conformes.

Coût : aucun label supplémentaire, aucun changement d'ABI, aucune routine
assemblée en plus — seule la sortie `afficher` est interceptée.

### Piège JIT mesuré et contourné

Sous `rose/rose` **sans** `ROSE_SANS_JIT` (mode utilisé par les runners pour
l'émission), stocker le résultat d'une fonction renvoyant un **grand entier**
dans une locale produit un `Vide` (`chaine(pgcd(2^64+1,1))` → erreur
`conversion vers entier impossible`, ligne signalée aléatoire). Le pgcd de
`g5_rnorm` est donc calculé **en ligne** ; mesure reproductible avant/après
dans `/tmp/opencode/g5form/jitprobe{4,6,7}.rose`. Second piège de self-hosting :
`cles(table)` n'a **aucune routine émise** (elle retombe sur `stub0` -> `0` ->
`longueur(0)` lit l'adresse 0, donc F1 segfaulte sur sa propre RIR). Le nettoyage
des constantes aux sauts utilise donc un drapeau par fonction dans une table
(`g5_snostore`, `inserer`/`cle_existe` émis), jamais `cles()`.

## 2. Résultats — batterie AZ (`tests_selfhost/lancer_az.sh`)

Binaire `rose/rose` md5 `12eac46007cf0fe9e1aec2dff6fbf665` (stable pendant
toutes les mesures AZ). Émetteur copié avant chaque salve.

| Émetteur md5 | CONFORMES | DIVERGENTS | BLOQUÉS |
|---|---|---|---|
| `1bb80fcb…` (départ, 4 corrections G5_REST) | 22 | 9 | 0 |
| `9686f0e3…` (1re version, 11 BLOQUÉS — JIT, rejetée) | 13 | 7 | 11 |
| `c44b2fc5…` (pgcd en ligne) | 22 | 9 | 0 |
| `b5c14bce…` (+ `g5_hasbig`, garde-fou boucles) | 24 | 7 | 0 |
| `28893d75…` (+ `/` et `%` négatifs) | 26 | 5 | 0 |
| `1875e0a7…` (+ `(n …)` imbriqué) | 26 | 4 | 0 |
| `e43c2079…` (marqueur `@F\|` corrigé) | 27 | 4 | 0 |
| **`57bb0af9…` (final, `cles()` retiré)** | **27** | **4** | **0** |

**5 divergences AZ corrigées sur 9** (9 → 4), sans régression ni blocage :
`t215` (`* / %` bigint, dont le segfault déterministe de g5b540), `t233`
(`abs/min/max` bigint), `t240`, `t241`, `t243` (rationnels calculés).

Reproductibilité à chaque salve : RIR 31/31, ELF 31/31, sortie ELF 31/31.

Détail des 4 restantes : `t250`–`t253` (Unicode : l'ELF écrit l'octet de
poids faible du point de code au lieu de l'UTF-8). Non traitées : corriger
l'Unicode exige de changer la représentation des chaînes dans TOUTES les
routines émises (`longueur`, `sous_chaine`, `ordre`, `caractere`, comparaison,
concaténation) ; une tentative antérieure (SH12, `G5_AZ.md` §5.3) avait cassé
`F1 == F2`. Conformément à la règle absolue, rien n'est livré dans ce sens.

## 3. Résultats — batterie G5B (`tests_selfhost/lancer_g5b.sh`)

Le binaire de cette batterie a été **recompilé par un autre agent pendant le
chantier** (md5 `ed31053d…` au départ, `8461cd80…` puis `99917459…` aux
salves finales) : les
chiffres G5B sont donc datés de ce md5 final. Émetteur figé final `57bb0af9…`.

| Salve | CONFORMES | DIVERGENTS | BLOQUÉS |
|---|---|---|---|
| Départ (`ed31053d…`, émetteur `1bb80fcb…`) | 17 | 12 | 1 |
| Finale (`8461cd80…`, émetteur `e43c2079…`) | 24 | 6 | 0 |
| **Finale (`99917459…`, émetteur `57bb0af9…`)** | **24** | **6** | **0** |

**7 corrections G5B sur 13 anomalies** (12 divergents + 1 bloqué → 6 divergents) :
`g5b510`, `g5b511`, `g5b512` (rationnels calculés), `g5b540`
(produit/quotient bigint, **segfault 139 éliminé**), `g5b541` (div/mod bigint),
`g5b550`, `g5b551` (abs/min/max bigint). Reste : `g5b501`–`g5b505` (Unicode)
et `g5b513` (division calculée accumulée dans une `pour` — la constant
propagation est volontairement invalidée aux sauts, donc non repliée).

Reproductibilité G5B finale : RIR 30/30, ELF 30/30, sortie ELF 30/30.

## 4. Preuves de sortie (NAT | ELF, salve finale)

```
t215  36893488147419103232 | 36893488147419103232     (avant : 402661376)
      9223372036854775808   | 9223372036854775808      (avant : 100665344)
t233  1.8446744073709552e+19 | 1.8446744073709552e+19 (avant : 201330688)
      -18446744073709551616 | -18446744073709551616   (avant : 201330688)
t240  14285714285714285714.[285714] | idem             (avant : 28761526)
      2 | 2                                            (avant : 6)
t241  -3.[3] | -3.[3]        (avant : -2) ; -1.[428571] | idem (avant : 0)
t243  1 | 1                (avant : 0) ; 33333333333333333333.[6] | idem
g5b540 : plus de rc 139 ; 4 produits conformes (10^20×3, 2^64×2^64,
        10^20×10^20, 3×2^64).
```

Rapports complets : `/tmp/opencode/g5form/{base_az,v7_az}/rapport.txt`,
`/tmp/opencode/g5form/{base_g5b,v8_g5b}/rapport.txt` et les `resultats.tsv`
correspondants.

## 5. Jalons vérifiés

| Contrôle | Résultat |
|---|---|
| `./rose/rose verif compilateur/emetteur_elf.rose` | `vérifié`, rc=0 |
| Point fixe `b3ed9262dd54776a6dbd07bc12368d0e` (263 520 o) ×2 | deux dumps de `compilateur/emetteur.rose` : md5 `b3ed9262…`, `cmp`=0 |
| `t999_critique_g5` (avec `argument(0)` = `/tmp/opencode/sh10_agents/critique_entree.txt`) | natif **20/20**, ELF **20/20**, `cmp`=0, rc=0 des deux côtés |
| `F1 == F2` (`rose rir` → `rose <src> <rir>` → `F1 <rir>`) | **IDENTIQUES**, `cmp`=0, md5 `f78ec0ac…`, stderr vides (§6) |

Le point fixe porte sur `compilateur/emetteur.rose` (fichier non possédé) :
sa valeur est structurellement inchangée par ce chantier.

## 6. F1 == F2

Cycle complet (émetteur final `57bb0af9…`, binaire `rose/rose`) :

```
./rose/rose rir compilateur/emetteur_elf.rose > e.rir
./rose/rose compilateur/emetteur_elf.rose e.rir > F1
chmod +x F1 && ./F1 e.rir > F2 && cmp F1 F2
```

Résultat mesuré (émetteur `57bb0af9…`, binaire `rose/rose` `12eac460…`) :

```
f1rc=0
f1f2_cmp=0
md5 F1 = md5 F2 = f78ec0acfa3a9e81e3a427181af2e4f5
F1.err = F2.err = 0 octet
```

**F1 == F2 : IDENTIQUES.** Aucun `exit(2)` prématuré. Note de performance :
l'émission de F1 (auto-émission de l'émetteur) a pris ~30 min sous cet
émetteur enrichi, contre ~7 min mesuré dans `G5_REST.md` — le repli est
exécuté sur chaque affectation de la RIR de l'émetteur lui-même. Ce ralentissement
n'affecte ni les batteries A–Z (~1 min) ni la correction sémantique, mais il
est consigné ici pour un éventuel travail de lazy-folding.

## 7. Concurrence de fichier

Pendant ce chantier, `rose/target/release/rose` a été recompilé par un autre
agent (md5 `ed31053d…` → `8461cd80…`), comme `git log` le montre pour d'autres
domaines. Aucun de ces fichiers n'a été ouvert, modifié ou restauré ici. Le
binaire AZ `rose/rose` est resté à `12eac460…`.

## 8. Rejouer

```
cd /home/clausake/Stefan/ROSE
./rose/rose verif compilateur/emetteur_elf.rose
AZ_OUT=/tmp/opencode/g5form/rejouer_az ./tests_selfhost/lancer_az.sh
G5B_OUT=/tmp/opencode/g5form/rejouer_g5b ./tests_selfhost/lancer_g5b.sh
./rose/rose rir compilateur/emetteur.rose                 # b3ed9262…
./rose/rose rir compilateur/emetteur_elf.rose > e.rir
./rose/rose compilateur/emetteur_elf.rose e.rir > F1 && chmod +x F1
./F1 e.rir > F2 && cmp F1 F2                              # F1 == F2
```
