# 🌹 Rose

> **Le langage compilé en français qui se compile lui-même.**
> Interprète exact · JIT Cranelift · AOT LLVM · auto-hébergement.

![Rose banner](docs/assets/img/og-rose.png)

<p align="center">
  <img src="https://img.shields.io/badge/version-0.1.5-E6202A?style=for-the-badge" alt="version">
  <img src="https://img.shields.io/badge/moteur-Rust%20%2B%20Cranelift%20%2B%20LLVM-000000?style=for-the-badge" alt="moteur">
  <img src="https://img.shields.io/badge/licence-MIT-8CE026?style=for-the-badge" alt="licence">
  <img src="https://img.shields.io/badge/parité%20C++-54%2F54-8CE026?style=for-the-badge" alt="parité">
  <img src="https://img.shields.io/badge/bench%20global-0.09s-8CE026?style=for-the-badge" alt="bench">
</p>

---

## ✨ En une phrase

Rose est un **langage de programmation** en français, à typage dynamique, avec
arithmétique exacte (entiers arbitraires, rationnels, complexes, constantes
irrationnelles π/e/φ), **auto-hébergé** : ses maillons (lexer, analyseur,
vérificateur, émetteur RIR) sont réécrits en Rose et rendent la sortie de l'oracle
à l'octet près sur 104 fichiers.

Le moteur Rust n'est qu'**une** implémentation du langage — interprète exact, JIT
Cranelift (compilation à chaud), AOT LLVM (`rose llvm`, ORC), FFI dlopen, bundle
AOT (`comp`), module WASM — à côté du moteur C++ de référence (parité 54/54).

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
| **Rust rose (v0.1.3)** | **0,09 s** | JIT Cranelift + boucles accumulateur natives |
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

### Factorielle : le million ! en ~0,9 s

`factorielle(1000000)` (5 565 709 chiffres) est calculé par facteurs premiers
(Légendre) + **arbre de produit glouton multi-cœur** — le même algorithme que
GMP, pur Rust, sans FFI :

| Moteur | 1 000 000 ! | 100 000 ! |
|---|---|---|
| **Rose** (Légendre + arbre Huffman + NTT maison) | **200 ms** | **25 ms** |
| Rust pur, même algorithme (`num-bigint`) | 2 231 ms | 61 ms |
| GMP/C (référence) | 169 ms | 8,2 ms |
| Rose avant la NTT | 3 797 ms | 41 ms |

Rose est **11× plus rapide que Rust pur** sur `1 000 000!`, grâce à sa NTT
(`rose/src/mul_rapide.rs`) que `num-bigint` n'a pas, à son arbre de produit
équilibré et à sa conversion décimale par multiplication. GMP reste **1,18×
plus rapide** sur le calcul : sa FFT de style Fermat (moduli `2^k+1`, dont
les papillons ne sont que des rotations — *aucune* multiplication modulaire)
reste plus rapide que notre NTT de Montgomery. Détail complet des mesures
et de ce qui a été essayé (et abandonné) plus bas, à la section
« Ce que Rose fait pour être rapide ».

Et pour les nombres géants, `view(x)` ouvre une visionneuse paginée dans le
terminal (façon nano) : `[Espace]` page suivante, `b` page précédente, `q`
pour quitter.

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
π = 4\cdot\arctan(1/5) - \arctan(1/239)
π ≈ 3.14159265358979323846
>> e(20)
e = \sum_{k=0}^{\infty} \frac{1}{k!}
e ≈ 2.71828182845904523536
>> phi(20)
φ = \frac{1 + \sqrt{5}}{2}
φ ≈ 1.61803398874989484820
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

L'exécution affiche la **valeur seule** ; le LaTeX est rendu **à la
demande**, par `rose latex` (programme entier) ou par les lignes marquées
`%%` :

```
>>> $s = 0
>>> pour j de 0 vers 10 { $s = $s + j }
>>> $s
45
>>> 3 + 4i
5
>>> n = 3*x - 7
```

⚠ **Seule exception** : les constantes irrationnelles `pi`, `phi`, `e`
écrivent leur formule exacte **sur la ligne précédente** (une ligne de
LaTeX, comme une ligne `%%`), puis leur valeur approchée :

```
>>> phi
φ = \frac{1 + \sqrt{5}}{2}
φ ≈ 1.61803398874989484820
```

C'est le moteur C++ de référence qui imprime déjà cette ligne : la parité
avec lui est ainsi préservée à l'octet près.

```
$ rose latex prog.rose
n = 10 \\
\operatorname{afficher}(n!) \\
\operatorname{afficher}(1024) \\
\operatorname{afficher}(\frac{1}{3}) \\
\operatorname{afficher}(\varphi)
```

⚠ La sortie d'exécution ne porte **aucune** formule LaTeX pour les
expressions calculées (factorielle, binaire, appel utilisateur, index…) :
`latex::formule_composee` rend `false` pour toute expression, et le
prédicat est partagé par l'interpréteur et le JIT (parité garantie). Les
constantes π, φ, e font exception par leur affichage propre (ligne de
formule puis valeur, cf. ci-dessus).

La formule est générée à partir de l'AST (module `src/latex.rs`) par :
`programme_latex`, `expr_latex` et `instr_latex`. Pour `pour j de`, la borne
finale est exclusive : `pour j de 0 vers 3` rendra `Σ_{j=0}^{2}`.

## 📦 Chaîne AOT LLVM « à la demande »

Le binaire par défaut est **léger** : 4,8 Mo, aucune dépendance à `libLLVM`,
démarrage en 1 ms. La chaîne AOT (`rose orc`, `rose link`) exige LLVM : le
moteur la **cherche tout seul**.

1. le helper `rose-aot` (Windows : `rose-aot.exe`) est cherché à côté de
   l'exécutable, dans `~/.cache/rose/`, puis dans le `PATH` ;
2. s'il est absent, il est **téléchargé** depuis la release du projet,
   vérifié par **SHA-256**, installé dans le cache, puis exécuté.

Sur Windows, l'installeur ne pose que `rose.exe` et `rose-shell.exe` :
`rose-aot.exe` n'est **pas** livré — le stub compilé ici n'embarque pas LLVM
(il pèserait 432 Ko pour ne rien compiler). C'est `rose.exe` qui gère
`orc` et `link`, et qui télécharge le **vrai** helper LLVM (2,1 Mo, avec
LLVM) dans le cache au premier usage.

```bash
rose orc programme.rose -o programme.ll   # télécharge l'AOT si besoin
```

L'URL et l'empreinte de référence se surchargent par `ROSE_AOT_URL` et
`ROSE_AOT_SHA256` (miroir interne). Pour embarquer LLVM dans le binaire :

```bash
cargo build --release --features llvm        # binaire + helper AOT
```

### `rose orc` : fonctions utilisateur prises en charge

`rose orc` compile les **fonctions définies par le programme**
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
installateur/   installeur Windows (Velopack : `.nupkg` + `Rose-win-Setup.exe`)
bench/          bancs de référence
rose-cpp/tests/ suites (courir.py, parite.py)
```

## 🔧 Compiler depuis les sources

```bash
cd rose
cargo build --release
# binaire : rose/target/release/rose-20
```

Tests :

```bash
cd rose
cargo t --release        # alias de `test --bins` (111 tests)
```

> ⚠ N'utilisez pas `cargo test` nu sur x86_64 : `[lib]` déclare un
> `crate-type = ["cdylib", "rlib"]` (pour le module WebAssembly) et cargo
> exécute alors le cdylib comme harnais de test — une bibliothèque partagée
> n'a pas de point d'entrée, d'où un SIGSEGV avant le premier test. L'alias
> `cargo t` passe par les binaires, dont `src/main.rs` déclare les mêmes
> modules que `lib.rs`.

## 📖 Syntaxe Rose — référence complète

Rose est un langage de programmation en français. Cette référence contient
**toute la syntaxe** pour qu'un développeur ou une IA écrive du Rose du
premier coup. Un exemple vivant de tout cela : `rose exemples/tour.rose`.

### Types

| Type Rose | Ensemble | Description |
|---|---|---|
| `entier` | `ℤ` | entier exact (BigInt natif, illimité) |
| `rationnel` | `ℚ` | fraction exacte `a/b` — s'écrit `1/3` |
| `flottant` | `ℝ` | flottant IEEE 754 double — s'écrit `3.14` |
| `booleen` | `𝔹` | `vrai` / `faux` |
| `chaine` | — | texte **UTF-8 en caractères** (pas en octets) |
| `complexe` | `ℂ` | nombre complexe `3 + 4i` |
| `irrationnel` | — | racine carrée exacte `racine(2)` = √2 |
| `liste` | — | liste homogène `[1, 2, 3]` (un seul type) |
| `table` | — | table associative `t = table()` |
| `tableau` | — | tableau fixe `tableau(10)` |
| `tampon` | — | tampon d'octets `tampon(64)` |
| `vide` | — | absence de valeur |

Le type courant est retenu à la première affectation : les promotions
numériques passent (`$a = 2` puis `$a = 2.5` → flottant), mais écrire une
chaîne dans une variable numérique est refusé (`$n = 5` puis
`$n = "mot"` = erreur).

### Variables et littéraux

```rose
$n = 42                  # entier (illimité) : 100 ! a 158 chiffres
$f = 3.14                # flottant (point décimal)
$r = 1/3                 # rationnel exact (barre oblique)
$s = "bonjour"           # chaîne UTF-8
$b = vrai                # booléen (vrai / faux)
$z = 3 + 4i              # complexe — « i » minuscule = imaginaire
$l = [1, 2, 3]           # liste (un seul type par liste)
$t = table()             # table associative
	tab["cle"] = 7
$a = tableau(10)         # tableau de 10 cases
```

### Opérateurs

```rose
+  -  *  /  %  ^           # / = division exacte (7/2 → 3.5)
⋅  ·  ×  ÷  ≤  ≥  ≠  ≈     # variantes Unicode (* ≤ ≥ != ~)
et  ou  non                # logique (et/ou/non)
n!   2³  √9                # postfixés : factorielle, puissance, racine
n!!  n!!!  n!!!!           # MULTIFACTORIELLES (k = nombre de « ! »)
n#                         # primorielle : produit des premiers ≤ n   (10# → 210)
n?                         # termial de KUREPA : somme des factorielles 1!+…+n!   (5? → 154)
termial(n)                 # termial CLASSIQUE n(n+1)/2 (triangulaire)   (38 → 741)
termial_kurepa(n)          # Kurepa, écrit sans ambiguïté   (38 → 537169001220328488991089808037100875620940314)
n$                         # super-factorielle : 1!·2!·…·n!   (3$ → 12)
!n                         # PRÉFIXE : sous-factorielle (dérangements)  (!5 → 44)
```

| Opérateur | Effet | Exemple |
|---|---|---|
| `/` | division **exacte** : entier/entier → flottant affiché, `1/3` → rationnel | `7 / 2` → `3.5` |
| `÷` | division entière | `7 ÷ 2` → `3` |
| `%` | modulo (entiers) | `7 % 3` → `1` |
| `^` | puissance | `2 ^ 10` → `1024` |
| `≈` | comparaison approchée | `0.1 + 0.2 ≈ 0.3` → `vrai` |
| `==` `!=` `<` `<=` `>` `>=` | comparaison (nombres et chaînes) | |
| `n!` | factorielle ordinaire (A000142) | `5!` → `120` |
| `n!!` | **multifactorielle** d'ordre 2 (A000165/A001147) | `7!!` → `105` |
| `n!!!` | multifactorielle d'ordre 3 (A007661) | `8!!!` → `80` |
| `!n` | **sous-factorielle** (dérangements, A000166) | `!5` → `44` |

#### Les multifactorielles ne sont pas des factorielles répétées

`n!!` **n'est pas** `(n!)!`. C'est le produit des entiers ≤ n de même classe
modulo k, k étant le nombre de `!` :

```
n!⁽ᵏ⁾ = ∏_{i=0}^{⌊(n−1)/k⌋} (n − i·k)
```

| Écriture | Calcul | Valeur |
|---|---|---|
| `7!!` | 7·5·3·1 | **105** |
| `8!!` | 8·6·4·2 | **384** |
| `8!!!` | 8·5·2 | **80** |
| `9!!!` | 9·6·3 | **162** |
| `11!!!!` | 11·7·3 | **231** |
| `(7!)!` | factorielle **du** factoriel | 5040! (16 686 chiffres) |

`0!!` et `(−1)!!` valent 1 (produit vide), comme `0!` = 1.
Pour une factorielle répétée, écris `(n!)!` **avec les parenthèses**.

#### `!n` : deux significations, une seule retenue

Le même symbole `!n` désigne historiquement deux choses :

- la **sous-factorielle** (dérangements, A000166) : `!0`=1, `!1`=0, `!2`=1,
  `!3`=2, `!4`=9, `!5`=44 — c'est l'usage retenu par Rose ;
- la **factorielle gauche de Kurepa** (A003422) : 0, 1, 2, 4, 10, 34, 154…
  — c'est ce que Rose nomme **`n?`** (terminal), déjà présent avant.

Les deux sont donc accessibles, sans ambiguïté : `!n` = dérangements,
`n?` = Kurepa. Les fonctions nommées `sous_factorielle(n)` et
`multifactorielle(n, k)` donnent le même résultat quand k est calculé.

#### `n!` exact, `factorial_approx(n)` approximatif, `factorielle_rapide(n)` : les trois vitesses

Trois façons de calculer une factorielle, à ne pas confondre :

| Écriture | Ce qu'elle donne | Coût | Usage |
|---|---|---|---|
| `n!` | l'entier **exact**, tous les chiffres | O(n log²n) — arbre de produit | résultat final, vérification |
| `factorial_approx(n)` | `"m.eE"` — **Stirling** : √(2πn)·(n/e)^n, série asymptotique `1/(12n) − 1/(360n³) + 1/(1260n⁵)` | **O(1)** — quelques `f64` | savoir la **taille** (`100000!` ≈ `2.8e456573`) sans calculer |
| `factorielle_rapide(n)` | l'entier exact, par le même moteur crible+arbre | identique à `n!` | alias explicite (le nom `rapide`/`factorial_rapide` marche aussi) |

```rose
>> factorial_approx(100)     # 9.332622e157  (≈ 100! = 9.3326…×10¹⁵⁷)
>> nb_chiffres(100!)         # 158  — le nombre EXACT
```

⚠ `factorial_approx` **ne donne pas le nombre exact** : c'est une mantisse à
6 décimales et un exposant. Pour `n ≤ 170` un `f64` suffirait, mais Rose passe
toujours par Stirling — pour `n!` exact, utilise `n!`.

#### Ce que Rose fait pour être rapide (sans GMP)

Rose **ne dépend pas de GMP** : tout est en Rust pur, avec `num-bigint`.

| Optimisation | Effet mesuré |
|---|---|
| Suppression des clones d'opérandes (`mul`, `add`, `sub`) | `n!` 41 → 25 ms, `!n` 753 → 681 ms |
| Seuil de parallélisation de l'arbre (500 000 bits cumulés) | `n#` 2 → 1 ms ; en dessous les threads coûtent plus que le travail |
| **Arbre « fusion des deux plus petits »** (Huffman) au lieu de l'appariement adjacent | `1 000 000!` : 2 172 → 1 158 ms sur l'arbre seul (1,88×) |
| **NTT à 2 modules de 62 bits en Montgomery** | 1,74× sur la multiplication modulaire (1,68 → 0,96 ns/op) |
| **NTT DIF/DIT** (plus de permutation de bit-reversal) | 2,4× sur la NTT (460 → 225 ms à 2²⁰) |
| **NTT parallèle** (20 cœurs, un seul scope par multiplication) | NTT à 2²⁰ : 225 → 57 ms |
| **NTT four-step** (3 passes mémoire au lieu de log₂ n) | NTT à 2²⁰ : 78,6 → 57 ms |
| **Conversion décimale par multiplication** (au lieu de division) | `1 000 000!` : 2 881 → **365 ms** (7,9×) |
| **Conversion par niveaux** (un scope par niveau, pas un fil par multiplication) | 1 400 → 396 ms |
| **Répartition LPT des puissances p^e** | calcul : 224 → 200 ms |
| Bilan global | `1 000 000!` : calcul 906 → **200 ms**, total 3 787 → **565 ms** (6,7×) |

⚠ Ce que la mesure a montré : `num-bigint` fait **déjà** école → Half-Karatsuba
→ Karatsuba → **Toom-3** en place, et il est excellent. Notre propre Toom-3 est
4× plus lent que le sien. La seule chose qui lui manque est la **FFT** — d'où
`rose/src/mul_rapide.rs`.

#### Bench comparatif complet (N = 100 000, mêmes opérations partout)

| Opération | Rose | Rust pur (`num-bigint`) | Rust + `rug` (GMP) | GMP/C | GMP/C++ | Python |
|---|---|---|---|---|---|---|
| `n!` | 25 ms | 60,7 ms | 22,7 ms | 8,2 ms | 12,1 ms | 74 ms |
| `n!!` | 10 ms | 9,9 ms | 4,7 ms | 3,3 ms | 5,0 ms | 469 ms |
| `n!!!` | 7 ms | 5,8 ms | 2,9 ms | 1,9 ms | 3,1 ms | 210 ms |
| `n!!!!` | 5 ms | 3,1 ms | 2,0 ms | 1,3 ms | 2,2 ms | 119 ms |
| `n#` | 3 ms | 1,2 ms | 0,7 ms | 0,5 ms | 0,9 ms | — |
| `!n` | **722 ms** | 686 ms | 436 ms | 442 ms | 442 ms | 2959 ms |

À `1 000 000!` (5 565 709 chiffres) :

| Implémentation | Calcul | Conversion en chaîne | Total |
|---|---|---|---|
| **Rose** | **200 ms** | 365 ms | **565 ms** |
| Rust pur (`num-bigint`) | 2 231 ms | 2 881 ms | 5 112 ms |
| GMP/C++ (gmpxx) | 193 ms | 342 ms | 535 ms |
| GMP/C | 169 ms | 342 ms | 511 ms |

Lecture :

- **Rose bat Rust pur (`num-bigint`) de 11×** sur le calcul de
  `1 000 000!` et de **9× sur le total** (calcul + affichage) — grâce à la
  NTT, que `num-bigint` n'a pas, à l'arbre de produit équilibré et à la
  conversion décimale par multiplication ;
- **Rose bat Python de 3× (`n!`) à 47× (`n!!`)** ;
- ⚠ **Rose n'a PAS encore depassé GMP** : 1,18× sur le calcul, 1,07× sur la
  conversion, 1,11× sur le total. L'écart restant est structurel : GMP
  utilise une FFT de style **Fermat** (moduli `2^k+1`, dont les papillons
  ne sont que des rotations — *aucune* multiplication modulaire), alors que
  la nôtre fait une multiplication de Montgomery par papillon. C'est le
  chantier suivant si l'on veut franchir la barre.

⚠ **Ce qui a été essayé et ABANDONNÉ** (et pourquoi, mesures à l'appui) :

- **Inverse de Newton pour la division** : désormais **correct** (testé de
  40 à 1000 limbs contre `num-bigint`) mais **25× plus lent** à toutes les
  tailles mesurées (1000 limbs : 3,92 ms contre 169 µs). La division délègue
  donc à `num-bigint`. Les pièges rencontrés : la bonne itération est
  `v ← ⌊v·(2·B^p − d·v)/B^p⌋` (l'autre ne converge pas), le produit `d·v`
  ne doit **jamais** être tronqué, le ré-échelonnage entre paliers est
  `(m2−m)` limbs et non `2(m2−m)`, et l'amorçage doit être décalé de `B^(n−1)`.
- **Burnikel-Ziegler** : récursion infinie quand le demi-limb ne décroît plus.
  ⚠ À noter : `num-bigint` a DÉJÀ Burnikel-Ziegler (seuil 64 limbs), en plus
  de Knuth, Toom-3 et Karatsuba. Il ne lui manque que la FFT.
- **Réduction de Barrett** : rejetée (μ ne couvre pas un produit de 124 bits).
  Le `% u128` n'est que 1,8× plus lent qu'un `% u64` : ce n'est pas le goulot.
- **Une seule NTT par module** (c = a + i·b) : deux formes essayées, toutes
  deux fausses. La symétrie hermitienne ne s'applique pas dans un corps fini.
- **Seuil NTT** : balayé sur `1 000 000!` — 16,4 M bits est le meilleur
  (907 ms ; 6,14 M → 1024 ms ; ∞ → 912 ms).
- **Conversion décimale maison** : `num-bigint` fait déjà le même découpage
  récursif ; notre version est légèrement plus lente (3,07 s contre 2,88 s).
  Elle reste testée et disponible, non branchée.

Sources : `bench/suite/fact_gmp.c`, `fact_gmp.cpp` (gmpxx), `fact_rs.rs`
(Rust + `rug`), `fact_rs_pur.rs` (Rust pur), `bench_factorielles.sh`.

### Affichage et saisie

```rose
>> expr1, expr2        # affiche : les valeurs (LaTeX via `rose latex`)
<< entier_var          # saisit un entier au clavier
<< float_var           # saisit un flottant (la variable doit exister, typée)
```

### Structures de contrôle

```rose
si (cond) { … }                          # si
si (cond) { … } sinon { … }              # si / sinon
? (cond) [ instr ; instr ]               # raccourci « si » en une ligne
tant_que (cond) { instr ; instr ; … }    # boucle tant_que (« ; » entre instr.)
@ (cond) [ instr ; instr ]               # raccourci « tant_que » en une ligne
pour i de 0 vers 10 { … }                # boucle pour — borne FINALE EXCLUSIVE
pour k de 1 vers 11 { … }                # itère k = 1..10
```

⚠ Dans les corps `[ … ]` et `{ … }` raccourcis, les instructions sont
séparées par **`;`** (pas `,`).

### Fonctions — types FACULTATIFS

```rose
# Forme allégée : ni type de paramètre ni type de retour
fn carre(x) { retour x * x }
>> carre(7)                      # 49

# Forme typée (réservée au JIT/AOT : types entier, booleen, flottant)
fn fib(n: entier) -> entier {
    si (n < 2) { retour n }
    retour fib(n - 1) + fib(n - 2)
}
>> fib(20)                       # 6765

# « -> » ou « → », les deux marchent
fn double(x: flottant) → flottant { retour 2.0 * x }

# Une fonction peut retourner n'importe quel type (chaîne, liste, table…)
fn saluer(nom) { retour "bonjour " + nom }
>> saluer("rose")
```

Le type du **paramètre et du retour est facultatif**. Quand il est déclaré,
le LaTeX l'affiche comme un ensemble : `fib(n ∈ ℤ) → ℤ` ;
quand il est omis, la signature est allégée : `carre(x)`.

### Listes et tables

```rose
$l = [1, 2, 3]
>> $l[1]                    # 2 (index depuis 0)
>> longueur($l)             # 3
joindre($l, 4)              # ajoute en fin → [1, 2, 3, 4]

$t = table()
$t["cle"] = 7               # affectation par clé
>> $t["cle"]
>> cle_existe($t, "cle")    # vrai
>> cles($t)                 # ["cle"]
>> valeurs($t)
supprimer($t, "cle")
inserer($t, "a", 1)         # équivalent à $t["a"] = 1
```

### Chaînes

```rose
$t = "été radieux"
>> longueur($t)               # 11 (caractères UTF-8, pas octets)
>> $t[0]                      # "é"
>> ordre("A")                 # 65 (point de code)
>> caractere(960)             # "π"
>> sous_chaine($t, 2, 4)      # "é " (début inclus, fin exclue)
>> concatener(["ab", "cd"])   # "abcd" (prend une LISTE)
```

### Constantes irrationnelles exactes

Aucune valeur n'est figée — elles sont calculées à la demande, avec leur
formule exacte en LaTeX et une approximation `≈` :

```rose
>> pi        # π = 4\cdot\arctan(1/5) - \arctan(1/239)  (ligne de formule)
             # π ≈ 3.14159…
>> e         # e = Σ 1/k!   e ≈ 2.71828…
>> phi       # φ = (1+√5)/2   φ ≈ 1.61803…
>> racine(2) # \sqrt{2}   √2
>> pi(1000)  # 1000 décimales (Chudnovsky, scission binaire)
```

### Fonctions standard

| Domaine | Fonctions |
|---|---|
| Nombres | `abs` `min` `max` `partie_entiere` `factorielle`(`fact`) `rapide`(`factorial_rapide`) `approx`(`factorial_approx`) `multifactorielle`(`n!!`) `sous_factorielle`(`derangement`, `!n`) `primorielle`(`n#`) `terminal`(`n?`) `super_factorielle`(`n$`) `fib`(`fibonacci`) `pgcd`(`gcd`) `ppcm`(`lcm`) `puissance`(`pow`) `nb_chiffres`(`digits`) `est_premier`(`is_prime`) `compter_premiers` |
| Gros nombres | `view(x)` — visionneuse paginée dans le terminal (façon nano) pour les nombres à des millions de chiffres |
| Exact | `pi(n)` `e(n)` `phi(n)` `racine(n)` `fraction(a,b)` |
| Types | `type(x)` → chaîne ("entier", "flottant", …) |
| Complexes | `re(z)` `im(z)` `conj(z)` `arg(z)` `modulus(z)` |
| Chaînes | `longueur`(`len`) `ordre` `caractere` `sous_chaine`(`sub`) `concatener(liste)` |
| Tables | `table` `map` `inserer` `cle_existe` `supprimer` `cles` `valeurs` |
| Listes | `joindre(l, x)` (ajoute x à la fin) |
| Hasard | `graine(n)`(`seed`) `hasard(n)` `prng(n)` `hasard_systeme(n)` `rng_materiel` |
| Entrée/sortie | `lire_texte(chemin)` `ecrire_texte(chemin, texte)` `ecrire_binaire` `argument(i)` `nb_arguments` |
| Temps | `temps` `millisecondes`(`ms`) `time` `duree(n)` |
| Divers | `afficher` `afficher_erreur` `quitter(n)` `tampon(n)` `octets(chaine)` `eval_expr(chaine)` |

### FFI (bibliothèques C partagées)

```rose
@dll("m") @sym("pow")
fn pow_c(x: flottant, y: flottant) -> flottant   # pas de corps = externe

>> pow_c(2.0, 10.0)          # 1024
```

### Commentaires

```rose
# tout ce qui suit « # » sur la ligne est ignoré
```

### Affichage : la valeur, et le LaTeX sur demande

`>> expr` affiche la **valeur seule** (`45`, `49`, `3628800`). La formule
LaTeX s'obtient par `rose latex prog.rose` (programme entier, fonctions
comprises, signatures `n ∈ ℤ` incluses) ou par les lignes `%%` — voir la
section « Formule exacte en LaTeX ».

```
>>> $s = 0
>>> pour j de 0 vers 10 { $s = $s + j }
>>> $s
45
>>> carre(7)
49
```

⚠ Exception : `pi`, `phi` et `e` affichent leur formule exacte devant leur
valeur approchée.

#### LaTeX des lignes choisies : le marqueur `%%`

Une ligne qui **commence** par `%%` (à blanc près) est une annotation TeX,
**ignorée par le lexer** comme un commentaire : le programme reste exécutable
tel quel.

```rose
%% $N!$ : la factorielle
$N = 100
%% $N!!$ : la multifactorielle, PAS $(N!)!$
>> $N!!
```

| Commande | Ce qu'elle rend |
|---|---|
| `rose latex prog.rose` | tout le programme |
| `rose latex prog.rose --lignes` (ou `-l`) | **seulement** les lignes `%%`, chacune suivie de `\\` |
| `rose latex prog.rose --apercu` (ou `-a`) | rendu **visible** (Unicode), sans LaTeX installé |
| `rose latex prog.rose --lignes --apercu` | les lignes `%%` en rendu visible |

Sortie de `--lignes` sur l'exemple :

```
$N!$ : la factorielle \\
$N!!$ : la multifactorielle, PAS $(N!)!$ \\
```

⚠ Le marqueur est `%%` **en début de ligne** — pas `$…$` : `$x$` seul
serait lu comme du code Rose (une variable). Pour VÉRIFIER le rendu sans
LaTeX installé, `--apercu` traduit les commandes en caractères Unicode :

```
$ rose latex prog.rose --apercu
n = 12
afficher(n!)
S = n · (n + 1)/2
afficher(φ)
afficher(√2 · √2)
```

Toute commande que l'aperçu ne connaît pas est affichée entre `« »`
(`«\inconnue»`) au lieu d'être perdue : rien n'est jamais silencieux.

⚠ `%` est l'opérateur modulo en Rose : sans la reconnaissance de `%%` **en
début de ligne**, le lexer voyait deux modulos et rejetait le programme
(« Attente d'une expression, trouvé '%' »).

La super-factorielle `n$` s'affiche `\operatorname{sf}(n)` : `$` est le
délimiteur des mathématiques en LaTeX, « `3$` » se lirait comme un prix.

### Console interactive (REPL)

Deux points d'entrée, **une seule implémentation** (`rose/src/repl.rs`) :

```bash
rose console                  # depuis le moteur
rose console prog.rose        # charge prog.rose, puis console
rose prog.rose --console      # exécute prog.rose, puis console (variables disponibles)

rose-shell                    # exécutable AUTONOME (installé à côté du moteur)
rose-shell prog.rose          # charge le programme, puis console
rose-shell prog.rose --sans-exec   # charge sans exécuter les >>
```

`rose-shell` est un **lanceur** (≈ 400 Ko, pas 7 Mo) : il localise `rose.exe`
installé — à côté de lui dans `bin\`, dans le `PATH`, ou via `ROSE_EXE` — et lui
délègue `rose console`. Le moteur n'est donc compilé qu'une fois, et le shell
hérite forcément de la version du moteur installé (aucune désynchronisation
entre un `rose.exe` 0.1.5 et un `rose-shell.exe` d'une autre version).

L'installeur dépose aussi un **profil Windows Terminal**
(`%LOCALAPPDATA%\Microsoft\Windows Terminal\Fragments\Rose\rose.json`) :
« Rose » apparaît dans le menu déroulant des onglets, avec son icône et son
titre d'onglet, sans toucher à `settings.json`. Le désinstalleur retire ce
fragment.

Édition de ligne réelle (console brute `ReadConsoleInputW`, sans dépendance) :

| Touche | Effet |
|---|---|
| ← / → | déplace le curseur |
| Home / End | début / fin de ligne |
| Retour arrière / Suppr | édite au milieu de la ligne |
| ↑ / ↓ | historique (le brouillon en cours est conservé) |
| Tab | complète le mot sous le curseur |
| Ctrl-C | annule la saisie (`^C`) |
| Ctrl-L | efface l'écran |
| Ctrl-Z | termine |
| Échap | efface la ligne |

**Compatibilité terminal** : Windows Terminal, conhost, PowerShell, cmd,
Git Bash, VS Code, ainsi que tout terminal ANSI sous Linux/macOS. Au
démarrage, `repl::preparer_console` pose `SetConsoleOutputCP(65001)` (UTF-8,
donc accents et symboles mathématiques corrects) et active
`ENABLE_VIRTUAL_TERMINAL_PROCESSING` / `ENABLE_VIRTUAL_TERMINAL_INPUT` : sans
cela, sur conhost, les accents sortent en mojibake et les codes couleur
s'affichent littéralement. Les couleurs se désactivent avec
`ROSE_NO_COLOR=1` et se forcent avec `ROSE_COLOR=1` (utile dans un pipe).

Commandes : `:help`, `:show`, `:reset`, `:load <fichier>`, `:save <fichier>`,
`:type <expr>`, `:latex`, `:quit`. Les blocs multi-lignes (accolades) sont
acceptés, et un `{` à l'intérieur d'un commentaire `#` ne prolonge pas la
saisie.

### Pièges connus (à lire avant d'écrire du Rose)

1. **`i` minuscule est l'imaginaire** (`3 + 4i`, `i * i` → `-1`).
   Le lexème `i` est partagé avec les variables : dans une boucle
   `pour i de …`, `i` devient la VARIABLE entière. Pour une variable
   compteur hors boucle `pour`, **préfère un autre nom** (`$compteur`) :
   `tant_que ($i > 0) { $i = $i - 1 }` fonctionne, mais dans un contexte
   ambigu `i` reste l'imaginaire.
2. **`pour … vers B` est exclusif** : `pour j de 0 vers 3` → j = 0, 1, 2.
3. **Listes homogènes** : le vérificateur exige un seul type par liste.
4. **Corps raccourcis `[ ]`** : instructions séparées par `;` (jamais `,`).
5. **`<<` saisie** : la variable doit exister au préalable pour un type
   autre qu'entier ; `<< x` lit un entier si `x` est inconnue.
6. **`/` est la division exacte** ; la division entière s'écrit `÷`.
7. **Typage fort dynamique** : promotions numériques OK, mais chaîne ↔
   nombre refusé (`$n = 5` puis `$n = "mot"` = erreur).
8. **Fonctions typées = voie rapide** : les fonctions dont paramètres/retour
   sont `entier`/`flottant`/`booleen` passent par le JIT puis l'AOT ;
   les fonctions non typées ou à retour complexe/chaine restent interprétées.
9. **Backtick interdit** dans les noms ; les accents sont permis.
10. Le moteur tourne dans un thread à pile de 8 Mo (récursion profonde OK).

---

## 🧰 Skill Rose

Le skill complet pour les agents IA est publié dans un dépôt dédié :

**[`Stef302/Rose-skill`](https://github.com/Stef302/Rose-skill)** — `rose-skill.zip`
(23 Ko) à décompresser dans `.opencode/skills/`, ou le contenu en clair dans
`rose-dev/` :

- `SKILL.md` — règles, architecture, et les pièges mesurés (ressources PE,
  GDI+, ZLIB, PATH, colonnes console) ;
- `references/syntaxe.md` — **toute la syntaxe** Rose ;
- `references/cli.md` — la **ligne de commande** complète, le REPL, l'AOT ;
- `tour.rose` — exemple vivant de toute la syntaxe.

Le ZIP est aussi publié dans les ressources de la release `v0.1.5`.

## 📦 Installation

### Windows — installeur Velopack (remplace WiX et NSIS)

`Rose-win-Setup.exe` est l'installeur **Velopack** (assistant Tauri + paquets
`.nupkg`). Il remplace définitivement les installeurs WiX et NSIS.

- Installation **versionnée** dans `%LOCALAPPDATA%\Rose\app-<version>\` :
  le launcher `Update.exe` applique les mises à jour par **deltas binaires** et
  **revient à la version précédente** si la nouvelle ne démarre pas.
- **Aucun droit administrateur** : tout est écrit dans le profil utilisateur
  (`HKCU`, `%LOCALAPPDATA%`), compatible avec les stratégies d'entreprise qui
  verrouillent `HKLM`.
- Le moteur `rose.exe` et le shell `rose-shell.exe` sont **embarqués** dans le
  paquet ; l'installeur pose le PATH et les raccourcis, la désinstallation est
  gérée par Velopack. Le helper AOT LLVM n'est **pas** embarqué : `rose.exe`
  le télécharge au premier `rose orc` / `rose link`.
- **Trois exécutables Windows**, liés **statiquement** (aucune DLL
  `VCRUNTIME140` ni `api-ms-win-crt-*` requise, Windows 10/11 les lance
  directement) :
  - `rose.exe` — le moteur (console, PE32+ x86-64) ;
  - `rose-shell.exe` — le lanceur de la console interactive (délègue à `rose.exe`) ;
  - `Rose-win-Setup.exe` — l'installeur **GUI** (fenêtre Win32 dessinée à la
    main, `rose/src/bin/rose-setup.rs` + `rose/src/setup_gui.rs`) :
    - **fenêtre entièrement personnalisée** : `WS_POPUP` + `WS_THICKFRAME`,
      sans aucun cadre Windows 11 — barre de titre, boutons réduire / agrandir
      / fermer et bords de redimensionnement dessinés au GDI ;
    - gabarit de 1120 × 780 **mis à l'échelle de la fenêtre** (aucun texte
      rogné, aucun bouton coupé) ; en plein écran le ratio n'est plus imposé,
      sinon la barre d'état sortait sous le bord inférieur de l'écran ;
    - **manifeste `PerMonitorV2`** embarqué (`RT_MANIFEST` dans le `.rsrc`) :
      sans lui Windows étire la fenêtre en bitmap sur les écrans à 125 / 150 %,
      d'où le rendu pixélisé ; double-bufferisation + `ANTIALIASED_QUALITY` ;
    - **branding Kare** : marque « Rose », titres de section et libellés de
      boutons en Kare majuscule (police d'affichage), texte courant en Segoe UI ;
    - la rose et la police Kare embarquées dans une section `.rsrc`
      (PNG + TrueType + icônes 16 → 256 px + manifeste,
      `installateur/velopack/pack-res.py`) ; le groupe d'icônes `RT_GROUP_ICON`
      est posé sur la fenêtre, la classe et la barre des tâches ;
    - couleurs du site, et recherche automatique du moteur à poser
      (`payload\rose.exe`, `rose.exe`, ou l'installeur lui-même s'il a été
      renommé).
    Rendu de contrôle sous Linux : `installateur/velopack/previsualiser.py`
    (aperçu PNG des cinq vues, textes réels, couleurs GDI exactes, contrôle
    mesuré des boutons et des cartes) ; ressources validées par
    `installateur/velopack/verifier-res.py` (arbre `.rsrc`, PNG, police,
    icônes, groupe d'icônes et manifeste).
- **Builds produits depuis cette machine Linux** (`clang` + `lld-link` +
  SDK/UCRT de `cargo-xwin`), pas seulement sur Windows :
  `sh installateur/velopack/build-linux.sh` puis
  `sh installateur/velopack/pack-linux.sh` (paquet Velopack + manifeste).
- Chaîne de build Windows complète : `installateur/velopack/build.ps1` —
  `cargo tauri build` puis `vpk pack` (voir `installateur/velopack/README.md`).
- La voie de mise à jour est vérifiable **sans Windows** :
  `sh installateur/velopack/verifier.sh` (6/6).

### Windows / Linux — depuis le moteur

```bash
rose update              # menu : mettre à jour / réinstaller / réparer / désinstaller
rose update --force      # retélécharge même si la version est déjà à jour
rose update --velopack   # Windows : passe par Velopack (delta + rollback)
```

### Mesurer une lenteur — `rose debug-lenteur`

Un programme qui « met un certain temps avant d'afficher » peut perdre son
temps avant le moteur (lancement du processus, scan antivirus), dans la
préparation de la console, dans l'analyse ou dans l'exécution. Cet outil
ventile le temps au lieu de le deviner :

```bash
rose debug-lenteur              # mesures reproductibles (5 répétitions, médiane)
rose debug-lenteur prog.rose    # ajoute le programme qui vous intéresse
rose debug-lenteur -n 10        # plus de répétitions
```

Il chronomètre `rose version` (lancement pur), `rose info`, un programme
minimal, `rose latex` (analyse seule) et le programme demandé, puis donne la
MÉDIANE et un verdict : *le lancement coûte tant* (antivirus / démarrage) ou
*l'analyse* / *l'exécution* coûte tant. `ROSE_TIMING=1 rose prog.rose`
affiche aussi la ventilation interne (`entree_main`, `console`, `pret`,
`post_spawn`).

**« Mettre à jour »** interroge l'API GitHub (`releases/latest`), compare
la version publiée à celle installée, **télécharge l'artefact** de la
plateforme, **vérifie son SHA-256** contre `SHA256SUMS`, puis remplace le
binaire installé.

**Velopack (Windows, la seule voie)** : pour les installations faites par
l'installeur Velopack (variable `VELOCABI` posée par le launcher), `rose update`
bascule automatiquement sur Velopack — il lit `releases.win.json`, compare la
version installée (`current` → `app-<version>`), télécharge le paquet complet,
**vérifie son SHA-256** et le dépose dans `packages\` ; le launcher applique la
mise à jour au démarrage suivant avec **rollback automatique** si la nouvelle
version casse. Les arguments internes `--veloapp-updated` / `--veloapp-obsolete`
sont routés en tête de `main()`. La chaîne de build est
`installateur/velopack/build.ps1` (`cargo tauri build` → `vpk pack` →
`vpk local` pour tester, `vpk push` pour publier).
Velopack n'ayant pas d'installeur natif Linux, `rose update` simple reste
sous Linux sur la voie artefacts GitHub (`docs/telechargements/` + SHA-256).

Même logique en console ; sur Linux : `~/.local/bin/rose` (ou `/usr/local`),
PATH dans `~/.bashrc` / `~/.profile`, type MIME `.rose` enregistré.

---

## 📜 Licence et gouvernance

- **Licence** : MIT — voir [LICENSE](LICENSE).
- **Sécurité** : comment signaler une vulnérabilité, ce qui est garanti, la
  chaîne de confiance des releases (SHA-256, Velopack) — voir [SECURITY.md](SECURITY.md).
- **Contribuer** : environnement, règles de parité, style des PR — voir [CONTRIBUTING.md](CONTRIBUTING.md).
- **Documentation longue** : [wiki](https://github.com/Stef302/Rose/wiki) ·
  [site](https://stef302.github.io/Rose/) · [releases](https://github.com/Stef302/Rose/releases).

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
| `rose latex prog.rose --lignes` | Formule LaTeX des seules lignes `%%` | choisir les formules à rendre |
| `rose comp prog.rose [-o sortie]` | Exécutable autonome (moteur + source) | distribution |
| `rose orc prog.rose -o prog.o` | Compilation native LLVM ORC | AOT, vectorisation NEON |
| `rose link prog.o [-o sortie]` | Lier un objet ELF en binaire | avec `rose orc` |
| `rose console [prog.rose]` | Console interactive (REPL) | explorer un programme, taper des expressions |
| `rose-shell [prog.rose]` | Console autonome (même module `repl`) | lancer un vrai shell Rose |
| `rose update` | Installe / met à jour Rose (64 bits) | installation, réparation, désinstallation |
| `rose info` / `rose version` | État du moteur | diagnostic |
| `rose --help` | Aide | — |

Variables d'environnement utiles :
- `ROSE_SANS_JIT=1` : force l'interpréteur pur (sans JIT Cranelift).
- `ROSE_AOT=1` : active la voie AOT LLVM ORC (vectorisation NEON).
- `ROSE_TIMING=1` : affiche les temps de compilation/exécution.
- `ROSE_DUMP_IR=1` : affiche l'IR Cranelift.
- `ROSE_NO_AOT=1` : désactive la voie AOT (force le JIT).
- `ROSE_DEBUG=1` / `ROSE_STEP=1` : débogage pas-à-pas.

> **Portée des opérateurs postfixes.** `!`, `#`, `?` et `$` portent sur des
> entiers **arbitraires** (BigInt). L'interpréteur et le JIT les exécutent ;
> la voie AOT LLVM (`rose orc`, `ROSE_AOT=1`) ne sait pas abaisser un BigInt
> en registre machine et **refuse explicitement** ces quatre opérateurs
> (« primorielle (#) non implémentée en ORC pour l'instant ») plutôt que de
> produire un résultat faux. Utiliser `rose run` (ou le module WASM) pour un
> calcul factoriel. La voie `rose llvm` (transpilation C historique dans
> `archive/`) ne connaît pas non plus ces opérateurs.
