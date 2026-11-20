# SPEED3 — attaque du dernier front commun : parse et construction RIR (CONSTAT DATÉ)

Auteur : sous-agent `speed3`. **Aucun git commit, aucune publication, aucun
`rose/rose` remplacé.** Un seul fichier source du dépôt modifié :
`rose/src/front.rs` (+302/−40). `rir.rs`, `main.rs`, `interp.rs`, `jit.rs`,
`run.rs`, `i128x.rs`, `Cargo.toml`, `PLAN.md` et les docs des autres agents
n'ont **pas** été touchés. Tous les correctifs antérieurs présents dans
`front.rs` (CLOSE1 lecture source unique, CLOSE2, correction régression
orchestrateur, INT1 STDLIB 14 entrées) sont intacts.

**Horloge des mesures : 20/11/2026 ~12:20–12:56 UTC** (la mission annonce le
21/11). Tout chiffre ci-dessous est un constat daté, à re-mesurer après tout
changement de binaire, de pinage, de source ou de topologie.

---

## 1. Verdict

| Porte | Résultat |
|---|---|
| Rose ≤ Rust — suite complexe | ❌ **non atteinte** (ratio final/Rust 1,25–1,32) |
| Rose ≤ Rust — englobant | ❌ **non atteinte** (ratio 1,51–1,54) |
| Rose ≤ Rust — global | ❌ **non atteinte** (ratio mur CPU 1,91–1,96) |
| Gains internes mesurés (analyse) | ✅ **−51 µs englobant / −32 µs global / −20 µs suite** (cntvct, médianes de 9) |
| Gains au mur imputables | ⚠ **non démontrables** : différences appariées −10 à −69 µs, dans la bande de bruit A/A (−20 à +72 µs) |

Aucune triche : pas de formule fermée, pas de valeur codée en dur, aucun banc
modifié, aucun cache disque, mêmes valeurs de sortie vérifiées à chaque tirage.

---

## 2. Protocole réellement exécuté

| Élément | Mesure |
|---|---|
| HEAD annoncé / HEAD aux mesures | `6e8f6db` / `6e8f6db` (inchangé) |
| Arbre source isolé | `/tmp/opencode/speed3_agents/src_clean` (copie de `rose/` à l'arrivée, `Cargo.toml`, `Cargo.lock`, `build.rs`, `src/`) ; tous les binaires A/B/C construits avec `CARGO_TARGET_DIR` dédié |
| Binaire témoin | `rose_base`, SHA256 `04e9b29417ab7bedd48c3334ede8bb6191546fce13ecc8c71b4e721e43ca0b52` (HEAD non modifié) |
| Binaire final | `rose_final`, SHA256 `ab275c057f3e4b5982b319ec0dfd16e233336ca16be0a1fd1908f7e570c04e44`, static-pie |
| Jumeaux Rust | `suite_rs` et `eng_rs` copiés de `/tmp/opencode/speed2_agents` (SHA inchangés), `bench_global_rs` du dépôt |
| Pinage | `taskset -c 6`, `-c 16`, `-c 19` (capacités re-mesurées : les trois OK) |
| Échantillons | **12 par implantation et par cœur**, rotation A/B/C puis B/C/A à chaque répétition, médianes **et différences appariées** ; salves finales 12/12/12 sur les trois bancs |
| Horloges | CPU `getrusage(RUSAGE_CHILDREN)` pour les ratios ; `cntvct_el0` pour la décomposition (fréquence **mesurée** : `cntfrq_el0 = 1 000 000 000`, donc 1 tick = 1 ns) |
| Valeurs | suite `0.9999999999860738` / `333333833333500000` ; englobant `46297232.426252 / 564 / 157158 / 11899223598484` ; global 11 premières lignes — vérifiées à **chaque** tirage |
| Bruit | A/A (même binaire, c16, 12 tirages) : suite `final−base` méd +2,5 µs / moy +71,7 µs ; englobant méd −20 µs / moy −36 µs. Les gains au mur sont donc **plus petits que le bruit de la machine** |
| Instrumentation | temporaire, insérée seulement dans des copies `src_tree` / `src_prof2` hors dépôt, **retirée du livrable** (`grep -c 'sp3_\|cntvct' rose/src/front.rs rose/src/rir.rs rose/src/main.rs` = 0/0/0 ; seuls 4 commentaires `SP3` expliquent le pourquoi du code) |

Harnais : `/tmp/opencode/speed3_agents/bench_{suite,englobant,global,multi}.py`,
`prof_run.py` ; logs/CSV/JSON : `F_{suite,engl,global}_c{6,16,19}.{csv,json}`
et `AA_{suite,engl}_c16.*`.

---

## 3. Décomposition par phase (instrumentation temporaire, RETIRÉE)

Compteurs `cntvct_el0` insérés dans `front.rs` / `rir.rs` des copies isolées,
9 échantillons entrelacés c16, médianes en ns converties en µs.

### 3.1 Avant changement

| Phase | suite | englobant | global |
|---|---:|---:|---:|
| `tokeniser` (lexer) | 8,0 | 28,4 | 18,1 |
| `Parser::programmer` | 11,7 | 30,0 | 23,2 |
| repli de constantes (principal + corps) | 3,7 | 16,2 | 11,3 |
| détection compteur « i » | 0,2 | 0,35 | 0,37 |
| réparation « i » (recopie AST) | 4,5 | 16,0 | 8,1 |
| `verifier` | **20,2** | **47,5** | **31,8** |
| └ clone `prog.instructions` | 0,3 | 8,2 | 6,5 |
| └ pré-passe types | 0,3 | 9,2 | 2,7 |
| └ `verifier_bloc` principal | 3,0 | 16,4 | 8,8 |
| **TOTAL `front::analyser`** | **50,8** | **142,6** | **96,8** |
| RIR : `abaisser` + `abaisser_principal` | 0,9 | 18,6 | 9,4 |

**Goulot #1 mesuré = `verifier`** : 40 % de l'analyse sur la suite, 33 % sur
l'englobant, 33 % sur le global. Le sous-poste « copie intégrale de l'AST »
(clone `Signature` + clone `prog.instructions` + recopie par `replier_instr` /
`rep`) pèse à lui seul **~40 µs sur l'englobant**. Le mur global déclaré par
`ROSE_TIMING` (`parse=162,64 µs`, `transform=16,7 µs`, RIR) est cohérent avec
ces compteurs (la somme des phases + les appels non instrumentés donne ~160 µs).

### 3.2 Après changement

| Phase | suite | englobant | global |
|---|---:|---:|---:|
| lexer | 9,6 | 30,4 | 20,2 |
| parser | 10,1 | 28,8 | 21,0 |
| repli | 0,8 | 1,8 | 4,0 |
| détection « i » | 0,2 | 0,34 | 0,34 |
| réparation « i » | 0,2 | 1,3 | 0,6 |
| `verifier` | 8,1 | 23,8 | 16,4 |
| **TOTAL `front::analyser`** | **30,7** | **91,8** | **64,7** |
| RIR (`abaisser` + principal) | 0,8 | 22,0 | 9,6 |

Le lexeur et le parseur deviennent alors les deux postes dominants (~59 µs sur
l'englobant) ; ils n'ont pas cédé (voir §5).

---

## 4. La décision structurelle retenue (une seule, mesurée)

**Supprimer les copies intégrales de l'AST sur le chemin parse → vérification,
et partager le texte des jetons** — tout dans `front.rs`.

1. **Repli et réparation EN PLACE.** `replier_ip` / `replier_instr_ip` et
   `rep_ip` mutent les nœuds existants ; ils ne reconstruisent un `Expr` que
   quand un repli réussit réellement. Les anciens `replier_instr` et `rep`
   (qui recopiaient chaque `Instr`, chaque `String` d'opérateur, chaque
   sous-block) ne sont plus appelés sur le chemin d'analyse. La logique de
   décision est recopiée instruction pour instruction (y compris les deux
   branches `LitComplexe 1+1i`, `deplier_i_rec`, `deplier_corps_i`) : **aucune
   divergence de sortie mesurée** (515 tests, 81/81 ×2, 11 valeurs globales,
   point fixe ×2).
   Mesuré : repli 16,2 → 1,8 µs, réparation 16,0 → 1,3 µs sur l'englobant.
2. **Plus de clone de `Signature` ni de `prog.instructions` dans `verifier`.**
   `verifier_bloc` ne prend qu'un `&Programme` : l'itération sur
   `prog.signatures` et le double emprunt immutable de `prog.instructions`
   suffisent. Mesuré : `verifier` 43,8 → 23,8 µs sur l'englobant
   (l'instrumentation intermédiaire montrait encore le clone à 7,9 µs).
3. **`Token.texte` passe de `String` à `Rc<str>`.** `Parser::suivant()` clone
   un jeton à chaque consommation : le clone devient un incrément de
   compteur au lieu d'une allocation. Mesuré : parser 31,4 → 28,8 µs
   (~2,6 µs), gain réel mais sous le bruit du mur.

Cumul interne mesuré : **142,6 → 91,8 µs** (−50,8 µs) sur l'englobant,
**96,8 → 64,7 µs** (−32,1 µs) sur le global, **50,8 → 30,7 µs** (−20,1 µs)
sur la suite. RIR inchangé (aucune optimisation de `rir.rs` retenue :
l'abaissement n'est que 0,8–22 µs et son coût est dominé par l'AST déjà
recopié en amont).

### Pistes mesurées puis non retenues

| Piste | Mesure / raison | Décision |
|---|---|---|
| Lexeur un passe sans `Vec<char>` (scan octets) | Non implémenté : 30 µs mesurés, mais gain attendu ≤ 15 µs, sous la bande de bruit A/A (±20–72 µs), avec risque élevé sur les identifiants Unicode (`π`, `φ`, accents) couverts par les 515 tests | non retenu |
| `suivant()` par référence au lieu d'un clone | Refactor ~50 sites pour un gain attendu ~5 µs | non retenu |
| Table de symboles à indexation directe dans `verifier` | `expr_type` travaille sur des `HashMap<String, Ty>` ; modifier la sémantique des erreurs de type pour ~10 µs | non retenu |

---

## 5. Gains réels au mur (A/A/A entrelacé, 12 échantillons, CPU médian)

| Banc | Pin | base méd | final méd | Rust méd | final−base apparié | final/Rust |
|---|---:|---:|---:|---:|---:|---:|
| suite | c6 | 1,726 ms | 1,714 ms | 1,371 ms | **−9,5 µs** | 1,250 |
| suite | c16 | 1,845 ms | 1,846 ms | 1,518 ms | **−13,0 µs** | 1,216 |
| suite | c19 | 1,591 ms | 1,636 ms | 1,239 ms | **+37,0 µs** | 1,321 |
| englobant | c6 | 1,790 ms | 1,767 ms | 1,151 ms | **−36,5 µs** | 1,535 |
| englobant | c16 | 1,850 ms | 1,838 ms | 1,215 ms | **−49,0 µs** | 1,512 |
| englobant | c19 | 1,701 ms | 1,646 ms | 1,083 ms | **−68,5 µs** | 1,520 |
| global | c6 | 4,706 ms | 4,841 ms | 2,536 ms | **−11,5 µs** | 1,909 |
| global | c16 | 4,774 ms | 4,797 ms | 2,542 ms | **−30,5 µs** | 1,887 |
| global | c19 | 4,758 ms | 4,556 ms | 2,326 ms | **−122,5 µs** | 1,959 |

Honnêteté : le contrôle A/A (même binaire des deux côtés, c16, 12 tirages)
donne `final−base` = +2,5 µs (suite) et −20 µs (englobant) en médiane, avec
des moyennes de +72 µs et −36 µs. **Les différences ci-dessus ne sont pas
statistiquement séparables du bruit** sur le mur. Ce qui est prouvé sans
ambiguïté, ce sont les chronomètres internes (§4), mesurés sur le même cœur
avec la même horloge. Le banc principal 9/9 n'est pas régressé : les trois
différences appariées suite sont −9,5 / −13 / +37 µs, aucune tendance
systématique à la hausse.

---

## 6. Pourquoi les portes restent fermées — reste chiffré

Le parse + RIR représente maintenant, au total : **~114 µs** sur l'englobant,
**~74 µs** sur le global, **~31 µs** sur la suite. Ce sont les seuls postes
que ce chantier pouvait atteindre (`front.rs`, `rir.rs`, `main.rs`). Même en
les ramenant **à zéro** :

| Banc | Plancher Rose (mur actuel moins parse+RIR) | Rust (médiane c6/c16/c19) | Écart résiduel |
|---|---:|---:|---:|
| suite | 1,59 − 0,031 = **≥ 1,56 ms** | 1,37 / 1,52 / 1,24 ms | 0,04–0,32 ms |
| englobant | 1,65 − 0,114 = **≥ 1,54 ms** | 1,15 / 1,22 / 1,08 ms | 0,32–0,46 ms |
| global | 4,56 − 0,074 = **≥ 4,49 ms** | 2,54 / 2,54 / 2,33 ms | ~2,0 ms |

Le reste est structurellement hors périmètre :

- **suite** : l'accumulation complexe (~518 µs) et les carrés exacts (~71 µs)
  sont dans `interp.rs` / `i128x.rs` (interdits) ; rustc supprime la somme de
  carrés (formule fermée interdite côté Rose) ;
- **englobant** : S6 Monte-Carlo (~224 µs), Philox (~125 µs), S4/S5/S7
  (~34 µs), tous dans `interp.rs` / `run.rs` (interdits). SPEED2 avait déjà
  mesuré un plancher Rose ~500 µs internes contre ~287 µs au jumeau Rust
  `-O3 target-cpu=native` (codegen `Cargo.toml` interdit) ;
- **global** : tables (~0,6 ms, `interp.rs`), RNG (~0,34 ms, `run.rs`),
  rationnels exacts (~0,5 ms, `i128x.rs`), démarrage/chargement (~1,1 ms,
  `main.rs`/`run.rs`) — et une réserve sémantique : Rust déborde ses i64 et
  calcule en f64 là où Rose reste exact.

Ce ne sont pas des renoncements prématurés : le front commun a été attaqué,
mesuré et amélioré de 20 à 51 µs en interne ; il ne peut pas, à lui seul,
combler un écart de 0,3 à 2,0 ms situé dans des fichiers formellement interdits.

---

## 7. Validation (preuves de sortie réelles, binaire `rose_final` isolé)

```
cargo build --release --bin rose (dépôt, target dédié)        rc=0, static-pie
cargo build --release --bin rose (arbre isolé)               rc=0
cargo test --release (dépôt, target dédié)                   rc=0
  258 + 242 + 0 + 6 + 8 + 1 + 0 = 515 passed, 0 failed, 6 ignored
jardins/tests/premiers.rose   ── jardin/premiers : 20 tests, 0 échec ──
fib(25)                       75025
point fixe (2 voies)          b3ed9262dd54776a6dbd07bc12368d0e ×2
                              (rose rir … / rose … … ; stderr 0 octet des 2 côtés)
9 domaines multi, c6, 9 tirages   81/81 OK (sorties octet pour octet = base)
9 domaines multi, c16, 9 tirages  81/81 OK (sorties octet pour octet = base)
bench_global.rose                 rc=0, 12 lignes, 11 valeurs IDENTIQUES à la base,
                                  stderr 0 octet
detail géant (/tmp/opencode/demo_geant.rose) rc=0, propriétés dynamiques affichées,
                                  stderr 0 octet
suite complexe (chaque tirage)    0.9999999999860738 / 333333833333500000
englobant (chaque tirage)         46297232.426252 / 564 / 157158 / 11899223598484
```

Précision sur le contrôle multi : le harnais de SPEED2 n'ayant pas été retrouvé
intact sur cette machine, `/tmp/opencode/speed3_agents/bench_multi.py` a été
reconstruit (9 domaines : fib, boucle, premiers, factorielle, pgcd, chaîne,
liste, table, E/S ; 9 tirages par cœur). Les 81/81 sont une preuve de
**non-régression des sorties**, pas une réaffirmation des ratios du 20/11.

---

## 8. État final factuel

- **Fait** : décomposition chiffrée des trois bancs ; goulot #1 (`verifier` et
  ses copies d'AST) identifié ; décision structurale unique (analyse en place +
  jetons partagés) appliquée dans `front.rs` seul ; re-mesure c6+c16+c19 avec
  rotations appariées ; instrumentation retirée ; validations ci-dessus.
- **Vérifié** : −50,8 / −32,1 / −20,1 µs sur le temps interne d'analyse
  (englobant / global / suite, cntvct, médianes de 9) ; stabilité byte pour
  byte des sorties partout ; 515 tests verts ; aucun commit, aucune publication,
  aucun binaire installé dans `rose/rose`.
- **Non atteint** : `Rose ≤ Rust` sur les trois bancs. Ratios finaux : suite
  1,22–1,32 ; englobant 1,51–1,54 ; global 1,89–1,96. Blocage précis et
  chiffré : même en annulant tout le parse/RIR, il reste 0,04–0,32 ms (suite),
  0,32–0,46 ms (englobant) et ~2,0 ms (global) de calcul/startup situés dans
  `interp.rs`, `run.rs`, `i128x.rs`, `jit.rs` et dans la codegen native — tous
  hors du périmètre autorisé.
