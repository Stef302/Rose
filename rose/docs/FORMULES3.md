# F3 — formules optimales sur les primitives BigInt / Rationnel (constat daté, 21/11/2026)

Sous-agent `f3`. **Aucun commit, aucune publication, aucun `rose/rose` du dépôt
remplacé par mes soins.** Périmètre source : **`rose/src/i128x.rs` uniquement**
(2 fonctions, +32/−4 lignes) et ce document. Les fichiers `jit.rs`, `interp.rs`,
`front.rs`, `run.rs`, `rir.rs` appartiennent à d'autres agents (F1/F2/G5) et
n'ont pas été touchés. Toutes les mesures ont été faites dans un arbre isolé
(`git archive HEAD` → `/tmp/opencode/formules3/tree`) avec **un seul fichier
différent** entre les deux côtés du A/B, afin que les éditions concurrentes
des autres agents ne polluent pas le comparatif.

Aucun chiffre ci-dessous ne vient d'une spec, d'un README ou d'une estimation :
tout est mesuré (horloge `perf_counter` autour du sous-processus, ou `Instant`
in-process pour les micro-bancs).

---

## Verdict en une ligne

Sur les quatre hypothèses de la mission, **deux étaient déjà implémentées**
(Stein sur les grands nombres via `num-bigint`, multiplication NTT et
conversion décimale divide-and-conquer via `mul_rapide`), **une est vérifiée**
(borne de Robbins de Stirling : 0 violation) et **une a produit un gain réel** :
la voie chaude rationnelle (`div_rem_petit` + `pgcd_scalar`) gagne **8,9 %**
(médiane appariée, 32/32 tirages plus rapides, ~6× le bruit A/A) sur le banc
rationnel amplifié. Le banc **global ne bouge pas** (−0,45 %, dans le bruit) :
la section rationnelle n'y pèse qu'une petite fraction et F2 la replie de
toute façon à la compilation dans l'arbre courant.

---

## 1. Protocole réellement exécuté

| Élément | Valeur |
|---|---|
| Arbre de mesure | `git archive HEAD` (019433a…) + mon seul `i128x.rs` |
| Pinage | `taskset -c 6` (Cortex-X925, `cpu_capacity=997`) **et** `-c 16` (capacity 1017) |
| Harnais | `/tmp/opencode/formules3/aaa.py` : paires A/B entrelacées, ordre alterné à chaque répétition (annule dérive thermique et avantage cache du « second ») |
| Statistique primaire | **médiane des deltas appariés B−A**, MAD, et compte de signes — robuste aux pics (un pic ×5 ne déplace pas une médiane) |
| Bruit A/A | global : MAD 0,137–0,370 ms sur ~3,5 ms ; rationnel : MAD 0,19–0,36 ms sur ~16 ms (signes ~13/19 → pur bruit) |
| Bancs | `bench_global.rose` (11 valeurs), `rat_heavy.rose` (40× la boucle rationnelle exacte, ~16,5 ms), `rat_only.rose`, micro-banc Rust in-process |
| Binaires | `rose_head_base` sha256 `0f766ff3…` ; `rose_divinv` `76f91669…` ; final `454fc201…` |

---

## 2. Hypothèses 1 à 4 — ce que le code faisait déjà

### 2.1 PGCD binaire (Stein) sur les grands nombres — **déjà fait**

`num-bigint 0.5.1`, `src/biguint.rs:241-269` implémente `Integer::gcd` avec
**l'algorithme de Stein** (`twos()`, décalages, soustractions) ; `bigint.rs:435`
délègue `BigInt::gcd` à ce `BigUint::gcd`. Remplacer l'Euclide par des décalages
sur les grands nombres est donc **déjà le comportement de la dépendance** : aucun
changement possible ici sans réécrire le crate (hors périmètre, et GMP fait
mieux avec Lehmer — non disponible ici).

En revanche le **PGCD scalaire** `pgcd_scalar` (u64, voie chaude de
`add_rat_inv_small`) utilisait encore l'Euclide à `%`. Voir §3.2.

### 2.2 Multiplication rapide — **déjà seuillée, et pas sur les petits nombres**

`mul_rapide::mul` (`src/mul_rapide.rs:96-169`) :
- `num-bigint` fait école → Half-Karatsuba → Karatsuba (≤256 limbs) → **Toom-3
  en place** ;
- notre Toom-3 maison est **mesuré 4× plus lent** (7,6 ms contre 1,8 ms à
  8000 limbs) : il est délibérément désactivé ;
- la NTT parallèle ne prend le relais qu'au-dessus de `SEUIL_NTT_BITS =
  520_000` bits (`:159`), seuil balayé sur l'arbre réel de 1000$ :
  300 k → +12 %, **520 k → +14 %**, 700 k → +11 %, 900 k → +5 %,
  1,5 M → −8 % (`:143-158`).

Donc `mul_rapide` n'est **pas** appelé sur les petits nombres, et c'est
correct : c'est le régime où `num-bigint` est le plus rapide. Aucun seuil à
déplacer. Ce constat recoupe `BIGINTS_RAPIDES.md` §3.3(a) : la multiplication
`a*a` du banc global passe par le noyau NEON / la forme fermée, et les
rationnels par `add_rat_inv_small` — jamais par `mul_rapide`.

### 2.3 Conversion décimale divide-and-conquer — **déjà fait**

`mul_rapide::vers_chaine` (`:1887-2320`) implémente déjà le découpage
récursif **par multiplication NTT** (`T(n)=2T(n/2)+M(n/2)`), mesuré
2,0× plus rapide que la conversion par divisions de `num-bigint` sur
1 000 000! (1 400 ms contre 2 881 ms), avec un seuil de feuille `mul_b1e9`
à 64 limbs lui aussi balayé (`:1977-1988`).

Cas particulier du banc global : la fraction rationnelle (~1441 bits au
numérateur, ~1438 au dénominateur) passe par `Gq::chaine()` → `to_string()`.
`vers_chaine` délègue **lui-même** à `to_string()` sous 10 000 bits
(`:2314-2317`) : les deux voies sont donc littéralement le même code. Aucun
gain à attendre ; laissé inchangé (mesure in-process : 5,7 µs par conversion
de la fraction de 868 chiffres).

### 2.4 Borne de Stirling (Robbins) — **vérifiée, 0 violation**

La série utilisée dans `log10_fact_fixed` (`:1215-1220`, `:1451-1465`) est

```
ln(n!) = (n+1/2)ln n − n + ½ln(2π) + Σ_{j=1}^{m} B₂ⱼ / (2j(2j−1) n^{2j−1}) + Rₘ
|Rₘ| ≤ |B₂ₘ₊₂| / ((2m+2)(2m+1) n^{2m+1})      (n ≥ 1)
```

Vérification numérique indépendante (`/tmp/opencode/formules3/stirling_check.py`,
mpmath 220 dps, n=1..40 × m=1..12, soit 480 points, plus n=10³ et 10⁶) :
**0 violation**, pire ratio `|Rₘ| / borne = 0,99982151` (n=40, m=1). Le ratio
tend vers 1 par valeurs inférieures quand n croît : la borne est **serrée mais
valide**. Le choix de `j_est` (marge de +12 termes, `:1445`) et le garde-fou
`2*j+2 >= bern.len()` (`:1462`) sont donc corrects. Aucun changement.

---

## 3. Changements réellement retenus (2 fonctions, `i128x.rs`)

### 3.1 `div_rem_petit` : réciproque (`umulh`) au lieu de `udiv`

Ancien corps : `qd.push((cur/d64) as u32); rem = cur % d64;` — deux
divisions 64 bits par digit u32. Nouveau corps : `m = ceil(2^64/d)` calculé une
fois, puis `q = (cur·m)>>64`, `r = cur − q·d`, une correction au plus.

*Preuve de la correction* : `m·d = 2^64 + t` avec `0 ≤ t < d`, donc
`cur/d + cur·t/(d·2^64) < cur/d + 1` (car `cur < 2^64`). Le quotient approché
vaut donc `⌊cur/d⌋` ou `⌊cur/d⌋+1`, jamais plus ; un seul `if r >= d { r += d;
q -= 1; }` suffit. `d = 1` est traité à part (`m = 2^64` ne tient pas en u64).

*Exactitude* : comparaison exhaustive sur ~600 diviseurs (1, 2, 3, …, u32::MAX,
aléatoires) × 8 tailles de n (1 à 90 limbs u32), soit ~4 800 cas, contre
`BigInt::div_rem` : **0 divergence** (test temporaire retiré avant livraison).

*Micro-banc in-process* : 238 → **158 ns** par appel sur un dénominateur de
1440 bits (1,51×).

### 3.2 `pgcd_scalar` : Stein au lieu de l'Euclide à `%`

*Micro-banc in-process* : paires u64 aléatoires 115 → **63 ns** (1,83×) ;
paires réelles de `add_rat_inv_small` (`c ≤ 1000`) 6 → **3 ns** (1,76×) ;
résultats identiques sur ~200 000 paires.

### 3.3 Mesures bout en bout (banc rationnel amplifié, `rat_heavy.rose`)

| Comparaison | n | Delta apparié (B−A) | Ratio | Signes | Bruit A/A |
|---|---:|---:|---:|---:|---:|
| A/A témoin | 12 | −0,088 ms | 1,0075 | 11/13 | MAD 0,363 |
| A/A témoin (`divinv`) | 16 | −0,018 ms | 0,9979 | 13/19 | MAD 0,257 |
| base → `div_rem_petit` | 16 | **−1,126 ms** | 0,9313 | **0/32** | MAD 0,265 |
| `divinv` → + Stein | 16 | −0,260 ms | 0,9826 | 25/32 | MAD 0,259 |
| `divinv` → + Stein (confirmation) | 24 | **−0,314 ms** | 0,9715 | **41/48** | MAD 0,190 |
| **base → final (les deux)** | 16 | **−1,547 ms** | **0,9112** | **0/32** | MAD 0,224 |

Gain total **−8,9 %**, reproductible sur c6 (−9,2 %) **et** c16 (−9,1 %),
avec un delta primaire 6,9× le MAD du témoin et 32/32 tirages plus rapides.
C'est au-delà du bruit défini par la consigne : **conservé**.

### 3.4 Banc global — pas de gain détectable

| Comparaison | n | Delta apparié | Ratio | Signes | MAD |
|---|---:|---:|---:|---:|---:|
| base → final | 20 | −0,018 ms | 0,9955 | 16/24 | 0,138 |
| A/A témoin | 20 | +0,108 ms | 1,0201 | 23/17 | 0,149 |

−0,45 % est **moins que le bruit A/A (+2,0 %)** : non revendiqué. Deux raisons
mesurées : (i) la section rationnelle ne représente qu'une fraction des
~3,5 ms du global ; (ii) dans l'arbre courant, F2 (`FORMULES2.md`) recompose
la somme harmonique exacte par PPCM et la replie à la compilation — le calcul
sort de la zone chronométrée. Le gain reste réel pour **tout programme
rationnel non constant**, qui est bien l'objet de la mission.

---

## 4. Candidat testé puis rejeté

`mod_petit` avec modulo final par réciproque (au lieu de `% c`) : micro-banc
62 ns contre 47 ns pour l'original (plus lent, le u128 intermédiaire coûte plus
que l'`udiv` sur ce chemin). **Rejeté**, aucune ligne livrée.

---

## 5. Invariants vérifiés (binaire final `454fc201…`)

| Contrainte | Résultat |
|---|---|
| 11 valeurs du banc global | **bit-identiques** (`diff` sur les 11 premières lignes : 0 écart). Seule la ligne 12 (`millisecondes()`) change, comme attendu. |
| Fraction exacte (~800 chiffres) | ligne 3 identique : 434 chiffres au numérateur, 433 au dénominateur, **868 caractères** au total. |
| Tests | Arbre isolé : **265 + 249 + 6 + 8 + 1 = 529 réussis, 0 échec**. Arbre live final (après les commits concurrents) : **270 + 254 + 6 + 8 + 1 = 539 réussis, 0 échec**. (Un test GUI a été vu une fois rouge en exécution parallèle, puis vert isolé et vert au 2ᵉ passage : flaky, indépendant de `i128x`.) La consigne cite 517 (compte périmé). |
| Point fixe `b3ed9262` ×2 | `rose rir compilateur/emetteur.rose` **et** `rose compilateur/emetteur.rose compilateur/emetteur.rose` → `b3ed9262dd54776a6dbd07bc12368d0e` **×2**, stderr 0 octet des deux côtés. |

---

## 6. Reproduction

```bash
cd /home/clausake/Stefan/ROSE
mkdir -p /tmp/opencode/formules3/tree
git archive HEAD | tar -x -C /tmp/opencode/formules3/tree
cp rose/src/i128x.rs /tmp/opencode/formules3/tree/rose/src/i128x.rs
cd /tmp/opencode/formules3/tree/rose && cargo build --release --bin rose

# A/A/A c6+c16 (le harnais est dans /tmp/opencode/formules3/aaa.py)
cd /tmp/opencode/formules3
python3 aaa.py --a rose_head_base --b rose_final --bench rat_heavy.rose --n 16
python3 aaa.py --a rose_head_base --b rose_final --bench rat_heavy.rose --n 16  # A/A : témoin

# Invariants
./rose_final /home/clausake/Stefan/ROSE/rose/bench_global.rose   # 11 valeurs
(cd /home/clausake/Stefan/ROSE && /tmp/opencode/formules3/rose_final rir compilateur/emetteur.rose | md5sum)
```

**Limites** : c6/c16 sont des cœurs X925 (capacité 997 / 1017) ; les rapports
absolus ne sont pas comparables à ceux mesurés sur d'autres cœurs. Le bruit de
la machine (agents concurrents) a été quantifié par des contrôles A/A dans
chaque salve, jamais supposé.
