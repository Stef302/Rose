# JIT — coût du chemin JIT des boucles entières (domaine 2)

**Constat daté : 18/11/2026, 17:41 UTC.** Toutes les mesures ci-dessous sont des
constats à cet horodatage, pas des consignes : re-mesurer après tout changement
de code, de quantification ou de topologie. Pinage `taskset -c 15` **et** `-c 19`,
9 échantillons entrelacés (Rose/C en ordre alterné), médianes. Aucune mesure
concurrente lourde.

- Binaire de départ : `rose/rose` sha256 `6e3ff795456d3f9bcf01c74dcb5a443c65e969c29ee8515b18b6497a5dacf801`
- Binaire final : `rose/rose` sha256 `2c46dd2deca2db92349cfb47b9aa50cc950c0785f8e14ba75ae79483f8e5493c`
- Périmètre : **`rose/src/jit.rs` uniquement** (`+162 −28` lignes). Aucun autre
  `.rs`, aucun `Cargo.toml`, aucun doc d'un autre agent. Aucun commit, aucune
  publication.

---

## 1. Reproduction (étape 1)

Harnais focalisé `/tmp/opencode/benchmulti/d2_focus.py`, 9 échantillons entrelacés
par pin, valeur `499999500000` vérifiée à chaque tirage.

| pin | Rose méd. (base) | C -O2 méd. | ratio | valeur |
|---|---|---|---|---|
| c15 | **9,2835 ms** (min 9,2360 / max 9,3150) | 0,2670 ms | 34,77 | 499999500000 ✓ |
| c19 | **9,2977 ms** (min 9,2620 / max 9,3424) | 0,2613 ms | 35,58 | 499999500000 ✓ |

Le constat de `BENCH_APRES_GOULOT.md` (9,30 ms, ×35,8) est reproduit.

---

## 2. Décomposition par la preuve (étape 2) — le chemin supposé était FAUX

Le contexte de mission pointait `jit.rs:2966-3000` (chemin **AST** `binaire()`,
~20 instructions avec `brif`). **Le dump du code machine prouve que ce n'est pas
ce chemin qui exécute la boucle.**

`ROSE_DUMP_CODE=/tmp/opencode/d2_base.dump rose r_loop.rose` puis
`objdump -D -b binary -m aarch64` :

```
base, corps de somme() (0x44–0xf0) :
  0x44: cmp x19, x27 ; 0x48: b.lt 0x84      ← condition $k < n
  0x8c: ldr x6, 0x94 ; ... ; 0xac: blr x6   ← APPEL rose_add_ent ($s+$k)
  0xc8: ldr x6, 0xd0 ; ... ; 0xe4: blr x6   ← APPEL rose_add_ent ($k+1)
  0xf0: b 0x44
```

**2 appels runtime par itération**, pas 20 instructions inline. La cause est
`expr_eabstrait_rir` (`jit.rs`, bras `Oper::Add|Sub|Mul`, anciennement ~2058-2077)
qui émet `rose_add_ent(va, tag_a, vb, tag_b, p_sv, p_st)` avec 2 stack slots,
pour chaque `+` entier. Le chemin AST décrit dans la mission n'est pas emprunté
ici (`emettre_fonction_rir` est sélectionné dès que le RIR est compilable).

Pourquoi l'appel coûte ~4 ns : `tagged_binop_abi` (`run.rs:1618`) appelle
`mpz_ptr` sur chaque opérande, c.-à-d. **2 recherches dans le `HashSet` thread-local
`MPZ_BOXES`**, avant même le `checked_add`. Les tags transmis ne sont pas fiables
(`run.rs:1541` documente les tags périmés sur `Eabstrait::Slot`) : c'est
l'appartenance au registre des boîtes, et non le tag, qui fait foi.

Décomposition du 9,3 ms/itération (≈9,3 ns) :
- ~8,0 ns : les 2 appels + 2 HashSet lookups + 2 loads de stack (`0xac`, `0xe4`) ;
- ~1,3 ns : condition de boucle, prologue, `mov` de permutation, arithmétique inline.

La part « checks de tag/overflow du chemin AST » est donc **nulle ici** : elle
n'est pas exécutée.

---

## 3. Décision retenue (étape 3)

**Drapeau de non-boxage par slot RIR + addition/soustraction inline avec détection
d'overflow et repli runtime inchangé.**

Preuve de sûreté en trois maillons :

1. `slot_unboxed: Vec<Variable>` (i64, 1 = i64 nu, 0 = « non prouvé »), un par
   slot RIR, maintenu à chaque `Op::Affect`. Un `Eabstrait::Slot` lit **son propre**
   drapeau, jamais le `tag_tmp` partagé et périmé.
2. Le drapeau vaut 1 seulement pour : un littéral i64 (garanti par
   `eabstrait_rir_compilable`, qui exige `parse::<i64>()`), une locale initialisée
   à 0, ou un résultat d'`Add`/`Sub` dont l'overflow vient d'être écarté par le
   test inline. **Les paramètres restent à 0** : `appeler_trampoline` prouve
   l'absence de boxe pour Rust→JIT, mais un `AppelRir` JIT→JIT transmet des i64
   de slot bruts. `AppelRir`, `Neg`, comparaisons, `Mod`, `Div` restent
   conservatifs à 0.
3. Le repli appelle le **même** helper runtime avec des tags à 0 : c'est sûr car
   `tagged_binop_abi` ne se fie pas au tag (test d'appartenance à `MPZ_BOXES`).

Overflow signé (formule vérifiée) :

```
+ : overflow ⇔ ((r^a) & (r^b)) < 0
- : overflow ⇔ ((a^b) & (a^r)) < 0
```

Le repli boite exactement comme avant ; les grands entiers restent exacts. `*` est
resté sur `rose_mul_ent` (promotion bigint non prouvable sans test i128).

Dump final, corps chaud (0x50–0x194) :

```
  0x94..0xc4 : flags + add + test d'overflow + brif
  0x10c..0x114 : chemin RAPIDE (3 mov), aucun appel
  0x118..0x148 : flags + add + test d'overflow + brif (incrément)
  0x188..0x18c : chemin RAPIDE (2 mov)
  0xec, 0x170 : blr x6  ← CHEMINS FROIDS UNIQUEMENT (repli runtime)
```

~35 instructions sur le chemin chaud contre ~44 avec 2 appels. Le gain ne vient
donc **pas** du nombre d'instructions mais de la suppression des deux appels
runtime (2 HashSet lookups).

### Variantes rejetées, avec mesures

| Variante | Résultat mesuré | Décision |
|---|---|---|
| Formule d'overflow `(a^b)&(r^a)` pour `+` | `acc_pos(100)` → `-9223372036854675816` au lieu de `9223372036854875800` (base) | **Rejetée**, corrigée en `(r^a)&(r^b)`. Le même bug existait dans le chemin AST `+`, corrigé aussi. |
| Dériver `tag_tmp` du drapeau sur `Eabstrait::Slot` (0 → tag 1) | `jardins/tests/premiers.rose` **SIGSEGV** après 13/20 tests (`pgcd_jardin(48,18)` rendait `6` avec un tag 1 → le trampoline déréférençait l'adresse 6) ; base 20/20 | **Rejetée**, comportement d'origine restauré. `0` signifie « non prouvé », pas « boxé ». |
| Paramètres marqués non-boxés (preuve trampoline seule) | Non retenu : faux pour les appels JIT→JIT `AppelRir` | **Rejetée** (précaution conservatrice ; D2 mesuré à 1,29 ms après ce changement, sans comparaison A/B isolée de cette seule variante). |
| Inliner `*` avec test i128 | Non implémenté (risque sur D4 factorielle) | **Conservé** sur `rose_mul_ent`. |
| Déferral d'overflow sans preuve de borne | Non implémenté : un débordement réel en cours de boucle rendrait la somme fausse | **Rejeté** sur le plan de la sûreté. |

---

## 4. Résultats (étape 4) — porte atteinte

D2, 9+9 échantillons entrelacés, pinés c15 **et** c19 (harnais final `d2_FINAL2.log`) :

| pin | avant | après | porte < 3,0 ms | valeur |
|---|---|---|---|---|
| c15 | 9,2835 ms | **1,2955 ms** (min 1,2892 / max 1,3395) | **ATTEINTE** | 499999500000 ✓ |
| c19 | 9,2977 ms | **1,2921 ms** (min 1,2897 / max 1,3403) | **ATTEINTE** | 499999500000 ✓ |

Gain **×7,2**. Ratio vs C -O2 : 34,8 → **4,94**. Plancher machine mesuré
0,2564 ms ; émetteur natif M3.1 3,465 ms (`PLAFOND_APRES.md` §2.1) — le JIT
Cranelift est désormais **plus rapide que l'émetteur natif** sur ce programme.

### Tableau 9 domaines — before/after (2 salves A/A c19 + 1 c15, 9 échantillons)

| Domaine | base c19 | base c15 | final c19a | final c19b | final c15 | ratio c19a | ratio c15 |
|---|---|---|---|---|---|---|---|
| D1 fib35 | 11,5699 | 11,7384 | 11,6034 | 11,5274 | 11,6592 | 1,003 | 0,993 |
| **D2 loop 1e6** | **9,2735** | **9,3042** | **1,2914** | **1,2929** | **1,2912** | **0,139** | **0,139** |
| D3 primes 50k | 9,4688 | 9,4414 | 9,4540 | 9,4567 | 9,4719 | 0,998 | 1,003 |
| D4 fact 5000 | 1,4700 | 1,4668 | 1,4388 | 1,4312 | 1,4359 | 0,979 | 0,979 |
| D4b gcd 1000 ch | 6,7107 | 6,7082 | 6,7263 | 6,7179 | 6,7112 | 1,002 | 1,000 |
| D5 chaîne 20k | 3,3308 | 3,3210 | 3,3359 | 3,3366 | 3,3499 | 1,002 | 1,009 |
| D6 liste 100k | 38,7626 | 38,7117 | 38,5976 | 38,6067 | 38,5564 | 0,996 | 0,996 |
| D7 table 3000 | 1,3901 | 1,3849 | 1,3796 | 1,3848 | 1,3803 | 0,992 | 0,997 |
| D8 E/S 10 Mo | 6,1892 | 6,2793 | 6,1971 | 7,7237 | 5,8754 | 1,001 | 0,936 |

(ms, timer interne Rose ; D8 bruité par les E/S disque, 5,1–7,7 ms sur les salves
précédentes, sans rapport avec le JIT.) **Aucune régression systématique** : tous
les autres domaines sont dans le bruit ±1,2 % ; D4 est même légèrement plus rapide.

---

## 5. Exactitude bigint (validation obligatoire)

Sortie commune base/final (`/tmp/opencode/exact_cons_*.txt`, `diff` muet →
**IDENTIQUE**) :

```
9227465                                  # fib(35)
75025                                    # fib(25)
499999500000                             # somme(1000000)
9223372036854775808                      # i64::MAX + 1  → bigint
100000000000000000000                    # 1e10 * 1e10
-10000000000000000000                    # négatif hors i64
18446744073709551614                     # MAX - (-MAX)
999999999999999998000000000000000001     # grand produit
```

`d2_exact_loop.rose` (boxage **pendant** une boucle JIT, débordement négatif,
retour direct d'un slot boxé, débordement différé) → 6/6 identiques à la base.
`factorielle 5000` : md5 des chiffres `6ceab791bd8bf35106485a9d77fea516`
identique base et final.

---

## 6. Validation complète

- `cargo build --release --bin rose` : OK (7,77 s), copié dans `rose/rose`.
- Jardin `20 tests, 0 échec` ×3 (c19, sans pin) — **rc=0**.
- Point fixe `bdbef63681608a0b22a5637539e9d738` ×2 (les deux MD5 égaux à chaque
  passage).
- `bash scripts/audit-final.sh` → `rc=0` : oracles **152/0 ×4** (lexeur,
  analyseur, vérificateur, émetteur), point fixe ATTEINT, jardin 20/20,
  fib(25) conforme, `bench_global` rc=0, `cargo test --lib link_natif` 7/0,
  `natif_emetteur` 6/0, `cargo test --bin rose` 144/0.
  Log : `/tmp/opencode/audit_jit_FINAL.log`.
- `cargo test --release` : `160/0`, `144/0`, `0/0`, `6/0`, `1/0`, `0/0`.
- `git status --short` : `M rose/src/jit.rs` uniquement.

Artefacts : `/tmp/opencode/d2_base.dump`, `/tmp/opencode/d2_final.dump`,
`/tmp/opencode/d2_*.log`, `/tmp/opencode/AA_*.json`, `/tmp/opencode/exact_*`.

---

## 7. Proposition hors périmètre (non implémentée)

Pour réduire encore l'écart résiduel (1,29 ms vs 0,26 ms), il faudrait éliminer
les ~35 instructions du chemin chaud, en particulier les `mov` de permutation
(`PLAFOND_APRES.md` §2.2) et les checks de drapeau. Cela exigerait une nouvelle
ABI (valeur + drapeau dans des registres stables, ou déoptimisation avec reprise
de boucle) touchant `run.rs` (registre des boîtes, trampoline) — **hors de mon
périmètre**, non fait, non mesuré. Le gain actuel (×7,2) est conservé sans
aucune modification hors `jit.rs`.
