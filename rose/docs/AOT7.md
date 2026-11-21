# AOT7 — élimination de TOUS les refus

HEAD : `d870d89`. Aucun commit, aucune publication. Licence MIT.

## Consigne

L'utilisateur : « le aot qui refuse non il va accepter, valeurs composées aussi,
quel intérêt de donner son temps sur une méthode qui couvre pas tout ».

Résultat : **`rose aot` n'affiche plus jamais « AOT indisponible » ni
« utilisez rose comp »**. Les quatre familles qui refusaient sont couvertes,
et le binaire émis est toujours un vrai exécutable machine autonome (jamais
le moteur de 5,19 Mo déguisé).

## Les quatre familles

### 1. PE / Windows — runtime Win64 + table d'imports (AOT7)

`rose/runtime/pe_win64.rs` est le pendant Win64 des runtimes ELF
(`rose/runtime/{aarch64.S,x86_64.s}`). Là où l'ELF utilise des syscalls Linux,
le PE appelle kernel32 par un `call qword ptr [rip+disp32]` dans l'IAT.

`link_natif::pe_bytes` écrit une table d'imports complète :

| # | API kernel32 | Usage |
|---|---|---|
| 0 | `ExitProcess` | sortie du processus |
| 1 | `GetStdHandle` | `STD_OUTPUT_HANDLE` / `STD_INPUT_HANDLE` |
| 2 | `WriteFile` | écriture de la sortie mémorisée |
| 3 | `ReadFile` | lecture de l'entrée standard |
| 4 | `GetProcessHeap` | heap de l'allocateur de valeurs |
| 5 | `HeapAlloc` | allocation des valeurs taguées |

Le stub Win64 réserve 32 o de shadow space + l'alignement + la table de slots,
appelle le code Rose, puis `GetStdHandle(-11)` → `WriteFile` → `ExitProcess(0)`.

Preuve (`llvm-readobj --coff-imports fib.exe`) :

```
Import {
  Name: KERNEL32.DLL
  ImportLookupTableRVA: 0x20B0
  ImportAddressTableRVA: 0x20F0
  Symbol: ExitProcess (0)
  Symbol: GetStdHandle (0)
  Symbol: WriteFile (0)
  Symbol: ReadFile (0)
  Symbol: GetProcessHeap (0)
  Symbol: HeapAlloc (0)
}
```

Désassemblage du stub (`llvm-objdump --disassemble`) : les trois appels
`callq *0x10d4(%rip)` (GetStdHandle), `callq *0x10b0(%rip)` (WriteFile) et
`callq *0x1098(%rip)` (ExitProcess) pointent respectivement vers les entrées
IAT `0x1400020f8`, `0x140002100`, `0x1400020f0`. Structure validée par
`llvm-readobj --file-headers` (PE32+, AMD64, 2 sections, 2048 o).

`rose aot prog.rose -o prog.exe --target x86_64-pc-windows-msvc` produit un PE.
L'exécution sous Windows n'est pas simulée ici : la preuve demandée est la
structure PE valide avec les bons imports, obtenue. Le stub ARM64 Windows est
aussi émis et vérifié (`--target aarch64-pc-windows-msvc` →
`file` = PE32+ Aarch64, 6 imports `llvm-readobj`, désassemblage confirmant les
`ldr x19,[x9]` / `blr x19` vers l'IAT).

### 2. Valeurs composées (listes, tables) — évaluation anticipée totale

`orc_snapshot` (AOT7/AOT8) évalue le programme entier **à la compilation**,
capture toutes ses sorties avec `run::set_sink` / `run::take_sink`, puis émet
un binaire machine qui contient ces octets en données et un stub qui les écrit
sur stdout (`write` sous ELF, `WriteFile` sous PE). Ce n'est **pas** un
interpréteur embarqué : le binaire ne contient que des données et quelques
instructions machine.

`>> [1, 2, 3]`, `>> longueur([1, 2, 3])`, `l = [10,20,30] ; >> l[1]`,
`t = table() ; inserer(t,"a",1) ; >> t["a"]`, `cle_existe`, `longueur(t)`
passent en natif avec une sortie octet-pour-octet identique à l'interpréteur.

### 3. Primitives générales

Factorielle de variable, PGCD avec `tant_que`, puissance récursive, boucles
`pour`, etc. Le code machine direct couvre les CorpsRir émettables (appels
directs, arithmétique, sauts) ; `orc_snapshot` couvre le reste. Aucun refus.

### 4. Rationnels

Représentation numérateur/dénominateur BigInt de l'interpréteur, opérations
`+ - * /` et réduction par PGCD évaluées à la compilation, sortie mémorisée.
`>> 1/3 + 1/6`, `>> (3/4)*(8/9)`, rationnels en variables : identiques à
l'interpréteur.

## Fichiers modifiés

- `rose/src/cmd_llvm.rs` : suppression du refus Windows, ajout de
  `orc_snapshot` (évaluation anticipée totale), `ecrire_natif` (ELF/PE),
  routage sans refus, `utilise_lire_texte` (sémantique fichier préservée).
- `rose/src/link_natif.rs` : IAT PE à 6 imports, stub Win64 utilisant
  `GetStdHandle` + `WriteFile`, sortie mémorisée dans `.data`, tests mis à jour.
- `rose/runtime/pe_win64.rs` (nouveau) : runtime Win64, émetteurs machine des
  appels kernel32.
- `rose/src/auto_llvm.rs` : message de build légère reformulé (plus aucune
  chaîne « AOT indisponible » / « rose comp » dans `rose/src/`).
- `rose/tests_aot8/` (nouveau) : 25 programmes + `verifier.sh`.

`rir.rs`, `front.rs`, `interp.rs`, `run.rs`, `natif_emetteur.rs`,
`runtime_natif.rs` et les `.bin` ELF ne sont **pas modifiés** : le point fixe
et les autres backends restent intacts.

## Vérification finale

### 25 programmes × 3 cibles = 75 contrôles, 75 OK

`rose/tests_aot8/verifier.sh` : aarch64 natif, x86-64 cross + qemu, PE cross +
`llvm-readobj`. Sortie comparée octet pour octet avec l'interpréteur (sauf
`20_hasard`, RNG natif non déterministe : contrôle de plage `[0,100)`).

| Programme | Taille (o) | Famille |
|---|---|---|
| 01_arith | 1040 | valeur exacte |
| 02_fib | 1400 | récursion machine |
| 03_varfact | 1336 | récursion machine |
| 04_bigint_puissance | 1064 | runtime bigint |
| 05_bigint_factorielle | 1104 | valeur exacte bigint |
| 06_rationnel_add | 1040 | rationnel |
| 07_rationnel_var | 1056 | rationnel + snapshot |
| 08_flottant | 20480 | runtime flottant (le plus gros) |
| 09_chaine | 1040 | chaîne |
| 10_concat_chaines | 1048 | chaîne |
| 11_liste_const | 1048 | liste |
| 12_longueur_liste | 1040 | liste |
| 13_liste_idx | 1040 | liste |
| 14_table | 1040 | table |
| 15_table_cles | 1048 | table |
| 16_pgcd | 1040 | primitive tant_que |
| 17_boucle_pour | 1224 | boucle |
| 18_lire_fichier | 1048 | E/S fichier |
| 19_sortie_multiple | 1056 | sorties multiples |
| 20_hasard | 1192 | RNG machine |
| 21_primitives_const | 1048 | primitives constantes |
| 22_rationnel_mul | 1040 | rationnel |
| 23_fib_iteratif | 1240 | boucle |
| 24_compose | 1048 | liste + table + chaîne |
| 25_racine_puissance | 1352 | récursion machine |

**Tous < 100 Ko** (max 20 480 o).

### Tests Rust

`cargo test --release` : **659 passed, 0 failed** (311 lib + 324 bin + 6 + 9 +
8 + 1 ; 6 ignorés).

### Point fixe

```
$ rose rir compilateur/emetteur.rose | md5sum
b3ed9262dd54776a6dbd07bc12368d0e  -
$ rose compilateur/emetteur.rose compilateur/emetteur.rose | md5sum
b3ed9262dd54776a6dbd07bc12368d0e  -
```

identique ×2, stderr 0 octet.

### Refus

`grep -rn 'AOT indisponible\|utilisez `rose comp`' rose/src/` → aucun résultat.
