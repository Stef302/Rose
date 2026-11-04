# 🧪 Rose Labs — Laboratoire d'expérimentation des optimisations

Ce dossier permet de faire une **copie de Rose** et d'expérimenter différentes
briques d'optimisation de la chaîne de compilation, sans toucher au dépôt
principal (`rose/` et `rose-cpp/`).

## Objectif

Comparer et mesurer l'impact des briques d'optimisation suivantes sur le
moteur Rose (Rust/Cranelift) :

| # | Brique | Choix | Gain attendu | Statut |
|---|--------|-------|--------------|--------|
| 1 | JIT | Cranelift | ~10× la compilation | ✅ Déjà intégré dans `rose/` |
| 2 | AOT + chaud | LLVM ORC -O2 + PGO | pic le plus haut, +8-19 % compilation seulement | ❌ À expérimenter |
| 3 | Tier 1 | copy-and-patch maison | < 1 ms / 1000 instructions | ❌ À expérimenter |

## Niveaux d'optimisation

| Niveau | Usage | Drapeaux |
|--------|-------|----------|
| dev itératif | développement rapide | `-O1` |
| release / production | usage courant | `-O2` (défaut actuel) |
| noyau numérique serré | SIMD, non limité mémoire | `-O3` |
| avec profil relevé | meilleur ratio | `-O2 -fprofile-use` (PGO) |

## Arborescence

```
rose_labs/
├── README.md              # ce fichier
├── exp/                   # expériences par brique
│   ├── jit_cranelift/     # mesures JIT Cranelift (référence)
│   ├── aot_llvm_orc/      # AOT LLVM ORC + PGO
│   └── tier1_copy_patch/  # copy-and-patch maison
├── bench/                 # programmes de test et résultats
│   ├── *.rose             # sources de banc
│   └── resultats.csv      # mesures agrégées
├── tools/                 # scripts d'automatisation
│   ├── copier_rose.sh     # fait une copie de rose/ vers copies/
│   ├── lancer_exp.sh      # lance une expérience
│   └── mesurer.sh         # mesure temps + mémoire
└── copies/                # copies de travail de Rose
    └── rose_ref/          # copie de référence (vide, à remplir)
```

## Démarrage rapide

```bash
# 1. Faire une copie de rose/ pour expérimenter
./rose_labs/tools/copier_rose.sh

# 2. Lancer l'expérience JIT (référence)
./rose_labs/tools/lancer_exp.sh jit_cranelift

# 3. Lancer l'expérience AOT LLVM ORC
./rose_labs/tools/lancer_exp.sh aot_llvm_orc

# 4. Lancer l'expérience Tier 1 copy-and-patch
./rose_labs/tools/lancer_exp.sh tier1_copy_patch

# 5. Voir les résultats
cat rose_labs/bench/resultats.csv
```

## Prérequis

- Rust 1.99+ avec cible `x86_64-pc-windows-gnu` (pour les builds croisés)
- `x86_64-w64-mingw32-gcc` (déjà installé sur gx10-2)
- `makensis` (NSIS, pour l'installeur Windows)
- `python3`
