# JIT natif self-hosté — état expérimental (non intégré)

## Constat

Un compilateur RIR → code machine aarch64 en Rose pur a été écrit et validé
en isolation (agent `jit`, 624 lignes, `compilateur/jit_self.rose` dans son
worktree). Il réutilise les encodeurs de `emetteur_elf.rose` et émet un ELF64
aarch64 minimal avec routine décimale intégrée (syscalls `write`/`exit`).

**Formes couvertes** : `n`, `entier`, `slot`, `v`, `bin`, `neg`, `app`,
`appel_rir`, `retour`, `afficher_abstrait`, sauts, comparaisons, récursion.
**Non couvertes** : bigints > 64 bits, `^`, `%`, listes, tables, chaînes,
rationnels, flottants, E/S, arguments.

## Mesure en isolation (agent, médianes, `fib(28)`)

| Configuration | Temps |
|---|---:|
| Moteur self-hosté avant | 8 512 ms |
| Moteur + JIT natif | 4,23 ms |
| Moteur `interprete` (repli) | 8 520 ms |
| Rust JIT (Cranelift) | 1,78 ms |

→ ×2 000 vs l'interpréteur self-hosté, mais **2,4× plus lent que Cranelift**.

## Pourquoi il n'est PAS intégré dans le moteur livré

1. **Fusion instable** : après intégration du module JIT dans
   `moteur_complet.rose` (déjà mémoïsé), la commande `jit` produit un
   **bus error** (rc=135) — l'ELF émis par `js_run` n'est pas exécutable de
   façon fiable en présence des globals de mémoïsation.
2. **Aucun gain réel** : la mémoïsation des fonctions pures (chemin retenu,
   commit `3b44296`) donne déjà **`fib(28)` en 2,2 ms**, soit le même ordre de
   grandeur que le JIT natif (4,23 ms) et à 2× du JIT Rust — **sans casser la
   parité 141/141**.
3. **Risque de régression** : le JIT émettait de fausses sorties sur 15
   programmes non éligibles (bigints, `^`, listes, `argument`) avant renforcement
   du filtre. Livrer un chemin qui peut produire un résultat faux est inacceptable.

## Décision

- **Chemin livré** : mémoïsation des fonctions pures (défaut), parité 141/141,
  point fixe `b3ed9262dd54776a6dbd07bc12368d0e`, 3/5 programmes battent le JIT
  Rust.
- **JIT natif** : conservé comme piste de recherche, non activé. Il faudra
  corriger l'émission ELF en présence de la mémoïsation, puis re-mesurer avant
  toute intégration.
