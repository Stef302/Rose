# Banc englobant Rose (VÉRIFIÉ) — passerelle rose_labs → stable

Exerce le LANGAGE ENTIER dans des boucles chaudes et compare six moteurs sur les
MÊMES valeurs :

    rose (interpréteur/JIT) → rose AOT natif → rose AOT `--llvm`
    → C++ (g++ -O3 -march=native) → Rust (rustc -C opt-level=3 -C target-cpu=native)
    → python3 (référence lente)

Les jumeaux C++, Rust et Python sont des MIROIRS EXACTS du noyau `bench_englobant.rose` :
mêmes formules, mêmes bornes, même largeur native (u64 / f64), même sémantique de
générateur aléatoire, même ordre d'addition flottante, même FORMAT d'affichage.
Les entiers ILLIMITÉS (`rose_ent` > int64, donc GMP) sont hors périmètre : `bench/global/`.

## Lancer

    cd bench/englobant
    ./run_englobant.sh                     # N = 4096 (l'échelle du banc)
    ./run_englobant.sh 262144              # N × 64
    ROSE=/chemin/rose ./run_englobant.sh   # moteur Rose Labs (expérimental)
    OUT=/tmp/bench_suite/n.csv ./run_englobant.sh

## Pourquoi N = 4096 par défaut

Le banc mesure des DÉBITS, pas des temps morts. À N = 1 000 000, S4 seul coûte
~2,5 s par passe et par moteur ; ×5 passes ×6 moteurs ≈ 11 minutes pour UNE
mesure, et chaque moteur travaillerait à une échelle différente de son voisin (les
caches ne tiennent plus pareil) : une divergence n'y prouverait plus une
sémantique, elle prouverait un cache. À N = 4096 l'épreuve reste VRAIE
(8 ops flottantes × 4096 × 5 passes en S4 ; 8 192 blocs Philox ; 10⁶ tirages
Lemire) et elle tient dans la machine.

## Les quatre sections du noyau

| Section | Ce qu'elle exerce | Ce qu'elle attrape |
|---|---|---|
| S4 | arithmétique IEEE stricte, DEUX accumulateurs additionnés à la fin | tout réordonnement : ffast-math, repli FMA (AArch64 + NEON), vectorisation, deux chaînes au lieu d'une |
| S5 | builtin `compter_premiers` (table N+1 octets, borne INCLUSIVE [2..N]) | un crible « approché », une table d'un octet de trop ou de trop peu |
| S6 | `hasard()` en chaîne (splitmix64) + Monte-Carlo π, Lemire `(r·10⁶)>>64` | un RNG vectorisé, une avance d'état sautée, un masque au lieu de Lemire |
| S7 | `remplir_hasard_philox` : compteur philox4x32-10, voie MASQUÉE, stride 8 | glanée par mots alignés, découpage en threads, mélange d'état différent |

Les quatre lignes comparées sont `d45 + d55`, `nb5`, `interieur6`, `tot7`. La 5ᵉ
ligne est un temps : elle est **coupée avant** la comparaison.

## Ce que la parité prouve vraiment

Elle compare des **CHAÎNES**, pas des « à peu près ». Trois conséquences, toutes
mesurées ici :

* la forme affichée est une DONNÉE du banc : l'afficheur de l'interpréteur
  (`texte_flottant`, `rose.cpp`) est le « plus court fidèle » `%.*g`. Un `%.17g`
  ou un `{:e}` fixe écrit autre chose pour le MÊME double = un FAUX DIVERGENT
  entre moteurs qui calculent pareil ;
* une SOMME n'est pas un invariant de permutation : l'ordre de lecture de S7 est
  unique (8 192 blocs < 100 000 → `nthreads = 1`), donc threader ou vectoriser la
  glanée change la VALEUR, et le banc le voit ;
* la Philox comparée est celle du RUNTIME. Mesure du 27/10 : les seize premiers
  octets de `remplir_hasard_philox(tampon(65536), alphabet64, 2026)` sont
  `JB4SoQbKpFghP_Op…`, ce que rend `philox4x32` avec `k0 = graine`, `k1 = graine >> 32`
  — donc **k0 = 2026, k1 = 0** pour la graine 2026, et non k0 = k1 = 2026. Les
  jumeaux portent cette clé mesurée.

## Banc VÉRIFIÉ = exit 0

1. zéro BUILD FAIL (un build réussi = un binaire EXECUTABLE, pas un `.cpp`/`.ll` tronqué) ;
2. zéro FAIL runtime (rc ≠ 0) et zéro DIVERGENT sur les 4 valeurs ;
3. les SIX moteurs comparent entre eux (c'est la comparaison croisée qui fait un
   banc « englobant » — un banc fermé d'un seul opcode ne prouve rien) ;
4. 5 passes par moteur, min retenu. La résolution de `time -p` est SONDEE et
   dépassée par une boucle d'échauffement : un temps de 0,000 s serait une horloge
   muette, et le banc doit rester rouge dans ce cas ;
5. relancer AUSSI `bench/suite/run_suite.sh`, `bench/langages/bench.sh` et
   `bench/global/run_global.sh` : les bancs sont INDÉPENDANTS, un seul vert ne
   prouve pas « vert ».

Sortie : `results.csv` → `kernel,moteur,secondes,rc`.

## Pièges connus du fichier `.rose`

* tout `>` en DÉBUT de ligne, y compris dans un commentaire, est lu comme
  l'afficheur : les commentaires de continuation doivent être préfixés `#` ;
* `…`, `—`, `«`, `»` et `&` dans une ligne de commentaire cassent le lexeur ;
* les variables `t1…tN` sont RÉSERVÉES par le moteur (`t5` → « nom réservé ») ;
* `$b[i] = v` est refusé par le parseur → les décalages big-endian s'écrivent en
  MULTIPLICATIONS par 256, 65 536 et 16 777 216 (exactes, identiques partout) ;
* les produits restent sous 2⁶⁴ par construction : jamais de promotion GMP, donc
  la parité compare des VALEURS, pas des approximations.

## Ce que la course a corrigé (preuves, pas intentions)

* `transpiler.cpp` — `remplir_hasard_philox` émettait `(rose_int)expr` sur un
  `rose_ent` : le AOT et le LLVM ne COMPILAIENT PAS (moteur inerte). Émis
  depuis : `(expr.gros() ? (rose_int)0 : expr.petit())`, qui recopie la règle de
  l'interpréteur (`mpz_fits_slong_p ? (int64_t)mpz_get_si : 0`) : un bornage hors
  int64 tombe à ZÉRO des deux côtés, jamais au mot de poids faible.
* `run_englobant.sh` — `$( { time -p … } 2>/dev/null )` dans une fonction ne
  capture PAS la sortie du mot-clé `time` : tous les moteurs sains étaient
  marqués `FAIL (rc=2)`. La sortie passe par un fichier.
* `head -4` et non `head -5` : la 5ᵉ ligne est un temps, jamais une valeur.

Dernière mesure complète (AArch64 Cortex-A520, 27–28/10/2026, N = 4096, 5 passes,
min retenu, `ROSE_CXX=/usr/bin/clang++`) :

    moteur   secondes
    rose      5.250     interpréteur
    aot       0.0325
    llvm      0.0317
    cpp       0.0081    g++ -O3 -march=native -ffp-contract=off
    rs        0.0039    rustc -O3 -C target-cpu=native
    py        1.330     référence

Parité : les SIX moteurs rendent la MÊME chaîne sur les quatre lignes.

À N = 262 144 (×64), `./run_englobant.sh 262144` sort aussi en exit 0, même
parité sur les six moteurs :

    rose 11.450   aot 0.0866   llvm 0.0877   cpp 0.0141   rs 0.0127   py 1.580
