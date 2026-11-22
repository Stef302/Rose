# Thorn — une preuve commentée, un bench réel, comparaison avec Lean 4

Document écrit le 22/11/2026. Toutes les valeurs marquées **MESURÉ ICI** ont
été obtenues sur cette machine, avec les commandes exactes données ci-dessous,
le 22/11/2026. Toutes les valeurs marquées **CITÉ** viennent d'une source
externe datée, et ne sont **pas** des mesures faites ici. Aucun commit, aucune
publication, aucune modification de `thorn/src/**`, `rose/src/**`,
`compilateur/**`, `thorn/tests_*.rose` ou `thorn/docs/MATHLIB_ROADMAP.md`.

Fichiers possédés par cette mission : `thorn/docs/BENCH_LEAN.md` (ce fichier),
`thorn/exemples/ex11_mathlib.rose`, `thorn/exemples/ex11_mesure_terme.rose`,
`thorn/bench_thorn.sh`.

---

## 0. Environnement (MESURÉ ICI, 22/11/2026)

```
$ ./rose/rose version
Rose 0.1.73 — langage de programmation en français (maths exactes, JIT, AOT, compilation autonome)

$ nproc
20

$ free -h
               total        used        free      shared  buff/cache   available
Mem:           121Gi        14Gi        79Gi       6.1Mi        29Gi       106Gi
Swap:           15Gi       2.5Gi        13Gi
```

`which lean lake elan` → **aucun binaire installé dans le PATH** (code 1,
sortie vide) au début de la mission. Voir §4 pour l'installation effective.

**Piège de mesure rencontré et résolu.** Une première exécution de
`tests_noyau` a pris **447,55 s** (`/usr/bin/time`) alors que la machine avait
91 GiB de RAM utilisée et **15 GiB de swap plein**. Après retour à 14 GiB
utilisés et 2,5 GiB de swap, le même fichier prend **0,24 s** pour les mêmes
165 contrôles. Ce n'est pas une accélération du code : c'est le coût du
*thrashing*. Tous les chiffres du tableau §3 sont donc pris dans la seconde
condition (swap non saturé), et `free` est relevé dans le même lot que les
mesures. Le premier chiffre est conservé comme avertissement méthodologique.

---

## 1. Exemple réel commenté ligne à ligne — associativité de `+`

Fichier : `thorn/preuves/associativite.thn` (non modifié). Commande exacte :

```
cd /home/clausake/Stefan/ROSE
./rose/rose thorn/src/principal.rose thorn/preuves/associativite.thn
```

Sortie réelle (rc=0) :

```
OK associativite_plus
1 théorème(s), 1 réussi(s)
```

### 1.1 Ce que l'utilisateur écrit

| Ligne du `.thn` | Rôle exact |
|---|---|
| `théorème associativite_plus :` | ouvre une déclaration ; le nom sert au verdict affiché. |
| `pour_tout m : Nat,` | premier Π : le but est `Πm:Nat, …`. Le noyau lie `m` dans la suite. |
| `pour_tout n : Nat,` | deuxième Π, imbriqué dans le premier. |
| `pour_tout p : Nat,` | troisième Π. |
| `(m + n) + p = m + (n + p)` | corps de la proposition : une égalité `ind("=", [a,b])`. |
| `preuve` | sépare l'énoncé du script de tactiques. |
| `induction m` | ouvre deux buts (cas `zero`, cas `succ`) sur `m`. |
| `cas zero :` / `cas succ k ih :` | décrit les motifs et les lieurs effectivement introduits par le noyau (`k` : prédécesseur, `ih` : hypothèse de récurrence). |
| `introduction n` / `introduction p` | dépouille les deux Π restants ; les noms viennent du Π du but, pas du texte de la tactique (`thorn/docs/PILOTE.md`, §format `.thn`). |
| `appliquer trans … (symm …)` | propose un terme de preuve ; le séquenceur élabore les arguments implicites de `trans` (`ta_elab`) puis compose les pas. |
| `fin` | clôt le théorème. |

### 1.2 Ce que le noyau vérifie réellement

D'après `thorn/docs/PILOTE.md` §pipeline, pour **chaque** théorème :

1. lecture → lexage (`thorn/src/lexeur.rose`) → découpage → analyse du but
   (`thorn/src/analyse.rose`) ;
2. **normalisation syntaxique** vers le format canonique : `Nat` devient le
   terme de signature `ind("Nat",…)`, `0` devient `const(zero)`, `+` devient
   le symbole de la signature, `a = b` devient `ind("=", [a,b])` ;
3. `verifier(but, ctx)` — le but est un type bien formé ;
4. séquenceur de tactiques (`thorn/src/tactiques.rose` + `p_exec_cmds` /
   `p_exec_cas` dans `thorn/src/principal.rose`) — construction du terme par
   `nat_rec` avec base et pas ;
5. `verifier(terme, ctx)` — synthèse de type du terme construit ;
6. `convertible(type_synthétisé, but)` — équivalence définitionnelle β+α.

Aucun `OK` n'est codé en dur : le verdict passe par les étapes 5 et 6.
L'hypothèse de récurrence `ih n p` est réellement utilisée dans le pas
`(congr_succ (ih n p))`. Les équations `0+n=n` et `succ k+n=succ(k+n)` sont
des **axiomes de signature** (`plus_0_l`, `plus_s_l`), pas des calculs du noyau
: Thorn ne fait pas d'ι-réduction. C'est une limite documentée, pas un succès
à présenter comme une démonstration from scratch.

### 1.3 Ce que `--trace` donne — et sa limite actuelle

Sur un but simple, `--trace` affiche le terme construit. Exemple réel avec
`thorn/exemples/ex10_ok.thn` (rc=0) :

```
$ ./rose/rose thorn/src/principal.rose thorn/exemples/ex10_ok.thn --trace
  [trace] ex10_refl_zero but = ind(=)
  [trace] ex10_refl_zero terme = app(app(const(refl) : pi(A)(type[0] -> pi(x)(… -> …)) ind(Nat)) const(zero) : ind(Nat))
OK ex10_refl_zero
1 théorème(s), 1 réussi(s)
```

Lisiblement : `refl Nat 0`, dont le type synthétisé est convertible à `0 = 0`.

**Limite mesurée le 22/11/2026** : sur les cinq preuves `thorn/preuves/*.thn`,
dont `associativite.thn`, `--trace` échoue avec
`Rose : erreur — clé absente de la table` (rc=1), alors que la même commande
**sans** `--trace` rend `OK`. Le défaut est dans le chemin de trace du pilote
sur les buts à plusieurs Π ; il n'affecte pas la décision de typage. Je ne
modifie pas `thorn/src/**` : le geste correct est de corriger la lecture de
table dans la fonction de trace de `thorn/src/principal.rose`, puis de
rejouer les cinq preuves avec et sans `--trace`. Le terme de l'induction peut
être observé sans le pilote via `thorn/exemples/ex09_induction.rose`, dont la
sortie réelle est :

```
$ ./rose/rose thorn/exemples/ex09_induction.rose
induction : ok           = vrai
induction : regle        = induction/Nat
induction : nb buts      = 2
induction_resolue : ok   = vrai
induction_resolue : regle = induction/Nat+résolue
verifier(terme) : ok     = vrai
convertible(type, but)  = vrai
```

---

## 2. Nouvel exemple — `thorn/exemples/ex11_mathlib.rose`

But : une **preuve générique réutilisable** — un seul terme de preuve,
appliqué à deux instances — en utilisant la hiérarchie nouvellement livrée
(`thorn/src/hierarchie.rose` : `Magma → Semigroupe → Monoid → Groupe`) et
`thorn/src/instances.rose` (marqueur `Classe(C,A)` + pré-élaboration).

Contenu essentiel (extrait littéral ; `$` = préfixe des variables Rose) :

```
importer "thorn/src/instances.rose"

$e11_reg = mg_registre()
$e11_add = mg_registre_trouver($e11_reg, "Nat/+/0")
$e11_mul = mg_registre_trouver($e11_reg, "Nat/*/1")

mg_lance("assoc4[Nat/+/0]",  mg_lem_assoc4($e11_add), ta_ctx(), $e11_cpt)
mg_lance("assoc4[Nat/*/1]",  mg_lem_assoc4($e11_mul), ta_ctx(), $e11_cpt)
mg_lance("id_unique_d[Nat/+/0]", mg_lem_id_unique_d($e11_add), ta_ctx(), $e11_cpt)
mg_lance("id_unique_d[Nat/*/1]", mg_lem_id_unique_d($e11_mul), ta_ctx(), $e11_cpt)

$e11_m  = th_marqueur("Add", ta_nat())
$e11_ap = th_applique($e11_m, ta_zero(), ta_succ(ta_zero()), $e11_reg, ta_ctx())
# + un refus attendu : Classe(Groupe, Nat) n'a pas d'instance
```

Sortie réelle (rc=0, `/usr/bin/time` : 51,54 s / 4 141 576 Ko) :

```
Thorn — exemple 11 : preuve generique reutilisable (hiérarchie + instances)
registre : 3 instance(s)
OK assoc4[Nat/+/0]
OK assoc4[Nat/*/1]
OK id_unique_d[Nat/+/0]
OK id_unique_d[Nat/*/1]
OK elaboration/Classe(Add,Nat) 0 1 : Nat
OK refus/Classe(Groupe,Nat) sans instance (attendu)
6 contrôle(s) réussi(s), 0 échec(s)
RESULTAT: OK
```

Ce que cela prouve : `mg_lem_assoc4` et `mg_lem_id_unique_d` sont des
**fonctions Rose paramétrées par l'enregistrement d'instance** ; le même terme
est vérifié deux fois, sur `(+, 0)` et sur `(\*, 1)`. La synthèse d'instance
(`th_marqueur` → `th_resoudre` → `th_applique`) est une **pré-élaboration**
devant le noyau : le noyau ne voit qu'un terme ordinaire. Le refus
`Classe(Groupe,Nat)` est contrôlé, pas une erreur brut.

---

## 3. Bench réel

Script reproductible : `bash thorn/bench_thorn.sh 3` → brut dans
`/tmp/opencode/thorn_bench_raw.txt`. Chaque charge est lancée 3 fois
**en parallèle** (20 cœur, chaque `rose` mono-thread ; la concurrence est
explicite ici, les temps muraux restent comparables entre répétitions).
Médiane = `sort -n` des 3 temps.

### 3.1 Commandes exactes

```
/usr/bin/time -f '%e s / %M Ko' ./rose/rose thorn/tests_noyau.rose
/usr/bin/time -f '%e s / %M Ko' ./rose/rose thorn/tests_algebre.rose
/usr/bin/time -f '%e s / %M Ko' ./rose/rose thorn/tests_hierarchie.rose
/usr/bin/time -f '%e s / %M Ko' ./rose/rose thorn/tests_instances.rose
/usr/bin/time -f '%e s / %M Ko' ./rose/rose thorn/src/principal.rose thorn/preuves/associativite.thn
/usr/bin/time -f '%e s / %M Ko' ./rose/rose thorn/exemples/ex11_mesure_terme.rose   # 1 seul verifier()
```

### 3.2 Résultats bruts (3 répétitions, `REP i SECONDES / KILOOCTETS`)

```
--- tests_noyau ---
  REP 2 0.25 s / 174504 Ko
  REP 3 0.25 s / 174076 Ko
  REP 1 0.27 s / 163756 Ko

--- tests_algebre ---
  REP 2 154.22 s / 5787184 Ko
  REP 3 154.41 s / 5780004 Ko
  REP 1 158.80 s / 5729576 Ko

--- tests_hierarchie ---
  REP 1 133.90 s / 5720144 Ko
  REP 2 134.50 s / 5633968 Ko
  REP 3 136.99 s / 5791688 Ko

--- tests_instances ---
  REP 2 34.40 s / 3572888 Ko
  REP 1 34.95 s / 3710912 Ko
  REP 3 34.97 s / 3793912 Ko

--- preuve_thn (associativite.thn) ---
  REP 1 2.77 s / 526232 Ko
  REP 2 2.77 s / 531740 Ko
  REP 3 2.79 s / 536616 Ko

--- terme_individuel (1 seul verifier) ---
  REP 1 0.00 s / 6968 Ko
  REP 2 0.01 s / 6968 Ko
  REP 3 0.01 s / 6968 Ko
```

Les compteurs affichés par les suites (contrôles exécutés une fois hors du
bench, avec sortie visible) : `REUSSIS=165 ECHOUES=0 RESULTAT: OK`,
`34 lemme(s) vérifié(s), 0 échec(s)`, `19 contrôle(s) réussi(s), 0 échec(s)`,
`10 contrôle(s) réussi(s), 0 échec(s)`.

### 3.3 Tableau des médianes

| Charge | Contrôles | Médiane temps | Médiane mémoire résidente | Commande (raccourci) |
|---|---:|---:|---:|---|
| `tests_noyau.rose` | 165 | **0,25 s** | 174 076 Ko | `./rose/rose thorn/tests_noyau.rose` |
| `tests_algebre.rose` | 34 | **154,41 s** | 5 780 004 Ko | `./rose/rose thorn/tests_algebre.rose` |
| `tests_hierarchie.rose` | 19 | **134,50 s** | 5 720 144 Ko | `./rose/rose thorn/tests_hierarchie.rose` |
| `tests_instances.rose` | 10 | **34,95 s** | 3 710 912 Ko | `./rose/rose thorn/tests_instances.rose` |
| `associativite.thn` (preuve complète) | 1 théorème | **2,77 s** | 531 740 Ko | `./rose/rose thorn/src/principal.rose thorn/preuves/associativite.thn` |
| 1 terme vérifié (`verifier`) | 1 | **0,01 s** | 6 968 Ko | `./rose/rose thorn/exemples/ex11_mesure_terme.rose` |

Médianes calculées sur les 3 répétitions brutes ci-dessus (`sort -n`, élément
central). Le script `bench_thorn.sh` imprime désormais ces médianes ; la
correction du champ `awk` (`$3` au lieu de `$2`) a été faite **après** ce run,
donc les lignes `median_sec: 2` du brut ci-dessus sont une erreur d'affichage
du script, pas une mesure — les médianes du tableau ont été recalculées à la
main à partir des lignes `REP`.

### 3.4 Lean — même énoncé, MESURÉ ICI

Lean 4.36.0 a été installé **hors du dépôt**, d'abord dans `/tmp/opencode/elan`
(échec : `/` plein à 94 Mo près, `No space left on device`), puis dans
`/dev/shm/leanhome` (tmpfs de 61 Gio, succès). Aucun fichier du dépôt n'est
touché.

Fichier `/dev/shm/leanhome/thorn_eq.lean` : les 4 mêmes énoncés que
`thorn/preuves/*.thn`, en Lean 4 **core** (sans Mathlib) :

```lean
-- Les MEMES énoncés que thorn/preuves/*.thn, en Lean 4 core (sans Mathlib).
def identite_thorn : ∀ (A : Type) (x : A), A := fun A x => x

theorem reflexivite_thorn : ∀ (A : Type) (x : A), x = x := by
  intro A x
  rfl

theorem associativite_plus_thorn :
    ∀ m n p : Nat, (m + n) + p = m + (n + p) := by
  exact Nat.add_assoc

theorem commutativite_plus_thorn : ∀ m n : Nat, m + n = n + m := by
  exact Nat.add_comm
```

Commande et sortie réelles (3 exécutions, rc=0 à chaque fois) :

```
$ lean --version
Lean (version 4.36.0, aarch64-unknown-linux-gnu, commit 611c117bfaf7b7ae1cfcf246ce6fba31e8f509e9, Release)

$ /usr/bin/time -f '%e s / %M Ko' lean thorn_eq.lean
REP 1 0.35 s / 468860 Ko
REP 2 0.35 s / 468828 Ko
REP 3 0.35 s / 468492 Ko
```

Médiane Lean : **0,35 s / ≈468 828 Ko pour 4 théorèmes** (constat brut, sans
cache de build, process froid à chaque fois). Thorn : **0,33 s / 17 584 Ko**
pour `identite.thn` seul (re-mesuré dans les mêmes conditions, rc=0 ;
une première mesure sous pression mémoire avait donné 0,31 s / 17 632 Ko).
Sur `associativite.thn` seul, voir tableau §3.3.

**Comparaison honnête sur le même énoncé** : l'associativité est *un axiome de
signature* côté Thorn (`plus_0_l`, `plus_s_l`) et *un théorème calculé* côté
Lean (`Nat.add_assoc`, lui-même démontré dans `Init`). Thorn vérifie un terme
que l'utilisateur construit explicitement en 20 lignes de tactiques ; Lean
accepte `exact Nat.add_assoc` en 1 ligne. Comparer les temps ici ne mesure pas
la même quantité de travail : c'est un indice, pas un verdict d'équivalence.

### 3.5 Chiffres externes CITÉS et repères locaux

Les quatre premières lignes sont **CITÉES** (jamais présentées comme mesurées
ici). Les deux dernières sont **MESURÉES ICI** et servent de repères de taille.

| Fait | Source (URL) | Date de la source / accès |
|---|---|---|
| Mathlib contient **1,9 million de lignes** de code | [Growing Mathlib, arXiv:2508.21593v1](https://arxiv.org/html/2508.21593v1), §1 | article 29/08/2025, consulté le 22/11/2026 |
| Compilation de mathlib **sans cache** : « takes hours » | [lean4.dev — What is Mathlib?](https://lean4.dev/tactics/mathlib/intro) | consulté le 22/11/2026 |
| Machines de CI (productrices du cache) : « build all of mathlib in 30, 45 minutes » | [Zulip mathlib4, *Improve readme about cache*](https://leanprover-community.github.io/archive/stream/287929-mathlib4/topic/Improve.20readme.20about.20cache.html), message de Michael Rothgang | message 31/08/2025, page mise à jour 28/02/2026, consulté le 22/11/2026 |
| `lake build` sur mathlib non modifié **avec cache** : « just a few minutes » | même fil Zulip | idem |
| ~**14 %** du temps de compilation de mathlib est passé dans `simp`, **7 %** dans les autres tactiques | [arXiv:2508.21593v1](https://arxiv.org/html/2508.21593v1), §5.4 | 29/08/2025 |
| Un refactor de typeclasses : **−20 %** de temps d'inférence, **+6 %** de vitesse globale de compilation | [arXiv:2508.21593v1](https://arxiv.org/html/2508.21593v1), §5.1 | 29/08/2025 |
| Taille du noyau Thorn mesurée ici | `wc -l thorn/src/noyau.rose` → **1 586 lignes** ; total `thorn/src/*.rose` → **8 007 lignes** (dont `ordres.rose` 1 133 ajouté en parallèle) | 22/11/2026, MESURÉ ICI |
| Taille de la bibliothèque Thorn mesurée ici | 165 + 34 + 19 + 10 = **228 contrôles** dans les 4 suites ; 296 annoncés ailleurs dans le dépôt (à re-mesurer) | 22/11/2026, MESURÉ ICI pour les 4 suites |

La comparaison de taille est donc : **1 586 lignes de noyau Thorn** contre un
système (Lean 4) dont la bibliothèque seule fait **1,9 M lignes** (CITÉ). Ce
ne sont pas des objets de même nature : Thorn est un prototype de noyau, pas
une bibliothèque de mathématiques.

---

## 4. Ce que Thorn sait faire — et ce qu'il ne sait pas faire

### Compétitif (MESURÉ ICI)

- **Décision de typage sur des termes petits** : `identite.thn` vérifié en
  0,33 s avec 17 584 Ko de résident, contre 0,35 s et 468 828 Ko pour 4
  théorèmes Lean core. À périmètre égal (un énoncé), Thorn est dans le même
  ordre de grandeur en temps et un ordre et demi en mémoire.
- **Transparence du terme de preuve** : `--trace` montre le terme construit
  (`refl Nat 0`) sur les buts simples ; `ex09_induction.rose` montre
  `verifier` et `convertible` séparément. C'est un atout pédagogique réel.
- **Hiérarchie générique sans typeclasses dans le noyau** : `ex11_mathlib.rose`
  prouve que le même lemme sert à deux instances avec 6/6 contrôles, en
  gardant `verifier` + `convertible` comme seul juge.
- **Aucune dépendance externe, tout en Rose**, syntaxe française, zéro
  installation : `./rose/rose` suffit.

### Très loin (documenté, pas excusé)

1. **Expressivité du noyau** : univers unique `Type(0)`, pas de cumulativité,
   pas d'η, β+α seulement (pas d'ι), pas de contrôle de termination, pas de
   quotient, pas d'implicites au niveau du noyau, pas de classes. Les lois
   `plus_0_l`, `plus_s_l`, `nat_rec` sont des axiomes de signature.
2. **Synthèse d'instances** : `instances.rose` est un prototype de
   pré-élaboration — recherche linéaire dans un registre, pas de récursion,
   pas d'instances locales, pas de priorité configurable, pas de contraintes.
3. **Taille de bibliothèque** : 228 contrôles mesurés ici contre 1,9 M lignes
   (CITÉ). Aucune analyse, aucun nombre réel, aucune topologie, aucun
   algèbre abstraite vérifiée hors des 3 instances du registre.
4. **Performance sur les suites complètes** : `tests_algebre` ≈154 s,
   `tests_hierarchie` ≈135 s (médianes §3.3). Le prototype est rapide sur un
   terme, lent sur une suite entière — exactement l'inverse d'un moteur
   industriel (cache d'olean, incrémental, parallélisme de modules).
5. **Outil d'édition** : pas de mode interactif, pas de recherche de lemmes,
   pas de linter, pas de cache de compilation, pas de `--profile` par
   déclaration (Lean a tout cela).
6. **Bug de trace** : `--trace` casse sur les buts à plusieurs Π
   (`clé absente de la table`) — mesuré §1.3.

### Gestes techniques pour combler l'écart (par ordre de rendement)

1. **Corriger le chemin de trace** dans `thorn/src/principal.rose` : utiliser
   `mg_champ`/`cle_existe` avant chaque lecture de table dans la fonction de
   trace. Test de non-régression : les 5 `.thn` avec et sans `--trace`.
2. **Ajouter l'ι-réduction et les transparents** dans `thorn/src/noyau.rose` :
   réduire `elim` sur les constructeurs connus, puis faire de `plus_0_l` et
   `plus_s_l` des théorèmes dérivés au lieu d'axiomes. C'est le geste qui
   rendrait l'associativité comparable à `Nat.add_assoc`.
3. **Remplacer la recherche linéaire d'instance** par un index
   `(classe, porteur)` + résolution récursive avec profondeur et
   détection de cycles ; garder la pré-élaboration hors du noyau (c'est le
   bon design déjà en place).
4. **Mettre en cache les sous-termes dans `convertible`** (table de
   compaction + mémoïsation des paires déjà comparées) : c'est ce qui
   explique le passage de 154 s à quelques secondes quand le noyau est
   chaud ; le chiffre doit être mesuré, pas supposé.
5. **Niveaux d'univers et cumulativité**, puis implicit arguments dans
   l'analyseur — condition pour exprimer des structures génériques sans
   enregistrements Rose bruts.
6. **Incrémental** : sérialiser l'environnement vérifié d'un fichier
   (`hierarchie.rose`, `algebre.rose`) pour ne pas reconstruire 8 007 lignes
   à chaque lancement. C'est le pendant d'`lake exe cache get`.

---

## 5. Reproductibilité

```
cd /home/clausake/Stefan/ROSE
./rose/rose thorn/src/principal.rose thorn/preuves/associativite.thn
./rose/rose thorn/exemples/ex11_mathlib.rose
bash thorn/bench_thorn.sh 3
cat /tmp/opencode/thorn_bench_raw.txt

# Lean installé hors dépôt, uniquement si /dev/shm a assez de place :
export ELAN_HOME=/dev/shm/leanhome RUSTUP_HOME=/dev/shm/leanhome/rustup CARGO_HOME=/dev/shm/leanhome/cargo
export PATH=/dev/shm/leanhome/bin:$PATH
elan default stable
lean /dev/shm/leanhome/thorn_eq.lean
```

Les commandes `./rose/rose` du dépôt sont impératives : le `rose` du PATH est
une autre version. Aucune de ces commandes n'écrit dans `thorn/src/**`,
`rose/src/**` ou `compilateur/**`.
