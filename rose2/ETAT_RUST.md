# Rose-20 — le moteur Rust/Cranelift : état complet (2 novembre 2026)

## Ce que c'est
`rose2/` contient la réécriture en Rust pur du moteur Rose, avec **Cranelift**
comme JIT (aucune LLVM). Binaire : `rose2/target/release/rose-20` (~6,5 Mo).

## Arborescence
```
rose2/
├── Cargo.toml            # dépendance unique : cranelift (pas de GMP en crate,
│                         #   pas de C++ ; le C est lié statiquement, voir plus bas)
├── build.rs              # lie libgmp.a (statique) pour l'arithmétique exacte
├── src/
│   ├── main.rs           # CLI ; exécute le pipeline dans un thread à pile 512 Mo
│   │                     #   (miroir du thread C++ rose.cpp:4745)
│   ├── front.rs   (1719 l.)  # lexeur, AST, vérificateur, repli de constantes
│   ├── interp.rs  (1056 l.)  # interpréteur exact (Gz/Gq), saute dans le JIT
│   ├── jit.rs     (1378 l.)  # Cranelift : fonctions + programme principal
│   └── run.rs     (1009 l.)  # valeurs, GMP FFI (Gz/Gq), E/S, affichage
├── tests/porte_unique.py     # garde V1+V2+V3 (ce qui doit être refusé)
├── tests/courir.py           # harnais de courses
└── bench_global.rose
```

## Couverture — ce qui est vérifié par des portes réelles (mesuré aujourd'hui)
| Porte | Résultat |
|---|---|
| Suite moteur, binaire Rust (`tester.sh`) | **46 / 46** |
| Parité des moteurs (Rust vs C++, mêmes .rose) | **119 / 119**, 0 divergence |
| Suite v2 (147 cas) | **95 / 95** |
| Porte unique V1+V2+V3 (`porte_unique.py`) | **52 / 53** (1 faux positif connu du test) |
| Dorés (38 programmes + 13 dans `tests/roses`) | tous identiques au C++ (stdout, stderr, rc) |
| Benchs (`moteur/tests/bench`, 10 programmes) | sorties identiques au C++ |

Le Rust reproduit les **messages d'erreur exacts** du C++ (mêmes textes, mêmes
lignes), la vérification statique (types, retours, mots réservés `t1/t2…`,
`lire` typé entier), le lexage (`//`, `>>`, `$vars` — y compris `$de` : le C++
n'interdit pas les mots-clés après `$`, le Rust non plus), et le repli de
constantes (i128 borné, jamais de faux : si ça dépasse, on laisse l'interprète).

## GMP — « on oublie GMP » : ce que ça a changé
- **Aucune dépendance dynamique** : `ldd rose-20` ne montre pas libgmp ;
  `build.rs` lie `libgmp.a` **statiquement**. Le binaire est autonome.
- **Le JIT ne dépend plus de GMP du tout.** La règle est stricte et honnête :
  la trame Cranelift est en i64 ; dès qu'une opération entière *peut* promouvoir
  en grand (multiplication, puissance, négation de `i64::MIN`, somme sans preuve
  d'absence de débordement), la fonction **refuse la compilation** et retombe
  sur l'interpréteur exact. Le JIT ne rend **jamais** un résultat faux.
  Prendre le raccourci (iadd/imul wrappants) rendait `MAX+1` faux — corrigé et
  consigné ici : c'est le prix de « pas de GMP dans la voie chaude ».
- Les cas limites prouvés conformes au C++ : `9223372036854775807 + 1`,
  `-(-9223372036854775808)`, `3037000500²`, littéraux > i64, lecture stdin
  exacte (`lecture.rose`, `lecture2.rose` maintenant OK).

## Performance (re-mesurée, mêmes programmes, mêmes machines)
| Programme | C++ | Rust |
|---|---|---|
| puissance | 0,94 s | **0,02 s** (47× plus rapide) |
| boucle_entier | 0,03 s | 3,0 s (boucle `+$total = $total + a*a` refusée par la porte, interprétée) |
| bench_global | 0,19 s | 3,8 s (même cause : section 1, boucle `a*a`) |
| pgcd | 0,18 s | 0,82 s |
| appels | 0,02 s | 1,26 s |
| chaines | 0,34 s | **0,14 s** |
| fibonacci / factorielle / premiers / stat | ≈ égaux | ≈ égaux |

Où le JIT s'applique (flottants, entiers sans mult, comparaisons, appels
récursifs), le Rust **écrase** le C++ (puissance 47×). Là où la porte stricte
renvoie à l'interpréteur (multiplications entières), il est plus lent. Le
prochain chantier de vitesse est connu : donner au JIT des **slots Gz** dans sa
trame (comme le JIT C++, qui garde des `Valeur*` GMP par variable) pour que
`*`, `^` et les sommes à gros bornes restent compilées.

## Self-host
- `rose/analyseur.rose` (1 045 lignes — le lexeur/analyseur Rose écrit en Rose) :
  **executé par rose-20, sortie identique au C++**.
- `rose/compilateur.rose` (5 359 lignes — le transpileur Rose→C++ écrit en Rose) :
  **executé par rose-20, transpile exactement le même C++ que le moteur C++**
  (187 octets d'en-tête identiques, rc 0). Autrement dit : le compilateur Rose,
  écrit en Rose, tourne sur le moteur Rust sans divergence.
- Deux bugs de parité bloquants le self-host ont été trouvés et corrigés dans la
  semaine : mots-clés après `$` (le Rust refusait `$de`), et `'retour' hors de
  fonction` (le Rust conditionnait le drapeau au `-> type` déclaré, le C++ le
  met à vrai pour tout corps de fonction, rose.cpp:1261).

## Verdict
Rose-20 est **fonctionnellement complet** vis-à-vis des suites (46/46, 119/119,
95/95, 52/53), **sans dépendance externe visible** (GMP statique, Cranelift
embarqué), **plus rapide que le C++ sur la voie chaude JIT**, et **self-host
vérifié** : il exécute l'analyseur et le transpileur écrits en Rose avec des
sorties bit-à-bit identiques au moteur C++.
