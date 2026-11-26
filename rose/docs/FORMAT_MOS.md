# Format exécutable Rose `.mos` (v1)

> Constat daté : 26/11/2026. Worktree `v10mos`, HEAD `564dcb1`.
> Fichiers : `compilateur/format_mos.rose`, `briques/abi/mos.bud`,
> `briques/tests_mos.rose`. Aucun fichier du moteur Rust n'a été modifié.

## 1. Pourquoi un conteneur Rose

Les writers existants (`compilateur/emetteur_elf.rose`,
`compilateur/emetteur_pe_x64.rose`) produisent des formats **hôtes**. Le
chargeur générique `briques/abi/appel.rose` ne fait que reconnaître un magic
ELF/PE et déléguer la sérialisation ; `compilateur/vm_rose.rose` lit le
bytecode ROSEBC v3 mais n'a pas d'en-tête d'image, de table de sections ni de
contrôle d'intégrité.

`.mos` comble ce fossé : c'est un **conteneur universel** qui transporte un
RIR Rose et des métadonnées de cible. L'architecture est une donnée lue dans
`briques/abi/mos.bud` (`mos_arch_code`) : le chargeur ne contient aucun
`si architecture == "x86"` et aucun numéro de machine ELF/PE.

## 2. Format v1 — spécification octet pour octet

Le fichier est **ASCII-safe** : tous ses caractères sont < 128, sauf le NUL
du magic. C'est une contrainte délibérée : `lire_texte` remplace les octets
UTF-8 invalides par U+FFFD, donc un conteneur binaire ne survivrait pas à une
relecture en Rose pur. Les charges utiles sont donc hexadécimales.

| Offset | Longueur | Champ |
|---|---:|---|
| 0 | 4 | magic `MOS\0` (`4D 4F 53 00`) |
| 4 | 4 | version `0001` (majeure `00`, mineure `01`) |
| 8 | 10 | taille **totale** en caractères, décimale zéro-padded |
| 18 | 10 | code architecture (table dans `mos.bud`) |
| 28 | 10 | adresse d'entrée **logique**, décimale |
| 38 | 2 | nombre de sections (`03` en v1) |
| 40 | 22 × 3 | table des sections : `offset(10) | taille(10) | droits(2)` |

Longueur de l'en-tête : `40 + 22 × nombre_de_sections = 106` octets.
Droits : bit 0 = lecture, bit 1 = écriture, bit 2 = exécution (`05` =
lecture + exécution, `01` = lecture seule). Ce sont des droits abstraits,
pas des bits de pagination matérielle.

Sections v1, contiguës dans l'ordre de la table :

| # | Nom | Droits | Contenu |
|---|---|---|---|
| 1 | `rir` | 05 | RIR Rose, 4 chiffres hex par point de code |
| 2 | `cible` | 01 | métadonnées cible, 4 chiffres hex par point de code |
| 3 | `hash` | 01 | 24 chiffres hex : H1(8) + H2(8) + H3(8) |

Pourquoi 4 chiffres par point de code : les chaînes Rose sont des suites de
points de code dans le moteur Rust **et** dans l'émetteur G5. Encoder sur 4
chiffres rend la relecture exacte pour `π`, `é` ou un nom de fonction
accentué, sans dépendre de `u_bytes`/`u_texte`.

Empreintes (toutes calculées sur les caractères `[0, offset_hash)` — en-tête
et charges des sections 1 et 2 incluses) :

- H1 : `h = (h × 257 + code + 1) mod (2^31 − 1)`, graine 17 ;
- H2 : `h = (h × 131 + code + 7) mod (10^9 + 7)`, graine 31 ;
- H3 : `h = (h × 31 + code + 3) mod 2147483629`, graine 7.

Les trois moduli tiennent dans un `i64`, produit intermédiaire compris.
⚠ **Constat mesuré** : une version antérieure utilisait un modulus
`2^61 − 1` ; l'interpréteur Rust calculait la bonne valeur mais l'ELF
auto-hébergé C0 rendait `0`, car le bigint G5 est plus étroit. D'où le choix
de trois polynômes 31 bits, identiques sur les deux moteurs.

Codes de retour de `mos_valider` :

| Code | Signification |
|---:|---|
| 0 | valide |
| -1 | fichier tronqué (< 106 caractères) |
| -2 | magic absent |
| -3 | version non supportée |
| -4 | taille déclarée ≠ taille réelle |
| -5 | architecture ou adresse d'entrée invalide |
| -6 | nombre de sections ≠ 3 |
| -7 | table des sections incohérente (offset/droits) |
| -8 | section hors fichier |
| -9 | section hash absente ou de mauvaise taille |
| -11 | chiffre hexadécimal invalide |
| -12 | **empreinte mismatch** (octet modifié) |
| -13 | charge utile dont la longueur n'est pas un multiple de 4 |

## 3. API (`compilateur/format_mos.rose`)

Module autonome : aucun `importer`, aucune fonction externe. Il s'ajoute à
un programme par `importer "compilateur/format_mos.rose"` ou par
concaténation.

```
mos_ecrire(rir: chaine, cible: chaine, chemin: chaine) -> entier
    0 = fichier écrit ; -1 = architecture absente du .bud ;
    -2/-3 = champ hors largeur / taille incohérente.

mos_valider(octets: chaine) -> entier
    `octets` = contenu brut tel que rendu par lire_texte(chemin).
    0 = valide, <0 = code d'erreur (table ci-dessus).

mos_lire(chemin: chaine) -> liste<chaine>
    [sentinelle, statut, rir, cible, code_arch, entree, h1, h2, h3]
    statut = "OK" ou "ERR:<code>". La sentinelle suit la convention
    des autres briques (élément 0 inutilisé).
```

Autres fonctions publiques utiles : `mos_arch_code(nom)` (code lu dans
`mos.bud`), `mos_hex`/`mos_dehex`, `mos_hash1/2/3`, `mos_cible_champ`.

Format de l'argument `cible`, 6 champs séparés par `|` (le sixième est
facultatif, défaut `4194304`) :

```
architecture|boutisme|largeur_mot|largeur_page|abi|adresse_entree
GENERIQUE|petit|64|4096|rose-utilisateur-v0|4194304
```

## 4. Compiler une application en `.mos`

Il n'existe pas encore de commande `rose mos`. La chaîne est une
composition de trois outils déjà présents :

```bash
# 1. RIR de l'application (sortie texte ASCII + accents)
rose rir app.rose > /tmp/app.rir

# 2. petit pilote Rose (une seule ligne par application)
cat > /tmp/compil_mos.rose <<'EOF'
importer "compilateur/format_mos.rose"
$cible = "GENERIQUE|petit|64|4096|rose-utilisateur-v0|4194304"
$rc = mos_ecrire(lire_texte("/tmp/app.rir"), $cible, "/tmp/app.mos")
>> "ecriture=" + chaine($rc)
EOF
rose /tmp/compil_mos.rose

# 3. contrôle à l'entrée
cat > /tmp/verif_mos.rose <<'EOF'
importer "compilateur/format_mos.rose"
>> "validation=" + chaine(mos_valider(lire_texte("/tmp/app.mos")))
EOF
rose /tmp/verif_mos.rose
```

Un `.mos` peut aussi transporter des **données d'application** (les 43
programmes MOSX de `monos64-rose.rose` sont des listes d'octets Rose) : on
convertit chaque octet en point de code, puis on appelle `mos_ecrire`. Le
conteneur est agnostique : il transporte du RIR ou des données, sans jamais
imposer de format hôte.

Le banc `briques/tests_mos.rose` montre le cycle complet écriture →
relecture → validation → détection de 4 corruptions (magic, version,
troncature, un chiffre hex de la charge utile).

## 5. Preuves exécutées (26/11/2026)

```
rose verif compilateur/format_mos.rose           → vérifié
rose verif briques/tests_mos.rose                → vérifié

rose briques/tests_mos.rose           → 20 lignes, rc=0
C0_141.elf briques/tests_mos.rose → ELF aarch64, rc=0
ELF vs interpréteur                   → diff vide, 0 octet de stderr
```

Fichier produit par le banc (constat daté) :

- chemin : `/tmp/opencode/tests_mos_demo.mos`
- taille : **542 octets** (= 542 caractères ASCII)
- magic : `4D 4F 53 00` (`MOS\0`)
- version : `0001`, 3 sections, architecture `0`, entrée `4194304`
- hash v1 : `0e1cb24c285b34691b2fa57d`
- md5 du fichier : `10167614f72418bb8b1effbeaf5236ab`
- relecture : `statut=OK`, `rir_ok=1`, `cible_ok=1`, `hash_ok=1`
- corruptions : `faux_magic=-2`, `fausse_version=-3`, `tronque=-1`,
  `faux_payload=-12`, `relecture_corrompue=ERR:-12`

43 applications MOSX (`monos64-rose.rose` : 43 tables `$app_*_octets`,
48 088 octets au total) ont été empaquetées par un pilote temporaire :

```
apps43 ecriture=43/43
apps43 validation=43/43
apps43 contenu=43/43
```

(exemple : `paint.mos`, 3 374 octets, magic `MOS\0`, hash
`20e92fec3607d6522fc19c7c`.)

Invariants du moteur, vérifiés après l'ajout :

```
scripts/verifier_parite_selfhost.sh --moteur /tmp/opencode/final.elf
  tests=141  CONFORMES=139  DIVERGENTS=0  BLOQUES=2
  ERREURS self-host sur programme accepté par Rust = 0  → RC=0
  (les 2 BLOQUES sont les refus historiques t55/t107)

rose rir compilateur/emetteur.rose | md5sum
rose compilateur/emetteur.rose compilateur/emetteur.rose | md5sum
  b3ed9262dd54776a6dbd07bc12368d0e ×2, 263 520 octets, cmp = 0,
  stderr = 0 octet des deux côtés
```

## 6. Limites

- Le conteneur est **ASCII-safe** : charges utiles hexadécimales, donc
  fichier ~4× plus gros que le RIR brut. C'est le prix d'une relecture exacte
  par `lire_texte` sans primitive de lecture binaire.
- Le hash est un **contrôle d'intégrité**, pas une signature
  cryptographique : trois polynômes 31 bits détectent une alteration
  accidentelle, pas un adversaire déterminé.
- Le format est **fixe en v1** : exactement 3 sections, table de 22 octets,
  version `0001`. Une évolution doit incrémenter la version et le validateur.
- `.mos` **ne remplace pas l'émetteur ELF/PE** : il n'y a pas encore de
  chargeur natif qui exécute un `.mos`. Il fournit le conteneur, l'en-tête,
  la table et le contrôle d'entrée — l'exécution reste celle du RIR par
  `compilateur/interpreteur_rir.rose` ou par `compilateur/vm_rose.rose`
  après extraction de la section 1.
- L'adresse d'entrée est **logique** : aucun mapping mémoire matériel n'est
  appliqué (pas de pagination, pas de segment, pas de segment ELF/PE).
- Le hachage parcourt le texte caractère par caractère : sur un très gros
  RIR ( > 1 Mio de caractères), le coût est linéaire et sensiblement plus
  élevé sous l'ELF auto-hébergé que sous le moteur Rust.
- `lire_texte` refuse les fichiers > 2^28 octets : c'est la limite
  pratique d'un `.mos` lu par Rose.
- `mos_arch_code` lit `briques/abi/mos.bud` **relativement au répertoire
  courant** : les outils doivent être lancés depuis la racine du dépôt.
- Aucun écrit binaire natif n'est utilisé : `ecrire_texte` suffit parce que
  le contenu est entièrement ASCII.
