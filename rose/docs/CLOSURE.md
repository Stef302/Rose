# CLOSURE — fermeture du résidu commun de démarrage (CONSTAT DATÉ, 20/11/2026)

Auteur : sous-agent `close1`. **Aucun git commit, aucune publication, `rose/rose`
intact, aucun `.rose` modifié, aucun banc modifié, aucun document d'un autre
agent modifié.** HEAD annoncé et final : `7277a5c673299307ab7024fa5cae7228b18fc7b5`.
Fichiers possédés modifiés : `rose/src/main.rs`, `rose/Cargo.toml`, ce document.
`rose/src/front.rs` a été instrumenté temporairement puis **restauré à l'octet
près** (`git diff` vide pour ce fichier).

Binaire livrable : `/tmp/opencode/close1_agents/rose_fin`,
SHA256 `aa7ebc83b95f844fc30c0b6b7462205cfb06fb8a89f2ebfc29414353e68c2234`,
static-pie, 4 932 512 octets. Témoin : `rose_base`, même source sans mes deux
modifications, SHA256 `bc8d585f509b25e7f4619b9a5f5a3bf7e5793e8104e9a5570e468512020ab0b0`,
static-pie, 5 587 928 octets. Rust : `suite_rs` (banc historique, dynamique,
non stripé). Tous deux dans `/tmp/opencode/close1_agents/`.

---

## 1. Verdict — porte DURE NON ATTEINTE, écart réduit sans régression

**Rose ≤ Rust sur la suite complexe : ❌ non atteint.** Aucune des quatre
variantes mesurées ne bat Rust sur les trois cœurs. La meilleure (PGO, §6)
reste à **+131 à +146 µs CPU par exécution** (différences appariées, 21
rotations entrelacées par cœur).

Salve définitive, 21 rotations A/A/A entrelacées, `taskset -c`, CPU
`getrusage(RUSAGE_CHILDREN)`, médianes des **différences appariées par
rotation** ( Rose − Rust , qui annulent la dérive temporelle ) :

| Binaire | c6 | c16 | c19 |
|---|---:|---:|---:|
| témoin `base` | +223 µs (×1,194) | +252 µs (×1,196) | +190 µs (×1,180) |
| **livrable `fin`** (main.rs + LTO) | **+184 µs** (×1,163) | **+177 µs** (×1,121) | **+192 µs** (×1,167) |
| `fnpn` (fin + non-PIE + `native`) | +152 µs (×1,129) | +143 µs (×1,112) | +142 µs (×1,121) |
| `pgo` (fnpn + PGO) | **+146 µs** (×1,124) | **+139 µs** (×1,113) | **+131 µs** (×1,119) |

Médianes marginales CPU de la même salve (ms) : c6 base 1,382 / fin 1,333 /
fnpn 1,303 / pgo 1,298 / Rust 1,150 ; c16 1,714 / 1,619 / 1,506 / 1,582 /
1,436 ; c19 1,348 / 1,328 / 1,279 / 1,247 / 1,121.

Gain du livrable sur le témoin (apparié) : **−57 µs c6, −80 µs c16, −22 µs
c19**. Gain de la recette accélérée complète : **−77 / −113 / −59 µs**.
L'écart initial de la mission (0,21 / 0,16 ms) est donc réduit d'environ
**un tiers à la moitié**, pas annulé.

⚠ Bruit : deux processus Wine à ~100 % CPU (`wineboot.exe`, `start.exe`,
10 h d' elapsed) et `load average` 2,85–4,69 pendant toute la session. Les
médianes marginales de Rust ont varié de 1,05 à 1,48 ms d'une salve à l'autre ;
c'est précisément pour cela que le verdict s'appuie sur les différences
appariées et sur ≥9, ici 21, rotations. Sur une salve antérieure (12 rotations,
c19) Rust médian était 1,482 ms contre 1,359 ms pour Rose, soit un ratio
apparent 0,92 < 1 : **je ne le revendique pas comme une victoire**, c'est un
artefact de dérive, corrigé par l'appariement.

---

## 2. Décomposition du démarrage (instrumentation permanente `ROSE_TIMING`)

Médianes sur 3 à 5 tirages, `taskset -c 6`, binaire témoin, suite complexe :

| Phase | Mesure | Nature |
|---|---:|---|
| `entree_main` | 16–48 ns | négligeable |
| `console` (`preparer`, no-op Linux) | 48–96 ns | négligeable |
| `pret` (args, hook de panic, lecture source, 3 clones) | 8–15 µs | **dans mon fichier** |
| `post_spawn` (création du fil 512 Mo, temps mur) | 42–56 µs | **dans mon fichier** (≈20 µs CPU, §5) |
| `parse` | 46–70 µs | `front.rs` |
| `transform` | 7–11 µs | interdit (`rir`/`interp`) |
| `jitnew` | 21–33 µs | interdit (`jit.rs`) |
| `compiletout` | **91–132 µs**, alors que `eligibles=[]` | interdit (`jit.rs`/`interp.rs`) |
| `exec` | **603–611 µs** | interdit (`jit.rs`) |
| total interne | 769–867 µs | — |

Le plancher processus, sans Rose, mesuré en salve entrelacée : `/bin/true`
0,486–0,684 ms, binaire Rust trivial 0,557–0,930 ms, `rose version`
0,454–0,901 ms CPU selon le cœur. Le démarrage spécifique à Rose est donc de
l'ordre de quelques dizaines de µs seulement ; l'essentiel est le plancher
commun `taskset` + chargeur, identique pour Rust.

Décomposition du parse, obtenue avec une instrumentation **temporaire** ajoutée
à `front.rs` puis retirée (5 tirages, c6) : `canonicalize` 3,1–4,0 µs ;
`analyser_sans_verifier` 24–30 µs (dont lexeur 7–10 µs, parser 8–10 µs, repli
de constantes + réparation 4,5–5,4 µs, détection compteur `i` ~0,2 µs) ;
`verifier` 12–14 µs.

### Goulot #1

Deux goulots distincts, à ne pas confondre :

1. **Goulot de calcul interne : `exec` = 603–611 µs, soit ~78 % du total
   interne.** Ce n'est pas une boucle lente : la boucle native Rose `calc`
   mesure 515 µs contre 559 µs pour la boucle Rust (mesures sui1). La seconde
   boucle du programme (somme des carrés, 10⁶ itérations) ajoute ~80 µs et
   n'existe pas dans le jumeau Rust, qui récupère `total_im` de sa première
   boucle. Ce poste est dans `jit.rs`, fichier interdit.
2. **Goulot de surcoût fixe supprimable : `compiletout` = 91–132 µs pour un
   module Cranelift vide** (`eligibles=[]` sur ce banc depuis sui1), auquel
   s'ajoutent 21–33 µs de `Jit::nouveau` et ~50–70 µs de parse. C'est l'ensemble
   de ces postes, situés dans `jit.rs`/`interp.rs`/`front.rs`, qui explique
   l'écart avec Rust. Mes deux fichiers ne peuvent agir que sur une fraction
   (§3 et §6).

---

## 3. Changement fait dans les fichiers possédés

### `rose/src/main.rs` — une seule lecture du source

`main.rs` lisait le fichier (l.1244), puis `interp::executer_fichier` le relisait
(`interp.rs:6297`). Le clone `source_t` n'était jamais utilisé sur le chemin
normal. Nouveau chemin :

* `sans_imports = !source.contains("importer")`. `importer` est un **mot réservé
  exact** du lexeur (`front.rs:91`, `front.rs:829`) : un faux positif (mot dans
  un commentaire) conserve l'ancien chemin, un faux négatif est impossible.
* Sans imports, `interp::executer_source(source, args)` reçoit la chaîne déjà
  lue. `origine` ne sert, dans `executer_source_analyse`, qu'à
  `front::analyser_fichier_source`, c'est-à-dire uniquement à résoudre les
  imports ; sans imports, `analyser_fichier_source` et `analyser` exécutent le
  même `analyser_sans_verifier` + `verifier` (vérifié dans le code). La pile
  512 Mo et le `join` sont inchangés.
* Avec imports (jardins), l'ancien chemin `executer_fichier` est conservé tel
  quel. Repli si la création du fil échoue : relecture du fichier et
  reconstruction des args (chemin rare, sémantique identique).

Gain isolé mesuré : dans le bruit (c6 +6 µs, c16 +12 µs, c19 −44 µs sur 15
rotations) — la seconde lecture est en cache et coûte ~2–5 µs. Conservé parce
qu'il supprime du travail redondant **sans aucun changement de sortie**, pas
parce qu'il explique à lui seul un gain mesuré.

### `rose/Cargo.toml` — LTO + une unité de codegen

`[profile.release] lto = "fat"`, `codegen-units = 1`. Réglages portables,
aucune option de CPU. Gain apparié : −57 / −80 / −22 µs (c6/c16/c19). Taille
5 587 928 → 4 932 512 octets.

---

## 4. Recette accélérée (hors livrable, documentée) et portabilité

Ces options ne sont **pas** dans `Cargo.toml` : elles ne sont pas dans les
fichiers que je possède (`.cargo/config.toml`) et/ou posent un problème de
portabilité. Elles sont mesurées et rejouables ; le repli générique reste
`cargo build --release` tel quel (binaire `rose_fin`).

```
cd /home/clausake/Stefan/ROSE/rose
export PATH="$HOME/.cargo/bin:$PATH"
# A. non-PIE seul (portable aarch64 Linux, gains -45/-110/-43 us appariés)
RUSTFLAGS="-C target-feature=+crt-static -C relocation-model=static -C link-arg=-no-pie" \
  CARGO_TARGET_DIR=/tmp/opencode/close1_agents/target_fnp cargo build --release --bin rose
# B. + target-cpu=native (gain supplémentaire faible/inconstant, voir ci-dessous)
RUSTFLAGS="-C target-feature=+crt-static -C relocation-model=static -C link-arg=-no-pie -C target-cpu=native" \
  CARGO_TARGET_DIR=/tmp/opencode/close1_agents/target_fnpn cargo build --release --bin rose
```

`rustc --print target-cpus` résout `native` en **cortex-x925** sur cette machine.
Impact exact mesuré : en static-pie, `native` seul 1 692 µs contre 1 702 µs
médianes marginales sur c6 (≈1 %) ; combiné au non-PIE et au LTO, `fnpn` gagne
45 / 110 / 43 µs par rapport à `fin` (différences appariées, 21 rotations).
Le binaire `fnpn` s'exécute correctement sur c11 (Cortex-A725) : sortie
`0.9999999999860738` / `333333833333500000`. **Cela ne garantit pas** un
fonctionnement sur un aarch64 plus ancien : `-C target-cpu=native` autorise
l'émission d'instructions propres au x925. C'est pourquoi la porte générique
reste le livrable ; le `native` n'est qu'une option locale documentée.

PGO (gain supplémentaire −21 / −15 / −10 µs seulement) :

```
# instrumenté, mêmes options que B + -C profile-generate=.../pgo
# entraînement : 3× complexe.rose + bench_global.rose + jardins/tests/premiers.rose
PD=$HOME/.rustup/toolchains/stable-aarch64-unknown-linux-gnu/lib/rustlib/aarch64-unknown-linux-gnu/bin/llvm-profdata
$PD merge -o .../pgo/merged.profdata .../pgo/*.profraw
# rebuild avec -C profile-use=.../pgo/merged.profdata
```

⚠ Le `llvm-profdata` de `~/.local/bin` (LLVM 18, format 9) **ne sait pas lire**
les `.profraw` de rustc 1.99 (format 11) : utiliser celui de rustup. Le profil
doit être régénéré après tout changement de source ou de drapeaux.

---

## 5. Tentatives mesurées puis REJETÉES

| Tentative | Mesure | Décision |
|---|---|---|
| `ROSE_SANS_JIT=1` | Sortie md5 identique mais `exec` passe de 611 µs à **368 ms** : les superinstructions natives vivent dans `jit.rs` et sont désactivées | rejeté |
| `ROSE_JIT_OPT=none` | `compiletout` 88 → 60 µs sur la suite, mais glb3 a mesuré **D3 +26 %** (primes 50k) sur c6 et c16 | rejeté (régression ailleurs) |
| Pile du fil réduite 512 → 8 Mo | Micro-mesure CPU/spawn : 20,1 µs (512 Mo) contre 14,2 µs (8 Mo), soit **~6 µs** seulement ; le surcoût de 512 Mo est réservé (mmap paresseux) | rejeté (risque pour la récursion profonde, gain négligeable) |
| Exécution sur le fil principal | Supprimerait ~20 µs CPU de spawn, mais casserait la correction 512 Mo (`it(10000)` débordait à 8 Mo) | rejeté |
| `lto=fat` + `codegen-units=1` seuls | −57 / −80 / −22 µs : conservé (§3) | retenu |
| `target-cpu=native` seul | ≈1 % sur c6, rien de stable ailleurs | non intégré (portabilité) |
| PGO seul | −10 à −21 µs au-delà de `fnpn`, recette lourde et profil périssable | documenté, non intégré |

---

## 6. Reste chiffré — pourquoi la porte reste fermée

Addition des postes mesurés sur c6, en gardant les bornes observées :

| Poste | Rose | Rust |
|---|---:|---:|
| plancher `taskset` + chargeur (étalon `/bin/true`, Rust trivial) | 0,49–0,68 ms | 0,49–0,68 ms |
| démarrage spécifique Rose (`main`, lecture, fil) | ~0,02–0,06 ms | ~0,02 ms |
| parse + transform + `Jit::nouveau` | ~0,08–0,11 ms | 0 |
| `compiletout` module vide | ~0,09–0,13 ms | 0 |
| boucle(s) + sortie | ~0,61 ms (dont ~0,515 `calc` + ~0,080 seconde boucle) | ~0,56 ms |
| **total CPU attendu** | **~1,30–1,45 ms** | **~1,10–1,30 ms** |

Le mur des ~130–190 µs restants est donc **presque entièrement du surcoût fixe
de pré-exécution** (module Cranelift vide + `Jit::nouveau` + parse), situé dans
`jit.rs` et `interp.rs`, fichiers interdits. Le supprimer exigerait l'une de ces
décisions, toutes hors de mon périmètre ou interdites par la mission :

1. empêcher `declarer_delegues()` (`jit.rs`) de remplir `delegues`, ce qui
   désactive le raccourci « module vide » de sui1 et déclenche deux
   `finalize_definitions` inutiles (~90 µs) ; l'appel vient de `interp.rs:6867` ;
2. rendre `Jit::nouveau` (~25 µs) inutile sans toucher à `jit.rs` ;
3. supprimer le parse (~50–70 µs) sans pré-calcul fermé ni résultat codé en dur ;
4. accélérer la boucle native de 20–30 % (`jit.rs`), alors qu'elle est déjà
   **plus rapide que Rust** (515 vs 559 µs).

Aucune formule fermée, aucun résultat codé en dur, aucun cache disque, aucun
bench modifié n'a été utilisé. Le livrable réduit l'écart de ~57 à 80 µs et la
recette accélérée de ~131 à 146 µs, sans régression fonctionnelle ; la porte
Rose ≤ Rust reste **fermée sur un constat chiffré**, pas sur une victoire
revendiquée.

---

## 7. Validation (preuves réelles, binaire livrable `rose_fin`)

```
cargo build --release --bin rose                 rc=0, static-pie,
                                                 SHA256 aa7ebc83…6fb8a89f2ebfc29414353e68c2234
cargo test --release                             rc=0 ; 220+204+0+6+1+0 = 431 passed, 0 failed
                                                 (6 ignored)
jardins/tests/premiers.rose                      ── jardin/premiers : 20 tests, 0 échec ──
fib(25)                                          75025
point fixe (2 voies)                             b3ed9262dd54776a6dbd07bc12368d0e ×2
9 domaines multi, c6, 9 tirages                  81/81 OK (9 domaines × 9 reps)
9 domaines multi, c16, 9 tirages                 81/81 OK
bench_global.rose                                rc=0, 12 lignes, 0 octet stderr,
                                                 11 valeurs identiques à EXPECTED
suite complexe, c6 + c16 + c19                   md5 ba144f964dc4b8775b176c31205c98fd
                                                 (0.9999999999860738 / 333333833333500000)
englobant, c6 + c19, 9 tirages                   4 valeurs identiques à chaque tirage :
                                                 46297232.426252 / 564 / 157158 / 11899223598484
                                                 (CPU méd fin 1,348 c6 / 1,594 c19,
                                                  Rust 0,928 / 1,149 → ×1,45 / ×1,39)
```

`git status --short` à la fin : `M rose/Cargo.toml`, `M rose/src/main.rs`
uniquement. Aucun commit, aucune publication, aucun remplacement de `rose/rose`.

## 8. Reproduction

```
cd /tmp/opencode/close1_agents
python3 bench_close1.py 6 21 definitive_c6 \
  base=./rose_base fin=./rose_fin fnpn=./rose_fnpn pgo=./rose_pgo rust=./suite_rs
python3 bench_close1.py 16 21 definitive_c16 ...   # idem
python3 bench_close1.py 19 21 definitive_c19 ...   # idem
# différences appariées : CSV definitive_c*.csv, colonnes rep/impl/cpu_ms
```

Tous harnais, CSV, journaux et binaires sont dans `/tmp/opencode/close1_agents/`.

---

## Rectification de titre (21/11, orchestrateur)

Le titre du commit `8c31a38` dit « écart suite ×1,19→×0,9 sur une partie ». **Il est imprécis** :
aucune salve n'a montré Rose ≤ Rust. Les ratios finaux de la salve définitive sont
**×1,40 c6 / ×1,39 c16 / ×1,39 c19** pour la recette PGO (tableau § titre du rapport),
soit un écart **réduit mais toujours positif**. La branche `main` étant protégée, le titre
n'a pas pu être amendé ; ce paragraphe fait foi. Aucune victoire n'est revendiquée ici.
