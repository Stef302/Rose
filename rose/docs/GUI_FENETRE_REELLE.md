# GUI native : une VRAIE fenêtre OS, sans navigateur ni dépendance

> **Constat daté au 19/11/2026**, Rose 0.1.5, dépôt à HEAD `d20bed0`.
> Ce document rapporte ce qui a été **réellement exécuté et observé**, pas une
> intention. Aucun fichier verrouillé (`main.rs`, `lib.rs`, `front.rs`,
> `interp.rs`, `jardins/**`) n'a été modifié dans le dépôt. Aucun commit, aucune
> publication, aucun binaire du dépôt remplacé.

## 1. La demande et la réponse courte

Demande : « le gui je veux pas html vraie fenêtre comparé aot jit ». Le jardin
`jardins/gui.rose` produit un document HTML rendu par un navigateur. Le
demandeur veut une fenêtre du système d'exploitation.

Ce qui est livré par ce chantier :

| Fichier | Rôle |
|---|---|
| `rose/src/gui_native.rs` | module Rust autonome, zéro crate externe : Win32 + X11 |
| `rose/docs/gui_native.patch` | patch **non appliqué** qui câble 5 primitives à la stdlib Rose |
| `rose/docs/GUI_FENETRE_REELLE.md` | ce document |
| `/tmp/opencode/jc_agents/gui_native.rose` | jardin proposé (hors dépôt, `jardins/**` appartient à un autre agent) |
| `/tmp/opencode/jc_agents/gui_native_demo.rose` | démo du jardin proposé |
| `/tmp/opencode/jc_agents/target/release/rose` | binaire local construit pour ce chantier (`rose/rose` **non remplacé**) |

**Fait marquant** : une vraie fenêtre X11 a été ouverte et **observée de
l'extérieur** sur cette machine, sous un serveur Xvfb démarré pour l'occasion.
Ce n'est pas une simulation, pas du HTML, pas un « si jamais ça marcherait ».

## 2. Ce qui est réellement disponible sur cette machine

Mesures du 19/11/2026 (à re-mesurer : un état de service est un constat daté) :

```
uname -m                       -> aarch64
echo $DISPLAY                  -> (vide)      $WAYLAND_DISPLAY -> (vide)
ldconfig -p | wc -l            -> 0            <-- cache ldconfig VIDE, piège
find / -name 'libX11.so*'      -> /usr/lib/aarch64-linux-gnu/libX11.so.6.4.0
                                  + libX11.so.6, libX11.so, libXext.so*
which Xvfb Xorg xterm xwininfo -> /usr/bin/Xvfb, /usr/bin/Xorg, /usr/bin/xterm,
                                  /usr/bin/xwininfo (aussi xprop, xdpyinfo, xset)
```

⚠ **Piège documenté** : `ldconfig -p | grep libX11` ne renvoie RIEN (cache
vide, 0 entrée), mais `libX11.so.6` est bien présent et `Xvfb` y est lié
(`ldd /usr/bin/Xvfb` montre `libX11.so.6 => /lib/aarch64-linux-gnu/libX11.so.6`).
Conclure « pas de X11 » de ce seul `ldconfig` serait faux.

Il n'y a **pas de session graphique active** au départ. J'ai démarré un serveur
virtuel :

```
Xvfb :99 -screen 0 1280x1024x24 -nolisten tcp     # pid 3595216
DISPLAY=:99 xdpyinfo  ->  X.Org 21.1.11, 1280x1024, écran actif
```

Ce serveur a été laissé tourner pour la reproductibilité des preuves
(`kill 3595216` pour l'arrêter). C'est un serveur **virtuel sans fenêtre
visuelle pour un humain** : la fenêtre X11 existe bien dans le serveur, elle
n'est pas affichée sur un écran physique. C'est la limite exacte de la preuve
sur cette machine.

## 3. La voie native choisie et pourquoi

| Voie | Verdict | Raison |
|---|---|---|
| **Win32 `user32.dll` en `extern "system"`** | **retenue pour Windows** | `user32.dll` fait partie de Windows : aucun crate, aucune DLL à livrer. C'est exactement ce que fait déjà `rose/src/setup_gui.rs` (fenêtre de l'installateur), donc la voie est éprouvée dans ce dépôt. |
| **X11 `libX11.so.6` chargée par `dlopen`/`dlsym`** | **retenue pour UNIX** | le binaire n'a **aucun `-lX11` au build** ; il tourne même sur une machine sans X11 et renvoie une erreur explicite. Sur cette machine, X11 est présent et utilisable sous Xvfb. |
| GTK / Qt / winit / tao | rejeté | crate externe + bibliothèques à livrer, contraire à « sans dépendance ». |
| `xmessage` / `zenity` appelés en sous-processus | rejeté | dépendance à un programme externe ; la boîte de message X11 est donc implémentée en pur Xlib (`XDrawString`). |
| HTML (jardin actuel) | conservé comme **repli documenté** | toujours utile sur un moteur non patché ou sans affichage. |

Le module choisit automatiquement le backend à la compilation (`#[cfg(windows)]`
/ `#[cfg(not(windows))]`). Sur UNIX, la détection de l'affichage est **au
moment de l'appel** : `DISPLAY` vide → `Erreur::PasDeDisplay`, sans ouvrir de
fenêtre et sans mentir.

## 4. Primitives réellement implémentées

Rust (`rose/src/gui_native.rs`, 100 % std, zéro crate ajouté) :

| Fonction Rust | Rôle |
|---|---|
| `fenetre_creer(titre, largeur, hauteur) -> Result<u64, Erreur>` | `CreateWindowExW` après `RegisterClassExW` / `XCreateSimpleWindow` ; enregistre la fenêtre dans un registre `Mutex<HashMap<u64, Fenetre>>` |
| `fenetre_afficher(id)` | `ShowWindow` / `XMapWindow` + `XSync` |
| `fenetre_attendre(id)` | `GetMessageW`/`DispatchMessageW` / boucle `XPending`+`XNextEvent` (touche, clic, `ClientMessage`) |
| `fenetre_fermer(id)` | `DestroyWindow` / `XDestroyWindow` + `XCloseDisplay`, retrait du registre |
| `message(titre, texte)` | `MessageBoxW` / fenêtre modale X11 native dessinée avec `XLoadQueryFont` + `XDrawString` |
| `diagnostic()`, `borner_dimension()`, `nettoyer_texte()`, `utf16_avec_nul()`, `compter_fenetres()`, `creer_simulee()` | diagnostic et helpers purs, testables sans écran |

Primitives Rose proposées par le patch (noms français) :

| Primitive Rose | Type | Correspondance |
|---|---|---|
| `fenetre_native(titre, largeur, hauteur)` | `chaine × entier × entier -> entier` | renvoie la poignée |
| `fenetre_afficher(poignee)` | `entier -> vide` | |
| `fenetre_attendre(poignee)` | `entier -> vide` | |
| `fenetre_fermer(poignee)` | `entier -> vide` | |
| `message_native(titre, texte)` | `chaine × chaine -> vide` | |

Détails d'implémentation vérifiés :

- dimensions bornées à `[1, 8192]` ; titre nettoyé (NUL retirés, 1024 caractères max) ;
- texte converti en UTF-16 terminé par NUL pour les API `...W` ;
- `XEvent` dimensionné à 256 octets (`xevent` réel = 192 octets sur 64 bits) ;
- une erreur est **toujours** renvoyée si le backend manque :
  `Rose : erreur — GUI natif : aucun affichage disponible (DISPLAY vide ou libX11 absente)` ;
- `ROSE_GUI_ATTENTE_MS` (test uniquement) borne `fenetre_attendre` en ms ;
  0 ou absent = attente infinie.

## 5. Ce qui a été PROUVÉ sur cette machine

### 5.1 Vraie fenêtre X11 observée de l'extérieur

Programme `/tmp/opencode/jc_agents/fenetre_preuve.rose` exécuté par le binaire
patché, avec `DISPLAY=:99` et `ROSE_GUI_ATTENTE_MS=7000`, puis interrogé par
`xwininfo` **depuis un autre processus** pendant que Rose attendait :

```
0x600001 "Preuve Rose GUI native": ()  420x180+40+40  +40+40

xwininfo -id 0x600001 :
  Window id: 0x600001 "Preuve Rose GUI native"
  Absolute upper-left X:  40
  Absolute upper-left Y:  40
  Width: 420
  Height: 180
  Map State: IsViewable
xprop -id 0x600001 WM_NAME :
  WM_NAME(STRING) = "Preuve Rose GUI native"
```

`Map State: IsViewable` est la preuve décisive : la fenêtre est **mappée et
visible dans le serveur X**. Le titre et les dimensions demandés correspondent
exactement à ceux observés.

Autres observations réelles :

- boîte de message : `0x600001 "Boite Rose": () 420x160+100+100` apparue sous
  Xvfb (processus interrompu par `timeout` après 3 s, car modale sans clic) ;
- **sans `DISPLAY`** : `rc=1`, erreur honnête ci-dessus, aucune fenêtre créée ;
- jardin proposé : `0x600001 "Jardin GUI natif": () 380x170+40+40`, démo
  terminée `rc=0`, repli HTML long de 178 caractères ;
- binaire autonome produit par `rose comp` : `0x600001 "Fenetre depuis rose comp":
  () 333x177+40+40` — le chemin embarqué ouvre aussi une vraie fenêtre.

### 5.2 Compilation et tests

Exécutés dans la copie isolée `/tmp/opencode/jc_agents/rose` (patch appliqué),
avec `CARGO_TARGET_DIR=/tmp/opencode/jc_agents/target` :

```
cargo build --release --bin rose   -> Finished `release` profile [optimized] in 8.89s
cargo t --release                  -> 195 passed; 0 failed   (bin rose, dont les 7 tests gui_native)
                                      0 passed;  0 failed
                                      6 passed;  0 failed
rustc --edition 2024 --test rose/src/gui_native.rs   -> 7 passed; 0 failed
rustc --edition 2024 --target x86_64-pc-windows-gnu \
      --crate-type lib rose/src/gui_native.rs        -> rc=0, 0 erreur, 0 warning
```

Validation du patch depuis la racine du dépôt (dry-run, aucun fichier touché) :

```
patch -p1 --dry-run < rose/docs/gui_native.patch
checking file rose/src/main.rs
checking file rose/src/lib.rs
checking file rose/src/front.rs
checking file rose/src/interp.rs
```

`rose/src/main.rs`, `lib.rs`, `front.rs`, `interp.rs` n'ont **pas** été modifiés
dans le dépôt : seul le fichier `gui_native.rs` (nouveau), le patch et ce
document y ont été écrits. Le patch a été appliqué uniquement sur la copie
isolée pour y prouver la compilation.

### 5.3 Ce qui RESTE à tester ailleurs — honnêteté

| Point | État réel |
|---|---|
| Fenêtre sur un **écran physique** | NON testé ici : Xvfb est un serveur virtuel. La fenêtre est prouvée dans le serveur X, pas visible par un humain. |
|Backend **Win32 à l'exécution** | NON testé : pas de Windows ici. Le module a été **compilé** pour `x86_64-pc-windows-gnu` sans erreur, mais `CreateWindowExW`, `MessageBoxW` et la boucle `GetMessageW` n'ont jamais tourné. À tester sur poste Windows. |
| Intégration avec un **gestionnaire de fenêtres** (décorations, `WM_DELETE_WINDOW`) | NON testé : Xvfb n'a pas de WM. La fermeture par `ClientMessage` (type 33) est gérée, mais le protocole `WM_PROTOCOLS` n'est pas négocié (pas de `XChangeProperty`). |
| `WM_CLASS` / icône de la fenêtre X11 | NON posés (`XCreateSimpleWindow` seul). `xlsclients` ne l'identifie donc pas. |
| Détection automatique du mode native/html dans Rose | NON implémentée : Rose n'a pas de `try`/`essayer`. Le jardin proposé demande le mode explicitement. |

## 6. AOT vs JIT — mesures réelles

Témoin `/tmp/opencode/jc_agents/bench_window.rose` : **24 vraies fenêtres X11**
créées, affichées et fermées (`fenetre_native` + `fenetre_afficher` +
`fenetre_fermer` en boucle). 9 échantillons **entrelacés**, processus épinglé
avec `taskset -c 9`, `DISPLAY=:99`, 2 échauffements non comptés.

### 6.1 Résultat principal (mesure finale, charge machine la plus basse)

| Exécution | Médiane | Min | Max |
|---|---|---|---|
| Moteur patché, voie par défaut (JIT Cranelift activé) | **6,674 ms** | 6,175 | 7,652 |
| Exécutable autonome `rose comp` (moteur embarqué) | **7,649 ms** | 7,197 | 8,752 |

Écart des médianes : **+0,975 ms (+14,6 %)** en faveur de la voie par défaut.

Échantillons (ms) — défaut : 6,18 6,76 6,38 6,95 6,32 7,30 6,62 7,65 6,67.
Échantillons (ms) — `comp` : 7,28 7,20 8,20 7,65 7,53 7,63 8,59 7,87 8,75.

### 6.2 Variabilité observée — à ne pas masquer

Trois séries de 9 échantillons ont été exécutées ; les valeurs absolues bougent
beaucoup selon la charge, l'écart relatif reste du même signe :

| Série | Contexte | Médiane défaut | Médiane `comp` | Écart |
|---|---|---|---|---|
| A | avant finalisation du module, charge faible | 3,520 ms | 4,622 ms | +31,3 % |
| B | 2 `rustc` concurrents (~370 % CPU, autre agent) | 12,832 ms | 16,233 ms | +26,5 % |
| C | 2 `rustc` concurrents | 11,990 ms | 14,592 ms | +21,7 % |
| **D (principale)** | module final, charge redescendue | **6,674 ms** | **7,649 ms** | **+14,6 %** |

Le signe est stable (l'exécutable autonome est plus lent), l'amplitude ne
l'est pas. Ce sont des **constats datés**, pas des specs : re-mesurer.

### 6.3 AOT LLVM ORC (`rose orc --link`) : NON MESURABLE ici

La chaîne LLVM AOT ne sait **pas compiler le nouveau primitif** :

```
$ rose orc bench_window.rose -o bench_window_aot --link
rose: échec AOT LLVM ORC : erreur d'analyse : Rose : erreur —
      [Ligne 4, Colonne 0] Fonction 'fenetre_native' inconnue
rc=1
```

Cause vérifiée, pas une spéculation : `rose/src/auto_llvm.rs` fige
`VERSION_HELPER = "v5.10.0"` et le helper présent est
`~/.cache/rose/rose-aot-v5.10.0` (2 102 320 octets). C'est un binaire
**plus ancien**, avec sa propre table de primitives : il ne connaît pas
`fenetre_native`. La chaîne AOT fonctionne par ailleurs sur un programme témoin
sans GUI (`rose orc simple_aot.rose --link` → binaire de 70 392 octets qui
affiche `49`), donc le problème est bien la table du helper, pas LLVM.

Une mesure AOT LLVM honnête exigerait de reconstruire le helper avec le patch,
ce qui n'a pas été fait ici (feature `llvm` non compilable sur cette machine,
et le cache du helper n'appartient pas à ce chantier). **Aucune mesure AOT
LLVM n'est présentée.**

⚠ `rose comp` n'est PAS de l'AOT LLVM : c'est un exécutable autonome qui
embarque le moteur et la source. Il est publié ici parce que la mission
l'autorisait explicitement comme alternative, et parce qu'il exécute le module
patché (fenêtre observée, voir 5.1).

Note : le binaire est lié statiquement (`+crt-static`) et l'éditeur de liens
avertit que `dlopen` nécessite les bibliothèques glibc disponibles à l'exécution.
Cet avertissement est **préexistant** dans Rose (FFI `@dll`) ; `dlopen` de
`libX11.so.6` a fonctionné dans toutes les mesures ci-dessus.

## 7. Le jardin proposé (hors dépôt)

`/tmp/opencode/jc_agents/gui_native.rose` — non ajouté à `jardins/**`, qui
appartient à un autre agent. API :

- `ouvrir_native(titre, largeur, hauteur) -> entier` (crée + affiche) ;
- `attendre_native(p)` (boucle d'événements + fermeture) ;
- `fermer_native(p)`, `dire_native(titre, texte)` ;
- `html_repli(titre, largeur, hauteur) -> chaine` (repli documenté) ;
- `choisir(titre, largeur, hauteur, mode) -> chaine`.

Vérifications réelles : `rose verif gui_native.rose` → `vérifié` ; démo
`gui_native_demo.rose` → vraie fenêtre `380x170` observée sous Xvfb, `rc=0`.

## 8. Limites du patch et travaux restants

1. Le patch doit être appliqué par l'agent qui possède `main.rs`/`lib.rs`/
   `front.rs`/`interp.rs`, puis le helper AOT doit être reconstruit pour que
   `rose orc` connaisse les primitives GUI.
2. Le module X11 ne pose pas `WM_CLASS`, ni les protocoles `WM_PROTOCOLS` :
   l'intégration avec un vrai gestionnaire de fenêtres reste à faire.
3. Le dessin X11 de la boîte de message est volontairement minimal (`fixed`,
   une ligne, pas de vrai bouton) : c'est un équivalent fonctionnel de
   `MessageBoxW`, pas une réplique graphique.
4. Aucune gestion de la fermeture par la croix n'est testée sans WM.
5. `fenetre_attendre` n'a qu'un seul événement d'arrêt générique (touche, clic
   ou `ClientMessage`) : pas de callbacks Rose (le langage n'a pas de
   fermetures stockables).

Aucun commit, aucune publication, aucun fichier du dépôt autre que les trois
fichiers annoncés n'a été modifié.
