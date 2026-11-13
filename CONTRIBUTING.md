# Contribuer à Rose

Merci ! Rose est un petit moteur Rust sans dépendance runtime ; les contributions
les plus utiles sont les bogues reproductibles, les tests de parité en plus, et
les correctifs ciblés.

## Avant de commencer

- Lis le [README](README.md) (référence de syntaxe + architecture) et le
  [wiki](https://github.com/Stef302/Rose/wiki).
- Pour une idée de fonctionnalité, ouvre d'abord une **issue** : le langage est
  délibérément petit, toute extension du vocabulaire se discute.

## Environnement

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cd rose && cargo build --release
cp target/release/rose ../rose/rose && cp target/release/rose ../rose-20
cargo test --release
```

- Le binaire de test est `rose/rose` (et `rose-20`). Sans le `cp`, on teste un
  binaire périmé.
- Suite de conformité : `python3 rose/tests/porte_unique.py` (attendu : 48/53,
  les 5 écarts connus sont documentés dans le fichier).
- Parité JIT ↔ interprète : `bash bench/suite/run_parite_jit.sh`.

## Règles du projet

1. **La parité est sacrée** : interprète, JIT et AOT doivent rendre la même
   sortie, caractère par caractère. Toute PR qui casse un test de parité est
   refusée.
2. **Zéro dépendance runtime** : pas de GMP, pas de FFI externe dans le cœur.
   L'arithmétique reste en Rust natif (`rose/src/i128x.rs`).
3. **Messages d'erreur en français**, noms de fonctions du moteur en français
   (`expr_type`, `deplier_i_rec`…).
4. **Le moteur ne se tait jamais** : une erreur nomme la variable et l'endroit.
5. Commits en français, préfixe conventional (`feat:`, `fix:`, `perf:`,
   `docs:`, `bench:`, `build:`).
6. Un exemple modifié ⇒ mets à jour la sortie attendue
   (`rose/exemples/tour_sortie.txt`).

## Style des contributions

- Une PR = un sujet. Sépare le refactor du comportement.
- Ajoute le test qui reproduit le bogue avant le correctif.
- Les gains de performance se prouvent : join un avant/après mesuré (ex. la
  factorielle multi-cœur : `1000000!` de 2,2 s → 0,93 s).
- Ne casse jamais la compilation — `cargo build --release` et `cargo test
  --release` verts avant de pousser.

## Où toucher quoi

| Fichier | Rôle |
|---|---|
| `rose/src/front.rs` | lexer + analyseur + table STDLIB |
| `rose/src/interp.rs` | interprète + fonctions stdlib |
| `rose/src/i128x.rs` | BigInt maison (factorielle multi-cœur, arbre glouton) |
| `rose/src/jit.rs` | JIT Cranelift |
| `rose/src/aot_llvm_orc.rs` | AOT LLVM ORC |
| `rose/src/run.rs` | `view_grand_nombre` (visionneuse nano) |
| `vscode-rose/` | extension VS Code (tmLanguage, snippets, STDLIB) |
| `docs/` | le site (GitHub Pages) + artefacts |

Les PRs venant d'un compte sans 2FA activée peuvent être retenues par les
protections de dépôt : c'est voulu.
