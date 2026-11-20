# SELF_HOST_SUITE — rejeu de la chaîne de réflexivité (constat daté, 19/11/2026)

Sous-agent `sh2`. Dépôt `/home/clausake/Stefan/ROSE` (`Stefan/ROSE`), HEAD de départ `6f5ba64`.
Deux fichiers possédés : `compilateur/emetteur_elf.rose` et ce document.
**Aucun commit, aucune publication, aucun remplacement de `rose/rose`.**
Répertoire de travail isolé : `/tmp/opencode/sh2_agents/`.

## 1. Binaire et point fixe re-mesurés

Binaire livré utilisé tel quel : `./rose/rose`, md5 `12eac46007cf0fe9e1aec2dff6fbf665`
(re-mesuré le 19/11 à 22:50+01:00 ; identique à celui des docs `G5_TYPE_CONFUSION.md`
et `G5_TAS.md`).

`compilateur/emetteur.rose` : md5 `390335d29fb9cee368b3456dd892e377` (re-mesuré ;
contient les 5 bornes basses `si ($q < 0) { $q = 0 }` en `T0`/`VX0`/`T1`/`VX1`/`GL0`).
**Fichier interdit ici, non modifié.**

`./rose/rose verif compilateur/emetteur_elf.rose` → **`vérifié`** (re-mesuré).

Dump RIR de référence, rejoué avec le binaire livré :

```
$ ./rose/rose rir compilateur/emetteur.rose > /tmp/opencode/sh2_agents/self.rir
rc=0, 263 520 octets, md5 b3ed9262dd54776a6dbd07bc12368d0e   (0,027 s)
```

⚠ Deux corrections de la consigne, établies par lecture du code :
1. `b3ed9262…` est le md5 **du RIR**, pas du `dump.out` attendu a priori. Le test de
   réflexivité consiste à ce que le binaire émis reproduise **ce RIR exact**.
2. Le binaire émis par `emetteur_elf.rose` exécute le `$principal` de `emetteur.rose`,
   qui fait `$src = lire_texte(argument(0))` (`emetteur.rose` ligne 3481) : son entrée
   est un **source `.rose`**, pas le RIR. Les deux entrées ont été testées : même échec.

## 2. Chaîne complète rejouée (timeout 900 s, jamais 420 s)

| Étape | Commande | Résultat mesuré |
|---|---|---|
| 1 | `rose rir compilateur/emetteur.rose > self.rir` | rc=0, 263 520 o, `b3ed9262…`, 0,027 s |
| 2 | `rose compilateur/emetteur_elf.rose self.rir > self.elf` | **rc=0, 531 428 o, `98dadd85cce0f2dceb4deb9edca26412`, 372 s** (22:51:08→22:57:20+01:00), stderr vide |
| 3a | `./self.elf compilateur/emetteur.rose > dump_src.out` | **rc=139 (SIGSEGV), 0 octet**, `d41d8cd9…` |
| 3b | `./self.elf self.rir > dump_rir.out` | **rc=139, 0 octet**, `d41d8cd9…` |

Le timeout de 420 s de la consigne était **insuffisant** : l'émission a demandé 372 s
dans cette salve, et ~375 s dans les deux suivantes. Aucune sortie, aucun dump RIR.

`file self.elf` → `ELF 64-bit LSB executable, ARM aarch64, statically linked, no section header`.

Non-régression : `f(7) → 42`, rc=0, ELF 2 836 o (programme minimal compilé par le même
émetteur). L'émetteur fonctionne donc sur de petits programmes.

## 3. Diagnostic gdb du défaut

### 3.1 Point de crash (binaire non instrumenté, 531 428 o)

```
Program received signal SIGSEGV
pc   = 0x481704   strb w4,[x5,x3]      (copie de g5_emit_cat)
x0=x5= 0x63ffe460   x3=0x1ba0(7072)   x8=0x2371(9073)   x11=0x1   x30=0x40c154
[0x60000000] = 0x640007e8             (pointeur libre)
tas RW = 0x60000000..0x64000000 (64 Mio) ; pointeur initial 0x60004000
```

Adresse d'écriture `0x63ffe460 + 0x1ba0 = 0x64000000` = fin exacte du tas. L'allocation
avait déjà placé le pointeur libre à `0x640007e8`, **2 536 octets au-delà** de la fin.

### 3.2 Ventilation des allocations (instrumentation sur une COPIE, compteurs RW lus sous gdb)

Copie `/tmp/opencode/sh2_agents/emetteur_instr.rose` : `g5_alloc_inline($site)` incrémente
deux compteurs par site à `0x60002A00 + 16*site`. ELF instrumenté 607 228 o, même crash.

| Site | Routine émettrice | appels | octets |
|---:|---|---:|---:|
| 1 | `g5_make_str` | 31 842 | 764 480 |
| 4 | `g5_emit_sous` | 91 712 | 9 295 392 |
| 5 | `g5_emit_idx` | 123 | 2 952 |
| 6 | `g5_emit_join` | 186 588 | 7 463 520 |
| 7 | `g5_emit_ch` | 90 760 | 2 904 264 |
| 8 | `g5_emit_cat` | 161 161 | **46 507 480** |
| 11 | `g5_emit_lire` | 1 | 155 520 |

**Total = 67 094 504 octets**, soit l'épuisement exact du tas de 64 Mio.

### 3.3 Ce n'est PAS un simple manque de place

- Tas patché à **256 Mio** (`p_memsz` de l'ELF, sans réémission) : même SIGSEGV en 1,28 s,
  nouveau pc `0x48130c` (`str x1,[x5]` dans `g5_emit_ch`), pointeur libre `0x70000048`.
- Tas patché à **4 Gio** : même SIGSEGV en 3,27 s.
- Le crash se produit **même avec un source de 6 octets** (`>> 1`), un fichier vide ou
  un fichier inexistant : boucle d'allocation non bornée, indépendante de la taille d'entrée.

### 3.4 Instruction fautive (traçage de l'op RIR courante)

Copie `/tmp/opencode/sh2_agents/emetteur_trace.rose` : avant chaque op, l'ELF émis stocke
l'index global de l'op à `0x60003C80` et l'index de fonction à `0x60003C88`.
ELF tracé 591 252 o. Avec `>> 1` en entrée, au crash :

```
0x60003c80 = 0xcb = 203    0x60003c88 = 0x0d = 13
fonction[13] = can_nud ; op 203 (index émetteur, décalé de 1) =
  0001 saut_si_faux 3 (bin == (v kk) (s "v"))
pc = 0x4900f8   ldr x2,[x6,#8]     (entrée de g5_emit_streq)
x0 = 0x1        (kk : chaîne taguée au pointeur nul)
x1 = 0x60004b89 (littéral "v", valide)
x30 = 0x41a120
```

Désassemblage de l'appel (0x41a0f4..0x41a11c) : contrôle des tags puis `bl 0x4900f0`
(`streq`). La valeur `kk` provient de l'op précédente `affect_ast slot 3 = (idx (v k) (v e))`.
Au moment du crash, dans la frame de `can_nud` :

```
[sp+0x10] = 0x60004632   -> k (liste, mask 0x60004630)
[sp+0x20] = 0xfffffffffffffe80  -> e = -384 = -48 << 3
```

`idx(k, -48)` calcule l'adresse `0x60004630 + (-48*8 + 24) = 0x600044a8`, **sous le
descripteur**, et y lit la valeur `1`. D'où `kk = 1`, puis `streq(1, "v")` qui lit `[0+8]`
→ SIGSEGV. **La cause immédiate est l'argument `e = -48` reçu par `can_nud`.**

`e` est calculé dans `post` (op 0002) par `(app entier (app NOD (v r)))`.
`NOD` (`emetteur.rose` ligne 170) fait `sous_chaine($r, $q + 1, $n)` ; quand `$q == $n`,
les bornes sont inversées. Dans le runtime G5, `g5_emit_sous` calcule `len = b - a = -1`,
alloue un descripteur de 16 octets **sans zone de données** et `entier()` lit l'octet
suivant le descripteur. Un octet `0` lu hors descripteur donne `0 - 48 = -48`, la valeur
exactement observée.

## 4. Correctif appliqué dans le fichier possédé

`compilateur/emetteur_elf.rose`, `fn g5_emit_sous` : ajout d'un bornage de la longueur
calculée à 0 quand `b < a`, avant l'allocation du descripteur. Deux labels locaux
(`g5_next()`), `cmpi(3,0)` + branchement `b.lt` vers `movz(3,0)`.
Sauvegarde de l'original : `/tmp/opencode/sh2_agents/emetteur_elf.rose.ORIGINE`
(md5 `a18e4456a4ce96bab2c23a543cd1dda4`).
md5 après correctif : `1feed21b93b00e554f6c83c63b6088cd`. `rose verif` → `vérifié`.

### 4.1 Résultat des deux correctifs (re-mesurés)

| Version | md5 ELF | taille | Exécution sur `compilateur/emetteur.rose` |
|---|---|---:|---|
| originale | `98dadd85…` | 531 428 | rc=139, 0 octet |
| + bornage `sous_chaine` | `1f5d5635…` | 531 444 | rc=139, 0 octet |
| + retour direct `entier("")` | `3dbf56be…` | 531 448 | rc=139, 0 octet |

Les deux correctifs sont **vérifiés présents** dans le code émis (l'ELF a grandi de
16 puis 4 octets) mais **ne changent pas le point de défaillance** : même pc
(`0x481758`, `streq`), même `x0=1`, et surtout même `e=-48` dans la frame de `can_nud`.
Ils ne sont donc pas la cause racine. Je les conserve (ils sont corrects sémantiquement)
mais je ne prétends pas qu'ils réparent le dump.

### 4.2 Comparaison native / émis (preuve de divergence)

Copie instrumentée hors dépôt `/tmp/opencode/sh2_agents/emetteur_native_trace.rose`
(ajout d'un `>> "E=" + chaine($e) + " Q=" + chaine($q)` juste avant l'appel à `can_nud`
dans `post`, source originale intacte) :

```
$ rose emetteur_native_trace.rose /tmp/opencode/sh2_agents/one.rose
E=1 Q=3                      <- natif : e = 1
rir / fonction $principal / ...
```

```
ELF émis, même entrée : e = -384 = -48 << 3   dans la frame de can_nud
```

**Divergence prouvée** : `e = entier(NOD(r))` vaut `1` dans le moteur Rose et `-48`
dans le binaire émis. La faute est donc bien dans la sémantique générée par
`compilateur/emetteur_elf.rose`, pas dans `emetteur.rose`.

### 4.3 Cause racine — TROUVÉE et corrigée

Troisième traçage (`emetteur_trace2.rose`, 590 144 o, 377 s) : l'argument brut passé
à `entier()` est stocké à `0x60003C90`. Mesure au crash :

```
0x60003c90 = 0x60004da9            (chaîne taguée valide)
[0x60004da8] = 0x00000001          (tag = 1)
[0x60004db0] = 0x00000001          (len = 1)
[0x60004db8] = 0x00000000          (UNIQUE octet = NUL)
```

`NOD()` devait rendre `"1"` ; elle rend `"\0"`. La faute est dans `g5_emit_sous` :

```rose
g5_emit(orrr(8, 31, 1))     # MOV x8,x1 = a (début), PAS un tag
...
g5_emit(addr(7, 8, 6))      # x7 = a + i
g5_emit(ldrbr(7, 10, 7))    # ldrb w7,[x10,x7] -> [pointeur + a + i]
```

Les données d'un descripteur Rose commencent à **+16**. L'ancien code lisait donc
16 octets trop tôt, dans l'en-tête `[tag][len]` — d'où des zéros à la place des
caractères. Preuve directe, avant correction :

```
sous_chaine("abcdef",2,5)      natif "cde"   ELF "abc"
entier(sous_chaine("91|5",3,4)) natif 5      ELF 9
```

**Correctif appliqué** dans `g5_emit_sous` : `x7 = 16 + a + i` au lieu de `a + i`
(`addi(7,6,16)` puis `addr(7,8,7)` avant le `ldrbr`). Après correction, les deux
tests ci-dessus donnent **"cde" et 5**, identiques au natif. md5 du fichier livré
après ce correctif : `11f291a9fdec3093ae1521dc09faca41` ; `rose verif` → `vérifié`.
Les deux correctifs défensifs des §4 et §4.1 sont conservés (ils sont sémantiquement
justes) mais n'étaient pas la cause.

### 4.4 Second défaut — `joindre` ne mutait pas la variable appelante

Après le correctif `sous_chaine`, l'ELF réémis (`8ee0b51ba3c5…`, 531 452 o) échoue
encore, mais **plus tard** : le tas s'épuise encore, maintenant dans `$principal`
à l'op `0545 affect_ast slot 75 = (bin + (v topb) (s ","))`.

gdb avec point d'arrêt sur le `cat` (processus patché à 16 Gio pour ralentir) :

```
HIT 1: x0=0x60005f99  topb_len=3
HIT 2: x0=0x60006921  topb_len=4
...                         (topb grandit de 1 octet par itération)
p(slot76) = 3, 4, 5, 6, 7, ...   (p AVANCE, la boucle n'est pas sur p)
```

À la première itération, `$gen` (slot 18) vaut :

```
gen ptr = 0x60004062 -> tag=2, len=1, cap=1, unique élément 0x60004001
```

`$gen` devrait contenir tous les jetons puis `"eof"` ; il reste à **1 élément**.
Cause : Rose appelle `joindre($gen, …)`
**sans réaffecter** (`emetteur.rose` ligne 3808 `joindre($gen, "eof")`, et les
`joindre($gen,…)` du lexeur) parce que `joindre` mute la liste en place. Le runtime
G5, lui, **réalloue** dès que `cap` est atteinte (cap initiale 8 pour `lstvide`,
exacte pour `(lst …)`) et renvoie un nouveau pointeur que l'appelant ignore.

**Correctif appliqué** : dans `g5_eval`, cas `app joindre`, si le premier argument
est `(v nom)` et que `nom` est un slot de la fonction, on émet un
`str x0,[sp,slotoff(nom)]` juste après le `bl` vers la routine `joindre`. Ainsi
la mutation en place est reproduite sans toucher à `emetteur.rose`.
Preuve sur un programme isolé : 20 `joindre` sur une liste → `longueur = 21`,
`l[19] = "18"`, identique au natif.

md5 du fichier livré après ce correctif : `4e0d460fa157a209bf758ec1128b6ab0`,
`rose verif` → `vérifié`.

### 4.5 Troisième défaut — appels à 36 arguments

L'ELF réémis (`1743fdf0…`, 532 116 o) échoue encore, mais à un **nouveau** point :

```
pc = 0x40de34   ldr x1,[x0,#1376]
x0 = 0x300000000000000   x30 = 0x60005819   [0x60000000]=0x60005838
```

Séquence `ldr x28,[sp,#1328] ; ldr x29,[sp,#1336] ; ldr x30,[sp,#1344] ;
ldr xzr,[sp,#1352] ; ldr x0,[x0,#1368]` : chargement des arguments d'un appel.
Le RIR contient des appels à **36 arguments** (`parcourir`), or l'encodage AArch64
n'a que 5 bits de registre : `rd = i % 32`, donc `x31 = xzr`, `x32 = x0`,
`x33 = x1` — l'argument 0 et `x30` (LR) sont écrasés. C'est le défaut
« appels à 22 arguments » des notes précédentes, ici mesuré à 36.

**Correctif appliqué** : les arguments d'indice ≥ 31 sont passés par une zone RW
fixe à `0x60002400` (juste au-dessus de la table de caractères `0x60001300`,
sous le tas `0x60004000`) ; le prologue de l'appelé les relit immédiatement dans
ses slots. Aucune imbrication n'est possible entre l'écriture et la lecture, car
le prologue s'exécute avant tout autre appel. Un registre scratch `x14` (et non
`x0`) sert à charger la valeur, pour ne pas écraser l'argument 0.

Preuve : fonction de 36 paramètres retournant `a36+a32+a1` → natif `69`,
ELF **`69`** (avant correctif : `104`).

md5 du fichier livré après ce correctif : `d031e6bf6fc06c0a524cac55d30dd776`,
`rose verif` → `vérifié`.

### 4.6 Bug introduit puis corrigé dans le correctif 4.5

L'ELF réémis (`5bd7cafd…`, 532 836 o) échoue dans `g5_emit_idx` avec
`x0 = 0x8effd620e83848cc` (valeur aléatoire) : le slot 11 (`ino`) de
`check_instr`, fonction à **35 paramètres**, était corrompu. Cause : ma boucle
4.5 utilisait x14/x15 comme scratch **après** avoir chargé x0..x30, écrasant
les arguments 14 et 15. Correctif : stocker les arguments ≥31 **avant** de
charger x0..x30. Test conservé : fonction 36 params → `69` des deux côtés.
md5 du fichier livré : `72575f51db0d62ab0582439c84bc09ef`, `vérifié`.

## 4.7 État final — porte NON atteinte

Dernière réémission complète (`b37f50d2…`, 532 836 o, 370 s, 00:27:52 →
00:34:02+01:00) : **rc=139, 0 octet**, `d41d8cd98f00b204e9800998ecf8427e`.
Le `dump.out` identique au RIR (`b3ed9262…`) n'est **pas** produit.

Dernier point de défaillance, re-mesuré sous gdb avec `one.rose` (6 octets) :

```
pc  = 0x4818c4   ldr x2,[x6]          (entrée de g5_emit_idx)
x0  = 0x8effd620e83848cc              (conteneur : valeur aléatoire, non taggée)
x1  = 0x8                              (indice 1, tagué)
x6  = 0x8effd620e83848c8
x30 = 0x4655c4
[0x60000000] = 0x600059a8              (tas sain, 25 896 octets utilisés)
```

Au point d'appel `0x4655ac`, les slots lus sont :

```
[sp+0x68] (slot 10) = 0x8effd620e83848cc
[sp+0x68+8] (slot 11) = 0x8effd620e83848cc
[sp+0x78] (slot 12) = 0x60002420      (adresse de la zone d'arguments ≥31)
[sp+0x10] (slot 0)  = 0x8
```

Deux slots consécutifs portent la même valeur invalide ; le slot 12 porte une
adresse de la zone RW d'arguments. La piste la plus probable est une interférence
entre la zone d'arguments ≥31 `0x60002400` et les slots d'une fonction appelée
avec 35 arguments (`check_instr`). **Ce défaut reste ouvert** : il faut un
traçage supplémentaire (index d'op + valeur des slots au moment de l'appel)
pour trancher. Le correctif 4.5 doit être réexaminé : la zone fixe n'est peut-être
pas sûre malgré l'ordre corrigé.

## 5. Ce qui reste documenté (non corrigé ici)

- Stubs `table` / `cle_existe` / `inserer` : sémantique non implémentée.
- UTF-8 : octets manipulés comme points de code.
- Appels à 22 arguments : registres scratch écrasés (déjà noté dans les docs antérieures).
- `rose/rose` exige toujours `cargo build --release` : **0 % de self-hosting réel**.

## 5.1 Verdict final sous-agent

- **Le compilateur généré tourne-t-il ? NON.** `self*.elf` se termine toujours en
  SIGSEGV rc=139, 0 octet, pour toute entrée testée (source `emetteur.rose`, `self.rir`,
  fichier de 6 octets, fichier vide, fichier inexistant).
- **Self-hosting réel : NON.** `rose/rose` exige toujours `cargo build --release`
  (md5 `12eac46007cf0fe9e1aec2dff6fbf665`, inchangé).
- **Progrès réel et vérifié** : quatre défauts d'émission distincts, chacun localisé
  par gdb et corrigé dans `compilateur/emetteur_elf.rose`, avec preuve par test
  minimal natif vs ELF. Le point de crash a reculé à chaque fois
  (`0x481704` → `0x48130c` → `0x481758` → `0x40de34` → `0x4818c4`).
- **Fichier livré** : `compilateur/emetteur_elf.rose`, md5
  `72575f51db0d62ab0582439c84bc09ef`, `rose verif` → `vérifié`.
  Document : `rose/docs/SELF_HOST_SUITE.md`.
- **Aucun commit, aucune publication**, aucun fichier interdit modifié
  (`emetteur.rose` md5 `390335d29fb9cee368b3456dd892e377`,
  `rose/rose` md5 `12eac46007cf0fe9e1aec2dff6fbf665` re-mesurés en fin de chantier).

## 6. Rejouabilité

Tous les artefacts sont dans `/tmp/opencode/sh2_agents/` :
`self.rir`, `self.elf`, `self_instr.elf`, `self_heap256.elf`, `self_heap4g.elf`,
`self_trace.elf`, `emetteur_instr.rose`, `emetteur_trace.rose`, `opmap.txt`,
`gdb_*.log`, `chain.log`, `instr.log`, `heap256.log`, `trace.log`.
