# Rose

**Rose** is a compiled programming language with French syntax. It has exact
arithmetic (unbounded integers, rationals, complex numbers, exact π / e / φ),
automatic LaTeX rendering, and a native Rust engine: interpreter, Cranelift JIT,
LLVM ORC AOT and a WebAssembly module.

Rose is self-hosted and versioned as **0.1.5**.

## First program

```rose
afficher("Bonjour, monde !")

fn carre(n: entier) -> entier { retour n * n }

>> carre(12)
>> 10!
```

`>>` prints the value of an expression; `afficher` prints a string.

## What makes Rose different

- **Exact numbers by default.** Integers are unbounded, rationals stay rational,
  and π / e / φ are computed to any number of digits.
- **LaTeX built in.** `rose latex prog.rose` turns a whole program into a formula.
- **French syntax, no ceremony.** `si`, `sinon`, `pour`, `tant_que`, `retour`.
- **Four execution paths.** Interpreter, Cranelift JIT, LLVM ORC AOT and WASM —
  the interpreter, JIT and WASM agree on every value.
- **A factorial family.** Beyond `n!`, Rose has `n#` (primorial), `n?` (terminal)
  and `n$` (superfactorial), all exact.

## Where to go next

| Page | Content |
|---|---|
| [Syntax](Syntax) | Literals, operators, control flow, functions |
| [Numbers](Numbers) | Exact arithmetic, π / e / φ, complex, factorial family |
| [CLI](CLI) | `rose run`, `rose latex`, `rose orc`, `rose update` |
| [Install](Install) | Linux, Windows (Velopack), VS Code extension |
| [Architecture](Architecture) | Engine layout, JIT, AOT, WASM |
| [Contributing](Contributing) | Build, tests, parity harness |

## Links

- Engine and releases: <https://github.com/Stef302/Rose>
- Discussions: <https://github.com/Stef302/Rose/discussions>
- Documentation site: <https://stef302.github.io/Rose/>
- Licence: MIT
