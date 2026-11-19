# Accélérer les jardins avant délocalisation de la stdlib

**Constat daté : 19/11/2026.** Dépôt `/home/clausake/Stefan/ROSE`, HEAD
`d20bed0`. Aucun `.rs`, `Cargo.toml`, `PLAN.md` ni document d'un autre agent
n'a été modifié. Binaire utilisé pour TOUTES les mesures : construit depuis
l'archive `git archive HEAD rose` dans `/tmp/opencode/ja_agents/`, copié en
`/tmp/opencode/ja_agents/rose_bin` (`stat -c%s` = 5 391 080 octets).
`rose/rose` n'a **pas** été remplacé. ⚠ Pendant la mission, un autre agent a
modifié `rose/src/jit.rs`, `rose/src/interp.rs` et deux fichiers de
`compilateur/` puis reconstruit `rose/rose` : son md5 diffère de celui de mon
binaire isolé. **Toutes les mesures de ce document viennent du binaire isolé
construit sur `d20bed0`** ; les validations finales ont été rejouées avec le
`rose/rose` courant (de l'autre agent) et passent aussi. Ne pas comparer les
chiffres de vitesse d'ici et ceux obtenus avec le JIT modifié sans re-mesure.

## 1. Méthode

- **Pinage** : `taskset -c 8` sur chaque lancement.
- **Horloge** : primitive Rose `temps()` (nanoseconds), lue autour de chaque
  bloc, convertie en ms à la diffusion. Aucun temps de démarrage de processus,
  aucun temps de compilation JIT dans les chiffres.
- **Échantillonnage** : **9 exécutions indépendantes** par banc, médianes
  rapportées, échantillons bruts conservés dans
  `/tmp/opencode/ja_agents/{base,after}_*.json`.
- **Contrôles** : à chaque banc les primitives natives sont mesurées dans le
  MÊME programme. Leur médiane varie de ≤ 0,7 % entre avant et après : c'est
  le bruit de fond, les gains annoncés sont bien au-delà.
- **Accumulateurs** : imprimés et comparés des deux côtés ; identiques
  (`125206381662596`, `358460`, `936033`, `16859953`, `680000`,
  `4200000`), donc les temps comparent le même travail.
- Outils de diagnostic : `ROSE_TIMING=1` (liste `TIMING-JIT eligibles=`),
  `ROSE_PROFIL_RIR=1` (`ops_executees=` = nombre d'opérations interprétées),
  `ROSE_PROFIL_JIT=1` (`appels=`, `temps_total`, `moyenne` par appel).

## 2. Reproduction de l'écart (avant optimisation, médianes de 9, ms)

| Banc (corpus) | Natif | Jardin | Rapport |
|---|---:|---:|---:|
| `factorielle`, 20 000 appels sur `i % 15` | 7,2757 | 9,7803 | × 1,34 |
| `compter_premiers`, 2 000 appels sur `i + 100` | 2,6598 | 19,3422 | × 7,27 |
| `pgcd`, 20 000 paires | 12,6379 | 13,0985 | × 1,04 |
| `fib`, 20 000 appels sur `i % 21` | 12,1586 | 9,0277 | **jardin × 1,35 plus rapide** |
| `longueur` (chaîne de 34 caractères), 20 000 appels | 3,8274 | 307,2447 | × 80,3 |
| `somme_liste` vs boucle en ligne, 20 000 appels sur 20 éléments | 71,2117 | 70,7165 | × 1,00 |

Balayage large (500 appels, médianes de 9) : `primorielle` natif 7,2927 ms
contre jardin 0,2452 ms (**jardin × 29,7 plus rapide**) ; `super_factorielle`
14,0569 contre 0,4370 (**× 32,2**) ; `terminal` 0,2144 contre 0,4333 (× 2,0
plus lent) ; `sous_factorielle` 0,2156 contre 0,3798 (× 1,8) ; `nb_chiffres`
0,1336 contre 0,1668 (× 1,25) ; `concatener` 0,2296 contre 1,2987 (× 5,7).

Conclusion : **le constat de `DELOCALISATION_40.md` est reproduit** pour
`factorielle` et `compter_premiers`, et il est incomplet : trois jardins
(`fib`, `primorielle`, `super_factorielle`) sont déjà plus rapides que le
natif, et `longueur_jardin` est le vrai goulet (× 80).

## 3. Décomposition par la preuve

### 3.1 Prémisse « paramètres non annotés → pas de JIT → lent » : **fausse en l'état**

Seules **5 fonctions sur ~95** n'ont pas de paramètre typé : `abs_jardin`,
`rang_numerique`, `min_jardin`, `max_jardin` (`math.rose`) et `html_widget`
(`gui.rose`). Elles sont dynamiques **par nature** (entier / rationnel /
flottant pour les quatre premières). `factorielle_jardin`,
`compter_premiers_jardin`, `pgcd_jardin`, `fib_jardin`, `longueur_jardin`,
`somme_liste` sont **déjà typées**. `ROSE_TIMING=1` confirme que les quatre
premières sont dans `TIMING-JIT eligibles` ; `longueur_jardin` ne l'est pas,
mais à cause de `sous_chaine` (chaîne), pas de son paramètre.

Mesures typé vs non typé, **même corps**, 9 échantillons c8 :

| Corps | Typé (JIT) | Non typé (RIR) | Interprétation |
|---|---:|---:|---|
| `carre(x) = x*x`, 20 000 appels | 8,7183 ms | **6,2289 ms** | le non typé est *plus rapide* : le trampoline JIT coûte 139,2 ns/appel (20 000 appels JIT mesurés) contre 20 000 ops RIR pour l'autre |
| `factorielle`, 10 000 appels | 4,8123 ms | **4,3501 ms** | même effet, ~10 % ; 174,1 ns/appel JIT, 282 568 ops RIR |
| `compter_premiers`, 2 000 appels | **9,5386 ms** | 116,7864 ms | ici le JIT gagne × 12,2 : le coût d'appel est amorti par le corps |

Donc : l'annotation de type **détermine bien l'éligibilité JIT**, mais
« JIT ⇒ plus rapide » n'est pas vrai universellement. Le JIT lourdement
avantageux sur un corps à boucle, et légèrement défavorable sur un corps
trivial (frais de franchissement de frontière ~140-175 ns). **À re-mesurer
après tout changement de `jit.rs` par l'autre agent** : ces trois nombres sont
des constats du binaire `d20bed0`, pas des lois.

### 3.2 Algorithme : le crible natif est la vraie différence sur `compter_premiers`

Variantes testées (2 000 appels sur `i + 100`, une salve, accumulateur
`358460` identique) :

| Variante | ms | Verdict |
|---|---:|---|
| division d'essai d'origine (impairs + helper) | 19,4 | référence |
| crible avec `table()` / `cle_existe` / `inserer` | 633,1 | **rejeté** : fonction non JIT-éligible (`ROSE_TIMING`), 33× pire |
| crible avec liste de premiers + `joindre` | 3933,4 | **rejeté** : non JIT-éligible, 200× pire |
| appel à la primitive native `est_premier` depuis le jardin | 493,2 | **rejeté** : appel générique, pas la spécialisation JIT du point d'appel |
| roue modulo 6 simple (`% 2`, `% 3`, pas 2/4) | 18,7 | gain marginal |
| roue 6 déroulée + boucle externe impaire | 12,5 | × 1,55 |
| **15 petits premiers déroulés + roue 6 + boucle impaire** | **9,5** | **retenu, × 2,03** |

Le gain ne vient pas d'une structure de données plus intelligente (toutes
les tentatives avec table/listes ont été catastrophiques parce qu'elles
sortent du sous-ensemble JIT), mais d'une **réduction du nombre de divisions**
et d'une **élimination de la boucle interne pour n < 2401**.

### 3.3 Construction de `Valeur` / frais d'appel : négligeables sur les corps épais

`somme_liste` (jardin typé) 70,5158 ms contre la même boucle écrite en ligne
71,1650 ms : **aucun surcoût mesurable** (ratio 0,99). Le coût de `Valeur`
est donc porté par le franchissement JIT/interprète (~140 ns), pas par
l'accumulation elle-même. C'est pour cela que `carre` typé paraît « plus lent »
alors que `compter_premiers` typé est 12× plus rapide.

### 3.4 Récursion

`fib` natif est récursif (12,1586 ms), `fib_jardin` est itératif (9,0277 ms) :
le jardin est **déjà × 1,35 plus rapide**. Une table déroulée de F(0..20) n'a
apporté que 3 % (8,9442 → 8,6544 ms) : rejetée. Aucun changement de la
récursion n'était nécessaire.

### 3.5 Indexation et découpe de chaîne

- `longueur_jardin` d'origine : une primitive `sous_chaine(s, i, i+1)` par
  caractère → 307,2447 ms pour 34 caractères × 20 000 appels.
- Dichotomie sur `sous_chaine(s, 0, m) == s` (-borne ramenée à la taille) :
  ~2 log₂ n découpes au lieu de n → **129,9682 ms, × 2,364**.
- `tableau` + affectation par index (`$a[$i] = 1`) dans une fonction :
  **inutilisable** — erreur d'exécution « Variable 'i' utilisée sans valeur »
  en voie RIR (l'abaissement ne capture pas l'index), et « Affectation par
  index sur une non-table » pour un `tableau` local. Ces tentatives ont
  simplement été abandonnées ; aucun `.rs` n'a été touché.

## 4. Optimisations retenues

1. **`jardins/math.rose` — `est_premier_basique`** : 15 petits premiers
   déroulés (2..47), puis roue 6k±1 déroulée (pas 4, puis 2) au-delà de
   2401 = 49². Correctitude vérifiée sur 0..3000 contre `compter_premiers`
   et sur 10 points jusqu'à 100 000 : **0 divergence**.
2. **`jardins/math.rose` — `compter_premiers_jardin`** : boucle externe sur
   les impairs seulement, 2 compté à part. Même domaine, même sortie.
3. **`jardins/chaines.rose` — `longueur_jardin`** : recherche exponentielle
   puis dichotomie ; cas vide traité à part (sinon la dichotomie rend 1).
   Correctitude vérifiée sur les longueurs 0..200 et 9 chaînes Unicode :
   **0 divergence**.

Optimisations **rejetées** (mesures ci-dessus) : table crible, crible par
liste de premiers, appel à `est_premier` natif, table déroulée de `fact` (×
1,09), table déroulée de `fib` (× 1,03), helper adaptatif pour n < 50
(2,3 %, dans le bruit), signature non typée sur les fonctions chaudes
(casse l'API typée et rend `compter_premiers` 12× plus lent).

**Régression assumée** : `primorielle_jardin` (n ≤ 19 dans son banc) passe de
0,2452 à 0,2528 ms (+ 3 %, la chaîne de `si` déroulés est plus longue que
l'ancienne boucle courte sur de très petits n). Le helper adaptatif testé ne
récupère que 2,3 %, dans le bruit. Ce jardin reste × 29 plus rapide que le
natif ; la régression n'est pas compensée par plus de complexité.

## 5. Résultats avant / après (9 échantillons pinés c8, médianes ms)

| Banc | Avant | Après | Gain |
|---|---:|---:|---:|
| `compter_premiers_jardin` | 19,3422 | 9,5571 | **× 2,024** |
| `longueur_jardin` | 307,2447 | 129,9682 | **× 2,364** |
| `factorielle_jardin` (contrôle) | 9,7803 | 9,7866 | × 1,00 |
| `pgcd_jardin` (contrôle) | 13,0985 | 13,0914 | × 1,00 |
| `fib_jardin` (contrôle) | 9,0277 | 9,0068 | × 1,00 |
| `somme_liste` (contrôle) | 70,7165 | 70,5158 | × 1,00 |
| Primitive `compter_premiers` (contrôle) | 2,6598 | 2,6648 | × 1,00 |
| Primitive `longueur` (contrôle) | 3,8274 | 3,8153 | × 1,00 |

Les deux objectifs « au moins × 2 sur 2 fonctions » sont atteints.
Contre-vérification en une seule salve (non médiane) avec le `rose/rose`
courant reconstruit par l'autre agent : `cp_jardin` 9,5559 ms,
`longueur_jardin` 130,289 ms — les gains se reproduisent. Les
jardins restent **plus lents que le natif** sur ces deux fonctions :
`compter_premiers` 9,5571 contre 2,6648 ms (× 3,6), `longueur` 129,9682
contre 3,8153 ms (× 34). Ce document ne prétend pas le contraire.

## 6. État des tests

Rejoués avec `./rose/rose` (binaire du dépôt, **non remplacé**) et avec
`/tmp/opencode/ja_agents/rose_bin` : sorties **octet pour octet identiques**
aux empreintes MD5 relevées avant modification (`/tmp/opencode/ja_agents/baseline/`).

```
./rose/rose verif jardins/math.rose           → vérifié
./rose/rose verif jardins/chaines.rose        → vérifié
bash scripts/rose-jardin.sh verif-tous        → ✓ Tous les jardins (7) sont valides.
jardins/tests/math.rose      → 35 primitives, 45 vérifications, 0 échec
jardins/tests/chaines.rose   → 5 fonctions testées, 0 échec
jardins/tests/listes.rose    → 19 tests fonctionnels, 0 échec
jardins/tests/premiers.rose  → 20 tests, 0 échec
jardins/tests/gui_demo.rose  → 3 widgets, 3 actions, 0 échec
/tmp/opencode/ja_agents/verif_finale.rose → VERIF_FINALE_ERREURS=0
```

## 7. Feuille de route de délocalisation

### 7.1 Garde-fous JIT — ne JAMAIS retirer ni court-circuiter

- `est_premier` / `is_prime` : détecteur de corps et spécialisation au point
  d'appel ; le jardin `premiers.rose` définit un `est_premier` Rose, ce qui ne
  l'autorise **pas** à remplacer la primitive.
- `joindre` (spécialisations liste), `inserer` (spécialisation d'instruction),
  famille `table` / `map` / `cle_existe` / `supprimer` / `cles` / `valeurs`.
- `somme` (nom spécialisé, `detecter_somme_nu`) : n'existe pas dans `STDLIB`,
  ne pas introduire de fonction de ce nom.
- Les 5 noms `abs`, `min`, `max`, `partie_entiere`, `fraction` restent natifs
  tant que le noyau gère les complexes (`modulus`), le dispatch dynamique et
  les dénominateurs > i64 (cf. `DELOCALISATION_40.md`, §2 et §6).

### 7.2 Primitives dont le jardin est désormais plus rapide — retrait envisageable

| Primitive | Jardin | Mesure (500 appels) | Condition |
|---|---|---|---|
| `fib` / `fibonacci` | `fib_jardin` itératif | 9,01 ms vs 12,16 ms (20 000 appels) | domaine `n ≥ 0` identique |
| `primorielle` | `primorielle_jardin` | 0,25 ms vs 7,44 ms | après re-mesure du léger ralentissement de +3 % |
| `super_factorielle` | `super_factorielle_jardin` | 0,44 ms vs 14,12 ms | domaine `n ≥ 0` identique |

### 7.3 Primitives correctes mais encore trop lentes — retrait NON recommandé

- `compter_premiers` : exact sur 0..3000 + points jusqu'à 100 000, mais
  × 3,6 plus lent que le crible natif. Ne retirer qu'après accord explicite
  sur le coût, ou après un changement JIT qui réduise le coût d'appel.
- `longueur` / `len` : le jardin ne couvre que les chaînes (le natif couvre
  liste, table, tampon) et reste × 34 plus lent. **Garder la primitive**.
- `concatener` (× 5,7 plus lent), `terminal` (× 2), `sous_factorielle` (× 1,8),
  `nb_chiffres` (× 1,25) : mêmes réserves.

### 7.4 Méthode obligatoire avant chaque retrait réel

1. rejouer les cinq fichiers de tests ci-dessus **sur le binaire reconstruit** ;
2. vérifier `stat -c%s rose/rose` avant/après dans la même session (aucun
   autre agent ne doit reconstruire entre les deux mesures) ;
3. ne retirer qu'UNE famille à la fois, avec re-mesure A/A/A sous `taskset -c 8`
   (9 échantillons, médianes, accumulateurs imprimés) ;
4. ne jamais supprimer de poids, cache HF ou dossier de modèle (règle GB10).

## 8. Fichiers livrés et reproductibilité

Modifiés : `jardins/math.rose`, `jardins/chaines.rose`, `jardins/README.md`,
ce document. Aucun `.rs`, `Cargo.toml`, `PLAN.md` ni doc d'un autre agent.

Bancs et données (hors dépôt, sous `/tmp/opencode/ja_agents/`) :
`rose_bin`, `bench_main.rose`, `bench_broad.rose`, `bench_cp_variants*.rose`,
`bench_variants4.rose`, `bench_variants5.rose`, `bench_len2.rose`,
`bench_typed.rose`, `bench_type_hot.rose`, `bench_type_cp.rose`,
`verif_finale.rose`, `run9.py`, `base_*.json`, `after_*.json`, `baseline/*.out`.

Commande de reproduction d'un banc :

```sh
cd /home/clausake/Stefan/ROSE
taskset -c 8 /tmp/opencode/ja_agents/rose_bin /tmp/opencode/ja_agents/bench_main.rose
python3 /tmp/opencode/ja_agents/run9.py /tmp/opencode/ja_agents/bench_main.rose 9
```
