# rose-cpp — Moteur C++ OBSOLÈTE

Ce répertoire contient l'**ancien moteur Rose écrit en C++**. Il est **OBSOLÈTE** et
**non maintenu**.

## Pourquoi

Le moteur Rose est désormais écrit en **Rust** (`rose/`), avec :
- un lexeur/parseur/vérificateur/interpréteur natif ;
- un JIT Cranelift pour le code chaud ;
- un RIR (Représentation Intermédiaire de Rose) pour le corps principal ;
- zéro dépendance GMP (arithmétique exacte BigInt maison).

Le moteur C++ n'est plus la référence et ne doit pas être utilisé comme tel.

## Statut

- **Non maintenu** : aucun commit n'y est effectué depuis le passage au moteur Rust.
- **Non publié** : n'est pas inclus dans les releases ni les installeurs.
- **Conservé à des fins historiques uniquement**.

## Les dorés (tests de conformité)

Les tests de conformité (dorés) ont été déplacés vers `bench/dores/` pour être
utilisables avec le moteur Rust actuel.

## Ne PAS utiliser

Ne référencez pas ce répertoire depuis la documentation utilisateur ni depuis
les scripts de build. Le moteur Rust (`rose/`) est le seul moteur supporté.
