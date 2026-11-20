# SPEED2 — dépasser Rust sur la suite complexe et l'englobant (CONSTAT DATÉ)

Auteur : sous-agent `speed2`. **Aucun git commit, aucune publication, `rose/rose`
non remplacé.** Un seul fichier source possédé et livré modifié :
`rose/src/interp.rs` (plus ce document). `jit.rs`, `i128x.rs`, `run.rs`,
`front.rs`, `main.rs`, `Cargo.toml`, `PLAN.md` et les docs des autres agents
n'ont **pas** été modifiés. Tous les correctifs antérieurs (INT2, ENG1 S4,
GLB2 superinstructions, CLOSE2, correction régression D5/D6/D7 du 20/11)
sont préservés : le diff livré ne touche que `hasard_i64` (ajout de deux
helpers) et `exec_mc` (boucle ILP), rien d'autre.

**Horloge de la machine : 20/11/2026 ~09:33–10:21 UTC** (la consigne annonce
21/11). Tout chiffre ci-dessous est un constat daté, à re-mesurer après tout
changement de binaire, de pinage ou de topologie.

---

## 1. Verdict

| Porte | Résultat |
|---|---|
| Rose ≤ Rust — suite complexe | ❌ **non atteinte** (ratio 1,09–1,19 selon cœur/salve) |
| Rose ≤ Rust — englobant | ❌ **non atteinte** (ratio 1,19–1,36) |
| Gain réel mesuré sur l'englobant | ✅ **−50 à −55 µs CPU** par exécution, stable en signe sur c6, c16, c19 |
| Gain réel sur la suite complexe | ❌ **aucun** (la voie modifiée n'y est pas empruntée ; les écarts mesurés sont du bruit) |

Aucune triche : pas de mémoïsation de résultat, pas de formule fermée sur une
boucle du banc, pas de constante du banc codée en dur, pas de banc modifié,
pas de cache disque. Les bornes, diviseurs, rayons et bornes `hasard` sont lus
dans le RIR, comme dans le code d'origine.

---

## 2. Protocole réellement exécuté

| Élément | Mesure |
|---|---|
| Binaire témoin | `/tmp/opencode/speed2_agents/rose_base`, SHA256 `1658a608c6251316f086c5dd2c3b4b0e4c5a7b450f6530bc52357acb9d3e47f4` |
| Binaire final | `/tmp/opencode/speed2_agents/rose_final`, SHA256 `743e2203d8417b5c7ed2c50e9b3cbf303d7f3366a38e98afb8fed3c7a96430bb`, static-pie |
| Source isolée | `/tmp/opencode/speed2_agents/src_tree` = copie de l'arbre au départ ; `interp.rs` et `i128x.rs` repris de `HEAD 2961f3d` (un autre agent avait provisoirement cassé `i128x.rs` en cours de session). **Tous** les binaires A/B viennent de la même copie, avec `CARGO_TARGET_DIR` dédiés. |
| Rust suite | `rustc -O complexe_rs.rs` → SHA256 `efb8915dafbba0e18f929f21110589814c233ec40dd1cd18258902c0e3082e14` (identique au jumeau des agents précédents) |
| Rust englobant | `rustc -C opt-level=3 -C target-cpu=native -C overflow-checks=off -C debug-assertions=off` → SHA256 `d9ae967d1f1e97e21da652c62e22c6f4e723ef26c0125e3710818474dcbe56cb` (identique au jumeau précédent) |
| Pinage | **`taskset -c 6`, `-c 16` et `-c 19`**, capacités re-mesurées (taskset OK sur les trois) |
| Échantillons | **12 par implantation et par cœur**, rotation A/B/C puis B/C/A à chaque répétition, médianes **et différences appariées** ; 20/11/2026 09:47–10:13 UTC |
| Horloges | CPU `getrusage(RUSAGE_CHILDREN)` pour les ratios ; `cntvct_el0` (1 GHz mesuré via `cntfrq_el0`) pour la décomposition |
| Valeurs | vérifiées à **chaque tirage** : suite `0.9999999999860738` / `333333833333500000` ; englobant `46297232.426252 / 564 / 157158 / 11899223598484` |
| Bruit | charge 4,2–8,7 ; min-max CPU très larges (ex. suite c16 base 1,144–3,010 ms). C'est pourquoi la décision repose sur les **différences appariées entrelacées**, pas sur les médianes marginales. |

Harnais : `/tmp/opencode/speed2_agents/bench_suite.py`, `bench_englobant.py`,
`bench_multi.py` ; logs/CSV/JSON : `final_{suite,engl}_{c6,c16,c19}.{log,csv,json}`.

---

## 3. Décomposition par phase (instrumentation temporaire, RETIRÉE avant livraison)

Compteurs `AtomicU64` + `cntvct_el0` insérés dans `interp.rs`, relevés sur le
binaire `rose_prof` ; **aucune instrumentation ne subsiste dans le livrable**
(`grep -c 'SP2\|sp2_dump\|cntvct' rose/src/interp.rs` = 0, sauf les deux helpers
nommés `sp2_*` de la section 4).

### 3.1 Suite complexe (c16, médianes de 5, µs)

| Phase | µs |
|---|---:|
| parse | 36–42 |
| transform + clone AST | 5–6 |
| RIR fonctions + principal | 17–20 |
| `prepare_jit` + Jit + compile | ~0,1 (CLOSE2 effectif : aucun module Cranelift construit) |
| **exec** | **~600** |
| └ appel natif complexe (`PROFIL-JIT appels=1`) | **~518** |
| └ superinstruction carrés `j*j` | **~71,5** |
| └ surcoût RIR résiduel | ~10 |

Goulot #1 : l'accumulation complexe, **~86 % de `exec`**. Rust jumeau
`-O` fait le même calcul en **558 µs** (micro-jumeau, 9 tirages, meilleur) :
Rose est déjà **plus rapide sur cette boucle**. En revanche rustc élimine
complètement la seconde boucle `j*j` (0 µs mesuré, résultat non utilisé),
alors que Rose doit rendre la somme **exacte** en BigInt (71,5 µs).

### 3.2 Englobant (c16, médianes de 5, µs)

| Phase | Avant | Après |
|---|---:|---:|
| parse | 121–130 | inchangé |
| transform / RIR fonctions / RIR principal | 13 / 19 / 37 | inchangé |
| `prepare_jit` + Jit | ~0,2 | inchangé |
| **exec** | **~495** | **~432** |
| └ **S6 Monte-Carlo** | **280–286** | **223–225** |
| └ Philox (remplissage 65 536 octets) | 125–127 | inchangé |
| └ S4 IEEE | 12,8 | inchangé |
| └ glanée S7 | 7,8 | inchangé |
| └ crible S5 | 6–7 | inchangé |
| └ détection superinstructions + clone du corps | 13 + 8 | inchangé |

Goulot #1 : **S6 Monte-Carlo, ~58 % de `exec`** (~37 % du temps interne total).

Sections du jumeau Rust (`eng_sections`, `-O3 target-cpu=native`, même salve) :
S4+S5 ≈ 12 µs, **S6 = 204,7 µs**, S7 = 68–90 µs ; total calcul ≈ **287 µs**.
Rose paie en plus parse + RIR ≈ 190 µs, que le jumeau Rust n'a pas.

---

## 4. La décision structurale retenue (une seule, mesurée)

**Monte-Carlo ILP-8 dans `exec_mc`** (`rose/src/interp.rs`).

L'état splitmix avance d'une constante `C = 0x9E3779B97F4A7C15` à chaque
tirage. Les 16 états des 8 itérations suivantes sont donc `e + k·C` et peuvent
être calculés d'abord ; les 16 mixages (`sp2_mix`, miroir bit pour bit des
deux multiplications de `crate::run::rng_tirer`) puis les 16 réductions de
Lemire (`sp2_lemire`) sont entrelacés, ce qui remplit la file d'attente des
multiplications au lieu d'une chaîne dépendante `x` puis `y`. La queue
(`borne - cpt` non multiple de 8) **et** les bornes puissances de 2 (voie
masque, pas Lemire) et hors `i64` gardent exactement l'ancien chemin
`hasard_i64`. Le compteur, l'accumulateur et l'état final du RNG sont mis à
jour comme avant. Micro-jumeau isolé : 243,6 → 224,0 µs (sans `target-cpu`) ;
dans Rose : 285 → **224 µs**, soit **−60 µsInternes**.

### Tentatives mesurées puis REJETÉES (code retiré)

| Tentative | Mesure | Décision |
|---|---|---|
| Philox ILP-8 réécrit dans `interp.rs` (8 blocs entrelacés, mirror exact) | **58 µs** dans un jumeau isolé, mais **117–127 µs** une fois compilé dans le crate Rose (vérifié par compteur interne, 4 blocs/8 blocs, `#[inline(always)]`, pile locale, `get_unchecked`, `#[inline(never)]` : toujours ~120 µs). Aucun gain vs les 125 µs historiques. | **retiré** |
| NEON pour splitmix64 | `std::arch::aarch64` n'expose pas `vmulq_u64` ; l'assembleur GNU refuse `mul v2.2d, …` (absence de MUL NEON sur lanes 64 bits). Le `umulh` de Lemire reste scalaire de toute façon. | abandonné |
| Philox NEON 4 blocs en `u32x4` | nécessite `umull`/`umull2`+`uzp2` par tour ; gain potentiel ~30 µs mais complexité de parité élevée, et le contexte de compilation Rose a déjà annulé un gain de ×2 attendu sur la version scalaire. | non retenu |
| Unroll MC à 4 | 229,8 µs (micro) contre 224,0 pour ILP-8 | ILP-8 retenu |
| Miroir d'état avec `e + k*C` en constantes | débordement de constante au calcul ; remplacé par 16 `wrapping_add` cumulés | — |

---

## 5. Gains réels (A/A/A entrelacé, 12 échantillons, CPU médian et différences appariées)

### Suite complexe — la voie modifiée n'est pas empruntée

| Pin | base méd | final méd | Rust méd | final−base (apparié) | final/Rust | base/Rust |
|---|---:|---:|---:|---:|---:|---:|
| c6 | 1,578 ms | 1,480 ms | 1,371 ms | **−38,5 µs (bruit)** | 1,079 | 1,151 |
| c16 | 1,374 ms | 1,346 ms | 1,131 ms | **+2,5 µs (bruit)** | 1,190 | 1,214 |
| c19 | 1,465 ms | 1,463 ms | 1,347 ms | **+5,0 µs (bruit)** | 1,086 | 1,088 |

Honnêteté : **aucun gain imputable au changement**. Le banc n'appelle jamais
`exec_mc` ; les différences positives/négatives sont du bruit de la machine
(une salve A/A base/base avait déjà donné ±126 µs le 20/11 à 09:47).

### Englobant — gain réel, stable en signe

| Pin | base méd | final méd | Rust méd | final−base (apparié) | final/Rust | base/Rust |
|---|---:|---:|---:|---:|---:|---:|
| c6 | 1,618 ms | 1,596 ms | 1,218 ms | **−50,0 µs** | 1,310 | 1,328 |
| c16 | 1,588 ms | 1,393 ms | 1,167 ms | **−55,0 µs** | 1,194 | 1,361 |
| c19 | 1,486 ms | 1,435 ms | 1,057 ms | **−50,0 µs** | 1,357 | 1,406 |

Écart résiduel apparié `final−Rust` : **+364,5 µs (c6), +368,5 µs (c16),
+393,0 µs (c19)**. Aucune salve n'a donné Rose ≤ Rust.

---

## 6. Pourquoi les deux portes restent fermées — reste chiffré

**Englobant.** Même en ramenant S6 au temps du jumeau Rust natif (204,7 µs)
et Philox à son meilleur (58 µs), le plancher Rose est :

```
parse 120 + transform 13 + RIR 56 + S4 13 + S5 7 + S7 8
 + S6 205 + Philox 58 + divers/interface ~20  ≈ 500 µs internes
```

contre **287 µs** internes pour le jumeau Rust `-O3 target-cpu=native`, et un
démarrage de processus équivalent (~0,70 ms des deux côtés). Il manque donc
**~210 µs**, uniquement dus à des postes situés hors du fichier possédé
(`front.rs` : parse ; `rir.rs` : abaissement) ou à une codegen
`target-cpu=native` (`Cargo.toml` interdit). Aucun gain de boucle native ne
peut combler l'écart sans toucher à ces fichiers.

**Suite complexe.** Calcul Rose inévitable = complexe 518 µs (déjà **40 µs
plus vite** que les 558 µs du jumeau Rust) + carrés exacts 71,5 µs = **589 µs**,
avant même parse (40 µs) et RIR (18 µs). Le jumeau Rust paie 558 µs parce que
rustc supprime la somme de carrés (résultat bigint non calculé côté Rust).
Ramener les carrés à zéro exigerait une **formule fermée**, explicitement
interdite par la consigne ; supprimer parse/RIR exigerait `front.rs` /
`rir.rs` / `main.rs`, interdits. Écart structurel minimal : **~120 µs**.

Ce sont des blocages physiques dans le périmètre autorisé, pas un renoncement
prématuré : les deux goulots mesurés ont été attaqués, un a cédé (−60 µs),
l'autre a été mesuré puis rejeté honnêtement faute de gain réel dans ce crate.

---

## 7. Validation (preuves réelles, binaire `rose_final`)

```
cargo build --release --bin rose (arbre source isolé, target dédié)   rc=0, static-pie
cargo build --release --bin rose (dépôt courant, target dédié)        rc=0
cargo test --release              rc=0 ; 220+204+0+6+1+0 = 431 passed, 0 failed,
                                  6 ignored (6 tests ignorés au total)
jardins/tests/premiers.rose       ── jardin/premiers : 20 tests, 0 échec ──
fib(25)                           75025
point fixe (2 voies, rir)         b3ed9262dd54776a6dbd07bc12368d0e ×2
                                  (base et final, md5 identiques)
9 domaines multi, c6, 9 tirages   81/81 OK (sorties OCTET POUR OCTET = base)
9 domaines multi, c16, 9 tirages  81/81 OK (sorties OCTET POUR OCTET = base)
bench_global.rose                 rc=0, 12 lignes, 0 octet stderr,
                                  11 valeurs IDENTIQUES à la base / CLOSE2
suite complexe c6+c16+c19         md5 ba144f964dc4b8775b176c31205c98fd (identique base)
englobant c6+c16+c19              46297232.426252 / 564 / 157158 / 11899223598484
MC différentiel 30 cas            30/30 OK (graines, bornes 1..257, x/y finaux)
                                  (-bornes multiples de 8, tail non multiple, etc.)
```

Précision sur le test « 9 domaines multi » : le harnais historique
`/tmp/opencode/benchmulti/bench_now.py` n'existe plus sur cette machine ; j'ai
écrit `/tmp/opencode/speed2_agents/bench_multi.py` (9 domaines : fib, boucle,
premiers, factorielle, pgcd, chaîne, liste, table, E/S ; 9 tirages par cœur),
et il vérifie l'**égalité des sorties final = base**, pas un ratio de vitesse
face à Rust. Les 81/81 sont donc une preuve de non-régression, pas une
réaffirmation des ratios du 20/11.

---

## 8. État final

- Fait : décomposition chiffrée des deux bancs ; goulot #1 identifié sur
  chacun ; une décision structurale mesurée (MC ILP-8) ; re-mesure c6+c16+c19
  avec rotations appariées ; instrumentation retirée ; validations ci-dessus.
- Vérifié : gains −50/−55/−50 µs sur l'englobant ; stabilité des sorties
  partout ; compilation et 431 tests verts.
- Reste : **portes non atteintes**. Suite ~×1,09–1,19 ; englobant ~×1,19–1,36.
  Blocage précis : parse/RIR (`front.rs`, `rir.rs`) et codegen native
  (`Cargo.toml`), tous hors du fichier exclusif, auxquels s'ajoute la somme
  de carrés exacte dont l'élimination par formule fermée est interdite.
- Aucun commit, aucune publication, aucun binaire installé dans `rose/rose`.
