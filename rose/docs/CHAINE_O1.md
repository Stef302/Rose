# Chaîne O(n) — fermeture du goulot de concaténation (constat daté)

**Horodatage** : 2026-11-18T15:34:46Z → 15:48:03Z (reproductions, variantes,
salves), 15:50Z → 15:56Z (validation finale et salves A/A finales). Tout est un
**constat daté** (règle `AGENTS.md`) : re-mesurer après tout rebuild, tout
changement de topologie ou tout changement d'état machine.

**Périmètre respecté** : deux fichiers modifiés, `rose/src/interp.rs` et
`rose/src/run.rs`. Aucun autre `.rs`, aucun `Cargo.toml`, aucun doc d'un autre
agent, aucun commit, aucune publication. Les `#[inline(always)]` de `vers_z`,
`cmp_vals`, `valeur_egale` et les `#[inline]` des constructeurs (commit `0773395`)
sont **préservés** (vérifié par diff : aucune ligne d'attribut supprimée).

**Binaire d'entrée** : `ce2188308eab1f43846f2d95aa7e42135ced6661f62992189557bf7620fb6c31`
(reconstruit depuis les sources d'origine, hash identique —
`/tmp/opencode/chain_opt/rose_base_ab`).
**Binaire final** : `c05d72e5574a8279326aa2f3b8bc24f88b8b68df1abc2e1219d2aaca6725a4fb`
(`cargo build --release --bin rose`, copié dans `rose/rose`).

**Protocole** : 9 (puis 15) échantillons entrelacés par rotation, pinés par
`taskset -c 15` ET `-c 19`, horloge interne `temps()` du programme, hash du
binaire contrôlé avant/après chaque salve, valeur de sortie vérifiée à chaque
échantillon. Deux salves A/A complètes du bench multi-domaines (`PIN_CPU=19
NSAMP=9 python3 bench.py`).

---

## 1. Reproduction du goulot (étape 1)

`/tmp/opencode/benchmulti/r_str.rose` : `$s = ""` puis 20 000 × `$s = $s + "x"`,
puis `longueur($s)`. Valeur vérifiée : **20000** à chaque échantillon.

| Binaire | c19 médiane | c15 médiane | étendue (c19) |
|---|---:|---:|---:|
| `ce218830…` (départ) | **6,181 ms** | **6,157 ms** | 6,151 – 6,233 |

Reproduit le `6,160 ms` du constat `BENCH_APRES_GOULOT.md` à 0,3 % près.

Deux diagnostics isolent la part O(n²) du coût fixe de la boucle :

| Programme (20 000 tours) | c19 | c15 | contenu |
|---|---:|---:|---|
| `diag_add_int` | 2,139 ms | 2,145 ms | `$s = 0` ; `$s = $s + 1` (voie abstraite i64) |
| `diag_assign` | 2,228 ms | 2,223 ms | `$s = ""` ; `$s = "x"` (sans concaténer) |
| `r_str` | 6,181 ms | 6,157 ms | `$s = $s + "x"` |

**Constat [M]** : le plancher de la boucle + synchronisation est ~**2,1 ms** ;
la concaténation O(n²) n'ajoute que ~**4,0 ms**. Ce chiffre est central pour
la porte : même une concaténation parfaite en O(1) ne peut pas descendre sous
~2 ms sans modifier la boucle RIR (`rir.rs`, fichier interdit).

---

## 2. Décision architecturale (étape 2)

### 2.1 Rejet mesuré de (a) — append en place `Rc::make_mut`

Instrumentation temporaire de `binaire_valeurs` (`Rc::strong_count(x)`),
bâtie puis retirée :

```
RCCOUNT 3 0
RCCOUNT 3 1
RCCOUNT 3 2
...
```

**Le refcount réel est 3**, pas 1 : le slot, `globaux["s"]`, et le clone local
`va` créé par `eval_slots(Expr::Var)` détiennent le même `Rc<String>` au moment
du `+`. `Rc::make_mut` clones donc systématiquement la chaîne entière → **aucun
gain**, l'option (a) est éliminée par la mesure, pas par supposition.

Deuxième obstacle, structurel : `$s = $s + "x"` est abaissé en
`Op::AffectAst` (le `ChaineLit` n'est pas abstraisable, `rir.rs::abstraire`
rend `None`). L'unique point d'accroche disponible dans les fichiers autorisés
est `Interp::eval_ast`, appelé **aussi** par `Op::ExprStmt`, `Op::SautSiFaux`,
`Op::Retour`, `Op::Lever`. Une mutation en place déclenchée depuis ce hook
corromprait des expressions non-affectation réelles du compilateur Rose,
par exemple `compilateur/emetteur_cpp.rose:1204` `>> $sig + " {"` ou
`jardins/tests/premiers.rose:21` `>> "✗ " + nom + ...`. Détecter le contexte
d'affectation exigerait de modifier `rir.rs` (interdit). **Rejeté.**

### 2.2 Architecture retenue — (b) rope persistante

`Valeur::Chaine(Rc<String>)` → `Valeur::Chaine(Rc<ChaineRep>)` avec

```rust
pub struct ChaineRep { pub n: usize, pub octets: usize, pub noeud: NoeudChaine }
pub enum NoeudChaine { Plein(String), Concat(Rc<ChaineRep>, Rc<ChaineRep>) }
```

- `+` = `ChaineRep::concat` : **O(1)**, deux clones de `Rc`, zéro octet copié ;
- `longueur` = `s.n` : **O(1)** (le nombre de *caractères* Unicode est maintenu
  à chaque concaténation, pas le nombre d'octets) ;
- `aplatir()` itératif (pile explicite) pour les primitives qui ont besoin des
  octets (`sous_chaine`, conversions, affichage, indexation, `ecrire_binaire`) ;
- `Drop` **écrit à la main et itératif** : un arbre de 20 000 nœuds détruit par
  le drop récursif par défaut déborderait la pile. Le `Vec` n'est alloué que
  pour un nœud `Concat` (mesuré : `vec![noeud]` systématique coûtait une
  allocation par feuille, soit 4,2 ms → 3,9 ms sur la boucle) ;
- comparaison/égalité avec court-circuit sur les feuilles `Plein` (aucun regain
  d'allocation pour les clés de table, domaine D7).

**Blast radius mesuré** : 21 sites `Valeur::Chaine`, **tous** dans `interp.rs`
et `run.rs` — les deux fichiers autorisés. Aucun `Rc<String>`, aucun
`Rc::ptr_eq`, aucun `Rc::into_raw` ailleurs dans `src/`.

Complément mesuré : les littéraux chaîne sont internés par thread
(`valeur_litteral_chaine`, `Box<str>` + `Rc<ChaineRep>`). Le diagnostic
`$v = "x"` hoisté hors boucle donnait **2,292 ms** contre 3,908 ms avec le
littéral dans la boucle → **1,6 ms** étaient de l'allocation répétée de `"x"`.

### 2.3 Variantes mesurées et rejets

Toutes les variantes ci-dessous ont été réellement compilées, binaires copiés
et mesurées (15 échantillons, c19). `D2` = boucle entière de `somme(10⁶)`.

| Variante | D5 (ms) | D2 (ms) | Verdict |
|---|---:|---:|---|
| départ `ce218830…` | 6,173 | 8,526 | référence |
| rope seule (pas d'internage, sync d'origine) | 4,147 | 8,509 | gain réel mais insuffisant |
| rope + internage + sync `get_mut(&str)` sur tous les slots | 2,991 | 9,54 | **rejetée** : +12 % sur D2 |
| rope + internage + sync conditionnel (`corps.globaux`) | 2,96 | 9,80 | **rejetée** : +15 % sur D2 |
| rope + internage à cache linéaire (sans hachage) | 3,214 | 9,423 | **rejetée** : plus lente et +11 % D2 |
| rope + internage (cache HashMap) + sync d'origine | **3,292** | **9,03** | **retenue** |

Le gain de la variante retenue est net (+2,88 ms sur D5) et la régression D2
(+0,5 ms, +6 % en mesure isolée, +3 % dans le bench complet) est inférieure à
celle de toutes les variantes plus rapides. La cause de cette régression n'a
**pas** été isolée : la partie rope seule ne la produit pas ; elle apparaît dès
que le code d'internage est présent, alors que D2 n'évalue aucun `ChaineLit`.
Hypothèse : permutation de placement de code LLVM dans l'interpréteur. Les
rebuilds du binaire de départ sont déterministes (8,520 et 8,523 ms), donc ce
n'est pas du bruit de build. **Constat honnête, pas une explication prouvée.**

---

## 3. Porte (étape 3) — NON ATTEINTE

| | avant | après (2 salves A/A, bench complet) | gain |
|---|---:|---:|---:|
| Domaine 5, chaîne 20 000 | 6,160 ms | **3,284 / 3,308 ms** | **1,87 ×** |

**La porte < 1,0 ms n'est pas atteinte.** Le meilleur temps obtenu toutes
variantes confondues est **2,96 ms** (variante rejetée pour régression D2) ;
le temps de la variante retenue est 3,28 ms. Le plafond structurel mesuré est le
plancher de boucle de ~1,7–2,2 ms (`diag_add_int` après optimisation : 1,71 ms
sur c15), qui siège dans `rir.rs::executer_avec_ptr_sync` / la synchronisation
globale — fichier interdit à cette mission. Pour passer sous 1 ms il faudrait
aussi supprimer la synchronisation slots ↔ globaux par opération, ce qui est
un autre chantier architectural, hors périmètre.

**Preuve de la complexité** (9 échantillons, c19, mêmes binaires) :

| N | départ | final | gain |
|---:|---:|---:|---:|
| 5 000 | 1,259 ms | 0,821 ms | 1,53 × |
| 10 000 | 2,665 ms | 1,647 ms | 1,62 × |
| 20 000 | 6,159 ms | 3,298 ms | 1,87 × |
| 40 000 | 17,896 ms | 6,567 ms | 2,73 × |

Le doublement de N **double** le temps final (0,82 → 1,65 → 3,30 → 6,57 :
×2,01 / ×2,00 / ×1,99) alors qu'il le multiplie par 2,1 → 2,3 → 2,9 dans le
binaire de départ. **O(n²) → O(n) prouvé par la mesure**, indépendamment de la
porte chiffrée.

---

## 4. Tableau des 9 domaines (avant / après, médianes internes, c19)

| # | Domaine | avant (ms) | s1 (ms) | s2 (ms) | gain |
|---|---|---:|---:|---:|---:|
| 1 | `fib(35)` récursif | 11,600 | 11,584 | 11,617 | 1,00 × |
| 2 | Boucle 1..10⁶ | 8,743 | 9,037 | 9,060 | **0,97 ×** (régression) |
| 3 | Premiers < 50 000 | 9,453 | 9,339 | 9,323 | 1,01 × |
| 4 | Factorielle 5 000 | 1,484 | 1,480 | 1,482 | 1,00 × |
| 4b | PGCD 1 000 chiffres | 6,735 | 6,721 | 6,708 | 1,00 × |
| 5 | **Chaîne 20 000** | **6,160** | **3,284** | **3,308** | **1,87 ×** |
| 6 | Liste 100 000 entiers | 38,591 | 38,106 | 38,122 | 1,01 × |
| 7 | Table 3 000 clés | 6,332 | 6,283 | 6,287 | 1,01 × |
| 8 | E/S 10 Mo | 13,490 | 14,564 | 14,822 | **0,92 ×** (bruit E/S) |

D6 et D7, qui passent par le même `interp.rs`, ne régressent **pas** (ils
gagnent ~1 %). D2 régresse de 3 % de façon reproductible (voir §2.3). D8 est
dominé par le bruit E/S : min 13,27 / 14,41 ms après contre 13,44 ms avant,
max 16,5 ms — la médiane n'est pas attribuable avec confiance à ce changement.

---

## 5. Exactitude (étape 4) — sorties réelles

Programme `/tmp/opencode/chain_opt/exactitude.rose`, exécuté sur le binaire
final (pin c19). Toutes les sorties sont conformes :

| Expression | Attendu | Obtenu |
|---|---|---|
| `longueur("abcé" + "Ω")` | 5 | **5** |
| `("abcé" + "Ω")` | abcéΩ | **abcéΩ** |
| `sous_chaine($b, 3, 5)` | éΩ | **éΩ** |
| `longueur("")` | 0 | **0** |
| `longueur("" + "x")` | 1 | **1** |
| `longueur($v)` après 20 000 × `+ "x"` | 20000 | **20000** |
| `sous_chaine($v, 19998, 20000)` | xx | **xx** |
| `$y = $x ; $x = $x + "c" ; → $y` (aliasage) | ab | **ab** |
| idem `→ $x` | abc | **abc** |
| `longueur($y)` / `longueur($x)` | 2 / 3 | **2 / 3** |
| `longueur("" + "")` | 0 | **0** |

L'aliasage est préservé : le `+` construit un nœud neuf, il ne mute jamais le
`Rc` d'origine. L'aplatissement itératif est validé sur une chaîne de 20 000
nœuds (`sous_chaine` en fin de chaîne) — pas de débordement de pile.

---

## 6. Validation obligatoire (binaire final `c05d72e…`)

| Gate | Commande exacte | Résultat réel |
|---|---|---|
| Build | `cargo build --release --bin rose` + copie dans `rose/rose` | rc=0, SHA `c05d72e…` |
| Jardin | `./rose/rose jardins/tests/premiers.rose` | **20 tests, 0 échec** |
| fib25 | `./rose/rose /tmp/opencode/fib25.rose` | **75025** |
| Point fixe | `./rose/rose rir compilateur/emetteur.rose > …` ×2 + `md5sum` | **bdbef63681608a0b22a5637539e9d738 ×2** |
| Oracles M4 | `bash scripts/audit-final.sh` | lexeur **152/0**, analyseur **152/0**, vérificateur **152/0**, émetteur **152/0** |
| cargo test | `cargo test --release` (depuis `rose/`) | **0 échec** (160 + 144 + 6 + 1 pass) |
| Bench A/A | `PIN_CPU=19 NSAMP=9 python3 bench.py` ×2 | 9 domaines, §4 ; D5 convergé à 0,7 % |

L'audit complet rend `rc=0` et son verdict :
`1. Oracles M4 : lexeur 152/0 | analyseur 152/0 | vérificateur 152/0 | émetteur 152/0`,
`Point fixe octet pour octet ATTEINT`.

---

## 7. Ce qui reste

1. **La porte < 1 ms n'est pas atteinte** : plafond mesuré ~1,7–2,2 ms venant
   de la synchronisation slots ↔ globaux dans `rir.rs` (fichier interdit ici).
   Chantier suivant possible : supprimer cette synchronisation du chemin chaud,
   avec sa propre mesure et ses propres oracles.
2. **Régression D2 de 3 %** (bench) / 6 % (isolé), reproductible, cause non
   isolée (§2.3). À ré-examiner si un autre agent modifie la disposition du
   code de l'interpréteur.
3. **Régression D8 apparente de 8 %**, non attribuable avec confiance (bruit
   E/S, min comparable).
4. L'internage des littéraux garde une entrée par texte distinct dans un
   `thread_local` : borne mémoire = taille du source, pas de croissance liée à
   l'exécution.

## 8. Reproduction

```
cd /tmp/opencode/chain_opt
BIN=/home/clausake/Stefan/ROSE/rose/rose PIN=19 N=9 python3 mesure.py final
BIN=/home/clausake/Stefan/ROSE/rose/rose PIN=15 N=9 python3 mesure.py final
cd /tmp/opencode/benchmulti && PIN_CPU=19 NSAMP=9 python3 bench.py
sha256sum /home/clausake/Stefan/ROSE/rose/rose
# c05d72e5574a8279326aa2f3b8bc24f88b8b68df1abc2e1219d2aaca6725a4fb
```

Données brutes : `/tmp/opencode/chain_opt/mesure_*.json`, `mesure_*.log`,
`final_s1.json`, `final_s2.json`, `audit_final2.log`, `cargo_test_final.log`,
`exactitude.rose`, `scale_*.rose`, `rose_base_ab` (binaire de départ hashé).
