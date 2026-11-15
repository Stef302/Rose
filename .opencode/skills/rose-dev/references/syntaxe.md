# 📖 Syntaxe Rose — référence complète

Rose est un langage de programmation en français. Cette référence contient
**toute la syntaxe** pour qu'un développeur ou une IA écrive du Rose du
premier coup. Un exemple vivant de tout cela : `rose exemples/tour.rose`.

### Types

| Type Rose | Ensemble | Description |
|---|---|---|
| `entier` | `ℤ` | entier exact (BigInt natif, illimité) |
| `rationnel` | `ℚ` | fraction exacte `a/b` — s'écrit `1/3` |
| `flottant` | `ℝ` | flottant IEEE 754 double — s'écrit `3.14` |
| `booleen` | `𝔹` | `vrai` / `faux` |
| `chaine` | — | texte **UTF-8 en caractères** (pas en octets) |
| `complexe` | `ℂ` | nombre complexe `3 + 4i` |
| `irrationnel` | — | racine carrée exacte `racine(2)` = √2 |
| `liste` | — | liste homogène `[1, 2, 3]` (un seul type) |
| `table` | — | table associative `t = table()` |
| `tableau` | — | tableau fixe `tableau(10)` |
| `tampon` | — | tampon d'octets `tampon(64)` |
| `vide` | — | absence de valeur |

Le type courant est retenu à la première affectation : les promotions
numériques passent (`$a = 2` puis `$a = 2.5` → flottant), mais écrire une
chaîne dans une variable numérique est refusé (`$n = 5` puis
`$n = "mot"` = erreur).

### Variables et littéraux

```rose
$n = 42                  # entier (illimité) : 100 ! a 158 chiffres
$f = 3.14                # flottant (point décimal)
$r = 1/3                 # rationnel exact (barre oblique)
$s = "bonjour"           # chaîne UTF-8
$b = vrai                # booléen (vrai / faux)
$z = 3 + 4i              # complexe — « i » minuscule = imaginaire
$l = [1, 2, 3]           # liste (un seul type par liste)
$t = table()             # table associative
	tab["cle"] = 7
$a = tableau(10)         # tableau de 10 cases
```

### Opérateurs

```rose
+  -  *  /  %  ^           # / = division exacte (7/2 → 3.5)
⋅  ·  ×  ÷  ≤  ≥  ≠  ≈     # variantes Unicode (* ≤ ≥ != ~)
et  ou  non                # logique (et/ou/non)
n!   2³  √9                # postfixés : factorielle, puissance, racine
n#                         # primorielle : produit des premiers ≤ n   (10# → 210)
n?                         # terminal : somme des factorielles 1!+…+n!   (5? → 154)
n$                         # super-factorielle : 1!·2!·…·n!   (3$ → 12)
```

| Opérateur | Effet | Exemple |
|---|---|---|
| `/` | division **exacte** : entier/entier → flottant affiché, `1/3` → rationnel | `7 / 2` → `3.5` |
| `÷` | division entière | `7 ÷ 2` → `3` |
| `%` | modulo (entiers) | `7 % 3` → `1` |
| `^` | puissance | `2 ^ 10` → `1024` |
| `≈` | comparaison approchée | `0.1 + 0.2 ≈ 0.3` → `vrai` |
| `==` `!=` `<` `<=` `>` `>=` | comparaison (nombres et chaînes) | |

### Affichage et saisie

```rose
>> expr1, expr2        # affiche : [formule LaTeX]  résultat
<< entier_var          # saisit un entier au clavier
<< float_var           # saisit un flottant (la variable doit exister, typée)
```

### Structures de contrôle

```rose
si (cond) { … }                          # si
si (cond) { … } sinon { … }              # si / sinon
? (cond) [ instr ; instr ]               # raccourci « si » en une ligne
tant_que (cond) { instr ; instr ; … }    # boucle tant_que (« ; » entre instr.)
@ (cond) [ instr ; instr ]               # raccourci « tant_que » en une ligne
pour i de 0 vers 10 { … }                # boucle pour — borne FINALE EXCLUSIVE
pour k de 1 vers 11 { … }                # itère k = 1..10
```

⚠ Dans les corps `[ … ]` et `{ … }` raccourcis, les instructions sont
séparées par **`;`** (pas `,`).

### Fonctions — types FACULTATIFS

```rose
# Forme allégée : ni type de paramètre ni type de retour
fn carre(x) { retour x * x }
>> carre(7)                      # 49

# Forme typée (réservée au JIT/AOT : types entier, booleen, flottant)
fn fib(n: entier) -> entier {
    si (n < 2) { retour n }
    retour fib(n - 1) + fib(n - 2)
}
>> fib(20)                       # 6765

# « -> » ou « → », les deux marchent
fn double(x: flottant) → flottant { retour 2.0 * x }

# Une fonction peut retourner n'importe quel type (chaîne, liste, table…)
fn saluer(nom) { retour "bonjour " + nom }
>> saluer("rose")
```

Le type du **paramètre et du retour est facultatif**. Quand il est déclaré,
le LaTeX l'affiche comme un ensemble : `fib(n ∈ ℤ) → ℤ` ;
quand il est omis, la signature est allégée : `carre(x)`.

### Listes et tables

```rose
$l = [1, 2, 3]
>> $l[1]                    # 2 (index depuis 0)
>> longueur($l)             # 3
joindre($l, 4)              # ajoute en fin → [1, 2, 3, 4]

$t = table()
$t["cle"] = 7               # affectation par clé
>> $t["cle"]
>> cle_existe($t, "cle")    # vrai
>> cles($t)                 # ["cle"]
>> valeurs($t)
supprimer($t, "cle")
inserer($t, "a", 1)         # équivalent à $t["a"] = 1
```

### Chaînes

```rose
$t = "été radieux"
>> longueur($t)               # 11 (caractères UTF-8, pas octets)
>> $t[0]                      # "é"
>> ordre("A")                 # 65 (point de code)
>> caractere(960)             # "π"
>> sous_chaine($t, 2, 4)      # "é " (début inclus, fin exclue)
>> concatener(["ab", "cd"])   # "abcd" (prend une LISTE)
```

### Constantes irrationnelles exactes

Aucune valeur n'est figée — elles sont calculées à la demande, avec leur
formule exacte en LaTeX et une approximation `≈` :

```rose
>> pi        # π = 4\cdot\arctan(1/5) - \arctan(1/239)   π ≈ 3.14159…
>> e         # e = Σ 1/k!   e ≈ 2.71828…
>> phi       # φ = (1+√5)/2   φ ≈ 1.61803…
>> racine(2) # \sqrt{2}   √2
>> pi(1000)  # 1000 décimales (Chudnovsky, scission binaire)
```

### Fonctions standard

| Domaine | Fonctions |
|---|---|
| Nombres | `abs` `min` `max` `partie_entiere` `factorielle`(`fact`) `rapide`(`factorial_rapide`) `approx`(`factorial_approx`) `primorielle`(`n#`) `terminal`(`n?`) `super_factorielle`(`n$`) `fib`(`fibonacci`) `pgcd`(`gcd`) `ppcm`(`lcm`) `puissance`(`pow`) `nb_chiffres`(`digits`) `est_premier`(`is_prime`) `compter_premiers` |
| Gros nombres | `view(x)` — visionneuse paginée dans le terminal (façon nano) pour les nombres à des millions de chiffres |
| Exact | `pi(n)` `e(n)` `phi(n)` `racine(n)` `fraction(a,b)` |
| Types | `type(x)` → chaîne ("entier", "flottant", …) |
| Complexes | `re(z)` `im(z)` `conj(z)` `arg(z)` `modulus(z)` |
| Chaînes | `longueur`(`len`) `ordre` `caractere` `sous_chaine`(`sub`) `concatener(liste)` |
| Tables | `table` `map` `inserer` `cle_existe` `supprimer` `cles` `valeurs` |
| Listes | `joindre(l, x)` (ajoute x à la fin) |
| Hasard | `graine(n)`(`seed`) `hasard(n)` `prng(n)` `hasard_systeme(n)` `rng_materiel` |
| Entrée/sortie | `lire_texte(chemin)` `ecrire_texte(chemin, texte)` `ecrire_binaire` `argument(i)` `nb_arguments` |
| Temps | `temps` `millisecondes`(`ms`) `time` `duree(n)` |
| Divers | `afficher` `afficher_erreur` `quitter(n)` `tampon(n)` `octets(chaine)` `eval_expr(chaine)` |

### FFI (bibliothèques C partagées)

```rose
@dll("m") @sym("pow")
fn pow_c(x: flottant, y: flottant) -> flottant   # pas de corps = externe

>> pow_c(2.0, 10.0)          # 1024
```

### Commentaires

```rose
# tout ce qui suit « # » sur la ligne est ignoré
```

### Affichage LaTeX automatique

Chaque `>> expr` affiche la **formule exacte en LaTeX formée du programme**
puis le résultat :

```
>>> $s = 0
>>> pour j de 0 vers 10 { $s = $s + j }
>>> $s
\sum_{j = 0}^{9} \langle s = s + j\rangle
s   45
>>> 3 + 4i
3 + 4\mathrm{i}   5
>>> carre(7)
\operatorname{carre}(7)   49
```

`rose latex prog.rose` rend le **programme entier** en LaTeX, fonctions
comprises, signatures `n ∈ ℤ` incluses.

### Console interactive (REPL)

Deux points d'entrée, **une seule implémentation** (`rose/src/repl.rs`) :

```bash
rose console                  # depuis le moteur
rose console prog.rose        # charge prog.rose, puis console
rose prog.rose --console      # exécute prog.rose, puis console (variables disponibles)

rose-shell                    # exécutable AUTONOME (installé à côté du moteur)
rose-shell prog.rose          # charge le programme, puis console
rose-shell prog.rose --sans-exec   # charge sans exécuter les >>
```

`rose-shell` est un **lanceur** (≈ 400 Ko, pas 7 Mo) : il localise `rose.exe`
installé — à côté de lui dans `bin\`, dans le `PATH`, ou via `ROSE_EXE` — et lui
délègue `rose console`. Le moteur n'est donc compilé qu'une fois, et le shell
hérite forcément de la version du moteur installé (aucune désynchronisation
entre un `rose.exe` 0.1.5 et un `rose-shell.exe` d'une autre version).

L'installeur dépose aussi un **profil Windows Terminal**
(`%LOCALAPPDATA%\Microsoft\Windows Terminal\Fragments\Rose\rose.json`) :
« Rose » apparaît dans le menu déroulant des onglets, avec son icône et son
titre d'onglet, sans toucher à `settings.json`. Le désinstalleur retire ce
fragment.

Édition de ligne réelle (console brute `ReadConsoleInputW`, sans dépendance) :

| Touche | Effet |
|---|---|
| ← / → | déplace le curseur |
| Home / End | début / fin de ligne |
| Retour arrière / Suppr | édite au milieu de la ligne |
| ↑ / ↓ | historique (le brouillon en cours est conservé) |
| Tab | complète le mot sous le curseur |
| Ctrl-C | annule la saisie (`^C`) |
| Ctrl-L | efface l'écran |
| Ctrl-Z | termine |
| Échap | efface la ligne |

**Compatibilité terminal** : Windows Terminal, conhost, PowerShell, cmd,
Git Bash, VS Code, ainsi que tout terminal ANSI sous Linux/macOS. Au
démarrage, `repl::preparer_console` pose `SetConsoleOutputCP(65001)` (UTF-8,
donc accents et symboles mathématiques corrects) et active
`ENABLE_VIRTUAL_TERMINAL_PROCESSING` / `ENABLE_VIRTUAL_TERMINAL_INPUT` : sans
cela, sur conhost, les accents sortent en mojibake et les codes couleur
s'affichent littéralement. Les couleurs se désactivent avec
`ROSE_NO_COLOR=1` et se forcent avec `ROSE_COLOR=1` (utile dans un pipe).

Commandes : `:help`, `:show`, `:reset`, `:load <fichier>`, `:save <fichier>`,
`:type <expr>`, `:latex`, `:quit`. Les blocs multi-lignes (accolades) sont
acceptés, et un `{` à l'intérieur d'un commentaire `#` ne prolonge pas la
saisie.

### Pièges connus (à lire avant d'écrire du Rose)

1. **`i` minuscule est l'imaginaire** (`3 + 4i`, `i * i` → `-1`).
   Le lexème `i` est partagé avec les variables : dans une boucle
   `pour i de …`, `i` devient la VARIABLE entière. Pour une variable
   compteur hors boucle `pour`, **préfère un autre nom** (`$compteur`) :
   `tant_que ($i > 0) { $i = $i - 1 }` fonctionne, mais dans un contexte
   ambigu `i` reste l'imaginaire.
2. **`pour … vers B` est exclusif** : `pour j de 0 vers 3` → j = 0, 1, 2.
3. **Listes homogènes** : le vérificateur exige un seul type par liste.
4. **Corps raccourcis `[ ]`** : instructions séparées par `;` (jamais `,`).
5. **`<<` saisie** : la variable doit exister au préalable pour un type
   autre qu'entier ; `<< x` lit un entier si `x` est inconnue.
6. **`/` est la division exacte** ; la division entière s'écrit `÷`.
7. **Typage fort dynamique** : promotions numériques OK, mais chaîne ↔
   nombre refusé (`$n = 5` puis `$n = "mot"` = erreur).
8. **Fonctions typées = voie rapide** : les fonctions dont paramètres/retour
   sont `entier`/`flottant`/`booleen` passent par le JIT puis l'AOT ;
   les fonctions non typées ou à retour complexe/chaine restent interprétées.
9. **Backtick interdit** dans les noms ; les accents sont permis.
10. Le moteur tourne dans un thread à pile de 8 Mo (récursion profonde OK).

---
