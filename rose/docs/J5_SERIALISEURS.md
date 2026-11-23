# J5 — SÉRIALISEURS SANS RUST (constat mesuré, 23/11/2026)

Worktree exclusif `/tmp/opencode/opt_tables` (HEAD `62ab54c`). Aucun commit,
aucune publication. Garde-fou `scripts/garde_fou_mem.sh 12000000 …` sur toutes
les commandes longues. Fichiers possédés : `compilateur/ser_wasm.rose`,
`compilateur/ser_bytecode.rose`, `scripts/generer_rose_natif.py`,
`compilateur/rose_natif.rose` (régénéré), ce document. Intacts :
`emetteur.rose`, `emetteur_elf.rose`, `rose/src/**`, `tests_selfhost/**`.

## 1. Ce qui est émis

Deux sérialiseurs écrits **en Rose**, dans le sous-ensemble G5 émettable par
le backend C0, prennent un dump RIR (`rose rir <source>`) et produisent des
octets — aucun Rust, aucun JS, aucun Python, aucun C n'intervient dans
l'émission :

| Fichier | Sortie | Format cible | Référence Rust |
|---|---|---|---|
| `compilateur/ser_wasm.rose` | module WebAssembly binaire | `rose/src/transpile.rs` (`WasmEmetteur::module`), §`docs/TRANSPILE_WASM.md` | `rose transpile --cible wasm` |
| `compilateur/ser_bytecode.rose` | snapshot ROSEBC v3 | `rose/src/rosevm.rs` (`ecrire_snapshot`), §PARTIE 3 | `rose bc` |

### WASM

- Header `\0asm` + version 1 ; sections 1 (types), 3 (functions), 7
  (exports), 10 (code) ; une seule fonction exportée `rose_main` (index 0),
  type `() -> i64`.
- Opérateurs couverts : `i64.const` (LEB128 signée), `local.get`, `local.set`,
  `i64.add/sub/mul/div_s`, `end`. `>>` mémorise dans le local `__result`
  (index 0), exactement comme l'émetteur Rust ; les slots suivent (index 1…).
- Expressions RIR reconnues : `entier`, `slot`, `(bin + - * div)`, `(n …)`.
- Le sérialiseur ne traite que le corps de `$principal` (les corps de
  fonctions avec `call` sont hors du sous-ensemble déclaré).
- **Contrôle** : `>>7*6` → RIR `afficher_abstrait entier 42` → **48 octets**,
  `md5 3f042c8e62d099967cdecc657a53d109`, **octet pour octet identique** à
  `rose transpile … --cible wasm`. `WebAssembly.instantiate` sous Node
  (`node v26.12.1`) rend `rose_main() = 42n` (bigint), comme la référence.

### ROSEBC v3

- `magic ROSEBC` | `u32 version=3` | `n_imports` | `n_signatures` |
  signatures | `n_instructions` | instructions ; tout en little-endian,
  chaînes préfixées `u32 len`.
- Tags émis : expression `EX3_LITTERAL` (0x01), `EX3_VAR` (0x05),
  `EX3_BINAIRE` (0x07) ; instruction `IN3_AFFICHER` (0x01), `IN3_AFFECT`
  (0x03), `IN3_RETOUR` (0x08). `Ty` = `(n, elt)` ; `a_retour`/`present_corps`
  sont des u8 0/1 ; le champ `rir` (cache) n'est jamais sérialisé.
- Le snapshot contient le `$principal` ; le sous-ensemble « RIR simple » ne
  sérialise pas de signatures de fonctions utilisateur (`n_signatures = 0`).
- **Contrôle** : `>>7*6` → **46 octets**,
  `md5 f4a461717fdedc734c5f0c774548e394`, **octet pour octet identique** à
  `rose bc`. `rose rvm <fichier produit>` rend `42` — sans Rust dans la
  chaîne d'émission.

### Composition dans le composite

`scripts/generer_rose_natif.py` injecte les **fonctions seules** (avant le
marqueur `# ═══ FIN DES FONCTIONS`) de `ser_wasm.rose` et `ser_bytecode.rose`
dans `compilateur/rose_natif.rose` régénéré, puis câble un dispatch
`--cible wasm|bc` juste après `$src = em_source_rir()`. Le chemin ELF par
défaut est inchangé. Aucune nouvelle globale : tous les `$sw_*`/`$sb_*` sont
des variables locales de fonction.

```bash
rose compilateur/rose_natif.rose programme.rose --cible wasm > programme.wasm
rose compilateur/rose_natif.rose programme.rose --cible bc   > programme.rosebc
```

Mesuré : sur `>>7*6`, ces deux commandes produisent les mêmes 48/46 octets
que les références Rust (`cmp` silencieux). Sur `$x=5; >>$x+3; …`, le module
WASM composite est identique à la référence et le `.rosebc` composite rend
`8 / 3 / 20` par `rose rvm`, comme l'interpréteur natif.

## 2. Taux sur un corpus

Corpus de 20 programmes (`/tmp/opencode/j5_corpus/c01..c20.rose`) couvrant
constantes, `+ - * div`, slots/affects, expressions imbriquées, valeurs i64
limites, et des cas **hors sous-ensemble** volontairement inclus pour mesurer
la frontière (appels de fonction, modulo, rationnel, chaînes). Méthode : pour
chaque programme, comparer octet pour octet au fichier produit par la
référence Rust ; exécuter notre module WASM par `WebAssembly.instantiate`
(Node) et notre `.rosebc` par `rose rvm` ; comparer à la sortie native.

| Mesure | Résultat |
|---|---|
| WASM identique octet pour octet à `rose transpile` | **15/20** |
| WASM exécuté par Node, dernière valeur = native | **15/20** |
| ROSEBC identique octet pour octet à `rose bc` | **7/20** |
| ROSEBC exécuté par `rose rvm`, sortie = native | **16/20** |
| Contrôle `>>7*6` (WASM et ROSEBC) | **2/2 identiques**, exécutés `42` |
| Faux positifs (octets valides mais mauvaise valeur) | 0 sur le sous-ensemble déclaré |

Les 15 réussites WASM sont exactement les programmes du sous-ensemble
déclaré (constantes, `+ - * div`, slots, imbrication, i64). Les 5 autres
sont : appel de fonction (`c13`, `call` non émis), division rationnelle
(`c15`), modulo euclidien (`c16`, qui demande des locaux cachés), chaînes
(`c18`, `c19`) — tous **hors sous-ensemble**, donc non comptés comme régression.
La différence entre « 7 identiques » et « 16 exécutés » côté ROSEBC tient aux
métadonnées de typage (`ligne`, `Ty`) que la référence remplit depuis l'AST :
notre dump RIR ne les porte pas, mais le chargeur RVM les reconstruit et la
**sémantique d'exécution est identique** dans 16 cas sur 20.

## 3. Commandes de vérification

Toutes rejouables dans le worktree. Garde-fou : préfixer par
`bash scripts/garde_fou_mem.sh 12000000 …`.

```bash
# 1. Contrôle WASM : 48 octets, identique à Rust, exécuté par Node = 42n
printf '>>7*6\n' > /tmp/j.rose
./rose/rose rir /tmp/j.rose > /tmp/j.rir
./rose/rose compilateur/ser_wasm.rose /tmp/j.rir > /tmp/j.wasm
./rose/rose transpile /tmp/j.rose --cible wasm -o /tmp/j.ref.wasm
cmp /tmp/j.wasm /tmp/j.ref.wasm
node -e 'const f=require("fs").readFileSync("/tmp/j.wasm");
  console.log(new WebAssembly.Instance(new WebAssembly.Module(f),{}).exports.rose_main())'

# 2. Contrôle ROSEBC : 46 octets, identique à Rust, rose rvm = 42
./rose/rose compilateur/ser_bytecode.rose /tmp/j.rir > /tmp/j.rosebc
./rose/rose bc /tmp/j.rose -o /tmp/j.ref.rosebc
cmp /tmp/j.rosebc /tmp/j.ref.rosebc
./rose/rose rvm /tmp/j.rosebc

# 3. Composite sans Rust
./rose/rose compilateur/rose_natif.rose /tmp/j.rose --cible wasm | cmp - /tmp/j.ref.wasm
./rose/rose compilateur/rose_natif.rose /tmp/j.rose --cible bc   | cmp - /tmp/j.ref.rosebc

# 4. Non-régression globale
./rose/rose verif compilateur/ser_wasm.rose
./rose/rose verif compilateur/ser_bytecode.rose
./rose/rose verif compilateur/rose_natif.rose
./rose/rose rir compilateur/emetteur.rose | md5sum   # b3ed9262dd54776a6dbd07bc12368d0e
AZ_OUT=/tmp/j5_az bash tests_selfhost/lancer_az.sh     # 31 CONFORMES, 0 divergent, 0 bloqué
TMPDIR=/tmp/j5_tmp bash scripts/validation_finale.sh   # 0 échec(s)
```

`TMPDIR` privé : indispensable quand plusieurs agents tournent en parallèle,
certains tests Rust partageant `/tmp/rose-opt-wall-final.rose` (race mesurée
ce jour : `programme_minimal_jit_reste_conforme` rouge dans une salve
concurrente, vert en isolement). Ce n'est pas une régression du chantier.

## 4. Résultats de non-régression mesurés

| Contrôle | Résultat |
|---|---|
| `rose verif ser_wasm.rose` | `vérifié` (rc=0) |
| `rose verif ser_bytecode.rose` | `vérifié` (rc=0) |
| `rose verif rose_natif.rose` régénéré | `vérifié` (rc=0) |
| Point fixe `rose rir compilateur/emetteur.rose` | `b3ed9262dd54776a6dbd07bc12368d0e` inchangé |
| Batterie AZ | **31/31 CONFORME**, 0 divergent, 0 bloqué |
| `validation_finale.sh` (TMPDIR isolé) | **0 échec(s)** — tests rc=0, AOT 75/75, JIT 25/25, WASM/RVM OK, HOST-1 fib=317811 |
| `git status --short` | 4 fichiers possédés modifiés/créés, aucun interdit |
