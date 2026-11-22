# C1 — goulot mémoire du front-end auto-hébergé (diagnostic + correctif)

Date : 22/11/2026. Périmètre : `scripts/generer_rose_natif.py` et les composites
générés `compilateur/rose_natif{,_pe}.rose`. `compilateur/emetteur.rose`
(point fixe `b3ed9262dd54776a6dbd07bc12368d0e`) et
`compilateur/emetteur_elf.rose` (correctif G5 d'un autre agent) sont INTACTS.

## 1. Mesures de montée en charge (avant correctif, `C0_v4.elf`)

`scripts/prof_mem_rose.sh -n 3 -m 8`, médianes ; temps réel = calcul + 0–5 s de
polling du garde-fou.

| source | taille | temps médian | pic RSS médian | note |
|---|---:|---:|---:|---|
| `/tmp/opencode/fib28.rose` | 100 o | 5,01 s | 3 672 Ko (0,00 Gio) | C0 correct : `0200b700`, 317811 |
| `thorn/src/noyau.rose` | 57 720 o | 20,05 s | 53 660 Ko | rc=1 : refus syntaxique du front-end, pas un cas mémoire |
| `compilateur/emetteur.rose` | 155 504 o | 60,15 s | 3 295 932 Ko (**3,14 Gio**) | rc=0 |
| `compilateur/rose_natif.rose` | 441 584 o | ~19 min | **18,4 Gio**, état D, swap épuisé | rc=143 (garde-fou), C1 jamais produit |

Croissance ~superlinéaire : ×2,85 de source → ×5,9 de RSS.

### Isolation par phases (variantes `quitter(0)` injectées par le générateur)

Même source de 155 Ko ; `RNAT_PHASE=<phase>` place un marqueur puis `quitter(0)`
dans la copie générée du front-end (jamais dans `emetteur.rose`).

| phase atteinte | temps | pic RSS | delta |
|---|---:|---:|---:|
| après lexage | 31,80 s | 17 960 Ko | — |
| + descente vers les pools | 42,82 s | 83 456 Ko | +65 Mo |
| + repli de constantes | 42,81 s | 84 896 Ko | +1 Mo |
| + vérification de types | 43,00 s | 161 884 Ko | +77 Mo |
| + émission RIR (`rir`) | 39,13 s | 1 517 696 Ko | **+1,36 Gio** |
| + assembly du dump RIR (`dump`) | 44,67 s | 2 829 856 Ko | **+1,31 Gio** |
| + backend G5 (complet) | 60,15 s | 3 295 932 Ko | +0,45 Gio |

**Cause exacte** : dans le binaire G5, `+` sur chaînes **copie les deux
operandes** dans une nouvelle allocation (`g5_emit_cat`, emetteur_elf.rose
l.3431+) et le tas est un allocateur bump SANS libération. La rope O(1) du
moteur Rust n'existe pas côté natif. Toute accumulation `s = s + morceau` en
boucle fuit `O(longueur²)` octets. Les deux sites dominants sont :

1. `rir_patch_op` (l.3302) : pour **chaque** op RIR, boucle sur **tous** les
   labels, avec `position_sous` (`sous_chaine` à chaque position) puis
   `remplacer_tout` qui recopie la ligne caractère par caractère —
   O(#ops × #labels × longueur), le +1,36 Gio de l'émission RIR.
2. la queue du front-end injectée par le générateur :
   `$rnat_dump = $rnat_dump + $outl[$u] + "\n"` — le +1,31 Gio du dump.

Facteurs aggravants : `piece()` reconstruit chaque morceau par `buf = buf +
s[j]`, et `em_corps`/`rir_corps` reconstruisent les chaînes d'indices de la même
manière.

Piège rencontré : la primitive native `concatener` du runtime G5 est
défectueuse (mesuré : `concatener(["a","b"])`compile mais rend `0` ; avec une
liste de ropes plus longue, segfault). Le correctif utilise donc
`sous_chaine(s, 0, longueur(s))` pour aplatir, jamais `concatener`.

## 2. Correctif injecté (copie générée uniquement)

`RNAT_FRONT_OPT=1` (défaut) dans `scripts/generer_rose_natif.py`, appliqué à
`em_head` / `em_main` / la queue avant concaténation avec le backend :

| injection | effet |
|---|---|
| `rnat_join(p, d, f)` | concaténation **équilibrée** d'une liste de morceaux : O(n·log n) octets alloués au lieu de O(n²). |
| `rnat_flat(s) = sous_chaine(s, 0, longueur(s))` | aplatit une chaîne accumulée en une seule feuille. |
| `rir_patcher` : table `label -> cible` construite **une fois**, passée à `rir_patch_op` ; `rir_patch_op` réécrit en **un seul balayage** de la ligne (`@` + identifiant → `cle_existe`/`$tab[$lab]`) puis `rnat_join`. | supprime le O(#ops × #labels) — le gain décisif. |
| `remplacer_tout` : morceaux dans une liste + `rnat_join`. | plus de recopie O(longueur²) par remplacement. |
| `piece()` : recherche des séparateurs puis **une seule** `sous_chaine`. | plus de `buf = buf + s[j]`. |
| `em_corps` / `rir_corps` : découpage par positions de virgules + `sous_chaine`. | idem sur les corps d'instructions. |
| `$topb`, `$topg`, `$fcorps[*]` aplatis via `rnat_flat` quand l'accès est terminé. | indexation caractère par caractère en O(1). |
| dump RIR : `$outl[$u] + "\n"` dans une liste, puis `rnat_join`. | plus de `$rnat_dump = $rnat_dump + …`. |

`RNAT_FRONT_OPT=0` désactive tout le correctif (A/B). `RNAT_PHASE=` ( Diagnostic,
défini plus haut) et `RNAT_OUT_ELF=` permettent de régénérer une variante
instrumentée dans `/tmp` sans écraser le composite livré. Ces variables sont
sans effet sur la génération normale.

## 3. Après correctif (`C0_opt.elf`, même chaîne RIR + backend)

| source | temps | pic RSS | gain |
|---|---:|---:|---:|
| fib28 (100 o) | < 1 s | ~2,6 Mo | C0 inchangé : `0200b700`, 317811, `cmp=OK` vs `fib_ref.elf` |
| `compilateur/emetteur.rose` (155 Ko) | 60,15 s (médiane 3 runs) | **635 968 Ko (0,61 Gio)** | **3,14 → 0,61 Gio (−80,6 %)** |
| `compilateur/rose_natif.rose` (441 Ko) | 433,23 s | **3 756 096 Ko (3,58 Gio)** | **18,4 → 3,58 Gio (−80,5 %)**, sous la cible 4 Gio |

Le C1 produit est un ELF valide : 1 602 056 octets, magic `7f454c4602010100`
(même taille et même magic que C0_opt, issue de la même source générée).
Confirmation 3 runs en cours dans `/tmp/opencode/prof_nat_final.log`.

Phases intermédiaires avec le correctif, sur 441 Ko :
après vérification 281 404 Ko ; après émission RIR 4 551 936 Ko (avant la
dernière injection `rir_patcher`, qui a précisément divisé ce chiffre).

## 4. Non-régressions

- `python3 scripts/generer_rose_natif.py` : OK, composites régénérés
  (442 293 o ELF / 213 097 o PE dans la version finale).
- `rose verif compilateur/rose_natif.rose` et `..._pe.rose` : **vérifié**.
- Point fixe `rose rir compilateur/emetteur.rose | md5sum` =
  `b3ed9262dd54776a6dbd07bc12368d0e` (inchangé).
- SHA-256 des sources possédées intactes :
  `emetteur.rose` = `77181a8a…90a2e`, `emetteur_elf.rose` = `1c36e4cf…0aa53`.
- Batterie AZ : `tests=31 CONFORMES=31 DIVERGENTS=0 BLOQUES=0`,
  comparaisons `natif/ELF=31/31`, `RIR=31/31`, `ELF=31/31`, `sortie=31/31`.
- fib : ELF octet-pour-octet identique à la référence ; sortie `317811`.

## 5. Ce qui reste à l'orchestrateur

Relancer la chaîne C0/C1/C2 complète avec le composite livré (non fait ici,
conformément à la consigne). Pic attendu C1 ≈ 3,6 Gio, donc le garde-fou peut
rester à 8 Gio. Le backend G5 (déjà corrigé par un autre agent) contribue
environ 0,4 Gio du pic ; la voie PE n'a pas été mesurée en C1 (le correctif lui
est appliqué par le même générateur).
