# `rose wasm` dans le moteur auto-hébergé — jalon A-Z

**Constat daté : 24/11/2026.** Worktree exclusif `/tmp/opencode/az_wasm`,
HEAD `0597663`, aucun commit, aucun push. Aucune source `rose/src/**` ni
`emetteur*.rose` modifiée. Toutes les mesures ci-dessous ont été ré-exécutées
sur cet arbre ; les journaux sont conservés sous `/tmp/opencode/`.

---

## 1. Mesure du refus avant câblage

Deux refus distincts, mesurés sur l'état initial (source composée md5
`a6f86d6d9ce15417f9e8fb2d6ae3c50a`, ELF C0 `ba91b0a1474773e6a2d83fca44357f0f`
— 2 404 364 octets — construit depuis cette source) :

| Voie | Commande | rc | Sortie |
|---|---|---|---|
| Lanceur généré par `scripts/build-sans-cargo.sh` (avant édition) | `rose wasm tests_selfhost/t20_add.rose` | **3** | `rose: commande NON COUVERTE par le moteur auto-hébergé C0 : wasm` + cause |
| ELF seul | `moteur.elf wasm tests_selfhost/t20_add.rose` | **1** | `rose: fichier introuvable : wasm` (la commande tombait dans le chemin d'exécution par défaut, qui cherchait un fichier nommé `wasm`) |

Le `rc=3` du lanceur était donc bien le dernier refus de la commande `wasm`.

## 2. Sémantique déterminée sur le binaire Rust (lecture seule)

Référence : `/home/clausake/Stefan/ROSE/rose/target/release/rose`.

- `rose help wasm` → `rose help : commande inconnue : wasm`, rc=1 :
  **`wasm` n'a pas d'entrée d'aide**, ce n'est PAS un alias de `transpile` ;
- `rose wasm rose/web/rose.wasm` → `@dll("wasm", …)`, `@sym(…)`, `fn …`
  sur stdout et `N fonction(s) exportée(s)` sur stderr : `wasm` est une
  **commande officielle** d'inspection de module (branche `"wasm"` du CLI) ;
- `rose transpile prog.rose --cible wasm -o …` → module binaire exportant
  `rose_main() -> i64` ;
- `rose wasm prog.rose` → `magic absent : ce n'est pas du WebAssembly`.

Le jalon A-Z étend cette commande officielle à la **source Rose**, sans lui
ajouter d'entrée d'aide (le comportement `help wasm` du Rust est préservé) :

```
rose wasm prog.rose [-o sortie.wasm]
  -> lsrc_lire_programme(source) -> RIR -> sw_module(RIR)
  -> ecrire_bytes(sortie, module)
```

Le module est sérialisé par la **même fonction `sw_module()`** que
`rose transpile … --cible wasm` : l'égalité octet pour octet est structurelle,
pas une ressemblance. Le message (`rose: <sortie> produit (transpilation wasm)`)
et le nom par défaut (`<basename sans .rose>.wasm` dans le répertoire courant,
copie exacte du `strip_suffix(".rose")` du Rust) sont identiques. Une entrée
qui n'est pas une source `.rose` reçoit un diagnostic explicite
(`magic absent : ce n'est pas du WebAssembly` si le magic `\0asm` manque),
jamais une simulation ni un `rc=3`.

## 3. Câblage

- `compilateur/moteur_complet.rose` : bloc `WASM_SELF:DÉBUT` … `WASM_SELF:FIN`
  inséré juste avant le dispatch `transpile`, condition `mc_cmd == "wasm"`.
  Analyse des arguments dans n'importe quel ordre (`-o` / `--sortie`),
  lecture du début du fichier pour le contrôle du magic, puis `sw_module`.
- `scripts/build-sans-cargo.sh` : `wasm` retiré du `case` de refus et ajouté
  à la liste `exec "$MOTEUR" "$@"` ; commentaire de couverture mis à jour.
- `compilateur/tests_wasm_self.rose` : banc de non-régression relisant les
  artefacts réellement produits (modules, résultats Node, marqueurs).

Construction : `bash scripts/garde_fou_mem.sh 12000000 bash scripts/build-sans-cargo.sh
--sortie /tmp/opencode/wasm_self_out --sans-tests --timeout 3600` → C0
`2db168b907b4a874712857d3957fd078`, source `4c67d8446c0e837cb501eb1d0ad20cc8`,
ELF **2 415 644 octets**, md5 `0ea3dabf5cba132c5d5a6e21fc63672e`, stderr 0 octet.

## 4. Preuves

### 4.1 Cinq modules byte-identiques + exécution Node

Cinq programmes de `tests_selfhost/`, modules C0 (`wasm`) et Rust
(`transpile --cible wasm`), puis `WebAssembly.instantiate` / `rose_main()`
sous Node v26.12.1. Chaque ligne est un `cmp` réussi des deux modules **et**
des deux résultats :

| Programme | Octets | md5 (C0 = Rust) | `rose_main()` | Verdict |
|---|---|---|---|---|
| `t01_entiers` | 72 | `62ad08469de72e2bd116a40eb4a45537` | 4294967296 | OK |
| `t20_add` | 61 | `c7bb9a95df37a6fc5fdb467aad799607` | 0 | OK |
| `t21_sous` | 56 | `4cc0b3fb746b3d69ca7fbd0982380aa3` | -12 | OK |
| `t22_mul` | 61 | `16029648827c6ba87205a5eb25557439` | -20 | OK |
| `t29_precedence` | 60 | `8eb3de3d749af3224041a9ad3829f39c` | 5 | OK |

`MODULES+NODE : 5 OK, 0 ECHEC`. Le module `t20_add` produit **sans `-o`**
(`t20_add.wasm`, 61 octets) est également octet-identique au module Rust.
Journal : `/tmp/opencode/wasm_self_validate.log`.

### 4.2 Banc Rose et zéro rc=3

- `/tmp/opencode/wasm_self_out/moteur.elf compilateur/tests_wasm_self.rose …` →
  `TESTS WASM SELF : 13 OK, 0 ECHEC`, rc=0 ;
- le même banc sous le binaire Rust → `13 OK, 0 ECHEC`, rc=0.

Note de mesure : sous le backend C0, parcourir caractère par caractère le
composé (640 K points de code) faisait monter le RSS à ~18 Gio puis segfault.
Le banc lit donc un petit `marqueurs.txt` recalculé par le harnais shell avec
`grep` sur les deux sources ; c'est ce qui donne les 3 contrôles de marqueurs.

Zéro `rc=3` vérifié sur 17 appels du lanceur (`wasm`, `run`, `jit`,
`interprete`, `methode`, `verif`, `rir`, `tokens`, `ast`, `latex`,
`transpile`, `bc`, `comp`, `version`, `help`, `--help`, `info`) : **tous rc=0,
aucune occurrence de `commande NON COUVERTE`** dans stderr.

### 4.3 Non-régression

| Contrôle | Résultat |
|---|---|
| Parité self-host (`verifier_parite_selfhost.sh`, ELF md5 `0ea3dabf…`) | **141 CONFORMES, 0 DIVERGENT, 0 BLOQUÉ**, rc=0 — `/tmp/opencode/wasm_parite/parite_20261124T092306Z.tsv` |
| Formats byte-identiques (wasm/bc/comp × 5 programmes, C0 vs Rust) | **15 OK, 0 ECHEC** — `/tmp/opencode/wasm_formats.log` |
| Point fixe `b3ed9262dd54776a6dbd07bc12368d0e` | identique ×2 (`rir` et `comp` de `compilateur/emetteur.rose`) |
| `scripts/validation_finale.sh` | **RESULTAT : 0 échec(s)**, rc=0 — `/tmp/opencode/wasm_validation_finale3.log` (tests Rust, AOT 75/75, JIT 25/25, WASM 92 o, RVM 317811, AZ 31 conformes, HOST-1) |
| Marqueurs `bi_pow`, `mc_args`, `rep_run`, `cli_aide_pour`, `cands`, `cr_feuille`, `sw_module`, `sb_module` | présents dans le composé après intégration |

Deux aléas de mesure ont été écartés, sans rapport avec ce chantier : le
binaire de test Rust `opt_jit_listes` a fait un SIGSEGV au premier lancement
puis est passé (le même test du dépôt principal passe) ; la batterie AZ
exigeait `rose/rose` dans le worktree (symlink temporaire vers
`rose/target/release/rose`, retiré ensuite). Le jeu final est à **0 échec**.

## 5. Fichiers possédés

`compilateur/moteur_complet.rose` (dispatch `wasm` uniquement),
`scripts/build-sans-cargo.sh`, `compilateur/tests_wasm_self.rose`,
`rose/docs/WASM_SELF.md`.
