# GUI_MORE — fenêtre native « grands nombres » et bouton « Plus de chiffres »

> **Constat daté : 21/11/2026** (horodatage machine : 20/11/2026 ~11 h, la machine
> est réglée un jour en retard ; la mission annonce le 21/11). HEAD lu au départ :
> `af8164fd00d6de316bc6279eec0682d3e33a29e7`. Périmètre GUI1 **uniquement** :
> `rose/src/gui_native.rs`, `jardins/gui_native.rose`, ce document.
> Aucun commit, aucune publication, aucun autre fichier du dépôt modifié par
> GUI1. D'autres agents travaillent en parallèle (`interp.rs`, `run.rs`,
> `repl.rs`, `main.rs`, `jit.rs`) : leurs modifications ne sont ni attribuées ni
> revendiquées ici.
>
> Règle de vérité : **rien de ce qui est écrit ici n'est un succès d'interface
> simulé**. Un clic « réel » signifie ci-dessous un événement X transmis par le
> serveur X et relu via `XNextEvent` — pas une variable locale posée à `1`.

---

## 1. Verdict en une phrase

**La logique d'interface est implémentée et vérifiée en Rose ; les primitives
OS pour créer un bouton enfant et recevoir son clic sont implémentées et le
clic X11 est prouvé sous Xvfb ; mais le bouton n'est PAS encore cliquable depuis
un programme Rose, parce que le câblage de ces primitives dans la stdlib
(`interp.rs`) appartient à INT1 et m'est interdit.** Aucun succès de bouton
« depuis Rose » n'est revendiqué.

| Élément | État |
|---|---|
| Jardin Rose : fenêtre, champ lecture seule, bouton, 4 pas exclusifs, événement, `decrire()` enrichi | **Implémenté et vérifié** |
| Primitives Rust : `Bouton`, `boutons_creer`, `attendre_clic`, hit-test, registre | **Implémenté et testé** (hors écran + machine à états) |
| Bouton enfant X11 + réception d'un `ButtonPress` | **Implémenté et prouvé sous Xvfb** (aller-retour serveur X) |
| Bouton enfant Win32 + `WM_COMMAND` | **Écrit, jamais compilé ni exécuté ici** (aucun Windows sur cette machine) |
| Appel depuis un `.rose` (`grand_nombre_fenetre` → vrai bouton OS) | **BLOQUÉ** : `interp.rs` hors périmètre (INT1) |
| Production des chiffres (`Valeur::Symbolique::demander_plus`) | **Dépendance VAL1** : le jardin reçoit la chaîne en argument, ne la calcule pas |

---

## 2. Ce qui est réellement implémenté

### 2.1 Le jardin — `jardins/gui_native.rose` (347 lignes après ajout)

Fonctions ajoutées (lignes 241‑347), **sans modifier aucune fonction existante**
et sans changer le format de `decrire()` (lignes 175‑189) :

| Fonction | Ligne | Rôle |
|---|---|---|
| `champ_lecture_seule(label, valeur)` | 241 | `champ()` + clé `lecture_seule = "1"` |
| `_pas_depuis_chaine(s)` | 249 | conversion des pas `"10"/"30"/"100"/"1000"` |
| `choisir_pas(f, n)` | 255 | exclusivité du groupe de cases, une seule à `"1"` |
| `pas_selectionne(f)` | 272 | lit le pas courant |
| `grand_nombre_fenetre(expression)` | 276 | construit la fenêtre complète |
| `fixer_chiffres(f, nouveaux)` | 296 | écrit la chaîne dans `f["chiffres"]` **et** dans le widget 0 |
| `clic_plus(f, nouveaux)` | 305 | incrémente `f["clics"]` du pas et met à jour le champ |
| `decrire_grand_nombre(f)` | 313 | `decrire(f)` + 7 lignes d'état déterministes |
| `geometrie_boutons(f)` | 331 | lignes `B|id|libellé|x|y|largeur|hauteur` pour le pont OS |

`grand_nombre_fenetre("factorielle(100)")` produit exactement 6 widgets et
1 action : champ `chiffres` (lecture seule), bouton `Plus de chiffres`, cases
`10/30/100/1000` (30 sélectionné par défaut), action `plus` → `plus_de_chiffres`.
Le libellé du bouton, les pas, le compteur et l'exclusivité sont **dans Rose** :
aucun de ces éléments n'est écrit en Rust.

### 2.2 Les primitives natives — `rose/src/gui_native.rs` (1071 lignes après ajout)

API publique **ajoutée**, l'API existante (`fenetre_creer`, `fenetre_afficher`,
`fenetre_attendre`, `fenetre_fermer`, `message`, `compter_fenetres`,
`creer_simulee`) est conservée telle quelle :

| Symbole | Ligne | Rôle |
|---|---|---|
| `Bouton` + `Bouton::nouveau` | 70 / 79 | définition générique, texte nettoyé, géométrie bornée |
| `bouton_sous(boutons, x, y)` | 454 | hit-test géométrique pur |
| `boutons_creer(id, &[Bouton])` | 474 | crée les enfants X11 (`XCreateSimpleWindow` + `XSelectInput` + `XMapWindow` + `XDrawString`) ou Win32 (`CreateWindowExW`, classe `BUTTON`) |
| `compter_boutons(id)` | 535 | diagnostic |
| `attendre_clic_limite(id, ms)` | 543 | boucle d'événements, rend `Some(id_du_bouton)` ou `None` |
| `attendre_clic(id)` | 627 | idem, délai lu dans `ROSE_GUI_ATTENTE_MS` |
| `clic_simuler(id, bouton)` | 636 | IE de test **sans écran**, clic posé dans une file |
| `x11_envoyer_clic(id, bouton)` | 667 | IE d'automatisation : `XSendEvent` vers le serveur X |
| `XSelectInput` / `XSendEvent` | 733‑734, 783‑784 | nouveaux symboles `dlsym` |
| `WM_COMMAND` (`0x0111`) dans `rose_wnd_proc` | 872 | clic Win32 → file `(HWND, id)` |
| `PeekMessageW` | extern Win32 | attente non bloquante avec délai |

`SYMBOLES_X11` passe de 16 à 18 noms. `fenetre_fermer` nettoie désormais les
registres de boutons et de clics ; c'est le seul comportement existant modifié
(ajout d'un nettoyage, aucune signature changée).

### 2.3 Preuve du clic réel (Xvfb)

```
$ ROSE_GUI1_TEST_X11=1 xvfb-run -a cargo test --release \
    gui_native::tests::gui1_x11_clic_reel -- --nocapture
[GUI1] test X11 reel demarre
[GUI1] clic X11 reel recu = Some(42)
test gui_native::tests::gui1_x11_clic_reel_aller_retour_serveur ... ok
test result: ok. 1 passed; 0 failed; ...
```

Le test est **optionnel** (`ROSE_GUI1_TEST_X11=1`) : sans ce drapeau, il
imprime `[GUI1] clic X11 reel SAUTE (ROSE_GUI1_TEST_X11!=1 ou DISPLAY vide)`.
Cela évite qu'il ouvre une vraie fenêtre pendant la suite parallèle et fausse
les tests de registre qui comptent les fenêtres.

Ce test crée une vraie fenêtre X, un vrai bouton enfant, puis envoie un
`ButtonPress` avec `XSendEvent` (filet X11 `ButtonPressMask = 4`). Le serveur X
le rediffuse ; `attendre_clic_limite` le reçoit par `XNextEvent`, reconnaît la
fenêtre enfant et rend **42**. Sans `DISPLAY`, le test imprime
`[GUI1] clic X11 reel SAUTE (DISPLAY vide)` et rend `ok` sans rien simuler.
Il a été rejoué deux fois (cible `lib` et cible `bin`) avec le même résultat.

### 2.4 Preuve de la logique Rose (sans écran)

Programme `/tmp/opencode/gui1_agents/test_garden_final.rose` (hors dépôt),
exécuté avec le binaire isolé :

```
importer "jardins/gui_native.rose"
$f = grand_nombre_fenetre("factorielle(100)")
>> decrire_grand_nombre($f)
>> geometrie_boutons($f)
...
```

Sortie réelle (extraits) :

```
FENETRE|Grands nombres : factorielle(100)|520|240|widgets=6|actions=1
W|champ|chiffres|12|16
W|bouton|Plus de chiffres|12|56
W|case|10|12|96
W|case|30|72|96
W|case|100|132|96
W|case|1000|200|96
EXPRESSION|factorielle(100)
CHIFFRES|
PAS|30
CLICS|0
LECTURE_SEULE|1
VALEUR_CHAMP|
ACTION_PLUS|1|plus_de_chiffres
B|1|Plus de chiffres|12|56|160|28

cases_initial|10=0 30=1 100=0 1000=0
apres_clic|30|30
cases_100|10=0 30=0 100=1 1000=0
apres_clic2|130|SUITE
```

`apres_clic2|130` prouve l'incrément cumulé : 30 (pas initial) puis 100
(après `choisir_pas($f, 100)`).

---

## 3. Ce qui est BLOQUÉ, et la preuve

### 3.1 Aucun appel possible depuis Rose

Le câblage de la stdlib est dans `rose/src/interp.rs`, fichier explicitement
interdit à GUI1 (INT1). Relevé réel :

```
$ grep -n 'boutons_creer\|attendre_clic\|compter_boutons\|gui_native::' rose/src/interp.rs
5056: match crate::gui_native::fenetre_creer(&titre, l, h) {
5065:     "fenetre_afficher" => crate::gui_native::fenetre_afficher(id as u64),
5066:     "fenetre_attendre" => crate::gui_native::fenetre_attendre(id as u64),
5067:     _ => crate::gui_native::fenetre_fermer(id as u64),
5076: if let Err(e) = crate::gui_native::message(&titre, &texte) {
```

**Seules les 5 primitives historiques sont connectées.** Il n'existe aucun nom
Rose pour `boutons_creer` ou `attendre_clic`. Par conséquent :

- un `.rose` peut construire la table, calculer `decrire_grand_nombre()` et
  produire `geometrie_boutons()` ;
- il **ne peut pas** demander au moteur de créer le bouton, ni attendre son clic ;
- le « test qui échoue » demandé par la mission est donc l'absence de pont,
  matérialisée par le grep ci‑dessus. Tenter d'appeler `boutons_creer(...)` dans
  un programme Rose échoue à la vérification : fonction inconnue. Aucun test
  n'a été écrit pour « faire croire » que le bouton répond depuis Rose.

Ce qui manque à INT1 (hors de mon périmètre) : ajouter les noms à
`nom_std_connu()` (interp.rs 1071‑1084), les valeurs à `stdlib_valeur()`
(4852+), et convertir la chaîne `geometrie_boutons()` en `&[Bouton]` avant
d'appeler `boutons_creer`, puis `attendre_clic`. Le format `B|id|label|x|y|w|h`
a été conçu pour cette conversion.

### 3.2 Les chiffres ne sont pas produits ici

`clic_plus(f, nouveaux)` reçoit la chaîne cumulée en argument. La mission
annonce que VAL1 fournira `Valeur::Symbolique` avec `demander_plus` ; GUI1 code
contre cette API prévue **sans modifier `run.rs`**. Tant que ce pont n'existe
pas, le champ se met à jour avec la chaîne donnée par l'appelant : c'est une
logique d'interface, pas un calcul de chiffres. Aucun résultat arithmétique
n'est revendiqué ici.

### 3.3 Le « lecture seule » est une propriété Rose, pas un contrôle OS

`champ_lecture_seule()` pose `lecture_seule = "1"` dans la table du widget.
Aucun contrôle `EDIT` en lecture seule n'est créé côté système : le backend X11
**dessine** le libellé/valeur avec `XDrawString`, et la branche Win32 ne crée
que des boutons. Il n'y a donc pas de champ saisissable à verrouiller. C'est
une propriété d'interface testable via `decrire_grand_nombre()` — pas une
promesse de contrôle de texte natif.

### 3.4 Win32 non vérifié

La branche `#[cfg(windows)]` (externes + `WM_COMMAND` + `CreateWindowExW` sur
la classe `BUTTON`) n'a **jamais été compilée** sur cette machine Linux. Elle
est donc « écrite », pas « prouvée ». Le code X11, lui, a été compilé et
exécuté. La voie « ELF généré par Rose » (`generer_fenetre`, lignes 216‑221 du
jardin) n'a pas été étendue aux boutons : elle reste exactement comme avant.

---

## 4. Validation

Binaire isolé : `/tmp/opencode/gui1_agents/rose-gui1` (copie de
`rose/target/release/rose` construite après les modifications GUI1 ;
`rose/rose` **n'a pas été remplacé**). La présence des nouveaux symboles dans
le binaire a été vérifiée : `strings rose-gui1 | grep XSelectInput` → 1 occurence.

| Oracle | Commande | Résultat réel |
|---|---|---|
| Jardin GUI1 | `rose-gui1 verif jardins/gui_native.rose` | `vérifié` |
| Jardin 20/20 | `rose-gui1 jardins/tests/premiers.rose` | `── jardin/premiers : 20 tests, 0 échec ──` |
| fib25 | programme `/tmp/opencode/gui1_agents/fib25.rose` | `75025` |
| Point fixe | `rose-gui1 rir compilateur/emetteur.rose` \| md5sum | `b3ed9262dd54776a6dbd07bc12368d0e` |
| Point fixe (voie 2) | `rose-gui1 compilateur/emetteur.rose compilateur/emetteur.rose` \| md5sum | `b3ed9262dd54776a6dbd07bc12368d0e` — **×2 égaux** |
| Banc 9 domaines | `rose-gui1 rose/bench_global.rose` depuis `/tmp/opencode/gui1_agents` | `rc=0`, 11 valeurs produites (dont `bench global Rose`) |
| Tests GUI1 | `cargo test --release gui_native::tests::gui1` | 5 passés / 0 échec (lib et bin) |
| Clic X11 réel | `ROSE_GUI1_TEST_X11=1 xvfb-run -a cargo test --release gui_native::tests::gui1_x11_clic_reel` | `Some(42)` reçu, 1 passé / 0 échec (lib et bin) |

**`cargo test --release` complet, état final : 0 échec.** Résultats réels :
`lib` 245 passés / 0 échec / 3 ignorés, `bin rose` 229 passés / 0 échec /
3 ignorés, les 4 autres cibles 0 échec. Les **12** tests de `gui_native`
(7 existants + 5 nouveaux) passent tous.

Constat daté sur dépôt partagé : au cours de cette mission, deux exécutions
intermédiaires ont échoué **sur le code d'autres agents** — `src/run.rs:541`
(`zeros_finaux`, attendu `BigInt`, trouvé `Gz`, depuis corrigé par VAL1) puis
`repl::tests::more_sur_un_objet_val1_reel` (`src/repl.rs:1198`, `left: 158`,
`right: 160`, depuis corrigé par CLI1). Une course entre les deux tests de
registre de `gui_native` a aussi été corrigée **côté GUI1** : mon nouveau test
ne compare plus `compter_fenetres()` à une valeur absolue, et le test X11 réel
est devenu optionnel. Les valeurs brutes du banc ont changé entre deux
exécutions parce que `run.rs`/`interp.rs` ont été modifiés entre-temps par
VAL1/INT1 ; GUI1 ne touche ni l'un ni l'autre. Le critère de non-régression
retenu ici est `rc=0`, identique à celui de `scripts/audit-final.sh`.

---

## 5. Ce qui reste à faire (hors périmètre GUI1)

1. INT1 : connecter `boutons_creer` / `attendre_clic` / `compter_boutons` à la
   stdlib Rose, avec conversion de `geometrie_boutons()` en `&[Bouton]`.
2. VAL1 : fournir la chaîne cumulée de `Valeur::Symbolique::demander_plus` à
   `clic_plus(f, nouveaux)` lors d'un clic.
3. Rejouer alors un test de bout en bout : ouvrir la fenêtre depuis un `.rose`,
   envoyer un clic, vérifier que `decrire_grand_nombre()` montre `CLICS` et
   `VALEUR_CHAMP` modifiés. Tant que ce test n'existe pas, GUI1 ne revendique
   **aucun** bouton cliquable depuis Rose.
4. Sur une machine Windows : compiler et exécuter la branche `WM_COMMAND`.

*Toute mesure ci‑dessus est un constat daté : re-mesurer après tout changement
de `interp.rs`, `run.rs` ou `repl.rs` par les agents concurrents.*
