# DÉMARRAGE / CARRÉS — attaque glb3 (CONSTAT DATÉ, 20/11/2026)

**Auteur** : sous-agent `glb3`. **Aucun commit, aucune publication, aucun
`rose/rose` remplacé.** Binaire construit dans `/tmp/opencode/glb3_agents/` avec
`CARGO_TARGET_DIR` isolé, jamais copié dans l'arbre.

> **Verdict court** : les deux postes sont attaqués avec succès sur leur part
> **contrôlable** : pré-exécution complète (parse+RIR+JIT) divisée par **1,98**
> (c16) et **2,04** (c6), carrés par **1,66**.
> Le poste « hors-chrono complet » (mur − calcul interne) n'est **pas** divisé
> par 2 (×1,41 c16 / ×1,22 c6 sur les salves finales) : le plancher de démarrage processus/harnais
> (~0,93 ms) est hors de mon périmètre. Aucune triche : pas de formule fermée,
> pas de résultat codé en dur, pas de cache disque entre échantillons, 11
> valeurs identiques vérifiées à chaque tirage. **Rust reste devant** ; aucune
> victoire revendiquée sur le global.

---

## 0. Protocole réellement exécuté

| Élément | Mesure |
|---|---|
| HEAD annoncé / final | `2d0e80dbc06b7af2ca651fab4c7e2d5e6bd52fb3` (inchangé ; aucun commit) |
| Binaire témoin (HEAD, arbre courant) | `/tmp/opencode/glb3_agents/rose_head_baseline` = copie de `/tmp/opencode/glb2_agents/rose_final`, SHA256 `b699249d…abaa`, **static-pie** vérifié |
| Binaire final | `/tmp/opencode/glb3_agents/rose_final`, SHA256 `7a297c7301a783573fc5a7e5ee5b947f8350fc602aa47dee48a9cc18fc117614`, static-pie, identique avant/après validation |
| ⚠ Piège de build corrigé | un premier binaire construit avec `RUSTFLAGS=""` **écrasait** le `crt-static` de `.cargo/config.toml` → dynamique, +0,25 ms de démarrage. Toutes les mesures ci-dessous sont sur des binaires **statiques**, construits avec `RUSTFLAGS` non posé. |
| Pinage | **`taskset -c 6` ET `taskset -c 16`** (capacités 997 / 1017), re-mesurées dans `/sys/devices/system/cpu/cpu*/cpu_capacity` |
| Échantillons | **15 par implantation et par cœur**, entrelacés en rotation avec alternance A/B à chaque répétition, médianes (la décomposition interne : 9 échantillons entrelacés) |
| Horloges | H1 = mur `time.perf_counter_ns()` autour du sous-processus ; H2 = `temps()` interne (ns) via une copie instrumentée du banc ; H3 = `ROSE_TIMING` (infrastructure permanente, pas une instrumentation ajoutée) |
| Valeurs | 11 lignes vérifiées à **chaque tirage** ; 15 × 2 × 2 = 60 exécutions finales |
| Fenêtre | `2026-11-20T05:59Z` → `06:22Z`. Tout chiffre ci-dessous est un **constat daté** |
| Bruit | mur Rose A/A : 5,6–7,4 ms ; Rust 2,1–3,1 ms (médianes de salves). Les ratios mur vs Rust sont donc à lire avec prudence |

---

## 1. Décomposition précise du poste « parse / JIT / démarrage »

### 1.1 Poste complet (H1 mur − H2 calcul interne), témoin HEAD, c16

| Sous-poste | Durée mesurée | % du poste | Dans mon périmètre ? |
|---|---:|---:|---|
| Démarrage processus + `main` (`rose version` mur) | ~930 µs | 34 % | ❌ (chargeur + `main.rs`) |
| ├─ dont plancher harnais (`taskset`+subprocess+chargeur, binaire Rust trivial) | ~900 µs | 33 % | ❌ |
| ├─ dont surcoût spécifique Rose | ~30 µs | 1 % | ❌ |
| Lectures fichier + spawn du fil 512 Mo + sortie | ~340 µs | 12 % | ⚠ partiel (3 lectures → 2 après correction) |
| **Parse** (`front::analyser_fichier` : canonicalize + read + lexing + parsing + repli + vérification) | **85 µs** | 3 % | ✅ |
| Transform (extraction boucles + RIR + `prog.clone()`) | 16 µs | 1 % | ✅ |
| `Jit::nouveau` (drapeaux Cranelift, table de symboles runtime, module) | 40 µs | 1 % | ✅ |
| **Compilation Cranelift** (`compiler_tout`) : eligibles 25 µs + declare 2 µs + 3 × `define_function` (valeurs instantanées 38–550 µs, très bruitées) + **`finalize_definitions`** (instantané 490–1 560 µs) | **777 µs** (total, médiane 15 échantillons) | **29 %** | ✅ |
| `compiler_principal` | 2 µs | <0,1 % | ✅ |
| **TOTAL poste** | **~2,71 ms** | 100 % | — |

⚠ Les durées des sous-phases affichées par `ROSE_TIMING` sont des lectures
instantanées à forte dispersion (ex. `define_function` 38–550 µs, `finalize`
490–1 560 µs sur 5 tirages) et **ne s'additionnent pas** au total : seule la
médiane du total `compiletout` (777 µs) est utilisée pour les ratios.

**Goulot #1 du poste = la compilation Cranelift + `finalize` (777 µs, 29 %)**,
devant le démarrage processus (~930 µs, mais 900 µs sont le plancher du
harnais, non modifiable). À l'intérieur de la part contrôlable
(pré-exécution = 906 µs), Cranelift représente **86 %**.

### 1.2 Vérification du plancher (étalon réel, pas une extrapolation)

Binaire Rust trivial (`/tmp/opencode/glb2_agents/triv_rs`) : mur `version` =
**0,905 ms (c16) / 0,934 ms (c6)**. Rose `version` static-pie = **0,934 / 0,881 ms**.
Le démarrage spécifique à Rose n'est donc que ~30 µs ; le reste est le coût du
sous-processus `taskset` + chargeur, **identique pour Rust et Rose**, et que
aucune décision dans `jit.rs`/`front.rs`/`run.rs`/`interp.rs` ne peut réduire.

---

## 2. Décisions structurelles prises

### 2.1 Lecture racine redondante supprimée (`front.rs`, `interp.rs`) — correcte mais immatérielle

`executer_fichier` lisait le source, puis `executer_source_analyse` l'ignorait
et `front::analyser_fichier` relisait le même fichier. Nouveau chemin
`analyser_fichier_source(chemin, source)` : le source racine déjà en mémoire est
réutilisé, les **imports restent lus et résolus exactement comme avant**. Gain
mesuré : 1–5 µs (lecture en cache) — correctness conservée, effet négligeable.

### 2.2 Superinstruction NEON `umlal` pour `acc += a*a` (`jit.rs`) — décision majeure

Le détecteur strict existant `Emetteur::detecter_carres_acc` (bornes `0 ≤ debut
< fin ≤ 2^24`, forme RIR exacte à 6 opérations) reconnaît la boucle. Au lieu de
la compiler en Cranelift (16 accumulateurs i64 + vidage i128), on l'exécute
par une boucle Rust native vectorisée NEON :

* `vmlal_u32` = produit ÉLARGI u32×u32→u64 **accumulé** en une seule
  instruction (2 carrés/instruction, contre 2 instructions/carré en scalaire) ;
* 8 voies u64 (`4 × uint64x2`), vidage toutes les 8192 itérations vectorielles
  dans un total i128.

**Preuve d'exactitude** : `a ≤ 2^24` ⇒ `a` tient en u32 et `a² ≤ 2^48` ; le
produit élargi est entier, sans arrondi ni réassociation ; chaque voie reçoit
≤ 8192 termes ⇒ `≤ 2^48 · 2^13 = 2^61 < 2^64`, aucun débordement ; le total
i128 rend une somme **bit-à-bit identique** au chemin Cranelift remplacé.
Aucune formule fermée (les `fin − debut` carrés sont calculés un à un), aucun
résultat codé en dur, aucun cache disque.

**Garde de sécurité** : la superinstruction est désactivée si `$principal` est
JIT-compilé (ses appels directs exigent le pointeur Cranelift) et hors
aarch64 — le chemin Cranelift d'origine reste intact comme repli.

**Itéré à la mesure** : 1 accumulateur (prototype) → 4 × `uint64x2` retenu.
Le prototype scalaire Rust à 16 accumulateurs (1,316 ms) était plus lent que le
JIT ; la vectorisation seule apporte le gain.

### 2.3 Réglage Cranelift testé puis REJETÉ

`ROSE_JIT_OPT=none` réduit `compiletout` de ~40 µs sur le global (342→299 µs
c16) mais **régresse D3 (primes 50k) de 0,851 → 1,077 ms, +26 %, sur c16 ET c6**
(9 échantillons entrelacés). Le défaut reste `speed`. Rejet documenté, pas
appliqué — aucune régression ailleurs.

---

## 3. Gains réels (15 échantillons entrelacés, médianes, static-pie)

### 3.1 Global complet — final vs témoin HEAD

| pin | témoin H1 mur | final H1 mur | ratio mur | H2 interne témoin | H2 interne final |
|---|---:|---:|---:|---:|---:|
| c16 | 6,858 ms | **5,563 ms** | **1,233 ×** (−19 %) | 4 133 022 ns | **3 630 460 ns** |
| c6 | 6,487 ms | **5,597 ms** | **1,159 ×** (−14 %) | 4 169 981 ns | **3 691 552 ns** |

### 3.2 Carrés — section 1 du banc (`temps()`, ns, 9 échantillons)

| pin | témoin | final | gain |
|---|---:|---:|---:|
| c16 | 1 257 663 ns (30,4 % interne) | **759 103 ns (20,9 %)** | **1,657 ×** |
| c6 | ~1 255 000 ns | ~759 000 ns | ~1,65 × |

Le goulot #1 du calcul interne est désormais **les rationnels (23,4 %)**,
devant les carrés (20,9 %) : changement de classement mesuré, pas supposé.

### 3.3 Pré-exécution complète — H3 `ROSE_TIMING` (15 échantillons entrelacés)

Ventilation complète `parse + transform + jitnew + compiletout + compileprincipal` :

| pin | témoin | final | gain |
|---|---:|---:|---:|
| c16 | 906 µs (parse 89 + transform 14 + jitnew 25 + compiletout 777 + principal 2) | **457 µs** (82 + 12 + 24 + 337 + 2) | **1,98 ×** (à 4 µs de la porte) |
| c6 | 985 µs (98 + 14 + 29 + 841 + 2) | **482 µs** (86 + 13 + 25 + 357 + 2) | **2,04 ×** ✅ |

`compiletout` seul : 777 → 337 µs (c16, **2,30 ×**), 841 → 357 µs (c6,
**2,36 ×**). Le goulot #1 de la part contrôlable est divisé par plus de 2 sur
les deux cœurs ; la pré-exécution totale est divisée par 2 sur c6 et s'en
écarte de 0,9 % sur c16 (dans le bruit).

### 3.4 Portes atteintes ou non — honnêteté absolue

| Porte | Résultat |
|---|---|
| Carrés réduits « sensiblement » | ✅ **1,66 ×** (1,26 → 0,76 ms) |
| Pré-exécution (part contrôlable) divisée par 2 | ✅ sur c6 (**2,04 ×**) ; ⚠ **1,98 ×** sur c16 (à 0,9 % / 4 µs de la porte) |
| Poste complet « mur − interne » divisé par 2 | ❌ **×1,41 (c16) / ×1,22 (c6)** sur les salves finales à 15 échantillons (×1,7 sur une salve antérieure plus bruitée). Raison chiffrée §4 |
| Battre Rust sur le global | ❌ jamais revendiqué (ratio final c16 2,88, c6 2,57 ; Rust déborde ses i64 et utilise des f64 — sémantique différente, documentée ailleurs) |

---

## 4. Reste chiffré — pourquoi la porte « poste complet ×2 » n'est pas atteinte

Plancher arithmétique mesuré : `poste = démarrage (~930 µs, dont ~900 µs de
plancher harnais non contournable) + lecture/spawn/sortie (~340 µs) +
pré-exécution (457 µs)` = **~1,73 ms** final, contre ~2,71 ms au témoin. La
cible « moitié de 2,71 » est **1,355 ms** : elle est arithmétiquement
atteignable (le plancher fixe est ~1,27 ms) mais exigerait de ramener la part
contrôlable de 457 µs à **≤ 85 µs**, soit **−81 %** supplémentaires — sans
marge face au bruit. Concrètement, il faudrait supprimer le parse **et** la
compilation Cranelift restante, ce qui sort des deux postes assignés. Je ne
prétends donc **pas** avoir divisé le poste complet par 2 ; seule la
pré-exécution l'est (§3.3).

Reste actionnable **dans** le périmètre, non fait (coût/bénéfice défavorable ou
hors mission) :

* `finalize_definitions` résiduel (~200 µs) pour `fib` + `__rose_loop_1` :
  `ROSE_JIT_OPT=none` le réduit mais casse D3 (§2.3) ; supprimer Cranelift pour
  ces deux fonctions exigerait un moteur natif pour la récursion binaire et la
  division flottante, avec garde bigint — hors objectif des deux postes ;
* lectures fichier restantes (~100 µs) : une lecture est dans `main.rs`
  (hors périmètre), une dans `interp.rs` (nécessaire au parse) ;
* carrés 759 µs : déjà SIMD exact ; un déroulage supplémentaire augmenterait la
  pression de registres (glb2 avait mesuré la régression à 32 accumulateurs) ;
* rationnels 850 µs : goulot #1 interne désormais, mais `i128x.rs` est interdit
  et la sémantique exacte doit rester intacte.

---

## 5. Validation (preuves de sortie réelles, binaire final)

| Gate | Résultat |
|---|---|
| `cargo build --release --bin rose` (target dir isolé, `RUSTFLAGS` non posé) | **OK**, static-pie, SHA256 `7a297c73…c117614` |
| `cargo test --release` | **431 passed, 0 failed**, 6 ignored (220 + 204 + 0 + 6 + 1 + 0) |
| Jardin `jardins/tests/premiers.rose` | `── jardin/premiers : 20 tests, 0 échec ──` |
| `fib(25)` | `75025` |
| Point fixe (2 voies) | `b3ed9262dd54776a6dbd07bc12368d0e` **×2** (`rose rir …` et `rose … …`) |
| 9 domaines multi, c16 **et** c6 | **81/81** valeurs conformes par cœur (défaut `speed`) |
| `bench_global.rose` | **rc = 0**, 12 lignes, **11 valeurs identiques** à chaque tirage (60 exécutions finales) |
| Différentiel carrés (bornes 41 / 777 / 2^24, init −7, 5→6, 1→2) | sorties **octet pour octet identiques** au témoin HEAD, grands entiers inclus |
| `ROSE_JIT_OPT=none` | rejeté : D3 +26 % (documenté §2.3), défaut `speed` conservé |
| Propreté | aucun `PROF`/`DBG` ajouté (seule l'infrastructure permanente `ROSE_TIMING` a été lue) ; aucun commit, aucune publication, `rose/rose` inchangé |
| Périmètre | `git status` ne montre que `rose/src/front.rs`, `rose/src/interp.rs`, `rose/src/jit.rs` (+ `compilateur/emetteur_elf.rose`, travail non committé d'un autre agent, **non touché**) et ce document |

---

## 6. Commandes rejouables

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cd /home/clausake/Stefan/ROSE/rose
unset RUSTFLAGS                      # ⚠ garder le crt-static de .cargo/config.toml
export CARGO_TARGET_DIR=/tmp/opencode/glb3_agents/target
cargo build --release --bin rose
cp /tmp/opencode/glb3_agents/target/release/rose /tmp/opencode/glb3_agents/rose_final
sha256sum /tmp/opencode/glb3_agents/rose_final

# Global A/A/A 15 échantillons, c16 + c6, 11 valeurs vérifiées
cd /home/clausake/Stefan/ROSE/bench/global
python3 /tmp/opencode/glb3_agents/global_bench.py \
    /tmp/opencode/glb3_agents/rose_head_baseline /tmp/opencode/glb3_agents/rose_final
python3 /tmp/opencode/glb3_agents/global_bench.py /tmp/opencode/glb3_agents/rose_final

# Décomposition par section (temps(), ns)
python3 /tmp/opencode/glb3_agents/prof_sections.py /tmp/opencode/glb3_agents/rose_final speed
# Ventilation H3
ROSE_TIMING=1 taskset -c 16 /tmp/opencode/glb3_agents/rose_final bench_global.rose >/dev/null
```

Tous les harnais, journaux et binaires sont dans `/tmp/opencode/glb3_agents/`.
Aucun fichier du dépôt hors de mon périmètre n'a été modifié.
