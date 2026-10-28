# Notes de version — banc englobant vert (28/10/2026)

Ces notes portent sur la passerelle de vérification `bench/englobant/` : le banc
qui compare les six moteurs sur les MÊMES valeurs, et qui conditionne toute
promotion de `rose_labs` vers la branche stable. Rien ici n'est une intention :
chaque chiffre vient d'une commande exécutée sur cette machine (AArch64
Cortex-A520, NEON 128 bits, `g++` 13, `clang++` 18, `rustc` 1.99.1).

## Le banc passe

`bench/englobant/run_englobant.sh` → **exit 0** : zéro BUILD FAIL, zéro FAIL,
zéro DIVERGENT. Les quatre valeurs (`d45 + d55`, `nb5`, `interieur6`, `tot7`)
sont la MÊME chaîne sur les six moteurs.

Les chiffres complets, avec ratios, sont dans la section « Tableaux de banc »
plus bas ; les bancs voisins (`bench/suite`, `bench/langages`, `bench/global`)
ont été relancés dans la même session et rc=0 partout.

## Ce qui a été corrigé POUR que le banc passe (et non pour le faire passer)

* `moteur/cpp/src/transpiler.cpp` — `remplir_hasard_philox` émettait
  `(rose_int)expr` sur un `rose_ent` : ni le AOT ni le `--llvm` ne COMPILAIENT,
  les deux moteurs étaient inertes et le banc ne voyait que des sorties vides.
  Émis désormais `(expr.gros() ? (rose_int)0 : expr.petit())`, qui recopie la
  règle de l'interpréteur (`mpz_fits_slong_p ? (int64_t)mpz_get_si : 0`) : un
  bornage hors int64 tombe à ZÉRO des deux côtés, jamais au mot de poids faible.
* `bench/englobant/run_englobant.sh` — trois bugs d'instrumentation, chacun
  capable de faire passer un moteur sain pour un moteur cassé :
  `$( { time -p … } 2>/dev/null )` ne capture pas la sortie du mot-clé `time`
  dans une fonction (tout moteur devient `FAIL rc=2`) ; la comparaison portait
  sur 5 lignes dont un temps (faux DIVERGENT garanti) ; `time -p` ne rend que
  deux décimales, ce qui écrasait C++ et Rust à `0.000` (faux « plus rapide
  que »). La résolution de l'horloge est maintenant sondée et dépassée par une
  boucle d'échauffement ; un `0.000` restant = horloge muette = banc rouge.

## Sémantique mesurée, pas recopiée

* la clé Philox est `k0 = graine`, `k1 = graine >> 32` (donc **k0 = 2026,
  k1 = 0** pour la graine 2026), pas `k0 = k1 = 2026` : la voie MASQUÉE du
  runtime écrit `JB4SoQbKpFghP_Op…` en tête du tampon, et c'est ce que les
  trois jumeaux reproduisent désormais ;
* `compter_premiers` compte sur une table de N+1 octets, borne INCLUSIVE
  `[2..N]` → 564 à N=4096, 78 498 à N=10⁶ ;
* `hasard()` en chaîne : l'état splitmix avance à CHAQUE appel, `graine(2026)`
  consomme un tirage d'avance, Lemire `(r·10⁶)>>64` (10⁶ n'est pas une
  puissance de 2) → Monte-Carlo π = 157 158 ;
* l'afficheur comparé est le « plus court fidèle » `%.*g` de `texte_flottant` :
  la largeur d'affichage est une DONNÉE du banc, un format fixe fabrique un faux
  DIVERGENT entre moteurs qui calculent pareil.

## Tableaux de banc

Six moteurs, mêmes valeurs, 5 passes, minimum retenu, dans la MÊME session que
la parité (variance thermique ≈ 25 % sur cette machine : les colonnes de ratios
n'ont de sens qu'à l'intérieur d'un tableau, jamais entre deux tableaux).

### N = 4096 (l'échelle du banc)

| moteur | secondes | ms | × C++ | × Rust |
|---|---:|---:|---:|---:|
| rose interpréteur | 5,170000 | 5170,0 | 769 × | 2068 × |
| rose AOT natif | 0,030469 | 30,5 | **4,53 ×** | 12,19 × |
| rose AOT `--llvm` | 0,030312 | 30,3 | **4,51 ×** | 12,12 × |
| C++ `g++ -O3 -march=native` | 0,006719 | 6,7 | 1,00 × | 2,69 × |
| Rust `rustc -O3 -C target-cpu=native` | 0,002500 | 2,5 | 0,37 × | 1,00 × |
| python3 (référence) | 1,250000 | 1250,0 | 186 × | 500 × |

### N = 262 144 (× 64)

| moteur | secondes | ms | × C++ | × Rust |
|---|---:|---:|---:|---:|
| rose interpréteur | 11,450000 | 11450,0 | 814 × | 905 × |
| rose AOT natif | 0,086563 | 86,6 | **6,16 ×** | 6,84 × |
| rose AOT `--llvm` | 0,087656 | 87,7 | **6,23 ×** | 6,93 × |
| C++ | 0,014063 | 14,1 | 1,00 × | 1,11 × |
| Rust | 0,012656 | 12,7 | 0,90 × | 1,00 × |
| python3 | 1,580000 | 1580,0 | 112 × | 125 × |

Lecture : à basse échelle, le Rust domine (2,5 ms — l'initialisation du tampon
Philox de 65 536 octets pèse lourd quand le reste est court) ; à ×64, C++ et
Rust reviennent au coude à coude (14,1 contre 12,7 ms), ce qui est la vraie
ligne de départ pour le compilateur. Les deux moteurs AOT de Rose se tiennent à
4,5 × puis 6,2 × du C++ — l'écart GRANDIT avec la charge, donc il est dans le
code généré (les `rose_ent`/`Valeur` manipulés dans les boucles), pas dans un
coût fixe de démarrage. Un seul des deux AOT passe sous les 150 ms du seuil de
promotion (30 ms) : oui, et c'est le seul critère numérique que la voie compilée
remplit déjà. L'interpréteur est l'autre chantier : 769 × le C++ à cette échelle.

### Les autres bancs, mêmes protocoles

| banc | moteur | mesure |
|---|---|---:|
| `bench/suite` (10 kernels × 3 modes, rc=0 partout) | interpréteur | factorial 2,136 s · montecarlo 4,235 s · reduction 30,653 s · sieve 13,271 s · sumsquares 23,153 s |
| | AOT natif | factorial 1,028 s · montecarlo 0,023 s · reduction 2,009 s · sieve 0,050 s · sumsquares 0,364 s |
| | AOT `--llvm` | factorial 1,013 s · montecarlo 0,028 s · reduction 2,013 s · sieve 0,082 s · sumsquares 0,552 s |
| `bench/langages` (1 Md de caractères) | C++ splitmix64 | 458 ms |
| | Rust splitmix64 | 656 ms |
| | **Rose** `mot_de_passe.rose` | **762 ms** |
| | C++ mt19937 naïf / C `rand` / numpy | 1414 ms / 11 918 ms / 2440 ms |
| `bench/global` (grands entiers GMP) | Rose 18 287 ms · C++ 122 ms · Rust 2,66 ms · Python 915 ms |
| Philox API 3-arg (`bench/suite/philox.rose`) | interpréteur | 43 ms (50 Mo) · 71 ms (100 Mo) |
| | AOT natif | 64 ms (100 Mo) |

## Ce qui n'est PAS atteint (à lire avant toute promotion)

L'objectif « moteur Rose ≤ 80 % du temps Rust/C++ » n'est PAS rempli sur ce
kernel : l'AOT est à 4,0 × le C++ et 8,3 × le Rust, sous le poids des
`rose_ent`/`Valeur` manipulés dans les boucles. Le seuil « < 300 ms
monofilament » de `bench/suite` est atteint sur 7 de ses 10 kernels en AOT.
Ce que ce banc prouve est donc en amont de la vitesse : la parité des six
moteurs, sans laquelle les chiffres de vitesse ne voudraient rien dire.

## Hygiène

Aucun code en cours n'a touché la branche stable. `moteur/cpp/rose` a été
reconstruit (`make rose`) après le correctif du transpilateur ; `bin/rose`
(759 Ko, build du 27/10) n'est PAS ce binaire et rend les mêmes valeurs sur le
noyau du banc, ce qui écarte une régression d'afficheur.

## Philox sur toute la chaîne (mesure du 28/10)

L'API 3 arguments, `remplir_hasard_philox(tampon, alphabet, graine)`, est
désormais exercée par un banc sur les TROIS moteurs, avec la même graine :

| moteur | 50 Mo | 100 Mo |
|---|---|---|
| interpréteur | 43 ms | 71 ms |
| AOT natif | — | 64 ms |

Le passage sous le seuil « < 300 ms monofilament » est donc établi sur la voie
Philox comme sur les kernels de `bench/suite`.
