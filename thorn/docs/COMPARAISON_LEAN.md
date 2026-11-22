# Thorn 0.1.73 vs Lean 4.36 core — mesures nouvelles

Document créé le **22/11/2026**. Fichier possédé par cette mission :
`thorn/docs/COMPARAISON_LEAN.md` (nouveau). Aucun commit, aucune publication,
aucune écriture dans `thorn/src/**`, `rose/src/**`, `compilateur/**`,
`thorn/tests_*.rose`, `thorn/docs/BENCH_LEAN.md` ou `thorn/exemples/**`.

**Ce document n'est pas une recopie de `BENCH_LEAN.md`.** Il apporte une campagne
de mesure **nouvelle**, avec de nouveaux fichiers générés par
`thorn/scripts/compare_lean.sh`, sur un ensemble de sept énoncés dont deux
(les lemmes d'ordre) n'y étaient pas mesurés.

---

## 1. Protocole

```
$ bash thorn/scripts/compare_lean.sh 3
date UTC    : 2026-11-22T18:23:47Z
rose        : Rose 0.1.73 — langage de programmation en français (...)
lean        : Lean (version 4.36.0, aarch64-unknown-linux-gnu, commit 611c117bfaf7b7ae1cfcf246ce6fba31e8f509e9, Release)
répétitions : 3 (séquentielles, un seul process à la fois)
messages    : /usr/bin/time -v —Elapsed + Maximum resident set size (kbytes)
```

- **MESURÉ ICI** : toutes les valeurs du tableau §3, obtenues le 22/11/2026 à
  18:23–18:24 UTC. Brut conservé dans `/tmp/opencode/thorn_lean_compare/brut.txt`,
  tableau machine dans `/tmp/opencode/thorn_lean_compare/tableau.md`.
- **CITÉ** : une seule valeur dans ce document, explicitement marquée, au §5
  (taille de mathlib, déjà citée dans `BENCH_LEAN.md`). Tout le reste est mesuré.
- Lean est lancé en **core**, sans mathlib (non installée) : uniquement `Init`.
- Un coeur est pris par C2 (self-hosting) : les 42 exécutions ont été faites
  **en séquentiel**, jamais en parallèle.
- Médiane sur 3 valeurs (`sort -n`, élément central). Le temps est l'*elapsed*
  de `/usr/bin/time -v`, la mémoire le *Maximum resident set size* du même
  message (pic RSS du process, pas du système).

### 1.1 Fichiers générés (tous hors dépôt)

Thorn : `thorn_identite.thn`, `thorn_reflexivite.thn`, `thorn_symetrie.thn`
(écrits par le script), `thorn_associativite.thn` et `thorn_commutativite.thn`
(copiés de `thorn/preuves/`), `thorn_ordre_refl.rose`, `thorn_ordre_trans.rose`.
Lean : `lean_*.lean` correspondants, un par énoncé.

Exemple — ordre, côté Thorn (généré) et côté Lean :

```rose
importer "thorn/src/ordres.rose"
$reg = ord_registre()
$par = ord_trouver($reg, "Nat/≤/PartialOrder")
ord_lance("ordre/trans[Nat]", ord_lem_trans($par), ta_ctx(), $cpt)
>> "OK=" + chaine($cpt["r"]) + " ECHEC=" + chaine($cpt["e"])
```

```lean
theorem ordre_trans : ∀ a b c : Nat, a ≤ b → b ≤ c → a ≤ c := by
  intro a b c h1 h2
  exact Nat.le_trans h1 h2
#check ordre_trans
```

Les sept fichiers des deux camps ont été rejoués à la main après la campagne :
tous rendent un verdict `OK` (Thorn) / un `#check` valide (Lean), rc=0 partout.

---

## 2. Brut (3 répétitions, `REP i : secondes / Ko`)

```
thorn_identite       0.31/22868   0.31/23064   0.32/22004
lean_identite        1.29/451548  0.33/457040  0.34/457020    ← REP 1 à froid
thorn_reflexivite    0.33/30268   0.33/29608   0.32/26476
lean_reflexivite     0.33/456376  0.36/456712  0.39/456720
thorn_symetrie       0.34/30912   0.34/31540   0.34/31132
lean_symetrie        0.56/456420  0.39/456812  0.33/456540
thorn_associativite  1.19/247736  1.20/248204  1.33/247588
lean_associativite   0.34/457360  0.36/457352  0.33/456984
thorn_commutativite  0.68/141728  0.69/141816  0.69/142336
lean_commutativite   0.32/457112  0.31/457308  0.33/457352
thorn_ordre_refl     0.09/80056   0.08/79448   0.08/79768
lean_ordre_refl      0.33/457200  0.36/457104  0.32/457256
thorn_ordre_trans    0.13/97272   0.10/96988   0.12/97276
lean_ordre_trans     0.39/457208  0.36/457528  0.33/456952
```

Le premier lancement de Lean (`identite`, 1,29 s) est un démarrage à froid ;
la médiane le neutralise. C'est précisément pour cela que 3 répétitions et des
médianes sont utilisées, et non une mesure unique.

---

## 3. Tableau comparatif — toutes valeurs MESURÉES ICI

| Énoncé | Thorn méd. s | Thorn méd. Ko | Lean méd. s | Lean méd. Ko | Ratio temps (Thorn/Lean) | Ratio RSS |
|---|---:|---:|---:|---:|---:|---:|
| identite | 0.31 | 22868.00 | 0.34 | 457020.00 | 0.91 | 0.05 |
| reflexivite | 0.33 | 29608.00 | 0.36 | 456712.00 | 0.92 | 0.06 |
| symetrie | 0.34 | 31132.00 | 0.39 | 456540.00 | 0.87 | 0.07 |
| associativite | 1.20 | 247736.00 | 0.34 | 457352.00 | 3.53 | 0.54 |
| commutativite | 0.69 | 141816.00 | 0.32 | 457308.00 | 2.16 | 0.31 |
| ordre_refl | 0.08 | 79768.00 | 0.33 | 457200.00 | 0.24 | 0.17 |
| ordre_trans | 0.12 | 97272.00 | 0.36 | 457208.00 | 0.33 | 0.21 |

Ratio < 1 = Thorn plus rapide / moins gourmand. Ratio > 1 = Lean devant.

---

## 4. Lecture honnête — ce que les chiffres disent, et ce qu'ils ne disent pas

### 4.1 Là où Thorn gagne (mesuré)

- **Légèreté mémoire sur petit terme** : 22 868 Ko contre 457 020 Ko sur
  `identite`, soit un rapport de **0,05**. Même ordre de grandeur sur
  `reflexivite` (0,06) et `symetrie` (0,07). Le pic RSS de Lean est dominé par
  le coût de démarrage de son environnement, pas par l'énoncé.
- **Lemmes d'ordre pré-élaborés** : `ordre_refl` (0,08 s) et `ordre_trans`
  (0,12 s) sont **3 à 4 fois plus rapides** que leurs équivalents Lean core
  (0,33 / 0,36 s). Ici Thorn paie le chargement de `ordres.rose`, mais le lemme
  est déjà construit dans un registre : le travail par énoncé est minuscule.
- **Temps sur petit `.thn`** : 0,31–0,34 s, dans le même bruit que Lean
  (0,32–0,39 s) — Thorn est devant de 5 à 13 % sur ces trois cas.
- **Zéro dépendance** : `./rose/rose` suffit, aucun `$PATH`, aucune installation,
  aucun fichier d'environnement hors dépôt. Lean, ici, vit dans `/dev/shm/leanhome`
  avec un `elan` dédié.
- **Transparence du terme de preuve** : `ex09_induction.rose` montre
  `verifier` et `convertible` séparément ; `--trace` affiche le terme sur les
  buts simples (`refl Nat 0`). C'est pédagogiquement supérieur à un `#check`.

### 4.2 Là où Lean gagne (mesuré et/ou constaté)

- **Preuves de bibliothèque** : `associativite` = **3,53× plus rapide** chez Lean
  (0,34 s contre 1,20 s) et `commutativite` **2,16×** (0,32 s contre 0,69 s).
  Ces deux lois sont `Nat.add_assoc` / `Nat.add_comm`, démontrées une fois dans
  `Init` et réutilisées. Thorn reconstruit le terme par `nat_rec` à chaque lancement.
- **Expressivité du noyau** : Lean a les univers, la cumulativité, η, l'ι,
  les implicites, les classes, les quotients. Thorn : `Type(0)` unique, β+α,
  axiomes de signature pour `plus_0_l` / `plus_s_l`.
- **Écosystème et cache** : Lean a `lake`, le cache d'olean, l'incrémental,
  le linter, le mode interactif. Thorn reconstruit tout : `tests_algebre`
  **29,69 s**, contre ~0,3 s pour un terme isolé (mesuré dans `CAPACITES.md`).
- **Bibliothèque** : 1,9 million de lignes de mathlib (**CITÉ**, source déjà
  donnée dans `BENCH_LEAN.md` §3.5) contre un prototype de noyau. Ce n'est pas
  un objet de même nature, et le chiffre n'est pas une mesure locale.

### 4.3 Limite méthodologique — ne pas sur-interpréter le tableau

« Même énoncé » ne veut **pas** dire « même quantité de travail » :

1. Côté Thorn, `associativite.thn` est une preuve de 20 lignes de tactiques,
   vérifiée par `verifier` + `convertible`, avec `plus_0_l` / `plus_s_l` comme
   axiomes. Côté Lean, `exact Nat.add_assoc` réutilise un théorème déjà prouvé.
   Le ratio 3,53 mesure un coût de *construction*, pas la difficulté de la loi.
2. Les deux `ordre_*` côté Thorn ne sont pas des preuves écrites par
   l'utilisateur : ce sont des lemmes de registre pré-élaborés. Le ratio 0,24
   mesure l'accès à un registre, pas une inférence.
3. Les pics RSS de Lean incluent le démarrage du runtime ; ceux de Thorn
   incluent le démarrage de l'interpréteur Rose. À périmètre égal (process froid),
   la comparaison reste valable, mais elle est spécifique à cette machine.
4. Aucun cache n'a été réchauffé volontairement, et aucun `sync`/`drop_caches`
   n'a été fait : les mesures sont des constats datés, pas des consignes.

---

## 5. Reproductibilité

```
cd /home/clausake/Stefan/ROSE
bash thorn/scripts/compare_lean.sh 3
cat /tmp/opencode/thorn_lean_compare/brut.txt
cat /tmp/opencode/thorn_lean_compare/tableau.md

# catalogue des capacités et garde-fou de périmètre :
bash thorn/scripts/capacites.sh
bash thorn/scripts/verifier_proprete.sh
```

Rejouer `compare_lean.sh` écrase les fichiers générés dans
`/tmp/opencode/thorn_lean_compare/` et produit de nouvelles médianes : attendre
des écarts de quelques centièmes de seconde, surtout sur `lean_*` (bruit de
démarrage). Seules les valeurs du §3, datées du 22/11/2026 à 18:23 UTC, sont
celles de cette campagne.

---

## 6. Ce qui reste à mesurer

- Le même bench avec cache chaud côté Thorn (sérialisation de l'environnement
  vérifié) : c'est le geste qui testerait réellement l'écart incrémental.
- Une charge « suite entière » (`tests_algebre`, `tests_hierarchie`) face à un
  `lake build` incrémental — volontairement non lancée ici (C2 occupe un coeur,
  et ces suites dépassent 60 s).
- L'ι-réduction et les transparents dans le noyau, qui rendraient
  l'associativité comparable à `Nat.add_assoc` sur un pied d'égalité réel.
