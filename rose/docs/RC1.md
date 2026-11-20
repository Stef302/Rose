# RC1 — carrés & flottants du banc global (CONSTAT DATÉ, 20/11/2026)

Sous-agent `rc1`. **Aucun commit, aucune publication, aucun `rose/rose` du dépôt
remplacé.** Périmètre exclusif : **`rose/src/jit.rs` (+71/−14 lignes)** et ce
document. Aucun autre fichier source n'a été modifié par moi. Les binaires ont
été construits dans `/tmp/opencode/rc1_agents/` avec `CARGO_TARGET_DIR` isolé.
`rose/src/front.rs` et `thorn/docs/*` portent des modifications concurrentes
d'autres agents : je n'y ai pas touché.

> **Verdict en une ligne.** Le poste **flottants (517 µs) est au plancher
> matériel** — preuve par micro-jumeau ci-dessous, aucun code livré. Le poste
> **carrés (541 µs) a une marge réelle de ×1,29** (noyau 16 accumulateurs
> au lieu de 8), mais **le noyau actif du banc global est dans `interp.rs`
> (`sp5_carres_acc`), hors de mon périmètre** : mon optimisation du noyau
> `jit.rs` est livrée et prouvée en micro-jumeau, mais elle est **bypassée**
> par le dispatch, donc sans effet sur le global. Ce fait est la découverte
> centrale de cette mission (§2).

---

## 0. Protocole réellement exécuté

| Élément | Mesure |
|---|---|
| Binaire témoin A | `/tmp/opencode/rc1_agents/rose_base`, SHA256 `a22a8bd86efdd1fe62c73361d5977bd73a0f5bbe4ba9aede35711b54bb2ef4b4` |
| Binaire final B | `/tmp/opencode/rc1_agents/rose_cand` (construit depuis le même arbre figé, seul `jit.rs` diffère) |
| Arbres figés | `/tmp/opencode/rc1_agents/src_base` et `src_cand` (copies de `rose/`, `target` exclu) |
| Pinage | `taskset -c 6` **et** `-c 16`, capacités 997/1017 re-vérifiées ; CPU = Cortex-X925 (c16) / Cortex-A725 (c6), aarch64 + NEON + SVE2 |
| Échantillons | micro-jumeaux **13 tirs** par noyau et par cœur ; global **A/A/A 12 tirs** entrelacés `A/B/B/A` puis `B/A/A/B` avec rotations appariées (`aaa.py`) |
| Horloges | `Instant::now()` (micro-jumeaux Rust), `temps()` ns (profil Rose), `millisecondes()` + mur `perf_counter_ns` (global) |
| Valeurs | 11 lignes du global vérifiées **à chaque tirage** ; 81/81 ×2 cœurs ; point fixe ×2 ; voir §6 |
| Fenêtre | `2026-11-20T22:00Z` → `22:15Z`. Tout chiffre est un **constat daté**, pas une consigne |

Harnais : `/tmp/opencode/rc1_agents/aaa.py`, `validate_multi_rc1.py`,
`micro_carres.rs`, `micro_carres2.rs`, `micro_flottants.rs`, `micro_floor.rs`.
Banc instrumenté copié hors dépôt : `run/prof.rose` (structure identique à
`bench/global/bench_global.rose`, lectures `temps()` insérées **autour** des
boucles, sans toucher à leur corps ni à leur détecteur). **Aucun `.rose` du
dépôt n'a été modifié.**

---

## 1. Mesure de départ (profil Rose, médianes, ns)

| Phase | c16 | c6 |
|---|---:|---:|
| carrés (10⁷ produits) | 542 159 | ~541 000 |
| flottants (10⁶ `x += c/b`) | 518 192 | ~517 000 |

Ces chiffres reproduisent ceux du constat SP9 (540 710 / 517 143 ns c16).

---

## 2. Découverte centrale — le dispatch (sonde `eprintln!` temporaire, retirée)

Une sonde insérée dans les 5 points d'entrée candidats (`jit.rs::carres_acc_neon`,
`carres_acc_natif`, `emettre_carres_acc`, `flottants_acc_natif`, et
`interp.rs::sp5_carres_acc`), compilée dans un binaire jetable
(`rose_probe`), donne sur **une exécution de `prof.rose`** :

```
HIT flottants_acc_natif
HIT sp5_carres_acc 1 10000001
```

**Conséquence** : dans le banc global, la boucle de carrés est servie par
`interp.rs::sp5_carres_acc` (8 accumulateurs, ajouté par SPEED5), **pas** par
`jit.rs::carres_acc_neon`. Ce dernier est contourné par le test
`sp5_carres_plan` dans `interp.rs::appel_rir_direct` (l.3894-3901) : dès que le
plan RIR correspond — ce qui est le cas ici — c'est le noyau 8 accumulateurs
qui tourne, et la closure native installée par `jit.rs` est ignorée. Un
programme de fonction explicite (`fn somme_carres(acc0) { … } >> somme_carres(0)`)
confirmé : **aucun** `HIT` côté `jit.rs`, le noyau `jit.rs` est effectivement
du code mort sur tous les bancs présents.

Les flottants, eux, passent bien par `jit.rs::flottants_acc_natif` : c'est mon
périmètre, et ils sont au plancher (§4).

---

## 3. Optimisation livrée dans `jit.rs` — noyau de carrés 16 accumulateurs

`carres_acc_neon` passe de **4** à **16** accumulateurs `uint64x2_t` et de
**16** à **64** éléments par tour, avec un vecteur de comptage maintenu dans un
registre (`vaddq_u32`) au lieu de 4 à 8 rechargements depuis un tableau
temporaire sur la pile. Aucun nom de fonction, aucune borne du banc, aucune
valeur de résultat codée en dur : les bornes viennent du détecteur
`detecter_carres_acc` (`fin ≤ 2²⁴`), les `fin-debut` carrés sont calculés un à
un par paquets SIMD.

### 3.1 Micro-jumeau (`rustc -O`, 10⁷ termes, 13 tirs, médianes, ns)

| Noyau | c16 | c6 | vs v4 |
|---|---:|---:|---:|
| v4 — 4 accumulateurs + rechargements pile (code d'origine) | 710 560 | ~708 000 | 1,00 |
| v8 — 8 accumulateurs + rechargements pile (`sp5`) | 538 720 | ~537 000 | ×1,32 |
| v8inc — 8 accumulateurs + vecteur incrémenté | 455 264 | ~453 000 | ×1,56 |
| **v16inc — 16 accumulateurs + vecteur incrémenté (livré)** | **418 799** | **~419 000** | **×1,70** |
| v8inc_inter — 16 accum. mais entrelacement différent | 455 888 | ~456 000 | ×1,56 (rejeté) |

Les deux leviers sont indépendants et additifs : le passage 4→8 accumulateurs
donne ×1,32 (chaîne de dépendance `umlal` divisée par deux) ; la suppression
des allusions mémoire sur le vecteur de comptage donne un gain supplémentaire
de ×1,18 (538 720 → 455 264 ns). Le passage 8→16 donne ×1,09 de plus.
Tous les noyaux rendent **exactement** `333333383333335000000` (i128).

### 3.2 Preuve d'exactitude

* `a ≤ fin ≤ 2²⁴` ⇒ `a` tient en `u32` et `a² ≤ 2⁴⁸` : `vmlal_u32` fait un
  produit élargi u32×u32→u64 **sans arrondi ni réassociation**.
* 16 registres × 2 voies × 2 produits/tour × `FLUSH = 8192` tours = 2¹⁴
  termes par voie ⇒ `voie ≤ 2⁴⁸ · 2¹⁴ = 2⁶² < 2⁶⁴` : aucun débordement u64.
* Le vidage additionne les 32 voies en `i128`. L'addition entière est
  associative : le regroupement n'affecte pas le résultat bit à bit.
* La queue scalaire traite les < 64 derniers termes un à un.
* Aucune formule fermée (pas de `n(n+1)(2n+1)/6`), aucun résultat codé en dur.
  Le jumeau LLVM qui fait 7,7 µs par forme fermée reste **interdit et non
  adopté**.

### 3.3 Effet réel sur le global — AUCUN (attendu, expliqué)

A/A/A 12 tirs c6+c16, `prof.rose`, `B − A` apparié (médianes) :

```
c16 CARRES   ns: A 562232  B 567720  B-A apparié +1440  (bruit)
c6  CARRES   ns: A 547272  B 547824  B-A apparié +2624  (bruit)
c16 FLOTTANT ns: A 529256  B 519880  B-A apparié  -880  (bruit)
c6  FLOTTANT ns: A 515440  B 517535  B-A apparié   -80  (bruit)
```

Global complet (9 tirs, 11 valeurs vérifiées à chaque tirage) :

```
c16 mur: A 4.615 ms  B 4.411 ms  B-A apparié -0.036 ms
c6  mur: A 4.203 ms  B 4.017 ms  B-A apparié -0.201 ms
```

Écart dans la bande de bruit (A/A typique ±2 à ±4 % sur ce banc). Cause : §2.
**Je ne revendique donc aucun gain sur le global pour ce changement** : il
améliore le noyau de repli `jit.rs`, mesuré en micro-jumeau, et devient actif
dès que le dispatch n'intercepte plus (autre plateforme, autre plan RIR).

---

## 4. Flottants — plancher matériel PROUVÉ (aucun code livré)

Micro-jumeau `rustc -O`, 10⁶ itérations `x += 1.0/b`, c16, 13 tirs, médianes
(ns) — résultats bit-pour-bit identiques (`bits = 4624292002893000918`,
`14.392726722864989`) pour **toutes** les variantes :

| Variante | c16 | c6 | Verdict |
|---|---:|---:|---|
| `x += c/b` scalaire (noyau livré) | 514 176 | ~514 000 | référence |
| divisions NEON 2 voies (`vdivq_f64`) | 514 112 | ~515 000 | **aucun gain** |
| divisions NEON 4 voies | 514 367 | ~515 000 | **aucun gain** |
| **chaîne d'additions seule** (`x += constante`) | **514 160** | ~514 000 | **identique** |

La division n'est **pas** le goulot : elle est entièrement recouverte par
l'exécution hors-ordre (une boucle qui ne garde que la dernière division
s'effondre à 32 ns — LLVM la réduit à une seule). Le coût exclusif est la
**latence de la chaîne de dépendance `fadd`** : chaque `x` dépend du précédent.
Vectoriser les divisions ne change rien, et utiliser plusieurs accumulateurs
ou réassocier les additions **changerait le résultat IEEE** (interdit : mêmes
valeurs de sortie, 11 valeurs global identiques).

Le plancher est donc atteint et démontré, sans formule fermée ni valeur codée
en dur. Aucune modification de `flottants_acc_natif` n'est livrée ; seul un
commentaire documentant la preuve a été ajouté.

---

## 5. Correctif suggéré au propriétaire de `interp.rs` (NON appliqué — hors périmètre)

Pour que le gain ×1,29 profite au global, il faut remplacer le corps de
`interp.rs::sp5_carres_neon` (l.223-273, 8 accumulateurs) par le noyau
**v16inc** déjà validé dans `jit.rs` (§3) : même signature
`unsafe fn(debut, fin, acc0) -> i128`, mêmes bornes (`fin ≤ 2²⁴`), même détecteur
`sp5_carres_plan` inchangé. Gain attendu mesuré en micro-jumeau :
**~542 µs → ~420 µs c16/c6, soit −120 µs sur le global** (~5 % du temps
interne). Je n'ai **pas** appliqué ce patch : `interp.rs` n'est pas mon fichier.
Le code exact est celui de `rose/src/jit.rs::carres_acc_neon` (l.750-820).

---

## 6. Validation (preuves de sortie réelles, binaire final `rose_cand`)

| Gate | Résultat |
|---|---|
| `cargo build --release --bin rose` (target isolé) | **OK** |
| `cargo test --release` (target isolé) | **529 passed, 0 failed** (265+249+6+8+1), 6 ignored |
| Jardin `jardins/tests/premiers.rose` | inchangé, aucun `.rose` du dépôt modifié |
| `fib(25)` | `75025` |
| Point fixe (2 voies) | `b3ed9262dd54776a6dbd07bc12368d0e` **×2** (`rir emetteur.rose` et `emetteur.rose emetteur.rose`) |
| 9 domaines × 9 tirages c6 **et** c16 | **81/81 + 81/81 = 162/162** sorties octet pour octet identiques au témoin |
| `bench_global.rose` | **rc=0**, 12 lignes, **11 valeurs identiques** vérifiées à chaque tirage (9+9 tirs A/A/A) |
| Différentiel noyaux SIMD | 4 variantes de carrés et 3 de flottants : i128 / bits f64 **identiques** |
| Périmètre | `git diff` : seul `rose/src/jit.rs` (71 insertions) ; aucun commit, aucune publication |

---

## 7. État final factuel

- **Fait** : noyau `jit.rs::carres_acc_neon` réécrit en 16 accumulateurs NEON +
  vecteur incrémenté (gain micro-jumeau ×1,70 vs l'ancien noyau 4 acc.) ;
  preuve documentée du plancher des flottants ; découverte du dispatch réel
  (carrés = `interp.rs`, flottants = `jit.rs`).
- **Vérifié** : build, 529 tests, 162/162 multi, point fixe ×2, fib25,
  11 valeurs global inchangées, A/A/A 12 tirs c6+c16, 7 variantes SIMD
  bit-exactes.
- **Non atteint** : réduction des 541 µs **sur le global** — le noyau actif est
  dans `interp.rs`, hors périmètre ; le correctif exact est fourni §5.
- **Aucun commit, aucune publication, aucun binaire installé dans `rose/rose`.**
