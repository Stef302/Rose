# Offensive suite complexe — 19/20 novembre 2026

Fichier possédé : `rose/src/jit.rs` (seul). Binaire construit dans
`/tmp/opencode/cplx1_agents/rose_final2` — **`rose/rose` n'a PAS été remplacé**.
Aucun git commit, aucune publication.

## 1. Garde levé — OUI

`bloc_compilable` (branches `Instr::TantQue` et `Instr::Pour`) refusait toute
boucle dont le corps contenait un `Expr::LitComplexe`. Le garde est désormais
conditionnel :

* nouvelle fonction `corps_a_accumulateur_flottant_ou_complexe` : détecte un
  accumulateur `Flottant`/`Complexe` dans le corps (en publiant les types des
  variables au fur et à mesure, variable de boucle comprise) ;
* la porte ne saute **que** si cet accumulateur existe **et** si `i` n'est pas
  le compteur entier du programme (`i_est_boucle()` / `i_est_entier_verifie()`
  restent des garde-fous) ;
* `JitVal::Complexe` (déjà présent) émet le corps en deux registres f64 —
  aucune `Valeur` allouée par opération.

Mesure `ROSE_TIMING=1 rose_final2 bench/suite/complexe.rose` :

```
avant : TIMING-JIT eligibles=["__rose_loop_0"]        exec=310.880 ms
après : TIMING-JIT eligibles=["calc", "__rose_loop_0"] exec=1.003 ms
```

`calc` est donc compilé. Le nombre de fonctions éligibles passe de 1 à 2.

## 2. Exactitude bit-à-bit — OUI

Deux corrections ont été nécessaires, toutes deux mesurées, pas devinées :

1. **`expr()` / `i_est_imaginaire`** — l'ancienne heuristique émettait le
   lexème `i` (`LitComplexe {0,1}`) comme l'entier `1` dès que
   `!i_est_compteur()`. Or dans `complexe.rose`, `i` est la VRAIE unité
   imaginaire (`i_est_compteur=false, i_est_boucle=false,
   i_est_entier_verifie=false`, mesuré). Résultat avant correction :
   `total_re = n` (1 000 000) au lieu de `0.9999999999860738`.
   Un champ `i_est_imaginaire`, calculé par `signature_avec_complexe(sig)`,
   conditionne désormais l'heuristique.
2. **`type_of()`** — quand `types["i"] == Entier` (compteur `pour i de …`),
   le `LitComplexe {0,1}` doit être typé `Entier`, sinon `binaire()` le traite
   comme complexe et `$s + i` part en f64. Mesuré sur
   `rose-cpp/tests/roses/rose_boucles.rose` : 99 au lieu de 4950, puis 0 au
   lieu de 12 pour `$max`. Corrigé, le principal retombe en interpréteur
   (`compileprincipal=2.6 µs`, comme la base).

Troisième correction de parité : les helpers JIT `rose_affiche_prefixe` /
`rose_affiche_formule` émettaient encore un préfixe LaTeX `\sum_{…}` alors que
l'interpréteur n'en émet plus (`run.rs:1030`, « formule toujours vide depuis le
16/11 », test `interp.rs:6796`). Ils sont devenus des no-ops. **Les appels dans
l'émetteur ont été conservés** : les retirer changeait l'éligibilité du
principal (mesuré) — c'est une leçon de cette offensive.

Preuve : sortie complète de `complexe.rose`, base vs final2 —

```
md5 base   = ba144f964dc4b8775b176c31205c98fd
md5 final2 = ba144f964dc4b8775b176c31205c98fd   (cmp -l : 0 octet divergent)
0.9999999999860738
333333833333500000
```

## 3. Gain réel — 9 échantillons entrelacés, pin c6, médianes

Harnais `/tmp/opencode/cplx1_agents/bench_c6.py` (A/A/A entrelacé par rotation,
valeur f64 vérifiée à chaque tirage, 4 implémentations, 9 tirages chacune).

| Moteur | mur méd (min–max) ms | CPU méd (min–max) ms |
|---|---:|---:|
| Rose base (avant) | 312,547 (310,295–317,246) | 312,266 (309,904–317,084) |
| Rose patché (final2) | **3,210 (2,787–3,924)** | **2,850 (2,631–3,250)** |
| Rust `-O` | 1,645 (1,304–1,884) | 1,533 (1,214–1,736) |
| C++ `-O3` | 1,511 (1,365–1,773) | 1,458 (1,293–1,705) |

* Ratio CPU Rose/Rust : **203,70 → 1,86**.
* Division du ratio : **÷109,6** (mur : ÷97,4).
* Cible « ÷5 au moins » (≤ ×38 d'après `COMPARATIF_TOTAL`, ≤ ×36,5 d'après
  BIG1) : **atteinte avec une marge considerable**. Rose bat toujours Python
  (~547 506 ms) et n'est plus qu'à ~1,9× de Rust/C++ dans cette mesure c6.

Le chiffre `×189` de RUN2 était mesuré sur c16 ; le `×203,70` ci-dessus est
celui de c6. Les deux sont des constats datés, pas des consignes ; le rapport
 reste « ÷110 », pas « ×189 exact ».

## 4. Validation (binaire `rose_final2`, réjouée après la dernière modification)

```
cargo build --release --bin rose                         rc=0
cargo test --release                  431 passed, 0 failed (3+3 ignored)
jardins/tests/premiers.rose           ── jardin/premiers : 20 tests, 0 échec ──
fib(25)                               75025
point fixe ×2                         b3ed9262dd54776a6dbd07bc12368d0e (×2, md5 identiques)
9 domaines multi (c6, 9 tirages)      81/81 valeurs vérifiées, rc=0
bench/global/bench_global.rose        rc=0 ; 11 lignes de VALEUR md5-identiques à la base
                                      (ligne 12 = temps interne, variable)
parité JIT ↔ interpréteur             111 ok / 119 ; 8 divergences
```

**Honnêteté sur la parité et global :**

* Le harnais `bench_global_c11.py` échoue sur une **valeur attendue périmée**
  (ligne 3 : `0` attendu, grand rationnel rendu) — **identique dans la base**,
  donc préexistant et étranger à ce chantier. Le programme lui-même rend
  `rc=0` et ses 11 premières lignes sont bit-identiques à la base
  (md5 `1bf6d33252555ff20a4ab5d96659e4ab` des deux côtés).
* Parité JIT ↔ interpréteur : base = 7 divergences, final2 = 8. La huitième
  est `bench/suite/sieve.rose`, programme **non déterministe** (valeurs
  730–748 observées sur 5 tirages du MÊME binaire, base comme final2) : bruit,
  pas régression. `rose_boucles.rose`, brièvement régressé pendant le chantier,
  a été corrigé et rend à nouveau 4950 / 12 dans les deux voies.
* Aucun bench, aucun programme de test, aucun autre fichier source n'a été
  modifié. `interp.rs`, `run.rs`, `Cargo.toml`, `PLAN.md` n'ont été que lus.

## 5. État des lieux factuel

* **Fait** : garde levé, `calc` JIT-compilé, corps complexe en deux f64.
* **Vérifié** : sortie bit-égale, ÷110 du ratio, Rust presque atteint (×1,86),
  toutes les validations demandées rejouées (431 tests, jardin, fib25, point
  fixe, 81 valeurs multi, global rc=0).
* **Reste** : battre Rust (×1,86 aujourd'hui) demanderait un autre chantier.
  `ROSE_TIMING` sur `complexe.rose` donne `compiletout≈0,66–1,26 ms` et
  `exec≈0,99–3,66 ms` selon les tirages (variance forte mesurée, 3 tirages) :
  le coût résiduel se répartit entre la compilation JIT, l'exécution des deux
  boucles et le démarrage du processus — la décomposition fine en f64 n'est
  pas stable assez pour être affirmée. Aucune triche, aucun bench modifié.
