# BUD_CABLE — les manifestes `.bud` sont maintenant LUS

**État : actif (vérifié).** Avant ce chantier, les 7 fichiers `briques/**/*.bud`
étaient des spécifications documentées : aucun code du compilateur ne les
ouvrait (`grep '.bud' compilateur/*.rose rose/src/*.rs` ne trouvait aucune
lecture). Ils sont désormais branchés sur un lecteur Rose pur, intégré au
moteur auto-hébergé et exposé par une commande CLI.

## Ce qui est actif

| Élément | Rôle |
|---|---|
| `compilateur/lecteur_bud.rose` | lecteur autonome en Rose pur (api `bud_*`) |
| `compilateur/moteur_complet.rose` | module intégré + commande `rose bud FICHIER.bud` |
| `scripts/build_moteur_sans_rust.sh` | le module est ajouté à la composition (regen) |
| `briques/tests_bud.rose` | pilote : lit `exemple.bud` et vérifie 5 champs |
| `moteur_complet.elf bud FICHIER.bud` | affiche tous les champs aplatis du manifeste |

Les champs sont maintenant des DONNÉES accessibles au compilateur :
`bud_champ`, `bud_entier` et `bud_booleen` les lisent depuis le fichier, pas
depuis une constante. Les 7 manifestes du dépôt sont lus et validés (sections
« Preuves » ci-dessous).

## API Rose

```
bud_lire(chemin)             -> chaine         lit le fichier (lire_texte)
bud_tokens(txt)              -> liste<chaine>  jetons (élément 0 = sentinelle "")
bud_parse(txt)               -> liste<chaine>  paires "cle=valeur" aplaties
bud_champ(paires, cle)       -> chaine         valeur, ou "" si absente
bud_entier(paires, cle)      -> entier         valeur décimale (signe '-' accepté), 0 si absente
bud_booleen(paires, cle)     -> entier         1 pour vrai/oui/1, 0 sinon
bud_valider(paires)          -> entier         0 = valide ; -1/-2/-3 + message sur stderr
```

`bud_parse` rend une liste plate dont l'élément 0 est une sentinelle `""`
(Rose refuse le littéral `[]`) : les paires vont de l'indice 1 à
`longueur(paires) - 1`.

## Grammaire supportée

Sous-ensemble réellement utilisé par les `.bud` de `briques/` :

```
cle: valeur                paire simple
cle: { c1: v1, c2: v2 }    objet -> clés aplaties `cle.c1`, `cle.c2`
cle: [ v1, v2, ... ]       liste -> clés aplaties `cle.0`, `cle.1`, ...
cle: [ { ... }, { ... } ]  liste d'objets -> `cle.0.champ`, `cle.1.champ`, ...
"texte"                    chaîne (guillemets retirés, espaces conservés)
# commentaire              jusqu'à la fin de la ligne, ignoré
{ } [ ] , :                ponctuation (la virgule est ignorée)
```

Exemple d'aplatissement (`briques/exemple.bud`) :

```
nom                            -> nom=noyau-generique
cible: { architecture: GENERIQUE }  -> cible.architecture=GENERIQUE
materiel: { regions: [ { base: 0, taille: 1048576, role: "ram" } ] }
                               -> materiel.regions.0.base=0
                                  materiel.regions.0.taille=1048576
                                  materiel.regions.0.role=ram
options: { freestanding: vrai } -> options.freestanding=vrai
```

`exemple.bud` produit 34 paires ; `profil/noyau.bud` 64 ; `image/contour.bud`
50 ; `abi/appel.bud` 50 ; `interruptions/vecteurs.bud` 40 ;
`memoire/arena.bud` 41 ; `pilote/contrat.bud` 29. Les 7 passent
`bud_valider` avec `rc=0`.

## Validation

`bud_valider` contrôle chaque paire : présence d'un `=`, clé non vide,
absence de clé dupliquée (le chemin aplati est comparé aux autres paires).
Une VALEUR vide est légale : `ecriveur: ""` dans `abi/appel.bud` est un
champ réel. En cas d'erreur, un message est écrit sur stderr et le rc est
négatif (-1 paire sans `=`, -2 clé vide, -3 doublon).

## Limites (délibérées)

- **Aucune évaluation** : les valeurs restent des chaînes ; `bud_entier` et
  `bud_booleen` font la conversion à la demande. Pas d'expressions, pas
  d'arithmétique, pas de références entre champs.
- **Aucune vérification de typage sémantique** : `largeur_mot: GENERIQUE`
  serait accepté comme chaîne ; c'est l'appelant qui décide quoi en faire.
- **Pas d'import entre `.bud`** : chaque fichier est autonome.
- **Commentaires** : seulement `#` jusqu'à la fin de ligne (pas de
  commentaire sur plusieurs lignes). Un `#` dans une chaîne est protégé.
- **Échappements** : seulement `\"` (et `\\`) à l'intérieur des chaînes.
- **Accolades** : un objet mal fermé est toléré par l'aplatisseur (les
  paires lues avant la fin sont rendues) ; `bud_valider` ne vérifie pas
  l'équilibrage des jetons, seulement la forme des paires.
- **Portée** : le lecteur rend les champs disponibles et la CLI les affiche ;
  les backends d'émission (ELF/PE) ne consomment pas encore ces valeurs pour
  générer du code. Le branchement « donné → décision de compilation » reste
  à faire par les modules qui utilisent ces champs.

## Commandes de vérification

```bash
cd <racine du dépôt>
ROSE=/home/clausake/Stefan/ROSE/rose/rose
$ROSE verif compilateur/lecteur_bud.rose          # vérifié
$ROSE verif briques/tests_bud.rose                # vérifié
$ROSE briques/tests_bud.rose                      # TESTS BUD VERTS : 34 champs...
$ROSE rir compilateur/emetteur.rose | md5sum      # b3ed9262dd54776a6dbd07bc12368d0e
/tmp/opencode/C0_h.elf compilateur/moteur_complet.rose > moteur.elf
moteur.elf bud briques/exemple.bud               # nom=..., cible.architecture=..., ...
```

La parité self-hostée reste mesurée par
`scripts/verifier_parite_selfhost.sh --moteur <elf>` : 141/141.
