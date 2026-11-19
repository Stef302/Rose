# SE1 — D2 : battre Rust sur la boucle 1M par réassociation à 8 accumulateurs

**Constat daté : 19/11/2026, 14:12 → 15:35 UTC.** Toutes les mesures ci-dessous
sont des constats à cet horodatage, pas des consignes : re-mesurer après tout
changement de moteur, de quantification ou de topologie.

**Périmètre respecté** : `rose/src/jit.rs` (+167 −1 ligne) et ce document
(nouveau). Aucun autre `.rs`, aucun `Cargo.toml`, aucun `PLAN.md`, aucun doc
d'un autre agent. Aucun commit, aucune publication. `rose/rose` **non remplacé**
: le binaire de travail a été construit dans `/tmp/opencode/se1_agents/`.

**Protocole** : 9 (puis 12) échantillons **entrelacés** Rose/Rust dans la même
salve, ordre alterné à chaque tirage, médianes, pinage `taskset -c 6`
(contrôles `-c 15` et `-c 19`), résultat `499999500000` vérifié à chaque tirage.
Horloge interne Rose (`temps()`) et horloge interne Rust (`Instant`), donc
calcul seul, hors démarrage process. Anti-optimisation Rust : `black_box(k)`.

---

## 1. Reproduction du point de départ (étape 1)

Binaire construit depuis HEAD `0f0f58d` :
`/tmp/opencode/se1_agents/rose_base`, SHA256
`3e208248cb12a44fcbbf9b742c7ca0488e30de703302f9221e708c6003f6dedf`.
Log : `d2_base.log` (14:12Z, 9 échantillons/pin).

| pin | Rose méd. | Rust -O méd. | Rose/Rust |
|---|---:|---:|---:|
| **c6** | **0,2591 ms** (min 0,2585 / max 0,3001) | 0,2589 ms | **1,0007** |
| c15 | 0,2591 ms | 0,2580 ms | 1,0045 |
| c19 | 0,2588 ms | 0,2614 ms | 0,9900 |

Le constat d'entrée (Rose ~0,259, Rust ~0,259, ratio ~1,00) est reproduit.
Plancher machine mesuré `PLAFOND_APRES.md` : **0,2564 ms** (1 addition
dépendante par itération à 3,9 GHz) — le résidu est de **+1,0 %** seulement.

### Dump et décomposition (`d2_base.dump` / `.dis`, `ROSE_DUMP_CODE` + objdump)

Chemin chaud = 0x38–0xac, **9 instructions pour 2 itérations** (4,5 instr/it) :

```
0x38 sub x1, x19, #1      ← n−1 (invariant, recalculé à chaque tour)
0x3c cmp x13, x1 ; 0x40 b.lt 0x98
0x98 add x3, x13, #2 ; 0x9c add x4, x0, x13 ; 0xa0 add x5, x13, #1
0xa4 add x0, x4, x5       ← SEULE addition de la chaîne critique : s += k + (k+1)
0xa8 mov x13, x3 ; 0xac b 0x38
```

**Prémission corrigée** : le résidu n'est PAS un excès d'instructions
(9/2 itérations est déjà compact) ni des checks de tag/d'overflow (ils ont été
supprimés en V21, `JIT_BOUCLE_HOT.md`). C'est la **dépendance sérielle
`x0 → x4 → x0`** : une seule addition critique par itération. Aucune
réduction du nombre d'instructions ne peut descendre sous 0,2564 ms avec une
seule chaîne. C'est le goulot #1, et la décision qui suit est la seule qui le
traitement : **casser la chaîne critique par réassociation**, pas réduire le
code mort.

---

## 2. Variantes isolées (étape 2)

Toutes les variantes ont été mesurées **dans un seul binaire**
(`7478b5e26b4f6718e56fcf4b4352aa4f479cefe4938114604808d0ce5c064cdc`),
sélectionnées par une variable d'environnement lue au moment de la
compilation JIT — donc hash binaire identique et biais inter-salves nul.
Log `d2v_varA.log` (14:22Z, 9 échantillons × c6 + c19), duel `d2_a4a8.log`
(14:23Z, 12 échantillons × c6 + c19).

| Variante | c6 méd. | c19 méd. | Dump chemin chaud | Verdict |
|---|---:|---:|---|---|
| A1 = V21 historique (1 chaîne, ×2) | 0,2586 | 0,2586 | 9 instr / 2 it | référence, au plancher |
| A2 = 2 accumulateurs, pas ×2 | 0,2647 | 0,2586 | 6 instr / 2 it | **rejetée** : nul gain |
| A4 = 4 accumulateurs | 0,0983 | 0,0996 | 10 instr / 4 it | bonne, mais A8 mieux |
| **A8 = 8 accumulateurs** | **0,0982** | **0,0982** | **18 instr / 8 it** | **conservée** |
| A16 = 16 accumulateurs | 0,1274 (c6) | — | 6 `ldur` de débordement | **rejetée** |
| Rust -O (contrôle) | 0,2616 | 0,2604 | — | — |

Duel A4/A8 à 12 échantillons : c6 A4 0,0992 / A8 0,0984 (A8/A4 = 0,9924) ;
c19 A4 0,0986 / A8 0,0982 (0,9961). A8 gagne sur les deux cœurs, écart faible
mais reproductible → retenu.

**Pourquoi A2 ne gagne rien** (preuve par le dump) : avec 2 accumulateurs,
le compteur reste une chaîne sérielle `x14 → x10 → x14` à 1 addition par
groupe de 2 ; les deux accumulateurs en dépendent, donc la chaîne critique
reste **1 addition par itération**. A4/A8, eux, ont 4/8 additions
indépendantes par groupe, soit 0,25/0,125 addition critique par itération.

**Pourquoi A16 régresse** : 16 accumulateurs + compteur + garde dépassent les
registres disponibles ; le dump montre 6 accumulateurs débordés sur la pile
(38 `str`/`ldur` dans la fonction contre 0 pour A8) → 0,1274 ms.

### Rejets sans mesure (preuve structurelle)

| Variante | Preuve | Décision |
|---|---|---|
| Garde bigint hoistée | Déjà émise **une seule fois** dans le bloc `entry` (avant la boucle), dans A1 comme dans A8. Rien à hoister. | sans objet |
| Registres callee-saved | Déjà utilisés : prologue `stp x19,x20 … x27,x28`, boucle A8 entièrement en x20–x27/x0/x1. | sans objet |
| Spécialisation somme étendue | Le détecteur `detecter_somme_nu` reconnaît déjà **exactement** le RIR de D2 (7 ops) ; l'élargir ne change pas D2. | sans objet |
| Forme close `n(n−1)/2` | Rejetée en `JIT_BOUCLE_HOT.md` §3.3 : contourne le benchmark au lieu d'optimiser la boucle. | rejetée (inchangé) |

---

## 3. Décision (étape 3) — émetteur à 8 accumulateurs

`emettre_somme_acc(nacc)` garde **tout** le cadre de l'émetteur V21 :
détecteur strict inchangé, garde `n ≤ 2^32`, repli bigint **identique**
(boucle générique vérifiée avec `inline_add_tagge` + `rose_add_ent`, tags
réels), publication du tag de retour. Seul le chemin rapide change :

```
k = 0 ; acc[0..7] = 0
tant que k < n−8 :  acc[j] += k+j  (j = 0..7) ; k += 8
queue (< 8 termes) : acc[0] += k ; k += 1
s = Σ acc[j]
```

**Preuve de borne (formelle, n ≤ 2^32)** : `acc[j] = Σ_{m=0}^{n/8−1}(8m+j)
= n²/16 − n/2 + j·n/8`. Maximum en `j = 7` : `n²/16 + 3n/8 = 2^60 + 3·2^29/8`
pour `n = 2^32`, très inférieur à `i64::MAX = 2^63−1`. L'addition étant
associative sur cette plage (aucun débordement), le résultat est **exactement**
celui de la boucle source. `n < 0` : aucune itération, `s = 0`.

**Dump final (`d2_final.dump`)** : 0xd0–0x114 = **18 instructions / 8
itérations** (2,25 instr/it ; 2,625 avec le header), 8 chaînes `add acc, acc, k`
indépendantes dans x20–x27, **0 `blr`, 0 accès pile, 0 `mov` de permutation**
sur le chemin chaud (1 seul `mov x0,x1` de recyclage du compteur). Contre
9 instr/2 it pour A1 : ×2 d'instructions par itération en moins, et surtout
**8 additions critiques au lieu de 1 par bloc de 8**.

---

## 4. Résultat (étape 4) — porte atteinte

Binaire final `/tmp/opencode/se1_agents/rose_final`, SHA256
`3801584d9ff819f7a03216e2b1358bdd369bb7d71a05bd2de27041a1bf96360f`.
Log `d2_final.log` (14:26Z, 9 échantillons entrelacés base/final/Rust).

| pin | base (A1) | **final (A8)** | Rust | final/base | **final/Rust** |
|---|---:|---:|---:|---:|---:|
| **c6** | 0,2610 | **0,0999** | 0,2599 | 0,3827 | **0,3844** |
| c15 | 0,2617 | **0,0994** | 0,2600 | 0,3797 | **0,3821** |
| c19 | 0,2597 | **0,0987** | 0,2589 | 0,3799 | **0,3811** |

**Porte D2 < 0,259 ms atteinte** sur les trois cœurs, avec marge :
0,0987–0,0999 ms, ratio **0,38** vs Rust dans la même salve (gain ×2,6).

### Tableau 9 domaines (bench multi, c6, 9 échantillons, base → final)

Logs `bench_se1_base_c6.log` / `bench_se1_c6b.log`, résumés
`summary_base.json` / `summary_final.json`. Temps internes Rose, ms.

| Domaine | base | final | base/Rust | final/Rust | Δ |
|---|---:|---:|---:|---:|---:|
| D1 fib35 | 11,6310 | 11,6561 | 0,917 | 0,921 | +0,2 % |
| **D2 boucle 1M** | **0,2599** | **0,0994** | 0,997 | **0,382** | **−61,8 %** |
| D3 premiers 50k | 0,8542 | 0,8528 | 0,851 | 0,851 | −0,2 % |
| D4 fact 5000 | 1,4346 | 1,4254 | 0,219 | 0,218 | −0,6 % |
| D4b pgcd 1000 ch. | 0,1014 | 0,1019 | 0,687 | 0,690 | +0,5 % |
| D5 chaîne 20k | 0,0109 | 0,0112 | 0,740 | 0,764 | +3,4 % |
| D6 liste 100k | 0,3878 | 0,4077 | 2,061 | 2,129 | +5,1 % |
| D7 table 3000 | 0,1086 | 0,1081 | 1,143 | 1,125 | −0,4 % |
| D8 E/S 10 Mo | 6,8871 | 6,8357 | 0,991 | 0,989 | −0,7 % |

**Aucune régression causale.** D5/D6 sont du bruit inter-salves : un duel
A/B entrelacé dédié sur `r_list.rose` (12 tirages, c6) donne base 0,3312 ms,
final 0,3274 ms (final/base = **0,9887**) — le +5,1 % du tableau n'existe pas
quand les deux binaires sont mesurés dans la même salve. Le changement est
confiné à l'émetteur `somme`, que D1/D3–D8 n'appellent pas. (D4b et D5 sont
désormais devant Rust dans le dépôt partagé : gains d'autres agents, non
attribuables à SE1.)

---

## 5. Double verdict (règle utilisateur)

**(a) Verdict « même source, même algorithme itératif »** : Rose 0,259–0,262 ms
contre Rust 0,259–0,261 ms, ratio ≈ 1,00. **Pas de 6ᵉ victoire revendiquée.**
Cause chiffrée : l'émetteur V21 est déjà au plancher machine (0,2564 ms,
écart +1 %), avec 1 addition dépendante par itération ; aucune optimisation
de la même chaîne ne peut le battre de façon robuste.

**(b) Verdict « meilleur algorithme »** : **6ᵉ victoire revendiquée.**
Méthode : **réassociation de la somme en 8 chaînes d'accumulation
indépendantes, déroulage ×8** (même programme source Rose, même garde `n ≤
2^32`, même repli bigint exact). Rose 0,0987–0,0999 ms, Rust 0,2589–0,2600 ms,
ratio **0,38** (×2,6) sur c6/c15/c19 dans la même salve.

---

## 6. Exactitude et validation

- `cargo build --release --bin rose` : OK.
- `cargo test --release` : **220 + 204 + 6 + 1 = 431 passed, 0 failed**
  (3 + 3 ignorés), rc=0.
- Jardin `jardins/tests/premiers.rose` : **20 tests, 0 échec**. `fib(25) = 75025`.
- Point fixe M4.5 : `rose rir compilateur/emetteur.rose` et
  `rose compilateur/emetteur.rose compilateur/emetteur.rose` →
  **`b084ac5c926f6ae486845f9bc96c616d` ×2** (égaux).
- `bench_global.rose` (depuis un dossier temporaire) : **rc=0**.
- `d2_exact_loop.rose` : md5 **`fb6dacf0f54be7f9b941ec62fb3bfd8e`**
  identique base/final, diff vide (6 cas bigint/boxage/débordement).
- `exact_consolidee.rose` : md5 **`be8ec0bd7c94519fb2ebe4e12b81c93c`**
  identique base/final, diff vide (8 cas dont `1e10²`, `MAX−(−MAX)`).
- 30 valeurs de `n` (−5, −1, 0, 1…17, 31/32/33, 63/64/65, 100, 1000,
  65535/65536, 999999, 1000000/1000001) : sorties **identiques** entre A1,
  A2, A4 et A8 (`d2_exact_acc.rose`, `exact_acc*.txt`).
- Repli bigint forcé (`ROSE_D2_LIM=8` pendant l'expérimentation, hook retiré
  depuis) : `n = 9…1000001` identiques à A1 — boucle générique vérifiée correcte.
- Oracles M4 partiels (audit adapté, `audit_se1.log`) : M4.1 lexeur **174/0**,
  M4.2 analyseur **174/0** (l'audit complet a dépassé le délai imparti en M4.3 ;
  ces deux portes et les contrôles directs ci-dessus constituent la preuve).

---

## 7. Artefacts

- Binaires : `/tmp/opencode/se1_agents/rose_base` (`3e208248…`),
  `rose_acc` (`7478b5e2…`, variantes), `rose_final` (`3801584d…`).
- Logs : `d2_base.log`, `d2v_varA.log`, `d2_a4a8.log`, `d2_final.log`,
  `bench_se1_base_c6.log`, `bench_se1_c6b.log`, `audit_se1.log`,
  `cargo_test.log`.
- JSON : `d2_base.json`, `d2v_varA.json`, `d2_a4a8.json`, `d2_final.json`,
  `summary_base.json`, `summary_final.json`.
- Dumps : `d2_base.dump/.dis`, `d2_acc1/2/4/8/16.dump/.dis`, `d2_final.dump/.dis`.
- Harnais : `d2_rs.py`, `d2_variants.py`, `d2_a4a8.py`, `d2_final.py`,
  `bench_se1.py`, `bench_se1_base.py`.

Constat daté, pas consigne.
