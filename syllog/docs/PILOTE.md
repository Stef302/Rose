# Syllog — pilote CLI (PROV4)

Fichiers livrés : `syllog/src/principal.rose`, `syllog/docs/PILOTE.md`,
`syllog/tests_pilote.rose`. Aucune modification de `rose/src/**`, du noyau,
du lexeur, de l'analyseur ou des tactiques.

## Commandes

```
rose syllog/src/principal.rose <fichier.slg> [--trace]
rose syllog/src/principal.rose --demo [fichier.slg ...] [--trace]
rose syllog/src/principal.rose --aide
```

Le pilote s'exécute dans la philosophie Rose : un seul fichier source, aucune
dépendance externe, tous les messages en français.

| Option | Effet |
|---|---|
| `<fichier.slg>` | vérifie chaque déclaration `théorème` du fichier |
| `--demo` | joue les preuves du dossier `syllog/preuves/` |
| `--aide` (`-h`, `--help`) | affiche l'aide en français |
| `--trace` | affiche les termes construits (`[trace] ...`) à chaque étape |

## Format `.slg` attendu

```
théorème <nom> : <type du but>
preuve <tactique | terme>
```

Exemples réels dans `syllog/preuves/` :

```
théorème reflexivite : pour_tout A:type, pour_tout x:A, Eq A x x
preuve reflexivite
```

```
théorème identite : pour_tout A:type, pour_tout x:A, A
preuve fn A:type => fn x:A => x
```

Les lignes commençant par `#` sont des commentaires (le lexeur les ignore).
Le mot `théorème` admet aussi l'écriture sans accent `theoreme` ; `preuve`
admet `preuv` et `proof`.

Tactiques reconnues (noms français, avec alias) : `reflexivite`
(`réflexivité`, `refl`), `symetrie` (`symétrie`, `sym`), `introduction`
(`intro`), `application` (`app`), `induction` (`ind`), `simplifier`.
Si le jeton après `preuve` n'est pas une tactique, il est traité comme un
TERME de preuve et confié à `analyser`.

## Pipeline exact, pour chaque théorème

1. Lecture du fichier (`lire_texte`) ; échec ⇒ code 2, message français.
2. Lexage (`lexer` du module PROV2) ; échec ⇒ code 2.
3. Découpage du flux de jetons en déclarations (mot réservé `théorème`),
   analyse du but (`analyser` du module PROV2).
4. **Vérification du TYPE du théorème** par le noyau (`verifier`), avec le
   contexte vide.
5. **Exécution de la preuve** par les tactiques du module PROV3
   (`reflexivite`, `symetrie`, `introduction`, `application`, `induction`,
   `simplifier`). Chaque tactique construit un terme et le fait vérifier par
   le noyau ; le pilote contrôle aussi qu'il ne reste aucun but ouvert.
   Quand la preuve est un terme direct, il est analysé sans tactique.
6. **Vérification du TERME de preuve final** par le noyau (`verifier`), puis
   comparaison du type synthétisé au but avec `convertible`.
7. Affichage de `OK <nom>` ou `ÉCHEC <nom> : <raison>`, puis du bilan
   `N théorème(s), M réussi(s)`.

Aucune étape n'est simulée : le pilote appelle les modules des autres agents
par les signatures de `syllog/SPEC.md` et n'implémente aucune règle de
typage, d'analyse ou de tactique lui-même.

## Codes de sortie

| Code | Sens |
|---|---|
| 0 | tous les théorèmes passent |
| 1 | au moins un échec de preuve |
| 2 | lecture impossible, lexage/analyse impossible, ou aucun théorème |

Les erreurs sont attrapées par `essayer ... attraper` : aucun message
d'erreur Rust, aucun panic intentionnel.

## Mode `--demo` — limite honnête

Rose n'expose **aucune primitive pour énumérer un répertoire**. Le mode
`--demo` ne peut donc pas « scanner » `syllog/preuves/` tout seul : il
utilise un **manifeste** de noms connus, filtré par `lire_texte` (les noms
absents sont silently ignorés). Le manifeste est maintenu dans
`pilote_manifeste()` quand PROV3 ajoute des fichiers `.slg`. Deux
échappatoires sont prévues :

* `rose syllog/src/principal.rose --demo syllog/preuves/x.slg` joue
  explicitement les fichiers passés en arguments ;
* si aucun fichier du manifeste n'est lisible, le pilote affiche
  `Erreur : aucun fichier .slg jouable dans syllog/preuves/ (dossier absent ou vide)`
  et sort avec le code 2.

Le bilan de `--demo` est la somme des bilans des fichiers joués, précédé
d'un en-tête `=== <chemin> ===` par fichier.

## Tests bout en bout

```
rose syllog/tests_pilote.rose
```

`syllog/tests_pilote.rose` importe le pilote et appelle directement
`pilote_executer` / `pilote_demo` / `pilote_aide` sur des fichiers temporaires
écrits dans `/tmp/opencode/`. Il compare les sorties **exactes** (fichier
vide, théorème résolu par réflexivité), vérifie le code 2 d'une lecture
impossible, la présence des lignes `[trace]`, le bilan de `--demo` et la
présence de l'aide. Il affiche `TEST OK <nom>` / `TEST ÉCHEC <nom>` par test
et finit par `N test(s) réussi(s) sur 6`, avec code 0 ou 1.

## Dépendances sur les autres modules (PROV1–PROV3)

| Fonction appelée | Module | Rôle |
|---|---|---|
| `verifier`, `convertible`, `noy_ctx_vide`, `noy_champ`, `noy_genre`, `noy_erreur`, … | `syllog/src/noyau.rose` (PROV1) | synthèse de types, conversions, constructeurs de termes |
| `lexer` | `syllog/src/lexeur.rose` (PROV2) | jetons français/unicode |
| `analyser` | `syllog/src/analyse.rose` (PROV2) | but et terme de preuve |
| `reflexivite`, `symetrie`, `introduction`, `application`, `induction`, `simplifier` | `syllog/src/tactiques.rose` (PROV3) | construction des preuves |

`rose verif syllog/src/principal.rose` doit répondre `vérifié` dès que
`syllog/src/analyse.rose` est livré ; avant cela, l'import échoue avec
`importer : fichier introuvable 'syllog/src/analyse.rose'`. C'est un blocage
externe documenté, pas une simulation.

## État des vérifications (à re-mesurer après tout changement de module)

*Les sorties réelles obtenues sont consignées dans le rapport final du
chantier ; ce fichier décrit le contrat, pas un état figé.*
