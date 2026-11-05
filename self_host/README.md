# Auto-hébergement Rose (M5)

## État actuel
Le moteur Rose est entièrement en Rust (`rose/`).
Le compilateur Rose est écrit en Rose (`compilateur/`).
L'interpréteur Rose est écrit en Rose (`self_host/moteur/`).

## Chaîne de self-hosting (validée)
```
rose (Rust engine) → compilateur.rose (Rose) → interpreteur.rose (Rose) → binaire natif
```

Le compilateur Rose génère du C++ en sortie (backend `emetteur_cpp.rose`).
Le moteur Rust exécute le compilateur et le programme compilé.

## Pour un self-hosting 100% Rust
Il faudrait créer un backend `emetteur_rust.rose` ou `emetteur_llvm.rose`
qui génère du code Rust ou de l'IR LLVM directement.
C'est une évolution future du compilateur.

## Ce qui est déjà Rust
- Moteur d'exécution (JIT Cranelift + AOT LLVM ORC)
- Arithmétique BigInt (num-bigint natif)
- Compilation AOT (IR LLVM, bitcode, fichier objet)
- CLI et outils
