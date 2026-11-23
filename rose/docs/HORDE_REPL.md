# HORDE REPL — console interactive de Rose en Rose pur (23/11/2026)

Worktree exclusif `/tmp/opencode/horde_repl`, HEAD `101b48d`.
Fichiers possédés (les trois livrés) :

| Fichier | Rôle | Lignes |
|---|---|---:|
| `compilateur/repl_rose.rose` | REPL en Rose pur (boucle + moteur de valeurs + parseur) | 807 |
| `compilateur/tests_repl_rose.rose` | 32 assertions dont la transcription dorée des 10 commandes | 127 |
| `rose/docs/HORDE_REPL.md` | ce document | — |

Aucun autre fichier du dépôt n'a été modifié. Aucun commit, push ou
publication. Tous les artefacts d'essai sont hors dépôt, dans
`/tmp/opencode/repl_work/`.

## 1. Ce qui a été lu (sémantique cible)

`rose/src/repl.rs` (lecture seule) a été lu en entier : boucle
`lancer()` (l. 529-764), `parse_commande()` (l. 392-398),
`MethodeRepl` (l. 223-271), `banniere()` (l. 456-492), `aide()`
(l. 315-339), `est_definition()` (l. 511-521), `braces_balanced()`
(l. 282-311), historique (l. 174-197). Le contrat retenu est :

* invite `rose[<méthode>]> `, mémoire de la méthode (`jit` par défaut) ;
* commandes `:` avec argument séparé au premier blanc ;
* définitions mémorisées dans un buffer, `>>` implicite sur les autres
  lignes, donc persistance d'état entre lignes ;
* historique des entrées (borné à 1000) ;
* `:show` imprime le buffer, `:quit` termine, `:methode` change l'invite.

## 2. Architecture du port

`repl_rose.rose` est autonome (sous-ensemble G5 d'émettabilité C0) :

1. **`rep_*` value engine** (marqueurs `BEGIN/END REP_VALUE_ENGINE`) :
   valeurs taguées `i:` `s:` `b:` `v:`, arithmétique décimale exacte
   (`rep_bnorm`, `rep_badd`, `rep_bsub`, `rep_bmul`, `rep_bdivmod`,
   `rep_bpow`, `rep_fact_big`), `rep_val_*`. C'est une réplique fidèle du
   moteur de valeurs de `compilateur/interpreteur_rir.rose`.
2. **Lexeur + parseur Pratt** `rep_tokens` / `rep_parse_or` … : entiers
   exacts, chaînes, booléens, `+ - * / % ^`, comparaisons `== != < <= > >=`,
   `et/ou/non`, parenthèses, unaire `-`, postfixe `!`, appels de
   primitives (`fact`, `fib`, `abs`, `longueur`, `type`, `pgcd`, `min`,
   `max`). Précédences identiques à `rose/src/interp.rs`.
3. **Contexte** : `rep_var_get`/`rep_var_set` (liste de paires `nom=valeur`)
   donnent la persistance des `$x = …` entre lignes ; `rep_hist` est
   l'historique, `rep_buffer` le buffer de définitions.
4. **`rep_run(script)`** (l. 715-801) : la boucle complète. Le point
   d'entrée (`BEGIN/END REP_MAIN`) ne fait que
   `>> rep_run(lire_texte(argument(0)))`. Isoler la boucle permet au banc
   de tests d'appeler `rep_run` sans entrée-sortie.

## 3. Chaîne C0 → ELF (preuves d'exécution)

### 3.1 C0 rebuilt depuis CE worktree

```
cd /tmp/opencode/horde_repl
rose/rose rir compilateur/rose_natif.rose > /tmp/opencode/repl_work/rose_natif.rir
   903 626 octets, md5 921db7cb8aff2f600efb6d6a56d3fbb7
rose/rose compilateur/emetteur_elf.rose <rir> > /tmp/opencode/repl_work/C0_repl.elf
   2 153 820 octets, ELF aarch64 statique, magic 7f454c4602010100
```
⚠ Ce C0 reconstruit exige un second argument (chemin de sortie) ; avec un
seul argument il répond « index hors des limites des arguments ». Le C0
de référence `md5 7bb091200cf927022eb9d86a548d2502` (J6, déjà présent
dans `/tmp/opencode/C0_ext.elf`) accepte un seul argument. Les deux ont
été utilisés et donnent le même résultat.

### 3.2 Compilation du REPL

```
# C0 de référence (1 argument)
bash scripts/garde_fou_mem.sh 12000000 /tmp/opencode/C0_ext.elf \
     compilateur/repl_rose.rose > /tmp/opencode/repl_work/repl2.elf
   rc=0, 160 516 octets, magic 7f454c4602010100

# C0 reconstruit ici (2 arguments)
/tmp/opencode/repl_work/C0_repl.elf compilateur/repl_rose.rose /tmp/opencode/repl_work/out
   rc=0, 160 792 octets sur stdout, même magic
```

### 3.3 Exécution et parité avec `rose console`

Script d'entrée `/tmp/opencode/repl_work/input10.txt` (10 commandes) :

```
2 + 3
10!
$x = 7
$x * $x
5 > 3
"ab" + "cd"
:methode interprete
:show
:help
:quit
```

Oracle : `ROSE_NO_COLOR=1 rose/rose console < input10.txt` (2 090 octets).

| Exécutable | Sortie | Comparaison à l'oracle |
|---|---|---|
| `rose/rose compilateur/repl_rose.rose` (moteur Rust) | 2 091 o | identique après `rstrip('\n')` |
| ELF C0 de référence | 2 091 o | **identique après `rstrip('\n')`** |
| ELF C0 rebuilt ici | 2 091 o | **identique après `rstrip('\n')`** |

L'unique différence d'un octet est le `\n` final : `afficher`/`>>` du
côté Rose l'ajoutent, le REPL Rust ne l'ajoute pas sur `:quit`. La
comparaison normalise ce seul octet (documenté et déterministe).

## 4. Intégration avec l'interpréteur RIR et le moteur de valeurs

Le générateur `/tmp/opencode/repl_work/build_integration.py` (hors dépôt)
construit une source combinée de 2 609 lignes :

```
compilateur/moteur_valeurs.rose
compilateur/moteur_ops.rose
compilateur/interpreteur_rir.rose  (point d'entrée	retiré)
compilateur/repl_rose.rose         (REP_VALUE_ENGINE remplacé par 34 wrappers)
```

Les 34 wrappers `rep_x(...) -> ... { retour x(...) }` font appel aux
vraies fonctions `val_add`, `val_str`, `bnorm`, `fact_big`, `vtag`, …
de `interpreteur_rir.rose`. Preuves :

* `rose verif /tmp/opencode/repl_work/combined.rose` → `vérifié` (rc=0) ;
* ELF C0 : 389 732 octets, rc de compilation 0, exécution rc=0 ;
* sortie sur les 10 commandes : **identique à l'oracle** (2 090 o).

Donc le REPL porte bien sur le moteur de valeurs / l'interpréteur RIR,
et la version autonome n'est qu'une copie locale de ces primitives.

## 5. Tests — `tests_repl_rose.rose`

Compilé par concaténation (`repl_rose.rose` sans `BEGIN/END REP_MAIN` +
 le banc), `rose verif` → `vérifié`, ELF C0 211 696 octets :

```
TESTS REPL : 32 OK, 0 ECHEC
```

Répartition : 10 tests arithmétique/valeurs, 12 tests évaluateur,
8 tests analyse de ligne/contexte, 1 test de transcription dorée
(les 10 commandes, 1 833 octets comparés octet pour octet à la sortie
mesurée de `rose console`), 1 test d'alias `:aide`. Le dernier test
compare la chaîne construite par `rep_run($script10)` à la transcription
attendue ; les deux variantes (moteur Rust et ELF C0) donnent 0 échec.

## 6. Couverture et écarts (constats mesurés)

### Couvert

| Élément de `repl.rs` | Porté | Preuve |
|---|---|---|
| Invite méthode + changement `:methode jit\|aot\|interprete` | oui | transcription, prompt `rose[interprete]>` |
| `:quit` / `:q` / `:exit` | oui | arrêt de boucle, 10e ligne |
| `:aide` (alias `:help`, `:h`, `:?`) | oui | 32 tests + transcription |
| `:show` / `:code` (buffer) | oui | ligne `$x = 7` dans la transcription |
| `:reset`, `:type` | oui | tests unitaires |
| Bannière ASCII et aide (textes exacts) | oui | transcription 2 090 o |
| Persistance d'état `$x = 7` → `$x * $x` = 49 | oui | transcription |
| Historique (`rep_hist`, borne 1000) | oui | code, l. 743 et 769 |
| Continuation sur ligne vide (prompts concaténés) | oui | `rose[jit]> rose[jit]> 49` |
| Arithmétique exacte illimitée (décimales) | oui | `10!` = 3628800, tests bigint |

`:aide` est une **extension** du port (le REPL Rust ne connaît que
`:help`/`:h`/`:?` et répond `commande inconnue`). Le script doré de
parité utilise donc `:help`. Un second script de 7 commandes a vérifié
que tout le reste — `:methode aot` (prompt `rose[aot]>`), `4*5`,
`:type 42`, `:reset`, `:show`, `:quit` — reste octet-identique à
l'oracle ; seule la ligne `:aide` diverge, par extension voulue.

### Écarts documentés (périmètre G5, non des promesses)

* Entrée **scriptée** (`argument(0)`) uniquement : pas d'édition de ligne
  (flèches, Tab, Ctrl-C/L), absente du runtime G5 et propre à
  `ReadConsoleInputW` sous Windows. Aucune couleur ANSI : la comparaison
  est faite avec `ROSE_NO_COLOR=1` (comportement du REPL Rust en pipe).
* Pas de continuation `...>` multi-ligne (`braces_balanced`) : le script
  fournit une ligne par entrée.
* Commandes non portées : `:load`, `:save`, `:latex`, `:more`/`:plus`/
  `:digits` (résultats symboliques absents de l'ABI G5).
* Valeurs : entiers exacts, chaînes, booléens. Rationnels/flottants/
  complexes/listes/tables ne sont pas évalués par le parseur du REPL
  (le moteur RIR complet, lui, les porte ; voir J3).
* `fn ...` définies en ligne : non prises en charge ; seules les
  affectations `$x = …` alimentent le contexte.
* Version de la bannière figée à 0.1.73 (celle du binaire oracle).

## 7. Pièges G5 mesurés pendant ce chantier

1. `afficher("x")` est **éliminé** par C0 ; seule la forme `>> expr`
   émet une écriture. Le point d'entrée utilise donc `>> rep_run(...)`.
2. C0 évalue **avec empressement** l'argument d'un appel de fonction
   dans un `et` : `rep_op($t[$q])` dans `tant_que ($q < longueur($t) et …)`
   segfaultait à l'index hors bornes. Remplacé par `rep_op_at()` qui
   teste la borne **avant** l'indexation.
3. Un littéral chaîne de 39 lignes concaténées en une seule expression
   globale segfaultait à l'exécution ; découpé en 4 globales
   (`$gpart0` … `$gpart3`) dans le banc de tests.
4. Rose n'accepte pas `retour [liste]` (erreur de parse) : passer par une
   locale `$rr = [...]` puis `retour $rr`.
5. `["", "", ""]` crée des emplacements aux index 0,1,2 ; `joindre`
   ajoute en 3,4. Un résultat de 2 éléments se construit avec `[""]`.

## 8. Validation globale

* `rose verif compilateur/repl_rose.rose` → `vérifié` (rc=0).
* `rose verif` sur la concaténation des tests → `vérifié` (rc=0).
* ELF C0 REPL + ELF C0 tests : rc=0, `TESTS REPL : 32 OK, 0 ECHEC`.
* Point fixe émetteur : `b3ed9262dd54776a6dbd07bc12368d0e` (inchangé).
* `RUST_TEST_THREADS=1 bash scripts/garde_fou_mem.sh 12000000 \
  bash scripts/validation_finale.sh` → **RC=0, `=== RESULTAT: 0 échec(s) ===`**
  (`/tmp/opencode/repl_work/validation_finale_run2.log`) : 314+327 tests Rust
  verts, point fixe, AOT 75/75, JIT 25/25, WASM, RVM 317811, AZ 31/31
  CONFORME, HOST-1 fib=317811.
  ⚠ Première exécution : 1 échec (AZ 31 BLOQUÉ) uniquement parce que le
  worktree ne contenait pas encore `rose/rose` (seul `rose/target/release/rose`
  existait après l'étape 1). Copie du binaire buildé vers `rose/rose`
  (artefact gitignoré, aucun fichier source modifié) puis re-exécution → 0 échec.

## 9. Rejouer

```bash
cd /tmp/opencode/horde_repl
# vérification
./rose/rose verif compilateur/repl_rose.rose
# banc de tests (concaténation)
awk '/BEGIN REP_MAIN/{s=1} /END REP_MAIN/{s=0; next} !s' compilateur/repl_rose.rose > /tmp/opencode/repl_work/repl_no_main.rose
cat /tmp/opencode/repl_work/repl_no_main.rose compilateur/tests_repl_rose.rose > /tmp/opencode/repl_work/tests_full.rose
./rose/rose verif /tmp/opencode/repl_work/tests_full.rose
bash scripts/garde_fou_mem.sh 12000000 /tmp/opencode/C0_ext.elf /tmp/opencode/repl_work/tests_full.rose > /tmp/opencode/repl_work/tests.elf
chmod +x /tmp/opencode/repl_work/tests.elf && /tmp/opencode/repl_work/tests.elf
# REPL complet
bash scripts/garde_fou_mem.sh 12000000 /tmp/opencode/C0_ext.elf compilateur/repl_rose.rose > /tmp/opencode/repl_work/repl.elf
chmod +x /tmp/opencode/repl_work/repl.elf
ROSE_NO_COLOR=1 ./rose/rose console < /tmp/opencode/repl_work/input10.txt > ref.out
/tmp/opencode/repl_work/repl.elf /tmp/opencode/repl_work/input10.txt > elf.out
diff <(python3 -c "print(open('ref.out','rb').read().rstrip(b'\n').decode())") \
     <(python3 -c "print(open('elf.out','rb').read().rstrip(b'\n').decode())")
```

Toutes les mesures sont des constats datés du 23/11/2026 dans ce worktree ;
à re-mesurer après tout changement de C0, de la graine Rust ou de la
topologie.
