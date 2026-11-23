# VAGUE FINALE — le TEMPS MUR (constats datés du 23/11/2026)

## 1. Mission et périmètre

Le banc `bench/perf` (9 tirages, cœur 15) montre Rose wall **2,0–5,6 ms**
contre Rust **1,9–4,5 ms**, alors que le calcul interne est souvent 5–14×
plus rapide (rationnel 0,110×, chaîne 0,081×, puissance 0,465×). L'agent
startup avait attribué l'écart à (a) un binaire *static-pie* avec des
milliers de relocations `RELA` + `BIND_NOW`, et (b) la préparation JIT
Cranelift sur programme minimal.

Fichiers possédés, SEULS modifiés : `rose/.cargo/config.toml`, `rose/build.rs`,
`rose/src/lib.rs` (uniquement le commentaire d'initialisation JIT),
`rose/tests/opt_wall_final.rs` (nouveau), `rose/docs/OPT_WALL_FINAL.md`
(nouveau). **Interdits et intactes** : `main.rs`, `demarrage.rs`, `jit.rs`,
`interp.rs`. Aucun commit, aucune publication, aucun fichier du banc modifié.

## 2. Protocole anti-variance

* **Binaire** : sha256 calculé avant ET après chaque salve (identique, voir
  `measure.json` → `sha_stable: true`). Les quatre variantes sont construites
  depuis le MÊME source HEAD `8886c1d`, même `lto=fat`/`codegen-units=1`,
  seul le drapeau d'édition de liens diffère.
* **Horloge** : `time.perf_counter_ns` autour de `fork / execve / wait4` —
  `/usr/bin/time -f %e` arrondit à 0,01 s et est illisible sur 3 ms.
* **Pin** : `taskset -c 15` (gouverneur `performance`, 20 cœurs).
* **Entrelacement** : **21 lots**, chaque lot =
  `[base, variantes pivotées..., base]`. `base` encadre toutes les variantes,
  donc la différence appariée `variante − base` annule la dérive lente.
  2 échauffements par (variante, domaine), non comptés.
* **Estimateurs** : médiane des différences appariées (signal), médiane des
  valeurs brutes et minimum absolu (coût intrinsèque), nombre de lots où la
  variante est PLUS LENTE que `base` (significativité : ~10–14/21 = bruit).
* **Vérification** : chaque tirage doit rendre la `RESULT:` de référence
  Python ; `rc != 0` fait échouer la salve. Aucun tirage rejeté.
* Deux salves indépendantes (11 puis 21 lots) ont été exécutées ; les chiffres
  ci-dessous sont ceux de la salve définitive à 21 lots, la première servant
  de confirmation de signe.

## 3. Coût mesuré des relocations et de la préparation JIT

### 3.1 Relocations (lecture ELF, `readelf`)

| variante | `file` | relocations | drapeaux dynamiques |
|---|---|---|---|
| **base** (témoin conservé) | static-pie | **6 288** (6 275 `R_AARCH64_RELATIV` + 5 `R_AARCH64_IRELATI` + en-têtes) | `BIND_NOW`, `FLAGS_1: NOW PIE` |
| non-PIE | statically linked | **8** (5 `IRELATI` seulement) | aucun `PIE`/`NOW` |
| lazy | static-pie | 6 288 (identiques) | `FLAGS_1: PIE` (plus de `NOW`) |
| RELR/lld | static-pie | 6 284 (dépliées) | `RELR` 1 968 o = **246 entrées** au lieu de 6 275 entrées RELA (~150 Ko) |

Coût isolé = `rose version` (démarrage PUR, aucun programme, aucun JIT) :
la suppression des 6 275 `RELATIV` fait gagner **−0,061 ms** (salve 21 lots)
et **−0,114 ms** (salve 11 lots), soit un coût réel de **~0,06–0,11 ms**
pour le chargeur. Retirer `BIND_NOW` seul fait gagner −0,059 / −0,085 ms.

### 3.2 Préparation JIT (`ROSE_TIMING=1`, `debug-lenteur -n 9`)

```
debug-lenteur (médianes, 9 répétitions) :
  lancement (rose version)     534 µs
  état (rose info)             376 µs
  programme minimal            806 µs   (surcoût d'exécution 271 µs)
  analyse seule (rose latex)   673 µs

ROSE_TIMING=1 rose <programme minimal>, 5 tirages :
  compiletout = 426,5 / 193,9 / 215,3 / 192,9 / 207,6 µs  → MÉDIANE 207,6 µs
  jitnew      = 31,5 / 18,7 / 24,9 / 17,1 / 18,3 µs        → MÉDIANE 18,7 µs
  parse       ≈ 20 µs ; exec ≈ 21 µs
```

**Attribution** : sur un programme minimal, le mur ~2,1–2,5 ms se décompose
en ~1,3 ms d'`execve`/chargement de base, ~0,06–0,11 ms de relocations,
~0,2 ms de préparation JIT (`compiletout`) et ~40 µs de parse/exécution.
Le mur est donc dominé par le **chargement + le JIT**, et les relocations ne
sont qu'un petit poste : c'est pourquoi les leviers d'édition de liens ne
peuvent pas, à eux seuls, combler l'écart avec Rust.

## 4. Leviers testés et décisions

### 4.1 `-C relocation-model=static` (non-PIE) — **REJETÉ**

6 275 relocations supprimées, gain réel sur `version` (−0,061 / −0,114 ms),
mais **aucun gain sur les domaines qui exécutent un programme** :

| domaine | base méd. | non-PIE méd. | Δ apparié (21 lots) | confirmation (11 lots) |
|---|---|---|---|---|
| version | 2,053 | 1,660 | **−0,061** | −0,114 |
| startup | 2,296 | 1,922 | +0,014 | +0,012 |
| rationnel | 2,126 | 1,853 | +0,008 | −0,037 |
| chaîne | 2,046 | 1,954 | +0,014 | +0,058 |
| fib_iter | 3,369 | 3,334 | +0,045 | +0,034 |

Sur les 4 domaines d'exécution, `lots_plus` = 11–14/21 : le signe est du
bruit, et la tendance est **neutre à plus lente**. La préparation JIT
(~0,2 ms) écrase le gain de 0,06–0,11 ms. **Trade-off de sécurité** : le
non-PIE supprime l'ASLR du binaire (adresses de code, de pile et de tas
prévisibles) — une régression de défense en profondeur pour un gain de mur
non démontré. Rejeté.

### 4.2 `-Wl,-z,lazy` (lazy binding) — **REJETÉ**

`BIND_NOW` disparaît (FLAGS_1 = `PIE` seul) mais les 6 275 `RELATIV`
restent. Gain sur `version` −0,059 / −0,085 ms ; sur l'exécution :

| domaine | base méd. | lazy méd. | Δ apparié (21 lots) | confirmation (11 lots) |
|---|---|---|---|---|
| version | 2,053 | 1,641 | −0,059 | −0,085 |
| startup | 2,296 | 2,019 | +0,020 | −0,020 |
| rationnel | 2,126 | 1,796 | −0,003 | +0,070 |
| chaîne | 2,046 | 2,019 | +0,008 | +0,052 |
| fib_iter | 3,369 | 3,302 | +0,039 | +0,122 |

Le signe change d'une salve à l'autre sur `startup`/`rationnel` : pas de
gain reproductible sur le mur. De plus, perdre `BIND_NOW` affaiblit la
protection RELRO complète (résolution différée de la table GOT). Rejeté —
d'autant que l'ASLR, elle, est conservée mais sans bénéfice mesuré.

### 4.3 Réduction des symboles exportés vers le JIT — **SANS OBJET (mesuré)**

Le JIT n'utilise plus `dlsym` : `jit.rs::table_symboles_runtime()` enregistre
ses ~38 symboles **par adresse** dans la table du module Cranelift (1,2 ms
économisés en amont). En static-pie, `.dynsym` ne contient que **3 entrées de
section**. Expérience : retirer `-rdynamic` dans `build.rs` produit un binaire
**bit-identique** au témoin (sha `8896ace22cc1dbff…` des deux côtés). Il n'y
a donc aucun symbole exporté à réduire. Le `-rdynamic` est conservé pour les
cibles non statiques, avec un commentaire de mesure dans `build.rs`.

### 4.4 `-Wl,-z,pack-relative-relocs` via lld (bonus) — **REJETÉ**

6 275 entrées RELA (~150 Ko) compressées en 246 entrées RELR (1 968 o).
L'option est **ignorée avec GNU ld 2.42** (`-z pack-relative-relocs ignored`),
il faut lld. Résultat sur le mur :

| domaine | Δ apparié vs base (21 lots) |
|---|---|
| version | −0,003 |
| startup | +0,037 |
| rationnel | +0,067 |
| chaîne | +0,016 |
| fib_iter | +0,033 |

Régression sur l'exécution (code layout lld différent, traitement RELR du
noyau aarch64 pas plus rapide ici). Rejeté.

### 4.5 Contrôle croisé avec le harnais officiel

Deux salves complètes `bench/perf/harness.py` (9 tirages, rotation des
langages, c15) ont été exécutées : `base` (sha `8896ace2…`) et `non-PIE`
(sha `48b30659…`). Les temps INTERNES sont bit-identiques (ex. rationnel
0,061 / 0,062 ms ; chaîne 0,003 / 0,003 ; fib_iter 1,084 / 1,084) — aucune
régression fonctionnelle. Les deltas de wall entre salves vont de −0,52 à
+0,63 ms **sans direction cohérente** : c'est de la dérive entre salves,
pas un effet de l'éditeur de liens. Cela confirme que seul le protocole
A/B/A entrelacé (§2) est apte à trancher.

## 5. Décision finale

**Aucun levier n'est appliqué.** Le binaire reste **static-pie + BIND_NOW**
(6 275 RELATIV), ce qui préserve l'ASLR et la protection RELRO complète.
Les gains mesurés ne existent que sur `version` (démarrage sans JIT) et
sont écrasés par la préparation JIT sur tous les domaines d'exécution du
banc. Le mur Rose/Rust ne se corrige donc PAS par un drapeau d'édition de
liens : il faudrait agir sur la décision JIT pour les micro-programmes —
or le prototype de l'agent startup (refus de Cranelift sur RIR court sans
saut) donnait −0,45 ms sur `startup` mais **+0,11 ms de régression sur
`fib_iter`**, donc rejeté lui aussi. Aucun changement de linking n'est
appliqué à l'aveugle : `opt_wall_final.rs` échouera volontairement si un
drapeau change sans re-mesure.

## 6. Validation (preuves d'exécution)

* `cargo test --release` (`RUST_TEST_THREADS=1`, `CARGO_TARGET_DIR` partagé) :
  **22 harnais, 714 tests passés, 0 échec**, dont les **4 nouveaux**
  `opt_wall_final` (version stable, programme JIT conforme, ventilation
  TIMING sur stderr, configuration ELF = static-pie + BIND_NOW + RELATIV).
* `bash scripts/validation_finale.sh` (`/tmp/opencode/validation_finale.log`,
  23/11 06:30 UTC) : **0 échec** — point fixe émetteur `b3ed9262…`,
  **AOT 75/75**, **JIT 25/25**, WASM 92 o, RVM 317811,
  AZ 31 CONFORME / 0 non-conforme, self-host HOST-1 fib=317811.
* Commandes `version`/`info`/`aide` et sorties du banc inchangées (sha et
  md5 contrôlés, internes bit-identiques entre témoins et variantes).

## 7. Diff précis

```
rose/.cargo/config.toml     | +23 lignes  (commentaire de décision mesurée :
                            |  3 leviers rejetés, chiffres, trade-off ASLR/RELRO)
rose/build.rs               |  +7 lignes  (commentaire : -rdynamic = no-op
                            |  mesuré en static-pie, symboles JIT par adresse)
rose/src/lib.rs             |  +7 lignes  (commentaire : init JIT sans coût
                            |  global différable, pas de OnceLock/table à réduire)
rose/tests/opt_wall_final.rs| nouveau, 4 tests de garde-fou (configuration +
                            |  conformité fonctionnelle, AUCUNE mesure de durée)
rose/docs/OPT_WALL_FINAL.md | nouveau, ce document
```

Aucune modification de `main.rs`, `demarrage.rs`, `jit.rs`, `interp.rs`,
`Cargo.toml`, des scripts ou du banc. Données brutes des salves :
`/tmp/opencode/opt_wall_bins/meas/measure.json` (A/B/A),
`/tmp/opencode/opt_wall_bins/results_{base,nopic}.json` (harnais officiel).
