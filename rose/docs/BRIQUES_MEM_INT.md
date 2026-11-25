# BRIQUES_MEM_INT — interruptions génériques (B3) + mémoire déterministe (B4)

**État livré :** deux briques Rose pures, deux manifestes `.bud`, deux pilotes,
ce document. Worktree exclusif `/tmp/opencode/v2mem`, HEAD de départ
`e4c8e59538937cd5f2fc00f6441bf13278ead73d`. **Aucun commit, aucun push,
aucun fichier existant modifié.**

---

## 1. Périmètre et doctrine

`rose/docs/BRIQUES_UNIVERSELLES.md` fixe l'ordre de dépendance
`B5 → B2 → B6 → B1 → B4 → B3 → B8 → B7 → B9` et la règle de
`briques/README.md` : **une brique décrit un mécanisme, jamais un appareil**.

| Fichier créé | Rôle |
|---|---|
| `briques/interruptions/vecteurs.rose` | B3 — table de vecteurs, handlers déclaratifs, sauvegarde/restauration |
| `briques/interruptions/vecteurs.bud` | manifeste de la brique B3 (données seulement) |
| `briques/interruptions/tests_vecteurs.rose` | pilote B3 (32 lignes de sortie) |
| `briques/memoire/arena.rose` | B4 — allocateur déterministe + carte de translation |
| `briques/memoire/arena.bud` | manifeste de la brique B4 (données seulement) |
| `briques/memoire/tests_arena.rose` | pilote B4 (50 lignes de sortie) |

Aucun nom de CPU, d'OS, de contrôleur, de bus, de format de table de pages
ni d'appel système (`IDT`, `PIC`, `APIC`, `GIC`, `PLIC`, `x86`, `aarch64`,
`mmap`, `VirtualAlloc`...) n'apparaît dans le code des deux briques. La
documentation et les `.bud` ne nomment aucun contrôleur : `mappage.controleur:
GENERIQUE`.

Rose n'a pas de `importer` : une brique se **concatène** avec le programme,
comme `compilateur/brique_runtime.rose` et `compilateur/gc_rose.rose`.

```bash
cat briques/memoire/arena.rose briques/interruptions/vecteurs.rose mon_noyau.rose > /tmp/prog.rose
/tmp/opencode/C0_h.elf /tmp/prog.rose > /tmp/prog.elf     # ELF64 aarch64
```

---

## 2. Dépendances

### 2.1 Dépendance conceptuelle sur B2

Les deux briques supposent **les déclarations de B2** (`matériel/accès`) :
`largeur_mot`, `boutisme`, largeur d'un registre logique. Elles ne les
importent pas : chaque valeur est un `entier` Rose et sa largeur effective
est fixée par le `.bud` de l'assemblage. Le passage de l'entier logique à
des octets matériels est fait par la brique d'invocation (B1) et les
primitives B2 — **jamais** par `arena.rose` ou `vecteurs.rose`.

### 2.2 Dépendances réelles du code

Aucune bibliothèque externe : ni libc, ni `std`, ni FFI C, ni helper Rust,
ni GMP. Seules les primitives déjà émises par le compilateur C0 sont
utilisées : `entier`, `liste<entier>`, `joindre`, `longueur`, `partie_entiere`,
`si`/`sinon`, `tant_que`, `>>`.

Les deux briques sont **indépendantes l'une de l'autre** : aucune fonction
de l'une n'appelle l'autre, aucun nom ne se recouvre (préfixes `am_`/`vt_`).
Le pilote combiné (§6) le prouve.

### 2.3 Convention d'état — pourquoi une `liste<entier>`

Constat G5 mesuré et déjà documenté dans `brique_runtime.rose` et
`gc_rose.rose` : une lecture de `table` est typée `?` par le vérificateur
(`Argument de lire: ? reçu, table attendu`, `La fonction retourne ?,
attendu entier`) et `inserer` sur une clé existante crée un doublon. Les
deux briques stockent donc tout leur état dans une `liste<entier>` :

- l'indexation d'une liste est typée `entier` (pas `?`) ;
- chaque mise à jour reconstruit explicitement une liste neuve
  (`am_set`, `vt_set`) — aucune mutation cachée, aucun doublon ;
- chaque opération **rend le nouvel état** et l'appelant le reprend :
  `$a = arena_allouer($a, 8)`, `$t = vtable_installer($t, 0, 2)`.

---

## 3. Brique B4 — `briques/memoire/arena.rose`

### 3.1 Allocateur

```rose
fn arena_nouvelle(base: entier, taille: entier) -> liste<entier>
fn arena_allouer(a: liste<entier>, n: entier) -> liste<entier>
fn arena_liberer(a: liste<entier>, bloc: entier) -> liste<entier>
fn arena_compassion(a: liste<entier>) -> liste<entier>     # compaction
fn arena_compacter(a: liste<entier>) -> liste<entier>      # alias
```

`base` est une **origine logique**, pas une adresse physique. Les décalages
sont relatifs à cette base. Stratégie figée et reproductible :

1. **first-fit** par ID croissant parmi les blocs libres ; si le bloc est
   plus grand que la demande, le reste devient un nouveau bloc libre ;
2. sinon **bump** à la fin, si la capacité le permet ;
3. sinon **échec explicite** (`statut = -1`, `dernier_bloc = -1`, aucun bloc
   créé).

Les ID de blocs sont **stables** : la compaction déplace les données sans
changer les identifiants, donc `arena_liberer` reste valable après
`arena_compassion`.Une double libération est refusée (`statut = -1`).

Layout de l'état (10 champs puis 3 entiers par bloc) :

| Index | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 |
|---|---|---|---|---|---|---|---|---|---|---|
| Champ | base | taille | bump | nb blocs | n_alloc | n_libere | n_compact | dernier bloc | statut | gain |

Descripteur `i` : `debut = a[10+3i]`, `longueur = a[11+3i]`, `etat = a[12+3i]`
(0 libre, 1 occupé). Lecture : `arena_debut`, `arena_longueur`, `arena_etat`,
`arena_adresse_logique`, `arena_base`, `arena_taille`, `arena_bump`,
`arena_nb_blocs`, `arena_allocs`, `arena_liberes`, `arena_compactions`,
`arena_dernier_bloc`, `arena_statut`, `arena_gain`.

### 3.2 Carte de translation générique

```rose
fn carte_nouvelle() -> liste<entier>
fn carte_mapper(carte: liste<entier>, v: entier, p: entier, taille: entier) -> liste<entier>
fn carte_traduire(carte: liste<entier>, v: entier) -> entier
fn carte_traduire_inverse(carte: liste<entier>, p: entier) -> entier
fn carte_taille(carte: liste<entier>, v: entier) -> entier
```

`v` et `p` sont des entiers abstraits ; `taille` est une étendue logique.
Aucune taille de page, aucun niveau de table, aucun bit de protection n'est
codé dans la brique : la granularité vient de `carte.granularite` dans
`arena.bud`. Un `v` déjà présent est remplacé (pas de doublon). Une traduction
sans entrée rend `-1`.

Exemple générique :

```rose
$a = arena_nouvelle(4096, 64)
$a = arena_allouer($a, 8)       # bloc 0, debut 0
$a = arena_liberer($a, 0)
$a = arena_compassion($a)       # blocs vivants tassés vers 0
$c = carte_nouvelle()
$c = carte_mapper($c, 100, 4096, 16)
>> carte_traduire($c, 100)      # 4096
```

---

## 4. Brique B3 — `briques/interruptions/vecteurs.rose`

```rose
fn vtable_nouvelle(n: entier) -> liste<entier>
fn vtable_installer(t: liste<entier>, idx: entier, handler: entier) -> liste<entier>
fn vtable_programme(t: liste<entier>, handler: entier, actions: liste<entier>) -> liste<entier>
fn vtable_handler(t: liste<entier>, idx: entier) -> entier
fn vtable_sauvegarder(contexte: liste<entier>) -> liste<entier>
fn vtable_restaurer(contexte: liste<entier>, cadre: liste<entier>) -> liste<entier>
fn vtable_appliquer(t: liste<entier>, handler: entier, contexte: liste<entier>) -> liste<entier>
fn vtable_dispatch(t: liste<entier>, idx: entier, contexte: liste<entier>) -> liste<entier>
```

`vtable_nouvelle(n)` crée une **donnée** : `n` cases de handlers (initialisées
à `-1`) et `n` emplacements de programme de 25 entiers chacun. `handler`
est un **identifiant entier**, jamais une adresse ni un pointeur de fonction.

Rose n'a ni fonction-valeur ni closure : le handler est décrit par une
**table d'actions déclarative** (motif déjà utilisé par `jardins/gui.rose`),
codée en triplets `[op, arg1, arg2]` :

| `op` | Action |
|---|---|
| 0 | rien |
| 1 | `contexte[arg1] = arg2` |
| 2 | `contexte[arg1] = contexte[arg1] + arg2` |
| 3 | `contexte[arg1] = contexte[arg2]` |
| 4 | échanger `contexte[arg1]` et `contexte[arg2]` |

8 actions maximum par handler, indices hors contexte ignorés. Le noyau
décide, dans son `.bud`, comment un identifiant de handler devient une vraie
routine machine : la brique ne connaît ni routine ni adresse.

`vtable_dispatch` impose l'ordre **sauvegarde → application → restauration**
et rend un rapport :

```
[handler, statut, largeur] + contexte_restaure + contexte_apres_handler
```

Le contexte restauré est identique au contexte d'entrée (preuve que
l'interrompu reprend son état) ; le second bloc montre l'effet du handler.
Un index hors table rend `handler = -1`, `statut = -1`, sans action.
Lecteurs : `rapport_handler`, `rapport_statut`, `rapport_longueur`,
`rapport_restaure(r, j)`, `rapport_apres(r, j)`.

Exemple générique :

```rose
$t = vtable_nouvelle(4)
$t = vtable_installer($t, 0, 2)
$p = [1, 0, 99]
$t = vtable_programme($t, 2, $p)      # poser(contexte[0], 99)
$r = vtable_dispatch($t, 0, [10, 20, 30])
>> rapport_restaure($r, 0)            # 10
>> rapport_apres($r, 0)               # 99
```

---

## 5. Mappage sur du matériel via `.bud` (le point clé)

Les deux `.bud` sont des **manifestes de données** : aucune fonction, aucun
`si`, aucune boucle. Le compilateur les lit avant l'analyse ; c'est ce qui
permet un seul backend pour toutes les cibles :

- `cible.architecture: GENERIQUE`, `boutisme`, `largeur_mot` fixent la
  largeur des entiers logiques manipulés par les briques ;
- `interruptions/vecteurs.bud` : `materiel.vecteurs` (nombre d'entrées),
  `registre_largeur` (largeur d'un registre logique),
  `mappage.largeur_vecteur`, `mappage.abi_sauvegarde` et
  `mappage.traduction_handler`. Le noyau construit la table Rose, puis
  sérialise ses entiers vers son matériel **par la brique d'invocation B1**
  paramétrée par ce manifeste. Aucun encodeur spécialisé n'est ajouté au
  compilateur ;
- `memoire/arena.bud` : `materiel.regions` fournit la base logique,
  `carte.granularite`/`niveaux`/`bits_attributs` décrivent la translation
  sans rien imposer à `arena.rose`. Les bits de protection et le format
  matériel appartiennent au noyau, pas à la brique.

**Réversibilité (garantie 4 de la doctrine) :** remplacer un `.bud` par un
autre (autre nombre de vecteurs, autre largeur de registre, autre
granularité) ne change aucune ligne des deux fichiers `.rose`.

---

## 6. Validation — preuves de sortie réelles

Commandes exécutées depuis `/tmp/opencode/v2mem` (C0 =
`/tmp/opencode/C0_h.elf`, compilateur Rust = `/home/clausake/Stefan/ROSE/rose/rose`) :

```bash
rose verif briques/interruptions/vecteurs.rose       # vérifié, rc=0
rose verif briques/memoire/arena.rose                # vérifié, rc=0
$C0 briques/interruptions/vecteurs.rose > v.elf      # rc=0
$C0 briques/memoire/arena.rose      > a.elf          # rc=0

cat briques/interruptions/vecteurs.rose briques/interruptions/tests_vecteurs.rose > pv.rose
cat briques/memoire/arena.rose      briques/memoire/tests_arena.rose      > pa.rose
rose verif pv.rose && rose verif pa.rose             # vérifié, rc=0
$C0 pv.rose > pv.elf && $C0 pa.rose > pa.elf
chmod +x pv.elf pa.elf && ./pv.elf && ./pa.elf
```

Résultats mesurés :

| Fichier ELF | `od -An -tx1 -N4` | `od -An -tu2 -j18 -N2` | Sortie | rc |
|---|---|---|---|---|
| `v.elf` (B3 seule) | `7f 45 4c 46` | `183` (aarch64) | — | 0 |
| `a.elf` (B4 seule) | `7f 45 4c 46` | `183` (aarch64) | — | 0 |
| `pv.elf` (pilote B3) | `7f 45 4c 46` | `183` | 32 lignes | 0 |
| `pa.elf` (pilote B4) | `7f 45 4c 46` | `183` | 50 lignes | 0 |
| pilote combiné B4+B3 | `7f 45 4c 46` | `183` | 82 lignes | 0 |

- Les 82 lignes du pilote combiné sont **octet pour octet** la concaténation
  des 50 lignes du pilote B4 et des 32 lignes du pilote B3 (`diff` vide) :
  les deux briques coexistent sans collision.
- Les sorties ELF et interpréteur Rust (`ROSE_SANS_JIT=1 rose <fichier>`)
  sont identiques pour les deux pilotes (`diff` vide).
- Le vérificateur auto-hébergé C0 (`/tmp/opencode/vf_sans_cargo/rose verif`)
  accepte également les deux briques et les deux pilotes concaténés.
- Le pilote B4 vérifie notamment : base logique 4096 ; first-fit avec split
  (bloc 0 réutilisé, reste 3 en bloc 3) ; double libération `-1` ;
  compaction `debut(2) = 5`, `bump = 9`, `gain = 55` ; échec de capacité
  sans création de bloc ; remplacement d'une entrée de carte sans doublon.
- Le pilote B3 vérifie notamment : handler installé, programme appliqué
  (`[10,20,30] → [99,25,99]`), contexte restauré identique à l'entrée,
  handler sans programme, index hors table `-1/-1`.

Les `.bud` ne sont **pas encore lus par le compilateur** (convention nouvelle,
voir `briques/README.md`) : ils sont des spécifications de données
exécutables par le futur chargeur de manifeste, pas des entrées de `C0`.

---

## 7. Limites explicites

1. **Pas de fonction-valeur.** Rose n'a ni closure ni pointeur de fonction :
   un handler est un identifiant + un programme de 8 actions max. Un noyau
   qui veut des routines arbitraires les traduit à la frontière (son `.bud`
   et son propre code), pas dans la brique.
2. **Pas d'appareil.** Aucun vecteur n'est armé, aucune interruption n'est
   masquée, aucune page n'est protégée : la brique produit des données. Le
   matériel reste hors périmètre, par construction.
3. **Priorités / masquage absents.** Le modèle B3 couvre table, dispatch et
   ABI de sauvegarde ; la politique de priorité est une propriété de la
   cible, à déclarer, pas un code de brique.
4. **Taille bornée par construction.** `n` vecteurs, 8 actions/handler,
   25 entiers/handler, 3 entiers/bloc. Aucune borne dynamique n'est
   imposée par le compilateur ; c'est le `.bud` d'assemblage qui fixe le
   budget. La liste d'état peut croître si un appel dépasse ces bornes,
   sans jamais dépasser la taille explicitement demandée par l'appelant.
5. **Allocation au sens Rose.** Les `liste<entier>` sont des valeurs que
   la brique **reconstruit explicitement** (`am_set`, `vt_set`). Il n'y a
   ni `malloc` hôte, ni appel système, ni GC déclenché par la brique. En
   revanche, le langage Rose alloue bien des cellules de liste : la
   garantie porte sur l'absence d'allocation *implicite et invisible*, pas
   sur l'absence totale de cellules de liste.
6. **Coût non mesuré en cycles.** `latence_max: 0` dans les `.bud` signifie
   « non bornée / non mesurée », conformément à la convention. Les seules
   mesures publiées ici sont des sorties et des formats d'ELF, pas des
   durées matérielles.
7. **Pas de fausse protection mémoire.** `carte_traduire` est une fonction
   de traduction logique ; elle ne vérifie ni droits, ni alignement, ni
   appartenance à une région matérielle. Ces contrôles sont ceux du noyau.
8. **Pas de concurrence.** Les opérations sont déterministes et séquentielles ;
   aucun verrou n'est fourni (un noyau ajoutera sa propre discipline).
