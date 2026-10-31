# Rose v2 — compilateur multi-cibles du langage de mathématiques Rose

> **Le produit principal est le moteur natif** : `../moteur/rose` est un
> binaire autonome (interpréteur + compilateur + GMP embarqué) qui fait
> tourner un `.rose` sans rien installer. La chaîne décrite ci-dessous
> est l'**export polyglotte** : émettre du Rose vers C, Python, Julia,
> Rust ou Zig reste possible pour s'insérer dans un projet existant.

Rose est un langage de calcul **exact** (entiers et rationnels de précision
arbitraire) avec FFI, compilé vers cinq cibles :

| cible    | exactitude            | atout                              |
|----------|-----------------------|------------------------------------|
| **C**    | GMP (mpz/mpq)         | performance native (bignum complet)|
| Python 3 | int / Fraction        | écosystème, scripts                |
| Julia    | BigInt / Rational     | communauté scientifique            |
| Rust     | i64 / f64 (machine)   | intégrité mémoire, vitesse         |
| Zig      | i64 / f64 (machine)   | contrôle bas niveau                |

Les cibles machine **refusent à la compilation** (message nommant la
fonctionnalité) tout programme qui déborderait leur domaine — jamais de
troncature silencieuse.

## Démarrage rapide

```console
$ bin/rose run exemples/signature.rose            # cible C (GMP) par défaut
$ bin/rose run programme.rose --cible python
$ bin/rose build programme.rose -o prog --cible rust
$ bin/rose emettre programme.rose --cible c       # voir le code généré
$ bin/rose info                                   # toolchains détectées
```

Dépendances : Python ≥ 3.10 (hôte du compilateur — **stdlib uniquement**),
puis selon les cibles : gcc ou clang **+ GMP** (C), `python3` (Python),
`julia` (Julia), `rustc` (Rust), `zig` (Zig). Variables d'environnement :
`ROSE_GMP_DIR` (préfixe GMP : `include/` + `lib/`), `ROSE_CC`, `ROSE_PYTHON`…
Le détecteur cherche aussi `/tmp/opencode/toolchain` et `~/.cargo/bin`.

## Le langage en 30 secondes

```rose
>> 1 / 2 + 1 / 3          # 0.8[3] — rationnel EXACT, période détectée
>> 100!                    # factorielle (produit-arbre GMP)
>> 10 ^ -2                 # 0.01 — exposant négatif → rationnel exact
>> 17 div 5                # 3  — division euclidienne (opérateur `div` ou `÷`)
>> 2^3^2                   # 512 — puissance droite-associative

fn collatz(n: entier) -> entier {
    $c = 0
    tant_que ($n != 1) {
        si ($n % 2 == 0) { $n = $n div 2 } sinon { $n = 3 * $n + 1 }
        $c = $c + 1
    }
    retour $c
}
>> collatz(27)             # 111

>> est_premier(2^61 - 1)   # vrai (Miller-Rabin)
>> nombre_pi(50)           # 50 décimales, arrondi correct
>> nb_chiffres(factorielle(100000))   # 456574

@dll("m") @sym("sqrt")     # FFI statique : lié à la compilation (-lm)
fn racine(x: flottant) -> flottant
>> racine(2.0)

h = dynamique::charger("./libmaths_ext.so")   # FFI dynamique (dlopen)
>> dynamique::appeler2(h, "hypotenuse", 3.0, 4.0)
```

Types : `entier`, `rationnel`, `flottant`, `booleen`, `chaine`, `liste<T>`.
Promotion `entier → rationnel → flottant` ; `/` entier/entier donne un
**rationnel exact**. Sucre : `? (cond) […]`, `@ (cond) […]`, `≤ ≥ ≠ × ÷ ≈ ⋅`.
Grammaire EBNF et sémantique complètes : **SPEC.md**.

## Performance (mesurée, Cortex-X925 / aarch64)

| charge de travail                                            | v1 (moteur C#) | v2 cible C |
|--------------------------------------------------------------|---------------:|-----------:|
| `100000!`, `2^100000`, `1000!+1001!`, `(2^64−1)(2^64+1)`     | 7,30 s         | 0,11 s     |
| + compilation gcc (bout en bout)                             |   —            | incluse    |
| résultats                                                    | identiques     | identiques |

`rose run exemples/performance.rose` (100 000! + F(100 000) + π(50) + crible
10⁷ + Miller-Rabin sur M₆₁) : **0,18 s** compilation comprise, toutes valeurs
justes (456 574 chiffres pour 100 000!, 664 579 premiers sous 10⁷…).

## FFI — intégrer des bibliothèques

- **Statique** (5 cibles) : `@dll("m")` / `@sym("pow")` devant une signature
  sans corps ; la bibliothèque est ajoutée au lien (`-lm`, `-l…`, ou chemin).
- **Dynamique** (C, Python, Julia) : `dynamique::charger / appeler0..2 / fermer`
  sur tout `.so` (signatures `double f(double, …)`), comme
  `exemples/ffi_dyn/` le démontre (construction :
  `gcc -shared -fPIC -o libmaths_ext.so maths_ext.c -lm`).

## Structure

```
bin/rose              lanceur CLI
src/rose2/            lexer, parser, vérificateur, 5 backends, conducteur
exemples/             signature.rose, performance.rose, ffi_libm.rose, ffi_dyn/
tests/courir.py       suite de tests multi-cibles (147 cas, golden par cible)
SPEC.md               grammaire EBNF, sémantique, CLI
```

## Tests

```console
$ python3 tests/courir.py            # toutes cibles disponibles
$ python3 tests/courir.py c python   # sous-ensemble rapide
```

## Portabilité

Le compilateur est en Python pur ; les toolchains sont détectées au runtime.
Testé sur Linux **aarch64** (gcc 13, clang 18, GMP 6.3, Julia 1.10.4,
rustc 1.99, Zig 0.14.1) ; aucune raison de ne pas tourner sur x86-64 avec les
mêmes outils.
