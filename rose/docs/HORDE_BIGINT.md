# HORDE_BIGINT — bigint performant dans le moteur auto-hébergé (J6 §11)

**Constat daté : 23/11/2026.** Worktree exclusif
`/tmp/opencode/horde_bigint2`, HEAD de départ `101b48d25a85a3a4a5be80232af9dda7bff037f1`.
Aucun commit, aucun push. Trois fichiers créés, **aucun autre touché** :

| Fichier | Lignes | Octets | md5 |
|---|---:|---:|---|
| `compilateur/bigint_rose.rose` | 795 | 22 346 | `f50937ade524ca1d3892f7d5fa5b0113` |
| `compilateur/tests_bigint_rose.rose` | 579 | 203 228 | `b21dd04057575bd6f356150bab27fbe2` |
| `rose/docs/HORDE_BIGINT.md` | ce document | — | — |

Toutes les commandes longues sont passées par
`bash scripts/garde_fou_mem.sh 12000000 <commande>`. Moteur de référence :
`/home/clausake/Stefan/ROSE/rose/target/release/rose` (Rose 0.1.73, aarch64).

---

## 1. Point de départ mesuré

Le repli exact actuel (`compilateur/moteur_valeurs.rose` +
`compilateur/moteur_ops.rose`) représente les grands entiers en **chiffres
décimaux** (`[6, signe, cnt, d0…]`, base 10) et calcule :

- l'addition par retenues décimales (correct) ;
- la multiplication par la **méthode russe** (additions + divisions par 2) ;
- la division/modulo **uniquement par un petit entier** (`bz_div_small`,
  `bz_rest_small` : l'opérande `b` est réduit à un `entier` via `o_bzvers`) ;
- la puissance par élévation au carré, avec le coût de la méthode russe ;
- **aucun PGCD**, aucune division bigint/bigint, aucune fenêtre k-aire.

Les documents `rose/docs/OPT_ALGOS.md` et `rose/docs/OPT_BIGINT.md` identifient
Lehmer, Sinclair, k-aire et NTT comme axes d'optimisation dans le **moteur
Rust** (`rose/src/i128x.rs`). Ce chantier porte ces idées dans le **sous-ensemble
G5**, où les contraintes sont différentes (voir §2).

---

## 2. Décision de représentation : base 10^8, pas 10^9 ni 2^32

### 2.1 Mesure du débordement G5 (décisif)

L'ABI G5 code un entier `n` en `n << 3` (`rose/docs/G5_ABI_VALEURS.md` §2).
L'émetteur `compilateur/emetteur_elf.rose` (:1921) implémente `*` par
`mulr` (64 bits) puis `lsr #3` : le produit calculé est `(n·2^3)·(m·2^3)`,
soit **`n·m·64`**. Il faut donc `n·m < 2^58 ≈ 2,88·10^17` pour ne pas déborder.

| Base | Produit maximal de deux limbs | Compatible G5 ? |
|---|---:|---|
| 10^9 (demandée) | ≈ 10^18 | **NON** (> 2,88·10^17) |
| 2^32 (demandée) | ≈ 2^64 | **NON** |
| **10^8 (retenue)** | ≈ 10^16 | **OUI** |
| 2^28 (alternative) | ≈ 2^56 | OUI, mais conversion décimale plus chère |

Preuve exécutée avec une première version en base 10^9, mêmes entrées,
**natif (Rust) vs ELF C0** :

```
mul  natif ...84163228417754774603769118715729759701478493151537282416640
mul  ELF  ...210950675600458049267099147324168047614341439793282416640   ← divergent
gcd  natif = 2     gcd  ELF = 1                                          ← divergent
pow3^200 natif = 2656139888758747693…90044001
pow3^200 ELF   = 2656139888749834845…931270817                         ← divergent
```

Après passage à 10^8 (tous les produits < 10^16), les trois colonnes sont
devenues **octet pour octet identiques** natif/ELF (§4). Le choix 10^9/2^32
de la consigne est donc techniquement impossible sous l'ABI G5 actuelle ;
10^8 en est le plus proche analogue décimal.

### 2.2 Format

```
[signe, cnt, l0, l1, ..., l(cnt-1)]
signe ∈ {0, -1} ; cnt >= 1 ; limbs little-endian, 0 <= li < 10^8 ;
l(cnt-1) != 0 sauf le zéro unique [0,1,0].
```

Discipline G5 respectée : aucune globale, aucun `[]` littéral, aucun
`retour [ … ]`, aucune affectation par index, aucun paramètre nommé `i`,
ni `cles`/`concatener`/`abs`/`min`/`max`. **Toute liste modifiée est copiée
dans un local d'abord** : sous G5, `joindre(paramètre, x)` ne modifie pas la
liste de l'appelant (bug mesuré : table de fenêtre réduite à l'entrée 0 →
SIGILL sur `bi_pow` avec `e >= 256`).

---

## 3. Algorithmes retenus / rejetés

Banc : `millisecondes()`, 3 salves entrelacées, médianes, moteur Rose
interprété (Rust). Entrées 200 bits sauf indication contraire.

### 3.1 Addition / soustraction / comparaison / conversion — RETENUES

Retenue limb par limb en base 10^8 (somme max ≈ 2·10^8), emprunt pour la
soustraction de magnitudes, comparaison par `cnt` puis limbs de poids
décroissant. `bi_texte` regroupe les limbs par paquets de 8 chiffres ;
`bi_depuis_chaine` les reconstruit de droite à gauche.

**La comparaison de chaînes calculées (`sous_chaine(s,0,1) == "-"`) s'est
révélée non fiable sous le JIT** (mesuré : condition fausse sur `"-1"`) :
`bi_depuis_chaine` compare le **code** du premier caractère (`ordre(...) == 45`).

### 3.2 Multiplication — ÉCOLE RETENUE, Karatsuba REJETÉ

`bi_mul` = produit école rangée par rangée : `bi_mul_small(a, b[j])` puis
accumulation par `bi_add_shift(acc, row, j)`. Karatsuba est **implanté**
(`bi_mul_karatsuba`) mais le seuil est fixé à 999 999 (jamais déclenché).

Banc `MUL limbs=n, reps=r` (ms, médiane de 3) :

| n limbs | reps | Karatsuba seuil 4 | seuil 16 | seuil 64 | **École (seuil 999999)** |
|---:|---:|---:|---:|---:|---:|
| 8 | 400 | 1093 | 304 | 216 | **215** |
| 16 | 200 | 2056 | 1311 | 346 | **346** |
| 32 | 100 | 4484 | 4367 | 674 | **608** |
| 64 | 40 | 9304 | 9170 | 4940 | **902** |
| 128 | 10 | — | 13704 | 14104 | **865** |
| 256 | 4 | — | 37133 | 38147 | **1353** |

À 256 limbs, Karatsuba est **27× plus lent** : G5 interdit l'écriture par
index, donc chaque mise à jour d'un limb recopie une liste entière ; les
recursions et les `bi_add_pos` intermédiaires coûtent plus que l'économie
asymptotique. Verdict : **rejeté, chiffres à l'appui**.

### 3.3 Division — KNUTH D RETENU

`bi_divmod_pos` = Knuth algorithmique D en base 10^8 : normalisation par
`f = B/(v_top+1)`, estimation `qhat = ⌊(u1·B+u2)/v_top⌋`, correction de
Jebelean bornée à 4 pas, soustraction du produit décalé, puis division du
reste par `f` pour dénormaliser. Le quotient et le reste sont rendus dans
**une seule liste plate** `[qcnt, q…, rcnt, r…]` (les listes Rose sont
homogènes : pas de bigint dans un bigint).

Deux bogues détectés et corrigés pendant la validation :
1. la soustraction n'était faite que quand `qhat` était trop grand — le
   reste rendait l'entrée `a` elle-même (`1000000001 / 10^9` → reste
   `1000000001`) ;
2. `nu` était calculé une fois alors que `u` rétrécit à chaque soustraction
   → accès hors bornes à partir du 3e quotient.

`bi_div`/`bi_mod` signés reproduisent la sémantique Rose : quotient FLOOR
et reste du signe du diviseur. Ce sont des capacités **nouvelles** : le repli
décimal ne sait diviser que par un petit entier.

### 3.4 Puissance — FENÊTRE K-AIRE RETENUE

`bi_pow` : table plate `[sg0,cnt0,limbs…,sg1,…]` de `base^i` pour
`i < 2^w`, construite par carrés ; lecture de l'exposant **de gauche à
droite** en base `2^w` (`r ← r^(2^w) · tab[d_i]`). `w = 1` sous 2^8,
`w = 2` sous 2^16, `w = 4` au-delà. `bi_pow_binaire` est conservée pour le
banc.

| Banc (ms, médiane de 3) | k-aire | binaire | Gain |
|---|---:|---:|---:|
| `3^200`, 60 appels | 31 | 38 | **1,23×** |
| `3^5000`, 1 appel | 70 | 232 | **3,3×** |

`w ≥ 5` n'a pas été retenu (table plus coûteuse que l'économie de fenêtres,
conformément à `OPT_ALGOS.md` §2.2). Un premier essai « droite à gauche »
donnait `3^256 = 3` : la fenêtre doit être consommée depuis le chiffre le
plus significatif (corrigé et testé).

### 3.5 PGCD — EUCLIDE RETENU, Lehmer REJETÉ

`bi_gcd_euclide` = Euclide sur la division de Knuth D. `bi_gcd` contient une
implantation complète de **Lehmer** (fenêtre sur les 4 limbs de tête,
matrice unimodulaire à coefficients bigints signés, garde de progression,
repli Euclidien) ; elle est exacte (PGCD identique à Euclide sur les bancs)
mais **mesurée plus lente** :

| Banc (ms, médiane de 3) | Lehmer | Euclide | Verdict |
|---|---:|---:|---|
| PGCD 200 bits, 100 appels (avant bascule) | 1173 | 842 | Lehmer 1,39× plus lent |
| PGCD 10 000 bits, 10 appels | 91 117 | 69 973 | Lehmer 1,30× plus lent |

La fenêtre de 2 limbs native (première tentative) faisait **croître** les
nombres (7 limbs → 9 limbs en 8 itérations) : rejetée. Le seuil de la
version 4-limb est donc mis hors portée (`u[1] <= 100000`) : `bi_gcd`
délègue à Euclide, comme dans le moteur Rust où Lehmer ne gagne qu'au-delà
de ~1 024 bits **avec des limbs machine**, pas avec des listes G5.

### 3.6 Comparaison au repli décimal actuel

Mêmes entrées (61 chiffres ≈ 2^200), moteur Rose interprété, médiane de 3 :

| Opération | Nouveau (base 10^8) | Repli décimal (base 10) | Gain | Pic RSS |
|---|---:|---:|---:|---|
| Addition, 3 000× | 63 ms | 247 ms | **3,9×** | — |
| Multiplication, 300× | 66 ms | 7 196 ms | **109×** | 7 352 Ko vs 5 100 Ko |
| Multiplication, 100× (61 chiffres) | 50 ms | 2 330 ms | **47×** | 7 200 Ko vs 5 100 Ko |

Le pic RSS augmente de ~41 % (5,1 → 7,2 Mio) : c'est le coût des listes de
limbs plus denses et des temporaires de `bi_add_shift`. Aucun pic au-delà
de 12 000 000 Ko (garde-fou), et l'ELF de test complet reste à 68 544 Ko.

---

## 4. Preuves d'exécution

### 4.1 Chaîne de compilation

```
C0 construit depuis CE worktree :
  rose rir compilateur/rose_natif.rose -> rose_natif.rir
  rose compilateur/emetteur_elf.rose rose_natif.rir -> C0.elf
```

⚠ Le `C0.elf` ainsi construit à `101b48d` rejette **tout** source avec
« index hors des limites des arguments » (reproduit sur `>> 1+1`), alors que
le C0 figé du jalon J6 (`/tmp/opencode/j6_agents/C0_j6.elf`, md5
`7bb091200cf927022eb9d86a548d2502`) compile correctement. La compilation
G5 ci-dessous utilise donc ce dernier binaire, dont la provenance J6 est
tracée dans `rose/docs/J6_MOTEUR_ROSE.md` §3.1. Aucun fichier du dépôt n'a
été modifié pour contourner ce point.

```
rose verif compilateur/bigint_rose.rose                    -> vérifié (rc=0)
rose verif <bigint + tests + pilote>                       -> vérifié (rc=0)
C0_good tests_g5.rose -> tests_g5.elf  3 375 304 o  md5 1dc57aea09be47e8e264e423b6a8d65b
```

### 4.2 Batterie de 120 cas

`compilateur/tests_bigint_rose.rose` contient deux batteries sur **les mêmes
120 cas** :

- `lancer_tests()` compare aux **oracles décimaux** générés par Python
  (arithmétique illimitée, identique aux BigInt Rust), exécutable sous
  l'interpréteur Rust **et** sous l'ELF C0 ;
- `lancer_tests_natifs()` recalcule les 7 résultats avec les opérateurs
  natifs de Rose (`+ - * div % pgcd ^` sur ses entiers BigInt) et vérifie
  que l'oracle lui-même est conforme au moteur Rust.

Couverture : 0, ±1, nombres de 2^64 à 2^200, négatifs, nombres de Carmichael
(561, 1105, 1729, 2465, 2821, 6601, 8911, 10585, 15841, 29341), puissances
de 2/3/10, exposants 0, 1, 64, 65, 200, 255, 256, 300, 65536, 65537, 70000
et 100 paires pseudo-aléatoires dans [0, 2^200) (graine 20261123).

```
Interpréteur Rust : NATIF cas=120 echecs=0 / ORACLE cas=120 echecs=0
                    2,77 s, RSS 15 164 Ko
ELF C0             : ORACLE cas=120 echecs=0 / G5 echecs=0
                    0,05 s, RSS 68 544 Ko
```

Parité supplémentaire sur un pilote de 9 impressions (addition,
multiplication, division, modulo, PGCD, `3^200`, négatif) : `diff` natif/ELF
**aucune différence**, octet pour octet.

### 4.3 Non-régression

| Contrôle | Résultat |
|---|---|
| `rose verif` bibliothèque, banc, concaténation complète | `vérifié` (rc=0) |
| C0 → ELF puis exécution de la batterie | rc=0, 0 échec |
| Point fixe émetteur (deux passes) | `b3ed9262dd54776a6dbd07bc12368d0e` ×2, `cmp` identique |
| `RUST_TEST_THREADS=1 bash scripts/validation_finale.sh` | voir §6 |
| `git status --short` | 3 fichiers nouveaux, aucun fichier interdit modifié |

---

## 5. Reproductibilité

```
cd /tmp/opencode/horde_bigint2
B=/home/clausake/Stefan/ROSE/rose/target/release/rose
$B verif compilateur/bigint_rose.rose
cat compilateur/bigint_rose.rose compilateur/tests_bigint_rose.rose \
    /tmp/opencode/horde_bigint2_art/drv_nat.rose > /tmp/opencode/horde_bigint2_art/tests_nat.rose
$B /tmp/opencode/horde_bigint2_art/tests_nat.rose
cat /tmp/opencode/horde_bigint2_art/tests_g5.rose > /dev/null   #voir pilote G5
/tmp/opencode/horde_bigint2_art/C0_good.elf /tmp/opencode/horde_bigint2_art/tests_g5.rose \
    > /tmp/opencode/horde_bigint2_art/tests_g5.elf
/tmp/opencode/horde_bigint2_art/tests_g5.elf
```

Le pilote G5 est minimal : `$n = lancer_tests()` puis
`>> "G5 echecs=" + chaine($n)`. Les pilotes et bancs générés vivent hors
dépôt dans `/tmp/opencode/horde_bigint2_art/`.

---

## 6. État final — validation verte

`RUST_TEST_THREADS=1 bash scripts/garde_fou_mem.sh 12000000 bash
scripts/validation_finale.sh` (journal
`/tmp/opencode/horde_bigint2_art/validation_finale2.log`) : **RC=0, 0 échec**.

| Étape | Résultat mesuré |
|---|---|
| 1. build + tests Rust | 314 + 327 tests, **0 failed** (3 ignored chacun), rc=0 |
| 2. point fixe émetteur | `b3ed9262dd54776a6dbd07bc12368d0e` ×2, OK |
| 3. AOT | 75 OK, 0 ECHEC, aucun refus AOT dans `rose/src/` |
| 4. JIT Cranelift | 25/25 |
| 5. WASM / RVM | 92 o / 317811 |
| 6. batterie AZ | 31 CONFORME, 0 non-conforme |
| 7. HOST-1 | fib = 317811 |

**Incident environnemental (tracé, non imputable aux fichiers livrés)** : la
première salve donnait « AZ 0 CONFORME, 31 BLOQUE, rc_nat=127 ». Cause prouvée
par `/tmp/opencode/vf_az.log` : `tests_selfhost/lancer_az.sh` utilise en dur
`$ROOT/rose/rose`, absent de ce worktree (le binaire n'y est pas versionné) →
`md5sum: …/rose/rose: No such file or directory`. Un lien symbolique
**temporaire** `rose/rose -> /home/clausake/Stefan/ROSE/rose/target/release/rose`
a été posé pour la salve canonique, puis retiré : `git status --short` ne
montre que les trois fichiers créés par la mission.

---

## 7. Reste (hors périmètre de cette mission)

- Éventuellement réétudier Karatsuba **avec une représentation à accès
  direct** (tableaux de limbs) : avec des listes G5 sans écriture
  par index, les mesures §3.2 sont sans appel.
- Porter la fenêtre k-aire sur une base 2^28 (productions de limbs
  alignées sur des bits) pour mesurer l'économie sur la conversion décimale.
- Intégrer `bi_*` dans `moteur_ops.rose` (fichier possédé par un autre agent
  : non touché ici) pour remplacer les appels à `bz_mul`/`bz_div_small`.
