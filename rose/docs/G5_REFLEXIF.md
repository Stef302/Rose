# G5-R — extension réflexive de l'émetteur Rose (constat daté, 19/11/2026)

Agent `se2` (première mission). Dépôt `/home/clausake/Stefan/ROSE`, `HEAD = 0f0f58d412f696c3bca9cfb97127b53c95ce970d`.
Des commits **concurrents** (autres agents) sont arrivés pendant le chantier : `HEAD = 9bfda874…`
à la clôture. Aucun n'a touché `compilateur/emetteur.rose`, `rose/rose` ni mes deux fichiers
(md5 re-vérifiés en fin de chantier). Je n'ai fait **ni commit ni publication**.
**Deux fichiers du dépôt touchés au total** :
`compilateur/emetteur_elf.rose` (fichier confié) et ce document.
`compilateur/emetteur.rose` est **inchangé** : md5 `7b6ad5351a1b95830242ab28c3aab9cb`
(re-vérifié en fin de chantier). `rose/rose` est **inchangé** : md5 `3d72c37163eeaa03e19c5262c06dc914`.

Binaire construit pour la mesure, sans remplacer le produit :
`/tmp/opencode/se2_agents/target/release/rose`, md5 `d5af52aafe5ffee84d8d9513e459132d`
(`CARGO_TARGET_DIR=/tmp/opencode/se2_agents/target CARGO_BUILD_JOBS=4 taskset -c 10-13 cargo build --release --bin rose`, 1 min 09 s).
Toutes les mesures ci-dessous ont été rejouées **aussi avec le binaire livré `./rose/rose`** quand c'était utile.

## 1. Verdict court

**Progrès réel et très borné sur l'abaissement ; réflexivité NON atteinte au sens « compilateur auto-hébergé fonctionnel ».**

| Mesure | Avant (G5 initial) | Après (G5-R) |
|---|---|---|
| Couverture syntaxique des ops de `emetteur.rose` (3 614 ops) | **1 667 / 3 614 = 46,1 %** | **3 585 / 3 614 = 99,2 %** en sémantique stricte ; **3 614 / 3 614 = 100 %** en comptant les primitives neutres |
| Fonctions entières de `emetteur.rose` acceptées | **0** | **111 / 111** émises dans un ELF |
| `emetteur.rose` → ELF | rc=2, 0 octet (`forme parenthésée non couverte : s`) | **rc=0, ELF 327 336 octets** ; exécution → **SIGSEGV (rc=139)** |
| `emetteur_elf.rose` → son propre RIR → ELF | rc=2, 0 octet (`lstvide`) | **rc=0, ELF 183 604 octets** (87 fonctions) ; exécution → **SIGSEGV (rc=139)** |
| Une fonction réelle de `emetteur.rose` (`BASE`) compilée en ELF et exécutée | ❌ | ✅ ELF 1 532 octets, `BASE("a|b")` → `"a"`, `longueur` affiche `1`, rc=0 |
| `rose/rose` sans cargo | ❌ 0 % | ❌ **0 %** |

La différence entre « 100 % des ops abaissées » et « réflexivité » est essentielle : l'ELF du compilateur
est **produit** mais **ne fonctionne pas**. Ce n'est donc pas un bootstrap.

## 2. Méthode de mesure de la couverture

Analyseur indépendant écrit pour ce chantier : `/tmp/opencode/se2_agents/coverage3.py`.
Il rejoue les règles de `g5_eval` (formes, opcode binaire, résolution des appels) sur le RIR réel
`fpA.rir` (= `rose rir compilateur/emetteur.rose`, md5 `b084ac5c…`, 262 802 octets, 111 fonctions, 3 614 ops).

```
 Avant : python3 /tmp/opencode/rc_agents/coverage.py  fpA.rir
         -> 1667/3614 ops couvertes = 46.1 %
 Après : python3 /tmp/opencode/se2_agents/coverage3.py fpA.rir
         -> strict: 3585/3614 ops = 99.2 % ; fonctions completes 108/111
            non couvertes (strict) : app 26, bin 3
         -> stub  : 3614/3614 ops = 100.0 % ; fonctions completes 111/111
            primitives stubées : flottant 4, nb_arguments 1, lire_texte 2,
            argument 1, table 1, inserer 21, cle_existe 1
```

Les 26 `app` du mode strict sont des primitives désormais **remplacées par un retour neutre**
(retour 0 ou chaîne vide), pas des appels implémentés. Le mode « stub » mesure l'acceptation syntaxique,
le mode « strict » la sémantique réellement implémentée. **Aucune des deux mesures ne prouve
l'exécutabilité du compilateur émis.**

## 3. Formes ajoutées et preuves

Toutes les preuves sont des commandes exécutées avec le binaire construit ici ; `R` désigne
`/tmp/opencode/se2_agents/target/release/rose`, `W=/tmp/opencode/se2_agents`.

### 3.1 Chaînes `(s "…")` — première forme bloquante

- `g5_tokens` est devenu sensible aux guillemets : un littéral contenant espaces, parenthèses ou
  échappements est **un seul jeton** (auparavant `(s "0 ")` se découpait en 4 jetons).
- `g5_unescape` décode `\\`, `\"`, `\n`, `\r`, `\t` comme `ast_echapper` dans `emetteur.rose`.
- Représentation machine : descripteur `[tag=1][len][octets…]`, aligné 8, alloué dans un tas.

Preuve (programme `W/t_str.rose`, `coup()` équivalent à `BASE` de `emetteur.rose`) :
```
$R rir W/t_str.rose > W/t_str.rir
$R compilateur/emetteur_elf.rose W/t_str.rir > W/t_str.elf     # rc=0, 1548 octets
chmod +x W/t_str.elf && W/t_str.elf                             # affiche 2, rc=0
$R W/t_str.rose                                                 # affiche 2
```

### 3.2 Runtime machine

- Nouveau `PT_LOAD` RW à `0x60000000` (1 Mio de BSS) ; le mot `[0x60000000]` est le pointeur libre.
- Allocation **en ligne** (`g5_alloc_inline`) : un `BL` interne écraserait `x30` (LR) de la fonction appelante.
- Helpers émis : `alloc`, `longueur` (tags 1/2), `ordre` (octet), `sous_chaine` (copie),
  `idx` (1 octet pour chaîne, élément pour liste), `joindre` (append avec copie si débordement),
  `entier` (décimal signé), stubs neutres.
- Encodages vérifiés avec `as`/`objdump` sur aarch64 (`strb` reg `0x38206800`, `ldrb` reg `0x38606800`,
  `movz` décalé `0xd2ac0009` pour `0x60000000`).
- ELF : 2 phdrs (`R E` + `RW`), `code_off = 176`, entrée `0x4000b0`, `e_shoff = 0`
  (pas de sections ni de symboles — inchangé).

### 3.3 Listes `lstvide`, `(lst …)`, `joindre`, `idx`

- `lstvide` et `(lst …)` construisent `[tag=2][len][cap][valeurs…]`.
- `joindre` ajoute en place tant que `len < cap`, sinon copie dans un(descripteur) plus grand.
- `inserer` est un **stub** (retourne la liste inchangée) : documenté comme tel.

### 3.4 Autres formes qui bloquaient la compilation du fichier entier

Dans l'ordre exact où les échecs ont été rencontrés et corrigés :

1. `G5 : forme parenthésée non couverte : s` → `(s …)` (§3.1).
2. `G5 : forme parenthésée non couverte : lstvide` → listes (§3.3).
3. `G5 : appel non résolu : nb_arguments` → primitives système neutres.
4. `Rose : erreur — index hors des limites de la liste` → 14 sauts dont la cible vaut
   `n_ops` (fin de fonction) : `g5_cible()` mappe ces cibles à un label de fin (`g5_endlbl`).
5. `G5 : forme parenthésée non couverte : f` → `(f 0.0)` replié en entier.
6. `G5 : unaire non couvert : -` → `(uni - …)` traité comme négation.
7. `G5 : op bin non couverte : ^` → exponentiation entière par boucle de multiplications.
8. `variable inconnue : nb_vars` (globaux du RIR de `emetteur_elf.rose`) → repliés à **0**
   (perte sémantique assumée).
9. `immédiat hors uint64 : 18446744073709551616` → repli modulo 2^64 (G5 reste i64 machine,
   les bigints exacts de Rose ne sont **pas** émulés).
10. `sous_chaine invalide : 0..-1 … <"))">` → `g4_operand` cherchait `affect slot ` **n'importe où**
    dans l'op et capturait le littéral `(s "affect slot ")` du propre RIR G5 ; le préfixe est
    maintenant testé au début de l'op.

## 4. Fonctions réellement compilées en ELF

### 4.1 Une fonction réelle de `emetteur.rose` : `BASE`

RIR extrait **tel quel** de `fpA.rir` (fonction `BASE`, 8 ops), avec un `$principal` synthétique
appelant `BASE("a|b")` et affichant sa longueur (`W/fn_base_driver.rir`) :

```
$R compilateur/emetteur_elf.rose W/fn_base_driver.rir > W/fn_base_driver.elf
# rc=0, 1532 octets ; chmod +x ; ./fn_base_driver.elf -> 1, rc=0
```

`BASE` coupe avant le premier `|`, donc `BASE("a|b") = "a"`, `longueur = 1`. ✅

### 4.2 Le fichier `emetteur.rose` entier

```
$R rir compilateur/emetteur.rose > W/fpA.rir                       # md5 b084ac5c…
$R compilateur/emetteur_elf.rose W/fpA.rir > W/final_emetteur.elf  # rc=0, 327336 octets
./rose/rose compilateur/emetteur_elf.rose W/fpA.rir > W/delivered_full.elf
# rc=0, 327336 octets (identique au binaire construit ici)
```

Comptage des fonctions par traçage (`W/count.rose`, copie de debug du fichier confié) :
**111 functions** émises (= 111 `fonction` dans le RIR).

```
file W/final_emetteur.elf
# ELF 64-bit LSB executable, ARM aarch64, statically linked, no section header
readelf -h  # Type EXEC, 2 program headers, Entry 0x4000b0
chmod +x W/final_emetteur.elf && W/final_emetteur.elf
# SIGSEGV, rc=139, 0 octet sur stdout/stderr
```

Cause observée sous gdb : `SIGSEGV` à `pc=0x44fc1c`, instruction `ldr x2,[x0,#8]`
(prologue de `joindre`) avec **`x0 = 1`** au lieu d'un descripteur de liste. Le flot de valeurs
et/ou l'ABI des appels à plus de 8 arguments n'est pas correct sur un programme de cette taille.

### 4.3 Auto-réflexivité (émission seulement)

```
$R rir compilateur/emetteur_elf.rose > W/final_self.rir     # 87 fonctions
$R compilateur/emetteur_elf.rose W/final_self.rir > W/final_self.elf
# rc=0, 183604 octets ; 87 functions tracées
W/final_self.elf                                            # SIGSEGV, rc=139
```

## 5. Chaîne G4/G5 rejouée (non-régression)

4 programmes, avec le binaire construit ici (et la voie `rose/rose` livrée vérifiée pour le point fixe) :

| Programme | RIR → ELF (octets) | Sortie ELF | rc ELF | Sortie Rose |
|---|---:|---|---:|---|
| `t_bin.rose` (`7+35`) | 1 196 | `42` | 0 | `42` |
| `t_fib.rose` (`fib(10)` récursif) | 1 368 | `55` | 0 | `55` |
| `t_sum.rose` (boucle) | 1 348 | `105` | 0 | `105` |
| `t_str.rose` (chaîne + `sous_chaine`) | 1 548 | `2` | 0 | `2` |

Point fixe M4.5 rejoué avec `./rose/rose` :
```
md5sum W/fpA.rir W/fpB.rir
# b084ac5c926f6ae486845f9bc96c616d  ×2 ; cmp_rc=0
```
`rose verif compilateur/emetteur_elf.rose` → **vérifié**.

Validation globale : `cargo test --release` (target dir isolé) →
**220 + 204 + 6 + 1 = 431 tests réussis, 0 échec**, 6 ignorés ;
`./scripts/rose-jardin.sh verif-tous` → **10/10 jardins valides** ;
`./rose/rose jardins/tests/premiers.rose` → **20 tests, 0 échec**.

## 6. Frontière exacte restante

1. **Sémantique des valeurs** : bigints exacts absents (modulo 2^64), flottants absents,
   `nb_arguments`/`argument`/`lire_texte`/`table`/`cle_existe`/`inserer` = stubs, globaux repliés à 0.
   31 appels sur 3 614 sont des stubs (`flottant 4`, `nb_arguments 1`, `lire_texte 2`,
   `argument 1`, `table 1`, `inserer 21`, `cle_existe 1`).
2. **ABI / valeur** : l'ELF de `emetteur.rose` segfault (`joindre` reçoit l'entier 1 au lieu d'un
   descripteur). L'émission réussit, l'exécution non.
3. **Writer** : deux `PT_LOAD`, toujours `e_shoff = 0`, aucune section, aucun symbole, pas de linker,
   pas de PE/Mach-O.
4. **Produit** : `rose/rose` exige toujours `cargo build --release`. Aucun des ELF émis n'est
   capable de reconstruire le produit. **0 % sans Rust.**

Ce qui a été franchi : **G4.1 (abaissement RIR) au niveau syntaxique sur 100 % des ops du
compilateur**, et l'exécution correcte d'une fonction réelle extraite de ce compilateur (`BASE`).
Ce qui ne l'est pas : G4.2 (runtime complet), G4.3 (writer complet), G4.4 (porte finale), G2/G3
(intacts). **Verdict : NON, toujours pas full hosted — mais la frontière est maintenant
« ELF produit mais non exécutable », au lieu de « émission refusée ».**

## 7. Rejouabilité

```
/tmp/opencode/se2_agents/target/release/rose          # binaire construit ici
/tmp/opencode/se2_agents/emetteur_elf.rose.BASE_G5   # copie du fichier avant modification
/tmp/opencode/se2_agents/{t_bin,t_fib,t_sum,t_str}.rose
/tmp/opencode/se2_agents/{fpA,fpB}.rir                # point fixe b084ac5c…
/tmp/opencode/se2_agents/{fn_base_driver,final_emetteur,final_self}.elf
/tmp/opencode/se2_agents/coverage3.py                 # analyseur 99,2 % / 100 %
/tmp/opencode/se2_agents/count.rose                   # copie de debug pour compter les fonctions
/tmp/opencode/se2_agents/val/                         # ELF de non-régression
```

Fichier livré modifié : `compilateur/emetteur_elf.rose`, md5 final
`1e881fe11e5b9fb8cb573cadf9936812` (initial `ec2c50c4a2e5b95db201ce55d6860f7e`,
+491 lignes / −14 lignes).
