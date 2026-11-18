# G2 — un pas concret vers l'autonomie : AOT natif vérifié, sans Cranelift *dans le mécanisme*

**Constat daté : 18/11/2026, sur ASUS GX10 (aarch64), Rust 1.99.1, Rose 0.1.5, branche `main`.**
Tout ce qui suit est des sorties réellement exécutées. Re-mesurer après tout changement de
`Cargo.toml`, de feature ou de backend.

---

## 0. Vérité courte (à lire avant de conclure)

- **Le chemin AOT est prouvé fonctionnel** : un RIR pré-analysé (somme 1..10) est émis en
  code machine par `natif_emetteur` (M3.1), écrit en ELF64 statique par `link_natif` (M3.2),
  exécuté deux façons (mmap direct + ELF lancé par le noyau) : **code de retour 55**.
- **Le mécanisme ne contient aucun Cranelift et aucun `libc` crate** : démontré dans le
  harnais isolé `/tmp/opencode/rose_g2_harness/` (zéro dépendance Cargo externe).
- **Mais le binaire livré dans le dépôt n'est PAS encore « sans Cranelift »** : il utilise
  la bibliothèque `rose`, dont `Cargo.toml` déclare `cranelift` en dépendance **non
  optionnelle** sur cible native. Le graphe Cargo contient donc Cranelift et
  `libc features=["default","std"]`. **G2 dans le dépôt n'est pas fait** : c'est un câblage
  à réaliser par l'orchestrateur (retirer la dépendance / feature-gater `jit.rs`), pas une
  lacune de l'émetteur.

> Ne pas écrire « Rose autonome ». Écrire : « le sous-ensemble AOT est autonome ; le
> binaire de démonstration traîne encore Cranelift via la lib ; la preuve zéro-Cranelift
> est le harnais isolé ».

---

## 1. Fichiers livrés

| Fichier | Rôle |
|---|---|
| `rose/src/bin/rose-autonome.rs` | Binaire de démonstration AOT, **sans appel à `rose::jit`**. Construit le RIR directement (`Affect`, `Saut`, `SautSiFauxAbstrait`, `RetourAbstrait`), émet, écrit l'ELF, exécute. Le câblage `[[bin]]` est à faire par l'orchestrateur (ce agent ne touche pas `Cargo.toml`). |
| `rose/docs/AUTONOMIE_G2.md` | Ce document. |
| `/tmp/opencode/rose_g2_harness/` | Harnais isolé à zéro dépendance : recopie fidèle de `natif_emetteur.rs` (lignes 1–775, tests `#[cfg(test)]` retirés) et de `link_natif.rs` (**md5 identique** `b3a4592b…`), plus des types RIR minimaux locaux. Preuve que le blocage est structurel, pas algorithmique. |
| `/tmp/opencode/rose_g2_validate/` | Validation du binaire livré : crate temporaire avec `rose = { path = "/home/clausake/Stefan/ROSE/rose" }` et `[[bin]] path = …/rose-autonome.rs`. |
| `/tmp/opencode/rose_g2_libc_ctrl/` | Contrôle : `libc = { default-features = false }` seul, pour montrer l'état cible `libc features=[]`. |

Aucun des fichiers interdits (`main.rs`, `lib.rs`, `Cargo.toml`, `PLAN.md`, `jit.rs`,
`interp.rs`, `natif_emetteur.rs`, `mul_rapide.rs`, `i128x.rs`) n'a été modifié.
Aucun commit n'a été fait.

---

## 2. Ce qui tourne vraiment sans Cranelift

### 2.1 Binaire `rose-autonome` (validé via `rose_g2_validate`)

```
$ cargo build --bin rose-autonome
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.78s
$ ./target/debug/rose-autonome /tmp/opencode/rose-autonome-elf
AOT natif sur aarch64 : 188 octets machine, 8 ops, 0 appels runtime, 0 fallbacks
exécution machine directe : somme(1..10) = 55
ELF écrit : /tmp/opencode/rose-autonome-elf (1144 octets, magic=[7F, 45, 4C, 46])
À exécuter : /tmp/opencode/rose-autonome-elf ; echo $?   → attendu 55
options de lien : slots=19 base=0x400000
$ /tmp/opencode/rose-autonome-elf ; echo $?
55
ELF exit code = 55
$ file /tmp/opencode/rose-autonome-elf
ELF 64-bit LSB executable, ARM aarch64, version 1 (SYSV), statically linked, not stripped
```

Le programme Rose pré-analysé est `somme(1..10) = 55` : le stub `_start` écrit par
`link_natif` n'initialise pas les slots, donc le programme pose lui-même `n = 10`.
Zéro appel runtime (`appels=0`, `fallbacks=0`) : la trampoline n'est jamais Touchée,
l'ELF n'a pas besoin du runtime Rose embarqué. `readelf -d` sur l'ELF généré :
`There is no dynamic section in this file.` → **aucun linker, aucune libc dynamique**.

### 2.2 Harnais isolé — la vraie preuve « zéro Cranelift »

```
$ cd /tmp/opencode/rose_g2_harness && cargo build
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.23s
$ ./target/debug/rose_g2_harness /tmp/opencode/rose_g2_harness/elf-somme
[HARNESS G2] arch=aarch64 code=188o ops=8 appels=0 fallbacks=0
[HARNESS G2] exécution mmap : somme(1..10) = 55
[HARNESS G2] ELF /tmp/opencode/rose_g2_harness/elf-somme (1144o) magic=[7F, 45, 4C, 46]
[HARNESS G2] OK — AOT natif autonome (sans cranelift, sans libc crate)
$ /tmp/opencode/rose_g2_harness/elf-somme ; echo $?
55
```

Contrôles d'absence :

```
$ cargo tree
rose_g2_harness v0.1.5 (/tmp/opencode/rose_g2_harness)
$ cargo tree -e features -i libc
error: package ID specification `libc` did not match any packages        (rc=101)
$ cargo tree -e features -i cranelift
error: package ID specification `cranelift` did not match any packages   (rc=101)
$ nm target/debug/rose_g2_harness | grep -ci cranelift
0                                                                        (rc=1)
$ strings target/debug/rose_g2_harness | grep -i cranelift
 AOT natif autonome (sans cranelift, sans libc crate)
```

⚠ Le `strings` unique est **notre propre message println**, pas un symbole (le `nm`
donne 0). ⚠ Le binaire du harnais est lié dynamiquement à `libc.so.6` comme tout
binaire `std` : « zéro **crate** libc » ≠ « zéro libc système ». L'**ELF produit**, lui,
est statique et sans section dynamique.

---

## 3. La vraie sortie de `cargo tree -e features -i libc`

### 3.1 Sur le binaire livré (lié à la lib `rose`) — **`libc features=[]` : NON**

```
$ cd /tmp/opencode/rose_g2_validate && cargo tree -e features -i libc
libc v0.2.191
└── rose v0.1.5 (/home/clausake/Stefan/ROSE/rose)
    ├── rose feature "default"
    │   └── rose_g2_validate v0.1.5 (/tmp/opencode/rose_g2_validate)
    │       └── rose_g2_validate feature "default" (command-line)
    └── rose feature "native"
        └── rose feature "default" (*)
├── libc feature "default"
│   ├── cranelift-jit v0.137.2
│   │   └── cranelift-jit feature "default"
│   │       └── cranelift v0.137.2
│   │           ├── cranelift feature "frontend"
│   │           │   └── rose v0.1.5 (/home/clausake/Stefan/ROSE/rose) (*)
│   │           ├── cranelift feature "jit"
│   │           │   └── rose v0.1.5 (/home/clausake/Stefan/ROSE/rose) (*)
│   │           ├── cranelift feature "module"
│   │           │   └── rose v0.1.5 (/home/clausake/Stefan/ROSE/rose) (*)
│   │           └── cranelift feature "native"
│   │               └── rose v0.1.5 (/home/clausake/Stefan/ROSE/rose) (*)
│   ├── memmap2 v0.9.12
│   │   └── memmap2 feature "default"
│   │       └── cranelift-jit v0.137.2 (*)
│   ├── region v3.0.2
│   │   └── region feature "default"
│   │       └── cranelift-jit v0.137.2 (*)
│   └── wasmtime-internal-jit-icache-coherence v50.0.2
│       └── wasmtime-internal-jit-icache-coherence feature "default"
│           └── cranelift-jit v0.137.2 (*)
└── libc feature "std"
    └── libc feature "default" (*)
```

**Réponse à la question posée : non, on n'obtient pas `libc features=[]` ici.** La chaîne
`cranelift 0.137 → cranelift-jit → {memmap2, region, icache}` réactive `libc default+std`.
Cela confirme exactement la conclusion de `NO_STD_MOTEUR.md` §1.1 : `libc/std` est injecté
par Cranelift, pas par la déclaration directe de `rose/Cargo.toml`.

### 3.2 Contrôle — l'état cible est atteignable une fois G2 fait

```
$ cd /tmp/opencode/rose_g2_libc_ctrl && cargo tree -e features -i libc
libc v0.2.191
└── rose_g2_libc_ctrl v0.1.5 (/tmp/opencode/rose_g2_libc_ctrl)
    └── rose_g2_libc_ctrl feature "default" (command-line)
```

Aucune ligne `libc feature "default"` / `"std"` : c'est bien `libc features=[]`. La
déclaration directe de Rose (`libc = { version = "0.2", default-features = false }`) est
donc correcte ; **G2 suffira à faire tomber le `std` du graphe libc**.

### 3.3 Harnais isolé

`cargo tree` = 1 seule ligne (§2.2) : aucun `libc` dans le graphe du tout.

---

## 4. Ce qui bloque encore (honnête)

| Blocage | État mesuré | Porte |
|---|---|---|
| **G2 dans le dépôt** : `cranelift` déclaré sans `optional` dans `[target.'cfg(not(wasm32))'.dependencies]` | `cargo tree -e features -i libc` §3.1 montre `libc feature "std"` via `cranelift-jit` | Retirer la dépendance et feature-gater `jit.rs` dans `lib.rs` (interdit à ce agent). C'est l'étape suivante, à confier à l'orchestrateur. |
| **Récursion / `AppelRir`** : l'émetteur délègue à la trampoline (`fallback_runtime`) ; le runtime Rose n'est pas embarqué dans l'ELF | `ecrire_elf_depuis_emitted` refuse explicitement (`ErreurLien::RuntimeRequis`). Contexte H5 (non re-mesuré ici) : natif 4× plus rapide sur boucle, **234× plus lent sur récursion**. | Embarquer la table de trampolines + le runtime minimal dans l'ELF, ou émettre les appels directs (appels RIR résolus à l'édition de liens). |
| **`std::` dans les deux modules M3.1/M3.2** | `grep -c 'std::'` : `natif_emetteur.rs` = **23**, `link_natif.rs` = **19** (commentaires inclus) | Remplacer `fmt/io/path/fs` par `core::fmt` + `sys_io` (G2.3 de `NO_STD_MOTEUR.md`). |
| **`num-*`/`std`** | `num-bigint`/`num-integer`/`num-traits` déclarés avec leurs défauts dans `rose/Cargo.toml` | `default-features = false` + imports `alloc` (G3.1, prouvé en isolateur mais non appliqué au dépôt). |
| **`catch_unwind` / `Any`** dans `lib.rs` (~l.197 et l.291) | Binaire `no_std` exige `panic="abort"`, incompatible avec `catch_unwind` (voir le commentaire du profil release). | Remplacer par propagation `Result` en gardant la sémantique `essayer`/`attraper` sur le corpus doré (G3.4, **blocage architectural**). |
| **`i128x.rs` / `mul_rapide.rs` en `no_std`** | Re-mesuré le 18/11 sur `/tmp/opencode/rose_i128x_nostd` : **271 erreurs** (`E0425`=125, `E0433`=107, `E0599`=6, + 1 « unwinding panics are not supported without std »). Dominantes : `Vec`/`Box`/`String` non importés de `alloc`, `to_string`/`fract`/`to_u64` hors traits, `std::thread`/`Arc`/`Mutex`. (Le chiffre « 270 » des notes antérieures est périmé.) | Imports `alloc::*`, traits `num_traits`, repli séquentiel `std::thread` **mesuré** (G3.3). |
| **`AppelRir` / fallback générique** | Les expressions hors sous-ensemble passent par `Trampoline::eval_fallback`, absente de l'ELF autonome | Même travail que la récursion : runtime embarqué ou abaissement complémentaire. |

---

## 5. Route G2 → G3 → G4 honnête

- **G2 (retirer Cranelift) — prérequis dur, PAS fait ici.** Le mécanisme AOT est prêt et
  prouvé isolément ; il reste à (a) retirer `cranelift` de `Cargo.toml`, (b) feature-gater
  `pub mod jit;`/`auto_llvm;` dans `lib.rs`, (c) porter les 23+19 `std::` vers `core`/`sys_io`.
  Porte : `cargo tree -e features -i libc` → plus aucun `libc feature "std"` (contrôle §3.2),
  `nm | grep -ci cranelift` = 0 sur le binaire Rose réel.
- **G3 (moteur `no_std` + alloc) — commence seulement après G2.** `num-*` sans `std`,
  syscalls `sys_io`, correctifs `i128x`/`mul_rapide` (271 erreurs à réduire à 0),
  suppression de `catch_unwind`, amorçage `no_main`/`panic_handler`. Porte détaillée dans
  `NO_STD_MOTEUR.md` §3.
- **G4 (binaire émis par le compilateur Rose) — hors portée ici.** Le jalon G4 ne sera
  atteint que quand l'ELF produit par `link_natif` embarquera son propre runtime (appels
  RIR, récursion, E/S) assez pour exécuter un programme Rose réel sans hôte Rust. Le
  présent travail couvre le **sous-ensemble** (boucle arithmétique) : `somme(1..10)=55`,
  pas un interpréteur complet auto-hébergé.

---

## 6. Reproduction rapide

```bash
export PATH="$HOME/.cargo/bin:$PATH"
# 1) binaire livré (crate de validation temporaire)
cd /tmp/opencode/rose_g2_validate && cargo build --bin rose-autonome \
  && ./target/debug/rose-autonome /tmp/opencode/rose-autonome-elf \
  && /tmp/opencode/rose-autonome-elf ; echo "exit=$?"     # 55
# 2) preuve zéro-cranelift
cd /tmp/opencode/rose_g2_harness && cargo build \
  && ./target/debug/rose_g2_harness && ./elf-somme ; echo "exit=$?"   # 55
# 3) graphe libc
cd /tmp/opencode/rose_g2_validate && cargo tree -e features -i libc
cd /tmp/opencode/rose_g2_harness && cargo tree -e features -i libc    # rc=101
```

**État final :** compilation AOT autonome du sous-ensemble PROUVÉE (ELF statique, exit 55) ;
`libc features=[]` NON obtenu sur le binaire lié à la lib (cranelift réactive `std`) ; G2
dans le dépôt reste à faire par l'orchestrateur ; aucun fichier interdit modifié, aucun
commit.
