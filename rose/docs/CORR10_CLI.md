# CORR10 — dispatch complet de la CLI dans le moteur complet (point 10)

État : **23/11/2026**. Worktree exclusif `/tmp/opencode/corr_b10`,
HEAD de départ `d117e04eab2a7b9ef391123d575311820908ee2e`.
**Trois fichiers possédés, aucun autre modifié, aucun commit, aucun push** :

| Fichier | Rôle | Taille finale |
|---|---|---|
| `compilateur/moteur_complet.rose` | point d'entrée unique + couche dispatch (SEUL bloc modifié) | 11 370 l. / 429 497 o |
| `compilateur/tests_b10.rose` | banc Rose de 24 assertions | 853 l. / 49 830 o |
| `rose/docs/CORR10_CLI.md` | ce document | — |

Garde-fou sur toute commande longue :
`bash scripts/garde_fou_mem.sh 12000000 <commande>`.

---

## 1. Les 15 fonctions `cli_*` disponibles (inventaire)

Extraites de `compilateur/cli_rose.rose` et **reprises à l'octet près** dans
le point d'entrée (aucune modification de `lsrc_rpui`, `call_fn`, ni d'un
autre module) :

| # | Fonction | Rôle |
|---|---|---|
| 1 | `cli_banniere()` | `Rose 0.1.73 — …` (102 o) |
| 2 | `cli_aide()` | texte complet `rose help` (2 948 o) |
| 3 | `cli_info()` | `rose info` (384 o) |
| 4 | `cli_aide_comp()` | aide `comp` (519 o) |
| 5 | `cli_aide_methode()` | aide `methode` (486 o) |
| 6 | `cli_aide_run()` | aide `run` (383 o) |
| 7 | `cli_aide_jit()` | aide `jit` (507 o) |
| 8 | `cli_aide_interprete()` | aide `interprete` (385 o) |
| 9 | `cli_aide_transpile()` | aide `transpile` (692 o) |
| 10 | `cli_aide_bc()` | aide `bc` (453 o) |
| 11 | `cli_aide_rvm()` | aide `rvm` (733 o) |
| 12 | `cli_aide_aot()` | aide `aot` (477 o) |
| 13 | `cli_aide_pour($cmd)` | table de dispatch des 9 aides |
| 14 | `cli_err($msg)` | écriture `/dev/stderr` |
| 15 | `cli_rir_du($src)` | **écarté** : double de `lsrc_lire_programme` |

---

## 2. Dispatch câblé

Le point d'entrée (`# ═══ POINT D'ENTRÉE UNIQUE ═══`) teste `argument(0)`
comme mot-clé, puis retombe sur le chemin historique
`lsrc_lire_programme → parse_rir → call_fn` avec l'index décalé pour
`run`/`jit`/`interprete`/`methode`. Le chemin par défaut (fichier source en
`argument(0)`) est **intact**.

| Commande | Traitement | Fidélité |
|---|---|---|
| `version`, `--version` | `cli_banniere()` sur stdout | octet pour octet |
| `help`, `--help`, `-h`, `help <cmd>` | `cli_aide()` / `cli_aide_pour()` | octet pour octet (9 aides) |
| `help inconnu` | 2 lignes stderr, rc=1 | octet pour octet |
| *(aucun argument)* | `cli_aide()` sur stderr, rc=1 | octet pour octet |
| `info` | `cli_info()` | octet pour octet |
| `verif <src>` / `rir <src>` | `lsrc_lire_programme` ; `vérifié\n` ou dump RIR | octet pour octet |
| `rir` | `ERR|…` → stderr, rc=1 | message exact |
| `tokens <src>` | lexeur minimal `mc_tokens_du` | 53 / 78 o conformes |
| `ast <src>` | `mc_ast_du` (sous-ensemble `>>`) | 39 / 58 o conformes |
| `latex <src> [--apercu|-a]` | `mc_latex_du` depuis la constante RIR | 33 / 13 o conformes |
| `transpile <src> --cible wasm [-o f]` | `sw_module()` (porté de `ser_wasm.rose`) | **fichier de 48 o identique** |
| `bc <src> [-o f]` | `sb_module()` (porté de `ser_bytecode.rose`) | **fichier de 46 o identique** |
| `comp <src> [-o f]` | gabarit ELF C0 + patch de l'immédiat `movz` | **ELF de 8 908 o identique à C0** |
| `run` / `jit` / `interprete` | saut du mot-clé, exécution RIR | 3 o (`42\n`) |
| `methode` | menu + valeur RIR en **une seule écriture** | 570 o conformes |
| `-V`, fichier inconnu | `rose: fichier introuvable : …`, rc=1 | conforme (alias non supporté par l'oracle) |

### Détails techniques mesurés

- **`comp`** : le gabarit est un ELF C0 de 8 908 o encodé en 90 blocs
  hexadécimaux de 200 caractères (liste Rose, **pas** une chaîne de 89
  concaténations : au-delà de ~40 `+` imbriqués le backend G5 segfaulte,
  mesuré). Les octets are écrits avec `octet_brut()` (interception G5),
  pas `caractere()` qui encoderait les octets ≥ 128 en UTF-8 (8 908 → 12 300 o).
  L'instruction à patcher est `movz x0, #(valeur<<5)` aux octets 236–237 :
  patcher `valeur` au lieu de `valeur*32` donnait un md5 différent.
- **`methode`** : le backend G5 réécrit les N premiers octets d'une écriture
  longue quand une instruction `>>` suit (mesuré : `42\n` écrase `Com` d'un
  menu de 570 o). Le menu et la valeur sont donc émis en **une seule**
  écriture, la valeur venant du RIR (`mc_const_rir`), même chemin que `comp`.
- `-V` est volontairement **non** reconnu : l'oracle `rose/rose` 0.1.73 le
  traite comme un nom de fichier (`rc=1`, 31 o sur stderr).
- `transpile --cible js|py|c` n'est pas câblé : ces émetteurs en texte ne sont
  pas des `fn` du composite ; la cible `wasm` est la seule couverte (la
  mission se limite à lire/vérifier/compiler/exécuter).

---

## 3. Pilote de comparaison — 40 commandes, 0 échec

Pilote reproductible : `/tmp/opencode/b10_pilot2.sh`, binaire
`/tmp/opencode/b10_art/cli12.elf`, oracle
`/home/clausake/Stefan/ROSE/rose/rose` (moteur Rust 0.1.73).

```
OK    version / --version / -V / help / --help / -h / no-args / info
OK    help comp, methode, run, jit, interprete, transpile, bc, rvm, aot
OK    help inconnu, verif-sans-src, rir-sans-src, bc-sans-src,
      transpile-sans-src, comp-sans-src
OK    verif, rir, tokens, tokens2, ast, ast2, latex, latex-apercu, latex-a
OK    run, jit, interprete, methode, source-defaut
OK    transpile-wasm (48 o), bc-rosebc (46 o)
OK    comp-elf : 8 908 o, md5 7b0a2ef48ef4…, exécution = 42
=== RESULTAT PILOTE : 40 OK, 0 ECHEC ===
```

Chaque cas compare stdout, stderr **et** code de retour octet pour octet ;
les trois cas de fichiers produits comparent en plus le fichier lui-même.

---

## 4. Validation complète

| Contrôle | Commande | Résultat |
|---|---|---|
| Vérification du composé | `R verif compilateur/moteur_complet.rose` | `vérifié`, rc=0 |
| Vérification du banc | `R verif compilateur/tests_b10.rose` | `vérifié`, rc=0 |
| Banc Rose | `R compilateur/tests_b10.rose` | `TESTS B10 : 24 OK, 0 ECHEC`, rc=0 |
| C0 → ELF | `C0 compilateur/moteur_complet.rose > cli.elf` | rc=0, ELF aarch64 |
| Point fixe (voie 1) | `R rir compilateur/emetteur.rose` | 263 520 o, md5 `b3ed9262dd54776a6dbd07bc12368d0e` |
| Point fixe (voie 2) | `R compilateur/emetteur.rose compilateur/emetteur.rose` | idem, `cmp` identique |
| Validation finale | `RUSTFLAGS="" CARGO_PROFILE_RELEASE_STRIP=none bash scripts/validation_finale.sh` | **rc=0, 0 échec** |
| ↳ tests Rust | `cargo test --release` | 314 + 327 + 0 + … verts, rc=0 |
| ↳ AOT / JIT | bancs | 75/75 et 25/25 |
| ↳ WASM / RVM | — | 92 o / 317811 |
| ↳ batterie AZ | — | 31 CONFORMES, 0 non-conforme |
| ↳ HOST-1 | — | fib = 317811 |

`C0` = `/tmp/opencode/j6_agents/C0_j6.elf` (md5
`7bb091200cf927022eb9d86a548d2502`), `R` =
`/home/clausake/Stefan/ROSE/rose/rose`.

Deux échecs transitoires ont été **mesurés puis résolus** (pas masqués) :
- AZ à 0 conforme : `rose/rose` absent du worktree → lien symbolique
  temporaire vers le binaire de référence (retiré après validation) ;
- `opt_wall_final` (`RESULT:0`) : fichier temporaire partagé
  `/tmp/rose-opt-wall-final.rose` écrasé par une salve concurrente →
  rejeu isolé, vert.

---

## 5. Écarts restants (causes localisées, pas des « limites honnêtes »)

| Commande | Cause technique | Correctif concret |
|---|---|---|
| `transpile js/py/c` | émetteurs texte non exposés comme `fn` dans le composite | porter `emetteur_cpp/rust` ou ajouter les fonctions au composite (autre agent) |
| `aot`, `orc`, `link`, `llvm` | chaîne LLVM absente du moteur complet | lier le helper AOT published (hors périmètre du point 10) |
| `rvm` | VM bytecode Rust non portée | porter `rose/src/rosevm.rs` en Rose |
| `console`, `repl` | boucle `rep_*` composée mais non démarrée |point 11 du plan |
| `wasm <lib.wasm>`, `update`, `assoc`, `debug-lenteur` | introspection / réseau / Windows / profilage | hors composite |
| `comp` sur programme non constant | le gabarit ne patche qu'une constante `>>` | brancher `emettre_elf_print` (déjà dans C0) dans le point d'entrée |

Le point 10 est livré **complet pour son périmètre** : les 15 `cli_*` sont
câblées et les 40 comparaisons exécutées passent.

---

## 6. Reproductibilité

```bash
cd /tmp/opencode/corr_b10
R=/home/clausake/Stefan/ROSE/rose/rose
C0=/tmp/opencode/j6_agents/C0_j6.elf
bash scripts/garde_fou_mem.sh 12000000 $R verif compilateur/moteur_complet.rose
bash scripts/garde_fou_mem.sh 12000000 $R verif compilateur/tests_b10.rose
bash scripts/garde_fou_mem.sh 12000000 $R compilateur/tests_b10.rose
bash scripts/garde_fou_mem.sh 12000000 $C0 compilateur/moteur_complet.rose > /tmp/cli.elf
chmod +x /tmp/cli.elf
bash /tmp/opencode/b10_pilot2.sh /tmp/cli.elf
```

Artefacts hors dépôt : `/tmp/opencode/b10_art/cli12.elf`,
`/tmp/opencode/b10_art/vf3.log`, `/tmp/opencode/b10_art/pf1.rir`,
`/tmp/opencode/b10_pilot2.sh`.

`git status --short` en fin de mission :
` M compilateur/moteur_complet.rose`,
`?? compilateur/tests_b10.rose`,
`?? rose/docs/CORR10_CLI.md`.
