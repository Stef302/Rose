# JIT du corps principal — mur des types non numériques (D5 chaîne, D6 liste)

**Constat daté : 18/11/2026, 19:02 UTC.** Toutes les mesures ci-dessous sont des
constats à cet horodatage, pas des consignes : re-mesurer après tout changement
de moteur, de quantification ou de topologie. Pinage `taskset -c 15` **et**
`-c 19`, 9 échantillons entrelacés, médianes. Aucune mesure concurrente lourde.

- Binaire de départ (HEAD `c83ab4a`, source inchangée, rebuild propre) :
  `/tmp/opencode/rose_baseline_bin` sha256
  `114424e0b625798e71351512700bf940aa72d42566e6e4c122b0540441a784cb`.
  Le binaire livré au début du chantier (`f9ae30b3…`) a été remplacé par les
  builds successifs ; sa source est exactement HEAD, et le rebuild propre ci-dessus
  reproduit les mêmes temps (D5 3,33 ms ; D6 38,7 ms).
- Binaire final : `rose/rose` sha256
  `8383468b745439c6993b1553bb7c960a40844a42494af9f91c40017b61c00950`.
- Périmètre : **`rose/src/jit.rs` uniquement** + ce document. `git diff --stat`
  final : `rose/src/jit.rs | 559 +, 10 -`. `interp.rs` et `rir.rs` sont revenus à
  l'état HEAD (instrumentation temporaire retirée). Aucun commit, aucune publication.

---

## 1. Temps 1 — diagnostic chiffré (avant toute implémentation)

### 1.1 Le JIT refuse le principal de D5 et D6

`ROSE_PROFIL_RIR=1 rose r_str.rose` / `r_list.rose` (binaire de départ) :

```
PROFIL-RIR principal noms=["d","s","i","n","f"] ops=11
PROFIL-RIR fonctions_abaissees=0 principal=true
```

Le RIR est bien abaissé (`principal=true`) mais `rir_compilable_jit`
(`jit.rs:1031`) rejette `Op::AffectAst` (hors `temps()`), `Op::ExprStmt` et
`Op::SautSiFaux`. `ROSE_DUMP_RIR` (instrumentation temporaire, retirée) montre
les boucles exactes :

```
D5 op[4] AffectAst { slot:1, e: Binaire{"+", Var("s"), ChaineLit("x")} }
D6 op[4] ExprStmt(Appel "joindre"(Var("l"), Var("i")))
D6 op[9] SautSiFaux { test: Binaire{"<", Var("j"), Appel "longueur"(Var("l"))} }
D6 op[10] AffectAst { slot:3, e: Binaire{"+", Var("s"), Index(Var("l"),Var("j"))} }
```

### 1.2 Où va réellement le temps — programmes isolés

Harnais `/tmp/opencode/diag/diag.py` et `diag2.py`, 9 échantillons entrelacés,
pin c19, binaire de départ. `noRIRprinc` = `ROSE_NO_RIR_PRINCIPAL=1`
(interprétation AST d'origine).

| Programme isolé | base (ms) | noRIRprinc (ms) | lecture |
|---|---:|---:|---|
| boucle vide 20 k, **JIT** | **0,024** | 7,23 | le JIT élimine ~99,7 % du coût |
| boucle vide 20 k, interprétée | 2,18 | 7,23 | 109 ns/it pour 4 ops |
| `$s = $s` (slot→slot) | 1,95 | 4,16 | 97 ns/it : **dispatch + synchro seuls** |
| `$s = "x"` (AST, sans concat) | 2,19 | 3,98 | +12 ns/it pour le dispatch AST |
| `$s = $s + "x"` = D5 | 3,34 | 7,15 | +57 ns/it pour la rope elle-même |
| `joindre` seul 100 k | 13,8 | 30,7 | 138 ns/it |
| `longueur($l)` dans le test 100 k | 24,8 | 48,6 | 248 ns/it |
| index 100 k, borne **hissée** | 15,5 | 48,6 | **−9,3 ms** : l'appel `longueur` par itération coûte 93 ns |
| D6 complet | 38,6 | 79,1 | 13,8 + 24,8 ≈ 38,6 (cohérent) |

**Prémisse corrigée.** Le coût dominant n'est NI la rope (57 ns/it sur 167),
NI la synchro, NI l'appel de `longueur` isolément : c'est le **dispatch RIR
interprété** (97 ns/it, 58 % de D5) — et le JIT le supprime (boucle vide ×91).
Un hissage de `longueur($l)` ne gagnerait que 9,3 ms sur D6 (38,6 → 29,4),
insuffisant pour la porte < 10 ms. La synchro a été mesurée séparément
(no-op `synchroniser_slots`, instrumentation retirée) : 3,34 → 2,34 ms sur D5
(−30 %) et 38,5 → 31,0 ms sur D6 (−19 %) — réel mais secondaire.

### 1.3 Décision

Approche **(a)** retenue : étendre le JIT du principal aux opérations sur
types non numériques via des délégués runtime. Justification mesurée : le
diagnostic montre que le dispatch interprété domine, et qu'un JIT complet
apporte ×91 sur la boucle témoin. Approches (b) opcodes RIR spécialisés et
(c) deferral de synchro écartées : elles plafonnent à ~20–30 % d'après 1.2.

---

## 2. Temps 2 — implémentation

### 2.1 Representation : handles

Les valeurs non numériques vivent dans une table thread-local
`Vec<Valeur>` ; un slot JIT non numérique contient un **indice i64** (handle).
Le code natif n'accède jamais à la `Valeur` directement. Helpers ajoutés
(dans `jit.rs`, déclarés dans `helpers_runtime()` et `table_symboles_runtime()`) :
`rose_jit_chaine_concat` (règle exacte `binaire_valeurs("+")` → rope de C1),
`rose_jit_liste_vide`, `rose_jit_joindre` (mutation `Rc<RefCell>`),
`rose_jit_longueur` (O(1)), `rose_jit_index_i64`.

Les littéraux de chaîne sont alloués **une fois à l'émission** (handle constant
émis en `iconst`), pas à chaque itération. Variante intermédiaire mesurée :
appel runtime `rose_jit_chaine_lit(id)` à chaque itération → D5 1,533 ms ;
handle constant → **0,92 ms** (−0,61 ms).

### 2.2 Garde statique

`naturesDuCorps` infère par point fixe la nature des slots
(`Entier` / `Chaine` / `ListeEntiers`). `slot_entier_nu_statique` prouve qu'un
compteur est un i64 nu (littéral initial + `slot ± littéral`, exactement la
preuve J1/P1 déjà utilisée par `expr_peut_boxer`). Une liste n'est réputée
« d'entiers nus » que si **tous** ses `joindre` reçoivent un tel compteur.
Sinon le JIT est refusé : repli interprété, aucune règle dupliquée, aucune
régression possible.

`slot_types_rir` marque désormais les slots handle comme `TyKind::Chaine` /
`TyKind::Liste` : `eabstrait_rir_compilable` refuse donc de les lire comme des
entiers (sécurité contre une lecture i64 d'un handle).

### 2.3 Émetteur

Nouveaux cas dans `emettre_fonction_rir` : `AffectAst` reconnue (chaîne, liste,
`longueur`, `+` chaîne, `index + entier`), `ExprStmt(joindre)`,
`SautSiFaux(j < longueur(liste))`. L'addition avec un élément de liste réutilise
`expr_add_sous_inline`, copie conforme du chemin i64 existant (overflow →
`rose_add_ent`, exactitude bigint préservée). Aucune modification du chemin
J1/P1/L1 existant : `git diff` ne montre que le réagencement de `AffectAst`.

### 2.4 Variantes rejetées (mesurées)

| Variante | Mesure | Verdict |
|---|---|---|
| (b) opcodes RIR spécialisés | non implémentée — plafond estimé 15 ns/op soit ~0,8/12 ms | écartée au profit de (a), ×91 prouvé |
| (c) synchro désactivée | D5 3,34→2,34 ; D6 38,5→31,0 ms | insuffisant seul, et casse la sémantique globale |
| Handle de littéral alloué par itération | D5 1,533 ms | remplacé par handle constant (0,92 ms) |
| Garde `SautSiFaux` oubliée (1er build) | D6 restait interprété à 38,5 ms | corrigée dans le garde |
| `std::env::var` dans la boucle de synchro (instrumentation) | D5 4,83 ms au lieu de 3,34 | instrumentation retirée |

---

## 3. Résultats (binaire final `8383468b…`)

### 3.1 A/A entrelacé avant/après, 9 échantillons, valeur vérifiée

| pin | Domaine | AVANT (ms) | APRÈS (ms) | gain | porte |
|---|---|---:|---:|---:|---|
| c19 | D5 chaîne | 3,342 | **0,939** | 3,56 × | **< 1,5 ms atteinte** |
| c15 | D5 chaîne | 3,340 | **0,930** | 3,59 × | atteinte |
| c19 | D6 liste | 38,634 | **2,131** | 18,13 × | **< 10 ms atteinte** |
| c15 | D6 liste | 38,685 | **2,137** | 18,10 × | atteinte |

### 3.2 Tableau des 9 domaines (médianes 9 échantillons)

| # | Domaine | AVANT (c19) | APRÈS c19 s1 | APRÈS c19 s2 | APRÈS c15 | ratio Rust (c15) |
|---|---|---:|---:|---:|---:|---:|
| 1 | `fib(35)` | 11,58 | 11,815 | 11,568 | 11,654 | 0,92 ✅ |
| 2 | Boucle 1..10⁶ | 1,290 | 1,294 | 1,292 | 1,295 | 4,95 |
| 3 | Premiers < 50 000 | 1,481 | 1,479 | 1,479 | 1,478 | 1,47 |
| 4 | Factorielle 5 000 | 1,45 | 1,377 | 1,376 | 1,378 | 0,21 ✅ |
| 4b | PGCD 1 000 ch. | 6,73 | 6,736 | 6,719 | 6,717 | 45,5 |
| 5 | Chaîne 20 000 | 3,33 | **0,911** | **0,924** | **0,927** | **65,3** |
| 6 | Liste 100 000 | 38,75 | **2,163** | **2,184** | **2,159** | **11,8** |
| 7 | Table 3 000 | 1,377 | 1,341 | 1,342 | 1,344 | 14,0 |
| 8 | E/S 10 Mo | 5,47 | 6,049 | 5,184 (bruit) | 6,177 | 1,01 ≈ |

Aucune régression systématique. D8 reste bruité (5,2–6,2 ms sur les trois
salves, comme dans les constats précédents).

---

## 4. Exactitude et validation

- `cargo build --release --bin rose` copié dans `rose/rose` avant toute mesure.
- `cargo test --release` : **160 + 144 + 6 + 1 = 311 passés, 0 échec**.
- `bash scripts/audit-final.sh` : oracles **152/0 ×4**, point fixe
  `bdbef63681608a0b22a5637539e9d738` atteint, jardin **20/20**, fib(25) OK,
  `cargo` lié/natif/bin rc=0. Verdict : « AUCUNE porte non atteinte ».
- Tests ciblés rope/listes comparés **octet pour octet** au binaire HEAD, en
  mode JIT et `ROSE_SANS_JIT=1` : **25/25 OK** (`/tmp/opencode/diag/exact_compare.sh`) :
  aliasage (`$t=$s` puis concat), Unicode (`héllo 🌍` + `é🌍`, `longueur` en
  caractères), insertion/lecture (`joindre` + `$l[$j]`), grand entier dans une
  liste (repli interprété), concatenation 20 000 + index final.
- Programmes du bench : 13/13 valeurs de sortie identiques à HEAD.

---

## 5. Limites documentées (honnêteté)

- `rose_jit_index_i64` suppose des éléments de liste i64 nus ; la preuve est
  statique (`slot_entier_nu_statique`). Une liste alimentée autrement reste
  interprétée, mais une expression `joindre($l, <littéral>)` est refusée par
  l'émetteur (repli JIT complet) — correct, au prix d'une opportunité perdue.
- Un programme finissant par `>> $liste` (handle) reste interprété :
  `AfficherAbstrait` exige un type scalaire. C'est le cas de `d_join.rose`
  (13,7 ms, hors bench officiel). Aucune porte ne porte sur ce cas.
- La table de handles n'est pas libérée pendant l'exécution (20 k handles pour
  D5, ~0 pour D6) ; elle est réinitialisée avant chaque compilation du principal.

## 6. Rejouer

```
cd /tmp/opencode/diag
python3 aa_d5d6.py                 # A/A entrelacé, c19 puis c15
cd /tmp/opencode/benchmulti
PIN_CPU=19 NSAMP=9 python3 bench.py   # ×2
PIN_CPU=15 NSAMP=9 python3 bench.py
bash exact_compare.sh
cd /home/clausake/Stefan/ROSE
bash scripts/audit-final.sh
```

Artefacts : `/tmp/opencode/diag/` (logs, JSON des salves, scripts),
`/tmp/opencode/rose_baseline_bin` (binaire HEAD de référence).
