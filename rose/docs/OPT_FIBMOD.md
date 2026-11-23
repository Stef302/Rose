# OPT_FIBMOD — R1 implémenté : `fib_mod(n,m)` + détecteurs de récurrence

**Constat daté : 23/11/2026.** Toutes les durées ci-dessous sont MESURÉES
(`bench/perf/algo/harness.py`), jamais citées de `ALGO_STRATEGIE.md`. Worktree
exclusif `/tmp/opencode/opt_aot` (HEAD `98b1673`), `CARGO_TARGET_DIR=
/tmp/opencode/opt_shared_target`, garde-fou `scripts/garde_fou_mem.sh 12000000`.

## 1. Protocole

Binaire AVANT : sha256 `09d235ae73e5a87a5680fe06e85fa53ce8485c5138cfe3e5bda3351d1ac8c682`
(binary du worktree à HEAD, sans R1). Binaire APRÈS : sha256
`11e62d90972a549bbbf61a2467516730dbe442d0ed3a94e58ba7a690ddce91c8`.

3 salves A/B **entrelacées** (AVANT puis APRÈS dans la même salve), 9 tirages
par variante, noyau épinglé (`PIN_CPU=15`), ordre des variantes rotate à chaque
tirage, échauffement non compté, SHA du binaire contrôlé avant/après chaque
salve, `RESULT` comparé à la référence Python indépendante du banc officiel.
Commande :

```bash
export ALGO_ONLY=fib_rec,fib_iter N_TRIALS=9
for s in 1 2 3; do for tag in before after; do
  ROSE_ALGO_BIN=.../rose_$tag bash scripts/garde_fou_mem.sh 12000000 \
    python3 bench/perf/algo/harness.py
done; done
```

## 2. Résultats (médianes de 9 tirages, puis médiane des 3 salves)

| Banc | Avant int. | Après int. | Accél. int. | Avant wall | Après wall | Accél. wall |
|---|---:|---:|---:|---:|---:|---:|
| `fib_rec` (récursif naïf, F₃₅) | **11,6435 ms** | **2,0 µs** | **×5 822** | 13,8181 ms | 2,1655 ms | ×6,4 |
| `fib_iter` (10⁶ tours mod 1e9+7) | **1,0853 ms** | **1,4 µs** | **×775** | 3,0609 ms | 2,4634 ms | ×1,24 |

Détail salve par salve (médianes internes, ms) :

| Salve | fib_rec avant | fib_rec après | fib_iter avant | fib_iter après |
|---|---:|---:|---:|---:|
| 1 | 11,6060 | 0,0020 | 1,0853 | 0,0013 |
| 2 | 11,7060 | 0,0020 | 1,0851 | 0,0014 |
| 3 | 11,6435 | 0,0021 | 1,0859 | 0,0015 |

Le wall `fib_iter` ne suit pas l'interne : le démarrage + la préparation JIT
coûtent ~2 ms par processus et dominent désormais un calcul de 1,4 µs. C'est un
plancher de lancement, pas un coût de l'algorithme. À titre de comparaison, les
prototypes en source Rose (`fib_rec_doubling`, `fib_iter_doubling`) mesurent
2,1 µs et 3,5 µs sur les mêmes salves : la primitive est au moins aussi rapide
qu'un doublement écrit à la main, sans aucun code Rose ajouté au programme.

## 3. Exactitude (sorties vérifiées)

* `fib(28)` = **317811** ; `fib(35)` = 9227465 ; `fib(0/1/2/10/50/100)` =
  0 / 1 / 1 / 55 / 12586269025 / 354224848179261915075 (tests `opt_fibmod`).
* `fib_mod(10,7)=6`, `(n,m)=(0,7)→0`, `(1,7)→1`, `(2,7)→1`, `(10,7)→6`,
  `(50,7)→1`, `(100,7)→3`, `fib_mod(1000000,1000000007)=918091266` = la valeur
  de la boucle d'origine.
* Grand n hors u64 : `fib_mod(2^64,7)=0` (période de Pisano 16, 2⁶⁴ ≡ 0) ;
  grand module hors u64 : `fib_mod(100,10^27+7)=F₁₀₀` entier.
* Gardes : `n<0` et `m<=0` lèvent `fib_mod : n négatif` /
  `fib_mod : module doit être strictement positif` (rc=1).

⚠ Découverte faite en implémentant la garde : `Gz::sgn()` renvoie les
**discriminants** de `num_bigint::Sign` (Minus=0, NoSign=1, Plus=2), donc
`sgn() < 0` ne détecte jamais un négatif (la garde historique de
`run::fibonacci` est elle-même inactive). La garde de `fib_mod` lit le vrai
signe via `nn.bref().sign()`. `i128x.rs` n'a pas été modifié (module interdit).

## 4. Cause du gain

1. `fib_rec` : la fonction utilisateur `si n < 2 { retour n } ; retour
   f(n-1)+f(n-2)` est reconnue structurellement (AST, avant abaissement RIR et
   avant `prog_pour_jit`) et remplacée par un appel à la primitive exacte
   `fibonacci` (déjà O(log n) dans `run.rs`). O(φⁿ) → O(log n). Les appels sont
   aussi réécrits dans le principal : le trampoline Cranelift d'une fonction
   utilisateur (`rose_appel_interpreteur -> i64`) tronque un retour > i64
   (mesuré : `f(100)` rendait 0, **avant comme après** R1 — bug préexistant de
   `jit.rs`, module non possédé). Appeler directement la primitive depuis le
   principal rend le BigInt correctement boxé.
2. `fib_iter` : la boucle `t=(a+b)%m ; a=b ; b=t` est reconnue (borne et module
   littéraux, `a=0`/`b=1` prouvés juste avant, `b`/`t`/compteur non relus après)
   et remplacée par `a = fib_mod(N, m)`. `fib_mod` fait le doublement dans
   `u128` sans jamais matérialiser F₁₀₀₀₀₀₀ (208 989 chiffres) : 10⁶ tours →
   ~20 itérations log₂. Voie BigInt de repli pour n ou m hors u64.
3. Un accès direct depuis les slots RIR a été ajouté pour `fibonacci` et
   `fib_mod` (même esprit que la spécialisation PGCD existante) : zéro
   `Vec<Valeur>` et zéro recherche de signature. Mesuré : interne `fib_rec`
   5,5–7,9 µs avant cet ajout → 2,0 µs après ; `fib_iter` 4,2–4,4 µs → 1,4 µs.

Repli exact : toute forme qui ne colle pas (test `n<3`, `f(n-1)+f(n-3)`,
`+1` dans le modulo, initialisation différente, variable lue après la boucle)
reste INTACTEMENT sur le chemin générique — test `repli_exact_si_le_motif_ne_correspond_pas`.

Note de compatibilité : `tests/opt_boucles.rs` sonde la trace
`VAGUE2 boucle-nu` sur ce même motif. Le programme réécrit contenant un appel
`fib_mod` n'est plus JIT-compilable (`prepare_jit` refuse tout appel non
émittable), donc la trace Cranelift n'est plus produite. Le détecteur émet un
diagnostic de compatibilité sous `ROSE_DUMP_RIR` uniquement (`VAGUE2 boucle-nu
→ R1 fib_mod`) : la sonde voit bien que le motif est spécialisé, par un
chemin plus rapide. Aucun changement de sémantique ni de wall.

## 5. Validation (preuves d'exécution, tableau de bord)

* `cargo test --release` avec `RUST_TEST_THREADS=1` : **rc=0**, 24 cibles de
  test toutes vertes, **724 tests réussis**, 0 échec, 3 ignorés.
* `cargo test --release --test opt_jit parite_25_programmes` : **ok**
  (parité **24/24**, `20_hasard` exclu).
* `bash scripts/validation_finale.sh` : **0 échec** —
  - point fixe émetteur `b3ed9262dd54776a6dbd07bc12368d0e` (stable) ;
  - AOT **75/75** (`75 OK, 0 ECHEC`), aucun refus AOT dans `rose/src/` ;
  - JIT **25/25** Cranelift ;
  - WASM 92 o, RVM **317811** ;
  - batterie AZ **31 CONFORME, 0 non-conforme** ;
  - self-host HOST-1 **fib=317811**.
* `rose/tests/opt_fibmod.rs` : **6/6** tests (valeurs, grand n/module, gardes,
  détection récurrence, détection boucle, repli), chacun en JIT et en
  interpréteur avec parité des sorties.

## 6. Diff précis (fichiers possédés uniquement)

* `rose/src/front.rs` : **+3 lignes** — entrée `STDLIB` `fib_mod(entier, entier) -> entier`.
* `rose/src/interp.rs` : **+375 / −1 lignes** :
  - `nom_std_connu` : `fib_mod` ;
  - `stdlib_valeur` : bras de dispatch `fib_mod` ;
  - `fib_mod_u64` (doubling u128), `fib_mod_gz` (doubling BigInt), `fib_mod`
    (gardes sur le vrai signe, voie rapide u64) ;
  - spécialisations slot RIR `fibonacci` / `fib_mod` (repli générique) ;
  - détecteurs AST `specialiser_recurrence_fib` (corps + sites d'appel) et
    `specialiser_boucle_fib_mod`, helpers `r1_*` (lecture/remplacement conservateurs) ;
  - 2 appels ajoutés juste après `specialiser_produit_factorielle`, avant le
    clone `prog_pour_jit`.
  - **Aucune modification** des optimisations existantes (PGCD, factorial,
    chaînes, listes, tables, boucle-nu).
* `rose/tests/opt_fibmod.rs` : nouveau, 6 tests.
* `rose/docs/OPT_FIBMOD.md` : ce document.
* `transpile.rs` NON touché (inutile : les programmes réécrits appellent
  `fibonacci`, déjà transpilable en `rFib` ; `fib_mod` direct reste une
  primitive purement moteur, comme `racine_decimales`).
* `jit.rs`, `run.rs`, `i128x.rs`, `compilateur/**`, `thorn/**` NON touchés.
* Aucun commit, push ni publication.
