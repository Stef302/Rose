# Briques universelles — preuve d'exécution multi-cibles (sans émetteur ELF)

*Constat daté : 25/11/2026. Worktree exclusif `/tmp/opencode/v5uni4`, HEAD `a43dab8` (aucun commit, aucun push).*

## 1. Ce qui est démontré

Les briques universelles (`compilateur/backend_universel.rose`, décrit par
`briques/backend/cible.bud`) produisent des **octets exécutables** pour trois
architectures, à partir d'**une seule description abstraite** du calcul. Aucun
émetteur architectural du dépôt (`emetteur_elf.rose`, `emetteur_pe_x64.rose`,
`emetteur_x64.rose`) n'est inclus, appelé ou modifié. Le moteur Rust sert
d'oracle ; le programme de test est aussi compilé par **C0** (compilateur
self-hosté) en ELF autonome, qui écrit les binaires cibles sans passer par un
émetteur ELF du dépôt.

Chaîne mesurée :

```
IR abstrait (bu_operation/bu_sequence/bu_ajouter)
        │  profil = DONNÉES (bu_cible : famille, boutisme, enveloppe…)
        ▼
bu_traduire(seq, cible) ──> octets aarch64 / x86_64 / wasm
        │
        ├─ bu_emettre ──> six binaires bruts + modules wasm
        └─ tm_elf(code, machine) ──> conteneur ELF64 minimal (2 cibles natives)
```

`bu_traduire` ne nomme aucune architecture : la famille d'encodeur vient du
profil, la disponibilité d'une opération de `$bu_encoder_table`, le boutisme de
`bu_profil`. Le conteneur ELF écrit par le test est **120 octets d'en-tête**
(64 ELF + 56 PHDR, un seul `PT_LOAD` R+X, aucune section), avec le numéro de
machine (`183` aarch64 / `62` x86_64) passé en **donnée** à la même fonction.

## 2. Deux calculs utiles, écrits une seule fois

| Suite | IR abstrait | Résultat |
|---|---|---|
| `somme` | `imm r0,0` puis, pour k=1..10 : `imm r1,k` ; `add r0,r0,r1` ; `ret` (21 opérations, boucle déroulée : l'IR n'a pas de saut conditionnel) | **55** |
| `borne` | `imm r0,250` ; `imm r1,10` ; `add r0,r0,r1` ; `imm r2,255` ; `and r0,r0,r2` ; `ret` | **4** (260 & 255) |

Le même jeu d'ids d'opérations alimente les trois traductions. Pour l'exécution
native, la suite est réutilisée avec une sortie `exit` propre à l'ABI :
`syscall(93)` (aarch64, `x0` = statut), `mov rdi,rax` + `syscall(60)` (x86_64).
La suite wasm garde `ret` : le module exporte `f() -> i32`.

## 3. Fichiers ajoutés

| Fichier | Rôle |
|---|---|
| `briques/backend/tests_multi.rose` | 33 assertions : IR, sémantique de référence, traduction des 3 cibles, émission des 6 binaires + 2 conteneurs ELF, exécution des octets x86-64 **dans le programme** (décodeur des motifs émis). Module source à concaténer avec le backend. |
| `rose/docs/BRIQUES_MULTI_CIBLES.md` | ce document |

Aucun fichier suivi n'a été modifié (`git status` = 2 entrées `??`, HEAD inchangé).

## 4. Reproduction complète

```bash
cd /tmp/opencode/v5uni4
mkdir -p /tmp/opencode/multi/out
cat compilateur/backend_universel.rose briques/backend/tests_multi.rose \
  > /tmp/opencode/multi/tests_multi_prog.rose

# Oracle Rust
ROSE_SANS_JIT=1 /home/clausake/Stefan/ROSE/rose/rose interprete \
  /tmp/opencode/multi/tests_multi_prog.rose > /tmp/opencode/multi/multi_nat.txt

# Self-hosté : C0 compile le programme de test en ELF aarch64 autonome
/tmp/opencode/C0_141.elf /tmp/opencode/multi/tests_multi_prog.rose \
  > /tmp/opencode/multi/tests_multi.elf
chmod +x /tmp/opencode/multi/tests_multi.elf
/tmp/opencode/multi/tests_multi.elf > /tmp/opencode/multi/multi_self.txt
diff /tmp/opencode/multi/multi_nat.txt /tmp/opencode/multi/multi_self.txt
```

Résultat :

```
TOTAL 33/33                          (oracle Rust)
TOTAL 33/33                          (ELF C0 self-hosté)
md5 multi_nat.txt   = 90b992507e8284a75e78b2afdebdeb96
md5 multi_self.txt  = 90b992507e8284a75e78b2afdebdeb96
```

Les deux sorties sont **octet pour octet identiques** ; c'est l'ELF C0, pas le
moteur Rust, qui a écrit les dix fichiers dans `/tmp/opencode/multi/out/`.

## 5. Résultats par cible

### 5.1 aarch64 — natif, sans émulateur

```bash
cd /tmp/opencode/multi/out && chmod +x *.elf
./somme.aarch64.elf ; echo "rc=$?"
./borne.aarch64.elf ; echo "rc=$?"
```

```
somme rc=55
borne rc=4
```

L'hôte est `Linux gx10-2 7.0.0-1025-nvidia … aarch64` : ces ELF sont **exécutés
directement par le noyau**, pas interprétés. Preuve complémentaire de
l'encodage (`objdump -D -b binary -m aarch64 out/somme.aarch64.bin`) :

```
38: 8b010000  add  x0, x0, x1
3c: d2800101  mov  x1, #0x8
...
50: 8b010000  add  x0, x0, x1
54: d65f03c0  ret
```

Taille des binaires : `somme.aarch64.bin` 88 o, `borne.aarch64.bin` 24 o ;
conteneurs : `somme.aarch64.elf` 212 o, `borne.aarch64.elf` 148 o.

### 5.2 x86_64 — émulation système + décodage des octets émis

Aucun x86_64 natif sur cet hôte aarch64 ; `qemu-x86_64-static` est disponible :

```bash
qemu-x86_64-static ./somme.x86_64.elf ; echo "rc=$?"
qemu-x86_64-static ./borne.x86_64.elf ; echo "rc=$?"
```

```
somme rc=55
borne rc=4
```

Les ELF x86_64 sont donc **réellement exécutés** (syscall `exit`), sous qemu.
En complément, `tm_exec_x64` dans `tests_multi.rose` décode les octets bruts
produits par `bu_traduire` (motifs `48 B8+r`, `48 8B /r`, `48 01 /r`,
`48 21 /r`, `C3`) et retrouve 55 et 4 **sans qemu, dans le programme**, des
deux côtés oracle Rust et ELF C0 (assertions 24 et 25).

Désassemblage indépendant (Capstone 5.0.7, car l'`objdump` de cette machine est
aarch64-only) de `borne.x86_64.bin` :

```
0: movabs rax, 0xfa
a: movabs rcx, 0xa
14: mov rax, rax
17: add rax, rcx
1a: movabs rdx, 0xff
24: mov rax, rax
27: and rax, rdx
2a: ret
```

Taille des binaires : `somme.x86_64.bin` 171 o, `borne.x86_64.bin` 43 o ;
conteneurs : `somme.x86_64.elf` 305 o, `borne.x86_64.elf` 177 o.

### 5.3 wasm — module valide exécuté par Node

```bash
node /tmp/opencode/multi/run_wasm.mjs
```

```
WASM somme.wasm f() = 55
WASM borne.wasm f() = 4
```

Node 26.12.1 construit `new WebAssembly.Module(...)` puis appelle
`instance.exports.f()` : ce sont bien les **modules émis par le backend** qui
tournent (magie `0061736d01000000`, section type `() -> i32`, export `"f"`).
Tailles : `somme.wasm` 151 o, `borne.wasm` 65 o.

### 5.4 Tableau de parité des résultats

| Calcul | IR (référence Rose) | Octets x86_64 décodés en Rose | aarch64 natif | x86_64 sous qemu | wasm sous Node |
|---|---|---|---|---|---|
| somme 1..10 | 55 | 55 | rc=55 | rc=55 | `f()`=55 |
| borne_add 250+10 (8 bits) | 4 | 4 | rc=4 | rc=4 | `f()`=4 |

**Même résultat sur les cinq voies, dont trois architectures distinctes.**

## 6. Validation de non-régression

| Contrôle | Résultat mesuré |
|---|---|
| `rose verif compilateur/backend_universel.rose` | `vérifié` |
| `rose verif /tmp/opencode/multi/tests_multi_prog.rose` (concaténation) | `vérifié` |
| `rose rir compilateur/emetteur.rose \| md5sum` | `b3ed9262dd54776a6dbd07bc12368d0e` (point fixe intact) |
| `bash scripts/verifier_parite_selfhost.sh --moteur /tmp/opencode/moteur_141.elf --workdir /tmp/opencode/multi/parite` | `tests=141 CONFORMES=141 DIVERGENTS=0 BLOQUES=0`, `VERDICT … RC=0` (TSV `/tmp/opencode/multi/parite/parite_20261125T193142Z.tsv`) |
| `git status --short` | 2 fichiers nouveaux, aucun suivi modifié, HEAD `a43dab8` |

## 7. Ce qui marche / ce qui est bloqué

### Marche

- Traduction d'un IR unique vers trois familles d'encodeurs, **sans nom
  d'architecture dans `bu_traduire`** (`bu_cible` est la seule porte d'entrée).
- `aarch64` : `movz` 16 bits, `add`, `and`, `ret`, `syscall` (`movz x8,n` +
  `svc #0`) encodés correctement ; exécution native confirmée.
- `x86_64` : `movabs`, `mov r/m,r`, `add`, `and`, `ret`, `syscall` ; exécution
  sous qemu confirmée.
- `wasm` : module complet valide, `i32.const`/`i32.add`/`i32.and`/`return`,
  export `f` ; exécution sous Node confirmée.
- Émission de fichiers binaires par `ecrire_bytes` depuis le **ELF C0** (aucun
  émetteur Rust/ELF du dépôt dans la chaîne).
- Conteneur ELF minimal généré par la **même fonction** pour deux `machine`
  différentes ; les deux ELF passent `file` et s'exécutent.

### Bloqué / limites honnêtes

- **x86_64 natif impossible ici** : l'hôte est aarch64. La preuve d'exécution
  passe par `qemu-x86_64-static` ; l'`objdump` présent ne connaît que
  l'aarch64, le désassemblage x86-64 a été fait avec Capstone Python.
- **wasm** : `syscall` est volontairement `unreachable` (il faudrait une section
  import), `load`/`store` encodés mais le module minimal n'a pas de section
  mémoire ; registres = **locaux i32** (32 bits) contre 64 bits sur les deux
  autres cibles. Les deux calculs choisis tiennent dans 32 bits, donc la
  comparaison est significative ; une valeur > 32 bits divergerait.
- **IR sans branchement conditionnel** : `jmp`/`call` existent (registre ou
  profondeur), mais pas de `si` machine ; la somme 1..10 est donc déroulée.
- **Aucune relocation, aucun label, aucun éditeur de liens** : l'enveloppe ELF
  est un `PT_LOAD` unique à base fixe `0x400000`, sans sections ni PIC.
- **Rose n'a pas d'affectation par index de liste** (`$r[1] = v` est refusé) :
  le banc de test simulation les registres par une fonction pure `tm_setreg`
  qui reconstruit la liste. Rose n'a pas non plus d'opérateur bit-à-bit :
  `tm_and` décompose les bits en arithmétique exacte.
- **Le décodeur x86_64 du test ne couvre que les motifs émis par
  `bu_x64_encode`** (imm/mov/add/sub/and/or/xor/ret/syscall) ; ce n'est pas un
  décodeur x86 complet.
- **`longueur()` sur une chaîne binaire brute ne compte pas la même chose**
  sous C0 (points de code) et sous le moteur Rust (octets) : les tailles
  annoncées sont calculées arithmétiquement, jamais déduites de `longueur($s)`.
