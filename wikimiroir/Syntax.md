# Syntax

Rose reads close to French. Statements end at the end of the line; blocks use
`{ }`.

## Values and literals

```rose
>> 42              # integer, unbounded
>> 3.14            # floating point
>> fraction(1, 3)  # exact rational
>> 2 + 3i          # complex
>> "bonjour"       # string
>> vrai
>> faux
```

## Printing

- `afficher(x)` prints a value.
- `>> expr` prints the value of an expression.

```rose
afficher("Bonjour, monde !")
>> 10!
```

## Operators

| Operator | Meaning |
|---|---|
| `+` `-` `*` `/` | arithmetic (`/` is exact when possible) |
| `^` | power (right associative) |
| `div` `%` | integer division, Euclidean modulo |
| `!` `#` `?` `$` | factorial, primorial, terminal, superfactorial |
| `≤` `≥` `≠` `==` `!=` | comparisons (`~=` also means "different") |
| `et` `ou` `non` | logical operators |

## Variables

```rose
$n = 1
tant_que ($n < 100) { $n = $n * 2 }
>> $n
```

`$` starts a variable name. At the end of an expression it is the
superfactorial operator — the lexer decides by context.

## Control flow

```rose
si (2 > 3) { >> 1 } sinon { >> 2 }

? (7 > 5) [>> "grand"]

pour k de 1 vers 11 { >> k }

tant_que ($n < 100) { $n = $n * 2 }
```

`? (...)` followed by `[...]` is the compact `si` form.

## Functions

```rose
fn carre(n: entier) -> entier { retour n * n }

fn fact(n: entier) -> entier {
    si (n <= 1) { retour 1 }
    retour n * fact(n - 1)
}

>> carre(12)
>> fact(10)
```

## Collections

```rose
$l = [1, 2, 3]
joindre($l, 4)
>> $l[2]
>> longueur($l)
```

## Comments

A line starting with `#` (possibly after leading whitespace) is a comment.
Elsewhere, `#` is the primorial operator — the lexer decides by position.

```rose
# ceci est un commentaire
>> 10#    # primorielle : 210
```
