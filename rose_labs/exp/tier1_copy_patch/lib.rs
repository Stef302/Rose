//! Tier 1 copy-and-patch — Compilation ultra-rapide par copie de templates.
//!
//! Ce module implémente un compilateur tier 1 basé sur le principe du
//! copy-and-patch : des templates de code machine précompilés sont copiés
//! et assemblés avec des relocations minimales.
//!
//! Objectif : < 1 ms pour 1000 instructions compilées.
//!
//! Usage :
//!   use tier1_copy_patch::{templates, PatchBuilder};
//!   let tmpl = templates::find_template("ret").unwrap();
//!   let mut builder = PatchBuilder::new();
//!   builder.add_template(tmpl);
//!   let code = builder.finalize();

pub mod copy_patch;
pub mod templates;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_loads() {
        let tmpl = templates::find_template("ret").unwrap();
        assert_eq!(tmpl.name, "ret");
        assert_eq!(tmpl.bytes, &[0xc3]);
    }
}
