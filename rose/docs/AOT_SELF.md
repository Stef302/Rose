# AOT_SELF — `rose aot` dans le moteur auto-hébergé, sans Rust

**Constat daté — 24/11/2026, worktree `/tmp/opencode/az_aot` (HEAD 162d7a2).**
Toutes les valeurs ci-dessous ont été obtenues par exécution ; aucune n'est
déduite d'une doc ou d'une spec.

## 1. Ce qui a été câblé

`compilateur/moteur_complet.rose` contient désormais deux blocs délimités par
des marqueurs explicites :

| Marqueur | Rôle |
|---|---|
| `# ═══ AOT_SELF:DÉBUT …` / `# ═══ AOT_SELF:FIN …` | émetteur ELF intégré (état G5, fonctions atteignables, pont `ea_rir_vers_elf`) |
| `# ── aot \| orc \| link \| llvm : …` | dispatch CLI + filtre `mc_aot_soutenable` |

Chaîne réelle (aucun binaire Rust, aucun helper LLVM, aucun cargo) :

```
moteur.elf aot prog.rose -o sortie
  → lsrc_lire_programme(prog.rose)        # source → RIR, same que `rose rir`
  → mc_aot_soutenable(rir)                # refus explicite hors sous-ensemble
  → ea_rir_vers_elf(rir)                  # "OK|<ELF>" ou "ERR|<message>"
  → ecrire_bytes(sortie, ELF) + chmod(sortie, 0755)
```

`ea_rir_vers_elf` est la transposition exacte du « main G5 » de
`compilateur/emetteur_elf.rose` (lecture seule) : réservation des labels,
`g5_parse`, `g5_compile_fonction`, routines G5, `g5_patch`, `g5_elf`.

Deux pièges Rose rencontrés et corrigés (preuve par exécution) :

1. **Une affectation dans une fonction crée une locale**, même avant
   l'affectation : `$g5_code = g5_patch()` dans le pont masquait la globale
   lue par `g5_patch`/`g5_elf` → tous les `bl`/`b.cond` du binaire émis
   étaient à zéro (`Illegal instruction` à l'exécution). Correction :
   `g5_elf($g5_code_patche)` reçoit le tableau patché en paramètre ; le pont
   n'écrit jamais `$g5_code`.
2. `eval_expr` existe déjà dans le moteur (4 arguments) : l'émetteur intégré
   utilise `ea_eval_expr` (1 argument), après renommage de la définition **et**
   de tous les appels. `octet_brut` n'est pas dupliqué : c'est une primitive
   interceptée par le backend G5 sous C0.

`link` et `llvm` sont de vrais alias du même chemin (comme `orc`) : ils
prennent une **source Rose**, pas un objet `.o`, et écrivent le même ELF.

## 2. Couverture mesurée — 10 programmes, 10 conformes

Commande : `bash /tmp/opencode/aot_self_probe/valider10.sh`
(artefacts : `/tmp/opencode/aot_self_probe/valide/`).

| Programme | Valeur du binaire | Message du compilateur |
|---|---|---|
| `rose/tests_aot8/programmes/01_arith.rose` | `14` | valeur exacte mémorisée |
| `02_fib.rose` (récursion) | `610` | code de retour = valeur du programme |
| `03_varfact.rose` (factorielle récursive) | `479001600` | idem |
| `06_rationnel_add.rose` | `0.5` | valeur exacte mémorisée |
| `09_chaine.rose` | `salut` | valeur exacte mémorisée |
| `17_boucle_pour.rose` | `45` | code de retour = valeur du programme |
| `22_rationnel_mul.rose` | `0.[6]` | valeur exacte mémorisée |
| `23_fib_iteratif.rose` | `4181` | code de retour = valeur du programme |
| `25_racine_puissance.rose` | `6561` | idem |
| `10_rationnel_div.rose` (`>> 7 / 2`) | `3.5` | valeur exacte mémorisée |

Pour chacun : `rc=0` des deux compilations, binaire ELF aarch64 exécutable,
**sortie octet pour octet identique** à `rose aot` Rust (preuve : le TSV
`valide/resultats10.tsv`). Le tableau ci-dessus donne le message du moteur
Rust. Celui du moteur C0 est identique sur 6 programmes (dont tous les
`afficher_abstrait entier`) ; sur les 4 autres (`06`, `09`, `22`, `10` :
constantes rationnelles/chaîne, RIR `afficher (f …)` / `afficher (s …)`) il
affiche « code de retour = valeur du programme » au lieu de « valeur exacte
mémorisée ». **Seul le texte du message diffère, jamais le binaire ni sa
sortie** : écart cosmétique documenté, non une divergence d'AOT.

Le banc `compilateur/tests_aot_self.rose` rejoue les dix sorties et affiche
`TESTS AOT SELF : 10 OK, 0 ECHEC`, rc=0 — sous le moteur Rust **et** sous
l'ELF C0.

## 3. Refus explicites (aucun fichier écrit, rc=1)

`mc_aot_soutenable` refuse avant toute écriture :

| RIR contenant | Raison mesurée |
|---|---|
| `flottant` | le repli machine additionne des flottants comme des entiers : `08_flottant.rose` affichait `0` au lieu de `3.75` |
| `bin ^ … entier B` avec `B >= 60` | au-delà de l'ABI G5 (`n<<3` sur 64 bits) : `2 ^ 60` affichait `--1152921504606846975.0`, `2 ^ 62` et `2 ^ 100` affichaient `0` |
| `hasard` | routine RNG non exécutable dans le binaire émis (`Illegal instruction`) |
| `lire_texte` | idem sur `18_lire_fichier.rose` |
| toute tête d'op RIR hors `{afficher, afficher_abstrait, affect, affect_ast, retour, retour_abstrait, saut, saut_si_faux, saut_si_faux_abstrait, expr}` | listes, tables, opérations non émettables |

En plus, si l'émetteur lui-même signale `g5_err`, `ea_rir_vers_elf` rend
`ERR|…` et la CLI affiche `rose aot : programme hors sous-ensemble natif …`
sans créer le fichier de sortie.

Refus mesurés sur le corpus AOT8 : `04_bigint_puissance`, `08_flottant`,
`18_lire_fichier`, `20_hasard` (Rust les accepte, le moteur C0 refuse ;
cette couverture est documentée comme écart, pas simulée). `05_bigint_factorielle`,
`21_primitives_const`, `24_compose` sont refusés par les deux moteurs.

## 4. Non-régression

Mesuré sur le même arbre après intégration :

- point fixe `b3ed9262dd54776a6dbd07bc12368d0e` inchangé
  (`rose rir compilateur/emetteur.rose` md5) ;
- `TESTS RIR : 15 OK, 0 ECHEC` (`rose compilateur/tests_interpreteur_rir.rose`) ;
- parité self-host `141/141` avec l'ELF final
  (`scripts/verifier_parite_selfhost.sh --moteur …/moteur_final.elf` :
  `tests=141  CONFORMES=141  DIVERGENTS=0  BLOQUES=0`, rc=0) ;
- formats `15/15` octet-identiques : wasm 5/5, bc 5/5, comp 5/5 — pour
  `comp`, la comparaison doit utiliser **le même binaire de référence** que
  celui que le gabarit lit (`rose/target/release/rose`) : comparer avec la
  copie `rose/rose`, périmée et de taille différente, donne 5 faux écarts ;
- `scripts/validation_finale.sh` n'a PAS été rejoué ici (il relance
  `cargo build` pendant que d'autres agents compilent sur le même nœud) ;
  les contrôles qu'il couvre et qui sont indépendants de cargo ont été
  rejoués un par un ci-dessus.

## 5. Entretien

`compilateur/moteur_complet.rose` est un fichier **composé** : ne pas lancer
`scripts/build_moteur_sans_rust.sh --source-only` sans réappliquer ensuite le
bloc `AOT_SELF` (le script le régénère depuis les modules et écrase les
patchs). Le contrôle après régénération est :

```bash
grep -c 'AOT_SELF:DÉBUT' compilateur/moteur_complet.rose   # doit être 1
grep -c 'mc_cmd == "aot"' compilateur/moteur_complet.rose  # doit être >= 1
```

Le lanceur généré par `scripts/build-sans-cargo.sh` ne refuse plus ces quatre
commandes (commit 0597663, `aot|orc|link|llvm` retirés du `case … exit 3`) :
`./release-sans-cargo/rose aot …` passe désormais au moteur C0. Le dernier
refus du lanceur est `wasm`.

Fichiers possédés par ce chantier : `compilateur/moteur_complet.rose`,
`compilateur/tests_aot_self.rose`, `rose/docs/AOT_SELF.md`.
`compilateur/emetteur_elf.rose` et `rose/src/**` n'ont PAS été modifiés.
