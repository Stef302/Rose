# VM Rose — ROSEBC v3 exécuté en Rose pur

## But

Le moteur auto-hébergé (`compilateur/moteur_complet.rose`, compilé par C0 en
ELF aarch64) exécute désormais :

```
rose rvm fichier.rosebc [classe] [methode] [args…]
```

sans faire appel au moteur Rust. Le chemin Rust (`rose/src/rosevm.rs`,
`executer_snapshot`) reste la référence de format ; le port Rose est
`compilateur/vm_rose.rose`, assemblé dans le composite juste avant le point
d'entrée CLI. Le dispatch ajouté est **le seul changement** apporté au point
d'entrée existant.

## Format exécuté (ROSEBC v3)

```
"ROSEBC" | u32 version=3
u32 n_imports | n_imports × chaîne
u32 n_signatures | n_signatures × signature
    chaîne nom | u32 n_params | n_params × (chaîne, ty2)
    ty2 retour | bool a_retour | bool corps_present
    [u32 n_ins | n_ins × instruction]
    chaîne dll | chaîne dll_son | chaîne symbole | u32 ligne
u32 n_ins | n_ins × instruction

chaîne = u32 len + UTF-8 ; bool = 1 octet ; ty2 = 2 octets ;
expression = tag u8 + charges + u32 ligne ; instruction = tag u8 + charges.
```

Les octets sont relus par `u_bytes(lire_texte(chemin))` sous forme de
`liste<entier>` : sous C0, `longueur`/`sous_chaine` comptent des **points de
code**, pas des octets. L'indexation du bytecode est donc faite sur la liste
d'octets, et les chaînes du snapshot sont reconstruites par
`u_texte(u_points(...))`.

## Couverture

### Expressions — 18/18 tags

| Tag | Rose | Tag | Rose |
|---|---|---|---|
| 0x01 `LITTERAL` | `vi` / `vf` (selon `flott`/`rationnel`) | 0x0A `MULTIFACTORIELLE` | produit pas de `k` |
| 0x02 `LITCOMPLEXE` | réservé, valeur `s:""` (voir écarts) | 0x0B `SOUSFACTORIELLE` | `!n` |
| 0x03 `CHAINE` | `vs` | 0x0C `PRIMORIELLE` | `n#` |
| 0x04 `BOOLEEN` | `vb` | 0x0D `TERMINAL` | `n(n+1)/2` |
| 0x05 `VAR` | environnement | 0x0E `SUPERFACTORIELLE` | `1!·2!·…·n!` |
| 0x06 `APPEL` | fonctions du snapshot + `call_prim` + primitives mutantes | 0x0F `INDEX` | chaîne / liste / table |
| 0x07 `BINAIRE` | `+ - * / ÷ % ^ < ≤ > ≥ == != et ou` | 0x10 `LISTE` | `lst_empty`/`lst_append` |
| 0x08 `UNAIRE` | `-`, `neg`, `non`, `!`, `¬` | 0x11 `LISTEVIDETYPEE` | `lst_empty` |
| 0x09 `FACTORIELLE` | `fact_big` | 0x12 `TABLE` | `tab_empty`/`tab_set` |

### Instructions — 13/13 tags

`AFFICHER`, `LIRE`, `AFFECT`, `AFFECTINDEX`, `SI`, `TANTQUE`, `POUR`,
`RETOUR`, `EXPRSTMT`, `BLOC`, `SPAWN`, `ESSAYER`, `LEVER`.

### Sémantique

- **Valeurs** : les valeurs taguées de `interpreteur_rir.rose`
  (`i:`, `r:`, `f:`, `s:`, `b:`, `l:`, `t:`, `v:`), les mêmes que le moteur
  auto-hébergé — pas de conversion vers un autre modèle.
- **Appels** : les signatures du snapshot sont indexées au chargement ;
  `rvm_call_fn` ouvre une trame, lie les paramètres, exécute le corps et
  referme la trame. La fermeture pose un marqueur `"@<taille>"` dans le
  journal d'environnement : les liaisons créées par l'appel deviennent
  invisibles, ce qui donne la portée correcte **y compris en récursion**
  (fib(5) → 5, fib(12) → 144).
- **Primitives mutantes** : `inserer` et `joindre` sont des `ExprStmt` dans
  le snapshot Rust ; le VM relie le résultat de `call_prim` à la variable
  premier argument (table/listes persistantes).
- **Arguments CLI** : `argument(i)` et `nb_arguments()` dans le bytecode
  voient les arguments **après** `[classe] [methode]`, exactement comme
  `cmd_rvm` de `main.rs`.
- **Encodage des résultats** : position et statut sont des champs de 10
  chiffres (pas de séparateur `|`) — une valeur Rose peut contenir `|`.

## Écarts assumés

1. **v1/v2** : les anciens `.rosebc` à classes/méthodes ne sont pas exécutés
   par le port Rose (le dispatch vise le snapshot v3 produit par `rose bc`
   et par `ser_bytecode.rose`). Le binaire Rust, lui, garde la
   rétrocompatibilité.
2. **`LITCOMPLEXE`** (0x02) : les 16 octets IEEE-754 ne sont pas décodés en
   `Complexe` ; le tag est consommé et une chaîne vide est produite. Aucun
   programme du corpus de parité ne l'utilise.
3. **E/S** : `LIRE` lit `/dev/stdin` comme une chaîne ; le typage `entier` de
   `Instr::Lire` n'est pas appliqué.
4. **Budget/sandbox** : pas de limite d'instructions ni de nombre de slots
   côté Rose (garde de boucle de 10^8 tours seulement, anti-boucle
   accidentelle). Le chargeur valide magic, version, troncature et compteurs,
   pas la profondeur maximale d'imbrication.
5. **`SPAWN`** est exécuté séquentiellement (pas de threads) ; `ESSAYER`
   exécute `corps` et ignore `attraper`/`finalment` ; `LEVER` rend le statut
   d'erreur sans propagation d'exception structurée.
6. **Lanceur** : le script `rose` généré par `scripts/build-sans-cargo.sh`
   refuse encore `rvm` (rc=3). Le moteur ELF, lui, l'exécute. Pour une
   distribution complète, retirer `rvm` de la liste refusée du lanceur
   (une ligne, hors du périmètre de ce chantier).

## Pièges C0 contournés (mesurés)

- `entier(appel_de_fonction(...))` fait segfaulter l'émetteur G5 : toutes
  les conversions passent par une variable locale (`rvm_ip`, `rvm_ival`,
  `rvm_vstr_i`, `rvm_octet_c`).
- `|` comme séparateur de résultat casse les valeurs liste/table/chaîne :
  remplacé par un préfixe de position à largeur fixe.
- `joindre` sur une globale ne peut que **ajouter** : la portée des trames
  est obtenue par marqueurs `"@n"`, pas par troncature.
- `longueur`/`sous_chaine` sur le fichier binaire comptent les points de
  code : lecture par `u_bytes` + indexation `liste<entier>`.

## Preuves

- `compilateur/tests_vm_rose.rose` : **54 OK, 0 ECHEC** (couverture des 31
  tags, chargeur, encodage, dispatch, 10 marqueurs de non-régression).
- 11 snapshots générés par `rose bc` (littéraux, appels, récursion,
  `tantque`, `pour`, `si`, listes, tables, factorielles, chaînes, arguments
  CLI) : sortie **octet pour octet identique** à `rose rvm` Rust, rc=0 des
  deux côtés, avec l'ELF auto-hébergé complet (2 051 156 octets, compilé
  par C0 avec le garde-fou 12 Go).
- `ser_bytecode.rose` reste byte-identique à `rose bc` Rust sur le cas
  minimal (contrôle md5). Sur un programme mêlant literals repliés, le
  sérialiseur écrit encore `TyKind=0x01` et des numéros de ligne décalés
  au lieu de `0x0c` et des lignes Rust (écart préexistant, hors périmètre) ;
  la VM l'ignore et rend **la même sortie** pour les deux fichiers.
- Marqueurs vérifiés après câblage : `bi_pow`, `mc_args`, `rep_run`,
  `cli_aide_pour`, `cands`, `cr_feuille`, `sw_module`, `sb_module`,
  `sb_module_avec`.
