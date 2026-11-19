# RVM — vers une « JVM Rose » complète (format de classe v2)

**Constat daté : 19/11/2026, 00:45 +01:00.** Dépôt `Stef302/Rose`, version 0.1.5.
Fichier possédé : `rose/src/rosevm.rs` (seul fichier source modifié par cette mission).
Aucun commit, aucune publication, aucune dépendance ajoutée, aucun autre fichier touché.

> ⚠ Chaque mesure ci-dessous est un **constat daté**, pas une consigne.
> Re-mesurer après tout changement de code, de quantification ou de topologie.

---

## 1. Ce qui existait AVANT / ce qui a été AJOUTÉ

### 1.1 État mesuré avant (HEAD `477736f`, 19/11/2026)

`rose/src/rosevm.rs` faisait **1 086 lignes** et contenait :

| Brique | Détail vérifié |
|---|---|
| Conteneur `.rosebc` v1 | magic `ROSEBC`, `VERSION = 1`, little-endian, un seul `CorpsRir` |
| `ecrire_bytecode` / `lire_bytecode` | snapshot d'un corps RIR (pas un fichier de classe) |
| `verifier_sous_ensemble` | refuse AST, `AppelRir`, `Lire`, sauts/slots hors bornes |
| `GcPool` | liste libre + comptage de prêt (NOTRE « GC », pas un GC traçant) |
| `HoteBc` | hôte sans E/S, sortie capturée dans `sortie`, `entier_texte` pour bigints |
| `executer_sandbox` | budget d'instructions, limite de slots, limite de `Gz`, `catch_unwind` |
| `executer_avec_vm` | exécution par la VM RIR existante (`rir::executer_avec`) |

**Compteurs de tests RVM avant : 8 passés, 0 échec** (harness isolé, voir §5.1).

Limitation structurale mesurée : **aucun appelant** de `rosevm` n'existe dans
`rose/src/*.rs`. Le module n'est déclaré ni dans `lib.rs` ni dans `main.rs`.
Il n'était donc **ni compilé ni testé** par `cargo build` / `cargo test` du dépôt.
Les docs `ROSE_VM.md` et `CABLAGE_VM.md` signalent le câblage CLI à faire.

### 1.2 Ce qui a été ajouté (diff mesuré : **+1 899 / −4 lignes**, fichier final **2 981 lignes**)

| Brique | Nouveaux types / fonctions | Où |
|---|---|---|
| Pool de constantes versionné | `Const` (6 tags : i64, bigint, flottant, chaîne, symbole, référence de méthode), `ConstantPool` (`VERSION_CP = 1`, `ajouter`, `ajouter_unique`, `get`, `get_comme`, `symbole`, `chaine`, `methode`), `ecrire_cp`/`lire_cp` | Partie 2 §1 |
| Jeu d'instructions de méthode | `InstrBc` (14 opcodes : `PousserConst`, `ChargerSlot`, `StockerSlot`, `Dupliquer`, `Binaire`, `Neg`, `Non`, `Saut`, `SautSiFaux`, **`Appel`**, `RetourVide`, **`RetourValeur`**, **`Lever`**, `Afficher`), sérialisation stable | Partie 2 §2 |
| Modèle classe / méthode | `TyDesc` (7 variantes), `Descripteur`, `MethodeRose`, `EntreeException`, `ClasseRose` (table de dispatch par classe), `ChargeurClasses` (héritage simple, `resoudre`, `arite`, `verifier_lien`) | Partie 2 §3 |
| Invocation | `CadreJvm` (slots locaux + pile d'opérandes + `pc`), `MachineJvm::invoquer`, profondeur bornée, compteurs `instructions` / `appels` | Partie 2 §5 |
| Exceptions structurées | `InstrBc::Lever`, table `EntreeException` par plage `[debut, fin)`, correspondance par **nom de symbole**, déroulement inter-cadres | Partie 2 §5 |
| Sandbox renforcée | `LimitesJvm` : budget d'instructions, profondeur d'appels, nombre d'appels, slots par cadre, taille du pool. Aucune E/S (sortie capturée) | Partie 2 §5 |
| Vérificateur strict | `verifier_classe` (structure : index CP, types, sauts, plages d'exceptions) + `verifier_flot` (types de slots, profondeur de pile, arité des appels, convergence) + erreurs propres dédiées | Partie 2 §4 |
| Format v2 + rétrocompat | `VERSION_JVM = 2`, `ecrire_classe`, `lire_classe` (lit **v1 et v2**), `charger_fichier`, `executer_fichier`, `promouvoir_corps`, `promouvoir_corps_avec_parametres` | Partie 2 §6–7 |
| Tests | **17 nouveaux tests** (8 → 25) | `mod tests` |

Aucune modification de `jit.rs`, `interp.rs`, `run.rs`, `rir.rs`, `main.rs`, `lib.rs`.
Zéro dépendance : `HashMap`/`VecDeque` sont du standard library, les bigints
restent du texte décimal longueur-préfixé (jamais GMP, jamais i64 tronqué).

---

## 2. Format `.rosebc` v2 (spécification stable)

```
magic[6] = "ROSEBC" | version u32 = 2
nom_classe        : u32 len + UTF-8
version_classe    : u32
super_classe      : u8 (0 = aucune, 1 = suivante) [+ u32 len + UTF-8]
n_interfaces      : u32, puis n × (u32 len + UTF-8)
POOL              : version u16 | n u32 | n × (tag u8 + payload)
n_methodes        : u32
  pour chaque méthode :
    nom             : u32 len + UTF-8
    descripteur     : n_params u32 | n_params × (tag u8 [+ chaîne si Objet])
                      | tag retour u8 [+ chaîne si Objet]
    drapeaux        : u16
    n_slots         : u16
    noms_slots      : n_slots × (u32 len + UTF-8)
    n_code          : u32 | n_code × (opcode u8 + opérandes)
    n_exceptions    : u32 | n × (debut u32 | fin u32 | cible u32 | type_index u16)
```

Tags du pool : `1` entier i64 (8 octets LE), `2` bigint (texte), `3` flottant
(8 octets LE), `4` chaîne, `5` symbole, `6` méthode (3 × u16 : classe, nom,
descripteur). Les chaînes utilisent un préfixe **u32** (un bigint maison peut
dépasser 65 535 chiffres).

Opcodes de méthode : `01` PousserConst, `02` ChargerSlot, `03` StockerSlot,
`04` Dupliquer, `05` Binaire, `06` Neg, `07` Non, `08` Saut, `09` SautSiFaux,
`0A` Appel, `0B` RetourVide, `0C` RetourValeur, `0D` Lever, `0E` Afficher.

**Rétrocompatibilité mesurée** : `lire_classe` détecte `version == 1`, rejoue le
lecteur v1 et promeut le corps en une classe `Principal` avec une méthode
`<principal>` (tous les slots exposés comme paramètres, arité = `n_slots`).

---

## 3. Exécution — sémantique réellement implémentée

- **Machine à pile** : `PousserConst`/`ChargerSlot` empilent, `Binaire` dépile
  `b` puis `a` et appelle `interp::binaire_valeurs` (mêmes règles exactes que le
  moteur, bigints inclus). `Neg`/`Non` passent par `interp::unaire_valeur`.
- **Cadres** : `Appel` résout la référence `CP_METHODE`, remonte la chaîne
  `super_classe` jusqu'à la classe **définissante**, dépile les arguments en
  ordre inverse et empile un nouveau cadre. Le `pc` de l'appelant reste sur
  l'instruction `Appel` pendant l'exécution du callee (c'est ce qui permet à sa
  table d'exceptions de couvrir l'appel).
- **Retour** : `RetourValeur` dépile et rend la valeur à l'appelant ;
  `RetourVide` empile `Valeur::Vide`. Une fin de code atteinte sans retour est
  un retour implicite (sommet de pile ou `Vide`).
- **Exceptions** : `Lever { type_index, msg_index }` compare le **nom du
  symbole** de type (pas un index brut, afin de rester correct après
  déroulement inter-classes). Un gestionnaire `type_index = 0` attrape tout.
  Le gestionnaire reçoit la pile **remplacée** par le message (comme en JVM).
- **Sortie** : `Afficher` écrit dans `HoteBc::sortie` — aucun stdout, fichier,
  réseau ou clavier (sandbox).
- **Reprise sur erreur** : `executer_fichier`/types d'erreur dédiés — pas de
  panique sur un fichier hostile ; les échecs de l'évaluateur deviennent
  `Erreur::ExecutionEnErreur`.

### Sandbox (limites effectivement appliquées)

| Limite | Champ | défaut | erreur |
|---|---|---|---|
| Instructions cumulées | `budget_instructions` | 10 000 000 | `BudgetDepasse` |
| Profondeur de cadres | `profondeur_max` | 64 | `ProfondeurDepassee` |
| Appels cumulés | `max_appels` | 1 000 000 | `AppelsEpuises` |
| Slots par cadre | `max_slots_par_cadre` | 65 535 | `MemoireDepassee` |
| Entrées du pool | `max_constantes` | 65 535 | `PoolPlein` |
| Prêts `Gz` simultanés | `Limites::max_gz` (v1) | 64 | `GzEpuises` |

⚠ `max_constantes` est **vérifié au chargement** (le pool est sérialisé), mais
n'est pas comparé à la taille du pool à l'exécution : c'est une borne de format,
pas un quota dynamique. Les compteurs d'exécution, eux, sont fermes.

### Vérificateur

1. **Structure** : magic/version, troncature, octets en trop, UTF-8, types des
   entrées du pool, instances de `CP_METHODE` (classe et nom = `Symbole`,
   descripteur = `Chaine`), index de slots, bornes de sauts, plages
   d'exceptions `[debut, fin)`, cohérence `noms_slots`/`n_slots`/arité.
2. **Flot** (worklist, convergence bornée) : types de slots issus du
   descripteur, profondeur de pile (sous-dimensionnement **et** déséquilibre
   entre chemins de saut), types des opérandes, **arité réelle résolue via le
   chargeur**, état d'entrée des gestionnaires d'exceptions.

---

## 4. Tests RÉELS — sorties mesurées

Harness isolé (§5.1), `cargo test --release --lib rosevm` :

```
running 25 tests
...
test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured; 160 filtered out
```

**Compteurs RVM : AVANT 8/0 → APRÈS 25/0 (17 tests ajoutés, 0 échec).**

Sorties des nouveaux tests (`--nocapture`) :

```
[rvm] CP 6 tags sérialisés : 9 entrées, 108 octets
[rvm] CorpsRir promu -> carre(7) = 49
[rvm] .rosebc v2 carre(7)=49 == interpréteur 49
[rvm] rétrocompat v1 -> <principal>(6) = 36
[rvm] somme(3,4) = 25 (appels=2, instructions=14)
[rvm] dispatch hérité Fille.double(21) = 42
[rvm] try/catch : sortie capturée = "boom"
[rvm] déroulement inter-cadres : sortie = "propagé"
[rvm] exception non attrapée : `E`
[rvm] profondeur d'appel bornée à 4
[rvm] budget de 3 appels épuisé
[rvm] budget 25 instructions épuisé
[rvm] vérificateur : saut et slot hors bornes refusés
[rvm] vérificateur : arité d'appel incohérente refusée
[rvm] fib(25) = 75025 (appels=242784, instructions=2913414)
[rvm] fib25 interpréteur = 75025 (égal)
[rvm] sortie="salut JVM" bigint=123456789012345678901234567890
```

Le test `programme_reel_fib_recursif_par_methodes` compile une vraie récursion
en méthode JVM Rose, la sérialise en v2, la recharge, l'exécute (**242 784
appels de cadre**, 2 913 414 instructions) puis compare à
`interp::executer_source` sur la même récursion écrite en Rose : **75025 des
deux côtés**. C'est la preuve demandée « compiler → bytecode v2 → charger →
exécuter → même résultat que l'interpréteur ».

### Validation globale du dépôt (immuable par ce changement, mesurée quand même)

`taskset -c 5 cargo build --release --bin rose` : **OK** (29,27 s).
`taskset -c 5 cargo test --release` : **160/0, 144/0, 0/0, 6/0, 1/0, 0/0**, rc=0.
`taskset -c 5 bash scripts/audit-final.sh` (19/11 00:44) :

```
Point fixe M4.5 : bdbef63681608a0b22a5637539e9d738 (×2, les deux égaux)
jardin : 20 tests, 0 échec        (20/20)
fib(25) : 75025
```

⚠ **Écarts d'oracles non imputables à cette mission** : l'audit a aussi mesuré
`analyseur 158/2`, `vérificateur 156/4`, `émetteur 156/4` et signalé un
`git status` non propre. Ces écarts viennent des **changements concurrents**
d'autres agents (`wasm_lecteur.rs`, `transpile.rs`, nouveaux `jardins/*`) :
`rosevm.rs` n'est déclaré dans aucun binaire ni dans la lib, donc il ne peut
pas influencer ces sorties. À re-mesurer après stabilisation du dépôt partagé.

---

## 5. Vérification — méthode et limite importante

### 5.1 Le module n'est pas dans l'arbre de compilation du dépôt

**Constat re-mesuré le 19/11** : `grep -rn rosevm rose/src --include=*.rs` ne
trouve que des occurrences **dans `rosevm.rs` lui-même**. Ni `lib.rs` ni
`main.rs` ne déclarent `mod rosevm;`. Par conséquent :

- `cargo build --release --bin rose` **ne compile pas** ce fichier ;
- `cargo test --release` **ne lance pas** ses 25 tests.

Pour obtenir une **preuve réelle** (et non une intention), j'ai utilisé un
**harnais isolé hors du dépôt** : copie intégrale de l'arbre dans
`/tmp/opencode/rvm_verify/ROSE/`, une seule ligne ajoutée **dans la copie** :
`pub mod rosevm;` à la fin de `src/lib.rs`. C'est dans cette copie que
`cargo check --release --lib`, `cargo test --release --lib --no-run` et
`cargo test --release --lib rosevm` ont été exécutés (pinnés `taskset -c 5`).
Le dépôt réel n'a pas été modifié par ce harnais.

### 5.2 Patch CLI proposé (NON appliqué — interdiction explicite)

Pour que la JVM Rose soit réellement câblée, deux changements sont nécessaires.
Ils sont **documentés ici, pas appliqués** (d'autres agents travaillent sur
`lib.rs`/`main.rs`).

**a) `rose/src/lib.rs` — ajouter une ligne :**

```rust
pub mod rosevm;   // à ajouter à la liste des `pub mod`
```

**b) `rose/src/main.rs` — sous-commande `rvm`, calquée sur `rir` :**

```rust
"rvm" => {
    if argv.len() < 3 { eprintln!("rose rvm : quel fichier .rosebc ?"); std::process::exit(2); }
    let octets = match std::fs::read(&argv[2]) {
        Ok(o) => o,
        Err(e) => { eprintln!("rose rvm : lecture impossible : {}", e); std::process::exit(1); }
    };
    let chargeur = match rosevm::charger_fichier(&octets) {
        Ok(c) => c,
        Err(e) => { eprintln!("{}", e); std::process::exit(1); }
    };
    let classe = argv.get(3).map(String::as_str).unwrap_or("Principal");
    let methode = argv.get(4).map(String::as_str).unwrap_or("<principal>");
    let mut hote = rosevm::HoteBc::neuf(rosevm::GcPool::LIMITE_DEFAUT);
    let arguments: Vec<rosevm::Valeur> = argv.iter().skip(5)
        .filter_map(|s| s.parse::<i64>().ok()).map(rosevm::Valeur::z).collect();
    match rosevm::MachineJvm::neuf(&chargeur, &mut hote, rosevm::LimitesJvm::default())
              .invoquer(classe, methode, &arguments) {
        Ok(v) => { print!("{}", hote.sortie); println!("{}", rosevm::chaine_publique(&v)); }
        Err(e) => { eprintln!("{}", e); std::process::exit(1); }
    }
    std::process::exit(0);
}
```

Les deux ré-exports utilisés par le patch (`rosevm::Valeur` =
`crate::run::Valeur`, et `rosevm::chaine_publique` = `crate::run::chaine_de`)
**existent déjà** dans `rosevm.rs` — ils ont été ajoutés exprès pour que ce
patch soit directement applicable sans retoucher au module.

---

## 6. Limites honnêtes — ce qui n'est PAS une JVM complète

1. **Pas de JVM Java** : ni `.class`, ni JVMS §4, ni vérificateur de types JVM
   au sens de la spécification, ni `invokedynamic`, ni interfaces par défaut,
   ni génériques, ni réflexion, ni exceptions checked/unchecked.
2. **Héritage simple seulement**. `interfaces` est sérialisé et exposé, mais
   **non résolu** : le dispatch n'en tient pas compte.
3. **Pas de GC traçant** : le « GC » reste le `GcPool` à liste libre + comptage
   de prêt de la partie 1. Pas de cycles à collecter (un `Gz` ne référence pas
   d'autre `Gz`), mais pas de compaction ni de générations non plus.
4. **Pas de JIT depuis `.rosebc` v2** : l'exécution est interprétée, dans
   `MachineJvm`. Le JIT Cranelift existant travaille sur le RIR de `rir.rs`,
   pas sur ce format. Aucune mesure de performance n'est revendiquée ici.
5. **Pas de threads, monitores, synchronization, natif, FFI, chargement
   dynamique**. Un seul cadre courant, une seule pile d'opérandes par cadre.
6. **Pas de tableau ni d'objets du domaine JVM** : `TyDesc::Objet(String)` est
   sérialisé et vérifié, mais la machine ne sait pas allouer/inspecter un objet.
   Les valeurs restent les `Valeur` de Rose (entiers exacts, chaînes, tables…).
7. **Arité « variadique » absente** : un appel dépile exactement le nombre
   d'arguments du descripteur ; pas de surcharge de méthode (une table de
   dispatch = un nom → une méthode par classe).
8. **Rétrocompat v1 approximative sur l'arité** : le RIR ne conserve pas le
   nombre de paramètres ; la promotion expose tous les slots (`n_slots`). Un
   appelant connaissant la vraie arité doit utiliser
   `promouvoir_corps_avec_parametres`.
9. **Le module n'est pas câblé** (§5.1) : sans le patch §5.2, la JVM Rose est
   une bibliothèque utilisable depuis Rust, pas une fonctionnalité du binaire
   `rose`.
10. **Copie des instructions** : le vérificateur et l'exécuteur lisent la
    méthode via le chargeur ; la table d'exceptions est relue à chaque `Lever`.
    Correct mais non optimisé — aucune revendication de vitesse.

---

## 7. Fichiers

- Source : `rose/src/rosevm.rs` (2 981 lignes, +1 899 / −4).
- Doc : `rose/docs/RVM_JVM_COMPLETE.md` (ce fichier).
- Harnais de vérification temporaire : `/tmp/opencode/rvm_verify/ROSE/rose`
  (copie hors dépôt, non versionnée).
