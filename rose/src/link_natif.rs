// ═══════════════════════════════════════════════════════════════════════
// M3.2 — LINK NATIF : ELF64 / PE64 ÉCRITS À LA MAIN
// ═══════════════════════════════════════════════════════════════════════
//
// but : `./prog` produit par `rose build` tourne sur une machine SANS gcc,
// clang, ld, ni libc de développement. ZÉRO outil externe, ZÉRO dépendance
// Rust supplémentaire : on écrit les octets du format directement.
//
// entrée : les octets machine de `natif_emetteur::emettre` (M3.1), c.-à-d.
//          une fonction `i64 f(base: *mut i64, tramp: *const Trampoline)`.
//
// Le writer ajoute un stub `_start` qui :
//   1. réserve `n_slots_total * 8` octets sur la pile (table des slots),
//   2. pose `base = sp` (aarch64 : x0 ; x86-64 : rdi),
//   3. pose `tramp = adresse du stub de trampoline` (aarch64 : x1 ; x86 : rsi),
//   4. appelle le code machine émis,
//   5. termine par le syscall `exit(valeur_de_retour)` (93 sur aarch64,
//      60 sur x86-64) : la valeur de retour de la fonction Rose devient le
//      code de retour du processus.
//
// ⚠ Le trampoline embarqué est un STUB qui rend 0. Les programmes qui
// utilisent `AppelRir` / `eval_fallback` ont besoin du runtime Rose embarqué
// (étape suivante) : `ecrire_*_depuis_emitted` les refuse explicitement
// plutôt que de produire un binaire silencieusement faux. Voir
// `docs/LINK_NATIF.md`.

pub use crate::natif_emetteur::Architecture;
use crate::natif_emetteur::{Emitted, SCRATCH_SLOTS};
use std::fmt;
use std::path::Path;

/// Adresse virtuelle de chargement par défaut (non-PIE, pas de relocations).
pub const ADRESSE_BASE: u64 = 0x0040_0000;

/// Convention d'appel du **stub d'entrée** inséré devant le code émis.
///
/// Ce choix ne concerne que l'interface stub → code émis et stub → API Windows.
/// Le corps émis par `natif_emetteur` (M3.1) garde sa convention SysV/AAPCS64
/// interne : voir `docs/LINK_NATIF.md` §7 pour ce qui doit encore y changer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Abi {
    /// SysV AMD64 (`rdi`, `rsi`, `rdx`, `rcx`) / AAPCS64 (`x0`, `x1`) — ELF.
    SysV,
    /// Microsoft x64 / Windows ARM64 : 4 registres d'arguments, **32 octets
    /// de shadow space** réservés par l'appelant, pile alignée sur 16, et
    /// sortie du processus par `kernel32!ExitProcess` appelé via l'IAT.
    Win64,
}

// ── constantes PE ───────────────────────────────────────────────────────
/// RVA de `.text` (début du stub d'entrée).
const PE_RVA_TEXT: u32 = 0x1000;
/// RVA de `.data` (table de trampoline + structures d'import).
const PE_RVA_DATA: u32 = 0x2000;
/// ImageBase typique d'un PE64 non-PIE.
const PE_IMAGE_BASE: u64 = 0x0000_0001_4000_0000;
/// Décalages dans `.data`, relatifs à `PE_RVA_DATA`.
const PE_TRAMP_OFF: u32 = 0x000;  // table de trampoline : 3 × u64 = 24 o
const PE_IMPORT_DESC_OFF: u32 = 0x020; // 2 descripteurs de 20 o (dont le nul)
const PE_HINT_NAME_OFF: u32 = 0x050;   // IMAGE_IMPORT_BY_NAME(ExitProcess)
const PE_ILT_OFF: u32 = 0x060;         // Import Lookup Table (2 × u64)
const PE_IAT_OFF: u32 = 0x070;         // Import Address Table (2 × u64)
const PE_DLL_NAME_OFF: u32 = 0x080;    // "KERNEL32.DLL\0"

// ── API PUBLIQUE ───────────────────────────────────────────────────────

/// Réglages de l'édition de liens.
#[derive(Clone, Debug)]
pub struct Options {
    /// Architecture du code machine fourni.
    pub architecture: Architecture,
    /// Base de mapping (doit être alignée sur 0x1000).
    pub adresse_base: u64,
    /// Nombre de slots i64 à réserver pour la table de slots.
    pub n_slots_total: usize,
    /// Nom du symbole machine dans la table des symboles.
    pub nom_code: String,
    /// ⚠ AOT uniquement : écrire la valeur entière en décimal sur stdout
    /// avant de sortir. Le code de retour POSIX ne porte que 8 bits, donc
    /// sans cela une valeur comme 317811 serait tronquée à 115.
    /// `false` par défaut : tous les appels existants gardent leur stub.
    pub afficher_valeur: bool,
    /// ⚠ AOT « valeurs exactes » (vague 78) : texte décimal complet (avec
    /// `'\n'`) calculé par l'interpréteur Rose à la compilation. Le stub ELF
    /// l'écrit sur stdout au lieu de convertir la valeur i64 rendue par le
    /// code machine. `None` par défaut : tous les appels existants gardent
    /// leur stub (et leurs octets) exactement comme avant.
    pub sortie_exacte: Option<Vec<u8>>,
    /// AOT6 : le runtime natif est présent dans `code`. Le stub appelle
    /// ces routines avant de sortir pour formater/écrire le résultat.
    pub runtime: Option<crate::natif_emetteur::RuntimeLiens>,
}

impl Options {
    /// Réglages par défaut pour une architecture donnée.
    pub fn nouveau(architecture: Architecture) -> Self {
        Self {
            architecture,
            adresse_base: ADRESSE_BASE,
            n_slots_total: SCRATCH_SLOTS as usize,
            nom_code: "rose_code".to_string(),
            afficher_valeur: false,
            sortie_exacte: None,
            runtime: None,
        }
    }

    /// Reprend l'architecture et le nombre de slots d'un résultat d'émission.
    pub fn depuis_emitted(emitted: &Emitted) -> Self {
        let mut o = Self::nouveau(emitted.architecture);
        o.n_slots_total = emitted.n_slots_total;
        o
    }
}

impl Default for Options {
    fn default() -> Self {
        Self::nouveau(Architecture::native())
    }
}

/// Erreurs de l'éditeur de liens natif.
#[derive(Debug)]
pub enum ErreurLien {
    /// Écriture disque.
    Io(std::io::Error),
    /// Aucun octet machine fourni.
    CodeVide,
    /// Le programme appelle la trampoline : le runtime Rose n'est pas encore
    /// embarqué dans l'exécutable autonome.
    RuntimeRequis,
    ///taille hors de l'immédiat encodable (trop de slots).
    Depassement(String),
}

impl fmt::Display for ErreurLien {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErreurLien::Io(e) => write!(f, "écriture exécutable : {}", e),
            ErreurLien::CodeVide => write!(f, "code machine vide"),
            ErreurLien::RuntimeRequis => write!(
                f,
                "ce programme utilise la trampoline (appels/fallbacks) : \
                 le runtime Rose embarqué n'est pas encore disponible"
            ),
            ErreurLien::Depassement(s) => write!(f, "dépassement : {}", s),
        }
    }
}
impl std::error::Error for ErreurLien {}
impl From<std::io::Error> for ErreurLien {
    fn from(e: std::io::Error) -> Self { ErreurLien::Io(e) }
}

/// Construit un exécutable ELF64 en mémoire (ET_EXEC, non-PIE, statique).
pub fn elf_bytes(code: &[u8], options: &Options) -> Result<Vec<u8>, ErreurLien> {
    if code.is_empty() { return Err(ErreurLien::CodeVide); }
    if options.adresse_base % 0x1000 != 0 {
        return Err(ErreurLien::Depassement(
            "adresse_base doit être alignée sur 0x1000".to_string()));
    }

    // Table des slots sur la pile, arrondie à 16 octets (alignement AAPCS64).
    let octets_slots = options.n_slots_total.checked_mul(8).ok_or_else(|| {
        ErreurLien::Depassement("nombre de slots déborde usize".to_string())
    })?;
    let frame = aligner(octets_slots, 16);
    if options.architecture == Architecture::Aarch64 && frame > 0xFFF {
        return Err(ErreurLien::Depassement(format!(
            "table de slots de {} octets > immédiat 12 bits (max 4095)", frame)));
    }
    if options.architecture == Architecture::X86_64 && frame > i32::MAX as usize {
        return Err(ErreurLien::Depassement("table de slots trop grande".to_string()));
    }

    let base = options.adresse_base;
    let off_text: usize = 0x100;
    let vaddr_text = base + off_text as u64;

    // Un ELF est un format Unix : le stub utilise toujours SysV/AAPCS64 et
    // termine par le syscall exit (aucune table d'imports n'existe en ELF).
    let abi = Abi::SysV;
    // ⚠ AOT valeurs exactes : le texte décimal (calculé par l'interpréteur à
    // la compilation) est stocké dans .rodata et écrit par le stub. L'adresse
    // n'est connue qu'après le placement des sections : on génère d'abord le
    // stub avec une adresse factice (la suite d'instructions a une LONGUEUR
    // FIXE), puis on le régénère avec la vraie adresse plus bas.
    let donnees = options.sortie_exacte.clone();
    let stub_len = taille_stub(options.architecture, abi, frame,
                               options.afficher_valeur,
                               donnees.as_ref().map(|d| d.len()),
                               options.runtime.as_ref());
    let vaddr_code = vaddr_text + stub_len as u64;
    let off_tramp_stub = stub_len + code.len();
    let vaddr_tramp_stub = vaddr_text + off_tramp_stub as u64;

    let stub = stub_depart(options.architecture, abi, vaddr_text, vaddr_code,
                           vaddr_tramp_stub, 0, frame, options.afficher_valeur,
                           donnees.as_ref().map(|d| (0u64, d.len())),
                           options.runtime.as_ref());
    debug_assert_eq!(stub.len(), stub_len);
    let tramp_stub = stub_trampoline(options.architecture);

    let mut text = Vec::with_capacity(stub_len + code.len() + tramp_stub.len());
    text.extend_from_slice(&stub);
    text.extend_from_slice(code);
    text.extend_from_slice(&tramp_stub);

    // .rodata : le texte exact quand il y en a ; sinon la chaîne innocente
    // historique, suffisante à matérialiser la section.
    let rodata: Vec<u8> = donnees.clone()
        .unwrap_or_else(|| b"ROSE-LINK-NATIF\0".to_vec());
    // .data : la table de trampoline (3 pointeurs) lue par le code émis.
    let mut data = Vec::with_capacity(24);
    p_u64(&mut data, vaddr_tramp_stub); // appel_rir
    p_u64(&mut data, vaddr_tramp_stub); // eval_fallback
    p_u64(&mut data, 0);                // ctx

    // ── tables de chaînes ───────────────────────────────────────────────
    let mut strtab = StrTab::neuve();
    let nom_start = strtab.ajouter("_start");
    let nom_code = strtab.ajouter(&options.nom_code);
    let nom_tramp = strtab.ajouter("rose_tramp");

    let mut shstrtab = StrTab::neuve();
    let sn_text = shstrtab.ajouter(".text");
    let sn_rodata = shstrtab.ajouter(".rodata");
    let sn_data = shstrtab.ajouter(".data");
    let sn_symtab = shstrtab.ajouter(".symtab");
    let sn_strtab = shstrtab.ajouter(".strtab");
    let sn_shstrtab = shstrtab.ajouter(".shstrtab");

    // ── table des symboles (4 entrées de 24 octets) ────────────────────
    let mut symtab = Vec::with_capacity(4 * 24);
    sym(&mut symtab, 0, 0, 0, 0, 0); // entrée nulle
    sym(&mut symtab, nom_start, 0x12, 1, vaddr_text, stub_len as u64); // _start (FUNC global)
    sym(&mut symtab, nom_code, 0x12, 1, vaddr_code, code.len() as u64); // rose_code
    // adresse/taille corrigées après le placement de .data (plus bas).
    sym(&mut symtab, nom_tramp, 0x11, 3, 0, 0);

    // ── placement des sections dans le fichier ─────────────────────────
    let off_rodata = aligner(off_text + text.len(), 16);
    if let Some(d) = &donnees {
        // On connaît maintenant l'adresse réelle du texte : on réécrit le
        // stub (même longueur, seuls les immédiats changent).
        let vaddr_donnees = base + off_rodata as u64;
        let stub = stub_depart(options.architecture, abi, vaddr_text, vaddr_code,
                               vaddr_tramp_stub, 0, frame, options.afficher_valeur,
                               Some((vaddr_donnees, d.len())),
                               options.runtime.as_ref());
        debug_assert_eq!(stub.len(), stub_len);
        text[0..stub_len].copy_from_slice(&stub);
    }
    let off_data = aligner(off_rodata + rodata.len(), 8);
    let off_symtab = aligner(off_data + data.len(), 8);
    let off_strtab = off_symtab + symtab.len();
    let off_shstrtab = off_strtab + strtab.octets.len();
    let off_shdr = aligner(off_shstrtab + shstrtab.octets.len(), 8);
    // AOT6 : le runtime utilise des zones absolues (SORTIE/TMP/digits).
    // Le PT_LOAD couvre tout le fichier : on le remplit de zéros jusqu'à
    // ZONE_FIN pour que ces adresses soient mappées et écrivables.
    let zone_fin = (crate::runtime_natif::ZONE_FIN - base) as usize;
    if options.runtime.is_some() {
        if off_text + text.len() > 0x2000 {
            return Err(ErreurLien::Depassement(
                "code machine trop long : chevauche la zone de données du runtime".into()));
        }
    }
    let taille_fichier = if options.runtime.is_some() {
        (off_shdr + 7 * 64).max(zone_fin)
    } else {
        off_shdr + 7 * 64
    };

    // rose_tramp est dans .data : on connaît maintenant son adresse.
    let vaddr_data = base + off_data as u64;
    ecrire_u64(&mut symtab, 3 * 24 + 8, vaddr_data);
    ecrire_u64(&mut symtab, 3 * 24 + 16, 24);

    let mut f = vec![0u8; taille_fichier];
    f[0..16].copy_from_slice(&[0x7F, b'E', b'L', b'F', 2, 1, 1, 0,
                               0, 0, 0, 0, 0, 0, 0, 0]);
    ecrire_u16(&mut f, 16, 2);  // ET_EXEC
    ecrire_u16(&mut f, 18, machine_elf(options.architecture));
    ecrire_u32(&mut f, 20, 1);  // EV_CURRENT
    ecrire_u64(&mut f, 24, vaddr_text); // e_entry = _start
    ecrire_u64(&mut f, 32, 64); // e_phoff
    ecrire_u64(&mut f, 40, off_shdr as u64);
    ecrire_u32(&mut f, 48, 0);  // e_flags
    ecrire_u16(&mut f, 52, 64); // e_ehsize
    ecrire_u16(&mut f, 54, 56); // e_phentsize
    ecrire_u16(&mut f, 56, 1);  // e_phnum
    ecrire_u16(&mut f, 58, 64); // e_shentsize
    ecrire_u16(&mut f, 60, 7);  // e_shnum
    ecrire_u16(&mut f, 62, 6);  // e_shstrndx = .shstrtab

    // Un seul PT_LOAD couvre tout le fichier (RWX) : aucune relocation,
    // aucun interpréteur dynamique, aucun lien avec ld.
    ecrire_u32(&mut f, 64, 1);       // PT_LOAD
    ecrire_u32(&mut f, 68, 7);       // PF_R | PF_W | PF_X
    ecrire_u64(&mut f, 72, 0);       // p_offset
    ecrire_u64(&mut f, 80, base);    // p_vaddr
    ecrire_u64(&mut f, 88, base);    // p_paddr
    ecrire_u64(&mut f, 96, taille_fichier as u64);
    ecrire_u64(&mut f, 104, taille_fichier as u64);
    ecrire_u64(&mut f, 112, 0x1000); // p_align

    f[off_text..off_text + text.len()].copy_from_slice(&text);
    f[off_rodata..off_rodata + rodata.len()].copy_from_slice(&rodata);
    f[off_data..off_data + data.len()].copy_from_slice(&data);
    f[off_symtab..off_symtab + symtab.len()].copy_from_slice(&symtab);
    f[off_strtab..off_strtab + strtab.octets.len()].copy_from_slice(&strtab.octets);
    f[off_shstrtab..off_shstrtab + shstrtab.octets.len()].copy_from_slice(&shstrtab.octets);

    // Section headers : 0 = nul, 1 .text, 2 .rodata, 3 .data,
    //                  4 .symtab, 5 .strtab, 6 .shstrtab.
    let sh = [
        shdr(0, 0, 0, 0, 0, 0, 0, 0, 0, 0),
        shdr(sn_text, 1, 0x6, vaddr_text, off_text as u64, text.len() as u64, 0, 0, 16, 0),
        shdr(sn_rodata, 1, 0x2, base + off_rodata as u64, off_rodata as u64,
             rodata.len() as u64, 0, 0, 1, 0),
        shdr(sn_data, 1, 0x3, vaddr_data, off_data as u64, data.len() as u64, 0, 0, 8, 0),
        shdr(sn_symtab, 2, 0, 0, off_symtab as u64, symtab.len() as u64,
             5, 1, 8, 24), // link = .strtab, info = 1er symbole global
        shdr(sn_strtab, 3, 0, 0, off_strtab as u64, strtab.octets.len() as u64, 0, 0, 1, 0),
        shdr(sn_shstrtab, 3, 0, 0, off_shstrtab as u64, shstrtab.octets.len() as u64, 0, 0, 1, 0),
    ];
    for (i, s) in sh.iter().enumerate() {
        f[off_shdr + i * 64..off_shdr + (i + 1) * 64].copy_from_slice(s);
    }
    Ok(f)
}

/// Écrit l'exécutable ELF64 sur disque.
pub fn ecrire_elf(code: &[u8], options: &Options, chemin: &Path) -> Result<(), ErreurLien> {
    std::fs::write(chemin, elf_bytes(code, options)?)?;
    rendre_executable(chemin)
}

/// Variante pratique : prend directement le résultat de `natif_emetteur::emettre`.
pub fn ecrire_elf_depuis_emitted(emitted: &Emitted, chemin: &Path) -> Result<(), ErreurLien> {
    verifier_sans_runtime(emitted)?;
    ecrire_elf(&emitted.code, &Options::depuis_emitted(emitted), chemin)
}

/// Construit un exécutable PE64 en mémoire avec une table d'imports minimale
/// (`kernel32.dll!ExitProcess`) et un stub d'entrée **Win64**.
///
/// Structure vérifiable sous Linux (`file`, `objdump -x`, assertions octet par
/// octet) ; **l'exécution sous Windows n'a PAS été testée ici**. Voir
/// `docs/LINK_NATIF.md` §6 (preuves) et §7 (ABI du code émis, hors périmètre).
pub fn pe_bytes(code: &[u8], options: &Options) -> Result<Vec<u8>, ErreurLien> {
    if code.is_empty() { return Err(ErreurLien::CodeVide); }
    let octets_slots = options.n_slots_total.checked_mul(8).ok_or_else(|| {
        ErreurLien::Depassement("table de slots déborde usize".to_string())
    })?;
    let frame = aligner(octets_slots, 16);
    // `sub sp, #alloc` (ARM64) utilise un imm12 : 4095 o maximum.
    if frame > 0xFFF {
        return Err(ErreurLien::Depassement(
            "table de slots trop grande pour le stub Win64 (imm12)".to_string()));
    }

    // Un PE ne tourne que sous Windows : l'ABI Win64 est imposée ici, quel
    // que soit l'hôte qui produit le fichier (vérifiable sous Linux).
    let abi = Abi::Win64;
    // Le stub Win64 n'a pas de sortie décimale (pas de syscall Unix) : on
    // garde exactement le même octet pour les tests / produits PE existants.
    let stub_len = taille_stub(options.architecture, abi, frame, false, None, None);
    let rva_text = PE_RVA_TEXT;
    let rva_data = PE_RVA_DATA;
    let rva_code = rva_text + stub_len as u32;
    let rva_tramp_stub = rva_code + code.len() as u32;
    let image_base = PE_IMAGE_BASE;
    // Adresse virtuelle de l'entrée IAT de ExitProcess, patchée par le loader.
    let iat_abs = image_base + (rva_data + PE_IAT_OFF) as u64;

    let stub = stub_depart(options.architecture, abi,
                           image_base + rva_text as u64,
                           image_base + rva_code as u64,
                           image_base + rva_tramp_stub as u64,
                           iat_abs, frame, false, None, None);
    debug_assert_eq!(stub.len(), stub_len);
    let tramp = stub_trampoline(options.architecture);
    let mut text = Vec::with_capacity(stub_len + code.len() + tramp.len());
    text.extend_from_slice(&stub);
    text.extend_from_slice(code);
    text.extend_from_slice(&tramp);

    let text_raw = aligner(text.len(), 0x200);
    let off_text = 0x400usize;
    let off_data = off_text + text_raw;
    let taille = off_data + 0x200;

    // ── .data : trampoline + table d'imports ────────────────────────────
    let mut data = vec![0u8; 0x200];
    // table de trampoline (3 pointeurs lus par le code émis)
    ecrire_u64(&mut data, PE_TRAMP_OFF as usize, image_base + rva_tramp_stub as u64);
    ecrire_u64(&mut data, PE_TRAMP_OFF as usize + 8, image_base + rva_tramp_stub as u64);
    ecrire_u64(&mut data, PE_TRAMP_OFF as usize + 16, 0);
    // IMAGE_IMPORT_DESCRIPTOR pour KERNEL32.DLL + descripteur nul.
    let d = PE_IMPORT_DESC_OFF as usize;
    ecrire_u32(&mut data, d + 0, rva_data + PE_ILT_OFF);       // OriginalFirstThunk
    ecrire_u32(&mut data, d + 4, 0);                           // TimeDateStamp
    ecrire_u32(&mut data, d + 8, 0);                           // ForwarderChain
    ecrire_u32(&mut data, d + 12, rva_data + PE_DLL_NAME_OFF); // Name
    ecrire_u32(&mut data, d + 16, rva_data + PE_IAT_OFF);      // FirstThunk
    // (le second descripteur, à d+20, reste à zéro =terminateur)
    // IMAGE_IMPORT_BY_NAME : Hint (u16) + "ExitProcess\0".
    let h = PE_HINT_NAME_OFF as usize;
    ecrire_u16(&mut data, h, 0);
    data[h + 2..h + 14].copy_from_slice(b"ExitProcess\0");
    // ILT et IAT : RVA du nom, puis entrée nulle (PE32+ = 8 octets/entrée).
    for &off in &[PE_ILT_OFF, PE_IAT_OFF] {
        ecrire_u64(&mut data, off as usize, (rva_data + PE_HINT_NAME_OFF) as u64);
        ecrire_u64(&mut data, off as usize + 8, 0);
    }
    data[PE_DLL_NAME_OFF as usize..PE_DLL_NAME_OFF as usize + 13]
        .copy_from_slice(b"KERNEL32.DLL\0");

    // ── en-têtes ────────────────────────────────────────────────────────
    let mut f = vec![0u8; taille];
    f[0] = b'M'; f[1] = b'Z';
    ecrire_u32(&mut f, 0x3C, 0x80); // e_lfanew
    f[0x40..0x4A].copy_from_slice(b"Rose PE64\0");
    f[0x80..0x84].copy_from_slice(b"PE\0\0");
    ecrire_u16(&mut f, 0x84, machine_pe(options.architecture));
    ecrire_u16(&mut f, 0x86, 2);   // NumberOfSections
    ecrire_u16(&mut f, 0x94, 0xF0); // SizeOfOptionalHeader (PE32+)
    ecrire_u16(&mut f, 0x96, 0x22); // EXECUTABLE_IMAGE | LARGE_ADDRESS_AWARE
    let o = 0x98usize;              // début de l'en-tête optionnel PE32+
    ecrire_u16(&mut f, o, 0x20B);               // Magic = PE32+
    ecrire_u32(&mut f, o + 4, text_raw as u32); // SizeOfCode
    ecrire_u32(&mut f, o + 8, 0x200);           // SizeOfInitializedData
    // ⚠ L'entrée est le STUB, pas le code émis (l'ancien writer pointait
    //   directement sur le code : le loader n'aurait jamais fait le setup).
    ecrire_u32(&mut f, o + 16, rva_text);        // AddressOfEntryPoint
    ecrire_u32(&mut f, o + 20, rva_text);        // BaseOfCode
    ecrire_u64(&mut f, o + 24, image_base);      // ImageBase
    ecrire_u32(&mut f, o + 32, 0x1000);          // SectionAlignment
    ecrire_u32(&mut f, o + 36, 0x200);           // FileAlignment
    ecrire_u16(&mut f, o + 40, 6);               // MajorOSVersion
    ecrire_u16(&mut f, o + 48, 6);               // MajorSubsystemVersion
    ecrire_u32(&mut f, o + 56, 0x3000);          // SizeOfImage
    ecrire_u32(&mut f, o + 60, 0x400);           // SizeOfHeaders
    ecrire_u16(&mut f, o + 68, 3);               // Subsystem = CUI
    ecrire_u64(&mut f, o + 72, 0x10_0000);       // SizeOfStackReserve
    ecrire_u64(&mut f, o + 80, 0x1000);          // SizeOfStackCommit
    ecrire_u64(&mut f, o + 88, 0x10_0000);       // SizeOfHeapReserve
    ecrire_u64(&mut f, o + 96, 0x1000);          // SizeOfHeapCommit
    ecrire_u32(&mut f, o + 108, 16);             // NumberOfRvaAndSizes
    // Data directories (PE32+ : 16 × 8 octets à o+112).
    let dirs = o + 112;
    ecrire_u32(&mut f, dirs + 1 * 8, rva_data + PE_IMPORT_DESC_OFF); // [1] IMPORT
    ecrire_u32(&mut f, dirs + 1 * 8 + 4, 40);   // 2 descripteurs (dont le nul)
    ecrire_u32(&mut f, dirs + 12 * 8, rva_data + PE_IAT_OFF);        // [12] IAT
    ecrire_u32(&mut f, dirs + 12 * 8 + 4, 16);  // 2 entrées de 8 octets
    // ── section headers ─────────────────────────────────────────────────
    let s = o + 0xF0;
    f[s..s + 5].copy_from_slice(b".text");
    ecrire_u32(&mut f, s + 8, text.len() as u32);
    ecrire_u32(&mut f, s + 12, rva_text);
    ecrire_u32(&mut f, s + 16, text_raw as u32);
    ecrire_u32(&mut f, s + 20, off_text as u32);
    ecrire_u32(&mut f, s + 36, 0x6000_0020);     // CODE|EXECUTE|READ
    let d = s + 40;
    f[d..d + 5].copy_from_slice(b".data");
    ecrire_u32(&mut f, d + 8, 0x200);
    ecrire_u32(&mut f, d + 12, rva_data);
    ecrire_u32(&mut f, d + 16, 0x200);
    ecrire_u32(&mut f, d + 20, off_data as u32);
    ecrire_u32(&mut f, d + 36, 0xC000_0040);     // INITIALIZED_DATA|READ|WRITE
    // ── contenu brut ────────────────────────────────────────────────────
    f[off_text..off_text + text.len()].copy_from_slice(&text);
    f[off_data..off_data + data.len()].copy_from_slice(&data);
    Ok(f)
}

/// Écrit l'exécutable PE64 sur disque.
pub fn ecrire_pe(code: &[u8], options: &Options, chemin: &Path) -> Result<(), ErreurLien> {
    std::fs::write(chemin, pe_bytes(code, options)?)?;
    rendre_executable(chemin)
}

/// Variante pratique pour un résultat d'émission RIR.
pub fn ecrire_pe_depuis_emitted(emitted: &Emitted, chemin: &Path) -> Result<(), ErreurLien> {
    verifier_sans_runtime(emitted)?;
    ecrire_pe(&emitted.code, &Options::depuis_emitted(emitted), chemin)
}

fn verifier_sans_runtime(e: &Emitted) -> Result<(), ErreurLien> {
    if e.appels.is_empty() && e.fallbacks.is_empty() {
        Ok(())
    } else {
        Err(ErreurLien::RuntimeRequis)
    }
}

#[cfg(unix)]
fn rendre_executable(chemin: &Path) -> Result<(), ErreurLien> {
    use std::os::unix::fs::PermissionsExt;
    let mut p = std::fs::metadata(chemin)?.permissions();
    p.set_mode(0o755);
    std::fs::set_permissions(chemin, p)?;
    Ok(())
}
#[cfg(not(unix))]
fn rendre_executable(_chemin: &Path) -> Result<(), ErreurLien> { Ok(()) }

// ═══════════════════════════════════════════════════════════════════════
// ENCODAGE MACHINE DES STUBS
// ═══════════════════════════════════════════════════════════════════════

/// Longueur du stub : on le génère une fois avec des adresses factices (les
/// déplacements ne changent pas la taille des instructions).
fn taille_stub(arch: Architecture, abi: Abi, frame: usize, afficher: bool,
               donnees: Option<usize>,
               runtime: Option<&crate::natif_emetteur::RuntimeLiens>) -> usize {
    stub_depart(arch, abi, 0, 0, 0, 0, frame, afficher,
                donnees.map(|n| (0u64, n)), runtime).len()
}

/// Séquence « écrire la valeur en décimal sur stdout » (SysV / Linux).
///
/// ⚠ Le code de retour d'un processus POSIX est un OCTET : sans cette
/// séquence, 317811 sortirait 115. On garde le code de retour (utile pour
/// les valeurs ≤ 255) ET on rend la valeur complète visible sur stdout.
/// Les octets ont été validés par assemblage croisé (`aarch64-linux-gnu-as`)
/// pour AArch64 ; la suite x86-64 est encodée à la main (REX/SIB explicites).
fn sequence_affichage_a64() -> Vec<u8> {
    let mut v = Vec::new();
    for w in [
        0xAA00_03EEu32, // mov x14, x0   (sauvegarde de la valeur)
        0xAA0E_03E9,    // mov x9, x14   (dividende = la valeur)
        0x9100_43EA,    // add x10, sp, #16  (fin du tampon, dans la trame)
        0xD280_014B,    // mov x11, #10
        0xD100_054A,    // sub x10, x10, #1  (place du '\n')
        0x5280_014D,    // mov w13, #10
        0x3900_014D,    // strb w13, [x10]
    ] { p_u32(&mut v, w); }
    let debut_boucle = v.len();
    for w in [
        0x9ACB_092C,    // udiv x12, x9, x11
        0x9B0B_A58D,    // msub x13, x12, x11, x9
        0x9100_C1AD,    // add x13, x13, #'0'
        0xD100_054A,    // sub x10, x10, #1
        0x3900_014D,    // strb w13, [x10]
        0xAA0C_03E9,    // mov x9, x12
    ] { p_u32(&mut v, w); }
    let insn = v.len();
    let disp = (debut_boucle as i64 - insn as i64) / 4;
    p_u32(&mut v, 0xB500_0000 | (((disp as u32) & 0x7_FFFF) << 5) | 9); // cbnz x9
    for w in [
        0xD280_0020,    // mov x0, #1  (fd 1 = stdout)
        0xAA0A_03E1,    // mov x1, x10
        0x9100_43EF,    // add x15, sp, #16
        0xCB0A_01E2,    // sub x2, x15, x10  (longueur)
        0xD280_0808,    // mov x8, #64 (__NR_write)
        0xD400_0001,    // svc #0
        0xAA0E_03E0,    // mov x0, x14 (restitue la valeur pour `exit`)
    ] { p_u32(&mut v, w); }
    v
}

fn sequence_affichage_x86() -> Vec<u8> {
    let mut v = vec![
        0x49, 0x89, 0xC0,                   // mov r8, rax (valeur)
        0x4C, 0x8D, 0x4C, 0x24, 0x10,       // lea r9, [rsp+16]
        0x49, 0xFF, 0xC9,                   // dec r9  (place du '\n')
        0x41, 0xC6, 0x01, 0x0A,             // mov byte [r9], 10
        0x49, 0xBA, 10, 0, 0, 0, 0, 0, 0, 0,// movabs r10, 10
    ];
    let debut_boucle = v.len();
    v.extend_from_slice(&[
        0x48, 0x31, 0xD2,                   // xor rdx, rdx
        0x49, 0xF7, 0xF2,                   // div r10
        0x80, 0xC2, 0x30,                   // add dl, '0'
        0x49, 0xFF, 0xC9,                   // dec r9
        0x41, 0x88, 0x11,                   // mov [r9], dl
        0x48, 0x85, 0xC0,                   // test rax, rax
    ]);
    let insn = v.len();
    let disp = debut_boucle as i64 - (insn as i64 + 2);
    v.extend_from_slice(&[0x75, disp as u8]); // jnz boucle
    v.extend_from_slice(&[
        0xB8, 1, 0, 0, 0,                   // mov eax, 1 (__NR_write)
        0xBF, 1, 0, 0, 0,                   // mov edi, 1 (stdout)
        0x4C, 0x89, 0xCE,                   // mov rsi, r9
        0x48, 0x89, 0xE2,                   // mov rdx, rsp
        0x48, 0x83, 0xC2, 0x10,             // add rdx, 16
        0x4C, 0x29, 0xCA,                   // sub rdx, r9 (longueur)
        0x0F, 0x05,                         // syscall
        0x4C, 0x89, 0xC0,                   // mov rax, r8 (REX.R : source = r8)
    ]);
    v
}

// ═══════════════════════════════════════════════════════════════════════
// AOT « VALEURS EXACTES » — écrire une chaîne de digits mémorisée
// ═══════════════════════════════════════════════════════════════════════
//
// `sequence_affichage_*` sait écrire un i64 en le divisant par 10. Une
// primorielle comme `565#` a 228 chiffres : la division i64 ne suffit plus.
// On mémorise donc le TEXTE DÉCIMAL complet dans `.rodata` (calculé par
// l'interpréteur Rose à la compilation) et le stub fait un unique
// `write(1, adresse, longueur)`. La suite d'instructions a une LONGUEUR FIXE,
// indépendante de l'adresse et de la longueur : c'est ce qui permet de
// calculer `stub_len` avant de connaître l'emplacement de `.rodata`.

/// Suite SysV AMD64 : `write(1, addr, len)`, puis restauration de la valeur
/// rendue par le code machine pour le `exit` (r8 ← rax au début).
fn sequence_ecrire_donnees_x86(addr: u64, len: usize) -> Vec<u8> {
    let mut v = vec![
        0x49, 0x89, 0xC0,                   // mov r8, rax (valeur sauvegardée)
        0xB8, 1, 0, 0, 0,                   // mov eax, 1 (__NR_write)
        0xBF, 1, 0, 0, 0,                   // mov edi, 1 (stdout)
        0x48, 0xBE,                         // movabs rsi, addr
    ];
    v.extend_from_slice(&addr.to_le_bytes());
    v.extend_from_slice(&[0x48, 0xBA]);     // movabs rdx, len
    v.extend_from_slice(&(len as u64).to_le_bytes());
    v.extend_from_slice(&[
        0x0F, 0x05,                         // syscall
        0x4C, 0x89, 0xC0,                   // mov rax, r8 (REX.R : source = r8)
    ]);
    v
}

/// Suite AAPCS64 : `write(1, addr, len)`. Les immédiats sont émis en quatre
/// `movz`/`movk` (16 bits chacun), donc FIXES en longueur, quelle que soit
/// l'adresse de `.rodata` (0x0040_0000 + décalage).
fn sequence_ecrire_donnees_a64(addr: u64, len: usize) -> Vec<u8> {
    let mut v = Vec::with_capacity(44);
    let mut p = |w: u32| v.extend_from_slice(&w.to_le_bytes());
    p(0xAA00_03EE); // mov x14, x0 (valeur sauvegardée)
    p(0xD280_0020); // mov x0, #1 (stdout)
    for i in 0..4 {
        let imm = ((addr >> (16 * i)) & 0xFFFF) as u32;
        // movz x1, #imm (i=0) ; movk x1, #imm, lsl #16/32/48 (i>0)
        // ⚠ 0xD280_0000 = movz SUR 64 BITS (x1), 0xF280_0000 = movk x1 ;
        //   0x5280_0000 / 0x7280_0000 seraient les formes 32 bits (w1).
        p((if i == 0 { 0xD280_0000 } else { 0xF280_0000 | ((i as u32) << 21) })
          | (imm << 5) | 1);
    }
    for i in 0..2 {
        let imm = ((len >> (16 * i)) & 0xFFFF) as u64 as u32;
        p((if i == 0 { 0xD280_0000 } else { 0xF280_0000 | ((i as u32) << 21) })
          | (imm << 5) | 2);
    }
    p(0xD280_0808); // mov x8, #64 (__NR_write)
    p(0xD400_0001); // svc #0
    p(0xAA0E_03E0); // mov x0, x14 (restitue la valeur pour `exit`)
    v
}

fn stub_depart(arch: Architecture, abi: Abi, text_vaddr: u64, code_vaddr: u64,
               tramp_vaddr: u64, iat_vaddr: u64, frame: usize,
               afficher: bool, donnees: Option<(u64, usize)>,
               runtime: Option<&crate::natif_emetteur::RuntimeLiens>) -> Vec<u8> {
    let mut s = Vec::new();
    match (abi, arch) {
        // ── SysV / AAPCS64 (ELF) : terminer par le syscall exit ─────────
        (Abi::SysV, Architecture::Aarch64) => {
            // sub sp, sp, #frame  (imm12 non mis à l'échelle)
            p_u32(&mut s, 0xD100_0000 | (((frame as u32) & 0xFFF) << 10) | (31 << 5) | 31);
            p_u32(&mut s, 0x9100_03E0); // mov x0, sp   → base
            let ici = text_vaddr + 8;
            p_u32(&mut s, a64_adr(1, tramp_vaddr as i64 - ici as i64)); // adr x1, tramp
            let ici = text_vaddr + 12;
            p_u32(&mut s, a64_bl(code_vaddr as i64 - ici as i64));      // bl rose_code
            // AOT6 : le runtime natif formate puis écrit SORTIE sur stdout.
            if let Some(rt) = runtime.filter(|r| r.tag != 0) {
                if let Some(off) = rt.formater {
                    let ici = text_vaddr + s.len() as u64;
                    p_u32(&mut s, a64_bl(code_vaddr as i64 + off as i64 - ici as i64));
                }
                let ici = text_vaddr + s.len() as u64;
                p_u32(&mut s, a64_bl(code_vaddr as i64 + rt.sortir as i64 - ici as i64));
            }
            if let Some((addr, len)) = donnees {
                s.extend_from_slice(&sequence_ecrire_donnees_a64(addr, len));
            } else if afficher { s.extend_from_slice(&sequence_affichage_a64()); }
            p_u32(&mut s, 0xD280_0BA8); // mov x8, #93 (__NR_exit)
            p_u32(&mut s, 0xD400_0001); // svc #0
        }
        (Abi::SysV, Architecture::X86_64) => {
            if frame <= 127 {
                s.extend_from_slice(&[0x48, 0x83, 0xEC, frame as u8]); // sub rsp, imm8
            } else {
                s.extend_from_slice(&[0x48, 0x81, 0xEC]);
                s.extend_from_slice(&(frame as u32).to_le_bytes()); // sub rsp, imm32
            }
            s.extend_from_slice(&[0x48, 0x89, 0xE7]); // mov rdi, rsp → base
            let off = s.len();
            s.extend_from_slice(&[0x48, 0x8D, 0x35]); // lea rsi, [rip+d32] → tramp
            let d = tramp_vaddr as i64 - (text_vaddr + off as u64 + 7) as i64;
            s.extend_from_slice(&(d as i32).to_le_bytes());
            let off = s.len();
            s.push(0xE8); // call rose_code
            let d = code_vaddr as i64 - (text_vaddr + off as u64 + 5) as i64;
            s.extend_from_slice(&(d as i32).to_le_bytes());
            // AOT6 : appels aux routines natives de formatage / sortie.
            if let Some(rt) = runtime.filter(|r| r.tag != 0) {
                if let Some(off) = rt.formater {
                    let ici = s.len();
                    s.push(0xE8);
                    let d = (code_vaddr + off as u64) as i64
                        - (text_vaddr + ici as u64 + 5) as i64;
                    s.extend_from_slice(&(d as i32).to_le_bytes());
                }
                let ici = s.len();
                s.push(0xE8);
                let d = (code_vaddr + rt.sortir as u64) as i64
                    - (text_vaddr + ici as u64 + 5) as i64;
                s.extend_from_slice(&(d as i32).to_le_bytes());
            }
            if let Some((addr, len)) = donnees {
                s.extend_from_slice(&sequence_ecrire_donnees_x86(addr, len));
            } else if afficher { s.extend_from_slice(&sequence_affichage_x86()); }
            s.extend_from_slice(&[0x48, 0x89, 0xC7]); // mov rdi, rax → code de sortie
            s.extend_from_slice(&[0xB8, 0x3C, 0x00, 0x00, 0x00]); // mov eax, 60 (__NR_exit)
            s.extend_from_slice(&[0x0F, 0x05]); // syscall
        }
        // ── Win64 (PE) : ExitProcess via l'IAT, shadow space + align 16 ──
        (Abi::Win64, Architecture::X86_64) => {
            // À l'entrée RSP ≡ 8 (mod 16) (l'appelant a poussé l'adresse de
            // retour). On réserve 32 o de shadow space + 8 o d'alignement +
            // le frame des slots : RSP ≡ 0 (mod 16) avant chaque `call`.
            let alloc = 40usize + frame;
            if alloc <= 127 {
                s.extend_from_slice(&[0x48, 0x83, 0xEC, alloc as u8]); // sub rsp, imm8
            } else {
                s.extend_from_slice(&[0x48, 0x81, 0xEC]);
                s.extend_from_slice(&(alloc as u32).to_le_bytes());
            }
            // L'émetteur M3.1 attend base dans rdi et tramp dans rsi (SysV),
            // quelles que soient les conventions utilisées plus haut : on les
            // pose explicitement ici, l'appelant étant notre propre stub.
            s.extend_from_slice(&[0x48, 0x89, 0xE7]); // mov rdi, rsp → base
            s.extend_from_slice(&[0x48, 0xBE]);       // movabs rsi, tramp
            s.extend_from_slice(&tramp_vaddr.to_le_bytes());
            let off = s.len();
            s.push(0xE8); // call rose_code
            let d = code_vaddr as i64 - (text_vaddr + off as u64 + 5) as i64;
            s.extend_from_slice(&(d as i32).to_le_bytes());
            s.extend_from_slice(&[0x89, 0xC1]); // mov ecx, eax → uExitCode
            if frame <= 127 {
                s.extend_from_slice(&[0x48, 0x83, 0xC4, frame as u8]); // add rsp, frame
            } else {
                s.extend_from_slice(&[0x48, 0x81, 0xC4]);
                s.extend_from_slice(&(frame as u32).to_le_bytes());
            }
            // RSP pointe maintenant sur les 32 octets de shadow space.
            let off = s.len();
            s.extend_from_slice(&[0xFF, 0x15]); // call qword ptr [rip+d32]
            let d = iat_vaddr as i64 - (text_vaddr + off as u64 + 6) as i64;
            s.extend_from_slice(&(d as i32).to_le_bytes());
            s.extend_from_slice(&[0x0F, 0x0B]); // ud2 (ExitProcess ne revient pas)
        }
        (Abi::Win64, Architecture::Aarch64) => {
            // Windows ARM64 : SP déjà aligné sur 16 à l'entrée (LR porte
            // l'adresse de retour). 32 o de "home space" + le frame des slots.
            let alloc = 32usize + frame;
            debug_assert!(alloc <= 0xFFF);
            p_u32(&mut s, 0xD100_0000 | (((alloc as u32) & 0xFFF) << 10) | (31 << 5) | 31);
            p_u32(&mut s, 0x9100_03E0); // mov x0, sp → base (AAPCS64/WinARM64)
            let ici = text_vaddr + 8;
            p_u32(&mut s, a64_adr(1, tramp_vaddr as i64 - ici as i64)); // adr x1, tramp
            let ici = text_vaddr + 12;
            p_u32(&mut s, a64_bl(code_vaddr as i64 - ici as i64));      // bl rose_code
            // x0 contient déjà le code de retour (uExitCode = 32 bits).
            p_u32(&mut s, 0x9100_0000 | (((frame as u32) & 0xFFF) << 10) | (31 << 5) | 31);
            // add sp, sp, #frame → sp pointe sur la home space
            let ici = text_vaddr + 20;
            p_u32(&mut s, a64_adr(9, iat_vaddr as i64 - ici as i64)); // adr x9, IAT
            p_u32(&mut s, 0xF940_0129); // ldr x9, [x9]  (adresse ExitProcess)
            p_u32(&mut s, 0xD63F_0120); // blr x9
            p_u32(&mut s, 0xD420_0000); // brk #0
        }
    }
    s
}

fn stub_trampoline(arch: Architecture) -> Vec<u8> {
    match arch {
        // rend 0 dans x0 (les appels runtime ne sont pas encore branchés).
        Architecture::Aarch64 => {
            let mut s = Vec::new();
            p_u32(&mut s, 0xD280_0000); // mov x0, #0
            p_u32(&mut s, 0xD65F_03C0); // ret
            s
        }
        Architecture::X86_64 => vec![0x31, 0xC0, 0xC3], // xor eax,eax ; ret
    }
}

fn a64_adr(rd: u32, delta: i64) -> u32 {
    let imm = (delta as u64) & 0x1_FFFF;
    0x1000_0000 | (((imm & 3) as u32) << 29) | ((((imm >> 2) & 0x7_FFFF) as u32) << 5) | rd
}
fn a64_bl(delta: i64) -> u32 {
    0x9400_0000 | ((((delta / 4) as u64) & 0x03FF_FFFF) as u32)
}
fn machine_elf(arch: Architecture) -> u16 {
    match arch { Architecture::Aarch64 => 183, Architecture::X86_64 => 62 }
}
fn machine_pe(arch: Architecture) -> u16 {
    // 0xAA64 = IMAGE_FILE_MACHINE_ARM64. (0xA641 = ARM64EC, une saveur
    // d'émulation différente : ne pas l'utiliser pour un PE ARM64 natif.)
    match arch { Architecture::Aarch64 => 0xAA64, Architecture::X86_64 => 0x8664 }
}

// ═══════════════════════════════════════════════════════════════════════
// PETITS OUTILS D'ÉCRITURE
// ═══════════════════════════════════════════════════════════════════════

fn aligner(v: usize, a: usize) -> usize { (v + a - 1) & !(a - 1) }
fn p_u32(v: &mut Vec<u8>, x: u32) { v.extend_from_slice(&x.to_le_bytes()); }
fn p_u64(v: &mut Vec<u8>, x: u64) { v.extend_from_slice(&x.to_le_bytes()); }
fn ecrire_u16(v: &mut [u8], o: usize, x: u16) { v[o..o + 2].copy_from_slice(&x.to_le_bytes()); }
fn ecrire_u32(v: &mut [u8], o: usize, x: u32) { v[o..o + 4].copy_from_slice(&x.to_le_bytes()); }
fn ecrire_u64(v: &mut [u8], o: usize, x: u64) { v[o..o + 8].copy_from_slice(&x.to_le_bytes()); }

struct StrTab { octets: Vec<u8> }
impl StrTab {
    fn neuve() -> Self { Self { octets: vec![0] } }
    fn ajouter(&mut self, s: &str) -> u32 {
        let o = self.octets.len() as u32;
        self.octets.extend_from_slice(s.as_bytes());
        self.octets.push(0);
        o
    }
}

fn sym(v: &mut Vec<u8>, nom: u32, info: u8, shndx: u16, valeur: u64, taille: u64) {
    v.extend_from_slice(&nom.to_le_bytes());
    v.push(info);
    v.push(0); // st_other
    v.extend_from_slice(&shndx.to_le_bytes());
    v.extend_from_slice(&valeur.to_le_bytes());
    v.extend_from_slice(&taille.to_le_bytes());
}
fn shdr(nom: u32, typ: u32, flags: u64, addr: u64, off: u64, taille: u64,
        link: u32, info: u32, align: u64, entsize: u64) -> [u8; 64] {
    let mut b = [0u8; 64];
    b[0..4].copy_from_slice(&nom.to_le_bytes());
    b[4..8].copy_from_slice(&typ.to_le_bytes());
    b[8..16].copy_from_slice(&flags.to_le_bytes());
    b[16..24].copy_from_slice(&addr.to_le_bytes());
    b[24..32].copy_from_slice(&off.to_le_bytes());
    b[32..40].copy_from_slice(&taille.to_le_bytes());
    b[40..44].copy_from_slice(&link.to_le_bytes());
    b[44..48].copy_from_slice(&info.to_le_bytes());
    b[48..56].copy_from_slice(&align.to_le_bytes());
    b[56..64].copy_from_slice(&entsize.to_le_bytes());
    b
}

// ═══════════════════════════════════════════════════════════════════════
// TESTS — preuve d'exécution réelle
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rir::{CorpsRir, Eabstrait, Op};

    fn programme_49() -> Emitted {
        // `retour 49` : l'émetteur produit l'équivalent de `mov x0, #49 ; ret`.
        let corps = CorpsRir {
            ops: vec![Op::RetourAbstrait(Eabstrait::Entier("49".into()))],
            noms: vec![],
            globaux: vec![],
            n_slots: 0,
        };
        crate::natif_emetteur::emettre(&corps, Architecture::Aarch64).unwrap()
    }

    #[test]
    fn entete_elf_aarch64_est_valide() {
        let e = programme_49();
        let octets = elf_bytes(&e.code, &Options::depuis_emitted(&e)).unwrap();
        assert_eq!(&octets[0..4], b"\x7FELF");
        assert_eq!(octets[4], 2); // ELF64
        assert_eq!(octets[5], 1); // little endian
        let machine = u16::from_le_bytes([octets[18], octets[19]]);
        assert_eq!(machine, 183); // EM_AARCH64
        let entree = u64::from_le_bytes(octets[24..32].try_into().unwrap());
        assert_eq!(entree, ADRESSE_BASE + 0x100);
        assert_eq!(u16::from_le_bytes([octets[16], octets[17]]), 2); // ET_EXEC
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn elf_aarch64_retourne_49_en_reel() {
        use std::os::unix::process::ExitStatusExt;
        if Architecture::native() != Architecture::Aarch64 {
            eprintln!("test ignoré : hôte non-aarch64");
            return;
        }
        let e = programme_49();
        let octets = elf_bytes(&e.code, &Options::depuis_emitted(&e)).unwrap();
        // Chemin déterministe : laisse le fichier pour `readelf -h` / `file`.
        let chemin = std::env::temp_dir().join("rose_m3_2_prog.elf");
        std::fs::write(&chemin, &octets).unwrap();
        rendre_executable(&chemin).unwrap();

        let statut = std::process::Command::new(&chemin)
            .status()
            .expect("impossible de lancer l'ELF produit");
        // Un code de retour 49 n'est PAS un `success()` : on vérifie juste
        // qu'il n'y a pas eu de signal et que le code est bien celui attendu.
        assert!(statut.signal().is_none(), "signal inattendu : {:?}", statut.signal());
        assert_eq!(statut.code(), Some(49),
                   "code de retour attendu 49, obtenu {:?}", statut.code());
        eprintln!("ELF exécuté : {} → code {}", chemin.display(), statut.code().unwrap());
    }

    #[test]
    fn squelette_pe_a_la_bonne_structure() {
        let e = programme_49();
        let o = pe_bytes(&e.code, &Options::depuis_emitted(&e)).unwrap();
        assert_eq!(&o[0..2], b"MZ");
        let lfanew = u32::from_le_bytes(o[0x3C..0x40].try_into().unwrap()) as usize;
        assert_eq!(&o[lfanew..lfanew + 4], b"PE\0\0");
        let machine = u16::from_le_bytes([o[0x84], o[0x85]]);
        assert_eq!(machine, 0xAA64); // IMAGE_FILE_MACHINE_ARM64
        let opt = lfanew + 4 + 20;
        assert_eq!(u16::from_le_bytes([o[opt], o[opt + 1]]), 0x20B); // PE32+
        // L'entrée pointe sur le stub (0x1000), pas sur le code émis.
        assert_eq!(u32::from_le_bytes(o[opt + 16..opt + 20].try_into().unwrap()), PE_RVA_TEXT);
        let sections = opt + 0xF0;
        assert_eq!(&o[sections..sections + 5], b".text");
        assert_eq!(&o[sections + 40..sections + 45], b".data");
    }

    fn programme_49_x86() -> Emitted {
        let corps = CorpsRir {
            ops: vec![Op::RetourAbstrait(Eabstrait::Entier("49".into()))],
            noms: vec![], globaux: vec![], n_slots: 0,
        };
        crate::natif_emetteur::emettre(&corps, Architecture::X86_64).unwrap()
    }

    #[test]
    fn pe_import_et_stub_win64_sont_complets() {
        let e = programme_49_x86();
        let o = pe_bytes(&e.code, &Options::depuis_emitted(&e)).unwrap();

        // ── en-têtes ────────────────────────────────────────────────────
        assert_eq!(&o[0..2], b"MZ");
        assert_eq!(u32::from_le_bytes(o[0x3C..0x40].try_into().unwrap()), 0x80);
        assert_eq!(&o[0x80..0x84], b"PE\0\0");
        assert_eq!(u16::from_le_bytes([o[0x84], o[0x85]]), 0x8664); // AMD64
        assert_eq!(u16::from_le_bytes([o[0x94], o[0x95]]), 0xF0);
        let opt = 0x98usize;
        assert_eq!(u16::from_le_bytes([o[opt], o[opt + 1]]), 0x20B); // PE32+
        assert_eq!(u32::from_le_bytes(o[opt + 16..opt + 20].try_into().unwrap()),
                   PE_RVA_TEXT, "AddressOfEntryPoint doit viser le stub");
        assert_eq!(u64::from_le_bytes(o[opt + 24..opt + 32].try_into().unwrap()),
                   PE_IMAGE_BASE);
        assert_eq!(u32::from_le_bytes(o[opt + 108..opt + 112].try_into().unwrap()), 16);

        // ── data directories ────────────────────────────────────────────
        let dirs = opt + 112;
        assert_eq!(u32::from_le_bytes(o[dirs..dirs + 4].try_into().unwrap()), 0,
                   "pas de table d'export");
        assert_eq!(u32::from_le_bytes(o[dirs + 8..dirs + 12].try_into().unwrap()),
                   PE_RVA_DATA + PE_IMPORT_DESC_OFF, "IMAGE_DIRECTORY_ENTRY_IMPORT");
        assert_eq!(u32::from_le_bytes(o[dirs + 12..dirs + 16].try_into().unwrap()), 40);
        assert_eq!(u32::from_le_bytes(o[dirs + 12 * 8..dirs + 12 * 8 + 4].try_into().unwrap()),
                   PE_RVA_DATA + PE_IAT_OFF, "IMAGE_DIRECTORY_ENTRY_IAT");
        assert_eq!(u32::from_le_bytes(o[dirs + 12 * 8 + 4..dirs + 12 * 8 + 8].try_into().unwrap()), 16);

        // ── .data : descripteurs, ILT/IAT, noms ─────────────────────────
        let sec = opt + 0xF0;
        let off_data = u32::from_le_bytes(o[sec + 40 + 20..sec + 40 + 24].try_into().unwrap()) as usize;
        assert_eq!(&o[sec + 40..sec + 45], b".data");
        let d = off_data + PE_IMPORT_DESC_OFF as usize;
        assert_eq!(u32::from_le_bytes(o[d..d + 4].try_into().unwrap()), PE_RVA_DATA + PE_ILT_OFF);
        assert_eq!(u32::from_le_bytes(o[d + 12..d + 16].try_into().unwrap()), PE_RVA_DATA + PE_DLL_NAME_OFF);
        assert_eq!(u32::from_le_bytes(o[d + 16..d + 20].try_into().unwrap()), PE_RVA_DATA + PE_IAT_OFF);
        assert_eq!(&o[d + 20..d + 40], &[0u8; 20], "descripteur nul de fin");
        let h = off_data + PE_HINT_NAME_OFF as usize;
        assert_eq!(u16::from_le_bytes([o[h], o[h + 1]]), 0);
        assert_eq!(&o[h + 2..h + 14], b"ExitProcess\0");
        for &off in &[PE_ILT_OFF, PE_IAT_OFF] {
            let p = off_data + off as usize;
            assert_eq!(u64::from_le_bytes(o[p..p + 8].try_into().unwrap()),
                       (PE_RVA_DATA + PE_HINT_NAME_OFF) as u64);
            assert_eq!(&o[p + 8..p + 16], &[0u8; 8]);
        }
        assert_eq!(&o[off_data + PE_DLL_NAME_OFF as usize..
                      off_data + PE_DLL_NAME_OFF as usize + 13], b"KERNEL32.DLL\0");

        // ── stub Win64 : shadow space, alignement, appel via IAT ────────
        let off_text = u32::from_le_bytes(o[sec + 20..sec + 24].try_into().unwrap()) as usize;
        let frame = aligner(e.n_slots_total * 8, 16); // 16 slots → 128 o
        assert_eq!(frame, 128);
        let stub = &o[off_text..off_text + 42];
        assert_eq!(&stub[0..3], &[0x48, 0x81, 0xEC], "sub rsp, imm32");
        assert_eq!(u32::from_le_bytes(stub[3..7].try_into().unwrap()), (40 + frame) as u32);
        assert_eq!(&stub[7..10], &[0x48, 0x89, 0xE7], "mov rdi, rsp (base)");
        assert_eq!(&stub[10..12], &[0x48, 0xBE], "movabs rsi, tramp");
        assert_eq!(stub[20], 0xE8, "call rose_code");
        assert_eq!(&stub[25..27], &[0x89, 0xC1], "mov ecx, eax (uExitCode)");
        assert_eq!(&stub[27..30], &[0x48, 0x81, 0xC4], "add rsp, imm32");
        assert_eq!(u32::from_le_bytes(stub[30..34].try_into().unwrap()), frame as u32);
        assert_eq!(&stub[34..36], &[0xFF, 0x15], "call [rip+d32] via IAT");
        let iat_abs = PE_IMAGE_BASE + (PE_RVA_DATA + PE_IAT_OFF) as u64;
        let rip = PE_IMAGE_BASE + PE_RVA_TEXT as u64 + 40;
        assert_eq!(i32::from_le_bytes(stub[36..40].try_into().unwrap()) as i64,
                   iat_abs as i64 - rip as i64);
        assert_eq!(&stub[40..42], &[0x0F, 0x0B], "ud2");
    }

    #[test]
    fn pe_arm64_machine_et_stub_blr() {
        let e = programme_49();
        let o = pe_bytes(&e.code, &Options::depuis_emitted(&e)).unwrap();
        assert_eq!(u16::from_le_bytes([o[0x84], o[0x85]]), 0xAA64); // ARM64
        let sec = 0x98 + 0xF0;
        let off_text = u32::from_le_bytes(o[sec + 20..sec + 24].try_into().unwrap()) as usize;
        let stub = &o[off_text..off_text + 32];
        let alloc = 32 + aligner(e.n_slots_total * 8, 16); // home space + frame
        let sub_sp = 0xD100_0000u32 | (((alloc as u32) & 0xFFF) << 10) | (31 << 5) | 31;
        assert_eq!(&stub[0..4], &sub_sp.to_le_bytes()); // sub sp,sp,#alloc
        assert_eq!(&stub[24..28], &0xF940_0129u32.to_le_bytes()); // ldr x9,[x9]
        assert_eq!(&stub[28..32], &0xD63F_0120u32.to_le_bytes()); // blr x9
        assert_eq!(&o[off_text + 32..off_text + 36], &0xD420_0000u32.to_le_bytes()); // brk #0
    }

    #[test]
    fn pe_ecrit_fichiers_pour_outils_externes() {
        let dir = std::env::temp_dir().join("opencode");
        std::fs::create_dir_all(&dir).unwrap();
        for (nom, e) in [
            ("rose_m3_2_pe_x86_64.exe", programme_49_x86()),
            ("rose_m3_2_pe_arm64.exe", programme_49()),
        ] {
            let chemin = dir.join(nom);
            let octets = pe_bytes(&e.code, &Options::depuis_emitted(&e)).unwrap();
            std::fs::write(&chemin, &octets).unwrap();
            eprintln!("PE écrit : {} ({} octets)", chemin.display(), octets.len());
        }
    }

    #[test]
    fn stub_avec_affichage_est_plus_long_et_contient_write() {
        let e = programme_49_x86();
        let nu = elf_bytes(&e.code, &Options::depuis_emitted(&e)).unwrap();
        let mut o = Options::depuis_emitted(&e);
        o.afficher_valeur = true;
        let avec = elf_bytes(&e.code, &o).unwrap();
        assert!(avec.len() > nu.len(), "le stub d'affichage doit être émis");
        // `mov rax,r8` (REX.R) : restaure la valeur après le syscall write.
        assert!(avec.windows(3).any(|w| w == [0x4C, 0x89, 0xC0]));
        // Le stub sans affichage reste octet pour octet conforme aux tests M3.2.
        assert!(!nu.windows(3).any(|w| w == [0x4C, 0x89, 0xC0]));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn elf_avec_valeur_exacte_ecrit_le_texte_memorise() {
        let e = programme_49();
        if Architecture::native() != Architecture::Aarch64 { return; }
        let nu = elf_bytes(&e.code, &Options::depuis_emitted(&e)).unwrap();
        let mut o = Options::depuis_emitted(&e);
        let texte = b"808257643035939103769006214256896739823014172881267515160156\n";
        o.sortie_exacte = Some(texte.to_vec());
        let octets = elf_bytes(&e.code, &o).unwrap();
        assert!(octets.len() > nu.len(), "la séquence write doit être émise");
        assert!(octets.windows(texte.len()).any(|w| w == texte),
                "le texte exact doit être mémorisé dans .rodata");

        let chemin = std::env::temp_dir().join(format!(
            "rose_aot3_valeur_exacte_{}_{}.elf", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default().as_nanos()));
        std::fs::write(&chemin, &octets).unwrap();
        rendre_executable(&chemin).unwrap();
        let sortie = std::process::Command::new(&chemin).output()
            .expect("impossible de lancer l'ELF AOT3");
        assert!(sortie.status.success() || sortie.status.code() == Some(49),
                "status inattendu : {:?}", sortie.status);
        assert_eq!(sortie.stdout, texte);
        eprintln!("AOT3 : {} → {} octets sur stdout", chemin.display(), sortie.stdout.len());
    }

    #[test]
    fn runtime_requis_est_refuse() {
        let e = programme_49();
        let mut avec_appel = e;
        avec_appel.appels.push(crate::natif_emetteur::Appel {
            nom: "f".to_string(), args_slots: [None; 4],
        });
        let chemin = std::env::temp_dir().join("rose_m3_2_jamais_ecrit.elf");
        assert!(matches!(ecrire_elf_depuis_emitted(&avec_appel, &chemin),
                         Err(ErreurLien::RuntimeRequis)));
    }
}
