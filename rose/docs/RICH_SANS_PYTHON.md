# Rendu texte « inspiré de Rich » — en Rose pur, sans Python

**Ce n'est pas Rich.** C'est un **jardin Rose** (`jardins/texte.rose`) qui
reprend trois idées de présentation de
[Rich](https://github.com/Textualize/rich) : le panneau bordé, la ligne de
tableau alignée et la règle titrée. Rich a été **lu comme modèle de format**
(box `┌─┬┐` / `│ ││` / `└─┴┘`, padding `(0, 1)`, titre centré de `Rule`) ;
**aucun code Python n'est dans la chaîne d'exécution** : ni CPython, ni
RustPython, ni venv, ni wasmtime, ni module Python importé. Le produit est du
Rose interprété/compilé par `./rose/rose`, avec les chaînes Unicode et les
entiers exacts du langage.

Date : 18/11/2026 — Rose 0.1.5.

## 1. Ce qui est porté

| Fonction | Signature | Rôle |
|---|---|---|
| `panneau` | `panneau(t: chaine, largeur: entier) -> chaine` | cadre à 3 lignes `┌─┐ / │ … │ / └─┘`, texte centré, une case de marge de chaque côté (padding de Rich), tronqué s'il déborde ; largeur totale ramenée à 4 si elle est plus petite |
| `tableau_simple` | `tableau_simple(colonnes: liste<chaine>, largeurs: liste<entier>) -> chaine` | **une** ligne d'en-tête, cellules alignées à gauche et bourrées à leur largeur, séparées par `│`, avec `│` aux deux extrémités |
| `barre` | `barre(t: chaine, largeur: entier, motif: chaine) -> chaine` | règle titrée d'une ligne (type `rich.rule.Rule`) : motif répété, titre entouré d'espaces au centre ; règle sans titre si le titre ne tient pas |

Deux utilitaires internes sont aussi exportés (Rose n'a pas de visibilité
privée) : `motif_repetit(motif, n)` (répète puis coupe à `n` caractères) et
`ajuster(texte, n)` (aligne à gauche sur exactement `n` cases).

### Résultats réels (comparaison exacte, 9/9)

```
=== 1. panneau("Bonjour", 20) ===
┌──────────────────┐
│     Bonjour      │
└──────────────────┘
✓ panneau Bonjour/20
=== 4. tableau_simple ===
│ Nom      │ Age  │
✓ tableau_simple Nom/Age
=== 6. barre titrée ===
────── Titre ───────
✓ barre Titre/20
=== 7. barre motif multi-caractères ===
-=- T -=-=
✓ barre motif -=
=== rendu composé (barre + panneau + tableau + barre) ===
════════ FICHE ═════════
┌──────────────────────┐
│   Rose sans Python   │
└──────────────────────┘
│ Prenom     │ Ville    │
═════════ FIN ══════════
RESULTAT : 9/9 comparaisons exactes réussies.
```

`panneau("Bonjour", 20)` fait exactement **20 caractères par ligne** :
`1 + 1 + 4 + 7 + 5 + 1 + 1`, soit `│`, une marge, 4 espaces de centrage,
`Bonjour`, 5 espaces, une marge, `│`. `sous_chaine` compte les **caractères**
Unicode (pas les octets), donc `─` vaut une case.

## 2. Ce qui n'est PAS porté (documentation honnête)

- **couleurs et styles ANSI** : pas de `Style`, pas de 256 couleurs, pas de
  truecolor, pas de gras/italique/souligné, pas de détection de terminal,
  pas de `Console` / `Segment`. Les chaînes produites sont du texte brut.
- **Markdown** : aucun analyseur markdown, aucun rendu de titre `#`, liste,
  citation, code fence, lien ou tableau markdown (`rich.markdown.Markdown`).
- **tables complètes** : pas de lignes de données, pas de séparateur
  `├─┼─┤`, pas d'alignement à droite/au centre par colonne, pas de
  `Table.grid`, pas de tableaux imbriqués, pas de fusion de cellules.
  `tableau_simple` ne produit **qu'une ligne d'en-tête**.
- **panneaux avancés** : pas de titre/subtitre dans la bordure, pas de
  style de bordure, pas de texte multi-lignes ni de retour à la ligne
  (wrapping), pas de `Group`, `Columns`, `Tree`, `Progress` ni `Live`.
- **largeur d'affichage** : pas de prise en charge des caractères East Asian
  Width (CJK/emoji « larges ») : chaque caractère Unicode compte pour 1.
- ** Rich lui-même** : aucune API, aucun objet, aucun module de Rich n'est
  appelé. L'installation locale de Rich n'a servi qu'à lire les formats.

En résumé : trois primitives de mise en page, pas une bibliothèque de
rendu terminal.

## 3. Utilisation

Depuis la racine du dépôt :

```rose
importer "jardins/texte.rose"

>> panneau("Bonjour", 20)
>> tableau_simple(["Nom", "Age"], [8, 4])
>> barre("Titre", 20, "─")
```

```sh
./rose/rose ton_programme.rose
```

`importer` résout le chemin tel quel puis à côté du fichier importateur ; les
imports sont récursifs et chaque fichier n'est lu qu'une fois (voir
`jardins/README.md`, §3).

### Pièges de syntaxe Rose rencontrés et respectés

- Rose n'a **pas d'arguments par défaut** : `tableau_simple` exige ses deux
  arguments. Pour une liste d'entiers vide, écrire `[0 : entier]` (le `0`
  donne le type, ce n'est pas un élément) — jamais `[]`.
- `sous_chaine(s, debut, fin)` a une borne **fin exclusive** et compte les
  caractères ; `div` fait la division entière (`/` donne un rationnel).
- Les listes sont homogènes : `liste<chaine>` et `liste<entier>` sont deux
  paramètres distincts, jamais une liste mixte.
- `>>` est une instruction de programme : il ne peut pas y en avoir dans un
  jardin (`scripts/rose-jardin.sh verif` le vérifie).

## 4. Validation exécutée (commandes et sorties réelles)

```sh
$ ./rose/rose verif jardins/texte.rose
vérifié

$ bash scripts/rose-jardin.sh verif texte
── verification de /home/clausake/Stefan/ROSE/jardins/texte.rose ──
  ✓ fichier présent (105 lignes)
  ✓ aucun marqueur Rust/FFI (pas de Rust, pas de C, pas de wasm)
  ✓ bibliothèque pure (aucune instruction '>>')
  ✓ rose verif : vérifié
  → 5 fonction(s) exportée(s)
✓ JARDIN VALIDE : /home/clausake/Stefan/ROSE/jardins/texte.rose (Rose pur, vérifié)

$ ./rose/rose /tmp/opencode/test_texte.rose
... 9 lignes ✓ ...
RESULTAT : 9/9 comparaisons exactes réussies.
```

Le programme de test est `/tmp/opencode/test_texte.rose` (hors dépôt). Il
importe le jardin, affiche chaque rendu et compare chaque sortie à une
chaîne littérale attendue avec `==`. Seul `./rose/rose` a été utilisé pour
produire et vérifier ces sorties.

## 5. Fichiers

- `jardins/texte.rose` — le jardin Rose pur (105 lignes, 5 fonctions).
- `rose/docs/RICH_SANS_PYTHON.md` — ce document.
- `jardins/README.md` — catalogue des jardins (ligne `texte.rose` ajoutée).
