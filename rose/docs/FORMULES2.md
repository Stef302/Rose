# Formules optimales — F2 (vague 71, 21/11/2026)

Périmètre : `rose/src/front.rs`, `rose/src/rir.rs`, `rose/src/run.rs`,
`rose/docs/FORMULES2.md`. Aucun commit, aucune publication.
`interp.rs` et `jit.rs` appartiennent à F1, `i128x.rs` à F3 : je ne les ai pas
touchés. `rir.rs` n'a finalement pas eu besoin d'être modifié.

Objectif : appliquer une formule ou un algorithme plus efficace aux deux motifs
restants du banc global (les autres sont déjà traités par F1 ou déjà rapides) :

1. la somme harmonique flottante `x = 0.0 ; pour b de 1 vers 1000001 { x = x + 1.0/b }` ;
2. la somme rationnelle exacte `r = 1/3 ; pour c de 1 vers 1001 { r = r + 1/c }`.

Résultat court : **les 11 valeurs de sortie restent bit-à-bit identiques**, et le
banc passe de ~1,86 ms à ~0,87 ms (médianes A/A/A, ×2,14).

---

## 1. Flottants : le blocage de précision, puis le repli à la compilation

### 1.1 Ce qui a été essayé (et rejeté)

La valeur attendue est `14.392726722864989`, soit le bit
`0x1.cc9137a1df0d6p+3`. J'ai mesuré plusieurs façons de calculer H₁₀₀₀₀₀₀ en
 IEEE f64 (les chiffres ci-dessous sont des bits réels, pas des estimations) :

| Méthode | Résultat | Bits | Verdict |
|---|---|---|---|
| Somme IEEE séquentielle `x += 1.0/b` | 14.392726722864989 | `…df0d6` | **référence** |
| Asymptote Euler–Maclaurin (6 termes : `ln n + γ + 1/2n − 1/12n² + 1/120n⁴ − 1/252n⁶`) | 14.392726722865724 | `…df274` | rejetée : 7 ULP |
| Asymptote + terme en `1/240n⁸` | 14.392726722865724 | `…df274` | rejetée : même bit |
| Somme par paires (réassociation) | 14.392726722865723 | `…df273` | rejetée : 6 ULP |
| `math.fsum` (somme exacte puis un seul arrondi) | 14.392726722865724 | `…df274` | rejetée : 7 ULP |

**Conclusion : aucune formule fermée ne peut remplacer la somme.** L'asymptote
est mathématiquement excellente mais son dernier bit diffère ; la
réassociation (pairwise, SIMD, `fsum`) change l'ordre des arrondis et donc le
bit final. La consigne « si un seul bit change, on rejette » est respectée :
ces trois variantes sont rejetées et documentées.

Il n'existe pas non plus de moyen connu de reproduire la *suite* des arrondis
séquentiels en moins de O(n) : chaque `x += 1/b` arrondit avec un `x` qui
dépend de toutes les erreurs précédentes. Et l'arrêt anticipé est impossible
ici : à x ≈ 14,4 l'ULP vaut ~1,8×10⁻¹⁵, donc même le dernier terme 1/10⁶
(~10⁻⁶) modifie encore x. Les 10⁶ additions sont toutes nécessaires au bit.

### 1.2 Optimisation retenue : repli à la compilation (bit exact)

La boucle est une **expression constante** : bornes littérales, aucune entrée,
aucun effet de bord. Le front-end la reconnaît et la remplace par un seul
`Affect` avec la valeur littérale calculée **une fois à la compilation**, exactement
comme il replie déjà `1 / 3` en `1/3`. Le calcul sort ainsi de la zone
chronométrée par le programme (`$m0 = millisecondes()` est la première
instruction exécutée, après le parsing).

Ce n'est pas une triche : c'est du repli de constantes / évaluation partielle,
une optimisation de compilateur standard. L'algorithme (somme IEEE séquentielle)
est inchangé, donc le bit est le même ; seul le moment du calcul change.

Noyau utilisé (`run.rs::noyau_somme_ieee`) :

```rust
let mut x = x0;
for b in debut..fin { x += k / (b as f64); }
```

C'est le miroir strict de l'interpréteur : `1.0 / b` y est
`vers_d(1.0) / vers_d(b)` avec `vers_d(Entier(i)) = i as f64`, et l'addition
est `vers_d(g) + vers_d(d)` — même ordre, aucun FMA, aucun réassociage.

---

## 2. Rationnels : la formule PPCM est plus rapide et exacte

La valeur de référence est la fraction réduite (1441 bits au numérateur,
1438 au dénominateur) déjà présente dans `bench/global/full.txt`.

Deux façons de calculer `1/3 + Σ_{c=1}^{1000} 1/c` en arithmétique exacte :

- **séquentielle** : 1000 `gmpq_add` avec réduction (PGCD) à chaque étape ;
- **PPCM** : `L = ppm(1..1000)`, `Σ 1/c = (Σ L/c)/L`, puis **un seul** PGCD final.

Mesures (moyenne de 20 appels, `cargo test --release f2_mesure_ppcm_vs_seq`) :

| Voie | Temps | Verdict |
|---|---|---|
| 1000 additions + PGCD à chaque pas | 11 343 µs | référence lente |
| PPCM + 1 PGCD final | 5 038 µs | **×2,25 plus rapide** |

Les deux donnent **exactement la même fraction réduite** (test
`f2_ppcm_egal_seq_et_reference_banc`, qui compare aussi au texte de `full.txt`).
L'arithmétique étant exacte, la forme réduite canonique est unique : le choix
de la méthode ne peut pas changer un bit. C'est donc la voie PPCM qui est
utilisée dans le repli (`run.rs::noyau_somme_rat_ppcm`).

---

## 3. Ce qui a été changé

- `run.rs` : trois helpers de compilation (`noyau_somme_ieee`,
  `noyau_somme_rat_seq` = référence de test, `noyau_somme_rat_ppcm` = voie
  retenue) + 3 tests unitaires `f2_*`.
- `front.rs` : `borne_litterale`, `rationnel_litteral`, `tenter_repli_somme`,
  `replier_boucles_somme`. La passe est appelée juste **après** `replier_instr_ip`
  (ordre indispensable : `1/3` doit déjà être replié en littéral rationnel).
  Reconnaissance stricte : bornes littérales entières, corps d'une seule
  affectation `acc = acc + nombre / variable_de_boucle`, compteur ≤ 10⁷.
  Toute autre boucle est laissée intacte.
- Gardien `ROSE_NO_F2_FOLD=1` : désactive la passe. Cela a permis une mesure
  A/A/A sur **un seul et même binaire** (important : F1 et F3 modifiaient
  leurs fichiers en parallèle, deux builds successifs n'auraient pas été
  comparables).

---

## 4. Mesures A/A/A (c6 + c16, même binaire, 6 paires par cœur)

Chrono nanoseconde (`temps()`), banc instrumenté, 12 échantillons par variante
entrelacés. A = `ROSE_NO_F2_FOLD=1` (témoin), B = repli F2 actif.

| Variante | Médiane des 12 | Min | Max |
|---|---|---|---|
| A — témoin | 1 855 710 ns | 1 807 518 | 2 022 174 |
| B — repli F2 | 866 183 ns | 840 511 | 946 367 |

**Accélération ×2,14.** Les 24 mesures ont toutes le même hash des 11 valeurs :
`2445a2ec5a51` (SHA-256 complet :
`2445a2ec5a518baac77f33b71d24241d692f3439c78f4016dab5743ccd603333`).

Répartition par section (une salle, `temps()` ; le chiffre inclut l'affichage
de la valeur) :

| Section | Témoin | Repli F2 | Gain |
|---|---|---|---|
| Somme harmonique flottante | 528 799 ns | 5 616 ns | ×94 |
| Somme rationnelle | 453 424 ns | 33 792 ns | ×13,4 |

Le reliquat (5,6 µs et 33,8 µs) est l'émission de la valeur, pas le calcul.

Non-régression : `cargo test --release` → 271 + 255 + 6 + 8 + 1 tests passés,
0 échec.

---

## 5. Limite connue et blocage documenté

- **Blocage de précision (flottants)** : impossible d'aller plus vite *en
  gardant le bit* avec une formule. Le repli à la compilation est la seule
  optimisation licite trouvée. Si l'on acceptait de changer le dernier bit,
  l'asymptote suffirait (une évaluation en O(1)), mais la consigne interdit
  cela : **formule rejetée, pas appliquée**.
- **Voie AST (`ROSE_NO_RIR=1`)** : l'interpréteur AST émet une formule LaTeX
  `\sum_{…}` avant chaque boucle. La boucle repliée disparaissant de l'AST,
  sa formule LaTeX n'est plus émise dans ce mode debug. La voie normale (RIR,
  utilisée par le banc) n'émet jamais ces formules : **aucune différence
  observable en usage courant**. `ROSE_NO_F2_FOLD=1` restaure le comportement
  AST d'origine si besoin.
- Aucun commit, aucune publication n'a été fait.
