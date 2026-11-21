# LOGO4 — pied de la tige coupé dans le launcher : correctif SYSTÉMIQUE (vague 81)

Date : 21/11/2026. Périmètre traité : `installateur/velopack/embed-assets.py`,
`installateur/velopack/rose-installeur.png` (régénéré), `rose/src/assets_installeur.rs`
(régénéré), `rose/src/paint.rs`, `rose/src/setup_gui.rs`, `rose/src/png.rs`, ce
document. **Aucun commit, aucune publication.** Les modifications concurrentes
de `rose/src/velopack.rs`, `rose/src/bin/rose-tray.rs` et de la zone tray de
`setup_gui.rs` (TRAY4, autre agent) n'ont PAS été touchées.

## 1. Cause réelle, prouvée sur les octets — ce n'était PAS `bornes_opaques`

LOGO3 supposait que `bornes_opaques()` s'arrêtait avant le dernier pixel du
pied. **Faux.** Mesure directe des octets de l'ancien PNG :

| Fichier | md5 | Taille | bornes alpha > 4 | Dernière ligne opaque |
|---|---|---|---|---|
| ancien `rose-installeur.png` | `1b5bbf4380a7c772c2a507a2bd3d5311` | 276 × 343 | (6, 6, 270, 343) = 264 × 337 | y=342, x 149..165, **17 px** = tige seule |
| référence `rose/rose.ico` (256) | `2ea01977bda2ac7ec30b07fe030e9be5` | 256 × 256 | (35, 16, 214, 249) = 179 × 233 | y=248, x 118..156, **39 px** = pied ; tige 12 px en y=242 |

`bornes_opaques()` incluait bien le dernier pixel (y1 = 343). Le pied **n'existait
pas dans le PNG** : il avait été supprimé à la génération.

Preuve sur le visuel source `docs/assets/img/logo-rose.png` (1400 × 1400) :
- tige : x 710..743 (34 px), jusqu'en y=902 ;
- **pied (base en T) : x 670..784 (115 px), lignes 904..916** ;
- vide : lignes 917..1039 ;
- mot « ROSE » gravé : x 320..1081, à partir de y=1040.

Or `embed-assets.py` portait `FRACTION_BAS = 0.645` → `bas = int(1400 × 0.645)
= 903`, avec le commentaire « le mot ROSE commence à la ligne 904 ». Erreur
d'interprétation : la ligne 904 est le **pied**, pas le mot. La coupe à 903
produisait une tige à bout net, exactement le symptôme signalé.

## 2. Correctif appliqué

1. **`embed-assets.py`** : `FRACTION_BAS` supprimé. Nouvelle fonction `bord_bas()`
   qui **lit les octets** : première ligne non vide, puis premier trou vertical
   d'au moins 12 lignes vides → `bas = 917` (pied inclus jusqu'en 916, mot
   « ROSE » exclu, premier vide 917). La ligne `y1 = min(y1, maxy + 1)`, qui
   rognait le pied, est supprimée.
2. **PNG régénéré** par `python3 installateur/velopack/embed-assets.py` :
   184 × 233 (facteur 3×), md5 `fbb4828551afb1a6285a4f9c04be098d`.
3. **`paint.rs`** : bornes et ratio **recalculés depuis ces octets** (aucune
   valeur écrite à la main) — `ROSE_BORNES = (4, 4, 180, 233)`, contenu
   `ROSE_L × ROSE_H = 176 × 229`, `RATIO_ROSE = 229/176 = 1,301136364`
   (avant : 337/264 = 1,276515152). Marge basse 8 → **10 px** (+2 demandés),
   total 4 + 10 = **14 px**.
4. **`boite_source_rose()`** ajoutée dans `paint` (module PUR) : c'est la seule
   fonction qui calcule le rectangle source `(sx0, sy0, sw, sh)`. `dessiner_rose`
   l'appelle, et le test l'appelle aussi — il n'y a plus de formule parallèle
   entre le dessin et la preuve.

### Bornes AVANT / APRÈS (preuve octets)

| | AVANT | APRÈS |
|---|---|---|
| PNG | 276 × 343, `1b5bb…` | 184 × 233, `fbb48…` |
| bornes opaques (α > 4) | (6, 6, 270, 343) | **(4, 4, 180, 233)** |
| contenu | 264 × 337 | **176 × 229** |
| ratio | 337/264 | **229/176** |
| dernière ligne | 17 px (tige, pas de pied) | **38–39 px (pied)** |
| tige 12 lignes plus haut | 17 px | 12 px |
| référence ico (256) | 39 px de pied, 12 px de tige | **conforme** |
| marges | 4 + 8 = 12 | **4 + 10 = 14** |

Les trois seuils α > 0, α > 4 et α > 128 donnent les mêmes bornes sur le
nouveau PNG : le pied est opaque, pas un artefact anti-aliasé.

## 3. Tests

* `paint::tests::png_opaque_176x229_et_reduction_garde_le_pied` : lit les octets,
  vérifie les bornes constantes, le ratio, et que la dernière ligne réduite
  contient encore le pied.
* **Nouveau** `paint::tests::dernier_pixel_opaque_le_pied_est_dans_la_boite_source`
  (mission LOGO4) : cherche le dernier pixel opaque **directement dans les
  octets, sans passer par `bornes_opaques`**, prouve que sa ligne est au moins
  2× plus large que la tige (base en T), puis vérifie qu'il est dans la boîte
  source renvoyée par `boite_source_rose()` — la même que `dessiner_rose()`
  utilise — et que `sy0 + sh = dernier_y + 1`. Enfin, la réduction garde au
  moins 8 px de pied sur la dernière ligne. **Ce test échouait sur l'ancien PNG**
  (base en T absente) : il verrouille la régression.
* `boite_destination_contient_tout_le_contenu_opaque_et_les_marges` : **144
  combinaisons** (6 largeurs × 8 hauteurs × 3 DPI), marge basse exacte 10 px.
* `png.rs` : dimensions et taille de flux mises à jour (184 × 233).

## 4. Vérifications exécutées (constats datés du 21/11/2026)

| Commande | Résultat |
|---|---|
| `cargo test --release` | **exit 0 — 0 échec** (salve finale : lib 296 pass / 3 ignored ; bin 306 pass / 3 ignored ; shell 6 ; tray 9 ; intégration 8 + 1) |
| `cargo test --release --lib paint::tests` | **13 passed, 0 failed** |
| Replica Python des formules (hors dépôt) | **144/144**, marge bas exacte 10 px |
| `cargo xwin check --release --target x86_64-pc-windows-msvc` | **exit 0** (adaptateur clang local `/tmp/opencode/bin/clang`, aucun fichier du dépôt modifié à cette fin) |
| `./rose/rose rir compilateur/emetteur.rose` | md5 `b3ed9262dd54776a6dbd07bc12368d0e`, 263 520 o, stderr 0 |
| `./rose/rose compilateur/emetteur.rose compilateur/emetteur.rose` | **même md5 `b3ed9262…`**, 263 520 o, stderr 0, `cmp` identique |

Transitoires observés pendant la session (liés aux modifications CONCURRENTES
de l'autre agent dans `main.rs`/`front`, pas au logo) : un échec du test AOT
`link_natif::tests::elf_avec_valeur_exacte_ecrit_le_texte_memorise` sur une
salve, puis `error[E0599] as_afficher_expr` sur une compilation en pleine
modification. Les salves finales, après stabilisation de l'autre chantier,
sont **toutes vertes** (compilation incluse). Aucun lien avec LOGO4.

Table géométrie (replica indépendante, mêmes formules que `paint`) :

| Fenêtre / DPI | Avant : boîte | Avant : dessin | Avant : marge bas | Après : boîte | Après : dessin | Après : marge bas |
|---|---|---|---:|---|---|---:|
| 1120×780 / 96  | 110×152 | 110×140 | 8 px | 110×157 | 110×143 | **10 px** |
| 806×562 / 96   |  79×113 |  79×101 | 8 px |  79×117 |  79×103 | **10 px** |
| 564×600 / 96   |  55×82  |  55×70  | 8 px |  55×86  |  55×72  | **10 px** |
| 736×512 / 96   |  72×104 |  72×92  | 8 px |  72×108 |  72×94  | **10 px** |
| 1120×600 / 144 |  85×121 |  85×109 | 8 px |  85×125 |  85×111 | **10 px** |
| 1920×1080 / 192| 152×206 | 152×194 | 8 px | 152×212 | 152×198 | **10 px** |

## 5. État des lieux

* **Fait** : cause systémique identifiée sur les octets (coupe à 903 au lieu de
  917), pied réintégré au PNG régénéré, bornes et ratio dérivés des octets,
  marge basse +2 px, boîte source unique partagée dessin/test, test dédié au
  dernier pixel opaque, 144 combinaisons vertes, suite complète verte,
  cross-compile Windows vert, point fixe `b3ed9262` ×2.
* **Non fait** : capture visuelle réelle sous Windows (Wine absent ici), aucun
  aperçu PNG régénéré (ils ne sont pas produits par `setup_gui`). La preuve est
  donc binaire (octets + tests), pas une photo d'écran.
