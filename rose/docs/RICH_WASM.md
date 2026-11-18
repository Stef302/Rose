# Rich 15.0.0 depuis Rose via WebAssembly — test « textuel » (0.1.5)

⚠ Document de vérification, pas une annonce de succès. Date : 18/11/2026.
Ce sous-agent n'a modifié aucun fichier du dépôt hormis ce document. Aucun
commit, aucune publication. Tous les artefacts compilés sont sous
`/tmp/opencode/`. Aucune dépendance GMP n'a été ajoutée.

⚠ Deux précautions de mesure : (1) toutes les exécutions Rose natives ci-dessous
utilisent `/home/clausake/Stefan/ROSE/rose/rose` tel que construit le
18/11/2026 à 15:20 (avant les modifications concurrentes signalées plus bas) ;
à reconstruire et re-mesurer après tout changement du moteur. (2) En fin de
mission, `git status --short` montrait aussi `M rose/src/interp.rs`,
`M rose/src/run.rs`, `M rose/src/wasm_lecteur.rs` : modifications parallèles
(archi A2 / L1 / cache de décodage wasm) **étrangères à cette mission**,
apparues pendant les tests. Elles ne sont ni miennes, ni prises en compte dans
les résultats ci-dessous.

---

## 1. Verdict en 5 points

1. **Rich entier (la bibliothèque Python) ne tourne PAS dans Rose via le pont
   wasm maison, et ce n'est pas faisable avec les moyens présents.** Rich est
   du Python 3.12 pur (77 fichiers `.py`) qui exige un interpréteur Python
   complet (`os`, `sys`, `re`, `dataclasses`, `typing`, classes, tables de
   dispatch, `sys.stdout`). Le pont Rose exige un module **core-wasm v1
   autonome : 0 import, 0 `start`, 0 `call_indirect`**, donc **aucun runtime
   Python, aucune libc, aucun GC, aucune I/O**.
2. **CPython ne compile pas un module Python en wasm ici** : aucun module
   `wasmtime` Python, aucun paquet `cpython-wasm` / `anypython` sur l'index
   (`No matching distribution found`). RustPython n'est pas installé et
   **n'existe pas sur PyPI** (`pip install rustpython` → `rc=1`, `from
   versions: none`). Donc aucun interpréteur Python exécutable en wasm n'a
   été trouvé ou installé.
3. **Ce qui a réellement été fait** : une **réécriture manuelle en C** d'une
   *seule* fonction de rendu de Rich — `rich.panel.Panel` avec `box.ROUNDED`,
   titre centré, `padding=(0,1)`, une ligne de texte ASCII — compilée par
   `scripts/rose-wasm-lib.sh` (`-O0`, 0 import/start/call_indirect) et
   **appelée depuis Rose** via `@dll("wasm", …)`. Le rendu produit est
   **byte-for-byte identique** à celui du vrai Rich 15.0.0 sur le cas test
   (`md5` identique). **Ce n'est PAS Rich** : ni le module Python Rich, ni
   ses classes, ni ses styles, ni son moteur de mise en page ne sont portés.
4. **Deux limitations réelles du pont ont été rencontrées et contournées** :
   (a) le lecteur wasm de Rose ne lit pas l'octet de drapeaux des segments
   de données bulk-memory émis par `wasm-ld` → tout module avec des données
   statiques initialisées est refusé (`offset non constant (op 0x00)`) ;
   (b) une boucle C `while (s[n] && n < 4096)` est mal exécutée par
   l'interpréteur (chaîne coupée à 2 caractères) — la forme simple
   `while (s[n]) n++` fonctionne ; (c) un titre vide (`""`) fait **boucler
   l'interpréteur à 100 % CPU** sur ce module, alors que le même `.wasm`
   tourne correctement dans Node.
5. **Le wasm web (`rose/web/rose.wasm`, 736 683 o) exécute bien du texte Rose**
   (`execute`, `analyse`, `latex`, parité native, 15/15 tests OK) mais
   **ne peut pas charger un module wasm externe** (`@dll("wasm", …)` →
   « bibliothèque introuvable »). Les exports `rose_add_ent` /
   `rose_affiche_packed` sont des helpers internes du JIT pour les entiers
   (`rose_add_ent(7,35) = 42`, tag 0), **pas** une API de rendu texte.

---

## 2. Étape 1 — faisabilité : commandes et sorties exactes

### 2.1 Rich est installé (Python 3.12)

```
$ python3 --version
Python 3.12.3
$ python3 -m pip show rich
Name: rich
Version: 15.0.0
Summary: Render rich text, tables, progress bars, syntax highlighting, markdown and more to the terminal
Location: /home/clausake/.local/lib/python3.12/site-packages
Requires: markdown-it-py, pygments
Required-by: rich-toolkit, typer
$ ls -la /home/clausake/.local/lib/python3.12/site-packages/rich/__init__.py
-rw-rw-r--  1 clausake clausake 6131 Jul 15 21:21 /home/clausake/.local/lib/python3.12/site-packages/rich/__init__.py
```

### 2.2 Ce que Rich importe (inventaire AST réel, 77 fichiers `.py`)

```
$ python3 - <<'PY'   # ast.walk sur rich/*.py, comptage des imports de premier niveau
...
PY
files: 77
top-level imports: {'IPython': 3, '__future__': 8, 'abc': 7, 'argparse': 4,
 'array': 1, 'ast': 1, 'attr': 1, 'builtins': 1, 'collections': 5,
 'colorsys': 3, 'configparser': 1, 'contextlib': 2, 'ctypes': 4,
 'dataclasses': 9, 'datetime': 4, 'enum': 2, 'fractions': 1, 'functools': 9,
 'getpass': 1, 'html': 2, 'inspect': 8, 'io': 7, 'ipywidgets': 1,
 'itertools': 11, 'json': 2, 'linecache': 1, 'logging': 2, 'markdown_it': 2,
 'math': 6, 'mmap': 1, 'operator': 9, 'os': 11, 'pathlib': 2, 'pickle': 1,
 'platform': 2, 'pty': 1, 'pydoc': 1, 'pygments': 11, 'random': 3, 're': 8,
 'reprlib': 1, 'rich': 97, 'sys': 19, 'textwrap': 1, 'threading': 4,
 'time': 12, 'traceback': 1, 'types': 8, 'typing': 72,
 'typing_extensions': 2, 'warnings': 2, 'zlib': 1}
```

Conclusion : impossible à exécuter sur un module wasm sans import (le code de
`wasm_lecteur.rs` refuse explicitement la section `IMPORT` et le `start`).

### 2.3 CPython → wasm ? Non, pas ici.

```
$ python3 -c 'import wasmtime; print(wasmtime.__file__)'
Traceback (most recent call last):
  File "<string>", line 1, in <module>
ModuleNotFoundError: No module named 'wasmtime'
rc=1
$ python3 -m pip index versions anypython
ERROR: No matching distribution found for anypython
$ python3 -m pip index versions cpython-wasm
ERROR: No matching distribution found for cpython-wasm
$ python3 -m pip download --no-deps -d /tmp/opencode/dl cpython-wasm
ERROR: Could not find a version that satisfies the requirement cpython-wasm (from versions: none)
ERROR: No matching distribution found for cpython-wasm
```

Les commandes `pip index` fonctionnent (preuve que l'index est joignable) :

```
$ python3 -m pip index versions rich | head -2
rich (15.0.0)
Available versions: 15.0.0, 14.3.4, 14.3.3, ...
```

L'absence de `anypython`/`cpython-wasm` n'est donc **pas** un problème réseau.
CPython n'a pas de backend wasm dans cette installation ; aucun communicateur
Python→wasm n'a été trouvé ni installé.

### 2.4 RustPython ? Pas installé, pas sur PyPI.

```
$ which rustpython
rc=1
$ python3 -m venv /tmp/opencode/rustpy-venv
venv_rc=0
$ timeout 120 /tmp/opencode/rustpy-venv/bin/pip install rustpython > /tmp/opencode/rustpy-install.log 2>&1
install_rc=1
$ cat /tmp/opencode/rustpy-install.log
ERROR: Could not find a version that satisfies the requirement rustpython (from versions: none)
ERROR: No matching distribution found for rustpython
$ /tmp/opencode/rustpy-venv/bin/python -c 'import rustpython; print(rustpython)'
Traceback (most recent call last):
  File "<string>", line 1, in <module>
ModuleNotFoundError: No module named 'rustpython'
import_rc=1
$ python3 -m pip index versions rustpython
ERROR: No matching distribution found for rustpython
```

(PyPI propose bien `wasmtime` 50.0.1 et `pyodide-py` 0.27.8, mais ce sont
**d'autres runtimes**, non installés ici et surtout **non branchés sur le pont
`@dll("wasm", …)` de Rose**.)

---

## 3. Étape 2 — port C d'un sous-ensemble de Rich, appelé depuis Rose

### 3.1 Référence réelle (CPython, hors Rose)

```
$ python3 - <<'PY'
from rich.console import Console
from rich.panel import Panel
c = Console(width=48, force_terminal=False, color_system=None, highlight=False)
c.print(Panel("Bonjour Rose", title="Test", width=30))
PY
╭─────────── Test ───────────╮
│ Bonjour Rose               │
╰────────────────────────────╯
```

Algorithme lu dans le vrai code installé (`rich/panel.py` lignes 110-270,
`rich/box.py` `ROUNDED`) : `_title()` fait `Text.pad(1)` ; `align_text(width,
"center", box.top)` pose `left = excess_space // 2` ; le haut est
`top_left + top + align_text + top + top_right` ; le corps est
`mid_left + Padding(renderable,(0,1)) + mid_right` ; le bas est
`get_bottom([width-2])`.

### 3.2 Ce qui a été porté (fichier `/tmp/opencode/rich_subset/panel.c`)

**Porté** : `box.ROUNDED` (╭ ╮ ╰ ╯ │ ─), titre centré avec un espace de
part et d'autre, `padding=(0,1)`, **une** ligne de texte ASCII, largeur
fixée, retour d'une chaîne C terminée par zéro.

**Non porté** : le module Python Rich lui-même, les styles/ANSI/markup,
`highlight`, `Text`, `Console.render_lines` (retour à la ligne), `subtitle`,
`expand`/`Measurement`, `cell_len` (wcwidth unicode), les tables, le markdown,
les barres de progression, `sys.stdout`, `dataclasses`, `typing`, `re`, `os`.
Les chaînes d'entrée sont **ASCII** (pas d'UTF-8multicolonne). Ce test porte
donc sur **la formule de rendu d'un panneau**, pas sur Rich.

### 3.3 Première compilation : module refusé par Rose (bug du lecteur)

```
$ cd /tmp/opencode/rich_subset
$ bash /home/clausake/Stefan/ROSE/scripts/rose-wasm-lib.sh \
    --export 'rich_panel=panel_rendre(chaine,entier,chaine)->chaine' \
    --out librich_panel --dir /tmp/opencode/rich_subset panel.c
==> compilation: .../clang ... -O0 ... -> ./librich_panel.wasm
WASM_CHECK imports=0 start=0 call_indirect=0 memory_exported=yes
exports=rich_panel
==> OK: ./librich_panel.wasm  (3871 octets)
$ cd /home/clausake/Stefan/ROSE && ./rose/rose wasm /tmp/opencode/rich_subset/librich_panel.wasm
rose wasm : ... : offset non constant (op 0x00)
$ ./rose/rose /tmp/opencode/rich_subset/demo.rose
Rose : erreur — appel WebAssembly : module WebAssembly invalide (...) : offset non constant (op 0x00)
```

Diagnostic (lecture octet par octet) : `wasm-ld` émet une section `data` avec
`count=1, flags=0x00, offset=41 80 08 0b` (`i32.const 1024 ; end`) ; or
`rose/src/wasm_lecteur.rs::lire_offset` lit **directement** l'octet suivant
`count` et attend `0x41` — il ne consomme pas le `flags` des segments
bulk-memory. Le script de compilation, lui, ne voit qu'un module valide
(0 import/start/call_indirect) et le valide. **Contournement** : supprimer
toute donnée statique initialisée du C (chaînes et tableaux `const`), et
construire les octets UTF-8 à l'exécution (`octet(0xE2); octet(0x95);
octet(0xAD);` pour ╭, etc.). Nouvelle section : plus de section 11.

```
sections: [(1, 25), (3, 13), (5, 3), (6, 8), (7, 23), (10, 3770), (0, 143), (0, 127), (0, 44)]
$ ./rose/rose wasm /tmp/opencode/rich_subset/librich_panel2.wasm
@dll("wasm", "/tmp/opencode/rich_subset/librich_panel2.wasm")
@sym("rich_panel")
fn rich_panel(a0: entier, a1: entier, a2: entier) -> entier
1 fonction(s) exportée(s)
```

(`rose wasm` affiche tous les paramètres en `entier` : il ne connaît pas le
type `chaine`=pointeur i32. Le `.rose` généré, lui, déclare bien `chaine`.)

### 3.4 Deuxième obstacle : boucle C mal exécutée par l'interpréteur

Avec `static int ascii_len(const char *s) { int n=0; while (s[n] && n < 4096) n++; return n; }`
exporté en plus :

```
$ ./rose/rose /tmp/opencode/rich_subset/test_long.rose
2
2
0
```

alors que les octets écrits en mémoire sont corrects (export diagnostic
`premiers("Bonjour Rose")` → `1114599018` = `0x426f6e6a` = `"Bonj"`,
`premiers("Test")` → `1415934836` = `0x54657374` = `"Test"`). Le module
tourne correctement dans Node avec les mêmes arguments : le défaut est donc
dans l'exécution Rose, pas dans le C ni dans l'ABI.

En remplaçant par la forme identique à celle de `mini_hash.c` connue pour
marcher (`while (s[n]) n++`) :

```
$ ./rose/rose /tmp/opencode/rich_subset/test5.rose
12
4
0
╭─────────── Test ───────────╮
│ Bonjour Rose               │
╰────────────────────────────╯
```

### 3.5 Preuve finale : rendu identique au vrai Rich

Fichiers : `/tmp/opencode/rich_subset/panel_dbg.c` (source C),
`librich_panel5.wasm` (4 592 o), `librich_panel5.rose` (déclarations) :

```
# AUTO-GENERE par rose-wasm-lib.sh — importer ces declarations dans Rose.
@dll("wasm", "/tmp/opencode/rich_subset/librich_panel5.wasm")
@sym("rich_panel")
fn rich_panel(a0: chaine,a1: entier,a2: chaine) -> chaine

@dll("wasm", "/tmp/opencode/rich_subset/librich_panel5.wasm")
@sym("longueur")
fn longueur(a0: chaine) -> entier
```

Vérification du module et exécution :

```
$ bash scripts/rose-wasm-lib.sh \
   --export 'rich_panel=panel_rendre(chaine,entier,chaine)->chaine' \
   --export 'longueur=ascii_len(chaine)->entier' \
   --out librich_panel5 --dir /tmp/opencode/rich_subset panel_dbg.c
WASM_CHECK imports=0 start=0 call_indirect=0 memory_exported=yes
exports=rich_panel,longueur
==> OK: ./librich_panel5.wasm  (4592 octets)

$ cat /tmp/opencode/rich_subset/test5.rose
importer "/tmp/opencode/rich_subset/librich_panel5.rose"
afficher(longueur("Bonjour Rose"))
afficher(longueur("Test"))
afficher(longueur(""))
afficher(rich_panel("Bonjour Rose", 30, "Test"))

$ cd /home/clausake/Stefan/ROSE && ./rose/rose /tmp/opencode/rich_subset/test5.rose
12
4
0
╭─────────── Test ───────────╮
│ Bonjour Rose               │
╰────────────────────────────╯
```

Comparaison byte-for-byte avec la sortie réelle de Rich 15.0.0 :

```
$ sed -n '4,6p' /tmp/opencode/rich_subset/test5.out > rose_panel_final.txt
$ cmp /tmp/opencode/rich_reference.txt rose_panel_final.txt
cmp_rc=0
$ md5sum /tmp/opencode/rich_reference.txt rose_panel_final.txt
dd2d628c11600c804c0914ec40f698da  /tmp/opencode/rich_reference.txt
dd2d628c11600c804c0914ec40f698da  rose_panel_final.txt
$ cat rose_panel_final.txt
╭─────────── Test ───────────╮
│ Bonjour Rose               │
╰────────────────────────────╯
```

⚠ Ce `cmp` porte sur **une fonction réimplémentée**, pas sur Rich en
exécution : les lignes 1-3 de `test5.out` sont `12`, `4`, `0` (tests de
`longueur`) et le panneau commence à la ligne 4.

Autres cas réels :

```
$ ./rose/rose one.rose     # rich_panel("Rose", 20, "X")
╭─────── X ────────╮
│ Rose             │
╰──────────────────╯
$ node /tmp/opencode/rich_subset/node_test.mjs    # MÊME .wasm, hors Rose
--- text="Bonjour Rose" width=30 title="Test"
╭─────────── Test ───────────╮
│ Bonjour Rose               │
╰────────────────────────────╯
--- text="Rose" width=20 title=""
╭──────────────────╮
│ Rose             │
╰──────────────────╯
```

### 3.6 Bug confirmé : titre vide → boucle infinie dans Rose

```
$ printf 'importer "..."\nafficher(rich_panel("Rose", 20, ""))\n' > empty_title.rose
$ timeout 15 ./rose/rose empty_title.rose
rc=124                       # aucune sortie
$ ps -o pid,etime,pcpu,rss,stat,cmd -p <pid>
    PID     ELAPSED %CPU   RSS STAT CMD
 905782       00:03 99.6  3728 Sl   ./rose/rose .../empty_title.rose
```

Le même appel avec `"X"` ou `" "` réussit (`space_rc=0`) ; `longueur("")`
renvoie `0`. Le `.wasm` est donc valide (Node le rend correctement) mais
l'interpréteur Rose 0.1.5 entre dans une boucle sans fin sur ce cas. À ne pas
confondre avec un échec de compilation : **le pont compile, l'exécution
boucle**.

---

## 4. Étape 3 — `rose/web/rose.wasm` et le texte

### 4.1 Exports réels (Node 26.12.1)

```
$ node /tmp/opencode/rich_subset/wasm_exports.mjs
module: /home/clausake/Stefan/ROSE/rose/web/rose.wasm 736683 octets
exports: memory, rose_add_ent, rose_affiche_packed, rose_alloc, rose_analyse,
 rose_clone_ent, rose_dernier_panic, rose_erreur_div_flottant_zero,
 rose_erreur_div_zero, rose_erreur_mod_zero, rose_execute, rose_execute_latex,
 rose_free, rose_free_buffer, rose_get_ret_tag, rose_mul_ent, rose_neg_ent,
 rose_orc_affiche_complexe, rose_orc_affiche_f64, rose_prendre_sortie,
 rose_raz_panic, rose_set_ret_tag, rose_sub_ent, rose_version
memory pages: 17
rose_analyse function 2
rose_add_ent function 6
rose_affiche_packed function 2
rose_execute function 2
rose_version function 0
```

### 4.2 Ce qui marche : exécution, analyse, LaTeX, parité native

```
$ node rose/web/test-wasm.mjs rose/web/rose.wasm
module  : rose/web/rose.wasm
version : Rose 0.1.5 — compilateur Rust + JIT Cranelift (sémantique exacte)
natif   : /home/clausake/Stefan/ROSE/rose/target/release/rose
OK   parité native : addition
OK   parité native : flottants
OK   parité native : rationnel exact
OK   parité native : entier exact
OK   parité native : boucle (pour)
OK   parité native : fonction
OK   parité native : sortie multiple
OK   parité native : complexes
OK   parité native : irrationnels
OK   parité native : comparaisons
OK   erreur runtime
OK   erreur d'analyse
OK   latex
OK   parité native : latex du programme
OK   analyse
Tous les tests passent.
rc=0
```

Test texte dédié (glue `rose/web/rose-wasm.js`, sans modification du dépôt) :

```
$ node /tmp/opencode/rich_subset/web_text_test.mjs
=== chaine simple
execute: "Bonjour Rose\n"
analyse: "programme valide — 1 instruction(s), 0 fonction(s)\nvérification : OK"
latex  : "\\mathrm{afficher}\\left(\\text{Bonjour Rose}\\right)\n"
=== longueur
execute: "12\n"
analyse: "programme valide — 1 instruction(s), 0 fonction(s)\nvérification : OK"
latex  : "\\mathrm{afficher}\\left(\\operatorname{longueur}(\\text{Bonjour Rose})\\right)\n"
=== sous_chaine
execute: "Bonjour\n"
analyse: "programme valide — 1 instruction(s), 0 fonction(s)\nvérification : OK"
latex  : "\\mathrm{afficher}\\left(\\operatorname{sous_chaine}(\\text{Bonjour Rose}, 0, 7)\\right)\n"
```

### 4.3 Ce qui manque : charger une bibliothèque wasm depuis le web

```
=== dll wasm depuis le web
execute: "Rose : erreur — bibliothèque introuvable (/tmp/opencode/rich_subset/librich_panel5.wasm)\n"
analyse: "programme valide — 1 instruction(s), 1 fonction(s) : rich_panel(3)\nvérification : OK"
latex  : "\\operatorname{rich_panel}(a0 \\in \\mathrm{chaine}, a1 \\in \\mathbb{Z}, a2 \\in \\mathrm{chaine}) \\to \\mathrm{chaine} \\ \\text{(fonction externe)}\n..."
```

Le front-end accepte la déclaration (analyse OK) mais l'exécution web ne
peut pas ouvrir le fichier.Dans `rose/src/interp.rs`, le module
`wasm_ffi` est sous `#[cfg(not(target_arch = "wasm32"))]` : le pont
`@dll("wasm", …)` n'est pas compilé dans la cible wasm. Donc le panneau
C→wasm qui tourne dans le Rose natif **ne peut pas tourner dans le Rose web**.

### 4.4 Les exports « bas niveau » ne sont pas une API texte

```
$ node /tmp/opencode/rich_subset/lowlevel.mjs
rose_add_ent(7,35) = 42n tag = 0n
rose_prendre_sortie après rose_affiche_packed(42) = ""
```

`rose_add_ent(a, at, b, bt, out, outt)` est un helper arithmétique du JIT
(`rose/src/run.rs`) avec des entiers taggés ; `rose_affiche_packed(i128)` est
le helper d'affichage du JIT. Aucun des deux ne rend du texte riche (couleurs,
tableaux, panneaux, markdown).

---

## 5. Alternatives réalistes (non exécutées ici, à ne pas confondre avec un résultat)

- **Porter fonction par fonction en C/Rust autonome**, comme fait ici pour
  `Panel` : c'est la seule voie qui fonctionne **aujourd'hui** avec le pont
  Rose (preuve : `cmp_rc=0` sur le panneau). Limites : pas de wrap, pas de
  styles, données statiques interdites, attention aux boucles composées et
  aux chaînes vides.
- **Compiler un interpréteur Python complet en wasm** (CPython/WASI,
  RustPython depuis les sources, Pyodide) puis l'intégrer : changerait de
  runtime, exigerait un module wasm avec imports/start (ex. WASI) et un
  système de fichiers — **incompatible avec le pont M6.1 actuel** (0 import).
  Aucun de ces runtimes n'est installé ni testé ici.
- **Utiliser un runtime wasm externe côté hôte** (`wasmtime` Python dispo sur
  PyPI mais non installé) pour exécuter un module Python compilé ailleurs :
  ce ne serait **pas Rose** qui exécuterait Rich, et nécessiterait un
  backend Python→wasm qui n'est pas présent.
- **Étendre le lecteur Rose** : (a) lire le `flags` des segments de données,
  (b) corriger l'exécution des boucles à conditions composées, (c) corriger
  le cas chaîne vide, (d) exposer le pont aussi en `wasm32`. Ce sont des
  modifications du moteur, **hors du périmètre de cette mission** (aucune
  source `.rs` touchée).

---

## 6. État final factuel

- Fait : CPython 3.12 + Rich 15.0.0 présents ; aucun compilateur Python→wasm,
  aucun RustPython ; port C d'un sous-ensemble de `Panel` compilé en wasm via
  `scripts/rose-wasm-lib.sh` et appelé depuis Rose avec un rendu identique au
  vrai Rich sur un cas (md5 `dd2d628c…`).
- Vérifié : `WASM_CHECK imports=0 start=0 call_indirect=0 memory_exported=yes`,
  `cmp_rc=0`, `test-wasm.mjs` 15/15, exports Node listés, bug du titre vide
  reproduit à 99,6 % CPU.
- Non fait / non prouvé : **Rich entier ne tourne pas dans Rose** ; aucun
  rendu Rich réel (tableau, markdown, couleurs ANSI) n'a été exécuté via le
  pont ; ce sous-agent n'a modifié aucun fichier du dépôt autre que ce
  document ; aucun commit ni publication. Les trois fichiers `.rs` modifiés
  vus en fin de mission par `git status` proviennent d'un autre chantier
  concurrent et n'ont pas été rebuildés ni re-testés ici.
- Reste : si l'objectif est réellement « Rich dans Rose », il faut soit un
  runtime Python en wasm compatible avec un pont à imports (hors de portée du
  pont M6.1), soit porter chaque fonction voulue en C/Rust autonome — en
  documentant précisément ce qui est perdu à chaque port.
