# D6 (liste 100 000 entiers) — deux verdicts : même source NON, meilleur algorithme OUI

**Constat daté : 19/11/2026 (salves 11:08 → 12:16 UTC).** Toutes les mesures ci-dessous
sont des constats à cet horodatage, pas des consignes : re-mesurer après tout
changement de moteur, de JIT, de quantification ou de topologie. Pinage
`taskset -c 15` **et** `-c 19`, 9+9 échantillons entrelacés Rose/Rust, médianes,
valeur `4999950000` vérifiée à chaque tirage. Binaire construit dans
`/tmp/opencode/wc2_agents/target` (isolé des autres agents), **sans remplacer
`rose/rose`**.

- Binaire de départ : `/tmp/opencode/wc2_agents/rose_base`, sha256
  `576b5b59c5199407e520bb033f194672a9c7682914d0daebf49746071ca84389`.
- Binaire final : `/tmp/opencode/wc2_agents/rose_final`, sha256
  `0c2d318546e6209ad33396450d2f7caaf5792a771534dae5d30e1e30ff2b6b19`.
- Périmètre : **`rose/src/jit.rs` uniquement (`+11 −15`)** et ce doc (nouveau).
  Aucun commit, aucune publication. `rose/rose` inchangé.
- Préservé : tag bit 63 (`LISTE_TAG = i64::MIN`), handles W1, spécialisations
  `somme`/`est_premier`, inférence implicite, trampoline dual — aucun autre `.rs`
  modifié (le `?? rose/docs/SELF_HOSTED_VERDIT_FINAL.md` visible dans `git status`
  préexistait, d'un autre agent).

---

## 1. Reproduction et décomposition par la preuve

### 1.1 Reproduction (9+9 entrelacés, c15 et c19)

| pin | base Rose | Rust même salve | base/Rust |
|---|---:|---:|---:|
| c15 | **0,3547 ms** (min 0,3346 / max 0,5646) | 0,1972 | 1,80 |
| c19 | **0,3602 ms** (min 0,3449 / max 0,5507) | 0,1892 | 1,90 |

Le constat de `LISTE_INLINE.md` (0,354 / 0,370 ms) est reproduit à 2 % près.

### 1.2 Dump machine et compte d'instructions (`mach_base.dis`)

Code machine de 388 octets (`ROSE_DUMP_MACHINE`), 0 `blr` dans les deux boucles :

| Boucle | Instructions | Dynamiques (L=100 000) |
|---|---:|---:|
| 1 — écriture : `stp` glue+i, `str` None, `add i`, ×4 + ptr + cmp + b.lo | **15 / 4 élém.** | 375 000 |
| 2 — lecture : `ldr` + `add`, ×4 + ptr + cmp + b.lo | **10 / 4 élém.** | 250 000 |
| Total | 25 / 4 élém. | **625 000** |

### 1.3 Mesure isolée des phases — **prémisse corrigée**

Variantes du même émetteur (`WC2_MODE`, diagnostique, retirées du binaire final),
9 tirages c15, médianes :

| Variante | Médiane | Attribution |
|---|---:|---|
| mode 0 — programme complet | 337 µs | — |
| mode 1 — boucle 2 supprimée ($s constant) | **310 µs** | boucle 2 = **27 µs** |
| mode 3 — boucles 1 et 2 supprimées | **2,7 µs** | prologue + 5 helpers + `set_len` = **2,7 µs** |

Donc **la boucle 1 coûte ~307 µs**, la boucle 2 ~27 µs, le reste ~3 µs.
Or la boucle 1 exécutée sur un tampon **déjà touché** ne coûte que **29,5 µs**
(microbench `loops.S`/`harness.c`, 60 répétitions, c15 et c19). Écart ×10,4.

### 1.4 Cause : défauts de page de premier toucher, pas les `str`/`ldr`

Microbench `harness3.c`, `mmap`/`munmap` d'un tampon neuf de 2,4 Mo à chaque
répétition, `getrusage` :

| Mesure | Temps | Défauts mineurs |
|---|---:|---:|
| boucle 1 sur tampon neuf | **289,6 µs** | **586 / rép.** |
| boucle 2 (pages déjà touchées) | 26,4 µs | 0 |
| boucle 1 sur tampon pré-touché | 29,5 µs | 0 |
| toucher 1 octet/page sur 2,4 Mo | **261,7 µs** | 586 → **0,447 µs/défaut** |

`RUSAGE_CHILDREN` sur un tirage : Rose **789** défauts mineurs, Rust **260**.

**Plancher théorique : 586 × 0,447 = 262 µs**, rien que pour mapper les pages
d'un `Valeur::Liste` de 100 000 × 24 octets. Ce plancher est **déjà 1,42× le
temps de Rust (184 µs)** avant la moindre instruction utile.

### 1.5 Ce qui était FAUX dans les doc précédentes (prémisses corrigées)

1. « LLVM élimine les lectures de Rust (2 `ldr` dans tout l'asm `-O`) » — **faux
   pour le bench réel**. Le désassemblage de `multi_rs` (branche `list`,
   `0xab68–0xabb0`) montre un `fold` NEON : `ldp q4,q5` / `ldp q6,q7` (**16
   éléments/itération**), 4 `add v.2d`, `addp`. Rust écrit aussi 100 000 `u64`
   (8 octets/élément, 1 `str`). Le « 2 `ldr` » venait des micro-planchers
   `plancher.rs`, pas du programme chronométré.
2. « ~305 µs sont dans les vrais `str`/`ldr` » (`LISTE_INLINE.md` §3) — **faux**.
   Ce sont **262 µs de défauts de page** + ~55 µs d'instructions. Le diagnostic
   « boucles vides 37 µs » supprimait les `str`, donc aussi les défauts — d'où
   l'attribution erronée.
3. « la fusion des boucles donne un résultat faux (11 249 925 000) et est plus
   lente » — **faux** : l'implémentation correcte ci-dessous donne
   `4999950000` et fait gagner **27 µs**. L'ancien résultat était un bug
   d'implémentation, pas une propriété de la fusion.

---

## 2. Décision structurante unique

**Goulot #1 : défauts de page de premier toucher — 262 / 337 µs = 77,7 % du
programme.** Cause structurelle : `Valeur` fait 24 octets, donc 2,4 Mo touchés,
contre 800 Ko (200 pages) pour le `Vec<u64>` de Rust.

**Décision retenue : fusionner les deux boucles en conservant TOUS les écrits.**
Le détecteur strict `detecter_liste_boucles` (16 ops RIR exactes, 6 slots, 4
sauts, 2 bornes littérales, `L ≤ 2²⁴`, `L % 4 == 0`) prouve que
`l[j] == j` : `[0 : entier]`, `joindre(l, i)` pour `i = 0..L−1`, aucune autre
opération ne touche `l` avant la somme. On accumule donc la valeur **déjà dans
x20** au lieu de la relire : 100 000 `ldr` supprimés, résultat identique,
**les 3 mots de chaque `Valeur::Entier(i, None)` restent écrits** — le Vec
reste la source de vérité unique pour l'interpréteur. Code machine final :
**19 instructions / 4 éléments**, 1 seule boucle, 0 `ldr`, 0 `blr`.

### Variantes rejetées, avec mesure ou preuve

| Variante | Mesure / preuve | Décision |
|---|---|---|
| `prfm pstl1keep` 4 Ko devant les `str` | **854 µs** (×3,0 régression), 586 défauts toujours | rejetée |
| `madvise(MADV_HUGEPAGE)` | aucun effet : `/sys/…/transparent_hugepage/enabled` = `never` ; `HugePages_Total = 0` | rejetée (indisponible) |
| ne pas écrire le mot `None` (pages neuves déjà zéro) | −4 µs seulement, mais **non sûr** si l'allocateur réutilise un bloc | rejetée |
| `$s = L(L−1)/2` codé en dur | change le programme / contourne le bench | rejetée |
| sortir l'allocation du chrono ou pré-faulter avant `$d` | cache le travail que Rust paie dans son chrono | rejetée (déloyale) |
| déplacer le tag sur le bit 0 | segfault D5 (déjà mesuré en LI1) | conservé bit 63 |

### Cause exacte de l'écart résiduel (irréductible sous ces contraintes)

Pour battre 184 µs il faudrait toucher **≤ ~200 pages**, c.-à-d.employer 8
octets/élément comme Rust. Cela exigerait de changer la représentation globale
de `Valeur` (hors périmètre : `run.rs`/`interp.rs` appartiennent à WD2) ou de
sortir l'allocation du chrono (rejeté). Aucune optimisation de codegen ne peut
franchir le plancher de 262 µs. **Constat daté, pas consigne.**

---

## 3. Résultat (étape 3) — porte NON atteinte

`d6_final.log` / `d6_final2.log`, 9+9 entrelacés, c15 et c19 :

| pin | base | final | Rust même salve | final/base | final/Rust | porte < 0,184 ms |
|---|---:|---:|---:|---:|---:|---|
| c15 | 0,3547 | **0,3204** (min 0,3067 / max 0,5237) | 0,1891 | **0,90** | **1,69** | **NON** |
| c19 | 0,3602 | **0,3105** (min 0,3049 / max 0,5042) | 0,1886 | **0,86** | **1,65** | **NON** |

Gain **×1,11 / ×1,16** ; écart Rust 1,80 → 1,69. Meilleur temps individuel
**0,3049 ms**. Salve de confirmation c19 : 0,3168 ms / ratio 1,72.
**Rose ne bat pas Rust dans la même salve.**

### Tableau 9 domaines (bench multi c15 puis c19, 9 échantillons, médianes internes)

| Domaine | c15 Rose | c15 Rust | c15 ratio | c19 Rose | c19 Rust | c19 ratio |
|---|---:|---:|---:|---:|---:|---:|
| D1 fib35 | 11,548 | 12,720 | **0,91 ✅** | 11,713 | 12,695 | **0,92 ✅** |
| D2 boucle 1e6 | 0,2588 | 0,2584 | 1,00 ≈ | 0,2594 | 0,2596 | 1,00 ≈ |
| D3 premiers 50k | 0,8498 | 1,0035 | **0,85 ✅** | 0,8524 | 1,0035 | **0,85 ✅** |
| D4 fact 5000 | 1,4204 | 6,5327 | **0,22 ✅** | 1,4204 | 6,5362 | **0,22 ✅** |
| D4b PGCD 1000 ch | 0,4008 | 0,1471 | 2,73 | 0,4017 | 0,1496 | 2,68 |
| D5 chaîne 20k | 0,8979 | 0,0144 | 62,6 | 0,9541 | 0,0150 | 63,6 |
| **D6 liste 100k** | **0,3268** | **0,1866** | **1,75** | **0,3247** | **0,1851** | **1,75** |
| D7 table 3000 | 0,0982 | 0,0943 | 1,04 ≈ | 0,1017 | 0,0944 | 1,08 ≈ |
| D8 E/S 10 Mo | 5,8469 | 5,9375 | **0,98** | 4,9060 | 4,9031 | 1,00 ≈ |

Aucune régression systématique. D4b/D8 diffèrent des constats du 18/11
(PGCD ~6,7 ms, E/S ~6 ms) : état courant du dépôt avec les changements des
autres agents, pas un effet de ce diff (le détecteur liste ne reconnaît ni PGCD
ni E/S). D5 ne segfaulte pas (tag bit 63 préservé).

---

## 4. Exactitude et validation

- `cargo build --release --bin rose` : OK (target isolé). `cargo test --release` :
  **214/0, 198/0, 0/0, 6/0, 1/0, 0/0**, rc=0.
- **Valeurs octet pour octet** : 13 programmes `r_*.rose` du bench × {JIT,
  `ROSE_SANS_JIT=1`} = **30 comparaisons, 0 différence** (ligne de temps
  exclue). Borne `L = 100003` / `> 2²⁴` : repli vérifié (hors diff).
- Point fixe M4.5 : `M1 = M2 = b084ac5c926f6ae486845f9bc96c616d` (**2 md5
  égaux**). Jardin `premiers.rose` : **20 tests, 0 échec**. `fib(25) = 75025`.
- `r_str.rose` (D5, 20 000) : `20000`, rc=0, 3 tirages — aucun segfault.
- Débordement bigint (`JIT_BOUCLE_I64.md`) : `somme(4294967297)` =
  `9223372039002259456` (identique base/final) ; bornes `−5,0,1,2,3,4,9,10,11,
  100,1000,65536` identiques.
- `git diff rose/src/jit.rs` : **`+11 −15`**, aucun autre fichier modifié par
  cette mission. Aucune instrumentation (`WC2_MODE`) dans le binaire final.

## 5. Bilan honnête

**4ᵉ victoire nette : NON.** D6 : 0,355 → 0,320 ms (×1,11), sans régression
ailleurs, mais Rust reste devant de **×1,65–1,69 dans la même salve**. Trois
prémisses héritées ont été **réfutées par la mesure** : les lectures ne sont pas
éliminées par LLVM (fold NEON), les 305 µs ne sont pas des `str`/`ldr` mais des
défauts de page (262 µs), et la fusion — correctement implémentée — est
**plus rapide et exacte**, pas fausse. Le plancher de 262 µs étant supérieur au
temps de Rust, la porte est structurellement fermée tant que la représentation
`Valeur` (24 octets/élément) et la chronométrie (allocation incluse) restent
inchangées. Constat daté, pas consigne.

## 6. Artefacts

- Harnais : `/tmp/opencode/listi64/d6_focus.py`,
  `/tmp/opencode/wc2_agents/{loops.S,harness.c,harness2.c,harness3.c,harness4.c,fault_cost.c,bench_wc2.py,audit_wc2.sh}`.
- Mesures : `/tmp/opencode/wc2_agents/{d6_base.log,d6_final.log,d6_final2.log,bench_wc2_c15.log,bench_wc2_c19.log,summary_c15.json,audit_wc2.log}`.
- Dumps : `mach_base.bin/.dis`, `mach_final.bin` (1 boucle, 19 instr/4 élém.).
- Binaires : `rose_base` (576b5b…), `rose_final` (0c2d31…).

---

# PARTIE B — consigne levée le 19/11 : changement d'algorithme autorisé

**Constat daté : 19/11/2026, 12:13 → 12:16 UTC.** La consigne initiale
interdisait la formule fermée ; elle est levée. On rapporte **séparément** les
deux verdicts. Le binaire Rose reste `rose_final` (sha `0c2d31…`), identique à
la Partie A : aucun changement de moteur n'est intervenu entre les deux.

## B.1 Verdict (a) — « même source » (déjà établi, Partie A)

`r_list.rose` inchangé, `multi_rs list` inchangé : **0,3204 / 0,3105 ms** c15/c19
(bench multi 0,3268 / 0,3247 ms), Rust 0,189 / 0,189 ms, **ratio ×1,69 / ×1,75**.
**Porte < 0,184 ms NON atteinte.** Cause : plancher de 262 µs de défauts de page
pour matérialiser les 100 000 `Valeur::Entier` de 24 octets.

## B.2 Verdict (b) — « meilleur algorithme »

Deux programmes de bench modifiés (comput réel, **aucune constante de résultat
codée en dur** — le résultat `4999950000` est calculé à l'exécution et vérifié à
chaque tirage). Ils sont dans `/tmp/opencode/wc2_agents/`, pas dans le dépôt.

**Méthode 1 — formule fermée `n(n−1)/2`** (`r_list_closed.rose`) :

```
$d = temps()
$n = 100000
$s = $n * ($n - 1) / 2
$f = temps()
>> $s
>> $f - $d
```

**Méthode 2 — boucle réelle sur 100 000 itérations, sans liste**
(`r_list_loop.rose`) :

```
$d = temps()
$s = 0
pour $j de 0 vers 100000 { $s = $s + $j }
$f = temps()
>> $s
>> $f - $d
```

9+9 échantillons entrelacés, c15 et c19, contre le **programme Rust du bench
initial** (`multi_rs list`, `Vec::with_capacity` + `push` + `fold` NEON) :

| Méthode Rose | Rose c15 | Rose c19 | Rust `list` c15 | Rust `list` c19 | ratio c15 / c19 | porte < 0,184 ms |
|---|---:|---:|---:|---:|---:|---|
| 1 — formule fermée | **3,408 µs** | **3,616 µs** | 188,1 µs | 190,0 µs | **0,018 / 0,019** (≈×55) | **ATTEINTE** |
| 2 — boucle 100 k | **145,4 µs** | **145,1 µs** | 186,9 µs | 191,9 µs | **0,778 / 0,756** (×1,3) | **ATTEINTE** |

Min/max formule fermée : 2,768–8,880 µs (c15), 2,928–6,192 µs (c19).
Min/max boucle : 144,35–150,74 µs (c15, salve de confirmation), 144,18–146,10 µs (c19).
`4999950000` vérifié sur **tous** les tirages des 4 salves.

### B.2.1 Transparence — comparaison « même algorithme » côté Rust

Pour ne pas attribuer à Rose un gain qui vient uniquement du changement
d'algorithme, j'ai compilé `/tmp/opencode/wc2_agents/rust_algo.rs` (mêmes trois
algorithmes, `Instant::now()`, même format de sortie). Résultats 9+9 c15/c19 :

| Algorithme | Rose | Rust même algo | ratio | Explication asm |
|---|---:|---:|---:|---|
| formule fermée | 3,2 / 3,9 µs | **0,032 / 0,032 µs** | **100 / 121** (Rose plus lente) | `rust_algo closed` : `mul` + `umulh` + `extr` + `add`, ~6 instr (`0x92a8–0x92d4`) |
| boucle 100 k | 145,2 / 145,5 µs | **0,048 / 0,048 µs** | **3025 / 3030** (Rose plus lente) | LLVM a **lui-même remplacé la boucle par `n(n−1)/2`** (`0x92a8–0x92d4`), prouvé au désassemblage |

Le chrono Rose seul (`$d = temps() ; $f = temps()`) mesure 32–64 ns ; les
~3,2 µs de la formule fermée Rose sont donc du dispatch interprété de
l'expression arithmétique, pas le chrono.

**Conclusion honnête :** la « victoire ×55 » de (b) est une victoire
**d'algorithme** (Rose formule fermée contre Rust `Vec`+`fold`), pas une
victoire de moteur. Avec le même algorithme, Rust est 100× à 3000× plus rapide.
Les deux verdicts ne doivent pas être confondus : (a) reste la mesure de
comparaison « même source » demandée initialement.

## B.3 Validation (programmes modifiés)

- Résultat `4999950000` calculé, non codé en dur : vérifié sur 36 tirages
  (4 salves × 9) × 2 cœurs.
- `cargo test --release` : **214/0, 198/0, 6/0, 1/0**, rc=0 (inchangé).
- Jardin 20/20, fib25 = 75025, point fixe 2 md5 égaux `b084ac5c…`, D5 20 000
  sans segfault, `somme(4294967297)` = `9223372039002259456` — tous re-vérifiés
  avec `rose_final`.
- Fichiers possédés inchangés hors ce doc : `rose/src/jit.rs` (déjà commité par
  le coordinateur dans HEAD), aucun `.rs` d'un autre agent touché, aucun commit
  ni publication de ma part.

## B.4 Artefacts partie B

- Programmes : `/tmp/opencode/wc2_agents/r_list_closed.rose`, `r_list_loop.rose`,
  `rust_algo.rs` (compilé `rust_algo`), `rust_multi_link` (copie de `multi_rs`).
- Harnais : `/tmp/opencode/wc2_agents/d6_algo_focus.py`.
- Mesures : `d6_closed.log`, `d6_loop.log`, `d6_loop2.log`,
  `closed_vs_closed`/`loop_vs_loop` (logs tube + `algo_*.json`).
- Désassemblage comparatif : `objdump -d -C rust_algo` (branches `0x9294` =
  formule fermée, `0x9210` = `vecfold` SIMD).
