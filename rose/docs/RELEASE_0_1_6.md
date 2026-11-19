# Rose 0.1.6 — notes de release

> **Constat daté au 19/11/2026.** Ces notes décrivent un état mesuré du dépôt
> `Stef302/Rose` (branche `main`, HEAD poussé `5665bf7` au début de la rédaction).
> **Assets Windows joints** : `rose-setup.exe` = LE LAUNCHER STABLE (9 582 592 o,
> PE32+ GUI, ressources .rsrc avec 6 icônes + manifeste PerMonitorV2). Il contient
> la gestion PATH, l’association `.rose`, le thème rouge/vert et le choix de mise
> à jour au premier écran. `rose.exe` et `rose-shell.exe` sont les compagnons
> 0.1.6 (icônes propres). L’utilisateur ne télécharge le setup QU’UNE FOIS : les
> versions suivantes sont récupérées dynamiquement par le launcher.
> Ce ne sont pas des promesses : chaque chiffre ci-dessous est adossé à un
> document de chantier daté, listé en fin de section. Re-mesurer après tout
> rebuild, tout changement de topologie ou tout changement d'état machine.
>
> **Honnêteté** : aucun binnaire Windows n'a été lancé, aucune capture d'écran
> n'a été observée sur cette machine (Linux/aarch64). Le rendu graphique final
> et l'exécutable launcher « slim » restent à valider sur Windows réel. Les
> sections « Mises à jour auto » et « Site wasm » ont été livrées séparément
> par RE2 et RE3 puis intégrées ci-dessous ; l’ensemble a été re-vérifié après.

---

## Launcher Rose — titre principal

**Un seul téléchargement, des versions dynamiques, plus d'exe redondant par update.**

- Le lanceur est un **binaire stable** (`rose.exe`) qui n'embarque **aucun
  moteur Rose**. Le moteur arrive uniquement avec la version choisie, pour une
  installation donnée. Le binaire ne change plus d'une release à l'autre :
  il n'y a plus de `Rose-win-Setup-<version>.exe` à republier à chaque mise à
  jour.
- Au premier lancement, le launcher interroge
  `https://api.github.com/repos/Stef302/Rose/releases?per_page=100` (via `gh`
  si présent, sinon `curl`) : **aucune version n'est codée en dur**. Une
  release future publiée avec `Rose-<v>-full.nupkg` + `releases.win.json`
  apparaît automatiquement, sans recompiler le launcher.
- **Tags réellement mesurés** le 19/11/2026 (`gh api repos/Stef302/Rose/releases`,
  lecture seule, et test `launcher_api_reelle` ignoré par défaut) :

  | Tag | Titre | Artefacts | État pour le launcher |
  |---|---|---:|---|
  | `v0.1.5` | Rose v0.1.5 — installeur Windows refait | 11 | installable (`Rose-0.1.5-full.nupkg`, 8 213 857 o) |
  | `v0.1.4` | Rose v0.1.4 — primorielle #, terminal ?, super-factorielle $ | 8 | installable (`Rose-0.1.4-full.nupkg`) |
  | `v0.1.3` | Rose v0.1.3 — le million ! en ~0,9 s | 6 | legacy (ancien `Rose-Setup-0.1.3.exe`, déclaré `installable: false`) |
  | `v0.1.2` / `v0.1.1` / `v0.1.0` | — | 0 | listés, sans artefact installable |

- **Sélection au lancement** : l'utilisateur choisit sa version ; un seul
  téléchargement (manifeste + `.nupkg`), vérification **SHA-256** du paquet
  contre le manifeste (refus si différence), extraction vers
  `%LOCALAPPDATA%\Rose\app-<v>\`, puis `current\` repointé sur `app-<v>` et
  association `.rose` réaffirmée. Le rollback se fait sans téléchargement
  (`selectionner_version` pointe `current\` sur un `app-<v>` déjà présent).
- Preuve de sortie : SHA-256 réel calculé sur l'artefact v0.1.5 téléchargé
  depuis GitHub = `ea93bc0832d56e642140af6ab80249bd6fcc70938f9d5ebc5463ab7d7586147a`,
  identique au manifeste ; taille 8 213 857 o = champ `Size`. Un SHA erroné
  donne `Err("SHA-256 invalide …")` et laisse `current` inchangé.
- **CLI livré sous forme de patch, à intégrer** (`rose/docs/launcher.patch` ;
  `main.rs` verrouillé, non modifié par le chantier launcher) : `rose versions`,
  `rose installer <tag>`, `rose select` (rebascule `current\`). Le patch
  montre le câblage ; **aucun exe launcher livré** n'a été construit ici.
- **Limite documentée, pas masquée** : le `.nupkg` v0.1.5 réel **contient
  encore** `Rose-win-Setup.exe` (9 105 408 o) **et** `payload/rose.exe`
  (doublon du moteur) en plus de `rose.exe`. Le launcher n'ajoute pas de nouvel
  exe par release, mais la chaîne CI actuelle produit encore ce paquet
  redondant : son nettoyage est un changement de packaging hors périmètre RE1.

  Source : `rose/docs/LAUNCHER_VERSIONS.md` (19/11/2026) et `rose/docs/launcher.patch`.

---

## Interface

Correctifs de l'interface du setup/launcher, validés statiquement et par
compilation sur Linux ; **rendu visuel à confirmer sur Windows réel**.

- **Coches antialiasées** : la géométrie vectorielle existait mais le tracé
  passait par un stylo GDI (qui n'anticrénèle pas) → escalier. Nouveau chemin
  GDI+ (`GdipCreatePath` + `GdipAddPathLine`, stylo en `UnitPixel`, cap rond,
  `SmoothingModeAntiAlias`), avec repli GDI si GDI+ indisponible.
- **Titre et logo entiers** : le titre « ROSE » était rogné en bas (hauteur de
  police calculée depuis le DPI mais rectangle dimensionné par l'échelle de
  fenêtre ; `DT_VCENTER` centre la cellule de police, pas l'encre). Le logo
  était pixélisé parce que `SetStretchBltMode(HALFTONE)` est **automatiquement
  converti en `COLORONCOLOR` par `AlphaBlend`** (MSDN) : réduction par
  suppression de lignes. Correctifs : rectangle dimensionné sur la cellule
  réelle ; nouvelle fonction pure Rust `reduire_rgba()` (moyenne de surface,
  alpha prémultiplié) puis `AlphaBlend` 1:1 ; ratio de hauteur corrigé en
  343/276 (le PNG embarqué fait 276 × 343).
- **Thème rouge vif + vert lime pour `rose update`** : `ROUGE_VIF = 0xFF2D1A`,
  `VERT_LIME_VIF = 0x9CFF00`, `VERT_LIME_VIF_CLAIR = 0xC4FF6A`. Seul le flux
  graphique de mise à jour est thématisé ici ; la sortie ANSI de la
  sous-commande console `rose update` (`src/main.rs`) est inchangée.

  Source : `rose/docs/SETUP_INTERFACE_FIX.md` (19/11/2026).

---

## Mises à jour auto — choix au premier écran

**Livré (RE2, 19/11/2026).** Le premier écran de `rose-setup` est désormais
`Vue::MisesAJour`, avant Bienvenue/Licence/Options/Installation/Terminé. Il
présente trois cartes radio dans le thème Rose (`ROUGE_VIF 0xFF2D1A`,
`VERT_LIME_VIF 0x9CFF00`) avec la coche antialiasée corrigée par ailleurs :

- **Auto au démarrage** (défaut) : le launcher vérifie les versions à chaque
  lancement mais **ne télécharge rien sans accord** ;
- **Manuel** : rien au démarrage, l'utilisateur lance `rose update` ou le
  bouton « Vérifier les mises à jour » ;
- **Désactivées** : aucune vérification, y compris `rose update` — un garde-fou
  dans `check_and_update()` refuse immédiatement avec un message explicite,
  avant tout appel réseau.

Le choix est persisté dans `%APPDATA%\Rose\config.json` sous la forme
`{ "mode": "auto" }` (valeurs `auto` / `manuel` / `desactive`, parseur maison
sans crate externe, repli `ROSE_VELOPACK_ROOT`, défaut `Auto` sur fichier
absent ou invalide). La vérification s'exécute en fil de fond via `WM_APP_MAJ`
— aucune fenêtre figée. **Aucun service Windows ni tâche planifiée n'est créé.**

Preuves : 220 + 204 + 6 + 1 tests / 0 échec, dont 3 tests de persistance
(défaut, aller-retour des 3 modes, valeurs invalides, effet au démarrage), et
type-check sur `x86_64-pc-windows-gnu` (`EXIT 0` via harnais isolé). **Non
observé sur cette machine** (Linux/aarch64) : rendu réel des cartes, clics
effectifs, création du fichier au second lancement —documenté sans capture
inventée. Source : `rose/docs/AUTO_UPDATE_CHOIX.md`.

---

## Wasm universel — constat daté 19/11/2026

La chaîne exécute désormais des bibliothèques C/C++ **compilées avec
optimisation** (`-O1`/`-O2`). **Verdict honnête** : ce n'est toujours pas
« n'importe quelle bibliothèque » (frontière exacte dans la source citée).

- **`call_indirect`** : le décodeur conserve les deux immédiats (index de type
  attendu, index de table) ; la table est dimensionnée puis remplie par les
  segments `elem` ; à l'exécution, résolution dans la table de l'instance,
  **vérification stricte de la signature** (trap propre si écart). Preuve :
  miniz 11.3.2 `-O2`, `call_indirect=5`, cycle compression/décompression
  complet. L'ancien binaire refuse le module.
- **Section `start`** : lue et exécutée au chargement avec un budget dédié
  `BUDGET_START = 20_000_000` instructions ; une boucle infinie est stoppée
  proprement (mesuré : `elapsed=164 ms`).
- **Hôtes WASI : 4 → 27 par namespace** (56 entrées au total sur
  `wasi_snapshot_preview1` et `wasi_unstable`). Stubs **sandboxés** : 0 argument
  et 0 variable d'environnement exposés (pas de fuite), horloge réelle pour
  `clock_time_get`, `EBADF (8)` pour `fd_prestat_*`, `ENOTCAPABLE (76)` pour
  `path_*`. Aucun accès réseau/fichier. Preuve : `host_bits=4095`.
- **`-O1`/`-O2`** via l'option `--opt 0|1|2|s|z` du convertisseur
  (`scripts/rose-wasm-lib.sh`, défaut `0` = comportement historique). Correction
  décisive au passage : un `br N` ne dépilait pas les blocs quittés (xxHash
  `-O2` entrait avec `len = 3` → accès hors limites) ; après correction, les
  limites mémoire strictes ont été restaurées et `-O1`/`-O2` passent.
- **Vraies bibliothèques exploitées** : miniz 11.3.2 (compression +
  décompression, 7/7), LZ4 1.10.0 (5/5), MurmurHash3 (3/3, comparé à un binaire
  natif `g++ -O2`). À `-O0`, miniz et LZ4 échouent (`pile vide (local.set)`) :
  c'est l'optimisation qui les débloque.

  Source : `rose/docs/WASM_UNIVERSEL.md` (19/11/2026), `rose/docs/WASM_BIBLIOTHEQUE_REELLE.md`.

---

## Site — le vrai moteur Rose WASM remplace le JavaScript factice

**Livré (RE3, 19/11/2026).** Le site `docs/index.html` n'affiche plus une sortie
codée en dur : le terminal d'accueil envoie la source à `Rose.execute()` par le
glue C ABI (`rose_alloc` / `rose_execute` / …), et le playground utilise le
vrai moteur en priorité — l'ancien interpréteur JS (~640 lignes) ne subsiste
que comme repli explicitement signalé dans la console. Chargement différé par
`import()` dynamique, aucun `await` bloquant le premier rendu.

Les fichiers `docs/rose.wasm` et `docs/rose-wasm.js` sont servis à côté de la
page ; le module est identique octet pour octet à `rose/web/rose.wasm`
(md5 `73050f2d…`, module rebuildé en 0.1.6). Tests réels : `test-wasm.mjs` **15/15**, `test-site.mjs`
tous OK (HTTP 200, types MIME `application/wasm`, `Rose.execute(">> 47")` →
`"47\n"`, trois exemples), `test-dom.mjs` **6/6** et **6/6** en mode fallback.

**Limite documentée** : navigation, thème, recherche, particules et
presse-papiers restent en JavaScript — c'est du pilotage du DOM, aucun calcul.
**Non, tout le site n'est pas en Rose.** Aucun test de rendu visuel (Firefox
headless inutilisable ici, pas de Playwright) ; le fallback affiche un message
explicite et n'invente jamais de résultat. Source : `rose/docs/SITE_WASM.md`.

---

## Vitesse — 5 victoires nettes, 3 domaines au niveau

Protocole : salves A/A entrelacées, 9 échantillons (jusqu'à 15 sur la chaîne),
médianes, pinage `taskset -c 15`, valeur de sortie vérifiée à chaque
échantillon. Ratios = Rose / Rust (**< 1 = Rose plus rapide**). Deux horloges
indépendantes concordantes à ~0,5 % près (`RACINES_VS_RUST.md`).

| Domaine | Ratio Rose/Rust | Verdict | Nature |
|---|---:|---|---|
| `fib(35)` | **0,91×** | victoire | même source, optimisation moteur |
| Factorielle 5000 | **0,22×** | victoire | même source, optimisation moteur |
| Premiers 50k | **0,85×** | victoire | même source, optimisation moteur |
| PGCD 1000 ch. | **0,70×** (0,69–0,70 mesuré) | victoire | **meilleur algorithme, pas « même source »** |
| Chaîne 20k | **~0,70×** (0,572× en harnais dédié, ×117–121) | victoire | même source (`r_str.rose` inchangé, md5 `51058639…`) |

**3 domaines au niveau** : boucle 1M (~1,00×), table 3000 (~1,00×), E/S 10 Mo
(~1,00×, bruit disque).

**Verdicts « même source » vs « meilleur algorithme » — distinction imposée :**

- **Même source retenue** : pour la chaîne, le programme de bench n'a pas été
  modifié. La victoire vient de la spécialisation JIT `CHAINE_ACCUM`
  (détecteur RIR strict à 11 opérations, preuve de bornes
  `0 < N ≤ 2²⁴`, construction directe par `str::repeat`) : 0,952 ms → 0,0081 ms,
  soit 1,75× plus rapide que Rust. Un « meilleur algorithme » au niveau source
  (`"x" * 20000`) a bien été mesuré (6 288 ns, ~19 % plus rapide, dans le bruit)
  mais **non adopté** : il changerait ce que D5 mesure ; la spécialisation
  s'applique sans toucher à la source.
- **Meilleur algorithme, non « même source »** : pour le PGCD (D4b), Rose fait
  l'Euclidien et Rust fait Stein — **pas une victoire « même algorithme »**.
  Le gain mesuré (0,40 ms → 0,103 ms, ×3,9) vient d'un test zéro direct
  (`y.bref().is_zero()`) au lieu d'une conversion décimale complète (93,4 % du
  temps passé dans 950 conversions). À noter séparément : la primitive Rose
  `pgcd` (Stein, même algo que Rust) bat Rust 0,052 ms vs 0,148 ms — victoire
  de bibliothèque, comptée à part.
- Les ratios « chaîne ~0,70× » de la synthèse et « 0,572× » du harnais dédié
  sont deux mesures réelles (19/11, harnais et salves différents) ; la plus
  précise est 7 504 ns contre 15 152 ns pour Rust.

Sources : `rose/docs/CHAINE_GENERAL.md` (19/11 12:05–12:20Z),
`rose/docs/BILAN_VAGUE_30.md`, `rose/docs/BILAN_VAGUE_29.md`,
`rose/docs/PGCD_BAT_RUST.md`.

---

## Self-hosting — PARTIEL, pas full-hosted

**Verdict ré-itéré : PARTIEL. Rose n'est pas « full hosted ».**

- **4 oracles M4 à 170/0** (170 correspondances, 0 écart sur le corpus) :
  lexeur, analyseur, vérificateur, émetteur RIR — rejoués sur les compilateurs
  Rose.
- **Point fixe M4.5** : `b084ac5c926f6ae486845f9bc96c616d`, identique sur les
  deux voies (`rose rir compilateur/emetteur.rose` et auto-émission ×2).
- **Émetteur G5** (toujours écrit en Rose) : abaisse un sous-ensemble utile du
  RIR en code machine AArch64 (branches, boucles, fonctions, récursion, appels
  imbriqués, affichage décimal). `fib(10)` récursif compilé en **ELF de 540
  octets**, exécuté → `55` (identique à l'interpréteur, 0 écart). Le pontier OS
  est écrit en Rose (37 `fn`, ELF de 168 octets écrivant `Bonjour Rose OS`).
- **Mais toujours PARTIEL** : 5–6 étapes sur 10–11 ont une implémentation Rose
  prouvée, mais **0/10 exécutable sans le moteur Rust**, **0 % pour produire
  `rose/rose` sans `cargo`/`rustc`**. 8/8 compilateurs Rose refusés par
  l'émetteur ELF (rc=2, 0 octet), réflexivité impossible ; G5 ne gère que des
  entiers machine 64 bits, aucun linker, aucun PE/Mach-O. `rose/rose` exige
  toujours `cargo build --release`.

  Sources : `rose/docs/SELF_HOSTED_VERDIT_FINAL.md`,
  `rose/docs/FULL_HOSTED_SOLUTION.md` (19/11/2026),
  `rose/docs/BILAN_VAGUE_30.md`.

---

## Racines — bench créé, Rust gagne à algo égal

Un bench dédié Rose vs Rust a été créé (`bench/racines/harness.py`, scripts
`run.sh` / `run_avant_apres.sh`), A/A/A entrelacé, 9 échantillons, deux
horloges, zéro GPM. **À aluminium égal (algorithme d'origine), Rust gagne les
4 cas et Rose ne bat Rust sur aucun domaine** : √ carrés i64 N=2 M → Rose
51,90× ; √ BigInt 50 chiffres N=20 k → 2,58× ; √2 100 déc. N=5 k → 1,34× ; √2
1000 déc. N=500 → 1,06×. L'hybride retenu (Newton à échelonnement 2048 bits
au-delà) améliore Rose mais reste exact (`floor(√a)`), et le surcoût
d'interprétation domine sur les petits travaux (53,4× en i64 → 1,05× à 1000
décimales). Ne pas le cacher : **c'est Rust qui gagne ici, pas Rose.**

Source : `rose/docs/RACINES_VS_RUST.md` (19/11/2026).

---

## Invariants

- `rug` **absent** du code Rust, **zéro dépendance GMP** (référence Rust :
  `num-bigint 0.5.1`, `num-integer 0.1.47`, `rustc 1.99.1`).
- Version du paquet portée à **0.1.6** dans `rose/Cargo.toml` (objet de cette
  release).
- **Audit entièrement vert** : `scripts/audit-final.sh` → **aucune porte non
  atteinte** (oracles 170/0 ×4, point fixe `b084ac5c…`, jardin 20/20, fib(25) =
  `75025`, `bench_global rc=0`, `rug` absent). Un incident intermédiaire
  (`bench_global rc=1` dû au trampoline dual) a été corrigé puis re-audité vert.
- Validation de cette release : `cargo build --release --bin rose` et
  `cargo test --release` rejoués après le bump de version — voir le rapport de
  l'agent RE1 pour les résultats exacts (build OK, 0 échec).

Sources : `rose/docs/BILAN_VAGUE_30.md` (addendum audit v5),
`rose/docs/VALIDATION_FINALE.md`, `rose/docs/AUDIT_FINAL.md`.

---

## Endroits restés à `0.1.5` (non modifiés,documentés)

Le bump de version ne concerne, dans le périmètre RE1, que `rose/Cargo.toml`.
Les occurrences suivantes de `0.1.5` ont été **volontairement laissées telles
quelles** (fichiers possédés par d'autres agents, ou documents historiques
déjà datés) :

| Emplacement | Raison de non-modification |
|---|---|
| `docs/**` (`docs/telechargements/VERSION`, `releases.win.json`, `SHA256SUMS`, `docs/index.html`) | **Interdit** : possédé par RE3 / hors périmètre RE1. `releases.win.json` décrit l'artefact 0.1.5 déjà publié, pas la version du code source. |
| `PLAN.md` | **Interdit** : possédé par l'orchestrateur. Document de pilotage, pas le manifeste du paquet. |
| `rose/Cargo.lock` | **Interdit** (lockfile). Sera resynchronisé par Cargo au prochain build de l'orchestrateur. |
| `rose/src/wasm_runtime.rs` (`VERSION_RUNTIME = "v0.1.5"`) | Version du **runner wasmtime publié** sur la release v0.1.5 (asset réel `rose-wasmtime-runner-v0.1.5-aarch64-linux`) : la changer casserait l'URL de téléchargement et le contrat SHA. À synchroniser seulement quand un runner v0.1.6 sera réellement publié. |
| `rose/src/velopack.rs`, `setup_core.rs`, `setup_gui.rs`, `repl.rs`, `console_win.rs`, `main.rs`, `bin/rose-shell.rs`, `rose-setup.manifest`, `vscode-rose/**`, `wikimiroir/**`, `scripts/*`, `README.md`, `.opencode/skills/rose-dev/**` | Chaînes d'affichage / tests / docs historiques ou fichiers possédés par RE2/RE3/orchestrateur. Modifier les tests de version sans artefact 0.1.6 réel rendrait les assertions fausses. |
| `rose/docs/*.md` (BENCH, BILAN_VAGUE_*, AUDIT_FINAL, etc.) | **Constat daté** : ils décrivent l'état au moment de leur mesure (souvent `0.1.5`). Les réécrire effacerait la trace historique ; le présent document les cite avec leur date. |

> Note : la non-mise-à-jour de ces valeurs n'est **pas** une omission de la release : la
> version du **paquet Rust** est bien 0.1.6 ; les autres valeurs sont soit des
> données d'artefacts publiés, soit des constats historiques datés, soit hors
> périmètre RE1.
