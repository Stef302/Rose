# Banc multi-langages — « générateur de mot de passe »

Même programme, six façons. N caractères (défaut : 1 milliard) pris au hasard
dans un alphabet de 64, écrits dans un buffer initialisé à zéro — l'allocation
et son memset sont donc payés par tout le monde. Chacun se chronomètre
lui-même (horloge monotone), trois passes, minimum retenu.

| Programme | RNG | Algorithme de remplissage |
|---|---|---|
| `mdp_splitmix.cpp` | splitmix64 | aligné octet, 4 tirages par lot — **le jumeau exact du builtin Rose** |
| `mdp_mt19937.cpp` | mt19937_64 | ce qu'on écrit spontanément en C++ (`rng() % 64`) |
| `mdp_rand.c` | `rand()` glibc | la写法 historique `rand() & 63` |
| `mdp.rs` | splitmix64 | jumeau Rust |
| `mdp.zig` | splitmix64 | jumeau Zig |
| `mdp_numpy.py` | `default_rng().choice` | la traduction directe du script Python d'origine |
| `mot_de_passe.rose` | splitmix64 (`graine`) | le banc Rose (`moteur/tests/bench`) |

## Lancer

```console
$ ./bench.sh              # N = 1 000 000 000
$ ./bench.sh 100000000    # plus rapide pour un contrôle rapide
```

Outils détectés s'ils existent : `g++`, `rustc` (ou `~/.cargo/bin/rustc`),
`zig` (`ZIG=` ou `/tmp/opencode/zig-aarch64-*/zig`), `python3` + numpy, et le
moteur Rose (`ROSE=` pour le changer, défaut `../../moteur/cpp/rose`).

## Résultats — AArch64, 1 Md de caractères, session chaude (oct. 2026)

| | temps |
|---|---:|
| Rust (splitmix64) | **375 ms** |
| C++ (splitmix64) | **385 ms** |
| Zig (splitmix64) | **453 ms** |
| **Rose** (`mot_de_passe.rose`) | **517 ms** |
| C++ (mt19937 naïf) | 1 337 ms |
| C (`rand()` glibc) | 12 008 ms |
| numpy (`choice`, mesuré à 1/10ᵉ puis ×10) | 66 690 ms |

À lire avec deux précautions, jamais comme des chiffres absolus :

1. **l'état thermique de la machine change tout** — la même table froide donait
   C++ 681 ms / Rose 895 ms la même journée, et numpy 17 s lors d'une session
   antérieure (17 s → 67 s du simple au quadruple selon l'état) ;
2. l'écart Rose ↔ C++ (≈ 130 ms ici) est le **coût fixe de l'interpréteur**
   (démarrage, vérification, allocation du tampon) : à 100 M de caractères,
   le même écart absorbe davantage le temps de boucle, à 10 Md il devient
   négligeable.

Ce que la table montre vraiment : **la brique de Rose est du niveau du natif
écrit à la main avec le même algorithme** (mêmes constantes, même structure
de boucle dans les trois langages), et que les formulations « spontanées »
(mt19937 `%`, `rand()`, `np.random.choice`) coûtent de 2,6× à 130× plus cher
que la formulation vectorisée par lot — en Rose comme ailleurs.
