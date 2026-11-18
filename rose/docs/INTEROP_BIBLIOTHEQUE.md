# Interopérabilité : une vraie bibliothèque C appelée depuis Rose via WebAssembly

Ce document relate une expérimentation **réelle et reproductible** : compiler une
bibliothèque C existante en WebAssembly, l'exécuter avec l'interpréteur maison
de Rose (`rose/src/wasm_lecteur.rs`), et appeler ses fonctions depuis un
programme Rose avec `@dll("wasm", …)` / `@sym(…)`.

Tout le code C, les modules `.wasm` et les programmes Rose de test vivent sous
`/tmp/opencode/interop/` — **rien n'a été ajouté au dépôt** hormis ce document.
Aucun `.rs`, `Cargo.toml`, `PLAN.md` ou fichier d'un autre agent n'a été modifié.

---

## 1. Bibliothèque choisie

| Champ | Valeur |
|---|---|
| Nom | **xxHash v0.8.2** (`XXH32`, `XXH64`) |
| Auteur | Yann Collet |
| Licence | **BSD 2-Clause** (`SPDX-License-Identifier: BSD-2-Clause`) |
| Source amont | <https://github.com/Cyan4973/xxHash> — tag `v0.8.2` |
| Fichiers récupérés | `xxhash.h` (253 096 o), `xxhash.c` (1 855 o) |
| Téléchargement | `curl -sSL -o xxhash.h https://raw.githubusercontent.com/Cyan4973/xxHash/v0.8.2/xxhash.h` (id. pour `xxhash.c`) |
| Copie locale | `/tmp/opencode/interop/xxhash.{h,c}` |

**Pourquoi celle-ci** : c'est une bibliothèque de hachage *réelle*, très
répandue (elle n'a pas été écrite pour l'occasion), et son chemin de calcul est
**pur** : `XXH32()` / `XXH64()` ne font ni E/S, ni allocation, ni appel système.
C'est exactement le profil exigé par le pont M6.1 (core-wasm v1.0 autonome,
sans imports, sans `start`, sans `call_indirect`).

Les autres candidats présents sur la machine ont été écartés après inspection :
`giflib/gif_hash.c` (torchcodec) appelle `malloc`/`memset` et exige trois
en-têtes ; `libspdm_sha.c` (NVIDIA) dépend de l'API crypto du noyau Linux
(`lkca_hash_*`, `crypto_shash_*`) ; `libbacktrace/sort.c` (tvm_ffi) est un
`qsort` à pointeur de fonction (donc `call_indirect`, refusé).

---

## 2. Compilation en WebAssembly

### 2.1 Neutraliser les dépendances libc

`xxhash.h` utilise `memcpy` et `memset` pour ses lectures non alignées. Deux
petits fichiers ** hors bibliothèque ** suffisent (ce ne sont pas des
réimplémentations de xxHash) :

`/tmp/opencode/interop/shim.c` — primitives mémoire C standards, écrites comme
de simples boucles pour que clang les inline/élimine sans libc :

```c
#include <stddef.h>
void *memcpy(void *dst, const void *src, size_t n) {
    unsigned char *d = (unsigned char *)dst;
    const unsigned char *s = (const unsigned char *)src;
    for (size_t i = 0; i < n; i++) d[i] = s[i];
    return dst;
}
void *memset(void *dst, int c, size_t n) {
    unsigned char *d = (unsigned char *)dst;
    for (size_t i = 0; i < n; i++) d[i] = (unsigned char)c;
    return dst;
}
```

`/tmp/opencode/interop/glue.c` — adaptateur d'ABI. Rose copie une `chaine` tout
à la fin de la mémoire linéaire avec **seulement `len+1` octets** disponibles
(`interp.rs:3147-3165`) ; or xxHash lit des blocs de 16 octets. L'adaptateur
recopie donc la chaîne dans un tampon statique (avec marge) **avant** d'appeler
la vraie fonction de la bibliothèque :

```c
#include <stddef.h>
#include "xxhash.h"
#define BUF 16384
static unsigned char g_buf[BUF + 32];

unsigned int xxh32_str(const char *s, int n, unsigned int seed) {
    if (n < 0) n = 0;
    if (n > BUF) n = BUF;
    for (int i = 0; i < n; i++) g_buf[i] = (unsigned char)s[i];
    return XXH32(g_buf, (size_t)n, seed);   /* ← la vraie xxHash */
}
unsigned long long xxh64_str(const char *s, int n, unsigned long long seed) {
    if (n < 0) n = 0;
    if (n > BUF) n = BUF;
    for (int i = 0; i < n; i++) g_buf[i] = (unsigned char)s[i];
    return XXH64(g_buf, (size_t)n, seed);   /* ← la vraie xxHash */
}
```

### 2.2 Commande exacte

```sh
/home/clausake/.local/wasi-sdk/bin/clang \
  --target=wasm32-wasi \
  --sysroot=/home/clausake/.local/wasi-sdk/share/wasi-sysroot \
  -O0 -nostdlib -Wl,--no-entry \
  -Wl,--export=xxh32_str -Wl,--export=xxh64_str \
  shim.c xxhash.c glue.c -o libxxhash.wasm
```

Résultat : `libxxhash.wasm`, **6 901 octets**. `-nostdlib` et `--no-entry`
garantissent l'absence de runtime ; le sysroot sert uniquement aux en-têtes
(`stddef.h`, `stdlib.h`, `string.h`, `limits.h`).

⚠ **`-O0` est obligatoire ici** (voir §7) : `-O1` et `-O2` produisent un module
que l'interpréteur Rose exécute faux (pointeur corrompu / accès hors limites).

### 2.3 Le module est bien autonome

Parseur de sections maison sur le binaire (`/tmp/opencode/interop/parse_wasm.py`) :

```
FUNCTION section: 19
MEMORY section: 1
   min=2 flags=0
EXPORT section: 3
   memory kind=2 idx=0
   xxh32_str kind=0 idx=17
   xxh64_str kind=0 idx=18
done
```

→ **aucune section `IMPORT`, aucune section `START`**, `memory` exportée, et
aucune instruction `call_indirect` (vérifié par désassemblage
`llvm-objdump -d --triple=wasm32-unknown-unknown libxxhash.wasm`).

---

## 3. Chargement dans Rose

```
$ ./rose/rose wasm /tmp/opencode/interop/libxxhash.wasm
@dll("wasm", "/tmp/opencode/interop/libxxhash.wasm")
@sym("xxh32_str")
fn xxh32_str(a0: entier, a1: entier, a2: entier) -> entier

@dll("wasm", "/tmp/opencode/interop/libxxhash.wasm")
@sym("xxh64_str")
fn xxh64_str(a0: entier, a1: entier, a2: entier64) -> entier64

2 fonction(s) exportée(s)
```

Le module charge sans erreur : le pont a bien reconnu l'ABI C attendue
`(pointeur i32, longueur i32, graine i32) -> i32`.

---

## 4. Programme Rose et résultat

`/tmp/opencode/interop/prog_xxhash.rose` :

```rose
@dll("wasm", "libxxhash.wasm")
@sym("xxh32_str")
fn xxh32_str(data: chaine, n: entier, seed: entier) -> entier

afficher(xxh32_str("", 0, 0))
afficher(xxh32_str("abc", 3, 0))
afficher(xxh32_str("Hello, world!", 13, 0))
afficher(xxh32_str("The quick brown fox jumps over the lazy dog", 43, 0))
afficher(xxh32_str("abc", 3, 12345))
```

Exécution réelle :

```
$ ./rose/rose prog_xxhash.rose
46947589
852579327
834093149
-396450594
1853483049
```

---

## 5. Comparaison avec une implémentation de référence

Référence : **la même source `xxhash.c` compilée nativement avec gcc 13.3**
(`gcc -O2 -I. ref.c xxhash.c -o ref`, fichier `/tmp/opencode/interop/ref.c`) :

```
"" n=0 | XXH32(seed=0)=46947589 (0x02CC5D05) | XXH32(seed=12345)=2839904920 (0xA9458298)
"abc" n=3 | XXH32(seed=0)=852579327 (0x32D153FF) | XXH32(seed=12345)=1853483049 (0x6E79E829)
"Hello, world!" n=13 | XXH32(seed=0)=834093149 (0x31B7405D) | XXH32(seed=12345)=844570737 (0x32572071)
"The quick brown fox jumps over the lazy dog" n=43 | XXH32(seed=0)=3898516702 (0xE85EA4DE) | XXH32(seed=12345)=3517633706 (0xD1AAD4AA)
```

| Chaîne / graine | Rose (pont wasm) | Référence native | Verdict |
|---|---|---|---|
| `""`, 0 | 46947589 | 46947589 (`0x02CC5D05`) | ✅ |
| `"abc"`, 0 | 852579327 | 852579327 (`0x32D153FF`) | ✅ |
| `"Hello, world!"`, 0 | 834093149 | 834093149 (`0x31B7405D`) | ✅ |
| `"The quick brown fox…"`, 0 | −396450594 | 3898516702 (`0xE85EA4DE`) | ✅ (même mot 32 bits : 3898516702 − 2³² = −396450594) |
| `"abc"`, 12345 | 1853483049 | 1853483049 (`0x6E79E829`) | ✅ |

Les deux premières valeurs sont en outre des **vecteurs de test officiels
xxHash** (`0x02CC5D05` et `0x32D153FF`), ce qui écarte une erreur commune aux
deux compilations. **5/5 résultats corrects**, y compris le cas de 43 octets qui
exerce la boucle interne de 16 octets de XXH32.

---

## 6. Mesure du temps d'appel à travers le pont

Mesure **en interne** avec le builtin `temps()` de Rose (nanosecondes), sur
1000 appels de `xxh32_str("abc", 3, 0)` dans le même programme, 21 échantillons :

`/tmp/opencode/interop/bench.rose` :

```rose
@dll("wasm", "libxxhash.wasm")
@sym("xxh32_str")
fn xxh32_str(data: chaine, n: entier, seed: entier) -> entier

$h = 0
$debut = temps()
pour i de 1 vers 1000 {
    $h = xxh32_str("abc", 3, 0)
}
$fin = temps()
>> $fin - $debut
>> $h
```

Résultats (ns pour 1000 appels) : médiane **15 711 482 ns**, min 9 085 002,
max 15 904 825 — distribution bimodale (~9,1 ms et ~15,7 ms, bruit de
l'hôte). Soit **≈ 15,7 µs par appel** (médiane).

Référence de la boucle Rose seule (1000 itérations sans appel wasm) : médiane
774 174 ns, soit 0,77 µs/itération → le coût propre du pont est bien dominant.

À titre de comparaison, la même fonction compilée nativement (gcc -O2,
accumulation des résultats pour empêcher l'élimination de la boucle, 10⁷
appels) donne **2,14 ns/appel** (médiane de 5 essais ; étendue 2,14–3,68 ns).
L'interpréteur est donc de l'ordre de **7 000× plus lent** que le code natif —
attendu pour un pont de compatibilité, pas une voie de performance.

---

## 7. Limites observées (important)

1. **Optimisation `-O1` / `-O2` : échec.** L'exécution d'un module optimisé
   déclenche un défaut de contrôle de flux dans l'interpréteur (sauts / blocs
   imbriqués), avec des erreurs du type :
   - `-O2` : `appel WebAssembly : libxxhash.wasm : accès mémoire hors limites : 131084+4 > 131081`
   - `-O1` : `appel WebAssembly : libxxhash.wasm : accès mémoire hors limites : 3384764901+4 > 131084` (pointeur devenu garbage)
   - `-Os` : échoue sur la chaîne vide (`131080+4 > 131081`) mais passe sur `"abc"`.
   Seul **`-O0`** donne les 5 résultats corrects. La correction doit se faire
   dans `wasm_lecteur.rs` (cibles de blocs / remise à niveau de la pile), ce qui
   est hors du périmètre de cette mission — aucun `.rs` n'a été touché.

2. **`XXH64` est exporté mais non appelable depuis une source Rose.**
   `rose wasm` affiche le type `entier64` pour un retour/argument `i64`, mais
   `entier64` n'existe pas comme mot-clé source (`TyKind` ne contient que
   `Entier`, `Rationnel`, `Flottant`, `Booleen`, `Chaine`, …). Déclarer
   `-> entier` ramènerait la valeur à `i32` via `ValWasm::as_i32()`, qui rend
   `0` pour un `I64`. Il faudrait un type `entier64`/`i64` côté Rose, ou un
   adaptateur C renvoyant la moitié basse en `i32`.

3. **Passage de chaînes.** Rose place la `chaine` en fin de mémoire avec
   `len+1` octets seulement ; toute fonction lisant des mots de 4/8/16 octets
   au-delà du terminateur provoque une erreur `accès mémoire hors limites`.
   L'adaptateur `glue.c` est donc indispensable pour xxHash. Une primitive Rose
   `ecrire_memoire` exposée aux programmes utilisateurs permettrait de s'en
   passer (cf. `WASM_FFI.md`, § « Ce qui n'est PAS encore fait »).

4. **Entiers = i32.** `interp.rs` convertit les arguments avec
   `z.get_i64() as i32` (troncature silencieuse au-delà de 32 bits) et lit les
   retours avec `as_i32()`. Les valeurs `u32` supérieures à 2³¹ reviennent
   donc **signées négatives** en Rose (cas du 4ᵉ test ci-dessus) : c'est le même
   motif 32 bits, pas une erreur de calcul.

5. **Bibliothèques à éviter avec ce pont** : tout ce qui appelle `malloc`,
   `printf`, `qsort` (pointeur de fonction → `call_indirect`) ou une API
   système. Les imports sont refusés proprement par `charger()`. Ici,
   `memcpy`/`memset` ont été neutralisés et aucune allocation n'est faite.

---

## 8. État final / preuves

| Vérification | Commande | Résultat |
|---|---|---|
| Exports du module | `./rose/rose wasm libxxhash.wasm` | 2 exports, 0 import, 0 start ✅ |
| Exécution Rose | `./rose/rose prog_xxhash.rose` | 5 valeurs ✅ |
| Référence native | `./ref` | 5 valeurs identiques ✅ |
| Chrono | `./rose/rose bench.rose` × 21 | médiane 15,7 µs/appel ✅ |
| Vérification d'autonomie | `python3 parse_wasm.py libxxhash.wasm` | 0 import, 0 start, `memory` exportée ✅ |

Fichiers (tous hors dépôt) : `/tmp/opencode/interop/{xxhash.h,xxhash.c,shim.c,
glue.c,libxxhash.wasm,prog_xxhash.rose,ref.c,ref,bench.rose,bench_native.c,
parse_wasm.py,verify_all.sh}`.

Seul fichier du dépôt modifié : **`rose/docs/INTEROP_BIBLIOTHEQUE.md`** (ce
document). Aucun commit n'a été fait.
