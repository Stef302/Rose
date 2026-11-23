# CORR2 — rationnels négatifs dans l'interpréteur RIR Rose

Correctif 2 de `rose/docs/HORDE_INTEGRATION.md` §6, ligne 209-214 :
« Arithmétique rationnelle négative — `vf` (`interpreteur_rir.rose:574`) et
`val_str` (`:482`). `-7/3` rend un entier 64 bits tronqué puis une décimale
de plusieurs milliers de chiffres au lieu de `-3.[3]` (`t07`, `g5b606`,
`t242`, `t243`). Cause : la normalisation de signe du numérateur et la
représentation `r:<num>/<den>` ne sont pas appliquées quand le littéral est
replié en négatif. »

Worktree exclusif `/tmp/opencode/corr_p2`, HEAD de départ
`d117e04eab2a7b9ef391123d575311820908ee2e`. **Trois fichiers possédés, aucun
autre modifié, aucun commit, aucun push** :

| Fichier | Rôle |
|---|---|
| `compilateur/interpreteur_rir.rose` | correctif dans `val_str` (l.482) et `vf` (l.574) uniquement |
| `compilateur/tests_corr2.rose` | nouveau — 50 rationnels négatifs/zéro/positifs |
| `rose/docs/CORR2_RATIONNELS.md` | ce document |

Garde-fou sur TOUTE commande longue :
`bash scripts/garde_fou_mem.sh 12000000 <commande>`.

---

## 1. Mesure de l'état initial (AVANT)

Référence `ROSE_SANS_JIT=1 rose/rose` sur les quatre sources citées :

| Source | Référence |
|---|---|
| `t07_rationnel_neg` | `-3.[3]` / `-3.[3]` / `-3.[3]` |
| `g5b606_rat_litteraux` | `14285714285714285714.[285714]`, `6148914691236517205.[3]`, `-14285714285714285715.[285714]`, `10` |
| `t242_rat_litteraux_grands` | identique à g5b606 |
| `t243_rat_somme_calcul` | `1`, `33333333333333333333.[6]` |

Moteur complet auto-hébergé compilé par C0 depuis l'état de départ
(`/tmp/opencode/corr2_work/moteur_complet.elf`, 1 404 724 octets) :

| Source | ELF AVANT |
|---|---|
| `t07` | `96076792050570579`, puis une décimale de **plus de 3 000 chiffres** commençant `0.00000000000000002428…`, puis `96076792050570579` |
| `g5b606` / `t242` | `38898550215390939.[428571]`, `0`, `2277217806282166.[857142]`, puis une décimale de plusieurs milliers de chiffres |
| `t243` | `1`, `90763283835912192.[3]` |

Cause mesurée en deux temps :

1. Le front-end composé (`lecteur_source.rose`, fonction `lsrc_replier_expr`)
   replie `-7/3` dans un `entier` tagué G5 : le dump RIR intermédiaire produit
   `(f 7/288230376151711741)` au lieu de `(f -7/3)` (mesuré via un pilote
   d'affichage du RIR). Ce volet « repli 64 bits » est le **correctif 1**
   (`CORR1_CONSTANTES.md`), déjà appliqué dans le worktree `corr_p1`.
2. Une fois le repli corrigé (ELF `/tmp/opencode/corr_p1_art_fix/moteur_complet.elf`),
   il reste une divergence résiduelle sur `-(-7/3)` et `-(-1/2)` : la sortie
   est `7/3` et `1/2` (texte de littéral cru) au lieu de `2.[3]` et `0.5`.
   C'est le **correctif 2** : `vf` reconstruisait le littéral avec le tag `f`
   et ne passait jamais par la normalisation/réduction de `rat_make`, et
   `val_str` affichait directement le payload d'un littéral étiqueté `i`
   contenant un texte `p/q`.

Corollaire : le tag `f` produit par `vf` pour un littéral contenant `/`
faisait aussi répondre `type(7/3)` = `flottant` au lieu de `rationnel`
(mesuré : ELFAVANT `flottant`, référence `rationnel`).

---

## 2. Correctif appliqué

### `vf` (l.574)

- Extraction du numérateur **et** du dénominateur dans deux variables.
- Un préfixe `--` (négation d'un littéral déjà signé, émis par le repli) est
  réduit à un seul `-`.
- Appel à `rat_make($num, $den)` : le signe est porté sur le numérateur,
  le dénominateur est rendu strictement positif, le PGCD est réduit, et la
  forme canonique `r:<num>/<den>` est produite (retour `vi` pour un entier).
- Le littéral est étiqueté `r:` — et non plus `f:` — car un texte contenant
  `/` est un **rationnel**. Les littéraux réellement flottants (avec `.`)
  gardent le chemin `f:` inchangé.

### `val_str` (l.482)

- Pour un tag `i`, on cherche un `/` dans le payload. S'il y en a un, le texte
  est un rationnel non normalisé laissé par le repli : on le fait passer par
  `rat_make` puis `rat_dec` avant affichage.
- Les textes entiers (sans `/`) et tous les autres tags sont inchangés.

Exactitude grands numérateurs : `rat_make`, `rat_dec`, `bgcd`, `bdivmod`,
`bmul`, `badd` opèrent déjà sur des **chaînes décimales** (arithmétique
bigint en base 10), sans passer par l'`entier` tagué G5. Le correctif n'ajoute
aucune conversion `entier()`, donc aucun débordement n'est introduit.

`rose verif` sur les deux fichiers possédés : `vérifié` (rc=0).

---

## 3. Tests — 50 rationnels, 0 divergence

Fichier `compilateur/tests_corr2.rose` (50 expressions) :
négatifs, zéro (`-0`, `-0/7`, `0/-7`, `0/5`), positifs, grands numérateurs
(> 2^60, dont `±10^20/7` et `±2^64/3`), réductions PGCD (`2/-4`, `-2/-4`,
`1/-1`, `-1/-1`), sommes/produits/quotients de rationnels négatifs et
périodes longues (`-355/113`, `-123456789/97`).

Comparaison octet pour octet à `ROSE_SANS_JIT=1 rose/rose`, sur les DEUX
chaînes d'exécution :

| Chaîne | Commande ELF | Résultat |
|---|---|---|
| Interpréteur RIR seul | C0(`interpreteur_rir.rose`) lit le dump `rose rir` | **0 divergence / 50** |
| Moteur complet (corr_p1 + corr2) | C0(`moteur_complet.rose`) lit la source | **0 divergence / 50** |

Les quatre sources citées dans §6 sont toutes **CONFORMES** sur la chaîne
moteur complet (sortie ELF = sortie référence, `diff` vide) :

`t07_rationnel_neg`, `g5b606_rat_litteraux`, `t242_rat_litteraux_grands`,
`t243_rat_somme_calcul`.

Contrôle complémentaire : `type(7/3)` et `type(-7/3)` rendent désormais
`rationnel` dans l'interpréteur RIR (référence : `rationnel`).

Note de périmètre : la double négation littérale `-(-7/3)` est correcte dans
le moteur complet (RIR en mémoire), mais le **dump texte** `rose rir` rend
`(f -7/3)` à la fois pour `-(-7/3)` et `-7/3` : l'information de négation
est perdue dans la sérialisation textuelle. Ce défaut du format de dump,
extérieur à ce correctif, est pourquoi `tests_corr2.rose` n'en contient pas
et reste un banc à 0 divergence sur les deux chaînes.

---

## 4. Validation globale

Toutes les commandes sous garde-fou `12000000` Ko.

| Contrôle | Résultat |
|---|---|
| `rose/rose verif compilateur/interpreteur_rir.rose` | `vérifié`, rc=0 |
| `rose/rose verif compilateur/tests_corr2.rose` | `vérifié`, rc=0 |
| C0 → ELF de `interpreteur_rir.rose` | rc=0, 205 900 o, magic `7f454c4602010100`, stderr vide |
| Point fixe émetteur | `b3ed9262dd54776a6dbd07bc12368d0e` sur deux passes |
| `cargo test --release` (RUST_TEST_THREADS=1) | 314 + 327 passés, 0 échec |
| `validation_finale.sh` | **RC=0, 0 échec** : point fixe, AOT 75/75, JIT 25/25, WASM 92 o, RVM 317811, AZ 31 CONFORME/0 non-conforme, HOST-1 fib=317811 |

Journal complet : `/tmp/opencode/corr2_work/validation_finale.out`, dernière
ligne `=== RESULTAT: 0 échec(s) ===`.

---

## 5. Ce qui reste hors périmètre

Les autres lignes de `HORDE_INTEGRATION.md` §6 restent telles quelles :
(1) repli 64 bits — traité séparément dans `CORR1_CONSTANTES.md` ;
(3) flottants (`1.5` → `1`) ; (4) alias primitifs ; (5) arguments ;
(6)–(12) couverture RIR/JIT/AOT/gc/runtime/CLI/REPL/FFI.
Ce document ne couvre que la ligne (2).

Toutes les mesures ci-dessus sont des constats datés du 23/11/2026 dans ce
worktree ; elles doivent être re-mesurées après tout changement de composite,
de graine Rust ou de topologie.
