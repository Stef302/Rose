# Backend freestanding — brique universelle 16 / 32 / 64 bits

*Constat daté : 26/11/2026. Worktree exclusif `/tmp/opencode/v11x64`, HEAD
`c2c8e54`. Aucun commit, aucun push. Aucun noyau, aucun démo OS, aucun
`.mos` : uniquement le mécanisme, comme `-target x86_64-freestanding-none`.*

## 1. Doctrine

Le backend machine générique ne contient **aucune logique spécifique à un
système**. Il n'émet ni ELF, ni PE, ni COFF, ni table de symboles, ni
relocation, ni en-tête, ni runtime. Il traduit un IR abstrait en octets et
laisse un contrat de chargement décider où ces octets sont placés.

```
IR abstrait (ops + args) ──bu_traduire(seq, profil)──> octets nus
                                   ▲
                                   └── profil = DONNÉES (bu_cible)
                    ──ln_config(adresse, alignement, entrée)──> binaire nu
```

- `bu_traduire` ne contient **aucun nom d'architecture**. Il lit la famille
  d'encodeurs dans le profil, vérifie la disponibilité de l'opération dans
  `$bu_encoder_table`, puis délègue à l'encodeur de cette famille.
- `bu_x86nu_encode` est **UN SEUL encodeur partagé** par les familles 4, 5 et
  6. Il ne contient aucun `si x86_16` / `si x86_64` : la largeur d'opérande
  (`2/4/8`), le préfixe 0x66, le préfixe de registre (`REX.W = 0x48`), la
  largeur d'adresse (`2/4`), le boutisme, l'opcode d'E/S et le vecteur sont
  des **champs de données** du profil `bu_cible`.
- La forme d'E/S est également une donnée numérique : `syscall_opcode = 0`
  sélectionne `int_opcode vecteur` (`CD imm8`) ; `syscall_opcode = 15` avec
  `syscall_code = 5` sélectionne `0F 05`. Aucun nom de système n'apparaît
  dans le code. Le test 29 remplace **un seul champ** du profil 64 bits et
  obtient `CD 80` du même encodeur — la preuve que le choix vient des données.
- L'adresse de chargement n'est **pas écrite dans le fichier** : c'est un
  paramètre du contrat (`ln_config`), lu par les accesseurs `ln_charge`,
  `ln_entree`, `ln_adresse_section`. Le fichier commence par le premier octet
  de code.

Le linker (`compilateur/link_nu.rose`) est Rose pur, sans `ld`, sans `cc`,
sans objet, sans outil externe. `ln_octets(config)` rend la liste d'octets
 porteuse ; `ln_bytes` et `ln_ecrire` en construisent la vue en chaîne brute
 localement. Cette séparation garantit que la source de vérité est la même
 sous l'oracle Rust et sous le moteur self-hosté C0 (une chaîne brute
 **retournée** par une fonction était altérée par C0 — pièce mesurée, corrigée
 dans ce chantier).

## 2. Cibles

| Cible (`bu_cible`) | Famille | Registres | Immédiat / préfixe | Adressage | E/S | Enveloppe |
|---|---|---|---|---|---|---|
| `x86_16` | 4 | 16 bits | `B8+r + imm16`, `0x66` si > 0xFFFF | `modrm mod=10 + disp16` | `CD vecteur` (0x10) | `brut` |
| `x86_32` | 5 | 32 bits | `B8+r + imm32` | `modrm mod=10 + disp32` | `CD vecteur` (0x80) | `brut` |
| `x86_64_nu` | 6 | 64 bits | `REX.W 48 B8+r + imm64` | `modrm mod=10 + disp32` | `0F 05` (`CD vecteur` par donnée) | `brut` |

`x86_64` (famille 2) reste le profil historique à `movabs` du backend
universel : la brique NU 64 bits est un **profil distinct** (`x86_64_nu`,
famille 6), ajouté sans modifier la famille 2 ni casser `tests_multi.rose`.

Les profils sont déclarés — uniquement en données — dans
`briques/backend/freestanding.bud` et `briques/backend/cible.bud` :
`largeur_registre`, `boutisme`, `mot_octets`, `registres`, `format_saut`,
`entree_sortie`, `enveloppe`, `adresse_charge`, `alignement`, `entre`,
`segments`, `prefixe_reg`, `syscall_opcode`, `syscall_code`, `int_opcode`.

### Limites honnêtes

- Les opérations à 3 opérandes sont abaissées en `mov dst,src` puis opération
  à 2 opérandes (aucun opcode x86 à 3 opérandes).
- REX.W est émis avec R/X/B à 0 : les registres 8..15 ne sont pas extensibles
  par la famille 6 ; seuls les registres 0..7 sont couverts par les tests.
- En mode 16 bits, `modrm rm=001` désigne `[bx+di+disp16]` (pas un `disp16`
  pur) : les octets `8B 81 10 00` sont conformes à l'encodage, l'interpretation
  d'adresse dépend du mode. Aucune relocation ni label dans cette couche.
- Aucun prologue/épilogue, aucun appel système de haut niveau, aucun
  ordonnanceur : ce document décrit un mécanisme de production d'octets,
  pas un système d'exploitation.

## 3. Suite d'opérations commune

La MÊME suite abstraite est traduite vers les trois cibles :

```
imm r0, 5 ; imm r1, 7 ; add r0, r0, r1 ; store r0, [r1+16] ; jmp r1 ; ret
```

| Cible | Octets produits | Taille |
|---|---|---|
| `x86_16` | `b80500 b90700 8bc0 01c8 89811000 ffe1 c3` | 17 |
| `x86_32` | `b805000000 b907000000 8bc0 01c8 898110000000 ffe1 c3` | 23 |
| `x86_64_nu` | `48b80500000000000000 48b90700000000000000 488bc0 4801c8 48898110000000 48ffe1 c3` | 37 |

Preuve REX.W : 6 octets `0x48` dans la suite 64 bits, **0** dans la suite
32 bits (premier octet `B8`). Le saut `FF E1` est commun aux trois cibles.

Preuve d'adresse de chargement (linker générique) :

| image | `ln_charge` | `ln_entree` | alignement | `.data` | taille fichier |
|---|---|---|---|---|---|
| 16 bits | `31232` (0x7C00) | 31232 | 1 | offset 17 / adr. 31249 | 19 |
| 32 bits | `1048576` (0x100000) | 1048576 | 16 | offset 32 / adr. 1048608 | 36 |
| 64 bits | `4194304` (0x400000) | 4194304 | 16 | offset 48 / adr. 4194352 | 52 |

Aucun en-tête : les fichiers commencent par `b8 05` (16/32) ou `48 b8` (64) ;
jamais `7f 45 4c 46` (ELF) ni `4d 5a` (PE). Les trous d'alignement sont des
zéros ; `.sig` = `55 AA` pour la cible 16 bits, `.data` = `2A 00 00 00`
(42 en little endian) pour les cibles 32 et 64 bits.

## 4. Validation reproductible

```bash
cd /tmp/opencode/v11x64
mkdir -p /tmp/opencode/v11x64_fs
cat compilateur/backend_universel.rose compilateur/link_nu.rose \
    briques/tests_freestanding.rose > /tmp/opencode/v11x64_fs/fs.rose

bash scripts/garde_fou_mem.sh 12000000 \
  /home/clausake/Stefan/ROSE/rose/rose verif /tmp/opencode/v11x64_fs/fs.rose
bash scripts/garde_fou_mem.sh 12000000 env ROSE_SANS_JIT=1 \
  /home/clausake/Stefan/ROSE/rose/rose interprete /tmp/opencode/v11x64_fs/fs.rose \
  > /tmp/opencode/v11x64_fs/interp.out
bash scripts/garde_fou_mem.sh 12000000 /tmp/opencode/C0_h.elf \
  /tmp/opencode/v11x64_fs/fs.rose > /tmp/opencode/v11x64_fs/fs.elf
chmod +x /tmp/opencode/v11x64_fs/fs.elf
/tmp/opencode/v11x64_fs/fs.elf > /tmp/opencode/v11x64_fs/elf.out
diff /tmp/opencode/v11x64_fs/interp.out /tmp/opencode/v11x64_fs/elf.out

# Désassemblage externe (hôte aarch64 : objdump i386/x86_64 absents)
llvm-objcopy -I binary -O elf64-x86-64 -B x86_64 out/nu64.bin out/nu64.o
llvm-objdump -D --triple=x86_64 --section=.data out/nu64.o
llvm-objcopy -I binary -O elf32-i386 -B i386 out/noyau.bin out/noyau.o
llvm-objdump -D --triple=i386 --section=.data out/noyau.o

bash scripts/garde_fou_mem.sh 12000000 bash scripts/verifier_parite_selfhost.sh \
  --moteur /tmp/opencode/moteur_141.elf \
  --rose-bin /home/clausake/Stefan/ROSE/rose/rose \
  --workdir /tmp/opencode/v11x64_fs/parite
R=/home/clausake/Stefan/ROSE/rose/rose
bash scripts/garde_fou_mem.sh 12000000 "$R" rir compilateur/emetteur.rose > /tmp/pf1.rir
bash scripts/garde_fou_mem.sh 12000000 "$R" compilateur/emetteur.rose compilateur/emetteur.rose > /tmp/pf2.rir
md5sum /tmp/pf1.rir /tmp/pf2.rir
```

### Résultats mesurés le 26/11/2026

- `rose verif` sur le programme concaténé : **`vérifié`**.
- Suite `briques/tests_freestanding.rose` : **55/55** sous l'oracle Rust
  *et* sous le moteur self-hosté C0 ; sorties octet pour octet identiques
  (md5 `feae9fda87819e1e4614a1fb4f73cd42` des deux côtés, `diff` muet).
- Fichiers écrits : `bootloader.bin` 19 o, `noyau.bin` 36 o, `nu64.bin` 52 o.
- `llvm-objdump --triple=x86_64` : `movabsq $0x5,%rax`, `movabsq $0x7,%rcx`,
  `movq`, `addq`, `movq %rax,0x10(%rcx)`, `jmpq *%rcx`, `retq` — REX.W
  (`48`) présent sur chaque opération de registre.
  `llvm-objdump --triple=i386` : `movl $0x5,%eax`, `movl $0x7,%ecx`,
  `movl`, `addl`, `movl %eax,0x10(%ecx)`, `jmpl *%ecx`, `retl` — **aucun**
  `0x48`. Capstone 5.0.7 confirme aussi le mode 16 bits (`mov ax,5`,
  `add ax,cx`, `jmp cx`).
- Parité self-host : **141/141 CONFORMES**, 0 divergent, 0 bloqué, 0 erreur,
  RC=0 (TSV horodaté dans `/tmp/opencode/v11x64_fs/parite`).
- Point fixe : `b3ed9262dd54776a6dbd07bc12368d0e` sur les deux passes
  (`rir emetteur.rose` et `emetteur.rose emetteur.rose`), `cmp` muet.
- Aucun fichier du moteur Rust ni de `emetteur.rose` modifié.

Fichiers modifiés (worktree exclusif, **aucun commit/push**) :
`compilateur/backend_universel.rose` (famille 6, encodeur partagé, E/S en
données), `compilateur/link_nu.rose` (`ln_octets`, parité Rust/C0),
`briques/backend/freestanding.bud`, `briques/backend/cible.bud` (profil
`x86_64_nu`, données seules), `briques/tests_freestanding.rose` (suite commune
16/32/64, 55 assertions), `rose/docs/BACKEND_FREESTANDING.md` (ce document).
