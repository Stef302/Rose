# Rose vs Rust AOT — enquête vitesse (`rustc -O`)

**Horodatage** : 2026-11-18T12:17Z → 12:32Z. Toutes les mesures sont des
**constats datés** (règle AGENTS.md) : re-mesurer après tout changement de
binaire, de topologie ou d'état machine.

**Machine** : aarch64 GB10, 20 cœurs hétérogènes — X925 `cpu_capacity≈1017`
(cœurs 15–19, 3,9 GHz) et A725 `cpu_capacity≈718` (2,8 GHz), `governor=performance`.
Toutes les salves sont **pinées** sur un cœur X925 via `taskset -c`.

**Binaire final** : `rose/rose` SHA256 `8a4ac955dced44dab318ca6eebbe309af4f3e8f0480b5bb3c2ae76fae48d6eab`
(commit `3cbf401` + modifications de cette étape). `rustc 1.99.1`.

## Protocole

- 9 échantillons par implémentation, **entrelacés par rotation de l'ordre**
  (A/A/A …), médianes rapportées (`min–max` aussi pour le cœur de la décision).
- Deux horloges distinctes : `wall` (processus complet, `subprocess`) et
  `interne` (chronomètre Rose `temps()` / `Instant` Rust), **jamais confondues**.
- Même entrée, même cœur, résultat vérifié à chaque échantillon
  (`9227465`, `5133`, `16326`) : aucun rejet.

## 1. Baseline du jour (12:17:20Z → 12:17:29Z, cœur 15, 9 échantillons)

Reproduction exacte de `BENCH_COMPARATIF.md` : le binaire reconstruit depuis
`3cbf401` a le **même SHA256** que celui du doc (`72d8cc31…`), donc la baseline
est reproductible.

| Benchmark | Rose interne | Rust interne | Rapport |
|---|---:|---:|---:|
| fib(35) récursif | **208,009 ms** | 12,656 ms | Rust **16,4×** plus rapide |
| premiers < 50000 | 26,963 ms | 1,004 ms | Rust 26,9× plus rapide |
| factorielle 5000 | **1,497 ms** | 7,843 ms | **Rose 5,24× plus rapide** |

→ Rose bat **déjà** Rust sur un benchmark entier à la baseline : factorielle
5000, même produit itératif `r *= k`, mêmes entrées. Le gain vient de la
bibliothèque bigint embarquée (`num-bigint`), pas d'un algorithme différent.
Le reste de cette étape vise le cas fib, où l'écart est le plus grand.

## 2. Analyse — pourquoi le JIT était 16× plus lent sur fib

Preuve par dump IR (`ROSE_DUMP_IR=1`, fichier `dump_fib.txt`) : `fib` n'est **pas**
compilé par la voie RIR mais par la voie AST (`emettre_fonction`), parce que
`rir::abstraire` refuse un appel dont les arguments ne sont pas des slots :
`fib(n - 1)` a pour argument `n - 1`, donc `AppelRir` n'est pas produit et
`rir_compilable_jit` échoue (ce que décrivait le contexte de mission).

Le dump IR du binaire initial montrait, **par appel de fib** :

1. **2 appels `rose_sub_ent`** (`n-1`, `n-2`). Chaque helper fait 2 recherches
   `MPZ_BOXES.contains()` dans un `HashSet` thread-local (`mpz_ptr` dans
   `run.rs`). Le tag est délibérément ignoré par `mpz_ptr` (le `tag_tmp` du JIT
   peut être périmé), donc **aucun court-circuit** n'existait : ~4 recherches de
   table de hachage par appel de fib.
2. 2 appels directs Cranelift à `fib` (`fn2 = colocated …`) — bons, pas de
   trampoline.
3. 1 `+` déjà optimisé : `iadd` i64 + détection de débordement en i128, repli
   sur `rose_add_ent` uniquement en cas de débordement.
4. 1 appel `rose_set_ret_tag(0)` (mesuré : **coût nul** — voir expérience rejetée).

**Deuxième cause, hors de portée d'un correctif local** : `rustc -O`/LLVM
réécrit la double récursion en boucle (`objdump -d pure_rs`) :

```
fib(n): if n<2 return n; acc=0; loop: acc += fib(n-1); n -= 2; if n>1 goto loop; return n+acc
```

Le nombre d'appels passe de `2·F(36)−1 = 29 860 703` (récursion naïve Rose) à
≈ `F(36) = 14 930 352` (Rust) : **Rust fait 2× moins d'appels**, chacun avec
une ABI maigre et aucun helper runtime.

## 3. Correctifs essayés (avant/après, tous pinés, 9 échantillons)

Une salve A/B/C/D entrelacée a été construite pour comparer les variantes sur
le **même cœur 19**, afin d'éliminer le bruit (voir §5) :
`ab_fib.py`, sortie `ab_fib_v3_out.txt` (12:28:43Z → 12:28:46Z).

| Variante | fib(35) interne médian | vs v0 |
|---|---:|---:|
| **v0** baseline (`3cbf401`) | 208,061 ms | 1,000 |
| **v1** chemin rapide `-` (i64 +Overflow i128, repli runtime) | 67,489 ms | 0,324 |
| **v2** v1 + détection de débordement bit à bit pour `-` | 52,985 ms | 0,255 |
| **v3** v2 + détection bit à bit pour `+` (**final**) | **51,302 ms** | **0,247** |
| Rust AOT (`rustc -O`) | 12,610 ms | 0,061 |

### 3.1 v1 — chemin rapide pour la soustraction (le gros gain)

Symétrie avec le `+` existant dans `jit.rs::expr` : `isub` i64 inline,
détection de débordement, bloc lent qui appelle `rose_sub_ent` uniquement si un
opérande est boxé ou si le résultat déborde. **208 → 67,5 ms (×3,08)**.
Correct : `fib(25)=75025`, jardin `premiers.rose` 20/20.

### 3.2 v2/v3 — détection de débordement en 4 instructions i64

Remplacement de l'aller-retour i128 (`sextend` → `isub`/`iadd` i128 →
`ireduce` → `sextend` → `icmp`) par l'identité signée :

```
débordement ⇔ ((a ^ b) & (r ^ a)) < 0    avec r = a ± b
```

Pour `-` : 67,5 → 53,0 ms (×1,27). Pour `+` : 53,0 → 51,3 ms (×1,03,
dans le bruit mais reproductible sur 9 échantillons entrelacés). Correct sur les
cas limites mesurés : `MAX+1`, `MAX-1`, `MIN-1`, `MAX+MAX`, `10^18*10^18`
(voir `overflow_test.rose`).

### 3.3 Expériences rejetées (documentées, non retenues)

- **Supprimer les appels `rose_set_ret_tag`** : redirection du symbole JIT vers
  un no-op → **67,461 → 67,635 ms** = bruit. Le coût n'est pas là. Reverté.
- **Émetteur machine natif (`ROSE_NATIF=1`, `natif_emetteur.rs`)** : non retenu.
  Même en étendant `peut_emettre` pour faire passer le RIR de fib, chaque
  `AppelRir` passe par `Trampoline::appel_rir` → `rose_appel_interpreteur`
  (recherche par nom + dispatch interprété) : **plus lent** que l'appel direct
  Cranelift. Le plafond de cette voie est donc inférieur à la voie JIT actuelle
  tant que les auto-appels ne sont pas émis en code machine direct.
- **Salve contaminée (12:24Z)** : une mesure intermédiaire donnait 107 ms pour
  Rose **et** 24,7 ms pour Rust **et** 1 182 ms pour Python. Cause mesurée :
  un autre agent pinait un `fact1m.rose` sur le **même cœur 15** et lançait
  `cargo test`. Toute la salve a été invalidée et rejouée sur le cœur 19.
  C'est l'application directe de la règle « re-mesurer l'état réel » : un
  rapport 2× sur les trois implémentations est un artefact, pas un résultat.

## 4. Résultats finaux (12:29:56Z → 12:30:03Z, cœur 19, binaire final)

| Benchmark | Rose interne | Rust interne | Verdict |
|---|---:|---:|---|
| fib(35) récursif | 51,310 ms (51,257–51,394) | 12,663 ms (12,574–12,683) | **Rust 4,05× plus rapide** |
| premiers < 50000 | 26,957 ms | 1,004 ms | Rust 26,9× plus rapide |
| factorielle 5000 | **1,466 ms** (1,463–1,470) | 7,829 ms (7,773–7,854) | **Rose 5,34× plus rapide** |
| 40 % en jardin | 1,776 ms | — | effet nul (`×1,00`) |

Comparé à la baseline du même jour : **fib 208,0 → 51,3 ms (×4,05)**, tandis
que premiers (26,96 → 26,96 ms) et factorielle (1,497 → 1,466 ms) sont
inchangés — le changement est bien ciblé sur le chemin AST de la soustraction.

### Verdict honnête

- **Oui, Rose bat Rust AOT sur un benchmark entier** : `factorielle 5000`,
  1,466 ms vs 7,829 ms (5,34×), **dès la baseline**, même produit itératif,
  mêmes entrées, mesures pinées. Ce n'est pas une victoire du langage mais de
  la bibliothèque numérique embarquée face au bigint maison naïf du `rustc -O`
  de référence (le doc `BENCH_COMPARATIF.md` le dit déjà).
- **Non, Rose ne bat pas Rust sur fib(35)** : 51,3 ms vs 12,7 ms. Le meilleur
  chiffre obtenu est **51,3 ms**, soit un écart réduit de 16,4× à 4,05×.
- **Non, Rose ne bat pas Rust sur le comptage de premiers** (26,9×), inchangé.
- Ne pas en conclure « Rose bat Rust » en général : c'est vrai sur un cas
  (bigint), faux sur les deux benchmarks entiers machine.

## 5. Ce qui est théoriquement possible — et ce qui bloque

Le plafond est **le code machine natif sans runtime**, c.-à-d. Rust équivalent.
Trois frais séparent Rose de ce plafond, mesurés ou identifiés :

1. **Frais de démarrage** (mesurés) : 1,65 ms de `wall − interne` sur fib,
   3,34 ms sur factorielle. Hors du calcul, mais ils dominent les petits
   programmes (règle « vitesse du moteur ≠ vitesse d'exécution »).
2. **Nombre d'appels** (identifié par désassemblage) : LLVM divise par 2 le
   nombre d'appels de la récursion fib. Tant que le JIT n'effectue pas cette
   transformation, Rose paie 29,86 M appels contre 14,93 M à Rust. Même avec
   un coût par appel identique, Rose resterait ~2× plus lent.
3. **Coût par appel** (mesuré) : 51,3 ms / 29,86 M = **1,72 ns/appel** pour
   Rose, contre 12,66 ms / 14,93 M = **0,85 ns/appel** pour Rust. Il reste
   ~3 tests de débordement + branches de fusion + le slot `tag_tmp` par
   invocation, contre une poignée d'instructions chez Rust.

**Conclusion** : pour battre Rust sur fib avec le même algorithme, il faudrait
*cumuler* la transformation en boucle de LLVM **et** un coût par appel égal à
celui de Rust. Aucune des deux n'est réalisable par un correctif local sûr dans
le temps de cette étape : la première est une optimisation de compilation
générale (et special-caser `fib` serait tricher), la seconde exigerait un
émetteur machine avec ABI registre et auto-appels directs — or l'émetteur M3.1
existant passe par une trampoline interprétée (plus lent). Le chiffre honnête
est donc **51,3 ms, 4,05× plus lent que Rust**, avec une amélioration réelle
de **×4,05** par rapport à la baseline du jour.

## 6. Reproduction

```bash
cd /home/clausake/Stefan/ROSE/rose
cargo build --release --bin rose && cp -f target/release/rose rose
cd /tmp/opencode/rosebench
PIN_CPU=19 python3 measure.py            # 4 groupes × 9 échantillons entrelacés
python3 ab_fib.py                        # A/B/C/D v0,v1,v2,v3,Rust — 9 échantillons
/home/clausake/Stefan/ROSE/rose/rose fib25.rose                    # 75025
/home/clausake/Stefan/ROSE/rose/rose /home/clausake/Stefan/ROSE/jardins/tests/premiers.rose  # 20/20
/home/clausake/Stefan/ROSE/rose/rose overflow_test.rose            # cas limites bigint
```

Sorties brutes conservées dans `/tmp/opencode/rosebench/` :
`baseline_out.txt` (12:17Z, cœur 15), `sub_fastpath_out.txt`,
`nooptag_out.txt` (rejeté), `bitwise_out.txt` (contaminé, rejeté),
`ab_fib_v3_out.txt` (12:28Z, cœur 19, décision), `final_out.txt` (12:30Z,
cœur 19, binaire final), `overflow_test.rose`, `fib25.rose`.
Binaire `rose_v0` = baseline git HEAD, `rose_v1/v2/v3` = variantes.
