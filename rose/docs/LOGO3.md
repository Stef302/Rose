# LOGO3 — tige de la rose coupée : correctif SYSTÉMIQUE (vague 80)

Date : 21/11/2026. Périmètre : `rose/src/setup_gui.rs`, `rose/src/paint.rs`,
ce document. Aucun commit ni publication de ma part, aucun PNG modifié.
⚠ Un agent concurrent a `git add`-é puis commité ces deux fichiers dans son
propre commit `c6702ff` (« 0.1.73 systémique : AOT universel, tray autonome
avec icône PE, logo entier ») pendant la mission : le contenu de mon correctif
y est intact, mais l'historique ne porte pas mon nom.

## 1. Cause systémique — pourquoi les deux rustines précédentes échouaient

Le dessin passait par deux formules **découplées** :

1. `geometrie_marque()` fixait `largeur_logo = s(110)` puis calculait
   `hauteur_logo = min(round(largeur × 337/264), place_logo)` : un **clamp de
   la hauteur seule**. La largeur restait la même ⇒ le ratio de la boîte
   changeait dès que la place manquait.
2. `dessiner_rose()` réservait ensuite une marge **proportionnelle**
   (`hauteur_max/24`, bornée 4..8 px) *à l'intérieur* de cette boîte, donc la
   hauteur de la rose était réduite une seconde fois, sans être réservée dans
   la géométrie. La place réservée au titre était calculée sur une boîte qui
   n'incluait aucune marge : la pointe anti-aliasée arrivait sur le bord.

Tant que la marge reste calculée *après* la boîte, et que la hauteur peut être
clampée sans ajuster la largeur, le symptôme revient à chaque nouvelle taille
de fenêtre. Correction précédente (LOGO_FIX) : ratio du contenu 264×337 au
lieu du PNG 276×343, marge 4..8 px inside → deux rustines, même résultat.

## 2. Solution imposée, appliquée

Toute la géométrie de la rose est déplacée dans `paint` (module PUR, compilé
et testé sous Linux) ; `setup_gui` ne traduit plus que des valeurs en GDI.

* `RATIO_ROSE = 337/264` défini **une seule fois**, mesuré sur les octets du
  PNG : bornes alpha (6, 6, 270, 343) ⇒ contenu opaque 264 × 337.
* Marges FIXES en pixels de destination : `MARGE_HAUT_ROSE = 4`,
  `MARGE_BAS_ROSE = 8`, total `MARGE_TOTALE_ROSE = 12`.
* `hauteur_rose_requise(largeur) = round(largeur × 337/264) + 12`.
  C'est cette hauteur — marge incluse — qui est réservée **avant** le titre,
  le sous-titre et les cartes.
* `boite_rose(largeur_souhaitée, hauteur_disponible)` réduit la **largeur**
  par recherche binaire sur `hauteur_rose_requise` (fonction monotone)
  jusqu'à ce que la hauteur requise tienne. **Aucun clamp de la hauteur seule.**
* `dessiner_rose` utilise le même ratio pour la boîte SOURCE (contenu opaque
  entier) et la boîte DESTINATION (`largeur × (hauteur − 12)`), puis place le
  contenu à `y + 4` : 4 px au-dessus, 8 px sous la pointe, aucune ligne perdue
  (réduction par moyenne de surface conservée, `AlphaBlend` à taille égale).
* Le petit logo de la barre du **launcher** utilise la même boîte :
  `boite_rose(s(24), s(40))` au lieu du `dessiner_rose(…, s(24), s(24))` écrit
  en dur. Même formule pour les deux logos.

## 3. Table des géométries — avant / après (pixels écran)

Replica indépendante des deux formules (script Python hors dépôt) sur
6 largeurs × 8 hauteurs (600 → 1080) × 3 DPI (96/120/144) = **144/144
combinaisons conformes** après correction.

`e = min(largeur/1120, hauteur/780)`. « Dessin » = taille réelle du contenu
opaque réduit ; « marge bas » = pixels réels entre la pointe et le bas de la
boîte.

| Fenêtre / DPI | Avant : boîte | Avant : dessin | Avant : marge bas | Après : boîte | Après : dessin | Après : marge bas |
|---|---|---|---:|---|---|---:|
| 1120×780 / 96  | 110×140 | 106×135 | 5 px | 110×152 | 110×140 | **8 px** |
| 806×562 / 96   |  79×101 |  76×97  | 4 px |  79×113 |  79×101 | **8 px** |
| 564×600 / 96   |  55×70  |  52×66  | 4 px |  55×82  |  55×70  | **8 px** |
| 736×512 / 96   |  72×92  |  69×88  | 4 px |  72×104 |  72×92  | **8 px** |
| 1120×600 / 144 |  85×109 |  82×105 | 4 px |  85×121 |  85×109 | **8 px** |
| 1920×1080 / 192| 152×194 | 146×186 | 8 px | 152×206 | 152×194 | **8 px** |

La boîte après est 12 px plus haute (marge réservée), le contenu dessiné est
plus grand (largeur totale utilisée au lieu d'une réduction indirecte) et la
marge basse est **exactement 8 px dans les 144 cas**, au lieu de 4–8 px
dépendant de la taille.

## 4. Tests

`paint.rs` contient trois nouveaux tests (ils tournent sous Linux, contrairement
aux anciens tests `#[cfg(windows)]`) :

* `boite_destination_contient_tout_le_contenu_opaque_et_les_marges` —
  les **144 combinaisons** : reconstruit le rectangle exact de
  `dessiner_rose`, vérifie `hauteur_logo = hauteur_rose_requise(largeur) ≤
  place_logo`, ratio source = destination, marge haute 4, marge basse 8,
  contenu entier dans la boîte, titre sous la boîte, étapes au-dessus des
  cartes.
* `place_insuffisante_reduit_la_largeur_pas_la_hauteur` — quand la place
  manque, la largeur baisse d'un pixel (`boite_rose(110,151) = (109,151)`),
  jamais la hauteur seule.
* `png_opaque_264x337_et_reduction_garde_la_pointe` — sur les **octets** du
  PNG embarqué : bornes (6,6,270,343), ratio 264/337 exact, pointe présente
  dans la dernière ligne réduite.

Les anciens tests Windows de `setup_gui` qui vérifiaient l'ancienne formule
(`RATIO_CONTENU_ROSE`, `marge_basse_logo`) ont été remplacés par deux tests
Windows alignés sur la boîte systémique (logo de barre et logo de panneau).

## 5. Vérifications exécutées (constats datés du 21/11/2026)

| Commande | Résultat |
|---|---|
| `cargo test --release` | **exit 0 — 614 passed, 0 failed, 6 ignored** (8 binaires de test) |
| `cargo test --release --lib paint::tests` | **12 passed, 0 failed** (dont les 3 nouveaux) |
| Replica Python des formules | **144/144**, marge bas exacte 8 px |
| `cargo xwin check --release --target x86_64-pc-windows-msvc` | **exit 0** |
| `cargo xwin check --release --lib --target …` et `--bin rose-setup --target …` | **exit 0** |
| Point fixe `./rose/rose rir compilateur/emetteur.rose` | md5 `b3ed9262dd54776a6dbd07bc12368d0e`, 263 520 o, stderr 0 |
| Point fixe `./rose/rose compilateur/emetteur.rose compilateur/emetteur.rose` | **même md5 `b3ed9262…`**, 263 520 o, stderr 0, `cmp` identique |

Note cross-compilation : `build.rs` (hors périmètre) transmet à `clang` les
drapeaux `/imsvc` fournis par cargo-xwin, que clang 18 refuse. Un adaptateur
**local** (`/tmp/opencode/bin/clang`, convertissant `/imsvc` en `-isystem`) a
été placé dans le PATH pour la vérification ; aucun fichier du dépôt n'a été
modifié à cette fin.

## 6. État des lieux

* Fait : cause systémique supprimée (plus de clamp de hauteur seule), géométrie
  centralisée dans `paint`, marges 4+8 réservées avant tout, même ratio source
  et destination, deux logos (launcher et panneau) sur la même formule,
  144 combinaisons prouvées, tests verts, cross-compile Windows vert, point
  fixe `b3ed9262` ×2.
* Non fait : capture visuelle réelle sous Windows (Wine 32 bits absent ici —
  voir LOGO_FIX §5, même limite) ; aucun PNG d'aperçu régénéré (preuve de
  rendu impossible, et ils ne sont pas produits par `setup_gui`).
* Extérieur au périmètre, non touché : modifications concurrentes en cours
  dans `main.rs`, `cmd_llvm.rs`, `build.rs`, `lib.rs`, `demarrage.rs`.
