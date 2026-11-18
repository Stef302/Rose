# Délocalisation de la bibliothèque standard vers des jardins Rose

**Constat daté : 19/11/2026.** Dépôt `/home/clausake/Stefan/ROSE`, HEAD poussé
`477736f` au début de la mission. Aucun fichier `.rs`, `Cargo.toml`, `PLAN.md`
ni document d'un autre agent n'a été modifié. Toutes les mesures ci-dessous ont
été prises avec `taskset -c 10`.

## 1. Résultat chiffré — 40 % NON atteint

La table `STDLIB` du module `front` contient **112 entrées** (une entrée = un
nom de primitive, alias compris). Classification :

| Catégorie | Entrées | Part |
|---|---:|---:|
| **A — délocalisées** dans `math.rose` / `chaines.rose`, équivalence mesurée | **40** | **35,7 %** |
| B — exclues par garde-fou JIT (`est_premier`, `is_prime`, `inserer`, `joindre`, famille `table`) | 10 | 8,9 % |
| C — non délocalisables en Rose pur (E/S, horloge, hasard, tampon/GPU, complexe, irrationnels, types, `sous_chaine`) | 62 | 55,4 % |

**Le taux réel est 40 / 112 = 35,7 %**, pas 40 %. En dédupliquant les alias
(112 entrées → **80 familles sémantiques**), 24 familles touchent à un jardin,
soit **24 / 80 = 30,0 %**. Les deux chiffres sont donnés pour éviter toute
inflation : compter les alias gonfle le pourcentage, les dédupliquer le réduit.

Les fonctions du catalogue ANNONCÉES dans la consigne mais **absentes de
`STDLIB`** (`separer`, `majuscule`, `miniscule`, `inverser`, `tri`, `filtrer`,
`plier`, `aplatir`, `somme`) ne pouvaient donc pas être comptées. `separer`
apparaît seulement dans la liste blanche `nom_std_connu` du module `interp`,
sans entrée `STDLIB` et sans implémentation de primitive. Les utilitaires
équivalents écrits dans `listes.rose` sont **hors décompte** (§5).

## 2. Les 40 fonctions délocalisées et leurs tests

Toutes les fonctions de jardin portent le suffixe `_jardin` : aucun jardin ne
réutilise un nom sensible au JIT (`est_premier`, `somme`, `joindre`, `inserer`,
tables) et les tests distinguent sans ambiguïté `f(x)` (Rust) de
`f_jardin(x)` (Rose).

| Jardin | Primitives STDLIB couvertes | Test |
|---|---|---|
| `jardins/math.rose` | `factorielle`, `fact`, `fibonacci`, `fib`, `pgcd`, `gcd`, `ppcm`, `lcm`, `puissance`, `pow`, `compter_premiers`, `nb_chiffres`, `digits`, `factorielle_rapide`, `factorial_rapide`, `rapide`, `primorielle`, `termial`, `triangulaire`, `termial_classique`, `terminal`, `termial_kurepa`, `kurepa`, `super_factorielle`, `multifactorielle`, `multifactoriel`, `sous_factorielle`, `derangement`, `sous_factorial`, `abs`, `min`, `max`, `partie_entiere`, `fraction`, `duree` (35) | `jardins/tests/math.rose` |
| `jardins/chaines.rose` | `longueur`, `len`, `ordre`, `caractere`, `concatener` (5) | `jardins/tests/chaines.rose` |

Corpus : **25 cas** par fonction unaire (0..24), **24 paires** pour les
fonctions binaires, **20 cas** pour les exposants négatifs, **25 entiers + 20
flottants + 20 rationnels** pour `abs`/`min`/`max`/`partie_entiere`, **22
chaînes** (dont Unicode) pour `longueur`/`len`, **20 caractères** pour `ordre`
et `caractere`, **20 listes** pour `concatener`. Chaque test compare `chaine()`
**et**, pour `min`/`max`/`puissance`, `type_de()` : la promotion de type fait
partie de l'équivalence.

Sortie réelle (19/11/2026, binaire reconstruit pendant la mission par un autre
agent — voir §4) :

```
── jardin/math : 35 primitives, 45 vérifications, 0 échec ──
── jardin/chaines : 5 fonctions testées, 0 échec ──
── jardin/listes : 19 tests fonctionnels, 0 échec ──
── jardin/premiers : 20 tests, 0 échec ──          ← jardin 20/20 intact
✓ Tous les jardins (7) sont valides.
```

Validation complète rejouée :

```sh
cd /home/clausake/Stefan/ROSE
taskset -c 10 ./rose/rose verif jardins/math.rose
taskset -c 10 ./rose/rose verif jardins/chaines.rose
taskset -c 10 ./rose/rose verif jardins/listes.rose
bash scripts/rose-jardin.sh verif math
bash scripts/rose-jardin.sh verif chaines
bash scripts/rose-jardin.sh verif listes
bash scripts/rose-jardin.sh verif-tous
taskset -c 10 ./rose/rose jardins/tests/math.rose
taskset -c 10 ./rose/rose jardins/tests/chaines.rose
taskset -c 10 ./rose/rose jardins/tests/listes.rose
taskset -c 10 ./rose/rose jardins/tests/premiers.rose
```

### Domaines restreints (à ne pas silence)

- `ordre_jardin` / `caractere_jardin` : **ASCII imprimable 32..126** seulement.
  Le natif couvre 0..1114111. Ils ne peuvent PAS remplacer le natif en l'état.
- `puissance_jardin` aux exposants négatifs : dénominateur doit tenir en i64.
  Mesure : `1 / puissance_pos_jardin(16,16)` lève « division par zéro » alors
  que `puissance(16,-16)` rend `1/18446744073709551616`. L'opérateur `/` de
  Rose ne construit pas de rationnel à grand dénominateur (GMP côté natif).
- `fraction_jardin` : même limite sur le dénominateur après canonicalisation.
- `partie_entiere` : trois variantes typées (`partie_entiere_entier_jardin`,
  `..._flottant_...`, `..._rationnel_...`) car le vérificateur refuse
  `entier(x)` sur un paramètre dynamique ; un appelant dynamique ne peut pas
  être distribué automatiquement en Rose 0.1.5.
- `duree_jardin` : `ns >= 0`, comme le natif.
- Fonctions de suites : domaine `n >= 0` (le natif lève une erreur sur les
  négatifs ; un jardin Rose ne sait pas lever d'erreur).

## 3. Effet « léger et rapide » — mesures réelles

### Taille du binaire : AUCUN effet attribuable à cette mission

`stat -c%s rose/rose` : **5 128 936 octets** en début de mission, **5 194 472
octets** en fin de mission. L'écart de +65 536 octets ne vient **PAS** des
jardins : pendant la mission, un autre agent a modifié `rose/src/rosevm.rs`,
`rose/src/wasm_lecteur.rs`, ajouté `rose/src/transpile.rs` et reconstruit le
binaire (mtime 19/11 00:33:40). Cette mission n'a écrit **aucun** `.rs`.

Formulation exacte : **la bibliothèque est externalisée et prête à être
retirée du noyau dans une étape ultérieure** ; le binaire ne maigrit pas tant
que le code Rust n'est pas supprimé. Toute affirmation de gain de taille ici
serait fausse.

### Temps d'exécution (médiane de 5 séries, `taskset -c 10`, ms)

| Chargement | Natif | Jardin | Rapport |
|---|---:|---:|---:|
| `factorielle`, 20 000 appels sur `i % 15` | 13 ms | 18 ms | **× 1,4** |
| `compter_premiers`, 2 000 appels sur `i + 100` | 5 ms | 46 ms | **× 9,2** |

Séries brutes `factorielle` : natif 12/13/13/13/13, jardin 18/18/18/17/18.
Séries brutes `compter_premiers` : natif 4/5/4/5/5, jardin 46/46/46/46/45.
Accumulateurs imprimés identiques des deux côtés (`125206381662596`,
`358460`) : les temps comparent bien le même travail.

Conclusion honnête : un jardin Rose n'est **pas** « plus rapide » qu'une
primitive Rust compilée. Sur une boucle simple le JIT réduit l'écart à ~1,4× ;
sur un algorithme avec test de primalité répété l'écart reste ~9× (crible natif
vs division trial en Rose). Le gain visé est la **réduction de la surface du
noyau**, pas un gain de vitesse d'exécution.

## 4. Anomalies JIT rencontrées — constats datés, NON corrigées

Aucune n'a été corrigée dans Rust (interdiction de périmètre). Les jardins
contiennent des contournements documentés, vérifiés sur corpus.

1. **Somme de deux grands entiers boxés en boucle** : une accumulation du type
   `$f = $f * $k; $s = $s + $f` diverge du natif dès `k = 22`.
   Mesuré : `acc(22) = 2360699865296135453816`, `terminal(22) =
   1177652997443428940314`. `sous_factorielle` divergeait de la même façon.
   Contournement : recalculer `factorielle_jardin(k)` à chaque pas et n'ajouter
   que `±1` dans le dérangement. Après correction : 0 divergence sur 0..40.
2. ** motif `n * (n + 1) div 2` appelé via un alias clobbre un global** :
   avec `fn base(n) { retour n * (n + 1) div 2 }` et `fn alias(n) { retour
   base(n) }`, un global `$e = 0` passe à 1, 2, 3, 4 après quatre appels.
   Contournement : décomposer en `$a = n + 1; $b = n * $a; $c = $b div 2`.
3. **`abs` natif sur rationnel rend un flottant** : `abs(-9/2) = 4.5` de type
   `flottant` (branche `modulus | abs` atteinte avant la branche rationnelle
   dédiée). Le jardin multiplie par `1.0` pour reproduire ce type.
4. **Grand dénominateur rationnel** : voir §2 (limite i64 de `/`).
   `rationnel(1, 18446744073709551616)` fait même panic le front-end
   (`Option::unwrap()`) : ne pas appeler cette forme.

Ces faits sont des **constats du 19/11/2026 sur le binaire reconstruit à 00:33
par un autre agent** ; à re-mesurer après toute modification du JIT.

## 5. `listes.rose` — hors décompte

`jardins/listes.rose` contient 11 utilitaires (`joindre_listes`,
`inverser_liste`, `somme_liste`, `produire_liste`, `maximum_liste`,
`minimum_liste`, `compter_liste`, `filtrer_pairs`, `filtrer_positifs`,
`tri_insertion`, `aplatir3`). **Aucun n'est une primitive `STDLIB`** : ils ne
comptent pas dans le 35,7 %. Ils sont testés sur 19 cas fonctionnels
(`jardins/tests/listes.rose`). Les noms `somme` et `joindre` (spécialisés par
le JIT) sont volontairement évités : `somme_liste`, `joindre_listes`.

## 6. Feuille de route de retrait (À FAIRE, pas fait)

1. **Garde-fous JIT — ne JAMAIS retirer ni court-circuiter** :
   - `est_premier` / `is_prime` : détecteur de corps (`detecter_corps_premier`)
     et spécialisation au point d'appel dans le module `jit` ; le jardin
     `premiers.rose` définit déjà un `est_premier` Rose, mais ne PAS l'utiliser
     pour remplacer la primitive.
   - `joindre` (spécialisations liste dans `jit`), `inserer` (spécialisation
     d'instruction), `table`/`map`/`cle_existe`/`supprimer`/`cles`/`valeurs` :
     famille table entière.
   - `somme` (nom spécialisé, `detecter_somme_nu`) : n'existe pas dans
     `STDLIB`, ne pas introduire de fonction de ce nom.
2. **Retrait possible sans changement de sémantique** (après re-pointage des
   noms et maintien d'alias de compatibilité) : les 26 primitives suivantes,
   dont le jardin couvre tout le domaine où la primitive est définie —
   `factorielle`, `fact`, `fibonacci`, `fib`, `pgcd`, `gcd`, `ppcm`, `lcm`,
   `factorielle_rapide`, `factorial_rapide`, `rapide`, `primorielle`, `termial`,
   `triangulaire`, `termial_classique`, `terminal`, `termial_kurepa`, `kurepa`,
   `super_factorielle`, `multifactorielle`, `multifactoriel`, `sous_factorielle`,
   `derangement`, `sous_factorial`, `duree`, `concatener`.
   ⚠ `compter_premiers` est correct mais ~9× plus lent : ne le retirer
   qu'après accord explicite sur le coût.
3. **Retrait conditionnel** (garde native obligatoire) : `longueur`/`len` (le
   natif marche aussi sur liste/table/tampon, le jardin seulement sur chaîne) ;
   `ordre`/`caractere` (ASCII seulement) ; `puissance` et `fraction`
   (dénominateurs > i64) ; `abs`/`min`/`max` (le natif gère aussi les complexes via
   `modulus`) ; `partie_entiere` (pas de dispatch dynamique en Rose 0.1.5).
4. **Méthode obligatoire avant chaque retrait** :
   - rejouer les quatre fichiers de tests ci-dessus sur le binaire reconstruit ;
   - vérifier `stat -c%s rose/rose` AVANT/APRÈS dans la MÊME session (aucune
     autre agent ne doit reconstruire entre les deux mesures) ;
   - ne retirer qu'UNE famille à la fois, avec re-mesure A/A/A sous
     `taskset -c 10` ;
   - ne jamais supprimer de poids, cache HF ou dossier de modèle (règle GB10).

## 7. Fichiers livrés

- `jardins/math.rose` (40 `fn`), `jardins/chaines.rose` (6 `fn`),
  `jardins/listes.rose` (11 `fn`) ;
- `jardins/tests/math.rose`, `jardins/tests/chaines.rose`,
  `jardins/tests/listes.rose` ;
- `jardins/README.md` (catalogue mis à jour) ;
- ce document. Aucun autre fichier n'a été écrit.
