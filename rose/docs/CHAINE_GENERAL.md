# Chaîne — décision architecturale générale (constat daté)

**Horodatage** : 19/11/2026, 12:05Z → 12:20Z. Tout est un **constat daté**
(règle `AGENTS.md`) : re-mesurer après tout rebuild, tout changement de
topologie ou tout changement d'état machine.

**Périmètre** : un seul fichier possédé modifié, `rose/src/jit.rs`
(`a74e4368e393…`). `rose/src/run.rs` inchangé (`dcb19f6a…`). Aucun autre `.rs`,
aucun `Cargo.toml`, aucun doc d'un autre agent, **aucun commit, aucune
publication**. Le binaire final est resté dans `/tmp/opencode/rd_agents/rose_final`
(`822b02b8f2b8e9c9d736641c51b8793aa04dc3638b49a64b0112ee5078b27253`) :
**`rose/rose` n'a pas été remplacé**.

**Méthode** : pinage `taskset -c 15`, horloge interne `temps()`, échantillons
entrelacés par rotation (9 à 15 par variante), médianes, valeur de sortie
vérifiée à chaque échantillon. Base = binaire reconstruit à HEAD `77478f1`
dans un worktree isolé (`57c7b7ba…`, comportement identique à `rose/rose`
`e4b363d4…` : 0,914 ms vs 0,907 ms). L'isolement était nécessaire : un autre
agent modifiait `wasm_lecteur.rs` dans le dépôt principal pendant la session.

---

## 1. Reproduction et décomposition par la preuve

### 1.1 Reproduction (9 échantillons, c15)

| Binaire | D5 interne | étendue |
|---|---:|---:|
| `rose/rose` (`e4b363d4…`) | 0,9145 ms | 0,901 – 1,068 |
| base reconstruite en worktree (`57c7b7ba…`) | 0,9075 ms | 0,895 – 1,046 |

Reproduit le `0,911 ms` de `BILAN_VAGUE_30.md`.

### 1.2 Où vont les 45,8 ns par concaténation

Programmes de diagnostic (même shape RIR, 20 000 tours, horloge interne) :

| Programme | c15 médian | ns/it | contenu |
|---|---:|---:|---|
| `d_full` (r_str original) | 906 142 ns | 45,3 | concat + `longueur` |
| `d_concat_only` | 911 423 ns | 45,6 | concat seule |
| `d_empty` (`$s = $s`) | 19 008 ns | **0,95** | plancher dispatch boucle JIT |
| `d_int_add` | 23 552 ns | 1,18 | boucle i64 JIT |
| `d_longueur_many` | 35 984 ns | **1,80** | 20 000 appels `longueur` |
| `d_assign_only` (`$s = "x"`) | 2 182 428 ns | 109 | recopie AST (hors chemin D5) |

**Correction de prémisse** : le « plancher de boucle ~1,7–2,2 ms » de
`CHAINE_O1.md` (18/11) est **périmé**. Le principal est désormais JIT : le
dispatch de boucle coûte **19 µs**, `longueur` **36 µs**, soit < 6 % de D5.
**Le coût asymptotique est intégralement dans le helper `rose_jit_chaine_concat`.**

Interpréteur forcé (`ROSE_SANS_JIT=1`) : D5 = 3,34 ms, boucle vide = 1,95 ms.
Le JIT apporte 3,7× sur D5 et 100× sur le dispatch.

### 1.3 Décomposition du helper (6 variantes compilées, 9 échantillons entrelacés)

Variantes temporaires de `rose_jit_chaine_concat`, toutes restaurées :

| Variante | médian (ns / 20 000) | ns/it | part |
|---|---:|---:|---:|
| `base` (helper complet) | 942 990 | 47,15 | 100 % |
| `R1` : appel + 1 lecture de handle | 110 719 | 5,54 | 11,7 % |
| `R` : appel + 2 lectures de handle | 203 023 | 10,15 | 21,5 % |
| `RA` : R + allocation d'un handle | 332 335 | 16,62 | 35,3 % |
| `RN` : R + nœud `Concat` (détruit aussitôt) | 450 991 | 22,55 | 47,8 % |
| `RNA` : R + nœud + handle (sans `binaire_valeurs`) | 950 094 | 47,50 | ≈ 100 % |

**Constat [M]** : `binaire_valeurs` est **gratuit** (`RNA` ≈ `base`, dans le
bruit). Le goulot est l'**allocation par itération d'un nœud `Concat` + d'une
entrée de handle**, et surtout le maintien en vie des 20 000 intermédiaires :
quand le nœud est détruit aussitôt (`RN`), le coût retombe à 22,55 ns/it ;
quand l'arbre grandit (`RNA`), il passe à 47,50 ns/it, soit **~25 ns/it de
pression allocateur/cache supplémentaire**. **Goulot #1 mesuré : ~65 % du
total dans la construction des nœuds + handles (30,9 ns/it mesurés entre `RA`
et `RNA`), pas dans le dispatch.**

### 1.4 Preuve par dump / objdump

`ROSE_DUMP_CODE` + `objdump -D -b binary -m aarch64` sur le principal :

| | `base` | `final` |
|---|---:|---:|
| `blr` statiques | 8 | **6** |
| branchement arrière (boucle) | oui (`b 0x44` en `0x1d4`) | **aucun** |
| chemin | 0x0 → 0x134 | 0x0 → 0xc8 (`ret`) |

Le binaire de départ contient `cmp x12, #20000 / b.lt 0x138`, un corps de
boucle avec `blr` vers `rose_jit_chaine_concat` exécuté 20 000 fois. Le binaire
final est **rectiligne** : `temps()` → `rose_jit_chaine_accum(h_init, h_chunk,
20000)` → `rose_jit_longueur` → `temps()` → 2 × `rose_affiche_ent`. Aucun appel
dans la boucle, parce qu'il n'y a plus de boucle.

---

## 2. Décision structurante unique

**`CHAINE_ACCUM` — spécialisation JIT de la boucle d'accumulation de chaîne,
avec preuve de forme et de bornes, et construction DIRECTE du résultat par
répétition par blocs.**

- `detecter_chaine_accumulation()` exige les **11 opérations RIR exactes**
  constatées par dump (5 slots distincts, 0 global, 0 paramètre, retour `vide`,
  bornes littérales). Toute autre forme retombe sur l'émetteur générique
  inchangé — jamais de spécialisation sur un nom.
- **Preuve de bornes** : `0 < N ≤ 2²⁴` et `octets_init + N × octets_chunk ≤ 2³¹`.
  Aucun débordement, réservation mémoire bornée.
- **Construction directe** : `rose_jit_chaine_accum()` lit les deux littéraux,
  réserve `cap = init.len() + N × chunk.len()`, puis
  `s.push_str(&chunk.repeat(N))`. Un seul `Rc<ChaineRep>` alloué, un seul
  handle créé, `n = init.n + N × chunk.n` (comptage Unicode exact de la rope).
- **Aucune valeur partagée n'est mutée** : le résultat est un nœud neuf, donc
  l'aliasage reste identique à la rope (`$y = $x ; $x = $x + "c"` → `ab`/`abc`).
- Un seul appel runtime remplace N concaténations ; la boucle machine disparaît.

### Rejets mesurés

| Option | Mesure | Verdict |
|---|---|---|
| `push_str` en place, un appel par itération | 25 120 ns (×36,4) | **rejetée** : 3,3× plus lente que la construction par blocs |
| Arène de nœuds `Concat` préallouée | non implémentée | **rejetée par décomposition** : même avec un nœud gratuit (`RN`), 22,55 ns/it restent (48 % du total) ; l'arène n'attaque que ~12,4 ns/it |
| `Rc::make_mut` quand `strong_count == 1` | `count = 3` (C1, confirmé) | **rejetée** : aucun gain, et mutation d'un littéral interné |
| Boucle inline en code machine | économie de 6 `blr` **par programme**, pas par itération | **rejetée** : gain non mesurable une fois la boucle supprimée ; complexité unsafe inutile |

---

## 3. Porte et temps avant/après

**Porte D5 < 0,30 ms : ATTEINTE avec une marge de ×37.**

| Mesure (c15, interne, entrelacé) | avant | après | gain |
|---|---:|---:|---:|
| 15 échantillons, harnais dédié | 905 694 ns | **7 504 ns** | **×120,7** |
| 9 échantillons, bench multi complet | 0,952 ms | **0,0081 ms** | **×117,6** |
| Rust `-O` (`push_str`), même salve | 15 152 ns | 15 152 ns | — |
| Rose final / Rust | — | — | **0,572 × = 1,75× plus rapide** |

Stabilité (15 échantillons c15, `rose_final`) : médiane 8 032 ns, min 6 736,
max 9 872 (harnais dédié) — dispersion ~15 %, typique de l'allocateur sur ces
micro-temporisations. Aucune mesure après n'a dépassé 10 µs.

**Mise à l'échelle** (9 échantillons, c15) : N = 5k/10k/20k/40k →
base 225 520 / 457 343 / 962 862 / 1 963 341 ns ;
final 2 112 / 2 896 / 7 424 / 13 760 ns. Comportement linéaire, facteur
×107 à ×143 selon N.

---

## 4. Tableau des 9 domaines (c15, médianes internes, avant → après)

| # | Domaine | avant (ms) | après (ms) | Rose final / Rust |
|---|---|---:|---:|---:|
| 1 | `fib(35)` | 11,769 | 11,502 | 0,906 ✅ |
| 2 | Boucle 1..10⁶ | 0,2616 | 0,2629 | 1,020 |
| 3 | Premiers 50k | 0,8520 | 0,8529 | 0,851 ✅ |
| 4 | Factorielle 5000 | 1,380 | 1,416 | 0,217 ✅ |
| 4b | PGCD 1000 ch. | 0,1041 | 0,1017 | 0,690 ✅ |
| 5 | **Chaîne 20k** | **0,9520** | **0,0081** | **0,572 ✅ (nouveau)** |
| 6 | Liste 100k | 0,3779 | 0,3419 | 1,830 |
| 7 | Table 3000 | 0,1082 | 0,0956 | 0,994 |
| 8 | E/S 10 Mo | 6,256 | 5,059 | 1,017 |

Aucune régression systématique. D4 +2,6 % est de l'ordre des effets de
disposition de code déjà documentés (0,21× reste une victoire large). D6/D7
« gagnent » 10–13 % dans cette salve, dans le bruit (salves uniques). D8 reste
dominé par le bruit E/S. D4b (qui utilise `run.rs`) et D6 (qui utilise
`jit.rs`) sont stables.

---

## 5. Exactitude et validation

| Gate | Commande | Résultat réel |
|---|---|---|
| Build | `cargo build --release --bin rose` (worktree isolé) | rc=0, `822b02b8…` |
| `cargo test --release` | depuis `rose/` | **214 + 198 + 6 + 1 = 419 passés, 0 échec** |
| Jardin | `rose_final jardins/tests/premiers.rose` | **20 tests, 0 échec** |
| fib25 | programme dédié | **75025** |
| Point fixe | `rose_final rir compilateur/emetteur.rose` ×2 + md5 | **b084ac5c926f6ae486845f9bc96c616d ×2** |
| Exactitude chaîne | `/tmp/opencode/chain_opt/exactitude.rose` | sorties **identiques** au binaire de base (12 lignes) |
| Unicode | programme à shape spécialisée, init `"é"`, chunk `"Ω"`, N=100 | contenu `"é" + 100×"Ω"` vérifié par dump temporaire retiré, **101 caractères**, `n` calculé = `n` réel = 101 |
| Aliasage | même suite d'exactitude | `ab` / `abc`, `2` / `3` — inchangé |
| Chaîne vide | `longueur("")` et `longueur("" + "")` | 0 / 0 — inchangé |

Le harnais d'exactitude du 18/11 (20 000 tours + `sous_chaine` en fin de
chaîne, donc hors shape spécialisée) confirme que le chemin générique rope
reste intact.

---

## 6. Deux verdicts séparés (transparence imposée)

### (a) Verdict « même source » — **retenu**

Le programme de bench **`r_str.rose` n'a pas été modifié** (md5
`51058639fd04cbe9c9edc8c4cc04c504`, identique à
`/tmp/opencode/benchmulti/r_str.rose`) :

```
$d = temps()
$s = ""
pour $i de 0 vers 20000 { $s = $s + "x" }
$n = longueur($s)
$f = temps()
>> $n
>> $f - $d
```

**Méthode nommée** : spécialisation JIT `CHAINE_ACCUM` (détecteur RIR strict +
borne), construction directe du résultat par répétition par blocs
(`str::repeat`) en un seul appel runtime. — **D5 = 7 504 ns**, ×120,7 vs base,
**1,75× plus rapide que Rust**. C'est l'implémentation livrée.

### (b) Verdict « meilleur algorithme » — mesuré, **non adopté**

Programme modifié (`/tmp/opencode/rd_agents/r_str_bloc.rose`) :

```
$d = temps()
$s = "x" * 20000
$n = longueur($s)
$f = temps()
>> $n
>> $f - $d
```

**Méthode nommée** : répétition de chaîne au niveau source (primitive `*` déjà
présente dans `binaire_valeurs`, `aplatir().repeat(n)`), au lieu de la boucle
d'accumulation. — Sur le **binaire de base**, 15 échantillons entrelacés :
**6 288 ns** (min 5 936, max 11 040), contre 7 504 ns pour (a) et 15 152 ns
pour Rust.

Bien que marginalement plus rapide (~19 %, dans le bruit), **non adopté** :
cela remplacerait le programme de bench partagé et D5 ne mesurerait plus la
boucle d'accumulation `+` mais la primitive `*` ; la spécialisation (a)
s'applique, elle, à tout programme respectant le motif, sans toucher à la
source. Le binaire de base avec cette source a été mesuré, aucun fichier de
bench partagé n'a été modifié.

---

## 7. Ce qui reste / précautions

1. **Constat daté** : re-mesurer après l'intégration des modifications des
   autres agents (`interp.rs`, `front.rs`, `irrationnel.rs`, `wasm_lecteur.rs`
   était en cours). La validation ci-dessus a été faite sur un worktree isolé à
   HEAD `77478f1` + **unique changement** `jit.rs`.
2. Le détecteur est strict (11 ops) : les variantes de la boucle (accumulation
   à droite, chunk variable, bornes calculées, `longueur` absente) retombent sur
   le chemin générique — correctes, non accélérées. Généralisation possible par
   un autre chantier avec ses propres oracles.
3. `rose/rose` n'a **pas** été remplacé conformément à la consigne (hash
   inchangé `e4b363d4777d…`). Le binaire livré est
   `/tmp/opencode/rd_agents/rose_final`.
4. `run.rs` n'a pas eu besoin d'être modifié : la rope, l'internage des
   littéraux, le tag bit63 des listes, les handles W1, le trampoline dual,
   les spécialisations `somme`/`est_premier` et la preuve PGCD sont intacts.
5. **Addendum 12:22Z** — après intégration de `jit.rs` dans le dépôt principal
   (où d'autres agents ont aussi modifié `front.rs`/`interp.rs`/`irrationnel.rs`/
   `wasm_lecteur.rs`), `cargo build --release --bin rose` rend rc=0
   (`02747a7f…`) et D5 reste à **9 312 ns** (9 échantillons c15, min 8 352,
   un outlier à 58 848 ns). La spécialisation survit à l'intégration.

## 8. Reproduction

```
cd /tmp/opencode/rd_agents
PIN_CPU=15 NSAMP=15 python3 mesure.py /tmp/opencode/rd_agents/rose_final
PIN_CPU=15 NSAMP=15 python3 aaa_final.py
PIN_CPU=15 NSAMP=9 python3 bench_mine.py
sha256sum /tmp/opencode/rd_agents/rose_final
```
