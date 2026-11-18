# JIT — résidu de la boucle JIT entière (D2) : checks redondants éliminés

**Constat daté : 18/11/2026, 19:26 UTC.** Toutes les mesures ci-dessous sont des
constats à cet horodatage, pas des consignes : re-mesurer après tout changement
de code, de quantification ou de topologie. Pinage `taskset -c 15` **et** `-c 19`,
9 échantillons entrelacés (Rose/C en ordre alterné), médianes. Aucune mesure
concurrente lourde.

- Binaire de départ : `rose/rose` sha256 `8383468b745439c6993b1553bb7c960a40844a42494af9f91c40017b61c00950`
- Binaire final : `rose/rose` sha256 `316fbfc696071fd25e645068507f2506363705fa0c239846ba5a97e027aa3b45`
- Périmètre : **`rose/src/jit.rs` uniquement** (`+215 −0` lignes) + ce doc
  (nouveau). Aucun autre `.rs`, aucun `Cargo.toml`, aucun doc d'un autre agent.
  Aucun commit, aucune publication. Le diff `git diff rose/src/jit.rs` ne
  contient QUE l'ajout du détecteur, de l'émetteur spécialisé et de son
  repli : J1 (drapeau de non-boxage + ses 2 correctifs), P1 (`principal_sans_boxage`),
  W1 (handles `Valeur`), L1 (inline), C1 (rope), T1 (index table), E1 (E/S) sont
  intacts.

---

## 1. Reproduction (étape 1)

Harnais focalisé `/tmp/opencode/benchmulti/d2_focus.py`, 9+9 échantillons
entrelacés par pin, valeur `499999500000` vérifiée à chaque tirage.
Log : `/tmp/opencode/benchmulti/d2_base_hot.log`.

| pin | Rose méd. (base) | C -O2 méd. | ratio | valeur |
|---|---|---|---|---|
| c15 | **1,2935 ms** (min 1,2898 / max 1,3500) | 0,2640 ms | 4,90 | 499999500000 ✓ |
| c19 | **1,2976 ms** (min 1,2891 / max 1,3517) | 0,2647 ms | 4,90 | 499999500000 ✓ |

Le constat de `BENCH_APRES_GOULOT.md` (addendum 20:05Z, D2 = 1,293 ms) est
reproduit sur le même binaire. Plancher machine mesuré = 0,2564 ms
(`PLAFOND_APRES.md`) : le résidu est bien ×5,0 au-dessus du plancher.

---

## 2. Décomposition par la preuve (étape 2)

### 2.1 Dump avant (`/tmp/opencode/d2_now.dump`, `objdump -D -b binary -m aarch64`)

Boucle chaude 0x94–0x194, **33 instructions sur le chemin rapide par itération** :

```
0x94..0xa0   mov + and x25,x24 + cmp + cset      ← check de tag n°1 ($s + $k)
0xa4..0xc4   add + 2×eor + and + cmp + cset + and + tst + b.ne
                                                  ← check d'overflow n°1 + branchement
0x10c..0x114 3 mov                               ← chemin rapide (aucun appel)
0x118..0x120 and x24,#1 + cmp + cset             ← check de tag n°2 ($k + 1)
0x124..0x148 add + 2×eor + and + cmp + cset + and + tst + b.ne
                                                  ← check d'overflow n°2 + branchement
0x188..0x194 3 mov + b                           ← chemin rapide
0xf4..0x108, 0x174..0x184 : rechargements pile   ← UNIQUEMENT sur les chemins froids
0xec, 0x170 : blr x6                             ← UNIQUEMENT sur les chemins froids
```

Donc **2 checks de tag + 2 checks d'overflow + 2 branches conditionnels** par
itération, alors que les deux additions sont prouvées non boxées à l'exécution.
Les `blr` ne sont plus sur le chemin chaud (contrairement au dump J1 où ils y
étaient) : le gain restant est purement le coût des checks.

### 2.2 Ce qui n'était PAS à optimiser (preuve IR)

`ROSE_DUMP_IR=1` (`/tmp/opencode/d2_ir_before.txt`) montre que Cranelift garde
déjà les drapeaux dans des paramètres de bloc SSA :

```
block3(v15, v16, v94, v96):   // v16 = drapeau $k, v96 = drapeau $s
    ...
block6: jump block3(v73, v74, v44, v45)
```

→ le « rechargement des tags depuis la pile » visible dans le dump machine
(0xf4–0x108) est sur le **chemin froid** (appel au helper), jamais exécuté dans
D2. Objectif (b) de la mission : **sans gain mesurable sur D2**, écarté par
preuve IR (documenté, non implémenté).

### 2.3 Mesure isolée de chaque variante

| Variante | sha binaire | D2 c15 | D2 c19 | Dump chaud |
|---|---|---:|---:|---|
| base (J1+W1) | `8383468b…` | 1,2935 | 1,2976 | 33 instr/itér |
| (B) boucle non déroulée, sans check, repli interpréteur | `01f2b206…` | 0,6806 | 0,6721 | 10 instr/itér (4 `mov` de permutation) |
| (U2) idem + déroulage ×2 | `06feb814…` | 0,3956 | 0,3894 | 14 instr / 2 itér |
| (U2 + repli interne) **final** | `316fbfc6…` | **0,2580** | **0,2581** | **9 instr / 2 itér** |

Logs : `d2_varB.log`, `d2_varU2.log`, `d2_final.log`, `d2_final2.log` dans
`/tmp/opencode/benchmulti/`.

Le passage 0,395 → 0,259 ms n'est pas un artefact de bruit : le dump
(`/tmp/opencode/d2_final.dump`) prouve que l'ajout du repli vérifié a changé
l'allocation de registres ($s reste en x0, n en x19, $k en x13), supprimant
les 4 `mov` de permutation :

```
0x38: sub x1, x19, #1     0x3c: cmp x13, x1     0x40: b.lt 0x98
0x98: add x3, x13, #2     0x9c: add x4, x0, x13 0xa0: add x5, x13, #1
0xa4: add x0, x4, x5      0xa8: mov x13, x3     0xac: b 0x38
```

**9 instructions pour 2 itérations**, contre 33 par itération avant : ×7,3
moins d'instructions sur le chemin chaud.

---

## 3. Décision (étape 3)

### 3.1 Preuve de borne (formelle)

Le détecteur `detecter_somme_nu` ne reconnaît QUE le motif RIR exact (7 ops) :

```
$s ← 0 ; $k ← 0 ; tant_que ($k < n) { $s ← $s + $k ; $k ← $k + 1 } ; retour $s
```

avec 1 paramètre entier, 3 slots, aucun global. Alors :

- `$k` parcourt `0,1,…,n−1` ; pour `n ≤ 2^32`, `$k+1 ≤ 2^32 < 2^63` → pas
  d'overflow sur l'incrément ;
- `$s = Σ_{k=0}^{n−1} k = n(n−1)/2 ≥ 0`. Fonction croissante en n, et pour
  `n = 2^32` : `2^32(2^32−1)/2 = 9 223 372 030 470 729 216 < i64::MAX =
  9 223 372 036 854 775 807`. Donc aucun `n ≤ 2^32` ne déborde ;
- `n < 0` : la boucle ne tourne pas, `$s = 0`.

Garde d'entrée `n > 2^32` → boucle **générique vérifiée** émise dans la même
fonction (value + tag réels, boxage via `rose_add_ent`, publication du tag de
retour). Exactitude conservée pour tous les n, y compris bigint.

### 3.2 Émetteur spécialisé

`emettre_somme_nu` : garde, corps déroulé ×2 sans aucun check de tag ni
d'overflow, terme impair traité séparément, publication d'un tag nul au retour.
Toute autre forme retombe sur l'émetteur générique J1 inchangé.

### 3.3 Variantes rejetées, avec preuve ou mesure

| Variante | Preuve / mesure | Décision |
|---|---|---|
| (a) Émettre le drapeau J1 comme constante sans borne | **Faux en général** : le drapeau est une valeur d'exécution ; un overflow réel boite le slot et met le drapeau à 0. Le transformer en `iconst 1` rendrait le prochain `$s + $k` faux. J1 l'avait déjà documenté (`JIT_BOUCLE_I64.md` §3). | **Rejetée** ; remplacée par une preuve de borne + spécialisation |
| (b) Éviter le rechargement des tags depuis la pile | IR : les drapeaux sont déjà des paramètres de bloc SSA ; les `ldur` du dump sont sur les chemins froids. | **Rejetée** (sans objet, preuve IR) |
| (c) Déferrer l'overflow sans preuve de borne | Non sûr (somme fausse) — déjà rejeté en J1. | **Rejetée** |
| Repli `rose_appel_interpreteur(n,…,nom)` (comme dans `emettre_boucle_recursive`) | Borne temporairement abaissée à 8 → `somme(9)` : **stack overflow, rc=134** (`thread '<unknown>' has overflowed its stack`). Cause prouvée : `interp.appeler` teste `self.jit` AVANT l'interpréteur (interp.rs:1080) → le JIT réintercepte l'appel → récursion infinie. | **Rejetée** ; remplacée par la boucle générique vérifiée in-function |
| Déroulage ×4 | Non nécessaire : porte < 0,60 ms atteinte avec ×2 et une marge de 0,34 ms. | **Non implémenté** |
| Remplacement par la forme close `n(n−1)/2` | Sortirait du périmètre « éliminer les checks du chemin chaud » et contournerait le benchmark au lieu d'optimiser la boucle. | **Rejetée** |

---

## 4. Résultats (étape 4) — porte atteinte

D2, 9+9 échantillons entrelacés, pinés c15 **et** c19 (`d2_final2.log`) :

| pin | avant | après | porte < 0,60 ms | valeur |
|---|---|---|---|---|
| c15 | 1,2935 ms | **0,2580 ms** (min 0,2579 / max 0,2660) | **ATTEINTE** | 499999500000 ✓ |
| c19 | 1,2976 ms | **0,2581 ms** (min 0,2580 / max 0,2685) | **ATTEINTE** | 499999500000 ✓ |

Gain **×5,01 / ×5,03**. Ratio vs C -O2 : 4,90 → **0,98 / 0,99** (Rose JIT
désormais légèrement plus rapide que C -O2 sur ce programme). Écart au plancher
machine mesuré 0,2564 ms : **+0,6 %**.

### Tableau 9 domaines — before/after (2 salves A/A c19 + 1 c15, 9 échantillons)

« Avant » = addendum 20:05Z de `BENCH_APRES_GOULOT.md` (même binaire
`8383468b…`). « Après » = `bench_final_v21_c19A/B.log`, `bench_final_v21_c15C.log`.
Temps internes Rose, ms.

| Domaine | avant c19 / c15 | final c19a | final c19b | final c15 |
|---|---:|---:|---:|---:|
| D1 fib35 | 11,70 / 11,51 | 11,580 | 11,735 | 11,601 |
| **D2 loop 1e6** | **1,293 / 1,293** | **0,258** | **0,259** | **0,258** |
| D3 primes 50k | 1,477 / 1,481 | 1,481 | 1,481 | 1,480 |
| D4 fact 5000 | 1,386 / 1,386 | 1,416 | 1,424 | 1,421 |
| D4b gcd 1000 ch | 6,733 / 6,722 | 6,724 | 6,734 | 6,740 |
| D5 chaîne 20k | 0,941 / 0,929 | 0,934 | 0,932 | 0,928 |
| D6 liste 100k | 2,146 / 2,155 | 2,170 | 2,160 | 2,157 |
| D7 table 3000 | 1,357 / 1,355 | 1,353 | 1,358 | 1,347 |
| D8 E/S 10 Mo | 5,09 / 4,86 (bruit) | 6,085 | 5,940 | 6,151 (bruit disque) |

**Aucune régression systématique.** D4 est +2,2 % sur la table 20:05Z mais
reste sous les 1,468–1,471 ms mesurés dans les salves 18:08 sur le même binaire :
dérive inter-salves, pas effet du changement (le détecteur ne reconnaît pas
`pour $k de 2 vers 5001`, qui multiplie). D8 reste bruité par les E/S disque
(5–31 ms sur les salves précédentes).

---

## 5. Exactitude (validation obligatoire)

Comparaison **octet pour octet** entre le binaire de départ et le binaire final
(`/tmp/opencode/v21_exact/`, `diff` muet) :

- `exact_consolidee.rose` : 8/8 identiques — fib(35)=9227465, fib(25)=75025,
  somme(1e6)=499999500000, `i64::MAX + 1` = 9223372036854775808,
  `1e10*1e10` = 100000000000000000000,
  `0 - 1e19` = -10000000000000000000,
  `MAX - (-MAX)` = 18446744073709551614,
  grand produit = 999999999999999998000000000000000001.
- `d2_exact_loop.rose` : 6/6 identiques (boxage pendant boucle, débordement
  négatif, retour de slot boxé, débordement différé) — les 8 cas de
  `JIT_BOUCLE_I64.md` sont couverts par ces deux fichiers.
- `d2_exact.rose` : identique.
- Bornes `n = −5, 0, 1, 2, 3, 4, 9, 10, 11, 100, 1000, 65536` : identiques.
- **Vrai dépassement au-dessus de la borne** : `somme(4294967297)` rend
  `9223372039002259456` (bigint boxé) sur les deux binaires → repli vérifié.
- Repli forcé (borne temporairement abaissée à 8) : `n = 9..65536` identiques
  à la base → la boucle générique vérifiée est correcte.
- `factorielle 5000` : 16 327 octets, md5 `0147c25aca0af0c24e9b16f739df3b94`
  identique base/final.
- Grands entiers / négatifs : couverts par `exact_consolidee.rose` ci-dessus.

### Validation complète

- `cargo build --release --bin rose` : OK, copié dans `rose/rose`
  (sha `316fbfc6…`).
- `cargo test --release` : **160/0, 144/0, 0/0, 6/0, 1/0, 0/0**, rc=0.
- `bash scripts/audit-final.sh` : **rc=0** — oracles **152/0 ×4**, point fixe
  `bdbef63681608a0b22a5637539e9d738` ATTEINT ×2, jardin **20/20**, fib(25)=75025,
  bench_global rc=0, `link_natif` 7/0, `natif_emetteur` 6/0, `bin rose` 144/0.
  Log : `/tmp/opencode/v21_audit.log`.
- `git diff rose/src/jit.rs` : `+215 −0`, aucun autre fichier modifié par cette
  mission (les `?? 1` et `?? rose_v3_test.txt` visibles dans `git status`
  préexistaient).

---

## 6. Artefacts

- Dumps : `/tmp/opencode/d2_now.dump` (avant), `/tmp/opencode/d2_varB.dump`,
  `/tmp/opencode/d2_varU2.dump`, `/tmp/opencode/d2_final.dump`.
- IR : `/tmp/opencode/d2_ir_before.txt`, `/tmp/opencode/d2_ir_varB.txt`,
  `/tmp/opencode/d2_ir_final.txt`.
- Mesures : `/tmp/opencode/benchmulti/d2_{base_hot,varB,varU2,final,final2}.log`,
  `bench_final_v21_c19A.log`, `bench_final_v21_c19B.log`, `bench_final_v21_c15C.log`.
- Binaire de départ conservé : `/tmp/opencode/rose_base_hot`.
- Variantes compilées : `/tmp/opencode/rose_varB`, `rose_varU2`, `rose_varU2b`,
  `rose_varU2_limit8` (test repli).
