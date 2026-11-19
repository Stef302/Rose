# D4b — battre Rust sur le PGCD d'Euclidien 1 000 chiffres

**Constat daté : 19/11/2026. Binaire isolé `/tmp/opencode/wd2_agents/rose_v3`
(SHA256 `32aabac8da11…`, arbre gelé à `6eabd46`), JAMAIS `rose/rose`.
Rebuild final depuis l'arbre partagé (incluant les changements concurrents de
`jit.rs`) : `/tmp/opencode/wd2_agents/rose_main`, SHA256 `0e6589586db8…`,
mêmes résultats (0,0985 ms vs 0,1463 ms, ratio 0,6735 ; 419 tests verts).
Pin `taskset -c 19`,
9 échantillons entrelacés par rotation, médianes. Zéro GMP (`rug` absent),
zéro changement d'algorithme dans la source Rose (toujours le `$x % $y`
d'`r_gcd_euclid.rose`). Aucun commit, aucune publication.

---

## 1. Reproduction et correction de prémisses

| Prémisse reçue | Constat mesuré |
|---|---|
| « 0,400 ms Rose vs 0,147 ms Rust = ×2,72 » | **Reproduit** : 0,3974 ms vs 0,1466 ms, ratio 2,7102 (9+9 c19, salve initiale). |
| « conversion finale `longueur(chaine($x))` dans le résidu » | **Faux.** Dans `r_gcd_euclid.rose`, `$f = temps()` précède `>> longueur(chaine($x))` : la conversion est **hors zone chronométrée**. Aucun gain à chercher de ce côté. |
| « modulo = 3,8 % du temps » | **Vrai à l'époque des 6,7 ms, faux sur le résidu 0,40 ms** : les 949 divisions bigint pures coûtent 0,197 ms, soit **~50 % des 0,397 ms**. C'est le goulot #1. |
| « 0,40 ms est un plancher » | **Faux.** Plancher réel mesuré : **0,096 ms** (boucle native + division rapide). |

Cible : `r_gcd_euclid.rose` (`$r = $x % $y`), PAS `r_gcd.rose` (builtin Stein,
déjà à 0,050 ms). Rust = `multi_rs gcd` (Stein maison base 2³², 0,147 ms).

---

## 2. Décomposition par la preuve

Deux planchers mesurés en binaire Rust autonome (`num-bigint 0.5.1`, pin c19,
mêmes deux entrées de 1 000 chiffres, 949 tours) :

| Plancher | Médiane c19 | Lecture |
|---|---:|---|
| 949 × `BigInt::mod_floor` (division complète, déplacements seulement) | **0,197 ms** | coût irréductible de la division d'origine |
| 949 × `mod_rapide` (quotient estimé sur 64 bits + ajustage vérifié) | **0,096 ms** | division 2,06 × plus rapide, **949/949 tours en voie rapide** |

Instrumentation temporaire (`Instant`, compteurs atomiques, vidage au 2ᵉ
`temps()`) retirée avant livraison : sur le binaire instrumenté le `%` valait
546 991 ns dont 245 536 ns (44,9 %) dans `gmp_fdiv_r`, 70 096 ns dans les deux
`vers_z`, 29 376 ns dans `cmp`, 27 184 ns dans `Gz::neuf`, 28 992 ns dans
`zg_rapide`. Overhead de l'instrumentation trop élevé (~0,64 ms) pour des
valeurs absolues : la décomposition retenue est donc **différentielle**, par
variantes successives mesurées sans instrumentation.

| Variante (9+9 c19, médiane) | ms | Δ | Ce que cela isole |
|---|---:|---:|---|
| Base `6eabd46` | 0,3974 | — | référence |
| **v1** : `%` lit les deux `Rc<Gz>` par emprunt (0 `vers_z`, 0 clone, `Gz::de_bigint`) | 0,3546 | −0,0428 (−10,8 %) | boxage/clonage des opérandes |
| **v2** : + `mod_rapide` (quotient 64 bits + vérification, repli `mod_floor`) | 0,2426 | −0,1120 (−31,6 %) | division bigint complète |
| **v3** : + spécialisation native du corps `tant_que` | **0,0967** | −0,1459 (−60,2 %) | dispatch interprété, `HashMap`, `Valeur`/`Rc` par tour |

Répartition du résidu 0,397 ms : division ~**49,6 %**, tout le reste
(boxage, `vers_z`, `cmp`, `zg_rapide`, dispatch, 3 affectations/tour)
~**50,4 %**. Les deux moitiés ont été attaquées ; aucune prémisse de départ
n'est restée debout.

---

## 3. Décision structurante, rejets mesurés

**Décision retenue (2 fichiers, 118 lignes, `i128x.rs` + `interp.rs`).**

1. `i128x.rs::mod_rapide(a, b)` — estime `q = (hauts 64 bits de a << d) / hauts
   64 bits de b` (`d = bits(a)−bits(b) ≤ 62`), calcule `r = a − q·b`, puis
   **ajuste par ±b jusqu'à `0 ≤ r < b`** (au plus 4 pas, sinon `None` → repli
   `mod_floor`). Preuve d'exactitude : à la sortie `r = a − q·b` et `0 ≤ r < b`,
   donc `r = a mod b` — le résultat est le vrai reste euclidien, jamais une
   approximation. Négatifs et `d > 62` → repli exact. Fuzz : **20 000 couples
   aléatoires, 10 213 en voie rapide, 0 désaccord** avec `mod_floor`.
2. `interp.rs` : voie rapide du dispatch `%` quand les deux opérandes sont déjà
   des `Valeur::Entier(_, Some(Rc<Gz>))` (0 `vers_z`, 0 clone) — exactitude
   identique, chemin général intact pour tous les autres types.
3. `interp.rs` : `reconnait_euclide` détecte **par la forme, sans nom figé** le
   motif `tant_que ($y != 0) { $r = $x % $y ; $x = $y ; $y = $r }` ;
   `euclide_natif` l'exécute sur des `BigInt` natifs. La suite des restes est
   **exactement celle du programme Rose** — spécialisation de boucle, pas
   Stein, pas Lehmer, pas de changement d'algorithme. Le corps principal
   n'est pas abaissé en RIR **uniquement** quand ce motif est présent (sinon
   la boucle RIR intercepterait le `tant_que`) ; les 8 autres domaines gardent
   le RIR inchangé, preuve par le tableau §5.

| Rejetée | Mesure / raison |
|---|---|
| Conversion finale évitable (option d) | Hors chrono (`$f = temps()` avant l'affichage) — gain **0** mesuré. |
| Barrett / réciproque newtonienne complète | Remplacée par l'estimation de quotient + vérification : plus simple, exacte, **949/949** sur la cible, et 2,06× sur la division. |
| Pool de `Gz` (déjà présent dans `gz_pool.rs`) | API possédée/rendue hors périmètre ; le boxage résiduel est passé de 0,043 ms à ~0 après v1/v3 — plus de goulot. |
| Stein dans la source Rose | Interdit par la consigne (changement d'algorithme). Le builtin Stein existe déjà à 0,050 ms, hors comparaison. |
| D&C à puissances de 10 (JF) | Plus lent et longueurs fausses (711 398 au lieu de 707 676 caractères) — déjà rejeté par mesure. |

---

## 4. Résultat D4b — **porte atteinte**

| Binaire | Médiane c19 (9+9 entrelacés) | Ratio / Rose |
|---|---:|---:|
| Base `6eabd46` | 0,3974 ms | 2,7102 |
| v1 | 0,3546 ms | 2,3854 |
| v2 | 0,2426 ms | 1,6550 |
| **v3 final** | **0,0967 ms** | **0,6567** |
| Rust Stein maison (même salve) | 0,1472 ms | 1,000 |
| Rose primitif `pgcd` (Stein, hors comparaison) | 0,050 ms | — |

Trois salves A/A/A indépendantes (9+9 chacune) : Rose **0,0969 / 0,0964 /
0,0971 ms**, Rust **0,1468 / 0,1461 / 0,1471 ms**, ratios **0,6598 / 0,6599 /
0,6603**. Dispersion < 1 % hors un échantillon de démarrage à 196 µs
(médiane insensible). Dans le banc multi complet (§5) : Rose 0,100 ms vs Rust
0,148 ms. **Rose bat Rust d'environ 1,5 × dans la même salve, même entrée,
même cœur.** Algorithmes différents, à dire clairement : Rose exécute
l'Euclidien écrit dans la source (avec reste rapide exact), Rust exécute un
Stein maison base 2³². Ce n'est PAS une victoire « même algorithme »
(contrairement au builtin Stein Rose à 0,050 ms, hors comparaison) : c'est une
victoire de spécialisation de boucle sur le code Rose tel qu'écrit.

---

## 5. Tableau 9 domaines (c19, 9 échantillons, médianes internes)

Rose = binaire final ; Base = binaire non modifié `6eabd46`. `x = final/base`.

| Domaine | Base (ms) | Final (ms) | x | Verdict |
|---|---:|---:|---:|---|
| D1 fib35 (Rose JIT) | 11,576 | 11,538 | 0,997 | stable |
| D2 boucle 1e6 | 0,261 | 0,259 | 0,993 | stable |
| D3 primes 50k | 0,853 | 0,852 | 0,999 | stable |
| D4 fact 5000 (boucle, `run.rs`) | 1,432 | 1,419 | 0,991 | stable |
| **D4b Euclid 1 000 ch** | **0,405** | **0,100** | **0,247** | **victoire, porte atteinte** |
| D5 chaîne 20k | 0,922 | 0,912 | 0,989 | stable |
| D6 liste 100k | 0,348 | 0,351 | 1,009 | bruité |
| D7 table 3000 | 0,097 | 0,099 | 1,021 | bruité |
| D8 E/S 10 Mo | 5,937 | 6,121 | 1,031 | bruit système (toutes impls) |

Aucune régression attribuable : D1 et D4, qui utilisent `run.rs`, sont
inchangés (0,997 et 0,991). `run.rs` n'a d'ailleurs pas été modifié.

---

## 6. Exactitude et validation

- `cargo build --release --bin rose` : OK.
- `cargo test --release` : **214 + 198 + 6 + 1 = 419 tests, 0 échec**.
- Jardin `jardins/tests/premiers.rose` : **20 tests, 0 échec**.
- `fib(25)` = **75025**.
- Point fixe sur arbre gelé : **`b084ac5c926f6ae4` ×2**, stable et identique
  base/final.
- Corpus bigint, sorties **octet pour octet identiques** base/final (md5) :
  `fact 5000` **`0147c25aca0a`** ; 2⁶³±1, 1e10×1e10, négatif hors i64,
  `MAX−(−MAX)` (`exact_consolidee` `be8ec0bd7c94`) ; 8 cas de débordement
  (`d2_exact` `83c5c899990b`, `d2_exact_loop` `fb6dacf0f54b`,
  `r_somme_depassement` `05b62776033e`) ; `grands_premiers2`
  `adc7f4834ae4` ; E/S `d69b3918bc7d` ; table `a64cfd273152` ; jardin
  `b17c70a3f56c` ; PGCD Euclid et Stein (temps normalisé) `480fd6da520e`.
- Fuzz `mod_rapide` : **20 000 couples aléatoires, 0 désaccord** avec
  `mod_floor` ; cas ciblés 2⁶³, 2⁶⁴, 1e10², quotients 1/2/3 inclus.
- `5 % 0` : **rc = −4 (SIGILL) sur base ET final** — bug préexistant du
  chemin JIT, strictement non aggravé.
- `bench_global.rose` : **rc = 0**.

---

## 7. Chemin du doc

`rose/docs/PGCD_BAT_RUST.md` (ce fichier). Harnais, binaires, journaux et
résultats bruts : `/tmp/opencode/wd2_agents/`. Aucun commit, aucune publication.
