# Plafond de vitesse de Rose APRÈS les vagues 9–10 — mesures et cible chiffrée

**Horodatage des salves** : 2026-11-18T14:43:34+01:00 (salve 1) et
2026-11-18T14:45:48+01:00 (salve 2 de confirmation). Deux salves **A/A/A
entrelacées**, **9 échantillons par cas**, médianes, hash du binaire contrôlé
**avant et après chaque salve**.

**Binaire mesuré** : `rose/rose` SHA256
`ab01f8fd43f4fda304706697d1fe9e91981b50746d86ab282f8412770d665ee1`
(md5 `5f97012ca8ddbdc4b62dcf0731a02ac1` ; identique avant/après dans les deux
salves, et identique au binaire final de `BATTRE_RUST.md`). C'est le binaire
qui contient H1 (chemin rapide `isub`) et I1 (transformation
récursion → boucle) — **pas** les travaux « types rapides » et G2 annoncés en
cours dans la vague 10 : `git status` montrait `rose/{Cargo.toml,src/jit.rs,
src/lib.rs,src/main.rs,src/run.rs}` modifiés non commités à 14:47:38, sans
rebuild du binaire. **Tout chiffre ci-dessous est un constat daté de ce binaire
précis** : re-mesurer après tout rebuild. Ce document n'a modifié aucun autre
fichier du dépôt et n'a fait aucun commit.

**Machine [M]** : aarch64, cœur 15 = Cortex-X925 (`/proc/cpuinfo`,
`CPU part : 0xd85`), `scaling_cur_freq = 3900000 kHz`, `scaling_governor =
performance` lu dans `/sys/devices/system/cpu/cpu15/cpufreq/`,
`lscpu : CPU max MHz = 3900.0000`. Fréquence retenue **3,900 GHz**. Toutes les
exécutions sont pinées par `taskset -c 15` (les variantes « 20 cœurs »
explicitement non pinées).

**Légende** : **[M]** = mesuré dans ces salves (sortie vérifiée à chaque
échantillon) ; **[E]** = calculé, formule affichée ; **[H]** = chiffre daté
d'une source, non re-mesuré ici.

---

## 1. Avant / après — tableau des mesures

Deux salves indépendantes ; la colonne « après » est la **médiane des deux
médianes de salve**. Écarts inter-salves : boucle JIT 13,872 / 13,885 ms
(0,09 %), natif 3,465 / 3,466 ms (0,03 %), fib Rose 19,888 / 19,927 ms
(0,20 %), fib Rust 12,665 / 12,612 ms (0,42 %).

| Cas (médiane interne, sauf démarrage) | Avant [H] `PLAFOND_VITESSE.md` | Après [M] binaire `ab01f8f…` | rapport |
|---|---:|---:|---:|
| Boucle 1M, émetteur natif M3.1 | 3,459 ms | **3,465 ms** | 1,00× (stable) |
| Boucle 1M, JIT Cranelift | 13,848 ms | **13,878 ms** | 1,00× (stable) |
| Boucle 1M, interpréteur RIR | 14,510 ms | **14,970 ms** | 1,03× (bruit) |
| `fib(35)`, JIT Cranelift | 51,30 ms (binaire `8a4ac9…`) | **19,908 ms** | **×2,58** |
| `fib(35)`, Rust `rustc -O` | 12,63 ms [H] | **12,638 ms** | 1,00× (stable) |
| `factorielle(5000)`, primitive native | non mesuré | **0,369 ms** (piné) | — |
| `factorielle(5000)`, Rust naïf base 1e9 | non mesuré | **7,829 ms** | Rose 24,7× [M] |
| `factorielle(1e6)`, primitive native, 20 cœurs | 196,15 ms | **216,57 ms** | 1,10× (plus de charge) |
| `factorielle(1e6)`, primitive native, pinée c15 | 1 026,76 ms | **1 022,61 ms** | 1,00× (stable) |
| Bande passante copie, 1 cœur | 28,86 Go/s | **28,25 Go/s** | 0,98× |
| Bande passante lecture, 1 cœur | 17,73 Go/s | **17,60 Go/s** | 0,99× |
| Bande passante copie, 20 cœurs | 129,7 Go/s | **121,2 Go/s** | 0,93× (charge) |
| Démarrage, programme vide (harnais Python) | 1,057 ms | **1,053 ms** | 1,00× |
| Démarrage, `>> 2+3` (harnais Python) | 1,062 ms | **1,174 ms** | bruit |

Horloge de démarrage séparée, `rose debug-lenteur -n 9` [M] : lancement
(`rose version`) **224–225 µs**, programme minimal **316–337 µs**,
`petit.rose` **379 µs**. Le harnais Python ajoute ~0,8 ms de `subprocess` :
ne pas confondre les deux horloges.

**Constat** : les vagues 9–10 n'ont pas changé le chemin boucle ni la
factorielle. Le seul gain réel est sur `fib(35)` : **51,30 → 19,91 ms
(×2,58)**, exactement le gain attribué à I1 dans `BATTRE_RUST.md`. La cible
« dépasser Rust » n'est pas atteinte : **19,908 / 12,638 = 1,575×**.

---

## 2. Plafonds théoriques mis à jour

### 2.1 Plafond CPU de la boucle entière [E]

Une seule dépendance `s += k` (addition i64, latence 1 cycle) par itération :

> 10⁶ itérations / 3,9 GHz = **0,2564 ms**

[M] `gcc -O2` (`n` lu de `argv`, somme vérifiée) = **0,256447 ms** sur 9
échantillons → **1,0001× le plancher** : le plancher machine est confirmé par
la mesure, pas seulement calculé.

| Chemin [M] | Temps | / plancher 0,2564 ms | / C -O2 |
|---|---:|---:|---:|
| C -O2 | 0,2564 ms | 1,00× | 1,00× |
| Rose natif M3.1 | 3,465 ms | **13,51×** | 13,51× |
| Rose JIT | 13,878 ms | **54,13×** | 54,12× |
| Rose interprété | 14,970 ms | **58,38×** | 58,38× |

Gains encore **possibles** [E] : ×4,01 pour amener le JIT au niveau du natif,
×13,5 pour amener le natif au plancher C, ×54 pour amener le JIT au plancher
absolu. Aucun de ces gains n'est mesuré aujourd'hui.

### 2.2 Coût minimal d'un appel après transformation récursion → boucle [E] + [M]

Nombre d'appels [E], calculé sur la structure du code machine dumpé (un `bl`
par itération de la boucle) :

```
T(0)=T(1)=0 ; T(n) = Σ_{a = n-1, n-3, … ≥ 1} (1 + T(a))
T(35) = 14 930 351 = F(36) - 1      (récursion naïve : 29 860 703 = 2·F(36)-1)
```

- **Plancher absolu [E]** : 1 cycle par appel (un `bl` + retour surflux,
  aucune autre instruction critique) → 14 930 351 / 3,9 GHz =
  **3,828 ms**. C'est une borne optimiste, jamais atteinte avec un appel réel.
- **Coût pratique de l'ABI directe [M]** : Rust -O, même transformation,
  même cœur : 12,638 ms / 14 930 351 = **0,8465 ns/appel =
  3,301 cycles/appel**. Son corps de boucle est **6 instructions, 0 `mov` de
  permutation** (`objdump -d`, `sub / bl / sub / add / cmp / b.hi`), soit un
  IPC effectif de 6 / 3,301 = **1,817** [E].
- **Rose aujourd'hui [M]** : 19,908 ms / 14 930 351 = **1,3334 ns/appel =
  5,200 cycles/appel**. Corps de boucle dumpé (`ROSE_DUMP_CODE`, 0x1028–
  0x1054) : **13 instructions dont 6 `mov` de permutation**
  (0x1028, 0x1030, 0x1034, 0x103c, 0x1044, 0x1050), `n` n'étant pas maintenu
  dans un registre callee-saved stable. IPC effectif 13 / 5,200 = **2,500** [E].

| Cas | ns/appel [M] | cycles/appel [M/E] | instr./appel [M] |
|---|---:|---:|---:|
| Rose JIT | 1,3334 | 5,200 | 13 (6 `mov`) |
| Rust -O | 0,8465 | 3,301 | 6 (0 `mov`) |
| Plancher 1 cycle [E] | 0,2564 | 1,000 | — |

### 2.3 Plafond mémoire [E] avec bande passante [M]

Bande passante mesurée cette salve : copie 1 cœur **28,25 Go/s**, lecture
1 cœur **17,60 Go/s**, copie 20 cœurs **121,2 Go/s** (3 salves, 512 Mio
par palier, checksum `volatile` pour empêcher l'élimination des accès).

**Taille des résultats [M] + [E]** : `1 000 000!` = 5 565 709 chiffres →
5 565 709 × log₂10 = **18 488 885 bits = 2,20 MiB** ; `5000!` = 16 326
chiffres → 54 234 bits = **6,62 KiB**.

**Plancher I/O strict de la dernière fusion de `1e6!` [E]** : opérandes [H]
`i128x.rs:183` = 7 727 004 + 10 761 882 bits, résultat 18 488 885 bits →
trafic minimal lire A + lire B + écrire C = **4,41 MiB** :
- à 121,2 Go/s (20 cœurs) → **0,0381 ms** ;
- à 28,25 Go/s (1 cœur) → **0,1636 ms**.

**Plancher de trafic NTT de la dernière fusion [E]** : limbs u32 =
(7 727 004 + 10 761 882)/32 = 577 778 → N = 2²⁰ = 1 048 576 ; coefficient
u64 = 8 o → 8,0 MiB/tableau ; 20 étages × (R+W) = 320 MiB/transformée ;
6 transformées (2 modules × [2 directes + 1 inverse]) = **1,88 GiB** :
- à 121,2 Go/s → **16,6 ms** ; à 28,25 Go/s → **71,3 ms**.

**Plancher NTT de toute la factorielle [E]** (17 niveaux d'arbre, 8,3–11,3 Go
de trafic, formule de `PLAFOND_VITESSE.md` §4.5 réutilisée comme borne) :
**68–93 ms** à 121,2 Go/s ; **294–400 ms** à 28,25 Go/s.

**Passage mémoire élémentaire de `5000!` [E]** : 2 × 6,62 KiB à 28,25 Go/s =
**0,5 µs**.

---

## 3. Distance au plafond et gains restants

| Cas | Meilleur [M] | Plancher [E] | Distance | Gains encore possibles [E] |
|---|---:|---:|---:|---|
| Boucle 1M, natif M3.1 | 3,465 ms | 0,2564 ms | **13,5×** | ~×13,5 en passant à un émetteur registre-à-registre |
| Boucle 1M, JIT | 13,878 ms | 0,2564 ms | **54,1×** | ×4,0 vers le natif, puis ×13,5 |
| `fib(35)`, Rose JIT | 19,908 ms | 3,828 ms (1 cycle/appel) | **5,20×** | ×1,575 suffit pour battre Rust ; ×5,20 borne absolue |
| `fib(35)`, Rust -O | 12,638 ms | 3,828 ms | **3,30×** | référence empirique de l'ABI directe |
| `1e6!`, 20 cœurs | 216,57 ms | 93 ms (trafic NTT total) | **2,32×** | passes NTT / CRT / retenues |
| `1e6!`, 1 cœur | 1 022,61 ms | 400 ms (trafic NTT total) | **2,56×** | idem |
| `1e6!`, I/O strict | 216,57 ms | 0,0381 ms | **5 679×** | inaccessible sans changer d'algorithme (le calcul NTT domine) |
| `5000!`, Rose | 0,369 ms | 0,0005 ms (1 passage mémoire) | **769×** | coût des multiplications, pas de la mémoire |
| Démarrage `2+3` | 0,224 ms (horloge Rust) | < 1 µs de calcul | — | dominé par fork/exec : optimiser le calcul n'aurait aucun effet |

Détail décisionnel pour `fib(35)` :
- écart à combler **7,269 ms = 57,5 %** [M] ;
- économie nécessaire **≥ 0,4869 ns/appel = 1,899 cycles/appel** [E] ;
- Rose a **6 `mov` de permutation par itération** de plus que Rust [M,
  objdump]. Si (hypothèse **non mesurée**) chacun de ces `mov` occupe un cycle
  sur le chemin critique, **2 `mov` retirés suffisent** à passer sous Rust ;
  retirer les 6 ramènerait vers la borne basse de 3,83 ms, mais cette
  additivité n'est pas prouvée ( Rose atteint déjà 2,50 IPC effectif contre
  1,82 pour Rust : une partie des `mov` est probablement éliminée au renommage).

---

## 4. Verdict — peut-on battre Rust sur `fib(35)` ?

**PROUVÉ [M]** : non aujourd'hui. Sur le binaire `ab01f8f…`, même machine,
même cœur, même transformation récursion → boucle, même nombre d'appels
(14 930 351), Rose met **19,908 ms** contre **12,638 ms** à Rust -O, soit
**1,575× plus lent**. Le gain de I1 (×2,58) a été confirmé, mais il s'arrête
à 1,58× de la cible.

**PROUVÉ [E] par le calcul** : le plafond théorique (3,828 ms à 1 cycle/appel)
est **3,30× sous le temps de Rust** et **5,20× sous celui de Rose**. Donc,
*oui, l'architecture actuelle a la marge théorique* : il n'y a aucun mur
physique qui interdise à Rose de battre Rust sur ce cas, à condition de
supprimer au moins ~1,9 cycle/appel (~2 `mov` critiques).

**ESTIMÉ, NON PROUVÉ** : le changement nécessaire est un **émetteur machine à
registres stables** — maintenir `n` dans un registre callee-saved (comme `x19`
chez Rust) du prologue à la fin de la boucle, au lieu des 6 `mov` de
permutation actuels — avec une **ABI directe** déjà en place (`bl` direct,
pas de `rose_set_ret_tag`, pas de trampoline sur le chemin rapide). C'est un
travail d'allocation de registres / d'émission, **pas** une optimisation de
compilation supplémentaire et **pas** un changement d'algorithme : l'algorithme
de Rust a déjà été reproduit exactement (nombre d'appels identique).

**CE QUI N'EST PAS ACQUIS** : aucun résultat mesuré ne montre Rose sous Rust.
L'estimation « 2 `mov` suffisent » suppose que ces `mov` sont sur le chemin
critique ; le passage de 19,9 ms à < 12,6 ms doit être **mesuré** (9
échantillons entrelacés, hash avant/après) avant d'être annoncé. Les travaux
« types rapides » et G2 en cours au moment de la salve n'étaient pas dans le
binaire mesuré : leur effet éventuel sur ce chemin reste à mesurer, pas à
supposer.

**Cible chiffrée réaliste pour la suite** : faire passer `fib(35)` de
**19,91 ms à < 12,64 ms** (gain ≥ **1,58×**, économie ≥ **0,49 ns/appel**)
en éliminant au moins 2 des 6 `mov` par itération ; objectif de fond
**~10,7 ms** [E] si la boucle tombe à 7 instructions (~6 + `bl`) en gardant
l'IPC effectif actuel de 2,5, puis **3,83 ms** [E] comme borne absolue
(inatteignable avec un `bl` par appel). La boucle 1M reste l'autre chantier : **3,47 → ~0,26 ms**
(×13,5) exigerait le même émetteur registre-à-registre.

---

## 5. Reproduction

```bash
# Salve 1 (hash, démarrage, 3 chemins, fib, fact 5000/1e6, C -O2, bande passante)
cd /tmp/opencode/plafond && PIN_CPU=15 python3 plafond_apres_mesure.py
# Salve 2 (confirmation A/A/A + Rust fact5000 + debug-lenteur)
cd /tmp/opencode/plafond && PIN_CPU=15 python3 plafond_apres_confirm.py
# Tous les calculs [E] ci-dessus, entrées [M] lues dans les JSON
python3 /tmp/opencode/plafond/calcul_plafond_apres.py

# Preuve code machine (boucle fib JIT et Rust)
cd /home/clausake/Stefan/ROSE/rose
ROSE_DUMP_CODE=/tmp/opencode/plafond/fib35_code.bin taskset -c 15 ./rose /tmp/opencode/plafond/fib35.rose
objdump -D -b binary -m aarch64 /tmp/opencode/plafond/fib35_code.bin | sed -n '15,60p'
objdump -d --start-address=0x90bc --stop-address=0x9110 /tmp/opencode/rosebench/pure_rs

sha256sum /home/clausake/Stefan/ROSE/rose/rose   # doit donner ab01f8fd… pour ces chiffres
```

Données brutes : `/tmp/opencode/plafond/results_apres.json`,
`results_apres_salve2.json`, `calculs_apres.txt`, `apres_salve1.log`,
`apres_salve2.log`. Chaque mesure est un **constat daté** : re-mesurer après
tout rebuild du binaire ou tout changement de topologie.
