# BOOTSTRAP_AZ — pipeline de bootstrap A–Z avec preuve C1.3 (constat daté, 21/11/2026)

Sous-agent `sh13`. Dépôt `/home/clausake/Stefan/ROSE` (`Stefan/ROSE`), HEAD de départ
`cadcdb83dd8d9627b69b5de88bb21d5030470df1`. Deux fichiers possédés, et **aucun
autre** : `scripts/selfhost_total.sh` et ce document. Aucun commit, aucune
publication. Artefacts : `/tmp/opencode/sh13_agents/`.

**Règle de ce document** : tout ce qui est annoncé comme mesuré ici a été
exécuté, avec rc, sortie et chemin de log. Le reste est explicitement marqué
« documenté (SHx) » avec sa source. Aucune extrapolation.

---

## 1. Ce que le pipeline mesure

`scripts/selfhost_total.sh` est un **constat daté** : il n'écrit aucun fichier
source (`compilateur/**`, `rose/src/**`, `Cargo.toml` intacts), seulement un
`WORKDIR`. Le binaire Rose sert de graine ; tout ce qu'il émet est vérifié.

| Étape | Chaîne exécutée | Ce que cela prouve | Coût mesuré |
|---|---|---|---|
| **a** | `rose verif compilateur/emetteur_elf.rose` | la source de l'émetteur ELF est valide | < 1 s |
| **b** | `rose rir compilateur/emetteur.rose` + md5 | le dump RIR de référence est reproductible (`b3ed9262…`, 263 520 o) | < 1 s |
| **c** | `rose rir emetteur_elf.rose` puis `rose emetteur_elf.rose e.rir > C0.elf` | le natif émet un ELF aarch64 | ~374 s (timeout 900 s) |
| **d** | `C0.elf compilateur/emetteur.rose` + `cmp` avec `self.rir` | C0 (= émetteur RIR compilé) reproduit le front-end RIR octet pour octet | ~6 s |
| **e** | `P0 = rose emetteur_elf.rose e.rir`, `P1 = P0 e.rir`, sha256 | point fixe du compilateur généré (graine = natif, **pas** C0) | ~2×430 s |
| **f** | `C0 fib25.rose` | **diagnostic** : C0 produit du RIR, pas un ELF → BLOQUÉ *attendu* | ~1 s |
| **g** | `F1 = rose emetteur_elf.rose e.rir`, `F2 = F1 e.rir`, md5 + `cmp` | **C1.3** : auto-régénération du compilateur, sans patch externe | ~2×430 s en `--quick` ; ~0 s en `--full` (réutilise P0/P1 de e) |
| **h** | pour chaque source : `rose rir src` → `F1(RIR)` → ELF exécuté ; comparaison stdout/stderr/rc au natif | **critère A–Z** : le compilateur généré compile une source arbitraire | quelques s par source |

`--with-c13` rend g optionnelle : c'est la seule étape chère qui soit
réellement facultative. `--inventaire` n'exécute aucune mesure : il affiche les
faits documentés SH5/SH6/SH7/SH10/SH11 et sort avec rc=0.

### Verdicts et codes de sortie (depuis SH13)

| Code | Verdict | Condition |
|---|---|---|
| 0 | **A–Z ATTEINT** | a..e PASS, g PASS, h PASS (toutes les sources reproduites) |
| 2 | **A–Z PARTIEL** | a..e PASS, g PASS, h PARTIEL (fib marche, bigints/tables bloqués) |
| 3 | **BLOQUÉ** | une étape a..e BLOQUÉE/Ignorée (cas `--quick`), ou g/h BLOQUÉS |
| 4 | **A–Z NON ÉVALUÉ** | a..e PASS mais g IGNORÉ (`--with-c13` absent) |
| 1 | **ERREUR** | binaire/source introuvable |

Le verdict **ne dépend plus de f** : f est un diagnostic historique. C0 est
l'émetteur RIR (`$principal` de `emetteur.rose`) ; il ne compile pas une source
en ELF. Le critère A–Z est h, exécuté avec F1 (ou P0, même compilateur généré).

---

## 2. Définition opérationnelle de « A–Z atteint »

**A–Z atteint** signifie, sans exception ni succès simulé :

1. la chaîne de bootstrap a..e est verte (a, b, c, d, e PASS) ;
2. **C1.3 est prouvée** : `F1 == F2` octet pour octet, F1 ayant été généré depuis
   le source courant et F2 produit par F1 exécutant son propre RIR, sans patch
   manuel de l'en-tête ELF ;
3. **h est PASS sur toutes les sources de l'échantillon** : pour *chacune*,
   `stdout`, `stderr` et `rc` de l'ELF émis par F1 sont identiques au natif.
   L'échantillon SH13 couvre volontairement un cas qui marche (fib récursif,
   arithmétique entière) et deux cas historiquement bloqués (bigint `2^64`,
   lecture de valeur dans une table). Tant que l'un d'eux diverge, le verdict
   est **PARTIEL**, jamais « atteint » ;
4. le point fixe `b3ed9262dd54776a6dbd07bc12368d0e` reste inchangé (aucune
   modification des sources de l'émetteur RIR).

**Deux niveaux distincts, à ne pas confondre :**

- **A–Z « émetteur »** (h tel qu'implémenté) : F1 produit l'ELF ; le RIR est
  encore obtenu via `rose/rose rir` (front-end compilé en Rust). C'est le
  critère demandé à SH13.
- **A–Z « Rust-free complet »** : le RIR lui-même doit être produit par C0
  (`C0 source.rose > source.rir`), puis F1 produit l'ELF. C0 est précisément
  l'émetteur RIR validé en d. Tant que ce couplage C0→F1 n'est pas joué de
  bout en bout sur l'échantillon, le niveau « Rust-free complet » n'est pas
  démontré, même si h est PASS. Ce document le dit explicitement au §6.

---

## 3. Fais documentés repris tels quels (SH5 → SH11)

| Fait | État | Source |
|---|---|---|
| `quitter`, `caractere`, `partie_entiere` (C0) | OK | SH7 `G5_C0.md` §2–3 : natif=ELF octet pour octet, rc=7 pour `quitter` |
| globales G5 en RW, auto-exécution de l'émetteur (C1) | OK | SH10 `G5_C1.md` §2, §4, §6 |
| F1==F2 (C1.3) | OK | SH11 `C13_FIX.md` §5.2 : 432 544 o, md5 `6f2715e5c5fe26c3e26f6ac404051bd2` |
| point fixe `b3ed9262` | OK | SH11 `C13_FIX.md` §6 : identique ×2 (dump ELF) + ×2 (dump natif) |
| affichage listes, `argument(i)` hors limites, flottants ≥ 1e16 | OK | SH6 `G5_PRIMITIVES.md` §3–5 |
| grands entiers > 64 bits (ABI multi-limbes) | ABSENT | SH6 §7.1 : `2^64 → 0` ; SH5 `SELFHOST_TOTAL_PLAN.md` l.116 |
| tables : valeurs de `inserer`, `t["a"]`, `supprimer` | ABSENT/PARTIEL | SH6 §7.2, SH5 l.96/101 |
| rationnels négatifs/calculés, Unicode par octets, `type`/`abs`/`min`/`max`, graine/hasard | PARTIEL | SH7 §4, SH11 §8.2 |
| HOF, surcharge, listes de listes, `fact` | ABSENT (7 blocages) | SH7 §4, SH10 §7.2 |
| batterie SH9 (80 programmes) | 57 conformes / 16 divergents / 7 bloqués | SH11 §7 (aucune régression depuis SH7) |

Le tableau lisible complet est produit par `--inventaire` (§5.2). SH12
(tables réelles + rationnels négatifs) est un **chantier concurrent** : son
résultat n'est pas repris ici, par principe.

---

## 4. Protocole de rejeu

```bash
cd /home/clausake/Stefan/ROSE
scripts/selfhost_total.sh --quick                    # contrôles rapides
scripts/selfhost_total.sh --inventaire               # faits documentés, rc=0
scripts/selfhost_total.sh --full --with-c13          # mesure A–Z complète
# réutiliser un C0 déjà émis pour éviter ~374 s :
SELFHOST_C0_PATH=/tmp/opencode/sh13_agents/full/C0.elf \
  scripts/selfhost_total.sh --full --with-c13 --workdir /tmp/opencode/sh13_agents/rejeu
```

Toutes les variables d'environnement sont documentées dans l'en-tête du
script (`SELFHOST_ROSE_BIN`, `SELFHOST_ROSE_ROOT`, `SELFHOST_EMETTEUR`,
`SELFHOST_EMETTEUR_ELF`, `SELFHOST_WORKDIR`, `SELFHOST_RIR_MD5`,
`SELFHOST_EMIT_TIMEOUT`, `SELFHOST_C0_MIN_SIZE`, `SELFHOST_C0_PATH`,
`SELFHOST_C13_MD5`, `SELFHOST_C13_MIN_SIZE`).

---

## 5. Résultats RÉELS mesurés par SH13 (21/11/2026)

### 5.1 `--quick` — rc=3 (BLOQUÉ, portée réduite)

Commande :
```
scripts/selfhost_total.sh --quick --workdir /tmp/opencode/sh13_agents/quick
```
Log exact : `/tmp/opencode/sh13_agents/quick.log`. Sortie (lignes utiles) :

```
date        : 2026-11-20T17:35:38Z
mode        : quick   (--with-c13=0)
binaire Rose: /home/clausake/Stefan/ROSE/rose/rose
md5 binaire : 12eac46007cf0fe9e1aec2dff6fbf665
uname -m    : aarch64
[PASS]   a — verif(emetteur_elf.rose) : rc=0, sortie=vérifié
[PASS]   b — self.rir + md5 b3ed9262 : 263520 o, md5=b3ed9262dd54776a6dbd07bc12368d0e
[IGNORÉ] c — émission C0 (timeout 900s) : mode --quick : émission C0 non tentée (nécessite --full, ~374 s mesurées)
[IGNORÉ] d — C0(emetteur.rose) == self.rir : mode --quick : C0 absent
[IGNORÉ] e — point fixe compilateur P0 == P1 : mode --quick : émissions P0/P1 non tentées
[BLOQUÉ] f — C0 compile fib(25) == natif (diagnostic) : natif=75025 OK mais C0 absent (mode --quick) : compilation par C0 non démontrée
[IGNORÉ] g — C1.3 : F1 == F2 (auto-régénération, --with-c13) : C1.3 non demandé (--with-c13 ; ~2×430 s en --quick, réutilisation de e en --full)
[IGNORÉ] h — A–Z : F1 compile des sources arbitraires == natif : aucun compilateur généré disponible en --quick (utiliser --full ou --with-c13)
VERDICT: BLOQUÉ   (code=3)
```
Ce résultat est **attendu** : `--quick` ne prétend pas mesurer A–Z. Rapport
Markdown : `/tmp/opencode/sh13_agents/quick/RAPPORT.md`.

### 5.2 `--inventaire` — rc=0

Commande : `scripts/selfhost_total.sh --inventaire`. Log exact :
`/tmp/opencode/sh13_agents/inventaire.log`. La sortie complète est reproduite
 dans le log ; elle contient 11 lignes **OK**, 5 lignes **PARTIEL**, 6 lignes
**ABSENT**, puis :

```
Batterie SH9 (80 programmes) — dernière mesure documentée (SH11 §7) :
  57 CONFORMES / 16 DIVERGENTS / 7 BLOQUÉS (aucune régression SH7→SH11).
Validation globale documentée (SH11 §7) : cargo 515 passés/0 échec ;
  jardins 9/10 (gui_native.rose : faux positif commentaire « Rust ») ; fib25 OK.
NOTE: SH12 (tables réelles + rationnels négatifs) est un chantier CONCURRENT.
      Son résultat n'est PAS repris ici : seuls les faits SH5–SH11 sont affichés.
```

### 5.3 Validations rejouées par SH13

| Contrôle | Commande | Résultat réel |
|---|---|---|
| Syntaxe | `bash -n scripts/selfhost_total.sh` | rc=0 |
| Point fixe | `rose rir compilateur/emetteur.rose` | 263 520 o, `b3ed9262dd54776a6dbd07bc12368d0e` (identique, source non touchée) |
| `verif` émetteur | `rose verif compilateur/emetteur_elf.rose` | `vérifié`, rc=0 ; source md5 `8df588b0a77f663a61e5eef39a53509f` |
| Tests Rust | `cargo test --release -- --test-threads=1` (`CARGO_BUILD_JOBS=2`) | **rc=0, 515 passés, 0 échec**, 6 ignorés (258+242+6+8+1) — `/tmp/opencode/sh13_agents/cargo.log` |
| Jardins | `scripts/rose-jardin.sh verif-tous` | **rc=1, 9 valides / 10** ; seul `jardins/gui_native.rose` rejeté (commentaire l.236 contenant « Rust ») — `/tmp/opencode/sh13_agents/jardin.log` |
| fib25 natif | `rose tests_selfhost/t74_fib_rec.rose` | `75025`, rc=0 |

### 5.4 Plein pipeline `--full --with-c13` — rc=2 (A–Z PARTIEL)

Commande :
```
scripts/selfhost_total.sh --full --with-c13 --workdir /tmp/opencode/sh13_agents/full
```
Début `2026-11-20T17:35:49Z`, fin ~`17:50Z`. Log exact :
`/tmp/opencode/sh13_agents/full.log` ; rapport :
`/tmp/opencode/sh13_agents/full/RAPPORT.md`. Sortie (intégrale des étapes) :

```
[PASS]   a — verif(emetteur_elf.rose) : rc=0, sortie=vérifié
[PASS]   b — self.rir + md5 b3ed9262 : 263520 o, md5=b3ed9262dd54776a6dbd07bc12368d0e
[PASS]   c — émission C0 (timeout 900s) : 542212 o, ELF aarch64, md5=05a15ab30989c0ecf983e5d83cc29909, 399s, rc=0
[PASS]   d — C0(emetteur.rose) == self.rir : rc=0, 263520 o, cmp octet-pour-octet == self.rir (b3ed9262)
[PASS]   e — point fixe compilateur P0 == P1 : sha256(P0)=sha256(P1)=572ba193ee362dee008e82edc7cb210db163fea2417bc6848bbf0aaaee552565 (445092 o) — graine = binaire natif, PAS C0
[BLOQUÉ] f — C0 compile fib(25) == natif (diagnostic) : natif=75025, C0 rc=0 mais sortie NON-ELF (entête 'rir') : C0 = émetteur RIR ($principal de emetteur.rose), pas compilateur source→ELF
[PASS]   g — C1.3 : F1 == F2 (auto-régénération, --with-c13) : F1==F2=9eae0bfad64ef32707719cb4811c178f, 445092 o, cmp=0 — réutilisés de l'étape e (même source courant, 0 émission rejouée) (réf. documentée 6f2715e5c5fe26c3e26f6ac404051bd2, source différente)
   compilateur généré utilisé : F1 (C1.3)
    • fib (t74_fib_rec.rose) : MARCHE — rc=0, stdout='75025', cmp=0 avec le natif
    • bigint (t33_big_2_64.rose) : BLOQUÉ — natif rc=0 stdout='18446744073709551616' | ELF rc=0 stdout='0'
    • tables (az_tabval.rose) : MARCHE — rc=0, stdout='vrai', cmp=0 avec le natif
[PARTIEL] h — A–Z : F1 compile des sources arbitraires == natif : 2 MARCHE / 1 BLOQUÉ (via F1 (C1.3)) — voir lignes ci-dessus
VERDICT: A–Z PARTIEL   (code=2)
```

**Md5 et tailles RÉELS de C1.3 mesurés ici** (F1 et F2 sont le même fichier,
`cmp` muet, rc=0) :

| Artefact | Taille | md5 |
|---|---|---|
| `C0.elf` (étape c) | 542 212 o | `05a15ab30989c0ecf983e5d83cc29909` |
| `emetteur_elf.rir` (entrée de F1) | 315 365 o | `918dc037d76a3440b856f07050ca6101` |
| `P0.elf` = `F1.elf` | 445 092 o | `9eae0bfad64ef32707719cb4811c178f` |
| `P1.elf` = `F2.elf` | 445 092 o | `9eae0bfad64ef32707719cb4811c178f` |

`sha256(P0)=sha256(P1)=572ba193ee362dee008e82edc7cb210db163fea2417bc6848bbf0aaaee552565`.

**Écart avec la référence documentée — explication factuelle, pas une
anomalie du script** : C13_FIX.md §5.2 annonce 432 544 o / `6f2715e5…` pour
un artefact produit à `16:24–16:32Z` avant le commit final `5afd0ae` (`16:42Z`).
Pendant la mesure SH13 (`17:35–17:50Z`), un agent concurrent (SH12) a modifié
`compilateur/emetteur_elf.rose` à `17:39Z` : le md5 de travail est passé de
`8df588b0a77f663a61e5eef39a53509f` (utilisé par c/d) à
`4808db60a1f83b9943113ef046688b71` (utilisé par e/g/h, vérifié par
`rose verif` → `vérifié`). Le RIR produit maintenant est `918dc037…` contre
`71d15806…` dans SH11 : 5 appels supplémentaires à `g5_tagbits`, cohérents
avec +12 548 o dans l'ELF émis. **La propriété C1.3 reste vraie sur la source
courante** (`F1 == F2` octet pour octet) ; seuls les chiffres changent, comme
l'exige la règle « tout résultat est un constat daté ». Le script signale
l'écart à la référence au lieu de le masquer.

### 5.5 Complément : chaîne Rust-free C0 → RIR → F1 → ELF (mesuré)

Pour vérifier le niveau « Rust-free complet » défini au §2, la même chaîne a
été rejouée **sans appeler `rose rir`**, avec les artefacts de 5.4 :

```
C0.elf tests_selfhost/t74_fib_rec.rose > fib.rir
F1.elf fib.rir > fib.elf ; ./fib.elf
```

| Source | C0→RIR | RIR == RIR natif ? | F1→ELF | Sortie ELF | Sortie native |
|---|---|---|---|---|---|
| `t74_fib_rec.rose` | rc=0, 347 o | **oui**, `cmp`=0 | rc=0, 5 260 o | `75025` | `75025` |
| `az_tabval.rose` (tables) | rc=0, 296 o | **oui**, `cmp`=0 | rc=0, 5 744 o | `vrai/10/1` | `vrai/10/1` |
| `t33_big_2_64.rose` | rc=0, 173 o | **non** (C0 écrit `0` au lieu de `18446744073709551616` dans le RIR) | rc=0, 5 008 o | `0/0` | `18446744073709551616` ×2 |

Artefacts : `/tmp/opencode/sh13_agents/rustfree/`. Le couplage C0→F1 fonctionne
donc pour fib et les tables ; il bute sur le même bigint `2^64` que h, et C0
truncé déjà le littéral en amont.

### 5.6 Mini-batterie courante rejouée avec F1 (20/11 ~17:52Z)

15 programmes de `tests_selfhost/` compilés avec F1 puis exécutés, comparés au
natif (stdout, stderr, rc). Log : `/tmp/opencode/sh13_agents/batterie13/`.

| MARCHE (9 ici + t74 en §5.4 = 10) | DIVERGENT (6) |
|---|---|
| t06 rationnel littéral, t10 liste littérale, t12/t13 tables, t21 soustraction, t60/t61/t63/t64 tables ; t74 fib en §5.4 | t04 Unicode, t07 rationnel négatif, t08 rationnel calculé, t14 `type`, t23 division, t24 modulo |

Écarts exacts mesurés :

```
t04  natif 'été radieux/11/🚀/'        ELF octets 'e9 74 e9 20 … 80' (points de code tronqués à leur octet de poids faible)
t07  natif '-3.[3]/...'                 ELF '-2.[3]/...'
t08  natif '3.[142857]/1/'              ELF '3/1/'
t14  natif 'entier/chaine/liste/booleen/' ELF '0/0/0/0/'
t23  natif '14.[285714]/6/-4.5/'        ELF '14.[285714]/6/-3.5/'
t24  natif '2/0/-1/'                     ELF '2/0/0/'
```

---

## 6. État A–Z courant et ce qui reste

**Verdict SH13 mesuré : A–Z PARTIEL (code 2), jamais « atteint ».**

Ce qui **marche** (preuves SH13 5.4–5.6) : la chaîne de bootstrap a..e, le
point fixe `b3ed9262`, la propriété C1.3 sur la source courante, fib récursif
(`75025`) y compris en chaîne Rust-free, l'arithmétique entière simple, les
tables avec valeurs et `cle_existe` (correctif concurrent SH12 visible dans
`git diff compilateur/emetteur_elf.rose`), les listes littérales.

Ce qui **reste bloqué** (mesuré, pas cité) :

1. **grands entiers > 64 bits / ABI** : `2^64` natif `18446744073709551616`
   contre ELF `0`, et C0 tronque déjà le littéral dans le RIR. Nécessite un
   changement d'ABI (SH5 l.116, SH6 §7.1). C'est le seul blocage de l'échantillon
   h, donc le seul qui empêche h PASS aujourd'hui.
2. **rationnels négatifs / calculés et division-modulo négatifs** : t07, t08,
   t23, t24 divergents (§5.6). Le chantier SH12 annoncé pour les rationnels
   négatifs n'est pas validé dans les artefacts mesurés ici.
3. **Unicode** : t04 divergent ; `g5_cps` (SH11) a rendu l'émetteur idempotent,
   mais la lecture G5 par octets reste opposée à la lecture native par points
   de code pour les sources arbitraires (C13_FIX §3/§8).
4. **`type`, `abs`, `min`, `max`, graine/PRNG, HOF, surcharge, listes de
   listes, `fact`** : divergences/blocages documentés SH7/SH10/SH11 ;
   `type` re-mesuré divergent ici (t14).
5. **stubs résiduels** : `afficher_erreur` générique, `eval_expr` (ancien
   compilateur, contourné par `quitter`).

**Condition pour passer de PARTIEL à ATTEINT** : corriger le bigint `2^64`
dans l'ABI G5 (et répercuter dans C0 pour le RIR), puis rejouer
`--full --with-c13` : h doit afficher `3 MARCHE / 0 BLOQUÉ` et le verdict
`A–Z ATTEINT (code=0)`. Toute autre source ajoutée à l'échantillon devra
elle aussi être conforme.

---

## 7. Périmètre et honnêteté

- Aucun fichier `rose/src/**`, `compilateur/**`, `Cargo.toml`, `PLAN.md` ni
  document d'un autre agent n'a été modifié. Seuls `scripts/selfhost_total.sh`
  et ce document le sont. Aucun commit, aucune publication.
- Le md5 `6f2715e5c5fe26c3e26f6ac404051bd2` est une **référence documentée**
  (C13_FIX.md §5.2). Le script le compare mais ne l'impose pas : le critère de
  C1.3 est `F1 == F2` sur le source courant. Si la source évolue, un md5
  différent avec `F1 == F2` reste un C1.3 valide et est signalé comme tel.
- Le script n'invente jamais un succès : une étape sans compilateur généré
  disponible est IGNORÉE (quick) ou BLOQUÉE (full), et chaque divergence de h
  est affichée avec les deux sorties et les deux rc.
