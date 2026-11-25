# Briques universelles Rose — état des lieux et plan

> Date : 25/11/2026. Dépôt : `Stef302/Rose`, HEAD `1708a4bf4bd4810244c2a8e3fb89f49f766d45f2`.
> Périmètre de la recherche : `compilateur/*.rose`, `rose/src/**`, `rose/runtime/**`, `scripts/**`.
> Chaque état ci-dessous est appuyé par un `fichier:ligne` réellement lu. Les absences sont des
> comptages de correspondances dans ce périmètre, pas des impressions.

## 0. Pourquoi ce document

L'objectif n'est **pas** d'ajouter à Rose des fonctions natives prêtes à l'emploi pour un OS
donné (« pilote clavier », « driver framebuffer », « pilote disque ATA »…). Ce sont des
*recettes* : elles codent en dur un matériel, un contrôleur, une architecture et une ABI.

L'objectif est de doter Rose des **briques génériques** dont un système d'exploitation, une
bibliothèque mathématique ou un firmware se construisent ensuite **en Rose**, exactement comme
Zig fournit un compilateur autonome et une `std` compilée avec le programme — pas un Runtime
précuit.

Règle directrice : **une brique décrit un mécanisme, jamais un appareil.** Le passage du
mécanisme à l'appareil se fait par une **description de cible déclarative** (`.bud`), jamais
par une fonction spécialisée dans le compilateur.

Conséquence immédiate : Rose doit pouvoir « tourner sur n'importe quel appareil électronique »
(et, plus tard, sur des qubits) **sans qu'aucun backend ne connaisse le nom de cet appareil**.
Le compilateur connaît des primitives ; le manifeste connaît le matériel.

---

## 1. Tableau des 9 briques

| # | Brique demandée | État | Preuve (fichier:ligne) | Universel / spécifique | Brique générique à construire |
|---|-----------------|------|------------------------|------------------------|-------------------------------|
| 1 | backend freestanding (sans OS hôte, sans PIE, startup propre, linker script maîtrisé) | **existe (partiel)** | `rose/src/link_natif.rs:1-24` (ELF64/PE64 écrits à la main, zéro outil externe, stub `_start`) ; `rose/src/link_natif.rs:37` (non-PIE) ; `rose/src/natif_emetteur.rs:819-836` (ELF64 ET_EXEC statique, `_start` en dur, « aucun runtime, aucune libc, aucun linker externe ») ; `compilateur/emetteur_elf.rose:5733,5736` (e_type ET_EXEC, e_entry = `_start`) ; réserve : `rose/src/elf_linker.rs:1-5` délègue encore à `cc`/`ld` pour la voie LLVM | **Universel** : écriture binaire, layout non-PIE, stub d'entrée. **Spécifique** : adresses figées `0x40_0000`, syscall `exit` x86-64/aarch64, absence de linker script paramétrable | `noyau/invocation.bud` : description de l'arbre de sortie (adresse de charge, alignement, sections, point d'entrée, ABI). Remplacer les constantes en dur par des champs de manifeste |
| 2 | primitives matérielles (in/out, MMIO, asm inline) | **absent (comme brique exposée)** | Assembleur présent **uniquement** comme compiler-rt figé : `rose/runtime/x86_64.s:1-6`, `rose/runtime/aarch64.S:1-10`, 5 routines (`rose/src/runtime_natif.rs:50-56`). Syscalls codés en dur : `compilateur/emetteur_elf.rose:875-889` (`svc #0`, Linux aarch64). Aucun mot-clé `asm` dans le front-end : `rose/src/front.rs:169-176` | **Universel** : lecture/écriture volatile, barrières, port E/S. **Spécifique** : `inb/outb` x86, `svc #0` aarch64, numéros de syscalls Linux | `matériel/accès.bud` + primitives `lire_u8/ecrire_u8/lire_u32/ecrire_u32/barriere_*` paramétrées par `espace` (mémoire ou port) et par `largeur`/`boutisme` du manifeste |
| 3 | modèle d'interruptions (IDT, PIC/APIC, ABI handler save/restore) | **absent** | 0 occurrence de `IDT`, `APIC`, `table_des_interrupt`, `exception handler` dans le périmètre. Les `pic` trouvés sont des pics mémoire (`compilateur/gc_rose.rose:38`) — faux positifs écartés | **Universel** : table de vecteurs, convention de sauvegarde/restauration, masquage, priorités. **Spécifique** : IDT x86, PIC/APIC, GIC aarch64, vecteurs RISC-V | `noyau/interruptions.bud` : `contrôleur` (abstrait), `nombre_vecteurs`, `largeur_vecteur`, `abi_sauvegarde`. Aucun contrôleur nommé dans le compilateur |
| 4 | modèle mémoire (carte physique, pagination, translation, protection, allocateur déterministe) | **partiel** | Allocateur bump + file de récupération **en Rose** : `compilateur/gc_rose.rose:3,30-42` ; `gc_allouer`/`gc_liberer` en `compilateur/moteur_complet.rose:8563,8594`. Prototype explicite style Zig : `rose/src/mem_modele.rs:1-12,31-36,144,213` — **non câblé** (0 référence à `mem_modele` ailleurs dans `rose/src`). `mmap`/`munmap` no_std : `rose/src/sys_io.rs:23-27` | **Universel** : allocateur déterministe, arène, vues non possédantes, budget mémoire. **Spécifique** : carte physique, format de table de pages (x86-64 4 niveaux, aarch64 4 niveaux…), bits de protection matériels | `noyau/memoire.bud` : `largeur_page`, `niveaux`, `bits_attributs`, `carte` (liste de régions). L'allocateur reste indépendant de la pagination |
| 5 | sous-langage noyau (entiers bornés, pas d'alloc implicite, pas de GC, latence prévisible) | **partiel (incomplet)** | Rose n'a **pas** d'entiers bornés : `compilateur/verifieur.rose:287` (« L'entier Rose est à précision arbitraire »), mots-clés de type en `rose/src/front.rs:169-176` (`entier`, `rationnel`, `flottant`…). Le GC existe explicitement : `compilateur/gc_rose.rose:1-3` ; l'ELF émis contient un `g5_alloc_inline` (`compilateur/emetteur_elf.rose:1085`) et `+` **copie** les chaînes (`compilateur/emetteur_elf.rose:4654-4657`) — donc alloc implicite réel. La discipline G5 impose des règles de non-allocation dans le *code du compilateur* (`compilateur/gc_rose.rose:14-24`), pas une garantie du langage | **Universel** : types de largeur fixe, coût prévisible, absence d'allocation cachée. **Spécifique** : aucun ; c'est un choix de sémantique du langage | `profil/noyau.bud` : active un sous-ensemble (entiers `u8/u16/u32/u64` et `i8/…`, interdiction de `liste`/`table` dynamiques, GC désactivé, budget d'instructions). Vérifié par le vérificateur, pas par convention |
| 6 | runtime nu (memcpy/memset, panic, init .bss/pile, sans libc) | **partiel** | `memcpy`/`memset` freestanding générés par script : `scripts/rose-wasm-lib.sh:257-274` ; `#![no_std]` + `#[panic_handler]` générés : `scripts/rose-wasm-lib.sh:393-400` ; cible `wasm32-freestanding -fno-entry` : `scripts/rose-wasm-lib.sh:656`. Runtime natif = 5 routines machine (`rose/src/runtime_natif.rs:1-20,50-56`). **Réserves** : `rose/src/sys_io.rs:8-16` (le démarrage no_std n'est pas dans le module) et `rose/src/sys_io.rs:11-13` (`libc` embarque encore `std`). Aucune initialisation `.bss`/zéroillage de pile dans le stub : `rose/src/link_natif.rs:724,748` réserve seulement la pile | **Universel** : copie/remplissage, panic, zéroillage, setup de pile. **Spécifique** : cible wasm vs ELF, ABI de l'hôte | `runtime/base.bud` : liste de routines embarquées, politique de panic (`aborter`/`boucle`), taille de pile, sections à zéroiller. Routines écrites en Rose puis compilées avec le programme |
| 7 | framework de pilotes (disque, framebuffer, clavier — polling/IRQ) | **absent — et volontairement** | 0 occurrence de `framebuffer`, `ATA`, `AHCI`, `NVME` dans le périmètre. `clavier` n'apparaît que comme primitive `lire` refusée par le JIT (`rose/src/aot_llvm_orc.rs:1022`) ; `$vga` est une variable de vue (`compilateur/cli_rose.rose:3461`), pas un pilote. Le mot `pilote` y signifie « programme de test » : `compilateur/bigint_rose.rose:4`, `compilateur/compilateur.rose:3793` | **Universel** : contrat de pilote (init, polling, IRQ, tampon, erreurs), registre de pilotes. **Spécifique** : **tous** les exemples de la consigne (clavier, framebuffer, disque). Ces derniers doivent rester **hors compilateur** | `pilote/contrat.bud` : `mode` (`polling`/`irq`), `bus` (`mmio`/`port`/`dma`), `format_tampon`, `vecteur`. Le pilote clavier/framebuffer/disque s'écrit ensuite **en Rose** par l'utilisateur |
| 8 | format exécutable + ABI utilisateur (chargeur, syscalls, isolation ring 3) | **partiel** | Writers ELF64 ET_EXEC et PE32+ : `compilateur/emetteur_elf.rose:5733`, `compilateur/emetteur_pe_x64.rose:1-3`, `rose/src/link_natif.rs:170,305`. Chargeur/vérificateur de bytecode Rose : `compilateur/vm_rose.rose:1-24`, `rose/src/rosevm.rs:1-12`. Syscalls = ceux de l'**hôte** : `compilateur/ffi_syscalls.rose:13`, `compilateur/emetteur_elf.rose:4286-4299` (Linux aarch64), PE → `kernel32` (`rose/src/link_natif.rs:401`). **Aucun** ABI utilisateur Rose ni ring 3 (0 occurrence) | **Universel** : format d'image, table de syscalls, contrat d'appel, vérification à l'entrée. **Spécifique** : numéros de syscalls Linux/Win32, privilèges x86 | `abi/utilisateur.bud` : `format_image`, `table_syscalls` (noms + signatures), `niveaux_privilège`, `validation`. L'isolation est une propriété déclarée, pas un `if x86` dans l'émetteur |
| 9 | chaîne d'image amorçable (ISO/floppy) | **absent** | 0 occurrence de `ISO9660`, `isohybrid`, `mkisofs`, `xorriso`, `multiboot`, `MBR`, `GPT`, `GRUB`, ` BIOS `, `UEFI`, `secteur d'amorçage` dans le périmètre ; 0 fichier `*.iso` dans le dépôt | **Universel** : assemblage d'une image à partir de sections et d'un point d'entrée. **Spécifique** : ISO, disquette, El Torito, MBR/GPT — ce sont des *formats de transport* | `image/contour.bud` : `conteneur` (`brut`/`iso`/`fat`), `secteur`, `entrée`, `liste_fichiers`. Générique : Rose sait écrire des octets et une table, pas « faire un ISO » |

État global : **1 existe (partiel)**, **4 partiels** (#4, #5, #6, #8), **4 absents** (#2 comme brique exposée, #3, #7*, #9).
\* #7 est absent volontairement : c'est la consigne, pas un oubli.

### Ce qui existe déjà et sert de fondation (à ne pas confondre avec les briques)

- `compilateur/emetteur_elf.rose` — backend ELF écrit en Rose (pas un simple utilitaire).
- `compilateur/emetteur_pe_x64.rose` — backend PE32+ x86-64 écrit en Rose pur.
- `compilateur/bigint_rose.rose` — arithmétique en précision arbitraire écrite en Rose.
- `compilateur/vm_rose.rose` / `rose/src/rosevm.rs` — machine virtuelle Rose (RVM/JVM Rose).
- `rose/src/jit.rs` — JIT Cranelift (i64, booléens, flottants) avec repli interpréteur.
- `compilateur/moteur_complet.rose` (656 Ko) avec `rose/src/interp.rs` — moteur complet auto-hébergé.

Ce sont des **réalisations** ; les briques ci-dessus sont les **interfaces manquantes** qui les
rendent réutilisables hors de leur contexte d'origine.

---

## 2. Plan de briques génériques

### 2.1 Ordre de dépendance (le plus bas d'abord)

```
B1 invocation (layout de sortie)
   └─ B5 profil noyau (entiers bornés, pas d'alloc implicite)
         ├─ B2 accès matériel (volatilité, barrières, largeur, boutisme)
         │     └─ B3 interruptions (table de vecteurs, ABI save/restore)
         ├─ B6 runtime nu (memcpy/memset, panic, zéroillage, pile)
         └─ B4 mémoire (allocateur déterministe, puis pagination/protection)
               └─ B8 ABI utilisateur + chargeur/image
                     ├─ B7 contrat de pilote (polling/IRQ)
                     └─ B9 image amorçable (conteneur de transport)
```

Raison de l'ordre : `in/out` ou MMIO sur un `entier` de précision arbitraire n'a pas de sens
(pas de largeur de registre). Le **profil noyau** (B5) est donc le préalable à tout le reste.

### 2.2 Briques, pas recettes — traduction concrète

| Recette interdite (spécifique à un OS) | Brique universelle correspondante |
|---|---|
| `pilote_clavier_ps2()` | `pilote/contrat.bud` + `lire_octet(bus, registre)` |
| `framebuffer_vga_800x600()` | `matériel/zone.bud` (adresse, pas, format de pixel) + `ecrire_u32` |
| `init_idt_x86()` / `init_apic()` | `noyau/interruptions.bud` (nombre de vecteurs, ABI de sauvegarde) |
| `alloc_page_x86_64()` | `noyau/memoire.bud` (largeur de page, niveaux, bits d'attributs) |
| `syscall_linux_write()` | `abi/utilisateur.bud` (table de syscalls abstraite) |
| `faire_iso_bootable()` | `image/contour.bud` (conteneur, secteur, point d'entrée) |
| `#define outb` en dur | `matériel/accès.bud` (`espace: port|mémoire`, `largeur`, `boutisme`) |

### 2.3 Esquisses d'API (à valider, pas encore implémentées)

```rose
# B2 — primitives matérielles génériques (pas de nom de CPU)
fn lire_reg(adresse: entier, largeur: entier, boutisme: chaine) -> entier
fn ecrire_reg(adresse: entier, valeur: entier, largeur: entier, boutisme: chaine)
fn barriere(kind: chaine)             # "lecture" | "ecriture" | "complet"

# B3 — modèle d'interruptions générique
fn table_vecteurs(n: entier, largeur: entier) -> table
fn poser_handler(t: table, vecteur: entier, handler: entier)
fn activer(t: table)                    # ABI save/restore décrite dans le .bud

# B4 — allocateur déterministe (indépendant de la pagination)
fn arena_creer(capacite: entier) -> table
fn arena_allouer(a: table, n: entier) -> entier
fn arena_liberer_tout(a: table)

# B6 — runtime nu
fn cpier(dst: entier, src: entier, n: entier)
fn remplir(dst: entier, octet: entier, n: entier)
fn paniquer(code: entier)               # politique définie par runtime/base.bud

# B7 — contrat de pilote (jamais un pilote concret)
fn pilote_declarer(nom: chaine, mode: chaine, bus: chaine, vecteur: entier) -> table
fn pilote_sondage(p: table) -> entier
```

Ces signatures utilisent provisoirement `entier` : elles **doivent** être restreintes aux types
bornés du profil noyau (B5) avant d'être figées.

### 2.4 La convention `.bud`

Voir `briques/README.md`. En résumé :

- `.rose` = **code** Rose ; `.bud` = **déclaration** lisible par machine (cible, briques, options).
- `.bud` joue le rôle de `build.zig` + `build.zig.zon` : **un seul fichier** décrit la cible,
  les briques à compiler avec le programme et leurs dépendances.
- Aucune logique dans un `.bud` : pas de `si`, pas de boucle — seulement des champs typés.
  La logique de construction reste dans le compilateur, écrite en Rose.
- Le compilateur lit le `.bud` **avant** l'analyse : le profil (noyau/utilisateur), la largeur
  des entiers, l'ordre des octets et l'ABI en découlent. C'est ce qui permet un seul backend
  pour toutes les cibles.
- Le dépôt ne contient aujourd'hui **aucun** fichier `.bud` ni aucune mention de cette
  extension (recherche : 0 correspondance) : la convention est nouvelle, à implémenter.

### 2.5 Garanties à exiger avant d'appeler une brique « finie »

1. **Compilation autonome** : la brique compile par `rose aot`/C0 sans gcc, ld ni libc ( modèle
   `rose/src/link_natif.rs:3-5`).
2. **Aucun nom de matériel dans le compilateur** : `grep -i 'apic\|framebuffer\|isa\|nvme\|idt'`
   dans `compilateur/` et `rose/src/` doit rester à 0 (hors documentation).
3. **Coût mesuré** : latence et empreinte annoncées dans le `.bud` sont vérifiées par un test,
   pas supposées.
4. **Réversibilité** : le `.bud` peut être remplacé par un autre sans modifier une ligne de
   code Rose de la brique.
5. **Précision des entiers** : toute primitive matérielle est typée en largeur explicite ; un
   `entier` illimité est refusé dans le profil noyau.

---

## 3. Ce qu'il ne faut PAS faire

- Ajouter `inb`/`outb` comme **fonctions natives du moteur** : ce sont des briques `matériel/accès`,
  pas des primitives de l'interpréteur.
- Coder un `IDT` dans `emetteur_elf.rose` : l'émetteur écrit des octets ; la table de vecteurs
  est une structure de données décrite par `noyau/interruptions.bud`.
- Spécialiser `memcpy` par architecture dans le JIT : `rose/src/jit.rs` doit rester sur les
  scalaires et déléguer le reste (comme il le fait déjà, `rose/src/jit.rs:3-6`).
- Confondre les syscalls **hôte** déjà présents (`ffi_syscalls.rose`) avec un ABI **utilisateur**
  Rose : les premiers servent à faire tourner Rose *sur* Linux/Windows, les seconds à faire
  tourner des programmes *sur* Rose.
- Prendre les routines asm de `rose/runtime/` pour un mécanisme extensible : ce sont un
  compiler-rt figé, embarqué en octets (`rose/src/runtime_natif.rs:9-14`), pas une interface.

---

## 4. État des lieux factuel — résumé exécutif

Le compilateur Rose sait déjà produire des exécutables **statiques, non-PIE, sans linker ni libc
externes** (brique 1 largement acquise), écrit en Rose (ELF, PE), avec un allocateur
déterministe en Rose (brique 4 partielle) et un runtime no_std vérifié (brique 6 partielle).

Il manque surtout **la couche d'abstraction** : des types bornés (5), des primitives
d'accès matériel génériques (2), un modèle d'interruptions (3), un contrat de pilote (7), un
ABI utilisateur isolé (8) et un conteneur d'image (9). Aucune de ces manques ne nécessite un
pilote concret : tous se résolvent par une **brique paramétrée par un `.bud`**.

Le chantier minimal pour « Rose comme Zig » est donc : **B5 → B2 → B6 → B1(paramétrage) →
B4 → B3 → B8 → B7 → B9**, dans cet ordre.
