# RUSTF — Philox masqué : une optimisation mesurée, porte Rust toujours fermée

Auteur : sous-agent `rustf`. **Aucun git commit, aucune publication, aucun
binaire copié dans `rose/rose`.** Un seul fichier source livré :
`rose/src/run.rs` (2 blocs modifiés : `remplir_philox` et
`philox4x32_10_pour`, ~55 lignes). `interp.rs` n'a **pas** été modifié. Les
fichiers interdits (`jit.rs`, `front.rs`, `rir.rs`, `main.rs`, `i128x.rs`)
n'ont pas été touchés. Aucun banc, aucun jumeau Rust, aucune valeur du banc
n'a été modifié.

**Horloge des mesures : 20/11/2026 ~20:45–22:25 UTC.** Tout chiffre est un
constat daté, à re-mesurer après tout changement de binaire, de pinage ou de
topologie.

---

## 1. Verdict

| Porte | Résultat |
|---|---|
| Une seule optimisation mesurée | ✅ voie masquée Philox spécialisée : **−16,8 µs par remplissage** (11/11 tirages appariés positifs), soit **−14,5 %** du poste |
| Gain `exec` interne apparié | ✅ **−21,5 µs c6 / −21,6 µs c16** (médianes, 15 rotations) |
| Gain au mur démontrable | ❌ **non** : dans la bande de bruit A/A (le mur reste 1,16–1,22× Rust) |
| Rose ≤ Rust — englobant | ❌ **non atteinte** : ratio final/Rust **1,16–1,22** (médianes c6/c16, 15 et 21 rotations) |

Aucune triche : pas de formule fermée, pas de valeur codée en dur, pas de
mémoïsation entre processus, mêmes quatre chaînes de sortie vérifiées à
chaque tirage du banc.

---

## 2. Protocole réellement exécuté

| Élément | Mesure |
|---|---|
| Arbres isolés | `/tmp/opencode/rustf_agents/final/{A,B}` (A = témoin, B = identique sauf `run.rs`) ; aucun binaire livré dans le dépôt |
| Binaires | A `da4be080830a0fc25a7a2deb2921f73ed31f256297742bf10030f0f6382c3e7d`, B `ebcb0b4fc7ebc8e27c21d35f10d7fd94edfff2673b74a0f82b1910d1b873d5f1` (SHA-256) |
| Construction | `CARGO_TARGET_DIR` dédié, `cargo build --release --bin rose` ; A puis B dans le même arbre figé, seul `run.rs` diffère (85 lignes de diff) |
| Pinage | `taskset -c 6` et `-c 16`, capacités re-vérifiées (`thread_siblings_list` = 6 / 16), Cortex-X925 / Cortex-A725 |
| Échantillons | mur : **21 rotations entrelacées A/B/Rust** par cœur (+ 15 en salve indépendante) ; `exec` interne : **15 rotations A/B** par cœur ; micro Philox : **11 rotations A/B** |
| Horloges | CPU `getrusage(RUSAGE_CHILDREN)` pour le mur ; `ROSE_TIMING.exec` (horloge interne ns) pour `exec` ; ligne `min:` du jumeau Rust |
| Jumeau Rust | `/tmp/opencode/speed2_agents/eng_rs` (non modifié) |
| Valeurs | englobant `46297232.426252 / 564 / 157158 / 11899223598484`, vérifiées à chaque tirage des deux cœurs |
| Bruit A/A | micro Philox 9 tirages A/A : écart apparié médian −2,8 µs sur 2 310 µs, mais valeurs jusqu'à −79 / +19 µs ; mur : écarts appariés A/B jusqu'à ±300 µs sur une rotation. **Un gain mural de 20 µs n'est donc pas separable du bruit** ; seuls les gains micro et `exec` interne le sont. |

Harnais hors dépôt : `/tmp/opencode/rustf_agents/{measure,micro,inner_ab,validate}.py`,
CSV/JSON `rustf_final{2,3}_c{6,16}.*`, `ph{FinalA6,H}.json`, `inner_c{6,16}.json`.

---

## 3. Re-mesure du binaire courant et décomposition

`ROSE_TIMING`, englobant `4096 1`, c6, 9 tirs chauds (instrumentation
temporaire dans un arbre `prof` jamais livré) :

| Poste | µs | % `exec` |
|---|---:|---:|
| `exec_mc` (Monte-Carlo, `interp.rs`) | 227 | ~50 % |
| `remplir_hasard_philox` (`run.rs`) | 120 | ~27 % |
| `exec_s4` (IEEE, `interp.rs`) | 13 | ~3 % |
| `exec_ph` (glanée, `interp.rs`) | 8 | ~2 % |
| solde (dispatch RIR, crible, sortie) | ~80 | ~18 % |
| **`exec` total** | **~450** | 100 % |

Jumeaux Rust isolés (mêmes formules, boucles désactivées une à une,
`rustc -O3 -C target-cpu=native`) : **MC 205 µs**, **Philox 63 µs**. Le
goulot actionnable était donc **Philox : 120 µs contre 63 µs**, et non le
dispatch.

---

## 4. L'unique optimisation livrée

`remplir_philox`, voie `alphabet.puissance_de_2` (cas du banc : 64 octets) :

1. le noyau `philox4x32_10_pour` est réécrit en scalaire (`#[inline(always)]`) :
   compteur et clé dans des registres, plus de tableau `[u32; 4]` réassigné
   à chaque tour ni d'appel à `mul_hi_lo` ;
2. la boucle sur les blocs entiers est séparée de la queue : plus de
   `if i+j < n` ni de `if pow2` par octet, un seul magasin de 4 octets par
   bloc, indexation `get_unchecked` bornée par le masque ;
3. la queue (tampon non multiple de 4) garde exactement le même compteur de
   bloc et le même mot Philox que la boucle d'origine.

La voie alphabet NON puissance de 2 et la gestion `s == 1` sont inchangées.
Les dix tours, les deux multiplicateurs, les deux constantes de clé et
l'ordre des XOR sont identiques : **aucune valeur n'est codée en dur**, aucun
tour n'est supprimé.

### Mesure micro — 20 remplissages, c6, 11 rotations A/B

| Binaire | `exec` médian | par remplissage |
|---|---:|---:|
| A (témoin) | 2 321,5 µs | 115,9 µs |
| B (final) | 1 989,8 µs | **99,5 µs** |
| A−B apparié | **+336,9 µs** | **−16,8 µs (−14,5 %)** |

**11/11 différences positives** (min +288,3 ; max +384,3 µs).

### Mesure `exec` interne — banc entier, 15 rotations A/B

| Pin | A médian | B médian | A−B apparié | min | max |
|---|---:|---:|---:|---:|---:|
| c6 | 452,2 µs | 439,4 µs | **+21,5 µs** | −31,4 | +83,3 |
| c16 | 437,7 µs | 420,3 µs | **+21,6 µs** | −31,9 | +99,0 |

---

## 5. Gain au mur c6/c16 — rotations appariées A/B/Rust

| Pin | rotations | A/Rust | B/Rust | B−A apparié (méd) |
|---|---:|---:|---:|---:|
| c6 | 21 | 1,182 | **1,204** | −7 µs |
| c16 | 21 | 1,248 | **1,223** | −23 µs |
| c6 | 15 | 1,346 | **1,198** | −30 µs |
| c16 | 15 | 1,205 | **1,158** | −67 µs |

Médianes CPU brutes de la salve 21 (ms) : c6 A 1,419 / B 1,446 / Rust
1,201 ; c16 A 1,116 / B 1,093 / Rust 0,894. **Honnêteté : les deux salves ne
se contredisent que dans le bruit** (écarts A/B de −67 à +23 µs, contre une
bande A/A de plusieurs centaines de µs). Le gain réel démontré est celui du
micro Philox et de `exec` interne ; le ratio mural final est **~1,2× Rust**,
pas ≤ 1.

---

## 6. Validation (preuves de sortie réelles, binaire B)

```
cargo build --release --bin rose (arbre B, target dédié)      rc=0
cargo test --release (arbre B, target dédié)                  rc=0 ;
     265 + 249 + 0 + 6 + 8 + 1 = 529 passed, 0 failed, 6 ignored
9 domaines multi, c6, 9 tirages    81/81 OK (octets = témoin A)
9 domaines multi, c16, 9 tirages   81/81 OK (octets = témoin A)
point fixe (2 voies)               b3ed9262dd54776a6dbd07bc12368d0e ×2
   (rose rir compilateur/emetteur.rose ET rose compilateur/emetteur.rose
    compilateur/emetteur.rose ; A = B, stderr 0 octet)
englobant c6 + c16, 3 tirages      46297232.426252 / 564 / 157158 / 11899223598484
philox différentiel A vs B          md5 8c0e0d5f7de6cad99316e4788814b719
   (965 octets : alphabet 64/63/10/7/2, tailles 65536/777/103/12/9,
    graines 2026 / 4294967297 / -7 / 99 / 1, y compris la queue non multiple de 4)
```

---

## 7. Blocage chiffré — pourquoi la porte reste fermée

Après ce changement, le décomposition interne reste (c6, médianes) :

| Poste | Rose | Rust | écart |
|---|---:|---:|---:|
| MC | 227 µs | 205 µs | +22 µs |
| Philox | ~103 µs | 63 µs | +40 µs |
| `exec` total | ~410–440 µs | calcul Rust **entier** ~290 µs | +120–150 µs |
| + front/JIT Rose | ~105 µs | — | — |

Même en supposant **Philox ramené à la parité Rust** (−40 µs) et MC à la
parité Rust (−22 µs), le plancher `exec` reste ~350 µs, déjà supérieur aux
~290 µs du calcul Rust **entier**, avant d'ajouter les ~105 µs de
`parse + transform + jitnew`. L'écart résiduel minimal est donc
**≥ 60 µs sur `exec` seul**, et **≥ 165 µs sur le total interne**.

Ce solde est dans `exec_mc` (`interp.rs`, 227 µs), le dispatch RIR et la
sortie — c'est-à-dire hors du poste unique autorisé par la consigne. Le
changer exigerait une deuxième optimisation structurale (noyau MC) et/ou une
modification du front-end, ce que cette mission ne demandait pas.

---

## 8. État final factuel

- **Fait** : 1 optimisation livrée dans `run.rs` seul. Aucun autre fichier
  source de ce sous-agent modifié, aucun commit, aucune publication, aucun
  binaire installé dans `rose/rose`.
- **Vérifié** : −16,8 µs par remplissage Philox (11/11), −21,5/−21,6 µs
  d'`exec` interne c6/c16, 529 tests verts, 81/81 ×2 cœurs, point fixe ×2,
  4 valeurs englobant ×6 tirages, 965 octets de Philox différentiel identiques.
- **Non atteint** : Rose ≤ Rust sur l'englobant. Ratio final **~1,2×** ;
  blocage chiffré en §7. Aucune triche, aucun banc modifié.
