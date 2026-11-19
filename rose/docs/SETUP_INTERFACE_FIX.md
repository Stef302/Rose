# Correctifs interface setup/launcher — constat du 19/11/2026

Périmètre : **un seul fichier source modifié**, `rose/src/setup_gui.rs` (ce document
est le second fichier possédé). HEAD de départ : `006ea56`. Aucun commit, aucune
publication, aucun binaire remplacé (`rose/rose` intact). Binaire de travail isolé
dans `/tmp/opencode/la_agents/`.

⚠ **Aucune machine Windows n'a été utilisée.** Aucune capture d'écran n'a été
produite ni observée. Tout ce qui suit est soit du **statique** (code + mesures de
ressources + documentation Microsoft), soit de la **compilation**, soit un test
exécuté sur Linux. Le rendu visuel final reste à vérifier sur Windows réel.

## 1. Causes statiques reproduites

### (a) Coches « un poil pixelisées »

AVANT (`setup_gui.rs.before:1135-1149`) :

```rust
unsafe fn coche(dc: HDC, cx: i32, cy: i32, demi: i32, couleur: u32) {
    let d = demi.max(2);
    let epaisseur = ((d as f64 * 0.34).round() as i32).max(2);
    let stylo = CreatePen(0, epaisseur, couleur);
    ...
    MoveToEx(dc, a.0, a.1, std::ptr::null_mut());
    LineTo(dc, b.0, b.1);
    ...
}
```

La géométrie vectorielle était déjà là (deux `MoveToEx`/`LineTo`), mais le tracé
passait par un **stylo GDI** : GDI n'anticrénèle pas les lignes, d'où l'escalier.
L'épaisseur n'était pas bornée haut (`.max(2)` seul). Ce n'était pas un glyph de
police : le commentairehistorique sur le glyphe `✓` absent concernait les cartes,
pas cette fonction.

### (b) Nom « ROSE » cropé vers le bas

AVANT (`setup_gui.rs.before:1487-1489`) :

```rust
let mut rl = RECT { left: 0, top: y_marque, right: lateral_w, bottom: y_marque + s(56) };
texte_echelle(dc, dpi, "Kare", "Rose", &mut rl, DT_CENTER | DT_VCENTER | DT_SINGLELINE, TEXTE, 36, false, echelle);
```

et, dans la barre de titre (`before:1356`) :

```rust
let mut rt = RECT { left: s(52), top: s(13), right: s(150), bottom: s(43) };
```

Deux causes mesurables :

1. `points_ecrits()` (l.794-798) calcule la hauteur de police en pixels depuis le
   **DPI** (`points * dpi / 72`) alors que le rectangle est dimensionné par
   `s(56)` = `56 * echelle`, et `echelle_tracé(w,h)` (paint.rs:102) ne regarde QUE
   la taille de fenêtre. Sur une fenêtre bridée par l'écran (ex. 1366×768 en 150 %
   → `echelle ≈ 0,905` pour `dpi = 144`), la cellule fait 72 px dans un rectangle
   de ~51 px : `DrawText` rogne le bas. Même à 96 DPI, une fenêtre réduite
   reproduit le décalage.
2. Métriques réelles de Kare (WOFF2 embarqué, lues avec fontTools le 19/11) :
   `unitsPerEm = 1000`, `hhea.ascent = 979`, `descent = -21`,
   `usWinAscent = 1163`, `usWinDescent = 37` (cellule = 1,2 em), mais les glyphes
   `R/o/s/e` ne montent qu'à `yMax ≈ 651..665`. `DT_VCENTER` centre la **cellule**,
   pas l'encre : le visible est poussé vers le bas du rectangle.

### (c) Logo non affiché en entier / pixélisé

AVANT (`setup_gui.rs.before:676, 765, 774`) :

```rust
unsafe fn dessiner_rose(dc: HDC, x: i32, y: i32, cote: i32) -> bool { ... }
SetStretchBltMode(dc, HALFTONE);
AlphaBlend(dc, dx, dy, dw, dh, memoire, sx0 as i32, sy0 as i32, sw as i32, sh as i32, melange);
```

et (`before:1471`) :

```rust
let hauteur_voulue = ((largeur_voulue as f64 * 169.0 / 132.0).round() as i32).max(1);
```

Faits vérifiés :

* le PNG embarqué (`installateur/velopack/rose-installeur.png`) fait **276 × 343**,
  avec du contenu opaque sur `x 6..270`, `y 6..343` (alpha > 4, mesuré avec
  Pillow et confirmé par les tests de `rose/src/png.rs`). L'ancien calcul
  `169 / 132` venait d'un visuel obsolète : la hauteur visée (141 px) était fausse
  et la boîte était CARRÉE (`cote`), avec un centrage qui compensait le ratio.
* **Cause réelle de la pixélisation** : MSDN, page `AlphaBlend`, Remarks —
  « *If the SetStretchBltMode function is used, the iStretchMode value is
  automatically converted to COLORONCOLOR for this function* ». Le
  `SetStretchBltMode(dc, HALFTONE)` du code source n'avait donc **aucun effet**
  sur la réduction 264 × 337 → ~107 × 137 : `COLORONCOLOR` supprime des
  lignes/colonnes entières. Le commentaire du fichier accusait à tort le mode
  par défaut du DC et préconisait HALFTONE ; il a été corrigé.
* Vérification complémentaire : la ligne du bas du PNG ne contient de l'alpha
  que sur `x = 149..165` (17 px, max 127) ; une réduction par suppression de
  lignes peut faire disparaître cette tige, ce qui correspond exactement au
  symptôme « logo non affiché en entier ».

### (d) Thème « rose update »

AVANT : bouton principal `ROUGE`, barre de progression `ROUGE_CLAIR`, succès
`VERT` (0x9ACD32), statut gris. Il n'existait aucun `ROUGE_VIF` explicite et
aucun lime vif pour la progression. La sortie ANSI de la sous-commande console
`rose update` est dans `rose/src/main.rs` (fichier **non possédé**, inchangé) :
seul le flux graphique de mise à jour a été thématisé ici.

## 2. Correctifs appliqués (après)

| Défaut | Correctif | Emplacement (fichier modifié) |
|---|---|---|
| Coches | `coche_lisse()` : chemin GDI+ (`GdipCreatePath` + 2 × `GdipAddPathLine`), stylo `GdipCreatePen1` en `UnitPixel`, largeur 2–3 px, cap rond, `avec_gdiplus` (SmoothingModeAntiAlias). Repli GDI conservé si GDI+ indisponible. | FFI ~l.98-102 ; `coche`/`coche_lisse` ~l.1174-1234 |
| Titre ROSE | Rectangle dimensionné sur la cellule réelle : `h_marque = max(s(56), marque_px + s(12))`, centré symétriquement sur le même centre `y_marque + s(28)`. Idem pour le « Rose » de la barre de titre. Police inchangée (36 pt / 15 pt). | ~l.1438-1448 et ~l.1565-1583 |
| Logo | Nouvelle fonction pure Rust `reduire_rgba()` : moyenne de surface avec alpha prémultiplié ; `AlphaBlend` appelé **1:1** sur une DIB de la taille réduite. Cadrage `largeur_max × hauteur_max` avec conservation du ratio et centrage ; hauteur calculée depuis **343/276**, plus de boîte carrée. | `reduire_rgba` ~l.685-738 ; `dessiner_rose` ~l.739-820 ; appelants ~l.1435, ~l.1558-1567 |
| Thème update | `ROUGE_VIF = 0xFF2D1A` (alias explicite de l'action), `VERT_LIME_VIF = 0x9CFF00`, `VERT_LIME_VIF_CLAIR = 0xC4FF6A`. | constantes ~l.414-440 |

Extraits après, pour les trois points :

```rust
// coches
if coche_lisse(dc, cx, cy, d, couleur, epaisseur) { return; }

// titre
let marque_px = points_ecrits(dpi, 36, echelle);
let h_marque = (s(56)).max(marque_px + s(12));
let centre_marque = y_marque + s(28);
let haut_marque = centre_marque - h_marque / 2;
let mut rl = RECT { left: 0, top: haut_marque, right: lateral_w,
                    bottom: haut_marque + h_marque };

// logo
let facteur = (largeur_max as f64 / sw as f64).min(hauteur_max as f64 / sh as f64);
let dw = ((sw as f64 * facteur).round() as i32).max(1).min(largeur_max);
let dh = ((sh as f64 * facteur).round() as i32).max(1).min(hauteur_max);
let Some(rgba) = reduire_rgba(&pixels, largeur, hauteur,
                              sx0, sy0, sw, sh, dw as usize, dh as usize) else { return false; };
AlphaBlend(dc, dx, dy, dw, dh, memoire, 0, 0, dw, dh, melange);
```

## 3. Thème rouge vif / vert lime — où il s'applique

| Élément de la fenêtre de mise à jour | Avant | Après |
|---|---|---|
| Bouton principal (« Mettre à jour » / « Installer ») | `ROUGE` | `ROUGE_VIF` (survol `ROUGE_CLAIR`, texte blanc) |
| Bouton secondaire de la fenêtre d'update | `VERT_LIME` / `VERT_LIME_CLAIR` | `VERT_LIME_VIF` / `VERT_LIME_VIF_CLAIR` (texte sombre) |
| Carte « installation détectée » + libellé | `VERT_LIME` / `TEXTE` | `VERT_LIME_VIF` / `VERT_LIME_VIF` |
| Carte de statut Installation | `ROUGE_CLAIR` | `ROUGE_VIF` si échec, `VERT_LIME_VIF` en progression |
| Segment de barre de progression | `ROUGE_CLAIR` | `VERT_LIME_VIF` |
| Bouton « Ouvrir l'outil PATH » | `VERT_LIME` | `VERT_LIME_VIF` |
| Carte « Terminé » + coche | `VERT` | `VERT_LIME_VIF` + coche antialiasée lime |
| Statut dans la barre d'état | `TEXTE_DOUX` | `VERT_LIME_VIF` en succès, `ROUGE_VIF` en échec (sinon inchangé) |

Les autres vues (accueil, licence, options hors update, outil PATH) gardent
leurs couleurs historiques. `theme_update = etat.existant || vue ∈ {Installation, Termine}`.

## 4. Compilation / preuves

Commandes exécutées le 19/11/2026, dans `/tmp/opencode/la_agents` ou `rose/` :

1. **Type-check du VRAI `setup_gui.rs` sur cible Windows** — le `cargo check
   --target x86_64-pc-windows-gnu` complet est bloqué avant la compilation Rust
   par `build.rs` : `x86_64-w64-mingw32-gcc` absent, puis `windows.h` absent avec
   `gnullvm`, et aucun SDK xwin local. Contournement : harnais
   `/tmp/opencode/la_agents/harness.rs` qui inclut les VRAIS `setup_gui.rs`,
   `png.rs`, `paint.rs`, `inflate.rs` (setup_core stubé, API identique) :

   ```
   rustc --edition 2024 --target x86_64-pc-windows-gnu --crate-type lib \
     --emit=metadata -A warnings -o harness_after.rmeta harness.rs
   ```

   Résultat : **exit 0**, `harness_after.rmeta` produit. Le même harnais compile
   la version AVANT (`harness_baseline.rmeta`, exit 0) — la différence est donc
   bien dans le fichier modifié, et le module type-check sur la cible Windows.
   `cargo check --release` (Linux, module `#[cfg(windows)]` exclu) : **exit 0**.
2. **Tests** : `cargo test --release --offline` → 217 + 201 + 6 + 1 tests passés,
   **0 échec**, 6 ignorés au total.
3. **Algorithme de réduction exécuté** : la fonction `reduire_rgba` a été extraite
   telle quelle du fichier modifié (`sed -n '685,738p'`) dans
   `/tmp/opencode/la_agents/reduire_extrait.rs`, compilée et lancée sur Linux avec
   le vrai PNG :

   ```
   OK: reduction 276x343 -> 110x137; alpha max du bas = 212
   OK: cadrage reel 264x337 -> 107x137; alpha max du bas = 212
   ```

   (tests unitaires 2×2 : moyenne rouge = `[255,0,0,255]`, moyenne avec un pixel
   transparent = `[191,0,0,191]`).

## 5. Ce qui reste à tester sur Windows réel

* Le rendu antialiasé des coches (présence de GDI+ au moment du `WM_PAINT` ;
  repli GDI prévu sinon).
* L'absence de crop du « ROSE » à 125 % / 150 % et après redimensionnement.
* Le cadrage complet de la tige du logo et l'absence de halo (alpha prémultiplié).
* Les couleurs réelles du thème (conversion `COLORREF` 0x00BBGGRR → vérifiée
  statiquement par `couleur()`, pas observée à l'écran).
* Le lien complet `rose-setup.exe` (nécessite SDK Windows / MinGW + en-têtes,
  indisponibles ici) et l'installeur packagé.

Aucun de ces points n'est présenté comme « vu » ou « vérifié visuellement ».
