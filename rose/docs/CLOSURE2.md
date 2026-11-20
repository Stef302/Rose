# CLOSURE2 — suppression du surcoût fixe du module JIT vide (CONSTAT DATÉ, 21/11/2026)

Auteur : sous-agent `close2`. **Aucun git commit, aucune publication, `rose/rose`
intact, aucun `.rose` modifié, aucun banc modifié, aucun document d'un autre
agent modifié.** HEAD annoncé : `c134b52f5f320b35d7f68159b3fc0bf44c31238c`.
HEAD en fin de session : `4e53f6a7050b1200cbafef50db03835aed44995d` (deux
commits concurrents, **docs et PLAN uniquement** : `git diff c134b52 4e53f6a`
est vide pour `jit.rs` et `interp.rs`). Aucun commit de ma part.

Fichiers possédés modifiés : `rose/src/jit.rs`, `rose/src/interp.rs`, ce document.
Binaire isolé : `/tmp/opencode/close2_agents/rose_final2`, SHA256
`92bc14b5614da25d1bbc5a319e4feb255480070b52c3269efa468fe78468051e`.
Témoin : `/tmp/opencode/close2_agents/rose_base` (même arbre avant mes deux
modifications), SHA256 `40616fb196bb624524a362159f761d72387b118704d34dd5b817a58ef714b10e`.
Rust : `suite_rs` (banc historique). Harnais : `bench_3way.py`, `bench_close2.py`
dans `/tmp/opencode/close2_agents/`.

---

## 1. Verdict — porte DURE NON ATTEINTE

**Rose ≤ Rust sur la suite complexe : ❌ non atteint.** Le surcoût fixe visé
est supprimé (mesurable, sans régression), mais l'écart résiduel reste
positif de **+119 à +174 µs CPU** selon le cœur et la salve.

Salve définitive A/A/A entrelacée (base, final, Rust), 31 rotations par cœur,
deux salves indépendantes, CPU `getrusage(RUSAGE_CHILDREN)`, valeurs f64 et
BigInt vérifiées à chaque tirage. Différences appariées (médianes) :

| Salve | c6 final−base | c6 final−Rust | c16 final−base | c16 final−Rust | c19 final−base | c19 final−Rust |
|---|---:|---:|---:|---:|---:|---:|
| 1 (31 rot.) | −74 µs | +127 µs | −61 µs | +122 µs | −40 µs | +174 µs |
| 2 (31 rot.) | −50 µs | +119 µs | −70 µs | +174 µs | −59 µs | +126 µs |

Écart du témoin `base` sur Rust dans les mêmes salves : +204 / +173 µs (c6),
+176 / +221 µs (c16), +271 / +183 µs (c19). **Gain réel de mon changement :
−40 à −74 µs par exécution**, stable en signe sur les trois cœurs et les deux
salves. Le bruit de la machine est élevé (load average 6,3–7,0 ; deux
processus Wine à ~100 % CPU ; un second processus `rose` à 100 % CPU pendant
les salves) : c'est précisément pourquoi la décision repose sur des
différences appariées entrelacées, pas sur des médianes marginales.

Aucune salve n'a montré Rose ≤ Rust. Je ne revendique aucune victoire.

---

## 2. Chemin EXACT du surcoût (reproduit avant modification)

`ROSE_TIMING=1 rose_base bench/suite/complexe.rose` (c6, tirages chauds) :

```
TIMING parse=42µs transform=6µs jitnew=18µs compiletout=80µs exec=609µs
TIMING-JIT eligibles=[]        ← 1er compiler_tout (depuis compiler_principal)
TIMING-JIT finalize=18µs
TIMING-JIT eligibles=[]        ← 2e compiler_tout (depuis interp.rs)
TIMING-JIT finalize=15µs
```

Chaîne exacte :

1. `interp.rs:6859` construisait `Jit::nouveau` (17–33 µs : JITBuilder, ~38
   symboles runtime, JITModule).
2. `interp.rs:6868` appelait **`declarer_delegues()`**, qui remplissait
   `delegues` et `decls` (imports runtime + fonctions non éligibles).
3. `compiler_principal` appelait `compiler_tout()` une 1re fois
   (`jit.rs:2896`) ; `interp.rs:6870` l'appelait une 2e fois.
4. Le raccourci « module vide » de sui1 exigeait
   `self.delegues.is_empty()`. L'étape 2 l'ayant rendu **toujours faux**, le
   raccourci ne déclenchait jamais. Les deux appels exécutaient donc
   `definir_delegues_restants()` + **`finalize_definitions()`** sur un module
   qui n'avait aucune fonction à définir : **2 finalises inutiles**.

Sur la suite complexe, `calc` est une superinstruction native
(`complex_natifs`) et `__rose_loop_0` une superinstruction de carrés
(`carres_natifs`) : `eligibles=[]`, donc aucun code Cranelift n'est produit.
Tout ce travail était du surcoût pur. Mesure instrumentée : `compiletout`
80–112 µs, 4 lignes `finalize=` (2 finalisations + 2 lignes mémoire) contre
`compiletout` 0,06–0,29 µs et **0 ligne `finalize=`** après correction.

---

## 3. Changement structurel (une décision, deux fichiers)

Décision : **ne rien initialiser, déclarer, compiler ni finaliser quand
aucune fonction n'est éligible et que le principal n'est pas compilable ;
repli complet dès qu'une fonction est éligible.**

### `rose/src/jit.rs`

* `installer_superinstructions()` (nouvelle méthode) : extrait de
  `compiler_tout` la construction des closures `Compile::Natif` des deux
  superinstructions (carrés NEON, accumulation complexe). Ces closures sont
  du Rust pur : elles ne touchent ni le module Cranelift ni les délégués.
* Raccourci « module vide » **rendu effectif** : la condition ne demande plus
  `delegues.is_empty()` ; elle est `eligibles.is_empty() &&
  self.foncs.is_empty() && self.permettre_carres_natifs`. On installe les
  superinstructions puis on sort avant `declarer_delegues`, les imports
  runtime, `definir_delegues_restants` et `finalize_definitions`. Le garde
  `permettre_carres_natifs` (faux seulement si le principal est JIT-compilé)
  assure le repli complet.
* `prepare_jit()` (nouvelle fonction publique) : décision de pré-exécution en
  une seule passe, **sans construire de module Cranelift**. Elle reproduit
  exactement les portes de `compiler_principal` et de la passe 1 de
  `compiler_tout` : principal compilable ? fonction éligible ? Elle réutilise
  le RIR du principal déjà calculé par `interp.rs` (pas de second
  `abaisser_principal`) et n'appelle `signatures_inferées` qu'une fois.
  Elle rend `(a_du_travail, superinstructions_natives)`.

### `rose/src/interp.rs`

* Suppression de l'appel préalable redondant à `declarer_delegues()` :
  `compiler_tout` le fait lui-même quand il y a du travail.
* Appel à `prepare_jit()` **avant** `Jit::nouveau`. Quand il répond `false`,
  on ne construit pas le Jit du tout : `jit_principal = None`, `jit = natifs`.
  Quand il répond `true`, chemin historique intact (`Jit::nouveau` +
  `compiler_principal` + `compiler_tout`).

### ⚠️ Bug trouvé et corrigé pendant la validation

Première version : `let mut jit = Jit::nouveau(...)` déclaré **dans** la
branche `if`. Le `Jit` était alors droppé à la fin du bloc, donc **avant**
`executer_principal()` : les pages de code exécutable (`MemoireJit`) étaient
unmappées pendant que les trampolines les appelaient → **SIGSEGV** reproductible
sur `fib(25)` et sur `rosevm::tests::programme_reel_fib_recursif_par_methodes`.
Corrigé par `_jit_garde: Option<Jit>` déclaré avant le `if` et assigné dans la
branche JIT : le Jit vit jusqu'à la fin de `executer_source_analyse`, comme
dans le code d'origine. Cause confirmée par bisection (le crash disparaît dès
que `jit` est déclaré hors du bloc). **Aucun correctif préexistant n'a été
affaibli.**

---

## 4. Pourquoi la porte reste fermée — reste chiffré

Le gain mesuré (−40 à −74 µs) est bien réel mais inférieur à l'écart. Après
suppression complète du poste fixe, la décomposition restante sur la suite
complexe est :

| Poste | Ordre de grandeur | Dans mon périmètre ? |
|---|---:|---|
| parse (`front::analyser`) | 40–60 µs | ❌ `front.rs` interdit |
| démarrage spécifique Rose (hors plancher commun) | ~20–30 µs | ❌ `main.rs` interdit |
| 2ᵉ boucle `j*j` (10⁶) | ~77 µs | ⚠ déjà superinstruction NEON native, au plus ~2 instr/élément ; l'éliminer exigerait une formule fermée (interdite) |
| boucle complexe native | 515 µs | déjà **plus rapide** que Rust (559 µs mesuré par sui1) |
| préflight `prepare_jit` | ~8 µs | ✅ optimisé (RIR réutilisé, une seule inférence) |

Même en ramenant le préflight à zéro, l'écart reste ~110–165 µs. Il est
dominé par deux postes situés dans des fichiers que je ne possède pas
(`front.rs`, `main.rs`) et par une boucle déjà native. Je n'ai utilisé ni
formule fermée, ni résultat codé en dur, ni cache disque, ni banc modifié.

Note d'honnêteté sur les chiffres de CLOSE1 : les « 91–132 µs de compiletout »
qu'il cite étaient mesurés avec `ROSE_TIMING` (eprintln inclus). Le coût
**réel** du poste supprimé, isolé par différences appariées, est de 40 à
74 µs. La mission tablait sur un gain de 150–190 µs ; la mesure contredit
cette hypothèse. Je rapporte la mesure, pas l'hypothèse.

---

## 5. Validation (preuves réelles, binaire `rose_final2`)

```
cargo build --release --bin rose        rc=0, static-pie
                                        SHA256 92bc14b5…468fe78468051e
cargo test --release                    rc=0 ; 220+204+0+6+1+0 = 431 passed,
                                        0 failed (6 ignored)
jardins/tests/premiers.rose             ── jardin/premiers : 20 tests, 0 échec ──
fib(25)                                 75025
point fixe (2 voies)                    b3ed9262dd54776a6dbd07bc12368d0e ×2
9 domaines multi, c6, 9 tirages         81/81 OK
9 domaines multi, c16, 9 tirages        81/81 OK
bench_global.rose                       rc=0, 12 lignes, 0 octet stderr,
                                        11 valeurs IDENTIQUES à CLOSE1
suite complexe, c6+c16+c19              md5 ba144f964dc4b8775b176c31205c98fd
englobant, c6, 4096 1                   46297232.426252 / 564 / 157158 /
                                        11899223598484
prog. avec fonctions JIT (fib)          75025, pas de SIGSEGV (rongé puis corrigé)
ROSE_TIMING suite complexe              compiletout 80–112µs → 0,06–0,29µs
                                        lignes finalize : 4 → 0
```

`git status --short` contient aussi, **sans que j'y aie touché**, des modifications
concurrentes d'un autre agent (`compilateur/emetteur_elf.rose`,
`rose/src/i128x.rs`, `rose/src/natif_emetteur.rs`, `rose/docs/SELF_HOST_EXT.md`).
Mes seules modifications sont `rose/src/jit.rs`, `rose/src/interp.rs` et ce
document. Aucun commit, aucune publication.

Déclaration d'incident mineur : le harnais `bench_multi_close1.py` copié pour
la validation écrivait ses CSV/JSON de sortie dans `/tmp/opencode/close1_agents/` ;
deux fichiers de résultats de banc (non source, non doc, hors dépôt) y ont été
écrasés par mes mesures. Aucun fichier source ou document d'un autre agent
n'a été modifié.

---

## 6. Reproduction

```
cd /tmp/opencode/close2_agents
python3 bench_3way.py 6 31 three_c6
python3 bench_3way.py 16 31 three_c16
python3 bench_3way.py 19 31 three_c19
ROSE_TIMING=1 taskset -c 6 ./rose_final2 \
  /home/clausake/Stefan/ROSE/bench/suite/complexe.rose
```
