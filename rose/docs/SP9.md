# SP9 — dernier écart du banc global (CONSTAT DATÉ)

Sous-agent `sp9`. **Aucun commit, aucune publication, aucun `rose/rose` du dépôt
remplacé.** Deux fichiers touchés au total : **`rose/src/i128x.rs` (+13 lignes,
0 suppression)** et ce document. `run.rs`, `jit.rs`, `front.rs`, `rir.rs`,
`interp.rs`, `main.rs`, `Cargo.toml`, `PLAN.md` et les docs des autres agents
n'ont **pas** été modifiés. Les correctifs antérieurs (rationnels `f9b0966`/
GLB1/GLB4, pool `38a8804`, mpz `0b87f77`, table `065a0d2`, VAL1, SPEED1/4/6,
SP8) sont intacts : `git diff` ne montre que les 13 lignes ci-dessous dans mon
périmètre.

> **Verdict : dépassement de Rust NON atteint.** Gain réel mesuré : section
> rationnelle **−19,7 µs c16 / −22,2 µs c6** (×0,961 / ×0,956), banc global
> complet **−24,6 µs c16 / −24,0 µs c6** en différences appariées, soit
> **~1 % du temps interne**. Le ratio Rose/Rust reste ~1,68 interne et
> ~1,78–1,81 au mur. Cause chiffrée du reste §7 : les deux plus gros postes
> (carrés + flottants = 46,5 % du calcul) sont dans `jit.rs` interdit, et
> ~1,0 ms hors chrono est dans `main.rs` interdit. Aucun chiffre de ce document
> ne vient d'une spec ou d'un README.

**Horloge : 20/11/2026 ~19:20–19:55 UTC** (la consigne annonce le 21/11).
L'arbre a bougé pendant la mission : HEAD annoncé `76b0235`, HEAD en fin de
mesure `df41f5d` (agents concurrents sur `jit.rs`, `lib.rs`, `Cargo.lock`,
docs). **Toutes les mesures A/B ont été faites sur deux arbres figés construits
au même instant** : seul `i128x.rs` diffère. Tout chiffre est un constat daté,
à re-mesurer après tout changement de source, de binaire ou de pinage.

---

## 1. Protocole réellement exécuté

| Élément | Mesure |
|---|---|
| Arbres figés | `/tmp/opencode/sp9_agents/src_final_base` et `src_final` (copie de `rose/` au même instant, `target` exclu) ; le seul fichier différent est `i128x.rs` (patch SP9) |
| Binaire témoin A | `rose_final_base`, SHA256 `e40a4c836e9dfb738a8d2489e10eb6c49c1301933f6446e20f8a537f7c29c96b` |
| Binaire final B | `rose_final`, SHA256 `acfb5cd3714f9b5351e21041bf8cb2a1850da174bac0bbf059876a9c58723f58` |
| Construction | `CARGO_TARGET_DIR` dédié par binaire (`target_final_base`, `target_final`) ; aucun binaire copié dans `rose/rose` |
| Pinage | `taskset -c 6` **et** `-c 16`, disponibilité re-vérifiée (`taskset -c … echo` OK) |
| Échantillons | **12 par binaire et par cœur** en rotation entrelacée `A/B/B/A` puis `B/A/A/B` (48 tirs/cœur), médianes **et** différences appariées ; A/A/A de 9 à 18 tirs en plus pour le bruit |
| Horloges | interne `temps()` (ns) sur banc copié hors dépôt ; `millisecondes()` du banc d'origine (ms entiers) ; mur `time.perf_counter_ns` ; `cntvct_el0` pour les micro-benchs |
| Valeurs | 11 lignes du global vérifiées à **chaque tirage** (`/home/…/bench/global/full.txt`) ; 9 domaines × 9 tirages × 2 cœurs = **162 exécutions** |
| Bruit | A/A 18 tirs sur la section rationnelle : médiane 444 256 ns c6 / 445 191 ns c16 (témoin), 422 119 / 423 120 ns (final) ; l'écart A/B (−19 à −21 µs) est supérieur à la dérive médiane entre salves |

Harnais hors dépôt : `/tmp/opencode/sp9_agents/{decomp.py,ratbench.py,global_ns_bench.py,global_bench.py,validate_multi_sp9.py,sp9_bench_suite.py,sp9_bench_englobant.py}`.
Bancs instrumentés copiés hors dépôt : `phase_bench.rose`, `rat_chrono.rose`,
`global_ns.rose`. **Aucun `.rose` du dépôt n'a été modifié.**

---

## 2. Décomposition du binaire courant (A/A, 12 échantillons/cœur)

Banc instrumenté identique à `bench_global.rose`, phases chronométrées en ns.

| Phase | c16 (ns) | % c16 | c6 (ns) | % c6 | Fichier de la voie chaude |
|---|---:|---:|---:|---:|---|
| carrés (10 M) | 540 710 | **23,7 %** | 541 063 | **22,9 %** | `jit.rs` — **interdit** |
| flottants (1 M) | 517 143 | **22,6 %** | 516 558 | **21,9 %** | `jit.rs` — **interdit** |
| rationnels (1 k) | 489 271 | **21,5 %** | 493 295 | **20,9 %** | `i128x.rs` — **1er poste accessible** |
| tables (5 k) | 415 942 | 18,2 % | 435 607 | 18,4 % | `run.rs` (déjà SPEED4/6) |
| RNG (1 M octets) | 171 856 | 7,5 % | 204 656 | 8,7 % | `run.rs` (déjà SPEED4) |
| listes (10 k) | 84 744 | 3,7 % | 90 352 | 3,8 % | `run.rs` |
| E/S | 59 912 | 2,6 % | 77 728 | 3,3 % | `run.rs` |
| fib(28) | 784 | <0,1 % | 1 352 | <0,1 % | `jit.rs` (SPEED1) |
| **Total phases** | **2 282 538** | 100 % | **2 363 107** | 100 % | |

**Goulot #1 = carrés (23,7 % c16 / 22,9 % c6), dans `jit.rs` formellement
interdit.** Goulot #2 = flottants (22,6 % / 21,9 %), également dans `jit.rs`.
Vérification faite que ce second poste est un plancher et non un évitement :
la boucle Rust équivalente compilée à part (`flotbench.rs`, `rustc -O`, 9 tirs)
mesure **512 800 ns c6 / 512 832 ns c16** pour 1 M itérations `x += c/b`,
contre 517 µs dans Rose — l'écart est le coût de la division flottante scalaire,
aucune optimisation de `run.rs` ou `i128x.rs` ne peut l'atteindre.

Le premier poste dans mon périmètre est donc **les rationnels (21,5 %)**.

### 2.1 Où va le temps rationnel (sondes temporaires, retirées)

Instrumentation `cntvct_el0` ajoutée à `i128x.rs` + `run.rs` dans un arbre-sonde,
puis **restaurée** ; 1000 additions `$r = $r + 1/c` (c16) :

| Poste | ns | part |
|---|---:|---:|
| `div_rem_petit(da, c²)` | 109 552 | 29 % |
| `mod_petit(N, c)` — 697 appels / 1000 | 73 696 | 20 % |
| bloc construction `c1,d1,g2` hors division | 51 248 | 14 % |
| `num *= c1` | 18 992 | 5 % |
| divisions finales par `g2` | 18 864 | 5 % |
| `num += d1` | 15 344 | 4 % |
| `gmpq_div` (construction de `1/c`) | **18 768** | **5 %** |
| **total `add_rat_inv_small`** | 376 736 | (bruit d'instrumentation inclus) |

Le dernier poste, ~19 µs pour 1000 appels, est presque entièrement le
`(n % c)` **BigInt avec allocation d'un quotient** dans `pgcd_grand_petit`,
alors que le numérateur y vaut 1. C'est la cible retenue.

---

## 3. Décision structurale livrée

Dans `pgcd_grand_petit` (`rose/src/i128x.rs`), **voie scalaire exacte quand le
numérateur tient dans `u64`** :

```rust
if c != 0 {
    if let Some(nu) = n.to_u64() {
        let mut a = c;
        let mut b = nu % c;
        while b != 0 { let t = a % b; a = b; b = t; }
        return a;
    }
}
```

Euclidienne identique, sur des entiers natifs, **zéro allocation**. Le cas
général BigInt est conservé octet pour octet. Ce n'est **ni** un cas spécial
« n = 1 » codé en dur, **ni** une formule fermée, **ni** une mémoïsation : la
condition est « le numérateur tient dans u64 », valable pour tout programme,
et toutes les additions sont toujours effectuées une par une avec la même
sémantique exacte. Le garde `c != 0` évite un `% 0` natif (le repli BigInt
gardait le même comportement).

---

## 4. Gains mesurés (12 échantillons appariés, c6 + c16)

| Mesure | témoin c16 | final c16 | Δ c16 | témoin c6 | final c6 | Δ c6 |
|---|---:|---:|---:|---:|---:|---:|
| **section rationnelle** (`rat_chrono.rose`) | 445 439 | 425 616 | **−20 687** | 445 839 | 426 920 | **−19 320** |
| phase rationnelle du banc instrumenté | 502 951 | 483 271 | **−19 680** (×0,961) | 505 496 | 483 288 | **−22 208** (×0,956) |
| **banc global complet**, horloge `temps()` ns | 2 352 588 | 2 319 452 | **−24 631** | 2 365 829 | 2 337 341 | **−24 016** |
| somme des 9 phases | 2 307 876 | 2 290 300 | −17 575 | 2 343 244 | 2 319 468 | −23 776 |
| carrés (contrôle, hors patch) | 546 583 | 546 920 | +336 | 547 023 | 545 559 | −1 464 |
| flottants (contrôle) | 514 183 | 516 959 | +2 776 | 517 568 | 516 687 | −881 |
| tables + RNG (contrôle) | 593 478 | 594 967 | +1 488 | 612 094 | 610 487 | −1 608 |

Différences appariées médianes : rationnels **−20 687 ns c16** (min −35 760,
max −12 561) et **−19 320 ns c6** (min −24 559, max +81 024 — une valeur
aberrante isolée du côté final). Les phases hors rationnels restent dans le
bruit : aucun gain n'a été pris hors de `i128x.rs`.

Le compteur interne d'origine (`millisecondes()`, ms entiers) affiche `2` pour
le témoin **et** le final : il ne peut pas résoudre 20 µs. Le banc Python
`global_bench.py` (9 tirs) donne ratio interne 1,454 → 1,450 c16 et 1,433 →
1,451 c6 — **indiscernable** avec cette horloge. Le chiffre de gain retenu est
celui de l'horloge ns, pas celui-là.

---

## 5. Tentatives mesurées puis REJETÉES (code retiré du livrable)

| Tentative (`i128x.rs`, arbre isolé) | Mesure | Décision |
|---|---:|---|
| Fusion `N = na*c1 + d1` et `N mod c` en une passe digits | rationnels **+144 768 ns c16 (×1,29)** | retirée : `BigInt::from_slice` + u128 par digit coûtent plus que les deux passes `num_bigint` remplacées |
| Condition suffisante renforcée (factorisation de `g`) pour sauter `mod_petit` | rationnels **+7 336 ns c16 (×1,015)** | retirée : la condition ne saute presque rien de plus et la factorisation coûte plus qu'elle ne rapporte |
| `mod_petit` sur digits u64 (base 2^64) | rationnels **+41 488 ns c16 / +27 936 ns c6** | retirée : le micro-bench sur tranches `&[u64]` donnait 2,1–2,3×, mais `BigInt::iter_u64_digits()` recompose les digits u32 et annule le gain |
| `mod_petit` avec un seul `%` u128 par digit | micro-bench **1,3–1,7× plus lent** | non implémentée |
| division par réciproque (magic number) dans `div_rem_petit` | micro-bench **+3 à +4 % seulement** | non implémentée |
| u64 digits + division u128 pour `div_rem_petit` | micro-bench **1,6–2× plus lent** | non implémentée |

Aucun de ces chiffres n'est extrapolé : ils viennent de salves exécutées ou de
micro-benchs `cntvct` (`divbench.rs`, `divbench2.rs`, `modbench.rs`,
`flotbench.rs`) avec 5–9 répétitions et étalons de correction.

---

## 6. Validation (preuves de sortie sur le binaire final)

| Gate | Résultat observé |
|---|---|
| `cargo build --release --bin rose` (dépôt, `CARGO_TARGET_DIR` dédié) | **rc = 0** |
| `cargo test --release` (dépôt, target dédié) | **515 passed, 0 failed** (258 + 242 + 6 + 8 + 1 ; ignored 3 + 3) |
| Jardin `jardins/tests/premiers.rose` | `── jardin/premiers : 20 tests, 0 échec ──` |
| `fib(25)` | `75025` |
| Point fixe (2 voies) | `b3ed9262dd54776a6dbd07bc12368d0e` **×2** (`rose rir compilateur/emetteur.rose` et `rose compilateur/emetteur.rose compilateur/emetteur.rose`), stderr 0 octet des deux côtés |
| **9 domaines × 9 tirages c6 et c16** | **81/81 + 81/81 = 162/162** sorties octet pour octet identiques au témoin |
| `bench_global.rose` (dossier temporaire) | **rc = 0**, **12 lignes**, **11 valeurs identiques** à `full.txt`, stderr 0 octet |
| Suite complexe (`complexe.rose`, 9 tirs/cœur) | valeurs conformes (`0.9999999999860738`, `333333833333500000`) ; final−base apparié −19 µs c6 / −2 µs c16 = bruit, **aucune régression** |
| Englobant (`bench_englobant.rose 4096 1`, 9 tirs/cœur) | valeurs conformes (`46297232.426252 / 564 / 157158 / 11899223598484`) ; final−base −33 µs c6 / +2 µs c16 = bruit, **aucune régression** |
| Détail géant (`demo_geant.rose`) | longueur `365657055180967481723488710810833949196`, zeros finaux `2499999999999999999999999999999999990`, derniers non nuls `467172352` — conformes, stderr 0 |
| Périmètre | `git diff --stat` : **un seul fichier source**, `rose/src/i128x.rs`, 13 insertions ; aucun commit, aucune publication |

---

## 7. Dépassement Rust : NON atteint — blocage chiffré

| Mesure (médianes, 9–12 tirs) | Rose témoin | Rose final | Rust |
|---|---:|---:|---:|
| global interne, horloge `temps()` ns — c16 | 2 352 588 | **2 319 452** | 1 379 000 |
| global interne — c6 | 2 365 829 | **2 337 341** | 1 378 000 |
| ratio interne final / Rust | 1,706 | **1,682** | 1,000 |
| ratio interne c6 | 1,717 | **1,696** | 1,000 |
| mur (taskset + chargement inclus) c16 | 4,500 ms | 4,463 ms | 2,513 ms |
| mur c6 | 4,737 ms | 4,622 ms | 2,555 ms |

Il reste donc **~0,94 ms de temps interne** à rattraper (2,32 − 1,38). Trois
bornes chiffrées montrent que ce n'est pas accessible dans le périmètre SP9 :

1. **Carrés + flottants = 1 058 µs c16 / 1 058 µs c6**, soit **46,3 % / 44,8 %**
   du calcul, dans `jit.rs` (interdit). Le flottant est un plancher mesuré à
   512,8 µs par la boucle Rust nue (§2) ; les carrés sont déjà SIMD NEON à
   540,7 µs pour 10 M produits.
2. **Hors chrono ≈ 1,0 ms** : mur − interne = 2,15 ms pour Rose contre 1,12 ms
   pour Rust, écart ≈ 1,03 ms, dans `main.rs` (interdit).
3. Mon périmètre entier (rationnels + tables + RNG = 1 076 µs c16) est déjà
   optimisé par GLB1/GLB4, SPEED4 et SPEED6 ; la décision livrée ne touche que
   5 % de la section rationnelle et rapporte 20–25 µs. Même en supprimant
   **toutes** ces phases (ce qui détruirait la sémantique et les 11 valeurs),
   il resterait ~1,28 ms interne + ~1,0 ms hors chrono, contre 1,38 ms + ~1,1 ms
   pour Rust : le mur resterait ≈3,4 ms contre ≈2,5 ms.

La réserve sémantique du jumeau Rust est maintenue : `bench_global_rs` rend
`1291990006563070912` (débordement `i64`) et `7.818804193883677` (`f64`),
tandis que Rose rend l'entier exact `333333383333335000000` et la fraction
rationnelle exacte. **Aucune triche, aucune adoption de cette sémantique,
aucun résultat codé en dur, 11 valeurs identiques à chaque tirage.**

---

## 8. État final factuel

- **Fait** : une voie scalaire exacte de 13 lignes dans `pgcd_grand_petit`,
  dans `rose/src/i128x.rs` uniquement.
- **Vérifié** : build, 515 tests, jardin 20/20, fib25, point fixe ×2,
  162/162 multi c6+c16, global rc=0 + 11 valeurs, suite et englobant sans
  régression, détail géant, A/B entrelacé 12 échantillons/cœur, 5 candidats
  mesurés et 4 rejetés.
- **Gain réel** : −19 à −25 µs sur la section rationnelle / le global,
  soit ~1 % du temps interne ; ratios Rose/Rust 1,68 interne, 1,78–1,81 au mur.
- **Non atteint** : dépassement de Rust. Blocage borné et chiffré en §7 :
  `jit.rs` (46 % du calcul) et `main.rs` (~1,0 ms) hors périmètre.
- **Aucun commit, aucune publication, aucun binaire installé dans `rose/rose`.**
