// ═══════════════════════════════════════════════════════════════════════
// M3.2-bis — ÉCRIVAIN MACH-O 64-BIT (macOS / iOS), SANS OUTIL EXTERNE
// ═══════════════════════════════════════════════════════════════════════
//
// Premier jet : complète `link_natif.rs` (ELF64 + PE64) avec le troisième
// format exécutable natif. Comme ses deux frères, le writer écrit les octets
// LUI-MÊME : aucune dépendance Rust, aucun `ld`, aucun SDK Apple requis.
//
// ⚠ PÉRIMÈTRE — CE QUE CE FICHIER PROUVE, ET CE QU'IL NE PROUVE PAS
//
//   • Il produit une STRUCTURE Mach-O 64-bit valide pour aarch64 (et x86-64),
//     vérifiable sous Linux avec `file` et `llvm-objdump -h/-d`.
//   • Il N'EXÉCUTE RIEN : nous sommes sous Linux, aucun binaire macOS n'a été
//     lancé, et aucun n'est revendiqué comme fonctionnel sur macOS/iOS.
//   • L'entrée est le code d'AMORÇAGE COMPLET (un stub `_start`-like qui pose
//     le code de retour puis termine par le syscall `exit`), pas la fonction
//     `i64 f(base, tramp)` émise par `natif_emetteur::emettre`. Le stub de
//     test ci-dessous utilise exactement les mêmes encodages que
//     `natif_emetteur::mov_imm` et `link_natif::stub_depart` ; câbler la
//     séquence « stub + code émis + trampoline » est la étape suivante.
//
// Structure écrite (little-endian, 220 octets pour un stub de 12 octets) :
//
//   0x000  mach_header_64              32 o   magic 0xfeedfacf, MH_EXECUTE
//   0x020  LC_SEGMENT_64 (__TEXT)       72 o   segment_command_64
//   0x068    section_64 (__text)        80 o   instr. PURE|SOME
//   0x0B8  LC_MAIN                      24 o   entryoff = 0xD0
//   0x0D0  code (stub)                  12 o   mov x0,#49; mov x8,#93; svc #0
//
// Le segment __TEXT couvre tout le fichier (fileoff 0), ce qui est la forme
// canonique d'un exécutable Mach-O : LC_MAIN porte un DÉCALAGE FICHIER vers
// le point d'entrée (et non une adresse virtuelle), donc aucun relocation
// n'est nécessaire.

use std::fmt;
use std::path::Path;

// ── constantes Mach-O / loader.h ────────────────────────────────────────

/// `MH_MAGIC_64` : mach_header_64 little-endian (octets `CF FA ED FE`).
pub const MH_MAGIC_64: u32 = 0xfeed_facf;

/// `CPU_TYPE_ARM64` = CPU_TYPE_ARM (12) | CPU_ARCH_ABI64 (0x01000000).
const CPU_TYPE_ARM64: u32 = 0x0100_000c;
const CPU_SUBTYPE_ARM64_ALL: u32 = 0;
/// `CPU_TYPE_X86_64` = CPU_TYPE_X86 (7) | CPU_ARCH_ABI64.
const CPU_TYPE_X86_64: u32 = 0x0100_0007;
const CPU_SUBTYPE_X86_64_ALL: u32 = 3;

const MH_EXECUTE: u32 = 2;    // fichier exécutable
const MH_NOUNDEFS: u32 = 0x1; // pas de symbols non résolus

/// `LC_SEGMENT_64` chargé avec `segment_command_64`.
const LC_SEGMENT_64: u32 = 0x19;
/// `LC_MAIN` = `LC_REQ_DYLD | 0x28` : point d'entrée (entry_point_command).
const LC_MAIN: u32 = 0x8000_0028;

/// `S_ATTR_PURE_INSTRUCTIONS | S_ATTR_SOME_INSTRUCTIONS`.
const S_ATTR_INSTRUCTIONS: u32 = 0x8000_0000 | 0x0000_0400;

const TAILLE_HEADER: usize = 32;   // mach_header_64
const TAILLE_SEGMENT: usize = 72;  // segment_command_64
const TAILLE_SECTION: usize = 80;  // section_64
const TAILLE_LC_MAIN: usize = 24;  // entry_point_command
const TAILLE_PAGE: u64 = 0x4000;   // granularité vmsize (page arm64 macOS)

/// Adresse virtuelle de chargement de __TEXT (usuelle sur arm64 macOS).
pub const VMADDR_TEXT_DEFAUT: u64 = 0x0000_0001_0000_0000;

// ── API PUBLIQUE ────────────────────────────────────────────────────────

/// Architecture cible du Mach-O (autonome : ce module se compile seul).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Architecture { Aarch64, X86_64 }

impl Architecture {
    pub fn cpu_type(self) -> u32 {
        match self { Architecture::Aarch64 => CPU_TYPE_ARM64,
                     Architecture::X86_64 => CPU_TYPE_X86_64 }
    }
    pub fn cpu_subtype(self) -> u32 {
        match self { Architecture::Aarch64 => CPU_SUBTYPE_ARM64_ALL,
                     Architecture::X86_64 => CPU_SUBTYPE_X86_64_ALL }
    }
    pub fn nom(self) -> &'static str {
        match self { Architecture::Aarch64 => "arm64", Architecture::X86_64 => "x86-64" }
    }
}

/// Réglages de l'édition de liens Mach-O.
#[derive(Clone, Debug)]
pub struct Options {
    pub architecture: Architecture,
    /// `vmaddr` de __TEXT (doit être aligné sur la page).
    pub adresse_base: u64,
    /// Alignement de `__text`, en LOGARITHME BASE 2 (4 = 16 octets).
    pub alignement: u32,
}

impl Options {
    pub fn nouveau(architecture: Architecture) -> Self {
        Self { architecture, adresse_base: VMADDR_TEXT_DEFAUT, alignement: 4 }
    }
    pub fn aarch64() -> Self { Self::nouveau(Architecture::Aarch64) }
}

impl Default for Options {
    fn default() -> Self { Self::aarch64() }
}

/// Erreurs de l'écrivain Mach-O.
#[derive(Debug)]
pub enum Erreur {
    Io(std::io::Error),
    CodeVide,
    Depassement(String),
    AlignementInvalide(u32),
}

impl fmt::Display for Erreur {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Erreur::Io(e) => write!(f, "écriture Mach-O : {}", e),
            Erreur::CodeVide => write!(f, "code machine vide"),
            Erreur::Depassement(s) => write!(f, "dépassement : {}", s),
            Erreur::AlignementInvalide(a) =>
                write!(f, "alignement {} hors plage (0..=30)", a),
        }
    }
}
impl std::error::Error for Erreur {}
impl From<std::io::Error> for Erreur {
    fn from(e: std::io::Error) -> Self { Erreur::Io(e) }
}

/// Construit un exécutable Mach-O 64-bit en mémoire.
pub fn macho_bytes(code: &[u8], options: &Options) -> Result<Vec<u8>, Erreur> {
    if code.is_empty() { return Err(Erreur::CodeVide); }
    if options.alignement > 30 {
        return Err(Erreur::AlignementInvalide(options.alignement));
    }
    if options.adresse_base % TAILLE_PAGE != 0 {
        return Err(Erreur::Depassement(
            "adresse_base doit être alignée sur 0x4000".to_string()));
    }

    let sizeofcmds = TAILLE_SEGMENT + TAILLE_SECTION + TAILLE_LC_MAIN; // 176
    let align_octets = 1usize << options.alignement;
    let code_off = aligner(TAILLE_HEADER + sizeofcmds, align_octets); // 208
    let taille_fichier = code_off.checked_add(code.len())
        .ok_or_else(|| Erreur::Depassement("taille de fichier déborde usize".into()))?;
    let vmsize = aligner64(taille_fichier as u64, TAILLE_PAGE);

    let mut f = vec![0u8; taille_fichier];

    // ── mach_header_64 (32 octets) ──────────────────────────────────────
    ecrire_u32(&mut f, 0, MH_MAGIC_64);
    ecrire_u32(&mut f, 4, options.architecture.cpu_type());
    ecrire_u32(&mut f, 8, options.architecture.cpu_subtype());
    ecrire_u32(&mut f, 12, MH_EXECUTE);
    ecrire_u32(&mut f, 16, 2);              // ncmds : SEGMENT_64 + MAIN
    ecrire_u32(&mut f, 20, sizeofcmds as u32);
    ecrire_u32(&mut f, 24, MH_NOUNDEFS);
    ecrire_u32(&mut f, 28, 0);              // reserved

    // ── LC_SEGMENT_64 __TEXT (72 + 80 octets) ──────────────────────────
    let seg = TAILLE_HEADER;
    ecrire_u32(&mut f, seg, LC_SEGMENT_64);
    ecrire_u32(&mut f, seg + 4, (TAILLE_SEGMENT + TAILLE_SECTION) as u32);
    copier_nom(&mut f, seg + 8, b"__TEXT");
    ecrire_u64(&mut f, seg + 24, options.adresse_base);       // vmaddr
    ecrire_u64(&mut f, seg + 32, vmsize);                     // vmsize
    ecrire_u64(&mut f, seg + 40, 0);                          // fileoff
    ecrire_u64(&mut f, seg + 48, taille_fichier as u64);      // filesize
    ecrire_u32(&mut f, seg + 56, 7);                          // maxprot R|W|X
    ecrire_u32(&mut f, seg + 60, 5);                          // initprot R|X
    ecrire_u32(&mut f, seg + 64, 1);                          // nsects
    ecrire_u32(&mut f, seg + 68, 0);                          // flags

    let sec = seg + TAILLE_SEGMENT;
    copier_nom(&mut f, sec, b"__text");
    copier_nom(&mut f, sec + 16, b"__TEXT");
    ecrire_u64(&mut f, sec + 32, options.adresse_base + code_off as u64); // addr
    ecrire_u64(&mut f, sec + 40, code.len() as u64);                     // size
    ecrire_u32(&mut f, sec + 48, code_off as u32);                       // offset
    ecrire_u32(&mut f, sec + 52, options.alignement);                    // align
    ecrire_u32(&mut f, sec + 56, 0);                                     // reloff
    ecrire_u32(&mut f, sec + 60, 0);                                     // nreloc
    ecrire_u32(&mut f, sec + 64, S_ATTR_INSTRUCTIONS);                   // flags
    ecrire_u32(&mut f, sec + 68, 0);                                     // reserved1
    ecrire_u32(&mut f, sec + 72, 0);                                     // reserved2
    ecrire_u32(&mut f, sec + 76, 0);                                     // reserved3

    // ── LC_MAIN (24 octets) ─────────────────────────────────────────────
    let main = seg + TAILLE_SEGMENT + TAILLE_SECTION; // 184
    ecrire_u32(&mut f, main, LC_MAIN);
    ecrire_u32(&mut f, main + 4, TAILLE_LC_MAIN as u32);
    ecrire_u64(&mut f, main + 8, code_off as u64); // entryoff = décalage fichier
    ecrire_u64(&mut f, main + 16, 0);             // stacksize par défaut

    // ── code ────────────────────────────────────────────────────────────
    f[code_off..].copy_from_slice(code);
    Ok(f)
}

/// Écrit l'exécutable Mach-O sur disque et le rend exécutable (Unix).
pub fn ecrire_macho(code: &[u8], options: &Options, chemin: &Path) -> Result<(), Erreur> {
    std::fs::write(chemin, macho_bytes(code, options)?)?;
    rendre_executable(chemin)
}

#[cfg(unix)]
fn rendre_executable(chemin: &Path) -> Result<(), Erreur> {
    use std::os::unix::fs::PermissionsExt;
    let mut p = std::fs::metadata(chemin)?.permissions();
    p.set_mode(0o755);
    std::fs::set_permissions(chemin, p)?;
    Ok(())
}
#[cfg(not(unix))]
fn rendre_executable(_chemin: &Path) -> Result<(), Erreur> { Ok(()) }

// ── petits outils d'écriture ────────────────────────────────────────────

fn aligner(v: usize, a: usize) -> usize { (v + a - 1) & !(a - 1) }
fn aligner64(v: u64, a: u64) -> u64 { (v + a - 1) & !(a - 1) }
fn ecrire_u32(v: &mut [u8], o: usize, x: u32) {
    v[o..o + 4].copy_from_slice(&x.to_le_bytes());
}
fn ecrire_u64(v: &mut [u8], o: usize, x: u64) {
    v[o..o + 8].copy_from_slice(&x.to_le_bytes());
}
fn copier_nom(v: &mut [u8], o: usize, nom: &[u8]) {
    v[o..o + nom.len()].copy_from_slice(nom); // le reste est déjà à zéro
}

// ═══════════════════════════════════════════════════════════════════════
// TESTS — preuve de structure (octet par octet + outils externes)
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    /// `movz rd, #imm` : formule EXACTE de `natif_emetteur::mov_imm`
    /// (premier mot de 16 bits, hw = 0, registre `rd`).
    fn movz(rd: u32, imm: u32) -> u32 { 0xD280_0000 | ((imm & 0xFFFF) << 5) | rd }

    /// Stub d'entrée autonome : `mov x0,#49 ; mov x8,#93 ; svc #0`.
    /// Les deux `movz` utilisent la formule de `natif_emetteur::mov_imm` ;
    /// `mov x8,#93` est l'octet `0xD280_0BA8` de `link_natif::stub_depart`.
    fn stub_49() -> Vec<u8> {
        let mut s = Vec::new();
        for insn in [movz(0, 49), movz(8, 93), 0xD400_0001] {
            s.extend_from_slice(&insn.to_le_bytes());
        }
        s
    }

    fn u32s(o: &[u8], p: usize) -> u32 {
        u32::from_le_bytes(o[p..p + 4].try_into().unwrap())
    }
    fn u64s(o: &[u8], p: usize) -> u64 {
        u64::from_le_bytes(o[p..p + 8].try_into().unwrap())
    }

    #[test]
    fn structure_macho_aarch64_octet_par_octet() {
        let stub = stub_49();
        assert_eq!(stub, vec![0x20, 0x06, 0x80, 0xd2, 0xa8, 0x0b, 0x80, 0xd2,
                              0x01, 0x00, 0x00, 0xd4]);
        let o = macho_bytes(&stub, &Options::aarch64()).unwrap();

        // `file` : magic little-endian de MH_MAGIC_64.
        assert_eq!(&o[0..4], &[0xCF, 0xFA, 0xED, 0xFE]);
        assert_eq!(u32s(&o, 0), MH_MAGIC_64);
        assert_eq!(u32s(&o, 4), 0x0100_000c); // CPU_TYPE_ARM64
        assert_eq!(u32s(&o, 8), 0);           // CPU_SUBTYPE_ARM64_ALL
        assert_eq!(u32s(&o, 12), 2);          // MH_EXECUTE
        assert_eq!(u32s(&o, 16), 2);          // ncmds
        assert_eq!(u32s(&o, 20), 176);        // sizeofcmds
        assert_eq!(u32s(&o, 24), 1);          // MH_NOUNDEFS
        assert_eq!(u32s(&o, 28), 0);          // reserved

        // LC_SEGMENT_64 __TEXT
        assert_eq!(u32s(&o, 32), 0x19);
        assert_eq!(u32s(&o, 36), 152); // 72 + 80
        assert_eq!(&o[40..46], b"__TEXT");
        assert_eq!(u64s(&o, 56), VMADDR_TEXT_DEFAUT); // vmaddr
        assert_eq!(u64s(&o, 72), 0);                 // fileoff
        assert_eq!(u64s(&o, 80), 220);               // filesize
        assert_eq!(u32s(&o, 88), 7);                 // maxprot
        assert_eq!(u32s(&o, 92), 5);                 // initprot
        assert_eq!(u32s(&o, 96), 1);                 // nsects

        // section_64 __text
        let sec = 104usize;
        assert_eq!(&o[sec..sec + 6], b"__text");
        assert_eq!(&o[sec + 16..sec + 22], b"__TEXT");
        assert_eq!(u64s(&o, sec + 32), VMADDR_TEXT_DEFAUT + 208); // addr
        assert_eq!(u64s(&o, sec + 40), stub.len() as u64);        // size = 12
        assert_eq!(u32s(&o, sec + 48), 208);   // offset (décalage fichier)
        assert_eq!(u32s(&o, sec + 52), 4);     // align = 16 octets
        assert_eq!(u32s(&o, sec + 64), S_ATTR_INSTRUCTIONS);

        // LC_MAIN
        let main = 184usize;
        assert_eq!(u32s(&o, main), LC_MAIN);
        assert_eq!(u32s(&o, main + 4), 24);
        assert_eq!(u64s(&o, main + 8), 208);  // entryoff
        assert_eq!(u64s(&o, main + 16), 0);   // stacksize

        // code + taille exacte (aucun octet parasite)
        assert_eq!(&o[208..], &stub[..]);
        assert_eq!(o.len(), 220);
    }

    #[test]
    fn cpu_x86_64_est_reconnu() {
        let o = macho_bytes(&stub_49(), &Options::nouveau(Architecture::X86_64)).unwrap();
        assert_eq!(u32s(&o, 4), 0x0100_0007); // CPU_TYPE_X86_64
        assert_eq!(u32s(&o, 8), 3);           // CPU_SUBTYPE_X86_64_ALL
    }

    #[test]
    fn erreurs_sont_refusees() {
        assert!(matches!(macho_bytes(&[], &Options::aarch64()), Err(Erreur::CodeVide)));
        let mut opt = Options::aarch64();
        opt.alignement = 31;
        assert!(matches!(macho_bytes(&stub_49(), &opt), Err(Erreur::AlignementInvalide(31))));
        opt = Options::aarch64();
        opt.adresse_base = 0x1000; // non alignée sur 0x4000
        assert!(matches!(macho_bytes(&stub_49(), &opt), Err(Erreur::Depassement(_))));
    }

    #[test]
    fn ecrit_fichier_pour_file_et_llvm_objdump() {
        let dir = std::env::temp_dir().join("opencode");
        std::fs::create_dir_all(&dir).unwrap();
        let chemin = dir.join("rose_m3_2_arm64.macho");
        ecrire_macho(&stub_49(), &Options::aarch64(), &chemin).unwrap();
        let octets = std::fs::read(&chemin).unwrap();
        assert_eq!(octets.len(), 220);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&chemin).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o755);
        }
        eprintln!("Mach-O écrit : {} ({} octets)", chemin.display(), octets.len());
    }
}
