# CORR9 — Branchement des modules self-hostés dans `call_prim` / `tok_line`

Worktree exclusif `/tmp/opencode/corr_b9`, HEAD `d117e04`.
Fichiers possédés : `compilateur/interpreteur_rir.rose` (uniquement `tok_line`
et `call_prim`), `compilateur/tests_b9.rose` (nouveau), ce document.
Aucun commit, push ou publication. Aucun autre fichier source modifié.

## 1. Problème (constat HORDE_INTEGRATION.md §9)

Les modules `bigint_rose`, `tables_rose`, `chaine_rose` et `unicode_rose`
sont **composés** (leurs `fn` sont présentes dans l'ELF `moteur_complet`) mais
leurs primitives n'étaient pas appelées : `call_prim` (`interpreteur_rir.rose:830`)
et `tok_line` (`:610`) utilisaient encore les listes plates sérialisées
(`l:<n>|<len>:<v>|…`, `t:…`) et l'arithmétique décimale maison (`bnorm`,
`badd`, `bmul`, `bgcd`, `bpow`). Le branchement est une **adaptation d'API**,
pas une réécriture.

## 2. Ce qui a été branché

| Module | Fonction module | Primitive RIR / site | Adaptation d'API | Résultat |
|---|---|---|---|---|
| `unicode_rose` | `u_ordre` | `ordre` | `u_ordre(chaine)` → entier, conversion directe | **branché** (repli si < 0) |
| `unicode_rose` | `u_caractere` | `caractere` | `u_caractere(cp)` → chaîne, conversion directe | **branché** (repli si vide) |
| `bigint_rose` | `bi_gcd` | `pgcd` / `gcd` | `bi_depuis_chaine` → `bi_gcd` → `bi_texte` → `bnorm` | **branché** (repli si non décimal) |
| `bigint_rose` | `bi_pow` | `puissance` / `pow` | même adaptateur ; exposant entier ≥ 0 | **branché** (repli si exposant < 0) |
| `chaine_rose` | `cr_creer`, `cr_feuille`, `cr_tranche` | `tok_line` (littéraux) | une arène locale par littéral, `$tok` reste `liste<chaine>` | **branché** |
| `tables_rose` | `t_inserer`, `t_cle_existe` | `inserer`, `cle_existe` | API incompatible (4 magasins persistants + racine) | **repli exact** documenté §3 |
| `unicode_rose` | `u_longueur`, `u_sous` | `longueur`, `sous_chaine` | API compatible mais régression G5 mesurée | **repli exact** documenté §3 |
| `chaine_rose` | `cr_concat` | `concatener` | API compatible mais régression G5 mesurée | **repli exact** documenté §3 |

`idx` est une forme d'`eval_expr`, pas une entrée de `call_prim` : il n'a pas
été touché (hors périmètre « uniquement `call_prim` et `tok_line` »).

## 3. Replis exacts — mesures à l'appui

Toutes les mesures sont des **constats datés du 23/11/2026** dans ce worktree,
obtenus avec l'émetteur auto-hébergé `C0_j6.elf` (AArch64) sur la même machine,
médianes de 3 séries entrelacées, garde-fou `garde_fou_mem.sh 12000000`.

| Cas | Module | Repli | Mesure C0 |
|---|---|---|---|
| `longueur` sur 26 cp × 20 000 | `u_longueur` 1 288 ms | natif 1 147 ms | module **12 % plus lent** (va-et-vient UTF-8 en `liste<entier>`) |
| `sous_chaine` tranches longues | `u_sous` | natif | module 5–7× plus lent |
| `concatener` 60 × 10×480 cp | rope `cr_*` 1 883–2 945 ms | natif 246–361 ms | rope **5–7× plus lent** (chaque `joindre` recopie l'arène) |
| `inserer`/`cle_existe` | `t_*` | `tab_set`/`tab_has` | API incompatible : 4 magasins `nd, cl, vl, fl` + racine mutés en place, aucun globale supplémentaire autorisé |
| `puissance` exposant < 0 | `bi_pow` | `val_pow` → `bpow` | `bi_pow` indexe `bi_tab_get(tab, -1)` : API incompatible |

Le repli est **le code d'origine, non modifié** : la parité est donc exacte par
construction sur ces chemins.

## 4. Gain de performance mesuré (objectif « si mesurable »)

Même banc, before = ELF compilé depuis l'interpreteur d'origine (205 084 o),
after = ELF compilé depuis l'interpreteur branché. Dump RIR généré une fois par
`rose/rose rir`, non inclus dans le chronomètre.

| Banc (4 000 itérations) | before | after | Gain |
|---|---:|---:|---:|
| `puissance(7, 40)` (1 exécution) | 1 944 ms | 542 ms | **3,6× plus rapide** |
| `puissance(7, 40)` (2 exécutions, médiane) | 1 873 ms | 497 ms | **3,8× plus rapide** |
| `pgcd(1071, 462)` (médiane) | ~137 ms | ~124 ms | **~1,1× plus rapide** |
| `puissance` + `pgcd` | 1 619 ms | 174 ms | **9,3× plus rapide** |

Le coût résiduel du rope dans `tok_line` est **dans le bruit de mesure** :
A/B sur le même ELF avec/sans les deux lignes `cr_feuille`/`cr_tranche`
(26 cp × 20 000 appels de `longueur`) : 1 162–2 256 ms avec rope contre
1 347–2 392 ms sans, séries entrelacées non discriminantes.

Cause : `bi_pow` (fenêtre 2^w, table de carrés) et `bi_gcd` (Lehmer) remplacent
l'exponentiation binaire décimale et l'Euclide décimal sur chaînes. Le gain est
porté par `puissance`, massivement appelé dans ce banc.

Sur les 20 programmes de parité (charges mixtes courtes), le temps total est
**équivalent** : before 17/23/22 ms, after 20/24/21 ms (3 séries) — dans le
bruit de mesure, aucun gain ni régression.

## 5. Parité — 20 programmes avant/après

Méthode : 20 sources ciblant `longueur`, `joindre`, `idx`, `inserer`,
`cle_existe`, bigint et unicode (liste dans `/tmp/opencode/b9_parite.sh`) ;
dump `rose/rose rir` généré hors chaîne ; oracle `ROSE_SANS_JIT=1 rose/rose` ;
sortie ELF comparée octet pour octet.

| ELF | OK | BAD | SKIP |
|---|---:|---:|---:|
| before (205 084 o) | 20 | 0 | 0 |
| after (345 964 o) | 20 | 0 | 0 |

Aucune divergence introduite par le branchement.

Extension au corpus J3 complet (141 `tests_selfhost/` + 25
`rose/tests_aot8/programmes/`) : **OK=157 BAD=1 SKIP=8**, résultat **identique
au constat J3 d'origine** (157/158 = 99,4 %). L'unique divergence est
`20_hasard` (tirage aléatoire, non reproductible par construction).

## 6. Validation

| Contrôle | Preuve |
|---|---|
| `rose verif` source composée | `vérifié` (interpréteur + 4 modules, 107 992 o) |
| `rose verif` composite | `vérifié` (`compilateur/moteur_complet.rose`, 10 842 lignes / 394 792 o) |
| C0 → ELF interpréteur composé | rc=0, magic `7f454c46`, 345 964 o |
| C0 → ELF composite | rc=0, magic `7f454c46`, 1 483 580 o ; 6/6 sources comparées conformes |
| `tests_b9.rose` (oracle Rust) | `TESTS B9 : 32 OK, 0 ECHEC` |
| `tests_b9.rose` (ELF C0) | `TESTS B9 : 32 OK, 0 ECHEC` |
| parité J3 complète | `OK=157 BAD=1 SKIP=8` = référence J3 |
| point fixe `b3ed9262` | `b3ed9262dd54776a6dbd07bc12368d0e` ×2, identique |
| `validation_finale.sh` | **RC=0, 0 échec** : tests 314+327 ok, AOT 75/75, JIT 25/25, WASM 92 o, RVM 317811, AZ 31 CONFORME, HOST-1 fib=317811 |

⚠ Le premier passage de `validation_finale.sh` a montré un faux échec
(`programme_minimal_jit_reste_conforme`, sensible au parallélisme) et des
journaux partagés (`/tmp/opencode/vf_*`) entre agents simultanés. Le contrôle
retENU est la commande canonique du projet, `RUST_TEST_THREADS=1`, avec des
journaux privés : **RC=0, 0 échec**.

⚠ **Composition requise** : comme le langage Rose n'a pas d'`include`, les
quatre modules doivent être concaténés à l'interpréteur pour que les appels
`u_*`/`bi_*`/`cr_*` soient résolus. Le fichier possédé
`compilateur/interpreteur_rir.rose` seul n'est plus autonome : c'est le sens
même du branchement. `scripts/build_moteur_sans_rust.sh` fait déjà cette
composition pour `moteur_complet.rose`.

## 7. Rejouer

```bash
cd /tmp/opencode/corr_b9
# composition interpréteur + 4 modules (fichier temporaire, non versionné)
{ cat compilateur/interpreteur_rir.rose compilateur/bigint_rose.rose \
    compilateur/tables_rose.rose compilateur/chaine_rose.rose \
    compilateur/unicode_rose.rose ; } > /tmp/opencode/b9_interp_composed.rose
rose/rose verif /tmp/opencode/b9_interp_composed.rose
# ELF + parité 20 programmes
C0=/tmp/opencode/j6_agents/C0_j6.elf
bash scripts/garde_fou_mem.sh 12000000 timeout 900 $C0 \
  /tmp/opencode/b9_interp_composed.rose > /tmp/opencode/b9_interp_after.elf
bash scripts/garde_fou_mem.sh 12000000 bash /tmp/opencode/b9_parite.sh \
  /tmp/opencode/b9_interp_after.elf replay
# tests du point 9 (interpréteur sans main + modules + tests_b9)
python3 - <<'PY'
from pathlib import Path
def r(n): return Path(f'compilateur/{n}.rose').read_text(encoding='utf-8')
i=r('interpreteur_rir').splitlines()
head='\n'.join(i[:next(k for k,l in enumerate(i) if "Point d'entrée" in l)])
mods=[r(n) for n in ('bigint_rose','tables_rose','chaine_rose','unicode_rose')]
Path('/tmp/opencode/b9_tests_composed.rose').write_text('\n'.join([head]+mods+[r('tests_b9')]),encoding='utf-8')
PY
ROSE_SANS_JIT=1 rose/rose /tmp/opencode/b9_tests_composed.rose
$C0 /tmp/opencode/b9_tests_composed.rose > /tmp/opencode/b9_tests.elf
./b9_tests.elf
# composite complet + validation
bash scripts/build_moteur_sans_rust.sh --source-only
rose/rose verif compilateur/moteur_complet.rose
RUST_TEST_THREADS=1 bash scripts/validation_finale.sh
```

## 8. État factuel

Fait : 4 primitives branchées (`ordre`, `caractere`, `pgcd`/`gcd`,
`puissance`/`pow`) + `tok_line` ; 3 replis exacts justifiés par mesure ;
parité 20/20 avant et après ; gain 3,6× sur `puissance`.
Vérifié : `rose verif`, C0→ELF ×2, tests 32/32 Rust et C0, parité 20/20,
parité J3 complète 157/158 égale à la référence, point fixe `b3ed9262`,
`validation_finale.sh` RC=0 / 0 échec (RUST_TEST_THREADS=1, journaux privés).
Reste : le porteur de la horde doit régénérer `moteur_complet.rose` depuis les
sources possédées (`build_moteur_sans_rust.sh --compilateur …`) avant
publication ; ce fichier a été restauré à son état git d'origine dans ce
worktree pour respecter le périmètre des fichiers possédés.
