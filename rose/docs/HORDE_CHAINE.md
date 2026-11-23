# HORDE_CHAINE — représentation des chaînes (ChaineRep rope) en Rose pur

**Constat daté : 23/11/2026.** Worktree exclusif `/tmp/opencode/horde_rope`,
HEAD de départ `101b48d25a85a3a4a5be80232af9dda7bff037f1`. Aucun commit,
aucune publication. Trois fichiers créés :

| fichier | rôle |
|---|---|
| `compilateur/chaine_rose.rose` | rope AVL en Rose pur, 366 lignes |
| `compilateur/tests_chaine_rose.rose` | banc : 151 980 concaténations, 20 scénarios, 247 lignes |
| `rose/docs/HORDE_CHAINE.md` | ce document |

## 1. Ce qui a été lu (lecture seule)

* `rose/src/run.rs:311` — `NoeudChaine` : **6 variantes** (`Plein`, `PleinCourt`,
  `Concat` + `RefCell<Option<String>>`, `FichierMmap`, `PleinIndexe`,
  `FichierMmapIndexe`) ; `ChaineRep{n, octets, noeud}` à `:416`.
* `rose/src/run.rs:527` — `aplatir` : cache dans `Concat`, parcours itératif.
* `rose/src/run.rs:609` — `tranche` : cache en tête, puis descente avec tables
  d'index pour les feuilles non ASCII.
* `compilateur/moteur_ops.rose` — `o_ch` (codes via `sous_chaine` caractère par
  caractère), `op_ch_longueur` (`o_long` = `longueur(v)-1`, O(1)), `op_ch_joint`
  (recopie les deux listes de codes), `op_ch_idx` (`caractere(o_chcode(s,k))`),
  `op_ch_contient` (scan O(n·m)). Vague OPT_CHAINES : `Concat` + cache
  d'aplatissement, `concat_possessif`, accélérateur RIR `AppendChaine`.

Le repli J6 est une **liste plate de codes** : longueur O(1), mais concaténation
et tranche O(n) (O(n²) en boucle d'accumulation). Le port ci-dessous remplace
cette liste par un arbre.

## 2. Représentation retenue

Une rope = **une `liste<entier>` (arène) append-only**. Rose interdit
l'affectation par index et l'ABI G5 réalloue les listes sur `joindre` : tout est
donc ajouté à la fin, jamais réécrit, et les index absolus restent valides.

* **Run de codes** : `t` entiers consécutifs = points de code Unicode.
* **Cellule de nœud** (8 entiers) :
  `+0` kind (1 feuille / 2 concat), `+1` premier code ou fils gauche,
  `+2` nombre de codes ou fils droit, `+3` nombre de caractères du sous-arbre,
  `+4` hauteur AVL, `+5..7` réservés.
* **Cellule d'état** (8 entiers, **toujours la dernière**) :
  `+0` = 0, `+1` racine, `+2` début du cache, `+3` longueur du cache,
  `+4` longueur totale, `+5` hauteur, `+6` dernier nœud, `+7` réservé.

Invariant : `longueur(a) - 8` pointe toujours sur l'état courant ; la mise à
jour se fait par ajout d'une cellule d'état, sans reconstruction de liste.

### Complexités (mesurées et vérifiées sur l'invariant)

| opération | complexité | preuve |
|---|---|---|
| `cr_longueur` | **O(1)** | champ `+4` de l'état ; exact à chaque tour des 151 980 concaténations |
| `cr_concat` | **O(log n) amorti** | jonction AVL ; hauteur 11→16 pour 1 000→20 000 feuilles |
| `cr_tranche` | **O(log n + k)** sans cache, **O(k)** avec | descente par `cr_collecter` ; cache testé en tête |
| `cr_aplatir` | **O(n)** une fois, puis O(1) | run de codes ajouté une seule fois, `cr_cache_debut != 0` |
| `cr_comparer` | **O(min(n,m))** | comparaison des deux runs de codes après aplatissement |

Arbre **AVL complet** (jonction récursive + rotations simples et doubles) :
l'invariant `|h(gauche) − h(droite)| ≤ 1` et `h(noeud) = 1 + max(hg,hd)` est
vérifié récursivement sur **tous les nœuds** de chaque scénario
(`verif_noeud`, 0 erreur sur 20 scénarios).

### Relation avec les 6 variantes Rust

| Rust `NoeudChaine` | port Rose | détail |
|---|---|---|
| `Plein` / `PleinCourt` | Cellule feuille + run de codes | un run par feuille, déjà son propre cache |
| `Concat` + cache | Cellule concat + cache d'état | cache au niveau de l'état (une seule matérialisation) |
| `PleinIndexe` | index implicite | `cr_tranche` compile `début`/`fin` en points de code par descente, sans reconstruire d'index |
| `FichierMmap`, `FichierMmapIndexe` | non construits par ce module | le module ne lit pas de fichier ; l'arène reste en mémoire Rose |

La sémantique `longueur` = **points de code** est la même que `ChaineRep.n`
(`s.chars().count()`) : `ordre(sous_chaine(s,k,k+1))` donne le code Unicode,
y compris astral (😀 = 128512).

## 3. Parité octet pour octet

### Banc principal (interpréteur Rust = référence)

`cat compilateur/chaine_rose.rose compilateur/tests_chaine_rose.rose > /tmp/t.rose`
puis `ROSE_SANS_JIT=1 rose /tmp/t.rose` :

```
SC:1000:0:0:11:999        ...  SC:20000:3:0:16:19999
UNICODE:0:2000:2000
CMP:0
VIDE:0:0:0
TOTAL:0:151980:20
```

* **0 erreur** sur 151 980 concaténations réelles, 20 scénarios
  (tailles 1 000 / 2 000 / 5 000 / 10 000 / 20 000 morceaux × 4 motifs).
* Parité des octets : `parite_codes` compare **chaque code** du cache aplati à
  la génération indépendante du morceau par `+` du moteur Rust.
* Parité des tranches : `parite_tranche` compare `cr_tranche` à une chaîne
  construite par le moteur Rust, avant **et** après aplatissement, sur 4
  positions par scénario (début, milieu, fin, hors limites).
* Unicode : 2 000 caractères mêlant `é`, `中`, `😀`, `Ω`, `a` ; codes et
  tranches identiques, longueur 2 000 attendue = 2 000 obtenue.
* Comparaison : `-1 / 0 / 1` et égalité conformes sur `abc`, `abd`, `ab`.

### C0 → ELF, natif = ELF

Pilote `>>` (format attendu par l'émetteur auto-hébergé) concaténé avec
`chaine_rose.rose`, compilé par `C0_j6.elf` (md5
`7bb091200cf927022eb9d86a548d2502`, chaîne J6) :

```
./C0_j6.elf drv_c0_all.rose > drv_c0.elf     # 36 196 o, magic 7f454c4602010100
ROSE_SANS_JIT=1 rose drv_c0_all.rose > nat.out
./drv_c0.elf > elf.out
cmp nat.out elf.out                          # 0
md5 nat.out = md5 elf.out = 0a09163bb6b1dcb456a98ca42e4aea37
```

Sortie identique (5 lignes) :

```
ROPE:200:9:200
UNICODE:60:60
CMP:-1:1
TR:bd
VIDE:0:0
```

Un C0 a aussi été émis depuis CE worktree :
`rose rir compilateur/rose_natif.rose` → 903 626 o (md5 `921db7cb…`) puis
`rose compilateur/emetteur_elf.rose rose_natif.rir` → ELF 2 153 820 o
(md5 `9736aace…`). La chaîne de régénération de l'émetteur donne
`F1 = F2` (910 236 o, md5 `5cdac027f999c96b4fcab85c3906d759`), mais cet
émetteur local attend un RIR en entrée : la preuve source→ELF ci-dessus est
donc faite avec le C0 de la chaîne J6, dont le contrat est source→ELF.

## 4. Performances (moteur Rust, interpréteur, `temps()` interne, ns convertis en ms)

Banc `bench_chaine.rose` (morceaux de 3 caractères, même graine des deux côtés) :

| morceaux | rope concat | `aplatir` | `+` natif (Rust ChaineRep) | repli `op_ch_joint` (O(n²)) | arena (entiers) | hauteur |
|---:|---:|---:|---:|---:|---:|---:|
| 2 000 | **290 ms** | 8 ms | 3 ms | 1 959 ms | 394 752 | 12 |
| 5 000 | **1 017 ms** | 20 ms | 8 ms | 12 191 ms | 1 098 352 | 14 |
| 20 000 | **3 574 ms** | 80 ms | 50 ms | ~195 s (extrapolé en n²) | 5 035 040 | 16 |

* **Gain mesuré vs repli** : 6,75× à 2 000 morceaux, **11,98× à 5 000**.
  Le repli recopie la liste entière à chaque ajout (`op_ch_joint`) ; la rope
  ne fait que O(log n) nœuds. À 20 000, le repli n'a pas été exécuté
  (extrapolation quadratique à partir des deux points mesurés).
* Le `+` natif du moteur Rust reste plus rapide (rope Rust avec
  `concat_possessif` + accélérateur RIR) : 50 ms vs 3 574 ms à 20 000, soit
  71×. Ce port est celui du **moteur auto-hébergé G5**, où le `+` recopie :
  il n'y a pas de rope Rust disponible dans l'ELF.
* Coût par concaténation : ~145 µs à 2 000, ~203 µs à 5 000, ~179 µs à
  20 000 — croissance logarithmique, pas quadratique.

### Tranches (5 000 morceaux, 1 000 tranches de 12 caractères)

| variante | temps |
|---|---:|
| descente arbre, **sans cache** (`cr_tranche` sur la rope brute) | 92 ms |
| **avec cache** (`cr_tranche` après `cr_aplatir`) | **13 ms** |
| `cr_aplatir` (une seule fois) | 20 ms |

Gain du cache : **7,1×** sur 1 000 tranches. La descente O(log n) reste
utilisable sans matérialisation : 92 ms pour 1 000 tranches.

## 5. Validation

```
rose verif compilateur/chaine_rose.rose                 -> vérifié
rose verif chaine_rose.rose + tests_chaine_rose.rose     -> vérifié
ROSE_SANS_JIT=1 rose tests_all.rose                      -> rc=0, TOTAL:0:151980:20
C0_j6.elf drv_c0_all.rose                                -> ELF 36 196 o, rc=0
cmp natif ELF                                            -> 0 (md5 identiques)
rose rir compilateur/emetteur.rose                       -> b3ed9262dd54776a6dbd07bc12368d0e ×2
bash scripts/validation_finale.sh                        -> RESULTAT: 0 échec(s)
```

Détail de `validation_finale` : tests Rust 314 + 327 passés / 0 échec ;
point fixe `b3ed9262…` identique ×2 ; AOT 75/75 ; JIT 25/25 ; WASM 92 o ;
RVM 317811 ; batterie AZ **31 CONFORME / 0 non-conforme** ; HOST-1 fib
317811. Garde-fou mémoire `scripts/garde_fou_mem.sh 12000000` appliqué à
toutes les commandes longues.

## 6. Reproductibilité

```bash
cd /tmp/opencode/horde_rope   # worktree de ce chantier
cat compilateur/chaine_rose.rose compilateur/tests_chaine_rose.rose > /tmp/t.rose
rose verif /tmp/t.rose
ROSE_SANS_JIT=1 rose /tmp/t.rose                # TOTAL:0:151980:20
rose rir compilateur/emetteur.rose | md5sum     # b3ed9262…
bash scripts/validation_finale.sh               # 0 échec
```

Les trois fichiers possédés sont les seuls fichiers source créés ; aucun
commit, aucun push, aucun autre fichier source modifié.
