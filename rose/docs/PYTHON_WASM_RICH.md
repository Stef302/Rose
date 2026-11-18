# Chaîne Python → wasm → Rose : test avec une vraie bibliothèque Python (Rich)

**Date** : 18/11/2026 · **Dépôt** : `Stef302/Rose` 0.1.5, branche `main`, commit `dea92be`.
**Périmètre** : une seule étape — tester la chaîne de conversion wasm avec une bibliothèque
Python réelle (Rich 15.0.0), exactement comme demandé : *« wasm et opérationnel avec la
commande convertisseur, donc teste avec une bibliothèque python comme rich »*.

⚠️ **Avertissement à lire en premier** : ce document distingue strictement deux choses :

1. **compiler Rich depuis Python vers un `.wasm` autonome** → **IMPOSSIBLE** avec les outils
   disponibles (démontré ci-dessous, preuves à l'appui) ;
2. **reprendre un algorithme de Rich, le réécrire en C, le convertir avec
   `scripts/rose-wasm-lib.sh` et l'appeler depuis Rose** → **RÉUSSI**, avec sorties
   byte-for-byte identiques à Rich.

Ce n'est **pas** « Rich compilé depuis Python ». C'est un **port C manuel** d'un sous-ensemble
de Rich. Toute la différence est détaillée au §2 et au verdict.

---

## 1. Tentative de compilation Python → C → wasm : ÉCHEC documenté

### 1.1 Cython

```sh
cd /tmp/opencode
python3 -m venv venv-cython
venv-cython/bin/pip install --quiet cython
venv-cython/bin/cython --version
# Cython version 3.3.1
```

Source Python utilisant **vraiment** Rich (`/tmp/opencode/pywasm/rich_bridge.py`) :

```python
from rich.cells import cell_len
from rich.panel import Panel
from rich.console import Console

def rich_cell_len(text: str) -> int:
    return cell_len(text)

def rich_panel_width(text: str) -> int:
    panel = Panel.fit(text)
    return panel.__rich_measure__(Console(), Console().options).maximum
```

```sh
cd /tmp/opencode/pywasm
/tmp/opencode/venv-cython/bin/cython -3 rich_bridge.py -o rich_bridge.c
wc -c rich_bridge.c
# 343608 rich_bridge.c
grep -c "Python.h\|PyObject\|PyUnicode\|PyLong\|PyImport\|__Pyx_" rich_bridge.c
# 2466
```

Le C généré est un **module d'extension CPython**, pas une bibliothèque autonome :

```sh
grep -n "PyInit_rich_bridge\|PyModule_Create\|PyImport_ImportModule" rich_bridge.c
# 1112:    inspect = PyImport_ImportModule("inspect");
# 3218:__Pyx_PyMODINIT_FUNC PyInit_rich_bridge(void)
# 3366:  __pyx_t_1 = PyModule_Create(&__pyx_moduledef);
```

Passage au convertisseur Rose :

```sh
bash /home/clausake/Stefan/ROSE/scripts/rose-wasm-lib.sh \
  --export 'rich_cell_len=rich_cell_len(chaine)->entier' \
  --out librich_cython rich_bridge.c
```

Sortie réelle :

```
==> compilation: /home/clausake/.local/wasi-sdk/bin/clang ... -O0 ... -> ./librich_cython.wasm
rich_bridge.c:19:10: fatal error: 'Python.h' file not found
   19 | #include "Python.h"
      |          ^~~~~~~~~~
1 error generated.
```

**Échec 1.** Même en ajoutant les en-têtes Python hôte, le module reste un `PyInit_*`
appelant `PyModule_Create` et `PyObject_Call` : il exigerait un CPython complet **dans le
wasm**, donc des **imports wasm** — incompatible avec le pont M6.1 (0 import exigé).

### 1.2 mypyc (même limite)

```sh
venv-cython/bin/pip install --quiet mypy setuptools
venv-cython/bin/mypyc --version      # mypy 2.4.1 (compiled: yes)
cd /tmp/opencode/pywasm
/tmp/opencode/venv-cython/bin/mypyc mini.py     # def add(a:int,b:int)->int: return a+b
```

Le build natif réussit (`build/__native.c`, 7 587 octets), mais :

```sh
grep -c "Python.h\|PyObject\|PyLong\|PyImport" build/__native.c   # 35
grep -c "PyInit_" build/__native.c                               # 1
bash /home/clausake/Stefan/ROSE/scripts/rose-wasm-lib.sh \
  --export 'add=mini_add(entier,entier)->entier' --out libmini_mypyc build/__native.c
# build/__native.c:1:10: fatal error: 'init.c' file not found
```

**Échec 2.** Même mur : mypyc génère une extension CPython (`init.c`, `PyInit_…`).

Autres voies vérifiées absentes : `rustpython` (command not found), `nuitka`
(command not found). Le compilateur présent est `clang 18.1.2-wasi-sdk`, qui sait compiler
du **C autonome**, pas du Python.

---

## 2. Ce qui a réellement été converti et appelé depuis Rose

Puisqu'on ne peut pas compiler Rich, j'ai **porté manuellement en C** trois fonctions lues
dans la source installée de Rich 15.0.0, en citant les fichiers d'origine dans le code :

| Fonction Rose | Algorithme Rich reproduit | Source Rich |
|---|---|---|
| `rich_cell_len` | `get_character_cell_size()` / `_cell_len()` | `rich/cells.py` |
| `rich_panel_width` | `Panel.__rich_measure__().maximum` | `rich/panel.py` |
| `rich_panel_fit` | rendu `Panel.fit` avec `box.ROUNDED` | `rich/panel.py`, `rich/box.py` |

Fichier : `/tmp/opencode/pywasm/rich_algo.c` (md5 `804e7d6004592d50d6bfae287e6bdbbf`).

**Limites explicites du port** (simplification fidèle, pas copie intégrale) :
- `cell_len` reproduit **exactement** les règles ASCII/contrôle de Rich et le défaut
  « 1 cellule » ; la table Unicode complète (CJK/emoji, `rich/_unicode_data.py`) n'est pas
  embarquée. Les chaînes de test sont ASCII.
- `Panel.fit` est reproduit pour du texte court, sans wrap, sans style, sans titre.
- Les octets UTF-8 des bordures sont émis en **constantes numériques** et non en littéraux
  de chaîne (voir §5, piège du parseur de section DATA).

### 2.1 Conversion avec la commande du projet

```sh
cd /tmp/opencode/pywasm
bash /home/clausake/Stefan/ROSE/scripts/rose-wasm-lib.sh \
  --export 'rich_cell_len=rich_cell_len_c(chaine)->entier' \
  --export 'rich_panel_width=rich_panel_width_c(chaine)->entier' \
  --export 'rich_panel_fit=rich_panel_fit_c(chaine)->chaine' \
  --out librich rich_algo.c
```

Sortie réelle :

```
==> compilation: /home/clausake/.local/wasi-sdk/bin/clang ... -O0 ... -> ./librich.wasm
WASM_CHECK imports=0 start=0 call_indirect=0 memory_exported=yes
exports=rich_cell_len,rich_panel_width,rich_panel_fit
==> OK: ./librich.wasm  (4755 octets)
==> OK: ./librich.rose
==> Verifie: 0 import, 0 start, 0 call_indirect, memoire exportee. -O0 force.
```

`librich.wasm` : md5 `a69717c210e9c89e7849d7b591b8cf14`, 4 755 octets,
**0 import / 0 start / 0 call_indirect / mémoire exportée**.

### 2.2 Chargement dans Rose

```sh
cd /home/clausake/Stefan/ROSE
./rose/rose wasm /tmp/opencode/pywasm/librich.wasm
```

Sortie réelle :

```
@dll("wasm", "/tmp/opencode/pywasm/librich.wasm")
@sym("rich_cell_len")
fn rich_cell_len(a0: entier) -> entier

@dll("wasm", "/tmp/opencode/pywasm/librich.wasm")
@sym("rich_panel_width")
fn rich_panel_width(a0: entier) -> entier

@dll("wasm", "/tmp/opencode/pywasm/librich.wasm")
@sym("rich_panel_fit")
fn rich_panel_fit(a0: entier) -> entier

3 fonction(s) exportée(s)
```

(`rose wasm` affiche les types wasm bruts : `i32` → `entier`. Les déclarations Rose
générées dans `librich.rose` portent bien `chaine` et le retour `-> chaine`.)

### 2.3 Appel depuis Rose

`/tmp/opencode/pywasm/demo_rich.rose` (extrait) :

```rose
importer "/tmp/opencode/pywasm/librich.rose"

afficher(rich_cell_len(""))
afficher(rich_cell_len("abc"))
afficher(rich_cell_len("Hello, world!"))
afficher(rich_panel_width(""))
afficher(rich_panel_width("abc"))
afficher(rich_panel_width("Hello"))
afficher(rich_panel_width("Hello, world!"))
afficher(rich_panel_fit("Hello"))
afficher(rich_panel_fit("Hello, world!"))
```

```sh
cd /home/clausake/Stefan/ROSE
timeout 60 ./rose/rose /tmp/opencode/pywasm/demo_rich.rose
```

Sortie réelle :

```
=== rich.cell_len (port C, appelé via wasm depuis Rose) ===
0
3
13
=== rich Panel.__rich_measure__().maximum (port C) ===
4
7
9
17
=== rich Panel.fit rendu (box.ROUNDED), retour chaine depuis wasm ===
╭───────╮
│ Hello │
╰───────╯

╭───────────────╮
│ Hello, world! │
╰───────────────╯

╭─────╮
│ abc │
╰─────╯
```

Le **retour `-> chaine` depuis le wasm fonctionne** : Rose relit la chaîne C et affiche un
vrai panneau à bordures arrondies.

### 2.4 Vérification différentielle contre le vrai Rich

14 chaînes ASCII, `cell_len` et `panel_width` calculés d'un côté par Rose+WASM, de l'autre
par Rich 15.0.0 en Python :

```
expected lines: 28 got lines: 28
DIFFERENTIAL cell_len+panel_width: PASS (identique a Rich)
```

4 panneaux rendus comparés octet pour octet à `Console(record=True).export_text()` :

```
EXACT MATCH: True
```

---

## 3. Ce que le convertisseur peut et ne peut pas faire pour du Python

**Peut :**
- convertir du **C autonome** (donc du code généré à la main, ou par un transpileur
  produisant du C sans runtime) en `.wasm` M6.1 vérifié :
  `0 import / 0 start / 0 call_indirect / mémoire exportée`, `-O0` forcé ;
- exposer des fonctions avec types Rose `entier`, `booleen`, `flottant`, `rationnel`,
  `chaine`, en paramètre **et en retour** (retour `chaine` confirmé ici) ;
- générer le `.rose` de déclarations `@dll`/`@sym`/`fn` prêt à `importer`.

**Ne peut pas :**
- avaler un `.py` : Cython et mypyc produisent des **extensions CPython**
  (`Python.h`, `PyInit_*`, `PyObject_Call`) → échec de compilation WASI, et même avec les
  en-têtes hôtes, le résultat exigerait un CPython embarqué (imports wasm) ;
- embarquer un runtime Python dans le `.wasm` — et c'est **volontaire** : le pont M6.1
  refuse les imports par conception (c'est ce qui garantit « sans runtime de langage ») ;
- porter seul la sémantique d'une bibliothèque Python : les dépendances internes de Rich
  (textes stylés, measurement Unicode, box, consoles) devraient toutes être réécrites.

Le script `scripts/rose-wasm-lib.sh` n'a **pas été modifié** : ajouter un chemin `.py`
qui « marche » reviendrait à intégrer un compilateur Python→C autonome inexistant ici.
Un message d'erreur explicite pour `.py` pourrait être ajouté plus tard, mais ce test ne
nécessitait pas de toucher à un fichier partagé avec d'autres agents.

---

## 4. Recommandation

1. **Ne pas chercher à faire tourner Rich entier dans Rose.** Le coût (CPython complet
   compilé en wasm, imports, poids, démarrage) annule l'intérêt du pont M6.1.
2. **Garder Rich hors du produit** (outillage de développement / génération de chaînes
   au build si besoin), et écrire le sous-ensemble réellement nécessaire **en C pur ou en
   Rust `wasm32-unknown-unknown`** pour le pont — comme démontré ici.
3. **Si un algorithme Python doit être porté**, en extraire la logique algorithmique, la
   réécrire en C sans libc, et la passer à `rose-wasm-lib.sh` : c'est la seule voie qui
   produit un module autonome vérifié.
4. **Documenter la provenance** (fichier source Rich + ligne) dans le C généré, comme dans
   `rich_algo.c`, pour que le port reste auditable.
5. **Corriger les deux limitations du pont découvertes ici** (§5) si des modules avec
   données statiques ou des chaînes de contrôle doivent être supportés.

---

## 5. Limitations du pont M6.1 découvertes pendant ce test (constats datés)

⚠️ Ces constats ont été mesurés avec le binaire `rose/rose` daté du **18/11 15:20**.
Pendant ce test, d'autres agents ont modifié `rose/src/interp.rs`, `rose/src/run.rs` et
`rose/src/wasm_lecteur.rs` (15:57–16:02) **sans que je n'y touche** : après reconstruction
du binaire, **re-mesurer** ces trois points — ils pourraient être corrigés. Ce document ne
constate que ce qui a été réellement exécuté sur le binaire testé.

1. **Section DATA au format bulk-memory refusée.** Avec une chaîne littérale en C, clang-18
   émet une section DATA dont chaque segment commence par un octet de drapeaux (0x00)
   avant l'offset. `rose/src/wasm_lecteur.rs` lit l'offset directement et renvoie :

   ```
   rose wasm : /tmp/opencode/pywasm/librich.wasm : offset non constant (op 0x00)
   ```

   Le module était pourtant valide (vérificateur : 0 import/start/call_indirect). Les
   démos existantes (xxHash, FNV-1a) n'ont **aucune section DATA**, ce qui masquait le
   problème. **Contournement utilisé** : émettre les octets en constantes numériques.
   Correctif possible (hors périmètre de cette mission, fichier `.rs` non modifié) : lire
   et ignorer l'octet de drapeaux des segments DATA.

2. **Blocage sur les caractères de contrôle en entrée de chaîne.** `rich_cell_len("a\tb")`
   fait boucler Rose indéfiniment (`timeout` → code 124), alors que le **même module**
   appelé via wasmtime renvoie correctement `2`. La chaîne littérale avec tabulation est
   pourtant gérée par Rose hors wasm (`afficher("a\tb")` OK). Le défaut est donc dans le
   chemin d'appel wasm / copie de chaîne, pas dans le module. Non corrigé ici (fichiers
   `.rs` intouchables).

3. `rose wasm` affiche tous les arguments/retours en `entier` (types wasm bruts `i32`) ;
   les déclarations `.rose` générées, elles, sont correctes. Cosétique.

---

## 6. Verdict en 5 points

1. **Rich entier compilé depuis Python vers wasm autonome : NON.** Cython 3.3.1 et mypyc
   2.4.1 produisent des extensions CPython (`Python.h`, `PyInit_*`, `PyObject_Call`) ; le
   convertisseur échoue sur `Python.h` / `init.c`, et un tel module exigerait un runtime
   CPython (imports wasm) interdit par M6.1.
2. **Ce qui a été converti et appelé avec succès :** un port C manuel de 3 fonctions de
   Rich (`cell_len`, `Panel.__rich_measure__`, rendu `Panel.fit`/`ROUNDED`), via
   `rose-wasm-lib.sh`, en un module de 4 755 octets `0/0/0`, chargé par `rose wasm` et
   appelé par `demo_rich.rose`, **retour `chaine` compris**, affichant de vrais panneaux.
3. **Fidélité prouvée :** 28/28 valeurs `cell_len`/`panel_width` identiques à Rich sur 14
   chaînes ; 4/4 panneaux identiques octet pour octet à `export_text()` de Rich 15.0.0.
4. **Le convertisseur reste un outil C→wasm** : il fonctionne parfaitement sur du C
   autonome (y compris retour chaîne), mais ne peut pas avaler du Python ; la tentative
   d'extension `.py` n'a pas été faite car aucun transpileur Python→C autonome n'est présent.
5. **Recommandation :** garder Rich hors du produit, porter le sous-ensemble utile en
   C/Rust pur, documenter la source, et corriger les deux limitations M6.1 du §5 (DATA
   bulk-memory, chaînes avec contrôles) avant d'élargir l'usage du pont.

---

### Fichiers de ce test (tous sous `/tmp/opencode/pywasm/`, aucun commit, aucune publication)

| Fichier | Rôle |
|---|---|
| `rich_bridge.py` | source Python appelant le vrai Rich (échec Cython) |
| `rich_bridge.c` | Cython généré (343 608 o, 2 466 marqueurs CPython) |
| `mini.py`, `build/__native.c` | test mypyc (échec) |
| `rich_algo.c` | port C manuel des algorithmes Rich |
| `librich.wasm`, `librich.rose` | module autonome + déclarations générées |
| `demo_rich.rose`, `diff_test.rose`, `diff_panel.rose` | programmes de démonstration/différentiel |
