# J6 — Port du moteur de valeurs en Rose (constat mesuré, 23/11/2026)

Sous-agent J6. Worktree exclusif `/tmp/opencode/opt_vm`, HEAD de départ
`62ab54c` (`v0.1.76 : algorithmes supérieurs et self-hosting étendu`).
Trois fichiers créés, **aucun autre touché**, aucun commit, aucune publication :

| Fichier | Rôle | Lignes |
|---|---|---|
| `compilateur/moteur_valeurs.rose` | représentation taguée + constructeurs/accesseurs | 225 |
| `compilateur/moteur_ops.rose` | 20 opérations + repli bigint décimal | 650 |
| `rose/docs/J6_MOTEUR_ROSE.md` | ce document | — |

**Résumé exécutif** : 20 opérations sur 20 écrites en Rose, compilées par
C0 en ELF aarch64 et exécutées **sans Rust**, avec sortie octet pour octet
identique à l'interpréteur Rose sur 42 lignes de tests (35 + 7), dont
`fib(10) = 55`, `2^70`, `2^100` et les divisions/modulos signés. 875 lignes
de Rose contre 74 771 lignes du moteur Rust mesuré : 100 % du périmètre
opérationnel J6, ~1,17 % du moteur complet.

Garde-fou utilisé sur toute commande longue :
`bash scripts/garde_fou_mem.sh 12000000 <commande>`.

Interdits non touchés (vérifié par `git status --short` en fin de mission) :
`rose/src/**`, `compilateur/emetteur.rose`, `emetteur_elf.rose`,
`rose_natif.rose`, `scripts/generer_rose_natif.py`.

---

## 1. Inventaire précis (lecture seule)

### 1.1 Structures de valeurs du moteur Rust

Mesuré par `grep -n` sur les sources (aucune modification).

| Structure | Ancrage | Rôle | Complexité à porter |
|---|---|---|---|
| `Valeur` (13 variantes) | `rose/src/run.rs:798` | `Vide`, `Entier(i64, Option<Rc<Gz>>)`, `Rationnel(Rc<Gq>)`, `Flottant(f64)`, `Booleen(bool)`, `Chaine(Rc<ChaineRep>)`, `Liste(Liste)`, `Table(Table)`, `Tampon`, `Tableau`, `Complexe{re,im}`, `Irrationnel(Rc<Gz>)`, `Symbolique(Rc<ObjetParesseux>)` | **centrale** — enum à 13 bras, dont 3 pointeurs partagés |
| `Gz(pub *mut Mpz)` | `rose/src/i128x.rs:525` | entier exact, pointeur nu vers `Mpz` | `Mpz{v:BigInt}` (:15) —ABLE BigInt Rust, pas de GMP |
| `Gq(pub *mut Mpq)` | `rose/src/i128x.rs:773` | rationnel exact `num/den` | dépend de `Gz` |
| `Liste(Rc<RefCell<Vec<Valeur>>>, Rc<RefCell<CompactListe>>)` | `rose/src/run.rs:109` | canon + miroir compact | `CompactListe{Opac,I64}` (:100), 8 méthodes (:114-224), pool d'arènes (:226-249) |
| `Table(pub Rc<RefCell<TableRep>>)` | `rose/src/run.rs:294` | table associative, `TableRep` en :3400 | hash/tri maison |
| `ChaineRep{n, octets, noeud}` | `rose/src/run.rs:416` | rope (arbre de morceaux), `n` = NB DE CARACTÈRES | 6 variantes de `NoeudChaine` (:311) : `Plein`, `PleinCourt`, `Concat`+cache, `FichierMmap`, `PleinIndexe`, `FichierMmapIndexe` |
| `Tampon`, `Tableau` | `run.rs:296`, `:298` | octets / cases i64 | secondaires |
| `ObjetParesseux`, `CacheParesseux` | `run.rs:845`, `:834` | entiers géants NON matérialisés (n! avec n = 10^37) | hors périmètre J6 |
| `Interp` | `rose/src/interp.rs:29` | état d'exécution complet | hors périmètre J6 |

Volume mesuré : `find rose/src -name '*.rs' | xargs wc -l` → **74 771 lignes**
(dont `interp.rs` 9 138, `run.rs` 3 618, `i128x.rs` 2 079). L'énoncé
annonçait 72 606 : l'ordre de grandeur est confirmé, le chiffre exact
mesuré ici est 74 771.

### 1.2 Opérations indispensables pour exécuter un Rose

Ancrages dans le moteur Rust (règles exactes, pas des approximations) :

| Domaine | Opérations | Ancrage |
|---|---|---|
| Arithmétique exacte | `+ - * / ÷ % ^`, promotion i64→bigint, rationnels, complexes, irrationnels | `interp.rs:4785` `binaire_valeurs`, `:4747` `unaire_valeur`, `i128x.rs` `gmp_*` |
| Comparaisons | `== != < <= > >=`, `valeur_egale` | `run.rs:1494`, `interp.rs:4785` |
| Chaînes | `longueur` (caractères), concat O(1) par rope, `idx`, `sous_chaine`, `ordre`, `caractere` | `run.rs:311-447`, `interp.rs:6957` `indexer_valeur` |
| Listes | `joindre` (mute + miroir), `longueur` O(1), `idx`, voie compacte i64 | `run.rs:114-224`, `interp.rs:6934`, `:6943` |
| Tables | création, `inserer`, `cle_existe`, lecture/affectation par clé, `cles`, `valeurs`, `supprimer` | `run.rs:294`/`:3400`, `interp.rs:6957`, `:7014` |
| Affichage | `afficher`, `chaine_de`, `afficher_formule`, décimal exact | `run.rs:1594`, `:1686`, `:1705` |
| Divers JIT/Runtime | appel de fonction, slots, globaux, REPL, CLI, GC/Rc | `interp.rs` (9 138 l.), `jit.rs`, `repl.rs`, `main.rs` |

### 1.3 Ce que couvre déjà le runtime G5 émis

L'émetteur `compilateur/emetteur_elf.rose` génère de vraies routines machine
pour un sous-ensemble. Relevé dans le dispatch `g5_op` (:2013-2047) et
confirmé par `rose/docs/G5_ABI_VALEURS.md`, `G5_PRIMITIVES.md`, `G5_C0.md` :

**ABI unifiée (déjà en place)** : `entier = n<<3` (bits bas `000`), booléen =
entier tagué, chaîne = `ptr|1` vers `[tag][len][octets]`, liste = `ptr|2`
vers `[tag][len][cap][valeurs]` (`G5_ABI_VALEURS.md` §2).

| Primitives G5 émises | Ancrage | État |
|---|---|---|
| `+ - * / % ^`, `neg`, `non`, comparaisons, `et`, `ou` | `g5_op` / `g5_eval` | ✅ entiers 64 bits (débordement **non** exact) |
| `+` sur chaînes = concaténation de contenu, `==`/`!=` sur chaînes = comparaison de contenu | ABI §2 | ✅ |
| `longueur`, `ordre`, `sous_chaine`, `chaine`, `idx` | :2013-2016 | ✅ |
| `joindre` (listes), `inserer`, `cle_existe`, `table`, `supprimer` | :2017-2018, :2029, :2046-2047 | ✅ tables à clés **et valeurs** (correctif SH12) |
| `caractere`, `partie_entiere`, `quitter`, `abs`, `min`, `max`, `type` | :2031-2041 | ✅ |
| `lire_texte`, `ecrire_texte`/`ecrire_bytes`, `argument`, `nb_arguments` | :2021-2024 | ✅ |
| Affichage décimal, chaînes **et listes** | `g5_emit_print`, `g5_emit_plist` | ✅ |
| Grands entiers `> 2^60` (immédiat tagué) / `> 2^64` (mot machine) | — | ❌ repli à 0 (`G5_PRIMITIVES.md` §7.1) |
| Rationnels, complexes, irrationnels, rope UTF-8, `concatener`, `cles` | — | ❌ absents / stubs |
| Variables globales | — | ❌ repliées à 0 (`G5_C0.md` §6) |
| JIT Cranelift, GC Rust, CLI, REPL | — | ❌ absents du binaire G5 |

**Conclusion de l'inventaire** : la couche **valeurs** est le bon premier
chantier — l'ABI G5fournit déjà les primitives mécaniques, mais elle triche
sur l'exactitude au-delà de 2^60 et sur les types non numériques. C'est
précisément ce trou que J6 comble, en Rose, au-dessus de l'ABI.

---

## 2. Ce qui est réellement porté

### 2.1 `moteur_valeurs.rose` — représentation

Une valeur Rose est une `liste<entier>` **plate, homogène** (contrainte du
vérificateur et de G5), dont l'index 0 est le tag :

| Tag | Sens | Forme |
|---|---|---|
| 0 | VIDE | `[0]` |
| 1 | ENTIER | `[1, n]` |
| 2 | BOOLEEN | `[2, 0\|1]` |
| 3 | CHAINE | `[3, c0, c1, …]` (codes de caractères) |
| 4 | LISTE | `[4, e0, e1, …]` (éléments entiers) |
| 5 | TABLE | `[5, c1, v1, c2, v2, …]` (clés = 1 caractère ASCII) |
| 6 | BIGZ | `[6, signe, cnt, d0…d(cnt-1)]`, chiffres **décimaux**, poids croissant |
| 7 | ERREUR | `[7, code]` |

Constructeurs : `v_vide`, `v_ent`, `v_boo`, `v_ch`, `v_liste`, `v_table`,
`v_bigz`, `v_err`.
Accesseurs : `v_tag`, `v_est`, `v_ent_val`, `v_boo_val`, `v_longueur`,
`v_ch_code`, `v_ch_car`, `v_liste_idx`, `v_big_signe`, `v_big_cnt`,
`v_big_chiffre`, `v_tab_cle`, `v_tab_val`.
Conversions exactes : `bz_depuis_entier`, `bz_vers_entier`, `bz_texte`.

### 2.2 `moteur_ops.rose` — 20 opérations

| # | Famille | Opération | Repli exact |
|---|---|---|---|
| 1 | entiers | `op_add` | seuil 1e9 → `bz_add` (retenues décimales) |
| 2 | entiers | `op_sub` | `bz_neg` + `op_add` |
| 3 | entiers | `op_mul` | seuil 1e9 → `bz_mul` (méthode russe) |
| 4 | entiers | `op_div` | quotient bigz par division longue sur chiffres |
| 5 | entiers | `op_mod` | reste de la division longue |
| 6 | entiers | `op_neg` | `bz_neg` si hors seuil |
| 7 | entiers | `op_cmp` | comparaison signée sur chiffres |
| 8 | entiers | `op_eq` | `op_cmp == 0` |
| 9 | entiers | `op_pow` | élévation au carré, repli dans `op_mul` |
| 10 | chaînes | `op_ch_longueur` | codes |
| 11 | chaînes | `op_ch_joint` | concaténation par codes |
| 12 | chaînes | `op_ch_idx` | indexing par codes |
| 13 | chaînes | `op_ch_contient` | recherche de sous-séquence |
| 14 | listes | `op_lst_longueur` | — |
| 15 | listes | `op_lst_joindre` | reconstruction (pas de mutation de paramètre) |
| 16 | listes | `op_lst_idx` | — |
| 17 | tables | `op_tab_inserer` | clé = 1 caractère, valeur entière |
| 18 | tables | `op_tab_cle_existe` | — |
| 19 | tables | `op_tab_idx` | — |
| 20 | affichage | `op_afficher` | rendu des 8 tags, dont `bz_texte` |

**Décisions de conception (toutes mesurées / nécessaires)** :
- **Seuils explicites (1e9)** plutôt qu'une détection de débordement
  machine : le même seuil est évalué par l'interpréteur Rust (où il est
  toujours faux pour les petits nombres) et par l'ELF G5 (où il garantit
  qu'aucun `n<<3` ne dépasse 2^60). Les deux moteurs prennent *le même
  chemin* — c'est la condition de parité.
- **Convention de signe bigz** : `0` = positif ou zéro, `-1` = négatif
  (pas de `+1`). Deux bogues de signe détectés et corrigés pendant la
  validation (`bz_neg`, `bz_mul`).
- **Division longue de droite à gauche avec renversement** : la retenue
  de `÷2` va du chiffre haut vers le bas (`10^i/2 = 5·10^(i-1)`). Deux
  bogues détectés et corrigés (`bz_div2`, `bz_div_small`).
- **Aucune globale, aucun `[]`, aucun `retour [ … ]`, aucune affectation
  par index sur une liste, aucun paramètre nommé `i`** (l'imaginaire).
  `moteur_ops.rose` porte une réplique locale des accesseurs sous des noms
  préfixés `o_*` afin que `rose verif` réussisse sur **chaque fichier
  séparément** — Rose n'a pas de module/`include`.

---

## 3. Preuves d'exécution — SANS Rust

### 3.1 Le compilateur C0 utilisé est bien celui de ce worktree

```
cd /tmp/opencode/opt_vm
./rose/rose rir compilateur/rose_natif.rose > rose_natif.rir
   686 314 octets, md5 8fca1e723014b6ad377be61c17b74ab1
./rose/rose compilateur/emetteur_elf.rose rose_natif.rir > C0_j6.elf
   1 603 744 octets, md5 7bb091200cf927022eb9d86a548d2502, rc=0
```
`C0_j6.elf` est un **ELF aarch64 statique** (`file` : `ELF 64-bit LSB
executable, ARM aarch64, statically linked, no section header`). Son md5
est identique à celui du `C0_ext.elf` préexistant utilisé lors des premiers
essais : provenance confirmée. **Toute la compilation ci-dessous est faite
en exécutant cet ELF — aucun appel à `rose/rose`, aucun cargo.**

### 3.2 Compilation de CHAQUE fichier par C0 (séparément)

```
./C0_j6.elf compilateur/moteur_valeurs.rose > mv.elf
   rc=0, 20 536 o, magic 7f454c46 (ELF aarch64), exécution rc=0 sans sortie
./C0_j6.elf compilateur/moteur_ops.rose > mo.elf
   rc=0, 62 796 o, magic 7f454c46 (ELF aarch64), exécution rc=0 sans sortie
```
Les deux modules sont donc chacun compilables sans Rust. Rose n'ayant pas
d'include, le programme de test est leur concaténation plus un pilote.

### 3.3 Chaîne de validation

```
cat compilateur/moteur_valeurs.rose compilateur/moteur_ops.rose \
    /tmp/opencode/j6_agents/drv_j6.rose > prog_j6.rose
rose/rose verif prog_j6.rose                      -> vérifié (rc=0)

ROSE_SANS_JIT=1 rose/rose prog_j6.rose > nat.out   (référence, 35 lignes, rc=0)
./C0_j6.elf prog_j6.rose > prog_j6.elf             (ELF 91 160 o, rc=0)
./prog_j6.elf > elf.out                            (rc=0)
diff nat.out elf.out                              -> AUCUNE DIFFÉRENCE (octet pour octet, 35 lignes)

# Second pilote : division/modulo signés (sémantique FLOOR du `div` natif)
ROSE_SANS_JIT=1 rose/rose prog_neg.rose > neg.nat.out
./C0_j6.elf prog_neg.rose > prog_neg.elf && ./prog_neg.elf > neg.elf.out
diff neg.nat.out neg.elf.out                      -> AUCUNE DIFFÉRENCE (7 lignes)
```

### 3.4 Résultats extraits (identiques des deux côtés)

```
fib10=55                              add=42        sub=42        mul=56088
div=123                               mod=4         neg=7         pow=1024
cmp=-1                                eq=vrai
big_pow=1180591620717411303424        (= 2^70, au-delà de 2^64)
big_add=1180591620717411303425        big_mul=3541774862152233910272
big_div=1152921504606846976           (= 2^60)
big_cmp=-1
ch_len=3    ch_joint=abcDEF   ch_idx=b   ch_contient=vrai   ch_cmp=-1
lst_len=3   lst_idx=20        lst=[10, 20, 30]
tab_existe=vrai   tab_idx=99   tab_absent=faux   tab={a:99, b:20}
negbig_mul=-4000000000000000000       big_sub=1180591620717411303423
big_mod=24  big_addneg=1180591620717411303421
big_neg=-1180591620717411303424       big_cmp_neg=-1
p100=1267650600228229401496703205376  (= 2^100)
tab_big=0                             (sentinelle déterministe, cf. §5)
```
Le second pilote couvre `-1234 div 10 = -124`, `1234 div -10 = -124`,
`-1234 div -10 = 123`, `-7 % 3 = 2`, `7 % -3 = -2`, `-7 % -3 = -1` :
`op_div`/`op_mod` reproduisent exactement la sémantique floor / reste signé
du moteur Rust (deux bogues détectés puis corrigés pendant la validation).

`fib(10)` est calculé par une fonction Rose récursive utilisant
`op_add(v_ent(k))` : le résultat `55` sort de l'ELF produit par C0, sans
aucun moteur Rust. `2^70` et `2^100` prouvent que le repli décimal
remplace effectivement l'arithmétique 64 bits débordante de l'ABI G5.

### 3.5 Contrôles de non-régression

| Contrôle | Résultat mesuré |
|---|---|
| `rose verif compilateur/moteur_valeurs.rose` | `vérifié` (rc=0) |
| `rose verif compilateur/moteur_ops.rose` | `vérifié` (rc=0) |
| `rose verif` sur la concaténation des deux + pilote | `vérifié` (rc=0) |
| `rose verif compilateur/emetteur_elf.rose` | `vérifié` (rc=0) |
| Point fixe `./rose/rose rir compilateur/emetteur.rose \| md5sum` | `b3ed9262dd54776a6dbd07bc12368d0e` — **inchangé** |
| `RUST_TEST_THREADS=1 bash scripts/validation_finale.sh` (garde-fou) | **RC=0, 0 échec** : 641 tests Rust (314+327+0) verts, point fixe, AOT 75/75, JIT 25/25, WASM, RVM 317811, AZ 31/31, HOST-1 fib=317811 |

---

## 4. Taux de couverture — chiffres exacts

| Mesure | Valeur |
|---|---|
| Opérations ciblées par la mission (arithmétique, comparaison, chaînes, listes, tables, affichage) | 20 |
| Opérations **écrites en Rose, compilées par C0 et exécutées sans Rust** | **20 / 20 (100 %)** |
| Lignes de ces opérations + accesseurs (Rose) | 875 |
| Lignes du moteur Rust mesuré (`find rose/src -name '*.rs'`) | 74 771 |
| Couverture **du moteur entier** en lignes de source | **~1,17 %** (875 / 74 771) |
| Types `Valeur` Rust modélisés | 6 / 13 (Entier, Booleen, Chaine, Liste, Table, Vide) + 1 extension (BIGZ) |
| Types `Valeur` Rust NON modélisés | 7 : Rationnel, Flottant, Complexe, Irrationnel, Tampon, Tableau, Symbolique (l'`Entier` natif est approximé par le tag BIGZ) |

**Lecture honnête** : 100 % du *périmètre opérationnel fixé pour J6* est
atteint et prouvé ; ~1 % du *moteur Rust complet* est porté. Ce jalon est
une **fondation exécutable**, pas le remplacement de `cargo`.

---

## 5. Reste exact (ce qui manque encore)

| # | Manquant | Pourquoi ce n'est pas fait | Effort (estimation de planification, non mesurée) |
|---|---|---|---|
| 1 | **JIT Cranelift / AOT LLVM** | Le binaire G5 n'a pas de JIT ; `op_*` sont des fonctions compilées en machine par C0, pas un compilateur à la volée | 3-6 mois ( équivalent module `jit.rs`, ~7 000 l. Rust) |
| 2 | **GC / gestion mémoire** | ABI G5 = allocateur bump **sans libération** (`G5_ABI_VALEURS.md` §5). Il faut un GC traçable pour les listes/chaînes longues | 1-3 mois |
| 3 | **CLI (`rose run`, `comp`, `rir`, `latex`, `info`, drapeaux)** | Le binaire G5 n'exécute qu'un seul fichier ; aucune gestion d'arguments complète (`argument` seul existe) | 2-4 semaines |
| 4 | **REPL / console** | `repl.rs` (édition de ligne, historique, couleurs, `:help`…) est 100 % Rust | 1-2 mois |
| 5 | **Parser / lexeur / vérificateur intégrés au moteur** | C0 compile depuis le RIR produit par l'émetteur Rose ; le moteur J6 consomme des valeurs, il ne lit pas encore une source Rose | 2-3 mois (réutiliser `compilateur/lexeur.rose`, `analyseur.rose`, `verifieur.rose`) |
| 6 | **Rationnels, flottants, complexes, irrationnels** | 7 variantes `Valeur` non modélisées ; G5 n'a pas de flottants | 2-4 mois |
| 7 | **Variables globales G5** | 386 globales repliées à 0 (`G5_C0.md` §6) — bloque l'auto-exécution de l'émetteur | 2-4 semaines |
| 8 | **Bigint performant (NTT, FFT)** | Le repli décimal est **exact mais O(d²·log)** : 2^100 passe, 100 000! est hors de portée | 3-6 mois |
| 9 | **Unicode réel (rope, index de caractères, mmap)** | Chaînes J6 = codes ASCII ; `ChaineRep` a 6 variantes dont 4 non portées | 1-2 mois |
| 10 | **Tables comme valeurs complètes** | clés = 1 caractère, valeurs = entiers ≤ 1e9 ; un bigz stocké rend la sentinelle `0` (déterministe sous Rust et G5, voir `tab_big=0`) | 2-4 semaines |
| 11 | **`eval_expr`, FFI, threads, erreurs runtime, symbolique** | hors ABI G5 | 2-4 mois |

Total indicatif pour un remplacement complet de `rose/src` : **~18 à 36
mois-homme**, avec un ordre de grandeur de 2 à 4 ans sur un seul agent. Ce
n'est PAS une mesure : c'est une estimation de planification à re-évaluer
après chaque jalon.

---

## 6. Jalons intermédiaires proposés

| Jalon | Critère vérifiable (commande réelle) |
|---|---|
| **J6.1 — valeurs étendues** | Modéliser `Flottant` (tag 8) et `Rationnel` (tag 9, num/den bigz) ; `rose verif` + parité sur 10 programmes |
| **J6.2 — bigint rapide** | Remplacer la méthode russe par une multiplication base 10^9 + Karatsuba ; mesurer 1 000! natif vs ELF |
| **J6.3 — chaînes Unicode** | `v_ch` sur `sous_chaine` UTF-8 + index de caractères ; parité sur le jardin Unicode (31 programmes AZ) |
| **J6.4 — tables générales** | clés de longueur quelconque + valeurs bigz encodées ; parité sur les 4 divergences tables documentées (`G5_PRIMITIVES.md` §7.2) |
| **J6.5 — lecteur de source** | Le moteur J6 exécute un RIR produit par `compilateur/emetteur.rose` (`argument`/`lire_texte` déjà émis) |
| **J6.6 — intégration CLI** | Un ELF C0 remplace `rose run prog.rose` sur la batterie 43 programmes (`G5_PRIMITIVES.md` §6), 0 divergence |
| **J6.7 — retrait de cargo** | `scripts/build-sans-rust.sh --compilateur C0 ...` sur tout le corpus, puis suppression de `rose/src` — **jamais avant J6.6 + validation_finale verte** |

---

## 7. Reproductibilité

Tous les artefacts d'essai sont hors dépôt, dans
`/tmp/opencode/j6_agents/` : `C0_j6.elf`, `prog_j6.rose`, `prog_j6.elf`,
`prog_j6.nat.out`, `prog_j6.elf.out`, `drv_j6.rose`, `build_C0.log`.

```
cd /tmp/opencode/opt_vm
# 1. vérification des deux modules
rose/rose verif compilateur/moteur_valeurs.rose
rose/rose verif compilateur/moteur_ops.rose
# 2. construction de C0 depuis CE worktree (≈ 6 min, garde-fou)
bash scripts/garde_fou_mem.sh 12000000 bash -c '
  ./rose/rose rir compilateur/rose_natif.rose > /tmp/opencode/j6_agents/rose_natif.rir
  ./rose/rose compilateur/emetteur_elf.rose /tmp/opencode/j6_agents/rose_natif.rir \
     > /tmp/opencode/j6_agents/C0_j6.elf'
# 3. compilation SANS Rust et comparaison
cat compilateur/moteur_valeurs.rose compilateur/moteur_ops.rose \
    /tmp/opencode/j6_agents/drv_j6.rose > /tmp/opencode/j6_agents/prog_j6.rose
/tmp/opencode/j6_agents/C0_j6.elf /tmp/opencode/j6_agents/prog_j6.rose \
    > /tmp/opencode/j6_agents/prog_j6.elf
chmod +x /tmp/opencode/j6_agents/prog_j6.elf
/tmp/opencode/j6_agents/prog_j6.elf > /tmp/opencode/j6_agents/elf.out
ROSE_SANS_JIT=1 ./rose/rose /tmp/opencode/j6_agents/prog_j6.rose \
    > /tmp/opencode/j6_agents/nat.out
diff /tmp/opencode/j6_agents/nat.out /tmp/opencode/j6_agents/elf.out
```
