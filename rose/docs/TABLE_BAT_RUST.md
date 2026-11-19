# Domaine 7 — battre Rust sur la table 3 000 clés

**Constat daté : 19/11/2026, 17:25Z → 19:0xZ UTC.** Toutes les mesures ci-dessous
sont des constats à cet horodatage, pas des consignes : re-mesurer après tout
changement de moteur, de JIT, de source de bench ou de topologie. Pinage
`taskset -c 6` (cœur X925, `scaling_governor=performance`, 3,90 GHz mesuré),
A/A/A entrelacé par rotation de l'ordre, médianes de 9 à 21 échantillons,
résultat `3000` vérifié à chaque tirage, aucun échantillon rejeté.

**Périmètre**：`rose/src/run.rs` **uniquement** (+49/−3 lignes) + ce document
(nouveau). Aucun autre `.rs`, aucun `Cargo.toml`, aucun `PLAN.md`, aucun doc
d'un autre agent, aucun commit, aucune publication. **`rose/rose` n'a PAS été
remplacé** : tous les binaires sont construits dans `/tmp/opencode/ta2_agents/`
avec `CARGO_TARGET_DIR=/tmp/opencode/ta2_agents/target`. Les modifications non
commitées d'autres agents présentes dans l'arbre (`compilateur/emetteur_elf.rose`,
`rose/docs/G5_ABI_VALEURS.md`) n'ont pas été touchées.

| binaire | sha256 |
|---|---|
| `rose_BASE` (HEAD `8c17076`, `run.rs` d'origine) | `c261a7a4c16fc5685ed90f08a4b7535d36ef40bdd66daaa2518217d2bb220f34` |
| `rose_FINAL` (hybride) | `32e015c29db1c0440093e420d33129ae38f4c6ec80f2fb0be077be7146f7a599` |
| `multi_rs` (Rust `-O`, référence inchangée) | `9ff9d70d4489344ac6c1d10a58f73d2d400fb2509337ba734ca2d4ed01e0de6f` |

Source bench : `/tmp/opencode/benchmulti/r_table.rose` — **inchangée** (3 000
`inserer($t,$i,$i*2)` puis 3 000 `cle_existe($t,$j)`, `>> $c`, `>> $f-$d`).
Rust : `HashMap<u64,u64>` avec hacheur par défaut, `multi.rs:155-157`.

---

## 1. Reproduction et décomposition (étape 1)

### 1.1 Reproduire l'écart

A/A/A entrelacé serré (3 processus par répétition, aucun travail intercalé),
c6, 11 échantillons, `/tmp/opencode/ta2_agents/aaa_base_c6.log` :

| A1 (base) | Rust même salve | A2 (base, contrôle) | A2/A1 | Rust/A1 |
|---:|---:|---:|---:|---:|
| 0,1032 ms | 0,0962 ms | 0,1044 ms | 1,012 | 0,932 |

Bruit A/A mesuré ≈ 1,2 % sur les médianes (dispersion individuelle jusqu'à
30 % — d'où 9 à 21 échantillons et des médianes, jamais un tirage isolé).

### 1.2 Décomposition par phase

`decomp_ab.py`, A/B entrelacé, 21 échantillons, c6, programmes
`/tmp/opencode/d7/d7_{ins,look}_only.rose` (la création de table est **hors**
zone chronométrée dans ces deux programmes) :

| phase | BASE | HYBRIDE | HYBRIDE/BASE |
|---|---:|---:|---:|
| 3 000 insertions seules | 0,0595 ms | 0,0618 ms | 1,039 |
| 3 000 recherches seules | 0,0251 ms | 0,0168 ms | **0,669** |
| programme complet (création incluse) | 0,0921 ms | 0,0877 ms | 0,952 |

Le programme avec phases internes (`r_table_phases.rose`, 21 échantillons
entrelacés) donne BASE 0,0604 + 0,0253 ms, HYBRIDE 0,0632 + 0,0170 ms.

**Goulot #1 = la voie d'insertion** : 0,0604 ms sur 0,0857 ms de calcul mesuré
= **70 %** ; les recherches ne pèsent que 30 %. C'est donc l'insertion qu'il
faut attaquer en priorité — mais c'est précisément elle qui résiste à toutes
les variantes testées (§2.3), tandis que la recherche recèle le seul gain
structural disponible.

### 1.3 Décomposition par « instrumentation retirée »

Chaque variante est un binaire reconstruit en patchant **uniquement** `run.rs`,
puis `run.rs.BASE` est restauré immédiatement. Toutes les mesures sont
entrelacées avec BASE et Rust dans la même salve. Les pourcentages sont
`variante / BASE − 1` (positif = plus lent).

| variante (ce qui est retiré/modifié) | salve | BASE | variante | écart | décision |
|---|---|---:|---:|---:|---|
| `addref` — **ajoute** un vrai `borrow()` avant l'accès unsafe | 11 | 0,0980 | 0,1025 | **+4,6 %** | l'accès sans contrôle vaut 4,6 % (déjà retenu en HEAD) |
| `noreserve` — supprime `reserve(4096)` | 11 | 0,0980 | 0,1409 | **+44 %** | réservation indispensable (déjà retenue) |
| `nocollision` — supprime la boucle `collisions` de `trouve_i64` | 11 | 0,0980 | 0,0969 | −1,1 % | gain marginal, casse la sémantique → rejeté |
| idem, 2ᵉ salve | 11 | 0,1081 | 0,1030 | −4,7 % | |
| `inthash` — `h_entier` sans conversion f64 | 11 | 0,0980 | 0,0981 | +0,1 % | **aucun gain** → rejeté |
| idem, 2ᵉ salve | 11 | 0,1081 | 0,1097 | +1,5 % | |
| `noguard` — supprime les gardes `z_est_nu` | 11 | 0,0980 | 0,1018 | +3,9 % | **ralentit** → conservés |
| `noval` — valeur stockée `Valeur::Vide` | 11 | 0,0980 | 0,0981 | +0,1 % | aucun gain → rejeté |
| `collguard` — `if !collisions.is_empty()` | 15 | 0,0944 | 0,0964 | +2,1 % | rejeté |
| `finF1` — finaliseur `i*C ^ (i>>32)` | 15 | 0,0944 | 0,3359 | **+256 %** | rejeté |
| `finF2` — finaliseur `(i^(i>>33))*C` | 15 | 0,0944 | 0,2290 | **+143 %** | rejeté |
| `finF3` — finaliseur `i*C` seul | 15 | 0,0944 | 1,4222 | **+1 406 %** | rejeté |
| `hraw` — h canonique = `k as u64` | 15 | 0,0944 | 0,1018 | +7,8 % | rejeté |
| `hrawmix` — h canonique = `k ^ D` | 15 | 0,0944 | 0,1001 | +6,0 % | rejeté |
| `umulhB9` / `umulhC` / `umulhB9xor` — finaliseur en 1 `umulh` | 15 | 0,0930 | 0,0952 / 0,0966 / 0,0959 | +2,4 / +3,9 / +3,1 % | rejetés |
| `open` — index plat à adressage linéaire | 15 | 0,0961 | 0,1088 / 0,1078 | +13 % | rejeté |
| `cap8192` — réserver 8 096 buckets | 15 | 0,0937 | 0,1166 | +24 % | rejeté |
| `cap3000items` — réserver 3 000 items | 15 | 0,0937 | 0,0952 | +1,6 % | rejeté |
| `unchecked` — `spare_capacity_mut` + `set_len` | 15 | 0,0937 | 0,0981 | +4,7 % | rejeté |

Contrôle A/A dans la salve `umulh` : `BASE2/BASE = 1,006` — les écarts de
2–4 % sont donc au-dessus du bruit, mais les rejets sont de toute façon
sans gain.

**Cause chiffrée des rejets.** Les finaliseurs `finF1/F2/F3` dégénèrent parce
que les bits HAUTS du h canonique `(k as f64).to_bits()*C ^ D` sont presque
constants sur `k ∈ [0,3000)` (ex. `k ∈ [1024,2048)` a le même exposant) :
sans avalanche `i^(i>>29)` puis `^ (z>>32)`, les 3 000 clés se retrouvent dans
quelques buckets. Simulation Python indépendante sur le h canonique réel :
finaliseur actuel = 2,59 sondes moyennes/bucket de 4 096 (max 37) ; `umulh` =
1,74 (max 27) ; `lowmul` = 1 500 (max 3 000). `umulh` a pourtant **ralenti**
la mesure réelle de 2–4 % : la simulation linéaire ne modélise pas les
control-bytes/h2 de hashbrown, qui restent sensibles aux bits hauts.

---

## 2. Décision structurale (étape 2)

### 2.1 Une seule décision retenue : partie tableau directe (style Lua)

`TableRep` gagne un champ `array_part: Vec<u32>` de `N_ARRAY = 4 096` entrées
(16 Kio). Pour une clé i64 nue `k ∈ [0, 4096)`, `array_part[k] = position + 1`
(0 = absent). La voie rapide `contient_i64` consulte d'abord cette case :
si elle est non nulle **et** que `cle_i64_egale(&items[p-1].0, k)` est vrai,
la clé est présente — retour immédiat, sans hachage, sans sonde `HashMap`.
Sinon, repli intégral sur l'index haché existant.

Propriétés vérifiées, pas supposées :

- **Aucun faux positif** : chaque hit direct est validé contre l'item réellement
  stocké. Un `5/1` ou `5.0` inséré par la voie générale n'écrit pas `array_part` ;
  une recherche de `5` tombe donc sur le hachage canonique, qui le trouve.
- **Cohérence** : `inserer_i64` tient la case à jour à chaque insertion/mise à
  jour ; `reindexer` et `retirer` reconstruisent la partie tableau (décalage des
  positions). `items` + `index` restent la **source de vérité** : c'est un
  accélérateur pur, pas un second index divergent.
- **Généralité** : hors `[0,4096)`, le comportement est exactement l'ancien.
  Le coût est 16 Kio par table, documenté explicitement (cf. le compromis
  déjà assumé de ~230 Kio/table pour `reserve(4096)`).

### 2.2 Effet mesuré

| phase (21 échantillons entrelacés c6) | avant | après | rapport |
|---|---:|---:|---:|
| 3 000 insertions | 0,0595 ms | 0,0618 ms | 1,039 |
| 3 000 recherches | 0,0251 ms | 0,0168 ms | **0,669** |
| programme complet | 0,0921 ms | 0,0877 ms | **0,952** |

Le gain vient donc uniquement de la recherche (−34 %), l'insertion payant
+4 % pour tenir la case directe à jour. C'est le contraire du goulot #1 (§1.2)
: l'insertion est structurellement déjà au plancher (toutes les variantes
§1.3 la détériorent), la recherche était le seul endroit encore attaquable.

### 2.3 Rejets structuraux (cause chiffrée)

- **Index plat à adressage linéaire** (`open`) : +13 %. Le bitmap
  d'occupation + sonde logique coûte plus cher que les control-bytes SIMD de
  hashbrown, malgré 1,74 sonde simulée contre 2,59.
- **Réservation plus grande** (`cap8192`) : +24 % — le doublement de
  l'empreinte (68 → 136 Kio) sort du cache. `reserve(4096)` est le point
  optimal mesuré.
- **Push non vérifié** (`unchecked`) : +4,7 % — la branche de capacité
  prévisible de `Vec::push` est moins chère que la logique `spare_capacity_mut`.
- **Toutes les variantes de hachage** (§1.3) : 0 % à +1 406 %. Le hacheur et
  le finaliseur actuels sont un optimum local très étroit ; les « simplifier »
  détruit la distribution h2.

---

## 3. Résultat (étape 3) — porte atteinte en salve serrée

Trois salves indépendantes, **9 échantillons chacune**, c6, entrelacement par
rotation `BASE → R → FINAL → FINAL2(contrôle)` ; `FINAL2` est le **même
binaire** que `FINAL` (contrôle A/A).

| salve | BASE | Rust | FINAL | FINAL2 | FINAL/Rust | FINAL2/Rust | FINAL/BASE |
|---|---:|---:|---:|---:|---:|---:|---:|
| s1 | 0,0921 | 0,0936 | **0,0877** | 0,0875 | **0,937** | 0,935 | 0,952 |
| s2 | 0,0930 | 0,0942 | **0,0884** | 0,0887 | **0,938** | 0,942 | 0,951 |
| s3 | 0,0944 | 0,0940 | **0,0910** | 0,0922 | **0,968** | 0,981 | 0,965 |

**Rose bat Rust dans les six comparaisons (3 salves × 2 copies), ratio
0,935–0,981 < 1.** Gain médian sur BASE ≈ 4,5 % ; meilleur temps individuel
observé 0,0845 ms (s1). Le contrôle A/A `FINAL/FINAL2` = 0,998–1,013 confirme
que l'écart n'est pas du bruit.

### 3.1 Artifact de mesure — honnêteté absolue

Le harnais multi-domaines (`bench_sp1.py`, 6 implémentations entrelacées,
dont `Python list assoc` à 83 ms) **insère des trous d'environ 100 ms entre
deux exécutions Rose**. Or cet espacement ralentit les deux moteurs, mais
Rose davantage (mesure dédiée, c6, 11 échantillons) :

| protocole | BASE | FINAL | Rust |
|---|---:|---:|---:|
| serré (aucun trou) | 0,0944 | 0,0908 | 0,0967 |
| 100 ms de trou entre tirs | 0,1168 | 0,1137 | 0,1047 |

L'effet est **antérieur à mon changement** (BASE perd 24 % sous trous, FINAL
25 %) : il n'est donc pas imputable à `array_part`. Le harnais multi donne en
conséquence D7 Rose 0,100–0,104 ms contre Rust 0,095–0,096 ms (Rose/Rust ≈
1,04–1,09), alors que la salve D7 serrée donne 0,94–0,98. Les deux constats
sont rapportés ; c'est le protocole serré, sans travail intercalé, qui est
utilisé pour la porte, conformément à la consigne « même salve que Rust ».

---

## 4. Tableau 9 domaines (salve multi complète, c6, 9 échantillons)

`ta2_multi_c6c.log` / `ta2_multi_summary.json`, 19/11 17:55Z, binaire
`rose_FINAL` sha `32e015c2…`. Temps internes médians, ms. Rapport = Rose/Rust
(`< 1` = Rose gagne). `ta2_multi_c6.log` et `c6b.log` donnent les mêmes
verdicts à 1–3 % près.

| # | domaine | Rose | Rust `-O` | Rose/Rust | verdict source |
|---|---|---:|---:|---:|---|
| 1 | fib(35) | 11,647 | 12,701 | **0,917** | gagné |
| 2 | boucle 1e6 | 0,101 | 0,263 | **0,384** | gagné |
| 3 | premiers 50k | 0,854 | 1,002 | **0,852** | gagné |
| 4 | fact 5 000 | 1,429 | 6,543 | **0,218** | gagné |
| 4b | PGCD 1 000 ch | 0,103 | 0,147 | **0,701** | gagné |
| 5 | chaîne 20k | 0,010 | 0,015 | **0,667** | gagné |
| 6 | liste 100k | 0,395 | 0,194 | 2,036 | perdu |
| 7 | **table 3 000** | **0,104** | **0,095** | **1,088** | perdu *dans ce harnais* (cf. §3.1), gagné en salve serrée (§3) |
| 8 | E/S 10 Mo | 4,546 | 7,338 | **0,620** | gagné |

Aucune régression systématique attribuable à mon changement : les domaines 1–6
et 8 ne touchent pas au chemin `Table` modifié, et leurs temps sont ceux du
HEAD courant (d'autres agents ont optimisé D1/D2/D3/D5/D6/D8 depuis
`BENCH_MULTI.md` du 18/11). Le seul domaine concerné, D7, est vérifié par
l'exactitude (§5) et par la décomposition (§1.2).

---

## 5. Double verdict (consigne utilisateur)

| verdict | protocole | résultat |
|---|---|---|
| **Même source / même algorithme** | `r_table.rose` identique, table = `Vec<(Valeur,Valeur)>` + `HashMap<u64,usize>` avec hacheur dédié, aucune partie directe | **à égalité** : `BASE/Rust` = 0,984–1,004 sur les 3 salves serrées de décision (1,073 sur la première salve de reproduction, plus bruitée : `Rust/A1 = 0,932`). La porte n'est **pas** franchie par ce seul chemin. |
| **Meilleur algorithme** | même source Rose, moteur doté d'une partie tableau directe (style Lua) pour les petites clés i64, index haché conservé comme source de vérité | **gagné** : `FINAL/Rust` = 0,935–0,981 sur 3 salves × 2 copies. |

Le gain est donc un gain **d'algorithme interne**, pas un gain de langage ni
une modification du programme de bench. Rust reste libre d'implémenter la même
partie directe ; le rapport ne prétend pas que `HashMap` est battu « à armes
égales ».

---

## 6. Validation

Toutes les commandes ont été exécutées le 19/11/2026 sur le binaire
`/tmp/opencode/ta2_agents/rose_FINAL` ; `rose/rose` du dépôt n'a pas été
touché.

- **`cargo build --release --bin rose`** (`CARGO_TARGET_DIR` isolé) : rc=0,
  `Finished release profile in 9,30s`. Binaire final sha `32e015c2…`, **identique**
  au binaire de la variante `rose_hybrid` (build reproductible).
- **`cargo test --release`** : `220 pass / 0 fail` (3 ignored) + `204 / 0`
  (3 ignored) + `6 / 0` + `1 / 0` = **431 tests réussis, 0 échec**,
  rc=0 (`cargo_test.log`).
- **Exactitude des tables** : `exact_check.py` — `rose_BASE` (HEAD) contre
  `rose_FINAL`, **19 programmes × 2 modes (JIT et `ROSE_SANS_JIT=1`) = 0
  différence octet pour octet**, dont `table_exactitude.rose` **43/43 sorties**
  et `table_stress.rose` **8/8**, plus `jit_table_eq/exact/neg/ratkey/strkey/
  update`, `jit_path_*` et `coh1..coh4` (cohérence JIT ↔ interpréteur,
  `5`, `5/1`, `5.0`, limites ±2²⁴, suppression, mise à jour).
- **Jardin** : `rose_FINAL jardins/tests/premiers.rose` → `── jardin/premiers :
  20 tests, 0 échec ──` (**20/20**).
- **fib(25)** : **75025**.
- **Point fixe** : `rose rir compilateur/emetteur.rose` et
  `rose compilateur/emetteur.rose compilateur/emetteur.rose` → md5
  `8ac53eed3d4967a0d7d9ab333256801c` **×2**, égaux.
- **`audit-final.sh`** lancé avec le binaire isolé : oracles M4.1
  **174 match / 0 écart** et M4.2 **174 / 0** avant que le script ne dépasse
  le délai de 600 s fixé (il recompile les composants avec le `target/` par
  défaut). Les points vérifiés séparément ci-dessus (jardin, fib, point fixe)
  sont tous conformes ; le dépassement de délai est un constat, pas un échec
  de validation.
- **9 domaines** : salve complète c6, 9 échantillons, rc=0 sur tous les
  tirages (§4). `bench_global.rose` et les suites `link_natif` /
  `natif_emetteur` n'ont pas été relancées ici (elles utilisent le `target/`
  par défaut et échouent avec rc=101 dans l'état courant de l'arbre, qui
  contient les changements non commités d'autres agents) — **non prouvé**,
  dit explicitement.

---

## 7. Bilan

**Objectif atteint sous réserve du double verdict.** Le goulot #1 chiffré est
l'insertion (70 % du calcul), mais elle est au plancher mesuré : 13 variantes
d'insertion/hachage/réservation ont toutes été rejetées par la mesure, certaines
avec une dégradation de 1 406 %. La seule décision structurale gagnante est une
partie tableau directe de 4 096 entrées qui divise le coût de la recherche par
1,5 et fait passer le domaine 7 de 1,0× à 0,94–0,98× Rust en salve serrée.
En salve multi avec trous de 100 ms, le même binaire reste à 1,04–1,09× —
artifact mesuré, présent aussi sur le binaire de départ. Constat daté, pas
consigne.

### Artefacts

- Harnais : `/tmp/opencode/ta2_agents/aaa.py`, `decomp.py`, `decomp_ab.py`,
  `phases_ab.py`, `exact_check.py`, `ta2_bench_multi.py`.
- Salves décision : `aaa_decision_s{1,2,3}.log`, `aaa_final_c6.log`,
  `aaa_hybrid_c6.log`, `aaa_hybrid_c6_big.log`.
- Décomposition / rejets : `aaa_decomp_c6{,b}.log`, `aaa_hash_c6.log`,
  `aaa_umulh_c6.log`, `aaa_open_c6.log`, `aaa_cache_c6.log`.
- 9 domaines : `ta2_multi_c6.log`, `ta2_multi_c6b.log`, `ta2_multi_c6c.log`,
  `ta2_multi_summary.json`.
- Binaires variantes : `rose_{BASE,addref,noreserve,inthash,nocollision,noguard,
  noval,collguard,finF1,finF2,finF3,hraw,hrawmix,umulhB9,umulhC,umulhB9xor,
  open,cap8192,cap8192both,cap3000items,unchecked,hybrid,FINAL}`.
- Validation : `cargo_test.log`, `exact_check.log`, `audit_final.log`.
