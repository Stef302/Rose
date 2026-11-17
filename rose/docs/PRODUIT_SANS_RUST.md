# M5.3 — Produit final sans dépendance Rust : étude et préparation

**Porte du PLAN** (`PLAN.md:260-265`) : « quand M4.5 est atteint, le binaire final est
produit par le compilateur auto-hébergé ; le moteur Rust n'est plus livré. Porte :
`rose --version` du binaire auto-hébergé ne dépend d'aucune bibliothèque Rust. »

**Nature du livrable** : étude + préparation uniquement. Aucun fichier source, `Cargo.toml`,
`PLAN.md` ou fichier d'un autre agent n'a été modifié. Aucun commit n'a été fait.
Au début de la mission, `git status --short` était vide (HEAD `3108921`, branche `main`).
En fin de mission, d'autres agents ayant travaillé en parallèle, il montre aussi
`rose/src/link_natif.rs` (agent B2, M3.2), `scripts/mesurer-wasm.sh` et
`rose/docs/PORTE_WASM.md` (agent B6) comme fichiers non suivis — **aucun de ces fichiers
n'a été créé ou touché par ce document** (la liste peut encore s'allonger).

**Blocage** : M4.5 (point fixe octet) n'est pas atteint (`PLAN.md:50`, `PLAN.md:232-235` :
« EN COURS (auto-émission validée, point fixe octet non atteint) »). Tant que le
compilateur auto-hébergé ne se reproduit pas à l'octet près, il ne peut pas être la
source du binaire final. Le binaire livré aujourd'hui est **encore intégralement
produit par rustc** et le déclare lui-même (voir §2.3).

---

## 1. État exact de ce qui existe déjà (citations de code vérifiées)

### 1.1 M5.1 — arithmétique exacte Rose pur ✅ (mais encore sur `num-*` et `std`)

`rose/src/i128x.rs:1-11` :

```rust
// Rose — arithmétique exacte SANS GMP, sans FFI, sans dépendance externe.
// Entiers : num_bigint::BigInt (illimité, pur Rust). Rationnels : (BigInt,BigInt).
use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::{One, Signed, ToPrimitive, Zero};
use std::cmp::Ordering;
```

* GMP n'est pas une dépendance : `rose/Cargo.toml` ne contient ni `rug` ni `gmp`
  (vérifié par `grep -ni 'rug\|gmp' rose/Cargo.toml` → aucun résultat). La règle absolue
  est tenue.
* La façade `gmp_*` est **une API maison** sur `BigInt` (`i128x.rs:660-836`), pas un
  appel à la bibliothèque GMP.
* `rose/src/mul_rapide.rs:1-3` : « Multiplication d'entiers RAPIDE, à la manière de GMP
  — sans GMP » ; NTT Montgomery + arbre de produit équilibré (`i128x.rs:20-97`,
  `178-358`). `PLAN.md:244-250` : 1 000 000 ! = 5 565 709 chiffres en 200 ms, porte tenue.
* **Limite pour M5.3** : ces deux modules utilisent encore `std` (13 occurrences de
  `std::` dans `i128x.rs`, 41 dans `mul_rapide.rs` — `std::thread`, `std::collections`,
  `std::cmp`, `std::mem`…) et les crates `num-bigint`, `num-integer`, `num-traits`.
  « Sans GMP » ≠ « sans Rust std ».

### 1.2 M5.2 — E/S syscalls bruts ✅ (module propre, NON câblé au binaire)

`rose/src/sys_io.rs:1-6` : « E/S et mémoire SANS `std` — jalon M5.2. Ce module ne
contient AUCUN chemin `std::` : il ne dépend que de `core`, de `alloc` et (sur Linux) de
la crate `libc`… Il est donc compilable dans une crate `#![no_std]` avec
`extern crate alloc;`. »

* Vérification : les 5 occurrences de `std::` dans le fichier sont **toutes dans des
  commentaires** (lignes 3, 32, 94, 428, 448) — aucune dans du code exécutable.
* API : `ouvrir`/`lire`/`ecrire`/`ecrire_tout`/`fermer`/`lire_fichier`/`ecrire_fichier`,
  plus `linux::{openat,read,write,close,mmap,munmap}` via `libc::syscall`
  (`sys_io.rs:108-233`, `360-456`) et `win32::*` (`sys_io.rs:237-358`).
* Déclaré dans `rose/src/lib.rs` : `/// M5.2 — E/S et mémoire par appels système directs
  (sans std). pub mod sys_io;`.
* **Limites documentées** (`rose/docs/NO_STD.md:6-9`, `120-151`) : module livré mais non
  câblé dans `main.rs` ; la preuve `no_std` a été faite dans un **crate temporaire**
  (`/tmp/opencode/rose_nostd_check`) avec allocateur maison + `panic_handler` +
  `rust_eh_personality`, pas dans Rose. Et `libc` a `default = ["std"]` :
  `Cargo.toml:52` = `libc = "0.2"` (NO_STD.md:29-32), donc `std` revient par `libc`
  tant que la ligne n'est pas `libc = { version = "0.2", default-features = false }`.
* `main.rs` utilise encore massivement std : `std::fs` ×6, `std::io` ×3, `std::env` ×10,
  `std::time` ×1, `std::process` ×40 (comptage par `grep -c`).
* Horloge non couverte : `std::time` reste dans `main.rs:612`, `debug_lenteur.rs:27`,
  `run.rs:143-172`, `setup_gui.rs:2903` (NO_STD.md:221-230).

### 1.3 M3.1 — émetteur machine depuis le RIR ✅ (module fait, câblage en cours)

`rose/src/natif_emetteur.rs:1-6` : « ÉMETTEUR MACHINE DEPUIS LE RIR (sans LLVM, sans
Cranelift)… AUCUN OUTIL EXTERNE : l'émetteur écrit les octets lui-même. »

* Traduit `CorpsRir` en octets x86-64 / aarch64, ABI `i64 code(base, tramp)`
  (`natif_emetteur.rs:8-38`), sous-ensemble chaud : `Affect`, `RetourAbstrait`, `Saut`,
  `SautSiFauxAbstrait` + arithmétique i64 et trampoline pour le reste. `PLAN.md:379` :
  module + 6/6 tests, 1,57 ms (commit `8fdef8d`) ; `PLAN.md:161-167` : porte ≤ 3 ms.
* Déclaré dans `lib.rs` (`#[cfg(not(target_arch = "wasm32"))] pub mod natif_emetteur;`).
* **Limite pour M5.3** : `natif_emetteur.rs:42-43` = `use std::fmt; use std::io;`
  (23 occurrences de `std::` dans le fichier). Câblage dans le JIT en cours (agent B4,
  `PLAN.md:394`) — sans ce câblage, Cranelift reste le chemin d'exécution machine.

### 1.4 M3.2 — link natif sans gcc ⚠ (en cours par un autre agent)

* Au moment de la première mesure, `rose/src/link_natif.rs` **n'existait pas**
  (`ls` → « No such file or directory »). Le fichier a été créé **pendant** cette
  mission par l'agent B2 (il apparaît comme non suivi dans `git status` en fin de
  mission). Son en-tête lu après coup (`link_natif.rs:1-24`) annonce « ELF64 / PE64
  ÉCRITS À LA MAIN… ZÉRO outil externe », avec un stub `_start` qui appelle le code de
  `natif_emetteur` et termine par `exit`. ⚠ Le trampoline embarqué est un **stub qui
  rend 0** : les programmes utilisant `AppelRir`/`eval_fallback` sont refusés. Le module
  utilise encore `std::fmt` et `std::path::Path` (`link_natif.rs:25-26`). La porte
  `PLAN.md:392` (« ELF exécutable sans gcc, exit 49 ») reste à valider par l'agent B2.
* `rose/src/elf_linker.rs:1-7` **ne remplit pas cette porte** : « Linker ELF (délégation
  à cc/ld pour la résolution libc)… On délègue le lien à `cc`/`gcc`/`ld` du système »,
  via `use std::process::Command` (`elf_linker.rs:7`, appels `Command::new("cc")` ligne 38
  et `Command::new("ld")` ligne 44). De plus `Cargo.toml` a `[build-dependencies] cc = "1"`.
* `compilateur/emetteur_elf.rose:1-4` est un émetteur ELF64 **écrit en Rose**, zéro
  dépendance externe, mais il ne produit qu'un binaire minimal « affiche le texte et
  sort avec un code » (`emetteur_elf.rose:110`, `516`, `659`, `738-775`, `819-900`) :
  démonstration de faisabilité, pas un backend complet pour un programme Rose arbitraire.

### 1.5 M4.x — auto-hébergement (blocage direct)

* Présents : `compilateur/lexeur.rose`, `analyseur.rose`, `verifieur.rose`,
  `emetteur.rose`, `compilateur.rose`, `emetteur_cpp.rose`, `emetteur_rust.rose`,
  `emetteur_elf.rose`.
* `PLAN.md:376` : M4.4 fait — 30/46 + auto-émission (commit `08c42a0`).
* `PLAN.md:50, 391` : M4.5 « EN COURS (auto-émission validée, point fixe octet non
  atteint) », porte : `rose-2` émet le MÊME RIR octet pour octet que `rose-1` sur
  `compilateur.rose`. Agent B1 en cours.
* ⚠ `self_host/BOOTSTRAP.md` utilise une **numérotation différente** (v5.0, « Jalon M5 »
  = interpréteur en Rose) et déclare le moteur d'exécution encore en Rust
  (`BOOTSTRAP.md:8-20`, case « Moteur d'exécution (interprète + JIT) | Rust | ❌ »). Ce
  n'est PAS le M5 du `PLAN.md` : ne pas confondre les deux feuilles de route.

---

## 2. Chaîne de dépendances qui bloque M5.3

### 2.1 Chaîne (du binaire livré vers rustc)

```
rose/target/release/rose  (4 866 792 octets, ELF aarch64 static-pie, STRIPPED)
├─ src/main.rs        std::env ×10, std::process ×40, std::fs ×6, std::io ×3, std::time ×1
├─ src/lib.rs → modules Rust :
│   ├─ interp.rs (143 « std:: »), run.rs (62), jit.rs (86), front.rs (14), rir.rs (13)…
│   ├─ i128x.rs (13) + mul_rapide.rs (41) → crates num-bigint / num-integer / num-traits
│   ├─ natif_emetteur.rs (23 : std::fmt, std::io) — module, pas encore chemin principal
│   └─ sys_io.rs (0 « std:: » exécutable) — module propre mais non câblé à main.rs
├─ libc = "0.2"  (Cargo.toml:52, default-features = ["std"] → std réinjecté à l'édition de lien)
├─ cranelift 0.137 (JIT, 487 symboles v0-manglés dans la rlib) + inkwell/LLVM optionnel
└─ build-dependencies : cc = "1"  (gcc nécessaire pour CONSTRUIRE, pas pour EXÉCUTER)
```

**Lecture importante** : `ldd` → « statically linked » signifie qu'aucune bibliothèque
n'est chargée à l'exécution ; cela **ne signifie pas** « pas de std Rust ». Le `std` est
**lié en statique à l'intérieur** du binaire (preuve : chaînes `library/std`, `rustc/`,
`panicked at`, et 886 symboles v0-manglés référençant `std` dans la rlib non stripée).

### 2.2 Mesures de référence réelles (commandes exécutées le 17/11/2026, `uname -m` = aarch64)

Commandes demandées, sorties exactes :

```
$ nm rose/target/release/rose | grep -c 'std::'
0
$ ldd rose/target/release/rose
	statically linked
$ du -h rose/target/release/rose
4.7M	rose/target/release/rose
$ file rose/target/release/rose
rose/target/release/rose: ELF 64-bit LSB pie executable, ARM aarch64, version 1 (SYSV), static-pie linked, BuildID[sha1]=613315bf4165cc8a84c269a9fdc348c1bbe32b4d, for GNU/Linux 3.7.0, stripped
```

⚠ **Le « 0 » de `nm | grep -c 'std::'` est un artefact de strip, PAS une preuve
d'absence de std.** `nm rose/target/release/rose` répond en effet :
`nm: rose/target/release/rose: no symbols`. Tous les exécutables présents dans
`rose/target/` (release, release/deps, cross-targets) sont stripés ; il n'existe aucun
binaire non stripé avec symboles pour refaire cette mesure directement.

Mesures de contournement réellement exécutées :

| Commande | Résultat |
|---|---|
| `./rose/target/release/rose --version` | `Rose 0.1.5 — compilateur Rust + JIT Cranelift (sémantique exacte)` |
| `strings … \| grep -c 'std::'` | `2` |
| `strings … \| grep 'std::'` (lignes exactes) | `stdoutstderrstd::process::exit called re-entrantlystack backtrace:…` / `LazyLock instance has previously been poisoneduse of std::thread::current() is not possible after the thread's local data has been destroyedfatal runtime error: init_current() was re-entrant, …` |
| `strings … \| grep -o 'std::[a-z_:]*' \| sort -u` | `std::process::exit`, `std::thread::current` |
| `strings … \| grep -ci 'library/std'` | `52` (ex. `/rustc/58cf8f9c35b355199727627d07ae34a4e2846d67/library/std/src/thread/scoped.rs`) |
| `strings … \| grep -ci 'rustc/'` | `110` |
| `strings … \| grep -ci 'cranelift'` | `117` |
| `strings … \| grep -ci 'panicked at'` | `3` |
| `readelf --dyn-syms …` | 3 entrées (index 0 + `.init` + `.data`) : aucun symbole dynamique applicatif |
| `size …` | `text 4702578 · data 145280 · bss 25416 · dec 4873274` |

Comptage sur les **objets non stripés de la rlib** (`librose.rlib`, 6110 symboles ;
mangling Rust v0, donc `std::` n'apparaît pas littéralement mais `3std` oui) :

| Motif (`nm rose/target/release/librose.rlib \| grep -c …`) | Compte |
|---|---|
| `std::` (littéral) | `0` |
| `3std` (crate `std` en mangling v0) | `868` |
| `std` (insensible à la casse) | `886` |
| `5alloc` | `2029` |
| `4core` | `2583` |
| `cranelift` | `487` |
| `num_bigint` | `295` |
| `hashbrown` | `241` |

Exemples de symboles réels confirmant la dépendance (extraits de `nm` sur la rlib) :
`…_3std4hash6random11RandomState…`, `…_3std6thread5localINtB6_8LocalKey…`,
`…_3std7processNtB6_7Command4args…`, `…_4core3stre12trim_matches…`,
`…_10cranelift13compiled_blob12CompiledBlob…`.

**Conclusion de mesure** : le binaire livré dépend bien de Rust std (statiquement),
de Cranelift, de num-bigint et de libc avec feature `std`. La porte M5.3 n'est pas
tenue. Le `0` affiché par la commande demandée ne doit **jamais** être cité comme une
réussite.

### 2.3 Auto-déclaration du binaire

`rose --version` imprime : « **compilateur Rust + JIT Cranelift** (sémantique exacte) ».
Même sans `nm`, le produit dit lui-même ce qu'il est. Une porte M5.3 réussie devra
imprimer une version équivalente depuis un binaire **émis par le compilateur Rose**,
sans que cette chaîne `rustc/`/`library/std`/`cranelift` apparaisse dans `strings`.

---

## 3. Plan concret : produire un binaire final sans Rust

Ordre imposé par le `PLAN.md:355-364` (M4.5 → M3.1/M3.2 → M5.2/M5.3). Aucune étape
ci-dessous n'a été exécutée dans ce document : ce sont les portes à faire valider par
les agents concernés puis par l'orchestrateur.

**G0 — M4.5, point fixe (B1, bloquant).**
Faire en sorte que `rose-2`, fabriqué par `rose-1`, émette un RIR octet-identique à
celui de `rose-1` sur `compilateur.rose` (`PLAN.md:232-235`). Porte : `cmp` octet par
octet sur le corpus doré. Sans G0, il n'existe pas de compilateur auto-hébergé pouvant
produire le binaire final.

**G1 — M3.2, link natif sans gcc (B2, fichier apparu pendant cette étude).**
Valider `rose/src/link_natif.rs` (+ `docs/LINK_NATIF.md`) : écriture directe ELF/PE, sans
appel à `cc`/`gcc`/`ld` (contrairement à `elf_linker.rs:7,38,44`), et **sans stub de
trampoline rendant 0**. Porte : exécutable ELF qui sort avec le code 49 sur une machine
sans gcc ; vérifier `readelf -h` + exécution. Le module doit aussi être débarrassé de
`std::fmt`/`std::path` pour la porte G3.

**G2 — M3.1 câblé et remplacement de Cranelift (B4 puis retrait).**
Câbler `natif_emetteur` dans le JIT (`fib machine = 75025`), puis retirer le chemin
Cranelift du binaire livré. Porte mesurable : `strings … | grep -ci cranelift` passe de
`117` à `0`, et les `487` symboles `cranelift` de la rlib disparaissent. Le module
`natif_emetteur.rs` lui-même doit être débarrassé de `std::fmt`/`std::io`.

**G3 — no_std + alloc seul dans Rose (chantier d'envergure, après G0-G2).**
1. `Cargo.toml:52` : `libc = { version = "0.2", default-features = false }`.
2. Remplacer dans `main.rs` : `std::env` (10), `std::process` (40), `std::fs` (6),
   `std::io` (3), `std::time` (1) par `sys_io` ; ajouter à `sys_io` ce qui manque :
   `stat`, `fchmod`, `unlink`, `mkdir`, `rename`, `readlink`, `clock_gettime`
   (NO_STD.md:221-230, 252-255).
3. Amorçage du binaire : `#[global_allocator]` (allocateur mmap maison), `#[panic_handler]`,
   `#![no_main]` + `extern "C" fn main()`, stub `rust_eh_personality` (NO_STD.md:232-243).
4. Traiter les 23 fichiers source contenant `std::` (classement §2.1) ; `interp.rs` (143),
   `setup_core.rs` (138), `setup_gui.rs` (109), `jit.rs` (86), `run.rs` (62) sont les
   gros postes. `catch_unwind`, `HashMap`, `String`, `format!`, threads sont à remplacer.
5. Porte : build **non stripé** puis `nm … | grep -c 'std::'` = 0 (résultat significatif
   cette fois), `ldd` = `libc.so.6` seulement ou « statically linked » sans chaînes
   `library/std`/`rustc/`. ⚠ Exiger un build non stripé, sinon le `0` est encore un
   artefact (§2.2).

**G4 — sortie du moteur Rust du produit (le cœur de M5.3).**
Compiler `compilateur.rose` avec le moteur devenu simple amorce → `rose-1` ; `rose-1`
compile `compilateur.rose` → `rose-2` (G0) ; généraliser `emetteur_elf.rose` au-delà du
stub write+exit pour émettre le binaire complet. Porte : `rose --version` rendu par ce
binaire, avec (a) sortie identique à l'oracle Rust actuel, (b) `strings | grep -ci
'rustc/\|library/std\|cranelift'` = 0, (c) `ldd` sans dépendance Rust, (d) `file` confirmant
un ELF exécutable. Le moteur Rust n'est plus livré : il reste au plus dans l'arbre de
source comme amorce de bootstrap, pas dans le produit.

**Protocole de mesure à appliquer à chaque porte** : mêmes commandes que §2.2, sur un
binaire non stripé, avec la rlib comme témoin secondaire (`3std`, `5alloc`, `cranelift`).
Ne jamais accepter un `nm … | grep -c` sans avoir vérifié au préalable que `nm` répond
autre chose que « no symbols ».

---

## 4. Recommandation honnête

1. **Maintenant** : ne rien implémenter de M5.3 dans le moteur. Le bloquant est M4.5,
   détenu par un autre agent ; M3.2 et le câblage M3.1 le sont aussi. Ce document est le
   livrable attendu (B3) et il suffit à la porte « étude + mesures nm/ldd » du
   `PLAN.md:393`. Laisser `main.rs`/`lib.rs`/`Cargo.toml` à l'orchestrateur.
2. **Rapporter sans ambiguïté** que le chiffre `0` de `nm … | grep -c 'std::'` est dû au
   strip, et que le binaire contient bien std (886 symboles v0-manglés `std` dans la rlib,
   `library/std` ×52 dans `strings`, `--version` qui dit « compilateur Rust + JIT
   Cranelift »). Toute lecture inverse serait fausse et retarderait le projet.
3. **Reporter** à après G0-G2 : le passage `no_std` de Rose lui-même (G3) et la production
   du binaire par le compilateur auto-hébergé (G4). Le crate temporaire de M5.2 prouve
   que la mécanique no_std fonctionne, mais il ne couvre ni le moteur ni l'intégration.
4. **Demander à l'orchestrateur** (seul habilité à toucher `Cargo.toml` et aux commits) :
   - d'ajouter un profil de build non stripé, sinon les futures portes `nm` resteront
     non mesurables ;
   - d'arbitrer la sémantique exacte de « sans dépendance Rust » : `sans std` seul
     (num-bigint est no_std et pourrait rester) ou `sans aucun crate Rust`
     (alors i128x/mul_rapide doivent être réécrits en Rose pur). Le PLAN parle de
     « moteur Rust n'est plus livré », ce qui plaide pour la seconde lecture.
5. **Risques à surveiller** : `libc` à features par défaut (`Cargo.toml:52`) réinjecte
   `std` même avec un module `sys_io` propre ; Cranelift (117 chaînes / 487 symboles)
   et `cc` en build-dependency sont les deux autres sources de dépendance ; la porte
   M4.5 « point fixe structurel documenté » ne doit pas être confondue avec le point
   fixe octet exigé par `PLAN.md:234-235` si l'on veut appeler le produit « auto-hébergé ».

---

## 5. Résumé des faits vérifiés

* Fait : M5.1 et M5.2 sont livrés, M3.1 est un module fait, M3.2 (fichier apparu en
  cours de mission) et M4.5 sont en cours par d'autres agents ; aucun ne produit
  aujourd'hui un binaire sans Rust.
* Fait : `nm rose/target/release/rose | grep -c 'std::'` → **0**, mais le binaire est
  `stripped` (`nm` : « no symbols ») : mesure non significative.
* Fait : `ldd rose/target/release/rose` → **statically linked** ; std est donc embarqué,
  pas chargé dynamiquement. Taille 4,7 Mio, ELF aarch64 static-pie.
* Fait : la rlib non stripée contient **886** symboles `std` (868 `3std`), **487**
  `cranelift`, **2029** `5alloc`, **2583** `4core`.
* Fait : `rose --version` → « Rose 0.1.5 — compilateur Rust + JIT Cranelift (sémantique
  exacte) » : le produit s'auto-déclare dépendant de Rust.
* Fait : aucun fichier source, `Cargo.toml`, `PLAN.md` ou fichier d'autrui n'a été touché ;
  aucun commit n'a été fait. Le seul fichier créé par cette mission est ce document
  (les autres fichiers non suivis visibles en fin de mission appartiennent aux agents
  parallèles, voir en tête).
