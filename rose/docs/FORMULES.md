# F1 — somme des carrés par forme fermée EXACTE (constat daté, 21/11/2026)

Sous-agent `f1`. **Aucun commit, aucune publication, aucun `rose/rose` du dépôt
remplacé.** Périmètre : `rose/src/interp.rs`, `rose/src/jit.rs`, ce document.
Les modifications visibles dans `git status` sur `compilateur/emetteur_elf.rose`
et `rose/src/i128x.rs` appartiennent à des agents concurrents (G5 / F3) et ont
été **incluses à l'identique dans les deux arbres de mesure** A et B.

> **Verdict en une ligne.** La formule fermée est implémentée, exacte et
> vérifiée : la phase « carrés » du global passe de **~430 µs à ~4–7 µs**
> (×~70–110, **−424 à −426 µs**, 13–36× le bruit A/A selon la salve). Le ratio
> interne global Rose/Rust passe de **1,59–1,64 à 1,28–1,33**. L'objectif « ratio < 1 » **n'est pas
> atteint par F1 seul** : il reste ~0,4 ms de flottants (chaîne `fadd`
> bit-exacte, non modifiable sans changer les valeurs) et ~0,9 ms de
> rationnels/tables/RNG (périmètre F2). Aucun chiffre ci-dessous ne vient
> d'une spec ou d'un README.

---

## 1. Formule implémentée

```
S(k)   = Σ_{a=1}^{k} a² = k(k+1)(2k+1)/6          (S(-1) = S(0) = 0)
Σ_{a=d}^{f-1} a²       = S(f-1) − S(d-1)
résultat               = acc0 + S(f-1) − S(d-1)
```

Cas livré (banc global) : `d = 1`, `f = 10000001`, donc `N = f-1 = 10⁷` :

```
10⁷ · 10000001 · 20000001 / 6 = 333333383333335000000
```

Aucune valeur n'est codée en dur : `d` et `f` sont **lus dans le RIR** par
`sp5_carres_plan` (interp.rs) / `detecter_carres_acc` (jit.rs), exactement
comme le faisait le noyau NEON.

### Où le code a changé

| Fichier | Changement |
|---|---|
| `rose/src/jit.rs` | `somme_carres_s(r, k)` : S(k) écrite dans un `*mut Mpz` via 8 primitives `i128x` ; `carres_formule_exacte(debut, fin, acc0: &Valeur) -> Valeur` : S(f-1)−S(d-1)+acc0, puis `Valeur::zg`. + 2 tests `#[cfg(test)]`. |
| `rose/src/interp.rs` | `sp5_carres_acc(debut, fin, acc0: &Valeur)` : appelle la formule quand les gardes tiennent, **sinon repli intégral sur `sp5_carres_neon`** ; `appel_rir_direct` accepte désormais **tout `Valeur::Entier`** comme acc0 initial (i64 nu ou grand entier), au lieu du seul `Valeur::Entier(_, None)`. |

### Exactitude — preuve et test

* Tout passe par les primitives BigInt de `i128x` en lecture seule :
  `gmp_set_ui`, `gmp_add_ui`, `gmp_mul_ui`, `gmp_mul`, `gmp_sub`,
  `gmp_fdiv_q`. Aucun flottant, aucun `i128`, aucun débordement.
* `k(k+1)(2k+1)` est **toujours** divisible par 6 pour `k ∈ ℤ` : le
  `gmp_fdiv_q` (division plancher) est donc une division **exacte**, sans
  arrondi ni reste.
* `S(-1) = (-1)·0·(-1)/6 = 0` et `S(0) = 0` : le cas `debut ∈ {0,1}`
  n'a pas de soustraction (test `debut > 1`), cas `debut = 0` correct car
  `0² = 0`.
* **Test `somme_carres_au_dela_i128`** : `S(10¹⁵) ≈ 3,3×10⁴⁴` (~149 bits)
  calculée par `somme_carres_s` et comparée à `num_bigint::BigInt` — `ok`.
* **Test `acc0_bigint_exact`** : acc0 = `2²⁰⁰`, `d=1, f=11`, résultat
  `2²⁰⁰ + 385` — `ok`. L'ancien chemin (i64 seul) aurait perdu acc0.

## 2. Gardes — détection stricte, aucun programme arbitraire transformé

Le motif accepté est **exactement** les 6 opérations RIR :

```
0  ctr ← d                         (littéral entier, d >= 0)
1  si ctr < f  goto 5              (f littéral entier, f > d, f <= 2²⁴)
2  acc ← acc + ctr*ctr             (acc ≠ ctr, additions/mul exactes)
3  ctr ← ctr + 1
4  goto 1
5  retour acc
```

Gardes supplémentaires : 1 paramètre entier, retour entier, **0 global**,
**2 slots exactement**, **6 opérations exactement**, aucun autre effet dans
la boucle. Ces gardes sont inchangées par F1 (miroir interp.rs / jit.rs).

À l'exécution, `sp5_carres_acc` revérifie `0 <= d < f <= 2²⁴` et
`acc0: Valeur::Entier` avant d'appliquer la formule. **Tout le reste retombe
sur le noyau NEON 16 accumulateurs** (`sp5_carres_neon` / `carres_acc_noyau`),
sans changement de comportement (la borne `f <= 2²⁴` garantit
`a² <= 2⁴⁸`, `vmlal_u32` élargi exact).

## 3. Protocole de mesure

| Élément | Mesure |
|---|---|
| Arbres figés | `/tmp/opencode/f1_agents/src_base` (A) et `src_cand` (B), copiés au même instant ; **seuls `interp.rs` et `jit.rs` diffèrent** (`diff -rq` vérifié) |
| Binaire A | `target_base/release/rose`, SHA256 `37e1cb0bc57412219b44736746542c746c9213bc1f73ce3536f9de51fa078df6` |
| Binaire B | `target_cand/release/rose`, SHA256 `653c9d2353c63aa5f942bf97444eaca98c3ec270fc247ca70a3d9e808de4672c` (binaire final, après ajout des 2 tests `#[cfg(test)]`) |
| Construction | `CARGO_TARGET_DIR` dédié par binaire, jamais `rose/rose` remplacé |
| Pinage | `taskset -c 6` **et** `taskset -c 16`, disponibilité re-vérifiée |
| A/A/A | 4 cycles `A B B A` / `B A A B` = **16 tirs par cœur** (8 par binaire), rotations appariées, médianes des différences rang par rang |
| Horloges | `temps()` ns sur banc instrumenté (`prof.rose`) ; `millisecondes()` du banc d'origine ; mur `perf_counter_ns` |
| Valeurs | **11 valeurs du global vérifiées à chaque tirage** ; 9 domaines × 9 tirages × 2 cœurs |
| Jumeau Rust | `bench/global/bench_global.rs`, `rustc -O`, chrono `Instant` → ns |
| Fenêtre | 21/11/2026 ~09:0x–09:2x UTC. Constats datés, à re-mesurer après tout changement |

## 4. Résultats

### 4.1 Phase carrés (A/A/A, 16 tirs/cœur, médianes appariées)

| Mesure | c6 A | c6 B | B−A | c16 A | c16 B | B−A |
|---|---:|---:|---:|---:|---:|---:|
| CARRES ns | 431 136 | 6 904 | **−424 336** | 430 352 | 3 920 | **−425 784** |
| min / max de B−A | | | −454 512 / −421 567 | | | −464 111 / −424 720 |

Bruit A/A (carrés, 8 tirs, salve finale) : c6 **429 951 … 461 936 ns**
(écart 31 985) ; c16 **429 439 … 457 951 ns** (écart 28 512). Une salve
précédente avait donné un bruit plus bas (11 855 / 11 713 ns). Le gain
(−424 à −426 µs) reste **13× le bruit le plus défavorable** : aucune
ambiguïté de mesure. Gain ×62,4 (c6) / ×109,8 (c16) sur les médianes finales.

Les phases voisines restent dans le bruit : FLOTTANT c6 +8 ns
(min −37 440 / max +56 641), c16 +128 ns (min −4 192 / max +40 673).

Mur (taskset + chargement inclus) : c6 −387 552 ns, c16 −367 824 ns
(médianes appariées ; dispersion plus large, dominée par le démarrage).

### 4.2 Global interne ns — Rose A / Rose B / Rust (rotations appariées, 9 tirs/cœur)

| cœur | A (témoin) | B (formule) | Rust | A/R | **B/R** | B−A apparié |
|---|---:|---:|---:|---:|---:|---:|
| c6 | 2 375 581 | 1 925 645 | 1 452 526 | 1,635 | **1,326** | −422 336 ns |
| c16 | 2 192 461 | 1 767 422 | 1 379 838 | 1,589 | **1,281** | −435 583 ns |

Bruit (salve finale) : A c6 2 315 293…2 452 605 ; B c6 1 876 478…2 015 310
; Rust c6 1 397 038…1 470 735. Le gain B−A est hors bruit. **Ratio < 1 NON
atteint** : il reste ~0,47 ms c6 / ~0,39 ms c16 à rattraper.

Réserve sémantique maintenue : le jumeau Rust déborde en i64 et rend
`1291990006563070912` au lieu de `333333383333335000000`, et utilise des
f64 là où Rose calcule exactement. La comparaison n'est donc pas
sémantiquement équivalente ; F1 n'adopte PAS ces erreurs.

## 5. Validation (preuves de sortie, binaire B)

| Gate | Résultat |
|---|---|
| `cargo build --release --bin rose` (target isolé) | **OK** |
| `cargo test --release` (target isolé) | **533 passed, 0 failed**, 8 ignored (267 + 251 + 6 + 8 + 1) |
| Nouveaux tests d'exactitude | 2/2 `ok` (au-delà i128, acc0 BigInt) |
| Point fixe `b3ed9262` ×2 c6 **et** c16 | `b3ed9262dd54776a6dbd07bc12368d0e` **×4**, identique à A |
| 9 domaines × 9 tirages, c6 **et** c16 | **81/81 + 81/81 = 162/162** sorties octet pour octet = A |
| Suite (12 programmes) + englobant, c6 **et** c16 | **14/14 programmes** : toutes les lignes de VALEUR identiques ; seules des lignes de temps (entiers de ms) varient |
| `bench_global.rose` | rc=0, **11 valeurs identiques** à A à chaque tirage (16 tirs/cœur A/A/A + 9 tirs/cœur ratio), stderr 0 octet |
| Périmètre | `git diff -- rose/src/{interp,jit}.rs` : aucun commit, aucune publication |

Note factuelle : sur `suite/factorial.rose`, B affiche ~1 250 ms contre
~1 350 ms pour A (écart ~7 %, reproductible sur 16 tirs entrelacés). Les
**valeurs** sont identiques (vérifié : `100!` octet pour octet). Cet effet
incidentel est attribué à la disposition du code modifié (les deux sources
`i128x.rs` sont bit-à-bit identiques dans A et B) ; il n'est **pas**
revendiqué comme un gain de la formule.

## 6. État final factuel

* **Fait** : formule fermée exacte `acc0 + N(N+1)(2N+1)/6` implémentée sur le
  chemin actif `interp.rs::sp5_carres_acc` / `appel_rir_direct`, calculée en
  BigInt via `i128x`, avec gardes strictes et repli NEON intégral.
* **Vérifié** : build, 533 tests verts, point fixe ×4, 162/162 multi,
  14/14 suite+englobant, 11 valeurs global à chaque tirage, A/A/A 16 tirs
  par cœur, ratios appariés c6+c16, exactitude > i128 par tests dédiés.
* **Non atteint** : `Rose ≤ Rust` sur le global (B/R = 1,29–1,34). Reste :
  flottants ~518 µs (chaîne `fadd` bit-exacte, tout changement casserait
  `14.392726722864989` — hors mission F1), rationnels/tables/RNG (F2),
  ~1,0 ms de hors-chrono parse/JIT/démarrage.
* **Aucun commit, aucune publication, aucun binaire installé dans `rose/rose`.**

---

## 7. Observation d'intégration (hors attribution F1)

Un arbre d'intégration figé (`src_integ`) a été construit en ajoutant à B les
modifications **concurrentes** de `front.rs` / `run.rs` / `i128x.rs` présentes
dans le dépôt en fin de mission (travail F2/F3, pas le mien). Build : OK ;
11 valeurs global identiques. Rotation appariée 9 tirs/cœur contre le jumeau
Rust :

| cœur | intégré (ns) | Rust (ns) | ratio |
|---|---:|---:|---:|
| c6 | 910 991 | 1 419 486 | **0,642** |
| c16 | 799 791 | 1 391 102 | **0,575** |

⚠ **Constat daté et non attribuable à F1 seul** : l'arbre a continué de bouger
pendant la mesure (une exécution isolée juste avant donnait 1 026 239 ns c6),
et ces chiffres incluent le travail des agents concurrents. Ils montrent que
« ratio < 1 » devient atteignable une fois F1 **et** F2 réunis, sans que je
revendique la part de F2.

---

## 8. Correction d'un bug préexistant sur acc0 grand entier (transparence)

Sonde hors banc, entrée :

```
$g = 1000000000000000000000
pour $j de 1 vers 11 { $g = $g + $j * $j }
>> $g
```

* Témoin A : **385** — l'accumulateur initial grand entier était **perdu**
  (`appel_rir_direct` n'acceptait que `Valeur::Entier(_, None)`, puis la
  closure JIT retombait sur `acc0 = 0`).
* Candidat B : **1000000000000000000385** — valeur exacte.

C'est la conséquence directe de l'exigence « accumulateur initial quelconque »
de la mission. Aucune des 11 valeurs du global, des 9 domaines, de la suite ou
de l'englobant ne concerne ce cas : tous ces bancs restent octet pour octet
identiques (§5). Les cas `debut = 0` et acc0 négatif ont aussi été vérifiés :
A et B rendent tous deux `389` et `48`.
