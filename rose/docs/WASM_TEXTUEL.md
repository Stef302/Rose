# Exécuter du code Rose **textuel** via le module WASM

Statut : **mesuré le 18/11/2026** sur cette machine (Rose 0.1.5, Node v26.12.1,
Python 3.12.3, `rose/web/rose.wasm` = 736 683 octets). Tous les résultats ci-dessous
ont été réellement exécutés ; rien n'est cité de mémoire.

## Verdict court

**Oui, le texte Rose complet s'exécute dans le wasm.** L'ABI n'est pas limitée à
`add`/`analyse` : elle expose **`rose_execute(ptr, len)`**, qui compile et exécute
un programme Rose entier (variables, fonctions, boucles, rationnels, JIT
Cranelift). Le présupposé « seulement add/analyse » est faux pour ce binaire.

Deux voies hôtes fonctionnent et ont été vérifiées :

| Voie | Résultat `>> 2+3` | Résultat `carre(7)` |
|------|-------------------|---------------------|
| Node 26, ABI C brute (script standalone) | `5` | `49` |
| Python 3.12 + `wasmtime` 50.0.1 (venv /tmp) | `5` | `49` |

Le glue fourni `web/rose-wasm.js` (API `Rose.execute/latex/analyse/version`)
fonctionne aussi : `node web/test-wasm.mjs web/rose.wasm` → **15 contrôles OK**,
dont la parité avec le binaire natif `rose/target/release/rose`.

---

## 1. L'ABI C exposée (23 exports, relevés sur le binaire)

Exports listés par `WebAssembly.instantiate` + `Object.keys(instance.exports)` :

```
memory
rose_version                 rose_alloc            rose_free
rose_execute                 rose_execute_latex    rose_analyse
rose_free_buffer             rose_dernier_panic    rose_raz_panic
rose_prendre_sortie
rose_add_ent                 rose_sub_ent          rose_mul_ent
rose_neg_ent                 rose_clone_ent        rose_affiche_packed
rose_get_ret_tag             rose_set_ret_tag      rose_orc_affiche_f64
rose_orc_affiche_complexe
rose_erreur_div_zero         rose_erreur_div_flottant_zero
rose_erreur_mod_zero
```

### Noyau « texte » (ce qui répond à la mission)

| Export | Signature | Rôle |
|--------|-----------|------|
| `rose_alloc(len)` | `usize → *mut u8` | alloue `len` octets dans la mémoire linéaire ; l'hôte y écrit l'UTF-8 source |
| `rose_free(ptr, len)` | `(*mut u8, usize) → void` | libère le tampon source (`len` d'origine obligatoire) |
| `rose_execute(p, len)` | `(*const u8, usize) → *mut u8` | **exécute le programme Rose**, rend sa sortie stdout |
| `rose_execute_latex(p, len)` | idem | rend le LaTeX du programme entier |
| `rose_analyse(p, len)` | idem | rend le résumé d'analyse (sans exécuter) |
| `rose_version()` | `→ *mut u8` | rend la version du moteur |
| `rose_free_buffer(p)` | `(*mut u8) → void` | libère un tampon de sortie (la longueur est dans l'en-tête) |
| `rose_prendre_sortie()` | `→ *mut u8` | sortie partielle capturée, lisible après un abort |
| `rose_dernier_panic()` | `→ *mut u8` | message de la dernière panique (consommé à la lecture) |
| `rose_raz_panic()` | `→ void` | vide le tampon de panique |

**Convention des tampons de sortie** (voir `src/wasm.rs`, `encoder()`) :
4 octets = `u32` *little endian* = longueur N, puis N octets UTF-8. L'hôte lit
l'en-tête, décode, puis appelle `rose_free_buffer(ptr)`.

**Piège `panic = "abort"`** : `wasm32-unknown-unknown` impose l'abort, donc
`catch_unwind` ne peut rien. Une erreur Rose (« ROSE_ERR:… ») fait un `unreachable`
wasm ; le hook range le message avant l'abort et **l'hôte doit le relire** avec
`rose_dernier_panic()` + `rose_prendre_sortie()` (exactement ce que fait le glue
`appeler()` dans `web/rose-wasm.js`). Le script Node ci-dessous le fait aussi.

### Helpers JIT (ne pas confondre avec l'API texte)

`rose_add_ent(a:i64, at:i64, b:i64, bt:i64, out:*mut i64, outt:*mut i64)`,
`rose_sub_ent`, `rose_mul_ent`, `rose_neg_ent`, `rose_clone_ent` sont les
primitives arithmétiques **appelées par le code Cranelift compilé**, pas une API
d'évaluation d'expressions. Un entier « taggé » est `(valeur:i64, tag:i64)` :
`tag == 0` → la valeur est un i64 ; `tag != 0` → pointeur vers un `Mpz` boxé
(voir `src/run.rs:1385`). Appelés directement avec des petits entiers (`tag=0`)
depuis l'hôte, ils répondent : mesuré `rose_add_ent(1,1) = 2 tag=0`,
`rose_mul_ent(6,7) = 42 tag=0` — mais ce n'est **pas** du code Rose textuel.
`rose_erreur_*`, `rose_orc_*` et `rose_affiche_packed` sont de même nature.

---

## 2. Voie Node — script standalone (ABI brute, sans glue)

Script : `/tmp/opencode/rose-wasm-textuel.mjs`. Il prend la source Rose en 1er
argument (le chemin du wasm en 2e, optionnel), écrit via `rose_alloc`, appelle
`rose_execute`, décode l'en-tête et gère l'abort.

```bash
node /tmp/opencode/rose-wasm-textuel.mjs '>> 2+3'
node /tmp/opencode/rose-wasm-textuel.mjs 'fn carre(n: entier)->entier { retour n*n } >> carre(7)'
```

Sorties **réellement obtenues** :

```
$ node /tmp/opencode/rose-wasm-textuel.mjs '>> 2+3'
5

$ node /tmp/opencode/rose-wasm-textuel.mjs 'fn carre(n: entier)->entier { retour n*n } >> carre(7)'
49

$ node /tmp/opencode/rose-wasm-textuel.mjs '>> 2 ^ 100'
1267650600228229401496703205376

$ node /tmp/opencode/rose-wasm-textuel.mjs '$z = 0
>> 5 / $z'
Rose : erreur — division par zéro
```

### Correction importante sur l'énoncé du test

La forme littérale demandée `fn carre(n)->entier { retour n*n } >> carre(7)`
**est invalide en Rose** : le paramètre exige une annotation de type. Mesuré,
le wasm **et** le natif renvoient la même erreur :

```
Rose : erreur — [Ligne 1, Colonne 0] La fonction retourne ?, attendu entier
```

La forme correcte est `fn carre(n: entier) -> entier { retour n * n }`
(identique à celle du test de référence `web/test-wasm.mjs`), qui donne `49`
en wasm **et** en natif. Le test demandé est donc réussi **après correction de
synthaxe**, pas avec la chaîne initiale — ceci est dit explicitement pour ne pas
prétendre un test non exécuté.

### Le glue fourni (alternative plus simple)

```js
import fs from 'node:fs';
import Rose from './web/rose-wasm.js';
Rose.configure({ bytes: fs.readFileSync('web/rose.wasm') });
await Rose.ready();
console.log(await Rose.execute('>> 2+3'));           // 5
console.log(await Rose.version());                   // Rose 0.1.5 — …
```

Vérifications supplémentaires faites par appel ABI direct (sorties réelles) :

```
rose_version()            = "Rose 0.1.5 — compilateur Rust + JIT Cranelift (sémantique exacte)"
rose_execute('>> 1/2 + 1/3') = "0.8[3]\n"
rose_execute_latex('>> 1+1') = "\\mathrm{afficher}\\left(2\\right)\n"
rose_analyse('fn f(x: entier)->entier{ retour x } >> f(1)')
   = "programme valide — 1 instruction(s), 1 fonction(s) : f(1)\nvérification : OK"
```

---

## 3. Voie Python — état réel et contournement

### Constat initial (avant installation)

```
$ python3 -c 'import wasmtime; print(wasmtime.__version__)'
ModuleNotFoundError: No module named 'wasmtime'

$ which rustpython
(absent)

$ which cargo
(absent)
```

**CPython seul n'exécute pas de wasm** : il lui faut un runtime hôte. Aucun
runtime n'était présent dans l'environnement de base, et le portage RustPython
n'est pas installé (ni binaire, ni chaîne `cargo` pour le compiler ici).

### Test réel : runtime installé dans un venv isolé /tmp

Pas d'installation globale : `python3 -m venv /tmp/opencode/venv-wasmtime` puis
`/tmp/opencode/venv-wasmtime/bin/pip install wasmtime`. Réseau disponible
(`https://pypi.org/simple/wasmtime/` → HTTP 200), moisson réussie :

```
Successfully installed wasmtime-50.0.1
```

Script : `/tmp/opencode/rose-wasm-textuel.py` (même logique que le script Node :
`rose_alloc` → `memory.write` → `rose_execute` → en-tête `u32 LE` →
`rose_free_buffer`, avec relecture de `rose_dernier_panic()` sur `Trap`).

Sorties **réellement obtenues** :

```
$ venv/bin/python /tmp/opencode/rose-wasm-textuel.py '>> 2+3'
5

$ venv/bin/python /tmp/opencode/rose-wasm-textuel.py 'fn carre(n: entier)->entier { retour n*n } >> carre(7)'
49

$ venv/bin/python /tmp/opencode/rose-wasm-textuel.py '>> 2 ^ 100'
1267650600228229401496703205376

$ venv/bin/python /tmp/opencode/rose-wasm-textuel.py '$z = 0
>> 5 / $z'
Rose : erreur — division par zéro
```

**Piège API wasmtime-python** (coûté un essai, documenté ici) :
`Memory.read(store, start, stop)` prend une **plage** `[start, stop)`, pas
`(offset, longueur)`. Avec `mem.read(store, ptr, 4)` on obtient une `bytearray`
vide et `struct.unpack` échoue. Il faut `mem.read(store, p, p+4)` puis
`mem.read(store, p+4, p+4+n)`. `Memory.write(store, value, start)`, lui, prend
bien un offset.

### Portage RustPython

Non testé plus avant : `rustpython` absent et `cargo` absent sur cette machine.
RustPython ne serait de toute façon **pas nécessaire** : le module wasm de Rose
est un module WebAssembly autonome (aucun import), n'importe quel runtime WASM
hôte suffit — Node et wasmtime-python suffisent. Le « portage Rust » pertinent
ici n'est pas RustPython mais le fait que **Rose lui-même est écrit en Rust** et
que son moteur est déjà compilé en wasm ; il n'y a rien à réécrire.

---

## 4. Limites et ce qui manque

1. **Erreur = abort du module.** Après une division par zéro, l'instance wasm
   est arrêtée (`panic="abort"`). Le message est récupérable une fois, mais il
   faut réinstancier le module pour exécuter un nouveau programme. Le glue JS
   masque cela en relisant les tampons ; le script Python le fait aussi.
2. **Aucun REPL incrémental.** `rose_execute` repart de zéro à chaque appel :
   pas d'état persistant entre deux appels (pas de `$x` conservé).
3. **Les helpers `rose_*_ent` ne sont pas une API expression** : ce sont des
   primitives internes au JIT, avec convention de tags ; les utiliser comme
   calculatrice contournerait le compilateur Rose.
4. **Pas d'API wasm pour « lire un fichier .rose »** : c'est à l'hôte de lire le
   fichier et de passer les octets UTF-8 à `rose_alloc`.
5. Python nécessite un runtime tiers (`wasmtime`) ; **non présent** dans
   l'environnement de base. Le venv `/tmp` utilisé pour ce test n'est pas une
   installation système et peut disparaître.

## 5. Recommandation

- **Pour exécuter du texte Rose : utiliser directement `rose_execute`** via le
  glue `web/rose-wasm.js` (navigateur/Node) — API propre, gestion d'erreurs déjà
  écrite. Le script `/tmp/opencode/rose-wasm-textuel.mjs` montre l'appel ABI
  minimal si l'on veut s'affranchir du glue.
- **Depuis Python : installer `wasmtime` dans un venv dédié** et reprendre
  `/tmp/opencode/rose-wasm-textuel.py` (en respectant `read(start, stop)`). Ne
  pas attendre de CPython nu qu'il exécute du wasm.
- **Ne pas chercher RustPython** pour ce besoin : le moteur Rose est déjà un
  wasm autonome, Node ou wasmtime suffisent.
- Écrire les fonctions avec des **annotations de type** (`n: entier`), sinon
  le compilateur refuse (`La fonction retourne ?, attendu entier`).
