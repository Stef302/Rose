# DIAG_141 — les 3 derniers BLOQUES du corpus self-host (141/141)

Constat **daté et mesuré** : 2026-11-23T23:49Z UTC (24/11 heure locale).
Worktree exclusif `/tmp/opencode/diag_msg`, HEAD `4af74d9`, sans commit ni
push. Garde-fou `scripts/garde_fou_mem.sh 12000000` utilisé sur chaque
émission C0.

## 1. Résultat

| État | Avant | Après |
|---|---:|---:|
| CONFORMES | 138 | **141** |
| DIVERGENTS | 0 | **0** |
| BLOQUES | 3 | **0** |

Le harnais demandé rend `tests=141 CONFORMES=141 DIVERGENTS=0 BLOQUES=0`,
`ERREURS self-host sur programme accepté par Rust = 0`, **RC=0**.

Contrôle plus strict (rc + stdout **et stderr** octet pour octet sur les
141 sources) : `STRICT tests=141 IDENTIQUES=141 DIVERGENTS=0`.

## 2. Reproduction (avant correction)

Reconstruction depuis le source possédé, avec le C0 existant :

```bash
scripts/garde_fou_mem.sh 12000000 /tmp/opencode/C0_i.elf \
  compilateur/moteur_complet.rose > /tmp/opencode/diag_base.elf
```

ELF aarch64 `1902136` octets, md5 `26d0774dbe1f1a83147e95bd48ece6b2`
(= `/tmp/opencode/moteur_i.elf`, reproductibilité bit à bit).

| Test | Oracle Rust (stderr) | Self (stdout avant, stderr vide) |
|---|---|---|
| `t55_liste_imbriquee` | `Rose : erreur — [Ligne 1, Colonne 0] Listes de listes non supportées pour l'instant` | `|Rose : erreur — [Ligne 1, Colonne 0] Listes de listes non supportées pour l'instant` |
| `t106_hof_param` | `Rose : erreur — [Ligne 1, Colonne 0] Fonction 'fn' inconnue` | `|Rose : erreur — [Ligne 1, Colonne 0] Fonction 'fn' inconnue` |
| `t107_hof_var` | `Rose : erreur — [Ligne 2, Colonne 0] Variable 'carre' utilisée sans valeur` | `|Rose : erreur — [Ligne 3, Colonne 0] Fonction 'f' inconnue` |

rc=1 des deux côtés : seuls le flux et, pour t107, le message différaient.

## 3. Cause profonde

### 3.1 Diagnostic écrit sur stdout avec un `|` préfixé

`compilateur/moteur_complet.rose`, point d'entrée unique (chemin par
défaut `SOURCE.rose`) :

```
$mc_res = lsrc_lire_programme($mc_chemin)
si (longueur($mc_res) < 3 ou sous_chaine($mc_res, 0, 3) != "OK|") {
  >> sous_chaine($mc_res, 3, longueur($mc_res))     # AVANT
  quitter(1)
}
```

Deux erreurs dans cette seule ligne :

1. `>>` écrit sur **stdout** ; l'oracle écrit sur **stderr**. Le harnais
   `verifier_parite_selfhost.sh` classe alors « deux refus mais flux
   différents » en BLOQUE (`cmp -s nat.err self.err` échoue, stdout aussi).
2. `sous_chaine(s, debut, fin)` rend `[debut, fin)`. Le préfixe d'erreur
   est `ERR|` (4 octets) mais celui de réussite est `OK|` (3 octets) :
   réutiliser le décalage 3 laisse le `|` collé au message. Mesure directe :
   `sous_chaine("ERR|abc", 3, 7)` → `|abc`, `sous_chaine("ERR|abc", 4, 7)`
   → `abc`.

Correctif : `cli_err(sous_chaine($mc_res, 4, longueur($mc_res)))`
(`cli_err` = `ecrire_bytes("/dev/stderr", $msg + "\n")`). Le même décalage
3 → 4 est appliqué aux 19 autres branches d'erreur CLI du point d'entrée
(`verif`, `rir`, `comp`, `aot`, …) : c'est la même fuite de diagnostic.
Les branches `OK|` gardent le décalage 3.

### 3.2 t107 : un nom de fonction traité comme une valeur

`lsrc_ce_raw()` (miroir préfixé de `ce_raw()` dans
`lecteur_source.rose`) avait un repli CORR7 qui acceptait n'importe quel
nom présent dans `$fnoms` comme expression de type `fonction|vide`. Dans
`t107_hof_var`, `$f = carre` (ligne 2) était donc accepté ; le refus
n'arrivait qu'à la ligne 3 sur `$f(5)` → `Fonction 'f' inconnue`. L'oracle
Rust n'a pas de valeurs-fonction de première classe et signale dès la
ligne 2 : `Variable 'carre' utilisée sans valeur`.

Correctif : suppression du repli sur `$fnoms`. Seuls les paramètres et
variables déjà présents dans les environnements local/global
(`$t != ""`) restent acceptés. `t106_hof_param` n'est pas affecté : `$fn`
est un paramètre de `applique`, trouvé dans l'environnement, et l'appel
`$fn($x)` continue de produire `Fonction 'fn' inconnue` à la ligne 1.

## 4. Fichiers modifiés

- `compilateur/moteur_complet.rose` — zones de diagnostic uniquement
  (point d'entrée + `lsrc_ce_raw`). Marqueurs vérifiés **inchangés** après
  édition : `bi_pow` ×6, `mc_args` ×2, `rep_run` ×2, `cli_aide_pour` ×2,
  `cands` ×5, `cr_feuille` ×5.
- `compilateur/lecteur_source.rose` — même retrait du repli CORR7 dans
  `ce_raw()`, pour que la source génératrice reste conforme à l'oracle.
- `compilateur/tests_diag.rose` (nouveau) — banc de non-régression.
- `rose/docs/DIAG_141.md` (nouveau) — ce document.

Interdits inchangés : `emetteur.rose`, `emetteur_elf.rose`, `rose/src/**`
(`git diff --name-only` ne montre que les deux fichiers Rose ci-dessus).

## 5. Banc `compilateur/tests_diag.rose`

Vérifié par les deux moteurs, **sortie octet pour octet identique** :

```
tests_diag : 9 OK, 0 ECHEC
```

Il contrôle les 3 formulations exactes, l'extraction du payload `ERR|`
(décalage 4, et l'ancien décalage 3 documenté comme fautif), et la règle
« nom de fonction ≠ valeur » (avec conservation du paramètre-fonction).
Les compteurs sont incrémentés au niveau supérieur : le backend G5 ne
propage pas la mutation d'une globale depuis un corps de fonction.

## 6. Validation

| Contrôle | Preuve |
|---|---|
| Reproduction C0 avant | md5 `26d0774d…` = `moteur_i.elf` |
| Moteur corrigé | ELF `1909060` octets, md5 `c0f90c6ebfb6835b1725f0a6dbec2977` (reconstruit deux fois, même md5) |
| Parité demandée | `141/0/0`, RC=0, `/tmp/opencode/parite_diag_final/` |
| Parité stricte (stderr inclus) | `141 IDENTIQUES / 0 DIVERGENTS` |
| `rose verif` | `lecteur_source.rose`, `moteur_complet.rose`, `tests_diag.rose` → `vérifié`, rc=0 |
| Point fixe émetteur | `b3ed9262dd54776a6dbd07bc12368d0e` (les deux passes) |
| `validation_finale.sh` | **`RESULTAT: 0 échec(s)`**, rc=0, `/tmp/opencode/diag_validation3.log` |

`validation_finale.sh` a été lancé depuis le checkout principal
(`/home/clausake/Stefan/ROSE/scripts/validation_finale.sh`) : le worktree
n'a pas de binaire `rose/rose` compilé, et la batterie AZ en a besoin.
`RUST_TEST_THREADS=1` a été posé pour ce run : deux tests Rust écrivent
un chemin fixe dans `/tmp` (`/tmp/rose-opt-wall-final.rose`,
`/tmp/opencode/rose_m3_2_prog.elf`) et se sont entrechoqués lors des deux
premiers runs (`RESULT:0` absent, puis `Text file busy`). Aucun de ces
deux tests ne porte sur les fichiers modifiés ici ; avec threads=1 la
suite complète passe (`314 + 327 + …`, rc=0).
