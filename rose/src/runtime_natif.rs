// ═══════════════════════════════════════════════════════════════════════
// RUNTIME NATIF AOT6 — opérations complexes exécutées EN CODE MACHINE
// ═══════════════════════════════════════════════════════════════════════
//
// `rose/runtime/{aarch64,x86_64}.S` est le « compiler-rt » de Rose : ces
// routines sont assemblées UNE FOIS (voir `rose/runtime/generer.sh`) et
// leurs octets sont embarqués ici avec `include_bytes!`. `rose aot` copie
// ces octets dans le binaire émis : à l'exécution, ce sont de vraies
// instructions machine qui tournent, pas un interpréteur Rust externe et
// pas le moteur de 5,19 Mo recopié.
//
// Les routines suivent la signature machine des fonctions Rose émises :
//
//       i64 routine(base: *mut i64, tramp: *const Trampoline)
//
// Elles lisent leurs arguments dans `base[0..]` et rendent le résultat
// dans x0 / rax. Elles sont donc appelables par un `bl`/`call` relatif
// exactement comme une fonction Rose du même programme.
//
// Zones de données absolues réservées par le linker (ET_EXEC non-PIE,
// base 0x40_0000) :
//   SORTIE_ADDR 0x40_9000 : [u64 len][texte...]
//   TMP_ADDR    0x40_B000 : workspace bigint ; 0x40_D000 : digits
//
// Licence MIT.

use crate::natif_emetteur::Architecture;

pub const SORTIE_ADDR: u64 = 0x0040_2000;
pub const SORTIE_TAILLE: usize = 4096;
pub const TMP_ADDR: u64 = 0x0040_3000;
pub const TMP_TAILLE: usize = 4096;
/// Le workspace bigint utilise aussi 0x40_4000 pour les digits inversés.
pub const ZONE_FIN: u64 = 0x0040_5000;

/// Octets machine bruts, assemblés une fois pour toutes.
static A64_BIN: &[u8] = include_bytes!("../runtime/aarch64.bin");
static X64_BIN: &[u8] = include_bytes!("../runtime/x86_64.bin");

/// Décalage de chaque routine dans le blob (valeurs générées par
/// `generer.sh`, vérifiées par le test `symboles_runtime_sont_corrects`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Primitif {
    Hasard,
    FormaterFlottant,
    BigintPuissance,
    LireTexte,
    Sortir,
}

pub fn nom_primitif(p: Primitif) -> &'static str {
    match p {
        Primitif::Hasard => "$rt_hasard",
        Primitif::FormaterFlottant => "$rt_formater_flottant",
        Primitif::BigintPuissance => "$rt_bigint_puissance",
        Primitif::LireTexte => "$rt_lire_texte",
        Primitif::Sortir => "$rt_sortir",
    }
}

/// Offset d'une routine dans le blob machine de son architecture.
pub fn offset(arch: Architecture, p: Primitif) -> Option<usize> {
    let (bin, table): (&[u8], &[(Primitif, usize)]) = match arch {
        Architecture::Aarch64 => (A64_BIN, &[
            (Primitif::Hasard, 0x0000),
            (Primitif::Sortir, 0x00a0),
            (Primitif::FormaterFlottant, 0x00e0),
            (Primitif::LireTexte, 0x0770),
            (Primitif::BigintPuissance, 0x07c0),
        ]),
        Architecture::X86_64 => (X64_BIN, &[
            (Primitif::Hasard, 0x0000),
            (Primitif::Sortir, 0x00c0),
            (Primitif::LireTexte, 0x00f0),
            (Primitif::BigintPuissance, 0x0120),
            (Primitif::FormaterFlottant, 0x0240),
        ]),
    };
    let o = table.iter().find(|(q, _)| *q == p).map(|(_, o)| *o)?;
    if o < bin.len() { Some(o) } else { None }
}

/// Blob complet (toutes les routines). Les programmes n'en paient que les
/// routines réellement référencées : `emettre_runtime` extrait les tranches.
#[derive(Clone, Debug)]
pub struct RuntimeEmis {
    pub code: Vec<u8>,
    /// `(nom machine, décalage dans code)`.
    pub routines: Vec<(String, usize)>,
}

/// Émet les routines demandées. L'ordre est fixe et reproductible.
/// `$rt_sortir` est ajouté automatiquement dès qu'une routine produit du
/// texte dans SORTIE.
pub fn emettre_runtime(arch: Architecture, _p: &[Primitif]) -> RuntimeEmis {
    // ⚠ Les pools de littéraux de l'assembleur sont placés en fin de
    // section : extraire une seule routine casserait les `ldr =imm` /
    // `lea rip`. On embarque donc TOUT le runtime (≈ 2,4 Ko) dès qu'une
    // routine est nécessaire. Largement sous la limite de 30 Ko.
    let bin = match arch {
        Architecture::Aarch64 => A64_BIN,
        Architecture::X86_64 => X64_BIN,
    };
    let routines = [
        Primitif::Hasard, Primitif::Sortir, Primitif::FormaterFlottant,
        Primitif::LireTexte, Primitif::BigintPuissance,
    ];
    RuntimeEmis {
        code: bin.to_vec(),
        routines: routines.iter()
            .filter_map(|p| offset(arch, *p).map(|o| (nom_primitif(*p).to_string(), o)))
            .collect(),
    }
}

/// Tag de formatage du résultat final.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TagSortie {
    Entier = 0,
    Flottant = 1,
    Bigint = 2,
    Chaine = 3,
    Booleen = 4,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_embarque_est_du_code_machine() {
        for arch in [Architecture::Aarch64, Architecture::X86_64] {
            let r = emettre_runtime(arch, &[Primitif::Hasard, Primitif::FormaterFlottant]);
            assert!(r.code.len() > 500, "runtime trop court pour {:?}", arch);
            assert!(r.routines.iter().any(|(n, _)| n == "$rt_sortir"));
            assert!(r.routines.iter().any(|(n, _)| n == "$rt_hasard"));
        }
    }

    #[test]
    fn offset_est_dans_le_blob() {
        for arch in [Architecture::Aarch64, Architecture::X86_64] {
            for p in [Primitif::Hasard, Primitif::FormaterFlottant,
                      Primitif::BigintPuissance, Primitif::LireTexte, Primitif::Sortir] {
                let o = offset(arch, p).expect("routine absente");
                let n = match arch {
                    Architecture::Aarch64 => A64_BIN.len(),
                    Architecture::X86_64 => X64_BIN.len(),
                };
                assert!(o < n);
            }
        }
    }
}
