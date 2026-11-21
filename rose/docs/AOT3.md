# AOT3 — AOT des valeurs exactes (`>> 565#`, `>> 100!`)

Vague 78 du `PLAN.md`. Aucune publication, aucun commit demandé pour ce
chantier (un commit intermédiaire a été créé par une autre session pendant
les mesures ; les derniers tests de garde-fou restent dans l'arbre de
travail).

## 1. Problème mesuré

```
$ cat prime.rose
>> 565#
$ ./rose comp prime.rose          # OK, 229 octets = 228 chiffres + '\n'
$ ./rose aot prime.rose -o prime
rose: compilation native impossible : le programme ne rend pas de valeur calculable directement
```

Cause : `Expr::Primorielle` n'est pas dans le sous-ensemble de `rir::abstraire`.
Le RIR contient `afficher (prim (n 565))`, qui devient une opération
`Afficher` non émettable ; `abaisser_principal_sortie` rend `None` et
`cmd_llvm::orc()` refuse. Le chemin RIR ne sait de toute façon représenter
qu'un `i64`, alors que `565#` a **228 chiffres** (le « 229 chiffres » de la
consigne compte le `'\n'` de la sortie de l'interpréteur).

## 2. Solution retenue : voie « AOT valeurs exactes » (option 1)

Un programme réduit à un unique `>> <terme constant>` n'a rien à exécuter
dans le binaire : sa valeur est **entièrement déterminée à la compilation**.
On la calcule une fois avec l'interpréteur Rose (`front` + `Interp::eval`),
puis on mémorise son texte décimal dans le binaire natif. Le stub d'entrée
écrit ce texte par un seul `write(1, adresse, longueur)` ; le code machine
rend la partie basse i64 de la valeur, donc le code de retour garde la
sémantique historique pour les petites valeurs.

Fichiers touchés :

| Fichier | Changement |
|---|---|
| `rose/src/rir.rs` | `est_constant(e)` (littéraux, binaires, unaires, `!`, `#`, `?`, `$`, `!!`… ; **aucun appel, aucune variable**) et `abaisser_principal_valeur_exacte(corps, retour_i64)`. Ces deux fonctions sont **nouvelles** : `rose rir` et le point fixe `b3ed9262` gardent leur sortie. |
| `rose/src/natif_emetteur.rs` | `ProgrammeEmis::sortie: Option<Vec<u8>>` (`None` partout dans le chemin historique) + `emettre_valeur_exacte()`, qui réutilise `emettre_programme` sur un corps `retour <i64>`. |
| `rose/src/link_natif.rs` | `Options::sortie_exacte` (par défaut `None`) ; `.rodata` contient le texte ; `sequence_ecrire_donnees_x86` (`movabs` + `syscall write`) et `sequence_ecrire_donnees_a64` (`movz`/`movk` + `svc #0`). Longueur **fixe** quelle que soit l'adresse, ce qui permet de calculer `stub_len` avant de placer `.rodata`, puis de régénérer le stub avec la vraie adresse. Les stubs historiques (`afficher_valeur`, PE) sont octet pour octet inchangés. |
| `rose/src/cmd_llvm.rs` | `valeur_constante_aot()` : détection + évaluation via `Interp`, texte construit exactement comme `run::afficher` ; nouvelle voie dans `orc()` **avant** tout abaissement RIR. Refus explicite sur PE (stub Win64 sans syscall Unix) avec conseil `rose comp`. |

`Valeur::Entier(i, Some(z))` est converti par `Gz::chaine()` — la même
fonction que celle utilisée par l'interpréteur — donc l'égalité est
octet pour octet. Les 64 bits bas sont calculés depuis le `BigInt`
(`& BigInt::from(u64::MAX)`) pour le code de retour.

## 3. Preuves d'exécution (aarch64, Linux, 21/11/2026)

Build : `cargo build --release` → rc 0 ; binaire copié dans `rose/rose`,
`rose/rose-20` et `rose-20` (md5 `ed61bf38f90f03343c5aadbd0a19ec1a`).

```
$ ./rose aot prime.rose -o prime_nat
rose: prime_nat produit (exécutable natif aarch64, valeur exacte mémorisée)
$ ./rose prime.rose > ref.txt ; ./prime_nat > nat.txt
$ wc -c ref.txt nat.txt
229 229
$ cmp ref.txt nat.txt && echo IDENTIQUE
IDENTIQUE
$ head -c 40 nat.txt
80825764303593910376900621425689673982...
$ tail -c 30 nat.txt
702156042073079776431677671930
md5(nat.txt) = 4f72c972eb4a62d62a568d3ad9f06926
```

Désassemblage du stub (`objdump -d`) : `mov x1, #0x190` + `movk x1, #0x40, lsl #16`
→ adresse de `.rodata` ; `mov x2, #0xe5` = 229 octets ; `svc #0` (`write`)
puis `svc #0` (`exit`). Le code machine émis (`rose_code`) ne contient que
`retour <partie basse>`.

`>> 100!` :

```
$ wc -c ref.txt nat.txt
159 159
$ cmp ref.txt nat.txt && echo IDENTIQUE
IDENTIQUE
md5(nat.txt) = 72a76572cfb7b8464749bbdb3b2ec69f
```

Autres termes constants vérifiés de la même manière (sortie identique à
l'interpréteur pour chacun) : `20?`, `30$`, `!10`, `7!!`, `2^100`, `10#`.

Désassemblage vérifié aussi sur la séquence x86-64 produite (movabs + write),
non exécutable sur cet hôte aarch64.

## 4. Non-régression

| Contrôle | Résultat |
|---|---|
| `cargo build --release` | rc 0 |
| `cargo t --release` | **291 passed, 0 failed, 3 ignored** (lib/binaire `rose`) ; + 6 (`rose-shell`), + 5 (`rose-tray`), + 0 (`rose-setup`) — **302 tests, 0 échec** |
| Tests ajoutés | `aot3_primorielle_565_est_une_valeur_exacte`, `aot3_refuse_une_expression_avec_variable_ou_appel`, `elf_avec_valeur_exacte_ecrit_le_texte_memorise` (exécute réellement l'ELF produit) |
| Point fixe `b3ed9262` ×2 | `./rose rir compilateur/emetteur.rose` et `./rose compilateur/emetteur.rose compilateur/emetteur.rose` → md5 `b3ed9262dd54776a6dbd07bc12368d0e` **×2**, 263 520 o, `cmp = 0`, stderr vide |
| Parité JIT ↔ interprète (`bench/suite/run_parite_jit.sh`, 119 programmes) | base `40f412a` : 111 ok / 8 div. puis 112 ok / 7 div. ; **modifié : 111 ok / 8 div. puis 111 ok / 8 div.** — même ensemble de programmes (horodatages et hasard), aucune divergence nouvelle |
| `rose/tests/porte_unique.py` | **49/53** avant et **49/53** après (5 écarts préexistants avec le moteur C++ obsolète) |
| `rose comp` | inchangé (aucun module utilisé par `comp` modifié) |

⚠ Piège de mesure rencontré : un `ROSE_BIN` pointant vers un binaire
inexistant fait croire à « 119 ok / 0 divergence » (le script compare la
sortie de `sed`, qui vide la pipeline). La référence a donc été reconstruite
depuis `git archive 40f412a` dans `/home/clausake/rose-base-aot3` et
vérifiée présente avant chaque comparaison.

## 5. Limites

- La voie exacte ne s'active que pour un programme **unique** `>> <terme
  constant>` (expression sans variable ni appel). Tout le reste suit le
  chemin RIR historique, inchangé.
- PE/Windows refuse cette voie avec un message explicite (`rose comp` en
  repli) : le stub Win64 appelle `ExitProcess` via l'IAT et n'a pas de
  syscall Unix `write`. Ajouter `WriteFile` à la table d'imports reste un
  chantier séparé.
- Le code de retour d'un grand entier n'en porte que les 64 bits bas (8 bits
  observés par le shell), comme pour le chemin i64 historique ; **la sortie
  stdout, elle, est complète et exacte**.
