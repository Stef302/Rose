# OPTIM_ARG_SLOTS — appels à 31/36 arguments : 2ᵉ et 4ᵉ pires cas du benchmark global

**Constat daté** : mesuré le 2026-11-26 (UTC). Worktree exclusif `/tmp/opencode/v7args`,
HEAD de départ `faaf18c296c79c3eed040b557401476a33bad6ef`, aucun commit ni push.
Garde-fou `bash scripts/garde_fou_mem.sh 12000000 …` sur toutes les compilations C0.

## 1. Résultat

| Programme | Avant (ms) | Après (ms) | Gain | Rust JIT (ms) | Cible < 2 ms |
|---|---:|---:|---:|---:|:---:|
| `t100_arg31` | 2.051 | **1.027** | 2.00 × | 1.253 | ✅ |
| `t101_arg36` | 4.237 | **1.732** | 2.45 × | 2.253 | ✅ |

Protocole : médiane de **9 tirages entrelacés A/B** avec alternance de l'ordre,
1 échauffement, `taskset -c 18`, temps mur `perf_counter_ns`. A = ELF C0 compilé
depuis le HEAD de départ, B = ELF C0 compilé depuis le source optimisé.
Contrôle A/A (même binaire) : 0.989 (`t100`), 0.986 (`t101`) — le gain est bien
au-dessus du bruit. Confirmation indépendante avec 21 tirages sur le cœur 16 :
`t100` 2.324 → 1.205 ms, `t101` 4.206 → 1.584 ms.

Binaire final : ELF aarch64 `2 732 296` octets, md5 `070625802ccb38415adb7158dc951e53`,
compilé par `/tmp/opencode/C0_i.elf` (md5 `2db168b907b4a874712857d3957fd078`, rc=0).

## 2. Cause réelle (correction du diagnostic initial)

`rose/docs/BENCH_GLOBAL.md` attribuait t101 à « l'allocation + conversion du
vecteur de paramètres ». La mesure par programmes synthétiques infirme cette
lecture :

| Programme synthétique | Avant (ms) |
|---|---:|
| `>> 1` (aucune fonction) | 0.54 |
| `fn f(…36 params…) { retour $a1 }`, **sans appel** | 1.94 |
| idem **avec appel** `f(1,…,36)` | 2.39 |
| 36 paramètres + somme des 36 arguments | 4.08 |

Donc, sur t101 : ~1,4 ms venaient du **traitement de la signature à 36
paramètres avant l'appel**, ~0,45 ms de l'appel lui-même, et ~1,7 ms de la
**chaîne de 35 additions** dans le corps. Le vecteur d'arguments n'était pas
le poste dominant.

Chronométrage par étapes (`temps()` sous `rose run`, t101, avant optimisation) :
repli source 1,57 ms ; hash + cache RIR 1,42 ms ; `parse_rir` 5,44 ms ;
`calculer_purete` 0,48 ms ; exécution 1,29 ms. Le backend G5 indexe une chaîne
**depuis l'octet 0** (`g5_emit_idx`, `g5_emit_sous` dans
`compilateur/emetteur_elf.rose`) : tout balayage `s[$i]` / `sous_chaine(s,i,j)`
en boucle est O(n²). Un RIR de ~1,5 Ko coûtait ~1,1 M d'étapes rien que pour
le découpage en lignes.

## 3. Optimisations apportées (toutes génériques)

Une seule fonction modifiée : `compilateur/moteur_complet.rose`.

1. **`call_fn_direct` — slots plats en une passe** : les $ns slots du cadre sont
   créés une seule fois, l'argument j étant déposé directement dans le slot j
   pendant l'allocation. Avant : $ns appends « v: » puis $pn appels de `lset()`
   qui reconstruisaient toute la liste (O($pn × ($base+$ns)) copies).
   Aucune arité codée en dur.
2. **`val_add` — voie rapide entier + entier** : nouveau prédicat
   `est_entier_dec` ; deux décimales courtes (≤ 15 chiffres) sont additionnées
   en `entier()`, les autres par `badd()` (exact, tout signe, tout taille).
   L'ancien chemin passait par `rat_make` + `rat_add` (2 `bgcd`, 3 `bmul`,
   2 `bdivmod`) pour `1+2`. Rationnels, chaînes et valeurs repliées `p/q`
   conservent l'ancien chemin.
3. **`mcce_fold_source` — pré-filtre exact** : une source qui *commence* par
   `fn ` / `fonction ` ne peut pas être repliée ; retour immédiat sans découper
   ni analyser tout le texte. Les sources avec commentaire initial suivent la
   voie complète (aucun faux positif).
4. **`mc_cache_chemin` — hash unique** : `mc_hashes()` était appelé deux fois
   (repli source puis chemin de cache) ; le hash déjà calculé est réutilisé.
5. **`lignes_rapides` — découpage par blocs de 64** : `parse_rir` et
   `mcce_src_lines` découpent le texte par blocs (un `sous_chaine` long par
   bloc, recherche du `'\n'` sur un bloc court). Le balayage O(n²) devient
   ~O(n²/64 + 64n) — 8 à 11× moins d'étapes sur un RIR de 1,5 Ko.
6. **`mc_hashes` — hash par blocs de 64** : même procédé, aucun changement de
   valeur (les fichiers de cache RIR existants restent valides).

## 4. Validation

| Contrôle | Résultat |
|---|---|
| Parité rc + stdout vs oracle Rust interprète (`ROSE_SANS_JIT=1`), corpus 141 | **141 / 141** |
| Parité stricte rc + stdout + stderr | 139 / 141 — 2 BLOQUES historiques uniquement : `t107_hof_var`, `t55_liste_imbriquee` (déjà documentés dans `BENCH_GLOBAL.md` et `DIAG_141.md`) |
| Divergents | 0 |
| `rose verif compilateur/moteur_complet.rose` | `vérifié`, rc=0 |
| `rose verif compilateur/emetteur.rose` | `vérifié`, rc=0 |
| Point fixe `rose rir compilateur/emetteur.rose` | `b3ed9262dd54776a6dbd07bc12368d0e`, 263 520 o |
| Point fixe `rose run compilateur/emetteur.rose compilateur/emetteur.rose` | `b3ed9262dd54776a6dbd07bc12368d0e`, 263 520 o |
| C0 → ELF (garde-fou 12 000 000 Ko) | rc=0, 2 732 296 o, md5 `070625802ccb38415adb7158dc951e53` |

Fixtures du harnais reproduites à l'identique (`/tmp/opencode/sh9_agents/battery`,
`/tmp/aotwork/entree.txt`, argument `critique_entree.txt` pour `t80_*`, `t81_*`,
`t999_*`). Données brutes : `/tmp/opencode/v7args_bench/bench_final_v5_n9.json`,
`bench_final_v5_n21.json`, `parite_v5.log`, `ab_final_v5_n9.log`.

### Note sur le JIT Rust

La consigne signalait t100/t101 comme des cas où le JIT Rust panique. Avec le
binaire présent sur cette machine (`/home/clausake/Stefan/ROSE/rose/rose`,
mode accéléré par défaut **et** sous-commandes `run` / `jit`), **aucune panique
n'a été reproduite** : rc=0 des deux côtés, sorties `31` et `666`, identiques à
l'oracle interprété (`ROSE_SANS_JIT=1`). Les programmes n'ayant pas de boucle
chaude, le JIT ne s'active probablement pas ; ils restent néanmoins chronométrés
côté Rust avec le JIT actif (1,25 et 2,25 ms). Ce constat est daté et doit être
re-mesuré si le binaire Rust change.

## 5. État du worktree

Seul `compilateur/moteur_complet.rose` est modifié (+ le présent rapport ajouté).
`compilateur/emetteur*.rose` et `rose/src/**` sont intacts. Aucun commit, aucun push.
