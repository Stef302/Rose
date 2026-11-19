# Verdict final — Rose est-il « full hosted » ? Re-mesure du 19/11/2026

**Constat daté, pas une citation de documentation ancienne.**
Agent wb2. Dépôt `/home/clausake/Stefan/ROSE`, `HEAD = 6eabd463dff094d0ffe7737fd9cfe3999b7e7988`
**au début de la mesure**. Deux commits concurrents sont arrivés pendant le chantier
(`fd505bd` wasm, `507e7f5` PGCD) → `HEAD = 507e7f5d…` à la clôture. Aucun des deux ne touche
aux compilateurs Rose (md5 inchangés) ni à `rose/rose` (toujours `da7d7ddf…`) : les mesures
ci-dessous restent valides.
Machine `gx10-2`, `uname -m = aarch64`, noyau `7.0.0-1025-nvidia`.
`rustc 1.99.1 (58cf8f9c3 2026-10-13)`, `cargo 1.99.1 (5f94df478 2026-08-27)`.
Aucun commit, aucune publication, aucun fichier du dépôt modifié **hors ce document**.

## Binaire utilisé pour la re-mesure

Conformément à la consigne, j'ai construit **mon propre binaire**, sans remplacer `rose/rose` :

```sh
cd /home/clausake/Stefan/ROSE/rose && export PATH="$HOME/.cargo/bin:$PATH"
CARGO_TARGET_DIR=/tmp/opencode/wb2_agents/target CARGO_BUILD_JOBS=4 \
  taskset -c 10 cargo build --release
# Finished `release` profile [optimized] target(s) in 2m 38s (259 warnings)
```

| Binaire | md5 | Remarque |
|---|---|---|
| `/tmp/opencode/wb2_agents/target/release/rose` | `e020720dd2655aab6591a75e8ec96054` | **mon binaire**, construit à 12:09:25+01 |
| `rose/rose` (livré) | `da7d7ddfcf74279bbc8c260128c98b1c` | identique à `rose/target/release/rose` **au moment de la mesure** ; inchangé depuis 11:06:17+01. Un autre agent a depuis reconstruit `rose/target/release/rose` (md5 `3d72c371…`) sans toucher `rose/rose`. |
| `rose/rose` mesuré dans `SELF_HOSTED_REEL.md` | `33ec2e3b587a5c1842e3a874dd3013ef` | **périmé** : le produit a encore été reconstruit depuis |

`file` des deux : `ELF 64-bit LSB pie executable, ARM aarch64, static-pie linked, stripped`.

⚠ Concurrence : un autre agent a modifié `rose/src/jit.rs` à 12:15:53+01 (après ma compilation).
Les mesures ci-dessous portent donc sur le code source tel qu'il était à 12:09. Les sources Rose
(`compilateur/*.rose`) n'ont **pas** bougé pendant la mesure (md5 re-vérifiés en fin de chantier) :
`emetteur.rose = 7b6ad535…`, `verifieur.rose = 4f81675e…`, `emetteur_elf.rose = afeffb3b…`.

## 1. Réponse courte

**NON. Rose n'est pas « full hosted ». L'état réel est PARTIEL : auto-hébergement du front-end, pas du produit.**

| Question | Réponse mesurée |
|---|---|
| Les étapes lexage → AST → vérification → RIR existent-elles en Rose ? | ✅ **OUI** (oracles re-joués : tokens/ast 170/170 ; verif/rir détail §2) |
| Le point fixe M4.5 est-il atteint ? | ✅ **OUI**, `b084ac5c926f6ae486845f9bc96c616d` identique sur les deux voies |
| Un émetteur Rose produit-il un vrai ELF exécutable ? | ✅ **OUI**, mais **un stub `exit(K)` de 132 octets** |
| Le pontier OS est-il écrit en Rose ? | ✅ **OUI** (nouveau : 37 `fn`, ELF de 168 octets qui écrit `Bonjour Rose OS`) |
| Le compilateur Rose peut-il se compiler en binaire natif ? | ❌ **NON** : 8/8 compilateurs refusés par l'émetteur ELF (rc=2, 0 octet) |
| L'ELF généré peut-il à son tour compiler ? | ❌ **NON** : rc=47, 0 octet sur stdout |
| Peut-on produire `rose/rose` sans `cargo`/`rustc` ? | ❌ **NON** : **0 %** |

**Pourcentage** : sur 10 étapes de la chaîne (§3), **5/10 = 50 %** ont une implémentation Rose
prouvée (6/11 = 55 % si l'on ajoute le pontier OS, hors chaîne de compilation).
Mais ces 5 étapes sont **exécutées par le binaire Rust** : au sens strict de « auto-hébergée »
(écrite en Rose **et** exécutable sans le moteur Rust), le taux est **0/10 = 0 %**, et au sens
« produit reconstruit sans rustc », **0/1 = 0 %**.

Ne pas lire « 50 % » comme « à moitié autonome » : le milieu de la chaîne est écrit en Rose,
le produit livré ne l'est pas du tout.

## 2. Chaîne G4 rejouée à neuf (preuves)

Script rejouable : `/tmp/opencode/wb2_agents/chain_g4.sh`. Commandes exactes et sorties :

```sh
cd /home/clausake/Stefan/ROSE
W=/tmp/opencode/wb2_agents/g4
printf 'var x = 47\n>> x\n' > $W/p47.rose
md5sum $W/p47.rose                                    # 7ddd0b246f070610a01f372e81b39974
taskset -c 10 $MYROSE rir $W/p47.rose > $W/p47.rir    # rc=0
md5sum $W/p47.rir                                     # fe0f3ae4aafd98756c27787a507ea96c
taskset -c 10 $MYROSE compilateur/emetteur_elf.rose $W/p47.rir > $W/test.elf
                                                      # rc=0, stderr vide
stat -c%s $W/test.elf                                 # 132
md5sum $W/test.elf                                    # e498763a3cab57691f6bfa2e42cb004a
file $W/test.elf
readelf -h $W/test.elf | grep -E 'Type:|Machine:|Entry|Number of program'
chmod +x $W/test.elf && taskset -c 10 $W/test.elf; echo $?   # 47
```

```
/tmp/opencode/wb2_agents/g4/test.elf: ELF 64-bit LSB executable, ARM aarch64,
  version 1 (SYSV), statically linked, no section header
  Type:                              EXEC (Executable file)
  Machine:                           AArch64
  Entry point address:               0x400078
  Number of program headers:         1
  400078: d28005e0  mov x0, #0x2f     // #47
  40007c: d2800ba8  mov x8, #0x5d     // #93 (__NR_exit)
  400080: d4000001  svc #0x0
EXIT=47
```

Ce que l'ELF contient : **aucun compilateur**. `emetteur_elf.rose:174-225` (`g4_depuis_rir`)
extrait la dernière constante du RIR ; `g4_elf` émet `mov x0,#K ; mov x8,#93 ; svc #0` avec
`K = code % 256`. C'est un extracteur de constante, pas un abaissement RIR → machine.
Le filtre refuse explicitement `app `, `appel`, `saut`, `(bin` et toute seconde fonction.

### Point fixe M4.5 rejoué

```sh
taskset -c 10 $MYROSE rir compilateur/emetteur.rose > $W/fp1.rir       # rc=0
taskset -c 10 $MYROSE compilateur/emetteur.rose compilateur/emetteur.rose > $W/fp2.rir  # rc=0
md5sum $W/fp1.rir $W/fp2.rir
# b084ac5c926f6ae486845f9bc96c616d  ×2
cmp -s $W/fp1.rir $W/fp2.rir   # cmp_rc=0
```

La chaîne complète a aussi été **rejouée avec le binaire livré `rose/rose`** (md5
`da7d7ddf…`) : mêmes md5 (`p47.rir` `fe0f3ae4…`, `test.elf` `e498763a…`), même `EXIT=47`,
même point fixe `b084ac5c…`, même échec bout-en-bout `rc=2`. Le résultat ne dépend donc pas
du binaire reconstruit localement.

✅ Point fixe **ATTEINT**, valeur `b084ac5c…` (identique à celle annoncée dans
`BILAN_VAGUE_29.md`). L'ancien `bdbef636…` (`SELF_HOSTED_REEL.md`) puis `de5e7b54…`
(`ORACLES_FIDELITE_FINALE.md`) sont périmés : la source de l'émetteur a changé deux fois.
⚠ Ce point fixe prouve l'**identité de sortie du front-end**, pas la production d'un exécutable.
Les deux commandes sont exécutées par `rose/rose`, le binaire Rust.

### Oracles M4 rejoués (corpus 170 fichiers : examples 6 + jardins 20 + bench 43 + rose-cpp/tests 101)

Script : `/tmp/opencode/wb2_agents/oracles_full.py`, comparaison stricte `(code de retour, stdout+stderr)`.
⚠ Le balayage complet des 170 fichiers a été **interrompu après 22 min** : `tokens` et `ast`
étaient terminés (170/170 chacun), mais `verif` passait plus de 240 s sur plusieurs jardins
(`jardins/math.rose`, `jardins/os_syscalls.rose`, `jardins/tests/gui_native_*.rose`) —
chaque appel ré-analysant les 103 Ko de `verifieur.rose`. Les 5 écarts de rc listés ci-dessous
ont été relevés avant cet arrêt. Pour obtenir un résultat **complet**, j'ai rejoué les 4 oracles
sur un sous-corpus de 13 petits fichiers qui termine (§2.1). C'est une limite de méthode, pas
une conclusion sur la fidélité des compilateurs.

| Oracle | Résultat strict | Détail |
|---|---|---|
| M4.1 `tokens` vs `lexeur.rose` | **170/170** | 0 écart |
| M4.2 `ast` vs `analyseur.rose` | **170/170** | 0 écart |
| M4.3 `verif` vs `verifieur.rose` | 5 écarts de rc détectés sur la portion parcourue (messages identiques) | `bench/dores/nom_reserve.rose`, `bench/dores/t_symboles.rose`, `bench/englobant/bench_englobant.rose`, `bench/suite/complexes_parite.rose`, `bench/suite/matrix.rose` — message d'erreur **octet pour octet identique**, mais rc `1` (moteur) vs `0` (programme Rose). Écart de convention de code de retour, pas de fidélité de message. Sous-corpus complet : §2.1. |
| M4.4 `rir` vs `emetteur.rose` | sous-corpus complet : §2.1 | Le point fixe `b084ac5c…` prouve l'identité octet pour octet sur `emetteur.rose` lui-même. |

### 2.1 Sous-corpus rapide — 4 oracles complètement rejoués

Script `/tmp/opencode/wb2_agents/oracles_fast.py`, 13 petits fichiers
(6 `examples/`, 3 `rose-cpp/tests/roses/`, 2 `bench/`, 1 jardin, `compilateur/lexeur.rose`),
comparaison stricte `(rc, stdout+stderr)`, `taskset -c 10`, 127 s au total :

```
tokens  vs lexeur.rose       : 13/13 (2s)
ast     vs analyseur.rose    : 13/13 (4s)
  ECART verif bench/dores/nom_reserve.rose rc=1/0 oct=79/79 texte_identique=True
  ECART verif bench/englobant/bench_englobant.rose rc=1/0 oct=81/81 texte_identique=True
verif   vs verifieur.rose    : 11/13 (119s)
  ECART rir bench/dores/nom_reserve.rose rc=1/0 oct=79/79 texte_identique=True
  ECART rir bench/englobant/bench_englobant.rose rc=1/0 oct=81/81 texte_identique=True
rir     vs emetteur.rose     : 11/13 (127s)
TOTAL 48/52 (127s)
```

→ **52/52 = 100 % des messages octet pour octet identiques** ; **48/52 = 92,3 %** en comparaison
stricte. Les 4 écarts stricts sont **exclusivement** des codes de retour `1` (moteur) vs `0`
(programme Rose) sur deux fichiers d'erreur de nom réservé ; les messages sont identiques.
Le front-end Rose est donc bien un miroir fidèle du moteur Rust, mais il ne propage pas le
code d'erreur dans son propre `rc` — une convention, pas un défaut de compilation.

## 3. Tableau étape par étape

Légende : **Rose ?** = existence d'une implémentation écrite en Rose et prouvée par oracle ;
**Sans Rust ?** = exécutable sans le moteur `rose/rose` (cargo/rustc).

| # | Étape | Rose ? | Sans Rust ? | Preuve / commande |
|---:|---|:---:|:---:|---|
| 1 | Lexage / jetons | ✅ | ❌ | `rose tokens f` ≡ `rose compilateur/lexeur.rose f` : 170/170 |
| 2 | Analyse / AST | ✅ | ❌ | `rose ast f` ≡ `analyseur.rose f` : 170/170 |
| 3 | Repli de constantes | ✅ | ❌ | dans `emetteur.rose` / `verifieur.rose`, couvert par les oracles |
| 4 | Vérification de types | ✅ | ❌ | `rose verif f` ≡ `verifieur.rose f` ; 13/13 texte sur le sous-corpus complet, point fixe `b084ac5c…` |
| 5 | Émission du dump RIR | ✅ | ❌ | `rose rir f` ≡ `emetteur.rose f` ; 13/13 texte, point fixe `b084ac5c…` |
| 6 | Pontier OS (syscalls → ELF) *(hors chaîne de compilation)* | ✅ | ❌ | `os_syscalls_test.rose` → ELF 168 o → `Bonjour Rose OS`, rc=0 |
| 7 | Abaissement RIR → code machine **général** | ❌ | ❌ | `g4_depuis_rir` refuse `app`/`bin`/`saut`, rc=2 |
| 8 | Writer ELF complet (sections, symboles) + linker | ❌ | ❌ | ELF G4 = 132 o, `e_shoff=0`, `readelf -S` vide ; `link_natif.rs` reste Rust |
| 9 | Runtime, bigint, GC, E/S, allocateur | ❌ | ❌ | `ecrire_bytes` = `std::fs::write` en `interp.rs:4220` ; pas de GC en Rose |
| 10 | Moteur exécutable + bootstrap (`rose-2` → `rose-3`) | ❌ | ❌ | `main.rs`, `jit.rs` (Cranelift), `interp.rs` ; `cargo build --release` |

→ **5/10 = 50 %** de la chaîne a une implémentation Rose prouvée (6/11 = 55 % avec le pontier OS).
→ **0/10 = 0 %** est exécutable sans le moteur Rust.
→ **0/1 = 0 %** pour la reconstruction du produit.

## 4. Test décisif : produire un binaire sans cargo/rustc — ÉCHEC

Script : `/tmp/opencode/wb2_agents/decisive.sh`. Pour chaque compilateur Rose,
`rose rir compilateur/<f>.rose` puis `rose compilateur/emetteur_elf.rose <RIR>` :

| compilateur Rose | `rir` rc | RIR (o) | `emetteur_elf` rc | ELF produit (o) |
|---|---:|---:|---:|---:|
| `analyseur.rose` | 0 | 71 948 | 2 | 0 |
| `compilateur.rose` | 0 | 353 251 | 2 | 0 |
| `emetteur.rose` | 0 | 262 802 | 2 | 0 |
| `emetteur_cpp.rose` | 0 | 150 211 | 2 | 0 |
| `emetteur_elf.rose` | 0 | 171 560 | 2 | 0 |
| `emetteur_rust.rose` | **1** | 0 | — | 0 — `rose rir` échoue : `Rose : erreur — [Ligne 1597, Colonne 32] Attente d'une expression, trouvé ':'` |
| `lexeur.rose` | 0 | 26 873 | 2 | 0 |
| `verifieur.rose` | 0 | 180 311 | 2 | 0 |

**0 compilateur sur 8** ne peut être converti en ELF. Erreur identique à chaque fois :

```
emetteur_elf G4 : RIR hors sous-ensemble (fonctions, appels, bin, sauts : runtime requis)
```

Mesure sur le RIR réel de `emetteur.rose` (`fp1.rir`, 262 802 octets) : **111 fonctions,
3 614 ops, 262 appels, 1 292 sauts, 1 520 opérations `(bin …)`** — soit exactement les
catégories que `g4_depuis_rir` refuse.

### Auto-réflexivité : impossible

```sh
taskset -c 10 $W/test.elf compilateur/emetteur.rose > $W/reflex.out 2> $W/reflex.err
# rc=47, stdout=0 o, stderr=0 o
```

L'ELF de 132 octets ne lit pas `argv`, n'ouvre aucun fichier et n'écrit rien :
le « bootstrap complet » est physiquement impossible avec l'émetteur actuel.

### Voie historique de `emetteur_elf.rose` : un constant-folding cassé sur les vrais programmes

```sh
printf '>> 5+3\n' > simple.rose
rose compilateur/emetteur_elf.rose simple.rose simple.elf
# 4132 o, md5 195c37595235b7a5e335124ea3fb45b9, ./simple.elf → 8, rc=8
```

Mais sur un programme avec fonction (`fn fib …  >> fib(10)`) le même chemin produit un ELF de
4 132 octets qui affiche **`n`** au lieu de `55`. Ce n'est pas un abaissement général.

### Rose ne peut même pas recopier un binaire existant

Un raccourci possible (« Rose recopie les octets d'un binaire Rust existant ») a été testé :

```sh
$s = lire_texte("/tmp/opencode/wb2_agents/g4/test.elf")   # ELF de 132 octets
>> chaine(longueur($s))                                   # 131
ecrire_bytes("copie.elf", $s)
md5sum copie.elf test.elf
# 8c99ff9801dc4d378375ee2affacb8c2  copie.elf
# e498763a3cab57691f6bfa2e42cb004a  test.elf
```

`lire_texte` **mangle** les octets non-UTF-8 (`0xb7` → `0xfd`) et perd le dernier octet.
Même une copie d'un binaire existant n'est donc pas fiable par la voie Rose, et elle ne
serait de toute façon pas de l'auto-hébergement (le germe resterait le binaire Rust).

### `rose comp` n'est pas de l'auto-hébergement

```sh
rose comp fib.rose -o fib_comp.bin
# 5 391 422 o = 5 391 304 (mon binaire) + 118 o (source de 102 o + enveloppe)
strings fib_comp.bin | grep -c '/rustc/'   # 111
./fib_comp.bin                              # 55, rc=0
```

C'est « Rust + source collée », pas un binaire synthétisé par Rose. Le point de départ reste
obligatoirement un binaire issu de `cargo build --release`.

### Nouveauté réelle mais bornée : le pontier OS écrit en Rose

```sh
rose jardins/tests/os_syscalls_test.rose
# poc|elf=168|attendu=168   x11|elf=4368|attendu=4368   fenetre|elf=4448|attendu=4448
chmod +x /tmp/opencode/wb2_agents/os/bonjour_rose.elf && ./bonjour_rose.elf
# Bonjour Rose OS, rc=0
objdump -D -b binary -m aarch64 --adjust-vma=0x400000 bonjour_rose.elf
# 400078: mov x0,#1 ; 40007c: adr x1,0x400098 ; 400080: mov x2,#16
# 400084: mov x8,#64 ; 400088: svc #0
```

✅ Rose génère bien du code machine natif (37 `fn` dans `os_syscalls.rose`, 18 425 octets).
❌ Ce sont des **séquences de syscalls figées** (`write`, `exit`, socket X11), pas un compilateur.
Le lancement de l'ELF se fait encore par le shell : Rose n'a ni `exec` ni `mmap`.
La primitive `ecrire_bytes` reste dans le moteur Rust (`interp.rs:4220`).

## 5. Dépendances Rust restantes — chiffrées

```sh
cd rose && export PATH="$HOME/.cargo/bin:$PATH"
cargo tree --depth 1
# rose v0.1.5
# ├── cranelift v0.137.1
# ├── libc v0.2.191
# ├── num-bigint v0.5.1
# ├── num-integer v0.1.47
# └── num-traits v0.2.19
#     [build-dependencies] cc v1.11.1
cargo tree --prefix none | sort -u | wc -l     # 60 crates au total

ldd rose/rose                                  # statically linked
strings rose/rose | grep -c '/rustc/'          # 111
strings rose/rose | grep -c 'library/std/src'  # 50
strings rose/rose | grep -ci 'cranelift'       # 117
nm -C rose/target/release-dbg/rose | grep -ci cranelift   # 2064 symboles
```

`cargo tree --offline -e features -i libc` montre que `libc feature "default"` → `"std"` est
réintroduit **exclusivement par Cranelift** (`cranelift-jit → memmap2, region,
wasmtime-internal-jit-icache-coherence`) : G2 est un prérequis dur avant G3.

Taille du code Rust à remplacer pour être honnête sur l'effort : `natif_emetteur.rs` 929 l. +
`link_natif.rs` 834 l. (abaissement machine + writer) = **1 763 l.** ; `interp.rs` 5 954 l. +
`i128x.rs` 1 058 l. + `mul_rapide.rs` 2 671 l. (runtime/nombres) = **9 683 l.** ;
`front.rs` 3 067 l. + `rir.rs` 1 172 l. + `jit.rs` 6 962 l. + `main.rs` 1 430 l. = **12 631 l.**
de moteur. Les sources Rose `compilateur/*.rose` totalisent **21 785 l.** au 19/11, dont
**17 143 l.** pour le cœur front-end (`lexeur`, `analyseur`, `verifieur`, `emetteur`,
`emetteur_elf`, `compilateur`).

`rose --help` ne contient **aucune** commande `bootstrap` (le patch proposé dans
`rose/docs/self_hosted.patch` n'a pas été appliqué) : `rose bootstrap` → `fichier introuvable`.

## 6. Ce qui bloque précisément

1. **G4.1 — abaissement RIR général** : `bin`, `app`, `appel`, `saut`, `saut_si`, indexation,
   globaux sont refusés (rc=2). Il manque le stub `_start` (`n_slots*8` sur la pile), les
   trampolines d'appel, les frames et l'ABI AAPCS64.
2. **G4.2 — runtime** : bigints exacts sans GMP, `eval_fallback`, E/S, allocateur : tout est dans
   le moteur Rust. Le pontier OS ne couvre que 5 syscalls figés.
3. **G4.3 — writer complet** : `e_shoff = 0`, aucune section, pas de `.data`/`.rodata`/symboles,
   pas de PE64. Le linker Rose n'existe pas.
4. **G4.4 — porte finale** : aucun mécanisme ne remplace `rose/rose` par un binaire émis par Rose ;
   `rose comp` copie le moteur Rust.
5. **G2 — Cranelift** encore dans `Cargo.toml` (feature `native` par défaut) : 60 crates, et
   `libc/std` réinjecté.
6. **G3 — `no_std`** non fait : 270 erreurs documentées dans `i128x.rs`/`mul_rapide.rs`,
   783 symboles `std` ancrés, `catch_unwind`/`Any` incompatibles avec `panic=abort`.

## 7. Ce qui reste à faire — estimation factuelle

Les chiffres ci-dessous sont des **estimations documentées** (`AUTONOMIE_COMPLETE.md:349-352`),
pas des mesures de productivité ; les volumes de code, eux, sont mesurés au §5.

| Porte | Étapes | Estimation documentée | Nature du travail |
|---|---:|---|---|
| G2 | 3 | quelques heures à ~2 jours | câbler `natif_emetteur` comme chemin principal, retirer Cranelift, nettoyer 23 `std::` + 19 `std::` |
| G3 | 7 | ~1 à 3 semaines | `no_std` + alloc : 270 erreurs, `catch_unwind` → `Result` (le plus gros risque sémantique) |
| G4 | 4 | ~plusieurs semaines à quelques mois | porter ~1 763 l. d'émetteur/writer + ~9 683 l. de runtime en Rose, puis seulement alors remplacer le produit |
| **Total** | **14** | **~plusieurs semaines à quelques mois** | le goulot est G4, pas G2 |

Factuellement, le travail restant n'est **pas** un petit patch : c'est la réécriture en Rose
d'au moins ~11 400 lignes de sémantique machine/runtime (§5), plus le port `no_std` du moteur.
Aucune mesure ne permet d'affirmer qu'un agent unique le boucle en une session.

## 8. Méthode et rejouabilité

```
/tmp/opencode/wb2_agents/target/release/rose   # binaire construit pour la mesure
/tmp/opencode/wb2_agents/chain_g4.sh           # chaîne G4 + point fixe + bout-en-bout
/tmp/opencode/wb2_agents/decisive.sh           # 8 compilateurs + voie historique + rose comp
/tmp/opencode/wb2_agents/oracles_full.py       # 4 oracles sur 170 fichiers (interrompu à 22 min)
/tmp/opencode/wb2_agents/oracles_fast.py       # 4 oracles complets sur 13 petits fichiers
/tmp/opencode/wb2_agents/oracles_fast_result.txt  # sortie rejouée (48/52 strict)
/tmp/opencode/wb2_agents/g4/                   # artefacts (p47.rir, test.elf, fp*.rir)
/tmp/opencode/wb2_agents/decisive2/            # artefacts (fib.elf, fib_comp.bin)
/tmp/opencode/wb2_agents/os/                   # ELF générés par le pontier OS Rose
```

Toutes les mesures sont pinées `taskset -c 10`. **Je n'ai modifié aucun fichier du dépôt
hors ce document**, et je n'ai fait ni commit ni publication. `git status` en fin de chantier
montre `M rose/src/jit.rs` et `?? rose/docs/LISTE_BAT_RUST.md` : changements **concurrents**
d'autres agents, postérieurs à ma compilation (12:15:53+01), non examinés ici.

---

**État final factuel** : G4 rejoué ✅ (ELF 132 o, md5 `e498763a…`, `exit 47`) ; point fixe
`b084ac5c…` rejoué ✅ idempotent ; oracles tokens/ast **170/170** sur le corpus complet ✅ ;
sous-corpus complet 13 fichiers : **48/52 strict, 52/52 en texte** ✅ ; pontier OS Rose ✅
(ELF 168 o exécutable) ; bout-en-bout ❌ (8/8) ; réflexivité ❌ ; copie binaire ❌ ;
`rose/rose` nécessite toujours `cargo build --release`.
**Rose n'est pas full hosted. Verdict : NON (état PARTIEL, 50 % de la chaîne en source Rose, 0 % du produit sans Rust).**
