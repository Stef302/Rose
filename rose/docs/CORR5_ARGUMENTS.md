# CORR5 — Séparation des arguments (constat mesuré, 23/11/2026)

Sous-agent correctif 5. Worktree exclusif `/tmp/opencode/corr_p5`, HEAD de
départ `d117e04`. **Deux fichiers source modifiés/créés, aucun commit, aucun
push** :

| Fichier | Rôle |
|---|---|
| `compilateur/moteur_complet.rose` | **deux régions seulement** : `call_fn` (~:5453) et le point d'entrée unique (~:9982) |
| `compilateur/tests_corr5.rose` | nouveau — suite de 20 contrôles, une ligne de sortie par contrôle |
| `rose/docs/CORR5_ARGUMENTS.md` | ce document |

Garde-fou sur TOUTE commande longue :
`bash scripts/garde_fou_mem.sh 12000000 <commande>`.

Référence de comportement : le binaire Rust `rose/rose` (version 0.1.73,
copié depuis le dépôt principal dans le worktree ; binaire gitignoré, absent
du dépôt). Compilateur self-host : `/tmp/opencode/j6_agents/C0_j6.elf`
(md5 `7bb091200cf927022eb9d86a548d2502`).

---

## 1. Mesure initiale — le défaut

Chaîne reproduite : `C0_j6.elf compilateur/moteur_complet.rose >
moteur_complet.elf` (5 min 25,91 s, pic RSS < 12 Go, ELF aarch64 de
1 404 724 octets, magic `7f454c46`, machine `183`), puis
`./moteur_complet.elf SOURCE.rose [args…]`.

Fichier d'entrée du banc : `/tmp/opencode/sh9_agents/battery/critique_entree.txt`
(31 octets, `fonction principale : test SH9\n`).

| Programme | `rose/rose` (attendu) | ELF avant correctif |
|---|---|---|
| `t81_argument.rose` + 1 arg | `/tmp/…/critique_entree.txt` (rc=0) | `tests_selfhost/t81_argument.rose` (**chemin source**, rc=0) |
| `t82_argument_hors.rose`, 0 arg | rc=1, stderr `Rose : erreur — index hors des limites des arguments` | rc=0, stdout = chemin source |
| `t999_critique_g5.rose` + 1 arg | `1 / 31 / vrai / principale : test SH9 / …` (20 lignes) | `1 / 1705 / faux / rapide : le runtime G5…` (le fichier SOURCE est lu au lieu de l'entrée) |

Comportement attendu de `rose/rose`, mesuré préalablement :

```
>> nb_arguments()                 # rose prog.rose  → 0 ; +1 arg → 1 ; +3 args → 3
>> argument(0)                    # 0 arg  → rc=1 « index hors des limites des arguments »
                                  # 2 args → « PREMIER »
```

Donc `argument(0)` = **premier argument utilisateur**, index 0 excluant le
chemin du programme ; `nb_arguments()` = nombre d'arguments utilisateurs.

**Cause** : le point d'entrée lit la source avec `argument(0)` (hôte), qui est
donc le chemin source. La primitive `argument` du moteur self-host
(`call_prim`, `moteur_complet.rose:5225`) lit la table de l'hôte **sans
décalage** : `argument(0)` y rend le chemin source au lieu de l'argument
utilisateur. `nb_arguments()` y est déjà décalé correctement
(`nb_arguments() - 1`, :5226) — d'où l'incohérence : le compte est juste,
l'index est faux.

---

## 2. Correctif — deux régions, zéro autre fichier

### 2.1 Point d'entrée (après `parse_rir`, avant `call_fn`)

1. `joindre($fn_nom, "argument")` : enregistre une entrée « argument » dans
   la table des fonctions de l'interpréteur. `eval_expr` teste
   `find_fn($name) > 0` avant d'appeler `call_prim` ; avec cette entrée,
   les appels utilisateur à `argument` sont routés vers `call_fn`.
   Les fonctions **définies par l'utilisateur** restent prioritaires : elles
   sont déjà dans `$fn_nom` (rempli par `parse_rir`) et `find_fn` rend le
   premier match — l'entrée ajoutée est la dernière.
2. Construction de `$mc_args` à partir des arguments hôtes **1..N-1**
   (`nb_arguments()` hôte inclut le chemin) puis passage à
   `call_fn($mc_mem, "$principal", $mc_args)`. L'index hôte 0 reste
   exclusivement réservé à la lecture de la source.

### 2.2 `call_fn` — décalage à l'exécution

En tête de `call_fn`, avant `find_fn` :

```
si ($name == "argument") {
  si (longueur($args) < 2) { retour pack_ret(vs(""), $m) }
  $ix = entier(val_str($args[1]))
  si ($ix < 0) { $ix = nb_arguments() } sinon { $ix = $ix + 1 }
  retour pack_ret(vs(argument($ix)), $m)
}
```

- `argument(k)` utilisateur → `argument(k+1)` hôte : saute le chemin source.
- Le décalage se fait **à l'exécution**, donc il couvre les indices
  littéraux **et dynamiques** (`argument($i)`, `argument(1+1-1)`,
  `argument($i)` depuis une fonction) — un réécrivain de RIR n'aurait couvert
  que les littéraux.
- `k < 0` force un index hors table (`nb_arguments()` hôte) : l'hôte émet
  exactement le message d'erreur et le rc de `rose/rose`, sans jamais
  retomber sur l'index 0 (le chemin).
- `call_prim` (`moteur_complet.rose:5225`) est **inchangé** : aucune autre
  primitive n'est touchée.

`verif` des deux sources : `vérifié` (rc=0) des deux côtés.

---

## 3. Preuves d'exécution

### 3.1 Compilation C0 → ELF (après correctif)

`bash scripts/garde_fou_mem.sh 12000000 C0_j6.elf compilateur/moteur_complet.rose
> /tmp/opencode/corr_p5_art/fix/moteur_complet.elf`

rc=0, 5 min 46,23 s, stderr vide, ELF de **1 406 200 octets**, magic
`7f 45 4c 46 02 01 01 00` (ELF64 aarch64).

### 3.2 Les trois programmes du mission

| Programme | `rose/rose` | ELF corrigé | Résultat |
|---|---|---|---|
| t81 + 1 arg | rc=0, `/tmp/…/critique_entree.txt` | identique | **CONFORME** |
| t82, 0 arg | rc=1, `index hors des limites…` | identique | **CONFORME** |
| t999 + 1 arg | 20 lignes | 20 lignes, 19 identiques | voir note ci-dessous |

Note (préexistant, **hors périmètre CORR5**) : sur `t999`, la ligne 9
(`chaine(flottant("2"))`) rend `2` côté `rose/rose` et `2/1` côté ELF. Cette
divergence est **déjà présente dans l'ELF de référence avant correctif**
(vérifié : `/tmp/opencode/corr_p5_art/base/t999.out`, ligne 9 = `2/1`) ; elle
relève du défaut n°3 de `HORDE_INTEGRATION.md` (formatage flottant), pas de
la séparation des arguments. Toutes les lignes dépendant de `argument(0)` et
`nb_arguments()` sont désormais conformes (`1`, `31`, `vrai`,
`principale : test SH9`).

### 3.3 Vingt programmes distincts, avec et sans arguments

Vingt sources générées dans `/tmp/opencode/corr_p5_art/progs/` (hors dépôt),
comparées octet pour octet (stdout + stderr + rc) entre `rose/rose` et
l'ELF corrigé :

- **0 arg** : t01, t13, t16, t19 ;
- **1 arg** : t02, t08, t11, t12, t14, t17, t18, t20 ;
- **2 args** : t03, t09 ;
- **3 args** : t04, t05, t06, t07, t10, t15.

Couverture : `nb_arguments()` (sommet + fonction), `argument(0/1/2)` littéral,
index dynamique (variable, boucle, expression arithmétique, paramètre de
fonction), `argument` dans une fonction, `lire_texte(argument(0))`,
comparaisons, concaténation, absence de fuite du chemin source.

**Résultat : 20/20 conformes, 0 divergence.**

Cas d'erreur ajoutés (hors-limites, rc et stderr identiques des deux côtés) :
`argument(0)` sans arg, `argument(1)` avec 1 arg, `argument(2)` avec 2 args,
`argument(-1)` avec 1 arg → 4/4 conformes, rc=1,
`Rose : erreur — index hors des limites des arguments`.

### 3.4 Suite permanente `compilateur/tests_corr5.rose`

20 lignes `>>`, exécutée sous 5 configurations (0, 1, 2, 3, 5 arguments) :
**5/5 conformes, 0 divergence**, 20 lignes produites à chaque exécution
(marqueur `FIN_CORR5` en ligne 20).

### 3.5 Non-régression — 141 sources self-host

Même ELF avant et après correctif, sur les 141 programmes de
`tests_selfhost/*.rose` : **3 changements exactement**, tous attendus :

```
DIFF t81_argument        rc=0/0     (chemin source → argument utilisateur)
DIFF t82_argument_hors   rc=0/1     (chemin source → erreur hors-limites)
DIFF t999_critique_g5    rc=0/0     (source lue → fichier d'entrée lu)
```

Aucune autre source ne change, y compris `t80_nb_arguments.rose`.

---

## 4. Validation complète

| Contrôle | Commande | Résultat |
|---|---|---|
| Vérification des deux sources | `rose/rose verif …` | `vérifié`, rc=0 ×2 |
| C0 → ELF | `C0_j6.elf moteur_complet.rose` | rc=0, ELF 1 406 200 o, magic aarch64 |
| 20 programmes + 5 configs + 4 erreurs | comparaison `cmp` ref/ELF | **0 divergence** |
| Non-régression 141 sources | base vs fix | 3 changements attendus, 0 autre |
| Point fixe émetteur | `rir emetteur.rose` ×2 | `b3ed9262dd54776a6dbd07bc12368d0e` ×2 |
| `validation_finale.sh` | `RUST_TEST_THREADS=1 bash scripts/validation_finale.sh` | **RC=0, 0 échec** |

Détail de `validation_finale.sh` (`/tmp/opencode/corr_p5_art/validation_finale.log`) :

```
[15:51:03] === 1. build + tests Rust ===
    Finished `release` profile [optimized] target(s) in 1m 39s
test result: ok. 314 passed; 0 failed; 3 ignored
test result: ok. 327 passed; 0 failed; 3 ignored
[15:54:14] OK   tests rc=0
[15:55:37] md5: b3ed9262… / b3ed9262…  → OK point fixe
[15:55:37] TOTAL: 75 OK, 0 ECHEC  → OK AOT 75/75
[15:55:46] 25/25  → OK JIT
[15:55:46] OK WASM 92 o ; OK RVM 317811
[15:57:58] CONFORME 31  → OK AZ 31 conforme, 0 non-conforme
[15:58:11] OK HOST-1 fib=317811
[15:58:11] === RESULTAT: 0 échec(s) ===
VF_RC=0
```

---

## 5. Point d'attention pour l'agent parent

`scripts/build_moteur_sans_rust.sh` **régénère** `compilateur/moteur_complet.rose`
depuis les modules avec son propre `pilote` (le point d'entrée) — mais il
n'écrit PAS le correctif de `call_fn` (qui est dans le module
`interpreteur_rir.rose`, possédé par un autre agent). Une exécution future de
ce script écraserait donc le correctif. Ce script était **interdit** par la
mission (tout autre fichier), il n'a pas été modifié. Pour pérenniser, il
faudra soit porter le décalage dans `interpreteur_rir.rose` + le `pilote` du
générateur, soit figer le composite corrigé comme source de référence.

Artefacts hors dépôt : `/tmp/opencode/corr_p5_art/base/moteur_complet.elf`
(baseline), `/tmp/opencode/corr_p5_art/fix/moteur_complet.elf` (corrigé),
`/tmp/opencode/corr_p5_art/validation_finale.log`,
`/tmp/opencode/corr_p5_art/progs_out/` et `/tmp/opencode/corr_p5_art/tests/`.
