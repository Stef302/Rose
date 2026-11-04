# ⚡ Expérience 2 — AOT LLVM ORC -O2 + PGO

## Objectif
Remplacer ou compléter l'AOT actuel (transpilation C + gcc) par **LLVM ORC**
compilé in-process depuis Rust, avec **PGO** (`-fprofile-use`) pour le meilleur
ratio performance/compilation.

## Pourquoi LLVM ORC ?
- Pas de subprocess C/gcc : tout se fait dans le même processus Rust
- PGO possible : instrumentation → relevé → `-fprofile-use`
- Gain attendu : pic le plus haut, seulement +8-19 % de temps de compilation
  par rapport à un AOT simple

## Niveaux à tester
| Niveau | Drapeau | Usage |
|--------|---------|-------|
| dev itératif | `-O1` | compilation ultra-rapide |
| release | `-O2` | défaut actuel |
| noyau numérique | `-O3` | SIMD, non limité mémoire |
| PGO | `-O2 -fprofile-use` | meilleur ratio |

## État : À implémenter

### Étapes prévues
1. Ajouter `llvm-sys` ou `inkwell` comme dépendance dans `rose/Cargo.toml`
2. Créer `rose/src/aot_llvm_orc.rs` : module ORC JIT/AOT
3. Ajouter la commande `rose-20 orc prog.rose -o sortie`
4. Générer un profil avec `-fprofile-generate`, exécuter le bench, puis
   recompiler avec `-fprofile-use`
5. Mesurer et comparer avec le JIT Cranelift (exp 1)

### Squelette de code (à remplir dans rose_labs/exp/aot_llvm_orc/)

```rust
// rose_labs/exp/aot_llvm_orc/orc_compiler.rs — squelette
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::builder::Builder;
use inkwell::PassManagerOptions;

pub fn compile_orc(source: &str, opt_level: OptLevel) -> Result<Vec<u8>, String> {
    let context = Context::create();
    let module = context.create_module("rose");
    let builder = context.create_builder();
    
    // TODO : transpiler l'AST Rose vers LLVM IR
    // TODO : appliquer les passes d'optimisation selon opt_level
    // TODO : utiliser ORC JIT pour compilation in-process
    
    unimplemented!("LLVM ORC à implémenter")
}

pub enum OptLevel { O1, O2, O3, PGO }
```

## Comment lancer l'expérience
```bash
./rose_labs/tools/lancer_exp.sh aot_llvm_orc O2
```
*(Actuellement : compilation standard avec le niveau choisi, pas encore LLVM ORC natif)*
