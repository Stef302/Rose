# Images du prototype d'OS en Rose (100 % Rose)

Constat daté : 26/11/2026. Quatre visuels générés par le Studio
(`tools.studio.queue_still`, modèle FLUX3-K Fast, 1280×720) pour illustrer
le prototype d'OS écrit entièrement en Rose (`ROSE/noyau.rose` + les 9
briques) et le noyau amorçable (`ROSE/os-rose.rose` qui produit
`/tmp/os-rose.elf`, ELF32 i386 multiboot v1, vérifié sous qemu). Aucun
nom de CPU, d'OS ni de format n'est codé en dur dans la logique des
briques : tout vient des `.bud` déclaratifs.

Les images vivent dans la bibliothèque du Studio (DGX SPARK) et sont
récupérables par leurs chemins relatifs via `studio.list_library`.

## 1. Terminal Rose OS (prototype)

- Chemin : `scouts/2026-11/2026-11-26_071523_a-retro-futuristic-terminal-screen-on_7a1e50/2026-11-26_071523_a-retro-futuristic-terminal-screen-on_7a1e50_s0.png`
- Contact sheet : `scouts/2026-11/2026-11-26_071523_a-retro-futuristic-terminal-screen-on_7a1e50/sheet.jpg`
- Job : `1126-071523-7a1e50` (statut `done`, 57,2 s)
- Taille : 1 303 189 octets
- Sujet : écran terminal rétro-futuriste rose, texte monospace vert
  défilant avec les entiers du prototype
  (`0 64 2000000006 0 -32768 32767 1 3 1 2 15`),
  en-tête « ROSE OS 1.0 — 108/108 verts », scanlines CRT, accent rose.

## 2. Architecture des 9 briques

- Chemin : `scouts/2026-11/2026-11-26_071523_an-isometric-architecture-diagram-of-a_6d11e4/2026-11-26_071523_an-isometric-architecture-diagram-of-a_6d11e4_s0.png`
- Contact sheet : `scouts/2026-11/2026-11-26_071523_an-isometric-architecture-diagram-of-a_6d11e4/sheet.jpg`
- Job : `1126-071523-6d11e4` (statut `done`, 30,0 s)
- Taille : 1 453 153 octets
- Sujet : diagramme isométrique des neuf briques universelles
  (interruptions, mémoire, pilotes, ABI, image, profil, runtime, backend,
  qubit) en cristaux roses translucides, étiquettes françaises, lignes de
  données lumineuses, fond sombre avec grille rose.

## 3. Splash Rose OS

- Chemin : `scouts/2026-11/2026-11-26_071523_a-minimalist-splash-screen-for-rose_5c1c74/2026-11-26_071523_a-minimalist-splash-screen-for-rose_5c1c74_s0.png`
- Contact sheet : `scouts/2026-11/2026-11-26_071523_a-minimalist-splash-screen-for-rose_5c1c74/sheet.jpg`
- Job : `1126-071523-5c1c74` (statut `done`, 27,6 s)
- Taille : 752 119 octets
- Sujet : écran de démarrage minimaliste, rose stylisée faite de traces de
  circuits et de code, fond noir, sous-titre monospace
  « 100 % ROSE — 9 briques universelles », lueur rose douce.

## 4. Noyau amorçable (qemu)

- Chemin : `scouts/2026-11/2026-11-26_074515_a-black-terminal-screen-showing-a_0f86ac/2026-11-26_074515_a-black-terminal-screen-showing-a_0f86ac_s0.png`
- Contact sheet : `scouts/2026-11/2026-11-26_074515_a-black-terminal-screen-showing-a_0f86ac/sheet.jpg`
- Job : `1126-074515-0f86ac` (statut `done`, 28,3 s)
- Sujet : écran noir, mode texte VGA, « Rose OS » en haut à gauche avec
  curseur clignotant — ce que qemu affiche réellement après
  `Booting from ROM..Rose OS`.

## 5. Noyau avec prompt interactif (qemu)

- Chemin : `scouts/2026-11/2026-11-26_093342_a-black-terminal-screen-in-vga_c9f141/2026-11-26_093342_a-black-terminal-screen-in-vga_c9f141_s0.png`
- Contact sheet : `scouts/2026-11/2026-11-26_093342_a-black-terminal-screen-in-vga_c9f141/sheet.jpg`
- Job : `1126-093342-c9f141` (statut `done`, 29,9 s)
- Sujet : écran noir VGA, bannière 5 lignes + `Booting from ROM..OK` +
  prompt `> ` avec curseur clignotant — noyau Rose interactif (polling PS/2).

## 5. Shell interactif (qemu)

- Chemin : `scouts/2026-11/2026-11-26_095452_black-screen-in-vga-text-mode_5409fb/2026-11-26_095452_black-screen-in-vga-text-mode_5409fb_s0.png`
- Contact sheet : `scouts/2026-11/2026-11-26_095452_black-screen-in-vga-text-mode_5409fb/sheet.jpg`
- Job : `1126-095452-5409fb` (statut `done`, 28,4 s)
- Sujet : écran noir VGA, bannière « Rose OS 1.0 - 100% Rose » + 4 lignes de briques, « Booting from ROM..OK », curseur clignotant après le prompt — noyau Rose interactif en boucle de lecture clavier PS/2 avec echo série.

## Références croisées

- Prototype logique : `ROSE/noyau.rose` (343 lignes, 109 assertions `>>`)
- Noyau amorçable : `ROSE/os-rose.rose` → `/tmp/os-rose.elf`
  (ELF32 i386, multiboot v1, 4268 octets, entrée 0x10100c)
- Assemblage prototype : `ROSE/scripts/noyau.sh`
- Lancement noyau : `ROSE/scripts/os-rose.sh`
- Sortie vérifiée prototype : 108 entiers, md5 `70a841b80d1ddf51c8ad61edfa4a4cb8`,
  parité `interprete`/`jit`/`comp`/`verif` à l'identique, `verif` = « vérifié ».
- Noyau vérifié : `qemu-system-x86_64 -kernel /tmp/os-rose.elf -nographic`
  affiche `Booting from ROM..Rose OS` (testé i386 + x86_64).
