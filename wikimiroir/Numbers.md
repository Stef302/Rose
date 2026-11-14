# Numbers

Rose does exact arithmetic by default. Integers are unbounded (native Rust
BigInt, no GMP), rationals stay rational, and π / e / φ are computed to any
number of digits.

## Exact integers

```rose
>> 2 ^ 64
>> fibonacci(200)
>> nb_chiffres(2 ^ 1000)
```

Integers never overflow: `2 ^ 1000` is an exact 302-digit number.

## Rationals

```rose
>> 1 / 2 + 1 / 3
>> fraction(1, 3) + fraction(1, 6)
```

`1 / 2 + 1 / 3` stays exact and prints `0.8[3]`; `fraction(1, 3)` builds a
rational explicitly.

## Constants

```rose
>> nombre_pi(20)
>> nombre_e(20)
>> phi(20)
```

Each returns the constant to the requested number of digits, exactly rounded:

```
π ≈ 3.14159265358979323846
e ≈ 2.71828182845904523536
φ ≈ 1.61803398874989484820
```

## Complex numbers

```rose
>> (2 + 3i) * (1 - i)
```

## Irrationals

`√n` stays symbolic: `racine(2)` is kept as an exact irrational (`√2`) until a
decimal is requested.

## The factorial family

Rose extends the usual factorial with three related exact operators:

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

They are computed with prime decomposition (Legendre's formula), binary
exponentiation, and a greedy multi-core product tree, which is why `1000000!`
(5 565 709 digits) takes about one second.

Library aliases also exist: `factorielle(n)`, `primorielle(n)`,
`terminal(n)`, `super_factorielle(n)`.

### Scope

These operators act on arbitrary-size integers. The interpreter, the Cranelift
JIT and the WebAssembly module evaluate them. The LLVM ORC AOT path cannot lower
a BigInt into a machine register and **refuses them explicitly**
(`primorielle (#) non implémentée en ORC pour l'instant`) rather than returning
a wrong value.
