# Rose — Wiki

**Rose** is a compiled programming language with French syntax: exact
arithmetic (unbounded integers, rationals, complex numbers, exact π / e / φ),
automatic LaTeX rendering, and a native Rust engine (interpreter, Cranelift JIT,
LLVM ORC AOT, WebAssembly).

> This directory exists because GitHub does not let an API token create the
> `<owner>/<repo>.wiki` repository — only the first wiki page created from the
> web UI does. The wiki content is written in English and is mirrored here so it
> can be versioned and reviewed.

## Pages

| Page | File | Content |
|---|---|---|
| Home | `Home.md` | Overview, first program, what sets Rose apart |
| Syntax | `Syntax.md` | Literals, operators, control flow, functions |
| Numbers | `Numbers.md` | Exact arithmetic, π / e / φ, complex, factorial family |
| CLI | `CLI.md` | `rose run`, `rose latex`, `rose orc`, `rose update` |
| Install | `Install.md` | Windows (Velopack), Linux, VS Code, WASM |
| Architecture | `Architecture.md` | Engine layout, JIT, AOT, WASM |
| Contributing | `Contributing.md` | Build, tests, parity harness |

## Links

- Engine and releases: <https://github.com/Stef302/Rose>
- Discussions: <https://github.com/Stef302/Rose/discussions>
- Documentation site: <https://stef302.github.io/Rose/>
- Licence: MIT
