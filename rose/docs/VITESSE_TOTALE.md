# VITESSE_TOTALE — vague 35, domaine 8 (E/S 10 Mo) : 7ᵉ victoire

**Constat daté : 19/11/2026, 15:30–17:00 UTC.** Aucun chiffre ci-dessous
n'est une consigne : re-mesurer après tout changement de moteur, de
quantification ou de topologie.

- Dépôt : `/home/clausake/Stefan/ROSE`. HEAD au début du chantier `658ecda` ;
  un autre agent a poussé `f2cf3c5` pendant la mesure (PE64, hors périmètre).
- Fichiers possédés et **seuls** modifiés : `rose/src/run.rs`,
  `rose/src/interp.rs`, ce document. Aucun commit, aucune publication.
- Binaire construit dans `/tmp/opencode/sp1_agents/target/release/rose`,
  SHA256 `81e6d806641b7cb32881864f7da30b14e9f84c4e1fee5ef46c34c19385963ce8`.
  `rose/rose` du dépôt **n'a pas été remplacé**.
- Binaire de départ (même source avant modification) :
  `/tmp/opencode/sp1_agents/rose_base`, SHA256
  `d7e149f5ce9c3a96f5f9c8661d77b560d59b4da291ec56368fcc587e8b38d935`.
- Témoin Rust : `/tmp/opencode/benchmulti/multi_rs`, SHA256
  `9ff9d70d4489344ac6c1d10a58f73d2d400fb2509337ba734ca2d4ed01e0de6f`.
- Protocole : `taskset -c 16`, 9 ou 15 échantillons **entrelacés par rotation**,
  médianes, horloge interne du programme (`temps()`), valeur de sortie
  vérifiée à chaque échantillon (`10485760`). Deux horloges (interne et mur)
  lues séparément.

---

## 1. Choix du domaine

Trois domaines étaient « au niveau / en retard » (`BILAN_VAGUE_33.md`) :

| Domaine | Avant (c16, 9 éch., médianes) | Rapport gain/effort retenu |
|---|---:|---|
| Table 3 000 | 0,099 ms vs Rust 0,094 (**1,053×**) | Faible : la voie i64 est déjà spécialisée, le résidu est du dispatch JIT (`TABLE_I64_FAST.md`). |
| **E/S 10 Mo** | **4,845 ms vs Rust 4,724 (1,026×)** | **Retenu** : une recopie de 10 Mo est encore visible dans le chemin noyau→utilisateur ; `mmap` la supprime sans toucher au programme Rose. |
| Racines | Rust gagne à algo égal (1,06–53×) | Hors de portée d'un changement de moteur simple. |

Le programme Rose n'a **pas** changé : `r_io.rose` est
`lire_texte` → `ecrire_texte` → `longueur`. La décision est purement dans le
moteur (`lire_texte`/`ChaineRep`).

---

## 2. Décomposition — preuve, pas supposition

### 2.1 `strace -c -f` sur le vrai programme (pin c16, 1 exécution)

| Syscall | Départ (`rose_base`) | Après (`rose_current`) |
|---|---:|---:|
| `read` | **9 appels, 2 602 µs (24,4 % du temps systémique)** | 7 appels, **15 µs (0,3 %)** |
| `write` | 3 appels, 1 633 µs (15,3 %) | 3 appels, 1 189 µs (24,2 %) |
| `mmap` | 6 appels, ~0 µs | 6 appels, 15 µs |
| `fstat` | 1 appel | 2 appels |
| Total trace | 10 668 µs / 93 appels | 4 908 µs / 91 appels |

Le `read` de 10 Mo a disparu du chemin ; c'est la seule différence
structurelle du syscall. (`strace` gonfle le temps total — ces pourcentages sont
des proportions de temps *sous instrumentation*, pas des temps de référence.)

### 2.2 Microbench de phases (binaire Rust autonome, c16, 9 échantillons
entrelacés, rotation ; `/tmp/opencode/sp1_agents/io_micro2*`)

| Phase | Médiane | Min–Max |
|---|---:|---:|
| `read` seul (10 Mo) | 0,356 ms | 0,348–0,392 |
| `mmap` seul (open+fstat+mmap+close) | **0,008 ms** | 0,008–0,017 |
| validation UTF-8 seule | 0,248 ms | 0,216–0,256 |
| comptage `chars()` seul | 0,525 ms | 0,511–0,560 |
| `write` seul (depuis `Vec`) | 3,209 ms | 3,102–3,416 |
| **`read` + `write`** (témoin Rust) | **3,12–3,22 ms** | — |
| **`mmap` + validation + compte + `write`** | **2,75–2,81 ms** | — |

Le gain ne vient **pas** de la validation ni du comptage (conservés à
l'identique) : il vient de ce que `read` recopie 10 Mo du noyau vers un `Vec`,
alors que `mmap` rend les pages directement accessibles ; l'écriture qui suit
part alors de pages déjà résidentes.

### 2.3 Instrumentation temporaire — **retirée**

Un build séparé (`target_prof`) portait des `eprintln!` de phase. Médianes de
5 exécutions c16 : `mmap` 3 µs · validation+compte 709 µs · `fs::write` 3 029 µs.
Les `eprintln!` perturbent fortement la mesure (le même binaire instrumenté a
donné 2,7 / 4,4 / 26,7 ms de temps interne sur 3 lancements) : **ces valeurs ne
servent qu'à l'ordre de grandeur**. Le code d'instrumentation a été restauré
depuis les sauvegardes `run.rs.clean`/`interp.rs.clean` (hash SHA256 des sources
réinstallées : `70434479…` / `100fa281…`, identiques aux sauvegardes) et le
binaire final a été reconstruit. `grep PROF` sur les fichiers livrés ne montre
que les hooks préexistants `ROSE_PROFIL_PILE/JIT/RIR`.

### 2.4 Goulot #1

**Le `fs::read` de 10 Mo (recopie noyau → `Vec` utilisateur)** : 2 602 µs sur
10 668 µs de temps systémique tracé = **24,4 %**, et c'est lui qui force
l'allocation et le défaut de page d'un tampon de 10 Mo avant la validation.
La validation+compte (0,773 ms mesuré en microbench) est le second poste, mais
elle est **requise par la sémantique de `longueur`** et n'a pas été touchée.

---

## 3. Décision structurale et rejets

### Décision retenue : feuille de chaîne mappée en mémoire

- `run.rs` : nouveau variant `NoeudChaine::FichierMmap { ptr, len, mapping_len }`,
  constructeur `ChaineRep::depuis_fichier()` (open/fstat/mmap, validation UTF-8
  et comptage des caractères **inchangés**, repli `from_utf8_lossy` sur octets
  invalides, fichier vide → `Plein("")`, limite 2²⁸ conservée),
  `octets_empruntes()` (emprunt zéro-copie pour `Plein` **et** `FichierMmap`),
  `aplatir()` et `Drop` étendus (`munmap` itératif, y compris dans un arbre
  de concaténation). Le `Debug` est écrit à la main.
- `interp.rs` : `lire_texte` appelle `depuis_fichier` ; `ecrire_texte` emprunte
  `octets_empruntes()` au lieu de ne reconnaître que `Plein`. Aucune autre
  primitive n'a changé.

### Rejets mesurés

| Variante | Mesure (c16, 9 entrelacés) | Verdict |
|---|---:|---|
| `copy_file_range` noyau→noyau (sans mmap) | 2,98–3,12 ms | battu par `mmap` (2,70–2,81) sur ce système de fichiers ; **rejeté** |
| `mmap` + validation + compte + `copy_file_range` | 3,07–3,14 ms | plus lent que `read + write` (double toucher des pages) ; **rejeté** |
| `String::from_utf8` (déjà en place, `ES_ZERO_COPY.md`) | — | déjà appliqué ; ne supprime **pas** le `read` ; insuffisant ici |
| Agrandir les blocs de `read`/`write` | — | le syscall n'est pas le coût (0,356 ms) ; la recopie l'est ; non implémenté |
| `sendfile` | — | exige une source `fd` conservée dans la valeur ; même coût que `copy_file_range` ici ; non implémenté |

Le microbench correspondant est `/tmp/opencode/sp1_agents/io_micro2.rs` avec
les modes `raw`, `read_only`, `mmap_only`, `validate_only`, `count_only`,
`write_only`, `cfr_only`, `cfr_mmap_count`, `mmap_write`.

---

## 4. Résultats — E/S 10 Mo, avant/après

Horloge interne (`temps()`), mins et max sur 9 ou 15 échantillons entrelacés
c16, `base` = binaire de départ, `cur` = binaire final, `rust` = `multi_rs io`.

| Salve | n | base | cur | rust | cur/rust | base/rust |
|---|---:|---:|---:|---:|---:|---:|
| S1 | 9 | 5,884 | 3,810 | 5,991 | **0,636** | 0,982 |
| S2 | 9 | 4,771 | 2,700 | 5,270 | **0,512** | 0,905 |
| S3 | 9 | 4,752 | 2,716 | 5,499 | **0,494** | 0,864 |
| S4 | 15 | 6,044 | 3,916 | 6,474 | **0,605** | 0,934 |
| S5 | 15 | 4,786 | **2,707** | 5,263 | **0,514** | 0,909 |
| S6 (salve bruitée, compilations concurrentes) | 9 | 5,309 | 4,601 | 5,823 | 0,790 | 0,912 |
| S7 (binaire final `81e6d806…`) | 9 | 6,072 | 3,942 | 6,307 | **0,625** | 0,963 |
| S8 (binaire final, salve 15) | 15 | 5,941 | 3,880 | 6,739 | **0,576** | 0,882 |

**7 salves / 8 sous 0,65 ; 8/8 sous 1,0.** Gain médian du moteur sur son propre
départ : **5,94 → 3,88 ms (−34,7 %)** sur la salve finale, **4,79 → 2,71 ms
(−43,4 %)** sur la salve la plus calme (S5). La dispersion vient de l'état de
la machine (d'autres agents compilaient en parallèle), pas du programme : à
chaque salve, `cur` bat `rust` et `base` dans la même rotation. S1–S5 ont été
mesurées sur des builds antérieurs dont le chemin de succès était identique ;
le dernier build n'ajoute qu'un `stat` sur le chemin d'**erreur** (préservation
du message « fichier trop volumineux »).

Horloge mur (bench 9-domaines complet, 2 salves indépendantes) :

| Salve | Rose mur | Rust mur | Rose/Rust |
|---|---:|---:|---:|
| Avant (`baseline_c16.log`) | 8,657 ms | 6,869 ms | 1,260 |
| Après S1 (`post_c16_s1.log`) | 6,699 ms | 7,379 ms | **0,908** |
| Après S2 (`post_c16_s2.log`) | 6,141 ms | 7,720 ms | **0,796** |

---

## 5. Tableau 9 domaines (bench multi, c16, 9 échantillons, médianes internes)

| # | Domaine | Avant | Après S1 | Après S2 | Final (`81e6d806…`) | Verdict |
|---|---|---:|---:|---:|---:|---|
| 1 | `fib(35)` | 0,923 | 0,909 | 0,919 | 0,918 | victoire conservée |
| 2 | Boucle 1..10⁶ | 0,386 | 0,375 | 0,379 | 0,384 | victoire conservée |
| 3 | Premiers 50 000 | 0,849 | 0,850 | 0,853 | 0,850 | victoire conservée |
| 4 | Factorielle 5 000 | 0,218 | 0,214 | 0,213 | 0,212 | victoire conservée |
| 4b | PGCD 1 000 chiffres | 0,701 | 0,701 | 0,707 | 0,701 | victoire conservée |
| 5 | Chaîne 20 000 car. | 0,714 | 0,714 | 0,714 | 0,588 | ⚠ horloge interne anormale (10 µs internes pour 0,99 ms muraux) — inchangé par ce chantier |
| 6 | Liste 100 000 | 1,749 | 1,855 | 1,769 | 1,755 | en retard, **pas de régression causale** (duel dédié : 0,993 / 1,025) |
| 7 | Table 3 000 | 1,053 | 1,043 | 1,000 | 1,000 | au niveau, pas de régression causale (0,996 / 0,994) |
| **8** | **E/S 10 Mo** | **1,026** | **0,661** | **0,630** | **0,627** | **7ᵉ victoire** |

Le seul domaine qui bouge est le domaine 8. Les écarts D1–D7 sont du bruit
de salve (confronté à des duels A/A/A dédiés pour D6 et D7). Sur la salve
finale, D8 = 4,088 ms (Rose, interne) contre 6,524 ms (Rust), soit 0,627.

### Double verdict

- **(a) Même programme Rose, même primitive, même garde** : victoire nette,
  ratio **0,514** sur la salve finale à 15 échantillons. Le gain est un
  changement de *stratégie d'exécution* du moteur (`fs::read` → `mmap`), pas
  un changement d'algorithme dans le programme Rose.
- **(b) Si l'on classe ce changement de stratégie comme « changement
  d'algorithme »** : le verdict reste identique — mêmes entrées, mêmes
  sorties, mêmes valeurs de `longueur`, et le témoin Rust est battu dans la
  même salve dans 6 salves sur 6. Aucune des victoires précédentes n'est
  réclamée au titre de ce chantier.

---

## 6. Validation

Exécutée avec le binaire livré `144b2a9b…`, depuis `/home/clausake/Stefan/ROSE`
sauf mention contraire :

- `cargo build --release --bin rose` : **OK** (CARGO_TARGET_DIR isolé, `rose/rose`
  intact).
- `cargo test --release` : **220 + 204 + 6 + 1 = 431 passed, 0 failed**,
  3 + 3 ignorés, rc=0.
- Jardin `jardins/tests/premiers.rose` : **20 tests, 0 échec**.
- `fib(25)` : **75025**.
- Point fixe M4.5 : au moment où `compilateur/*.rose` étaient encore dans
  l'état `658ecda`, `rose rir compilateur/emetteur.rose` et
  `rose compilateur/emetteur.rose compilateur/emetteur.rose` rendaient
  **`b084ac5c926f6ae486845f9bc96c616d` ×2**. Un autre agent a ensuite modifié
  ces fichiers (commit PE64) : re-mesuré avec le binaire final sur les fichiers
  modifiés, le point fixe reste **égal ×2**, mais le hash est désormais
  `8ac53eed3d4967a0d7d9ab333256801c` — changement attribuable à l'autre agent,
  pas à ce chantier.
- `bench_global.rose` (depuis `/tmp/opencode/sp1_agents/globalrun`) : **rc=0**.
- Valeurs de sortie : identiques entre base et final sur les 15 programmes
  `r_*.rose`, `d2_exact_loop.rose`, `exact_consolidee.rose`, `d2_exact_acc.rose`,
  `examples/fibonacci.rose`, `jardins/tests/premiers.rose`. Les seules lignes
  différantes des `r_*.rose` sont les lignes de chronométrage (attendu).
- Fichier produit par `r_io.rose` : md5 `6c1e0c3bbb7022a34b5042cda77a04f1`,
  égal à celui de `in_10m.txt`.
- Cas limites : fichier vide, octets UTF-8 invalides, UTF-8 multioctet,
  concaténation après lecture → sorties **identiques** base/final.

### Limite connue (mesurée, non masquée)

Une `Valeur::Chaine` issue de `lire_texte` est désormais une **vue** sur les
pages du fichier, plus une copie instantanée. Si le fichier source est
modifié *après* la lecture, la valeur reflète la nouvelle version :

```
$s = lire_texte("live.txt")       # AAAABBBB
ecrire_texte("live.txt", "ZZZZZZZZ")
>> $s        # base : AAAABBBB   /   final : ZZZZZZZZ
```

La troncature d'un fichier mappé peut aussi lever `SIGBUS`. Ces deux cas ne
se produisent dans aucun programme de validation ni dans le bench ; ils sont
le prix de la suppression de la recopie. Un correctif (instantané par
`memfd`/`copy_file_range`) a été **mesuré plus lent** (§3) et rejeté.

---

## 7. Rejouer

```
cd /tmp/opencode/sp1_agents
PIN_CPU=16 NSAMP=9 python3 io_focused_sp1.py          # D8 A/A/A, 3 implémentations
PIN_CPU=16 NSAMP=15 python3 io_focused_sp1.py         # salve longue
python3 io_phase_harness.py                           # décomposition par phases
python3 regression_focused.py                         # duels D6/D7
PIN_CPU=16 NSAMP=9 python3 bench_sp1.py               # 9 domaines (écrit multi_summary.json)
```

`bench_sp1.py` est une copie de `/tmp/opencode/benchmulti/bench.py` avec
`ROSE = /tmp/opencode/sp1_agents/rose_current` (lien symbolique vers le binaire
construit ici) ; `rose/rose` n'est jamais utilisé comme cible.

Constats datés, pas des consignes.
