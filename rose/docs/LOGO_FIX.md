# LOGO1 — logo et titre de l'installeur Windows : diagnostic et correctif

Date : 21/11/2026 (constat daté sur l'état `6e8f6db` + correctif `setup_gui.rs`).
Périmètre : `rose/src/setup_gui.rs` uniquement (plus ce document). Aucun PNG modifié.

## 1. Cause confirmée

Dans `dessiner_decore` (ancien code, `setup_gui.rs` ~1575-1615) :

* `place_logo = (plancher - s(78) - s(120)).max(0)` réservait **120 unités** fixes
  pour le titre **et** sa description, indépendamment du DPI et de la hauteur réelle
  de la police Kare.
* `h_marque = max(s(72), marque_px + s(28))` : la cellule du titre suit le DPI
  (76 px à 96 DPI, 100 px à 144 DPI, 172 px à 192 DPI sur la grille 1120×780).
* `centre_marque = y_marque + s(34)` ⇒ **bas réel du titre** =
  `y_marque + s(34) + h_marque / 2`.
* Le sous-titre était posé à `top = y_marque + s(56)` : décalage **fixe**.
  Le bas du titre dépassant `s(56)`, le texte « Le langage compilé en français
  qui se compile lui-même. » recouvrait le mot « ROSE ».

Mesure du chevauchement (ancien code, `bas_titre - top_sous_titre`, pixels écran) :

| Fenêtre | DPI | Ancien chevauchement |
|---|---:|---:|
| 1120 × 780 | 96 | 16 px |
| 806 × 562 (taille min.) | 96 | 12 px |
| 564 × 600 (capture) | 96 | 8 px |
| 736 × 512 | 96 | 10 px |
| 1120 × 600 | 144 | 22 px |
| 1920 × 1080 | 192 | 55 px |

Le logo lui-même **n'était pas coupé en pixels** : `dessiner_rose` (l. 756-833)
calcule `bornes_opaques` sur le PNG 276 × 343 (contenu utile 264 × 337), réduit
par moyenne de surface puis recadre en conservant le ratio dans la boîte fournie.
Une boîte moins haute réduit le logo, elle ne rogne jamais la tige. L'impression
de « logo cropé » venait du texte posé par-dessus le bas du titre, pas d'une
perte de pixels.

## 2. Correctif

Nouvelle fonction pure `geometrie_marque(ht, dpi, echelle) -> GeometrieMarque`
(et sa struct), appelée **une seule fois** au début de `dessiner_decore`. Elle
concentre toute la géométrie verticale du panneau :

1. `plancher` calculé une seule fois (cartes du bas réservées avant tout) et
   réutilisé par les cartes, les étapes et les tests — plus de divergence.
2. `h_marque = max(s(72), points_ecrits(dpi,36,echelle) + s(28))` : hauteur
   RÉELLE de la cellule du titre.
3. `marge_marque = max(s(26), h_marque/2 - s(34) + s(4))` : garantit que le
   haut de Kare (montée 979/1000) ne remonte jamais sur la tige, même à 192 DPI.
4. **Sous-titre positionné depuis le bas réel du titre** :
   `haut_sous_titre = bas_titre + s(10)`, `bas_sous_titre = haut_sous_titre + s(56)`.
   Le `y_marque + s(56)` fixe a disparu.
5. **Place du logo réservée depuis tout ce qui suit** :
   `place_logo = plancher - s(78) - marge_marque - (s(34) + h_marque/2 + s(10)
   + s(56) + s(10))`, bornée à 0 ; `hauteur_logo = min(round(s(110)*343/276),
   place_logo, 343)`. Le plafond 343 = hauteur native du PNG : pas
   d'agrandissement. `dessiner_rose` conserve `bornes_opaques` + ratio.
6. **Étapes** : `y = bas_sous_titre + s(10)`, donc plus de remontée sur le
   sous-titre ; la boucle existante borne toujours la liste à `plancher`.

## 3. Géométrie avant / après (pixels écran)

`e = min(largeur/1120, hauteur/780)`, `s(v) = round(v*e)`.

| Fenêtre / DPI | Avant : titre | Avant : sous-titre | Chevauch. | Après : titre | Après : sous-titre | Marge | Hauteur logo |
|---|---|---|---:|---|---|---:|---:|
| 1120×780 / 96 | 237..313 | 297..353 | 16 | 237..313 | 323..379 | 10 | 137 |
| 806×562 / 96 | 170..225 | 213..254 | 12 | 170..225 | 232..272 | 7 | 98 |
| 564×600 / 96 | 118..156 | 148..176 | 8 | 118..156 | 161..189 | 5 | 68 |
| 736×512 / 96 | 154..204 | 194..231 | 10 | 154..204 | 211..248 | 7 | 89 |
| 1920×1080 / 192 | 294..466 | 411..488 | 55 | 303..475 | 489..567 | 14 | 189 |

La taille de la boîte du logo est **identique** avant/après dans ces cas
(elle n'était pas clampée) : le changement visible est bien la disparition du
recouvrement, pas un redimensionnement. Sur toutes les tailles testées,
`hauteur_logo >= s(40)` (seuil de dessin) et `bas_sous_titre <= plancher`.

## 4. Tests ajoutés

4 tests dans `#[cfg(test)] mod tests_geometrie` de `setup_gui.rs` :

* `geometrie_sans_chevauchement_grille` : 7 tailles (dont la capture 564×600 et
  la taille minimale 806×562) × 4 DPI (96/120/144/192), assertions :
  `hauteur_logo <= place_logo`, `<= 343`, ratio natif respecté,
  `haut_titre >= haut_logo + hauteur_logo`, `haut_sous_titre >= bas_titre + s(10)`,
  `y_etapes >= bas_sous_titre`, `bas_sous_titre <= plancher`, `y_etapes <= plancher`.
* `sous_titre_suit_le_bas_reel_du_titre_quand_le_dpi_monte` (régression 192 DPI).
* `logo_reste_visible_sur_fenetre_basse_et_etroite`.
* `premier_paint_sans_dimension_ne_panique_pas` (client 0 × 0).

⚠ **Exécution des tests** : `setup_gui.rs` commence par `#![cfg(windows)]`, donc
`cargo test` sous Linux **ne compile pas** ces tests. Exécution réelle retenue :
extraction du code exact de `GeometrieMarque` + `geometrie_marque` + du module de
tests depuis `setup_gui.rs`, compilation `rustc --test --edition 2021` →
**4 passed, 0 failed**. Le harnais `cargo xwin test --target x86_64-pc-windows-msvc`
échoue à l'édition de liens (**préexistant**, indépendant du correctif) :
`lld-link: duplicate symbol: dlopen/dlsym/dlclose/getrandom/mmap/...`
(`windows_compat.lib` passé en `/WHOLEARCHIVE` et déjà dans `librose.rlib`).
`cargo xwin check --release` (cible Windows) passe.

## 5. Aperçus `installateur/velopack/apercu-*.png` — NON régénérés

Raison précise, pas un oubli :

* Les 5 PNG datent du 16/11 et ont été produits par `previsualiser.py` (rendu
  Python/PIL de la maquette), **pas** par un rendu de `setup_gui.rs`. Ce script
  a sa propre géométrie figée (`place_logo = plancher - s(78) - s(150)`,
  `y_marque + s(28)`, sous-titre à `y_marque + s(58)`) qui **ne reflète déjà pas**
  le Rust d'avant ni celui d'après. Le régénérer ne constituerait donc **aucune
  preuve** du correctif ; le mettre à jour exigerait de modifier `previsualiser.py`
  (fichier hors périmètre).
* Tentative de preuve réelle faite : Wine 9.0 est présent mais 64 bits seulement
  (`wine32` absent) ; `wineboot --init` reste bloqué (`boot event wait timed out`)
  sur deux prefixes frais (`/tmp/opencode/wineprefix`, `/tmp/opencode/wp2`,
  `/tmp/opencode/wp3`) et la capture Xvfb `:99` est un écran **entièrement noir**
  (aucune fenêtre). Pas de rendu réel ⇒ pas de capture probante ⇒ PNG laissés
  intacts.

## 6. Validation (constats datés du 20-21/11)

* `cargo build --release` : **OK** (exécuté avant l'édition concurrente de
  `rose/src/front.rs` signalée ci-dessous).
* `cargo xwin check --release --target x86_64-pc-windows-msvc` : **OK** (warnings
  préexistants seulement).
* Tests de géométrie extraits : **4/4 OK**.
* `cargo t --release` (alias `test --bins`) à l'instant T : **3 échecs préexistants**,
  tous hors périmètre :
  - `repl::tests::more_sur_un_objet_val1_reel` : passe avec `--test-threads=1`
    (état global partagé entre tests parallèles) ;
  - `velopack::tests::catalogue_filtre_installable` et
    `launcher_selection_et_cache` : échouent en suite complète même sérialisée,
    **passent isolément** (pollution d'état global par un test antérieur).
  Aucun de ces fichiers n'est compilé depuis `setup_gui.rs` (module Windows),
  donc ces échecs ne peuvent pas venir du correctif. **Non corrigés : fichiers
  interdits par la mission.**
* `rose/rose jardins/tests/premiers.rose` → `20 tests, 0 échec` (**20/20**).
* `fib(25)` → **75025**.
* Point fixe M4.5 : `rose rir compilateur/emetteur.rose` et
  `rose compilateur/emetteur.rose compilateur/emetteur.rose` → MD5
  `b3ed9262dd54776a6dbd07bc12368d0e` **identiques (×2)**.
* `rose verif` : non pertinent (aucun fichier `.rose` modifié).

## 7. État des lieux / reste

* Fait : cause trouvée et chiffrée, correctif appliqué dans `setup_gui.rs`,
  4 tests ajoutés et exécutés, compilation Windows vérifiée, doc écrite.
* Non fait : régénération des PNG (preuve de rendu réelle impossible ici, §5) ;
  correction des 3 tests préexistants (hors périmètre).
* ⚠ Contexte : pendant cette mission, un **autre agent concurrent** a modifié
  `rose/src/front.rs` (optimisation « SP3 — texte partagé », `Rc<str>`). Ce
  fichier est actuellement **cassé** (`E0282`, `E0308` au `cargo check --release
  --bin rose`) et n'a **pas** été touché ici : hors périmètre. Le build OK cité
  plus haut date d'avant cette modification externe.
