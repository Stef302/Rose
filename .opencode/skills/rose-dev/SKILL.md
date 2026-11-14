---
name: Rose
description: Écrire, corriger et faire tourner des programmes dans le langage Rose (langage compilé en français — entiers exacts illimités, rationnels, complexes, constantes irrationnelles π/e/φ, LaTeX automatique, JIT Cranelift, AOT LLVM ORC, WASM). Utiliser ce skill dès qu'il s'agit de coder en Rose (.rose), d'utiliser la CLI rose (run/latex/comp/orc/link/info), ou de développer dans le dépôt ROSE.
metadata:
  opencode/autoinvoke: true
---

# Développer en Rose

Rose est un langage de programmation **en français**, typé fort dynamiquement,
à arithmétique **exacte** (BigInt natif, zéro GMP), compilé par un moteur Rust
(interprète · JIT Cranelift · AOT LLVM ORC · WASM).

Avant d'écrire du Rose, lis la référence complète :
`references/syntaxe.md` (à la racine du dépôt : section
« 📖 Syntaxe Rose — référence complète » du `README.md`).
L'exemple vivant de toute la syntaxe : `rose/exemples/tour.rose`.

## Règle d'or : vérifier avec le moteur

Le binaire de test est `rose/rose` (et sa copie `rose-20`, un fichier à la racine).
**Toujours tester chaque programme écrit** avant de le conclure :

```bash
cd rose && ./rose programme.rose          # exécute (JIT + formules LaTeX)
./rose latex programme.rose               # rendu LaTeX du programme entier
./rose info                               # état du moteur
```

Après une modification du moteur Rust (`rose/src/`) :

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cd rose && cargo build --release
cp target/release/rose ../rose/rose && cp target/release/rose ../rose-20
cargo test --release                      # tests attendus verts
bash ../bench/suite/run_parite_jit.sh     # parité JIT ↔ interprète
```

Sans ce `cp`, on teste un binaire périmé. Sans le `PATH`, cargo ne trouve
pas rustc.

## Les 10 pièges qui cassent un programme Rose

1. **`i` minuscule = l'imaginaire** (`3 + 4i`, `i * i` → `-1`). Le même
   lexème sert de variable : dans `pour i de …`, `i` devient la VARIABLE
   entière. Hors boucle `pour`, nomme tes compteurs autrement (`$compteur`).
2. **`pour j de A vers B` est EXCLUSIF** : `pour j de 0 vers 3` → 0, 1, 2.
3. **Corps raccourcis `? (cond) [ a ; b ]` et `@ (cond) [ a ; b ]`** :
   instructions séparées par `;`, JAMAIS `,`.
4. **`/` = division exacte** (`7 / 2` → `3.5`) ; division entière = `÷`.
5. **Listes homogènes** : `[1, "a"]` est refusé, un seul type par liste.
6. **Le type se fige à la première affectation** : `$s = "x"` puis
   `$s = 5` = erreur.
7. **`<< x` (saisie)** : variable inconnue → lit un entier ; pour un
   flottant, créer `$x = 0.0` avant.
8. **Types de fonctions FACULTATIFS** : `fn carre(x) { retour x * x }` est
   valable. Les types déclarés (`entier`/`flottant`/`booleen`) activent la
   voie rapide JIT/AOT ; les autres fonctions restent interprétées.
9. **`concatener` prend une LISTE** : `concatener(["ab","cd"])` → "abcd".
   `joindre($l, x)` AJOUTE x en fin de liste (mutateur, ne retourne rien).
10. **Backtick interdit** dans les identifiants ; accents et Unicode permis
    (`⋅ ≤ ≥ ≠ ≈ →` fonctionnent).

## Formules LaTeX (comportement de sortie)

Chaque `>> expr` affiche `formule_latex   résultat` (JIT et interprète, à
l'octet près) :

```
>> 3 + 4i      →  3 + 4\mathrm{i}   5
>> modulus($z) →  \left|z\right|   5
>> pi          →  π = 4\cdot\arctan(1/5) - \arctan(1/239)   π ≈ 3.14159…
```

`rose latex prog.rose` rend le programme entier. Signature typée →
`\operatorname{fib}(n \in \mathbb{Z}) \to \mathbb{Z}` ; signature allégée →
`\operatorname{carre}(x)` (pas de `∈ ?` ni de `→ ?` parasites).

## Modes d'exécution

| Commande | Rôle |
|---|---|
| `rose prog.rose` | interprète/JIT (défaut) |
| `ROSE_AOT=1 rose run prog.rose` | AOT LLVM ORC si helper dispo (parité Rust) |
| `rose latex prog.rose` | LaTeX du programme entier |
| `rose comp prog.rose -o sortie` | exécutable autonome |
| `rose orc prog.rose -o prog.o` puis `gcc -O3 prog.o -o prog -lm` | AOT LLVM |
| `rose link prog.o -o prog` | lier un objet ELF |
| `rose info` / `rose version` | diagnostic |

Env : `ROSE_SANS_JIT=1` (interprète pur), `ROSE_TIMING=1`, `ROSE_DUMP_IR=1`,
`ROSE_ORC_DUMP_IR=1`, `ROSE_AOT_URL`/`ROSE_AOT_SHA256` (miroir helper).

## Build LLVM / WASM (côté développeur seulement — jamais demandé à
l'utilisateur web)

- LLVM (`--features llvm`) : sortir `rose/.cargo/config.toml` du chemin
  (le `crt-static` casse les proc-macros inkwell), puis
  `LLVM_SYS_181_PREFIX=/usr/lib/llvm-18 cargo build --release --features llvm`,
  et RESTAURER le config. Recopier ensuite `target/release/rose-aot` vers
  `~/.cache/rose/rose-aot` ET `~/.cache/rose/rose-aot-v5.8.0-orc-fonctions`.
- WASM : `cargo build --profile wasm-release --target wasm32-unknown-unknown
  --no-default-features` puis `cp target/…/rose.wasm web/` et
  `node web/test-wasm.mjs`. Le `rose.wasm` commité dans `web/` doit rester
  autonome : la page web ne doit JAMAIS exiger cargo ni node.

## Référentiels dans ce dépôt

- `README.md` — référence de syntaxe + bancs + architecture.
- `rose/exemples/tour.rose` — tour complet de la syntaxe (sortie attendue :
  `rose/exemples/tour_sortie.txt`).
- `rose/exemples/exemple_complexe.rose`, `exemple_irrationnels.rose`,
  `exemple_fonctions.rose`, `ffi_libm.rose`, `performance.rose`.
- `examples/` — premiers programmes (arithmétique, fibonacci, flottants).
- `rose/tests/porte_unique.py` — suite de conformité (53 programmes).
- `bench/suite/run_bench_intercale.sh` — banc complexe (Rose JIT / Rose AOT
  / C++ / Rust, 40 tours alternés).
- `docs/WASM.md` — ABI et limitations WASM.

## Installation

- `rose update` : installe / met à jour Rose (64 bits natif). Détecte une
  installation existante, propose réinstaller / réparer / désinstaller.
- **Velopack** (Windows, recommandé) : `rose update --velopack` ou
  `ROSE_VELOPACK=1` — deltas binaires + rollback automatique si la nouvelle
  version casse. La chaîne de build est `installateur/tauri-installer/build.ps1`
  (publish portable → packaging → `velopack pack --self-contained` →
  `velopack local` pour tester). Publier en ligne : `velopack pack` +
  `velopack push <artifact> --repository <url>`.
  Velopack n'ayant pas d'installeur natif Linux, `rose update` simple reste
  sur la voie artefacts (`docs/telechargements/` + SHA-256) sous Linux.
- Windows : `Rose-Setup-<version>.exe` = binaire Rust `rose-setup` avec
  interface Win32 maison (bandeau rose, pétale, boutons GDI) — 64 bits natif,
  aucun droit admin, HKCU seulement.
- Le script de build de l'installeur est `installateur/build-setup-windows.sh`
  (plus de NSIS). Le moteur `rose.exe` est EMBARQUÉ dans l'installeur.

## Notation factorielle (v0.1.x)

| Écriture | Sens | Sortie |
|---|---|---|
| `n!` | factorielle exacte (multi-cœur, facteurs premiers) | entier |
| `n#` | **primorielle** : produit des premiers ≤ n (`10#` → 210) | entier |
| `n?` | **terminal** : somme des factorielles 1!+…+n! (`5?` → 154) | entier |
| `n$` | **super-factorielle** : produit 1!·2!·…·n! (`3$` → 12) | entier |
| `rapide(n)` / `factorielle_rapide(n)` | factorielle, voie rapide — `1000000!` en ~0,9 s | entier |
| `approx(n)` | Stirling complet, série `1/12n − 1/360n³ + 1/1260n⁵` | chaîne `{:.6}e{}` |
| `primorielle(n)` / `terminal(n)` / `super_factorielle(n)` | alias fonctions de `n#` / `n?` / `n$` | entier |
| `view(x)` | visionneuse paginée façon nano (Espace / b / q) | — |
| `type(x)` | nom du type (`entier`, `rationnel`, …) | chaine |

Les opérateurs analogues `!!` / `!?` sont RETIRÉS (reportés) : `!` reste la
seule factorielle postfixe. Lexage des nouveaux postfixes (front.rs) :
- `#` est la primorielle s'il suit immédiatement un caractère non-blanc ;
  en début de ligne ou après un blanc il reste le commentaire ;
- `?` est le terminal SAUF en début d'instruction suivi de `(` à blanc près
  (début de fichier, après `\n`, `;` ou `>`) → c'est le si compact `? (c) [ … ]` ;
- `$` est la super-factorielle s'il n'est PAS suivi d'une lettre ; suivi d'une
  lettre il ouvre une variable (`$x`).

Piège : le `#[derive(Clone)]` de `Gz` (i128x.rs, pointeur nu `*mut Mpz`) ne
clone QUE le pointeur — utiliser `clone_val()` sinon double-free/segfault.

## Style

- Messages et commentaires du moteur en français ; noms de fonctions Rust
  du moteur en français également (`expr_type`, `deplier_i_rec`…).
- Un programme Rose exemple se termine par ses sorties vérifiées : mets à
  jour `tour_sortie.txt` quand `tour.rose` change.
- Commit en français, préfixe conventional (`feat:`, `fix:`, `perf:`,
  `docs:`, `bench:`, `build:`).
