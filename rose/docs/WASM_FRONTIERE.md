# Frontière du pont WebAssembly — vague 33 — **constat daté 19/11/2026**

Dépôt `Stef302/Rose`, HEAD de départ `0f0f58d412f696c3bca9cfb97127b53c95ce970d`,
Rose **0.1.6**. Aucun commit, aucune publication. Binaire construit dans
`/tmp/opencode/se3_agents/rose` par `cargo build --release --bin rose` puis copié :
**`rose/rose` n'a PAS été remplacé**. Seuls deux fichiers sources ont été
touchés : `rose/src/wasm_lecteur.rs`, `scripts/rose-wasm-lib.sh`, plus ce
document (le document de référence de la vague 32, `WASM_UNIVERSEL.md`, n'a PAS
été modifié : il appartient à un autre agent).

> ⚠ **Verdict honnête en une ligne** : la brique « `malloc` externe » est
> levée — une vraie bibliothèque C qui alloue elle-même (**uthash**) s'exécute
> maintenant depuis Rose, avec 10/10. Ce n'est **toujours pas** « n'importe
> quelle bibliothèque » : la frontière exacte est en §5. `free` est un no-op
> (fuite bornée), pas un vrai recycleur.

---

## 1. Brique choisie et levée : **`malloc` / `calloc` / `realloc` / `free`**

### 1.1 Pourquoi celle-ci

La frontière de la vague 32 (`WASM_UNIVERSEL.md` §6) disait textuellement :

> `malloc`/libc complète, `printf` arbitraire — Seuls les hôtes listés en §1.5
> existent. Pour miniz nous avons dû fournir un allocateur bump dans l'adapteur.

C'était le bloqueur le plus coûteux en pratique : toute bibliothèque C qui
appelle `malloc` elle-même devait être réécrite ou fournir son propre
allocateur dans l'adapteur. Le convertisseur compile avec `-nostdlib` ; un appel
à `malloc` devient un import `env.malloc` non résolu, et le chargement échoue.

### 1.2 Ce qui a été ajouté au pont (`wasm_lecteur.rs`)

* Cinq hôtes dans `HOTES_INTEGRES` (namespace `env`, celui que `wasm-ld` utilise
  pour les symboles C non définis) : `malloc`, `calloc`, `realloc`, `free`,
  `exit`.
* `InstanceWasm.tas: Option<(prochain_libre, fin_de_zone, début_de_zone)>` :
  zone de tas créée **au-delà** de la mémoire initiale du module. C'est ce qui
  garantit l'absence de collision avec ses données et sa pile. Mesure sur le
  module uthash `-O2` : `__stack_pointer = 66576` dans 2 pages (131072 octets),
  donc le tas démarre à 131072 — très au-dessus.
* `tas_allouer()` : alignement 16 octets, extension par pages de 64 Kio avec
  1 Mio d'amortissement (comme `memory.grow`), **plafond sandboxé de 32 Mio** :
  au-delà, `malloc` rend `NULL` (0) au lieu d'allouer sans limite.
* `calloc` zéro explicitement la zone ; `realloc` copie l'ancien contenu ;
  `free` est un **no-op documenté** ; `env.exit` (assert/abort C, importé par
  uthash) rend une **erreur propre**, jamais `std::process::exit`.

### 1.3 Conversion (`rose-wasm-lib.sh`)

Nouvelle option `--malloc` : ajoute `-Wl,--allow-undefined` (symboles C non
définis → imports), force `--allow-imports`, et ajoute les cinq hôtes au miroir
Python de vérification. Message dédié :

```
==> --malloc: allocateur sandboxe env.malloc/calloc/realloc/free (tas incremental, plafond 32 Mio, free=no-op).
```

---

## 2. Preuves — bibliothèque réelle supplémentaire : **uthash 1.9+**

Source officielle téléchargée : `uthash.h` (Troy D. Hanson, licence BSD,
73 749 octets, `https://raw.githubusercontent.com/troydhanson/uthash/master/src/uthash.h`).
Adapteur fin `uthash_adapt.c` : `HASH_ADD_INT`, `HASH_FIND_INT`, `HASH_DEL`,
`HASH_COUNT`, `HASH_ITER`. Ce n'est **pas** une bibliothèque écrite pour le test :
c'est uthash, table de hachage utilisée en production, qui appelle `malloc`/
`calloc`/`free` en interne.

### 2.1 Compilation et vérification

```sh
cd /tmp/opencode/se3_agents/uthash
bash /home/clausake/Stefan/ROSE/scripts/rose-wasm-lib.sh \
  --export 'rw_uth_insert=rw_uth_insert(entier,entier)->entier' \
  --export 'rw_uth_find=rw_uth_find(entier)->entier' \
  --export 'rw_uth_count=rw_uth_count()->entier' \
  --export 'rw_uth_sum=rw_uth_sum()->entier' \
  --export 'rw_uth_delete=rw_uth_delete(entier)->entier' \
  --malloc --opt 2 --out libuthash_O2 uthash_adapt.c
```

```
WASM_CHECK imports=4 start=0 call_indirect=0 memory_exported=yes
imports_liste=env.malloc(kind=0),env.calloc(kind=0),env.free(kind=0),env.exit(kind=0)
==> OK: ./libuthash_O2.wasm  (2834 octets)
```

### 2.2 LE test décisif : ancien binaire vs nouveau binaire

**Ancien binaire** (`/tmp/opencode/rb_agents/rose`, vague 32) — le module est
refusé au chargement :

```
Rose : erreur — appel WebAssembly : module WebAssembly invalide (…/libuthash_O2.wasm) :
import non résolu : env.malloc — hôtes disponibles : env.log, env.ecrire, …
```

**Nouveau binaire** (`/tmp/opencode/se3_agents/rose`) — même module, même démo :

```text
$ rose demo_uthash.rose
OK insert 10
OK insert 20
OK insert 30
OK count=3
OK find 20=200
OK sum=600
OK delete 20
OK find 20 absent
OK count=2
OK sum=400
TOTAL: 10 OK, 0 ECHEC
```

### 2.3 Sonde directe des quatre hôtes

Module C compilé `-O2` (`malloc_probe.c`) vérifiant : somme `0..63` = 2016 après
`malloc(64)` ; `calloc(8,4)` tout à zéro puis somme 36 ; `realloc(p,128)` qui
préserve les 64 premiers octets **et** laisse l'autre bloc intact ; `free` sur
les trois pointeurs.

```text
$ rose demo_malloc_probe.rose
OK malloc/calloc/realloc/free probe = 12345
```

### 2.4 Borne du tas (preuve du plafond, pas une affirmation)

Module `-O0` (à `-O2` clang élimine le test de NULL via les hypothèses sur
`malloc`, mesuré) qui alloue des blocs de 1 Mio jusqu'à NULL :

```text
$ rose demo_cap_O0.rose
blocs 1 Mio alloues avant NULL: 32
```

→ exactement `TAS_PLAFOND = 32 Mio`. Le sandbox tient.

### 2.5 Stress 400 entrées (croissance du tas)

`uthash_stress.c` : 400 insertions, vérification des 400 `find`, suppression de
200 entrées (`HASH_DEL` + `free`), vérification de la somme des valeurs restantes.

```text
$ rose demo_stress.rose
OK stress uthash 400 entrees = 424242
```

---

## 3. Ce qui reste bloqué (mesuré/connu, pas « universel »)

| Limite | État exact après la vague 33 |
|---|---|
| `free` | **No-op** : les blocs ne sont pas réclamés. Cycles infinis → NULL à 32 Mio (mesuré). |
| libc complète, `printf` variadique | Toujours absente. Seuls `malloc`/`calloc`/`realloc`/`free`/`exit` ont été ajoutés. |
| Python, Go, JS, Java, C# | Exigent un runtime/GC importé. Le convertisseur les refuse explicitement. |
| Exceptions C++ / `panic=unwind` | `-fno-exceptions -fno-rtti`, `-C panic=abort` forcés. |
| SIMD `v128`, threads/`atomics`, `externref` | Opcodes/types hors sous-ensemble, refusés avec message. |
| Tables multiples, `call_indirect` sur table non nulle | Une seule table (index 0). |
| ABI structurées / variadiques | Le pont ne convertit que i32/i64/f32/f64 et pointeurs `chaine` (structs par valeur non supportées). |
| `path_*`/`fd_prestat_*` | errno explicites (ENOTCAPABLE 76 / EBADF 8) : aucun disque, aucun réseau. |

---

## 4. Validation (binaire `/tmp/opencode/se3_agents/rose`)

| Contrôle | Sortie mesurée |
|---|---|
| `cargo build --release --bin rose` | `Finished release [optimized]`, 0 erreur |
| `cargo test --release` | `220 + 204 + 0 + 6 + 1 + 0 = 431 passed, 0 failed` (3+3+0+0+0+0 ignorés) |
| xxHash `-O1` / `-O2` (`""`, `"abc"`) | `46947589`, `852579327` (identiques dans les deux cas) |
| FNV-1a (`""`, `"abc"`, `"Hello, world!"`) | `-2128831035`, `440920331`, `-309268332` |
| miniz 11.3.2 `-O2` | `TOTAL: 7 OK, 0 ECHEC` |
| LZ4 1.10.0 `-O2` | `TOTAL: 5 OK, 0 ECHEC` |
| jardin `jardins/tests/premiers.rose` | `20 tests, 0 échec` |
| point fixe `rose rir compilateur/emetteur.rose \| md5sum` | `b084ac5c926f6ae486845f9bc96c616d` |
| point fixe `rose compilateur/emetteur.rose compilateur/emetteur.rose \| md5sum` | `b084ac5c926f6ae486845f9bc96c616d` (**égalité ✓**) |
| `node web/test-wasm.mjs rose.wasm` | `Tous les tests passent.` (15 contrôles, dont parité native) |
| uthash `-O2` (nouveau) | `TOTAL: 10 OK, 0 ECHEC` |
| uthash `-O2` (ancien) | `import non résolu : env.malloc` (échec au chargement) |

⚠ Constat daté : pendant une partie du chantier, `cargo test --release` ne
compilait pas à cause d'une modification concurrente de `rose/src/jit.rs`
(erreur d'emprunt en `jit.rs:3764`) — fichier hors de mon périmètre. Cette
modification a été corrigée par son auteur ; le résultat 431/0 ci-dessus est
re-mesuré **après** correction.

---

## 5. Frontière mise à jour — « n'importe quelle bibliothèque » reste **FAUX**

**Supporté** (vague 32 + vague 33, preuves ci-dessus et dans `WASM_UNIVERSEL.md`) :

* C/C++/Rust/WAT/Zig compilés en wasm32 autonome, y compris `-O1`/`-O2` ;
* `call_indirect` (table 0 + vérification de signature), section `start`
  bornée, imports fonction/mémoire/table/global, budget d'instructions ;
* 56 hôtes WASI (27 × 2 namespaces) + `env.log`/`env.ecrire` +
  **5 hôtes d'allocation sandboxés** (`env.malloc/calloc/realloc/free/exit`) ;
* cinq vraies bibliothèques exploitées : miniz (7/7), LZ4 (5/5),
  MurmurHash3 (3/3), xxHash, **uthash (10/10 + stress 400)**.

**Nouvelle formulation exacte** : *toute bibliothèque dont le calcul est
réductible à des fonctions à arguments scalaires/chaînes, compilable en wasm32
autonome, est exploitable même avec `-O2` — y compris si elle appelle
`malloc`/`calloc`/`realloc`/`free` en interne (allocation sandboxée, plafond
32 Mio, `free` sans réclamation).*

**Restent hors périmètre** : runtimes de langage (Python/Go/JS/Java/C#),
exceptions/unwind, libc complète et `printf` variadique, SIMD `v128`, threads,
`externref`, tables multiples, ABI structurées/variadiques, accès disque/réel
système de fichiers. Le mot « universel » reste donc **inexact**.

---

## 6. Fichiers livrés

* `rose/src/wasm_lecteur.rs` — 5 hôtes + `InstanceWasm.tas` + `tas_allouer()`
  (137 lignes ajoutées) ; `rose/rose` **non remplacé**.
* `scripts/rose-wasm-lib.sh` — option `--malloc`, miroir des hôtes, messages
  (20 lignes modifiées).
* `rose/docs/WASM_FRONTIERE.md` — ce document.
* Matériel d'expérience (non versionné) : `/tmp/opencode/se3_agents/`
  (`rose`, `uthash/` + `uthash.h`, `malloc_probe.c`, `malloc_cap.c`,
  `fnv_adapt.c`, démos).
