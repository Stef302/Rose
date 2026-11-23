# FINAL_T3 — front-end : surcharges et fonctions d'ordre supérieur

**Constat daté : 23/11/2026.** Worktree exclusif `/tmp/opencode/final_t3`,
HEAD `b3a4173317b52546b615b50a9c4e7e7b86dc0c00`. Aucun commit, aucun push.
Binaire oracle `rose/rose` = `/home/clausake/Stefan/ROSE/rose/rose`, md5
`7614a534b9411917ef9cd49a834014a8` (identique à celui des chantiers
HORDE_SOURCE / CORR6).

Fichiers possédés, et les seuls écrits :

- `compilateur/lecteur_source.rose` (**uniquement** les règles de vérification
  des surcharges/HOF et l'émission des formes correspondantes ; les alias
  `type`/`prng` du correctif 4 et le repli de constantes sont intacts) ;
- `compilateur/tests_final_t3.rose` (nouveau) ;
- `rose/docs/FINAL_T3.md` (ce document).

**Interdits non touchés** (vérifié par `git status --short`) :
`compilateur/emetteur.rose`, `compilateur/interpreteur_rir.rose`,
`compilateur/verifieur.rose`, `rose/src/**`.

---

## 1. Reproduction des 4 refus (avant correction)

Journal intégral : `/tmp/opencode/final_t3_ev/00_reproduction.txt`.
Le compilateur C0 lui-même (`/tmp/opencode/C0.elf`, source → RIR) refuse les
quatre sources avec les mêmes diagnostics que le lecteur Rose ; journal
`/tmp/opencode/final_t3_ev/08_C0_refus.txt`.

| Source | `rose rir <src>` (oracle Rust) | `rose compilateur/lecteur_source.rose <src>` |
|---|---|---|
| `t104_surcharge_arith` | rc=1 `Fonction 'f' définie deux fois` (l.2) | `ERR\|… Argument de f: chaine reçu, entier attendu` (l.4) |
| `t105_surcharge_arite` | rc=1 `Fonction 'f' définie deux fois` (l.2) | `ERR\|… f attend 1 argument(s), reçu 2` (l.4) |
| `t106_hof_param` | rc=1 `Fonction 'fn' inconnue` (l.1) | `ERR\|… Fonction 'fn' inconnue` (l.1) |
| `t107_hof_var` | rc=1 `Variable 'carre' utilisée sans valeur` (l.2) | `ERR\|… Variable 'carre' utilisée sans valeur` (l.2) |

Les deux moteurs refusent : le Rust dès la déclaration (une seule entrée par
nom de `fn`), le lecteur Rose à l'appel (il garde les deux définitions mais
`check_appel` ne consulte que la première). C'est exactement le point
**HORDE_INTEGRATION §6.6** : le correctif 6 a doté l'interpréteur RIR des
formes `appel_rir` / `surcharge` / `fn`, mais le front-end ne les émettait
pas.

---

## 2. Correction apportée à la copie du front-end

Tout est dans `compilateur/lecteur_source.rose` (copie générée de
`emetteur.rose`, préfixée `lsrc_`). Quatre points, 237 lignes ajoutées :

1. **`ce_raw`, branche `v`** — un nom de `fn` employé comme expression vaut
   désormais `fonction|vide` au lieu de déclencher
   `Variable '…' utilisée sans valeur`. C'est la valeur de première classe
   qui manquait pour `$f = carre` et `applique(carre, 5)`.
2. **`check_appel`** —
   - collecte **toutes** les définitions d'un même nom ; quand il y en a
     plusieurs et qu'aucune ne correspond à l'appel, il choisit celle dont
     l'arité **et** les types d'arguments conviennent. Les essais utilisent
     une liste d'erreurs jetable, si bien que le diagnostic réel n'est
     produit que si aucune surcharge ne convient ;
   - un appel dont le calleur est une variable ou un paramètre de type
     `fonction` (ou Dyn) est accepté et typé `?|vide`.
3. **`check_instr`, `retour`** — une expression de type Dyn est acceptée en
   retour d'une fonction déclarée (cas du corps `$fn($x)`).
4. **Émission RIR** — les nœuds concernés sont marqués dans un pool parallèle
   `$tx2` (les listes Rose sont en ajout seul) puis rendus sous les formes
   qu'exécute l'interpréteur CORR6 :
   - surcharge → noms manglés `f@<arit>` et, si plusieurs surcharges partagent
     une arité, `f@<arit>@<codes-de-types>` (`e c b l r f t n v`) ; appel
     `(surcharge f …)` ;
   - fonction-valeur → `(fn carre)` (y compris dans `affect slot … = …`) ;
   - appel dynamique → `(appel_rir (v fn) slot …)`.

Deux contraintes ont guidé l'écriture : aucun programme existant n'a deux
`fn` de même nom et aucun ne nomme une fonction comme valeur (vérifié sur les
166 sources du corpus) ; les compteurs et le manglage utilisent des listes
parallèles et non une `table`, car une lecture de table est typée `valeur`
(`?`) et `? > entier` est refusé par le backend G5 (mesuré : premier essai
C0 `Ordre valeur > entier impossible`, l.4401).

---

## 3. Tests

### 3.1 Banc auto-hébergé

```
$ ./rose/rose verif compilateur/tests_final_t3.rose          -> vérifié (rc=0)
$ ./rose/rose compilateur/tests_final_t3.rose
TESTS FINAL_T3 : 21 OK, 0 ECHEC                              (rc=0)
```

Le banc (20 programmes + 1 contrôle négatif) compare octet pour octet le RIR
rendu par `lire_programme_reg` (registre de sources, aucun disque) aux 4
sources historiques et à 16 variantes : surcharges par type (2/3 types),
par arité (1/2/3), mixtes arité+type, `liste<entier>`, `flottant`, `booleen`,
paramètres-fonctions (1 ou 2 appels), composition, variables-fonctions (une
ou deux, y compris affectation chaînée `$g = $f`). Le contrôle négatif
vérifie qu'un nom réellement inconnu reste refusé — la relaxation n'est pas
globale.

### 3.2 Exécution des RIR émis par l'interpréteur CORR6

ELF `/tmp/opencode/corr6_interp_final.elf` (215 144 o, correctif 6). Journal
`/tmp/opencode/final_t3_ev/05_execution.txt` :

```
RESULTAT interpréteur CORR6 : OK=20 ECHEC=0
```

Valeurs : `t104` `3|a!`, `t105` `3|5`, `t106` `25`, `t107` `25`, et les 16
variantes `6|a?`, `2|3|6`, `17|x?|b`, `6|b`, `4|2.5`, `4|3`, `12|-x5`,
`2|a!|5`, `61`, `10`, `8`, `9|8`, `6`, `17`, `36`, `12`. Aucun `rc` d'erreur.

### 3.3 Non-régression — lecture avant/après (166 sources)

Méthode : lecteur sauvegardé avant correction (`lecteur_source.avant.rose`)
et lecteur corrigé, 141 `tests_selfhost/*.rose` + 25
`rose/tests_aot8/programmes/*.rose`, sortie comparée octet pour octet.
Journaux `01_avant_apres.txt` et `04_avant_apres.txt` :

```
AVANT/APRES : identiques=160 divergentes=6
```

Les 6 divergences sont exactement : les 4 sources cibles (désormais
acceptées — attendu) et 2 sources (`g5b606_rat_litteraux`,
`t242_rat_litteraux_grands`) qui échouent **déjà** des deux côtés avec le
même `conversion vers entier impossible (type Vide, val="")`, seul le
numéro de ligne du lecteur ayant bougé de 2576 à 2641 (ajout de code). Aucun
programme accepté n'a vu son dump changer.

### 3.4 Comparaison à `rose/rose` — 20 programmes similaires

Journal `02_oracle20.txt` / `04_oracle20.txt` :

```
/oracle 20 programmes : OK=19 BAD=1 SKIP=0
```

Les 20 programmes de la liste de non-régression du correctif 6
(`01_arith` … `25_racine_puissance`) sont tous acceptés par l'oracle ; 19
RIR sont identiques octet pour octet. Le seul écart, `24_compose`, est
**préexistant** : le lecteur sauvegardé avant correction rend déjà
`ERR|… Addition valeur + chaine impossible` (cause documentée dans
`SELFHOST_EXTENSION.md` §3, correctif dans le composite injecté, hors
périmètre). **0 refus supplémentaire.**

---

## 4. Validation

Toutes les commandes ont été exécutées dans le worktree, sous
`bash scripts/garde_fou_mem.sh 12000000 …` pour les longues.

| Contrôle | Commande | Résultat mesuré |
|---|---|---|
| Vérification | `./rose/rose verif compilateur/lecteur_source.rose` | `vérifié`, rc=0 |
| Banc | `./rose/rose compilateur/tests_final_t3.rose` | `21 OK, 0 ECHEC`, rc=0 |
| **C0 → RIR** | `/tmp/opencode/C0.elf compilateur/lecteur_source.rose` | rc=0, **287 650 o**, magic `rir`, stderr 0 |
| **RIR → ELF** | `/tmp/opencode/j4_P0.elf lecteur_c0.rir` | rc=0, **759 880 o**, `ELF 64-bit LSB executable, ARM aarch64, statically linked` |
| Lecteur autonome | ELF P0 × 21 sources | `OK=21 BAD=0`, sortie identique au lecteur interprété par la graine |
| **Point fixe** | `./rose/rose rir compilateur/emetteur.rose` **et** `./rose/rose compilateur/emetteur.rose compilateur/emetteur.rose` | **`b3ed9262dd54776a6dbd07bc12368d0e` ×2**, `cmp` = 0 |
| `validation_finale.sh` | `RUST_TEST_THREADS=1 bash scripts/garde_fou_mem.sh 12000000 bash scripts/validation_finale.sh` | **RC=0, `RESULTAT: 0 échec(s)`** : 314+327 tests, point fixe OK, AOT 75/75, JIT 25/25, WASM 92 o, RVM 317811, AZ 31 CONFORME/0 non-conforme, HOST-1 fib=317811 |
| Arbre | `git status --short` | `M compilateur/lecteur_source.rose`, `?? compilateur/tests_final_t3.rose` uniquement |

Journal de validation finale conservé dans
`/tmp/opencode/final_t3_ev/07_validation_finale.out` (le log global
`/tmp/opencode/validation_finale.log` est partagé entre agents).

---

## 5. Rejouer

```bash
cd /tmp/opencode/final_t3
R=./rose/rose
D=/tmp/opencode/final_t3_ev/replay
mkdir -p "$D"

# banc + 20 dumps + ATTENDUS
$R compilateur/tests_final_t3.rose

# 4 sources : lecture, exécution RIR
for f in t104_surcharge_arith t105_surcharge_arite t106_hof_param t107_hof_var; do
  $R compilateur/lecteur_source.rose tests_selfhost/$f.rose > "$D/$f.raw"
  sed 's/^OK|//' "$D/$f.raw" > "$D/$f.rir"
  /tmp/opencode/corr6_interp_final.elf "$D/$f.rir"
done

# chaîne sans Rust
bash scripts/garde_fou_mem.sh 12000000 /tmp/opencode/C0.elf \
  compilateur/lecteur_source.rose > "$D/lecteur.rir"
bash scripts/garde_fou_mem.sh 12000000 /tmp/opencode/j4_P0.elf \
  "$D/lecteur.rir" > "$D/lecteur.elf"

# point fixe
$R rir compilateur/emetteur.rose | md5sum
$R compilateur/emetteur.rose compilateur/emetteur.rose | md5sum
```

---

## 6. Ce qui reste hors de ce chantier

Le moteur Rust (`rose/src/**`, interdit ici) refuse toujours les 4 sources à
la déclaration (`Fonction 'f' définie deux fois`, etc.) : ce correctif porte
sur la **copie du front-end** `lecteur_source.rose`, qui peut désormais les
lire et produire un RIR exécutable par l'interpréteur du correctif 6. Le
périmètre « remplacer le binaire de référence » reste celui décrit dans
`HORDE_INTEGRATION.md` §6.

Toutes les mesures ci-dessus sont des constats datés du 23/11/2026 dans ce
worktree ; à re-mesurer après tout changement de C0, de P0, de la graine Rust
ou de la topologie.
