# VALIDATION FINALE — audit indépendant des vagues 9 & 10

> ⚠ Ce document est un **CONSTAT DATÉ**, pas une consigne. Chaque chiffre ci-dessous
> a été **mesuré** au horodatage indiqué, sur l'état du dépôt décrit. Aucun chiffre
> n'est recopié d'un document antérieur. Re-mesurer après tout changement de code.

## 0. Périmètre et méthode

- Mission : audit de validation **indépendant** des changements des vagues 9 et 10
  (moteur modifié par I1, types par un agent, G2 par un autre).
- Une seule porte d'écriture autorisée : ce fichier. **Aucun autre fichier modifié,
  aucun commit effectué par cet audit.**
- Commandes exécutées réellement (sorties exactes conservées dans
  `/tmp/opencode/audit-indep/`) :
  1. `bash scripts/audit-final.sh` (script de QA lecture seule du dépôt) ;
  2. mesures complémentaires : `fib(35)`, `verifier_factorielles.rose`,
     comparaison des sorties de `factorielles.rose 5000` ;
  3. `cargo build --release --bin rose` ;
  4. `cargo test --release` (global) ;
  5. inspections lecture seule : `git log/show/status/diff`, `grep`, `md5sum`, `nm`.

## 1. État du dépôt au moment des mesures

| Élément | Valeur mesurée |
|---|---|
| Audit `scripts/audit-final.sh` | début **2026-11-18T14:41:38+01:00**, fin **14:45:17+01:00** |
| Log brut | `/tmp/opencode/audit-indep/audit-final-20261118T144138.log` |
| Binaire audité | `/home/clausake/Stefan/ROSE/rose/rose` |
| Binaire — md5 | `5f97012ca8ddbdc4b62dcf0731a02ac1` |
| Binaire — sha256 | `ab01f8fd43f4fda304706697d1fe9e91981b50746d86ab282f8412770d665ee1` |
| Binaire — mtime | `2026-11-18 14:30:53.352392623 +0100` |
| Binaire — type | ELF 64-bit LSB PIE, aarch64, static-pie, stripped, BuildID `48aa0a600e7de3a20f39090b4a72984531841ace` |
| HEAD pendant l'audit | `776ff35bb686f860da2135ab4388e6ffdabdb55c` |
| `origin/main` pendant l'audit | `776ff35bb686f860da2135ab4388e6ffdabdb55c` (égaux) |
| `git status --short` pendant l'audit | `(propre)` |

**Stabilité du binaire (mesurée, pas supposée)** : md5 `5f97012c…` relevé
avant l'audit (14:41:20), après l'audit (14:45:17), puis **après** un
`cargo build --release --bin rose` rejoué à 14:46:16 → **identique à trois
reprises**. Le binaire audité est donc reproductible depuis les sources de
l'état mesuré.

**Mouvement git concurrent (constat)** : au tout premier relevé de cette session
(14:41:09) HEAD était `1855ebc6af5e2751b46ae48c83301b115cbde7ca` ; le commit
`776ff35` est arrivé avant le démarrage du script (14:41:38). Les deux commits
`1855ebc` et `776ff35` ne modifient **que `PLAN.md`** (`git show --stat` mesuré) :
le code et le binaire audités ne sont donc pas affectés par ce mouvement.

## 2. Résultats des portes — TOUTES VERTES dans le périmètre mesuré

### 2.1 Oracles M4 — 4 oracles, 151 fichiers, comparaison octet pour octet

Corpus réel mesuré par `find examples jardins bench rose-cpp/tests -name '*.rose'`
= **151 fichiers** (le texte du script annonce « 150 fichiers » : écart de
documentation du script, **pas** une régression du moteur).

| Oracle | Commande référence vs composant | Résultat mesuré |
|---|---|---|
| M4.1 lexeur | `rose tokens` vs `rose compilateur/lexeur.rose` | **151 match / 0 écart** |
| M4.2 analyseur | `rose ast` vs `rose compilateur/analyseur.rose` | **151 match / 0 écart** |
| M4.3 vérificateur | `rose verif` vs `rose compilateur/verifieur.rose` | **151 match / 0 écart** |
| M4.4 émetteur RIR | `rose rir` vs `rose compilateur/emetteur.rose` | **151 match / 0 écart** |

Auto-vérification des composants (`rose verif compilateur/<c>.rose`) :
`lexeur vérifié`, `analyseur vérifié`, `verifieur vérifié`, `emetteur vérifié`,
`compilateur vérifié`.

Référence historique `scripts/valider-chantiers.sh` (corpus **différent**, hors
périmètre des 151 — informationnel, aucune baseline de comparaison disponible) :
`lexeur : 9/9`, `analyseur : 47/47`, `vérificateur : 62/62`, `émetteur : 44/47`
(erreurs émission : 0). Ce 44/47 n'est **pas** retenu comme régression des
vagues 9/10 : il porte sur l'ancien corpus et n'a pas été comparé à un état
antérieur mesuré.

### 2.2 Point fixe M4.5

```
rose rir compilateur/emetteur.rose                       : bdbef63681608a0b22a5637539e9d738
rose compilateur/emetteur.rose compilateur/emetteur.rose : bdbef63681608a0b22a5637539e9d738
```
**CONSTAT : MD5 ÉGAUX — point fixe octet pour octet ATTEINT.**

### 2.3 Perf M2

- Jardin `jardins/tests/premiers.rose` : `── jardin/premiers : 20 tests, 0 échec ──`
  → **20/20**. Temps (3 échantillons) : `0.00 s`, `0.00 s`, `0.00 s`.
- `fib(25)` = **75025** → conforme.
- **`fib(35)` (mesure complémentaire)** : **9227465** sur 5 échantillons consécutifs,
  temps `0.02 s` chacun (5/5). Valeur correcte (F35 = 9227465).
- `factorielles.rose 5000` (3 échantillons) : `0.12 s`, `0.12 s`, `0.11 s` ;
  `factorielle n! : 2 ms, 16326 chiffres` ; `md5(chiffres) = 6ceab791bd8bf35106485a9d77fea516`
  identique sur les 3 échantillons.
- **Valeurs des factorielles (mesure complémentaire)** : la vérification des
  valeurs se fait par `bench/suite/verifier_factorielles.rose` (le bench
  n'affiche que des longueurs). Résultat mesuré : **rc=0, 65 `✓`, 0 `✗`,
  `FIN DE LA VÉRIFICATION`**. En complément, trois exécutions complètes de
  `factorielles.rose 5000` ont donné des md5 bruts différents, mais le `diff`
  montre que **seuls les chronomètres (ms) varient** ; les 8 nombres de chiffres
  extraits sont identiques entre les 3 runs (md5 `6ceab791bd8bf35106485a9d77fea516`) :
  `16326, 8164, 5444, 4083, 2134, 16326, 16326, 1177246`. → **valeurs identiques**.
- `bench_global.rose` exécuté depuis un dossier temporaire : **rc = 0**, dernière
  ligne `652`.

### 2.4 Backend M3 (audit, profil debug par défaut)

| Commande | rc | Détail |
|---|---|---|
| `cargo test --lib link_natif` | 0 | 7 tests passés, 0 échec |
| `cargo test --lib natif_emetteur` | 0 | 6 tests passés, 0 échec |
| `cargo test --bin rose` | 0 | 144 tests passés, 0 échec (30.61 s) |

### 2.5 Invariants

- `rug` absent du code Rust (`grep -rn rug rose/src --include='*.rs'` = vide) et
  de `rose/Cargo.toml` / `Cargo.lock` (vide) → **CONFIRMÉ**. Le seul « rug » de
  `rose/src` est une sous-chaîne dans un URI base64 de `setup.html`, pas la crate.
- Fichiers `.rs` sous `jardins/` : **0** → aucun jardin en Rust, CONFIRMÉ.
- Occurrences « jardin … rust » dans du code `.rose`/`.rs` : aucune, CONFIRMÉ.
- Version : `version = "0.1.5"`.
- Remote : `git@github.com:Stef302/Rose.git` (propriétaire correct, pas `clausake`).

### 2.6 Portes de build release (mesures complémentaires)

| Commande | Horodatage | rc | Résultat |
|---|---|---|---|
| `cargo build --release --bin rose` | 14:46:09 → 14:46:16 | **0** | `Finished release profile [optimized] target(s) in 7.00s` ; 227 warnings (138 duplicates), **0 error** ; binaire md5 `5f97012c…` (identique à celui audité) |
| `cargo test --release` (global) | 14:46:27 → 14:46:39 | **0** | **311 tests passés, 0 échec** (voir détail ci-dessous) |

Détail `cargo test --release` (lignes exactes) :
```
running 160 tests → test result: ok. 160 passed; 0 failed
running 144 tests → test result: ok. 144 passed; 0 failed
running   0 tests → test result: ok.   0 passed; 0 failed
running   6 tests → test result: ok.   6 passed; 0 failed
                    test result: ok.   1 passed; 0 failed   (doc-tests)
running   0 tests → test result: ok.   0 passed; 0 failed
```
Cibles exécutées : `src/lib.rs`, `src/main.rs`, `src/bin/rose-setup.rs`,
`src/bin/rose-shell.rs`, `tests/mul_cmp.rs`, `Doc-tests rose`.

## 3. Modules nouveaux — constat d'honnêteté

Au moment des mesures (état `776ff35`, arbre propre) :

- `rose/src/rosevm.rs`, `rose/src/mem_modele.rs`, `rose/src/macho_writer.rs`
  existent mais **ne sont déclarés par aucun `mod`** dans `lib.rs` ni `main.rs`.
- `rose/src/bin/rose-autonome.rs` existe mais **n'est pas déclaré comme `[[bin]]`**
  dans `rose/Cargo.toml` (seuls `rose`, `rose-setup`, `rose-shell` le sont).
- Aucune référence à ces quatre fichiers n'existe ailleurs dans `rose/src` ni
  dans `Cargo.toml` (`grep` exhaustif) ; `nm -C` sur `librose-*.rlib` ne montre
  aucun symbole `rosevm`/`mem_modele`/`macho_writer`.

**Conclusion factuelle** : le build principal n'est **pas cassé**, mais c'est
**trivial** — ces quatre modules ne sont **pas compilés du tout** dans l'état
mesuré. Le vert du build/test **ne valide pas** leur contenu. Il serait faux
d'affirmer qu'ils sont « intégrés et vérifiés ».

## 4. Changements concurrents POSTérieurs aux mesures — NON validés

À partir de **14:47:0x**, soit **après** toutes les mesures ci-dessus, un agent
concurrent a modifié l'arbre de travail (non commité) :

```
 M rose/Cargo.toml
 M rose/src/jit.rs
 M rose/src/lib.rs
 M rose/src/main.rs
 M rose/src/run.rs
```
(`git diff --stat` à 14:47:26 : 5 fichiers, 517 insertions, 5 suppressions.)

Nature observée par `git diff` (lecture seule) : passage de `required-features`
de `["native"]` à `["binaire"]`, `cranelift` rendu optionnel, nouvelle feature
`autonome`, et ajout d'un shim `jit` sans Cranelift dans `lib.rs`/`main.rs`
(câblage G2). **Ces modifications n'ont été ni compilées ni testées par cet
audit** et sont explicitement **hors du périmètre validé**. Le résultat final
de ce chantier G2 reste à mesurer une fois l'arbre stabilisé.

## 5. Verdict

**VERT (mesuré, état `776ff35` propre, 14:41:38 → 14:46:39) :**
1. 4 oracles M4 : 151/151 fichiers chacun, 0 écart ;
2. point fixe M4.5 : atteint (md5 identiques) ;
3. perf M2 : jardin 20/20, fib(25)=75025, fib(35)=9227465 (0.02 s),
   factorielles 65/65 valeurs exactes + valeurs identiques entre runs,
   bench_global rc=0 ;
4. backend M3 : 7/7 + 6/6 + 144/144 (debug) ;
5. build release : rc=0 ; `cargo test --release` global : 311/311, rc=0 ;
6. invariants : rug absent, 0 jardin Rust, version 0.1.5, remote Stef302 ;
7. cohérence git au moment de l'audit : HEAD == origin/main, arbre propre.

**ROUGE : aucune porte rouge dans le périmètre mesuré.**

**Régressions détectées dans le périmètre mesuré : AUCUNE.**

**Non couvert / réserves (à ne pas confondre avec des régressions) :**
- les 4 nouveaux modules ne sont pas câblés au build dans l'état mesuré → leur
  correction n'est pas prouvée par ce audit (§3) ;
- les modifications concurrentes non commitées apparues à 14:47 (G2 autonome)
  sont postérieures aux mesures et **non validées** (§4) ;
- le `44/47` du corpus historique `valider-chantiers.sh` est informationnel,
  sans baseline mesurée, et concerne un corpus distinct des 151 fichiers (§2.1).

---
*Audit réalisé le 2026-11-18 entre 14:41:38 et 14:47:26 (+01:00) — logs bruts :
`/tmp/opencode/audit-indep/`.*
