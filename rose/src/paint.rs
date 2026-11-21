//! Calculs de TRACÉ de l'installeur, indépendants de Win32.
//!
//! Ce module existe parce que le bug « fenêtre ouverte avec ses seules
//! bordures, sans intérieur » est invisible sous Linux : `setup_gui` n'est
//! compilé que sous Windows, et un plantage dans `WM_PAINT` laisse la fenêtre
//! VIDE (Windows la crée avec son fond, puis le fil de messages meurt : la
//! fenêtre reste à l'écran, sans contenu, jusqu'à ce que l'utilisateur la
//! ferme — « transparente puis disparaît au bout d'un moment »).
//!
//! Tout ce qui peut produire un indice hors bornes, une division par zéro ou
//! une boucle infinie est donc calculé ICI, en Rust ordinaire, et testé sous
//! Linux. `setup_gui` ne fait plus que traduire ces valeurs en appels GDI.
//!
//! Licence MIT.

/// Découpe la liste `src` en lignes de texte selon la police mesurée.
///
/// `mesurer(&str) -> i32` rend la largeur en pixels d'un texte dans la police
/// courante. Le découpage se fait sur les ESPACES, jamais au milieu d'un mot ;
/// un mot plus large que la zone reste sur sa propre ligne (il sera rogné par
/// `DT_END_ELLIPSIS` plutôt que de boucler).
///
/// `\n` impose une fin de ligne. Une ligne vide (double `\n`) produit un
/// paragraphe vide, ce qui conserve les espacements voulus par l'auteur.
pub fn lignes_ajustees(src: &str, largeur: i32, mesurer: &dyn Fn(&str) -> i32) -> Vec<String> {
    let mut sortie = Vec::new();
    if largeur <= 0 {
        return sortie;
    }
    for paragraphe in src.split('\n') {
        if paragraphe.is_empty() {
            sortie.push(String::new());
            continue;
        }
        let mut courante = String::new();
        for mot in paragraphe.split(' ') {
            if mot.is_empty() {
                // espaces multiples : on n'en garde qu'un
                continue;
            }
            let essai = if courante.is_empty() {
                mot.to_string()
            } else {
                format!("{courante} {mot}")
            };
            if mesurer(&essai) <= largeur {
                courante = essai;
            } else {
                if !courante.is_empty() {
                    sortie.push(std::mem::take(&mut courante));
                }
                courante = mot.to_string();
            }
        }
        sortie.push(courante);
    }
    sortie
}

/// Hauteur totale d'un texte multiligne, en pixels.
///
/// `hauteur_ligne` = interligne de la police (en pixels). On ajoute une ligne
/// d'avance : `DrawText` mesure toujours une ligne de plus que le nombre de
/// `\n` (la dernière ligne n'a pas de saut).
pub fn hauteur_texte(src: &str, largeur: i32, mesurer: &dyn Fn(&str) -> i32, hauteur_ligne: i32) -> i32 {
    let n = lignes_ajustees(src, largeur, mesurer).len() as i32;
    (n.max(1)) * hauteur_ligne.max(1)
}

/// Rectangle d'une carte dont la hauteur suit son contenu.
///
/// `minimum` = hauteur plancher (une carte ne doit jamais être plus petite
/// que son contenu décoratif : coche, marge haute et basse).
pub fn hauteur_carte(hauteur_contenu: i32, marge: i32, minimum: i32) -> i32 {
    (hauteur_contenu + 2 * marge.max(0)).max(minimum)
}

/// Position horizontale du segment animé de la barre de progression
/// indéterminée, en coordonnées RELATIVES à la piste (`0..largeur`).
///
/// `phase` avance à chaque `WM_TIMER`. Le résultat est borné : aucune
/// coordonnée négative, aucun dépassement à droite, donc aucun appel GDI avec
/// un rectangle inversé (que `FillRect` ignore silencieusement, laissant la
/// barre immobile).
pub fn segment_progression(phase: i32, largeur: i32) -> (i32, i32) {
    if largeur <= 0 {
        return (0, 0);
    }
    let segment = (largeur / 4).max(1);
    let course = largeur + segment;
    let debut = phase.rem_euclid(course);
    let a = (debut - segment).clamp(0, largeur);
    let b = debut.clamp(0, largeur);
    if b > a { (a, b) } else { (0, 0) }
}

/// Échelle du tracé pour une fenêtre de `w × h`, sur la grille 1120 × 780.
///
/// `min` des deux rapports, jamais zéro : une fenêtre de hauteur nulle
/// (premier `WM_PAINT` avant `WM_SIZE`) donnerait une échelle nulle, donc des
/// polices de taille 0 et un `CreateFontW` qui échoue.
pub fn echelle_tracé(w: i32, h: i32) -> f64 {
    let e = (w as f64 / 1120.0).min(h as f64 / 780.0);
    if !e.is_finite() || e <= 0.0 { 1.0 } else { e }
}

// ── Rose embarquée : géométrie PURE (aucun appel GDI) ──────────────────────
//
// Décision SYSTÉMIQUE (vague 80) : on arrête les rustines successives sur la
// boîte du logo. Tout ce qui dimensionne la rose vit ICI, en Rust ordinaire,
// donc testable sous Linux avec `cargo test`. `setup_gui` ne fait plus que
// traduire ces valeurs en appels `AlphaBlend`.

/// Ratio du CONTENU OPAQUE du PNG embarqué, mesuré UNE FOIS :
/// `installateur/velopack/rose-installeur.png` = 276 × 343, bornes alpha
/// x 6..270, y 6..343 ⇒ contenu utile 264 × 337 (la tige va au dernier pixel).
///
/// ⚠ Ce n'est PAS le ratio du PNG natif (343/276) : utiliser ce dernier
/// raccourcit la boîte de quelques pixels. La boîte SOURCE et la boîte
/// DESTINATION utilisent exactement le même ratio, sans aucun clamp
/// indépendant de la hauteur.
pub const RATIO_ROSE: f64 = 337.0 / 264.0;

/// Marge HAUTE fixe, en pixels de destination, au-dessus du contenu opaque.
pub const MARGE_HAUT_ROSE: i32 = 4;
/// Marge BASSE fixe, en pixels de destination, SOUS la pointe de la tige.
/// La pointe anti-aliasée posée sur le bord exact d'`AlphaBlend` disparaît :
/// 8 px sont réservés, quelle que soit la taille de la fenêtre ou le DPI.
pub const MARGE_BAS_ROSE: i32 = 8;
/// Total des marges inclus dans toute hauteur réservée à la rose.
pub const MARGE_TOTALE_ROSE: i32 = MARGE_HAUT_ROSE + MARGE_BAS_ROSE; // 12

/// Hauteur TOTALE à réserver pour une rose dont le contenu opaque mesure
/// `largeur` pixels de large : `round(largeur * 337/264) + 12`.
///
/// Les 12 px (4 haut + 8 bas) sont DÉJÀ inclus : c'est cette hauteur qu'on
/// retranche des cartes AVANT de poser le titre. On ne la clamped jamais
/// seule (voir `boite_rose` : c'est la LARGEUR qui diminue).
pub fn hauteur_rose_requise(largeur: i32) -> i32 {
    if largeur <= 0 { return 0; }
    (largeur as f64 * RATIO_ROSE).round() as i32 + MARGE_TOTALE_ROSE
}

/// Boîte de destination `(largeur, hauteur_totale)` d'une rose qui tient dans
/// `hauteur_disponible`, sans JAMAIS couper la tige.
///
/// Si la hauteur manque, on réduit la LARGEUR (donc la hauteur requise) par
/// recherche binaire sur `hauteur_rose_requise`, fonction monotone. Le
/// rectangle obtenu contient toujours le contenu opaque ENTIER plus les
/// 4 + 8 px de marge, et son ratio est exactement 264:337 pour le contenu.
pub fn boite_rose(largeur_souhaitee: i32, hauteur_disponible: i32) -> (i32, i32) {
    if largeur_souhaitee <= 0 || hauteur_disponible < MARGE_TOTALE_ROSE + 1 {
        return (0, 0);
    }
    let mut lo = 0i32;
    let mut hi = largeur_souhaitee;
    while lo < hi {
        let mid = lo + (hi - lo + 1) / 2;
        if hauteur_rose_requise(mid) <= hauteur_disponible {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    if lo == 0 { (0, 0) } else { (lo, hauteur_rose_requise(lo)) }
}

/// Hauteur en pixels d'une taille en points, mise à l'échelle de la fenêtre.
///
/// Plancher 13 px : en dessous, les glyphes deviennent illisibles.
pub fn points_ecrits(dpi: i32, points: i32, echelle: f64) -> i32 {
    let base = (points * dpi) / 72;
    let e = if echelle.is_finite() && echelle > 0.0 { echelle } else { 1.0 };
    ((base as f64 * e).round() as i32).max(13)
}

/// Géométrie verticale du bloc « logo / titre / sous-titre / étapes » du
/// panneau latéral. Fonction PURE : aucun appel GDI, testable sur toutes les
/// combinaisons DPI × taille de fenêtre.
///
/// La hauteur du logo est RÉSERVÉE avant tout (via `boite_rose`) : la rose
/// n'est jamais réduite en hauteur seule, et le titre ne peut pas monter sur
/// la marge basse de 8 px.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeometrieMarque {
    pub plancher: i32,      // rien du panneau ne descend sous cette ligne
    pub largeur_logo: i32,  // largeur du contenu opaque (= largeur de la boîte)
    pub hauteur_logo: i32,  // HAUTEUR TOTALE réservée = contenu + 12 px de marge
    pub place_logo: i32,    // place disponible (hauteur_logo <= place_logo)
    pub haut_titre: i32,
    pub bas_titre: i32,
    pub haut_sous_titre: i32,
    pub bas_sous_titre: i32,
    pub y_etapes: i32,
}

pub fn geometrie_marque(ht: i32, dpi: i32, echelle: f64) -> GeometrieMarque {
    let e = if echelle.is_finite() && echelle > 0.0 { echelle } else { 1.0 };
    let s = |v: i32| -> i32 { (v as f64 * e).round() as i32 };

    // Cartes du bas : mêmes réservations que dans `dessiner_decore`.
    let bas = ht - s(30) - s(18);
    let carte2_top = bas - s(38);
    let carte1_top = carte2_top - s(46);
    let plancher = carte1_top - s(16);

    // Hauteur RÉELLE de la cellule du titre (la police suit le DPI).
    let marque_px = points_ecrits(dpi, 36, e);
    let h_marque = s(72).max(marque_px + s(28));
    // Marge logo → titre : la montée de Kare ne touche jamais la tige.
    let marge_marque = s(26).max(h_marque / 2 - s(34) + s(4));

    // Place disponible pour la rose, calculée AVANT de poser le titre :
    // `hauteur_logo` inclut déjà les 4 + 8 px de marge.
    let apres_marque = s(34) + h_marque / 2 + s(10) + s(56) + s(10);
    let place_logo = (plancher - s(78) - marge_marque - apres_marque).max(0);
    // ⚠ Plus de `hauteur_logo = min(hauteur_voulue, place_logo)` : ce clamp
    // sur la hauteur seule changeait le ratio et rapprochait la tige du bord.
    // Ici c'est `boite_rose` qui réduit la largeur si la place manque.
    let (largeur_logo, hauteur_logo) = boite_rose(s(110).min(264), place_logo);

    let y_marque = s(78) + hauteur_logo + marge_marque;
    let haut_titre = y_marque + s(34) - h_marque / 2;
    let bas_titre = haut_titre + h_marque;
    let haut_sous_titre = bas_titre + s(10);
    let bas_sous_titre = haut_sous_titre + s(56);
    let y_etapes = bas_sous_titre + s(10);

    GeometrieMarque {
        plancher, largeur_logo, hauteur_logo, place_logo,
        haut_titre, bas_titre, haut_sous_titre, bas_sous_titre, y_etapes,
    }
}

/// Bornes du contenu visible d'une image RGBA (alpha non nul).
/// Renvoie (x0, y0, x1, y1) — rectangle semi-ouvert. Une seule lecture des
/// octets suffit : le ratio `RATIO_ROSE` en est déduit une fois pour toutes.
pub fn bornes_opaques(largeur: usize, hauteur: usize, pixels: &[u8]) -> (usize, usize, usize, usize) {
    let mut x0 = largeur;
    let mut y0 = hauteur;
    let mut x1 = 0usize;
    let mut y1 = 0usize;
    for y in 0..hauteur {
        for x in 0..largeur {
            if pixels[(y * largeur + x) * 4 + 3] > 4 {
                if x < x0 { x0 = x; }
                if x + 1 > x1 { x1 = x + 1; }
                if y < y0 { y0 = y; }
                if y + 1 > y1 { y1 = y + 1; }
            }
        }
    }
    if x1 <= x0 || y1 <= y0 { (0, 0, 0, 0) } else { (x0, y0, x1, y1) }
}

/// Réduction RGBA par MOYENNE DE SURFACE, en pur Rust.
///
/// `AlphaBlend` force COLORONCOLOR et supprimerait des lignes entières : on
/// réduit nous-mêmes le contenu opaque (chaque pixel source contribue), puis
/// on appelle `AlphaBlend` à taille ÉGALE. La boîte source est le contenu
/// opaque entier (264 × 337), la boîte destination a le MÊME ratio.
///
/// `pixels` est du RGBA 8 bits NON prémultiplié ; la sortie est du RGBA
/// PRÉMULTIPLIÉ, prêt pour `AC_SRC_ALPHA`.
pub fn reduire_rgba(pixels: &[u8], largeur: usize, hauteur: usize,
                    sx0: usize, sy0: usize, sw: usize, sh: usize,
                    cible_l: usize, cible_h: usize) -> Option<Vec<u8>> {
    if cible_l == 0 || cible_h == 0 || sw == 0 || sh == 0 { return None; }
    if sx0.checked_add(sw)? > largeur || sy0.checked_add(sh)? > hauteur { return None; }
    if pixels.len() < largeur * hauteur * 4 { return None; }
    let mut sortie = vec![0u8; cible_l * cible_h * 4];
    for oy in 0..cible_h {
        let y0_abs = sy0 + oy * sh / cible_h;
        let y1_abs = sy0 + ((oy + 1) * sh / cible_h).max(oy * sh / cible_h + 1);
        for ox in 0..cible_l {
            let x0_abs = sx0 + ox * sw / cible_l;
            let x1_abs = sx0 + ((ox + 1) * sw / cible_l).max(ox * sw / cible_l + 1);
            let mut sr = 0u64;
            let mut sg = 0u64;
            let mut sb = 0u64;
            let mut sa = 0u64;
            for py in y0_abs..y1_abs {
                for px in x0_abs..x1_abs {
                    let k = (py * largeur + px) * 4;
                    let a = pixels[k + 3] as u64;
                    sr += pixels[k] as u64 * a;
                    sg += pixels[k + 1] as u64 * a;
                    sb += pixels[k + 2] as u64 * a;
                    sa += a;
                }
            }
            let n = ((x1_abs - x0_abs) * (y1_abs - y0_abs)) as u64;
            let div_premul = n * 255;
            let o = (oy * cible_l + ox) * 4;
            sortie[o] = (sr / div_premul) as u8;
            sortie[o + 1] = (sg / div_premul) as u8;
            sortie[o + 2] = (sb / div_premul) as u8;
            sortie[o + 3] = (sa / n) as u8;
        }
    }
    Some(sortie)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mesure grossière : 8 px par caractère, comme Segoe UI à 9 pt.
    fn mesurer(s: &str) -> i32 {
        s.chars().count() as i32 * 8
    }

    #[test]
    fn decoupe_sur_les_espaces_jamais_au_milieu_d_un_mot() {
        let l = lignes_ajustees("Le langage compilé en français", 80, &mesurer);
        assert!(l.len() > 1, "un texte de 30 caractères doit tenir sur plusieurs lignes");
        for ligne in &l {
            assert!(mesurer(ligne) <= 80 || !ligne.contains(' '),
                    "ligne trop large et coupée dans un mot : {ligne:?}");
        }
        // Aucun mot perdu.
        let joint = l.join(" ");
        assert_eq!(joint, "Le langage compilé en français");
    }

    #[test]
    fn cartes_options_mesurees_sur_la_vraie_description() {
        // Régression de la capture Windows : « Un seul timer Windows toutes
        // les 6 heures… » coupée en bas de carte because `h_opt = 84` était
        // constant. La bonne hauteur se MESURE avec `hauteur_texte`.
        let (_, titre, desc) = crate::velopack::ModeMiseAJour::options_ui()[2];
        let largeur = 320; // fenêtre étroite
        let h_titre = hauteur_texte(titre, largeur, &mesurer, 19);
        let h_desc = hauteur_texte(desc, largeur, &mesurer, 17);
        let h_opt = 10 + h_titre + 4 + h_desc + 10;
        assert!(h_desc >= 3 * 17, "description sous-estimée : {h_desc} px");
        assert!(h_opt > 84, "carte trop courte pour la description : {h_opt} px");
    }

    #[test]
    fn respecte_les_sauts_de_ligne() {
        let l = lignes_ajustees("a\n\nb", 1000, &mesurer);
        assert_eq!(l, vec!["a".to_string(), String::new(), "b".to_string()]);
    }

    #[test]
    fn un_mot_plus_large_reste_seul() {
        let l = lignes_ajustees("anticonstitutionnellement x", 40, &mesurer);
        assert_eq!(l[0], "anticonstitutionnellement");
        assert_eq!(l[1], "x");
    }

    #[test]
    fn largeur_nulle_ne_boucle_pas() {
        // Cas mesuré : une fenêtre de largeur 0 au premier WM_PAINT.
        assert!(lignes_ajustees("texte", 0, &mesurer).is_empty());
        assert!(lignes_ajustees("texte", -10, &mesurer).is_empty());
    }

    #[test]
    fn hauteur_texte_compte_les_lignes() {
        let h = hauteur_texte("a\nb\nc", 1000, &mesurer, 12);
        assert_eq!(h, 36);
        assert_eq!(hauteur_texte("", 1000, &mesurer, 12), 12);
    }

    #[test]
    fn segment_de_progression_toujours_dans_la_piste() {
        for phase in -500..2000 {
            let (a, b) = segment_progression(phase, 300);
            assert!(a >= 0 && b <= 300, "segment hors piste : {a}..{b} (phase {phase})");
            assert!(b >= a);
        }
        assert_eq!(segment_progression(0, 0), (0, 0));
    }

    #[test]
    fn echelle_jamais_nulle() {
        assert_eq!(echelle_tracé(0, 0), 1.0);
        assert_eq!(echelle_tracé(1120, 780), 1.0);
        // Fenêtre plus basse que le gabarit : l'échelle suit la HAUTEUR.
        assert!(echelle_tracé(1120, 390) < 1.0);
    }

    #[test]
    fn hauteur_carte_ne_descend_pas_sous_le_minimum() {
        assert_eq!(hauteur_carte(0, 16, 150), 150);
        assert_eq!(hauteur_carte(200, 16, 150), 232);
    }

    // ── Rose : preuve que la tige n'est jamais coupée ───────────────────────

    /// Grille exhaustive demandée : 6 largeurs × 8 hauteurs (600 → 1080) ×
    /// 3 DPI (96 / 120 / 144) = 144 combinaisons. Pour CHACUNE, on recalcule
    /// le rectangle de destination exact de `dessiner_rose` et on vérifie
    /// qu'il contient le contenu opaque ENTIER + 4 px en haut + 8 px en bas.
    #[test]
    fn boite_destination_contient_tout_le_contenu_opaque_et_les_marges() {
        let largeurs = [560, 564, 736, 806, 1120, 1920];
        let hauteurs = [600, 660, 720, 780, 840, 900, 960, 1080];
        let dpis = [96, 120, 144];
        let mut combinaisons = 0usize;
        for largeur_fenetre in largeurs {
            for hauteur_fenetre in hauteurs {
                for dpi in dpis {
                    combinaisons += 1;
                    let e = echelle_tracé(largeur_fenetre, hauteur_fenetre);
                    let g = geometrie_marque(hauteur_fenetre, dpi, e);
                    let s = |v: i32| -> i32 { (v as f64 * e).round() as i32 };
                    let ctx = format!("{largeur_fenetre}×{hauteur_fenetre} {dpi} DPI");
                    let y_boite = s(78); // origine verticale passée à dessiner_rose

                    // La hauteur réservée est la hauteur REQUISE, jamais un
                    // clamp indépendant de la largeur.
                    assert_eq!(g.hauteur_logo, hauteur_rose_requise(g.largeur_logo),
                               "hauteur réservée incohérente : {ctx}");
                    assert!(g.hauteur_logo <= g.place_logo,
                            "rose plus haute que sa place : {ctx}");
                    assert!(g.largeur_logo > 0, "rose absente : {ctx}");

                    // Même ratio pour la boîte SOURCE (264 × 337) et la boîte
                    // DESTINATION du contenu : largeur_logo × contenu_h.
                    let contenu_h = g.hauteur_logo - MARGE_TOTALE_ROSE;
                    assert_eq!(contenu_h, (g.largeur_logo as f64 * RATIO_ROSE).round() as i32,
                               "ratio source/destination différent : {ctx}");
                    // C'est exactement ce que `dessiner_rose` calcule :
                    // facteur = min(largeur/264, contenu_h/337) ⇒ dw, dh.
                    let facteur = (g.largeur_logo as f64 / 264.0)
                        .min(contenu_h as f64 / 337.0);
                    let dw = (264.0 * facteur).round() as i32;
                    let dh = (337.0 * facteur).round() as i32;
                    assert_eq!((dw, dh), (g.largeur_logo, contenu_h),
                               "contenu réduit plus petit que la boîte : {ctx}");

                    // Rectangle réel : 4 px en haut, 8 px en bas, contenu entier.
                    let dy = y_boite + MARGE_HAUT_ROSE + (contenu_h - dh) / 2;
                    assert_eq!(dy, y_boite + MARGE_HAUT_ROSE, "marge haute : {ctx}");
                    assert_eq!(y_boite + g.hauteur_logo - (dy + dh), MARGE_BAS_ROSE,
                               "marge basse absente sous la tige : {ctx}");
                    assert!(dy >= y_boite && dy + dh <= y_boite + g.hauteur_logo,
                            "contenu hors de la boîte : {ctx}");
                    // Le titre commence AU PLUS TÔT au bas de la boîte : il
                    // laisse donc les 8 px de marge au-dessus de la tige.
                    assert!(g.haut_titre >= y_boite + g.hauteur_logo,
                            "titre sur la marge/tige : {ctx}");
                    assert!(g.haut_sous_titre >= g.bas_titre, "titre recouvert : {ctx}");
                    assert!(g.y_etapes <= g.plancher, "étapes sous les cartes : {ctx}");
                }
            }
        }
        assert_eq!(combinaisons, 144, "grille incomplète");
    }

    /// Quand la hauteur manque, c'est la LARGEUR qui baisse (donc la hauteur
    /// requise), jamais la hauteur seule : le ratio reste 264:337 et les 12 px
    /// de marge restent présents.
    #[test]
    fn place_insuffisante_reduit_la_largeur_pas_la_hauteur() {
        assert_eq!(boite_rose(110, 152), (110, 152)); // ajuste juste
        assert_eq!(boite_rose(110, 151), (109, 151)); // 110 demanderait 152
        assert_eq!(boite_rose(110, 13), (1, 13));     // plus petit cas valide
        assert_eq!(boite_rose(110, 12), (0, 0));      // pas de place pour la marge
        assert_eq!(boite_rose(0, 500), (0, 0));
        for dispo in 13..400 {
            let (l, h) = boite_rose(264, dispo);
            if l > 0 {
                assert_eq!(h, hauteur_rose_requise(l));
                assert!(h <= dispo, "boîte trop haute pour {dispo}");
                assert_eq!(l, 264.min(l));
                // Une largeur de plus ne tiendrait pas : c'est bien la
                // largeur qui a été réduite, pas la hauteur clampée.
                if l < 264 {
                    assert!(hauteur_rose_requise(l + 1) > dispo);
                }
            }
        }
    }

    /// Preuve sur les OCTETS du PNG réellement embarqué : la tige va jusqu'au
    /// dernier pixel, le ratio 264:337 est exact, et la réduction par moyenne
    /// de surface garde sa pointe dans la dernière ligne — même boîte source,
    /// même boîte destination, aucune ligne supprimée.
    #[test]
    fn png_opaque_264x337_et_reduction_garde_la_pointe() {
        let octets = include_bytes!("../../installateur/velopack/rose-installeur.png");
        let Some((l, h, pixels)) = crate::png::decoder_png(octets) else {
            panic!("PNG de la rose illisible");
        };
        assert_eq!((l, h), (276, 343));
        let (x0, y0, x1, y1) = bornes_opaques(l, h, &pixels);
        assert_eq!((x0, y0, x1, y1), (6, 6, 270, 343),
                   "bornes_opaques doit inclure la tige jusqu'au dernier pixel");
        let (sw, sh) = (x1 - x0, y1 - y0);
        assert_eq!((sw, sh), (264, 337));
        assert!(((sh as f64 / sw as f64) - RATIO_ROSE).abs() < 1e-9);

        // Destination réelle pour une rose de 107 px : 107 × round(107*337/264).
        let dw = 107usize;
        let dh = (dw as f64 * RATIO_ROSE).round() as usize;
        let réduit = reduire_rgba(&pixels, l, h, x0, y0, sw, sh, dw, dh)
            .expect("réduction impossible");
        let derniers: Vec<u8> = (0..dw).map(|x| réduit[((dh - 1) * dw + x) * 4 + 3]).collect();
        assert!(derniers.iter().any(|a| *a > 4),
                "la pointe de la tige a disparu de la dernière ligne réduite");
        // Hauteur totale réservée = contenu + 12, quelle que soit la largeur.
        assert_eq!(hauteur_rose_requise(dw as i32), dh as i32 + MARGE_TOTALE_ROSE);
    }
}
