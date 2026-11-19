# G5-TAS — instrumentation et réduction de l'épuisement du tas dans l'ELF auto-écrit (constat daté, 19/11/2026)

Agent `tas1` (sous-agent). Dépôt `/home/clausake/Stefan/ROSE`, HEAD de départ `a9b5409`.
**Aucun commit, aucune publication, aucun fichier tiers modifié.** Deux fichiers touchés :
`compilateur/emetteur_elf.rose` (fichier confié) et ce document. `rose/rose`,
`emetteur.rose`, `emetteur_pe.rose`, `run.rs`, `PLAN.md` sont inchangés.

Binaire construit pour TOUTES les mesures, **sans remplacer `rose/rose`** :
`/tmp/opencode/tas1_agents/rose` (build isolé
`CARGO_TARGET_DIR=/tmp/opencode/tas1_agents/target CARGO_BUILD_JOBS=2 cargo build --release --bin rose`,
md5 `553feae2a3b84403519c8a15d235d78e`). Le binaire livré `./rose/rose` avait md5
`12eac46007cf0fe9e1aec2dff6fbf665` au départ et n'a pas été touché.

## 1. Verdict court

**Objectif 1 ATTEINT** — instrumentation réelle par compteurs en mémoire RW, lus sous gdb :
**1 535 615 allocations / 67 108 136 octets** avant correction (moyenne 43,70 o),
ventilés par site d'allocation.

**Objectif 2 ATTEINT** — décision structurelle unique : représenter les sous-chaînes par
**tranche (pointeur, longueur)** au lieu de matérialiser un descripteur, et **interner les
chaînes immuables**. Résultat : **180 845 allocations / 6 460 008 octets**, soit
**−88,2 % d'appels et −90,4 % d'octets**. Le tas n'est plus épuisé.

**Objectif 3 NON ATTEINT** — l'ELF corrigé de `emetteur.rose` s'exécute toujours en
**rc=139, 0 octet** : le dump `8ac53eed…` n'est pas produit. Le point de défaillance a
changé de nature (plus d'épuisement du tas, mais une confusion de type dans la
concaténation) et est localisé précisément au §5.

**Verdict self-hosting : INCHANGÉ — NON.** `rose/rose` exige toujours `cargo build`.

## 2. Base rejouée avec le binaire isolé

```
R rir compilateur/emetteur.rose > W/fp.rir
   -> 263 065 octets, md5 8ac53eed3d4967a0d7d9ab333256801c   (identique à G5_IO)
R compilateur/emetteur_elf.rose W/fp.rir > W/self_base.elf
   -> rc=0, 434 640 octets, md5 61b0a901fc9d7425ade83ff1342e7011, ~155 s
chmod +x W/self_base.elf ; W/self_base.elf compilateur/emetteur.rose > dump
   -> rc=139, 0 octet, md5 d41d8cd98f00b204e9800998ecf8427e
```

gdb sur cette base : `pc=0x400560`, `x0=0x64000010`, `x1=1`, `x30=0x400538`,
instruction `str x1,[x0]` (création d'un descripteur `[tag=1][len=1][octet]`).

## 3. Instrumentation (sur une copie, jamais dans le fichier livré)

Copie `/tmp/opencode/tas1_agents/emetteur_elf_instr.rose` : `g5_alloc_inline` émet 6
instructions supplémentaires qui incrémentent deux compteurs 64 bits en page RW :
`0x60000040` = nombre d'appels, `0x60000048` = octets cumulés. L'ELF instrumenté
(`self_instr.elf`, 480 168 o) est identique au fonctionnel hormis ces compteurs.

Résultat avec `compilateur/emetteur.rose` en argument :

| Grandeur | Valeur |
|---|---|
| nb allocations | **1 535 615** |
| octets alloués | **67 108 136** (tas de 67 108 864) |
| moyenne | **43,70 octets/appel** |

Deuxième copie (`emetteur_elf_sites.rose`) : `g5_alloc_inline($site)` compte par site
d'émission. ventilation **mesurée** (les 12 sites totalisent exactement les chiffres
ci-dessus) :

| Site | Appels | Octets | Moyenne |
|---|---:|---:|---:|
| 1 — `g5_make_str` (littéraux RIR) | 470 288 | 11 286 904 | 24 |
| 2 — `lstvide` | 0 | 0 | — |
| 3 — `lst` | 0 | 0 | — |
| 4 — `g5_emit_sous` (`sous_chaine`) | 470 281 | 41 384 728 | 88 |
| 5 — `g5_emit_idx` (`idx` sur chaîne) | 595 044 | 14 281 056 | 24 |
| 6 — `g5_emit_join` | 0 | 0 | — |
| 7 — `g5_emit_entier` | 0 | 0 | — |
| 8 — `g5_emit_cat` | 0 | 0 | — |
| 9 — `g5_emit_stubs` (`table`) | 0 | 0 | — |
| 10 — `g5_emit_nbargs` | 1 | 48 | 48 |
| 11 — `g5_emit_lire` | 1 | 155 400 | 155 400 |
| 12 — `g5_emit_lire` (chemin d'erreur) | 0 | 0 | — |

**Cause confirmée, chiffrée** : ce ne sont pas 2,8 M appels de 24 o (l'estimation
arithmétique de `G5_IO.md` §4), mais 1,54 M appels hétérogènes, dont **trois boucles
d'allocation** : un descripteur de 1 caractère par `idx` (595 k), un descripteur de
littéral reconstruit à chaque exécution du code (470 k) et un descripteur de 8 octets
par `sous_chaine` (470 k).

Le point chaud de `sous_chaine` est identifié dans `emetteur.rose` ligne 3501 :
`sous_chaine($src, $j, $j + 8) == "importer"` dans le balayage du source, exécuté à
chaque position de caractère (470 281 appels × 88 o = 41,4 Mio).

*Piège de mesure documenté* : une première instrumentation plaçait les compteurs à
`0x60000040` et lisait `1 704 860 150` appels avec un chemin long
(`/tmp/opencode/tas1_agents/empty.rose`) : le tampon de chemin non borné à
`0x60000020` écrasait le compteur. Le chiffre retenu (1 535 615) est celui obtenu
avec le chemin court `compilateur/emetteur.rose`, et la somme des 12 sites
l'a confirmé.

## 4. Décision structurelle et réduction mesurée

Trois modifications, toutes dans `compilateur/emetteur_elf.rose` :

1. **Internement des littéraux** (`g5_make_str`) : table à `0x60000300`, 512 entrées
   de 16 o `[clé 64 bits][pointeur]` ; la clé est *vérifiée* avant réutilisation, donc
   une collision d'index ne peut jamais rendre une mauvaise chaîne. Le littéral n'est
   construit qu'une fois. Début du tas déplacé de `0x60000300` à `0x60004000`.
2. **Internement des caractères** (`g5_emit_idx`, branche chaîne) : table à
   `0x60003000`, 256 entrées de 8 o indexées par l'octet. Au plus 256 descripteurs
   de 1 caractère pour tout le programme. Les chaînes Rose étant immuables et
   l'égalité G5 étant par contenu (`g5_emit_streq`), le partage est sûr.
3. **Fusion `sous_chaine(s,a,b) == "littéral"`** (et `!=`) : détection du motif RIR
   dans `g5_eval` ; émission d'un test de longueur puis d'une comparaison directe
   des octets de la source avec le littéral connu à l'émission. Aucun descripteur
   de sous-chaîne n'est alloué : c'est la représentation par tranche
   `(pointeur, longueur)` demandée.

Mesure après (ELF `self_final2*.elf` émis depuis le même `fp.rir`) :

| Grandeur | Avant | Après | Variation |
|---|---:|---:|---:|
| allocations | 1 535 615 | **180 845** | **−88,2 %** |
| octets | 67 108 136 | **6 460 008** | **−90,4 %** |

Ventilation après (site → appels / octets) :
littéraux 75 / 1 816 ; `lst` 33 / 1 056 ; `sous_chaine` 953 / 38 024 ;
`idx` 123 / 2 952 ; `joindre` 118 361 / 4 734 440 ; `cat` 61 297 / 1 526 248 ;
`table` 1 / 24 ; `argument` 1 / 48 ; `lire_texte` 1 / 155 400.

Les deux sources dominantes restantes sont `joindre` (listes append-only, copie
doublante) et `cat` (concaténations de chaînes) — 4,7 Mio + 1,5 Mio d'octets,
largement sous le tas de 64 Mio. **L'épuisement du tas est supprimé.**

Un état intermédiaire (internement seul, sans fusion) a été mesuré : 760 743 appels
mais **67 092 584 octets** — le tas restait épuisé, les 41 Mio de `sous_chaine`
suffisant à eux seuls. C'est cette mesure qui a imposé la décision 3.

## 5. Nouveau point de défaillance (gate non atteint)

ELF final `self_final2.elf` : 530 228 o, md5 `440af926e625e75e0504819269226a52`,
rc d'émission 0. Exécution :

```
W/self_final2.elf compilateur/emetteur.rose > dump   -> rc=139, 0 octet
md5(dump) = d41d8cd98f00b204e9800998ecf8427e
```

gdb (charge utile `compilateur/emetteur.rose`) :

```
pc   = 0x4811c8   ldr x8,[x6,#8]        (routine concat cat, 0x4811b8)
x0   = 0x11       (17 = entier 2 auquel un bit de tag chaîne a été ajouté)
x1   = 0x6062d251 (descripteur de chaîne valide, pointeur libre tas = 0x6062d250)
x30  = 0x415998
[0x60000000] = 0x6062d250  ->  le tas n'est PAS épuisé (6,5 Mio utilisés)
```

Chaîne d'origine de la valeur invalide, lue dans le désassemblage :

```
0x4158ac  mov  x0,#1 ; lsl x0,x0,#3     -> 8 (entier 1 tagué)
0x4158b4  ldr  x1,[sp,#2576]            -> autre entier tagué
0x4158d0  add  x0,x1,x0                 -> 0x10 (entier 2 tagué)
0x4158f0  bl   0x480fd4                 -> orr x0,x0,#1  (queue de g5_emit_idx)
0x4158f4  str  x0,[sp,#1552]            -> 0x11 stocké dans un slot
...
0x415988  mov  x2,x0 ; mov x0,x1 ; mov x1,x2
0x415994  bl   0x4811b8                 -> cat(0x11, chaîne)
0x4811c8  ldr  x8,[x6,#8]               -> SIGSEGV (x6 = 0x10)
```

**Nature du défaut** : un entier tagué (`0x10`) reçoit un bit de tag « chaîne »
(`0x11`) puis est passé comme opérande de concaténation. La routine de tag
`0x480fd4` est la queue de `g5_emit_idx` (`orr x0,x0,#1 ; ret`) : un appel aboutit
sur cette queue, c'est-à-dire sur le label de fin de la routine `idx`.

**Ce défaut n'est pas introduit par la fusion** : preuve croisée par patch binaire
de l'ELF *intermédiaire* (internement seul, `self_fix.elf`, md5
`d2f7999b54522d5688c00a8087f1a068`) dont on a seulement agrandi `p_memsz` à 512 Mio
(second `PT_LOAD`, offset fichier 160). Avec 512 Mio, il ne meurt plus d'épuisement
mais au même endroit : `pc=0x481280`, `x0=0x11`, `x30=0x4159e4`. La confusion de
type était donc latente sous l'épuisement du tas ; mes réductions l'ont simplement
rendue visible. Je ne prétends pas l'avoir réparée.

Frontières non traitées, héritées de `G5_IO.md` §5 et toujours présentes :
stubs `table`/`inserer`/`cle_existe` (cle_existe renvoie toujours faux), octets vs
points de code UTF-8, appels à 22 arguments qui écrasent `x8`/`x9`. Le défaut de
concaténation ci-dessus est cohérent avec un de ces écarts de sémantique de valeurs,
mais je n'ai pas de preuve permettant de choisir lequel : je ne l'affirme pas.

## 6. Validation horizontale (constats re-mesurés)

```
rose verif compilateur/emetteur_elf.rose      -> vérifié
fib(10) -> RIR -> ELF final                   -> 55, rc=0
E/S : ELF final + /tmp/opencode/io1_agents/petit.txt
      -> sortie = Rose natif, md5 24003210c4a9b595b6e08ae7ac41b5af des deux côtés
point fixe M4.5 : rir emetteur.rose ×2        -> 8ac53eed… ×2 (binaire isolé)
jardin jardins/tests/premiers.rose            -> 20 tests, 0 échec
cargo test --release (target dir isolé)       -> 220+204+6+1 = 431 passés, 0 échec
ELF final + emetteur.rose                     -> rc=139, 0 octet (gate NON atteint)
```

Test ciblé de la fusion (`sous_chaine(...,j,j+8) == "importer"` dans une boucle,
avec un `!=`) : sortie ELF = sortie Rose natif, md5 `5d9cd3bd…` des deux côtés.
Test ciblé d'internement (boucle de `idx` + concaténations + comparaisons) :
sortie ELF = sortie Rose natif sur les valeurs chaînes ; la seule différence
observée est l'affichage des booléens (`1` dans l'ELF contre `vrai` dans Rose
natif), écart préexistant sans rapport avec ce chantier.

## 7. Ce que je ne dis pas

Je ne dis **pas** « self-hosting », « full hosted » ni « bootstrap » : l'ELF du
compilateur ne produit toujours aucun dump et `rose/rose` nécessite toujours cargo.
Je ne dis pas que la confusion de type du §5 est corrigée : elle ne l'est pas.
Les 431 tests ont été lus dans la sortie de `cargo test` (0 échec) ; cela ne prouve
pas le self-hosting pour autant.
Je ne dis pas que `ecrire_bytes` a été validé séparément de `ecrire_texte`.
Les compteurs d'instrumentation ne figurent dans aucun fichier livré : ils
n'existent que dans les copies `/tmp/opencode/tas1_agents/emetteur_elf_*instr*.rose`
et `*sites.rose`.
