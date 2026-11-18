# Vague 24 (D7) — compteurs de boucle table : PRÉMISSE RÉFUTÉE, gain mesuré ailleurs

**Constat daté : 18/11/2026, 21:38–21:50 UTC.** Toutes les mesures ci-dessous
sont des constats à cet horodatage, pas des consignes : re-mesurer après tout
changement de JIT, de RIR, de machine ou de topologie. Pinage `taskset -c 15`
**et** `-c 19`, 9 échantillons entrelacés, médianes. Aucune mesure concurrente
lourde.

- Binaire de départ : `rose/rose` sha256
  `e89f430fd76fb9a4ae01f4a8c0dd794442758770ae39166dc03e275cf3219fdd`
  (conservé `/tmp/opencode/d7/base_rose_d7c`).
- Binaire final : `rose/rose` sha256
  `d435a7116da259a4edabd16e8e6485441e4413810aa95c6b12d0c8c1d53d82f6`.
- Périmètre : **`rose/src/jit.rs` uniquement (`+17 −6` lignes)** + ce doc
  (nouveau). Aucun autre `.rs`, aucun `Cargo.toml`, aucun doc d'un autre agent.
  Aucun commit, aucune publication de cette mission.

---

## 1. Reproduction (étape 1)

Harnais `/tmp/opencode/d7/aaa_table.py` (base `caa7ba9c` / courant / Rust `-O`
entrelacés, 9 échantillons, valeur `3000` vérifiée à chaque tirage). Le
« base » du tableau est le binaire e89f de la vague 23 avec l'A/B dédié
`/tmp/opencode/d7/aba_base_new.py`.

| pin | e89f (base) | Rust même salve | e89f/Rust | valeur |
|---|---:|---:|---:|---|
| c19 | **0,2665 ms** (min 0,2616 / max 0,2852) | 0,0980 ms | 2,72 | 3000 ✓ |
| c15 | **0,2673 ms** (min 0,2609 / max 0,2956) | 0,0934 ms | 2,86 | 3000 ✓ |

Le constat 21:30Z (0,277 / 0,276 ms, ×2,93) est reproduit à 4 % près.

---

## 2. Décomposition — LA PRÉMISSE EST FAUSSE (étape 2)

### 2.1 RIR exact du principal (`ROSE_DUMP_RIR_JIT`, instrumentation temporaire retirée)

```
noms=[d,t,i,c,j,f] n_slots=6
 0: AffectAst d = temps()
 1: AffectAst t = table()
 2: Affect   i = 0
 3: SautSiFauxAbstrait  i < 3000 → 7
 4: ExprStmt inserer(t, i, i*2)
 5: Affect   i = i + 1
 6: Saut 3
 7: Affect   c = 0
 8: Affect   j = 0
 9: SautSiFauxAbstrait  j < 3000 → 14
10: SautSiFaux  cle_existe(t, j) → 12
11: Affect   c = c + 1
12: Affect   j = j + 1
13: Saut 9
14..16: temps(), >> c, >> f-d
```

Trois additions de compteur passent par le chemin générique `expr_eabstrait_rir`
(∼13 instructions de détection d'overflow + `blr rose_add_ent` sur le chemin
froid chacune) : `$i` (op 5), `$c` (op 11), `$j` (op 12). La prémisse de la
mission (« ∼12 instr/incrément = le goulot ») est **confirmée au niveau du
dump**, mais **réfutée au niveau du chrono** (§2.3).

### 2.2 Dump machine (avant, `/tmp/opencode/d7c_base.dump`)

- 1re boucle (insertions) : test `cmp x0,#0xbb8 ; b.lt 0x298`, corps
  `0x298..0x344` — `blr inserer` puis 13 instructions de check sur `$i+1`
  (`and/cmp/cset/add/eor/eor/and/cmp/cset/and/tst/b.ne`), `blr rose_add_ent`
  sur le chemin lent. **∼24 instr/itération sur le chemin chaud.**
- 2e boucle (recherches) : test `cmp x1,#0xbb8 ; b.lt 0x174`, corps
  `0x174..0x294` — `blr cle_existe`, `cbz`, puis DEUX blocs de check identiques
  (`$c+1` à `0x194..0x1c4`, `$j+1` à `0x210..0x240`). **∼30 instr/itération.**

### 2.3 Élimination des checks — implantée, mesurée, REJETÉE

J'ai implanté la recette H1 complète (spécialisation strictement prouvée,
`compteurs_bornes`) : binaire `7e179591…`. Preuve formelle : test `s < L` avec
`L` littéral `≤ 2^32`, init `s = v0` littérale hors boucle, seules écritures
`s ← s + d` (`1 ≤ d ≤ 4095`) dans le corps de cette boucle, boucles non
imbriquées. Alors `valeur_max(s) = v0 + Σ(L_i·d_i) ≪ i64::MAX` — **`$c` est
borné par L₂ = 3000, pas par L₁·L₂** — et les trois `iadd` sont émis nus.

Dump après (`/tmp/opencode/d7c_new.dump`) : les 3 blocs de check ont disparu.
1re boucle `0x17c..0x1a4` : **∼11 instr/it** ; 2e boucle `0x138..0x178` :
**∼10 instr/it** (∼×2,5 moins d'instructions).

**Résultat A/B/A entrelacé (9+9, mêmes salves) :**

| pin | e89f (checks) | sans checks | ratio | verdict |
|---|---:|---:|---:|---|
| c19 | 0,2692 ms | **0,2832 ms** | 1,052 | **régression 5,2 %** |
| c15 | 0,2693 ms | **0,2834 ms** | 1,052 | **régression 5,2 %** |

Conclusion : les ∼13 instructions de check sont **exécutées dans l'ombre de la
latence du `blr` vers le helper table** (out-of-order). Les supprimer ne libère
aucun temps utile ; le changement d'allocation de registres et de layout qui en
résulte coûte 5,2 %. **La prémisse « c'est le goulot, comme H1 sur `somme` » est
donc fausse ici** : sur `somme`, le chemin chaud n'avait aucun appel runtime
(9 instr/2 itérations) ; ici, 6000 appels runtime dominent.

### 2.4 Vrai goulot (mesuré par isolation, TB1 + plancher)

| Phase | Temps | Par op. |
|---|---:|---:|
| 3 000 insertions seules | 0,214 ms | 71,4 ns |
| 3 000 recherches (table pleine) | 0,071 ms | 23,7 ns |
| 3 000 recherches (table vide) | 0,029 ms | 9,7 ns |
| **plancher ~6 000 appels (table vide)** | **~0,06 ms** | — |
| programme complet | ~0,267 ms | — |

∼0,207 ms (77 %) sont dans `Table::inserer` / `Table::trouver` (`run.rs` :
`vers_d` → hachage canonique, `RefCell::borrow(_mut)`, 3 recherches `HashMap`
par insertion, `valeur_egale`). **Ce fichier est hors périmètre de cette
mission.** La porte < 0,094 ms demanderait ×2,9 sur cette partie, impossible
sans réécrire la table générique (ou la remplacer par un bitmap dense — ce qui
contournerait le benchmark au lieu d'optimiser la boucle, à l'instar de la
forme close `n(n−1)/2` rejetée en H1).

---

## 3. Décision retenue (étape 3) — gain réel mais modeste

J'ai conserve le correctif de la vraie source de surcoût **accessible dans le
périmètre** : `table_du_handle` clonait un `Valeur::Table` (un `Rc`) à chaque
appel, soit 6 000 increments/décréments + 6 000 drops. Le nouveau
`avec_table_ref` emprunte le handle directement (`tab.get(h).and_then(...)`),
sans clone. Sémantique strictement identique (même `Table::inserer` /
`Table::trouver`, même index canonique T1 comme source de vérité).

### Variantes rejetées, avec preuve de mesure

| Variante | Mesure | Décision |
|---|---|---|
| Suppression des checks d'overflow (spécialisation complète prouvée) | **+5,2 %** c15 **et** c19 (A/B/A) | **Rejetée** |
| Idem + déroulage ×2 | Non testé : le sans-déroulage régresse déjà, le déroulage ne peut pas récupérer 5,2 % + gagner ×2,9 | **Non implémenté** |
| Bitmap dense / table parallèle | Hors périmètre (`run.rs`) ou contournement du benchmark | **Rejetée** |
| `rose_appel_interpreteur` en repli | Stack overflow déjà mesuré en H1 (le JIT réintercepte l'appel) | **Rejetée** |
| `avec_table_ref` (emprunt sans clone) | **−6,4 %** c19 **et** c15, reproductible | **Conservée** |

---

## 4. Résultats (étape 4) — porte NON atteinte

D7, 9+9 échantillons entrelacés, A/B direct e89f vs final, pinés c15 et c19 :

| pin | avant (e89f) | après (d435a) | Rust même salve | après/Rust | porte < 0,094 ms |
|---|---:|---:|---:|---:|---|
| c19 | 0,2667 ms | **0,2495 ms** (min 0,2434 / max 0,2628) | ~0,093 ms | **2,7** | **NON atteinte** |
| c15 | 0,2679 ms | **0,2509 ms** (min 0,2482 / max 0,2585) | ~0,093 ms | **2,7** | **NON atteinte** |

Gain **×1,064 / ×1,068** (−6,4 %). **Rose ne bat PAS Rust dans la même
salve** : 0,2495 > 0,093. Meilleur temps observé : **0,2434 ms** (c19).

### Tableau 9 domaines (2 salves c19 + 1 c15, 9 échantillons)

Temps internes Rose, ms. « Avant » = addendum 21:30Z de
`BENCH_APRES_GOULOT.md`. Après = `bench_d7c_varA_c19A/B.log`,
`bench_d7c_varA_c15C.log`.

| Domaine | avant c19 / c15 | c19A | c19B | c15C | Δ c19A |
|---|---:|---:|---:|---:|---:|
| D1 fib35 | 11,60 / 11,62 | 11,546 | 11,667 | 11,732 | −0,5 % |
| D2 boucle 1e6 | 0,259 / 0,258 | 0,260 | 0,260 | 0,260 | +0,3 % |
| D3 premiers 50k | 0,853 / 0,853 | 0,853 | 0,855 | 0,854 | ≈ 0 |
| D4 fact 5000 | 1,43 / 1,44 | 1,431 | 1,428 | 1,427 | ≈ 0 |
| D4b PGCD 1000 ch | 6,71 / 6,71 | 6,723 | 6,739 | 6,720 | +0,2 % |
| D5 chaîne 20k | 0,94 / 0,93 | 0,907 | 0,930 | 0,913 | −3,5 % |
| D6 liste 100k | 2,16 / 2,18 | 2,058 | 2,079 | 2,085 | −4,7 % |
| **D7 table 3000** | **0,277 / 0,276** | **0,260** | **0,255** | **0,262** | **−6,1 %** |
| D8 E/S 10 Mo | 4,83 / 4,95 | 5,936 | 6,131 | 6,164 | bruit disque |

Aucune régression systématique. D5/D6 −3 à −5 % = dérive inter-salves (déjà
observée) : la modification ne touche que les helpers de table. D8 = bruit
disque connu (4,9–6,1 ms sur les salves précédentes). Les 3 victoires
antérieures (D1, D3, D4) et les 2 domaines au niveau (D2, D8) sont conservés.

---

## 5. Exactitude et validation (obligatoires)

`cargo build --release --bin rose`, copié dans `rose/rose` (sha `d435a711…`).
Comparaison **octet pour octet** entre e89f et d435a (`/tmp/opencode/d7c_exact/`) :

- `table_exactitude.rose` : **43/43 sorties identiques** (sha des deux flux
  identique) — clés chaîne, rationnelles, négatives, `5` vs `5/1` vs `5.0`,
  RHS vide, `9007199254740993`, suppression, indexation, égalité de tables.
- `table_stress.rose` : **8/8 sorties identiques**.
- 9 programmes différentiels `/tmp/opencode/d7/` (`jit_table_*`,
  `jit_path_*`, `d7_const_key`) : **résultats identiques** ; seule la 2ᵉ ligne
  (durée `temps()`) diffère, comme attendu pour un benchmark. Les chemins JIT
  (`jit_path_neg`, `jit_path_strkey`, `jit_path_update`) rendent 40 / 50 / 50
  sur les deux binaires.
- Repli `L > 2^32` : inchangé (le garde statique refuse, l'interpréteur prend
  le relais — aucun code nouveau sur ce chemin).
- `cargo test --release` : **160/0, 144/0, 0/0, 6/0, 1/0, 0/0**, rc=0.
- `bash scripts/audit-final.sh` : **rc=0** — oracles **152/0 ×4**, point fixe
  `bdbef63681608a0b22a5637539e9d738` **×2**, jardin **20/20**, fib(25)=75025,
  bench_global rc=0, `link_natif` 7/0, `natif_emetteur` 6/0, `bin rose` 144/0
  (`/tmp/opencode/d7c_audit.log`).
- `git diff rose/src/jit.rs` : **+17 −6**, un seul `.rs` modifié. L'instrumentation
  temporaire `ROSE_DUMP_RIR_JIT` a été retirée (vérifié : `grep` vide).

---

## 6. Bilan honnête

**4ᵉ victoire : NON.** La prémisse fournie était fausse et la mesure l'a
réfutée : supprimer les checks d'overflow des compteurs sur D7 **régressé de
5,2 %**, car les 6 000 appels aux helpers table (∼0,207 ms dans `run.rs`,
hors périmètre) dominent et masquent ces instructions. Le seul gain réel et
sûr dans le périmètre (`+17 −6` lignes) est l'emprunt sans clone `Rc` :
**0,277 → 0,250 ms (×1,11), écart Rust 2,9 → 2,7**, sans aucune régression
d'exactitude ni d'autre domaine. Constat daté, pas consigne.

## 7. Artefacts

- Dumps : `/tmp/opencode/d7c_base.dump` (avant), `/tmp/opencode/d7c_new.dump`
  (sans checks) + leurs `.dis`.
- Binaires variantes : `/tmp/opencode/d7/new_rose_d7c_v1` (sans checks,
  rejeté), `/tmp/opencode/d7/new_rose_d7c_varA` (final).
- Mesures : `/tmp/opencode/d7/{repro_c19,repro_c15,aba_v1_c19,aba_v1_c15,
  aba_varA_c19,aba_varA_c15}.json`, `/tmp/opencode/benchmulti/bench_d7c_varA_*.log`.
- Exactitude : `/tmp/opencode/d7c_exact/`. Audit : `/tmp/opencode/d7c_audit.log`.
