# Goulot structurel global de Rose — méthode Rust (architecture d'abord, cas par cas après)

**Horodatage des salves** : 2026-11-18T14:49:59Z → 14:50:04Z (salve de phases 1),
14:51:49Z → 14:51:58Z (salve de phases 2), 14:54:37Z → 14:55:54Z (salve de trame 1),
~14:55:55Z → 14:56:47Z (salve de trame 2). Tout est un **constat daté** (règle `AGENTS.md`) :
re-mesurer après tout rebuild, tout changement de topologie ou tout changement d'état machine.

**Binaire mesuré** : `/home/clausake/Stefan/ROSE/rose/rose`, SHA256
`7ba89679654986ff980d5aca55ae3f69bb45549e6950775ffdcb52c705231469`. Hash contrôlé
**identique avant et après chaque salve** par les harnais (voir sorties `DEBUT`/`FIN` ci-dessous).
Rose 0.1.5. Le dépôt a continué à bouger pendant la mission (`git log` est passé de `bf379ed`
à `d74ecdc` ; `rose/src/interp.rs` et `rose/src/wasm_lecteur.rs` modifiés par d'autres agents)
: **aucun fichier source n'a été modifié par cette mission, aucun commit n'a été fait**.

**Machine** : aarch64 GB10, cœur X925 15 (`scaling_governor=performance`,
`scaling_cur_freq=3900000`), toutes les exécutions pinées par `taskset -c 15`.
D'autres agents compilaient en parallèle (`rustc` à 315–427 % de CPU sur les cœurs 2/14,
`g2-rose-final` à 100 % sur le cœur 3) : les **valeurs absolues dérivent d'un facteur ~2 entre
les deux salves de trame**, mais les **ratios sont reproductibles au pour cent**. C'est précisément
pour cela que la conclusion repose sur des rapports, jamais sur une valeur absolue isolée.

**Périmètre** : un seul fichier du dépôt écrit — ce document. Expériences, données brutes et
harnais sous `/tmp/opencode/goulot/`.

---

## 1. Protocole et programme représentatif

### 1.1 Le programme

`/tmp/opencode/goulot/mix.rose` — 1 Mo de texte, comptage, table de fréquences, liste, tri,
affichage. Il mélange volontairement les quatre familles de valeurs de Rose : chaîne
(`lire_texte`, `sous_chaine`, `longueur`), table (`inserer`, `cle_existe`, indexation),
liste (`joindre`), tableau typé et nombres (tri par insertion).

```rose
$d0 = temps()
$s = lire_texte("/tmp/opencode/goulot/in_1m.txt")   # 1 048 576 octets ASCII
$n = longueur($s)
$t = table()
pour $i de 0 vers 199 {
  $pos = $i * 5000
  $w = sous_chaine($s, $pos, $pos + 6)
  si (cle_existe($t, $w)) { $t[$w] = $t[$w] + 1 } sinon { inserer($t, $w, 1) }
}
$l = [0 : entier]
pour $j de 0 vers 199 { joindre($l, $j * $j) }
$a2 = tableau(200)
pour $k de 0 vers 199 { $a2[$k] = $l[$k] }
pour $a de 1 vers longueur($a2) - 1 { /* tri par insertion */ }
$d1 = temps()
>> $n
>> longueur($t)
>> longueur($l)
>> $a2[0]
>> $d1 - $d0
```

Résultat vérifié à **chaque** échantillon : `1048576`, `141`, `199`, `39204` (le temps
interne variant). Entrée : 1 048 576 octets, 100 mots distincts `mot000…mot099` tirés avec
`random.seed(3021)`.

### 1.2 Horloges et instrument

- `ROSE_TIMING=1` → `TIMING parse / transform / jitnew / compiletout / compileprincipal / exec / total`
  (`interp.rs:5122-5127`) et les sous-phases JIT (`jit.rs:1634-1732`), plus
  `TIMING entree_main / console / pret / post_spawn` (`main.rs:1193-1198`).
- `rose debug-lenteur -n 9` → ventilation processus (lancement, programme minimal, analyse seule).
- `temps()` Rose → horloge interne du calcul (`run.rs`, primitive `temps`).
- `wall` = `subprocess.run` complet (harness Python).

**9 échantillons par programme**, entrelacés par **rotation de l'ordre** à chaque round ;
médianes et min–max conservés dans les JSON.

### 1.3 Commande exacte de la salve de phases (sortie réelle)

```
$ cd /tmp/opencode/goulot && python3 mesure_phases.py salve1
DEBUT 2026-11-18T14:49:59.308961+00:00 binaire=7ba89679654986ff... PIN=15 N=9
  round  1 v1_io      exec=    0.931ms parse=0.0494ms wall=   2.475ms res=['1048576']
  round  1 v2_extract exec=  115.061ms parse=0.0453ms wall= 116.550ms
  round  1 v3_table   exec=  120.419ms parse=0.0555ms wall= 122.350ms
  round  1 v4_liste   exec=   65.752ms parse=0.0695ms wall=  67.328ms
  round  1 mix        exec=  200.245ms parse=0.0803ms wall= 202.192ms
  ... (9 rounds, rotation ; extrait)
FIN   2026-11-18T14:50:03.936958+00:00 binaire=7ba89679654986ff... identical=True
```

Confirmation indépendante (`phases_salve2.log`, 14:51:49Z → 14:51:58Z) :
`mix` exec médian **200,317 ms**, parse **0,070 ms** — écart inter-salves 0,04 %.

---

## 2. Résultat 1 — ventilation par phase du programme représentatif

Médianes de la salve 1 (9 échantillons, pinées, hash constant) :

| Phase (instrument `ROSE_TIMING`) | Médiane | % du `total` interne | Code |
|---|---:|---:|---|
| `parse` (lexeur + parseur + vérification) | 0,0803 ms | **0,0401 %** | `interp.rs:4640-4650` → `front::analyser_fichier` |
| `transform` (réparation, boucles → fonctions synthétiques) | 0,0261 ms | 0,0130 % | `interp.rs:4651-5035` |
| `jitnew` (abaissement RIR + `Jit::nouveau`) | 0,0205 ms | 0,0102 % | `interp.rs:5047-5071` |
| `compiletout` (déclaration/émission/finalisation JIT) | 0,0515 ms | 0,0257 % | `interp.rs:5072-5075`, `jit.rs:1636-1732` |
| `compileprincipal` (émetteur natif, repli RIR) | 0,0020 ms | 0,0010 % | `interp.rs:5078-5102` |
| **`exec` (exécution + `flush_out`)** | **200,2447 ms** | **99,9072 %** | `interp.rs:5105-5110` |
| `total` interne | 200,4307 ms | 100 % | |
| `wall − total` (fork/exec + harness Python) | 1,6787 ms | 0,84 % du wall | `main.rs:780-1208` |

**Constat [M]** : la compilation (toutes phases confondues) coûte **0,180 ms**, soit
**0,09 %** du temps interne. Le parseur + vérificateur coûtent **0,080 ms**, soit
**0,04 %**. **99,91 % du temps est dans l'exécution.** Optimiser le parseur, le vérificateur,
le RIR ou le JIT ne peut pas améliorer ce programme de manière mesurable : le goulot est
**après** la compilation.

`debug-lenteur -n 9` sur le même programme confirme avec une horloge différente :

```
lancement (rose version)                 215 µs   code Some(0), 69 o, 1er = 312 µs
état (rose info)                         201 µs
programme minimal                        320 µs   surcoût d'exécution = 105 µs
programme /tmp/opencode/goulot/mix.rose  201.00 ms  code Some(0), 1er = 201.32 ms
analyse seule (rose latex)               217 µs
```

### 2.1 Décomposition de `exec` par variantes incrémentales

| Variante | Contenu ajouté | `exec` médian (ms) | Δ (ms) | % de `mix` |
|---|---|---:|---:|---:|
| `v1_io` | lire 1 Mo + `longueur` | 0,8465 | 0,8465 | 0,42 % |
| `v2_extract` | + 200 × `sous_chaine` sur 1 Mo | 115,0404 | **114,1939** | **57,03 %** |
| `v3_table` | + table (141 clés, `cle_existe`/`inserer`) | 120,4192 | 5,3788 | 2,69 % |
| `v4_liste` | (départ de `v1`) + 200 `joindre` + tri tableau | 65,6520 | 64,8055 | 32,36 % |
| `mix` | tout | 200,2447 | — | 100 % |
| **résidu** | non additivité (variables supplémentaires) | 15,0200 | — | 7,50 % |

Le résidu n'est pas du bruit : `mix` déclare 15 variables (vu par
`ROSE_PROFIL_RIR` : `noms=[d0,s,n,t,i,pos,w,l,j,a2,k,a,x,b,d1]`), les variantes 6–10. Ce
résidu est **lui aussi imputable au goulot #1** (§3), dont la propriété est justement de
faire dépendre le coût du nombre de variables.

---

## 3. Résultat 2 — goulot structurel #1, avec preuve

### 3.1 Énoncé

> **Goulot #1 [M] : la frontière RIR → AST matérialise l'environnement ENTIER
> (`HashMap<String, Valeur>`) à chaque opération non-abstractable (appel de primitive,
> indexation, affectation par index). Le coût par opération est
> O(nombre de variables en portée) au lieu de O(1).**

Ce n'est ni (c) le parse/vérif (0,04 % mesuré), ni (e) l'ABI des appels JIT (aucun appel JIT
dans `mix` : `TIMING-JIT eligibles=[]`), ni (d) le boxage des chaînes/listes pris isolément
(le §3.5 montre qu'un tableau typé `Vec<i64>` est **aussi lent** qu'une liste boxée), ni
(a) la conversion JIT↔interpréteur prise seule (§3.5). C'est un mélange de (a) et (b) **à la
frontière RIR↔AST** : clonage de chaque `String` de nom **et** de chaque `Valeur` à chaque
opération non-abstractable.

### 3.2 Preuve par le code (lignes exactes)

`interp.rs:1773-1823`, `eval_avec_slots_fallback` (appelé à chaque opérande hors sous-ensemble,
cf. le commentaire même du code en `1780-1803`) :

```rust
let mut trame = self.trame_rir.clone();   // 1811
trame.clear();                            // 1812
for (i, n) in noms.iter().enumerate() {   // 1813
    if let Some(v) = slots.get(i) {
        trame.insert(n.clone(), v.clone());  // 1815 : String + Valeur clonés
    }
}
self.trames.push(trame);                  // 1818
let r = self.eval_interne(e);             // 1819
```

`interp.rs:2679-2697`, `affecter_index_ast` (chemin de **toute** affectation par index
depuis le RIR, voir `rir.rs:869-874` `Op::AffectIndex`) :

```rust
let mut trame = self.trame_rir.clone();   // 2686
trame.clear();                            // 2687
for (i, n) in corps.noms.iter().enumerate() {
    if let Some(v) = slots.get(i) { trame.insert(n.clone(), v.clone()); }  // 2689
}
self.trames.push(trame);
let r = self.exec(&instr);                // évaluation AST complète
```

Pourquoi ces chemins sont-ils atteints ? Parce que le JIT et la machine à slots refusent
tout ce qui n'est pas un opérateur abstrait :

- `jit.rs:439-441` : `compilable(ty)` = **entier / booléen / flottant / complexe uniquement** ;
- `jit.rs:1003-1027` : `rir_compilable_jit` accepte `Affect`, `RetourAbstrait`,
  `SautSiFauxAbstrait` et **rejette explicitement tout le reste** (`jit.rs:1022` :
  `// Tout le reste (AST, blocs, lire, affect index, afficher AST...) -> fallback`) ;
- `rir.rs:484-517` : le `pour` est bien abaissé en slots, mais son corps contient des appels
  (`joindre`, `sous_chaine`, `longueur`) qui retombent dans `eval_avec_slots_fallback`.

Mesure directe sur `mix` : `TIMING-JIT eligibles=[]`, et `PROFIL-RIR` affiche
`fonctions_abaissees=0 principal=true` — **aucune fonction de ce programme n'est compilée** ; tout passe par la
machine à slots + repli AST.

### 3.3 Preuve par la mesure — le coût croît avec le nombre de variables

Expérience contrôlée : même opération, même 100 000 itérations, même cœur, même binaire ;
on ajoute uniquement `$v0 … $v59 = 0..59` **jamais utilisés** avant la boucle. Si le coût
venait de l'opération elle-même, il ne changerait pas. S'il vient du clonage de trame, il
croît linéairement avec le nombre de variables déclarées.

Deux salves de 9 échantillons entrelacés, pinées, hash constant. Médianes `exec` (ms) :

| Programme | Variables ajoutées | Salve 1 (14:54:37Z) | Salve 2 (~14:56Z) | Ratio V=30 / V=0 |
|---|---:|---:|---:|---:|
| `v6_add` — `$s = $s + $j` (abstract) | 0 / 30 | 4,392 / 4,431 | 1,385 / 1,423 | **1,009 / 1,027** |
| `v5` — `$a[$j] = $j` (index, AST) | 0 / 10 / 30 / 60 | 155,2 / 457,4 / 1228,6 / 2332,9 | 77,6 / 229,0 / 611,7 / 1164,3 | **7,92 / 7,88** |
| `v7` — `joindre($l,$j)` (appel, AST) | 0 / 30 | 145,3 / 1203,8 | 73,3 / 598,1 | **8,29 / 8,16** |
| `v8` — `$s = $s + $a[$q]` (index lecture) | 0 / 30 | 419,6 / 2566,4 | 210,0 / 1281,7 | **6,12 / 6,11** |

**Constat [M]** :

- la boucle arithmétique **abstraite** est insensible au nombre de variables :
  +0,9 % / +2,7 % pour 30 variables supplémentaires ;
- toutes les opérations passant par le repli AST sont **~8× plus lentes** avec 30 variables
  inutiles, et **~15×** avec 60 ;
- la dérive absolue entre salves est de ~2× sur **toutes** les lignes (état machine :
  compilations concurrentes), mais les rapports sont stables à 1–2 % près.

Marginalité mesurée (salve 2) : `(1164,3 − 77,6) / 60 = 18,1 ms` pour 100 000
affectations indexées, soit **0,181 µs par variable supplémentaire par opération**.
Forme empirique : `coût ≈ coût_de_base × (1 + 0,23 × V)` (identique dans les deux salves :
`36,3/155,2 = 0,234` et `18,1/77,6 = 0,233`). Autre lecture, plus directe : avec
30 variables, **87,3 % du temps mesuré est la part attribuable au clonage de trame**
(`(611,7 − 77,6)/611,7` ; salve 1 : `(1228,6 − 155,2)/1228,6 = 87,4 %`).

**Deux outliers honnêtes** (salve 1, round 9) : `v6_add_slots_30` = 2,431 ms au lieu de
~4,43 ms et `v7_joindre_slots_30` = 731 ms au lieu de ~1204 ms. Ils n'ont pas été retirés
du JSON brut ; les médianes sur 9 échantillons en sont affectées de façon négligeable et la
salve 2 les reproduit sans.

### 3.4 Preuve mécanique : le modèle Rust du clonage de trame

`/tmp/opencode/goulot/trame_modele.rs` reproduit les trois opérations de
`interp.rs:1811-1817` (`HashMap::clone` + `clear` + `insert(n.clone(), v.clone())`) sur un
enum `Valeur` de même forme, `rustc -O`, `taskset -c 15`, 3 salves entrelacées, 100 000 ops :

| variables | ns/opération | ns / variable / opération |
|---:|---:|---:|
| 1 | 75,0 | 75,0 |
| 8 | 423,0 | 52,9 |
| 16 | 1073,0 | 67,1 |
| 32 | 2149,6 | 67,2 |
| 64 | 4256,9 | 66,5 |

**[E] + [M-modèle]** : ~67 ns par variable par opération dans le modèle nu, contre
~180 ns mesurés dans Rose (salve 2) — même ordre de grandeur, le facteur ~2,7 venant du
travail d'interprétation supplémentaire. Le mécanisme identifié dans le code suffit à
expliquer l'essentiel du coût observé.

### 3.5 Élimination des autres candidats (mesures, pas suppositions)

| Candidat | Preuve | Verdict |
|---|---|---|
| (c) parse / vérif | 0,080 ms sur 200,4 ms = **0,040 %** ; `rose latex` (analyse seule) 217 µs ≈ lancement 215 µs | **exclu** |
| (e) ABI des appels JIT | `TIMING-JIT eligibles=[]` sur `mix` ; A/A/A sur `r_loop.rose` : JIT 27,78 ms, interpréteur 30,19 ms, natif 6,47 ms → JIT/interpréteur **1,09×** seulement | **non dominant** |
| (a) conversion JIT↔interpréteur | 100 000 appels à `double($i)` : 102,6 ms (défaut) / 94,9 ms (`ROSE_SANS_JIT=1`) / 102,0 ms (natif) ; `PROFIL-JIT appels=0` — le chemin coûteux est l'appel **AST**, pas une frontière JIT | **non dominant ici** |
| (d) boxage chaîne/liste isolé | `v4a_liste_seule` (100 k `Valeur` boxées) = **237,9 ms** vs `v4b_tableau_seul` (`Vec<i64>` non boxé) = **245,6 ms** : le tableau typé est même **plus lent** (bruit/dispatch) | **exclu comme facteur discriminant** |
| (b) allocation/clonage des `Valeur` | C'est le **mécanisme** du goulot #1 (l.1815/l.2689), mais il ne devient destructeur que parce que le clonage porte sur **toutes les variables** à chaque opération | **cause, pas diagnostic terminal** |

À noter : `vers_z` (`run.rs:333-352`) alloue bien un `Gz::neuf()` (`i128x.rs:461`) à chaque
conversion, et `Drop for Gz` fait `Box::from_raw` (`i128x.rs:585-586`). Ce coût existe, mais
la voie rapide i64 de `binaire_valeurs` (`interp.rs:2801-2832`) et de `valeur_egale`
(`run.rs:491-498`) l'évite déjà sur les opérations arithmétiques et les clés entières — ce
qui est précisément pourquoi la boucle abstraite reste insensible au nombre de variables.

---

## 4. Comparaison avec l'architecture Rust

| Propriété | Rust (`rustc -O`) | Rose aujourd'hui | Effet mesuré |
|---|---|---|---|
| Types | statiques, résolus à la compilation | `enum Valeur` à 12 variantes (`run.rs:38-52`), tag testé à l'exécution | chaque opérande est déballé/requalifié |
| Passage d'un objet | `move`/`&` — zéro copie | `Rc<RefCell<…>>` (`run.rs:29-36`) + `clone()` | clonage systématique |
| Environnement | registres / pile de taille fixe connue | `HashMap<String, Valeur>` reconstruit par opération non-abstractable | **0,181 µs par variable et par op** |
| Collections | `Vec<T>` contigu, `HashMap` à hachage direct | `Rc<RefCell<Vec<Valeur>>>` et `Rc<RefCell<Vec<(Valeur,Valeur)>>>` avec **scan linéaire** (`interp.rs:3488-3507`) | table 3 000 clés : ×1946 vs HashMap (chiffre de contexte, non re-mesuré ici) |
| `&str` | tranche de bytes, `len()` O(1), slicing O(1) | `Rc<String>` + `chars().collect()` **à chaque accès** (`interp.rs:1917, 3473, 3553`) | 200 extractions sur 1 Mo : 114,2 ms (57 % de `mix`) |
| Code | LLVM AOT, une spécialisation par type, tout inlinable | interpréteur + JIT partiel (entiers seulement), avec un chemin de repli général | JIT/natif 4,3× ; interpréteur/natif 4,7× sur `r_loop` |

**Ce que Rust ne fait jamais** : reconstruire l'environnement entier pour exécuter une
seule instruction. Chez Rust, ajouter une variable locale non utilisée au programme **ne
coûte rien** à l'exécution. Chez Rose, cela coûte **+23 % par opération non-abstractable**
(mesuré, reproductible sur deux salves). Voilà la différence architecturale, avant même de
parler de boxage, de hachage ou de registres.

### Décision structurante proposée (UNE seule)

> **Supprimer la matérialisation de la trame du chemin d'exécution : faire de la machine à
> slots le protocole unique, et abaisser TOUTE opération (appels de primitives, `Index`,
> `AffectIndex`, `Afficher`) en opcode abstrait à opérandes slots — exactement comme
> `Op::Affect` / `Op::SautSiFauxAbstrait` le font déjà. Le `HashMap<String, Valeur>` ne
> doit plus jamais être construit dans une boucle, ni `eval_avec_slots_fallback`
> (`interp.rs:1811-1817`), ni `affecter_index_ast` (`interp.rs:2686-2690`).**

Ce n'est **pas** une liste de micro-optimisations : c'est une décision sur le **contrat
d'interface entre l'ordonnanceur RIR et les opérations**. Elle ne demande pas de changer
`Valeur`, `Gz`, le JIT, ni l'ordre d'évaluation : elle remplace un passage d'environnement
par un passage d'opérandes, comme le ferait un compilateur Rust (les valeurs sont des
arguments de fonction, pas un contexte global reconstruit).

**Porte mesurable (à passer avant toute autre optimisation)** : sur les trois paires de
programmes `v5_slots_0/30`, `v7_joindre_slots_0/30`, `v8_indexread_slots_0/30`, le rapport
`exec(V=30) / exec(V=0)` doit passer de **7,9 / 8,2 / 6,1** à **≤ 1,1** (9 échantillons
pinés, hash contrôlé, résultat identique) ; et le coût par `joindre` doit tomber sous
~100 ns/op contre 730–1 450 ns/op aujourd'hui. **Non mesuré : cible, pas résultat.**

---

## 5. Séquence ordonnée (goulot #1 → #2 → #3), dans l'ordre du PLAN

L'ordre suit le PLAN : **M2 perf**, **M3 backend**, **M5 runtime**. Chaque porte est
mesurable ; `[M]` = prouvé par les salves ci-dessus, `[C]` = chiffre de contexte daté non
re-mesuré ici, `[E]` = cible calculée, non atteinte.

### Étape 1 — M2.2 : protocole de trame unique (goulot #1)

- **Fait [M]** : 0,04 % du temps dans le parse, 99,91 % dans `exec` ; coût
  O(ops × variables) mesuré (×8 avec 30 variables inutiles) ; code à
  `interp.rs:1811-1817` et `interp.rs:2686-2690`.
- **Décision** : opcodes RIR abstrait pour `Index`/`AffectIndex`/`Appel`/`Afficher` ;
  suppression du `HashMap` du chemin chaud.
- **Porte [E]** : ratios V=30/V=0 ≤ 1,1 sur `v5`, `v7`, `v8` ; `mix.rose` doit perdre
  le résidu de 15,0 ms et au moins 50 % de son temps de table/liste.
- **Risque honnête** : le chemin AST reste nécessaire pour la sémantique exacte des cas
  rares ; la porte doit être tenue **sans** régression des oracles (`tokens/ast/verif/rir`
  et `jardins/tests/*`), sinon le gain n'est pas acceptable.

### Étape 2 — M3 (backend) : représentation des chaînes et opérations typées (goulot #2)

- **Fait [M]** : l'extraction de 200 mots sur 1 Mo coûte 114,2 ms (57 % de `mix`) ;
  la **même** boucle sur 1 Ko coûte 0,667 ms (`v2b_extract_court`, 9 échantillons).
  Cause au code : `sous_chaine` fait `s.chars().collect()` de **tout** le texte à chaque
  appel (`interp.rs:3553`), l'indexation de chaîne idem (`interp.rs:1917`), `longueur`
  fait `chars().count()` (`interp.rs:3473`).
- **Décision structurante** : une seule représentation de chaîne avec longueur O(1) et
  tranchage O(longueur extraite) (pointeur + longueur en octets, validation UTF-8 une
  seule fois à l'entrée) ; rendre ces opérations abstractables/JIT-compatibles.
- **Porte [E]** : 200 extractions sur 1 Mo **< 2 ms** (contre 115 ms), et le rapport
  `extract(1 Mo)/extract(1 Ko)` doit tomber de ~172× à ~1–5× (il doit dépendre de ce
  qu'on extrait, pas de la taille du fichier).

### Étape 3 — M5.2/M5.3 (runtime) : collections natives et E/S sans copie (goulot #3)

- **Fait [M]** : table = `Vec<(Valeur,Valeur)>` avec `cle_existe`/`inserer` en scan
  linéaire (`run.rs:32`, `interp.rs:3488-3507`) ; `lire_texte` valide et recopie tout le
  texte (`String::from_utf8_lossy`, voir `BENCH_MULTI.md §8` [C]) ; lecture+`longueur`
  de 1 Mo = 0,85 ms ici, soit déjà faible comparé aux deux étapes précédentes.
- **Décision structurante** : une table de hachage native au runtime avec clés typées,
  et des primitives E/S sur octets sans recopie ni validation UTF-8 systématique.
- **Porte [E]** : 3 000 insertions + 3 000 recherches doivent passer de ~188 ms [C] à
  < 10 ms ; E/S 10 Mo de ~14 ms [C] à < 6 ms (niveau Rust mesuré dans `BENCH_MULTI.md`).
  **Ces deux chiffres de départ sont [C] : ils doivent être re-mesurés sur le binaire
  d'alors avant d'être annoncés comme gagnés.**

### Ce qui est PROUVÉ / ESTIMÉ

- **PROUVÉ [M]** : phases (tableau §2), goulot #1 et sa dépendance linéaire au nombre de
  variables (§3.3), mécanisme de code (§3.2), ordre de grandeur du modèle (§3.4),
  élimination de (c), (d), (e) (§3.5).
- **ESTIMÉ [E]** : toutes les portes chiffrées, la suppression effective du repli AST sans
  régression sémantique, et le gain final sur `mix.rose`. Aucun chiffre de porte n'est
  présenté comme obtenu.

---

## 6. Reproduction

```bash
cd /tmp/opencode/goulot
python3 mesure_phases.py salve1          # 5 programmes × 9 échantillons entrelacés, phases
python3 mesure_phases.py salve2          # confirmation
python3 mesure_trame.py s1               # 10 programmes × 9, dépendance au nombre de variables
python3 mesure_trame.py s2               # confirmation
taskset -c 15 /home/clausake/Stefan/ROSE/rose/rose debug-lenteur -n 9 mix.rose
rustc -O -o trame_modele trame_modele.rs && taskset -c 15 ./trame_modele
sha256sum /home/clausake/Stefan/ROSE/rose/rose   # 7ba896796549… pour TOUS ces chiffres
```

Données brutes : `phases_salve1.json`, `phases_salve2.json`, `phases_salve1.log`,
`phases_salve2.log`, `trame_s1.json`, `trame_s2.json`, `trame_s1.log`,
`debug_lenteur_mix.txt`, `mix.rose`, `v*.rose`, `in_1m.txt`, `in_1k.txt`,
`mesure_phases.py`, `mesure_trame.py`, `trame_modele.rs`.

---

## 7. Verdict final

**Goulot structurel #1 identifié et prouvé** : la matérialisation complète de la trame
(`HashMap<String, Valeur>`) sur le chemin de repli RIR→AST, à chaque opération non-abstractable
— **0,181 µs par variable en portée et par opération**, soit **×8 de ralentissement pour
30 variables non utilisées** et **87,3–87,4 % du temps attribuable au clonage de trame**,
reproductible sur deux salves de 9 échantillons pinées avec un hash de binaire constant.
Ce mécanisme siège dans l'exécution, qui représente **99,9072 %** du temps du programme
représentatif.

**Ce n'est pas le parseur** (0,040 %), **pas le JIT** (0,026 %), **pas le boxage isolé**
(tableau typé aussi lent que liste boxée), **pas l'ABI JIT** (aucun appel JIT dans le
programme représentatif).

**Décision structurante** : une seule — rendre la machine à slots le protocole unique et
abaisser toutes les opérations en opcodes à opérandes slots, en supprimant le `HashMap` du
chemin chaud. Puis, seulement ensuite, la représentation O(1) des chaînes (M3) et les
collections/E/S natives (M5). Aucune date promise, aucun gain annoncé comme acquis : les
portes ci-dessus sont des cibles à mesurer.
