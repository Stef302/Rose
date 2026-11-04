# 🚀 Expérience 3 — Tier 1 copy-and-patch maison

## Objectif
Implémenter un compilateur de **tier 1** ultra-rapide basé sur le principe
**copy-and-patch** : au lieu de générer du code machine instruction par
instruction, on copie des "patchs" de code machine précompilés (templates)
et on les assemble avec des relocations minimales.

## Gain attendu
- **< 1 ms pour 1000 instructions** compilées
- Idéal pour le développement itératif (rechargement à chaud instantané)
- Moins optimisé que Cranelift/LLVM ORC, mais inutile pour le code "froid"

## Principe du copy-and-patch

```
Source Rose      →  AST  →  Séquence de "patchs" machine  →  Code exécutable
                                     ↑
                              Templates précompilés
                              (ajout, soustraction, appel, saut, etc.)
```

Chaque patch est un bloc de code machine avec des "trous" (relocations) pour
les adresses et immédiats. Le compilateur tier 1 copie ces patchs et remplit
les trous.

## Quand l'utiliser ?
- **Dev itératif** : compilation quasi-instantanée après chaque modification
- **Code froid** : fonctions appelées rarement, pas besoin d'optimisation
- **Rechargement à chaud** : modules rechargés sans redémarrer le moteur

## Quand ne PAS l'utiliser ?
- **Noyau numérique serré** : utiliser -O3 avec LLVM ORC (exp 2)
- **Code chaud appelé des millions de fois** : utiliser JIT Cranelift (exp 1)

## État : À implémenter

### Étapes prévues
1. Créer une bibliothèque de templates machine (x86_64 et aarch64)
2. Créer le copieur de patchs avec relocations
3. Intégrer comme tier 1 devant Cranelift (compilation tier 1 → optimisation tier 2 en arrière-plan)
4. Mesurer : < 1 ms / 1000 instructions

## Comment lancer l'expérience
```bash
./rose_labs/tools/lancer_exp.sh tier1_copy_patch O1
```
*(Actuellement : compilation standard avec -O1, pas encore copy-and-patch natif)*
