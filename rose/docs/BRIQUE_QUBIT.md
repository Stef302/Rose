# BRIQUE_QUBIT — simulateur logique de qubit en Rose pur

> Date : 25/11/2026. Worktree `v5uni3` (HEAD `a43dab8`). Fichiers :
> `briques/quantique/qubit.rose`, `briques/quantique/qubit.bud`,
> `briques/tests_qubit.rose`. Aucun commit, aucun push.

## 1. Doctrine : un simulateur, pas un appareil

La capacité « quantique » n'était qu'un mot dans `briques/appareil/contrat.bud`
(code 5, appareil abstrait `noeud_quantique`). Elle devient une **brique réelle** :
un simulateur de qubits écrit **en Rose pur**, qui n'a aucun rapport avec du
matériel.

Règles constitutionnelles, vérifiables dans le source :

- **Aucune primitive matérielle** : ni `in`/`out`, ni MMIO, ni assembleur, ni
  syscall, ni appel à un périphérique. Aucun nom d'architecture, d'OS, de bus,
  de contrôleur ni de format de fichier dans le code.
- **Aucune architecture supposée** : cible `GENERIQUE`, boutisme et largeur de
  mot déclarés dans le `.bud` uniquement comme données de construction.
- **L'état est une valeur** : `liste<entier>`. Les portes sont des fonctions
  pures qui rendent une liste nouvelle. Aucun effet de bord, aucune variable
  globale lue dans une fonction (garde-fou JIT/C0), aucun état caché.
- **Le hasard est arithmétique** : PRNG local Park-Miller, graine passée en
  argument. Même graine → même mesure, dans l'interpréteur, le JIT, le moteur
  self-hosté et l'ELF C0.
- **Pas de dépendance au moteur** : ni `hasard()` native, ni FFI, ni GMP, ni
  libc, ni linker externe. Rose concatène la brique au programme (pas de
  modules dans le sous-ensemble C0).

Positionnement : c'est une brique **mathématique/logique**, pas un pilote et
pas un backend. Elle modèle des amplitudes ; elle ne pilote rien.

## 2. Représentation

| Concept | Donnée Rose | Détail |
|---|---|---|
| Précision | `qubit_precision()` → `10000` | échelle fixed-point `P`, constante de code (et champ `simulateur.precision` du `.bud`) |
| Amplitude réelle `a` | entier `a*P` arrondi | négatifs permis |
| État 1 qubit | `[alpha, beta]` | `alpha² + beta² ≈ P²` après normalisation entière |
| État 2 qubits | `[c00, c01, c10, c11]` | même échelle, norme ≈ `P²` |
| Angle | entier en degrés | ramené dans `[0,360)` ; table cos/sin à pas de 15°, interpolation linéaire entière, puis renormalisation |
| Norme | `qubit_racine(n)` | racine carrée entière (Newton), plus grand `s` tel que `s² ≤ n` |

La normalisation estime `s = racine(alpha²+beta²)`, puis remplace chaque
amplitude par `a*P ÷ s` (division entière Rose, arrondie). `P = 10000` donne
`1/√2 → 7071` et `p(1) = 5000` pour l'état Hadamard.

## 3. API

| Fonction | Rend | Rôle |
|---|---|---|
| `qubit_precision()` | `entier` | précision `P = 10000` |
| `qubit_racine(n)` | `entier` | racine carrée entière |
| `qubit_nouveau(theta)` | `[alpha, beta]` | état pur à l'angle `theta` (degrés) |
| `qubit_porte(q, nom)` | `[alpha, beta]` | `X`, `Z`, `H`, `Y` ; nom inconnu → `[-1, 0]` |
| `qubit_probabilite1(q)` | `entier` | `beta²*P/(alpha²+beta²)`, scaled `[0,P]` |
| `qubit_tirer(graine, precision)` | `entier` | PRNG local → `[0, precision)` |
| `qubit_mesurer(q, graine)` | `0` ou `1` | mesure déterministe ; état de norme nulle → `-1` |
| `qubit_intriquer(q1, q2)` | `[c00, c01, c10, c11]` | état de Bell : H(q1) ⊗ q2 puis CNOT |
| `qubit_mesurer2(etat, graine)` | `[m1, m2]` | mesure des deux qubits ; norme nulle → `[-1,-1]` |

Portes, en données :

- `X` : `[beta, alpha]` (permutation exacte) ;
- `Z` : `[alpha, -beta]` (phase relative) ;
- `H` : `[(alpha+beta)/√2, (alpha-beta)/√2]` (seule porte qui renormalise) ;
- `Y` : `[beta, -alpha]`, c'est-à-dire `-i·Y` sur le sous-espace réel : la
  phase globale n'est pas observable et n'est pas stockée.

PRNG : `x = (graine*16807 + 12345) mod (2^31-1)`, `r = x mod P`. Le modulo
négatif est corrigé explicitement. Ce générateur est **rejouable**, pas
cryptographique.

## 4. Manifeste `.bud` (données seules)

`briques/quantique/qubit.bud` ne contient aucune logique : ni `si`, ni boucle,
ni fonction. Champs notables : `simulateur.precision = 10000`,
`simulateur.graine = 12345`, `simulateur.portes = ["X","Z","H","Y"]`,
`simulateur.prng = "park-miller-local"`, `capacite.nom = "quantique"`,
`interface.primitives_materielles = faux`.

Vérifié avec le lecteur réel `compilateur/lecteur_bud.rose` :
`bud_valider(...) = 0`, **52 paires** lues, `simulateur.precision = 10000`,
`simulateur.portes.0 = X`, `simulateur.portes.3 = Y`,
`capacite.nom = quantique`, `interface.primitives_materielles = 0`.

Changer ce `.bud` ne modifie aucune ligne de `qubit.rose`. Le code n'a pas
besoin de le lire à l'exécution : c'est une déclaration de construction.

## 5. Compilation et tests

```bash
cd <racine du dépôt>
ROSE=/home/clausake/Stefan/ROSE/rose/rose
cat briques/quantique/qubit.rose briques/tests_qubit.rose > /tmp/q.rose
$ROSE verif /tmp/q.rose                    # vérifié
$ROSE /tmp/q.rose                          # interpréteur / JIT
ROSE_SANS_JIT=1 $ROSE interprete /tmp/q.rose
/tmp/opencode/C0_141.elf /tmp/q.rose > /tmp/q.elf
chmod +x /tmp/q.elf && /tmp/q.elf          # ELF autonome
```

`rose verif briques/tests_qubit.rose` **seul** échoue (« fonction
`qubit_nouveau` inconnue ») : Rose n'a pas de modules dans ce sous-ensemble,
le pilote doit être concaténé à la brique — comme `briques/tests_arena.rose`.

### Sorties mesurées (100 tirs, graines 0..99)

| Contrôle | Résultat |
|---|---|
| `qubit_nouveau(0)` | `[10000, 0]` |
| `qubit_nouveau(90)` | `[0, 10000]` |
| `qubit_nouveau(45)` | `[7071, 7071]` |
| `H|0>` | `[7071, 7071]`, `qubit_probabilite1 = 5000` |
| H puis 100 mesures | **49 uns / 51 zéros** (~50/50, déterministe) |
| `X|0>` puis 100 mesures | `[0, 10000]`, **100 uns / 0 zéro** |
| `Z·H|0>` | `[7071, -7071]` |
| `Y|0>` | `[0, -10000]` |
| `H²|0>` | `[10000, 0]` (involution) |
| porte inconnue `"T"` | `[-1, 0]` (refus) |
| `qubit_intriquer(|0>,|0>)` | `[7071, 0, 0, 7071]` |
| 100 mesures de l'état de Bell | **100/100 corrélations** `m1 == m2` (49 `|00>`, 51 `|11>`) |

Les 29 lignes de sortie sont identiques octet pour octet sous l'interpréteur,
le JIT et l'ELF : `md5 = 061a3e9ca69906bdd08ccaa0801e83e0` des deux côtés
(`cmp` = 0). L'ELF C0 pèse **29 100 octets**, lié statiquement, sans libc.

### Non-régression du compilateur

| Contrôle | Mesure |
|---|---|
| `rose verif briques/quantique/qubit.rose` | `vérifié` (rc 0) |
| `rose verif /tmp/q.rose` | `vérifié` (rc 0) |
| Parité self-hostée 141 sources | `tests=141 CONFORMES=141 DIVERGENTS=0 BLOQUES=0`, `ERREURS=0`, verdict RC=0 (4 s, moteur `c0f90c6ebfb6…`) |
| Point fixe `rose rir compilateur/emetteur.rose \| md5sum` | `b3ed9262dd54776a6dbd07bc12368d0e` (inchangé) |

Harnais : `bash scripts/verifier_parite_selfhost.sh --moteur
/tmp/opencode/moteur_141.elf --rose-bin /home/clausake/Stefan/ROSE/rose/rose
--workdir /tmp/opencode/v5uni3/parite_qubit`. Aucun fichier suivi par git
n'a été modifié par ces mesures.

## 6. Limites explicites

1. **Aucun vrai qubit physique** : tout est calculé sur des entiers Rose. Aucun
   appel à un processeur quantique, aucun accès matériel, aucun cloud, aucun
   backend. La capacité `quantique` de `contrat.bud` reste abstraite.
2. **Précision entière** : `P = 10000`, erreur de représentation ≈ `1/P` par
   amplitude. `1/√2 ≈ 7071/10000`. Les statistiques approchent 50/50 sans
   l'atteindre nécessairement (49/51 mesuré) ; ce n'est pas un défaut de
  hasard matériel.
3. **Pas d'amplitudes complexes** : seul le sous-espace réel est simulé. La
   porte `Y` est implémentée comme `-i·Y` (phase globale non stockée). Les
   phases relatives entre branches via signes sont conservées ; les rotations
   arbitraires autour de Y avec phase imaginaire ne le sont pas.
4. **Angles approximés** : table à pas de 15° + interpolation linéaire.
   Un angle quelconque (par ex. 37°) n'est pas trigonométriquement exact.
5. **Intrication limitée** : `qubit_intriquer` construit un état de Bell avec
   H + CNOT. Ce n'est pas un simulateur de circuit universel : pas de
   registre N-qubits, pas de portes contrôlées arbitraires, pas de
   décohérence, pas de canaux quantiques.
6. **PRNG non cryptographique** : Park-Miller est suffisant pour rejouer des
   statistiques, pas pour produire de l'aléa sécurisé.
7. **Concaténation obligatoire** : le pilote de test n'est pas autonome ; il
   doit être concaténé à `qubit.rose` (convention des autres briques).

Ce simulateur est une **brique de départ** : il rend la notion de qubit
manipulable en Rose dès aujourd'hui, sans présupposer le moindre matériel.
