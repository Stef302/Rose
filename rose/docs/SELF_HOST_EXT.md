# SELF_HOST_EXT — extension de la couverture du compilateur généré (constat daté, 21/11/2026)

Sous-agent `sh4`. Dépôt `/home/clausake/Stefan/ROSE` (`Stefan/ROSE`), HEAD de départ
`902fbbf`. Deux fichiers possédés : `compilateur/emetteur_elf.rose` et ce document.
**Aucun commit, aucune publication, aucun autre fichier modifié.** Artefacts :
`/tmp/opencode/sh4_agents/`.

## 1. Méthode (mesurée, pas citée)

Chaîne utilisée pour **chaque** programme `p.rose` de la batterie (42 programmes) :

```
./rose/rose p.rose                                > p.nat.out      # exécution native
./rose/rose rir p.rose                            > p.rir          # dump RIR
./rose/rose compilateur/emetteur_elf.rose p.rir   > p.elf          # émission (~0,3 s)
./p.elf                                           > p.elf.out      # binaire émis
cmp p.nat.out p.elf.out                                            # octet pour octet
```

Correction indispensable apportée à la consigne : l'ELF émis **exécute le `$principal`
du programme compilé**, il ne produit pas son dump RIR. Le point fixe SH3 ne comparait
un dump que parce que le programme compilé était `emetteur.rose` lui-même.
Preuve : `arith.rose` (`>> add(mul(6,7),3)`) donne `45` des deux côtés, pas un dump.

Binaire natif utilisé : `./rose/rose` (md5 `12eac46007cf0fe9e1aec2dff6fbf665`).
Le `rose` du PATH (`~/.local/bin/rose`, 14/11, plus ancien) n'a **pas** été utilisé.

Deux runners :
* `/tmp/opencode/sh4_agents/run_tests.sh` — référence = natif avec JIT ;
* `/tmp/opencode/sh4_agents/run_tests_ref.sh` — référence = `ROSE_SANS_JIT=1`.

## 2. Bug de référence découvert — JIT natif (hors périmètre)

Mesures reproductibles, mêmes jetons RIR
`afficher_abstrait (bin < entier 3 entier 5)` :

| Programme | Première ligne | Natif avec JIT | Natif `ROSE_SANS_JIT=1` |
|---|---|---|---|
| `bool.rose` | `>> vrai` | `1 / 0 / 1 / …` | `vrai / faux / vrai / …` |
| `sonde2.rose` | `>> 3 < 5` | `vrai / vrai / …` | `vrai / vrai / …` |
| `sonde5.rose` | `>> 1` | `1 / vrai / vrai` | `1 / vrai / vrai` |
| `sonde_G.rose` | `>> 3 < 5`, puis `>> vrai` | `1 / 1` | `vrai / vrai` |

Le format d'affichage dépend du **programme entier**, pas du RIR. Avec
`ROSE_SANS_JIT=1`, Rose affiche **toujours** `vrai`/`faux` pour tout booléen, y compris
les littéraux `b`. Le défaut est donc dans le JIT de `rose/rose`
(`rose/src/jit.rs`, fichier non possédé, non modifié). C'est la référence
`ROSE_SANS_JIT=1` qui a tranché tous les correctifs de cette mission.

## 3. Correctifs appliqués dans `compilateur/emetteur_elf.rose`

Tous vérifiés par `rose verif` → `vérifié`, et par test minimal natif/ELF avant/après.

1. **Booléens `vrai`/`faux`.** L'ABI G5 n'a pas de tag booléen (0/8 comme entiers).
   `g5_est_bool` analyse statiquement la forme RIR (comparaisons, `b`, `et`/`ou`/`uni
   non`, `cle_existe`, appels `retour booleen`, et slots mémorisés) ; `g5_op` convertit
   `x0` en chaîne `"vrai"`/`"faux"` avant `g5_emit_print`. `g5_parse` capture désormais
   le type de retour (`$g5_fret`) et un suivi de slots booléens (`$g5_sbool`, table
   Rose utilisée uniquement pendant l'émission) permet d'afficher correctement
   `>> $x` et `>> chaine($x)`.
   Preuve : `chaines` `1/0/1` → `vrai/faux/vrai` ; `boolvar` `1/0/1/…` →
   `vrai/faux/vrai/…`. **26 → 39 tests conformes** sur la référence sémantique.

2. **Flottants / rationnels affichés directement.** `>> 7/3` (RIR `(f 7/3)`) donnait
   `0`, `>> 1.5` (RIR `flottant 1.5`) donnait `0`. `g4_ratio_texte` reproduit à
   l'émission le formateur `rose/src/run.rs::texte_rationnel` (7/3 → `2.[3]`,
   5/2 → `2.5`, 1/3 → `0.[3]`, 22/7 → `3.[142857]`, -7/3 → `-2.[3]`) ; `g5_op`
   émet `g5_make_str` pour les littéraux contenant `.` ou `/`. Les flottants à
   valeur entière sont déjà sérialisés `flottant 2` par Rose et gardent leur voie.
   Preuve : `arith2` `0` → `2.[3]` ; `sonde_B` `0` → `1.5` ; `sonde_float` 7 lignes OK.

3. **`cle_existe` avec clé entière — SIGSEGV.** `masktag` de la clé produisait un
   pointeur nul, puis `streq` lisait `[0+8]`. `g5_emit_clex` dispatche maintenant sur
   le tag : chaîne → comparaison de contenu (chemin historique), entier/booléen →
   comparaison des valeurs taguées. Preuve : `tab2` rc=139/0 octet → rc=0,
   `vrai/vrai/faux`. Une première version avait inversé `b.eq`/`b.ne` (mesuré :
   crash dans `streq` avec x0=x1=8 sous gdb) ; corrigée et re-mesurée.

## 4. Résultat de la batterie (référence `ROSE_SANS_JIT=1`)

**39 conformes / 43 tests, 0 erreur d'émission** (dont `fib25` → 75025 des deux
côtés). Runners et programmes dans
`/tmp/opencode/sh4_agents/tests/` et `run_tests_ref.sh`.

Divergences restantes, toutes re-mesurées, aucune non documentée :

| Test | Natif | ELF | Cause |
|---|---|---|---|
| `grand` | `123456789012345678901234567890`, `18446744073709551616`, `…`, `31` | `248789718555101906`, `0`, `100000000000000000`, `1` | ABI G5 : entiers limités à 64 bits (pas de BigInt), `2^64` déborde. Correctif = changement d'ABI, hors de ce chantier. |
| `ioarg` | `0` puis erreur « index hors des limites des arguments », rc=1 | `0`, rc=139 | `argument(0)` sans argument : l'ELF n'a pas de garde de borne. Sortie stdout identique (`0\n`), seul le rc diffère. |
| `sonde_C` | `[1, 2]` | `201328640` (adresse du descripteur) | `g5_emit_print` ne formate pas le tag 2 (liste). Correctif = routine d'impression récursive, non appliquée (risque de régression du point fixe). |
| `sonde_float2` | `1e+20` | `848750603811160064` | Flottant > 2^63 : le natif passe en notation exponentielle, l'ABI G5 déborde. |

Avec le JIT natif (runner `run_tests.sh`), 5 cas supplémentaires divergent uniquement
parce que le JIT affiche `1/0` : ce sont les cas documentés au §2, l'ELF suit la
sémantique Rose.

## 5. Point fixe — re-vérifié avec TOUS les correctifs

Chaîne complète rejouée après le dernier correctif (fichier émetteur md5
`e668fd426a65ebc1d66138020a01e21a`) :

| Étape | Commande | Résultat mesuré |
|---|---|---|
| 1 | `./rose/rose rir compilateur/emetteur.rose > self.rir` | rc=0, 263 520 o, `b3ed9262dd54776a6dbd07bc12368d0e` |
| 2 | `./rose/rose compilateur/emetteur_elf.rose self.rir > self_final2.elf` | **rc=0**, 534 856 o, `041edb582795e968a3fdebc5345dab4f`, ~374 s |
| 3 | `chmod +x self_final2.elf && ./self_final2.elf compilateur/emetteur.rose > dump_final2.out` | **rc=0**, 263 520 o, `b3ed9262dd54776a6dbd07bc12368d0e` |
| 4 | `cmp self.rir dump_final2.out` | **identique octet pour octet** |

Une émission intermédiaire (correctif booléen seul, fichier md5
`cbe07c529357eb1f69b36b8a6b6808b5`) avait déjà donné un point fixe identique :
ELF `ed699457a9b8cd0ea69a53bbae78fd39`, 534 780 o, dump `b3ed9262…`. L'ELF final
est plus gros de 76 octets (routines supplémentaires), mais la sortie réflexive est
**strictement inchangée**. Le point fixe n'a donc pas été cassé.

Le premier lancement du ELF intermédiaire a donné rc=126 (« Permission denied ») :
la redirection `>` crée le fichier en 0644. Après `chmod +x`, rc=0. Ce n'était pas
une régression du compilateur.

## 6. Validation finale obligatoire

| Contrôle | Résultat mesuré |
|---|---|
| `./rose/rose verif compilateur/emetteur_elf.rose` | **`vérifié`** |
| `self.rir` md5 | **`b3ed9262dd54776a6dbd07bc12368d0e`** (263 520 o) |
| ELF émis | **rc=0**, 534 856 o, `041edb582795e968a3fdebc5345dab4f` |
| Dump ELF vs RIR | **identique octet pour octet** (`cmp`) |
| `cargo test --release` (depuis `rose/`, `PATH=~/.cargo/bin:$PATH`) | **rc=0, 431 passés, 0 échec** (220+204+0+6+1+0 ; 3+3 ignorés) |
| Jardins `./scripts/rose-jardin.sh verif-tous` | **10/10 valides**, rc=0 — la consigne annonçait 20/20 : 10 est le nombre RÉEL de jardins présents dans le dépôt |
| `fib25` natif vs ELF | **75025 / 75025** |

`cargo` n'est pas dans le PATH par défaut sur cette machine (binaire
`~/.cargo/bin/cargo` → rustup) ; la commande a été lancée avec
`export PATH=/home/clausake/.cargo/bin:$PATH`. Aucun fichier Rust n'a été modifié par
ce sous-agent (`git status` montre `rose/src/i128x.rs` modifié par un autre agent,
non touché ici).

## 7. État final

- Fichier livré : `compilateur/emetteur_elf.rose`, md5
  `e668fd426a65ebc1d66138020a01e21a`, `rose verif` → `vérifié`.
- Document livré : `rose/docs/SELF_HOST_EXT.md` (ce fichier).
- Artefacts : `/tmp/opencode/sh4_agents/` (batterie `tests/`, runners
  `run_tests.sh` / `run_tests_ref.sh`, ELF `self_final.elf` / `self_final2.elf`,
  dumps, logs `cargo_test.log`, sauvegarde `emetteur_elf.rose.BASE` md5
  `e99437a2a73610c3e02a9dfc70b12129`).
- `rose/rose` inchangé : md5 `12eac46007cf0fe9e1aec2dff6fbf665`.
- `compilateur/emetteur.rose` inchangé : md5 `390335d29fb9cee368b3456dd892e377`.
- **Aucun commit, aucune publication, aucun fichier interdit modifié.**

## 8. Reste (documenté, non corrigé ici)

1. Entiers au-delà de 64 bits (ABI G5) : `2^64` → 0, grands décimaux tronqués.
   Correctif = passage à des entiers multiples de 64 bits dans l'ABI émise.
2. `argument(i)` hors limites : erreur contrôlée rc=1 côté natif, rc=139 côté ELF
   (sortie stdout identique). Garde de borne à ajouter dans `g5_emit_argument`.
3. Affichage d'une liste (`>> [1, 2]`) : l'adresse du descripteur est écrite au lieu
   de `[1, 2]` (`g5_emit_print` ne gère que les tags 0 et 1). Correctif = routine
   d'impression récursive du tag 2, non appliquée pour préserver le point fixe.
4. Flottants >= 2^63 : notation exponentielle native `1e+20` non reproduite.
5. Bug du JIT natif (`ROSE_SANS_JIT=1` vs défaut) documenté au §2 : fichier
   `rose/src/jit.rs` hors de ce périmètre.
