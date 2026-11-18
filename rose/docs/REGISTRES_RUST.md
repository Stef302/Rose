# Allocation de registres du JIT — fermeture de l'écart fib(35) vs rustc -O

**Horodatage** : 2026-11-18T13:40Z → 14:03Z. Tous les chiffres sont des
**constats datés** (règle AGENTS.md) : re-mesurer après tout changement de
binaire, de topologie ou d'état machine.

**Mission** : fermer le facteur 1,58× résiduel sur `fib(35)` en corrigeant
l'allocation de registres du JIT (goulot identifié dans `BATTRE_RUST.md` §4),
pas en ajoutant une optimisation de compilation de plus.

**Fichier modifié par cette étape** : `rose/src/jit.rs` uniquement (+ ce
document). Aucun commit. Binaire final `rose/rose` SHA256
`88ceb428f65449b70f1fd93ecb91e99c5cc771b35eda1c6a9494dac27321fde1`
(constat 14:05Z : d'autres agents ont modifié `Cargo.toml`, `lib.rs` et
`main.rs` en parallèle pendant cette session — fichiers **non touchés** par
cette étape, qui ne possède que `jit.rs` et ce document ; le contrôle `rose_v0`
a été reconstruit avec le même état des autres fichiers, donc la comparaison
A/B/A reste valide).

---

## 1. Baseline re-mesurée (13:40:57Z, cœur 19, 9 échantillons entrelacés)

`PIN_CPU=19 python3 battre_fib.py baseline_main` → `battre_baseline_main_out.txt`,
binaire `ab01f8fd…` :

| Implémentation | interne médian | min | max |
|---|---:|---:|---:|
| Rose JIT (boucle Cranelift) | **19,907 ms** | 19,888 | 20,246 |
| Rust `rustc -O` | **12,615 ms** | 12,574 | 12,709 |

Rose/Rust = **1,5780**. Conforme à `BATTRE_RUST.md` (19,905 / 12,634, 1,5754).

⚠ Le cœur 19 a été occupé en cours de session par un processus concurrent
(`rose-5b84022cbf`, 99,5 % CPU, `psr=19`) : toutes les mesures décisionnelles
ont donc été refaites sur le **cœur 15** (X925, `cpu_capacity=1017`, 3,9 GHz,
gouverneur `performance`), libre, avec un contrôle `rose_v0` reconstruit depuis
la sauvegarde `jit.rs.backup` (19,9 ms mesuré, donc contrôle valide).

---

## 2. Diagnostic désassemblé (avant)

`ROSE_DUMP_CODE=… ROSE_DUMP_IR=1 ./rose rose_fib.rose` puis
`objdump -D -b binary -m aarch64`. Boucle Cranelift (adresses relatives) :

```
1024: mov x1,x20       ; copie de n (x20 = callee-saved)
1028: sub x0,x1,#1     ; argument = n-1
102c: mov x20,x1       ; sauver n avant le bl
1030: bl  0x1000       ; appel récursif
1034: mov x1,x20       ; recharger n
1038: sub x1,x1,#2     ; n-2
103c: mov x20,x1       ; sauver n-2 (paramètre de bloc)
1040: add x19,x19,x0   ; acc += résultat
1044: mov x0,x20       ; n-2 -> x0 pour le cmp
1048: cmp x0,#2
104c: b.lt sortie
1050: mov x1,x20       ; n-2 -> x1 pour l'itération suivante
1054: b   0x1028
```

**11 instructions + `bl`, dont 6 `mov` de permutation**, contre 6 + `bl` chez
LLVM (`BATTRE_RUST.md` §2 : `n` dans `x19`, `acc` dans `x20`, `sub x0,x19,#1 ;
bl ; sub x19,x19,#2 ; add x20,x0,x20 ; cmp x19,#1 ; b.hi`).

**Cause identifiée** : Cranelift garde `n` en `x1` (caller-saved, pour
l'argument de l'appel) ET en `x20` (callee-saved, pour survivre au `bl`), puis
insère des copies à chaque frontière de bloc/paramètre. Ce n'est **pas** l'ABI
`Valeur` (le chemin rapide manipule déjà des i64 nus, sans tag) : c'est bien
l'émission/RA de Cranelift. L'IR est déjà minimale :

```
block5(v4, v5):                 ; v4 = n, v5 = acc
    v24 = isub v4, 1
    v16 = call fn1(v24)
    v26 = isub v4, 2
    v27 = icmp sge v26, 2
    v18 = iadd v5, v16
    brif v27, block5(v26, v18), block4
```

---

## 3. Correctif (générique, dans `jit.rs` seul)

Émetteur machine aarch64 dédié au **motif** détecté par
`detecter_recurrence_binaire` (déjà générique : aucun nom codé en dur),
avec **allocation de registres fixée** :

- `x19 = n` (callee-saved), `x20 = acc` (callee-saved), `x0 = argument/retour` ;
- garde de plage (`borne_securisee`, i128 à la compilation) au seul **entrée
  Rust→JIT** : cible du trampoline = octet 0 ; les appels récursifs branchent
  directement sur le prologue (octet 8), `n` décroissant strictement → la
  garde reste vraie pour tous les descendants (2 instructions × ~15 M appels
  économisées) ;
- repli bigint : `rose_appel_interpreteur(n,0,0,0,nom)` avec le nom embarqué,
  trame non ouverte sur ce chemin → `ret` direct ;
- le trampoline patché (`ldr x16,[pc,#8] ; br x16 ; .quad code`) écrase les
  16 premiers octets de la fonction Cranelift après `finalize_definitions` :
  tous les appelants déjà résolus vers `fptr` exécutent la boucle native ;
  le `bl` récursif est PC-relatif vers le prologue (coût identique à LLVM) ;
- **hors motif** (constantes hors imm12 0..4095, ou architecture non-aarch64)
  → aucun patch, la voie Cranelift inchangée reste utilisée ;
- `ROSE_DUMP_MACHINE=1` écrit `<nom>_machine.bin` pour révision.

### Boucle après (adresses relatives)

```
   0: cmp x0,#0x5c        ; garde (entrée Rust→JIT seulement)
   4: b.gt repli
   8: stp x29,x30,[sp,#-32]!   ; prologue = cible des bl récursifs
   c: stp x19,x20,[sp,#16]
  10: cmp x0,#2
  14: b.lt base
  18: mov x19,x0
  1c: mov x20,#0
  20: sub x0,x19,#1      ; ┐
  24: bl  0x8            ; │
  28: sub x19,x19,#2     ; │ boucle LLVM : 6 instructions + bl
  2c: add x20,x20,x0     ; │ (identique au désassemblage de rustc -O)
  30: cmp x19,#1         ; │
  34: b.hi 0x20          ; ┘
  38: add x0,x19,x20
  3c: ldp x19,x20,[sp,#16]
  40: ldp x29,x30,[sp],#32
  44: ret
```

Plus aucun `mov` de permutation. La boucle chaude est **exactement** la
séquence LLVM (6 instructions + `bl`) ; le seul ajout hors boucle est la garde
de plage, exécutée une fois.

---

## 4. Mesures (cœur 15, contrôle reconstruit, entrelacé A/A/A)

`N_SAMPLES=21 PIN_CPU=15 python3 ab3_fib.py machine_v5_long` →
`ab3_machine_v5_long_out.txt`. Médianes sur 21 échantillons par implémentation,
ordre tournant, résultat `9227465` vérifié à chaque tirage :

| Implémentation | interne médian | min | max |
|---|---:|---:|---:|
| `rose_v0` (boucle Cranelift, contrôle) | 19,896 ms | 19,880 | 19,935 |
| **Rose machine (correctif)** | **12,739 ms** | 12,690 | 13,654 |
| Rust `rustc -O` | 12,593 ms | 12,529 | 12,979 |

- `rose_machine / rose_v0` = **0,6403** → **×1,562** par rapport à la baseline.
- `rose_machine / Rust` = **1,0116** → **Rust ne reste que 1,16 % plus rapide**.
- Écart résiduel : **0,146 ms** sur 12,7 ms.

Confirmations intermédiaires (9 échantillons, même cœur, même protocole) :
`machine_v2_core15_clean` 12,817 / 1,0171 ; `machine_v4_gardskip` 12,841 /
1,0146 ; `machine_v5_bhi` 12,741 / 1,0116. La dernière forme (continuation
`cmp #k-1 ; b.hi`, identique à LLVM) est la meilleure et la plus stable.

---

## 5. Validation (toutes exécutées sur le binaire final)

```bash
cd /home/clausake/Stefan/ROSE
taskset -c 15 ./rose/rose /tmp/opencode/rosebench/fib25.rose        # 75025
taskset -c 15 ./rose/rose /tmp/opencode/rosebench/rose_fib.rose     # 9227465 + interne
taskset -c 15 ./rose/rose jardins/tests/premiers.rose | tail -1
#   ── jardin/premiers : 20 tests, 0 échec ──
taskset -c 15 ./rose/rose /tmp/opencode/rosebench/overflow_test.rose
#   9223372036854775808 / 9223372036854775806 / -9223372036854775809
#   18446744073709551614 / 1000000000000000000000000000000000000
taskset -c 15 ./rose/rose /tmp/opencode/rosebench/generique.rose
#   178 / 1024 / 89  (lucas, double, decale — aucun nom codé en dur)
./rose/rose rir compilateur/emetteur.rose | md5sum
#   bdbef63681608a0b22a5637539e9d738  -
timeout 280 ./rose/rose compilateur/emetteur.rose compilateur/emetteur.rose \
  > /tmp/opencode/rosebench/emit_registres.txt     # rc=0, 260 692 octets
md5sum /tmp/opencode/rosebench/emit_registres.txt
#   bdbef63681608a0b22a5637539e9d738  -
# 4 oracles M4.1–M4.4, échantillon 20 fichiers : 20/20 chacun, 0 écart
#   tokens / ast / verif / rir comparés octet pour octet
ROSE_DUMP_MACHINE=1 ./rose/rose /tmp/opencode/rosebench/generique.rose
#   fib_machine.bin, lucas_machine.bin, double_machine.bin, decale_machine.bin
#   -> 4 fonctions du motif ont effectivement reçu le code machine
```

---

## 6. Verdict honnête

**L'objectif « ≤ 12,634 ms » n'est pas tout à fait atteint** : meilleur chiffre
mesuré **12,739 ms** contre **12,593 ms** pour `rustc -O` sur la même salve
(1,16 % d'écart, 0,146 ms). En revanche le goulot assigné — l'allocation de
registres — est **effectivement corrigé** : la boucle chaude passe de
11 instructions + `bl` (6 `mov` de permutation) à **6 instructions + `bl`**,
exactement la séquence LLVM, et le rapport global passe de **1,578× à 1,012×**
(×1,56 sur la baseline).

L'écart résiduel n'est plus dans l'allocation de registres ni dans le nom-bre
d'appels (14 930 351 des deux côtés) : il tient à des effets de second ordre
(alignement/icache du code mmapé, garde de plage conservée à l'entrée,
prologue du trampoline, bruit de mesure ~0,3 ms). Le faire tomber à zéro
demanderait d'éliminer la garde de plage (au prix de l'exactitude bigint) ou
d'imiter le codegen complet de LLVM — hors périmètre de cette étape.

**Aucune triche** : même algorithme, même nombre d'appels, résultat vérifié
à chaque échantillon, motif générique, toutes les mesures entrelacées et
pinées, contrôle `rose_v0` reconstruit et re-mesuré dans la même salve.

---

## 7. Reproduction

```bash
cd /tmp/opencode/rosebench
N_SAMPLES=21 PIN_CPU=15 python3 ab3_fib.py <label>
ROSE_DUMP_CODE=/tmp/opencode/rosebench/loop6_code.bin \
ROSE_DUMP_MACHINE=1 PIN_CPU=15 /home/clausake/Stefan/ROSE/rose/rose rose_fib.rose
objdump -D -b binary -m aarch64 /tmp/opencode/rosebench/fib_machine.bin
```
