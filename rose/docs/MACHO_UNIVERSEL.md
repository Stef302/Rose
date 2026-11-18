# Mach-O, ELF, PE… et le format universel (Rose 0.1.5)

> **Chantier M3.2-bis** — écrivain Mach-O 64-bit (`rose/src/macho_writer.rs`),
> premier jet. Aucune dépendance ajoutée (GMP jamais dépendance, aucun crate
> nouveau). Ce document est un **constat daté (18/11/2026)**, pas une consigne.

---

## 1. Ce qui est prouvé, et ce qui ne l'est pas

| Preuve | Sortie réelle obtenue sous Linux |
|---|---|
| `cargo test` (crate isolée `/tmp/opencode/macho_test`, `#[path]`) | `4 passed; 0 failed` |
| `file rose_m3_2_arm64.macho` | `Mach-O 64-bit arm64 executable, flags:<NOUNDEFS>` |
| `llvm-objdump -h` | `file format mach-o arm64` + section `__text`, taille `0x0c`, VMA `0x1000000d0` |
| `llvm-objdump -d` | `mov x0, #0x31 ; mov x8, #0x5d ; svc #0` |
| assertions octet par octet | magic `CF FA ED FE`, cputype `0x0100000C`, subtype `0`, `MH_EXECUTE=2`, `ncmds=2`, `sizeofcmds=176`, `LC_SEGMENT_64` + `section_64 __text`, `LC_MAIN entryoff=0xD0`, fichier 220 octets |
| fichier issu du **vrai** `natif_emetteur::emettre` (rlib du dépôt) | 260 octets, `file` et `llvm-objdump` identiques ; le `mov x0,#49` (`20 06 80 D2`) provient bien de l'émetteur |

**Non prouvé, non revendiqué :** aucun binaire n'a été exécuté sur macOS. Nous
sommes sous Linux (hôte aarch64). Le writer ne fait pas de signature de code,
n'émet ni `LC_SYMTAB`, ni `LC_BUILD_VERSION`, ni `LC_LOAD_DYLINKER`, ni le
runtime Rose embarqué. Comme le PE de `link_natif.rs`, la structure est
vérifiable ici ; le comportement à l'exécution reste à valider sur la cible.

⚠ **Le stub de test n'est pas un stub macOS valide fonctionnellement.** Il
utilise la convention Linux AAPCS64 (`x0=49`, `x8=93`, `svc #0`) fournie par la
consigne, qui est aussi celle que `link_natif.rs` emploie pour l'ELF. Sur
macOS arm64, la sortie de processus exige `x16 = 0x2000001` (BSD `exit`)
suivi de `svc #0x80`. Le stub prouve donc la **structure** du fichier ; il ne
ferait pas `exit(49)` sur macOS. Le câblage final devra utiliser la
convention Mach (autre différence, avec les en-têtes et l'ABI, entre ELF et
Mach-O).

---

## 2. Pourquoi trois formats natifs incompatibles

Un exécutable natif n'est pas seulement du code machine : c'est un **contrat
avec le chargeur du système d'exploitation**. Ce contrat porte sur :

- l'en-tête et la table de commandes de chargement (où placer les segments) ;
- les conventions d'appel et la disposition de la pile (ABI) ;
- la numérotation et la convention des appels système (`svc` sur AAPCS64,
  `syscall` sur SysV AMD64, `sysenter`/IAT sur Windows) ;
- le modèle de sécurité (signature de code obligatoire sur macOS/iOS,
  Authenticode sur Windows, absence équivalente sur Linux) ;
- l'éditeur de liens dynamique et le format des bibliothèques partagées.

Trois chargeurs, trois contrats :

| Format | Origine | En-tête | Point d'entrée | Section de code | Sortie de processus |
|---|---|---|---|---|---|
| **ELF64** | standard ouvert (gABI), décliné par OS | `7F ELF` + `e_entry` | `e_entry` (adresse virtuelle) | `.text` | syscall `exit` (60 / 93) |
| **PE64** | Microsoft, spécification contrôlée par l'éditeur | `MZ` + `PE\0\0` | `AddressOfEntryPoint` (RVA) | `.text` | `kernel32!ExitProcess` via IAT |
| **Mach-O 64** | Apple, format propriétaire | `CF FA ED FE` (`MH_MAGIC_64`) | `LC_MAIN.entryoff` (décalage fichier) | `__TEXT,__text` | syscall `exit` : `x16=0x2000001`, `svc #0x80` (macOS) |

Précision honnête : ELF est une **spécification ouverte**, pas un format
propriétaire au sens strict — mais chaque système ajoute sa propre ABI, son
interpréteur (`/lib64/ld-linux…`) et ses appels système, si bien qu'un ELF
Linux ne tourne pas sur un BSD ou un Android. PE et Mach-O, eux, sont
contrôlés par Microsoft et Apple. Dans les trois cas, **le format est lié à
l'OS** : il n'existe aucun exécutable natif qui soit chargé à la fois par
Linux, Windows et macOS.

Le piège classique est le « **Universal binary** » de Apple (un Mach-O *fat*
contenant plusieurs tranches, arm64 + x86-64) : il résout le problème de
l'**architecture**, pas celui du **système**. C'est toujours du Mach-O, donc
toujours macOS-only. De même, un `.exe` anycpu ou un ELF multiarch ne change
pas de chargeur. **Il n'existe donc pas de format natif universel.**

---

## 3. Pourquoi WebAssembly est le seul vrai format universel

WebAssembly (W3C, spécification formelle, version 1.0 puis 2.0) est un
**bytecode de machine virtuelle défini indépendamment de tout OS et de tout
processeur** :

- le même fichier `.wasm` s'exécute dans Chrome, Firefox, Safari, Node.js,
  wasmtime, wasmer, WAMR, WasmEdge… sans recompilation ;
- la sémantique est définie par la spécification (i32/i64/f32/f64, mémoire
  linéaire, imports/exports), pas par le noyau d'un système ;
- il est déjà déployé partout : navigateurs, serveurs, edge, mobile, et
  sandbox (le modèle de capacités par imports remplace l'accès système brut) ;
- il sert de **cible de compilation commune** : Rust, C/C++ (Emscripten/clang),
  Go, AssemblyScript, Zig, et désormais Rose le consomment.

Les autres candidats ne tiennent pas la comparaison : le bytecode JVM et le
CIL .NET sont portables mais exigent un runtime lourd et ne sont pas
universellement disponibles (en particulier pas dans le navigateur) ; le LLVM
IR est portable mais n'est pas un **format d'exécution** livrable à un
utilisateur final ; un « fat binary » ne règle que l'architecture (§2).

### 3.1 Comment Rose l'utilise déjà

Rose exploite le wasm dans **les deux directions** :

1. **Rose → wasm (cible de compilation).** `rose/src/wasm.rs` expose une
   interface C ABI sans dépendance externe (ni `wasm-bindgen`, ni
   `wasm-pack`) : `rose_alloc` / `rose_free` / `rose_execute` / `rose_version`.
   Le glue `web/rose-wasm.js` appelle directement le module compilé pour
   `wasm32-unknown-unknown` (`[lib] crate-type = ["cdylib", "rlib"]`,
   profil `wasm-release`). Le binaire natif n'est pas modifié. Voir
   `rose/docs/WASM.md`.

2. **wasm → Rose (pont d'appel, M6.1).** `rose/src/wasm_lecteur.rs`
   (1 391 lignes, zéro dépendance) lit un module wasm et l'exécute avec un
   interpréteur de pile maison. Déclaration `@dll("wasm", "lib.wasm")` puis
   `@sym(...)`, ABI C (`entier`→i32, `flottant`→f64, `chaine`→pointeur),
   `rose wasm lib.wasm` liste les exports. Rôle de **compatibilité**, pas de
   vitesse : `fib(27)` wasm ≈ 150 ms (référence `PLAN.md`) / médiane 172,1 ms
   mesurée le 17/11 (`PORTE_WASM.md`) ; `10000 pgcd` ≈ 2,5 ms. Voir
   `rose/docs/WASM_FFI.md` et `rose/docs/PORTE_WASM.md`.

3. **runtime officiel optionnel (M6.2).** L'interpréteur maison reste le
   défaut. Pour les modules chauds, un runtime `wasmtime` / `wasmer` est un
   **exécutable distinct** dans `~/.cache/rose/runtimes/`, téléchargé
   explicitement et vérifié par SHA-256 ; `Cargo.toml` n'est pas modifié et
   `executer_export()` ne télécharge jamais pendant une requête (repli
   explicite). Voir `rose/docs/WASM_RUNTIME.md`.

   ⚠ **Chiffres à re-mesurer.** `PLAN.md` rapporte la porte « ≤ 20 ms PASS,
   4,15 ms vs 172,1 ms » et `WASM_RUNTIME.md` « 2,885 ms (18/11, CLI réelle) »,
   tandis que `PORTE_WASM.md` (17/11) l'annonce « NON MESURÉE » et que
   l'artefact runner n'était pas publié. Ce sont des **constats datés et
   divergents** ; le présent chantier ne les a pas re-mesurés et ne les
   revendique pas.

---

## 4. Comparaison honnête

| Critère | Natif (ELF / PE / Mach-O) | WebAssembly |
|---|---|---|
| Universalité | ❌ un writer par OS | ✅ un seul bytecode, tous OS |
| Vitesse pure | ✅✅ code machine direct (fib(25) natif ~1,57 ms) | ⚠ interprété ~150–172 ms ; ✅ runtime JIT ~3–4 ms (chiffres datés) |
| Démarrage | ✅ immédiat | ⚠ instantiation + compilation JIT |
| Dépendances | ✅ zéro (`link_natif.rs`, `macho_writer.rs`) | ⚠ runtime externe pour la vitesse (non embarqué) |
| Accès système | ✅ direct (syscalls / IAT) | ⚠ imports explicites (sandbox) |
| Signature / distribution | ⚠ code signing macOS/Windows | ✅ module vérifiable, empreinte SHA-256 |
| Débogage outillage | ✅ `file`, `readelf`, `llvm-objdump` | ✅ outils wasm standard |
| Cible « un fichier partout » | ❌ | ✅ |

**La vraie phrase :** le natif est rapide mais **lié à l'OS** ; le wasm est
universel mais **interprété (lent) ou adossé à un runtime externe**. Aucun des
deux ne domine l'autre : ce sont deux points différents du compromis
portabilité ↔ performance.

---

## 5. Recommandation — garder les 3 writers + wasm

1. **Conserver les trois writers natifs.** Ils sont petits, sans dépendance et
   déjà vérifiés structurellement (`link_natif.rs` : ELF exécutable rend 49 ;
   PE64 table d'imports ; `macho_writer.rs` : structure Mach-O prouvée). Les
   supprimer reviendrait à abandonner la cible la plus rapide sur un OS connu.
2. **Garder le wasm comme cible universelle.** C'est le seul artefact qu'on
   puisse distribuer une seule fois pour navigateur + serveur + desktop, et le
   seul pont d'interopération avec les bibliothèques d'autres langages (M6).
3. **Ne pas chercher de « format natif universel »** : il n'existe pas (§2).
   L'Universal binary Apple ne règle que l'architecture.
4. **Ne pas fusionner les writers prématurément.** Leur point commun est
   l'entrée (octets machine de `natif_emetteur`) ; leur divergence (en-têtes,
   ABI, sortie de processus) est précisément ce qui doit rester explicite.

### Guide d'exploitation

| Besoin | Voie recommandée |
|---|---|
| Perf max, OS cible connu (Linux) | `elf_bytes` / `ecrire_elf` (rend 49, prouvé exécutable) |
| Perf max, OS cible connu (Windows) | `pe_bytes` / `ecrire_pe` (structure prouvée, exécution Windows à valider sur place) |
| Perf max, OS cible connu (macOS arm64) | `macho_bytes` / `ecrire_macho` (structure prouvée, exécution macOS à valider sur place) |
| Distribution multi-OS, web, sandbox | compiler en `.wasm` (`wasm.rs`) |
| Appeler une lib externe depuis Rose | pont M6.1 (`wasm_lecteur.rs`), runtime M6.2 si chaud |

Reproduire la preuve Mach-O (sous Linux, sans SDK Apple) :

```sh
# 1) tests isolés (aucun fichier du dépôt modifié)
cd /tmp/opencode/macho_test && PATH="$HOME/.cargo/bin:$PATH" cargo test

# 2) structure
file /tmp/opencode/rose_m3_2_arm64.macho
llvm-objdump -h /tmp/opencode/rose_m3_2_arm64.macho
llvm-objdump -d /tmp/opencode/rose_m3_2_arm64.macho
```

### Limites connues du premier jet Mach-O (à traiter ensuite)

- l'entrée doit être un **stub d'amorçage complet** ; le code de
  `natif_emetteur::emettre` est une fonction `i64 f(base, tramp)` : il faut
  encore câbler « stub + code émis + trampoline », exactement comme le fait
  déjà `link_natif.rs` pour ELF/PE ;
- pas de `LC_SYMTAB` / `LC_BUILD_VERSION` / `LC_LOAD_DYLINKER` ;
- le stub de démonstration utilise la convention syscall Linux (`x8=93`,
  `svc #0`) ; un stub macOS réel doit employer `x16=0x2000001`, `svc #0x80` ;
- pas de signature de code (obligatoire pour distribution macOS réelle) ;
- x86-64 est reconnu (`CPU_TYPE_X86_64`) mais seul arm64 a été vérifié avec
  les outils ;
- `llvm-objdump -x` échoue sur ce Mach-O (« Invalid/Unsupported object file
  format ») : limitation de LLVM 18, pas du fichier — `-h` et `-d` fonctionnent.
