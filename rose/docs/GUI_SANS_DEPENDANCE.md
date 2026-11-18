# GUI en Rose pur, sans dépendance : le jardin `gui`

> **Constat daté au 19/11/2026**, Rose 0.1.5, binaire `rose/rose` de Head
> `477736f`. Ce document décrit ce qui est réellement livré et vérifié, pas
> une intention. Aucun `.rs`, `Cargo.toml` ou document du cœur n'a été modifié.

## 1. La demande et la réponse courte

Demande : « crée un jardin en rose qui permet de faire des fenêtres GUI comme
tkinter et je veux sans dépendre de rust ou autre, comme python ».

Ce qui est livré : `jardins/gui.rose`, un jardin de **Rose pur** (23 `fn`, zéro
`>>`, zéro marqueur Rust/FFI/C/wasm) qui fournit un modèle de widgets —
fenêtre, bouton, champ, étiquette, case à cocher, liste déroulante, placement,
événements, mise en page — et un fonction `rendre(fenetre)` qui produit un
**document HTML autonome** (CSS et JavaScript en ligne).

**Limite honnête, à lire avant tout le reste** : ce jardin **n'ouvre aucune
fenêtre du système d'exploitation**. Rose n'a aucune primitive `system`,
`exec` ou `ouvrir_navigateur`, et le jardin n'en demande aucune. Le HTML est
une vraie GUI interactive **dans un navigateur** : le navigateur est l'hôte du
rendu, exactement comme un serveur web est l'hôte d'une page. Le code Rose,
lui, s'exécute sans navigateur, sans Rust, sans Python, sans Node et sans
réseau : il ne fait que calculer une chaîne.

## 2. Les trois voies de rendu possibles — choix retenu

| Voie | Ce que produit `rendre` | Hôte nécessaire | Retenu ? |
|---|---|---|---|
| **(a) Sortie portable HTML** | un document complet `<!doctype html>…`, CSS + JS en ligne, déterministe | un navigateur **pour afficher** ; aucun pour produire | **OUI — implémenté** |
| (b) Sortie TUI | des séquences ANSI/Unicode dessinant une « fenêtre » dans un terminal | un terminal | Non implémenté |
| (c) Pont natif optionnel | des appels vers un hôte OS via les imports wasm du moteur | un hôte natif + artefact binaire | Non implémenté, et contraire à la contrainte « zéro dépendance » |

Le choix (a) est le seul qui donne une GUI réellement interactive sans
artefact binaire et sans code natif. Le choix (c) resterait **optionnel** et
serait à documenter comme tel s'il était ajouté : il ne doit jamais devenir
une dépendance obligatoire du jardin.

Ce que `rendre` ne fait **pas** :

- il n'appelle pas le navigateur, ne lance pas de processus, n'ouvre pas de
  socket, n'écrit pas de fichier ;
- il n'exécute pas d'action d'événement : les actions liées sont rendues comme
  des **données HTML déclaratives** (`data-genre`, `data-cible`, texte), et le
  petit JavaScript intégré ne fait que refléter les clics/saisies dans une
  zone de statut. Il n'y a ni `eval`, ni appel réseau ;
- il n'y a pas de boucle d'événements côté Rose : `evenement(f, nom)` renvoie
  une **table d'actions**, pas un callback (Rose n'a pas de fermetures
  stockables dans une table).

## 3. API exacte exportée

Signatures telles qu'extraites par `./rose/rose ast jardins/gui.rose` et par
`bash scripts/rose-jardin.sh liste`. Les fonctions préfixées `_` sont des
aides internes ; les autres forment l'API publique.

### Construction

| Fonction | Type | Rôle |
|---|---|---|
| `fenetre(titre: chaine, largeur: entier, hauteur: entier) -> table` | public | fenêtre vide (tables `widgets`, `evenements`, `actions`) |
| `bouton(label: chaine, largeur: entier) -> table` | public | widget bouton |
| `champ(label: chaine, valeur: chaine) -> table` | public | champ de texte pré-rempli |
| `etiquette(texte: chaine) -> table` | public | libellé |
| `case(a_cocher: booleen, texte: chaine) -> table` | public | case à cocher |
| `liste(choix: liste<chaine>, selection: entier) -> table` | public | liste déroulante, `selection` = indice 0-based |
| `ajouter(fenetre: table, widget: table, x: entier, y: entier) -> table` | public | place le widget, lui donne un `id`, renvoie le widget |

### Événements

| Fonction | Type | Rôle |
|---|---|---|
| `lier_evenement(fenetre: table, nom: chaine, genre: chaine, cible: chaine, action: chaine)` | public | enregistre une action sous `nom` |
| `evenement(fenetre: table, nom: chaine) -> table` | public | renvoie la table d'actions du nom (`clic`, `saisie`, `fermeture`, …) |
| `compter_actions(fenetre: table) -> entier` | public | nombre d'actions liées |
| `compter_widgets(fenetre: table) -> entier` | public | nombre de widgets |

### Mise en page et rendu

| Fonction | Type | Rôle |
|---|---|---|
| `grille(fenetre: table, colonnes: entier, largeur_case: entier, hauteur_case: entier)` | public | `x=(i%c)*lc`, `y=(i div c)*hc` |
| `empiler(fenetre: table, ecart: entier)` | public | `x=0`, `y=i*ecart` |
| `cadrer(fenetre: table, marge: entier)` | public | marge intérieure en pixels |
| `rendre(fenetre: table) -> chaine` | public | document HTML complet, déterministe |

### Internes (préfixe `_`)

`_widget`, `_choix_chaine`, `_indice_barre`, `echapper`, `html_widget`,
`html_actions`, `feuille_de_style`, `script`. `echapper` est publique de fait
(elle peut être appelée par un programme) mais documentée comme utilitaire.

Total : **23 fonctions** — 15 publiques (API ci-dessus), 5 utilitaires de
rendu (`echapper`, `html_widget`, `html_actions`, `feuille_de_style`,
`script`, tous appelables depuis un programme) et 3 privées préfixées `_`.

## 4. Modèle de données — pourquoi tout est stocké en chaînes

En Rose, une valeur lue dans une table par `$t["cle"]` a le type statique
dynamique `?` (`TyKind::Dyn`, voir `rose/src/front.rs::expr_type`). Elle ne
peut donc pas être passée telle quelle à `longueur`, `sous_chaine`,
`concatener` ou à une fonction dont le paramètre est typé. Le jardin respecte
cette règle :

- tous les champs (y compris les nombres) sont stockés sous forme de chaînes ;
- à la lecture : `chaine($t["cle"])`, puis si besoin `entier(chaine(...))` ;
- à l'écriture : `chaine(entier)` ;
- les widgets sont rangés sous les clés `"0"`, `"1"`, … dans la table
  `widgets` (pas une `liste<table>` : `[0 : table]` ne se comporte pas comme
  `[0 : entier]` au vérificateur) ;
- les choix d'une liste déroulante sont sérialisés en `"a|b|c"` avec un
  compteur `nchoix`, pour éviter de relire une liste depuis une table.

C'est une contrainte du langage, pas un choix esthétique. Elle est vérifiée
par `rose verif` et par l'exécution de la démo.

`evenement(f, nom)` renvoie une table avec `nom`, `n`, puis `"0"`, `"1"`, …
(chacune : `genre`, `cible`, `action`). Une table vide a `n = 0`.

## 5. Démo réelle et sortie vérifiée

Fichier : `jardins/tests/gui_demo.rose`. Il importe `jardins/gui.rose` et
`jardins/hachage.rose`, construit une fenêtre « Bonjour Rose » (360 × 180,
marge 12) avec une étiquette, un champ pré-rempli `Rose` et un bouton `OK`,
lie trois événements (`clic`, `saisie`, `fermeture`), puis affiche le HTML et
une reçue.

Commande réelle (sortie abrégée ici, HTML complet sur 40 lignes) :

```sh
./rose/rose jardins/tests/gui_demo.rose
```

Extraits réels de la sortie :

```html
<!doctype html>
<html lang="fr">
<head>
<meta charset="utf-8">
<title>Bonjour Rose</title>
<style>
body{font-family:sans-serif;background:#eef2f7;margin:24px;color:#1b2340}
...
</style>
</head>
<body>
<div class="rose-fenetre">
<div class="rose-barre">Bonjour Rose</div>
<span class="rose-etiquette" style="left:16px;top:20px">Saisissez votre nom :</span>
<label class="rose-champ" style="left:16px;top:48px">Nom <input type="text" data-id="1" value="Rose"></label>
<button class="rose-bouton" data-id="2" style="left:16px;top:86px;width:90px">OK</button>
</div>
<div class="rose-actions">
<span class="rose-action" data-genre="bouton" data-cible="b2">dire bonjour</span>
<span class="rose-action" data-genre="champ" data-cible="c1">memoriser le nom</span>
<span class="rose-action" data-genre="fenetre" data-cible="f0">quitter</span>
</div>
<div id="rose-statut"></div>
<script>
...
</script>
</body>
</html>
```

Dernière ligne, comparée **octet pour octet** à un littéral dans la source du
programme (avec `hachage_djb2_mod` du jardin `hachage`, modulus
1 000 000 007, pour garder une reçue courte) :

```
✓ GUI-DEMO | titre=Bonjour Rose | widgets=3 | actions=3 | clic=1 | octets=2139 | djb2mod=425427703
```

Le document produit est déterministe : ni horodatage, ni aléatoire, ni chemin
absolu. 2 139 caractères (Unicode compté en caractères par Rose).

## 6. Validations réellement exécutées (19/11/2026)

| Commande | Résultat observé |
|---|---|
| `./rose/rose verif jardins/gui.rose` | `vérifié` |
| `bash scripts/rose-jardin.sh verif gui` | `✓ JARDIN VALIDE : …/jardins/gui.rose (Rose pur, vérifié)` — 390 lignes, 23 fonctions, aucun marqueur Rust/FFI, aucune `>>` |
| `./rose/rose verif jardins/tests/gui_demo.rose` | `vérifié` |
| `./rose/rose jardins/tests/gui_demo.rose` | reçue `✓ … octets=2139 \| djb2mod=425427703` |
| `bash scripts/rose-jardin.sh verif-tous` | échec **hors de ce chantier** : `jardins/math.rose` (agent parallèle) contient `interp.rs` dans un commentaire en ligne 271, ce que le filtre `\.rs([^a-z]|$)` refuse ; les 6 autres jardins, dont `gui.rose`, sont `✓ JARDIN VALIDE` |

Une vérification supplémentaire, exécutée dans `/tmp/opencode/gui_api_test.rose`
(hors livrable), exerce toute l'API : 5 widgets dont une case cochée et une
liste à 3 choix, 4 actions dont 2 pour `clic`, `grille(2,100,40)` →
`w3 = (100,40)`, `empiler(25)` → `w2 = (0,50)`, `cadrer(9)` → `marge = 9`,
`rendre` → 2 458 caractères avec `checked`, `selected` et les 3 options
présentes. Un second test (`/tmp/opencode/gui_escape_test.rose`) confirme
l'échappement `& < > " '` en `&amp; &lt; &gt; &quot; &#39;`.

## 7. Ce qui reste / ce qui n'est pas promis

- pas de rendu TUI (voie b) et pas de pont natif (voie c) : documentés comme
  options, non implémentés ;
- pas de vraie boucle d'événements Rose : les actions sont des données, le JS
  intégré est générique ;
- pas de fenêtre OS, pas de focus, pas de gestionnaire de fenêtres : le HTML est
  le produit, le navigateur en est l'hôte ;
- `verif-tous` reste rouge tant que le commentaire `interp.rs` de
  `jardins/math.rose` n'est pas corrigé par son auteur — ce fichier n'appartient
  pas à cette mission et n'a pas été modifié ici.

## 8. Fichiers livrés par cette mission

| Fichier | Rôle |
|---|---|
| `jardins/gui.rose` | le jardin (23 `fn`, Rose pur) |
| `jardins/tests/gui_demo.rose` | démo + reçue littérale |
| `jardins/README.md` | une ligne ajoutée au catalogue §2 |
| `rose/docs/GUI_SANS_DEPENDANCE.md` | ce document |

Aucun commit, aucune publication, aucun autre fichier modifié.
