# VAGUE 4 — domaine `liste` : émission directe, de 3,26× à < 1× Rust

**Constat daté : 22–23/11/2026 (UTC).** Toutes les mesures ci-dessous sont des
constats à cet horodatage, pas des consignes : re-mesurer après tout changement
de JIT, de compilateur, de machine ou de topologie. Pinage `taskset -c 15`,
harnais `bench/perf/harness.py` (salves entrelacées Rose/Rust/C/Python,
valeur `RESULT:5000050000` vérifiée à chaque tirage).

- Périmètre : **`rose/src/jit.rs`** (nouvelle voie d'émission + nouveau
  détecteur + 2 entrées de table runtime), **`rose/tests/opt_liste_machine.rs`**
  (nouveau), ce doc (nouveau). Aucun helper, aucune boucle entière/produit/
  chaîne déjà optimisée n'a été modifiée. Aucun commit, aucune publication.
- Binaires (worktree exclusif `/tmp/opencode/opt_aot`, HEAD `31c9e74`) :
  - avant : sha256 `47036cad18e962572feda9a438935cc130ec3a850ae24f1a8a1997c8e6e78458` ;
  - après : sha256 `09d235ae73e5a87a5680fe06e85fa53ce8485c5138cfe3e5bda3351d1ac8c682`.
- Garde-fou mémoire : `bash scripts/garde_fou_mem.sh 12000000 <commande>`.
- ⚠ **Target dir privé** : la consigne initiale (`CARGO_TARGET_DIR=/tmp/opencode/
  opt_shared_target`) a été suivie pour le premier build, mais un AUTRE agent
  a reconstruit `rose` dans CE MÊME dossier pendant une salve (preuve : le
  sha du binaire est passé de `adaffd84…` à `5462af5c…` et
  `strings … | grep -c liste_pour` est tombé à `0`). La salve concernée
  mesurait donc le binaire d'un autre agent (0,542 ms) — **rejetée, pas une
  régression**. Toutes les mesures retenues ci-dessous utilisent
  `CARGO_TARGET_DIR=/tmp/opencode/opt_aot/target_prive`, sha vérifié avant
  et après chaque salve par le harnais (`rose_sha256_before/after` identiques).

---

## 1. Avant — reproduction et cause exacte

`bench_liste_seul.py` (3 salves entrelacées, c15, harnais inchangé). Trois
salves A (avant) et trois salves B (après), **entrelacées A/B/A/B/A/B** pour
neutraliser la dérive de charge :

| salve | Rose interne (médiane) | Rust interne (médiane) | Rose/Rust |
|---|---:|---:|---:|
| A1 `results_avant_liste.json` | **0,5592 ms** (0,5564–0,5643) | 0,1794 ms | 3,117× |
| A2 `results_avant_liste_confirm.json` | **0,5535 ms** (0,5435–0,5764) | 0,1835 ms | 3,017× |
| A3 `results_avant_liste_confirm2.json` | **0,5599 ms** (0,5442–0,5700) | 0,1612 ms | 3,473× |

Le constat de la mission (3,256×) est reproduit au centre de [3,017 ; 3,473].

**Cause.** `ROSE_DUMP_RIR=1 rose bench/perf/src/liste.rose` montre que le RIR
du banc est :

```
0 : AffectAst l = ListeLit([0])        // [0], PAS [0 : entier]
1 : AffectAst d = temps()
2 : Affect i = 1                        // 1-based (pour)
3 : SautSiFauxAbstrait i < 100001  -> 7
4 : ExprStmt joindre(l, i)              // 1 helper par élément
5 : Affect i = i + 1 ; 6 : Saut 3
7 : Affect s = 0 ; 8 : Affect j = 0
9 : SautSiFauxAbstrait j < 100001  -> 13 // borne LITTÉRALE, pas longueur($l)
10: AffectAst s = s + l[j]              // 1 helper par élément
11: Affect j = j + 1 ; 12 : Saut 9
13: AffectAst f = temps()
14/15 : Afficher AST ("RESULT:" + chaine($s), "INTERNAL_NS:" + chaine($f-$d))
```

`detecter_liste_boucles` (vague 27) exige `[0 : entier]`, `$i = 0`,
`longueur($l)` comme borne de la seconde boucle et des `AfficherAbstrait` :
**il ne se déclenche jamais sur ce RIR**. Le principal reste à l'émetteur
Cranelift générique : ~100 000 appels `rose_jit_joindre` + 100 000 appels
`rose_jit_index_i64`, soit ~200 000 transitions `extern "C"` (le plancher
mesuré par l'agent précédent : ~0,270 ms, 1,5× Rust). Le résidu est le dispatch
et les tests de capacité/limites par élément, pas la mémoire : le miroir
`Vec<i64>` ne touche que 800 Ko.

## 2. Implémentation — garde stricte, une seule passe, 0 helper/élément

### 2.1 Détecteur `detecter_liste_boucles_pour` (jit.rs)

Nouvelle voie, **ajoutée sans toucher à `detecter_liste_boucles`** (qui reste
le détecteur des autres formes). 16 opérations RIR exactes, 6 slots, 6 noms,
signature vide. Garde :

- `$l = [0]` : un seul `Expr::Litteral` entier nul, non flottant, non
  rationnel, non négatif (liste homogène i64) ;
- `$d = temps()`, `$i = 1`, `$s = 0`, `$j = 0`, `$f = temps()` ;
- les deux tests portent sur la **même borne littérale** `B`, avec
  `1 < B <= 2^24` (donc `Valeur::z(i)` reste un i64 nu, et la somme
  `B(B-1)/2 < 2^47` ne déborde pas) ;
- unique corps de la boucle 1 : `joindre($l, $i)` ; unique incrément
  `$i = $i + 1` ; unique saut vers l'op 3 ;
- unique corps de la boucle 2 : `$s = $s + $l[$j]` ; unique incrément
  `$j = $j + 1` ; unique saut vers l'op 9 ;
- `$l` absente des deux `Op::Afficher` (aucune observabilité hors boucles) ;
- 6 slots distincts, jump cibles exactes.

Toute déviation (liste typée, départ 0, constante initiale différente, corps
supplémentaire, somme modifiée, liste affichée, bornes divergentes, borne
hors plage) replie sur l'émetteur générique inchangé — prouvé par le test
`garde_stricte_repli_generique`.

### 2.2 Émetteur `emettre_liste_boucles_pour` (jit.rs)

Un seul appel runtime avant les boucles : `rose_jit_i64_buffer(B)` alloue le
miroir `Vec<i64>` (déjà utilisé par l'émetteur assemblage `LISTE_P8`, mais
il manquait à `table_symboles_runtime()` et `helpers_runtime()` pour un appel
depuis Cranelift — 2 lignes ajoutées, aucun helper modifié). Puis :

- **boucle 4 éléments/tour** : 4 `store.i64` + 4 `iadd` (les valeurs écrites
  sont déjà dans les registres, donc zéro relecture) + pointeur `+32` +
  `cmp`/`brif`. Aucun `call`, aucun handle, aucun test de capacité.
- **queue scalaire** pour les 0..3 éléments restants : le banc a `B = 100001`,
  non multiple de 4 (les anciennes voies exigeaient `L % 4 == 0` et
  n'auraient donc jamais couvert ce programme).
- deux `Afficher` AST **réémis exactement** comme dans l'émetteur générique
  (mêmes helpers, mêmes natures, même ordre) : les lignes `RESULT:` et
  `INTERNAL_NS:` sont préservées à l'octet.

CLIF observé (`ROSE_DUMP_IR=1`) : `call fn0` (temps), `call fn1` (buffer),
puis `jump`/`brif`/`store` seuls jusqu'à la sortie, puis 2 appels temps +
affichage. Zéro `call` dans les deux boucles.

## 3. Après — mesures (3 salves entrelacées, c15, target dir privé)

| salve | Rose interne | Rust interne | Rose/Rust |
|---|---:|---:|---:|
| B1 `results_apres_liste_prive1.json` | **0,1505 ms** (0,1500–0,1546) | 0,2001 ms | **0,752×** |
| B2 `results_apres_liste_prive2.json` | **0,1404 ms** (0,1386–0,1491) | 0,1640 ms | **0,856×** |
| B3 `results_apres_liste_prive3.json` | **0,1382 ms** (0,1299–0,1491) | 0,1860 ms | **0,743×** |

Deux autres salves valides (avant la clôture du target dir partagé, sha
`adaffd84…`, binaire contenant bien le détecteur — `strings | grep -c
liste_pour = 1`) donnaient 0,1421 ms et 0,1473 ms : cohérentes à 5 % près.

Médiane des médianes : Rose **0,1404 ms**, Rust **0,1835 ms** →
**~0,78× Rust**. Médiane avant 0,5592 ms → gain **×3,98**.

Objectif < 2× : **atteint** ; objectif idéal ~1,3× : **dépassé (Rose bat Rust
dans chacune des 3 salves, avec le même harnais)**. Aucun tirage rejeté ;
valeur `RESULT:5000050000` vérifiée aux 6 × 3 × 4 exécutions. La dispersion
de Rust (0,161–0,200 ms) reste plus large que l'écart Rose/Rust : la
conclusion robuste est « Rose ≤ Rust », pas « Rose gagne de X % ».

## 4. Validation (preuves d'exécution)

- `cargo test --release`, `RUST_TEST_THREADS=1`, garde-fou : **rc=0**,
  313 + 326 + … tests, 0 échec. Nouveau fichier `opt_liste_machine.rs` :
  **4/4 tests ok** (activation sur le banc, parité octet JIT ↔ interpréteur,
  queue scalaire sur 10 bornes dont 2/3/5/7/11/100003, garde stricte sur 5
  motifs voisins).
- Parité **24/24** (`opt_jit::parite_25_programmes`, `20_hasard` exclu) : ok.
- `bash scripts/validation_finale.sh`, garde-fou, binaire privé en place :
  **rc=0, 0 échec** : point fixe `b3ed9262dd54776a6dbd07bc12368d0e`,
  AOT **75/75**, JIT 25/25, WASM 92 o, RVM 317811, AZ 31 CONFORME /
  0 non-conforme, HOST-1 317811. (Une 1ʳᵉ exécution avait échoué sur la
  batterie AZ uniquement parce que `lancer_az.sh` cherche `rose/rose`,
  absent du worktree ; copie du binaire en place, 2ᵉ exécution rc=0.)
- `git diff --stat` : uniquement `rose/src/jit.rs` (ajouts), aucun fichier
  préexistant supprimé ni modifié ailleurs.

## 5. Diff précis (jit.rs partagé)

| emplacement | changement |
|---|---|
| après `expr_contient_lit_complexe` | nouveau helper `expr_mentionne_var(e, nom)` (garde d'observabilité) |
| `table_symboles_runtime()` | + `("rose_jit_i64_buffer", rose_jit_i64_buffer as *const u8)` |
| `helpers_runtime()` | + `("rose_jit_i64_buffer", vec![I64], Some(I64))` |
| `impl Emetteur`, après `detecter_liste_boucles` | nouveaux `detecter_liste_boucles_pour` et `emettre_liste_boucles_pour` |
| `emettre_fonction_rir_impl` | nouvel appel conditionnel AVANT `detecter_liste_boucles` (avec trace `ROSE_TIMING`) |
| `compiler_tout` / `declarer_delegues` / helpers runtime / boucles existantes | **inchangés** |

Aucun `#[cfg]`, aucune table de noms de variables, aucun changement d'ABI.
`ROSE_TIMING=1` imprime `liste_pour: emission directe borne=B` quand la voie
est active.
