# BIGSYM — primitives mathématiques pour factorielles géantes symboliques

MATH1, 21/11/2026. Implémentation dans `rose/src/i128x.rs` **uniquement**.
Aucune dépendance externe (pas de GMP/`rug`), aucun `f64`, aucun état global,
aucune mémoïsation. `num_bigint::BigInt` partagé avec le reste du moteur.
Compatible x86-64 et aarch64 (arithmétique Ruby/Rust pure).

Objectif : répondre à des questions sur `n!` pour `n` énorme (ex. `10^38`,
`10^1000`) **sans jamais matérialiser l'entier `n!`**, sauf sur le petit
chemin `n ≤ 1000` où le calcul exact sert d'oracle interne.

## API publique

```rust
pub fn log10_factorielle_precision(n: &Gz, digits: usize) -> (Gq, bool);
pub fn zeros_finaux_factorielle(n: &Gz) -> Gz;
pub fn derniers_non_nuls(n: &Gz, k: usize) -> String;
pub fn premiers_chiffres_factorielle(n: &Gz, count: usize, precision: usize) -> String;
```

### 1. `log10_factorielle_precision(n, digits)`

Retourne `(q, exact)` où `q` est `log10(n!)` tronqué à `digits` décimales,
sous forme rationnelle `num / 10^digits`. De `q` on déduit :

- **nombre de chiffres décimaux de `n!`** = `floor(q) + 1` (partie entière) ;
- **partie fractionnaire de `log10(n!)`** = `q − floor(q)`.

`exact = true` signifie : la borne d'erreur **prouvée** est strictement plus
petite que la distance du résultat au bord de troncature supérieur/inférieur
aux `digits` demandées. Tous les chiffres rendus sont alors garantis corrects.
`exact = false` ne signifie pas « faux » : le résultat reste le meilleur
encadrement calculé, mais la dernière décimale peut être ambiguë (cas d'un
`log10(n!)` tombant très près d'une frontière de troncature).

Bornes : `n ≥ 0`, `n` jusqu'à `10^1000` testé ; `digits` quelconque
(20/50/100 validés ; la précision de travail croît comme `digits + log10(n)`).
`n < 0` → `erreur_runtime`.

### 2. `zeros_finaux_factorielle(n)`

`Σ_{k≥1} ⌊n/5^k⌋` (Légendre) — nombre exact de zéros finaux de `n!`.
Coût `O(log₅ n)` : pour `10^38`, 55 divisions BigInt, ~microseconde.
`n ≤ 0` → `0`.

### 3. `derniers_non_nuls(n, k)`

Les `k` derniers chiffres **non nuls** de `n!`, sans zéros de tête
(`10! → "288"`, `20! → "17664"`, `100! → "5210916864"`).
`k = 0 → ""`, `n = 0` ou `1 → "1"`, `n < 0 → erreur_runtime`.

Méthode : on retire tous les facteurs 2 et 5 de chaque terme, on multiplie
mod `10^k`, puis on réintroduit l'excès `ν₂(n!) − ν₅(n!)` de facteurs 2.

Complexité (honête) :

| Régime | Algorithme | Coût |
|---|---|---|
| `n ≤ 2·10⁶` | produit direct | `O(n·k)` |
| `n > 2·10⁶` et `5^k ≤ 10⁷` | récurrence `mod 5^k` + CRT | `O(5^k·log₅ n)` |
| `n > 2·10⁶` et `5^k > 10⁷` | repli direct | `O(n·k)` — **lent, documenté** |

Le chemin récursif n'est déclenché que pour `k` modestes. Il a été validé
contre le produit direct sur `n = 1001, 1234, 2000, 5000, 9999` et
`k = 1, 3, 5`. Pour `k` très grand et `n` géant, la fonction reste correcte
mais coûteuse : c'est la limite connue, pas une promesse de performance.

### 4. `premiers_chiffres_factorielle(n, count, precision)`

Les `count` premiers chiffres décimaux de `n!`, obtenus par
`10^{frac(log10 n!)}` en virgule fixe. `precision` fixe le nombre de
décimales de travail : **« plus de chiffres » = augmenter `precision` et
recalculer** (aucun état, aucune mémoïsation). `count = 0 → ""`.
`100! → "933262154439441526816992388562"`.

Cas limite traité : certaines factorielles ont une mantisse décimale *exacte*
(`10! → 3,6288` pile). Toute erreur résiduelle, même à `10^{-120}`, ferait
perdre une unité au dernier chiffre par troncature. La correction relève
alors d'un si — et seulement si — la valeur calculée est à moins de la borne
d'erreur **prouvée** sous l'entier supérieur. Un epsilon aveugle arrondirait
à tort les valeurs proches de `x,5` (ex. `100!` doit rester `…38562`).

## Précision prouvée

### Série d'Euler–Maclaurin / Stirling

```
ln(n!) = (n+1/2)·ln n − n + (1/2)·ln(2π)
         + Σ_{j=1}^{m} B_{2j} / (2j(2j−1)·n^{2j−1}) + R_m
```

Borne de Robbins : `|R_m| ≤ |B_{2m+2}| / ((2m+2)(2m+1)·n^{2m+1})`.
Le nombre de termes `m` est choisi **par comparaison entière exacte** :
`|B_{2m+2}| · 10^{p+5} < (2m+2)(2m+1)·n^{2m+1}` (donc `|R_m| < 10^{−(p+5)}`).
Les `B_{2j}` sont engendrés comme rationnels exacts par la réccurrence
`Σ_{k=0}^{j} C(j+1,k)·B_k = 0` — aucune valeur codée en dur.

### Virgule fixe

Toutes les quantités réelles sont des `BigInt / 10^p`. Chaque
multiplication/division tronque ≤ 1 ulp ; l'atanh sur `K` termes accumule
≤ `8K+16` ulps ; la multiplication par `⌊log₂ x⌋` amplifie l'erreur de
`ln 2` ; le terme dominant `(n+1/2)·ln n` amplifie l'erreur de `ln n` par
`n`. La borne conservatrice `e` (en ulps) est calculée en entiers et :
- fixe le drapeau `exact` (distance au bord d'arrondi) ;
- commande la correction de mantisse dans `premiers_chiffres_factorielle`.

`ln` : `x = f·2^k`, `f ∈ [1,2)`, `z = (f−1)/(f+1) ∈ [0,1/3]`,
`ln f = 2·Σ z^{2j+1}/(2j+1)` — chaque terme est divisé par ≥ 9, convergence
en ≈ `1,05·p` termes. `ln 2 = 2·atanh(1/3)`. `π = 16·atan(1/5) − 4·atan(1/239)`
(Machin). `exp` par `Σ x^j/j!`, arrêt quand le terme devient nul.

Précision de travail retenue : `p = digits + chiffres_décimaux(n) + 60`
(120 dans `premiers_chiffres_factorielle`). Le terme additionnel compense
l'amplification par `n` : pour `n = 10^1000`, `log10(n!) ≈ 10^1003`, donc
~1003 chiffres de travail supplémentaires sont nécessaires et fournis.

## Validation (`cargo test --release`, 0 échec sur le périmètre MATH1)

Oracles indépendants : `mpmath` 200–250 dps (`loggamma`), factorielles
exactes `num_bigint`/`math.factorial`.

- `log10(n!)` à 100 décimales : `n = 10, 100, 1000, 10000` — **4/4 conformes** ;
  convergence 20/50/100 chiffres imbriquée ; nombre de chiffres de `n!`
  vérifié contre `factorielle_parallele(n).chaine().len()`.
- `zeros_finaux` : `100! → 24`, `1000! → 249`, `1 000 000! → 249 998`,
  et `0..=1000` exhaustif.
- `derniers_non_nuls` : `10! → 288`, `20! → 17664`, `7! → 504`,
  `25! → 985984`, `100! → 5210916864`, `1000! → 27753472`,
  et `0..=1000 × k ∈ {1,3,5,10,20}` exhaustif contre la factorielle exacte.
- `premiers_chiffres` : `100!` 30 chiffres, stabilité à
  `precision ∈ {30,40,80,120}`, comparaison à la factorielle exacte pour
  `n ∈ {10,100,500,1000}`, `n = 10^38` (30 chiffres + partie entière de
  `log10` vérifiée).
- `n = 10^1000` : `log10` renvoie une partie entière de 1003 chiffres,
  stabilité du raffinement, zéros finaux de 1000 chiffres.

## Cas limites / limites connues

- `n < 0` : `erreur_runtime` pour `log10`, `derniers_non_nuls`,
  `premiers_chiffres` ; `0` pour `zeros_finaux`.
- `digits = 0` ou `count = 0` ou `k = 0` : dégénérés gérés.
- `premiers_chiffres` : si la vraie mantisse est à moins de la borne
  d'erreur d'une puissance de 10 (frontière entre `99…` et `10…`), le
  dernier chiffre peut rester ambigu — augmenter `precision` tranche.
- `derniers_non_nuls` grand `n` + grand `k` : correct mais coûteux (tableau
  ci-dessus).
- Aucun passage par `f64` dans les calculs ; `Gq::chaine()` peut afficher
  une forme rationnelle réduite (le dénominateur n'est pas garanti être
  `10^digits` après canonicalisation) : reconstruire la décimale par
  `num · 10^digits / den`.
