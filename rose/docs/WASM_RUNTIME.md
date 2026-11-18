# Runtime WebAssembly officiel optionnel (M6.2)

## Principe — le choix, sans dépendance imposée

M6.1 a livré le pont WebAssembly maison (`rose/src/wasm_lecteur.rs`) : un
interpréteur de pile, **zéro dépendance**, qui reste **le défaut**. M6.2 ajoute
simplement la possibilité d'utiliser un runtime officiel (wasmtime / wasmer)
pour les modules chauds.

| Propriété | Interpréteur maison | Runtime officiel |
|---|---|---|
| Défaut | ✅ | ❌ (optionnel) |
| Dépendance dans `rose.exe` | aucune | **aucune** (exécutable séparé) |
| Emplacement | code du moteur | `~/.cache/rose/runtimes/` |
| Téléchargement | jamais | **explicite seulement** |
| Perf `fib(27)` | ~150 ms (référence historique) | ≤ 20 ms — **mesuré 2,885 ms** (18/11/2026, CLI réelle) |
| Sortie | référence | **identique** (normalisée en i64) |

Règles contractuelles :

1. l'interpréteur maison est toujours le défaut ; rien n'est embarqué dans le
   binaire et `Cargo.toml` n'est pas modifié ;
2. le runtime est un exécutable distinct, rangé dans le cache sous un nom
   **déterministe** `<runtime>-<version>` (ex. `wasmtime-v0.1.5`) ;
3. le téléchargement est **explicite** (`rose wasm --install-runtime …`) et
   **vérifié par SHA-256**. Aucune empreinte = refus d'installer (on n'exécute
   jamais un binaire téléchargé non authentifié) ;
4. `executer_export()` **ne télécharge jamais** : si le runtime demandé est
   absent, il replie sur l'interpréteur maison et le dit explicitement dans
   `Resultat::repli` (jamais de téléchargement silencieux pendant une requête) ;
5. une erreur du runner est **reportée**, jamais masquée par un repli silencieux.

## Cache

```
~/.cache/rose/
├── rose-aot-v5.10.0          (helper AOT, M5 — autre module)
└── runtimes/
    ├── wasmtime-v0.1.5
    └── wasmer-v0.1.5
```

Le dossier est choisi dans cet ordre : `ROSE_CACHE` → `XDG_CACHE_HOME` →
`$HOME/.cache`, puis `rose/runtimes/`. Le fichier est créé avec les droits
`0755` (Unix) après vérification de l'empreinte. `trouver_runtime()` cherche
d'abord dans le cache, puis un binaire `rose-<runtime>-runner` dans le `PATH`.

## Contrat du runner officiel

Le runner est un petit exécutable fourni par une release `Stef302/Rose` :

```
runner <module.wasm> <export> [arg1 arg2 ...]
```

* il écrit **un entier signé décimal par ligne** sur stdout (une ligne par
  valeur de retour de l'export) ;
* les lignes vides et les commentaires `#` sont ignorés ; une ligne
  `result: 42` est tolérée (dernier jeton numérique) ;
* tout ce qui n'est pas numérique est une erreur ;
* sur erreur, code de sortie non nul + message sur stderr → `ErreurRuntime`.

Exemple de wrapper `wasmtime` (à publier comme artefact de release) :

```sh
#!/bin/sh
# $1 = module, $2 = export, $3.. = arguments
mod="$1"; export_nom="$2"; shift 2
exec wasmtime run --invoke "$export_nom" "$mod" "$@"
```

`wasmtime run --invoke fib fib.wasm 27` affiche bien `196418` sur une ligne :
le format est donc compatible sans adaptation. L'équivalent wasmer est
`wasmer run "$mod" --invoke "$export_nom" -- "$@"`.

## Commandes (câblage CLI attendu)

```sh
# inspection (par défaut, interpréteur maison — inchangé)
rose wasm lib.wasm

# exécution avec le runtime officiel ; repli maison si absent du cache
rose wasm --runtime officiel lib.wasm --export fib --args 27

# installation explicite du runtime (téléchargement + SHA-256)
rose wasm --install-runtime wasmtime
rose wasm --install-runtime wasmer
```

`--runtime officiel` sélectionne wasmtime ; `wasmtime` / `wasmer` sont aussi
acceptés. `--args` est une liste d'entiers décimaux.

## Installation locale (SANS réseau) vs publication — 18/11/2026

L'artefact n'étant **pas encore publié** sur `Stef302/Rose`,
`rose wasm --install-runtime wasmtime` échouerait au téléchargement (URL de
release introuvable). Pour installer le runner **déjà construit et vérifié**
sans aucun accès réseau, copier le fichier dans le cache sous le nom
déterministe attendu par `dossier_cache()` / `nom_fichier()` :

```sh
SHA=c7b2bf3c2b85e72ec37192f2ba855bd98de0c144288c9cf04aa63f11880739d1
SRC=/tmp/opencode/rose-runner-final1/rose-wasmtime-runner-v0.1.5-aarch64-linux
DST="$HOME/.cache/rose/runtimes/wasmtime-v0.1.5"   # ROSE_CACHE / XDG_CACHE_HOME non définis ici
mkdir -p "$(dirname "$DST")"
[ "$(sha256sum "$SRC" | cut -d' ' -f1)" = "$SHA" ] || { echo 'SHA inattendu'; exit 1; }
cp "$SRC" "$DST" && chmod 0755 "$DST"
sha256sum "$DST"          # → c7b2bf3c…80739d1  (6 493 120 octets)
```

C'est exactement ce que fait `installer()` **après** son téléchargement :
même chemin, même nom de fichier, droits `0755`, même contrôle SHA-256.
`trouver_runtime()` (et donc `est_installe()`) le trouve ensuite sans
distinction, qu'il vienne d'une copie locale ou d'un téléchargement.

Quand l'artefact sera publié, la procédure normale reprendra : publication
côté mainteneur `Stef302` (release `v0.1.5`, fichier
`rose-wasmtime-runner-v0.1.5-aarch64-linux`), puis, côté utilisateur,
`rose wasm --install-runtime wasmtime`. L'empreinte intégrée
`SHA_WASMTIME_V0_1_5_AARCH64_LINUX` (**source de vérité**, surchargeable par
`ROSE_WASM_RUNTIME_SHA256`) doit alors être celle du fichier **réellement
publié** : la re-mesurer avant toute publication. **Rien n'a été publié sur
GitHub lors de cette étape.**

## Variables d'environnement

| Variable | Rôle |
|---|---|
| `ROSE_CACHE` | Remplace le dossier de cache (y est ajouté `runtimes/`) |
| `XDG_CACHE_HOME` / `HOME` | Base du cache si `ROSE_CACHE` absent |
| `ROSE_WASM_RUNTIME_URL` | URL de l'artefact (miroir interne, tests) |
| `ROSE_WASM_RUNTIME_SHA256` | Empreinte attendue (miroir, tests, déploiement privé) |

URL par défaut :
`https://github.com/Stef302/Rose/releases/download/<tag>/rose-<runtime>-runner-<tag>-<cible>`
(dépôt **Stef302/Rose**, jamais `clausake`).

## API publique (`rose/src/wasm_runtime.rs`)

```rust
pub const VERSION_RUNTIME: &str;
pub const SOUS_DOSSIER_CACHE: &str;

pub enum RuntimeOfficiel { Wasmtime, Wasmer }
impl RuntimeOfficiel {
    pub fn nom(self) -> &'static str;
    pub fn depuis_str(s: &str) -> Option<Self>;
}

pub enum Voie { Maison, Officiel }

pub struct Resultat {
    pub voie: Voie,
    pub valeurs: Vec<i64>,
    pub repli: Option<String>,
}

pub struct ErreurRuntime(pub String);

pub fn dossier_cache() -> PathBuf;
pub fn nom_fichier(rt: RuntimeOfficiel) -> String;
pub fn nom_artefact(rt: RuntimeOfficiel) -> String;
pub fn cible() -> String;
pub fn chemin_runtime(rt: RuntimeOfficiel) -> PathBuf;
pub fn url_runtime(rt: RuntimeOfficiel) -> String;
pub fn sha256_attendu(rt: RuntimeOfficiel) -> String;
pub fn verifier_sha256(octets: &[u8], attendu: &str) -> Result<(), ErreurRuntime>;
pub fn est_installe(rt: RuntimeOfficiel) -> bool;
pub fn trouver_runtime(rt: RuntimeOfficiel) -> Option<PathBuf>;

#[cfg(not(target_arch = "wasm32"))]
pub fn installer(rt: RuntimeOfficiel) -> Result<PathBuf, ErreurRuntime>;

pub fn executer_maison(chemin_wasm: &Path, export: &str, arguments: &[i64])
    -> Result<Vec<i64>, ErreurRuntime>;
pub fn executer_export(chemin_wasm: &Path, export: &str, arguments: &[i64],
    runtime: Option<RuntimeOfficiel>) -> Result<Resultat, ErreurRuntime>;
pub fn parser_resultats(sortie: &str) -> Result<Vec<i64>, ErreurRuntime>;
```

`verifier_sha256` réutilise `crate::sha256::sha256_hex` (aucune duplication).
Les arguments sont typés selon la signature de l'export : `ValWasm::as_i32()`
ne convertit pas un `I64`, donc `executer_maison` construit les `ValWasm` dans
le variant déclaré par le module (sinon `fib(27)` rendait 0 — mesuré).

## Porte de performance (à mesurer sur la machine cible)

```
fib(27) = 196418
  runtime officiel : ≤ 20 ms
  interprété       : ~150 ms (référence)
  sortie           : IDENTIQUE au caractère près
```

Protocole : même module `fib.wasm`, mêmes arguments, ≥6 échantillons,
médianes ; accepter le changement seulement s'il bat les contrôles voisins.
Ne jamais citer une spec : mesurer.

### Constat daté du 18/11/2026 — CLI Rose réelle, voie officielle

Binaire `release` reconstruit depuis `rose/src/wasm_runtime.rs` (table SHA
renseignée), runner installé localement dans le cache (voir section précédente).
Commande exacte, depuis la racine du dépôt :

```sh
./rose/rose wasm /tmp/opencode/fib27.wasm --export fib --args 27 --runtime officiel
```

`/tmp/opencode/fib27.wasm` = module du base64
`AGFzbQEAAAABBgFgAX8BfwMCAQAHBwEDZmliAAAKHgEcACAAQQJIBH8gAAUgAEEBaxAAIABBAmsQAGoLCw==`
(61 octets, export `fib`). Chronométrage du **processus CLI complet**, 6
échantillons consécutifs :

```
3.069 / 2.286 / 3.748 / 2.700 / 3.881 / 2.058 ms
```

**Médiane 2.885 ms** (min 2.058, max 3.881) — porte ≤ 20 ms **validée**,
avec ~7× de marge. Chaque échantillon : `stdout = 196418`, `stderr = ''`,
code de sortie 0 : **aucun message de repli**. Le câblage CLI confirme
qu'`--runtime officiel` → `Some(Wasmtime)` → `trouver_runtime()` → cache, et
que `executer_export()` ne rapporte `Voie::Maison` qu'en émettant le message
`rose wasm : … repli …` sur stderr. Son absence prouve la voie officielle.

**Contrôle négatif** : runner temporairement retiré du cache, la même commande
rend `stderr = "rose wasm : appel de 'fib' : récursion wasm trop profonde (>256)"`,
code 1 — le repli maison échoue sur ce module avec ce binaire. Le `196418`
obtenu avec le runner en place ne peut donc **pas** venir du repli. Le runner
a été restauré ensuite (SHA inchangé, droits `0755`).

### Constat historique du 17/11/2026 (crate de validation isolée)

`fib(27) = 196418` via l'interpréteur maison, 6 échantillons
`171.4 / 171.8 / 172.0 / 172.2 / 172.4 / 172.5 ms`, **médiane 172.1 ms**.
La voie officielle n'avait PAS pu être mesurée alors : aucun binaire
wasmtime/wasmer n'était installé et l'artefact du runner n'était pas publié.
Ce chiffre est conservé à titre historique ; il n'est pas la mesure CLI du
binaire livré, et le repli maison de ce binaire échoue sur `fib(27)` (voir
contrôle négatif ci-dessus).

## Limites

* **Aucun runtime n'est embarqué** et aucun n'est téléchargé par défaut :
  `installer()` est le seul chemin de téléchargement, et il exige une empreinte
  SHA-256. La table intégrée contient désormais l'empreinte du runner
  `v0.1.5-aarch64-linux` vérifié localement ; les autres cibles restent vides
  et refusent l'installation jusqu'à mesure de leur artefact. L'artefact
  n'étant pas encore publié, le téléchargement échouera côté réseau : utiliser
  la procédure de copie locale ci-dessus (définir `ROSE_WASM_RUNTIME_SHA256`
  reste possible pour un miroir de test). Ce n'est pas un bug : c'est la
  garde-fou.
* Téléchargement via `curl` (puis `wget`) en ligne de commande : aucune
  bibliothèque TLS n'est ajoutée, aucun proxy n'est géré en propre (curl/wget
  s'en chargent). Nécessite ces outils et un accès réseau au moment de
  l'installation **uniquement**.
* Les cibles sans processus externe (compilation `wasm32` de la bibliothèque)
  n'ont pas de runtime officiel : `executer_export()` y force la voie maison.
* La normalisation couvre les valeurs entières (`i64`). Les exports `f32`/`f64`
  sont convertis en entiers tronqués : pour un module flottant, étendre
  `Resultat::valeurs` avant d'activer la voie officielle.
* Le runner doit être de confiance : la vérification SHA-256 authentifie le
  fichier téléchargé, pas la logique du runner. Utiliser exclusivement les
  artefacts publiés par `Stef302/Rose` ou un miroir internalisé dont on
  maîtrise l'empreinte.
