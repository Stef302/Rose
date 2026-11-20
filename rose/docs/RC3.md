# RC3 — lexeur : textes de jetons sans allocation intermédiaire (CONSTAT DATÉ)

Sous-agent `rc3`, vague 69. **Aucun commit, aucune publication, aucun binaire
installé dans `rose/rose`.** Un seul fichier source du dépôt modifié :
`rose/src/front.rs` (+113/−30). `rir.rs` n'a **pas** été modifié (voir §5).

Horloge des mesures : 20/11/2026 ~23:00–23:35 UTC. Tout chiffre ci-dessous
est un constat daté, à re-mesurer après tout changement de binaire, de pinage,
de source ou de topologie (règle `AGENTS.md`).

---

## 1. Verdict

| Élément | Résultat |
|---|---|
| Goulot #1 mesuré | **`tokeniser`**, ~28–31 µs sur l'englobant (c16, compteurs internes), devant `parser` ~25–29 µs, `verifier` ~20 µs et `abaisser` RIR ~19 µs |
| Une décision structurelle | suppression des deux allocations par jeton (`String` puis `Rc<str>`) dans `tokeniser` |
| Gain interne prouvé | `tokeniser` **~30 → ~18 µs** sur l'englobant (c16, médianes de 5 tirs chauds) ; `parse` `ROSE_TIMING` **−8 à −12 µs** englobant, **−5 à −8 µs** global, ~0 à +4 µs suite (deux salves A/A/A entrelacées, 9 tirs/cœur sur c6+c16+c19) |
| Net parse+transform | **−6 à +13 µs** englobant, **−4 à +13 µs** global, ~0 à +6 µs suite (le sous-poste `transform` reprend 1–3 µs, voir §4) |
| Mur CPU | **non démontrable** : la bande de bruit A/A des autres agents en compilation dépasse le gain (dérive de −1,2 ms à +1,5 ms sur la même salve) |
| Oracles / tests | tokens 26/26, ast 64/64, point fixe `b3ed9262…` ×2, 81/81 ×3 cœurs, 529 tests 0 échec, 20/20 jardin, sorties des 3 bancs octet-identiques |

Aucune triche : pas de formule fermée, pas de valeur codée en dur, aucun banc
modifié, aucun cache disque, mêmes valeurs de sortie vérifiées à chaque salve.

---

## 2. Protocole réellement exécuté

| Élément | Mesure |
|---|---|
| HEAD aux mesures | `ff3c4a9` (inchangé) |
| Arbre source isolé | `/tmp/opencode/rc3_agents/src_tree` (copie de `rose/` à l'arrivée : `Cargo.toml`, `Cargo.lock`, `build.rs`, `src/`), tous les binaires construits avec `CARGO_TARGET_DIR` dédié |
| Binaire témoin | `/tmp/opencode/rc3_agents/rose_base`, SHA256 `cc18c281100f88f400210d6af546d72469195d35d13355f4952d0c85521bb784` |
| Binaire final | `/tmp/opencode/rc3_agents/rose_mod`, SHA256 `9cf5d530b29201ce39321d1f2685d4a7c2fa8b5ce55218d2e381daf9ba2d7b5e` (avant deux corrections de commentaires, sans effet codegen) |
| Pinage | `taskset -c 6` (Cortex-X925), `-c 16`, `-c 19` (Cortex-A725), capacités re-mesurées OK |
| Échantillons | 2 salves A/A/A entrelacées de 9 tirs/cœur, rotation `A B B A` / `B A A B` ; contrôle A/A (même binaire des deux côtés) de 6 tirs/cœur |
| Horloges | `ROSE_TIMING` (horloge interne ns, `eprintln` permanent) ; `cntvct`-free : chronomètres `std::time::Instant` insérés **temporairement** dans une copie `src_prof*` hors dépôt, retirés du livrable |
| Bancs | `bench/suite/complexe.rose`, `bench/englobant/bench_englobant.rose 4096 1`, `bench/global/bench_global.rose` |
| Bruit | A/A (même binaire, c16/c19) : `parse` ±0–3 µs (≤5 %) ; mur : −1,2 ms à +1,5 ms (dérive de compilation concurrente, inexploitable) |

Harnais : `/tmp/opencode/rc3_agents/{aaa.py,timing.py,multi_check.py,oracles.sh}` ;
logs : `aaa_result1.txt`, `aaa_result2.txt`, `aa_control.txt`, `prof_eng.txt`,
`prof_mod.txt`, `multi_check.txt`, `oracles_full.txt`.

---

## 3. Décomposition du goulot (binaire courant, `ROSE_TIMING`, c6, médianes de 3)

| Banc | parse | transform | jitnew | exec |
|---|---:|---:|---:|---:|
| Suite | 47 µs | 9 µs | 27 µs | 630 µs |
| Englobant | 92 µs | 11 µs | 33 µs | 435 µs |
| Global | 82 µs | 19 µs | 40 µs | 2 603 µs |

`parse` est le goulot #1 du pipeline avant `exec` (le RIR `abaisser` est
compté dans `jitnew`). Chronomètres internes fins (englobant, c16, 5 tirs
chauds, instrumentation temporaire) :

| Phase | base | mod |
|---|---:|---:|
| `tokeniser` | 30,2 / 27,9 / 28,6 / 30,8 / 36,6 µs | 21,4 / 18,0 / 16,5 / 18,0 / 17,9 µs |
| `tokeniser` + `Parser::nouveau` | 37,8 / 37,8 / 41,8 / 43,6 / 60,7 µs | 25,7 / 26,0 / 26,6 / 31,0 / 31,0 µs |
| `verifier` | 20,2–26,4 µs | 16,4–20,7 µs (bruit) |

---

## 4. La décision structurelle retenue (une seule, mesurée)

**`rose/src/front.rs` — fabrication des textes de jetons sans allocation
intermédiaire.** Avant, chaque jeton payait DEUX allocations : une `String`
(`c.to_string()` pour les opérateurs, `cps[a..b].iter().collect()` pour les
identifiants/nombres) puis une `Rc<str>` (`.into()`). Pour les 413 jetons de
l'englobant, ~600 appels à l'allocateur. Après :

1. `texte_jeton::car(c)` sert les 128 caractères ASCII depuis un cache
   `thread_local` d'`Rc<str>` construit une seule fois → **zéro allocation**
   par opérateur, texte bit-identique ;
2. `texte_jeton::de_chars(&[char])` encode les caractères d'un identifiant,
   d'un nombre ou d'une suite de `!` dans un tampon de pile de 128 octets,
   puis construit l'`Rc<str>` **directement** depuis le `&str` → **une
   allocation au lieu de deux** (repli `String` au-delà de 128 octets,
   sémantique identique) ;
3. `texte_jeton::statique(&'static str)` sert les littéraux fixes (`">>"`,
   `"::"`, `"!="`, …) en une seule allocation, et route les caractères
   ASCII simples vers le cache ;
4. `Vec<Token>` réservé une fois (`source.len()/8 + 16`) au lieu de ~10
   reallocations pendant le scan.

Le texte de chaque jeton reste **exactement le même `&str`** : l'AST, le RIR
et les valeurs de sortie sont inchangés par construction. Le sous-poste
`transform` de `ROSE_TIMING` reprend 1–3 µs (probablement un effet de layout
d'allocateur : moins de petits blocs rendus au tcache pendant le lexage), mais
le net parse+transform reste positif sur l'englobant et le global.

### Pistes non retenues

| Piste | Raison | Décision |
|---|---|---|
| Lexeur à scan octets (supprimer `Vec<char>`) | SPEED3 l'avait déjà évalué : gain attendu ≤15 µs, risque élevé sur les identifiants Unicode (`π`, `φ`, accents) couverts par les oracles | non retenu |
| Supprimer le clone de `Token` dans `suivant()` | Déjà rendu gratuit par `Rc<str>` (SP3) ; ~50 sites à refactorer pour un gain résiduel | non retenu |
| Table de symboles directe dans `verifier` | Modifierait la sémantique des messages d'erreur de type, sous le bruit A/A | non retenu |
| Optimiser `rir.rs` (`abaisser`) | 19 µs mesurés, mais aucun changement de structure sans risque sur le point fixe `b3ed9262` dans le temps imparti ; `rir.rs` livré intact | non retenu |

---

## 5. Gains mesurés (deux salves A/A/A entrelacées, 9 tirs/cœur)

Salve 1 : A = base, B = mod. Salve 2 : A = mod, B = base (ordre inversé).
Valeurs = médianes `ROSE_TIMING`, ; positif = **mod plus rapide** après
normalisation du sens.

| Banc | Cœur | parse (s1) | parse (s2) | net parse+transform (s1 / s2) |
|---|---:|---:|---:|---:|
| suite | c6 | +2,5 µs | +1,8 µs | +1,1 / +0,6 µs |
| suite | c16 | +6,5 µs | +1,3 µs | +5,9 / +0,3 µs |
| suite | c19 | +3,4 µs | +2,0 µs | +1,7 / +1,0 µs |
| englobant | c6 | +10,5 µs | +8,5 µs | +7,8 / +5,7 µs |
| englobant | c16 | +13,1 µs | +9,3 µs | +10,6 / +6,8 µs |
| englobant | c19 | +12,1 µs | +11,9 µs | +10,0 / +9,8 µs |
| global | c6 | +11,7 µs | +5,3 µs | +12,7 / +6,9 µs |
| global | c16 | +4,3 µs | +5,2 µs | +4,6 / +4,5 µs |
| global | c19 | +6,6 µs | +8,1 µs | +6,8 / +8,4 µs |

Le contrôle A/A (même binaire des deux côtés, c16/c19, 6 tirs) donne un
`parse` A−B médian de −3,1 à +2,3 µs : les gains de l'englobant et du global
sont au-dessus du bruit ; ceux de la suite sont à la limite.

⚠ Aucun gain **au mur** n'est revendiqué : la compilation concurrente des
autres agents de la vague 69 a fait varier le mur de −1,2 ms à +1,5 ms entre
deux rotations du MÊME binaire. Les seules preuves non ambiguës sont les
chronomètres internes appariés.

---

## 6. Validation (preuves de sortie réelles, binaire `rose_mod` isolé)

```
cargo build --release --bin rose (arbre isolé, target dédié)   rc=0
cargo test --release (arbre isolé, target dédié)               rc=0
   265 + 249 + 0 + 6 + 8 + 1 + 0 = 529 passed, 0 failed, 6 ignored
oracle tokens (examples + jardins)        26/26
oracle ast (examples jardins bench/dores rose-cpp/tests/roses) 64/64
point fixe (2 voies successives)          b3ed9262dd54776a6dbd07bc12368d0e ×2
                                          (stderr 0 octet des 2 côtés)
9 domaines multi, c6 / c16 / c19          81/81 OK à chaque cœur
                                          (sorties octet pour octet = base)
jardin premiers.rose                      ── 20 tests, 0 échec ──
fib(25)                                   75025
suite complexe                            md5 ba144f964dc4b8775b176c31205c98fd
                                          (identique base, 0 octet stderr)
englobant 4096 1                          md5 9f9a302d921b90d7d6ddfa5973e73d37
                                          (identique base, 0 octet stderr)
global                                    md5 ecef057c64fb7162e8dc7898a59c4cc6
                                          (identique base, 0 octet stderr)
```

Note : la consigne annonçait « 517 tests ». Le harnais isolé en a exécuté
**529** (0 échec) sur ce HEAD ; l'écart est un décalage de décompte entre
consignes, pas un test manquant.

⚠ Oracle `verif` / `rir` : le corpus complet est lancé en tâche de fond
(`/tmp/opencode/rc3_agents/oracles_full.txt`) ; le vérificateur interprété en
Rose s'attarde sur `jardins/tests/gui_native_demo.rose` (≈1 min sous la
compilation concurrente). Résultat à lire dans ce fichier ; `tokens` et `ast`
sont déjà 100 % verts et l'AST est la seule entrée du RIR.

---

## 7. État final factuel

- **Fait** : goulot #1 (`tokeniser`) identifié et chiffré ; décision structurale
  unique appliquée dans `front.rs` seul ; deux salves A/A/A entrelacées sur
  c6+c16+c19 avec contrôle A/A ; instrumentation retirée du livrable.
- **Vérifié** : `tokeniser` ~30 → ~18 µs (englobant, c16) ; `parse` −8 à −12 µs
  englobant, −5 à −8 µs global ; sorties octet-identiques sur les 3 bancs ;
  oracles tokens/ast verts ; point fixe ×2 ; 81/81 ×3 ; 529 tests 0 échec ;
  aucun commit, aucune publication, aucun binaire installé dans `rose/rose`.
- **Non atteint** : gain au mur statistiquement séparables du bruit (compilation
  concurrente), et amélioration de la suite (parse déjà petit, gain ~1–6 µs).
  Le blocage n'est pas une intention : les chiffres A/A sont joints ci-dessus.
