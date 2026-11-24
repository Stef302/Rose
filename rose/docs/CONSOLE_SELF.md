# CONSOLE SELF — démarrage de `console` / `repl` depuis le moteur auto-hébergé (24/11/2026)

Worktree exclusif `/tmp/opencode/az_repl`, HEAD `162d7a2`. Aucun commit,
aucun push. Garde-fou `bash scripts/garde_fou_mem.sh 12000000 …` sur toutes
les commandes longues (émissions C0, bancs, parité, validation).

| Fichier | Rôle |
|---|---|
| `compilateur/moteur_complet.rose` | **uniquement le dispatch `console`/`repl` du point d'entrée** |
| `compilateur/tests_console_self.rose` | nouveau banc de tests (6 assertions) |
| `rose/docs/CONSOLE_SELF.md` | ce document |

`rose/src/**`, `compilateur/repl_rose.rose` (807 lignes, 56 `fn rep_*`),
`compilateur/emetteur*.rose` : **lecture seule**, aucun octet modifié.

---

## 1. Ce qui a été lu

* `compilateur/repl_rose.rose` : `rep_run($src: chaine) -> chaine` (l. 713-801)
  et son `BEGIN/END REP_MAIN` (`>> rep_run(lire_texte(argument(0)))`).
  `rep_split_lines` (l. 640) ignore `\r`, pose un sentinel vide en tête,
  `rep_trim` nettoie chaque entrée ; `rep_run` retourne la transcription
  **bannière + invites + résultats**, sans `\n` final ajouté par `>>`.
* `compilateur/moteur_complet.rose` (point d'entrée) : anciennement, 0 argument
  écrivait l'aide sur stderr (rc=1) et `console` / `repl` tombaient dans le
  chemin fichier normal (« fichier introuvable »).
* `rose/docs/HORDE_REPL.md` §3.3, `rose/docs/CORR11_REPL.md` §4 : l'oracle est
  `ROSE_NO_COLOR=1 rose console < script.txt` (2 090 octets pour les
  10 commandes du protocole).

## 2. Câblage (point d'entrée seul)

```rose
si ($mc_n == 0 ou $mc_cmd == "console" ou $mc_cmd == "repl") {
  $mc_script = "\n"
  $mc_sans_fichier = 1
  si ($mc_n >= 2) { $mc_script = lire_texte(argument(1)); $mc_sans_fichier = 0 }
  si (longueur($mc_script) == 0) { $mc_script = "\n"; $mc_sans_fichier = 1 }
  $mc_transcript = rep_run($mc_script)
  # En fin d'entrée sans :quit, le REPL Rust referme l'invite par un '\n'.
  si ($mc_sans_fichier == 1) { $mc_transcript = $mc_transcript + "\n" }
  ecrire_bytes("/dev/stdout", $mc_transcript)
  quitter(0)
}
```

* **`console` / `repl` avec fichier** → `rep_run(lire_texte(argument(1)))` ;
* **sans fichier** (`console` seul, `repl` seul, ou 0 argument) → script
  sentinel `"\n"` = bannière + invite `rose[jit]> ` + `\n` (996 octets,
  octet-identique à `rose console < /dev/null`) ;
* **sinon** → le chemin fichier normal (`verif`, `rir`, `comp`, `bc`,
  `transpile`, `methode`, `run`… et `SOURCE.rose`) est intact.

`ecrire_bytes` (et non `>>`) est indispensable pour l'égalité OCTET POUR
OCTET sur les 10 commandes : sous C0 il écrit la chaîne Rose en UTF-8
(mesuré : `╭─ é` → 9 o), alors que `>>` ajouterait un `\n` final
(2 091 o au lieu de 2 090). Le `\n` de fin d'entrée du REPL Rust est ajouté
**uniquement** dans le cas sans fichier, où la transcription se termine sur
l'invite (les scripts à `:quit`, dont les 10 commandes, se terminent sans
nouvelle ligne des deux côtés). Les opérandes de `ou` sont tous sans
indexation (piège HORDE_REPL §7.2).

`moteur_complet.rose` est généré par `scripts/build_moteur_sans_rust.sh` :
ce script n'a PAS été modifié (hors périmètre) ; une régénération devra
reporter ce bloc. Constat de propriété, pas une limite technique.

## 3. Banc de tests — `compilateur/tests_console_self.rose`

Concaténation (`moteur_complet.rose` privé de son point d'entrée + le banc)
→ `rose verif` : **vérifié**, rc=0. Six assertions :

1. transcription dorée des 10 commandes (2 090 octets UTF-8) ;
2. console vide : bannière + `rose[jit]> ` ;
3. aller-retour `ecrire_texte` / `lire_texte` du script ;
4. `rep_run(lire_texte(fichier))` rend la même transcription ;
5. une entrée sans fichier s'arrête après `2 + 3` → `5` ;
6. `:quit` arrête la boucle même s'il reste des lignes.

Résultat : **`TESTS CONSOLE SELF : 6 OK, 0 ECHEC`**, rc=0, stderr 0 —
avec le moteur Rust `rose/rose` **et** avec l'ELF C0 (1 861 964 octets,
magic `7f454c4602010100`). Le binaire `rose/target/release/rose` construit
le 24/11 par un autre chantier replie mal les longues concaténations de
chaînes de ce banc (`longueur($banniere)` = 2 au lieu de 760) : c'est un
constat sur CE binaire-là, sans rapport avec le câblage (le même banc passe
sur `rose/rose` et sur l'ELF C0).

## 4. Parité avec `rose console` (oracle Rust, `ROSE_NO_COLOR=1`)

Script `input10.txt` : `2 + 3`, `10!`, `$x = 7`, `$x * $x`, `5 > 3`,
`"ab" + "cd"`, `:methode interprete`, `:show`, `:help`, `:quit`.

| Exécution (`final2.elf`) | Sortie | `cmp` avec l'oracle |
|---|---:|---|
| `console input10.txt` | 2 090 o, rc=0, stderr 0 | **IDENTIQUE** |
| `repl input10.txt` | 2 090 o, rc=0, stderr 0 | **IDENTIQUE** |
| 0 argument | 996 o, rc=0, stderr 0 | **IDENTIQUE** à `rose console < /dev/null` |
| script 7 commandes (`:methode aot`, `4*5`, `:type 42`, `:reset`, `:show`, `2^10`, `:quit`) | 1 150 o | **IDENTIQUE** |
| `tests_selfhost/t01_entiers.rose` (chemin fichier normal) | 5 lignes | **IDENTIQUE** à `rose tests_selfhost/t01_entiers.rose` |

ELF final : `bash scripts/garde_fou_mem.sh 12000000 /tmp/opencode/C0_141.elf
compilateur/moteur_complet.rose` → rc=0, **1 967 496 octets**, magic
`7f454c4602010100`, md5 `4e78981f1be3827ff807402c5ad8aa01`, stderr 0.

## 5. Non-régression

Toutes les commandes ont été exécutées sous le garde-fou (borne 12 000 000 Ko
jamais atteinte).

| Contrôle | Preuve mesurée |
|---|---|
| `rose verif compilateur/moteur_complet.rose` | `vérifié`, rc=0 |
| Parité self-host 141 sources | `tests=141 CONFORMES=141 DIVERGENTS=0 BLOQUES=0`, `ERREURS self-host sur programme accepté par Rust = 0`, **rc=0** |
| Banc RIR / formats | `TESTS RIR : 15 OK, 0 ECHEC` ; `TESTS FORMATS : 25 OK, 0 ECHEC` |
| Point fixe `b3ed9262` | `rose rir emetteur.rose` et `rose emetteur.rose emetteur.rose` → `b3ed9262dd54776a6dbd07bc12368d0e` ×2, `cmp`=0 |
| `validation_finale` (sections 2–7, copie privée) | **`=== RESULTAT: 0 échec(s) ===`**, rc=0 |
| Détail validation | AOT **75 OK / 0 ECHEC** ; JIT **25/25** ; WASM **92 o** ; RVM **317811** ; AZ **31 CONFORME, 0 non-conforme** ; HOST-1 **fib=317811** |
| Marqueurs conservés | `bi_pow` ×6, `mc_args` ×2, `rep_run` ×3 (2 + l'appel ajouté), `cli_aide_pour` ×2, `cands` ×5, `cr_feuille` ×5, `sw_module` ×3, `sb_module` ×5 |
| Fichiers interdits | `git status --short` : seuls les 3 fichiers possédés apparaissent |

⚠ Deux constats de mesure, tous deux résolus et non des régressions :

* la première exécution de `validation_finale` a échoué sur AOT + AZ parce
  que ce worktree ne contenait pas de binaire `rose/rose` (chemins codés en
  dur dans les scripts) ; copie de l'oracle dans `rose/rose` (artefact
  gitignoré) puis `ROSE=<binaire avec helper AOT>` → **0 échec** ;
* un passage de `tests_formats.rose` a affiché « ECHEC : marqueur mc_args »
  parce qu'il lisait `moteur_complet.rose` pendant une édition du fichier ;
  passage propre sur source stable → **25 OK, 0 ECHEC**.

## 6. État final

Câblage en place, ELF C0 construit, parité octet pour octet démontrée sur
10 + 7 commandes, console vide et chemin fichier ; banc 6/6 ; parité 141/141 ;
formats 15/15 et 25/25 ; validation 0 échec ; point fixe intact. Aucun
fichier interdit modifié.

## 7. Rejouer

```sh
cd /tmp/opencode/az_repl
R=/home/clausake/Stefan/ROSE/rose/target/release/rose
C0=/tmp/opencode/C0_141.elf
A=/tmp/opencode/az_repl_art

bash scripts/garde_fou_mem.sh 12000000 "$R" verif compilateur/moteur_complet.rose
bash scripts/garde_fou_mem.sh 12000000 "$C0" compilateur/moteur_complet.rose > "$A/final2.elf"
chmod +x "$A/final2.elf"
ROSE_NO_COLOR=1 "$R" console < "$A/input10.txt" > "$A/ref10.out"
"$A/final2.elf" console "$A/input10.txt" > "$A/elf10.out"
cmp "$A/ref10.out" "$A/elf10.out"          # 2 090 o, rc=0
"$A/final2.elf" > "$A/elf0.out"
ROSE_NO_COLOR=1 "$R" console < /dev/null > "$A/ref0.out"
cmp "$A/ref0.out" "$A/elf0.out"            # 996 o, rc=0
```
