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
complet inclus, meilleur de 15.

| Moteur | Temps |
|---|---|
| Rust (`-O`) | 0,90 ms |
| C++ (`-O3`) | 1,90 ms |
| **Rose (JIT Cranelift)** | **2,90 ms** |
| Python (CPython) | 256 ms |
| Rose (interprète) | 700 ms |

Les trois versions C++ / Rust / Python expriment le **produit complexe
général**, comme le programme Rose — sinon la comparaison ne serait pas
équivalente.

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

## 📜 Licence

MIT — voir [LICENSE](LICENSE).

---

<p align="center">
  <b>ROSE</b> — conçu et implémenté par <a href="https://github.com/Stef302">Stef302</a>
</p>
