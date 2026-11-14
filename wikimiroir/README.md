# Rose — Wiki

**Rose** is a compiled programming language with French syntax: exact
arithmetic (unbounded integers, rationals, complex numbers, exact π / e / φ),
automatic LaTeX rendering, and a native Rust engine (interpreter, Cranelift JIT,
LLVM ORC AOT, WebAssembly).

> This repository exists because GitHub does not let an API token create the
> `<owner>/<repo>.wiki` repository — only the first wiki page created from the
> web UI does. The wiki content is written in English and is mirrored here so it
> can be versioned and reviewed.

## Pages

| Page | Content |
|---|---|
| [Home](Home) | Overview, install, first program |
| [Syntax](Syntax) | Literals, operators, control flow, functions |
| [Numbers](Numbers) | Exact arithmetic, factorial family (`!`, `#`, `?`, `$`), π / e / φ |
| [CLI](CLI) | `rose run`, `rose latex`, `rose orc`, `rose link`, `rose update` |
| [Install](Install) | Linux, Windows, VS Code extension |
| [Architecture](Architecture) | Engine layout, JIT, AOT, WASM |
| [Contributing](Contributing) | Build, tests, parity harness |

## Factorial family

| Notation | Meaning | Example |
|---|---|---|
| `n!` | exact factorial | `5!` → `120` |
| `n#` | primorial: product of primes ≤ n | `10#` → `210` |
| `n?` | terminal: 1!+2!+…+n! | `5?` → `154` |
| `n$` | superfactorial: 1!·2!·…·n! | `3$` → `12` |

`1000000!` (5 565 709 digits) is computed in about 0.93 s.

## Links

- Engine and releases: <https://github.com/Stef302/Rose>
- Documentation site: <https://stef302.github.io/Rose/>
- License: MIT
