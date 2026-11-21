# JIT réel (vague 83) — constat daté, 21/11/2026

## Préambule : corriger la prémisse

La note `VERITE_METHODES.md` (vague 81) affirmait :

> « Les superinstructions NEON/fib sont des closures Rust, pas du Cranelift »

**Ce n'est plus vrai sur l'arbre courant** (constat re-mesuré le 21/11 avec
`target/release/rose`, HEAD `e73e1344`, avant et après les modifications de ce
chantier). `superinstructions_autorisees()` vaut

```rust
std::env::var("ROSE_SUPERINST").map(|v| v == "1").unwrap_or(false)
```

Donc **par défaut, aucune superinstruction n'est installée** (`natifs=false`
dans toutes les lignes `TIMING-JIT` ci-dessous). Même avec `ROSE_SUPERINST=1`,
les motifs détectés ne couvrent pas les programmes d'audit : `fib.rose`
(`832040`) et `carres.rose` (`328350`) restent dans `eligibles` avec
`emit=` / `define=` non nuls. Les closures `Compile::Natif` des
superinstructions existent toujours dans `installer_superinstructions`, mais
elles ne servent plus ces programmes.

## Ce que `eligibles` mesure (et ce qu'il ne mesure pas)

`TIMING-JIT eligibles=[...]` est imprimé par `compiler_tout` : il ne recense
que les **fonctions nommées** confiées à Cranelift. Le **programme principal**
(`$principal`) est compilé séparément par `compiler_principal`, avec son propre
`declare_function` / `define_function` / `finalize_definitions`. Un programme
sans fonction (ex. `>> 7*6`) a donc `eligibles=[]` **même quand son principal
est réellement compilé par Cranelift**.

Pour rendre ce fait vérifiable, `compiler_principal` publie désormais sous
`ROSE_TIMING=1` :

```
TIMING-JIT principal=cranelift voie=rir|ast define_function+finalize_definitions OK
TIMING-JIT principal=repli-interpreteur (squelette non prouve compilable)
```

Aucune superinstruction native n'a été touchée.

## Mesures sur les 5 programmes demandés

Binaire : `rose/target/release/rose`. Les deux définitions de fonctions
écrites sans type de paramètre (`fn carre(n)->entier`, `fn fib(n)->entier`)
sont **refusées par le vérificateur** (« La fonction retourne ?, attendu
entier ») : la forme typée correcte est `fn carre(n: entier)->entier`. Les
mesures ci-dessous utilisent la forme typée.

| # | Programme | `eligibles` | Principal Cranelift ? | Résultat |
|---|---|---|---|---|
| 1 | `>> 7*6` | `[]` (aucune fonction) | **oui**, `voie=rir` | `42` |
| 2 | `fn carre(n: entier)->entier{retour n*n} >> carre(7)` | `["carre"]`, `rir=true`, `emit=33.6µs`, `define=322.2µs` | **oui**, `voie=ast` | `49` |
| 3 | `>> 565#` | `[]` | **non** — `PROFIL-RIR principal=true`, l'opérateur `Primorielle` reste une expression AST à 1 op, non émissible | valeur exacte (300 chiffres) |
| 4 | `fn fib(n: entier)->entier{…} >> fib(20)` | `["fib"]`, `rir=false` (voie AST), `emit=27.2µs`, `define=168.9µs` | **oui**, `voie=ast` | `6765` |
| 5 | `pour i de 0 vers 10000 {}` | `[]` (corps vide : aucune fonction synthétique `__rose_loop_*`) | **oui**, `voie=rir` | — |

Lignes brutes (extrait) :

```
# 1
TIMING-JIT eligibles=[] rir={} natifs=false
TIMING-JIT principal=cranelift voie=rir define_function+finalize_definitions OK
42
# 2
TIMING-JIT eligibles=["carre"] rir={"carre": true} natifs=false
TIMING-JIT emit=33.6µs
TIMING-JIT define=322.224µs (emit=33.6µs fin=2.896µs)
TIMING-JIT principal=cranelift voie=ast define_function+finalize_definitions OK
49
# 4
TIMING-JIT eligibles=["fib"] rir={"fib": false} natifs=false
TIMING-JIT emit=27.168µs
TIMING-JIT define=168.896µs (emit=27.168µs fin=1.856µs)
TIMING-JIT principal=cranelift voie=ast define_function+finalize_definitions OK
6765
# 5
TIMING-JIT eligibles=[] rir={} natifs=false
TIMING-JIT principal=cranelift voie=rir define_function+finalize_definitions OK
```

**Verdict : 4 des 5 programmes sont réellement compilés par Cranelift**
(`define_function` + `finalize_definitions` prouvés). Le seul qui reste
interprété est `565#` (primorielle : résultat BigInt de 300 chiffres, hors trame
i64/f64 du JIT). `eligibles` n'est non vide que pour 2 d'entre eux, parce que
les autres n'ont pas de fonction nommée — pas parce qu'ils ne sont pas compilés.

## Élargissement du compilable (ce chantier)

Trois trous réels ont été comblés dans `rose/src/jit.rs` :

1. **`AppelRir` à arguments littenaux** (`eabstrait_rir_compilable_ctx` +
   `expr_eabstrait_rir`). L'émetteur ignorait silencieusement tout argument
   qui n'était pas `Expr::Var`, produisant un appel d'arité réduite ; la porte
   refusait donc le corps. Entiers, booléens et flottants sont désormais émis
   en constantes, avec conversion `fcvt_from_sint` vers un paramètre flottant.
   Littéraux rationnels / complexes / chaîne restent refusés.
2. **Puissance entière dans le chemin AST** (`exposant_ast_const`,
   `bloc_compilable`, `i64_pow`). L'ancien `i64_pow` était un stub qui rendait
   `0` : le compilable refusait donc `^` entier. Il déroule maintenant
   `rose_mul_ent` exactement comme la voie RIR (promotion BigInt au
   dépassement), et la porte accepte un exposant littéral borné 0..=64.
3. **`Div` / `Mod` / `Flottant`** : déjà couverts par la voie RIR avant ce
   chantier (`eabstrait_rir_compilable_ctx`, `Oper::Div|Mod|DivEntier`,
   `Eabstrait::Flottant`). Vérifiés avec `ops.rose` : les 5 fonctions
   `fdiv`, `fmod`, `fpow`, `ffloat`, `fcall` sont toutes `rir=true` et rendent
   `3.5 / 1 / 125 / 3.75 / 1`. Aucune modification n'était nécessaire.

## Vérifications

- `cargo build --release --bin rose` : OK.
- `cargo test --release` : **308 + 321 + 6 + 9 + 8 + 1 = 653 passés, 0 échec**
  (3 + 3 ignorés), plus les cibles 0-test.
- Point fixe `b3ed9262dd54776a6dbd07bc12368d0e`, 263 520 o :
  `rose rir compilateur/emetteur.rose` **×2** et
  `rose compilateur/emetteur.rose compilateur/emetteur.rose` **×2** →
  md5 identique, stderr 0 octet.
- Banc « 9 domaines × 9 tirages = 81/81 » : **non re-mesuré**. Le script
  `/tmp/opencode/benchmulti/bench_now.py` cité dans `RC5.md` n'existe plus sur
  cette machine. Aucun résultat 81/81 n'est revendiqué ici.

## Limites

- `565#` reste interprété : `Expr::Primorielle` n'a pas de forme `Eabstrait`
  et son résultat (BigInt) ne tient pas dans un registre i64. Le JIT n'a pas
  été étendu à ce cas.
- `eligibles` continue de ne pas recenser `$principal` : c'est une décision de
  diagnostic, pas de compilation. La ligne `TIMING-JIT principal=…` est la
  preuve à utiliser pour les programmes sans fonction.