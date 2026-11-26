# Backend freestanding — binaire machine nu à adresse fixe

*Constat daté : 26/11/2026, worktree `/tmp/opencode/v10free`, HEAD `564dcb1`.*
*Chantier « fossé système n°1 » : aucun commit, aucun push.*

## 1. Pourquoi

Le backend universel (`compilateur/backend_universel.rose`) savait produire
des octets pour aarch64, x86-64 et wasm, mais toujours **dans une enveloppe**
(ELF, PE, module wasm). Il manquait la chaîne qui produit un **binaire machine
nu à adresse fixe**, sans runtime, sans ELF, comme
`zig build-exe -target x86_32-freestanding-none -T kernel.ld`.

Rose la fournit désormais en trois pièces, **toutes en Rose pur** :

```
 IR abstrait (ops)  ──bu_traduire(seq, bu_cible("x86_32"))──>  octets machine
                                    │
                                    ▼
            ln_config(0x100000, 16, 0x100000)
            ln_ajouter_section(".text", octets)
            ln_ecrire(cfg, "noyau.bin")   ──>  BINAIRE NU (aucun en-tête)
```

Le contrat « adresse de charge / alignement / mode / entrée / segments » est
déclaré dans `briques/backend/freestanding.bud` : c'est une **donnée**, pas une
structure codée dans le compilateur. Aucun appel à `ld`, `cc`, `objcopy` ni
aucun runtime n'est nécessaire.

## 2. Cibles réellement supportées

| Nom | Famille | Mode | Mot | Préfixe | Adressage | Vecteur `int` | Empreinte |
|---|---|---|---|---|---|---|---|
| `x86_16` | 4 | réel (bootloader) | 2 octets | `0x66` si immédiat > `0xFFFF` | `mod=10` + disp16 | `0x10` (BIOS) | `b8 05 00` |
| `x86_32` | 5 | protégé, anneau 0 (noyau) | 4 octets | aucun | `mod=10` + disp32 | `0x80` | `b8 05 00 00 00` |

**Les deux cibles partagent UN SEUL encodeur**, `bu_x86nu_encode` : la
largeur d'opérande, le préfixe 0x66, la largeur d'adresse, le boutisme et le
vecteur d'interruption sont lus dans le profil renvoyé par `bu_cible`.
`bu_traduire` ne contient toujours **aucun nom d'architecture** : il lit la
famille dans le profil, vérifie la disponibilité de l'opération dans la table
de données `$bu_encoder_table` (lignes `4:code:4xx` et `5:code:5xx`), puis
appelle l'encodeur de la famille. Ajouter une cible = ajouter un `si` de
données dans `bu_cible`, 15 lignes de table et (au plus) un encodeur de famille.

Opérations couvertes (les 15 de l'IR) : `nop imm mov add sub mul and or xor
load store jmp call ret syscall`. Les ops à 3 opérandes sont abaissées en
`mov dst,src` puis opération x86 à 2 opérandes ; `jmp`/`call` utilisent
`FF /4` et `FF /2` (saut indirect par registre) ; `syscall` est `int imm8`.

## 3. API

### Backend (inchangée, étendue)

```
bu_operation(nom, args) -> entier          # id d'une op abstraite
bu_sequence() -> liste<entier>             # suite porteuse [0]
bu_ajouter(seq, op) -> entier              # ajoute l'op (mutation en place)
bu_cible(nom) -> liste<chaine>             # profil = DONNÉES (cle=valeur)
bu_profil(p, cle) / bu_profil_entier(p, cle)
bu_traduire(seq, cible) -> liste<entier>   # octets porteurs [0]
bu_emettre(seq, cible, chemin) -> entier   # écriture via ecrire_bytes
```

Champs de profil propres aux cibles NU : `largeur_operande`, `prefixe_32`,
`forcer_32`, `taille_adresse`, `vecteur_defaut`, `mode`. Une clé absente vaut
0 (`bu_profil_entier` la gère) : un profil peut omettre les champs d'une autre
famille sans rompre la traduction.

### Linker NU — `compilateur/link_nu.rose`

```
ln_config(adresse_charge, alignement, entre) -> liste<entier>
ln_ajouter_section(nom, octets) -> entier      # id de section (1-based)
ln_ecrire(config, chemin) -> entier            # octets écrits, ou -1
ln_bytes(config) -> liste<entier>              # octets porteurs, contrôle portable
ln_charge / ln_alignement / ln_entree / ln_entree_offset / ln_statut (config)
ln_nb_sections() / ln_offset(id) / ln_taille_section(id)
ln_adresse_section(config, id) / ln_nom_section(id) / ln_valider(config)
```

- `adresse_charge` et `entre` sont des **adresses absolues** ; l'offset
  d'entrée vaut `entre - adresse_charge` (`ln_entree_offset`).
- `alignement` est en octets ; les trous entre sections sont des zéros.
- `ln_valider` exige que l'entrée tombe dans une section (code `-4` sinon).
- Le fichier ne contient **aucun en-tête** : le premier octet écrit est le
  premier octet de `.text`. L'adresse de charge est un paramètre du contrat
  de chargement, pas un champ du fichier (exactement comme un secteur de boot
  placé à `0x7C00` par le BIOS).

### Données — `briques/backend/freestanding.bud`

Déclarations seules : `adresse_charge`, `alignement`, `entre`, `mode`,
`segments`, largeur d'opérande, préfixe, taille d'adresse, vecteur par défaut,
jeu d'opérations et table d'encodeurs. Aucune logique.

## 4. Compiler un noyau en Rose

```bash
cd /path/to/Rose
mkdir -p /tmp/opencode/freestanding/out

# 1. Rose n'a pas d'`include` : concaténer le backend, le linker et le programme.
cat compilateur/backend_universel.rose \
    compilateur/link_nu.rose \
    briques/tests_freestanding.rose > /tmp/opencode/freestanding/fs.rose

# 2. Vérification statique, puis exécution (oracle Rust).
rose verif /tmp/opencode/freestanding/fs.rose
ROSE_SANS_JIT=1 rose interprete /tmp/opencode/freestanding/fs.rose

# 3. Le MÊME programme compilé par C0 en ELF aarch64 autonome.
/tmp/opencode/C0_h.elf /tmp/opencode/freestanding/fs.rose > fs.elf
chmod +x fs.elf && ./fs.elf          # sortie octet pour octet identique

# 4. Les deux fichiers nus sont dans /tmp/opencode/freestanding/out/.
od -An -tx1 .../bootloader.bin
od -An -tx1 .../noyau.bin
```

Programme minimal (noyau à `0x100000`) :

```rose
$c = bu_cible("x86_32")
$s = bu_sequence()
bu_ajouter($s, bu_operation("imm", [0, 5]))
bu_ajouter($s, bu_operation("imm", [1, 7]))
bu_ajouter($s, bu_operation("add", [0, 0, 1]))
bu_ajouter($s, bu_operation("ret", [0]))
$oct = bu_traduire($s, $c)

$cfg = ln_config(1048576, 16, 1048576)
ln_ajouter_section(".text", $oct)
ln_ecrire($cfg, "noyau.bin")       # 15 octets, aucun en-tête
```

## 5. Preuves mesurées (26/11/2026)

**Opérations (addition + saut), même IR traduit deux fois :**

```
x86_16 (13 octets) : b8 05 00 b9 07 00 8b c0 01 c8 ff e1 c3
x86_32 (17 octets) : b8 05 00 00 00 b9 07 00 00 00 8b c0 01 c8 ff e1 c3
```

Préfixe 0x66 (immédiat 70000 = `0x11170`) :
`66 ba 70 11 01 00` en x86_16, `ba 70 11 01 00` en x86_32.
Adressage `[r1+16]` : `8b 81 10 00` (disp16) vs `8b 81 10 00 00 00` (disp32).
`syscall` : `cd 10` (mode réel) vs `cd 80` (mode protégé).

**Désassemblage** (`objdump -D -b binary -mi8086/-mi386` n'existe PAS sur cet
hôte aarch64 ; équivalent mesuré : `llvm-objcopy -I binary -O elf32-i386 -B
i386` puis `llvm-objdump -D --triple=i386 [--mattr=+16bit-mode]`) :

```
x86_16 : movw $0x5,%ax ; movw $0x7,%cx ; movw %ax,%ax ;
         addw %cx,%ax ; jmpw *%cx ; retw ; pushw %bp ; stosb
x86_32 : movl $0x5,%eax ; movl $0x7,%ecx ; movl %eax,%eax ;
         addl %ecx,%eax ; jmpl *%ecx ; retl
```

**Binaire nu et adresse de chargement :**

| Image | Adresse de charge | Entrée | Sections | Taille fichier | Octets |
|---|---|---|---|---|---|
| `bootloader.bin` | `31232` = `0x7C00` | `31232` (offset 0) | `.text`@0, `.sig`@13 | 15 | `…c3 55 aa` |
| `noyau.bin` | `1048576` = `0x100000` | `1048576` (offset 0) | `.text`@0, `.data`@32 | 36 | `…c3` + 15×`00` + `2a 00 00 00` |

Preuve d'adresse : `ln_charge(cfg)`, `ln_entree(cfg)`,
`ln_adresse_section(cfg, id_sig)` = `31245` = `0x7C0D`, et
`ln_adresse_section(cfg, id_data)` = `1048608` = `0x100020`.

**Validation globale :**

- `rose verif` : `compilateur/backend_universel.rose` **vérifié**, programme
  concaténé (backend + linker + tests) **vérifié**.
- C0 → ELF : compilation rc=0 ; sorties interpréteur et ELF **identiques**,
  `TOTAL 37/37` des assertions de `briques/tests_freestanding.rose`.
- Parité self-host `scripts/verifier_parite_selfhost.sh` :
  `tests=141 CONFORMES=141 DIVERGENTS=0 BLOQUES=0`, verdict RC=0.
- Point fixe émetteur : `md5(rir(compilateur/emetteur.rose))` =
  `md5(compilateur/emetteur.rose compilateur/emetteur.rose)` =
  `b3ed9262dd54776a6dbd07bc12368d0e` (inchangé).
- Non-régression : la suite historique `briques/backend/tests_multi.rose`
  rend toujours `TOTAL 33/33`, sortie octet pour octet identique.

## 6. Limites

- **8 registres** (`registres=8`) : les opérations `imm`, `mov`, `add`…
  utilisent `B8+r`, `8B /r` et les modrms `mod=11` ; aucun REX, donc pas de
  registres 8–15.
- Immédiat : 16 bits en mode réel (au-delà → préfixe `0x66` + immédiat 32
  bits), 32 bits en mode protégé. Aucun immédiat 64 bits sur ces cibles.
- **Pas de saut conditionnel** dans l'IR : `jmp`/`call` sont indirects par
  registre (`FF /4`, `FF /2`). Les branches conditionnelles, les étiquettes
  et les relocations ne sont pas résolues par ce linker.
- **Pas de symboles, pas de relocations, pas de segments dans le fichier** :
  l'image est une concaténation de sections + padding zéro. Le chargement est
  à la responsabilité du BIOS ou du bootloader de première étape.
- **Pas de runtime** : ni libc, ni crt, ni tas, ni GC, ni pilote d'E/S.
  `syscall` se traduit par `int imm8` (vecteur de l'IR ou `vecteur_defaut`
  du profil) ; un noyau réel doit installer sa propre table d'interruptions.
- L'entrée doit appartenir à une section, sinon `ln_valider` rend `-4` et
  `ln_ecrire` ne écrit rien.
- L'état du linker est une **génération** : les accesseurs de sections
  (`ln_offset`, `ln_adresse_section`, `ln_bytes`) lisent la génération
  courante. Après un second `ln_config`, capturer les valeurs de la génération
  précédente avant de poser la suivante (voir `briques/tests_freestanding.rose`).
- Pièges de moteur : `lire_texte` remplace les octets > `0x7F` par U+FFFD
  (pas de lecture binaire en Rose) ; sous le backend natif G5, `longueur` et
  `sous_chaine` sur une chaîne d'octets bruts comptent des **points de code**,
  pas des octets — d'où `ln_bytes -> liste<entier>` ; `octet_brut` ne doit être
  défini qu'**une seule fois** par programme concaténé (ici dans le backend).
- Le répertoire de sortie (`/tmp/opencode/freestanding/out/`) doit exister :
  Rose n'a pas de primitive `mkdir` dans ce sous-ensemble.

## 7. Fichiers

| Fichier | Rôle | État |
|---|---|---|
| `compilateur/backend_universel.rose` | familles 4/5 + encodeur NU partagé + table | modifié |
| `compilateur/link_nu.rose` | linker script nu, API `ln_*` | créé |
| `briques/backend/freestanding.bud` | profils de cible en données | créé |
| `briques/tests_freestanding.rose` | suite ops + linker, 37 assertions | créé |
| `rose/docs/BACKEND_FREESTANDING.md` | cette doctrine | créé |
