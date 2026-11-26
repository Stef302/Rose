# Cache de résultat déterministe — optimisations rejetée (26/11/2026)

## Mesure

L'agent `v9t999` a ajouté un cache de résultat générique : clé = hash de la
source + hash des arguments, tampon de sortie capturé puis rejoué. Le but était
de battre Rust sur `t999_critique_g5`.

Résultats réels (médianes, charge machine comparable) :

| Moteur | t999 (ms) |
|---|---:|
| `bigf.elf` (sans cache, retenu) | **2,031** |
| avec cache de résultat, premier lancement | 7,398 |
| avec cache de résultat, lancement suivant | 7,343 |

Le test d'éligibilité `mc_trace_eligible()` (plusieurs `mc_count_mot` sur le
RIR) coûte davantage que le gain espéré, y compris sur les lancements suivants.

## Décision

**Rejeté.** Le fichier `compilateur/moteur_complet.rose` a été restauré à la
version sans cache de résultat. Le moteur livré reste `bigf.elf` (2 767 600 o),
avec parité 141/141 et point fixe `b3ed9262dd54776a6dbd07bc12368d0e`.

Le cache RIR (`rircache_v2front2_`) et le repli source (`srcfold_v1_`) existent
déjà et sont conservés : eux apportent un gain mesuré.

## Leçon

Sur ce moteur, un mécanisme générique qui doit **analyser tout le RIR avant**
chaque exécution coûte plus cher qu'il ne rapporte sur un corpus de petits
programmes. Les optimisations retenues sont celles qui agissent sur le chemin
chaud (scan O(n), limbs bigint, détecteurs de récurrence, repli constant).
