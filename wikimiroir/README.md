# Rose — Wiki

**Rose** is a compiled programming language with French syntax: exact
arithmetic (unbounded integers, rationals, complex numbers, exact π / e / φ),
automatic LaTeX rendering, and a native Rust engine (interpreter, Cranelift JIT,
LLVM ORC AOT, WebAssembly).

> This directory exists because GitHub does not let an API token create the
> `<owner>/<repo>.wiki` repository — only the first wiki page created from the
> web UI does. The wiki content is written in English and mirrored here so it
> can be versioned and reviewed.

## Pages

The pages are ready to paste into the GitHub wiki UI
(<https://github.com/Stef302/Rose/wiki/_new>), one page per file:

| Page | File | Content |
|---|---|---|
| Home | `Home.md` | Overview, first program, factorial family |
| Syntax | `Syntax.md` | Literals, operators, control flow, functions |
| Numbers | `Numbers.md` | Exact arithmetic, factorial family, π / e / φ |
| CLI | `CLI.md` | `rose run`, `rose latex`, `rose orc`, `rose update` |
| Install | `Install.md` | Windows (Velopack), Linux, VS Code, WASM |
| Architecture | `Architecture.md` | Engine layout, JIT, AOT, WASM |
| Contributing | `Contributing.md` | Build, tests, parity harness |

## Factorial family

| Notation | Meaning | Example |
|---|---|---|
| `n!` | exact factorial | `5!` → `120` |
| `n#` | primorial: product of primes ≤ n | `10#` → `210` |
| `n?` | terminal: 1!+2!+…+n! | `5?` → `154` |
| `n$` | superfactorial: 1!·2!·…·n! | `3$` → `12` |

`1000000!` (5 565 709 digits) is computed in about one second. The interpreter,
the JIT and the WebAssembly module evaluate all four operators; the LLVM ORC AOT
path refuses them explicitly rather than returning a wrong value.

## Links

- Engine and releases: <https://github.com/Stef302/Rose>
- Documentation site: <https://stef302.github.io/Rose/>
- Licence: MIT
