# CORR4 — alias `type` / `prng` absents du `std_sig` du lecteur

**Date du constat :** 23/11/2026. **Worktree :** `/tmp/opencode/corr_p4` (HEAD `d117e04`).
Aucun commit, aucun push. Tous les chiffres ci-dessous sont des **constats datés**,
re-mesurés dans ce worktree avec le binaire Rose `rose/target/release/rose`
(reconstruit localement, `cargo build --release --bin rose`).

---

## 1. Problème

`compilateur/lecteur_source.rose` est le front-end source → RIR du moteur sans
Rust (`moteur_complet.rose`). Sa table `fn std_sig($nom: chaine)` (ligne ~1992)
ne contenait ni `type` ni `prng`. Le contrôleur `check_appel` (ligne ~2246) fait :

```
$spec = std_sig($nom)
si ($spec == "") { signaler($lg0, "Fonction '" + $nom + "' inconnue", $err) }
```

Résultat mesuré **avant** correctif :

| programme | `ROSE_SANS_JIT=1 rose <prog>` (natif) | `rose lecteur_source.rose <prog>` |
|---|---|---|
| `t14_type.rose` | `entier / chaine / liste / booleen` (rc=0) | `ERR\|… Fonction 'type' inconnue` |
| `t91_prng.rose` | `0 / 9` (rc=0) | `ERR\|… Fonction 'prng' inconnue` |

Pendant ce temps, le compilateur composite `rose_natif.rose` accepte les deux :
`scripts/generer_rose_natif.py` injecte les alias **dans sa copie générée** du
front-end (`STD_SIG_EXTENSION`, ligne 121 du générateur), sans toucher à
`compilateur/emetteur.rose` (point fixe `b3ed9262` préservé). Le lecteur, lui,
était laissé pour compte.

---

## 2. Diagnostic — ce que fait le générateur

`STD_SIG_EXTENSION` (generer_rose_natif.py:121-126) :

```python
STD_SIG_EXTENSION = (
    '  # EXTENSION : alias natifs déjà émis par le backend G5.\n'
    '  si ($nom == "type") { retour "chaine|tout" }\n'
    '  si ($nom == "prng") { retour "entier|entier" }\n'
)
STD_SIG_INJECT += STD_SIG_EXTENSION
```

Injecté via `replace_once(em_head, STD_SIG_ANCRE, STD_SIG_INJECT, …)` (ligne 568)
dans la **copie concaténée** du front-end, pas dans la source. Les signatures
sont le miroir exact de `rose/src/front.rs` (non modifié) :

- `front.rs:746` — `EntreeStd { nom: "type", args: &["tout"], retour: "chaine" }`
  → `"chaine|tout"` ;
- `front.rs:786` — `EntreeStd { nom: "prng", args: &["entier"], retour: "entier" }`
  → `"entier|entier"`.

---

## 3. Correctif appliqué

Reproduction **directe dans la source du lecteur** (plus simple qu'une injection
différée : `lecteur_source.rose` est un fichier possédé, et le moteur sans Rust
lit la source telle quelle). Ajout de 8 lignes dans `fn std_sig` :

```rose
  # CORR4 — alias natifs déjà émis par le backend G5 (miroir de
  # scripts/generer_rose_natif.py::STD_SIG_EXTENSION) : le moteur Rust
  # front.rs:746/786 les expose ; le lecteur doit les connaître pour ne pas
  # refuser t14_type / t91_prng (« Fonction '…' inconnue »). L'exécution RIR
  # est déjà assurée par interpreteur_rir.rose (val_type / hasard).
  si ($nom == "type") { retour "chaine|tout" }
  ...
  # CORR4 — alias de `hasard` exposé par front.rs:786 (voir `type` plus haut).
  si ($nom == "prng") { retour "entier|entier" }
```

**Aucune autre source modifiée.** `compilateur/emetteur.rose`,
`emetteur_elf.rose` et `rose/src/**` sont intacts (point fixe vérifié, §6).

---

## 4. Implémentation RIR — déjà présente, rien à ajouter

Point 2 de la mission (« ajouter l'implémentation RIR si manquante ») : elle
n'est **pas** manquante. `compilateur/interpreteur_rir.rose` contient déjà :

```
896:  si ($name == "type" ou $name == "type_de") { si ($n < 1) { retour vs("vide") }; retour vs(val_type($a[1])) }
912:  si ($name == "graine") { si ($n >= 1) { graine(entier(val_str($a[1]))) }; retour "v:" }
913:  si ($name == "hasard" ou $name == "prng" ou $name == "hasard_systeme") { si ($n < 1) { retour vi("0") }; retour vi(chaine(hasard(entier(val_str($a[1]))))) }
```

et `val_type` (ligne 461) couvre `i/r/f/s/b/l/t` → `entier/rationnel/flottant/
chaine/booleen/liste/table`. Preuve de fonctionnement : le moteur complet sans
Rust exécute t14 et t91 (§5.3). Aucune modification de l'interpréteur n'a donc
été faite (il n'est pas dans les fichiers possédés).

---

## 5. Tests — 0 refus

### 5.1 t14 / t91 — lecteur

Après correctif, `rose lecteur_source.rose <prog>` répond `OK|rir` (plus aucun
`ERR|… inconnue`) et émet `(app type …)` / `(app prng …)` :

```
t14 : 0000 afficher (app type (n 2)) … 0003 afficher (app type (b vraie))
t91 : 0000 expr (app graine (n 7))  0001 afficher (app prng (n 10)) …
```

### 5.2 Vingt programmes distincts — balayage

20 fichiers générés dans `/tmp/opencode/corr4_progs/` (p01…p20), couvrant
`type` sur entier/rationnel/flottant/chaine/liste/booleen/table, `type` imbriqué,
`type_de`, expressions, unaire, concaténation, `graine` + `prng` (bornes 0/1/10/
100), `hasard`, `prng` imbriqué. Résultat (`/tmp/opencode/corr4_sweep.tsv`) :

```
total=20  natif_rc0=20  refus=0  rir_egal_oracle=20
```

Autrement dit : **20/20 acceptés par le lecteur, 0 refus**, 20/20 exécutables
nativement, et 20/20 RIR du lecteur **octet pour octet identiques** à l'oracle
`rose rir`.

### 5.3 Fichier de test possédé + pipeline complet

`compilateur/tests_corr4.rose` (nouveau) regroupe les 20 cas en un seul
programme. Résultats :

- natif : 20 lignes, rc=0 (`entier, rationnel, flottant, chaine, liste,
  booleen, table, chaine, entier, entier, entier, chaine, 0, 9, 0, 45, 2,
  8632209307422871798, 1, 0`) ;
- lecteur : `OK|rir`, 21 ops, **0 refus** ;
- `rose rir tests_corr4.rose` vs RIR du lecteur (après retrait du préfixe
  `OK|`) : `diff` vide → **identique octet pour octet** ;
- `moteur_complet.rose` (sans Rust, régénéré pour l'essai puis remis à `HEAD`)
  exécute le fichier rc=0.

**Observation hors périmètre, non imputable à CORR4 :** sur `type(2/3)`, le
natif affiche `rationnel` mais le moteur RIR affiche `flottant`. Ce n'est **pas**
une régression du correctif : l'oracle `rose rir` émet lui aussi `(f 2/3)` pour
cette expression, et le RIR du lecteur est identique à l'oracle. L'écart est
préexistant entre l'interpréteur natif et l'interpréteur RIR sur un littéral
rationnel replié, et n'est pas un refus (`diff` RIR = 0).

### 5.4 Composites (ELF + PE)

| composite | entrée | résultat |
|---|---|---|
| `rose_natif.rose` | t14 | `emit_rc=0`, magic `7f454c46`, ELF `out=entier chaine liste booleen` |
| `rose_natif.rose` | t91 | `emit_rc=0`, magic `7f454c46`, ELF `out=0 9` |
| `rose_natif_pe.rose` | t14 | `emit_rc=0`, magic `MZ`, 4096 o |
| `rose_natif_pe.rose` | t91 | `emit_rc=0`, magic `MZ`, 3584 o |

Régénération déterministe vérifiée : `RNAT_OUT_ELF=… RNAT_OUT_PE=… python3
scripts/generer_rose_natif.py` produit des fichiers **md5 identiques** aux
composites livrés (`c88cda31…` / `92a54835…`). Les alias injectés par le
générateur restent donc inchangés et compatibles avec le correctif du lecteur.

---

## 6. Validation finale

Exécutée avec le garde-fou mémoire demandé (`bash scripts/garde_fou_mem.sh
12000000 …`). Une copie isolée du script (logs privés sous
`/tmp/opencode/c4_vf/`, `RUST_TEST_THREADS=1`, `ROOT` figé sur le worktree) a
été utilisée pour échapper à la pollution des logs partagés par les worktrees
frères (`/tmp/opencode/validation_finale.log`, `/tmp/opencode/vf_tests.log`)
et au test sensible à la charge `opt_wall_final.rs::programme_minimal_jit_reste_conforme`.

Résultat final (`/tmp/opencode/c4_vf/validation_finale.log`) :

```
=== RESULTAT: 0 échec(s) ===
1. tests rc=0                 (314 + 327 + … tests, 0 failed)
2. point fixe                 b3ed9262dd54776a6dbd07bc12368d0e / idem
3. AOT 75/75                  「TOTAL: 75 OK, 0 ECHEC」
4. JIT                        25/25
5. WASM 92 o + RVM            317811
6. AZ                         31 CONFORME, 0 non-conforme
7. HOST-1                     fib=317811
```

Le point fixe `b3ed9262dd54776a6dbd07bc12368d0e` est confirmé directement :
`rose rir compilateur/emetteur.rose` et
`rose compilateur/emetteur.rose compilateur/emetteur.rose` donnent le même md5.

---

## 7. État des lieux factuel

**Fait :**
- `compilateur/lecteur_source.rose` : 8 lignes ajoutées dans `std_sig` (`type`,
  `prng`) — seul fichier source modifié.
- `compilateur/tests_corr4.rose` : nouveau, 20 cas type/prng.
- `rose/docs/CORR4_ALIAS.md` : ce document.
- Composites `rose_natif.rose` / `rose_natif_pe.rose` : régénérés en essai,
  **octet pour octet identiques** à ceux livrés (aucun diff à committer).
- `interpreteur_rir.rose` : implémentation RIR déjà présente, vérifiée, non
  modifiée (hors périmètre de possession).

**Vérifié :** t14/t91 acceptés (0 refus), 20 programmes 0 refus + RIR == oracle,
composites ELF/PE OK, point fixe b3ed9262, validation_finale 0 échec.

**Reste :** rien dans le périmètre CORR4. L'écart natif/RIR sur `type(2/3)`
(§5.3) est préexistant et hors périmètre ; il n'affecte ni l'acceptation par le
lecteur ni la conformité du RIR à l'oracle.
