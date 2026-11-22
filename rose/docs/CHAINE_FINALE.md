# Chaîne finale du self-hosting A–Z — point fixe étendu (22/11/2026)

## Verdict

**SUCCÈS mesuré le 22/11/2026 à 18:16:44 UTC.** `C1 == C2` octet pour octet
(`cmp` rc=0), `md5=9fbd21d2b06efe7f1e86ac5b5730d2c8`.
Le compilateur natif auto-émis se recompile lui-même de façon idempotente :
**le point fixe étendu du self-hosting A–Z est atteint.**

Aucune publication, aucun commit, aucun changement de code source n'a été
effectué dans le cadre de cette surveillance. Le pilotage de la chaîne
appartient à l'orchestrateur (PID 3939168, script
`/tmp/opencode/chain_finale2.sh`).

---

## 1. Protocole exact exécuté

Script : `/tmp/opencode/chain_finale2.sh`
Journal : `/tmp/opencode/chain_finale2.log` (637 octets)
Répertoire : `/home/clausake/Stefan/ROSE`
Garde-fou mémoire : `G="bash scripts/garde_fou_mem.sh 20000000"` (20 000 000 Ko
≈ 20 Gio), appliqué à **chaque** émission native.

| Ordre | Commande réelle |
|---|---|
| 1 | `./rose/target/release/rose rir compilateur/rose_natif.rose > /tmp/opencode/rnat4.rir` |
| 2 | `$G ./rose/target/release/rose compilateur/emetteur_elf.rose /tmp/opencode/rnat4.rir > /tmp/opencode/C0_v4.elf 2> /tmp/opencode/C0_v4.err` |
| 3 | `./rose/target/release/rose compilateur/rose_natif.rose /tmp/opencode/fib28.rose > /tmp/opencode/fib_ref.elf` |
| 4 | `$G /tmp/opencode/C0_v4.elf /tmp/opencode/fib28.rose > /tmp/opencode/fib_v4.elf` ; `timeout 60 /tmp/opencode/fib_v4.elf` ; `cmp fib_v4.elf fib_ref.elf` |
| 5 | `$G /tmp/opencode/C0_v4.elf /tmp/opencode/p3.rose > /tmp/opencode/p3_v4.elf` |
| 6 | `$G /tmp/opencode/C0_v4.elf compilateur/rose_natif.rose > /tmp/opencode/C1_v4.elf 2> /tmp/opencode/C1_v4.err` |
| 7 | `$G /tmp/opencode/C1_v4.elf compilateur/rose_natif.rose > /tmp/opencode/C2_v4.elf 2> /tmp/opencode/C2_v4.err` |
| 8 | `cmp -s /tmp/opencode/C1_v4.elf /tmp/opencode/C2_v4.elf` ; `md5sum /tmp/opencode/C1_v4.elf` |

Le garde-fou (`scripts/garde_fou_mem.sh`) surveille la RSS toutes les 5 s et
tue la commande avec `exit 75` au-delà de 20 000 000 Ko.

---

## 2. Sortie brute du journal (intégrale)

```text
[18:01:40] RIR du composite
[18:01:40]   686038 octets
[18:01:40] émission C0 (garde-fou 20 Gio)
[18:02:00]   C0 rc=0 taille=1602056 magic=7f454c4602010100
[18:02:00] test fib
[18:02:11]   fib rc=0 header=0200b700 sortie=317811 cmp=OK
[18:02:11] sonde non-ASCII (p3)
[18:02:16]   rc=0 taille=9104
[18:02:16] C1 = C0(rose_natif.rose) — garde-fou 20 Gio
[18:09:33]   C1 rc=0 taille=1602056 magic=7f454c4602010100 err=
[18:09:33] C2 = C1(rose_natif.rose) — garde-fou 20 Gio
[18:16:44]   C2 rc=0 taille=1602056 magic=7f454c4602010100 err=
[18:16:44] SUCCES — C1 == C2, point fixe étendu atteint, md5=9fbd21d2b06efe7f1e86ac5b5730d2c8
```

---

## 3. Preuves sur les artefacts (relevées à 18:17:07 UTC)

### 3.1 Magic — `xxd -p -l 8 <fichier> | tr -d '\n'`

```text
C0_v4: 7f454c4602010100
C1_v4: 7f454c4602010100
C2_v4: 7f454c4602010100
```

### 3.2 Tailles — `wc -c`

```text
1602056 /tmp/opencode/C0_v4.elf
1602056 /tmp/opencode/C1_v4.elf
1602056 /tmp/opencode/C2_v4.elf
4806168 total
```

### 3.3 MD5 — `md5sum`

```text
9fbd21d2b06efe7f1e86ac5b5730d2c8  /tmp/opencode/C0_v4.elf
9fbd21d2b06efe7f1e86ac5b5730d2c8  /tmp/opencode/C1_v4.elf
9fbd21d2b06efe7f1e86ac5b5730d2c8  /tmp/opencode/C2_v4.elf
```

**C0 = C1 = C2 : les trois binaires sont identiques octet pour octet**, y compris
le binaire de départ émis par l'émetteur hébergé. Le point fixe est donc déjà
atteint dès la première itération auto-hébergée.

### 3.4 Résultat du `cmp` demandé

```text
$ cmp /tmp/opencode/C1_v4.elf /tmp/opencode/C2_v4.elf
rc_cmp=0 (identiques)
```

Le test du script (`cmp -s …`) a rendu la branche `SUCCES` à 18:16:44.

### 3.5 Stderr des émissions

```text
C1_v4.err : 0 octet
C2_v4.err : 0 octet
```

### 3.6 En-tête ELF — `readelf -h /tmp/opencode/C2_v4.elf` (extrait)

```text
Class:     ELF64
Data:      2's complement, little endian
Type:      EXEC (Executable file)
Machine:   AArch64
Entry point address: 0x4000b0
Number of program headers: 2
Section header string table index: 0
```

`e_machine = 0x00b7 = 183` correspond aux octets `02 00 b7 00` attendus en
offset 16. La machine hôte est `aarch64`, donc l'ELF émis est exécutable
nativement. Contrôle complémentaire (aucune compilation relancée) :
`/tmp/opencode/fib_v4.elf` → `317811`, rc=0.

### 3.7 Horodatages UTC (stat)

```text
/tmp/opencode/chain_finale2.log  2026-11-22 18:16:44  637 o
/tmp/opencode/C0_v4.elf          2026-11-22 18:01:59  1602056 o
/tmp/opencode/C1_v4.elf          2026-11-22 18:09:29  1602056 o
/tmp/opencode/C2_v4.elf          2026-11-22 18:16:39  1602056 o
```

---

## 4. Surveillance active (faits relevés pendant l'attente)

- C1 : 18:02:16 → 18:09:33 (437 s), `rc=0`.
- C2 : 18:09:33 → 18:16:44 (431 s), `rc=0`.
- Échantillons C2 (PID 12986, garde-fou PID 12985), temps CPU / RSS :

| Heure UTC | TIME CPU | RSS |
|---|---|---|
| 18:10:09 | 00:00:36 | 8 060 Ko |
| 18:10:39 | 00:01:06 | 10 312 Ko |
| 18:11:18 | 00:01:45 | 23 052 Ko |
| 18:12:18 | 00:02:45 | 39 480 Ko |
| 18:13:48 | 00:04:15 | 151 892 Ko |
| 18:14:24 | 00:04:51 | 404 904 Ko |
| 18:16:24 | 00:06:51 | 408 180 Ko |

- Le temps CPU a avancé de façon continue (100 % d'un cœur), sans disparition
  de processus ni ligne d'erreur : **aucun incident**.
- Pic RSS observé ~408 Mio, très loin du garde-fou 20 Gio. Le garde-fou
  n'a jamais été déclenché (`C1_v4.err`/`C2_v4.err` vides, rc=0).
- `nproc=20` : un job de profilage préexistant (`prof_mem_rose.sh` avec
  `C0_opt.elf`, PID 3801702/4043217) consommait un autre cœur pendant C2.
  Il n'a **pas** été lancé par cette surveillance et n'a pas été modifié ;
  avec 20 cœurs il n'a pas retardé la chaîne (C2 a mis 431 s contre 437 s à C1).
- Le script orchestrateur (PID 3939168) est sorti normalement après la ligne
  `SUCCES`.

---

## 5. Ce que ça implique pour le self-hosting A–Z

1. **Amorçage complet validé** : le moteur Rust (`rose`) produit le RIR,
   l'émetteur hébergé produit `C0`, et `C0` compile `compilateur/rose_natif.rose`
   en un ELF natif autonome (`C1`) qui, à son tour, compile la même source en
   `C2` — avec un résultat **bit-identique**. La chaîne
   Rust → émetteur Rose → C0 → C1 → C2 est fermée.
2. **Le blocage matériel de 14:39 UTC est levé** : l'échec décrit dans
   `SELFHOST_FINAL.md` §6 (84 Gio, swap épuisé, état `D`) ne se reproduit pas.
   Avec les correctifs en place, le pic RSS reste ~408 Mio et les deux
   générations s'achèvent en ~7 min chacune. La cause était bien une question
   de pic d'allocation, non une erreur de compilation.
3. **Le correctif Unicode/G5 est validé en chaîne complète** : la sonde `p3`
   (retour `"≈"`) émet 9 104 o via `C0`, le test `fib(28)` donne `317811`
   (`cmp=OK` contre le binaire de référence, header `0200b700`).
4. **Jalon M4 / point fixe étendu** : `M4.5` (point fixe de bootstrap sur
   `emetteur.rose`, md5 `83449e53…`) avait été atteint côté source ; ici c'est
   le **binaire natif auto-émis** qui satisfait `C0 == C1 == C2`
   (md5 `9fbd21d2…`). C'est la porte de sortie mesurable du self-hosting A–Z.
5. **Périmètre de cette preuve** : elle établit l'idempotence du compilateur
   natif auto-hébergé. Elle ne couvre pas M5 (produit sans dépendance Rust) ni
   la parité de performance avec Zig, qui restent des jalons distincts.

---

## 6. Décision

Aucune action de publication ni de commit. L'orchestrateur est informé :
`SUCCES`, `cmp` rc=0, artefacts dans `/tmp/opencode/`, rapport ci-dessus.
Prochaine étape éventuelle (à la discrétion de l'orchestrateur) : intégrer
ce résultat dans `SELFHOST_FINAL.md`, `PLAN.md` et les notes de release,
puis décider d'une publication — hors du périmètre de cette surveillance.
