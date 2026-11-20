# SP8 — `prepare_jit` sans recopie d'AST ni de RIR (CONSTAT DATÉ)

Auteur : sous-agent `sp8`. **Aucun git commit, aucune publication, aucun
`rose/rose` remplacé.** Un seul fichier source du dépôt modifié :
`rose/src/jit.rs` (+23/−11). `front.rs`, `rir.rs`, `interp.rs`, `run.rs`,
`i128x.rs`, `Cargo.toml`, `Cargo.lock`, `PLAN.md` et les docs des autres agents
n'ont **pas** été touchés. Le fichier `compilateur/emetteur_elf.rose`, déjà
modifié à l'arrivée, n'a pas été modifié non plus. Tous les correctifs antérieurs
sont préservés : SPEED1 fib (`detecter_fib_rir` présent), SPEED3 analyse sans
clones (`replier_ip`, `replier_instr_ip`, `rep_ip` présents), CLOSE2 et
correction régression du 20/11 (commentaires intacts dans `interp.rs`, garde
`a_des_fonctions` absente : `grep -c` = 0).

**Horloge des mesures : 20/11/2026 ~19:20–19:45 UTC** (la consigne annonce le
21/11). Tout chiffre ci-dessous est un **constat daté**, à re-mesurer après tout
changement de binaire, de pinage, de source ou de topologie.

---

## 1. Verdict

| Porte | Résultat |
|---|---|
| Rose ≤ Rust — englobant | ❌ **non atteinte** (ratio final/Rust 1,26–1,41 au mur ; plancher interne 1,49–1,52) |
| Gain interne mesuré (`jitnew`) | ✅ **−32 à −34 µs**, signe stable sur c6/c16/c19, 11 tirages appariés chacun |
| Gain au mur englobant | ⚠ **visible sur c6 seulement** (−153 µs) ; dans la bande de bruit sur c16 (−27 µs) et c19 (+9 µs) |
| Blocage physique | ✅ **prouvé** : le seul `exec` de Rose (436–441 µs) dépasse déjà tout le calcul Rust (290–294 µs) |

Aucune triche : pas de formule fermée, pas de valeur codée en dur, aucun banc
modifié, pas de mémoïsation entre processus, mêmes valeurs vérifiées à chaque
tirage.

---

## 2. Protocole réellement exécuté

| Élément | Mesure |
|---|---|
| HEAD annoncé / HEAD aux mesures | `76b0235` / `76b0235` (inchangé) |
| Arbres isolés | `/tmp/opencode/sp8_agents/src_base` (témoin) et `src_cand` (identique sauf `jit.rs`) ; profilage dans `src_prof`, jamais livré |
| Binaire témoin | `rose_base`, SHA256 `1cd87315ea0bcc112decc5e8c697493ecdd145df6c184b6ed18388ba4ed6b48b` |
| Binaire final | `rose_cand`, SHA256 `f0aeafb5d16a95c622b7e4e60a371588ee1a2a2f55e5d5ebd80774a67c8df59e` |
| Construction | `CARGO_TARGET_DIR` dédié par binaire ; A et B viennent du même arbre figé, seul `jit.rs` diffère. Aucun binaire copié dans `rose/rose`. |
| Pinage | **`taskset -c 6`, `-c 16`, `-c 19`**, capacités re-vérifiées (`thread_siblings_list` = 6 / 16 / 19) |
| Échantillons | interne **11 appariés/cœur** (`inner_ab.py`) ; mur **9–12/cœur** en rotation entrelacée A/B/Rust puis B/Rust/A (`bench_englobant.py`) ; plancher **11/cœur** |
| Horloges | CPU `getrusage(RUSAGE_CHILDREN)` pour les ratios muraux ; `cntvct_el0` (fréquence **mesurée** `cntfrq_el0 = 1 000 000 000`, 1 tick = 1 ns) pour la décomposition ; `ROSE_TIMING` pour les phases haut niveau |
| Valeurs | englobant `46297232.426252 / 564 / 157158 / 11899223598484`, suite `0.9999999999860738 / 333333833333500000`, global 11 lignes — vérifiées à chaque tirage |
| Bruit A/A | même binaire des deux côtés, 9 tirages : englobant `b−a` médian **+23 µs (c6), −47 µs (c16), +31 µs (c19)**. La décision interne repose sur des différences appariées 10× plus stables que ce bruit. |
| Instrumentation | temporaire, dans `src_prof` seulement ; **retirée du livrable** (`grep -c 'sp8_' rose/src/jit.rs` = 0 ; la chaîne `SP8` n'y reste que dans 3 lignes de commentaire explicatif) |

Harnais : `/tmp/opencode/sp8_agents/{bench_englobant,bench_suite,bench_multi,inner_ab,floor_proof}.py`.
CSV/JSON : `sp8_{AA,AB}_c{6,16,19}.*`, `sp8_suite*`.

---

## 3. Décomposition à nouveau mesurée sur le binaire courant

### 3.1 Phases haut niveau — englobant, c16, `ROSE_TIMING`, 9 tirs chauds (µs)

| Phase | témoin (méd) | candidat (méd) |
|---|---:|---:|
| `parse` (`front.rs`) | 87,1 | 93,5 (bruit) |
| `transform` (`interp.rs`) | 11,6 | 12,4 (bruit) |
| `jitnew` = RIR + `prepare_jit` + reste | 65,2 | 37,2 |
| `compiletout` | ~0,1 | ~0,1 |
| `compileprincipal` | 4,3 | 4,8 |
| `exec` (`interp.rs` / `run.rs`) | ~435 | ~450 (bruit) |
| **total interne** | **~597** | **~598** |

### 3.2 Détail fin — instrumentation `cntvct_el0` dans `src_prof`, c16, 9 tirs chauds (µs)

| Poste | µs | % du total interne |
|---|---:|---:|
| lexeur (`tokeniser`) | 25–28 | ~4,5 % |
| **parseur (`Parser::programmer`)** | **49–54** | **~8,5 %** |
| repli de constantes | ~1 | ~0,2 % |
| détection compteur « i » | ~0 | ~0 % |
| réparation « i » | ~0–1 | ~0,2 % |
| `verifier` | 17–19 | ~3 % |
| RIR : `abaisser_principal` | 19–24 | ~3,5 % |
| `prepare_jit` | 19–23 | ~3,5 % |
| └ clone `prog.instructions` | **~12** | ~2 % |
| └ clone `rir_principal` | **~20** | ~3,5 % |
| └ `rir_compilable_jit` | ~3 | ~0,5 % |
| └ `bloc_compilable` / détections superinstructions | ~1 | ~0,2 % |
| `exec` | ~435 | ~73 % |

**Goulot #1 global = `exec` (~73 %)**, intégralement dans `interp.rs` / `run.rs`
(interdits). **Goulot #1 dans mon périmètre = le parseur (~50 µs)**, suivi du
lexeur (~26 µs) et de `verifier` (~18 µs). Les deux clones de `prepare_jit`
(~32 µs cumulés) étaient le plus gros poste **entièrement évitable** sans
réécriture du langage.

Note d'honnêteté : le chiffre « `prepare_jit` 70 µs » hérité de SPEED5 mesurait
en fait l'intervalle `ROSE_TIMING.jitnew` tout entier (RIR + `prepare_jit`), pas
`prepare_jit` seul. La décomposition ci-dessus le corrige.

---

## 4. La décision structurale retenue (une seule, mesurée)

**`prepare_jit` n'a plus jamais besoin de posséder l'AST ni le RIR qu'il inspecte.**

Tout dans `rose/src/jit.rs` :

1. **Séparation de la décision et du conteneur.** `rir_compilable_jit(&sig, …)` ne
   fait plus que déléguer à une nouvelle fonction `rir_compilable_corps(corps,
   sig, …)` qui reçoit le `&CorpsRir` en paramètre. Les 5 autres appels de
   `rir_compilable_jit` gardent exactement leur ancien comportement.
2. **Plus de `prog.instructions.clone()`.** `Signature.corps` vaut `None` pour le
   faux principal ; `bloc_compilable(&prog.instructions, …)` lit les instructions
   d'origine. Cette fonction ne faisait que lire — le clone était purement gratuit.
3. **Plus de `rir_principal.clone()`.** Quand l'interpréteur a déjà construit le
   RIR, il est **emprunté** (`rir_principal.or(propre.as_ref())`) ; le repli
   `abaisser_principal` n'est appelé que s'il est absent.

La décision `principal_compilable` est bit à bit la même : `rir_compilable_corps`
reçoit exactement le corps que le clone contenait, et `sig` n'est utilisé nulle
part ailleurs dans `prepare_jit`.

### Mesure interne appariée (11 tirages/cœur, A/B entrelacé, `ROSE_TIMING.jitnew`)

| Pin | base méd | candidat méd | candidat−base apparié | min | max |
|---|---:|---:|---:|---:|---:|
| c6 | 63,28 µs | 30,02 µs | **−33,82 µs** | −41,61 | −29,33 |
| c16 | 60,03 µs | 28,32 µs | **−32,35 µs** | −40,22 | −31,12 |
| c19 | 60,27 µs | 27,02 µs | **−33,14 µs** | −60,88 | −29,04 |

**Aucun tirage n'est positif.** C'est la preuve la plus solide de ce chantier :
le gain est reproductible et indépendant du cœur, contrairement au mur.

### Gains au mur (englobant, getrusage CPU)

| Pin | base méd | candidat méd | Rust méd | c−b apparié | base/Rust | cand/Rust |
|---|---:|---:|---:|---:|---:|---:|
| c6 (12) | 1,524 ms | 1,400 ms | 0,991 ms | −153 µs | 1,539 | 1,413 |
| c16 (9) | 1,539 ms | 1,497 ms | 1,188 ms | −27 µs | 1,295 | 1,260 |
| c19 (12) | 1,312 ms | 1,251 ms | 0,958 ms | +9 µs | 1,370 | 1,306 |

Honnêteté : la bande A/A va de −47 à +31 µs. Seul c6 sort nettement de cette
bande ; sur c16 et c19 le gain mural n'est **pas** démontrable séparément du
bruit. Le gain interne, lui, l'est.

### Pistes mesurées puis REJETÉES (rien dans le livrable)

| Piste | Mesure | Décision |
|---|---|---|
| Lexeur en une passe (scan octets) | 26 µs mesurés ; risque Unicode (`π`, `φ`, accents) couvert par 515 tests | non retenu |
| Table de symboles indexée dans `verifier` | `chercher_std` instrumenté : **1–2 µs sur 47 appels** — le scan linéraire n'est pas un goulot | non retenu |
| Court-circuit JIT « programme sans fonction » | `bloc_compilable` ne coûte déjà que ~1 µs (il échoue au premier builtin) : aucun gain | non retenu |
| Supprimer les `Box<Expr>` du parseur | 94 sites dans 12 fichiers dont `interp.rs` / `run.rs` (interdits) | non retenu |

---

## 5. Pourquoi la porte reste fermée — blocage physique prouvé

Mesure dédiée (`floor_proof.py`, 11 tirs/cœur, `ROSE_TIMING` d'un côté, ligne
`min:` du jumeau Rust de l'autre) :

| Pin | `exec` Rose | total interne Rose | calcul Rust **entier** | `exec` / Rust | plancher Rose si front+JIT = 0 |
|---|---:|---:|---:|---:|---:|
| c6 | 441,1 µs | 562,6 µs | 290,0 µs | **1,521** | 445,2 µs |
| c16 | 438,5 µs | 565,1 µs | 294,0 µs | **1,491** | 447,1 µs |
| c19 | 436,3 µs | 554,4 µs | 292,0 µs | **1,494** | 440,0 µs |

Le jumeau Rust fait **tout** le calcul en ~290 µs. Rose, après avoir ramené à
zéro **tout** mon périmètre (`parse` ~78 + `transform` ~10 + `jitnew` ~28 ≈
116 µs), paie encore ~440–447 µs de `exec` seul. Ce `exec` est :

```
S6 Monte-Carlo   ~223 µs   (exec_mc, interp.rs — interdit)
Philox           ~120 µs   (remplir_hasard_philox, run.rs — interdit)
S4 IEEE           ~12 µs   (exec_s4, interp.rs — interdit)
S7 glanée          ~7 µs   (exec_ph, interp.rs — interdit)
dispatch RIR      ~32 µs   (interp.rs — interdit)
```

Écart résiduel minimal **≥ 145 µs**, entièrement hors du périmètre autorisé.
Ce n'est pas un renoncement : le goulot a été décomposé, le plus gros poste
évitable du périmètre a été supprimé et mesuré, et le solde est structurellement
inatteignable depuis `jit.rs` / `front.rs` / `rir.rs`.

---

## 6. Validation (preuves de sortie réelles, binaire `rose_cand`)

```
cargo build --release --bin rose (dépôt, target dédié)                  rc=0
cargo test --release (dépôt, target dédié)   rc=0 ;
    258 + 242 + 0 + 6 + 8 + 1 = 515 passed, 0 failed, 6 ignored
jardins/tests/premiers.rose      ── jardin/premiers : 20 tests, 0 échec ──
fib(25)                          75025
point fixe (2 voies)             b3ed9262dd54776a6dbd07bc12368d0e ×2
   (rose rir compilateur/emetteur.rose ET rose compilateur/emetteur.rose
    compilateur/emetteur.rose ; identique au témoin)
9 domaines multi, c6, 9 tirages  81/81 OK (sorties OCTET POUR OCTET = base)
9 domaines multi, c16, 9 tirages 81/81 OK (sorties OCTET POUR OCTET = base)
bench_global.rose                rc=0, 11 valeurs IDENTIQUES à la référence,
                                 stderr 0 octet
suite complexe                   md5 ba144f964dc4b8775b176c31205c98fd
                                 (chaque tirage, c6+c16+c19)
englobant c6+c16+c19             46297232.426252 / 564 / 157158 / 11899223598484
detail géant demo_geant.rose      rc=0, 5 propriétés typées affichées,
                                 stderr 0 octet
```

---

## 7. État final factuel

- **Fait** : 1 changement structurel livré dans `jit.rs` seul (+23/−11), sans
  toucher à `front.rs` ni `rir.rs`. Aucune instrumentation de profilage dans le
  livrable. `Cargo.lock`, modifié automatiquement par `cargo build` (0.1.6 →
  0.1.7), a été **restauré** à son contenu d'origine.
- **Vérifié** : −32 à −34 µs appariés sur `jitnew` c6/c16/c19 (aucun tirage
  positif) ; 515 tests verts ; jardin 20/20 ; fib25 ; point fixe ×2 ; 81/81 ×2
  cœurs ; 11 valeurs globales ; md5 suite ; 4 valeurs englobant ; `detail()`
  géant.
- **Non atteint** : Rose ≤ Rust sur l'englobant. Reste chiffré : plancher
  interne 440–447 µs vs 290–294 µs Rust, dominé par `exec` dans `interp.rs` et
  `run.rs` (interdits). Aucune triche, aucun banc modifié.
- **Propreté** : aucun commit, aucune publication, aucun binaire installé dans
  `rose/rose`, aucun autre fichier source modifié par ce sous-agent.
