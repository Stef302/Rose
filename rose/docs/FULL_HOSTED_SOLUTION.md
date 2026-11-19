# Full-hosted : état réel après extension G5 — 19/11/2026

**Constat daté, pas une promesse.** Dépôt `/home/clausake/Stefan/ROSE`,
`HEAD = 77478f1bf874688c5df4c0adddd7576c9fb52cc2` au début du chantier.
Machine `gx10-2`, `uname -m = aarch64`. Binaire livré `rose/rose` md5
`3d72c37163eeaa03e19c5262c06dc914` (non remplacé). Binaire construit pour la
mesure : `/tmp/opencode/rc_agents/target/release/rose` md5
`9c5fffa0d1f84213b08f261016e9ddea`
(`CARGO_TARGET_DIR=/tmp/opencode/rc_agents/target CARGO_BUILD_JOBS=4 taskset -c 12-15 cargo build --release --bin rose`,
43,85 s, 261 warnings). Aucun commit, aucune publication. Seuls
`compilateur/emetteur_elf.rose` (fichier confié) et ce document ont été modifiés
par ce chantier ; `compilateur/emetteur.rose` est **inchangé** (md5
`7b6ad535…`).

## 1. Verdict court

**Toujour pas « full hosted ». Le produit `rose/rose` exige toujours
`cargo build --release`.** Ce qui a réellement changé : l'émetteur Rose ne se
contente plus d'extraire une constante. Il abaisse maintenant un sous-ensemble
utile du RIR en code machine AArch64 et produit des ELF autonomes qui
**exécutent** des programmes Rose réels (arithmétique, branches, boucles,
fonctions, récursion, appels imbriqués, affichage décimal).

| Question | Avant (G4, re-mesuré) | Après (G5, mesuré) |
|---|---|---|
| `rose rir p47.rose` → `emetteur_elf.rose` | ELF 132 o, `exit(47)` | ELF autonome calculant et affichant la valeur |
| `bin` / `saut` / `appel` dans le RIR | refusés (rc=2, 0 o) | émis en instructions natives |
| Programme Rose avec fonction → ELF | refusé | ✅ `fib(10)` → `55` |
| Point fixe M4.5 | `b084ac5c…` | `b084ac5c…` (inchangé) |
| `emetteur.rose` → ELF | rc=2 | rc=2 (`G5 : forme parenthésée non couverte : s`) |
| `rose/rose` sans cargo | ❌ 0 % | ❌ **0 %** |

## 2. État initial re-mesuré (sauvegarde G4)

`/tmp/opencode/rc_agents/emetteur_elf.rose.BASE_G4` est la copie exacte du
fichier avant modification (md5 `afeffb3b477c44bf591fcc2c22f52167`).

```
rose rir p47.rose                                    -> p47.rir
rose <BASE_G4> p47.rir                               -> 132 octets, rc=0
./p47.elf                                            -> exit 47
rose <BASE_G4> t_bin.rir                             -> rc=2, 0 octet
stderr : emetteur_elf G4 : RIR hors sous-ensemble (fonctions, appels, bin, sauts : runtime requis)
```

`g4_depuis_rir` (lignes 174-229 de la version initiale) scannait les ops,
refusait explicitement `app `, `appel`, `saut`, `(bin`, puis extrayait la
dernière constante `affect slot N = entier K` ; `g4_elf` (lignes 232-267)
émettait uniquement `mov x0,#K ; mov x8,#93 ; svc #0`. Helpers : `u8le`,
`u16le`, `u32le`, `u64le`, `g4_pos`, `g4_entier_ou_zero`, `g4_trim`,
`g4_nu_op`, `g4_operand`, `g4_lignes`, `g4_affect_val`.

## 3. Ce qui a été ajouté (émetteur G5, toujours en Rose)

Le nouveau moteur occupe les lignes ~48-620 de `compilateur/emetteur_elf.rose`;
la voie historique (programme source, constant-folding) est conservée et
rejouée (voir §6). Il est déclenché uniquement si `argument(0)` commence par
`rir`.

### 3.1 Analyse du RIR

- `g5_parse` : découpe `fonction …` / `param` / `n_slots` / `noms [i]nom` /
  `ops`, avec une étiquette par fonction et une par opération (listes
  append-only, car Rose interdit l'affectation par index sur une liste).
- `g5_tokens` : tokenizer des expressions RIR (parenthèses, atomes).
- `g5_noms`, `g5_slot_of` : table `nom → slot`.
- `g5_slots` : slots dans le frame, surface temporaire par profondeur
  d'expression (32 niveaux), zone d'arguments (8 registres AAPCS64).

### 3.2 Opcodes RIR maintenant émis

| Famille | Formes RIR couvertes | Code généré |
|---|---|---|
| `affect`, `affect_ast` | `affect slot N = entier K`, `= slot M`, `= (bin …)`, `= (app …)` | évaluation dans x0 puis `str x0,[sp,#16+8N]` |
| `bin` | `+ - * / div % == != < <= > >= et ou` | `ADD/SUB/MUL/SDIV/UDIV+MSUB/AND/ORR`, `CMP` + `CSET` |
| `saut`, `saut_si_faux`, `saut_si_faux_abstrait` | cible = index d'op | `B`, `CMP x0,#0` + `B.EQ`, patches d'offset |
| `appel`, `appel_rir`, `(app f …)` | 0..8 arguments | evaluation → pile, `ldr x0..x7`, `BL f`, résolution interne par nom |
| `retour`, `retour_abstrait` | valeur dans x0 | épilogue (`ldr x30`, `add sp`, `ret`) |
| `afficher`, `afficher_abstrait` | valeur dans x0 | `BL print_i64` |
| `expr` | expression nue | évaluation, résultat ignoré |
| littéraux | `(n K)`, `entier K`, `slot N`, `(v nom)`, `(b vraie/faux)`, `(neg …)`, `(non …)`, `(uni non/neg …)` | `MOVZ`/`MOVK`, `LDR`, `SUB x0,xzr,x0`, `CSET` |

Encodage : constantes AArch64 vérifiées sur place avec `as` + `objdump`
(`MOVZ 0xD2800000`, `MOVK 0xF2800000`, `ADD 0x91000000`, `SUB 0xD1000000`,
`ADD reg 0x8B000000`, `MUL 0x9B007C00`, `SDIV 0x9AC00800`, `CMP 0xEB00001F`,
`LDR/STR 0xF94…/0xF90…`, `B 0x14000000`, `BL 0x94000000`, `B.cond 0x54000000`,
`RET 0xD65F03C0`, `SVC 0xD4000001`). Les décalages de branchement sont
calculés en nombre de mots (`target - pc`) et patchés après émission.

### 3.3 Prologue / épilogue / convention

Chaque fonction : `sub sp,sp,#frame` ; `str x30,[sp]` ; `str x0..xn,[sp,#slot]`
pour ses paramètres. Retour : `ldr x30,[sp]` ; `add sp,sp,#frame` ; `ret`.
La récursion fonctionne car chaque appel a son propre frame. `$principal`
joue le rôle de `_start` : pas de retour, branchement vers la routine de sortie.

### 3.4 Runtime minimal émis en machine

`print_i64` : conversion décimale par `UDIV`/`MSUB` sur le frame, gestion de
`0` et du signe `-`, écriture par syscall `write(1, buffer, len)` (syscall 64),
puis `exit(0)` (syscall 93). Aucune libc, aucun linker, aucun `.rs` dans le
chemin G5.

Le writer ELF reste un ET_EXEC aarch64 d'un seul `PT_LOAD` (header 64 o +
phdr 56 o + code), `e_shoff = 0`. Il n'émet ni sections ni symboles.

## 4. Programmes réellement compilés en ELF par Rose (preuves)

Chaîne rejouée deux fois : avec `rose/rose` livré **et** avec le binaire
construit dans `/tmp/opencode/rc_agents/`. Script :
`/tmp/opencode/rc_agents/validate_g5.sh`, sortie
`/tmp/opencode/rc_agents/validation_final.log`.

| Programme | RIR | ELF (o) | Sortie ELF | Sortie Rose | rc ELF |
|---|---:|---:|---|---|---:|
| `t_bin.rose` (`7+35`) | 4 ops | 408 | `42` | `42` | 0 |
| `t_fib.rose` (`fib(10)` récursif) | 4 ops, 2 fonctions | 540 | `55` | `55` | 0 |
| `t_sum.rose` (boucle + 2 fonctions) | 9 ops, 3 fonctions | 588 | `110` | `110` | 0 |
| `examples/fibonacci.rose` (3 `>>`) | 6 ops | 580 | `556765317811` | `55 6765 317811` | 0 |
| `t_neg.rose` (entiers négatifs) | 2 ops | 400 | `-3-12` | `-3 -12` | 0 |
| `t_args.rose` (3 paramètres, 2 appels) | 5 ops | 560 | `123789` | `123 789` | 0 |

`file` sur chacun : `ELF 64-bit LSB executable, ARM aarch64, version 1 (SYSV),
statically linked, no section header`. `objdump` montre bien `sub sp,sp,#…`,
`str x30,[sp]`, `bl`, `cset`, `b.eq`, `udiv`, `msub`, `svc`.

Exemple rejouable :

```sh
cd /home/clausake/Stefan/ROSE
./rose/rose rir /tmp/opencode/rc_agents/t_fib.rose > /tmp/opencode/rc_agents/t_fib.rir
./rose/rose compilateur/emetteur_elf.rose /tmp/opencode/rc_agents/t_fib.rir > fib.elf
chmod +x fib.elf && ./fib.elf          # 55, rc=0
```

## 5. Réflexivité : NON atteinte

| Cible | `rose rir cible.rose` | `rose compilateur/emetteur_elf.rose <rir>` |
|---|---|---|
| `compilateur/emetteur.rose` (262 802 o de RIR) | rc=0 | rc=2, 0 o : `G5 : forme parenthésée non couverte : s` |
| `compilateur/emetteur_elf.rose` (201 429 o de RIR) | rc=0 | rc=2, 0 o : `G5 : forme parenthésée non couverte : lstvide` |

La première forme bloquante est `(s "ident")` : les chaînes RIR sont des
objets runtime, pas des immédiats machine. Viennent ensuite `(idx …)` (385),
`(lst …)` (64), les appels vers les primitives Rose (`longueur`,
`sous_chaine`, `ordre`, `joindre`…), puis le bigint.

**Couverture mesurée sur le RIR réel de `emetteur.rose`** (111 fonctions,
3 614 ops) par l'analyseur indépendant
`/tmp/opencode/rc_agents/coverage.py` (qui reproduit exactement les règles de
`g5_eval`) : **1 667 / 3 614 = 46,1 % des opérations sont abaissables** par la
grammaire G5, dont 787 `affect`, 339 `saut_si_faux_abstrait`, 115
`retour_abstrait`, 328 `saut`, 57 `affect_ast`, 14 `retour`, 27 `expr`.
⚠ Ce pourcentage est une **couverture syntaxique potentielle** : l'émetteur
s'arrête à la première forme non couverte, donc **0 fonction complète de
`emetteur.rose` n'est produite**. Sur les quatre programmes de la §4, la
couverture est de 100 % des ops.

Formes imbriquées comptées dans ce RIR : `(v` 7 087, `(bin` 2 437, `(s` 1 823,
`(app` 1 306, `(n` 426, `(idx` 385, `(appel_rir` 269, `(b` 66, `(lst` 64,
`(uni` 11, `(non` 11, `(neg` 7.

## 6. Validation

```
cd rose && CARGO_TARGET_DIR=/tmp/opencode/rc_agents/target cargo test --release
# 214 + 198 + 6 + 1 = 419 tests, 0 échec

cd .. && ./scripts/rose-jardin.sh verif-tous
# ✓ Tous les jardins (10) sont valides
./rose/rose jardins/tests/premiers.rose | tail -1
# ── jardin/premiers : 20 tests, 0 échec ──

./rose/rose rir compilateur/emetteur.rose > fpA.rir
./rose/rose compilateur/emetteur.rose compilateur/emetteur.rose > fpB.rir
md5sum fpA.rir fpB.rir  # b084ac5c926f6ae486845f9bc96c616d ×2, cmp_rc=0

./rose/rose verif compilateur/emetteur_elf.rose  # vérifié

# voie historique préservée :
printf '>> 5+3\n' > hist.rose
./rose/rose compilateur/emetteur_elf.rose hist.rose hist.elf   # 4132 o, ./hist.elf -> 8
```

## 7. Frontière exacte restante — ce qui empêche encore le full hosting

1. **Runtime / sémantique Rose** : G5 ne gère que des entiers machine 64 bits.
   Les valeurs hors `[-2^63, 2^64)` sont refusées ; `+`, `*` etc. sont modulo
   2^64, alors que Rose utilise des bigints exacts. `(s …)`, `(idx …)`,
   `(lst …)`, `(b …)` partiel, `ecrire_bytes`, `lire_texte` et les primitives
   chaîne/listes ne sont pas abaissées.
2. **Abaissement incomplet** : 46,1 % des ops de `emetteur.rose` sont
   abaissables ; l'émetteur s'arrête sur `(s …)` dès la première fonction.
3. **Writer ELF minimal** : `e_shoff = 0`, aucune section, aucun symbole,
   aucun linker, aucun PE/ Mach-O depuis G5.
4. **Produit** : `rose/rose` reste un binaire Rust/Cranelift. Les ELF émis
   par G5 s'exécutent seuls, mais **la chaîne de compilation** (l'émetteur
   Rose) tourne encore dans `rose/rose`, et **0 %** du produit livré est
   reconstruit sans `cargo build --release`. Mono binaire
   construit ici par cargo, jamais installé dans le dépôt.

Ce qui reste à faire n'est pas un patch de plus : il faut porter en Rose la
représentation des valeurs (bigint, chaîne, liste), le runtime E/S et un writer
ELF complet, puis seulement alors remplacer le produit. La porte G4 reste donc
**ouverte mais loin d'être franchie**.

## 8. Fichiers et rejouabilité

- `compilateur/emetteur_elf.rose` : émetteur G4+G5 (3 369 lignes, md5
  `ec2c50c4a2e5b95db201ce55d6860f7e`).
- `/tmp/opencode/rc_agents/emetteur_elf.rose.BASE_G4` : version initiale.
- `/tmp/opencode/rc_agents/g5_full.rose` : noyau G5 autonome utilisé pour le
  développement.
- `/tmp/opencode/rc_agents/{t_bin,t_fib,t_sum,t_neg}.rose` et leurs `.rir`/`.elf`.
- `/tmp/opencode/rc_agents/validate_g5.sh`, `validation_final.log`,
  `coverage.py`, `stats_rir.txt`, `build.log`, `tests.log`.
- `/tmp/opencode/rc_agents/target/release/rose` : binaire construit pour la
  mesure ; `rose/rose` du dépôt n'a pas été remplacé.

**État final factuel** : 4 programmes Rose réels (+ variante négative)
compilés en ELF autonomes par l'émetteur Rose et exécutés avec la bonne valeur,
sous les deux binaires ; point fixe M4.5 inchangé ; 419 tests/0 échec ; jardin
20/20. Réflexivité ❌, `rose/rose` toujours dépendant de cargo ❌.
**Verdict : toujours NON — progrès réel mais borné sur G4.1/G4.3, aucun
changement sur G2, G3, G4.2 et G4.4.**
