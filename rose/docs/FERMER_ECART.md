# Fermeture de l'écart résiduel fib(35) — Rose BAT rustc -O

**Horodatage** : 2026-11-18T14:12Z → 14:32Z. Tous les chiffres sont des
**constats datés** (règle `AGENTS.md`) : re-mesurer après tout changement de
binaire, de topologie ou d'état machine.

**Mission** : fermer les 1,16 % restants (`REGISTRES_RUST.md` : Rose 12,739 ms
vs Rust 12,593 ms) par un correctif sûr et générique, sans special-case.

**Fichiers modifiés par cette étape** : `rose/src/jit.rs` uniquement (+ ce
document). **Aucun commit.** Aucun autre fichier du dépôt touché (`Cargo.toml`,
`lib.rs`, `main.rs`, `run.rs` appartiennent à d'autres agents ; leurs commits
`0d1c36a`/`a28c764` sont intervenus pendant la session, le binaire final a été
reconstruit ensuite et est **byte-identique** — SHA256 inchangé).

**Binaire final** `rose/rose` SHA256
`7ba89679654986ff980d5aca55ae3f69bb45549e6950775ffdcb52c705231469`
(constat 14:27Z : `cargo build --release` reproductible, `target/release/rose` =
`rose/rose` au même SHA).

---

## 1. Baseline re-mesurée (14:12:30Z, cœur 15, 21 échantillons entrelacés)

`N_SAMPLES=21 PIN_CPU=15 python3 /tmp/opencode/rosebench/ab3_fib.py baseline_21`
→ `ab3_baseline_21_out.txt`. Binaire `88ceb428…` (avant correctif), ordre rotatif,
résultat `9227465` vérifié à chaque tirage :

| Implémentation | interne médian | min | max |
|---|---:|---:|---:|
| `rose_v0` (boucle Cranelift, 19,9 ms historique) | 19,912 ms | 19,877 | 29,727 |
| Rose machine (avant correctif) | **12,757 ms** | 12,695 | 13,403 |
| Rust `rustc -O` | **12,629 ms** | 12,538 | 13,352 |

`rose_machine / Rust = 1,0101` → baseline reproduite (1,01 % d'écart).

Protocole : `taskset -c <cœur>` (pinage), rotation de l'ordre à chaque
répétition, ≥21 échantillons par implémentation, médianes (et min/max),
contrôle `rose_ctl_v0` figé (copie du binaire avant modification). Deux cœurs
X925 indépendants utilisés : 15 et 19 (`cpu_capacity≈1017`, 3,9 GHz,
gouverneur `performance`).

---

## 2. Diagnostic — où sont les 0,13 ms ?

### 2.1 Désassemblage réel (avant)

`ROSE_DUMP_MACHINE=1 ./rose/rose rose_fib.rose` + `objdump -D -b binary -m aarch64`
vs `objdump -d pure_rs` :

```
Rose (avant)                         Rust rustc -O (référence)
0: cmp x0,#0x5c  (garde, 1×)          90bc stp x29,x30,[sp,#-32]!
4: b.gt repli                         90c0 stp x20,x19,[sp,#16]
8: stp x29,x30,[sp,#-32]!             90c4 mov x29,sp
c: stp x19,x20,[sp,#16]               90c8 mov x19,x0
10: cmp x0,#2                         90cc cmp x0,#2
14: b.lt 0x48   (NON pris, chaud)      90d0 b.cs 0x90e4   (PRIS, chaud)
18: mov x19,x0                        90e4 mov x20,xzr
1c: mov x20,#0                        90e8 sub x0,x19,#1
20: sub x0,x19,#1   ┐                 90ec bl 0x90bc
24: bl 0x8          │                 90f0 sub x19,x19,#2
28: sub x19,x19,#2  │ 6 + bl          90f4 add x20,x0,x20
2c: add x20,x20,x0  │                 90f8 cmp x19,#1
30: cmp x19,#1      │                 90fc b.hi 0x90e8
34: b.hi 0x20       ┘                 9100 add x0,x19,x20
```

La boucle chaude est déjà **instruction pour instruction équivalente** à LLVM
(Rose a même **une** instruction de moins dans le prologue : `x29` n'est pas
positionné). Le résidual n'est donc NI le nombre d'appels (14 930 351 des deux
côtés), NI le corps de boucle, NI l'allocation de registres.

### 2.2 Mesures diagnostiques (drapeaux `ROSE_*` temporaires, tous retirés depuis)

A/A/A entrelacé, `rose_ctl` = binaire figé avant modification, `rose_new` =
binaire sous le drapeau testé, 15 ou 60 échantillons, cœur 15 :

| Variante | `rose_new / rose_ctl` | `rose_new / Rust` | Verdict |
|---|---:|---:|---|
| défaut (contrôle) | 0,9990 → 1,0012 | 1,0094 → 1,0136 | — |
| `ROSE_NO_GUARD=1` (garde bigint désactivée) | 0,9925 → 1,0033 | 1,0113 → 1,0117 | **aucun gain** → garde conservée |
| `ROSE_MACHINE_PAD=1..8` (nop d'alignement) | 0,9948…1,0110 | 1,0052…1,0304 | bruit, pas de gain fiable |
| `ROSE_PROLOG3=1` (prologue 3 registres seul) | 1,0022 | 1,0128 | neutre seul |
| **`ROSE_BASE_CS=1` (test de base PRIS, forme LLVM)** | **0,9143…0,9206** | **0,9243…0,9343** | **victoire ~7 %** |
| **`ROSE_PROLOG3=1 ROSE_BASE_CS=1`** | **0,9054…0,9229** | **0,9154…0,9315** | **victoire ~8 %** |

Le facteur décisif est la **direction du branchement du test de base** :
`cmp x0,#2 ; b.lt base` (non pris sur le chemin chaud) coûte ~1,1 ms sur 12,7 ms
face à `cmp x0,#2 ; b.cs init_boucle` (pris). Même nombre d'instructions,
même sémantique, même résultat : seule la prédiction de branchement change.
L'alignement et la garde bigint ne sont **pas** la cause (mesuré, pas supposé).

Mise à l'échelle (`ab_scaling.py`, 40 échantillons, cœur 15) : l'écart initial
croît avec le nombre d'appels (fib25 +1,3 µs, fib30 +39 µs, fib35 +117 µs),
confirmant un effet par appel et non un surcoût fixe de chronométrage.

---

## 3. Correctif permanent et générique

Dans `emettre_boucle_machine` (`rose/src/jit.rs`), pour TOUTE fonction reconnue
par `detecter_recurrence_binaire` (aucun nom codé en dur) :

1. **Test de base en branchement pris** : `cmp x0,#k ; b.cs idx_init` (cas
   strict) ou `b.hi idx_init` (cas non strict `n<=k`), cas de base en
   fallthrough, avec `b epi` patché. Forme exacte de `rustc -O`.
2. **Prologue/épilogue à 3 registres** : `stp x19,x30,[sp,#-32]!` +
   `str x20,[sp,#16]` ; `ldr x20,[sp,#16]` + `ldp x19,x30,[sp],#32`.
   Ce code ne modifie jamais `x29` : le sauver/restaurer est inutile. Même
   nombre d'instructions, un stockage de moins par appel récursif.
3. **Garde bigint conservée** : `cmp x0,#borne ; b.gt repli` reste à l'entrée
   (octet 0, cible du trampoline) ; les appels récursifs branchent sur le
   prologue (octet 8). Exactitude des grands n préservée.
4. Tous les drapeaux diagnostics (`ROSE_NO_GUARD`, `ROSE_MACHINE_PAD`,
   `ROSE_PROLOG3`, `ROSE_BASE_CS`) ont été **retirés** du code final.

### Code machine final (constat objdump)

```
   0: cmp x0,#0x5c        ; garde bigint (entrée Rust→JIT, 1 seule fois)
   4: b.gt 0x4c           ; repli interpréteur
   8: stp x19,x30,[sp,#-32]!   ; prologue = cible des bl récursifs
   c: str x20,[sp,#16]
  10: cmp x0,#2
  14: b.cs 0x1c          ; BRANCHEMENT PRIS sur le chemin chaud
  18: b 0x40             ; cas de base (froid)
  1c: mov x19,x0
  20: mov x20,#0
  24: sub x0,x19,#1      ┐
  28: bl 0x8             │ boucle LLVM : 6 instructions + bl
  2c: sub x19,x19,#2     │
  30: add x20,x20,x0     │
  34: cmp x19,#1         │
  38: b.hi 0x24          ┘
  3c: add x0,x19,x20
  40: ldr x20,[sp,#16]
  44: ldp x19,x30,[sp],#32
  48: ret
  4c: … repli rose_appel_interpreteur(n,0,0,0,nom)
```

---

## 4. Mesures finales (binaire `7ba896…`, 21 échantillons × 4 salves)

`N_SAMPLES=21 PIN_CPU=<cœur> python3 /tmp/opencode/rosebench/ab_cmp.py FINAL-…`
→ `final_bench_out.txt`. Ordre rotatif, `9227465` vérifié à chaque tirage :

| Salve | `rose_ctl` (avant) | **Rose final** | Rust | `Rose/Rust` |
|---|---:|---:|---:|---:|
| cœur 15, round 1 | 12,762 ms | **11,673 ms** | 12,611 ms | **0,9256** |
| cœur 15, round 2 | 12,755 ms | **11,547 ms** | 12,658 ms | **0,9122** |
| cœur 19, round 1 | 12,778 ms | **11,573 ms** | 12,601 ms | **0,9184** |
| cœur 19, round 2 | 12,775 ms | **11,611 ms** | 12,618 ms | **0,9202** |

- Meilleur chiffre : **11,547 ms** (vs 12,658 ms pour Rust dans la même salve).
- Médiane des 4 ratios : **≈0,9184** → **Rose est ~8,2 % plus rapide**.
- Gain vs l'état antérieur : 12,757 → 11,55…11,67 ms (≈×1,10).
- `rose_ctl / Rust` = 1,0077…1,0140 dans les mêmes salves : le contrôle figé
  reste bien ~1 % plus lent que Rust, donc le gain vient du correctif, pas
  d'une dérive de l'état machine.

---

## 5. Validation (toutes exécutées sur le binaire final)

```bash
cd /home/clausake/Stefan/ROSE
taskset -c 15 ./rose/rose /tmp/opencode/rosebench/fib25.rose
#   75025
taskset -c 15 ./rose/rose /tmp/opencode/rosebench/rose_fib.rose
#   9227465
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
  > /tmp/opencode/rosebench/emit_final_step.txt    # rc=0, 260 692 octets
md5sum /tmp/opencode/rosebench/emit_final_step.txt
#   bdbef63681608a0b22a5637539e9d738  -  (point fixe intact)
# 4 oracles M4.1–M4.4, échantillon de 20 fichiers VALIDES, comparaison
# octet pour octet aux composants Rose :
#   tokens 20/20 | ast 20/20 | verif 20/20 | rir 20/20  (0 écart)
ROSE_DUMP_MACHINE=1 ./rose/rose /tmp/opencode/rosebench/generique.rose
#   lucas_machine.bin, double_machine.bin, decale_machine.bin
#   → plusieurs fonctions du motif ont reçu le code machine (générique)
```

⚠ Un premier échantillon incluait des fichiers invalides du corpus
(`rose-cpp/tests/verifs/*`) : `verif`/`rir` y donnaient 11/19 sur **le binaire
contrôle figé comme sur le binaire final** (messages d'erreur différents) —
écart préexistant, **pas une régression** de cette étape. Le chiffre retenu
(20/20) porte sur des fichiers valides.

⚠ Le repli bigint (`n > borne`) délègue au corps AST d'origine, qui est une
récursion naïve : exact mais exponentiel pour de très grands `n`. Ce
comportement est antérieur au correctif et inchangé ; la garde n'a jamais été
retirée.

---

## 6. Verdict

**Oui, Rose bat Rust sur fib(35).** Chiffre final **11,55…11,67 ms** contre
**12,60…12,66 ms** pour `rustc -O`, sur 4 salves entrelacées de 21 échantillons
pinées sur deux cœurs X925 distincts : **Rose est ~8 % plus rapide** (ratio
médian 0,9184), contre 1,16 % de déficit au départ.

La cause n'était ni l'alignement ni la garde bigint (tous deux mesurés neutres),
mais la **direction du branchement du test de base** : le `b.lt` non pris du
chemin chaud contre le `b.cs` pris de LLVM. Le correctif est générique (motif
structurel, aucun nom de fonction codé en dur), conserve la garde d'exactitude,
et n'a touché que `rose/src/jit.rs`. Aucun commit effectué.
