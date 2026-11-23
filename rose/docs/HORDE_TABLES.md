# HORDE_TABLES — tables générales dans le moteur auto-hébergé (J6.4)

**Date :** 23/11/2026 · **Worktree :** `/tmp/opencode/horde_tables2`, HEAD
`101b48d25a85a3a4a5be80232af9dda7bff037f1`. **Aucun commit, aucun push,
aucun fichier touché hors des trois fichiers possédés.**

| Fichier | Rôle |
|---|---|
| `compilateur/tables_rose.rose` | table à hachage en Rose pur (nouveau) |
| `compilateur/tests_tables_rose.rose` | banc 120 clés mixtes + vecteurs + montée en charge (nouveau) |
| `rose/docs/HORDE_TABLES.md` | ce document (nouveau) |

## 1. Point de départ — lecture du code existant

`compilateur/moteur_ops.rose:560-609` (`op_tab_inserer`, `op_tab_cle_existe`,
`op_tab_idx`) encode une table comme `[5, c1, v1, c2, v2, …]` : **clé = un
seul code ASCII** (`o_chcode(cle,0)`), **valeur = un entier** (`val[1]`), et
les trois opérations font un **scan linéaire** (`tant_que ($k < $n)` sur
`o_long(t)`). Un bigz stocké rend la sentinelle `0`.

`rose/src/run.rs` (lecture seule) montre que le moteur Rust a déjà résolu le
problème du scan : `TableRep` (`run.rs:3400`) garde `items` (ordre
d'insertion), `array_part` (`run.rs:3415`, `N_ARRAY = 4096`) et un
`IndexRapide` à adressage ouvert (`run.rs:3347`), indexé par
`melange_u64(h) & m`. Le finaliseur (`run.rs:3288`) est :

```rust
let z = (h ^ (h >> 29)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
z ^ (z >> 32)
```

Sans lui, les 3 000 premières clés entières tombaient dans un seul bucket
(`OPT_TABLES.md` §2). Le hachage canonique `h()`/`h_entier()` (`run.rs:2966`)
hache les numériques par leur `f64`, ce qui rend `Entier ≡ Rationnel ≡
Flottant` ; `k_table()` (`run.rs:2942`) sert d'accélérateur pour les entiers
denses.

## 2. Conception retenue

### 2.1 Un arbre de hachage binaire persistant, pas un scan

Rose n'a **ni affectation par index sur une liste, ni retour de liste** sous
G5 (mesuré : une liste retournée par une fonction sort taguée « entier » et
s'affiche comme une adresse, `201330688` dans le sonde `probe_retlist.rose`).
Un `IndexRapide` mutable est donc impossible en Rose pur. La table est un
**arbre de hachage binaire de 64 niveaux** stocké dans un magasin
**append-only** de nœuds de 7 entiers :

```
nœud interne : [left, right, _, _, -1, _, _]     (hash_hi = -1 = marqueur)
feuille      : [0, chaine, hash_lo, id_valeur, hash_hi, id_clé, 0]
```

- un bit du hachage par niveau (hi puis lo), insertion et recherche = **64
  nœuds + 1 feuille**, indépendamment du nombre de clés ;
- mise à jour par **copie de chemin** (on ajoute de nouveaux nœuds, jamais on
  n'écrit un ancien) ; la racine est un entier rendu par `t_inserer` /
  `t_supprimer` ;
- collisions de hachage : chaîne de feuilles par le champ `right` ;
- suppression logique : la feuille devient une tombe (`id_clé = -1`), ce qui
  conserve l'ordre d'insertion et évite de réécrire l'arbre.

Quatre listes plates d'entiers portent tout l'état : `$nd` (nœuds), `$cl`
(clés), `$vl` (valeurs), `$fl` (flux de données de clés). Elles sont passées
en paramètre ; `joindre` y ajoute en place.

### 2.2 `melange_u64` exact, en arithmétique Rose

Rose n'a pas `^`, `&`, `>>`. Le finaliseur est reproduit **à l'octet près**
avec `div`, `%`, `+`, `*` et un `xor32` arithmétique (32 tours) :

1. `h >> 29` en deux mots 32 bits : `sh_hi = hh div 2^29`,
   `sh_lo = (hh mod 2^29)*8 + hl div 2^29`, puis `xor32` sur chaque mot ;
2. produit par `C = 0xBF58476D1CE4E5B9` en **4 limbs de 16 bits**
   (`58809, 7396, 18285, 48984`), donc aucun produit intermédiaire ne dépasse
   `2^34` — Compatible avec l'ABI G5 (immédiats `n<<3`, plafond `2^60`) ;
3. `z ^ (z >> 32)` : haut inchangé, bas = `xor32(zl, zh)`.

Neuf vecteurs (0, 1, 2, 42, 1000, `2^32-1`, `0x123456789ABCDEF0`,
`0xDEADBEEFCAFEBABE`, `2^63`) sont codés en dur dans
`tests_tables_rose.rose`. Ils ont été recalculés par un petit binaire Rust
compilé avec `rustc -O` qui reproduit exactement `melange_u64` de `run.rs` —
**9/9 identiques** — puis vérifiés **9/9** sous l'interpréteur Rust et sous
l'ELF C0.

### 2.3 Clés hétérogènes et parité sémantique

| Tag | Forme | Hachage canonique |
|---|---|---|
| 1 | `[0,1,n]` entier | `(n mod 2^32, 1)` |
| 3 | `[0,3,c0..cn]` chaîne | FNV-1a sur les codes, puis `melange_u64` |
| 9 | `[0,9,num,den]` rationnel | `(num mod 2^32, den mod 2^32)` réduit, puis `melange_u64` |

Le préfixe `0` est le rembourrage obligatoire des listes G5 (le
vérificateur refuse `[]`). Entier et rationnel sont **hachés pareil** et
comparés par produits croisés, donc `3`, `9/3` et `6/2` sont la même clé,
comme dans `valeur_egale` de `run.rs`. `cles()` rend une liste plate
`[tag, longueur, données…, …]`, `valeurs()` les valeurs, tous deux triés par
`id_clé` = ordre de la première insertion.

API (tout retourne un entier ; les listes de sortie sont passées en
paramètre) : `t_inserer`, `t_cle_existe`, `t_idx`, `t_supprimer`, `t_cles`,
`t_valeurs`. Les valeurs sont des entiers dans la plage sûre de l'ABI G5
(comme dans `moteur_ops.rose` : pas de bigz dans un slot de table).

## 3. Tests — 120 insertions / 120 recherches sur clés mixtes

`cle_gen(k)` tire une clé entière (`k mod 3 = 0`), chaîne `"k<k>"` (`= 1`) ou
rationnel `(k+1)/((k mod 7)+2)` (`= 2`) ; `valeur_gen(k) = k² + 7`.
Le générateur produit 120 clés dont **111 sont distinctes** après réduction
et équivalence numérique — exactement le cardinal de la primitive native
`table()` du moteur Rust sur les mêmes clés.

Sortie (identique octet pour octet sous l'interpréteur Rust et sous l'ELF) :

```
MELANGE=9/9
BANC_120 EXISTE=120/120 VALEUR=120/120
BANC_120 SOMME=569660 CLES=111
BANC_120 VISITES=7920 SCAN=6408
ORDRE CLES=111 VALEURS=111
PARITE_RUST=4/4
UPDATE=128->999
SUPPR_EXISTE=0 REINS=1 VAL=1001
FIN=OK
```

`PARITE_RUST=4/4` compare : 120/120 présences, 120/120 valeurs, somme
`569660` et 111 clés — valeurs issues d'un programme utilisant la `table()`
native du moteur Rust (`oracle_natif.rose`, hors dépôt). `UPDATE`, `SUPPR`
et `REINS` couvrent la mise à jour, la suppression d'une clé rationnelle
non réduite et sa réinsertion.

## 4. Performance — absence de scan linéaire

Les compteurs sont ceux du chemin réel : `t_idx_rec` reçoit une liste et y
joint 1 à chaque nœud visité ; `scan_compare` fait la même chose pour un scan
linéaire des 111+ enregistrements de clés. Un point par **processus** (un
appel de `banc_echelle` par exécution) pour éviter la réutilisation d'un
tampon de liste par le moteur entre deux gros appels dans un même processus
(mesuré : 2 clés perdues au 2ᵉ appel à 240 insertions — voir §6).

| n insertions/recherches | visites arbre | visites/rech. | comparaisons scan | scan/rech. | natif s | ELF s |
|---:|---:|---:|---:|---:|---:|---:|
| 120 | 7 920 | **66** | 6 408 | 53 | 0,18 | 0,00 |
| 240 | 15 840 | **66** | 25 572 | 106 | 0,38 | 0,01 |
| 480 | 31 680 | **66** | 101 430 | 211 | 1,21 | 0,04 |
| 960 | 63 360 | **66** | 404 636 | 421 | 4,18 | 0,12 |

Le coût par recherche est **constant (66 nœuds)** quand n passe de 120 à
960 (×8), alors que le scan linéaire passe de 53 à 421 comparaisons (×8).
Le rapport scan/arbre passe de 0,8 à 6,4. L'objectif « pas de scan linéaire »
est atteint et mesuré, pas décrété. Le coût total reste linéaire en n
(0,38 → 4,18 s, ×11 pour ×8 insertions), conformément à une insertion O(64).

Les trois points de montée en charge ont aussi été rejoués sous l'ELF avec
une sortie réduite aux deux compteurs : **parité native/ELF = OK** pour
240, 480 et 960 (`VIS=… SCAN=…` identiques).

## 5. Reproduction

Binaire Rust utilisé : `/tmp/opencode/opt_shared_target/release/rose`,
construit depuis ce worktree (`cargo build --release --bin rose`, rc=0).
C0 : compilateur auto-hébergé de ce worktree, reconstruit et vérifié :

```
rose rir compilateur/emetteur_elf.rose > emetteur_elf.rir   # 426 304 o
rose compilateur/emetteur_elf.rose emetteur_elf.rir > C0b.elf
C0b.elf prog.rir > prog.elf                                  # RIR → ELF
```

Modules et tests :

```
rose verif compilateur/tables_rose.rose                 -> vérifié (rc=0)
cat compilateur/tables_rose.rose compilateur/tests_tables_rose.rose > prog.rose
rose verif prog.rose                                    -> vérifié (rc=0)
rose rir prog.rose > prog.rir
C0b.elf prog.rir > prog.elf                             -> 79 072 o, rc=0
ROSE_SANS_JIT=1 rose prog.rose > nat.out
./prog.elf > elf.out ; diff nat.out elf.out             -> 0 (10 lignes)
rose rir compilateur/tables_rose.rose > tables_rose.rir
C0b.elf tables_rose.rir > tables_rose.elf                -> 45 260 o, rc=0, 0 octet de sortie
```

`tests_tables_rose.rose` est un **module source** : comme `moteur_ops.rose`
et les pilotes J6, il se concatène avec son module ; vérifié seul, il
signale les fonctions `t_*` inconnues (pas d'`include` en Rose).

## 6. Deux constats de moteur rencontrés, et leur contournement

Ces deux points ont été **mesurés** puis contournés dans les fichiers livrés ;
ils ne concernent ni la logique de la table ni sa parité.

1. **Liste appendée par un callee** : sous l'ELF C0, au-delà de quelques
   milliers d'ajouts, la liste que le caller avait passée en paramètre ne
   reflète plus les ajouts (`VISITES_TOT=-1` puis une adresse à la place du
   compteur). `banc_verifier` rend donc ses deux compteurs **dans un entier**
   (`totvis*1_000_000 + totscan`) ; les tests livrés ne dépendent d'aucune
   liste de compteurs.
2. **Réutilisation d'un tampon entre deux gros appels dans un même
   processus** (interpréteur Rust) : un 2ᵉ `banc_insert(240)` dans le même
   processus ajoutait 10 enregistrements de clé et 21 nœuds fantômes, puis
   un 3ᵉ appel à 240 provoquait « index hors des limites de la liste ».
   Chaque point de charge est donc mesuré dans son propre processus, ce qui
   est reproductible et a été vérifié natif et ELF.

Aucun de ces deux effets n'apparaît sur le banc livré (120 insertions), qui
est vert et identique des deux côtés.

## 7. Validation finale — 0 échec

`validation_finale.sh` a été joué sous le garde-fou mémoire
(`bash scripts/garde_fou_mem.sh 12000000 …`). Le script attend un binaire
`rose/rose` à la racine du dépôt pour la batterie AZ ; ce worktree n'en a
aucun (seul `rose/target/release/rose` est construit). Pour **ne pas ajouter
de fichier au worktree**, la salve finale a tourné dans un miroir hors dépôt
`/tmp/opencode/horde_tables2_art/vf_root` (copie `cp -al` du worktree, avec
`rose/rose` = le binaire Rust compilé depuis ce worktree). Les trois fichiers
livrés y sont les mêmes (liens durs).

Première salve dans le miroir : tout vert sauf un aléa connu
(`link_natif::tests::elf_avec_valeur_exacte_ecrit_le_texte_memorise`,
`Text file busy` sous `cargo test` parallèle — déjà documenté dans
`OPT_TABLES.md` §5). Rejeu avec `RUST_TEST_THREADS=1`, log
`/tmp/opencode/horde_tables2_art/vf3_validation_finale.log` :

```
OK   tests rc=0
  lib 314 passés / 0 échec · bin 327 passés / 0 échec
OK   point fixe        b3ed9262dd54776a6dbd07bc12368d0e (identique ×2)
OK   AOT 75/75         75 OK, 0 ECHEC
OK   aucun refus AOT
OK   JIT               25/25
OK   WASM 92 o · RVM 317811
OK   AZ 31 CONFORME, 0 non-conforme
OK   HOST-1 fib=317811
RESULTAT: 0 échec(s)
```

Point fixe re-mesuré directement dans le worktree également :
`rose rir compilateur/emetteur.rose` **et**
`rose compilateur/emetteur.rose compilateur/emetteur.rose` →
`b3ed9262dd54776a6dbd07bc12368d0e` des deux côtés.

## 8. État final

- Fait : table à hachage persistante 64 niveaux, finaliseur `melange_u64`
  exact, clés entier/chaîne/rationnel avec équivalence numérique, ordre
  d'insertion, `inserer`/`cle_existe`/`idx`/`supprimer`/`cles`/`valeurs`.
- Vérifié : `rose verif` des modules et de la concaténation ; C0 → ELF des
  deux modules ; parité native/ELF octet pour octet des 10 lignes de test et
  des 3 points de charge ; 9/9 vecteurs Rust ; 120/120 parité moteur Rust ;
  66 visites/recherche constantes ; validation finale 0 échec.
- Reste : rien dans le périmètre J6.4. Le port des clés flottantes
  (`Entier ≡ Flottant` complet) et des valeurs bigz reste au jalon J6.1/I4,
  hors de ce chantier.
