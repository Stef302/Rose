# Capacités réelles et théoriques de Rose

> Document de synthèse pour le rapport à l'utilisateur. Rédigé le 18/11/2026.
> Sources : `PLAN.md`, `rose/docs/AUDIT_FINAL.md`, `NATIF.md`, `LINK_NATIF.md`,
> `WASM_RUNTIME.md`, `WASM_FFI.md`, `COMPONENT_MODEL.md`, `PRODUIT_SANS_RUST.md`,
> `NO_STD_MOTEUR.md`, `NO_STD_CHECKLIST.md`, `BINAIRE_NO_STD.md`, et les fichiers
> `rose/src/{rir,jit,i128x,mul_rapide,wasm_lecteur,natif_emetteur,link_natif}.rs`.

**Convention de lecture — importante.**

- **PROUVÉ** = une commande a été exécutée, ou un document vérifié la rapporte avec sa date. Je cite la source et la date.
- **THÉORIQUE** = un plafond permis par l'architecture. Ce n'est **pas** une promesse, ni une porte franchie.
- Trois chronomètres distincts, à ne jamais mélanger :
  1. l'exécution d'un **programme Rose** par le moteur ;
  2. le temps de **compilation / d'émission machine** du moteur ;
  3. le temps d'un **appel à une bibliothèque externe** via WebAssembly.
  Un chiffre de la catégorie 3 ne dit rien de la vitesse du langage Rose.

---

## 1. Ce que Rose fait vraiment aujourd'hui (constats datés)

### 1.1 Exécution

Rose s'exécute aujourd'hui par trois chemins (skill `rose-dev`, `PLAN.md`) :
l'interpréteur/RIR (`ROSE_SANS_JIT=1` pour forcer l'interpréteur pur), le JIT
Cranelift (chemin par défaut), et l'AOT LLVM via un helper téléchargé à la
demande (`rose orc`, `rose link`). Le code source le dit : `rir.rs:1-28` décrit
une « représentation intermédiaire linéaire » à slots numériques et sauts
indexés ; `jit.rs:1-13` décrit un JIT Cranelift avec repli transparent sur
l'interpréteur pour tout ce qui sort du sous-ensemble (chaînes, listes, FFI…).

Le résultat d'un programme est la **valeur seule**, pas une formule LaTeX
(`PLAN.md`, règle invariante 4).

### 1.2 Calcul exact illimité

`i128x.rs:1-4` : « Entiers : `num_bigint::BigInt` (illimité, pur Rust) ». Aucune
borne logicielle à 64 bits, aucun débordement silencieux. La façade `gmp_*` est
une API maison sur `BigInt` — **pas** un appel à GMP.

`mul_rapide.rs:1-20` : multiplication rapide maison. Écolage, Karatsuba,
Toom-3, puis NTT (transformée de Fourier numérique) au-delà d'un seuil calibré
par mesure. Le code actuel (`mul_rapide.rs:507-521`) utilise **2 modules premiers
de 62 bits** + théorème des restes chinois, et non plus 3 modules de 30 bits.

Chiffre PROUVÉ (`PLAN.md:247-250`, jalon M5.1, état 18/11) : `1 000 000!` =
**5 565 709 chiffres en 200 ms**, soit 11× plus rapide que `num-bigint` et
1,18× plus lent que GMP. La porte était ≤ 250 ms. C'est une mesure du moteur
arithmétique, pas du JIT ni du frontal.

### 1.3 Opérateurs symboliques au cœur

`PLAN.md`, règle invariante 3 : `n!`, `n!!`, `n!!!`, `n#`, `n?`, `n$` et le
préfixe `!` sont des **primitives du langage**, pas des jardins. Le skill
`rose-dev` précise les sens et les pièges : `7!!` = 105 (multifactorielle, pas
`(7!)!`), `!5` = 44 (dérangements), `38?` = 741 (termial classique), `10#` =
210 (primorielle), `3$` = 12 (super-factorielle). Les jardins, eux, sont du
Rose pur : zéro fichier `.rs` sous `jardins/` (audit 18/11, `AUDIT_FINAL.md:115`).

### 1.4 Quatre oracles 150/150 et point fixe du compilateur

Mesure PROUVÉE du 18/11/2026, `bash scripts/audit-final.sh` exécuté de
11:05:36 à 11:09:07 (`AUDIT_FINAL.md:3-13, 29-54`) :

| Oracle | Résultat |
|---|---|
| M4.1 lexeur (`rose tokens`) | **150 match / 0 écart** |
| M4.2 analyseur (`rose ast`) | **150 match / 0 écart** |
| M4.3 vérificateur (`rose verif`) | **150 match / 0 écart** |
| M4.4 émetteur RIR (`rose rir`) | **150 match / 0 écart** |

Corpus : 150 fichiers `.rose` (examples 6 + jardins 2 + bench 41 +
rose-cpp/tests 101), comparaison **octet pour octet**. Les cinq composants
(lexeur, analyseur, vérificateur, émetteur, compilateur) s'auto-vérifient.

Point fixe M4.5 (`AUDIT_FINAL.md:51-54, 157`) : l'émetteur s'auto-émit avec un
md5 identique des deux côtés —
`bdbef63681608a0b22a5637539e9d738`. C'est un **constat daté**, pas une
consigne ; les valeurs plus anciennes (`c81b9920…`, `83449e53…`) sont périmées.

Autres verts du même audit : jardin 20/20, `fib(25) = 75025`, `bench_global`
rc = 0 ; `cargo test` : 7 tests `link_natif`, 6 tests `natif_emetteur`,
144 tests du binaire `rose`, tous verts.

### 1.5 Performance (chiffres mesurés, avec leur portée)

| Mesure | Valeur | Date / source | Catégorie |
|---|---|---|---|
| Jardin (jalon M2.1) | **4,11 ms** ; le jardin bat la primitive native | 17/11, `PLAN.md:38,377` (commit `82b3e6b`) | exécution Rose |
| Jardin sans JIT : avant → après RIR | 3,65 s → **0,57 s** (6×) | 17/11, `PLAN.md:71-76` | exécution Rose |
| Boucle nue 2 M au niveau principal | 1 660 → **826 ms** (2×) | 17/11, `PLAN.md:74` | exécution Rose |
| `fib(25)` en JIT | **1–2 ms** (contre 113 ms en RIR pur) | `PLAN.md`, règle invariante 7 | exécution Rose |
| `fib(25)` émis par le backend machine | **1,57 ms**, 268 octets émis | 17/11, `NATIF.md:170-185`, harnais isolé release | émission + exécution |
| Factorielles N = 5000, gain de l'arène `Gz` | **+15,52 %**, valeurs bit-identiques | `PLAN.md:40` (commits `b50eefd+24daf0c`) | moteur arithmétique |
| Factorielles N = 5000, audit frais | 0,12 / 0,10 / 0,10 s (processus), `n!` 1–2 ms, 16 326 chiffres, md5 stable | 18/11, `AUDIT_FINAL.md:67-76` | processus complet |
| `1 000 000!` | **200 ms**, 5 565 709 chiffres | 18/11, `PLAN.md:247-250` | moteur arithmétique |
| `fib(27)` via runtime wasm officiel | **médiane 2,885 ms** (6 échantillons, 2,058–3,881 ms) | 18/11, `WASM_RUNTIME.md:199-229`, CLI réelle | **appel bibliothèque externe** |
| `fib(27)` via interpréteur wasm maison | ~150 ms (référence historique) ; 172,1 ms médiane dans un harnais isolé le 17/11 | `PLAN.md:282`, `WASM_RUNTIME.md:231-239` | appel bibliothèque externe |

Lecture honnête de la ligne wasm : le 2,885 ms mesure un **processus CLI qui
appelle un module wasm externe**. Ce n'est pas la vitesse à laquelle Rose
exécute un programme Rose. Le contrôle négatif de `WASM_RUNTIME.md:225-229`
(runner retiré du cache → échec du repli) prouve que le `196418` obtenu venait
bien du runner officiel.

Référence C du `PLAN.md:100-103` (test de division jusqu'à √n, N = 50 000) :
C `-O2` = 4 ms, boucle nue Rose = 26 ms, primitive native = 51 ms.

### 1.6 Backend machine, wasm, Windows

- **Backend machine** (`NATIF.md`) : `natif_emetteur.rs` traduit le RIR en
  octets x86-64 / aarch64 sans LLVM ni Cranelift, ABI `i64 code(base, tramp)`.
  6/6 tests verts, exécution réelle via `mmap` (7×7−5 = 44 sur aarch64 natif),
  x86-64 vérifié sous qemu, `fib(25)` = 75025 sous qemu. Le module est câblé
  (`PLAN.md` vague 2, agent B4) mais le chemin principal reste Cranelift.
- **Lien natif** (`LINK_NATIF.md`) : `link_natif.rs` écrit l'ELF64 et le PE32+
  directement en Rust, sans `gcc`/`ld`. Preuve PROUVÉE le 17/11 : un ELF
  aarch64 issu de `retour 49` s'exécute et rend **code de sortie 49**.
- **Wasm** (`WASM_FFI.md`, `WASM_RUNTIME.md`) : lecteur maison core wasm 1.0,
  zéro dépendance, `@dll("wasm", "lib.wasm")` + `@sym(...)`. Runtime officiel
  optionnel, séparé, vérifié par SHA-256. Aucun runtime n'est embarqué.
- **Windows** : la voie de livraison est Velopack, avec des exécutables liés
  statiquement (`rose.exe`, `rose-shell.exe`, installeur GUI — skill `rose-dev`).
  Le PE émis par `link_natif.rs` a une structure valide (imports
  `KERNEL32.DLL!ExitProcess`, stub Win64), **mais son exécution sous Windows
  n'a jamais été testée** (`LINK_NATIF.md:191-198, 331-332`).

---

## 2. Ce que Rose peut faire théoriquement (plafonds, pas des promesses)

### 2.1 Calcul exact : taille et complexité

- **Taille maximale des entiers** : il n'y a **pas de borne logicielle**. Un
  entier est un `BigInt` stocké en limbs ; sa taille est limitée par la mémoire
  disponible (RAM + swap), pas par le langage. Théoriquement, un entier de
  N bits occupe O(N) octets. Démontré à 5 565 709 chiffres (`1 000 000!`) ;
  rien dans l'architecture n'interdit d'aller plus loin si la mémoire suit.
  **THÉORIQUE** : la limite pratique est la mémoire et le temps, pas un format.
- **Complexité des opérations** (`mul_rapide.rs:3-20, 507-521`) : addition
  O(n) ; école O(n²) ; Karatsuba O(n^1,585) ; Toom-3 ; NTT **O(n log n)**
  au-delà du seuil, sur 2 modules de 62 bits + CRT. n = nombre de limbs.
  La factorielle utilise un arbre de produit équilibré. **PROUVÉ** pour les
  seuils et les modules (commentaires de code calibrés par mesure) ;
  **THÉORIQUE** pour l'ordre de grandeur asymptotique, qui est le plafond
  d'un tel schéma, pas une garantie de temps sur une machine donnée.

### 2.2 Vitesse : plafonds des trois chemins

| Chemin | Plafond théorique | Ce qui est MESURÉ | Ce qui reste ESTIMÉ |
|---|---|---|---|
| Interpréteur à registres (RIR) | Le coût du dispatch par opcode. Même avec des slots, il ne rejoint pas le code machine sur une boucle serrée. | 3,65 s → 0,57 s après RIR ; boucle nue 826 ms après optimisation (17/11) ; 26 ms vs 4 ms en C sur N = 50 000 (`PLAN.md:100-103`) | Aucun chiffre fiable au-delà de ces points ; le « 50× le C » de `rir.rs:4-14` est un diagnostic **avant** RIR, pas l'état actuel. |
| JIT Cranelift | Code machine + compilation + appels runtime. Peut approcher C/Rust sur le sous-ensemble compilable, pas dépasser le matériel ; paie les replis. | `fib(25)` 1–2 ms ; jardin avec JIT 50 → 47 ms (17/11, `PLAN.md:76`) | Pas de banc comparatif JIT vs Rust publié dans ces documents. |
| Code machine natif (`natif_emetteur`) | Le processeur lui-même, sans dispatch. C'est le plafond ultime, au niveau de C/Rust. | `fib(25)` 1,57 ms en harnais isolé release (17/11) | Atteindre ce plafond **dans le binaire livré** demanderait un runtime embarqué et l'ABI Win64 corrigée : non fait. |

Comparaison à Python : **aucune mesure Python n'existe dans les documents
vérifiés**. Théoriquement, un interpréteur de bytecode comme CPython est dans
la même famille d'ordres de grandeur que l'interpréteur Rose (quelques dizaines
de fois plus lent que C sur des boucles arithmétiques). C'est une **estimation
générale, non une mesure de ce dépôt**. Ne pas la présenter comme un résultat.

`natif_emetteur.rs` ne manipule que des **i64 non boxés** (`NATIF.md:13-19`) :
au-delà de i64, ou pour les flottants et la division, il appelle une trampoline
qui retombe sur l'interpréteur. Le plafond de vitesse du natif est donc
conditionnel au sous-ensemble couvert.

### 2.3 Auto-hébergement

Le point fixe octet est **PROUVÉ** (§1.4) : le compilateur en Rose reproduit
l'émetteur à l'octet près. Théoriquement, cela ouvre la voie annoncée par
`PLAN.md:12-14, 179-187` : modifier le compilateur **en Rose**, re-bootstrapper,
vérifier le point fixe, et faire régresser le moteur Rust au rôle d'amorce
(stage0), comme Zig.

**Mais le binaire sans Rust n'existe pas encore.** Le binaire livré est toujours
produit par `rustc` et le dit lui-même, mesure PROUVÉE le 17/11
(`PRODUIT_SANS_RUST.md:173, 208-213`) :
`Rose 0.1.5 — compilateur Rust + JIT Cranelift (sémantique exacte)`.
std est lié **statiquement** (886 symboles `std` v0-manglés dans la rlib,
`library/std` ×52 et `cranelift` ×117 dans `strings`). Le « 0 » de
`nm | grep -c 'std::'` sur le binaire stripé est un artefact, pas une preuve
d'absence de std. G4 (`PRODUIT_SANS_RUST.md:258-265`) reste à faire.

### 2.4 Interopérabilité wasm

Théoriquement, **toute bibliothèque compilable en wasm est appelable depuis
Rose** : C/C++ (`clang --target=wasm32 -nostdlib`), Rust
(`wasm32-unknown-unknown`), Zig, Emscripten (`WASM_FFI.md:13-18`). C'est le
format universel visé par M6. Le pont exige un module **autonome** : pas
d'imports, pas de section `start`, pas de `call_indirect` (`WASM_FFI.md:78-80`,
`COMPONENT_MODEL.md:1.1`).

Limites réelles de l'ABI C actuelle (`COMPONENT_MODEL.md:122-141`,
`WASM_FFI.md:50-66, 103-113`) :

- scalaires seulement : `entier` → **i32 (32 bits)**, `flottant` → f64,
  `chaine` → pointeur avec zéro terminal ;
- pas de type `liste`/`tableau`/`record` : un `int*` est un simple `entier`
  (adresse) avec une longueur passée à la main (`somme_tableau(int *t, int n)`) ;
- troncature silencieuse : `z.get_i64() as i32` coupe sans erreur hors
  `[−2³¹, 2³¹−1]` ;
- convention maison (marge de 8 octets, écriture en fin de mémoire), pas de
  protocole d'allocation ni de durée de vie ;
- l'interpréteur wasm reste un pont de **compatibilité**, pas une voie de
  vitesse.

Le **Component Model / WIT** (`COMPONENT_MODEL.md`) est une **étude**, pas une
implémentation. Il apporterait une interface typée (`list<s32>` = couple
`ptr,len`) et une durée de vie définie. Estimation de planification
**EXPLICITEMENT non mesurée** : ≈ 2–3 semaines-homme pour un sous-ensemble
`list<s32>` + chaînes, 6–10 pour un composant v2 complet. Rien n'est à
implémenter pour 0.1.5/0.1.6.

### 2.5 Portabilité : ELF/PE sans linker, runtime minimal

Théoriquement réalisé et partiellement PROUVÉ (`LINK_NATIF.md`) :

- **ELF64** : `ET_EXEC` statique, un seul `PT_LOAD` RWX, aucune relocation,
  stub `_start` qui appelle le code émis et sort. Exécution réelle prouvée sur
  aarch64 (**code 49**, 17/11).
- **PE32+** : en-têtes, table d'imports `KERNEL32.DLL!ExitProcess`, stub Win64
  avec shadow space. **Structure prouvée, exécution Windows non testée.**
  Une correction ABI reste nécessaire dans `natif_emetteur.rs` dès que le corps
  appelle la trampoline (registres Win64 + 32 octets de shadow space), sinon
  `RuntimeRequis` est refusé — ce qui évite un binaire silencieusement faux.
- **Runtime minimal** : `sys_io.rs` n'a aucun `std::` exécutable ; un binaire
  `#![no_std]` + `alloc` **réel** calcule `50!` = 65 chiffres exacts, mesuré le
  18/11 (`BINAIRE_NO_STD.md:126-137`). Ce binaire est une démonstration : 4
  opcodes schématiques en dur, aucun frontal ni moteur Rose (`§5` du même doc).

---

## 3. Les limites actuelles et ce qui les bloque (honnêteté)

1. **G2/G3 — `no_std` : Cranelift est un prérequis dur.** Mesure PROUVÉE le
   18/11 (`NO_STD_MOTEUR.md:25-69`) : `Cargo.toml:61` contient déjà
   `libc = { version = "0.2", default-features = false }`, mais
   `cargo tree -e features -i libc` montre que la feature `std` de `libc` est
   réactivée **uniquement** par la chaîne
   `cranelift → cranelift-jit → {memmap2, region, wasmtime-internal-jit-icache-coherence}`.
   G3 est donc impossible avant G2 (retrait de Cranelift du binaire).
   `i128x.rs` + `mul_rapide.rs` échouent seuls en `no_std` : **270 erreurs**
   (125 prelude manquant, 107 `std::` non résolu, 32 macros, 6 traits), log
   conservé (`NO_STD_MOTEUR.md:234-293`). `catch_unwind` dans `lib.rs` et
   `SystemTime` dans `run.rs` sont des blocages architecturaux (`G3.4`).
   Le binaire `no_std` livré ne prouve pas que le moteur complet compile sans std.

2. **ABI C vs types complexes.** La frontière wasm ne transporte que des
   scalaires ; les listes, tables, records et variants ne passent pas. Voir
   §2.4 et `COMPONENT_MODEL.md:122-141`. Le Component Model n'est pas implémenté.

3. **PE non exécuté sous Windows.** Structure valide et ABI du stub cohérente,
   mais aucun PE n'a été lancé (`LINK_NATIF.md:191-198, 239, 331-332, 373-408`).
   La preuve attendue est un code de retour `49` observé sur une machine Windows.

4. **Release 0.1.5.** Règle invariante 6 du `PLAN.md` : 0.1.6 seulement si M2.1
   ou M5.1 dépasse GMP au banc `factorielles.rose` — ce n'est pas le cas
   (`1 000 000!` = 1,18× plus lent que GMP). Audit 18/11 : `version = "0.1.5"`.

5. **Pas de GMP — règle absolue, et respectée.** `rug` apparaît uniquement dans
   `bench/suite/fact_rs.rs` comme référence de comparaison. Audit 18/11
   (`AUDIT_FINAL.md:107-114`) : `grep -rn rug rose/src --include="*.rs"` vide,
   `grep -n rug rose/Cargo.toml rose/Cargo.lock` vide. Le match littéral dans
   `setup.html` est une sous-chaîne dans un URI base64, pas la crate Rust.
   Aucun jardin en Rust. Remote : `git@github.com:Stef302/Rose.git`.

6. **Backend natif encore partiel.** i64 seulement, pas de PIC, une seule
   fonction par exécutable, `frame ≤ 4095` sur aarch64, trampoline stub qui
   rend 0 dans le lien natif (`NATIF.md:187-216`, `LINK_NATIF.md:113-123`).
   La parité sur les 40 dorés n'est pas démontrée par ce module seul.

---

## 4. Ce que Rose n'est PAS (éviter les malentendus)

- **Pas un compilateur C.** Rose ne lit pas de source C. Il consomme des
  modules **wasm déjà compilés** par clang/rustc/zig (`WASM_FFI.md`). La
  capacité « compiler du C sans runtime » appartient à Zig, pas à Rose
  (`PLAN.md:16-27` donne Zig comme référence, pas comme fonctionnalité Rose).
- **Pas un runtime JavaScript.** Rose n'embarque ni V8 ni moteur JS. Le
  WebAssembly est un format binaire, pas du JavaScript. Le lecteur maison est
  un interpréteur de pile pour un sous-ensemble de core wasm 1.0, avec zéro
  dépendance (`wasm_lecteur.rs:1-20`).
- **Pas encore autonome de Rust.** Le point fixe du compilateur est prouvé,
  mais le produit est encore compilé par `rustc`, lié statiquement à std,
  Cranelift et num-bigint, et s'auto-déclare « compilateur Rust + JIT
  Cranelift ». Les portes G2 → G4 du `PRODUIT_SANS_RUST.md` restent ouvertes.
- **Pas un langage typé statiquement pour l'utilisateur.** `README.md:20` :
  « typage dynamique ». Le type est fort, mais se fixe à la première
  affectation ; les annotations `entier` / `flottant` / `booleen` sur les
  fonctions sont **facultatives** et servent à activer la voie rapide JIT/AOT,
  pas à imposer un typage statique (`PLAN.md:12-14`, skill `rose-dev` piège 8).
- **Pas un moteur de mesure de performance générale.** Les chiffres cités ici
  sont des constats datés sur une machine aarch64 donnée. `AGENTS.md` impose
  de re-mesurer après tout changement de modèle, de quantification ou de
  topologie. Aucun chiffre du présent document ne doit être recopié comme
  consigne.

---

## En bref

- **Aujourd'hui** : Rose exécute du Rose par interpréteur/RIR, JIT Cranelift et
  AOT LLVM ; calcul exact illimité sans GMP ; opérateurs symboliques natifs ;
  4 oracles à 150/150 et un point fixe de compilateur md5 `bdbef636…` le 18/11.
- **Perf mesurée** : jardin 4,11 ms, `fib(25)` 1–2 ms en JIT, `1 000 000!` en
  200 ms, `fib(27)` wasm officiel 2,885 ms — ce dernier étant un appel
  bibliothèque, pas une vitesse du langage.
- **Théoriquement** : entiers limités par la mémoire seulement, multiplication
  NTT en O(n log n), natif au plafond du processeur, wasm ouvre l'interop,
  ELF/PE sans linker. Ce sont des plafonds, pas des promesses.
- **Bloque** : G2/G3 no_std (Cranelift d'abord), ABI C vs types complexes, PE
  pas exécuté sous Windows, binaire encore issu de rustc, release 0.1.5.
- **Identité** : langage en français, typé dynamiquement, sans GMP, jardins en
  Rose, sortie = valeur seule, dépôt `Stef302/Rose`.
