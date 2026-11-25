# BRIQUES B7/B8 — contrat de pilote générique + ABI utilisateur générique

**État livré :** `briques/pilote/contrat.rose`, `briques/pilote/contrat.bud`,
`briques/pilote/tests_contrat.rose`, `briques/abi/appel.rose`,
`briques/abi/appel.bud`, `briques/abi/tests_appel.rose`.
Worktree exclusif `/tmp/opencode/v2abi`, HEAD de départ `e4c8e59`.
**Aucun commit, aucun push, aucun fichier existant modifié.**

---

## 1. Doctrine

Une brique décrit un **mécanisme**, jamais un appareil. Il n'existe ici
**aucun** `clavier.rose`, `framebuffer.rose`, `disque.rose`, aucun `outb`,
aucune adresse MMIO, aucun numéro d'IRQ matériel, aucun syscall Linux ou
Win32, aucun « ring 3 » x86, aucun nom de CPU, de bus ou de contrôleur.

Le passage du mécanisme à un appareil se fait par des **données déclarées**
(`.bud`) et par un **adaptateur écrit ailleurs**, jamais en modifiant ces
deux fichiers. Deux profils de données distincts sont fournis pour prouver
la réversibilité : `contrat.bud` (`ops: [1, 8, 64, 0]`, polling/bloc) et
`ops_alternatif: [2, 1, 16, 7]` (irq/caractère) ; le code Rose est identique.

### Dépendances (ordre de la doctrine `BRIQUES_UNIVERSELLES.md` §2.1)

```
B8 abi/appel.rose  →  B7 pilote/contrat.rose  →  B9 image (à venir)
```

B8 ne dépend de rien d'autre que des primitives du langage Rose déjà émises
par C0 (`joindre`, `longueur`, `ordre`, `inserer` n'est pas utilisé ici).
B7 est **autonome** : il n'appelle aucune fonction de B8, mais il occupe
l'étage suivant et peut être chargé avec lui (les deux se concatènent sans
collision de noms : tous les identifiants sont préfixés `pilote_` / `abi_`,
`syscat_`, `isolate_`, `chargeur_`).

Les writers existants restent la source de vérité pour la sérialisation :

| Format | Writer existant | Rôle de B8 |
|---|---|---|
| ELF64 ET_EXEC aarch64 | `compilateur/emetteur_elf.rose` | reconnaissance d'en-tête + description de délégation |
| PE32+ | `compilateur/emetteur_pe_x64.rose` | idem |
| brut | aucun | validateur « tout » |

B8 ne réimplémente **pas** l'émetteur : `chargeur_ecriveur(fmt)` rend le nom
du writer à utiliser, `chargeur_charger` rend son code dans le descripteur.

---

## 2. B7 — `briques/pilote/contrat.rose`

### API

```rose
fn pilote_declarer(nom: chaine, famille: chaine, ops: liste<entier>) -> liste<entier>
fn pilote_ouvrir(p: liste<entier>, identifiant: entier) -> liste<entier>
fn pilote_lire(p: liste<entier>, n: entier) -> liste<entier>
fn pilote_ecrire(p: liste<entier>, donnees: liste<entier>) -> liste<entier>
fn pilote_fermer(p: liste<entier>) -> liste<entier>
fn pilote_poller(p: liste<entier>) -> liste<entier>
fn pilote_notifier(p: liste<entier>, evenement: entier) -> liste<entier>
```

- `famille` est une donnée : `"bloc"`, `"caractere"`, `"trame"`. Aucun
  appareil n'est nommé ; une famille inconnue donne le code `0`.
- `ops` vient du `.bud` : `[mode, largeur_bloc, profondeur_trame, vecteur]`
  avec `mode` = `1` polling / `2` irq, `vecteur` = identifiant abstrait
  d'événement (`0` = aucun), **jamais** un numéro d'IRQ machine.
- `identifiant` est un handle abstrait (entier), pas une adresse.
- `pilote_poller` ne consomme un octet qu'en mode polling ; `pilote_notifier`
  ne compte un événement qu'en mode irq. Aucun des deux ne touche au
  matériel : l'adaptateur fournit les octets et les événements.

Observabilité (fonctions pures) : `pilote_etat`, `pilote_identifiant`,
`pilote_famille_code`, `pilote_mode_code`, `pilote_fournis`, `pilote_lus`,
`pilote_ecritures`, `pilote_sondages`, `pilote_notifications`,
`pilote_evenement`, `pilote_empreinte`, `pilote_disponible`,
`pilote_sortie`.

Adaptation des données (hors brique) : `pilote_fournir(p, octets)` alimente
la zone d'entrée ; `pilote_sortie(p)` rend les octets écrits. Ce sont des
listes d'octets, pas des registres.

### Représentation de l'état

`p` est une `liste<entier>` : 16 cellules de métadonnées (indices 0..15),
puis `n_entree` octets d'entrée (à partir de 16), puis `n_sortie` octets de
sortie. Le détail des indices est en tête de `contrat.rose`.

L'état est une **valeur persistante** : chaque opération retourne un nouvel
état, à rebinder (`p = pilote_ouvrir(p, 17)`). Cela évite les globales et
la mutation de l'état de l'appelant, non fiables sous C0/G5.

### Exemple générique

```rose
$ops = [1, 2, 64, 0]                       # données du .bud
$p = pilote_declarer("generique", "bloc", $ops)
$p = pilote_ouvrir($p, 17)
$p = pilote_fournir($p, [1, 2, 3, 4])      # adaptateur, pas la brique
$l = pilote_lire($p, 4)                    # -> [1, 2, 3, 4]
$p = pilote_ecrire($p, [9, 8])
$p = pilote_poller($p)                     # sans IRQ
$s = pilote_sortie($p)                     # -> [9, 8]
$p = pilote_fermer($p)
```

---

## 3. B8 — `briques/abi/appel.rose`

### API

```rose
fn abi_declarer(nom: chaine, registres: liste<entier>, pile: liste<entier>) -> liste<entier>

fn syscat_nouvelle() -> liste<entier>
fn syscat_enregistrer(cat: liste<entier>, numero: entier, nom: chaine, signature: chaine) -> liste<entier>
fn syscat_appeler(cat: liste<entier>, numero: entier, args: liste<entier>) -> entier
fn syscat_chercher(cat, numero) -> entier
fn syscat_arite(cat, numero) -> entier
fn syscat_code(cat, numero) -> entier
fn syscat_trap(cat, numero, args) -> liste<entier>

fn isolate_nouveau(niveaux: entier) -> liste<entier>
fn isolate_autoriser(iso: liste<entier>, operation: chaine, niveau_min: entier) -> liste<entier>
fn isolate_verifier(iso: liste<entier>, niveau: entier, operation: chaine) -> booleen

fn chargeur_enregistrer(fmt: chaine, validateur: chaine) -> liste<entier>
fn chargeur_ajouter(reg: liste<entier>, fmt: chaine, validateur: chaine) -> liste<entier>
fn chargeur_valider(reg: liste<entier>, fmt: chaine, octets: liste<entier>) -> booleen
fn chargeur_charger(fmt: chaine, octets: liste<entier>) -> liste<entier>
fn chargeur_ecriveur(fmt: chaine) -> chaine
```

`abi_declarer` reçoit les conventions en données : `registres =
[nb_registres, convention, registre_retour]` (`1` = registres d'abord,
`2` = pile d'abord) et `pile = [sens, alignement, taille_min, nb_args_max]`
(`1` = croît, `2` = décroît). Aucun registre nommé, aucune largeur câblée.

`signature` est une chaîne de données `"<code>:<arite>:<type>"`, par
exemple `"2:2:entier"`. Les codes d'opération sont **abstraits** :
`1 identite`, `2 somme`, `3 produit`, `4 compte`, `5 nul`, `6 melange`.
Ce ne sont pas des syscalls d'un OS ; une table d'OS se mappe sur ces codes
via le `.bud`, sans toucher au code Rose.

`syscat_appeler` rend le résultat, `-1` (numéro inconnu), `-2` (arity
incorrecte) ou `-3` (opération inconnue). Convention des arguments : `[0]`
est un porte-type obligatoire (Rose n'a pas de littéral `[]`) ; les
arguments réels suivent. `[0]` = 0 argument, `[0, 42]` = un argument.

`isolate_nouveau(n)` définit des niveaux `0..n-1` : `0` = utilisateur,
`n-1` = plus privilégié. `isolate_autoriser` déclare le niveau minimal
requis par une opération ; `isolate_verifier` rend `vrai` si le niveau est
dans `[0, n)` et que l'opération est autorisée à ce niveau. Aucune
correspondance avec un anneau matériel n'est supposée.

`chargeur_charger` rend `[valide, code_format, taille, code_validateur,
code_ecriveur]`. Les codes validateur sont `1` (magic ELF `7f 45 4c 46`),
`2` (magic PE `4d 5a`), `3` (tout) ; les codes écriveur sont `1`
(`emetteur_elf.rose`), `2` (`emetteur_pe_x64.rose`), `0` (brut).

### Exemple générique

```rose
$a = abi_declarer("rose-utilisateur-v0", [4, 1, 0], [2, 8, 64, 4])
$cat = syscat_nouvelle()
$cat = syscat_enregistrer($cat, 1, "identite", "1:1:entier")
>> syscat_appeler($cat, 1, [0, 42])          # 42

$iso = isolate_nouveau(4)
$iso = isolate_autoriser($iso, "lire", 0)
>> isolate_verifier($iso, 0, "lire")         # vrai

$reg = chargeur_enregistrer("elf", "elf")
>> chargeur_valider($reg, "elf", [127, 69, 76, 70, 2, 1])  # vrai
>> chargeur_ecriveur("elf")                  # emetteur_elf.rose
```

---

## 4. Validation réellement exécutée (25/11/2026)

Moteur : `/home/clausake/Stefan/ROSE/rose/rose`, `Rose 0.1.73`.
Compilateur auto-hébergé : `/tmp/opencode/C0_h.elf`, md5
`2db168b907b4a874712857d3957fd078` (ELF aarch64, `e_machine = 183`).

```bash
cd /tmp/opencode/v2abi
ROSE=/home/clausake/Stefan/ROSE/rose/rose
C0=/tmp/opencode/C0_h.elf

$ROSE verif briques/pilote/contrat.rose            # vérifié
$ROSE verif briques/abi/appel.rose                 # vérifié

cat briques/pilote/contrat.rose briques/pilote/tests_contrat.rose \
  > /tmp/p.rose
$ROSE verif /tmp/p.rose                            # vérifié
$ROSE /tmp/p.rose > /tmp/p.rose.out
$C0 /tmp/p.rose > /tmp/p.elf                       # ELF64 aarch64
/tmp/p.elf > /tmp/p.elf.out
cmp /tmp/p.rose.out /tmp/p.elf.out                 # rc=0, 23 lignes

cat briques/abi/appel.rose briques/abi/tests_appel.rose > /tmp/a.rose
$ROSE verif /tmp/a.rose                            # vérifié
$ROSE /tmp/a.rose > /tmp/a.rose.out
$C0 /tmp/a.rose > /tmp/a.elf                       # ELF64 aarch64
/tmp/a.elf > /tmp/a.elf.out
cmp /tmp/a.rose.out /tmp/a.elf.out                 # rc=0, 32 lignes

# Chaîne complète B8 -> B7, ordre de dépendance respecté :
cat briques/abi/appel.rose briques/pilote/contrat.rose \
    briques/abi/tests_appel.rose briques/pilote/tests_contrat.rose \
  > /tmp/combined.rose
$ROSE verif /tmp/combined.rose                     # vérifié
$ROSE /tmp/combined.rose > /tmp/c.rose.out
$C0 /tmp/combined.rose > /tmp/c.elf
/tmp/c.elf > /tmp/c.elf.out
cmp /tmp/c.rose.out /tmp/c.elf.out                 # rc=0, 55 lignes
od -An -tu2 -j18 -N2 /tmp/c.elf                    # 183 (aarch64)
```

Résultats couverts : `pilote_declarer`/`ouvrir`/`fournir`/`lire`/
`ecrire`/`sortie`/`poller`/`notifier`/`fermer` sur deux profils de données
différents ; `syscat_enregistrer`/`appeler`/`trap` (identité, somme,
produit, compte, erreurs `-1`/`-2`) ; `isolate_verifier` (autorisé, refusé,
hors limites) ; `chargeur_valider`/`charger`/`ecriveur` (ELF, PE, brut).
**Toutes les sorties stdout sont octet pour octet identiques** entre
l'interpréteur et l'ELF, avec `rc=0` des deux côtés.

---

## 5. Limites explicites

1. **Rose n'a pas de modules ni de fonctions d'ordre supérieur.** Les
   briques se concatènent au programme ; un gestionnaire ou un validateur
   ne peut pas être passé comme valeur. Les « ops » et les « validateurs »
   sont donc des **codes en données**, interprétés par un mécanisme écrit
   en Rose. L'adaptateur matériel reste hors de ces fichiers.
2. **Aucun pilote concret, aucun OS, aucun CPU.** Ajouter un appareil
   consiste à écrire un adaptateur ailleurs et à changer le `.bud`, pas à
   modifier `contrat.rose` ni `appel.rose`.
3. **IRQ abstrait.** `pilote_notifier` enregistre un événement dans une
   liste ; il ne programme aucun contrôleur d'interruptions. B3
   (`interruptions/vecteurs.rose`) n'est pas encore livré dans ce worktree.
4. **Isolation abstraite.** `isolate_verifier` est une décision logique sur
   des niveaux `0..n-1` ; elle ne configure aucune table de pages ni aucun
   bit de protection matériel (B4).
5. **Sérialisation déléguée.** B8 reconnaît des en-têtes et nomme le writer
   compétent ; il n'émet ni ELF ni PE. L'émission complète passe par
   `compilateur/emetteur_elf.rose` / `emetteur_pe_x64.rose` ou par C0.
6. **Coût non mesuré.** Aucune latence ni empreinte n'est annoncée ici : les
   champs `latence_max` des `.bud` sont des cibles, pas des mesures. La
   règle 5 de `briques/README.md` exige un banc avant toute affirmation.
7. **Précision des entiers.** Les vecteurs d'état utilisent des `entier`
   Rose non bornés (B5 n'est pas encore câblé). Le profil noyau devra
   restreindre ces largeurs avant une utilisation matérielle réelle.
8. **Constat daté.** Les contraintes C0/G5 observées (pas de littéral `[]`,
   table lue typée `?`, `inserer` n'écrase pas une clé existante) sont
   valables pour le C0 ci-dessus ; elles doivent être re-mesurées après tout
   changement de compilateur. Les fichiers évitent délibérément ces
   constructions : tout l'état est dans des `liste<entier>` typées.
