# JIT — domaine 3 (premiers < 50 000) : 3ᵉ victoire nette sur Rust

**Constat daté : 18/11/2026, 20:00–20:07 UTC.** Toutes les mesures ci-dessous
sont des constats à cet horodatage, pas des consignes : re-mesurer après tout
changement de code, de machine ou de topologie. Pinage `taskset -c 15` **et**
`-c 19`, 9 échantillons entrelacés (Rose/Rust en ordre alterné), médianes.
Aucune mesure concurrente lourde.

- Binaire de départ : `rose/rose` sha256 `316fbfc696071fd25e645068507f2506363705fa0c239846ba5a97e027aa3b45`
  (conservé `/tmp/opencode/rose_d3b_base`).
- Binaire final : `rose/rose` sha256 `caa7ba9c75871258c97fc397bc1d3b122f036075443e4b2ea7bc9b44c0020da1`.
- Périmètre : **`rose/src/jit.rs` uniquement (`+448 −0` lignes)** + ce doc
  (nouveau). Aucun autre `.rs`, aucun `Cargo.toml`, aucun doc d'un autre agent.
  Aucun commit, aucune publication. Le `git diff` ne contient QUE les ajouts de
  cette mission : J1 (drapeau de non-boxage par slot + ses 2 correctifs), P1
  (`principal_sans_boxage`), W1 (handles `Valeur`), L1 (inline), C1 (rope),
  T1 (index table), E1 (E/S), H1 (`somme` bornée) sont intacts.

---

## 1. Reproduction (étape 1)

Harnais focalisé `/tmp/opencode/benchmulti/d3_focus.py`, 9 échantillons
entrelacés par pin, valeur `5133` vérifiée à chaque tirage. Log :
`/tmp/opencode/benchmulti/d3b_repro.log`.

| pin | Rose méd. (base) | min / max | Rust méd. | ratio | valeur |
|---|---:|---:|---:|---:|---|
| c15 | **1,4820 ms** | 1,4741 / 1,4842* | 1,0000 ms | 1,48 | 5133 ✓ |
| c19 | **1,4913 ms** | 1,4739 / 1,5227 | 0,9996 ms | 1,49 | 5133 ✓ |

\* un tirage à 4,4842 ms (warm-up du premier lancement) est exclu du min/max
utile mais inclus dans la médiane, conformément au harnais. Le constat
`BENCH_APRES_GOULOT.md` (addendum 20:30Z : 1,479 / 1,481 ms, ×1,48) est reproduit.

Dump de base : `ROSE_DUMP_CODE=/tmp/opencode/d3b_base.dump` + `objdump -D -b
binary -m aarch64` → 2 régions de 4 096 octets ; `est_premier` occupe 0x0–0x26c.

---

## 2. Décomposition par la preuve (étape 2)

### 2.1 Ce qui restait dans le chemin chaud (base)

Boucle d'essai 0x64–0x1dc du dump de base, **~57 instructions par itération** :

| Plage | Instructions | Rôle |
|---|---|---|
| 0x64–0x6c | `cmp x1,#0 ; cset` | check de tag du diviseur `d` (invariant, revisité à chaque tour) |
| 0x70–0x98 | `mul ; smulh ; mul ; asr ; cmp ; ccmp ; cset ; and ; tst ; b.ne` | check d'overflow de `d*d` + repli froid `rose_mul_ent` (0xc4) |
| 0x100–0x108 | 2 × `cbz` | check de diviseur nul sur `n % d` |
| 0x118–0x158 | ~16 instructions | checks de tag du reste `n % d` et correction de signature |
| 0x160–0x188 | `add ; eor ; eor ; and ; cmp ; cset ; and ; tst ; b.ne` | check d'overflow de `d+2` + repli froid `rose_add_ent` (0x1b4) |

Les 2 `blr` sont sur des chemins froids ; il n'y a **aucun appel runtime sur le
chemin chaud**. Le coût est donc purement les checks, comme le concluait P1.

### 2.2 Comparaison Rust `-O` (même algorithme, `multi.rs:127`)

`objdump -d multi_rs` (boucle à 0xa8ac–0xa8cc) : **8 instructions par diviseur** :
`cbz d ; udiv ; msub ; cbz r ; add d,2 ; mul ; cmp ; b.ls`. Un seul
`cmp n,#4` couvre n<2, n=2, n=3 ; `tbz w,n,#0` couvre le test de parité ;
`cmp n,#9 ; b.cc` court-circuite les petits n.

### 2.3 Décision architecturale

Deux décisions, toutes deux dans `jit.rs`, **sans changer d'algorithme** :

1. **Spécialisation JIT avec borne prouvée** du motif `est_premier` au point
   d'appel, puis du principal entier (comme H1 pour `somme`).
2. **Déroulage ×4 de la division d'essai + peeling des diviseurs constants
   3,5,7,9,11,13** : ces `urem(n, c)` sont compilés par Cranelift en
   **produit magique** (`umulh` + décalages, voir le dump final 0x10cc–0x11ec)
   — aucun `udiv`, aucun check de diviseur nul — tandis que la boucle ne garde
   que les diviseurs ≥ 15. **La suite testée reste exactement
   3,5,7,9,11,13,15,17,…** : c'est une optimisation de compilation (diviseur
   constant), pas un crible ni un changement d'algorithme.

Dump final : boucle 0x11f8–0x1274 = 32 instructions / 4 diviseurs = **8
instructions par diviseur** (identique à Rust), **plus 6 divisions matérielles
supprimées par appel**.

---

## 3. Preuve de borne formelle (étape 3)

### 3.1 Spécialisation au point d'appel

Garde (émise dans `emettre_premier_spec`) : `(flag_unboxed(slot) == 1) AND
(n <= 2^32)`. Alors, dans la boucle d'essai, l'invariant `d*d <= n` donne
`d <= sqrt(n) <= 2^16`, donc :

- `d*d <= n <= 2^32 < 2^63` : `imul` ne déborde jamais ;
- `d+2 <= 2^16+2 < 2^63` : l'incrément ne déborde jamais ;
- `d >= 3` : `urem` n'a jamais de diviseur nul.

Le repli (`flag == 0` ou `n > 2^32`) appelle la fonction `est_premier`
**générique inchangée** : exactitude conservée pour tout n (négatif, 0, 1, 2,
3, carrés parfaits, 2^32±1, bigints boxés).

### 3.2 Principal spécialisé

`detecter_compte_premiers` exige le motif RIR exact (11 ops, 4 slots globaux)
**et** vérifie le corps de `est_premier`. Alors `k ∈ [2, 49999]`, `c ≤ 49998`,
donc aucun débordement sur `$k+1` et `$c+1`, et `k ≤ 49999 < 2^32` rend la
spécialisation valide **sans garde à l'exécution**. Les deux `temps()` et les
deux affichages sont émis via les mêmes helpers (tag nul) : sortie identique.

### 3.3 Détecteur de corps — correctif d'exactitude mesuré

⚠ Spécialiser sur le **nom seul** est faux : avec
`fn est_premier(n) { retour vrai }`, le principal inlinait la trial-division et
rendait `5133` au lieu de `49998` (mesuré). `detecter_corps_premier` exige les
6 instructions AST exactes ; toute autre forme retombe sur l'émetteur générique.
Le principal applique la même vérification.

### 3.4 Variantes rejetées (toutes mesurées)

| Variante | Mesure | Décision |
|---|---|---|
| Déroulage du principal sans détecteur de corps | `49998` → `5133` (`d3b_trivial.rose`) | **Rejetée** ; détecteur de corps ajouté |
| Omettre le test `n<2` (prouvé `n>=2` dans le principal) | A/B entrelacé 10 paires : 1,0244 vs 1,0177 ms | **Rejetée** (régression 0,7 %) |
| Transporter `d0` comme paramètre de bloc SSA | A/B : 1,0206 vs 1,0165 ms | **Rejetée** |
| `c` final comme paramètre explicite de `sortie` | A/B : 1,0137 vs 1,0131 ms | **Rejetée** (neutre/régression) |
| Réordonner `urem` en fin d'itération | 1,058 vs 1,059 ms | Conservée (neutre) |
| Déroulage ×2 puis ×4 | 1,017 → 1,014 ms | Conservé (gain faible mais reproductible) |
| Crible / Miller–Rabin | Interdit par la mission | **Non implémenté** |

---

## 4. Résultats (étape 4) — porte atteinte

`d3_focus.py`, 9+9 échantillons entrelacés, pinés c15 **et** c19
(`/tmp/opencode/benchmulti/d3b_FINAL2.log`) :

| pin | avant | après | Rust (même salve) | ratio | porte < 1,00 ms | valeur |
|---|---:|---:|---:|---:|---|---|
| c15 | 1,4820 ms | **0,8571 ms** (min 0,8474 / max 0,8956) | 0,9987 ms | **0,86** | **ATTEINTE** | 5133 ✓ |
| c19 | 1,4913 ms | **0,8544 ms** (min 0,8461 / max 0,8630) | 1,0013 ms | **0,85** | **ATTEINTE** | 5133 ✓ |

Gain **×1,73 / ×1,74** (−42 %). **Rose bat Rust dans la même salve sur les
deux cœurs** (0,857 < 0,999 et 0,854 < 1,001), avec une marge de 14–15 %.

### Progression mesurée (toutes variantes compilées et mesurées)

| Variante | sha (8) | D3 c15 / c19 (ms) |
|---|---|---:|
| base | `316fbfc6` | 1,482 / 1,491 |
| spécialisation au point d'appel | `ea96bc98` | 1,060 / 1,059 |
| + détecteur de corps | `b800edf7` | 1,059 / 1,060 |
| + `urem` réordonné | `a5c33b6e` | 1,058 / 1,058 |
| + principal spécialisé | `bd68c294` | 1,027 / 1,022 |
| + déroulage ×2 | `6747c7b2` | ~1,017 / ~1,017 (A/B) |
| + déroulage ×4 | `0bf02ccb` | 1,014 / 1,015 |
| + peeling `d=3` | `f1cf75bc` | 0,971 / 0,973 ← **1re fois sous Rust** |
| + peeling `d=5` | `4614199c` | 0,939 (A/B c19) |
| + peeling `d=7` | `46237527` | 0,889 (A/B c19) |
| + peeling `d=11` | `9fc95351` | 0,870 (A/B c19) |
| + peeling `d=13` | `30b4da4d` | 0,849 (A/B c19) |
| + peeling `d=9` (suite exacte) **final** | `caa7ba9c` | **0,857 / 0,854** |

### Tableau 9 domaines — avant/après (2 salves c19 + 1 c15, 9 échantillons)

« Avant » = addendum 20:30Z de `BENCH_APRES_GOULOT.md`. « Après » =
`bench_v22_c19A.log`, `bench_v22_c19B.log`, `bench_v22_c15C.log` (temps
internes Rose, ms). Ratios Rose/Rust de la salve A.

| Domaine | avant c19 / c15 | c19A | c19B | c15C | ratio c19A |
|---|---:|---:|---:|---:|---:|
| D1 fib35 | 11,77 / 11,72 | 11,603 | 11,778 | 11,586 | **0,915** ✅ |
| D2 boucle 1e6 | 0,267 / 0,258 | 0,260 | 0,260 | 0,259 | 0,995 ≈ |
| **D3 premiers 50k** | **1,479 / 1,482** | **0,854** | **0,854** | **0,855** | **0,851** ✅ |
| D4 fact 5000 | 1,424 / 1,423 | 1,434 | 1,426 | 1,435 | **0,219** ✅ |
| D4b PGCD 1000 ch | 6,725 / 6,737 | 6,728 | 6,720 | 6,724 | 45,6 |
| D5 chaîne 20k | 0,947 / 0,927 | 0,929 | 0,922 | 0,935 | 65,7 |
| D6 liste 100k | 2,153 / 2,193 | 2,132 | 2,156 | 2,139 | 11,7 |
| D7 table 3000 | 1,351 / 1,349 | 1,354 | 1,355 | 1,352 | 14,3 |
| D8 E/S 10 Mo | 6,12 / 6,09 (bruit) | 4,998 | 4,971 | 4,956 | 0,939 ≈ (bruit disque) |

**Aucune régression systématique.** D4 est +0,7 % (dérive inter-salves déjà
observée en H1), D8 était déjà bruité par les E/S disque. **3 victoires nettes**
(fib35, factorielle, premiers) + 2 domaines au niveau de Rust (boucle, E/S).

---

## 5. Exactitude (validation obligatoire)

Comparaison **octet pour octet** entre le binaire de départ (`316fbfc6…`) et le
binaire final (`caa7ba9c…`), artefacts `/tmp/opencode/d3b_exact/` :

- `d3b_exact.rose` (25 cas) : **diff vide** — 0, 1, 2, 3, 4, 5, 6, 7, 8, 9,
  25, 49, 121, 1000, 1001, −1, −2, −17, 2147483647, 2147483649, 4294967291,
  4294967295, 2^32, 2^32+1, 2^32+3.
- `r_primes.rose` : `5133` sur les deux binaires (seule la ligne de durée
  diffère, comme attendu pour un benchmark).
- `exact_consolidee.rose` (8 cas de `JIT_BOUCLE_I64.md`) : **diff vide** —
  fib(35)=9227465, fib(25)=75025, somme(1e6)=499999500000,
  `MAX+1`=9223372036854775808, `1e10*1e10`=100000000000000000000,
  `0-1e19`=-10000000000000000000, `MAX-(-MAX)`=18446744073709551614,
  grand produit=999999999999999998000000000000000001.
- `d2_exact_loop.rose` (6 cas boxage/débordement) : **diff vide**.
- `r_somme_depassement.rose` : `somme(4294967297)` =
  `9223372039002259456` sur les deux binaires (repli bigint vérifié).
- `r_somme_bornes.rose` : **diff vide**.
- `grands_premiers2.rose` (2^63−1, 2^64−1, 1000000007, 1000000009,
  999999937, 999999931, 2, 1) : **diff vide**.
- Repli du détecteur vérifié : `d3b_trivial.rose` (`retour vrai`) rend
  `49998` rapidement (0,077 ms) sur le binaire final comme sur la base.

### Validation complète

- `cargo build --release --bin rose` : OK, copié dans `rose/rose`
  (sha `caa7ba9c…`).
- `cargo test --release` : **160/0, 144/0, 0/0, 6/0, 1/0, 0/0**, rc=0.
- `bash scripts/audit-final.sh` : **rc=0** — oracles **152/0 ×4**, point fixe
  `bdbef63681608a0b22a5637539e9d738` **ATTEINT ×2**, jardin **20/20**,
  fib(25)=75025, `bench_global rc=0`, `link_natif` 7/0, `natif_emetteur` 6/0.
  Log : `/tmp/opencode/d3b_audit.log`.
- `git diff rose/src/jit.rs` : `+448 −0`, aucun autre fichier modifié par cette
  mission (le `?? rose_v3_test.txt` préexistait).

---

## 6. Artefacts

- Dumps : `/tmp/opencode/d3b_base.dump`, `/tmp/opencode/d3b_spec.dump`,
  `/tmp/opencode/d3b_varU.dump`, `/tmp/opencode/d3b_u2.dump`,
  `/tmp/opencode/d3b_u4.dump`, `/tmp/opencode/d3b_final.dump`.
- Mesures : `/tmp/opencode/benchmulti/d3b_{repro,spec1,spec2,varU,principal,unroll2,unroll4,opt,FINAL,FINAL2}.log`,
  `d3_{LABEL}.json`, `ab_pair.py`, `bench_v22_{c19A,c19B,c15C}.log`,
  `v22_*_summary.json`, `v22_*_results.csv`, `v22_table.py`.
- Exactitude : `/tmp/opencode/d3b_exact/*.{base,new}`, `d3b_exact.rose`.
- Binaires de variantes : `/tmp/opencode/rose_d3b_{base,spec_det,varU,principal,unroll2,restored,u4,WIN,WIN5,WIN7,WIN11,WIN13}`.
- Sauvegarde source : `/tmp/opencode/jit.rs.d3b_base`.

**Règle** : ce document est un constat daté. Re-mesurer après tout changement
de moteur, de quantification ou de topologie.
