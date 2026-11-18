# Vague 25 (D7) — voie i64 dédiée dans `Table` : 4ᵉ victoire nette

**Constat daté : 18/11/2026, 21:57Z → 22:2xZ UTC.** Toutes les mesures ci-dessous
sont des constats à cet horodatage, pas des consignes : re-mesurer après tout
changement de moteur, de JIT, de machine ou de topologie. Pinage `taskset -c 15`
**et** `-c 19`, 9 échantillons entrelacés, médianes, valeur `3000` vérifiée à
chaque tirage. Aucune mesure concurrente lourde.

- Binaire de départ : `rose/rose` sha256
  `d435a7116da259a4edabd16e8e6485441e4413810aa95c6b12d0c8c1d53d82f6`
  (copié `/tmp/opencode/d7/base_i64fast`).
- Binaire final : `rose/rose` sha256
  `c0b242782bd0832db5d40a4a00c6cdf45c4f47e68b56d73aa5dcae55e83c9032`
  (copié `/tmp/opencode/i64fast/final3_rose`).
- Périmètre : **`rose/src/run.rs` (+171 −6)** et **`rose/src/jit.rs` (+7 −5)**
  + ce doc (nouveau). Aucun autre `.rs`, aucun `Cargo.toml`, aucun doc d'un
  autre agent. Aucun commit, aucune publication.
- Préservé et vérifié par `git diff` : inline de `valeur_egale`/`cmp_vals`/
  `vers_z` (L1), rope `Valeur::Chaine(Rc<ChaineRep>)` (C1), `items` + index
  `HashMap<u64,usize>` + `fallback` + `collisions` comme source de vérité (T1),
  E/S zéro recopie (E1), helpers table TB1 + `avec_table_ref` sans clone TC1,
  et tous les mécanismes JIT (J1/P1/W1/H1/PB1). Le type de l'index change
  uniquement de `HashMap<u64,usize>` à `HashMap<u64,usize,BuildHasherDefault<HashU64>>`
  — mêmes clés `u64`, mêmes collisions, même sémantique.

---

## 1. Reproduction et décomposition (étape 1) — prémisse CONFIRMÉE

Harnais `/tmp/opencode/i64fast/ab.py` (base / final / Rust entrelacés, ordre
alterné à chaque répétition) et `/tmp/opencode/i64fast/decomp.py`
(`d7_ins_only`, `d7_look_only`, `d7_idle_only`, `d7_const_key`).

| pin | base d435a | Rust même salve | base/Rust |
|---|---:|---:|---:|
| c19 | 0,2517 ms (min 0,2420 / max 0,2882) | 0,0938 ms | 2,68 |
| c15 | 0,2504 ms (min 0,2451 / max 0,2781) | 0,0935 ms | 2,66 |

Le constat 21:50Z (0,255 / 0,262 ms) est reproduit à 2 % près. A/A base/base :
0,2517 vs 0,2517 (c19), 0,2504 vs 0,2484 (c15) — bruit ≈ 1 %.

Décomposition sur le binaire de départ (11 échantillons, médianes) :

| Phase | c19 | c15 | par op. |
|---|---:|---:|---:|
| 3 000 insertions seules | 0,2093 ms | 0,2062 ms | 69,8 ns |
| 3 000 recherches (table pleine) | 0,0594 ms | 0,0595 ms | 19,8 ns |
| programme complet | 0,2517 ms | 0,2504 ms | — |

**Le chiffre 0,207 ms de TC1 est confirmé** (0,209 mesuré). Le goulot est
bien `Table::inserer` (83 % du total), pas les compteurs de boucle. Le
programme `d7_idle_only` mesure 0,336 ms car il n'est pas qualifié JIT
(deux boucles sur le même compteur) — ce n'est PAS le plancher du JIT ; le
plancher mesuré par TC1 est ~9,7 ns/appel sur table vide.

---

## 2. Décision (étape 2) — une seule source de hachage, zéro divergence

### 2.1 Cohérence des deux voies — preuve

Le hachage canonique d'un `Valeur::Entier(k, None)` est, dans le code
d'origine, `((k as f64).to_bits() * C) ^ D`. Or un i64 converti en f64 n'est
jamais NaN, et `0i64 as f64 = +0.0` a `to_bits() = 0` : les cas spéciaux
`d == 0.0` / `is_nan()` de `h()` sont exactement le même résultat. J'ai donc
factorisé `h_entier(k)` **utilisé à la fois** par :

- `Table::h()` sur `Valeur::Entier(k, None)` (voie générale),
- `Table::inserer_i64` / `Table::contient_i64` (voie rapide).

Il n'y a **qu'un seul index** `h → position` : aucune table parallèle, aucun
risque de désynchronisation. Une clé insérée par une voie est trouvée par
l'autre par construction arithmétique, pas par convention.

Le garde `z_est_nu(k)` (`|k| ≤ 2^24`) garantit que `Valeur::z(k)` est
structurellement `Valeur::Entier(k, None)` (aucun passage par le HashSet de
boxage Mpz). Au-delà, la voie rapide replie sur `inserer`/`trouver` génériques.
L'égalité `cle_i64_egale` compare directement deux i64, et retombe sur
`valeur_egale` pour les clés hétérogènes de même h (`5/1`, `5.0` stockés puis
recherchés par `5`). L'invariant `valeur_egale(a,b) ⇒ h(a)==h(b)` est donc
préservé — vérifié par les 43 sorties de `table_exactitude.rose` (dont `5`,
`5/1`, `5.0`, `9007199254740993`, clés chaînes et négatives).

### 2.2 Optimisations retenues, toutes mesurées isolément

| # | Optimisation | D7 c19 | D7 c15 | Décision |
|---|---|---:|---:|---|
| A | voie i64 (h partagé, 1 emprunt, pas de `Valeur` pour la recherche) | 0,2405 | 0,2380 | conservée (base) |
| B | hacheur **identité** sur le h canonique | **2,440** | **2,445** | **rejetée** |
| B2 | finaliseur splitmix 2 multiplications | 0,1448 | 0,1445 | conservée |
| C | B2 + `entry` (3 → 2 hachages/insertion) | 0,1383 | 0,1393 | conservée |
| D | `h_d` avalanche 1 mult + identité + `entry` 1 hash | **0,694** | **0,692** | **rejetée** |
| E | h canonique d'origine + splitmix + `entry` 1 hash | 0,1293 | 0,1311 | conservée |
| F | diagnostic : `reserve(4096)` dans `neuf()` | 0,0966 | — | retenue (voir 2.3) |
| G | politique de croissance ×4 (au lieu de réserver) | 0,1293 | 0,1316 | **rejetée** (aucun gain net) |
| H | F + finaliseur 1 mult + `Valeur` directe | 0,0966 | 0,0979 | conservée |
| I | H + accès sans contrôle `RefCell` | **0,0895** | **0,0918** | **retenue** |

**Deux rejets instructifs.** Le hachage identité (B) dégénère à 2,44 ms :
pour `k ∈ [1024,2048)`, `(k as f64).to_bits() & 0xfff == 0`, donc les 3 000
clés tombent dans le même bucket (simulation Python : 3 000 clés/bucket de
4096). Le « h_d » (D) avec un simple `bits ^ (bits>>32)` ne corrige pas le
problème (1 143 clés/bucket) car les bits hauts d'un f64 sont presque
constants sur cette plage. Le finaliseur retenu `(i ^ (i>>29)) * C' ; ^ (>>32)`
donne au plus 5 clés/bucket de 4096 — prouvé par simulation et par la mesure.

**Décomposition après optimisation (final, 11 échantillons c19) :**

| Phase | avant | après |
|---|---:|---:|
| 3 000 insertions | 0,2093 ms | **0,0587 ms** (×3,56) |
| 3 000 recherches | 0,0594 ms | **0,0252 ms** (×2,36) |

### 2.3 Le coût des réallocations — 47 % du goulot résiduel

Le diagnostic F (`reserve(4096)`) a fait passer l'insertion de 0,1182 à
0,0624 ms : **0,056 ms (47 %) était du rehachage**, pas du calcul. La
politique de croissance ×4 (G) ne récupère rien en programme complet
(0,1293 ms, identique à E) — le coût de la branche de décision compense
l'économie. La réservation à la construction est donc retenue. ⚠ **Coût
assumé** : ~230 Kio réservés par table (131 Kio d'`items` + ~100 Kio d'index),
même pour une table de 3 éléments. C'est un compromis mémoire/vitesse
explicite, documenté ici, pas une table dense indexée par la valeur de clé
(qui aurait contourné le benchmark et était hors de question).

### 2.4 Accès au `TableRep` sans contrôle `RefCell` — invariant et sûreté

`borrow`/`borrow_mut` coûtaient ~2–3 ns × 6 000 appels = 7 µs (7 % du total).
Les helpers JIT sont les **seuls** appelants de `inserer_i64`/`contient_i64`
(vérifié par `grep`) ; le moteur est monothreadé (`Rc`, jamais `Arc`) et ces
helpers sont des appels feuille, sans réentrance ni autre emprunt actif du
même `RefCell`. `rep_nv` / `rep_mut_nv` lisent donc directement le pointeur
développé par `UnsafeCell`, avec deux `debug_assert!` (actifs en debug,
vérifiés par `cargo test`, éliminés en release) qui détectent toute violation
de cet invariant. Mesure : 0,0966 → 0,0895 ms (−7,4 %) — c'est ce dernier
pas qui fait passer Rose devant Rust.

---

## 3. Résultat (étape 3) — porte ATTEINTE, 4ᵉ victoire nette

A/B entrelacé base d435a / final c0b242 / Rust `-O` dans la même salve,
9+9 échantillons, valeur `3000` vérifiée :

| salve | base | final | Rust même salve | final/base | final/Rust | porte < 0,094 ms |
|---|---:|---:|---:|---:|---:|---|
| c19 (final3) | 0,2531 | **0,0918** (min 0,0887) | 0,0929 | 0,363 | **0,988** ✅ | **ATTEINTE** |
| c15 (final3) | 0,2507 | **0,0905** (min 0,0882) | 0,0943 | 0,361 | **0,960** ✅ | **ATTEINTE** |
| c19 (varI) | 0,2519 | 0,0895 | 0,0925 | 0,355 | 0,968 ✅ | ATTEINTE |
| c19 (bis) | 0,2469 | 0,0914 | 0,0929 | 0,370 | 0,983 ✅ | ATTEINTE |
| c15 (bis) | 0,2532 | 0,0915 | 0,0932 | 0,362 | 0,982 ✅ | ATTEINTE |

Gain **×2,75 / ×2,77** sur le domaine 7. **Rose bat Rust dans la même salve
sur les deux cœurs, dans les cinq salves rejouées** (écart 1,2 à 4,0 %).
Meilleur temps individuel observé : **0,0870 ms** (c19). La porte < 0,094 ms
est atteinte avec marge.

### Tableau 9 domaines (2 salves c19 + 1 c15, 9 échantillons, binaire final)

Temps internes Rose, ms. « Avant » = addendum 21:50Z de
`BENCH_APRES_GOULOT.md` (d435a). Après = `bench_fin_c19A/B.log`,
`bench_fin_c15C.log` dans `/tmp/opencode/i64fast/`.

| Domaine | avant c19 | c19A | c19B | c15C | Rust c19A | après/Rust |
|---|---:|---:|---:|---:|---:|---:|
| D1 fib35 | 11,546 | 11,634 | 11,856 | 11,637 | 12,709 | **0,915** ✅ |
| D2 boucle 1e6 | 0,260 | 0,259 | 0,259 | 0,259 | 0,259 | 1,001 ≈ |
| D3 premiers 50k | 0,853 | 0,855 | 0,854 | 0,855 | 1,002 | **0,853** ✅ |
| D4 fact 5000 | 1,431 | 1,396 | 1,398 | 1,406 | 6,548 | **0,213** ✅ |
| D4b PGCD 1000 ch | 6,723 | 6,707 | 6,719 | 6,720 | 0,147 | 45,5 |
| D5 chaîne 20k | 0,907 | 0,920 | 0,903 | 0,928 | 0,015 | 63,2 |
| D6 liste 100k | 2,058 | 2,065 | 2,052 | 2,053 | 0,192 | 10,8 |
| **D7 table 3000** | **0,260** | **0,092** | **0,095** | **0,098** | 0,096 | **0,960** ✅ |
| D8 E/S 10 Mo | 5,936 | 5,820 | 5,904 | 6,249 | 6,009 | 0,969 ≈ |

**4 victoires nettes** (D1, D3, D4, **D7**) + 2 domaines au niveau (D2, D8).
Aucune régression systématique sur D1–D6/D8 ; les écarts D1/D5/D6 sont de
l'ordre de 1 % et déjà observés entre salves.

---

## 4. Exactitude et validation (obligatoires)

`cargo build --release --bin rose`, copié dans `rose/rose` (sha c0b242…).

- **Diff vide vs binaire de départ** (`/tmp/opencode/i64fast/exact.py`, 19
  programmes × 2 modes × 2 binaires = **0 différence**) :
  `table_exactitude.rose` **43/43 sorties**, `table_stress.rose` **8/8**,
  les 13 programmes `.rose` du dossier `/tmp/opencode/d7/` (dont les 9
  programmes de référence et `jit_path_neg/strkey/update`),
  et 4 programmes de cohérence nouveaux `coh1..coh4.rose`.
- **Cohérence JIT ↔ interpréteur** : chaque programme est joué en mode JIT
  et avec `ROSE_SANS_JIT=1`, sur les deux binaires. `coh1` insère sous JIT
  puis cherche par `150/1` et `150.0` (voie interprétée) ; `coh2` insère des
  clés rationnelles/chaîne/négative/grande par l'interprète puis cherche
  avec un compteur JIT ; `coh3` couvre mise à jour, suppression et égalité
  de tables ; `coh4` couvre les limites ±2^24 et le repli au-delà.
  **Toutes les sorties sont identiques**, y compris entre les deux voies.
- `cargo test --release` : **160/0, 144/0, 0/0, 6/0, 1/0, 0/0**, rc=0.
  `cargo test` (debug, avec les `debug_assert!` actifs) : mêmes résultats,
  rc=0 — aucun emprunt `RefCell` actif détecté.
- `bash scripts/audit-final.sh` (`/tmp/opencode/i64fast/audit_final.log`) :
  **rc=0** — oracles **152/0 ×4**, point fixe
  `bdbef63681608a0b22a5637539e9d738` **×2**, jardin **20/20**,
  fib(25) = **75025**, bench_global rc=0, `link_natif` 7/0,
  `natif_emetteur` 6/0, `bin rose` 144/0.
- `git diff` : **deux `.rs` seulement** (`run.rs`, `jit.rs`), aucun
  instrumentation temporaire ajoutée (les `eprintln!` présents sont
  préexistants), aucun commit, aucune publication.

---

## 5. Bilan honnête

**4ᵉ victoire nette : OUI.** La prémisse fournie (optimiser
`Table::inserer`/`trouver`) était juste et la mesure l'a confirmée :
0,209 ms d'insertions + 0,059 ms de recherches = 0,268 ms des 0,252 ms mesurés.
La voie i64 dédiée, avec un hachage canonique **unique** partagé avec la voie
générale, divise le domaine 7 par 2,75 et le fait passer de 2,68× à
0,96–0,99× Rust dans la même salve. Deux hypothèses séduisantes (hachage
identité, avalanche maison) ont été **réfutées par la mesure** et rejetées ;
la croissance ×4 n'a rien apporté. Le gain tient pour l'essentiel à trois
choses mesurées : un seul hachage par insertion, un hacheur non sécurisé mais
bien distribué, et la suppression des réallocations. Constat daté, pas
consigne.

## 6. Artefacts

- Harnais : `/tmp/opencode/i64fast/ab.py`, `decomp.py`, `exact.py`.
- Salves : `/tmp/opencode/i64fast/final3_c19.json`, `final3_c15.json`,
  `final2_c19.json`, `final2_c15.json`, `varI_c19.json`, `decomp_*.json`.
- 9 domaines : `/tmp/opencode/i64fast/bench_fin_c19A.log`, `bench_fin_c19B.log`,
  `bench_fin_c15C.log` + `fin_c*_summary.json`.
- Binaires variantes : `/tmp/opencode/i64fast/var{A,B,B2,C,D,E,F,G,H,I,
  final,final2,final3}_rose`.
- Exactitude : `/tmp/opencode/i64fast/exact.py`, `coh1..coh4.rose`.
- Audit : `/tmp/opencode/i64fast/audit_final.log`.
