# RC5 — optimisation restante des phases tables / RNG du banc global

**Constat daté du 20/11/2026.** Aucun commit, aucune publication. Périmètre :
`rose/src/run.rs` uniquement (ce document). Binaire isolé :
`/tmp/opencode/rc5_agents/rose_final`.

---

## 1. Objectif et point de départ

Le banc global (`bench/global/bench_global.rose`, instrumenté par
`temps()` dans `phase_bench.rose`) décompose l'exécution en 9 phases. Les deux
phases cibles, mesurées sur la machine courante (aarch64, 20 cœurs, cores 6
et 16 épinglés) :

| Phase | Référence tâche | Mesuré ici (base, médiane 21 rotations) |
|---|---|---|
| Tables (5 000 insertions `inserer($tab, "k"+chaine(f), f*f)`) | ~416 µs | **334 µs** |
| RNG (`graine` + `tampon(10⁶)` + `remplir_hasard` alphabet 64) | ~172 µs | **168 µs** |

Déjà intégrés avant RC5 (non retirés) : hachage sur `octets_empruntes`,
index plat à adressage ouvert (`IndexRapide`), `PleinCourt`, voie NEON
`vqtbl4q_u8`, Philox puissance-de-2.

---

## 2. Optimisation retenue (UNE, en deux points cohérents)

 **`Table::neuf` : réserver 8 192 paires au lieu de 4 096.**
 La boucle du banc pousse 5 000 paires de 48 octets. Avec 4 096, le
 4 097ᵉ `items.push` déclenche un `realloc` (~196 Kio copiés) suivi d'une
 seconde arène. 8 192 couvre le banc sans recopie. Coût : 393 Kio réservés
 au lieu de 196 Kio, payés une seule fois par table (l'allocation est
 paresseuse côté pages).

 **`Table::inserer` : accès direct `rep_mut_nv()` au lieu de
 `self.0.borrow_mut()`.** La voie i64 rapide (`inserer_i64`) utilise déjà
 cet accès sans contrôle dynamique, avec l'invariant documenté : moteur
 monothreadé (`Rc` jamais `Arc`), aucun réemprunt possible pendant la
 méthode (le hachage, `valeur_egale` et `index.inserer` ne touchent aucune
 autre table). Le drapeau `RefCell` est donc toujours libre ; économie du
 test/incrément sur 5 000 insertions.

Aucune sémantique modifiée : mêmes `items` (ordre d'insertion), même
`index`, même validation `valeur_egale`, mêmes hash canoniques.

### Candidats écartés (mesurés, non retenus)

| Candidat | Résultat A/B | Verdict |
|---|---|---|
| Hash 8 octets en un mot (FNV remplacé par `mix64`) | **+9 %** tables (mauvaise dispersion sur `k1…k5000`) | rejeté |
| `IndexRapide` réduit à 4 096 | +4 % tables (deux réhashages) | rejeté |
| `IndexRapide` porté à 16 384 | +1 % tables (zeroing 192 Kio) | neutre, rejeté |
| `array_part` paresseux (ajouté aux deux points) | −3,1 % tables, +complexité 6 chemins | non retenu (gain dans le bruit) |
| RNG parallèle (4/8 threads, états splitmix64 décalés, sortie **bit-identique** vérifiée) | +16 à +71 % (coût `thread::scope` > gain de défauts de page) | rejeté |

---

## 3. Mesure A/A/A — rotations appariées c6 + c16

Harnais `/tmp/opencode/rc5_agents/measure.py` : 21 rotations entrelacées
(`A B / B A` alterné à chaque rotation et chaque cœur), 42 tirages par
binaire, `taskset -c 6` et `-c 16`. Médianes en ns (`temps()` interne).

| Binaire | Table médiane (ns) | RNG médiane (ns) |
|---|---|---|
| `rose_base_now` (avant) | 333 776 | 167 544 |
| `rose_final` (après) | **327 208** | 168 120 |
| Δ (B−A) | **−6 568 ns (−2,0 %)** | +576 ns (bruit) |

Par cœur : c6 −6 481 ns, c16 −7 520 ns sur la phase tables. Le bruit A/A
(même binaire des deux côtés, 15 rotations) était de −1 424 ns (0,4 %) sur
tables : le gain est ~4,6× le bruit et reproductible sur les deux cœurs.

Contrôle A/A/A des 11 valeurs du global à chaque salve : **identiques**
(base et final). Point fixe `b3ed9262dd54776a6dbd07bc12368d0e` produit
**×2** (base et final, `rose rir compilateur/emetteur.rose`, 263 520 o,
`cmp` octet-pour-octet).

---

## 4. Micro-jumeau — plancher de la phase RNG

Le micro-jumeau `/tmp/opencode/rc5_agents/fault_test.rs` isole la vraie
nature du coût RNG :

```
remplir_hasard 1 Mo, 64 octets alphabet, NEON vqtbl4q_u8 :
  appel à froid (premier remplissage d'un tampon neuf) : 266 384 ns
  appels suivants (mêmes pages, même arène)            :  41 008 ns
```

La phase RNG du banc (~168 µs mesurées) est donc **~75 % de défauts de page
sur 1 Mio de mémoire anonyme neuve** (256 pages de 4 Kio) et ~25 % de calcul
splitmix64+NEON. `THP` est désactivé sur la machine (`enabled [never]`) :
aucune page géante ne peut réduire le nombre de défauts. Le parallélisme ne
les réduit pas non plus (thread spawn 64 µs pour 4 threads > gain), et la
sortie doit rester bit-identique à la suite splitmix64 séquentielle.
**Plancher atteint pour RNG dans `run.rs` : aucun changement mesurable.**

Le micro-jumeau tables (`micro/`, 5 000 clés) confirme que le reste du
goulot est hors périmètre : `format!("k{}", f)` (dans la superinstruction
`interp.rs`) coûte 123 µs, `Valeur::s`/`ChaineRep::plein` + allocation ~67 µs
sur les 190 µs de construction de clé. La partie `run.rs` (insertion pure)
est ~94 µs ; les deux changements ci-dessus en retirent ~7 µs mesurés.

---

## 5. Validation complète

| Gate | Résultat |
|---|---|
| `cargo build --release --bin rose` (target isolé) | OK |
| `cargo test --release` | **529 passed, 0 failed** (265+249+0+6+8+1+0 ; 6 ignored) |
| 9 domaines × 9 tirages × c6 **et** c16 | **81/81** sorties octet-identiques par cœur |
| `bench_global.rose` — 11 valeurs | identiques base/final (contrôlées à chaque rotation) |
| Point fixe auto-hébergé | `b3ed9262…` ×2, `cmp` identique |
| Nouveaux `rug`/GMP | aucun |
| Commit / publication | aucun |

---

## 6. État final factuel

- **Fait** : deux modifications de `rose/src/run.rs` (`Table::neuf`,
  `Table::inserer`), aucune autre.
- **Vérifié** : gain tables −2,0 % (6,6 µs) en A/A/A apparié c6+c16, RNG
  inchangé (plancher prouvé par micro-jumeau), 529 tests, 81/81 ×2 cœurs,
  11 valeurs globales, point fixe ×2.
- **Reste** : le gros du banc global est hors de ce périmètre
  (`interp.rs` : superinstructions, `format!` des clés ; défauts de page
  RNG ; `jit.rs` : carrés). Le gain ci-dessus est un **constat daté du
  20/11/2026**, à re-mesurer après tout changement de topologie ou de
  compilateur.
