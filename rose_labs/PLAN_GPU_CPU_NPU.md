# Plan CPU/GPU/NPU/Mémoire — Rose v3.16+

## Décision stratégique (v3.16+)

Le banc fermé `mot_de_passe` est abandonné comme métrique unique. On mesure désormais un **banc global** qui exerce toute la syntaxe Rose (arithmétique illimitée, flottants, rationnels, chaînes Unicode, listes, tables, récursion, RNG+tampon, fichiers) — voir `bench/global/` (Rose + C++ + Rust + Python). Le mot de passe reste un sous-banc (`bench/langages/`).

Pour battre Rust/C++/Zig durablement, le goulot n'est pas le calcul mais la mémoire et la gestion du temps de vie. Pistes retenues : arènes/régions, ownership linéaire optionnel, GC générationnel optionnel, mémoire plate pour tableaux numériques, SoA, work-stealing scheduler, fusion de boucles via MLIR, GPU offload, NPU via dialecte tensor.

## Banc global (`bench/global/`)

| Composant | Fichier |
|---|---|
| Rose | `bench_global.rose` |
| C++ | `bench_global.cpp` |
| Rust | `bench_global.rs` |
| Python | `bench_global.py` |
| Lanceur | `run_global.sh` |

Résultats actuels : Rose ~16.5 s (interpréteur), C++ ~5.4 ms (AOT). Cible : descendre sous Rust/C++ via AOT `--llvm` + SSA (Voie 1).

---


# Plan GPU / CPU / NPU — Rose

Objectif : dépasser Rust/Zig/C++ sur le banc `mot_de_passe` et sur les opérations
de tableaux, en exploitant la puissance disponible (CPU multi-coeurs, GPU, NPU).

## Etat actuel

- CPU mono-thread : Rose ~400-550 ms sur 1 Md (splitmix64), dans la classe de
  Rust (~350-600 ms) et C++ (~350-580 ms). Goulot = bande passante memoire
  (~68 GB/s) + chaine de dependance de splitmix64.
- MLIR : SSA de la boucle de tirage -> entrelacement automatique des 4 chaines
  splitmix64 -> ~4 ms sur 100 M (gain ~10x vs Rose AOT ~41 ms). Levier = forme SSA.
- GPU : exploration uniquement (`gpu_probe.mlir`), pas de CUDA/ROCm dans cet
  environnement. Squelette de kernel valide par `mlir-opt-18 --verify-each`.
- NPU : non explore.

## Principe fondamental

Le goulot n'est pas le calcul (splitmix64 est trivial) mais la bande passante
memoire et la dependance de donnees. Tout plan doit donc :
1. Reduire les ecritures memoire (stores paquets, non-temporels) - effet marginal
   mesure (~+-10 %, instable, thermique).
2. Casser la dependance de donnees via SSA / entrelacement - gain ~10x demontre
   en MLIR.
3. Passer a un RNG parallele par construction (Philox/Threefry) pour scaler
   sur plusieurs coeurs / GPU / NPU - seule voie pour battre les mono-thread.

## Voie 1 - CPU (court terme, demontre)

| Etape | Action | Statut |
|---|---|---|
| 1.1 | SSA-friendly `rose_remplir_hasard` (etat RNG local) | fait (v3.15) |
| 1.2 | Build moteur avec `-O3 -march=native` | fait |
| 1.3 | Integrer la forme SSA dans l'emission LLVM de `rose build --llvm` | en cours — AOT fonctionnel sur le banc global (0.55 s clang, 0.61 s g++), transpileur corrige (index de table), GMP multi-arch, suite AOT verte (56/56) |
| 1.4 | Emettre l'IR LLVM dans la forme entrelacee (4 chaines) | a faire |
| 1.5 | Mesurer : viser < 300 ms sur 1 Md (vs Rust ~350) | cible |

Risque : le gain SSA est demontre en MLIR (LLVM 18) mais pas encore dans le
moteur. L'emetteur C++ actuel emet des appels a `rose_remplir_hasard` (builtin),
donc le gain depend du compilateur hote. L'emetteur LLVM doit emettre la boucle
SSA directement.
## Voie 2 - CPU multi-coeurs (moyen terme)

Le remplissage est massivement parallele si on utilise un RNG a saut O(1)
(Philox). splitmix64 a un etat lineaire (`st += C`) donc le saut est O(1) aussi :
`st_k = st_0 + k*C`. On peut donc decouper le buffer en plages et donner a
chaque thread son etat de depart.

| Etape | Action | Statut |
|---|---|---|
| 2.1 | Implementer `remplir_hasard` parallele (std::thread) sur splitmix | tente, plus lent (bande passante memoire saturee, contention) |
| 2.2 | Passer a Philox 4x32 (saut O(1), pas de dependance) | a faire |
| 2.3 | Tuilage NUMA-aware (1 thread par coeur, plages alignees sur les pages) | a faire |
| 2.4 | Mesurer le scaling : viser lineaire jusqu'au nombre de coeurs | cible |

Decision : garder le mono-thread pour le banc actuel (meilleur rapport
simplicite/perf) et reserver le multi-coeurs pour les gros buffers (> 100 M) ou
la bande passante memoire n'est pas le goulot.

## Voie 3 - GPU (moyen/long terme)

Le GPU gagne grace a la bande passante HBM (~1-2 TB/s) et aux milliers de
threads. Mais il faut un RNG parallele (Philox) - splitmix64 est une chaine.

| Etape | Action | Statut |
|---|---|---|
| 3.1 | Implementer Philox 4x32 en MLIR (`gpu` dialect) | a faire |
| 3.2 | Kernel `remplir_hasard` : 1 thread = 1 bloc de 32 octets, etat = Philox(counter, key) | a faire |
| 3.3 | Pipeline `gpu -> nvvm` (CUDA) / `gpu -> rocdl` (ROCm) / `gpu -> spirv` (Vulkan) | a faire |
| 3.4 | Host code : allocation device, launch, copie resultat | a faire |
| 3.5 | `rose build --gpu` : emet kernel + host | a faire |
| 3.6 | Bench : viser > 10x le CPU sur 1 Md | cible |

Contrainte environnement : pas de CUDA/ROCm ici. Validation par
`mlir-opt-18 --verify-each` uniquement. Tests reels necessitent une machine
avec GPU.

Fichiers : `rose_labs/gpu/gpu_probe.mlir` (squelette), `rose_labs/gpu/README.md`.

## Voie 4 - NPU (investigation)

Les NPU sont optimises pour l'inference de reseaux de neurones (matmul, conv),
pas pour le remplissage aleatoire. Leur utilite pour Rose est limitee mais a
verifier.

| Question | Reponse attendue |
|---|---|
| Le NPU peut-il executer du code arbitraire (RNG) ? | Non en general : API fermees (CoreML, NNAPI, DirectML), operations figees. |
| Peut-on exprimer `remplir_hasard` comme un matmul/conv ? | Non : sortie = fonction RNG du compteur, pas une multiplication de matrices. |
| Le NPU a-t-il une bande passante memoire superieure ? | Variable : souvent partagee avec le CPU, latence elevee pour petits tenseurs. |
| Cas d'usage Rose plausibles | Inference de modeles (si Rose integre un runtime NN), pre-traitement de tenseurs, pas le RNG. |

Conclusion preliminaire : le NPU n'est pas la voie pour battre les langages sur
le banc `mot_de_passe`. Il devient pertinent si Rose integre des primitives de
calcul tensoriel (map/reduce sur grands tableaux -> matmul), auquel cas le NPU
peut accelerer ces operations. A reevaluer quand un dialecte `rose.tensor`
existera.

| Etape | Action | Statut |
|---|---|---|
| 4.1 | Inventorier les NPU disponibles (Apple ANE, Qualcomm Hexagon, Intel NPU, AMD XDNA) | a faire |
| 4.2 | Tester l'execution d'un kernel MLIR `gpu`/`linalg` sur un NPU via IREE ou ONNX Runtime | a faire |
| 4.3 | Decider : integrer ou ignorer selon les gains mesures | a faire |

## Ordre de priorite recommande

1. Court terme : SSA dans l'emetteur LLVM (Voie 1.3-1.4) - gain ~10x deja
   demontre, le plus sur.
2. Moyen terme : Philox + CPU multi-coeurs (Voie 2.2-2.4) - scaling reel.
3. Moyen/long terme : GPU via MLIR (Voie 3) - plus gros gain potentiel,
   necessite machine GPU.
4. Investigation : NPU (Voie 4) - faible priorite, depend d'un dialecte tensor.

## Mesure de succes (criterions operationnels)

Un banc est « englobant » s'il execute une charge complete : arithmetique
entiere, flottants, chaines, listes, tables, recursions, RNG, entrees-sorties.
Un banc de point (meme RNG repete a l'infini, type `bench/suite/mot_de_passe`)
ne prouve RIEN : la bande passante memoire sature avant, ce qui masque les
fuites de conception.

Un banc englobant est considere **vert** quand, par kernel :
1. chaque moteur compare est lance au moins **5 fois** avec le **minimum** retenu
   (les versions actuelles ne font qu'une seule passe ; a changer) ;
2. par kernel, au moins 4 des 7 kernels au sens large (reduction, factorielle,
   crible, transposition, FFT, recherche, Monte-Carlo) sont presentes ;
3. la comparaison croisee est faite : moteur stable vs `rose_labs` vs
   `bench/suite` (AOT natif et `--llvm`) ;
4. le seuil de promotion est atteint : < 300 ms monofilament sur 1 Md pour
   `bench/suite` ; < 150 ms pour le banc englobant ; parite de sortie verifiee.

- Tests de parite : 46/46 verts (aucune regression de sortie).

### Etat mesure le 28/10/2026 (AArch64 Cortex-A520, NEON 128 bits, 20 coeurs)

`bench/englobant/` est le banc englobant VERT : `./run_englobant.sh` sort en
**exit 0**, zéro BUILD FAIL, zéro FAIL, zéro DIVERGENT sur les quatre valeurs
(`d45 + d55`, `nb5`, `interieur6`, `tot7`) des SIX moteurs, à deux échelles.

| critère du plan | mesure | verdict |
|---|---|---|
| 1. 5 passes, min retenu | `RUNS=5`, min, et la résolution de `time -p` est SONDEE puis dépassée par l'échauffement | VERT (avant la correction, le mot-clé `time` avalait la sortie et tous les moteurs sains passaient à `FAIL rc=2`) |
| 2. ≥ 4 des 7 kernels larges | réduction (S4), crible (S5), Monte-Carlo (S6), recherche/glanée mémoire à stride (S7) = 4 | VERT |
| 3. comparaison croisée | interpréteur, AOT natif, AOT `--llvm`, C++, Rust, Python | VERT |
| 4. seuils + parité | parité des six moteurs = OUI ; vitesses = NON, tableau ci-dessous | **PARTIEL** |

Totaux par moteur (5 passes, min retenu, `ROSE_CXX=clang++`, même session que
la parité ; ratios sur la MÊME session, variance thermique ≈ 25 %) :

| moteur | N = 4096 | ms | × C++ | N = 262 144 | ms | × C++ |
|---|---:|---:|---:|---:|---:|---:|
| rose interpréteur | 5,170000 s | 5170,0 | 769 × | 11,450000 s | 11450,0 | 814 × |
| rose AOT natif | 0,030469 s | 30,5 | **4,53 ×** | 0,086563 s | 86,6 | **6,16 ×** |
| rose AOT `--llvm` | 0,030312 s | 30,3 | **4,51 ×** | 0,087656 s | 87,7 | **6,23 ×** |
| C++ `g++ -O3 -march=native` | 0,006719 s | 6,7 | 1,00 × | 0,014063 s | 14,1 | 1,00 × |
| Rust `rustc -O3 -C target-cpu=native` | 0,002500 s | 2,5 | 0,37 × | 0,012656 s | 12,7 | 0,90 × |
| python3 (référence) | 1,250000 s | 1250,0 | 186 × | 1,580000 s | 1580,0 | 112 × |

Lecture du critère 4, sans enjoliver :

* contre l'objectif « ≤ 80 % du temps Rust/C++ », les moteurs AOT du langage
  sont à **4,5 ×** le C++ (et 12,1 × le Rust) à N = 4096, **6,2 ×** le C++ à
  N = 262 144. Le seuil n'est PAS atteint sur le banc englobant, et l'écart qui
  GRANDIT avec la charge dit que le différentiel est dans le CODE GÉNÉRÉ (les
  `rose_ent`/`Valeur` manipulés dans les boucles), pas dans un coût de
  démarrage — c'est donc le chantier AOT, et il est chiffrable ;
* le seul critère numérique rempli par la voie compilée est « < 150 ms » :
  30 ms à N = 4096, 87 ms à ×64 ;
* le seuil « < 300 ms monofilament » des kernels `bench/suite` est atteint sur
  7 des 10 kernels en AOT (les trois restants : `reduction` 2,01 s,
  `sumsquares` 0,36 s, `sieve` 0,05 s en AOT mais 13,3 s en interpréteur,
  `factorial` 1,03 s) ;
* Philox API 3-arg : interpréteur 43 ms (50 Mo) et 71 ms (100 Mo), AOT 64 ms
  (100 Mo) — sous les 300 ms ;
* `bench/langages` (1 Md) : Rose 762 ms entre C++ 458 ms et Rust 656 ms ;
  `bench/global` (GMP) : Rose 18 287 ms contre C++ 122 ms, Rust 2,66 ms ;
* ce qui est PROUVÉ par ce banc, ce n'est pas la vitesse, c'est la PARITÉ : les
  six moteurs rendent la MÊME chaîne, donc tout écart mesuré est un écart de
  MOTEUR, pas de sémantique. C'est la condition pour que les chiffres ci-dessus
  veuillent dire quelque chose.
Trois corrections trouvées PAR ce banc (preuves dans `bench/englobant/README.md`) :
le transpilateur émettait un `rose_ent` là où le runtime veut un `rose_int`
(AOT et LLVM ne compilaient plus) ; le runner capturait la sortie d'un mot-clé
`time` (tout moteur sain devenait `FAIL`) ; et la comparaison portait sur 5 lignes
dont un temps (faux DIVERGENT garanti).

---

## Réflexion : technologie pour descendre sous Rust et C++ (v3.16+)

> **Orientée par** : `STRATEGIE_DEPASSEMENT.md` — ce que font réellement
> llvm/rustc pour gagner les bancs, l'état de l'art 2025–2026, le diagnostic
> par kernel du banc englobant et les jalons chiffrés J1–J4. Les deux
> documents se complètent : celui-ci dit QUAND et avec quels livrables,
> `STRATEGIE_DEPASSEMENT.md` dit COMMENT et dans quel ordre.

Le goulot n'est pas le calcul mais la mémoire et la gestion du temps de vie.

### 1. Gestion mémoire — au-delà du GC et du borrow-checker
- **Rust** : borrow checker + ownership, zéro coût à l'exécution, friction à la compilation. **C++** : RAII + smart pointers, coût à l'exécution (atomic refcount) + UB.
- **Rose actuel** : `shared_ptr` partout (coût atomic), GMP pour les grands entiers.
- **Pistes** :
  - **Régions / arènes** : allocation par blocs à durée de vie courte, libération O(1). Élimine le refcounting dans les boucles chaudes.
  - **Ownership linéaire optionnel** : mode "affine types" à la Rust pour les chemins critiques, sans annotation lourde.
  - **GC générationnel optionnel** : pour structures dynamiques (tables, listes), compactant par génération, désactivable pour le code de perf.
  - **Pas de GC par défaut sur les tableaux numériques** : mémoire plate + arènes, comme NumPy/Rust.

### 2. Représentation des données — cache-friendly
- **SoA** : imposer SoA pour les tableaux numériques (meilleure vectorisation).
- **Packed stores / non-temporels** : déjà testé, effet marginal (~±10 %). Le vrai levier est la forme SSA (10×).
- **Alignment & padding automatiques** : `assume_aligned` / `restrict` injectés par le compilateur.

### 3. Concurrence & parallélisme
- **Work-stealing scheduler** intégré au runtime (comme Go/Rayon) plutôt que `std::thread` brut.
- **SIMD auto-vectorisé** via MLIR `affine-super-vectorize`.
- **GPU offload transparent** : `rose build --gpu` génère kernel + host code.

### 4. Compilation — au-delà de C++/Rust
- **MLIR comme IR principal** : dialecte `rose` → passes de haut niveau → LLVM. Battre Rust/Zig/C++ sur les opérations de tableaux (fusion, tuilage, vectorisation) avant LLVM.
- **JIT pour le dev** (interpréteur actuel) + **AOT pour la prod** (`rose build`).
- **PGO / BOLT** : optimisation guidée par profil + réordonnancement de code.

### 5. Évaluation paresseuse / fusion de boucles
Fusionner `map` + `reduce` + `remplir_hasard` en une seule passe mémoire (comme les itérateurs Rust, mais automatique). C'est ce que MLIR permet via des passes de fusion.

### 6. NPU — intégration via dialecte tensor
Le NPU n'est pas pertinent pour le RNG. Il devient utile avec un dialecte tensor `rose.tensor` :
- Définir `rose.tensor` (matmul, conv, reduce) → lowering MLIR → IREE/ONNX Runtime.
- Kernels NPU pour les opérations tensorielles (matmul, attention).
- HBM / mémoire unifiée pour les tenseurs.

### Roadmap complète (priorités)
1. Court terme : SSA dans l'émetteur LLVM (Voie 1.3–1.4) — gain ~10× déjà démontré → Rose < 300 ms mono-thread.
2. Moyen terme : arènes (2.1) + Philox + CPU multi-cœurs (Voie 2) + MLIR passes (fusion/vectorisation) → < 100 ms.
3. Long terme : GPU offload transparent + arènes mémoire + SoA → plusieurs × en dessous.
4. NPU : investigation secondaire, utile uniquement avec `rose.tensor`.

### Banc de mesure
- Principal : `bench/global/` (toute la syntaxe).
- Secondaire : `bench/langages/` (mot_de_passe).
- Cible : Rose AOT < 300 ms mono-thread SSA sur le banc global, puis < 100 ms multi-cœurs/GPU.