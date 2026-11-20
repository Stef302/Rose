# SELFHOST_TOTAL_PLAN — cartographie exacte du self-hosting A à Z (constat daté)

Mission SH5. Dépôt `/home/clausake/Stefan/ROSE`, HEAD de départ `b8875b6`.
Je n'ai **rien écrit d'autre que ce document**. Aucun commit, aucune publication,
aucun fichier source modifié. Artefacts : `/tmp/opencode/sh5_agents/`.
Horloge machine au moment des mesures : **20/11/2026** (la mission est datée du
21/11/2026 ; j'indique les heures mesurées, pas une date recopiée).

## 0. Avertissement de version (important)

Pendant cette mission, **un autre agent a modifié `compilateur/emetteur_elf.rose`
en parallèle** (travail SH6 : ajout de l'affichage des listes, +204 lignes).
Versions observées :

| Version | md5 | Rôle |
|---|---|---|
| `e668fd426a65ebc1d66138020a01e21a` (SH4) | base avant SH6 | point fixe `b3ed9262…` vérifié |
| **`aa0cf5898a5ccc26d70eed404f68f835`** | **version finale stable analysée** | + `g5_puts` (l.1300), `g5_emit_plist` (l.1322), `$g5_slist`, `g5_list_kind` |
| snapshot local | `/tmp/opencode/sh5_agents/emetteur_elf_snapshot.rose` | copie gelée de `aa0cf…` |

**Tous les numéros de ligne ci-dessous se réfèrent à `aa0cf…`.**
L'ELF de self-host final (`host2.elf`) a été émis depuis cette version.
L'agent concurrent est arrêté : md5 relu trois fois identique.

---

## 1. Ce qui MARCHE déjà (preuve mesurée, pas citée)

Point fixe SH3/SH4 rejoué ici sur la version `aa0cf…` :

| Étape | Résultat mesuré |
|---|---|
| `rose rir compilateur/emetteur.rose` | 263 520 o, md5 `b3ed9262dd54776a6dbd07bc12368d0e` |
| `rose compilateur/emetteur_elf.rose <rir> > elf` | rc=0, 534 856 o, ~374 s |
| `./elf compilateur/emetteur.rose` | rc=0, 263 520 o, md5 `b3ed9262…` |
| `cmp` | identique octet pour octet |

Donc : **le compilateur généré sait compiler `emetteur.rose` et reproduire son
propre RIR.** Ce n'est PAS la même chose que « il sait exécuter
`emetteur_elf.rose` » — c'est précisément l'objet de cette cartographie.

---

## 2. Expérience décisive : le self-hosting de `emetteur_elf.rose` échoue

Chaîne exécutée (aucun artefact réutilisé d'un autre agent) :

```
rose rir compilateur/emetteur_elf.rose          -> 288 636 o, md5 75c3d6ce60225f701ce9dae9c2b8fe9b
rose compilateur/emetteur_elf.rose <ce rir>     -> 382 388 o, md5 f11006eab1623e0a8a2044be054134f6
                                                   375,81 s, RSS 104 900 Ko (mesuré /usr/bin/time)
chmod +x ; ./host2.elf tiny.rir                 -> rc=139, 1,08 s, 0 octet de sortie
```

Reproduit deux fois (avant et après les ajouts SH6) : `host.elf` 369 880 o,
347,5 s, rc=139 ; `host2.elf` 382 388 o, 375,81 s, rc=139. **Le correctif
d'affichage des listes ne débloque rien.**

Diagnostic instrumenté (gdb + strace sur `host2.elf tiny.rir`) :

- `strace` : `openat("tiny.rir")=3`, `read(...,99)=99`, `close(3)` — puis
  `SIGSEGV si_addr=NULL`. **Aucun `openat("/dev/stdout")`** : le chemin G5
  n'a jamais atteint l'écriture de l'ELF.
- `gdb` : pc `0x45c874`, instruction `ldr x2,[x1]`, **x0 = 0, x1 = 0**. C'est
  l'entrée de la routine `longueur` (`g5_masktag` puis `ldr [len]`). Une valeur
  **taguée 0** (entier 0, pas un descripteur) lui a été passée.
- Appelant : `bl 0x45c870` preceded d'un simple copiage de slot — donc une
  primitive en amont a rendu `0` là où le code attendait une chaîne.

**Verdict : le binaire généré ne sait PAS encore exécuter `emetteur_elf.rose`.
Il plante avant d'avoir émis le moindre octet.**

---

## 3. Matrice des primitives G5

Légende état : **OK** = conforme mesurée ; **PARTIEL** = cas limites cassés ;
**STUB** = retourne 0 (ou chaîne vide) sans-op ; **ABSENT** = pas de routine
émise du tout. « appels » = nombre d'appels `app <nom>` dans le RIR de
`emetteur_elf.rose` (288 636 o), mesuré par comptage.

| Primitive (routine émise) | Utilisée par — lignes `aa0cf…` / appels RIR | Sémantique requise | État G5 actuel | Effort estimé | Test minimal natif vs ELF (mesuré) | Blocage self-host |
|---|---|---|---|---|---|---|
| `alloc` — `g5_emit_alloc` l.1462 | inline partout (g5_make_str, join, cat…) | bump allocator, aligné 8 | **OK** | — | allocations OK dans 43 tests SH4 | non |
| `longueur` — `g5_emit_len` l.1474 | dispatch l.877 ; **387 appels** | tag0→0, tag1→len, tag2→len | **PARTIEL** (plante sur tag0) | faible (garde) | `>> longueur(0)` : natif erreur / ELF rc=139 | **oui** (si une primitive amont rend 0) |
| `ordre` — `g5_emit_ordre` l.1495 | l.878 ; 3 appels | octet à index 0 | **OK** | — | — | non |
| `sous_chaine` — `g5_emit_sous` l.1505 | l.879 ; **389 appels** | tranche [a,b), vide si a>b | **OK** (correctif SH2) | — | 389 appels exécutés dans le point fixe | non |
| `idx` — `g5_emit_idx` l.1558 | g5_eval `head=="idx"` ; 1 appel | chaîne→caractère, liste→élément | **OK** | — | `ref4` natif=ELF=`a` (SH3) | non |
| `joindre` — `g5_emit_join` l.1605 | l.881/897 ; **54 appels** | mute en place, descripteur stable, doublement cap | **OK** (SH3) | — | `ref3` 3/3, `ref4` a/a | non |
| `inserer` — `g5_emit_inser` l.1657 (RET nu) mais redirigé vers `joindre` l.897 | l.882 puis 897 ; 1 appel | insertion à index | **PARTIEL** volontaire : `inserer(t,c,v)` = `joindre(t,c)`, `v` ignoré | moyen (vraie insertion) | `tab2` natif=ELF=`vrai/vrai/faux` | non (les tables de l'émetteur sont des ensembles de clés) |
| `entier` — `g5_emit_entier` l.1664 | l.883 ; 11 appels | décimal signé exact, arbitraire | **PARTIEL** : 64 bits, s'arrête au 1er non-chiffre | **élevé** (ABI) | `p_big` : natif `…567890` / ELF `248789718555101906` | **oui** (pour sources arbitraires) |
| `chaine` — `g5_emit_ch` l.1718 | l.880 ; 18 appels | identité si chaîne, décimale si entier | **OK** | — | `entbig` natif=ELF=`10000000000000000` | non |
| `cat` (`+`) — `g5_emit_cat` l.1822 | g5_eval `op=="+"` ; implicite | concatène deux chaînes | **OK** | — | `chaines` `vrai/faux/vrai` (SH4) | non |
| `streq` (`==`) — `g5_emit_streq` l.1880 | g5_eval `==/!=`, `clex` | égalité de contenu | **OK** | — | `cle_existe` clé chaîne OK (SH4) | non |
| `table` — `g5_emit_table` l.1905 | l.896 ; 1 appel | table associative | **PARTIEL** : liste de clés | faible si clés seules | `tabtest` natif=ELF=7 | non |
| `cle_existe` — `g5_emit_clex` l.1923 | l.893 ; 3 appels | clé chaîne (contenu) ou entière (valeur) | **OK** (SH4) | — | `tab2` rc=0 | non |
| `nb_arguments` — `g5_emit_nbargs` l.2004 | l.885 ; 2 appels | argc−1 | **OK** | — | `probe_io` natif=ELF=1 | non |
| `argument` — `g5_emit_argument` l.2020 | l.886 ; 4 appels | argv[i+1] en chaîne, garde de borne | **PARTIEL** | faible (garde) | `p_arg` sans arg : natif rc=1 / ELF **rc=139** | secondaire |
| `lire_texte` — `g5_emit_lire` l.2064 | l.887 ; 2 appels | openat+fstat+read+close | **OK** | — | `strace` : 3 syscalls OK, 99 o lus | non |
| `ecrire_bytes` / `ecrire_texte` — `g5_emit_ecrire` l.2144 | l.888 ; 3 appels | openat(O_CREAT)+write+close | **OK** | — | point fixe écrit 263 520 o | non |
| `afficher` — `g5_emit_print` l.1190 | l.1060… ; partout | entier, chaîne, `\n` final | **OK** | — | 39/43 tests SH4 | non |
| affichage liste — `g5_emit_plist` l.1322 + `g5_puts` l.1300 | g5_op l.1025… ; SH6 | `[a, b]`, éléments int/chaîne/bool | **OK (neuf, non intégré au point fixe)** | — | `p_list` : natif=`[1, 2]` / ELF=`[1, 2]` | non |
| `flottant` → `entier` l.892 | 1 app | parse décimal | **PARTIEL** | moyen | `p_float` : natif `1e+20` / ELF `848750603811160064` | non (le dump de l'émetteur n'a que des flottants entiers) |
| `stub0` (fonction inconnue) l.1974 | fallback l.… (tout nom non reconnu) | — | **STUB dangereux** : rend 0 | — | `p_car` ELF `0` puis rc=139 | **oui, cause directe** |
| **`caractere`** | **l.9, 15, 26, 45 (u8le/u16le/u32le/u64le), 420–424, 2228–2230, 3016…3145 ; 45 appels** | n → descripteur chaîne `[1][1][n%256]` | **ABSENT → stub0** | **faible** (~25 instr, 100 o) | `p_car` natif `A / 1 / AX` ; ELF `0` + **rc=139** | **BLOQUANT #1** |
| **`partie_entiere`** | **l.14, 20, 22, 24, 31, 33, 35, 37, 39, 41, 43 (u16/32/64le), 278–279 (ldroff/stroff), 365–367 (**g5_load**), 467, 2968 ; 18 appels** | quotient entier vers zéro | **ABSENT → stub0** | **faible** (~20 instr, 80 o) | `p_par` natif `3 / 3` ; ELF `0 / 0` | **BLOQUANT #2** (g5_load tronque à 16 bits) |
| **`quitter`** | **l.2353, 2357 (chemin G5), 3276, 5093 ; 4 appels** | `exit_group(code)` | **ABSENT → stub0** | **très faible** (2 instr + syscall 93) | `p_quit` natif `avant`, **rc=7** ; ELF `avant`, `apres`, **rc=0** | **BLOQUANT #3** (le chemin G5 ne s'arrête pas) |
| `afficher_erreur` | l.2352, 3275, 5092 ; 3 appels | écrire sur stderr | **ABSENT → stub0** | faible (syscall write fd 2) | silencieux côté ELF | secondaire |
| **`eval_expr`** | **l.2642…5081 ; 29 appels, tous dans la DEUXIÈME section ( ancien compilateur )** | évaluer une expression texte | **ABSENT → stub0** | **élevé** (parseur + évaluateur) | `p_eval` natif `3` ; ELF `0` | **BLOQUANT #4 si `quitter` ne marche pas ; sinon contournable** |
| BigInt > 2^60 | `u64le` l.29–45, `g5_load` l.359–373 | entiers arbitraires dans l'ABI | **ABSENT** (tag 3 bits → 61 bits utiles) | **très élevé** (changement ABI) | `p_big` natif correct / ELF `0` et tronqué | **oui pour A-Z, non pour le seul dump de l'émetteur** |
| constante `2^64` | **l.362–363 (`g5_load`), l.453 (`g5_make_str` hash)** | modulo 2^64 | **CASSÉ sous G5** : `entier("2^64")`→0, donc `$u >= 0` toujours vrai puis `% 0` → 0 sur aarch64 (SDIV/0 = 0, pas de trap) ; tous les `movk` disparaissent | faible (supprimer les gardes / remplacer par sentinel) | déduction confirmée : `p_big` `2^64`→`0` | **BLOQUANT #2b** |
| `argument(i)` hors bornes | l.886 | erreur contrôlée rc=1 | aucun garde-fou | faible | `p_arg` : **rc=139** vs rc=1 | non |

---

## 4. Analyse : pourquoi ça plante, sans embellir

La chaîne de causes est **cumulative**, pas un seul bug :

1. `quitter` est un stub. Le `$principal` de `emetteur_elf.rose` contient
   **deux compilateurs** : le chemin G5 (l.2260…2357) et un ancien compilateur
   source→ELF (l.2366…5093). Le chemin G5 se termine par `quitter(0)` (l.2353).
   Sous l'ELF, ce `quitter` ne fait rien : l'exécution **retombe dans l'ancien
   compilateur**, qui appelle `argument(1)` (inexistant), `eval_expr` (stub),
   `caractere` (stub)…
2. `caractere` est un stub : `u8le/u16le/u32le/u64le` (l.8–45) construisent
   l'ELF octet par octet. Avec `caractere(n) → 0`, `$out + caractere(n)` est une
   **addition d'entiers** (les deux opérandes ont le tag 0) au lieu d'une
   concaténation : `$out` devient l'entier 0. La première `longueur($out)`
   suivante déréférence 0 → **SIGSEGV**. C'est exactement le crash observé
   (`x0=0, x1=0` dans `g5_emit_len`).
3. `partie_entiere` est un stub : `g5_load` (l.365–367) ne génère que le
   `movz` bas, tous les `movk` des bits 16/32/48 sont perdus. L'adresse du tas
   `0x60000000` devient `0x0` → écriture à 0 → SIGSEGV.
4. La constante `2^64` (l.362–363, l.453) devient 0 sous l'ABI 61 bits, ce qui
   transforme `$u >= 2^64` en `$u >= 0` (toujours vrai) et `$u % 2^64` en
   `$u % 0` (aarch64 rend 0 silencieusement). Résultat : `g5_load` charge 0.
5. `eval_expr` (29 appels) n'est nécessaire **que** pour l'ancien compilateur.
   Si `quitter` est réparé, cette section ne s'exécute jamais et il n'y a pas
   besoin de l'implémenter pour la porte de self-hosting.

**Corollaire important** : il ne faut PAS ajouter « une primitive » ; il faut
au minimum **`caractere` + `partie_entiere` + `quitter` + correction de `2^64`**.
`eval_expr` n'est nécessaire que si l'ancien compilateur reste atteignable.
L'ordre ci-dessus est l'ordre de blocage mesuré, pas un ordre de facilité.

---

## 5. Chemin de bootstrap recommandé

**Chemin A (recommandé) — réparer le runtime G5 existant, sans changer l'ABI
pour la porte :**

- C0 : `quitter` + `caractere` + `partie_entiere` + suppression des gardes
  `2^64` dans `g5_load`/`g5_make_str` (4 changements, ~150–250 lignes d'émetteur,
  ~1,5 Kio de code machine ajouté).
- C1 : réémettre `host.elf`, vérifier qu'il émet un ELF depuis `tiny.rir`, puis
  depuis n'importe quel RIR de la batterie SH4 (39 tests de référence).
- C2 : seulement ensuite, BigInt/flottants complets pour le « A à Z ».

**Chemin B (alternative honnête) — interpréteur RIR en Rose séparé :**
Écrire un interpréteur de dumps RIR en Rose, le compiler avec l'émetteur actuel,
et faire exécuter l'interpréteur rather que le code machine généré. Il évite
d'étendre l'ABI (l'interpréteur peut être écrit dans le sous-ensemble déjà
supporté), mais **déplace le problème** : l'interpréteur lui-même doit tourner
sous G5 et aura besoin des mêmes `caractere`/`partie_entiere`/`quitter`.
Coût similaire pour C0, mais il double la surface de code à maintenir. **Non
recommandé tant que C0 n'a pas été tenté.**

---

## 6. Coût réel (mesuré, pas estimé à la hausse ni à la baisse)

| Poste | Mesure |
|---|---|
| Émission native de `host2.elf` (depuis `aa0cf…`) | **375,81 s** (~6 min 16) |
| Émission native du point fixe SH4 | ~374 s (re-mesuré dans la littérature, cohérent) |
| Mémoire RSS pendant l'émission | **104 900 Ko** (~102 Mio) |
| Espace d'adressage RW mappé | **2 Gio** (lazy, `g5_elf` l.…, `p_memsz`) — pas de pression réelle |
| Taille `host2.elf` | **382 388 o** (code machine de tout `emetteur_elf.rose` + runtime) |
| Taille du runtime G5 actuel | ~2,2 Kio (23 routines, comptage des `g5_emit(`) |
| Coût des routines C0 | ~1–2 Kio de code machine, ~150–250 lignes d'émetteur |
| Coût BigInt complet (C2) | **changement d'ABI** : nouveau tag, +/-, ×, ÷, %, comparaison, `chaine`,.parseur → **plusieurs milliers de lignes, dizaines de Kio** |

Sur la durée : chaque itération C0 coûte **~6 min de machine** pour l'émission
du ELF de test + quelques secondes de tests. Une dizaine d'itérations = ~1 h de
temps machine, sans compter le débogage.

---

## 7. Plan d'étapes numéroté, critère de réussite mesurable

### C0 — rendre le runtime G5 conforme aux primitives déjà utilisées

- **C0.0 — Baseline gelée.** md5 fichier `aa0cf…`, RIR `75c3d6ce…`,
  `host2.elf` `f11006ea…`, crash rc=139 reproductible ×2.
  *Critère :* les 4 valeurs relues à la fin sont identiques.
- **C0.1 — `quitter`.** Émettre `mov x0,<code>; mov x8,#93; svc #0` et câbler
  `quitter` dans le dispatch `app` (l.885-897) vers cette routine au lieu de
  `stub0`. *Critère :* `p_quit` natif=ELF → sortie `avant` uniquement, **rc=7**
  des deux côtés.
- **C0.2 — `caractere`.** Routine émise : `n` tagué → `and x9,x0,#~7`, byte
  `n%256`, descripteur `[1][1][octet]` tag1. *Critère :* `p_car` natif=ELF →
  `A`, `1`, `AX`, rc=0 ; plus aucun `longueur(0)` dans `p_car`.
- **C0.3 — `partie_entiere`.** Routine émise : `asr` des deux opérandes,
  `sdiv`, tag. *Critère :* `p_par` natif=ELF → `3`, `3` ; **et** le code émis
  par `g5_load(9, 0x60000000)` contient bien `movz` + `movk #0x6000`.
- **C0.4 — constantes `2^64`.** Remplacer les gardes l.362-363 et le modulo
  l.453 par une forme représentable en 61 bits (ou les supprimer : sous G5,
  `entier` rend déjà un 64-bit signé). *Critère :* `g5_load` émis n'appelle
  plus `% 0` ; `>> 18446744073709551616` ne provoque plus de division par zéro.
- **C0.5 — `afficher_erreur`.** write(2,…) + `quitter(1)`. *Critère :* une
  erreur connue (`cle_existe` sur clé invalide) rend le même message stderr
  natif et ELF.

### C1 — franchir la porte de l'auto-exécution

- **C1.1 — Réémettre et lancer.** `rose rir emetteur_elf.rose > e.rir`,
  `rose emetteur_elf.rose e.rir > host.elf`, `./host.elf tiny.rir > out.elf`.
  *Critère :* **rc=0**, `out.elf` non vide et commence par `\x7fELF`
  (`xxd -l 8`), taille de l'ordre de 300–400 Kio. C'est le critère qui manque
  aujourd'hui (rc=139, 0 octet).
- **C1.2 — Généralité.** Relancer la batterie SH4 (43 programmes,
  `ROSE_SANS_JIT=1` comme référence) avec `host.elf` comme émetteur.
  *Critère :* ≥ 39/43 conformes (niveau SH4), puis viser 43/43 avec `p_arg`,
  `p_big`, `p_float`.
- **C1.3 — Point fixe du self-host.** `./host.elf e.rir > host2.elf`, puis
  `./host2.elf tiny.rir > out2.elf`. *Critère :* `cmp out.elf out2.elf` et/ou
  md5 identiques : le compilateur généré se régénère lui-même.
- **C1.4 — Non-régression du point fixe historique.** Rejouer la chaîne
  `emetteur.rose → b3ed9262…`. *Critère :* md5 `b3ed9262dd54776a6dbd07bc12368d0e`
  inchangé, `cmp` octet pour octet.

### C2 — self-hosting A à Z (hors porte)

- **C2.1 — BigInt dans l'ABI G5.** Nouveau tag (3), descripteur de grande
  taille, routines `+ − × ÷ % cmp chaine(parse)`. *Critière :* `p_big`
  natif=ELF sur 30 décimales et `2^64`.
- **C2.2 — Flottants.** Formateur exponentiel et rationnels >= 2^63.
  *Critère :* `p_float` natif=ELF → `1.5`, `1e+20`.
- **C2.3 — Gardes d'arguments.** *Critère :* `p_arg` natif=ELF → rc=1.
- **C2.4 — Validation finale.** Batterie 43/43 native=ELF, `cargo test
  --release` (431 passés attendus), jardins 10/10, point fixe ×2.

---

## 8. Faisabilité — verdict honnête

| Objectif | Verdict |
|---|---|
| Point fixe actuel (`emetteur.rose`) | **RÉALISÉ** — rejoué et vérifié |
| Exécuter `emetteur_elf.rose` par le binaire généré | **RÉALISABLE MAIS COÛTEUX** — 4 correctifs C0, ~1–2 Kio, ~6 min/itération ; risque réel de régression du point fixe à chaque label ajouté (la réservation de labels est une suite de `g5_next()` avec des nombres magiques, l.2264–2316) |
| Émettre un ELF depuis n'importe quelle source (A à Z) | **TRÈS COÛTEUX / BLOQUÉ SANS CHANGEMENT D'ABI** — BigInt + flottants exigent une refonte de la représentation des valeurs taguées ; ce n'est pas une « primitive de plus » |
| Interpréteur RIR en Rose séparé (chemin B) | **RÉALISABLE mais déplace le problème** — non recommandé avant C0 |

**Ce qui reste faire, en une phrase :** commencer par C0.1 (`quitter`), qui est
le correctif le moins cher et qui empêche le retombement dans l'ancien
compilateur ; puis C0.2 (`caractere`) et C0.3 (`partie_entiere`), les deux stubs
qui détruisent la construction octet par octet de l'ELF ; ne toucher à `eval_expr`
et à l'ABI BigInt qu'après avoir mesuré C1.1.

---

## 9. Artefacts et reproductibilité

Tout est dans `/tmp/opencode/sh5_agents/` :

- `emetteur_elf_snapshot.rose` (md5 `aa0cf…`), `emetteur_elf.rir` (278 603 o,
  version pré-SH6) et `emetteur_elf_current.rir` (288 636 o, md5 `75c3d6ce…`).
- `host.elf` (369 880 o, pré-SH6) et `host2.elf` (382 388 o, md5 `f11006ea…`).
- `tiny.rose` / `tiny.rir`, `probe_io.*`, `p_car/par/eval/list/big/float/quit/arg.*`.
- Logs `host2_emit_time.txt`, `host_run_err.txt`, `strace_host.txt`, dumps gdb.

`rose/rose` inchangé (md5 `12eac46007cf0fe9e1aec2dff6fbf665`).
`compilateur/emetteur.rose` inchangé (md5 `390335d29fb9cee368b3456dd892e377`).
`compilateur/emetteur_elf.rose` **modifié par un autre agent**, pas par moi.
Aucun commit, aucune publication, aucun fichier source écrit par SH5.
