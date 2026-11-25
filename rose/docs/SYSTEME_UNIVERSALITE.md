# SYSTEME_UNIVERSALITE — un seul `.bud`, une seule fonction, tout est branché

**État : actif et vérifié le 25/11/2026.** Worktree exclusif `/tmp/opencode/v6brique`
(HEAD `7085f79`), sans commit ni push.

Avant ce chantier, le câblage était **éclaté** : `compilateur/lecteur_bud.rose`
lisait le manifeste, `briques/appareil/chargeur.rose` déclarait les appareils,
`briques/appareil/contrat.rose` exécutait les opérations — mais il fallait
appeler ces trois couches à la main, choisir une fiche et choisir une
opération. Il existe maintenant **une seule fonction Rose**,
`systeme_depuis_bud(chemin)`, qui enchaîne lecture → validation →
déclaration → exécution → résultat. Le pilote
`briques/tests_systeme.rose` l'appelle et affiche le résultat.

> **Le changement d'un appareil ne touche jamais au code Rose.** Il se fait
> uniquement dans le `.bud` (`briques/appareil/contrat.bud`).

## 1. Ce qui a été ajouté

| Fichier | Rôle |
|---|---|
| `briques/appareil/systeme.rose` | la fonction unique `systeme_depuis_bud(chemin)` |
| `briques/appareil/contrat.bud` | bloc `essai:` (données seulement) : appareil, opération, arguments |
| `briques/tests_systeme.rose` | pilote « un clic » : charge, exécute « calculer », affiche |
| `rose/docs/SYSTEME_UNIVERSALITE.md` | ce document |

Aucun fichier de `compilateur/` ni de `rose/src/` n'a été modifié.

## 2. La commande unique

```rose
>> systeme_depuis_bud("briques/appareil/contrat.bud")
```

Cet appel lit le manifeste, valide ses paires, déclare les opérations et les
appareils qu'il contient, exécute l'opération demandée et rend le résultat.
Le `>>` l'affiche. C'est la seule instruction à écrire.

### Signature

```
systeme_depuis_bud(chemin: chaine) -> liste<entier>
```

Résultat : `[rc, capacite, empreinte, nb_appareils, id_appareil]`

| Champ | Sens |
|---|---|
| `rc` | `0` = succès ; `-1` = manifeste invalide ; `-2` = déclaration des opérations impossible ; `-3` = aucun appareil ; `-1/-2/-3` = refus propagés par `appareil_executer` (opération inconnue / capacité absente / fiche invalide) |
| `capacite` | code de la capacité examinée (`1` = calculer, `2` = stocker, `3` = communiquer, `4` = mesurer, `5` = quantique) |
| `empreinte` | accusé de réception déterministe de l'opération et des arguments |
| `nb_appareils` | nombre d'appareils réellement enregistrés depuis le `.bud` |
| `id_appareil` | identifiant de l'appareil utilisé |

La liste est homogène (cinq entiers) : aucun type mixte, donc le même résultat
passe par l'interpréteur, le JIT et le compilateur self-hosté C0.

## 3. Les six étapes, en Rose pur

1. **Lecture** : `bud_lire(chemin)`, puis `bud_parse` (aplatissement en paires
   `cle=valeur`).
2. **Validation** : `bud_valider(paires)` (forme des paires, clés vides,
   doublons). Un échec rend `[−1, 0, 0, 0, 0]`.
3. **Déclaration** : `appareil_operations_depuis_bud(chemin)` puis
   `appareil_depuis_bud(chemin)`. Les opérations déjà connues ne sont pas
   dupliquées ; les appareils sans capacité sont ignorés (Rose n'a pas de
   littéral `[]`).
4. **Choix de l'appareil** : `essai.appareil` du `.bud`, sinon le premier
   appareil déclaré (`appareil_enumere()[1]`).
5. **Choix de l'opération** : `essai.operation` du `.bud`, sinon la première
   capacité de l'appareil. Si le nom choisi est une **capacité** (par exemple
   « calculer »), la fonction l'inscrit dans la table d'opérations avec le code
   fourni par `appareil_code_capacite` — aucun nom n'est codé en dur. Les
   arguments viennent de `essai.arguments.<j>`, sinon `[2, 3]`.
6. **Exécution** : `appareil_executer(app, operation, args)` — refus explicite
   (`rc < 0`) si la capacité requise n'est pas déclarée, jamais de repli
   silencieux. Le résultat est renvoyé, avec les compteurs ajoutés.

Rose n'ayant pas de modules, les fichiers se **concatènent** dans cet ordre :
`lecteur_bud.rose` → `contrat.rose` → `chargeur.rose` → `systeme.rose` →
`tests_systeme.rose`. L'ordre garantit que les globales de la brique sont
initialisées avant le pilote (les `importer` ne le garantissent pas).

```bash
cd <racine du dépôt>
cat compilateur/lecteur_bud.rose \
    briques/appareil/contrat.rose \
    briques/appareil/chargeur.rose \
    briques/appareil/systeme.rose \
    briques/tests_systeme.rose > /tmp/tests_systeme_full.rose
ROSE=/home/clausake/Stefan/ROSE/rose/rose
$ROSE verif /tmp/tests_systeme_full.rose     # vérifié
$ROSE /tmp/tests_systeme_full.rose           # interpréteur
/tmp/opencode/C0_h.elf /tmp/tests_systeme_full.rose > /tmp/tests_systeme_full.elf
chmod +x /tmp/tests_systeme_full.elf
/tmp/tests_systeme_full.elf                  # ELF self-hosté
```

## 4. Configurer un appareil sans écrire de Rose

Tout se passe dans `briques/appareil/contrat.bud`. Le bloc ajouté :

```yaml
essai: {
  appareil: "calculateur"
  operation: "calculer"
  arguments: [2, 3]
}
```

Ajouter un appareil, ou changer l'appareil / l'opération testée, se fait
**uniquement** dans ce fichier :

```yaml
appareils: [
  { nom: "calculateur", capacites: ["calculer", "stocker"] }
  { nom: "capteur", capacites: ["mesurer", "communiquer"] }
  { nom: "noeud_quantique", capacites: ["quantique", "calculer", "communiquer"] }
  { nom: "mon_appareil", capacites: ["stocker"] }        # ← ajout
]
essai: {
  appareil: "mon_appareil"                              # ← changement
  operation: "ecrire"                                   # ← changement
  arguments: [7, 8, 9]                                  # ← changement
}
```

Le même code Rose produit alors : `rc=0`, `capacite=2 (stocker)`,
`appareils=4`, `appareil=4 (mon_appareil)`. Aucune ligne de
`systeme.rose`, `contrat.rose` ou `chargeur.rose` n'a été touchée.

Capacités abstraites reconnues : `calculer` (1), `stocker` (2),
`communiquer` (3), `mesurer` (4), `quantique` (5). Aucun nom d'architecture,
de système hôte, de format d'image ou de matériel concret n'apparaît dans la
brique. Le jour où un adaptateur réel sera branché, ce sera **hors** de cette
brique.

## 5. Preuves (commandes réellement exécutées)

### 5.1 Concaténation, vérification et exécution

```
$ wc -l /tmp/tests_systeme_full.rose
880 /tmp/tests_systeme_full.rose
$ ROSE verif /tmp/tests_systeme_full.rose
vérifié
$ ROSE /tmp/tests_systeme_full.rose
rc=0
capacite=1 (calculer)
empreinte=415543085
appareils=3
appareil=1 (calculateur)
TESTS SYSTEME VERTS : calculer sur calculateur, 3 appareils depuis contrat.bud
```

### 5.2 C0 → ELF, puis parité interpréteur = ELF

```
$ /tmp/opencode/C0_h.elf /tmp/tests_systeme_full.rose > /tmp/tests_systeme_full.elf
$ ls -l /tmp/tests_systeme_full.elf
-rwxrwxr-x 1 clausake clausake 77152 Nov 25 22:58 /tmp/tests_systeme_full.elf
$ /tmp/tests_systeme_full.elf          # mêmes six lignes, mêmes octets
$ diff /tmp/opencode/sys_work/final_check/interp.out \
       /tmp/opencode/sys_work/final_check/elf.out
SORTIES INTERPRETE = ELF (octet pour octet)
```

rc = 0 des deux côtés ; stderr vide des deux côtés.

### 5.3 Invariants du moteur (rien n'a été modifié)

```
$ bash scripts/verifier_parite_selfhost.sh --moteur /tmp/opencode/final.elf \
    --workdir /tmp/opencode/sys_work/parite_final
moteur self-hosté : /tmp/opencode/final.elf (md5 bab13df633e53d15b1ee181029a717c2)
oracle Rust       : /home/clausake/Stefan/ROSE/rose/rose (md5 7614a534b9411917ef9cd49a834014a8)
tests=141  CONFORMES=139  DIVERGENTS=0  BLOQUES=2
ERREURS self-host sur programme accepté par Rust = 0
VERDICT : RC=0
```

Les 2 BLOQUES sont les refus historiques des deux moteurs
(`t55_liste_imbriquee`, `t107_hof_var`), pas des programmes acceptés par Rust.

```
$ ROSE rir compilateur/emetteur.rose | md5sum
b3ed9262dd54776a6dbd07bc12368d0e
$ ROSE compilateur/emetteur.rose compilateur/emetteur.rose | md5sum
b3ed9262dd54776a6dbd07bc12368d0e
```

Point fixe octet pour octet intact.

⚠ **Constat daté** : une recompilation *fraîche* du moteur avec
`/tmp/opencode/C0_h.elf` (C0 du 23/11) ne reproduit PAS la parité de HEAD :
28 divergences numériques. Le moteur de référence mesuré ici est
`/tmp/opencode/final.elf` (md5 `bab13df…`, 25/11 22:25), celui qui porte la
parité 141/141 annoncée par HEAD. Ce constat ne concerne pas le chantier
(présent) — aucun fichier du moteur n'a été modifié — mais il interdit de
présenter une recompilation C0_h comme preuve de la parité.

## 6. Limites

- Un appareil doit déclarer au moins une capacité (Rose n'a pas de littéral de
  liste vide) ; sinon le chargeur l'ignore et `systeme_depuis_bud` rend `-3`
  s'il ne reste aucun appareil.
- `bud_valider` vérifie la forme des paires, pas la sémantique des capacités :
  un nom inconnu donne le code `0` et l'exécution est refusée.
- Les champs `essai.*` sont **optionnels** : sans eux, la brique choisit le
  premier appareil, sa première capacité et `[2, 3]`.
- `empreinte` n'est pas le résultat physique de l'opération : c'est la preuve
  que le dispatch par capacité a eu lieu. L'effet réel appartient à un
  adaptateur hors de cette brique.
- Les pilotes utilisent des chemins relatifs à la racine du dépôt.
- Le compilateur self-hosté peut compiler ce programme (`C0 → ELF`) et la
  sortie est identique à l'interpréteur, comme mesuré en §5.2.
