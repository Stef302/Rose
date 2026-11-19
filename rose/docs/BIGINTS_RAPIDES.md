# BIGINTS_RAPIDES — réduire les défaites « gros nombres » du comparatif

**Constat daté : 19/11/2026, 22:15–22:25 UTC.** Aucune mesure de ce document
n'est une consigne : re-mesurer après tout changement de moteur, de binaire,
de noyau de pinage ou de topologie.

**Auteur** : sous-agent `big1`. **Aucun commit, aucune publication.**
Binaire construit dans `/tmp/opencode/big1_agents/`, **jamais installé sur
`rose/rose`**. Périmètre source : **`rose/src/run.rs` uniquement** (+20 lignes).
`rose/src/i128x.rs` n'a PAS été modifié. Le correctif rationnel `f9b0966`
(`q_chaine`/`texte_rationnel`) et le pool de pages `38a8804` sont intacts.
`rose/src/jit.rs` a été modifié pendant la session par un AUTRE agent (DISP1) :
toutes les mesures finales ont été rejouées avec le même `jit.rs` des deux
côtés du A/B (voir §6).

---

## 1. Protocole réellement exécuté

| Élément | Valeur mesurée |
|---|---|
| HEAD annoncé | `eeebb824a64c3b6c2982931d11d0a1e66a521786` |
| Pinage | **`taskset -c 6`** — c6 = Cortex-X925, `cpu_capacity=997`, max 3,9 GHz (c11 du comparatif = A725, capacity 731). Les chiffres ici **ne sont pas comparables** à ceux de `COMPARATIF_TOTAL.md`. |
| Échantillons | **9+9 entrelacés** A/B (ordre alterné à chaque répétition), médianes, min–max publiés. |
| Bruit A/A mesuré | global **0,02 %** (403,012 vs 402,920 ms) ; suite **0,13 %** ; englobant **0,25 %**. |
| Horloges | H1 mur = `perf_counter` autour du sous-processus ; H2 = CPU `getrusage(RUSAGE_CHILDREN)` ; global a en plus l'horloge interne Rose (`millisecondes()`). |
| Valeurs | vérifiées à **chaque** tirage : global 11 lignes ; suite `0.9999999999860738` ; englobant 4 valeurs ; multi 9 valeurs. |
| Binaire de départ | `/tmp/opencode/big1_agents/rose_big1_base`, sha256 `c8f779fd14111cfd288311b935f0a8119c18702be35613edaae5a7578c781c12`. |
| Binaire final | `/tmp/opencode/big1_agents/rose_big1_final`, sha256 `ab0b22e620ae6988b2f599d03cbfc8bec74b89877491279e7fece12c444cd64b`. |
| Jumeaux Rust | ceux de `cmp2` (`global_rs`, `suite_rs`, `eng_rs`), rejoués dans la même salve. |

---

## 2. Reproduction des trois défaites (c6, base, 9 échantillons)

| Banc | Rose mur méd ms | Rust mur méd ms | Ratio |
|---|---:|---:|---:|
| Global 8 domaines | **405,161** (402,226–405,998) | 3,154 | **128,5×** |
| Suite complexe 10⁶ | **332,386** (327,807–335,708) | 1,819 | **182,7×** |
| Englobant 4 sections | **90,971** (90,527–91,515) | 1,341 | **67,8×** |

---

## 3. Décomposition — goulot #1

### 3.1 Par section (banc global, copie `.rose` temporaire avec chronomètres, puis retirée)

| Section | ms sur 400 | % |
|---|---:|---:|
| **1 — entiers `10⁷` (`$total = $total + a*a`)** | **382** | **95,5 %** |
| 2 — flottants `10⁶` | 2 | 0,5 % |
| 3 — rationnels (`1/3 + Σ1/c`, 1000 tours) : calcul 11 ms, affichage 0 ms | 11 | 2,8 % |
| 4–9 — chaînes, listes, tables, fib, RNG, E/S | 5 | 1,3 % |

Le goulot #1 est **la boucle entière**, pas les rationnels ni l'affichage.
Le `ROSE_SANS_JIT=1` donne 745 ms contre 380 ms : le JIT est actif et divise
déjà le temps par ~2.

### 3.2 Par instrumentation (counters temporaires dans `run.rs`, puis retirés)

Sur les 10⁷ tours de la section 1 :

| Compteur | Valeur |
|---|---:|
| appels `rose_mul_ent` (`a*a`) | 10 000 000 |
| appels `rose_add_ent` (`$total + …`) | 6 975 385 (débordement i64 à `a≈3,02×10⁶`) |
| **appels `mpz_ptr`** | **40 926 156** (≈4,1 par tour) |
| `mpz_ptr` rejetés par la plage | 33 950 772 (**82,9 %**) |
| `mpz_ptr` ayant fait un `HashSet::contains` | 6 975 384 (17,1 %, **tous des hits**) |
| `checked_add`/`checked_mul` réussis | 10 000 001 |
| Débordement i64 détecté | 1 |

Chaque `mpz_ptr` faisait un `MPZ_BOXES.with(|s| s.borrow().contains(&p))`,
c.-à-d. un accès thread-local + un `RefCell::borrow` + un **SipHash** de 8
octets, y compris quand la valeur était un simple `i64` (`a*a`, compteur).
Le coût a été isolé par deux A/B successifs (binaire témoin `jit.rs`=HEAD) :

| Étape | Global mur méd | Delta | % du temps de départ |
|---|---:|---:|---:|
| Départ | 403,438 ms | — | — |
| + filtre de plage dans le HashSet thread-local | 232,700 ms | **−170,7 ms** | **42,3 %** |
| + bornes atomiques globales (suppression du thread-local sur le chemin chaud) | 183,877 ms | **−48,8 ms** | **12,1 %** |
| **Total gagné** | | **−219,5 ms** | **54,4 %** |

**Goulot #1 chiffré : 54,4 % du temps du banc global était le reconnaissance
des petits entiers comme pointeurs de grands entiers (`mpz_ptr`), pas le
calcul BigInt lui-même.**

### 3.3 Hypothèses de la mission, tranchées

- **(a) `mul_rapide` (Toom3/NTT) déjà présent** : `grep` montre qu'il n'est
  appelé que par la factorielle (`i128x.rs:76,246,264,290`) et par
  `Gz::chaine()` (`:551`). **Ni la multiplication `a*a` (i64 `checked_mul`
  dans le JIT), ni le cumul boxé (`+=` sur `BigInt`), ni les rationnels
  (`add_in`/`div_in` de `Gq`) ne passent par `mul_rapide`.** Hypothèse écartée
  par lecture du code, pas par supposition.
- **(b) pool de `Gz`** : déjà présent (`gz_pool.rs`) et utilisé par
  `binop_z_pool` dans l'interpréteur. Le chemin JIT, lui, utilise `mpz_box`.
  C'est ce chemin qui a été optimisé.
- **(c) voie rapide i64 pour rationnels à petit dénominateur** : la section 3
  ne pèse que 11 ms / 400 (2,8 %). Même un gain de 100 % n'atteindrait pas
  la porte ×2. Hypothèse écartée à la mesure.

---

## 4. Changement structural retenu (décision unique)

Dans `run.rs`, fonction `mpz_ptr` (la reconnaissance tag → pointeur) :

1. Trois atomiques (`BOX_LO`, `BOX_HI`, `BOX_NB`) mis à jour **une seule fois
   par boîte créée**, dans `mpz_box`.
2. `mpz_ptr` commence par : « aucune boîte ? → `None` », puis « valeur hors de
   `[lo,hi]` ? → `None` », **avant** tout accès thread-local et avant tout
   hachage.
3. Le `HashSet` thread-local **reste la preuve finale** : une borne globale
   est un sur-ensemble des boîtes d'un fil, donc un faux positif est toujours
   éliminé. La sémantique est strictement identique (un vrai pointeur boxé
   est toujours dans `[lo,hi]` et toujours dans le HashSet). `fetch_min`,
   `fetch_max`, `fetch_add` en `Ordering::Relaxed` : ce ne sont que des
   filtres, jamais des données.

Coût : +20 lignes, 3 `AtomicUsize`, aucune allocation, aucun changement
d'ABI, aucun autre fichier `.rs` touché.

---

## 5. Gain réel avant / après (A/B entrelacé 9+9, c6, **même `jit.rs` des deux côtés**)

| Banc | Avant mur méd | Après mur méd | Facteur | Rust | Ratio avant → après |
|---|---:|---:|---:|---:|---:|
| **Global 8 domaines** | **405,161 ms** | **186,093 ms** | **2,18×** | 3,154 ms | **128,5 → 59,0 (÷2,18)** ✅ |
| Suite complexe | 332,386 ms | 326,993 ms | 1,016× | 1,819 ms | 182,7 → 179,8 |
| Englobant | 90,971 ms | 88,838 ms | 1,024× | 1,341 ms | 67,8 → 66,2 |

Horloge interne Rose sur le global : 401 → 183 ms (**2,19×**).
**Porte ≥ ×2 atteinte sur le global**, sans régression sur les deux autres
bancs. Le gain de 1,6 % sur la suite dépasse le bruit A/A mesuré (0,13 %) :
c'est un effet réel mais marginal, pas une victoire revendiquée. L'englobant
gagne 2,4 %, également au-delà de son bruit A/A (0,25 %) : même conclusion,
gain réel mais sans rapport avec la porte ×2.

---

## 6. Validations exécutées (binaire final)

```
cargo build --release --bin rose                       → OK
cargo test --release  → 220 + 204 + 6 + 1 = 431 passed, 0 failed
jardin jardins/tests/premiers.rose                     → 20 tests, 0 échec
fib(25)                                                → 75025
point fixe (rir vs rose sur compilateur/emetteur.rose) → b3ed9262dd54776a6dbd07bc12368d0e ×2
bench_global.rose rc=0, 11 valeurs identiques avant/après
9 domaines multi (9+9 c6, valeurs vérifiées)           → B/A ∈ [0,967 ; 1,022], aucune régression
```

Les 11 lignes du global sont inchangées, y compris la ligne 3 (rationnel
exact, correctif `f9b0966` préservé) et `333333383333335000000`.
Le sha256 du binaire a été relevé avant et après la salve de mesure.

**Mesures avec le `jit.rs` de HEAD (témoin interne, salve 22:19–22:21)** :
global 403,438 → 183,877 ms (÷2,19) ; suite 324,877 → 318,727 ms ;
englobant 89,378 → 88,878 ms. Le changement de `jit.rs` de DISP1 pendant la
session n'a donc pas changé le verdict.

---

## 7. Ce qui reste

1. **Le global à 186 ms reste ~59× plus lent que Rust** (3,154 ms). Le
   résidu mesuré est la boucle JIT elle-même (~162 ms de section 1 après
   optimisation) : `rose_add_ent`/`rose_mul_ent`, le `BigInt::from(b)` +
   `+=` en place sur l'accumulateur boxé, et les checks d'overflow. Le
   jumeau Rust fait du `i64` qui déborde volontairement : **la sémantique
   exacte coûte ~59× ici,pas une faute de l'interpréteur**.
2. **Suite complexe (÷1,016)** : 326,993 ms contre 1,819 ms. Cause non
   traitée : le type `Valeur::Complexe` est construit à chaque itération
   dans le chemin interprété de `interp.rs:2895–2952` (hors périmètre de
   ce sous-agent). Le JIT gère bien `JitVal::Complexe`, mais le banc passe
   aussi par `re`/`im` en appel de primitive. Aucun gain à attendre de
   `mpz_ptr` ici : il n'y a aucun grand entier dans ce banc.
3. **Englobant (÷1,024)** : 88,838 ms contre 1,341 ms. Cause : exécution
   interprétée des 4 sections (2×10⁵ tirages `hasard()` + Philox), sans
   grand entier non plus.
4. **Hypothèses non testées** (hors périmètre ou non rentables à la mesure) :
   accumulator i128 dédié dans l'ABI taggée, changement de représentation
   de `Mpz`, optimisation de `Gq::add_in`. Aucune n'est revendiquée.

---

## 8. Limites

- **c6 est un cœur X925 (capacity 997)**, pas le c11 A725 (731) du
  `COMPARATIF_TOTAL.md` : les rapports absolus (128,5× etc.) ne doivent pas
  être comparés à ceux de ce document.
- Un autre agent a modifié `jit.rs` pendant la session (détecteur
  `racine(k*k)`, +192 lignes). L'arbre a été copié dans
  `/tmp/opencode/big1_agents/tree/` avec le `jit.rs` voulu pour chaque salve
  A/B ; **aucun `.rs` d'un autre agent n'a été écrit**.
- L'instrumentation (compteurs atomiques + garde de thread-local) a été
  **retirée** : le diff final de `run.rs` ne contient que le filtre de plage.
- Aucun commit, aucune publication, aucun remplacement de `rose/rose`.

---

## 9. Commandes rejouables

```bash
cd /home/clausake/Stefan/ROSE
export PATH="$HOME/.cargo/bin:$PATH"
mkdir -p /tmp/opencode/big1_agents
(cd rose && cargo build --release --bin rose)
cp rose/target/release/rose /tmp/opencode/big1_agents/rose_big1_final
sha256sum /tmp/opencode/big1_agents/rose_big1_final

# A/B 3 bancs, 9+9, c6 (harnais écrit pour l'occasion)
cd /tmp/opencode/big1_agents
python3 bench_c6.py --bench global    --a /tmp/opencode/big1_agents/rose_big1_base_cj \
       --b /tmp/opencode/big1_agents/rose_big1_final_cj --n 9 --tag final_cj
python3 bench_c6.py --bench suite     --a ... --b ... --n 9 --tag final_cj
python3 bench_c6.py --bench englobant --a ... --b ... --n 9 --tag final_cj

# 9 domaines multi
python3 multi_ab.py /tmp/opencode/big1_agents/rose_big1_base \
                    /tmp/opencode/big1_agents/rose_big1_final

# Validations
cd /home/clausake/Stefan/ROSE/rose && cargo test --release
R=/tmp/opencode/big1_agents/rose_big1_final
$R jardins/tests/premiers.rose | tail -1
$R rir compilateur/emetteur.rose | md5sum
$R compilateur/emetteur.rose compilateur/emetteur.rose | md5sum
```

Harnais, journaux et JSON : `/tmp/opencode/big1_agents/`.
