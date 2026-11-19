# RUN_OFFENSIVE — offensive libre dans `run.rs` (CONSTAT DATÉ, 19/11/2026)

**Auteur** : sous-agent dédié `run.rs`. **Aucun commit, aucune publication, `rose/rose` intact,
aucun autre `.rs`, `Cargo.toml`, `PLAN.md` ni doc d'un autre agent modifié.**
Binaire construit dans `/tmp/opencode/run2_agents/`, jamais installé dans l'arbre.

---

## 0. Protocole réellement exécuté

| Élément | Valeur mesurée |
|---|---|
| HEAD annoncé | `f5a82ad` |
| Arbre | des agents concurrents ont modifié `jit.rs`/`interp.rs` **pendant** la session ; chaque mesure A/A compare donc deux binaires bâtis sur le **même** arbre, seul `run.rs` diffère. |
| Binaire final | `/tmp/opencode/run2_agents/rose_final2`, SHA256 `b66c26007cf2ca3a09ff6a0b2f0b1abe39aa2b0c0350e51ec27e8aa54776a467` |
| Contrôle A | `/tmp/opencode/run2_agents/rose_base2` = `run.rs` d'origine + arbre courant, SHA256 `10388abe61f65c0e5adb454b1e7bda99364af6a9788383f4114fb1564439ad9f` |
| Pinage | **`taskset -c 16`** (Cortex-X925, `cpu_capacity=1017`). Les chiffres ne sont PAS comparables à ceux du `COMPARATIF_TOTAL.md` pinés `c11` (A725, 731). |
| Échantillons | **9 par implantation**, entrelacés par rotation A/B, dans la même salve. Aucune mesure isolated. |
| Horloges | H2 interne du programme (`$f - $d`, en ns) pour D7 ; mur `perf_counter` + CPU `getrusage` pour la suite. |
| Valeurs | vérifiées à **chaque** tirage : D7 `3000`, suite `0.9999999999860738` (f64 bit-égal sur 4 moteurs). |
| Fenêtre | `2026-11-19T22:34Z` → `22:52Z`. |

---

## 1. Domaine 1 — table 3000 clés : **PORTE ATTEINTE**

Le coût mesuré avant optimisation se décomposait (sonde `probe_table_parts.rose`, 5 tirs, c16) :
création de la table ~8–12 µs, **insertions 60–84 µs**, recherches ~18,5 µs.
Les 3 000 insertions payaient un `HashMap::entry` + `insert` alors que les clés `0..2999`
sont **déjà** indexées par `array_part[k] = position+1` (16 Kio).

### Changement (run.rs, 80 lignes ajoutées / 18 retirées)

1. **`array_part` devient l'index primaire** pour toute clé numérique dont la valeur est un
   entier exact dans `[0, N_ARRAY)` — helper `Table::k_table` (Entier nu, Flottant intégral,
   Rationnel dénominateur 1, Booleen, Entier boxé). Ces clés **n'ont plus d'entrée dans
   `index`**.
2. **Toutes les voies de recherche consultent `array_part` d'abord** (`trouver_h`,
   `contient_i64`, `inserer_i64`), et **chaque hit est validé par `valeur_egale`** contre
   l'item stocké. L'égalité sémantique est donc inchangée, y compris
   `Entier ≡ Flottant ≡ Rationnel ≡ Booleen` ; les collisions et le repli linéaire restent
   opérationnels pour les clés non denses.
3. `reindexer` / `retirer` reconstruisent les deux index de façon cohérente.
4. **`index` est créé paresseusement** : `HashMap::with_hasher` sans `reserve(4096)`.
   D7 n'insère aucune clé non dense → ~64 Kio d'arène et son initialisation économisés.
5. Correctifs existants **préservés à l'octet près** : rationnel `f9b0966`, pool de pages
   `38a8804`, garde mpz `0b87f77`.

### Mesures

A/A/A entrelacé, pin c16, 9 échantillons, **même salve**, H2 interne :

| Implémentation | H2 méd (min–max) ms | mur méd ms | ratio/Rust |
|---|---:|---:|---:|
| Rose **base** (`run.rs` d'origine, arbre courant) | 0,090 (0,087–0,113) | 2,92 | 0,968 |
| Rose **patché** | **0,063 (0,060–0,068)** | 2,88 | **0,677** |
| Rust `HashMap` | 0,093 (0,091–0,133) | 1,15 | 1,000 |

**Porte « Rose ≤ 0,8 × Rust sur D7 dans la même salve » : 0,063 ≤ 0,074 → ATTEINTE.**
Le gain propre du patch est **0,090 → 0,063 ms, soit ÷1,43**.

Recoupement dans la salve complète des 9 domaines (`bench_cmp2.py`, pin c16, 9 tirages
entrelacés, valeurs vérifiées 81/81) :

| D7 | H2 méd ms | min | max | Rose/Rust |
|---|---:|---:|---:|---:|
| Rose patché | **0,062** | 0,058 | 0,071 | **0,667** |
| Rust `HashMap` | 0,093 | 0,091 | 0,096 | 1,000 |

Un tirage intermédiaire (avant le passage à l'`index` paresseux) avait donné 0,080/0,097 =
0,825 dans cette même salve, c.-à-d. **hors porte** : c'est ce constat qui a imposé la
seconde optimisation. La valeur retenue est la dernière salve, rejouée.

---

## 2. Domaine 2 — suite complexe : **PORTE NON ATTEINTE**

### Ce qui a été mesuré (et tenté)

A/A/A entrelacé, pin c16, 9 échantillons, valeur f64 vérifiée à chaque tirage :

| Implémentation | mur méd (min–max) ms | CPU méd ms | ratio Rose/Rust |
|---|---:|---:|---:|
| Rose base | ~317–323 | ~317 | ~190 |
| Rose **patché** (ce chantier) | **322,3 (317,3–332,7)** | 321,6 | **189,1** |
| Rust `-O` (`suite_rs`) | 1,7 (1,2–2,6) | 1,4 | 1,000 |

Aucun gain mesurable : la variation Rose base → patch est dans le bruit. C'est un résultat
négatif **réel**, pas une omission.

Diagnostic rejoué, pas cité :
* `ROSE_TIMING=1 ... probe_calc.rose` → `TIMING-JIT eligibles=[]` : **`calc` n'est jamais
  compilé par le JIT**, le temps est `exec=315 ms` d'interprétation pure.
* Décomposition (`probe_calc.rose`, `probe_somme.rose`) : `calc(10⁶)` seul = **313–316 ms**,
  la seconde boucle de carrés (elle, JIT-compilée en `__rose_loop_0`) = **3,6 ms**.
  Le goulot est bien la première boucle.
* `ROSE_SANS_JIT=1` ne change rien (313,1 ms contre 316,1 ms) : confirmation que ce chemin
  était déjà interprété.

### Blocage précis (nommer, pas contourner en trichant)

Deux portes, **toutes deux dans des fichiers interdits à ce sous-agent** :

1. `rose/src/jit.rs`, `bloc_compilable`, branche `Instr::Pour` :
   `if corps_contient_lit_complexe(corps) { return false; }`.
   Le corps de `calc` contient `$z_im * i`, et `i` est un `Expr::LitComplexe {0,1}` produit
   par le lexeur. Toute boucle portant le littéral imaginaire est donc refusée par le JIT,
   alors que `JitVal::Complexe` sait parfaitement l'émettre en deux registres f64.
2. `rose/src/interp.rs`, `binaire_valeurs` (~2934–3092) : chaque opération complexe y
   construit un `Valeur::Complexe { re, im }` et passe par `vers_d`. C'est le coût identifié
   par BIG1, hors périmètre de ce chantier.

Depuis `run.rs` seul, aucun levier n'atteint ces deux points : `run.rs` ne voit pas l'AST
avant `compiler_tout()`, ne contient aucun opérateur arithmétique complexe et aucun
enregistreur f64 thread-local exploitable. Trois pistes de la mission ont été évaluées :
* **voie rapide f64 sans allocation `Valeur`** : impossible à brancher — les opérandes sont
  construits par `interp.rs`, pas par un helper `run.rs` ;
* **opérations en place** : idem, l'affectation de slot est dans `interp.rs` ;
* **représentation compacte** : `Valeur` fait 24 octets, dont 16 de payload pour `Complexe`;
  un `impl Drop` manuel à voie rapide POD a été tenté puis **rejeté à la compilation**
  (`E0509`, type avec `Drop` dont on déplace des champs) et retiré. Même s'il compilait,
  ~9 Md'affectations × quelques ns ne peuvent pas fournir ÷5 sur 322 ms.

**Verdict honnête : ÷5 non atteint.** La cible était un ratio ≤ 268,6/5 = 53,7 (référence
`COMPARATIF_TOTAL`, c11) ou ≤ 182,7/5 = 36,5 (référence BIG1, c6). Mesuré : **189,1**
(c16). Il faudrait, pour l'atteindre, lever la porte `corps_contient_lit_complexe` dans
`jit.rs` **et/ou** ajouter un chemin complexe natif dans `interp.rs` — précisément les deux
fichiers exclus. Je ne les ai ni modifiés ni contournés.

---

## 3. Validations (toutes rejouées sur le binaire final `rose_final2`)

```
cargo build --release --bin rose                       rc=0
cargo test --release                                   rc=0
  220 + 204 + 0 + 6 + 1 + 0 = 431 passed, 0 failed (3+3 ignored)
jardins/tests/premiers.rose  → ── jardin/premiers : 20 tests, 0 échec ──
fib(25)                      → 75025
point fixe ×2 :
  rose rir compilateur/emetteur.rose                    → b3ed9262dd54776a6dbd07bc12368d0e
  rose compilateur/emetteur.rose compilateur/emetteur.rose → b3ed9262dd54776a6dbd07bc12368d0e
9 domaines multi (bench_cmp2.py, c16, 9 tirages)       rc=0, 81 valeurs vérifiées
bench/global/bench_global.rose                         rc=0 (dernière ligne 19)
```

---

## 4. État des lieux factuel

* **Fait** : `run.rs` modifié seul (+80/−18 lignes), table à index dense primaire.
* **Vérifié** : D7 Rose/Rust **0,063/0,093 = 0,68** en salve dédiée, **0,062/0,093 = 0,67**
  en salve multi → **porte ≤0,8 atteinte avec marge**. 431 tests/0, jardin 20/20, fib25,
  point fixe ×2, 9 domaines rc=0, global rc=0.
* **Non atteint** : suite complexe ÷5. Mesuré 189 au lieu de ≤36,5–53,7. Blocage nommé :
  `jit.rs::bloc_compilable` refuse tout corps de boucle contenant `Expr::LitComplexe`, et
  `interp.rs::binaire_valeurs` possède seul le chemin complexe interprété — deux fichiers
  exclus. Aucune triche, aucun bench modifié.
* **Reste** : si le périmètre est ouvert, lever la porte `corps_contient_lit_complexe`
  pour les boucles dont le type statique de l'accumulateur est `Flottant`, puis re-mesurer
  A/A/A. C'est la seule voie identifiée pour un ÷5 réel ; je ne l'ai pas faite ici.
