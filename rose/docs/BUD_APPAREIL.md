# BUD_APPAREIL — déclarer un appareil dans un `.bud`, sans écrire de code Rose

**État : actif.** Les manifestes `.bud` ne sont plus seulement lis par le
compilateur (voir `BUD_CABLE.md`) : ils **alimentent maintenant la brique
appareil**. Le nom des appareils, leurs capacités et les opérations abstraites
viennent du fichier ; `briques/appareil/contrat.rose` et
`briques/appareil/chargeur.rose` ne changent pas.

## Ce qui est branché

| Élément | Rôle |
|---|---|
| `compilateur/lecteur_bud.rose` | lit le `.bud` et aplatit les champs en paires `cle=valeur` |
| `briques/appareil/contrat.rose` | annuaire, capacités, opérations, validation, exécution |
| `briques/appareil/chargeur.rose` | `appareil_depuis_bud` / `appareil_operations_depuis_bud` |
| `compilateur/moteur_complet.rose` | commande `appareil` du moteur auto-hébergé |
| `briques/tests_chargeur.rose` | banc : 3 appareils lus, exécution et refus |

## API Rose ajoutée

```
appareil_depuis_bud(chemin)             -> entier   nb d'appareils enregistrés (< 0 : invalide)
appareil_operations_depuis_bud(chemin)  -> entier   nb d'opérations ajoutées (< 0 : invalide)
appareil_operation_connue(nom)          -> entier   1 si déjà dans la table (contrat.rose)
appareil_operation_pour(app)            -> chaine   opération dont la capacité est déclarée
```

`appareil_depuis_bud` lit `appareils.<i>.nom` et
`appareils.<i>.capacites.<j>`, puis appelle `appareil_declarer(nom, caps)`.
`appareil_operations_depuis_bud` lit `operations.<i>.nom` / `.capacite` et
n'ajoute que les opérations absentes de la table (celles déjà déclarées par
`contrat.rose` ne sont pas dupliquées).

## Déclarer un nouvel appareil

Éditer **uniquement** le `.bud` (ici `briques/appareil/contrat.bud`) :

```yaml
appareils: [
  { nom: "calculateur", capacites: ["calculer", "stocker"] }
  { nom: "capteur", capacites: ["mesurer", "communiquer"] }
  { nom: "noeud_quantique", capacites: ["quantique", "calculer", "communiquer"] }
  { nom: "mon_appareil", capacites: ["mesurer", "stocker"] }   # ← ajout
]
```

Capacités abstraites reconnues : `calculer` (1), `stocker` (2),
`communiquer` (3), `mesurer` (4), `quantique` (5). Aucun nom d'architecture,
de système d'exploitation, de format de fichier ou de matériel concret
n'apparaît dans la brique.

Ajouter une opération se fait de la même manière, toujours dans le `.bud` :

```yaml
operations: [
  { nom: "transmettre", capacite: 3 }   # ← ajout : aucun code Rose à toucher
]
```

La commande `appareil` choisit alors automatiquement, pour chaque appareil,
la première opération de la table dont la capacité requise est déclarée sur
cet appareil — y compris une opération nouvellement déclarée.

## Commande

Depuis la racine du dépôt (le moteur auto-hébergé `moteur.elf` est obtenu par
`C0 compilateur/moteur_complet.rose`) :

```bash
moteur.elf appareil briques/appareil/contrat.bud
# appareils=3
# 1|calculateur|addition|cap=calculer|rc=0
# 2|capteur|emettre|cap=communiquer|rc=0
# 3|noeud_quantique|addition|cap=calculer|rc=0
```

La même chose avec le fichier relatif au dossier de la brique :

```bash
cd briques/appareil && moteur.elf appareil contrat.bud
```

`rc=0` signifie que la capacité requise par l'opération est bien déclarée sur
l'appareil. Une opération absente de la table donne `rc=-1` ; une opération
présente mais dont la capacité manque sur l'appareil donne `rc=-2` — jamais
de repli silencieux.

## Tester sans modifier le code

`briques/tests_chargeur.rose` vérifie : 3 appareils chargés depuis le fichier,
les 3 noms, la cohérence des fiches, une exécution réussie sur `calculateur`,
le refus (`rc=-2`) de `ecrire` sur `noeud_quantique` (il n'a pas `stocker`) et
le refus (`rc=-1`) d'une opération inconnue. Rose n'ayant pas de modules, ce
pilote se concatène avec ses dépendances (les `importer` fusionnent les
instructions **après** le fichier courant, donc les globals de la brique ne
seraient pas encore initialisés) :

```bash
cat compilateur/lecteur_bud.rose \
    briques/appareil/contrat.rose \
    briques/appareil/chargeur.rose \
    briques/tests_chargeur.rose > /tmp/tests_chargeur_full.rose
rose verif /tmp/tests_chargeur_full.rose
rose /tmp/tests_chargeur_full.rose
# TESTS CHARGEUR VERTS : 3 appareils lus dans briques/appareil/contrat.bud
moteur.elf /tmp/tests_chargeur_full.rose   # même sortie (parité interprète/ELF)
```

## Limites

- Un appareil doit déclarer **au moins une capacité** : Rose n'a pas de
  littéral de liste vide ; un appareil sans capacité est ignoré par le
  chargeur.
- `bud_valider` contrôle la forme des paires (doublons de clés), pas la
  sémantique des capacités : un nom de capacité inconnu donne le code `0` et
  `appareil_valider` refuse la fiche (`-2`).
- Les opérations déjà présentes dans la table de `contrat.rose` ne sont pas
  dupliquées ; leurs capacités restent celles de la brique.
- `compilateur/moteur_complet.rose` est un assemblage **généré puis patché**
  (commande `bud`, puis `appareil`). Une régénération par
  `scripts/build_moteur_sans_rust.sh --source-only` écraserait ces patchs :
  ne pas la lancer pour une validation.
- Le pilote de test utilise des chemins relatifs à la racine du dépôt.
- **Limite G5 préexistante, mesurée ici** : dans un programme utilisateur exécuté
  par l'ELF self-hosté, une globale modifiée depuis une fonction par `joindre`
  n'est pas vue par le niveau principal (`$x=[""] ; fn f(){joindre($x,"a")} ;
  f() ; >> longueur($x)` rend `1` sous ELF et `2` à l'interpréteur ; voir
  `rose/docs/G5_C1.md`, « Affecter une globale depuis une fonction crée une
  locale »). Le banc `tests_chargeur.rose` est donc vert à l'interpréteur ;
  sous ELF, l'annuaire rempli par `appareil_declarer` n'est pas visible du
  corps du test. La preuve interpréteur = ELF du branchement `.bud` → appareil
  se fait sur la commande intégrée au moteur (où le code est au niveau
  principal) : `appareils=3` et les trois lignes sont octet pour octet
  identiques des deux côtés.
