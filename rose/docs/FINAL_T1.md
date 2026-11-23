# FINAL-T1 — dernière divergence flottante du moteur complet (ELF C0)

Date : 23/11/2026. Worktree exclusif `/tmp/opencode/final_t1`, HEAD
`b3a4173317b52546b615b50a9c4e7e7b86dc0c00`. Aucun commit, aucun push.

Fichiers possédés et modifiés :

| fichier | région |
|---|---|
| `compilateur/lecteur_source.rose` | repli de constante flottante : `replier_expr` (unaire `-`), nouveaux `flot_neg_texte` / `flot_texte_exact`, `vers_flottant` |
| `compilateur/moteur_complet.rose` | **uniquement** la même région, préfixée `lsrc_` (le composé est généré, les autres modules du worktree sont plus anciens que les correctifs 1 et 3 déjà intégrés ici : aucune régénération n'a été faite) |
| `compilateur/tests_final_t1.rose` | nouveau corpus (21 cas) |
| `rose/docs/FINAL_T1.md` | ce document |

`interpreteur_rir.rose`, `rose/src/**`, `lsrc_rpui`, `call_fn` et le point
d'entrée n'ont **pas** été touchés.

## 1. Reproduction (avant correctif)

Chaîne : `C0_3.elf compilateur/moteur_complet.rose > moteur.elf` puis
`./moteur.elf tests_selfhost/t09_flottant.rose`.

```text
ELF C0 (b3a4173)         rose/rose (référence)
1                        1.5
2                        2
10000000000000000        10000000000000000
```

Le compilateur C0 → ELF a réussi (rc=0, 8 min 11 s, 1 885 628 octets,
magic `7f454c4602010100`). La divergence est donc bien dans le moteur
complet auto-hébergé, pas dans la compilation.

## 2. Cause prouvée

Une copy du moteur dont le point d'entrée a été remplacé par
`>> lsrc_lire_programme(argument(0))`, compilée par C0, donne le dump RIR
**interne** :

```text
avant : 0000 afficher_abstrait flottant 1
                  0001 afficher_abstrait flottant 2.0
après : 0000 afficher_abstrait flottant 1.5
                  0001 afficher_abstrait flottant 2
        0002 afficher_abstrait entier 10000000000000000
```

Le dump RIR final du lecteur C0 est **octet pour octet identique** à
`rose rir tests_selfhost/t09_flottant.rose` (après retrait du préfixe
`OK|` ajouté par la sonde).

La constante est donc perdue **avant** `vf`, dans le repli du lecteur.
Mesures complémentaires, sur de petits programmes compilés directement par
C0 (sans le lecteur) :

```text
>> 1.5                     -> 1        (au lieu de 1.5)
>> 3.14                    -> 3        (au lieu de 3.14)
>> flottant("1.5")         -> 1        (au lieu de 1.5)
>> chaine(flottant("1.5")) -> 1        (au lieu de 1.5)
>> -0.5                    -> segfault à la compilation C0
```

`lsrc_vers_flottant` appelait `chaine(flottant($txt))`. Sous l'ELF C0,
cette conversion native tronque la fraction (et l'arithmétique flottante
native sur une littérale négative segmente). Le repli unaire de
`lsrc_replier_expr` faisait de même via `lsrc_val_d` + `lsrc_flot_txt`.
En chaîne native (interprétée par `rose/rose`), le même source donnait
déjà `1.5` : le défaut est propre au backend C0.

## 3. Correctif

Aucune opération sur le type `flottant` n'est plus utilisée dans ce repli :

1. `vers_flottant` / `lsrc_vers_flottant` court-circuite toute texte sans
   `/` vers le nouveau formateur textuel `flot_texte_exact` :
   développement textuel d'un exposant, suppression des zéros finaux,
   suppression du point quand la fraction devient vide (`2.0 → 2`),
   signe conservé (`-0.5 → -0.5`). Le texte décimal source arrive ainsi
   intact dans le RIR (`flottant 1.5`), où `vf` (déjà corrigé en CORR3)
   construit le rationnel exact `f:3/2`.
2. Le repli unaire `-` de `replier_expr` remplace
   `flot_txt(-val_d(...))` par `flot_neg_texte(vue_texte(...))` : simple
   inversion du signe du texte, sans conversion ni négation flottante.
3. La boucle de suppression des zéros finaux a dû être réécrite sans
   condition composée : mesure C0 directe,
   `tant_que (longueur($fp) > 0 et sous_chaine($fp, longueur($fp)-1, longueur($fp)) == "0")`
   avec `$fp = "0"` laisse `fp=[0]` sous l'ELF C0 alors que le natif donne
   `fp=[]`. Une boucle à drapeau `tant_que ($fin == faux) { … }` donne
   `fp=[]` des deux côtés. Sans cela, `2.0` restait `flottant 2.0` dans le
   RIR (la sortie était tout de même juste, `vf` réduisant `2.0` en `2`).
4. Les deux fichiers reçoivent exactement le même correctif (partie
   `lsrc_` du composé), sans toucher aux régions interdites.

## 4. Preuves d'exécution

Toutes les commandes ont été exécutées dans le worktree, sous
`bash scripts/garde_fou_mem.sh 12000000 …` pour les opérations lourdes.

```text
rose verif compilateur/lecteur_source.rose     -> vérifié (rc=0)
rose verif compilateur/moteur_complet.rose     -> vérifié (rc=0)
rose verif compilateur/tests_final_t1.rose     -> vérifié (rc=0)
C0 compilateur/moteur_complet.rose             -> rc=0, 1 894 804 o, magic 7f454c4602010100
ELF C0 corrigé : t09                           -> 1.5 / 2 / 10000000000000000
cmp(t09 ELF corrigé, t09 rose/rose)            -> 0 (identique octet pour octet)
corpus tests_final_t1.rose (21 cas)            -> 21/21 identiques à rose/rose
141 programmes tests_selfhost : RIR du lecteur avant/après (chaîne native) -> 141 identiques, 0 divergence
141 programmes tests_selfhost : sortie ELF avant/après -> 140 identiques, 1 seul changement : t09
sonde RIR compilée C0                          -> RIR final identique à rose rir
point fixe émetteur                            -> b3ed9262dd54776a6dbd07bc12368d0e (×2)
```

`scripts/validation_finale.sh` rejoué dans une copie pointant `B` vers le
binaire graine et `ROOT` vers ce worktree (le script original attend un
`rose/rose` et un `rose/target/release/rose` dans le worktree ; la copie
utilise les binaires déjà construits) :

```text
=== RESULTAT: 0 échec(s) ===
Rust build + tests rc=0 (327 passed, 0 failed)
point fixe OK ; AOT 75 OK, 0 ECHEC ; JIT 25/25
WASM 92 o ; RVM 317811 ; AZ 31 CONFORME, 0 non-conforme
HOST-1 fib=317811
```

Deux essais intermédiaires avaient échoué pour des raisons de harnais,
pas de code : une copie avec `ROOT` pointing sur le dossier d'artefacts,
puis un `ROSE` exporté vers la graine qui faisait échouer l'AOT (celui-ci
doit utiliser le binaire release du worktree). Les deux ont été corrigés
dans la copie du harnais et rejoués : 0 échec.

## 5. État final

* 2 fichiers source modifiés (2 régions chacun), 1 corpus de test ajouté,
  1 document ajouté. Aucun commit, aucun push.
* `git status` : `M compilateur/lecteur_source.rose`,
  `M compilateur/moteur_complet.rose`, `?? compilateur/tests_final_t1.rose`,
  `?? rose/docs/FINAL_T1.md`.
* Artefacts hors dépôt : `/tmp/opencode/final_t1_art/mc_base.elf`
  (avant), `/tmp/opencode/final_t1_art/mc_final.elf` (après),
  `/tmp/opencode/final_t1_art/probes/out/probe_final.elf` (sonde RIR).
