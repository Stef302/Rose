# 🌹 Rose

> **Le langage compilé en français qui se compile lui-même.**
> Interprète exact · JIT Cranelift · AOT LLVM · auto-hébergement.

![Rose banner](docs/assets/img/og-rose.png)

<p align="center">
  <img src="https://img.shields.io/badge/version-5.0.0-E6202A?style=for-the-badge" alt="version">
  <img src="https://img.shields.io/badge/moteur-Rust%20%2B%20Cranelift%20%2B%20LLVM-000000?style=for-the-badge" alt="moteur">
  <img src="https://img.shields.io/badge/licence-MIT-8CE026?style=for-the-badge" alt="licence">
  <img src="https://img.shields.io/badge/parité%20C++-54%2F54-8CE026?style=for-the-badge" alt="parité">
  <img src="https://img.shields.io/badge/bench%20global-0.09s-8CE026?style=for-the-badge" alt="bench">
</p>

---

## ✨ En une phrase

Rose est un langage de programmation en français, à typage dynamique, avec arithmétique
exacte (entiers arbitraires, rationnels), qui se compile lui-même. Son moteur est écrit
en Rust : interprète exact, JIT Cranelift (compilation à chaud), AOT LLVM (`rose llvm`),
FFI dlopen, bundle AOT (`comp`). Parité parfaite avec le moteur C++ de référence.

## 🚀 Démarrage rapide

```bash
# Linux (x64 / arm64)
cd rose && cargo build --release
cp target/release/rose-20 /usr/local/bin/rose
rose mon_programme.rose
```

```rose
>> "Bonjour, Rose !"
$total = 0
pour i de 1 vers 1000001 { $total = $total + i * i }
>> $total
```

## 📊 Benchmarks

`bench/global/bench_global.rose` (arithmétique, flottants, rationnels, chaînes,
listes, tables, récursion, RNG, fichiers) :

| Moteur | Temps | Notes |
|---|---|---|
| **Rust rose (v5.0)** | **0,09 s** | JIT Cranelift + boucles accumulateur natives |
| C++ (référence) | 0,19 s | JIT x64 + GMP |
| Rust ancien (GMP) | 5,10 s | obsolète |
| Python (archivé) | n/a | non exécutable sur ce bench |

Comparatif détaillé (`comparatif_moteurs.sh`) :

| Domaine | C++ | Rust rose |
|---|---|---|
| boucle 10 M | 0,037 s | **0,023 s** |
| fib(28) | 0,043 s | **0,016 s** |
| big (factorielle/puissance) | 0,002 s | 0,014 s |
| crible 5 M | 0,013 s | 0,031 s |

## 🧮 Constantes irrationnelles exactes

Aucune valeur n'est figée : `pi(n)`, `e(n)`, `phi(n)` et `racine(n)` calculent
à la demande, avec la formule exacte affichée en LaTeX et une approximation
suivie de `≈`.

| Constante | Algorithme | 100 000 décimales | 1 000 000 décimales |
|---|---|---|---|
| `pi(n)` | Chudnovsky, **scission binaire** | 145 ms | **5,3 s** |
| `e(n)` | Taylor, scission binaire | 28 ms | 722 ms |
| `phi(n)` | √5 par racine entière (Karatsuba) | 97 ms | 3,0 s |

```
>> pi(20)
π = 4\cdot\arctan(1/5) - \arctan(1/239)   π ≈ 3.14159265358979323846
>> e(20)
e = \sum_{k=0}^{\infty} \frac{1}{k!}   e ≈ 2.71828182845904523536
>> phi(20)
φ = \frac{1 + \sqrt{5}}{2}   φ ≈ 1.61803398874989484820
```

## ⚡ Vitesse du JIT

Banc complexe (`bench/suite/run_bench.sh`) : 1 000 000 d'itérations, processus
complet inclus, meilleur de 40 entrelacés.

| Moteur | Temps |
|---|---|
| Rust (`-O`) | ~0,9 ms |
| C++ (`-O3`) | ~1,7 ms |
| **Rose (JIT Cranelift)** | **~1,5–2,6 ms** |
| Python (CPython) | 256 ms |
| Rose (interprète) | ~700 ms |

Le goulot de Rose est le **démarrage** (chargement + JIT, ≈ 0,5 ms), qui a déjà
été réduit : le lien statique (`target-feature=+crt-static`) a supprimé
≈ 0,3 ms, et le démarrage seul de Rose (0,32 ms) est maintenant au niveau de
Rust (0,35 ms). Le reste de l'écart (vectorisation NEON de LLVM que Cranelift
ne produit pas encore) est le prochain chantier.

Les trois versions C++ / Rust / Python expriment le **produit complexe
général**, comme le programme Rose — sinon la comparaison ne serait pas
équivalente.

## 🌐 WebAssembly — Rose sur le web

Rose se compile en **module WebAssembly** pour s'exécuter dans le navigateur
ou dans tout runtime sans système de fichiers :

```bash
# Prérequis (une seule fois)
rustup target add wasm32-unknown-unknown

# Construit le module .wasm (profil LTO, 579 Ko)
cargo build --profile wasm-release --target wasm32-unknown-unknown --no-default-features

# Le .wasm est prêt : target/wasm32-unknown-unknown/wasm-release/rose.wasm
```

Le moteur est une **bibliothèque** (`src/lib.rs`) : en WASM le JIT Cranelift
(émetteur de code machine natif) est remplacé par un shim d'API identique, et
le programme s'exécute en mode **interprété** (mêmes résultats, mêmes messages
d'erreur, ≈ 40× plus lent que le JIT natif). Toutes les fonctions de Rose sont
disponibles sauf l'accès fichiers, le réseau, l'horloge et l'entrée clavier.

Le module expose une interface C ABI (`web/rose-wasm.js`) et le LaTeX :

```js
await Rose.ready()
const out = await Rose.execute('pour j de 0 vers 3 { >> j }')
console.log(out)   // formule Σ + 0 1 2
const tex = await Rose.latex('>> 1 + 1')   // \mathrm{afficher}\left(2\right)
```

Voir `docs/WASM.md` (prérequis, build, ABI, limitations) et `web/` (démo
navigateur, glue JS, tests Node).

## ⚛️ Formule exacte en LaTeX

Chaque affichage `>>` est précédé de la **formule exacte** du programme en
LaTeX, suivie du résultat (convention déjà en place pour π, φ et e) :

```
>>> $s = 0
>>> pour j de 0 vers 10 { $s = $s + j }
>>> $s
\sum_{j = 0}^{9} \langle s = s + j\rangle
s   45
>>> 2 * pi
2 * pi   6.283185307179586
>>> 3 + 4i
3 + 4\mathrm{i}   5
```

La formule est générée à partir de l'AST (module `src/latex.rs`) par :
`programme_latex`, `expr_latex` et `instr_latex`. Elle est produite de façon
**identique par le JIT et l'interpréteur** (parité prouvée par campagne de
tests). Pour `pour j de`, la borne finale est exclusive : `pour j de 0 vers 3`
rendra `Σ_{j=0}^{2}`.

## 📦 Chaîne AOT LLVM « à la demande »

Le binaire par défaut est **léger** : 4,8 Mo, aucune dépendance à `libLLVM`,
démarrage en 1 ms. La chaîne AOT (`rose orc`, `rose link`) exige LLVM : le
moteur la **cherche tout seul**.

1. le helper `rose-aot` est cherché à côté de l'exécutable, dans
   `~/.cache/rose/`, puis dans le `PATH` ;
2. s'il est absent, il est **téléchargé** depuis la release du projet,
   vérifié par **SHA-256**, installé dans le cache, puis exécuté.

```bash
rose orc programme.rose -o programme.ll   # télécharge l'AOT si besoin
```

L'URL et l'empreinte de référence se surchargent par `ROSE_AOT_URL` et
`ROSE_AOT_SHA256` (miroir interne). Pour embarquer LLVM dans le binaire :

```bash
cargo build --release --features llvm        # binaire + helper AOT
```

### `rose orc` : fonctions utilisateur prises en charge

Depuis v5.8.0, `rose orc` compile les **fonctions définies par le programme**
(appel croisé, récursion, boucles `pour` / `tant_que` avec phi nodes) : les
en-têtes sont déclarés avant les corps, et l'objet émis se lie directement
avec `gcc`/`clang`. Un appel de DLL « extern » reste résolu par le linker.

```bash
rose orc fib.rose -o fib.o && gcc fib.o -o fib && ./fib
```

## 🧪 Conformité

- **54/54** comparaisons parité C++ ↔ Rust, **0 divergence**.
- **46/46** tests dorés (moteur).
- Campagnes génératives sans divergence.
- Arithmétique exacte illimitée (num-bigint, zéro GMP).

## 🏗️ Architecture

```
front.rs  → analyseur (lexer + parser + vérificateur + RIR)
interp.rs → interprète exact (repli du JIT)
jit.rs    → JIT Cranelift (compilation à chaud, taggé i64 + promotion BigInt)
run.rs    → runtime, FFI, affichage, helpers taggés
i128x.rs  → arithmétique exacte (BigInt natif, façade gmp_*)
```

Trois voies d'exécution, une seule sortie :
1. **Interprète** — exact, repli universel.
2. **JIT Cranelift** — compilation à chaud des fonctions entiers/flottants/booléens.
3. **AOT LLVM** — `rose llvm` produit un binaire natif.

## 📁 Structure

```
rose/           moteur Rust (source)
rose-cpp/       moteur C++ de référence (GMP)
compilateur/    front-end écrit en Rose
docs/           site GitHub Pages
installateur/   installeur Windows (NSIS)
bench/          bancs de référence
rose-cpp/tests/ suites (courir.py, parite.py)
```

## 🔧 Compiler depuis les sources

```bash
cd rose
cargo build --release
# binaire : rose/target/release/rose-20
```

## 📖 Syntaxe Rose — référence complète

Rose est un langage de programmation français. Voici toute la syntaxe,
pour un développeur ou une IA qui écrit du Rose.

### Types

| Type Rose | Ensemble mathématique | Description |
|---|---|---|
| `entier` | `ℤ` | entier exact (BigInt natif, illimité) |
| `rationnel` | `ℚ` | fraction exacte `a/b` |
| `flottant` | `ℝ` | flottant IEEE 754 double |
| `booleen` | `{vrai, faux}` | booléen |
| `chaine` | — | chaîne de caractères |
| `complexe` | `ℂ` | nombre complexe `a+bi` |
| `irrationnel` | `√n` | racine carrée exacte |
| `liste` | — | liste dynamique |
| `table` | — | table associative |
| `tampon` | — | tampon d'octets |
| `vide` | — | absence de valeur |
| `?` (Dyn) | — | type dynamique (inféré) |

### Variables et constantes

```rose
$x = 42                # entier
$pi = 3.14159          # flottant
$z = 3 + 4i            # complexe
$ch = "bonjour"        # chaîne
$t = table()           # table
$l = liste(1, 2, 3)    # liste

# Constantes irrationnelles exactes (avec formule LaTeX)
>> pi                  # π = 4·arctan(1/5) - arctan(1/239)   π ≈ 3.14…
>> e                   # e = Σ 1/k!   e ≈ 2.718…
>> phi                 # φ = (1+√5)/2   φ ≈ 1.618…
>> racine(2)           # √2
```

Les types sont **optionnels** pour les variables (`$x = 42` infère entier). Le
symbole `i` (minuscule) désigne l'imaginaire : `3 + 4i`, `2*i`, `modulus(z)`.

### Opérateurs

```rose
$a + $b   $a - $b   $a * $b   $a / $b   $a % $b   $a ^ $b
$a == $b  $a != $b  $a < $b  $a <= $b  $a > $b  $a >= $b
$a et $b  $a ou $b  non $a
```

### Affichage et saisie

```rose
>> expr1, expr2, …     # affiche (formule LaTeX + résultat)
<< nom                  # saisie (lire)
```

### Structures de contrôle

```rose
si (cond) { … }                    # si
si (cond) { … } sinon { … }        # si/sinon
? (cond) [ … ]                     # raccourci si
@ (cond) [ … ]                     # boucle tant_que
tant_que (cond) { … }              # boucle tant_que
pour i de 0 vers 10 { … }          # boucle pour (borne exclusive)
pour i de 0 vers n+1 { … }        # borne calculée
```

### Fonctions

```rose
# Types optionnels (paramètres et retour)
fn carre(x) { retour x * x }
fn double(x: entier) -> entier { retour x * 2 }
fn demi(x) { retour x / 2.0 }      # retour flottant inféré
fn fib(n: entier) -> entier {
    si (n < 2) { retour n }
    retour fib(n - 1) + fib(n - 2)
}
```

### Commentaires

```rose
# commentaire jusqu'à la fin de la ligne
```

### FFI (appel de bibliothèques C)

```rose
@dll("m") @sym("pow")
fn pow_libm(x: flottant, y: flottant) -> flottant

@dll("m") @sym("sqrt")
fn racine(x: flottant) -> flottant

>> pow_libm(2.0, 10.0)          # 1024
```

### Affichage LaTeX automatique

Chaque `>> expr` affiche la **formule exacte en LaTeX formée du programme**
suivie du résultat :

```
>>> $s = 0
>>> pour j de 0 vers 10 { $s = $s + j }
>>> $s
\sum_{j = 0}^{9} \langle s = s + jangle
s   45
>>> 2 * pi
2 * pi   6.283185307179586
>>> 3 + 4i
3 + 4\mathrm{i}   5
>>> carre(7)
\operatorname{carre}(7)   49
```

La commande `rose latex prog.rose` rend le **programme entier** en LaTeX
(fonctions comprises, avec les signatures `n \in \mathbb{Z}`).

### Compilation native

```bash
# JIT (par défaut, Cranelift)
rose prog.rose

# AOT LLVM ORC (vectorisation NEON, parité avec Rust)
rose orc prog.rose -o prog.o && gcc -O3 -o prog prog.o -lm

# WebAssembly (pour le web)
cargo build --profile wasm-release --target wasm32-unknown-unknown --no-default-features
```

### Règles importantes pour l'IA / développeur

1. Le symbole `i` (minuscule) est l'imaginaire ; la variable de boucle
   `i` partage le même jeton — dans `pour i de …` et `tant_que (i < n)`,
   `i` est la VARIABLE entière.
2. `pour j de A vers B` est **exclusif** : `pour j de 0 vers 3` itère
   j = 0, 1, 2 (borne finale B-1).
3. Les types sont optionnels pour les paramètres et le retour.
4. `>>` affiche la formule LaTeX + le résultat.
5. `pi`, `phi`, `e` sont des constantes irrationnelles exactes.
6. `racine(n)` = √n exact.
7. `modulus(z)` = |z|.
8. `re(z)`, `im(z)`, `conj(z)` = parties réelle/imaginaire, conjugué.
9. Les entiers sont illimités (BigInt natif, zéro GMP).
10. Le moteur tourne dans un thread à pile de 8 Mo (récursion profonde).

---

## 📜 Licence
## 📜 Licence

MIT — voir [LICENSE](LICENSE).

---

<p align="center">
  <b>ROSE</b> — conçu et implémenté par <a href="https://github.com/Stef302">Stef302</a>
</p>

## 🎮 Modes d'exécution (récapitulatif)

| Commande | Rôle | Quand l'utiliser |
|---|---|---|
| `rose prog.rose` | Interprète / JIT le programme | défaut, avec formules LaTeX |
| `rose run prog.rose` | Identique | alias explicite |
| `rose run prog.rose` + `ROSE_AOT=1` | AOT LLVM ORC si disponible | vitesse maximale (≈ Rust) |
| `rose latex prog.rose` | Formule LaTeX du programme entier | jouer avec le code, puis obtenir le LaTeX |
| `rose comp prog.rose [-o sortie]` | Exécutable autonome (moteur + source) | distribution |
| `rose orc prog.rose -o prog.o` | Compilation native LLVM ORC | AOT, vectorisation NEON |
| `rose link prog.o [-o sortie]` | Lier un objet ELF en binaire | avec `rose orc` |
| `rose info` / `rose version` | État du moteur | diagnostic |
| `rose --help` | Aide | — |

Variables d'environnement utiles :
- `ROSE_SANS_JIT=1` : force l'interpréteur pur (sans JIT Cranelift).
- `ROSE_AOT=1` : active la voie AOT LLVM ORC (vectorisation NEON).
- `ROSE_TIMING=1` : affiche les temps de compilation/exécution.
- `ROSE_DUMP_IR=1` : affiche l'IR Cranelift.
- `ROSE_NO_AOT=1` : désactive la voie AOT (force le JIT).
- `ROSE_DEBUG=1` / `ROSE_STEP=1` : débogage pas-à-pas.
