# Syllog — assistant de démonstration expérimental

Projet EXPÉRIMENTAL, distinct de Rose. But: faire de l'analyse formelle poussée
(types dépendants, propositions-comme-types, noyau de vérification, tactiques)
dans la philosophie Rose: syntaxe française, code en Rose, zéro dépendance externe.

## Interface commune (contract entre agents)

Les termes sont représentés par une table Rose `Terme` avec champ `genre`:
- `var` (champ `nom`)
- `type` (champ `niveau`: entier)
- `pi` (`nom`, `domaine`, `codomaine`)
- `lam` (`nom`, `domaine`, `corps`)
- `app` (`fonction`, `argument`)
- `const` (`nom`, `type`)
- `ind` (`nom`, `params`) — type inductif
- `constructeur` (`nom`, `type`)
- `elim` (`nom`, `motif`, `branches`)

Contexte = liste d'associations `nom -> type` (table Rose).

### Noyau (`syllog/src/noyau.rose`)
```
fn verifier(t: Terme, ctx: Contexte) -> Resultat    # Resultat {ok: bool, type: Terme, erreur: chaine}
fn beta(t: Terme) -> Terme                          # reduction beta complete (avec garde de pas)
fn convertible(a: Terme, b: Terme) -> bool          # equivalence definitional (beta + alpha)
fn bien_forme(t: Terme, ctx: Contexte) -> bool
```

### Analyse syntaxique (`syllog/src/lexeur.rose`, `syllog/src/analyse.rose`)
```
fn lexer(src: chaine) -> liste<chaine>              # jetons (mots reserves francais)
fn analyser(jetons: liste<chaine>) -> Terme
```
Mots réservés: `théorème`, `preuve`, `pour_tout`, `existe`, `implique`, `et`, `ou`,
`non`, `vrai`, `faux`, `induction`, `cas`, `soit`, `montrer`, `par`, `type`, `fn`.

### Tactiques (`syllog/src/tactiques.rose`)
```
fn reflexivite(but: Terme) -> Resultat
fn symetrie(but: Terme) -> Resultat
fn introduction(but: Terme, ctx: Contexte) -> Resultat
fn application(but: Terme, ctx: Contexte) -> Resultat
fn induction(but: Terme, ctx: Contexte) -> Resultat
fn simplifier(t: Terme) -> Terme
```

### Pilote (`syllog/src/principal.rose`)
```
syllog <fichier.slg>    -> verifie chaque théorème, affiche OK / ÉCHEC + raison
syllog --demo           -> joue les preuves de syllog/preuves/
```

## Règles
- Aucune dépendance externe; tout en Rose.
- Ne PAS modifier Rose (`rose/src/**`) — Syllog est un consommateur de Rose.
- Vérifier avec `./rose/rose verif <fichier.rose>` avant toute livraison.
- Honnêteté: documenter ce qui est implémenté, ce qui est simulé, ce qui est bloqué.
