# Numbers

Rose does exact arithmetic by default. Integers are unbounded (native Rust
BigInt, no GMP), rationals stay rational, and π / e / φ are computed to any
number of digits.

## Exact integers

```rose
>> 2 ^ 64
>> fibonacci(200)
>> nb_chiffres(factorielle(1000))
```

## The factorial family

| Notation | Name | Sequence | Example |
|---|---|---|---|
| `n!` | factorial | — | `5!` → `120` |
| `n#` | primorial | [A002110](https://oeis.org/A002110) | `10#` → `210` |
| `n?` | terminal | [A003422](https://oeis.org/A003422) | `5?` → `154` |
| `n$` | superfactorial | [A000178](https://oeis.org/A000178) | `3$` → `12` |

Definitions:

- `n!` = 1·2·…·n
- `n#` = product of all primes ≤ n
- `n?` = 1! + 2! + … + n!
- `n$` = 1! · 2! · … · n!

### How it is computed

`n!` is decomposed into prime powers (Legendre's formula), each power is
computed by binary exponentiation, and the factors are multiplied by a greedy
multi-core product tree. That is why `1000000!` — 5 565 709 digits — takes
about one second.

Library aliases exist as well: `factorielle(n)`, `primorielle(n)`,
`terminal(n)`, `super_factorielle(n)`.

### Scope

These four operators act on arbitrary-size integers. The interpreter, the
Cranelift JIT and the WebAssembly module evaluate them. The LLVM ORC AOT path
cannot lower a BigInt into a machine register and **refuses them explicitly**
(`primorielle (#) non implémentée en ORC pour l'instant`) instead of returning a
wrong value.

## Rationals

```rose
>> 1 / 2 + 1 / 3
>> fraction(1, 3) + fraction(1, 6)
```

## Constants

```rose
>> nombre_pi(20)
>> nombre_e(20)
>> phi(20)
```

Each returns the constant to the requested number of digits, exactly rounded.

## Complex numbers

```rose
>> (2 + 3i) * (1 - i)
```

## Irrationals

`√n` stays symbolic: `racine(2)` is kept as an exact irrational until a decimal
is requested.
