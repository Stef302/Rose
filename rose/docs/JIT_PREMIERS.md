# JIT — coût du domaine 3 (premiers < 50 000) : déplacement de la boucle dans le JIT

**Constat daté : 18/11/2026, 18:34 UTC.** Toutes les mesures ci-dessous sont des
constats à cet horodatage, pas des consignes : re-mesurer après tout changement
de code, de quantification ou de topologie. Pinage `taskset -c 15` **et**
`-c 19`, 9 échantillons entrelacés (Rose/Rust en ordre alterné), médianes.
Aucune mesure concurrente lourde.

- Binaire de départ : `rose/rose` sha256 `2c46dd2deca2db92349cfb47b9aa50cc950c0785f8e14ba75ae79483f8e5493c`
- Binaire final : `rose/rose` sha256 `f9ae30b379e6689db912cbbc60cebdb74c9d8ef8a1343cf19e32f9c51d5f5871`
- Périmètre : **`rose/src/jit.rs` uniquement** (`+94 −3` lignes) + ce document.
  Aucun autre `.rs`, aucun `Cargo.toml`, aucun doc d'un autre agent. Aucun
  commit, aucune publication.

---

## 1. Reproduction (étape 1)

Harnais focalisé `/tmp/opencode/benchmulti/d3_focus.py`, 9 échantillons entrelacés
par pin, valeur `5133` vérifiée à chaque tirage.

| pin | Rose méd. (base) | min / max | Rust méd. | ratio | valeur |
|---|---:|---:|---:|---:|---|
| c15 | **9,4283 ms** | 9,4000 / 9,5169 | 0,9975 ms | 9,45 | 5133 ✓ |
| c19 | **9,4535 ms** | 9,4012 / 9,5490 | 1,0010 ms | 9,44 | 5133 ✓ |

Le constat de `BENCH_APRES_GOULOT.md` (9,51 / 9,46 ms, ×9,49) est reproduit.

---

## 2. Décomposition par la preuve (étape 2) — la prémisse était FAUSSE

La mission supposait le coût dominant dans les appels runtime (`rose_mul_ent`,
`rose_mod_ent`) et dans l'appel de `est_premier`. **Le dump du code machine et
les micro-programmes prouvent le contraire.**

### 2.1 Dump du chemin chaud (base)

`ROSE_DUMP_CODE=/tmp/opencode/d3_base.dump rose r_primes.rose` puis
`objdump -D -b binary -m aarch64` : **une seule région de 4 096 octets**, celle
de `est_premier`. Le programme principal n'est PAS compilé :

```
TIMING parse=49,36µs ... compiletout=371,375µs compileprincipal=832ns
TIMING-JIT eligibles=["est_premier"] ... memoire finalize=... regions=1
TIMING exec=9,412574ms total=9,853069ms
```

Dans `est_premier` (0x0–0x26c), les deux `blr` sont sur des **chemins froids** :
- `0xc4` : repli `rose_mul_ent` si le test d'overflow `mul`/`smulh` (0x74–0x94) échoue ;
- `0x1b4` : repli `rose_add_ent` si le test d'overflow de `$d+2` échoue ;
- `0x260` : diviseur zéro.

Le chemin chaud ne contient **aucun appel runtime** : `sdiv`/`msub` inline pour
`n % $d`, `imul` + `smulh` inline pour `$d*$d`, additions inline avec test
d'overflow. `PROFIL-JIT appels=0` confirme que le trampoline de `appel()`
(interp.rs:1102) n'est même pas emprunté : l'appel vient de `appel_rir_direct`
(interp.rs:2125).

### 2.2 Mesures de décomposition (7 échantillons, pin c15, médianes)

| Programme | médiane | ns/itération | ce que cela isole |
|---|---:|---:|---|
| `r_primes.rose` (réel) | 9,4489 ms | 189 | boucle interprétée + trampoline + JIT |
| `d3_looponly.rose` (boucle sans appel) | 5,4097 ms | 108 | **interprétation de la boucle + synchro slots↔globales** |
| `d3_tramp.rose` (boucle + `trivial()` JIT) | 9,8620 ms | 197 | + **89 ns/appel de trampoline** (9,862 − 5,410) |
| `d3_jittrivial.rose` (boucle JIT + `trivial()`) | 0,0722 ms | 1,4 | appel JIT→JIT direct : quasi gratuit |
| `d3_jitloop.rose` (boucle JIT + `est_premier`) | 1,4849 ms | 29,7 | **code natif de `est_premier` seul** |

**Conclusion mesurée :** le coût est à **57 % dans l'interprétation de la
boucle du principal** (5,4 ms) et à **~47 % dans le trampoline** (4,45 ms). Le
code natif de `est_premier` ne coûte que 1,49 ms. Les pistes (a) « inliner `*` »
et (b) « inliner l'appel JIT→JIT » ne peuvent donc pas atteindre la porte :
elles ciblent respectivement un appel déjà inline et un appel déjà direct.

---

## 3. Décision retenue (étape 3)

**Faire compiler le corps principal en JIT**, en étendant le sous-ensemble RIR
aux deux opérations d'E/S sans état qui l'en empêchaient, avec une garde
conservatrice sur le boxage :

1. `rir_compilable_jit` accepte `Op::AffectAst { e: temps() }` et
   `Op::AfficherAbstrait(e)` ; tous les autres ops hors sous-ensemble restent
   refusés (repli interpréteur inchangé).
2. L'émetteur RIR pose `temps()` via un helper `rose_temps` — le MÊME
   `crate::run::temps_unix_nanos()` que `interp.rs:4208` — et les affichages via
   `rose_affiche_ent/bool/f64`, **sans préfixe LaTeX**, exactement comme
   `HoteRir::afficher_valeur` dans `rir::executer_avec_ptr_sync`.
3. Garde `principal_sans_boxage` : le principal n'est compilé que si aucune
   `Op::Affect` ne peut poser un BigInt (`*`, `/`, `^`, `div`, `-` unaire, appel
   rendant un entier). Les fonctions déjà compilées (`est_premier`, `fib`,
   `somme`…) gardent leur chemin historique : `compiler_tout` passe `false`.

Dump final : **2 régions**. Le principal natif (0x1000–0x1250) contient une
seule `blr` dans la boucle (`0x1158` = appel direct à `est_premier`, constante
`0x99cf1000`) plus les comparaisons et incréments inline. `rose_temps` est
appelé deux fois **hors boucle** (`0x1030`, `0x1080`, constante `0x998c349c`),
les deux `rose_affiche_ent` aussi (`0x109c`, `0x1120`, constante
`0x998c2d80`) ; `0x10f0`, `0x11c0` et `0x1244` sont les replis froids de
`rose_add_ent`.

### Variantes rejetées, avec mesures

| Variante | Mesure | Décision |
|---|---|---|
| (a) inliner `*` (checked_mul + promotion) | Déjà présent dans le JIT : dump base 0x74–0x94 = `mul`+`smulh`+`ccmp`, aucun `rose_mul_ent` sur le chemin chaud | **Sans objet** : la prémisse était fausse, J1 l'avait déjà fait |
| (b) inliner JIT→JIT `est_premier` | Déjà présent : `AppelRir` → `FuncRef` direct. `d3_jitloop` = 1,486 ms | **Sans objet** : 1,49 ms sur 9,45 ms |
| Extension SANS garde (1re version) | `fact5000_chiffres.rose` affichait `281022126188032` (pointeur Mpz formaté en i64) au lieu de 16 326 chiffres ; md5 `3096a1444b12e82d3c29abc961ba38be` ≠ base `0147c25aca0af0c24e9b16f739df3b94` | **Rejetée** |
| Suivre le tag réel des slots (`slot_tags` parallèle à `slot_unboxed`) | md5 `b2b21c50268d8e42b417c46cc38cfaca` ≠ base `0147c25a…` : la cause n'était pas le tag lu mais le `*`, qui boxe sans que le principal sache le propager | **Rejetée** (retrait du code), remplacée par la garde |
| Compiler le principal avec `Op::Afficher(Vec<Expr>)` (formule LaTeX) | Non retenu : le préfixe LaTeX n'est pas émis par `AfficherAbstrait` du RIR ; risque de casser la parité des sorties | **Rejetée** (précaution) |

---

## 4. Résultats (étape 4) — porte atteinte

D3, 9+9 échantillons entrelacés, pinés c15 **et** c19 (harnais final `d3_FINAL.log`) :

| pin | avant | après | porte < 3,0 ms | ratio vs Rust |
|---|---:|---:|---|---:|
| c15 | 9,4283 ms | **1,4803 ms** (min 1,4738 / max 1,5219) | **ATTEINTE** | 1,48 |
| c19 | 9,4535 ms | **1,4757 ms** (min 1,4730 / max 1,4998) | **ATTEINTE** | 1,48 |

Gain **×6,4** (−84,4 %). Le résidu 1,48 ms est exactement le prix du code
natif de `est_premier` mesuré isolément (1,486 ms) : le plancher de ce
programme est désormais l'algorithme lui-même, plus le trampoline d'appel
unique du principal.

### Tableau 9 domaines — before/after (base c19+c15, final 2×c19 + c15, 9 échantillons)

| Domaine | base c19 | base c15 | final c19a | final c19b | final c15 | ratio c19a | ratio c15 |
|---|---:|---:|---:|---:|---:|---:|---:|
| D1 fib35 | 11,7591 | 11,6026 | 11,6019 | 11,5894 | 11,6256 | 0,987 | 1,002 |
| D2 loop 1e6 | 1,2908 | 1,2933 | 1,2939 | 1,2938 | 1,2934 | 1,002 | 1,000 |
| **D3 primes 50k** | **9,4640** | **9,4486** | **1,4815** | **1,4837** | **1,4765** | **0,157** | **0,156** |
| D4 fact 5000 | 1,4263 | 1,4318 | 1,4427 | 1,4458 | 1,4476 | 1,011 | 1,011 |
| D4b gcd 1000 ch | 6,7158 | 6,7262 | 6,7343 | 6,7456 | 6,7480 | 1,003 | 1,003 |
| D5 chaîne 20k | 3,3276 | 3,3220 | 3,3390 | 3,3454 | 3,3647 | 1,003 | 1,013 |
| D6 liste 100k | 38,5681 | 38,5912 | 38,7052 | 38,7154 | 38,7572 | 1,004 | 1,004 |
| D7 table 3000 | 1,3847 | 1,3738 | 1,3798 | 1,3846 | 1,3817 | 0,996 | 1,006 |
| D8 E/S 10 Mo | 5,8107 | 6,0553 | 6,0674 | 5,9384 | 6,0278 | 1,044 | 0,995 |

Aucune régression significative : les écarts hors D3 sont ≤ 1,4 % et dans le
bruit de mesure (D8 a une dispersion interne de 4 % entre les deux salves
c19). Les fichiers `jitp_*.json/log/csv` contiennent les données brutes.

---

## 5. Exactitude (sorties réelles)

- Comparaison des **16 programmes** (`d3_compare_out.py`) base vs final, sortie
  complète avec temps normalisé : **0 différence / 16**, dont `5133`,
  `9227465`, `499999500000`, `16326`, `500`, `20000`, `4999950000`, `3000`,
  `10485760`.
- Comparaison des **152 fichiers .rose** du corpus (`corpus_compare.py`) :
  toutes les différences observées (9) sont des **durées** (`millisecondes()`
  / `temps()`), jamais des résultats — vérifié par ré-exécution 3× sur le même
  binaire (ex. `sumsquares` : base 1862/1860/1868, final 1890/1881/1897).
- `exact_consolidee.rose` (8 cas de débordement) : md5 identique
  `be8ec0bd7c94519fb2ebe4e12b81c93c` base et final.
  `d2_exact_loop.rose` (boxage pendant une boucle, débordements) : md5 identique
  `fb6dacf0f54be7f9b941ec62fb3bfd8e`.
- `factorielle 5000` : md5 des chiffres **identique** base/final
  `0147c25aca0af0c24e9b16f739df3b94` (garde conservatrice vérifiée).
- Grands entiers (`grands_premiers2.rose`) : sortie **identique** base et final
  `faux, faux, vrai, vrai, vrai, faux, vrai, faux` pour
  `9223372036854775807` (2⁶³−1), `18446744073709551615` (2⁶⁴−1),
  `1000000007`, `1000000009`, `999999937`, `999999931`, `2`, `1`.
  Un test sur 2⁶¹−1 (vrai premier) a été tenté mais dépasse 300 s en
  trial-division sur les deux binaires : ce n'est pas un écart de sortie.
- Jardin `jardins/tests/premiers.rose` : **20 tests, 0 échec**.
- `fib(25)` = `75025`.

---

## 6. Validation complète

- `cargo build --release --bin rose` : OK (7,84 s), copié dans `rose/rose`.
- `bash scripts/audit-final.sh` → `rc=0` : oracles **152/0 ×4**, point fixe
  `bdbef63681608a0b22a5637539e9d738` ×2, jardin 20/20, fib(25) conforme,
  `bench_global rc=0`, « Portes NON atteintes constatées : AUCUNE ».
  Log : `/tmp/opencode/d3_audit_FINAL.log`.
- `cargo test --release` : `160/0`, `144/0`, `6/0`, `1/0`, `0/0`, `0/0`.
- `git status --short` : `M rose/src/jit.rs` (ce document est le second fichier
  du périmètre). Un fichier non suivi d'un autre agent (`rose_v3_test.txt`)
  est présent et **non modifié**.

Artefacts : `/tmp/opencode/d3_{base,final}.dump`, `/tmp/opencode/d3_*.log`,
`/tmp/opencode/d3_*.json`, `/tmp/opencode/benchmulti/jitp_*.{log,json,csv}`,
`/tmp/opencode/corpus_compare.log`, `/tmp/opencode/jit.rs.{base,JITPRINC}`.

---

## 7. Piste résiduelle (hors périmètre, non implémentée)

Passer de 1,48 ms à ~0,07 ms demanderait d'éliminer le trial-division de
`est_premier` (crible ou test de Miller–Rabin déterministe), c'est-à-dire un
changement d'algorithme dans le **programme** — hors du périmètre `jit.rs`.
Le JIT a désormais fait sa part : 30 ns par appel, contre 190 ns avant.
