# Architecture

Rose is a self-hosted language: its reference engine is written in Rust, with
no GMP, no FFI and no external big-number library beyond pure-Rust `num-bigint`.

## Engine layout

| Module | Role |
|---|---|
| `front.rs` | Lexer, parser, AST, type checker |
| `interp.rs` | Exact interpreter, value model, factorial family |
| `i128x.rs` | Exact integers (`Gz`), rationals (`Gq`), product trees |
| `jit.rs` | Cranelift JIT for integer / float / boolean functions |
| `aot_llvm_orc.rs` | LLVM ORC AOT (helper binary) |
| `wasm.rs` | WebAssembly surface (`rose_execute`, `rose_analyse`, …) |
| `latex.rs` | Automatic LaTeX rendering |
| `run.rs` | Standard library and runtime helpers |
| `velopack.rs` | Velopack protocol (manifest, packages, updates) |
| `setup_gui.rs` | Win32 installer window (GDI only) |
| `inflate.rs` | Shared DEFLATE / zlib / PNG decoder |

## Execution paths

1. **Interpreter** — exact semantics, always available, the reference.
2. **Cranelift JIT** — compiles eligible functions at runtime; falls back to the
   interpreter for anything it cannot lower.
3. **LLVM ORC AOT** — native code through a separate `rose-aot` helper.
4. **WebAssembly** — the same interpreter and library, compiled to `wasm32`.

The interpreter, the JIT and WASM agree on every value; the AOT path refuses the
BigInt postfix operators rather than returning a wrong result.

## Exact arithmetic

- Integers: `num_bigint::BigInt`, unbounded, pure Rust.
- Rationals: `(BigInt, BigInt)` pairs, reduced.
- Factorials: prime decomposition (Legendre), binary exponentiation, then a
  greedy multi-core product tree. In WebAssembly the tree is sequential, since
  the target has no threads.

## Windows build

The three Windows executables are produced from Linux with `clang`, `lld-link`,
`llvm-lib` and the SDK/UCRT that `cargo-xwin` caches, targeting
`x86_64-pc-windows-msvc` with `+crt-static`.

## Resources

The installer's resources live in a single `.rsrc` COFF section:

- `RT_RCDATA 1` — the rose PNG;
- `RT_FONT 2` — Kare, converted from WOFF2 to TrueType;
- `RT_ICON 1..6` — six sizes, grouped under one type;
- `RT_GROUP_ICON 1` — the icon group.

`installateur/velopack/verifier-res.py` validates the whole tree without Windows.
