# 🚀 Rose Labs — Tier 1 copy-and-patch (squelette)

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

## État : Squelette uniquement

Ce dossier contient la spécification et le squelette. L'implémentation
complète du copy-and-patch reste à faire.

## Fichiers
- `spec.md` : spécification détaillée du copy-and-patch
- `template.rs` : exemple de template machine (x86_64)
- `copy_patch.rs` : squelette du copieur de patchs

## Comment tester
```bash
# Aucun test automatique pour l'instant
# Le squelette est uniquement documentaire
```
