# Runner WebAssembly officiel — contrat, fabrication, empreinte, publication

> **État au 17/11/2026 : artefact runner NON publié, porte ≤ 20 ms NON mesurée.**
> Ce document prépare la publication (M6.2). Il ne publie rien et ne modifie
> aucun `.rs` : la table intégrée de `sha256_attendu()` reste à renseigner par
> l'orchestrateur. Source de vérité : `rose/src/wasm_runtime.rs`.

---

## 1. Contrat exact attendu par `wasm_runtime.rs`

### 1.1 Noms, version, URL (constantes du code)

| Élément | Expression du code | Valeur sur cette machine (aarch64/Linux) |
|---|---|---|
| Version du runner | `VERSION_RUNTIME` | `v0.1.5` |
| Nom dans le cache | `nom_fichier(rt)` = `<runtime>-<version>` (`.exe` sous Windows) | `wasmtime-v0.1.5` |
| Dossier du cache | `dossier_cache()` = `ROSE_CACHE` → `XDG_CACHE_HOME` → `$HOME/.cache`, puis `rose/runtimes/` | `~/.cache/rose/runtimes/` |
| Nom de l'artefact de release | `nom_artefact(rt)` = `rose-<runtime>-runner-<version>-<cible>` | `rose-wasmtime-runner-v0.1.5-aarch64-linux` |
| Cible | `cible()` | `aarch64-linux` |
| URL par défaut | `url_runtime(rt)` | `https://github.com/Stef302/Rose/releases/download/v0.1.5/rose-wasmtime-runner-v0.1.5-aarch64-linux` |
| Dépôt / tag | code en dur | **`Stef302/Rose`**, tag **`v0.1.5`** (pas `0.1.5`, jamais `clausake`) |

Table de `cible()` (à respecter pour les autres artefacts) :
`aarch64-linux`, `x86_64-linux`, `x86_64-windows.exe`, `aarch64-macos`,
`x86_64-macos`, sinon `<arch>-<os>`.

Recherche d'un runner déjà présent (`trouver_runtime()`) : 1) fichier exécutable
`<cache>/wasmtime-v0.1.5` ; 2) binaire `rose-wasmtime-runner` dans le `PATH`.
Le nom court `rose-wasmer-runner` / `wasmer` vaut pour l'autre runtime.

### 1.2 Format d'appel (`lancer_runner()`, ligne 405 du source)

```
runner <module.wasm> <export> [arg1 arg2 ...]
```

Concrètement, pour la porte M6.2 :

```sh
~/.cache/rose/runtimes/wasmtime-v0.1.5 /tmp/fib.wasm fib 27
# doit écrire : 196418
```

* le premier argument est le **chemin du module `.wasm`** ;
* le deuxième est le **nom de l'export** (`fib`) ;
* les suivants sont les arguments décimaux, convertis par `i64::to_string()`
  (`27` → chaîne `27`). Ce sont des entiers signés ; le runner doit les typer
  selon la signature de l'export (le module de mesure est `i32 -> i32`) ;
* le runner est lancé avec `std::process::Command::output()` : **stdout et
  stderr séparés**, pas de shell intermédiaire.

### 1.3 Format de sortie (`parser_resultats()`, ligne 434 du source)

* **un entier signé décimal par ligne** sur stdout, une ligne par valeur de
  retour de l'export (pour `fib`, une seule ligne : `196418`) ;
* les lignes vides et celles commençant par `#` sont ignorées ;
* une ligne préfixée est tolérée : `result: 42` est acceptée (dernier jeton
  numérique) ;
* toute ligne non numérique est une **erreur** (`ErreurRuntime`) ;
* **code de sortie 0** exigé ; sinon `ErreurRuntime` avec le message stderr
  (le message est reporté, jamais masqué par un repli).

Le harnais `scripts/mesurer-wasm.sh` vérifie `196418` à chaque échantillon et
n'imprime `VERDICT: PASS` que si la médiane de 6 échantillons est ≤ 20 ms **et**
si la sortie est cohérente.

### 1.4 Garde-fou SHA-256 (`installer()`, ligne 230 du source)

`installer()` est le **seul** chemin de téléchargement. Il échoue immédiatement
si `sha256_attendu(rt)` est vide — c'est le cas aujourd'hui. Aucune empreinte =
aucun binaire téléchargé exécuté. La vérification utilise `crate::sha256::
sha256_hex` (hexadécimal, comparaison insensible à la casse). Après vérification,
le fichier est installé en `0755` sous `nom_fichier(rt)`. `executer_export()`
ne télécharge jamais : sans runner, il replie sur l'interpréteur maison en
renseignant `Resultat::repli`.

---

## 2. Produire le runner (deux recettes)

⚠ **Aucune dépendance Rust n'est ajoutée à Rose.** Fabriquer le runner dans un
répertoire temporaire **hors du dépôt** (ex. `/tmp/opencode/rose-wasmtime-runner/`).
Ne pas créer de « jardin Rust » dans `Stefan/ROSE`, ne pas toucher à
`rose/Cargo.toml`. GMP reste absente. Seul le **binaire fini** est publié comme
asset de release.

### Recette A — wrapper shell (simple, mais NON autonome)

```sh
#!/bin/sh
# $1 = module, $2 = export, $3.. = arguments
mod="$1"; export_nom="$2"; shift 2
exec wasmtime run --invoke "$export_nom" "$mod" "$@"
```

`wasmtime run --invoke fib fib.wasm 27` imprime `196418` sur une ligne : le
format est compatible sans adaptation. Équivalent wasmer :
`wasmer run "$mod" --invoke "$export_nom" -- "$@"`.

**Limite** : ce wrapper exige que `wasmtime` soit dans le `PATH` de la machine
cible. Ce n'est **pas** un artefact autonome ; il ne convient que pour un miroir
interne où wasmtime est déjà installé. Pour une release publique, préférer B.

### Recette B — petit exécutable Rust autonome (recommandé)

Projet **temporaire hors du dépôt**, avec la crate `wasmtime` (version à adapter
au toolchain local ; exemple ci-dessous) :

```toml
# /tmp/opencode/rose-wasmtime-runner/Cargo.toml
[package]
name = "rose-wasmtime-runner"
version = "0.1.5"
edition = "2021"

[dependencies]
wasmtime = "24"        # version d'exemple, à ajuster/valider localement
```

```rust
// src/main.rs — contrat : runner <module.wasm> <export> [args...]
use wasmtime::{Engine, Linker, Module, Store, Val, ValType};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: runner <module.wasm> <export> [args...]");
        std::process::exit(2);
    }
    let engine = Engine::default();
    let module = Module::from_file(&engine, &args[0])
        .unwrap_or_else(|e| { eprintln!("{e}"); std::process::exit(1); });
    let mut store = Store::new(&engine, ());
    let mut linker = Linker::new(&engine);
    if linker.module(&mut store, "", &module).is_err() {
        eprintln!("échec de l'instanciation du module"); std::process::exit(1);
    }
    let func = linker.get(&mut store, None, &args[1]).and_then(|f| f.into_func())
        .unwrap_or_else(|| { eprintln!("export '{}' introuvable", args[1]); std::process::exit(1); });
    let ty = func.ty(&store);
    let params: Vec<Val> = ty.params().enumerate().map(|(i, t)| {
        let v: i64 = args.get(i + 2).and_then(|s| s.parse().ok())
            .unwrap_or_else(|| { eprintln!("argument {i} manquant/invalide"); std::process::exit(2); });
        match t {
            ValType::I32 => Val::I32(v as i32),
            ValType::I64 => Val::I64(v),
            ValType::F32 => Val::F32((v as f32).to_bits()),
            ValType::F64 => Val::F64((v as f64).to_bits()),
            _ => { eprintln!("type non pris en charge: {t:?}"); std::process::exit(2); }
        }
    }).collect();
    let mut resultats = vec![Val::I32(0); ty.results().len()];
    if func.call(&mut store, &params, &mut resultats).is_err() {
        eprintln!("échec de l'appel de '{}'", args[1]); std::process::exit(1);
    }
    for r in resultats {
        let n: i64 = match r {
            Val::I32(x) => x as i64,
            Val::I64(x) => x,
            Val::F32(b) => f32::from_bits(b) as i64,
            Val::F64(b) => f64::from_bits(b) as i64,
            _ => { eprintln!("résultat non entier"); std::process::exit(2); }
        };
        println!("{n}");   // un entier décimal signé par ligne
    }
}
```

Build : `cd /tmp/opencode/rose-wasmtime-runner && cargo build --release`, puis
copier `target/release/rose-wasmtime-runner` vers le nom d'artefact attendu :

```sh
cp target/release/rose-wasmtime-runner \
   /tmp/opencode/rose-wasmtime-runner/rose-wasmtime-runner-v0.1.5-aarch64-linux
chmod +x /tmp/opencode/rose-wasmtime-runner/rose-wasmtime-runner-v0.1.5-aarch64-linux
```

⚠ Cet exemple n'a **pas** été compilé ici : aucun runtime wasmtime/wasmer n'est
installé sur cette machine et la mission interdit toute publication. Le valider
(cargo build + appel `fib 27` → `196418`) avant de calculer l'empreinte.
Ne pas confondre avec le runner de test factice
`/tmp/opencode/rose-wasm-runtime-test/http/rose-wasmtime-runner-test` (script
`echo 49`, 18 octets) : **il n'est pas publiable**.

---

## 3. Empreinte SHA-256 officielle

1. Calculer sur le fichier **exact** qui sera uploadé (ne plus le reconstruire
   ni éditer ensuite ; tout changement invalide l'empreinte) :

   ```sh
   sha256sum rose-wasmtime-runner-v0.1.5-aarch64-linux
   # 64 caractères hexadécimaux -> <HASH>
   ```

2. Le publier dans `SHA256SUMS` de la release, et le transmettre :
   * **test / miroir** : `ROSE_WASM_RUNTIME_SHA256=<HASH>` (cette variable
     **remplace** la table intégrée, y compris pour une URL miroir avec
     `ROSE_WASM_RUNTIME_URL`) ;
   * **définitif** : l'orchestrateur renseigne la table de `sha256_attendu()`
     dans `rose/src/wasm_runtime.rs`. Tant que cette table est vide,
     `installer()` refuse — garde-fou voulu, pas un bug.

3. Vérifier après publication que l'asset téléchargé a la même empreinte :

   ```sh
   curl -fsSL https://github.com/Stef302/Rose/releases/download/v0.1.5/rose-wasmtime-runner-v0.1.5-aarch64-linux \
     | sha256sum          # doit rendre <HASH>
   ```

---

## 4. Publication (préparation non destructive)

`scripts/publier-runtime.sh` vérifie le compte (`gh api user -q .login` doit
rendre `Stef302`), l'existence et le caractère exécutable de l'artefact, calcule
son `sha256sum`, puis **affiche** la commande d'upload sans l'exécuter :

```sh
bash scripts/publier-runtime.sh                       # dry-run : rien n'est publié
bash scripts/publier-runtime.sh /chemin/artefact      # artefact explicite
bash scripts/publier-runtime.sh /chemin/artefact --exec  # après accord, tapez OUI
```

Le token est lu dans `~/.config/opencode/.gh_token` (jamais écrit dans le
dépôt). Tout compte autre que `Stef302` est refusé avant toute action. Le mode
`--exec` re-vérifie le compte, exige la saisie de `OUI`, puis exécute
`gh release upload v0.1.5 <artefact> --repo Stef302/Rose` (ajout d'un asset ;
pas d'écrasement d'un asset existant).

Contrôle avant/après :

```sh
GH_TOKEN="$(cat ~/.config/opencode/.gh_token)" gh release view v0.1.5 --repo Stef302/Rose
```

Ensuite seulement : installer (`ROSE_WASM_RUNTIME_SHA256=<HASH> rose wasm
--install-runtime wasmtime`), tester le contrat (`... wasmtime-v0.1.5 fib.wasm fib
27` → `196418`), puis mesurer (`bash scripts/mesurer-wasm.sh`). Voir
`rose/docs/PORTE_WASM.md` §4 pour la procédure de mesure complète et
`rose/docs/WASM_RUNTIME.md` pour les règles contractuelles.

**Rappel** : le harnais imprime aujourd'hui `PORTE NON MESURABLE: runtime absent`.
Le seuil ≤ 20 ms n'est pas revendiqué comme atteint ; toute valeur de
performance est un constat daté, pas une consigne.
