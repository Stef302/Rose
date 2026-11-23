# CORR 11 — Démarrage de la boucle REPL depuis le moteur self-hosté (23/11/2026)

Worktree exclusif `/tmp/opencode/corr_b11`, HEAD `d117e04`.
Fichiers possédés (les trois livrés) :

| Fichier | Rôle |
|---|---|
| `compilateur/moteur_complet.rose` | **uniquement le point d'entrée** (fin de fichier) |
| `compilateur/tests_b11.rose` | nouveau banc de tests (4 assertions) |
| `rose/docs/CORR11_REPL.md` | ce document |

Aucun autre fichier du dépôt n'a été modifié : ni `lsrc_rpui`, ni `call_fn`,
ni le dispatch CLI (d'autres agents les modifient dans leurs worktrees).
Aucun commit, push ou publication. Tous les artefacts d'essai sont hors dépôt,
dans `/tmp/opencode/b11_art/`.

---

## 1. Ce qui a été lu

* `compilateur/repl_rose.rose` : la boucle `rep_run($src: chaine) -> chaine`
  (l. 713-801) et son `BEGIN/END REP_MAIN` (l. 804-807) :
  `>> rep_run(lire_texte(argument(0)))`. Le **protocole d'entrée scriptée**
  est donc un texte à **une entrée de console par ligne**, passé à `rep_run`
  sous forme de chaîne ; `rep_split_lines` ignore le `\r`, `rep_trim` nettoie,
  la première ligne est un sentinel. `rep_run` retourne la transcription
  complète (bannière + invites + résultats) **sans le `\n'` final** qu'ajoute
  l'opérateur `>>` du point d'entrée. Les 56 `fn rep_*` sont toutes présentes
  dans `moteur_complet.rose` (vérifié : `grep -c '^fn rep_'` = 56).
* `rose/src/repl.rs` `lancer()` (l. 525-600) et `rose/src/main.rs` (l. 1799) :
  `rose console [fichier]` lit stdin ligne par ligne en pipe, d'où l'oracle
  `ROSE_NO_COLOR=1 rose/rose console < script.txt`.
* `compilateur/moteur_complet.rose:9982-10001` : l'ancien point d'entrée
  n'avait qu'un mode (`lsrc_lire_programme` → `parse_rir` → `call_fn`) et
  refusait 0 argument avec `usage : moteur_complet SOURCE.rose`, rc=2.

`lire_texte` ayant une sémantique **fichier** (mmap, `ChaineRep::depuis_fichier`
dans `rose/src/interp.rs:5920`), il ne lit pas un pipe : la branche console
prend donc son script dans un **fichier** passé en argument, exactement comme
le `REP_MAIN` de `repl_rose.rose`.

## 2. Câblage (point d'entrée SEUL)

```
$mc_cmd = ""
si (nb_arguments() >= 1) { $mc_cmd = argument(0) }
si (nb_arguments() < 1 ou $mc_cmd == "console" ou $mc_cmd == "repl") {
  $mc_script = ""
  si (nb_arguments() >= 2) { $mc_script = lire_texte(argument(1)) }
  si (longueur($mc_script) == 0) { $mc_script = "\n" }
  >> rep_run($mc_script)
  quitter(0)
}
$mc_chemin = $mc_cmd          # chemin fichier normal, inchangé ensuite
...
```

* **0 argument** → console vide : `"\n"` produit une seule invite
  `rose[jit]> `, octet-identique à `rose console < /dev/null`.
* **`console` / `repl`** → `rep_run(lire_texte(argument(1)))`.
* **sinon** → l'ancien chemin fichier est intact (`fib.rose` → `6765`).

Le `>>` est indispensable : C0 élimine `afficher("x")` (piège HORDE_REPL §7.1).
Les operands de `ou` sont tous sans indexation (piège de l'évaluation
empressée, §7.2). `quitter(0)` est dans un `si` comme l'ancien `quitter(2)`.

⚠ `moteur_complet.rose` est **généré** par `scripts/build_moteur_sans_rust.sh` :
ce script n'a PAS été modifié (hors périmètre). Une régénération devra
reporter ce bloc — c'est un constat de propriété, pas une limite technique.

## 3. Tests — `compilateur/tests_b11.rose`

Concaténation (`repl_rose.rose` sans `BEGIN/END REP_MAIN` + le banc, 909 lignes)
→ `rose verif` : **vérifié** (rc=0). Le banc contient 4 assertions :

1. **transcription des 10 commandes** — script `input10.txt` :
   `2 + 3`, `10!`, `$x = 7`, `$x * $x`, `5 > 3`, `"ab" + "cd"`,
   `:methode interprete`, `:show`, `:help`, `:quit`. La transcription
   attendue est reconstruite en 3 globales + bannière (piège §7.3) et
   comparée **octet pour octet** à `rep_run($script10)`.
2. **console vide** — `rep_run("\n")` = bannière + `rose[jit]> `.
3. **`lire_texte` du script** — `ecrire_texte` puis relecture du fichier.
4. **`rep_run(lire_texte(fichier))`** — la transcription dorée.

Résultat : `TESTS B11 : 4 OK, 0 ECHEC`, rc=0, stderr 0 octet — avec le moteur
Rust **et** avec l'ELF C0 (194 752 o, magic `7f454c4602010100`).

## 4. Parité avec `rose console` (oracle Rust, `ROSE_NO_COLOR=1`)

Commande : `ROSE_NO_COLOR=1 rose/rose console < input10.txt` → **2 090 o**.

| Exécution | Sortie | Comparaison |
|---|---|---|
| `rose/rose moteur_complet.rose console input10.txt` | 2 091 o | `rstrip('\n')` **égal** |
| ELF C0 `moteur_complet.elf console input10.txt` | 2 091 o | `rstrip('\n')` **égal**, stderr 0 |
| ELF C0 `moteur_complet.elf repl input10.txt` | 2 091 o | `cmp` = 0 avec `console` |
| ELF C0 `moteur_complet.elf` (0 argument) | 996 o | `cmp` = 0 avec `rose console < /dev/null` |
| ELF C0 `moteur_complet.elf fib.rose` | `6765` | chemin fichier intact |
| script 7 commandes (`:methode aot`, `4*5`, `:type 42`, `:reset`, `:show`, `2^10`, `:quit`) | 1 150 / 1 151 o | `rstrip('\n')` **égal** |

L'unique octet de différence est le `\n'` ajouté par `>>` côté Rose ; le REPL
Rust ne l'ajoute pas sur `:quit`. Normalisation déterministe, déjà documentée
dans `HORDE_REPL.md` §3.3.

## 5. Validation

Toutes les commandes ont été **exécutées** depuis `/tmp/opencode/corr_b11` sous
`bash scripts/garde_fou_mem.sh 12000000 …` (borne 12 000 000 Ko jamais atteinte).

| Contrôle | Preuve mesurée |
|---|---|
| `rose verif compilateur/moteur_complet.rose` | `vérifié`, rc=0 |
| `rose verif <concaténation tests_b11>` | `vérifié`, rc=0 |
| C0 → ELF `moteur_complet.rose` | rc=0, **1 405 852 o**, magic `7f454c4602010100`, md5 `f53f7390f2da0d5826d1e3a07d355dae`, 5 min 51 s, stderr 0 |
| C0 → ELF banc de tests | rc=0, 194 752 o, `TESTS B11 : 4 OK, 0 ECHEC` |
| Tests du banc (Rust + ELF) | `4 OK, 0 ECHEC`, rc=0 ×2 |
| Point fixe b3ed9262 | `rose rir emetteur.rose` **et** `rose emetteur.rose emetteur.rose` → `b3ed9262dd54776a6dbd07bc12368d0e` ×2, 263 520 o, `cmp` = 0, stderr 0 |
| `validation_finale` | **rc=0, `=== RESULTAT: 0 échec(s) ===`** (`/tmp/opencode/b11_art/vf/validation_finale.log`) |

Détail de `validation_finale` : Rust 314 + 327 passés / 0 échec ; point fixe
OK ; AOT 75/75 ; JIT 25/25 ; WASM 92 o ; RVM 317811 ; AZ 31 CONFORME, 0
non-conforme ; HOST-1 fib=317811.

⚠ **Deux exécutions, une lesson de mesure** : le premier passage avec le
script partagé `scripts/validation_finale.sh` a donné 1 échec (`HOST-1`,
`out=` vide) parce que **plusieurs agents écrivaient simultanément**
`/tmp/opencode/vf_host.elf` (`/bin/bash: Text file busy`, fichier truncé à 0 o).
Le script a alors été **recopié dans `/tmp/opencode/b11_art/vf/` avec des
chemins privés** (aucun fichier du dépôt modifié, aucun autre agent dérangé) ;
le passage privé donne **0 échec**. C'est une collision de chemins partagés,
pas une régression du câblage.

## 6. État final

Câblage en place, tests verts, parité démontrée sur 10 + 7 commandes, point
fixe intact, validation_finale à 0 échec. Les seules modifications suivies par
git sont `compilateur/moteur_complet.rose` (point d'entrée) et le nouveau
`compilateur/tests_b11.rose`, plus ce document.
