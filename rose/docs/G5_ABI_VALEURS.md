# G5 — ABI unique des valeurs (constat daté, 19/11/2026)

Agent `ab1` (vague 36). Dépôt `/home/clausake/Stefan/ROSE`, HEAD au départ `8c17076`.
Binaire construit pour les mesures, **sans remplacer `rose/rose`** :
`/tmp/opencode/ab1_agents/rose` (md5 `de1fbfd918131b7317df13e6f6aaec22`,
`CARGO_BUILD_JOBS=2 cargo build --release --target-dir /tmp/opencode/ab1_agents/target`).

## 1. Représentation constatée AVANT le correctif (diagnostic rejoué)

Mesures initiales rejouées avec le binaire construit ici :

```
RIR de emetteur.rose : md5 8ac53eed3d4967a0d7d9ab333256801c, 263065 octets
ELF émis             : 334092 octets, rc=0
exécution            : SIGSEGV rc=139, pc=0x4514ec (g5_emit_len), x0=0x120002b40
```

Le désassemblage sous gdb de l'ELF de départ a localisé l'origine exacte de
`0x120002b40` : une instruction `add x0, x1, x0` (pc `0x40d088`) qui **additionne
deux pointeurs de descripteurs chaîne**. Cause réelle : en Rose, `+` sur deux
chaînes est une CONCATÉNATION ; G5 émettait un `add` entier brut. Un `ldr x1,[x0]`
sur la somme des deux pointeurs donnait le SIGSEGV observé dans `longueur`.
Les gardes de plage ajoutées plus tôt (`ch`) ne pouvaient pas réparer cela : le
placage d'un entier 1 dans `joindre` et la somme de deux pointeurs sont des
**erreurs de sémantique d'opérateur**, pas des erreurs d'index.

Tableau `valeur → bits en mémoire` avant unification :

| Valeur Rose | Emise par | Bits dans x0 / dans un slot | Ce qu'attendait le helper |
|---|---|---|---|
| entier `(n 42)` | `g5_load` | `0x2a` brut | pointeur `[tag][len]` |
| booléen `(b vraie)` | `g5_load` | `0x1` brut | pointeur / booléen |
| chaîne `(s "a")` | `g5_make_str` | `ptr` brut vers `[1][len][octets]` | `ptr` brut (OK) |
| liste `(lst …)` | `g5_eval` | `ptr` brut vers `[2][len][cap][valeurs]` | `ptr` brut (OK) |
| `longueur(s)` | `g5_emit_len` | renvoie `len` **brut** | l'appelant attend un entier brut (OK par hasard) |
| `entier(s)` | `g5_emit_entier` | renvoie entier **brut** | idem |
| `+` sur 2 chaînes | `g5_eval/bin` | `ptr1 + ptr2` = **pointeur invalide** | — |
| `==` sur 2 chaînes | `g5_eval/bin` | compare les **pointeurs**, pas le contenu | — |
| `inserer` (stub) | `g5_emit_inser` | laisse x0 inchangé (= n'importe quoi) | liste |

## 2. Décision d'unification : tag dans les 3 bits bas

**Une seule représentation pour toutes les valeurs G5** (aucun pointeur nu, aucun
entier nu) :

| Valeur Rose | Bits en mémoire (x0, slots, arguments x0..x7) |
|---|---|
| entier `n` | `(n << 3) & 2^64−1` (bits bas `000`) |
| booléen | `(0 or 1) << 3` = `0` ou `8` |
| chaîne | `ptr | 1` où `ptr` = `*[tag=1][len][octets…]` (descripteur aligné 8) |
| liste | `ptr | 2` où `ptr` = `*[tag=2][len][cap][valeurs…]` |
| flottant, nul, stub numérique | replié en entier `0` tagué |

Choix du tag **dans les bits bas** plutôt qu'un tag dans le mot 0 du descripteur :
les entiers restent des immédiats (pas d'allocation par littéral), `add`, `sub`,
les comparaisons et `et`/`ou` restent corrects sans déballage, et le tag est
consultable par une seule instruction `and xN,xM,#7`. Le désalignement des
pointeurs tagués impose un `and xN,xM,#0xfffffffffffffff8` (`g5_masktag`) avant
tout déréférencement ; c'est fait dans TOUS les helpers.

Encodages AArch64 vérifiés avec `as`/`objdump` sur cette machine (aarch64) :
`and #~7 = 0x927df000`, `and #7 = 0x92400800`, `lsr #3 = 0xd343fc00`,
`lsl #3 = 0xd37df000`, `orr #1 = 0xb2400000`, `orr #2 = 0xb27f0000`.

Performance d'émission (mesuré, `/usr/bin/time`, une seule émission à la fois
sauf mention) : version de départ **101,39 s** ; première version ABI à base de
labels `g5_next()` pour les dispatches > 4 min (deux émissions concurrentes,
~3000 labels supplémentaires dans `g5_patch` qui fait une recherche linéaire) ;
version finale à **sauts courts à déplacement calculé à l'émission**
(`g5_branch_off`, zéro label ajouté) **155,21 s** (aucune tâche concurrente) —
donc du même ordre que la version de départ, sans faire de `g5_patch` une
recherche linéaire sur 7 000 labels.

Conséquences appliquées de façon cohérente :
- tous les littéraux (`n`, `entier`, `f`, `flottant`, `b`, jeton nu) passent par
  `g5_load_val` (= `g5_load` + `lsl #3`) ;
- les chaînes et listes sont taguées à la construction (`orr #1` / `orr #2`) ;
- les paramètres sont stockés tels quels (déjà tagués) dans les slots ;
- `+` teste les tags : entiers → `add` ; sinon appel au helper de concaténation
  (nouveau) qui recopie les octets des deux descripteurs ;
- `==` / `!=` testent les tags : deux chaînes → comparaison de **contenu**
  (nouveau helper `streq`), sinon comparaison des immédiats ;
- `*`, `/`, `%`, `^` déballent (`lsr #3`), calculent, puis re-ballent (`lsl #3`) ;
- `neg`, `non`, comparaisons, `et`, `ou` opèrent directement sur les valeurs
  taguées (les comparaisons produisent maintenant `0` ou `8`) ;
- `longueur`, `ordre`, `entier`, `idx`, `sous_chaine`, `joindre`, `chaine`,
  `stubs` renvoient des valeurs **taguées** ; tous les pointeurs reçus sont
  masqués avant déréférence ;
- `afficher` (`g5_emit_print`) distingue le tag 1 (impression des octets du
  descripteur) du tag 0 (impression décimale après `lsr #3`).

## 3. Mesures après unification

| Cas de test | Avant | Après |
|---|---|---|
| `fib(10)` → RIR → ELF | `55`, rc=0 | `55`, rc=0 |
| `BASE("a\|b")` → `longueur` | `1`, rc=0 | `1`, rc=0 |
| `t_str` (`sous_chaine`) | `2`, rc=0 | `2`, rc=0 |
| `emetteur.rose` entier → ELF | 334092 o, SIGSEGV `0x120002b40` | 433908 o, épuisement du tas (§5) |

Deux bogues d'encodage détectés et corrigés au passage, avec preuve par
désassemblage (`objdump`) et test dédié :
- `msubr` utilisait `Ra*16` au lieu de `Ra*1024` : `%` renvoyait `4` pour
  `5 % 3` (word `0x9b008052` = `msub x18,x2,x0,x0` au lieu de
  `0x9b008443` = `msub x3,x2,x0,x1`). Après correction : `5 % 3 = 2`,
  `5%3 + 2^4 = 18` (identique à Rose).
- la concaténation allouait `16+total+8` octets non arrondis à 8 : le pointeur
  libre du tas devenait désaligné, les tags se chevauchaient et `joindre`
  écrivait hors de la page (SIGSEGV `str x1,[x4]`, `x4=0x600ffffa`).
  Taille corrigée en `16 + 8*ceil(total/8)` ;
- le déballage des entiers négatifs utilisait `lsr #3` au lieu de `asr #3` :
  `entier("-42")` s'affichait `2305843009213693910`. Correction (`g5_asr3`)
  dans l'entrée de `afficher`, `/`, `%`, `sous_chaine`, `idx` et `^`.
  Mesure finale : `entier("-42")` → `-42`, `5%3 + 2^4` → `18`.

Limite de représentation assumée : un booléen est un entier tagué, donc
`afficher(vrai)` produit `1` dans l'ELF au lieu de `vrai` dans Rose. Le tag ne
distingue pas `entier` de `booléen` ; c'est un choix de l'ABI unique, pas une
régression de l'émission.

Taille des ELF de test (final) : `fib` 1368 → 1956 o ; `fn_base_driver` 1532 →
2208 o ; `t_cat` (concat + égalité de chaînes, nouveau) → `1/1/4` comme Rose
(hormis `vrai` affiché `1`) ; `t_conv` → `123/-42/0/98`.
`rose verif compilateur/emetteur_elf.rose` = **vérifié** après modification.

## 4. Validation horizontale (constats)

```
rose verif compilateur/emetteur_elf.rose        -> vérifié
point fixe M4.5 : rose rir emetteur.rose ×2      -> 8ac53eed3d4967a0d7d9ab333256801c ×2, cmp_rc=0
cargo test --release (target dir isolé)          -> 220+204+6+1 = 431 tests, 0 échec
rose/rose jardins/tests/premiers.rose            -> 20 tests, 0 échec
```

`git diff` contient aussi `rose/src/run.rs` (52 lignes) : modification
**concurrente d'un autre agent** (TA2), étrangère à ce chantier. Je n'ai touché
que `compilateur/emetteur_elf.rose` et ce document ; aucun commit ni publication.

## 5. Verdict réflexivité : **NON atteinte**, frontière déplacée

Avec l'ABI unifiée, l'ELF de `emetteur.rose` est toujours émis (433 908 o,
rc=0, 155,21 s, md5 du source RIR d'entrée `8ac53eed…`) mais **ne produit aucun
octet sur stdout** avant de segfault (rc=139, 0 octet, md5 `d41d8cd9…` ≠
`8ac53eed…`).
Le SIGSEGV d'origine (`x0=0x120002b40`, `g5_emit_len`) a disparu : c'était une
addition de deux pointeurs de chaîne, pas un défaut de plage. Le nouveau point
de défaillance est **l'allocateur du tas de l'ELF émis** :

| Tas RW (`p_memsz`) | Comportement observé (gdb) |
|---|---|
| 1 Mio | `[0x60000000]=0x60100018`, `x0=0x60100000`, `str x1,[x0]` à `pc=0x411fe8`, `x30=0x411fc0` |
| 64 Mio (ELF final livré, 433 908 o, `/tmp/opencode/ab1_agents/full_final2.elf`) | `[0x60000000]=0x64000008`, `x0=0x63fffff0`, `strb w4,[x5,x3]` à `pc=0x469d00` (copie de `concat`), `x30=0x465a74` |
| 512 Mio (même ELF, header patché) | `[0x60000000]=0x80000008`, `str x6,[x4]` à `pc=0x469bb4` (`joindre`), `x30=0x43fd58`, en **1,32 s** |

Le tas est épuisé dans les trois cas, y compris 512 Mio en 1,3 s : ce n'est pas
un simple manque de place pour compiler un dump de 263 Ko, c'est une **boucle
d'allocation sans fin** dans le compilateur émis (les primitives dont il dépend
sont des stubs : `nb_arguments`=0, `lire_texte`/`argument`="" , `table`/`inserer`/
`cle_existe` neutres). Ajouter de la mémoire ne fait que reculer le même mur.

Donc : **réflexivité fonctionnelle NON atteinte**. Aucune prétention de
self-hosting complet. `rose/rose` exige toujours `cargo build`.

## 6. Frontière restante

1. **Boucle d'allocation** ci-dessus : premier obstacle exécutable, avant même
   le premier octet de dump RIR.
2. **Entrée/sortie du compilateur émis** : `nb_arguments` renvoie 0 (tagué),
   `argument` et `lire_texte` sont des stubs → source vide, même si la boucle
   d'allocation était réparée, le dump ne pourrait pas être identique
   (`8ac53eed…`). `ecrire_bytes` n'est pas implémenté non plus ; seule la voie
   `>>`/`afficher` l'est (avec saut de ligne ajouté).
3. **Tables** : `table()`, `inserer(table,…)`, `cle_existe` restent des stubs
   neutres ; le compilateur utilise `inserer` 21 fois pour ses mots-clés.
4. **Sémantique de valeurs résiduelle** : booléens affichés `1`/`0` au lieu de
   `vrai`/`faux`, flottants repliés en entiers (ex. `(f 7/2)` → 0), bigints
   modulo 2^64, globaux repliés à 0, `chaine(entier)` → chaîne vide.
5. **Runtime** : allocateur bump sans libération, tas 64 Mio, deux `PT_LOAD`,
   pas de sections ni de symboles, pas de PE/Mach-O dans ce fichier.
