# Self-hosting réel de Rose — mesure du 19/11/2026

**Constat daté, à re-mesurer après tout changement de binaire, de source ou de topologie.**

- Dépôt : `/home/clausake/Stefan/ROSE`, `HEAD = 477736ffa8f5887e414d1cb2404eb8924898bccc` au début de la mesure.
- Machine : `uname -m = aarch64` (`gx10-2`, noyau `7.0.0-1025-nvidia`).
- Binaire mesuré en premier lieu : `rose/rose` = `ELF 64-bit LSB pie executable, ARM aarch64, static-pie linked, stripped`,
  md5 `c21b91a46b58995f11138c766f2ec2dc`, **identique** à `rose/target/release/rose` (même md5).
  ⚠ Un autre agent a reconstruit le binaire **pendant la mesure** : md5 final `33ec2e3b587a5c1842e3a874dd3013ef`
  (toujours égal à `rose/target/release/rose`). Toutes les conclusions ont été **re-jouées sur ce binaire final** (§10).
- Outils : `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, `cargo 1.99.1 (5f94df478 2026-08-27)` (via `~/.cargo/bin`, absents du `PATH` nu).
- Aucun `.rs`, `Cargo.toml`, `PLAN.md` ni fichier d'un autre agent n'a été modifié. Aucun commit, aucune publication.

## 1. Réponse courte

**NON — Rose n'est PAS entièrement self-hosted. C'est PARTIEL.**

| Critère | Résultat mesuré |
|---|---|
| Le **front-end** (lexage → AST → vérification → RIR) existe en Rose et reproduce l'oracle Rust octet pour octet | ✅ **OUI**, 20/20 oracles + point fixe `bdbef636…` rejoué le 19/11 |
| Un **émetteur machine écrit en Rose** produit un vrai ELF aarch64 exécutable | ✅ **OUI**, mais **uniquement un stub `exit(K)` de 132 octets** |
| Le compilateur Rose peut **se compiler lui-même en binaire natif** | ❌ **NON** : rc=2, 0 octet sur stdout pour les 8 compilateurs Rose testés (7 dumps RIR valides + 1 qui échoue déjà au dump) |
| Le binaire livré `rose/rose` peut être **reconstruit sans rustc/cargo** | ❌ **NON** : 0 % — il reste un binaire Rust + std + Cranelift |
| **Auto-hébergement complet du produit** | ❌ **NON** |

**Pourcentage** (10 étapes de la chaîne, détail §5) : **5/10 ≈ 50 % des étapes ont une implémentation Rose prouvée**.
Mais les 5 le sont *exécutées par le binaire Rust* : sous le critère strict « produire `rose/rose` sans rustc », le taux est **0/1 = 0 %**.
Ne pas lire « 50 % » comme « à moitié autonome » : le milieu de la chaîne est écrit en Rose, le produit livré ne l'est pas du tout.

## 2. Chaîne G4 rejouée (preuve réelle, 19/11/2026)

```sh
cd /home/clausake/Stefan/ROSE
W=/tmp/opencode/rose-selfhost
cat > $W/p47.rose <<'EOF'
var x = 47
>> x
EOF
md5sum $W/p47.rose                                    # 7ddd0b246f070610a01f372e81b39974
./rose/rose rir $W/p47.rose > $W/p47.rir               # rc=0
md5sum $W/p47.rir                                     # fe0f3ae4aafd98756c27787a507ea96c
md5sum compilateur/emetteur_elf.rose                  # afeffb3b477c44bf591fcc2c22f52167
./rose/rose compilateur/emetteur_elf.rose $W/p47.rir > $W/test.elf   # rc=0
stat -c%s $W/test.elf                                 # 132
md5sum $W/test.elf                                    # e498763a3cab57691f6bfa2e42cb004a
file $W/test.elf
readelf -h $W/test.elf | grep -E 'Type:|Machine:|Entry|Number of program'
chmod +x $W/test.elf && $W/test.elf; echo $?          # 47
```

Sorties exactes :

```
/tmp/opencode/rose-selfhost/test.elf: ELF 64-bit LSB executable, ARM aarch64, version 1 (SYSV), statically linked, no section header
  Type:                              EXEC (Executable file)
  Machine:                           AArch64
  Entry point address:               0x400078
  Number of program headers:         1
EXIT=47
```

RIR d'entrée (149 octets) :

```
rir
fonction $principal
  retour vide
  n_slots 1
  noms [0]x
  globaux 0
  ops
    0000 affect slot 0 = entier 47
    0001 afficher_abstrait slot 0
```

**Ce que l'ELF contient réellement** (preuve décisive, `objdump -D` sur l'entrée `0x400078`) :

```
  4000f0: d28005e0  mov x0, #0x2f     // #47
  4000f4: d2800ba8  mov x8, #0x5d     // #93 (__NR_exit)
  4000f8: d4000001  svc #0x0
```

Il n'y a **aucun compilateur dans cet ELF** : `emetteur_elf.rose` ne fait que *lire la dernière constante*
du RIR (`g4_depuis_rir`, lignes 173-225) puis émettre 3 instructions. Le code source l'indique
lui-même : `g4_elf($code)` → `mov x0,#K ; mov x8,#93 ; svc #0`, avec `K = code % 256`.
C'est un extracteur de constante, pas un abaissement RIR → machine.

## 3. Point fixe M4.5 rejoué

```sh
./rose/rose rir compilateur/emetteur.rose > $W/emetteur.rir
md5sum $W/emetteur.rir                 # bdbef63681608a0b22a5637539e9d738  (260 692 o, rc=0, 0,015 s)
./rose/rose compilateur/emetteur.rose compilateur/emetteur.rose > $W/emetteur.selfrir
md5sum $W/emetteur.selfrir             # bdbef63681608a0b22a5637539e9d738  (260 692 o, rc=0, 75,9 s)
cmp $W/emetteur.rir $W/emetteur.selfrir  # IDENTIQUE octet pour octet
```

✅ Point fixe **ATTEINT et re-mesuré le 19/11/2026** : l'émetteur écrit en Rose reproduit exactement
le dump RIR que produit la commande Rust `rose rir` sur son propre source.
⚠ Ce point fixe prouve l'**identité de sortie du front-end**, pas la capacité de produire un exécutable.
Les deux commandes sont **exécutées par `rose/rose`**, le binaire Rust.

## 4. Test de bout en bout : ÉCHEC MESURÉ

### 4.1 Le vrai RIR de `emetteur.rose` est refusé

```sh
./rose/rose compilateur/emetteur_elf.rose $W/emetteur.rir > $W/emetteur.elf 2> $W/emetteur.elf.err
# rc=2
# stderr : emetteur_elf G4 : RIR hors sous-ensemble (fonctions, appels, bin, sauts : runtime requis)
# stdout : 0 octet — md5 d41d8cd98f00b204e9800998ecf8427e (chaîne vide)
```

Pourquoi : le RIR de `emetteur.rose` contient **111 fonctions, 3 576 ops, 983 appels,
1 273 sauts, 1 502 opérations `(bin …)`**. Le filtre G4 (`emetteur_elf.rose:203-204`) refuse
dès qu'une op contient `app `, `appel`, `saut` ou `(bin`, et s'arrête à la seconde fonction.

### 4.2 Aucun compilateur Rose ne passe

Commande : `./rose/rose rir compilateur/$f.rose > $W/$f.rir` puis
`./rose/rose compilateur/emetteur_elf.rose $W/$f.rir > $W/$f.elf`.

| `$f` | RIR (o) | rc `rose rir` | rc émetteur ELF | ELF produit |
|---|---:|---:|---:|---:|
| `lexeur` | 26 873 | 0 | 2 | 0 o |
| `analyseur` | 71 843 | 0 | 2 | 0 o |
| `verifieur` | 177 683 | 0 | 2 | 0 o |
| `compilateur` | 353 251 | 0 | 2 | 0 o |
| `emetteur` | 260 692 | 0 | 2 | 0 o |
| `emetteur_cpp` | 150 211 | 0 | 2 | 0 o |
| `emetteur_rust` | 0 | **1** | 1 | 0 o — `rose rir` échoue déjà : `Rose : erreur — index hors des limites des arguments` |
| `emetteur_elf` | 171 560 | 0 | 2 | 0 o |

**0 compilateur sur 8 ne peut être converti en ELF par l'émetteur Rose.**

### 4.3 Réflexivité : impossible

L'ELF de 132 octets ne lit pas `argv`, n'ouvre aucun fichier et n'écrit rien sur stdout :

```sh
./test.elf compilateur/emetteur.rose > $W/reflex.out 2> $W/reflex.err
# rc=47, stdout=0 o, stderr=0 o, md5 d41d8cd98f00b204e9800998ecf8427e
cmp $W/reflex.out $W/emetteur.rir   # cmp_rc=1 (différent)
```

Le « bootstrap complet » (un ELF auto-généré qui compile à son tour `emetteur.rose`) est donc
**physiquement impossible avec l'émetteur actuel**.

### 4.4 Voie historique de `emetteur_elf.rose` : un stub, pas un compilateur

```sh
printf '>> 5+3\n' > $W/simple.rose
./rose/rose compilateur/emetteur_elf.rose $W/simple.rose $W/simple.elf
# rc=0 — "rose: …/simple.elf produit (binaire ELF autonome, 4132 octets)"
file $W/simple.elf    # ELF64 aarch64 EXEC, entry 0x401000, md5 195c37595235b7a5e335124ea3fb45b9
./simple.elf          # affiche 8, rc=8
```

Mais sur un vrai programme (`examples/fibonacci.rose`) le même chemin produit un ELF de 4 136 octets
qui affiche `n\nn\nn` au lieu de `55 / 6765 / 317811`. La voie historique est elle aussi un
constant-folding stub, pas un abaissement général.
⚠ Détail d'invocation : l'émetteur lit le nom de sortie dans `argument(1)` **sans option `-o`** ;
`-o` crée un fichier littéralement nommé `-o` (artefact créé puis supprimé pendant cette mesure).

## 5. Inventaire des étapes — qui fait quoi

| # | Étape | Implémentation Rose | Implémentation Rust (réelle) | Auto-hébergée ? |
|---:|---|---|---|---|
| 1 | Lexage / jetons | `compilateur/lexeur.rose` (451 l.) | `rose/src/front.rs` | ✅ source (exécutée par Rust) |
| 2 | Analyse / AST | `compilateur/analyseur.rose` (1 277 l.) | `rose/src/front.rs` | ✅ source (exécutée par Rust) |
| 3 | Repli de constantes | dans `emetteur.rose` / `verifieur.rose` | `front.rs` | ✅ source |
| 4 | Vérification de types | `compilateur/verifieur.rose` (2 750 l.) | `front.rs` | ✅ source |
| 5 | Émission du dump RIR | `compilateur/emetteur.rose` (4 025 l.) — `bdbef636…` | `rose/src/rir.rs` | ✅ source |
| 6 | Abaissement RIR → code machine **général** | ❌ refusé (`app`, `bin`, `saut`) ; stub `exit(K)` seulement | `rose/src/natif_emetteur.rs` | ❌ |
| 7 | Writer ELF/PE/Mach-O complet | ❌ ELF 132 o, `e_shoff=0`, aucune section | `rose/src/link_natif.rs`, `macho_writer.rs`, `elf_linker.rs` | ❌ |
| 8 | Runtime, bigint, GC, E/S, allocateur | ❌ absent | `std`, `num-bigint`, `i128x.rs`, `sys_io.rs`, `libc` | ❌ |
| 9 | Binaire moteur exécutable | ❌ aucun | `rose/src/main.rs` + `lib.rs` + `front.rs` + `jit.rs` (Cranelift) + `interp.rs` — `cargo build --release` | ❌ |
| 10 | Bootstrap sans rustc (`rose-2` produit `rose-3`) | ❌ | ❌ | ❌ |

→ **5 / 10 = 50 %** au sens « implémentation Rose prouvée » ; **0 %** au sens « produit reconstruit sans rustc ».

## 6. Ce qui oblige encore `cargo` / `rustc`

1. **Le binaire livré est le binaire Cargo.** `md5(rose/rose) == md5(rose/target/release/rose) == c21b91a46b58995f11138c766f2ec2dc`.
   `Cargo.toml` déclare `autobins = false`, `[[bin]] name = "rose" path = "src/main.rs"`,
   dépendances `num-bigint 0.5`, `num-integer`, `num-traits`, `libc 0.2`,
   `inkwell 0.7` (feature `llvm`, optionnelle), `cranelift 0.137` (feature `native`, défaut),
   `[build-dependencies] cc = "1"`.
2. **`rose comp` n'est pas de l'auto-hébergement.** Sur `p47.rose`, il produit un exécutable de
   **5 128 968 octets** (contre 5 128 936 pour `rose/rose`, soit le moteur + 32 octets de source collée) :
   `file` identique (BuildID `d249e3f…`), et `strings` y trouve
   `/rustc/58cf8f9c…/library/...`, `cranelift-module-0.137.1`, `cranelift-codegen-…/inst_builder.rs`.
   C'est « Rust + source collée », pas un binaire synthétisé par Rose.
   Exécution : affiche `47`, rc=0.
3. **G2/G3/G4 non atteints** (constats du dépôt, cohérents avec mes mesures) : Cranelift encore dans
   `Cargo.toml:65` ; `i128x.rs` + `mul_rapide.rs` = 270 erreurs en `no_std` ; 783 symboles `std`
   ancrés dans le binaire `release-dbg` non stripé ; `catch_unwind`/`Any` incompatibles avec `panic=abort`.
4. **L'émetteur Rose manque** : `_start`/trampolines AAPCS64, appels, frames, runtime bigints,
   PE64, sections/symboles ELF, valeurs > 255 (tronquées à `K = code % 256`).

## 7. Écart supplémentaire trouvé pendant la mesure : le mot-clé `var`

Le moteur Rust accepte `var x = 47` (`rose/src/front.rs:1069-1077`, ajouté par le commit
`26987c55` le 08/11/2026). **Aucun des front-end Rose ne le connaît** (`grep '"var"'` dans
`compilateur/lexeur.rose` → 0 résultat).

Mesure reproductible :

```sh
./rose/rose rir $W/p47.rose            > $W/a.rir   # md5 fe0f3ae4aafd98756c27787a507ea96c
./rose/rose compilateur/emetteur.rose $W/p47.rose > $W/b.rir   # md5 8f27be11d32d4ed272d0219664aa2976
# $W/b.rir contient : Rose : erreur — [Ligne 1, Colonne 0] Variable 'var' utilisée sans valeur
```

Les 4 oracles échouent sur `var` : l'AST Rose ajoute `expr (v var)`, le vérificateur Rose rejette
`Variable 'var' utilisée sans valeur`, et l'émetteur Rose écrit ce rejet sur stdout (rc=0).
Sur un corpus classique sans `var` (`examples/math.rose`, `arithmetique.rose`, `fibonacci.rose`,
`rose_puissances.rose`, `rose_flottants.rose`), les 4 oracles donnent **20/20 OK**
(`python3 /tmp/opencode/rose-selfhost/oracles2.py`). Le point fixe reste valide parce que
`emetteur.rose` n'utilise pas `var` dans son propre source.

Ce correctif toucherait `compilateur/{lexeur,analyseur,verifieur,emetteur}.rose`, qui appartiennent
à d'autres agents : **non appliqué**. Patch proposé, non appliqué, dans `rose/docs/self_hosted.patch`.

## 8. Ce que j'ai comblé / ce qui reste

**Comblé** (sans toucher aux fichiers d'autres agents) :
- ce document (`rose/docs/SELF_HOSTED_REEL.md`) avec la réponse factuelle et les preuves rejouées ;
- `rose/docs/self_hosted.patch` : patch **proposé, NON appliqué** (mot-clé `var` + commande de
  diagnostic `rose bootstrap`, et description du travail restant) ;
- scripts de mesure reproductibles sous `/tmp/opencode/rose-selfhost/` (`oracles.py`, `oracles2.py`).

**Reste, et que je ne peux pas faire dans ce chantier sans enfreindre les règles de propriété :**
- le passage `var` dans les 4 `.rose` (fichiers d'autres agents) ;
- l'abaissement RIR général en Rose (des milliers de lignes : `bin`, `app`, `sauts`, frames AAPCS64) ;
- le writer ELF/PE complet et le runtime en Rose ;
- la sortie du binaire Rust (port `no_std` G3 : 270 erreurs, 783 symboles `std`), puis G2 (Cranelift) et G4.

Aucun de ces travaux ne se résume à un petit patch sur `main.rs`/`lib.rs`/`Cargo.toml` :
**prétendre le contraire serait faux.**

## 9. Invocation exacte de re-validation

Toutes les commandes des §2, §3, §4 et §7 sont dans un seul script rejouable :

```sh
cd /home/clausake/Stefan/ROSE
W=/tmp/opencode/rose-selfhost
# (1) G4 : chaîne p47.rose -> p47.rir -> test.elf -> exit 47
# (2) Point fixe : bdbef63681608a0b22a5637539e9d738 des deux côtés
# (3) Bout en bout : emetteur.rir -> emetteur_elf.rose -> rc=2, 0 octet
# (4) Oracles : python3 $W/oracles2.py  -> 20/20
```

Documents de référence : `rose/docs/AUTOHOST_G4.md` (G4 = stub, porte non atteinte),
`rose/docs/AUTONOMIE_COMPLETE.md:14-21` (G0 fait, G4 partiel), `rose/docs/ARCHI_INTERP.md:176-177`.

## 10. Re-validation après reconstruction concurrente du binaire

Le binaire `rose/rose` est passé de `c21b91a46b58995f11138c766f2ec2dc` à
`33ec2e3b587a5c1842e3a874dd3013ef` en cours de chantier (autre agent, `cargo build --release`).
Les mesures critiques ont été **rejouées intégralement sur le binaire final** :

```sh
md5sum compilateur/emetteur_elf.rose compilateur/emetteur.rose rose/rose rose/target/release/rose
# afeffb3b477c44bf591fcc2c22f52167  compilateur/emetteur_elf.rose  (inchangé)
# 66ccc84d3f3afb08b1e2e7b7612538af  compilateur/emetteur.rose
# 33ec2e3b587a5c1842e3a874dd3013ef  rose/rose
# 33ec2e3b587a5c1842e3a874dd3013ef  rose/target/release/rose

./rose/rose rir $W/p47.rose > $W/p47b.rir           # md5 fe0f3ae4... (identique)
./rose/rose compilateur/emetteur_elf.rose $W/p47b.rir > $W/testb.elf
# rc=0, 132 o, md5 e498763a3cab57691f6bfa2e42cb004a (identique)
chmod +x $W/testb.elf && $W/testb.elf; echo $?       # EXIT=47

./rose/rose rir compilateur/emetteur.rose > $W/fp1.rir
./rose/rose compilateur/emetteur.rose compilateur/emetteur.rose > $W/fp2.rir
md5sum $W/fp1.rir $W/fp2.rir
# bdbef63681608a0b22a5637539e9d738  ×2 (260 692 o), cmp = IDENTIQUE

./rose/rose compilateur/emetteur.rose $W/p47.rose | md5sum
# 8f27be11d32d4ed272d0219664aa2976 — écart `var` toujours présent
```

**Aucune conclusion du document n'est invalidate par la reconstruction concurrente.**
Le binaire final reste un produit `cargo build --release` (md5 identique à `target/release/rose`).

---

**État final factuel** : G4 rejoué ✅ (ELF 132 o, `exit 47`) ; point fixe `bdbef636…` rejoué ✅ ;
bout-en-bout ❌ (rc=2, 0 o) ; réflexivité ❌ ; oracles 20/20 ✅ ; écart `var` mesuré ✅.
Le binaire `rose/rose` nécessite toujours `cargo build --release`. **Rose n'est pas entièrement self-hosted.**
