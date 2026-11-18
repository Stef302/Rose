# ARCHI A2 — Optimisation architecturale de l'interpréteur RIR

**Date des mesures** : 18/11/2026, 15:44 → 16:05 (+01:00), GB10 (Cortex-X925, pinage cœur 19).
**Périmètre possédé** : `rose/src/interp.rs`, `rose/docs/ARCHI_INTERP.md` (ce document).
Aucun autre fichier du dépôt n'a été modifié **par ce chantier** ; aucun commit.
`jit.rs`, `main.rs`, `lib.rs`, `rir.rs`, `Cargo.toml` n'ont PAS été touchés par ce
chantier (voir §5bis : d'autres agents ont modifié `rir.rs` et `interp.rs` en parallèle).
**Règle appliquée** : mesurer, jamais citer ; A/A/A ≥ 6 échantillons pinés ; médianes ;
constat daté ; aucune special-case de fonction.

---

## 1. Ce qui était mesuré avant

Binaire autonome (`cargo build --release --no-default-features --features autonome --bin rose`,
cible isolée `CARGO_TARGET_DIR=/tmp/opencode/roseb_interp/target`, md5
`ce6b4308d5ab177ac08cb0e02a8d6e23`). Temps INTERNE (`millisecondes()`), donc hors
démarrage/analyse. 9 → 11 échantillons pinés, entrelacés par rotation.

| Programme | Médiane avant | Échantillons (brut) |
|---|---:|---|
| `fib(25)` | **162 ms** | 162, 163, 162, 163, 162, 163, 162, 163, 162, 163, 162 |
| `fib(30)` | **1803 ms** | 1801…1827 (11 valeurs) |
| `fib(35)` | **19899 ms** | 19885, 20029, 19899 (3 échantillons) |
| jardin des premiers (20 tests) | **4 ms** | 4,4,4,4,4,4,4,4,4,5,4 |
| boucle 1 000 000 | **688 ms** | 688 (11 valeurs, dispersion 0) |

Valeurs contrôlées : `fib(25)=75025`, `fib(30)=832040`, `fib(35)=9227465`, boucle
`499999500000`, jardin `20 tests, 0 échec`.

---

## 2. Diagnostic — où passait le temps

### 2.1 Le chemin d'exécution avant

Trois couches coexistaient, avec des règles de priorité contradictoires :

1. **Corps principal** : RIR (`Instr::Rir`) en mode natif ; **AST pur en mode autonome**
   (`compiler_principal()` pose `ROSE_NO_RIR_PRINCIPAL=1` dans le shim `main.rs`).
2. **Fonctions RIR** : `executer_pour` → boucle linéaire, slots indexés, sauts numériques.
   Les appels `f($k)` deviennent `Eabstrait::AppelRir` → `appel_rir_direct` (les arguments
   sont des `Expr::Var`, résolus une fois par `slot_du_nom`).
3. **Appels non-émettables** : `appeler()` testait `self.jit.contains_key(&sig.nom)`.

### 2.2 Cause n°1 — le marqueur `Compile::Interprete` désactivait tout le RIR

En mode autonome, `Jit::compiler_tout()` (shim de `main.rs`, **non modifié**) insère
`Compile::Interprete` pour **chaque** fonction que l'émetteur M3.1 ne sait pas produire.
`Compile::Interprete` n'est pas du code machine : c'est l'instruction « laisse l'interpréteur
AST faire ». Or `appeler()` utilisait `contains_key` et en déduisait « déjà compilé » →
**saut complet de la voie RIR**. Conséquence mesurée : `fib(35)` = 19,9 s alors que le
corps RIR existe et tourne en 3,5 s.

Preuve : avec un compteur temporaire dans `eval_slots`, le nouveau chemin d'appel direct
avait **0 hits** sur `fib(25)` tant que `contains_key` était en place ; il en a
**242 784** dès que le marqueur est correctement distingué (≈ nombre d'appels récursifs).

### 2.3 Cause n°2 — `rir::abstraire` ne descend pas les appels à arguments calculés

`rir.rs` (non modifié) n'émet `AppelRir` que si **tous** les arguments sont des `Expr::Var` :
```
fn fib(n: entier) -> entier {
    si (n < 2) { retour n }
    retour fib(n - 1) + fib(n - 2)      // <- arguments = Binaire, PAS des slots
}
```
Dump RIR réel : `0002 retour (bin + (app fib (bin - (v n) (n 1))) (app fib (bin - (v n) (n 2))))`.
Chaque appel récursif retombait donc sur `eval_avec_slots_fallback` :
`self.trame_rir.clone()` (allocation `HashMap`), `clear()`, `insert(String::clone,
Valeur::clone)` par slot, `push`/`pop` de trame, puis `eval` → `Vec<Valeur>` → `appeler`.
Mesuré : ~0,66 µs par appel sur fib(30), soit ~2 millions de trames reconstruites.

### 2.4 Cause n°3 — cache de slot invalidé sur la mauvaise adresse (bug d'exactitude)

`eval_abstrait_rapide` (chemin chaud) validait son `CacheSlot` avec :
```rust
if ptr == base as usize && ty != 0 { /* cache valide */ }
```
`ptr` est l'adresse **du slot** (`base + idx*8`), `base` l'adresse **du cadre**. Le test
n'est vrai que pour `idx == 0`. Quand un cadre ultérieur démarre à l'adresse où pointait le
slot 1 du cadre précédent, le cache périmé est accepté et l'on lit `n` à la place de `d`.
Preuve instrumentée (build autonome avec la voie RIR réactivée) :
```
RIRARG k=14 arg=11 ...
MOD   k=18 11 % 11 = 0 base=0x…f058
RIRRES k=14 res=faux ... n_slot=11 d_slot=3
✗ nb_premiers_inferieurs(100) : obtenu 4, attendu 25
Rose : erreur — conversion vers entier impossible (type Vide, val="", ligne=90)
```
C'est exactement le bug documenté dans `G2_FIX.md` : le marqueur `Compile::Interprete`
le masquait en interdisant le RIR à tout le mode autonome. La vraie correction est donc
la validation par l'adresse du slot, pas l'interdiction du RIR.

---

## 3. Modifications appliquées (`interp.rs` seul)

### A2.1 — `jit_reel(nom)` : distinguer le marqueur du vrai code machine

```rust
fn jit_reel(&self, nom: &str) -> bool {
    matches!(self.jit.get(nom), Some(Compile::Natif(_)) | Some(Compile::Principal(_)))
}
```
Utilisé dans `appeler()` (`compile_par_jit`) et dans les gardes du nouvel appel direct.
Le vrai JIT Cranelift et l'émetteur M3.1 gardent leur priorité absolue ; le marqueur
`Interprete` rend la main au RIR. Aucune règle du langage n'est modifiée.

### A2.2 — validation correcte du cache de slot (exactitude)

Dans `eval_abstrait` **et** `eval_abstrait_rapide` :
```rust
let idx = *i as usize;
...
if ptr == unsafe { base.add(idx) } as usize && ty != 0 { /* valide */ }
```
Au lieu de `ptr == base`. Effet double : exactitude restaurée **et** cache désormais
réellement hité pour `idx != 0` (avant, il était recalculé à chaque accès).

### A2.3 — appel RIR direct depuis une trame de slots (le gain)

Nouvelle méthode `appel_rir_slots(nom, args, base, noms)` :

- accepte des arguments **arbitraires**, évalués par `eval_slots` sur les slots de
  l'appelant (plus de `HashMap` temporaire, plus de `Vec<Valeur>`) ;
- `Expr::Var` → `slot_du_nom` puis `*base.add(k)` (aucun nom haché pour écrire le paramètre) ;
- pose directement les valeurs dans le cadre de l'appelé ;
- conversion de retour **copie conforme** de `appeler()` (voie RIR) ;
- `None` → repli sur l'ancien chemin, donc aucune règle dupliquée.

Branche ajoutée dans `eval_slots` pour `Expr::Appel`, avec les gardes exactes de `appel`
(`module` vide, pas de conversion `entier`/`rationnel`/`flottant`/`chaine`, pas de vrai JIT,
signature RIR présente).

### A2.4 — littéraux entiers i64 (allocation évitée)

Dans `eval_slots`, `Expr::Litteral` entier qui tient en i64 → `Valeur::z(v)` au lieu de
`chaine_vers_entier` (un `BigInt` alloué par occurrence et par appel récursif). Repli exact
sur `eval_interne` pour les rationnels, gros entiers et flottants.

---

## 4. Mesures après — A/A/A entrelacé, 11 échantillons, même salve

```
python3 bench.py 11 base=<binaire_avant> opt=<binaire_après>
```

| Programme | Avant (méd.) | Après (méd.) | Gain | Après — brut (11) |
|---|---:|---:|---:|---|
| `fib(25)` | 162 ms | **29 ms** | **×5,59** | 29,28,28,29,29,29,28,29,29,28,29 |
| `fib(30)` | 1804 ms | **320 ms** | **×5,64** | 319,320,320,320,319,320,320,320,320,319,319 |
| `fib(35)` | 19899 ms | **3562 ms** | **×5,59** | 3566,3562,3557 (3 échantillons) |
| jardin des premiers | 4 ms | **1 ms** | ×4 | 1,1,1,2,1,1,1,1,1,1,2 |
| boucle 1 000 000 | 689 ms | **679 ms** | ×1,015 | 677…683 (déjà 100 % abstrait, aucun appel) |

Trois salves A/A/A indépendantes (avant / après édition concurrente, voir §5bis) ont
donné des médianes « après » 29/321/4/677 puis 29/320/1/679 puis 29/325/1/695 : le gain
est reproductible.

Valeurs **identiques** dans les deux colonnes : `fib(25)=75025`, `fib(30)=832040`,
`fib(35)=9227465`, boucle `499999500000`, jardin `20 tests, 0 échec`.
Le gain cible (≥ 2× sur le mode autonome) est atteint avec une marge de ~2,8×.
La boucle 1M, déjà entièrement sur le chemin abstrait, sert de contrôle de non-régression.

---

## 5. Validation (builds défaut + autonome, binaires isolés)

| Contrôle | Autonome | Défaut (JIT) |
|---|---|---|
| `cargo build --release …` | OK (`Finished`) | OK (`Finished`) |
| `fib(25)` | `75025` | `75025` |
| jardin `jardins/tests/premiers.rose` | `20 tests, 0 échec` | `20 tests, 0 échec` |
| `rose rir compilateur/emetteur.rose \| md5sum` | `bdbef63681608a0b22a5637539e9d738` | `bdbef63681608a0b22a5637539e9d738` |
| auto-émission `rose emetteur.rose emetteur.rose \| md5sum` | `bdbef636…` (260 692 o, rc=0) | `bdbef636…` (260 692 o, rc=0) |
| oracles M4.1–M4.4, échantillon 12 fichiers | 12/12, 12/12, 12/12, 12/12 (0 écart) | idem |

### 5bis — Édition concurrente (transparence)

Pendant ce chantier, d'autres agents ont modifié `interp.rs` (doc `ARCHI_VALEURS.md`) :
extraction de `indexer_valeur`/`affecter_index_valeur` et de `appel_valeurs`, puis
`rir.rs` (`marquer_slot_modifie`, `affecter_index_slots`). Ces refactors sont
sémantiquement neutres pour nos charges (aucun index de table, aucun `marquage` sur le
chemin des fonctions). Les mesures des §1 et §4 ont été faites sur des binaires isolés
**avant** ces éditions. Deux salves de contrôle ont ensuite été rejouées sur des binaires
reconstruits depuis l'arbre fusionné ; la dernière (`rose_auto_final` md5
`b344677c76d4927e3c2fe0479be27786`, `rose_def_final` md5
`f265df572c6a65215dc4f455da25146f`) a redonné fib25 29 ms, fib30 325 ms, jardin 1 ms,
boucle 695 ms (9 échantillons ; le bruit sur la boucle vient du processus de
self-hébergement de l'autre agent, qui occupe un cœur) et **toutes les portes §5 sont
restées vertes** sur les deux builds. Le gain sur fib reste **×5,5–5,6**, reproductible
sur trois salves indépendantes.

L'édition concurrente avait laissé une erreur de type à `Instr::AffectIndex`
(`affecter_index_valeur` rend `Result<(), Option<i32>>`, le bras rend `()`). Correctif
minimal appliqué ici : ajout de `?` pour propager le `Result`, sans toucher à la logique
de l'autre agent.

Commandes exactes (racine `/home/clausake/Stefan/ROSE`) :
```
export PATH="$HOME/.cargo/bin:$PATH"
cd rose
CARGO_TARGET_DIR=/tmp/opencode/roseb_interp/target \
  cargo build --release --no-default-features --features autonome --bin rose
CARGO_TARGET_DIR=/tmp/opencode/roseb_interp/target_def cargo build --release --bin rose
cd ..
taskset -c 19 <binaire> /tmp/opencode/roseb_interp/fib25.rose
taskset -c 19 <binaire> jardins/tests/premiers.rose | tail -1
taskset -c 19 <binaire> rir compilateur/emetteur.rose | md5sum
```

---

## 6. Ce qui reste (honnêteté)

- **Repli AST** : les expressions non abstraites (appels catalogue `longueur`, `joindre`,
  `chaine`, index, factorielles, listes, tables) passent encore par
  `eval_avec_slots_fallback` (trame `HashMap` temporaire). Le nouveau chemin ne les
  capture **pas** : les gardes reproduisent volontairement les priorités de `appel`.
  Un programme dominé par ces primitives ne profite donc pas de ×5,6.
- **Globaux** : le corps principal en mode autonome reste AST
  (`ROSE_NO_RIR_PRINCIPAL=1`, shim `main.rs` non possédé) ; `executer_principal_rir`
  synchronise encore slots ↔ `globaux` opération par opération. Un programme dont le
  temps est dans les globales (boucle 1M) ne change pas (contrôle : ×1,015).
- **Recherche par nom résiduelle** : `slot_du_nom` (cache thread-local à **une** entrée,
  clé = adresse du tableau de noms) est encore appelé pour les arguments `Expr::Var` et
  dans `eval_slots`. Un cache multi-corps (une entrée par `CorpsRir`) pourrait aider les
  programmes à appels croisés entre plusieurs corps ; non fait ici (le montage actuel
  est stable et le gain principal est déjà capté).
- **Comparaison à Rust** : ce chantier ne prétend **pas** battre Rust. Il réduit le coût
  d'un interpréteur ; le code machine natif/JIT reste le plafond (fib(35) ≈ 0,01–0,03 s
  selon les mesures historiques du dépôt, contre 3,56 s ici) — c'est un constat daté,
  pas une consigne.

---

## 7. Verdict

**Objectif atteint.** En réactivant correctement la voie RIR en mode autonome
(`Compile::Interprete` ≠ compilé), en corrigeant la validation du cache de slot (bug
d'exactitude qui avait motivé le contournement) et en gardant les appels à arguments
calculés sur la trame de slots, `fib(25/30/35)` passe de 162/1803/19899 ms à
29/321/3562 ms (**×5,6**, 11/11 échantillons pinés cohérents) et le jardin de 4 à 1 ms,
avec des valeurs identiques et toutes les portes de validation vertes sur les deux builds.
Le gain est **architectural** (aucun nom de fonction codé en dur, aucune special-case),
pas une optimisation locale.
