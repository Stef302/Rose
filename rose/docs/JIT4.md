# JIT4 — JIT Cranelift TOTAL (vague 84)

**Constat daté : 21/11/2026, soirée UTC.** Toutes les mesures ci-dessous ont
été prises sur l'arbre en cours (HEAD `d870d89` + changements JIT4 portés dans
`rose/src/jit.rs`) avec `rose/target/release/rose` reconstruit (`cargo build
--release --bin rose`, rc 0). Ce sont des constats, pas des consignes.

Périmètre de ce chantier : **`rose/src/jit.rs` uniquement** + ce document.
Aucun commit, aucune publication, aucun tag effectué par ce chantier.
`rose/src/rir.rs` est inchangé.

---

## 1. Diagnostic initial (avant modification)

Harnais `/tmp/opencode/jit4/{make_progs.py,audit.py,parite.py}` : 25 programmes
variés, `ROSE_TIMING=1`, recherche de `TIMING-JIT eligibles=…`,
`TIMING-JIT principal=…` et de toute ligne contenant « repli interpréteur ».

Résultat de départ (`audit_baseline.txt`) :

- **10/25** programmes avec `principal=cranelift` ;
- **1** ligne explicite `principal=repli-interpreteur` (`p25_mix`) ;
- **14** programmes sans AUCUNE ligne `principal=` : `prepare_jit` répondait
  `a_du_travail=false` et aucun module Cranelift n'était construit.

Causes exactes (`ROSE_JIT4_DUMP=1`, instrumentation ajoutée dans la porte) :

| Programme | Refus RIR (première opération rejetée) |
|---|---|
| `p03_rat` | `Afficher([Litteral rationnel "1/2"])` |
| `p04_bigint_pow` | `AfficherAbstrait(Binaire{Puissance, 2, 100})` — exposant > 64 |
| `p05`/`p06` | `Afficher([Primorielle/Factorielle(…])` |
| `p08` | `Afficher([Binaire{"+", ChaineLit, ChaineLit}])` |
| `p09`/`p12`/`p22`/`p23` | `Afficher([Appel "longueur"/"cle_existe"/Index])` |
| `p10` | `Afficher([ListeLit(…)])` |
| `p19` | `AffectAst(Litteral rationnel)` + `Afficher([Binaire "+" rationnel])` |
| `p20` | `Affect{Puissance 2^70}` refusé par `expr_peut_boxer` |
| `p21` | `Afficher([Appel "saluer"(ChaineLit)])` — fonction à paramètre chaîne non éligible |
| `p25` | `AffectAst(Binaire "+" chaîne + chaine(carre($j)))` |

---

## 2. Implémentation — du VRAI code généré + runtime machine

### 2.1 ABI des handles

Une valeur non triviale (rationnel exact, BigInt, irrationnel, chaîne, liste,
table, complexe boxé) est un **handle `i64`** indexé dans `JIT_HANDLES`,
l'arène thread-local déjà utilisée par les littéraux de chaîne. Les entiers,
floats et booléens restent en registres (`i64` / `f64` / `0-1`).
`expr_ast_handle` convertit un i64 nu en handle au moyen d'un appel machine
quand une opération mixte l'exige (`$r + 1` avec `$r` rationnel). Un f64 est
converti par `rose_jit_flottant_handle` — jamais par son motif de bits.

### 2.2 Helpers machine ajoutés

Déclarés dans `table_symboles_runtime()` (adresse résolue sans `dlsym`) et
`helpers_runtime()` (signature Cranelift), implémentés en bas de `jit.rs` :

| Helper | Signature | Rôle |
|---|---|---|
| `rose_jit_entier_handle` | `(i64)->i64` | boxe un entier nu en handle |
| `rose_jit_flottant_handle` | `(f64)->i64` | boxe un f64 en handle |
| `rose_jit_valeur_afficher` | `(i64)->()` | affichage exact d'un handle (`run::afficher`) |
| `rose_jit_valeur_binaire` | `(i64,i64,i64)->i64` | `+ - * / ^ % == != < <= > >=` sur valeurs exactes via `interp::binaire_valeurs` |
| `rose_jit_valeur_unaire` | `(i64,i64)->i64` | factorielle, primorielle, terminal, multi/sous/super-factorielle |
| `rose_jit_valeur_chaine` | `(i64)->i64` | conversion `chaine(valeur exacte)` |
| `rose_jit_table_inserer_handle` | `(i64,i64,i64)->()` | insertion à clé handle (chaîne) |
| `rose_jit_table_cle_existe_handle` | `(i64,i64)->i64` | présence d'une clé handle |

Ces routines sont appelées par des `call` Cranelift ; aucune ne relit un
bytecode Rose, aucune ne remplace une fonction compilée.

### 2.3 Portes et émetteur

- `NatureSlot::Valeur` / `Booleen` : inférence des slots handle et des résultats
  booléens (`cle_existe`), avec **parité d'affichage** (`vrai`/`faux`, corrigé
  aussi sur la voie AST).
- `Op::Afficher(Vec<Expr>)` est désormais émis (i64, f64, booléen ou handle),
  plus aucun `Afficher` AST ne force le repli.
- `AffectAst` couvre littéraux rationnels/grands entiers, factorielles,
  `ListeLit`, `chaine(...)`, `cle_existe`, `Index` à indice littéral, et les
  appels de fonctions compilées.
- Puissance entière : borne d'exposant 64 → 1024, le résultat boxé publie son
  tag et le drapeau par slot est mis à jour (aucune lecture i64 d'un pointeur
  Mpz).
- `joindre($l, $j*$j)` : preuve d'i64 nu étendue aux expressions `+ - *`.
- Appels utilisateur : `expr_ast_rir` émet un appel direct vers la fonction
  Cranelift (handles en paramètres/retour). Quand la fonction n'est pas
  déclarée, un appel au helper à 5 arguments est émis avec arité adaptée
  (plus d'appel à 7 arguments vers une signature de 5 — assertion Cranelift
  `abi.rs` corrigée). Le trampoline Rust→JIT convertit handle↔`Valeur`.

### 2.4 Garde-fous de sûreté (mesurés)

Ces restrictions évitent de compiler des corps dont l'ABI ne peut pas
transporter l'état ; elles sont documentées, pas masquées :

- fonction à handle : corps ≤ 8 opérations (les jardins GUI enchaînent tables
  mutées et appels délégués → `inserer attend une table`) ;
- aucun appel de plus de 4 paramètres dans un corps compilé ;
- ABI de fonction limitée aux chaînes (listes/tables/rationnels restent des
  handles dans le **principal** uniquement) : sans cela le point fixe
  `b3ed9262` cassait (`Variable 'fn' utilisée sans valeur`) ;
- une fonction sans retour (`Vide` ou `Dyn` sans `retour expr`) n'est pas une
  valeur affichable (`v_aff_vide` rendait `0` au lieu de rien).

Diagnostic optionnel : `ROSE_JIT4_DUMP=1` imprime les refus de porte
(`JIT4-RIR`), les échecs d'émission (`JIT4-EMET-*`) et les appels émis
(`JIT4-APPEL`).

---

## 3. Résultat : 25/25 par le code Cranelift

Binaire final reconstruit ; `ROSE_TIMING=1` sur les 25 programmes, une seule
ligne `principal=` retenue par programme (la seconde série `eligibles=` est
celle de `compiler_tout`).

| # | Programme | Valeur (parité confirmée) | `eligibles` | Principal |
|---|---|---|---|---|
| 1 | `p01_arith` | `47` | `[]` | `principal=cranelift voie=rir` |
| 2 | `p02_float` | `11.5` | `[]` | `principal=cranelift voie=rir` |
| 3 | `p03_rat` | `0.5` | `[]` | `principal=cranelift voie=rir` |
| 4 | `p04_bigint_pow` | `1267650600228229401496703205376` | `[]` | `principal=cranelift voie=rir` |
| 5 | `p05_primorielle` | `8082…1560420730…` (300 chiffres) | `[]` | `principal=cranelift voie=rir` |
| 6 | `p06_factorielle` | `815915283247897734345611269596115894272000000000` | `[]` | `principal=cranelift voie=rir` |
| 7 | `p07_complexe` | `23+2i` | `[]` | `principal=cranelift voie=rir` |
| 8 | `p08_chaine_lit` | `abcdef` | `[]` | `principal=cranelift voie=rir` |
| 9 | `p09_chaine_boucle` | `20` | `[]` | `principal=cranelift voie=rir` |
| 10 | `p10_liste_lit` | `[1, 2, 3]` | `[]` | `principal=cranelift voie=rir` |
| 11 | `p11_liste_boucle` | `20` / `49` | `[]` | `principal=cranelift voie=rir` |
| 12 | `p12_table` | `vrai` / `2` | `[]` | `principal=cranelift voie=rir` |
| 13 | `p13_fib` | `6765` | `["fib"]` | `principal=cranelift voie=rir` |
| 14 | `p14_carre` | `49` | `["carre"]`, `rir=true` | `principal=cranelift voie=rir` |
| 15 | `p15_ops` | `3.5` / `1` | `["fdiv","fmod"]`, `rir=true` | `principal=cranelift voie=rir` |
| 16 | `p16_boucle_ent` | `1` | `[]` | `principal=cranelift voie=rir` |
| 17 | `p17_pgcd` | `6` | `["pgcd"]` | `principal=cranelift voie=rir` |
| 18 | `p18_bool` | `vrai` / `faux` | `["est"]`, `rir=true` | `principal=cranelift voie=rir` |
| 19 | `p19_rationnel_var` | `0.[428571]` | `[]` | `principal=cranelift voie=rir` |
| 20 | `p20_grand_entier_var` | `1180591620717411303425` | `[]` | `principal=cranelift voie=rir` |
| 21 | `p21_chaine_chaine` | `salut monde` | `["saluer"]`, `rir=true` | `principal=cranelift voie=rir` |
| 22 | `p22_liste_long` | `3` / `20` | `[]` | `principal=cranelift voie=rir` |
| 23 | `p23_table_ent` | `vrai` / `2` | `[]` | `principal=cranelift voie=rir` |
| 24 | `p24_appels_imbriques` | `6` | `["triple","add"]` | `principal=cranelift voie=rir` |
| 25 | `p25_mix` | `014916` | `["carre"]`, `rir=true` | `principal=cranelift voie=rir` |

Preuves brutes : `/tmp/opencode/jit4/audit_final.txt` (25 lignes
`TIMING-JIT principal=cranelift voie=… define_function+finalize_definitions OK`,
**0** ligne « repli interpréteur »), `/tmp/opencode/jit4/audit.json`.
`eligibles=[]` ne signifie PAS « interprété » : aucun de ces programmes n'a de
fonction nommée, mais son **principal** a bien été défini et finalisé.

---

## 4. Vérifications

| Contrôle | Résultat |
|---|---|
| `cargo build --release --bin rose` | rc 0 (arbre intégré HEAD `94fb256` + jit.rs JIT4) |
| `cargo test --release` (arbre intégré) | **659 passés, 0 échec** (311 + 324 + 6 + 9 + 8 + 1 ; 6 ignorés) — 653 sur l'arbre isolé `d870d89` + JIT4 |
| Point fixe `b3ed9262` ×2 | `rose rir compilateur/emetteur.rose` et `rose compilateur/emetteur.rose compilateur/emetteur.rose` → md5 `b3ed9262dd54776a6dbd07bc12368d0e` ×2, 263 520 o, `cmp = 0`, stderr 0 octet |
| Parité des 25 programmes (JIT vs `ROSE_SANS_JIT=1`) | **25/25**, sortie octet pour octet |
| Banc `bench/suite/run_parite_jit.sh` (119 programmes) | arbre intégré : **109 ok / 10 divergences** ; arbre isolé : **112 ok / 7** contre 110 ok / 9 au départ. Toutes les divergences listées sont des programmes qui impriment des **durées en ms** (`power`, `sieve`, `sumsquares`, `factorial`, `montecarlo`, `philox`, `machine`) : aucune divergence sémantique nouvelle |
| Banc « 9 domaines × 9 tirages = 81/81 » | **non re-mesuré** : le harnais `/tmp/opencode/benchmulti/bench_now.py` cité dans `JIT_REEL.md` n'existe plus sur cette machine. Aucun résultat 81/81 n'est revendiqué ici |

## 5. Limites factuelles

- Les fonctions très complexes à handles (jardins GUI : listes de chaînes,
  tables mutées, appels > 4 arguments) restent interprétées : leur ABI ne
  transporte pas l'état sans casser le self-host. Aucune de ces fonctions
  n'apparaît dans les 25 programmes ci-dessus, où **tout** passe par le code
  Cranelift.
- Le helper `rose_appel_interpreteur` reste le mécanisme d'appel des
  fonctions non déclarées ; les appels sont alors émis avec l'arity correcte
  (5 arguments) par du code Cranelift.
