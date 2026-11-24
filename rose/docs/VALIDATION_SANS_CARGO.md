# VALIDATION_SANS_CARGO — validation complète sans appeler cargo

**Constat daté : 24/11/2026, 10:46Z.** Worktree exclusif
`/tmp/opencode/fin_validation`, HEAD `0d0ebb3`, aucun commit, aucun push.
Aucun fichier `rose/src/**`, `compilateur/**` ni `Cargo.toml` modifié.
Tout chiffre ci-dessous est une sortie réellement exécutée, sous le garde-fou
`bash scripts/garde_fou_mem.sh 12000000 …`.

## 1. Commande

```sh
cd /tmp/opencode/fin_validation
bash scripts/garde_fou_mem.sh 12000000 \
  bash scripts/validation_finale.sh --sans-cargo
# → RC=0, journal /tmp/opencode/validation_finale_sans_cargo.log
```

Le comportement par défaut (`bash scripts/validation_finale.sh`) est
**inchangé** : seules cinq lignes de dispatch d'option ont été ajoutées en tête
du script ; la CI historique garde son `cargo build --release --bin rose` et
ses `cargo test --release`.

`scripts/validation_sans_cargo.sh` contient toute la mécanique du mode
`--sans-cargo`. Variables optionnelles :

| Variable | Rôle |
|---|---|
| `ROSE_REFERENCE` | binaire Rose **préconstruit** utilisé par les sections 2-7 (défaut : `$ROOT/rose/target/release/rose`, puis `$ROOT/rose/rose`, puis les mêmes chemins dans `/home/clausake/Stefan/ROSE`) |
| `VF_REUTILISER=1` | rejoue la preuve avec le moteur C0 déjà construit dans `/tmp/opencode/vf_sans_cargo` |
| `VF_SOURCE_C0` | source composée passée à `build-sans-cargo.sh` (défaut `compilateur/moteur_complet.rose`) |

## 2. Ce que §1 fait désormais

1. Un binaire `cargo` **piège** est placé en tête du PATH
   (`/tmp/opencode/vf_sans_cargo/bin/cargo`) : tout appel réel journalise la
   commande dans `cargo_appels.log` et sort avec rc=99. Le script exige un
   journal **vide** en fin de course (`OK aucun appel cargo (journal vide)`).
2. Le moteur C0 est construit par `scripts/build-sans-cargo.sh` avec la
   source `compilateur/moteur_complet.rose` (md5 `4c67d8446c0e837cb501eb1d0ad20cc8`),
   sous garde-fou mémoire, **sans cargo, rustc, clang ou LLVM** :
   - C0 `/tmp/opencode/C0_h.elf`, md5 `2db168b907b4a874712857d3957fd078` ;
   - ELF `/tmp/opencode/vf_sans_cargo/moteur.elf`, **2 415 644 octets**,
     md5 `0ea3dabf5cba132c5d5a6e21fc63672e` ;
   - batterie intégrée `12/12 rc=0`.
   La batterie intégrée reste active : `build-sans-cargo.sh` termine avec
   rc=1 si `--sans-tests` rend `total=0` (sa condition finale), donc le mode
   `--sans-cargo` ne passe **pas** `--sans-tests`.
3. Preuves d'exécution du moteur C0 lui-même (pas un stub) :

| Contrôle C0 | Sortie mesurée |
|---|---|
| `moteur.elf version` | `Rose 0.1.73 — langage de programmation en français (maths exactes, JIT, AOT, compilation autonome)` |
| `moteur.elf rir compilateur/emetteur.rose` | 263 520 o, md5 `b3ed9262dd54776a6dbd07bc12368d0e` |
| `moteur.elf /tmp/opencode/fib28.rose` | `317811` |
| `moteur.elf aot rose/tests_aot8/programmes/01_arith.rose -o … --target aarch64-unknown-linux-gnu` | ELF aarch64 exécuté → `14`, identique à l'interpréteur |
| `moteur.elf transpile tests_selfhost/t20_add.rose --cible wasm -o …` | module de **61 octets**, octet pour octet identique à l'oracle |

Les sections 2 à 7 reprennent **les mêmes commandes, les mêmes artefacts et
les mêmes seuils** que `validation_finale.sh`, avec `$B` = binaire
préconstruit. Aucun `cargo build`, aucun `cargo test` : le binaire est fourni
par la release (un toolchain Rust n'est pas nécessaire pour l'exécuter).
`ROSE` est exporté pour `rose/tests_aot8/verifier.sh` et un lien symbolique
temporaire `$ROOT/rose/rose` est posé pour `tests_selfhost/lancer_az.sh`
(chemin codé en dur), puis retiré par un `trap` : aucun fichier suivi du dépôt
n'est modifié.

## 3. Résultats complets (RC=0)

Journal : `/tmp/opencode/validation_finale_sans_cargo.log`.

Course **depuis zéro** (moteur C0 supprimé puis reconstruit, 20 min 33 s) :

```
[10:25:48] binaire de référence (préconstruit, aucun cargo) : /home/clausake/Stefan/ROSE/rose/target/release/rose
[10:25:48] === 1. moteur C0 + tests (SANS cargo) ===
[10:25:48] ARTEFACTS : /tmp/opencode/vf_sans_cargo/moteur.elf (2415644 o, md5 0ea3dabf5cba132c5d5a6e21fc63672e)
[10:25:48] TESTS : 12/12 rc=0
[10:41:40] OK   build C0 rc=0
[10:41:40] OK   C0 version: Rose 0.1.73 — langage de programmation en français (maths exactes, JIT, AOT, compilation autonome)
[10:42:25] OK   C0 rir emetteur b3ed9262dd54776a6dbd07bc12368d0e
[10:42:35] OK   C0 fib28=317811
[10:42:40] OK   C0 AOT natif=14
[10:42:45] OK   C0 WASM byte-identique (61 o)
[10:42:45] === 2. point fixe émetteur ===
[10:43:45] md5: b3ed9262dd54776a6dbd07bc12368d0e / b3ed9262dd54776a6dbd07bc12368d0e
[10:43:45] OK   point fixe
[10:43:45] === 3. AOT zéro refus ===
TOTAL: 75 OK, 0 ECHEC (75 contrôles attendus : 25 programmes × 3 cibles)
[10:43:45] OK   AOT 75/75
[10:43:45] OK   aucun refus AOT
[10:43:45] === 4. JIT 25/25 Cranelift ===
[10:43:47] 25/25
[10:43:47] OK   JIT
[10:43:47] === 5. WASM + RVM ===
[10:43:47] OK   WASM 92 o
[10:43:47] OK   RVM 317811
[10:43:47] === 6. batterie AZ (Unicode) ===
CONFORME 31
[10:46:03] OK   AZ 31 CONFORME, 0 non-conforme
[10:46:03] === 7. self-host HOST-1 ===
[10:46:16] OK   HOST-1 fib=317811
[10:46:16] OK   aucun appel cargo (journal vide)
[10:46:16] === RESULTAT: 0 échec(s) ===
```

Rejeu avec `VF_REUTILISER=1` (10:20:27 → 10:25:04) : **RC=0**, mêmes
chiffres, mêmes md5, 0 appel cargo.

| Famille | Seuil | Mesure |
|---|---|---|
| 1. moteur C0 | ELF exécutable + 12/12 | 2 415 644 o, md5 `0ea3dabf…`, 12/12 |
| 2. point fixe | `b3ed9262dd54776a6dbd07bc12368d0e` ×2 | identique ×2 |
| 3. AOT | 75 OK, 0 ECHEC | **75/75** |
| 4. JIT | 25 programmes avec `principal=` | **25/25** |
| 5. WASM / RVM | taille > 0 ; `317811` | **92 o** ; **317811** |
| 6. AZ | ≥ 31 conformes, 0 non-conforme | **31 conformes, 0** |
| 7. HOST-1 | magic ELF + `317811` | `7f454c4602010100` + **317811** |
| 8. garde-fou cargo | journal vide | **0 appel** |

Preuve complémentaire (aucun appel cargo dans le code du mode) :

```sh
grep -n 'cargo' scripts/validation_sans_cargo.sh
# uniquement le commentaire d'en-tête, la création du piège et le contrôle final
wc -l /tmp/opencode/vf_sans_cargo/cargo_appels.log   # 0
```

## 4. Pourquoi les sections 2-7 utilisent un binaire préconstruit

Le composé C0 au HEAD `0d0ebb3` couvre l'exécution, `rir`, `verif`,
`tokens`, `ast`, `latex`, `run`/`jit`/`interprete`/`methode`, `comp`, `aot`,
`transpile --cible wasm`, `wasm`, `bc`, `version`, `help`, `info` (zéro
`rc=3` dans le lanceur). Quatre comportements exigés par les sections 2-7
ne sont **pas** dans ce composé et ont été mesurés comme tels :

1. le JIT n'émet pas la ligne `principal=` quand `ROSE_TIMING=1` ;
2. `aot` ignore `--target` et émet toujours un ELF aarch64 (donc ni cible
   x86-64+qemu, ni PE64, soit 25 contrôles sur 75) ;
3. `sw_module`/`sb_module` ne sérialisent que le corps `$principal` : sur
   `fib28.rose` ils produisent 48 o de WASM et 66 o de ROSEBC au lieu de
   92 o et 292 o, et la commande `rvm` a été retirée du point d'entrée lors
   d'une régénération ultérieure du composé ;
4. la compilation réflexive de `emetteur.rose` par le moteur interprété
   dépasse la limite du garde-fou (13 892 076 Ko lus avant l'arrêt à 12 000 000 Ko),
   donc le point fixe complet, la batterie AZ et HOST-1 ne peuvent pas tourner
   uniquement sur l'interpréteur C0 sous ce garde-fou.

Le mode `--sans-cargo` ne modifie aucun de ces points : il supprime la
**dépendance cargo** de la validation (premier objectif, et seule raison pour
laquelle une machine sans toolchain Rust ne pouvait pas la lancer), construit
et prouve le moteur C0 en §1, puis rejoue les sept familles à l'identique
avec le binaire de release. Toutes les valeurs du tableau ci-dessus sont des
sorties mesurées, pas des estimations.
