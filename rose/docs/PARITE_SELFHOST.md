# PARITE_SELFHOST — harnais permanent de parité du moteur auto-hébergé

Constat daté : **23/11/2026**, worktree exclusif `/tmp/opencode/rel_rel3`
(HEAD `cdd57af`). Aucun commit, aucun push. Deux fichiers ajoutés :
`scripts/verifier_parite_selfhost.sh` et ce document. Aucun fichier de
`rose/src/**`, `compilateur/**` ou d'un autre script n'a été modifié
(la régénération du composite a été testée dans une copie hors dépôt).

## 1. Ce que le harnais mesure

Les **141** sources `tests_selfhost/*.rose` sont exécutées deux fois :

| Rôle | Commande |
|---|---|
| oracle Rust | `ROSE_SANS_JIT=1 <rose/rose> SOURCE.rose [args]` |
| moteur self-hosté | `<moteur_complet.elf> SOURCE.rose [args]` |

Comparaison : **rc + stdout octet pour octet**. stderr n'est pas comparé
(c'est un diagnostic propre à chaque moteur) mais il est conservé dans les
artefacts. Un contrôle de déterminisme (2e exécution) est fait sur tout cas
non conforme.

Classes (esprit de `tests_selfhost/lancer.sh`) :

- **CONFORME** : `rc_rust=0`, `rc_self=0`, stdout identiques ;
- **DIVERGENT** : Rust refuse (`rc_rust≠0`) et Rose accepte (`rc_self=0`),
  ou les deux rc=0 mais stdout différents ;
- **BLOQUE** : les deux refusent, ou timeout/artefact manquant des deux côtés.

Verdict : **rc=0 ssi `erreur_selfhost=0` sur toutes les lignes**, c'est-à-dire
aucun cas `rc_rust=0` avec `rc_self≠0` ou stdout différent. C'est la
traduction exacte de « aucun cas où Rose self-hosté se trompe sur un
programme que Rust accepte ». Les divergences où **Rust refuse** ne sont pas
des erreurs Rose : elles comptent dans `DIVERGENTS` mais pas dans `erreurs`.

Le TSV horodaté est `parite_<UTC>.tsv` dans le workdir ; une copie stable
`/tmp/opencode/parite_selfhost_last.tsv` est laissée à chaque passe.

## 2. Commande exacte

Depuis le worktree :

```bash
cd /tmp/opencode/rel_rel3
bash scripts/garde_fou_mem.sh 12000000 \
  bash scripts/verifier_parite_selfhost.sh --workdir /tmp/opencode/parite_final
```

Le script découvre tout seul : le binaire Rust (`$ROOT/rose/rose`, puis
`/home/clausake/Stefan/ROSE/rose/rose`, puis `target/release/rose`), un C0
valide dans `/tmp/opencode/C0*.elf`, et un moteur ELF valide dans
`/tmp/opencode/moteur*.elf`. S'il ne trouve pas de moteur, il compile
`compilateur/moteur_complet.rose` avec C0 sous le même garde-fou
(8 min 26 mesurées, ELF identical md5 `9344caf8…`). Il crée idempotemment
les fixtures `/tmp/opencode/sh9_agents/battery/{input.txt,critique_entree.txt,
out.txt,out2.txt}` et `/tmp/aotwork/entree.txt`, et recrée si besoin le
fichier hors dépôt `/tmp/opencode/point_entree_complet.txt` à partir du
composite suivi (le générateur en a besoin pour `--source-only`).

Options utiles : `--moteur ELF`, `--c0 ELF`, `--rose-bin BIN`,
`--timeout N`, `--filtre MOT`, `--forcer-build`, `--sans-fixtures`,
`--sans-args`. `--regen-source` appelle
`scripts/build_moteur_sans_rust.sh --source-only` **avant** C0 ; ne
l'employer que si le composite est réellement absent : la régénération
écrase les correctifs appliqués *après* génération (mesuré : sources
régénérée `437461` o / md5 `2a8c667b…` vs suivie `469824` o /
md5 `65d12b38…`, soit tous les correctifs CORR1–CORR6 perdus). Le chemin
par défaut conserve donc la source suivie, ce qui est le comportement
reproductible du jalon.

## 3. Résultat brut — passe courante (moteur construit depuis HEAD)

Exécution réelle, `rc=1` :

```
[FIXTURES] /tmp/opencode/sh9_agents/battery/{input.txt,critique_entree.txt,out.txt,out2.txt}, /tmp/aotwork/entree.txt
[MOTEUR] /tmp/opencode/moteur_h.elf (md5 9344caf808074eb283b2ec86dafb78af, 1902136 octets)
[ORACLE] /home/clausake/Stefan/ROSE/rose/rose (md5 7614a534b9411917ef9cd49a834014a8)
BATTERIE DE PARITÉ SELF-HOST — 2026-11-23T22:22:34Z — durée 3 s
moteur self-hosté : /tmp/opencode/moteur_h.elf (md5 9344caf808074eb283b2ec86dafb78af)
oracle Rust       : /home/clausake/Stefan/ROSE/rose/rose (md5 7614a534b9411917ef9cd49a834014a8)
fixtures externes : 1   arguments t80/t81/t999 : 1
comparaison       : rc + stdout octet pour octet (stderr = diagnostic, non comparé)
tests=141  CONFORMES=134  DIVERGENTS=3  BLOQUES=4
ERREURS self-host sur programme accepté par Rust = 1

=== DIVERGENCES ===
  t104_surcharge_arith       rust_refuse(rc=1)                      nat= | self=3|0|
  t105_surcharge_arite       rust_refuse(rc=1)                      nat= | self=3|3|
  t999_critique_g5           sortie                                 nat=1|31|vrai|principale : test SH9||102|f|4096|2|17|vrai|12|11|vrai|faux|3|67|7600078848|2482 | self=1|31|vrai|principale : test SH9||102|f|4096|2/1|17|vrai|12|11|vrai|faux|3|67|7600078848|24

=== BLOQUES ===
  t106_hof_param             refus_des_deux(rc=1/1)                 Rose : erreur — [Ligne 1, Colonne 0] Fonction 'fn' inconnue
  t107_hof_var               refus_des_deux(rc=1/1)                 Rose : erreur — [Ligne 2, Colonne 0] Variable 'carre' utilisée sans valeur
  t55_liste_imbriquee        refus_des_deux(rc=1/1)                 Rose : erreur — [Ligne 1, Colonne 0] Listes de listes non supportées pour l'instant
  t82_argument_hors          refus_des_deux(rc=1/1)                 Rose : erreur — index hors des limites des argumentsRose : erreur — index hors des limites des arguments

TSV : /tmp/opencode/parite_final/parite_20261123T222231Z.tsv
Artefacts : /tmp/opencode/parite_final
VERDICT : 1 cas où Rose self-hosté se trompe sur un programme que Rust accepte — RC=1
```

TSV produit (lignes non CONFORMES ; les 134 autres ont `rc_rust=rc_self=0`,
md5 égaux, `erreur_selfhost=0`) :

```
nom	classe	rc_rust	rc_self	md5_rust	md5_self	erreur_selfhost	note
t104_surcharge_arith	DIVERGENT	1	0	d41d8cd98f00b204e9800998ecf8427e	b0421c9e7ed036d30400a5858055c6ee	0	rust_refuse(rc=1)
t105_surcharge_arite	DIVERGENT	1	0	d41d8cd98f00b204e9800998ecf8427e	24f6e6d78ba285f5c3c90539890a1924	0	rust_refuse(rc=1)
t106_hof_param	BLOQUE	1	1	d41d8cd98f00b204e9800998ecf8427e	a972cf9522949a50ab109476bcdbd30e	0	refus_des_deux(rc=1/1)
t107_hof_var	BLOQUE	1	1	d41d8cd98f00b204e9800998ecf8427e	75a7d432551e4734ec0b8347b5929739	0	refus_des_deux(rc=1/1)
t55_liste_imbriquee	BLOQUE	1	1	d41d8cd98f00b204e9800998ecf8427e	356fd6d99269912af7a27ba89a8b0ae5	0	refus_des_deux(rc=1/1)
t82_argument_hors	BLOQUE	1	1	d41d8cd98f00b204e9800998ecf8427e	d41d8cd98f00b204e9800998ecf8427e	0	refus_des_deux(rc=1/1)
t999_critique_g5	DIVERGENT	0	0	de94136cd3a3e6a2e8ee2f38be9cf7ce	87dcc856470dc8c2e235c3ea626280d6	1	sortie
```

La compilation depuis la source suivie a été rejouée séparément sous le
garde-fou (`--forcer-build --c0 /tmp/opencode/C0_h.elf`) : même ELF,
md5 `9344caf808074eb283b2ec86dafb78af`, 1 902 136 octets, 8 min 26,
stderr vide ; TSV `/tmp/opencode/parite_build_h/parite_20261123T221253Z.tsv`.

## 4. Interprétation des 5 divergences et 3 bloqués (jalon 133/141)

Le chiffre `133/141` de `PLAN.md` (vague 97) est **reproductible telles
quelles** avec le profil historique : moteur ELF `moteur_b.elf`
(md5 `4b0da1c6c0500e58658be83de7fe1390`, antérieur au correctif
d'arguments `5f63719`) et aucun argument passé aux tests. Commande :

```bash
cd /tmp/opencode/rel_rel3
bash scripts/garde_fou_mem.sh 12000000 \
  bash scripts/verifier_parite_selfhost.sh \
    --moteur /tmp/opencode/moteur_b.elf --sans-args \
    --workdir /tmp/opencode/parite_hist_b
```

Résultat brut, `rc=0` :

```
tests=141  CONFORMES=133  DIVERGENTS=5  BLOQUES=3
ERREURS self-host sur programme accepté par Rust = 0
  t104_surcharge_arith       rust_refuse(rc=1)   nat= | self=3|0|
  t105_surcharge_arite       rust_refuse(rc=1)   nat= | self=3|3|
  t81_argument               rust_refuse(rc=1)   nat= | self=chemin_source|
  t82_argument_hors          rust_refuse(rc=1)   nat= | self=chemin_source|
  t999_critique_g5           rust_refuse(rc=1)   nat=0| | self=0|1705|faux|…
  t106_hof_param             refus_des_deux(rc=1/1)
  t107_hof_var               refus_des_deux(rc=1/1)
  t55_liste_imbriquee        refus_des_deux(rc=1/1)
VERDICT : 0 cas où Rose self-hosté se trompe sur un programme que Rust accepte — RC=0
```

TSV : `/tmp/opencode/parite_hist_b/parite_20261123T222240Z.tsv`.

Lecture des **5 divergences** (toutes `rc_rust=1`, `rc_self=0`,
`erreur_selfhost=0`) :

| Source | Rust | Rose self-host | Cause |
|---|---|---|---|
| `t104_surcharge_arith` | refuse : fonction `f` définie deux fois | accepte, `3`, `0` | surcharge arithmétique non implémentée dans le front-end Rust |
| `t105_surcharge_arite` | refuse : fonction `f` définie deux fois | accepte, `3`, `3` | idem, surcharge par arité |
| `t81_argument` | refuse (0 argument : `argument(0)` hors limites) | accepte, imprime le chemin source | collision argument(0) de l'ELF pré-`5f63719` |
| `t82_argument_hors` | refuse (0 argument) | accepte, imprime le chemin source | idem |
| `t999_critique_g5` | refuse (0 argument) | accepte, lit la source comme fichier (1705 o) | idem |

Lecture des **3 bloqués** (`rc_rust=1`, `rc_self=1`, `erreur_selfhost=0`) :
`t106_hof_param` (« Fonction 'fn' inconnue »), `t107_hof_var` (« Variable
'carre' utilisée sans valeur ») et `t55_liste_imbriquee` (« Listes de listes
non supportées pour l'instant »). Aucun dump n'existe pour eux : les deux
moteurs refusent la source, donc Rose ne peut pas se tromper sur un
programme que Rust accepte.

⚠ **Correction factuelle** : `PLAN.md` §vague 97 attribue les 5 divergences
à « surcharges t104/t105, fichiers externes absents t83/t84/t86 ». Le TSV
horodaté ci-dessus montre qu'avec les fixtures créées, `t83`/`t84`/`t86`
sont **CONFORMES** ; les trois autres divergences sont `t81`/`t82`/`t999`
(cas `argument(0)` sans argument). Sans fixtures, `t83` seul bascule en
divergent. Les noms du PLAN ne sont donc pas ceux mesurés ; les **compteurs**
133/5/3 et le verdict 0 erreur sont, eux, exactement reproduits.

Écart avec la passe courante (134/3/4, rc=1) : le moteur construit depuis
HEAD contient le correctif `5f63719`, donc `t81` devient CONFORME. `t82`,
sans argument, devient BLOQUÉ des deux côtés (au lieu de divergent). Et
`t999`, avec l'argument passé comme dans `lancer.sh`, devient une vraie
divergence : ligne 9 `chaine(flottant("2"))` → `2` chez Rust, `2/1` chez
Rose. Ce défaut de formatage flottant est **préexistant et documenté**
(`rose/docs/CORR5_ARGUMENTS.md` §3.2, défaut n°3 de
`HORDE_INTEGRATION.md`) ; il n'est pas couvert par le correctif d'arguments.
Le harnais le signale à juste titre par `erreur_selfhost=1` et `rc=1`.
C'est la seule erreur Rose de l'état courant.

## 5. Rejouer après chaque modification du compilateur Rose

1. Reconstruire le moteur depuis la source suivie (≈8 min, garde-fou
   obligatoire) puis lancer la batterie en une seule commande :

   ```bash
   cd /tmp/opencode/rel_rel3
   bash scripts/garde_fou_mem.sh 12000000 \
     bash scripts/verifier_parite_selfhost.sh --forcer-build \
       --c0 /tmp/opencode/C0_h.elf --workdir /tmp/opencode/parite_apres_patch
   ```

   Le script vérifie le magic ELF64/aarch64, la taille, chmod +x, et
   enregistre md5 du moteur et de l'oracle dans le rapport.

2. Comparer au TSV précédent, sans ignorer les colonnes de rc :

   ```bash
   diff -u /tmp/opencode/parite_final/parite_20261123T222231Z.tsv \
           /tmp/opencode/parite_apres_patch/parite_*.tsv
   awk -F'\t' 'NR>1 && $7==1' /tmp/opencode/parite_apres_patch/parite_*.tsv
   ```

   Une ligne qui passe de `erreur_selfhost=0` à `1` est une régression,
   même si le nombre de CONFORMES monte.

3. Objectif annoncé `138/141` : 5 nouveaux CONFORMES. Le harnais ne
   validera que si la colonne `erreur_selfhost` est à 0 partout ; la
   correction du formatage flottant de `t999` est le prérequis pour rc=0
   dans le profil courant.

4. Ne jamais utiliser `--regen-source` sur le worktree de référence sans
   avoir d'abord réappliqué les correctifs CORR1–CORR6 au composite : cf.
   §2, la régénération depuis les modules ne les contient pas.

## 6. Artefacts conservés

| Chemin | Contenu |
|---|---|
| `/tmp/opencode/parite_final/parite_20261123T222231Z.tsv` | TSV courant (141 lignes) |
| `/tmp/opencode/parite_final/rapport.txt` | rapport courant |
| `/tmp/opencode/parite_build_h/` | build depuis la source suivie + TSV |
| `/tmp/opencode/parite_hist_b/parite_20261123T222240Z.tsv` | TSV du jalon 133/5/3 |
| `/tmp/opencode/parite_selfhost_last.tsv` / `.txt` | dernières copies stables |
| `/tmp/opencode/point_entree_complet.txt` | point d'entrée reconstruit (hors dépôt) |
| ELF moteur | `/tmp/opencode/moteur_h.elf`, `/tmp/opencode/parite_build_h/moteur_complet.elf`, md5 `9344caf8…` |
| C0 | `/tmp/opencode/C0_h.elf`, md5 `2db168b907b4a874712857d3957fd078` |
