# G2 RÉEL — le binaire `rose` compile et tourne SANS Cranelift

**Constat daté : 18/11/2026, ASUS GX10 (aarch64), Rust 1.99.1, Rose 0.1.5, branche `main`.**
Toutes les sorties ci-dessous ont été réellement exécutées. Re-mesurer après tout changement
de `Cargo.toml`, de feature, de `jit.rs`, `lib.rs` ou `main.rs`.

---

## 0. Vérité courte

- **G2 dans le dépôt est FAIT** : `cranelift` est devenu une dépendance **optionnelle**,
  tirée uniquement par la feature `jit`. Le comportement **par défaut est inchangé**
  (`native` ⇒ `jit` ⇒ Cranelift).
- **`--no-default-features --features autonome` compile le binaire `rose` sans Cranelift** :
  `cargo tree -e features -i libc` rend **`libc features=[]`** et
  `cargo tree -e features -i cranelift` rend `error: package ID specification 'cranelift'
  did not match any packages` (rc=101).
- **Ce n'est PAS du `no_std`** : `std`, `libc` (sans features), `num-*`, `cc` restent
  présents. G2 = « zéro Cranelift », pas « autonomie système complète » (G3).
- **Ce n'est PAS « Rose autonome » au sens G4** : l'ELF émis par `link_natif` n'embarque
  toujours pas de runtime pour la récursion / les E/S (voir `AUTONOMIE_G2.md` §4).

---

## 1. Ce qui a été changé (4 fichiers possédés)

| Fichier | Changement |
|---|---|
| `rose/Cargo.toml` | `cranelift = { …, optional = true }` ; features `native = ["jit","binaire"]`, `jit = ["dep:cranelift"]`, `autonome = ["binaire"]`, `binaire = []`. La garde `required-features` du bin `rose` passe de `["native"]` à `["binaire"]` (les deux configurationsnatatives l'activent ; le wasm reste exclu). |
| `rose/src/lib.rs` | `pub mod jit;` n'est compilé que sous `all(not(wasm32), feature="jit")`. Ajout d'un shim `jit` de même API sous `all(not(wasm32), not(feature="jit"))`, plus `mod jit_ne` (copie locale de `natif_emetteur.rs`, déclarée au niveau `crate` pour la résolution de `#[path]`). |
| `rose/src/main.rs` | Même bascule `#[cfg(feature="jit")] mod jit;` / shim autonome. L'aide affiche le moteur réel (`MOTEUR` dépend de la feature). |
| `rose/docs/G2_REEL.md` | Ce document (nouveau). |

**Aucun commit, aucun autre fichier `.rs` modifié par ce travail.** `jit.rs` a été modifié
en parallèle par I1 (agent distinct) ; ses corrections ont été laissées intactes et deux
améliorations convergentes (refus des retours `Vide`, marqueur de repli) ont été alignées
dans le shim de `lib.rs`.

### Le shim autonome en deux voies

1. **Compilation native (par défaut)** — `Jit::compiler_tout()` parcourt les signatures
   dont le RIR a été abaissé et appelle `compiler_natif()` (émetteur M3.1 `natif_emetteur`,
   écrit en Rust par Rose, zéro Cranelift). Portes conservatrices : paramètres et retour
   `Entier`/`Booleen`, `peut_emettre`, `emettre` sans `appels` (donc sans récursion) ni
   `fallbacks`. Sous-ensemble couvert = boucles arithmétiques sur i64/booléens.
   `ROSE_G2_PROF=1` affiche la liste réelle des fonctions émises.
2. **Repli interpréteur** — toute autre fonction reçoit `Compile::Interprete`. Ce marqueur
   est essentiel : sans Cranelift, la voie RIR n'a ni trampoline ni délégués et rend des
   valeurs fausses (mesuré : `nb_premiers_inferieurs(100)` = 4 au lieu de 25, puis
   `Valeur::Vide` converti vers entier → erreur « ligne=90 »). Le RIR du **programme
   principal** est également désactivé (`ROSE_NO_RIR_PRINCIPAL=1` positionné dans
   `compiler_principal`) : sur un programme profond il sature la pile de slots RIR.
   L'interpréteur AST, lui, est la voie prouvée par le corpus.

---

## 2. Graphe de dépendances — preuve réelle

### 2.1 Configuration par défaut (`native` ⇒ `jit`) — Cranelift présent

```
$ cargo tree -e features -i libc
libc v0.2.191
└── rose v0.1.5 (/home/clausake/Stefan/ROSE/rose)
    ├── rose feature "binaire"
    │   └── rose feature "native"
    │       └── rose feature "default" (command-line)
    ├── rose feature "default" (command-line)
    ├── rose feature "jit"
    │   └── rose feature "native" (*)
    └── rose feature "native" (*)
├── libc feature "default"
│   ├── cranelift-jit v0.137.1
│   │   └── cranelift-jit feature "default"
│   │       └── cranelift v0.137.1
│   │           ├── cranelift feature "frontend" ...
│   │           ├── cranelift feature "jit" ...
│   │           ├── cranelift feature "module" ...
│   │           └── cranelift feature "native" ...
│   ├── memmap2 v0.9.12 ...
│   ├── region v3.0.2 ...
│   └── wasmtime-internal-jit-icache-coherence v50.0.1 ...
└── libc feature "std"
    └── libc feature "default" (*)
```

```
$ cargo tree -e features -i cranelift | head -6
cranelift v0.137.1
├── cranelift feature "frontend"
│   └── rose v0.1.5 (/home/clausake/Stefan/ROSE/rose)
│       ├── rose feature "binaire"
│       │   └── rose feature "native"
│       │       └── rose feature "default" (command-line)
```

### 2.2 Configuration autonome — **`libc features=[]`**

```
$ cargo tree -e features -i libc --no-default-features --features autonome
libc v0.2.191
└── rose v0.1.5 (/home/clausake/Stefan/ROSE/rose)
    ├── rose feature "autonome" (command-line)
    └── rose feature "binaire"
        └── rose feature "autonome" (command-line)
```

Aucune ligne `libc feature "default"` ni `"std"` : **c'est exactement l'état cible**
décrit dans `AUTONOMIE_G2.md` §3.2. Le graphe complet ne contient plus que
`libc`, `num-bigint`, `num-integer`, `num-traits`, `autocfg` (build) et `cc` (build).

```
$ cargo tree -e features -i cranelift --no-default-features --features autonome
error: package ID specification `cranelift` did not match any packages
rc=101
```

---

## 3. Builds

```
$ cargo build --release --bin rose
    Finished `release` profile [optimized] target(s) in 0.03s     (incrémental, cache)

$ cargo build --release --no-default-features --features autonome --bin rose
    Finished `release` profile [optimized] target(s) in 6.39s

$ cargo build --release --no-default-features --features autonome   # lib + bin
    Finished `release` profile [optimized] target(s) in ...
```

Tailles des binaires release (même cible, même profil `strip = true`) :

| Binaire | Taille |
|---|---|
| défaut (JIT Cranelift) | **4 932 328 octets** |
| autonome (zéro Cranelift) | **2 703 960 octets** |

`nm | grep -ci cranelift` = **0** sur les deux (profil `strip = true` : le tableau de
symboles est retiré, ce test n'est donc **pas probant**). Le test probant est `cargo tree`
ci-dessus. `strings | grep -ci cranelift` : **117** (défaut) contre **2** (autonome) ;
les 2 restants sont des messages de `src/interp.rs` (`rose info`), fichier non possédé ici.

---

## 4. Valeurs comparées (les deux binaires, mêmes sources)

Programme boucle `/tmp/g2_somme.rose` :
```
fn somme(n: entier) -> entier {
  $s = 0
  pour j de 1 vers n + 1 { $s = $s + j }
  retour $s
}
>> somme(10)
```

| Contrôle | Défaut (JIT) | Autonome | Égal ? |
|---|---|---|---|
| `somme(10)` | `55` | `55` | ✅ |
| `fib(25)` | `75025` | `75025` | ✅ |
| `fib(35)` | `9227465` (0,03 s) | `9227465` (21,31 s) | ✅ (valeur) |
| `jardins/tests/premiers.rose` | `20 tests, 0 échec` | `20 tests, 0 échec` | ✅ |
| Point fixe M4.5 — `rose rir compilateur/emetteur.rose` (md5) | `bdbef63681608a0b22a5637539e9d738` | `bdbef63681608a0b22a5637539e9d738` | ✅ |
| Point fixe M4.5 — `rose compilateur/emetteur.rose compilateur/emetteur.rose` (md5) | `bdbef63681608a0b22a5637539e9d738` | `bdbef63681608a0b22a5637539e9d738` (validé par l'orchestrateur après K2, 3 runs) | ✅ |

Preuve que la voie native est réellement active dans le binaire autonome :

```
$ ROSE_G2_PROF=1 ./g2-rose-final /tmp/g2_somme.rose
G2 natif : 1 fonction(s) sur 1 — ["somme"]
55
```

Sur le jardin, `G2 natif : 0 fonction(s) sur 7` : toutes les fonctions ont un appel RIR,
un `%` (fallback) ou un retour `Vide` → **repli interpréteur**, et le résultat est
néanmoins exact (20/20). C'est le comportement attendu.

---

## 5. Tests

```
$ cargo test --release                                   # défaut
test result: ok. 160 passed; 0 failed   (unittests src/lib.rs)
test result: ok. 144 passed; 0 failed   (unittests src/main.rs)
test result: ok.   0 passed; 0 failed   (rose-setup)
test result: ok.   6 passed; 0 failed   (rose-shell)
test result: ok.   1 passed; 0 failed   (tests/mul_cmp.rs)
test result: ok.   0 passed; 0 failed   (doc-tests)

$ cargo test --release --no-default-features --features autonome
test result: ok. 157 passed; 0 failed   (unittests src/lib.rs)
test result: ok. 141 passed; 0 failed   (unittests src/main.rs)
test result: ok.   6 passed; 0 failed   (rose-shell)
test result: ok.   1 passed; 0 failed   (tests/mul_cmp.rs)
test result: ok.   0 passed; 0 failed   (doc-tests)
```

(−3 tests de chaque côté : les tests internes de `jit.rs`, non compilé en mode autonome.
`rose-setup` n'est pas compilé en mode autonome : il est gardé par `required-features =
["native"]`, comportement inchangé.)

---

## 6. Limites honnêtes

1. **Le mode autonome est plus lent sur la récursion** : `fib(35)` 21,31 s contre 0,03 s
   (×710). Cause : pas de Cranelift, et le repli AST est volontairement préféré au RIR
   (voie RIR non sûre sans JIT, voir §1). La vitesse n'était pas un objectif de G2.
2. **G2 ≠ `no_std`** : `libc` est présent mais sans feature (`features=[]`) ; `std`,
   `num-*`, `cc` restent là. G3 reste à faire.
3. **G2 ≠ binaire auto-hébergé** : l'ELF statique de `AUTONOMIE_G2.md` ne contient pas de
   runtime pour `AppelRir`, les E/S ou la récursion. Le sous-ensemble AOT reste
   `somme(1..10) = 55` (exit 55).
4. **Deux chaînes `Cranelift` subsistent dans le binaire autonome** (`rose info`,
   `src/interp.rs`), fichier hors de notre possession. Aucun symbole ni dépendance.
5. **Le parallélisme d'agents** a modifié `jit.rs` pendant ce travail ; les builds
   défaut/autonome et tous les tests ont été re-mesurés **après** la dernière modification
   de `jit.rs` (15:20). Re-mesurer si I1/J1 reprend la main.

---

## 7. Reproduction

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cd /home/clausake/Stefan/ROSE/rose

# 1) graphe (la preuve)
cargo tree -e features -i libc --no-default-features --features autonome
cargo tree -e features -i cranelift --no-default-features --features autonome  # rc=101

# 2) builds
cargo build --release --bin rose
cargo build --release --no-default-features --features autonome --bin rose

# 3) valeurs
/tmp/opencode/g2-rose-final /tmp/g2_somme.rose              # 55
/tmp/opencode/g2-rose-final /tmp/vfib.rose                 # 75025
/tmp/opencode/g2-rose-final /home/clausake/Stefan/ROSE/jardins/tests/premiers.rose
ROSE_G2_PROF=1 /tmp/opencode/g2-rose-final /tmp/g2_somme.rose

# 4) tests
cargo test --release
cargo test --release --no-default-features --features autonome
```
