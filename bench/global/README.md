# Banc global de la syntaxe Rose

Mesure un programme qui exerce **toute la syntaxe Rose** dans des boucles chaudes, chronométrées, à comparer avec les équivalents C++/Rust/Python du même dossier.

## Sections
1. Arithmétique entière (précision illimitée) — 10 M itérations
2. Flottants (1 M) + rationnels (1 k)
3. Chaînes Unicode — 1 k concaténations
4. Listes — 10 k push
5. Tables — 5 k insertions + lookup
6. Récursion (fib 28)
7. RNG + tampon — 1 M octets via `remplir_hasard`
8. Fichiers (écriture/lecture)

## Lancement
```
cd bench/global
./run_global.sh
```
Ou individuellement :
```
rose bench_global.rose                         # interprété
rose build bench_global.rose -o b --llvm        # AOT LLVM
g++ -O3 -march=native bench_global.cpp -o bcpp && ./bcpp
rustc -O bench_global.rs -o brs && ./brs
python3 bench_global.py
```

## Résultats actuels
- Rose interprète : ~17.5 s
- Rose AOT (g++, transpilé) : ~0.61 s
- Rose AOT (clang --llvm) : ~0.55 s
- C++ hand-written : (voir bench_global.cpp transpilé) ~0.6 s
- Cible : < 300 ms mono-thread (SSA), puis < 100 ms multi-cœurs.

## Note
Ce banc remplace le banc fermé `mot_de_passe` comme métrique principale. Le mot de passe reste un sous-banc dans `bench/langages/`.