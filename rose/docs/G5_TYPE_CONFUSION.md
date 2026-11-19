# G5-TYPE — confusion de type `entier` tagué « chaîne » dans l'émetteur auto-hébergé (constat daté, 19/11/2026)

Agent `type1` (sous-agent). Dépôt `/home/clausake/Stefan/ROSE`, HEAD de départ
`b459efafe9735f8521ee6efb3512e98af807eb4a`. **Aucun commit, aucune publication.**
Deux fichiers touchés au total : `compilateur/emetteur_elf.rose` (fichier confié) et
ce document. `rose/rose`, `emetteur.rose`, `emetteur_pe.rose`, `run.rs`, `PLAN.md`
sont inchangés (md5 de `rose/rose` re-mesuré : `12eac46007cf0fe9e1aec2dff6fbf665`).

Binaire construit pour TOUTES les mesures, **sans remplacer `rose/rose`** :
`/tmp/opencode/type1_agents/rose` (build isolé
`CARGO_TARGET_DIR=/tmp/opencode/type1_agents/target CARGO_BUILD_JOBS=2 cargo build --release --bin rose`,
md5 `2c3be8b84b00f12e4cbf69d30ec74642`). Répertoire de travail :
`/tmp/opencode/type1_agents/W/`.

## 1. Verdict court

**Objectif 1 ATTEINT** — `x0=0x11` reproduit sous gdb avec le binaire courant et
l'ELF de `emetteur.rose` ; chaîne `add → orr #1 → slot → cat` confirmée
instruction par instruction (§2).

**Objectif 2 ATTEINT** — la cause racine est une **collision de labels** dans
l'émetteur : `g5_emit_idx` utilise trois labels (`$lbl`, `$lbl+1`, `$lbl+2`) mais
la réservation n'en allouait que deux, si bien que `$g5_chl` (routine `chaine`)
était le même numéro que `$lbl+2` de `idx`. Chaque appel à `chaine()` atterrissait
sur la queue `orr x0,x0,#1 ; ret` de `idx`, taguant une valeur entière comme
chaîne. Trois correctifs appliqués et vérifiés (§3).

**Objectif 3 NON ATTEINT** — l'ELF corrigé de `emetteur.rose` (531 168 o,
`48e3e59e…`) s'exécute toujours en **rc=139, 0 octet**. Le dump `8ac53eed…`
n'est pas produit. Un **nouveau point de défaillance**, distinct et mesuré, est
localisé au §5. Preuve qu'il est préexistant : il persiste dans un ELF d'émetteur
reconstruit **sans** le correctif de conversion décimale.

**Verdict self-hosting : INCHANGÉ — NON.** Je ne prétends pas le full-hosting.

## 2. Reproduction gdb du `x0=0x11` (avant correction)

Chaîne de base rejouée :

```
R rir compilateur/emetteur.rose > W/fp.rir
   -> 263 065 octets, md5 8ac53eed3d4967a0d7d9ab333256801c   (identique à G5_IO/G5_TAS)
R compilateur/emetteur_elf.rose W/fp.rir > W/self.elf
   -> rc=0, 530 228 octets, md5 440af926e625e75e0504819269226a52 (identique à G5_TAS, 6m12s)
chmod +x W/self.elf ; W/self.elf compilateur/emetteur.rose > dump
   -> rc=139, 0 octet, md5 d41d8cd98f00b204e9800998ecf8427e
```

`gdb -batch -x W/repro.gdb` (charge utile `compilateur/emetteur.rose`) :

```
pc   = 0x4811c8   ldr x8,[x6,#8]        (routine cat, entrée 0x4811b8)
x0   = 0x11       (17 = entier 2 tagué « chaîne »)
x6   = 0x10       (16 = entier 2 tagué « entier »)
x1   = 0x6062d269 (descripteur de chaîne valide)
x30  = 0x415998
[0x60000000] = 0x6062d280  -> le tas n'est PAS épuisé (6,5 Mio utilisés)
```

Désassemblage de la zone appelante (gdb `x/50i 0x415880`), la chaîne complète :

```
0x4158ac  mov  x0,#1 ; lsl x0,x0,#3     -> 8   (entier 1 tagué)
0x4158b4  ldr  x1,[sp,#2576]            -> autre entier tagué
0x4158b8  and  x2,x1,#7 ; cmp x2,#0 ; b.ne 0x4158d8   (tests de tag : entiers)
0x4158c4  and  x2,x0,#7 ; cmp x2,#0 ; b.ne 0x4158d8
0x4158d0  add  x0,x1,x0                 -> 0x10  (entier 2 tagué)
0x4158e8  str  x0,[sp,#2128]
0x4158ec  ldr  x0,[sp,#2128]
0x4158f0  bl   0x480fd4                 -> orr x0,x0,#1  (QUEUE de g5_emit_idx)
0x4158f4  str  x0,[sp,#1552]             -> 0x11 stocké dans un slot
...
0x415980  add  x0,x1,x0
0x415994  bl   0x4811b8                 -> cat(0x11, chaîne)
0x4811c8  ldr  x8,[x6,#8]                -> SIGSEGV (x6 = 0x10)
```

`0x480fd4` désassemblé : `orr x0,x0,#1 ; ret`, suivi en `0x480fdc` par
`asr x1,x1,#3 … ldr x0,[x2] ; ret` — exactement la fin de `g5_emit_idx`
(branche chaîne puis branche liste, `compilateur/emetteur_elf.rose` lignes
1140-1151). Le `bl` d'un appel `chaine()` atterrit bien sur la queue de `idx`.

**Preuve statistique** (comptage des `bl` dans le code machine de `self.elf`) :
**140 `bl 0x480fd4`** (queue de `idx`) et **0** `bl` vers la vraie routine
`chaine` — or `fp.rir` contient exactement **140 `(app chaine …)`**. La
correspondance est exacte, un pour un.

## 3. Cause racine et correctifs

### 3.1 Collision de labels (cause du `x0=0x11`)

`g5_emit_idx` émet trois marqueurs : `g5_mark($lbl)` (l.1113),
`g5_mark($lbl+2)` (l.1140, la queue `orr #1`), `g5_mark($lbl+1)` (l.1143).
La réservation de labels dans `$principal` n'allouait qu'**un** label
supplémentaire après `$g5_idxl` (`tant_que ($k < 1)`). `$g5_chl` était donc
alloué comme `$g5_idxl + 2`, c'est-à-dire le même numéro que le `$lbl+2` de
`idx`. `g5_labelidx()` rend la **première** occurrence dans `$g5_lbldef` ; `idx`
étant émis avant `chaine`, un `bl $g5_chl` résolvait vers la queue de `idx`.

**Correctif 1** (l.1669) : `tant_que ($k < 1)` → `tant_que ($k < 2)` pour `idx`.
Après correction, dans l'ELF `self_fix.elf` : **140 `bl` vers la vraie routine
`chaine` (0x4812e4), 0 `bl` vers la queue de `idx` (0x480fd4)** — preuve
machine de la résolution du collision.

### 3.2 Conversion décimale absente (révélée par le correctif 1)

Une fois la collision levée, `chaine(entier)` appelait la vraie routine qui ne
fabriquait qu'une chaîne **vide**. Or `emetteur.rose` sérialise ses nœuds avec
`chaine($p+1) + "|" + chaine($i)` (`emetteur.rose` l.456, 513-589, 995) : 140
appels dans `fp.rir`. Le dump ne pouvait donc pas être bit-à-bit identique.

**Correctif 2** : `g5_emit_ch` convertit désormais un entier en décimale signée
exacte (scratch RW `0x60003800..0x60004000`, 2 Kio libres entre la table de
caractères `0x60003000+2 Kio` et le tas `0x60004000` — adresses re-mesurées dans
l'ELF émis). Micro-test `chaine(42) → 42`, `-7 → -7`, `1234567890 →
1234567890`, `0 → 0` : sortie ELF = sortie Rose natif, `diff` silencieux.

### 3.3 Littéraux négatifs du RIR (révélés par le correctif 2)

`fp.rir` contient **62 `(n --1)`** : le RIR sérialise les négatifs avec un double
moins. `g4_entier_ou_zero` n'enlevait qu'un seul `-` (`entier("-7") = -7`, puis
`-(-7) = +7` : signe perdu ; pire, `--7` était rejeté comme invalide → 0).

**Correctif 3** : consommer les deux `-` et capturer `$deb` (début des chiffres)
avant la boucle de validation. Micro-test `chaine(-7) → -7` : sortie ELF =
natif.

Les trois correctifs sont dans le SEUL fichier confié. `rose verif
compilateur/emetteur_elf.rose` → **vérifié** après chaque modification.

## 4. Résultat des trois cas de test demandés

| Cas | Résultat mesuré | Verdict |
|---|---|---|
| (a) `fib(10)` | ELF émis (2 960 o) → `55`, **rc=0** | OK |
| (b) E/S fichier md5 | ELF + `/tmp/opencode/io1_agents/petit.txt` → sortie byte-à-byte = natif, **md5 `24003210c4a9b595b6e08ae7ac41b5af` des deux côtés** | OK |
| (c) `emetteur.rose` entier | ELF `self_fix.elf` (531 168 o, `48e3e59e…`), rc d'émission 0 ; exécution → **rc=139, 0 octet**, md5 `d41d8cd98f00b204e9800998ecf8427e` | **PORTE NON ATTEINTE** |

Le point de défaillance a changé de nature (§5) : la confusion de type
`0x11` a disparu, l'émetteur progresse plus loin avant de mourir.

## 5. Nouveau point de défaillance (gate non atteint)

`W/self_fix.elf compilateur/emetteur.rose > dump` → rc=139, 0 octet.gdb
(charge utile emetteur.rose) :

```
Mode dominant (emetteur.rose, fib.rose, t_io.rose, t_ch5.rose) :
pc   = 0x481644   ldr x2,[x6,#8]        (routine streq, 0x48163c)
x0   = 0x1        (pointeur NUL tagué « chaîne »)
x1   = 0x6002b0d1 (descripteur de chaîne valide)
x6   = 0x0        (= x0 & ~7)
x30  = 0x4571e8
[0x60000000] = 0x6062d320 -> tas TOUJOURS non épuisé
```

Chaîne d'origine lue dans le désassemblage : le résultat de la fonction `T0`
(`emetteur.rose` l.116-121 : `$q = $p ; si ($q >= $n) { $q = $n-1 } ; retour
$g[$q]`) est utilisé comme position par `post()` (l.607 `$t = T0($g, $q)`) puis
comparé à un littéral. `T0` reçoit un `$p` **négatif** : tracing gdb sur
`break *0x4111a8` (T0) avec `fib.rir`, les indices passés sont
`1,1,2,2,1,1,1,1,1, -47, -5228` ; `T0` ne borne que par le HAUT
(`$q >= $n`), pas par le bas, donc `idx($g, -654)` lit hors du descripteur et
dérape sous `0x60000000` (non mapé) → SIGSEGV. Sur la petite source `t_ch.rose`,
le crash se produit directement dans `idx` : `pc=0x4812dc ldr x0,[x2]`,
`x1=-5228`, `x2=0x5fff9db0` (< `0x60000000`).

Le `x0=0x1` de `streq` est le même symptôme vu depuis un autre chemin : une
valeur `0x0|1` (entier 0 tagué chaîne, pointeur nul) circule dans la table de
jetons et est comparée par contenu.

**Ce défaut est PRÉEXISTANT, pas introduit par mes correctifs.** Preuve par
réversion isolée : un ELF d'émetteur reconstruit depuis une copie de
`emetteur_elf.rose` où **la conversion décimale est annulée** (correctifs 1 et 3
conservés ; `em_noch.rose`, ELF `cc1ad8c7…`, 344 568 o) échoue **également** en
rc=139, 0 octet sur `fib.rir` (gdb : `pc=0x453954 ldr x2,[x1]`, `x0=nil`). La
conversion décimale n'est donc pas la cause. Le défaut était masqué avant le
correctif 1 par le crash `chaine`/`cat` beaucoup plus précoce.

## 6. Validation horizontale (constats re-mesurés)

```
rose verif compilateur/emetteur_elf.rose      -> vérifié
fib(10) -> RIR -> ELF final                    -> 55, rc=0
E/S : ELF + /tmp/opencode/io1_agents/petit.txt -> sortie = Rose natif,
      md5 24003210c4a9b595b6e08ae7ac41b5af des deux côtés
point fixe M4.5 : rir emetteur.rose ×3         -> 8ac53eed… ×3, cmp_rc=0
cargo test --release (target dir isolé, RUSTFLAGS="")
      -> 220 + 204 + 6 + 1 = 431 passés, 0 échec
rose/rose jardins/tests/premiers.rose         -> 20 tests, 0 échec
ELF final + emetteur.rose                      -> rc=139, 0 octet (gate NON atteint)
```

Le point fixe M4.5 est mesuré sur la **génération du RIR** (voie interprète),
pas sur l'ELF auto-hébergé : il ne prouve pas la réflexivité.

## 7. Frontière restante

1. **Position négative dans `T0`/`VX0`/`SEPR`** (§5) — premier obstacle
   exécutable. `SEPR` (`emetteur.rose` l.144-154) dérive la position de
   `entier(sous_chaine($r,0,$q))` ; si `$r` ne contient pas de `|` ou si la
   position est négative, `T0` indexe hors liste. Correctif hors du périmètre
   de ce sous-agent (`emetteur.rose` est interdit) : borne inférieure dans
   `T0`/`VX0` ou validation dans `SEPR`.
2. **UTF-8 octets vs points de code** (hérité de `G5_IO.md` §5) : `emetteur.rose`
   contient 3 394 octets non ASCII ; `longueur`/`idx` G5 comptent les octets,
   Rose les points de code.
3. **Stubs `table`/`inserer`/`cle_existe`** : `cle_existe` renvoie toujours faux.
4. **Appels à 22 arguments** écrasent `x8`/`x9` (hérité de `G5_IO.md` §5).
5. **Réflexivité de la conversion `chaine` elle-même** non retentée : pour
   l'instant seule la voie « ELF compile `emetteur.rose` » est bloquée.

## 8. Ce que je ne dis pas

Je ne dis **pas** « self-hosting », « full hosted » ni « bootstrap » : l'ELF du
compilateur ne produit toujours aucun dump et `rose/rose` nécessite toujours
`cargo build`. Je ne dis pas que la confusion de type du §2 est la seule
frontière : elle est corrigée, mais une nouvelle confusion de valeurs (§5) la
relait. Je ne dis pas que `rose/rose` a été remplacé : le binaire livré a gardé
son md5 (`12eac460…`) et mon binaire est isolé dans `/tmp/opencode/type1_agents/`.
Je ne dis pas que le point fixe `8ac53eed…` est produit par l'ELF : il est
produit par la voie interprète (§6).
