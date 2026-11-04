# 🔧 Expérience 1 — JIT Cranelift (référence)

## Objectif
Mesurer les performances du JIT Cranelift déjà intégré dans `rose/`.
C'est la base de référence contre laquelle on compare les autres briques.

## Ce qui est déjà en place
- `rose/src/jit.rs` : émetteur Cranelift (entiers i64 taggés, promotion BigInt)
- `rose/src/main.rs` : commande `rose-20 prog.rose` (interpréteur + JIT à chaud)
- Parité parfaite avec le moteur C++ (54/54, 46/46 dorés)
- Bench global : 0,084 s (vs 0,18 s C++)

## Comment mesurer
```bash
./rose_labs/tools/lancer_exp.sh jit_cranelift O2
```

## Résultats attendus
| Niveau | Temps bench global | Mémoire |
|--------|-------------------|---------|
| O1     | ~0,10 s           | ~7 Mo   |
| O2     | ~0,084 s          | ~7 Mo   |
| O3     | ~0,08 s           | ~7 Mo   |
