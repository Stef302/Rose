# Parité de format — WASM / ROSEBC v3 / `comp` (constat daté)

Worktree exclusif `/tmp/opencode/format_parite`, HEAD de départ `a8914cb`,
aucun commit, aucun push. Garde-fou sur toute commande longue :
`bash scripts/garde_fou_mem.sh 12000000 <commande>`. Mesures du
**24/11/2026**. Moteur Rust de référence :
`/home/clausake/Stefan/ROSE/rose/target/release/rose` (5 456 976 o).

Fichiers possédés : `compilateur/ser_wasm.rose`,
`compilateur/ser_bytecode.rose`, `compilateur/moteur_complet.rose` (zones
`sw_module`/`sb_module` et dispatch `comp` uniquement),
`compilateur/tests_formats.rose` (nouveau), ce document. `rose/src/**`,
`emetteur.rose` et `emetteur_elf.rose` sont intacts.

## 1. Les écarts, mesurés avant correction

Cinq programmes simples de `tests_selfhost/`, choisis dans le sous-ensemble
i64 couvert par les sérialiseurs : `t01_entiers`, `t20_add`, `t21_sous`,
`t22_mul`, `t29_precedence`.

| Programme | WASM Rust / C0 | `.rosebc` Rust / C0 | `comp` Rust / C0 |
|---|---|---|---|
| `t01_entiers` | **72 / 82** | 157 / 157 (md5 différents) | 5 457 037 / 8 908 o (5 / 1 lignes) |
| `t20_add` | **61 / 62** | 116 / 116 (md5 différents) | 5 457 034 / 8 908 o (4 / 1 lignes) |
| `t21_sous` | **56 / 60** | 94 / 94 (md5 différents) | 5 457 022 / 8 908 o (3 / 1 lignes) |
| `t22_mul` | **61 / 64** | 119 / 119 (md5 différents) | 5 457 031 / 8 908 o (4 / 1 lignes) |
| `t29_precedence` | 60 / 60 (déjà conforme) | 117 / 117 (md5 différents) | 5 457 051 / 8 908 o (4 / 1 lignes) |

`xxd` du cas documenté (82 contre 72), avant :

```
Rust  … 0a 24 01 22 01 01 7e 42 00 21 00 42 2a 21 00 42 79 21 00
          42 80 80 80 80 08 21 00 42 80 80 80 80 10 21 00 20 00 0b
C0    … 0a 25 01 23 01 01 7e 42 00 21 00 42 2a 21 00 42 c3 b9 00 21 00
          42 c2 80 c2 80 c2 80 c2 80 08 21 00 42 c2 80 c2 80 c2 80 c2 80
          10 21 00 20 00 0b
```

Deux défauts se superposent :

1. `-7` donnait `c3 b9 00` au lieu de `79` : l'ancien `sw_sleb` divisait une
   valeur **négative** par 128 puis testait `$x == -1`. Sous le composé C0 le
   test échouait (reste `0`, haut à 1) et émettait un octet de trop.
2. Tout octet ≥ 128 était encodé en **UTF-8** par `caractere()` sous C0 :
   `0x80` → `c2 80`, `0xf9` → `c3 b9`. Sous le moteur Rust, au contraire,
   `caractere(n)` est déjà un octet brut (`ecrire_bytes` vérifié : `195` →
   `c3`). D'où les « LEB128 surlongs » du `RELEASE_SANS_CARGO.md` §3.

Une **troisième cause**, découverte en cours de chantier, expliquait les
high bits perdus après la première correction : sous C0, `longueur()` et
`sous_chaine()` comptent des **points de code**, pas des octets. Mesure
isolée (`t_sub.rose`, C0) :

```
s = "000005" + 42 + 80 80 80 80 08     # 12 octets au total
longueur(s)                      = 8   # au lieu de 12
sous_chaine(s, 6, longueur(s))   = 42 80 80   # au lieu de 42 80 80 80 80 08
```

Le `sw_module()` du composé découpait justement la valeur de `sw_expr()`
avec `sous_chaine($r, 6, longueur($r))` et calculait les tailles de sections
avec `longueur($corps)` : les octets ≥ 128 invalident l'UTF-8 et étaient
tronqués / sous-comptés. C'est pourquoi le premier essai donnait 66 o au
lieu de 72 (et un module invalide pour Node).

`.rosebc` avant, sur `t01_entiers` (`cmp -l`, 14 octets différents) :

```
 37  14   1     # TyKind 0x0c (Dyn) côté Rust, 0x01 côté C0
 61  14   1 62   2   1 66   2   1   # expr ligne=2 / index=2 perdus
 84   1   0 85  14   1 86   3   1 90   3   1
117  14   1 118   4   1 122   4   1
149  14   1 150   5   1 154   5   1
```

La référence v3 écrit, pour chaque `Expr::Litteral`
(`rosevm.rs::ecrire_expr3`) : `tag 0x01 | chaine texte | bool flott |
bool rationnel | bool neg | TyKind u8 | ligne u32`, puis l'instruction
ajoute son propre `ligne u32`. Le C0 écrivait `TyKind=1`, `neg=0` et
`ligne=1` partout.

`comp` avant : le gabarit de 8 908 o ne patchait qu'**une** constante
(`movz x0` aux octets 236–237) et produisait un exécutable d'une ligne
(`0`) au lieu des 4 lignes de `rose comp`.

## 2. Correctifs appliqués

### WASM

- `sw_octet()` du **composé** utilise `octet_brut($v % 256)` (interception
  G5) au lieu de `caractere()` ; la copie autonome `ser_wasm.rose` garde
  `caractere()`, qui est l'octet brut côté moteur Rust.
- `sw_expr()` ne découpe plus jamais une chaîne d'octets : elle retourne
  une **liste de trois chaînes** `[position, octets, nb_octets]` construite
  par `joindre()` (les littéraux de liste à plusieurs éléments avec
  variables sont refusés/segfaulteurs par le front-end — mesuré). Les
  longueurs de section sont calculées par `sw_uleb_len()` / `sw_sleb_len()`
  qui comptent les octets, et la section 10 est assemblée explicitement
  `sw_octet(10) + sw_uleb($ncs) + $code_section` au lieu de `sw_section(10, …)`.
- `sw_sleb()` réécrit **sans aucune arithmétique sur un négatif** : pour
  `v < 0` on code `u = ~v = -v-1 ≥ 0` puis on complémente les 7 bits de
  chaque octet (`SLEB(v) = ~SLEB(~v)` sur 7 bits). Arrêt dès que `u < 64`
  (bit de signe nul). Cas vérifiés : `-1 → 7f`, `-7 → 79`, `-65 → bf 7f`,
  `579 → c3 04`, `2^31 → 80 80 80 80 08`.

### ROSEBC v3

- `sb_octet()` du composé passe aussi par `octet_brut()`.
- `sb_expr()` écrit le **TyKind** réel (`0x0c` Dyn pour un littéral
  direct, `0x01` Entier pour un littéral issu du repli), le drapeau `neg`
  (`1` pour `-7` direct, `0` pour un `-7` replié), et le `ligne u32`.
- `sb_instrs()` écrit le `ligne u32` de chaque instruction (index 1-based
  de la source, qui vaut le numéro de ligne pour ces programmes).
- Le dump RIR ne portant ni ligne ni TyKind, `sb_module_avec($rir, $src)`
  lit la source dans le dispatch `bc` et en dérive les descripteurs
  `ligne|ty|neg`. `sb_module($rir)` reste disponible en repli.

### `comp`

- Le dispatch `comp` n'écrit plus le gabarit d'une constante : il copie le
  **moteur complet** (`lire_texte`/`ecrire_bytes` sont transparents aux
  octets sous C0, vérifié par copie md5 identique), y ajoute la source,
  puis la queue `[u64 len LE]["ROSEB1\x01\x02"]`. C'est exactement
  `comp_programme_avec_message()` de `rose/src/main.rs`, donc la sortie est
  byte-identique à `rose comp` et exécutable (4 lignes).

## 3. Preuves après correction

Moteur self-hosté recompilé par C0 : `work/moteur_final.elf`,
**1 965 640 o**, magic `7f454c46`, machine 183 (aarch64), md5
`8c77c4e592a8b62b8cf7ecd57251fef6`, stderr 0 o. Banc :
`bash work/validate_final.sh` → **30 OK, 0 ECHEC** (6 contrôles × 5
programmes : octets wasm, résultat Node, octets bc, sortie `rose rvm`,
octets comp, exécution de l'exécutable).

| Programme | WASM (md5 commun) | `.rosebc` (md5 commun) | `comp` (md5 commun) |
|---|---|---|---|
| `t01_entiers` | 72 o · `62ad08469de7` | 157 o · `92d8a0d440e0` | 5 457 037 o · `3009fd69a3c6` |
| `t20_add` | 61 o · `c7bb9a95df37` | 116 o · `c0fece99f737` | 5 457 034 o · `4e18f929df3c` |
| `t21_sous` | 56 o · `4cc0b3fb746b` | 94 o · `e98dd36f681e` | 5 457 022 o · `023093090749` |
| `t22_mul` | 61 o · `16029648827c` | 119 o · `540fbb029a72` | 5 457 031 o · `7c156cab64c5` |
| `t29_precedence` | 60 o · `8eb3de3d749a` | 117 o · `b41850244c90` | 5 457 051 o · `92e1c08b3231` |

`cmp -s` rend 0 sur les cinq paires pour les trois formats. Sortie Node
(`WebAssembly.instantiate(...).exports.rose_main()`, même valeur des deux
côtés) : `4294967296`, `0`, `-12`, `-20`, `5`. Sortie `rose rvm` des deux
`.rosebc` : octet-identique sur les cinq. Sortie des exécutables `comp` :
4, 5, 3, 4, 4 lignes, octet-identique à `rose comp`.

`xxd` du même cas après correction (72 o des deux côtés) :

```
… 0a 24 01 22 01 01 7e 42 00 21 00 42 2a 21 00 42 79 21 00
  42 80 80 80 80 08 21 00 42 80 80 80 80 10 21 00 20 00 0b
```

## 4. Non-régression

| Contrôle | Commande réelle | Résultat |
|---|---|---|
| `verif` des 3 sources possédées + du banc | `rose verif …` | `vérifié`, rc=0 (×4) |
| Banc formats | `rose compilateur/tests_formats.rose` | **25 OK, 0 ECHEC** |
| Point fixe `b3ed9262` | `rose rir compilateur/emetteur.rose` ×2 | `b3ed9262dd54776a6dbd07bc12368d0e` ×2, `cmp`=0 |
| Parité self-host | `bash scripts/verifier_parite_selfhost.sh --moteur work/moteur_final.elf` | **141 CONFORMES, 0 DIVERGENT, 0 BLOQUE**, rc=0 |
| AOT `validation_finale` §3 | `ROSE=…/rose/target/release/rose bash rose/tests_aot8/verifier.sh` | **75 OK, 0 ECHEC** |
| JIT `validation_finale` §4 | `ROSE_TIMING=1 rose tests_aot8/programmes/*.rose` | **25/25** avec `principal=` |
| WASM + RVM `validation_finale` §5 | `rose transpile /tmp/opencode/fib28.rose …`, `rose bc`, `rose rvm` | fichier WASM produit, RVM = `317811` |
| AZ `validation_finale` §6 | `bash tests_selfhost/lancer_az.sh` | **31 CONFORME, 0 non-conforme** |
| HOST-1 `validation_finale` §7 | `rose compilateur/rose_natif.rose /tmp/opencode/fib28.rose` | magic `7f454c4602010100`, `317811` |
| Marqueurs exigés | `grep` sur `moteur_complet.rose` | `bi_pow`(1), `mc_args`(2), `rep_run`(2), `cli_aide_pour`(2), `cands`(5), `cr_feuille`(5) — tous présents |
| `rose/src/**`, `emetteur.rose`, `emetteur_elf.rose` | `git status --short` | intacts (aucune entrée) |

Les sections 2 à 7 de `scripts/validation_finale.sh` ont été rejouées
séparément dans ce worktree (le worktree ne contient pas de binaire
`rose/target/release/rose` local : la valeur `ROSE` de l'oracle a été posée
sur le binaire de référence partagé). La section 1 (build + tests cargo) n'a
pas été rejouée : aucun fichier `rose/src/**` n'a été modifié, et un build
cargo concurrent aurait seulement consommé la mémoire du nœud. Aucun échec
n'est donc masqué : le constat vaut pour les contrôles 2–7.

## 5. Commandes reproductibles

```sh
cd /tmp/opencode/format_parite
R=/home/clausake/Stefan/ROSE/rose/target/release/rose
bash scripts/garde_fou_mem.sh 12000000 "$R" verif compilateur/ser_wasm.rose
bash scripts/garde_fou_mem.sh 12000000 "$R" verif compilateur/ser_bytecode.rose
bash scripts/garde_fou_mem.sh 12000000 "$R" verif compilateur/moteur_complet.rose
bash scripts/garde_fou_mem.sh 12000000 "$R" compilateur/tests_formats.rose
bash scripts/garde_fou_mem.sh 12000000 /tmp/opencode/C0_h.elf \
  compilateur/moteur_complet.rose > work/moteur_final.elf
bash work/validate_final.sh          # banc hors dépôt : 30 contrôles, voir §3
```

Le banc `work/validate_final.sh` (hors dépôt, non versionné) enchaîne pour
chaque programme : `rose transpile … --cible wasm` des deux côtés + `cmp -s`
+ `WebAssembly.instantiate` sous Node ; `rose bc` des deux côtés + `cmp -s` +
`rose rvm` + `cmp -s` des sorties ; `rose comp` des deux côtés + `cmp -s` +
exécution + `cmp -s` des sorties. `work/` contient aussi les artefacts bruts
(`val/*.wasm`, `val/*.rosebc`, `val/*.comp`, `validate_final.out`).
