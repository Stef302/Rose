# Thorn plus léger — diagnostic, correctif et mesures (22/11/2026)

Document de la mission « rendre Thorn plus léger ». **Aucun commit, aucune
publication.** Un seul fichier source modifié : `thorn/src/noyau.rose`.
Aucune modification de `rose/src/**`, `compilateur/**`, `scripts/**`,
`thorn/docs/BENCH_LEAN.md`, `thorn/docs/MATHLIB_ROADMAP.md`,
`thorn/exemples/**` ni des suites `thorn/tests_*.rose`.

---

## 1. Protocole de mesure

* Binaire **figé** pour l'avant/après : copie de `rose/rose` du 22/11/2026
  (md5 `7614a534b9411917ef9cd49a834014a8`), `Rose 0.1.73`, placée dans
  `/tmp/opencode/thorn_mem/rose_snapshot`. Le dépôt `rose/src/**` étant modifié
  concurremment par un autre agent, cette copie garantit que **seul le source
  Thorn change** entre les deux séries.
* Toutes les mesures passent par
  `scripts/prof_mem_rose.sh -n 3 -- <commande>` → garde-fou 16 Gio systématique,
  3 exécutions, médianes du temps réel et du pic RSS. `MemAvailable` ≥ 101 Gio
  au départ de chaque salve, swap libre ≥ 13,7 Gio, aucun `rc=75`.
* Le temps réel affiché par `prof_mem_rose.sh` inclut 0–5 s de polling du
  garde-fou : sur une charge de 5 s il n'est **pas** significatif ; sur
  `tests_algebre` (30–160 s) il l'est peu.

Commandes (toutes lancées depuis la racine du dépôt) :

```
scripts/prof_mem_rose.sh -n 3 -- /tmp/opencode/thorn_mem/rose_snapshot \
    thorn/src/principal.rose thorn/preuves/associativite.thn
scripts/prof_mem_rose.sh -n 3 -- /tmp/opencode/thorn_mem/rose_snapshot \
    thorn/tests_algebre.rose
scripts/prof_mem_rose.sh -n 3 -- /tmp/opencode/thorn_mem/rose_snapshot \
    thorn/tests_noyau.rose
```

---

## 2. Résultats avant / après (médianes de 3 runs, sous garde-fou 16 Gio)

| Charge | Avant temps | Avant pic RSS | Après temps | Après pic RSS | Gain RSS | Gain temps |
|---|---:|---:|---:|---:|---:|---:|
| `associativite.thn` (induction complète) | 5,01 s* | **557 440 Ko** (0,53 Gio) | 5,01 s* | **247 756 Ko** (0,24 Gio) | **2,25 ×** | — |
| `tests_algebre.rose` (34 lemmes) | 160,40 s | **5 785 200 Ko** (5,52 Gio) | 30,07 s | **5 486 604 Ko** (5,23 Gio) | 1,05 × | **5,33 ×** |
| `tests_noyau.rose` (165 contrôles) | 5,01 s* | 174 076 Ko (réf. `BENCH_LEAN.md`) / ~163 756–174 504 Ko | 5,01 s* | **153 272 Ko** (0,15 Gio) | ~1,1 × | — |

\* temps mural dominé par le polling du garde-fou (0–5 s) ; la mesure de
référence `BENCH_LEAN.md` donne 2,77 s pour `associativite.thn` et 0,25 s pour
`tests_noyau` **sans** garde-fou. Les rapports bruts sont dans
`/tmp/opencode/thorn_mem/{BASE_assoc,OPT_assoc,OPT2_assoc,OPT3_assoc,BASE_algebre,OPT_algebre,OPT_noyau}.txt`.

**Verdict honnête par rapport à l'objectif :**

* objectif ≥ 3× sur `associativite.thn` : **non atteint** — 2,25 × mesuré ;
* objectif ≥ 2× sur `tests_algebre` : **non atteint en RSS** (1,05 ×), mais
  **dépassé en temps** (5,33 ×). La mémoire des suites reste de l'ordre du
  Gio ; voir §4 pour la cause résiduelle, qui n'est pas corrigeable depuis
  `thorn/src/**` seul.

Étapes intermédiaires sur `associativite.thn` (médianes RSS) :
557 440 Ko (avant) → 250 184 Ko (registre partagé) → 246 912 Ko (+ retour
anticipé de `noy_iota`) → 247 756 Ko (+ test de variable libre sans liste,
dans le bruit). Le partage du registre explique à lui seul ~99 % du gain.

---

## 3. Cause identifiée (mesurée, pas supposée)

Le chemin chaud est :

```
verifier / convertible
  → noy_synth0 (App, ind, cumul)
    → noy_beta_lim
      → noy_redex_r  (récursif sur TOUS les nœuds)
        → noy_iota_r
          → noy_registre()
```

`noy_registre()` construisait **un registre neuf à chaque visite de nœud** :
3 inductifs, 6 constructeurs, ~40 tables/listes Rose (`Rc<RefCell<Vec<…>>>`).
Sur un terme de preuve de taille modeste, une seule normalisation fait
visiter des centaines de nœuds, plusieurs fois par pas de β ; une preuve
complète enchaîne des dizaines de normalisations. Le registre était donc
reconstruit des milliers de fois pour un contenu **identique** — duplication
pure, sans effet sémantique. L'ancien commentaire du source (« construit
localement à chaque appel ») documentait le symptôme, pas une contrainte :
Rose ne permet pas d'indexer une **globale** par une variable dans une
fonction (constat re-vérifié ici : `retour $g[k]` rend un type `?`), mais le
partage par **paramètre explicite** fonctionne parfaitement.

Deux causes secondaires confirmées comme marginales par mesure :
`noy_spine()` appelé sur des nœuds non-application (≈1 % du gain), et les
listes de variables libres reconstruites par `noy_subst` (dans le bruit).

---

## 4. Correctif retenu (un seul fichier : `thorn/src/noyau.rose`)

1. **Registre unique partagé sur tout le chemin chaud.** Les fonctions
   internes `noy_iota_r`, `noy_redex_r`, `noy_redex_liste_r`,
   `noy_beta_lim_r`, `noy_convertible_res_r`, `noy_cumul_res_r`,
   `noy_univers_res_r`, `noy_synth_r` et `noy_synth0_r` reçoivent un
   paramètre `reg: liste<table>`. `verifier_lim()` construit le registre
   **une fois** pour toute la synthèse ; `convertible()`, `beta()`,
   `cumulatif()` et `noy_univers()` en construisent un pour leur appel isolé.
   **Toutes les signatures publiques sont conservées** par des wrappers
   (`noy_beta_lim`, `noy_convertible_res`, `noy_cumul_res`,
   `noy_univers_res`, `noy_synth`) : les 165 contrôles de `tests_noyau`
   appellent encore les anciennes signatures, sans modification des tests.
2. **`noy_iota_r` retourne immédiatement** si le nœud n'est pas une
   application : ι ne réduit que `plus`, `not`, `nat_rec`, `bool_rec`,
   `list_rec` avec au moins un argument, donc `noy_spine()` était inutile
   sur la majorité des nœuds.
3. **`noy_libre_dans`** teste la présence d'une variable libre par
   parcours court-circuitant, sans construire de liste de chaînes ; le calcul
   de l'environnement d'évitement de capture (`noy_noms(t)`, `noy_libres(val)`)
   est déplacé dans la seule branche où une capture se produit réellement.

### Pourquoi `tests_algebre` reste à ~5,2 Gio

Une mesure isolée (hors garde-fou de comptage, sous garde-fou 16 Gio) montre
que **construire puis jeter** 1, 2, 3 fois le terme `al_lem_plus_assoc4()`
donne des pics RSS de 1 034 020, 1 747 976, 2 178 232 Ko : la mémoire est
**cumulative**, et elle est identique avec ou sans `verifier()` final
(5 appels : 2 522 248 Ko sans vérification contre 2 663 584 Ko avec).
Le coût résiduel est donc dans la **construction répétée des termes de
preuve** par les tactiques et dans la non-réutilisation de l'arena d'allocation
du runtime Rose entre ces constructions successives. Thorn ne dispose d'aucune
primitive de libération explicite, et le moteur Rose est hors périmètre de
cette mission : ce plafond ne peut pas être divisé par 2 depuis
`thorn/src/**` seul. Le gain réel, mesuré et conservé, est le **temps** :
160,40 s → 30,07 s (5,33 ×).

---

## 5. Ce qui est « normal » et ce qui ne l'est pas

**Normal (Lean).** 4 théorèmes Lean 4.36 core : 0,35 s / ≈468 828 Ko, mesuré
ici et rapporté dans `thorn/docs/BENCH_LEAN.md`. Ce n'est pas « 469 Mo pour
4 théorèmes » : c'est le coût fixe d'un runtime complet — environnement
chargé, parseur, élaborateur, gestionnaire de packages, métadonnées de
`Init` — présent dès le premier théorème et amorti ensuite sur des millions
de lignes de bibliothèque. Un process froid paie cette mise en place ; c'est
un prix d'entrée, pas une fuite.

**Normal (Thorn, après correctif).** 7 Mo pour un terme isolé, ~150–
250 Mo pour une preuve complète ou 165 contrôles : proportionné à la taille
des termes et à la profondeur des tactiques.

**Anormal (Thorn, avant correctif).** 5,7 Go pour 19 lemmes, 5,8 Go pour
34 lemmes, 532 Mo pour une seule induction : ce n'était pas le prix d'un
noyau de vérification de types, c'était ~40 tables hash reconstruites à
chaque nœud de chaque passe de β. Un même registre, immuable, était
dupliqué des milliers de fois. C'est ce gaspillage — et seulement lui — que
le correctif supprime. La comparaison honnête n'est donc pas « Lean 469 Mo
contre Thorn 5,7 Go », mais « coût fixe d'un compilateur industriel contre
coût fixe d'un prototype + duplication évitable » ; après correction,
l'écart de nature reste, l'écart de gaspillage a disparu sur le chemin de
vérification.

---

## 6. Non-régressions (toutes exécutées après le correctif)

```
$ bash thorn/tests_regression.sh
  cas exécutés : 17 / 17 attendus ; réussis 17 ; échoués 0
  compteurs observés : REUSSIS=165
  BILAN: OK — aucune régression

$ ./rose/rose verif thorn/src/noyau.rose          → vérifié
$ ./rose/rose verif thorn/src/{algebre,analyse,bibliotheque,hierarchie,
                              instances,lexeur,ordres,principal,tactiques}.rose
                                                  → vérifié (10/10)

thorn/tests_noyau.rose       → REUSSIS=165  ECHOUES=0  RESULTAT: OK
thorn/tests_algebre.rose     → 34 lemme(s) vérifié(s), 0 échec(s), RESULTAT: OK
thorn/tests_hierarchie.rose  → 19 contrôle(s) réussi(s), 0 échec(s), RESULTAT: OK
thorn/tests_instances.rose   → 10 contrôle(s) réussi(s), 0 échec(s), RESULTAT: OK
thorn/tests_ordres.rose      → 50 contrôle(s) réussi(s), 0 échec(s), RESULTAT: OK
$ /tmp/opencode/thorn_mem/rose_snapshot thorn/src/principal.rose \
    thorn/preuves/{identite,reflexivite,symetrie,commutativite,associativite}.thn
                                                  → OK, 1 théorème / 1 réussi chacun
```

Aucun résultat de preuve, aucun compteur et aucune sortie de test n'a changé.
