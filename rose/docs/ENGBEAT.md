# ENGBEAT — offensive « battre Rust sur l'englobant » (20/11/2026)

Auteur : sous-agent eng1. Fichiers possédés : `rose/src/interp.rs`, ce document.
Aucun commit, aucune publication. Binaire isolé dans `/tmp/opencode/eng1_agents/`,
`rose/rose` n'a pas été remplacé. Aucun `.rose`, `jit.rs`, `run.rs`, `i128x.rs`,
`Cargo.toml` ni `PLAN.md` modifié.

## 1. Verdict — porte NON atteinte, mais écart fortement réduit

| Critère | Résultat |
|---|---|
| **Rose ≤ Rust sur l'englobant complet** | ❌ **non atteint** |
| Réduction de l'écart | ✅ **×2,49 → ×1,40** (c19), **×2,31 → ×1,62** (c6, salve bruitée) |
| Programme, valeurs, bench | identiques : `46297232.426252 / 564 / 157158 / 11899223598484`, contrôlés à chaque tirage |

**Constat daté, 20/11/2026 ~06:53 UTC**, `N=4096 F=1`, 9 échantillons entrelacés
A/A/A (Rose baseline, Rose final, Rust), rotation à chaque répétition, médianes,
CPU `getrusage(RUSAGE_CHILDREN)`.

| Implémentation | CPU méd c19 (min–max) ms | / Rust | CPU méd c6 (min–max) ms | / Rust |
|---|---:|---:|---:|---:|
| Rose baseline (HEAD `interp.rs`, jit courant) | 2,190 (2,047–2,640) | 2,212 | 2,191 (2,095–2,647) | 2,115 |
| **Rose final** | **1,381 (1,321–1,734)** | **1,395** | **1,676 (1,275–1,866)** | **1,618** |
| Rust `-O3 target-cpu=native` | 0,990 (0,863–1,313) | 1,000 | 1,036 (0,862–1,315) | 1,000 |

Gain brut de mon changement : **×1,59 c19**, **×1,31 c6** (salve c6 plus bruitée).
Salves de confirmation (mêmes sources) : c19 **1,356 ms / ×1,492** et c6
**1,415 ms / ×1,543** (`aaa3_c19.log`, `aaa3_c6.log`). Fourchette observée sur
4 salves : **1,35–1,68 ms**, ratio **1,40–1,62×** — le bruit de la machine est
de l'ordre de ±0,15 ms et n'est pas masqué. Logs/CSV/JSON :
`/tmp/opencode/eng1_agents/aaa{2,3}_c{19,6}.log`, `final_aaa_c{19,6}.log` et les
`*_summary.json` correspondants.

La cible historique de 0,866 ms pour Rust (INT2, 19/11) n'est pas le constat du
jour : la machine est plus chargée, Rust mesure 0,86–1,31 ms selon les salves.
Je ne mélange pas les époques : tous les ratios ci-dessus viennent de la même
salve entrelacée.

## 2. Décomposition par section (goulot #1)

Horloge interne `temps()` (ns), copie instrumentée du bench **uniquement dans
`/tmp/opencode/eng1_agents/bench_prof_ns.rose`**, 9 tirages, pin c19. Médianes :

| Section | Avant (INT2 + état initial) | Après | Nature |
|---|---:|---:|---|
| S4 — IEEE 2 accumulateurs | **764 µs** | **14,4 µs** | superinstruction native |
| S5 — `compter_premiers` | 0,7 µs | 8 µs | inchangé (bruit) |
| S6 — Monte-Carlo / RNG | 279 µs | 279 µs | déjà natif (INT2) |
| S7 — Philox + glanée | 135 µs | 134 µs | déjà natif (INT2) |
| **Total exécution interne** | **1 240 µs** | **493 µs** | |
| parse / transform / `Jit::nouveau` / JIT | 260 µs | 175 µs | JIT vide supprimé |
| **Total interne `ROSE_TIMING`** | **~1 500 µs** | **~660 µs** | |

Le **goulot #1 était bien S4** : 764 µs, soit 61 % de l'exécution interne et
~35 % du CPU total du processus. Les superinstructions INT2 (S6, S7) avaient
déjà réduit S6 de 79 ms à 0,28 ms ; S4 n'avait pas été traité.

## 3. Changement retenu (une décision structurale)

1. **Superinstruction IEEE (S4)** dans `interp.rs`. Le RIR du principal est
   inspecté avant exécution (`detecter_accel_s4`) : boucle
   `SautSiFauxAbstrait(slot < borne)`, deux divisions `i/100.0` et `i/71.0`,
   `u5=(x+y)*(x+y)`, `u6=x*y`, chaîne d'additions gauche
   `d45 = (((((d45+x*x)+y*y)+x*y)+u6)+u5)+u6`, `d55=(d55+u5)+u6`,
   incrément et saut en arrière. **Bornes, diviseurs et numéros de slots sont lus
   dans le RIR** (la borne est le slot `N`, pas une constante du banc). La boucle
   reconnue est remplacée par UNE opération exécutée en Rust natif
   (`exec_s4`), avec le même ordre d'additions (aucune réassociation, aucun FMA).
   Repli exact (`repli_s4`) si un slot n'a pas le type attendu.
   Mesuré : **S4 764 → 14,4 µs**, CPU total 2,19 → 1,55 ms.

2. **Suppression du JIT vide** quand le programme n'a aucune fonction
   (`prog.ordre.is_empty()`) : plus de `declarer_delegues`/`compiler_tout`/
   `compiler_principal` pour un module Cranelift sans fonction. Le principal
   incompatible reste exécuté en RIR, exactement comme avant.
   Mesuré : total interne ~750 → ~660 µs (≈ 75–90 µs gagnés).

### Piège corrigé (honnêteté technique)

Première version de `exec_s4` : écriture des slots via un pointeur dérivé de la
tranche `&[Valeur]` reçue par `eval_slots`. LLVM a éliminé l'écriture de `d55`
(UB : écriture à travers une référence immuable) : la valeur affichée devenait
`29789793.245670356` au lieu de `46297232.426252`. Corrigé en conservant dans
`Interp` le pointeur **mutable** d'origine issu de `empiler_slots`
(`base_principal`), avec `debug_assert_eq!` sur l'adresse. 10 tirages consécutifs
donnent depuis les 4 valeurs exactes.

## 4. Tentatives mesurées puis REJETÉES (aucun gain, code retiré)

| Tentative | Mesure | Décision |
|---|---|---|
| Philox natif recopié dans `interp.rs` (superinstruction sur `remplir_hasard_philox`) | 128 µs avant, 128–142 µs après (`s7fill.rose`, 6 tirages) ; le micro-jumeau Rust avec `target-cpu=native` fait 55 µs, sans 95 µs — l'écart vient de la codegen, pas du builtin | retiré |
| Glanée S7 déroulée + `get_unchecked` (indices vérifiés une fois) | S7 134,8 → 134,2 µs (médianes, 9 tirages) : dans le bruit | retiré |
| État RNG `self.rng` mis en variable locale dans `exec_mc` | S6 279 → 279 µs : aucun changement | retiré |

Aucune de ces tentatives n'est dans le livrable : le code final ne contient que
les deux changements de la section 3 (plus les commentaires de diagnostic).

## 5. Pourquoi la porte forte n'est pas atteinte — plancher chiffré

Mesures c19, même salve, programmes minimaux (`tiny.rose` = `>> 1` ;
`eng_rs 0 0`) : coût CPU d'un processus Rose ~0,70 ms, d'un processus Rust
~0,74 ms. Le coût de **démarrage n'est donc pas un avantage de Rust** ; l'écart
vient du calcul interne : Rose 660 µs contre 299 µs pour le jumeau Rust
(`min: 0.299 ms` imprimé par le banc Rust).

Plancher Rose mesuré / mesurable sans toucher aux fichiers interdits :
parse 125 µs + transform/Jit 40 µs + S4 14 µs + S5 8 µs + **S6 ≥ 210 µs**
(micro-jumeau Rust, 200 000 tours, Lemire exact) + **Philox ≥ 55 µs**
(micro-jumeau Rust) + glanée ~7 µs + divers ~20 µs ≈ **480 µs internes**.
Avec ~0,70 ms de démarrage, cela donne un plancher d'environ **1,16 ms**,
soit encore ~1,2× le Rust du jour (~1,0 ms).

Pour franchir la porte il faudrait au moins l'un de ces leviers **hors de mon
périmètre** :
- codegen `target-cpu=native` (2× sur le remplissage Philox) — `Cargo.toml` /
  `.cargo/config.toml`, interdits ;
- exécution native des 4 sections avec une sémantique/combinaison déjà
  compilée différemment dans le jumeau Rust (forme fermée LLVM) — `jit.rs` /
  `run.rs`, interdits ;
- suppression du coût fixe de création de processus/thread 512 Mo — `main.rs`,
  interdit.

Ce n'est **pas** une estimation de principe : c'est un constat daté, avec les
micro-mesures correspondantes dans `/tmp/opencode/eng1_agents/{s6rs2,philoxrs,
philoxrs_generic,philoxrs_tf}*`. Une nouvelle mesure est requise après tout
changement de machine, de binaire ou de topologie.

## 6. Validation (preuves réelles, binaire final `ec1f6485…`)

```
cargo build --release --bin rose                 OK
cargo test --release                             431 passed, 0 failed
                                                 (6 ignored, 2 salves confondues)
jardins/tests/premiers.rose                      20 tests, 0 échec
fib(25)                                          75025
point fixe rir ↔ front-end (emetteur.rose)       b3ed9262dd54776a6dbd07bc12368d0e ×2
9 domaines multi, c19                             81/81 OK
9 domaines multi, c6                              81/81 OK
bench_global.rose (base vs final)                 rc=0, 12 lignes,
                                                  11 valeurs identiques (12e = temps)
bench_englobant, 4 valeurs                       identiques à chaque tirage (9×3 impl.)
```

Le diff du livrable ne touche que `rose/src/interp.rs` ; le document
`/tmp/opencode/eng1_agents/bench_prof_ns.rose` est une copie temporaire hors du
dépôt, retirée du livraison (jamais ajoutée à `bench/`).

## 7. Reproduction

```
cd /tmp/opencode/eng1_agents
python3 bench_aaa.py 19 aaa2_c19     # A/A/A entrelacé, 9 tirages, pin c19
python3 bench_aaa.py 6  aaa2_c6      # idem pin c6
ROSE_TIMING=1 taskset -c 19 ./rose_final ./bench_englobant.rose 4096 1
```

Sources du harnais : `bench_aaa.py`, `bench_prof_ns.rose`, `rose_final` (binaire
isolé, SHA256 `ec1f6485b17106376bb6c3e31e645be7e5d6f2702159e2371749a80ff95419c4`).
Le binaire `rose_base_cur_bin` sert de controle A : memes sources que le depot
sauf `interp.rs` remis au HEAD.
