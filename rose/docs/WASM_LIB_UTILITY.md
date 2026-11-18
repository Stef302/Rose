# Utilitaire : bibliothèque C existante → Rose en une commande

`scripts/rose-wasm-lib.sh` compile un ou plusieurs `.c` en un module **core-wasm v1
autonome** (pont M6.1) et génère le `.rose` de déclarations `@dll`/`@sym`/`fn`
prêt à `importer`. Il ne modifie aucune source C, n'ajoute **aucune dépendance
(en particulier aucun GMP)** et ne touche ni aux `.rs`, ni à `Cargo.toml`.

---

## 1. Mode d'emploi en 5 lignes

```sh
# 1.aller dans le dossier des sources C
cd /chemin/vers/la/bibliotheque
# 2.une seule commande : --export 'SYMBOLE=FONCTION_C(types Rose)->ret'
bash /home/clausake/Stefan/ROSE/scripts/rose-wasm-lib.sh \
     --export 'xxh32_str=XXH32(chaine,entier,entier)->entier' --out libxxhash xxhash.c
# 3.le module est verifie (0 import, 0 start, 0 call_indirect, memoire exportee)
/chemin/rose/rose wasm libxxhash.wasm
# 4.importer les declarations generees dans un programme Rose
#    importer "libxxhash.rose"
# 5.appeler : xxh32_str("abc", 3, 0)
```

Types Rose acceptés dans la signature : `entier`, `booleen`, `chaine`,
`flottant`, `rationnel`. Plusieurs `--export` sont autorisés dans la même
commande (un seul `.wasm`, un `.rose` avec toutes les déclarations).

---

## 2. Exemple complet : xxHash v0.8.2 (BSD-2-Clause)

Sources réelles : `/tmp/opencode/interop/xxhash.{h,c}` (cf.
`INTEROP_BIBLIOTHEQUE.md`).

```sh
cd /tmp/opencode/interop
bash /home/clausake/Stefan/ROSE/scripts/rose-wasm-lib.sh \
  --export 'xxh32_str=XXH32(chaine,entier,entier)->entier' \
  --out libxxhash_gen xxhash.c
```

Sortie réelle :

```
==> compilation: .../clang ... -O0 ... -> ./libxxhash_gen.wasm
WASM_CHECK imports=0 start=0 call_indirect=0 memory_exported=yes
exports=xxh32_str
==> OK: ./libxxhash_gen.wasm  (3282 octets)
==> OK: ./libxxhash_gen.rose
==> Verifie: 0 import, 0 start, 0 call_indirect, memoire exportee. -O0 force.
```

Fichier `libxxhash_gen.rose` généré :

```rose
# AUTO-GENERE par rose-wasm-lib.sh — importer ces declarations dans Rose.
# Module: /tmp/opencode/interop/libxxhash_gen.wasm
# XXH32(chaine,entier,entier)->entier  =>  xxh32_str
@dll("wasm", "/tmp/opencode/interop/libxxhash_gen.wasm")
@sym("xxh32_str")
fn xxh32_str(a0: chaine,a1: entier,a2: entier) -> entier
```

Chargement et exécution (depuis la racine du dépôt) :

```
$ ./rose/rose wasm libxxhash_gen.wasm
@dll("wasm", "libxxhash_gen.wasm")
@sym("xxh32_str")
fn xxh32_str(a0: entier, a1: entier, a2: entier) -> entier
1 fonction(s) exportée(s)

$ cat demo_rose_lib.rose
importer "/tmp/opencode/interop/libxxhash_gen.rose"
afficher(xxh32_str("", 0, 0))
afficher(xxh32_str("abc", 3, 0))
afficher(xxh32_str("abc", 3, 12345))
afficher(xxh32_str("Hello, world!", 13, 0))

$ ./rose/rose demo_rose_lib.rose
46947589
852579327
1853483049
834093149
```

`46947589` = `0x02CC5D05` (vecteur officiel pour la chaîne vide), `852579327`
= `0x32D153FF` (vecteur officiel pour `"abc"`) : les deux sont identiques à la
référence native gcc documentée dans `INTEROP_BIBLIOTHEQUE.md`.

---

## 3. Deuxième démonstration : mini-hash FNV-1a (généricité)

Bibliothèque écrite pour le test : `/tmp/opencode/interop/mini_hash.c`, fonction
`fnv1a_calc(const unsigned char*, unsigned int)` et `text_len(const char*)`.

```sh
bash scripts/rose-wasm-lib.sh \
  --export 'fnv1a_rose=fnv1a_calc(chaine,entier)->entier' \
  --export 'text_len_rose=text_len(chaine)->entier' \
  --out libminihash mini_hash.c
```

Résultat du vérificateur : `imports=0 start=0 call_indirect=0 memory_exported=yes`,
`exports=fnv1a_rose,text_len_rose`, module de 1 625 octets ; `rose wasm` affiche
2 fonctions exportées.

```
$ ./rose/rose demo_hash.rose
-2128831035      # ""  = 0x811C9DC5 (signé)
440920331        # "abc" = 0x1A47E90B — identique à zlib/python
-309268332       # "Hello, world!" = 0xED90F094 (signé)
3                # text_len("abc")
```

Deux exports dans un seul module, un type `chaine` seul, et un retour `entier`
signé : l'utilitaire n'est pas spécialisé pour xxHash.

---

## 4. Ce que fait l'utilitaire

1. **Shim mémoire** : génère `memcpy`/`memset` en simples boucles (freestanding),
   sans libc.
2. **Adaptateur d'ABI** : pour chaque paramètre `chaine`, le wrapper généré
   calcule la longueur jusqu'au NUL, copie les octets dans un tampon statique
   (`RW_BUF`, 16 Ko par défaut, +32 de marge) puis appelle la vraie fonction C.
   Le `strlen` est **inliné** dans le wrapper : un helper `static` séparé fait
   émettre à clang-18 un `call_indirect` parasite (mesuré), refusé par M6.1.
3. **Compilation** : `--target=wasm32-wasi`, `-nostdlib`, `--no-entry`,
   `--export-memory`, `-Wl,--export=<SYMBOLE>` et **toujours `-O0`**.
4. **Vérification** (lecture des sections du binaire + `llvm-objdump`) : refuse
   tout module avec des imports, une section `start`, un `call_indirect` ou une
   mémoire non exportée.
5. **Génération** du `.rose` avec des chemins absolus (fonctionne depuis n'importe
   quel répertoire) et des commentaires rappelant la signature d'origine.

Crochet de test : `RW_ALLOW_UNDEFINED=1` passe `-Wl,--allow-undefined` à wasm-ld
pour produire un module contenant un import et prouver le refus du détecteur :

```
WASM_CHECK imports=1 start=0 call_indirect=0 memory_exported=yes
ERREUR: incompatible M6.1: 1 import(s)     # section IMPORT: env.puts
```

---

## 5. Limites (à ne pas dépasser)

| Limite | Détail |
|---|---|
| **Types** | `entier`/`booleen` → i32 (`int`), `flottant`/`rationnel` → f64 (`double`), `chaine` → i32 pointeur (`const char*`). Pas de `entier64` / i64 (mot-clé absent côté Rose), pas de `void`, pas de pointeurs arbitraires, pas de tableaux. |
| **`-O0` obligatoire** | Forcé par l'outil. `-O1`/`-O2`/`-Os` produisent un module mal exécuté par le pont (accès mémoire hors limites, pointeurs corrompus) — voir `INTEROP_BIBLIOTHEQUE.md` §7. |
| **Imports / start / call_indirect** | Refusés. Donc pas de `malloc`, `printf`, `qsort` (pointeur de fonction), ni d'API système. Les bibliothèques doivent être pures et autonomes. |
| **Chaînes** | Rose ne fournit que `len+1` octets en fin de mémoire : l'adaptateur copie dans un tampon statique de 16 Ko. Au-delà, la chaîne est tronquée. Les retours `chaine` ne sont pas garantis. |
| **Contrôle de flux** | Le pont exécute mal les boucles imbriquées (mesuré : un CRC32 tableless avec `while` dans `for` **bloque indéfiniment** alors que FNV-1a à boucle simple passe). Préférer des fonctions à boucle unique. |
| **Entiers signés** | Les `unsigned` > 2³¹ reviennent négatifs en Rose (`as_i32()`) : c'est le même motif 32 bits (ex. `-2128831035` = `0x811C9DC5`). |
| **Portée** | Cet utilitaire ne rend pas *toute* bibliothèque C appelable : il cible les fonctions pures à arguments scalaires/chaines et à boucles simples. Un échec de compilation, de vérification ou d'exécution doit être traité en adaptant la bibliothèque, pas en contournant les vérifications. |

Aucun commit n'a été fait. Fichiers livrés : `scripts/rose-wasm-lib.sh` et ce
document. Tous les tests vivent sous `/tmp/opencode/interop/`.
