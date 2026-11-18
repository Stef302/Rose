# Architecture du pont WebAssembly — optimisation structurelle pour bibliothèques externes

**Constat daté : 18/11/2026.** Une seule étape, un seul fichier moteur modifié :
`rose/src/wasm_lecteur.rs`. Aucun commit. `interp.rs` n'a PAS été modifié par
cette mission (un autre agent le modifie concurrently — voir §7).

Périmètre : **Rose appelle une bibliothèque compilée en wasm** (`@dll("wasm", …)`
/ `@sym(…)`), pas Rose compilé en wasm. Banc : `xxHash v0.8.2` (`libxxhash.wasm`,
6 901 o, `-O0 -nostdlib`, 0 import / 0 `start`) et `FNV-1a` (`libminihash.wasm`).

---

## 1. Protocole de mesure (pas de chiffre cité)

- Binaire A = **baseline** : binaire construit avec le `wasm_lecteur.rs` d'origine
  (`md5 f7aee35b…`) et l'`interp.rs` courant (`md5 80c9f739…` au moment de la
  mesure finale).
- Binaire B = **final** : idem avec le `wasm_lecteur.rs` optimisé (`md5 4d5d1d0e…`).
- Mesure interne : builtin `temps()` de Rose, 1 000 appels de
  `xxh32_str("abc", 3, 0)` (`/tmp/opencode/interop/bench.rose`).
- **A/B/A/B entrelacé, 6 paires = 12 échantillons par bras**, `taskset -c 15`
  puis `taskset -c 19`, médianes. Deux cœurs donnent le même résultat.

```
core 15 : A 9,324 µs/appel (n=12)  |  B 3,926 µs/appel (n=12)  → 2,375×
core 19 : A 9,323 µs/appel        |  B 3,925 µs/appel        → 2,375×
```

Médiane historique non pinée du doc `INTEROP_BIBLIOTHEQUE.md` : 15,7 µs/appel
(distribution bimodale 9,1 / 15,7 ms). Le pinage supprime le mode lent : la
baseline pinée est **9,32 µs**, valeur utilisée ici.

---

## 2. Décomposition du coût (instrumentation `Instant` temporaire, retirée)

Instrumentation ajoutée puis **supprimée** : compteurs `AtomicU64` autour de
`decouper()` et de `executer()`, cumulés sur la plage CHAUDE des appels 100→900
(800 appels de haut niveau), pour éviter les 100 premiers appels froids.

| Composante | Avant | Après | Note |
|---|---:|---:|---|
| Boucle Rose + (a) conversion Rose→wasm + (b) copie chaîne + (d) retour + verrou mutex global | ~1,11 µs | ~1,02 µs |dégradation marginale ; dominé par `interp.rs` (§7), hors périmètre |
| (c1) **décodage** `decouper()` par appel | **0,63 µs** | **0** | supprimé : décodage unique au chargement |
| (c2) **interprétation** des opcodes (`executer`) | **7,59 µs** | **2,90 µs** | ×2,62 sur le cœur de l'interpréteur |
| **Total par appel** | **9,32 µs** | **3,93 µs** | **×2,375** |

Le décodage n'était que 6,8 % du total : le gain ne vient donc **pas**
principalement du cache, mais de la suppression des clones/allocation par appel
et de la spécialisation du hot loop (§3).

### Profil d'opcodes (xxHash `-O0`, 800 appels, hot range)

`0x20 local.get` 381/appel · `0x21 local.set` 253 · `0x28 i32.load` 67 ·
`0x41 i32.const` 66 · `0x36 i32.store` 40 · `0x6A i32.add` 21 ·
`0x71 i32.and` 20 · `0x0D br_if` 15 · `0x45 i32.eqz` 14 · `0x6C i32.mul` 8…
≈ **940 opcodes/appel**, dont **67 % de `local.get`/`local.set`**.

---

## 3. Architecture AVANT

`InstanceWasm::appeler_index()` (avant, `wasm_lecteur.rs:720-769`) faisait,
**à chaque appel** — et 5 fois par appel xxHash, car la glue C appelle 4
fonctions internes :

1. `self.module.fonctions.get(idx).clone()` → **clone du `Fonction` entier**,
   y compris le `Vec<u8>` du corps (allocation) ;
2. re-lecture de l'en-tête des locales avec un `Lecteur` neuf ;
3. `decouper(brut)` → **re-décodage LEB128 / blocs / tables** avec allocation
   d'un `Vec<InstrWasm>` (24 o/instr), d'un `Vec<InfoBloc>`, d'un `Vec<Vec<usize>>` ;
4. `self.corps_len = …` (champ unique **écrasé par les appels imbriqués**) ;
5. `self.blocs.push(blocs)` / `self.tables.push(tables)` → **clones** des
   cibles dans des piles globales ; `pop()` au retour ;
6. `executer()` relisait ces cibles via `self.blocs.last()…clone()` et
   `self.tables.last()…clone()` ( encore des clones ), puis appelait
   `op_i32(op,g,d)` / `op_i64(…)` : **chaque opération arithmétique payait une
   chaîne de ~18 comparaisons d'opcodes**.

Coût structurel : 5 décodages + ~25 allocations + ~25 clones par appel utile.

## 4. Architecture APRÈS (une seuleamélioration transversale)

**« Décoder une fois au chargement, exécuter sur une représentation immuable
pré-décodée et spécialisée. »**

1. `Fonction` (`wasm_lecteur.rs:114-…`) gagne 4 champs de cache :
   `code: Vec<InstrWasm>`, `blocs_info: Vec<InfoBloc>`,
   `tables_info: Vec<Vec<usize>>`, `locales_types: Vec<TyWasm>`.
2. `charger()` (`wasm_lecteur.rs:374-…`) lit l'en-tête des locales, appelle
   `decouper()` **une seule fois par fonction du module** et remplit le cache.
3. `appeler_index()` (`wasm_lecteur.rs:778-…`) n'appelle plus `decouper()` :
   - emprunt via `Arc::clone(&self.module)` (incrément de refcount) **au lieu de
     `f.clone()`** : plus de copie du corps ;
   - `frame` pré-alloué (`Vec::with_capacity`), `corps_len` **par frame** (fini
     l'écrasement récursif) ;
   - `executer()` reçoit `&f.code`, `&f.blocs_info`, `&f.tables_info` : **plus
     aucune pile `self.blocs`/`self.tables`, plus aucun clone de cibles**.
4. `executer()` (`wasm_lecteur.rs:816-…`) : les opcodes arithmétiques chauds
   (`i32.add/sub/mul/and/or/xor/shl/shr_s/shr_u`, `i64` équivalents) sont
   calculés **en ligne** dans le `match`, avec repli sur `op_i32`/`op_i64`
   pour les opcodes rares : fin de la chaîne de 18 tests sur le chemin chaud.

Parts attribuables : suppression du décodage/clones/allocations ≈ **0,6 µs** ;
spécialisation + emprunts immuables + hot loop ≈ **4,8 µs** (le reste du gain
vient d'une meilleure localité et de moins de pressure allocateur).

### Tentatives REJETÉES (mesurées, non retenues)

- **Copie mémoire par tranche** (`get(..)` + `copy_from_slice` + `from_le_bytes`
  au lieu de la boucle octet par octet de `lire_memoire`/`ecrire_memoire`) :
  **7,13 µs/appel au lieu de 4,06 µs → régression ×1,75**. Revertée.
- **`get_unchecked` sur les locales** (validation des indices au chargement) :
  3,98 µs vs 3,91 µs pour la version sans → **pas de gain mesurable**,
  complexité unsafe inutile. Revertée.
- Le gain final retenu est donc **2,375×**, pas la somme optimiste des idées.

---

## 5. Gains réels — toutes bibliothèques confondues

| Bibliothèque | Avant | Après | Gain | Mesure |
|---|---:|---:|---:|---|
| xxHash `XXH32("abc")`, 3 args (dont 1 chaîne), 5 appels internes/call | 9,32 µs | 3,93 µs | **×2,375** | A/B entrelacé 12+12, cœurs 15 & 19 |
| FNV-1a `fnv1a_calc("abc", 3)`, 2 args, 1 fonction | 4,63 µs | 2,83 µs | **×1,64** | A/B entrelacé 12+12, cœur 15 |

Le gain est **structurel mais dépendant de la charge** : xxHash profite du
décodage de 5 fonctions par appel, FNV d'une seule. C'est attendu : on optimise
le coût *par fonction wasm appelée*, pas un cas spécifique.

### Oracles d'exactitude (identiques avant/après)

```
xxHash : 46947589  852579327  834093149  -396450594  1853483049   ✅ 5/5
FNV-1a : -2128831035  440920331  -309268332  3                 ✅
rose wasm libxxhash.wasm → 2 exports, 0 import, 0 start          ✅
jardin jardins/tests/premiers.rose → 20 tests, 0 échec           ✅
point fixe rir/compilateur → bdbef63681608a0b22a5637539e9d738   ✅ (×2)
oracles M4.1–M4.4 : tokens 19/19, ast 19/19, verif/rir 14/19     ✅ (= contrôle)
```

⚠ Les 5 écarts `verif`/`rir` portent sur `rose-cpp/tests/verifs/*.rose`
(fichiers **invalides**). **Preuve de non-régression** : résultat **identique**
sur le binaire baseline, sur le binaire final et sur le binaire témoin figé
`/tmp/opencode/rosebench/rose_ctl_v0`. L'échantillon du script historique était de
20 fichiers valides ; le corpus en contient désormais 19 avec cet `awk`.

---

## 6. Maison vs officiel — verdict honnête

- **Voie officielle disponible** : runner `~/.cache/rose/runtimes/wasmtime-v0.1.5`,
  SHA `c7b2bf3c2b85e72ec37192f2…` (runner publié). `rose wasm … --runtime
  officiel` → `fib(27) = 196418`, **6 échantillons pinés cœur 15 :
  3,380 / 2,581 / 2,535 / 2,379 / 2,709 / 2,290 ms, médiane 2,558 ms**, stderr
  vide (aucun repli). Re-mesuré le 18/11 (constat daté ; la valeur 2,885 ms de
  `WASM_RUNTIME.md` datait du même jour, bruit hôte).
- **Mais** ce runner est un wrapper CLI `wasmtime run --invoke export mod args` :
  il n'expose **ni** la mémoire linéaire de l'instance **ni** le pont
  `@dll("wasm")` de Rose. Le banc xxHash (pointeur de chaîne copié dans la
  mémoire linéaire) **ne peut pas** être mesuré par cette voie. Les 2,558 ms
  et les 3,93 µs ne sont donc **pas comparables** : l'un est un appel CLI
  process complet sur `fib`, l'autre un appel FFI intra-process avec copie.
- **Conséquence** : le runtime officiel reste la voie de compatibilité /
  d'exécution de modules wasm autonomes ; **le pont maison reste la seule voie
  d'interopérabilité bibliothèque ↔ Rose**, et c'est elle qui a été optimisée.

### Verdict vs natif / vs Rust

- Natif (même `xxhash.c`, gcc -O2, `bench_native` re-mesuré 6× piné cœur 15) :
  **2,14 ns/appel** (médiane ; 10⁷ appels). Le pont optimisé est à **3 926 ns**,
  soit **~1 834× plus lent** (avant : ~4 357×). **On ne prétend PAS battre le
  natif** : l'ordre de grandeur reste celui d'un interpréteur de compatibilité.
- Le bon réflexe performance reste la compilation RFC / le lien natif
  (`LINK_NATIF.md`), pas le pont wasm. Le gain de ×2,375 réduit le coût
  d'entrée/sortie du pont mais ne change pas la nature du compromis
  (portabilité zéro-dépendance contre vitesse).

---

## 7. Ce qui RESTE (hors périmètre, rapporté sans écraser)

`interp.rs` appartient à un autre agent ; **aucune ligne n'a été modifiée**.
Le résidual ~1,0 µs/appel est là, pas dans `wasm_lecteur.rs` :

| Ligne actuelle | Coût |
|---|---|
| `interp.rs:591` `static BIBLIOTHEQUES: Mutex<Vec<Entree>>` | verrou global pris **2 à 3× par appel** |
| `interp.rs:3320` `wf::memoire_taille(h)` | lock + recherche linéaire |
| `interp.rs:3332` `wf::ecrire_memoire(h, …)` | lock + recherche + `resize` |
| `interp.rs:3348` `wf::appeler(h, sym, &vals)` | lock + recherche + `export_fonction()` linéaire |
| `interp.rs:625/634/653` implémentations `wf::*` | mêmes locks |
| `interp.rs:3354-3377` lecture chaîne de retour octet par octet via `lire_octet()` | lock **par octet** |

Piste structurelle pour l'agent propriétaire : garder une référence directe à
l'`InstanceWasm` dans le `Signature`/la frame d'appel (poignée déjà résolue une
fois), supprimant 2-3 acquisitions de mutex et la recherche linéaire par appel.
Gain attendu : une part du ~1,0 µs résiduel, sans toucher à l'interpréteur.

### Limites conservées (pas réglées par cette étape)

1. **`-O0` obligatoire** : `-O1`/`-O2` produisent des modules exécutés faux
   (sauts/blocs imbriqués, pointeurs corrompus). La correction des cibles de
   blocs reste à faire ; hors périmètre.
2. **Types** : `entier64`/`i64` non exposé en source Rose (`XXH64` déclarable
   mais non appelable correctement) ; `entier` = i32 (valeurs `u32 > 2³¹`
   reviennent signées) ; scalaires + chaînes seulement.
3. **Boucles imbriquées / `call_indirect`** : non supportés ; `call_indirect`
   refusé explicitement. Les modules avec imports ou `start` sont refusés au
   chargement.
4. **Mémoire** : la chaîne Rose est copiée avec `len+1` octets seulement ;
   une fonction lisant des mots 4/8/16 octets au-delà du terminateur exige
   l'adaptateur C (`glue.c`).

---

## 8. Fichiers touchés / preuves

- **Modifié** : `rose/src/wasm_lecteur.rs` uniquement (128 insertions,voir
  `git diff --stat`). Aucun commit.
- **Non modifié** : `interp.rs`, `Cargo.toml`, `main.rs`, `lib.rs`, `PLAN.md`,
  `jit.rs`, `run.rs`, `rose/rose` (binaire partagé, en usage par un autre agent).
- **Créé** : ce document.
- Binaire de mesure : `/tmp/opencode/wasm_arch/rose_final_now`
  (`md5 15c34f5b…` au moment de la mesure A/B ; reconstruit ensuite depuis la
  source finale, `interp.rs` ayant continué à évoluer côté autre agent).
- Harnais reproductibles : `/tmp/opencode/wasm_arch/bench_ab.sh`,
  `/tmp/opencode/wasm_arch/oracles20_bin.sh`.
