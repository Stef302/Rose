# CORR1 — repli de constantes > 2^60 dans `moteur_complet.rose`

Correctif 1 de `rose/docs/HORDE_INTEGRATION.md` §6 (ligne 202). Worktree
exclusif `/tmp/opencode/corr_p1`, HEAD `d117e04eab2a7b9ef391123d575311820908ee2e`.

## Cause mesurée

`compilateur/moteur_complet.rose` est le moteur Rose auto-hébergé compilé par
C0 vers un ELF G5 aarch64. Sous G5, `entier` est **tagué** : un immédiat est
`n << 3`, soit un plafond de magnitude de 2^60 (le bit de signe occupe le
reste du mot 64 bits). Le repli de constantes hérité de `lecteur_source.rose`
calculait `b^e` et les sommes/produits de littéraux directement dans des
variables `entier` :

- `lsrc_rpui($b: entier, $e: entier) -> entier` (`moteur_complet.rose:551`) :
  `$res = $res * $f` en boucle ;
- `lsrc_replier_expr` (`moteur_complet.rose:1587` avant correctif) :
  `$p1 = lsrc_rnum($ta)`, `$np = $p1 * $q2 + $p2 * $q1`, `$num = $num * $p1`,
  puis `chaine($np)` / `lsrc_rnorm($np, $nq)`.

Dès que le résultat dépasse 2^60, le tag G5 tronque **avant l'exécution** :
le littéral injecté dans le RIR est déjà faux. Le module `bigint_rose.rose`
(déjà composé dans le moteur) fournit bien `bi_pow`/`bi_add`/`bi_mul`, mais
n'était pas branché dans le repli.

Le moteur de référence Rust (`rose/src/front.rs`, `fn replier`) utilise
`num_bigint::BigInt` : ses résultats sont exacts. D'où la divergence ELF vs
référence, pas une divergence de l'interpréteur.

## Correctif appliqué

Fichiers touchés (aucun autre) :

| Fichier | Changement |
|---|---|
| `compilateur/moteur_complet.rose` | `lsrc_rpui` retypé `(chaine, entier) -> chaine` et branché sur `bi_pow` ; nouveaux helpers `lsrc_rpetit`, `lsrc_rpiece`, `lsrc_rplie` ; `lsrc_replier_expr` utilise des chaînes décimales + `bi_add`/`bi_sub`/`bi_mul`/`bi_div`/`bi_pow`/`bi_gcd`. |
| `compilateur/tests_corr1.rose` | nouveau : 81 expressions deterministes (dont 50 puissances). |
| `rose/docs/CORR1_CONSTANTES.md` | ce document. |

Détail :

1. `lsrc_rpui($b: chaine, $e: entier) -> chaine` :
   `bi_texte(bi_pow(bi_depuis_chaine($b), $e))`.
2. `lsrc_rpiece($t, $k)` extrait numérateur/dénominateur d'un littéral
   `p/q` **sans conversion vers `entier`** (dénominateur implicite `1`).
3. `lsrc_rpetit($t)` valide l'exposant comme petit entier décimal (0..64),
   miroir de `parse::<i64>()` côté Rust ; un exposant géant reste non replié.
4. `lsrc_rplie(...)` calcule `(n1/d1) OP (n2/d2)` en bigint, normalise le
   signe du dénominateur, réduit par `bi_gcd`, et rend `p` ou `p/q`.
   Les limbs 10^8 de `bigint_rose.rose` gardent `$x * q + retenue < 2^60`,
   donc aucun débordement de tag G5.
5. `lsrc_replier_expr` ne convertit plus jamais un grand littéral en
   `entier` : `lsrc_contient_barre($r)` décide le drapeau `q`/`""`.

Le point d'entrée d'origine (`rose/src/front.rs`) reste inchangé : le moteur
Rose compilé reproduit maintenant exactement ses décisions de repli.

## Chiffres AVANT (ELF G5 compilé depuis l'état d'origine)

ELF avant : `/tmp/opencode/corr_p1_art/moteur_complet.elf`, 1 482 232 octets,
md5 `07ac6d0bff9cc1be7792a9a4b8a8996e`. Il a été obtenu par la procédure
`build_moteur_sans_rust.sh` du document HORDE_INTEGRATION (composition des
modules + C0) ; la section de repli y est octet pour octet identique à celle
du fichier `d117e04`, seule la queue des modules a grandi depuis la mesure
originale (10 769 lignes au lieu de 10 001).

Batterie `compilateur/tests_corr1.rose` (81 expressions), référence
`ROSE_SANS_JIT=1 rose/rose` vs ELF `moteur_complet.elf` (`cmp` octet pour
octet) : **61 divergences / 81**, 20 conformes.

| Expression | Référence | ELF avant |
|---|---:|---:|
| `3^41` | 36472996377170786403 | 155968982055106659 |
| `10^21` | 1000000000000000000000 | 128825129711960064 |
| `5^28` | 37252902984619140625 | 71184461048325649 |
| `4^33` | 73786976294838206464 | 0 |
| `(10^10)^2` | 100000000000000000000 | 272289851507736576 |
| `-5 + 12` | 7 | 288230376151711751 |
| `2^59 - 1` | 576460752303423487 | -1 |
| `2^59` | 576460752303423488 | 0 |
| `2^59 + 1` | 576460752303423489 | 1 |
| `(2^59 - 1) + 1` | 576460752303423488 | 288230376151711744 |
| `2^60` | 1152921504606846976 | 0 |
| `2^63` | 9223372036854775808 | 0 |
| `2^64` | 18446744073709551616 | 0 |
| `2^64 + 1` | 18446744073709551617 | 1 |
| `2^64 + 2^64` | 36893488147419103232 | 0 |
| `123456789012345678901234567890 + 987654321098765432109876543210` | 1111111110111111111011111111100 | 516230772886440380 |

Sources originales, toutes **DIVERGENTES** avant :
`t20_add` (1 ligne sur 4), `t220_depassement_2_59` (5/5),
`t221_depassement_2_60_63` (6/6), `t222_depassement_2_64` (3/5),
`t223_decimal_geant` (1/3), `g5b521_puissance_bases_foldables` (5/5).

## Contrôle intermédiaire (bigint sous G5, avant compilation du moteur complet)

`compilateur/bigint_rose.rose` + les 5 helpers + un pilote de 12 appels,
compilés séparément par C0 (`/tmp/opencode/corr_p1_scratch/bi_test.elf`) :
sortie G5 **identique** à la sortie de `rose/rose` sur le même fichier
(`diff` vide, 12/12), y compris `3^41`, `2^64`, `(-3)^41`, `-5 + 12`,
`2^62 + 2^62`, l'addition de deux décimaux de 30 chiffres et `(-10) div 3`.

## Chiffres APRÈS (ELF G5 corrigé)

ELF : `/tmp/opencode/corr_p1_art_fix/moteur_complet.elf`, 1 408 528 octets,
magic `7f454c46`, machine `183` (aarch64), md5 `9125baa66c5c7d2f2a96ab0a1189ec67`.
C0 inchangé : `/tmp/opencode/j6_agents/C0_j6.elf`, md5
`7bb091200cf927022eb9d86a548d2502`.

Batterie `compilateur/tests_corr1.rose` (81 expressions), même référence :
**0 divergence / 81** (`cmp -s` octet pour octet).

| Expression | Référence | ELF après |
|---|---:|---:|
| `3^41` | 36472996377170786403 | 36472996377170786403 |
| `10^21` | 1000000000000000000000 | 1000000000000000000000 |
| `5^28` | 37252902984619140625 | 37252902984619140625 |
| `4^33` | 73786976294838206464 | 73786976294838206464 |
| `(10^10)^2` | 100000000000000000000 | 100000000000000000000 |
| `-5 + 12` | 7 | 7 |
| `2^59 - 1` | 576460752303423487 | 576460752303423487 |
| `2^59` | 576460752303423488 | 576460752303423488 |
| `2^59 + 1` | 576460752303423489 | 576460752303423489 |
| `2^60` | 1152921504606846976 | 1152921504606846976 |
| `2^63` | 9223372036854775808 | 9223372036854775808 |
| `2^64` | 18446744073709551616 | 18446744073709551616 |
| `2^64 + 1` | 18446744073709551617 | 18446744073709551617 |
| `2^64 + 2^64` | 36893488147419103232 | 36893488147419103232 |
| `123456789012345678901234567890 + 987654321098765432109876543210` | 1111111110111111111011111111100 | 1111111110111111111011111111100 |

Sources originales, toutes **CONFORMES** après : `t20_add`,
`t220_depassement_2_59`, `t221_depassement_2_60_63`,
`t222_depassement_2_64`, `t223_decimal_geant`,
`g5b521_puissance_bases_foldables`.

Bilan sur les 141 sources de `tests_selfhost` (mêmes ELF avant/après,
référence `ROSE_SANS_JIT=1 rose/rose`, `timeout 20` par programme) :

| | CONFORME | DIVERGENT | BLOQUÉ |
|---|---:|---:|---:|
| Avant | 103 | 28 | 10 |
| Après | **130** | **1** | 10 |

27 des 28 divergences disparaissent, dont toutes celles de repli entier
(`t220`–`t223`, `t20_add`, `g5b521`, `t224`, `t226`, `t22_mul`, `t23_div`,
`t32_big_2_63`, `t33_big_2_64`, …). La seule restante est `t09_flottant`
(correctif 3 de HORDE_INTEGRATION §6 : repli flottant, hors périmètre de ce
correctif). Les 10 bloquées sont inchangées et déjà documentées ailleurs
(`t104`–`t107`, `t14_type`, `t55`, `t81`, `t82`, `t91`, `t999`).

## Validation

Exécutées dans `/tmp/opencode/corr_p1` sous garde-fou 12 000 000 Ko :

| Contrôle | Commande | Résultat |
|---|---|---|
| Vérification du composé | `rose/rose verif compilateur/moteur_complet.rose` | `vérifié`, rc=0 |
| Vérification de la batterie | `rose/rose verif compilateur/tests_corr1.rose` | `vérifié`, rc=0 |
| Compilation C0 → ELF | `C0_j6.elf compilateur/moteur_complet.rose > moteur_complet.elf` | rc=0, 1 408 528 octets, ELF aarch64 |
| Tests comparatifs | `compare.sh` (81 expressions + 6 sources) | 0 divergence, 6/6 CONFORME |
| Point fixe | `rose/rose rir compilateur/emetteur.rose \| md5sum` (×2) | `b3ed9262dd54776a6dbd07bc12368d0e` sur les deux passes, `cmp` égal |
| Validation finale | `RUST_TEST_THREADS=1 CARGO_BUILD_JOBS=2 bash scripts/validation_finale.sh` | **RC=0, 0 échec** : tests 314+327+0, point fixe, AOT 75/75, JIT 25/25, WASM 92 o, RVM 317811, AZ 31 conformes, HOST-1 fib=317811 |

Le moteur Rose écrit en Rose, exécuté sous l'interpréteur Rust sur
`tests_corr1.rose`, donne également 0 divergence / 81 (contrôle de la logique
de repli indépendamment du tag G5).
