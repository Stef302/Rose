# Rose Labs — expériences (non production)

Ce dossier contient des expériences **hors du moteur principal** : tentative de
dépasser Rust/Zig/C++ sur le banc `mot_de_passe`, et exploration de MLIR.
Rien ici n'est intégré au moteur (`moteur/`) tant qu'un gain mesurable et
reproductible n'est pas démontré.

## Perf : objectif « plus bas que natif »

Banc de référence (N = 100 M, 5 passes, min retenu) :
- C++ splitmix64 : ~37 ms
- Rose AOT LLVM IR : ~41 ms
- Rust splitmix64 : ~69 ms
- Zig splitmix64 : ~453 ms (1 Md)

Variantes testées (voir `perf/`) :
| Variante | Min | Verdict |
|---|---:|---|
| 4 tirages/lot (base) | 37 ms | référence |
| 8 tirages/lot | 35 ms | marginal, instable |
| stores packés uint64 | 32–53 ms | instable (thermique) |
| stores non-temporels | 44–47 ms | plus lent |
| `restrict` + `assume_aligned` | 51–60 ms | plus lent |

Conclusion : le goulot est la bande passante mémoire (~68 GB/s) et la chaîne
de dépendance de splitmix64. Aucun gain fiable > 10 % trouvé.

## MLIR : statut

MLIR n'est **pas** installé dans cet environnement. Pour expérimenter :

```bash
sudo apt install libmlir-18-dev llvm-18-dev clang-18
```

Idée d'intégration (future) : définir un dialecte `rose` (ops `rose.remplir_hasard`,
`rose.hasard`, `rose.graine`) qui se lower vers le dialecte `scf`/`memref` puis
LLVM, permettant des passes custom (vectorisation, fusion) avant l'émission
LLVM IR. À n'engager que si un gain ≥ 10 % est mesuré sur le banc.

## Structure

- `perf/` : variantes C++ du banc pour expérimentation.
- `mlir/` : squelettes MLIR + pipeline automatisé + exemple vectoriel.
- `gpu/`  : exploration GPU (dialecte gpu, stratégie RNG parallèle).

## MLIR : statut (mis à jour)

MLIR **est maintenant installé** (libmlir-18-dev + mlir-18-tools, via extraction
sans sudo si besoin : voir `mlir/build.sh`).

Pipeline validé (ARM64, LLVM 18.1.3) :

```
mlir-opt-18  input.mlir --convert-scf-to-cf --convert-cf-to-llvm \
             --convert-arith-to-llvm --finalize-memref-to-llvm \
             --convert-func-to-llvm --reconcile-unrealized-casts -o lowered.mlir
mlir-translate-18 --mlir-to-llvmir lowered.mlir -o out.ll
clang -O3 out.ll driver.c -o bench
```

Résultat clé : en exprimant la boucle de tirage comme `scf.for` dont l'état
RNG est une variable SSA (au lieu d'un global), LLVM **entrelace automatiquement
les 4 chaînes de splitmix64** (2 tirages par itération, stores groupés `stp`)
et le banc passe de ~41 ms (Rose AOT) à **~4 ms** sur 100 M (gain ~10×).

Le levier n'est donc pas MLIR lui-même mais la forme SSA de la boucle, qui
permet à l'optimiseur d'interleaver les chaînes de dépendance. À reproduire
dans le moteur : rendre `rose_rng_tirer` / `rose_remplir_hasard` SSA-friendly
(état explicite) plutôt que global, ou émettre l'IR LLVM dans cette forme.

## MLIR pour tout le langage (vision)

Au-delà de `remplir_hasard`, MLIR permet d'exprimer le langage Rose à un
niveau supérieur (dialecte `rose` : `rose.map`, `rose.reduce`, `rose.remplir_hasard`)
et d'appliquer des passes de haut niveau (fusion de boucles, tuilage, vectorisation
via `affine-super-vectorize`, parallélisation via `scf.parallel`) **avant**
l'émission LLVM. C'est la voie pour battre Rust/Zig/C++ sur les opérations de
tableaux. Voir `mlir/vec_double.mlir` (exemple affine vectorisé) et
`gpu/gpu_probe.mlir` (kernel GPU). Intégration moteur à engager uniquement si
un gain ≥ 10 % est mesuré sur le banc.

Outils (sans sudo) :

```bash
cd rose_labs/mlir
./build.sh            # extrait mlir-18-tools si absent, puis compile
```

Fichiers :
- `mlir/remplir_hasard_std.mlir` : boucle `scf.for` + splitmix64 (dialectes standard)
- `mlir/driver.c` : driver de banc
- `mlir/build.sh` : pipeline automatisé
## Nouvelle expérience MLIR (26/10/2026) — réduction

- `mlir/reduction_std.mlir` : boucle `scf.for` avec accumulateur en SSA (`iter_args`),
  lowering standard → LLVM IR, compilé par clang -O3 -march=native.
- Résultat (N = 100 M int64) : MLIR 24,0 ms vs C++ -O3 -march=native 29,0 ms
  → gain ~17 % sur un noyau de réduction simple.
- Conclusion : la forme SSA de l'accumulateur permet à LLVM d'entrelacer/vectoriser
  la réduction, comme pour `remplir_hasard`. Généralisable à tout noyau `map`/`reduce`.

Fichiers : `mlir/reduction_std.mlir`, `mlir/reduction_driver.c`, `mlir/build/reduction_std.bench`.

## Correction M4.4 (26/10/2026) — promotion manquante

- Bug : les opérateurs composés en place (`+=`, `-=`, `*=`) promouvaient le membre
  gauche via `a = rose_ent(rose_vers_mpz(a))`, mais le constructeur `rose_ent(mpz_class)`
  re-démote les petites valeurs → `a.g_` restait null → crash dans `__gmpz_add_ui`
  dès que le résultat dépassait int64 (boucle de sommes de carrés).
- Fix : forcer l'allocation `a.g_ = new mpz_class(...)` (comme les opérateurs binaires).
- Effet mesuré sur `bench/global/bench_global.rose` : AOT 1.09 s → 0.12 s (×9),
  parité conservée (119/0), suite verte.

## Correction honnête (26/10/2026) — ampleur réelle du gain SSA

- Le « ×10 » annoncé pour `remplir_hasard` (41 ms → 4 ms) comparait des
  charges **différentes** : le MLIR stocke des int64 bruts (1 tirage / 8 octets),
  tandis que le banc password fait une recherche dans l'alphabet + empaquetage
  (4 tirages / 32 octets). Comparaison de pommes et d'oranges.
- Sur des noyaux **comparables** (réduction 100 M int64), le gain SSA mesuré
  est de **~15–17 %** vs C++ `-O3 -march=native` (réduction : 24,0 vs 29,0 ms ;
  somme des carrés : 26,4 vs 31,0 ms).
- Le banc password LLVM est déjà à 39 ms, à parité avec C++ splitmix64 (37 ms) :
  le goulot est la bande passante mémoire, pas la forme de la boucle.
- Conséquence : la vraie valeur de Rose Labs est dans les **passes MLIR de haut
  niveau** (fusion, vectorisation, tuilage) sur des données structurées, et dans
  le **multi-cœurs/GPU** (bande passante), pas dans la micro-optimisation du RNG.

## Expérience MLIR — vectorisation affine (26/10/2026)

- `mlir/sumsquares_affine.mlir` : somme des carrés en dialecte `affine`, vectorisée
  via `--affine-super-vectorize` (vectorize-reductions, virtual-vector-size 2 et 4).
- Résultats (N = 100 M int64) :
  - scalaire MLIR : 26,4 ms
  - vectorisé vec4 : 51,9 ms (×2 plus lent)
  - vectorisé vec2 : 65,0 ms (×2,5 plus lent)
- Conclusion : l'auto-vectorisation affine **dégrade** les performances sur cet
  ARM64 (NEON 128 bits) pour ce noyau — le masquage de voies et les
  `transfer_read` coûtent plus que le gain SIMD. Le scalaire reste le meilleur.
  La vectorisation n'est rentable qu'avec la bonne largeur (SVE) et des noyaux
  plus lourds (matmul, conv). À réévaluer avec SVE/AVX-512.
