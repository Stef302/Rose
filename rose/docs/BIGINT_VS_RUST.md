# BigInt Rose vs Rust — attaque D4b (PGCD 1 000 chiffres)

**Constat daté : 19/11/2026. Jour `d20bed0`. Binaire isolé `/tmp/opencode/jf_agents/`,
jamais `rose/rose`. Pin `taskset -c 6`, 9 échantillons entrelacés A/B/B/A,
médianes. Zéro GMP, zéro changement d'algorithme dans le programme Rose.**

## 1. Correction de prémisses

| Prémisse reçue | Constat mesuré |
|---|---|
| « le programme du bench est `r_gcd.rose` (Euclid) » | **Faux.** `r_gcd.rose` appelle le **builtin** `pgcd` (Stein sur `num-bigint`), déjà gagnant à **0,052 ms** contre 0,148 ms pour le Rust. L'Euclid interprété, c'est **`r_gcd_euclid.rose`** (`$r = $x % $y` en boucle), et c'est le D4b à 6,7 ms. |
| « D4b = 6,72 ms » | Reproduit : **6,693 ms** (18 échantillons), puis 6,716 / 6,735 ms selon la salve. |
| « 45,6× plus lent que Rust » | Reproduit : Rust Stein maison = **0,147 ms**. Ratio **45,6×**. |
| « gagner en optimisant le modulo BigInt » | **Faux.** Le modulo n'est que 3,8 % du temps. Le coût dominant est une conversion décimale. |

Les deux programmes coexistent dans `bench.py` : `D4b_gcd_1000ch` contient
`Rose (Euclid interprété)` = `r_gcd_euclid.rose` (cible) **et**
`Rose primitif pgcd (Stein)` = `r_gcd.rose` (hors comparaison, bibliothèque différente).

## 2. Décomposition par la preuve (instrumentation temporaire retirée)

Instrumentation `JF_PROF=1` (compteurs + `Instant` dans `i128x.rs` / `run.rs`,
vidage `atexit`), puis retirée. Binaire instrumenté : interne 7,218 ms avec
instrumentation contre 6,704 ms sans (overhead ~0,51 ms). Pourcentages calculés
sur la base non instrumentée 6,704 ms.

| Site | Appels | ns cumulés | % de D4b |
|---|---:|---:|---:|
| **`Gz::chaine()` → conversion décimale** | **950** | **6 259 547** | **93,4 %** |
| `gmp_fdiv_r` (modulo `num_bigint`) | 949 | 256 528 | 3,8 % |
| `vers_z` (boxage + copie `BigInt`) | 3 796 | 176 751 | 2,6 % |
| `cmp_gz` (comparaison) | 949 | 44 848 | 0,7 % |
| reste (boucle interprétée, dispatch, `zg_rapide`, `Rc`) | — | ~40 000 | ~0,6 % |
| `Gz::neuf()` : 4 750 allocations · `Gz::v()` : 4 745 clones | | | |

**Cause exacte.** Le dispatch `%` de `interp.rs:3049-3053` (fichier possédé par
JE, hors de ma propriété) fait :

```rust
let y = vers_z(&d);
if y.chaine() == "0" { erreur_runtime("modulo par zéro"); }
```

`Gz::chaine()` appelle `mul_rapide::vers_chaine`, dont le seuil est
`x.bits() < 10_000 → x.to_string()`. Les restes d'Euclide font 3 322 bits puis
décroissent : **950 conversions décimales complètes pour un simple test zéro**.
`num-bigint 0.5.1` utilise déjà `to_radix_digits_le_divide_and_conquer` avec
puissances de 10 précalculées *par appel* : 6,6 µs par conversion, déjà proche
de l'optimum pour cette taille.

**Conséquence structurelle :** même en supprimant totalement le modulo, le
boxage et la comparaison (6,5 % du temps), il reste 6,26 ms. La porte 0,147 ms
est **inatteignable en modifiant seulement `run.rs` / `i128x.rs`** : il faut
que le propriétaire de `interp.rs` remplace `y.chaine() == "0"` par un test
`y.bref().is_zero()` (O(1)). Ce n'est **pas** fait ici, par respect des
propriétés exclusives.

## 3. Décision retenue et variantes rejetées

### Retenu (4 changements, 35 lignes, `run.rs` + `i128x.rs`)

1. `Gz::cmp_gz` compare par emprunt (`bref()`) au lieu de 2 clones `BigInt`.
2. `gmp_fdiv_r` : zéro clone des opérandes, reste calculé dans une locale
   (sûr même si `r == a`, cas réel `interp.rs:3509`).
3. `Gz::chaine()` : court-circuit exact `is_zero() → "0"` (1 conversion sur
   950 évitée).
4. `cmp_vals` : voie rapide grand-entier-boxé vs i64, dans les deux ordres —
   plus de `vers_z` ni de `cmp_gz` pour `$y != 0`. Exactitude : `to_i64()` puis
   signe quand la valeur sort de l'intervalle.

### Rejetées, avec preuve

| Variante | Mesure / raison | Décision |
|---|---|---|
| Barrett / division réciproque sur le modulo | Modulo = 3,8 % (0,257 ms) ; gain maximal théorique 3,8 %, porte à 6,4 ms | **Non implémentée** (sans intérêt pour la porte) |
| Pool de `Gz` (`GcPool`) | `vers_z` rend un `Gz` **possédé** à des appelants de `interp.rs` ; `Drop for Gz` fait `Box::from_raw`. Recycler exige un changement d'API hors de mes fichiers. Coût total des allocations : 2,6 % | **Rejetée** |
| Conversion décimale à puissances de 10 mises en cache (D&C réécrite dans `i128x.rs`) | Bancmark sur les 949 vrais restes : **6,28 ms vs 6,19 ms** pour `to_string()`, ET longueurs fausses (711 398 vs 707 676 caractères) | **Rejetée** (plus lente et inexacte) |
| Remplacer l'Euclid par Stein dans `r_gcd_euclid.rose` | Changerait l'algorithme du programme Rose = hors comparaison ; le builtin Stein existe déjà à 0,052 ms | **Interdite par la consigne** |

## 4. Résultat D4b — porte NON atteinte

| Binaire | Médiane interne c6 (9×2 échantillons entrelacés) |
|---|---:|
| Base `d20bed0` | 6,716 ms / 6,722 ms |
| **Final** | **6,646 ms / 6,672 ms** |
| Rust Stein maison | 0,147 ms / 0,148 ms |

Gain **~1,1 %** reproductible sur deux salves A/B/B/A inversées (ratios
0,9896 et 0,9900). Porte 0,147 ms : **non atteinte**, écart **×45** (6,65 ms
au lieu de 0,147 ms). Cause : §2.

> Une salve isolée a montré des pics à 12,9 ms sur c6 (charge concurrente,
> `load average` 3,59). Les médianes retenues viennent des comparaisons
> entrelacées, qui annulent cette dérive.

## 5. Tableau 9 domaines (c6, 9 échantillons, médianes internes, salve inversée)

Rose = impl de référence de chaque domaine. `x` = final/base (<1 = plus rapide).

| Domaine | Base (ms) | Final (ms) | x | Verdict |
|---|---:|---:|---:|---|
| D1 fib35 (Rose JIT) | 11,6206 | 11,5844 | 0,997 | stable |
| D2 loop 1e6 | 0,2591 | 0,2599 | 1,003 | stable |
| D3 primes 50k | 0,8553 | 0,8528 | 0,997 | stable |
| D4 fact 5000 (boucle) | 1,4058 | 1,3783 | 0,981 | léger gain reproductible |
| **D4b Euclid 1 000 ch** | **6,7353** | **6,6618** | **0,989** | **léger gain, porte non atteinte** |
| D5 chaîne 20k | 0,9874 | 0,9681 | 0,980 | léger gain reproductible |
| D6 liste 100k | 0,4428 | 0,4322 | 0,976 | bruité (0,917 / 0,976) |
| D7 table 3000 | 0,1089 | 0,1111 | 1,021 | bruité (0,938 / 1,021), pas de conclusion |
| D8 E/S 10 Mo | 6,0775 | 7,0532 | 1,161 | bruit système : **toutes** les impls +18 % dans cette salve |

Aucune régression attribuable aux changements. D4b reste **45× plus lent que
Rust** et **128× plus lent que le builtin Stein Rose** (0,052 ms).

## 6. Exactitude et validation

- `cargo build --release --bin rose` : OK.
- `cargo test --release` : **204 + 188 + 6 + 1 = 399 tests, 0 échec**.
- Jardin `jardins/tests/premiers.rose` : **20 tests, 0 échec**.
- `fib(25)` = **75025** (et `fib(35)` = 9227465).
- Point fixe sur sources **gelées à `d20bed0`** : `bdbef636…` ×2, identique
  base et final. ⚠ Dans l'arbre partagé, `compilateur/emetteur.rose` a été
  modifié par un autre agent le 19/11 à 09:56 : le md5 du point fixe y a
  changé pour **les deux** binaires — c'est un déplacement de cible, pas une
  régression de ce patch.
- Corpus bigint, sorties **octet pour octet identiques** base/final :
  `fact 5000` md5 **`0147c25aca0a`** ; 2^63−1, 2^63, 1e10×1e10, négatif hors
  i64, `MAX−(−MAX)`, grand produit (`exact_consolidee.rose`) ;
  `d2_exact_loop.rose`, `r_somme_depassement.rose`, `d2_exact.rose`
  (débordements JIT, md5 `fb6dacf0f54b` / `05b62776033e` / `83c5c899990b`) ;
  `grands_premiers2.rose` ; E/S exactitude ; table exactitude ; PGCD Euclid et
  Stein (ligne de temps normalisée).
  (`grands_premiers.rose`, primarialité géante, dépasse 180 s sur **les deux**
  binaires : hors corpus de cette validation, déjà couvert par `grands_premiers2`.)

## 7. Limite à lever pour la prochaine victoire

Une ligne dans `interp.rs` (possession JE) —
`if y.chaine() == "0"` → `if y.bref().is_zero()` — supprimerait **93,4 % du
temps D4b** (~6,26 ms) d'un coup, sans changer la sémantique du modulo ni
l'algorithme Rose. Le reste (modulo 0,257 ms + boxage 0,177 ms) donnerait alors
~0,45 ms, toujours au-dessus de 0,147 ms mais 15× plus proche de la porte.
Ce chantier-ci s'arrête à la frontière de propriété, avec la preuve à l'appui.
