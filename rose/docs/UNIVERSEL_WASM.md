# Rose, universel via WebAssembly — guide pratique (v0.1.5)

> **Constat daté du 18/11/2026.** Tout ce qui est marqué « testé » ci-dessous a
> été réellement exécuté sur cette machine pendant la rédaction de ce guide.
> Les performances citées sont des mesures antérieures datées, pas des specs.
> Les états de build et de ports changent : re-mesurer avant de réutiliser une
> conclusion.

Rose est un langage à arithmétique exacte, sans GMP, écrit en Rust. Le
WebAssembly (wasm) le rend **universel** de trois façons distinctes. Ce guide
donne la commande exacte pour chacune, les prérequis, les limites réelles, et
un démarrage en 5 minutes.

---

## 0. Les trois façons d'être « universel »

| Voie | Ce qui est wasm | Ce que ça apporte | Commande reine |
|---|---|---|---|
| **(a) Rose COMPILÉ EN wasm** | Le moteur lui-même (`rose.wasm`, ~586 Ko) | Le langage tourne dans un navigateur, Node, un edge runtime — partout où il existe un hôte wasm | `cargo build --profile wasm-release --target wasm32-unknown-unknown --no-default-features` |
| **(b) Rose QUI APPELLE du wasm** | Une bibliothèque C/Rust/Zig externe | Rose utilise du code d'autres langages **sans embarquer leur runtime** (`@dll("wasm", …)`) | `rose wasm lib.wasm` puis `importer "lib.rose"` |
| **(c) wasm comme cible de distribution** | Le binaire Rose livré | **Un seul fichier**, zéro dépendance OS (ni glibc, ni DLL, ni GMP) | copier `rose.wasm` + `rose-wasm.js` |

Les trois voies sont indépendantes : on peut compiler Rose en wasm **et** faire
appeler du wasm à Rose (la voie (b) fonctionne aussi dans le moteur natif).

---

## 1. Démarrage en 5 minutes — ce qui a été testé

### 1.1 Prérequis (vérifiés)

Toutes les commandes de cette section partent de la **racine du dépôt**
(`/home/clausake/Stefan/ROSE` ici).

```bash
# Node.js — suffisant pour la voie (a) sans navigateur
node --version                  # ici : v26.12.1

# Pour la voie (b) avec le binaire Rose fourni : aucun prérequis supplémentaire
./rose/rose --version           # ici : Rose 0.1.5 — compilateur Rust + JIT Cranelift
```

### 1.2 Test A — Rose compilé en wasm, piloté par Node (TESTÉ, code 0)

Depuis `rose/`, avec le module précompilé `wasm-release/rose.wasm` et le glue
officiel `web/rose-wasm.js` (aucun `wasm-bindgen`, aucun `wasm-pack`) :

```bash
cd rose
node --input-type=module -e '
import fs from "node:fs";
const { default: Rose } = await import("./web/rose-wasm.js");
Rose.configure({ bytes: fs.readFileSync("target/wasm32-unknown-unknown/wasm-release/rose.wasm") });
await Rose.ready();
console.log("version :", Rose.version());
console.log("1+1     :", JSON.stringify(await Rose.execute(">> 1 + 1")));
console.log("6*7     :", JSON.stringify(await Rose.execute("afficher(6 * 7)")));
console.log("latex   :", JSON.stringify(await Rose.latex(">> 1 + 1")));
console.log("analyse :", JSON.stringify(await Rose.analyse(">> 1 + 1")));
'
```

Sortie réelle :

```
version : Rose 5.12.0 — compilateur Rust + JIT Cranelift (sémantique du moteur C++)
1+1     : "2\n"
6*7     : "42\n"
latex   : "\\mathrm{afficher}\\left(2\\right)\n"
analyse : "programme valide — 1 instruction(s), 0 fonction(s)\nvérification : OK"
```

Le test complet du dépôt (`node web/test-wasm.mjs`) s'instancie aussi
correctement et valide addition, flottants, rationnel exact, erreurs runtime,
erreurs d'analyse, LaTeX et analyse. **6 contrôles de parité échouent** face au
binaire natif actuel : le module date du 12/11 et le natif du 18/11 a ajouté des
annotations LaTeX dans sa sortie (`2^{100}   1267650600228229401496703205376`
au lieu de la valeur seule). La **valeur calculée est correcte** ; c'est un
décalage de formatage entre deux versions, pas un calcul faux. Le test complet
rend `6 test(s) en échec.` / code 1 — ne pas le présenter comme vert.

### 1.3 Test B — Rose appelle un module wasm (TESTÉ, code 0)

Module `fib` récursif de 61 octets (base64 vérifié dans `docs/PORTE_WASM.md`) :

```bash
cd /home/clausake/Stefan/ROSE
printf 'AGFzbQEAAAABBgFgAX8BfwMCAQAHBwEDZmliAAAKHgEcACAAQQJIBH8gAAUgAEEBaxAAIABBAmsQAGoLCw==' \
  | base64 -d > /tmp/opencode/fib27.wasm
./rose/rose wasm /tmp/opencode/fib27.wasm                                  # inspection
./rose/rose wasm /tmp/opencode/fib27.wasm --export fib --args 27 --runtime officiel
```

Sortie réelle (inspection) :

```
@dll("wasm", "/tmp/opencode/fib27.wasm")
@sym("fib")
fn fib(a0: entier) -> entier

1 fonction(s) exportée(s)
```

Sortie réelle (exécution via le runtime officiel) :

```
196418
```

---

## 2. Voie (a) — Rose compilé EN wasm

### 2.1 Prérequis

```bash
rustup target add wasm32-unknown-unknown     # cible installée ici (avec wasm32-wasip1)
```

Aucun Emscripten, wasm-pack ou wasm-bindgen : le glue `web/rose-wasm.js` parle
directement à l'interface C ABI exportée par `src/wasm.rs`. Seules dépendances
Rust : `num-bigint`, `num-traits`, `libc` (sans GMP). Le JIT Cranelift est
exclu sur cette cible et remplacé par un shim (`src/lib.rs`).

### 2.2 Construire le module

Commande canonique du projet (profil dédié, LTO + 1 codegen-unit) :

```bash
cd rose
cargo build --profile wasm-release --target wasm32-unknown-unknown --no-default-features
cp target/wasm32-unknown-unknown/wasm-release/rose.wasm web/rose.wasm
```

Variante demandée dans le cahier des charges (profil `release`, feature
`native` par défaut) :

```bash
cargo build --release --target wasm32-unknown-unknown --lib
ls -la target/wasm32-unknown-unknown/release/*.wasm
```

### 2.3 ⚠ État réel du build au 18/11/2026 — ÉCHEC des deux commandes

Les deux commandes ci-dessus **échouent aujourd'hui** avec exactement les deux
erreurs suivantes (exit code `101`) :

```
error[E0425]: cannot find function `compiler_natif` in module `crate::jit`
   --> src/interp.rs:749:42
    |
749 |             if let Some(c) = crate::jit::compiler_natif(&corps, &sig) {
    |                                          ^^^^^^^^^^^^^^ not found in `crate::jit`

error: this arithmetic operation will overflow
   --> src/mul_rapide.rs:1454:12
    |
1454 |     if n > (1usize << 32) {
    |            ^^^^^^^^^^^^^^ attempt to shift left by `32_i32`, which would overflow
```

Origine : le commit `1b73c74` (M3.1, câblage JIT `compiler_natif`) a ajouté
l'appel sans ajouter la fonction au shim JIT wasm de `src/lib.rs` ; le commit
`24daf0c` (M2.3) a introduit `1usize << 32`, qui déborde sur un `usize` 32 bits.
`56 warnings` accompagnent l'échec (aucun n'est bloquant).

**Conséquence pratique** : on ne peut pas **reconstruire** le module wasm depuis
la source actuelle. Les modules déjà compilés restent utilisables :

```
target/wasm32-unknown-unknown/release/rose.wasm        695 732 o  (14/11/2026)
target/wasm32-unknown-unknown/wasm-release/rose.wasm   585 916 o  (12/11/2026)
web/rose.wasm                                          585 916 o  (14/11/2026)
```

Ne pas prétendre qu'un build frais fonctionne : il échoue. La correction
(hors du périmètre de ce guide, qui ne touche à aucun `.rs`) consiste à ajouter
`compiler_natif` au shim `jit` et à remplacer le décalage par une comparaison
sûre en 64 bits (`(n as u64) > (1u64 << 32)`).

### 2.4 L'interface C ABI (pour un glue maison)

| Fonction | Rôle |
|---|---|
| `rose_alloc(len) -> *mut u8` | alloue `len` octets pour l'hôte |
| `rose_free(ptr, len)` | libère un tampon alloué |
| `rose_execute(ptr, len) -> *mut u8` | exécute, rend la sortie |
| `rose_execute_latex(ptr, len) -> *mut u8` | rend le LaTeX |
| `rose_analyse(ptr, len) -> *mut u8` | rend le résumé d'analyse |
| `rose_version() -> *mut u8` | rend la version |
| `rose_prendre_sortie()` / `rose_dernier_panic()` / `rose_raz_panic()` | sortie capturée et messages d'erreur |
| `rose_free_buffer(ptr)` | libère un tampon à en-tête |

Convention des tampons renvoyés : `octets 0..3 = u32 little-endian (longueur N)`,
puis `N` octets UTF-8. Le glue lit la longueur, décode, puis appelle
`rose_free_buffer`. La pile wasm fait 1 Mio par défaut ; pour les expressions
très imbriquées :

```bash
RUSTFLAGS="-C link-arg=-zstack-size=8388608" cargo build --profile wasm-release \
    --target wasm32-unknown-unknown --no-default-features
```

### 2.5 Et dans un navigateur ?

Le glue fournit le chargement par URL :

```html
<script type="module">
  import Rose from './rose-wasm.js';
  Rose.configure({ url: './rose.wasm' });
  await Rose.ready();
  console.log(await Rose.execute('>> 1 + 1'));
</script>
```

**Non testé en navigateur lors de cette session** : seule l'exécution sous
Node.js a été validée. Le glue et la page de démo `web/index.html` existent,
mais ne pas affirmer que Rose tourne dans un navigateur sans l'y avoir ouvert.

---

## 3. Voie (b) — Rose QUI APPELLE du wasm (pont M6.1)

### 3.1 Inspector un module

```bash
rose wasm lib.wasm
```

Affiche une ligne `@dll("wasm", …)`, une ligne `@sym("…")` par export, la
signature Rose déduite et le nombre de fonctions exportées (voir §1.3, testé).

### 3.2 Compiler une bibliothèque C en module Rose-callable

Prérequis : wasi-sdk (clang + sysroot). Ici :
`/home/clausake/.local/wasi-sdk/bin/clang` (clang-18) et son `share/wasi-sysroot`.

```bash
bash scripts/rose-wasm-lib.sh \
  --export 'xxh32_str=XXH32(chaine,entier,entier)->entier' \
  --out libxxhash xxhash.c
./rose/rose wasm libxxhash.wasm
```

L'utilitaire génère `libxxhash.wasm` (core-wasm v1 autonome, 0 import, 0 start,
0 `call_indirect`, mémoire exportée, **`-O0` forcé**) et `libxxhash.rose` :

```rose
@dll("wasm", "/chemin/absolu/libxxhash.wasm")
@sym("xxh32_str")
fn xxh32_str(a0: chaine,a1: entier,a2: entier) -> entier
```

Ensuite, dans un programme Rose :

```rose
importer "/chemin/absolu/libxxhash.rose"
afficher(xxh32_str("abc", 3, 0))     # → 852579327 (vecteur officiel xxHash32)
```

### 3.3 Runtime officiel optionnel (M6.2) — TESTÉ

L'interpréteur de pile maison est le **défaut**, zéro dépendance. Pour les
modules chauds, un runtime officiel distinct peut être installé explicitement :

```bash
rose wasm --install-runtime wasmtime                 # téléchargement + SHA-256
rose wasm lib.wasm --export fib --args 27 --runtime officiel    # → 196418 (testé)
```

Ici le runner est déjà présent dans `~/.cache/rose/runtimes/wasmtime-v0.1.5`
(6 493 120 octets, droits 0755). **Aucun runtime n'est embarqué dans Rose** et
aucun n'est téléchargé pendant une requête : sans runner, repli sur l'interpréteur
maison. Mesure datée du 18/11/2026 (`docs/WASM_RUNTIME.md`) : médiane **2,885 ms**
sur 6 échantillons pour `fib(27)` via le runner officiel, contre **172,1 ms**
pour l'interpréteur maison — la sortie `196418` est identique. Nous avons
reproduit la sortie, pas re-mesuré les timings : re-mesurer sur la machine cible.

---

## 4. Voie (c) — wasm comme cible de distribution

L'asset `rose-wasm.wasm` est annoncé dans la release `v0.1.5` de
`Stef302/Rose` selon le contexte du projet. **Vérification en ligne non
obtenue au moment de la rédaction** : `gh release view 0.1.5 --repo Stef302/Rose`
a répondu `release not found` (sortie réelle). Vérifier le compte avant toute
publication avec un PAT : le dépôt est `Stef302/Rose`, jamais `clausake`.

Concrètement, distribuer Rose en wasm = livrer **deux fichiers statiques** :

```
rose.wasm        le moteur (585 916 o)
rose-wasm.js     le glue (7 671 o)
```

Aucun installateur, aucune bibliothèque système, aucune dépendance OS : les
mêmes octets tournent sous Linux, macOS, Windows, dans Node, dans un navigateur
ou dans un edge runtime. C'est le sens de « universel » : **le fichier est le
contrat**, pas l'environnement.

---

## 5. Comparatif : natif (ELF / PE / Mach-O) vs wasm (universel)

| Critère | Natif (JIT Cranelift) | wasm (interprété / hôte wasm) |
|---|---|---|
| Vitesse `fib(27)` | ~4,6 ms (binaire natif, `docs/PORTE_WASM.md`) | ~172 ms interpréteur maison ; 2,885 ms avec runtime officiel (mesuré le 18/11) |
| Taille | `rose` ~4,9 Mio | `rose.wasm` ~586 Ko (profil `wasm-release`) |
| Dépendances OS | glibc / MSVCRT, ABI plateforme | aucune ; uniquement un hôte wasm (navigateur, Node, runtime) |
| Fichiers à livrer | 1 exécutable par OS/arch | 1 `.wasm` + 1 glue JS, identiques partout |
| JIT | oui (Cranelift, code machine natif) | non (impossible dans wasm) — interprétation seule |
| Système de fichiers / processus / réseau | oui | non sur `wasm32-unknown-unknown` (voir §6) |
| Mise à jour | réinstaller le binaire | remplacer un fichier statique |
| Cas d'usage | CLI, serveur, calcul intensif local | web, sandbox, distribution universelle, bibliothèque embarquée |

Règle de choix : **natif pour la vitesse, wasm pour la portabilité**. Le runtime
officiel réduit l'écart mais n'efface pas le JIT.

---

## 6. Limites honnêtes

1. **ABI C = scalaires + pointeur chaîne uniquement** (`COMPONENT_MODEL.md` §1).
   Pas de types composites (`liste` impossible), pas de tableaux typés, pas de
   `void`. Une chaîne Rose traverse la frontière comme un `i32` pointeur vers la
   mémoire linéaire, avec convention maison (zéro terminal, marge de 8 octets,
   écriture en fin de mémoire). Longueur hors contrat pour `int*`, troncature
   silencieuse en i32, aucun `realloc`/`post-return` négocié. Le pont M6.1 exige
   des modules purs : 0 import, 0 section `start`, 0 `call_indirect` — donc pas
   de `malloc`, `printf` ou `qsort`. Le script force `-O0` : `-O1` et plus
   produisent des modules mal exécutés.
2. **Interpréteur maison lent** : ~172 ms pour `fib(27)` contre ~4,6 ms en natif
   (~40×). Mesurer sur la cible avant de promettre un temps.
3. **Runtime officiel optionnel** : non embarqué, non téléchargé par défaut,
   installation explicite vérifiée SHA-256 ; sans lui, repli automatique sur
   l'interpréteur. Le runner ne normalise que les entiers (`f32`/`f64` tronqués).
4. **Pas de filesystem, pas de processus, pas de réseau** sans WASI. Sur
   `wasm32-unknown-unknown` : aucun fichier lu/écrit, `quitter(n)` arrête le
   module, `lancer_thread` panique, FFI native `@dll`/`@sym` échoue proprement.
   Le pont M6.1 (Rose appelant un module wasm) est une voie distincte .
5. **Pas d'horloge** : `time` / `temps` rendent `0 s` ; la graine aléatoire est
   un compteur monotone (utiliser `graine(n)`). `essayer`/`attraper` n'attrape
   rien car la cible impose `panic = abort` — le message est relu via
   `rose_dernier_panic()` par le glue.
6. **Pile** : 1 Mio par défaut, à élargir avec `-zstack-size` (voir §2.4).
7. **Build cassé au 18/11/2026** : voir §2.3 — deux erreurs de compilation
   empêchent de régénérer le module depuis la source actuelle.

---

## 7. Fichiers de référence dans le dépôt

| Fichier | Rôle |
|---|---|
| `rose/docs/WASM.md` | port complet : ABI, adaptations, limitations |
| `rose/docs/WASM_LIB_UTILITY.md` | utilitaire C → wasm, exemples xxHash / FNV-1a |
| `rose/docs/WASM_RUNTIME.md` | runtime officiel optionnel, mesures, SHA-256 |
| `rose/docs/COMPONENT_MODEL.md` | limites exactes de l'ABI C actuelle |
| `rose/docs/PORTE_WASM.md` | module fib de test et protocole de mesure |
| `rose/docs/WASM_FFI.md` | conventions de la FFI wasm |
| `scripts/rose-wasm-lib.sh` | compile un `.c` en module core-wasm + `.rose` |
| `scripts/publier-runtime.sh` | publication du runner (mainteneur `Stef302`) |
| `rose/web/rose-wasm.js`, `rose/web/index.html`, `rose/web/test-wasm.mjs` | glue, démo, test Node |

---

## 8. Journal des tests réels de cette session (18/11/2026)

| Action | Commande | Résultat |
|---|---|---|
| Build cahier des charges | `cargo build --release --target wasm32-unknown-unknown --lib` (depuis `rose/`, `PATH=~/.cargo/bin`) | **ÉCHEC**, exit 101, 2 erreurs (§2.3) |
| Build canonique | `cargo build --profile wasm-release --target wasm32-unknown-unknown --no-default-features` | **ÉCHEC**, mêmes 2 erreurs, exit 101 |
| Modules existants | `ls -la target/wasm32-unknown-unknown/release/*.wasm` | 695 732 o et 585 916 o présents |
| Exécution ABI (Node 26) | glue officiel + `WebAssembly.instantiate` via `rose-wasm.js` | **OK**, `1+1 → 2`, `6*7 → 42`, LaTeX et analyse OK, exit 0 |
| Test complet dépôt | `node web/test-wasm.mjs` | s'instancie, 6 échecs de parité (formatage), exit 1 — documentés §1.2 |
| Pont M6.1 | `./rose/rose wasm /tmp/opencode/fib27.wasm` | **OK**, 1 fonction exportée, exit 0 |
| Runtime M6.2 | `./rose/rose wasm … --export fib --args 27 --runtime officiel` | **OK**, `196418`, exit 0 |
| Syntaxe scripts | `bash -n scripts/{rose-wasm-lib,publier-runtime,mesurer-wasm}.sh` | **OK** pour les trois |
| Release en ligne | `gh release view 0.1.5 --repo Stef302/Rose` | `release not found` (non vérifié) |

Aucun commit n'a été fait. Aucun fichier hors de `rose/docs/UNIVERSEL_WASM.md`
n'a été modifié dans le dépôt.
