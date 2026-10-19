# Rose v2 — Spécification du langage

Rose est un langage de calcul mathématique exact (entiers et rationnels de précision
arbitraire, flottants, chaînes, listes) compilé vers plusieurs cibles natives :
**C (GMP)** pour la performance, **Python** et **Julia** pour l'écosystème,
**Rust** et **Zig** pour l'intégrité mémoire (types machine).

## Moteur natif (mode principal)

Le produit central est **`../moteur/rose` : un binaire autonome** — lexeur,
parseur, vérificateur, repli de constantes, interpréteur, bibliothèque
mathématique exacte (GMP liée statiquement) et FFI (`dlopen`) embarqués.
Un programme `.rose` s'exécute sans aucune dépendance hors la libc :

```
rose prog.rose            → interprétation directe
rose comp prog.rose       → exécutable autonome (moteur + programme collé)
```

Les backends ci-dessous sont l'**export polyglotte** (outil `rose2/bin/rose`,
`--cible X`) : mêmes types, mêmes valeurs, même écriture — vérifié identique
au caractère près contre le moteur (suite `moteur/tests/courir.py`).

## Pipeline

```
source .rose → lexer → parser (AST) → repli de constantes → vérificateur de types
             → soit : interpréteur natif (GMP)            [moteur, mode par défaut]
             → soit : exécution compilée embarquée          [rose comp]
             → soit : IR structurée → backend              [export --cible]
                ├─ C      : GMP (mpz/mpq), gcc/clang/zig cc, bignum complet
                ├─ Python : int / Fraction / float
                ├─ Julia  : BigInt / Rational / Float64
                ├─ Rust   : i64 / f64 (types machine)
                └─ Zig    : i64 / f64 (types machine)
```

## Grammaire EBNF (noyau)

```ebnf
programme   ::= { fonction } { instruction }
fonction    ::= [ annotation_dll ] "fn" ident "(" [ params ] ")" [ "->" type ]
                ( bloc | ABSENT )          ; sans corps = fonction externe
annotation_dll ::= '@dll' "(" chaine [ "," chaine ] ")"
                | '@sym' "(" chaine ")"
type        ::= "entier" | "rationnel" | "flottant" | "booleen" | "chaine"
              | "liste" "<" type ">"
instruction ::= affectation | si | tantque | pour | retour | affichage | lecture
              | condition_sucre | boucle_sucre | expr_affect | bloc | appel
affectation ::= "$" ident "=" expression      ; le $ est optionnel
si          ::= "si" "(" expression ")" bloc [ "sinon" (bloc | si) ]
tantque     ::= "tant_que" "(" expression ")" bloc
pour        ::= "pour" ident "de" expression "vers" expression bloc
retour      ::= "retour" [ expression ]
affichage   ::= ">>" expression
lecture     ::= "<<" "$"? ident
cond_sucre  ::= "?" "(" expression ")" "[" instruction { ";" instruction } "]"
boucle_sucre::= "@" "(" expression ")" "[" instruction { ";" instruction } "]"
bloc        ::= "{" { instruction [ ";" ] } "}"
```

## Expressions (précédence croissante)

```ebnf
expression  ::= ou_expr
ou_expr     ::= et_expr  { "ou"  et_expr }
et_expr     ::= non_expr { "et"  non_expr }
non_expr    ::= "non"? comparaison
comparaison ::= somme { ("<"|"≤"|"<="|">"|">="|"≥"|"=="|"/="|"!="|≠|"≈"|"?="|"=~") somme }
somme       ::= produit { ("+"|"-") produit }
produit     ::= puissance { ("*"|"×"|"⋅"|"div"|"÷"|"/"|":") puissance }
puissance   ::= factorielle [ "^" puissance ]      ; droite-associatif
factorielle ::= unary [ "!" ]+
unary       ::= ("-"|"+") unary | atome
atome       ::= nombre | chaine | "vrai" | "faux" | liste | ident
              | appel | "(" expression ")" | index
liste       ::= "[" [ expression { "," expression } ] "]"
index       ::= atome "[" expression "]"
```

## Types et promotion

| type        | C/GMP      | Python     | Julia      | Rust/Zig |
|-------------|-----------|------------|------------|----------|
| entier      | mpz_t     | int        | BigInt     | i64      |
| rationnel   | mpq_t     | Fraction   | Rational{BigInt} | — |
| flottant    | double    | float      | Float64    | f64      |
| booleen     | int       | bool       | Bool       | bool     |
| chaine      | char*     | str        | String     | —        |
| liste\<T\>  | RoseListe | list       | Vector     | —        |

Promotion arithmétique : `booleen → entier → rationnel → flottant`.
En C/Python/Julia, `entier / entier` produit un **rationnel exact** (3/2 → 3/2,
jamais 1.5). Les cibles machine **Rust/Zig n'ont pas de rationnel** : tout
programme qui en utilise (`/` entre entiers, exposant négatif, chaînes, listes,
FFI dynamique) est **refusé à la compilation** avec un message nommant la
fonctionnalité manquante — jamais de troncature silencieuse.

## Sémantique des opérateurs

- `+ - *` : arithmétique ; `chaine + chaine` = concaténation.
- `/` : division **exacte** (entier/entier → rationnel) ; flottant → flottant.
- `div` / `÷` : division **euclidienne** (arrondi vers le bas) ; entier → entier,
  flottant → flottant. C'est l'opérateur des algorithmes entiers (Euclide, collatz…).
- `%` : modulo euclidien (entier) — cohérent avec `div` : `-7 % 3 = 2`.
- `^` : puissance, **droite-associative** (`2^3^2 = 512`). Exposant entier → même
  type (exposant négatif sur entier → rationnel) ; exposant flottant → flottant.
- `!` postfixe : factorielle (entier ≥ 0, produit arbre binaire).
- Comparaisons → `booleen`. `≈` / `?=` / `=~` : égalité avec tolérance relative
  1e-9 sur flottants, exacte sinon.
- Virgule décimale : **non**. `,` est strictement le séparateur d'arguments et de
  listes (comme dans Rose v1) ; le point sépare l'entier de la décimale.
- `et`, `ou`, `non` : booléens, évaluation courte (court-circuit).

## Entrées/sorties

- `>> expr` : affiche la valeur. Les rationnels s'affichent en développement
  décimal exact avec **période détectée** (`1/6 → 0.1[6]`), ou en `p/q` au-delà
  de 500 chiffres ; les flottants s'affichent en la représentation décimale la
  plus courte qui les reconstruit exactement (aller-retour fidèle).
  La conversion `chaine(x)` rend très exactement le texte que `>> x` afficherait.
- `<< x` : lit une ligne depuis stdin dans `x`. Si `x` a déjà un type
  (affectation antérieure), on respecte ce type ; sinon `x` devient `entier`.
  La lecture est **tolérante** : une ligne `p/q` est lue comme fraction exacte
  (`2/3`), une ligne avec `.`/`e` comme décimale. Sur une variable `entier`, la
  valeur est **tronquée vers zéro** (`2.25` → `2`, `7/2` → `3`) ; sur `rationnel`
  elle est prise exactement ; sur `flottant` convertie en flottant. Lecture vers
  `chaine`/`liste`/`booleen` refusée à la compilation.
- Fonctions standard : `afficher`, `saisir_entier`, `saisir_flottant`,
  `factorielle`, `fibonacci`, `pgcd`, `ppcm`, `puissance`, `est_premier`,
  `compter_premiers`, `nb_chiffres`, `longueur`, `joindre`, `abs`, `min`, `max`,
  `partie_entiere` (= ⌊x⌋, plancher), `entier`, `flottant`, `rationnel`, `chaine`, `fraction`,
  `nombre_pi` (arrondi correct à la dernière décimale).

## v3 — Briques de l'auto-hébergement

Rose v3 ajoute ce qu'il faut pour écrire le compilateur Rose **en Rose**.
Ces mécanismes sont validés par les cinq moteurs au caractère près
(inclus les messages d'erreur) mais ne sont **pas exportables** par la
chaîne `rose2` (les cibles refusent explicitement).

### Notation symbolique

Rose s'écrit avec des symboles mathématiques. Ces écritures sont des
équivalents exacts des notations ASCII (traitées au lexeur, identiques
sur les cinq moteurs) :

| Symbole | Équivalent | Sens |
|---|---|---|
| `×` `⋅` `·` | `*` | multiplication |
| `÷` | `div` | division **entière** (`7 ÷ 2` → `3`) |
| `≤` `≥` `≠` | `<=` `>=` `!=` | comparaisons |
| `≈` | — | comparaison approchée (tolérance relative 1e-9) |
| `√x` | `x ^ 0.5` | racine carrée, exacte sur les carrés parfaits (`√9` → `3`, `√(1/4)` → `0.5`) |
| `x²` `x³` | `x^2` `x^3` | puissances |
| `π` | `3.141592653589793` | constante (flottant double) |
| `s ← v` | `s = v` | affectation |
| `fn f(x : entier) → entier` | `->` | flèche de signature |

`√` est un préfixe unaire, plus lié que `^` : `√2 ^ 2` se lit `(√2) ^ 2`.
Les lettres grecques et accentuées restent des caractères d'identifiant,
sauf `π` réservé comme constante.

### Arithmétique mixte rationnel ⊛ flottant

Toute opération entre un rationnel et un flottant se calcule en
**double précision** après conversion du rationnel (numérateur et
dénominateurs en double). La règle est **identique dans le repli de
constantes et à l'exécution** : `1/4 + 0.5` → `0.75`,
`1/3 - 0.25` → `0.08333333333333331`, `(1/4) ^ 0.5` → `0.5`.
`%` exige deux entiers (`1 % 0.5` → « Le modulo exige deux entiers »).
Les exposants fractionnaires d'un rationnel sont tronqués
(`(1/4) ^ (3/2)` → `(1/4) ^ 1` → `0.25`) : écrire des flottants pour un
vrai exposant fractionnaire (`0.25 ^ 1.5`).

Note — dernier ulp : `pow` (exposant fractionnaire) dépend de la
bibliothèque mathématique de la plateforme ; un écart de 1 ulp est
possible entre libms différents. La racine carrée via `√` (exposant
0.5) est correctement arrondie partout.

### Chaînes : caractères et codes

Positions et longueurs se comptent en **caractères Unicode** (points de
code), pas en octets : `"été"` a pour longueur 3 et `"été"[1]` vaut `t`.

- `s[i]` — le iᵉ caractère (index **à partir de 0**), sous forme de chaîne
  d'un caractère ; hors plage → erreur d'exécution.
- `longueur(s)` — nombre de caractères (fonctionne aussi sur les listes).
- `ordre(s)` — code du caractère unique : `ordre("a") → 97`,
  `ordre("é") → 233`, `ordre("\n") → 10`.
- `caractere(n)` — chaîne du code `n` (0 à 1114111, sauf 55296 à 57343).
- `sous_chaine(s, debut, fin)` — caractères `debut` (inclus) à `fin`
  (exclus), `fin` bornée à la fin ; bornes négatives ou `fin < debut` →
  erreur.
- `concatener(liste_de_chaines)` — concaténation en temps linéaire : avec
  `joindre`, c'est le constructeur qui évite le coût quadratique de `$s =
  $s + "x"` répété.

### Fichiers du programme

- `lire_texte(chemin)` — contenu lu **brut** (aucune normalisation de fins
  de ligne) ; fichier absent → erreur.
- `ecrire_texte(chemin, contenu)` — écriture avec écrasement.

### Ligne de commande et sortie

- `nb_arguments()` / `argument(i)` — arguments passés après le nom du
  programme (ou du bundle) ; index à partir de 0.
- `afficher_erreur(v)` — affiche sur le flux d'erreur standard.
- `quitter(code)` — sortie immédiate **sans message** avec le code `code`
  (sorties déjà émises préservées).

### Appel nu en instruction

Une ligne contenant seulement un appel est une instruction valable (comme
`joindre($l, 4)` depuis toujours) : `ecrire_texte(...)`,
`afficher_erreur(...)`, `quitter(0)` s'écrivent nus, sans affectation.

### Tranches de chaînes (v3.3)

`s[a:b]` est un raccourci pour `sous_chaine(s, a, b)` — bornes en
caractères, `b` exclus. Bornes omises : `s[:b]` (début à 0), `s[a:]`
(fin à la longueur), `s[:]` (copie).


### Tables (v3.1)

Tables associatives ordonnées (insertion order), clés scalaires (texte ou
nombre), valeurs hétérogènes possibles. Non exportables par `rose2`.

- `table()` — table vide.
- `$t[cle]` — lecture ; clé absente → erreur d'exécution.
- `inserer($t, cle, valeur)` — insertion ou écrasement (comme `joindre`).
- `cle_existe($t, cle)` / `supprimer($t, cle)` → booléen.
- `cles($t)` / `valeurs($t)` → listes (ordre d'insertion).
- `longueur($t)` → nombre d'entrées.
- Affichage : `["a" => 1, "b" => 2]` ; vide : `[]`.
- Égalité `==`/`!=` : même taille, mêmes clés (ordre indifférent), mêmes
  valeurs.
- Tables imbriquées possibles : `$t["a"]["b"]`.
- Les clés booléennes sont refusées (ambiguïté avec 0/1).


## FFI — « intégrer des DLL »

```rose
@dll("m")                       # bibliothèque à lier (-lm)
@sym("pow")                     # nom du symbole C
fn pow_libm(x: flottant, y: flottant) -> flottant    # pas de corps → extern

@dll("ma_bibliotheque", "libma_bibliotheque.so")
fn calcul_sacré(x: entier) -> entier
```

Chargement **dynamique** (dlopen/ctypes/Libdl) — cibles C, Python et Julia :

```rose
h = dynamique::charger("./librose_ext.so")
y = dynamique::appeler1(h, "double_carre", 5.0)     # double f(double)
z = dynamique::appeler2(h, "hypotenuse", 3.0, 4.0)  # double f(double, double)
w = dynamique::appeler0(h, "reponse_univers")        # double f(void)
dynamique::fermer(h)
```

Les cibles machine (Rust/Zig) n'offrent que la liaison **statique** `@dll` ;
`dynamique::` y est refusé à la compilation.

## Exemples

```rose
$fact = 100!                      # factorielle exacte
>> 1 / 2 + 1 / 3                   # → 5/6 (rationnel exact)
$pi = nombre_pi(50)                # 50 décimales de π
>> $pi

fn fibrapide(n: entier) -> entier {
    si (n <= 1) { retour n }
    $a = 0 ; $b = 1
    pour i de 0 vers n { $c = $a + $b ; $a = $b ; $b = $c }
    retour $a
}
>> fibrapide(1000)

pour i de 0 vers 5 { ? ($i % 2 == 0) [>> $i] }      # sucre conditionnel
```

## v3.7 — Aléatoire et tampons d'octets (moteur C++)

Briques de performances, disponibles dans le moteur C++ uniquement (comme la
FFI ci-dessus : les cibles `rose2` les refusent explicitement).

- `graine(n)` — fixe la graine du générateur (splitmix64) : le tirage devient
  reproductible, octet pour octet, sur toutes les architectures.
  Sans appel, la graine initiale varie à chaque exécution.
- `hasard(n)` — entier uniforme dans [0, n). Puissances de deux : masque
  exact ; sinon réduction de Lemire (sans biais).
- `tampon(n)` — buffer contigu de `n` octets, passé par référence (comme les
  listes), coût d'affectation indépendant de la taille.
- `octets(chaine)` — tampon des octets UTF-8 bruts d'une chaîne.
- `tampon_case(t, i)` / `tampon_pose(t, i, v)` — lecture/écriture d'un octet
  (0–255, écriture modulo 256), `longueur(t)` et `chaine(t)` fonctionnent.
- `remplir_hasard(t, alphabet)` — remplit le tampon `t` de caractères pris au
  hasard dans `alphabet` (tampon), en boucle native : c'est l'équivalent des
  appels vectorisés de numpy — la boucle vit dans le moteur.
- Un tampon ne s'affiche pas directement (`>> $t` refuse) : convertir par
  `chaine(t)`. Deux tampons se comparent avec `==`.

Les noms de variables de la forme `t<chiffres>` (`t0`, `t1`, …) sont **réservés
par le moteur** : ce sont les emplacements des temporaires internes, et une
variable portant le même nom les écrasait (le vérificateur refuse désormais
ces noms avec un message explicite).

## CLI

```
rose run programme.rose [--cible c|python|rust|julia|zig] [--cc gcc|clang|zig]
rose build programme.rose -o sortie [--cible c] [--opt niveau]
rose emettre programme.rose --cible c        # montre le code généré
rose info                                    # toolchains et GMP détectés
```

Variables d'environnement : `ROSE_GMP_DIR` (préfixe GMP), `ROSE_CC` (cc par défaut).
