# RC4 — démarrage du banc global : diagnostic, goulot #1, blocage (CONSTAT DATÉ)

Sous-agent `rc4`, fenêtre **20/11/2026 22:56 → 23:30 UTC locale** (la consigne annonce le 21/11).
**Aucun commit, aucune publication, aucun binaire copié dans le dépôt, aucune
autre modification que ce document.** `rose/src/main.rs` est resté
**octet pour octet identique** à l'état de départ (SHA-256
`6e686fb87c38cdb574c9ffb476bf7a30cd8f7a62e10d4f1230885abe890a0383`) : les
trois optimisations candidates ont été construites et mesurées **hors arbre**
puis abandonnées, leurs gains étant sous le bruit. Le changement RUSTX
(exécution directe sans thread sur graphe acyclique, `catch_unwind`
préservé, thread 512 Mio conservé sur cycle) est **intact**. Le garde
`a_des_fonctions` (régression D5/D6/D7 de `f315407`) n'a **pas** été
réintroduit ; aucun fichier `interp.rs`/`jit.rs` n'a été touché.

> **Verdict : blocage prouvé dans le périmètre.** Le « ~1 ms mur hors chrono »
> est dominé par la création de processus + l'initialisation CRT/Rust
> **avant** `main` (~0,25–0,35 ms de plancher mur, 25–90 µs mesurées en
> interne) : ce n'est pas du code `main.rs`. Le seul poste encore pilotable
> par `main.rs` sur le global est la création du fil 512 Mio (~50–80 µs),
> requise par la récursion `fib` et donc conservée. Trois changements réels
> de `main.rs` ont été mesurés en A/A/A c6 **et** c16 : aucun ne bat les deux
> témoins. Aucun gain n'est revendiqué.

Tout chiffre ci-dessous a été **re-mesuré** sur le binaire isolé ; aucune
valeur n'est citée d'un README, d'une spec ou d'un doc antérieur sans
re-mesure.

---

## 1. Protocole réellement exécuté

| Élément | Mesure |
|---|---|
| Arbre figé | `/tmp/opencode/rc4_agents/src_base` (copie de `rose/` à 22:56 : `Cargo.toml`, `Cargo.lock`, `build.rs`, `src/`, **et `.cargo/config.toml`**) |
| Binaire témoin A | `/tmp/opencode/rc4_agents/rose_base`, SHA-256 `c1276d9ba0b2a2a7e7fce4a4be1e2e55f917df2d24d444d2f3e48de7b1b435d3`, **static-pie** (`file` vérifié) |
| ⚠ Piège de build | une première copie sans `.cargo/config.toml` produisait un binaire **dynamique** (SHA `9e79f0e0…`) : +0,25–0,3 ms de démarrage. Toutes les mesures retenues sont sur le binaire static-pie ; le résultat dynamic a été écarté, pas moyenné. |
| Candidats | `rose_cand1` (`_exit`), `rose_cand` (zéro clone + console), `rose_cand3` (= `rose_cand` + lecture `fs::read`), chacun dans un `src_*` et un `CARGO_TARGET_DIR` dédiés |
| 9 programmes | `multi_{io,fib,boucle,premiers,fact,pgcd,chaine,liste,table}.rose` (les mêmes 9 domaines que le contrôle 81/81), copiés dans `/tmp/opencode/rc4_agents/bench/` ; plus `bench_global.rose` pour le hors-chrono |
| Pinage | `taskset -c 6` **et** `-c 16` pour `/usr/bin/time` et `ROSE_TIMING` ; pour l'A/A/A, `sched_setaffinity` du **processus père** (les fils héritent) — supprime un `taskset` supplémentaire par tirage |
| Échantillons | A/A/A entrelacé `A/B/A2` puis `B/A2/A` (A2 = copie byte-identique de A), **15** tirages par binaire/cœur pour le candidat 2, **21** pour le candidat 3, **9** pour le candidat 1 (salve bruitée, refaite ensuite) ; médianes des **différences appariées** `B − (A+A2)/2` **et** dérive `A2 − A` |
| Horloges | H1 mur `perf_counter_ns` autour de `posix_spawn`+`wait4` ; H2 CPU `getrusage` du fils (`ru_utime+ru_stime`) ; H3 `/usr/bin/time` (30 tirs groupés, résolution 0,01 s) ; H4 `ROSE_TIMING` (infrastructure permanente) ; H5 sonde temporaire `.init_array` + `atexit` (`clock_gettime(CLOCK_MONOTONIC)`), **jamais livrée** |
| Bruit | A/A (A2−A) médian −79 à +30 µs sur la salve finale : **tout écart A/B plus petit que cela n'est pas un gain** |
| Chargement concurrent | 4–9 autres agents compilaient (`rustc` à 100 %) et lançaient des `rose` pendant les salves (load 7–10). C'est précisément pour cela que la décision repose sur les **différences appariées** et la dérive A/A, pas sur les médianes marginales |

---

## 2. `/usr/bin/time` + `ROSE_TIMING` sur les 9 programmes (binaire témoin)

`total` = temps CPU+mur pour **30 exécutions** consécutives pinées c6, division
mentale ≈ 1/30. `pret` et `post_spawn` sont les compteurs `main.rs` ;
`parse`/`transform`/`jitnew`/`exec` viennent de `interp.rs`.

| Programme | `/usr/bin/time` 30 tirs | ~mur/tir | `ROSE_TIMING` (µs) |
|---|---:|---:|---|
| `multi_io` | 0,01 s | ~0,33 ms | parse 11,3 · pret 8,8 · post_spawn 53,2 · total 27,6 |
| `multi_fib` | 0,01 s | ~0,33 ms | parse 19,9 · pret 9,4 · post_spawn 36,9 · total 42,7 |
| `multi_boucle` | 0,02 s | ~0,67 ms | parse 22,2 · jitnew 20,9 · compiletout 268,7 · exec 157 · total 477 |
| `multi_premiers` | 0,03 s | ~1,0 ms | parse 17,8 · exec 837 · total 870 |
| `multi_fact` | 0,01 s | ~0,33 ms | parse 12,3 · pret 9,4 · post_spawn 89,2 · total 57,0 |
| `multi_pgcd` | 0,01 s | ~0,33 ms | parse 11,4 · pret 8,8 · post_spawn 61,8 · total 31,9 |
| `multi_chaine` | 0,02 s | ~0,67 ms | parse 24,2 · exec 282 · total 322 |
| `multi_liste` | 0,02 s | ~0,67 ms | parse 236,5 · transform 11,6 · exec 78,6 · total 371 |
| `multi_table` | 0,02 s | ~0,67 ms | parse 25,7 · exec 345 · total 386 |
| **`bench_global`** | **0,10 s** | **~3,33 ms** | **parse 75,3 · transform 15,4 · jitnew 36,5 · exec 2 524,8 · total 2 654,2** · entree_main 16 ns · console 48 ns · **pret 12,9 µs · post_spawn 49,5 µs** |

Le chrono interne du banc global rend `3` (ms entiers) ; le mur mesuré est
~3,33 ms : **le hors-chrono est de l'ordre de 0,3 ms dans ce harnais**, pas
1 ms — le chiffre « ~1 ms » des docs antérieures venait d'un harnais
`taskset`+`subprocess.run` qui ajoute ~0,5–0,8 ms de création de processus
(mesuré ici : `/bin/true` = 220–310 µs CPU, `multi_io` = 250 µs CPU sur le
même harnais).

---

## 3. Goulot #1 du démarrage — décomposition horodatée (sonde H5)

Sonde temporaire ajoutée uniquement dans `src_probe3` : un constructeur
`.init_array` (`clock_gettime`) et un handler `atexit` qui imprime **une
seule fois** les écarts. Médianes de 9 tirs c6, hors valeurs de premier
tir (fautes de page à froid).

| Phase (global) | Mesure | Fichier |
|---|---:|---|
| `execve` → constructeur (`init_array`) | fusionné dans le plancher mur | noyau / chargeur, **hors périmètre** |
| constructeur → entrée dans `main` | **25–90 µs** | CRT + `std::rt::init` (args/env, garde de débordement de pile, panic runtime), **hors `main.rs`** |
| `main` → hook de panic / `argv` | < 0,2 µs chacun | `main.rs` (déjà négligeable) |
| `argv` → fin lecture source (`pret`) | 7–21 µs | `lire()` + deux `env::var` de debug |
| `pret` → décision `exec_direct` | 1–15 µs | `programme_a_cycle_appel` + `ROSE_FORCE_THREAD` |
| décision → `post_spawn` | **50–82 µs** | `pthread_create` + `mmap` 512 Mio (conservé : `fib` a un cycle) |
| `post_spawn` → sortie | 2,5–3,3 ms | exécution (`fib`, JIT), **dans le chrono du banc** |
| fin `main` → `atexit` | 2–10 µs | `libc::exit` + handlers |

**Goulot #1 du poste hors-chrono = création de processus + initialisation
CRT/Rust avant `main`** (~0,25–0,35 ms mur, dont 25–90 µs internes) : il
est **antérieur à la première instruction de `main.rs`**, donc hors de mon
périmètre. Le goulot #1 *encore pilotable* sur le global est le **fil 512 Mio
(50–80 µs)** ; la suppression du fil sur graphe acyclique a déjà été faite
par RUSTX et couvre 8 des 9 programmes. Le retirer aussi pour `fib`
exigerait de prouver statiquement une borne de profondeur de récursion ;
aucune telle preuve n'existe dans `main.rs`, et le garde `a_des_fonctions`
est précisément le type de raccourci qui a déjà cassé D5/D6/D7.

---

## 4. Optimisations candidates — toutes mesurées, toutes abandonnées

### 4.1 `libc::_exit` après vidage explicite des flux

`std::process::exit` (≈ `libc::exit` + handlers atexit) remplacé par
`stdout().flush()` + `stderr().flush()` + `libc::_exit(rc)`.

- H5 : fin de `main` → `atexit` = **2–10 µs** : le gain théorique est déjà
  sous le bruit.
- A/A/A 9 tirs c6/c16 (salve bruitée, `taskset`, 9 programmes) : différences
  appariées **−121 à +782 µs**, dérive A/A jusqu'à ±492 µs — aucun signal.

**Décision : abandonné** (le gain attendu est nul et `_exit` change la
sémantique des handlers de sortie).

### 4.2 Zéro clone sur le chemin direct + appel console no-op supprimé sous Unix

`Tache` construite **dans** la branche retenue, valeurs **déplacées** au lieu
de `source.clone()`/`args.clone()` dans le `catch_unwind` ; appel à
`console_win::attendre_fermeture()` omis sous Unix (`console_ephemere()` y
rend toujours faux après deux lectures d'environnement inutiles). RUSTX
intact (même `programme_a_cycle_appel`, même `ROSE_FORCE_THREAD`, même
thread 512 Mio).

A/A/A **15 tirs, c6 ET c16**, 8 programmes + global (différence appariée,
dérive A/A entre parenthèses, µs) :

| Programme | c6 diff (dérive) | c16 diff (dérive) |
|---|---:|---:|
| io | +1,2 (−10,8) | −7,7 (−13,8) |
| fib | +30,3 (−11,2) | −10,8 (−14,0) |
| boucle | +4,5 (−6,7) | −10,0 (+20,9) |
| premiers | +43,2 (−5,4) | +14,7 (−18,8) |
| chaine | −14,5 (−12,6) | +19,9 (−0,7) |
| liste | +3,0 (−8,4) | −10,0 (−38,4) |
| table | −5,6 (−9,9) | +9,5 (−24,9) |
| global | −2,8 (−17,9) | +17,4 (−2,7) |

**Décision : abandonné.** Aucun écart ne dépasse la dérive A/A sur les deux
cœurs ; la copie de 1–7 Ko de source coûte moins que le bruit de mesure.

### 4.3 Lecture source en un tampon (`fs::read` + `String::from_utf8`)

`read_to_string` remplacé par `std::fs::read` puis `String::from_utf8`
(même sémantique d'échec sur fichier illisible ou non UTF-8 ; suppression
du `lseek` et du second `read`), cumulé avec 4.2.

A/A/A **21 tirs, c6 ET c16**, 9 programmes + global, différence appariée
médiane (dérive A/A) :

| Programme | c6 diff (dérive) | c16 diff (dérive) |
|---|---:|---:|
| io | +12,5 (−17,3) | +16,8 (−50,1) |
| fib | +22,0 (−14,3) | −0,7 (−20,8) |
| boucle | +6,9 (−7,4) | −30,4 (−28,7) |
| premiers | +3,8 (−21,8) | +17,3 (−56,8) |
| fact | +2,3 (−9,8) | +14,1 (−26,9) |
| pgcd | +2,9 (−10,2) | +6,5 (−13,3) |
| chaine | −34,5 (−3,9) | +23,8 (−34,2) |
| liste | +47,8 (+29,8) | +10,5 (−4,6) |
| table | +1,6 (−20,5) | +34,8 (−9,6) |
| global | +16,0 (−55,2) | **+55,0 (−79,2)** |

Le candidat est même **plus lent** sur le global aux deux cœurs (+16 / +55 µs,
dérive −55 / −79 µs) : `String::from_utf8` après `fs::read` n'améliore pas
`read_to_string`, déjà dimensionné depuis `metadata()`.

**Décision : abandonné**, code restauré.

---

## 5. Validation de l'état livré (binaire témoin, source inchangée)

| Contrôle | Résultat |
|---|---|
| `cargo build --release --bin rose` (arbre figé, static-pie) | OK |
| `cargo test --release` (arbre figé) | **265 + 249 + 6 = 520 réussis, 0 échec**, 6 ignorés, rc=0. La consigne cite 517 (chiffre périmé, cf. `EXTENSION.md`) ; l'arbre figé en compte 520 — **aucun échec** dans tous les cas |
| 9 domaines × 9 tirages × c6 **et** c16 | **81/81 + 81/81 = 162/162** sorties identiques au témoin |
| Point fixe `rir compilateur/emetteur.rose` | 263 520 octets, md5 `b3ed9262dd54776a6dbd07bc12368d0e` **deux fois de suite** |
| Valeurs du global | 12 lignes, identiques aux 11 premières lignes de `full.txt` (troncatures du jumeau Rust documentées dans `RUSTX.md`, non adoptées) |
| RUSTX préservé | `programme_a_cycle_appel`, `catch_unwind`, `ROSE_FORCE_THREAD`, fil 512 Mio sur cycle : présents et inchangés (`sha256 main.rs` = celui de départ) |
| Garde `a_des_fonctions` | **absent** (`git log -S` : uniquement le commit de réparation `f315407`, aucun réajout ici) |

---

## 6. État final

| Question | Réponse |
|---|---|
| Goulot #1 du démarrage identifié ? | **Oui** : plancher création de processus + CRT/Rust pre-`main` (~0,25–0,35 ms mur ; 25–90 µs internes), **hors `main.rs`**. Pilotable restant : fil 512 Mio (50–80 µs, conservé pour la récursion). |
| Optimisation `main.rs` avec gain A/A/A c6+c16 ? | **Non** : trois candidats réels mesurés, tous dans la bande de dérive A/A ; aucun ne bat les deux témoins. |
| `main.rs` modifié ? | **Non** — byte-identique au point de départ. |
| RUSTX préservé ? | **Oui**, intégralement. |
| Régression D5/D6/D7 ? | **Aucune** : aucun garde sur les fonctions ajouté ; 162/162 sorties identiques. |
| Commit / publication ? | **Aucun** ; seul `rose/docs/RC4.md` est ajouté à l'arbre. |

**Piste restante (hors périmètre RC4)** : réduire le plancher de création de
processus (chargeur / `std::rt::init`) ou la compilation Cranelift
(`jit.rs`) — pas une décision de `main.rs`. Toute tentative de supprimer le
fil sur un programme récursif doit d'abord produire une preuve de borne de
profondeur, sinon elle casse `it(10000)` et les jardins.
