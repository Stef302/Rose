# RAT_FINAL — coût des rationnels exacts, attaque glb4 (CONSTAT DATÉ, 20/11/2026)

**Auteur** : sous-agent `glb4`. **Aucun commit, aucune publication, aucun
`rose/rose` remplacé de ma part.** Un seul fichier source possédé et modifié :
**`rose/src/i128x.rs`**. Binaire construit dans `/tmp/opencode/glb4_agents/`
avec `CARGO_TARGET_DIR` isolé, jamais copié dans l'arbre. Instrumentation
temporaire **retirée** avant livraison (`grep PROF|rdclk` → 0).

> **Verdict : porte « diviser le poste rationnel par 2 » NON ATTEINTE.**
> Gain réel mesuré : **×1,861 (c16) / ×1,865 (c6)** sur la section rationnelle
> interne, soit 826 → 443 µs. La cible était ≤ 413 µs : il manque ~7 %.
> Toutes les autres portes de validation sont franchies. Aucune triche :
> pas de mémoïsation de résultat, pas de formule fermée, pas de valeur codée
> en dur, 11 valeurs du global vérifiées à chaque tirage.

---

## 0. Protocole réellement exécuté

| Élément | Mesure |
|---|---|
| HEAD annoncé | `902fbbf` |
| HEAD final | `77ea7c7` — **la session parente a commité mon travail de mi-parcours pendant ma mesure** ; je n'ai fait aucun `git commit`. Mon seul résidu non commité est le `thread_local SCRATCH_DIV` (11 lignes). |
| Binaire témoin | `/tmp/opencode/glb4_agents/rose_base`, SHA256 `790d80dabaf9f020324c877a9f3d2a090985bf041b480bb24430c5f2d8ce37b4` |
| Binaire final | `/tmp/opencode/glb4_agents/rose_final`, SHA256 `8b3e91581af1ac2d6719d98ff839bbcd8ab353af132b587bbe52cbca3fed676d`, static-pie |
| Pinage | **`taskset -c 6` ET `taskset -c 16`**, capacités re-mesurées 997 / 1017 |
| Échantillons | **9 par implantation et par cœur**, entrelacés en rotation avec alternance A/B à chaque répétition, médianes ; salves A/A finales pour le bruit |
| Horloges | H2 = `temps()` interne (ns) ; H1 = mur `time.perf_counter_ns()` ; décomposition = `cntvct_el0` (cycles brutaux, relatifs) |
| Valeurs | le grand rationnel de la section vérifié à **chaque tirage** ; 11 lignes du global vérifiées à chaque tirage |
| Fenêtre | `2026-11-20T07:5xZ` → `09:0xZ`. **L'horloge de la machine annonce le 20/11, pas le 21/11.** Tout chiffre ci-dessous est un constat daté, à re-mesurer après tout changement. |
| Bruit | A/A final H2 : 439 904–443 871 ns (c16), 436 911–441 488 ns (c6) ; écart A/A < 1,1 %. Le mur est très bruité (0,85–1,36×) : **seul H2 sert aux ratios**. |

---

## 1. Décomposition (instrumentation temporaire, retirée avant livraison)

Compteurs `AtomicU64` + lecture de `cntvct_el0` insérés dans `i128x.rs`,
vidés à la dernière itération (`c = 1000`). Micro-banc :

```
$a = temps()
$r = 1 / 3
pour c de 1 vers 1001 { $r = $r + 1 / c }
$b = temps()
>> $r
>> $b - $a
```

### 1.1 État initial (témoin, code GLB1), cycles cumulés sur 1000 additions

| Poste | cycles | % de l'addition |
|---|---:|---:|
| `pgcd_grand_petit(&n2.abs(), c)` — 2ᵉ PGCD | **169 072** | **33 %** |
| `d1 = da.clone()/g` | 94 016 | 18 % |
| `pgcd_grand_petit(da, c)` — 1ᵉʳ PGCD | 92 384 | 18 % |
| `n2 = na*c1 ± d1` | 83 984 | 16 % |
| réduction `n2/g2, d2/g2` | 19 824 | 4 % |
| `d2 = da*c1` | 19 264 | 4 % |
| **`gmpq_add` total** | **507 904** | 100 % |
| `gmpq_div` (construction de `1/c`) | 160 096 | — |
| `Gq::neuf` (4 allocations/itération) | 90 864 | — |
| `canonicalize` | 3 040 | — |

Total ≈ 762 000 cycles ≈ 827 µs mesurées. **Goulot #1 = le second PGCD
`gcd(N, c)` sur un grand nombre (33 % de l'addition)**, devant les deux
divisions du dénominateur (36 % réunis). L'affichage final du rationnel
(`>> $r`) coûte **17 µs (2 %)** : hors périmètre et négligeable.

### 1.2 Fait structurant vérifié hors ligne

Simulation avec `fractions.Fraction` sur les 1000 itérations, **sans toucher
au code** :

- `g = gcd(da,c)` vaut `c` dans **783 cas**, `1` dans 179, autre dans 38 ;
- le second PGCD est > 1 dans seulement **41 cas / 1000 (4,1 %)** ;
- **condition `da % g² == 0` : 303 cas, 0 contre-exemple** (théorème ci-dessous).

Donc 96 % du goulot #1 était payé pour rien, et 30 % pouvait être prouvé
sans aucun calcul sur le grand nombre.

---

## 2. Décisions structurelles mesurées

Dix décisions testées, sept retenues. Chaque ligne est un A/B entrelacé 9
échantillons, c16 **et** c6, avec valeur vérifiée ; les gains sont ceux de
l'étape, pas des cumuls.

### 2.1 Retenues

| # | Décision | Gain H2 mesuré |
|---|---|---|
| 1 | `pgcd_grand_petit` gère le signe sur le **petit** reste `(n%c).abs()` : suppressions du clone `.abs()` de `n2` | ×1,01 |
| 2 | Un seul `div_rem(da, c)` fournit quotient **et** reste ; le quotient devient `d1` quand `c | da` (78 % des cas), plus de `da/g` séparé | **×1,15** |
| 3 | Division par **`c²`** (un seul passage) : le reste `r2` donne `g`, `d1`, et la preuve `g² | da` → **second PGCD sauté** dans 30 % des cas | ×1,05 |
| 4 | Voie rapide `gmpq_div` pour `(a/1)/(b/1) = a/b` : deux multiplications BigInt, deux clones et un PGCD BigInt supprimés | **×1,26** (`DIV` 155 k → 16 k cycles) |
| 5 | `mod_petit` : résidu par lecture directe des limbs, sans quotient alloué | ×1,004 |
| 6 | Pool `thread_local` de réutilisation des blocs `Gq` (`neuf`/`Drop`) —allocation, pas mémoïsation de résultat | ×1,045 |
| 7 | **Opérations en place** : `add_rat_inv_small` vole les deux buffers BigInt de l'opérande (unique appelant dans `interp.rs`, un `Gq` jeté juste après) ; plus de clone de `n2`/`d2` | ×1,02 |
| 8 | Récurrence de résidu à divisions **64 bits** au lieu de `u128 % u64` par limb | ×1,08 |
| 9 | `div_rem_petit` maison par digits 32 bits (diviseur `c² ≤ 10⁶`, un `udiv` 64 bits par digit) | ×1,02 |
| 10 | Buffer de quotient `thread_local` réutilisé dans `div_rem_petit` | ×1,02 |

**Théorème utilisé en 2.3** (preuve dans le code) : avec `g = gcd(da,c)`,
`d1 = da/g`, `c1 = c/g`, `pgcd(d1,c1)=1`, `pgcd(na,da)=1`, si `g² | da`
alors pour tout `p | c` : ou `p | c1` et alors `p ∤ d1` donc `p ∤ N` ; ou
`p | g` et alors `p | d1` donc `N ≡ na·c1 (mod p)` avec `p ∤ na` et
`p ∤ c1`. Donc `gcd(N,c) = 1`. La réciproque est fausse et n'est pas utilisée.

### 2.2 Rejetées à la mesure (honnêteté)

| Décision | Résultat |
|---|---|
| Résidu par digits 32 bits dans `mod_petit` | **×0,94** (régression c16 et c6) |
| Hybride « division par `c` + preuve via `q mod c` » | **×0,90** — le résidu sur `q` coûte plus que la reconstruction économisée |
| `BigInt::from(BigUint::new(qd))` au lieu de `from_slice` | **×0,61** (forte régression) |
| `div_rem` explicite au lieu de `%` dans le 2ᵉ PGCD | ×0,98 |
| `RUSTFLAGS=""` (déjà vu en glb3 : casse le `crt-static`) | non retenté |

---

## 3. Gains réels c6 + c16

| pin | témoin H2 | final H2 | **gain H2** | témoin mur | final mur | gain mur (bruité) |
|---|---:|---:|---:|---:|---:|---:|
| c16 | 825 920 ns | **443 871 ns** | **×1,861** | 1,633 ms | 1,202 ms | ×1,36 |
| c6 | 824 639 ns | **442 207 ns** | **×1,865** | 1,889 ms | 1,519 ms | ×1,24 |

**Porte « ×2 » : NON ATTEINTE.** Cible 412 960 ns (c16) / 412 320 ns (c6) ;
obtenu 443 871 / 442 207 ns, soit **+7,5 % / +7,3 %** au-dessus de la cible.

Banc global complet (`bench_global.rose`) : **rc = 0**, 12 lignes, **11 valeurs
identiques** à chaque tirage. Le chrono interne du banc n'a que la résolution
de la milliseconde (4 ms → 3 ms) ; le mur est trop bruité pour en tirer un
ratio fiable (1,00 c16 / 0,91 c6 sur cette salve). Le gain imputable reste
donc celui de la section rationnelle, mesuré au nanosecond.

---

## 4. Reste chiffré — pourquoi la porte n'est pas atteinte

Dernière décomposition instrumentée (cycles, add_rat_inv_small comprise) :

| Poste | cycles | commentaire |
|---|---:|---|
| `gmpq_add` complet | 341 344 | cœur du poste |
| ├─ division du dénominateur (`div_rem_petit`) | 145 536* | *inflation possible : `asm!` sans `nomem` = barrière d'optimisation, les A/B directs donnent ×1,02 seulement |
| ├─ second PGCD résiduel `mod_petit` | 73 120 | les 70 % de cas non prouvés ; le saut ne couvre que 30 % |
| ├─ opérations en place finales | 28 016 | |
| ├─ reconstruction de `d1` | 14 880 | |
| `Gq::neuf` (instrumenté) | 90 992 | ≈ 50 000 réels, le reste est l'overhead des compteurs |
| `copier_de` (clone `vers_q`) | 29 696 | 15 cycles/clone, immatériel |
| `gmpq_div` (déjà accéléré) | 15 760 | |
| **total mesuré** | ≈ 477 000 | ≈ 443 µs |

Le second PGCD ne peut pas être supprimé dans les 70 % restants sans un
passage supplémentaire sur `na` (`na mod c`), dont le coût mesuré annule
l'économie (décision 2.2). Une approche à facteurs premiers du dénominateur
pourrait le contourner, mais elle exigerait un état de factorisation persistant
(= mémoïsation, interdite) et un changement de représentation de `Gq`, hors
périmètre. Les 4 % de réductions réellement nécessaires sont, eux, inévitables.

---

## 5. Validation (preuves de sortie réelles, binaire final)

| Gate | Résultat |
|---|---|
| `cargo build --release --bin rose` (target dir isolé, `RUSTFLAGS` non posé) | **OK**, static-pie, SHA256 `8b3e9158…d676d` |
| `cargo test --release` | **431 passed, 0 failed** (220 + 204 + 0 + 6 + 1 + 0), 6 ignored |
| Jardin `jardins/tests/premiers.rose` | `── jardin/premiers : 20 tests, 0 échec ──` |
| `fib(25)` | `75025` |
| Point fixe, 2 voies (`rose rir compilateur/compilateur.rose` et émission directe) | hash `5d5d1a30df49db5598afe34e8507d8ff` et `d0ef3714fd9765c3990c9714446b09a2`, **identiques témoin et final**. ⚠ le hash `b3ed9262…` cité dans les docs GLB antérieures ne correspond pas à l'arbre courant (`compilateur/compilateur.rose`) : ce qui est prouvé ici est l'égalité base = final, pas la valeur historique. |
| 9 domaines multi, c16 **et** c6 | 387 lignes × 2 cœurs, **0 valeur différente** du témoin (43 paires domaine/implémentation) |
| `bench_global.rose` | **rc = 0**, 12 lignes, **11 valeurs identiques** vérifiées à chaque tirage |
| Sorties rationnelles aléatoires | **420 lignes**, `diff` vide entre témoin et final |
| Correctifs antérieurs | `gmpq_add` voie rapide GLB1 (`add_rat_inv_small`) **préservé et étendu**, `f9b0966` (affichage rationnel) intact, `pgcd_grand_petit` conservé comme repli |
| Propreté | aucun `PROF`/`rdclk`/`eprintln` résiduel ; aucun commit ni publication de ma part ; aucun `.rose` du dépôt modifié |

---

## 6. État final factuel

- **Fait** : `rose/src/i128x.rs` — voie rapide exacte étendue (division par `c²`,
  preuve `g²|da`, opérations en place, `gmpq_div` entier/entier, pool `Gq`,
  résidu 64 bits, division 32 bits).
- **Vérifié** : gain ×1,86 sur la section rationnelle (c6 et c16), 431 tests,
  jardin 20/20, fib25, point fixe ×2, 0 divergence sur 9 domaines ×2 cœurs,
  11 valeurs globales inchangées, 420 sorties aléatoires identiques.
- **Non atteint** : la division par 2 du poste (7,5 % d'écart). Cause chiffrée
  §4 : le second PGCD résiduel (73 k cycles) et la division du dénominateur
  dominent, et la seule voie structurale pour les supprimer sort du périmètre
  (`i128x.rs` seul, pas de mémoïsation).
- **À re-mesurer** : tout changement de code, de topology ou de quantification
  invalide ces chiffres. Le commit `77ea7c7` a été fait par la session parente,
  pas par ce sous-agent.
