# W1 — réparation du build WebAssembly (constat daté)

Mesures exécutées le **2026-11-20T21:01+01:00**. Aucun commit, aucune publication.
Le mission annonçait HEAD `df41f5d` ; HEAD observé au moment des mesures :
`a361a233d6e3047c00d20e5eca50906a2b3dc174` (dépôt partagé, commits concurrents).

## 1. Cause

`cargo build --release --target wasm32-unknown-unknown --lib` échouait avec
**6 erreurs** (4 racines, 2 cascades) :

| Erreur | Lieu | Détail |
|---|---|---|
| E0433 | `src/interp.rs:1248` | `crate::repl::enregistrer_objet_val1(o.clone())` dans `enregistrer_symbolique_affiche` |
| E0433 | `src/interp.rs:1420` | `crate::repl::enregistrer_objet_val1(o.clone())` dans `plus_de_chiffres_int1` |
| E0433 | `src/interp.rs:5279` | `crate::repl::format_table_detail(t)` dans `afficher_detail` |
| E0277 ×2 | `src/interp.rs:5279` | cascades de l'E0433 (type de retour non résolu) |
| E0425 | `src/interp.rs:7389` | `crate::jit::prepare_jit(...)` absent du shim `jit` WASM de `lib.rs` |

Cause des E0433 : `repl` est déclaré `#[cfg(not(target_arch = "wasm32"))] pub mod repl;`
dans `src/lib.rs:92-93`, mais INT1 a ajouté les appels ci-dessus dans `interp.rs`
sans garde. Cause de l'E0425 : `prepare_jit` a été ajouté à `jit.rs` natif après
l'écriture du shim `jit` WASM de `lib.rs`, sans mise à jour de ce shim.

## 2. Correctif retenu : option (b), shim WASM dans `lib.rs`

**Aucune modification des appels dans `interp.rs`.** Tout est dans `lib.rs`
(fichier autorisé), avec un `#[cfg(target_arch = "wasm32")]` :

1. `pub mod repl` minimal :
   - `enregistrer_objet_val1(Rc<ObjetParesseux>)` = **no-op** (pas de console
     interactive en WASM, donc `:more` n'a pas de sens) ;
   - `format_table_detail(&Table) -> Vec<String>` = **copie fidèle** du gabarit
     natif (`deux espaces`, libellé padé à 28, séparateur ` : `) pour que
     `afficher_detail(detail(x))` rende la même chose dans le navigateur.
2. `jit::prepare_jit` ajouté au shim `jit` WASM : même signature que le natif,
   retourne `(false, HashMap::new())` (rien n'est jamais compilable en WASM).

Pourquoi l'option (b) plutôt que (a) : un seul point de correction, zéro
`#[cfg]` dispersé dans `interp.rs`, zéro modification du REPL natif ni de ses
tests `:more` (déjà déterministes avec un `Mutex`). Le module natif `repl.rs`
reste inchangé. `jit.rs` (interdit) n'a pas été touché.

## 3. Build WASM — commande réelle et sortie

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cd /home/clausake/Stefan/ROSE/rose
CARGO_TARGET_DIR=/tmp/rose-wasm-target cargo build --release --target wasm32-unknown-unknown --lib
```

Résultat : **rc=0**. Fin de sortie réelle :

```
warning: `rose` (lib) generated 93 warnings (run `cargo fix --lib -p rose` to apply 44 suggestions)
    Finished `release` profile [optimized] target(s) in 7.68s
```

Cible isolée : `/tmp/rose-wasm-target` (aucune collision avec les builds des
autres agents). 93 warnings préexistants (unused vars/imports/doc comments),
0 erreur.

## 4. Preuve réelle sur le `.wasm` produit

Fichier : `/tmp/rose-wasm-target/wasm32-unknown-unknown/release/rose.wasm`

- `file` : `WebAssembly (wasm) binary module version 0x1 (MVP)`
- taille : **967 750 octets**
- sha256 : `140566e461398b6f7ca00aa1df84cfc84e9e4837eba3a587095326860ee0aafe`
- md5 : `bc1abb952466c8f9f480646020ea9144`
- `wasm-tools` et `wasm-objdump` ne sont PAS installés sur cette machine ;
  la validation a donc été faite avec **`file` + Node.js `WebAssembly`** :

```
node -e "…"  →  bytes=967750 magic=0061736d version=1
                validate=true
                module_OK imports=0 exports=24
```

`wasm-tools validate` n'a donc pas pu être appelé (outil absent), mais le module
est **réellement validé** par le moteur V8 et **instancié** avec succès
(`new WebAssembly.Module(b)` + `new WebAssembly.Instance(m, {})` : OK,
23 fonctions exportées + mémoire, 0 import).

### Test fonctionnel via l'ABI (`src/wasm.rs`)

Script : `/tmp/opencode/w1_wasm_smoke.js` (hors dépôt). Sortie réelle :

```
validate   = true
version    = "Rose 0.1.7 — compilateur Rust + JIT Cranelift (sémantique exacte)"
fib(25)    = "75025\n"
SMOKE_OK
```

Les deux points d'appel REPL shimés ont aussi été exercés dans le module WASM :

- `plus_de_chiffres(entier_geant_factorielle(100), 5)` →
  `93326215443944152681699238856266700…` (aucun piège, no-op OK) ;
- `/tmp/opencode/demo_geant.rose` (`afficher_detail(detail($x))`) → les 5
  propriétés avec le gabarit exact, p. ex.
  `  Premiers chiffres            : 901820028144300502340101362711…`

## 5. Non-régression native (constats datés)

- `CARGO_TARGET_DIR=/tmp/rose-native-target cargo test --release` → **rc=0**,
  `258 + 242 + 6 + 8 + 1 = **515 passed, 0 failed**, 6 ignored` (les suites
  `0 tests` s'ajoutent sans changement du total). La suite complète, y compris
  les tests `:more` et `detail`, passe.
- `./rose/rose jardins/tests/premiers.rose` → `── jardin/premiers : 20 tests, 0 échec ──`
- fib(25) → `75025`
- Point fixe ×2 :
  - `./rose/rose rir compilateur/emetteur.rose` → md5 `b3ed9262dd54776a6dbd07bc12368d0e`
  - `./rose/rose compilateur/emetteur.rose compilateur/emetteur.rose` → md5 `b3ed9262dd54776a6dbd07bc12368d0e`
- Banc 9 domaines × 9 tirages × c6/c16 (`/tmp/opencode/w1_multi_valeurs.py`,
  harnais historique `/tmp/opencode/launch2_agents/multi_valeurs.py` avec
  `FIN` reposé sur `rose/rose`, référence `/tmp/opencode/speed1_agents/rose_base`)
  → `c16: 81/81`, `c6: 81/81`, `TOTAL: 162/162`, **rc=0**
  (2026-11-20T19:55:40Z → 19:58:43Z).

Les portes M2 (jardin/fib/point fixe/banc) ont été rejouées avec le binaire
existant `rose/rose` (sha256 `2143e996eeff7f9fd1f4014c846a579323fb265e076f9b894be689f1293aad9c`) :
le correctif est exclusivement sous `#[cfg(target_arch = "wasm32")]`, donc il
ne peut pas modifier le natif ; `cargo test --release`, lui, compile bien
depuis la source courante.

## 6. Reste / limites

- Aucune erreur restante dans le build WASM ; aucune erreur n'a exigé un
  fichier interdit (`run.rs`, `i128x.rs`, `jit.rs`, `front.rs`, `setup_*`,
  `Cargo.toml`, `PLAN.md`, docs d'autres agents n'ont pas été modifiés).
- `:more` est volontairement inactif dans le module WASM (pas de REPL
  interactif) ; le REPL natif et son `Mutex` restent intacts et testés.
- `wasm-tools validate` / `wasm-objdump` absents : preuve de validation fournie
  par `file` + V8 (`WebAssembly.validate`/`instantiate`) + appel ABI réel.
- L'arbre de travail contenait, avant et pendant cette mission, des
  modifications concurrentes d'autres agents (`compilateur/emetteur_elf.rose`,
  `rose/src/setup_core.rs`, `setup_gui.rs`, `velopack.rs`, `rose/docs/G5_AZ.md`).
  Les seules modifications de cette mission sont `rose/src/lib.rs` et le
  présent document.
