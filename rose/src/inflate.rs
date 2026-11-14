//! Décompression DEFLATE (RFC 1951), zlib (RFC 1950) et PNG.
//!
//! Partagé par l'installeur (`setup_gui`, pour décoder la rose embarquée) et
//! par la mise à jour Velopack (`velopack`, pour extraire les `.nupkg`).
//! Aucune dépendance : arbres canoniques + lecture bit à bit, longueurs et
//! distances selon RFC 1951.
//!
//! Licence MIT.

/// Décompresse un flux zlib (en-tête 2 octets + DEFLATE + Adler-32 ignoré).
pub fn decompresser_zlib(donnees: &[u8]) -> Option<Vec<u8>> {
    if donnees.len() < 2 {
        return None;
    }
    let cmf = donnees[0];
    let flg = donnees[1];
    if cmf & 0x0f != 8 || ((cmf as u16) * 256 + flg as u16) % 31 != 0 {
        return None;
    }
    decompresser(&donnees[2..])
}

/// Décompresse un flux DEFLATE brut.
pub fn decompresser(donnees: &[u8]) -> Option<Vec<u8>> {
    let mut bits = Bits { donnees, pos: 0, acc: 0u64, n: 0 };
    let mut sortie = Vec::with_capacity(donnees.len() * 4);
    loop {
        let dernier = bits.lire(1)?;
        let type_bloc = bits.lire(2)?;
        match type_bloc {
            0 => {
                bits.aligner();
                let n = bits.lire(16)? as usize;
                let _ = bits.lire(16)?;
                for _ in 0..n {
                    sortie.push(bits.lire(8)? as u8);
                }
            }
            1 => {
                let (longueurs, distances) = longueurs_fixes();
                bloc(&mut bits, &mut sortie, &longueurs, &distances)?;
            }
            2 => {
                let hlit = bits.lire(5)? as usize + 257;
                let hdist = bits.lire(5)? as usize + 1;
                let hclen = bits.lire(4)? as usize + 4;
                const ORDRE: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
                let mut code_longueurs = [0u8; 19];
                for k in 0..hclen {
                    code_longueurs[ORDRE[k]] = bits.lire(3)? as u8;
                }
                let arbre_code = Huffman::construire(&code_longueurs)?;
                let mut longueurs = vec![0u8; hlit + hdist];
                let mut k = 0;
                while k < longueurs.len() {
                    let symbole = arbre_code.decoder(&mut bits)? as usize;
                    match symbole {
                        0..=15 => {
                            longueurs[k] = symbole as u8;
                            k += 1;
                        }
                        16 => {
                            let rep = bits.lire(2)? as usize + 3;
                            let prec = if k > 0 { longueurs[k - 1] } else { 0 };
                            for _ in 0..rep {
                                if k < longueurs.len() {
                                    longueurs[k] = prec;
                                    k += 1;
                                }
                            }
                        }
                        17 => k += bits.lire(3)? as usize + 3,
                        _ => k += bits.lire(7)? as usize + 11,
                    }
                }
                let arbre_longueurs = Huffman::construire(&longueurs[..hlit])?;
                let arbre_distances = Huffman::construire(&longueurs[hlit..])?;
                bloc(&mut bits, &mut sortie, &arbre_longueurs, &arbre_distances)?;
            }
            _ => return None,
        }
        if dernier == 1 {
            break;
        }
    }
    Some(sortie)
}

struct Bits<'a> {
    donnees: &'a [u8],
    pos: usize,
    acc: u64,
    n: u32,
}

impl<'a> Bits<'a> {
    fn lire(&mut self, combien: u32) -> Option<u32> {
        while self.n < combien {
            let octet = *self.donnees.get(self.pos)? as u64;
            self.pos += 1;
            self.acc |= octet << self.n;
            self.n += 8;
        }
        let v = (self.acc & ((1u64 << combien) - 1)) as u32;
        self.acc >>= combien;
        self.n -= combien;
        Some(v)
    }

    fn aligner(&mut self) {
        let reste = self.n % 8;
        self.acc >>= reste;
        self.n -= reste;
    }
}

struct Huffman {
    /// (code inversé, longueur, symbole)
    table: Vec<(u16, u8, u16)>,
}

/// Inverse les `n` bits de poids faible (DEFLATE lit les codes bit de poids
/// faible d'abord).
fn inverser(code: u32, n: u8) -> u16 {
    let mut sortie = 0u16;
    for i in 0..n {
        sortie |= (((code >> i) & 1) as u16) << (n - 1 - i);
    }
    sortie
}

impl Huffman {
    fn construire(longueurs: &[u8]) -> Option<Huffman> {
        let mut table: Vec<(u16, u8, u16)> = Vec::new();
        for (symbole, &l) in longueurs.iter().enumerate() {
            if l == 0 {
                continue;
            }
            table.push((symbole as u16, l, 0));
        }
        table.sort_by_key(|&(symbole, l, _)| (l, symbole));
        let mut code = 0u32;
        let mut prec = 0u8;
        let mut finale = Vec::new();
        for &(symbole, l, _) in &table {
            code <<= (l - prec) as u32;
            prec = l;
            finale.push((inverser(code, l), l, symbole));
            code += 1;
        }
        Some(Huffman { table: finale })
    }

    fn decoder(&self, bits: &mut Bits) -> Option<u16> {
        let mut code = 0u16;
        for longueur in 1..=15u8 {
            code |= (bits.lire(1)? as u16) << (longueur - 1);
            for &(c, l, symbole) in &self.table {
                if l == longueur && c == code {
                    return Some(symbole);
                }
            }
        }
        None
    }
}

fn longueurs_fixes() -> (Huffman, Huffman) {
    let mut longueurs = [0u8; 288];
    for (i, l) in longueurs.iter_mut().enumerate() {
        *l = if i < 144 { 8 } else if i < 256 { 9 } else if i < 280 { 7 } else { 8 };
    }
    let distances = [5u8; 32];
    (
        Huffman::construire(&longueurs).expect("arbre fixe"),
        Huffman::construire(&distances).expect("arbre fixe"),
    )
}

const BASE_LONGUEURS: [usize; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const EXTRA_LONGUEURS: [u32; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const BASE_DISTANCES: [usize; 30] = [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const EXTRA_DISTANCES: [u32; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

fn bloc(bits: &mut Bits, sortie: &mut Vec<u8>, longueurs: &Huffman, distances: &Huffman) -> Option<()> {
    loop {
        let symbole = longueurs.decoder(bits)?;
        match symbole {
            0..=255 => sortie.push(symbole as u8),
            256 => return Some(()),
            257..=285 => {
                let idx = (symbole - 257) as usize;
                let longueur = BASE_LONGUEURS[idx] + bits.lire(EXTRA_LONGUEURS[idx])? as usize;
                let d = distances.decoder(bits)? as usize;
                if d >= 30 {
                    return None;
                }
                let distance = BASE_DISTANCES[d] + bits.lire(EXTRA_DISTANCES[d])? as usize;
                if distance > sortie.len() {
                    return None;
                }
                let debut = sortie.len() - distance;
                for k in 0..longueur {
                    let octet = sortie[debut + k];
                    sortie.push(octet);
                }
            }
            _ => return None,
        }
    }
}

/// Décode un PNG RGBA 8 bits en (largeur, hauteur, pixels RGBA).
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
                if corps[8] != 8 || corps[9] != 6 {
                    return None;
                }
            }
            b"IDAT" => compression.extend_from_slice(corps),
            b"IEND" => break,
            _ => {}
        }
        i += 12 + taille;
    }
    let brut = decompresser_zlib(&compression)?;
    let pas = largeur * 4;
    if largeur == 0 || hauteur == 0 || brut.len() < hauteur * (pas + 1) {
        return None;
    }
    let mut precedent = vec![0u8; pas];
    let mut pixels = Vec::with_capacity(pas * hauteur);
    for y in 0..hauteur {
        let filtre = brut[y * (pas + 1)];
        let mut ligne = brut[y * (pas + 1) + 1..(y + 1) * (pas + 1)].to_vec();
        for x in 0..pas {
            let a = if x >= 4 { ligne[x - 4] } else { 0 };
            let b = precedent[x];
            let c = if x >= 4 { precedent[x - 4] } else { 0 };
            match filtre {
                1 => ligne[x] = ligne[x].wrapping_add(a),
                2 => ligne[x] = ligne[x].wrapping_add(b),
                3 => ligne[x] = ligne[x].wrapping_add(((a as u16 + b as u16) >> 1) as u8),
                4 => {
                    let p = a as i32 + b as i32 - c as i32;
                    let pa = (p - a as i32).abs();
                    let pb = (p - b as i32).abs();
                    let pc = (p - c as i32).abs();
                    let pr = if pa <= pb && pa <= pc { a } else if pb <= pc { b } else { c };
                    ligne[x] = ligne[x].wrapping_add(pr);
                }
                _ => {}
            }
        }
        pixels.extend_from_slice(&ligne);
        precedent = ligne;
    }
    Some((largeur, hauteur, pixels))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inflate_reproduit_le_texte() {
        // « bonjour bonjour bonjour » en DEFLATE brut (zlib.compressobj(-15))
        let compresse: Vec<u8> = vec![
            0x4b, 0xca, 0xcf, 0xcb, 0xca, 0x2f, 0x2d, 0x52, 0x48, 0x42, 0xa5, 0x01,
        ];
        assert_eq!(decompresser(&compresse).unwrap(), b"bonjour bonjour bonjour");
    }

    #[test]
    fn zlib_reproduit_le_texte() {
        // même texte, encapsulé zlib (zlib.compress)
        let compresse: Vec<u8> = vec![
            0x78, 0xda, 0x4b, 0xca, 0xcf, 0xcb, 0xca, 0x2f, 0x2d, 0x52, 0x48, 0x42, 0xa5,
            0x01, 0x6e, 0x3c, 0x09, 0x3e,
        ];
        assert_eq!(decompresser_zlib(&compresse).unwrap(), b"bonjour bonjour bonjour");
    }
}
