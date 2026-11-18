# Noyau « JVM Rose » — bytecode `.rosebc`, chargeur vérifié, VM RIR, GC minimal, sandbox

> **Statut** : module `rose/src/rosevm.rs` **livré et testé** (8 tests verts). Le
> **câblage CLI est à faire par l'orchestrateur** : ce document décrit
> l'API existante et les commandes à brancher, il ne prétend pas que `rose bc`
> ou `rose runcarre.rosebc` fonctionnent déjà.
> **Cible** : Rose 0.1.5 — dépôt `Stef302/Rose`. **Constat daté** : 18/11/2026.
> Aucune dépendance ajoutée (ni serde, ni GMP), aucun autre fichier modifié,
> aucun commit.

---

## 1. Ce que c'est — et ce que ce n'est pas

Rose exécute déjà un **bytecode interne**, le RIR (`rose/src/rir.rs`) : opérations
linéaires, slots indexés, sauts numériques, boucle à `pc`, JIT Cranelift et
émetteur natif. Ce module ajoute la couche qui manquait pour parler de bytecode
*au sens Java* : **un format binaire stable, un chargement sans source, un
vérificateur à l'entrée** — plus une exécution bornée.

| | Ce que fait ce module | Ce qu'il ne fait PAS |
|---|---|---|
| Format | Snapshot binaire versionné d'un `CorpsRir` (magic `ROSEBC`, version u32) | Nouvel IR, nouvelle sémantique |
| Chargement | `lire_bytecode` reconstruit un `CorpsRir` sans lire le `.rose` | Chargeur de classes, lien dynamique |
| Vérification | Refus propre de magic/version/sauts/slots/opcodes/AST | Vérificateur de types JVM, héritage |
| Exécution | VM RIR existante (`rir::executer_avec`, `rir::executer_avec_ptr`) via un hôte minimal `HoteBc` | Compilation JIT depuis `.rosebc` (étape S4 de `BYTECODE_ROSE.md`, non faite) |
| Mémoire | Pool de `Gz` avec **comptage de prêt**, limite ferme | **GC générationnel**, ramassage par traçage, collecte de cycles |
| Isolement | Sandbox : slots bornés, budget d'instructions, zéro E/S, zéro appel | Isolation de processus, capacités fines, sandbox OS |

**Formulation exacte à employer** : « Rose exécute un IR linéaire (le RIR). Un
format `.rosebc` optionnel en enregistre un snapshot versionné, rechargeable
sans la source et exécutable dans une sandbox bornée. **Ce n'est pas une JVM** :
pas de GC générationnel, pas de chargeur de classes, pas de JIT depuis le
bytecode. »

---

## 2. Format v1 (`ROSEBC`, version 1, little-endian)

Un seul corps par fichier. Ce n'est **pas** le conteneur multi-corps de
l'étude `BYTECODE_ROSE.md` §3 (qui décrivait 73 octets pour deux corps avec
signatures) : c'est un conteneur minimal, volontairement plus simple, qui
sérialise exactement les quatre champs de `CorpsRir`.

| Offset | Taille | Champ |
|---|---|---|
| `0x00` | 6 | magic ASCII `ROSEBC` (`52 4F 53 45 42 43`) |
| `0x06` | 4 | version `u32` = `1` (incompatible ⇒ refus) |
| `0x0A` | 4 | `n_slots` `u32` |
| `0x0E` | 4 | `n_ops` `u32` |
| `0x12` | 4 | `n_globaux` `u32` |
| `0x16` | — | `noms` : `n_slots` × (`u32 len` + UTF-8) |
| | | `globaux` : `n_globaux` × `u16` |
| | | `ops` : `n_ops` × (`u8 opcode` + opérandes) |

### Opcodes

`Op` (`rir.rs`) : `0x01 Affect` (slot `u16` + expr), `0x02 AfficherAbstrait`
(expr), `0x03 Lire` **réservé mais refusé** (entrée clavier), `0x04 Saut`
(cible `u32`), `0x05 SautSiFauxAbstrait` (expr + cible `u32`),
`0x06 RetourAbstrait` (expr), `0x07 RetourVide`, `0x80` = **AST refusée**.

`Eabstrait` : `0x10 Slot(u16)`, `0x11 Entier(u32 len + texte décimal)`,
`0x12 Flottant(f64 IEEE-754 LE)`, `0x13 Binaire(u8 oper + expr + expr)`,
`0x14 Neg(expr)`, `0x15 Non(expr)`, `0x16 AppelRir` **refusé**.

`Oper` : `0x01` add … `0x0F` ou (table complète dans `rosevm.rs`).

⚠ Deux choix de format à connaître :
- le **`CacheSlot(Cell)` n'est jamais sérialisé** : `Slot(s, _)` est reconstruit
  en `Slot(s, CacheSlot::neuve())` au chargement. Sérialiser le pointeur
  mémorisé produirait un dangling pointer.
- les longueurs de chaînes sont en **`u32`**, pas `u16` comme proposé dans
  l'étude : un entier exact peut dépasser 65 535 chiffres, et un préfixe `u16`
  tronquerait silencieusement. Les littéraux restent du **texte décimal**,
  jamais un i64 fixe — zéro GMP, arithmétique exacte préservée.

### Sous-ensemble binaire (v1)

Sont **acceptés** : affectations et retours d'expressions abstraites,
affichage abstrait, sauts numériques, tests abstraits. Sont **refusés par le
vérificateur** (marqués `0x80` à l'écriture, rejetés au chargement) : les 8
variantes de `Op` qui portent un `Expr`/`Instr` (`AffectAst`, `AffectIndex`,
`Afficher`, `SautSiFaux`, `Retour(Some)`, `ExprStmt`, `Lever`, `Bloc`),
`AppelRir`, `Lire`. C'est ce refus qui garantit l'absence d'accès fichier,
réseau, clavier ou FFI depuis un bytecode : aucune opération ne peut charger
une bibliothèque ni appeler une fonction.

---

## 3. API Rust (existante)

```rust
use rose::rosevm::{
    ecrire_bytecode, lire_bytecode, verifier_sous_ensemble,
    executer_avec_vm, executer_sandbox, HoteBc, Limites, Erreur,
};
use rose::run::Valeur;
use rose::rir::FinRir;

// 1. Produire (depuis un CorpsRir déjà abaissé par rir::abaisser)
verifier_sous_ensemble(&corps)?;          // recommandé avant distribution
let octets: Vec<u8> = ecrire_bytecode(&corps);

// 2. Charger + vérifier (aucun panic sur fichier hostile)
let charge = lire_bytecode(&octets)?;

// 3a. Exécuter via la VM RIR existante (chemin « nu », sans budget)
let mut hote = HoteBc::neuf(64);
let fin: FinRir = executer_avec_vm(&charge, &[Valeur::z(7)], &mut hote);
let sortie_capturee = &hote.sortie;       // pas de stdout

// 3b. Exécuter en sandbox (chemin sûr pour du bytecode non fiable)
let limites = Limites { budget_instructions: 1_000_000, max_slots: 256, max_gz: 32 };
let mut hote = HoteBc::neuf(limites.max_gz);
let valeur: Valeur = executer_sandbox(&charge, &[Valeur::z(7)], &mut hote, &limites)?;
```

`Erreur` implémente `Display` + `std::error::Error`. Variantes notables :
`MagicInvalide`, `VersionInsupportee`, `Tronque`, `OctetsEnTrop`,
`SlotHorsBornes`, `SautHorsBornes`, `OpcodeInconnu`, `OperationRefusee`,
`MemoireDepassee`, `BudgetDepasse`, `GzEpuises`, `PaniqueInterne`.

---

## 4. Commandes à câbler (PAS implémentées ici)

L'orchestrateur doit ajouter dans `main.rs` / `lib.rs` (hors de mon périmètre) :

| Commande | Rôle | Liaison suggérée |
|---|---|---|
| `rose bc prog.rose [-o prog.rosebc]` | analyser + `rir::abaisser` + `verifier_sous_ensemble` + `ecrire_bytecode` | écrire les octets returned par `ecrire_bytecode` |
| `rose run prog.rosebc` | `lire_bytecode` puis `executer_sandbox` | afficher `hote.sortie` |

⚠ Le câblage CLI n'est **pas fait** : ne pas annoncer ces commandes comme
disponibles. Le module Rosé est compilable isolément (harnais décrit §7).

---

## 5. GC minimal — stratégie réelle

**Liste libre + comptage de prêt**, pas un GC traçant.

Un `Gz` (`i128x.rs:457-587`) est une poignée possédée sur un `Box<Mpz>`, avec
un `Drop` qui libère la boîte. Il ne contient aucune référence vers un autre
`Gz` : **il n'y a pas de cycle à collecter**. `GcPool` (`rosevm.rs`) gère trois
états :

- `prendre()` : prélève une boîte libre ou alloue via `Gz::neuf()`, et
  incrémente `prets`. Refuse au-delà de `limite` (`Erreur::GzEpuises`) ;
- `rendre(g)` : décrémente `prets` et remet la boîte en réserve (cap 64) ;
- `echapper()` : comptabilise un `Gz` qui **quitte le pool** pour devenir une
  `Valeur::Entier(_, Some(Rc<Gz>))` ; sa vie est alors gérée par le comptage de
  références Rust, exactement comme dans `gz_pool.rs` de l'interpréteur.

Le bytecode ne peut pas allouer de `Gz` : aucun opcode d'allocation n'existe
dans le sous-ensemble. Seul `HoteBc::entier_texte` passe par le pool pour
matérialiser un littéral hors i64. Le reste de l'arithmétique délègue à
`interp::binaire_valeurs` / `interp::unaire_valeur` (règles exactes du moteur).

**Ce que ce n'est pas** : pas de marquage, pas de balayage, pas de générations,
pas de compaction, pas de finaliseurs. Le mot « GC » ici signifie « gestion
bornée des temporaires entiers exacts », rien de plus.

---

## 6. Sandbox — contrôles effectifs

1. **Mémoire** : `n_slots > Limites::max_slots` ⇒ `Erreur::MemoireDepassee`.
   Le chargeur borne aussi la taille de ses réservations par la taille du
   fichier (un `n_slots` énorme n'alloue pas un `Vec` énorme).
2. **Budget d'instructions** : chaque opération consomme un pas ; au-delà de
   `budget_instructions` ⇒ `Erreur::BudgetDepasse` (boucle infinie arrêtée).
3. **Pas d'E/S** : le vérificateur refuse `Lire` (clavier), toute opération à
   AST (qui pourrait apporter FFI/`@dll`/`spawn`) et `AppelRir`. `HoteBc`
   n'écrit jamais sur stdout : `AfficherAbstrait` capture dans `hote.sortie`.
   Aucun fichier, aucun réseau, aucun appel système depuis le bytecode.
4. **Cohérence mémoire** : index de slots et cibles de sauts vérifiés au
   chargement ; profondeur d'expression bornée à 64 (anti-récursion hostile) ;
   `CacheSlot` reconstruit à vide.
5. **Paniques** : `executer_sandbox` enveloppe l'exécution dans
   `catch_unwind` : une erreur runtime Rose devient `Erreur::PaniqueInterne`
   au lieu de faire tomber le processus hôte.

⚠ **Limite de la sandbox** : elle est logique, pas système. Elle protège contre
un bytecode malformé ou une boucle infinie, mais pas contre un hôte qui
fournirait lui-même des primitives dangereuses. Ce n'est pas une isolation
de processus.

---

## 7. Ce que ça apporte — avec le statut de preuve

| Bénéfice | Statut réel |
|---|---|
| Distribution sans source | **Implémenté** : `.rosebc` ne contient que le snapshot RIR, pas le texte `.rose`. ⚠ Le bytecode est **réversible** : ce n'est ni du chiffrement ni une protection. |
| Exécution vérifiée | **Implémenté et testé** : 8 rejets propres (magic, version, saut, slot, troncature, octets en trop, AST, budget). |
| Sandbox bornée | **Implémentée et testée** : budget, slots, `Gz`, zéro E/S. |
| Démarrage plus rapide | **Hypothèse NON MESURÉE**. Le chargement évite lexage + analyse + vérification + abaissement, mais aucun A/B (≥ 6 échantillons, médianes) n'a été fait. Ne pas annoncer de gain chiffré. |
| JIT depuis `.rosebc` | **Non implémenté** (étape S4 de `BYTECODE_ROSE.md`). Les consommateurs restent la VM RIR et, via le `CorpsRir` reconstruit, les chemins JIT/natif existants — à câbler séparément. |

Taille mesurée : `carre` (`fn carre(n: entier) -> entier { retour n * n }`) =
**36 octets** en `.rosebc` v1 (22 d'en-tête + 5 de nom + 9 d'opération), contre
49 octets de source et 188 octets de dump texte `rose rir` (mesuré dans
`BYTECODE_ROSE.md`). Les 73 octets cités dans cette étude portaient sur un
**autre conteneur** (deux corps + signatures) : les deux chiffres ne sont pas
comparables directement.

---

## 8. Limites à dire clairement

- **Un seul corps par fichier** ; pas de table de signatures, pas de nom de
  fonction, pas de type de retour dans l'en-tête (contrairement au §3.3 de
  l'étude).
- **Sous-ensemble réduit** : 7 opcodes sur 14 ; les fonctions les plus
  complexes restent sur la voie source `.rose`.
- **Pas de paramètres dans le fichier** : l'appelant fournit les arguments via
  `executer_avec_vm` / `executer_sandbox` (slots de tête).
- **Globaux non synchronisés** : la sandbox est isolée ; `corps.globaux` est
  sérialisé pour fidélité du round-trip, mais `synchroniser_slots` est un no-op
  dans `HoteBc`.
- **Pas de classloader, pas de GC générationnel, pas de JIT depuis le bytecode,
  pas de CLI câblée.**
- **Format figé à la version 1** : la release reste 0.1.5 ; tout changement
  d'opcode doit incrémenter `VERSION` et casser les anciens fichiers proprement.

---

## 9. Preuve d'exécution (sortie réelle)

Harnais isolé : copie de la crate dans `/tmp/opencode/rosevm-harness`, `lib.rs`
de la copie avec `pub mod rosevm;`, `crate-type = ["rlib"]` (pour éviter le
SIGSEGV du cdylib sur x86_64 documenté dans `.cargo/config.toml`), vrais modules
`front`/`rir`/`run`/`interp`/`jit` de la crate, `cargo test --lib rosevm -- --nocapture` :

```
running 8 tests
[rosevm] GC pool : total=2 libres=1 echappes=1
test rosevm::tests::gc_pool_limite_et_recyclage ... ok
[rosevm] saut hors bornes refusé : cible=999 n_ops=1
test rosevm::tests::refus_saut_hors_bornes ... ok
[rosevm] budget épuisé proprement
[rosevm] taille .rosebc pour carre = 36 octets
test rosevm::tests::sandbox_refuse_ast_et_boucle_infinie ... ok
test rosevm::tests::refus_troncature_octets_en_trop_et_slot_hors_bornes ... ok
test rosevm::tests::sandbox_limite_slots ... ok
[rosevm] RIR source  : carre(7) = 49
[rosevm] RIR .rosebc : carre(7) = 49
[rosevm] sandbox carre(9) = 81
test rosevm::tests::execution_sandbox_capture_sans_es ... ok
test rosevm::tests::refus_magic_et_version ... ok
[rosevm] interpréteur : rc=0 sortie="49\n"
test rosevm::tests::round_trip_carre_chargement_execution_et_parite ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 160 filtered out; finished in 0.01s
```

Ce que cette sortie prouve : sérialisation puis rechargement d'un corps réel
abaissé, exécution par la VM RIR existante, **parité avec l'interpréteur complet**
(`sortie = "49\n"`), exécution sandboxed (`carre(9) = 81`), et refus effectif
d'un bytecode corrompu. Ce qu'elle ne prouve **pas** : le gain de démarrage, la
voie JIT, ni l'existence des commandes CLI.

---

## 10. Périmètre

Fichiers livrés : `rose/src/rosevm.rs`, `rose/docs/ROSE_VM.md`.
Aucun autre `.rs`, `Cargo.toml`, `main.rs`, `lib.rs` ou `PLAN.md` modifié.
Aucun commit. Aucune dépendance ajoutée. Release inchangée : 0.1.5.
