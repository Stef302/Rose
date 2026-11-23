# J4 — JIT sans Rust : syscalls G5 + AOT à la volée

**Constat daté du 23/11/2026**, mesuré dans le worktree
`/tmp/opencode/opt_jit` (HEAD `62ab54c`). Toutes les valeurs ci-dessous sont
des sorties réellement exécutées ; elles doivent être re-mesurées après tout
changement de l'émetteur ou de la topologie.

## 1. Problème de départ (§7 de `SELFHOST_EXTENSION.md`)

Le runtime G5 émis par `compilateur/emetteur_elf.rose` ne connaissait que
`x8 ∈ {0, 56, 57, 63, 64, 80, 93}` (`read`, `openat`, `close`, `getrandom`,
`write`, `fstat`, `exit`). Ni `mmap` (222), ni `mprotect` (226) : un code
machine généré à l'exécution ne pouvait donc ni être placé en mémoire
exécutable, ni être lancé. Le JIT Cranelift (`jit.rs`) restait le seul chemin.

## 2. Syscalls ajoutés au runtime G5

Toutes les routines sont **émises dans chaque ELF** produit par
`emetteur_elf.rose` (comme `g5_emit_lire` / `g5_emit_ecrire`), sans toucher
aux optimisations mémoire / `idx` / `octet_brut` existantes.

| Nom Rose | Syscall aarch64 | N° | Arguments | Retour |
|---|---|---|---|---|
| `mmap(addr,len,prot,flags,fd,off)` | `mmap` | **222** | 6 entiers tagués | adresse (ou `-errno`) taguée |
| `mprotect(addr,len,prot)` | `mprotect` | **226** | 3 entiers tagués | `0` (ou `-errno`) tagué |
| `chmod(chemin,mode)` | **`fchmodat`** | **53** | chaîne + entier | `0` (ou `-errno`) tagué |
| `execve(chemin)` | `execve` | **221** | chaîne | ne revient pas ; `-errno` sinon |

Routines : `g5_emit_mmap`, `g5_emit_mprotect`, `g5_emit_chmod`,
`g5_emit_execve`. Motif identique aux routines existantes : `asr #3` sur
chaque argument entier (ABI G5 `n<<3`), `movz(8, n°)`, `svc #0`, `lsl #3`
sur `x0`, `ret`. `chmod`/`execve` recopient la chaîne Rose dans le tampon RW
`0x60000020` et ajoutent le NUL, exactement comme `g5_emit_lire` le fait pour
`openat`. `execve` construit `argv = {chemin, NULL}` et `envp = {NULL}` dans
le trou RW libre `0x60005000..0x60008000`.

**Piège matériel mesuré** : aarch64 n'a **pas** de syscall `chmod` —
`__NR_capget = 90` (le premier essai a appelé `capget`, strace :
`capget(...) = -1 EINVAL`). Le bon appel est `fchmodat(AT_FDCWD, chemin, mode,
0)` = **53** (`__NR_fchmod = 52`, `__NR_fchmodat = 53` dans
`asm-generic/unistd.h`). Le nom Rose reste `chmod`, l'appel noyau est
`fchmodat`.

Preuve d'émission (ELF produit par l'émetteur modifié, désassemblé en binaire
brut) :

```
mov x8, #0xde   // 222 mmap
mov x8, #0xe2   // 226 mprotect
mov x8, #0x35   //  53 fchmodat
mov x8, #0xdd   // 221 execve
```

Le nombre de globales G5 du composite passe de **405 à 409** (borne 512),
donc sans débordement.

## 3. Mécanisme AOT à la volée

`compilateur/test_jit_g5.rose` tourne **dans un ELF produit par C0**, donc
sans moteur Rust pour l'exécution du code généré :

1. `mmap(0, 4096, RW, MAP_PRIVATE|MAP_ANONYMOUS, -1, 0)` — page de travail ;
2. `mprotect(page, 4096, PROT_READ|PROT_EXEC)` — bascule W^X ;
3. construction **octet par octet en Rose** d'un ELF64 aarch64 minimal :
   `ET_EXEC`, `EM_AARCH64`, un `PT_LOAD` RX à `0x400000`, entrée `0x400078` ;
   ses 33 mots de code calculent `fib(10)` itérativement, convertissent le
   résultat en décimal (`udiv`/`msub` par 10), écrivent sur `stdout` via
   `write(64)` puis `exit(93)` ;
4. `ecrire_bytes("/tmp/j4_jit_fib10.elf", elf)`, `chmod(..., 0755)`,
   `execve(...)` — l'image du processus est **remplacée** par le binaire
   généré à la volée.

Les quatre noms `mmap`, `mprotect`, `chmod`, `execve` sont déclarés comme
`fn` fictives dans la source : le front-end Rose les accepte, et le dispatch
G5 intercepte leur nom **avant** la recherche de fonction utilisateur
(le même motif que `octet_brut`). Les fonctions fictives sont émises mais
jamais appelées.

**Pourquoi `execve` suffit (pas besoin de `blr`)** : le code généré est un
ELF autonome ; `execve` le charge comme une nouvelle image processus. On n'a
donc pas besoin d'un appel indirect à une adresse mémoire (qui aurait exigé
une routine supplémentaire `blr`). `mprotect` est tout de même exercé pour
de vrai (bascule W^X sur la page mappée), ce qui prouve que le syscall 226
est émis et accepté par le noyau.

Un détail G5 important : `longueur(chaine)` **compte les points de code UTF-8**,
pas les octets. Les octets ≥ 0x80 du code machine (`0x8b`, `0xaa`, `0xd2`…)
faussaient le `p_filesz` (219 au lieu de 252). La taille est donc calculée
comme `codeoff + 4 * longueur($mots)`, jamais `longueur($code)`.

## 4. Preuves d'exécution

Chaîne de production (garde-fou mémoire à chaque étape) :

```bash
rose/rose rir compilateur/test_jit_g5.rose > /tmp/opencode/j4_test.rir
rose/rose compilateur/emetteur_elf.rose /tmp/opencode/j4_test.rir > /tmp/opencode/j4_test.elf
chmod +x /tmp/opencode/j4_test.elf
/tmp/opencode/j4_test.elf          # -> 55
```

`strace -f -e trace=mmap,mprotect,fchmodat,execve /tmp/opencode/j4_test.elf` :

```
execve("/tmp/opencode/j4_test.elf", [...], 0x... /* 41 vars */) = 0
mmap(NULL, 4096, PROT_READ|PROT_WRITE, MAP_PRIVATE|MAP_ANONYMOUS, -1, 0) = 0xfb9a8bf23000
mprotect(0xfb9a8bf23000, 4096, PROT_READ|PROT_EXEC) = 0
fchmodat(AT_FDCWD, "/tmp/j4_jit_fib10.elf", 0755) = 0
execve("/tmp/j4_jit_fib10.elf", ["/tmp/j4_jit_fib10.elf"], 0x60005010 /* 0 vars */) = 0
55
+++ exited with 0 +++
```

L'ELF généré à la volée est **octet pour octet identique** à un ELF témoin
assemblé une fois avec `as` :

```
md5sum /tmp/j4_jit_fib10.elf /tmp/opencode/j4_tiny.elf
de39c48767f32a2ac45d6ed2b431c46a  /tmp/j4_jit_fib10.elf
de39c48767f32a2ac45d6ed2b431c46a  /tmp/opencode/j4_tiny.elf
```

Le `55` affiché est donc bien produit par le **code machine généré à
l'exécution**, exécuté sans `jit.rs` / Cranelift.

### 4.1 Chaîne intégralement auto-hébergée (aucun Rust à l'émission)

L'émetteur modifié **se re-compile lui-même** : `P0` (ELF émis par le binaire
natif depuis le RIR de l'émetteur) puis `P1 = P0(eelf3.rir)` ont le même
sha256 `a2bac7fcfa6b9162b0741744ec1f84cdd9143a1c29330ea2f8f7a6c2eefe7bcd`
(910 236 o, `cmp -s` muet). En utilisant **P0** — un ELF, pas le binaire
Rust — pour émettre le test J4, la chaîne complète tourne sans Rust à
l'émission ni à l'exécution :

```bash
/tmp/opencode/j4_P0.elf /tmp/opencode/j4_final.rir > /tmp/opencode/j4_P0_test.elf
/tmp/opencode/j4_P0_test.elf          # -> 55
```

`strace` sur ce binaire rend exactement les 4 syscalls de la §4, puis `55`.
La seule étape encore assurée par la graine Rust est la production du RIR
(`rose rir ...`), c.-à-d. le front-end — pas le JIT.

## 5. Mesures de non-régression

| Contrôle | Commande | Résultat |
|---|---|---|
| Corpus 10 programmes | `bash /tmp/opencode/j4_corpus.sh` | **10/10 CONFORMES** (natif == ELF) |
| Batterie AZ | `AZ_OUT=… bash tests_selfhost/lancer_az.sh` | **31/31 CONFORMES**, 0 divergent, 0 bloqué ; comparaisons natif/ELF, RIR, ELF, sortie **31/31** sur les 4 |
| Sonde `idx` (`probe_idx.rose`, 12 cas) | natif vs ELF | `RESULT:5000050000` identique |
| Sonde `u16le(183)` | `header.rose` natif vs ELF | `b7 00` identique |
| Sonde `octet_brut(183)` | `octet_seul.rose` ELF | `b7` (1 octet brut) |
| `fib(10)` sans JIT Rust | pipeline §4 | `55`, rc=0 |
| Syscalls émis | désassemblage de l'ELF émis | `0xde`, `0xe2`, `0x35`, `0xdd` présents |
| Globales G5 | RIR du `$principal` | 409 ≤ 512 |
| Auto-compilation de l'émetteur modifié | `P0(eelf3.rir)` vs `P0` | point fixe, sha256 `a2bac7…7bcd` identique |
| J4 émis par P0 (ELF, sans Rust) | `j4_P0_test.elf` | `55`, 4 syscalls au strace |

Détails machine : `/tmp/opencode/j4_corpus/resultats.tsv`,
`/tmp/opencode/j4_az/resultats.tsv`, `/tmp/opencode/j4_sondes/`.

## 6. Fichiers modifiés / possédés

- `compilateur/emetteur_elf.rose` — 4 routines G5 nouvelles + globals,
  dispatch, réservation de labels, appels d'émission. Aucune optimisation
  existante touchée.
- `compilateur/test_jit_g5.rose` — nouveau programme de démonstration.
- `rose/docs/J4_JIT_G5.md` — ce document.

Interdits inchangés : `rose/src/**`, `compilateur/emetteur.rose`,
`compilateur/rose_natif.rose`, `scripts/**`. Aucun commit/push/publication.
