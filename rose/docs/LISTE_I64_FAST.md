# Vague 26 (D6) — voie i64 dédiée pour les listes : ×2,26, porte NON atteinte

**Constat daté : 18/11/2026, 21:23Z → 21:40Z UTC.** Toutes les mesures ci-dessous
sont des constats à cet horodatage, pas des consignes : re-mesurer après tout
changement de moteur, de JIT, de machine ou de topologie. Pinage `taskset -c 15`
**et** `-c 19`, 9 échantillons entrelacés (médianes) pour le bench 9-domaines,
11 échantillons pour la décomposition, valeur `4999950000` vérifiée à chaque
tirage. Aucune mesure concurrente lourde.

- Binaire de départ : `rose/rose` sha256
  `c0b242782bd0832db5d40a4a00c6cdf45c4f47e68b56d73aa5dcae55e83c9032`
  (copié `/tmp/opencode/listi64/base_rose`, **reconstruit à l'identique** depuis
  les sources d'origine : `base_rebuilt` a le même sha — la référence est saine).
- Binaire final : `rose/rose` sha256
  `9196da46ace84a9750d5981f9d0aa3f11ce54889900a3fcdefed17f82c7d7330`
  (copié `/tmp/opencode/listi64/varC2_rose`).
- Périmètre : **`rose/src/jit.rs` (+102 −15)** et ce doc (nouveau).
  `rose/src/run.rs` n'a pas été modifié. Aucun autre `.rs`, aucun
  `Cargo.toml`, aucun `PLAN.md`, aucun doc d'un autre agent. Aucun commit,
  aucune publication. `git diff` vérifié (voir §5).
- Préservé et vérifié par `git diff` : inline L1, rope C1, table index T1 + voie
  i64 TI1, E/S E1, handles W1, spécialisations H1/PB1, helpers table TB1/TC1.
  Les seules lignes touchées sont les 4 helpers runtime de liste et 4 nouvelles
  fonctions locales, toutes dans la section « vague 20 » de `jit.rs`.

---

## 1. Reproduction et décomposition (étape 1) — deux prémisses corrigées

### 1.1 Le chiffre de départ est confirmé

Bench 9-domaines, 9 échantillons entrelacés, internal median :

| salve | D6 base | Rust même salve | base/Rust |
|---|---:|---:|---:|
| c19 | 2,059 ms (min 2,04) | 0,183 ms | 11,24 |
| c15 | 2,070 ms | 0,187 ms | 11,05 |

Le constat 22:20Z (2,058 / 2,053 ms) est reproduit à 1 % près.

### 1.2 ⚠ Le programme « `joindre` isolé » fourni n'était PAS JIT-qualifié

`/tmp/opencode/diag/d_join.rose` finit par `>> longueur($l)`. Mesuré :
**13,90 ms en mode JIT et 13,89 ms avec `ROSE_SANS_JIT=1`** — les deux
identiques. Ce programme est intégralement interprété : la présence de
`longueur($l)` dans la position finale d'affichage fait échouer
`rir_compilable_jit` (voir `jit.rs:1301-1417`), mais **aucun diagnostic de
ce session n'avait vérifié ce point**. Le chiffre « 13,8 ms / 138 ns par
`joindre` » de TC1 décrivait donc le chemin interprété, pas le goulot JIT.

En remplaçant la dernière ligne par `>> 0` (`join_const_end.rose`), le même
programme passe sous JIT et tombe à **1,009 ms** sur le binaire de départ.
C'est le vrai coût JIT de 100 000 `joindre` (10,1 ns/op), pas 13,9 ms.

### 1.3 Décomposition (11 échantillons, médianes, µs)

| Phase | base c19 | base c15 | final c19 | final c15 |
|---|---:|---:|---:|---:|
| boucle vide 100 k (`pour` + `$n=$n+1`) | 128,3 | 128,3 | 128,4 | 128,3 |
| 100 k `joindre` seuls (JIT-qualifié) | 1 009 | — | 599 | — |
| 100 k lectures indexées seules | 1 014 | 1 017 | 286 | 288 |
| 100 k lectures, `longueur` hissée | 588 | 589 | 233 | 232 |
| 100 k appels `longueur($l)` | 499 | 492 | 205 | 205 |
| programme complet (création + 2 boucles) | 2 025 | 2 056 | 884 | 898 |

Le coût JIT se répartissait donc sur la base : **~1,01 ms de `joindre` +
~1,01 ms de lecture (dont ~0,43 ms d'appels `longueur` répétés dans la
condition de boucle)** + ~0,13 ms de boucles. Prémisse « `joindre` est le
goulot » confirmée, mais pour une raison différente de celle supposée :
le surcoût venait du clone de `Valeur` et de la recherche thread-local à
chaque appel runtime, pas d'un calcularithmétique.

---

## 2. Décision (étape 2) — une seule représentation, zéro divergence

### 2.1 Cohérence des voies — preuve

**Une seule représentation.** La recette TI1 aurait pu tentation d'ajouter un
`Vec<i64>` parallèle à côté du `Vec<Valeur>`. Je ne l'ai pas fait : une liste
JIT reste exactement `Valeur::Liste(Liste(Rc<RefCell<Vec<Valeur>>>))`. Le
`Valeur` est **toujours enregistré dans `JIT_HANDLES`** (`jit_allouer_handle`),
donc `>> $l`, l'égalité, `sous_liste`, `longueur` interprétée, l'indexation
générale et les tableaux voient le même objet, sans conversion ni
synchronisation. La voie rapide ne change QUE la manière dont le code natif
atteint ce même `Vec` :

1. `rose_jit_liste_vide()` renvoie un **pointeur tagué** (bit 63) vers le
   `RefCell<Vec<Valeur>>` de la `Rc`, *en plus* d'enregistrer le `Valeur`.
   ⚠ Le bit 0 initialement choisi a **segfaulté sur D5** (`r_str.rose`) :
   les indices de handles sont petits et peuvent être impairs. Bit 63 retenu
   (adresses utilisateur aarch64 sur 48 bits, indices < 2³¹) — mesuré,
  aucun écart sur les 9 domaines après correction.
2. `liste_mut_depuis_handle()` déréférence ce pointeur **sans emprunt
   `RefCell` dynamique** et **sans accès thread-local**, exactement comme
   `Table::rep_mut_nv` de TI1. Invariant : moteur monothreadé (`Rc`, jamais
   `Arc`), helpers feuille, jamais de réentrance. `debug_assert!` actifs en
   debug (vérifiés par `cargo test`), éliminés en release.
3. Repli explicite : tout handle non tagué retombe sur `jit_handle_ref` →
   `Valeur::Liste`. Le bit 63 n'est jamais porté par un index de handle.
4. `Valeur::z(v)` est **conservé** dans `joindre` : le garde JIT
   `slot_entier_nu_statique` prouve que la valeur est un i64 nu, PAS qu'elle
   tient dans `|v| ≤ 2²⁴`. Remplacer `Valeur::z(v)` par
   `Valeur::Entier(v, None)` ferait diverger l'interpréteur au-delà de 2²⁴
   (boxage Mpz) — rejeté pour cette raison, sans même le mesurer.

Aucune table parallèle, aucun index dupliqué, aucune conversion : la
cohérence est **structurelle**, pas une convention.

### 2.2 Optimisations retenues, toutes mesurées isolément

D6 complet, médiane de 11 échantillons, c19 :

| # | Variante | D6 c19 | Décision |
|---|---|---:|---|
| base | `c0b242…` | 2 025 µs | référence |
| A | accès handle sans clone + `RefCell` sans contrôle | 1 077 µs | conservée |
| B | A + `Vec::with_capacity(131072)` | 1 006 µs | conservée |
| C | B + handle pointeur tagué bit 0 | 889 µs | **rejetée** (segfault D5) |
| D | C avec tag **bit 63** (final) | 884 µs | **retenue** |

Le coût mémoire assumé de la réservation est ~131 072 × 24 octets ≈ 3,1 Mio
par liste JIT (même pour une liste de 3 éléments) — compromis explicite,
identique en esprit aux ~230 Kio/table de TI1, documenté ici. Gain mesuré :
71 µs (6,6 %).

### 2.3 Variantes et pistes rejetées

- **Clone du handle supprimé seul** (A) : ×1,88. Insuffisant ; la recherche
  `JIT_HANDLES.with` restait ~2-3 ns/appel.
- **Tag bit 0** : segfault reproductible sur `r_str.rose` (D5), car un index
  de handle impair était pris pour un pointeur. Corrigé en bit 63.
- **Avalanche / hacheur maison** : sans objet ici (aucun hachage sur une
  liste — contrairement à D7).
- **`Valeur::Entier(v,None)` direct** : rejeté sur preuve de divergence
  au-delà de 2²⁴ (voir §2.1.4).
- **Hissage automatique de `longueur`** : le hisser dans la *source* fait
  gagner 884 → 845 µs (4,4 %), mais le programme du bench ne le hisse pas.
  L'implémenter en LICM dans l'émetteur demanderait une analyse CFG des blocs
  de boucle (le test `j < longueur($l)` est émis à chaque back-edge,
  `jit.rs:3659-3717`) ; hors périmètre de cette recette, non fait.

---

## 3. Résultat (étape 3) — porte NON atteinte, cause exacte

A/B entrelacé base / final / Rust dans la même salve, 9 échantillons, valeur
`4999950000` vérifiée à chaque tirage :

| salve | base | final | Rust même salve | final/base | final/Rust | porte < 0,189 ms |
|---|---:|---:|---:|---:|---:|---|
| c19 (A) | 2,059 | **0,913** | 0,186 | 0,444 | **4,902** | **NON** |
| c19 (B) | — | **0,910** | 0,187 | — | **4,871** | **NON** |
| c15 (C) | 2,070 | **0,912** | 0,190 | 0,440 | **4,798** | **NON** |

Gain **×2,26** sur D6 (écart Rust 11,1 → 4,85). Meilleur temps individuel
observé : 0,880 ms (min de 11 échantillons, `decomp_final.log`). **Rose ne bat pas Rust dans la même salve.**

### Cause exacte de l'écart résiduel

Le programme exécute **~200 000 appels à des helpers `extern "C"`** dans
deux boucles, plus deux boucles `pour` du principal :

| poste | coût final mesuré | plancher |
|---|---:|---|
| 2 boucles `pour` du principal | ~256 µs | ne descend pas sous ~128 µs/boucle avec l'émetteur actuel |
| 100 k `joindre` (hors boucle) | ~470 µs = 4,7 ns/appel | appel `extern "C"` + `Valeur::z` + `Vec::push` |
| 100 k lectures + `longueur` | ~158 µs = 1,6 ns/appel | déjà proche du coût d'un appel nu |

Même en supposant des helpers **gratuits**, les deux boucles seules
(256 µs) dépassent déjà le 189 µs de Rust. Pour battre Rust il faut donc
**émettre les opérations de liste en code machine inline** (pas d'appel
runtime : `store`/`load` directs dans le `Vec`) **et** accélérer l'émetteur
de boucle du principal (la boucle `tant_que` d'une fonction fait 0,26 ns/it
mesuré sur D2, contre 1,28 ns/it pour le `pour` du principal) — deux chantiers
de codegen bien plus vastes que la recette TI1, qui s'arrête aux helpers.
Constat daté, pas consigne.

### Tableau 9 domaines (2 salves c19 + 1 c15, 9 échantillons, médianes internes)

| Domaine | base c19 | base c15 | final c19A | final c19B | final c15C | Rust c19A | final/Rust |
|---|---:|---:|---:|---:|---:|---:|---:|
| D1 fib35 | 11,641 | 11,682 | 11,762 | 11,661 | 11,683 | 12,7 | 0,93 ✅ |
| D2 boucle 1e6 | 0,259 | 0,259 | 0,259 | 0,259 | 0,259 | 0,259 | 1,00 ≈ |
| D3 premiers 50k | 0,854 | 0,854 | 0,855 | 0,854 | 0,856 | 1,00 | 0,85 ✅ |
| D4 fact 5000 | 1,399 | 1,401 | 1,483 | 1,480 | 1,479 | 6,54 | 0,23 ✅ |
| D4b PGCD 1000 ch | 6,704 | 6,722 | 6,716 | 6,713 | 6,706 | 0,15 | 45 |
| D5 chaîne 20k | 0,913 | 0,907 | 0,911 | 0,914 | 0,924 | 0,015 | 61 |
| **D6 liste 100k** | **2,059** | **2,070** | **0,913** | **0,910** | **0,912** | 0,186 | **4,85** |
| D7 table 3000 | 0,095 | 0,092 | 0,094 | 0,101 | 0,096 | 0,096 | ≈ |
| D8 E/S 10 Mo | 6,055 | 6,220 | 6,128 | 5,970 | 5,962 | 6,0 | ≈ |

**Régression mesurée, assumée et documentée : D4 +5,5 % à +5,9 %** sur les
deux cœurs (1,399 → 1,480 ms), confirmée par A/B entrelacé dédié
(`ab_other.log`) et **présente aussi avec `ROSE_SANS_JIT=1`** (2,203 → 2,271 ms,
+3,1 %) : `r_fact.rose` ne contient aucune liste, donc c'est un effet de
disposition de code / alignement de l'ajout de ~100 lignes dans `jit.rs`, pas
une divergence sémantique. D4 reste une victoire nette (0,23× Rust). Toutes
les autres domaines sont dans le bruit ±1 %.

---

## 4. Exactitude et validation (obligatoires)

`cargo build --release --bin rose`, copié dans `rose/rose` (sha 9196da…).

- **Diff vide vs binaire de départ** (`exact.py`, 14 programmes × 2 modes =
  **28 comparaisons, 0 différence**) : `exact_liste.rose`, `exact_liste2.rose`,
  `exact_alias.rose`, `exact_idxneg.rose`, `exact_gros.rose`, 4 programmes
  `exact_coh1..4.rose` (listes hétérogènes entiers/chaînes/rationnels/booléens,
  index négatif, `$l[0]=80`, égalité, alias, `sous_liste`, valeurs > 2²⁴ et
  9 007 199 254 740 993), `r_list.rose`, `d_list_10k.rose`, `d_idx.rose`.
  Pour les 3 programmes chronométrés, seule la ligne de temps est exclue de
  la comparaison ; la valeur calculée (`4999950000` / `49995000`) est
  comparée octet pour octet.
- **Cohérence JIT ↔ interpréteur** : chaque programme est joué en mode JIT
  et avec `ROSE_SANS_JIT=1`, sur les deux binaires. Aucune divergence de
  résultat. Le programme du bench donne `4999950000` dans les deux modes
  (0,91 ms JIT contre 38,8 ms interprété).
- `cargo test --release` : **160/0, 144/0, 0/0, 6/0, 1/0, 0/0**, rc=0
  (les `debug_assert!` des helpers sont donc exercés en debug).
- `bash scripts/audit-final.sh` : **rc=0** — oracles **152/0 ×4**, point fixe
  `bdbef63681608a0b22a5637539e9d738` **×2** (rejoué directement), jardin
  **20/20**, fib(25) = **75025**, bench_global rc=0, `link_natif` 7/0,
  `natif_emetteur` 6/0, `bin rose` 144/0.
- `git diff` : **un seul `.rs`** (`jit.rs`), aucun instrumentation
  temporaire, aucun commit, aucune publication.

---

## 5. Bilan honnête

**4ᵉ victoire nette : NON.** La prémisse « transposer TI1 aux listes » était
partiellement juste : les trois ingrédients mesurés de TI1 (accès sans clone,
réservation de capacité, accès `RefCell` vérifié) fonctionnent et donnent
**×2,26**, avec une cohérence structurelle plus simple que celle de D7 (une
seule représentation au lieu d'un hachage partagé). Mais l'écart initial était
de ×10,9, pas de ×2,7 : après le gain, Rose reste **4,8-4,9× derrière Rust**
dans la même salve. La cause est mesurée et expliquée en §3 : aux ~200 000
appels runtime s'ajoutent deux boucles `pour` à 1,28 ns/it. La porte demandait
une réduction supplémentaire de ×4,8 que la recette des helpers ne peut pas
fournir. Deux prémisses ont été corrigées en cours de route (le « joindre isolé
» n'était pas JIT-qualifié ; un tag sur le bit 0 casse D5). Constat daté,
pas consigne.

## 6. Artefacts

- Harnais : `/tmp/opencode/listi64/decomp.py`, `dec_join_variants.py`,
  `exact.py`, `ab_other.py`, `summarize.py`, `bench_env.py`.
- Décomposition : `decomp_base.log`, `decomp_varA.log`, `decomp_varB.log`,
  `decomp_varC.log`, `decomp_final.log`, `dec_join_final.log`.
- Salves : `bench_base_c19.log`, `bench_base_c15.log`,
  `bench_final_c19A.log`, `bench_final_c19B.log`, `bench_final_c15C.log`
  + `*_summary.json` / `*_results.csv`.
- Binaires variantes : `base_rose`, `base_rebuilt`, `varA_rose`, `varB_rose`,
  `varC_rose`, `varC2_rose` (= final), `varE_rose`.
- Exactitude : `exact_all.log`, `cargo_test_release.log`, `audit_final.log`,
  `ab_other.log`.
