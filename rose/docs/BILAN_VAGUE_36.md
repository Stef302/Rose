# Bilan vague 36 (19/11/2026) — racines, table, ABI G5, audit vert

## Audit final v8 (après toutes les livraisons de la vague 36)

`scripts/audit-final.sh` rejoué sur `f9b86e4` : **AUCUNE porte non atteinte**.
Oracles 174/0 ×4 · point fixe `8ac53eed…` · jardin 20/20 · fib(25) ·
bench_global rc=0 · rug absent · version 0.1.6.

## Livraisons

- **RA2 (racines)** : cache `n·10^(2d)` → +9,2 % interne. Verdict « meilleur
  algo » gagné (0,965/0,975 vs hybride, 0,873 vs Rust origine) ; verdict
  « même source » perdu de 3,2–3,7 % — aucune victoire algo égal revendiquée.
  Exactitude 171/171.
- **TA2 (table)** : partie directe 4 096 entrées (`Vec<u32>`, 16 Kio).
  Recherche −33 % ; 3 salves serrées FINAL/Rust 0,937/0,938/0,968 ; mais
  harnais multi 1,088 (artifact de trous ~100 ms, antérieur au changement).
  Double verdict : à source égale à égalité, meilleur algo gagné.
- **AB1 (G5 ABI)** : représentation unique avec tag 3 bits bas (entier `n<<3`,
  chaîne `ptr|1`, liste `ptr|2`). SIGSEGV d'origine supprimé ; `fib(10)`→55
  et `BASE`→1 rc=0. `emetteur.rose` entier → ELF 433 908 o, émission rc=0,
  mais exécution rc=139 : boucle d'allocation car E/S (`nb_arguments`,
  `argument`, `lire_texte`) sont des stubs. Réflexivité NON atteinte.
  Agent **IO1** lancé pour implémenter ces syscalls.

## Bench

7 victoires nettes (fib 0,92× · fact 0,21× · premiers 0,85× · PGCD 0,70× ·
chaîne 0,59× · boucle 0,38× · E/S 0,62×) + table/racines gagnantes sous le
verdict « meilleur algo ». Self-hosting toujours PARTIEL.
