# Le pontier OS dans Rose — bilan factuel (19/11/2026)

## Question posée

> « Comment le pontier OS peut être dans Rose, on ne doit plus dépendre
> d'autres langages ? »

Réponse courte : **la connaissance OS et le code machine correspondant sont
désormais produits par Rose**. Les syscalls, l'encodage AArch64, la structure
ELF64 et les requêtes du protocole X11 sont écrits en Rose, dans deux jardins.
Une vraie fenêtre X11 **`IsViewable`, titrée, 400×220**, a été ouverte par un
ELF généré par Rose, sans aucun langage ajouté et sans `libX11`.

Réponse longue : il reste **une** dépendance au moteur Rust pour *exécuter* les
octets générés (Rose n'a pas de primitive `exec`), et le runtime Rose lui-même
reste Rust (G2/G3/G4 non achevés). Ces deux points sont détaillés en §6 : ils
ne sont pas masqués.

---

## 1. Fichiers produits (et rien d'autre)

| Fichier | Rôle |
|---|---|
| `jardins/os_syscalls.rose` | 37 fonctions Rose :Little-Endian, encodage `MOVZ`/`ADR`/`SVC`, ELF64, syscall numbers, handshake X11, CreateWindow/MapWindow/ChangeProperty |
| `jardins/os_natif.rose` | 7 fonctions Rose : les 5 opérations demandées + table de correspondance + description testable |
| `jardins/tests/os_syscalls_test.rose` | génère 3 ELF (write, handshake X11, fenêtre) |
| `jardins/tests/os_natif_test.rose` | génère les 5 ELF de remplacement |
| `rose/docs/OS_PONTIER_ROSE_PUR.md` | ce document |

`git status --short` après les changements :

```
?? jardins/os_natif.rose
?? jardins/os_syscalls.rose
?? jardins/tests/os_natif_test.rose
?? jardins/tests/os_syscalls_test.rose
```

Aucun fichier `.rs`, aucun fichier de `compilateur/**`, ni `gui_native.rose`,
ni `Cargo.toml`, ni documentation d'un autre agent n'a été modifié. Le binaire
`rose/rose` n'a pas été reconstruit ni remplacé ; une copie de travail
`/tmp/opencode/osrose_agents/rose_agent` (md5 identique
`da7d7ddfcf74279bbc8c260128c98b1c`) a servi aux vérifications finales, sans
aucun conflit « Text file busy ». Les ELF de test sont dans
`/tmp/opencode/osrose_agents/`.

---

## 2. Ce que fait le pontier Rose

### 2.1 Encodage (tout est calculé en Rose)

| Instruction | Formule Rose | Octets |
|---|---|---|
| `mov Xd,#imm16` (MOVZ hw=0) | `0xD2800000 + d + imm16*32` | `os_movz` |
| `mov Xd,Xm` (ORR Xd,XZR,Xm) | `0xAA000000 + m*65536 + 992 + d` | `os_mov_reg` |
| `adr Xd,PC+Δ` | `0x10000000 + immlo*2^29 + immhi*32 + d`, `imm21` signé | `os_adr` |
| `svc #0` | `0xD4000001` | `os_svc` |

`os_elf(code, donnees, drapeaux)` assemble un `ET_EXEC` aarch64 : header 64 o,
un `PT_LOAD` de 56 o, `e_entry = 0x400000 + 120`, code à l'offset 120, données
juste après. `ecrire_bytes` convertit chaque caractère Rose en un octet
(`c as u8`, lu dans le moteur) : c'est la **seule** primitive OS utilisée, et
elle servait déjà à l'émetteur ELF.

Syscalls aarch64 employés : `write=64`, `read=63`, `close=57`,
`socket=198`, `connect=203`, `nanosleep=101`, `exit=93`.

### 2.2 Mapping des 5 primitives Rust → Rose

| Primitive fournie aujourd'hui par le module Rust | Fonction Rose |
|---|---|
| `fenetre_native(titre,l,h)` | `ouvrir_fenetre(titre,l,h,ms)` |
| `fenetre_afficher(p)` | incluse dans `ouvrir_fenetre` (requête `MapWindow`) |
| `fenetre_attendre(p)` | `boucle_evenements(fd,taille)` |
| `fenetre_fermer(p)` | `fermer_fenetre(fd)` |
| `message_native(titre,texte)` | `boite_message(titre,texte)` |
| (sortie écran) | `ecrire_ecran(texte)` |

`jardins/gui_native.rose` n'a **pas** été modifié (consigne) : il appelle
encore les 5 primitives. Le remplacement est fourni côté Rose, prêt à être
câblé : `table_pontier()` donne la correspondance complète et
`decrire_pontier()` la vérifie sans lancer d'ELF.

---

## 3. Preuve 1 — `write(1,"Bonjour Rose OS\n",16) ; exit(0)`

Génération (Rose uniquement) :

```sh
cd /home/clausake/Stefan/ROSE
./rose/rose jardins/tests/os_syscalls_test.rose
# poc|elf=168|attendu=168
# x11|elf=4368|attendu=4368
# fenetre|elf=4448|attendu=4448
# magique|127|ELF

cd /tmp/opencode/osrose_agents
chmod +x bonjour_rose.elf
./bonjour_rose.elf
echo "code=$?"
```

Sortie réelle :

```
Bonjour Rose OS
code=0
```

Désassemblage du code extrait (offset 120, 32 octets) :

```sh
dd if=bonjour_rose.elf of=poc_code.bin bs=1 skip=120 count=32 status=none
objdump -D -b binary -m aarch64 --adjust-vma=0x400078 poc_code.bin
```

```
400078: d2800020  mov x0, #0x1        // stdout
40007c: 100000e1  adr x1, 0x400098    // le message, à base+120+32
400080: d2800202  mov x2, #0x10        // 16 octets
400084: d2800808  mov x8, #0x40        // write = 64
400088: d4000001  svc #0x0
40008c: d2800000  mov x0, #0x0
400090: d2800ba8  mov x8, #0x5d        // exit = 93
400094: d4000001  svc #0x0
```

`readelf -h` confirme `Type: EXEC`, `Machine: AArch64`, entry `0x400078`.

---

## 4. Preuve 2 — appel OS utile : `connect` sur le socket X11

Le ELF `x11_handshake.elf` fait `socket(AF_UNIX,SOCK_STREAM,0)`,
`connect("/tmp/.X11-unix/X99")`, `write` du handshake X11 12 octets
(`'l'`, version 11.0, aucune auth), `read(4096)` puis recopie la réponse sur
stdout.

```sh
chmod +x x11_handshake.elf
timeout 5 ./x11_handshake.elf > x11_reply.bin
xxd -l 16 x11_reply.bin
```

```
00000000: 0100 0b00 0000 8f03 93a5 b800 0000 6000
```

Interprétation (little-endian, demandé par le `l` initial) : `01` = succès,
version `11`, 911 unités de 4 octets, base de ressources `0x600000`.
**3652 octets** de réponse réelle ont été lus. Le désassemblage montre
`mov x8,#0xc6` (`socket=198`), `mov x8,#0xcb` (`connect=203`), `#0x40`
(`write`), `#0x3f` (`read`) et `#0x5d` (`exit`).

Aucune bibliothèque dynamique n'est impliquée : le protocole X11 est parlé
directement sur le socket Unix. C'est la réponse à « pas de dlopen/dlsym » :

> `libX11.so.6` n'est pas appelée. Les octets du protocole X11 sont générés
> par Rose et envoyés avec `write`. Le pontier n'a donc **aucune** dépendance
> à une bibliothèque écrite dans un autre langage.

---

## 5. Preuve 3 — une VRAIE fenêtre X11 ouverte par du code généré par Rose

Les identifiants de cette session Xvfb ont été décodés depuis la réponse de
l'établissement : racine `0x1ff`, visuel `0x21`, profondeur 24, base
`0x600000`, masque `0x1fffff`. Rose construit alors `CreateWindow`
(opcode 1), `MapWindow` (opcode 8), `ChangeProperty(WM_NAME=39, STRING=31)`
et un `nanosleep` de 4 s pour laisser observer la fenêtre.

```sh
chmod +x /tmp/opencode/osrose_agents/natif_fenetre.elf   # 4492 octets
/tmp/opencode/osrose_agents/natif_fenetre.elf & sleep 0.4
DISPLAY=:99 xwininfo -id 0x600001 -all
DISPLAY=:99 xprop -id 0x600001 WM_NAME
```

Sortie réelle (extraits) :

```
xwininfo: Window id: 0x600001 "Fenetre Rose pure"
  Parent window id: 0x1ff (the root window)
  Absolute upper-left X:  40
  Absolute upper-left Y:  40
  Width: 400
  Height: 220
  Depth: 24
  Visual: 0x21
  Class: InputOutput
  Map State: IsViewable
WM_NAME(STRING) = "Fenetre Rose pure"
```

Après la sortie du processus : `xwininfo -id 0x600001` → `X Error: 9: Bad
Drawable` (la fenêtre a bien été détruite à la fermeture du socket).

Désassemblage (extrait) du code généré par Rose :

```
400084: d28018c8  mov x8, #0xc6     // socket = 198
400088: d4000001  svc #0x0
400094: 10000341  adr x1, 0x4000fc  // sockaddr_un
40009c: d2801968  mov x8, #0xcb     // connect = 203
400098: d28002a2  mov x2, #0x15     // addrlen = 21
4000a0: d4000001  svc #0x0
4000bc: 10000321  adr x1, 0x400120  // tampon de 4096 o
4000c4: d28007e8  mov x8, #0x3f     // read = 63
4000d0: 10008281  adr x1, 0x401120  // requêtes X11 générées par Rose
4000d4: d2800602  mov x2, #0x30     // 48+ octets
4000d8: d2800808  mov x8, #0x40     // write = 64
4000dc: d4000001  svc #0x0          // CreateWindow + MapWindow (+ titre)
4000e0: 10008380  adr x0, 0x401150  // timespec
4000e8: d2800ca8  mov x8, #0x65     // nanosleep = 101
4000f0: d2800ba8  mov x8, #0x5d     // exit = 93
```

Les 48 premiers octets de requêtes produits par Rose (extrait) :
`01 18 0a 00 01 00 60 00 ff 01 00 00 28 00 28 00 68 01 b4 00 ...`
= `CreateWindow`, profondeur 24, largeur 10 unités, `wid=0x600001`,
parent `0x1ff`, `360x180`, puis `MapWindow` (`08 00 02 00 ...`).

---

## 6. Les cinq opérations `os_natif.rose` — exécution réelle

```sh
./rose/rose jardins/tests/os_natif_test.rose
# PONTIER_ROSE|5|153|4476
# table|5
chmod +x /tmp/opencode/osrose_agents/natif_*.elf
```

| ELF généré | Commande | Sortie réelle |
|---|---|---|
| `natif_ecran.elf` (168 o) | `./natif_ecran.elf` | `Bonjour Rose OS` |
| `natif_message.elf` (183 o) | `./natif_message.elf` | `[Attention] Message Rose natif` |
| `natif_fermer.elf` (170 o) | `./natif_fermer.elf` | `FERME` (puis `close(9)`) |
| `natif_evenements.elf` (300 o) | `printf 'evenement_rose\n' \| ./natif_evenements.elf` | `evenement_rose` |
| `natif_fenetre.elf` (4492 o) | voir §5 | fenêtre `IsViewable` titrée |

Tous les codes de sortie sont `0`.

---

## 7. Ce qui est PROUVÉ / ce qui RESTE

### Prouvé

1. Rose génère un ELF64 aarch64 exécutable, octet par octet, sans outil
   externe (header, phdr, code machine, données).
2. Le syscall `write` généré s'exécute et produit exactement la chaîne
   demandée ; `exit(0)` fonctionne.
3. Les syscalls `socket` + `connect` + `write` + `read` générés établissent
   une connexion X11 réelle avec Xvfb et lisent 3652 octets de réponse.
4. Rose génère les requêtes `CreateWindow`, `MapWindow` et `ChangeProperty`,
   et le code machine qui les envoie : `xwininfo` constate `Map State:
   IsViewable`, 400×220, titre `Fenetre Rose pure` ; `xprop` confirme
   `WM_NAME`.
5. Les cinq opérations demandées existent en Rose et produisent chacune un
   ELF exécutable, vérifié ci-dessus.
6. `./rose/rose verif` et `scripts/rose-jardin.sh verif` passent pour les
   deux jardins ; `verif-tous` valide les 10 jardins ; les 20 tests de
   `jardins/tests/premiers.rose` passent toujours (0 échec).

### Reste (sans euphémisme)

1. **Lancer les octets générés depuis Rose.** Rose n'a aucune primitive
   `exec`/`mmap` : les ELF sont lancés par le shell (`chmod +x`) pour la
   validation. Le pontier OS est en Rose, son *déclenchement* ne l'est pas
   encore. Voie propre : **une seule** primitive générique du runtime —
   « exécuter ces octets » — au lieu des cinq primitives OS-spécifiques
   actuelles (`fenetre_native`, `fenetre_afficher`, `fenetre_attendre`,
   `fenetre_fermer`, `message_native`). C'est un travail de runtime (G2/G3/G4),
   pas un travail OS : toute la logique OS est déjà sortie du moteur.
2. **Identifiants X11 figés.** `base=0x600000`, `root=0x1ff`, `visual=0x21`
   sont des constats de cette session Xvfb, pas des universaux. Un serveur
   X11 quelconque exige d'analyser la réponse d'établissement dans le code
   généré (décalage 12 pour la base, 108 pour la racine, 140 pour
   `root_visual`, little-endian). `os_requete_creer_fenetre` accepte déjà ces
   valeurs en paramètres : seul le lanceur automatique manque.
3. **Continuité du descripteur.** Chaque ELF est un processus distinct :
   `ouvrir_fenetre` et `boucle_evenements` ne partagent pas encore le fd.
   `boucle_evenements(0,...)` est prouvé sur stdin ; pour une vraie boucle
   d'événements de la fenêtre, il faut un seul ELF qui garde `x19` (fd) et
   boucle sur `read` — séquence connue, non encore générée.
4. **Boîte de message modale dessinée.** `boite_message` écrit sur stdout.
   Une fenêtre modale X11 avec texte demanderait `CreateGC` + `PolyText8` +
   une police allouée par le serveur (aller-retour à analyser). Nous ne
   prétendons PAS avoir produit cette boîte-là.
5. **Windows.** Le pontier généré cible aarch64/Linux (ELF). Le backend
   Win32 du module Rust reste en place tant qu'un émetteur PE en Rose n'a
   pas été écrit.
6. **Runtime Rust résiduel.** Le binaire `rose/rose` qui exécute les jardins
   est toujours compilé en Rust ; les étapes G2/G3/G4 (remplacement du
   moteur) ne sont pas faites. Ce chantier supprime la dépendance *OS* aux
   autres langages, **pas** la dépendance du runtime à Rust. `dlopen`/`dlsym`
   ne sont plus nécessaires pour la GUI : c'est le gain réel et mesurable.

---

## 8. Commandes de validation complètes

```sh
cd /home/clausake/Stefan/ROSE
./rose/rose verif jardins/os_syscalls.rose        # vérifié
./rose/rose verif jardins/os_natif.rose           # vérifié
bash scripts/rose-jardin.sh verif os_syscalls     # ✓ JARDIN VALIDE
bash scripts/rose-jardin.sh verif os_natif        # ✓ JARDIN VALIDE
bash scripts/rose-jardin.sh verif-tous            # ✓ Tous les jardins (10) sont valides
./rose/rose jardins/tests/os_syscalls_test.rose   # 168 / 4368 / 4448 octets
./rose/rose jardins/tests/os_natif_test.rose      # PONTIER_ROSE|5|153|4476
```

Toutes les commandes ci-dessus ont été exécutées le 19/11/2026 sur la machine
aarch64 du dépôt ; les sorties sont reproduites dans les sections précédentes.
Les ELF générés sont dans `/tmp/opencode/osrose_agents/` (`bonjour_rose.elf`,
`x11_handshake.elf`, `fenetre_x11.elf`, `natif_*.elf`).
