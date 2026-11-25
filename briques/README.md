# `briques/` — bibliothèque de briques universelles Rose

> Ce dossier est un **squelette de convention**, pas un code compilable.
> Aucun de ses fichiers n'est aujourd'hui lu par le compilateur. Voir
> `rose/docs/BRIQUES_UNIVERSELLES.md` pour l'état des lieux des 9 briques visées.

## 1. Philosophie : des briques, pas des recettes

Zig ne fournit pas un « pilote clavier » : il fournit un compilateur qui sait écrire des octets
et une bibliothèque standard **compilée avec le programme**. Rose doit faire de même.

**Une brique décrit un mécanisme. Une recette décrit un appareil.**

| Brique (universelle, ici) | Recette (spécifique, dehors) |
|---|---|
| accès volatil à un registre, largeur et boutisme paramétrés | `outb` x86, `svc #0` Linux |
| table de vecteurs + ABI de sauvegarde | IDT/PIC/APIC, GIC, PLIC |
| allocateur déterministe, arène, budget | `mmap` Linux, `VirtualAlloc` Win32 |
| contrat de pilote (init / polling / IRQ / tampon) | pilote PS/2, framebuffer VGA, NVMe |
| format d'image + table d'appels abstraite | ISO El Torito, disquette MBR, syscall 64 |
| backend de sortie paramétré (adresse, alignement, entrée) | `_start` écrit en dur à `0x40_0000` |

**Règles du dossier**

1. Aucun nom d'architecture, d'OS, de bus ou de contrôleur dans le nom d'une brique.
   `lire_reg.rose` oui ; `lire_reg_apic.rose` non.
2. Aucun appareil dans le code d'une brique. Le matériel est **déclaré** dans un `.bud`.
3. Une brique est écrite en Rose pur, dans le sous-ensemble compilable par C0 / `rose aot`.
   Elle ne dépend ni du moteur Rust, ni de GMP, ni de libc, ni d'un linker externe.
4. Une brique est **incluse dans le programme** par le compilateur (modèle Zig : pas de shared
   runtime). Le programme paie uniquement les briques qu'il declare.
5. Le coût (instructions, octets, latence) est une propriété **mesurée** et déclarée dans le
   `.bud`, jamais une promesse en commentaire.
6. Le profil noyau d'une brique interdit l'allocation implicite et le GC : l'allocation est une
   brique explicite (`memoire/arena.rose`), pas un effet de bord de `+` ou de `liste`.
7. Une brique ne devient « finie » que lorsqu'elle compile sur **deux cibles distinctes** avec
   deux `.bud` différents, sans modification de son code source.

## 2. La convention `.bud`

`.bud` est l'extension des fichiers de **déclaration** Rose. Elle joue le rôle de
`build.zig` **et** de `build.zig.zon` réunis : un seul fichier, lisible par l'homme et par le
compilateur, dit quoi compiler, pour quelle cible, avec quelles briques et quelles dépendances.

- `.rose` = **code** (exécutable ou bibliothèque).
- `.bud` = **données de construction** (cible, briques, options, empreintes).
- Un `.bud` ne contient **aucune logique** : pas de `si`, pas de boucle, pas de fonction.
  Un champ manquant vaut la valeur par défaut du compilateur.
- Le compilateur lit le `.bud` **avant** l'analyse lexicale : il en déduit le profil
  (`noyau`/`utilisateur`), la largeur des entiers, le boutisme, l'ABI et la cible de sortie.
  C'est ce mécanisme — et non un `si architecture == x86` dans l'émetteur — qui rend un seul
  backend capable de viser n'importe quel appareil.

### 2.1 Grammaire (spécification, non encore implémentée)

Format texte, UTF-8, une paire `cle: valeur` par ligne, `#` = commentaire.
Types acceptés : `chaine` (guillemets), `entier`, `booleen` (`vrai`/`faux`),
`liste` (`[a, b, c]`), `table` (`{ cle: valeur, … }`).

```
nom: <chaine>                      # identifiant du projet / de la brique
version: <chaine>

cible: {
  architecture: <chaine>           # GENERIQUE | x86_64 | aarch64 | riscv64 | ...
  boutisme: <chaine>               # petit | grand
  largeur_mot: <entier>            # 8 | 16 | 32 | 64
  largeur_page: <entier>           # 0 = pas de pagination
  abi: <chaine>                    # nom de l'ABI abstraite choisie
}

profil: <chaine>                   # noyau | utilisateur | maths

materiel: {                        # description declarative, jamais de code
  regions: [ { base: <entier>, taille: <entier>, role: <chaine> } ]
  vecteurs: <entier>               # 0 = pas d'interruptions
  registre_largeur: <entier>       # largeur d'un registre de peripherique
}

briques: [
  "memoire/arena.rose",
  "materiel/acces.rose",
  "runtime/base.rose"
]

options: {
  freestanding: vrai               # aucun OS hôte, aucune libc
  pie: faux                        # sortie à adresse fixe
  gc: faux                         # aucune allocation implicite
  alloc_implicite: faux
  latence_max: <entier>            # cycles, 0 = non bornée
}

sortie: {
  format: <chaine>                 # brut | elf | pe
  entree: <chaine>                 # symbole `_start` de la brique invocation
  adresse_charge: <entier>
  alignement: <entier>
}

dependances: [                     # empreinte + chemin, comme un .zon
  { chemin: "briques/maths/bigint", empreinte: "<sha256>" }
]
```

Le format `brut` est le seul supporté aujourd'hui ; `elf`/`pe` sont les formats déjà écrits
par `emetteur_elf.rose` / `emetteur_pe_x64.rose`.

### 2.2 Exemple

Voir `exemple.bud` dans ce dossier. Il décrit un noyau générique, **sans** nommer de CPU ni
de contrôleur d'interruptions : le contrôleur est une propriété de la cible, pas du code.

### 2.3 Règles de nommage des fichiers

```
briques/
  README.md                 ce fichier
  exemple.bud               exemple de manifeste (spécification)
  <famille>/
    <brique>.rose           code Rose de la brique
    <brique>.bud            déclaration de cette brique (cible, options, coût)
    tests_<brique>.rose     tests, écrits en Rose
```

Familles prévues (une par mécanisme, aucune par appareil) :
`invocation/`, `materiel/`, `interruptions/`, `memoire/`, `profil/`, `runtime/`,
`pilote/`, `abi/`, `image/`.

## 3. Relation avec le code existant

Ce dossier ne remplace aucun fichier existant. Il donne une **interface** aux éléments déjà
présents :

- `compilateur/emetteur_elf.rose`, `compilateur/emetteur_pe_x64.rose` → famille `invocation/`
  (aujourd'hui paramétrée par des constantes en dur, à lire depuis le `.bud`) ;
- `compilateur/gc_rose.rose`, `rose/src/mem_modele.rs` → famille `memoire/` (prototype non câblé) ;
- `rose/runtime/{x86_64,aarch64}.s`, `scripts/rose-wasm-lib.sh` → famille `runtime/`
  (compiler-rt figé et shims générés, à remplacer par des routines Rose déclarées) ;
- `compilateur/vm_rose.rose`, `rose/src/rosevm.rs` → famille `abi/` (chargeur de bytecode Rose,
  pas encore ABI utilisateur) ;
- `compilateur/bigint_rose.rose` → famille `maths/` (brique déjà universelle, modèle à suivre).

## 4. Ce que ce dossier n'est pas

- Pas un catalogue de pilotes : aucun `clavier.rose`, `framebuffer.rose`, `disque.rose`.
- Pas un packaging d'exécutable : `rose comp` / `rose aot` restent les seules voies de sortie.
- Pas une dépendance externe : zéro crate, zéro wheel, zéro fichier binaire précompilé.
- Pas une promesse de compilation : tant que le compilateur ne lit pas `.bud`, ces fichiers
  sont de la documentation exécutable en attente de câblage.
