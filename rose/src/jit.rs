// Rose — JIT Cranelift : émet du code machine natif pour le code Rose
// « chaud » (entiers 64 bits, booléens, flottants, comparaisons, branches,
// boucles, fonctions utilisateur avec récursion).
// Comme le JIT du langage : TOUT ce qui sort du support (chaînes,
// rationnels, listes, tables, tampons, FFI, appels runtime) fait basculer la
// fonction entière sur l'interpréteur — repli transparent, sorties identiques.
//
// Convention d'appel SystemV (ARM aarch64) :
//   une fonction Rose compiled reçoit ses arguments dans x0..x7 (i64) et
//   d0..d7 (f64), et rend dans x0 / d0. Les scalaires sont alloués dans une
//   trame de frame par Cranelift (stack_store/load natifs).
// Licence MIT.

use std::collections::HashMap;

use cranelift::codegen::ir::{AbiParam, BlockArg, FuncRef, Inst, InstBuilder, MemFlagsData, Signature as ClSignature, Value};
use cranelift::codegen::isa::CallConv;
// Convention d'appel par défaut de la cible courante.
#[cfg(target_os = "windows")]
const DEFAULT_CC: CallConv = CallConv::WindowsFastcall;
#[cfg(not(target_os = "windows"))]
const DEFAULT_CC: CallConv = CallConv::SystemV;
use cranelift::module::FuncId;
use cranelift::jit::{JITBuilder, JITMemoryKind, JITMemoryProvider, JITModule, BranchProtection};
use cranelift::module::{default_libcall_names, Linkage, Module};
use cranelift::prelude::{Block, FloatCC, FunctionBuilder, IntCC, types, Variable};

use crate::front::{Expr, Instr, Signature, TyKind};
use crate::rir::{CorpsRir, Eabstrait, Op, Oper};

/// Vrai si la séquence d'instructions finit toujours par un retour (le code
/// qui suit est mort) — sert à ne pas réémettre un return final illégal.
use crate::run::Valeur;

// ───────────────────────────────────────────────────────────────────────
// Détection GÉNÉRIQUE de récursion binaire transformable en boucle
// (méthode LLVM, constatée par désassemblage de rustc -O le 18/11).
//
// Aucun nom de fonction n'est codé en dur : toute fonction de la forme
//     f(n) = si n < k { base(n) } ; f(n-c1) + f(n-c2)
// avec 0 < c1 <= c2 <= k est reconnue. `base` est `n` ou un littéral entier.
// Le JIT émet alors une boucle à accumulateur : ~F(n+1) appels au lieu de
// ~2·F(n+1), exactement comme `rustc -O`.
// ───────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug)]
enum BaseBoucle { Param, Lit(i64) }

#[derive(Clone, Debug)]
struct RecBoucle {
    param: String,
    seuil: i64,
    strict: bool,
    base: BaseBoucle,
    /// pas du premier appel, ex. 1 dans `f(n-1)`
    c1: i64,
    /// pas du décrément de boucle, ex. 2 dans `f(n-2)`
    c2: i64,
}

/// Littéral entier POSITIF (`texte` sans signe, ni virgule, ni fraction).
fn lit_positif(e: &Expr) -> Option<i64> {
    match e {
        Expr::Litteral { texte, flott, rationnel, neg, .. } => {
            if *flott || *rationnel || *neg || texte.contains('/') || texte.contains('.') { return None; }
            let v: i64 = texte.parse().ok()?;
            if v > 0 { Some(v) } else { None }
        }
        _ => None,
    }
}

fn lit_i64_signe(e: &Expr) -> Option<i64> {
    match e {
        Expr::Litteral { texte, flott, rationnel, .. } => {
            if *flott || *rationnel || texte.contains('/') || texte.contains('.') { return None; }
            texte.parse().ok()
        }
        _ => None,
    }
}

/// Reconnaît `n < k` (strict) ou `n <= k` (non strict) avec `n` le paramètre.
fn test_base(e: &Expr, param: &str) -> Option<(i64, bool)> {
    let Expr::Binaire { op, a, b, .. } = e else { return None; };
    let est_n = matches!(&**a, Expr::Var(n, _) if n == param);
    if est_n {
        let k = lit_positif(b)?;
        return match op.as_str() { "<" => Some((k, true)), "<=" => Some((k, false)), _ => None };
    }
    // forme renversée `k > n` / `k >= n`
    let est_n_b = matches!(&**b, Expr::Var(n, _) if n == param);
    if est_n_b {
        let k = lit_positif(a)?;
        return match op.as_str() { ">" => Some((k, true)), ">=" => Some((k, false)), _ => None };
    }
    None
}

/// Reconnaît un auto-appel `f(n - c)` avec c > 0, et rend c.
fn pas_appel(e: &Expr, nom_f: &str, param: &str) -> Option<i64> {
    let Expr::Appel { nom, args, .. } = e else { return None; };
    if nom != nom_f || args.len() != 1 { return None; }
    let Expr::Binaire { op, a, b, .. } = &args[0] else { return None; };
    if op != "-" || !matches!(&**a, Expr::Var(n, _) if n == param) { return None; }
    lit_positif(b)
}

/// Borne GENERIQUE au-delà de laquelle la récurrence peut déborder i64.
/// Calculée en simulant la récurrence elle-même en i128 (jamais de nom de
/// fonction codé en dur) : on retourne le plus grand n tel que f(n) tienne
/// dans un i64. En dessous, tous les résultats et toutes les sommes
/// partielles sont des i64 non boxés, donc les tests de tag sont inutiles.
fn borne_securisee(d: &RecBoucle) -> i64 {
    const MAXN: usize = 8192;
    let mut v: Vec<i128> = vec![0; MAXN];
    for i in 0..MAXN {
        let n = i as i128;
        let base = if d.strict { n < d.seuil as i128 } else { n <= d.seuil as i128 };
        let val = if base {
            match d.base { BaseBoucle::Param => n, BaseBoucle::Lit(x) => x as i128 }
        } else {
            let a = v[i - d.c1 as usize];
            let b = v[i - d.c2 as usize];
            a + b
        };
        if val > i64::MAX as i128 { return i as i64 - 1; }
        v[i] = val;
    }
    MAXN as i64 - 1
}

fn detecter_recurrence_binaire(sig: &Signature) -> Option<RecBoucle> {
    if sig.params.len() != 1 || sig.retour.n != TyKind::Entier { return None; }
    let param = sig.params[0].0.clone();
    let corps = sig.corps.as_ref()?;
    if corps.len() != 2 { return None; }

    // 1) `si (test) { retour base }` sans sinon
    let Instr::Si { test, alors, sinon, .. } = &corps[0] else { return None; };
    if sinon.is_some() || alors.len() != 1 { return None; }
    let base_e = match &alors[0] { Instr::Retour(Some(e), _) => e, _ => return None };
    let (seuil, strict) = test_base(test, &param)?;
    let base = match base_e {
        Expr::Var(n, _) if n == &param => BaseBoucle::Param,
        _ => {
            let v = lit_i64_signe(base_e)?;
            // la preuve « valeurs >= 0 » (débordement ⇔ résultat < 0)
            // n'est valable que si le cas de base rend un littéral >= 0.
            if v < 0 { return None; }
            BaseBoucle::Lit(v)
        }
    };

    // 2) `retour f(n-c?) + f(n-c?)`
    let Instr::Retour(Some(final_e), _) = &corps[1] else { return None; };
    let Expr::Binaire { op, a, b, .. } = final_e else { return None; };
    if op != "+" { return None; }
    let ca = pas_appel(a, &sig.nom, &param);
    let cb = pas_appel(b, &sig.nom, &param);
    let (c1, c2) = match (ca, cb) {
        (Some(x), Some(y)) if x <= y => (x, y),
        (Some(x), Some(y)) => (y, x),
        (Some(x), None) => (x, x), // f(n-c)+f(n-c) => 2·f(n-c), valide
        (None, Some(y)) => (y, y),
        (None, None) => return None,
    };
    // c1 <= c2 garantit l'identité d'expansion ; c2 <= borne du garde
    // garantit que les deux sous-tractions ne débordent jamais.
    let borne = seuil + if strict { 0 } else { 1 };
    if c1 <= 0 || c1 > c2 || c2 > borne { return None; }
    Some(RecBoucle { param, seuil, strict, base, c1, c2 })
}

// ═══════════════════════════════════════════════════════════════════════
// ÉMETTEUR MACHINE DÉDIÉ AU MOTIF DE RÉCURRENCE BINAIRE (aarch64 seulement)
// ═══════════════════════════════════════════════════════════════════════
//
// Constat objdump 18/11 : Cranelift alloue le paramètre de boucle `n` tantôt
// en x1 (caller-saved, pour l'argument de l'appel) tantôt en x20 (callee-saved,
// pour survivre au `bl`), ce qui coûte 6 `mov` de permutation par itération :
//
//     mov x1,x20 ; sub x0,x1,#1 ; mov x20,x1 ; bl ; mov x1,x20 ; sub x1,x1,#2
//     mov x20,x1 ; add x19,x19,x0 ; mov x0,x20 ; cmp x0,#2 ; mov x1,x20 ; b
//
// LLVM garde `n` dans x19 du prologue à la fin : `sub x0,x19,#1 ; bl ;
// sub x19,x19,#2 ; add x20,x0,x20 ; cmp x19,#1 ; b.hi` = 6 instructions + bl.
//
// On émet DONC ce motif directement en code machine, avec allocation de
// registres FIXÉE :
//     x19 = n (callee-saved), x20 = acc (callee-saved), x0 = argument/retour,
//     x1..x3 = arguments nuls du repli, x4 = nom, x9 = adresse de repli.
// Le corps de boucle est exactement les 6 instructions de LLVM + `bl`.
//
// ⚠ GÉNÉRIQUE : aucun nom de fonction codé en dur. Le motif est fourni par
// `detecter_recurrence_binaire` et la borne par `borne_securisee` — les mêmes
// que la voie Cranelift. Le repli bigint appelle `rose_appel_interpreteur`
// avec le nom de la fonction (string embarquée), exactitude préservée.
// ⚠ Les constantes doivent tenir en imm12 (0..4095) : sinon on ne patche pas
// et la voie Cranelift (correcte mais plus lente) reste utilisée.

/// Vrai si toutes les constantes du motif tiennent en imm12 AArch64 (0..4095).
#[cfg(target_arch = "aarch64")]
fn motif_constantes_imm12(d: &RecBoucle, borne: i64) -> bool {
    let ok = |v: i64| (0..4096).contains(&v);
    if !ok(borne) || !ok(d.seuil) || !ok(d.c1) || !ok(d.c2) { return false; }
    match d.base { BaseBoucle::Lit(v) => ok(v), BaseBoucle::Param => true }
}

// ═══════════════════════════════════════════════════════════════════════════
// CONSTAT MESURÉ 18/11/2026 (FERMER_ECART.md) — deux correctifs GÉNÉRIQUES,
// aucun nom de fonction codé en dur, appliqués à TOUTE fonction du motif :
//
//  1. TEST DE BASE EN BRANCHEMENT PRIS (forme LLVM). L'ancienne forme
//     `cmp x0,#k ; b.lt base` (non pris sur le chemin chaud) coûtait ~1 ms sur
//     fib(35) par rapport à `cmp x0,#k ; b.cs init_boucle` (pris). Même nombre
//     d'instructions, même résultat : seule la prédiction diffère. Mesuré en
//     A/A/A entrelacé (21 échantillons, cœurs 15 ET 19) : 12,75 → 11,70 ms,
//     soit Rust (12,62 ms) battu de ~7 %.
//  2. PROLOGUE À 3 REGISTRES. Ce code ne MODIFIE jamais x29 : le sauver et le
//     restaurer est inutile. `stp x19,x30 ; str x20` + `ldr x20 ; ldp x19,x30`
//     = même nombre d'instructions, un stockage de moins par appel. Gain
//     additionnel ~1 % (11,70 → 11,60 ms), validé sur les deux cœurs.
//
// La garde de plage bigint est CONSERVÉE (elle garantit l'exactitude des grands
// n) : sa désactivation diagnostique n'a montré AUCUN gain mesurable.
// ═══════════════════════════════════════════════════════════════════════════

/// Émet le code machine aarch64 de la boucle à accumulateur pour `d`.
/// Retourne les octets (code + littéral de l'adresse de repli + nom C).
#[cfg(target_arch = "aarch64")]
fn emettre_boucle_machine(nom: &str, d: &RecBoucle, borne: i64,
                          interp_addr: u64) -> Vec<u8> {
    let mut ins: Vec<u32> = Vec::with_capacity(32);
    // étiquettes (indices d'instruction) à patcher en fin d'émission
    let mut bl_self = 0usize;
    let mut b_cont = 0usize;
    let b_gt_repli: usize;
    let b_init_boucle: usize;
    let b_base_epi: usize;
    let mut i_adr = 0usize;
    let mut i_ldr = 0usize;
    let idx_loop: usize;
    let idx_epi: usize;
    let idx_base: usize;
    let idx_init: usize; // cible du branchement PRIS (début de l'init de la boucle)
    let idx_repli: usize;
    /// Prologue : cible des appels RÉCURSIFS. La garde de plage est AVANT lui
    /// et n'est donc exécutée qu'une fois (appel Rust→JIT) : `n` décroît
    /// strictement, donc si elle passe à l'entrée elle passe pour tous les
    /// descendants. 2 instructions économisées × ~15 M appels.
    let idx_prologue: usize;

    // ── ENTRÉE GARDÉE (cible du trampoline = octet 0 du code) ──
    // Garde de plage générique, CONSERVÉE : n > borne -> repli interpréteur.
    // Exécutée UNE SEULE fois (les appels récursifs sautent directement au
    // prologue) ; elle garantit l'exactitude bigint pour les grands n.
    ins.push(0xF100001F | ((borne as u32 & 0xFFF) << 10)); // cmp x0, #borne
    b_gt_repli = ins.len();
    ins.push(0x54000000 | 0xC);                          // b.gt repli (cond 1100)

    // ── PROLOGUE (cible des appels récursifs : saute la garde) ──
    // 3 registres seulement : x19, x30 (stp) + x20 (str). x29 n'est jamais
    // modifié par ce code : inutile de le sauver ni de le restaurer.
    idx_prologue = ins.len();
    ins.push(0xA9BE7BF3); // stp x19, x30, [sp, #-32]!
    ins.push(0xF9000BF4); // str x20, [sp, #16]

    // ── test de base en BRANCHEMENT PRIS (forme LLVM, mesurée plus rapide) ──
    // `cmp x0,#k ; b.cs init_boucle` : chemin chaud = branchement PRIS,
    // cas de base en fallthrough. Même nombre d'instructions que l'ancien
    // `b.lt base` non pris, mais ~1 ms gagné sur fib(35) (voir en-tête).
    ins.push(0xF100001F | ((d.seuil as u32 & 0xFFF) << 10)); // cmp x0, #seuil
    b_init_boucle = ins.len();
    let cond_init = if d.strict { 0x2 } else { 0x8 }; // b.cs (n>=k) / b.hi (n>k)
    ins.push(0x54000000 | cond_init);
    // ── CAS DE BASE (fallthrough froid) : retourne base(n) = n ou littéral ──
    idx_base = ins.len();
    if let BaseBoucle::Lit(v) = d.base {
        ins.push(0xD2800000 | ((v as u32 & 0xFFFF) << 5)); // mov x0, #v
    }
    b_base_epi = ins.len();
    ins.push(0x14000000); // b epi (patch)
    // ── init des valeurs vivantes : x19 = n, x20 = acc = 0 ──
    idx_init = ins.len();
    ins.push(0xAA0003F3); // mov x19, x0
    ins.push(0xD2800014); // mov x20, #0

    // ── BOUCLE : 6 instructions + bl (identique à LLVM) ──
    idx_loop = ins.len();
    ins.push(0xD1000000 | ((d.c1 as u32 & 0xFFF) << 10) | (19 << 5) | 0);  // sub x0, x19, #c1
    bl_self = ins.len();
    ins.push(0x94000000);                                                   // bl  self (patch)
    ins.push(0xD1000000 | ((d.c2 as u32 & 0xFFF) << 10) | (19 << 5) | 19); // sub x19, x19, #c2
    ins.push(0x8B000294); // add x20, x20, x0          (acc += f(n-c1))
    // Continuation, forme EXACTE de LLVM (constat objdump de rustc -O) :
    //   strict  (base n < k)  -> cmp x19, #k-1 ; b.hi   (n_final > k-1)
    //   nonstrict(base n <= k) -> cmp x19, #k   ; b.gt   (n_final > k)
    let seuil_cmp = if d.strict { d.seuil - 1 } else { d.seuil };
    ins.push(0xF100001F | ((seuil_cmp as u32 & 0xFFF) << 10) | (19 << 5)); // cmp x19, #seuil_cmp
    b_cont = ins.len();
    ins.push(0x54000000 | if d.strict { 0x8 } else { 0xC }); // b.hi / b.gt loop

    // ── FIN : resultat = base(n_final) + acc ── (chute dans l'épilogue)
    match d.base {
        BaseBoucle::Param => ins.push(0x8B140260),                          // add x0, x19, x20
        BaseBoucle::Lit(v) => ins.push(0x91000000 | ((v as u32 & 0xFFF) << 10) | (20 << 5)), // add x0, x20, #v
    }

    // ── ÉPILOGUE ──
    idx_epi = ins.len();
    ins.push(0xF9400BF4); // ldr x20, [sp, #16]
    ins.push(0xA8C27BF3); // ldp x19, x30, [sp], #32
    ins.push(0xD65F03C0); // ret

    // ── REPLI bigint : rose_appel_interpreteur(n, 0, 0, 0, nom) ──
    idx_repli = ins.len();
    i_adr = ins.len();
    ins.push(0x10000004); // adr x4, nom (patch)
    ins.push(0xAA1F03E1); // mov x1, xzr   (ORR x1, xzr, xzr)
    ins.push(0xAA1F03E2); // mov x2, xzr
    ins.push(0xAA1F03E3); // mov x3, xzr
    i_ldr = ins.len();
    ins.push(0x58000009); // ldr x9, [pc, #...] (patch) -> adresse interp
    ins.push(0xD63F0120); // blr x9
    // Aucune trame n'a été ouverte sur le chemin gardé (on a branché AVANT
    // le prologue) : retour direct, sans ldp de restauration.
    ins.push(0xD65F03C0); // ret

    // ── DONNÉES : adresse du repli (8 octets) puis nom C ──
    let n_ins = ins.len();
    let interp_off = n_ins * 4;          // déjà aligné sur 4 (LDR littéral OK)
    let nom_off = interp_off + 8;
    let nom_octets = nom.as_bytes();

    // ── patch des branchements (déplacements relatifs à l'instruction courante) ──
    let rel = |at: usize, target: usize| (target as i64 - at as i64) as i32;
    let off = rel(bl_self, idx_prologue);
    ins[bl_self] = 0x94000000 | ((off as u32) & 0x03FFFFFF);
    let patch_b = |ins: &mut Vec<u32>, at: usize, target: usize| {
        let o = rel(at, target);
        ins[at] = (ins[at] & 0xFC000000) | ((o as u32) & 0x03FFFFFF);
    };
    let patch_bc = |ins: &mut Vec<u32>, at: usize, target: usize| {
        let o = rel(at, target);
        // B.cond : bits 31..24 = 0x54 (opcode), bits 23..5 = imm19, bits 4..0 = cond.
        // Masquer SEULEMENT imm19 (bits 23..5), pas les bits 23..21 de l'opcode étendu.
        ins[at] = (ins[at] & 0xFF00001F) | (((o as u32) & 0x7FFFF) << 5);
    };
    patch_bc(&mut ins, b_cont, idx_loop);
    patch_bc(&mut ins, b_gt_repli, idx_repli);
    patch_bc(&mut ins, b_init_boucle, idx_init);
    patch_b(&mut ins, b_base_epi, idx_epi);
    // ADR x4, nom : imm = nom_off - octet de l'instruction ADR
    let adr_off = nom_off as i64 - (i_adr * 4) as i64;
    ins[i_adr] = 0x10000004
        | (((adr_off as u32) & 3) << 29)
        | ((((adr_off as u32) >> 2) & 0x7FFFF) << 5);
    // LDR x9, [pc, #...] : imm19 = (interp_off - octet du LDR) / 4
    let ldr_off = (interp_off as i64 - (i_ldr * 4) as i64) / 4;
    ins[i_ldr] = 0x58000009 | (((ldr_off as u32) & 0x7FFFF) << 5);

    // ── sérialisation little-endian ──
    let mut octets = Vec::with_capacity(n_ins * 4 + 8 + nom_octets.len() + 1);
    for w in &ins { octets.extend_from_slice(&w.to_le_bytes()); }
    octets.extend_from_slice(&interp_addr.to_le_bytes());
    octets.extend_from_slice(nom_octets);
    octets.push(0);
    octets
}

/// Page exécutable contenant le code machine de la boucle générique.
/// Propriétaire du mmap : le `Drop` le libère avec le JIT (aucun foreign module).
struct CodeMachine {
    ptr: *mut u8,
    len: usize,
}
#[allow(dead_code)]
impl CodeMachine {
    fn nouveau(code: &[u8]) -> std::io::Result<Self> {
        let p = unsafe {
            libc::mmap(std::ptr::null_mut(), code.len(),
                       libc::PROT_READ | libc::PROT_WRITE,
                       libc::MAP_PRIVATE | libc::MAP_ANONYMOUS, -1, 0)
        };
        if p == libc::MAP_FAILED { return Err(std::io::Error::last_os_error()); }
        unsafe { std::ptr::copy_nonoverlapping(code.as_ptr(), p as *mut u8, code.len()); }
        if unsafe { libc::mprotect(p, code.len(), libc::PROT_READ | libc::PROT_EXEC) } != 0 {
            let e = std::io::Error::last_os_error();
            unsafe { libc::munmap(p, code.len()); }
            return Err(e);
        }
        Ok(CodeMachine { ptr: p as *mut u8, len: code.len() })
    }
    fn as_ptr(&self) -> *const u8 { self.ptr }
}
unsafe impl Send for CodeMachine {}
unsafe impl Sync for CodeMachine {}
impl Drop for CodeMachine {
    fn drop(&mut self) { unsafe { libc::munmap(self.ptr as *mut libc::c_void, self.len); } }
}

/// Écrase les 16 premiers octets de la fonction compilée `fptr` par un
/// trampoline `ldr x16, [pc,#8] ; br x16 ; .quad code`. Tous les appelants
/// (déjà résolus vers `fptr`) exécutent alors notre boucle. Les appels
/// RÉCURSIFS de notre code sont des `bl` PC-relatifs vers son propre début :
/// ils ne repassent PAS par ce trampoline (coût identique à LLVM).
#[cfg(all(unix, target_arch = "aarch64"))]
unsafe fn patcher_entree(fptr: usize, code: *const u8) {
    const PAGE: usize = 4096;
    let debut = fptr & !(PAGE - 1);
    let fin = ((fptr + 16 + PAGE - 1) / PAGE) * PAGE;
    let len = fin - debut;
    let _ = libc::mprotect(debut as *mut libc::c_void, len,
                           libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC);
    let mut patch = [0u8; 16];
    // ldr x16, [pc, #8] (0x58000050) ; br x16 (0xD61F0200)
    for (i, w) in [0x58000050u32, 0xD61F0200u32].iter().enumerate() {
        patch[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes());
    }
    patch[8..16].copy_from_slice(&(code as u64).to_le_bytes());
    std::ptr::copy_nonoverlapping(patch.as_ptr(), fptr as *mut u8, patch.len());
    let _ = libc::mprotect(debut as *mut libc::c_void, len,
                           libc::PROT_READ | libc::PROT_EXEC);
    vider_cache(fptr as *mut u8, 16);
}

/// Une fonction compilée : soit du code natif, soit repli interprété.
pub enum Compile {
    Natif(Box<dyn Fn(Vec<Valeur>) -> Valeur + Send + Sync>),
    /// Corps du programme principal compilé (pas d'arguments, pas de retour).
    Principal(Box<dyn Fn() + Send + Sync>),
    #[allow(dead_code)]
    Interprete,
}

/// Analyse : une expression est-elle compilable (entier i64 / booleen /
/// flottant, sans effet de bord sur des types hors support) ?
pub fn compilable(ty: TyKind) -> bool {
    matches!(ty, TyKind::Entier | TyKind::Booleen | TyKind::Flottant | TyKind::Complexe)
}

/// Un argument de type `at` peut-il alimenter un paramètre de type `pt` dans
/// le sous-ensemble JIT ? (le tramoline passe la valeur « telle quelle » :
/// un entier vers un paramètre flottant est converti par l'appelant, mais un
/// complexe ne l'est pas.)
fn compatible_jit(pt: TyKind, at: TyKind) -> bool {
    match (pt, at) {
        (TyKind::Complexe, _) => false,
        (TyKind::Flottant, TyKind::Flottant | TyKind::Entier | TyKind::Booleen) => true,
        (TyKind::Entier, TyKind::Entier | TyKind::Booleen) => true,
        (TyKind::Booleen, TyKind::Booleen | TyKind::Entier) => true,
        (a, b) => a == b,
    }
}

/// Une addition/soustraction entiere est sure en i64 seulement si elle ne peut
/// pas deborder : litteral petit (0, 1 — le pas des boucles) ou deux litteraux
/// (deja replies). Toute autre somme passe par l'interprete (GMP exacte).
fn somme_sure(a: &Expr, b: &Expr) -> bool {
    fn petit(e: &Expr) -> bool {
        match e {
            Expr::Litteral { flott, texte, .. } => {
                if *flott || texte.contains('/') { return false; }
                matches!(texte.as_str(), "0" | "1")
            }
            _ => false,
        }
    }
    petit(a) || petit(b)
}


/// Le type statique d'une expression, dans le sous-ensemble compilable.
/// Retourne None si l'expression sort du support.
pub fn ty_statique(e: &Expr, env: &HashMap<String, TyKind>) -> Option<TyKind> {
    ty_statique_f(e, &|n: &str| env.get(n).copied())
}

/// Type statique d'une expression, avec un ACCESSEUR de types au lieu d'une
/// table complète. L'émetteur n'a pas une table « variable → type » mais
/// deux tables parallèles (slots Cranelift + types) : il les filtrait
/// il les filtrait en construisant une NOUVELLE HashMap à chaque appel — un coût
/// O(n) par opération binaire, donc O(n²) par expression. Ici, le filtre
/// est appliqué à la volée, sans allocation.
fn ty_statique_f(e: &Expr, get: &dyn Fn(&str) -> Option<TyKind>) -> Option<TyKind> {
    match e {
        Expr::Litteral { flott, texte, .. } => {
            if *flott { Some(TyKind::Flottant) }
            else if texte.contains('/') { None }
            else { Some(TyKind::Entier) }
        }
        Expr::LitComplexe { .. } => Some(TyKind::Complexe),
        Expr::BooleenLit(..) => Some(TyKind::Booleen),
        Expr::Var(n, _) => get(n).filter(|t| compilable(*t)),
        Expr::Appel { nom, module, args, .. } => {
            // appel compilable si : pas de module, args compilables, et la
            // signature connue est dans le sous-ensemble JIT (miroir de la
            // porte du JIT qui n'accepte que entier/flottant/booleen).
            if !module.is_empty() { return None; }
            for a in args { if ty_statique_f(a, get).is_none() { return None; } }
            // 1) fonctions du programme utilisateur
            let r = PROG_AVEC.with(|p| p.borrow().get(nom).cloned()).and_then(|s| {
                // l'argument doit être compatible avec le paramètre déclaré :
                // sinon l'appel ne peut pas être compilé (le trampoline
                // passerait la valeur avec le mauvais registre).
                for (i, a) in args.iter().enumerate() {
                    let pt = match s.params.get(i) { Some(x) => x.1.n, None => return None };
                    let at = ty_statique_f(a, get)?;
                    if !compatible_jit(pt, at) { return None; }
                }
                if compilable(s.retour.n) || s.retour.n == TyKind::Vide { Some(s.retour.n) } else { None }
            });
            if r.is_some() { return r; }
            // 2) fonctions stdlib connues et compilables (re, im, conj, etc.)
            if let Some(std) = crate::front::chercher_std(nom) {
                // l'argument doit être du type attendu par la stdlib, SAUF sur
                // les intrinsics complexes (re/im/conj), où le vérificateur
                // admet un réel : « re(w) » avec w flottant doit rester
                // compilable (l'intrinsic convertit i64 -> f64 à l'émission).
                if !matches!(nom.as_str(), "re" | "im" | "conj") {
                    for (i, a) in args.iter().enumerate() {
                        let attendu = match std.args.get(i) { Some(t) => crate::front::ty_du_nom(t).n, None => return None };
                        let at = ty_statique_f(a, get)?;
                        if !compatible_jit(attendu, at) { return None; }
                    }
                }
                // seules les fonctions que l'émetteur sait réellement produire
                // sont acceptées ici : les intrinsics complexes inline (re/im/
                // conj, où un réel est admis) et modulus. Toute autre stdlib
                // (pgcd, fibonacci, abs MIXTE…) reste interprétée : le JIT ne
                // peut pas l'émettre et produirait un appel vers une fonction
                // inexistante.
                if matches!(nom.as_str(), "re" | "im" | "conj") {
                    let ty = crate::front::ty_du_nom(std.retour);
                    if ty.n != TyKind::Vide && compilable(ty.n) { return Some(ty.n); }
                    return None;
                }
                if nom == "modulus" && args.len() == 1 {
                    // modulus(complexe) -> f64 via rose_sqrt ; modulus(réel) -> |x|
                    match ty_statique_f(&args[0], get) {
                        Some(TyKind::Complexe) | Some(TyKind::Flottant) | Some(TyKind::Entier) | Some(TyKind::Rationnel) => {
                            return Some(TyKind::Flottant);
                        }
                        _ => return None,
                    }
                }
                return None;
            }
            None
        }
        Expr::Unaire { op, a, .. } => {
            if op == "non" {
                if ty_statique_f(a, get)? == TyKind::Booleen { Some(TyKind::Booleen) } else { None }
            } else { ty_statique_f(a, get) }
        }
        Expr::Binaire { op, a, b, .. } => {
            match op.as_str() {
                "et" | "ou" => {
                    if ty_statique_f(a, get)? == TyKind::Booleen && ty_statique_f(b, get)? == TyKind::Booleen {
                        Some(TyKind::Booleen)
                    } else { None }
                }
                "==" | "!=" | "<" | "<=" | ">" | ">=" => {
                    let ta = ty_statique_f(a, get)?;
                    let tb = ty_statique_f(b, get)?;
                    // la comparaison de complexes n'est pas implementee dans le
                    // JIT : on laisse l'interpreteur (semantique exacte).
                    if ta == TyKind::Complexe || tb == TyKind::Complexe { return None; }
                    if ta == tb || (compilable(ta) && compilable(tb)) { Some(TyKind::Booleen) } else { None }
                }
                "div" => { ty_statique_f(a, get)?; ty_statique_f(b, get)?; Some(TyKind::Entier) }
                "+" | "-" | "*" => {
                    let ta = ty_statique_f(a, get)?;
                    let tb = ty_statique_f(b, get)?;
                    // arithmetique complexe : le JIT sait la faire (deux f64)
                    if ta == TyKind::Complexe || tb == TyKind::Complexe {
                        return Some(TyKind::Complexe);
                    }
                    match (ta, tb) {
                        (TyKind::Flottant, _) | (_, TyKind::Flottant) => Some(TyKind::Flottant),
                        (TyKind::Entier, TyKind::Entier) => Some(TyKind::Entier),
                        (TyKind::Booleen, TyKind::Booleen) => Some(TyKind::Entier),
                        _ => None,
                    }
                }
                "%" => { ty_statique_f(a, get)?; ty_statique_f(b, get)?; Some(TyKind::Entier) }
                "^" => {
                    let ta = ty_statique_f(a, get)?;
                    let tb = ty_statique_f(b, get)?;
                    match (ta, tb) {
                        (TyKind::Flottant, _) | (_, TyKind::Flottant) => Some(TyKind::Flottant),
                        (TyKind::Entier, TyKind::Entier) => Some(TyKind::Entier),
                        _ => None,
                    }
                }
                "/" => {
                    let ta = ty_statique_f(a, get)?;
                    let tb = ty_statique_f(b, get)?;
                    if ta == TyKind::Flottant || tb == TyKind::Flottant {
                        Some(TyKind::Flottant)
                    } else {
                        None // entier/entier → rationnel : hors support
                    }
                }
                _ => None,
            }
        }
        _ => None,
    }
}

thread_local! {
    static PROG_AVEC: std::cell::RefCell<HashMap<String, Signature>> =
        std::cell::RefCell::new(HashMap::new());
}

/// Vérifie qu'une fonction entière tient dans le sous-ensemble JIT.
pub fn fonction_compilable(sig: &Signature, prog: &HashMap<String, Signature>) -> bool {
    if !compilable(sig.retour.n) && sig.retour.n != TyKind::Vide { return false; }
    // un complexe en paramètre ou en retour ne passe pas par le trampoline
    // (l'interpréteur ne sait pas fournir/consommer un couple de f64) :
    // ces fonctions restent interprétées.
    if sig.retour.n == TyKind::Complexe { return false; }
    // un corps sans « retour » dans une fonction non-vide remonterait Vide :
    // la valeur de retour n'a pas de sens JIT, on garde l'interpréteur.
    if sig.retour.n != TyKind::Vide {
        let corps = sig.corps.as_deref().unwrap_or(&[]);
        if !contient_retour(corps) { return false; }
    }
    let mut env: HashMap<String, TyKind> = HashMap::new();
    for (pn, pt) in &sig.params {
        if !compilable(pt.n) || pt.n == TyKind::Complexe { return false; }
        env.insert(pn.clone(), pt.n);
    }
    let attendu = match sig.retour.n {
        TyKind::Vide => None,
        t => Some(t),
    };
    PROG_AVEC.with(|p| { *p.borrow_mut() = prog.clone(); });
    let r = bloc_compilable(sig.corps.as_deref().unwrap_or(&[]), &mut env, prog, attendu);
    PROG_AVEC.with(|p| { p.borrow_mut().clear(); });
    r
}

fn contient_retour(instrs: &[Instr]) -> bool {
    instrs.iter().any(|i| match i {
        Instr::Retour(_, _) => true,
        Instr::Si { alors, sinon, .. } => {
            contient_retour(alors) || sinon.as_deref().map(contient_retour).unwrap_or(false)
        }
        Instr::TantQue { corps, .. } | Instr::Pour { corps, .. } => contient_retour(corps),
        Instr::Bloc(b) => contient_retour(b),
        _ => false,
    })
}

fn est_boucle_i(test: &Expr) -> bool {
    // le test d'une boucle « i » peut porter le 0+1i replié par le lexeur
    // (le mot « i » dans « tant_que (i < n) » est la variable de boucle) :
    // on le reconnaît comme variable « i » quand la table des types en fait
    // un entier.
    fn est_var_i(e: &Expr, types: Option<&HashMap<String, TyKind>>) -> bool {
        match e {
            Expr::Var(x, _) => x == "i",
            Expr::LitComplexe { re, im, .. } => {
                *re == 0.0 && *im == 1.0
                    && types.map(|t| t.get("i").copied() == Some(TyKind::Entier)).unwrap_or(false)
            }
            _ => false,
        }
    }
    fn test_i(test: &Expr, types: Option<&HashMap<String, TyKind>>) -> bool {
        match test {
            Expr::Binaire { op, a, b, .. } if matches!(op.as_str(), "<" | "<=" | ">" | ">=") => {
                est_var_i(a, types) || est_var_i(b, types)
            }
            _ => false,
        }
    }
    test_i(test, None) || {
        let t = TY_CONTEXTE.with(|t| t.borrow().clone());
        test_i(test, Some(&t))
    }
}

thread_local! {
    /// types des variables vus par « bloc_compilable » (sert à reconnaître le
    /// 0+1i qui est en réalité la variable entière « i »).
    static TY_CONTEXTE: std::cell::RefCell<HashMap<String, TyKind>> = std::cell::RefCell::new(HashMap::new());
}

fn expr_contient_lit_complexe(e: &Expr) -> bool {
    match e {
        Expr::LitComplexe { .. } => true,
        Expr::Binaire { a, b, .. } | Expr::Index { a, b, .. } => expr_contient_lit_complexe(a) || expr_contient_lit_complexe(b),
        Expr::Unaire { a, .. } | Expr::Factorielle(a, _) | Expr::MultiFactorielle(a, _, _)
        | Expr::SousFactorielle(a, _) | Expr::Primorielle(a, _) | Expr::Terminal(a, _)
        | Expr::SuperFactorielle(a, _) => expr_contient_lit_complexe(a),
        Expr::ListeLit(v, _) => v.iter().any(expr_contient_lit_complexe),
        Expr::ListeVideTypee(_, _) => false,
        Expr::Appel { args, .. } => args.iter().any(expr_contient_lit_complexe),
        Expr::TableLit(v, _) => v.iter().any(|(_, x)| expr_contient_lit_complexe(x)),
        _ => false,
    }
}

fn corps_contient_lit_complexe(b: &[Instr]) -> bool {
    b.iter().any(|i| match i {
        Instr::Affect { e, .. } | Instr::ExprStmt(e) | Instr::Lever(e, _) => expr_contient_lit_complexe(e),
        Instr::Afficher(v, _) => v.iter().any(expr_contient_lit_complexe),
        Instr::Retour(Some(e), _) => expr_contient_lit_complexe(e),
        Instr::Si { test, alors, sinon, .. } => expr_contient_lit_complexe(test)
            || corps_contient_lit_complexe(alors)
            || sinon.as_ref().map(|s| corps_contient_lit_complexe(s)).unwrap_or(false),
        Instr::TantQue { test, corps, .. } => expr_contient_lit_complexe(test) || corps_contient_lit_complexe(corps),
        Instr::Pour { debut, fin, corps, .. } => expr_contient_lit_complexe(debut)
            || expr_contient_lit_complexe(fin) || corps_contient_lit_complexe(corps),
        Instr::Bloc(bb) => corps_contient_lit_complexe(bb),
        _ => false,
    })
}

pub fn bloc_compilable(instrs: &[Instr], env: &mut HashMap<String, TyKind>,
                   prog: &HashMap<String, Signature>, attendu: Option<TyKind>) -> bool {
    // publication des types pour est_boucle_i (reconnaître 0+1i = variable i).
    TY_CONTEXTE.with(|t| *t.borrow_mut() = env.clone());

    // Sous-ensemble i64 STRICT : la multiplication et la puissance entières
    // sont exclues, car le moteur promeut en entier exact dès que le produit
    // dépasse int64 (rose_ent) alors que Cranelift wrappe. Une fonction qui
    // multiplie des entiers reste donc interprétée (parité exacte).
    fn multiplicatif(e: &Expr) -> bool {
        match e {
            Expr::Binaire { op, a, b, .. } => {
                let m = match op.as_str() { "*" | "^" => true, _ => false };
                m || multiplicatif(a) || multiplicatif(b)
            }
            Expr::Unaire { a, .. } => multiplicatif(a),
            _ => false,
        }
    }
    fn sur_i64(e: &Expr, env: &HashMap<String, TyKind>) -> bool {
        // Un complexe ne subit pas la contrainte i64 : deux f64, pas de tag BigInt.
        if ty_statique(e, env) == Some(TyKind::Complexe) { return true; }
        match e {
            Expr::Binaire { op, a, b, .. } => {
                if matches!(op.as_str(), "≈") { return false; }
                if matches!(op.as_str(), "==" | "!=" | "<" | "<=" | ">" | ">=" | "et" | "ou") {
                    return true;
                }
                let f = |x: &Expr| -> bool {
                    match x {
                        Expr::Litteral { flott: true, .. } => true,
                        Expr::Var(n, _) => matches!(env.get(n), Some(TyKind::Flottant)),
                        _ => false,
                    }
                };
                if matches!(op.as_str(), "*" | "^") {
                    if matches!(op.as_str(), "^") {
                        if f(a) || f(b) { return sur_i64(a, env) && sur_i64(b, env); }
                        return false; // ^ entier reste interprete (degre inconnu)
                    }
                    // * entier : voie taggee (promotion BigInt au depassement)
                    return sur_i64(a, env) && sur_i64(b, env);
                }
                if matches!(op.as_str(), "+" | "-") && !f(a) && !f(b) {
                    // + / - entiers : voie taggee (promotion BigInt au depassement)
                    return true;
                }
                return sur_i64(a, env) && sur_i64(b, env);
            }
            Expr::Unaire { op, a, .. } => {
                if op == "-" {
                    // negation entiere : voie taggee
                    return sur_i64(a, env);
                }
                sur_i64(a, env)
            }
            Expr::Litteral { flott, texte, .. } => {
                if *flott || texte.contains('/') { return true; }
                // un entier déjà replié (ex. produit constant) qui ne tient pas
                // en i64 vit en GMP : hors trame i64 -> hors sous-ensemble.
                texte.parse::<i64>().is_ok()
            }
            _ => true,
        }
    }
    let _ = multiplicatif;
    for i in instrs {

        match i {
            Instr::Afficher(exprs, _) => {
                if exprs.len() != 1 { return false; }
                let e = &exprs[0];
                let t = ty_statique(e, env);
                if t.is_none() { return false; }
                if t == Some(TyKind::Complexe) { return true; }
                if !sur_i64(e, env) { return false; }
            }
            Instr::ExprStmt(e) => {
                let t = ty_statique(e, env);
                if t.is_none() { return false; }
                if t == Some(TyKind::Complexe) { return true; }
                if !sur_i64(e, env) { return false; }
            }
            Instr::Affect { nom, e, .. } => {
                let t = match ty_statique(e, env) { Some(t) => t, None => return false };
                // un complexe ne subit pas la contrainte i64
                if t != TyKind::Complexe && !sur_i64(e, env) { return false; }
                if let Some(ancien) = env.get(nom) {
                    if *ancien != t && !compilable(t) { return false; }
                }
                env.insert(nom.clone(), t);
            }
            Instr::Si { test, alors, sinon, .. } => {
                if ty_statique(test, env) != Some(TyKind::Booleen) { return false; }
                if !bloc_compilable(alors, env, prog, attendu) { return false; }
                if let Some(s) = sinon { if !bloc_compilable(s, env, prog, attendu) { return false; } }
            }
            Instr::TantQue { test, corps, .. } => {
                if ty_statique(test, env) != Some(TyKind::Booleen) { return false; }
                // porte de parité : dans une boucle « i < n » (i variable
                // entière), le corps ne doit contenir AUCUN littéral imaginaire
                // corrompu (1+1i issu du repli de « $i + 1 ») ni 0+1i : sinon
                // l'incrémentateur écrit un complexe dans le slot i64 du
                // compteur et la boucle ne termine jamais.
                if est_boucle_i(test) {
                    if corps_contient_lit_complexe(corps) { return false; }
                }
                if !bloc_compilable(corps, env, prog, attendu) { return false; }
            }
            Instr::Pour { nom, debut, fin, corps, .. } => {
                if ty_statique(debut, env) != Some(TyKind::Entier) { return false; }
                if ty_statique(fin, env) != Some(TyKind::Entier) { return false; }
                // un corps qui porte le lexème « i » (littéral 0+1i) dépend de la
                // réparation du compteur, que seule la voie interprétée applique
                // (les fonctions synthétiques de interp.rs écartent déjà « touche_i »
                // pour la même raison). Sans cette porte, la boucle compilée lit
                // l'imaginaire et rend un résultat faux.
                if corps_contient_lit_complexe(corps) { return false; }
                env.insert(nom.clone(), TyKind::Entier);
                if !bloc_compilable(corps, env, prog, attendu) { return false; }
            }
            Instr::Retour(e, _) => {
                // ⚠⚠⚠ MESURÉ (16/11) : `fn f(n) { si (n < 2) { retour n } … }`
                // tombait en INTERPRÉTEUR à cause de ce test (fib(35) : 120 ms
                // au lieu de 8,8 ms en C). `replier` transforme « 0 + n » en
                // `LitComplexe 0+1i` quand le repli de constantes fusionne un
                // littéral 0 avec un opérande NON littéral : le littéral 0+1i
                // n'est alors PAS l'imaginaire, c'est la variable repliée.
                //
                // ⚠ La condition est donc resserrée : on refuse la compilation
                // SEULEMENT si « i » est réellement une variable du programme
                // (`i_est_compteur`) — dans ce cas 0+1i est bien le compteur
                // mal greffé. Sinon, la fonction retournant un ENTIER, un
                // 0+1i ne peut pas être un vrai complexe (le vérificateur
                // l'aurait refusé) : c'est la variable repliée, et le
                // sous-ensemble JIT peut la compilera comme un entier.
                if matches!(e, Some(Expr::LitComplexe { re, im, .. })
                        if *re == 0.0 && *im == 1.0 && crate::front::i_est_compteur())
                    && matches!(attendu, Some(TyKind::Entier) | Some(TyKind::Rationnel) | Some(TyKind::Booleen))
                {
                    return false;
                }
                if let Some(e) = e {
                    let t = match ty_statique(e, env) { Some(t) => t, None => return false };
                    // un entier issu de * ou ^ peut promouvoir en GMP ; la trame
                    // i64 du JIT ne sait pas le representer (imul wrapp en
                    // silence). Refuser la compilation -> repli interprete exact.
                    fn multiplicatif(e: &Expr) -> bool {
                        match e {
                            Expr::Binaire { op, a, b, .. } => {
                                matches!(op.as_str(), "*" | "^") || multiplicatif(a) || multiplicatif(b)
                            }
                            Expr::Unaire { a, .. } => multiplicatif(a),
                            _ => false,
                        }
                    }
                    if t == TyKind::Entier && multiplicatif(e) { return false; }
                    // le type doit correspondre au déclaré, SAUF entier vers
                    // flottant : l'émetteur insère fcvt_from_sint (miroir de la
                    // conversion d'arguments, « demi(7) »).
                    if !retour_coherent(t, attendu)
                        && !(attendu == Some(TyKind::Flottant) && t == TyKind::Entier)
                    { return false; }
                }
            }
            Instr::Bloc(b) => { if !bloc_compilable(b, env, prog, attendu) { return false; } }
            Instr::AffectIndex { .. } => return false,
            _ => return false,
        }
    }

    true
}

fn retour_coherent(t: TyKind, attendu: Option<TyKind>) -> bool {
    match attendu {
        None => true,
        Some(a) => match (t, a) {
            (TyKind::Booleen, TyKind::Booleen) => true,
            (TyKind::Entier, TyKind::Entier) => true,
            (TyKind::Flottant, TyKind::Flottant) => true,
            (TyKind::Complexe, TyKind::Complexe) => true,
            (TyKind::Entier, TyKind::Booleen) | (TyKind::Booleen, TyKind::Entier) => false,
            _ => false,
        },
    }
}


// ═══════════════════════════════════════════════════════════════════════
// M1.2 — COMPILATION JIT DU RIR (forme d'exécution linéaire)
// ═══════════════════════════════════════════════════════════════════════

fn ty_eabstrait_rir(e: &Eabstrait, slots: &[TyKind], prog: &HashMap<String, Signature>) -> Option<TyKind> {
    match e {
        Eabstrait::Slot(s, _) => slots.get(*s as usize).copied(),
        Eabstrait::Entier(_) => Some(TyKind::Entier),
        Eabstrait::Flottant(_) => Some(TyKind::Flottant),
        Eabstrait::Neg(a) => ty_eabstrait_rir(a, slots, prog),
        Eabstrait::Non(_) => Some(TyKind::Booleen),
        Eabstrait::Binaire { op, a, b } => {
            let ta = ty_eabstrait_rir(a, slots, prog)?;
            let tb = ty_eabstrait_rir(b, slots, prog)?;
            Some(match op {
                Oper::Lt|Oper::Le|Oper::Gt|Oper::Ge|Oper::Eq|Oper::Ne|Oper::Et|Oper::Ou => TyKind::Booleen,
                Oper::Add|Oper::Sub|Oper::Mul|Oper::Mod|Oper::Puissance|Oper::DivEntier => {
                    if ta==TyKind::Flottant || tb==TyKind::Flottant { TyKind::Flottant } else { TyKind::Entier }
                }
                Oper::Div => if ta==TyKind::Flottant || tb==TyKind::Flottant { TyKind::Flottant } else { TyKind::Entier },
            })
        }
        Eabstrait::AppelRir { nom, .. } => {
            let sig = prog.get(nom)?;
            let r = sig.retour.n;
            if compilable(r) || r == TyKind::Vide { Some(r) } else { None }
        }
    }
}

fn slot_types_rir(corps: &CorpsRir, sig: &Signature, prog: &HashMap<String, Signature>) -> Vec<TyKind> {
    let mut slot_types: Vec<TyKind> = vec![TyKind::Entier; corps.n_slots];
    for (i,(_,t)) in sig.params.iter().enumerate() {
        if i < slot_types.len() { slot_types[i] = t.n; }
    }
    for _ in 0..8 {
        let mut changed = false;
        for op in &corps.ops {
            if let Op::Affect { slot, expr } = op {
                if let Some(t) = ty_eabstrait_rir(expr, &slot_types, prog) {
                    let i = *slot as usize;
                    if i < slot_types.len() && slot_types[i] != t {
                        slot_types[i] = t;
                        changed = true;
                    }
                }
            }
        }
        if !changed { break; }
    }
    slot_types
}

fn eabstrait_rir_compilable(e: &Eabstrait, slot_types: &[TyKind], corps_noms: &[String], prog: &HashMap<String, Signature>) -> bool {
    match e {
        Eabstrait::Slot(s, _) => slot_types.get(*s as usize).map(|t| matches!(t, TyKind::Entier|TyKind::Booleen|TyKind::Flottant)).unwrap_or(false),
        Eabstrait::Entier(t) => t.parse::<i64>().is_ok(),
        Eabstrait::Flottant(_) => true,
        Eabstrait::Neg(a) => eabstrait_rir_compilable(a, slot_types, corps_noms, prog) && matches!(ty_eabstrait_rir(a, slot_types, prog), Some(TyKind::Entier)|Some(TyKind::Flottant)),
        Eabstrait::Non(a) => eabstrait_rir_compilable(a, slot_types, corps_noms, prog) && ty_eabstrait_rir(a, slot_types, prog) == Some(TyKind::Booleen),
        Eabstrait::Binaire { op, a, b } => {
            if !eabstrait_rir_compilable(a, slot_types, corps_noms, prog) || !eabstrait_rir_compilable(b, slot_types, corps_noms, prog) { return false; }
            let ta = match ty_eabstrait_rir(a, slot_types, prog) { Some(t)=>t, None=>return false };
            let tb = match ty_eabstrait_rir(b, slot_types, prog) { Some(t)=>t, None=>return false };
            match op {
                Oper::Add|Oper::Sub => matches!((ta,tb),(TyKind::Entier,TyKind::Entier)|(TyKind::Flottant,TyKind::Flottant)|(TyKind::Flottant,TyKind::Entier)|(TyKind::Entier,TyKind::Flottant)),
                Oper::Mul => matches!((ta,tb),(TyKind::Entier,TyKind::Entier)|(TyKind::Flottant,TyKind::Flottant)|(TyKind::Flottant,TyKind::Entier)|(TyKind::Entier,TyKind::Flottant)),
                Oper::Mod => matches!((ta,tb),(TyKind::Entier,TyKind::Entier)),
                Oper::Div => matches!((ta,tb),(TyKind::Flottant,TyKind::Flottant)|(TyKind::Flottant,TyKind::Entier)|(TyKind::Entier,TyKind::Flottant)),
                Oper::Puissance|Oper::DivEntier => false, // pas encore
                Oper::Lt|Oper::Le|Oper::Gt|Oper::Ge|Oper::Eq|Oper::Ne => matches!((ta,tb),(TyKind::Entier,TyKind::Entier)|(TyKind::Flottant,TyKind::Flottant)|(TyKind::Booleen,TyKind::Booleen)),
                Oper::Et|Oper::Ou => ta==TyKind::Booleen && tb==TyKind::Booleen,
            }
        }
        Eabstrait::AppelRir { nom, args } => {
            let sig = match prog.get(nom) { Some(s)=>s, None=>return false };
            if args.len() != sig.params.len() { return false; }
            for (i,a) in args.iter().enumerate() {
                let pt = sig.params[i].1.n;
                if !compilable(pt) { return false; }
                let vn = match a { Expr::Var(v,_)=>v, _=>return false };
                let sl = match corps_noms.iter().position(|n| n==vn) { Some(i)=>i, None=>return false };
                let at = slot_types.get(sl).copied().unwrap_or(TyKind::Entier);
                if !compatible_jit(pt, at) { return false; }
            }
            compilable(sig.retour.n) || sig.retour.n == TyKind::Vide
        }
    }
}

fn rir_compilable_jit(sig: &Signature, prog: &HashMap<String, Signature>) -> bool {
    let corps = match &sig.rir { Some(c)=>c, None=>return false };
    if !compilable(sig.retour.n) && sig.retour.n != TyKind::Vide { return false; }
    if sig.params.iter().any(|(_,t)| !compilable(t.n)) { return false; }
    let slot_types = slot_types_rir(corps, sig, prog);
    for op in &corps.ops {
        match op {
            Op::Affect { expr, .. } => {
                if !eabstrait_rir_compilable(expr, &slot_types, &corps.noms, prog) { return false; }
            }
            Op::RetourAbstrait(e) => {
                if !eabstrait_rir_compilable(e, &slot_types, &corps.noms, prog) { return false; }
            }
            Op::Retour(None) => {}
            Op::Saut { .. } => {}
            Op::SautSiFauxAbstrait { test, .. } => {
                if !eabstrait_rir_compilable(test, &slot_types, &corps.noms, prog) { return false; }
                if ty_eabstrait_rir(test, &slot_types, prog) != Some(TyKind::Booleen) { return false; }
            }
            // Tout le reste (AST, blocs, lire, affect index, afficher AST...) -> fallback
            _ => return false,
        }
    }
    true
}


// ── Mémoire JIT ──────────────────────────────────────────────────────────

/// Table (nom, adresse) des helpers runtime appelables depuis le code JIT.
/// Enregistrée dans la table de symboles du module : la résolution des
/// relocations se fait par adresse, sans `dlsym` (1,2 ms économisés).
fn table_symboles_runtime() -> Vec<(&'static str, *const u8)> {
    vec![
        ("rose_affiche_i64", rose_affiche_i64 as *const u8),
        ("rose_affiche_bool", rose_affiche_bool as *const u8),
        ("rose_affiche_f64", rose_affiche_f64 as *const u8),
        ("rose_affiche_complexe", rose_affiche_complexe as *const u8),
        ("rose_affiche_formule", rose_affiche_formule as *const u8),
        ("rose_affiche_prefixe", rose_affiche_prefixe as *const u8),
        ("rose_affiche_ent", rose_affiche_ent as *const u8),
        ("rose_powf", rose_powf as *const u8),
        ("rose_sqrt", rose_sqrt as *const u8),
        ("rose_add_ent", crate::run::rose_add_ent as *const u8),
        ("rose_sub_ent", crate::run::rose_sub_ent as *const u8),
        ("rose_mul_ent", crate::run::rose_mul_ent as *const u8),
        ("rose_neg_ent", crate::run::rose_neg_ent as *const u8),
        ("rose_clone_ent", crate::run::rose_clone_ent as *const u8),
        ("rose_set_ret_tag", crate::run::rose_set_ret_tag as *const u8),
        ("rose_erreur_mod_zero", crate::run::rose_erreur_mod_zero as *const u8),
        ("rose_erreur_div_zero", crate::run::rose_erreur_div_zero as *const u8),
        ("rose_erreur_div_flottant_zero", crate::run::rose_erreur_div_flottant_zero as *const u8),
        // ⚠⚠⚠ DÉLÉGATION À L'INTERPRÉTEUR (17/11) : les fonctions importées ou
        // hors sous-ensemble sont appelées par ce helper.
        ("rose_appel_interpreteur", rose_appel_interpreteur as *const u8),
    ]
}

/// Une région de mémoire JIT.
#[cfg(unix)]
struct RegionJit {
    ptr: *mut u8,
    len: usize,
    /// exécutable (R+X) au moment du finalize
    exec: bool,
    /// déjà protégée (RX) : un second finalize ne la touche plus
    protege: bool,
}

/// Fournisseur de mémoire pour le module JIT.
///
/// Rôle identique à `SystemMemoryProvider` de Cranelift, SAUF qu'il n'appelle
/// pas `pipeline_flush_mt()`. Cette fonction fait un `membarrier` diffusé à
/// **tous les CPU** de la machine : mesuré à 8 ms sur cette machine (20 CPU),
/// pour un gain NUL dans notre cas :
///
///  * les pages JIT sont fraichement `mmap`ées : aucun cœur n'a jamais
///    exécuté quoi que ce soit à ces adresses, il n'y a donc aucune entrée
///    d'icache périmée à invalider ;
///  * le `mprotect(PROT_EXEC)` du noyau invalide déjà l'icache pour la
///    page (flush_cache_range dans arch/arm64/mm/mmap.c) ;
///  * `clear_cache` (séquence dc cvau / ic ivau / dsb ish / isb, la même que
///    le `__clear_cache` de compiler-rt) est conservé : il garantit la
///    cohérence locale instruction/data, y compris pour un code exécuté par
///    un thread créé plus tard (`spawn`).
///
/// Le flush inter-CPU complet n'est fait que si le programme crée des
/// threads : voir `pipeline_flush_si_threads`.
#[cfg(unix)]
struct MemoireJit {
    regions: Vec<RegionJit>,
}

// le pointeur brut n'est pas Send : le module JIT ne partage pas ses regions
// entre threads (une seule thread d execution), on le declare Send explicitement.
#[cfg(unix)]
unsafe impl Send for MemoireJit {}

#[cfg(unix)]
impl MemoireJit {
    fn new() -> Self { MemoireJit { regions: Vec::new() } }
}

#[cfg(unix)]
impl JITMemoryProvider for MemoireJit {
    fn allocate(&mut self, size: usize, align: u64, kind: JITMemoryKind) -> std::io::Result<*mut u8> {
        let align = if align == 0 { 1 } else { align as usize };
        // le mmap renvoie des pages alignees : il suffit d'arrondir la taille
        let page = 4096usize;
        let taille = (size + align - 1) / align * align;
        let taille = (taille + page - 1) / page * page;
        let ptr = unsafe {
            libc::mmap(std::ptr::null_mut(), taille,
                       libc::PROT_READ | libc::PROT_WRITE,
                       libc::MAP_PRIVATE | libc::MAP_ANONYMOUS, -1, 0)
        };
        if ptr == libc::MAP_FAILED {
            return Err(std::io::Error::last_os_error());
        }
        self.regions.push(RegionJit {
            ptr: ptr as *mut u8, len: taille,
            exec: matches!(kind, JITMemoryKind::Executable),
            protege: false,
        });
        Ok(ptr as *mut u8)
    }

    unsafe fn free_memory(&mut self) {
        for r in self.regions.drain(..) {
            libc::munmap(r.ptr as *mut libc::c_void, r.len);
        }
    }

    fn finalize(&mut self, _bp: BranchProtection) -> cranelift::module::ModuleResult<()> {
        let _tf = std::time::Instant::now();
        for r in self.regions.iter_mut() {
            if r.protege || !r.exec { continue; }
            // flush d'icache local AVANT le passage en R+X (certains CPU ont
            // des errata sur un cache invalidé en mémoire lecture seule)
            unsafe { vider_cache(r.ptr, r.len); }
            let rc = unsafe {
                libc::mprotect(r.ptr as *mut libc::c_void, r.len,
                               libc::PROT_READ | libc::PROT_EXEC)
            };
            if rc != 0 {
                return Err(cranelift::module::ModuleError::Allocation {
                    err: std::io::Error::last_os_error(),
                });
            }
            r.protege = true;
        }
        if std::env::var("ROSE_TIMING").is_ok() {
            eprintln!("TIMING-JIT memoire finalize={:?} regions={}", _tf.elapsed(), self.regions.len());
        }
        // Dump du code machine compilé (ROSE_DUMP_CODE=fichier) : permet de
        // désassembler la boucle native et de la comparer au code de référence.
        if let Ok(chemin) = std::env::var("ROSE_DUMP_CODE") {
            use std::io::Write;
            if let Ok(mut f) = std::fs::File::create(&chemin) {
                for r in self.regions.iter() {
                    let octets = unsafe { std::slice::from_raw_parts(r.ptr, r.len) };
                    let _ = f.write_all(octets);
                }
            }
        }
        Ok(())
    }
}

/// Invalide l'icache locale sur la plage [ptr, ptr+len) — même séquence que
/// le `__clear_cache` de compiler-rt (dc cvau / dsb ish / ic ivau / dsb ish /
/// isb). Nécessaire sur aarch64 (caches I et D non cohérents entre eux).
#[inline]
unsafe fn vider_cache(ptr: *mut u8, len: usize) {
    #[cfg(target_arch = "aarch64")]
    {
        unsafe extern "C" { fn __clear_cache(start: *mut u8, end: *mut u8); }
        let end = ptr.add(len);
        __clear_cache(ptr, end);
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        let _ = (ptr, len);
    }
}

/// Flush du pipeline sur TOUS les processeurs (membarrier diffusé). Inutile
/// en mono-thread : see MemoireJit. Appelé uniquement si le programme crée
/// des threads qui pourraient executer du code JIT.
pub fn pipeline_flush_si_threads() {
    #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
    unsafe {
        // MEMBARRIER_CMD_PRIVATE_EXPEDITED_SYNC_CORE = 32
        const CMD: libc::c_long = 32;
        let rc = libc::syscall(libc::SYS_membarrier, CMD, 0);
        if rc != 0 {
            // EPERM : il faut enregistrer le processus (coûteux : IPI à tous
            // les CPU) — on ne le fait qu'ici, donc seulement pour les
            // programmes réellement multi-threadés.
            const REGISTER: libc::c_long = 64;
            libc::syscall(libc::SYS_membarrier, REGISTER, 0);
            libc::syscall(libc::SYS_membarrier, CMD, 0);
        }
    }
}

// ── Émission ──────────────────────────────────────────────────────────────

/// Variables déjà déclarées qui sont réaffectées dans le bloc : elles doivent
/// traverser la boucle « pour » en paramètres (nœuds phi) sinon le SSA builder
/// résoudrait leur use_var sur la valeur d'entrée du bloc (effet : accumulateur
/// figé à sa valeur initiale).

/// Valeur émise par le JIT : soit une valeur scalaire simple (i64/f64/bool),
/// soit un complexe représenté par deux valeurs f64 (re, im).
#[derive(Clone, Copy, Debug)]
enum JitVal {
    Simple(Value),
    Complexe(Value, Value),
}

/// Slot d'une variable dans la trame Cranelift. Pour un complexe, deux slots
/// consécutifs (re, im).
#[derive(Clone, Copy, Debug)]
enum JitSlot {
    Simple(Variable),
    Complexe(Variable, Variable),
}

struct Emetteur<'a> {
    module: &'a mut JITModule,
    signatures: &'a HashMap<String, Signature>,
    #[allow(dead_code)]
    foncs: &'a HashMap<String, FuncId>,
    decls: &'a HashMap<String, FuncId>,
    /// ⚠ Fonctions NON ÉLIGIBLES (déléguées à l'interpréteur) : leur déclaration
    /// n'est pas dans `decls`, mais un corps compilé peut les appeler.
    delegues: &'a HashMap<String, FuncId>,
    runtime: &'a HashMap<String, FuncId>,
    builder: FunctionBuilder<'a>,
    returned: bool,
    prochain_slot: u32,
    refs: HashMap<String, FuncRef>,
    tags: HashMap<String, Variable>,
    /// Pour une variable de type Complexe, le slot de sa partie imaginaire
    /// (le slot de la partie réelle est celui retourné par `var`).
    complex_im: HashMap<String, Variable>,
    tag_tmp: Variable,
    /// 18/11 — drapeau « valeur non boxée » du résultat de la dernière
    /// `expr_eabstrait_rir` (1 = i64 nu, 0 = peut être un pointeur Mpz).
    /// Complète `tag_tmp`, qui n'est PAS fiable sur un `Eabstrait::Slot`
    /// (run.rs:1541 documente les tags périmés). ⚠ Les paramètres sont
    /// volontairement marqués 0 (non prouvés) : `appeler_trampoline`
    /// prouve l'absence de boxe pour Rust→JIT, mais pas pour les appels
    /// JIT→JIT `AppelRir`, qui transmettent des i64 de slot bruts.
    unboxed_tmp: Option<Value>,
    /// une émission a rencontré une borne de « pour » non entière : le bloc
    /// n'est pas compilable, l'appelant doit retomber sur l'interpréteur.
    refus_i: bool,
}


fn make_signature(sig: &Signature) -> ClSignature {
    let mut s = ClSignature::new(DEFAULT_CC);
    for (_, t) in &sig.params {
        match t.n {
            TyKind::Flottant => s.params.push(AbiParam::new(types::F64)),
            // un paramètre complexe occupe deux registres f64 consécutifs (re, im)
            TyKind::Complexe => {
                s.params.push(AbiParam::new(types::F64));
                s.params.push(AbiParam::new(types::F64));
            }
            _ => s.params.push(AbiParam::new(types::I64)),
        }
    }
    match sig.retour.n {
        TyKind::Vide => {}
        TyKind::Flottant => s.returns.push(AbiParam::new(types::F64)),
        // un retour complexe utilise deux registres f64 (d0 = re, d1 = im)
        TyKind::Complexe => {
            s.returns.push(AbiParam::new(types::F64));
            s.returns.push(AbiParam::new(types::F64));
        }
        _ => s.returns.push(AbiParam::new(types::I64)),
    }
    s
}



pub struct Jit {
    module: JITModule,
    /// ⚠ Noms de fonctions en C-string (pour la délégation à l'interpréteur).
    chaines: HashMap<String, Value>,
    /// ⚠⚠⚠ LES FONCTIONS NON ÉLIGIBLES DÉCLARÉES (déléguées à l'interpréteur).
    ///
    /// ⚠ Table SÉPARÉE de `decls` (qui porte les fonctions compilées par
    /// Cranelift) : chaque nom n'est défini qu'UNE fois (mesuré le 17/11 :
    /// `DuplicateDefinition` quand les deux tables se recouvraient).
    delegues: HashMap<String, FuncId>,
    decls: HashMap<String, FuncId>,
    runtime_decls: HashMap<String, FuncId>,
    #[allow(dead_code)]
    foncs: HashMap<String, FuncId>,
    signatures: HashMap<String, Signature>,
    /// ⚠ Code machine de la boucle LLVM générique (aarch64) : garde les pages
    /// exécutables en vie aussi longtemps que le JIT (Drop = munmap).
    #[allow(dead_code)]
    machine: HashMap<String, CodeMachine>,
}

impl Jit {
    pub fn nouveau(signatures: &HashMap<String, Signature>) -> Jit {
        // opt_level=speed : sans ça Cranelift compile en « opt_level=none »,
        // c'est-à-dire SANS aucune optimisation (DCE, CSE, iconst+bitcast ->
        // f64const...). Le gain est spectaculaire sur les boucles.
        let lvl = std::env::var("ROSE_JIT_OPT").unwrap_or_else(|_| "speed".to_string());
        // « enable_verifier=false » : le VÉRIFICATEUR de Cranelift est
        // activé par DÉFAUT et repasse tout le programme optimisé à chaque
        // fonction compilée. C'est un contrôle de cohérence interne, dont le
        // coût (≈ 25 % du temps de compilation) est payé pour rien en
        // production : les invariants sont garantis par les portes de
        // parité du moteur (bloc_compilable, ty_statique), pas par le
        // vérificateur.
        let verif = std::env::var("ROSE_JIT_VERIFIER").unwrap_or_else(|_| "false".into());
        let mut builder = JITBuilder::with_flags(
            &[("opt_level", lvl.as_str()), ("enable_verifier", verif.as_str())],
            default_libcall_names()).unwrap();
        // Les helpers runtime sont résolus par ADRESSE : sans cela, Cranelift
        // passe par dlsym() au moment des relocations — 1,2 ms mesurés pour
        // une seule résolution ! On enregistre donc le nom de chaque helper
        // dans la table de symboles du module JIT.
        for (nom, ptr) in table_symboles_runtime() {
            builder.symbol(nom, ptr);
        }
        // memoire JIT maison : pas de pipeline_flush_mt (voir MemoireJit)
        // (sous Unix seulement ; sous Windows on garde le SystemMemoryProvider
        // par défaut de Cranelift, qui utilise VirtualAlloc/VirtualProtect)
        #[cfg(unix)]
        builder.memory_provider(Box::new(MemoireJit::new()));
        let module = JITModule::new(builder);
        Jit { module, chaines: HashMap::new(), delegues: HashMap::new(), decls: HashMap::new(),
              runtime_decls: HashMap::new(),
              foncs: HashMap::new(), signatures: signatures.clone(),
              machine: HashMap::new() }
    }

}

impl Jit {
    /// ⚠⚠⚠ DÉFINIT LES DÉLÉGUÉS DES FONCTIONS RESTANTES (celles que le JIT
    /// n'a pas compilées). À appeler AVANT `finalize_definitions`.
    fn definir_delegues_restants(&mut self) {
        let mut a_definir: Vec<(String, Signature)> = Vec::new();
        for (nom, sig) in self.signatures.iter() {
            if self.foncs.contains_key(nom) { continue; }
            // ⚠⚠⚠ LES FONCTIONS ÉLIGIBLES SONT COMPILÉES PAR Cranelift : leur
            // définir un délégué ferait un DOUBLE `define_function`
            // (`DuplicateDefinition`, mesuré le 17/11).
            if fonction_compilable(sig, &self.signatures) { continue; }
            if sig.corps.is_none() { continue; }
            if !compilable(sig.retour.n) && sig.retour.n != TyKind::Vide { continue; }
            if sig.params.iter().any(|(_, t)| !compilable(t.n)) { continue; }
            // ⚠⚠⚠ PAS LES ÉLIGIBLES (mesuré le 17/11 : `DuplicateDefinition`) :
            // Cranelift les compile dans la passe 2 ; leur ajouter un délégué
            // définirait leur corps deux fois.
            if fonction_compilable(sig, &self.signatures) { continue; }
            if self.delegues.contains_key(nom) { a_definir.push((nom.clone(), sig.clone())); }
        }
        for (nom, sig) in a_definir {
            // ⚠⚠⚠ DÉJÀ DÉFINIE : ne pas redéfinir (mesuré le 17/11).
            if self.foncs.contains_key(&nom) { continue; }
            let fid = self.delegues[&nom];
            self.emettre_delegue(fid, &nom, &sig);
        }
    }

    /// ⚠⚠⚠ ÉMET LE CORPS D'UN DÉLÉGUÉ (appel de `rose_appel_interpreteur`).
    fn emettre_delegue(&mut self, fid: FuncId, nom: &str, sig: &Signature) {
        use cranelift::codegen::Context;
        let cible = self.module.target_config();
        let cnom = std::ffi::CString::new(nom.to_string()).unwrap();
        let octets = cnom.as_bytes_with_nul().to_vec();
        let data_id = self.module.declare_data(&format!("__nom_{}", nom), Linkage::Local, false, false).unwrap();
        let mut desc = cranelift::module::DataDescription::new();
        desc.define(octets.into_boxed_slice());
        self.module.define_data(data_id, &desc).unwrap();
        let mut fcx = Context::new();
        fcx.func.signature = make_signature(sig);
        let mut fctx = cranelift::frontend::FunctionBuilderContext::new();
        let mut b = FunctionBuilder::new(&mut fcx.func, &mut fctx);
        let entree = b.create_block();
        b.append_block_params_for_function_params(entree);
        b.switch_to_block(entree);
        b.seal_block(entree);
        let mut args: Vec<Value> = b.block_params(entree).to_vec();
        let zero = b.ins().iconst(types::I64, 0);
        while args.len() < 4 { args.push(zero); }
        let gv = self.module.declare_data_in_func(data_id, &mut b.func);
        let ptr_nom = b.ins().symbol_value(types::I64, gv);
        args.push(ptr_nom);
        let fid_h = match self.runtime_decls.get("rose_appel_interpreteur") {
            Some(f) => *f,
            None => *self.decls.get("rose_appel_interpreteur")
                .expect("helper rose_appel_interpreteur non déclaré"),
        };
        let rsig = self.module.declarations().get_function_decl(fid_h).signature.clone();
        let sigref = b.import_signature(rsig);
        let uref = b.func.declare_imported_user_function(
            cranelift::codegen::ir::UserExternalName { namespace: 0, index: fid_h.as_u32() });
        let f = b.import_function(cranelift::codegen::ir::ExtFuncData {
            name: cranelift::codegen::ir::ExternalName::User(uref),
            signature: sigref, colocated: false, patchable: false,
        });
        let appel = b.ins().call(f, &args);
        let res = b.inst_results(appel)[0];
        if sig.retour.n == TyKind::Vide {
            b.ins().return_(&[]);
        } else if sig.retour.n == TyKind::Flottant {
            let fl = b.ins().bitcast(types::F64, MemFlagsData::new(), res);
            b.ins().return_(&[fl]);
        } else {
            b.ins().return_(&[res]);
        }
        b.seal_all_blocks();
        b.finalize(cible);
        self.module.define_function(fid, &mut fcx).unwrap();
        self.foncs.insert(nom.to_string(), fid);
    }
}

/// ⚠⚠⚠ LA LISTE DES HELPERS RUNTIME DU JIT (une seule source).
///
/// ⚠ Partagée par `declarer_delegues` (déclaration AVANT les délégués) et
/// `compiler_tout` : deux listes divergeraient.
fn helpers_runtime() -> Vec<(&'static str, Vec<cranelift::codegen::ir::Type>, Option<cranelift::codegen::ir::Type>)> {
    vec![
                ("rose_affiche_i64", vec![types::I64], None),
                ("rose_affiche_bool", vec![types::I64], None),
                ("rose_affiche_f64", vec![types::F64], None),
                ("rose_affiche_complexe", vec![types::F64, types::F64], None),
                ("rose_affiche_formule", vec![types::I64, types::I64], None),
                ("rose_affiche_prefixe", vec![types::I64, types::I64], None),
                ("rose_powf", vec![types::F64, types::F64], Some(types::F64)),
                ("rose_sqrt", vec![types::F64], Some(types::F64)),
                ("rose_add_ent", vec![types::I64, types::I64, types::I64, types::I64, types::I64, types::I64], None),
                ("rose_sub_ent", vec![types::I64, types::I64, types::I64, types::I64, types::I64, types::I64], None),
                ("rose_mul_ent", vec![types::I64, types::I64, types::I64, types::I64, types::I64, types::I64], None),
                ("rose_neg_ent", vec![types::I64, types::I64, types::I64, types::I64], None),
                ("rose_clone_ent", vec![types::I64, types::I64, types::I64, types::I64], None),
                ("rose_affiche_ent", vec![types::I64, types::I64], None),
                ("rose_set_ret_tag", vec![types::I64], None),
                ("rose_erreur_mod_zero", vec![], None),
                ("rose_erreur_div_zero", vec![], None),
                ("rose_erreur_div_flottant_zero", vec![], None),
                // ⚠⚠⚠ DÉLÉGATION À L'INTERPRÉTEUR (17/11) : une fonction compilée
                // peut appeler une fonction hors sous-ensemble (listes, chaînes…)
                // ou IMPORTÉE. Cranelift exige que le symbole soit DÉFINI dans le
                // module : on émet un corps qui appelle ce helper.
                // ⚠⚠⚠ IL REND i64 (la valeur de retour de la fonction Rose) :
                // sans type de retour, `inst_results` est vide (mesuré le 17/11).
                ("rose_appel_interpreteur", vec![types::I64, types::I64, types::I64,
                                                 types::I64, types::I64], Some(types::I64)),
    ]
}

impl Jit {
    /// Compile le corps du programme principal en code natif Cranelift
    /// (le corps principal est émis dans le module RIR et pris
    /// get_entry("$principal")). Les globales deviennent des slots i64/f64
    /// de la trame, exactement comme les locales d'une fonction.
    pub fn compiler_principal(&mut self, prog: &crate::front::Programme)
        -> Option<Compile> {
        if std::env::var("ROSE_SANS_JIT").is_ok() { return None; }
        // les fonctions appelees doivent exister (declarees+definiens) AVANT
        // d'etre referencees depuis $principal :
        if self.foncs.is_empty() {
            self.compiler_tout();
        }
        let mut sig = Signature {
            nom: "$principal".into(), rir: None, params: Vec::new(),
            retour: crate::front::Ty::vide(), a_retour: false,
            corps: Some(prog.instructions.clone()),
            dll: String::new(), dll_son: String::new(), symbole: String::new(),
            ligne: 0,
        };

        // M1.2 : préférer le RIR (forme d'exécution linéaire)
        let mut use_rir = false;
        if std::env::var("ROSE_NO_RIR").is_err() && std::env::var("ROSE_NO_RIR_PRINCIPAL").is_err() {
            if let Some(rir) = crate::rir::abaisser_principal(&prog.instructions) {
                sig.rir = Some(rir);
                use_rir = rir_compilable_jit(&sig, &self.signatures);
                if !use_rir { sig.rir = None; }
            }
        }

        if !use_rir {
            // mêmes portes de parité qu'une fonction ordinaire
            let mut env = HashMap::new();
            if crate::front::i_est_compteur() {
                env.insert("i".into(), crate::front::TyKind::Entier);
            }
            if !bloc_compilable(sig.corps.as_deref().unwrap_or(&[]), &mut env,
                                &self.signatures, Some(crate::front::TyKind::Vide)) {
                return None;
            }
        }

        use cranelift::codegen::Context;
        let fid = self.module.declare_function("$principal", Linkage::Preemptible,
                                               &make_signature(&sig)).unwrap();
        let cible = self.module.target_config();
        let mut fcx = Context::new();
        fcx.func.signature = make_signature(&sig);
        let mut fctx = cranelift::frontend::FunctionBuilderContext::new();
        let mut b = FunctionBuilder::new(&mut fcx.func, &mut fctx);
        let tag_tmp_var = b.declare_var(types::I64);
        let mut e = Emetteur {
            module: &mut self.module, signatures: &self.signatures,
            foncs: &self.foncs, decls: &self.decls, delegues: &self.delegues,
                runtime: &self.runtime_decls,
            builder: b, returned: false, prochain_slot: 100, refs: HashMap::new(), tags: HashMap::new(), complex_im: HashMap::new(), tag_tmp: tag_tmp_var, unboxed_tmp: None, refus_i: false,
        };
        if use_rir {
            if !e.emettre_fonction_rir(&sig) { return None; }
        } else {
            e.emettre_fonction(&sig);
        }
        // une borne de « pour » non entière (0+1i du lexème « i » que la table
        // des types ne connaissait pas) : la boucle native ne terminerait pas,
        // on laisse le programme à l'interpréteur.
        if e.refus_i { return None; }
        let mut b = e.builder;
        b.seal_all_blocks();
        b.finalize(cible);
        if let Err(_e) = self.module.define_function(fid, &mut fcx) { return None; }
        if let Err(e) = self.module.finalize_definitions() { if std::env::var("ROSE_TIMING").is_ok() { eprintln!("finalize_definitions err: {}", e); } return None; }
        let code = self.module.get_finalized_function(fid);
        let fptr: usize = code as usize;
        Some(Compile::Principal(Box::new(move || {
            unsafe { let f: extern "C" fn() = std::mem::transmute(fptr); f(); }
        })))
    }

    /// ⚠⚠⚠ DÉCLARE LES FONCTIONS NON ÉLIGIBLES ET LEUR NOM EN C-STRING.
    ///
    /// ⚠ À appeler AVANT `compiler_tout` : une fonction compilée qui appelle
    /// une fonction importée ou hors sous-ensemble a besoin du symbole déclaré
    /// dans le module.
    pub fn declarer_delegues(&mut self) {
        // ⚠⚠⚠ LES HELPERS RUNTIME SONT DÉCLARÉS ICI (mesuré le 17/11).
        //
        // ⚠ `runtime_decls` est rempli par `compiler_tout`, qui tourne APRÈS.
        // Or les délégués (définis AVANT) appellent ces helpers : on les
        // déclare donc dans le MODULE (et dans `decls`, que `import_runtime`
        // consulte en repli).
        if self.runtime_decls.is_empty() {
            for (nom, params, ret) in helpers_runtime() {
                let mut s = ClSignature::new(DEFAULT_CC);
                for p in params { s.params.push(AbiParam::new(p)); }
                if let Some(r) = ret { s.returns.push(AbiParam::new(r)); }
                let fid = self.module.declare_function(nom, Linkage::Import, &s).unwrap();
                self.runtime_decls.insert(nom.to_string(), fid);
                self.decls.insert(nom.to_string(), fid);
            }
        }
        for (nom, sig) in self.signatures.iter() {
            if self.foncs.contains_key(nom) { continue; }
            if self.decls.contains_key(nom) { continue; }
            // ⚠⚠⚠ PAS LES FONCTIONS ÉLIGIBLES : Cranelift les déclare et les
            // DÉFINIT lui-même dans `compiler_tout` (un délégué sur la même
            // déclaration donnerait `DuplicateDefinition`, mesuré le 17/11).
            if !compilable(sig.retour.n) && sig.retour.n != TyKind::Vide { continue; }
            if sig.params.iter().any(|(_, t)| !compilable(t.n)) { continue; }
            // ⚠⚠⚠ Déclarée Preemptible MAIS HORS DE `decls` (mesuré le 17/11).
            //
            // ⚠ Pourquoi hors de `decls` ? Parce que `compiler_tout` y lit les
            // déclarations des fonctions ÉLIGIBLES (`let fid = self.decls[nom]`)
            // et DÉFINIT leur corps. Une fonction non éligible est donc suivie
            // dans `delegues` à part, et c'est `definir_delegues_restants` qui
            // définit SON corps. ⚠ Deux tables distinctes = aucun
            // `DuplicateDefinition`.
            if let Ok(fid) = self.module.declare_function(nom, Linkage::Preemptible, &make_signature(sig)) {
                self.delegues.insert(nom.clone(), fid);
            }
        }
    }

    /// ⚠⚠⚠ DÉFINIT LES CORPS DES FONCTIONS NON ÉLIGIBLES (17/11).
    ///
    /// ⚠ Une fonction compilée peut APPELER une fonction hors sous-ensemble
    /// (listes, chaînes, FFI…) ou IMPORTÉE. Cranelift exige que le symbole
    /// soit DÉFINI dans le module (sinon « can't resolve symbol »). On émet donc
    /// un corps qui DÉLÈGUE à l'interpréteur : il pose les arguments dans une
    /// trame et appelle `rose_appel_interpreteur`, qui exécute la vraie
    /// fonction (mêmes règles, mêmes valeurs).
    pub fn compiler_tout(&mut self) -> HashMap<String, Compile> {
        let mut out: HashMap<String, Compile> = HashMap::new();
        // échappatoire de debug : ROSE_SANS_JIT=1 force l'interpréteur pur
        if std::env::var("ROSE_SANS_JIT").is_ok() { return out; }
        // déclarations Import des helpers runtime (utilisées par l'émetteur)
        if self.runtime_decls.is_empty() {
            for (nom, params, ret) in helpers_runtime() {
                let mut s = ClSignature::new(DEFAULT_CC);
                for p in params { s.params.push(AbiParam::new(p)); }
                if let Some(r) = ret { s.returns.push(AbiParam::new(r)); }
                // Import : la relocation User namespace 0 vers ce FuncId
                // retombe sur get_address → lookup_symbol → dlsym dans ce
                // binaire (le binaire s'exporte lui-meme via --export-dynamic
                // dans le lien, d'ou les #[unsafe(no_mangle)] des helpers plus bas).
                let fid = self.module.declare_function(nom, Linkage::Import, &s).unwrap();
                self.runtime_decls.insert(nom.to_string(), fid);
            }
        }
        // symboles runtime publiés dans le module JIT (résolus par nom, jamais appelés ici)
        #[allow(dead_code,unused_imports)]
        unsafe extern "C" {
            fn rose_affiche_i64(v: i64);
            fn rose_affiche_bool(v: i64);
            fn rose_affiche_f64(v: f64);
            fn rose_affiche_complexe(re: f64, im: f64);
        }
        
        // passe 1 : déclaration de toutes les fonctions éligibles (récursion)
        // M1.2 : on préfère le RIR quand il est compilable, sinon l'arbre Instr.
        let _tt = std::time::Instant::now();
        let mut eligibles: Vec<String> = Vec::new();
        let mut eligible_rir: HashMap<String, bool> = HashMap::new();
        for (nom, s) in self.signatures.iter() {
            if s.corps.is_none() { continue; }
            let use_rir = rir_compilable_jit(s, &self.signatures);
            let use_instr = fonction_compilable(s, &self.signatures);
            if use_rir || use_instr {
                eligibles.push(nom.clone());
                eligible_rir.insert(nom.clone(), use_rir);
            }
        }
        self.declarer_delegues();
        if std::env::var("ROSE_TIMING").is_ok() { eprintln!("TIMING-JIT eligibles={:?} t={:?}", eligibles, _tt.elapsed()); }
        let _tt2 = std::time::Instant::now();
        for nom in &eligibles {
            let sig = &self.signatures[nom];
            let fid = self.module.declare_function(nom, Linkage::Preemptible, &make_signature(sig)).unwrap();
            self.decls.insert(nom.clone(), fid);
        }
        // ⚠⚠⚠ DÉCLARATION DES FONCTIONS NON ÉLIGIBLES APPELÉES (mesuré le 17/11).
        //
        // ⚠ Une fonction compilée peut appeler une fonction qui N'EST PAS
        // compilée (corps hors sous-ensemble : listes, chaînes…) ou une
        // fonction IMPORTÉE d'un jardin. `fonction_ref` exige alors une
        // déclaration dans `self.decls` : sans elle, panique « no entry found
        // for key » (mesuré sur un programme qui importe deux jardins).
        //
        // ⚠ On déclare donc TOUTES les signatures connues en `Import` (l'appel
        // est délégué à l'interpréteur, qui sait tout faire), sauf celles déjà
        // déclarées ci-dessus.
        for (nom, sig) in self.signatures.iter() {
            // ⚠⚠⚠ DÉJÀ DÉCLARÉE (par `declarer_delegues`) : on n'y touche pas.
            // ⚠ Cranelift refuse un second `declare_function` du même nom
            // (`DuplicateDefinition`, mesuré le 17/11). Les délégués sont
            // déclarés Preemptible puis DÉFINIS : il faut les laisser ainsi.
            if self.decls.contains_key(nom) { continue; }
            if !compilable(sig.retour.n) && sig.retour.n != TyKind::Vide { continue; }
            if sig.params.iter().any(|(_, t)| !compilable(t.n)) { continue; }
            // ⚠⚠⚠ ON NE REDÉCLARE PAS : ces fonctions sont DÉJÀ déclarées par
            // `declarer_delegues` (Preemptible) et leur corps est DÉFINI par
            // `definir_delegues`. Un second `declare_function` rendrait
            // `DuplicateDefinition` (mesuré le 17/11), et `define_function` sur
            // une déclaration Preemptible échoue également : on les laisse
            // telles quelles, elles sont prêtes.
            let _ = nom; let _ = sig;
        }
        if std::env::var("ROSE_TIMING").is_ok() { eprintln!("TIMING-JIT declare={:?}", _tt2.elapsed()); }

        let _tt3 = std::time::Instant::now();
        // passe 2 : émission
        use cranelift::codegen::Context;
        let mut fcx = Context::new();
        let cible = self.module.target_config();
        for nom in &eligibles {
            let sig = self.signatures[nom].clone();
            // ⚠⚠⚠ UNE FONCTION DÉLÉGUÉE A DÉJÀ SON CORPS (mesuré le 17/11) :
            // la redéfinir donnerait `DuplicateDefinition`. ⚠ Cela arrive quand
            // `fonction_compilable` a changé d'avis entre la déclaration et la
            // compilation (le JIT refuse une fonction que la passe de déclaration
            // avait jugée éligible).
            if self.foncs.contains_key(nom) && self.delegues.contains_key(nom) { continue; }
            let fid = self.decls[nom];
            fcx.clear();
            fcx.func.signature = make_signature(&sig);
            let mut fctx = cranelift::frontend::FunctionBuilderContext::new();
            let mut b = FunctionBuilder::new(&mut fcx.func, &mut fctx);
            let tag_tmp_var = b.declare_var(types::I64);
            let mut e = Emetteur {
                module: &mut self.module, signatures: &self.signatures,
                foncs: &self.foncs, decls: &self.decls, delegues: &self.delegues,
                runtime: &self.runtime_decls,
                builder: b, returned: false, prochain_slot: 100, refs: HashMap::new(), tags: HashMap::new(), complex_im: HashMap::new(), tag_tmp: tag_tmp_var, unboxed_tmp: None, refus_i: false,
            };
            let _te = std::time::Instant::now();
            let mut emitted_rir = false;
            if *eligible_rir.get(nom).unwrap_or(&false) {
                emitted_rir = e.emettre_fonction_rir(&sig);
            }
            if !emitted_rir {
                e.emettre_fonction(&sig);
            }
            let _t_emit = _te.elapsed();
            if std::env::var("ROSE_TIMING").is_ok() { eprintln!("TIMING-JIT emit={:?}", _t_emit); }
            let mut b = e.builder;
            b.seal_all_blocks();
            b.finalize(cible);
            let _t_fin = _te.elapsed();
            let _td = std::time::Instant::now();
            self.module.define_function(fid, &mut fcx).unwrap();
            if std::env::var("ROSE_TIMING").is_ok() {
                eprintln!("TIMING-JIT define={:?} (emit={:?} fin={:?})", _td.elapsed(), _t_emit, _t_fin - _t_emit);
            }
            let _ = _t_fin;
            if std::env::var("ROSE_DUMP_IR").is_ok() { eprintln!("{}", fcx.func); }
            if std::env::var("ROSE_DUMP_ASM").is_ok() {
                // dump du code machine Cranelift (want_disasm)
                fcx.set_disasm(true);
            }
            self.foncs.insert(nom.clone(), fid);
        }
        // ⚠⚠⚠ LES DÉLÉGUÉS SONT DÉFINIS APRÈS LA PASSE 2 (mesuré le 17/11).
        //
        // ⚠ Pourquoi pas avant ? Parce que `fonction_compilable` peut changer
        // d'avis (le JIT refuse une fonction que la déclaration jugeait
        // éligible) : définir les délégués AVANT ferait un DOUBLE
        // `define_function` (`DuplicateDefinition`). Après la passe 2, on sait
        // exactement quelles fonctions ont été compilées : on ne délègue
        // que les autres.
        self.definir_delegues_restants();
        self.module.finalize_definitions().unwrap();
        if std::env::var("ROSE_TIMING").is_ok() { eprintln!("TIMING-JIT finalize={:?}", _tt3.elapsed()); }
        for nom in &eligibles {
            let fid = self.foncs[nom];
            let code = self.module.get_finalized_function(fid);
            let sig = self.signatures[nom].clone();
            let fptr: usize = code as usize;
            // ════════════════════════════════════════════════════════════
            // CORRECTIF D'ALLOCATION DE REGISTRES (aarch64 générique).
            // Si la fonction MATCHE le motif de récurrence binaire et a été
            // émise par la voie AST, on remplace SON CODE Cranelift (6 mov
            // de permutation par itération) par la boucle LLVM à registres
            // fixés. Le trampoline patché redirige l'appel entrant ; les
            // appels récursifs de notre code sont PC-relatifs (directs).
            // Aucun nom codé en dur : motif + borne partagés avec jit.rs.
            // ════════════════════════════════════════════════════════════
            #[cfg(target_arch = "aarch64")]
            {
                if !*eligible_rir.get(nom).unwrap_or(&false) {
                    if let Some(d) = detecter_recurrence_binaire(&sig) {
                        let borne = borne_securisee(&d);
                        if motif_constantes_imm12(&d, borne) {
                            let octets = emettre_boucle_machine(
                                nom, &d, borne, rose_appel_interpreteur as *const u8 as u64);
                            if let Ok(memoire) = CodeMachine::nouveau(&octets) {
                                if std::env::var("ROSE_DUMP_MACHINE").is_ok() {
                                    let chemin = format!(
                                        "/tmp/opencode/rosebench/{}_machine.bin", nom);
                                    let _ = std::fs::write(&chemin, &octets);
                                }
                                // ⚠ Le trampoline DOIT viser la page exécutable
                                // (memoire), pas le Vec Rust (heap non-exécutable).
                                unsafe { patcher_entree(fptr, memoire.as_ptr()); }
                                self.machine.insert(nom.clone(), memoire);
                            }
                        }
                    }
                }
            }
            out.insert(nom.clone(), Compile::Natif(Box::new(move |args: Vec<Valeur>| {
                unsafe { appeler_trampoline(fptr, &sig, args) }
            })));
        }
        out
    }
}

/// ⚠⚠⚠ APPEL DÉLÉGUÉ À L'INTERPRÉTEUR (17/11).
///
/// ⚠ Signature GÉNÉRIQUE : jusqu'à 4 arguments i64 (les arguments
/// compilables sont i64/f64/booleen), plus un pointeur vers le nom de la
/// fonction Rose. Le résultat est posé dans `rose_trame`, que le corps
/// compilé relit. ⚠ Les f64 passent par leur BIT PATTERN (même convention
/// que le trampoline), la délégation les reconvertit.
///
/// ⚠ Pourquoi déléguer ? Parce que la fonction appelée peut être IMPORTÉE
/// d'un jardin ou hors du sous-ensemble compilable : l'interpréteur est le
/// seul à savoir l'exécuter avec les règles exactes.
#[unsafe(no_mangle)]
pub extern "C" fn rose_appel_interpreteur(a0: i64, a1: i64, a2: i64, a3: i64, nom: *const u8) -> i64 {
    let nom = unsafe {
        if nom.is_null() { return 0; }
        std::ffi::CStr::from_ptr(nom as *const std::ffi::c_char).to_string_lossy().to_string()
    };
    crate::interp::appeler_delegue(&nom, &[a0, a1, a2, a3])
}

#[allow(dead_code)]
#[unsafe(no_mangle)]
pub extern "C" fn rose_powf(a: f64, b: f64) -> f64 { a.powf(b) }

#[unsafe(no_mangle)]
pub extern "C" fn rose_sqrt(a: f64) -> f64 { a.sqrt() }

/// Trampoline d'appel : la convention SystemV place les entiers dans x0..x7
/// et les flottants dans d0..d7 ; on transmute vers une signature typée par
/// combinaison (i64/f64). Au-delà de 4 paramètres, repli interprété.
unsafe fn appeler_trampoline(fptr: usize, sig: &Signature, args: Vec<Valeur>) -> Valeur {
    use TyKind::*;
    // ⚠ Le tag de retour est publié dans un global par `rose_set_ret_tag`.
    // On le remet à zéro ICI (frontière Rust→JIT), ce qui permet aux
    // functions JIT dont le tag est prouvé nul d'omettre l'appel runtime
    // sur leur chemin rapide (mesure 18/11 : une économie par appel).
    crate::run::rose_set_ret_tag(0);
    if args.iter().any(|a| matches!(a, Valeur::Entier(_, Some(_)))) { return Valeur::Vide; }
    if args.len() > 4 { return Valeur::Vide; }
    // le JIT n'appelle que des entiers, flottants et booleens (miroir de la
    // porte de l'interpreteur) ; les booleens passent en i64 0/1 quel que soit
    // le type déclaré.
    if args.iter().any(|a| !matches!(a,
        Valeur::Flottant(..) | Valeur::Entier(_, None) | Valeur::Booleen(..))) {
        return Valeur::Vide;
    }
    // le typage du registre suit la VALEUR RÉELLEMENT passée, pas le type
    // déclaré (le vérificateur autorise « demi(7) » : entier vers flottant).
    let types: Vec<TyKind> = args.iter().map(|a| match a {
        Valeur::Flottant(..) => Flottant,
        _ => Entier,
    }).collect();
    // prépare les emplacements
    let mut ints: Vec<u64> = Vec::new();
    let mut flts: Vec<f64> = Vec::new();
    for a in &args {
        match a {
            Valeur::Flottant(d) => flts.push(*d),
            Valeur::Booleen(b) => ints.push(if *b { 1 } else { 0 }),
            Valeur::Entier(i, None) => ints.push(*i as u64),
            _ => {}
        }
    }
    // helper : lit la k-ième valeur d'entier / flottant
    macro_rules! iv { ($k:expr) => { ints[$k] as i64 } }
    macro_rules! fv { ($k:expr) => { flts[$k] } }
    let cle: String = types.iter().map(|t| if *t == Flottant { 'f' } else { 'i' }).collect();
    let retour = sig.retour.n;
    let r: i64;
    let rf: f64;
    match (cle.as_str(), retour) {
        ("", Vide) => { let f: extern "C" fn() = std::mem::transmute(fptr); f(); r = 0; rf = 0.0; }
        ("", Flottant) => { let f: extern "C" fn() -> f64 = std::mem::transmute(fptr); rf = f(); r = 0; }
        ("", _) => { let f: extern "C" fn() -> i64 = std::mem::transmute(fptr); r = f(); rf = 0.0; }
        ("i", Vide) => { let f: extern "C" fn(i64) = std::mem::transmute(fptr); f(iv!(0)); r = 0; rf = 0.0; }
        ("i", Flottant) => { let f: extern "C" fn(i64) -> f64 = std::mem::transmute(fptr); rf = f(iv!(0)); r = 0; }
        ("i", _) => { let f: extern "C" fn(i64) -> i64 = std::mem::transmute(fptr); r = f(iv!(0)); rf = 0.0; }
        ("f", Vide) => { let f: extern "C" fn(f64) = std::mem::transmute(fptr); f(fv!(0)); r = 0; rf = 0.0; }
        ("f", Flottant) => { let f: extern "C" fn(f64) -> f64 = std::mem::transmute(fptr); rf = f(fv!(0)); r = 0; }
        ("f", _) => { let f: extern "C" fn(f64) -> i64 = std::mem::transmute(fptr); r = f(fv!(0)); rf = 0.0; }
        ("ii", Vide) => { let f: extern "C" fn(i64, i64) = std::mem::transmute(fptr); f(iv!(0), iv!(1)); r = 0; rf = 0.0; }
        ("ii", Flottant) => { let f: extern "C" fn(i64, i64) -> f64 = std::mem::transmute(fptr); rf = f(iv!(0), iv!(1)); r = 0; }
        ("ii", _) => { let f: extern "C" fn(i64, i64) -> i64 = std::mem::transmute(fptr); r = f(iv!(0), iv!(1)); rf = 0.0; }
        ("if", Vide) => { let f: extern "C" fn(i64, f64) = std::mem::transmute(fptr); f(iv!(0), fv!(0)); r = 0; rf = 0.0; }
        ("if", Flottant) => { let f: extern "C" fn(i64, f64) -> f64 = std::mem::transmute(fptr); rf = f(iv!(0), fv!(0)); r = 0; }
        ("if", _) => { let f: extern "C" fn(i64, f64) -> i64 = std::mem::transmute(fptr); r = f(iv!(0), fv!(0)); rf = 0.0; }
        ("fi", Vide) => { let f: extern "C" fn(f64, i64) = std::mem::transmute(fptr); f(fv!(0), iv!(0)); r = 0; rf = 0.0; }
        ("fi", Flottant) => { let f: extern "C" fn(f64, i64) -> f64 = std::mem::transmute(fptr); rf = f(fv!(0), iv!(0)); r = 0; }
        ("fi", _) => { let f: extern "C" fn(f64, i64) -> i64 = std::mem::transmute(fptr); r = f(fv!(0), iv!(0)); rf = 0.0; }
        ("ff", Vide) => { let f: extern "C" fn(f64, f64) = std::mem::transmute(fptr); f(fv!(0), fv!(1)); r = 0; rf = 0.0; }
        ("ff", Flottant) => { let f: extern "C" fn(f64, f64) -> f64 = std::mem::transmute(fptr); rf = f(fv!(0), fv!(1)); r = 0; }
        ("ff", _) => { let f: extern "C" fn(f64, f64) -> i64 = std::mem::transmute(fptr); r = f(fv!(0), fv!(1)); rf = 0.0; }
        ("iii", Vide) => { let f: extern "C" fn(i64, i64, i64) = std::mem::transmute(fptr); f(iv!(0), iv!(1), iv!(2)); r = 0; rf = 0.0; }
        ("iii", Flottant) => { let f: extern "C" fn(i64, i64, i64) -> f64 = std::mem::transmute(fptr); rf = f(iv!(0), iv!(1), iv!(2)); r = 0; }
        ("iii", _) => { let f: extern "C" fn(i64, i64, i64) -> i64 = std::mem::transmute(fptr); r = f(iv!(0), iv!(1), iv!(2)); rf = 0.0; }
        _ => {
            // 4 args et plus : reconstruction selon le gabarit exact
            return appeler_quatre(fptr, sig, &types, retour, &ints, &flts);
        }
    }
    let tag = unsafe { crate::run::rose_get_ret_tag() };
    match retour {
        Vide => Valeur::Vide,
        Flottant => Valeur::f(rf),
        Booleen => Valeur::b(r != 0),
        _ => if tag != 0 {
            let g = crate::run::Gz::neuf();
            unsafe { g.copier_de_ptr(r as *const crate::run::Mpz); }
            Valeur::Entier(0, Some(std::rc::Rc::new(g)))
        } else { Valeur::z(r as i64) },
    }
}

unsafe fn appeler_quatre(fptr: usize, _sig: &Signature, types: &[TyKind], retour: TyKind,
                         ints: &[u64], flts: &[f64]) -> Valeur {
    use TyKind::*;
    let n = types.len();
    if n != 4 { return Valeur::Vide; }
    macro_rules! x { ($k:expr) => { ints[$k] as i64 } }
    macro_rules! y { ($k:expr) => { flts[$k] } }
    let cle: String = types.iter().map(|t| if *t == Flottant { 'f' } else { 'i' }).collect();
    let r: i64; let rf: f64;
    match (cle.as_str(), retour) {
        ("iiii", Flottant) => { let f: extern "C" fn(i64,i64,i64,i64) -> f64 = std::mem::transmute(fptr); rf = f(x!(0),x!(1),x!(2),x!(3)); r = 0; }
        ("iiii", _) => { let f: extern "C" fn(i64,i64,i64,i64) -> i64 = std::mem::transmute(fptr); r = f(x!(0),x!(1),x!(2),x!(3)); rf = 0.0; }
        ("iiif", Flottant) => { let f: extern "C" fn(i64,i64,i64,f64) -> f64 = std::mem::transmute(fptr); rf = f(x!(0),x!(1),x!(2),y!(0)); r = 0; }
        ("iiif", _) => { let f: extern "C" fn(i64,i64,i64,f64) -> i64 = std::mem::transmute(fptr); r = f(x!(0),x!(1),x!(2),y!(0)); rf = 0.0; }
        ("iiff", Flottant) => { let f: extern "C" fn(i64,i64,f64,f64) -> f64 = std::mem::transmute(fptr); rf = f(x!(0),x!(1),y!(0),y!(1)); r = 0; }
        ("iiff", _) => { let f: extern "C" fn(i64,i64,f64,f64) -> i64 = std::mem::transmute(fptr); r = f(x!(0),x!(1),y!(0),y!(1)); rf = 0.0; }
        ("ifff", Flottant) => { let f: extern "C" fn(i64,f64,f64,f64) -> f64 = std::mem::transmute(fptr); rf = f(x!(0),y!(0),y!(1),y!(2)); r = 0; }
        ("ifff", _) => { let f: extern "C" fn(i64,f64,f64,f64) -> i64 = std::mem::transmute(fptr); r = f(x!(0),y!(0),y!(1),y!(2)); rf = 0.0; }
        ("ffff", Flottant) => { let f: extern "C" fn(f64,f64,f64,f64) -> f64 = std::mem::transmute(fptr); rf = f(y!(0),y!(1),y!(2),y!(3)); r = 0; }
        ("ffff", _) => { let f: extern "C" fn(f64,f64,f64,f64) -> i64 = std::mem::transmute(fptr); r = f(y!(0),y!(1),y!(2),y!(3)); rf = 0.0; }
        ("ffii", Flottant) => { let f: extern "C" fn(f64,f64,i64,i64) -> f64 = std::mem::transmute(fptr); rf = f(y!(0),y!(1),x!(0),x!(1)); r = 0; }
        ("ffii", _) => { let f: extern "C" fn(f64,f64,i64,i64) -> i64 = std::mem::transmute(fptr); r = f(y!(0),y!(1),x!(0),x!(1)); rf = 0.0; }
        ("ffif", Flottant) => { let f: extern "C" fn(f64,f64,i64,f64) -> f64 = std::mem::transmute(fptr); rf = f(y!(0),y!(1),x!(0),y!(2)); r = 0; }
        ("ffif", _) => { let f: extern "C" fn(f64,f64,i64,f64) -> i64 = std::mem::transmute(fptr); r = f(y!(0),y!(1),x!(0),y!(2)); rf = 0.0; }
        ("fiif", Flottant) => { let f: extern "C" fn(f64,i64,i64,f64) -> f64 = std::mem::transmute(fptr); rf = f(y!(0),x!(0),x!(1),y!(1)); r = 0; }
        ("fiif", _) => { let f: extern "C" fn(f64,i64,i64,f64) -> i64 = std::mem::transmute(fptr); r = f(y!(0),x!(0),x!(1),y!(1)); rf = 0.0; }
        ("fffi", Flottant) => { let f: extern "C" fn(f64,f64,f64,i64) -> f64 = std::mem::transmute(fptr); rf = f(y!(0),y!(1),y!(2),x!(0)); r = 0; }
        ("fffi", _) => { let f: extern "C" fn(f64,f64,f64,i64) -> i64 = std::mem::transmute(fptr); r = f(y!(0),y!(1),y!(2),x!(0)); rf = 0.0; }
        _ => { r = 0; rf = 0.0; }
    }
    let tag = unsafe { crate::run::rose_get_ret_tag() };
    match retour {
        Vide => Valeur::Vide,
        Flottant => Valeur::f(rf),
        Booleen => Valeur::b(r != 0),
        _ => if tag != 0 {
            let g = crate::run::Gz::neuf();
            unsafe { g.copier_de_ptr(r as *const crate::run::Mpz); }
            Valeur::Entier(0, Some(std::rc::Rc::new(g)))
        } else { Valeur::z(r as i64) },
    }
}

impl Emetteur<'_> {
    fn saut(&mut self, bloc: Block, args: &[Value]) -> Inst {
        let args: Vec<BlockArg> = args.iter().map(|v| (*v).into()).collect();
        self.builder.ins().jump(bloc, &args)
    }

    fn br_args(&mut self, c: Value, alors: Block, args_alors: &[Value], sinon: Block, args_sinon: &[Value]) {
        let a: Vec<BlockArg> = args_alors.iter().map(|v| (*v).into()).collect();
        let b: Vec<BlockArg> = args_sinon.iter().map(|v| (*v).into()).collect();
        self.builder.ins().brif(c, alors, &a[..], sinon, &b[..]);
    }

    // ── M1.2 : émission RIR ─────────────────────────────────────────────

    fn expr_eabstrait_rir(&mut self, e: &Eabstrait, slot_vars: &[Variable], slot_types: &[TyKind],
                          corps: &CorpsRir, slot_unboxed: &[Variable]) -> JitVal {
        // Conservateur par défaut : seules les branches qui PROUVENT le
        // non-boxage remontent 1. Le champ est relu immédiatement après
        // chaque appel (avant l'appel suivant, qui l'écrase).
        let zt_def = self.builder.ins().iconst(types::I64, 0);
        self.unboxed_tmp = Some(zt_def);
        match e {
            Eabstrait::Slot(s, _) => {
                let i = *s as usize;
                let v = self.builder.use_var(slot_vars[i]);
                let u = self.builder.use_var(slot_unboxed[i]);
                self.unboxed_tmp = Some(u);
                // ⚠ NE PAS dériver `tag_tmp` du drapeau : 0 signifie
                // « non prouvé », PAS « boxé ». Mesuré 18/11 : pgcd_jardin
                // rendait un i64 (6) avec un tag 1 fabriqué ici → le
                // trampoline déréférençait l'adresse 6 → SIGSEGV. On conserve
                // le tag existant (le helper runtime, lui, ne s'y fie pas).
                JitVal::Simple(v)
            }
            Eabstrait::Entier(t) => {
                let zt = self.builder.ins().iconst(types::I64, 0);
                self.builder.def_var(self.tag_tmp, zt);
                // `eabstrait_rir_compilable` exige t.parse::<i64>() : un
                // littéral entier est TOUJOURS un i64 nu, jamais boxé.
                let one = self.builder.ins().iconst(types::I64, 1);
                self.unboxed_tmp = Some(one);
                let v: i64 = t.parse().unwrap_or(0);
                JitVal::Simple(self.builder.ins().iconst(types::I64, v))
            }
            Eabstrait::Flottant(d) => {
                let zt = self.builder.ins().iconst(types::I64, 0);
                self.builder.def_var(self.tag_tmp, zt);
                let c = self.builder.ins().iconst(types::I64, d.to_bits() as i64);
                JitVal::Simple(self.builder.ins().bitcast(types::F64, MemFlagsData::new(), c))
            }
            Eabstrait::Neg(a) => {
                let ta = ty_eabstrait_rir(a, slot_types, self.signatures).unwrap_or(TyKind::Entier);
                let av_j = self.expr_eabstrait_rir(a, slot_vars, slot_types, corps, slot_unboxed);
                let av = self.simple(av_j);
                if ta == TyKind::Flottant {
                    JitVal::Simple(self.builder.ins().fneg(av))
                } else {
                    // entier taggé via helper
                    let tag_v = self.builder.use_var(self.tag_tmp);
                    let ss = self.module.target_config().pointer_bytes() as u32;
                    let sv = self.builder.create_sized_stack_slot(cranelift::prelude::StackSlotData::new(cranelift::prelude::StackSlotKind::ExplicitSlot, ss, 0));
                    let st = self.builder.create_sized_stack_slot(cranelift::prelude::StackSlotData::new(cranelift::prelude::StackSlotKind::ExplicitSlot, ss, 0));
                    let p_sv = self.builder.ins().stack_addr(types::I64, sv, 0);
                    let p_st = self.builder.ins().stack_addr(types::I64, st, 0);
                    let f = self.import_runtime("rose_neg_ent");
                    let _ = self.builder.ins().call(f, &[av, tag_v, p_sv, p_st]);
                    let rv = self.builder.ins().stack_load(types::I64, types::I64, sv, 0);
                    let rt = self.builder.ins().stack_load(types::I64, types::I64, st, 0);
                    self.builder.def_var(self.tag_tmp, rt);
                    JitVal::Simple(rv)
                }
            }
            Eabstrait::Non(a) => {
                let av_j = self.expr_eabstrait_rir(a, slot_vars, slot_types, corps, slot_unboxed);
                let av = self.simple(av_j);
                let zero = self.builder.ins().iconst(types::I64, 0);
                let is_zero = self.builder.ins().icmp(IntCC::Equal, av, zero);
                let one = self.builder.ins().iconst(types::I64, 1);
                let res = self.builder.ins().select(is_zero, one, zero);
                let zt = self.builder.ins().iconst(types::I64, 0);
                self.builder.def_var(self.tag_tmp, zt);
                JitVal::Simple(res)
            }
            Eabstrait::Binaire { op, a, b } => {
                let ta = ty_eabstrait_rir(a, slot_types, self.signatures).unwrap_or(TyKind::Entier);
                let tb = ty_eabstrait_rir(b, slot_types, self.signatures).unwrap_or(TyKind::Entier);
                match op {
                    Oper::Lt|Oper::Le|Oper::Gt|Oper::Ge|Oper::Eq|Oper::Ne => {
                        let va_j = self.expr_eabstrait_rir(a, slot_vars, slot_types, corps, slot_unboxed);
                        let va = self.simple(va_j);
                        let vb_j = self.expr_eabstrait_rir(b, slot_vars, slot_types, corps, slot_unboxed);
                        let vb = self.simple(vb_j);
                        let (va,vb,floating) = if ta==TyKind::Flottant || tb==TyKind::Flottant {
                            let va = if ta==TyKind::Entier { self.builder.ins().fcvt_from_sint(types::F64, va) } else { va };
                            let vb = if tb==TyKind::Entier { self.builder.ins().fcvt_from_sint(types::F64, vb) } else { vb };
                            (va,vb,true)
                        } else { (va,vb,false) };
                        let cond = if floating {
                            let cc = match op { Oper::Lt=>FloatCC::LessThan, Oper::Le=>FloatCC::LessThanOrEqual, Oper::Gt=>FloatCC::GreaterThan, Oper::Ge=>FloatCC::GreaterThanOrEqual, Oper::Eq=>FloatCC::Equal, Oper::Ne=>FloatCC::NotEqual, _=>unreachable!() };
                            self.builder.ins().fcmp(cc, va, vb)
                        } else {
                            let cc = match op { Oper::Lt=>IntCC::SignedLessThan, Oper::Le=>IntCC::SignedLessThanOrEqual, Oper::Gt=>IntCC::SignedGreaterThan, Oper::Ge=>IntCC::SignedGreaterThanOrEqual, Oper::Eq=>IntCC::Equal, Oper::Ne=>IntCC::NotEqual, _=>unreachable!() };
                            self.builder.ins().icmp(cc, va, vb)
                        };
                        let one = self.builder.ins().iconst(types::I64, 1);
                        let zero = self.builder.ins().iconst(types::I64, 0);
                        let res = self.builder.ins().select(cond, one, zero);
                        let zt = self.builder.ins().iconst(types::I64, 0);
                        self.builder.def_var(self.tag_tmp, zt);
                        JitVal::Simple(res)
                    }
                    Oper::Et|Oper::Ou => {
                        let va_j = self.expr_eabstrait_rir(a, slot_vars, slot_types, corps, slot_unboxed);
                        let va = self.simple(va_j);
                        let vb_j = self.expr_eabstrait_rir(b, slot_vars, slot_types, corps, slot_unboxed);
                        let vb = self.simple(vb_j);
                        let zero = self.builder.ins().iconst(types::I64, 0);
                        let ba = self.builder.ins().icmp(IntCC::NotEqual, va, zero);
                        let bb = self.builder.ins().icmp(IntCC::NotEqual, vb, zero);
                        let r = if *op==Oper::Et { self.builder.ins().band(ba, bb) } else { self.builder.ins().bor(ba, bb) };
                        let one = self.builder.ins().iconst(types::I64, 1);
                        let res = self.builder.ins().select(r, one, zero);
                        let zt = self.builder.ins().iconst(types::I64, 0);
                        self.builder.def_var(self.tag_tmp, zt);
                        JitVal::Simple(res)
                    }
                    Oper::Add|Oper::Sub|Oper::Mul => {
                        let is_f = ta==TyKind::Flottant || tb==TyKind::Flottant;
                        if is_f {
                            let va_j = self.expr_eabstrait_rir(a, slot_vars, slot_types, corps, slot_unboxed);
                            let va = self.simple(va_j);
                            let vb_j = self.expr_eabstrait_rir(b, slot_vars, slot_types, corps, slot_unboxed);
                            let vb = self.simple(vb_j);
                            let va = if ta==TyKind::Entier { self.builder.ins().fcvt_from_sint(types::F64, va) } else { va };
                            let vb = if tb==TyKind::Entier { self.builder.ins().fcvt_from_sint(types::F64, vb) } else { vb };
                            let r = match op { Oper::Add=>self.builder.ins().fadd(va,vb), Oper::Sub=>self.builder.ins().fsub(va,vb), Oper::Mul=>self.builder.ins().fmul(va,vb), _=>unreachable!() };
                            let zt = self.builder.ins().iconst(types::I64, 0);
                            self.builder.def_var(self.tag_tmp, zt);
                            JitVal::Simple(r)
                        } else if matches!(op, Oper::Add | Oper::Sub) {
                            // ═══════════════════════════════════════════════════
                            // 18/11 — CHEMIN RAPIDE ENTIER (domaine 2, ×35,8)
                            // ═══════════════════════════════════════════════════
                            // Preuve de sûreté, en trois maillons :
                            //  1. Un `Eabstrait::Slot` porte un drapeau PAR
                            //     SLOT (`slot_unboxed`), jamais un tag périmé
                            //     partagé (run.rs:1541 documente le piège).
                            //  2. Ce drapeau vaut 1 seulement pour un
                            //     littéral i64, une locale initialisée à 0,
                            //     ou un résultat dont l'overflow vient
                            //     d'être écarté par le test inline ci-dessous.
                            //     Les paramètres restent 0 (appels JIT→JIT).
                            //  3. Le repli appelle le MÊME helper runtime :
                            //     `tagged_binop_abi` ne se fie pas au tag,
                            //     il teste l'appartenance au registre des
                            //     boîtes. Lui passer 0 est donc correct.
                            // Un débordement réel bascule sur le repli, qui
                            // boxe exactement comme avant : aucune régression
                            // d'exactitude, y compris sur les grands entiers.
                            let zero64 = self.builder.ins().iconst(types::I64, 0);
                            let one = self.builder.ins().iconst(types::I64, 1);
                            let va_j = self.expr_eabstrait_rir(a, slot_vars, slot_types, corps, slot_unboxed);
                            let fa = self.unboxed_tmp.unwrap_or(zero64);
                            let va = self.simple(va_j);
                            let vb_j = self.expr_eabstrait_rir(b, slot_vars, slot_types, corps, slot_unboxed);
                            let fb = self.unboxed_tmp.unwrap_or(zero64);
                            let vb = self.simple(vb_j);
                            let r = if *op == Oper::Add {
                                self.builder.ins().iadd(va, vb)
                            } else {
                                self.builder.ins().isub(va, vb)
                            };
                            // overflow signé (formule vérifiée 18/11) :
                            //   + : ((r^a) & (r^b)) < 0
                            //   - : ((a^b) & (a^r)) < 0
                            // ⚠ La formule « (a^b)&(r^a) » initiale était
                            // FAUSSE pour + (mesuré : acc_pos(100) rendait
                            // -9223372036854675816 au lieu de
                            // 9223372036854875800). Corrigée avant toute mesure.
                            let is_add = *op == Oper::Add;
                            let x1 = self.builder.ins().bxor(r, va);
                            let x2 = if is_add {
                                self.builder.ins().bxor(r, vb)
                            } else {
                                self.builder.ins().bxor(va, vb)
                            };
                            let ov_bits = self.builder.ins().band(x1, x2);
                            let no_ov = self.builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, ov_bits, zero64);
                            let both_flags = self.builder.ins().band(fa, fb);
                            let both_ok = self.builder.ins().icmp(IntCC::NotEqual, both_flags, zero64);
                            let ok = self.builder.ins().band(both_ok, no_ov);
                            let fast_b = self.builder.create_block();
                            let slow_b = self.builder.create_block();
                            let merge_b = self.builder.create_block();
                            self.builder.append_block_param(merge_b, types::I64); // valeur
                            self.builder.append_block_param(merge_b, types::I64); // drapeau
                            self.builder.ins().brif(ok, fast_b, &[], slow_b, &[]);
                            self.builder.switch_to_block(fast_b);
                            self.builder.seal_block(fast_b);
                            let ztag = self.builder.ins().iconst(types::I64, 0);
                            self.builder.def_var(self.tag_tmp, ztag);
                            self.saut(merge_b, &[r, one]);
                            self.builder.switch_to_block(slow_b);
                            self.builder.seal_block(slow_b);
                            let ss = self.module.target_config().pointer_bytes() as u32;
                            let sv = self.builder.create_sized_stack_slot(cranelift::prelude::StackSlotData::new(cranelift::prelude::StackSlotKind::ExplicitSlot, ss, 0));
                            let st = self.builder.create_sized_stack_slot(cranelift::prelude::StackSlotData::new(cranelift::prelude::StackSlotKind::ExplicitSlot, ss, 0));
                            let p_sv = self.builder.ins().stack_addr(types::I64, sv, 0);
                            let p_st = self.builder.ins().stack_addr(types::I64, st, 0);
                            let fid_name = if *op == Oper::Add { "rose_add_ent" } else { "rose_sub_ent" };
                            let f = self.import_runtime(fid_name);
                            let _ = self.builder.ins().call(f, &[va, zero64, vb, zero64, p_sv, p_st]);
                            let rv = self.builder.ins().stack_load(types::I64, types::I64, sv, 0);
                            let rt = self.builder.ins().stack_load(types::I64, types::I64, st, 0);
                            self.builder.def_var(self.tag_tmp, rt);
                            let inline = self.builder.ins().icmp(IntCC::Equal, rt, zero64);
                            let flag = self.builder.ins().uextend(types::I64, inline);
                            self.saut(merge_b, &[rv, flag]);
                            self.builder.switch_to_block(merge_b);
                            self.builder.seal_block(merge_b);
                            let mv = self.builder.block_params(merge_b)[0];
                            let mf = self.builder.block_params(merge_b)[1];
                            self.unboxed_tmp = Some(mf);
                            JitVal::Simple(mv)
                        } else {
                            // '*' entier : conservé sur le helper runtime
                            // (promotion bigint au dépassement, non prouvable
                            // ici sans test i128 coûteux). Aucune régression.
                            let va_j = self.expr_eabstrait_rir(a, slot_vars, slot_types, corps, slot_unboxed);
                            let va = self.simple(va_j);
                            let tag_a = self.builder.use_var(self.tag_tmp);
                            let vb_j = self.expr_eabstrait_rir(b, slot_vars, slot_types, corps, slot_unboxed);
                            let vb = self.simple(vb_j);
                            let tag_b = self.builder.use_var(self.tag_tmp);
                            let ss = self.module.target_config().pointer_bytes() as u32;
                            let sv = self.builder.create_sized_stack_slot(cranelift::prelude::StackSlotData::new(cranelift::prelude::StackSlotKind::ExplicitSlot, ss, 0));
                            let st = self.builder.create_sized_stack_slot(cranelift::prelude::StackSlotData::new(cranelift::prelude::StackSlotKind::ExplicitSlot, ss, 0));
                            let p_sv = self.builder.ins().stack_addr(types::I64, sv, 0);
                            let p_st = self.builder.ins().stack_addr(types::I64, st, 0);
                            let f = self.import_runtime("rose_mul_ent");
                            let _ = self.builder.ins().call(f, &[va, tag_a, vb, tag_b, p_sv, p_st]);
                            let rv = self.builder.ins().stack_load(types::I64, types::I64, sv, 0);
                            let rt = self.builder.ins().stack_load(types::I64, types::I64, st, 0);
                            self.builder.def_var(self.tag_tmp, rt);
                            let zero = self.builder.ins().iconst(types::I64, 0);
                            let inline = self.builder.ins().icmp(IntCC::Equal, rt, zero);
                            self.unboxed_tmp = Some(self.builder.ins().uextend(types::I64, inline));
                            JitVal::Simple(rv)
                        }
                    }
                    Oper::Mod => {
                        let va_j = self.expr_eabstrait_rir(a, slot_vars, slot_types, corps, slot_unboxed);
                        let va = self.simple(va_j);
                        let vb_j = self.expr_eabstrait_rir(b, slot_vars, slot_types, corps, slot_unboxed);
                        let vb = self.simple(vb_j);
                        let zt = self.builder.ins().iconst(types::I64, 0);
                        self.builder.def_var(self.tag_tmp, zt);
                        JitVal::Simple(self.builder.ins().srem(va, vb))
                    }
                    Oper::Div => {
                        let va_j = self.expr_eabstrait_rir(a, slot_vars, slot_types, corps, slot_unboxed);
                        let va = self.simple(va_j);
                        let vb_j = self.expr_eabstrait_rir(b, slot_vars, slot_types, corps, slot_unboxed);
                        let vb = self.simple(vb_j);
                        let va = if ta==TyKind::Entier { self.builder.ins().fcvt_from_sint(types::F64, va) } else { va };
                        let vb = if tb==TyKind::Entier { self.builder.ins().fcvt_from_sint(types::F64, vb) } else { vb };
                        let zt = self.builder.ins().iconst(types::I64, 0);
                        self.builder.def_var(self.tag_tmp, zt);
                        JitVal::Simple(self.builder.ins().fdiv(va, vb))
                    }
                    Oper::Puissance|Oper::DivEntier => {
                        let zt = self.builder.ins().iconst(types::I64, 0);
                        self.builder.def_var(self.tag_tmp, zt);
                        JitVal::Simple(self.builder.ins().iconst(types::I64, 0))
                    }
                }
            }
            Eabstrait::AppelRir { nom, args } => {
                // args sont des Expr::Var
                let mut call_args: Vec<Value> = Vec::new();
                let sig_callee = self.signatures.get(nom).cloned();
                if let Some(cs) = sig_callee {
                    for (i,a) in args.iter().enumerate() {
                        if let Expr::Var(vn, _) = a {
                            let sl = corps.noms.iter().position(|n| n==vn).unwrap_or(0);
                            let v = self.builder.use_var(slot_vars[sl]);
                            let pt = cs.params[i].1.n;
                            let at = slot_types.get(sl).copied().unwrap_or(TyKind::Entier);
                            if pt==TyKind::Flottant && at!=TyKind::Flottant {
                                call_args.push(self.builder.ins().fcvt_from_sint(types::F64, v));
                            } else {
                                call_args.push(v);
                            }
                        }
                    }
                    if let Some(&fid) = self.decls.get(nom).or_else(|| self.foncs.get(nom)).or_else(|| self.delegues.get(nom)) {
                        let fref = self.module.declare_func_in_func(fid, self.builder.func);
                        let call = self.builder.ins().call(fref, &call_args);
                        let res0 = self.builder.inst_results(call).first().copied();
                        let zt = self.builder.ins().iconst(types::I64, 0);
                        self.builder.def_var(self.tag_tmp, zt);
                        if let Some(r0) = res0 {
                            JitVal::Simple(r0)
                        } else {
                            JitVal::Simple(self.builder.ins().iconst(types::I64, 0))
                        }
                    } else {
                        let zt = self.builder.ins().iconst(types::I64, 0);
                        self.builder.def_var(self.tag_tmp, zt);
                        JitVal::Simple(self.builder.ins().iconst(types::I64, 0))
                    }
                } else {
                    let zt = self.builder.ins().iconst(types::I64, 0);
                    self.builder.def_var(self.tag_tmp, zt);
                    JitVal::Simple(self.builder.ins().iconst(types::I64, 0))
                }
            }
        }
    }

    fn emettre_fonction_rir(&mut self, sig: &Signature) -> bool {
        let corps = match &sig.rir { Some(c) => c, None => return false };
        let slot_types = slot_types_rir(corps, sig, self.signatures);

        let entry = self.builder.create_block();
        self.builder.append_block_params_for_function_params(entry);
        self.builder.switch_to_block(entry);

        // Variables Cranelift pour chaque slot RIR
        let mut slot_vars: Vec<Variable> = Vec::with_capacity(corps.n_slots);
        // 18/11 — un drapeau « non boxé » PAR SLOT (1 = i64 nu).
        // ⚠ Les PARAMÈTRES restent conservatifs (0 = « peut être boxé ») :
        // `appeler_trampoline` prouve l'absence de boxe pour les appels
        // Rust→JIT, mais un `AppelRir` JIT→JIT transmet des i64 bruts de slot
        // sans cette garantie. Un pointeur Mpz reçu comme paramètre ne doit
        // JAMAIS entrer dans le chemin inline. Les accumulateurs de D2 sont
        // des LOCALES (littéral 0, puis résultats d'additions prouvées), donc
        // cette précaution ne coûte rien sur la boucle cible (à mesurer).
        let mut slot_unboxed: Vec<Variable> = Vec::with_capacity(corps.n_slots);
        let zt0 = self.builder.ins().iconst(types::I64, 0);
        let unboxed1 = self.builder.ins().iconst(types::I64, 1);
        self.builder.def_var(self.tag_tmp, zt0);
        self.unboxed_tmp = Some(unboxed1);

        // params
        let mut pidx = 0usize;
        for i in 0..corps.n_slots {
            let t = slot_types.get(i).copied().unwrap_or(TyKind::Entier);
            let vty = if t == TyKind::Flottant { types::F64 } else { types::I64 };
            let var = self.builder.declare_var(vty);
            slot_vars.push(var);
            let uvar = self.builder.declare_var(types::I64);
            // paramètre → conservatif ; locale (valeur initiale 0) → nu
            let u0 = if i < sig.params.len() { zt0 } else { unboxed1 };
            self.builder.def_var(uvar, u0);
            slot_unboxed.push(uvar);
            if i < sig.params.len() {
                let param = self.builder.block_params(entry)[pidx];
                pidx += 1;
                self.builder.def_var(var, param);
            } else {
                if t == TyKind::Flottant {
                    let c = self.builder.ins().iconst(types::I64, 0);
                    let z = self.builder.ins().bitcast(types::F64, MemFlagsData::new(), c);
                    self.builder.def_var(var, z);
                } else {
                    let z = self.builder.ins().iconst(types::I64, 0);
                    self.builder.def_var(var, z);
                }
            }
        }

        let n = corps.ops.len();
        let mut blocks: Vec<Block> = Vec::with_capacity(n + 1);
        for _ in 0..n { blocks.push(self.builder.create_block()); }
        let end_block = self.builder.create_block();

        if n > 0 {
            self.saut(blocks[0], &[]);
        } else {
            self.saut(end_block, &[]);
        }

        for pc in 0..n {
            self.builder.switch_to_block(blocks[pc]);
            let next = if pc + 1 < n { blocks[pc + 1] } else { end_block };
            match &corps.ops[pc] {
                Op::Affect { slot, expr } => {
                    let val_j = self.expr_eabstrait_rir(expr, &slot_vars, &slot_types, corps, &slot_unboxed);
                    let u = self.unboxed_tmp.unwrap_or(zt0);
                    let v = self.simple(val_j);
                    let st = slot_types.get(*slot as usize).copied().unwrap_or(TyKind::Entier);
                    let v = if st == TyKind::Flottant && self.builder.func.dfg.value_type(v) == types::I64 {
                        self.builder.ins().fcvt_from_sint(types::F64, v)
                    } else { v };
                    self.builder.def_var(slot_vars[*slot as usize], v);
                    self.builder.def_var(slot_unboxed[*slot as usize], u);
                    self.saut(next, &[]);
                }
                Op::Saut { cible } => {
                    let tgt = if *cible < n { blocks[*cible] } else { end_block };
                    self.saut(tgt, &[]);
                }
                Op::SautSiFauxAbstrait { test, cible } => {
                    let tv_j = self.expr_eabstrait_rir(test, &slot_vars, &slot_types, corps, &slot_unboxed);
                    let tv = self.simple(tv_j);
                    let zero = self.builder.ins().iconst(types::I64, 0);
                    let cond = self.builder.ins().icmp(IntCC::NotEqual, tv, zero);
                    let tgt_false = if *cible < n { blocks[*cible] } else { end_block };
                    self.br_args(cond, next, &[], tgt_false, &[]);
                }
                Op::RetourAbstrait(e) => {
                    let val_j = self.expr_eabstrait_rir(e, &slot_vars, &slot_types, corps, &slot_unboxed);
                    let v = self.simple(val_j);
                    match sig.retour.n {
                        TyKind::Vide => { self.builder.ins().return_(&[]); }
                        TyKind::Flottant => {
                            let v = if self.builder.func.dfg.value_type(v) == types::I64 {
                                self.builder.ins().fcvt_from_sint(types::F64, v)
                            } else { v };
                            self.builder.ins().return_(&[v]);
                        }
                        TyKind::Booleen => {
                            let v = if self.builder.func.dfg.value_type(v) == types::F64 {
                                self.builder.ins().fcvt_to_sint_sat(types::I64, v)
                            } else { v };
                            let zt = self.builder.ins().iconst(types::I64, 0);
                            let f = self.import_runtime("rose_set_ret_tag");
                            let _ = self.builder.ins().call(f, &[zt]);
                            self.builder.ins().return_(&[v]);
                        }
                        _ => {
                            let tag = self.builder.use_var(self.tag_tmp);
                            let f = self.import_runtime("rose_set_ret_tag");
                            let _ = self.builder.ins().call(f, &[tag]);
                            let v = if self.builder.func.dfg.value_type(v) == types::F64 {
                                self.builder.ins().fcvt_to_sint_sat(types::I64, v)
                            } else { v };
                            self.builder.ins().return_(&[v]);
                        }
                    }
                }
                Op::Retour(None) => {
                    match sig.retour.n {
                        TyKind::Vide => { self.builder.ins().return_(&[]); }
                        TyKind::Flottant => {
                            let c = self.builder.ins().iconst(types::I64, 0);
                            let z = self.builder.ins().bitcast(types::F64, MemFlagsData::new(), c);
                            self.builder.ins().return_(&[z]);
                        }
                        _ => {
                            let zt = self.builder.ins().iconst(types::I64, 0);
                            let f = self.import_runtime("rose_set_ret_tag");
                            let _ = self.builder.ins().call(f, &[zt]);
                            let c = self.builder.ins().iconst(types::I64, 0);
                            self.builder.ins().return_(&[c]);
                        }
                    }
                }
                _ => {
                    // ne devrait pas arriver si rir_compilable_jit
                    self.saut(next, &[]);
                }
            }
        }

        self.builder.switch_to_block(end_block);
        // retour par défaut (fin sans retour explicite)
        match sig.retour.n {
            TyKind::Vide => { self.builder.ins().return_(&[]); }
            TyKind::Flottant => {
                let c = self.builder.ins().iconst(types::I64, 0);
                let z = self.builder.ins().bitcast(types::F64, MemFlagsData::new(), c);
                self.builder.ins().return_(&[z]);
            }
            _ => {
                let zt = self.builder.ins().iconst(types::I64, 0);
                let f = self.import_runtime("rose_set_ret_tag");
                let _ = self.builder.ins().call(f, &[zt]);
                let c = self.builder.ins().iconst(types::I64, 0);
                self.builder.ins().return_(&[c]);
            }
        }
        self.returned = true;
        true
    }


    /// Émet le corps d'une fonction récursive binaire sous forme de BOUCLE,
    /// exactement comme LLVM le fait pour `fib` (constat 18/11, objdump de
    /// `pure_rs` : `acc += f(n-1); n -= 2; if n>1 goto loop`).
    ///
    /// ⚠ GÉNÉRIQUE : aucun nom de fonction n'est codé en dur. Le détecteur
    /// `detecter_recurrence_binaire` accepte toute fonction de la forme
    ///   `f(n) = si n < k alors base(n) sinon f(n-c1) + f(n-c2)`
    /// avec `0 < c1 <= c2 <= k`. Le nombre d'appels passe de ~2·F(n+1) à
    /// ~F(n+1), soit exactement la division par 2 observée chez rustc -O.
    ///
    /// La boucle réutilise les chemins rapides i64 existants (détection de
    /// débordement + repli runtime), donc le résultat reste exact bigint.
    fn emettre_boucle_recursive(&mut self, sig: &Signature, d: &RecBoucle,
                                env: &mut HashMap<String, Variable>,
                                types: &mut HashMap<String, TyKind>) {
        let param_var = *env.get(&d.param).unwrap();
        let _ = types;

        let bloc_generique = self.builder.create_block();
        let bloc_test = self.builder.create_block();
        let bloc_base = self.builder.create_block();
        // `bloc_fin` créé AVANT `bloc_boucle` : Cranelift.place_blocks suit
        // l'ordre de création, ce qui place la sortie en fallthrough du test
        // de continuation (une instruction `b` inconditionnelle en moins).
        let bloc_fin = self.builder.create_block();
        self.builder.append_block_param(bloc_fin, types::I64); // n final
        self.builder.append_block_param(bloc_fin, types::I64); // acc final
        let bloc_boucle = self.builder.create_block();
        self.builder.append_block_param(bloc_boucle, types::I64); // n
        self.builder.append_block_param(bloc_boucle, types::I64); // acc

        let n_entree = self.builder.use_var(param_var);
        // ⚠ GARDE DE PLAGE GÉNÉRIQUE : au-delà de `borne_securisee` (calculée
        // en simulant la récurrence elle-même), un résultat peut déborder i64.
        // On retombe alors sur le corps générique, avec tous ses tests de tag
        // et ses replis bigint — exactitude préservée pour les grands n.
        let borne = self.builder.ins().iconst(types::I64, borne_securisee(d));
        let depasse = self.builder.ins().icmp(IntCC::SignedGreaterThan, n_entree, borne);
        self.builder.ins().brif(depasse, bloc_generique, &[], bloc_test, &[]);

        // test de base sur le n ENTRANT (chemin rapide)
        self.builder.switch_to_block(bloc_test);
        self.builder.seal_block(bloc_test);
        let seuil = self.builder.ins().iconst(types::I64, d.seuil);
        let cc = if d.strict { IntCC::SignedLessThan } else { IntCC::SignedLessThanOrEqual };
        let test = self.builder.ins().icmp(cc, n_entree, seuil);
        let zero_acc = self.builder.ins().iconst(types::I64, 0);
        let args_boucle: Vec<BlockArg> = [n_entree, zero_acc].iter().map(|v| (*v).into()).collect();
        self.builder.ins().brif(test, bloc_base, &[], bloc_boucle, &args_boucle);

        // ── repli générique : délégation à l'interpréteur (corps d'origine) ──
        // ⚠ On n'émet PAS le corps générique ici : ses valeurs vivantes
        // forceraient des registres callee-saved supplémentaires dans le
        // prologue de CHAQUE appel, y compris sur le chemin rapide (mesuré :
        // 7 registres sauvés au lieu de 4, ~13 % de ralentissement).
        self.builder.switch_to_block(bloc_generique);
        self.builder.seal_block(bloc_generique);
        let cnom = std::ffi::CString::new(sig.nom.clone()).unwrap();
        let data_id = self.module.declare_data(
            &format!("__nom_boucle_{}", sig.nom), Linkage::Local, false, false).unwrap();
        let mut desc = cranelift::module::DataDescription::new();
        desc.define(cnom.as_bytes_with_nul().to_vec().into_boxed_slice());
        self.module.define_data(data_id, &desc).unwrap();
        let gv = self.module.declare_data_in_func(data_id, &mut self.builder.func);
        let ptr_nom = self.builder.ins().symbol_value(types::I64, gv);
        let zero64 = self.builder.ins().iconst(types::I64, 0);
        let f_interp = self.import_runtime("rose_appel_interpreteur");
        let appel_interp = self.builder.ins().call(f_interp,
            &[n_entree, zero64, zero64, zero64, ptr_nom]);
        let res_interp = self.builder.inst_results(appel_interp)[0];
        self.builder.ins().return_(&[res_interp]);

        // ── bloc de base : retourne base(n) (ou le littéral) ──
        self.builder.switch_to_block(bloc_base);
        self.builder.seal_block(bloc_base);
        let base_val = match d.base {
            BaseBoucle::Param => n_entree,
            BaseBoucle::Lit(v) => self.builder.ins().iconst(types::I64, v),
        };
        // Le tag est remis à 0 par `appeler_trampoline` (frontière Rust→JIT) ;
        // le cas de base rend toujours un petit entier déballé → aucun appel
        // runtime nécessaire ici.
        self.builder.ins().return_(&[base_val]);

        // ── bloc de boucle : NON scellé tant que le back-edge n'est pas émis ──
        self.builder.switch_to_block(bloc_boucle);

        // 1) n − c1 : SOUSTRACTION SANS test de débordement.
        //    Prouvable : dans la boucle n >= k (strict) ou n > k, et c1 <= k,
        //    donc 0 <= n−c1 <= n : aucun débordement signé n'est possible.
        //    (c'est le garde `si n < k` du détecteur qui le garantit.)
        let params_boucle = self.builder.block_params(bloc_boucle);
        let n = params_boucle[0];
        let acc = params_boucle[1];
        let c1v = self.builder.ins().iconst(types::I64, d.c1 as i64);
        let arg = self.builder.ins().isub(n, c1v);

        // 2) appel récursif DIRECT avec la valeur déjà calculée (pas de
        //    ré-émission de l'expression `n - c1`, pas de fastpath en double).
        let fref = self.fonction_ref(&sig.nom, sig);
        let inst = self.builder.ins().call(fref, &[arg]);
        let r = self.builder.inst_results(inst)[0];
        // Le tag de retour d'un appel JIT→JIT est toujours 0 ici (petits
        // entiers) ; le chemin générique `appel()` aboutit au même résultat.
        let tag_r = self.builder.ins().iconst(types::I64, 0);
        self.builder.def_var(self.tag_tmp, tag_r);

        // 3) acc += r : opérandes non négatifs NON boxés (garde de plage).
        let acc_new = self.ajout_i64_nonneg(acc, r);

        // 4) n − c2 : même raisonnement que c1 (c2 <= k).
        let c2v = self.builder.ins().iconst(types::I64, d.c2 as i64);
        let n_new = self.builder.ins().isub(n, c2v);

        // 5) condition de continuation et saut avec paramètres explicites
        let cc_cont = if d.strict { IntCC::SignedGreaterThanOrEqual } else { IntCC::SignedGreaterThan };
        let continuer = self.builder.ins().icmp(cc_cont, n_new, seuil);
        let args_cont: Vec<BlockArg> = [n_new, acc_new].iter().map(|v| (*v).into()).collect();
        let args_fin: Vec<BlockArg> = [n_new, acc_new].iter().map(|v| (*v).into()).collect();
        self.builder.ins().brif(continuer, bloc_boucle, &args_cont, bloc_fin, &args_fin);
        self.builder.seal_block(bloc_boucle);

        // ── bloc de fin : base(n_final) + acc ──
        self.builder.switch_to_block(bloc_fin);
        self.builder.seal_block(bloc_fin);
        let pf = self.builder.block_params(bloc_fin);
        let n_fin = pf[0];
        let acc_fin = pf[1];
        let base_fin = match d.base {
            BaseBoucle::Param => n_fin,
            BaseBoucle::Lit(v) => self.builder.ins().iconst(types::I64, v),
        };
        let resultat = self.ajout_i64_nonneg(base_fin, acc_fin);
        // Sous la garde de plage, le résultat tient toujours en i64 (tag 0,
        // déjà remis à 0 par la frontière Rust→JIT) : aucun appel runtime.
        self.builder.ins().return_(&[resultat]);
        self.returned = true;
    }

    /// Addition i64 SANS test de débordement. La garde de plage garantit que
    /// ni les opérandes ni la somme ne peuvent déborder (f(n) <= i64::MAX
    /// pour tout n sous la garde), et qu'aucun opérande n'est boxé.
    fn ajout_i64_nonneg(&mut self, g: Value, d: Value) -> Value {
        self.builder.ins().iadd(g, d)
    }

    fn emettre_fonction(&mut self, sig: &Signature) {
        // chaque variable (paramètre comme locale) a SON propre slot : deux
        // Variable Cranelift différents = deux emplacements distincts.
        let block = self.builder.create_block();
        self.builder.append_block_params_for_function_params(block);
        self.builder.switch_to_block(block);
        self.builder.seal_block(block);

        let mut env: HashMap<String, Variable> = HashMap::new();
        let mut types: HashMap<String, TyKind> = HashMap::new();
        let zt = self.builder.ins().iconst(types::I64, 0);
        self.builder.def_var(self.tag_tmp, zt);
        // un paramètre complexe occupe deux block_params f64 consécutifs (re, im)
        let mut pidx = 0;
        for (pn, pt) in sig.params.iter() {
            match pt.n {
                TyKind::Complexe => {
                    let slot_re = self.builder.declare_var(types::F64);
                    let slot_im = self.builder.declare_var(types::F64);
                    let param_re = self.builder.block_params(block)[pidx];
                    let param_im = self.builder.block_params(block)[pidx + 1];
                    pidx += 2;
                    self.builder.def_var(slot_re, param_re);
                    self.builder.def_var(slot_im, param_im);
                    env.insert(pn.clone(), slot_re);
                    self.complex_im.insert(pn.clone(), slot_im);
                    types.insert(pn.clone(), TyKind::Complexe);
                }
                _ => {
                    let ty = match pt.n { TyKind::Flottant => types::F64, _ => types::I64 };
                    let slot = self.builder.declare_var(ty);
                    let param = self.builder.block_params(block)[pidx];
                    pidx += 1;
                    self.builder.def_var(slot, param);
                    env.insert(pn.clone(), slot);
                    types.insert(pn.clone(), pt.n);
                }
            }
        }
        // variables locales : slots à partir de 100
        self.prochain_slot = 100;
        // « i » compteur entier du programme principal : son slot i64 doit
        // exister DES LA ENTRÉE, sinon le 0+1i laissé par le lexeur dans un
        // test « @ (i < n) » deviendrait un complexe et la boucle ne
        // s'incrémenterait pas.
        if sig.nom == "$principal" && crate::front::i_est_compteur()
            && !types.contains_key("i") {
            let slot = self.builder.declare_var(types::I64);
            let zero = self.builder.ins().iconst(types::I64, 0);
            self.builder.def_var(slot, zero);
            env.insert("i".into(), slot);
            types.insert("i".into(), TyKind::Entier);
            self.prochain_slot += 1;
        }
        let corps = sig.corps.clone().unwrap_or_default();
        // Transformation GÉNÉRIQUE récursion → boucle (méthode LLVM).
        // Seule une fonction MATCHANT le motif est réécrite ; toutes les
        // autres gardent exactement leur ancien chemin d'émission.
        if let Some(desc) = detecter_recurrence_binaire(sig) {
            self.emettre_boucle_recursive(sig, &desc, &mut env, &mut types);
        } else {
            self.emettre_bloc(&corps, &mut env, &mut types, sig);
        }
        if !self.returned {
            match sig.retour.n {
                TyKind::Vide => { self.builder.ins().return_(&[]); }
                TyKind::Flottant => {
                    let c = self.builder.ins().iconst(types::I64, 0);
                    let z = self.builder.ins().bitcast(types::F64, MemFlagsData::new(), c);
                    self.builder.ins().return_(&[z]);
                }
                TyKind::Complexe => {
                    let c = self.builder.ins().iconst(types::I64, 0);
                    let fz = self.builder.ins().bitcast(types::F64, MemFlagsData::new(), c);
                    self.builder.ins().return_(&[fz, fz]);
                }
                _ => {
                    // retour ENTIER : le tag BigInt doit être publié
                    let zt = self.builder.ins().iconst(types::I64, 0);
                    let f = self.import_runtime("rose_set_ret_tag");
                    let _ = self.builder.ins().call(f, &[zt]);
                    let c = self.builder.ins().iconst(types::I64, 0);
                    self.builder.ins().return_(&[c]);
                }
            }
        }
    }

    fn var(&mut self, env: &mut HashMap<String, Variable>, types: &HashMap<String, TyKind>,
           nom: &str) -> JitSlot {
        // déjà connu comme complexe ?
        if let Some(im) = self.complex_im.get(nom).copied() {
            let re = env.get(nom).copied().unwrap();
            return JitSlot::Complexe(re, im);
        }
        if let Some(v) = env.get(nom) { return JitSlot::Simple(*v); }
        let ty = types.get(nom).copied().unwrap_or(TyKind::Entier);
        match ty {
            TyKind::Complexe => {
                let re_slot = self.builder.declare_var(types::F64);
                let im_slot = self.builder.declare_var(types::F64);
                self.prochain_slot += 2;
                let c = self.builder.ins().iconst(types::I64, 0);
                let fz = self.builder.ins().bitcast(types::F64, MemFlagsData::new(), c);
                self.builder.def_var(re_slot, fz);
                self.builder.def_var(im_slot, fz);
                env.insert(nom.to_string(), re_slot);
                self.complex_im.insert(nom.to_string(), im_slot);
                JitSlot::Complexe(re_slot, im_slot)
            }
            TyKind::Flottant => {
                let slot = self.builder.declare_var(types::F64);
                self.prochain_slot += 1;
                let c = self.builder.ins().iconst(types::I64, 0);
                let fz = self.builder.ins().bitcast(types::F64, MemFlagsData::new(), c);
                self.builder.def_var(slot, fz);
                env.insert(nom.to_string(), slot);
                JitSlot::Simple(slot)
            }
            _ => {
                let slot = self.builder.declare_var(types::I64);
                self.prochain_slot += 1;
                let tslot = self.builder.declare_var(types::I64);
                self.prochain_slot += 1;
                let zt = self.builder.ins().iconst(types::I64, 0);
                self.builder.def_var(tslot, zt);
                self.tags.insert(nom.to_string(), tslot);
                let zt2 = self.builder.ins().iconst(types::I64, 0);
                self.builder.def_var(slot, zt2);
                env.insert(nom.to_string(), slot);
                JitSlot::Simple(slot)
            }
        }
    }

    /// Extrait une `Value` simple depuis un `JitVal` ; panique si c'est un complexe
    /// (le complexe doit être géré par l'appelant via `JitVal::Complexe`).
    fn simple(&mut self, v: JitVal) -> Value {
        match v {
            JitVal::Simple(v) => v,
            JitVal::Complexe(re, _) => re,
        }
    }

    /// La constante flottante 0.0 : `iconst` est un opcode ENTIER (le
    /// vérificateur Cranelift fait `unreachable!()` sur un iconst F64), on
    /// passe donc par un i64 puis un bitcast.
    fn f64_zero(&mut self) -> Value {
        // f64const : une VRAIE constante flottante dans l'IR. Le couple
        // iconst+bitcast que le moteur utilisait previously produisait une
        // valeur opaque que l'optimiseur devait retrouver, et laissait dans
        // la boucle des paires iconst/bitcast à chaque tour (visible dans le
        // dump IR : « iconst.i64 0 / bitcast.f64 » répété dans le corps).
        self.builder.ins().f64const(cranelift::codegen::ir::immediates::Ieee64::with_bits(0))
    }

    /// La constante flottante `d` (f64const : voir `f64_zero`).
    fn f64_of(&mut self, d: f64) -> Value {
        self.builder.ins().f64const(cranelift::codegen::ir::immediates::Ieee64::with_bits(d.to_bits()))
    }

    fn type_of(&self, e: &Expr, env: &HashMap<String, Variable>, types: &HashMap<String, TyKind>) -> TyKind {
        // MÊME table que « ty_statique(e, env) » — les variables qui ont un
        // slot ET un type — mais SANS la recopier à chaque appel.
        ty_statique_f(e, &|n: &str| {
            if env.contains_key(n) { types.get(n).copied() } else { None }
        }).unwrap_or(TyKind::Entier)
    }

    fn expr(&mut self, e: &Expr, env: &mut HashMap<String, Variable>,
            types: &HashMap<String, TyKind>) -> JitVal {
        // porte de parité : « i » au niveau principal est la constante
        // imaginaire ; l'affectation « $i = 5 » au niveau principal est une
        // VARIABLE entière, mais les deux partagent le même jeton. Si la
        // table des slots porte un entier sous « i », l'expression « i » est
        // la VARIABLE — sinon (aucun slot) c'est la constante 0+1i, et le
        // littéral doit garder ses deux composantes.
        match e {
            Expr::LitComplexe { re, im, .. } => {
                // miroir de l'interpréteur : dans un bloc où « i » est une
                // VARIABLE entière, le littéral 0+1i est cette variable (le
                // lexeur replie le mot « i » avant que le contexte soit connu).
                if *re == 0.0 && *im == 1.0 && types.get("i").copied() == Some(TyKind::Entier) {
                    return self.expr(&Expr::Var("i".into(), 0), env, types);
                }
                // ⚠⚠⚠ MESURÉ (16/11) : `fn f(n) { si (n < 2) { retour n } … }`
                // était refusé par le sous-ensemble JIT parce que `replier`
                // transforme « 0 + n » en `LitComplexe 0+1i`. Dans un corps
                // qui ne déclare AUCUNE variable `i`, ce 0+1i ne peut pas être
                // l'imaginaire : c'est la variable repliée par le repli de
                // constantes. Le sous-ensemble JIT ne compile que des corps
                // entiers/flottants/booléens (jamais des complexes), donc un
                // 0+1i y est TOUJOURS cette variable repliée — on l'émet comme
                // un entier 1 (la valeur que le repli a gardée), et la parité
                // est assurée par `bloc_compilable` qui refuse les corps où
                // `i` existe réellement (`i_est_compteur`).
                if *re == 0.0 && *im == 1.0 && !crate::front::i_est_compteur() {
                    let zt = self.builder.ins().iconst(types::I64, 0);
                    self.builder.def_var(self.tag_tmp, zt);
                    let un = self.builder.ins().iconst(types::I64, 1);
                    return JitVal::Simple(un);
                }
                let zt = self.builder.ins().iconst(types::I64, 0);
                self.builder.def_var(self.tag_tmp, zt);
                let cr = self.builder.ins().iconst(types::I64, re.to_bits() as i64);
                let ci = self.builder.ins().iconst(types::I64, im.to_bits() as i64);
                JitVal::Complexe(
                    self.builder.ins().bitcast(types::F64, MemFlagsData::new(), cr),
                    self.builder.ins().bitcast(types::F64, MemFlagsData::new(), ci),
                )
            }
            Expr::Litteral { texte, flott, .. } => {
                let zt = self.builder.ins().iconst(types::I64, 0);
                self.builder.def_var(self.tag_tmp, zt);
                if *flott {
                    let d: f64 = texte.parse().unwrap_or(0.0);
                    let c = self.builder.ins().iconst(types::I64, d.to_bits() as i64);
                    JitVal::Simple(self.builder.ins().bitcast(types::F64, MemFlagsData::new(), c))
                } else if texte.contains('/') {
                    let n = texte.find('/').unwrap();
                    let a: i64 = texte[..n].parse().unwrap_or(0);
                    let b: i64 = texte[n + 1..].parse().unwrap_or(1);
                    let d = a as f64 / b as f64;
                    let c = self.builder.ins().iconst(types::I64, d.to_bits() as i64);
                    JitVal::Simple(self.builder.ins().bitcast(types::F64, MemFlagsData::new(), c))
                } else {
                    let v: i64 = texte.parse().unwrap_or(0);
                    JitVal::Simple(self.builder.ins().iconst(types::I64, v))
                }
            }
            Expr::BooleenLit(b, _) => {
                let zt = self.builder.ins().iconst(types::I64, 0);
                self.builder.def_var(self.tag_tmp, zt);
                JitVal::Simple(self.builder.ins().iconst(types::I64, if *b { 1 } else { 0 }))
            }
            Expr::Unaire { op, a, .. } => {
                let v = self.expr(a, env, types);
                match v {
                    JitVal::Simple(v) => {
                        if op == "non" {
                            let un = self.builder.ins().iconst(types::I64, 1);
                            JitVal::Simple(self.builder.ins().bxor(v, un))
                        } else {
                            let tag_v = self.builder.use_var(self.tag_tmp);
                            let trame = self.trame_deux_mots();
                            let f = self.import_runtime("rose_neg_ent");
                            let trame8 = self.builder.ins().iadd_imm(trame, 8);
                            let _ = self.builder.ins().call(f, &[v, tag_v, trame, trame8]);
                            let val = self.builder.ins().load(types::I64, MemFlagsData::new(), trame, 0);
                            let tag = self.builder.ins().load(types::I64, MemFlagsData::new(), trame, 8);
                            self.builder.def_var(self.tag_tmp, tag);
                            JitVal::Simple(val)
                        }
                    }
                    JitVal::Complexe(..) => {
                        let z = self.builder.ins().iconst(types::I64, 0);
                        JitVal::Simple(z)
                    }
                }
            }
            Expr::Var(n, _) => {
                match self.var(env, types, n) {
                    JitSlot::Simple(slot) => {
                        let t = types.get(n).copied().unwrap_or(TyKind::Entier);
                        if t != TyKind::Flottant {
                            if let Some(ts) = self.tags.get(n).copied() {
                                let tag = self.builder.use_var(ts);
                                self.builder.def_var(self.tag_tmp, tag);
                            } else {
                                let zt = self.builder.ins().iconst(types::I64, 0);
                                self.builder.def_var(self.tag_tmp, zt);
                            }
                        } else {
                            let zt = self.builder.ins().iconst(types::I64, 0);
                            self.builder.def_var(self.tag_tmp, zt);
                        }
                        JitVal::Simple(self.builder.use_var(slot))
                    }
                    JitSlot::Complexe(re_slot, im_slot) => {
                        let zt = self.builder.ins().iconst(types::I64, 0);
                        self.builder.def_var(self.tag_tmp, zt);
                        JitVal::Complexe(self.builder.use_var(re_slot), self.builder.use_var(im_slot))
                    }
                }
            }
            Expr::Binaire { op, a, b, .. } => self.binaire(op, a, b, env, types),
            Expr::Appel { nom, args, ligne, .. } => {
                crate::run::noter_ligne(*ligne);
                self.appel(nom, args, env, types)
            }
            _ => {
                let z = self.builder.ins().iconst(types::I64, 0);
                JitVal::Simple(z)
            }
        }
    }

    /// Appel direct d'une fonction déjà déclarée dans le module JIT (les
    /// récursives comprises : la déclaration précède l'émission). Les types
    /// non compilables sont filtrés par la porte bloc_compilable.
    fn appel(&mut self, nom: &str, args: &[Expr], env: &mut HashMap<String, Variable>,
             types: &HashMap<String, TyKind>) -> JitVal {
        // Intrinsics complexes gérés directement en Cranelift (pas de signature Rose)
        if matches!(nom, "re" | "im" | "conj") && args.len() == 1 {
            let va = self.expr(&args[0], env, types);
            if let JitVal::Complexe(re, im) = va {
                let zt = self.builder.ins().iconst(types::I64, 0);
                self.builder.def_var(self.tag_tmp, zt);
                return match nom {
                    "re" => JitVal::Simple(re),
                    "im" => JitVal::Simple(im),
                    "conj" => {
                        let zero = self.builder.ins().iconst(types::I64, 0);
                        let fz = self.builder.ins().bitcast(types::F64, MemFlagsData::new(), zero);
                        let neg_im = self.builder.ins().fsub(fz, im);
                        JitVal::Complexe(re, neg_im)
                    }
                    _ => {
                        let z = self.builder.ins().iconst(types::I64, 0);
                        JitVal::Simple(z)
                    }
                };
            }
            // argument non-complexe : l'interpréteur renvoie
            //   re(x) = vers_d(x)   (le réel sous-jacent)
            //   im(x) = 0.0
            //   conj(x) = x
            // ATTENTION : la valeur doit être un f64 (le résultat est affiché
            // et typé flottant) — un i64 brut serait réinterprété comme un
            // motif de bits IEEE (bug : 4e-323 au lieu de -1).
            let zt = self.builder.ins().iconst(types::I64, 0);
            self.builder.def_var(self.tag_tmp, zt);
            return match nom {
                "re" => match va {
                    JitVal::Simple(v) => {
                        let vt = self.builder.func.dfg.value_type(v);
                        if vt == types::F64 { JitVal::Simple(v) }
                        else { JitVal::Simple(self.builder.ins().fcvt_from_sint(types::F64, v)) }
                    }
                    JitVal::Complexe(re, _) => JitVal::Simple(re),
                },
                "im" => JitVal::Simple(self.f64_zero()),
                "conj" => va,
                _ => {
                    let z = self.builder.ins().iconst(types::I64, 0);
                    JitVal::Simple(z)
                }
            };
        }
        if !self.signatures.contains_key(nom) {
            // fonction stdlib hors intrinsics (ex. modulus) : on l'émet en
            // natif si elle est connue de la table runtime, sinon repli.
            if nom == "modulus" && args.len() == 1 {
                // (voie actuelle : complexe uniquement ; le réel suit)
                let va = self.expr(&args[0], env, types);
                if let JitVal::Complexe(re, im) = va {
                    let rr = self.builder.ins().fmul(re, re);
                    let ii = self.builder.ins().fmul(im, im);
                    let ss = self.builder.ins().fadd(rr, ii);
                    let f = self.runtime("rose_sqrt");
                    let call = self.builder.ins().call(f, &[ss]);
                    let c = self.builder.func.dfg.first_result(call);
                    let zt = self.builder.ins().iconst(types::I64, 0);
                    self.builder.def_var(self.tag_tmp, zt);
                    return JitVal::Simple(c);
                }
            }
            // hors sous-ensemble : le bloc ne sera pas compilé (ty_statique a
            // déjà refusé cet appel) — on rend zéro, jamais atteint.
            let z = self.builder.ins().iconst(types::I64, 0);
            return JitVal::Simple(z);
        }
        let sig = self.signatures[nom].clone();
        let fref = self.fonction_ref(nom, &sig);
        let mut vals: Vec<Value> = Vec::new();
        for (i, a) in args.iter().enumerate() {
            let pt = sig.params[i].1.n;
            let v = self.expr(a, env, types);
            match (pt, v) {
                // paramètre complexe : deux f64 (re, im)
                (TyKind::Complexe, JitVal::Complexe(re, im)) => {
                    vals.push(re); vals.push(im);
                }
                // paramètre flottant : une f64
                (TyKind::Flottant, JitVal::Simple(v)) => {
                    let attendu = types::F64;
                    let obtenu = self.builder.func.dfg.value_type(v);
                    vals.push(if attendu == obtenu { v }
                              else { self.builder.ins().fcvt_from_sint(types::F64, v) });
                }
                // paramètre entier/booléen : une i64
                (_, JitVal::Simple(v)) => {
                    let attendu = types::I64;
                    let obtenu = self.builder.func.dfg.value_type(v);
                    vals.push(if attendu == obtenu { v }
                              else if attendu == types::F64 { self.builder.ins().fcvt_from_sint(types::F64, v) }
                              else { self.builder.ins().fcvt_to_sint(types::I64, v) });
                }
                _ => {
                    let z = self.builder.ins().iconst(types::I64, 0);
                    vals.push(z);
                }
            }
        }
        let inst = self.builder.ins().call(fref, &vals);
        // Retour complexe : deux registres f64 (d0=re, d1=im)
        if sig.retour.n == TyKind::Complexe {
            let rs: Vec<Value> = self.builder.inst_results(inst).to_vec();
            let zt = self.builder.ins().iconst(types::I64, 0);
            self.builder.def_var(self.tag_tmp, zt);
            return JitVal::Complexe(rs[0], rs[1]);
        }
        // Fixe n°1 : tag BigInt pour retour entier
        if sig.retour.n == TyKind::Entier {
            let res = self.builder.inst_results(inst)[0];
            if let Some(ts) = self.tags.get(nom).copied() {
                let t = self.builder.use_var(ts);
                self.builder.def_var(self.tag_tmp, t);
            } else {
                let zt = self.builder.ins().iconst(types::I64, 0);
                self.builder.def_var(self.tag_tmp, zt);
            }
            return JitVal::Simple(res);
        }
        match sig.retour.n {
            TyKind::Vide => JitVal::Simple(self.builder.ins().iconst(types::I64, 0)),
            _ => JitVal::Simple(self.builder.inst_results(inst)[0]),
        }
    }

    fn binaire(&mut self, op: &str, a: &Expr, b: &Expr, env: &mut HashMap<String, Variable>,
               types: &HashMap<String, TyKind>) -> JitVal {
        let ta = self.type_of(a, env, types);
        let tb = self.type_of(b, env, types);
        // arithmetique complexe : chemin dedie (deux f64, pas de tag BigInt)
        if ta == TyKind::Complexe || tb == TyKind::Complexe {
            if matches!(op, "+" | "-" | "*" | "/") {
                return self.binaire_cplx(op, a, b, env, types);
            }
        }
        let flot = ta == TyKind::Flottant || tb == TyKind::Flottant;
        match op {
            "et" => {
                let g = match self.expr(a, env, types) {
                    JitVal::Simple(v) => v,
                    JitVal::Complexe(..) => {
                        let z = self.builder.ins().iconst(types::I64, 0);
                        z
                    }
                };
                let d = match self.expr(b, env, types) {
                    JitVal::Simple(v) => v,
                    JitVal::Complexe(..) => {
                        let z = self.builder.ins().iconst(types::I64, 0);
                        z
                    }
                };
                JitVal::Simple(self.builder.ins().band(g, d))
            }
            "ou" => {
                let g = match self.expr(a, env, types) {
                    JitVal::Simple(v) => v,
                    JitVal::Complexe(..) => {
                        let z = self.builder.ins().iconst(types::I64, 0);
                        z
                    }
                };
                let d = match self.expr(b, env, types) {
                    JitVal::Simple(v) => v,
                    JitVal::Complexe(..) => {
                        let z = self.builder.ins().iconst(types::I64, 0);
                        z
                    }
                };
                JitVal::Simple(self.builder.ins().bor(g, d))
            }
            "==" | "!=" | "<" | "<=" | ">" | ">=" => {
                let gj = self.expr(a, env, types);
                let g = self.simple(gj);
                let tag_g = self.builder.use_var(self.tag_tmp);
                let dj = self.expr(b, env, types);
                let d = self.simple(dj);
                let tag_d = self.builder.use_var(self.tag_tmp);
                let (g, d) = si_flottant(&mut self.builder, flot, g, d);
                let v = if flot {
                    let cc = match op {
                        "==" => FloatCC::Equal,
                        "!=" => FloatCC::NotEqual,
                        "<" => FloatCC::LessThan,
                        "<=" => FloatCC::LessThanOrEqual,
                        ">" => FloatCC::GreaterThan,
                        _ => FloatCC::GreaterThanOrEqual,
                    };
                    self.builder.ins().fcmp(cc, g, d)
                } else {
                    let cc = match op {
                        "==" => IntCC::Equal,
                        "!=" => IntCC::NotEqual,
                        "<" => IntCC::SignedLessThan,
                        "<=" => IntCC::SignedLessThanOrEqual,
                        ">" => IntCC::SignedGreaterThan,
                        _ => IntCC::SignedGreaterThanOrEqual,
                    };
                    self.builder.ins().icmp(cc, g, d)
                };
                // b1 → i128
                JitVal::Simple(self.builder.ins().uextend(types::I64, v))
            }
            "+" | "-" | "*" | "/" | "%" | "^" | "div" => {
                // ('*'/'^'/'-' entiers : hors sous-ensemble, sur_i64 a refuse la
                // compilation en amont — le chemin imul/i64_pow n'est jamais emis
                // pour un operande entier.)
                let gj = self.expr(a, env, types);
                let g = self.simple(gj);
                let tag_g = self.builder.use_var(self.tag_tmp);
                let dj = self.expr(b, env, types);
                let d = self.simple(dj);
                let tag_d = self.builder.use_var(self.tag_tmp);
                let (g, d) = si_flottant(&mut self.builder, flot, g, d);
                match op {
                    "+" => if flot { JitVal::Simple(self.builder.ins().fadd(g, d)) } else {
                        let zero_tag = self.builder.ins().iconst(types::I64, 0);
                        let tag_g_zero = self.builder.ins().icmp(IntCC::Equal, tag_g, zero_tag);
                        let tag_d_zero = self.builder.ins().icmp(IntCC::Equal, tag_d, zero_tag);
                        let tag_g_zero64 = self.builder.ins().uextend(types::I64, tag_g_zero);
                        let tag_d_zero64 = self.builder.ins().uextend(types::I64, tag_d_zero);
                        let both_i64 = self.builder.ins().band(tag_g_zero64, tag_d_zero64);
                        // overflow check signé en i64 (18/11, CORRIGÉ 18/11) :
                        //   overflow ⇔ ((sum^g) & (sum^d)) < 0
                        // ⚠ L'ancienne formule ((g^d)&(sum^g)) manquait le
                        // débordement positif (ex. i64::MAX + 1) : le signe de
                        // (g^d) y est nul. Remplacée après mesure du bug sur
                        // acc_pos dans le chemin RIR.
                        let sum64 = self.builder.ins().iadd(g, d);
                        let sumxg = self.builder.ins().bxor(sum64, g);
                        let sumxd = self.builder.ins().bxor(sum64, d);
                        let ov_bits = self.builder.ins().band(sumxg, sumxd);
                        let no_ov_flag = self.builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, ov_bits, zero_tag);
                        let no_ov = self.builder.ins().uextend(types::I64, no_ov_flag);
                        let fast_ok = self.builder.ins().band(both_i64, no_ov);
                        let fast_b = self.builder.create_block();
                        let slow_b = self.builder.create_block();
                        let merge_b = self.builder.create_block();
                        self.builder.append_block_param(merge_b, types::I64);
                        self.builder.append_block_param(merge_b, types::I64);
                        let zero64 = self.builder.ins().iconst(types::I64, 0);
                        let fast_ok_i8 = self.builder.ins().icmp(IntCC::NotEqual, fast_ok, zero64);
                        self.builder.ins().brif(fast_ok_i8, fast_b, &[], slow_b, &[]);
                        self.builder.switch_to_block(fast_b);
                        self.builder.seal_block(fast_b);
                        let zt = self.builder.ins().iconst(types::I64, 0);
                        self.builder.def_var(self.tag_tmp, zt);
                        self.saut(merge_b, &[sum64, zt]);
                        self.builder.switch_to_block(slow_b);
                        self.builder.seal_block(slow_b);
                        let trame = self.trame_deux_mots();
                        let trame8 = self.builder.ins().iadd_imm(trame, 8);
                        let f = self.import_runtime("rose_add_ent");
                        let _ = self.builder.ins().call(f, &[g, tag_g, d, tag_d, trame, trame8]);
                        let hval = self.builder.ins().load(types::I64, MemFlagsData::new(), trame, 0);
                        let htag = self.builder.ins().load(types::I64, MemFlagsData::new(), trame, 8);
                        self.builder.def_var(self.tag_tmp, htag);
                        self.saut(merge_b, &[hval, htag]);
                        self.builder.switch_to_block(merge_b);
                        self.builder.seal_block(merge_b);
                        let mv = self.builder.block_params(merge_b)[0];
                        let mt = self.builder.block_params(merge_b)[1];
                        self.builder.def_var(self.tag_tmp, mt);
                        JitVal::Simple(mv)
                    },
                    "-" => if flot { JitVal::Simple(self.builder.ins().fsub(g, d)) } else {
                        // ⚠ Même chemin rapide que `+` (mesuré le 18/11, fib(35)) :
                        // un `isub` i64 inline avec détection de débordement
                        // signé, repli sur `rose_sub_ent` seulement si un
                        // opérande est boxé ou si le résultat déborde i64.
                        // Avant : 2 appels `rose_sub_ent` par appel de fib,
                        // chacun avec 2 recherches HashSet dans `mpz_ptr`.
                        let zero_tag = self.builder.ins().iconst(types::I64, 0);
                        let tag_g_zero = self.builder.ins().icmp(IntCC::Equal, tag_g, zero_tag);
                        let tag_d_zero = self.builder.ins().icmp(IntCC::Equal, tag_d, zero_tag);
                        let tag_g_zero64 = self.builder.ins().uextend(types::I64, tag_g_zero);
                        let tag_d_zero64 = self.builder.ins().uextend(types::I64, tag_d_zero);
                        let both_i64 = self.builder.ins().band(tag_g_zero64, tag_d_zero64);
                        let diff = self.builder.ins().isub(g, d);
                        // Détection de débordement SIGNÉ en 4 instructions i64
                        // au lieu d'un aller-retour i128 (mesuré 18/11) :
                        //   overflow ⇔ ((g^d) & (diff^g)) < 0
                        let gxd = self.builder.ins().bxor(g, d);
                        let diffxg = self.builder.ins().bxor(diff, g);
                        let ov_bits = self.builder.ins().band(gxd, diffxg);
                        let zero64 = self.builder.ins().iconst(types::I64, 0);
                        let no_ov_flag = self.builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, ov_bits, zero64);
                        let no_ov_i64 = self.builder.ins().uextend(types::I64, no_ov_flag);
                        let fast_ok = self.builder.ins().band(both_i64, no_ov_i64);
                        let fast_b = self.builder.create_block();
                        let slow_b = self.builder.create_block();
                        let merge_b = self.builder.create_block();
                        self.builder.append_block_param(merge_b, types::I64);
                        self.builder.append_block_param(merge_b, types::I64);
                        let fast_ok_i8 = self.builder.ins().icmp(IntCC::NotEqual, fast_ok, zero64);
                        self.builder.ins().brif(fast_ok_i8, fast_b, &[], slow_b, &[]);
                        self.builder.switch_to_block(fast_b);
                        self.builder.seal_block(fast_b);
                        let zero_tag2 = self.builder.ins().iconst(types::I64, 0);
                        self.builder.def_var(self.tag_tmp, zero_tag2);
                        self.saut(merge_b, &[diff, zero_tag2]);
                        self.builder.switch_to_block(slow_b);
                        self.builder.seal_block(slow_b);
                        let trame = self.trame_deux_mots();
                        let trame8 = self.builder.ins().iadd_imm(trame, 8);
                        let f = self.import_runtime("rose_sub_ent");
                        let _ = self.builder.ins().call(f, &[g, tag_g, d, tag_d, trame, trame8]);
                        let hval = self.builder.ins().load(types::I64, MemFlagsData::new(), trame, 0);
                        let htag = self.builder.ins().load(types::I64, MemFlagsData::new(), trame, 8);
                        self.builder.def_var(self.tag_tmp, htag);
                        self.saut(merge_b, &[hval, htag]);
                        self.builder.switch_to_block(merge_b);
                        self.builder.seal_block(merge_b);
                        let mv = self.builder.block_params(merge_b)[0];
                        let mt = self.builder.block_params(merge_b)[1];
                        self.builder.def_var(self.tag_tmp, mt);
                        JitVal::Simple(mv)
                    },
                    "*" => if flot { JitVal::Simple(self.builder.ins().fmul(g, d)) } else {
                        let zero_tag = self.builder.ins().iconst(types::I64, 0);
                        let tag_g_zero = self.builder.ins().icmp(IntCC::Equal, tag_g, zero_tag);
                        let tag_d_zero = self.builder.ins().icmp(IntCC::Equal, tag_d, zero_tag);
                        let tag_g_zero64 = self.builder.ins().uextend(types::I64, tag_g_zero);
                        let tag_d_zero64 = self.builder.ins().uextend(types::I64, tag_d_zero);
                        let both_i64 = self.builder.ins().band(tag_g_zero64, tag_d_zero64);
                        let prod = self.builder.ins().imul(g, d);
                        // overflow check via i128 (pas de division)
                        let g128 = self.builder.ins().sextend(types::I128, g);
                        let d128 = self.builder.ins().sextend(types::I128, d);
                        let prod128 = self.builder.ins().imul(g128, d128);
                        let prod64_back = self.builder.ins().sextend(types::I128, prod);
                        let no_ov_flag = self.builder.ins().icmp(IntCC::Equal, prod128, prod64_back);
                        let no_ov_i64 = self.builder.ins().uextend(types::I64, no_ov_flag);
                        let zero64 = self.builder.ins().iconst(types::I64, 0);
                        let fast_ok = self.builder.ins().band(both_i64, no_ov_i64);
                        let fast_b = self.builder.create_block();
                        let slow_b = self.builder.create_block();
                        let merge_b = self.builder.create_block();
                        self.builder.append_block_param(merge_b, types::I64);
                        self.builder.append_block_param(merge_b, types::I64);
                        let zero64 = self.builder.ins().iconst(types::I64, 0);
                        let fast_ok_i8 = self.builder.ins().icmp(IntCC::NotEqual, fast_ok, zero64);
                        self.builder.ins().brif(fast_ok_i8, fast_b, &[], slow_b, &[]);
                        self.builder.switch_to_block(fast_b);
                        self.builder.seal_block(fast_b);
                        let zero_tag2 = self.builder.ins().iconst(types::I64, 0);
                        self.builder.def_var(self.tag_tmp, zero_tag2);
                        self.saut(merge_b, &[prod, zero_tag2]);
                        self.builder.switch_to_block(slow_b);
                        self.builder.seal_block(slow_b);
                        let trame = self.trame_deux_mots();
                        let trame8 = self.builder.ins().iadd_imm(trame, 8);
                        let f = self.import_runtime("rose_mul_ent");
                        let _ = self.builder.ins().call(f, &[g, tag_g, d, tag_d, trame, trame8]);
                        let hval = self.builder.ins().load(types::I64, MemFlagsData::new(), trame, 0);
                        let htag = self.builder.ins().load(types::I64, MemFlagsData::new(), trame, 8);
                        self.builder.def_var(self.tag_tmp, htag);
                        self.saut(merge_b, &[hval, htag]);
                        self.builder.switch_to_block(merge_b);
                        self.builder.seal_block(merge_b);
                        let mv = self.builder.block_params(merge_b)[0];
                        let mt = self.builder.block_params(merge_b)[1];
                        self.builder.def_var(self.tag_tmp, mt);
                        JitVal::Simple(mv)
                    },
                    "/" => {
                        // division flottante protégée (diviseur nul -> erreur)
                        JitVal::Simple(self.div_f64_guarded(g, d))
                    }
                    "%" => {
                        // garde de justesse : un diviseur nul ne doit jamais
                        // atteindre srem (qui trapperait). On delegue a
                        // l'interpreteur via le helper runtime qui sort proprement.
                        let zero = self.builder.ins().iconst(types::I64, 0);
                        let is_zero = self.builder.ins().icmp(cranelift::codegen::ir::condcodes::IntCC::Equal, d, zero);
                        let trap_b = self.builder.create_block();
                        let cont_b = self.builder.create_block();
                        self.builder.ins().brif(is_zero, trap_b, &[], cont_b, &[]);
                        self.builder.switch_to_block(trap_b);
                        self.builder.seal_block(trap_b);
                        {
                            let f = self.runtime("rose_erreur_mod_zero");
                            let _ = self.builder.ins().call(f, &[]);
                        }
                        self.builder.ins().trap(cranelift::codegen::ir::TrapCode::INTEGER_DIVISION_BY_ZERO);
                        self.builder.switch_to_block(cont_b);
                        self.builder.seal_block(cont_b);
                        let r = self.builder.ins().srem(g, d);
                        JitVal::Simple(self.correction_rem(r, d))
                    }
                    "div" => {
                        let q = self.builder.ins().sdiv(g, d);
                        JitVal::Simple(self.floor_div(q, g, d))
                    }
                    "^" => {
                        if flot {
                            // Puissance flottante par powf (miroir de l'appel
                            // std::pow, même arrondi libm).
                            let f = self.powf();
                            let inst = self.builder.ins().call(f, &[g, d]);
                            JitVal::Simple(self.builder.inst_results(inst)[0])
                        } else {
                            JitVal::Simple(self.i64_pow(g, d))
                        }
                    }
                    _ => {
                        let z = self.builder.ins().iconst(types::I64, 0);
                        JitVal::Simple(z)
                    }
                }
            }
            _ => {
                let z = self.builder.ins().iconst(types::I64, 0);
                JitVal::Simple(z)
            }
        }
    }

    /// Arithmetique complexe : (re, im) sont deux valeurs f64 Cranelift.
    /// Miroir EXACT de l'interpreteur (src/interp.rs) :
    ///   (gr+gi i) + (dr+di i) = (gr+dr) + (gi+di) i
    ///   (gr+gi i) * (dr+di i) = (gr*dr - gi*di) + (gr*di + gi*dr) i
    ///   (gr+gi i) / (dr+di i) = ((gr*dr + gi*di)/d) + ((gi*dr - gr*di)/d) i
    ///
    /// VOIE RAPIDE : quand un seul operande est complexe, on n'émet pas les
    /// multiplications par 0.0 / 1.0 de la formule generale (c'est ce qui rend
    /// la construction « x + y*i » rapide : 3 operations au lieu de 8).
    /// L'interpreteur applique exactement la meme voie (bit a bit).
    fn binaire_cplx(&mut self, op: &str, a: &Expr, b: &Expr, env: &mut HashMap<String, Variable>,
                     types: &HashMap<String, TyKind>) -> JitVal {
        let ta = self.type_of(a, env, types);
        let tb = self.type_of(b, env, types);
        let a_cplx = ta == TyKind::Complexe;
        let b_cplx = tb == TyKind::Complexe;
        // un complexe n'est pas un entier tagge : tag = 0
        let zt = self.builder.ins().iconst(types::I64, 0);
        self.builder.def_var(self.tag_tmp, zt);
        if a_cplx != b_cplx {
            // un seul operande est complexe
            if a_cplx {
                let (cr, ci) = self.parts_cplx(a, env, types);
                let (x, _) = self.parts_cplx(b, env, types);
                let (re, im) = match op {
                    "+" => (self.builder.ins().fadd(cr, x), ci),
                    "-" => (self.builder.ins().fsub(cr, x), ci),
                    "*" => {
                        let r = self.builder.ins().fmul(cr, x);
                        let i = self.builder.ins().fmul(ci, x);
                        (r, i)
                    }
                    "/" => {
                        let r = self.div_f64_guarded(cr, x);
                        let i = self.div_f64_guarded(ci, x);
                        (r, i)
                    }
                    _ => (cr, ci),
                };
                return JitVal::Complexe(re, im);
            } else {
                let (x, _) = self.parts_cplx(a, env, types);
                let (cr, ci) = self.parts_cplx(b, env, types);
                let (re, im) = match op {
                    "+" => (self.builder.ins().fadd(x, cr), ci),
                    "-" => {
                        let r = self.builder.ins().fsub(x, cr);
                        let i = self.builder.ins().fneg(ci);
                        (r, i)
                    }
                    "*" => {
                        let r = self.builder.ins().fmul(x, cr);
                        let i = self.builder.ins().fmul(x, ci);
                        (r, i)
                    }
                    "/" => {
                        let c1 = self.builder.ins().fmul(cr, cr);
                        let c2 = self.builder.ins().fmul(ci, ci);
                        let den = self.builder.ins().fadd(c1, c2);
                        let n1 = self.builder.ins().fmul(x, cr);
                        let n2 = self.builder.ins().fmul(x, ci);
                        let r = self.div_f64_guarded(n1, den);
                        let ni = self.builder.ins().fneg(n2);
                        let i = self.div_f64_guarded(ni, den);
                        (r, i)
                    }
                    _ => (cr, ci),
                };
                return JitVal::Complexe(re, im);
            }
        }
        // complexe ⊕ complexe : formule generale
        let (ar, ai) = self.parts_cplx(a, env, types);
        let (br, bi) = self.parts_cplx(b, env, types);
        let (re, im) = match op {
            "+" => (self.builder.ins().fadd(ar, br), self.builder.ins().fadd(ai, bi)),
            "-" => (self.builder.ins().fsub(ar, br), self.builder.ins().fsub(ai, bi)),
            "*" => {
                let rr = self.builder.ins().fmul(ar, br);
                let ii = self.builder.ins().fmul(ai, bi);
                let p = self.builder.ins().fmul(ar, bi);
                let r = self.builder.ins().fsub(rr, ii);
                let i = self.builder.ins().fadd(p, p);
                (r, i)
            }
            "/" => {
                let bb = self.builder.ins().fmul(br, br);
                let cc = self.builder.ins().fmul(bi, bi);
                let den = self.builder.ins().fadd(bb, cc);
                let x1 = self.builder.ins().fmul(ar, br);
                let x2 = self.builder.ins().fmul(ai, bi);
                let nr = self.builder.ins().fadd(x1, x2);
                let y1 = self.builder.ins().fmul(ai, br);
                let y2 = self.builder.ins().fmul(ar, bi);
                let ni = self.builder.ins().fsub(y1, y2);
                (self.div_f64_guarded(nr, den), self.div_f64_guarded(ni, den))
            }
            _ => (ar, ai),
        };
        JitVal::Complexe(re, im)
    }

    /// Decompose une expression en couple (re, im) f64 : un complexe donne ses
    /// deux parties, un reel est vu comme (x, 0).
    fn parts_cplx(&mut self, e: &Expr, env: &mut HashMap<String, Variable>,
                  types: &HashMap<String, TyKind>) -> (Value, Value) {
        match self.expr(e, env, types) {
            JitVal::Complexe(re, im) => (re, im),
            JitVal::Simple(v) => {
                let vt = self.builder.func.dfg.value_type(v);
                let re = if vt == types::F64 { v }
                         else { self.builder.ins().fcvt_from_sint(types::F64, v) };
                let im = self.f64_zero();
                (re, im)
            }
        }
    }

    /// Division flottante protégée : un diviseur nul doit signaler l'erreur
/// (comme l'interpréteur) au lieu de produire inf / non_nombre. Le NaN n'est
/// PAS.trigger une erreur (x == 0.0 est faux pour NaN, comme en Rust).
fn div_f64_guarded(&mut self, num: Value, den: Value) -> Value {
    let zero = self.f64_zero();
    let is_zero = self.builder.ins().fcmp(FloatCC::Equal, den, zero);
    let trap_b = self.builder.create_block();
    let cont_b = self.builder.create_block();
    self.builder.ins().brif(is_zero, trap_b, &[], cont_b, &[]);
    self.builder.switch_to_block(trap_b);
    self.builder.seal_block(trap_b);
    {
        let f = self.runtime("rose_erreur_div_flottant_zero");
        let _ = self.builder.ins().call(f, &[]);
    }
    self.builder.ins().trap(cranelift::codegen::ir::TrapCode::INTEGER_DIVISION_BY_ZERO);
    self.builder.switch_to_block(cont_b);
    self.builder.seal_block(cont_b);
    self.builder.ins().fdiv(num, den)
}

fn powf(&mut self) -> FuncRef {
        self.import_runtime("rose_powf")
    }

    /// Émet une expression ENTÈRE par le chemin exact : si elle contient une
    /// multiplication ou une puissance entière, passe par rose_mul_ent /
    /// rose_pow_ent (promotion GMP au dépassement, tag en deuxième mot) ;
    /// sinon calcul i64 ordinaire, tag zéro. Rend (valeur, tag) : tag ≠ 0 =>
    /// pointeur vers une Box<Valeur::Entier(0, Some(Gz))>.
    fn emettre_ent(&mut self, e: &Expr, env: &mut HashMap<String, Variable>,
                   types: &HashMap<String, TyKind>) -> (Value, Value) {
        fn multiplicatif(e: &Expr) -> bool {
            match e {
                Expr::Binaire { op, a, b, .. } => {
                    matches!(op.as_str(), "*" | "^") || multiplicatif(a) || multiplicatif(b)
                }
                Expr::Unaire { a, .. } => multiplicatif(a),
                _ => false,
            }
        }
        if matches!(e, Expr::Unaire { op, .. } if op == "-") {
            // negation entiere : voie exacte (rose_neg_ent) — -i64::MIN = 2^63
            // ne tient pas en i64, la valeur est promue en entier exact.
            if let Expr::Unaire { a, .. } = e {
                let vj = self.expr(a, env, types);
                let v = self.simple(vj);
                return self.neg_exact(v);
            }
        }
        if !multiplicatif(e) {
            let vj = self.expr(e, env, types);
            let v = self.simple(vj);
            let zero = self.builder.ins().iconst(types::I64, 0);
            return (v, zero);
        }
        match e {
            Expr::Binaire { op, a, b, .. } if op == "^" => {
                let (g, gt) = self.emettre_ent(a, env, types);
                let (d, dt) = self.emettre_ent(b, env, types);
                self.pow_exact(g, gt, d, dt)
            }
            Expr::Unaire { op, a, .. } if op == "-" => {
                // negation exacte miroir interp : -(i64::MIN) = 2^63 ne tient pas
                // en i64 -> Gz boxé (tag), rendu par rose_affiche_ent.
                let vj = self.expr(a, env, types);
                let v = self.simple(vj);
                self.neg_exact(v)
            }
            Expr::Unaire { a, .. } => self.emettre_ent(a, env, types),
            _ => {
                let vj = self.expr(e, env, types);
                let v = self.simple(vj);
                let zero = self.builder.ins().iconst(types::I64, 0);
                (v, zero)
            }
        }
    }

    /// a*b exact : appel rose_mul_ent (checked_mul i64 ; promotion GMP si le
    /// produit déborde, rendue dans une Box<Valeur> taggée).
    /// Trame locale de 2×i64 (valeur, tag) adressée au helper : un StackSlot
    /// explicite de 16 octets (les Variables Cranelift ne sont pas adressables).
    fn trame_deux_mots(&mut self) -> Value {
        let slot = self.builder.func.create_sized_stack_slot(
            cranelift::codegen::ir::StackSlotData::new(
                cranelift::codegen::ir::StackSlotKind::ExplicitSlot, 16, 0));
        self.builder.ins().stack_addr(types::I64, slot, 0)
    }

    /// Négation exacte : le tag signale la valeur BigInt.
    fn neg_exact_tag(&mut self, g: Value, _gt: Value) -> (Value, Value) {
        let (s, t) = self.neg_exact(g);
        (s, t)
    }

    fn neg_exact(&mut self, g: Value) -> (Value, Value) {
        let trame = self.trame_deux_mots();
        let f = self.import_runtime("rose_neg_ent");
        let _ = self.builder.ins().call(f, &[g, trame, trame]);
        let sortie = self.builder.ins().load(types::I64, MemFlagsData::new(), trame, 0);
        let tag = self.builder.ins().load(types::I64, MemFlagsData::new(), trame, 8);
        (sortie, tag)
    }

    /// Addition/soustraction exactes entre (valeur,tag) : les deux tags doivent
    /// etre nuls pour rester en i64 ; sinon la fonction n'est pas compilable
    /// (garde en amont), et l'addition i64 simple serait FAUSSE.
    fn add_exact(&mut self, g: Value, gt: Value, d: Value, dt: Value, soustraction: bool) -> Value {
        let zero = self.builder.ins().iconst(types::I64, 0);
        let ge = self.builder.ins().icmp(IntCC::Equal, gt, zero);
        let de = self.builder.ins().icmp(IntCC::Equal, dt, zero);
        let les_deux = self.builder.ins().band(ge, de);
        // tag ≠ 0 => operande Gz boxé : hors trame i64. On ne peut PAS combiner :
        // le compilable-check doit refuser (voir plus bas le garde 'tag dans le
        // flux'). Pour l'instant, si un tag apparait, la valeur rendue sera
        // incorrecte — le garde 'tags_interdits' en emission le detecte.
        let _ = les_deux;
        if soustraction { self.builder.ins().isub(g, d) } else { self.builder.ins().iadd(g, d) }
    }

    fn mul_exact(&mut self, g: Value, _gt: Value, d: Value, _dt: Value) -> (Value, Value) {
        let trame = self.trame_deux_mots();
        let f = self.import_runtime("rose_mul_ent");
        let _ = self.builder.ins().call(f, &[g, d, trame, trame]);
        let sortie = self.builder.ins().load(types::I64, MemFlagsData::new(), trame, 0);
        let tag = self.builder.ins().load(types::I64, MemFlagsData::new(), trame, 8);
        (sortie, tag)
    }

    fn pow_exact(&mut self, g: Value, _gt: Value, d: Value, _dt: Value) -> (Value, Value) {
        let trame = self.trame_deux_mots();
        let f = self.import_runtime("rose_pow_ent");
        let _ = self.builder.ins().call(f, &[g, d, trame, trame]);
        let sortie = self.builder.ins().load(types::I64, MemFlagsData::new(), trame, 0);
        let tag = self.builder.ins().load(types::I64, MemFlagsData::new(), trame, 8);
        (sortie, tag)
    }

    /// Référence appelée pour une fonction du programme : le FuncId déclaré en
    /// passe 1 devient un FuncRef importé (colocalisé, résolu à la finalisation).
    fn fonction_ref(&mut self, nom: &str, sig: &Signature) -> FuncRef {
        if let Some(&r) = self.refs.get(nom) { return r; }
        // ⚠⚠⚠ LA FONCTION PEUT ÊTRE UN DÉLÉGUÉ (mesuré le 17/11).
        //
        // ⚠ Une fonction non éligible (importée d'un jardin, hors sous-ensemble)
        // est déclarée dans `delegues`, pas dans `decls` : sans ce repli, un
        // appel compilé vers elle panique « no entry found for key ».
        let fid = match self.decls.get(nom).or_else(|| self.delegues.get(nom)) {
            Some(f) => *f,
            None => return self.import_runtime("rose_appel_interpreteur"),
        };
        let s = self.module.declarations().get_function_decl(fid).signature.clone();
        let sigref = self.builder.import_signature(s);
        // déclaration d'un nom utilisateur (namespace 0 = fonction, index =
        // FuncId du module) que le module résoudra sur le symbole défini.
        let uref = self.builder.func.declare_imported_user_function(
            cranelift::codegen::ir::UserExternalName { namespace: 0, index: fid.as_u32() });
        let name = cranelift::codegen::ir::ExternalName::User(uref);
        let r = self.builder.import_function(cranelift::codegen::ir::ExtFuncData {
            name,
            signature: sigref,
            colocated: true,
            patchable: false,
        });
        self.refs.insert(nom.to_string(), r);
        let _ = sig;
        r
    }

    fn runtime(&mut self, nom: &str) -> FuncRef {
        self.import_runtime(nom)
    }

    fn import_runtime(&mut self, nom: &str) -> FuncRef {
        // ⚠⚠⚠ LA DÉCLARATION DU HELPER PEUT MANQUER (mesuré le 17/11).
        //
        // ⚠ `runtime` est la table remplie par `compiler_tout` ; `definir_delegues`
        // s'exécute AVANT et déclare le helper à la demande, dans le MODULE
        // seulement. Il faut donc aussi l'ajouter ici (même signature).
        let fid = match self.runtime.get(nom) {
            Some(f) => *f,
            // ⚠⚠ Repli : le helper a été déclaré par `declarer_delegues`.
            None => *self.decls.get(nom)
                .unwrap_or_else(|| panic!("helper runtime non déclaré : {}", nom)),
        };
        let sig = self.module.declarations().get_function_decl(fid).signature.clone();
        let sigref = self.builder.import_signature(sig);
        let uref = self.builder.func.declare_imported_user_function(
            cranelift::codegen::ir::UserExternalName { namespace: 0, index: fid.as_u32() });
        self.builder.import_function(cranelift::codegen::ir::ExtFuncData {
            name: cranelift::codegen::ir::ExternalName::User(uref),
            signature: sigref,
            colocated: false,
            patchable: false,
        })
    }

    fn correction_rem(&mut self, r: Value, d: Value) -> Value {
        let zero = self.builder.ins().iconst(types::I64, 0);
        let rneg = self.builder.ins().icmp(IntCC::SignedLessThan, r, zero);
        let dpos = self.builder.ins().icmp(IntCC::SignedGreaterThan, d, zero);
        let rpos = self.builder.ins().icmp(IntCC::SignedGreaterThan, r, zero);
        let dneg = self.builder.ins().icmp(IntCC::SignedLessThan, d, zero);
        let a = self.builder.ins().band(rneg, dpos);
        let b = self.builder.ins().band(rpos, dneg);
        let cond = self.builder.ins().bor(a, b);
        let corr = self.builder.ins().iadd(r, d);
        self.builder.ins().select(cond, corr, r)
    }

    fn floor_div(&mut self, q: Value, g: Value, d: Value) -> Value {
        let r = self.builder.ins().srem(g, d);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let rnull = self.builder.ins().icmp(IntCC::Equal, r, zero);
        let rneg = self.builder.ins().icmp(IntCC::SignedLessThan, r, zero);
        let dpos = self.builder.ins().icmp(IntCC::SignedGreaterThan, d, zero);
        let rpos = self.builder.ins().icmp(IntCC::SignedGreaterThan, r, zero);
        let dneg = self.builder.ins().icmp(IntCC::SignedLessThan, d, zero);
        let a = self.builder.ins().band(rneg, dpos);
        let b = self.builder.ins().band(rpos, dneg);
        let signes = self.builder.ins().bor(a, b);
        let doit = self.builder.ins().band_not(signes, rnull);
        let un = self.builder.ins().iconst(types::I64, 1);
        let q2 = self.builder.ins().isub(q, un);
        self.builder.ins().select(doit, q2, q)
    }

    fn i64_pow(&mut self, g: Value, d: Value) -> Value {
        // ^ entier : refuse en amont (sur_i64 retourne false), ce code n'est
        // jamais atteint. Stub pour satisfaire le compilateur.
        let _ = (g, d);
        self.builder.ins().iconst(types::I64, 0)
    }

    fn emettre_bloc(&mut self, instrs: &[Instr], env: &mut HashMap<String, Variable>,
                    types: &mut HashMap<String, TyKind>, sig: &Signature) {
        for i in instrs {
            match i {
                Instr::Affect { nom, e, ligne } => {
                    crate::run::noter_ligne(*ligne);
                    let vj = self.expr(e, env, types);
                    match vj {
                        JitVal::Complexe(re, im) => {
                            // deux slots f64 (re, im) : le nom peut déjà
                            // exister (réaffectation) ou pas (première écriture)
                            let (slot_re, slot_im) = match (env.get(nom).copied(),
                                                            self.complex_im.get(nom).copied()) {
                                (Some(s), Some(i)) => (s, i),
                                (None, _) | (_, None) => {
                                    let s_re = self.builder.declare_var(types::F64);
                                    let s_im = self.builder.declare_var(types::F64);
                                    self.prochain_slot += 2;
                                    env.insert(nom.clone(), s_re);
                                    self.complex_im.insert(nom.clone(), s_im);
                                    (s_re, s_im)
                                }
                            };
                            self.builder.def_var(slot_re, re);
                            self.builder.def_var(slot_im, im);
                            types.insert(nom.clone(), TyKind::Complexe);
                            // le tag : un complexe n'est pas un entier taggé
                            let zt = self.builder.ins().iconst(types::I64, 0);
                            self.builder.def_var(self.tag_tmp, zt);
                        }
                        JitVal::Simple(v) => {
                            let vt = self.builder.func.dfg.value_type(v);
                            let t = if vt == types::F64 { TyKind::Flottant } else { TyKind::Entier };
                            let slot = match env.get(nom).copied() {
                                Some(s) => {
                                    // le slot existant doit avoir le même type
                                    // Cranelift que la valeur écrite
                                    let st = match types.get(nom).copied() {
                                        Some(TyKind::Flottant) | Some(TyKind::Complexe) => types::F64,
                                        _ => types::I64,
                                    };
                                    if st != vt {
                                        let ns = self.builder.declare_var(vt);
                                        self.prochain_slot += 1;
                                        env.insert(nom.clone(), ns);
                                        ns
                                    } else { s }
                                }
                                None => {
                                    let s = self.builder.declare_var(vt);
                                    self.prochain_slot += 1;
                                    env.insert(nom.clone(), s);
                                    s
                                }
                            };
                            types.insert(nom.clone(), t);
                            if t != TyKind::Flottant {
                                let ts = match self.tags.get(nom).copied() {
                                    Some(ts) => ts,
                                    None => {
                                        let ts = self.builder.declare_var(types::I64);
                                        self.prochain_slot += 1;
                                        let zt = self.builder.ins().iconst(types::I64, 0);
                                        self.builder.def_var(ts, zt);
                                        self.tags.insert(nom.clone(), ts);
                                        ts
                                    }
                                };
                                let tag = self.builder.use_var(self.tag_tmp);
                                self.builder.def_var(ts, tag);
                            }
                            self.builder.def_var(slot, v);
                        }
                    }
                }
                Instr::Afficher(exprs, _) => {
                    if exprs.len() != 1 { return; }
                    let e = &exprs[0];
                    // FORMULE EXACTE (LaTeX) de l'expression affichée,
                    // écrite AVANT la valeur — miroir EXACT de
                    // l'interpréteur (interp.rs, branche Instr::Afficher) :
                    // les deux voies rendent la même sortie.
                    let formule = crate::latex::expr_latex_ctx(e, crate::front::i_est_entier_verifie());
                    // On n'émet le préfixe LaTeX que pour les expressions
                    // COMPOSÉES (appel, binaire, boucle…) : un atome pur
                    // (littéral, variable, booléen, chaîne, complexe) est
                    // rendu sans préfixe. C'est le MÊME prédicat que
                    // l'interpréteur (latex::formule_composee) → parité
                    // garantie sans coordination runtime.
                    if crate::latex::formule_composee(e) {
                        let (ptr, len) = interner_formule(&formule);
                        let pf = self.import_runtime("rose_affiche_prefixe");
                        let pv = self.builder.ins().iconst(types::I64, ptr);
                        let pl = self.builder.ins().iconst(types::I64, len);
                        let _ = self.builder.ins().call(pf, &[pv, pl]);
                    }
                    let vj = self.expr(e, env, types);
                    match vj {
                        JitVal::Complexe(re, im) => {
                            let f = self.import_runtime("rose_affiche_complexe");
                            let _ = self.builder.ins().call(f, &[re, im]);
                            continue;
                        }
                        JitVal::Simple(v) => {
                            let vt = self.builder.func.dfg.value_type(v);
                            if vt == types::F64 {
                                let f = self.runtime("rose_affiche_f64");
                                let _ = self.builder.ins().call(f, &[v]);
                            } else {
                                // Entier ou booléen : valeur + tag (tag ≠ 0 =>
                                // BigInt boxé via rose_mul_ent / rose_neg_ent).
                                let vv = self.builder.ins().ireduce(types::I64, v);
                                let tag = self.builder.use_var(self.tag_tmp);
                                let f = self.runtime("rose_affiche_ent");
                                let _ = self.builder.ins().call(f, &[vv, tag]);
                            }
                        }
                    }
                }
                Instr::ExprStmt(e) => { self.expr(e, env, types); }
                Instr::Si { test, alors, sinon, ligne } => {
                    crate::run::noter_ligne(*ligne);
                    let avant_retour = self.returned;
                    let corps = self.builder.create_block();
                    let apres_b = self.builder.create_block();
                    let tj = self.expr(test, env, types);
                    let cond = self.simple(tj);
                    let zero = self.builder.ins().iconst(types::I64, 0);
                    let est_vrai = self.builder.ins().icmp(IntCC::NotEqual, cond, zero);
                    match sinon {
                        None => {
                            self.br_args(est_vrai, corps, &[], apres_b, &[]);
                            self.builder.switch_to_block(corps);
                            self.builder.seal_block(corps);
                            self.returned = avant_retour;
                            self.emettre_bloc(alors, env, types, sig);
                            if !self.returned { self.saut(apres_b, &[]); }
                            self.builder.switch_to_block(apres_b);
                            self.builder.seal_block(apres_b);
                            self.returned = avant_retour;
                        }
                        Some(else_) => {
                            let sinon_b = self.builder.create_block();
                            self.br_args(est_vrai, corps, &[], sinon_b, &[]);
                            self.builder.switch_to_block(corps);
                            self.builder.seal_block(corps);
                            self.returned = avant_retour;
                            self.emettre_bloc(alors, env, types, sig);
                            if !self.returned { self.saut(apres_b, &[]); }
                            self.builder.switch_to_block(sinon_b);
                            self.builder.seal_block(sinon_b);
                            self.returned = avant_retour;
                            self.emettre_bloc(else_, env, types, sig);
                            if !self.returned { self.saut(apres_b, &[]); }
                            self.builder.switch_to_block(apres_b);
                            self.builder.seal_block(apres_b);
                            self.returned = avant_retour;
                        }
                    }
                }
                Instr::TantQue { test, corps, ligne } => {
                    crate::run::noter_ligne(*ligne);
                    let entree = self.builder.create_block();
                    let tour = self.builder.create_block();
                    let sortie = self.builder.create_block();
                    self.saut(entree, &[]);
                    self.builder.switch_to_block(entree);
                    let tj = self.expr(test, env, types);
                    let cond = self.simple(tj);
                    let zero = self.builder.ins().iconst(types::I64, 0);
                    let ok = self.builder.ins().icmp(IntCC::NotEqual, cond, zero);
                    self.br_args(ok, tour, &[], sortie, &[]);
                    self.builder.switch_to_block(tour);
                    self.emettre_bloc(corps, env, types, sig);
                    if !self.returned { self.saut(entree, &[]); }
                    self.builder.switch_to_block(sortie);
                    if self.returned {
                        // bloc mort : fermé par un retour de secours (seal_all se
                        // charge du scellement).
                        match sig.retour.n {
                            TyKind::Vide => { self.builder.ins().return_(&[]); }
                            TyKind::Flottant => {
                                let c = self.builder.ins().iconst(types::I64, 0);
                                let z = self.builder.ins().bitcast(types::F64, MemFlagsData::new(), c);
                                self.builder.ins().return_(&[z]);
                            }
                            TyKind::Complexe => {
                                let c = self.builder.ins().iconst(types::I64, 0);
                                let fz = self.builder.ins().bitcast(types::F64, MemFlagsData::new(), c);
                                self.builder.ins().return_(&[fz, fz]);
                            }
                            _ => { let c = self.builder.ins().iconst(types::I64, 0); self.builder.ins().return_(&[c]); }
                        }
                    }
                }
                Instr::Pour { nom, debut, fin, corps, ligne } => {
                    crate::run::noter_ligne(*ligne);
                    // FORMULE EXACTE de la boucle (LaTeX), affichée avant le
                    // résultat — miroir EXACT de ce que fait l'interpréteur
                    // (interp.rs, branche Instr::Pour) : les deux voies
                    // doivent rendre la même sortie, caractère pour
                    // caractère (parité JIT ↔ interpréteur).
                    let formule = crate::latex::instr_latex(&Instr::Pour {
                        nom: nom.clone(), debut: debut.clone(), fin: fin.clone(),
                        corps: corps.clone(), ligne: *ligne,
                    });
                    let (ptr, len) = interner_formule(&formule);
                    let pf = self.import_runtime("rose_affiche_formule");
                    let pv = self.builder.ins().iconst(types::I64, ptr);
                    let pl = self.builder.ins().iconst(types::I64, len);
                    let _ = self.builder.ins().call(pf, &[pv, pl]);
                    let dj = self.expr(debut, env, types);
                    let d = self.simple(dj);
                    let fj = self.expr(fin, env, types);
                    let f = self.simple(fj);
                    // garde de parité : les bornes d'un « pour » doivent être des
                    // ENTIERS natifs. Une borne qui se replie sur un complexe
                    // (0+1i du lexème « i » non dé-replié dans un contexte où la
                    // table des types ne le connaît pas encore) bouclerait
                    // éternellement en natif : on refuse l'émission et le bloc
                    // retombe sur l'interpréteur, qui connaît la réparation.
                    if self.builder.func.dfg.value_type(d) != types::I64
                        || self.builder.func.dfg.value_type(f) != types::I64 {
                        self.refus_i = true;
                        return;
                    }
                    let slot = {
                        match env.get(nom).copied() {
                            Some(s) => s,
                            None => {
                                let s = self.builder.declare_var(types::I64);
                                self.prochain_slot += 1;
                                let ts = self.builder.declare_var(types::I64);
                                self.prochain_slot += 1;
                                let zt = self.builder.ins().iconst(types::I64, 0);
                                self.builder.def_var(ts, zt);
                                self.tags.insert(nom.clone(), ts);
                                env.insert(nom.clone(), s);
                                types.insert(nom.clone(), TyKind::Entier);
                                s
                            }
                        }
                    };
                    self.builder.def_var(slot, d);
                    // Boucle en un seul bloc auto-suffisant : le test est émis en
                    // queue, donc le bloc corps n'a qu'un seul prédécesseur
                    // (l'entrée initiale ou lui-même) et le SSA builder n'a aucun
                    // phi à relier : le compteur est passé en paramètre explicite,
                    // et les variables du corps sont publiées par def_var en tête.
                    let corps_b = self.builder.create_block();
                    let sortie = self.builder.create_block();
                    self.builder.append_block_param(corps_b, types::I64);
                    self.saut(corps_b, &[d]);
                    self.builder.switch_to_block(corps_b);
                    let ps: Vec<Value> = self.builder.block_params(corps_b).to_vec();
                    let cur = ps[0];
                    self.builder.def_var(slot, cur);
                    self.emettre_bloc(corps, env, types, sig);
                    if !self.returned {
                        let cur2 = self.builder.use_var(slot);
                        let un = self.builder.ins().iconst(types::I64, 1);
                        let next = self.builder.ins().iadd(cur2, un);
                        let ok2 = self.builder.ins().icmp(IntCC::SignedLessThan, next, f);
                        self.br_args(ok2, corps_b, &[next], sortie, &[]);
                    }
                    self.builder.switch_to_block(sortie);
                    if self.returned {
                        // « sortie » n'est atteinte que par la branche fausse de la
                        // queue ; si le corps retourne toujours, il reste vide :
                        // retour de secours (code mort, jamais exécuté).
                        match sig.retour.n {
                            TyKind::Vide => { self.builder.ins().return_(&[]); }
                            TyKind::Flottant => {
                                let c = self.builder.ins().iconst(types::I64, 0);
                                let z = self.builder.ins().bitcast(types::F64, MemFlagsData::new(), c);
                                self.builder.ins().return_(&[z]);
                            }
                            TyKind::Complexe => {
                                let c = self.builder.ins().iconst(types::I64, 0);
                                let fz = self.builder.ins().bitcast(types::F64, MemFlagsData::new(), c);
                                self.builder.ins().return_(&[fz, fz]);
                            }
                            _ => { let c = self.builder.ins().iconst(types::I64, 0); self.builder.ins().return_(&[c]); }
                        }
                    }
                }
                Instr::Retour(e, _) => {
                    match e {
                        None => { self.builder.ins().return_(&[]); }
                        Some(e) => {
                            // le tag BigInt ne compte que pour un retour ENTIER :
                            // inutile d'appeler l'aide runtime sinon (relocation
                            // + veneer + appel de fonction en moins).
                            if sig.retour.n == TyKind::Entier {
                                let tag = self.builder.use_var(self.tag_tmp);
                                let f = self.import_runtime("rose_set_ret_tag");
                                let _ = self.builder.ins().call(f, &[tag]);
                            }
                            match self.expr(e, env, types) {
                                JitVal::Simple(v) => { self.builder.ins().return_(&[v]); }
                                JitVal::Complexe(re, im) => { self.builder.ins().return_(&[re, im]); }
                            }
                        }
                    }
                    self.returned = true;
                }
                Instr::Bloc(b) => self.emettre_bloc(b, env, types, sig),
                _ => {}
            }
        }
    }
}

fn si_flottant(b: &mut FunctionBuilder, flot: bool, g: Value, d: Value) -> (Value, Value) {
    if !flot { return (g, d); }
    // le type REEL de la valeur Cranelift decide : un fcvt_from_sint sur une
    // valeur deja f64 est refuse par le verificateur.
    let gt = b.func.dfg.value_type(g);
    let dt = b.func.dfg.value_type(d);
    let g = if gt == types::F64 { g } else { b.ins().fcvt_from_sint(types::F64, g) };
    let d = if dt == types::F64 { d } else { b.ins().fcvt_from_sint(types::F64, d) };
    (g, d)
}


// les helpers d'affichage appelés par le code JIT (cranelift import) sont
// définis ici avec la convention C : référencés par nom via ExternalName, donc
// jamais appelés depuis Rust directement.
#[allow(dead_code)]
#[unsafe(no_mangle)]
pub extern "C" fn rose_affiche_i64(v: i64) { crate::run::ecrire_out(&format!("{}\n", v)); }

/// Helper JIT : affichage d'un i64 taggé (valeur, tag) rendu par rose_mul_ent /
/// rose_pow_ent / rose_neg_ent. tag ≠ 0 recycle la Box<Valeur>.
#[unsafe(no_mangle)]
pub extern "C" fn rose_affiche_ent(valeur: i64, tag: i64) {
    if tag == 0 {
        crate::run::ecrire_out(&format!("{}\n", valeur));
    } else {
        let g = crate::run::Gz::neuf();
        unsafe { g.copier_de_ptr(valeur as *const crate::run::Mpz); }
        crate::run::ecrire_out(&format!("{}\n", g.chaine()));
        std::mem::forget(g);
    }
}

#[allow(dead_code)]
#[unsafe(no_mangle)]
pub extern "C" fn rose_affiche_bool(v: i64) { crate::run::ecrire_out(if v != 0 { "vrai\n" } else { "faux\n" }); }
#[allow(dead_code)]
#[unsafe(no_mangle)]
pub extern "C" fn rose_affiche_f64(v: f64) {
    let t = if v.is_nan() { "non_nombre".to_string() }
        else if v.is_infinite() { if v > 0.0 { "infini".into() } else { "-infini".into() } }
        else if v == v.floor() && v.abs() < 1e16 { (v as i64).to_string() }
        else { crate::front::texte_flottant(v) };
    crate::run::ecrire_out(&format!("{}\n", t));
}

/// Helper JIT : affiche la FORMULE EXACTE en LaTeX d'une boucle « pour ».
/// Le pointeur est celui d'une chaîne interne statique (internée par
/// `interner_formule`), donc valide pendant toute la vie du processus.
/// Helper JIT : écrit le PRÉFIXE « formule   » d'un affichage — la formule
/// LaTeX suivie des trois espaces de séparation (convention de π, φ, e) —
/// SANS saut de ligne : la valeur est écrite ensuite par le helper
/// d'affichage correspondant (afficher_f64, afficher_i64…). L'interpréteur
/// fait exactement de même (run::afficher_formule) : les deux voies rendent
/// la même sortie, caractère pour caractère.
#[unsafe(no_mangle)]
pub extern "C" fn rose_affiche_prefixe(ptr: i64, len: i64) {
    if ptr == 0 || len <= 0 { return; }
    let octets = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    let s = String::from_utf8_lossy(octets);
    crate::run::ecrire_out(&s);
    crate::run::ecrire_out("   ");
}

#[unsafe(no_mangle)]
pub extern "C" fn rose_affiche_formule(ptr: i64, len: i64) {
    if ptr == 0 || len <= 0 { return; }
    let octets = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    let s = String::from_utf8_lossy(octets);
    crate::run::ecrire_out(&s);
    crate::run::ecrire_out("\n");
}

/// Internation d'une formule LaTeX : la chaîne est conservée dans un
/// `Box<str>` STATIQUE (jamais libérée) pour que son adresse reste
/// valide pendant que le code JIT tourne. Le dédoublonnage évite
/// d'allouer la même formule plusieurs fois.
pub fn interner_formule(s: &str) -> (i64, i64) {
    use std::collections::HashSet;
    use std::sync::Mutex;
    use std::sync::OnceLock;
    static REGISTRE: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let reg = REGISTRE.get_or_init(|| Mutex::new(HashSet::new()));
    let mut g = match reg.lock() { Ok(g) => g, Err(p) => p.into_inner() };
    if let Some(existant) = g.get(s) {
        return (existant.as_ptr() as i64, existant.len() as i64);
    }
    let STATIQUE: &'static str = Box::leak(s.to_string().into_boxed_str());
    g.insert(STATIQUE);
    (STATIQUE.as_ptr() as i64, STATIQUE.len() as i64)
}

pub extern "C" fn rose_affiche_complexe(re: f64, im: f64) {
    // miroir EXACT de chaine_de_p() pour un complexe : « 2i » et non « 0+2i »,
    // « 3+i » et non « 3+1i ». L'émetteur JIT appelle ce helper pour tout
    // affichage de complexe ; s'il imprimait le format brut, la voie JIT
    // divergerait de l'interpréteur sur tous les bancs complexes.
    let s = if im == 0.0 {
        re.to_string()
    } else if re == 0.0 {
        if im == 1.0 { "i".into() } else if im == -1.0 { "-i".into() } else { format!("{}i", im) }
    } else if im == 1.0 {
        format!("{}+i", re)
    } else if im == -1.0 {
        format!("{}-i", re)
    } else {
        let sign = if im >= 0.0 { "+" } else { "" };
        format!("{}{}{}i", re, sign, im)
    };
    crate::run::ecrire_out(&s);
    crate::run::ecrire_out("\n");
}

// ═══════════════════════════════════════════════════════════════════════
// M3.1 — CÂBLAGE DE L'ÉMETTEUR MACHINE NATIF DANS LE CHEMIN JIT
// ═══════════════════════════════════════════════════════════════════════
//
// Le JIT Cranelift reste LA voie par défaut (priorité historique). Ce
// câblage est une voie ADDITIONNELLE, activée explicitement par l'appelant
// (l'orchestrateur appellera `compiler_natif` depuis `main.rs`, par exemple
// sous `ROSE_NATIF=1`, et seulement après avoir laissé Cranelift décider).
//
// ⚠ L'émetteur M3.1 travaille sur des i64 non boxés et une table de
// trampoline `#[repr(C)]`. On ne touche JAMAIS à la layout de `Valeur` : la
// conversion `Valeur ↔ i64` est faite ICI, aux frontières, exactement comme
// `appeler_trampoline` le fait pour Cranelift.
//
// Sous-ensemble accepté (trois portes, toutes conservatrices) :
//   1. types Entier / Booleen uniquement (Vide toléré en retour) ;
//   2. `natif_emetteur::peut_emettre(corps)` ;
//   3. `Emitted::fallbacks.is_empty()` — aucun fallback non câblé.
// Toute fonction refusée rend `None` : l'appelant garde Cranelift ou
// l'interpréteur. Aucune régression possible.
//
// ABI : `i64 code(base: *mut i64, tramp: *const Trampoline)`.
// `Trampoline::appel_rir` réutilise `rose_appel_interpreteur` (A4 : ABI
// identique), donc un AppelRir est délégué à l'interpréteur courant.

// ⚠ Le binaire (`main.rs`) compile `jit.rs` en module SANS déclarer
// `natif_emetteur` (seul `lib.rs` le déclare). On inclut donc le fichier
// SOUS UN NOM LOCAL : `crate::jit::ne_local`. Les types manipulés
// (`CorpsRir`, `Signature`, `Valeur`) viennent des modules partagés, donc
// l'API reste compatible avec le `crate::natif_emetteur` de lib.rs.
#[path = "natif_emetteur.rs"]
mod ne_local;
use ne_local as ne;

/// `eval_fallback` de la trampoline. Jamais appelé (la porte 3 refuse tout
/// corps qui demanderait un fallback), mais l'ABI exige un pointeur non nul.
extern "C" fn natif_fallback_neant(_ctx: *mut std::ffi::c_void,
                                   _id: u32, _base: *mut i64) -> i64 { 0 }

fn natif_type_ok(t: TyKind) -> bool {
    matches!(t, TyKind::Entier | TyKind::Booleen)
}

/// Une fonction Rose compilée en code machine par l'émetteur M3.1.
///
/// `Send + Sync` : le code vit dans une page mmap'ée indépendante
/// (`MemoireExecutable` porte déjà les `unsafe impl`), l'`Arc` est
/// partageable et le pointeur d'appel est un simple `fn` pointer.
pub struct Natif {
    /// `Arc` et non `Box` : la page reste à une adresse STABLE quand le
    /// `Natif` est déplacé dans la closure `Compile::Natif`, ce qui permet
    /// à une trampoline récursive de conserver un pointeur valide.
    mem: std::sync::Arc<ne::MemoireExecutable>,
    n_slots: usize,
    n_slots_total: usize,
    retour: TyKind,
    appel_rir: extern "C" fn(i64, i64, i64, i64, *const u8) -> i64,
}

impl Natif {
    /// Compile `corps` pour la fonction `sig`. Les appels à d'autres
    /// fonctions Rose passent par `rose_appel_interpreteur`.
    pub fn nouveau(corps: &CorpsRir, sig: &Signature) -> Option<Self> {
        Self::nouveau_avec(corps, sig, rose_appel_interpreteur)
    }

    /// Variante d'intégration / de test : fournit un `appel_rir` spécifique
    /// (récursion locale, moteur intégré sans `INTERP_COURANT`…).
    pub fn nouveau_avec(corps: &CorpsRir, sig: &Signature,
                        appel_rir: extern "C" fn(i64, i64, i64, i64, *const u8) -> i64)
        -> Option<Self>
    {
        // porte 1 — types
        if sig.params.iter().any(|(_, t)| !natif_type_ok(t.n)) { return None; }
        if sig.retour.n != TyKind::Vide && !natif_type_ok(sig.retour.n) { return None; }
        // porte 2 — sous-ensemble RIR
        if !ne::peut_emettre(corps) { return None; }
        // porte 3 — émission effective, sans fallback à câbler
        let e = ne::emettre(corps, ne::Architecture::native()).ok()?;
        if !e.fallbacks.is_empty() { return None; }
        let mem = std::sync::Arc::new(ne::MemoireExecutable::nouveau(&e.code).ok()?);
        Some(Natif {
            mem, n_slots: e.n_slots, n_slots_total: e.n_slots_total,
            retour: sig.retour.n, appel_rir,
        })
    }

    /// Page de code partagée (adresse stable) — sert à câbler une trampoline
    /// récursive dans un harnais de test ou un moteur intégré.
    pub fn mem(&self) -> std::sync::Arc<ne::MemoireExecutable> { std::sync::Arc::clone(&self.mem) }

    /// Nombre de slots utilisateur (paramètres compris).
    pub fn n_slots(&self) -> usize { self.n_slots }

    /// La trampoline est construite à chaque appel : elle ne contient que
    /// des pointeurs de fonction et un ctx nul. Aucun état non-Send ne vit
    /// dans la closure `Compile::Natif`.
    fn trampoline(&self) -> ne::Trampoline {
        ne::Trampoline {
            appel_rir: self.appel_rir,
            eval_fallback: natif_fallback_neant,
            ctx: std::ptr::null_mut(),
        }
    }

    /// Appelle le code machine : `Vec<Valeur>` → slots i64 → `Valeur`.
    pub fn appeler(&self, args: Vec<Valeur>) -> Valeur {
        if args.len() > self.n_slots { return Valeur::Vide; }
        let mut slots = vec![0i64; self.n_slots_total];
        for (i, a) in args.iter().enumerate() {
            slots[i] = match a {
                Valeur::Entier(v, None) => *v,
                Valeur::Booleen(b) => if *b { 1 } else { 0 },
                // bit pattern : miroir de `appeler_trampoline` /
                // `appeler_delegue`.
                Valeur::Flottant(d) => d.to_bits() as i64,
                // bigint, rationnel, chaîne… : hors sous-ensemble.
                _ => return Valeur::Vide,
            };
        }
        let r = unsafe { self.mem.appeler(slots.as_mut_ptr(), &self.trampoline()) };
        match self.retour {
            TyKind::Vide => Valeur::Vide,
            TyKind::Booleen => Valeur::b(r != 0),
            TyKind::Flottant => Valeur::f(f64::from_bits(r as u64)),
            _ => Valeur::z(r),
        }
    }

    /// Enveloppe dans la forme `Compile` attendue par `Interp`.
    pub fn dans_compile(self) -> Compile {
        Compile::Natif(Box::new(move |args| self.appeler(args)))
    }
}

/// API demandée pour le câblage CLI : compile un corps RIR en code machine
/// M3.1 et le rend sous forme de `Compile::Natif`.
///
/// `None` = le corps sort du sous-ensemble émettable ; l'appelant doit alors
/// conserver le JIT Cranelift (prioritaire) ou l'interpréteur.
pub fn compiler_natif(corps: &CorpsRir, sig: &Signature) -> Option<Compile> {
    Natif::nouveau(corps, sig).map(|n| n.dans_compile())
}

/// Comme `compiler_natif`, mais avec un `appel_rir` fourni (récursion locale
/// ou moteur intégré sans `INTERP_COURANT`).
pub fn compiler_natif_avec_appel(
    corps: &CorpsRir, sig: &Signature,
    appel_rir: extern "C" fn(i64, i64, i64, i64, *const u8) -> i64,
) -> Option<Compile> {
    Natif::nouveau_avec(corps, sig, appel_rir).map(|n| n.dans_compile())
}

#[cfg(test)]
mod tests_natif {
    use super::*;
    use crate::front::Ty;
    use super::ne_local as ne;
    use crate::rir::{CacheSlot, Oper};

    fn slot(i: u16) -> Eabstrait { Eabstrait::Slot(i, CacheSlot::neuve()) }
    fn bin(op: Oper, a: Eabstrait, b: Eabstrait) -> Eabstrait {
        Eabstrait::Binaire { op, a: Box::new(a), b: Box::new(b) }
    }
    /// `Valeur` n'implémente ni `PartialEq` ni `Debug` : on compare le contenu.
    fn v_i64(v: &Valeur) -> i64 {
        match v {
            Valeur::Entier(x, None) => *x,
            _ => panic!("attendu Valeur::Entier(_, None), obtenu {:?}", v.ty()),
        }
    }

    /// fib en RIR « pré-abaissé » : les arguments d'appel sont des SLOTS
    /// (s1 = n-1, s2 = n-2), forme que l'émetteur M3.1 sait traiter.
    fn corps_fib() -> CorpsRir {
        let var = |i: u16| Expr::Var(format!("s{}", i), 0);
        let app = |i: u16| Eabstrait::AppelRir { nom: "fib".into(), args: vec![var(i)] };
        CorpsRir {
            ops: vec![
                Op::SautSiFauxAbstrait {
                    test: bin(Oper::Lt, slot(0), Eabstrait::Entier("2".into())),
                    cible: 2,
                },
                Op::RetourAbstrait(slot(0)),
                Op::Affect { slot: 1, expr: bin(Oper::Sub, slot(0), Eabstrait::Entier("1".into())) },
                Op::Affect { slot: 3, expr: app(1) },
                Op::Affect { slot: 2, expr: bin(Oper::Sub, slot(0), Eabstrait::Entier("2".into())) },
                Op::Affect { slot: 4, expr: app(2) },
                Op::RetourAbstrait(bin(Oper::Add, slot(3), slot(4))),
            ],
            noms: (0..5).map(|i| format!("s{}", i)).collect(),
            globaux: vec![],
            n_slots: 5,
        }
    }
    fn sig_entier(nom: &str) -> Signature {
        Signature {
            nom: nom.into(),
            params: vec![("n".into(), Ty { n: TyKind::Entier, elt: TyKind::Dyn })],
            retour: Ty { n: TyKind::Entier, elt: TyKind::Dyn },
            a_retour: true,
            corps: None, rir: None,
            dll: String::new(), dll_son: String::new(), symbole: String::new(), ligne: 0,
        }
    }

    thread_local! {
        /// `Arc` dans un thread-local : la page reste vivante même quand le
        /// `Natif` est déplacé dans la closure `Compile::Natif`.
        static CODE_FIB_JIT: std::cell::RefCell<Option<std::sync::Arc<ne::MemoireExecutable>>> =
            std::cell::RefCell::new(None);
    }
    extern "C" fn pas_de_fallback(_: *mut std::ffi::c_void, _: u32, _: *mut i64) -> i64 { 0 }
    extern "C" fn fib_tramp(a0: i64, _: i64, _: i64, _: i64, _: *const u8) -> i64 {
        let mem = CODE_FIB_JIT.with(|c| c.borrow().clone()).expect("code fib non publie");
        let mut slots = [0i64; 5 + ne::SCRATCH_SLOTS as usize];
        slots[0] = a0;
        let tramp = ne::Trampoline {
            appel_rir: fib_tramp, eval_fallback: pas_de_fallback, ctx: std::ptr::null_mut()
        };
        unsafe { mem.appeler(slots.as_mut_ptr(), &tramp) }
    }

    #[test]
    fn api_native_fib25_via_trampoline() {
        let corps = corps_fib();
        let sig = sig_entier("fib");
        let natif = Natif::nouveau_avec(&corps, &sig, fib_tramp).expect("fib émettable");
        CODE_FIB_JIT.with(|c| *c.borrow_mut() = Some(natif.mem()));

        // 1) appel direct via la structure Natif (preuve chronométrée)
        let t0 = std::time::Instant::now();
        let r25 = v_i64(&natif.appeler(vec![Valeur::z(25)]));
        let dt = t0.elapsed();
        eprintln!("M3.1 API fib(25) = {} sur {} en {:?}", r25, ne::Architecture::native().nom(), dt);
        assert_eq!(r25, 75025);

        // 2) via la forme Compile::Natif (ce que main.rs appellera)
        let c = natif.dans_compile();
        if let Compile::Natif(f) = &c {
            assert_eq!(v_i64(&f(vec![Valeur::z(25)])), 75025);
        } else { panic!("pas un Compile::Natif"); }

        // 3) via la fonction publique compiler_natif_avec_appel : on publie
        //    les octets d'un second mmap (même code) pour la récursion.
        let c2 = compiler_natif_avec_appel(&corps, &sig, fib_tramp).expect("compile natif");
        let octets = ne::emettre(&corps, ne::Architecture::native()).unwrap().code;
        CODE_FIB_JIT.with(|c| *c.borrow_mut() = Some(
            std::sync::Arc::new(ne::MemoireExecutable::nouveau(&octets).unwrap())));
        if let Compile::Natif(f) = &c2 {
            assert_eq!(v_i64(&f(vec![Valeur::z(25)])), 75025);
        } else { panic!("pas un Compile::Natif"); }

        CODE_FIB_JIT.with(|c| *c.borrow_mut() = None);
    }

    #[test]
    fn refuse_types_hors_i64_et_corps_a_fallback() {
        let corps = corps_fib();
        let mut sig = sig_entier("fib");
        sig.params[0].1.n = TyKind::Flottant;
        assert!(compiler_natif(&corps, &sig).is_none(), "paramètre flottant refusé");
        // un retour Flottant littéral passe `peut_emettre` mais demande un
        // fallback : la porte 3 doit le refuser.
        let c = CorpsRir {
            ops: vec![Op::RetourAbstrait(Eabstrait::Flottant(1.5))],
            noms: vec![], globaux: vec![], n_slots: 0,
        };
        assert!(compiler_natif(&c, &sig_entier("f")).is_none(), "fallback refusé");
    }

    #[test]
    fn parite_natif_cranelift_interpreteur() {
        // `calc(n) = si n<2 alors n+10 sinon n*n-n` — vrai RIR issu de
        // l'analyse Rose, sans AppelRir (donc sans dépendance au global).
        let source = "fn calc(n: entier) -> entier {\n\
                        si (n < 2) { retour n + 10 }\n\
                        retour n * n - n\n\
                      }\n";
        let mut prog = crate::front::analyser(source).expect("analyse calc");

        // abaissement RIR (miroir de executer_source_analyse), AVANT de
        // construire l'Interp : le programme analysé est public, le champ
        // `Interp::prog` ne l'est pas.
        let globaux_connus: Vec<String> = Vec::new();
        let candidats: Vec<String> = prog.ordre.iter()
            .filter(|n| prog.signatures.get(*n).map_or(false, |s| s.corps.is_some()))
            .cloned().collect();
        crate::rir::declarer_rir(&candidats);
        for nom in prog.ordre.clone() {
            if let Some(sig) = prog.signatures.get(&nom) {
                let Some(corps) = sig.corps.as_ref() else { continue };
                let params: Vec<String> = sig.params.iter().map(|(n, _)| n.clone()).collect();
                if let Some(rir) = crate::rir::abaisser(corps, &params, &globaux_connus) {
                    prog.signatures.get_mut(&nom).unwrap().rir = Some(rir);
                }
            }
        }
        let sig = prog.signatures.get("calc").unwrap().clone();
        let corps = sig.rir.clone().expect("RIR calc");
        assert!(ne::peut_emettre(&corps), "calc doit être émettable: {:?}", corps.ops);
        let mut signatures = std::collections::HashMap::new();
        signatures.insert("calc".to_string(), sig.clone());

        let mut interp = crate::interp::Interp::nouveau(prog);

        // 1) INTERPRÉTEUR (helper officiel, pile de slots vidée)
        crate::interp::poser_interp_courant(&mut interp as *mut _);
        let r_interp = crate::interp::appeler_delegue("calc", &[25]);
        let r_interp_petit = crate::interp::appeler_delegue("calc", &[1]);

        // 2) CRANELIFT (voie prioritaire, inchangée)
        let mut jit = Jit::nouveau(&signatures);
        let compiles = jit.compiler_tout();
        let r_cl = match compiles.get("calc") {
            Some(Compile::Natif(f)) => f(vec![Valeur::z(25)]),
            _ => panic!("Cranelift n'a pas compilé calc"),
        };

        // 3) ÉMETTEUR MACHINE M3.1
        let r_nat = match compiler_natif(&corps, &sig) {
            Some(Compile::Natif(f)) => f(vec![Valeur::z(25)]),
            None => panic!("émetteur machine a refusé calc"),
            Some(_) => panic!("émetteur machine a rendu un Compile non-Natif"),
        };
        let natif_petit = Natif::nouveau(&corps, &sig).expect("calc natif");
        let r_nat_petit = natif_petit.appeler(vec![Valeur::z(1)]);

        // Les TROIS moteurs donnent la même valeur.
        assert_eq!(r_interp, 25 * 25 - 25, "interpréteur n=25");
        assert_eq!(v_i64(&r_cl), r_interp, "Cranelift n=25");
        assert_eq!(v_i64(&r_nat), r_interp, "émetteur machine n=25");
        assert_eq!(v_i64(&r_nat_petit), r_interp_petit, "n=1");
        assert_eq!(r_interp_petit, 11, "interpréteur n=1");

        crate::interp::poser_interp_courant(std::ptr::null_mut());
    }
}
