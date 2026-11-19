# G5-IO — vraies entrées/sorties dans l'ELF auto-écrit (constat daté, 19/11/2026)

Agent `io1` (sous-agent). Dépôt `/home/clausake/Stefan/ROSE`, HEAD de départ `f9b86e4`.
**Aucun commit, aucune publication.** Deux fichiers touchés au total :
`compilateur/emetteur_elf.rose` (fichier confié) et ce document.
`rose/rose`, `emetteur.rose`, `run.rs`, `interp.rs`, `PLAN.md` sont inchangés.

Binaire construit pour les mesures, **sans remplacer `rose/rose`** :
`/tmp/opencode/io1_agents/rose` (copie du cargo build isolé
`CARGO_TARGET_DIR=/tmp/opencode/io1_agents/target CARGO_BUILD_JOBS=2 cargo build --release --bin rose`,
md5 `09063cdc1ebf4448133fb3f0bf2006a3`). Le binaire livré `./rose/rose`
avait md5 `12eac46007cf0fe9e1aec2dff6fbf665` au départ.

## 1. Verdict court

**Objectif 1 ATTEINT** : `nb_arguments`, `argument`, `lire_texte`, `ecrire_bytes`/
`ecrire_texte` sont désormais de vraies routines **émises dans l'ELF** par `svc #0`
(openat/read/write/close/fstat). Un ELF G5 lit un vrai fichier passé en argument
et produit une sortie **octet pour octet identique à Rose natif** pour un contenu ASCII.

**Objectif 2 NON ATTEINT** : la chaîne réflexive complète émet bien un ELF du
compilateur (434 640 octets, rc=0, 155,40 s) mais son exécution épuise le tas
(64 Mio, 512 Mio **et 4 Gio**) avant le premier octet de dump : rc=139, 0 octet.

**Verdict self-hosting : INCHANGÉ — NON.** `rose/rose` exige toujours `cargo build`.

## 2. Primitives émises

| Primitive Rose | Routine émise | Syscalls aarch64 |
|---|---|---|
| `nb_arguments()` | `g5_emit_nbargs` | lecture de `argc` sur la pile initiale, moins 1 |
| `argument(i)` | `g5_emit_argument` | `argv[i+1]` + `strlen` + copie en descripteur tagué |
| `lire_texte(p)` | `g5_emit_lire` | `openat(56)`, `fstat(80)`, `read(63)`, `close(57)` |
| `ecrire_bytes(p,d)` / `ecrire_texte(p,d)` | `g5_emit_ecrire` | `openat(56, O_WRONLY\|O_CREAT\|O_TRUNC=577, 0644)`, `write(64)`, `close(57)` |
| `>> x` / `afficher` | `g5_emit_print` (réécrit) | `write(64)` vers stdout, puis `write` d'un `\n` |

Détails d'implémentation :

- **Capture argc/argv** : insérée dans `g5_compile_fonction` **avant** le
  `sub sp,sp,#9232` du `$principal`. `argc` brut est sauvé à `0x60000010`,
  l'adresse de `argv[0]` à `0x60000018`. Le pointeur libre du tas passe de
  `0x60000008` à **`0x60000300`** pour laisser ces cases intactes.
- **Cartographie RW** (`PT_LOAD` à `0x60000000`) :
  `0x60000000` pointeur libre ; `0x60000010` argc ; `0x60000018` argv ;
  `0x60000020` tampon de chemin (NUL terminal) ; `0x60000200` `struct stat`
  (`st_size` à +48) ; `0x60000300` début du tas.
- **`argument(i)`** lit l'argument utilisateur `i` (Rose `argument(0)` = `argv[1]`,
  vérifié dans `interp.rs` : `self.arguments` exclut le nom du programme).
- **`lire_texte`** alloue exactement `16 + 8*ceil(st_size/8)` octets, lit en boucle
  jusqu'à `st_size`, stocke la **longueur réellement lue** et renvoie `ptr|1`.
  Ouverture impossible → chaîne vide (comportement Rose).
- **`afficher` réécrit** : la version HEAD écrivait les chiffres et les chaînes
  à des **adresses absolues 16 et 64** (`movz x7,#16`, `movz x5,#64`) au lieu
  du cadre : **bug préexistant**, rejoué avec le binaire HEAD sur
  `>> "BONJOUR"` → rc=139. Les conversions utilisent maintenant `sp+64` /
  `sp+128`, et les chaînes sont écrites **directement par `write` par blocs**
  (une copie dans `sp+64` débordait le cadre de 9 232 o au-delà de ~9 Ko).

## 3. Preuves d'exécution (ELF G5 avec un vrai fichier en argument)

Programme `/tmp/opencode/io1_agents/t_io.rose` :

```
>> nb_arguments()
$chemin = argument(0)
>> $chemin
$txt = lire_texte($chemin)
>> longueur($txt)
>> $txt
```

Chaîne de mesure (binaire isolé `R`, répertoire `W`) :

```
R rir W/t_io.rose > W/t_io.rir                       # 347 octets
R compilateur/emetteur_elf.rose W/t_io.rir > W/t_io.elf   # rc=0, 2 532 octets
chmod +x W/t_io.elf ; W/t_io.elf W/petit.txt
# 1
# /tmp/opencode/io1_agents/petit.txt
# 16
# BONJOUR_IO_2026
# rc=0
```

- `W/petit.txt` = `BONJOUR_IO_2026\n` (16 octets).
- **Sortie ELF = sortie Rose natif**, `diff -q` silencieux, md5 identique
  `24003210c4a9b595b6e08ae7ac41b5af` des deux côtés.
- **Gros fichier** : `W/t_io.elf W/fp.rir` (263 065 octets) → rc=0, 263 107
  octets de sortie, contenu du RIR reproduit intégralement (la différence
  avec le natif porte uniquement sur la ligne `longueur`, voir §5).
- **`ecrire_texte` validé** par un test dédié (`W/t_write.rose`) : l'ELF écrit
  `ECRIT_PAR_ELF\n` (14 octets) dans un vrai fichier ; md5 du fichier produit
  `9a7b58ebb6d5d85d942370f3a024af2b`, **identique** à celui produit par Rose
  natif. Deux bogues corrigés au passage : (a) `openat` écrasait `x1` (les
  données) — sauvegarde dans `x11` ; (b) le registre de count du syscall
  `write` est `x2`, pas `x3`. `ecrire_bytes` partage exactement la même routine
  émise, mais n'est pas exercé séparément. Ces primitives ne figurent pas dans
  le RIR de `emetteur.rose` (0 occurrence) : elles ne changent donc rien à la
  chaîne réflexive §4.

## 4. Chaîne réflexive — résultat réel

```
R rir compilateur/emetteur.rose > W/fp.rir
   -> 263 065 octets, md5 8ac53eed3d4967a0d7d9ab333256801c
R compilateur/emetteur_elf.rose W/fp.rir > W/self.elf
   -> rc=0, 434 640 octets, md5 61b0a901fc9d7425ade83ff1342e7011, 155,40 s
chmod +x W/self.elf
W/self.elf compilateur/emetteur.rose > dump   -> rc=139, 0 octet
W/self.elf W/fp.rir                    > dump -> rc=139, 0 octet
W/self.elf W/empty.rose               > dump -> rc=139, 0 octet
```

Le dump attendu `8ac53eed…` n'est **jamais** produit. Le md5 de la sortie est
`d41d8cd98f00b204e9800998ecf8427e` (fichier vide) dans les trois cas.

**Tas** (`p_memsz` du second `PT_LOAD`, patché directement dans l'en-tête
sans réémettre, offset fichier 160) :

Mesures rejouées avec **la même source vide** (`W/empty.rose`), en ne patchant
que `p_memsz` dans l'en-tête de `W/self.elf` :

| `p_memsz` | durée | résultat |
|---|---|---|
| 64 Mio (valeur livrée) | 1,09 s | rc=139, 0 octet |
| 512 Mio | 1,31 s | rc=139, 0 octet |
| 4 Gio | 2,91 s | rc=139, 0 octet |

Le tas est épuisé **dans les trois cas** : ce n'est pas un manque de place
pour un dump de 263 Ko. Ajouter de la mémoire ne repousse que le même mur
(constat identique à `G5_ABI_VALEURS.md` §5). Le temps mesuré augmente avec la
taille du tas mais **pas de façon linéaire** (×8 en taille → ×1,2 puis ×2,2 en
durée) : je ne conclus rien de plus fin sur la cadence d'allocation.

**Points de crash gdb** (tous dans des allocations inline, `str x1,[x0]`) :

- `pc=0x400560`, `x0=0x64000028`, `x30=0x400538` : alloc 24 o du stub `table()`
  (op 0085 du `$principal`) avec `compilateur/emetteur.rose` en argument ;
- `pc=0x410bec`, `x0=0x64000000`, `x30=0x40fce4` : alloc 24 o (`idx` d'un
  caractère) avec une source vide.

L'ELF émis **démarre donc correctement** (les primitives d'E/S répondent :
`nb_arguments()` renvoie 1, `argument(0)` le chemin, `lire_texte` le contenu)
et progresse dans le compilateur avant d'épuiser le tas. Le point de défaillance
a été déplacé de « aucune source » (stubs) vers « boucle d'allocation interne ».

Trace complémentaire (point d'arrêt matériel sur `[0x60000000]`, seuil 64 Kio,
même ELF, source vide) : le premier franchissement se produit à
`pc=0x443e64` avec `x1=0x18` (24 octets), `tag=1`, `len=1`, octet `0x3d` (`=`)
écrit à l'offset 16 du descripteur, et `x30=0x443e40` — c'est le motif
d'allocation d'un descripteur de **un
caractère** dans la boucle de tokenisation (`RIR` op 0415/0416, `joindre` de
`"="` dans `$principal`). Le même motif se répète jusqu'à la fin du tas.
**Nombre d'appels** : avec un tas de 64 Mio épuisé par des blocs de 24 octets,
cela représente **≈ 2,8 millions d'appels à l'allocateur inline** (estimation
arithmétique `67108864/24`, pas un compteur instrumenté — une instrumentation
par labels uniques a été tentée mais rendait `g5_patch` trop lent et a été
interrompue ; je ne la présente pas comme une preuve).

## 5. Frontière restante

1. **Boucle d'allocation** (§4) — premier obstacle exécutable.
2. **Sémantique de valeurs** : la chaîne G5 compte les **octets**, Rose compte les
   **points de code UTF-8**. Preuve mesurée : `lire_texte` d'un fichier de 3 o
   (`é\n`) donne `longueur = 2` dans Rose et `3` dans l'ELF ; sur `fp.rir`,
   Rose donne `262 976` et l'ELF `263 065` (164 octets non ASCII dans le RIR).
   Même sans la boucle d'allocation, un dump bit-à-bit identique exigerait des
   primitives `longueur`/`idx`/`ordre` UTF-8-conscience.
3. **Tables** : `table`, `inserer`, `cle_existe` restent des stubs neutres ;
   `cle_existe` renvoie 0, donc tous les mots-clés sont classés « ident ».
4. **Appels à plus de 8 arguments** : `em_fonction_rir` est appelé avec 22
   arguments dans le `$principal` ; le chargement `x0..x21` écrase `x8`/`x9`
   (numéro de syscall / base du tas). Non traité ici.
5. **Réflexivité de l'émetteur lui-même** non retentée après ce chantier
   (elle exigerait en plus `ecrire_bytes` depuis l'ELF et les points 2–4).

## 6. Validation horizontale (constats)

```
rose verif compilateur/emetteur_elf.rose            -> vérifié
fib(10) -> RIR -> ELF                               -> 55, rc=0
BASE("a|b") -> longueur -> ELF                      -> 1, rc=0
point fixe M4.5 : rir emetteur.rose ×2              -> 8ac53eed… ×2, cmp_rc=0
cargo test --release (target dir isolé)             -> 220+204+6+1 = 431, 0 échec
rose/rose jardins/tests/premiers.rose               -> 20 tests, 0 échec
ELF G5 + vrai fichier                               -> sortie = Rose natif (md5 égal)
ELF G5 + ecrire_texte                               -> fichier = Rose natif (md5 égal)
```

## 7. Ce que je ne dis pas

Je ne dis **pas** « full hosted », « bootstrap » ni « self-hosting » : l'ELF du
compilateur ne produit aucun dump et `rose/rose` nécessite toujours cargo.
Je ne dis pas que `ecrire_bytes` a été validé séparément de `ecrire_texte`.
Je ne dis pas que la boucle d'allocation de §4 est corrigée : elle ne l'est pas.
