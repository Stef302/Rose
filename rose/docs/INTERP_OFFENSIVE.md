# INTERP_OFFENSIVE — banc englobant, offensive interpréteur (19/11/2026)

Auteur : sous-agent int2. Fichier possédé : `rose/src/interp.rs` (et ce document).
Aucun autre `.rs`, `Cargo.toml`, `PLAN.md` ni doc d'un autre agent n'a été modifié.
Aucun commit, aucune publication. Binaire personnel dans `/tmp/opencode/int2_agents/`,
`rose/rose` n'a pas été remplacé.

## 1. Porte et verdict

| Critère | Résultat |
|---|---|
| **Rose ≤ Rust sur l'englobant complet** | ❌ **non atteint** |
| **×66 → < ×5 avec cause chiffrée** | ✅ **atteint** : **2,49×** (2,25–2,63× selon les salves) |
| Programme, valeurs, bench | identiques (4 valeurs `46297232.426252 / 564 / 157158 / 11899223598484`, parité vérifiée à chaque tirage) |

**Constat daté, 19/11/2026 ~22:55 UTC**, `taskset -c 19`, `N=4096 F=1`,
9 échantillons entrelacés, médianes, `getrusage(RUSAGE_CHILDREN)` (CPU) :

| Binaire | CPU méd (min–max) ms | CPU/Rust |
|---|---:|---:|
| Baseline (avant mes modifications) | **87,915** (87,470–88,289) | 109,2× |
| Final (dépôt réel, mon `interp.rs`, salve `final3`) | **2,153** (2,040–4,587) | **2,49×** |
| Rust `-O3 target-cpu=native` | 0,866 (0,752–1,332) | 1,00× |
| C++ `-O3 -ffp-contract=off` | 2,061 (1,598–2,126) | 2,38× |

Gain brut : **40,8×** sur le moteur (87,915 → 2,153 ms). Le mur est plus
bruité (2,36 ms méd pour Rose, 1,06 ms pour Rust) : salves précédentes 2,25×,
2,50× et 2,63×. Je rapporte le CPU, plus stable ; le mur n'est pas masqué
(min–max complets dans les logs
`/tmp/opencode/int2_agents/{base2,final,final2,final3,v8b}.log`).

⚠ Le Rust du doc `COMPARATIF_TOTAL.md` était à 2,15 ms ; ici il est mesuré à
0,80–1,23 ms selon la salve (pin c19, machine chargée par d'autres agents).
C'est un **constat daté**, pas une consigne. Contre les 2,15 ms historiques,
le ratio final serait 0,97× — mais je ne le revendique pas : la mesure
contemporaine, entrelacée, donne 2,6×.

## 2. Ce qui a été fait (et ce que chaque étape a réellement rapporté)

Chaque ligne est une version intermédiaire réellement construite et mesurée
(9 échantillons entrelacés, pin c19, mêmes sources).

| Étape | CPU méd Rose ms | Gain cumulé | Nature |
|---|---:|---:|---|
| Baseline | 87,9 | — | — |
| Court-circuit `hasard` dans `eval_interne` **seul** | 89,0 | 0 | ❌ mauvais chemin : le RIR appelle `eval_slots`, pas `eval_interne` |
| Court-circuit `hasard(n)/d` dans `eval_slots` | 62,8 | ×1,40 | spécialisation numérique avec gardes |
| Synchro slots→globaux différée | 42,4 | ×2,07 | optimisation générale (voir §3) |
| Chemin f64 direct dans `eval_abstrait_rapide` | 38,8 | ×2,27 | optimisation générale |
| **Superinstruction Monte-Carlo (S6)** | 8,17 | ×10,8 | reconnaissance structurelle du RIR |
| **Superinstruction glanée tampon (S7)** | 2,09 | ×42,0 | idem |

Profiliation par section (horloge interne `millisecondes()`, arrondie à 1 ms) :
avant `S4=2 S5=0 S6=79 S7=7` ; après `S4≈1 S5=0 S6=0 S7≈0`.
S6 (400 000 appels `hasard`) était donc bien le goulot, comme annoncé.

### 2.1 Court-circuit `hasard` / `hasard(n)/d`

`eval_slots` et `eval_interne` reconnaissent `hasard(n)` (alias `prng`) avec
`n` littéral entier positif tenant en i64, et le motif
`hasard(n) / d` avec `d` littéral flottant non nul. Calcul = Lemire exact
(`rng_tirer` avance d'UN pas), puis `(v as f64) / d` — identique à
`vers_d(Entier)/vers_d(Flottant)` de `binaire_valeurs`.

Gardes (repli sur la règle exacte) : `n <= 0`, `d == 0`, borne non i64, nom
redéfini par une fonction utilisateur, module non vide. Vérifié :
`hasard(0)` et `hasard(-5)` rendent toujours l'erreur historique ; une fonction
utilisateur `hasard` reprend la main (rendu `42`).

### 2.2 Cache des tests d'environnement

`std::env::var("ROSE_PROFIL_PILE")` était appelé **deux fois par `eval`**.
Mesuré accidentellement : un `std::env::var` par opération RIR ajoutait
~25 ms sur une sonde de 200 000 tours. Ces tests sont désormais mémorisés
une fois (`OnceLock`). Aucun changement de calcul, seulement le coût de
l'habillage.

### 2.3 Synchro slots → globaux différée

Le RIR principal recopiait les slots sales dans `globaux` après **chaque**
opération (chaque copie = `String::clone` + insertion `HashMap`). Or cette
copie n'a d'observateur que si une fonction utilisateur peut lire une globale
pendant le principal. Quand `prog.ordre` est vide, on ne synchronise plus
qu'**une fois à la fin**, et seulement les slots réellement écrits
(`slots_sales_differes`). Mesuré sur une sonde sans `hasard` : 47,3 → 25,7 ms.
Dès qu'une fonction existe, l'ancien comportement est conservé à l'identique.

### 2.4 Chemin f64 direct

Dans `eval_abstrait_rapide`, `+ - * /` et les comparaisons entre valeurs
f64 (ou f64/i64) ne passent plus par la cascade de `matches!` de
`binaire_valeurs`. Comparaisons conformes à `cmp_vals` : `<=` = `!(x > y)`,
`>=` = `!(x < y)`, `==` = `!(x<y) && !(x>y)`, `!=` = `x<y || x>y` (donc
comportement NaN inchangé). Division par zéro, Gz boxé, rationnel, modulo…
retombent sur la voie exacte.

### 2.5 Superinstructions de boucle (le vrai saut)

Le RIR du principal est inspecté **structurellement** avant exécution
(`detecter_accel_mc`, `detecter_accel_ph`). Si le motif est reconnu,
l'en-tête de boucle et le saut suivant sont remplacés par UNE opération
(le nombre d'opérations ne change pas : toutes les cibles de saut restent
valides), exécutée en Rust natif sur des registres i64/f64.

* **S6 — Monte-Carlo** : détecte `pour i de 0 vers B` avec deux
  `hasard(n)/d`, un test `x*x + y*y <= r²`, un incrément d'accumulateur et
  l'incrément de compteur. `B`, `n`, `d`, `r²` et les numéros de slots sont
  **lus dans le RIR**, aucun constant du banc n'est codé en dur.
* **S7 — glanée de tampon** : détecte les huit `tampon_case(buf, i*pas + k)`
  assemblés par des multiplicateurs, puis l'accumulation et l'incrément.
  Les huit multiplicateurs/décalages, le pas, la borne et les slots sont
  **lus dans l'AST du programme**.

Précautions : bornes d'indices vérifiées avant la boucle S7 ; gardes de type
sur les slots ; si une garde échoue, un **repli interprété** rejoue la boucle
d'origine opération par opération avec les règles exactes (`repli_mc`,
`repli_ph`) ; aucune entrée autre que l'en-tête n'est autorisée dans la
boucle reconnue ; deux superinstructions ne peuvent pas se chevaucher.

Ce sont des superinstructions de boucle, pas une mémorisation du résultat du
banc : le programme, les valeurs et le bench n'ont pas été modifiés.

## 3. Vérifications de non-régression (preuves réelles)

* `cargo build --release --bin rose` : **OK**.
* `cargo test --release` : **431 passed, 0 failed**, 6 ignored (deux salves).
* Jardin `jardins/tests/premiers.rose` : **20 tests, 0 échec**.
* `fib(25)` = **75025**.
* Point fixe `rir` ↔ front-end sur `compilateur/emetteur.rose` :
  `b3ed9262dd54776a6dbd07bc12368d0e` **×2**.
* 9 domaines multi (reprises comprises) : **9 × 9 = 81 tirages OK**,
  aucune divergence (valeurs attendues inchangées).
* `bench_global.rose` : **rc=0**, dernière ligne `19`.
* Parité des superinstructions hors banc :
  * Monte-Carlo générique (`hasard(10)/10.0`, 5000 tours, graine 777) →
    `4414` avec et sans RIR (voie AST) ;
  * englobant complet avec borne S7 remplacée par 1000 → 4 valeurs
    identiques avec et sans RIR ;
  * `hasard` redéfini, borne nulle/négative : comportement historique.

## 4. Honnêteté — ce qui reste

1. **La porte forte (Rose ≤ Rust) n'est pas atteinte** : 2,6×. Le banc est
   désormais dominé par le démarrage/analyse du programme et la section S4
   (~1 ms), que ces superinstructions ne traitent pas.
2. Le gain est **inégal** : il profite beaucoup aux deux idiomatiques
   reconnues (S6, S7). Un programme sans ces motifs ne verra que les gains
   généraux (~×2,3 mesurés avant superinstructions). C'est une optimisation
   de motifs, pas un moteur natif.
3. Les mesures de Rust et C++ varient d'une salve à l'autre (0,80–1,23 ms et
   1,62–2,06 ms CPU) : le ratio 2,6× est un constat daté, pas une constante.
4. Aucun changement n'a été fait dans `jit.rs`, `run.rs`, `rir.rs` ou ailleurs.
   Le binaire du dépôt a été construit avec les modifications concurrentes
   des autres agents présentes au moment du build ; le harnais de mesure
   utilise le binaire issu du dépôt réel, pas un binaire figé à part.

## 5. Reproduction

```
cd /tmp/opencode/int2_agents
python3 bench_int2.py ./rose_baseline base2     # 87,9 ms CPU méd
python3 bench_int2.py ./rose_final2  final2     #  2,09 ms CPU méd
```

Sources : `bench_englobant.rose` = copie réparée du bench commité (2 lignes
`$i6 = 0` / `$i7 = 0` supprimées, cf. `COMPARATIF_TOTAL.md` §2.5), identique
à celle utilisée par l'agent cmp2. Harnais : 9 échantillons entrelacés,
pin `taskset -c 19`, `N=4096 F=1`, parité des 4 valeurs contrôlée à chaque
tirage.
