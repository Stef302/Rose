# PROMO_VALEUR — correction du blocage (a) « Addition valeur + entier »

Date de mesure : 24/11/2026. Worktree exclusif : `/tmp/opencode/last1`, HEAD
`be138616c4f33ca453f93bc342069e9cb2c3f524`. Aucun commit, aucun push.

## 1. Symptôme reproduit

Le moteur auto-hébergé compilé par C0 (`/tmp/opencode/rf_selfhost_out/moteur.elf`,
1 965 640 o, md5 `8c77c4e592a8b62b8cf7ecd57251fef6`, aarch64) refusait de
produire son propre RIR :

```sh
cd /tmp/opencode/last1
bash scripts/garde_fou_mem.sh 12000000 \
  /tmp/opencode/rf_selfhost_out/moteur.elf rir compilateur/moteur_complet.rose
# rc=1, 5 min 15 s, stderr :
# Rose : erreur — [Ligne 8562, Colonne 0] Addition valeur + entier impossible
```

L'oracle Rust, lui, acceptait le même fichier (`rose rir …` → rc=0, 795 634 o,
705 `fonction`, 1 795 `param`). Le défaut est donc dans le **vérificateur
self-host**, pas dans la grammaire ni dans le backend.

La ligne fautive (avant correction) :

```rose
$vivants = m[gc_c_vivants()] + 1
```

## 2. Cause exacte (mesurée dans la source)

`compilateur/moteur_complet.rose` :

* `lsrc_ce_raw`, branche `idx` (ligne 1920) : l'indexation d'une table
  rend **`"valeur|vide"`** ;
* `lsrc_estnum` (ligne 469) ne reconnaît que `entier`, `rationnel`, `flottant`,
  `complexe` — **`valeur` n'est pas numérique** ;
* `lsrc_check_bin`, opérateur `+` (ligne 2024) : ni `chaine+chaine`, ni
  `estnum+estnum`, ni un `?` (Dyn) ne correspond → `lsrc_signaler(
  "Addition valeur + entier impossible")`.

Côté Rust, l'indexation de table est typée `?` (Dyn) : la preuve est dans le RIR
produit par l'oracle, où l'affectation est **`affect_ast slot 6 = (bin + (idx …)
(n 1))`** (ast = dynamique), alors que les affectations entières sont `affect slot`.
Le self-host devait donc accepter cette arithmétique ; il ne le faisait pas.

Pourquoi une seule ligne a d'abord échoué : les autres occurrences
(`gc_set($m, gc_c_fresh(), m[gc_c_fresh()] + 1)`, etc.) passent comme **arguments
d'une fonction à paramètres non annotés** (`fn gc_set(m, cle, valeur)`), chemin
où le vérificateur ne contraint pas le type de l'argument (mesuré avec quatre
sondes minimales : `probe1` échoue en affectation, `probe4` passe en appel).
L'affectation à `$vivants`, elle, est contrôlée — d'où l'erreur à 8562.

## 3. Correctif appliqué

Deux expressions de `gc_allouer` seulement (zone `gc_*`, aucun `rose/src/**`,
aucun émetteur touché) :

```rose
# promotion valeur -> entier avant arithmétique
$vivants = entier(val_str(chaine(m[gc_c_vivants()]))) + 1
...
# promotion avant comparaison d'ordre (découverte au 2e passage du vérificateur)
si ($vivants > entier(val_str(chaine(m[gc_c_pic()])))) { gc_set($m, gc_c_pic(), $vivants) }
```

Pourquoi `chaine(...)` est indispensable : `entier(val_str(m[…]))` **ne
compile pas** — le check d'appel refuse `valeur` là où `val_str` attend `chaine`
(`Argument de val_str: valeur reçu, chaine attendu`) ; et `entier(m[…])` rend
`Conversion valeur → entier impossible`. Le cast `chaine(x)` est la seule
conversion dont le vérificateur (C0 **et** Rust) accepte l'argument sans le
typer : il rend `chaine|vide` inconditionnellement. `val_str` décode alors
l'étiquette (`i:…` → `…`), et `entier()` rend enfin le type `entier`.

Sémantique vérifiée indépendamment : `entier(val_str(chaine(t[9])))+1` sur une
table contenant `42` rend `43` sur les deux moteurs (sonde
`/tmp/opencode/c0_last1_out/runtime_probe.rose`).

Le vérificateur lui-même n'a **pas** été modifié : aucun changement de
diagnostic, donc aucun risque de déplacer le verdict des 141 programmes.

## 4. Preuves d'exécution

Toutes les commandes ont été réellement exécutées sous
`bash scripts/garde_fou_mem.sh 12000000 …` quand elles dépassent 60 s.

| Contrôle | Commande | Résultat mesuré |
|---|---|---|
| Avant, C0 | `moteur_ancien.elf rir moteur_complet.rose` | rc=1, `[Ligne 8562] Addition valeur + entier impossible` |
| Avant, Rust | `rose rir moteur_complet.rose` | rc=0, 795 634 o |
| Après, `verif` C0 | `moteur_ancien.elf verif moteur_complet.rose` | `vérifié`, rc=0 |
| Après, `verif` Rust | `rose verif moteur_complet.rose` | `vérifié`, rc=0 |
| Compilation C0 du source corrigé | `C0_h.elf moteur_complet.rose` | rc=0, 8 min 56 s, ELF aarch64 1 965 712 o, md5 `5e9256a664ac4dfd0c54bc5fec473489`, stderr 0 o |
| **RIR self-host** | `moteur_neuf.elf rir moteur_complet.rose` | **rc=0**, 5 min 30 s, **795 791 o**, **705** `fonction`, **1 795** `param`, stderr 0 o |
| Parité moteur | `verifier_parite_selfhost.sh --moteur moteur_neuf.elf …` | `tests=141 CONFORMES=141 DIVERGENTS=0 BLOQUES=0`, `ERREURS self-host sur programme accepté par Rust = 0`, rc=0 |
| Formats byte-identiques | `wasm`/`bc`/`comp` × `t01_entiers`, `t20_add`, `t21_sous`, `t22_mul`, `t29_precedence` | **15/15 identiques** (ex. `t20_add` : 61 o `c7bb9a95…` / 116 o `c0fece99…` / 5 457 034 o `4e18f929…`) |
| Point fixe | `rose rir compilateur/emetteur.rose \| md5sum` | `b3ed9262dd54776a6dbd07bc12368d0e`, rc=0 (263 520 o) |
| `validation_finale` | sections 2–7 (§1 `cargo build` non rejoué : `cargo` absent du worktree) | **0 échec** : AOT 75/75, JIT 25/25, WASM+RVM, AZ 31 conforme / 0 non-conforme, HOST-1 fib=317811 |
| Harnais dédié | `moteur_neuf.elf`/`rose` sur `compilateur/tests_promo.rose` | rc=0 des deux côtés, stdout octet-identique (md5 `2b5dd3a760ce2e3b1b2dc8ab62b66b95`) |

Le RIR self-host passe de 795 634 à 795 791 o (+157 o) : ce sont exactement
les nœuds supplémentaires `app chaine`, `app val_str`, `app entier` ajoutés aux
deux sites promus. Le nombre de fonctions (705) et de paramètres (1 795) est
inchangé : aucune fonction n'a été ajoutée ni supprimée.

`validation_finale` a été jouée avec le binaire Rust de référence
`/home/clausake/Stefan/ROSE/rose/target/release/rose` (le worktree n'a ni
`cargo` ni `rose/target/`), donc §1 (« build + tests Rust ») est neutralisé ;
ses sections 2–7 sont intégralement rejouées. Deux liens symboliques ignorés
par git (`rose/rose`, `rose/target/release/rose`) pointent vers ce binaire pour
que les harnais AOT et AZ trouvent leur exécutable.

## 5. Fichiers possédés modifiés / créés

* `compilateur/moteur_complet.rose` — 2 expressions promues + commentaire.
* `compilateur/tests_promo.rose` — nouveau harnais de non-régression.
* `rose/docs/PROMO_VALEUR.md` — ce document.

Aucune modification de `rose/src/**`, d'`emetteur.rose`, d'`emetteur_elf.rose`,
ni d'aucun script. `git status --short` ne montre que ces trois chemins.

## 6. État restant (hors périmètre)

Le blocage (b) de `RELEASE_100.md` reste ouvert : `emetteur_pe_x64.rose` ne
connaît pas les formes `s`/`lst`/tables dans `x_eval()`. Le présent chantier
ne portait que sur (a). Le RIR complet est désormais produit par le moteur C0,
ce qui lève le préalable de la bascule décrite au §3 de `RELEASE_100.md`.
