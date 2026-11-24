# XEVAL_EXT — extension de `x_eval`/`x_op` de l'émetteur PE x64

Constat daté du 24/11/2026, worktree exclusif `/tmp/opencode/last2` (HEAD
`be13861`). Ce document décrit ce que l'émetteur `compilateur/emetteur_pe_x64.rose`
couvre APRÈS l'extension, et ce qui reste approximatif. Aucun PE n'est exécuté :
l'hôte est aarch64, la validation est **structurelle**.

## 1. Blocage initial reproduit

```
$ C0 compilateur/emetteur_pe_x64.rose > emetteur_base.elf
$ ./emetteur_base.elf /tmp/opencode/rf_stage/moteur.rir.rust > rose.exe
rc=2 ; stderr : X64 : forme non couverte : lst
```

Le RIR complet (795 634 o, 705 `fonction`, 1 795 `param`) contient :

| Famille | occurrences | forme RIR |
|---|---:|---|
| `v` | 12 514 | variable / slot |
| `bin` | 6 946 | 14 opcodes (`+ - * / div % ^ == != < <= > >= et ou`) |
| `app` | 3 803 | 319 noms : 296 fonctions RIR + 23 primitives |
| `s` | 2 887 | littéral chaîne |
| `n` | 2 531 | littéral entier |
| `idx` | 1 082 | indexation |
| `appel_rir` | 966 | appel de fonction RIR |
| `lst` | 248 | littéral liste |
| `b`, `non`, `uni`, `neg`, `f` | 203 | booléens et unaires |
| `bloc` | 1 | marqueur d'instruction imbriquée (no-op) |

L'émetteur d'origine ne connaissait que `n/b/slot/v/bin/app/neg/non` et les
opérations entières non étiquetées.

## 2. ABI des valeurs étiquetées (x64, calquée sur le backend G5 aarch64)

| Type | Représentation | Descripteur (8 octets par mot) |
|---|---|---|
| entier / booléen | `n << 3` (tag `000`) | aucun |
| chaîne | `pointeur | 1` | `[1][ncp][noct][octets UTF-8…]` |
| liste | `pointeur | 2` | `[2][len][cap][pointeur vers]` ; le tableau de valeurs est détaché, donc `joindre` préserve les alias |
| table | `pointeur | 3` | `[3][len][cap][pointeur vers]` ; entrées `(clé, valeur)` de 16 octets |

`HeapAlloc` renvoyant des blocs alignés 8, les 3 bits bas du pointeur sont
libres. Les entiers étiquetés rendent `+`, `-`, `&`, `|`, `neg` et les
comparaisons signées directement corrects ; `*`, `/`, `%`, `^` désétiquettent
(`sar 3`) puis réétiquettent (`shl 3`).

## 3. Tas et syscalls KERNEL32

La table d'imports passe de 3 à **12 entrées** (ILT `0x60`, IAT `0xC8`,
hint/names 12 × 32 o à `0x140`, DLL name `0x2C0`, `.data` portée à 1024 o —
le pas de 16 o du stub tronquait `GetCommandLineA` et `GetTickCount64`) :

`GetStdHandle`, `WriteFile`, `ExitProcess`, **`GetProcessHeap`**,
**`HeapAlloc`**, **`HeapFree`**, `CreateFileA`, `ReadFile`, `CloseHandle`,
`GetFileSize`, `GetCommandLineA`, `GetTickCount64`.

Les 3 premiers étaient déjà présents ; `GetProcessHeap`/`HeapAlloc`/`HeapFree`
ont été **ajoutés** (ils n'étaient pas « déjà presented »). `HeapAlloc` sert à
toutes les allocations de descripteurs et de tableaux ; `HeapFree` est appelé
lors du redimensionnement d'une liste ou d'une table.

## 4. Formes couvertes

- Littéraux : `n`, `entier`, `f`, `flottant`, `b`, `s`, `lst` ; formes nues
  `slot N`, `entier N`, `v nom`, `b vraie|faux`.
- Opérateurs : `bin` (14 opcodes, avec concaténation de chaînes pour `+` et
  comparaison lexicographique UTF-8 pour `== != < <= > >=`), `neg`, `uni`,
  `non` (via `x_truth`), `idx` (liste, table, chaîne en points de code).
- Appels : `appel_rir` et `app` résolus vers une fonction RIR, avec passage
  des arguments 5..n sur la pile (`[rsp+40+8*(i-4)]`) ; `app` non résolu est
  dispatché vers la primitive correspondante.
- Instructions : `affect slot`, `affect_ast slot`, `expr`, `retour`,
  `retour_abstrait`, `saut`, `saut_si_faux`, `saut_si_faux_abstrait`,
  `afficher`, `afficher_abstrait`, `bloc` (no-op).

## 5. Primitives émises (23/23)

Implémentées avec une sémantique réelle :
`longueur` (ncp / len), `sous_chaine` (indices en points de code, UTF-8),
`joindre(liste, valeur)` (croissance avec `HeapFree` de l'ancien tableau),
`inserer`/`cle_existe`/`supprimer`/`table`, `chaine` (décimal signé),
`entier` (analyse décimale), `quitter` (`ExitProcess`), `afficher_erreur`
(stderr via `GetStdHandle(-12)` + `WriteFile`), `temps`/`hasard`
(`GetTickCount64`), `ordre`, `caractere`.

Approximations documentées (aucune ne bloque l'émission) :
`ordre` et `caractere` sont exacts pour l'ASCII et replient sur `?` hors plage ;
`lire_texte`, `ecrire_bytes`, `ecrire_texte` rendent une chaîne vide / `vide`
(les imports fichier sont présents mais non câblés) ;
`argument` rend `""`, `nb_arguments` rend `0` ;
`flottant`/`partie_entiere` se comportent comme l'identité sur un entier ;
`graine` est sans effet. Il n'y a ni flottants ni grands entiers ni rationnels
natives : ces valeurs circulent sous forme de chaînes, comme dans le RIR.

## 6. Performance de l'émetteur

Deux corrections ont été nécessaires pour rester sous le garde-fou
`garde_fou_mem.sh 12000000` :

1. `x_label()` ne balaie plus la liste des étiquettes (14,8 Gio atteints sur le
   RIR complet) : elle consulte une **table** `id -> offset`.
2. `x_patch()` utilise `pe_join()` (jointure équilibrée) au lieu d'une
   concaténation octet par octet.

Mesures sur un sous-RIR de 200 fonctions : 15,9 s / 49 Mio avant correction 1,
après : 83 s / 202 Mio (le temps restant est dominé par l'émission des
littéraux chaîne et les recherches de slots).

## 7. Tests

- `compilateur/tests_pe_x64.rose` : **13/13 OK** (non-régression).
- `compilateur/tests_xeval.rose` (nouveau) : 14 assertions sur les formes
  `s/lst/idx/bin/app`, le catalogue de primitives, `bloc`, les appels à 5
  arguments, les 12 imports et les encodeurs du tas.
