# OPT_TABLES — élimination du goulot « tables »

**Date :** 22/11/2026 · **Worktree :** `/tmp/opencode/opt_042` (branche `opt-042`,
base `e14b601`) · **Aucun commit, aucun push, aucun fichier touché hors périmètre.**

## 1. Contexte — ce que le doc disait, ce que le code faisait

`rose/docs/BENCH_MULTI.md` (§7, ligne 143) décrit encore `table()` comme une
liste d'association à scan linéaire (`Rc<RefCell<Vec<(Valeur,Valeur)>>>`) et
annonce **×1 946** contre `HashMap` Rust sur 3 000 insertions + 3 000
recherches. Cette description est **périmée** : depuis le 18/11, `TableRep`
porte un index de hachage (`IndexRapide`) et une partie tableau dense
(`array_part`), et `BENCH_MULTI.md` n'a pas été régénéré.

Le banc ci-dessous mesure donc **l'état réel du code à `e14b601`**, pas le
chiffre historique du document. Il a révélé un défaut latent du hachage qui
rendait précisément le chemin des clés entières **quadratique**.

## 2. Root cause — un seul bucket pour 3 000 clés

`h_entier(k) = bits(f64(k)).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x243F_6A88_85A3_08D3`.
Pour un petit entier, la mantisse de `k as f64` est alignée à droite : les
~50 bits de poids faible du produit sont nuls, donc **`h & (cap-1)` est
constant**. Vérification indépendante (Python, réplique exacte de la formule) :

```
mask 0x1fff (8192 buckets) : 1 bucket distinct pour k = 0..2999, occupance 3000
h(0..5) & 0x3fff = 0x8d3, 0x8d3, 0x8d3, 0x8d3, 0x8d3, 0x8d3
```

`IndexRapide` (adressage ouvert) indexait par `h & m` **sans avalancher**,
croyant le hachage déjà mélangé. Résultat : sonde linéaire de ~n/2 entrées à
chaque insertion → 0,54 ms à 1 k, 4,70 ms à 3 k, **60,4 ms à 10 k**.

Note : le finaliseur correct existait **déjà** dans `HashU64::write_u64`
(pour les `HashMap<u64,_>` de std), mais n'était pas appliqué à l'index
propre de `Table`. D'où divergence entre les deux voies.

## 3. Correctif (minimal, ABI inchangée)

`rose/src/run.rs` uniquement :

1. extraction du finaliseur en `#[inline] fn melange_u64(h: u64) -> u64`
   (un décalage, un xor, une multiplication, un xor) ;
2. `IndexRapide::get` et `IndexRapide::inserer` indexent par
   `melange_u64(h) & m` au lieu de `h & m` ;
3. `HashU64::write_u64` appelle la même fonction → **source unique**, les
   deux voies restent cohérentes (une clé insérée par une voie est trouvée
   par l'autre) ;
4. deux tests de garde-fou dans `run.rs` :
   `melange_u64_repartit_les_petits_entiers` (aurait échoué avant : 1 bucket)
   et `coherence_index_entier_et_general` (voie JIT `inserer_i64` ↔ voie
   générale).

`Valeur`, `Table`, `TableRep` : **aucun changement de layout**. Le helper JIT
`rose_jit_table_*` lit le même `Rc<RefCell<TableRep>>` et les mêmes champs.
**Aucun patch `jit.rs` n'est nécessaire** (contrairement au scénario
« bloqué par l'ABI » envisagé dans la consigne).

`cles()` / `valeurs()` lisent toujours `TableRep::items` dans l'ordre
d'insertion : la sémantique est strictement identique (vérifié par
`semantique_table_preservee` dans le nouveau test d'intégration).

## 4. Banc reproductible

`rose/tests/opt_tables.rs` — 3 échantillons par point, médianes, `HashMap`
Rust compilé dans **le même binaire de test**. Deux familles de clés :
`entier` (`0..n`, voie `array_part` + index) et `chaine` (`"k0".."kn"`, voie
hachage FNV). Pic RSS lu dans `VmHWM` de `/proc/self/status`.

```bash
cd /tmp/opencode/opt_042/rose
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR=/tmp/opencode/opt_shared_target
bash ../scripts/garde_fou_mem.sh 12000000 \
  cargo test --release --test opt_tables -- --nocapture bench_tables
```

### Avant (code `e14b601`, avant correctif) — médianes de 3

| n | clés | insert ms | recherche ms | total ms | Rust HashMap ms | Rose/Rust |
|---:|---|---:|---:|---:|---:|---:|
| 1 000 | entier | 0,5389 | 0,0051 | **0,5440** | 0,0261 | 20,83 × |
| 3 000 | entier | 4,6984 | 0,0151 | **4,7135** | 0,0678 | 69,55 × |
| 10 000 | entier | 60,4337 | 21,5426 | **81,9763** | 0,2354 | 348,26 × |
| 1 000 | chaîne | 0,0521 | 0,0624 | 0,1145 | 0,1277 | 0,90 × |
| 3 000 | chaîne | 0,1599 | 0,1890 | 0,3489 | 0,3697 | 0,94 × |
| 10 000 | chaîne | 0,5996 | 0,6566 | 1,2562 | 1,2485 | 1,01 × |

RSS : 1 592 Kio au départ → 3 668 Kio (VmHWM).

Référence secondaire, liste d'association linéaire Rust (l'ancien
`table()`) : 0,134 / 1,179 / 12,900 ms pour 1 k / 3 k / 10 k — la
structure de hachage existante battait déjà le scan linéaire sur les clés
chaîne, mais **pas** sur les clés entières à cause du bucket unique.

### Après (correctif) — médianes de 3

| n | clés | insert ms | recherche ms | total ms | Rust HashMap ms | Rose/Rust | gain |
|---:|---|---:|---:|---:|---:|---:|---:|
| 1 000 | entier | 0,0163 | 0,0050 | **0,0213** | 0,0268 | 0,79 × | **25,5 ×** |
| 3 000 | entier | 0,0256 | 0,0153 | **0,0409** | 0,0671 | 0,61 × | **115,2 ×** |
| 10 000 | entier | 0,2122 | 0,0935 | **0,3057** | 0,2350 | 1,30 × | **268,2 ×** |
| 1 000 | chaîne | 0,0532 | 0,0645 | 0,1177 | 0,1264 | 0,93 × | ~1 × |
| 3 000 | chaîne | 0,1657 | 0,1990 | 0,3647 | 0,3652 | 1,00 × | ~1 × |
| 10 000 | chaîne | 0,6343 | 0,7073 | 1,3416 | 1,2502 | 1,07 × | ~1 × |

RSS : 1 600 Kio au départ → 3 556 Kio (VmHWM), soit **112 Kio de moins**
qu'avant à charge égale.

`Rose/Rust < 1` signifie que Rose bat le `HashMap` std. Le gain demandé
(≥ 10 ×) est dépassé de 2,5 × à 27 × selon la taille. Les clés chaîne sont
restées au coude-à-coude avec `HashMap` (écart ≤ 10 %, dans le bruit de
mesure de la machine partagée).

Trois rejeux indépendants du banc après correctif donnent, sur la colonne
`total ms` entier : 0,0214 / 0,0390 / 0,3063 ; 0,0329 / 0,0757 / 0,5495
(salve perturbée par des compilations simultanées) ; 0,0198 / 0,0405 /
0,3104. Le gain reste ≥ 20 × dans toutes les salves.

## 5. Validation

Commande (copie isolée du script avec logs privés, pour éviter que les
autres sous-agents n'écrasent `/tmp/opencode/vf_*.log` — le script original
a aussi été exécuté tel quel) :

```bash
cd /tmp/opencode/opt_042
export PATH="$HOME/.cargo/bin:$PATH" CARGO_TARGET_DIR=/tmp/opencode/opt_shared_target
bash scripts/garde_fou_mem.sh 12000000 bash scripts/validation_finale.sh
```

Résultat final retenu (`/tmp/opencode/opt_042/vlogs/validation_finale.log`,
salve isolée avec binaire et répertoires temporaires privés) :

```
OK tests rc=0
  lib 313 passés / 0 échec · bin 326 passés / 0 échec
  + 6 + 9 + 8 + 2 + 1 + 2 tests d'intégration → 667 passés, 0 échec
OK point fixe        b3ed9262dd54776a6dbd07bc12368d0e (identique des deux côtés)
OK AOT 75/75         75 OK, 0 ECHEC
OK aucun refus AOT
OK JIT 25/25
OK WASM 92 o · RVM 317811
OK AZ 31/31
OK HOST-1 fib=317811
RESULTAT: 0 échec(s)
```

Deux aléas de mesure rencontrés et résolus, **sans lien avec le correctif** :
`link_natif::tests::elf_avec_valeur_exacte_ecrit_le_texte_memorise` a rendu
`Text file busy` une fois sous `cargo test` parallèle (passe seul et dans la
salve finale) ; les logs `/tmp/opencode/vf_*.log` et le répertoire
`/tmp/aot8-verify` étant partagés par plusieurs sous-agents, une salve a lu
des résultats d'un autre chantier (d'où la copie isolée ci-dessus).

## 6. Fichiers modifiés

- `rose/src/run.rs` — `melange_u64`, 2 points d'appel dans `IndexRapide`,
  `HashU64::write_u64`, 2 tests de garde-fou, commentaires de root cause.
- `rose/tests/opt_tables.rs` (nouveau) — banc 1 k/3 k/10 k + médianes +
  RSS + non-régression sémantique.
- `rose/docs/OPT_TABLES.md` (nouveau) — ce document.

Non modifiés : `jit.rs` (aucun patch requis), `interp.rs`, `mem_modele.rs`,
`Cargo.toml`, aucun binaire versionné, aucun commit/tag/publish.

## 7. Ce qui reste

- Clés chaîne : Rose ~= `HashMap` std (0,93–1,07 ×). Le finaliseur ajoute
  une multiplication par sonde ; le gain n'y est pas mesurable car FNV-1a
  distribue déjà correctement ses bits bas. Aucune action requise.
- 10 000 clés entières : Rose 1,30 × Rust HashMap. Les clés ≥ `N_ARRAY`
  (4096) passent par l'index haché en plus de `array_part` ; on pourrait
  économiser cette insertion redondante, mais le bénéfice attendu est
  < 1,3 × et le risque sémantique (égalité hétérogène Entier ≡ Flottant ≡
  Rationnel) n'en vaut pas la peine ici.
- `BENCH_MULTI.md` n'a pas été régénéré (hors périmètre) : son chiffre
  × 1 946 doit être lu comme historique, pas comme l'état courant.
