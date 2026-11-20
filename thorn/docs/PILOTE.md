# Thorn — pilote CLI (PROV4)

Mission TH1 (21/11/2026) : faire passer les 5 preuves de démonstration de
0/5 à 5/5, vérifiées par le noyau. Fichiers modifiés par CETTE mission :
`thorn/src/principal.rose`, `thorn/src/tactiques.rose`,
`thorn/docs/PILOTE.md`. `thorn/src/analyse.rose`, `thorn/src/noyau.rose`,
`thorn/src/lexeur.rose`, `rose/src/**` et les fichiers de preuves n'ont PAS
été modifiés. Aucun commit, aucune publication.

Note de contexte factuelle : un chantier parallèle a renommé les preuves
`.slg` en `.thn` et a été commité localement (`30f217b`, `2123200`) pendant
cette mission. Les 5 preuves sont donc désormais `thorn/preuves/*.thn` ;
leur contenu est inchangé.

## Commandes implémentées

```
./rose/rose thorn/src/principal.rose <fichier.thn> [--trace]
./rose/rose thorn/src/principal.rose --demo [fichiers.thn...] [--trace]
./rose/rose thorn/src/principal.rose --aide     # aussi -h, --help
```

`/tmp` de vérification : utiliser `./rose/rose` (0.1.6, présent dans le
dépôt) ; le `rose` installé dans le PATH (0.1.5) ne connaît pas `verif`.

| Option | Effet réel |
|---|---|
| `<fichier.thn>` | vérifie chaque `théorème`, affiche `OK <nom>` ou `ÉCHEC <nom> : <raison>`, puis `N théorème(s), M réussi(s)` |
| `--demo` | joue les `.thn` connus de `thorn/preuves/` (ou les fichiers passés en argument) |
| `--aide` | aide en français |
| `--trace` | affiche `[trace] <nom> but = …` et `[trace] <nom> terme = …` |

Codes de sortie : **0** tout passe, **1** au moins un échec de preuve,
**2** lecture/lexage/analyse impossible ou aucun théorème.

## Format `.thn` accepté

```
# commentaire
théorème <nom> : <type du but>
preuve <tactique ou terme>
fin
```

La preuve est **soit** un terme direct (`preuve type 0`), **soit** une
suite de tactiques. Les tactiques acceptées sont `reflexivite`,
`symetrie`, `introduction <nom>`, `application <terme>` / `appliquer
<terme>`, `induction <nom>` suivie de blocs `cas <motif> <params...> :
…`, `simplifier` (avec alias). Les arguments de `introduction` et
`appliquer` sont utilisés ; les noms de lieurs effectivement introduits
viennent du Π du but (le noyau tranche).

Pipeline par théorème : lecture → lexage (PROV2) → découpage → analyse du
but (PROV2) → **normalisation syntaxique** vers le format canonique →
`verifier(but, ctx)` (PROV1) → séquenceur de tactiques (PROV3) →
`verifier(terme, ctx)` (PROV1) → `convertible(type, but)` (PROV1) →
verdict. Aucun résultat n'est codé en dur : les 5 `OK` sont tous obtenus
après `verifier()` **et** `convertible()`.

## Cause 1 — format d'égalité (reflexivite, symetrie)

`analyse.rose` rend `a = b` sous la forme `ind("=", [a, b])` ;
`tactiques.rose` cherchait `ind("Eq", [A, a, b])`. Le parseur n'a pas été
modifié (les 8 tests de `tests_syntaxe.rose` figent son format). C'est
`tactiques.rose` qui a été aligné : `ta_eq(A,a,b)` garde sa signature mais
rend désormais `noy_ind("=", ta_params2(a,b))` — le type `A` est porté par
le contexte du noyau. `ta_refl_rec` et `ta_sym_rec` testent `nom == "="` et
lisent les deux membres ; `A` est inféré par `ta_infer_A()` depuis le
contexte. `ta_normalise()` rembourre les listes de params du parseur
(`[a,b]` → `[sentinelle,a,b]`) et mappe `Nat`→`noy_ind("Nat",…)`, `0`→
`ta_zero()`, `+`→`ta_plus_symbole()`.

Preuve : `ÉCHEC … réflexivité : le but n'est pas une égalité` → `OK
reflexivite`, `OK symetrie`.

## Cause 2 — signature `Nat`

`Nat` était analysé comme `var "Nat"` libre : le noyau répondait
`variable non liee: Nat` dès la vérification du but. `ta_normalise()`
appliquée au but analysé dans `pilote_executer()` le remplace par le
terme de signature `noy_ind("Nat", [sentinelle])`. Le test
`convertible(domaine, ta_nat())` de l'induction peut alors réussir.

Preuve : `ÉCHEC associativite_plus : type invalide — variable non liee:
Nat` → `OK associativite_plus`, `OK commutativite_plus`.

## Cause 3 — scripts multi-tactiques

Le pilote ne lisait que le premier jeton après `preuve`. Il contient
maintenant un séquenceur (`p_parse_cmds`, `p_exec_cmds`, `p_exec_cas`) qui
propage but, contexte, squelette de terme et trou ouvert à chaque étape ;
il compose les tactiques résiduelles, dépouille les Π annoncés par
`cas succ k ih` dans l'ordre (`λk` extérieur, `λih` intérieur), et
élabore les arguments implicites des lemmes (`trans h1 h2` → `trans A a
b c h1 h2`) en décomposant les types synthétisés par le noyau
(`ta_elab`, `ta_eq_parts`).

Deux pièges corrigés et mesurés :
- `noy_subst()` (capture-évitante) renommait `A` en `A'` lors du
  remplissage d'un trou, cassant la référence `A` dans le domaine : le
  terme final avait un `A` libre. Le remplissage utilise donc
  `p_subst_brut()` : les noms de trous sont générés frais, aucun
  conflit de capture n'est possible.
- la liste des paramètres de `cas` est une liste de chaînes non
  rembourrée ; la traiter comme rembourrée ne dépouillait qu'un Π sur
  deux et laissait un `p` libre puis un `k` libre.

Preuve : `ÉCHEC identite : … variable non liee: ?` → `OK identite`.

## Cause 4 — ordre des imports

Imposé par Rose : `tactiques`, `analyse`, `noyau`, `lexeur` (un autre
ordre donne « Fonction 'lexer_table' inconnue »). L'ordre existant de
`principal.rose` a été conservé ; aucun import n'a été réordonné.

## `--demo` : avant / après (sorties réelles)

AVANT (5 théorème(s), 0 réussi(s), rc=1) :

```
=== thorn/preuves/identite.slg ===
ÉCHEC identite : terme de preuve invalide — variable non liee: ?
1 théorème(s), 0 réussi(s)
=== thorn/preuves/reflexivite.slg ===
ÉCHEC reflexivite : réflexivité : le but n'est pas une égalité
1 théorème(s), 0 réussi(s)
=== thorn/preuves/symetrie.slg ===
ÉCHEC symetrie : symétrie : le but n'est pas une égalité
1 théorème(s), 0 réussi(s)
=== thorn/preuves/associativite.slg ===
ÉCHEC associativite_plus : type invalide — variable non liee: Nat
1 théorème(s), 0 réussi(s)
=== thorn/preuves/commutativite.slg ===
ÉCHEC commutativite_plus : type invalide — variable non liee: Nat
1 théorème(s), 0 réussi(s)
5 théorème(s), 0 réussi(s)
```

APRÈS (`./rose/rose thorn/src/principal.rose --demo`, rc=0) :

```
=== thorn/preuves/identite.thn ===
OK identite
1 théorème(s), 1 réussi(s)

=== thorn/preuves/reflexivite.thn ===
OK reflexivite
1 théorème(s), 1 réussi(s)

=== thorn/preuves/symetrie.thn ===
OK symetrie
1 théorème(s), 1 réussi(s)

=== thorn/preuves/associativite.thn ===
OK associativite_plus
1 théorème(s), 1 réussi(s)

=== thorn/preuves/commutativite.thn ===
OK commutativite_plus
1 théorème(s), 1 réussi(s)

5 théorème(s), 5 réussi(s)
```

Rose n'a aucune primitive d'énumération de répertoire : `--demo` utilise
un manifeste (`pilote_manifeste()`) filtré par `lire_texte`. Des chemins
explicites peuvent être passés après `--demo`.

## Vérifications réellement exécutées (20/11/2026)

```
$ ./rose/rose verif thorn/src/analyse.rose        → vérifié (rc=0)
$ ./rose/rose verif thorn/src/tactiques.rose      → vérifié (rc=0)
$ ./rose/rose verif thorn/src/principal.rose      → vérifié (rc=0)

$ ./rose/rose thorn/tests_noyau.rose       REUSSIS=35 ECHOUES=0 RESULTAT: OK (rc=0)
$ ./rose/rose thorn/tests_syntaxe.rose     OK = 8   ECHEC = 0 (rc=0)
$ ./rose/rose thorn/tests_tactiques.rose   REUSSIS=19 ECHOUES=0 RESULTAT: OK (rc=0)
$ ./rose/rose thorn/tests_pilote.rose      REUSSIS=12 ECHOUES=0 RESULTAT: OK (rc=0)

$ cd rose && ~/.cargo/bin/cargo test
  test result: ok. 258 passed; 0 failed; 3 ignored
  test result: ok. 242 passed; 0 failed; 3 ignored
  test result: ok. 6 passed;   0 failed
  test result: ok. 8 passed;   0 failed
  test result: ok. 1 passed;   0 failed
  → 515 passed, 0 failed (rc=0)
```

## Dépendances utilisées

| Fonction | Module |
|---|---|
| `verifier`, `convertible`, `beta`, `noy_*` | `thorn/src/noyau.rose` (PROV1) |
| `lexer` | `thorn/src/lexeur.rose` (PROV2) |
| `analyser` | `thorn/src/analyse.rose` (PROV2) |
| `reflexivite`, `symetrie`, `introduction`, `application`, `induction`, `simplifier`, `ta_normalise`, `ta_resoudre` | `thorn/src/tactiques.rose` (PROV3) |
| `p_parse_cmds`, `p_exec_cmds`, `p_exec_cas`, `p_subst_brut` | `thorn/src/principal.rose` (PROV4) |

Les sorties ci-dessus sont des constats datés : à re-mesurer après tout
changement de module.
