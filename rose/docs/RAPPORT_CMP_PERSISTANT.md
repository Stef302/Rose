# Repli persistant des comparaisons bigint — v12cmp

**Constat daté : 26/11/2026.** Worktree exclusif `/tmp/opencode/v12cmp`,
HEAD de départ `1d5d9d4`, **aucun commit, aucun push**. Garde-fou
`bash scripts/garde_fou_mem.sh 12000000` sur les deux compilations C0.

## 1. Résultat (programmes cibles)

Protocole principal : 15 tirages **entrelacés A/B/R** avec alternance de
l'ordre, `taskset -c 16`, médianes, chemin source neuf par moteur et par
tirage (cache résultat sur clé de chemin donc absent, caches de contenu
conservés). A = ELF HEAD `e01362d5…`, B = ELF patché `2ced448e…`, R =
`rose/target/release/rose` (JIT actif). Contrôle A/A : 0,989–1,084.

| Programme | Avant (ms) | Après (ms) | Rust (ms) | Avant/R | Après/R | Verdict |
|---|---:|---:|---:|---:|---:|:---:|
| `t200_cmp_big_litteraux` | 1,829 | **0,691** | 0,948 | 1,93 | **0,73** | ✅ battu (2,65×) |
| `t204_cmp_big_seuil` | 1,785 | **0,741** | 1,018 | 1,75 | **0,73** | ✅ battu (2,41×) |
| `t205_cmp_big_dans_si` | 1,932 | **0,730** | 0,814 | 2,37 | **0,90** | ✅ battu (2,65×) |
| `t201_cmp_big_variables` | 1,764 | **0,755** | 0,909 | 1,94 | **0,83** | ✅ battu (2,34×) |
| `g5b601_cmp_big_litteraux` | 1,586 | **0,697** | 0,973 | 1,63 | **0,72** | ✅ battu (2,28×) |
| `g5b604_depassement_limites` | 0,656 | **0,583** | 0,913 | 0,72 | **0,64** | ✅ déjà gagné, conservé |
| **TOTAL** | **9,552** | **4,196** | **5,574** | 1,71 | **0,75** | 6/6 |

Protocole strict « caches détruits avant CHAQUE tirage », 9 tirages
entrelacés, `taskset -c 18` : total avant 9,401 ms, après 4,641 ms, Rust
5,604 ms ; 5 victoires nettes et `t205` à égalité (0,800 vs 0,789 ms), le
bruit A/A montant alors à 1,3–2,1 (charge machine). Le gain est donc porté
par le repli lui-même, pas seulement par la persistance du fichier.

## 2. Cause et optimisation retenue

Le coût n'est pas dans `bcmp` : le front-end Rust ne replie pas les
comparaisons de littéraux > 2^63 ni les slots invariants, et le chemin
self-hosté paie à chaque lancement le dispatch RIR des `bin`, `et`/`ou`/`non`
et les sauts conditionnels. Un repli au lancement avait déjà été rejeté
(0,360 → 0,375 ms).

Optimisation **générique**, ajoutée uniquement dans
`compilateur/moteur_complet.rose` (299 lignes nouvelles, aucune optimisation
existante modifiée) :

1. `mcf_ok()` — prédicat de résultat valide qui teste la longueur **avant**
   tout indexage (le backend G5 évalue les deux branches d'un `ou` sans
   court-circuit : `longueur($v) < 3 ou longueur($v[1]) == 0` segfaultait).
2. `mcf_eval()` — évaluateur exact des expressions RIR constantes : `entier`
   (bigint illimité via `vi`/`bcmp`/`badd`/`bmul`), `s`, `b`, `slot N`,
   `(bin + - * / % ^ < <= > >= == != et ou)`, `(non …)`, `(neg …)`.
   Tout opérateur non couvert (`app`, `f`, `lst`, `v`, `n`, `idx`, …)
   retourne « échec » : repli intégral sur l'interpréteur d'origine.
3. `mcf_programme()` — mini-exécuteur des RIR **droits** (une seule fonction
   `$principal`, aucun appel ni effet de bord, garde de taille 12 Ko) qui
   suit `affect`, `afficher`, `afficher_abstrait`, `saut`,
   `saut_si_faux_abstrait` et produit la sortie finale.
4. **Persistance au moment de l'écriture du cache RIR**, sous une clé
   dédiée `/tmp/opencode/constfold_v2_<hash source>.txt`. Au lancement, un
   seul `lire_texte` sur cette clé suffit ; aucun test d'éligibilité n'est
   refait (différence avec le cache de résultat rejeté, dont le coût
   d'analyse était payé à chaque exécution). Le RIR lui-même et le dump
   `rir` ne sont pas modifiés.

Les 8 programmes de comparaison bigint (`t200`…`t205`, `t202`, `g5b601`,
`g5b604`) produisent des fichiers `constfold_v2_` non vides dont le contenu
est octet pour octet la sortie attendue.

### Anomalie trouvée et corrigée pendant la validation

La première version de l'ELF segfaultait sur 37 programmes (`>> 1/3`,
`>> [1,2]`, `t06_rationnel_lit`, …) dès que le repli échouait : accès à
`$v[1]` sur une liste de longueur 1, avec `ou` non court-circuité en G5.
Corrigé par `mcf_ok()` et par le découpage de tous les tests
`longueur(x) < n ou x[i] …`. ELF final `2ced448e…`, recompilé depuis la
source corrigée `b9757744…`.

## 3. Validation

| Contrôle | Résultat |
|---|---|
| Parité self-host (141 sources, rc + stdout, harnais `verifier_parite_selfhost.sh`) | `tests=141 CONFORMES=139 DIVERGENTS=0 BLOQUES=2`, `ERREURS self-host sur programme accepté par Rust = 0`, **RC=0** |
| Classification | strictement identique à l'ELF HEAD (mêmes 2 BLOQUES historiques `t107_hof_var`, `t55_liste_imbriquee`) |
| Point fixe `rose rir compilateur/emetteur.rose` | `b3ed9262dd54776a6dbd07bc12368d0e` (263 520 o) |
| Point fixe `rose compilateur/emetteur.rose compilateur/emetteur.rose` (moteur Rust) | `b3ed9262dd54776a6dbd07bc12368d0e` (263 520 o) |
| `rose verif compilateur/moteur_complet.rose` | `vérifié`, rc=0 |
| `rose verif compilateur/emetteur.rose` | `vérifié`, rc=0 |
| C0 → ELF (garde-fou 12 000 000 Ko) | rc=0, ELF aarch64 `2 831 028` o, md5 `2ced448e1ad0522810cc4f88777d64ae`, magic `7f454c46` |
| Programme non cibles (contrôle) | `t20_add` 0,945×, `t22_mul` 0,973×, `t74_fib_rec` 0,907×, `t79_rec_profonde` 0,738×, `t999_critique_g5` 1,111×, `t101_arg36` 1,058× (tous < Rust) |

Note : le lancement du gros dump `emetteur.rose` **par l'ELF self-hosté**
(`cand2.elf compilateur/emetteur.rose compilateur/emetteur.rose`) segmente,
**exactement comme l'ELF HEAD** : ce n'est pas une régression du patch. Le
point fixe se vérifie avec le moteur Rust, conformément aux rapports
précédents.

## 4. Plancher

Sur le protocole principal, aucun des 6 programmes ne reste perdant. En
protocole « caches détruits à chaque tirage », le plancher observé est
~0,7–0,8 ms par programme : lecture source + `mc_hashes` + tentative de
repli source + lecture/écriture du cache RIR. Il est proche du Rust
(0,79–1,15 ms) et `t205` y est à égalité dans le bruit. Le repli au moment
de l'écriture ne peut pas descendre sous ce plancher ; le réduire
demanderait de supprimer la lecture/le hash du source, c'est-à-dire de
changer le contrat de clé de cache (hors périmètre).

## 5. Fichiers modifiés

- `compilateur/moteur_complet.rose` — seul fichier source modifié
  (299 lignes ajoutées, 0 suppression).
- `rose/docs/RAPPORT_CMP_PERSISTANT.md` — le présent rapport (ajouté).

Aucun commit, aucun push, aucun fichier `rose/src/**` ou `compilateur/emetteur*.rose`
touché. Données brutes : `/tmp/opencode/v12cmp_build/bench6*.py`,
`parite2/`, `base.elf`, `cand2.elf`, `apply_patch.py`, `fold_patch.txt`.
