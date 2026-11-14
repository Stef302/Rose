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

afficher(carre(12))
>> 10!
>> 10#
```

`>>` prints the value of an expression; `afficher` prints a string.

## The four postfix operators

| Notation | Meaning | Example |
|---|---|---|
| `n!` | exact factorial | `5!` → `120` |
| `n#` | primorial, product of primes ≤ n (A002110) | `10#` → `210` |
| `n?` | terminal, 1!+2!+…+n! (A003422) | `5?` → `154` |
| `n$` | superfactorial, 1!·2!·…·n! (A000178) | `3$` → `12` |

`1000000!` (5 565 709 digits) is computed in about one second, using prime
factorisation, binary exponentiation and a greedy multi-core product tree — no
GMP.

## Where to go next

| Page | Content |
|---|---|
| [Syntax](Syntax) | Literals, operators, control flow, functions |
| [Numbers](Numbers) | Exact arithmetic, factorial family, π / e / φ |
| [CLI](CLI) | `rose run`, `rose latex`, `rose orc`, `rose update` |
| [Install](Install) | Linux, Windows (Velopack), VS Code extension |
| [Architecture](Architecture) | Engine layout, JIT, AOT, WASM |
| [Contributing](Contributing) | Build, tests, parity harness |

## Links

- Engine and releases: <https://github.com/Stef302/Rose>
- Documentation site: <https://stef302.github.io/Rose/>
- Licence: MIT
