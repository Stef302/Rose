# G5_PRIMITIVES — primitives manquantes du runtime G5 (constat daté, 21/11/2026)

Sous-agent `sh6`. Dépôt `/home/clausake/Stefan/ROSE` (`Stefan/ROSE`), HEAD de départ
`b8875b6`. Deux fichiers possédés : `compilateur/emetteur_elf.rose` et ce document.
**Aucun commit, aucune publication, aucun autre fichier modifié.**
Artefacts : `/tmp/opencode/sh6_agents/`.

## 1. Choix — et pourquoi

La mission énumère cinq primitives manquantes. Après lecture de `SELF_HOST_SH3.md`
et `SELF_HOST_EXT.md` et mesure des quatre divergences résiduelles de la batterie SH4,
j'ai implémenté **trois** primitives, dans l'ordre de risque croissant pour le point fixe :

| # | Primitive | Raison du choix |
|---|---|---|
| 1 | Affichage des listes `>> [1, 2]` | Divergence SH4 `sonde_C` : l'ELF écrivait l'adresse du descripteur. Correctif **isolé dans une routine émise distincte** (`g5_emit_plist`), sans toucher à `g5_emit_print` : le dump réflexif de `emetteur.rose` passe par `g5_emit_print` (chaînes) et reste bit-à-bit identique. |
| 2 | `argument(i)` hors limites | Divergence SH4 `ioarg` : rc=139 au lieu de rc=1. Correctif **local à `g5_emit_argument`** (garde de borne + écriture stderr + `exit(1)`). |
| 3 | Flottants `>= 1e16` | Divergence SH4 `sonde_float2` : `1e+20` natif contre débordement 64 bits. Correctif **à l'émission** : le formateur natif `chaine(flottant($tok))` est appelé par l'émetteur (qui tourne sous l'interpréteur Rose), aucun calcul flottant ajouté au binaire émis. |

**Non implémentées, documentées en §6** : grands entiers `> 64 bits` (priorité 1 de
l'énoncé) et tables réelles avec valeurs / `supprimer` (priorité 2).

### Pourquoi les grands entiers n'ont PAS été attaqués en premier

C'est la primitive **la plus bloquante sémantiquement** (`2^64 -> 0`, grands décimaux
tronqués), mais elle exige un **changement d'ABI** : aujourd'hui un entier Rose est
un immédiat `n<<3` dans un mot 64 bits (`G5_ABI_VALEURS.md`, décision AB1). Passer à
des multi-limbes impose de modifier `g5_load_val`, `g5_op` (tous les opérateurs `+ - * / % ^`,
comparaisons, `neg`, `et`/`ou`), `g5_emit_ch`, `g5_emit_print`, les slots et l'ABI
d'appel. Le risque de casser le point fixe `b3ed9262…` est maximal, et la mission
impose de **ne jamais livrer une version qui le casse**. J'ai donc priorisé les
trois correctifs prouvables sans toucher à la représentation des valeurs. C'est un
choix de risque mesuré, pas une omission : l'état est documenté §6.

## 2. Méthode (mesurée, pas citée)

Pour chaque primitive, programme minimal dans `/tmp/opencode/sh6_agents/tests/`,
référence `ROSE_SANS_JIT=1` (le JIT natif a un bug d'affichage des booléens documenté
dans `SELF_HOST_EXT.md` §2, hors de ce chantier) :

```
ROSE_SANS_JIT=1 rose/rose p.rose                       > p.nat.out  2> p.nat.err
ROSE_SANS_JIT=1 rose/rose rir p.rose                   > p.rir
rose/rose compilateur/emetteur_elf.rose p.rir          > p.elf
chmod +x p.elf ; ./p.elf                               > p.elf.out  2> p.elf.err
cmp p.nat.out p.elf.out ; cmp p.nat.err p.elf.err ; comparaison des codes rc
```

Binaire natif utilisé : `./rose/rose` md5 `12eac46007cf0fe9e1aec2dff6fbf665`.

## 3. Implémentation 1 — affichage des listes

- Nouvelle routine émise `g5_emit_plist` (x0 = liste taguée, x22 = genre des
  éléments : 0 entier, 1 chaîne, 2 booléen). Elle écrit `[`, puis chaque élément
  séparé par `, `, puis `]`, puis le saut de ligne final de `>>`.
- Conversion décimale et écriture des chaînes **dupliquées** dans cette routine :
  elle n'appelle pas `g5_emit_print`, donc le flux réflexif est inchangé.
- Descripteur de liste lu conformément à SH3 : `[tag=2][len][cap][data_ptr]`,
  élément i à `data_ptr + 8*i` (trois `add x2,x2,x2` après `add x2,x26,x26`).
- LR sauvegardé dans x11 (la routine est appelée par `BL` et `g5_make_str` écrase
  x0/x1/x2/x9/x12/x13/x14, mais pas x22/x23…x27).
- Détection du genre : table d'émission `$g5_slist` (`f|slot` → genre) renseignée
  lors de `affect slot`, plus analyse directe d'un littéral `(lst ...)` dans
  `g5_op`. `g5_list_kind` inspecte le premier élément (`s`→chaîne, `b`→booléen).

### Preuves natives vs ELF (octet pour octet)

| Test | Natif | ELF avant | ELF après |
|---|---|---|---|
| `liste.rose` `>> [1, 2]` / `>> [10, 20, 30]` | `[1, 2]` / `[10, 20, 30]` | `201328640` / `201328652` | identique (cmp=0) |
| `listvar.rose` `>> $l` (`l=[1,2,3]`) | `[1, 2, 3]` | `201328664` | identique |
| `listbool.rose` `>> [vrai, faux]` | `[vrai, faux]` | adresse | identique |
| `liststr.rose` `>> ["a", "bb", "ccc"]`, `>> ["", "x"]` | `[a, bb, ccc]` / `[, x]` | adresses | identique |
| `listneg.rose` `>> [-5, 0, 42]` | `[-5, 0, 42]` | adresse | identique |
| `listgrow.rose` (3 `joindre`) | `[1, 2, 3]` | adresse | identique |
| `listbig.rose` (12 éléments) | `[1, 2, …, 12]` | adresse | identique |

Trois défauts intermédiaires trouvés et corrigés par gdb avant d'obtenir `cmp=0` :
(1) le genre était passé dans x1, clobberé par `g5_puts` → déplacé en x22 ;
(2) l'adressement multipliait tout le pointeur par 8 au lieu de l'index seul ;
(3) le `]` était écrit sans saut de ligne final.

## 4. Implémentation 2 — `argument(i)` hors limites

`g5_emit_argument` lit désormais `argc` brut à `0x60000010`, en soustrait 1
(argc utilisateur) et compare à l'index `i` désétiqueté. Hors limites
(`i < 0` ou `i >= argc`), l'ELF écrit sur **fd 2** :

```
Rose : erreur — index hors des limites des arguments\n
```

puis exécute `exit(1)`. Le tiret cadratin est écrit en trois octets UTF‑8
explicites (`e2 80 94`) : un premier essai avec `g5_make_str` donnait `0x14` suivi
de NUL de bourrage (mesuré par `xxd`).

**Piège JIT** : une première version ajoutait un 6ᵉ paramètre à `g5_op` pour
transmettre le label de la routine de liste ; le JIT natif a paniqué
(`cranelift-codegen abi.rs:1831`, `left: 6 right: 5`). La signature est restée à
5 paramètres et le label est lu depuis la globale d'émission `$g5_plist_l`.
L'émetteur tourne donc toujours sous JIT par défaut, comme la chaîne de point fixe
l'exige.

### Preuves natives vs ELF

| Test | Natif rc / stdout / stderr | ELF avant | ELF après |
|---|---|---|---|
| `arg1.rose` `>> argument(0)` sans argument | 1 / vide / message | 139 / vide / vide | 1 / idem octet pour octet |
| `arg2.rose` `>> nb_arguments()` puis `>> argument(1)` | 1 / `0\n` / message | 139 / `0\n` / vide | 1 / idem |
| `argout.rose` (cas SH4) | 1 / `0\n` / message | 139 | 1 / idem |
| `arg1.elf AAA` (argument valide) | 0 / `AAA\n` | — | 0 / `AAA\n` |

## 5. Implémentation 3 — flottants `>= 1e16`

Dans `g5_op`, un littéral `flottant <tok>` dont le texte fait au moins 17
caractères, sans `.` ni `/`, est sérialisé à l'émission par
`g5_make_str(chaine(flottant($tok)))`. Le formateur de Rose produit exactement la
notation attendue (`1e+16`, `1e+20`, `-1e+20`, `1.2345678901234567e+19`).

### Preuves natives vs ELF

| Test | Natif | ELF avant | ELF après |
|---|---|---|---|
| `floatbig.rose` | `1e+20 / 1.5 / 2.[3]` | `848750603811160064 / 1.5 / 2.[3]` | identique |
| `floatmore.rose` (7 lignes) | `1e+16 / 1e+20 / -1e+20 / 1.2345678901234567e+19 / 1000000000000000 / 1.5 / 2.[3]` | débordements | identique (cmp=0) |

## 6. Validation finale (état stable `b8f73bb7…`)

| Contrôle | Résultat mesuré |
|---|---|
| `rose verif compilateur/emetteur_elf.rose` | **`vérifié`** |
| Chaîne de réflexivité (1re salve, source `e815f20f…`) | RIR `b3ed9262dd54776a6dbd07bc12368d0e` (263 520 o) ; ELF rc=0, 536 220 o, `4c0b67f3010aa6b0790cb0b737d40a31` ; dump rc=0, `b3ed9262…` ; `cmp` = 0 |
| Chaîne de réflexivité **×2**, salve finale, source `b8f73bb7bc5a138a5869b01560701d2c` | RIR `b3ed9262…` (263 520 o) ; émission A et B : rc=0, **546 668 o**, md5 **`c28aa97dc46a00645034bc646300e785`** identique ; exécutions A et B : rc=0, dump `b3ed9262…` ; `cmpA=0`, `cmpB=0`. Log : `/tmp/opencode/sh6_agents/pointfixe3.log` |
| Tests SH6 (13 programmes) | **13/13 conformes** (stdout, stderr et rc) |
| Batterie 43 programmes reconstruite | **39/43 conformes**, 0 erreur d'émission |
| `cargo test --release` (`CARGO_BUILD_JOBS=2`) | **rc=0, 515 passés, 0 échec** (258+242+6+8+1 ; 3+3 ignorés) |
| Jardins `./scripts/rose-jardin.sh verif-tous` | **9/10 valides**, rc=1 : seul `jardins/gui_native.rose` est rejeté (« marqueur Rust/FFI interdit »), contenu de jardin étranger à ce chantier |
| `fib25` natif vs ELF | **75025 / 75025** |

Les 4 divergences résiduelles de la batterie 43 sont : `b13`/`b30` (tables avec
valeurs — §7.2), `b31` (`2^63-1` — §7.1) et `b39` (`10/3` natif `3.[3]` contre `3` :
division rationnelle calculée, hors périmètre des 5 primitives de l'énoncé).

## 7. Reste (non implémenté, mesuré)

1. **Grands entiers `> 64 bits`** : `grand.rose` donne natif
   `18446744073709551616 / 18446744073709551616 / 123456789012345678901234567890 / 2147483648`
   et ELF `0 / 0 / 248789718555101906 / 2147483648`. Nécessite un changement d'ABI
   (§1). `2^64` reste 0.
2. **Tables réelles** : `inserer(t,c,v)` stocke encore uniquement `c` (la valeur `v`
   est ignorée), `t["a"]` n'est pas une lecture de valeur, `supprimer` est un stub.
   Mesure native de référence (`tabexplore.rose`) :
   `vrai / 10 / 2 / faux / 1`. Mesure ELF : `vrai / <adresse> / 2 / faux / 1`.
3. **Concurrence de fichier** : pendant ce chantier, `compilateur/emetteur_elf.rose`
   a été modifié par un autre agent (ajouts SH7/SH9, puis un état intermédiaire où
   `g5_gnam` était référencé avant déclaration → `rose verif` en échec, ensuite corrigé).
   Le md5 du source a changé cinq fois (`e668fd4…` → `e815f20…` → `80ea6a7…` →
   `bc2d846…` → `b8f73bb…` → `3a9ccb2…`). Les trois correctifs SH6 décrits ci-dessus
   étaient présents dans **tous** ces états ; le point fixe ×2 a été joué sur
   `b8f73bb…`, et les 13 tests SH6 ont été rejoués avec succès sur `3a9ccb2…`.
   Ce changement concurrent est signalé à l'orchestrateur : la validation finale
   doit être re-jouée si le source évolue encore.
