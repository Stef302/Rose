# AOT embarqué dans `rose` — deux voies, état daté du 21/11/2026

## 1. Voie en cours : émetteur machine intégré (vague 76)

À la date de cette note, `src/main.rs` et `src/cmd_llvm.rs` sont en cours de
réécriture (modifications non encore commitées) pour ne plus dépendre de LLVM :
`rose aot` / `rose orc` passent par `natif_emetteur` + `link_natif`, intégrés au
binaire. **Aucune fonctionnalité `llvm`, aucun helper, aucune bibliothèque
externe ne sont nécessaires.**

Commande de build release pour cette voie :

```sh
cd /home/clausake/Stefan/ROSE/rose
export PATH="$HOME/.cargo/bin:$PATH"        # si cargo n'est pas déjà dans PATH
cargo build --release --bin rose
```

Mesure faite sur l'arbre en cours de réécriture :

```
$ ./rose aot /tmp/opencode/simple.rose -o simple_aot
rose: simple_aot produit (exécutable natif aarch64, code de retour = valeur du programme)
$ ./simple_aot ; echo $?
42
```

Limite mesurée : un programme récursif (`fib(12)`) est refusé par
`natif_emetteur` (« le programme ne rend pas de valeur calculable
directement » / appels non émettables). C'est une limite du sous-ensemble
machine, pas de LLVM ; à re-mesurer quand la vague 76 sera terminée.

## 2. Voie LLVM ORC (mesurée avant la réécriture vague 76)

Cette voie fonctionnait aussi, mais exigeait `--features llvm` et une
bibliothèque LLVM partagée à l'exécution.

### Disponibilités mesurées

| Fait | Preuve |
|---|---|
| LLVM installé | `llvm-config --version` et `llvm-config-18 --version` → **18.1.3** ; `/usr/lib/llvm-18/bin/llvm-config` présent |
| Rust | `rustc -vV` → **1.99.1**, hôte `aarch64-unknown-linux-gnu` |
| Dépendance Rust | `Cargo.toml` : `inkwell = { version = "0.7", optional = true, features = ["llvm18-1-prefer-dynamic", ...] }` → **inkwell 0.7.1** |
| Build AOT LLVM | **OK** (51,51 s) avec la commande ci-dessous |

```sh
cd /home/clausake/Stefan/ROSE/rose
export PATH="$HOME/.cargo/bin:$PATH"
RUSTFLAGS='-C target-feature=-crt-static' cargo build --release --features llvm --bin rose
```

`RUSTFLAGS='-C target-feature=-crt-static'` était nécessaire ici :
`rose/.cargo/config.toml` impose `-C target-feature=+crt-static` sur
`aarch64-unknown-linux-gnu`, ce qui casse la proc-macro `inkwell_internals` :

```
error: cannot produce proc-macro for `inkwell_internals v0.12.0` as the target
       `aarch64-unknown-linux-gnu` does not support these crate types
```

Le drapeau `RUSTFLAGS` est ajouté après ceux de la configuration, donc il
annule `+crt-static` pour cette compilation. Aucun script de build ni
`.cargo/config.toml` n'a été modifié.

### Résultat mesuré

```
$ ./rose aot /tmp/opencode/fib.rose -o fib.o --link
rose: fib.o produit (objet natif via LLVM ORC)
rose: fib_aot produit (binaire exécutable lié)
$ ./fib_aot
144
```

Sans `--link`, cette voie ne produisait que l'IR texte (`sortie.ll`).

`src/aot_llvm_orc.rs` a reçu une branche `Instr::Rir(_) => Err(...)` : le
front-end produit une variante `Rir` que le compilateur ORC ne gérait pas
(`error[E0004]` au build `--features llvm`).

### Limite de la voie LLVM

Le code Rust d'inkwell est dans le binaire, mais la fonctionnalité
`llvm18-1-prefer-dynamic` laisse une dépendance dynamique :

```
$ ldd target/release/rose | grep -i llvm
    libLLVM.so.18.1 => /lib/aarch64-linux-gnu/libLLVM.so.18.1
```

C'est précisément ce que la voie 1 (émetteur machine intégré) supprime.

## 3. Transpilation C — inchangée, sans Python

`rose llvm` utilise le transpileur Rust intégré (`src/transpile.rs`, même API
que `rose transpile --cible c`) :

```
$ ./rose llvm /tmp/opencode/fib.rose -o fib.c
rose: fib.c produit (code C)
$ cc -O2 fib.c -o fib_c -lm && ./fib_c
144
$ ./rose llvm /tmp/opencode/fib.rose -o fib2.c --exe
rose: fib2 produit (exécutable)
$ ./fib2
144
```

`--exe` cherche un compilateur C déjà installé (`cc`, `gcc`, `clang`). S'il n'y
en a aucun, le fichier `.c` est rendu et l'absence est expliquée. Le C émis
contient bien `int main(void)` et se compile avec le compilateur présent.

## 4. Validation commune

- `cargo build --release --bin rose` : **OK**.
- `cargo build --release --features llvm --bin rose` (avec le `RUSTFLAGS`
  ci-dessus, avant la réécriture vague 76) : **OK**.
- `cargo test --release` : **0 échec** (275 tests lib, 263 tests bin, binaires
  annexes OK).
- `rose --help` : aucune taille, aucun octet, aucun nom d'outil de compilation.
- Point fixe `b3ed9262dd54776a6dbd07bc12368d0e` ×2 :
  `rose rir compilateur/emetteur.rose` et
  `rose compilateur/emetteur.rose compilateur/emetteur.rose`, 263 520 octets,
  `cmp` = 0.
