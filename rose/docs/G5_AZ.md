# G5_AZ — self-hosting A–Z : tables réelles et primitives (constat mesuré, 21/11/2026)

Sous-agent `sh12`. Dépôt `/home/clausake/Stefan/ROSE` (`Stefan/ROSE`), HEAD de départ `cadcdb8`.
Deux fichiers annoncés : `compilateur/emetteur_elf.rose` et ce document.
**Aucun commit, aucune publication, aucun autre fichier modifié par ce chantier.**
Artefacts : `/tmp/opencode/sh12_agents/` (dont `tests/`, `battery2/`, logs).

Émetteur final md5 `7742556ae483aee5e02a1458ce326c83` (`rose verif` → `vérifié`, rc=0).
Binaire Rose de référence `./rose/rose` md5 `12eac46007cf0fe9e1aec2dff6fbf665`.

---

## 1. Primitive prioritaire : tables RÉELLES (cles **et valeurs**)

### 1.1 Mesure avant / après (programme `/tmp/opencode/sh12_agents/tests/tab.rose`)

```
$t = table()
inserer($t, "a", 10)
inserer($t, "b", 20)
>> cle_existe($t, "a")
>> $t["a"]
>> longueur($t)
supprimer($t, "a")
>> cle_existe($t, "a")
>> longueur($t)
```

| | stdout (5 lignes) | rc |
|---|---|---|
| Natif `ROSE_SANS_JIT=1` | `vrai / 10 / 2 / faux / 1` | 0 |
| ELF **avant** | `vrai / 0 / 2 / vrai / 2` | 0 |
| ELF **après** | `vrai / 10 / 2 / faux / 1` | 0 |

`cmp natif/ELF` = 0, stderr vides des deux côtés, rc égaux. RIR :
`inserer` → `(app inserer …)`, `$t["a"]` → `(idx (v t) (s "a"))`,
`supprimer` → `(app supprimer …)`.

### 1.2 Ce qui était faux

- `inserer` était ré-affecté à `g5_joindrel` (route de `joindre`) : seule la CLÉ
  était stockée ; la valeur `v` était ignorée.
- `idx` sur une table entrait dans la branche « liste » et renvoyait le mot
  brut à l'offset `8*i` — donc `0` ou une adresse, jamais la valeur.
- `supprimer` tombait sur `g5_emit_stub0` (x0=0) : aucun effet.

### 1.3 Representation retenue

Descripteur de table étendu (le tag reste 2 pour ne pas casser `cle_existe` /
`longueur` / l'affichage liste) :

```
+0  tag = 2
+8  len = nombre d'entrées
+16 cap NÉGATIF  ← marqueur « table » (une liste a cap > 0) ; |cap| = capacité
+24 key_ptr   → 8 octets par clé
+32 value_ptr → 8 octets par valeur, même index
+48..175      blocs initiaux de 8 clés + 8 valeurs (allocation unique de 176 o)
```

Routines émises `g5_emit_inser`, `g5_emit_suppr` et branche table dans
`g5_emit_idx` (test du signe de `cap`). Mutation **en place** du descripteur :
les alias voient l'insertion/la suppression, comme dans le runtime Rose.

### 1.4 Défaut intermédiaire trouvé et corrigé par désassemblage

Au premier essai, la croissance (9ᵉ insertion) n'allouait que **la moitié** des
octets nécessaires : deux `add` de doubling au lieu de trois (`0x4008cc` dans
l'ELF), donc `8*newcap/2`. `value[0]` chevauchait `key[8]` ; `tabgrow2.rose`
donnait `9 / 8 / 107 / 108` au lieu de `9 / 100 / 107 / 108` (ELF). Corrigé,
la croissance est conforme.

### 1.5 Autres preuves (octet pour octet, natif = ELF)

| Test | Natif = ELF |
|---|---|
| `tabgrow2.rose` (9 insertions, seuil de croissance) | `9 / 100 / 107 / 108` |
| `tabgrow.rose` (14 insertions bouclées, 2 suppressions, clés entières) | `14 / 0 / 70 / 80 / 130 / 13 / faux / 90 / 12 / 130` |
| `sem.rose` (clés mixtes chaîne + entier) | `10 / un / faux / 1` pour les lignes de table |

Un bug latent de `g5_emit_clex` a été corrigé au passage : chercher une clé
chaîne dans une table dont une clé stockée est un entier appelait `streq` sur
un pointeur NUL masqué → SIGSEGV (mesuré : rc=139 sur `sem.rose`). Un test de
tag de la clé stockée a été ajouté (`$lbl+8`).

---

## 2. Stubs remplacés par de vraies routines

| Primitive | Nature | Avant (ELF) | Après (ELF = natif) |
|---|---|---|---|
| `abs`, `min`, `max` | routines entières | `0 / 0 / 0 / 0` | `7 / 7 / 3 / 5` |
| `type(2)`, `type("a")`, `type([1])`, `type(vrai)` | dispatch tag + replis à l'émission pour les littéraux | `0 / 0 / 0 / 0` | `entier / chaine / liste / booleen` |
| `chaine(3 < 5)` | `g5_est_bool` + routine `g5_emit_chbool` | `1` | `vrai` |
| `graine` / `prng` / `hasard` | splitmix64 complet (miroir de `rose/src/run.rs` : `rng_tirer`, `graine_de`, `hasard`, y compris le masque puissance-de-2 et `umulh`) | `0 / 0 / 0` … | t90 `15 / 27 / 34`, t91 `0 / 9`, t92 `1 / 0 / 0 / 2 / 2` (6 tirages) |

La graine d'état est un `u64` brut à `0x60007000` (hors tas, hors globales).
Les constantes splitmix64 sont émises par `movz`/`movk` en morceaux ≤ 16 bits :
aucun littéral > 2^60 n'est introduit dans l'émetteur (contrainte G5).

`type` ne peut pas distinguer booléen et entier **variables** sous l'ABI actuelle
(tag 0 pour les deux) : les littéraux et les slots connus booléens sont
corrects, les autres retombent sur `entier`. Limite ABI documentée §5.

---

## 3. Rationnels négatifs, division et modulo

- **Formateur** `g4_ratio_texte` : pour `-a/b`, la partie entière affichée est
  `ceil(a/b)` et les décimales restent celles de `a/b`. Mesure :
  `-7/3 → -3.[3]` (avant `-2.[3]`), `-7/2 → -4.5`, `-1/2 → -1.5`,
  `-22/7 → -4.[142857]`. `rat.rose` (11 cas) : **11/11 conformes**.
- **BUG d'encodage** : `sdivr()` utilisait la base `0x9AC00800` = `UDIV`
  au lieu de `0x9AC00C00` = `SDIV`. Corrigé ; `udivr()` (conversion décimale)
  reste sur `UDIV`, sans changement de comportement pour ses opérandes positifs.
- **Modulo plancher** : `-7 % 3` natif = `2`, ELF avant = `0` (après correction
  SDIV seulement : `-1`). Ajout de l'ajustement du reste quand son signe
  diffère de celui du diviseur → `2 / 0 / 2` conforme (t24).

Batterie : t07, t23, t24 passent de DIVERGENT à CONFORME.

---

## 4. Bilan batterie / jalons (état final md5 `7742556a…`)

| Contrôle | Résultat mesuré |
|---|---|
| `./rose/rose verif compilateur/emetteur_elf.rose` | `vérifié`, rc=0 |
| Batterie SH9 (80 programmes, `SH9_OUT=…/battery2`) | **66 CONFORMES / 7 DIVERGENTS / 7 BLOQUÉS** (référence SH11/SH12 de départ : 57 / 16 / 7). +9 conformes, 0 régression |
| `t999_critique_g5` (argument fourni) | **CONFORME, 20/20 lignes**, rc=0, stdout+stderr octet pour octet |
| Point fixe `b3ed9262dd54776a6dbd07bc12368d0e` (263 520 o) | **×2 IDENTIQUE** (dump ELF `cmp`=0 ; ELF émis `3b7c6604…`, rc=0) |
| `F1` vs `F2` (auto-régénération, timeout 900 s) | **IDENTIQUES** : 464 400 o, md5 `785a24939a2109c78db3af99804559a4`, `cmp`=0 |
| `cargo test --release -- --test-threads=1` | **rc=0, 515 passés, 0 échec**, 6 ignorés (258+242+6+8+1) |
| `scripts/rose-jardin.sh verif-tous` | 9/10 valides, rc=1 : seul `jardins/gui_native.rose` rejeté (« marqueur Rust/FFI interdit »), contenu étranger à ce chantier (identique à la référence) |
| `fib25` natif vs ELF | **75025 / 75025**, conforme |

Chaîne F1/F2 rejouée sur l'état final (artefacts `*final*`) :
```
./rose/rose rir compilateur/emetteur_elf.rose        > e_fix_final.rir  # 450b6480…
./rose/rose compilateur/emetteur_elf.rose e_fix_final.rir > F1_final.elf
./F1_final.elf e_fix_final.rir                       > F2_final.elf
cmp F1_final.elf F2_final.elf                                           # 0
```
Point fixe rejoué sur l'état final : `/tmp/opencode/sh12_agents/checkpoint2.log`
(début `19:51:40Z`, md5 émetteur `7742556a…`, deux dumps `b3ed9262…`, `cmp`=0).

---

## 5. Blocages ABI — NON tentés à l'aveugle (preuves)

Les 7 divergences restantes exigent toutes un changement de représentation des
valeurs. Aucune n'a été livrée : le point fixe et F1==F2 priment.

1. **Grands entiers** (t32 `2^62`, t33 `2^64`, t34 décimal géant).
   Un entier Rose G5 est un immédiat `n<<3` dans un mot 64 bits (≈61 bits
   utiles, décision AB1). Mesure natif vs ELF : `4611686018427387904 → 0`,
   `18446744073709551616 → 0`, `123456789012345678901234567890 → 248789718555101906`.
   Il faut des multi-limbes et modifier `g5_load_val`, tous les opérateurs de
   `g5_op`, `g5_emit_ch`, `g5_emit_print`, les slots et l'ABI d'appel. Risque
   maximal pour le point fixe : non tenté.
2. **Rationnel calculé à l'exécution** (t08 : `22/7` natif `3.[142857]`, ELF `3`).
   Les littéraux rationnels sont corrects (repli à l'émission), mais un résultat
   de division a besoin d'un nouveau tag « rationnel » avec numérateur/
   dénominateur : changement d'ABI, non tenté.
3. **Unicode** (t04, t41, t44). Tentative **complète puis annulée** :
   stockage UTF-8 dans les descripteurs (len = octets), `longueur()` recompte
   les points de code, `sous_chaine`/`idx`/`ordre` décodent, `caractere` replié
   à l'émission. Résultat mesuré : **69/80 conformes** (t04, t41, t44 corrigés),
   **t999 20/20 et point fixe ×2 OK** — mais **F1==F2 CASSE** : F1 émis sort par
   `exit(2)` immédiat, sans sortie, sur sa propre RIR (`e_fix.rir`, 340 378 o),
   alors qu'il émet correctement une petite RIR. Cause non identifiée dans le
   temps imparti (chemin d'erreur `g5_err` instrumenté : non atteint).
   Conformément à « NE JAMAIS livrer si l'un casse », **tout le bloc Unicode a
   été annulé** ; l'émetteur livré est l'état validé `7742556a…` (66 conformes,
   tous jalons verts). Ce blocage est donc documenté, pas livré.

---

## 6. Reste

- Les 7 divergences de la §5, toutes liées à l'ABI des valeurs.
- Les 7 programmes BLOQUÉS sont des erreurs de syntaxe/implémentation natives
  (surcharges, HOF, factorielle, listes de listes, argument hors limites) :
  la batterie les classe « bloqués » des deux côtés, ce ne sont pas des
  régressions G5.
- `type()` sur une variable booléenne non connue à l'émission (limite ABI 2).

## 7. Concurrence de fichier (signalée à l'orchestrateur)

Pendant ce chantier, `git status --short` a montré, **en plus** de
`compilateur/emetteur_elf.rose`, deux fichiers modifiés par d'autres agents :
`rose/Cargo.lock` et `rose/src/i128x.rs`. **Ce chantier ne les a ni ouverts,
ni modifiés, ni restaurés** (un autre travail compile la cible Windows en
parallèle). Aucun commit effectué.
