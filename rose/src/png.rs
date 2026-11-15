//! Décodeur PNG minimal (RGBA 8 bits), sans dépendance.
//!
//! L'installeur doit afficher la rose de la marque ; la seule source de
//! vérité est le PNG embarqué en ressource. Ce décodeur est maison (pas de
//! crate PNG) : il lit l'en-tête IHDR, concatène les IDAT, décompresse avec
//! `crate::inflate` et applique les filtres de ligne.
//!
//! Il vit dans son propre module — donc il est TESTABLE sous Linux, où
//! `setup_gui` n'est pas compilé. C'est ce qui a permis de vérifier que le
//! décodage produit bien des pixels opaques (et non une image entièrement
//! transparente, invisible à l'écran).
//!
//! Licence MIT.

/// Décode un PNG RGBA 8 bits en (largeur, hauteur, pixels RGBA).
/// Aucune dépendance : en-têtes locaux + DEFLATE maison.
pub fn decoder_png(donnees: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    if donnees.len() < 8 || &donnees[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let mut i = 8usize;
    let mut largeur = 0usize;
    let mut hauteur = 0usize;
    let mut compression = Vec::new();
    while i + 8 <= donnees.len() {
        let taille = u32::from_be_bytes([donnees[i], donnees[i + 1], donnees[i + 2], donnees[i + 3]]) as usize;
        let type_ = &donnees[i + 4..i + 8];
        let corps = donnees.get(i + 8..i + 8 + taille)?;
        match type_ {
            b"IHDR" => {
                largeur = u32::from_be_bytes([corps[0], corps[1], corps[2], corps[3]]) as usize;
                hauteur = u32::from_be_bytes([corps[4], corps[5], corps[6], corps[7]]) as usize;
                let profondeur = corps[8];
                let couleur = corps[9];
                if profondeur != 8 || couleur != 6 {
                    return None;
                }
            }
            b"IDAT" => compression.extend_from_slice(corps),
            b"IEND" => break,
            _ => {}
        }
        i += 12 + taille;
    }
    // ⚠ Les IDAT d'un PNG contiennent un flux **ZLIB** (en-tête de 2 octets
    // `78 DA`, puis DEFLATE), pas du DEFLATE brut. Appeler `decompresser`
    // (qui attend du DEFLATE nu) consommait l'en-tête comme des données :
    // le flux partait en vrille et le décodeur rendait `None`.
    // Conséquence mesurée : la rose de l'installeur ne s'affichait NI dans
    // la barre de titre, NI au-dessus de « ROSE » — le dessin était
    // silencieusement sauté.
    let brut = crate::inflate::decompresser_zlib(&compression)?;
    let pas = largeur * 4;
    if brut.len() < hauteur * (pas + 1) {
        return None;
    }
    let mut precedent = vec![0u8; pas];
    let mut pixels = Vec::with_capacity(pas * hauteur);
    for y in 0..hauteur {
        let filtre = brut[y * (pas + 1)];
        let ligne = &brut[y * (pas + 1) + 1..(y + 1) * (pas + 1)];
        let mut sortie = ligne.to_vec();
        for x in 0..pas {
            let a = if x >= 4 { sortie[x - 4] } else { 0 };
            let b = precedent[x];
            let c = if x >= 4 { precedent[x - 4] } else { 0 };
            match filtre {
                1 => sortie[x] = sortie[x].wrapping_add(a),
                2 => sortie[x] = sortie[x].wrapping_add(b),
                3 => sortie[x] = sortie[x].wrapping_add(((a as u16 + b as u16) >> 1) as u8),
                4 => {
                    let p = a as i32 + b as i32 - c as i32;
                    let pa = (p - a as i32).abs();
                    let pb = (p - b as i32).abs();
                    let pc = (p - c as i32).abs();
                    let pr = if pa <= pb && pa <= pc { a } else if pb <= pc { b } else { c };
                    sortie[x] = sortie[x].wrapping_add(pr);
                }
                _ => {}
            }
        }
        pixels.extend_from_slice(&sortie);
        precedent = sortie;
    }
    Some((largeur, hauteur, pixels))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Le PNG réel embarqué dans l'installeur.
    const PNG_ROSE: &[u8] = include_bytes!("../../installateur/velopack/rose-installeur.png");

    #[test]
    fn inflate_decompresse_le_png() {
        // Diagnostic : le DEFLATE maison est-il en cause ?
        let mut i = 8usize;
        let mut comp: Vec<u8> = Vec::new();
        while i + 8 <= PNG_ROSE.len() {
            let t = u32::from_be_bytes([PNG_ROSE[i], PNG_ROSE[i+1], PNG_ROSE[i+2], PNG_ROSE[i+3]]) as usize;
            let typ = &PNG_ROSE[i+4..i+8];
            if typ == b"IDAT" { comp.extend_from_slice(&PNG_ROSE[i+8..i+8+t]); }
            if typ == b"IEND" { break; }
            i += 12 + t;
        }
        // Les IDAT sont un flux ZLIB (en-tête 2 octets) : c'est
        // `decompresser_zlib` qui les décompresse, pas `decompresser`.
        let brut = crate::inflate::decompresser_zlib(&comp);
        assert!(brut.is_some(), "le DEFLATE maison echoue sur le PNG");
        assert!(brut.unwrap().len() >= 172 * (172 * 4 + 1));
    }

    #[test]
    fn decode_le_png_de_la_rose() {
        let (w, h, px) = decoder_png(PNG_ROSE).expect("le PNG embarqué doit se décoder");
        assert_eq!((w, h), (172, 172));
        assert_eq!(px.len(), w * h * 4);
    }

    #[test]
    fn la_rose_a_des_pixels_opaques_et_colores() {
        let (w, h, px) = decoder_png(PNG_ROSE).unwrap();
        let mut opaques = 0usize;
        let mut rouges = 0usize;
        let mut verts = 0usize;
        let mut transparents = 0usize;
        for k in 0..w * h {
            let r = px[k * 4] as u32;
            let v = px[k * 4 + 1] as u32;
            let b = px[k * 4 + 2] as u32;
            let a = px[k * 4 + 3];
            if a == 0 {
                transparents += 1;
            } else {
                opaques += 1;
                if r > 150 && v < 120 && b < 120 { rouges += 1; }
                if v > 120 && r < 160 && b < 120 { verts += 1; }
            }
        }
        // Une image entièrement transparente ne s'afficherait JAMAIS.
        assert!(opaques > w * h / 4, "trop peu de pixels opaques : {opaques}");
        assert!(rouges > 500, "pixels rouges absents : {rouges}");
        assert!(verts > 200, "pixels verts absents : {verts}");
        assert!(transparents > 100, "fond transparent absent : {transparents}");
    }
}
