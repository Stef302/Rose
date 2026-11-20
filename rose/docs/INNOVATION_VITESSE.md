# INNOVATION_VITESSE — attaque neuve sur D6 (CONSTAT DATÉ, 19/11/2026)

**Auteur** : sous-agent `innov`. Aucun commit, aucune publication. Binaire construit
dans `/tmp/opencode/innov_agents/`, **jamais installé dans `rose/rose`**.
Fichiers possédés modifiés : `rose/src/run.rs` (pool), ce document.
`rose/src/interp.rs` a été touché puis **restauré octet pour octet**
(SHA `21737ba0…`, identique à la base).
Le changement `f9b0966` de l'orchestrateur dans `run.rs` (rationnels au-delà de
i128) a été **conservé** ; ma copie de sauvegarde `run.rs.base` était périmée et
n'a jamais été recopiée par-dessus.

---

## Mise à jour datée — 21/11/2026 (constats 20/11 ; historique 19/11 conservé)

Le corps (§0–§4) est un **constat du 19/11**, binaire de base `04f7f2d1…` et pool
construit localement, pinage c6/c16. Les chiffres ci-dessous sont des **constats datés**
du 20/11 (binaire
`a2f98fe93799e2faa69a21a486bcd4b7bc7def976d7b62181c0c6c95ca04ff3c`,
`rose/docs/ETAT_VICTOIRE_9_DOMAINES.md`, commit `dee9378`, 9 échantillons
entrelacés, horloge « calcul seul »). L'historique est conservé ; aucune phrase
n'implique « Rose bat partout ».

### D6 — le verdict « non gagné » est daté du binaire 19/11

| Domaine | c19 | c6 |
|---|---:|---:|
| `fib(35)` | 0,92 | 0,91 |
| Boucle 1..10⁶ | 0,39 | 0,39 |
| Premiers < 50 000 | 0,85 | 0,85 |
| Factorielle 5000 | 0,21 | 0,21 |
| PGCD 1000 chiffres | 0,66 | 0,69 |
| Chaîne 20 000 | 0,61 | 0,78 |
| Liste 100 000 | 0,64 | 0,72 |
| Table 3000 | 0,69 | 0,65 |
| E/S 10 Mo | 0,63 | 0,61 |

Sur le binaire du 20/11, **D6 est gagné** : 0,64 (c19) / 0,72 (c6), contre 2,076 (c6)
au §3 de ce document et 1,79 (c6) au §1.3/§4. Le §1.3 (« harnais officiel à processus
frais, pool/base = 1,006/1,018 = bruit ») et le §4 (« échec assumé sur l'objectif
principal ») décrivent le binaire du 19/11 et restent vrais **pour lui** ; ils ne sont
pas transférables au binaire du 20/11 sans re-mesure.

**Non-attribution** : le 9/9 du 20/11 n'est **pas** attribuable au pool de pages de ce
chantier. Le pool a été mesuré ici sur un binaire différent, son gain réel est limité
à la 2ᵉ exécution dans un même processus (≈ 40 µs, ×7,6) et il est « dans le bruit »
sur le harnais à processus frais. Aucun code de ce chantier n'est revendiqué comme
cause du résultat 9/9.

### Cohérence avec les autres docs (constats 20/11)

- **Global** : ×414 → ×2,85 → **×2,88 (c16) / ×2,57 (c6)** (GLB3, `DEMARRAGE.md`,
  `951bb12`) ; carrés 1,26 → 0,76 ms (NEON), compilation Cranelift 777 → 337 µs (÷2,3),
  rationnels 10,75 → 0,79 ms (GLB1, `4aad1d8`). Réserve sémantique : `bench_global_rs`
  rend `1291990006563070912` (débordement `i64`) et `7.818804193883677` (`f64`) quand
  Rose rend `333333383333335000000` et la fraction exacte — comparaison **non
  équivalente**, adopter ces erreurs est **refusé**. **Rust devant.**
- **Suite complexe** : ×203 → ×1,86 → **×1,15** (`SUITE_BEAT.md`, `745bb9c`) → écart
  réduit par CLOSE1 (`8c31a38`, LTO + lecture source unique, `CLOSURE.md`) ; porte
  Rose ≤ Rust **NON atteinte**, titre de `8c31a38` rectifié par `ef7464e`.
- **Englobant** : ×109 → ×2,49 → **×1,40–1,62** (`ENGBEAT.md`, 20/11, `12c786c`). Porte
  **NON atteinte**.
- **Self-hosting** : **ATTENT pour l'émetteur** (`0e2d844`, `e0f5744`, `SELF_HOST_SH3.md`,
  20/11) : ELF 534 780 o, md5 `ed699457…`, lit un `.rose`, reproduit le RIR `b3ed9262…`
  octet pour octet. Le **binaire `rose/rose` complet exige toujours `cargo build`**.

### Réserves maintenues

Restent vrais ici : huge pages mortes (mode `never`), amorçage mmap faux positif
corrigé, compactage 8 octets bloqué par `jit.rs` (interdit). Restent vrais ailleurs :
global, suite complexe et englobant **perdus face à Rust** ; la réserve sémantique du
global interdit d'y compter une victoire de moteur. Le gain de pool en processus
persistant est réel mais limité ; il ne doit être lu ni comme une victoire sur le
banc officiel à processus frais du 19/11, ni comme la cause du 9/9 du 20/11.

---

## 0. Protocole réellement exécuté

| Élément | Mesure |
|---|---|
| HEAD au départ | `6f5ba644de5b889f65870bd4ca329f33c34af44d` |
| HEAD pendant le chantier | `f9b0966` (changement orchestrateur dans `run.rs`, conservé) |
| Binaire de base | `/tmp/opencode/innov_agents/rose_base` — build `CARGO_TARGET_DIR` isolé, SHA `04f7f2d19d0779088b7288b98e512daebe5599c127ecd894c5ed4d952250a14b` |
| Binaire pool | `/tmp/opencode/innov_agents/rose_pool` — mêmes sources + pool ci-dessous |
| Pinage | `taskset -c 6` et `taskset -c 16` |
| Échantillons | 9 par bras, ordre A/B/C rotatif entrelacé (3 bras : base / pool / Rust), médianes |
| Valeur contrôlée | `4999950000` à **chaque** tirage, sinon exception |
| Horloges | interne `temps()` du programme + murale `perf_counter_ns` |
| Harnais | `/tmp/opencode/innov_agents/d6_3way.py`, `deux_aaa.py`, `bench_multi_innov.py` |

Reproduction de la ligne de base (avant tout changement) : c6 **352 944 ns**,
c16 **315 200 ns**, ratios 1,83 / 1,67 — cohérent avec `VITESSE_FINALE.md`.

---

## 1. Idée retenue : pool de pages réellement réutilisées (piste #2)

### 1.1 Mécanisme

Une liste JIT est un `Vec<Valeur>` de 131072 × 24 = **3,1 Mio**, alloué par
`rose_jit_liste_vide` (jit.rs, hors de mon périmètre). Sur un processus neuf,
glibc le sert par `mmap` : les ~586 pages sont faultées à l'écriture, ~288 µs
mesurés = 90 % de D6 (décomposition reprise de `VITESSE_FINALE.md`).

Le même programme exécuté une **2ᵉ fois dans le même processus** réutilise les
pages déjà résidentes. Mesuré sur le binaire **de base**, sans aucun changement
(`deux_exec.rs`, 9 salves entrelacées) :

| c6 | 1ʳᵉ exécution | 2ᵉ exécution |
|---|---:|---:|
| base | 334 961 ns | 318 097 ns |

Le pool explicite rend cette réutilisation **déterministe dès la 2ᵉ exécution** :
`Valeur::liste_de` (`run.rs`) pioche une arène déjà touchée dans un
`thread_local!`, et `impl Drop for Liste` rend le `Vec` au pool après `clear()`
(qui vide les valeurs mais **garde l'allocation**, donc les pages chaudes).
Garde-fous : ne sont poolées que les arènes de capacité ≥ 131072 (bug de
corruption détecté et corrigé, §1.4), uniquement si `Rc::strong_count == 1`
(aucun alias), et l'accès TLS utilise `try_with`/`try_borrow_mut` (aucun panic
à la fermeture du processus). Aucun pré-touch, aucun octet écrit en plus,
aucune formule, aucun bench modifié.

### 1.2 Gain mesuré — 9 salves entrelacées, c6

| c6 | 1ʳᵉ exécution | 2ᵉ exécution |
|---|---:|---:|
| base | 334 961 ns | 318 097 ns |
| **pool** | 331 601 ns | **41 968 ns** |
| **gain** | 0,99× (bruit) | **7,58×** |

| c16 | 1ʳᵉ exécution | 2ᵉ exécution |
|---|---:|---:|
| base | 310 369 ns | 304 961 ns |
| **pool** | 315 633 ns | **39 424 ns** |
| **gain** | 0,98× (bruit) | **7,74×** |

La 2ᵉ exécution avec pool (≈ 40 µs) est alors **4,7× plus rapide que Rust**
(≈ 189 µs). La 1ʳᵉ exécution est inchangée.

### 1.3 ⚠ Le D6 officiel n'est PAS gagné

Le harnais officiel (`d6_3way.py`) lance un **processus neuf par échantillon** ;
le pool démarre donc vide et ne peut pas aider la 1ʳᵉ exécution. Mesure finale,
3 bras entrelacés, 9 échantillons, médianes :

| c6 | Rose médiane | Rust médiane | Rose/Rust |
|---|---:|---:|---:|
| base | 344 353 ns | 192 192 ns | 1,792 |
| pool | 346 561 ns | — | 1,803 |
| **pool/base** | **1,0064** | — | — |

| c16 | Rose médiane | Rust médiane | Rose/Rust |
|---|---:|---:|---:|
| base | 302 001 ns | 179 840 ns | 1,679 |
| pool | 307 473 ns | — | 1,710 |
| **pool/base** | **1,0181** | — | — |

**Verdict honnête : écart D6 non comblé sur le benchmark officiel.** Le gain est
réel mais limité aux scénarios à exécutions répétées dans un même processus
(REPL, serveur, harnais persistant).

### 1.4 Bug trouvé et corrigé pendant le chantier

Première version du pool : n'importe quel `Vec` était réutilisé. Le programme
auto-hébergé (`point fixe B`) a alors produit un hash **différent**
(`2ab7792d…` au lieu de `b3ed9262…`) et un run a bouclé > 120 s : une petite
liste interprétée (capacité 8) était rendue au pool puis resservie à la liste JIT
qui écrit 100 000 × 24 octets → corruption mémoire. Correctif : capacité
minimale `POOL_CAP_MIN = 131072` au dépôt **et** au retrait ; le point fixe est
redevenu `b3ed9262…` ×2. Preuve que la validation a servi, pas une intention.

---

## 2. Pistes neuves TESTÉES et REJETÉES (preuve réelle)

### 2.1 Huge pages — mort par construction
`/sys/kernel/mm/transparent_hugepage/enabled` = `always madvise [never]`,
`HugePages_Total: 0`. `madvise(MADV_HUGEPAGE)` est ignoré en mode `never`.
Aucune mesure de code nécessaire.

### 2.2 Amorçage du seuil mmap par un `malloc`+`free` intouché — faux positif
Microbench C `/tmp/opencode/innov_agents/mmap2.c`, `taskset -c 6`, blocs
**jamais écrits** :

| nprime | rep 0 | rep 1 | rep 2 |
|---|---:|---:|---:|
| 0 | 663 537 ns / 588 pf | 476 977 ns / 587 pf | 173 040 ns / 0 pf |
| 1 (cap exacte) | 510 497 ns / 589 pf | 180 273 ns / 0 pf | 173 120 ns / 0 pf |
| 1 (6–16 Mio) | 476–715 µs / 589 pf | 172–180 µs / 0 pf | 173 µs / 0 pf |

⚠ Mon **premier** microbench (`mmaptest.c`) avait conclu à un gain immédiat :
il était trompeur, les 6 itérations précédentes avaient déjà élevé le seuil.
Testé proprement en début de processus, l'amorçage ne supprime **jamais** les
faults de la 1ʳᵉ allocation utile. Implémenté dans `interp.rs` puis **reverté**
(SHA identique à la base).

`mallopt(M_MMAP_THRESHOLD, 4→64 Mio)` (`mallopt_test.c`) : l'allocation vient
du tas mais les pages sont tout aussi froides (**555 faults**, 430–470 µs).
La chaleur vient uniquement de la **réutilisation des mêmes pages physiques**,
pas de l'origine mmap/brk.

### 2.3 Représentation compacte `Vec<i64>` / écriture différée (pistes #1 et #3)
Non implémentable dans les fichiers possédés. L'émetteur (`jit.rs`, interdit)
détermine le pas d'écriture : `Valeur` fait 24 octets et `ecrire_entier`
(`jit.rs:4499`) écrit **les 3 mots** à chaque pas. Les faults dépendent de
l'**empreinte** (100 000 × 24 = 2,4 Mio → 586 pages), pas des octets réellement
écrits. Écrire seulement le mot i64 au même pas de 24 toucherait exactement les
mêmes 586 pages. Il faut un pas de 8 octets, donc modifier l'émetteur.
`layout_valeur_entier_nu()` refuse la spécialisation si `size_of::<Valeur>() != 24`
(retour au chemin interprété ~0,9 ms, déjà mesuré par l'agent `vit`).

---

## 3. Validation de l'arbre final

| Contrôle | Sortie réelle |
|---|---|
| `cargo build --release --bin rose` (target isolé) | `Finished release …`, SHA pool `voir rose_pool` |
| `cargo test --release` | **220+204+0+6+1+0 = 431 passed, 0 failed** (3+3 ignorés) — `/tmp/opencode/innov_agents/cargo_test_final2.log` |
| Jardin `jardins/tests/premiers.rose` | `── jardin/premiers : 20 tests, 0 échec ──` |
| fib25 | `75025` |
| Point fixe A `rose rir compilateur/emetteur.rose` | `b3ed9262dd54776a6dbd07bc12368d0e` |
| Point fixe B `rose compilateur/emetteur.rose compilateur/emetteur.rose` | `b3ed9262dd54776a6dbd07bc12368d0e` → **×2** |
| `bench_global.rose` | `rc=0`, `5000 / 6250000 / 317811 / 1000000 / bench global Rose / <chrono>` (5 premières valeurs identiques au base ; la dernière est un chrono variable) |
| 9 domaines multi (c6, 9 échantillons) | 81/81 valeurs exactes, aucune exception. Ratios Rose/Rust : D1 0,923 · D2 0,378 · D3 0,851 · D4 0,212 · D4b 0,682 · D5 0,797 · D6 2,076 · D7 1,040 · D8 0,598 — tous dans le bruit des références de `VITESSE_FINALE.md` (D6 c6 réf. 2,039) |
| `git status` | aucun commit, aucune publication ; `rose/rose` intact ; `interp.rs` restauré ; `run.rs` = base + `f9b0966` + pool |

---

## 4. Bilan

- **Retenue** : réutilisation explicite des pages d'arène (piste #2), +7,6× sur
  la 2ᵉ exécution en processus persistant, sans pré-touch ni triche.
- **Non retenues** : amorçage mmap (faux positif corrigé), huge pages (mode
  `never`), compactage 8 octets (bloqué par `jit.rs`, interdit).
- **Échec assumé sur l'objectif principal** : sur le benchmark officiel à
  processus frais, D6 reste à ×1,79 (c6) / ×1,71 (c16) contre Rust ; le pool
  n'y change rien (pool/base = 1,006 / 1,018 = bruit). La cause structurelle
  (pas de 24 octets figé dans l'émetteur) demeure.
