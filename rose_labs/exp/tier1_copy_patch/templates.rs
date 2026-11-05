//! Templates de code machine x86_64 pour le copy-and-patch tier 1.
//!
//! Chaque template est une séquence d'octets avec des trous (relocations).
//! Les offsets sont mesurés depuis le début du template.



/// Un template de code machine x86_64.
#[derive(Debug, Clone)]
pub struct Template {
    pub name: &'static str,
    pub bytes: &'static [u8],
    pub relocations: &'static [Reloc],
}

/// Type de relocation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RelocType {
    Imm32,    // entier signé 32 bits
    Rel8,     // saut relatif 8 bits
    Rel32,    // saut relatif 32 bits
}

/// Une relocation dans un template.
#[derive(Debug, Clone)]
pub struct Reloc {
    pub offset: usize,      // position dans le template (octets)
    pub size: usize,        // taille en octets (1, 4, 8)
    pub reloc_type: RelocType,
    pub target: &'static str, // nom du symbole cible (ou "" pour placeholder)
}

/// Templates x86_64 disponibles.
pub fn templates() -> &'static [Template] {
    &[
        // mov rdi, imm64  (charge une valeur immédiate 64 bits)
        Template {
            name: "mov_rdi_imm64",
            bytes: &[
                0x48, 0xbf, // mov rdi, imm64
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // imm64 (trou)
            ],
            relocations: &[
                Reloc { offset: 2, size: 8, reloc_type: RelocType::Imm32, target: "" },
            ],
        },
        // mov rax, rdi
        Template {
            name: "mov_rax_rdi",
            bytes: &[0x48, 0x89, 0xf8],
            relocations: &[],
        },
        // add rax, rdi
        Template {
            name: "add_rax_rdi",
            bytes: &[0x48, 0x01, 0xf8],
            relocations: &[],
        },
        // sub rax, rdi
        Template {
            name: "sub_rax_rdi",
            bytes: &[0x48, 0x29, 0xf8],
            relocations: &[],
        },
        // imul rax, rdi
        Template {
            name: "imul_rax_rdi",
            bytes: &[0x48, 0x0f, 0xaf, 0xf8],
            relocations: &[],
        },
        // cmp rax, rdi
        Template {
            name: "cmp_rax_rdi",
            bytes: &[0x48, 0x39, 0xf8],
            relocations: &[],
        },
        // je rel8 (saut conditionnel court)
        Template {
            name: "je_rel8",
            bytes: &[0x74, 0x00], // je rel8 (trou)
            relocations: &[
                Reloc { offset: 1, size: 1, reloc_type: RelocType::Rel8, target: "" },
            ],
        },
        // jmp rel32 (saut inconditionnel)
        Template {
            name: "jmp_rel32",
            bytes: &[0xe9, 0x00, 0x00, 0x00, 0x00], // jmp rel32 (trou)
            relocations: &[
                Reloc { offset: 1, size: 4, reloc_type: RelocType::Rel32, target: "" },
            ],
        },
        // ret
        Template {
            name: "ret",
            bytes: &[0xc3],
            relocations: &[],
        },
        // push rbp
        Template {
            name: "push_rbp",
            bytes: &[0x55],
            relocations: &[],
        },
        // mov rbp, rsp
        Template {
            name: "mov_rbp_rsp",
            bytes: &[0x48, 0x89, 0xe5],
            relocations: &[],
        },
        // pop rbp
        Template {
            name: "pop_rbp",
            bytes: &[0x5d],
            relocations: &[],
        },
    ]
}

/// Trouver un template par nom.
pub fn find_template(name: &str) -> Option<&'static Template> {
    templates().iter().find(|t| t.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_template() {
        assert!(find_template("mov_rdi_imm64").is_some());
        assert!(find_template("add_rax_rdi").is_some());
        assert!(find_template("ret").is_some());
        assert!(find_template("inexistant").is_none());
    }

    #[test]
    fn test_template_sizes() {
        assert_eq!(find_template("mov_rdi_imm64").unwrap().bytes.len(), 10);
        assert_eq!(find_template("ret").unwrap().bytes.len(), 1);
        assert_eq!(find_template("jmp_rel32").unwrap().bytes.len(), 5);
    }
}
