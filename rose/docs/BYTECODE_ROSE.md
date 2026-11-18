# Bytecode Rose « dans la mentalité de Java » — étude de faisabilité

> **Statut** : étude **documentaire uniquement**. Aucun fichier source, `Cargo.toml`,
> `PLAN.md` ou document d'un autre agent n'a été modifié. Aucun commit.
> **Cible** : Rose 0.1.5 — dépôt `Stef302/Rose`.
> **Constat daté** : 18/11/2026. Les mesures ci-dessous ont été **exécutées** sur le
> binaire `rose/target/release/rose` présent dans le dépôt ; tout ce qui n'a pas été
> mesuré est explicitement marqué **ESTIMÉ** ou **PROPOSÉ**.

---

## 0. Verdict en une phrase

**Oui, c'est intéressant — mais comme un incrément d'ingénierie, pas comme un changement
de paradigme.** Le RIR est **déjà** le bytecode interne de Rose (ops linéaires, slots
indexés, sauts numériques, boucle à `pc`, JIT et émetteur natif qui le consomment). Ce qui
manque pour parler de bytecode *au sens Java*, ce n'est pas le moteur d'exécution, c'est
**un format binaire stable + un chargeur + un vérificateur**. Le formaliser en `.rosebc`
serait utile ; appeler cela « une JVM Rose » serait faux (pas de GC, pas de classloader,
pas de sandbox).

---

## 1. Ce qu'est un bytecode — et ce que Rose a déjà

### 1.1 Définition de travail (4 critères)

Un « bytecode comme Java » n'est pas seulement « des opcodes numériques ». Un format
mérite ce nom quand il réunit **les quatre** propriétés suivantes :

| # | Critère | Ce que cela exige concrètement |
|---|---|---|
| C1 | **Format binaire stable et versionné** | Octets, en-tête magic + version, indépendant de la version du compilateur qui l'a produit |
| C2 | **Chargement sans source** | Le moteur peut exécuter le fichier sans relire le `.rose` |
| C3 | **Vérification à l'entrée** | Refus propre d'un fichier tronqué, d'un saut hors bornes, d'une version inconnue (équivalent du *bytecode verifier*, mais à échelle réduite) |
| C4 | **Exécution par une VM + compilation JIT possible** | Une boucle d'exécution interprète les ops ; un compilateur peut consommer les mêmes ops |

### 1.2 Comparaison avec ce qui existe dans Rose

| Critère | `.class` Java (JVM) | RIR Rose (`rose/src/rir.rs`) |
|---|---|---|
| C1 format binaire stable | ✅ fichier `.class`, magic `CAFEBABE`, version majeure/mineure | ❌ **forme en mémoire** uniquement : `CorpsRir` est un `struct` Rust (`rir.rs:201-211`), aucun en-tête, aucune version |
| C2 chargement sans source | ✅ la JVM charge le `.class` | ❌ `rose rir` **abaisse la source puis l'imprime** (`main.rs:218-256`, surtout `main.rs:229`) ; l'exécution normale repasse par `lire` + `analyser_fichier` (`main.rs:971-1004`) |
| C3 vérification | ✅ verifier de la JVM | ❌ aucun vérificateur de format ; la seule « vérification » est `front::verifier` sur la **source** (`main.rs:578-583`, `main.rs:840-846`) |
| C4 VM + JIT | ✅ JVM + HotSpot/C2 | ✅ **déjà présent** : `executer_avec_ptr` boucle sur `pc` (`rir.rs:923-1089`) ; `jit.rs:29` importe `CorpsRir/Eabstrait/Op/Oper` et compile ces ops (`jit.rs:1513`, `jit.rs:1765-1818`) ; `natif_emetteur` traduit le RIR en octets machine (`rose/docs/NATIF.md:8-11, 79-81`) |

**Lecture** : Rose satisfait déjà **C4**, et c'est le critère le plus coûteux à construire.
Il lui manque C1, C2, C3 — c'est-à-dire la couche *format de fichier*, pas la couche
*moteur*. Le reste du document ne porte que sur cette couche.

### 1.3 Ce que Rose a déjà d'autre (et qui ne doit pas être confondu)

- **`rose comp`** produit un **exécutable autonome** = moteur copié + **source collée** +
  une queue de 16 octets portant `ROSE_MAGIC` (`main.rs:545`, `comp_programme`
  `main.rs:570-611`, lecture en `main.rs:552-568`). C'est déjà un format de *distribution*,
  mais il transporte du **texte source**, pas du bytecode.
- **Le pont wasm est consommateur**, pas émetteur : `wasm_lecteur` / `wasm_runtime` /
  `@dll("wasm", …)` lisent et appellent des modules `.wasm` (`main.rs:848-933`). Aucun
  émetteur Rose→wasm n'existe dans `rose/src` ni dans `compilateur/`. Donc `.rosebc`
  **ne serait pas redondant avec wasm** pour distribuer un programme Rose — mais il ne
  faudrait pas le présenter comme « l'équivalent Rose de WebAssembly » : wasm est un
  standard inter-langages, `.rosebc` serait un snapshot interne d'un moteur précis.

---

## 2. Rose a-t-il déjà un bytecode sans le savoir ?

### 2.1 `CorpsRir` est-il sérialisable ? Non, pas tel quel

`CorpsRir` dérive uniquement `Clone, Debug` (`rir.rs:201`) ; il en va de même pour `Op`
(`rir.rs:36`), `Eabstrait` (`rir.rs:119`), `Expr` (`front.rs:397`) et `Instr`
(`front.rs:498`). Il n'y a **aucun** `serde` / `bincode` / `postcard` dans
`rose/Cargo.toml` ni aucun `derive(Serialize)` dans `rose/src`. Trois obstacles concrets
à une sérialisation directe :

1. **`CacheSlot` contient un `Cell`** (`rir.rs:166-183`) avec `unsafe impl Sync`
   (`rir.rs:175`). C'est un cache d'analyse *mémoire* (pointeur + code type, `rir.rs:123-138`),
   qui pointe vers une pile donnée. Il **ne doit jamais être sérialisé** : le chargeur
   doit reconstruire `Eabstrait::Slot(s, CacheSlot::neuve())` (`rir.rs:224`).
2. **`cible: usize`** dans `Saut` / `SautSiFaux*` (`rir.rs:67, 69, 72`). `usize` n'est pas
   un format sur disque : il faut une largeur fixe (u32 proposé) **et** une borne de
   vérification (`cible < n_ops`).
3. **Une majorité d'opcodes portent encore de l'AST**, pas seulement des slots :
   sur les 14 variantes de `Op` (`rir.rs:37-88`), 6 seulement sont entièrement
   « binaires » (`Affect`, `AfficherAbstrait`, `Lire`, `Saut`, `SautSiFauxAbstrait`,
   `RetourAbstrait`) ; **8 embarquent `Expr` ou `Instr`** (`AffectAst`, `AffectIndex`,
   `Afficher`, `SautSiFaux`, `Retour`, `ExprStmt`, `Lever`, `Bloc`). Même `Eabstrait`
   n'est pas totalement clos : `AppelRir { nom, args: Vec<Expr> }` (`rir.rs:155`) contient
   encore des expressions AST. Un `.rosebc` *complet* doit donc aussi编码 l'AST, ou
   restreindre volontairement son périmètre (§3.1, §6).

À noter : `Eabstrait::Entier(String)` (`rir.rs:140`) stocke le **texte exact** du littéral.
C'est cohérent avec l'arithmétique exacte BigInt maison et la règle « GMP jamais
dépendance » : un bytecode Rose ne doit **pas** écrire ces littéraux en i64 fixes, sinon
il perdrait les entiers arbitrairement grands.

### 2.2 Le dump `rose rir` : un « bytecode textuel » stable ?

La commande `rir` (`main.rs:830-839`) appelle `rir_programme_dump` (`main.rs:218-256`) :
elle **abaisse la source** (`rir::abaisser`, `main.rs:229`) puis sérialise le résultat en
texte via `rir_corps_dump` / `rir_op` / `rir_eabstrait` (`main.rs:194-216`, `96-192`).
Le format texte est parfaitement déterministe. **Mesure réelle effectuée pour cette étude** :

```
$ printf 'fn carre(n: entier) -> entier {\n  retour n * n\n}\n' > /tmp/opencode/bytecode/carre.rose
$ ./rose/target/release/rose rir /tmp/opencode/bytecode/carre.rose > dump1.txt
$ ./rose/target/release/rose rir /tmp/opencode/bytecode/carre.rose > dump2.txt
$ md5sum dump1.txt dump2.txt
76ed72db5484c360d8c7d40c2fcbf9d8  dump1.txt
76ed72db5484c360d8c7d40c2fcbf9d8  dump2.txt
$ wc -c carre.rose dump1.txt
 49 carre.rose
188 dump1.txt
```

Sortie réelle (citée telle quelle) :

```
rir
fonction carre
  param n entier
  retour entier
  n_slots 1
  noms [0]n
  ops
    0000 retour_abstrait (bin * slot 0 slot 0)
fonction $principal
  retour vide
  n_slots 0
  noms
  ops
```

Le même programme s'exécute correctement (`>> carre(7)` → `49`), ce qui confirme que le
dump décrit bien la forme réellement exécutée.

**Précision importante sur les « 150/150 oracles ».** Le chiffre est réel mais porte sur
l'**émetteur RIR de l'auto-hébergement**, pas sur un format de fichier public :

- `rose/docs/CAPACITES.md:61-75` : 4 oracles à 150/150 sur 150 fichiers, comparaison
  **octet pour octet**, dont « **M4.4 émetteur RIR (`rose rir`) : 150 match / 0 écart** » ;
- `rose/docs/AUDIT_FINAL.md:51-54` : point fixe M4.5 — `rose rir compilateur/emetteur.rose`
  et `rose compilateur/emetteur.rose compilateur/emetteur.rose` donnent le **même md5**
  `bdbef63681608a0b22a5637539e9d738`, « point fixe octet pour octet ATTEINT ».

Ce que cela prouve : la sortie de `rose rir` est un **artefact stable, comparable à
l'octet**, utilisé comme oracle. Ce que cela ne prouve **pas** : qu'un fichier de dump
puisse être *rechargé* et exécuté. Aujourd'hui aucun chemin de code ne le fait. La
stabilité est donc une **base solide** pour un format binaire, pas déjà un format binaire.

### 2.3 Texte vs binaire : la différence concrète

| | Dump texte `rose rir` | `.rosebc` proposé |
|---|---|---|
| Taille pour `carre` | **188 o mesurés** | **73 o calculés** (§3.5) |
| Chargement | ❌ illisible par le moteur (il faut re-parser le `.rose`) | ✅ but : `rose run carre.rosebc` |
| Versioning | ❌ aucun champ de version ; un changement de `rir_op` change silencieusement le texte | ✅ magic + versions majeure/mineure, refus explicite |
| Vérification | ❌ aucune borne à vérifier | ✅ bornes slots/sauts, longueur exacte |
| Découpage | lignes + espaces, nécessite un parser de texte | ✅ champs à longueur fixe/préfixée, `memcpy`-like |
| Lisibilité humaine | ✅ bonne | ❌ mauvaise (mais un `rose bc --dump` peut la restaurer) |
| Stabilité | ✅ démontrée (md5 identique, 150/150) | à construire — **plus contraignante** : un opcode attribué ne peut plus changer |

---

## 3. Design proposé pour un `.rosebc`

### 3.1 Principes directeurs

1. **Snapshot du RIR, pas nouvel IR.** Le sérialiseur consomme `CorpsRir` tel qu'il existe ;
   aucune sémantique nouvelle, aucun nouveau `Op`. Le chargeur reconstruit le même
   `CorpsRir` et l'envoie aux mêmes trois consommateurs (`executer_avec_ptr`, `jit`,
   `natif_emetteur`).
2. **Sous-ensemble binaire d'abord (v1).** Un prédicat `est_binairement_serialisable(corps)`
   refuse tout corps contenant un `Expr`/`Instr` (§2.1 point 3). Ces fonctions restent
   exécutées depuis la source `.rose`, exactement comme aujourd'hui quand `abaisser`
   rend `None` (`rir.rs:25-27`, `rir.rs:539`). Cela évite d'avoir à inventer un format
   AST complet dès la v1.
3. **Zéro dépendance, zéro GMP.** Encodage écrit à la main en Rust (comme `natif_emetteur`),
   `Cargo.toml` inchangé. Les entiers exacts restent du **texte décimal à longueur
   préfixée** (`Eabstrait::Entier(String)`, `rir.rs:140`).
4. **Little-endian fixe**, quel que soit l'hôte (les cibles de Rose sont x86-64/aarch64,
   toutes deux LE ; le contraire est refusé par le vérificateur).
5. **Le cache n'est jamais sérialisé** : `Slot(s, _)` est réécrit `Slot(s, CacheSlot::neuve())`
   au chargement.

### 3.2 En-tête (16 octets)

| Offset | Taille | Champ | Valeur |
|---|---|---|---|
| 0x00 | 6 | magic ASCII | `ROSEBC` = `52 4F 53 45 42 43` |
| 0x06 | 2 | version majeure (u16 LE) | `1` — changement incompatible |
| 0x08 | 2 | version mineure (u16 LE) | `0` — ajout rétrocompatible |
| 0x0A | 1 | drapeaux | bit0 = LE obligatoire (1), bit1 = noms de slots présents (1) |
| 0x0B | 1 | réservé | `0` |
| 0x0C | 4 | nombre de corps (u32 LE) | ≥ 1 |

> Le magic est distinct de `ROSE_MAGIC` (`main.rs:545`) qui marque la queue des
> exécutables `rose comp` : les deux formats doivent pouvoir coexister sans ambiguïté.

### 3.3 Corps et tables

Pour chaque corps, dans l'ordre :

| Champ | Encodage | Origine dans le code |
|---|---|---|
| nom de fonction | `u16 len` + UTF-8 | clé de `prog.signatures` |
| `n_params` | `u16` | `sig.params.len()` |
| `n_slots` | `u16` | `CorpsRir::n_slots` (`rir.rs:210`) |
| `n_ops` | `u32` | `CorpsRir::ops.len()` (`rir.rs:204`) |
| type de retour | `u8` (table des `TyKind`, `front.rs:20`) | `sig.retour` |
| paramètres | `n_params` × (`u16 len` + UTF-8 + `u8 type`) | `sig.params` |
| table des noms | `n_slots` × (`u16 len` + UTF-8) | `CorpsRir::noms` (`rir.rs:206`) |
| slots globaux | `u16 count` + `count` × `u16` | `CorpsRir::globaux` (`rir.rs:208`) |
| opcodes | `n_ops` × (`u8 opcode` + opérandes) | `CorpsRir::ops` (`rir.rs:204`) |

Codes de type (proposés, ordre de `TyKind` `front.rs:20`) :
`0x00=Dyn, 0x01=Entier, 0x02=Rationnel, 0x03=Flottant, 0x04=Booleen, 0x05=Chaine,
0x06=Liste, 0x07=Table, 0x08=Tampon, 0x09=Tableau, 0x0A=Complexe, 0x0B=Irrationnel,
0x0C=Vide`. `TyKind` dérive `Copy` (`front.rs:19`) : ces codes sont stables tant que
l'énumération ne change pas — d'où l'importance du numéro de version.

### 3.4 Opcodes numériques (dérivés de `Op` et `Eabstrait`)

`Op` (`rir.rs:37-88`) :

| Code | Variante Rust | Opérandes |
|---|---|---|
| `0x01` | `Affect` | `u16 slot` + expr |
| `0x02` | `AfficherAbstrait` | expr |
| `0x03` | `Lire` | `u16 slot` + `u8 type` |
| `0x04` | `Saut` | `u32 cible` |
| `0x05` | `SautSiFauxAbstrait` | expr + `u32 cible` |
| `0x06` | `RetourAbstrait` | expr |
| `0x07` | `RetourVide` | — (cas `Retour(None)`, `rir.rs:424`) |
| `0x80..0xEF` | variantes à AST | **refusées en v1** par `est_binairement_serialisable` |

`Eabstrait` (`rir.rs:120-156`) :

| Code | Variante Rust | Opérandes |
|---|---|---|
| `0x10` | `Slot` | `u16` (le `CacheSlot` est reconstruit à vide) |
| `0x11` | `Entier` | `u16 len` + texte ASCII décimal (BigInt, **jamais i64**) |
| `0x12` | `Flottant` | `f64` IEEE-754, 8 octets LE |
| `0x13` | `Binaire` | `u8 op` + expr + expr |
| `0x14` | `Neg` | expr |
| `0x15` | `Non` | expr |
| `0x16` | `AppelRir` | **refusé en v1** (contient `Vec<Expr>`, `rir.rs:155`) |

`Oper` (`rir.rs:187-191`) : `0x01=Add, 02=Sub, 03=Mul, 04=Div, 05=Mod, 06=Puissance,
07=DivEntier, 08=Lt, 09=Le, 0A=Gt, 0B=Ge, 0C=Eq, 0D=Ne, 0E=Et, 0F=Ou`.

Les sauts restent **numériques** (`usize` → `u32` + contrôle `cible < n_ops`), exactement
comme dans le RIR (`rir.rs:34-35`, `rir.rs:475-483`).

### 3.5 Exemple complet : `fn carre(n: entier) -> entier { retour n * n }`

Source réelle : **49 octets**. Dump RIR réel : **188 octets**. Encodage `.rosebc`
**calculé par le script d'étude** (deux corps, `carre` et `$principal`, comme dans le dump
réel) : **73 octets**.

```
0000  52 4F 53 45 42 43 01 00 00 00 00 00 02 00 00 00 |ROSEBC..........|
0010  05 00 63 61 72 72 65 01 00 01 00 01 00 00 00 01 |..carre.........|
0020  01 00 6E 01 01 00 6E 00 00 0B 13 03 10 00 00 10 |..n...n.........|
0030  00 00 0A 00 24 70 72 69 6E 63 69 70 61 6C 00 00 |....$principal..|
0040  00 00 00 00 00 00 04 00 00                      |.........|
```

Décodage annoté :

| Octets | Signification |
|---|---|
| `52 4F 53 45 42 43` | magic `ROSEBC` |
| `01 00` / `00 00` | version 1.0 |
| `00` / `00` | drapeaux LE + noms / réservé |
| `02 00 00 00` | 2 corps (`carre`, `$principal`) |
| `05 00 63 61 72 72 65` | nom `carre` |
| `01 00` / `01 00` / `01 00 00 00` / `01` | 1 paramètre, 1 slot, 1 op, retour `entier` |
| `01 00 6E 01` | paramètre `n`, type `entier` |
| `01 00 6E` | table des noms : slot 0 = `n` |
| `00 00` | aucun slot global |
| `0B` | opcode `RetourAbstrait` |
| `13 03` | `Binaire` + `Oper::Mul` |
| `10 00 00` | `Slot(0)` (cache reconstruit) |
| `10 00 00` | `Slot(0)` |
| `0A 00 24…` | second corps : `$principal` (10 octets de nom) |
| `… 00 00 00 00 00 00 04 00 00` | 0 param, 0 slot, 0 op, retour `vide` |

**Rapport de taille (mesuré/calculé, non extrapolé)** : 73 / 188 = **0,39** du dump texte,
et 73 / 49 = **1,49** de la source. Le binaire est plus compact que le dump mais **plus
gros que la source** sur un programme minuscule : l'intérêt de taille n'apparaîtrait que
sur des corps avec beaucoup d'opérations (les noms et l'en-tête sont amortis). **ESTIMÉ,
non mesuré** : sur un corps de 1 000 ops, le coût par op abstraite est de l'ordre de
4 à 12 octets selon l'expression, contre 25–60 octets en moyenne dans le dump texte ;
à valider par mesure sur le corpus des 150 fichiers avant de l'affirmer.

### 3.6 Versioning et vérification (C1 + C3)

Le vérificateur (nouveau module, ~150 lignes **ESTIMÉ**) refuse, sans panic et avec un
message stable :

1. magic absent ou erroné ;
2. version majeure ≠ 1 ; version mineure > celle supportée ;
3. `n_corps = 0` ou longueur de fichier ≠ somme des tailles déclarées (ni octets en trop,
   ni troncature) ;
4. UTF-8 invalide dans un nom / une chaîne d'entier ;
5. `n_slots > 65535` (u16), `n_ops = 0` pour un corps non principal, doublon de nom ;
6. **tout index de slot ≥ `n_slots`** et **toute cible de saut ≥ `n_ops`** ;
7. opcode inconnu (y compris une variante à AST en v1) ;
8. drapeau endianness non conforme.

C'est un « verifier » dans l'esprit, mais **beaucoup plus petit** que celui de la JVM :
Rose n'a pas de types de référence, pas de héritage, pas de bytecode de classe à
résoudre. Il n'y a pas lieu d'imaginer un « chargeur de classes » Rose.

---

## 4. Avantages et inconvénients

### 4.1 Avantages (réels, mais à mesurer pour la plupart)

| Avantage | Fondement dans le code | Réserve |
|---|---|---|
| **Démarrage plus rapide** | `rose rir` refait lexage + analyse + vérification + abaissement à chaque lancement (`main.rs:832-837`, `rir_programme_dump` `main.rs:229`) ; un `.rosebc` sauterait ces étapes | **Aucune mesure de temps de chargement `.rosebc` n'existe** (le format n'existe pas). Hypothèse à mesurer avec `rose debug-lenteur` (`main.rs:662-664`, `main.rs:1026-1031`) avant/après |
| **Distribution sans source** | `rose comp` embarque déjà la source en clair (`main.rs:597`) ; un `.rosebc` ne l'embarquerait plus | Le bytecode est **réversible** : ce n'est ni du chiffrement ni une protection. À ne pas vendre comme sûreté |
| **Exécution universelle via le moteur Rose** | Le chargeur alimente le même `CorpsRir` → mêmes `executer_avec_ptr` / JIT / émetteur natif | « Universelle » = **sur toute machine ayant le moteur Rose 0.1.5+**, pas sur toute machine (contrairement au wasm) |
| **JIT « à la HotSpot »** | `jit.rs:29` et `jit.rs:3452` compilent déjà `CorpsRir` ; `NATIF.md:8-11` émet des octets machine depuis le RIR | Le JIT existe déjà ; le `.rosebc` ne fait que lui fournir une entrée de plus. Pas de profiling adaptatif à inventer |
| **Stabilité de l'artefact** | Le dump est déjà octet-stable (md5 identique mesuré ; 150/150 émetteur RIR, point fixe M4.5) | La stabilité du *texte* ne garantit pas celle du *binaire* : elle doit être **maintenue activement** |

### 4.2 Inconvénients (à dire clairement)

| Inconvénient | Preuve / explication |
|---|---|
| **Complexité nouvelle** | 4 chemins à écrire et tester : encodeur, décodeur, vérificateur, exécution directe — alors que le moteur fonctionne déjà |
| **Divergence IR ↔ AST** | Le `.rosebc` serait un snapshot figé : après une modification du compilateur, un ancien `.rosebc` garde l'ancienne sémantique. Mitigation : versions + éventuel hash de l'émetteur ; mais c'est un **coût de maintenance permanent** |
| **Format stable à maintenir** | Chaque nouvelle variante de `Op`/`Eabstrait` (14 et 7 variantes aujourd'hui, `rir.rs:37-88, 120-156`) exige opcode + encodeur + décodeur + vérificateur. Une table d'opcodes mal gérée casse la compatibilité |
| **Pas de JVM derrière** | Aucun GC trouvé dans `rose/src` (seules des variables nommées `gc` dans `interp.rs:2740-2748` désignent un booléen de complexe, et un commentaire sur `wasm_lecteur.rs:11`) ; **aucun classloader**, **aucun sandbox** (seul `wasm_lecteur.rs:13` parle de wasm « sandboxé », pas de Rose). Exécuter un `.rosebc` inconnu n'est **pas plus sûr** que d'exécuter un `.rose` inconnu |
| **Périmètre v1 réduit** | 8 opcodes sur 14 portent de l'AST (`§2.1`) : les fonctions les plus complexes resteraient sur la voie source. Le bénéfice ne couvre donc pas tout le langage au départ |
| **Taille non garantie** | Sur `carre`, le binaire (73 o) est **plus gros** que la source (49 o). Le gain de taille n'est qu'une hypothèse sur les gros corps |
| **Risque de sur-vente** | Présenter ceci comme « Rose devient une JVM » serait factuellement faux : ce serait un **format de snapshot RIR**, rien de plus |

---

## 5. Recommandation argumentée

### 5.1 Réponse directe à la question posée

> « Est-ce intéressant d'incorporer un système de bytecode comme Java ? On fait déjà
> interpréteur et compilateur donc on est hybride. »

**Le diagnostic « on est déjà hybride » est exact.** L'interpréteur (RIR + `HoteRir`), le
JIT Cranelift (`jit.rs`), l'émetteur machine (`natif_emetteur.rs`) et le pont wasm
consommateur coexistent. **Mais l'hybridité interpréteur/compilateur n'est pas la même
chose que le bytecode.** Le bytecode au sens Java = **un contrat de format entre le
producteur et l'exécutant** ; l'hybridité = **deux stratégies d'exécution d'un même IR**.
Rose a la seconde, pas le premier.

### 5.2 Trois niveaux à ne pas confondre

| Niveau | État réel | Verdict |
|---|---|---|
| **(a) Bytecode interne** — IR linéaire exécuté par une VM et compilable JIT | ✅ **Déjà fait** : `CorpsRir`, `Op`, `Eabstrait`, boucle `pc`, JIT, émetteur natif | Rien à ajouter. C'est ici que se situent les 50× de gain RIR vs interpréteur récursif documentés en tête de `rir.rs:5-19` |
| **(b) Format binaire stable `.rosebc`** — magic, version, vérificateur, chargeur | ❌ Absent | **Incrément utile, de portée limitée** (~1 module + 1 commande). À retenir **uniquement** si un besoin mesuré (démarrage, distribution sans source) le justifie |
| **(c) Écosystème « JVM Rose »** — GC, classloader, sandbox, chargement dynamique de classes | ❌ Absent, et **non souhaitable ici** | **Hors périmètre**. Aucun mécanisme de ce type dans le moteur ; l'ajouter serait un autre projet, pas un bytecode |

### 5.3 Verdict

**Intéressant mais redondant au niveau du paradigme ; utile au niveau de l'ingénierie.**

- Le RIR **est déjà** le bytecode de Rose. Lui donner une extension `.rosebc` ne rend pas
  le langage « plus complet » au sens sémantique : aucune primitive, aucun type, aucune
  optimisation nouvelle n'apparaît.
- Cela **répondrait** à un vrai manque technique : aujourd'hui le seul artefact
  distribuable est `rose comp` = **moteur + source en clair** (`main.rs:570-611`). Un
  `.rosebc` permettrait de distribuer le programme sans sa source et d'éviter lexage +
  analyse au démarrage.
- Ce n'est **pas redondant avec wasm**, puisque wasm est ici consommé et non produit.
  Mais c'est **partiellement redondant avec `comp`**, qui couvre déjà la distribution
  autonome.
- **Priorité recommandée : basse/moyenne**, derrière tout ce qui améliore l'exécution
  elle-même. Si la question est « est-ce que ça rend Rose plus complet ? », la réponse
  honnête est **non, pas de façon fondamentale** ; si la question est « est-ce un bon
  petit projet bien délimité ? », la réponse est **oui, à condition de ne pas le
  surestimer**.

**Formulation à employer dans la documentation produit** :
> « Rose exécute un IR linéaire (le RIR). Un format `.rosebc` optionnel permet d'en
> enregistrer un snapshot versionné, rechargeable sans la source. Ce n'est pas une JVM :
> Rose n'a ni ramasse-miettes, ni chargeur de classes, ni bac à sable. »

---

## 6. Route d'implémentation si la recommandation est retenue

Aucune de ces étapes n'a été réalisée dans le cadre de cette étude. Elles sont **PROPOSÉES**.

| Étape | Contenu | Porte de validation **proposée** (à mesurer, pas encore mesurée) |
|---|---|---|
| **S0 — Mesurer le besoin** | Ventiler le temps avec `rose debug-lenteur` (`main.rs:662-664`) sur le corpus : lexage + analyse + vérification + abaissement vs exécution | Si la préparation représente < 5 % du temps total sur la majorité du corpus, **reporter** le projet |
| **S1 — Sérialiseur** | Nouveau `rose/src/ser_rir.rs` : `CorpsRir` → octets, sous-ensemble abstrait seulement, format §3 | Round-trip encode→decode→dump texte : dump identique **octet pour octet** au `rose rir` d'origine sur tous les corps éligibles des 150 fichiers |
| **S2 — Chargeur + vérificateur** | Nouveau `rose/src/load_rosebc.rs` : décodage + 8 contrôles du §3.6 | Corpus de fichiers volontairement corrompus (magic, version, saut hors bornes, slot hors bornes, troncature, octet en trop) → **100 % de rejets propres, zéro panic** |
| **S3 — Commandes** | `rose bc prog.rose [-o prog.rosebc]` et `rose run prog.rosebc` (route dans `main.rs` à côté de `rir`/`verif`, `main.rs:830-846`) | Sur les 150 fichiers : sortie d'exécution **identique** entre `.rose` et `.rosebc` ; les 4 oracles 150/150 et le point fixe M4.5 (`AUDIT_FINAL.md:51-54`) restent verts |
| **S4 — JIT depuis `.rosebc`** | Alimenter `jit::compiler_natif` (`jit.rs:3452`) et/ou `natif_emetteur` directement depuis le corps chargé | Résultats identiques à la voie source ; perf **non régressive** mesurée en A/A/A (≥ 6 échantillons, médianes) |

**Portes transversales (non négociables)** :
- zéro dépendance ajoutée (`Cargo.toml` inchangé : ni serde, ni GMP) ;
- version de release inchangée (0.1.5) tant que le format n'est pas figé ;
- dépôt `Stef302/Rose` uniquement ;
- aucun fichier d'un autre agent modifié, aucun commit dans le cadre de cette étude.

---

## 7. Preuves, références et périmètre de cette étude

### 7.1 Commandes réellement exécutées

```
$ cd /home/clausake/Stefan/ROSE
$ git status --short && git branch --show-current      # branche main, arbre propre
$ ls -la rose/target/release/rose                      # 4 932 328 octets, 18/11 11:07
$ grep -n '^version' rose/Cargo.toml                   # version = "0.1.5"
$ grep -n 'serde\|bincode\|postcard\|rmp' rose/Cargo.toml   # aucun résultat
$ ./rose/target/release/rose rir carre.rose > dump1.txt
$ ./rose/target/release/rose rir carre.rose > dump2.txt
$ md5sum dump1.txt dump2.txt                          # 76ed72db… identiques
$ wc -c carre.rose dump1.txt                           # 49 / 188
$ ./rose/target/release/rose appel.rose                # >> carre(7) → 49
$ python3 hex.py                                       # encodage .rosebc : 73 octets
```

### 7.2 Références sources citées

- `rose/src/rir.rs:5-27` — pourquoi le RIR existe, 50× vs interpréteur, conservatisme de l'abaissement.
- `rose/src/rir.rs:36-88` — les 14 variantes de `Op`.
- `rose/src/rir.rs:119-183` — `Eabstrait`, `Entier(String)`, `CacheSlot` et `unsafe impl Sync`.
- `rose/src/rir.rs:193-211`, `336-553` — `CorpsRir` et l'abaissement (sauts numériques).
- `rose/src/rir.rs:559-566`, `923-1089` — la boucle d'exécution à `pc`.
- `rose/src/main.rs:85-256` — le dump texte `rose rir` ; `:545, 552-568, 570-611` — bundle `comp` ; `:830-839` — commande `rir`.
- `rose/src/jit.rs:29, 1513, 1765-1818, 3452` — consommation RIR par le JIT.
- `rose/docs/NATIF.md:8-11, 79-81` — émission machine depuis le RIR.
- `rose/docs/CAPACITES.md:61-84` — 4 oracles 150/150 et point fixe M4.5.
- `rose/docs/AUDIT_FINAL.md:51-54` — md5 identique du point fixe octet.
- `rose/src/front.rs:19-23, 397, 498` — `TyKind`/`Ty`, `Expr`, `Instr` (dérives uniquement).
- `rose/src/interp.rs:743-745, 1135-1170, 2156+` — intégration du RIR dans l'interpréteur.

### 7.3 Ce qui est mesuré / calculé / estimé

| Nature | Élément |
|---|---|
| **Mesuré** | md5 identiques du dump `rir` ; tailles 49 o (source) et 188 o (dump) ; `carre(7) = 49` ; absence de serde/GMP/GC/classloader/sandbox dans le code ; version 0.1.5 |
| **Calculé** | les 73 octets du `.rosebc` et le décodage de l'exemple (script déterministe, §3.5) |
| **Documenté ailleurs (constat daté 18/11)** | 150/150 sur 4 oracles, point fixe M4.5 `bdbef636…` |
| **ESTIMÉ / PROPOSÉ** | taille du vérificateur, gain de taille sur gros corps, gain de démarrage, toutes les étapes S0–S4 et leurs portes |

**Périmètre** : ce document est une étude. Il ne modifie aucun `.rs`, aucun
`Cargo.toml`, aucun `PLAN.md` ; il ne contient aucun commit. Le format `.rosebc` décrit
ici est une **proposition**, pas une fonctionnalité livrée.
