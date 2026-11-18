# Runner wasmtime réel — M6.2 (construit, mesuré, non publié)

> **État au 18/11/2026 : runner VRAI compilé et porte ≤ 20 ms MESURÉE.
> Aucun artefact n'a été publié, aucun commit n'a été fait.**
> Ce document complète `RUNTIME_RUNNER.md` (contrat) et `PORTE_WASM.md`
> (procédure de mesure). Toute valeur de performance est un **constat daté**,
> pas une consigne : re-mesurer après tout changement de version, de toolchain
> ou de topologie.

---

## 1. Résultat en une ligne

| Élément | Valeur mesurée |
|---|---|
| Runner | `rose-wasmtime-runner` v0.1.5, Rust + **wasmtime 50.0.1** (cranelift + runtime) |
| Artefact | `rose-wasmtime-runner-v0.1.5-aarch64-linux`, 6 493 120 octets |
| **SHA-256 à passer à `ROSE_WASM_RUNTIME_SHA256`** | **`c7b2bf3c2b85e72ec37192f2ba855bd98de0c144288c9cf04aa63f11880739d1`** |
| Fib(27) | `196418` (identique à l'interpréteur maison) |
| Médiane (6 échantillons) | **4,149 ms** (passage 1) et **3,423 ms** (passage 2) |
| Référence maison (interpréteur de pile) | 172,1 ms |
| Seuil M6.2 | ≤ 20 ms → **PASS** (≈ 41× à 50× plus rapide que la référence) |
| Cible | aarch64 / Linux, binaire PIE dynamiquement lié (glibc + libgcc_s) |

---

## 2. Construire (sans rien publier)

```sh
cd /home/clausake/Stefan/ROSE
bash -n scripts/build-runtime-runner.sh      # syntaxe
bash scripts/build-runtime-runner.sh         # build réel, aucune publication
```

Le script est **auto-contenu** : il écrit lui-même le crate dans
`/tmp/opencode/rose-wasmtime-runner` (hors du dépôt, aucun jardin Rust, aucune
dépendance ajoutée à `rose/Cargo.toml`, GMP absente), compile en release,
copie le binaire sous le nom d'artefact attendu, vérifie `fib(27) → 196418`,
puis affiche le SHA-256. Il ne fait **ni** `git commit`, **ni**
`gh release upload`.

Variables optionnelles : `ROSE_RUNNER_BUILD_DIR` (dossier du crate),
`ROSE_RUNNER_TARGET` (suffixe d'artefact), `CARGO_BUILD_JOBS` (défaut 8).

`cargo` est résolu via le PATH puis `~/.cargo/bin/cargo` (il n'était pas dans
le PATH de la session de mesure). Toolchain : `cargo 1.99.1`,
`rustc 1.99.1` ; MSRV de wasmtime 50.0.1 = 1.97.0.

### 2.1 Dépendances

```toml
wasmtime = { version = "=50.0.1", default-features = false,
             features = ["cranelift", "runtime"] }
```

Les features par défaut de wasmtime (component-model, async, gc, profiling,
wit-parser…) sont volontairement désactivées : elles ne servent pas au contrat
et alourdiraient le build. Le cache local ne contenait au départ que des
sous-crates internes (`wasmtime-internal-core-50.0.1`,
`wasmtime-internal-jit-icache-coherence-…`) ; le crate `wasmtime` complet a été
téléchargé depuis `static.crates.io` (accès réseau vérifié : `crates.io`
répondait 403 sur la racine mais l'index cargo et le CDN des `.crate`
fonctionnaient). Un build hors ligne strict **échouerait** ; l'alternative
documentée dans `RUNTIME_RUNNER.md` §2 reste le wrapper
`wasmtime run --invoke`, non autonome (aucun binaire `wasmtime` n'était installé
sur cette machine).

### 2.2 Reproductibilité bit-à-bit — piège rencontré et corrigé

Un premier build propre dans un dossier *différent* produisait un SHA
différent (`1ea8b18f…` au lieu de `72aa92a6…`) **avec un `Cargo.lock` identique**
(`3f0221319ab5bdbca4b4ae724bd47b893d1ac3c6a72e0c2d427abbdce9a7a26f`).
Cause mesurée : rustc embarque le chemin source dans le binaire
(`strings` contenait `/tmp/opencode/rose-wasmtime-runner` vs
`/tmp/opencode/rose-runner-repro`, et les Build ID GNU différaient
`99d5b3ff…` vs `125051c3…`), malgré `strip = true`.

Correction appliquée dans le script :

```sh
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$BUILD_DIR=/rose-runner"
```

(les rustflags aarch64 locaux `-L $HOME/.local/lib-links` sont ré-ajoutés,
car `RUSTFLAGS` écrase la config cargo).

Preuve : deux builds **propres dans deux dossiers vierges distincts**
(`/tmp/opencode/rose-runner-final1` et `…final2`) ont rendu le **même** SHA :

```
c7b2bf3c2b85e72ec37192f2ba855bd98de0c144288c9cf04aa63f11880739d1  rose-runner-final1/rose-wasmtime-runner-v0.1.5-aarch64-linux
c7b2bf3c2b85e72ec37192f2ba855bd98de0c144288c9cf04aa63f11880739d1  rose-runner-final2/rose-wasmtime-runner-v0.1.5-aarch64-linux
```

Le hash du `Cargo.lock` affiché par le script permet de vérifier que les
versions transitives résolues sont les mêmes. Si un futur build donne un
`Cargo.lock` différent (mises à jour de cranelift, serde…), le SHA binaire
changera aussi : **ne pas réutiliser l'empreinte ci-dessus sans re-mesurer**.

---

## 3. Contrat d'appel (vérifié)

```
runner <module.wasm> <export> [args...]
```

Le runner charge le module avec `Module::from_file`, l'instancie avec
`Instance::new`, convertit chaque argument décimal selon le type du paramètre
(`I32/I64/F32/F64`), appelle l'export et imprime **un entier décimal signé par
ligne** sur stdout. Erreurs sur stderr, code de sortie non nul en cas
d'échec — conforme à `parser_resultats()` et `lancer_runner()` de
`rose/src/wasm_runtime.rs`.

Sortie réelle :

```sh
$ /tmp/opencode/rose-runner-final1/rose-wasmtime-runner-v0.1.5-aarch64-linux \
    /tmp/opencode/rose-wasmtime-runner/fib27.wasm fib 27
196418
```

Module de test : fib récursif i32→i32, 61 octets, export `fib`,
`md5 1ea8a09ca1186a7f9de48d2068f2dcba`,
`sha256 00c490e30433166670217e7999f5bd5dd678e67712ba6850ec94ca12c15119d3`
(identique à celui intégré dans `scripts/mesurer-wasm.sh`).

---

## 4. Mesure réelle de la porte (6 échantillons, médiane)

L'artefact **exact** du SHA `c7b2bf3c…` a été copié sous
`/tmp/opencode/rose-cache/runtimes/wasmtime-v0.1.5` puis le harnais officiel a
été lancé deux fois :

```sh
ROSE_CACHE=/tmp/opencode/rose-cache bash scripts/mesurer-wasm.sh
```

Premier passage — sortie intégrale :

```
PORTE MESURABLE — runtime détecté : /tmp/opencode/rose-cache/runtimes/wasmtime-v0.1.5 (source: cache/PATH-runner)
  mode d'invocation : runner
  module : fib.wasm récursif i32->i32, 61 octets, md5 1ea8a09ca1186a7f9de48d2068f2dcba
  mesure : 6 échantillons médians (seuil ≤ 20 ms)
  échantillons (ms) : 7.775 5.225 3.572 3.383 4.725 3.450 
  médiane : 4.149 ms
  sortie  : 196418 (identique à l'interpréteur) — OK
VERDICT: PASS — porte ≤ 20 ms mesurée et validée.
```

Deuxième passage (contrôle de bruit) :

```
  échantillons (ms) : 3.222 4.909 4.874 3.616 3.230 3.037 
  médiane : 3.423 ms
  sortie  : 196418 (identique à l'interpréteur) — OK
VERDICT: PASS — porte ≤ 20 ms mesurée et validée.
```

**Verdict : porte M6.2 ACCEPTÉE.** Les deux médianes sont ≤ 20 ms et la sortie
est `196418` à chaque échantillon. Le premier échantillon de chaque salve est
le plus lent (7,8 ms puis 3,2 ms) : il inclut le démarrage du processus et la
compilation JIT ; la médiane lisse ce bruit.

Comparaison : 172,1 ms (interpréteur de pile maison, 17/11/2026) → 4,149 ms,
soit un facteur d'environ **41×**.

---

## 5. Étapes suivantes (hors de cette mission — RIEN n'a été fait ici)

1. Vérifier le compte avant toute publication : `gh api user -q .login` doit
   rendre `Stef302` (le PAT `~/.config/opencode/.gh_token` est Stef302, le CLI
   `gh` est clausake). Dépôt `Stef302/Rose`, tag `v0.1.5`.
2. `bash scripts/publier-runtime.sh <artefact>` = **dry-run**, affiche la
   commande sans l'exécuter. Le mode `--exec` exige `OUI`.
3. Upload, puis contrôle d'empreinte :
   ```sh
   curl -fsSL https://github.com/Stef302/Rose/releases/download/v0.1.5/rose-wasmtime-runner-v0.1.5-aarch64-linux \
     | sha256sum   # doit rendre c7b2bf3c…
   ```
4. Transmission de l'empreinte : `ROSE_WASM_RUNTIME_SHA256=c7b2bf3c…` pour un
   miroir/test ; table `sha256_attendu()` de `rose/src/wasm_runtime.rs` pour le
   définitif (toute modification d'un `.rs` est interdite dans cette mission).

---

## 6. Limites

- **Aucune publication, aucun commit** : l'artefact vit sous `/tmp/opencode`,
  le dépôt n'a reçu que `scripts/build-runtime-runner.sh` et ce document.
- Binaire **dynamiquement lié** à glibc/libgcc_s (cible gnu) : autonome vis-à-vis
  de Rust/wasmtime, mais pas statique. Un miroir cible doit fournir un userspace
  aarch64-linux compatible.
- Mesure locale sur la machine de build (aarch64, 20 cœurs, 121 GiB RAM) avec
  `CARGO_BUILD_JOBS=8` ; la médiane inclut le démarrage processus + JIT, pas
  seulement le temps d'appel de la fonction.
- Le téléchargement initial des crates nécessite le réseau (`static.crates.io`) ;
  une fois le cache cargo rempli, le build fonctionne hors ligne.
- La valeur `ROSE_WASM_RUNTIME_SHA256` ci-dessus est liée à la toolchain
  1.99.1 et au lockfile `3f022131…` : re-mesurer et re-hasher après toute mise
  à jour.
