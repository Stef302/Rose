# NOYAU_ROSE_NU — compilation d'un noyau Rose en binaire machine NU x86 32 bits

Constat daté : 26/11/2026. Worktree `v10noyau`, HEAD `564dcb1`. Ce document
comble le **fossé système n°4 (la preuve)** : il existe désormais un petit
noyau dont la **logique est écrite en Rose** et qui est réellement **compilé
en code machine x86 32 bits nu** — pas un calcul d'octets recopiés à la main,
pas un ELF, pas un PE, pas de runtime Rose embarqué.

## 1. Ce qui est VRAIMENT compilé depuis Rose

| Élément | Rôle | Nature |
|---|---|---|
| `briques/noyau/demo.rose` | source du noyau : `principal()` | **logique Rose** |
| `compilateur/backend_universel.rose` | famille 4 `i386`, encodeur `bu_i386_encode` | **compilateur Rose** |
| `briques/noyau/lien_nu.rose` | lieur NU : contrat d'adresse, contrôles, écriture | **lieur Rose** |
| `briques/noyau/compiler_noyau.sh` | concaténation + exécution par le moteur Rose | pilote |
| `briques/backend/cible.bud` | déclaration de la cible `i386` et des ops 15/16 | données |

La chaîne de compilation est :

```
Rose (principal)  ->  IR abstrait (bu_operation)  ->  bu_traduire(i386)
                  ->  octets nus  ->  lien_nu  ->  kernel32.bin
```

- `principal()` construit une suite de **9 opérations abstraites** avec
  `bu_sequence`, `bu_ajouter`, `bu_operation`. Aucun opcode x86 n'y apparaît.
- Les caractères de `"ROSE KERNEL"` sont convertis en mots 32 bits par
  `ordre(sous_chaine(...))` **dans le Rose** (`noyau_mot`) : aucun `82`, `79`,
  `83`… recopié en dur, contrairement à `os-rose.rose`.
- Les adresses `0x100000` (chargement) et `0x101000` (chaîne) sont des
  **données du lieur** (`$NU_ADRESSE_CHARGE`, `$NU_ADRESSE_CHAINE`), pas des
  immédiats codés dans l'émetteur.
- L'encodeur `bu_i386_encode` est le **seul** endroit qui connaît les opcodes
  (`B8+r`, `89 /r`, `F4`, `EB disp8`…). Il réutilise `bu_mot32`, donc le
  boutisme vient du profil de cible, jamais du code.
- Deux opérations abstraites ont été ajoutées au jeu commun : `hlt` (code 15)
  et `jmp_rel8` (code 16). Les 15 premières opérations et les trois familles
  historiques (aarch64, x86_64, wasm) sont inchangées.

### Ce qui reste « calculé à la main » (et pourquoi ce n'est PAS ce noyau-ci)

`noyau.rose`, `os-rose.rose`, `os-rose-complet.rose`, `os-rose-final.rose`,
`boot-rose-complet.rose` et `monos64-rose.rose` écrivent des opcodes un par un
avec `octet_brut(190)`, `octet_brut(244)`… Ce sont des **calculs d'octets en
Rose**, utiles pour générer un ELF multiboot, mais **aucune compilation de
logique Rose en machine**. Ici, au contraire, la logique est dans une fonction
Rose et l'émetteur la traduit : changer `principal()` change le binaire sans
toucher à un seul opcode.

## 2. Résultat produit

- Fichier : `briques/noyau/kernel32.bin`
- Taille : **41 octets** (cible < 4 Kio)
- Machine : **i386 / x86 32 bits**, famille de backend 4
- Adresse de chargement : **0x100000** (contrat vérifié par `lien_nu.rose`,
  consigné dans `briques/noyau/kernel32.map`)
- Adresse de la chaîne : **0x101000**
- Aucun header ELF (`7F 45 4C 46`) ni PE (`4D 5A`) : le premier octet est `B8`.
- Aucun import, aucune relocation, aucun symbole, aucun runtime Rose.

Octets (décalage depuis l'entrée) :

```
00: b8 00 10 10 00        mov $0x101000, %eax       ; adresse fixe
05: b9 52 4f 53 45        mov $0x45534f52, %ecx     ; "ROSE"
0a: 89 88 00 00 00 00    mov %ecx, (%eax)
10: b9 20 4b 45 52        mov $0x52454b20, %ecx     ; " KER"
15: 89 88 04 00 00 00    mov %ecx, 0x4(%eax)
1b: b9 4e 45 4c 00        mov $0x4c454e, %ecx       ; "NEL\0"
20: 89 88 08 00 00 00    mov %ecx, 0x8(%eax)
26: f4                    hlt
27: eb fe                 jmp 0x27                  ; boucle sur HLT
```

## 3. Procédure

```bash
cd /tmp/opencode/v10noyau                       # ou tout clone à HEAD 564dcb1

# 1. compiler le noyau (garde-fou mémoire 12 Go inclus)
bash briques/noyau/compiler_noyau.sh

# 2. preuve de désassemblage — commande de référence :
objdump -D -b binary -mi386 briques/noyau/kernel32.bin
# SUR CETTE MACHINE (hôte aarch64) l'objdump GNU ne contient pas i386 :
# compiler_noyau.sh bascule automatiquement sur l'équivalent
#   llvm-objcopy -I binary -O elf32-i386 -B i386 kernel32.bin /tmp/.../kernel32_i386.o
#   llvm-objdump -D --triple=i386 --section=.data /tmp/.../kernel32_i386.o
# L'enveloppe ELF sert UNIQUEMENT à l'outil ; le fichier livré reste le .bin nu.

# 3. preuve d'exécution en anneau 0 (qemu) : enveloppe multiboot TEMPORAIRE,
#    dump mémoire, contrôle que "ROSE KERNEL\0" est à 0x101000
bash briques/noyau/verifier_noyau_qemu.sh

# 4. non-régression
bash scripts/garde_fou_mem.sh 12000000 \
  bash scripts/verifier_parite_selfhost.sh \
    --moteur /tmp/opencode/moteur_141.elf \
    --rose-bin /tmp/opencode/v8update/rose/target/release/rose \
    --workdir /tmp/opencode/v10noyau_parite
R=/tmp/opencode/v8update/rose/target/release/rose
bash scripts/garde_fou_mem.sh 12000000 "$R" rir compilateur/emetteur.rose > /tmp/opencode/pf1.rir
bash scripts/garde_fou_mem.sh 12000000 "$R" compilateur/emetteur.rose compilateur/emetteur.rose > /tmp/opencode/pf2.rir
md5sum /tmp/opencode/pf1.rir /tmp/opencode/pf2.rir   # b3ed9262dd54776a6dbd07bc12368d0e ×2
```

## 4. Preuves mesurées

- `objdump` (équivalent llvm sur hôte aarch64) : séquence exacte
  `movl $0x101000,%eax` / `movl $0x45534f52,%ecx` / `movl %ecx,(%eax)` /
  `movl $0x52454b20,%ecx` / `movl %ecx,0x4(%eax)` / `movl $0x4c454e,%ecx` /
  `movl %ecx,0x8(%eax)` / `hlt` / `jmp 0x27`. Aucun import, aucun header.
- `qemu-system-i386` : le dump de mémoire invitée contient
  `ROSE KERNEL\0` à l'adresse physique **0x101000** (vérifié par lecture des
  `PT_LOAD` du dump, pas seulement par recherche de la chaîne).
- Parité self-host : **141/141 CONFORMES**, 0 divergent, 0 bloqué
  (rc + stdout octet pour octet, oracle Rust `e358c4b9…` / moteur
  self-hosté `c0f90c6e…`).
- Point fixe émetteur : `b3ed9262dd54776a6dbd07bc12368d0e` sur les deux
  passes (`rir emetteur.rose` et `emetteur.rose emetteur.rose`), 263 520 octets.

## 5. Ce qui reste à faire (hors de ce chantier)

Le binaire est un **noyau de démonstration**, pas un système d'exploitation :
- **pas de GDT / TSS /anneau 0 explicite** : le processeur est déjà en anneau 0
  après le multiboot de la preuve ; l'installation de la GDT est du ressort de
  l'agent **v10asm** (ASM privilégié) ;
- **pas de pagination** (`cr0`, `cr3`, pages 4 Kio) : à faire avec v10asm ;
- **pas de IDT / PIC / interruptions matérielles** : le `hlt` est une boucle,
  pas un ordonnanceur ;
- **pas de chargeur propre** : `kernel32.bin` suppose un bootloader qui saute
  à `0x100000` (le wrapper multiboot du script de preuve n'est qu'un outil) ;
- **pas de syscalls** : `int 0x80` est encoding (code 14) mais non utilisé ;
  la table d'appels abstraite (`abi/appel.rose`) reste à brancher ;
- **pas de pilotes, ni de tas, ni de formats de fichiers** ;
- **pas de relocations** : toutes les adresses sont absolues, donc le noyau
  doit être chargé exactement à `0x100000` (le lieur le vérifie).

Le lieur de ce chantier est `briques/noyau/lien_nu.rose`, écrit ici parce que
`compilateur/link_nu.rose` est en cours dans le worktree de l'agent **v10free**
(worktree non touché). Quand ce lieur sera disponible, la seule étape à
remplacer sera l'appel `lien_nu($sequence_noyau)` dans `lien_nu.rose`.
