# v3.15.0 — Rose

## RNG
- Dual RNG : pseudo (splitmix64, reproductible) + vrai (OS/matériel).
- Alias courts : `graine`, `hasard`.
- `rose_remplir_hasard` rendu SSA-friendly (état RNG local) → entrelacement des
  chaînes splitmix64.

## Bench langage entier (mot de passe, 1 Md de caractères)
- C++ (splitmix64) : 385–505 ms
- Rust (splitmix64) : 458–576 ms
- **Rose** : 520–560 ms  (dans la classe de Rust/C++, bat Rust sur sessions chaudes)
- C++ (mt19937 naïf) : 1 295–1 504 ms
- C (rand glibc) : 12 338–12 629 ms
- numpy (choice, ×10) : 8 490–14 430 ms

## MLIR (rose_labs)
- Pipeline validé : `remplir_hasard_std.mlir` → `mlir-opt-18` → `mlir-translate-18`
  → `clang -O3` → bench ; SSA ~4 ms vs Rose AOT ~41 ms (~10×).
- `build.sh` auto-détecte `mlir-opt-18` sur PATH (sudo apt) ou extrait sans sudo.
- `vec_double.mlir` : exploration affine super-vectorize.
- Vision MLIR pour tout le langage (dialecte `rose`, passes haut niveau).

## GPU (rose_labs/gpu)
- `gpu_probe.mlir` : kernel GPU valide (`mlir-opt-18 --verify-each`).
- Stratégie : RNG parallèle (Philox/Threefry) + dialecte gpu + lowering nvvm/rocdl/spirv.

## Fichiers modifiés
- `moteur/cpp/src/rose_runtime.h` (SSA)
- `rose_labs/` (mlir, gpu, README, perf variants)
- `bench/langages/README.md` (mesure SSA)
