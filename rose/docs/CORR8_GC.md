# CORR8-GC — branchement de `gc_rose.rose` dans l'allocateur G5 (23/11/2026)

Point 8 du chantier de correction. Worktree exclusif `/tmp/opencode/corr_b8`,
HEAD `d117e04eab2a7b9ef391123d575311820908ee2e`. **Aucun commit, aucun push.**

Fichiers possédés et modifiés :

| fichier | rôle |
|---|---|
| `compilateur/emetteur_elf.rose` | variante GC branchée sur `g5_alloc_inline`, routines G5 émises, dispatch |
| `compilateur/tests_b8.rose` | batterie fonctionnelle ELF (nouveau) |
| `rose/docs/CORR8_GC.md` | ce document (nouveau) |

`compilateur/emetteur.rose`, `compilateur/rose_natif.rose` et `rose/src/**`
n'ont **pas** été touchés.

## 1. État de départ

`g5_alloc_inline()` (`emetteur_elf.rose:975`) est un allocationneur bump pur :

```
ldr x2,[x9]      ; x9 = 0x60000000, pointeur libre
orr x1,xzr,x0    ; taille demandée
orr x0,xzr,x2    ; pointeur rendu
add x2,x2,x1
str x2,[x9]      ; le pointeur ne fait que MONTER
```

Le module `compilateur/gc_rose.rose` fournit 35 fonctions `gc_*` (arena +
pool + checkpoints) en Rose pur, mais **aucune n'était appelée** par le
backend : le tas montant restait sans récupération. Le but ici est la
**récupération effective dans un ELF C0**, mesurée, pas l'ajout d'une API
supplémentaire.

## 2. Ce qui a été branché

Le code de `g5_alloc_inline` est conservé **tel quel** pour le chemin normal
(les ELF existants gardent leur sémantique), et la logique de `gc_rose.rose`
est réimplémentée dans cinq routines G5 **émises** dans l'ELF :

| routine G5 | équivalent `gc_rose.rose` | effet machine |
|---|---|---|
| `gc_allouer(n)` | `gc_allouer(m, valeur)` | dépile la file si non vide, sinon appelle `g5_alloc_inline()` ; compte `n_alloc`/`n_reuse` ; écrit un mot dans le bloc (page réellement touchée) |
| `gc_liberer(p)` | `gc_liberer(m, index)` via `gc_set(pool,queue,p)` | `pousser p` dans la file : `[p] = tête`, `tête = p` |
| `gc_sauvegarde()` | `gc_save(m)` | checkpoint = `[0x60000000]` (4 entiers en Rose, **1 mot** ici), `n_save++` |
| `gc_restauration(cp)` | `gc_restore(m, cp)` | `gc_set(libre, cp)` = récupération de **tous** les blocs de la phase ; file vidée, `n_restore++` |
| `gc_compteur(i)` | `gc_allocs`/`gc_reutilises`/`gc_saves`/`gc_restores` | 0=`n_alloc`, 1=`n_reuse`, 2=`n_save`, 3=`n_restore` (cumulatifs, jamais rembobinés) |

Cartographie RW complémentaire, dans la plage libre
`0x60005000..0x60008000` (après les 512 globales, avant le début du tas) :

```
0x60005000  checkpoint (pointeur tas brut)
0x60005008  tête de la file de blocs libres (0 = vide)
0x60005010  n_alloc        0x60005018  n_reuse
0x60005020  n_save         0x60005028  n_restore
```

Points d'insertion dans `emetteur_elf.rose` :

- `861-869` : variables G5 des cinq routines ;
- `994-1118` : `gc_meta()` + les cinq `g5_emit_gc_*` ; `g5_emit_gc_alloue`
  appelle explicitement `g5_alloc_inline()` sur le chemin bump
  (`l.1086`) — c'est le branchement demandé dans l'allocateur ;
- `2181-2189` : dispatch par nom (`gc_allouer`, `gc_liberer`,
  `gc_sauvegarde`, `gc_restauration`, `gc_compteur`) **avant** la recherche
  de fonction utilisateur, comme `mmap` : des stubs Rose rendent les noms
  acceptables par le front-end, l'émetteur leur substitue les routines ;
- `4872-4881` : réservation des étiquettes ; `4919-4923` : émission des
  routines dans l'ELF.

Sémantique de `gc_restauration` : le pointeur libre est **ramené** au
checkpoint. C'est plus fort que le `gc_restore` de `gc_rose.rose` (qui
garde le high-water et rend les cases au pool) : ici les blocs de la phase
sont récupérés en une seule écriture. La file est vidée en même temps pour
qu'un bloc déjà libéré pendant la phase ne soit pas réutilisé deux fois.

## 3. Batterie fonctionnelle

`compilateur/tests_b8.rose` : 11 points de contrôle, tous exécutés dans un
ELF C0 (`quitter(11..20)` distingue les échecs). Chaîne réellement exécutée :

```
rose verif compilateur/tests_b8.rose        -> vérifié, rc=0
rose rir compilateur/tests_b8.rose          -> tests_b8.rir
# le dump contient un `$principal` vide ajouté par le front-end : on échange
# les noms des deux fonctions `principal` et `$principal` dans le RIR pour
# que le corps de test soit le point d'entrée (procédure G5 standard).
rose compilateur/emetteur_elf.rose tests_b8_swap.rir > tests_b8s.elf
chmod +x tests_b8s.elf && ./tests_b8s.elf   -> rc=0
```

ELF : 16 784 octets, md5 `a5e70062b35398b9068530937dca1408`.
Couverture : bump (pas de 64), libération + réutilisation FIFO, checkpoint /
restore, 500 phases × 32 blocs (même adresse réutilisée à chaque phase),
compteurs (`n_alloc=16013`, `n_reuse=1`, `n_save=1`, `n_restore=1` au point
de contrôle), vidage de la file à la restauration.

## 4. Mesures de pic RSS — avant / après

Bancs générés dans `/tmp/opencode/corr_b8_probe/` par `mkbench.sh`, ELF émis
par `emetteur_elf.rose` modifié, **même routine `gc_allouer(64)`** dans les
deux cas :

- **naïf** = aucune restauration (comportement du tas montant d'origine) ;
- **gc** = checkpoint avant chaque phase de 250 allocations, restauration
  après (`4000` phases à 1 M).

Protocole : `scripts/garde_fou_mem.sh 12000000 bash scripts/prof_mem_rose.sh
-n 3 -- <ELF>` (3 exécutions, médianes, `time -v`, garde-fou 12 Gio).

| banc | allocations | pic RSS médian (3 runs) | temps médian | rc |
|---|---:|---:|---:|---:|
| naïf | 100 000 | **6 240 Ko** | 5,05 s | 0 |
| naïf | 1 000 000 | **62 400 Ko** | 5,04 s | 0 |
| **gc** | 100 000 | **3 328 Ko** | 0,00 s | 0 |
| **gc** | 1 000 000 | **3 672 Ko** | 5,01 s | 0 |

Faits mesurés :

- facteur naïf 100k → 1M = **10,0×** (croissance linéaire du tas touché) ;
- facteur GC 100k → 1M = **1,10×** : aucune croissance significative ;
- à 1 M, le GC consomme **17,0× moins** de pic RSS que le banc naïf
  (−94,1 %) ;
- les 12 exécutions rendent `rc=0`, swap libre constant à 11 179 144 Ko,
  garde-fou jamais déclenché ;
- rapports bruts : `/tmp/opencode/corr_b8_probe/prof_{naif,gc}_{100000,1000000}.txt`,
  journal `/tmp/opencode/prof_mem_rose_journal.tsv` ;
- ELF : `bench_naif_100000.elf` (10 592 o, md5 `f1eefdbf…`),
  `bench_naif_1000000.elf` (10 592 o, `2cdf488e…`),
  `bench_gc_100000.elf` (10 944 o, `0bb8a52c…`),
  `bench_gc_1000000.elf` (10 944 o, `2a6e718e…`).

## 5. Validation (preuves d'exécution)

| contrôle | commande | résultat |
|---|---|---|
| vérification syntaxique | `rose verif compilateur/emetteur_elf.rose` | `vérifié`, rc=0 |
| vérification batterie | `rose verif compilateur/tests_b8.rose` | `vérifié`, rc=0 |
| point fixe (1) | `rose rir compilateur/emetteur.rose \| md5sum` | `b3ed9262dd54776a6dbd07bc12368d0e` |
| point fixe (2) | `rose compilateur/emetteur.rose compilateur/emetteur.rose \| md5sum` | `b3ed9262dd54776a6dbd07bc12368d0e` |
| validation finale | `RUST_TEST_THREADS=1 bash scripts/garde_fou_mem.sh 12000000 bash scripts/validation_finale.sh` | **RC=0, 0 échec** (314 + 327 tests Rust, AOT 75/75, JIT 25/25, WASM 92 o, RVM 317811, AZ 31, HOST-1 317811) |
| batterie AZ isolée | `AZ_OUT=/tmp/opencode/corr_b8_az bash tests_selfhost/lancer_az.sh` | **31 CONFORME, 0 divergent, 0 bloqué** ; 4 comparaisons × 31/31 |
| sonde idx UTF-8 | `rose emer_base.rose /tmp/opencode/probe_idx.rir` vs `rose emetteur_elf.rose …` | sorties `cmp`=0 (39 o), rc=0/0 |
| sonde u16le (`e_machine`) | `xxd -s 18 -l 2` sur les deux ELF | `b7 00` dans les deux |

Journal complet : `/tmp/opencode/corr_b8_probe/validation_finale.out`,
`/tmp/opencode/corr_b8_probe/az.log`. Le binaire `rose/rose` (ignoré par
git) a été recopié depuis le dépôt principal pour que `validation_finale.sh`
et la batterie AZ s'exécutent dans le worktree ; `rose/target` est un
lien symbolique vers le `target` déjà compilé (aucun fichier source modifié
hors périmètre).

## 6. État des lieux

- **Fait** : le tas G5 a maintenant une voie de récupération réelle
  (file de blocs + checkpoint de phase), branchée sur `g5_alloc_inline`.
- **Vérifié** : 11 assertions ELF, point fixe, `verif`, validation finale
  0 échec, AZ 31/31, sondes idx/u16le inchangées.
- **Mesuré** : −94,1 % de pic RSS à 1 M allocations ; stabilité du GC
  entre 100 k et 1 M (facteur 1,10 contre 10,0 sans reprise).
- **Reste hors périmètre** : l'appel automatique des checkpoints depuis
  le compilateur lui-même (`rose_natif.rose` / `emetteur.rose`) demanderait
  de modifier ces fichiers, explicitement interdits ici ; le branchement
  est donc disponible pour tout ELF C0 qui utilise les primitives
  `gc_*`.
