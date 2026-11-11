# Rose 2.0 — compilation WebAssembly (WASM)

Ce document explique comment compiler le moteur Rose en **WebAssembly** pour
l'exécuter dans un navigateur, et ce qu'il faut adapter dans le dépôt.

Le principe : le moteur Rose devient une **bibliothèque** (`src/lib.rs`) et non
seulement un binaire. La bibliothèque expose

* une API Rust de haut niveau (`rose_executer`, `rose_analyser`, `rose_latex`) ;
* une interface **C ABI** (`src/wasm.rs`) sans aucune dépendance externe
  (ni `wasm-bindgen`, ni `wasm-pack`) : le glue JavaScript (`web/rose-wasm.js`)
  parle directement au module.

Le binaire natif `rose` n'est **pas modifié** : il continue de déclarer ses
propres modules et de fonctionner exactement comme avant.

---

## 1. Prérequis

```bash
rustup target add wasm32-unknown-unknown
```

C'est la seule cible nécessaire pour le navigateur. Aucun outil externe :
ni Emscripten, ni `wasm-pack`, ni `wasm-bindgen`. Le crate n'a que des
dépendances Rust (`num-bigint`, `num-rational`, `libc`).

---

## 2. Les adaptations à appliquer au dépôt

Cinq fichiers sont à adapter. Le détail est donné ci-dessous (§2.1 à §2.6) ;
les diffs unifiés complets sont fournis avec ce document.

| Fichier | Ce qu'il faut faire |
|---|---|
| `Cargo.toml` | Déclarer la bibliothèque (`[lib]`), sortir Cranelift des dépendances communes, isoler les binaires natifs, ajouter un profil `wasm-release`. |
| `build.rs` | Ne pas passer `-lm -lc -rdynamic` à la cible `wasm32` (le lieur `wasm-ld` les refuse). |
| `src/run.rs` | Tampon de capture de la sortie, horloge portable, graine aléatoire sans appel système, `erreur_front` sans `process::exit`, un `>> 32` sûr en 32 bits. |
| `src/interp.rs` | FFI sans `dlopen`, horloge portable pour `time`/`temps`, `Instant::now()` portable, `quitter` et « fonction inconnue » sans `process::exit`, graine du RNG sans `process::id()`. |
| `src/main.rs` | **Aucune modification** (voir §2.6). |

### 2.1 `Cargo.toml`

```diff
+[lib]
+name = "rose"
+crate-type = ["cdylib", "rlib"]
+path = "src/lib.rs"
+
 [[bin]]
 name = "rose"
 path = "src/main.rs"
+required-features = ["native"]
```

* `[lib]` : `cdylib` produit le `.wasm` (le `rlib` garde la crate utilisable
  en Rust et par les tests) ;
* `required-features = ["native"]` : avec `--no-default-features`, les
  binaires natifs ne sont **pas** construits (le binaire natif n'a pas de
  sens en WASM : il lit des fichiers, lance des processus, appelle LLVM).

```diff
 [dependencies]
-cranelift = { version = "0.137", default-features = false, features = ["jit", "frontend", "module", "native"] }
 libc = "0.2"
 ...
+
+# Cranelift (JIT natif) n'est PAS disponible en WebAssembly : le JIT émet du
+# code machine natif, qu'un module WASM ne peut pas contenir. La
+# dépendance est donc conditionnée à la cible ; en WASM, `src/lib.rs`
+# installe un shim `jit` d'API identique (exécution purement interprétée).
+[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
+cranelift = { version = "0.137", default-features = false, features = ["jit", "frontend", "module", "native"] }
```

C'est **la** modification indispensable : Cranelift (via `cranelift-jit`)
n'a pas de port `wasm32` et ne compile pas pour cette cible.

```diff
 [features]
-default = []
+default = ["native"]
+native = []
+wasm = []
 llvm = ["dep:inkwell"]
```

* `native` : active les binaires natifs (JIT Cranelift) ;
* `wasm` : simple marqueur documentaire (aucune feature n'est nécessaire pour
  compiler en WASM : le shim du JIT est choisi par la **cible**, pas par une
  feature).

```diff
+[profile.wasm-release]
+inherits = "release"
+opt-level = 3
+lto = true
+codegen-units = 1
+strip = true
```

Profil dédié : LTO + un seul `codegen-unit` → **579 Ko** au lieu de 653 Ko,
pour les mêmes performances (mesuré : 118 ms pour une boucle de 300 000 tours,
identique au build `release`).

> **À propos de `panic`** : ni `panic = "abort"` (il casserait
> `catch_unwind`, donc les erreurs runtime Rose **et** le
> `essayer`/`attraper` du langage), ni `panic = "unwind"` (les cibles
> `wasm32-unknown-unknown` et `wasm32-wasip1` **imposent** `abort` : rustc
> refuse `-C panic=unwind` sur ces cibles). On laisse donc la stratégie par
> défaut : `unwind` en natif, `abort` en WASM. Le message d'erreur n'est pas
> perdu en WASM : le hook de panique (`src/wasm.rs`) le range avant l'arrêt et
> le glue JS le relit.

### 2.2 `build.rs`

```diff
 fn main() {
+    // WebAssembly : pas de libc système, donc pas de -lm/-lc/-rdynamic
+    // (le lieur wasm-ld refuse ces drapeaux). Le module WASM n'a besoin
+    // d'aucun symbole natif : les helpers du JIT sont absents en WASM.
+    if std::env::var_os("CARGO_CFG_TARGET_ARCH").map_or(false, |v| v == "wasm32") {
+        return;
+    }
+
     if std::env::var_os("CARGO_CFG_TARGET_OS").map_or(false, |v| v == "windows") {
```

### 2.3 `src/run.rs`

Cinq adaptations :

1. **Capture de la sortie** — en WASM il n'y a pas de `stdout` : la sortie du
   programme est capturée dans un tampon mémoire.

   ```rust
   static SINK: Mutex<Option<Vec<u8>>> = Mutex::new(None);

   pub fn set_sink(active: bool) { /* ... *g = Some(Vec::new()) ... */ }
   pub fn take_sink() -> Option<String> { /* ... g.take() ... */ }

   pub fn ecrire_out(s: &str) {
       {
           let mut g = match SINK.lock() { Ok(g) => g, Err(p) => p.into_inner() };
           if let Some(buf) = g.as_mut() { buf.extend_from_slice(s.as_bytes()); return; }
       }
       let _ = std::io::stdout().write_all(s.as_bytes());
   }
   ```

2. **`>> 32` en 32 bits** — `b >> 32` déborde sur un `usize` de 32 bits (WASM) ;
   le compilateur le refuse : `((b as u64) >> 32) as u32`.

3. **`tirage_systeme()`** — `getrandom(2)` est un appel système natif : le
   module WASM ne doit importer aucun symbole de l'hôte. En WASM, on mélange
   un compteur monotone et l'horloge.

4. **`erreur_front()`** — `std::process::exit(1)` est un piège (`unreachable`)
   en WASM : il tue l'instance. On lève une panique `ROSE_ERR:` à la place
   (même message, mais rattrapable par le glue).

5. **Horloge portable** — `std::time::Instant::now()`,
   `std::time::SystemTime::now()` et `std::process::id()` **paniquent** en
   WASM (« time not implemented on this platform », « no pids on this
   platform »). Trois aides sont ajoutées :

   ```rust
   pub fn chrono() -> Chrono { /* Instant en natif, compteur factice en WASM */ }
   pub fn temps_unix_nanos() -> u64 { /* 0 en WASM */ }
   pub fn pid() -> u32 { /* process::id() en natif, compteur en WASM */ }
   ```

### 2.4 `src/interp.rs`

Six adaptations :

1. **FFI** — `dlopen`/`dlsym`/`dlclose` n'existent qu'`unix`/`windows` : pour
   les autres cibles (WASM), des versions qui rendent `NULL` sont fournies.
   Les appels `@dll`/`@sym` échouent alors proprement (« bibliothèque
   introuvable ») ;
2. **builtin `time`** et **builtin `temps`** : horloge portable
   (`crate::run::temps_unix_nanos()`) ;
3. **`executer_source`** : `std::time::Instant::now()` →
   `crate::run::chrono()` (sinon *chaque* exécution panique en WASM) ;
4. **`quitter(code)`** : `std::process::exit` → panique `ROSE_ERR:` en WASM ;
5. **fonction inconnue** : idem ;
6. **graine du RNG** : `std::process::id()` → `crate::run::pid()`.

### 2.5 `src/jit.rs`

**Aucune modification.** `src/jit.rs` n'est pas compilable en WASM
(Cranelift), donc `src/lib.rs` ne le déclare pas pour cette cible et
fournit à la place un **shim** d'API strictement identique :

```rust
#[cfg(not(target_arch = "wasm32"))]
pub mod jit;

#[cfg(target_arch = "wasm32")]
pub mod jit {
    pub enum Compile { Natif(..), Principal(..), Interprete }
    pub fn compilable(_ty: TyKind) -> bool { false }
    pub fn ty_statique(_e: &Expr, _env: &HashMap<String, TyKind>) -> Option<TyKind> { None }
    pub fn fonction_compilable(..) -> bool { false }
    pub fn bloc_compilable(..) -> bool { false }
    pub fn pipeline_flush_si_threads() {}
    pub struct Jit;
    impl Jit {
        pub fn nouveau(_signatures: &HashMap<String, Signature>) -> Jit { Jit }
        pub fn compiler_tout(&mut self) -> HashMap<String, Compile> { HashMap::new() }
        pub fn compiler_principal(&mut self, _prog: &Programme) -> Option<Compile> { None }
    }
}
```

`src/interp.rs` compile donc **à l'identique** dans les deux cas : en WASM,
tout passe par l'interpréteur (mêmes résultats, mêmes messages).

### 2.6 `src/main.rs`

**Aucune modification.** Le binaire natif déclare ses propres modules
(`mod front; mod jit; …`) : il n'utilise pas la bibliothèque et continue de
se construire exactement comme avant. La bibliothèque est simplement
construite *en parallèle* (et le module `jit` y est compilé une seconde
fois — c'est le prix d'une crate à la fois bibliothèque et binaire ; une
réfection plus profonde serait de faire du binaire un consommateur de la
`rlib`).

---

## 3. Construction

```bash
# 1. la cible (une seule fois)
rustup target add wasm32-unknown-unknown

# 2. le module
cargo build --profile wasm-release --target wasm32-unknown-unknown --no-default-features

# 3. le mettre à côté des pages web
cp target/wasm32-unknown-unknown/wasm-release/rose.wasm web/rose.wasm
```

Le binaire `wasm-release` est produit dans
`target/wasm32-unknown-unknown/wasm-release/rose.wasm`.

> **Pile d'exécution** : la pile d'un module WASM fait 1 Mio par défaut, alors
> que le moteur natif utilise un thread à 8 Mio. Les expressions très
> imbriquées peuvent donc saturer la pile (« memory access out of bounds »).
> Pour élargir la pile :
>
> ```bash
> RUSTFLAGS="-C link-arg=-zstack-size=8388608" cargo build --profile wasm-release \
>     --target wasm32-unknown-unknown --no-default-features
> ```
>
> (ou un `.cargo/config.toml` avec
> `[target.wasm32-unknown-unknown]` / `rustflags = ["-C", "link-arg=-zstack-size=8388608"]`).

Le binaire natif, lui, se construit comme avant :

```bash
cargo build --release              # binaire natif (JIT Cranelift)
cargo build --release --features llvm   # avec la chaîne AOT LLVM
cargo test --release               # 52 tests
```

---

## 4. Test hors navigateur (Node.js)

Un script de test est fourni : il instancie le module avec le **vrai** glue
JavaScript et vérifie les sorties, les erreurs runtime, les erreurs d'analyse,
le rendu LaTeX et l'analyse.

```bash
node web/test-wasm.mjs [chemin/vers/rose.wasm]
# → « Tous les tests passent. » (code de sortie 0)
```

En ligne de commande, le module s'instancie directement, sans outillage :

```js
import fs from 'node:fs';
import Rose from './web/rose-wasm.js';

Rose.configure({ bytes: fs.readFileSync('target/wasm32-unknown-unknown/wasm-release/rose.wasm') });
await Rose.ready();
console.log(await Rose.execute('>> 1 + 1'));   // "2\n"
console.log(await Rose.latex('>> 1 + 1'));     // "\mathrm{afficher}\left(2\right)"
```

Dans un navigateur, le même glue charge le fichier par URL :

```html
<script type="module">
  import Rose from './rose-wasm.js';
  Rose.configure({ url: './rose.wasm' });
  await Rose.ready();
</script>
```

---

## 5. L'interface C ABI

| Fonction | Rôle |
|---|---|
| `rose_alloc(len) -> *mut u8` | alloue `len` octets pour l'hôte |
| `rose_free(ptr, len)` | libère un tampon `rose_alloc` |
| `rose_execute(ptr, len) -> *mut u8` | exécute le programme, rend sa **sortie** |
| `rose_execute_latex(ptr, len) -> *mut u8` | rend le **LaTeX** du programme |
| `rose_analyse(ptr, len) -> *mut u8` | rend le résumé de l'analyse |
| `rose_version() -> *mut u8` | rend la version du moteur |
| `rose_prendre_sortie() -> *mut u8` | sortie capturée (à relire après un arrêt) |
| `rose_dernier_panic() -> *mut u8` | message de la dernière panique (consommé) |
| `rose_raz_panic()` | vide le tampon des messages de panique |
| `rose_free_buffer(ptr)` | libère un tampon à en-tête |

**Convention des tampons** : les fonctions qui rendent du texte renvoient un
tampon « en-tête » :

```
octet 0..3 : u32 PETIT BOYEN ENDS = longueur N du texte
octet 4..  : N octets UTF-8 du texte
```

Le glue lit l'`u32`, décode les octets (`TextDecoder`) puis appelle
`rose_free_buffer`.

---

## 6. L'API Rust

```rust
pub fn rose_executer(source: &str) -> String;                 // sortie (erreurs incluses)
pub fn rose_analyser(source: &str) -> Result<String, String>;  // résumé / message d'erreur
pub fn rose_latex(source: &str) -> String;                     // formule LaTeX
```

`rose_executer` renvoie **exactement** ce que le programme affiche ; en cas
d'erreur, le message est rendu dans la chaîne, au format du moteur natif
(`Rose : erreur — division par zéro`).

---

## 7. Limitations connues

### 7.1 Pas de JIT Cranelift

Le JIT émet du code machine natif : impossible à embarquer dans un module
WASM. En WASM, le moteur tourne **en interprétation**. Mesure (300 000 tours
de boucle) : **3 ms** en natif (JIT) contre **118 ms** en WASM, soit ≈ 40× plus
lent. Les *résultats* sont identiques au caractère près (vérifié par un test de
parité sur 19 programmes : arithmétique exacte, rationnels, complexes,
irrationnels, fonctions, récursion, listes, erreurs d'analyse et erreurs
runtime).

### 7.2 Pas de déballage de panique : `essayer`/`attraper` ne fonctionne pas

`wasm32-unknown-unknown` **impose** `panic = "abort"` (rustc refuse
`-C panic=unwind` sur cette cible) : il n'y a donc pas de remontée
d'exception, et `catch_unwind` ne peut rien rattraper.

Conséquence : le `essayer { … } attraper (e) { … }` de Rose **n'attrape
rien** en WASM — l'erreur runtime arrête le module. Le message n'est pas
perdu : le hook de panique (`src/wasm.rs`) le range avant l'arrêt, et le
glue JS attrape le piège (`WebAssembly.RuntimeError`), relit le message et
le rend à l'utilisateur :

```
Rose : erreur — division par zéro
```

Le `catch_unwind` de `src/lib.rs` sert sur les cibles qui savent déballer
(natif, et `wasm32-wasip1` si l'on accepte un runtime WASI).

### 7.3 Pas de système de fichiers, pas de processus, pas de réseau

* aucun fichier ne peut être lu ou écrit ;
* `quitter(n)` arrête le module (erreur rattrapable) au lieu de quitter le
  processus ;
* les threads (`lancer_thread`) ne sont pas disponibles : `std::thread::spawn`
  panique en WASM ;
* la FFI (`@dll`/`@sym`) échoue proprement (« bibliothèque introuvable »).

### 7.4 Pas d'horloge

`Instant::now()` et `SystemTime::now()` paniquent en WASM : les builtins
`time` et `temps` renvoient donc `0 s`. La graine du générateur aléatoire est
un compteur monotone (le RNG système n'a pas d'aléatoire matériel : utilisez
`graine(n)` pour des tirages reproductibles).

### 7.5 Entrées clavier non gérées

`saisir_entier` / `saisir_flottant` n'ont pas d'entrée standard : ils
renvoient une erreur. Une version « navigateur » de ces builtins demanderait
une saisie asynchrone à la page HTML — hors du périmètre du moteur.

### 7.6 Pile d'exécution

Voir §3 : 1 Mio par défaut, à élargir avec `-zstack-size`.

### 7.7 Quirks préexistantes du moteur

Le moteur a un quirk connu avec le nom de compteur `i` : dans une boucle
`pour i de …`, le corps qui utilise `i` dans une expression peut être
mal interprété. C'est **préexistant** (le binaire natif donne lui aussi un
résultat faux), mais en WASM — sans JIT — il se manifeste par une erreur
plutôt que par un résultat faux. Utilisez un autre nom de compteur (`j`,
`k`, …), comme le font les exemples du dépôt.

---

## 8. Fichiers du port WASM

| Fichier | Rôle |
|---|---|
| `src/lib.rs` | la bibliothèque : modules publics, shim JIT, API de haut niveau |
| `src/wasm.rs` | interface C ABI (`rose_execute`, `rose_alloc`, …) |
| `web/rose-wasm.js` | glue JavaScript (chargement, allocation, décodage) |
| `web/index.html` | démo complète (exécution, LaTeX, analyse, exemples) |
| `web/demo.html` | page minimale + commandes de construction |
| `web/test-wasm.mjs` | test hors navigateur (Node.js) du module |
| `docs/WASM.md` | ce document |
