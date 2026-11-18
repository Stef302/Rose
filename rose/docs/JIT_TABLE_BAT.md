# Vague 23 (D7) — table native sous JIT du principal

**Constat daté : 18/11/2026, 20:25–20:31Z (salves 9 domaines 20:25–20:26Z). Binaire final
`e89f430fd76fb9a4ae01f4a8c0dd794442758770ae39166dc03e275cf3219fdd`.**
Binaire de départ : `caa7ba9c75871258c97fc397bc1d3b122f036075443e4b2ea7bc9b44c0020da1`
(sauvé `/tmp/opencode/benchmulti/base_rose_d7`). C'est une **mesure**, pas une
consigne ; re-mesurer après tout changement de JIT, de RIR ou de topologie.

## 1. Reproduction et diagnostic — PRÉMISSE CORRIGÉE

Le constat d'entrée disait « le résidu est le dispatch interprète, ~226 ns/op ».
**C'est incomplet.** La mesure directe montre que le principal de `r_table.rose`
**n'était pas compilé du tout** :

```
$ ROSE_TIMING=1 rose r_table.rose
TIMING-JIT memoire finalize=... regions=0      # <-- aucun code natif
TIMING ... compileprincipal=... exec=1.39ms
$ ROSE_DUMP_CODE=... -> fichier de 0 octet
```

Cause prouvée par dump RIR (instrumentation `ROSE_DUMP_RIR_JIT` ajoutée puis
**retirée**) : les 17 opérations du principal contiennent
`ExprStmt(Appel "inserer", …)` et `SautSiFaux { test: Appel "cle_existe", … }`.
`rir_compilable_jit` les rangeait dans le `_ => return false` : le principal
tournait dans `rir::executer_avec_ptr_sync`, qui resynchronise **6 slots dans
les globales après CHAQUE opération**. Le compteur `ROSE_PROFIL_RIR` affichait
`ops_executees=0` parce qu'il ne couvre que `executer_avec_ptr` (sans sync) —
ce 0 était un faux positif, pas une preuve de natif.

Mesures de cadrage (9 échantillons, c19) :

| Chemin | Temps | Lecture |
|---|---:|---|
| `ROSE_NO_RIR=1` (AST pur) | 3,62 ms | référence lente |
| RIR sync (binaire de départ) | 1,36 ms | ×2,66 — le gain T1 venait du RIR + index |
| `ROSE_TIMING` + JIT principal | — | `regions=0` : aucun natif |

Corollaire : l'option (a) « étendre les délégués W1 à `inserer`/`cle_existe` »
n'était **pas applicable telle quelle** — `declarer_delegues` refuse une
fonction dont un paramètre n'est pas `compilable`, or `inserer` prend
`table`/`tout`. Il n'existe aucun délégué pour ces primitives : le coût réel
était la boucle RIR à synchro par opération, pas la table de délégués.

## 2. Décision retenue et preuve

Étendre le JIT du **principal** au couple `table()` / `inserer` / `cle_existe`,
avec le handle de table maintenu dans un registre (comme `NatureSlot::ListeEntiers`
pour D6) et des **helpers runtime** — c'est l'option (b) demandée, généralisée à
tout compteur entier borné (pas seulement le motif exact du benchmark).

- `NatureSlot::Table` nouveau ; `table()` alloue un handle **une fois** à
  l'émission (`rose_jit_table_vide`), exactement comme `[]`.
- Trois helpers dans `helpers_runtime()` / `table_symboles_runtime()` :
  `rose_jit_table_vide`, `rose_jit_table_inserer_i64`,
  `rose_jit_table_cle_existe_i64`. Le handle pointe sur le **vrai**
  `Valeur::Table` (Rc partagé) : les helpers appellent `Table::inserer` /
  `Table::trouver`, dont l'index `HashMap<u64,usize>` (T1) reste la source de
  vérité. `longueur`, `cles`, `valeurs`, `$t[k]` et l'affichage voient donc les
  mêmes paires que l'interpréteur — c'est ce qui rend l'optimisation sûre pour
  les clés chaîne, rationnelles, négatives et les mises à jour.
- Preuve statique, **sans repli à l'exécution** :
  - `slot_cle_nu_borne(corps, s)` — init bornée (|v| ≤ 2³³), pas `±petit
    littéral` (|d| ≤ 4095), test de boucle `s < L` borné. Cela écarte
    explicitement le pas `±2^62` qui boxerait en i64 et ferait passer un
    pointeur Mpz comme clé ;
  - `valeur_nue_ast` —Var prouvé, littéral entier, ou `compteur * c` (c ≤ 4095)
    sous la même borne, émis en `imul` direct ;
  - gardes dans `rir_compilable_jit` **et** dans `emettre_fonction_rir` (les
    deux doivent accepter, sinon repli interprété).

Tout ce qui ne satisfait pas le garde (clé littérale dans `cle_existe`, clé
constante, `>> longueur($t)` en fin de programme, `supprimer`, appels, blocs…)
**retombe sur l'interpréteur, inchangé**. Ce conservatisme est mesuré plus bas.

Le code machine final (objdump de `ROSE_DUMP_CODE`, 4096 o) montre le chemin
attendu : `blr` vers `rose_jit_table_inserer_i64` avec `(handle, i, i*2)` en
registres dans la première boucle (0x298–0x344) et vers
`rose_jit_table_cle_existe_i64` dans la seconde (0x174–0x290) — aucun
`HashMap<String,_>`, aucune `Valeur` boxée sur ces chemins.

### Rejets

- **Délégués W1 (a)** : non applicables, voir §1 (diagnostic + signature non
  compilable). Aucune mesure de variante nécessaire.
- **Table i64 parallèle** (index `HashMap<i64,i64>` distinct de la
  `Valeur::Table`) : rejetée **sur preuve de correction**, pas au chrono — elle
  rendrait `longueur`/`cles`/valeurs/affichage incohérents avec l'état réel.
- **Spécialisation des incréments `c = c + 1` / `j = j + 1`** (12 instructions
  de contrôle de débordement chacune, visibles à l'objdump) : non retenue ici —
  hors du périmètre « table » et toucherait tous les domaines. Le gain restant
  est documenté en §4, pas revendiqué.

## 3. Résultats — 9 échantillons entrelacés, piné, 3 salves

Harnais `/tmp/opencode/benchmulti/bench.py` (9 domaines, rotation des
implémentations) et A/A/A dédié `/tmp/opencode/d7/aaa_table.py`
(base / nouveau / Rust entrelacés).

### D7 seul (médianes, ms)

| Salve | Départ | Nouveau | Rust `-O` | Départ/Nouveau | Nouveau/Rust |
|---|---:|---:|---:|---:|---:|
| c19 A (9) | 1,3649 | **0,2717** | 0,0924 | ×5,02 | 2,94 |
| c19 B (9) | 1,3546 | **0,2705** | 0,0938 | ×5,01 | 2,88 |
| c15 C (9) | 1,3436 | **0,2754** | 0,0936 | ×4,88 | 2,94 |

Min/max nouveau : 0,2624 – 0,3252 ms. **Porte < 0,40 ms : ATTEINTE** sur les
trois salves, avec marge. Rust n'est **pas** battu : écart ×2,9 assumé.

### 9 domaines (médiane interne, ms ; `AVANT` = salve c19A de `588dd99`)

| Domaine | AVANT | c19A | c19B | c15C | Rust | xRust A/B/C |
|---|---:|---:|---:|---:|---:|---|
| D1 fib(35) | 11,603 | 11,540 | 11,634 | 11,563 | 12,695 | 0,91 / 0,92 / 0,91 |
| D2 boucle 10⁶ | 0,260 | 0,259 | 0,260 | 0,259 | 0,263 | 0,99 / 0,99 / 0,98 |
| D3 premiers 50 k | 0,854 | 0,852 | 0,854 | 0,853 | 1,002 | 0,85 / 0,85 / 0,85 |
| D4 fact 5 000 | 1,434 | 1,441 | 1,447 | 1,437 | 6,534 | 0,22 / 0,22 / 0,22 |
| D4b PGCD 1 000 ch. | 6,728 | 6,713 | 6,727 | 6,731 | 0,147 | 45,5 / 45,6 / 45,7 |
| D5 chaîne 20 k | 0,929 | 0,926 | 0,944 | 0,945 | 0,014 | 65,4 / 65,3 / 65,3 |
| D6 liste 100 k | 2,132 | 2,144 | 2,144 | 2,147 | 0,188 | 11,7 / 11,6 / 11,4 |
| **D7 table 3 000** | **1,354** | **0,278** | **0,278** | **0,277** | 0,095 | **2,95 / 2,96 / 2,90** |
| D8 E/S 10 Mo | 4,998 | 6,068* | 4,891 | 4,926 | 4,841 | 0,98 / 1,00 / 1,02 |

`*` bruit E/S déjà observé sur les salves précédentes (4,9–6,1 ms), sans
changement de code E/S. Aucune régression reproductible : D6 +0,5 % (bruit),
D4/D5 < 2 %. Les 3 victoires antérieures (D1, D3, D4) et les 2 domaines au
niveau (D2, D8) sont conservés.

## 4. Décomposition chiffrée (c19, 9 échantillons entrelacés)

Programmes isolés ; les fenêtres `temps()` ne contiennent que la phase mesurée.

| Phase | Départ | Nouveau | Par op. (nouveau) |
|---|---:|---:|---:|
| `r_table` complet (3 000 ins + 3 000 look) | 1,346 | **0,273** | 45,5 ns |
| 3 000 insertions seules (JIT) | 0,714* | **0,214** | 71,4 ns |
| 3 000 recherches seules (JIT) | 0,627 | **0,071** | 23,7 ns |
| 3 000 recherches sur table vide (JIT) | 0,430 | **0,029** | 9,7 ns |
| 3 000 mises à jour clé existante | 0,588* | 0,585* | — |
| 6 000 itérations `c = c + 1` (JIT) | 0,339 | 0,340 | 56,7 ns/it |
| `cle_existe(t, 5)` à clé constante | 1,160 | 1,160* | — (repli) |

`*` programme **non JIT compilé** (`regions=0`) parce qu'il se termine par un
`Afficher` AST (`>> longueur($t)`) ou parce que la clé n'est pas un compteur
borné : le garde refuse et l'interpréteur reprend. Ces lignes mesurent la
**frontière** de la spécialisation, pas une régression.

Lecture : le passage de 1,36 → 0,27 ms est presque entièrement dû à la
**compilation native du principal** (suppression du `synchroniser_slots` par
opération et de l'évaluation AST de `inserer`/`cle_existe`). Le coût résiduel
n'est pas le hachage : 71 ns/insert et 24 ns/lookup incluent déjà le handle,
`Valeur::z`, `Table::h` et la table de hachage canonique. Le vrai résidu
observable est le contrôle de débordement i64 des compteurs
(`c = c + 1`, `j = j + 1` : ~12 instructions et un `blr` sur le chemin lent
chacun), mesuré à 56,7 ns/itération sur la boucle témoin — hors périmètre de
cette étape.

## 5. Exactitude et validation

- `cargo build --release --bin rose`, copié dans `rose/rose` ; sha256
  `e89f430f…fdd`. `git status` : **un seul `.rs` modifié** (`rose/src/jit.rs`) ;
  aucun commit, aucune publication.
- **Diff vide** (nouveau vs binaire de départ) :
  - `table_exactitude.rose` : **43 sorties**, sha identique
    `95cd652a…d569` ;
  - `table_stress.rose` : **8 sorties**, sha identique `2c9d18ad…20a8` ;
  - 9 programmes différentiels supplémentaires (`/tmp/opencode/d7/`) : clés
    chaîne (2), rationnelles (2), négatives (2), mises à jour, égalité de
    tables, variants à chemin JIT (`jit_path_*`) — **diff vide** sur tous ;
  - les variants `jit_path_neg` et `jit_path_update` ont `regions=1` (chemin
    JIT effectivement exercé) et le même résultat que le binaire de départ.
- `bash scripts/audit-final.sh` → **rc=0** : oracles **152/0 ×4**, point fixe
  `bdbef63681608a0b22a5637539e9d738` **×2**, jardin **20/20**, `fib(25)=75025`,
  « Portes NON atteintes : AUCUNE ».
- `cargo test --release` → **0 échec** (160 + 144 + 6 + 1 tests, 0 failed).
- Instrumentation temporaire `ROSE_DUMP_RIR_JIT` **retirée** avant le build
  final (vérifié : `grep ROSE_DUMP_RIR_JIT rose/src/jit.rs` vide).

## 6. Fichiers

- Code : `rose/src/jit.rs` (seul fichier Rust touché).
- Doc : `rose/docs/JIT_TABLE_BAT.md` (ce fichier).
- Artefacts : `/tmp/opencode/d7/` (AAA, decomposition, audit, dumps, programmes
  différentiels), `/tmp/opencode/benchmulti/` (harnais et salves 9 domaines).
