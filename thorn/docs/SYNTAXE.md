# Thorn — syntaxe couverte par le lexeur et le parseur (PROV2)

État réel, vérifié le 21/11/2026 avec `./rose/rose verif` et
`./rose/rose thorn/tests_syntaxe.rose` (8 tests, 8 OK : les 5 programmes
demandés plus 3 cas supplémentaires lambda / juxtaposition / erreur).

## Fichiers

- `thorn/src/lexeur.rose` — lexeur autonome.
- `thorn/src/analyse.rose` — parseur ; importe `lexeur.rose`.
- `thorn/tests_syntaxe.rose` — 8 tests (5 programmes demandés + lambda
  autonome, application par juxtaposition, erreur localisée), AST attendu
  affiché pour chacun.

Aucun autre fichier n'est modifié par PROV2.

## API

```
lexer(src: chaine)            -> liste<chaine>   # jetons nus
lexer_detaille(src: chaine)   -> liste<chaine>   # jetons "texte~ligne"
lexer_table(src: chaine)      # interne : {n: nb jetons, j: jetons détaillés}
analyser(jetons: liste<chaine>) -> Terme
analyser_source(src: chaine)   -> Terme   # parse un terme depuis une chaine
analyser_fichier(src: chaine) -> liste de {genre:"declaration", nom, type, preuve}
```

`analyser` retire le suffixe `~N` s'il est présent : il accepte donc une
liste de jetons nus OU détaillés **à condition qu'elle soit typée**
`liste<chaine>` (par exemple un littéral Rose `["f", "x", "y"]`).
Le suffixe `~N`, quand il existe, sert à numéroter les erreurs.
Limitation Rose mesurée : le résultat de `lexer()` est typé `?` et ne peut
PAS être passé directement à `analyser` (qui exige `liste<chaine>`) —
utiliser `analyser_source(src)` pour un terme isolé issu d'une chaîne.
`analyser_fichier` utilise toujours `lexer_table` (jetons détaillés), donc
les erreurs y sont toujours localisées par numéro de ligne.

## Lexeur

- Identifiants : lettres ASCII, `_`, tout code Unicode ≥ 128 (`é`, `è`, `π`),
  puis lettres/chiffres/`_`.
- Nombres : suites de chiffres → jeton nombre.
- Opérateurs et ponctuation : `->`, `=>`, `=`, `:`, `,`, `;`, `(`, `)`,
  `+`, `-`, `*`, `/`, `[`, `]`, `{`, `}`.
- Commentaires : `#` jusqu'à la fin de ligne.
- Espace, tabulation, `\r` ignorés ; `\n` incrémente le numéro de ligne.
-Caractère inconnu → jeton `?` (jamais d'exception) ; le parseur le signale
  avec sa ligne.

## Grammaire des termes

Précédence, de la plus faible à la plus forte :

| Niveau | Syntaxe | AST (SPEC.md) |
|---|---|---|
| 1 (droite) | `A -> B`, `A implique B` | `pi("_", A, B)` |
| 2 (gauche) | `A ou B` | `ind("ou", [A, B])` |
| 3 (gauche) | `A et B` | `ind("et", [A, B])` |
| 4 (gauche) | `A = B` | `ind("=", [A, B])` |
| 5 (gauche) | `A + B`, `A - B` | `app(app(const("+", type(0)), A), B)` |
| 6 (préfixe) | `non A` | `ind("non", [A])` |
| 7 (gauche) | `f a b` (juxtaposition) | `app(app(f, a), b)` |
| 8 | atome | voir ci-dessous |

Atomes :

- identifiant → `var(nom)` ;
- `type` ou `type N` → `type(niveau)` (N entier, défaut 0) ;
- `( ... )` → terme entre parenthèses ;
- `fn x y : T => corps` → `lam(x, T, lam(y, T, corps))` ;
- `pour_tout x y : T, corps` → `pi(x, T, pi(y, T, corps))` ;
- `existe x : T, P` → `ind("existe", [var(x), T, P])` ;
- `vrai`, `faux` → `ind("vrai", [])`, `ind("faux", [])` ;
- nombre → `const("N", type(0))`.

Mots réservés : `théorème`, `preuve`, `pour_tout`, `existe`, `implique`,
`et`, `ou`, `non`, `vrai`, `faux`, `induction`, `cas`, `soit`, `montrer`,
`par`, `type`, `fn`. `sur` est reconnu comme mot-clé contextuel après
`par induction`.

## Grammaire des déclarations

```
fichier      := declaration*
declaration  := "théorème" nom ":" terme "preuve" tactique*
tactique     := "soit" nom+ ":" terme
              | "montrer" terme
              | "par" nom ["sur" nom+]
              | "cas" motif nom* ":" tactique*
              | ";"
```

Une déclaration produit une table
`{genre:"declaration", nom, type, preuve}` où `type` est un `Terme` et
`preuve` une liste de tables de tactiques :

- `{genre:"soit", noms:[...], type:Terme}`
- `{genre:"montrer", but:Terme}`
- `{genre:"par", nom:"..."}` ou `{genre:"par", nom:"induction", sur:[...]}`
- `{genre:"cas", motif:"...", params:[...], preuve:[...tactiques]}`

Les constructeurs de termes `t_var`, `t_type`, `t_pi`, `t_lam`, `t_app`,
`t_const`, `t_ind`, `t_constructeur`, `t_elim` sont publics dans
`analyse.rose` (conformément à la table `Terme` de `SPEC.md`).

## Erreurs

`analyser` et `analyser_fichier` n'échouent jamais par une exception Rose :
ils écrivent `afficher_erreur("Thorn — ligne N : <message>")` et rendent un
terme/de une liste partiels. Exemples réels observés :

- `ligne 3 : tactique inattendue '?'`
- `ligne 1 : attendu 'preuve', trouve ''`
- `ligne 1 : jetons en trop : 'et'` (avant correction de la priorité de `non`)

La numérotation vient du suffixe `~N` émis par `lexer_detaille`.

## Limites réelles

- Pas de notation infixe complexe (ni priorités personnalisées, ni
  `∧`/`∨`/`→` Unicode ; seulement les mots français et `->`, `=`, `+`, `-`).
- `=` est représenté par `ind("=", [g, d])` : aucun type d'égalité n'est
  inféré, ce point reste à accorder avec le noyau (PROV1).
- `existe` garde le nom du liant comme premier paramètre `var` ; ce n'est pas
  un `pi`, donc le noyau devra le reconnaître explicitement.
- Les preuves sont analysées comme des listes de tactiques ; aucune
  vérification n'est faite ici (c'est le rôle du noyau et des tactiques).
- Les messages d'erreur sont en français simplifié sans accents dans les
  chaînes émises par le moteur (` trouve`, ` caractere`) pour éviter tout
  souci de locale ; le code source, lui, est accentué.
- `analyser` sur des jetons nus ne peut pas donner de numéro de ligne.

## Vérification reproductible

```
cd /home/clausake/Stefan/ROSE
./rose/rose verif thorn/src/lexeur.rose     # vérifié
./rose/rose verif thorn/src/analyse.rose    # vérifié
./rose/rose thorn/tests_syntaxe.rose        # OK = 8   ECHEC = 0
```
