# Spécifications du langage Rose — v3.14.0 (candidat de release)

Document de référence pour la release stable. Il décrit ce que le moteur
officiel (C++, `moteur/cpp/`) **fait réellement**, mesuré sur cette machine,
et non ce qu'un moteur futur devrait faire. Les oracles (Rust, Zig, Python,
Julia) doivent rendre des sorties identiques au caractère près
(`moteur/tests/parite.py`, 119 comparaisons, 0 divergence) : la parité EST la
spécification exécutable ; ce document en est la lecture humaine.

- Version du moteur : `rose version` → `Rose 3.14.0 — moteur natif C++/GMP`
- Licence : MIT. Binaire autonome : GMP liée statiquement, aucune dépendance
  d'exécution (ni Python, ni gcc, ni .NET).
- Plateformes livrées : Linux x64/arm64 (compilation `make`), Windows x64
  (setup + zip, UCRT seule requise).

---

## 1. Syntaxe

### 1.1 Structure d'un programme

Un fichier `.rose` est une suite d'instructions. Le nom d'une variable est
préfixé `$` ; les noms d'affichage sont introduits par `>>`.

```rose
$total = 0
pour i de 1 vers 11 {
    $total = $total + i * i
}
>> $total          # affiche 385
```

### 1.2 Mots-clés (liste exhaustive, `rose.cpp`, `est_mot_cle`)

```
fn  retour  si  sinon  tant_que  pour  de  vers
vrai  faux  et  ou  non  div
entier  rationnel  flottant  booleen  chaine  liste  table
```

Ces mots sont réservés. Règle de nommage imposée par le moteur
(`verifier_nom_reserve`) : une variable `t` suivie de chiffres (`t1`, `t5`,
`t42`) est RÉSERVÉE à l'usage interne — le parseur la refuse.

### 1.3 Lexeur : ce qui casse, et pourquoi

Le lexage est sensible à la position (choix assumé, figé par les dorés) :

* `>` en **début de ligne** est lu comme l'afficheur, y compris à l'intérieur
  d'un commentaire `#` : une ligne de continuation de commentaire ne doit
  jamais commencer par `>` ;
* les commentaires sont des lignes `#` en ASCII pur : `…`, `—`, `«`, `»` et
  `&` hors code y cassent le parseur (« Caractère inattendu ») ;
* les chaînes acceptent l'UTF-8 complet ; `longueur("🌹")` vaut 1 (le moteur
  compte des points de code, pas des octets).

### 1.4 Types et déclaration

| type | littéral | largeur / domaine |
|---|---|---|
| `entier` | `42`, `-7` | signé 64 bits **tant que ça tient**, promotion GMP (entier illimité) dès le dépassement — jamais de wrap-around silencieux |
| `rationnel` | `1/3` | exact, GMP, rendu `a/b` (`chaine(3/4)` → `3/4`, JAMAIS `0.75` : c'est LA règle des deux rendus) |
| `flottant` | `0.5` | IEEE-754 double ; l'affichage est le « plus court fidèle » `%.*g` (plus petite précision p ∈ 1..17 qui se relit bit à bit) |
| `booleen` | `vrai` / `faux` | — |
| `chaine` | `"bonjour"` | UTF-8, points de code |
| `liste` | `[1, 2, 3]` | hétérogène, `joindre`, indices `[i]` |
| `table` | `table()` | clés→valeurs, `inserer`, `cle_existe`, `cles`, `valeurs` |
| `tampon` | `tampon(1024)` | octets bruts, accès par `tampon_case`/`tampon_pose` |

### 1.5 Fonctions

```rose
fn fib(n: entier) -> entier {
    si (n < 2) { retour n }
    retour fib(n - 1) + fib(n - 2)
}
>> fib(28)        # 317811
```

Types de paramètres et de retour annotés (`entier`, `flottant`, `booleen`,
`chaine`, `liste`, `table`, `rationnel`) ; récursion directe et croisée ;
portée lexique.

### 1.6 Boucles et conditions

```rose
si (x > 0) { … } sinon si (x == 0) { … } sinon { … }
tant_que (cond) { … }
pour i de 0 vers 10 { … }        # borne EXCLUE : i ∈ [0, 10[
```

Piège figé par les tests : `pour … vers N` n'inclut PAS N. Le builtin
`compter_premiers(N)` en revanche compte sur `[2..N]` INCLUS (564 à N=4096).

### 1.7 Opérateurs

`+ - * / // ^ %` ; `div` (division entière) ; comparaisons `== != < <= > >=` ;
`et ou non` ; affectation `=` (forme symbolique `←` acceptée) ; notation
symbolique `⋅ × ÷ √ π ² ³`. La division `/` entre entiers produit un
**rationnel exact** (`1/3 + 1/6` → `1/2`), jamais un arrondi.

---

## 2. Bibliothèque standard (liste exhaustive, table `STDLIB` du moteur)

Entrées/sorties : `afficher`, `saisir_entier`, `saisir_flottant`,
`afficher_erreur`, `quitter`, `temps`, `duree`, `millisecondes`,
`nb_arguments`, `argument`, `lire_texte`, `ecrire_texte`.

Maths exactes : `factorielle`, `fibonacci`, `pgcd`, `ppcm`, `puissance`,
`est_premier`, `compter_premiers`, `nb_chiffres`, `nombre_pi`, `abs`, `min`,
`max`, `partie_entiere`, `fraction`, `entier`, `chaine`, `caractere`,
`longueur`, `sous_chaine`, `concatener`, `ordre`.

Tables et listes : `table`, `inserer`, `cle_existe`, `supprimer`, `cles`,
`valeurs`, `joindre`.

RNG reproductible et vrai : `graine`, `graine_systeme`, `graine_vrai`,
`hasard`, `hasard_systeme`, `hasard_vrai`, `rng_materiel`, `rng_mat`.

Tampons et octets : `tampon`, `octets`, `tampon_case`, `tampon_pose`,
`remplir_hasard`, `remplir_hasard_philox`.

FFI : `charger`, `fermer`, `appeler0`, `appeler1`, `appeler2`
(`@dll` / `dynamique::`).

### 2.1 Sémantique RNG figée par le banc englobant (preuves 27–28/10/2026)

* `hasard(n)` : splitmix64 en chaîne, l'état avance à CHAQUE appel ;
  `graine(2026)` = état := 2026 PUIS un tirage d'avance ; réduction de
  Lemire `(r·n) >> 64` (jamais un masque sauf si n est puissance de 2) ;
* `remplir_hasard_philox(tampon, alphabet, graine)` : philox4x32-10, clé
  `k0 = graine`, `k1 = graine >> 32` (graine 2026 → k0=2026, k1=0), voie
  masquée `& (|alphabet|-1)` quand la taille est une puissance de 2,
  sinon Lemire ; déterministe quel que soit le nombre de threads ;
* `tampon_case(t, i)` : seul accès octet en lecture, lève « index hors
  tampon » au-delà de la taille réelle.

---

## 3. CLI (sortie réelle de `rose --help`)

```
Rose 2.0 — moteur natif (binaire autonome, maths exactes embarquées)
Usage :
  rose prog.rose            interpréter un programme Rose
  rose run prog.rose        interpréter (identique)
  rose build prog.rose [-o sortie] [--llvm]  compiler natif (C++ ou LLVM IR)
  rose comp prog.rose [-o sortie]   produire un exécutable autonome
  rose rir prog.rose             dumper la représentation intermédiaire (RIR)
  rose info                 état du moteur et des dépendances
  rose version              numéro de version
```

`rose build` transpile vers C++ puis invoke le compilateur
(surcharges `ROSE_CXX`, `ROSE_CXXFLAGS`, `ROSE_GMP`, `ROSE_HOME`) ;
`--llvm` passe par l'IR LLVM (AArch64/x86-64).

---

## 4. Contrats de correction (la release est conditionnée à tout vert)

| suite | commande | attendu |
|---|---|---|
| dorés (46 tests) | `moteur/tests/dorés` + `courir.py` | sortie identique en octets bruts |
| roses à tester (12) | `moteur/tests/roses` + `MANIFEST.md` | `.attendu` / `.attendu.err` octet pour octet |
| parité des moteurs | `moteur/tests/parite.py` | 119 comparaisons, 0 divergence |
| tests moteur C++ | suite `moteur/cpp` | 25/25 |
| banc englobant | `bench/englobant/run_englobant.sh` | exit 0 : 0 BUILD FAIL, 0 FAIL, 0 DIVERGENT sur 6 moteurs |
| banc suite | `bench/suite/run_suite.sh` | rc=0 sur 10 kernels × 3 modes |
| banc langages | `bench/langages/bench.sh` | rc=0 |
| banc global (GMP) | `bench/global/run_global.sh` | rc=0 |
| tests site | `node moteur/tests/site/test_moteur.js` | 67/67 |

Dernière mesure complète : 28/10/2026, TOUT VERT (détails et débits dans
`RELEASE_NOTES_bench_englobant.md`).

---

## 5. Promesses et limites de la v3.14.0

**Promesses tenues** : maths exactes sans limite (GMP) ; deux rendus
distincts rationnel/flottant ; AOT natif et `--llvm` compilant et rendant les
MÊMES valeurs que l'interpréteur (prouvé par le banc englobant) ; erreurs qui
parlent (`[Ligne L, Colonne C] …`) ; RNG reproductible à l'état près.

**Limites assumées, documentées, non cachées** :

* vitesse : sur le banc englobant, l'AOT est à 3,9–6,2 × du C++ natif et
  l'objectif « ≤ 80 % du temps Rust/C++ » n'est PAS atteint — le critère
  numérique rempli par la voie compilée est « < 150 ms » (30 ms à N=4096,
  87 ms à ×64). C'est le chantier `rose_labs/` (voir
  `rose_labs/PLAN_GPU_CPU_NPU.md`), pas une promesse de la stable — les
  mécanismes de dépassement et les jalons chiffrés sont dans
  `rose_labs/STRATEGIE_DEPASSEMENT.md` ;
* JIT Windows : absent, repli interpréteur à décisions identiques ;
* `rose build` sous Windows : rejet poli documenté (g++ requis) ;
* `pour … vers` exclut la borne (quirk figé) ; `(-8)^(1/3)` rend 1
  (plancher de l'exposant, quirk fidèlement rendu par les dorés) ;
* philox hors whitelist LLVM : voie C++ émise, documentée.

## 6. Site public et politique de canaux

Le site vit **uniquement sur GitHub Pages** — `https://stef302.github.io/Rose/`,
servi par le dossier `docs/` du dépôt. Règles vérifiées par audit le 28/10/2026 :

* aucune IP interne (VPN, LAN, docker), aucun nom de machine, aucun chemin
  personnel (`/home/…`) dans le site, les scripts, les docs ni les artefacts ;
* le `README-PARTAGE.md` embarqué dans `rose-windows.zip` ne pointe que vers
  GitHub Pages ; l'archive est régénérée par `scripts/build-share-zip.sh`,
  qui vérifie marques UTF-8 et platitude ;
* les empreintes SHA-256 affichées par `index.html` sont RÉELLEMENT mesurées
  sur les fichiers livrés et concordent avec `SHA256SUMS.txt`
  (`sha256sum -c` → OK ; contrôle refait à chaque livraison) ;
* `scripts/start-site.sh` ne sert qu'une prévisualisation locale
  (`127.0.0.1:18080`, surcharge `BIND=`) — jamais une interface réseau privée ;
* publication GitHub Pages : `Settings → Pages → Source: Deploy from a branch`,
  `Branch: main`, `Folder: /docs`. Le dépôt doit être public ; aucun dépôt
  privé de sauvegarde n'est un canal de distribution.

## 7. Processus de release

1. branche `main` = stable ; aucune expérimentation n'y entre ; le
   laboratoire vit dans `rose_labs/` et n'est livrable qu'après avoir passé
   TOUT le §4 ;
2. avant un tag : reconstruire le moteur (`make rose`), relancer les neuf
   suites du §4 dans la MÊME session, consigner les chiffres ;
3. artefacts : `bin/` (Linux), `docs/` (Windows setup + zip, empreintes
   SHA-256 réelles mesurées sur les fichiers livrés), site public sur
   GitHub Pages — le dépôt et le site ne contiennent ni IP interne, ni nom
   de machine, ni chemin personnel (politique vérifiée par audit) ;
4. notes de version obligatoires (`RELEASE_NOTES_*.md`) : chaque affirmation
   chiffrée doit être la sortie d'une commande exécutée.
