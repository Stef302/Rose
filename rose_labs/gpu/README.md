# Rose Labs — GPU

Objectif : faire tourner le langage Rose sur GPU **mieux** que sur CPU.

## Stratégie (MLIR)

MLIR expose des dialectes GPU (`gpu`, `nvgpu`, `nvvm`, `rocdl`, `spirv`, `amdgpu`)
et une conversion `vector → gpu`. La chaîne cible :

```
rose dialect (ops haut niveau : remplir_hasard, map, reduce, …)
  → scf/affine + memref
  → gpu dialect (gpu.launch, gpu.thread_id, …)
  → nvvm (CUDA)  ou  rocdl (ROCm)  ou  spirv (Vulkan/OpenCL)
  → LLVM IR → PTX / amdgcn
```

Les passes clés disponibles dans mlir-opt-18 (voir `mlir-opt-18 --help`) :
- `--convert-vector-to-gpu`     : vector → gpu dialect
- `--gpu-to-nvvm` / `--gpu-to-rocdl` / `--gpu-to-spirv` : lowering GPU
- `--nvvm-attach-target` + `--convert-gpu-to-nvvm` : pipeline CUDA
- `--gpu-kernel-outlining` : extrait les kernels
- `--gpu-async-region` : exécution asynchrone

## Pourquoi le GPU peut battre le CPU ici

Le goulot CPU est la **bande passante mémoire** (~68 GB/s) et la **chaîne de
dépendance** de splitmix64. Sur GPU :
- Bande passante mémoire beaucoup plus élevée (HBM ~1–2 TB/s).
- Des milliers de threads masquent la latence.
- `remplir_hasard` est massivement parallèle : chaque position de sortie est
  indépendante (si on découpe la séquence RNG en sous-séquences indépendantes,
  ex. via Philox/Threefry à la place de splitmix64).

## Plan d'intégration

1. **RNG parallèle** : remplacer splitmix64 (chaîne séquentielle) par un RNG
   parallèle (Philox 4×32 ou Threefry) qui permet à chaque thread de calculer
   son sous-flux sans dépendance. Sans ça, le GPU ne sert à rien (sérialisation).
2. **Dialecte `rose`** : définir `rose.remplir_hasard`, `rose.map`, `rose.reduce`
   qui se lowernt vers `gpu.launch` + memref.
3. **Bench** : comparer `remplir_hasard` CPU (SSA ~4 ms/100M) vs GPU kernel.
4. **Intégration** dans le moteur : `rose build --gpu` émet le kernel + host code.

## Statut

Exploration uniquement. Aucune intégration moteur. Voir `gpu_probe.mlir` pour un
squelette de kernel (non exécutable dans cet environnement : pas de CUDA/ROCm).