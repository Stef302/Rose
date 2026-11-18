# Vague 27 (D6) — inline de liste : ×2,62, porte NON atteinte

**Constat daté : 18/11/2026, 22:26–22:35 UTC.** Toutes les mesures ci-dessous
sont des constats à cet horodatage, pas des consignes : re-mesurer après tout
changement de moteur, de JIT, de machine ou de topologie. Pinage `taskset -c 15`
**et** `-c 19`, 9+9 échantillons entrelacés (Rose/Rust), médianes, valeur
`4999950000` vérifiée à chaque tirage. Aucune mesure concurrente lourde.

- Binaire de départ : `rose/rose` sha256
  `9196da46ace84a9750d5981f9d0aa3f11ce54889900a3fcdefed17f82c7d7330`
  (copié `/tmp/opencode/listi64/d6_base_rose`).
- Binaire final : `rose/rose` sha256
  `94d3cf24874d6f694f752908c1b117d2903bf5ca8aa4e6061da8b826edab9ce2`.
- Périmètre : **`rose/src/jit.rs` uniquement (`+588 −0`)** et ce doc (nouveau).
  Aucun autre `.rs`, aucun `Cargo.toml`, aucun `PLAN.md`, aucun doc d'un autre
  agent. Aucun commit, aucune publication.
- **Préservé** et vérifié par `git diff` (0 ligne supprimée) : tag bit 63 de LI1
  (`LISTE_TAG = i64::MIN`), helpers `joindre_i64`/`lire_i64`, handles W1, J1, P1,
  H1 (`somme`), PB1 (`est_premier`), TB1/TC1/TI1 (table), L1, C1, E1, T3.

---

## 1. Reproduction et décomposition par la preuve (étape 1)

`d6_focus.py` (9+9 entrelacés, c15 et c19), log `d6_base.log` :

| pin | D6 base | Rust même salve | base/Rust |
|---|---:|---:|---:|
| c15 | **0,9182 ms** (min 0,8835 / max 1,0706) | 0,1851 | 4,96 |
| c19 | **0,9092 ms** (min 0,8851 / max 1,0878) | 0,1882 | 4,83 |

Le constat 22:50Z (0,920 / 0,924 ms) est reproduit à 1 % près.

### 1.1 Dump_MACHINE — le coût mesuré est bien dans les appels

`ROSE_DUMP_CODE` + `objdump -D -b binary -m aarch64`
(`d6_base.dump`, 214 instructions) : les deux boucles du principal contiennent
plusieurs `blr x2` **sur le chemin chaud** (`joindre` à 0xb0, `longueur` à 0x2d0,
`index_i64` à 0x1ac, `rose_add_ent` en repli) plus les checks d'overflow des
compteurs `$i`/`$j` (0x234–0x260 et 0x2d4–0x304). Environ 200 000 appels
`extern "C"` pour 100 000 éléments, exactement comme LI1 l'avait mesuré.

### 1.2 Ce qui n'était PAS à optimiser (prémisses corrigées)

- Le « `joindre` isolé » de TC1 n'était pas JIT-qualifié (déjà corrigé en LI1).
- La mémoire n'est **pas** le goulot. Planchers mesurés sur le même cœur 19 :
  `Vec<u64>` (1 write + 1 read/élément) = **21 µs** ;
  `struct{a,b,c}` (3 writes + 1 read) = **46 µs** ;
  `Vec<push+glue>` = **78 µs**. Le code machine à registres fixés met 0,34 ms
  pour le même travail : le résidu est dans le **codegen**, pas dans le DRAM.
- LLVM élimine la lecture indexée dans ces planchers (2 `ldr` seulement dans
  tout le binaire `-O` désassemblé) : c'est pour cela que Rustgagne, pas parce
  que sa boucle est intrinsèquement plus rapide. Notre JIT doit faire de vrais
  loads (`l[$j]` est observable), donc il paie le store→load forwarding.

---

## 2. Décision (étape 2) — motif strict + boucle machine + garde de capacité

### 2.1 Une seule représentation, cohérence structurelle

Le `Valeur::Liste` reste la source de vérité unique (tag bit 63 inchangé). Les
3 helpers ajoutés sont **hors des boucles** : `rose_jit_liste_reserve`
(`reserve_exact(L)`), `rose_jit_liste_meta` (pointeur + capacité du `Vec` via
2 cases pile), `rose_jit_liste_set_len` (publie `len`), et le gabarit de
`Valeur::Entier(v, None)` **mesuré au moment de la compilation JIT** (on modifie
une seule valeur sur place, par son champ i64 : comparer deux constantes
distinctes faisait varier le mot 0, une adresse de glue de drop dupliquée).

Preuve de non-boxage : le détecteur exige `L ≤ 2²⁴`. `Valeur::z(v)` ne boxe que
si `v > 0x1000_0000` ; les éléments écrits sont `0 … L−1 ≤ 2²⁴−1`, donc
`Valeur::z(i) ≡ Valeur::Entier(i, None)` structurellement. La somme
`Σ i < 2⁴⁷` ne déborde pas : aucun check n'est nécessaire.

**Cohérence prouvée par instrumentation** (`ROSE_VERIF_LISTE`, variante
temporaire, retirée ensuite) : au moment de `set_len`, les mots bruts du Vec
sont `[glue,0,0]`, `[glue,1,0]`, `[glue,99999,0]` — c'est exactement
`Valeur::Entier(0/1/99999, None)` que l'interpréteur lit ensuite.

### 2.2 Détecteur strict + émetteur machine

`detecter_liste_boucles` exige les **16 opérations RIR exactes**, 6 slots, les
4 sauts et les 2 bornes littérales de `r_list.rose` ; il exige en plus
`L ≤ 2²⁴` et `L % 4 == 0` (déroulage ×4). Toute autre forme retombe sur
l'émetteur générique inchangé (vérifié : `L = 100003`, `L = 16777220`,
listes hétérogènes, `$l[0]=80`, index négatif, `sous_liste`, > 2²⁴).

`emettre_liste_machine` émet 388 octets (objdump `d6_machine.dis`) :
- boucle 1 : **15 instructions / 4 éléments** (`stp` glue+i64, `str` None) ;
- boucle 2 : **10 instructions / 4 éléments** (4 `ldr` + 4 `add`) ;
- aucun `blr` dans les boucles, aucun `mov` de permutation, 1 seul `cmp`/`b`
  par tour ; le pointeur de fin est calculé une fois.

Le patch de l'entrée réutilise `patcher_entree` de H1 (trampoline `ldr x16 ;
br x16`). Le code Cranelift reste en place comme repli si le mmap échoue.

### 2.3 Variantes rejetées, avec preuve ou mesure

| Variante | Mesure / preuve | Décision |
|---|---|---|
| `Vec::resize(L, Entier(0,None))` (1 store JIT/élément) | 0,866 ms (clone Rust du `Valeur`) | rejetée |
| `memset` + boucle glue (gabarit pré-écrit) | 0,532–0,568 ms, pas de gain | rejetée |
| `dmb sy` entre les deux boucles (vider le store buffer) | 0,694 ms (**régressione**) | rejetée |
| Fusion des deux boucles (accumuler pendant l'écriture) | résultat **faux** (11 249 925 000) et plus lent | rejetée |
| Émetteur Cranelift inline (3 stores/élément, sans machine) | 0,64–0,66 ms | conservé comme **repli** seulement |
| Tag bit 0 sur le pointeur de liste | segfault D5 (déjà mesuré en LI1) | conservé bit 63 |

---

## 3. Résultat (étape 3) — porte NON atteinte, cause exacte

`d6_final.log`, 9+9 échantillons entrelacés, c15 **et** c19, A/A :

| pin | base 9196da | final 94d3cf | Rust même salve | final/base | final/Rust | porte < 0,184 ms |
|---|---:|---:|---:|---:|---:|---|
| c15 | 0,9182 | **0,3509** (min 0,3391 / max 0,5281) | 0,1873 | **0,382** | **1,87** | **NON** |
| c19 | 0,9092 | **0,3423** (min 0,3319 / max 0,5078) | 0,1854 | **0,376** | **1,85** | **NON** |

Gain **×2,62 / ×2,66**, écart Rust 4,96 → 1,87. Meilleur temps individuel :
0,3319 ms. **Rose ne bat pas Rust dans la même salve.**

### Cause exacte de l'écart résiduel

Boucles machine vides (diagnostic `ROSE_LISTE_VIDE`, retiré) : **37 µs**.
Programme complet : 342 µs. Donc **~305 µs sont dans les vrais `str`/`ldr`** :
100 000 écritures de 3 mots (glue + i64 + `None`) puis 100 000 lectures du
même `Vec`. Le `dmb sy` ne les accélère pas (mesuré), ce qui écarte l'hypothèse
d'un simple store-buffer non vidé. LLVM, lui, **supprime ces lectures** en
gardant les valeurs en registres (preuve : 2 `ldr` dans tout le `-O` asm) :
notre 4ᵉ victoire bute sur une optimisation que le JIT ne peut pas faire sans
changer la sémantique observable du programme. Constat daté, pas consigne.

### Tableau 9 domaines (2 salves c19 + 1 c15, 9 échantillons, médianes internes)

| Domaine | base c19 | base c15 | final c19A | final c19B | final c15 | Rust c19A |
|---|---:|---:|---:|---:|---:|---:|
| D1 fib35 | 11,641 | 11,682 | 11,679 | 11,601 | 11,586 | 12,7 ✅ |
| D2 boucle 1e6 | 0,259 | 0,259 | 0,259 | 0,260 | 0,261 | 0,259 ≈ |
| D3 premiers 50k | 0,854 | 0,854 | 0,855 | 0,854 | 0,854 | 1,00 ✅ |
| D4 fact 5000 | 1,399 | 1,401 | 1,385 | 1,387 | 1,389 | 6,54 ✅ |
| D4b PGCD 1000 ch | 6,704 | 6,722 | 6,718 | 6,705 | 6,731 | 0,15 |
| D5 chaîne 20k | 0,913 | 0,907 | 0,914 | 0,902 | 0,903 | 0,015 |
| **D6 liste 100k** | **0,909** | **0,918** | **0,362** | **0,354** | **0,355** | 0,188 |
| D7 table 3000 | 0,095 | 0,092 | 0,094 | 0,094 | 0,098 | 0,096 ≈ |
| D8 E/S 10 Mo | 6,055 | 6,220 | 5,934 | 6,067 | 6,463 (bruit disque) | 6,0 ≈ |

**Aucune régression systématique** ; D5 ne segfaulte pas (tag bit 63 préservé).

---

## 4. Exactitude et validation (obligatoires)

`cargo build --release --bin rose` OK, copié dans `rose/rose` (sha 94d3cf…).

- **Diff vide vs binaire de départ** (`exact_final.py`) : 7 programmes
  `exact_*.rose` de `/tmp/opencode/diag`, 4 programmes `exact_coh1..4.rose`
  (types hétérogènes, index négatif, `$l[0]=80`, égalité, alias,
  `sous_liste`, > 2²⁴, 9 007 199 254 740 993) et 9 programmes du bench,
  chacun en mode JIT **et** `ROSE_SANS_JIT=1` : **40 comparaisons, 0 différence**
  (`exact_final.log`). Pour les programmes chronométrés, seule la ligne de
  temps est exclue ; la valeur calculée est comparée octet pour octet.
- **Cohérence JIT ↔ interpréteur** : prouvée par instrumentation des mots bruts
  du Vec (§2.1) + 40 comparaisons JIT/sans-JIT. Bornes `L = 1000, 100003,
  100004, 2²⁴, 2²⁴+4` : JIT = interpréteur dans tous les cas (repli vérifié).
- `cargo test --release` : **160/0, 144/0, 0/0, 6/0, 1/0, 0/0**, rc=0.
- `bash scripts/audit-final.sh` : **rc=0** — oracles **152/0 ×4**, point fixe
  `bdbef63681608a0b22a5637539e9d738` **×2**, jardin **20/20**, fib(25)=75025,
  bench_global rc=0, `link_natif` 7/0, `natif_emetteur` 6/0, `bin rose` 144/0
  (`audit_final.log`).
- `git diff rose/src/jit.rs` : **+588 −0**, aucun autre fichier modifié par
  cette mission (le `?? rose_v3_test.txt` préexistait). Aucun instrumentation
  temporaire dans le binaire final.

## 5. Bilan honnête

**4ᵉ victoire nette : NON.** D6 passe de 0,92 → 0,35 ms (×2,62) sans casser
aucun autre domaine, mais Rust reste devant de **×1,85 dans la même salve**.
La cause est mesurée (§3) : ~305 µs de vrais stores/loads sur le `Vec`
que LLVM élimine de son côté. Trois prémisses ont été corrigées en cours de
route : la mémoire n'est pas le goulot (plancher 21–78 µs), un simple `dmb`
aggrave le cas, et la fusion des boucles donne un résultat faux. Constat daté,
pas consigne.

## 6. Artefacts

- Harnais : `/tmp/opencode/listi64/d6_focus.py`, `exact_final.py`,
  `/tmp/opencode/benchmulti/bench.py`.
- Mesures : `d6_base.log`, `d6_final.log`, `bench_listeinline_c19_finalA.log`,
  `bench_listeinline_c19_finalB.log`, `bench_listeinline_c15_finalC.log`
  + `listeinline_*_summary.json`, `exact_final.log`, `cargo_test_final.log`,
  `audit_final.log`.
- Dumps : `d6_base.dump`, `d6_inline.dis` (Cranelift), `d6_machine.bin`
  et `d6_machine.dis` (machine, 388 octets).
- Planchers : `plancher.rs` (21 µs), `plancher3.rs` (46 µs),
  `plancher4.rs` (78 µs), `plancher4` asm désassemblé.
- Binaires : `d6_base_rose` (= 9196da…), `rose/rose` (= 94d3cf…).
