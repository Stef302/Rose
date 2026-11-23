# J6.7 — Assemblage du moteur complet SANS Rust (constat mesuré, 23/11/2026)

Sous-agent d'intégration. Worktree exclusif `/tmp/opencode/horde_integ`,
HEAD de départ `101b48d` (`PLAN : vague 92 — J1 à J6 intégrés`).
**Trois fichiers créés, aucun autre modifié, aucun commit, aucun push** :

| Fichier | Rôle | Taille |
|---|---|---|
| `compilateur/moteur_complet.rose` | généré, programme Rose autonome composé de 13 modules + point d'entrée unique | 10 001 lignes, 368 168 octets |
| `scripts/build_moteur_sans_rust.sh` | compose les modules et compile avec C0 | 9,3 Ko |
| `rose/docs/HORDE_INTEGRATION.md` | ce document | — |

Garde-fou utilisé sur TOUTE commande longue :
`bash scripts/garde_fou_mem.sh 12000000 <commande>`.

Résumé : **un ELF aarch64 autonome de 1 404 724 octets** a été produit en
5 min 26 s (pic 939 552 Ko) par le compilateur natif auto-hébergé C0, sans
appel au binaire moteur de référence ni au chaîne d'outils Rust. Il lit une
source Rose, la vérifie, la compile en RIR, exécute le RIR et affiche le
résultat. Sur 20 sources deterministes de `tests_selfhost/` : **19/20 (95 %)
sorties octet pour octet identiques** à la référence. Sur les 141 sources :
103 conformes, 31 divergentes, 7 bloquées.

---

## 1. Inventaire des modules (lecture seule)

`ls compilateur/*.rose` + `grep '^fn '` sur chaque fichier. Les modules
possédés par d'autres agents ont été lus dans leurs worktrees respectifs
(`/tmp/opencode/horde_*`) sans être modifiés.

| Module | Worktree | `fn` | Rôle |
|---|---|---:|---|
| `lexeur.rose` | integ | 7 | jetons au format `rose tokens` (redondant avec le front-end) |
| `analyseur.rose` | integ | 38 | AST au format `rose ast` (redondant) |
| `verifieur.rose` | integ | 79 | vérification complète, 219/219 fidélité |
| `emetteur.rose` | integ | 110 | source → dump RIR (`rose rir`) |
| `compilateur.rose` | integ | 123 | variante de l'émetteur (point fixe) |
| `emetteur_elf.rose` | integ | 175 | RIR → ELF G5 (c'est C0) |
| `emetteur_pe.rose` | integ | 94 | RIR → PE32+ G5 |
| `emetteur_pe_sel.rose` | integ | 98 | variante PE |
| `emetteur_cpp.rose` | integ | 32 | RIR → C++ |
| `emetteur_rust.rose` | integ | 32 | RIR → Rust |
| `emetteur_x64.rose` | integ | 53 | RIR → PE x86_64 |
| `rose_natif.rose` | integ | 393 | **composite généré** front-end + backend ELF |
| `rose_natif_pe.rose` | integ | 287 | composite front-end + backend PE |
| `ser_wasm.rose` | integ | 12 | sérialiseur WASM |
| `ser_bytecode.rose` | integ | 13 | sérialiseur ROSEBC v3 |
| `test_jit_g5.rose` | integ | 9 | mmap/mprotect/execve (hors portée runtime) |
| `tests_interpreteur_rir.rose` | integ | 3 | banc de test (non composé) |
| `interpreteur_rir.rose` | integ | 85 | **exécution du RIR** (J3, 157/158) |
| `moteur_valeurs.rose` | integ | 32 | valeurs taguées (J6) |
| `moteur_ops.rose` | integ | 50 | 20 opérations exactes (J6) |
| `lecteur_source.rose` | horde_src | 121 | **source → RIR** + imports récursifs (J6.5) |
| `bigint_rose.rose` | horde_bigint2 | 51 | bigint base 10⁹ + Karatsuba (J6.2) |
| `gc_rose.rose` | horde_gc | 34 | allocation / récupération en Rose pur |
| `tables_rose.rose` | horde_tables2 | 22 | tables à hachage persistantes (J6.4) |
| `chaine_rose.rose` | horde_rope | 31 | rope AVL immuable |
| `unicode_rose.rose` | horde_unicode | 23 | UTF-8 1..4 octets + NFC |
| `ffi_rose.rose` | horde_ffi | 52 | table de symboles natives |
| `ffi_syscalls.rose` | horde_ffi | 12 | horloge / fichiers |
| `repl_rose.rose` | horde_repl | 56 | console interactive (REPL) |
| `cli_rose.rose` | horde_cli | 408 (dont 15 `cli_*`) | couche CLI ; monolithe généré |

**Fonctions exportées retenues** (API réellement composée) :
`lsrc_lire_programme`/`lsrc_lire_programme_reg` (lecteur_source),
`parse_rir`/`call_fn`/`val_*`/`b*` (interpréteur),
`v_*`/`o_*`/`bz_*` (moteur de valeurs et opérations),
`bi_*` (bigint), `gc_*`, `t_*`, `cr_*`, `u_*`, `fx_*`/`ffi_*`, `sc_*`,
`rep_*`, `cli_*`, `sw_*`/`sb_*` (non retenus : déjà dans le composite C0).

---

## 2. Architecture de l'assemblage

Rose n'a **aucune** instruction `include` : un module est un fichier source
concaténé. `scripts/build_moteur_sans_rust.sh` fait la composition en
lecture seule, puis appelle **un seul** outil, le compilateur natif
auto-hébergé C0 (`ELF aarch64`, md5 `7bb091200cf927022eb9d86a548d2502`).

Ordre dans `moteur_complet.rose` :

1. `lecteur_source.rose` — **préfixé `lsrc_`** (121 `fn` renommées :
   définitions **et** appels). Raison : ses 110 fonctions de front-end
   portent exactement les mêmes noms que celles de `emetteur.rose`, et 74
   noms sont partagés avec `verifieur.rose`. Son `main` autonome est retiré.
2. `interpreteur_rir.rose` — globales + 85 `fn`, `main` retiré. Aucune
   collision avec (1) : seul `eval_expr` est commun à l'ensemble
   `rose_natif.rose`, mais il n'est pas dans la portion composée.
3. Modules J6 et horde : `moteur_valeurs`, `moteur_ops`, `bigint_rose`,
   `gc_rose`, `tables_rose`, `chaine_rose`, `unicode_rose`, `ffi_rose`,
   `ffi_syscalls`, `repl_rose` (son `main` est retiré). Zéro collision
   mesurée entre ces préfixes (`v_`, `o_`, `bi_`, `gc_`, `t_`, `cr_`, `u_`,
   `fx_`, `sc_`, `rep_`).
4. `cli_rose.rose` : module **monolithique généré** de 603 Ko — 393 de ses
  408 fonctions dupliquent le front-end et le backend déjà présents dans
   C0 et dans (1). Seules les **15 fonctions `cli_*`** sont importées,
   `cli_rir_du` étant écarté (double de `lsrc_lire_programme`).
5. **Point d'entrée unique** (`moteur_complet.rose:9982`) :
   `lsrc_lire_programme(chemin)` → `OK|<rir>` / `ERR|diagnostic` ;
   `parse_rir(rir)` ; `call_fn(mem, "$principal", args)` ; affichage par
   la primitive `afficher` de l'interpréteur.

`emetteur_elf.rose`, `emetteur_pe*`, `emetteur_cpp/rust/x64`,
`rose_natif*.rose`, `test_jit_g5.rose` et les 4 sérialiseurs ne sont PAS
liés dans l'ELF : ce sont des **backends de compilation**, déjà incarnés par
C0 lui-même ; les lier dupliquerait 393 définitions et dépasserait la borne
des 512 globales G5. `test_jit_g5.rose` définit en outre `octet_brut`, en
collision avec `unicode_rose.rose`.

Le script ne contient **aucun** appel au binaire moteur de référence ni à
l'outil Rust (vérifié : `grep -nE 'cargo|rose/rose' script` → aucun résultat).

---

## 3. Preuve de compilation

```
cd /tmp/opencode/horde_integ
bash scripts/garde_fou_mem.sh 12000000 bash scripts/build_moteur_sans_rust.sh \
  --compilateur /tmp/opencode/j6_agents/C0_j6.elf \
  --sortie /tmp/opencode/horde_integ_art/final
```

Résultat mesuré (`/usr/bin/time -v`) :

| Grandeur | Valeur |
|---|---|
| Source composée | 10 001 lignes, 368 168 octets, `vérifié` |
| Temps | 5 min 25,91 s |
| Pic RSS | 939 552 Ko (garde-fou 12 000 000 Ko, jamais atteint) |
| ELF | `moteur_complet.elf`, 1 404 724 octets, magic `7f454c46`, machine `183` (aarch64) |
| md5 ELF | `daf6a8e83046f78faf7f34a1455477ca` |
| md5 C0 | `7bb091200cf927022eb9d86a548d2502` |

---

## 4. Taux réel — 20 sources deterministes

Jeu fixé par `ls tests_selfhost/*.rose | sort | head -20` (reproductible) :
`g5b501` … `g5b560` (20 fichiers). Référence :
`ROSE_SANS_JIT=1 rose/rose <source>`. Comparaison `cmp -s` octet pour octet.

| Résultat | Nombre |
|---|---:|
| **CONFORME** | **19 / 20 (95 %)** |
| DIVERGENT | 1 / 20 (`g5b521_puissance_bases_foldables`) |
| BLOQUE | 0 |

Les 19 conformes couvrent : Unicode latin/grec/cyrillique/CJK/astral,
rationnels positifs/négatifs/somme/boucle, puissances de 2 au-delà de 2⁶⁴,
encodage de puissances, soustraction/division/modulo/adding bigints,
valeurs absolues et min/max big, signes opposés.

**Divergence exacte** (`g5b521`) :

| Expression | Référence | ELF |
|---|---|---|
| `3^41` | 36472996377170786403 | 155968982055106659 |
| `10^21` | 1000000000000000000000 | 128825129711960064 |
| `5^28` | 37252902984619140625 | 71184461048325649 |
| `4^33` | 73786976294838206464 | 0 |
| `(10^10)^2` | 100000000000000000000 | 272289851507736576 |

Cause : le repli de constantes `lsrc_rpui` (`moteur_complet.rose:551`,
source `lecteur_source.rose:532`) calcule `b^e` dans un `entier` G5 tagué
(64 bits, immédiats ≤ 2⁶⁰). Les puissances de 2 passent parce qu'elles ont
un chemin d'encodage dédié (g5b520/g5b522 conformes) ; les autres bases
débordent et sont tronquées **avant** l'exécution. Le module `bigint_rose.rose`
fournit bien `bi_pow`, mais il n'est pas branché dans le repli de constantes.

### Bilan large (141 sources, même ELF)

103 conformes, 31 divergentes, 7 bloquées. Répartition des 7 bloquées :
4 sont rejetées **aussi** par la référence (`t104`, `t105`, `t106`, `t107`,
`t55` : surcharge arithmétique/arité, paramètres fonction, liste imbriquée) ;
2 sont des refus réels de notre ELF (`t14_type`, `t91_prng`) ; 1 est un
programme à arguments (`t999_critique_g5`).

---

## 5. Validation

Exécutées dans `/tmp/opencode/horde_integ`, sous garde-fou :

| Contrôle | Commande | Résultat |
|---|---|---|
| Vérification du composé | `rose/rose verif compilateur/moteur_complet.rose` | `vérifié` (rc=0) |
| Point fixe | `rose/rose rir compilateur/emetteur.rose \| md5sum` | `b3ed9262dd54776a6dbd07bc12368d0e`, identique sur deux passes |
| Validation finale | `RUST_TEST_THREADS=1 bash scripts/validation_finale.sh` | **RC=0, 0 échec** : 641 tests (314+327+0), point fixe, AOT 75/75, JIT 25/25, WASM 92 o, RVM 317811, AZ 31 conformes, HOST-1 fib=317811 |

Le premier passage AZ avait échoué (31 `BLOQUE`) uniquement parce que
`rose/rose` n'existait pas dans le worktree (le banc utilise ce chemin
codé en dur) ; après `cp rose/target/release/rose rose/rose`, 31/31.

---

## 6. Ce qui reste EXACTEMENT pour remplacer le binaire de référence

Chaque ligne est une cause technique localisée, pas une estimation.

1. **Repli de constantes 64 bits** — `lsrc_rpui` (`moteur_complet.rose:551`).
   `b^e` et les littéraux > 2⁶⁰ sont calculés dans un `entier` tagué G5.
   Conséquence mesurée : `3^41` → 155968982055106659 au lieu de
   36472996377170786403 ; `t20_add` `-5+12` → 288230376151711751 ;
   `t220`/`t221`/`t222`/`t223` (dépassements 2⁵⁹..2⁶⁴) divergents.
   Correctif requis : typer les littéraux repliés comme chaîne décimale et
   brancher `bi_pow`/`bi_add` du module `bigint_rose` dans le repli.
2. **Arithmétique rationnelle négative** — `vf` (`interpreteur_rir.rose:574`)
   et `val_str` (`:482`). `-7/3` rend un entier 64 bits tronqué puis une
   décimale de plusieurs milliers de chiffres au lieu de `-3.[3]`
   (`t07`, `g5b606`, `t242`, `t243`). Cause : la normalisation de signe du
   numérateur et la représentation `r:<num>/<den>` ne sont pas appliquées
   quand le littéral est replié en négatif.
3. **Flottants** — `vf` (`:574`), `eval_expr` (`:922`). `1.5` s'affiche `1`
   (`t09`) : le repli de constante flottante arrondit à l'entier, et
   `val_str` n'a pas de formatage décimal IEEE 754 (ni exposition
   scientifique de `approx`).
4. **Alias primitifs absents du `std_sig` du lecteur** —
   `lecteur_source.rose:1992`. `type` et `prng` sont refusés
   (`Fonction 'type'/'prng' inconnue`, `t14`, `t91`) : le composite
   `rose_natif.rose` les injecte dans sa copie, `lecteur_source.rose`
   non. Correctif : injection dans la copie générée (comme le fait
   `scripts/generer_rose_natif.py`) + implémentation RIR des primitives.
5. **Séparation des arguments** — point d'entrée (`moteur_complet.rose:9982`).
   Le chemin source occupe `argument(0)`, donc `>> argument(0)` dans le
   programme utilisateur rend le chemin au lieu d'échouer (`t81`, `t82`,
   `t999`). Correctif : transmettre les arguments utilisateurs à partir de
   l'index 1 dans `call_fn`, ou retirer le chemin de la table d'arguments.
6. **RIR non couvert par l'interpréteur** — `eval_expr`/`call_prim`
   (`interpreteur_rir.rose:830`, `:922`). Les formes `appel_rir`, les
   surcharges aritaires/types et les paramètres-fonctions ne sont pas
   exécutées (J3 : 157/158). Les programmes correspondants sont refusés
   par les deux moteurs (`t104`–`t107`) : il faut d'abord étendre le
   front-end, puis l'interpréteur.
7. **Pas de JIT Cranelift ni d'AOT LLVM** — le binaire G5 est un émetteur
   statique ; `test_jit_g5.rose` ne fait qu'écrire un ELF à la volée via
   `mmap`/`mprotect`/`execve` (fib(10)=55). Aucun compilateur à la volée
   n'est présent dans l'ELF assemblé.
8. **Allocateur sans récupération effective** — `gc_rose.rose` fournit
   34 fonctions de comptage/pool en Rose pur, mais l'ABI G5 sous-jacente
   (`g5_alloc_inline`) reste un tas montant : aucune intégration de `gc_*`
   dans l'émetteur n'est faite. Les sources longues peuvent épuiser la
   mémoire (mesure antérieure : 84 Gio sur `rose_natif.rose`).
9. **Chaînes, tables, bigint et Unicode non branchés dans le runtime** —
   `chaine_rose.rose` (rope AVL), `tables_rose.rose` (hachage),
   `bigint_rose.rose` (Karatsuba) et `unicode_rose.rose` sont composés
   (leurs fonctions sont présentes dans l'ELF) mais les primitives que
   l'interpréteur appelle utilisent encore ses propres listes plates
   (`interpreteur_rir.rose:610` `tok_line`, `:830` `call_prim`). Le
   branchement est un travail d'adaptation d'API, pas une réécriture.
10. **CLI complète** — `cli_rose.rose` fournit 15 fonctions de textes
    (`cli_banniere`, `cli_aide`, `cli_info`, `cli_aide_pour`, …) ; le
    dispatch (`--version`, `help`, `comp`, `rir`, `latex`, …) n'est pas
    câblé dans le point d'entrée unique, volontairement limité à
    « lire / vérifier / compiler / exécuter un fichier ».
11. **REPL non démarré** — `repl_rose.rose` est composé (56 `fn rep_*`,
    moteur de valeurs propre), mais la boucle n'est pas lancée : le
    point d'entrée exécute un fichier puis se termine.
12. **FFI réelle** — `ffi_rose.rose`/`ffi_syscalls.rose` exposent une table
    de symboles et des appels système, mais pas de chargement dynamique
    de bibliothèques C (`dlopen`/`dlsym`) : `@dll`/`@sym` restent hors ABI.

Aucune de ces causes n'est une « limite honnête » : ce sont des écarts
localisés, mesurés, avec un correctif concret. Le remplacement complet
nécessite surtout (1), (2), (3), (4), (5) pour la parité sur le corpus
`tests_selfhost/`, puis (7)–(9) pour la parité de performances et de
robustesse.

---

## 7. Reproductibilité

```
cd /tmp/opencode/horde_integ
# composition seule (aucun compilateur appelé)
bash scripts/build_moteur_sans_rust.sh --source-only
# vérification du composé
rose/rose verif compilateur/moteur_complet.rose
# ELF autonome
bash scripts/garde_fou_mem.sh 12000000 bash scripts/build_moteur_sans_rust.sh \
  --compilateur /tmp/opencode/j6_agents/C0_j6.elf \
  --sortie /tmp/opencode/horde_integ_art/final
# 20 sources
for f in $(ls tests_selfhost/*.rose | sort | head -20); do
  ./art/final/moteur_complet.elf "$f" > /tmp/a.out
  ROSE_SANS_JIT=1 rose/rose "$f" > /tmp/b.out
  cmp -s /tmp/a.out /tmp/b.out && echo "OK $f" || echo "KO $f"
done
```

Artefacts hors dépôt : `/tmp/opencode/horde_integ_art/final/moteur_complet.elf`,
`/tmp/opencode/horde_integ_art/rate20/resultats.tsv`,
`/tmp/opencode/horde_integ_art/all/resultats.tsv`,
`/tmp/opencode/horde_integ_art/validation_finale2.log`.

`git status --short` en fin de mission :
`?? compilateur/moteur_complet.rose`, `?? scripts/build_moteur_sans_rust.sh`,
`?? rose/docs/HORDE_INTEGRATION.md` (aucun module d'un autre agent touché).
