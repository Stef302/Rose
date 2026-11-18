# Protocole de trame unique — fermeture du goulot #1

**Nature du document** : constat daté (règle `AGENTS.md`), pas une consigne.
Toutes les mesures ci-dessous ont été **exécutées** le 18/11/2026 ; aucun chiffre
n'est recopié de `GOULOT_ARCHITECTURE.md` sans re-mesure.

- Salves de mesure : `15:08:36Z` (reproduction), `15:10:33Z` (baseline sources),
  `15:16:35Z` (étape 1), `15:23:37Z` (salve finale), `15:26:30Z` (`mix` A/B).
- Audit complet : `2026-11-18T16:26:12+01:00`.
- Machine : GB10 aarch64, cœur X925 **15**, `taskset -c 15`, `ROSE_TIMING=1`.
- Binaire final : sha256 `ce2188308eab1f43846f2d95aa7e42135ced6661f62992189557bf7620fb6c31`
  (identique à `target/release/rose` rebâti depuis le HEAD courant).
- 9 échantillons par variante, **entrelacés par rotation**, médianes ; hash du
  binaire contrôlé avant et après chaque salve (`identical=True` partout).
- Fichers modifiés : `rose/src/interp.rs`, `rose/src/rir.rs`, ce document.
  **Aucun commit fait par cette mission** (voir §7).

---

## 1. Reproduction du goulot (avant toute modification)

Deux baselines ont été mesurées :

1. le binaire historique du doc `GOULOT_ARCHITECTURE.md` (sha `7ba89679…`),
   avec le harnais d'origine `mesure_trame.py` → `/tmp/opencode/goulot/trame_repro.json` ;
2. le binaire rebâti depuis les sources **telles qu'elles étaient à la prise en
   charge** (sha `bcbd0530…`, `cargo build --release`), afin que le A/B porte sur
   le même état source plus mes seules modifications →
   `/tmp/opencode/goulot/trame_baseline1.json`.

| Programme | avant (baseline sources) | +30 variables | ratio V30/V0 |
|---|---:|---:|---:|
| `v6_add` (arithmétique abstraite) | 0,892 ms | 0,928 ms | **1,040** |
| `v5` (`$a[$j] = $j`, 100 k) | 77,531 ms | 617,022 ms | **7,958** |
| `v7` (`joindre($l,$j)`, 100 k) | 72,514 ms | 604,570 ms | **8,337** |
| `v8` (`$s + $a[$q]`, 2×100 k) | 210,153 ms | 1300,391 ms | **6,188** |

La reproduction du doc (`7ba89679…`) donne 7,90 / 8,23 / 6,12 : le goulot est
reproductible sur deux binaires distincts.

---

## 2. Chemin exact et cause

Deux matérialisations `O(ops × variables)` ont été trouvées, pas une :

1. `eval_avec_slots_fallback` (ex-`interp.rs:1811-1817`) :
   `for (i,n) in noms { trame.insert(n.clone(), v.clone()) }` — déclenchée par
   tout appel non abstrait (`joindre`) et toute lecture indexée (`$a[$q]`).
2. **`synchroniser_slots`** (`interp.rs:2481`) : après **chaque** opération du
   corps principal, `for (i,n) in noms { globaux.insert(n.clone(), slots[i].clone()) }`.
   C'est le même goulot, déguisé : la première correction (suppression du repli
   AST) a fait tomber `v7` de 604 ms à 372 ms, mais le ratio est resté **~7,7**.
   La mesure intermédiaire a donc imposé la seconde correction.

`v6_add` est insensible aux variables (ratio 1,04) parce que sa boucle est
abaissée en **fonction** (`PROFIL-RIR fonctions_abaissees=1`, voie
`executer_avec_ptr`, sans synchronisation) ; `v5`/`v7`/`v8` gardent leur boucle
dans le **corps principal** (`fonctions_abaissees=0`, voie
`executer_avec_ptr_sync`, synchronisée). C'est ce qui a rendu le second O(V)
visible.

---

## 3. Approche choisie et implémentation

Décision : **approche (a), avec un complément incrémental** — opérandes résolues
en slots passées directement à l'opération ; aucun `HashMap` reconstruit dans la
boucle. Le repli AST reste disponible pour les formes rares, mais il n'est plus
sur les chemins mesurés.

### 3.1 `interp.rs` — appels et indexation sans trame

- `appel()` est factorisé en `appel_valeurs(nom, module, Vec<Valeur>)` : la
  *dispatch* est inchangée (conversion, `dynamique`, FFI, `appeler`, `stdlib`) ;
  seules les `Valeur` sont déjà résolues.
- `eval_slots()` traite désormais directement :
  - `Expr::Appel` → `appel_slots_sans_trame()` : chaque argument est évalué par
    `eval_slots` (lecture par index via `slot_du_nom`), puis passé à
    `appel_valeurs`. Le chemin RIR-direct existant (`appel_rir_slots`) reste
    prioritaire ; s'il décline, on vient ici au lieu du repli.
  - `Expr::Index` → `indexer_valeur(base, cle)`, extraite à l'identique de
    `eval_interne` (table, liste, chaîne, tableau, messages d'erreur conservés).
- `affecter_index_valeur()` extraite de `Instr::AffectIndex` (mêmes règles,
  même texte d'erreur) ; elle est appelée :
  - par le **corps principal** via la nouvelle méthode `affecter_index_slots`
    (table lue depuis le slot de `nom`, index et valeur lus depuis les slots) ;
  - par `Instr::AffectIndex` historique, inchangé dans son résultat.
- ⚠ Les corps de **fonction** gardent `HoteRir::affecter_index` (chemin
  historique `self.exec`, **sans construction de trame**). Raison mesurée :
  `appel_rir_slots` n'initialise pas les slots non-paramètres depuis `globaux`
  (bug préexistant) ; une table globale y apparaît comme `Vide`. Un programme
  de test dédié (`t_global3.rose`) donne **exactement la même erreur** avant et
  après : `Rose : erreur — [Ligne 6, Colonne 0] Variable 'k' utilisée sans valeur`.

### 3.2 `rir.rs` — synchronisation incrémentale

- Nouvelle méthode de trait `marquer_slot_modifie(slot)` (default no-op, pour ne
  pas casser l'autre implémentation `HoteBc` de `rosevm.rs`, hors périmètre).
- `executer_avec_ptr_sync` l'appelle après chaque écriture de slot
  (`Affect`, `AffectAst`, `Lire`).
- `Interp::synchroniser_slots` ne parcourt plus `noms` : il vide `slots_sales`
  et ne copie que ces slots dans `globaux`. Coût de la synchro d'une boucle
  devenu O(slots écrits), au lieu de O(variables déclarées).

---

## 4. Résultats après — porte V30/V0 ≤ 1,1

Salve finale `15:23:37Z`, binaire `ce2188…`, 9 échantillons pinés, hash constant.

| Programme | V=0 | V=30 | ratio (avant → après) | gain absolu V=0 |
|---|---:|---:|---:|---:|
| `v6_add` | 0,862 ms | 0,868 ms | 1,040 → **1,007** | ~1,03× |
| `v5` | 14,287 ms | 14,215 ms | 7,958 → **0,995** | 77,531 → 14,287 ms = **5,43×** |
| `v7` | 13,790 ms | 13,693 ms | 8,337 → **0,993** | 72,514 → 13,790 ms = **5,26×** |
| `v8` | 31,546 ms | 32,032 ms | 6,188 → **1,015** | 210,153 → 31,546 ms = **6,66×** |

**Porte ATTEINTE** : tous les ratios ≤ 1,1 (max mesuré 1,015). La dépendance
au nombre de variables a disparu : avec 60 variables, `v5` = 14,149 ms et
`v7` = 13,713 ms (encore dans le bruit).

Coût marginal avant : `(617,022 − 77,531)/30/100 000 = 180 ns` par variable et
par opération. Après : `(14,215 − 14,287)/30/100 000 ≈ −0,02 ns` (bruit).

`mix.rose` (A/B entrelacé, 9+9 échantillons, `15:26:30Z`) :
**198,693 ms → 123,915 ms = 1,603×**, valeurs identiques
(`1048576`, `141`, `199`, `39204`). Gain net 74,8 ms, très au-delà du résidu
de 15,0 ms et de la moitié du temps table+liste exigés par le doc.

### Objectif secondaire non atteint

Le doc fixait aussi « coût par `joindre` < ~100 ns/op ». Mesuré après :
`13,790 ms / 100 000 = 138 ns/op` (avant : 725 ns/op), soit **5,26× mais au-dessus
de 100 ns**. Ce chiffre n'est **pas** présenté comme atteint. Le résidu inclut
la boucle RIR elle-même (compteur, test), la construction du `Vec<Valeur>`
d'arguments et la dispatch `stdlib` — hors du périmètre de cette étape.

---

## 5. Validation sémantique (audit complet rejoué sur le binaire final)

| Commande | Résultat mesuré |
|---|---|
| `cargo build --release` | rc=0, hash `ce2188…` |
| `bash scripts/audit-final.sh` (adapté au binaire final) | rc=0 |
| Oracles M4 lexeur / analyseur / vérificateur / émetteur | **152/0 pour chacun** (corpus 152 fichiers) |
| Point fixe M4.5 `rose rir …` vs `rose compilateur/emetteur.rose …` | **bdbef63681608a0b22a5637539e9d738** égaux |
| `rose jardins/tests/premiers.rose` | **20 tests, 0 échec** |
| `fib(25)` | **75025** |
| `bench_global.rose` (depuis un dossier temporaire) | **rc=0** |
| `cargo test --release` (global) | rc=0 : 160 + 144 + 6 + 1 tests passés, 0 échec |
| Cas limite `t_global3.rose` | message d'erreur **identique** au baseline |

Log brut : `/tmp/opencode/goulot/audit_final.log`,
`/tmp/opencode/goulot/cargo_test_global_final.log`.
Le corpus est passé de 151 à 152 fichiers pendant la mission (dépôt partagé) :
le chiffre 152/0 est le constat du 18/11, pas une correction du 151/151 historique.

---

## 6. Ce qui reste

- `eval_avec_slots_fallback` construit encore une trame dans le repli AST, mais
  plus aucun des programmes mesurés (`v5`, `v7`, `v8`, `mix`) ne l'atteint.
- Le coût résiduel des opérations (138 ns/`joindre`) n'est pas la trame : c'est
  la boucle RIR, le `Vec` d'arguments et la dispatch — cible d'une étape ultérieure.
- Bug préexistant non corrigé (hors périmètre, comportement identique avant/après) :
  `appel_rir_slots` n'initialise pas les slots non-paramètres depuis `globaux`,
  donc une fonction qui modifie une table globale échoue. Le corriger exige de
  changer la règle d'initialisation des cadres, pas le protocole de trame.
- `slots_sales` est vidé au début du corps principal ; aucun autre usage n'a été
  introduit. Aucune mesure de mémoire(née) n'a été faite ici.

---

## 7. Mouvement git constaté

Aucun commit n'a été effectué par cette mission. Le dépôt étant partagé, un
autre agent a commité `interp.rs` et `rir.rs` **avec leurs modifications déjà
intégrées** dans `58b27bd` (« Architecture RIR : 5,59× sur récursion… »,
16:23:55+01:00). Vérification faite :
`git show 58b27bd:rose/src/interp.rs | grep -n 'depuis_slots\|slots_sales\|appel_slots_sans_trame'`
et l'équivalent sur `rir.rs` confirment que la version finale est bien présente.
`git status` est propre ; `cargo build --release` depuis HEAD redonne
exactement le hash mesuré `ce2188…`.

---

## 8. Reproduction

```bash
cd /tmp/opencode/goulot
BIN=/tmp/opencode/goulot/rose_baseline python3 mesure_trame_bin.py baseline1
BIN=/tmp/opencode/goulot/rose_apres2  python3 mesure_trame_bin.py apres2
python3 mesure_mix_final.py
bash /tmp/opencode/goulot/audit-final-final.sh
sha256sum /home/clausake/Stefan/ROSE/rose/target/release/rose
```
