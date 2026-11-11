# Référence de syntaxe Rose (copie locale de secours)

Source de vérité : section « 📖 Syntaxe Rose — référence complète » du
`README.md` à la racine du dépôt. Ce fichier n'est qu'un raccourci ; en cas
de divergence, c'est le README qui gagne.

## Types

`entier` ℤ (illimité) · `rationnel` ℚ (`1/3`) · `flottant` ℝ (`3.14`) ·
`booleen` (`vrai`/`faux`) · `chaine` (UTF-8 en caractères) · `complexe`
(`3 + 4i`) · `irrationnel` (`racine(2)`) · `liste` (`[1,2,3]`, homogène) ·
`table` (`table()`) · `tableau` (`tableau(10)`) · `tampon` (`tampon(64)`).

Le type courant est retenu à la première affectation ; les promotions
numériques (entier → flottant) passent, mais écrire une chaîne dans une
variable numérique est refusé (`$n = 5` puis `$n = "mot"` = erreur).

## Instructions

```rose
$x = 42                    # affectation (aussi « x = 42 » sans $)
>> expr, expr2             # affiche formule LaTeX + résultat
<< x                       # saisie au clavier
si (c) { … } sinon { … }
? (c) [ ins ; ins ]        # si raccourci (séparateur « ; »)
tant_que (c) { ins ; ins }
@ (c) [ ins ; ins ]        # tant_que raccourci
pour i de A vers B { … }   # B EXCLUSIVE
fn nom(p, q: entier) -> entier { … retour v … }   # types facultatifs, « → » aussi
@dll("m") @sym("pow") fn pow_c(x: flottant, y: flottant) -> flottant
# commentaire
```

## Opérateurs

`+ - * / % ^` · Unicode `⋅ · × ÷ ≤ ≥ ≠ ≈ →` · logique `et ou non` ·
postfixés `n!` `2³` `√9`. `/` exact (7/2 → 3.5), `÷` entier (7÷2 → 3).

## Standard (signatures réelles, vérifiées dans rose/src/front.rs STDLIB)

- Nombres : `abs(n)` `min(a,b)` `max(a,b)` `partie_entiere(x)`
  `factorielle(n)`/`fact` `fib(n)`/`fibonacci` `pgcd(a,b)`/`gcd`
  `ppcm(a,b)`/`lcm` `puissance(x,n)`/`pow` `nb_chiffres(n)`/`digits`
  `est_premier(n)`/`is_prime` `compter_premiers(n)`
- Exact : `pi(n digits)` `e(n)` `phi(n)` `racine(n)`→irrationnel
  `fraction(a,b)`→rationnel
- Types : `type_de(x)` / `type(x)` → chaîne
- Complexes : `re` `im` `conj` `arg` `modulus`
- Chaînes : `longueur(s)`/`len` `ordre(s)`→codepoint `caractere(n)`→chaîne
  `sous_chaine(s,d,f)`/`sub` `concatener(liste)`→chaîne
- Tables : `table()` `map()` `inserer(t,k,v)` `cle_existe(t,k)`
  `supprimer(t,k)`→booléen `cles(t)` `valeurs(t)` ; indexage `t["cle"]`
- Listes : `joindre(l, x)` ajoute x en fin (mutateur, retour vide)
- Hasard : `graine(n)`/`seed` `hasard(n)` `prng(n)` `hasard_systeme(n)`
  `hasard_vrai(n)` `rng_materiel()`
- Fichiers/args : `lire_texte(chemin)` `ecrire_texte(chemin,texte)`
  `ecrire_binaire(chemin,liste_octets)` `nb_arguments()` `argument(i)`
- Temps : `temps()` `millisecondes()`/`ms` `time()` `duree(n)`
- Divers : `afficher(x)` `afficher_erreur(x)` `quitter(n)` `tampon(n)`
  `octets(chaine)`→tampon `eval_expr(chaine)` `breakpoint` `watch(var)`

## Constantes affichées avec formule

```
>> pi    → π = 4\cdot\arctan(1/5) - \arctan(1/239)   π ≈ 3.14159265358979323846
>> e     → e = \sum_{k=0}^{\infty} \frac{1}{k!}   e ≈ 2.71828182845904523536
>> phi   → φ = \frac{1 + \sqrt{5}}{2}   φ ≈ 1.61803398874989484820
>> racine(2) → \sqrt{2}   √2
```

## Sortie LaTeX (`>>`) et `rose latex`

- `>> 3 + 4i` → `3 + 4\mathrm{i}   5`
- `>> modulus($z)` → `\left|z\right|   5`
- boucle : `\sum_{j = 0}^{9} \langle s = s + j\rangle`
- fonction typée : `\operatorname{fib}(n \in \mathbb{Z}) \to \mathbb{Z}`
- fonction allégée : `\operatorname{carre}(x)` (types omis = rien affiché)

## Console interactive (REPL)

```bash
rose console                  # console vide
rose console prog.rose        # charge prog.rose, puis console
rose prog.rose --console      # exécute prog.rose, puis console
```

Commandes : `:help`, `:show`, `:reset`, `:load <fichier>`, `:type <expr>`, `:quit`.

## Exemple complet vérifié (`./rose t.rose`)

```rose
# somme des carrés, récursion, complexe, table
$total = 0
pour k de 1 vers 11 { $total = $total + k * k }
>> $total                          # 385

fn fib(n: entier) -> entier {
    si (n < 2) { retour n }
    retour fib(n - 1) + fib(n - 2)
}
>> fib(20)                         # 6765

$z = 3 + 4i
>> modulus($z)                     # 5

$t = table()
$t["a"] = 1
>> cles($t)                        # [a]
```
