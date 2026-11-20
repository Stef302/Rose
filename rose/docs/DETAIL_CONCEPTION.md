# DETAIL_CONCEPTION — affichage paresseux des nombres géants façon Wolfram

**Document de CONCEPTION uniquement. Aucun fichier source modifié, aucun commit, aucune publication.**
Dépôt : `/home/clausake/Stefan/ROSE` — HEAD `2961f3d636b0b0206d1e511de10ceaa8fa8caa0a` — jour 21/11/2026.
Rédigé par le sous-agent RECH1. Toutes les références de lignes sont prises sur cet HEAD.

---

## 0. Périmètre et conclusion courte

Le produit demandé (afficher 30 chiffres d'un factorial géant puis « … », `more digits`, et une
fonction `detail()` dépendante du type) **ne peut pas être obtenu par un simple changement de
formatage**. La raison est factuelle et se lit dans le code : aujourd'hui `n!` est *matérialisé*
en un `BigInt` complet avant tout affichage (§1.3). Pour `n = 10^37` — l'exemple de l'écran — le
calcul échoue avant même l'affichage (§2.2). Il faut donc introduire une **valeur symbolique
paresseuse** qui ne stocke pas le nombre mais sa *description*, et dérive les propriétés à la
demande.

Le point d'entrée d'affichage est unique et déjà bien identifié : `run::afficher` /
`run::afficher_formule` (run.rs:1039-1071), appelés par l'interpréteur en
`interp.rs:3796-3813` et par la primitive `afficher` en `interp.rs:4852`. C'est là — plus
précisément dans `chaine_de_p` (run.rs:959-1026) — que se branche le rendu tronqué.

---

## 1. Point précis sur le code existant

### 1.1 Le type `Valeur` et la surface d'affichage

`Valeur` est définie dans `run.rs:354-368` avec **12 variantes**, dans cet ordre :

```
Vide, Entier(i64, Option<Rc<Gz>>), Rationnel(Rc<Gq>), Flottant(f64), Booleen(bool),
Chaine(Rc<ChaineRep>), Liste(Liste), Table(Table), Tampon(Tampon), Tableau(Tableau),
Complexe { re: f64, im: f64 }, Irrationnel(Rc<Gz>)
```

Les constructeurs sont dans `run.rs:370-411` (`Valeur::z`, `zg`, `qq`, `f`, `b`, `s`, …).
`Valeur::z` (run.rs:371-385) récupère un grand entier depuis l'ABI taggée quand `|v| >
0x1000_0000` ; `Valeur::zg` (run.rs:386-388) choisit `i64` si `fits_i64()`, sinon
`Entier(0, Some(Rc<Gz>))`. Côté interpréteur, `zg_rapide` (interp.rs:5911-5916) fait la même
chose sans cloner les limbs (`bref().to_i64()`).

`TyKind` est défini dans `front.rs:20` : `Vide, Entier, Rationnel, Flottant, Booleen, Chaine,
Liste, Table, Tampon, Tableau, Complexe, Irrationnel, Dyn` — 13 valeurs (avec `Dyn`). La
correspondance `Valeur → TyKind` est `Valeur::ty()` (run.rs:413-427), **match exhaustif**.

Chaîne de rendu :

| Fonction | Lignes | Rôle |
|---|---|---|
| `q_chaine` | run.rs:886-891 | `a/b` exact depuis les `BigInt`, sans troncature `i128` |
| `texte_rationnel` | run.rs:893-947 | décimale périodique `0.[3]`, seuil 500 chiffres → `a/b` |
| `chaine_de` | run.rs:949-955 | enveloppe de `chaine_de_p(v, false)` ; rationnels **bruts** |
| `chaine_de_p` | run.rs:957-1026 | **match exhaustif** sur les 12 variantes |
| `afficher_formule` | run.rs:1028-1055 | préfixe LaTeX éventuel (presque toujours vide) |
| `afficher` | run.rs:1057-1071 | point unique ; `match` avec rejets `_` |

`afficher` (run.rs:1062-1069) traite spécialement `Rationnel` (périodique), `Liste`
(périodique récursive), `Tampon` (erreur), `Tableau`/`Complexe` (via `chaine_de`), et renvoie
tout le reste — **y compris `Entier(0, Some(Gz))`** — à `chaine_de`. C'est exactement ce chemin
qui imprime aujourd'hui les 5 565 709 chiffres de `1000000!`.

Deux détails importants pour la conception :
- `chaine_de_p` gère déjà l'élision pour les **tableaux** (`run.rs:1010-1017` : au-delà de 8
  éléments, `[a, b, c, d ... N elems]`) et `texte_rationnel` sait déjà mettre `'…'`
  (run.rs:942-944). Le vocabulaire « … » est donc déjà présent, mais aucun pour les entiers.
- `ecrire_out` (run.rs:567-…) est la sortie unique, avec capture pour WASM/bibliothèque
  (run.rs:537-567). Toute nouvelle sortie doit passer par là.

### 1.2 L'arithmétique exacte (`i128x.rs`)

`i128x.rs:1-4` : « arithmétique exacte SANS GMP, sans FFI ; Entiers = `num_bigint::BigInt`,
Rationnels = `(BigInt, BigInt)` ; surface `Gz/Gq` inchangée ».

- `Gz` = `*mut Mpz` avec `Mpz { v: BigInt }` (i128x.rs:13-14, 457-466). `Drop` à la ligne
  623-625. Accès : `bref() -> &BigInt` (502), `v() -> BigInt` (clone, 499), `chaine()` (546-552)
  qui délègue à `mul_rapide::vers_chaine` (conversion par multiplication, 2× plus rapide que
  `num-bigint`, commentaire 542-545).
- `sizeinbase10()` (i128x.rs:559-562) est `x.to_string().len()` — **coûte une conversion
  complète**, donc inutilisable sur un objet paresseux.
- `Gq` = `*mut Mpq` (i128x.rs:627-747), avec `num_bigint()` / `den_bigint()` (664-665) et
  `chaine()` (671) ; `q_chaine` (run.rs:886) l'utilise pour éviter le piège `num_i128()=0`
  au-delà de 126 bits (commentaire run.rs:887-889).
- `factorielle_parallele(n: u64) -> Gz` (i128x.rs:20-…, signature 27) : crible d'Ératosthène
  `vec![true; n+1]` (ligne 37), exposants de Legendre `e_p = Σ⌊n/p^k⌋` (ligne 48,
  commentaire 21), puis arbre de produit glouton multi-cœur ; repli séquentiel sous WASM
  (lignes 63-70). Le commentaire 26 annonce « 1 000 000 ! ≈ 0,9 s ».
- `super_factorielle_parallele` (364) et `arbre_produit` (311) réutilisent le même moteur.
- Aucune fonction de logarithmme, d'exponentielle, de chiffres de tête ou de chiffres finaux
  n'existe aujourd'hui dans `i128x.rs` (vérifié par recherche : `Stirling|log10|zeros|
  trailing|non-zero` → 0 occurrence dans `rose/docs/`, et aucune dans `i128x.rs`).

### 1.3 Chaîne de calcul d'une factorielle (le blocage)

1. **Postfix `!`** : `Expr::Factorielle(a, _)` en `interp.rs:3303-3307` fait
   `let z = vers_z(&v); zg_rapide(factorielle(&z))`. Même chose pour `!!`, `!n`, `#`, `?`, `$`
   (3308-3332). Il n'existe ** aucun ** nœud AST « factorielle paresseuse ».
2. **Primitive fonctionnelle** : `nom_std_connu` (interp.rs:1070-1085, liste blanche) inclut
   `"factorielle" | "fact"` (ligne 1078) ; `stdlib_valeur` (4850+) les appelle en 4993 :
   `"factorielle" | "fact" => { let z = vers_z(&a[0]); zg_rapide(factorielle(&z)) }`.
   Variantes rapides 4994-4998 (`factorielle_rapide`, `factorial_approx`).
3. **Implémentation** : `run::factorielle` (run.rs:1166-1173) refuse tout `n` qui ne tient pas
   dans `i64` **ou** qui dépasse `0xFFFF_FFFF` — message `erreur_runtime("factorielle trop
   grande")` à la ligne 1168 — puis appelle `i128x::factorielle_parallele`.
4. **Affichage** : `>> n!` passe par `afficher_expression` (interp.rs:3800-3813) →
   `afficher_formule` → `afficher` → `chaine_de_p` → `z.chaine()` → 3,6×10^38 chiffres.

Le nombre de chiffres est exposé par `nb_chiffres` (run.rs:1207-1210) et la primitive
`"nb_chiffres" | "digits"` (interp.rs:5071) — mais `nb_chiffres` appelle `sizeinbase10`, donc
*transforme déjà le nombre entier en chaîne* avant de compter. Inutilisable en mode paresseux.

### 1.4 Approximations et « viewer » déjà présents (oracles à ne pas casser)

- `factorial_approx(n)` (interp.rs:6999-7018) : série de Stirling en **f64**,
  `ln(n!) ≈ n ln n − n + ½ln(2πn) + 1/(12n) − 1/(360n³) + 1/(1260n⁵)`, renvoie
  `"{:.6}e{}"` (mantisse, exposant). Exposée en `factorial_approx`/`approx` (4997-4998).
  Tests associés : interp.rs:7293-7323 — `10! → "3.628800e6"`, `0!`/`1! → "1"`, `100! →
  9.33262…e157`, `100000! → …e456573`, et l'assertion explicite que ce n'est **pas** le nombre
  exact. Ce test doit rester vert : la nouvelle voie paresseuse ne doit pas modifier
  `factorial_approx`.
- `view_grand_nombre(s)` (run.rs:1933-2048) : pager « nano/less » sur une chaîne déjà
  matérialisée, avec navigation clavier (espace/b/q/flèches), fenêtre de 70×20 caractères et
  index d'octets O(1) par page (ligne 1979). Primitive `"view"` en interp.rs:4853. Il s'agit
  d'un **visualiseur de chaîne**, pas d'un calcul paresseux : il suppose le nombre déjà
  convertible en texte. À conserver tel quel.
- `nombre_pi` / `nombre_e` / `nombre_phi` / `racine_decimales` (interp.rs:5072-5074,
  5044-5068) utilisent `irrationnel::calcul_*` (irrationnel.rs:85, 176, 249, 269, 330) qui
  produisent des décimales en précision arbitraire avec `BigInt`. Ce sont les seules
  infrastructure de précision arbitraire existantes dans le dépôt (aucun `rug`/`mpfr`).
- `i128x.rs:1183-1197` : test `factorielle_petits_entiers` (0,1,5,10,20,25,100 — chaîne
  complète de 100!). Test `arbre_sequentiel_egal_au_parallele` (1199-…) pour 1001, 2000, 5000.

### 1.5 REPL et GUI (points d'extension)

- `repl.rs:440-520` : les commandes `:` sont parsées par `split_once(char::is_whitespace)`
  (442-445) puis `match cmd` (446-517). Liste actuelle : `:quit/:q/:exit`, `:reset`,
  `:show/:code`, `:type`, `:latex`, `:load`, `:save`, `:help/:h/:?`. Le `_` final (517) rend
  une erreur « commande inconnue ». Un `:more` s'ajoute en **une seule branche** avant le `_`.
- `repl.rs:194-205` : `COMPLETIONS` (Tab) — ajouter `":more"`, `"detail"`,
  `"plus_de_chiffres"`, `"chiffres"`.
- `repl.rs:250-271` : `aide()` — tableau `[(&str,&str); 8]` à passer à 9 (ou plus). Le type
  du tableau est explicite, donc l'ajout d'une entrée impose de changer la taille.
- `repl.rs:529+` : une ligne normale est préfixée `>> ` et envoyée à `executer`. Le `:more`
  devra donc agir sur le **dernier résultat affiché**, pas sur une expression à réévaluer : il
  faut un état minimal dans la console (poignée vers l'objet paresseux courant).
- GUI native : `gui_native.rs` expose seulement `fenetre_creer` (173), `fenetre_afficher`
  (204), `fenetre_attendre` (240), `fenetre_fermer` (301), `message` (330) et
  `compter_fenetres`/`creer_simulee`. **Aucun widget, aucun bouton, aucun événement de clic**
  côté moteur (doc 1-20 : « Ce module ne sait RIEN du langage Rose »).
  `jardins/gui_native.rose` est la source de vérité de l'interface : `fenetre()` (51),
  `bouton()` (63), `champ()` (67), `etiquette()` (71), `ajouter()` (90),
  `lier_evenement()` (99), `evenement()` (112), `decrire()` (175), `ouvrir()` (193). Un
  bouton « more digits » est donc un `bouton(...)` + un `lier_evenement(..., "action", ...)`
  **côté Rose**, et non une primitive Rust à ajouter. Le pontier n'a pas à changer ; le
  rendu réel passe par `ecrire_ecran(texte)` / l'ELF généré (`generer_ecran`, 228-231).
- `lib.rs:37-130` : modules publics — `i128x`, `interp`, `run`, `gui_native`, `repl`, `front`,
  etc. Aucun module nouveau n'est nécessaire ; tout se loge dans les modules existants.

---

## 2. Objectif produit — identification exacte de l'exemple

L'écran fourni montre une factorielle d'environ `10^38` chiffres. En résolvant les
caractéristiques données on identifie sans ambiguïté **n = 10^37** :

| Propriété annoncée | Valeur calculée pour `n = 10^37` | Concordance |
|---|---|---|
| « Number length » `3.65657×10^38` | `log10((10^37)!) = 3.6565705518096748…×10^38` (série de Stirling, mpmath 80 décimales) | oui |
| « Power of 10 » `10^(10^38.56…)` | `log10(3.6565705518096748×10^38) = 38.563073957…` | oui (38.56…) |
| « Number of trailing zeros » `2499…990` | `Σ⌊10^37/5^k⌋ = 2499999999999999999999999999999999990` (37 chiffres, fin `…990`) | oui |
| « Last non-zero digits » `…6970467172352` | valeur **fournie par l'utilisateur** (13 chiffres) | non recalculée ici (voir §4.5) |

**Conséquence capitale** : `10^37` ne tient pas dans un `i64` (max `9,22×10^18`), donc
`run::factorielle` renvoie immédiatement « factorielle trop grande » (run.rs:1168). Même en
assouplissant ce test, le crible `vec![true; n+1]` de `i128x.rs:37` demanderait `10^37` octets.
L'exemple est **hors de portée du chemin actuel** : il impose la valeur symbolique (§3).

Valeurs de contrôle vérifiées localement (Python, `math.factorial`) :
`100!` → **158 chiffres** ; `1000!` → **2568 chiffres** ; zeros finaux de `1000000!` →
**249998** ; `10! = 3628800` → derniers chiffres non nuls **36288** (donc `…288`). Ces quatre
valeurs sont celles du mission ; elles sont exactes.

---

## 3. Conception : valeur symbolique paresseuse

### 3.1 Principe

Introduire une variante `Valeur::Symbolique(Rc<EntierSymbolique>)`, où l'objet décrit un
entier sans le matérialiser. Le cas de départ est la factorielle, mais le type est extensible
(puissance, primorielle, produit…).

```rust
// dans run.rs, à côté de Valeur (après ligne 368)
pub enum EntierSymbolique {
    Factorielle(u64),                 // n! — n tient dans u64 (10^37 oui)
    // plus tard : Primorielle(u64), SuperFactorielle(u64), Produit(Vec<...>)
}

pub struct ObjetParesseux {
    pub kind: EntierSymbolique,
    pub chiffres_demandes: Cell<usize>,      // 30 par défaut
    pub cache: RefCell<CacheParesseux>,      // precise digits + results
}

pub struct CacheParesseux {
    pub precision: usize,              // chiffres de log10 calculés (guard inclus)
    pub log10_fraction: Option<String>,// frac(log10 n!) en P décimales
    pub log10_entier: Option<BigInt>,  // partie entière de log10(n!)
    pub premiers: Option<String>,      // chiffres de tête, longueur >= precision demandée
}
```

`Rc` est obligatoire : `Valeur` est `Clone` (run.rs:354) et circulera dans les globals du REPL
et les tables Rose. `RefCell`/`Cell` permettent le `more digits` sans `&mut Valeur`.

**Où brancher l'affichage paresseux** :

1. `chaine_de_p` (run.rs:959-1026) : une branche
   `Valeur::Symbolique(o) => o.premiers_tronques()` qui rend `<30 chiffres>…` (ou le nombre
   complet s'il tient en moins de 30 chiffres). C'est le seul point à toucher pour que
   `afficher`, `afficher_formule`, `>> expr`, `chaine()` et tous les appels existants
   héritent du comportement.
2. `afficher` (run.rs:1058-1071) : **aucune modification nécessaire** — son `_` (1068)
   délègue déjà à `chaine_de`.
3. `afficher_formule` (run.rs:1039-1055) : `_` (1052) délègue déjà à `chaine_de`. Rien à faire.
4. `afficher_valeur` / `afficher_expression` (interp.rs:3796-3813) : rien à faire.
5. `view_grand_nombre` (run.rs:1945-2048) : rien à faire ; il reçoit la chaîne tronquée. Le
   pager complet resterait accessible via `view(chaine_complete(...))` si l'on ajoute un
   matérialiseur explicite (hors périmètre RECH1).

**Création de la valeur** : ne **pas** changer `Expr::Factorielle` (interp.rs:3303-3307),
sinon tous les tests existants de `factorielle` changeraient de type (oracle cassé, §5.3).
Ajouter une primitive explicite — par exemple `entier_geant_factorielle(n)` — enregistrée dans
`nom_std_connu` (interp.rs:1071-1085) puis branchée dans `stdlib_valeur` (4850+) :

```rust
"entier_geant_factorielle" => {
    let z = vers_z(&a[0]);
    if z.sgn() < 0 || !z.fits_u64() { erreur_runtime("factorielle paresseuse : n hors plage u64"); }
    Valeur::Symbolique(Rc::new(ObjetParesseux::factorielle(z.get_u64())))
}
```

Un raccourci syntaxique (nouveau postfix, ex. `n¡`) pourrait être ajouté plus tard dans
`front.rs`, mais ce n'est pas nécessaire pour la première version et cela toucherait le
parseur : à éviter.

### 3.2 API Rust minimale proposée

#### `i128x.rs` (couche MATH1 — calculs sur `n`, pas sur `n!`)

```rust
/// log10(n!) en virgule fixe BigInt : partie entière + `prec` décimales.
/// Série de Stirling, précision arbitraire (voir §4.1).
pub fn log10_factorielle(n: u64, prec: usize) -> (BigInt, BigInt);

/// Nombre de chiffres décimaux de n! = floor(log10(n!)) + 1.
pub fn longueur_factorielle(n: u64, prec: usize) -> BigInt;

/// `k` premiers chiffres décimaux de n! (sans point décimal).
pub fn premiers_chiffres_factorielle(n: u64, k: usize) -> String;

/// Σ_{j>=1} floor(n / 5^j) — nombre de zéros finaux de n!.
pub fn zeros_finaux_factorielle(n: u64) -> BigInt;

/// n! / 10^B(n) mod 10^k, sous forme de chaîne de k chiffres (voir §4.5).
pub fn derniers_non_nuls_factorielle(n: u64, k: usize) -> String;
```

Ces fonctions n'appellent **jamais** `factorielle_parallele` : elles ne font que des
arithmétiques sur `n` et sur des `BigInt` de taille `O(prec)` ou `O(k)`.

#### `run.rs` (Valeur symbolique + rendu)

```rust
impl ObjetParesseux {
    pub fn factorielle(n: u64) -> Self;
    pub fn premiers_tronques(&self) -> String;       // utilisé par chaine_de_p
    pub fn demander_chiffres(&self, plus: usize) -> String; // recalcule, voir §4.6
    pub fn proprietes(&self) -> Vec<(String, String)>; // pour detail()
}
pub fn afficher_detail(p: &[(String, String)]);       // rendu multi-lignes via ecrire_out
```

`proprietes()` est la seule fonction « typée » : elle ne renvoie que les cinq lignes
pertinentes pour une factorielle (§4.7).

#### `interp.rs` (dispatch)

```rust
// dans nom_std_connu (1071-1085), ajouter les 4 noms ;
// dans stdlib_valeur (4850+), 4 branches :
"detail"            => { /* Valeur::Table des propriétés, voir §4.7 */ }
"chiffres"          => { /* (valeur, nombre) -> String, défaut 30 */ }
"plus_de_chiffres"  => { /* (objet, combien) -> String, défaut 30 */ }
"entier_geant_factorielle" => { /* constructeur ci-dessus */ }
```

La branche `detail` dispatche sur la **variante de `Valeur`**, jamais sur une chaîne : c'est ce
qui garantit qu'on n'affiche pas « Number of trailing zeros » pour un rationnel.

#### `repl.rs`

```rust
":more" | ":p" => { /* appelle plus_de_chiffres sur dernier_objet, sinon message d'aide */ }
```
+ `COMPLETIONS` (195-205), `aide()` (253-262 : passer `[(&str,&str); 9]`), et un champ
`dernier_objet: Option<Rc<ObjetParesseux>>` dans la boucle de la console, mis à jour quand
l'expression affichée est symbolique.

#### GUI

Aucune primitive Rust. Dans `jardins/gui_native.rose` :

```
$f = fenetre("Détail de n!", 640, 480)
$f = ajouter($f, etiquette(detail(chaine)), 10, 10)
$f = ajouter($f, bouton("more digits", 120), 10, 300)
$f = lier_evenement($f, "more", "clic", "chiffres",
                    "plus_de_chiffres(objet, 30)")
```

`decrire(f)` (jardins/gui_native.rose:175-189) reste testable sans écran ; c'est elle qui
servira d'oracle pour le bouton (présence de la ligne `W|bouton|more digits`).

### 3.3 Risque d'exhaustivité des `match` sur `Valeur` — stratégie la moins invasive

J'ai recensé les `match` portant sur `Valeur`. **Deux seulement sont exhaustifs sans branche
`_`** :

| Occurrence | Lignes | Action obligatoire |
|---|---|---|
| `Valeur::ty()` | run.rs:413-427 | ajouter `Valeur::Symbolique(..) => TyKind::Entier` (ou nouveau `TyKind::Symbolique`, voir ci-dessous) |
| `chaine_de_p()` | run.rs:959-1026 | ajouter la branche `premiers_tronques()` |

Les autres ont déjà un rejet générique et **tomberont en cascade sans modification** :
`valeur_egale` (run.rs:855-881, `_ => cmp_vals(a,b) == 0`), `afficher` (1062-1069),
`afficher_formule` (1048-1053), `vers_z_dans` (interp.rs:5921-5945, `_ => erreur_runtime`),
`rnk` (interp.rs:5973-5979, `_ => 0`), `binaire_valeurs`/`unaire_valeur` (interp.rs:4261-4327),
`longueur` (5076-5095, `_ => chaine_de(...)`).

**Stratégie recommandée** (deux lignes de code + un `TyKind`) : ajouter la variante, ajouter les
deux branches ci-dessus, et faire pointer `ty()` vers `TyKind::Entier`. Raison : un
`TyKind::Symbolique` supplémentaire obligerait à toucher `front.rs:20` et tous les appels qui
prennent une décision sur `TyKind` (notamment `lire_slot` en interp.rs:3816+), sans bénéfice
fonctionnel — le symbole *est* un entier du point de vue du langage.

**Alternative évoquée et rejetée** : conserver `Valeur::Entier(0, Some(Rc<Gz>))` et stocker le
descripteur dans une table thread-local indexée par le pointeur `Gz::p()` (i128x.rs:494).
Elle évite les deux branches mais introduit un cycle de vie fragile (réutilisation d'adresse
après `Drop`, i128x.rs:623-625) et un état global invisible. Le coût de la stratégie recommandée
(2 branches) est plus faible que son risque.

**Règle de non-régression** : ne jamais modifier le corps de `Expr::Factorielle`
(interp.rs:3303-3307), ni `factorielle` (run.rs:1166-1173), ni `factorielle_parallele`
(i128x.rs:27). Le chemin exact reste le chemin par défaut ; le mode paresseux est **opt-in** par
le constructeur explicite.

---

## 4. Algorithmes et bornes de validité

### 4.1 `log10(n!)` par série de Stirling en précision arbitraire

Formule (même série qu'`factorial_approx`, interp.rs:7008-7013, mais évaluée en virgule fixe) :

```
ln(n!) = n·ln n − n + ½·ln(2πn) + Σ_{k≥1} B_{2k} / (2k(2k−1) n^(2k−1))
       = n·ln n − n + ½·ln(2πn) + 1/(12n) − 1/(360n³) + 1/(1260n⁵) − 1/(1680n⁷) + …
log10(n!) = ln(n!) / ln(10)
```

`B_{2k}` = nombres de Bernoulli (1/6, −1/30, 1/42, −1/30, 5/66, …).

Implémentation en virgule fixe `BigInt` avec échelle `S = 10^P` (`P = précision demandée +
gardes`, typiquement `D + 10`) :
- `ln x` : réduction `x = m·2^e` puis `ln x = e·ln 2 + ln m`, avec
  `ln m = 2·atanh((m−1)/(m+1)) = 2·Σ_{j≥0} t^(2j+1)/(2j+1)`, `t = (m−1)/(m+1)`, série
  convergente pour `m ∈ [1,2)` ; `ln 2` calculé une fois par la même série avec accélération
  (`t = 1/3`) ;
- `1/n^p` : division entière sur `BigInt` avec échelle `S` ;
- `ln(10)` : même routine, constant mis en cache.

**Bornes de validité** :
- la série est **asymptotique et divergente** : on s'arrête au terme décroissant minimal.
  L'erreur de troncature est de l'ordre du premier terme omis. Pour `n ≥ 100`, 7 termes
  donnent déjà ~27 décimales (calcul de contrôle : `1/(1188·100^9) ≈ 8×10⁻²¹`,
  `361/(1188·100^13) ≈ 3×10⁻²⁸`) ; pour `n ≥ 10^6`, la précision effective dépasse 100
  décimales ;
- pour `n < 100`, **ne pas utiliser la série** : calculer `n!` exactement et prendre son
  `chaine()` (i128x.rs:546). Cela couvre tous les oracles petits (§5) ;
- `P` doit rester ≤ quelques centaines (coût des divisions `BigInt` en `O(P²)`) ; au-delà,
  plafonner et signaler.

### 4.2 Nombre de chiffres

`D(n) = floor(log10(n!)) + 1`. Avec `log10(n!)` connu sous forme (partie entière `E`, fraction
`F`) : `D = E + 1`. Le résultat est un `BigInt` (10^38 tient large dans un `BigInt`).

**Piège à documenter** : ne jamais appeler `nb_chiffres` (run.rs:1207) ni `sizeinbase10`
(i128x.rs:559) ici — tous deux passent par `to_string()` du nombre complet.

*Oracles* : `D(100) = 158`, `D(1000) = 2568`, `D(10^37) ≈ 3,65657×10^38`.

### 4.3 Premiers chiffres

Si `log10(n!) = E + F` avec `0 ≤ F < 1`, alors `n! = 10^E · 10^F` et la mantisse décimale est
`10^F ∈ [1,10)`. Les `D` premiers chiffres sont :

```
premiers = floor( 10^F · 10^(D−1) )
```

`10^F` est évalué en virgule fixe `BigInt` : `F` étant dans `[0,1)`, on utilise
`exp(F·ln 10)` avec la série de Taylor réduite (`exp(x) = 2^j · exp(r)`, `|r| < 1/2`,
`exp(r) = Σ r^i / i!`), le tout à l'échelle `10^(D+gardes)`. On garde `D+2` chiffres puis on
arrondit vers le bas ; les 2 chiffres de garde absorbent l'erreur d'arrondi de `F`.

**Validité** : nécessite `F` connu avec au moins `D + log10(D) + 3` décimales correctes. Si le
calcul donne une mantisse ≥ 10 (arrondi), retrancher 1 à `E` et recommencer avec `F += 1` —
cas limite rare mais réel. Pour `n < 100`, lire directement les chiffres de `n!` exact.

*Oracles* : `100!` commence par `93326215443944152681…` (chaîne complète en
i128x.rs:1193) ; `10!` commence par `36288`.

### 4.4 Zéros finaux (Légendre)

`B(n) = Σ_{j≥1} floor(n / 5^j)` — boucle sur `j` jusqu'à `5^j > n`, soit `⌊log_5 n⌋`
itérations (53 pour `n = 10^37`). Opérations sur `u64`/`BigInt` uniquement, **aucune
matérialisation de `n!`**.

*Oracles* : `B(10!) = 2` (3628**00**) ; `B(100!) = 24` ; `B(1000!) = 249` ;
`B(1000000!) = 249998` (vérifié) ; `B(10^37) = 249999…990` (vérifié).

### 4.5 Derniers chiffres non nuls (mod 10^k après retrait des facteurs 2 et 5)

Écrire `n! = 2^{A(n)} · 5^{B(n)} · F(n)` où `F` est premier à 10,
`A(n) = Σ_{j≥1} ⌊n/2^j⌋`, `B(n)` comme en §4.4. Comme `A > B` pour `n ≥ 2`, le nombre de
zéros est `B` et la partie non nulle est `R = F(n) · 2^{A−B}`.

Calcul par **CRT** (algorithme vérifié localement pour `k = 1..5` contre `math.factorial`, sur
`n = 8,10,12,20,25,50,100`) :

- modulo `5^k` : poser `h(n) = ∏_{j≥0} ∏_{1≤i≤⌊n/5^j⌋, 5∤i} i  (mod 5^k)` ; alors
  `R ≡ h(n) · 2^{−B(n)}  (mod 5^k)` (2 est inversible modulo `5^k`) ;
- modulo `2^k` : dès que `A(n) − B(n) ≥ k`, `R ≡ 0 (mod 2^k)` — vrai pour tout `n` un peu
  grand ; sinon calculer `F` par produit direct et multiplier par `2^{A−B}` ;
- recombiner : `R = R5 + 5^k · t` où `t ≡ (R2 − R5)·(5^k)^{−1} (mod 2^k)`.

Le produit d'unités `∏_{1≤i≤r, 5∤i} i (mod 5^k)` se calcule par blocs : le produit de toutes
les unités modulo `5^k` vaut `−1` (Wilson généralisé, `5^k` impair), donc
`P(r) = (−1)^{⌊r/5^k⌋} · ∏_{1≤i≤r mod 5^k, 5∤i} i`.

**Bornes de validité** : la boucle de bloc coûte `O(5^k)` ; `k ≤ 8` (`5^8 = 390 625`) est
immédiat ; `k = 13` (les 13 chiffres de l'exemple) demande `5^13 ≈ 1,22×10⁹` itérations par
 niveau → **inacceptable**. Pour `k ≥ 9` il faut une décomposition récursive du produit
 partiel par blocs de `5^(k−1)` (coût `O(k·log n·poly(k))`), ou plafonner `k` et l'annoncer
 explicitement. La valeur `…6970467172352` de l'écran est **donnée par l'utilisateur** et n'a
 pas été recalculée ici ; elle doit servir d'oracle de validation de l'implémentation `k=13`,
 pas de résultat présumé.

*Oracles vérifiés* : `10!` → `36288` (donc `…288`) ; pour `100!`, les derniers chiffres non
 nuls se lisent dans la chaîne de i128x.rs:1193.

### 4.6 « more digits » — recalcul, pas mémoïsation interdite

Le mission l'autorise explicitement : un **cache local à l'objet paresseux** est acceptable.
Règle retenue :

1. `premiers_tronques()` demande `D = 30` chiffres. Si `cache.precision ≥ D + gardes`,
   renvoie `cache.premiers[..D] + "…"` immédiatement.
2. Sinon, **recalcule depuis `n`** (jamais depuis les chiffres déjà affichés) avec
   `P = D + 10`, met à jour `log10_fraction`, `log10_entier`, `premiers`, puis `precision`.
3. `plus_de_chiffres(objet, m)` pose `D ← D + m` et refait l'étape 2. La précision ne fait
   que **croître** : un appel avec `m ≤ 0` est un no-op documenté.
4. Le cache est stocké dans `RefCell<CacheParesseux>` ; pas de verrou global, pas de mémoire
   partagée entre objets. Sa taille est `O(D)`, jamais `O(n!)`.

Pourquoi il faut recalculer : `log10(n!)` et `10^F` sont des fonctions de `P` ; les chiffres
déjà produits à `P` décimales ne permettent pas d'en déduire `P+30` décimales correctes sans
reprise de la série. Le coût d'un `more digits` est donc celui d'un nouveau calcul de
`O(P² log n)`, pas d'un simple concaténage.

### 4.7 `detail()` dynamique selon le type

`detail(v) -> Valeur::Table` (table Rose, construite avec `Valeur::table()` +
`Table::inserer`, run.rs:408 / interp.rs:5097). Le rendu multi-lignes est fait par
`afficher_detail` (§3.2) ; `afficher(detail(x))` reste valide et rend la table compacte.

Propriétés **par variante** (aucune propriété d'un type n'est affichée pour un autre) :

| Variante de `Valeur` | Propriétés renvoyées |
|---|---|
| `Symbolique(Factorielle(n))` | Power of 10 representation ; First few digits ; Number length ; Number of trailing zeros ; Last non-zero digits |
| `Entier(..)` ordinaire | Signe ; Number of decimal digits ; Parité ; éventuel test de primalité (plafonné, via `est_premier`, run.rs:1203) |
| `Rationnel(q)` | Numérateur ; Dénominateur ; Signe ; longueur de la période décimale (via `texte_rationnel`, run.rs:895) |
| `Flottant(d)` | Classe (fini/NaN/infini) ; Signe ; exposant binaire (`to_bits`) ; représentation décimale (`texte_flottant`) |
| `Booleen(b)` | Valeur (`vrai`/`faux`) |
| `Chaine(s)` | Longueur en caractères (`s.n`, O(1), cf. interp.rs:5078-5081) ; aperçu tronqué |
| `Liste/Table/Tableau/Tampon` | Nombre d'éléments ; types des éléments (échantillon) |
| `Complexe{re,im}` | Partie réelle ; partie imaginaire ; module ; argument |
| `Irrationnel(z)` | Expression (`√n`) ; nombre de décimales disponibles |
| `Vide` | `(vide)` |

Le dispatch est un `match` sur `v` avec **une branche par variante** et un `_ => unreachable!()`
(uniquement dans cette fonction) : c'est la garantie compile-time qu'aucune propriété ne
déborde sur un autre type. `TyKind` n'intervient pas.

---

## 5. Plan de tests et oracles à ne pas casser

### 5.1 Tests unitaires de `i128x.rs` (nouveaux)

| Test | Oracle | Statut |
|---|---|---|
| `zeros_finaux_factorielle(10/100/1000/1000000)` | `2 / 24 / 249 / 249998` | 249998 vérifié localement |
| `derniers_non_nuls_factorielle(10, 5)` | finit par `36288` | vérifié localement |
| `derniers_non_nuls_factorielle(n, k)` pour `k=1..5`, `n∈{8,10,12,20,25,50,100}` | comparaison à `math.factorial(n)` tronqué | **algorithme déjà vérifié localement** |
| `longueur_factorielle(100/1000)` | `158 / 2568` | vérifié localement |
| `premiers_chiffres_factorielle(100, 20)` | `93326215443944152681` | lu dans i128x.rs:1193 |
| `log10_factorielle(10^37, 30)` | fraction `0.6565705518…`, exposant `38` | vérifié (mpmath) |
| cohérence `10^F` : `premiers_chiffres` commence par `9` pour `10^37` | mantisse `9.0182…` | vérifié (mpmath) |

### 5.2 Tests d'intégration `run.rs` / `interp.rs`

- `detail(fact(10))` ne contient **que** les clés de factorielle ; `detail(1/3)` ne contient
  **aucune** des clés « trailing zeros » / « last non-zero » ; `detail([1,2])` rend
  « nombre d'éléments = 2 ». C'est le test de non-chevauchement exigé.
- `chaine_de_p` sur un `Symbolique` rend exactement 30 chiffres suivis de `…` ; avec un `n`
  dont `n!` a moins de 30 chiffres (`n = 10`), rend la chaîne complète **sans** `…`.
- `plus_de_chiffres(obj, 30)` rend 60 chiffres ; le préfixe de 30 chiffres est **bit-à-bit
  identique** au rendu initial (test de stabilité du recalcul).
- `:more` en console : le résultat change ; `:more` sur un dernier résultat non symbolique
  rend un message d'aide, pas une panique.

### 5.3 Oracles existants à ne PAS casser (tests verts avant/après)

- `i128x.rs:1183-1197` `factorielle_petits_entiers` — chaîne complète de `100!`.
- `i128x.rs:1199-1224` `arbre_sequentiel_egal_au_parallele` (1001, 2000, 5000).
- `interp.rs:7297-7313` `factorial_approx_bonne_taille` — `"3.628800e6"`, `"1"`,
  `9.33262…e157`, `…e456573`. **Ne pas modifier `factorial_approx`** (interp.rs:7001-7018) :
  sa mantisse f64 est un oracle de compatibilité, même si la nouvelle voie est plus précise.
- `interp.rs:7318-7323` `factorial_approx_n_est_pas_le_nombre_exact` — la sortie compacte doit
  rester < 20 caractères.
- `interp.rs:7325-7348` OEIS sous-factorielle / multifactorielle.
- `gui_native.rs:626-666` machine à états des fenêtres (aucun primitive à ajouter).
- `repl.rs:969-997` `braces_balanced` / `est_definition` — l'ajout de `:more` ne doit pas
  changer la détection des définitions Rose.

### 5.4 Limites à documenter dans le code

- `n` doit tenir dans `u64` (le constructeur le vérifie) ; `10^37` convient, `10^100` non.
- Précision maximale plafonnée (proposition : `D ≤ 500`, `k ≤ 9` sans bloc récursif).
- Aucun appel à `factorielle_parallele`, `chaine()` ou `sizeinbase10()` depuis le chemin
  paresseux : c'est l'invariant structural de la refonte. À vérifier par relecture des deux
  branches ajoutées dans `run.rs`.
- Le mode paresseux est **opt-in** : `n!` et `factorielle(n)` gardent leur sémantique exacte
  et leur coût actuel.

---

## 6. État factuel final

- **Fait** : lecture de `run.rs`, `i128x.rs`, `interp.rs`, `repl.rs`, `gui_native.rs`,
  `jardins/gui_native.rose`, `lib.rs`, `front.rs:20`, `irrationnel.rs` ; relevé des lignes
  citées plus haut.
- **Vérifié par exécution** : `100!` = 158 chiffres, `1000!` = 2568 chiffres,
  `zeros(1000000!) = 249998`, derniers non nuls de `10!` = `36288`, identification de
  l'exemple `n = 10^37` (`log10` = 3.6565705518096748×10^38, `log10(log10)` = 38.56307…,
  zeros = 249999…990), et algorithme CRT des derniers non nuls validé pour `k=1..5`.
- **Non vérifié** : la valeur `…6970467172352` (13 chiffres) de l'écran — à utiliser comme
  oracle, pas comme résultat établi.
- **Reste à faire par l'agent d'implémentation** : les 4 fonctions `i128x.rs`, la variante
  `Valeur::Symbolique` + 2 branches de match, les 4 primitives `interp.rs`, `:more`, le bouton
  Rose, et les tests §5. Le présent document ne modifie aucun fichier source et ne contient
  aucun commit.
