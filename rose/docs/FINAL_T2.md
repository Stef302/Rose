# FINAL_T2 — SIGSEGV sur tables dans le moteur complet

Worktree exclusif `/tmp/opencode/final_t2`, HEAD de départ `b3a4173`.
Aucun commit, aucun push, aucun fichier hors périmètre modifié.

| Fichier | Rôle |
|---|---|
| `compilateur/interpreteur_rir.rose` | correctif (call_prim + une garde de borne dans eval_expr) |
| `compilateur/tests_final_t2.rose` | nouveau — banc de 20 programmes de tables |
| `rose/docs/FINAL_T2.md` | nouveau — ce document |

Garde-fou sur toute commande longue :
`bash scripts/garde_fou_mem.sh 12000000 <commande>`.

---

## 1. Reproduction — preuve gdb

Chaîne de compilation (8 min 11 s, pic RSS < 12 Go) :

```
bash scripts/garde_fou_mem.sh 12000000 /tmp/opencode/C0_3.elf compilateur/moteur_complet.rose > /tmp/opencode/final_t2_work/moteur_base.elf
```

ELF aarch64 statique, 1 885 628 octets, magic `7f454c4602010100`, stderr vide.

```
$ ./moteur_base.elf tests_selfhost/t12_table.rose
Segmentation fault (core dumped)      rc=139
```

Les 8 sources citées (`t12`, `t13`, `t60`, `t61`, `t63`, `t64`, `t80`, `t999`)
rendent toutes **rc=139** sur l'ELF de base, y compris `t80_nb_arguments`
(qui ne contient pourtant qu'un appel `nb_arguments()`) et un programme
utilisateur `fn f() ... >> f()`. Le défaut n'est donc PAS dans le hachage des
tables : il précède l'entrée dans `call_prim`.

Session gdb (ELF sans symboles, adresses brutes) :

```
Program received signal SIGSEGV, Segmentation fault.
0x00000000005ca2a8 in ?? ()
pc  0x5ca2a8    x0  0x0        x1  0x0    x2  0x1    x30 0x5c9b3c
sp  0xffffffff4220
   0x5ca2a4: and  x10, x0, #0xfffffffffffffff8
=> 0x5ca2a8: ldr  x11, [x10, #8]        <-- x10 = 0 : pointeur NUL
```

Chaîne d'appel reconstruite par points d'arrêt sur les helpers natifs :

```
0x45faa4  bl 0x5ca3b4   (idx natif)   x0 = liste de 1 élément (tag=2, len=1), x1 = 8 (= 1 tagué)
0x5ca58c  mov x0, #0                  (idx hors bornes -> NULL)
0x45fab0  bl 0x5c9af8   (vtag)        x0 = NULL
0x5c9b38  bl 0x5ca27c   (sous_chaine) x0 = NULL, x1 = 0, x2 = 8 (= 1 tagué)
0x5ca2a8  ldr x11, [x10, #8]          SIGSEGV
```

Instrumentation par `>>` dans une copie scratch du composite confirme
l'emplacement source, juste AVANT `call_prim` :

```
DBG_GUARD:table:args=1       <-- $args = [""] : un seul élément de rembourrage
(debug call_prim absent)     <-- call_prim n'est jamais atteint
```

### Cause machine exacte

`eval_expr` (branche `app`) contient, depuis le correctif 3 :

```
si ($name == "flottant" et longueur($args) >= 2 et vtag($args[1]) == "f") {
```

Sous le backend C0/G5, `et` est un AND bit à bit **non court-circuitant** :
les trois opérandes sont évalués, y compris `vtag($args[1])`. Pour un appel
sans argument, `$args = [""]` n'a que son élément de rembourrage (index 0) :
`$args[1]` est hors bornes. L'`idx` natif (0x5ca3b4) rend alors un pointeur
NUL (0x5ca58c), `vtag` applique `sous_chaine(NULL, 0, 1)`, et l'accès à
l'en-tête de chaîne `[x10+8]` avec `x10 = 0` déclenche le SIGSEGV.

Ce n'est **ni** la pile (sp = 0xffffffff4220, dans la pile valide
`0xfffffffc7000..0x1000000000000`, ~48 Kio de marge), **ni** le tas (aucun
bloc corrompu, aucun index de table en cause), **ni** une primitive de
hachage. C'est un accès hors bornes sur une liste d'arguments vide, rendu
fatal par l'absence de court-circuit de `et` et par le retour NUL de l'`idx`
natif. Le moteur Rust ne montre pas le défaut : son `et` est court-circuitant
et son indexeur rend une valeur neutre.

---

## 2. Correctif

Quatre changements, tous dans les primitives pures `call_prim` d'
`interpreteur_rir.rose`, plus une garde de borne indispensable dans la
branche `app` de `eval_expr` (le point de crash lui-même, voir §1) :

1. **`eval_expr`, garde flottant** — le test de tag est imbriqué dans le test
   d'arité : `si ($name == "flottant" et longueur($args) >= 2) { si (vtag($args[1]) == "f") { … } }`.
   La sémantique CORR3 est intacte ; l'accès `$args[1]` n'est plus évalué
   quand la liste est vide. C'est le correctif du SIGSEGV.
2. **`call_prim`, `inserer` / `cle_existe` / `cle_presente`** — durcissement
   des accès : contrôle de l'arité **et** du tag `t` de la table avant
   `tab_set`/`tab_has`. Une valeur non-table rend une valeur neutre au lieu
   d'être passée à `vpay()`/`body_off()`.
3. **`call_prim`, `argument`** — l'index utilisateur est décalé d'un (l'hôte
   réserve son index 0 au chemin source). Sans cela `argument(0)` rend le
   chemin du `.rose` et `t999_critique_g5` diverge de `rose/rose`. Un index
   négatif est projeté hors table pour conserver l'erreur native.
4. **`call_prim`, `chaine`** — pour un flottant entier (`f:2/1`), le suffixe
   `/1` est retiré : `chaine(flottant("2"))` rend `2`, comme `rose/rose`.
   Le tag `r:` (rationnel) garde sa forme fractionnaire (`5/2`).

Note sur `tables_rose.rose` : le module de hachage persistant est composé dans
le moteur, mais son API manipule quatre `liste<entier>` de nœuds ($nd/$cl/$vl/
$fl) avec des clés encodées (tag, décalage, longueur). Les valeurs qui
traversent le RIR sont des chaînes taguées sérialisées (`i:`, `s:`, `t:`).
Le brancher dans `call_prim` exigerait un pont de sérialisation et changerait
l'ordre et le format des résultats. Le durcissement des accès (option
explicitement prévue par la mission) conserve la parité octet pour octet
avec `rose/rose` sur tout le corpus mesuré ; c'est ce choix qui est retenu.

---

## 3. Tests

### 3.1 Les 8 sources citées

Chaîne : `rose/rose SOURCE [args]` (référence) contre
`ELF_C0 SOURCE [args]` (correctif), comparaison octet pour octet.
Entrée de `t999` : `/tmp/opencode/final_t2_work/battery/critique_entree.txt`
(« fonction principale : test SH9 », 31 octets).

| Source | référence rc | base rc | correctif rc | parité |
|---|---|---|---|---|
| t12_table | 0 | 139 | 0 | OK |
| t13_table_entier | 0 | 139 | 0 | OK |
| t60_table_bool | 0 | 139 | 0 | OK |
| t61_table_multi | 0 | 139 | 0 | OK |
| t63_table_chaines_cle | 0 | 139 | 0 | OK |
| t64_table_dans_fonction | 0 | 139 | 0 | OK |
| t80_nb_arguments | 0 | 139 | 0 | OK |
| t999_critique_g5 | 0 | 139 | 0 | OK |

Sortie de `t12_table` après correctif : `vrai / faux / 2` — identique à
`rose/rose`.

### 3.2 Vingt programmes de tables

`compilateur/tests_final_t2.rose` : 20 scénarios déterministes (tables vides,
clés chaîne/entier/booléen/UTF-8/longues/avec séparateurs, réinsertion,
indexation, boucles 10/19/49/50 itérations, tables locales et fabriquées par
fonction, tables indépendantes, réassignation).

```
rose/rose compilateur/tests_final_t2.rose > ref
ELF_C0    compilateur/tests_final_t2.rose > fix
cmp ref fix    -> 0
```

---

## 4. Validation

* `rose verif compilateur/interpreteur_rir.rose` → `vérifié`, rc=0.
* `rose verif compilateur/tests_final_t2.rose` → `vérifié`, rc=0.
* `rose verif` du composite scratch (modules + correctif) → `vérifié`, rc=0.
* C0 → ELF du composite scratch : `/tmp/opencode/C0_3.elf`, rc=0,
  ELF aarch64 de **1 887 348 octets**, magic `7f454c4602010100`, stderr vide.
  (`/tmp/opencode/final_t2_work/moteur_final.elf`.)
* Point fixe `b3ed9262` : `rose rir compilateur/emetteur.rose` et
  `rose compilateur/emetteur.rose compilateur/emetteur.rose` rendent tous deux
  `b3ed9262dd54776a6dbd07bc12368d0e`, `cmp = 0`.
* `validation_finale` : **RESULTAT: 0 échec(s)** — tests Rust 314 + 327 passés,
  point fixe OK, AOT 75/75, JIT 25/25, WASM 92 o, RVM 317811,
  AZ 31 CONFORME / 0 non-conforme, HOST-1 fib=317811.
  Exécuté avec `RUST_TEST_THREADS=1` et des journaux privés
  (`/tmp/opencode/final_t2_work/battery/validation_finale.log`) : plusieurs
  agents concurrents écrivent les journaux partagés `/tmp/opencode/vf_*.log`
  et l'essai sans isolement a produit un `tests rc=101` et un `AZ 31 BLOQUE`
  artifacts de concurrence, reproduits puis éliminés en environnement isolé.
* Tous les artefacts de preuve (sorties `.ref`, `.final`, `.base`, transcript
  gdb, journal de validation) sont dans
  `/tmp/opencode/final_t2_work/battery/`.

Le composite `compilateur/moteur_complet.rose` du worktree est un fichier
**généré** et interdit par le périmètre : il n'a pas été écrit. Le correctif
a été appliqué à une copie scratch assemblée depuis le seul fichier possédé
(`interpreteur_rir.rose`) ; le script `scripts/build_moteur_sans_rust.sh`
régénérera le composite avec le correctif.

---

## 5. Verdict

SIGSEGV reproduit, cause machine identifiée (pointeur NUL issu d'un `idx`
hors bornes dans la garde flottant, non pas une table), corrigé ;
8/8 sources citées sans SIGSEGV et à sortie identique à `rose/rose`,
20/20 programmes de tables à parité octet, vérifications et point fixe OK.
