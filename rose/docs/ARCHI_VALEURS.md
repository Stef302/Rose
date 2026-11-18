# L1 — Architecture des valeurs : suppression des indirections sur le chemin chaud

**Horodatage** : 2026-11-18, salve 15:47 → 16:10 UTC+1. **Constat daté**, pas une consigne.

**Périmètre** : `rose/src/run.rs` uniquement (12 insertions, 4 suppressions). Aucun commit.
`jit.rs`, `interp.rs`, `rir.rs`, `Cargo.toml`, `PLAN.md` intacts par cette mission
(ils ont été modifiés concurremment par d'autres agents — voir « contamination »
ci-dessous). Aucun poids, cache ni dossier modèle supprimé.

---

## 1. Architecture avant

`Valeur` (run.rs:40) est un enum à 12 variantes :

```rust
pub enum Valeur {
    Vide,
    Entier(i64, Option<Rc<Gz>>),          // i64 inline, grand entier boxé
    Rationnel(Rc<Gq>), Flottant(f64), Booleen(bool),
    Chaine(Rc<String>),
    Liste(Rc<RefCell<Vec<Valeur>>>), Table(Rc<RefCell<Vec<(Valeur,Valeur)>>>),
    ...
}
```

Le boxage des *données* était déjà réduit (i64 inline, `Rc` pour le reste). Le coût
architectural résiduel identifié est le **boxage des appels à l'API de valeurs** :
`valeur_egale`, `cmp_vals`, `vers_z`, `convertir` sont des fonctions `pub` non inlinées,
appelées des MILLIONS de fois par les boucles de l'interpréteur. Chaque appel paie :
sauvegarde de registres, branchement, et — pour `vers_z` — un accès thread-local
`LIGNE_COURANTE.with(...)` **mort** (calculé puis jamais lu, sauf dans le message
d'erreur qui le relit).

Mesure instrumentée (run.rs temporairement instrumenté, puis restauré) sur
`r_table.rose` : **9 000 000 appels à `valeur_egale`, 100 % sur la voie i64/i64**,
et 100 000 appels à `vers_z` sur `r_list.rose` (3,39 ms cumulés, ~34 ns/appel).

---

## 2. Protocole de mesure

- Binaire construit depuis la source courante avec `cargo build --release`
  (rustc 1.99.1), **A/B/C entrelacés par rotation des domaines et des bras**,
  **9 échantillons par bras**, pinés sur `taskset -c 8` (X925, cœur oisif mesuré
  par `mpstat`). Les cœurs 15/18/19 étaient occupés par d'autres agents pendant
  une partie de la salve et ont été écartés après mesure.
- Horloge **interne Rose** (`temps()`, ns) : calcul seul, parsing/affichage exclus.
- Résultat vérifié à chaque échantillon : `20000`, `4999950000`, `3000`.
- Sources : `/tmp/opencode/benchmulti/r_{str,list,table}.rose` ; harnais :
  `/tmp/opencode/archi_valeurs/{measure,ab,ab3}.py`.

---

## 3. Expériences A/B/C (médianes, cpu8, n=9)

| Variante | chaîne 20k | liste 100k | table 3000 |
|---|---:|---:|---:|
| **A — origine** (`git HEAD` de run.rs) | 14,378 ms | 236,320 ms | 22,601 ms |
| **B — `#[inline(always)]` sur `valeur_egale`/`cmp_vals`/`vers_z` + `#[inline]` sur les constructeurs/conversions + suppression du thread-local mort** | 14,449 ms | 238,813 ms | **12,607 ms (1,79 ×)** |
| **C — B + comparaison directe `BigInt`↔i64 sans `vers_z` (boxé/boxé, boxé/i64)** | 14,389 ms | 236,889 ms | 13,908 ms (1,63 ×) |

**Variante retenue : B.** La variante C a été **rejetée par la mesure** : les 9 M
comparaisons de `r_table.rose` portent toutes sur des i64 inline, donc les nouveaux
branches boxés sont du code mort qui gonfle le corps inliné et coûte ~1,3 ms.
C'est un résultat honnête : l'optimisation « éviter le boxage des Gz » paraissait
pertinente sur le papier, elle est **plus lente** sur ce domaine.

Variante B détaillée (autre salve, même protocole) :

| Domaine | origine | retenu | gain | dispersion retenue |
|---|---:|---:|---:|---|
| chaîne 20 000 car. | 14,380 ms | 14,341 ms | 1,003 × (bruit) | 14,3–14,5 |
| liste 100 000 entiers | 236,070 ms | 235,094 ms | 1,004 × (bruit) | 234,9–235,8 |
| table 3 000 clés | 22,627 ms | **12,527 ms** | **1,81 ×** | 12,4–12,6 |

---

## 4. Pourquoi 1 seul domaine bouge (diagnostic mesuré)

Le goulot n'est **pas** dans `run.rs` pour les deux autres domaines :

- **Chaîne** : la concaténation est `Valeur::s(format!("{}{}", x, y))` dans
  `interp.rs:2735`. Le `format!` copie ~200 Mo cumulés (20 000 concaténations de
  longueur croissante), coercition O(n²) dans la bibliothèque standard. `Valeur::s`
  ne reçoit que le `String` déjà fabriqué : aucune optimisation de représentation
  dans run.rs ne peut supprimer cette copie.
- **Liste** : variantes décomposées (même binaire) — `joindre` seul = 72 ms,
  boucle de somme avec `$l[$j]` = +113 ms, boucle i64 sans index = +16 ms.
  `Expr::Index` n'est **pas abaissé en RIR** (`rir.rs::abstraire` rend `None`),
  donc toute la boucle de somme retombe dans l'interpréteur AST (~1,16 µs/itération).
  `vers_z` n'y pèse que 3,39 ms sur 236 ms (1,4 %).
- **Table** : 9 M appels à `valeur_egale` en scans linéaires ; l'inlining de
  l'API de valeurs agit **directement** sur ce chemin, d'où le 1,81 ×.

### Salve finale (source gelée, A/B reconstruits dos à dos, cpu8, n=9)

Dernière salve, la plus propre : `interp.rs` et `wasm_lecteur.rs` ont été
vérifiés **octet pour octet identiques** avant et après les deux builds
(`/tmp/opencode/archi_valeurs/freeze_final*.sha`) :

| Domaine | A — origine | B — retenu | gain |
|---|---:|---:|---:|
| chaîne 20 000 car. | 14,330 ms | 14,274 ms | 1,004 × (bruit) |
| liste 100 000 entiers | 236,597 ms | 235,217 ms | 1,006 × (bruit) |
| table 3 000 clés | 22,589 ms | **12,507 ms** | **1,807 ×** |

**Verdict partiel** : l'objectif « ≥ 2× sur au moins 2 des 3 domaines » n'est
**pas atteint** avec le seul fichier possédé. Un deuxième facteur ×2 exigerait
`interp.rs` (concaténation en place / index RIR) ou `rir.rs` (abaissement de
`Expr::Index`) — fichiers explicitement interdits à cette mission.

---

## 5. Validation sémantique (sorties réelles)

| Gate | Commande | Résultat |
|---|---|---|
| build | `cargo build --release` | rc=0 |
| jardin | `rose jardins/tests/premiers.rose` | `20 tests, 0 échec` |
| fib25 | `rose /tmp/.../fib25.rose` | `75025` |
| point fixe M4.5 | `rose rir compilateur/emetteur.rose` \| md5sum | `bdbef63681608a0b22a5637539e9d738` |
| point fixe M4.5 | `rose compilateur/emetteur.rose compilateur/emetteur.rose` \| md5sum | `bdbef63681608a0b22a5637539e9d738` |
| oracles | `bash scripts/valider-chantiers.sh` (binaire A vs binaire B) | **165/168 identiques avant/après** (10/10, 47/48, 63/63, 45/48) |
| tests | `cargo test --release` | 6 + 1 + 0 tests, 0 échec |

⚠ À 16:13, un autre agent a laissé `interp.rs` dans un état temporairement
non compilant (`E0308` en `interp.rs:944`, `appel_slots_sans_trame` absent).
Ce n'est **pas** un effet de L1 : tous les binaires de cette salve ont été
construits avant cet edit, et `cargo test --release` était passé (6+1 tests,
0 échec) sur l'état compilable. À re-vérifier quand l'autre agent aura fini.

Les 3 écarts oracles (1 analyseur, 3 émetteur sur le corpus courant) sont
**préexistants** : la sortie est **octet pour octet identique** entre le binaire
d'origine et le binaire modifié, construits l'un après l'autre depuis la même
source `interp.rs`. Le chiffre historique 151/151 n'est plus reproductible sur
l'état courant du dépôt (d'autres agents modifient `interp.rs` / `wasm_lecteur.rs`).

---

## 6. Ce qui reste

1. **Réduction de `Valeur` (24 octets)** : l'i64 inline et le `Rc<Gz>` optionnel
   occupent 16 octets de charge + 8 de tag. Passer à 16 octets exigerait de
   changer les motifs `Valeur::Entier(x, None)` / `Some(o)` dans `interp.rs` —
   hors périmètre. Gain attendu sur la copie des slots, non mesuré ici.
2. **Rope de chaînes / append en place** : nécessite de modifier l'opérateur `+`
   dans `interp.rs`, pas seulement `Valeur::s`.
3. **Abaissement RIR de `Expr::Index`** : changerait la donne sur la liste
   (236 ms → potentiellement < 20 ms), mais c'est le chantier L2.
4. **Table `HashMap`** : l'insertion reste O(n²) dans `interp.rs` (4,5 M scans) ;
   l'arène `Gz` (M2.3) reste bloquée par `Drop for Gz` dans `i128x.rs`.
5. La comparaison avec Rust reste défavorable : en reprenant les temps Rust de
   `BENCH_MULTI.md` (0,014 / 0,202 / 0,097 ms), Rose est à ~1021× sur chaîne,
   ~1163× sur liste et ~129× sur table (avant : ~233×). **Aucune prétention
   de battre Rust ici.**

---

## 7. Fichiers et preuves

- Code : `rose/src/run.rs` (seul fichier .rs modifié par L1).
- Binaires A/B/C : `/tmp/opencode/archi_valeurs/r3_{orig,inline_only,full}`.
- Harnais et résultat brut : `/tmp/opencode/archi_valeurs/ab3.py`,
  `ab3_result.txt`, `baseline_core8.txt`.
- Variante C (rejetée) : `run.rs.full`, message de rejet en §3.
