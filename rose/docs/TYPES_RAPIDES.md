# Types non numériques — accélération ciblée (chaînes, listes, tables)

**Horodatage : 2026-11-18 (mesures pinées sur le cœur X925 19, `governor=performance`).**
Tous les chiffres ci-dessous sont des **constats datés** re-mesurés dans cette salve ;
aucun n'est recopié de `BENCH_MULTI.md` sans preuve locale.

Périmètre de la mission : **un seul fichier source modifié, `rose/src/run.rs`**, plus
ce document. `interp.rs`, `jit.rs`, `front.rs`, `rir.rs`, `i128x.rs`, `Cargo.toml`,
`main.rs`, `lib.rs` et `PLAN.md` n'ont **pas** été touchés. Aucun commit.

---

## 1. Protocole de mesure

- Binaire A (contrôle) : source `rose/src/run.rs` exact de `HEAD` (`git show HEAD:rose/src/run.rs`),
  compilé dans un arbre gelé `/tmp/opencode/typesrapides/src-base` avec
  `CARGO_TARGET_DIR=/tmp/opencode/typesrapides/target-orig`.
- Binaire B (correctif) : même arbre gelé, **seul `run.rs` diffère**,
  `CARGO_TARGET_DIR=/tmp/opencode/typesrapides/target-base`.
- Sources des trois benchmarks : identiques à ceux de `BENCH_MULTI.md`
  (`/tmp/opencode/benchmulti/r_str.rose`, `r_list.rose`, `r_table.rose`).
- **9 échantillons** par binaire en salve dédiée (`mesure.py`), puis **12 échantillons
  entrelacés A/A/B/A/A/B/A/B/A/A/B/A** avec rotation de l'ordre des domaines (`ab.py`).
- `taskset -c 19`, valeur de sortie vérifiée à chaque échantillon (`20000`,
  `4999950000`, `3000`) ; aucun échantillon rejeté.
- Horloge : `temps()` Rose, convertie de ns en ms. Médianes rapportées, min–max entre crochets.

## 2. Baseline reproduite (contrôle A, 9 échantillons)

| Domaine | Baseline A, médiane | `BENCH_MULTI.md` (31e0553) | Écart |
|---|---:|---:|---:|
| Chaîne 20 000 car. | 14,343 ms [14,328 – 14,377] | 14,326 ms | +0,1 % |
| Liste 100 000 entiers | 237,868 ms [237,155 – 238,977] | 238,335 ms | −0,2 % |
| Table 3 000 clés | 192,802 ms [190,900 – 196,981] | 188,139 ms | +2,5 % |

La baseline est donc bien reproduite sur la machine et le cœur de la salve d'origine.

## 3. Où le temps passe — décomposition (5 échantillons, contrôle A)

Micro-sources isolées (mêmes boucles, chronomètre déplacé) :

| Micro-domaine | Médiane | Lecture |
|---|---:|---|
| `m_str_cat` — 20 000 × `s = s + "x"` | 12,481 ms | **la concaténation seule** est le coût |
| `m_str_len` — `longueur(s)` sur 20 000 car. | 0,004 ms | `longueur` est négligeable |
| `m_list_join` — 100 000 × `joindre` | 72,926 ms | appel de primitive + `push` |
| `m_list_sum` — 100 000 × `s = s + l[j]` | 137,419 ms | **l'indexation** domine |
| `m_table_ins` — 3 000 × `inserer` | 95,015 ms | 4,5 M comparaisons de clés |
| `m_table_look` — 3 000 × `cle_existe` | 96,561 ms | 9 M comparaisons de clés |

Isolation supplémentaire du domaine liste (contrôle A) :

| Micro-domaine | Médiane |
|---|---:|
| `m_idx` — 100 000 × `$x = $l[$j]` (index seul) | 133,940 ms |
| `m_add` — 100 000 × `$s = $s + $j` (addition seule) | 1,393 ms |
| `m_boucle` — 100 000 tours, `longueur($l)` en borne | 14,364 ms |

### Localisation exacte des causes (lecture de code, pas supposition)

| Cause | Fichier : ligne | Modifiable dans le périmètre ? |
|---|---|---|
| `+` sur chaînes = `Valeur::s(format!("{}{}", x, y))`, recopie O(n²) | `interp.rs:2743-2746` | **non** |
| `joindre` = `l.0.borrow_mut().push(a[1].clone())` | `interp.rs:3525-3529` | **non** |
| `Expr::Index` liste = `vers_z(index)` + `fits_u64_lt` + `get_u64`, chacun reclône un `BigInt` | `interp.rs:1905-1916`, `i128x.rs:496,506` | **non** (sauf `vers_z`, voir §5) |
| `inserer` / `cle_existe` = scan linéaire avec `valeur_egale` sur `Vec<(Valeur,Valeur)>` | `interp.rs:3488-3510` | **non** |
| `valeur_egale` retombait sur `cmp_vals` → `vers_z` ×2 + `cmp_gz` → **~6 allocations par comparaison de deux i64** | **`run.rs:454-500`** | **oui** |

`Valeur::Table` est bien `Rc<RefCell<Vec<(Valeur, Valeur)>>>` (`run.rs:32`) : la structure
de hachage annoncée dans le diagnostic initial n'existe pas. La recherche est linéaire.

## 4. Correctif appliqué (`rose/src/run.rs` uniquement)

Deux voies rapides **sémantiquement exactes**, calquées sur celle que `interp.rs`
possède déjà dans `binaire_valeurs` pour l'arithmétique i64 :

1. `cmp_vals` : si les deux operands sont `Valeur::Entier(_, None)`, retour direct
   de `x.cmp(y)` mappé sur −1/0/1 — zéro allocation, zéro `BigInt`.
2. `valeur_egale` : si les deux operands sont `Valeur::Entier(_, None)`, retour
   direct de `x == y`.

Les valeurs `Valeur::Entier(_, Some(Rc<Gz>))` (grands entiers, résultats JIT via
l'ABI taggée) **ne prennent pas** ces voies : elles continuent vers l'arithmétique
exacte. Le comportement du langage est inchangé (les i64 tenant dans `None` ont
exactement la même valeur sous forme `BigInt`).

Taille du diff : **+23 lignes**, 0 ligne supprimée, dans un seul fichier `.rs`.

## 5. Tentative écartée (mesurée, non retenue)

Un troisième correctif a été testé puis **reverté** : construire le `Gz` directement
dans `vers_z` pour un i64 (`Gz(Box::into_raw(Box::new(Mpz { v: BigInt::from(*i) })))`)
afin d'éviter `Gz::neuf()` + `poser_i64` et l'accès thread-local `LIGNE_COURANTE`.

Résultat (9 échantillons) : liste 237,946 → 237,103 ms (**−0,35 %**, dans le bruit),
chaîne 14,276 → 14,299 ms, table inchangée. Gain non significatif → modification
retirée ; le diff final ne contient que les deux voies rapides de §4.

## 6. Gains mesurés — salve entrelacée A/A/B/A/A/B/A/B/A/A/B/A (12 échantillons)

| Domaine | A = HEAD | B = correctif | **Gain réel** | Plage A | Plage B |
|---|---:|---:|---:|---:|---:|
| Chaîne 20 000 car. | 14,341 ms | 14,275 ms | **1,00 ×** | 14,31 – 14,38 | 14,26 – 14,29 |
| Liste 100 000 entiers | 237,505 ms | 237,277 ms | **1,00 ×** | 236,77 – 238,00 | 236,99 – 238,21 |
| **Table 3 000 clés** | **192,128 ms** | **22,928 ms** | **8,38 ×** | 191,54 – 195,29 | 22,87 – 22,99 |

Ces trois lignes sont la salve finale, re-jouée avec le binaire B compilé depuis
un arbre gelé ne contenant **que** le correctif de §4 (les agents concurrents
modifiaient `jit.rs`/`lib.rs`/`main.rs`/`Cargo.toml` dans le dépôt pendant la
mission : un build fait directement depuis le dépôt vivant à 14:50 a fait
régression `fib(25)` et `bench_global`, alors que le build gelé passe toutes les
portes — voir §9).

Décomposition post-correctif (5 échantillons, B) :
`m_table_ins` 95,015 → **10,715 ms** (8,86 ×), `m_table_look` 96,561 → **11,361 ms**
(8,50 ×). Chaîne, liste, `m_str_cat`, `m_list_join` et `m_list_sum` : inchangés
(confirmation que le correctif est ciblé, sans effet de bord mesurable).

**Objectif « ≥2× sur au moins 2 des 3 domaines » : atteint sur 1 domaine sur 3.**
Le second domaine est hors du périmètre de fichiers autorisé : les deux autres
goulots sont dans `interp.rs` (concaténation `format!`, `joindre`, `Expr::Index`)
et `i128x.rs` (`fits_u64_lt`/`get_u64` reclônent le `BigInt`). Je ne prétends pas
les avoir accélérés.

## 7. Ce qui reste (honnêteté)

- **Chaînes** : `s = s + "x"` reste O(n²), ~620 ns par concaténation, ~200 Mo copiés
  en 12,4 ms (~16 Go/s). Le correctif `with_capacity` + `push_str` suppose de
  modifier `binaire_valeurs` (`interp.rs:2744`) — interdit ici. `longueur()` est déjà
  négligeable (0,004 ms).
- **Listes** : `joindre` (73 ms) est de la surcouche interpréteur + `Vec::push` ;
  l'indexation (134 ms) paie `vers_z` + **deux clones de `BigInt`** dans
  `fits_u64_lt` et `get_u64` (`i128x.rs`), plus le boxage `Valeur`/`Rc<Gz>`.
  Aucun de ces appels ne peut être supprimé depuis `run.rs`.
- **Tables** : le scan linéaire reste. 8,38 × provient uniquement du fait que
  chaque comparaison est devenue un `cmp` de deux i64 au lieu de 6 allocations.
  Une vraie table de hachage (`HashMap`) dans `inserer`/`cle_existe` (`interp.rs`)
  réduirait encore ~4,5 M + 9 M comparaisons à ~6 000, mais nécessite de modifier
  `interp.rs` **et** de définir un hachage stable des `Valeur` — hors périmètre.
- **Boxage / typage dynamique** : `Valeur` est un enum avec `Rc<Gz>`, `Rc<String>`,
  `Rc<RefCell<Vec<_>>>`. Tant que les types non numériques passent par
  l'interpréteur, le plancher reste ~1 µs/opération contre ~1 ns en Rust natif.

## 8. Verdict honnête vs Rust

| Domaine | Rose après correctif | Rust `-O` (`BENCH_MULTI.md`) | Ratio Rose/Rust après | Verdict |
|---|---:|---:|---:|---|
| Chaîne 20 000 car. | 14,275 ms | 0,014 ms | ~1 020 × | **Perdu** (aucun gain) |
| Liste 100 000 entiers | 237,277 ms | 0,202 ms | ~1 175 × | **Perdu** (aucun gain) |
| Table 3 000 clés | 22,928 ms | 0,097 ms (HashMap) | ~236 × | **Perdu**, mais 8,38 × plus rapide qu'avant |

**Je ne prétends pas battre Rust.** Le gain réel et reproductible de cette mission
est **8,38 × sur les tables**, obtenu en supprimant ~13,5 millions d'allocations
`BigInt`/`Mpz` sur les 13,5 millions de comparaisons de clés i64 du benchmark.
Les 1 000 × restants sur chaînes/listes sont structurels et demandent des
modifications de `interp.rs`/`i128x.rs` refusées par le périmètre de cette mission.

## 9. Validation (binaire B gelé, `2026-11-18T14:56:25+01:00 → 15:00:41+01:00`)

Toutes les portes du script inchangé `scripts/audit-final.sh` (copié dans `/tmp`
avec uniquement `RACINE` et `ROSE` pointés sur l'arbre gelé) passent :

| Porte | Commande / extrait | Résultat |
|---|---|---|
| Build | `cargo build --release --bin rose` (arbre gelé, `CARGO_TARGET_DIR=…/target-clean`) | `Finished release [optimized]`, 227 warnings préexistants |
| Oracles M4.1 lexeur | `rose tokens` vs `compilateur/lexeur.rose` | **151 match / 0 écart** |
| Oracles M4.2 analyseur | `rose ast` vs `compilateur/analyseur.rose` | **151 match / 0 écart** |
| Oracles M4.3 vérificateur | `rose verif` vs `compilateur/verifieur.rose` | **151 match / 0 écart** |
| Oracles M4.4 émetteur RIR | `rose rir` vs `compilateur/emetteur.rose` | **151 match / 0 écart** |
| Point fixe M4.5 | `md5(rose rir emetteur.rose)` vs `md5(rose emetteur.rose emetteur.rose)` | **`bdbef63681608a0b22a5637539e9d738` égaux — ATTEINT** |
| Jardin | `rose jardins/tests/premiers.rose` | **20 tests, 0 échec** |
| `fib(25)` | `fn fib(n)… >> fib(25)` | **75025** |
| `bench_global.rose` | exécuté depuis un dossier temporaire | **rc = 0**, dernière ligne `655` |
| `cargo test --lib link_natif` | — | 7 passed, 0 failed |
| `cargo test --lib natif_emetteur` | — | 6 passed, 0 failed |
| `cargo test --bin rose` | — | **144 passed, 0 failed** |
| Invariants | `rug` absent du Rust, jardins sans `.rs`, version 0.1.5, remote `Stef302/Rose` | CONFIRMÉ |

Verbatim de synthèse : `Portes NON atteintes constatées : AUCUNE (toutes les portes
mesurées sont atteintes).`

⚠ Le build fait à 14:50 directement depuis le dépôt vivant (où d'autres agents
avaient déjà modifié `Cargo.toml`, `jit.rs`, `lib.rs`, `main.rs`) a donné
`fib(25)` vide et `bench_global.rose` rc=139 (segfault). Ce n'est **pas** un effet
de ce correctif : le binaire gelé ne contenant que `run.rs` passe les mêmes
commandes. Ce constat est daté et devra être re-mesuré après convergence des
agents concurrents.
