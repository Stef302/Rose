# RVM complète — snapshot AST ROSEBC v3

**Constat daté : 21/11/2026.** Dépôt `Stef302/Rose`, `rose/src/rosevm.rs`,
`rose/src/main.rs`. Aucun commit, aucune publication, aucune dépendance ajoutée.
Les fichiers `rir.rs`, `interp.rs`, `run.rs` et `front.rs` n'ont **pas** été
modifiés.

> Chaque mesure ci-dessous est un **constat daté**, pas une consigne.

---

## 1. Ce qui bloquait

État prouvé avant cette mission :

- `rose bc` produisait un vrai `.rosebc` maison (magic `ROSEBC`, format de
  **classe v2** avec pool de constantes, méthodes, pile d'opérandes), mais en
  abaissant uniquement le **corps principal** en RIR via
  `rir::abaisser_principal`.
- Dès qu'un programme contenait un appel de fonction, une chaîne, une table,
  un rationnel ou une E/S, l'abaissement produisait un `Op::AffectAst` /
  `Op::Afficher` / `Op::ExprStmt`, marqué `OP_REFUSE` (`0x80`) puis rejeté au
  chargement par `verifier_sous_ensemble` :
  **`rose bc : bytecode Rose : opération \`AST\` refusée par la sandbox`**.
- `rose rvm fichier.rosebc` sans arguments de classe/méthode paniquait
  (`&argv[5..]` hors limites).

Mesure de départ (binaire HEAD `d164bca`, `/tmp/opencode/fib_baseline.rose`) :

```
$ rose bc fib.rose -o fib.rosebc
rose bc : bytecode Rose : opération `AST` refusée par la sandbox   # rc=1
$ rose rvm fib.rosebc
panicked at src/main.rs:1575:72: range start index 5 out of range for slice of length 3
```

---

## 2. Ce qui a été fait

### 2.1 Nouveau format : `ROSEBC` v3 = snapshot AST complet

`VERSION_AST = 3`. Le fichier sérialise **tout le programme Rose** :

```
magic[6] = "ROSEBC" | version u32 = 3
n_imports u32 | chaines
n_signatures u32 | signatures :
    nom, paramètres (nom + Ty), retour Ty, a_retour,
    corps présent ? (n_instr + instructions), dll, dll_son, symbole, ligne
n_instructions u32 | instructions (le principal)
```

Nouveaux opcodes tagués (u8) — **aucun n'est emprunté à Java** :

| Famille | Tags | Couvre |
|---|---|---|
| Types | `TK3_*` (0–12) | `TyKind` : entier, rationnel, flottant, booléen, chaîne, liste, table, tampon, tableau, complexe, irrationnel, dyn |
| Expressions | `EX3_LITTERAL`, `EX3_LITCOMPLEXE`, `EX3_CHAINE`, `EX3_BOOLEEN`, `EX3_VAR`, **`EX3_APPEL`**, `EX3_BINAIRE`, `EX3_UNAIRE`, `EX3_FACTORIELLE`, `EX3_MULTIFACTORIELLE`, `EX3_SOUSFACTORIELLE`, `EX3_PRIMORIELLE`, `EX3_TERMINAL`, `EX3_SUPERFACTORIELLE`, `EX3_INDEX`, **`EX3_LISTE`**, `EX3_LISTEVIDETYPEE`, **`EX3_TABLE`** | appels de fonction et primitives std, chaînes, rationnels, tables/listes, indexation, complexes, postfixes exacts |
| Instructions | `IN3_AFFICHER`, **`IN3_LIRE`**, `IN3_AFFECT`, `IN3_AFFECTINDEX`, `IN3_SI`, `IN3_TANTQUE`, `IN3_POUR`, `IN3_RETOUR`, `IN3_EXPRSTMT`, `IN3_BLOC`, `IN3_SPAWN`, `IN3_ESSAYER`, `IN3_LEVER` | E/S, contrôle de flux, récursion, exceptions |

API ajoutée dans `rosevm.rs` :

- `ecrire_snapshot(&Programme) -> Vec<u8>` — **ne refuse rien** ;
- `lire_snapshot(&[u8]) -> Result<Programme, Erreur>` — vérificateur propre
  (magic, version, troncature, octets en trop, compteurs, UTF-8, tags,
  booléens, profondeurs d'expression/instruction), zéro panic sur fichier
  hostile ;
- `executer_snapshot(octets, args) -> Result<String, Erreur>` — exécution ;
- `version_fichier(octets)` — discriminateur CLI v1/v2/v3 ;
- erreurs `SnapshotInvalide` et `ExecutionRose(String)` (message runtime Rose
  réel, pas un « erreur interne » opaque).

⚠ Le cache `Signature::rir` n'est **jamais sérialisé** : c'est un dérivé de
l'AST, reconstruit à l'exécution. Le snapshot est donc bien une photographie
du **source**, pas d'un état d'optimisation.

### 2.2 Exécution : la mécanique Rose, pas une JVM

`rose rvm` sur un v3 reconstruit le `Programme` puis le fait tourner par
`Interp::nouveau(prog).executer_principal()` : **slots + pile de trames**
(la mécanique demandée), avec le même moteur que l'interpréteur. Aucune classe
Java, aucun `.class`, aucune JVM, aucun bytecode JVMS n'intervient.

Pour une parité **octet pour octet** avec `rose run`, `executer_snapshot`
reconstruit aussi les caches RIR exactement comme `executer_source_analyse` :
`declarer_rir`, abaissement de chaque fonction, abaissement du principal
(sauf boucle euclidienne reconnue). Deux exceptions documentées :

- les programmes contenant `pi` / `phi` / `e` restent en AST pur, car le RIR
  ne sait pas résoudre ces constantes (mesuré : `>> pi` échoue en RIR alors
  que l'AST pur affiche π). **La complétude prime sur le formatage** ;
- une boucle euclidienne reste en AST (spécialisation native de `exec`),
  comme dans l'interpréteur.

### 2.3 Câblage CLI (`main.rs`)

- `cmd_bc` écrit maintenant `ecrire_snapshot(&prog)` : plus aucun refus AST.
- `cmd_rvm` lit la version : v3 → `executer_snapshot` (sortie capturée, non
  dupliquée) ; v1/v2 → ancienne machine à classes/méthodes (rétrocompat).
- correction du panic `rose rvm fichier.rosebc` sans classe/méthode.
- aide `rose help bc` / `rose help rvm` mise à jour.

---

## 3. Preuves de sortie (mesuré le 21/11/2026)

### 3.1 Objectif

```
$ rose bc fib.rose -o fib.rosebc
rose bc : 292 octets -> fib.rosebc
$ rose rvm fib.rosebc
75025                      # fib(25)

$ rose bc fib28.rose -o fib28.rosebc && rose rvm fib28.rosebc
317811                     # fib(28)
```

### 3.2 Chaînes / tables / rationnels / appels

```
$ rose bc t1.rose -o t1.rosebc && rose rvm t1.rosebc
0.5
[1, 2, 3]
bonjour
[a => 1, b => 2]
```

### 3.3 Parité `rose run` = `rose bc` + `rose rvm` (11 programmes)

OK : `fib.rose`, `a.rose` (appels imbriqués), `b.rose` (pour/tant_que/si),
`d.rose` (table + liste + `chaine()` + `afficher()`), `f.rose`,
`t1.rose`, `arithmetique.rose`, `fibonacci.rose`, `flottants.rose`,
`math.rose`, `premiers.rose`.

⚠ `examples/factorielle.rose` affiche un temps (`($fin - $debut)/1000000`)
qui varie **déjà entre deux `rose run`** (215,49 / 209,62 / …) : ce n'est pas
une divergence de la RVM.

### 3.4 Rétrocompatibilité v1/v2

Un `.rosebc` v2 généré par la machine à classes (`carre`, méthode
`Principal.carre`, appel `carre(7)`) tourne toujours :

```
$ rose rvm carre_v2.rosebc Principal carre 7
49
```

### 3.5 Tests / build / point fixe

- `cargo build --release` : **0 erreur**, dans le worktree propre à `d164bca`
  (changements de cette mission seuls) **et** dans l'arbre principal après
  que la session parente a commité son chantier WASM (`4538a0d`).
- `cargo test --release` dans le worktree propre : **302 tests lib + 315 tests
  binaires, 0 échec**. Dans l'arbre principal : **306 passés, 2 échecs** —
  les deux sont dans `link_natif.rs` (`elf_avec_valeur_exacte_ecrit_le_texte_memorise`,
  `stub_avec_affichage_est_plus_long_et_contient_write`), c'est-à-dire le
  chantier AOT6 **d'une autre session**, sans rapport avec la RVM.
  `cargo test --lib rosevm::` dans l'arbre principal : **30 passés, 0 échec**.
- `rose rir compilateur/emetteur.rose` ×2 → md5
  `b3ed9262dd54776a6dbd07bc12368d0e`, 263 520 octets, stderr 0, `cmp`
  identique. **Point fixe intact** (worktree propre et arbre principal).

⚠ VÉRITÉ D'ÉTAT : la session parente a commité pendant cette mission
(`4538a0d`), y compris le câblage `main.rs` de cette RVM ; les changements
`rosevm.rs` + ce document restent non commités. Aucune publication, aucun tag,
aucune modification de `rir.rs` / `interp.rs` / `run.rs` / `front.rs`.
La RVM est vérifiée ; les 2 échecs restants appartiennent au chantier AOT6
voisin et n'ont pas été touchés.

---

## 4. Limites honnêtes — ce que la RVM n'est toujours PAS

1. **Pas de JVM Java** : le format est à nous, le moteur est celui de Rose.
   On n'a emprunté que l'idée d'un fichier exécutable indépendant et, dans la
   couche v2 historique, celle d'une table de méthodes.
2. **Pas de GC traçant** : le « GC » reste le `GcPool` à liste libre + comptage
   de prêt de la partie 1. Pas de cycles (un `Gz` ne référence pas d'autre
   `Gz`), pas de générations, pas de compaction.
3. **Pas de JIT depuis `.rosebc`** : le snapshot v3 est exécuté par le moteur
   Rose ; les caches RIR/JIT sont reconstruits comme dans `rose run`, jamais
   lus depuis le fichier.
4. **Pas de threads / monitores / FFI / chargement dynamique** : une seule
   exécution à la fois, une seule pile de trames.
5. **V1/v2 conservés tels quels** : la machine à classes v2 reste limitée au
   sous-ensemble arithmétique (elle sert aux vieux `.rosebc`, pas à `rose bc`).
6. **Fichier hostile** : vérificateur sans panique, mais la RVM ne promet pas
   de limite de temps sur un programme v3 qui boucle infiniment — c'est le
   comportement normal de `rose run`, pas une sandbox de confinement.

---

## 5. Fichiers

- `rose/src/rosevm.rs` — parties 1 (v1), 2 (v2), **3 (v3, nouvelle)** + tests.
- `rose/src/main.rs` — câblage `bc` / `rvm`, aide, correction du panic CLI.
- `rose/docs/RVM_COMPLETE.md` — ce document.
- `rose/docs/RVM_JVM_COMPLETE.md` — doc historique v2 (mise à jour d'un
  avertissement en tête ; le point 9 y est marqué périmé).
