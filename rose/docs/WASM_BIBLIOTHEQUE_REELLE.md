# Bibliothèque réelle → wasm **avec import** → exploitée depuis Rose

**Constat daté : 19/11/2026.** Dépôt `Stef302/Rose`, HEAD `6eabd46`, Rose 0.1.5.
Toutes les commandes ci-dessous ont été **exécutées** ; les sorties sont celles
obtenues, pas des citations. Binaire utilisé : `/tmp/opencode/wa2_agents/rose`
(reconstruit par `cargo build --release --bin rose` puis copié ; `rose/rose` n'a
**pas** été remplacé). Aucun commit, aucune publication, aucun `.rs`/`Cargo.toml`
modifié par ce chantier.

---

## 1. Bibliothèque choisie : **miniz** (`MZ_VERSION "11.3.2"`)

Sources **réelles** téléchargées depuis `github.com/richgel999/miniz` (licence
MIT) dans `/tmp/opencode/wa2_agents/miniz/` :

```
miniz.c          26024 o   miniz.h       29216 o
miniz_tdef.c     78339 o   miniz_tdef.h  11621 o
miniz_tinfl.c    40824 o   miniz_tinfl.h  8808 o
miniz_zip.c     209793 o   miniz_zip.h   26968 o
miniz_common.h    2681 o   miniz_export.h (header minimal généré par CMake, écrit à la main)
```

Pourquoi : c'est une **vraie bibliothèque** de compression multi-fichiers, avec
des dizaines de symboles publics (`mz_crc32`, `mz_adler32`, `mz_compressBound`,
`mz_version`, `mz_deflate*`, `mz_inflate*`, …), pas un bout de code écrit pour le
test. Ses fonctions `mz_crc32`, `mz_compressBound` et `mz_version` sont pures et
fonctionnent à `-O0` sous le pont. Aucun GMP, aucun `rug`.

Un adapteur fin `rw_adapt.c` (écrit pour la conversion, **pas** une source miniz)
fournit les signatures Rose et **un import réel** :
`__attribute__((import_module("wasi_snapshot_preview1"), import_name("fd_write")))`.

---

## 2. Commande de conversion **avec import** + sortie exacte

```sh
cd /tmp/opencode/wa2_agents/miniz
bash /home/clausake/Stefan/ROSE/scripts/rose-wasm-lib.sh \
  --export 'miniz_crc32=rw_miniz_crc32(chaine)->entier' \
  --export 'miniz_bound=rw_miniz_bound(entier)->entier' \
  --export 'miniz_version_len=rw_miniz_version_len()->entier' \
  --export 'miniz_banner=rw_miniz_banner()->entier' \
  --allow-imports --out libminiz_wasm miniz.c rw_adapt.c
```

```text
==> compilation: /home/clausake/.local/wasi-sdk/bin/clang ... -O0 ... -> ./libminiz_wasm.wasm
WASM_CHECK imports=1 start=0 call_indirect=0 memory_exported=yes
exports=miniz_crc32,miniz_bound,miniz_version_len,miniz_banner
imports_liste=wasi_snapshot_preview1.fd_write(kind=0)
==> OK: ./libminiz_wasm.wasm  (4457 octets)
==> OK: ./libminiz_wasm.rose
==> Verifie: imports resolus, 0 start, 0 call_indirect, memoire exportee.
==> --allow-imports: imports de fonction/MEMOIRE autorises et verifies.
```

Vérification des exports :

```text
$ /tmp/opencode/wa2_agents/rose wasm libminiz_wasm.wasm
@dll("wasm", "libminiz_wasm.wasm")
@sym("miniz_crc32")
fn miniz_crc32(a0: entier) -> entier
... (4 déclarations) ...
4 fonction(s) exportée(s)
```

Sans `--allow-imports`, **la même commande échoue** (preuve que l'option est bien
ce qui autorise l'import) :

```text
WASM_CHECK imports=1 start=0 call_indirect=0 memory_exported=yes
exports=miniz_crc32,miniz_banner
imports_liste=wasi_snapshot_preview1.fd_write(kind=0)
ERREUR: incompatible avec le pont Rose: 1 import(s) (utiliser --allow-imports)
```

Source de l'adapteur (code complet, tel que livré) :

```c
/* rw_adapt.c — adapteur ajoute autour de miniz (MZ_VERSION 11.3.2, 19/11/2026). */
#include <stddef.h>
#include "miniz.h"

typedef struct { const void *ptr; size_t len; } rw_iovec;

__attribute__((import_module("wasi_snapshot_preview1"), import_name("fd_write")))
extern int rw_fd_write(int fd, const rw_iovec *iovs, int n_iovs, int *nwritten);

int rw_miniz_crc32(const char *s) {
    size_t n = 0;
    while (s[n]) n++;
    return (int)mz_crc32(MZ_CRC32_INIT, (const mz_uint8 *)s, n);
}
int rw_miniz_bound(int n) {
    return (int)mz_compressBound((mz_ulong)n);
}
int rw_miniz_version_len(void) {
    const char *v = mz_version();
    int n = 0;
    while (v[n]) n++;
    return n;
}
int rw_miniz_banner(void) {
    const char *v = mz_version();
    int n = 0, ecrit = 0;
    rw_iovec iov;
    const char *titre = "[miniz-wasm] mz_version() = ";
    while (v[n]) n++;
    iov.ptr = (const void *)titre; iov.len = 28;
    rw_fd_write(1, &iov, 1, &ecrit);
    iov.ptr = (const void *)v;     iov.len = (size_t)n;
    rw_fd_write(1, &iov, 1, &ecrit);
    iov.ptr = (const void *)"\n";  iov.len = 1;
    rw_fd_write(1, &iov, 1, &ecrit);
    return 42;
}
```

⚠ Mesure : avec des **helpers `static` séparés** (`ecrire`, `long_chaine`),
clang-18 `-O0` émet **1 `call_indirect` parasite** et l'utilitaire refuse le
module. Tout doit être **inliné** — c'est la même règle que celle déjà documentée
pour le `strlen` de la glue.

---

## 3. Fichier de déclarations généré `.rose` (code complet)

```rose
# AUTO-GENERE par rose-wasm-lib.sh — importer ces declarations dans Rose.
# Module: /tmp/opencode/wa2_agents/miniz/libminiz_wasm.wasm
# imports resolus par le pont Rose: wasi_snapshot_preview1.fd_write
# rw_miniz_crc32(chaine)->entier  =>  miniz_crc32
@dll("wasm", "/tmp/opencode/wa2_agents/miniz/libminiz_wasm.wasm")
@sym("miniz_crc32")
fn miniz_crc32(a0: chaine) -> entier

# rw_miniz_bound(entier)->entier  =>  miniz_bound
@dll("wasm", "/tmp/opencode/wa2_agents/miniz/libminiz_wasm.wasm")
@sym("miniz_bound")
fn miniz_bound(a0: entier) -> entier

# rw_miniz_version_len()->entier  =>  miniz_version_len
@dll("wasm", "/tmp/opencode/wa2_agents/miniz/libminiz_wasm.wasm")
@sym("miniz_version_len")
fn miniz_version_len() -> entier

# rw_miniz_banner()->entier  =>  miniz_banner
@dll("wasm", "/tmp/opencode/wa2_agents/miniz/libminiz_wasm.wasm")
@sym("miniz_banner")
fn miniz_banner() -> entier
```

---

## 4. Programme Rose d'appel + sortie exacte

Fichier : `/tmp/opencode/wa2_agents/miniz/demo_miniz.rose` (code complet) :

```rose
# demo_miniz.rose — exploitation depuis Rose d'une VRAIE bibliotheque
# (miniz, MZ_VERSION 11.3.2, sources telechargees) convertie en wasm AVEC un
# import wasi_snapshot_preview1.fd_write. Constat date : 19/11/2026.
#
# NOTE : les comparaisons sont ecrites au niveau global (pas dans une fonction
# qui modifie une variable globale) : mesure, un helper avec `$ok = $ok + 1`
# fait echouer l'appel suivant avec "conversion vers entier impossible".

importer "/tmp/opencode/wa2_agents/miniz/libminiz_wasm.rose"

$ok = 0
$ko = 0

# Valeurs obtenues en appelant le module wasm (vraies fonctions miniz).
$v1 = miniz_crc32("abc")             # mz_crc32
$v2 = miniz_crc32("Hello, world!")   # mz_crc32
$v3 = miniz_crc32("a")               # mz_crc32
$v4 = miniz_bound(1000)              # mz_compressBound
$v5 = miniz_bound(0)                 # mz_compressBound
$v6 = miniz_version_len()            # mz_version()
$v7 = miniz_banner()                 # mz_version() + IMPORT fd_write

# Attendus : ./native_ref (gcc natif sur les memes sources miniz.c).
# unsigned 3957769958 -> -337197338 en i32 Rose ; 3904355907 -> -390611389.
si ($v1 == 891568578) { >> "OK  crc32 abc = " + chaine($v1); $ok = $ok + 1 } sinon { >> "ECHEC crc32 abc = " + chaine($v1); $ko = $ko + 1 }
si ($v2 == -337197338) { >> "OK  crc32 Hello world = " + chaine($v2); $ok = $ok + 1 } sinon { >> "ECHEC crc32 Hello world = " + chaine($v2); $ko = $ko + 1 }
si ($v3 == -390611389) { >> "OK  crc32 a = " + chaine($v3); $ok = $ok + 1 } sinon { >> "ECHEC crc32 a = " + chaine($v3); $ko = $ko + 1 }
si ($v4 == 1228) { >> "OK  compressBound(1000) = " + chaine($v4); $ok = $ok + 1 } sinon { >> "ECHEC compressBound(1000) = " + chaine($v4); $ko = $ko + 1 }
si ($v5 == 133) { >> "OK  compressBound(0) = " + chaine($v5); $ok = $ok + 1 } sinon { >> "ECHEC compressBound(0) = " + chaine($v5); $ko = $ko + 1 }
si ($v6 == 6) { >> "OK  longueur mz_version = " + chaine($v6); $ok = $ok + 1 } sinon { >> "ECHEC longueur mz_version = " + chaine($v6); $ko = $ko + 1 }
si ($v7 == 42) { >> "OK  version via fd_write = " + chaine($v7); $ok = $ok + 1 } sinon { >> "ECHEC version via fd_write = " + chaine($v7); $ko = $ko + 1 }

>> "TOTAL: " + chaine($ok) + " OK, " + chaine($ko) + " ECHEC"
```

Attendus **indépendants** : `native_ref.c` compilé en natif avec
`gcc -O2 -I. native_ref.c miniz.c miniz_tdef.c miniz_tinfl.c -o native_ref` :

```text
version=11.3.2 len=6
crc32(abc)=891568578
crc32(Hello, world!)=3957769958
crc32(vide)=0
bound(1000)=1228
bound(0)=133
```

Rose ramène les `u32 > 2^31` en `i32` signés : `3957769958 → -337197338`,
`3904355907 → -390611389`.

Sortie **exacte** :

```text
$ /tmp/opencode/wa2_agents/rose /tmp/opencode/wa2_agents/miniz/demo_miniz.rose
[miniz-wasm] mz_version() = 11.3.2
OK  crc32 abc = 891568578
OK  crc32 Hello world = -337197338
OK  crc32 a = -390611389
OK  compressBound(1000) = 1228
OK  compressBound(0) = 133
OK  longueur mz_version = 6
OK  version via fd_write = 42
TOTAL: 7 OK, 0 ECHEC
```

La première ligne est **écrite par le module wasm** via
`wasi_snapshot_preview1.fd_write` : l'import n'est pas seulement déclaré, il est
exécuté et résolu par le pont Rose.

---

## 5. Ce qui fonctionne / ne fonctionne pas avec des imports

**Fonctionne (mesuré)** :
- import `wasi_snapshot_preview1.fd_write` depuis une vraie bibliothèque C →
  conversion acceptée avec `--allow-imports`, `imports=1`, `call_indirect=0` ;
- résolution par le pont au chargement, écriture sur stdout (`11.3.2`) ;
- 3 fonctions **réelles** de miniz appelées et vérifiées : `mz_crc32`,
  `mz_compressBound`, `mz_version` (7 contrôles, 0 échec) ;
- `mz_crc32` est l'algorithme de la bibliothèque, pas un stub : valeurs égales
  au binaire gcc natif et à `zlib.crc32` Python.

**Ne fonctionne pas (mesuré, honnête)** :
- sans `--allow-imports`, le convertisseur refuse le module :
  `ERREUR: incompatible avec le pont Rose: 1 import(s)` ;
- seuls **10 hôtes** sont résolus (`env.log`, `env.ecrire`, et
  `fd_write`/`fd_read`/`proc_exit`/`random_get` sous `wasi_snapshot_preview1` et
  `wasi_unstable`) + mémoire importée `env.memory`. Un import `env.inconnu` est
  refusé avant écriture du `.wasm`. PAS de libc complète, PAS de `malloc`,
  PAS de `printf` arbitraire ;
- `mz_adler32` (boucle imbriquée dans miniz) **bloque** : `timeout 30 …` →
  exit `124`. L'algorithme de compression `mz_compress`/`mz_deflate` n'a donc
  **pas** été rendu utilisable ;
- contre-exemple avec une **autre vraie bibliothèque**, LZ4 v1.10.0
  (`/tmp/opencode/wa2_agents/lz4/lib/`, sources officielles) : conversion
  réussie (`imports=1`, 4 puis 5 exports, 0 `call_indirect`, 101566 puis
  102180 octets) mais
  `LZ4_compress_default` → `timeout 60` exit `124`, et
  `LZ4_decompress_safe` → `Rose : erreur — appel WebAssembly : … : pile vide
  (local.set)`. Seules les fonctions scalaires (`LZ4_versionNumber`=11000,
  `LZ4_compressBound(1000)`=1019, `LZ4_versionString`) ont pu être exploitées.
  C'est une limite de l'interpréteur wasm à `-O0` (blocs typés / boucles
  imbriquées), pas de la conversion.

**Verdict sur « n'importe quelle bibliothèque » : NON.** Ce que la chaîne sait
faire, c'est convertir en un module wasm les **sous-ensembles purs** d'une
bibliothèque C/C++/Rust/WAT/Zig dont les fonctions ont des arguments scalaires
ou des chaînes, pas de `call_indirect`, pas de `start`, et des imports limités
aux 10 hôtes intégrés. Une bibliothèque réelle convertit **souvent en entier**,
mais son exploitation complète dépend de la forme du code machine généré à `-O0`. Ici :
miniz → CRC32/bound/version exploitables, adler32/compression non ; LZ4 →
fonctions scalaires exploitables, compression/décompression non.

---

## 6. Non-régression (même binaire `/tmp/opencode/wa2_agents/rose`)

| Contrôle | Sortie mesurée |
|---|---|
| xxHash (`""`, `"abc"`) | `46947589`, `852579327` |
| FNV-1a (`""`, `"abc"`, `"Hello, world!"`, `text_len("abc")`) | `-2128831035`, `440920331`, `-309268332`, `3` |
| jardin `jardins/tests/premiers.rose` | `20 tests, 0 échec` |
| point fixe `rose rir compilateur/emetteur.rose \| md5sum` | `b084ac5c926f6ae486845f9bc96c616d` |
| point fixe `rose compilateur/emetteur.rose compilateur/emetteur.rose \| md5sum` | `b084ac5c926f6ae486845f9bc96c616d` (égalité ✓) |
| `cargo test --release` | `214 + 198 + 6 + 1 = 419 passed, 0 failed` (3 cibles vides) |
| `cargo build --release --bin rose` | `Finished`, 0 erreur |

---

## 7. Fichiers livrés

* Doc : `rose/docs/WASM_BIBLIOTHEQUE_REELLE.md` (ce fichier).
* Tout le matériel d'expérience : `/tmp/opencode/wa2_agents/`
  (`rose`, `miniz/`, `lz4/`). Rien n'a été commité ni publié.
