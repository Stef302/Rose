# Câblage de la JVM Rose (`.rosebc`) et de l'allocateur arène dans le moteur

**Date** : 18/11/2026 · **Release** : 0.1.5 · **GMP** : aucune dépendance ajoutée ·
**GitHub** : `Stef302/Rose`

## 1. État — INTÉGRATION RÉUSSIE (sans conflit)

Les deux modules livrés mais non utilisés sont désormais **prouvés intégrables** :

| Module | Livré par | Contenu | Résultat |
|---|---|---|---|
| `rose/src/rosevm.rs` | I3 | `.rosebc` v1, chargeur vérifié, VM RIR, pool de `Gz` (GC), sandbox | ✅ compile + 8 tests + 6 tests d'intégration |
| `rose/src/mem_modele.rs` | I4 | `Vue<T>` non possédante, `AllocateurRose<T>` (Individuel / Arena) | ✅ compile + 5 tests + 2 tests d'intégration |

**Aucune erreur d'intégration** : aucun type privé de `rir` / `run` n'est utilisé. Les deux
modules n'accèdent qu'à des API déjà `pub` :

- `front::{Expr, Instr, analyser}`, `i128x::Gz`, `rir::{CacheSlot, CorpsRir, Eabstrait,
  FinRir, HoteRir, Op, Oper, executer_avec, poser_slot}`, `run::Valeur` ;
- `mem_modele` n'a **aucune** dépendance sur le moteur (autonome, `alloc` uniquement).

⚠ **Conflit évité** : `lib.rs` et `main.rs` ont été modifiés **en parallèle par G2_REEL**
(features `jit` / `autonome`, shim sans Cranelift). Je ne les ai **pas touchés**. Le patch
ci-dessous est fourni à l'orchestrateur pour application après G2_REEL — les points
d'ancrage (`pub mod rir;`, `mod rir;`, `"comp" =>`) restent stables dans la version actuelle.

---

## 2. Preuve réelle — harnais isolé

Harnais : `/tmp/opencode/rose_vm_harnais/` (crate `rose_vm_harnais` 0.1.5).
Son `src/lib.rs` est une copie adaptée du **vrai** `lib.rs` où chaque `pub mod X;` est
remplacé par `#[path = "/home/clausake/Stefan/ROSE/rose/src/X.rs"] pub mod X;`. Le moteur
testé est donc **le vrai code source**, sans copie ni modification du dépôt.

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cd /tmp/opencode/rose_vm_harnais
cargo check --all-targets     # ✅ Finished (vrai jit.rs inclus, warnings préexistants seulement)
cargo test                    # ✅ 179 passed / 0 failed (lib, vrai JIT) + 6 passed / 0 failed (intégration)
```

**Vérification du patch lui-même** : le patch exact du §4 et du §5 a été appliqué à une
**copie** des vrais `lib.rs` / `main.rs` dans `/tmp/opencode/rose_patch_check/`
(vrais modules par liens symboliques, vrai `jit.rs`, feature `jit` active) :

```bash
cd /tmp/opencode/rose_patch_check
cargo check --all-targets    # ✅ Finished — aucun erreur
cargo build --bin rose
./target/debug/rose bc /tmp/opencode/carre_patch.rose -o /tmp/opencode/carre_patch.rosebc --fn carre
# rose bc : carre → /tmp/opencode/carre_patch.rosebc (36 octets, 1 op, 1 slot(s))
./target/debug/rose runbc /tmp/opencode/carre_patch.rosebc 9
# 81
./target/debug/rose mem /tmp/opencode/carre_patch.rose --fn carre 7
# rose mem : carre = 49 (arène : mode=Arena, alloué=1, capacité=1, en_cours=1)
# rose mem : vue arène = 49
# rose mem : liberer_tout() → 1 bloc(s), en_cours=0
./target/debug/rose runbc /tmp/opencode/hostile2.rosebc   # magic='X'
# rose runbc : bytecode Rose : magic `ROSEBC` absent      (rc=1)
```

Chaîne prouvée (tests `a` → `f` dans `tests/integration.rs`) :

- **(a)** `analyser` → `rir::abaisser` → `verifier_sous_ensemble` → `ecrire_bytecode`
  → fichier `.rosebc` de **36 octets** pour `fn carre(n: entier) → entier` ;
- **(b)** `lire_bytecode` (vérification hostile) → `executer_sandbox` → `carre(9) = 81`,
  sortie capturée vide, `gc.prets() = 0`, parité avec l'interpréteur complet ;
- **(c)** le résultat du corps RIR est rangé dans un
  `AllocateurRose::<u64>::avec_capacite(Mode::Arena, n_slots)`, lu via `Vue::lire()`,
  puis `liberer_tout()` → `en_cours = 0` (compteurs `allocations = 1`, `liberations = 1`).

Commandes simulées (binaire `rose_cable`, handlers **identiques** à ceux proposés pour
`main.rs`) :

```
$ rose_cable bc essais/carre.rose -o essais/carre.rosebc --fn carre
rose bc : carre → essais/carre.rosebc (36 octets, 1 op, 1 slot(s))

$ xxd -l 16 essais/carre.rosebc
00000000: 524f 5345 4243 0100 0000 0100 0000 0100  ROSEBC..........

$ rose_cable runbc essais/carre.rosebc 9
81

$ rose_cable runbc essais/carre.rosebc 100000
10000000000                      # entier exact > i64, jamais tronqué

$ rose_cable runbc essais/carre.rosebc 9 --budget 0
rose runbc : sandbox Rose : budget de 0 instructions épuisé     (rc=1)

$ rose_cable runbc essais/carre.rosebc 9 --max-slots 0
rose runbc : sandbox Rose : 1 slots > limite mémoire             (rc=1)

$ rose_cable runbc essais/hostile.rosebc        # magic remplacé par 'X'
rose runbc : bytecode Rose : magic `ROSEBC` absent               (rc=1)

$ rose_cable mem essais/carre.rose --fn carre 7
rose mem : carre = 49 (arène : mode=Arena, alloué=1, capacité=1, en_cours=1)
rose mem : vue arène = 49
rose mem : liberer_tout() → 1 bloc(s), en_cours=0
```

---

## 3. Format `.rosebc` v1 (little-endian)

```
magic[6] = "ROSEBC" | version u32 (=1) | n_slots u32 | n_ops u32 | n_globaux u32
noms     : n_slots  × (u32 len + UTF-8)
globaux  : n_globaux × u16
ops      : n_ops × (u8 opcode + opérandes)
```

Opcode `0x80` = sentinelle « opération refusée » (AST, `AppelRir`, `Lire`) : écrite par
`ecrire_bytecode`, **rejetée par `lire_bytecode`**. C'est le chargement — côté attaquant —
qui fait autorité. Les entiers sont du **texte décimal longueur-préfixée par un u32**
(jamais de i64 fixe, jamais GMP). Le `CacheSlot(Cell)` de `Eabstrait::Slot` n'est jamais
sérialisé : il est reconstruit à vide (`CacheSlot::neuve()`).

---

## 4. Patch EXACT — `rose/src/lib.rs`

Juste après la ligne `pub mod rir;` (ligne 52 dans la version post-G2), ajouter :

```rust
/// JVM Rose — bytecode `.rosebc` distribuable, VM RIR, GC (pool de `Gz`), sandbox.
pub mod rosevm;

/// Modèle de mémoire EXPLICITE « style Zig » — `Vue<T>` + `AllocateurRose<T>`.
pub mod mem_modele;
```

Rien d'autre. Aucune feature, aucune dépendance, aucun `cfg` supplémentaire : `rosevm`
utilise `std` (comme le reste du moteur) et `mem_modele` est `no_std`-compatible.

---

## 5. Patch EXACT — `rose/src/main.rs`

### 5.1 Déclarations de modules

Juste après `mod rir;` (ligne 152 dans la version post-G2) :

```rust
mod rosevm;      // JVM Rose : .rosebc, VM, GC, sandbox
mod mem_modele;  // allocateur arène / individuel (mémoire explicite)
mod cablage;     // handlers CLI (nouveau fichier src/cablage.rs, voir §5.5)
```

### 5.2 Aucun import à ajouter dans `main.rs`

⚠ **Piège vérifié** : coller les handlers **directement à la racine** de `main.rs` produit
des erreurs `E0255` (`use crate::front;` entre en conflit avec `mod front;`) et `E0428`
(`fn lire` dupliqué). Les handlers doivent donc vivre dans leur **propre module**
`cablage` — exactement comme dans le harnais. Ce module apporte ses propres `use` :
aucune modification du bloc `use` de `main.rs` n'est nécessaire.

### 5.3 Ligne d'aide

Dans `usage()`, juste après la ligne `rose comp prog.rose…` :

```rust
  rose bc prog.rose [-o prog.rosebc] [--fn nom]   bytecode .rosebc
  rose runbc prog.rosebc [args…]   exécution sandbox
  rose mem prog.rose [--fn nom] [args…]   arène
```

### 5.4 Branches du `match action`

Juste avant `"comp" => {` :

```rust
        "bc" => std::process::exit(cablage::cmd_bc(&argv)),
        "runbc" => std::process::exit(cablage::cmd_runbc(&argv)),
        "mem" => std::process::exit(cablage::cmd_mem(&argv)),
```

### 5.5 Corps des trois handlers — nouveau fichier `rose/src/cablage.rs`

Créer `rose/src/cablage.rs` en copiant **intégralement**
`/tmp/opencode/rose_vm_harnais/src/cablage.rs` (255 lignes, déjà testé tel quel). Aucune
adaptation n'est nécessaire : ses `use crate::front;`, `use crate::rosevm;`, etc. sont
résolus dans le module `cablage`, pas à la racine. Le fichier contient les fonctions
publiques :

- `choisir_fonction` / `corps_de_source` — sélection ``--fn``, sinon `principal`, sinon
  le premier corps ordonné ; abaisse en RIR ;
- `cmd_bc(argv) -> i32` — `rose bc …` : vérifie le sous-ensemble **avant** écriture,
  écrit le `.rosebc`, affiche `rose bc : <fn> → <sortie> (N octets, N op, N slot(s))` ;
- `argument(s) -> Option<Valeur>` — parse i64 rapide sinon entier exact (validation
  stricte : au moins un chiffre, signe optionnel — évite le panic de `chaine_vers_entier`) ;
- `cmd_runbc(argv) -> i32` — `rose runbc …` : `fs::read` → `lire_bytecode` (vérification
  hostile) → `HoteBc::neuf(max_gz)` → `executer_sandbox` → imprime `hote.sortie` et le
  résultat ; code 1 sur erreur propre ;
- `cmd_mem(argv) -> i32` — `rose mem …` : compile en mémoire, exécute en sandbox, puis
  range la copie scalaire du résultat dans `AllocateurRose::<u64>` en mode `Arena`,
  lit la `Vue`, fait `liberer_tout()` et affiche les compteurs.

> ⚠ Le handler `bc` appelle `verifier_sous_ensemble` : une fonction avec appel, index,
> lecture clavier ou AST est **refusée à la compilation** avec un message explicite,
> au lieu de produire un fichier inutilisable. C'est une amélioration par rapport au
> `ecrire_bytecode` brut (qui réussit toujours, laissant le vérificateur refuser au
> chargement).

---

## 6. Limites (à ne pas sur-vendre)

1. **Un seul corps par fichier** (format v1) : `bc` compile UNE fonction, pas un programme
   complet avec ses imports et son corps principal. `--fn` permet de la choisir.
2. **Sous-ensemble sandbox** : seulement `Affect`, `AfficherAbstrait`, `Saut`,
   `SautSiFauxAbstrait`, `RetourAbstrait`, `Retour(None)`. Aucun appel à une autre
   fonction Rose (`AppelRir`), aucune AST, aucun `Lire` (clavier), aucun fichier/réseau.
3. **`rosevm` n'est pas une JVM complète** : pas de GC générationnel (liste libre +
   comptage de prêts, limite ferme `max_gz`), pas de chargeur de classes, pas de JIT
   depuis `.rosebc`, pas de chargement dynamique. Voir `docs/ROSE_VM.md`.
4. **Arène = bulk-free, sans destructeur individuel** : `Valeur` possède un `Drop`
   (`Rc<Gz>`) et ne doit **jamais** être placée en mode `Arena`. `rose mem` n'y range
   qu'une copie `u64` du résultat entier/booléen (les flottants/chaînes donnent 0).
   Le mode `Individuel` (`liberer` → `Box::from_raw`) est le bon choix pour les valeurs
   qui survivent à leur corps.
5. **Budgets par défaut** : `Limites { budget_instructions: 10_000_000, max_slots:
   u16::MAX, max_gz: 64 }`. `max_slots` est borné par l'encodage u16 des slots.
6. **Bytecode non signé** : le chargement vérifie la structure, pas l'origine. La
   distribution d'un `.rosebc` n'offre aucune garantie d'authenticité.
7. **Pas de modification des fichiers du dépôt** : ce document est le livrable ; le
   câblage réel dans `lib.rs` / `main.rs` doit être appliqué par l'orchestrateur une
   fois G2_REEL terminé, puis re-vérifié par `cargo test` sur le dépôt.

---

## 7. Fichiers

- preuve / code : `/tmp/opencode/rose_vm_harnais/` (`src/lib.rs`, `src/cablage.rs`,
  `src/main.rs`, `tests/integration.rs`) ;
- vérification du patch sur une copie du vrai `main.rs` : `/tmp/opencode/rose_patch_check/` ;
- modules (inchangés) : `rose/src/rosevm.rs`, `rose/src/mem_modele.rs` ;
- ce document : `rose/docs/CABLAGE_VM.md`.
