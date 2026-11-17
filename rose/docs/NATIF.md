# M3.1 — Émetteur machine natif depuis le RIR (sans LLVM externe)

Fichier : `rose/src/natif_emetteur.rs` (nouveau, **non déclaré** dans `lib.rs` /
`main.rs` — le câblage est à la charge de l'orchestrateur).

## 1. Principe

Le RIR (`rose/src/rir.rs`) est déjà une forme d'exécution linéaire : `Vec<Op>`,
slots indexés, sauts par index d'opération. L'émetteur traduit chaque opcode du
sous-ensemble chaud directement en octets machine. **Aucun LLVM, aucun
Cranelift, aucun outil externe** : les octets sont construits bit à bit en Rust.

### Modèle de valeurs — point critique

Le code émis manipule des **i64 non boxés**, un slot = 8 octets. Il **ne touche
jamais** à la layout de `Valeur` (enum Rust non garantie). Tout ce qui sort de
l'arithmétique entière est délégué à une trampoline `#[repr(C)]` passée en
paramètre. Conséquence : aucun couplage à la représentation Rust, donc pas de
fragilité lors d'un changement de `Valeur`.

### ABI

```
i64 code(base: *mut i64, tramp: *const Trampoline)
```

|                    | aarch64 (AAPCS64) | x86-64 (SysV) |
|--------------------|-------------------|---------------|
| `base`             | x0 → copié en x19 | rdi → copié en rbx |
| `tramp`            | x1 → copié en x20 | rsi → copié en r12 |
| valeur courante    | x0 (**registre de retour**) | rax |
| second opérande    | x9                | rcx |
| résultat           | x0                | rax |
| registres sauvés   | x19, x20, x29, x30 | rbx, r12, rbp |

Prologue/épilogue стандарт (save des callee-saved, `ret`). Les slots
temporaires sont `n_slots .. n_slots+15` (`SCRATCH_SLOTS = 16`) — l'appelant
doit allouer `Emitted::n_slots_total` i64.

### Trampoline (`#[repr(C)]`, décalages 0/8/16 garantis)

```rust
#[repr(C)]
pub struct Trampoline {
    pub appel_rir: extern "C" fn(i64, i64, i64, i64, *const u8) -> i64,
    pub eval_fallback: extern "C" fn(*mut c_void, u32, *mut i64) -> i64,
    pub ctx: *mut c_void,
}
```

* `appel_rir(a0,a1,a2,a3, nom)` : ABI **identique** à
  `jit::rose_appel_interpreteur` (réutilisable tel quel). Le nom est une
  chaîne NUL-terminée émise juste après le code et référencée par `adr` (aarch64)
  ou `lea rip-relative` (x86-64) — donc **aucune relocation absolue**.
* `eval_fallback(ctx, expr_id, base)` : évalue une expression hors sous-ensemble
  (flottants, `Div`, `Mod`, `Puissance`, `DivEntier`, `Et`, `Ou`). L'hôte
  reconstruit la table des expressions **dans l'ordre de `Emitted::fallbacks`**.

## 2. Opcode couverts

**`Op`** : `Affect`, `RetourAbstrait`, `Saut`, `SautSiFauxAbstrait`.

| `Eabstrait` | Traitement |
|-------------|------------|
| `Slot(u16, _)` | `ldr`/`mov` depuis `base + slot*8` |
| `Entier(String)` | `movz`/`movk` (aarch64), `mov imm32`/`movabs` (x86-64) ; parse en i64 |
| `Binaire` `+ - *` | `add` / `sub` / `mul` (ou `imul`) |
| `Binaire` `< <= > >= == !=` | `cmp` + `cset` (aarch64), `cmp` + `setcc` + `movzx` (x86-64) → 1/0 |
| `Neg` | `neg` |
| `Non` | `cmp #0` + `cset eq` / `test` + `sete` |
| `AppelRir` | `call tramp.appel_rir` avec jusqu'à 4 arguments-slot et le nom |
| `Flottant`, `Div`, `Mod`, `Puissance`, `DivEntier`, `Et`, `Ou` | `call tramp.eval_fallback` |

Tout autre `Op` (`AffectAst`, `Afficher`, `Lire`, `Bloc`, `Lever`, `Retour(AST)`,
`SautSiFaux(AST)`, `AffectIndex`, `ExprStmt`) → **`EmitError::OperationNonSupportee`**.
`peut_emettre(&CorpsRir) -> bool` permet de pré-créer cette décision sans lever
d'erreur. **Aucune régression** : la fonction reste interprétée.

### Correction des sauts

Deux passes : les offsets octet de chaque op sont mémorisés dans
`Emitted::op_offsets`, puis les déplacements `b`/`b.cond` (19/26 bits) et
`jmp`/`jcc` rel32 sont patchés. Une cible `== ops.len()` = épilogue (utilisé par
`RetourAbstrait`).

## 3. API publique

```rust
pub enum Architecture { Aarch64, X86_64 }
impl Architecture { pub fn native() -> Self; pub fn nom(self) -> &'static str; }

pub fn emettre(corps: &CorpsRir, arch: Architecture) -> Result<Emitted, EmitError>;
pub fn peut_emettre(corps: &CorpsRir) -> bool;

pub struct Emitted {
    pub code: Vec<u8>,              // code + noms NUL-terminés
    pub architecture: Architecture,
    pub n_slots: usize,
    pub n_slots_total: usize,       // à allouer par l'appelant
    pub op_offsets: Vec<usize>,
    pub appels: Vec<Appel>,
    pub fallbacks: Vec<u32>,
}

pub struct MemoireExecutable;       // mmap + mprotect, Drop = munmap
impl MemoireExecutable {
    pub fn nouveau(code: &[u8]) -> io::Result<Self>;
    pub fn as_ptr(&self) -> *const u8;
    pub fn taille(&self) -> usize;
    pub unsafe fn appeler(&self, base: *mut i64, tramp: *const Trampoline) -> i64;
}

pub const SCRATCH_SLOTS: u16 = 16;
pub struct Trampoline { /* ci-dessus */ }
pub struct Appel { pub nom: String, pub args_slots: [Option<u16>; 4] }
pub enum EmitError { /* ... */ }
```

`MemoireExecutable` utilise des `extern "C"` `mmap`/`mprotect`/`munmap` bruts :
**aucune dépendance ajoutée** (règle GMP respectée).

### Câblage suggéré dans `main.rs` (à faire par l'orchestrateur)

1. ligne 21-40 : ajouter `mod natif_emetteur;` à côté de `mod rir;`.
2. près de la ligne 675 (dispatch `orc`/`link`) : une action `natif` qui
   analyse la source, abaisse les signatures via `rir::abaisser`, filtre avec
   `natif_emetteur::peut_emettre`, puis pour chaque fonction :
   `let e = natif_emetteur::emettre(&rir, Architecture::native())?;`
   `let mem = MemoireExecutable::nouveau(&e.code)?;`
   et construit une `Trampoline` dont `appel_rir` délègue à
   `crate::jit::rose_appel_interpreteur` (même ABI).
3. Pour produire un `Compile::Natif(Box<dyn Fn(Vec<Valeur>) -> Valeur + Send + Sync>)` :
   convertir les `Valeur` d'entrée en i64, appeler `mem.appeler`, réemballer le
   résultat avec `Valeur::z`. La closure capture `mem` (Send + Sync) — la
   `Trampoline` doit être construite **par appel** si son `ctx` est mutable.
4. Même chose dans `lib.rs` (`pub mod natif_emetteur;`) si l'API doit être
   exposée aux tests hors-binaire. **Je n'ai touché ni l'un ni l'autre.**

## 4. Preuves réellement exécutées

Machine : `uname -m` → **aarch64** (donc exécution native aarch64 possible ;
x86-64 vérifié par `qemu-x86_64-static` 8.2.2).

Validation isolée (crate temporaire `/tmp/opencode/natif_test`, qui inclut le
fichier avec des mocks de `rir`/`front` — **aucun fichier du dépôt modifié hors
des deux fichiers possédés**) :

```
cargo test --release        → 6 tests passés / 6
cargo check --bin rose      → Finished (baseline inchangé)
```

Les 6 tests couvrent : émission `mul` sans trampoline (x86 + aarch64, octets
vérifiés), correction des sauts, refus des opcodes hors sous-ensemble,
**exécution réelle via mmap** (`7*7-5 = 44` sur aarch64 natif), et `fib(25)`.

Octets vérifiés par désassemblage croisé :
* aarch64 : `aarch64-linux-gnu-as` + `aarch64-linux-gnu-objdump` ;
* x86-64 : `clang --target=x86_64-linux-gnu` + `llvm-objdump-18`.

Un balayage de conformité `fib(n)` pour `n = 0..=20` (21 valeurs) est aussi
comparé à une référence Rust, **toutes correctes**, avant le chronométrage.

**Exécution x86-64 réelle** : les octets émis ont été injectés comme fonction
dans un binaire freestanding (`clang --target=x86_64-linux-gnu -nostdlib
-static -fuse-ld=lld`) exécuté sous `qemu-x86_64-static` :
* `a*a - b` avec `a=7, b=5` → **44**, code de sortie 0 ;
* `fib(25)` complet (appels RIR via la trampoline) → **75025**, code de sortie 0.

### Porte de performance — CONSTAT DATÉ (17/11/2026)

`fib(25)` compilé par cet émetteur, avec récursion passant par la trampoline
(shaf `ebenche_fib25`, crate isolée, profil `--release`, hôte aarch64) :

| mesure | valeur |
|--------|--------|
| résultat | 75025 (correct) |
| durée (4 salves) | **1,57 ms / 1,57 ms / 2,38 ms / 2,40 ms** |
| octets émis | 268 |
| profil debug (référence) | 5,88 ms |

→ La porte « ≤ 3 ms » est **atteinte dans ce harnais isolé en release**. Ce
n'est **pas** une mesure du binaire `rose` (le câblage CLI n'est pas fait) :
à re-mesurer impérativement après intégration, avec les mêmes précautions que
toute mesure sur cette machine (salves entrelacées, médianes, moteur arrêté).

## 5. Limites / ce qui manque

1. **Parité sur les 40 dorés de `bench/dores/` non démontrée par ce module seul.**
   Les opcodes non couverts sont refusés (`Err`) ; le repli interprété doit être
   câblé par l'orchestrateur. Ce module garantit l'absence de régression, pas
   la couverture complète.
2. **Booléens** : vrai = 1, faux = 0. `SautSiFauxAbstrait` saute si la valeur
   n'est pas exactement 1. Conforme au moteur pour les comparaisons et les
   valeurs non-booléennes ; un **entier littéral 1 utilisé directement comme
   test** serait traité comme vrai (divergence connue).
3. **Arithmétique i64 seulement** : pas de débordement vers i128/natif exact ;
   `Entier` hors i64 → `EmitError::EntierHorsPlage`. Les programmes needing
   l'exactitude au-delà de i64 doivent rester interprétés ou passer par la
   trampoline.
4. **Appel RIR ≤ 4 arguments** (limite de l'ABI de `rose_appel_interpreteur`).
   `AppelRir` dont un argument n'est pas un `Expr::Var` (slot) → erreur —
   conforme à `abstraire`, qui ne produit que des arguments-slots.
5. **`adr` aarch64** : portée ±1 Mo ; largement suffisant pour ces fonctions,
   mais une fonction > 1 Mo de code casserait.
6. **Pas de PIC / pas de relocation absolue** : le code doit être exécuté à
   l'adresse où il a été émis (mmap). Le déplacer after coupure invaliderait
   les `adr`/`lea` ? **Non** : ce sont des références PC-relatives, donc le
   déplacement du bloc entier est sûr. En revanche les sauts internes le sont
   aussi. Ajuster seulement si on ajoute des appels absolus.
7. **Aarch64 testé nativement ; x86-64 testé sous qemu**, pas sur une vraie
   machine x86-64. `qemu` n'est pas un banc de performance fiable (aucun
   chronométrage x86-64 n'est présenté).
8. **`peut_emettre` est pessimiste** sur les `Op` : un `Affect` dont
   l'`Eabstrait` contient un `Flottant` est émettable (fallback), mais un
   `Afficher` même abstrait ne l'est pas.

## 6. Fichiers possédés / limites de périmètre

Créés : `rose/src/natif_emetteur.rs`, `rose/docs/NATIF.md`.
Non touchés : `main.rs`, `lib.rs`, `Cargo.toml`, `Cargo.lock`, `jit.rs`,
`interp.rs`, `rir.rs`. Aucun commit Git.
