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
}
