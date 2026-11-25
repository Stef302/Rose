# Appareil universel — Rose est le langage, l'appareil est une donnée

> Date : 25/11/2026. Worktree : `/tmp/opencode/v4device`, HEAD `6d056bc`.
> Fichiers : `briques/appareil/contrat.rose`, `briques/appareil/contrat.bud`,
> `briques/tests_appareil.rose`. Aucun commit, aucun push.

## 0. La thèse

Rose doit pouvoir **décrire** n'importe quel appareil électronique — microcontrôleur,
processeur inhabituel, plus tard un dispositif quantique — sans qu'aucune architecture,
aucun système hôte, aucun format d'image et aucun mécanisme d'accès au matériel ne soit
écrit dans la logique du langage.

La séparation est totale :

| Côté | Nature | Contenu |
|---|---|---|
| `contrat.rose` | **code** (Rose pur) | un mécanisme : déclarer, valider, énumérer, dispatcher par capacité |
| `contrat.bud` | **données** | les appareils, leurs capacités, la table des opérations, la cible `GENERIQUE` |
| adaptateur (hors brique) | **code spécifique** | le jour où un appareil réel est branché |

**Une brique décrit un mécanisme, jamais un appareil.** Le passage du mécanisme à
l'appareil se fait par des données, jamais par une fonction spécialisée du compilateur
(`briques/README.md`, `rose/docs/BRIQUES_UNIVERSELLES.md`).

Aucun nom de famille de processeur, de système, de format ou de bus n'apparaît dans
`contrat.rose` ni dans `tests_appareil.rose`. Les seuls noms matériels possibles sont
dans un futur adaptateur, hors de cette brique.

## 1. Représentation : tout est liste d'entiers

Un appareil est une **valeur** persistantente, pas un objet mutable :

```
[id, empreinte_nom, nb_capacites, reussites, refus, code_cap_1, ...]
```

- `id` : identifiant abstrait (1, 2, 3…), jamais une adresse ni un descripteur ;
- `empreinte_nom` : hachage borné du nom (module `1000000007`) ;
- `nb_capacites` : nombre de codes suivants ;
- `reussites`, `refus` : conservés dans les champs de la fiche de l'annuaire ;
- codes de capacité : `1 = calculer`, `2 = stocker`, `3 = communiquer`,
  `4 = mesurer`, `5 = quantique`.

Trois collections globales, **modifiées seulement par ajout** (jamais de
réassignation d'une case, pour rester compilable par le compilateur auto-hébergé) :

- `$appareils_annuaire` : fiches plates concaténées, précédées d'une fiche vide `id = 0` ;
- `$appareils_noms` : noms parallèles indexés par `id` ;
- `$appareils_evenements` : journal de paires `[id, 0=succès | 1=refus]`.

La table des opérations est elle aussi en données : `$appareils_operations`
(noms) et `$appareils_requis` (capacité exigée par nom). Ajouter une opération =
ajouter deux valeurs, **sans toucher à la logique**.

## 2. API

```rose
fn appareil_declarer(nom: chaine, capacites: liste<chaine>) -> liste<entier>
fn appareil_executer(app: liste<entier>, operation: chaine,
                     args: liste<entier>) -> liste<entier>
fn appareil_enumere() -> liste<entier>
fn appareil_valider(app: liste<entier>) -> entier
```

Fonctions d'accompagnement (observabilité et extension) :

```rose
fn appareil_obtenir(id: entier) -> liste<entier>
fn appareil_nom(app: liste<entier>) -> chaine
fn appareil_code_capacite(s: chaine) -> entier
fn appareil_nom_capacite(c: entier) -> chaine
fn appareil_capacite_requise(operation: chaine) -> entier
fn appareil_operation_declarer(nom: chaine, capacite: entier) -> entier
fn appareil_statistique(app: liste<entier>, rang: entier) -> entier
```

### `appareil_executer` — résultat `[rc, capacite, empreinte]`

| `rc` | Sens |
|---|---|
| `0` | succès : la capacité requise est déclarée sur l'appareil ; `empreinte` est un accusé de réception déterministe de l'opération et des arguments |
| `-1` | opération inconnue de la table d'opérations |
| `-2` | **capacité requise absente** : refus explicite, aucun repli silencieux |
| `-3` | appareil invalide (voir `appareil_valider`) |

Le champ `capacite` du résultat indique la capacité examinée. Le champ `empreinte`
n'est **pas** le résultat physique d'une opération : c'est une signature bornée qui
prouve que le dispatch par capacité a eu lieu. L'effet réel appartient à l'adaptateur.

### `appareil_valider` — cohérence

| valeur | Sens |
|---|---|
| `0` | cohérent |
| `-1` | structure incohérente (longueur ≠ `5 + nb_capacites`) |
| `-2` | code de capacité inconnu |
| `-3` | capacité déclarée deux fois |
| `-4` | identifiant ou nom incohérent avec l'annuaire |

### `appareil_enumere`

Retourne les identifiants des appareils enregistrés. La case `0` est une sentinelle
(`0` = aucun appareil) : avec trois appareils, la liste est `[0, 1, 2, 3]`.

## 3. Déclarer un nouvel appareil — y compris un appareil quantique

Dans le `.bud`, uniquement des données :

```
appareils: [
  { nom: "noeud_quantique", capacites: ["quantique", "calculer", "communiquer"] }
]
```

Dans le programme, les mêmes données passées à la brique :

```rose
$noeud = appareil_declarer("noeud_quantique",
                           ["quantique", "calculer", "communiquer"])
>> appareil_valider($noeud)                    # 0
$r = appareil_executer($noeud, "porte", [1, 2])
>> $r[0]                                       # 0  (capacite declaree)
>> $r[1]                                       # 5  (quantique)
$refus = appareil_executer($calc, "porte", [1])
>> $refus[0]                                   # -2 (capacite absente)
```

Un appareil qui ne déclare pas `"quantique"` refuse toute opération quantique avec
`rc = -2` : il n'existe **aucun** chemin de repli vers une autre capacité. Pour
ajouter une opération quantique supplémentaire, on ajoute une ligne de données :

```rose
>> appareil_operation_declarer("mesure_quantique", 5)
```

Aucune ligne de `contrat.rose` n'a été modifiée : c'est le critère de réversibilité
de la doctrine (remplacer le `.bud`, pas le code).

## 4. Validation réellement exécutée

Chaîne rejouée le 25/11/2026 :

```bash
cd /tmp/opencode/v4device
ROSE=/home/clausake/Stefan/ROSE/rose/rose        # Rose 0.1.73
C0=/tmp/opencode/C0_h.elf                        # md5 2db168b907b4a874712857d3957fd078

cat briques/appareil/contrat.rose briques/tests_appareil.rose > /tmp/a.rose
$ROSE verif briques/appareil/contrat.rose        # vérifié (rc=0)
$ROSE verif /tmp/a.rose                          # vérifié (rc=0)
$ROSE /tmp/a.rose > /tmp/a.rose.out
$C0 /tmp/a.rose > /tmp/a.exe                     # ELF64 aarch64
/tmp/a.exe > /tmp/a.exe.out
cmp /tmp/a.rose.out /tmp/a.exe.out               # rc=0 — 51 lignes identiques
```

Résultats observés (51 lignes) : trois appareils déclarés et validés (`0, 0, 0`),
énumération `[0, 1, 2, 3]`, trois opérations autorisées (`rc = 0`, capacités `1`, `4`, `5`),
quatre refus explicites (`rc = -2` sur capacité absente, `rc = -1` sur opération
inconnue), deux appareils incohérents détectés (`-3` doublon, `-2` capacité
inconnue) et refusés à l'exécution (`-3`), statistiques cohérentes (1 réussite et
1 refus par appareil), puis extension par données et nouvelle réussite.

Deux contraintes du compilateur auto-hébergé ont été respectées dans la brique :
les empreintes sont **bornées** (module `1000000007`) pour rester dans le domaine
arithmétique où les deux exécuteurs concordent, et les collections globales ne sont
jamais réassignées depuis une fonction (seul l'ajout en fin de liste est utilisé).

## 5. Limites explicites — ce que Rose ne fait PAS encore

1. **Câblage du `.bud` absent.** Le compilateur ne lit pas encore les fichiers
   `.bud` : `contrat.bud` est une déclaration lisible par l'homme, pas encore une
   entrée de compilation. Aujourd'hui les trois appareils sont recopiés en données
   dans le pilote de test ; le câblage `.bud` → annuaire reste à implémenter.
2. **Aucun backend réel pour l'appareil.** `appareil_executer` valide la capacité et
   produit une empreinte déterministe ; il ne pilote aucun composant. Brancher un
   vrai appareil exige un adaptateur écrit hors de la brique, qui traduise les
   capacités abstraites en mécanismes concrets.
3. **Aucune sémantique d'effet, aucun calcul quantique.** `"quantique"` est un nom
   de capacité, pas une porte exécutée : ni mesure, ni état, ni qubit ne sont
   simulés. De même, `"calculer"` ne promet aucune opération arithmétique réelle.
4. **Pas de persistance ni d'isolation.** L'annuaire, le journal et la table
   d'opérations vivent dans le processus ; ils ne sont ni sérialisés ni protégés
   entre appareils.
5. **Profil noyau non appliqué.** Les entiers Rose restent de précision arbitraire ;
   le sous-ensemble à largeur fixe et sans allocation implicite décrit dans
   `BRIQUES_UNIVERSELLES.md` (B5) n'est pas encore vérifié par le compilateur.
6. **Aucun budget de coût mesuré.** `latence_max` du `.bud` est une déclaration, pas
   une garantie vérifiée par un test.

Ces limites sont des **absences déclarées**, pas des promesses : la brique fournit
le contrat d'abstraction, pas l'appareil.
