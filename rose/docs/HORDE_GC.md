# HORDE-GC — allocateur avec récupération écrit en Rose (23/11/2026)

Poste « GC / gestion mémoire » du plan J6 (`rose/docs/J6_MOTEUR_ROSE.md` §5,
ligne 278). Worktree `/tmp/opencode/horde_gc`, HEAD
`101b48d25a85a3a4a5be80232af9dda7bff037f1`. **Aucun commit, aucun push.**
Fichiers possédés, et eux seuls :

| fichier | rôle | lignes |
|---|---|---:|
| `compilateur/gc_rose.rose` | allocateur arena + pool + compteurs (module source Rose) | 243 |
| `compilateur/tests_gc_rose.rose` | batterie fonctionnelle (122 assertions) | 162 |
| `rose/docs/HORDE_GC.md` | ce document | — |

`git status --short` ne montre que ces deux fichiers `.rose` (le binaire
`rose/rose`, ignoré par git, a été recopié localement pour que la batterie AZ
de `validation_finale.sh` puisse s'exécuter dans un worktree — voir §6).

## 1. Causes mesurées au départ

- `rose/docs/MEM_OPT.md` : C0_final2.elf monte à **84 Gio de RSS en 27 min**
  sur `rose_natif.rose` ; croissance superlinéaire, seuil pratique ~100 Ko de
  source ; allocateur tas montant **sans libération**.
- `rose/docs/G5_MEMOIRE.md` : le tas RW de l'ELF G5 (`g5_alloc_inline`,
  `[0x60000000]`) ne libère jamais ; 31,3 Gio sur `emetteur.rir` avant deux
  correctifs algorithmiques, 456 Mio après.
- `rose/docs/MEM_HOST.md` : le chemin hébergé `Rc` garde une RSS bornée
  (~55 Mio sur `noyau.rose`) mais le goulot mesuré est `sous_chaine` →
  `aplatir()` O(n) par tour, 9,6 Gio d'octets copiés après optimisation.
- `rose/docs/G5_ABI_VALEURS.md` §5 : tas 64 Mio (puis 512 Mio) épuisé en
  ~1,3 s par une boucle d'allocation — **ajouter de la mémoire ne recule que
  le mur**, la cause est algorithmique.

Donc : le remède n'est pas un plus gros tas, c'est une **politique
d'allocation réutilisatrice** dans le moteur lui-même, sans `malloc` Rust.

## 2. Modèle et API

L'état est une **table Rose** de petites clés entières, mutable en place :

| clé | contenu |
|---:|---|
| 0 | `arena` : table index de case → valeur entière |
| 1 | `libre` : pointeur libre (plus haut index jamais créé, *high-water mark*) |
| 2 | `pool` : table position → index de descripteur récupérable |
| 3 | `tete` : première position non consommée du pool |
| 4 | `queue` : première position libre du pool |
| 5 / 6 / 7 | `n_alloc` / `n_libere` / `n_reuse` (cumulatifs) |
| 8 / 9 | `pic` / `vivants` (descripteurs simultanés) |
| 10 / 11 | `n_save` / `n_restore` |
| 12 | `n_fresh` : cases réellement créées dans l'arena |

API publique (toutes les fonctions sont en Rose, sous-ensemble G5) :

| fonction | effet |
|---|---|
| `gc_nouveau() -> table` | crée l'état (14 clés initialisées à 0, deux tables vides) |
| `gc_allouer(m, valeur) -> entier` | prend au pool si possible, sinon bump ; stocke `valeur` ; rend l'index |
| `gc_liberer(m, index)` | remet `index` dans le pool, décrémente `vivants` |
| `gc_save(m) -> liste` | checkpoint = `[libre, tete, queue, vivants]`, **sans rien copier** |
| `gc_restore(m, cp)` | rend au pool `[libre_sauve, libre)`, rouvre le pool consommé, restaure `vivants` |
| `gc_lire(m, index)` / `gc_ecrire(m, index, valeur)` | accès à la case |
| `gc_allocs`, `gc_liberes`, `gc_reutilises`, `gc_frais`, `gc_pic_vivants`, `gc_nb_vivants`, `gc_saves`, `gc_restores`, `gc_libre`, `gc_tete`, `gc_queue`, `gc_compteurs` | instruments de mesure, **jamais rembobinés** |

Sémantique du checkpoint, précisée ici parce qu'elle est non évidente :
`gc_restore` **ne rembobine pas le pointeur libre**. Le rembobiner ferait
réapparaître comme neuves des cases déjà rendues au pool (collisions d'index
dès la phase suivante). `libre` reste donc au plus haut atteint ; le
checkpoint sert uniquement à délimiter la plage d'index à récupérer. C'est la
règle standard d'un allocateur bump + free-list. Les libérations faites dans
une phase sont annulées par le restore puisque leurs cases sont récupérées en
bloc.

Le pool est une file circulaire **logique** stockée dans une table à clés
entières : `tete`/`queue` se déplacent, les-written cases sont écrasées par
`gc_set`, donc aucune table n'est jamais agrandie après le pic de vivants.
Le coût d'une écrase est O(taille du pool) — acceptable ici parce que le pool
est dimensionné par le **pic de vivants**, pas par le nombre total
d'allocations.

## 3. Contraintes G5 découvertes et contournées (mesurées, pas supposées)

Ces quatre points ont été isolés par sondes C0 pendant ce chantier ; ils ne
sont pas des « limites honnêtes », ce sont des faits de backend avec
correctif appliqué dans `gc_rose.rose` :

1. **`inserer(t, cle, v)` sur une clé existante n'écrase pas.** `g5_emit_inser`
   (emetteur_elf.rose:3196) ajoute toujours une paire ; `t[cle]` rend la
   *première* occurrence, donc l'ancienne valeur. Sonde : `init(t,1,5)` puis
   `ecrire(t,1,99)` → lecture `5` sous l'ELF (`6` lectures sur 6 fausses).
   **Correctif** : `gc_set(t,cle,v) = supprimer(t,cle); inserer(t,cle,v)`.
   Mesure après correctif : `OK init=5`, `OK update=99`.
2. **Lecture de table = type `?` pour le vérificateur** (`front.rs:2123`).
   `retour m[cle]` avec un type déclaré `-> entier` est refusé ; les
   accesseurs de l'état sont donc déclarés **sans annotation de retour** et
   les listes de checkpoint sont construites par `joindre` sur une liste
   homogène `?`.
3. **Deux motifs sont silencieusement perdus par le backend C0** : `<` entre
   deux lectures de table (`$tete < $queue`) et soustraction sur un `?`
   (`m[9] - 1`). Le premier produit un ELF muet (55 o) ; le second aussi.
   **Correctifs** : `$tete != $queue` (équivalent, car `tete ≤ queue`) et
   `m[9] + (0 - 1)`. Après quoi l'ELF complet est émis (16 636 o pour le
   module seul).
4. **`afficher`/`>>` n'émettent pas les valeurs `?`** : `"v=" + $t[1]` rend
   `v=` (vide) sous l'ELF. Les tests n'impriment donc que des littéraux de
   chaine et vérifient les valeurs par comparaison + `quitter(1)`.

## 4. Tests fonctionnels — résultats réels

Chaîne exécutée (C0 = `C0_j6.elf`, md5
`7bb091200cf927022eb9d86a548d2502` : le compilateur C0 figé de la vague J6,
décrit en `J6_MOTEUR_ROSE.md` §3.1 et réutilisé depuis
`/tmp/opencode/j6_agents/`. Une tentative de reconstruction depuis ce worktree
a produit un RIR différent — 903 626 o contre 686 314 o — et un ELF inopérant
(`index hors des limites des arguments`) : il n'a pas été utilisé.) :

```
cat compilateur/gc_rose.rose compilateur/tests_gc_rose.rose > tests_gc.rose
rose verif tests_gc.rose                 -> vérifié (rc=0)
ROSE_SANS_JIT=1 rose tests_gc.rose       -> rc=0, 122 lignes "OK …",
                                            dernière = "TOUS LES TESTS GC SONT VERTS"
C0_j6.elf tests_gc.rose > tests_gc.elf   -> rc=0, 47 144 o, magic 7f454c46
./tests_gc.elf                           -> rc=0, 122 lignes "OK …"
diff nat.out elf.out                     -> rc=0 (octet pour octet)
```

Couverture des 8 groupes de tests : état initial ; 10 allocations contiguës +
pic + compteurs ; écriture d'une case existante sans bump ; libération puis
réutilisation FIFO ; checkpoint/restore puis 50 réutilisations ; checkpoints
embriqués LIFO ; libérations annulées par restore ; 500 phases × 32
allocations (borne `libre=32`, `pic=32`, `fresh=32`, `reuse=15968`).

Validation supplémentaire du banc 1 M (ELF, assertions incluses) :
`OK 1M allocations`, `OK 250 cases creees`, `OK 999750 reuses`,
`OK libre=250`, `OK 4000 saves`, `OK 4000 restores` — donc les 1 000 000
d'allocations du mesurage sont bien exécutées, pas optimisées away.

## 5. Mesures de pic RSS — avant / après GC

Protocole : `scripts/prof_mem_rose.sh -n 3 -- <ELF>` (garde-fou 16 Gio,
3 exécutions, médianes, `time -v`), exécuté **toujours** sous
`scripts/garde_fou_mem.sh 12000000`. Sources des bancs générées dans
`/tmp/opencode/horde_gc_probe/` ; ELF émis par `C0_j6.elf`.

| banc | allocations | pic RSS médian (3 runs) | temps médian | swap libre min |
|---|---:|---:|---:|---:|
| naïf (table qui ne fait que grandir) | 100 000 | **3 664 Ko** | 5,03 s | 10,08 Gio |
| naïf | 1 000 000 | **32 000 Ko** | 5,02 s | 10,08 Gio |
| **gc_rose** (phases de 250 + checkpoints) | 100 000 | **3 480 Ko** | 5,01 s | 10,09 Gio |
| **gc_rose** | 1 000 000 | **3 412 Ko** | 5,01 s | 10,09 Gio |

Faits :

- facteur naïf 100k → 1M = **8,73×** : la croissance est bien linéaire (la
  table passe de ~100 000 à ~1 000 000 de paires, avec doublements successifs
  laissés sur le tas bump) ;
- facteur GC 100k → 1M = **0,98×** : **aucune croissance mesurable**, objectif
  ≤ 2× largement atteint ; l'écart de −68 Ko est du bruit (les 3 runs vont de
  3 376 à 3 484 Ko) ;
- à 1 M, le GC consomme **10,6× moins** de pic RSS que le banc naïf ;
- `rc=0` sur les 12 exécutions, swap libre constant (canary intact), garde-fou
  jamais déclenché ;
- le temps médian (~5,01 s) est dominé par la latence de polling de 5 s du
  garde-fou (`MEM_OPT.md` §2) : le temps ELF réel mesuré à part est 0,02 s
  (naïf 1 M) et 1,26 s (GC 1 M), le surcoût du GC étant le `supprimer`+
  `inserer` de `gc_set` sur l'arena et le pool.

Rapports bruts : `/tmp/opencode/horde_gc_probe/prof_{naive,gc}_{100000,1000000}.txt`,
journal `/tmp/opencode/prof_mem_rose_journal.tsv`.

## 6. Validation finale

| contrôle | résultat |
|---|---|
| `rose verif compilateur/gc_rose.rose` | `vérifié`, rc=0 |
| `rose verif` sur la concaténation + pilote | `vérifié`, rc=0 |
| C0 → ELF (module seul) | rc=0, 16 636 o, magic ELF |
| C0 → ELF (batterie) | rc=0, 47 144 o, exécution rc=0 |
| sortie native vs sortie ELF | `diff` rc=0, 0 différence |
| point fixe `rose rir compilateur/emetteur.rose \| md5sum` | `b3ed9262dd54776a6dbd07bc12368d0e` |
| point fixe `rose compilateur/emetteur.rose compilateur/emetteur.rose \| md5sum` | `b3ed9262dd54776a6dbd07bc12368d0e` |
| `RUST_TEST_THREADS=1 bash scripts/garde_fou_mem.sh 12000000 bash scripts/validation_finale.sh` | **RC=0, 0 échec** |
| └ tests Rust | 314 + 327 + 0passed, **0 failed**, 6 ignored |
| └ AOT / JIT | 75/75, 25/25 |
| └ WASM / RVM / AZ / HOST-1 | 92 o / 317811 / 31 CONFORME, 0 non-conforme / fib=317811 |

Journal complet : `/tmp/opencode/horde_gc_probe/validation_finale2.out`
(première passe, `validation_finale.out`, avait 1 échec AZ : `lancer_az.sh`
appelle `$ROOT/rose/rose`, absent dans tout worktree neuf — 31 programs
`BLOQUE rc=127`. Après copie du binaire compilé vers `rose/rose`, fichier
**ignoré par git**, passe finale 0 échec. Ce n'est pas un changement de code.)

## 7. Ce que cela débloque et ce qui reste

- Le moteur G5 dispose maintenant d'une primitive d'allocation **réutilisante**
  en Rose pur : un programme qui travaille par phases peut garder un vivant
  borné sans touches à `rose/src`. Le tas bump de l'ELF reste ce qu'il est :
  le GC ne rend pas de mémoire au noyau, il **empêche la croissance** en
  réutilisant les cases (mesure §5).
- Brancher ce module dans `compilateur/rose_natif.rose`/`emetteur_elf.rose`
  demande de remplacer leurs tables d'étiquettes et leurs listes de morceaux
  par des appels `gc_*` ; c'est hors périmètre de ce chantier (interdit de
  toucher ces fichiers) et doit être mesuré avec `prof_mem_rose.sh -n 3` après
  chaque changement.
- `gc_set` coûte O(pool) ; si le pic de vivants doit monter à 100 000, il
  faudra une file à deux niveaux (blocs de 256 réutilisés par bitmap) — idée
  de conception, **pas une mesure**.
- Les compteurs sont cumulatifs par construction : ils ne sont pas
  rembobinés par `gc_restore`, ce qui est ce qui permet de prouver qu'un banc
  a bien fait 1 000 000 d'allocations.
