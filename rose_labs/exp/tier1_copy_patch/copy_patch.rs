//! Copieur de patchs pour le copy-and-patch tier 1.
//!
//! Copie des templates de code machine et applique les relocations
//! pour produire du code x86_64 exécutable.

use crate::templates::Template;

/// Un patch compilé : code machine prêt à être exécuté.
#[derive(Debug, Clone)]
pub struct Patch {
    pub code: Vec<u8>,
}

impl Patch {
    /// Crée un nouveau patch à partir d'un template.
    pub fn from_template(template: &Template) -> Self {
        Patch {
            code: template.bytes.to_vec(),
        }
    }

    /// Applique une valeur à une relocation.
    ///
    /// Pour Imm32 : écrit une valeur signée 32 bits à l'offset.
    /// Pour Rel8 : écrit un offset relatif 8 bits (cible - position_actuelle - 1).
    /// Pour Rel32 : écrit un offset relatif 32 bits (cible - position_actuelle - 4).
    pub fn patch(&mut self, reloc_offset: usize, value: i64) {
        // Pour simplifier, on utilise le reloc_offset comme index direct
        // dans le code. En pratique, il faudrait chercher la relocation
        // par son offset dans la liste des relocations du template.
        // Ici, on écrit directement à l'offset donné.
        match self.code.len() > reloc_offset + 8 {
            true => {}
            false => return,
        }

        // Écriture little-endian
        match value {
            v if v >= i32::MIN as i64 && v <= i32::MAX as i64 => {
                let bytes = (v as i32).to_le_bytes();
                self.code[reloc_offset..reloc_offset + 4].copy_from_slice(&bytes);
            }
            v => {
                // Valeur 64 bits
                let bytes = v.to_le_bytes();
                self.code[reloc_offset..reloc_offset + 8].copy_from_slice(&bytes);
            }
        }
    }

    /// Calcule une relocation relative (RIP-relative).
    ///
    /// calcule : target_addr - (current_pos + instruction_size)
    pub fn patch_rel32(&mut self, reloc_offset: usize, target_addr: u64, current_pos: u64) {
        let instruction_size: u64 = 4; // taille de l'immédiat rel32
        let rel = target_addr as i64 - (current_pos as i64 + instruction_size as i64);
        let bytes = (rel as i32).to_le_bytes();
        self.code[reloc_offset..reloc_offset + 4].copy_from_slice(&bytes);
    }

    /// Calcule une relocation relative 8 bits.
    pub fn patch_rel8(&mut self, reloc_offset: usize, target_addr: u64, current_pos: u64) {
        let instruction_size: u64 = 1; // taille de l'immédiat rel8
        let rel = target_addr as i64 - (current_pos as i64 + instruction_size as i64);
        if rel >= i8::MIN as i64 && rel <= i8::MAX as i64 {
            self.code[reloc_offset] = rel as u8;
        }
    }

    /// Retourne le code machine final.
    pub fn finalize(self) -> Vec<u8> {
        self.code
    }
}

/// Copie et assemble plusieurs templates pour construire une fonction complète.
pub struct PatchBuilder {
    code: Vec<u8>,
    relocations: Vec<(usize, usize, i64)>, // (offset, taille, valeur)
    current_pos: u64,
}

impl PatchBuilder {
    pub fn new() -> Self {
        PatchBuilder {
            code: Vec::new(),
            relocations: Vec::new(),
            current_pos: 0,
        }
    }

    /// Ajoute un template au code.
    pub fn add_template(&mut self, template: &crate::templates::Template) -> usize {
        let offset = self.code.len();
        self.code.extend_from_slice(template.bytes);
        self.current_pos += template.bytes.len() as u64;
        offset
    }

    /// Ajoute une relocation à appliquer à la finalisation.
    pub fn add_relocation(&mut self, offset: usize, size: usize, value: i64) {
        self.relocations.push((offset, size, value));
    }

    /// Finalise et retourne le code machine complet.
    pub fn finalize(mut self) -> Vec<u8> {
        // Appliquer les relocations
        for (offset, size, value) in &self.relocations {
            match size {
                1 => self.code[*offset] = *value as u8,
                4 => {
                    let bytes = (*value as i32).to_le_bytes();
                    self.code[*offset..*offset + 4].copy_from_slice(&bytes);
                }
                8 => {
                    let bytes = value.to_le_bytes();
                    self.code[*offset..*offset + 8].copy_from_slice(&bytes);
                }
                _ => {}
            }
        }
        self.code
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::templates::find_template;

    #[test]
    fn test_patch_from_template() {
        let tmpl = find_template("ret").unwrap();
        let patch = Patch::from_template(tmpl);
        assert_eq!(patch.code, vec![0xc3]);
    }

    #[test]
    fn test_patch_builder() {
        let mut builder = PatchBuilder::new();
        let ret_tmpl = find_template("ret").unwrap();
        let offset = builder.add_template(ret_tmpl);
        assert_eq!(offset, 0);
        let code = builder.finalize();
        assert_eq!(code, vec![0xc3]);
    }
}
