# DETAIL_UX — design d'interface « Wolfram-like » pour les grands nombres et `detail()`

> **Document de design, daté 21/11/2026, HEAD `2961f3d`.**
> Périmètre de la mission UX1 : **ce fichier uniquement**. Aucun fichier source
> (`.rs`, `.rose`), aucun commit, aucun patch appliqué.
>
> Convention de vérité : tout ce qui est **existant** est cité avec son fichier
> et ses numéros de ligne, lus dans l'arbre à `2961f3d`. Le 21/11, un autre
> agent a ajouté 49 lignes d'instrumentation SPEED2 dans `rose/src/interp.rs`
> (worktree non commité) : les numéros de ligne de ce document sont ceux de
> **HEAD**, pas ceux du worktree modifié. Je n'ai touché aucun fichier source.
> Tout ce qui est
> **proposé** est marqué « Proposition ». Les valeurs de l'exemple
> `factorielle(10^38)` sont **celles fournies dans la demande**, non recalculées
> ici. Les valeurs de `100!` et `1000000!` données plus bas ont été recalculées
> le 21/11/2026 (`math.factorial`, formule de Legendre) et sont reproductibles.
>
> Ce document suit la règle de méthode rappelée dans
> `rose/docs/COMPARATIF_TOTAL.md` §0 (lignes 1‑14) : un constat est daté, ce
> n'est pas une consigne permanente ; après tout changement de moteur ou de
> backend, re-mesurer avant de réutiliser un résultat.

---

## 1. Points d'entrée existants (constats, avec numéros de ligne)

### 1.1 Console interactive — `rose/src/repl.rs` (1086 lignes)

| Point d'entrée | Lignes | Rôle exact |
|---|---|---|
| En-tête du module partagé | 1‑22 | Le module sert **à la fois** `rose console`/`rose repl` et le binaire autonome `rose-shell`. |
| Palette ANSI `Couleur` | 31‑66 | `prompt()` rouge Rose (55), `continuation()` (57), `info()` vert (59), `erreur()` rouge vif (61), `doux()` gris (63). `actif` = faux hors terminal ou `ROSE_NO_COLOR=1` (36‑48). |
| `COMPLETIONS` | 194‑205 | Liste blanche de Tab, contient les 8 commandes `:` (196) puis les mots-clés, dont `nb_chiffres`, `factorielle`, `est_premier` (198‑205). |
| `completer()` | 208‑211 | Première correspondance par préfixe. |
| `aide()` | 250‑271 | Table **fixe de 8 entrées** aux lignes 253‑262, alignement calculé sur la largeur max (263‑266), puis rappel `>> expr`, ↑/↓, Tab (268‑270). |
| `banniere()` | 273‑308 | Rose ASCII, version, et consigne « `:help` pour l'aide, `:quit` pour sortir » (306). |
| `executer()` | 315‑325 | Lance `interp::executer_source` dans un fil de pile 16 Mio. |
| `est_definition()` | 327‑337 | Décide ce qui reste dans le buffer (`fn `, `=`, `?`, `@`, …). |
| `lancer()` | 345‑542 | Point d'entrée public de la console. |
| Boucle principale | 391‑541 | Invite `rose> ` / `...> ` (399), lecture avec continuation tant que `braces_balanced()` est fausse (217‑246, appel 431), gestion Ctrl‑C / Ctrl‑L (409‑422). |
| Dispatch des commandes `:` | 440‑520 | Test `starts_with(':')` (441), séparation `cmd`/`arg` (442‑445), `match cmd` (446‑518) : `:quit`/`:q`/`:exit` (447), `:reset` (451), `:show`/`:code` (455), **`:type`** (462‑473, il fabrique `>> type(arg)` en 468/470), `:latex` (474), `:load` (484), `:save` (505), `:help`/`:h`/`:?` (516), **commande inconnue** (517, message rouge via `couleur.erreur`), `continue` (519). |
| Exécution du code Rose | 522‑540 | Préfixe automatique `>> ` si la ligne n'est ni une définition ni déjà préfixée (524‑528), concaténation avec le buffer (529‑533), exécution (534), mémorisation des définitions réussies (537‑540). |
| Lecture bas niveau | 545‑583 | `enum Entree` (545‑554), édition de ligne en terminal, `io::stdin().read_line()` en pipe/fichier (583). |
| Aide compacte du binaire autonome | 934‑948 | Récapitulatif des commandes (945). |
| Tests | 985‑1080 | `completer(":he")` → `:help` (985), `est_definition(">> 3 + 4")` → faux (996), couleurs (1014‑1018), historique (1057‑1060). |

### 1.2 CLI — `rose/src/main.rs` (1468 lignes)

- `usage()` **225‑259** : `rose console [fichier.rose]` en **247**, `rose repl` en **248**.
- `main()` **791** ; `console_win::preparer()` **801** ; détection de `--console` **815‑816** et **1333‑1334**.
- `let action = argv[1]` **836** ; grand `match action` **874**.
- Branche REPL **1224‑1227** : `repl::lancer(fichier, true)` ; le `true` signifie « exécuter les `>>` du fichier chargé » (voir repl.rs 345, 362‑369).
- Note d'architecture **1355‑1357** : « une seule implémentation, deux points d'entrée ».
- Le préfixe `>>` est aussi produit par le lexeur pour les flèches (`Tk::FlecheD => ">>"`, **963**).

### 1.3 Rendu des valeurs — `rose/src/run.rs` (2457 lignes)

| Point d'entrée | Lignes | Rôle exact |
|---|---|---|
| `ecrire_out()` | 567‑578 | Sortie unique : tampon de capture `SINK` sinon `stdout`. |
| `q_chaine()` | 886‑891 | Fraction exacte `a/b` depuis les BigInt (pas de troncature i128). |
| `texte_rationnel()` | 895‑947 | Décimale périodique `1/3 → 0.[3]`, seuil 500 chiffres → `a/b`, 200 chiffres de période max, `…` si dépassement (942‑944). |
| `chaine_de()` | 949‑955 | Rendu brut, délègue à `chaine_de_p(v, false)`. |
| `chaine_de_p()` | 957‑1026 | **Le point de bascule du formatage** : `Entier` 962‑963, `Rationnel` 964, `Flottant` 965‑971, `Booleen` 972, `Chaine` 973 (`aplatir()`), `Liste` 974‑983 (`[a, b]`), `Table` 984‑996 (`[k => v]`), `Tampon` 997, `Complexe` 998‑1009, `Tableau` 1010‑1017, `Irrationnel` 1018‑1024. |
| `afficher_formule()` | 1028‑1055 | Commentaire 1029‑1038 : `formule` est **toujours vide** depuis le 16/11 ; la sortie d'exécution est la valeur seule. |
| `afficher()` | 1057‑1071 | Point unique d'affichage interpréteur **et** JIT ; rationnels et listes en forme périodique (1063‑1064), tampon refusé (1065), `ecrire_out("\n")` final (1070). |
| `factorielle()` | 1166‑1173 | Bornée à `n ≤ 0xFFFF_FFFF`, arbre de produit parallèle. |
| `est_premier()` | 1203‑1205 | `n.i128() > 1 && n.est_premier()`. |
| `nb_chiffres()` | 1207‑1210 | `sizeinbase10()`, 1 si zéro — **outil déjà disponible pour `detail()`**. |
| `view_grand_nombre()` | 1933‑2048 | Viewer paginé existant (primitive `view`, voir 1.4) : bandeau 1954‑1956, petit nombre ≤ 2000 chiffres 1959‑1967, page de 70×20 = 1400 caractères 1970‑1972, index O(1) 1979, écran effacé 1988, en-tête avec position 1990, contrôles 2010, interprétation des touches 2020‑2042. |

### 1.4 Table des primitives — `rose/src/interp.rs`

- `nom_std_connu()` **1071‑1084** : liste blanche ; on y trouve `afficher`, `factorielle`, `est_premier`, `nb_chiffres`/`digits` (**1079**), `fenetre_native`, `fenetre_afficher`, `fenetre_attendre`, `fenetre_fermer`, `message_native` (**1083‑1084**).
- `stdlib_valeur()` : `afficher` en **4852**, **`view` en 4853** (`view_grand_nombre(&chaine_de(&a[0]))`), primitives GUI native à partir de **4855**.
- Dispatch numérique : `est_premier`/`is_prime` **5069**, `nb_chiffres`/`digits` **5071**.
- `type_de`/`type` **5722** : rend le `TyKind` sous forme de chaîne — base d'un libellé `type` dans `detail()`.

**Conséquence factuelle** : la refonte « Wolfram-like » n'a **pas** besoin de créer le rendu de zéro. Elle doit (a) ajouter une **décision par type** avant `chaine_de_p`/`afficher` (run.rs 959/1058) et (b) réutiliser `nb_chiffres`, `est_premier`, `texte_rationnel` et `view_grand_nombre` déjà présents.

### 1.5 GUI native — `rose/src/gui_native.rs` (684 lignes)

En-tête **1‑21** : module autonome, zéro crate, deux backends — Win32 `user32.dll` (`extern "system"` + `#[link]`) et X11 `libX11.so.6` chargé par `dlopen`/`dlsym`. Sur UNIX sans `DISPLAY`, erreur explicite `PasDeDisplay` (13‑16) ; le module **ne prétend jamais avoir ouvert une fenêtre**.

Types et garde-fous : `Etat`/`Backend`/`Fenetre` **34‑64**, `Erreur` **66‑91**, bornes `LARGEUR_MIN/MAX = 1/8192` et `TITRE_MAX = 1024` **97‑99**, registre `Mutex<HashMap<u64, Fenetre>>` **101‑104**, `borner_dimension()` **109‑111**, `nettoyer_texte()` **115‑117**, `utf16_avec_nul()` **120‑123**, `SYMBOLES_X11` **126‑143**, `diagnostic()` **147‑156**.

API publique : `fenetre_creer()` **173‑201**, `fenetre_afficher()` **204‑231**, `fenetre_attendre()` **240‑298**, `fenetre_fermer()` **301‑325**, `message()` **330‑383**, `compter_fenetres()` **385‑388**, `creer_simulee()` **393‑406**.

**Appels OS précis déjà disponibles :**

- X11 (déclarés en **417‑434**, résolus par `dlsym` en **457‑483**, liste publique **126‑143**) :
  `XOpenDisplay`, `XCloseDisplay`, `XDefaultRootWindow`, `XCreateSimpleWindow`, `XMapWindow`, `XDestroyWindow`, `XFlush`, `XSync`, `XStoreName`, `XNextEvent`, `XPending`, `XLoadQueryFont`, `XCreateGC`, `XDrawString`, `XFreeGC`, `XFreeFont`.
  Création effective en **488‑503** ; boucle d'événements `XPending` + `XNextEvent` en **265‑277**, arrêt sur types 2 (Key), 4 (Button), 33 (ClientMessage) en **269** ; boîte modale dessinée à `XDrawString` en **354‑365**.
- Win32 (`#[link(name = "user32")]` **539‑557**, `kernel32` **559‑563**) :
  `RegisterClassExW` **542**, `CreateWindowExW` **543‑547**, `ShowWindow` **548**, `DestroyWindow` **549**, `MessageBoxW` **550**, `GetMessageW` **551**, `TranslateMessage` **552**, `DispatchMessageW` **553**, `DefWindowProcW` **554**, `PostQuitMessage` **555**, `LoadCursorW` **556**, `GetModuleHandleW` **562**.
  `rose_wnd_proc()` **566‑572** ne traite **que** `WM_DESTROY` (`0x0002`) ; `win32_creer()` **575‑607** enregistre la classe `RoseFenetreN` et crée une fenêtre `WS_OVERLAPPEDWINDOW`.

⚠ **Limite à ne pas franchir en writing** : il n'existe **aucun** contrôle enfant natif (`CreateWindowExW` pour un bouton/une zone de texte, pas de `WM_COMMAND`, pas de `SetWindowText`). Les « widgets » d'aujourd'hui sont des **données Rose**, pas des contrôles Windows/X11. Toute amélioration interactive réelle est une **proposition** de la §4, pas un état courant.

### 1.6 Jardin — `jardins/gui_native.rose` (231 lignes)

- En-tête **1‑23** : « **SOURCE DE VÉRITÉ DE L'INTERFACE** » ; le moteur ne fournit que **5 primitives** (`fenetre_native`, `fenetre_afficher`, `fenetre_attendre`, `fenetre_fermer`, `message_native`), « aucun widget, aucune mise en page, aucun événement n'est hors de ce jardin : tout est Rose » (3‑7). Pas de navigateur, pas de HTML, pas d'interpréteur externe (13‑16).
- `importer "jardins/os_natif.rose"` **25** ; chemin Rose pur `generer_*` **18‑23**.
- Modèle **27‑47** : fenêtre = table Rose (`titre`, `largeur`, `hauteur`, `widgets`, `actions`, `nwidgets`, `nactions`) ; chaque widget est une table de chaînes (contrainte du vérificateur, 30‑31) ; `_cle(n)` **33‑35**, `_widget(type,label,valeur,x,y,largeur)` **37‑47**.
- Constructeurs : `fenetre` **51‑61**, **`bouton` 63‑65**, **`champ` 67‑69**, `etiquette` 71‑73, **`case_a_cocher` 75‑79**, **`liste_deroulante(choix, selection)` 81‑86**.
- Placement/événements : `ajouter` **90‑97**, `lier_evenement` **99‑110**, `evenement` **112‑122**, `compter_widgets`/`compter_actions` **124‑130**, `grille` **134‑145**, `empiler` **147‑159**, `cadrer` **161‑171**.
- Testabilité sans écran : `decrire(f)` **175‑189** produit `FENETRE|…` puis une ligne `W|<type>|<label>|<x>|<y>` par widget.
- Pont OS minimal : `ouvrir` **193‑199**, `attendre` **201‑204**, `fermer` **206‑208**, `dire` **210‑212**.
- Chemins Rose purs : `generer_fenetre` **216‑221**, `generer_message` **223‑226**, `generer_ecran` **228‑231** — ils écrivent un ELF avec `ecrire_bytes`.
- `jardins/os_natif.rose` : table de correspondance **13‑18**, `ecrire_ecran` **49**, `ouvrir_fenetre` **58**, `boite_message` **124**, noms exportés **132‑136**.

Vérification rejouée le 21/11/2026 : `./rose/rose verif jardins/gui_native.rose` → `vérifié`.

### 1.7 Ce que `COMPARATIF_TOTAL.md` apporte (et n'apporte pas)

`rose/docs/COMPARATIF_TOTAL.md` est un **comparatif de performances** (bancs 9 domaines **37‑61**, global **63‑83**, suite complexe **85‑90**), pas une spécification d'affichage. Il n'existe aucune occurrence de `detail`, `wolfram`, `plus_de_chiffres` ou `:more` dans ce fichier, ni dans `rose/docs/`, `rose/src/`, `jardins/` ou `examples/` (recherche faite le 21/11). Son apport ici est **méthodologique** : ne pas confondre une valeur mesurée à une date avec une règle, et vérifier la sortie réelle avant d'écrire qu'un chantier est fini (lignes 9‑14).

---

## 2. Proposition de syntaxe Rose et de format de sortie

### 2.1 Les quatre surfaces proposées

| Surface | Signature proposée | Comportement | Ancrage existant |
|---|---|---|---|
| `detail(expr)` | `detail(expr) -> table` | Affiche les propriétés **pertinentes pour le type**, puis rend un **contexte de détail** (table Rose) réutilisable. | `afficher()` run.rs 1058 ; `type_de` interp.rs 5722 |
| `chiffres(expr, n)` | `chiffres(expr, n: entier) -> chaîne` | Rend exactement les `n` premiers chiffres décimaux, sans signe, sans séparateur, sans `…`. Si le nombre a moins de `n` chiffres, rend tous les chiffres (pas de padding). `n = 0` → chaîne vide ; `n < 0` → erreur explicite. | `nb_chiffres()` run.rs 1207 ; `chaine_de_p()` run.rs 959 |
| `plus_de_chiffres(expr)` | `plus_de_chiffres(expr, n: entier = 30) -> chaîne` | Réitérable : chaque appel ajoute `n` chiffres au compteur de **cette même expression**, rend la chaîne cumulée. Le compteur vit dans le **contexte** rendu par `detail(expr)` quand on passe la table : `plus_de_chiffres($d, 30)`. En REPL, la forme courte `plus_de_chiffres(expr)` utilise le dernier contexte mémorisé. | `view_grand_nombre()` run.rs 1945 (défilement existant) |
| `:more` | commande REPL, **30 chiffres** par défaut, `:more 100` accepte un nombre | Réjoue l'affichage des chiffres du dernier `detail(expr)` avec le compteur incrémenté. Sans `detail` préalable : `rose: :more nécessite d'abord detail(expr)` en rouge. | dispatch repl.rs 446‑518, message d'erreur 517 |

Points d'entrée à modifier (design, pas encore faits) : `nom_std_connu()` interp.rs 1070‑1084 (ajouter les 3 noms) ; `COMPLETIONS` repl.rs 194‑205 (ajouter `:more`, `detail`, `chiffres`, `plus_de_chiffres`) ; `aide()` repl.rs 253‑262 (faire passer la table de **8 à 9** entrées) ; `match cmd` repl.rs 446‑518 (nouvelle branche `:more`) ; variable d'état du dernier contexte à côté de `buffer`/`code_sortie` repl.rs 348/384 ; réinitialisation par `:reset` repl.rs 451‑454.

### 2.2 Format de sortie exact — règle commune

`detail()` n'utilise **pas** le flux LaTeX (`formule` toujours vide, run.rs 1029‑1038). Il écrit via `ecrire_out()` (run.rs 567‑578), une ligne à la fois, avec le gabarit suivant :

```
<ligne vide>
<titre : l'expression telle que saisie, sans le préfixe >> >
  <libellé><espaces de remplissage> : <valeur>
  … une ligne par propriété pertinente …
```

Règles précises :
- **2 espaces** d'indentation, jamais de tabulation.
- Libellé **aligné à gauche sur 28 caractères**, puis ` : ` (espace, deux-points, espace), puis la valeur. Gabarit Rust équivalent : `format!("  {:<28} : {}", libelle, valeur)`.
- Un seul titre, une seule ligne par propriété, **aucune propriété non pertinente pour le type** (c'est la demande).
- Aucune couleur ANSI dans `detail()` lui-même : le formatage reste capturable (SINK, run.rs 569‑576) et comparable octet pour octet. Les couleurs du REPL (repl.rs 31‑66) ne colorent que l'invite et les messages de commande.
- Le titre reprend l'expression source (ex. `factorielle(10^38)`), ce qui rend `detail()` réitérable sans ambiguïté.

### 2.3 Exemple canonique — factorielle géante

Sortie spécifiée pour `>> detail(factorielle(10^38))`. Les valeurs sont **celles de la demande** ; le premier chiffres est représenté par un espace réservé car la demande ne le chiffre pas.

```
>> detail(factorielle(10^38))

factorielle(10^38)
  type                         : entier positif
  representation               : 10^(10^38.56…)
  premiers chiffres            : <30 premiers chiffres>…
  longueur                     : 3.65657×10^38 chiffres décimaux
  zeros finaux                 : 2499…990
  derniers chiffres non nuls   : …6970467172352
```

Propriétés retenues, et pourquoi (pertinence par type) : `representation` (la puissance de 10 est le seul résumé lisible), `premiers chiffres` (queue de la demande), `longueur` (`nb_chiffres`, run.rs 1207, ici sous forme scientifique), `zeros finaux` et `derniers chiffres non nuls` (propriétés d'une factorielle, calculables par la formule de Legendre — voir §5 pour la valeur exacte de `1000000!`). Aucune ligne `est premier`, `période`, `éléments` ou `contenu` : elles ne concernent pas ce type.

Le bouton « Plus de chiffres » remplace la ligne `premiers chiffres` par les 30 chiffres suivants cumulés (60, 90, …) ; les autres lignes ne bougent pas.

### 2.4 Quatre autres types — exemples de sortie

**(a) Entier premier** — `>> detail(1000003)` (primalité vérifiée le 21/11) :

```
>> detail(1000003)

1000003
  type                         : entier premier
  representation               : 1000003
  longueur                     : 7 chiffres décimaux
```

Pas de `zeros finaux`, pas de `derniers chiffres non nuls` : un nombre premier ne se termine pas par zéro et n'a pas de structure de factorielle. La ligne `representation` reste car l'entier tient sur une ligne.

**(b) Rationnel** — `>> detail(7/12)` (rendu `0.58[3]` par `texte_rationnel`, run.rs 895‑947) :

```
>> detail(7/12)

7/12
  type                         : rationnel
  representation               : 0.58[3]
  numerateur                   : 7
  denominateur                 : 12
  periode                      : 1 chiffre
```

Ni `longueur de nombre`, ni `zeros finaux` : ce sont des propriétés d'entier, pas de fraction. Au-delà du seuil de 500 chiffres (run.rs 906), `representation` passe en `a/b` et la ligne `periode` disparaît.

**(c) Liste** — `>> detail([1, 2, 3])` (rendu `[1, 2, 3]` par `chaine_de_p`, run.rs 974‑983) :

```
>> detail([1, 2, 3])

[1, 2, 3]
  type                         : liste
  representation               : [1, 2, 3]
  elements                     : 3
  premier element              : 1
  dernier element              : 3
```

Aucune ligne numérique (longueur décimale, zéros, période) : la longueur ici est un **nombre d'éléments**.

**(d) Chaîne** — `>> detail("bonjour")` (`aplatir()`, run.rs 973) :

```
>> detail("bonjour")

bonjour
  type                         : chaine
  representation               : bonjour
  longueur                     : 7 caracteres
```

Aucune propriété arithmétique. Pour une chaîne > 80 caractères, `representation` est tronquée à 77 caractères + `…` (règle proposée, à vérifier par test).

---

## 3. Widget GUI natif — spécification

### 3.1 Les trois widgets

Ils sont construits par les fonctions **existantes** de `jardins/gui_native.rose` (§1.6) :

| Widget | Appel Rose | Ligne du constructeur | Rôle dans l'écran « détail » |
|---|---|---|---|
| Zone de texte | `champ("chiffres", chaine(premiers))` | 67‑69 | Contient la chaîne courante des N premiers chiffres ; mise à jour par `plus_de_chiffres`. |
| Bouton « Plus de chiffres » | `bouton("Plus de chiffres", 160)` | 63‑65 | Déclenche l'ajout de 30 chiffres via un événement lié. |
| Sélecteur radio du nombre de chiffres | groupe de `case_a_cocher(vrai/faux, "10")`, `"30"`, `"100"`, `"1000"` | 75‑79 | Choisit le pas d'incrément. `case_a_cocher` est une **case** : l'exclusion mutuelle (comportement radio) est une **logique Rose à écrire dans le jardin**, pas un widget natif. |

Compléments proposés dans le jardin (Rose pur, aucun HTML, aucun CPython) :
- `choisir_pas(f, n)` : parcourt `f["widgets"]` avec `compter_widgets()` (124‑126), force la valeur `"0"` sur les cases `case` sauf celle du pas choisi, puis `inserer` — même boucle `tant_que` que `evenement()` (112‑122).
- `pas_selectionne(f)` : lit les valeurs, rend l'entier correspondant (conversion `entier(chaine(...))`, comme en 93/166).
- `lier_evenement(f, "plus", "clic", "chiffres", "plus_de_chiffres")` via la fonction existante **99‑110** ; les événements sont retrouvés par `evenement(f, "plus")` **112‑122**.
- Mise en page : `empiler(f, 28)` **147‑159** (bouton sous la zone de texte) puis `cadrer(f, 12)` **161‑171**. `grille()` **134‑145** reste disponible pour le groupe radio.
- Vérification sans écran : `decrire(f)` **175‑189** doit contenir `W|bouton|Plus de chiffres`, `W|champ|...` et une ligne `W|case|...` par pas.

### 3.2 Comment le jardin porte la logique SANS HTML et SANS CPython

Chaîne de responsabilité, déjà en place et vérifiable dans les fichiers :

1. **Le moteur ne sait rien du langage** (gui_native.rs 18‑20) : il ne manipule que des `u64` (poignées) et des `&str`, et expose 5 primitives (jardins/gui_native.rose 3‑7 ; `nom_std_connu` interp.rs 1083‑1084 ; `stdlib_valeur` interp.rs 4855+).
2. **Toute l'interface est en Rose** : tables de widgets (27‑47), constructeurs (51‑86), événements (99‑122), mise en page (134‑171), description déterministe (175‑189). Le jardin est vérifié par `./rose/rose verif jardins/gui_native.rose` → `vérifié` (re-joué le 21/11).
3. **Aucun HTML, aucun interpréteur externe** : l'en-tête du jardin 13‑16 l'affirme ; `generer_fenetre`/`generer_message`/`generer_ecran` (216‑231) produisent un **ELF** via `jardins/os_natif.rose` (`ouvrir_fenetre` 58, `boite_message` 124, `ecrire_ecran` 49) et l'écrivent avec `ecrire_bytes` — pas de navigateur, pas de Python, pas de Node.
4. **Repli explicite** : sans `DISPLAY` ou sans `libX11.so.6`, `Erreur::PasDeDisplay`/`SymboleManquant` (gui_native.rs 68‑76, 488‑495) est renvoyée ; le jardin peut alors choisir `decrire()` ou le viewer texte `view_grand_nombre` (run.rs 1945) — jamais une fenêtre simulée sauf le helper de test `creer_simulee()` (gui_native.rs 393‑406).

### 3.3 Extension native nécessaire pour des contrôles réels (proposition, non implémentée)

Aujourd'hui `rose_wnd_proc()` (gui_native.rs 566‑572) ne traite que `WM_DESTROY`, et `win32_creer()` (575‑607) ne crée que la fenêtre parente. Pour que les widgets deviennent des **contrôles OS** cliquables :

- étendre le `WndProc` avec `WM_CREATE` (création des enfants via `CreateWindowExW` : classe `BUTTON`, `EDIT`, `STATIC`) et `WM_COMMAND` (notification `BN_CLICKED` → incrément) ; déclarer `SendMessageW`/`SetWindowTextW`/`GetWindowTextW` à côté des externes **539‑557** ;
- côté X11, dessiner les trois zones avec `XDrawString` (déjà résolu, **432/480**) et router les `ButtonPress` (déjà détectés en **269**) vers un hit-test Rose ;
- conserver le contrat « le jardin possède l'interface, le moteur ne fournit que l'appel OS » (jardins/gui_native.rose 9‑11) : le Rust ne doit contenir **aucun** libellé, aucun pas de chiffres, aucune règle `detail()`.

---

## 4. Critères d'acceptation testables

Tous les chiffres ci-dessous ont été recalculés le 21/11/2026 (Python `math.factorial`, formule de Legendre pour les zéros finaux) ; les commandes sont à rejouer sur le binaire issu de l'implémentation, pas à considérer comme déjà passées.

### Affichage court et réitérable

1. `>> factorielle(100)` affiche **exactement 30 chiffres puis `…`**, sur une seule ligne :
   `933262154439441526816992388562…`
   (100! a 158 chiffres ; les 30 premiers sont vérifiés.)
2. `:more` juste après ajoute **exactement 30 chiffres** (cumul 60) :
   `933262154439441526816992388562667004907159682643816214685929…`
3. Un second `:more` donne 90 chiffres cumulés ; `:more 10` n'ajoute que 10 chiffres. À partir d'un nombre de ≤ 30 chiffres, aucun `…` n'apparaît.
4. `:more` sans `detail`/`>>` préalable affiche `rose: :more nécessite d'abord detail(expr)` et ne modifie pas le buffer.
5. `:reset` (repl.rs 451‑454) remet le compteur de chiffres à zéro.
6. `:help` (repl.rs 516) liste `:more` ; la complétion Tab (repl.rs 208‑211) propose `:mo` → `:more`.

### `detail()` dynamique

7. `>> detail(factorielle(1000000))` affiche exactement :
   - `representation` : `10^(5565708.917…)` (vérifié : `lgamma(1000001)/ln(10)` = 5 565 708,9171867… ; longueur = 5 565 709 chiffres) ;
   - `premiers chiffres` : `826393168833124006237664610317…` ;
   - `longueur` : `5 565 709 chiffres décimaux` ;
   - `zeros finaux` : **`249998`** (critère demandé) ;
   - `derniers chiffres non nuls` : `…2561765058412544` (16 derniers chiffres non nuls) ;
   - aucune ligne `est premier`, `période`, `éléments` ou `caracteres`.
8. `>> detail(1000003)` contient `type : entier premier` et `longueur : 7 chiffres décimaux`, et **ne contient pas** `zeros finaux`.
9. `>> detail(7/12)` contient `representation : 0.58[3]`, `numerateur : 7`, `denominateur : 12`, `periode : 1 chiffre`, et **ne contient pas** `longueur … chiffres décimaux`.
10. `>> detail([1, 2, 3])` contient `type : liste`, `elements : 3` et **ne contient aucune** propriété arithmétique.
11. `>> detail("bonjour")` contient `type : chaine`, `longueur : 7 caracteres` et **ne contient aucune** propriété numérique.
12. Le gabarit est respecté au caractère près : 2 espaces d'indentation, libellé paddé à 28, séparateur ` : `. Un test compare la sortie capturée via `SINK` (run.rs 560‑565) à une chaîne littérale.

### GUI et jardin

13. `./rose/rose verif jardins/gui_native.rose` → `vérifié` (déjà vrai à `2961f3d`).
14. Un programme test qui construit la fenêtre puis appelle `decrire(f)` contient les lignes
    `W|bouton|Plus de chiffres`, `W|champ|chiffres` et au moins quatre `W|case|…`.
15. Le groupe radio est exclusif : après `choisir_pas(f, 100)`, une seule case a la valeur `"1"` dans la table `f["widgets"]`.
16. Un clic simulé sur le bouton (`lier_evenement` + `evenement(f, "plus")`) incrémente le compteur de 30 et met à jour la valeur du `champ`.
17. Aucun fichier du jardin ne contient `html`, `python`, `CPython`, `import os` ou de balise `<` (vérifiable par `grep`) ; la sortie reste produite par les 5 primitives et `ecrire_bytes`.
18. Sans `DISPLAY` (`DISPLAY=`), `fenetre_creer()` renvoie `Erreur::PasDeDisplay` et le jardin replie sur `decrire()`/`view` sans paniquer — comportement déjà spécifié en gui_native.rs 13‑16 et 488‑495.

### Non-régression

19. `>> 1/3` continue de rendre `0.[3]` (run.rs 895‑947) et `>> 2.5` rend `2.5` : `detail()` ne modifie pas `chaine_de_p`/`afficher`.
20. Le viewer existant `>> view(factorielle(1000))` garde ses contrôles `[Espace/Entrée] suivant [b] précédent [q] quitter [flèches] défiler` (run.rs 2010) : la refonte l'**ajoute**, elle ne le remplace pas.

---

## 5. État des lieux final

- **Fait** : les six fichiers demandés ont été lus ; tous les points d'entrée ci-dessus sont cités avec leurs lignes dans l'arbre à `2961f3d`.
- **Vérifié** : `100!` = 158 chiffres, préfixe `933262154439441526816992388562`, 24 zéros finaux ; `1000000!` = 5 565 709 chiffres, préfixe `826393168833124006237664610317`, **249 998 zéros finaux**, 16 derniers chiffres non nuls `2561765058412544` ; `1000003` est premier ; `./rose/rose verif jardins/gui_native.rose` → `vérifié`.
- **Proposé, non implémenté** : `detail`, `chiffres`, `plus_de_chiffres`, la commande `:more`, les lignes de propriétés, l'extension `WndProc` pour de vrais contrôles natifs. Aucune de ces modifications n'a été écrite dans le dépôt.
- **Reste** : décider si `plus_de_chiffres` garde un état implicite en REPL (forme courte) ou exige toujours le contexte table explicite — le design ci-dessus propose **les deux**, avec priorité à la table explicite dans les programmes et au dernier contexte en REPL. C'est le seul point de conception encore ouvert.
