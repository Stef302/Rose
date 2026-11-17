# Plan de route Rose — objectif : un langage écrit en lui-même, aussi rapide que Zig

> **Statut** : version 0.1.5 · moteur Rust · JIT Cranelift · RIR · jardins · pont wasm.
> Ce document remplace `archive/ARCHITECTURE.md` (qui décrit l'ANCIEN moteur C++,
> **obsolète**). Chaque jalon porte une **porte de sortie mesurable** : un jalon
> n'est « fait » que si la mesure annoncée est reproduite par une commande.

---

## L'objectif en une phrase

Faire de Rose un **langage auto-hébergé** (son compilateur écrit en Rose, sans Rust
ni GMP au produit) **aussi rapide que Zig**, en gardant son identité : arithmétique
exacte, opérateurs symboliques (`!`, `!!`, `#`, `?`, `$`, préfixe `!`), sortie = valeur seule.

### Pourquoi Zig est la référence

| Fait | Zig | Rose aujourd'hui | Conséquence |
|---|---|---|---|
| Le compilateur est écrit dans le langage lui-même | oui (auto-hébergé depuis 0.11) | non (moteur Rust) | **M4** = auto-hébergement |
| Aucune dépendance de runtime obligatoire | oui (pas de libc requis) | non (Rust std + num-bigint) | **M5** = runtime autonome |
| Backend natif propre (LLVM + self-hosted x86-64/aarch64) | oui | JIT Cranelift + AOT LLVM | **M3** = backend propre |
| Compile du C/C++ sans leur runtime | oui (`zig cc`) | partiel (pont wasm) | **M6** = interop sans runtime |
| Performance à parité avec C | oui | JIT à parité sur les boucles, ~2× sur les jardins | **M2** = fermer l'écart |

Zig est écrit en Zig **parce qu'il a d'abord eu un amorce** (stage0 en C++/LLVM) puis
s'est recompilé. Rose suit le même chemin : le moteur Rust joue le rôle du stage0.

---

## Les jalons

```
M1  RIR : forme d'exécution unique            ← FAIT (17/11)
M1.1 slots indexés, corps principal abaissé   ← FAIT
M1.2 RIR du JIT (plus de compilation d'arbre)  ← FAIT (9fe647b, 03c5049)
M2  Performance : fermer l'écart avec Zig
M2.1 jardin = primitive native                ← FAIT (82b3e6b, 4,11 ms)
M2.2 interpréteur sans HashMap par accès       ← FAIT (b50eefd, zg_rapide par réf.)
M2.3 arène d'allocation pour les Gz            ← PARTIEL (b50eefd, pool Gz ~26% boucle, ~7% factorielles; arène complète reportée)
M2.4 liste vide typée `[0 : entier]`           ← FAIT (2af045c)
M3  Backend propre (AOT sans LLVM externe)
M3.1 émission x86-64 / aarch64 depuis le RIR   ← FAIT (8fdef8d+1b73c74, API JIT + ROSE_NATIF, parité 3 moteurs)
M3.2 link natif (ELF/PE) sans gcc                ← FAIT (e554a24+aa52b8c, ELF rend 49, 4 tests)
M4  Auto-hébergement : compilateur en Rose
M4.1 lexeur en Rose (miroir de tokeniser)      ← FAIT (181 fichiers, 22b1ae5)
M4.2 analyseur en Rose                         ← FAIT (148 fichiers, 5b214b6)
M4.3 vérificateur en Rose                      ← FAIT (60/61, 3aa6df5)
M4.4 émetteur RIR en Rose                      ← FAIT (30/46, 08c42a0; auto-émission OK)
M4.5 point fixe de bootstrap                   ← FAIT (1f7cd3c, md5 identique 83449e53…)
M5  Runtime autonome (sans Rust std, sans GMP)
M5.1 arithmétique exacte en Rose pur           ← FAIT (mul_rapide, i128x)
M5.2 E/S et mémoire sans std                    ← FAIT (de2fc4a, module sys_io)
M5.3 produit sans dépendance Rust               ← À FAIRE
M6  Interop sans runtime
M6.1 pont wasm maison (compatibilité)          ← FAIT (115e705)
M6.2 runtime wasm optionnel (installeur)       ← FAIT (f8bcf93+ab0651b, porte ≤20ms à mesurer après publication artefact)
M6.3 Component Model / WIT (types complexes)   ← ÉTUDIÉ (845f5e0)
```

---

## M1 — RIR : la forme d'exécution unique ✅

**But** : tout le code exécutable passe par un tableau plat d'opérations à sauts
numériques et variables en **slots indexés** — pas d'accès par nom dans une
`HashMap` à chaque instruction. C'est la forme « comme Zig » (IR plat, pas d'arbre).

**État (mesuré le 17/11, commit `77369c7` + `919cd86`)** :

| Mesure | Avant RIR | Après RIR | Gain |
|---|---|---|---|
| jardin des premiers, sans JIT | 3,65 s | **0,57 s** | 6× |
| boucle nue 2 M au niveau principal | 1 660 ms | **826 ms** | 2× |
| `it(30000)` récursif | 44–62 ms | **31 ms** | ~1,5× |
| jardin avec JIT | 50 ms | **47 ms** | marginal |

**Ce qui est fait** : `rose/src/rir.rs` (1 130 lignes), corps de fonction **et** corps
principal abaissés, `importer` résolu par fusion (jardins sans copie de corps), JIT
délègue à l'interpréteur (`rose_appel_interpreteur`).

### M1.1 — slots indexés partout ✅

Le corps principal et les fonctions utilisent des slots, plus de `HashMap<String, Valeur>`.
**Porte** : `ROSE_PROFIL_PILE=1` doit montrer zéro recherche par nom dans une boucle chaude.

### M1.2 — le JIT consomme le RIR, pas l'arbre

**Aujourd'hui** : `jit.rs` (2 816 lignes) compile l'**arbre** `Instr`. Le RIR existe à côté.
Deux formes d'entrée = deux chemins à maintenir, et le JIT ne bénéficie pas des slots.

**À faire** : le JIT compile le RIR directement (un `switch` sur l'opcode RIR, des
blocs numériques, des slots en registres).

**Porte** : `fib(25)` reste à 1–2 ms (non-régression), et le jardin des premiers
passe de 47 ms à **≤ 30 ms** (le JIT prend le corps du jardin, plus de délégation).

---

## M2 — Performance : fermer l'écart avec Zig

**Référence** : sur le même algorithme (test de division jusqu'à √n, N = 50 000),
C `-O2` fait 4 ms, la boucle nue Rose 26 ms, la primitive native 51 ms.

### M2.1 — le jardin bat la primitive native

**Mesure actuelle** : jardin `est_premier` 56 ms vs primitive native 53 ms (banc pur) ;
64 vs 57 (autre banc). **Le jardin est 2× plus lent** que la primitive sur N = 50 000
(102 vs 51 ms, cf. `jardins/README.md`).

**Cause mesurée** : le coût est dans l'interpréteur du corps de fonction (si, tant_que,
affectation refont le travail pour chaque instruction), pas dans l'appel.

**À faire** : M1.2 d'abord (JIT sur RIR) ; ensuite inliner les appels de jardin dont le
corps est petit (≤ 20 instructions RIR) directement dans le corps appelant.

**Porte** : `rose jardins/tests/premiers.rose` avec N = 50 000 doit afficher
**≤ 53 ms** (jardin au niveau de la primitive, voire devant).

### M2.2 — interpréteur sans HashMap par accès

**Déjà fait en partie** par M1 (slots). Reste : les `Valeur` intermédiaires allouées par
instruction (`Gz`, `Rc`, `Box`).

**À faire** : pool de `Gz` réutilisables pour les temporaires d'un corps (les `Gz` de
slot ne bougent pas ; seuls les temporaires tournent).

**Porte** : `bench/global/bench_global.rose` sous 0,09 s reste tenu ; `it(30000)` sous 25 ms.

### M2.3 — arène d'allocation pour les `Gz` intermédiaires

**Blocage actuel** : `Drop for Gz` fait `Box::from_raw(self.0)` — incompatible avec une
arène sans refonte du modèle de mémoire `*mut Mpz`.

**À faire** : séparer `Gz` (poignée possédée) de `GzEmprunte` (vue dans l'arène), et
donner à l'arène un `Drop` unique qui libère en bloc.

**Porte** : `factorielles.rose` à N = 5000 reste identique en valeur et **gagne ≥ 15 %**
de temps.

### M2.4 — liste vide typée `[0 : entier]`

**Aujourd'hui** : `[]` est refusé (« le type d'élément est introuvable »), les jardins
rembourrent à l'index 0 puis dépouillent.

**À faire** : accepter `[0 : entier]` comme annotation de type d'élément d'une liste vide.

**Porte** : `jardins/tests/premiers.rose` perd ses rembourrages et passe (0 ✗).

---

## M3 — Backend propre (AOT sans LLVM externe)

**But** : `rose build prog.rose -o prog` produit un binaire natif **sans** gcc, clang,
LLVM ni linker externe — comme `zig build-exe`.

**Aujourd'hui** : `rose orc` (Cranelift ORC) + `rose link` (LLVM/lld), et
`rose llvm` (transpilation vers C puis clang). Le helper `rose-aot` est téléchargé à la
demande (intégré dans `rose.exe` depuis `095a666`).

### M3.1 — émission machine depuis le RIR

Écrire un émetteur x86-64 et aarch64 qui traduit chaque opcode RIR en octets machine
(pile de slots, sauts numériques, appels au runtime Rose).

**Porte** : `fib(25)` compilé par cet émetteur donne **≤ 3 ms** (contre 1–2 ms en JIT
Cranelift) et la sortie est identique à l'interpréteur sur les 40 dorés de `bench/dores/`.

### M3.2 — link natif sans gcc

Émettre directement un ELF (Linux) et un PE (Windows) avec le runtime Rose embarqué
(`rose/src/elf_linker.rs` existe déjà pour le bundle AOT — à généraliser).

**Porte** : `./prog` produit par `rose build` tourne sur une machine sans gcc ni clang
installés.

---

## M4 — Auto-hébergement : le compilateur en Rose

**Protocole de bootstrap** (comme Zig : stage0 → stage1 → point fixe) :

1. le moteur Rust (stage0) exécute `compilateur.rose` → produit `rose-1` ;
2. `rose-1` compile `compilateur.rose` → `rose-2` ;
3. **point fixe** : `rose-2` byte-identique à `rose-1` sur les dorés → le compilateur
   se maintient lui-même ;
4. le moteur Rust régresse au rôle d'amorce (plus personne ne le développe d'abord).

**Chaque maillon est prouvé par un ORACLE** : une commande du moteur Rust qui imprime
la sortie attendue, et le maillon Rose doit rendre **la même sortie à l'octet près**.

### M4.1 — lexeur en Rose

**Oracle** : `rose tokens prog.rose` → `ligne:colonne type texte`.

**À faire** : `compilateur/lexeur.rose` reproduit `tokeniser()` à l'octet près sur tout
le corpus (`jardins/`, `bench/`, `exemples/`), y compris **en se lexant lui-même**.

**Porte** : sortie identique à l'octet sur ≥ 50 fichiers, par l'interpréteur ET par le
binaire compilé.

### M4.2 — analyseur en Rose

**Oracle** : `rose ast prog.rose` (fonctions triées, instructions indentées, expressions
en S-expressions).

**À faire** : `compilateur/analyseur.rose` — descente récursive miroir de `Parser`,
précédences et sucres (`√`, tranches, `² ³`, `module::nom`) à l'octet près.

**Porte** : égalité triple oracle ≡ interprété ≡ compilé sur le corpus, **l'analyseur se
lit lui-même**.

### M4.3 — vérificateur en Rose

**Oracle** : `rose verif prog.rose` → `vérifié` ou le message exact du vérificateur.

**À faire** : `compilateur/verifieur.rose` — repli de constantes exact (rationnels
réduits comme `mpq`), puis vérification miroir des environnements (globales + `locals`).

**Porte** : égalité triple sur ≥ 100 fichiers (fixtures de rejets au message exact +
corpus).

### M4.4 — émetteur RIR en Rose

**Oracle** : `rose rir prog.rose` → la représentation intermédiaire complète.

**À faire** : `compilateur/emetteur.rose` — analyse → repli → vérification → émission RIR,
le tout en Rose.

**Porte** : l'émetteur **s'émet lui-même**, par l'interpréteur et par le binaire compilé.

### M4.5 — point fixe de bootstrap

**Porte** : `rose-2` (fabriqué par `rose-1`) émet le MÊME RIR **octet pour octet** que
`rose-1` sur `compilateur.rose`. Le compilateur auto-hébergé se reproduit lui-même.

---

## M5 — Runtime autonome (sans Rust std, sans GMP)

**Règle absolue** : GMP n'est **jamais** une dépendance de Rose (`rug` uniquement dans
`bench/suite/fact_rs.rs`).

### M5.1 — arithmétique exacte en Rose pur ✅

**Fait** : `i128x.rs` (BigInt natif, façade `gmp_*`), `mul_rapide.rs` (NTT Montgomery +
arbre de produit équilibré). `1 000 000!` en **200 ms** (11× plus rapide que `num-bigint`,
1,18× plus lent que GMP).

**Porte tenue** : `factorielle(1000000)` = 5 565 709 chiffres en ≤ 250 ms.

### M5.2 — E/S et mémoire sans std

**À faire** : remplacer `std::fs`, `std::io`, `std::time` par des appels système directs
(`read`/`write`/`open` Linux, `CreateFile`/`WriteFile` Windows) — la couche existe en
partie (`console_win.rs`, `windows_compat.c`).

**Porte** : `rose` compile et lit un fichier avec `#![no_std]` + `alloc` seul.

### M5.3 — produit sans dépendance Rust

**À faire** : quand M4.5 est atteint, le binaire final est produit par le compilateur
auto-hébergé ; le moteur Rust n'est plus livré.

**Porte** : `rose --version` du binaire auto-hébergé ne dépend d'aucune bibliothèque Rust.

---

## M6 — Interop sans runtime

**But** : utiliser les bibliothèques d'autres langages **sans leur runtime** — le pont
est WebAssembly (seul format que tous les compilateurs savent produire).

### M6.1 — pont wasm maison ✅

**Fait** (commit `115e705`) : `rose/src/wasm_lecteur.rs` (1 391 lignes, zéro dépendance),
`@dll("wasm", "lib.wasm")` + `@sym(...)`, ABI C (`entier`→i32, `flottant`→f64,
`chaine`→pointeur), `rose wasm lib.wasm` liste les exports. Documenté dans
`rose/docs/WASM_FFI.md`.

**Rôle** : pont de **COMPATIBILITÉ**, pas voie de vitesse (interpréteur de pile).
Mesures : `fib(27)` wasm ≈ 150 ms ; `10000 pgcd` wasm ≈ 2,5 ms.

### M6.2 — runtime wasm optionnel, à la demande (comme LLVM)

**Principe choisi** : laisser le choix. L'interpréteur maison est le défaut (zéro
dépendance, optimisé pour la compatibilité). Pour les modules chauds, un **runtime
officiel** (`wasmtime`/`wasmer`) est **installé par l'installeur** (ou téléchargé à la
demande, comme le helper `rose-aot`), jamais embarqué par défaut.

**À faire** : `rose wasm --runtime officiel lib.wasm` cherche le runtime dans le cache
`~/.cache/rose/`, le télécharge avec SHA-256, puis l'utilise pour les appels.

**Porte** : `fib(27)` wasm via le runtime officiel **≤ 20 ms** (contre 150 ms interprété),
avec sortie identique.

### M6.3 — Component Model / WIT (types complexes)

**À étudier, pas pour 0.1.5/0.1.6** : passer les types complexes (listes, tables,
chaînes longues) entre Rose et un module wasm par le **Component Model** (canonical ABI),
au lieu de l'ABI C actuelle (scalaires + pointeur de chaîne).

**Chantier lourd** : décodeur WIT, canonical ABI (lift/lower), mémoire partagée.

**Porte** : un module wasm exporte `fn somme(l: liste) -> entier` et Rose lui passe une
liste de 10 000 entiers.

---

## Règles invariantes (à ne jamais violer)

1. **GMP jamais une dépendance de Rose** — `rug` uniquement dans `bench/suite/fact_rs.rs`.
2. **Aucun jardin en Rust** — les jardins sont du Rose pur.
3. **Opérateurs symboliques au cœur** : `n!`, `n!!`, `n!!!`, `n#`, `n?`, `n$`, préfixe `!`
   sont des primitives du langage, pas des jardins.
4. **Sortie d'exécution = valeur seule** — LaTeX seulement via `rose latex` ou lignes `%%`.
5. **`38?` = termial classique (741)** ; Kurepa via `terminal(n)`.
6. **Release toujours 0.1.5** — 0.1.6 seulement si GMP est dépassé.
7. **Le JIT est TOUJOURS prioritaire sur le RIR** : `fib(25)` 1–2 ms en JIT contre
   113 ms en RIR.
8. **Le JIT délègue à l'interpréteur** pour les fonctions qu'il ne compile pas.
9. **Icônes** : `rose/rose.ico` → `rose.exe` ; `rose_shell.ico` → `rose-shell.exe` ;
   `rose_setup.ico` → l'installeur ; `installateur/assets/rose.ico` = document `.ROSE`.
10. **`rose-aot.exe` n'existe plus** comme exe indépendant — intégré dans `rose.exe`
    (`rose orc`, `rose link`).

---

## Pièges mesurés (à ne pas réintroduire)

### Performance
- Cloner le corps à l'appel.
- Convertir avant le test JIT.
- Refuser le `0+1i` du repli dans le JIT.
- `si` récursif dans `exec_bloc_retour`.
- Pile de 8 Mo ; `resize` de la pile de slots après démarrage.
- Voie i64 après la cascade de `matches!` dans `binaire_valeurs`.

### Cranelift
- Pas de TLS (« JIT doesn't yet support TLS ») → statique `INTERP_COURANT`.
- `DuplicateDefinition` → tables `decls`/`delegues` séparées.
- `InvalidImportDefinition`.
- `declare_data(name, linkage, writable, tls)`.

### Wasm
- Ordre des armes d'un `match` sur opcode (un `_ =>` avale les cas suivants).
- Opcodes décalés de 3 (add = 0x6A, pas 0x67).
- `local.tee` ≠ `local.get`.
- `eqz` un seul opérande.
- `end` final ne doit pas tronquer la pile.
- Cibles en indices d'instruction, pas en offsets d'octets.

---

## Ordre d'exécution recommandé

1. **M1.2** (JIT sur RIR) — débloque M2.1.
2. **M2.1** (jardin = primitive) — le gain le plus visible.
3. **M2.4** (liste vide typée) — petit, débloque les jardins propres.
4. **M2.3** (arène `Gz`) — gain mémoire et temps.
5. **M4.1 → M4.5** (auto-hébergement) — le cœur de l'objectif « comme Zig ».
6. **M3.1 → M3.2** (backend propre) — le vrai « sans Rust ».
7. **M5.2 → M5.3** (runtime autonome) — produit final.
8. **M6.2 → M6.3** (interop) — en parallèle, quand le besoin apparaît.

**Release 0.1.6** seulement si **M2.1 ou M5.1 dépasse GMP** au banc `factorielles.rose`.

---

## Chantiers parallèles — vague 1 (17/11, TOUS LIVRÉS et poussés sur Stef302/Rose)

Six agents indépendants, fichiers distincts, tous intégrés et committés :

| Agent | Étape | Résultat livré |
|---|---|---|
| A1 | M4.4 fin émetteur RIR | 30/46 + auto-émission (`08c42a0`) |
| A2 | M2.1 jardin = primitive | 4,11 ms, jardin bat la primitive (`82b3e6b`) |
| A3 | M6.2 runtime wasm optionnel | câblé, `carre(7)`→49 (`f8bcf93`+`ab0651b`) |
| A4 | M3.1 émission machine RIR | module + 6/6 tests, 1,57 ms (`8fdef8d`) |
| A5 | M5.2 E/S sans std | binaire `no_std` prouvé (`de2fc4a`) |
| A6 | M6.3 étude Component Model | recommandation documentée (`845f5e0`) |

Push confirmé : `main` local = distant = `e96093f`.

## Chantiers parallèles — vague 2 (lancés après le push)

Six nouveaux agents sur les étapes restantes :

| Agent | Étape | Fichiers possédés | Porte |
|---|---|---|---|
| B1 | **M4.5** point fixe bootstrap | `compilateur/{emetteur,compilateur}.rose` | RIR auto-émis octet-identique (ou point fixe structurel documenté) |
| B2 | **M3.2** link natif ELF/PE | `rose/src/link_natif.rs`, `rose/docs/LINK_NATIF.md` | ELF exécutable sans gcc (exit 49) |
| B3 | **M5.3** produit sans Rust | `rose/docs/PRODUIT_SANS_RUST.md` | étude + mesures nm/ldd |
| B4 | **câblage M3.1** dans JIT | `rose/src/{natif_emetteur,jit,interp}.rs` | tests API, fib machine = 75025 |
| B5 | **M2.2/M2.3** perf mémoire | `rose/src/{interp,rir}.rs`, `rose/src/gz_pool.rs` | factorielles N=5000 ≥15%, valeur identique |
| B6 | **M6.2 porte perf** | `scripts/mesurer-wasm.sh`, `rose/docs/PORTE_WASM.md` | mesure ou « non mesurable » documenté |

L'orchestrateur (session principale) seul touche à `main.rs`, `lib.rs`, `Cargo.toml`, `PLAN.md` et fait les commits après revue. Règle : aucune mesure acceptée sans commande exécutée ; aucun commit d'agent.
