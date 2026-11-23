# CORR3 — flottants de l'interpréteur RIR auto-hébergé

Date : 23/11/2026. Worktree exclusif `/tmp/opencode/corr_p3`, HEAD `d117e04`.
Fichiers possédés : `compilateur/interpreteur_rir.rose` (uniquement `vf`,
`val_str`, `eval_expr`), `compilateur/tests_corr3.rose` (nouveau), ce document.
Aucun commit, aucun push, aucun autre fichier du dépôt modifié.

## 1. Mesure avant correctif

Corpus : `compilateur/tests_corr3.rose`, 40 valeurs (t09, signes, décimaux,
seuils scientifiques, plus courts fidèles, rationnels, `approx`, replis
`flottant("…")`). Référence : `ROSE_SANS_JIT=1 rose/rose` sur le même fichier.

```bash
ROSE=/home/clausake/Stefan/ROSE/rose/rose            # binaire graine, non modifié
$ROSE rir compilateur/tests_corr3.rose > /tmp/opencode/corr3/corr3.rir
$ROSE /tmp/opencode/corr3/orig_interp.rose /tmp/opencode/corr3/corr3.rir > avant.txt
```

**11 divergences / 40** avec l'interpréteur d'origine (`git show HEAD:…`) :

| # | référence `rose/rose` | avant | cause |
|---|---|---|---|
| 4 | `-0.5` | `-1.5` | `val_str` routait `f:` vers `rat_dec` (arrondi négatif des rationnels) |
| 5 | `-1.25` | `-2.25` | idem |
| 6 | `-3.14159` | `-4.14159` | idem |
| 7 | `-1e-6` | `-1.000001` | idem + absence d'exposition |
| 34 | `1` | *(vide)* | `approx(0)` non implémenté dans `eval_expr` |
| 35 | `3.628800e6` | *(vide)* | idem |
| 36 | `9.332622e157` | *(vide)* | idem |
| 37 | `4.023873e2567` | *(vide)* | idem |
| 38 | `2.824229e456573` | *(vide)* | idem |
| 39 | `10000000000` | `Rose : erreur — conversion chaine → entier impossible` | `vf` ne savait pas lire `1e10` |
| 40 | `0.0015` | *(vide)* | `vf` ne savait pas lire `1.5e-3` |

Les autres exposants (`1e+16`, `1e+20`, `1e-5`, `1e-6`) étaient déjà
divergents dans une sonde plus large de 47 valeurs : l'interpréteur écrivait
`10000000000000000` là où le moteur Rust écrit `1e+16`.

À propos de `t09` : dans **cette** chaîne (dump RIR → `interpreteur_rir.rose`),
`1.5` s'affichait déjà `1.5` avant le correctif ; le constat
`rose/docs/HORDE_INTEGRATION.md:216` (« 1.5 s'affiche 1 ») n'est pas
reproductible ici, non plus que dans la chaîne composite
(`rose_natif.rose tests_selfhost/t09_flottant.rose` → ELF : `1.5 / 2 /
10000000000000000`). Le correctif couvre néanmoins le repli de constante
flottante (`vf`, `eval_expr` : tokens `flottant` et `(f …)`), qui est le
point cité par l'audit.

## 2. Correctif

### `vf` — représentation exacte

* Toute littérale décimale devient `f:<num>/<den>` **exact** (plus jamais
  de tag `i:` intermédiaire), via `rat_make` sur les chaînes décimales.
* Une littérale fractionnaire `a/b` reçoit le tag `r:<num>/<den>` : c'est un
  rationnel, affiché par `rat_dec` (périodicité, et quirk de signe Rust
  `-1/2 → -1.5`), alors qu'un flottant `-0.5` est affiché `-0.5`.
* Forme scientifique acceptée : `1e10`, `1.5e-3`, `-2e-4` sont repliés en
  rationnel exact par décalage de la virgule (puissance de 10), sans passer
  par `entier()` (qui échouait sur `1e10`).

### `val_str` — formatage décimal / exposition

Le tag `f:` a maintenant son propre formateur, calqué sur
`run::chaine_de_p` + `front::texte_flottant` / `format_g` du moteur Rust :

1. valeur entière et `|v| < 1e16` → décimal sans point (`2.0 → 2`) ;
2. sinon expansion décimale **exacte** du rationnel `num/den` (longue division
   sur chaînes, au plus 60 chiffres ; repli `rat_dec` si non terminée) ;
3. exponentiel `e = nb_chiffres_ip - zéros_de_tête - 1`, significatif
   `sig` après suppression des zéros de tête et de queue ;
4. notation scientifique `d.ddde±X` si `e < -4` ou `e >= len(sig)` —
   d'où `1e+16`, `1e+20`, `1e-5` — sinon décimal avec point
   (`0.0001`, `1.5`, `0.30000000000000004`).

Le signe est traité avant l'expansion, ce qui corrige `-0.5 → -0.5`
(au lieu de `-1.5`). Les rationnels `r:` conservent `rat_dec`.

### `eval_expr` — `approx` et identité flottante

* Branche `app approx` / `factorial_approx` : Stirling
  `ln(n!) = n·ln n − n + ½·ln(2πn) + 1/(12n) − 1/(360n³) + 1/(1260n⁵)`
  calculé en virgule fixe décimale sur 24 chiffres (constantes `ln2`, `ln10`,
  `ln2π` ; `ln` par série atanh après réduction dans `[1,2)`, `exp` par
  Taylor après réduction par `ln2`), puis rendu `{:.6}e{exposant}` **sans
  `+`**, exactement comme `factorial_approx` en Rust.
* `flottant(x)` sur une valeur déjà `f:` est l'identité : sans cette porte,
  `call_prim` repassait `vpay()` (`"3/2"`) dans `vf` et le transformait en
  rationnel.
* Les tokens `flottant`/`(f …)` continuent d'alimenter `vf`.

## 3. Preuves d'exécution

Toutes les commandes ont été exécutées dans le worktree, sous le garde-fou
`bash scripts/garde_fou_mem.sh 12000000 …` pour les opérations lourdes.

```text
rose verif compilateur/interpreteur_rir.rose      → vérifié
rose verif compilateur/tests_corr3.rose            → vérifié
40 cas : nat.txt (40 lignes) vs RIR (40 lignes)    → diff vide, 0 divergence
ELF C0  : rose compilateur/rose_natif.rose \
          compilateur/interpreteur_rir.rose        → rc=0, 230 776 o, magic ELF
ELF sur corr3.rir vs nat.txt                       → diff vide, 0 divergence
point fixe émetteur : md5(rir(emetteur.rose))
  = md5(emetteur.rose(emetteur.rose))
  = b3ed9262dd54776a6dbd07bc12368d0e               → OK
rose compilateur/tests_interpreteur_rir.rose       → TESTS RIR : 15 OK, 0 ECHEC
cargo test --release --bin rose                    → 327 passed; 0 failed
scripts/validation_finale.sh (copie avec B=graine) → RESULTAT: 0 échec(s)
```

Détail du `validation_finale` rejoué : Rust build + tests `rc=0` ; point fixe
OK ; AOT `75 OK, 0 ECHEC` ; JIT `25/25` ; WASM `92 o` ; RVM `317811` ;
batterie AZ `31 CONFORME, 0 non-conforme` ; HOST-1 `fib=317811`.

Un premier passage du `validation_finale` avait montré `tests rc=101` sur
`link_natif::tests::elf_avec_valeur_exacte_ecrit_le_texte_memorise`
(`Text file busy`, `_os error 26`) et 31 BLOQUES AZ : le binaire
`rose/rose` n'existe pas dans un worktree neuf. Le test Rust a été rejoué
seul (`327 passed, 0 failed`) et la batterie AZ avec le chemin absolu de la
graine : `0 échec`. Ces deux incidents étaient environnementaux, pas des
régressions du correctif.

## 4. État final

* 3 fonctions modifiées dans `compilateur/interpreteur_rir.rose`, 1 fichier
  de test ajouté, 1 document ajouté.
* 40/40 flottants conformes au moteur Rust, en interprétation Rose et en ELF
  C0, avec `approx`, `1e10`, `-0.5` et `t09` dans le corpus.
* Aucun commit, aucun push, aucun autre fichier touché (`git status` : `M
  compilateur/interpreteur_rir.rose`, `?? compilateur/tests_corr3.rose`,
  `?? rose/docs/CORR3_FLOTTANTS.md`).
