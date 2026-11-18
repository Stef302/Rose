# Table native — index de hachage canonique (constat daté 18/11/2026)

**Périmètre** : domaine 7 du bench multi (`r_table.rose`), 3 000 insertions +
3 000 recherches de clés entières. Un seul agent actif, aucun tool lourd
concurrent. Fichiers modifiés : `rose/src/run.rs`, `rose/src/interp.rs` (et ce
document). **Aucun commit, aucune publication.**

**Binaire de départ** : `aa0f8c7ed5e6ab98cd07bca69337571b45f7ba2438ad4513f41a992becce6ee0`.
**Binaire final** : `6e3ff795456d3f9bcf01c74dcb5a443c65e969c29ee8515b18b6497a5dacf801`.

**Protocole** : `PIN_CPU={15,19} NSAMP=9 python3 bench.py` (médianes de 9
échantillons entrelacés, pinage `taskset -c`, valeur `3000` vérifiée à chaque
échantillon, horloge interne). Salves : 16:34Z (départ), 17:07Z / 17:08Z (final).

---

## 1. Reproduction du départ

| Pin | D7 avant (médiane, 9 échantillons) | min–max | Rust HashMap |
|---|---:|---:|---:|
| c19 | **6,341 ms** | 6,323–6,436 | 0,094 ms |
| c15 | **6,346 ms** | 6,319–6,371 | 0,097 ms |

Conforme au constat `BENCH_APRES_GOULOT.md` (6,33–6,34 ms, ×67).

---

## 2. Décomposition du coût (mesurée, instrumentation temporaire retirée)

### 2.1 Compteurs exacts (instrumentation `TEMP-PROF-TABLE`, 9 salves)

Compteurs insérés dans les 5 boucles de recherche et dans le chemin lent de
`cmp_vals` ; imprimés sur stderr via `ROSE_PROF_TABLE=1`. Valeurs **identiques
sur les 9 échantillons** :

| Site | Appels `valeur_egale` |
|---|---:|
| `inserer` (recherche linéaire) | 4 498 500 = 3000×2999/2 |
| `cle_existe` (recherche linéaire) | 4 501 500 = 3000×3001/2 |
| `indexer_valeur` / `affecter_index_valeur` / `supprimer` | 0 |
| **Total** | **9 000 000** |
| `cmp_vals` hors voie rapide i64 (allocations/boxing) | **0** |

L'instrumentation elle-même coûte ~2 ns/compteau (16,4 ms au lieu de 6,34 ms
sur le programme complet) : les temps de cette section ne sont donc PAS
comparables à ceux sans sonde ; seuls les **compteurs** sont retenus.

### 2.2 Temps par phase, SANS sonde (9 échantillons entrelacés)

| Programme | c15 | c19 |
|---|---:|---:|
| complet (insert + lookup) | 6,337 ms | 6,338 ms |
| insertions seules (3 000) | 3,202 ms | 3,191 ms |
| recherches seules (3 000, table pré-construite) | 3,149 ms | 3,134 ms |
| dispatch (3 000 primitifs + 3 000 itérations) | 0,598 ms | 0,600 ms |
| vide (6 000 itérations, aucun primitif) | 0,342 ms | 0,342 ms |

Insert + lookup = 6,351 ms ≈ complet 6,337 ms (écart 0,2 %) : les deux phases
épuisent le coût. Le scan linéaire représente ~5,2 ms sur 6,3 ms (~82 %) après
soustraction du plancher interprète/dispatch mesuré séparément ; **0 ns** est
attribuable aux allocations de comparaison (`cmp_slow=0`). La cause est donc
bien algorithmique (O(n²)), pas l'allocation ni la comparaison de valeurs.

### 2.3 Plancher après indexation (binaire final, c19, 9 échantillons)

| Opération | Médiane |
|---|---:|
| 3 000 `cle_existe` sur table **vide** | 0,425 ms |
| 3 000 `inserer` sur clé **existante** (mise à jour) | 0,583 ms |
| 3 000 `inserer` de clé **neuve** | 0,707 ms |
| 3 000 `cle_existe` sur table **pleine** | 0,664 ms |
| 6 000 itérations vides | 0,352 ms |

Le hachage ajoute ~0,24 ms à 3 000 insertions neuves et ~0,24 ms à 3 000
recherches ; le reste (~1,1 ms sur 1,38 ms) est le coût interprète/dispatch,
hors portée de l'index.

---

## 3. Architecture retenue

`Table` reste **un seul `Rc<RefCell<TableRep>>`** (comme avant : 8 octets), avec :

```rust
pub struct TableRep {
    pub items: Vec<(Valeur, Valeur)>,     // source de vérité, ordre d'insertion
    index: HashMap<u64, usize>,           // h canonique -> position
    fallback: Vec<usize>,                 // clés sans hachage canonique
    collisions: Vec<(u64, usize)>,        // (h, position), normalement vide
}
pub struct Table(pub Rc<RefCell<TableRep>>);
```

- `h(Valeur) -> Option<u64>` : entiers/rationnels/flottants par leur f64
  normalisé (`-0 ≡ +0`, NaN canonisé) — c'est exactement la comparaison de
  `cmp_vals` ; chaînes par FNV-1a de l'aplatissement ; booléens par tag ;
  `None` pour Liste/Table/Tampon/Tableau/Complexe/Irrationnel (repli linéaire).
  Deux valeurs égales au sens de `valeur_egale` ont toujours le même h.
- Collisions résolues par `valeur_egale` : l'égalité sémantique est inchangée,
  y compris `Entier(5) ≡ Rationnel(5/1) ≡ Flottant(5.0)`.
- `items` reste la source de vérité pour l'itération (`cles`, `valeurs`),
  l'affichage, `longueur` et l'égalité de tables : **aucun changement observable
  d'ordre**.
- `inserer` met à jour sans doublon ; `retirer` déscale toutes les positions ;
  `trouver` sert `cle_existe` et `indexer_valeur`.
- Les sites `inserer`/`cle_existe`/`supprimer`, l'indexation en lecture et en
  écriture et le littéral de table passent par ces méthodes. Les 3
  `#[inline(always)]` (vers_z/cmp_vals/valeur_egale), la rope
  `Valeur::Chaine(Rc<ChaineRep>)` et les corrections E/S zéro recopie sont
  intacts (`git diff` vérifié).

### Variantes mesurées et rejets

| Variante | D7 c19 (A/A/A entrelacé, 9 éch.) | Décision |
|---|---:|---|
| (a1) `HashMap<u64, Vec<usize>>`, 3 Rc | 1,484–1,506 ms | remplacée : 3 000 `Vec` de bucket alloués |
| (a1)+dispatch `appel_valeurs` (Var A) | 1,407–1,413 ms | gain nul en A/A/A → retirée |
| (a1)+fast-path slots (Var B) | 1,300–1,305 ms | gain réel mais +14 % D2, +10 % D6 → retirée |
| (a2) `HashMap<u64, usize>` + collisions, 4 Rc | 1,299–1,414 ms | remplacée : 2 Rc font régresser D5/D6 |
| (a3) 2 Rc (`TableRep` séparé) | 1,371–1,443 ms | remplacée : drop-glue modifiée, D5 +6 %, D6 +4 % |
| **(a4) 1 Rc, `TableRep` groupé (retenue)** | **1,380–1,386 ms** | retenue : meilleur compromis perf/régression |
| (b) `HashMap<Valeur, Valeur>` + `Hash`/`Eq` | non compilée | rejetée : changerait l'ordre d'itération et l'égalité (35 sites), alors que (a) réussit |
| (c) `BTreeMap` + `Ord` | non compilée | rejetée : plus lente en structure (comparaisons O(log n) sur des valeurs non `Ord`) et mêmes changements sémantiques |

Le déplacement du code de l'index en fin de `run.rs` a été mesuré : aucun
effet (< 0,5 %), hypothese de layout écartée. Le gain de la variante (a2)
venait de la suppression des `Vec` de bucket ; le gain final de (a4) vient du
retour à un seul `Rc`.

---

## 4. Résultat final (9 échantillons, pinage c15 et c19)

| Pin | Avant | Après | Gain | Rust HashMap | Ratio après |
|---|---:|---:|---:|---:|---:|
| c19 salve A | 6,341 ms | **1,383 ms** [1,369–1,414] | ×4,59 | 0,093 ms | 14,8 × |
| c19 salve B | 6,341 ms | **1,380 ms** | ×4,59 | 0,093 ms | 14,8 × |
| c15 | 6,346 ms | **1,386 ms** [1,375–1,408] | ×4,58 | 0,097 ms | 14,3 × |

**Porte < 1,0 ms NON atteinte** : plancher ~1,1 ms de dispatch interprète
(0,425 ms pour 3 000 appels sur table vide + 0,583 ms pour 3 000 insertions
sur clé existante − 0,352 ms d'itérations comptées deux fois). Nouveau record
**1,38 ms**, contre 6,34 ms au départ. Constat daté, pas consigne.

### Tableau 9 domaines (médianes internes, c19)

| # | Domaine | Avant | Après (A/B) | Écart | Rust | Ratio après |
|---|---|---:|---:|---:|---:|---:|
| 1 | fib(35) | 11,626 | 11,615 / 11,635 | ~0 % ✅ | 12,721 | 0,9 × ✅ |
| 2 | Boucle 1..10⁶ | 9,055 | 9,301 / 9,305 | **+2,7 %** | 0,261 | 35,6 × |
| 3 | Premiers < 50 000 | 9,297 | 9,424 / 9,437 | **+1,4 %** | 1,001 | 9,4 × |
| 4 | Factorielle 5 000 | 1,565 | 1,468 / 1,471 | −6,2 % ✅ | 6,539 | 0,2 × ✅ |
| 4b | PGCD 1 000 chiffres | 6,733 | 6,706 / 6,713 | −0,4 % | 0,148 | 45,4 × |
| 5 | Chaîne 20 000 car. | 3,278 | 3,324 / 3,333 | **+1,4 %** | 0,014 | 238 × |
| 6 | Liste 100 000 entiers | 38,089 | 38,672 / 38,751 | **+1,5 %** | 0,186 | 208 × |
| 7 | Table 3 000 clés | 6,341 | **1,383 / 1,380** | **−78,2 % ✅** | 0,093 | 14,8 × |
| 8 | E/S 10 Mo | 6,206 | 6,052 / 6,061 | −2,4 % ✅ | 6,078 | 1,0 × |

**Régressions mesurées, non masquées** : D2 +2,7 %, D3 +1,4 %, D5 +1,4 %,
D6 +1,5 %. Elles sont reproductibles en A/A/A entrelacé (binaire de départ
exécuté dans la même salve que le binaire final) et proviennent du changement
de shape du code de `stdlib`/`run.rs`, pas de l'algorithme de table. La porte
« sans régression sur D1/D5/D6 » n'est donc **pas strictement atteinte** pour
D5 et D6 (écarts ~1,5 %) ; D1 est stable. Le gain de ×4,59 sur D7 est, lui,
très au-dessus du bruit.

---

## 5. Exactitude — tests différentiels

Deux programmes Rose sont exécutés avec le binaire de départ et le binaire
final ; **sorties octet pour octet identiques** (`diff -u` vide) :

- `/tmp/opencode/benchmulti/table_exactitude.rose` → 43 lignes
- `/tmp/opencode/benchmulti/table_stress.rose` → 8 lignes

Extraits des sorties exactes :

```
0            # table vide : longueur
[]           # chaine_de(vide)
faux         # cle_existe(vide)
faux         # supprimer(vide)
3            # longueur après 4 insertions (1 clé mise à jour)
[3 => 30, 1 => 11, 2 => 20]   # ORDRE d'insertion préservé
[3, 1, 2] / [30, 11, 20]      # cles / valeurs
vrai / faux / 11              # cle_existe, clé absente, $t[1]
vrai / vrai / 70              # clé négative -7, grande clé 2^53+1
1 / CD / CD / [ab]            # clé rope "a"+"b" ≡ "ab" → mise à jour, pas de doublon
vrai / vrai / demi / [0.5, 0.75]  # clés rationnelles 1/2 et 2/4
vrai / vrai / vrai            # 5 ≡ 5/1 ≡ 5.0 (égalité inter-types)
1 / rationnel                 # insertion 5/1 = mise à jour de 5, longueur = 1
vrai / 4 / [3, 2, -7, 9007199254740993]  # suppression + décalage des positions
faux / vrai / 20 / vrai / [3, 2, -7]      # clé supprimée absente, clés restantes
vrai                          # égalité de tables (ordre d'insertion différent)
vrai / 80 / [3, 2, -7, 8]     # $t[8] = 80 ajoute une clé en fin
vrai / 1                      # clé non hachable (table) → repli linéaire
```

Stress (1 000 clés, 500 suppressions, 500 réinsertions) :
`500 / 0 / 1 / 999 / 1000 / 1000 / 14 / 15` — identique au départ.

---

## 6. Validation complète (binaire final)

| Gate | Résultat |
|---|---|
| `cargo build --release --bin rose` | OK, binaire copié dans `rose/rose` |
| Jardin `jardins/tests/premiers.rose` | **20 tests, 0 échec** |
| `fib25` | **75025** |
| Point fixe (`rose rir …` et `rose … …`) | **`bdbef63681608a0b22a5637539e9d738` ×2** |
| `bash scripts/audit-final.sh` | oracles **152/0 ×4**, verdict « Portes NON atteintes : AUCUNE » |
| `cargo test --release` | **160 + 144 + 6 + 1 passed, 0 failed** |
| Bench 2 salves A/A c19 + c15 | 9 domaines, tableau ci-dessus |
| `git status --short` | `M rose/src/interp.rs`, `M rose/src/run.rs`, `?? rose/docs/TABLE_NATIVE.md` |

Aucun commit, aucun push, aucune publication.

---

## 7. Rejouabilité

```
cd /tmp/opencode/benchmulti
PIN_CPU=19 NSAMP=9 python3 bench.py      # multi_summary.json
PIN_CPU=15 NSAMP=9 python3 bench.py
sha256sum /home/clausake/Stefan/ROSE/rose/rose
# 6e3ff795456d3f9bcf01c74dcb5a443c65e969c29ee8515b18b6497a5dacf801
```

Artefacts : `bench_table_base_c{15,19}.log`, `table_base_c{15,19}_summary.json`,
`bench_table_FINAL_c19{a,b}.log`, `table_FINAL_c19{a,b}_summary.json`,
`bench_table_FINAL_c15.log`, `table_FINAL_c15_summary.json`,
`prof_table_decomp.py`, `prof_table_phases.py`, `table_decomp_c{15,19}.log`,
`table_phases_c{15,19}.log`, `table_floor_FINAL_c19.log`,
`table_variantes_c{15,19}.log`, `table_crossdomain_c19.log`,
`table_exact*.txt`, `table_stress*.txt`, `table_audit_final2.log`.

**Règle** : ceci est un constat daté (18/11/2026, 16:34–17:08Z). Re-mesurer
après tout changement de moteur, de quantification ou de topologie.
