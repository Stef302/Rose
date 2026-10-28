# Modèle de sécurité « à la Rose » — design (v3.16+)

## Objectif
Offrir la sûreté mémoire de Rust sans le coût de complexité du borrow checker,
et sans GC lourd. Rose doit surpasser Rust/C++ sur l'ergonomie ET la vitesse.

## Trois piliers

### 1. Arènes / régions (pas de free individuel)
- `arene()` crée une région d'allocation.
- `arene_allouer(a, taille)` alloue dans la région (O(1), bump pointer).
- `arene_reset(a)` libère toute la région d'un coup.
- Pas de comptage de références dans les boucles chaudes → pas d'atomic, pas de GC.
- Cas d'usage : tampons temporaires, listes intermédiaires, structures de parseur.

### 2. Ownership linéaire optionnel (mode affine)
- Par défaut : partage par référence comptée (comme aujourd'hui, coût atomic).
- Mode `--affine` : types affines pour les chemins critiques. Le compilateur
  vérifie l'usage unique à la compilation (comme Rust, mais opt-in et localisé).
- 95 % du code reste en mode partagé ; les 5 % critiques (boucles, FFI) passent
  en affine pour zéro coût.

### 3. Mémoire plate pour les tableaux numériques
- `tampon` (octets) et futurs `vecteur_entier`/`vecteur_flottant` : mémoire
  contiguë, pas de boxing, pas de GC.
- SoA automatique pour les tableaux de structures (cache-friendly).
- Alignment automatique (assume_aligned / restrict injectés par le compilateur).

## Comparaison honnête

| Dimension | Rust | C++ | Rose (proposé) |
|---|---|---|---|
| Sûreté mémoire | borrow checker (compile) | manuel (UB possible) | arènes + affine opt-in |
| Coût runtime | zéro | refcount (shared_ptr) | zéro sur chemins affines |
| Coût compilation | élevé (borrow checker) | faible | moyen (analyse légère) |
| Ergonomie | faible (fights) | moyenne | élevée (langage haut niveau) |

## Statut
Design uniquement. Implémentation à engager après validation des gains de
performance (Voie 1.4, Voie 2) pour ne pas ralentir le moteur.
