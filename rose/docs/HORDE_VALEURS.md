# HORDE VALEURS — 7 types Valeur portés en Rose (constat mesuré, 23/11/2026)

Sous-agent HORDE VAL. Worktree exclusif `/tmp/opencode/horde_val`, HEAD de
départ `101b48d` (`PLAN : vague 92 — J1 à J6 intégrés, bilan self-hosting`).
Trois fichiers possédés, **aucun autre fichier suivi modifié**, aucun commit,
aucune publication :

| Fichier | État | Lignes | Rôle |
|---|---|---|---|
| `compilateur/moteur_valeurs.rose` | ÉTENDU (225 → 971, +746) | 971 | tags 8..14, constructeurs, accesseurs, opérations |
| `compilateur/tests_valeurs_types.rose` | NOUVEAU | 326 | 53 assertions comparées au moteur Rust |
| `rose/docs/HORDE_VALEURS.md` | NOUVEAU | ce document | rapport |

Garde-fou utilisé sur toute commande longue :
`bash scripts/garde_fou_mem.sh 12000000 <commande>`.

---

## 1. Périmètre — ce qui a été porté

Mission : combler les 7 types `Valeur` signalés comme non modélisés dans
`rose/docs/J6_MOTEUR_ROSE.md` §4/§5/§6.1 (J6.1 « valeurs étendues ») :
**Flottant, Rationnel, Complexe, Irrationnel, Table (avancée), ChaineRep
(rope), Liste (avancée)**. Les sémantiques de référence ont été lues dans
`rose/src/run.rs:798` (enum `Valeur`, 13 variantes), `:1494` (`valeur_egale`),
`:1530` (`q_chaine`), `:1539` (`texte_rationnel`), `:1594` (`chaine_de_p`),
`:1705` (`afficher`) et `rose/src/front.rs:2859` (`texte_flottant`) —
**lecture seule, aucun fichier `rose/src/**` touché**.

Représentation : une valeur reste une `liste<entier>` plate homogène dont
l'index 0 est le tag (contrainte du vérificateur et de l'ABI G5). Les tags
0..7 de J6 sont inchangés ; les 7 nouveaux tags sont :

| Tag | Type | Forme plate | Fidélité au moteur Rust |
|---|---|---|---|
| 8 | FLOTTANT | `[8, signe, exp, cnt, d0..d(cnt-1)]` | décimal exact signe/coefficient/exposant ; `texte_flottant` reproduit sur les vecteurs décimaux (entiers, `-0.0` → `0`, 17 chiffres) |
| 9 | RATIONNEL | `[9, ns, ncnt, nd.., ds, dcnt, dd..]` | num/den **réduit par PGCD**, dénominateur normalisé > 0 ; affichage **périodique** miroir de `texte_rationnel` (`1/3` → `0.[3]`, `22/7` → `3.[142857]`) et forme **brute** `a/b` pour les tables, miroir de `q_chaine` |
| 10 | COMPLEXE | `[10, rlen, re.., ilen, im..]` (re/im = FLOTTANT) | `chaine_de_p` : `3+4i`, `i`, `2i`, `3-4i`, `11-2i` ; `+ - *`, `conj`, accesseurs `re`/`im` |
| 11 | IRRATIONNEL | `[11, signe, cnt, chiffres..]` | racine **symbolique** `√n`, carré parfait ramené à l'entier (`racine(4)` → `2`, `racine(49)` → `7`), exactement comme `run.rs:1662` |
| 12 | TABLE_AV | `[12, cnt, (klen,k..,vlen,v..)*]` | clés **et** valeurs sont des valeurs encodées (J6 tag 5 était limité à 1 clé-caractère / valeurs entières) ; insertion, remplacement, lecture, `cle_existe`, compte, affichage `[k => v, …]` |
| 13 | CHAINE_REP | `[13, n, (len,codes..)*]` | rope = segments, `concat` en O(1) par ajout de segment, `longueur`, `aplatir`, code à l'index, `sous_chaine` |
| 14 | LISTE_AV | `[14, n, (len,valeur..)*]` | éléments = **valeurs** de longueur variable (J6 tag 4 était des entiers seuls), hétérogènes, avec `idx`, `ajouter`, compte, affichage |

Discipline G5 respectée : aucune variable globale, aucun `[]` littéral vide,
pas d'affectation par index, pas de `cles`/`concatener`/`abs`/`min`/`max`,
aucune dépendance à `moteur_ops.rose` pour `rose verif` du module seul (tous
les helpers sont préfixés `tv_` et définis dans `moteur_valeurs.rose`).

### Fidélités notables au moteur Rust (reproduites, pas « corrigées »)

- `texte_rationnel` décrémente la partie entière d'un négatif à reste non nul :
  `-2/6` (réduit `-1/3`) s'affiche **`-1.[3]`** dans Rust ; le modèle rend la
  même chaîne (test 18).
- `-0.0` s'affiche **`0`** (voie « entier-valued » de `run.rs:1613`), pas
  `-0.0` (test 3).
- Les tables rendent les rationnels en **brut** `a/b` (`q_chaine`) tandis que
  les listes et l'affichage direct rendent la forme **périodique**
  (`chaine_de_p`) : `tv_valeur_texte_brut` sépare les deux chemins (tests 45, 51).
- Modèle Flottant : arithmétique **décimale exacte**. Les vecteurs de test sont
  choisis décimalement exacts (`1.5`, `0.125`, `1.5*2`, …) et leurs sorties
  sont identiques à celles du moteur Rust réel. Les artefacts d'arrondi
  binaire IEEE-754 (`0.1 + 0.2` → `0.30000000000000004`) ne font pas partie
  du modèle décimal : aucun test ne les revendique, aucun résultat n'est
  falsifié. Les rationnels, complexes et irrationnels, eux, sont exacts par
  construction (PGCD, opérations sur coefficients entiers, radicande
  symbolique) — c'est le point d'exactitude demandé.
- Seuils de voie rapide : au-delà de 12 chiffres, l'affichage rationnel
  retombe sur `a/b` exact calculé sur les chiffres décimaux (miroir de la
  garde `bits() > 126` de `run.rs:1542`), jamais sur une approximation.

---

## 2. Tests — 53 assertions, toutes comparées au moteur Rust

`compilateur/tests_valeurs_types.rose` imprime une ligne `OK n <nom>` ou
`ECHEC n <nom> | obtenu=[…] attendu=[…]`, puis `TOTAL=<n>/53`. Les chaînes
**ATTENDU** ont été mesurées sur l'interpréteur Rust de référence avec les
littéraux natifs correspondants (`ROSE_SANS_JIT=1 rose interprete`, probes
`probe*.rose` conservés hors dépôt) : `1.5`, `1000000.0`, `-0.0`,
`1.5+2.5`, `1/3`, `22/7`, `(3+4i)*(1-2i)`, `racine(2)`, `table()`, listes.

| Groupe | Tag | Assertions | Résultat |
|---|---|---|---|
| A. Flottant | 8 | 10 (1–10) | 10 OK |
| B. Rationnel | 9 | 15 (11–25) | 15 OK |
| C. Complexe | 10 | 7 (26–32) | 7 OK |
| D. Irrationnel | 11 | 7 (33–39) | 7 OK |
| E. Table avancée | 12 | 6 (40–45) | 6 OK |
| F. ChaineRep / rope | 13 | 4 (46–49) | 4 OK |
| G. Liste avancée | 14 | 4 (50–53) | 4 OK |
| **Total** | **7/7** | **53** | **53 OK, 0 ECHEC** |

Le même source est exécuté par les deux moteurs ; la parité est une
**diff octet pour octet**, pas une inspection visuelle :

```
nat.out  = 934 octets   (ROSE_SANS_JIT=1 rose interprete tests_prog.rose)
elf.out  = 934 octets   (C0_val.elf tests_prog.rose dummy > tests.elf ; ./tests.elf)
diff nat.out elf.out  → 0 différence (sortie vide, RC=0)
```

---

## 3. Preuves d'exécution (commandes réelles et résultats)

Tous les artefacts sont hors dépôt, dans `/tmp/opencode/horde_val_artifacts/`.

### 3.1 Vérification

```
cd /tmp/opencode/horde_val
rose verif compilateur/moteur_valeurs.rose            → vérifié (RC=0)
cat compilateur/moteur_valeurs.rose \
    compilateur/tests_valeurs_types.rose > tests_prog.rose
rose verif tests_prog.rose                            → vérifié (RC=0)
```
Vérifié avec les deux binaires disponibles : `/home/clausake/Stefan/ROSE/rose/rose`
(0.1.73) et le binaire rebuild du worktree `rose/target/release/rose`.

### 3.2 C0 construit depuis CE worktree, sans Rust

```
rose rir compilateur/rose_natif.rose > rose_natif.rir        # 903 626 octets
rose compilateur/emetteur_elf.rose rose_natif.rir > C0_val.elf
# C0_val.elf : 2 153 820 octets, md5 9736aaceec986f1436928e297fdd0d94
```
Garde-fou `scripts/garde_fou_mem.sh 12000000` appliqué aux deux étapes, RC=0.
Le C0 de ce HEAD attend **deux** arguments (source + arg de sortie ignoré) et
écrit l'ELF sur `/dev/stdout` (` rose_natif.rose:12310`) :
`./C0_val.elf tests_prog.rose dummy > tests.elf`.

```
tests.elf : ELF 64-bit LSB executable, ARM aarch64, statically linked,
            139 764 octets — RC=0 à l'exécution
```

### 3.3 Point fixe et validation finale

| Contrôle | Résultat mesuré |
|---|---|
| `rose rir compilateur/emetteur.rose \| md5sum` | `b3ed9262dd54776a6dbd07bc12368d0e` — **inchangé** |
| `RUST_TEST_THREADS=1 bash scripts/garde_fou_mem.sh 12000000 bash scripts/validation_finale.sh` | **RC=0, 0 échec** : 641 tests Rust (314+327+0) verts, point fixe `b3ed9262`, AOT 75/75, JIT 25/25, WASM 92 o, RVM 317811, AZ 31 CONFORME / 0 non-conforme, HOST-1 fib=317811 |

### 3.4 Cause d'un échec rencontré et résolution

Premier passage de `validation_finale.sh` : **RC=1, 1 échec**, uniquement
à l'étape 6 (batterie AZ : 31 `BLOQUE`, `rc_nat=127`). Cause **environmentale
et prouvée**, sans rapport avec le port : `tests_selfhost/lancer_az.sh` impose
`ROSE="$ROOT/rose/rose"` (`lancer_az.sh:20`) or ce worktree frais ne contenait
pas ce binaire (`timeout: failed to run command '/tmp/opencode/horde_val/rose/rose':
No such file or directory` dans `/tmp/opencode/vf_az.log`). Les six autres
étapes étaient déjà vertes, point fixe compris.

Résolution : recopier le binaire **déjà construit localement** par l'étape 1
(`rose/target/release/rose`) vers le chemin attendu `rose/rose`. Ce chemin est
**gitignore** (`.gitignore:79`), donc aucun fichier suivi n'est ajouté et
`git status --short` ne montre que les deux fichiers source possédés. Second
passage : **RC=0, 0 échec**, AZ 31 CONFORME / 0 non-conforme.

---

## 4. Taux de couverture — chiffres exacts

| Mesure | Valeur |
|---|---|
| Types visés par la mission | 7 |
| Types **modélisés, vérifiés, compilés par C0 et exécutés sans Rust** | **7 / 7 (100 %)** |
| Assertions | **53 / 53 (100 %)**, 0 échec sous les deux moteurs |
| Diff de sortie Rust vs ELF | **0 octet sur 934** |
| Lignes Rose ajoutées (module + tests) | 746 + 326 = **1 072** |
| Variantes natives `Valeur` Rust couvertes | **10 / 13** (Vide, Entier, Booleen, Chaine, Liste, Table + Rationnel, Flottant, Complexe, Irrationnel) — contre 6 / 13 avant |
| Variantes natives restantes | 3 : `Tampon`, `Tableau`, `Symbolique` (hors périmètre de cette mission) |
| Échecs de tests | **0** |
| Causes des échecs rencontrés | 1 échec environnemental (binaire AZ absent), résolu, 0 échec final |

---

## 5. État final

- `rose verif` : vert sur le module étendu et sur la concatenation avec les tests.
- C0 reconstruit depuis ce worktree : ELF produit, exécuté sans Rust, sortie
  identique à l'interpréteur Rust sur 934 octets / 53 assertions.
- Point fixe `b3ed9262` : inchangé.
- `validation_finale.sh` : **RC=0, 0 échec**.
- `git status --short` (avant ajout de ce document) :
  ` M compilateur/moteur_valeurs.rose`, `?? compilateur/tests_valeurs_types.rose`.
  Aucun autre fichier suivi modifié, aucun commit, aucun push.
