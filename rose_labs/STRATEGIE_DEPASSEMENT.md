# Stratégie de dépassement — comment Rose vise Rust/C++

Ce document est la feuille de route de `rose_labs/` : il répond à une seule
question, **par quoi passe réellement le dépassement de Rust/C++ en vitesse**,
d'après ce que font ces compilateurs, d'après ce que mesure le banc englobant,
et d'après ce que le moteur Rose fait aujourd'hui. Il n'y a ici ni souhait ni
promesse : seulement des mécanismes identifiés, des mesures et un ordre
d'attaque. Les critères de promotion restent dans
`rose_labs/PLAN_GPU_CPU_NPU.md`.

## 0. La ligne de départ, mesurée (28/10/2026, AArch64 Cortex-A520)

Banc englobant N=4096, 5 passes, min, même session (voir
`RELEASE_NOTES_bench_englobant.md`) :

    C++   0,0081 s   (1,00 ×)
    Rust  0,0039 s   (0,48 × du C++ — Rust GAGNE ce banc)
    AOT   0,0314 s   (3,9 × du C++ ; 6,2 × à N=262 144)
    interp 5,23 s    (644 ×)

Trois lectures qui structurent toute la stratégie :

1. **Le différentiel grossit avec la charge** (3,9 × → 6,2 ×) : ce n'est pas
   un coût de démarrage, c'est le **code généré** dans les boucles. Le levier
   est donc dans le transpilateur/le backend, pas dans le runtime.
2. **Rust bat C++ sur ce terrain** : la cible n'est pas « un -O2 distrait »,
   c'est llvm avec vectorisation, PME et promotion de boucles activées. Toute
   stratégie qui viserait « à peu près clang » arrivera deuxième.
3. **La parité est déjà prouvée sur six moteurs** : chaque gain mesuré ensuite
   sera un gain de MOTEUR, pas de sémantique. C'est la condition rare qu'il
   fallait d'abord établir — elle est établie.

## 1. Ce que font réellement Rust et C++ pour être vite

Ce ne sont pas des « bons optimiseurs » génériques ; ce sont des choix
précis, tous observables et datés.

### 1.1 Le modèle de coût connu d'avance (le vrai avantage de Rust)

* `u64`/`f64` sont des **mots machine**, `Vec<T>` est `{ptr,cap,len}` de
  trois mots : le compilateur sait, sans le prouver, où est chaque octet.
  Rose oppose à cela un `rose_ent` de 16 octets avec promotion GMP possible à
  CHAQUE addition, et un `Valeur` à 32 octets : chaque opération en boucle
  doit TESTER la représentation. Le coût n'est pas l'arithmétique, c'est le
  test.
* C++ fait la même économie par contrat du langage (UB sur overflow = pas de
  test). Rose ne peut pas choisir l'UB (sécurité mémoire = la raison d'être),
  donc il doit obtenir le même résultat par **preuve**, pas par contrat.

### 1.2 Les passes qui gagnent les bancs (llvm 18 / rustc, mesurables)

1. **LICM** (loop-invariant code motion) : sortir divisions et constantes des
   boucles — c'est la première victoire du S4 du banc chez llvm.
2. **Réassociation restrictée** (`-ffast-math` est TRICHE : interdit ici, la
   parité des chaînes l'interdit) : llvm ne vectorise une réduction de
   flottants qu'avec `llvm.parallel.reduce` ou `-fassociative-math` explicite.
   Autrement dit : **le langage doit pouvoir DÉCLARER la réassociation**, ou
   le compilateur refusera de la faire — c'est une décision de syntaxe Rose,
   pas d'optimiseur.
3. **FMA** : fusion `a*b+c` en une instruction — d'où l'anti-FMA forcé
   (`-ffp-contract=off`) du banc pour comparer ce qui est comparable.
4. **Vectorisation** (SLP + boucles) : sur cette machine, NEON 128 bits seul,
   soit ×2 en double ; la vectorisation d'une réduction exige un ordre
   d'addition DÉclaré (arbres de réduction = valeurs différentes : le banc
   l'a montré avec ses deux accumulateurs).
5. **PMB / distribution de prédicats** (`llvm.loop.vectorize.enable-predicated-
   memory-access`) : les gather/scatter de S7 (stride 8) ne se vectorisent
   que par là.
6. **PGO** (profil-guidé) : llvm/rustc spécialisent les branches chaudes,
   dévectorisent le rare, déplient au bon endroit. Gain typique 5–15 % sur les
   boucles de banc — et c'est la passe la moins chère à brancher pour Rose,
   puisque le transpilateur émet déjà du C++/LLVM IR.
7. **LTO + promotion de l'allocation** : `Vec` alloué hors boucle, inliné
   dedans, souvent éliminé (SROA). Les allocations Rose (`rose_ent` GMP,
   `Valeur`, tampons) doivent subir le même sort : allocation en arène de
   frame, jamais `new` par itération.
8. **`noalias`/`restrict` et TBAA** : llvm prouve l'absence d'alias pour
   réordonner les accès tampon. L'IR Rose doit PORTER ces métadonnées, sinon
   chaque boucle mémoire reste séquentielle.

### 1.3 Ce que les champions du domaine font en plus

* **Julia** : JIT par fonction spécialisée sur les TYPES des arguments
  (type-inference → LLVM). Leçon : spécialiser par profil de types observé,
  pas interpréter un type dynamique générique.
* **V8** : inline caches + chaînes de compilation (interp → sparkplug →
  maglev → turbine). Leçon : le palier payant est celui où les types sont
  DES GUIDES, pas des devinettes.
* **Mojo** : pas de GC, ownership au compilateur, vectorisation par
  déclarations explicites (`@parameter`, `Simd[DTYPE, N]`). Leçon : la
  vectorisation d'un langage « sûr et exact » passe par une **porte explicite**
  dans le langage, pas par un optimiseur devin.
* **Cranelift/rustc** : le backend lit des annotations (`#[target_feature]`) —
  la cible CPU est une donnée du programme, pas de la ligne de commande.

### 1.4 L'état de l'art 2025–2026 (recherché le 28/10/2026, sources citées)

* **Les réductions flottantes exactes existent** : la doc officielle LLVM
  (Vectorizers) confirme que la vectorisation FP exige
  `-fassociative-math`, MAIS que sur **AArch64** LLVM sait produire des
  « ordered reductions » qui PRÉSERVENT le résultat exact — moins rapides que
  la réassociation, standards-conformes. C'est exactement la voie de Rose :
  ordered reduction d'abord, porte déclarative (§3.2) pour le reste.
* **Polly change de dimension** : intégré au pipeline officiel LLVM depuis la
  20, et l'extension MMTR (PLDI 2025, DOI 10.1145/3719276.3725197) remplace
  des boucles imbriquées prouvées équivalentes par des appels optimisés
  (jusqu'à ÷200 sur Polybench via OpenBLAS). Leçon : le gain n'est plus dans
  « un meilleur vectoriseur » mais dans la RECONNAISSANCE de motifs — un
  émetteur RIR qui sait dire « cette boucle est une réduction par arbre ×2 »
  récolte ce que l'optimiseur devin ne récolte jamais.
* **PGO multiplié par la spécialisation** : RIFS (CC'26) ajoute +6,3 % au‑dessus
  de `-O3 + PGO` en spécialisant les fonctions sur les arguments invariants
  observés à l'exécution. Pour Rose : les bornes de `pour … vers` et les
  tailles de tampons sont précisément ces arguments — un profil PGO qui les
  enregistre ouvre une spécialisation gratuite.
* **La vérification symbolique comme filet** : CoV (CGO 2026) n'intègre des
  transformations risquées (LLM ou autres) qu'après Alive2 + vérification à
  l'exécution. C'est le modèle de discipline qui doit précéder toute passe
  expérimentale de `rose_labs/` : une transformation non prouvée ne sort pas
  du laboratoire — le banc englobant six moteurs est l'équivalent Rose de ce
  filet.
* **L'apprentissage des passes est mesurable mais pas encore rentable** :
  l'étude empirique de la pipeline `-O3` (LLVM Dev'Meeting 2026 poster,
  Bruzzone) montre que l'essentiel du gain vient d'un SOUS-ENSEMBLE SURPRENANT
  de passes et que le compte d'instructions IR est un mauvais prédicteur.
  Leçon : mesurer le banc par passe (sondages `-mllvm -opt-bisect-limit`)
  avant d'investir dans une passe de plus.

## 2. Le diagnostic Rose, par kernel (où part le temps, mesuré)

| section | écart AOT/C++ à N=4096 | cause identifiée | levier |
|---|---|---|---|
| S4 flottants | dominant (l'essentiel du total) | le transpilateur passe les scalaires par `Valeur`/`rose_ent` au lieu de `double` en registre | §3.1 |
| S5 crible | moyen | bornes et table recréées par passe | §3.1 + LICM |
| S6 MC-π | moyen | la chaîne splitmix est inherently séquentielle (anti-SIMD voulu) : c'est le PIRE terrain pour la vectorisation, le meilleur pour le code généré pur | §3.1 |
| S7 philox | faible (déjà ~le même ordre : l'appel runtime fait le travail) | la glanée stride 8 en `tampon_case` est un appel par octet | §3.3 |

La conclusion du tableau est la colonne vertébrale du plan : **≈ tout se joue
en S4, donc se jouer §3.1 suffit probablement à diviser l'écart par deux ou
trois**, et le reste vient après.

## 3. Les trois chantiers, dans l'ordre de rentabilité

### 3.1 Chantier A — scalaires en registre (le plus gros, le premier)

Objectif : dans le code émis par `transpiler.cpp`, une variable entière ou
flottante **jamais promue** dans une boucle devient un `int64_t`/`double`
C++ local, pas un `rose_ent`/`Valeur`.

* **preuve d'absence de promotion** par analyse de flot : la variable n'est
  affectée que par des opérations dont le domaine est borné int64 (littéraux,
  bornes de boucle, résultats de `tampon_case`), et n'est jamais passée à une
  builtin non typée. Cette analyse est décidable sur le RIR existant : le
  banc englobant tout entier la satisfait.
* repli conservateur : si le doute existe, on garde `rose_ent` — la parité
  protège (le banc attrape la moindre erreur de promulgation, cf. la règle
  d'or : une divergence = un bug, jamais un arrondi).
* cible chiffrée : S4 rendu en boucle C++ pure ⇒ le AOT rejoint le C++ ±
  30 % sur ce kernel, l'écart total passe sous ~2 ×. C'est le premier jalon
  de promotion.

### 3.2 Chantier B — déclarations de vectorisation dans le langage (la porte Mojo)

La vectorisation des réductions flottantes EST interdite à llvm sans preuve
d'associativité. Donc c'est le langage qui doit l'offrir, sous contrôle :

```rose
# proposition (rose_labs, non stable) :
pour $i de 0 vers $N vectorise {          # le mot-clé DONNE la permission
    $s = $s + $a[$i] * $b[$i]
}
```

sémantique figée par contrat, exactement comme le banc l'a fait pour les deux
accumulateurs : la déclaration choisit un **arbre de réduction documenté**
(par exemple ×2 NEON, somme des paires puis repli), dont la valeur est dorée.
Le programmeur garde le choix déterminisme/vitesse ; l'optimiseur a sa
permission. C'est la seule voie qui mène à 0,5–0,8 × C++ sur les kernels
vectorisables, et c'est une décision de SPEC, donc un chantier `rose_labs`.

### 3.3 Chantier C — PGO et métadonnées d'alias (le moins cher, le dernier %)

* le transpilateur émet du C++ compilable par `clang -O3 -march=native` :
  brancher un flux PGO (deux passes, profil ramené par `rose bench`) ne
  demande que de la plomberie autour d'un pipeline existant ;
* annoter les accès tampon en `restrict`/`noalias` dans le LLVM IR émis
  (chaque builtin de tampon sait qu'il ne touche que SON tampon) ;
* gains attendus : 5–15 % sans toucher à la sémantique.

## 4. Ce que la stratégie NE fait pas

* pas de `-ffast-math`, pas de vectorisation silencieuse des flottants, pas
  de wrap-around entier : ce qui est gagné contre la parité est perdu en
  tant que langage ;
* pas de GC : la sécurité mémoire de Rose reste l'arithmétique vérifiée et
  l'absence d'UB, pas un ramasse-miettes ;
* pas de course poursuite sur les bancs fermés d'un opcode : le juge reste
  le banc englobant (six moteurs, mêmes chaînes), complété par
  `bench/suite`, `bench/langages`, `bench/global` ;
* aucun de ces chantiers n'entre en stable sans passer les neuf suites de
  `SPECIFICATIONS.md` §4 dans la même session.

## 5. Jalons chiffrés (le seul langage accepté ici : le ratio du banc)

| jalon | critère mesuré | état |
|---|---|---|
| J0 — parité six moteurs | `run_englobant.sh` exit 0 | **atteint 28/10/2026** |
| J1 — scalaires en registre (A) | AOT ≤ 2,0 × C++ sur le banc | à ouvrir |
| J2 — PGO + noalias (C) | AOT ≤ 1,5 × C++ | — |
| J3 — porte de vectorisation (B) | AOT ≤ 1,0 × C++ sur ≥ 3 sections | — |
| J4 — critère du plan | AOT ≤ 0,8 × Rust/C++ sur ≥ 5 kernels | promotion stable |

Chaque jalon se mesure dans la MÊME session que la parité, min de 5 passes,
sans jamais toucher aux valeurs attendues.
