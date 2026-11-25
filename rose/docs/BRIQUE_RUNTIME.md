# BRIQUE_RUNTIME — runtime nu + asm inline générique, en Rose pur

**État livré :** `compilateur/brique_runtime.rose` (bibliothèque Rose pure).
Aucun commit, aucun push. Worktree dédié `/tmp/opencode/agent_runtime`,
HEAD de départ `1708a4bf4bd4810244c2a8e3fb89f49f766d45f2`.

---

## 1. Philosophie — une brique, pas une recette OS

Zig est le modèle : **pas de libc par défaut**, démarrage contrôlé, std
compilée avec le programme. Ici, la « std » est un fichier Rose de
fonctions indépendantes que l'on concatène avec son programme :

```bash
cat compilateur/brique_runtime.rose mon_programme.rose > /tmp/prog.rose
/tmp/opencode/C0_h.elf /tmp/prog.rose > /tmp/prog.elf     # ELF64 aarch64
chmod +x /tmp/prog.elf && /tmp/prog.elf
```

Chaque fonction est une **brique** : elle ne sait rien du noyau, du
chargeur, des interruptions, des registres d'abord ni d'un éventuel
système de fichiers. Les seules dépendances sont les primitives déjà
émises par le compilateur Rose (`ecrire_bytes`, `quitter`, `joindre`,
`inserer`, `table`, `sous_chaine`…), c'est-à-dire le runtime G5 lui-même.
**Aucune dépendance libc, aucun appel C, aucun `std::`.**

Rose n'a pas (encore) de système de modules (`importer`). Le motif de
concaténation est déjà utilisé dans le dépôt, voir
`compilateur/ffi_syscalls.rose:6-9` :

> « Fichier SEPARÉ autorisé par la mission. Se concatène avec
> `ffi_rose.rose` : `cat compilateur/ffi_rose.rose
> compilateur/ffi_syscalls.rose compilateur/tests_ffi_rose.rose
> > /tmp/ffi/prog.rose`. »

### Ce qui existait avant (constat daté, HEAD `1708a4b`)

| Besoin | Existant | Fichier:ligne |
|---|---|---|
| encodeurs asm aarch64 | `movz`, `movk`, `addi`, `subi`, `addr`, `subr`, `cmpi`, `strboff`, `RET_I`, `SVC_I`… | `compilateur/emetteur_elf.rose:717-757` |
| assemblage ELF | `g5_assemble()` + en-têtes, segment RW (128 Gio de `.bss` virtuel) | `compilateur/emetteur_elf.rose:4669`, `:4690-4742` |
| encodeurs asm x86-64 | `xb`, `x_rr`, `x_mov`, `x_add`, `x_movimm`, `x_prologue`, `x_epilogue` | `compilateur/emetteur_pe_x64.rose:220`, `:318-372` |
| enveloppe PE | `x_pe()` | `compilateur/emetteur_pe_x64.rose:2201` |
| panic Rust | `erreur_runtime` → `panic!("ROSE_ERR:…")` | `rose/src/run.rs:1261-1267` |
| panic no_std | `#[panic_handler]`, write fd 2, `exit_group(101)` | `rose/src/bin/rose_nostd.rs:74-79` |
| hook panic moteur | masque les `ROSE_ERR`, garde les vrais bugs | `rose/src/main.rs:1350-1353` |
| sortie sans libc | syscalls bruts `openat/read/write/mmap`, **aucun asm inline nécessaire** | `rose/src/sys_io.rs:1-33` |
| écriture G5 | `ecrire_bytes` → `write(2)`, `quitter` → `exit` | `compilateur/emetteur_elf.rose:2172`, `:2195`, `:3771`, `:4472` |
| amorçage no_std | allocateur global, `extern "C" main`, `rust_eh_personality` | `rose/src/bin/rose_nostd.rs:28-37`, `:53-83` |
| docs associées | `NO_STD.md`, `NO_STD_CHECKLIST.md` | `rose/docs/NO_STD.md:1-25` |

**Manquant avant ce chantier :** aucun `memcpy`/`memset`/`panic`/startup
*générique et réutilisable* écrit en Rose. Il n'existait que des primitives
Rust internes, non callables depuis un programme Rose :
`rose/src/run.rs:2083-2088` (CUDA `cuMemcpyHtoD`/`cuMemsetD8`) et
`rose/src/transpile.rs:1207-1212` (`memcpy`/`memset` du runtime C émis).

---

## 2. API livrée

### 2.1 Primordia

| Fonction | Rôle |
|---|---|
| `rt_octet(v: entier) -> entier` | normalise un entier en octet `0..255` |
| `rt_sans_porteur(l: liste<entier>) -> liste<entier>` | retire la case porte-type d'une liste |
| `rt_ajoute(dst, src) -> entier` | ajoute tous les octets de `src` à `dst` (mutateur) |

### 2.2 Copie mémoire

```rose
fn rt_memcpy(src: liste<entier>, dst: liste<entier>, n: entier) -> liste<entier>
fn rt_memset(dst: liste<entier>, val: entier, n: entier) -> liste<entier>
fn rt_memcpy_tab(src: table, dst: table, n: entier) -> entier
fn rt_memset_tab(dst: table, val: entier, n: entier) -> entier
```

Un tampon d'octets est une `liste<entier>` dont chaque case vaut `0..255`.
**Convention du porte-type :** Rose n'a pas de littéral `[]`, donc toute
liste est créée avec au moins une case. `dst` est ce porteur : une liste
d'au moins une case, dont le **contenu est ignoré**. Le résultat est une
liste neuve contenant exactement `n` octets.

```rose
$src = [10, 20, 30, 40, 50]
$copie = rt_memcpy($src, [0], 3)   # -> [10, 20, 30]
$zero  = rt_memset([0], 255, 4)    # -> [255, 255, 255, 255]
```

`n = 0` rend le porteur inchangé (une case) : c'est le plus petit tampon
typable en Rose, documenté plutôt que simulé.

Les variantes `_tab` font une copie **vraiment en place** dans une table
à clés entières `0..n-1` ; `dst` est muté, aucun porteur n'est nécessaire.

### 2.3 Panique déterministe

```rose
fn rt_panic(msg: chaine)
fn rt_panic_code(msg: chaine, code: entier)
```

Écrit sur `/dev/stderr` avec `ecrire_bytes` puis `quitter(101)` (ou `code`).
**Pourquoi pas `afficher_erreur` ?** mesuré : c'est un stub dans l'ELF G5
(il n'écrit rien), alors qu'`ecrire_bytes` est réellement émis. La sortie
est donc déterministe et identique sous l'interpréteur et sous l'ELF.

### 2.4 ASM inline générique

```rose
fn rt_asm(ops: liste<chaine>) -> liste<entier>            # aarch64 / ELF
fn rt_asm_pe(ops: liste<chaine>) -> liste<entier>         # x86-64 / PE
fn rt_asm_vers(ops: liste<chaine>, cible: chaine) -> liste<entier>
```

`ops` est une liste de mnémos dont les arguments sont séparés par des
blancs. Le résultat est une liste d'**octets** : chaque mot aarch64 est
sérialisé en 4 octets little-endian (`rt_u32le`, miroir de `u32le` de
`emetteur_elf.rose:436`), chaque instruction x86-64 est déjà une suite
d'octets.

```rose
$elf = rt_asm(["movz 0 5", "addi 0 0 1", "ret"])   # 12 octets
$pe  = rt_asm_pe(["movimm 0 42", "ret"])           # 11 octets
```

Mnémos aarch64 : `movz`, `movk`, `addi`, `subi`, `addr`, `subr`, `andr`,
`orrr`, `eorr`, `umulh`, `mulr`, `sdivr`, `udivr`, `cmpr`, `cmpi`,
`ldroff`, `stroff`, `strboff`, `ldrboff`, `cset`, `ret`, `svc`.

Mnémos x86-64 : `mov`, `add`, `sub`, `cmp`, `movimm`, `addi`, `subi`,
`ret`.

**Délégation aux émetteurs existants.** Rose n'a pas de module : `rt_asm`
reproduit exactement les formules d'encodage de
`compilateur/emetteur_elf.rose:717-757` et
`compilateur/emetteur_pe_x64.rose:318-372`, qui restent la source de
vérité. Les deux niveaux sont distincts et complémentaires :

- `rt_asm` = niveau **instruction** (un mot ou quelques octets) ;
- `emetteur_elf.rose:4669` `g5_assemble()` / `emetteur_pe_x64.rose:2201`
  `x_pe()` = niveau **image binaire** (en-têtes, sections, relocations).

Pour produire un exécutable complet, compiler le programme avec C0 ;
pour injecter une instruction brute, utiliser `rt_asm`.

### 2.5 Traders de démarrage génériques

```rose
fn rt_bss_nouvelle() -> table              # table vide, prête à être remplie
fn rt_init_bss(t: table) -> entier         # modèle logique « zéro »
fn rt_bss_ecrire(t: table, idx: entier, v: entier) -> entier
fn rt_init_stack() -> liste<entier>        # base de pile typée [0]
fn rt_stack_push(st: liste<entier>, v: entier) -> liste<entier>
fn rt_stack_top(st: liste<entier>) -> entier
```

```rose
$bss = rt_bss_nouvelle()
rt_init_bss($bss)
rt_bss_ecrire($bss, 2, 300)     # -> 44 (300 mod 256)
$pile = rt_init_stack()
$pile = rt_stack_push($pile, 7)
$pile = rt_stack_push($pile, 8)
>> rt_stack_top($pile)          # 8
```

Ces traders sont des **modèles logiques** : aucun mapping mémoire, aucune
adresse, aucun chargeur n'est supposé. Un OS ou un noyau peut les
substituer par une vraie section `.bss` et une vraie pile machine sans
changer l'API du programme.

---

## 3. Contraintes mesurées (à respecter en écrivant une brique)

Ces points ont été trouvés en compilant la brique successivement avec le
moteur Rust et avec C0 ; ils sont des **constats datés**, pas des règles
universelles, mais ils sont vérifiés sur HEAD `1708a4b`.

1. **`i` minuscule = imaginaire.** Hors `pour i de …`, nommer les
   compteurs `$k`, `$idx`… Un paramètre nommé `i` casse la vérification
   (« Comparaison entier == complexe impossible »).
2. **`[]` n'existe pas** comme littéral ; `retour [0]` direct casse le
   parseur Rose. Passer par une variable : `$b = [0]; retour $b`.
3. **Une table lue est typée `?`** par le vérificateur : ne pas la passer
   à un paramètre `entier`, ni la déclarer comme retour `entier`.
4. **Le backend G5 ne met PAS à jour une clé de table entière déjà
   présente** (mesuré : `inserer(t,2,44)` après un `2` initialisé à 0
   laisse `0`). `rt_init_bss` ne pré-remplit donc pas ses cases.
5. **Une table remplie puis retournée par une fonction perd son contenu**
   sous G5. La table est fournie par l'appelant et mutée sur place.
6. **L'affectation d'une globale depuis une fonction n'est pas fiable**
   (Rust et G5). La cible de `rt_asm` est donc un argument explicite.
7. `afficher` et `afficher_erreur` sont des stubs en ELF G5 : utiliser
   `>>` pour les valeurs et `ecrire_bytes` pour les diagnostics.
8. `quitter(code)` est réellement émis (`g5_emit_quit`) : code de retour
   fiable des deux côtés.

---

## 4. Validation (preuves de sortie réelles)

Commandes exécutées depuis le worktree, avec
`C0 = /tmp/opencode/C0_h.elf` (md5 `2db168b907b4a874712857d3957fd078`) :

```bash
cd /tmp/opencode/agent_runtime
/home/clausake/Stefan/ROSE/rose/rose verif compilateur/brique_runtime.rose
# -> vérifié

$C0 compilateur/brique_runtime.rose > /tmp/opencode/brique_runtime.elf
od -An -tx1 -N4 /tmp/opencode/brique_runtime.elf     # -> 7f 45 4c 46
od -An -tu2 -j18 -N2 /tmp/opencode/brique_runtime.elf # -> 183 (aarch64)
```

Pilote de test (hors worktree, `/tmp/opencode/pilote_rt.rose`) concaténé à
la brique puis compilé :

```bash
cat compilateur/brique_runtime.rose /tmp/opencode/pilote_rt.rose \
  > /tmp/opencode/pilote_complet.rose
$C0 /tmp/opencode/pilote_complet.rose > /tmp/opencode/pilote.elf
chmod +x /tmp/opencode/pilote.elf && /tmp/opencode/pilote.elf
ROSE_SANS_JIT=1 rose/rose /tmp/opencode/pilote_complet.rose
```

Résultat : **sorties stdout byte-for-byte identiques** (17 lignes), rc=0
des deux côtés. Le pilote couvre `rt_memcpy`, `rt_memset`, les variantes
`_tab`, `rt_asm` (ELF et PE), `rt_init_bss`/`rt_bss_ecrire`,
`rt_init_stack`/`rt_stack_push`/`rt_stack_top`.

Test de panique dédié : `rt_panic("panique voulue")` écrit exactement
`PANIC Rose : panique voulue\n` sur stderr et rend **rc=101** sous
l'interpréteur comme sous l'ELF.

---

## 5. Dépendances

**Aucune.** Pas de libc, pas de `std`, pas de FFI C, pas de helper Rust.
Seules les primitives du langage Rose déjà émises par le compilateur C0
sont utilisées. Le fichier est autonome et se concatène à n'importe quel
programme Rose.
