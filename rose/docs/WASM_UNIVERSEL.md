# Pont WebAssembly « universel » — vague 32 — **constat daté 19/11/2026**

Dépôt `Stef302/Rose`, HEAD de départ `77478f1`, Rose 0.1.5. Aucun commit,
aucune publication. Binaire construit dans `/tmp/opencode/rb_agents/rose`
par `cargo build --release --bin rose` puis copié : **`rose/rose` n'a PAS
été remplacé**. Seuls trois fichiers ont été touchés :
`rose/src/wasm_lecteur.rs`, `scripts/rose-wasm-lib.sh`, ce document.

> ⚠ **Verdict honnête en une ligne** : la chaîne sait maintenant exécuter
> des bibliothèques C/C++ **compilées avec optimisation** (`-O1`/`-O2`),
> y compris avec `call_indirect` et `start`. Ce n'est **toujours pas**
> « n'importe quelle bibliothèque » : la frontière exacte est en §6.

---

## 1. Ce qui a été ajouté au pont (`wasm_lecteur.rs`)

### 1.1 `call_indirect` — le bloqueur n°1 — ✅ supporté et vérifié

* Le décodeur (`decouper`, opcode `0x11`) **conserve** les deux immédiats
  au lieu de les jeter : `imm` = index du type attendu, `aux` = index de la
  table. Auparavant les deux étaient lus puis `discard` → résolution impossible.
* `ModuleWasm` acquiert `table_min` ; la section 4 et les imports de table
  dimensionnent la table ; les segments `elem` (section 9) la remplissent
  dans `InstanceWasm::nouveau`.
* À l'exécution (`0x11`) : dépil de l'index de table, résolution dans la
  table de l'**instance**, **vérification stricte de la signature** contre
  `types[type_idx]` (trap propre si écart), dépil des arguments selon le
  type, appel de la fonction résolue, repoussement des résultats.
* **Preuve** : miniz 11.3.2 `-O2` compiled `WASM_CHECK … call_indirect=5` ;
  le module est chargé et les 5 appels indirects s'exécutent dans le cycle
  compression/décompression complet (§3.1). L'ancien binaire refuse le module.

### 1.2 Section `start` — ✅ supportée, bornée, vérifiée

* La section 8 lit l'index de fonction (un seul u32, pas un compte) et le
  stocke dans `ModuleWasm.start`.
* `InstanceWasm::nouveau` exécute cette fonction **au chargement**, avec un
  budget dédié `BUDGET_START = 20_000_000` instructions. Le pont ne pouvant
  pas changer la signature de `nouveau`, une erreur est mémorisée dans
  `start_erreur` et surfaceée par le premier `appeler`.
* **Preuve** (module bâti à la main, `make_start.py`) : la fonction `start`
  pose un global à `1234` ; `get()` rend `1234` avec le nouveau binaire,
  et l'ancien binaire échoue avec
  `section de départ (start) : non supportée`.
* **Preuve de la borne** (`make_loop.py`, `start` = `loop; br 0; end`) :

  ```
  Rose : erreur — appel WebAssembly : … : section `start` du module wasm :
  budget d'instructions wasm épuisé (boucle infinie ou calcul trop long)
  elapsed=164 ms
  ```

### 1.3 Budget d'instructions — ✅ garde-fou anti-boucle infinie

`BUDGET_APPEL = 500_000_000` instructions par appel public, partagé par les
appels imbriqués ; `BUDGET_START = 20_000_000`. Décrémenté dans la boucle
d'exécution. Mesuré ci-dessus sur une boucle infinie.

### 1.4 Imports de globals / table — ✅ acceptés (au lieu d'un refus)

* `GenreImport::Table { min, max }` : la table est allouée par Rose ;
* `GenreImport::Global(t, mut)` : le global importé est **prépendu** à
  `m.globals` (l'espace des globals compte les imports avant les définitions),
  valeur initiale `0` ou valeur fournie par `enregistrer_global_initial()`.
* **Preuve** (`make_imports.py`, module important `env.g` global i32 mutable
  et `env.t` table funcref min 2) : nouveau binaire → `import_global_get=0` ;
  ancien binaire → `import de global env.g : non supporté`.

### 1.5 Hôtes WASI — ✅ 4 → 27 par namespace (56 entrées au total)

`wasi_snapshot_preview1` **et** `wasi_unstable` exposent désormais :
`fd_write`, `fd_read`, `proc_exit`, `random_get` (déjà) **plus**
`args_get`, `args_sizes_get`, `environ_get`, `environ_sizes_get`,
`clock_time_get`, `sched_yield`, `fd_close`, `fd_seek`, `fd_fdstat_get`,
`fd_filestat_get`, `fd_prestat_get`, `fd_prestat_dir_name`, `fd_sync`,
`fd_datasync`, `path_open`, `path_filestat_get`, `path_link`, `path_rename`,
`path_symlink`, `path_readlink`, `path_unlink_file`, `path_remove_directory`,
`path_create_directory`.

**Stubs sandboxés honnêtes** : `args_*`/`environ_*` exposent 0 argument et
0 variable (pas de fuite de l'environnement hôte) ; `clock_time_get` rend
la vraie horloge système ; `fd_prestat_*` rend **EBADF (8)** (aucun
répertoire préouvert) ; `path_*` rend **ENOTCAPABLE (76)** (aucun accès
disque). Aucun accès réseau/fichier.

**Preuve** (`probe_hosts.c`, 12 appels) : `host_bits=4095` = tous les bits,
y compris les errno attendus 8 et 76 → les 12 nouveaux hôtes sont résolus
et exécutés. Le convertisseur a listé les 12 imports et les a acceptés.

### 1.6 Correction de contrôle identifiée au passage — ✅ décisive

Un `br N` remettait la pile et posait `pc`, mais **ne dépilait pas les
blocs quittés**. Les `end` suivants dépilaient donc les mauvais blocs et
un `br` imbriqué sautait au mauvais `fin`. Symptôme mesuré : xxHash `-O2`
entrait dans sa boucle de 16 octets avec `len = 3` →
`accès mémoire hors limites : 132108+4 > 131081`. Après correction
(`sauter_et_pile_ecrire` dépile `N+1` blocs, ou `N` pour un `loop` qui
reste actif), **plus aucune tolérance mémoire n'était nécessaire** : les
limites mémoire strictes d'origine ont été restaurées et -O1/-O2 passent.

---

## 2. Convertisseur (`rose-wasm-lib.sh`)

Option ajoutée : **`--opt 0|1|2|s|z`** (défaut `0` = comportement
historique préservé). Elle pilote `-O…` pour clang/clang++ et `-C
opt-level=…` pour rustc. La vérification ne refuse plus `start` ni
`call_indirect` : elle les **rapporte** dans `WASM_CHECK`. Le miroir
Python des hôtes passe de 10 à 56 entrées (27 par namespace WASI).

```
$ bash scripts/rose-wasm-lib.sh … --opt 2 --allow-imports …
WASM_CHECK imports=1 start=0 call_indirect=5 memory_exported=yes
==> Verifie: imports resolus, memoire exportee (start/call_indirect: voir WASM_CHECK).
==> --allow-imports: imports de fonction/MEMOIRE/TABLE/GLOBAL autorises et verifies.
==> Optimisation: -O2
```

---

## 3. Bibliothèques réelles converties et exploitées

Toutes les sources sont des bibliothèques **réelles téléchargées**, pas du
code écrit pour le test. Les adapteurs fins (`rw_adapt*.c/.cpp`) ne sont pas
des sources des bibliothèques.

### 3.1 miniz 11.3.2 — compression **et** décompression complètes — 7/7

Sources officielles (MIT) : `miniz.c`, `miniz_tdef.c`, `miniz_tinfl.c` +
headers. Adapteur : boucle aller-retour `mz_compress` → `mz_uncompress` +
comparaison octet à octet ; allocateur bump sandboxé et stub `__assert_fail`
nécessaires car `-nostdlib`.

```sh
cd /tmp/opencode/rb_agents/miniz
bash /home/clausake/Stefan/ROSE/scripts/rose-wasm-lib.sh \
  --export 'miniz_crc32=rw_miniz_crc32(chaine)->entier' \
  --export 'miniz_bound=rw_miniz_bound(entier)->entier' \
  --export 'miniz_version_len=rw_miniz_version_len()->entier' \
  --export 'miniz_banner=rw_miniz_banner()->entier' \
  --export 'miniz_roundtrip=rw_miniz_roundtrip(chaine)->entier' \
  --allow-imports --opt 2 --out libminiz_O2 \
  miniz.c miniz_tdef.c miniz_tinfl.c rw_adapt.c
```

```
WASM_CHECK imports=1 start=0 call_indirect=5 memory_exported=yes
==> OK: ./libminiz_O2.wasm  (36206 octets)
```

```text
$ /tmp/opencode/rb_agents/rose demo_miniz_O2.rose
[miniz-wasm] mz_version() = 11.3.2
OK crc32
OK bound
OK version
OK banner/import
OK roundtrip vide -> 8
OK roundtrip abc -> 14
OK roundtrip long -> 25
TOTAL: 7 OK, 0 ECHEC
```

Le banner est écrit **par le module wasm via l'import `fd_write`**.
À `-O0`, la même démo échoue :
`Rose : erreur — appel WebAssembly : … : pile vide (local.set)`.

### 3.2 LZ4 1.10.0 — compression et décompression — 5/5

Sources officielles `lz4.c`/`lz4.h`, adapteur avec `LZ4_compress_default` +
`LZ4_decompress_safe`.

```sh
cd /tmp/opencode/rb_agents/lz4
bash /home/clausake/Stefan/ROSE/scripts/rose-wasm-lib.sh \
  --export 'lz4_banner=rw_banner()->entier' \
  --export 'lz4_decompress=rw_decompress()->entier' \
  --export 'lz4_len=rw_len(chaine)->entier' \
  --export 'lz4_roundtrip=rw_roundtrip(chaine)->entier' \
  --allow-imports --opt 2 --out liblz4_O2 lz4.c rw_adapt.c
```

```
WASM_CHECK imports=1 start=0 call_indirect=0 memory_exported=yes
==> OK: ./liblz4_O2.wasm  (10248 octets)
```

```text
$ /tmp/opencode/rb_agents/rose demo_lz4_O2.rose
[lz4-wasm] version LZ4 = 1.10.0
OK banner/import
OK decompress_safe
OK len
OK roundtrip abc
OK roundtrip long
TOTAL: 5 OK, 0 ECHEC
```

À `-O0` : `Rose : erreur — appel WebAssembly : … : pile vide (local.set)`.
C'est exactement le timeout/échec signalé dans l'état initial : l'optimisation
le débloque.

### 3.3 MurmurHash3 (A. Appleby, domaine public) — 3/3

Source officielle `smhasher/src/MurmurHash3.cpp` téléchargée, C++ `-O2`,
fonction `MurmurHash3_x86_32`. Référence indépendante : le même fichier
compilé en natif `g++ -O2` (`native_murmur`).

```text
natif : abc -> 3017643002 (-1277324294) ; Hello, world! -> 3224780355 (-1070186941)
wasm  : OK murmur vide / OK murmur abc / OK murmur Hello world
```

---

## 4. Optimisation `-O1`/`-O2` mesurée

Deux axes, tous deux mesurés avec le même binaire :

**a) Déblocage fonctionnel** : `-O0` échoue sur miniz *et* LZ4
(`pile vide (local.set)`) ; `-O2` donne 7/7 et 5/5. C'est le gain décisif.

**b) Vitesse** — xxHash, 20 000 appels sur une chaîne de 26 octets, 3
échantillons, somme identique `230872540000` dans les 6 cas :

| Niveau | 1 | 2 | 3 | médiane |
|---|---:|---:|---:|---:|
| `-O0` | 271 ms | 285 ms | 270 ms | **271 ms** |
| `-O2` | 81 ms | 81 ms | 59 ms | **81 ms** |

→ **×3,35** sur ce micro-bench, résultat identique. `-O1` validé
séparément : `46947589`, `852579327` (vecteurs officiels).

---

## 5. Validation (binaire `/tmp/opencode/rb_agents/rose`)

| Contrôle | Sortie mesurée |
|---|---|
| `cargo build --release --bin rose` | `Finished`, 0 erreur |
| `cargo test --release` | `214 + 198 + 0 + 6 + 1 + 0 = 419 passed, 0 failed` (au moment du changement) ; re-mesuré en fin de chantier : `217 + 201 + 0 + 6 + 1 + 0 = 425 passed, 0 failed` — des agents concurrents ont ajouté des tests entre-temps |
| xxHash (`""`, `"abc"`) | `46947589`, `852579327` |
| FNV-1a (`""`, `"abc"`, `"Hello, world!"`, `text_len`) | `-2128831035`, `440920331`, `-309268332`, `3` |
| miniz historique (`demo_miniz.rose`) | `TOTAL: 7 OK, 0 ECHEC` |
| jardin `premiers.rose` | `20 tests, 0 échec` |
| point fixe `rose rir compilateur/emetteur.rose \| md5sum` | `b084ac5c926f6ae486845f9bc96c616d` |
| point fixe `rose compilateur/emetteur.rose compilateur/emetteur.rose \| md5sum` | `b084ac5c926f6ae486845f9bc96c616d` (**égalité ✓**) |

---

## 6. Frontière exacte du « n'importe quelle bibliothèque » — **NON**

Ce qui est **maintenant supporté** (preuves ci-dessus) :

* C / C++ / Rust / WAT / Zig compilés en wasm32 **sans runtime de langage**
  (`-nostdlib` pour C/C++), y compris avec `-O1`/`-O2` ;
* `call_indirect` (table d'éléments + vérification de type), section `start`
  (bornée), imports de fonction/mémoire/table/global ;
* 56 hôtes (2 `env` + 27 WASI × 2 namespaces), arguments scalaires et
  chaînes, mémoire linéaire, i32/i64/f32/f64, contrôle et mémoire MVP ;
* trois vraies bibliothèques exploitées : miniz (CRC32 + bound + version +
  compression/décompression), LZ4 (version + compression/décompression),
  MurmurHash3 (3 vecteurs).

Ce qui **n'est PAS supporté**, avec la raison technique :

| Limite | Raison |
|---|---|
| Python, Go, JS, Java, C# | Exigent un runtime/GC importé (CPython, Go runtime, Emscripten). Le pont n'embarque aucun runtime de langage. Le convertisseur les refuse explicitement. |
| Exceptions C++ / Rust `std::panic=unwind` | Le script force `-fno-exceptions -fno-rtti` et `-C panic=abort` : la spec wasm des exceptions n'est pas implémentée. |
| `malloc`/libc complète, `printf` arbitraire | Seuls les hôtes listés en §1.5 existent. Pour miniz nous avons dû fournir un allocateur bump dans l'adapteur. Pas de `fd_prestat` positif : le sandbox n'a pas de système de fichiers. |
| SIMD `v128`, threads, `externref`, tables multiples | Opcodes/types hors sous-ensemble ; refusés avec message explicite. |
| Sections `elem` hors table 0, globals importés avec une valeur non nulle | Valeur par défaut 0 ; un appel Rust `enregistrer_global_initial` peut la fournir, mais aucun chemin depuis `.rose` n'existe aujourd'hui. |
| Boucles infinies | Arrêtées par le budget (propre, mais l'appel échoue). |
| Appels wasm depuis du code non `extern "C"` / ABI complexes (structs par valeur, variadiques) | Le pont ne convertit que i32/i64/f32/f64 et pointeurs `chaine`. |

**Conclusion** : « n'importe quelle bibliothèque » reste faux. La formulation
exacte est : **toute bibliothèque dont le calcul est réductible à des
fonctions à arguments scalaires/chaînes, compilable en wasm32 autonome
(sans runtime, sans libc autre que les stubs ci-dessus), est désormais
exploitable même avec `-O2` — y compris quand elle utilise `call_indirect`
et `start`.** Les runtimes de langage, les exceptions, la libc complète
et les ABI structurées restent hors périmètre.

---

## 7. Fichiers livrés

* `rose/src/wasm_lecteur.rs` — pont étendu (call_indirect, start, budget,
  globals/tables importés, 56 hôtes, correction de `br`).
* `scripts/rose-wasm-lib.sh` — option `--opt`, vérification mise à jour.
* `rose/docs/WASM_UNIVERSEL.md` — ce document.
* Matériel d'expérience (non versionné) : `/tmp/opencode/rb_agents/`
  (`rose`, `miniz/`, `lz4/`, `xxh/`, `murmur*`, `probe_hosts.c`,
  `make_start.py`, `make_imports.py`, `make_loop.py`, démos).
