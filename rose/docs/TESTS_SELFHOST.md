# TESTS_SELFHOST — batterie SH9 (constat mesuré, 20/11/2026 12:53 UTC / 21/11 local)

Sous-agent `sh9`. Dépôt `/home/clausake/Stefan/ROSE` (`Stefan/ROSE`), HEAD de départ
`b8875b6`. Fichiers livrés : `tests_selfhost/*.rose` (80 programmes),
`tests_selfhost/lancer.sh`, ce document. **Aucun fichier source existant modifié,
aucun commit, aucune publication.** Artefacts : `/tmp/opencode/sh9_agents/`.

## 1. Ce qui a été trouvé au départ (honnêteté sur la consigne)

La consigne annonçait 43 tests dans `/tmp/opencode/sh4_agents/tests/`. **Ce dossier
n'existe plus** (`ls /tmp/opencode/` : seulement `sh5_agents`, `sh6_agents`). Les
seuls survivants sont 5 fichiers dans `/tmp/opencode/sh6_agents/tests/` :
`grand.rose`, `floatbig.rose`, `argout.rose`, `tabval.rose`, `liste.rose`. Ils ont
été repris (contenu identique ou équivalent) dans la batterie. Les autres tests
ont été **reconstruits** d'après les noms et les cas décrits dans
`rose/docs/SELF_HOST_EXT.md` (arith, bool, chaines, boolvar, arith2, sonde_B,
sonde_float, tab2, fib25, …). Ce n'est donc pas une copie littérale des 43 tests
SH4 : c'est une batterie équivalente, vérifiée par des mesures neuves.

Autre fait mesuré : `compilateur/emetteur_elf.rose` a été modifié par d'autres
agents **pendant** ce chantier (md5 `e668fd42…` → `8b6aab1e…` → `aa0cf589…` en
quelques minutes). Toutes les mesures ci-dessous sont donc figées sur un snapshot
pris au début de la salve : **md5 `8b6aab1e175ad3150e555176adf60963`**
(`/tmp/opencode/sh9_agents/emetteur_snapshot.rose`), binaire Rose
`12eac46007cf0fe9e1aec2dff6fbf665`. Le runner refige automatiquement l'émetteur
au début de chaque exécution et affiche son md5 : ne pas comparer ces chiffres à
une salve faite avec un autre md5.

## 2. Protocole

Pour chaque `tests_selfhost/*.rose`, `tests_selfhost/lancer.sh` exécute :

```
ROSE_SANS_JIT=1 rose p.rose            > p.nat      # référence sémantique
rose rir p.rose                        > p.rir      # dump RIR, 2 fois
rose <emetteur_figé> p.rir             > p.elf      # ELF émis, 2 fois
chmod +x p.elf && ./p.elf              > p.elfout   # sortie ELF
```

Comparaisons **octet pour octet** : `p.nat` vs `p.elfout`, `p.rir` vs second
`p.rir`, `p.elf` vs second `p.elf`. Timeouts : 25 s (nat/rir/ELF), 90 s
(émission). Classification :

- **CONFORME** : les 4 étapes ont rc=0 et les 3 comparaisons sont identiques ;
- **DIVERGENT** : tout s'exécute (rc=0) mais au moins une comparaison échoue ;
- **BLOQUE** : rc≠0, timeout ou artefact absent — l'étape exacte, le rc et le
  stderr sont consignés dans `rapport.txt` et `resultats.tsv`.

`ROSE_SANS_JIT=1` est la référence : le JIT natif affiche parfois `1/0` là où
Rose affiche `vrai/faux` (bug documenté SH4, hors périmètre).

## 3. Résultats réels (salve rejouée deux fois, chiffres identiques)

| Mesure | Valeur |
|---|---|
| Programmes | **80** |
| CONFORMES | **53** |
| DIVERGENTS | **20** |
| BLOQUES | **7** |
| Dump RIR produits | 75 (5 refus syntaxiques natifs) |
| ELF émis | 74 (1 refus d'émission : `fact`) |
| RIR et ELF reproductibles octet pour octet | oui, partout où l'émission a réussi |
| Durée de la salve | 57 s puis 66 s (deux salves complètes) |
| fib25 (test `t74_fib_rec`) | **75025 / 75025** (natif / ELF) |

Sortie : `/tmp/opencode/sh9_agents/battery/rapport.txt`,
`resultats.tsv` (80 lignes + en-tête), artefacts par test (`*.nat`, `*.rir`,
`*.elf`, `*.elfout`, stderr).

### 3.1 Divergences (20)

| Cause racine | Tests | Natif → ELF (extrait) |
|---|---|---|
| Stubs G5 : `caractere`, `partie_entiere`, `type`, `abs`/`min`/`max`, `hasard`/`prng`, `chaine(bool)` | `t05`, `t14`, `t15`, `t30`, `t31`, `t44`, `t90`, `t91`, `t92`, `t999` | `B`→`0`, `entier`→`0`, `vrai`→`1`, `7`→`0`, `f`→`0`, `15`→`0` |
| Entiers > 64 bits (ABI G5) | `t32`, `t33`, `t34` | `2^64`→`0`, `9223372036854775807`→`-1`, grand décimal tronqué |
| Rationnels négatifs / calculés, division et modulo négatifs | `t07`, `t08`, `t23`, `t24` | `-7/3` : `-3.[3]`→`-2.[3]` ; `-7/2` : `-4.5`→`-3.5` ; `-7%3` : `2`→`0` |
| Unicode : `sous_chaine` et affichage par octets, pas par points de code | `t04`, `t41` | `été`→`\xfft\xff` |
| Affichage des listes | `t10` | `[1, 2, 3]`→`[1, 3, 0]` (et newline manquante) |

Frequence par cause : stubs 10, grands entiers 3, rationnels 4, Unicode 2,
listes 1. (`t999` compte dans les stubs ; il est aussi le test critique, §4.)

### 3.2 Blocages (7)

| Test | Étape | rc / stderr |
|---|---|---|
| `t104_surcharge_arith`, `t105_surcharge_arite` | natif + RIR + émission | rc=1, `Fonction 'f' définie deux fois` — Rose n'a pas de surcharge de nom |
| `t106_hof_param`, `t107_hof_var` | natif + RIR + émission | rc=1, `Fonction 'fn' inconnue` / `Variable 'carre' utilisée sans valeur` — pas de fonctions d'ordre supérieur |
| `t35_factorielle` | émission | rc=2, `G5 : forme parenthésée non couverte : fact` |
| `t55_liste_imbriquee` | natif | rc=1, `Listes de listes non supportées pour l'instant` |
| `t82_argument_hors` | natif / ELF | rc=1 (`index hors des limites des arguments`) / rc=139 (SIGSEGV ELF) |

Ces blocages ne sont pas des régressions de la batterie : ce sont des limites
réelles, soit du langage Rose (surcharge, HOF, listes de listes), soit de
l'émetteur (`fact`), soit du runtime G5 (`argument` hors borne).

## 4. Test de fonctionnalité critique (signal rapide)

`tests_selfhost/t999_critique_g5.rose` : **20 lignes de sortie**, ~0,3 s par côté.
Il n'utilise que les primitives dont `emetteur_elf.rose` a besoin pour
s'exécuter : `lire_texte(argument(0))`, `nb_arguments`, `longueur`,
`sous_chaine`, `ordre`, `caractere`, `chaine`, `entier`, `flottant`,
`partie_entiere`, `joindre`, `table`/`inserer`/`cle_existe`, `ecrire_bytes`,
booléens, appels à **36 arguments**, immédiats `> 2^31`, `+ - * / % ^`.

Résultat mesuré (snapshot `8b6aab1e…`) : **17 lignes identiques sur 20**, trois
divergences ONLY :

```
ligne  7 : natif f    | ELF 0        -> caractere(102) non émis (stub)
ligne 10 : natif 17   | ELF 0        -> partie_entiere(17) non émis (stub)
ligne 11 : natif vrai | ELF 1        -> chaine(vrai) non converti en "vrai"
```

Tout le reste — lecture du source passé en `argument(0)`, découpage de chaîne,
liste mutable, table de mots-clés, appel à 36 paramètres, encodage des grands
immédiats — **fonctionne déjà** dans l'ELF émis. Ce test remplace un signal de
374 s par un signal de moins d'une seconde pour la question « le runtime G5
peut-il exécuter l'émetteur ? » : la réponse mesurée est **non, à cause de trois
primitives**.

## 5. Premier correctif recommandé

**Implémenter `caractere`, `partie_entiere` et `chaine(bool)` dans le runtime G5
de `compilateur/emetteur_elf.rose`.** Ordre de priorité factuel :

1. `caractere` — 87 occurrences dans l'émetteur figé, dès `u8le` (ligne 9),
   `u16le` (15), `u32le` (26), `u64le` (45). Sans lui, aucune instruction ELF
   ne peut être encodée par un ELF qui exécuterait l'émetteur.
2. `partie_entiere` — 18 occurrences, dès `u16le` (lignes 14, 20, 22, 24).
3. `chaine(vrai/faux)` — la conversion booléenne manquante (`1`/`0` au lieu de
   `vrai`/`faux`) ; déjà partiellement traitée pour les slots booléens suivis,
   pas pour un littéral ou un retour direct.

Ensuite seulement : ABI d'entiers > 64 bits (3 tests + `fact`), division/modulo
négatifs et rationnels calculés (4), Unicode par points de code (2), affichage
des listes (1). La surcharge de noms, les HOF et les listes de listes ne sont pas
des correctifs G5 : le langage Rose ne les accepte pas.

## 6. Validation demandée (tout re-mesuré)

| Contrôle | Résultat réel |
|---|---|
| Point fixe `b3ed9262` | `rose rir compilateur/emetteur.rose` → 263 520 o, `b3ed9262dd54776a6dbd07bc12368d0e`. Émission ELF (ran 13:45 UTC+1, source alors `e668fd42…`) → 534 856 o, `041edb582795e968a3fdebc5345dab4f`, rc=0. Exécution `elf compilateur/emetteur.rose` → rc=0, 263 520 o, `b3ed9262…`. `cmp` RIR/dump : **identique octet pour octet**. |
| `cargo test --release -- --test-threads=1` (depuis `rose/`) | **rc=0, 515 passés, 0 échec**, 6 ignorés (258+242+0+6+8+1+0). |
| Même commande **sans** `--test-threads=1` | 1 échec flaky mesuré : `gui_native::tests::machine_a_etats_simulee` (compteur de fenêtres global, 257 passés/1 échec). Le test isolé passe (rc=0) et la salve sérielle passe : ne pas conclure à une régression de cette batterie. |
| Jardins `scripts/rose-jardin.sh verif-tous` | **rc=1, 9 valides / 10** — la consigne annonçait 20/20 ; le dépôt contient 10 jardins. Le seul invalide est `jardins/gui_native.rose` : ligne 236, un **commentaire** contenant le mot « Rust » déclenche le filtre « marqueur Rust/FFI interdit ». Faux positif du vérificateur, fichier hors périmètre, non modifié. |
| fib25 | 75025 natif, 75025 ELF (test `t74_fib_rec`, salve 2). |
| `rose verif compilateur/emetteur_elf.rose` | `vérifié` au moment de la mesure (le fichier a continué à changer ensuite sous d'autres agents). |

## 7. Rejouer

```
cd /home/clausake/Stefan/ROSE
./tests_selfhost/lancer.sh                    # 80 tests, ~60 s
./tests_selfhost/lancer.sh critique           # seulement t999_critique_g5
SH9_EMETTEUR=/chemin/vers/emetteur_snapshot.rose ./tests_selfhost/lancer.sh
```

Le rapport lisible est affiché sur stdout et écrit dans
`/tmp/opencode/sh9_agents/battery/rapport.txt` ; le détail machine dans
`resultats.tsv`. Le script ne masque aucun blocage : chaque rc et le premier
stderr sont imprimés. La batterie évolue avec l'émetteur : re-mesurer après
chaque modification de `compilateur/emetteur_elf.rose`.

## 8. État final

- 80 nouveaux `.rose`, `tests_selfhost/lancer.sh`, ce document : livrés.
- Aucune modification de `compilateur/`, `rose/src/`, `Cargo.toml`, `PLAN.md`
  ou d'un doc d'un autre agent. Aucun commit, aucune publication.
- Fait non contourné : le dossier des 43 tests SH4 n'existe plus ; la batterie
  est une reconstruction documentée, pas une copie.
