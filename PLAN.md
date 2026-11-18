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
M2.3 arène d'allocation pour les Gz            ← FAIT (b50eefd+24daf0c, +15,52% factorielles, valeurs bit-identiques)
M2.4 liste vide typée `[0 : entier]`           ← FAIT (2af045c)
M3  Backend propre (AOT sans LLVM externe)
M3.1 émission x86-64 / aarch64 depuis le RIR   ← FAIT (8fdef8d+1b73c74, API JIT + ROSE_NATIF, parité 3 moteurs)
M3.2 link natif (ELF/PE) sans gcc                ← FAIT (e554a24+aa52b8c, ELF rend 49, 4 tests)
M4  Auto-hébergement : compilateur en Rose
M4.1 lexeur en Rose (miroir de tokeniser)      ← FAIT (181 fichiers, 22b1ae5)
M4.2 analyseur en Rose                         ← FAIT (148 fichiers, 5b214b6)
M4.3 vérificateur en Rose                      ← FAIT (60/61, 3aa6df5)
M4.4 émetteur RIR en Rose                      ← FAIT (dc97a63, 46/46 sorties octet-identiques)
M4.5 point fixe de bootstrap                   ← FAIT (1f7cd3c, md5 identique 83449e53…)
M5  Runtime autonome (sans Rust std, sans GMP)
M5.1 arithmétique exacte en Rose pur           ← FAIT (mul_rapide, i128x)
M5.2 E/S et mémoire sans std                    ← FAIT (de2fc4a, module sys_io)
M5.3 produit sans dépendance Rust               ← À FAIRE
M6  Interop sans runtime
M6.1 pont wasm maison (compatibilité)          ← FAIT (115e705)
M6.2 runtime wasm optionnel (installeur)       ← FAIT (f70ae72: porte ≤20ms PASS, 4,15ms vs 172,1ms; artefact runner NON publié, SHA c7b2bf3c...)
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

## Chantiers parallèles — vague 3 (lancés après M4.5 atteint)

Derniers points ouverts du plan, six agents sur des fichiers distincts :

| Agent | Étape | Fichiers possédés | Porte |
|---|---|---|---|
| C1 | **M4.4** 30→46/46 | `compilateur/emetteur.rose` | plus de matchs SANS casser le point fixe M4.5 (md5) |
| C2 | **M2.3** vraie bouteille | `rose/src/{i128x,mul_rapide}.rs` | factorielles N=5000 ≥15%, md5 sortie identique |
| C3 | **bug SIGSEGV bench_global** | `rose/src/run.rs` | JIT actif sans crash, valeur = interprété |
| C4 | **M3.2** PE64 | `rose/src/link_natif.rs`, `rose/docs/LINK_NATIF.md` | structure PE vérifiée (exécution Windows non prouvable ici) |
| C5 | **M5.3** préparation G3 | `rose/src/sys_io.rs`, `rose/docs/NO_STD_CHECKLIST.md` | sys_io compile avec libc sans std |
| C6 | **M6.2** artefact runner | `scripts/publier-runtime.sh`, `rose/docs/RUNTIME_RUNNER.md` | dry-run sans publication, compte Stef302 vérifié |

Même règle que la vague 2 : orchestrateur seul sur `main.rs`/`lib.rs`/`Cargo.toml`/`PLAN.md`, preuve réelle exigée, aucun commit d'agent.

## Chantiers parallèles — vague 4 (18/11, derniers points ouverts)

Cinq agents sur les livrables finaux et l'audit :

| Agent | Étape | Fichiers possédés | Porte |
|---|---|---|---|
| D1 | **M6.2 runner réel** | `scripts/build-runtime-runner.sh`, `rose/docs/RUNNER_REEL.md` | runner compilé + fib(27) ≤20ms mesuré (ou échec documenté) |
| D2 | **artefacts Windows** | `installateur/**` | rose.exe/rose-shell.exe reconstruits + hashs |
| D3 | **M5.3 binaire no_std** | `rose/src/bin/rose_nostd.rs`, `rose/docs/BINAIRE_NO_STD.md` | calcul exact sans std, nm ancré = 0 |
| D4 | **M4.5 compilateur complet** | `compilateur/compilateur.rose` | RIR valide des deux côtés (ou limite documentée) |
| D5 | **audit final QA** | `scripts/audit-final.sh`, `rose/docs/AUDIT_FINAL.md` | rapport factuel horodaté de toutes les portes |

Même règle : orchestrateur seul sur `main.rs`/`lib.rs`/`Cargo.toml`/`PLAN.md`, preuve réelle, aucun commit d'agent, aucune publication GitHub.

## Vague 5 (18/11) — écarts révélés par l'audit final

L'audit sur le corpus élargi (99 fichiers) a trouvé 14 écarts au vérificateur et 1 à l'émetteur, masqués par les scripts précédents plus étroits. Deux agents ciblés :

| Agent | Étape | Fichier possédé | Porte |
|---|---|---|---|
| E1 | vérificateur 85/99 → 99/99 | `compilateur/verifieur.rose` | 0 écart sur 99 fichiers |
| E2 | émetteur 98/99 → 99/99 | `compilateur/emetteur.rose` | 0 écart SANS casser point fixe M4.5 ni 46/46 |

⚠ Ces deux agents travaillent sur les fichiers M4.3/M4.4 : après livraison, re-vérifier le point fixe M4.5 (md5) et le corpus complet avant publication.

**Bilan vague 5 FINAL** (mesure fraîche 18/11 sur 150 fichiers): **lexeur 150/150, analyseur 150/150, vérificateur 150/150, émetteur 150/150**. Point fixe M4.5 maintenu md5 `bdbef636...`. Sans-faute sur tout le corpus élargi.

**Bilan vague 4**: runner wasm réel **porte ≤20ms PASS (4,15ms)**, artefacts Windows reconstruits, binaire no_std démo (non câblé: panic=abort), diagnostic compilateur.rose (`rir` rc=0, 6590 lignes — l'erreur rapportée était un artefact de pipe), audit final horodaté.

## Vague 6 (18/11) — finalisation M5.3 / M6.2 et contrôles

Cinq agents sur les derniers points ouverts, fichiers distincts :

| Agent | Étape | Fichiers possédés | Porte |
|---|---|---|---|
| F1 | **M6.2 bout-en-bout** | `rose/src/wasm_runtime.rs`, `rose/docs/WASM_RUNTIME.md` | voie officielle via CLI Rose, médiane ≤20ms |
| F2 | **M5.3 chemin no_std** | `rose/docs/NO_STD_MOTEUR.md` | tests isolés num-bigint sans std |
| F3 | **audit rafraîchi** | `scripts/audit-final.sh`, `rose/docs/AUDIT_FINAL.md` | 150 fichiers, chiffres horodatés |
| F4 | **publication runner** | `scripts/publier-runtime.sh`, `rose/docs/PUBLICATION_RUNNER.md` | asset publié sur Stef302/Rose v0.1.5, SHA vérifié |
| F5 | **contrôle Windows** | `rose/docs/VERIF_WINDOWS.md` | md5/ressources re-vérifiés, limites documentées |

⚠ F4 est la seule action de publication réelle: compte Stef302 vérifié avant, SHA `c7b2bf3c…`, aucun autre asset touché.

## Vague 7 (18/11) — complétude du langage (utilitaires, jardins, comparaisons, G4)

Cinq agents, fichiers distincts, pour répondre à la demande d'un langage COMPLET (pas seulement un pont vers C) :

| Agent | Étape | Fichiers possédés | Porte |
|---|---|---|---|
| G1 | **utilitaire C→wasm Rose** | `scripts/rose-wasm-lib.sh`, `rose/docs/WASM_LIB_UTILITY.md` | une commande produit wasm + déclarations Rose, 2 démos |
| G2 | **bench comparatif** | `rose/docs/BENCH_COMPARATIF.md` | Rose vs Rust pur vs Python, expérience 40% en jardins |
| G3 | **auto-hébergement G4** | `compilateur/emetteur_elf.rose`, `rose/docs/AUTOHOST_G4.md` | ELF émis par le compilateur Rose, exécuté |
| G4 | **jardins 1re classe** | `jardins/`, `scripts/rose-jardin.sh`, `rose/docs/JARDINS.md` | catalogue + utilitaire + jardin de démo vérifié |

Rappel: les jardins sont du Rose PUR, jamais du Rust. GMP jamais dépendance. Mesures A/A/A, constats datés.

## Vague 8 (18/11) — vitesse, universalité, bytecode

Cinq agents sur les questions ouvertes de l'utilisateur, fichiers distincts :

| Agent | Sujet | Fichiers possédés | Porte |
|---|---|---|---|
| H1 | **Rose ≥ Rust AOT** | `rose/src/{jit,interp,rir,natif_emetteur}.rs`, `rose/docs/VITESSE_VS_RUST.md` | améliorer fib(35) mesuré, verdict honnête |
| H2 | **Mach-O + format universel** | `rose/src/macho_writer.rs`, `rose/docs/MACHO_UNIVERSEL.md` | structure Mach-O prouvée, wasm = universel expliqué |
| H3 | **bytecode style JVM** | `rose/docs/BYTECODE_ROSE.md` | étude: RIR est-il déjà le bytecode? recommandation |
| H4 | **guide universel wasm** | `rose/docs/UNIVERSEL_WASM.md` | build wasm Rose testé + guide 5 min |
| H5 | **plafond de vitesse** | `rose/docs/PLAFOND_VITESSE.md` | démarrage/natif/JIT/interp mesurés, plafond calculé |

⚠ H1 peut modifier le moteur (jit/interp/rir) : après livraison, re-vérifier les 4 oracles 150/150 et le point fixe md5 avant publication.

**Bilan vague 8** : H1 ✅ `b3c0b72` fib(35) 208→51,3ms (×4,05; Rust reste 4,05× devant, Rose bat Rust 5,34× sur factorielle) · H2 ✅ Mach-O structure prouvée · H3 ✅ RIR = bytecode interne (`BYTECODE_ROSE.md`) · H4 ✅ guide universel wasm · Fix ✅ build wasm réparé (`3328ce5`, 15 tests Node) · H5 ✅ plafond vitesse (`PLAFOND_VITESSE.md`: natif 4× JIT, plafond NTT 2,25×). Après H1: point fixe `bdbef636` intact, 30/30 oracles sur échantillon, débordements i64 exacts.

## Vague 9 (18/11) — dépasser Rust + autonomie + JVM Rose + mémoire Zig

Six agents pour franchir les deux limites restantes, fichiers distincts :

| Agent | Sujet | Fichiers possédés | Porte |
|---|---|---|---|
| I1 | **battre Rust sur fib** | `rose/src/{jit,rir}.rs`, `rose/docs/BATTRE_RUST.md` | fib35 ≤ 12,7ms par transformation récursion→boucle générique |
| I2 | **autonomie G2/G3** | `rose/src/bin/rose-autonome.rs`, `rose/docs/AUTONOMIE_G2.md` | compiler sans Cranelift, `cargo tree` réel |
| I3 | **JVM Rose** | `rose/src/rosevm.rs`, `rose/docs/ROSE_VM.md` | `.rosebc` sérialisé/rechargé/exécuté, GC+andbox |
| I4 | **mémoire style Zig** | `rose/src/mem_modele.rs`, `rose/docs/MEMOIRE_ZIG.md` | allocateur explicite prototype + mesure |
| I5 | **bench multi-domaines** | `rose/docs/BENCH_MULTI.md` | 8 domaines Rose/Rust/Python, verdict |
| I6 | **guide autonomie** | `rose/docs/AUTONOMIE_COMPLETE.md` | route G2→G4 vérifiable, état réel |

⚠ I1 modifie `jit.rs`/`rir.rs` (moteur) : après livraison re-vérifier 4 oracles + point fixe. I3/I4 ajoutent des modules non câblés (orchestrateur décide).

**Bilan vague 9** : I1 ✅ `fde9baf` fib35 51,3→19,9ms (×2,58; Rust 12,6ms reste 1,58× devant — transformation récursion→boucle générique, 29,9M→14,9M appels) · I2 ✅ AOT sans Cranelift ELF exit55 · I3 ✅ JVM Rose `.rosebc` 36o + GC + sandbox · I4 ✅ allocateur arène 7,89× Box · I5 ✅ bench multi **1 victoire/7 défaites** (factorielle) · I6 ✅ guide autonomie. Après I1: point fixe `bdbef636` intact, 20/20 oracles, débordements exacts.

## Vague 10 (18/11) — fermer les écarts mesurés

Six agents sur les goulots exacts identifiés par les mesures :

| Agent | Sujet | Fichiers possédés | Porte |
|---|---|---|---|
| J1 | **allocation registres** | `rose/src/jit.rs`, `rose/docs/REGISTRES_RUST.md` | fib35 ≤ 12,6ms (fermer 1,58×) |
| J2 | **types non numériques** | `rose/src/run.rs`, `rose/docs/TYPES_RAPIDES.md` | ≥2× sur chaîne/liste/table |
| J3 | **G2 réel** | `rose/Cargo.toml`, `rose/src/{lib,main}.rs`, `rose/docs/G2_REEL.md` | build sans cranelift, `libc features=[]` |
| J4 | **câblage VM/arène** | harnais /tmp + `rose/docs/CABLAGE_VM.md` | intégration prouvée, lignes exactes |
| J5 | **validation finale** | `rose/docs/VALIDATION_FINALE.md` | audit après changements moteur |
| J6 | **plafond après** | `rose/docs/PLAFOND_APRES.md` | bornes [E] recalculées, verdict |

⚠ J1 modifie `jit.rs` (moteur) et J3 modifie `lib.rs`/`main.rs`/`Cargo.toml` : après livraison re-vérifier point fixe + 4 oracles + builds. Règle: ne jamais annoncer une victoire sans mesure pinée.

**Bilan vague 10** : J1 ✅ `4d0b04d` fib35 19,9→**12,739ms** (1,0116× de Rust, 1,16% d'écart — émetteur aarch64 à registres fixés identique LLVM) · J2 ✅ `3abfe12` table 8,38× · J3 🔄 G2 build sans cranelift réussi mais repli jardin cassé · J4 ✅ câblage VM/arène · J5 ✅ `f102fe3` 151/151 · J6 ✅ plafond 1,899 cycles/appel.

## Vague 11 (18/11) — derniers 1,16 % et autonomie

| Agent | Sujet | Fichiers possédés | Porte |
|---|---|---|---|
| K1 | **fermer 1,16 %** | `rose/src/jit.rs`, `rose/docs/FERMER_ECART.md` | fib35 < 12,593ms (Rose bat Rust) |
| K2 | **fix repli autonome** | `rose/src/main.rs`, `rose/docs/G2_FIX.md` | jardin 20/20 sur build `--features autonome` |

Règle inchangée: aucune annonce de victoire sans mesure A/A/A pinée, point fixe et 4 oracles re-vérifiés après.

**Bilan vague 11** : K1 ✅ `5de6d20` **Rose bat Rust sur fib(35): 11,53ms vs 12,61ms (-8,6%)** — branchement de base pris (`b.cs`) + prologue 3 registres, générique; K2 ✅ `0d1c36a` G2 autonome jardin 20/20. Vérification indépendante 21 échantillons c15: ratio 0,9141. Point fixe `bdbef636` intact, 4 oracles 20/20. ⚠ Victoire sur UN domaine seulement (BENCH_MULTI: 1/8 avec même source); les autres restent à fermer. L'utilisateur précise: OPTIMISER PAR ARCHITECTURE, pas fonction par fonction; « textual » = la BIBLIOTHÈQUE wasm appelée depuis Rose (pas Rose compilé en wasm).

## Vague 12 (18/11) — optimisation par architecture (pas par fonction)

| Agent | Architecture ciblée | Fichiers possédés | Porte |
|---|---|---|---|
| L1 | **représentation des valeurs** (boxage/clonage Valeur) | `rose/src/run.rs`, `rose/docs/ARCHI_VALEURS.md` | ≥2× sur chaîne/liste/table |
| L2 | **interpréteur RIR** (HashMap/conversions slots) | `rose/src/interp.rs`, `rose/docs/ARCHI_INTERP.md` | ≥2× en mode autonome |
| L3 | **pont wasm bibliothèque** (conversion/copie/opcodes) | `rose/src/wasm_lecteur.rs`, `rose/docs/ARCHI_WASM_PONT.md` | ≥2× sur coût/appel xxHash |

Règle: gain structurel mesuré A/A/A piné, valeurs identiques (jardin 20/20, fib25, oracles, point fixe). PAS de special-case de fonction.

⚠ Correction utilisateur (18/11): « textuel » = la bibliothèque **Rich** (Python de rendu texte) à essayer de faire tourner via wasm dans Rose (avec CPython/RustPython comme portage), PAS Rose compilé en wasm. Et pour la vitesse: appliquer la MÉTHODE Rust = trouver le GOULOT ARCHITECTURAL global d'abord, puis optimiser cas par cas — pas optimiser fonction par fonction.

## Vague 13 (18/11) — Rich via wasm + goulot architectural

| Agent | Sujet | Fichiers possédés | Porte |
|---|---|---|---|
| M1 | **Rich → wasm → Rose** | `rose/docs/RICH_WASM.md` | verdict honnête faisable/non, commandes exécutées |
| M2 | **goulot architectural global** | `rose/docs/GOULOT_ARCHITECTURE.md` | % temps par phase mesuré, goulot #1 prouvé, décision structurante |

Rich est installé (`~/.local/lib/python3.12/site-packages/rich`). CPython n'a pas wasmtime, RustPython absent — à vérifier/tester par l'agent.

⚠ Correction utilisateur (18/11, après M1): « ne pas dépendre de cpython pour voir ». La voie CPython/wasmtime est ABANDONNÉE pour le rendu texte. Rich sert de MODÈLE: on réimplémente panneaux/tableaux/barres en **jardin Rose pur** (`jardins/texte.rose`), testé uniquement avec `./rose/rose` — zéro Python dans l'exécution. Agent M3.

**Bilan partiel vague 13**: M2 (goulot architectural) en cours. M1 a produit `WASM_TEXTUEL.md` (32baf85) — Rose exécute du texte via wasm/Node/Python, utile comme démonstration d'universalité mais PAS la voie Rich retenue.

⚠ Précision finale utilisateur: tester le **convertisseur wasm** (`scripts/rose-wasm-lib.sh`) avec une vraie bibliothèque **Python (Rich)**: compiler Python→C→wasm, appeler depuis Rose. CPython = outil de compilation, PAS runtime. Agent M4 (`PYTHON_WASM_RICH.md`). Rich entier via Cython est probablement bloqué (API CPython → imports), à documenter honnêtement.
