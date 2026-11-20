# RC2 — tentative d'optimisation de `exec` dans `interp.rs` : blocage mesuré

Auteur : sous-agent `rc2`. **Aucun commit, aucune publication, aucun binaire
installé dans `rose/rose`.** Aucun fichier source du dépôt n'a été modifié
de façon permanente par ce chantier : à la copie (20/11 22:56 UTC),
`rose/src/interp.rs` avait le SHA-256
`02cabaead86696c69ec43aaa3685a9ad292d6da16f22a90fdde3b035c5554ff5`.
Des agents concurrents ont poursuivi pendant la mesure (HEAD passé de
`ff3c4a9` à `b6af2cc`, avec `cf9840c` qui modifie `sp5_carres_neon` dans le
MÊME fichier) : je n'ai **ni annulé ni incorporé** leur changement. Le noyau
`exec_mc` mesuré ici est identique dans les deux versions (le diff ne touche
que `sp5_carres_neon`, banc global). Seul ce document (`rose/docs/RC2.md`) est
ajouté. Toutes les variantes ont été compilées et mesurées dans
`/tmp/opencode/rc2_agents/` puis écartées.

**Horloge des mesures : 20/11/2026 ~22:56 → 23:40 UTC.** Tout chiffre est un
constat daté sur la charge machine observée (load 3–6, autres agents actifs),
à re-mesurer après tout changement de binaire, de pinage ou de topologie.

---

## 1. Verdict

| Question | Réponse |
|---|---|
| Une optimisation dans `interp.rs` qui bat le bruit A/A | ❌ **aucune trouvée** |
| Blocage physique prouvé | ✅ oui (§5 et §6) |
| `exec` réduit | ❌ non : plancher mesuré **413–415 µs**, inchangé |
| Valeurs du banc, point fixe, tests | ✅ identiques / verts (§7) |

Honnêteté : rapport **négatif**. Sept variantes ont été réellement compilées,
lancées et mesurées ; aucune n'a franchi la bande de bruit A/A. Quatre ont
ÉTÉ mesurées plus lentes de 15 à 220 µs. Je ne revendique aucun gain.

---

## 2. Protocole réellement exécuté

| Élément | Mesure |
|---|---|
| Arbres isolés | `/tmp/opencode/rc2_agents/{tree,A,B,prof,prof2,asmtree}` ; aucun binaire livré dans le dépôt |
| Binaires | A `a26da66cbadea9a53a399492b6a68e60ce3f3e68a0427b9a09d796c059e9bd30`, B = même arbre figé, seul `interp.rs` diffère ; `CARGO_TARGET_DIR` dédié |
| Construction | `cargo build --release --bin rose` (LTO grasse + 1 CGU du dépôt), A puis B dans le même arbre |
| Pinage | `taskset -c 6` (Cortex-X925), `-c 16` et `-c 19` (Cortex-A725) ; `thread_siblings_list` re-vérifié = 6 / 16 / 19 |
| Horloge | `ROSE_TIMING.exec` (horloge interne ns) ; harnais `/tmp/opencode/rc2_agents/measure.py`, 2 échauffements + **9 rotations entrelacées A/B/B/A** par cœur, médianes et différences appariées |
| Banc | `bench/englobant/bench_englobant.rose 4096 1`, 4 valeurs `46297232.426252 / 564 / 157158 / 11899223598484` vérifiées à **chaque** tirage des deux binaires |
| Jumeau Rust | `/tmp/opencode/speed2_agents/eng_rs 4096 1`, `min:` 294–308 µs à jeun (les 1,009 ms lus en pleine charge sont un artefact de charge, pas une mesure retenue) |
| Bruit A/A | binaire identique comparé à lui-même, 12 rotations : **A−A2 médian +16,0 µs c6 / +6,3 c16 / +4,3 c19**, extrêmes −55…+115 µs. **Aucun gain < 16 µs n'est démontrable sur c6.** |

CSV/JSON bruts : `A_baseline.json`, `AA_noise.json`, `AB_B1.json` … `AB_B9.json`
dans `/tmp/opencode/rc2_agents/`.

---

## 3. Décomposition re-mesurée — correction des chiffres transmis

Instrumentation temporaire dans un arbre `prof2` **jamais livré** (compteurs
atomiques, aucune E/S dans la boucle chaude) :

```text
PROF-ATOM ops=61 opt_us=[1,276,0,0,3,0,0,13,0,0,104,0] philox_us=97
```

| Poste | µs (c6, médian chaud) | Fichier réel | Écart vs consigne |
|---|---:|---|---|
| `exec_mc` (Monte-Carlo, ILP-8) | **226** | `interp.rs` | ≈ 227 (confirmé) |
| `remplir_philox` | **97** | `run.rs` (**hors périmètre**) | la consigne disait 120 : **RUSTF a déjà réduit ce poste de ~21 µs** dans le HEAD courant |
| `exec_s4` (IEEE) | 12 | `interp.rs` | ≈ 13 (confirmé) |
| `exec_ph` (glanée) | 8 | `interp.rs` | ≈ 8 (confirmé) |
| `AfficherAbstrait` (4 valeurs, `%.*g`) | 13 | `front.rs` (**hors périmètre**) | non décomposé dans la consigne |
| `SautSiFaux` + toute la boucle RIR (61 ops) | 3 | `rir.rs` / `interp.rs` | la consigne disait « dispatch ~80 µs » : **les ~57 µs manquantes sont Philox déjà optimisé + formatage**, pas le dispatch |
| solde (crible, `tampon(65536)`, conversions) | ~50 | `run.rs` / `rir.rs` | — |
| **`exec` total A** | **413,3 / 414,8 / 415,4** (c6/c16/c19) | — | ≈ 440 (le chiffre de la consigne est une mesure à froid) |

**Conséquence :** dans l'état réel du dépôt, le dispatch RIR n'exécute que
**61 opérations** (mesuré) et coûte ~3 µs. Le goulot actionnable restant dans
`interp.rs` est **unicement `exec_mc` (226 µs, 55 % de `exec`)**.

---

## 4. Les sept variantes compilées et mesurées

Toutes les différences sont `A − B` appariées (positif = B plus rapide).
Chaque ligne = 9 rotations par cœur, mêmes 4 valeurs vérifiées.

| # | Variante (`interp.rs`, boucle ILP-8 de `exec_mc`) | c6 | c16 | c19 | Décision |
|---|---|---:|---:|---:|---|
| B1 | ILP-16 : 32 états/tours, 16 points/itération | **−86,9** (0/9) | **−100,2** (0/9) | **−80,4** (0/9) | ❌ rejeté, +80…100 µs (spills) |
| B3 | fusion des deux boucles états + mixage (suppression du tableau `s`) | +3,8 (5/9) | +2,5 (5/9) | −5,3 (4/9) | ❌ dans le bruit |
| B4 | états `e + PAS·(j+1)` au lieu de la chaîne de `wrapping_add` | +5,5 (6/9) | +14,6 (7/9) | −0,4 (4/9) | ❌ LLVM applique déjà la transformation (vu en asm) |
| B5 | 8 accumulateurs indépendants au lieu d'un seul | **−49,3** (1/9) | **−52,4** (1/9) | **−51,1** (1/9) | ❌ rejeté, +50 µs (pression registres) |
| B6 | test du cercle en entier `qx²+qy² ≤ d²` avec repli flottant sur frontière | **−229,7** (0/9) | **−214,7** (0/9) | **−221,3** (0/9) | ❌ rejeté, +220 µs |
| B7 | 8 booléens locaux réduits en arbre au lieu de la chaîne de `cinc` | −3,8 (3/9) | −1,4 (4/9) | +1,8 (5/9) | ❌ dans le bruit |
| B9 | `#[inline(never)]` sur `exec_mc` | +2,8 (6/9) | +1,8 (7/9) | **−6,0** (4/9) | ❌ échoue sur c19 (règle : battre les deux contrôles voisins) |

Expérience complémentaire **non livrable** (valeurs volontairement altérées) :
remplacer les 16 `fdiv` par `q · (1/div)` donne des **valeurs de sortie
identiques sur ce banc**, mais est **plus lent de 14–22 µs** sur les 3 cœurs.
Le `fdiv` n'est donc pas le goulot.

---

## 5. Blocage physique prouvé — le noyau MC est au plancher matériel

Désassemblage du binaire A non stripé (`aarch64-linux-gnu-objdump`, boucle
`0x8012c…0x80418` dans `Interp::eval_slots`, où `exec_mc` est inliné) :

```text
add  x30, x28, x13      ; x13…x27 = k·PAS précalculés → états indépendants
eor  x30, x30, x30, lsr #30
mul  x30, x30, x12      ; splitmix64 exact, 2 multiplications
...
umulh x23, x23, x8      ; Lemire exact, 1 umulh
ucvtf d2, x30
fdiv  d2, d2, d0        ; x = q/div (division IEEE, 16 par itération)
fmul  d2, d2, d2
fadd  d2, d2, d3
fcmp  d2, d1
cinc  x23, x23, le      ; accumulateur, chaîne de 8 dépendances
```

Ce que le compilateur fait **déjà** et que mes variantes ne peuvent qu'imiter
ou casser :

1. les 16 états sont calculés par `add x28, constante`, donc la chaîne de
   dépendance des `wrapping_add` est déjà brisée (explication de B4 = neutre) ;
2. `umulh`, `ucvtf`, `fdiv`, `fmul`, `fadd`, `fcmp` et `cinc` sont entrelacés
   sur 8 points : le parallélisme d'instructions est déjà exploité au maximum
   disponible dans ce crate (explication de B1/B5 = spills) ;
3. la boucle tient dans ~24 registres sur 31 : il reste 7 registres
   architecturaux, ce qui interdit toute accumulation supplémentaire sans
   débordement (explication de B5/B6).

**Contrainte d'exactitude mesurée :** `fl(q/10⁶) ≠ fl(q·fl(1/10⁶))` pour
**305 678 valeurs de q sur 1 000 000** (vérification exhaustive en Python, bits
comparés via `struct.pack`). Le remplacement `fdiv → fmul` n'est donc pas une
 optimisation exacte, même si le compte final coïncide sur ce banc. Le test
 entier `qx²+qy² ≤ 10¹²` est mathématiquement exact mais coûte +220 µs mesurés
 (B6). Aucune optimisation arithmétique sûre ne reste.

**Plancher :** même en supposant une réécriture parfaite du noyau MC, le
meilleur jumeau Rust mesuré sur la même machine fait 205 µs pour MC : le gain
théorique maximal est ~21 µs, soit **5 % de `exec`**, et il est déjà au-delà de
la bande de bruit A/A mesurée (16 µs sur c6) pour une réécriture complète du
noyau — ce que la consigne ne demande pas (MC est « déjà ILP-8 »).

---

## 6. Blocage hors périmètre (complément)

| Poste | Fichier | Pourquoi inaccessible |
|---|---|---|
| Philox 97 µs | `run.rs` | possédé par un autre sous-agent (RUSTF l'a déjà réduit de ~21 µs) |
| Formatage 13 µs | `front.rs` (`texte_flottant`, boucle `p = 1..=17` avec `%.*e`) | hors fichier possédé |
| Crible, `tampon(65536)` | `run.rs` | hors fichier possédé |

Même en annulant **tout** le solde hors `interp.rs` (−97 −13 −~50 µs), le
plancher `exec` reste ~253 µs, déjà comparable aux ~300 µs du calcul Rust
**entier**, avant `parse + transform + jitnew` (~105 µs mesurés). La porte
« Rose ≤ Rust sur l'englobant » n'est pas fermée par une seule optimisation
dans `interp.rs`.

---

## 7. Validation (binaire A, état livré = état de départ)

```text
cargo build --release --bin rose                    rc=0
cargo test --release                                529 passed, 0 failed, 6 ignored
   (265 + 249 + 0 + 6 + 8 + 1 = 529 ; les 6 ignored sont les ignorés déclarés)
englobant 4096 1, c6/c16/c19, 9 tirages ×2          46297232.426252 / 564 /
                                                    157158 / 11899223598484
point fixe (2 voies consécutives)                   b3ed9262dd54776a6dbd07bc12368d0e ×2
   (rose rir compilateur/emetteur.rose | md5sum, stderr vide)
```

Le banc multi-domaines 9×9 (81/81) n'a pas été re-joué : aucun fichier source
n'étant modifié, il n'y a pas de sortie à comparer à un témoin. Le harnais
`/tmp/opencode/speed4_agents/bench_multi.py` reste disponible pour une
re-mesure si un changement est finalement livré.

---

## 8. État final factuel

- **Fait** : aucun fichier source du dépôt modifié ; seulement `rose/docs/RC2.md`
  ajouté. Aucun commit, aucune publication, aucun binaire installé.
- **Vérifié** : décomposition corrigée (MC 226, Philox 97, dispatch 3 µs),
  sept variantes mesurées en A/B entrelacé sur c6+c16+c19, bruit A/A quantifié,
  plancher matériel montré par désassemblage, 529 tests verts, 4 valeurs ×27
  tirages, point fixe ×2.
- **Non atteint** : réduction de `exec`. Aucune optimisation mesurée dans
  `interp.rs` ne bat la bande de bruit ; blocage documenté §5 et §6.
