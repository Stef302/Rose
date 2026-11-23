# CORR6 — formes RIR non couvertes par l'interpréteur

**Date des mesures** : 23/11/2026.
**Périmètre possédé** : `compilateur/interpreteur_rir.rose` (**uniquement**
`call_prim` et `eval_expr`), `compilateur/tests_corr6.rose` (nouveau),
`rose/docs/CORR6_RIR.md` (ce document).
**`compilateur/emetteur.rose` n'a PAS été modifié** (md5 inchangé
`390335d29fb9cee368b3456dd892e377`) : le point fixe `b3ed9262` est donc
préservé mécaniquement, et reverifié en §5.
Aucun commit, push ou publication. Aucun autre fichier du dépôt touché.

---

## 1. Mesure — ce qui manquait exactement

Point de départ : `t104_surcharge_arith`, `t105_surcharge_arite`,
`t106_hof_param`, `t107_hof_var` sont **refusés par les deux moteurs**, donc
aucun dump `rose rir` n'existe pour eux :

```
$ R=rose/target/release/rose
$ $R tests_selfhost/t104_surcharge_arith.rose
Rose : erreur — [Ligne 2, Colonne 0] Fonction 'f' définie deux fois     (rc=1)
$ $R rir tests_selfhost/t104_surcharge_arith.rose
Rose : erreur — [Ligne 2, Colonne 0] Fonction 'f' définie deux fois     (rc=1, 0 octet)
$ $R tests_selfhost/t106_hof_param.rose
Rose : erreur — [Ligne 1, Colonne 0] Fonction 'fn' inconnue              (rc=1)
$ $R tests_selfhost/t107_hof_var.rose
Rose : erreur — [Ligne 2, Colonne 0] Variable 'carre' utilisée sans valeur (rc=1)
```

J3 (`rose/docs/J3_RIR_ROSE.md`) couvrait 157/158 sur l'oracle sans JIT. La
cause n'est **pas** l'appel statique : `(appel_rir carre (v y))` est déjà
exécuté correctement (mesuré : `double(3) = 18`). Les trois formes réellement
absentes de `eval_expr`/`call_prim` sont :

| Forme RIR manquante | Ce qu'elle exprime | Pourquoi l'interpréteur échouait |
|---|---|---|
| `(fn carre)` / valeur `n:carre` | une fonction comme valeur de première classe (paramètre-fonction, variable) | aucun tag de fonction : `vtag`/`val_type` n'ont que `i r f s b l t g v` ; `(fn …)` tombait sur le `vi("0")` final |
| `(appel_rir (v fn) args…)` | appel à un **calleur dynamique** (la fonction est dans un slot/variable) | la branche `app/appel_rir/appel` lisait `$tok[p+2]` comme un **nom** : ici c'est `"("`, donc `find_fn("(")` échoue et `call_prim("(")` rend `v:` |
| `(surcharge f args…)` | dispatch de surcharge par **arité** et par **type** | `find_fn` ne rend que la première entrée d'un nom ; il n'existe aucun mécanisme de choix d'overload |

Mesure avant/après sur 14 dumps construits pour ces formes (§3) : l'ELF
**avant** correction rendait des suites de `0` et les arguments nus
(ex. `t106` → `|5|0|` au lieu de `25`) ; l'ELF **après** rend la valeur
attendue. Aucun `rc` d'erreur dans les deux cas — c'est une divergence
sémantique silencieuse, pas un plantage.

## 2. Correction apportée

Uniquement dans les deux fonctions possédées de
`compilateur/interpreteur_rir.rose` :

**`call_prim`** — nouveau tag de valeur `n:<nom>` pour les primitives
`fn`/`fonction` (le tag `n` est libre ; `f` est déjà le flottant). Deux
écritures acceptées : `(fn carre)` et `(app fn (s "carre"))`.

**`eval_expr`** —
1. littéral `(fn <nom>)` → `"n:<nom>"` ;
2. `surcharge` / `appel_surcharge` : évalue les arguments, calcule
   l'arity et le code de type de chaque valeur (`e` entier, `c` chaine,
   `b` booleen, `l` liste, `r` rationnel, `f` flottant, `t` table,
   `v` vide, `n` fonction), puis essaie dans l'ordre
   `<base>@<arité>@<codes>`, `<base>@<arité>`, `<base>` ;
3. branche `app`/`appel_rir`/`appel` : si le token après l'en-tête est
   `"("`, l'expression est évaluée, sa valeur `n:<nom>` est décodée et le
   nom réel est utilisé pour `call_fn`. Le chemin statique est inchangé.

**Aucune autre fonction de l'interpréteur n'a été touchée** (`parse_rir`,
`find_fn`, `call_fn`, `exec_fn`, le point d'entrée sont bit à bit identiques à
`HEAD`). Les dumps existants ne contiennent aucun de ces nouveaux heads :
leur sortie est donc byte-identique (§4).

## 3. Jeux d'essai — `compilateur/tests_corr6.rose`

14 dumps RIR sont intégrés au banc (4 refus historiques + 10 programmes
similaires) :

| Dump | Forme couverte | Sortie attendue |
|---|---|---|
| `t104_surcharge_arith` | surcharge par type, arité 1 | `3`, `a!` |
| `t105_surcharge_arite` | surcharge par arité 1/2 | `3`, `5` |
| `t106_hof_param` | fonction passée en paramètre | `25` |
| `t107_hof_var` | fonction stockée dans une variable | `25` |
| `c01_surcharge_3types` | 3 types (e/c/b) | `17`, `x?`, `b` |
| `c02_surcharge_arite3` | arités 1/2/3 | `2`, `3`, `6` |
| `c03_hof_double` | paramètre-fonction appelé deux fois | `61` |
| `c04_hof_chaine` | paramètre-fonction + primitive `longueur` | `6` |
| `c05_hof_recursive` | récursion via `(fn fib)` | `55` |
| `c06_fn_direct` | calleur dynamique littéral | `49` |
| `c07_fn_globale2` | deux globales fonction | `9`, `8` |
| `c08_surcharge_liste` | dispatch sur type `liste` | `3`, `42` |
| `c09_surcharge_mixte` | arité **et** type | `105`, `12`, `-x5` |
| `c10_hof_compose` | composition `g(f(x))` | `17` |

`tests_corr6.rose` écrit ces 14 dumps + `ATTENDUS.tsv` dans le dossier passé
en argument (par défaut `/tmp/opencode/corr6_fixtures`) et vérifie 13
assertions pures (encode/decode `n:`, codes de types, noms manglés, reliture
d'un dump).

## 4. Preuves d'exécution

Toutes les commandes ont été exécutées dans
`/tmp/opencode/corr_p6`, avec
`R=rose/target/release/rose`, `garde_fou_mem.sh 12000000` sur les
compilations, et `C0=/tmp/opencode/j1_C0_from_C0ext.elf` (ELF aarch64 de
compilation, md5 non modifié par ce chantier).

### 4.1 Vérification statique

```
$ $R verif compilateur/interpreteur_rir.rose            -> vérifié (rc=0)
$ $R verif compilateur/tests_corr6.rose                 -> vérifié (rc=0)
$ ROSE_SANS_JIT=1 $R compilateur/tests_corr6.rose <dir>
    -> TESTS CORR6 : 13 OK, 0 ECHEC                      (rc=0)
$ ls <dir>/*.rir | wc -l                                 -> 14
```

### 4.2 C0 → ELF

```
$ bash scripts/garde_fou_mem.sh 12000000 timeout 900 \
    $C0 compilateur/interpreteur_rir.rose > /tmp/opencode/corr6_interp.elf
    rc=0, 215 144 octets, magic 7f454c4602010100, stderr 0 octet
```
(ELF **avant** correction : 205 084 octets — la différence de 10 060 octets
est précisément le code ajouté.)

### 4.3 Les 14 formes nouvelles, avant / après

Même ELF C0, mêmes dumps, seule la source de l'interpréteur change :

```
$ while IFS=$'\t' read -r name expected; do ... done < ATTENDUS.tsv
OK=14 ECHEC=0
```

Extrait de la comparaison before/after (sortie brute) :

| Dump | AVANT | APRÈS | Attendu |
|---|---|---|---|
| `t104_surcharge_arith` | `0,0,0,2,0,0,0,0,a,0,0,0` | `3`, `a!` | `3`, `a!` |
| `t105_surcharge_arite` | `0,0,0,2,0,0,0,0,2,3,0,0,0,0` | `3`, `5` | `3`, `5` |
| `t106_hof_param` | ``, `5`, `0` | `25` | `25` |
| `t107_hof_var` | ``, `5`, `0` | `25` | `25` |
| `c10_hof_compose` | ``, `0`, `0`, `0`, `0`, `4`, `0` | `17` | `17` |

Les 14 lignes complètes sont dans le journal de mesure ci-dessus ; aucun
dump ne rendait la bonne valeur avant, tous la rendent après.

### 4.4 Non-régression — 20 programmes comparés à `rose/rose`

20 programmes de `rose/tests_aot8/programmes` (tous acceptés par la
référence), dumps générés par `$R rir`, ELF CORR6 exécuté, sortie comparée
octet à octet à `ROSE_SANS_JIT=1 $R <source>` :

```
01_arith 02_fib 03_varfact 04_bigint_puissance 06_rationnel_add
07_rationnel_var 09_chaine 10_concat_chaines 11_liste_const
12_longueur_liste 13_liste_idx 14_table 15_table_cles 16_pgcd
17_boucle_pour 19_sortie_multiple 22_rationnel_mul 23_fib_iteratif
24_compose 25_racine_puissance

RESULT: OK=20 BAD=0 SKIP=0
```

**0 refus supplémentaire, 0 divergence.** Le programme `20_hasard` est
exclu par construction (tirage aléatoire non reproductible), `05_bigint_factorielle`
et `21_primitives_const` sont exclus car la référence elle-même les refuse.

## 5. Validation finale

| Contrôle | Commande | Résultat mesuré |
|---|---|---|
| `verif` interpréteur | `$R verif compilateur/interpreteur_rir.rose` | `vérifié`, rc=0 |
| `verif` banc | `$R verif compilateur/tests_corr6.rose` | `vérifié`, rc=0 |
| Tests CORR6 | `ROSE_SANS_JIT=1 $R compilateur/tests_corr6.rose <dir>` | `13 OK, 0 ECHEC` |
| C0 → ELF | `$C0 compilateur/interpreteur_rir.rose` | rc=0, 215 144 o, magic ELF aarch64 |
| Jeux d'essai RIR | ELF × 14 dumps | `OK=14 ECHEC=0` |
| Non-régression | ELF × 20 dumps vs `rose/rose` | `OK=20 BAD=0 SKIP=0` |
| **Point fixe `b3ed9262`** | `$R rir compilateur/emetteur.rose` **et** `$R compilateur/emetteur.rose compilateur/emetteur.rose` | **`b3ed9262dd54776a6dbd07bc12368d0e` ×2**, 263 520 o, `cmp` = 0, stderr 0 |
| `validation_finale.sh` | `RUST_TEST_THREADS=1 bash scripts/garde_fou_mem.sh 12000000 bash scripts/validation_finale.sh` | **RC=0, `RESULTAT: 0 échec(s)`** : tests 314+327 OK, point fixe OK, AOT 75/75, JIT 25/25, WASM 92 o, RVM 317811, AZ 31 CONFORME/0 non-conforme, HOST-1 fib=317811. Journal propre : `/tmp/opencode/corr6_validation_finale_final.log`, rejoué sur l'état final après la dernière modification de `call_prim` |
| Arbre | `git status --short` | `M compilateur/interpreteur_rir.rose`, `?? compilateur/tests_corr6.rose`, `?? rose/docs/CORR6_RIR.md` uniquement |

## 6. Rejouer

```bash
cd /tmp/opencode/corr_p6
R=$PWD/rose/target/release/rose
C0=/tmp/opencode/j1_C0_from_C0ext.elf
D=/tmp/opencode/corr6_replay
mkdir -p "$D"

# 1. banc + dumps + ATTENDUS.tsv
ROSE_SANS_JIT=1 $R compilateur/tests_corr6.rose "$D"

# 2. compilateur C0 -> ELF
bash scripts/garde_fou_mem.sh 12000000 timeout 900 \
  $C0 compilateur/interpreteur_rir.rose > "$D/interp.elf"
chmod +x "$D/interp.elf"

# 3. 14 jeux d'essai
while IFS=$'\t' read -r n e; do
  [ -z "$n" ] && continue
  got=$("$D/interp.elf" "$D/$n.rir" | tr '\n' '|'); got=${got%|}
  [ "$got" = "$e" ] && echo "OK $n" || echo "ECHEC $n [$e] != [$got]"
done < "$D/ATTENDUS.tsv"

# 4. point fixe (doit rester b3ed9262 ×2)
$R rir compilateur/emetteur.rose | md5sum
$R compilateur/emetteur.rose compilateur/emetteur.rose | md5sum
```

## 7. Ce qui reste hors de ce correctif

Les sources `t104`–`t107` restent **refusées par le moteur Rust**
(`rose/src/**`, hors périmètre) : ce correctif rend l'**interpréteur RIR**
capable d'exécuter les formes correspondantes dès qu'un front-end les émet,
et fournit les 14 dumps de référence. L'extension du front-end natif pour
accepter la surcharge et les fonctions d'ordre supérieur est un chantier
distinct ; elle n'était pas nécessaire ici et aurait modifié le point fixe
`b3ed9262`, explicitement interdit.

Toutes les mesures ci-dessus sont des constats datés du 23/11/2026 dans ce
worktree ; à re-mesurer après tout changement de C0, de la graine Rust ou de
la topologie.
