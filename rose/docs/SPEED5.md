# SPEED5 — dernière offensive vitesse : `interp.rs`, dispatch et somme de carrés (CONSTAT DATÉ)

Auteur : sous-agent `speed5`. **Aucun git commit, aucune publication, aucun
`rose/rose` remplacé.** Deux fichiers du dépôt modifiés : `rose/src/interp.rs`
(+136/−1) et ce document. `run.rs`, `jit.rs`, `i128x.rs`, `front.rs`, `rir.rs`,
`main.rs`, `Cargo.toml`, `PLAN.md` et les docs des autres agents n'ont **pas**
été touchés. Tous les correctifs antérieurs sont préservés (INT2, ENG1 S4,
GLB2, CLOSE2, SPEED2 MC ILP-8, correction régression du 20/11) : le garde
`a_des_fonctions` n'a **pas** été réintroduit (`grep -c 'a_des_fonctions'
rose/src/interp.rs` = 0).

**Horloge des mesures : 20/11/2026 ~17:35–18:20 UTC** (la consigne annonce
21/11). Tout chiffre ci-dessous est un **constat daté**, à re-mesurer après
tout changement de binaire, de pinage, de source ou de topologie.

---

## 1. Verdict

| Porte | Résultat |
|---|---|
| Rose ≤ Rust — suite complexe | ❌ **non atteinte** (ratio final/Rust 1,036–1,075 ; reste 77–112 µs) |
| Rose ≤ Rust — englobant | ❌ **non atteinte** (ratio 1,29–1,60 ; reste 334–392 µs) |
| Gain réel sur la suite | ✅ **−27 à −37 µs** en différences appariées, signe stable sur c6/c16/c19 |
| Gain sur l'englobant | ❌ **aucun** (+20 / −8 / +5 µs = bruit) ; la voie modifiée n'y est pas empruntée |

Aucune triche : pas de formule fermée, pas de valeur codée en dur, pas de
banc modifié, pas de mémoïsation. Les bornes de la superinstruction sont lues
dans le RIR et l'accumulateur initial vient de l'argument réel de l'appel.

---

## 2. Protocole réellement exécuté

| Élément | Mesure |
|---|---|
| HEAD annoncé / HEAD aux mesures | `cadcdb8` / `cadcdb8` |
| Arbre source isolé | `/tmp/opencode/speed5_agents/src_base` (copie figée de `rose/` au départ, `target` exclu) |
| Binaire témoin | `rose_base`, SHA256 `102713f6fda93d9fa4797238e050fa639ed8d8268e5cfbb40bbe610429d092dc` |
| Binaire final | `rose_final`, SHA256 `31a2ecfa8c03ec21a51b89cd16468d0890d087d81b6e0a2a5a1d72db935976ad` (arbre figé) ; binaire du dépôt courant `0020e102…` |
| Construction | `CARGO_TARGET_DIR` dédié par binaire ; A et B viennent du **même arbre figé**, seul `interp.rs` diffère. Aucun binaire copié dans `rose/rose`. |
| Pinage | **`taskset -c 6`, `-c 16`, `-c 19`**, capacités re-vérifiées (`thread_siblings_list` = `6`, `16`, `19`) |
| Échantillons | suite **15/cœur**, englobant **12–15/cœur**, rotation entrelacée `base/final/rust` puis `final/rust/base` (`bench_suite.py`, `bench_englobant.py`) ; médianes **et** différences appariées |
| Horloges | CPU `getrusage(RUSAGE_CHILDREN)` pour les ratios ; `cntvct_el0` (fréquence **mesurée** `cntfrq_el0 = 1 000 000 000`, 1 tick = 1 ns) pour la décomposition |
| Valeurs | suite `0.9999999999860738` / `333333833333500000`, englobant `46297232.426252 / 564 / 157158 / 11899223598484`, global 11 lignes — vérifiées à **chaque** tirage |
| Bruit | A/A avec le MÊME binaire : suite `final−base` médian +1 µs mais médianes marginales 1,296 vs 1,490 ms ; englobant écarts appariés −151 à +93 µs. La décision repose sur les **différences appariées entrelacées**, pas sur les médianes marginales. |
| Instrumentation | temporaire, dans une copie isolée `src_prof` seulement ; **retirée du livrable** (`grep -c 'sp5_' rose/src/interp.rs` = 7 = les 7 identifiants de la superinstruction livrée, aucun compteur de profilage ; `grep -c 'SP5_\|sp5_dump' rose/src/interp.rs` = 0). |

---

## 3. Décomposition du temps restant dans `interp.rs`

Instrumentation `cntvct_el0` insérée dans la copie isolée, relevée avant/après
chaque phase ; 3 à 5 tirs c16 entrelacés, médianes. **Toute l'instrumentation a
été retirée avant livraison.**

### 3.1 Suite complexe (c16, µs, médianes chauffées)

| Phase | µs | % du interne |
|---|---:|---:|
| `parse` (front.rs) | 35,8 | 5,6 |
| `transform` (front.rs) | 8,8 | 1,4 |
| `prepare_jit` (jit.rs) | 17,6 | 2,7 |
| `compiler_*` | 0,4 | 0,1 |
| **total analyse + JIT** | **62,6** | **9,7** |
| **`executer_principal_rir`** | **583** | **90,8** |
| └ appel natif complexe (`calc`) | **517** | **80,5** |
| └ **somme de carrés** (`__rose_loop_0`) | **71,8** | **11,2** |
| └ eval dispatch / slots / synchro | ~12 | 1,9 |

**Goulot #1 = l'accumulation complexe, 80,5 % du temps interne.** Elle est
dans `jit.rs` (`complex_acc_neon`, interdit) et fait **517 µs contre 558 µs au
jumeau Rust** : Rose est déjà 41 µs plus rapide sur cette boucle. Le
deuxième poste, la somme de carrés (11,2 %), est servi par une closure
`Compile::Natif` définie dans `jit.rs` mais **appelée depuis `interp.rs`** :
c'est le seul poste réellement attaquable dans le fichier possédé.

`eval dispatch` : 3 appels à `eval_abstrait_rapide`, 2 à `eval_slots`, 0 à
`binaire_valeurs` — la boucle entière est déjà une superinstruction native.
Il n'y a plus de dispatch AST sur ce banc.

### 3.2 Englobant (c16, µs, médianes chauffées)

| Phase | µs | % du interne |
|---|---:|---:|
| `parse` (front.rs) | 91 | 15,8 |
| `transform` (front.rs) | 12 | 2,1 |
| `prepare_jit` (jit.rs) | 70 | 12,1 |
| `compiler_principal` | 4 | 0,7 |
| **total analyse + JIT** | **177** | **30,7** |
| **`executer_principal_rir`** | **~400** | **69,3** |
| └ **S6 Monte-Carlo** (`exec_mc`) | **223** | **38,6** |
| └ **`remplir_hasard_philox`** (run.rs) | **120** | **20,8** |
| └ S4 IEEE | 12 | 2,1 |
| └ S7 glanée (`exec_ph`) | 7 | 1,2 |
| └ S5 `compter_premiers` | 6 | 1,0 |
| └ dispatch RIR + autres builtins | ~32 | 5,5 |

**Goulot #1 = S6 Monte-Carlo (38,6 %)**, déjà réduit de 285 à 224 µs par
SPEED2 (ILP-8) ; goulot #2 = le remplissage Philox (20,8 %, dans `run.rs`,
interdit pour SPEED5). La détection des superinstructions (15 µs) et le clone
du corps (9 µs) ne sont plus que des restes.

### 3.3 Construction des clés `k<f>` (banc global, c16, instrumenté)

| Poste | µs (instrumenté) |
|---|---:|
| boucles de collection (`exec_boucle`) | 586–681 |
| dont **`format!("{}{}", prefixe, i)`** | **359–401** |

L'instrumentation ajoute ~30 ns par itération (5 000 clés) ; le coût réel de
`format!` est donc ~210–250 µs. Ce poste est bien dans `interp.rs`
(`exec_boucle`, branche `AccelGenre::Table`) mais **n'existe dans aucun des
deux bancs de cette mission** ; il est laissé à SPEED6, qui possède le banc
global. Aucun changement n'y a été apporté ici.

---

## 4. La décision structurale retenue (une seule, mesurée)

**Somme de carrés à 8 accumulateurs NEON, dispatchée depuis `interp.rs`.**

Le noyau `jit::carres_acc_neon` utilise 4 registres `u64×2` et paie DONC DEUX
`umlal` dépendants par registre et par bloc de 16 termes. Avec 8 registres,
chaque registre ne reçoit qu'UN `umlal` par bloc de 32 termes : la chaîne de
dépendance est divisée par deux. Micro-jumeau isolé, c16, 12 tirages,
médiane du meilleur : **71,8 → 52,3 µs** (−27 %), puis **53,9 µs** une fois
compilé dans Rose (mesuré par compteur `cntvct`).

Implémentation livrée (tout dans `rose/src/interp.rs`) :

1. `sp5_carres_neon` : 8 accumulateurs `uint64x2_t`, `vmlal_u32`, vidage en
   `i128` tous les 8192 blocs, queue scalaire. Repli scalaire sur les cibles
   non-aarch64. Mêmes termes, même ordre d'accumulation, même résultat i128.
2. `sp5_carres_plan` : détecteur **strict miroir** de
   `jit::Emetteur::detecter_carres_acc` (6 opérations exactes, 2 slots, 1
   paramètre entier, `fin ≤ 2^24`). Aucun nom de fonction codé en dur, aucune
   borne du banc codée en dur.
3. `appel_rir_direct` : si et seulement si le plan correspond et que
   l'argument est un `Valeur::Entier(_, None)`, appelle le nouveau noyau ;
   **sinon l'ancienne closure `f(nargs)` est appelée**, sans changer une
   seule valeur.

### Tentatives mesurées puis REJETÉES (aucune dans le livrable)

| Tentative | Mesure | Décision |
|---|---|---|
| Summer les carrés en i64 pur (8 accumulateurs scalaires) | micro-jumeau > NEON 8 voies | retiré |
| Réécrire Philox ILP/NEON dans `interp.rs` (pour l'englobant) | SPEED2 a mesuré 117–127 µs dans le crate contre 120 µs au noyau `run.rs` | non retenu (parité risquée, aucun gain) |
| Réécrire la boucle complexe dans `interp.rs` | 517 µs, déjà **41 µs sous** le jumeau Rust (558 µs) | non retenu |
| Supprimer `format!` des clés `k<f>` | hors des deux bancs ciblés ; SPEED6 possède le global | non retenu ici |

---

## 5. Gains réels c6 + c16 + c19

### Suite complexe — gain réel, stable en signe

| Pin | base méd | final méd | Rust méd | final−base (apparié) | final/Rust | base/Rust |
|---|---:|---:|---:|---:|---:|---:|
| c6 | 1,610 ms | 1,572 ms | 1,463 ms | **−37,0 µs** | 1,075 | 1,100 |
| c16 | 1,614 ms | 1,560 ms | 1,506 ms | **−37,0 µs** | 1,036 | 1,072 |
| c19 | 1,247 ms | 1,233 ms | 1,157 ms | **−27,0 µs** | 1,066 | 1,078 |

Le gain apparié (−27 à −37 µs) est du même ordre que le gain interne mesuré
au compteur (−18 µs sur la somme de carrés), le reste étant de la dérive.
**Aucune salve n'a donné Rose ≤ Rust.**

### Englobant — la voie modifiée n'est pas empruntée

| Pin | base méd | final méd | Rust méd | final−base (apparié) | final/Rust |
|---|---:|---:|---:|---:|---:|
| c6 | 1,332 ms | 1,429 ms | 0,893 ms | −8,0 µs (bruit) | 1,600 |
| c16 | 1,297 ms | 1,512 ms | 1,007 ms | +20,0 µs (bruit) | 1,501 |
| c19 | 1,430 ms | 1,423 ms | 1,100 ms | +5,0 µs (bruit) | 1,293 |

Les écarts sont dans la bande de bruit A/A (−151 à +93 µs). Le banc englobant
n'a aucune fonction de somme de carrés : le nouveau chemin n'y est jamais pris.

---

## 6. Pourquoi les deux portes restent fermées — reste chiffré

**Suite complexe.** Le jumeau Rust ne calcule PAS la somme de carrés (son
source n'affiche que les deux résultat complexes) : il fait 558 µs de calcul.
Rose, après optimisation, a :

```
complexe (déjà −41 µs vs Rust)     517 µs
carrés (au plancher NEON 8 voies)   54 µs
parse + transform + prepare_jit    62 µs
dispatch RIR résiduel               ~12 µs
                                  ───────
interne Rose                      ~645 µs  vs  558 µs Rust
```

Il manque **~87 µs** (mesuré au mur : 77–112 µs selon le cœur). Même en
ramenant à zéro TOUT ce qui est dans `interp.rs` (carrés 54 + dispatch 12 =
66 µs), il resterait ~21 µs de trop, dus à `parse` (35,8 µs, `front.rs`),
`transform` (8,8 µs, `front.rs`) et `prepare_jit` (17,6 µs, `jit.rs`) — trois
fichiers formellement interdits. La boucle complexe, elle, est déjà plus
rapide que Rust : il n'y a rien à y gagner.

**Englobant.** Le jumeau Rust `-O3 target-cpu=native` fait tout le calcul en
**287 µs**. Rose paie en plus :

```
parse + transform + prepare_jit + compile   177 µs   (front.rs / jit.rs, interdits)
S6 au mieux = Rust (205) au lieu de 223        +18 µs
Philox au mieux = Rust (~70) au lieu de 120    +50 µs
S4 + S5 + S7 + dispatch                       ~55 µs
                                            ────────
plancher Rose même avec MC et Philox parfaits  ≈ 510 µs  ≫ 287 µs
```

Écart minimal **≥ 223 µs**, entièrement dominé par des postes hors du fichier
possédé. Aucun gain de boucle native dans `interp.rs` ne peut combler un tel
écart. Ce sont des blocages physiques dans le périmètre autorisé, pas un
renoncement prématuré : le goulot a été décomposé, attaqué, et un gain réel
a été obtenu sur le seul banc où le fichier possédé peut agir.

---

## 7. Validation (preuves de sortie réelles, binaire `rose_final`)

```
cargo build --release --bin rose (arbre isolé, target dédié)      rc=0
cargo build --release --bin rose (dépôt courant, target dédié)     rc=0
cargo test --release (dépôt, target dédié)   rc=0 ;
   258 + 242 + 0 + 6 + 8 + 1 = 515 passed, 0 failed, 6 ignored
jardins/tests/premiers.rose      ── jardin/premiers : 20 tests, 0 échec ──
fib(25)                          75025
point fixe (2 voies)             b3ed9262dd54776a6dbd07bc12368d0e ×2
   (rose rir compilateur/emetteur.rose / rose compilateur/emetteur.rose
    compilateur/emetteur.rose ; identique au témoin et au binaire du dépôt)
9 domaines multi, c6, 9 tirages  81/81 OK (sorties OCTET POUR OCTET = base)
9 domaines multi, c16, 9 tirages 81/81 OK (sorties OCTET POUR OCTET = base)
bench_global.rose                rc=0, 11 valeurs IDENTIQUES à la base,
                                 stderr 0 octet
suite complexe c6+c16+c19        md5 ba144f964dc4b8775b176c31205c98fd
englobant c6+c16+c19             46297232.426252 / 564 / 157158 / 11899223598484
detail géant demo_geant.rose      rc=0, 5 propriétés typées affichées,
                                 stderr 0 octet
```

---

## 8. État final factuel

- **Fait** : 1 changement structurel livré dans `interp.rs` seul (somme de
  carrés à 8 accumulateurs NEON + détecteur strict + dispatch dans
  `appel_rir_direct`), 136 insertions / 1 suppression. Instrumentation de
  profilage **retirée** du livrable.
- **Vérifié** : somme de carrés 71,8 → 53,9 µs (compteur) ; suite −27 à
  −37 µs appariés sur c6/c16/c19 ; 515 tests verts ; jardin 20/20 ; fib25 ;
  point fixe ×2 ; 81/81 ×2 cœurs ; 11 valeurs globales ; md5 suite ;
  4 valeurs englobant ; `detail()` géant.
- **Non atteint** : Rose ≤ Rust sur les deux bancs. Reste chiffré ~87 µs
  (suite) et ≥223 µs (englobant), dont la quasi-totalité est dans `front.rs`,
  `jit.rs` et `run.rs` (interdits) ou est un calcul que Rose fait déjà plus
  vite que Rust. Aucune triche, aucun banc modifié.
- **Propreté** : aucun commit, aucune publication, aucun binaire installé
  dans `rose/rose`, aucun autre fichier source modifié par ce sous-agent.
