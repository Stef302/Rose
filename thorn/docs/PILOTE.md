# Thorn — pilote CLI (PROV4)

Fichiers livrés par ce chantier : `thorn/src/principal.rose`,
`thorn/tests_pilote.rose`, `thorn/docs/PILOTE.md`. Aucun autre fichier
n'a été modifié par ce chantier ; aucune publication n'a été faite.
Deux commits LOCAUX ont été créés pendant le chantier par un agent parallèle
(`8bc15ca`, `c833118`) ; aucun n'a été poussé. La correction finale du
manifeste `--demo` est incluse dans `c833118` ; `git status --short thorn`
est propre au dernier contrôle.

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
**2** lecture/lexage/analyse impossible ou aucun théorème. Aucun `panic`
n'est déclenché : les lectures et les appels au parseur sont protégés par
`essayer … attraper`.

## Format `.thn` accepté

```
# commentaire
théorème <nom> : <type du but>
preuve <tactique ou terme>
fin
```

`théorème`/`theoreme`, `preuve`/`preuv`/`proof` sont acceptés. `fin` est
le terminateur facultatif du bloc de preuve. Le PILOTE découpe le flux de
jetons produit par `lexer` sur ces mots réservés ; le but est confié à
`analyser`, la preuve est soit une tactique (`reflexivite`, `symetrie`,
`introduction`, `application`, `induction`, `simplifier`, avec alias),
soit un terme analysé par `analyser`.

Pipeline par théorème : lecture → lexage (PROV2) → découpage → analyse du
but (PROV2) → `verifier(but, ctx)` (PROV1) → tactique (PROV3) ou terme de
preuve → `verifier(terme, ctx)` (PROV1) → `convertible(type, but)` (PROV1)
→ verdict. Aucune étape n'est simulée.

## Vérifications réellement exécutées (21/11/2026)

```
$ ./rose/rose verif thorn/src/principal.rose
vérifié

$ ./rose/rose verif thorn/tests_pilote.rose
vérifié

$ ./rose/rose thorn/tests_pilote.rose
✓ aide : texte exact
✓ fichier OK : sortie exacte
✓ fichier OK : code 0
✓ fichier ÉCHEC : préfixe
✓ fichier ÉCHEC : bilan
✓ fichier ÉCHEC : code 1
✓ fichier absent : code 2
✓ trace : étiquette
✓ trace : terme
✓ demo : en-tête
✓ demo : bilan global
✓ demo : code 0
REUSSIS=12
ECHOUES=0
RESULTAT: OK
(rc=0)
```

Fichier OK utilisé par les tests (écrit dans `/tmp/opencode/` par le test) :

```
théorème t_ok : type 1
preuve type 0
fin
```

Sortie réelle :

```
OK t_ok
1 théorème(s), 1 réussi(s)
(rc=0)
```

Aide réelle (`./rose/rose thorn/src/principal.rose --aide`, rc=0) :

```
Thorn — pilote de démonstration (en Rose)

Usage :
  rose thorn/src/principal.rose <fichier.thn>  vérifie chaque théorème
  rose thorn/src/principal.rose --demo [fichiers.thn...]
  rose thorn/src/principal.rose --aide

Options :
  --trace  affiche les termes construits

Format d'un fichier .thn :
  théorème <nom> : <type du but>
  preuve <tactique>
Tactiques : reflexivite, symetrie, introduction, application,
induction, simplifier. La preuve peut aussi être un terme.

Sortie : OK <nom> ou ÉCHEC <nom> : <raison>, puis
« N théorème(s), M réussi(s) ».
Codes : 0 tout passe, 1 échec de preuve, 2 lecture/parsing.
```

## `--demo` : sortie réelle et limite

Rose n'a aucune primitive d'énumération de répertoire. `--demo` utilise
donc un manifeste de noms (`pilote_manifeste()`), filtré par `lire_texte` ;
les fichiers absents sont ignorés silencieusement, et si aucun n'est
lisible le pilote affiche
`Erreur : aucun fichier .thn jouable dans thorn/preuves/ (dossier absent ou vide)`
et sort avec le code 2. Des chemins explicites peuvent être passés après
`--demo`.

Sortie réelle de `./rose/rose thorn/src/principal.rose --demo` (rc=1) :

```
=== thorn/preuves/identite.thn ===
ÉCHEC identite : terme de preuve invalide — variable non liee: ?
1 théorème(s), 0 réussi(s)

=== thorn/preuves/reflexivite.thn ===
ÉCHEC reflexivite : réflexivité : le but n'est pas une égalité
1 théorème(s), 0 réussi(s)

=== thorn/preuves/symetrie.thn ===
ÉCHEC symetrie : symétrie : le but n'est pas une égalité
1 théorème(s), 0 réussi(s)

=== thorn/preuves/associativite.thn ===
ÉCHEC associativite_plus : type invalide — variable non liee: Nat
1 théorème(s), 0 réussi(s)

=== thorn/preuves/commutativite.thn ===
ÉCHEC commutativite_plus : type invalide — variable non liee: Nat
1 théorème(s), 0 réussi(s)

5 théorème(s), 0 réussi(s)
```

## Blocages réels entre modules (constats, pas des simulations)

Ces échecs ne sont pas masqués : ils viennent d'écarts de représentation
entre les modules livrés en parallèle.

1. **Égalité** : `analyse.rose` traduit `a = b` en `ind("=", [a, b])`
   (2 paramètres), alors que `tactiques.rose` cherche `ind("Eq", [A, a, b])`
   (3 paramètres). D'où « le but n'est pas une égalité » pour
   `reflexivite.thn` et `symetrie.thn`.
2. **Signature** : `Nat` est analysé comme une variable libre ; le noyau
   répond `variable non liee: Nat`. Les démos PROV3 construisent leurs
   termes dans Rose avec une signature explicite, absente des `.thn`.
3. **Scripts de tactiques** : `identite.thn` enchaîne `introduction A`,
   `introduction x`, `application x`. Le pilote n'interprète que la
   première tactique du bloc (les API SPEC ne permettent pas de composer
   les buts résiduels sans utiliser des fonctions hors contrat), d'où
   « variable non liee: ? ». Les preuves multi-lignes ne sont donc PAS jouées.
4. **Ordre des imports** : Rose doit importer `tactiques`, `analyse`,
   `noyau`, `lexeur` dans cet ordre ; avec `lexeur` avant `analyse`,
   `analyser_source` échoue avec « Fonction 'lexer_table' inconnue ».

Le pilote ne corrige aucun de ces écarts : il rapporte l'échec tel quel.
Un `.thn` autonome (`type 1` / `type 0`) passe réellement, ce qui prouve
que la chaîne lecture → lexage → analyse → noyau → bilan fonctionne.

## Dépendances utilisées

| Fonction | Module |
|---|---|
| `verifier`, `convertible`, `noy_ctx_vide`, `noy_type` | `thorn/src/noyau.rose` (PROV1) |
| `lexer` | `thorn/src/lexeur.rose` (PROV2) |
| `analyser` | `thorn/src/analyse.rose` (PROV2) |
| `reflexivite`, `symetrie`, `introduction`, `application`, `induction`, `simplifier` | `thorn/src/tactiques.rose` (PROV3) |

Les sorties ci-dessus sont des constats datés : à re-mesurer après tout
changement de module.
