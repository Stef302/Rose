# SELFHOST_TOTAL — self-hosting A–Z : état réel mesuré (vague 84, 21/11/2026)

Agent `HOST`. Dépôt `/home/clausake/Stefan/ROSE`. **Aucun commit, aucune publication.**
Périmètre : `compilateur/*.rose`, `scripts/*`, `rose/docs/SELFHOST_TOTAL.md`.
Aucun fichier `rose/src/*` touché (des agents concurrents y travaillent).

Ce document remplace la cartographie `SELFHOST_TOTAL_PLAN.md` (SH5) dont les
jalons C0/C1/C1.3 ont depuis été franchis par SH7/SH10/SH11/F4.

---

## 1. Ce qui était déjà vrai au départ (re-mesuré, pas cité)

| Contrôle | Mesure du 21/11/2026 |
|---|---|
| `seed_rose verif compilateur/emetteur_elf.rose` | `vérifié`, rc=0 |
| `seed_rose rir compilateur/emetteur.rose` | 263 520 o, md5 `b3ed9262dd54776a6dbd07bc12368d0e` |
| `seed_rose rir compilateur/emetteur_elf.rose` | 373 312 o |
| Batterie SH9 (`tests_selfhost/lancer.sh`) | **141 programmes : 121 CONFORMES / 13 DIVERGENTS / 7 BLOQUÉS** |
| Batterie AZ (`tests_selfhost/lancer_az.sh`) | **31 programmes : 27 CONFORMES / 4 DIVERGENTS / 0 BLOQUÉ** (les 4 = Unicode t250–t253) |
| md5 de l'émetteur G5 | `57bb0af95e7de0e472bcd79bbc9b08c8` |
| md5 du binaire Rust-graine utilisé | `7614a534b9411917ef9cd49a834014a8` (copié dans `/tmp/opencode/host84/seed_rose`) |

Piège documenté : `cargo test --release` **échoue actuellement** (rc=101, 4
erreurs `E0425`/`E0308` dans `rose/src/link_natif.rs`, champ `iat_vaddr`
introduit par un chantier concurrent AOT7). Ce n'est **pas** une régression du
self-hosting : le binaire-graine précompilé `rose/rose` reste fonctionnel et
c'est lui qui sert d'amorce. Le chiffre « 653 tests/0 » de la consigne ne peut
donc PAS être re-mesuré tant que le chantier Rust concurrent n'a pas fini.

---

## 2. Jalon HOST-1 — compilateur natif composite `.rose → ELF` : RÉUSSI

### 2.1 Ce qui a été écrit

- `scripts/generer_rose_natif.py` : composition déterministe de
  `compilateur/emetteur.rose` (front-end source→RIR) et de
  `compilateur/emetteur_elf.rose` (backend RIR→ELF G5), **sans modifier aucune
  des deux sources**.
- `compilateur/rose_natif.rose` : fichier généré, **407 226 octets**,
  md5 `acb75bf54f4242e6f396d896347cae93` (reproductible), 263 `fn`,
  `rose verif` → `vérifié` rc=0.
- `compilateur/rose_natif_pe.rose` : même composition avec le backend PE,
  **212 288 octets**, md5 `98472f751e636221a817159ec60810a7`, `rose verif`
  → `vérifié` rc=0. Le composite émet un **PE32+ AArch64 valide**
  (`file` : `PE32+ executable (console) Aarch64, for MS Windows, 3 sections`,
  magic `4d 5a`, 4 096 o pour fib, 0,56 s).
- `scripts/build-sans-rust.sh` : pilote de compilation qui n'appelle QUE le
  compilateur natif auto-hébergé (aucun `rose/rose`, aucun `cargo`).

### 2.2 Choix de composition (justifiés par mesure)

| Fait | Mesure |
|---|---|
| Collisions de noms de fonctions entre les deux sources | **0** (110 + 152 `fn`) |
| Globales du `$principal` de `emetteur.rose` | 116 (indices 0..115) |
| Globales du `$principal` de `emetteur_elf.rose` | 402 (indices 0..401) |
| Borne de la table des globales G5 | 512 entrées (`0x60004000 + 8*i`) |

Donc : le front-end est enfermé dans `fn em_source_rir() -> chaine` (ses 116
variables deviennent des **slots locaux**, la table reste à 402 < 512). Le
backend **reste au niveau supérieur** : ses `$g5_lenl`, `$g5_plist_l`, etc.
sont lus par les routines `g5_emit_*` depuis des fonctions et doivent rester
des globales. Le « vieux compilateur » à la fin de `emetteur_elf.rose` reste
présent mais est **inatteignable** : le chemin G5 se termine par `quitter(0)`.
Tentative de réduction mesurée : supprimer ce bloc (l.4721–6554, ~73 Ko) rend
`rose verif` **échec** — `Variable 'nb_vars' utilisée sans valeur` (l.8419 du
fichier composite) : les fonctions auxiliaires conservées lisent ses globales.
Le bloc doit donc rester, inatteignable mais vérifiable.

Le dumpeur RIR du front-end ne écrit plus sur stdout : sa boucle finale
construit une chaîne `$rnat_dump` retournée, qui alimente directement
`g5_parse($src)`. Aucun fichier temporaire, aucun tube.

### 2.3 Preuves d'exécution (compilateur interprété par la graine)

| Source | Résultat |
|---|---|
| `rose_natif.rose fib.rose > fib.elf` | rc=0, 8 448 o, magic `7f454c46`, ASCII machine AArch64 |
| `./fib.elf` | `75025`, rc=0 (identique au natif) |
| 7 sources de la batterie (`t01,t03,t06,t60,t70,t74,t999`) | **7/7 conformes** octet pour octet, rc identiques |
| PE composite sur `fib.rose` | PE32+ AArch64 valide, `file` OK, 4 096 o, 0,56 s |

Écart préexistant mesuré : `g5b606_rat_litteraux.rose` est rejeté par le
**front-end Rose** (`emetteur.rose` seul, ligne 2323 : `conversion vers entier
impossible (type Vide)`) alors que `rose rir` du binaire Rust accepte la
source. Ce n'est PAS un défaut de la jonction : le même rejet se reproduit
avec `emetteur.rose` seul.

Le compilateur composite produit donc un **ELF autonome exécutable** depuis
une source `.rose`, en ~1,5 s pour un petit programme — sans cargo, sans
appel à `rose comp`, sans LLVM.

Preuve que c'est du code machine (et non un stub) : `readelf -l fib.elf`
donne `ET_EXEC`, entry `0x4000b0`, segment `R E` à `0x400000` (8 448 o) et
segment `RW` à `0x60000000` (128 Gio virtuels, paresseux) ; `objdump -D -b
binary -m aarch64` montre `ldr x10,[sp]`, `movk x9,#0x6000,lsl #16`,
`lsl x0,x0,#3` (valeurs taguées), `bl 0x400108` — le runtime G5 est bien
émis en instructions AArch64.

### 2.4 RIR du compilateur composite

`seed_rose rir compilateur/rose_natif.rose` → **636 570 octets**,
md5 `cbcfe523ee9bb3ef817bd3d7753aba24`, **264 fonctions**, indice global max
**401** (sous la borne 512). C'est cette valeur qui est émise en ELF pour
obtenir le compilateur natif C0.

---

## 3. Jalon HOST-2 — C0 natif et point fixe étendu (en cours de mesure)

Chaîne visée (aucune étape ne fait appel à Rust après la première) :

```
C0 = seed_rose emetteur_elf.rose rnat.rir        # graine Rust, UNE SEULE fois
C1 = C0 rose_natif.rose                          # C0 se compile depuis sa source
C2 = C1 rose_natif.rose                          # C1 se recompile
cmp C1 C2                                        # point fixe étendu
```

Le C0 natif et le F1 de l'émetteur historique (`rose rir emetteur_elf.rose`
puis `seed_rose emetteur_elf.rose e.rir`) sont en cours d'émission dans
`/tmp/opencode/host84/`. Leurs md5, tailles et résultats seront consignés ici
après mesure — aucun résultat n'est anticipé.

---

## 4. Ce que le compilateur natif REMPLACE, et ce qu'il ne remplace pas encore

| Fonction du binaire Rust | Remplacée ? | Preuve |
|---|---|---|
| `rose comp prog.rose` (source → ELF natif autonome) | **OUI** | §2.3 |
| `rose rir prog.rose` (dump RIR) | indirectement : le front-end est embarqué dans C0 | §2 |
| `rose verif prog.rose` | NON (verifieur.rose existe mais n'est pas câblé dans C0) | à faire |
| `rose run` / interpréteur interactif / REPL / JIT | **NON** | voir §5 |
| AOT `rose aot` (runtime `.S` embarqué) | chemin distinct (`natif_emetteur.rs`), non fusionné | voir §5 |

Le résultat livré est donc un **remplacement du chemin de compilation**, pas
encore un `rose.exe` universel identique au binaire Rust de 5,3 Mio.

---

## 5. Cause technique exacte du blocage restant (pas une « limite honnête »)

1. **Pas de chargeur de code dynamique dans le runtime G5.** Le binaire émis
   par `emetteur_elf.rose` est un ET_EXEC statique, sans `mmap`/`mprotect`
   (les seuls syscalls émis sont `openat`/`read`/`write`/`close`/`exit`/
   `fstat`/`getrandom`). Un interpréteur `rose run` natif qui exécuterait un
   programme arbitraire ne peut donc pas compiler puis sauter dans le code
   produit : il lui faut soit unRose-written RIR interpreter (machine code
   émis une fois pour toutes), soit `mmap PROT_EXEC` ajouté à l'émetteur.
2. **Un interpréteur RIR en Rose reste à écrire.** Le format RIR produit par
   `rose rir` compte un jeu d'opcodes plus large que le sous-ensemble G5
   (notamment HOF, valeurs composées, `eval_expr` de l'ancien compilateur).
   Le court-circuit `quitter` de `emetteur_elf.rose` montre que la voie
   « ancien compilateur » est déjà inatteignable sous C0.
3. **Les blobs `rose/runtime/{aarch64,x86_64}.bin` (2 384 / 2 202 o) sont
  écrits pour l'AOT Rust** (`runtime_natif.rs`, adresses absolues `0x402000`
   et `0x403000`). Ils ne sont pas branchés dans l'émetteur G5, dont la carte
   mémoire réserve `0x60000000+` au tas et émet ses propres routines. Les
   intégrer exigerait une relocation des adresses absolues dans l'émetteur —
   chantier distinct, non nécessaire pour produire des ELF autonomes.

Aucune de ces causes n'est une « limite d'ambition » : ce sont des absences
de routine / de syscall / de code, nommées et reproductibles.

---

## 6. Rejouer

```bash
cd /home/clausake/Stefan/ROSE
python3 scripts/generer_rose_natif.py
./rose/rose verif compilateur/rose_natif.rose
./rose/rose compilateur/rose_natif.rose /tmp/opencode/host84/fib.rose > /tmp/fib.elf
chmod +x /tmp/fib.elf && /tmp/fib.elf     # 75025
./rose/rose rir compilateur/rose_natif.rose | md5sum   # cbcfe523…
./tests_selfhost/lancer.sh                # 121/13/7 sur 141
```
