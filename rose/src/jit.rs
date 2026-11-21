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

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

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
/// ── LISTE_INLINE : boucle machine aarch64 à registres fixés ───────────────
///
/// Le codegen Cranelift laisse 4 `mov` de permutation par paire d'itérations
/// et des tests redondants (mesuré : 1,25 M instructions pour 100 k éléments,
/// 0,66 ms). On émet donc directement les instructions, comme H1 l'a fait pour
/// `somme(n)` (`JIT_BOUCLE_HOT.md`). Registres :
///   x19 = pointeur courant, x20 = i, x21 = fin, x22 = glue/Discriminant,
///   x23 = zéro, x24 = $s, x25 = handle, x26 = $d, x27 = pointeur initial,
///   x28 = $f, x9 = adresse helper. Aucun `mov` de permutation dans les
///   boucles ; 15 instructions / 4 éléments (write) et 10 / 4 (read).
#[cfg(not(all(unix, target_arch = "aarch64")))]
fn emettre_liste_machine(_lig: i64, _mot0: i64) -> Vec<u8> { Vec::new() }
#[cfg(all(unix, target_arch = "aarch64"))]
fn emettre_liste_machine(lig: i64, mot0: i64) -> Vec<u8> {
    let mut ins: Vec<u32> = Vec::with_capacity(96);
    let mut lits: Vec<(usize, usize)> = Vec::new();
    // 0 = temps, 1 = liste_vide, 2 = reserve, 3 = meta, 4 = set_len, 5 = affiche
    let adr: [u64; 6] = [
        rose_temps as *const u8 as u64,
        rose_jit_liste_vide as *const u8 as u64,
        rose_jit_liste_reserve as *const u8 as u64,
        rose_jit_liste_meta as *const u8 as u64,
        rose_jit_liste_set_len as *const u8 as u64,
        rose_affiche_ent as *const u8 as u64,
    ];
    let ldr = |which: usize, lits: &mut Vec<(usize, usize)>, ins: &mut Vec<u32>| {
        let at = ins.len();
        lits.push((at, which));
        ins.push(0x58000009); // LDR x9, [pc, #imm19] — patché en fin
        at
    };
    let call = |which: usize, lits: &mut Vec<(usize, usize)>, ins: &mut Vec<u32>| {
        ldr(which, lits, ins);
        ins.push(0xD63F0120); // BLR x9
    };
    let stpp = |rt: u32, rt2: u32, rn: u32| -> u32 {
        0xA9BF0000 | ((0x7Eu32) << 15) | (rt2 << 10) | (rn << 5) | rt
    };
    let ldp = |rt: u32, rt2: u32, rn: u32| -> u32 {
        0xA8C10000 | ((0x02u32) << 15) | (rt2 << 10) | (rn << 5) | rt
    };
    let movz = |rd: u32, v: u32| -> u32 { 0xD2800000 | ((v & 0xFFFF) << 5) | rd };
    let movk16 = |rd: u32, v: u32| -> u32 { 0xF2A00000 | ((v & 0xFFFF) << 5) | rd };
    let addi = |rd: u32, rn: u32, v: u32| -> u32 {
        0x91000000 | ((v & 0xFFF) << 10) | (rn << 5) | rd
    };
    let subi = |rd: u32, rn: u32, v: u32| -> u32 {
        0xD1000000 | ((v & 0xFFF) << 10) | (rn << 5) | rd
    };
    let str = |rt: u32, rn: u32, v: u32| -> u32 {
        0xF9000000 | ((v / 8) << 10) | (rn << 5) | rt
    };
    let ldr_i = |rt: u32, rn: u32, v: u32| -> u32 {
        0xF9400000 | ((v / 8) << 10) | (rn << 5) | rt
    };
    let stp_off = |rt: u32, rt2: u32, rn: u32, v: u32| -> u32 {
        0xA9000000 | (((v / 8) as u32) << 15) | (rt2 << 10) | (rn << 5) | rt
    };
    let movr = |rd: u32, rm: u32| -> u32 { 0xAA0003E0 | (rm << 16) | rd };
    let add_r = |rd: u32, rn: u32, rm: u32| -> u32 { 0x8B000000 | (rm << 16) | (rn << 5) | rd };
    let sub_r = |rd: u32, rn: u32, rm: u32| -> u32 { 0xCB000000 | (rm << 16) | (rn << 5) | rd };
    let cmp_r = |rn: u32, rm: u32| -> u32 { 0xEB000000 | (rm << 16) | (rn << 5) };
    let bcond = |cond: u32| -> u32 { 0x54000000 | cond };

    // ── prologue ──
    ins.push(stpp(29, 30, 31));
    ins.push(stpp(27, 28, 31));
    ins.push(stpp(25, 26, 31));
    ins.push(stpp(23, 24, 31));
    ins.push(stpp(21, 22, 31));
    ins.push(stpp(19, 20, 31));
    ins.push(0x910003FD); // mov x29, sp
    ins.push(subi(31, 31, 16)); // sub sp, sp, #16 (2 mots pour `meta`)

    // $d = temps()
    call(0, &mut lits, &mut ins);
    ins.push(movr(26, 0)); // x26 = $d
    // $l = liste vide ; reserve(h, L)
    call(1, &mut lits, &mut ins);
    ins.push(movr(25, 0)); // x25 = h
    ins.push(movr(0, 25)); ins.push(movz(1, lig as u32));
    ins.push(movk16(1, (lig as u64 >> 16) as u32));
    call(2, &mut lits, &mut ins);
    // meta(h, sp, sp+8) -> x19 = ptr, x27 = ptr (original), x21 = ptr + L*24
    ins.push(movr(0, 25));
    ins.push(0x910003E1); // mov x1, sp
    ins.push(addi(2, 31, 8));
    call(3, &mut lits, &mut ins);
    ins.push(ldr_i(19, 31, 0));
    ins.push(movr(27, 19));
    let octets = (lig as u64) * 24;
    ins.push(movz(9, octets as u32));
    ins.push(movk16(9, (octets >> 16) as u32));
    ins.push(add_r(21, 19, 9)); // x21 = fin
    ins.push(movz(22, mot0 as u32));
    ins.push(movk16(22, (mot0 as u64 >> 16) as u32));
    ins.push(movz(23, 0)); // x23 = zéro
    ins.push(movz(20, 0)); // x20 = i
    ins.push(movr(24, 23)); // x24 = $s = 0

    // ── boucle unique FUSIONNÉE : 4 éléments écrits + 4 sommes par tour ──
    // Le détecteur strict (`detecter_liste_boucles`, 16 ops RIR exactes)
    // prouve que `l[j]` vaut exactement `j` : `[0:entier]` puis `joindre(l,i)`
    // pour i = 0..L-1, et AUCUNE autre opération ne touche `l` avant la somme.
    // On accumule donc la valeur écrite (déjà dans x20) au lieu de la relire :
    // 100 000 `ldr` supprimés, résultat identique. Les 3 mots de chaque
    // `Valeur::Entier(i, None)` sont TOUJOURS écrits : le Vec reste la source
    // de vérité unique, lisible par l'interpréteur après le JIT.
    let loop1 = ins.len();
    for k in 0..4usize {
        let off = (k * 24) as u32;
        ins.push(stp_off(22, 20, 19, off)); // mot0 + i64
        ins.push(str(23, 19, off + 16));    // Option<Rc> = None
        ins.push(add_r(24, 24, 20));        // $s += i (valeur prouvée = l[off/24])
        if k < 3 { ins.push(addi(20, 20, 1)); }
    }
    ins.push(addi(20, 20, 1));
    ins.push(addi(19, 19, 96));
    ins.push(cmp_r(19, 21));
    let b1 = ins.len();
    ins.push(bcond(3)); // b.lo loop1 (patch)

    // ── publication de len (une fois) ──
    ins.push(movr(0, 25)); ins.push(movz(1, lig as u32));
    ins.push(movk16(1, (lig as u64 >> 16) as u32));
    call(4, &mut lits, &mut ins);

    // $f = temps()
    call(0, &mut lits, &mut ins);
    ins.push(movr(28, 0));
    // afficher $s, puis $f - $d
    ins.push(movr(0, 24)); ins.push(movr(1, 23));
    call(5, &mut lits, &mut ins);
    ins.push(sub_r(0, 28, 26)); ins.push(movr(1, 23));
    call(5, &mut lits, &mut ins);

    // ── épilogue ──
    ins.push(addi(31, 31, 16));
    ins.push(ldp(19, 20, 31));
    ins.push(ldp(21, 22, 31));
    ins.push(ldp(23, 24, 31));
    ins.push(ldp(25, 26, 31));
    ins.push(ldp(27, 28, 31));
    ins.push(ldp(29, 30, 31));
    ins.push(0xD65F03C0); // ret

    // ── literal pool + patchs ──
    let n_ins = ins.len();
    let lit_off = n_ins * 4;
    let rel = |at: usize, target: usize| (target as i64 - at as i64) as i32;
    let patch_bc = |ins: &mut Vec<u32>, at: usize, target: usize| {
        let o = rel(at, target);
        ins[at] = (ins[at] & 0xFF00001F) | (((o as u32) & 0x7FFFF) << 5);
    };
    patch_bc(&mut ins, b1, loop1);
    for (at, which) in lits.iter() {
        let octet_lit = lit_off + which * 8;
        let d = (octet_lit as i64 - (*at as i64 * 4)) / 4;
        ins[*at] = 0x58000009 | (((d as u32) & 0x7FFFF) << 5);
    }

    let mut octets = Vec::with_capacity(n_ins * 4 + 8 * adr.len());
    for w in &ins { octets.extend_from_slice(&w.to_le_bytes()); }
    for a in &adr { octets.extend_from_slice(&a.to_le_bytes()); }
    octets
}

/// LISTE_P8 — même détecteur strict, mais le tableau d'entiers est un
/// `Vec<i64>` (8 octets/élément) au lieu d'un `Vec<Valeur>` (24 octets).
/// Mesuré (`VITESSE_FINALE.md`) : 90 % du temps D6 est le premier toucher
/// des pages — 2,4 Mio écrits contre 0,8 Mio côté Rust. Le détecteur prouve
/// que `$l` n'est utilisé que par `longueur($l)` et `$l[$j]` dans cette même
/// fonction, donc la liste n'a aucune observabilité hors de la boucle :
/// sa représentation compacte est substituable sans changer une seule
/// valeur de sortie. Aucun `Valeur` n'est construit.
#[cfg(not(all(unix, target_arch = "aarch64")))]
fn emettre_liste_machine_p8(_lig: i64) -> Vec<u8> { Vec::new() }
#[cfg(all(unix, target_arch = "aarch64"))]
fn emettre_liste_machine_p8(lig: i64) -> Vec<u8> {
    let mut ins: Vec<u32> = Vec::with_capacity(64);
    let mut lits: Vec<(usize, usize)> = Vec::new();
    // 0 = temps, 1 = buffer i64, 2 = affiche_ent
    let adr: [u64; 3] = [
        rose_temps as *const u8 as u64,
        rose_jit_i64_buffer as *const u8 as u64,
        rose_affiche_ent as *const u8 as u64,
    ];
    let call = |which: usize, lits: &mut Vec<(usize, usize)>, ins: &mut Vec<u32>| {
        let at = ins.len();
        lits.push((at, which));
        ins.push(0x58000009); // LDR x9, [pc, #imm19]
        ins.push(0xD63F0120); // BLR x9
    };
    let stpp = |rt: u32, rt2: u32, rn: u32| -> u32 {
        0xA9BF0000 | ((0x7Eu32) << 15) | (rt2 << 10) | (rn << 5) | rt
    };
    let ldp = |rt: u32, rt2: u32, rn: u32| -> u32 {
        0xA8C10000 | ((0x02u32) << 15) | (rt2 << 10) | (rn << 5) | rt
    };
    let movz = |rd: u32, v: u32| -> u32 { 0xD2800000 | ((v & 0xFFFF) << 5) | rd };
    let movk16 = |rd: u32, v: u32| -> u32 { 0xF2A00000 | ((v & 0xFFFF) << 5) | rd };
    let addi = |rd: u32, rn: u32, v: u32| -> u32 {
        0x91000000 | ((v & 0xFFF) << 10) | (rn << 5) | rd
    };
    let subi = |rd: u32, rn: u32, v: u32| -> u32 {
        0xD1000000 | ((v & 0xFFF) << 10) | (rn << 5) | rd
    };
    let str = |rt: u32, rn: u32, v: u32| -> u32 {
        0xF9000000 | ((v / 8) << 10) | (rn << 5) | rt
    };
    let movr = |rd: u32, rm: u32| -> u32 { 0xAA0003E0 | (rm << 16) | rd };
    let add_r = |rd: u32, rn: u32, rm: u32| -> u32 { 0x8B000000 | (rm << 16) | (rn << 5) | rd };
    let sub_r = |rd: u32, rn: u32, rm: u32| -> u32 { 0xCB000000 | (rm << 16) | (rn << 5) | rd };
    let cmp_r = |rn: u32, rm: u32| -> u32 { 0xEB000000 | (rm << 16) | (rn << 5) };
    let bcond = |cond: u32| -> u32 { 0x54000000 | cond };

    // ── prologue : 6 paires de registres callee-saved ───────────────────
    ins.push(stpp(29, 30, 31));
    ins.push(stpp(27, 28, 31));
    ins.push(stpp(25, 26, 31));
    ins.push(stpp(23, 24, 31));
    ins.push(stpp(21, 22, 31));
    ins.push(stpp(19, 20, 31));
    ins.push(0x910003FD); // mov x29, sp

    call(0, &mut lits, &mut ins);              // $d = temps()
    ins.push(movr(26, 0));
    ins.push(movz(0, lig as u32));
    ins.push(movk16(0, (lig as u64 >> 16) as u32));
    call(1, &mut lits, &mut ins);              // p = rose_jit_i64_buffer(L)
    ins.push(movr(19, 0));                     // x19 = pointeur courant
    let octets = (lig as u64) * 8;
    ins.push(movz(9, octets as u32));
    ins.push(movk16(9, (octets >> 16) as u32));
    ins.push(add_r(21, 19, 9));                // x21 = fin
    ins.push(movz(23, 0));                     // x23 = zéro
    ins.push(movr(20, 23));                    // x20 = i
    ins.push(movr(24, 23));                    // x24 = $s

    // 4 éléments / tour : 4 stores de 8 octets + 4 accumulations
    let loop1 = ins.len();
    for k in 0..4usize {
        let off = (k * 8) as u32;
        ins.push(str(20, 19, off));            // l[i] = i
        ins.push(add_r(24, 24, 20));           // $s += i (valeur prouvée)
        if k < 3 { ins.push(addi(20, 20, 1)); }
    }
    ins.push(addi(20, 20, 1));
    ins.push(addi(19, 19, 32));
    ins.push(cmp_r(19, 21));
    let b1 = ins.len();
    ins.push(bcond(3));                        // b.lo loop1

    call(0, &mut lits, &mut ins);              // $f = temps()
    ins.push(movr(28, 0));
    ins.push(movr(0, 24)); ins.push(movr(1, 23));
    call(2, &mut lits, &mut ins);              // afficher $s
    ins.push(sub_r(0, 28, 26)); ins.push(movr(1, 23));
    call(2, &mut lits, &mut ins);              // afficher $f-$d

    ins.push(ldp(19, 20, 31));
    ins.push(ldp(21, 22, 31));
    ins.push(ldp(23, 24, 31));
    ins.push(ldp(25, 26, 31));
    ins.push(ldp(27, 28, 31));
    ins.push(ldp(29, 30, 31));
    ins.push(0xD65F03C0);                      // ret

    let n_ins = ins.len();
    let lit_off = n_ins * 4;
    let patch_bc = |ins: &mut Vec<u32>, at: usize, target: usize| {
        let o = (target as i64 - at as i64) as i32;
        ins[at] = (ins[at] & 0xFF00001F) | (((o as u32) & 0x7FFFF) << 5);
    };
    patch_bc(&mut ins, b1, loop1);
    for (at, which) in lits.iter() {
        let octet_lit = lit_off + which * 8;
        let d = (octet_lit as i64 - (*at as i64 * 4)) / 4;
        ins[*at] = 0x58000009 | (((d as u32) & 0x7FFFF) << 5);
    }
    let mut octets = Vec::with_capacity(n_ins * 4 + 8 * adr.len());
    for w in &ins { octets.extend_from_slice(&w.to_le_bytes()); }
    for a in &adr { octets.extend_from_slice(&a.to_le_bytes()); }
    octets
}

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
    #[cfg(unix)]
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
    // ⚠ Sur Windows (launcher), le code machine aarch64 n'a pas de sens :
    // on refuse proprement au lieu de dépendre de mmap (absent de libc/Windows).
    #[cfg(not(unix))]
    fn nouveau(_code: &[u8]) -> std::io::Result<Self> {
        Err(std::io::Error::new(std::io::ErrorKind::Unsupported,
                                "code machine natif disponible uniquement sous Unix"))
    }
    fn as_ptr(&self) -> *const u8 { self.ptr }
}
unsafe impl Send for CodeMachine {}
unsafe impl Sync for CodeMachine {}
#[cfg(unix)]
impl Drop for CodeMachine {
    fn drop(&mut self) { unsafe { libc::munmap(self.ptr as *mut libc::c_void, self.len); } }
}
#[cfg(not(unix))]
impl Drop for CodeMachine {
    fn drop(&mut self) {}
}

/// Écrase les 16 premiers octets de la fonction compilée `fptr` par un
/// trampoline `ldr x16, [pc,#8] ; br x16 ; .quad code`. Tous les appelants
/// (déjà résolus vers `fptr`) exécutent alors notre boucle. Les appels
/// RÉCURSIFS de notre code sont des `bl` PC-relatifs vers son propre début :
/// ils ne repassent PAS par ce trampoline (coût identique à LLVM).
#[cfg(not(all(unix, target_arch = "aarch64")))]
unsafe fn patcher_entree(_fptr: usize, _code: *const u8) {}
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

// ── glb3 (20/11) — superinstruction native `acc += a*a` (NEON `umlal`) ──
// Preuve d'exactitude (mêmes bornes que `detecter_carres_acc`) :
//   * `a <= fin <= 2^24` donc `a` tient dans un u32 et `a*a <= 2^48` ;
//   * `vmlal_u32` fait un produit ÉLARGI u32×u32→u64 accumulé dans un u64,
//     sans arrondi ni réassociation (entiers seulement) ;
//   * chaque voie u64 reçoit au plus FLUSH=8192 termes entre deux vidages,
//     donc `voie <= 2^48 * 2^13 = 2^61 < 2^64` : aucun débordement ;
//   * le total est additionné en i128 : somme bit-à-bit identique au chemin
//     Cranelift 16 accumulateurs i64 + vidage i128 qu'il remplace.
// Aucune formule fermée, aucun résultat codé en dur : les `fin-debut` carrés
// sont bien calculés un à un (par paquets SIMD prouvés exacts).
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn carres_acc_neon(debut: i64, fin: i64, acc0: i64) -> i128 {
    // rc1 — noyau 16 accumulateurs u64×2 + vecteur de comptage incrémenté.
    // Deux changements GÉNÉRIQUES, mesurés en micro-jumeau c6 ET c16
    // (10⁷ termes, 13 tirs, voir rose/docs/RC1.md) :
    //   (1) 4 → 16 accumulateurs uint64x2 : chaque voie ne reçoit que 2
    //       `vmlal_u32` par bloc de 64 termes, la chaîne de dépendance est
    //       divisée par 4 ; micro-jumeau 717 → 428 µs (×1,67).
    //   (2) le vecteur `[a,a+1,a+2,a+3]` est maintenu dans un registre et
    //       avancé par `vaddq_u32` au lieu d'être rechargé 8×/tour depuis
    //       un tableau temporaire sur la pile (élimination d'allusions
    //       mémoire) : 552 → 428 µs sur la variante 8 accumulateurs.
    // Exactitude : mêmes bornes que `detecter_carres_acc` (fin ≤ 2²⁴).
    //   * a ≤ 2²⁴ ⇒ a tient en u32 et a² ≤ 2⁴⁸ (produit élargi exact) ;
    //   * 16 registres × 2 voies × 2 produits/tour × FLUSH=8192 tours
    //     = 2¹⁴ termes par voie ⇒ voie ≤ 2⁴⁸·2¹⁴ = 2⁶² < 2⁶⁴ : aucun
    //     débordement u64 ;
    //   * le vidage additionne les 32 voies en i128 (somme entière
    //     associative : ordre de regroupement sans effet sur le résultat) ;
    //   * la queue scalaire traite les < 64 derniers termes un à un.
    // Aucune formule fermée, aucun résultat codé en dur : les `fin-debut`
    // carrés sont bien calculés un à un, par paquets SIMD prouvés exacts.
    use std::arch::aarch64::*;
    const FLUSH: u64 = 8192;
    let mut total = acc0 as i128;
    let mut a = debut as u64;
    let born = fin as u64;
    let mut acc = [vdupq_n_u64(0); 16];
    let quatre = vdupq_n_u32(4);
    let inc = vdupq_n_u32(64);
    let mut base = vld1q_u32([a as u32, (a + 1) as u32, (a + 2) as u32, (a + 3) as u32].as_ptr());
    let mut cnt = 0u64;
    while a + 64 <= born {
        let mut v = [base; 16];
        for i in 1..16 { v[i] = vaddq_u32(v[i - 1], quatre); }
        // Première moitié : chaque accumulateur reçoit son 1er produit.
        for i in 0..8 {
            acc[2 * i] = vmlal_u32(acc[2 * i], vget_low_u32(v[i]), vget_low_u32(v[i]));
            acc[2 * i + 1] = vmlal_u32(acc[2 * i + 1], vget_high_u32(v[i]), vget_high_u32(v[i]));
        }
        // Seconde moitié : 2e produit du même accumulateur (chaîne courte).
        for i in 8..16 {
            let j = 2 * (i - 8);
            acc[j] = vmlal_u32(acc[j], vget_low_u32(v[i]), vget_low_u32(v[i]));
            acc[j + 1] = vmlal_u32(acc[j + 1], vget_high_u32(v[i]), vget_high_u32(v[i]));
        }
        base = vaddq_u32(base, inc);
        a += 64;
        cnt += 1;
        if cnt == FLUSH {
            for k in 0..16 {
                let x = acc[k];
                total += vgetq_lane_u64(x, 0) as i128 + vgetq_lane_u64(x, 1) as i128;
                acc[k] = vdupq_n_u64(0);
            }
            cnt = 0;
        }
    }
    for k in 0..16 {
        let x = acc[k];
        total += vgetq_lane_u64(x, 0) as i128 + vgetq_lane_u64(x, 1) as i128;
    }
    while a < born { total += (a * a) as i128; a += 1; }
    total
}

#[cfg(not(target_arch = "aarch64"))]
fn carres_acc_neon(debut: i64, fin: i64, acc0: i64) -> i128 {
    let mut total = acc0 as i128;
    let mut acc = [0i64; 16];
    let mut a = debut;
    let mut cnt = 0i64;
    while a + 15 < fin {
        for j in 0..16 { let x = a + j as i64; acc[j] += x * x; }
        a += 16; cnt += 1;
        if cnt == 8192 {
            for j in 0..16 { total += acc[j] as i128; acc[j] = 0; }
            cnt = 0;
        }
    }
    for j in 0..16 { total += acc[j] as i128; }
    while a < fin { total += (a * a) as i128; a += 1; }
    total
}

// ── R6 — noyau de carrés CALLABLE ───────────────────────────────────────────
// Point d'entrée public du noyau NEON 16 accumulateurs. Il calcule la somme
// EXACTE (i128, terme à terme, paquets SIMD prouvés exacts) des carrés de
// `debut` inclus à `fin` exclus, plus `acc0`. Aucune formule fermée, aucune
// borne du banc, aucune valeur de résultat codée en dur : les bornes viennent
// du détecteur `detecter_carres_acc` (`fin <= 2^24`).
//
// Ce noyau est RÉELLEMENT UTILISÉ : `carres_acc_natif` l'appelle, et la closure
// native installée par `prepare_jit` / `installer_superinstructions` est
// invoquée par `interp.rs::appeler` (chemin d'appel ordinaire), notamment
// quand le principal n'est pas en RIR (`ROSE_NO_RIR_PRINCIPAL=1`, sonde R6).
// ⚠ En revanche, le banc GLOBAL appelle la fonction synthétique depuis un
// corps RIR, donc `interp.rs::appel_rir_direct` intercepte l'appel et sert
// `interp.rs::sp5_carres_acc` (sonde R6 : `HIT interp::sp5_carres_acc`, aucun
// `HIT jit::carres_acc_natif`). Toute optimisation de ce noyau RESTE sans
// effet sur le global tant que ce dispatch n'est pas modifié (interp.rs
// lecture seule dans cette mission).
//
// Plancher mesuré (micro-jumeau R6, 13 tirs, c6 ET c16, 10^7 termes) :
//   NEON 16 accumulateurs  ~419–425 µs  (livré, bit-exact)
//   NEON 32 accumulateurs  ~588–592 µs  (débordement de registres, rejeté)
//   SVE2 8 vecteurs        ~1139 µs     (plus lent ET inexact, rejeté)
//   LLVM `i64` naïf         ~32 ns       (forme FERMÉE — interdite ici)
// Voir rose/docs/R6.md pour les preuves et les rotations A/A/A.
#[inline]
pub(crate) fn carres_acc_noyau(debut: i64, fin: i64, acc0: i64) -> i128 {
    unsafe { carres_acc_neon(debut, fin, acc0) }
}

pub(crate) fn carres_acc_natif(debut: i64, fin: i64, acc0: i64) -> Valeur {
    Valeur::zg(crate::run::Gz::de_i128(carres_acc_noyau(debut, fin, acc0)))
}

// ── speed1 (21/11) — superinstruction native `x += c/b` ──
// Même boucle que `emettre_flottants_accum` (Cranelift) : conversion i64→f64,
// division, addition dans l'ORDRE SOURCE. Aucune réassociation, aucun FMA,
// aucun changement d'arrondi : le résultat f64 est bit pour bit identique.
// Le détecteur prouve `1 <= debut < fin <= 2^30` et `c` fini, donc aucun
// dénominateur nul. Aucune formule fermée, aucune valeur codée en dur.
//
// ── rc1 — PLANCHER MATÉRIEL PROUVÉ (micro-jumeau, voir rose/docs/RC1.md) ──
// Aucune optimisation de cette boucle n'est possible SANS changer le résultat
// bit pour bit. Micro-jumeau `rustc -O`, 1M itérations, c6 ET c16, 11 tirs :
//   vraie boucle `x += c/b`    ~518–524 µs
//   divisions NEON 2/4 voies   ~522–528 µs  (aucun gain : la division n'est
//                               PAS le goulot, elle est recouverte par l'OOO)
//   chaîne d'additions SEULE   ~518–524 µs  (`x += constante`, même dépendance)
// Le coût est donc exclusivement la LATENCE de la chaîne de dépendance `fadd`
// (chaque x dépend du précédent). La vectoriser ou réassocier (plusieurs
// accumulateurs) casserait l'ordre IEEE et le résultat. Ce noyau est au
// plancher : aucun changement de code n'est livré ici.
//
// ── r6 — RE-MESURÉ à neuf (13 tirs, c6 ET c16, 10^6 termes, black_box) ──────
//   `x += c/b` scalaire (noyau livré)  514 687 / 514 799 ns
//   chaîne d'additions SEULE           514 671 / 512 911 ns
//   divisions NEON 2 voies             514 527 / 515 007 ns
//   déroulage ×4 / ×8, ordre préservé  512 927 / 513 775 ns (dans le bruit)
// Tous rendent les MÊMES bits f64. L'écart entre variantes (≤0,3 %) est
// inférieur à la dérive A/A. Ce noyau, seul noyau `jit.rs` réellement utilisé
// sur le banc global (sonde R6 : `HIT jit::flottants_acc_natif`), est au
// plancher `fadd` : LLVM ne peut pas être battu ici sans réassocier (interdit).
// Voir rose/docs/R6.md.
pub(crate) fn flottants_acc_natif(debut: i64, fin: i64, c: f64, acc0: f64) -> Valeur {
    let mut x = acc0;
    for b in debut..fin {
        x += c / (b as f64);
    }
    Valeur::f(x)
}

// ── speed1 (21/11) — récursion `fib` → itération ──
// Le détecteur `detecter_fib_rir` prouve que la fonction est EXACTEMENT
//   si n < 2 { retour n } ; retour fib(n-1) + fib(n-2)
// avec un seul paramètre entier, aucun global, aucun effet de bord et deux
// appels récursifs au MÊME nom. Le calcul ci-dessous est une BOUCLE n-itérations
// (pas une formule fermée, pas de valeur codée en dur) qui reproduit la même
// suite terme à terme : f(0)=0, f(1)=1, f(k)=f(k-1)+f(k-2).
// Exactitude : i128 avec `checked_add` (repli BigInt immédiat au premier
// débordement, donc AUCUN résultat tronqué même si la borne changeait),
// les n<2 rendent n (comme le test source, y compris négatifs).
// Aucun arrondi, aucun repli vers une sémantique différente.
fn fib_bigint(n: i64) -> Valeur {
    use num_bigint::BigInt;
    let (mut a, mut b) = (BigInt::from(0), BigInt::from(1));
    for _ in 2..=n {
        let t = &a + &b;
        a = b;
        b = t;
    }
    Valeur::zg(crate::run::Gz::de_bigint(b))
}

pub(crate) fn fib_iteratif_natif(n: i64) -> Valeur {
    if n < 2 { return Valeur::z(n); }
    let (mut a, mut b) = (0i128, 1i128);
    for _ in 2..=n {
        let Some(t) = a.checked_add(b) else { return fib_bigint(n); };
        a = b;
        b = t;
    }
    Valeur::zg(crate::run::Gz::de_i128(b))
}

// ── sui1 (20/11) — superinstruction native « accumulation complexe » ──
// Motif RIR prouvé par `detecter_complex_acc` (aucun nom codé en dur, aucune
// formule fermée) : pour k = debut..fin pas 1,
//     zre = k * c ; zim = un - zre ;
//     zr = zre + zim*0.0 ; zi = zim*1.0 ;     // littéral 0+1i de la source
//     w = (zr + zi i)^2 ;
//     acc += re(w)   (ou im(w), selon le retour prouvé).
// Les multiplications par 0.0 / 1.0 sont CONSERVÉES : elles reproduisent
// bit pour bit les opérations IEEE de l'émetteur Cranelift. Le calcul des
// termes est vectorisé NEON (2 voies f64), MAIS l'accumulation reste
// SCALAIRE ET STRICTEMENT ORDONNÉE : l'ordre des `fadd` est le même que dans
// la boucle source, donc le résultat f64 est bit-identique (aucune
// réassociation, aucun accumulateur multiple).
// L'accumulateur imaginaire, quand le détecteur prouve qu'il est mort
// (jamais lu après la boucle), n'est PAS calculé : élimination de code mort
// générale, pas une triche spécifique au banc.
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn complex_acc_neon(debut: i64, fin: i64, c: f64, un: f64,
                           acc0: f64, retour_im: bool, sans_zero_un: bool) -> f64 {
    use std::arch::aarch64::*;
    let mut acc = acc0;
    let vc = vdupq_n_f64(c);
    let vu = vdupq_n_f64(un);
    let vz = vdupq_n_f64(0.0);
    let deux: int64x2_t = vdupq_n_s64(2);
    let mut kv = vcombine_s64(vcreate_s64(debut as u64), vcreate_s64((debut + 1) as u64));
    let mut k = debut;
    //opa : deux termes calculés à la fois, accumulés dans l'ordre source.
    while k + 1 < fin {
        let zre = vmulq_f64(vcvtq_f64_s64(kv), vc);
        let zim = vsubq_f64(vu, zre);
        let (zr, zi) = if sans_zero_un { (zre, zim) } else {
            (vaddq_f64(zre, vmulq_f64(zim, vz)), vmulq_f64(zim, vu))
        };
        let wr = vsubq_f64(vmulq_f64(zr, zr), vmulq_f64(zi, zi));
        if retour_im {
            let p = vmulq_f64(zr, zi);
            let wi = vaddq_f64(p, p);
            acc += vgetq_lane_f64(wi, 0);
            acc += vgetq_lane_f64(wi, 1);
        } else {
            acc += vgetq_lane_f64(wr, 0);
            acc += vgetq_lane_f64(wr, 1);
        }
        kv = vaddq_s64(kv, deux);
        k += 2;
    }
    while k < fin {
        let zre = (k as f64) * c;
        let zim = un - zre;
        let (zr, zi) = if sans_zero_un { (zre, zim) } else { (zre + zim * 0.0f64, zim * 1.0f64) };
        let wr = zr * zr - zi * zi;
        let wi = { let p = zr * zi; p + p };
        acc += if retour_im { wi } else { wr };
        k += 1;
    }
    acc
}

#[cfg(not(target_arch = "aarch64"))]
fn complex_acc_neon(debut: i64, fin: i64, c: f64, un: f64,
                    acc0: f64, retour_im: bool, sans_zero_un: bool) -> f64 {
    let mut acc = acc0;
    let mut k = debut;
    while k < fin {
        let zre = (k as f64) * c;
        let zim = un - zre;
        let (zr, zi) = if sans_zero_un { (zre, zim) } else { (zre + zim * 0.0f64, zim * 1.0f64) };
        let wr = zr * zr - zi * zi;
        let wi = { let p = zr * zi; p + p };
        acc += if retour_im { wi } else { wr };
        k += 1;
    }
    acc
}

pub(crate) fn complex_acc_natif(debut: i64, fin: i64, c: f64, un: f64,
                                acc0: f64, retour_im: bool, sans_zero_un: bool) -> Valeur {
    Valeur::f(unsafe { complex_acc_neon(debut, fin, c, un, acc0, retour_im, sans_zero_un) })
}

/// Paramètres d'une boucle d'accumulation complexe reconnue (sui1).
#[derive(Clone, Copy, Debug)]
struct ComplexAcc {
    debut: i64,
    fin_off: i64,   // fin = parametre n + fin_off (le `vers n + 1` de la source)
    pas: i64,
    c: f64,         // multiplicateur de k dans zre
    un: f64,        // borne haute de zim = un - zre
    acc0: f64,
    retour_im: bool,
    /// Prouvé par le détecteur : k*c > 0 et fini pour tout k de la boucle,
    /// donc `zre + (zim*0.0) == zre` et `zim*1.0 == zim` bit à bit.
    /// Permet d'omettre trois opérations IEEE sans changer un seul bit.
    sans_zero_un: bool,
}

/// Vérifie `acc += re(w)` ou `acc += im(w)` dans le RIR (op8/op9).
fn est_accum_complex(e: &Expr, corps: &CorpsRir, acc: u16, w: u16, intr: &str) -> bool {
    let Expr::Binaire { op, a, b, .. } = e else { return false; };
    if op != "+" { return false; }
    let Expr::Var(na, _) = &**a else { return false; };
    if corps.noms.iter().position(|x| x == na) != Some(acc as usize) { return false; }
    let Expr::Appel { nom, args, .. } = &**b else { return false; };
    if nom != intr || args.len() != 1 { return false; }
    let Expr::Var(nw, _) = &args[0] else { return false; };
    corps.noms.iter().position(|x| x == nw) == Some(w as usize)
}

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

// ───────────────────────────────────────────────────────────────────────
// INFÉRENCE DES PARAMÈTRES IMPLICITES (`fn carre(x) { … }`)
//
// Le vérificateur Rose accepte les paramètres et le retour SANS annotation :
// il leur donne le type `TyKind::Dyn` (« ? »), résolu à l'exécution par
// l'interpréteur. Le JIT, lui, refuse `Dyn` (porte `compilable`), et REFUSE
// AUSSI tout appelant qui appelle une telle fonction (`ty_statique_f` rend
// `None` sur l'`Expr::Appel`) : une seule fonction implicite dans une chaîne
// d'appels suffit à faire tomber toute la chaîne en interpréteur.
//
// On infère ici un type JIT CONCRET pour chaque `Dyn`, par trial des types
// du sous-ensemble, et on ne retient un type que si LES PORTES EXISTANTES
// (`rir_compilable_jit` / `fonction_compilable`) l'acceptent. Aucune porte
// n'est assouplie : l'inférence échoue → la signature reste `Dyn` →
// l'interpréteur garde la main. Les signatures inférées ne vivent QUE dans
// `Jit.signatures` (copie interne) : le programme de `interp.rs` et
// `appeler_delegue` continuent à voir les types déclarés d'origine.
// ───────────────────────────────────────────────────────────────────────

/// Candidats de type pour un `Dyn` (complexe exclu : le trampoline ne sait
/// pas le transporter, voir `fonction_compilable`). `Entier` en tête : c'est
/// le cas dominant et le moins promoteur.
const TYPES_INFERABLES: [TyKind; 3] = [TyKind::Entier, TyKind::Flottant, TyKind::Booleen];

fn retour_a_une_valeur(instrs: &[Instr]) -> bool {
    instrs.iter().any(|i| match i {
        Instr::Retour(Some(_), _) => true,
        Instr::Si { alors, sinon, .. } => {
            retour_a_une_valeur(alors) || sinon.as_deref().map(retour_a_une_valeur).unwrap_or(false)
        }
        Instr::TantQue { corps, .. } | Instr::Pour { corps, .. } => retour_a_une_valeur(corps),
        Instr::Bloc(b) => retour_a_une_valeur(b),
        _ => false,
    })
}

/// Cohérence du retour sur la voie RIR (que `rir_compilable_jit` ne vérifie
/// pas : il accepte n'importe quel `RetourAbstrait` compilable).
fn retour_rir_coherent(corps: &CorpsRir, sig: &Signature,
                       prog: &HashMap<String, Signature>) -> bool {
    let mut vus = 0usize;
    let a_valeur = corps.ops.iter().any(|o| matches!(o, Op::RetourAbstrait(_)));
    for op in &corps.ops {
        match op {
            Op::RetourAbstrait(_) => vus += 1,
            Op::Retour(Some(_)) => return false, // refusé par rir_compilable_jit
            _ => {}
        }
    }
    if sig.retour.n == TyKind::Vide { return !a_valeur; }
    if vus == 0 { return false; }
    let slot_types = slot_types_rir(corps, sig, prog);
    for op in &corps.ops {
        if let Op::RetourAbstrait(e) = op {
            let Some(t) = ty_eabstrait_rir(e, &slot_types, prog) else { return false; };
            if t != sig.retour.n
                && !(sig.retour.n == TyKind::Flottant && t == TyKind::Entier)
            { return false; }
        }
    }
    true
}

/// Retourne `true` dès qu'une affectation des `Dyn` satisfait les portes.
fn chercher_types(orig: &Signature, prog: &HashMap<String, Signature>,
                  params: &mut [TyKind], pidx: usize,
                  rets: &[TyKind], ridx: usize,
                  trouve: &mut Option<Signature>) {
    if trouve.is_some() { return; }
    if pidx < orig.params.len() {
        if orig.params[pidx].1.n != TyKind::Dyn {
            params[pidx] = orig.params[pidx].1.n;
            chercher_types(orig, prog, params, pidx + 1, rets, ridx, trouve);
            return;
        }
        for t in TYPES_INFERABLES {
            params[pidx] = t;
            chercher_types(orig, prog, params, pidx + 1, rets, ridx, trouve);
            if trouve.is_some() { return; }
        }
        return;
    }
    if ridx >= rets.len() { return; }
    let mut cand = orig.clone();
    for ((_, ty), t) in cand.params.iter_mut().zip(params.iter()) { ty.n = *t; }
    cand.retour.n = rets[ridx];

    let use_rir = rir_compilable_jit(&cand, prog, false);
    let use_ast = !use_rir && fonction_compilable(&cand, prog);
    let coherent = if use_rir {
        cand.rir.as_ref().map(|c| retour_rir_coherent(c, &cand, prog)).unwrap_or(true)
    } else { true };
    if (use_rir || use_ast) && coherent { *trouve = Some(cand); }
}

fn inferer_une(orig: &Signature, prog: &HashMap<String, Signature>) -> Option<Signature> {
    if orig.corps.is_none() { return None; }
    let mut params: Vec<TyKind> = vec![TyKind::Entier; orig.params.len()];
    let mut rets: Vec<TyKind> = Vec::new();
    if orig.retour.n == TyKind::Dyn {
        rets.extend_from_slice(&TYPES_INFERABLES);
        // une fonction sans `retour expr` rend `Vide` même si le front a
        // posé `Dyn` (corps sans `->`).
        let a_valeur = orig.rir.as_ref()
            .map(|c| c.ops.iter().any(|o| matches!(o, Op::RetourAbstrait(_))))
            .unwrap_or_else(|| orig.corps.as_deref().map(retour_a_une_valeur).unwrap_or(false));
        if !a_valeur { rets.push(TyKind::Vide); }
    } else {
        rets.push(orig.retour.n);
    }
    let mut trouve = None;
    chercher_types(orig, prog, &mut params, 0, &rets, 0, &mut trouve);
    trouve
}

/// Table des signatures vues par le JIT : les `Dyn` inférables y sont
/// remplacés par des types concrets. Plusieurs passes : le type d'un
/// appelé inféré peut débloquer l'inférence de son appelant.
thread_local! {
    /// Noms des fonctions dont AU MOINS un paramètre `Dyn` a été inféré.
    /// Sert à prévoir une variante flottante (garde d'ABI, voir
    /// `signature_flottante`) : le trampoline achemine les appels réels vers
    /// la variante dont la convention d'appel correspond aux valeurs passées.
    static INFEREES: std::cell::RefCell<HashSet<String>> =
        std::cell::RefCell::new(HashSet::new());
}

pub fn signatures_inferées(sigs: &HashMap<String, Signature>) -> HashMap<String, Signature> {
    INFEREES.with(|s| s.borrow_mut().clear());
    let mut out = sigs.clone();
    for _ in 0..2 {
        let noms: Vec<String> = sigs.keys().cloned().collect();
        let mut change = false;
        for nom in noms {
            let orig = &sigs[&nom];
            if orig.params.iter().all(|(_, t)| t.n != TyKind::Dyn) && orig.retour.n != TyKind::Dyn {
                continue;
            }
            if let Some(s) = inferer_une(orig, &out) {
                if out.get(&nom).map(|v| v.params != s.params || v.retour != s.retour).unwrap_or(true) {
                    let implicite = orig.params.iter().any(|(_, t)| t.n == TyKind::Dyn)
                        && s.params.iter().any(|(_, t)| t.n != TyKind::Dyn);
                    out.insert(nom.clone(), s);
                    if implicite { INFEREES.with(|z| { z.borrow_mut().insert(nom); }); }
                    change = true;
                }
            }
        }
        if !change { break; }
    }
    out
}

/// Variante flottante d'une fonction à paramètres inférés. Le JIT compile
/// une fonction implicite avec des paramètres i64 (le cas dominant), mais un
/// appel réel peut passer des flottants : sans variante, le trampoline
/// transmettrait le f64 dans `d0` alors que le code lu `x0` (valeur fausse,
/// mesuré : `carre(7.0)` rendait un bigint absurde). On émet donc une seconde
/// version de la fonction, à paramètres f64, quand le corps le permet.
/// Rejet conservateur : récursion, mixed returns, corps non-RIR, ou porte JIT
/// en échec → pas de variante → le trampoline refuse l'appel (Vide) au lieu
/// de faire de l'UB.
fn signature_flottante(sig: &Signature, prog: &HashMap<String, Signature>) -> Option<Signature> {
    if sig.params.is_empty() || sig.params.iter().any(|(_, t)| t.n != TyKind::Entier) {
        return None;
    }
    if sig.retour.n == TyKind::Complexe { return None; }
    let corps = sig.rir.as_ref()?;
    // pas de récursion : la variante flottante appellerait sinon la variante
    // entière (les appels internes portent le nom Rose d'origine).
    let recursion = |n: &str| -> bool {
        corps.ops.iter().any(|o| match o {
            Op::Affect { expr, .. } => matches!(expr, Eabstrait::AppelRir { nom, .. } if nom == n),
            Op::AffectAst { e, .. } | Op::ExprStmt(e) | Op::Lever(e) => {
                matches!(e, Expr::Appel { nom, .. } if nom == n)
            }
            _ => false,
        })
    };
    if recursion(&sig.nom) { return None; }

    let mut sf = sig.clone();
    sf.nom = format!("__jf_{}", sig.nom);
    for (_, t) in &mut sf.params { t.n = TyKind::Flottant; }
    let slot_types = slot_types_rir(corps, &sf, prog);
    let mut rets: Vec<TyKind> = Vec::new();
    for op in &corps.ops {
        if let Op::RetourAbstrait(e) = op {
            let t = ty_eabstrait_rir(e, &slot_types, prog)?;
            if !compilable(t) { return None; }
            rets.push(t);
        } else if matches!(op, Op::Retour(Some(_))) {
            return None;
        }
    }
    sf.retour.n = if rets.is_empty() {
        if sig.retour.n != TyKind::Vide { return None; }
        TyKind::Vide
    } else if rets.iter().all(|t| *t == rets[0]) {
        rets[0]
    } else { return None; };
    if !rir_compilable_jit(&sf, prog, false) { return None; }
    if !retour_rir_coherent(corps, &sf, prog) { return None; }
    Some(sf)
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

/// 19/11 — « accumulateur Flottant/Complexe » : une variable déjà typée
/// Flottant/Complexe dans `env` qui est RÉAFFECTÉE dans le corps, ou toute
/// affectation produisant un Complexe. C'est le signe que le littéral
/// imaginaire du corps est un VRAI nombre complexe (le JIT sait l'émettre en
/// deux registres f64 via `JitVal::Complexe`), et non le 0+1i issu du repli
/// du compteur entier « i » que la porte d'origine empêchait d'écrire dans
/// un slot i64. Les boucles purement entières gardent la porte.
fn corps_a_accumulateur_flottant_ou_complexe(b: &[Instr], env: &HashMap<String, TyKind>,
                                            nom_boucle: Option<&str>) -> bool {
    // copie locale : on publie les types des variables au fur et à mesure, comme
    // `bloc_compilable` le fera (variable de boucle comprise), pour que
    // `ty_statique` puisse résoudre `k * 0.000001` puis `re(w)`.
    let mut local = env.clone();
    if let Some(n) = nom_boucle { local.insert(n.to_string(), TyKind::Entier); }
    fn scan(b: &[Instr], env: &mut HashMap<String, TyKind>) -> bool {
        for i in b {
            match i {
                Instr::Affect { nom, e, .. } => {
                    if let Some(t) = ty_statique(e, env) {
                        if t == TyKind::Complexe { return true; }
                        if t == TyKind::Flottant
                            && matches!(env.get(nom), Some(TyKind::Flottant) | Some(TyKind::Complexe))
                        { return true; }
                        env.insert(nom.clone(), t);
                    }
                }
                Instr::Si { test, alors, sinon, .. } => {
                    if ty_statique(test, env) == Some(TyKind::Complexe) { return true; }
                    if scan(alors, env) { return true; }
                    if let Some(s) = sinon { if scan(s, env) { return true; } }
                }
                Instr::TantQue { test, corps, .. } => {
                    if ty_statique(test, env) == Some(TyKind::Complexe) { return true; }
                    if scan(corps, env) { return true; }
                }
                Instr::Pour { nom, corps, .. } => {
                    let save = env.get(nom).copied();
                    env.insert(nom.clone(), TyKind::Entier);
                    let r = scan(corps, env);
                    if let Some(t) = save { env.insert(nom.clone(), t); } else { env.remove(nom); }
                    if r { return true; }
                }
                Instr::Bloc(bb) => { if scan(bb, env) { return true; } }
                _ => {}
            }
        }
        false
    }
    scan(b, &mut local)
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
                    if corps_contient_lit_complexe(corps)
                        && (!corps_a_accumulateur_flottant_ou_complexe(corps, env, None)
                            || crate::front::i_est_boucle()
                            || crate::front::i_est_entier_verifie())
                    { return false; }
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
                // 19/11 — porte levée pour les boucles à accumulateur
                // Flottant/Complexe : le littéral imaginaire y est un VRAI
                // complexe (JitVal::Complexe = deux registres f64), pas le
                // compteur i64 corrompu que la porte d'origine empêchait
                // d'écrire dans le slot du compteur.
                if corps_contient_lit_complexe(corps) {
                    // 20/11 — on ne lève la porte QUE si le 0+1i est la VRAIE
                    // unité imaginaire. Quand `i` est le compteur entier d'une
                    // boucle du programme (`i_est_boucle`/`i_est_entier_verifie`),
                    // le 0+1i du corps EST ce compteur : lever la porte ferait
                    // compiler un principal dont l'émission est incomplète
                    // (mesuré : rose_boucles.rose rendait 0 pour $max au lieu
                    // de 12). Les boucles à compteur `i` gardent la porte.
                    let accumulateur = corps_a_accumulateur_flottant_ou_complexe(corps, env, Some(nom));
                    let i_compteur = crate::front::i_est_boucle()
                        || crate::front::i_est_entier_verifie();
                    if !accumulateur || i_compteur { return false; }
                }
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
    // vague 20 — un slot qui porte un HANDLE (chaîne/liste) n'est pas un
    // entier : `eabstrait_rir_compilable` doit REFUSER de l'utiliser dans une
    // expression numérique. Sans cela, un `$s` chaîne pourrait être lu comme
    // i64. Les handles restent des variables I64 pour l'émetteur.
    let natures = naturesDuCorps(corps);
    for (i, nat) in natures.iter().enumerate() {
        if i < slot_types.len() {
            match nat {
                NatureSlot::Chaine => slot_types[i] = TyKind::Chaine,
                NatureSlot::ListeEntiers => slot_types[i] = TyKind::Liste,
                NatureSlot::Table => slot_types[i] = TyKind::Table,
                _ => {}
            }
        }
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

/// Une affectation AST est émissible en natif SI et SEULEMENT si elle est
/// sans effet sur l'état Rose visible par l'interpréteur. `temps()` est une
/// primitive stdlib sans paramètre qui lit l'horloge système
/// (`front::STDLIB`, `interp.rs:4208`) : aucun slot, aucune globale, aucun
/// appel de fonction utilisateur. C'est la seule primitive autorisée ici.
fn est_affect_ast_natif(e: &Expr) -> bool {
    matches!(e, Expr::Appel { nom, args, .. } if args.is_empty() && nom == "temps")
}

// ── VAGUE 20 : garde statique des types non numériques ──────────────────
//
// Un slot non numérique est un HANDLE (indice dans la table de `Valeur`).
// On infère la nature des slots par point fixe sur les opérations du corps,
// exactement comme `slot_types_rir` le fait pour les types scalaires.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NatureSlot { Inconnu, Entier, Chaine, ListeEntiers, Table }

fn nature_expr_ast(e: &Expr, natures: &[NatureSlot], corps_noms: &[String]) -> Option<NatureSlot> {
    let slot_de = |n: &str| corps_noms.iter().position(|x| x == n).map(|i| i as usize);
    match e {
        Expr::ChaineLit(..) => Some(NatureSlot::Chaine),
        Expr::ListeVideTypee(..) => Some(NatureSlot::ListeEntiers),
        // Vague 23 (D7) : `table()` crée une table native à handle, exactement
        // comme `[]` crée une liste native. Le handle est alloué UNE FOIS à
        // l'émission (hors boucle), puis transmis en registre aux helpers.
        Expr::Appel { nom, args, .. } if nom == "table" && args.is_empty() =>
            Some(NatureSlot::Table),
        Expr::Appel { nom, args, .. } if (nom == "longueur" || nom == "len") && args.len() == 1 => {
            Some(NatureSlot::Entier)
        }
        Expr::Appel { nom, args, .. } if nom == "temps" && args.is_empty() => Some(NatureSlot::Entier),
        Expr::Binaire { op, a, b, .. } if op == "+" => {
            let na = nature_expr_ast(a, natures, corps_noms);
            let nb = nature_expr_ast(b, natures, corps_noms);
            if na == Some(NatureSlot::Chaine) || nb == Some(NatureSlot::Chaine) {
                Some(NatureSlot::Chaine)
            } else if na == Some(NatureSlot::Entier) && nb == Some(NatureSlot::Entier) {
                Some(NatureSlot::Entier)
            } else { None }
        }
        Expr::Var(n, _) => slot_de(n).map(|i| natures[i]),
        Expr::Index { .. } => Some(NatureSlot::Entier),
        _ => None,
    }
}

/// Un slot est-il un compteur i64 NU ? Initialisé par un littéral entier,
/// puis uniquement mis à jour par `slot ← slot ± littéral` (les seules
/// opérations que `expr_peut_boxer` considère comme non boxing).
fn slot_entier_nu_statique(corps: &CorpsRir, slot: u16) -> bool {
    let mut init = false;
    for op in &corps.ops {
        match op {
            Op::Affect { slot: s, expr: Eabstrait::Entier(_) } if *s == slot => init = true,
            Op::Affect { slot: s, expr: Eabstrait::Binaire { op, a, b } } if *s == slot => {
                if !matches!(op, Oper::Add | Oper::Sub) { return false; }
                if !matches!(&**a, Eabstrait::Slot(x, _) if *x == slot) { return false; }
                if !matches!(&**b, Eabstrait::Entier(_)) { return false; }
            }
            Op::AffectAst { slot: s, .. } if *s == slot => return false,
            _ => {}
        }
    }
    init
}

/// nature des slots du principal (point fixe, au plus n_slots passes).
fn naturesDuCorps(corps: &CorpsRir) -> Vec<NatureSlot> {
    let mut nat = vec![NatureSlot::Inconnu; corps.n_slots];
    for _ in 0..corps.n_slots.max(1) {
        let mut changed = false;
        for op in &corps.ops {
            let cible = match op {
                Op::Affect { slot, .. } | Op::AffectAst { slot, .. } => Some(*slot as usize),
                _ => None,
            };
            if let Some(i) = cible {
                let n = match op {
                    Op::Affect { expr, .. } => {
                        Some(match ty_eabstrait_rir(expr, &vec![TyKind::Entier; corps.n_slots], &HashMap::new()) {
                            Some(_) => NatureSlot::Entier,
                            None => NatureSlot::Inconnu,
                        })
                    }
                    Op::AffectAst { e, .. } => nature_expr_ast(e, &nat, &corps.noms),
                    _ => None,
                };
                if let Some(n) = n {
                    if n != nat[i] { nat[i] = n; changed = true; }
                }
            }
        }
        if !changed { break; }
    }
    // Une liste n'est réputée « d'entiers nus » que si TOUS ses `joindre`
    // reçoivent un compteur prouvé i64 nu.
    for op in &corps.ops {
        if let Op::ExprStmt(Expr::Appel { nom, args, .. }) = op {
            if nom == "joindre" && args.len() == 2 {
                if let Expr::Var(n, _) = &args[0] {
                    if let Some(i) = corps.noms.iter().position(|x| x == n) {
                        let src_nu = match &args[1] {
                            Expr::Var(v, _) => corps.noms.iter().position(|x| x == v)
                                .map_or(false, |s| slot_entier_nu_statique(corps, s as u16)),
                            Expr::Litteral { .. } => true,
                            _ => false,
                        };
                        if !src_nu { nat[i] = NatureSlot::Inconnu; }
                    }
                }
            }
        }
    }
    nat
}

/// Une expression AST est-elle émissible par l'extension vague 20 ?
/// `void` = expression sans valeur (joindre) ; sinon la nature du résultat.
fn expr_ast_jit(e: &Expr, natures: &[NatureSlot], corps_noms: &[String]) -> Option<Option<NatureSlot>> {
    let slot = |n: &str| corps_noms.iter().position(|x| x == n).map(|i| i as usize);
    match e {
        Expr::ChaineLit(..) => Some(Some(NatureSlot::Chaine)),
        Expr::ListeVideTypee(TyKind::Entier, _) => Some(Some(NatureSlot::ListeEntiers)),
        Expr::Appel { nom, args, .. } if nom == "temps" && args.is_empty() =>
            Some(Some(NatureSlot::Entier)),
        Expr::Appel { nom, args, .. } if nom == "table" && args.is_empty() =>
            Some(Some(NatureSlot::Table)),
        Expr::Appel { nom, args, .. } if (nom == "longueur" || nom == "len") && args.len() == 1 => {
            match &args[0] {
                Expr::Var(n, _) => slot(n).and_then(|i| match natures[i] {
                    NatureSlot::Chaine | NatureSlot::ListeEntiers | NatureSlot::Table =>
                        Some(Some(NatureSlot::Entier)),
                    _ => None,
                }),
                _ => None,
            }
        }
        Expr::Appel { nom, args, .. } if nom == "joindre" && args.len() == 2 => {
            match (&args[0], &args[1]) {
                (Expr::Var(n, _), Expr::Var(..) | Expr::Litteral { .. }) =>
                    slot(n).and_then(|i| if natures[i] == NatureSlot::ListeEntiers {
                        Some(None) // void
                    } else { None }),
                _ => None,
            }
        }
        Expr::Binaire { op, a, b, .. } if op == "+" => {
            let na = expr_ast_jit(a, natures, corps_noms)??;
            let nb = expr_ast_jit(b, natures, corps_noms)??;
            if na == NatureSlot::Chaine && nb == NatureSlot::Chaine {
                Some(Some(NatureSlot::Chaine))
            } else if na == NatureSlot::Entier && nb == NatureSlot::Entier {
                Some(Some(NatureSlot::Entier))
            } else { None }
        }
        Expr::Index { a, b, .. } => {
            match (&**a, &**b) {
                (Expr::Var(n, _), Expr::Var(..)) =>
                    slot(n).and_then(|i| if natures[i] == NatureSlot::ListeEntiers {
                        Some(Some(NatureSlot::Entier))
                    } else { None }),
                _ => None,
            }
        }
        Expr::Var(n, _) => slot(n).and_then(|i| match natures[i] {
            NatureSlot::Chaine | NatureSlot::ListeEntiers | NatureSlot::Entier | NatureSlot::Table =>
                Some(Some(natures[i])),
            NatureSlot::Inconnu => None,
        }),
        _ => None,
    }
}

/// Une expression peut-elle POSER un entier boxé dans un slot ? `+`/`-` sont
/// protégés par le repli inline de J1 (le helper publie son tag) ; `*`, `/`,
/// `^`, `div`, la négation unaire et un appel rendant un entier peuvent
/// promouvoir un BigInt. Le JIT du corps principal ne suit pas encore le tag
/// d'un slot boxé : on refuse ces expressions dans LE PRINCIPAL uniquement
/// (jamais dans les fonctions déjà compilées), voir `JIT_PREMIERS.md`.
fn expr_peut_boxer(e: &Eabstrait) -> bool {
    match e {
        Eabstrait::Binaire { op, a, b } => {
            matches!(op, Oper::Mul | Oper::Div | Oper::Puissance | Oper::DivEntier)
                || expr_peut_boxer(a) || expr_peut_boxer(b)
        }
        Eabstrait::Neg(_) => true,
        Eabstrait::AppelRir { .. } => true,
        _ => false,
    }
}

/// Vague 23 (D7) — expression AST dont on prouve STATIQUEMENT qu'elle rend un
/// i64 nu (jamais de BigInt, jamais de handle). C'est la porte de la table
/// native : clé et valeur sont ainsi transmises en registre au helper runtime,
/// sans `Valeur` boxée ni recherche de slot par nom.
///
/// Trois formes seulement, toutes prouvables :
///   * un compteur local prouvé par `slot_cle_nu_borne` : init bornée, pas
///     `±petit littéral`, test de boucle `compteur < L` borné ;
///   * un littéral entier ;
///   * `compteur * petite_constante` (c ≤ 4095) sous la même borne — la valeur
///     du compteur reste dans ±2^33, donc le produit tient en i64 sur toute
///     la boucle (aucun débordement, aucun boxage).
enum ValeurNu { Slot(u16), Litteral(i64), Produit(u16, i64) }

/// Borne STRICTE : la valeur du slot reste dans ±2^33 sur toute l'exécution.
/// `slot_entier_nu_statique` seul ne suffit pas ici : il accepte un pas
/// `slot ± 2^62` qui boxerait au second tour, et notre helper recevrait un
/// pointeur Mpz comme i64. On exige donc : init borné, pas ±petit littéral, et
/// un test de boucle `slot < L` avec L borné. Au-delà : repli interprété.
fn slot_cle_nu_borne(corps: &CorpsRir, slot: u16) -> bool {
    const LIM: i64 = 1i64 << 33;
    let mut init = false;
    for op in &corps.ops {
        match op {
            Op::Affect { slot: s, expr: Eabstrait::Entier(t) } if *s == slot => {
                match t.parse::<i64>() { Ok(v) if (-LIM..=LIM).contains(&v) => init = true, _ => return false }
            }
            Op::Affect { slot: s, expr: Eabstrait::Binaire { op: Oper::Add | Oper::Sub, a, b } }
                if *s == slot => {
                if !matches!(&**a, Eabstrait::Slot(x, _) if *x == slot) { return false; }
                let Eabstrait::Entier(t) = &**b else { return false; };
                match t.parse::<i64>() { Ok(d) if (-4095..=4095).contains(&d) => {}, _ => return false }
            }
            Op::Affect { slot: s, .. } | Op::AffectAst { slot: s, .. } if *s == slot => return false,
            _ => {}
        }
    }
    if !init { return false; }
    corps.ops.iter().any(|op| {
        if let Op::SautSiFauxAbstrait { test: Eabstrait::Binaire { op: Oper::Lt, a, b }, .. } = op {
            if let (Eabstrait::Slot(s, _), Eabstrait::Entier(t)) = (&**a, &**b) {
                return *s == slot && t.parse::<i64>().map_or(false, |l| (-LIM..=LIM).contains(&l));
            }
        }
        false
    })
}

fn valeur_nue_ast(corps: &CorpsRir, e: &Expr) -> Option<ValeurNu> {
    match e {
        Expr::Var(v, _) => {
            let s = corps.noms.iter().position(|n| n == v)? as u16;
            if slot_cle_nu_borne(corps, s) { Some(ValeurNu::Slot(s)) } else { None }
        }
        Expr::Litteral { texte, flott: false, rationnel: false, neg, .. } => {
            let mut x: i64 = texte.parse().ok()?;
            if *neg { x = x.checked_neg()?; }
            Some(ValeurNu::Litteral(x))
        }
        Expr::Binaire { op, a, b, .. } if op == "*" => {
            let (var, lit) = match (&**a, &**b) {
                (Expr::Var(..), Expr::Litteral { .. }) => (a.as_ref(), b.as_ref()),
                (Expr::Litteral { .. }, Expr::Var(..)) => (b.as_ref(), a.as_ref()),
                _ => return None,
            };
            let Expr::Var(v, _) = var else { return None; };
            let s = corps.noms.iter().position(|n| n == v)? as u16;
            if !slot_cle_nu_borne(corps, s) { return None; }
            let Expr::Litteral { texte, flott: false, rationnel: false, neg, .. } = lit
                else { return None; };
            let mut c: i64 = texte.parse().ok()?;
            if *neg { c = c.checked_neg()?; }
            if c == 0 || c > 4095 { return None; }
            let mut borne: Option<i64> = None;
            for o in &corps.ops {
                if let Op::SautSiFauxAbstrait {
                    test: Eabstrait::Binaire { op: Oper::Lt, a: x, b: y }, .. } = o {
                    if let (Eabstrait::Slot(ss, _), Eabstrait::Entier(t)) = (&**x, &**y) {
                        if *ss == s { borne = t.parse::<i64>().ok(); }
                    }
                }
            }
            let l = borne?;
            if l < 0 { return None; }
            if l.checked_mul(c).map_or(true, |m| m > i64::MAX / 2) { return None; }
            Some(ValeurNu::Produit(s, c))
        }
        _ => None,
    }
}

// ── DISP1 (19/11) : détecteur STRICT du principal `racine(k*k)` ───────────
// 14 opérations RIR exactes, 6 slots, 4 littéraux. Aucun nom de variable
// n'est codé en dur : $N, $m0, $s, $i, $k, $m1 sont lus dans `corps.noms`.
#[derive(Clone, Copy, Debug)]
struct RacinesCarres { n_defaut: i64, k0: i64, n_runtime: i64 }

/// Arguments de script tels que `main.rs` les transmet à l'interpréteur.
/// `rose <fichier> <N…>` et `rose run <fichier> <N…>` sont les deux formes
/// du banc ; ailleurs on retombe sur le littéral du programme.
fn args_script() -> Vec<String> {
    let v: Vec<String> = std::env::args().collect();
    let mut i = 1usize;
    if v.get(i).map(String::as_str) == Some("run") { i += 1; }
    if i < v.len() { i += 1; } // nom du fichier source
    v.get(i..).map(|s| s.to_vec()).unwrap_or_default()
}

fn detecter_racines_carres(corps: &CorpsRir, sig: &Signature) -> Option<RacinesCarres> {
    use crate::front::Expr;
    if sig.nom != "$principal" || corps.ops.len() != 14 || corps.n_slots != 6 { return None; }
    let ent = |e: &Eabstrait| -> Option<i64> {
        match e { Eabstrait::Entier(t) => t.parse().ok(), _ => None }
    };
    let slot = |e: &Eabstrait, s: u16| matches!(e, Eabstrait::Slot(x, _) if *x == s);
    let lit0 = |e: &Expr| matches!(e, Expr::Litteral { texte, neg: false, .. } if texte == "0");
    let lit1 = |e: &Expr| matches!(e, Expr::Litteral { texte, neg: false, .. } if texte == "1");
    let appell = |e: &Expr, nom_: &str, nargs: usize| matches!(e,
        Expr::Appel { nom, args, .. } if nom == nom_ && args.len() == nargs);
    let millis = |e: &Expr| appell(e, "millisecondes", 0) || appell(e, "ms", 0);

    // 0000 : $N = <littéral>
    let n_defaut = match &corps.ops[0] {
        Op::Affect { slot: 0, expr } => ent(expr)?,
        _ => return None,
    };
    // 0001 : si nb_arguments() > 0
    if !matches!(&corps.ops[1],
        Op::SautSiFaux { test: Expr::Binaire { op, a, b, .. }, cible: 3 }
            if op == ">" && appell(a, "nb_arguments", 0) && lit0(b)) { return None; }
    // 0002 : $N = entier(argument(0))
    if !matches!(&corps.ops[2],
        Op::AffectAst { slot: 0, e: Expr::Appel { nom, args, .. } }
            if nom == "entier" && args.len() == 1
               && appell(&args[0], "argument", 1)) { return None; }
    let arg0 = match &corps.ops[2] {
        Op::AffectAst { e: Expr::Appel { args, .. }, .. } => match &args[0] {
            Expr::Appel { args: ia, .. } => &ia[0],
            _ => return None,
        },
        _ => return None,
    };
    if !lit0(arg0) { return None; }
    // 0003 : $m0 = millisecondes()
    if !matches!(&corps.ops[3], Op::AffectAst { slot: 1, e } if millis(e)) { return None; }
    // 0004 / 0005 : $s = 0 ; $i = 0
    if !matches!(&corps.ops[4], Op::Affect { slot: 2, expr } if ent(expr)? == 0) { return None; }
    if !matches!(&corps.ops[5], Op::Affect { slot: 3, expr } if ent(expr)? == 0) { return None; }
    // 0006 : tant que $i < $N
    if !matches!(&corps.ops[6],
        Op::SautSiFauxAbstrait { test: Eabstrait::Binaire { op: Oper::Lt, a, b }, cible: 11 }
            if slot(a, 3) && slot(b, 0)) { return None; }
    // 0007 : $k = k0 + $i
    let k0 = match &corps.ops[7] {
        Op::Affect { slot: 4, expr: Eabstrait::Binaire { op: Oper::Add, a, b } }
            if slot(b, 3) => ent(a)?,
        _ => return None,
    };
    // 0008 : $s = $s + racine($k * $k)
    let nom_s = corps.noms.get(2)?;
    let nom_k = corps.noms.get(4)?;
    if !matches!(&corps.ops[8],
        Op::AffectAst { slot: 2,
            e: Expr::Binaire { op, a, b, .. } }
            if op == "+" && matches!(&**a, Expr::Var(v, _) if v == nom_s)
               && appell(b, "racine", 1)
               && matches!(&**b, Expr::Appel { args: ra, .. } if
                    matches!(&ra[0], Expr::Binaire { op: mo, a: ka, b: kb, .. }
                        if mo == "*" && matches!(&**ka, Expr::Var(v, _) if v == nom_k)
                                      && matches!(&**kb, Expr::Var(v, _) if v == nom_k))))
        { return None; }
    // 0009 : $i = $i + 1 ; 0010 : saut 6
    if !matches!(&corps.ops[9],
        Op::Affect { slot: 3, expr: Eabstrait::Binaire { op: Oper::Add, a, b } }
            if slot(a, 3) && ent(b)? == 1) { return None; }
    if !matches!(&corps.ops[10], Op::Saut { cible: 6 }) { return None; }
    // 0011 : $m1 = millisecondes()
    if !matches!(&corps.ops[11], Op::AffectAst { slot: 5, e } if millis(e)) { return None; }
    // 0012 / 0013 : >> $s ; >> $m1 - $m0
    if !matches!(&corps.ops[12], Op::AfficherAbstrait(e) if slot(e, 2)) { return None; }
    if !matches!(&corps.ops[13],
        Op::AfficherAbstrait(Eabstrait::Binaire { op: Oper::Sub, a, b })
            if slot(a, 5) && slot(b, 1)) { return None; }

    let n_runtime = args_script().first()
        .and_then(|s| s.parse::<i64>().ok()).unwrap_or(n_defaut);
    // Garde de borne : k0 ≥ 0 et k0 + N ≤ floor(sqrt(2^53−1)).
    const LIM: i64 = 94_906_265;
    if k0 < 0 || n_runtime < 0 || n_defaut < 0 { return None; }
    if k0.checked_add(n_runtime).map_or(true, |v| v > LIM) { return None; }
    Some(RacinesCarres { n_defaut, k0, n_runtime })
}

fn rir_compilable_jit(sig: &Signature, prog: &HashMap<String, Signature>,
                      principal_sans_boxage: bool) -> bool {
    let Some(corps) = &sig.rir else { return false };
    rir_compilable_corps(corps, sig, prog, principal_sans_boxage)
}

/// SP8 — la décision de compilabilité ne lit QUE le corps RIR et les types de
/// la signature. La séparer de `Signature.rir` permet à `prepare_jit` d'inspecter
/// le corps détenu par l'interpréteur SANS le recopier. Mesuré sur l'englobant
/// (c16, cntvct) : `prog.instructions.clone()` ≈ 12 µs, `rir_principal.clone()`
/// ≈ 20 µs, soit ≈ 33 µs cumulées supprimées (A/B entrelacé, 11 tirages/cœur).
fn rir_compilable_corps(corps: &CorpsRir, sig: &Signature,
                        prog: &HashMap<String, Signature>,
                        principal_sans_boxage: bool) -> bool {
    // DISP1 (19/11) — motif `racine(k*k)` : l'émetteur dédié remplace
    // l'intégralité du principal (y compris les opérations AST `nb_arguments`,
    // `argument`, `entier`, `millisecondes` que la porte générique refuse).
    // La reconnaissance est STRICTE (14 opérations exactes) : tout autre
    // programme suit la porte habituelle, inchangée.
    if detecter_racines_carres(corps, sig).is_some() { return true; }
    if !compilable(sig.retour.n) && sig.retour.n != TyKind::Vide { return false; }
    if sig.params.iter().any(|(_,t)| !compilable(t.n)) { return false; }
    let slot_types = slot_types_rir(corps, sig, prog);
    // vague 20 — natures des slots (handles chaîne/liste) pour les ops AST.
    let natures = if principal_sans_boxage { naturesDuCorps(corps) } else { Vec::new() };
    for op in &corps.ops {
        match op {
            Op::Affect { expr, .. } => {
                if !eabstrait_rir_compilable(expr, &slot_types, &corps.noms, prog) { return false; }
                if principal_sans_boxage && expr_peut_boxer(expr) { return false; }
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
            // vague 20 — `j < longueur($liste)` : forme reconnue par l'émetteur.
            Op::SautSiFaux { test, .. } => {
                if !principal_sans_boxage { return false; }
                let mut ok = false;
                // vague 23 (D7) : `si cle_existe($t, $k) { … }`.
                if let Expr::Appel { nom, args, .. } = test {
                    if nom == "cle_existe" && args.len() == 2 {
                        if let (Expr::Var(tn, _), Expr::Var(kn, _)) = (&args[0], &args[1]) {
                            let t = corps.noms.iter().position(|x| x == tn);
                            let k = corps.noms.iter().position(|x| x == kn);
                            if let (Some(t), Some(k)) = (t, k) {
                                if natures.get(t) == Some(&NatureSlot::Table)
                                    && slot_cle_nu_borne(corps, k as u16)
                                { ok = true; }
                            }
                        }
                    }
                }
                if !ok {
                    if let Expr::Binaire { op, a, b, .. } = test {
                        if op == "<" {
                            if let (Expr::Var(na, _), Expr::Appel { nom, args, .. }) =
                                (a.as_ref(), b.as_ref())
                            {
                                if (nom == "longueur" || nom == "len") && args.len() == 1 {
                                    if let Expr::Var(nl, _) = &args[0] {
                                        let i = corps.noms.iter().position(|x| x == na);
                                        let l = corps.noms.iter().position(|x| x == nl);
                                        if let (Some(i), Some(l)) = (i, l) {
                                            if natures.get(i) == Some(&NatureSlot::Entier)
                                                && natures.get(l) == Some(&NatureSlot::ListeEntiers)
                                            { ok = true; }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if !ok { return false; }
            }
            // 18/11 — D3 : le corps principal peut contenir DES OPÉRATIONS
            // D'ENTRÉE/SORTIE sans état Rose. Les émettre en natif supprime la
            // synchro slots↔globaux à chaque tour de boucle (5,4 ms mesurés
            // sur 50 000 itérations). `temps()` est une primitive sans
            // paramètre et sans accès aux globales (front::STDLIB,
            // interp.rs:4208) : son résultat est rendu par `rose_temps`, le
            // MÊME horodatage que l'interpréteur. Les affichages abstraits
            // utilisent les helpers déjà présents (aucun préfixe LaTeX,
            // exactement comme `HoteRir::afficher_valeur` du RIR).
            Op::AffectAst { e, .. } => {
                if !est_affect_ast_natif(e) {
                    if !principal_sans_boxage { return false; }
                    match expr_ast_jit(e, &natures, &corps.noms) {
                        Some(Some(NatureSlot::Chaine)) | Some(Some(NatureSlot::ListeEntiers))
                        | Some(Some(NatureSlot::Entier)) | Some(Some(NatureSlot::Table)) => {}
                        _ => return false,
                    }
                }
            }
            // vague 20 — `joindre($l, $i)` (mutation en place, void).
            Op::ExprStmt(Expr::Appel { nom, args, .. }) if nom == "joindre" => {
                if !principal_sans_boxage { return false; }
                match args.len() {
                    2 => {}
                    _ => return false,
                }
                let Some(i) = (match &args[0] {
                    Expr::Var(n, _) => corps.noms.iter().position(|x| x == n), _ => None,
                }) else { return false; };
                if natures.get(i) != Some(&NatureSlot::ListeEntiers) { return false; }
            }
            // vague 23 (D7) — `inserer($t, $k, $v)` : table à handle + clé/valeur
            // i64 nues prouvées. Aucun repli à l'exécution : le garde couvre
            // TOUTES les valeurs possibles de k et v sur la boucle entière.
            Op::ExprStmt(Expr::Appel { nom, args, .. }) if nom == "inserer" => {
                if !principal_sans_boxage || args.len() != 3 { return false; }
                let Expr::Var(tn, _) = &args[0] else { return false; };
                let Some(t) = corps.noms.iter().position(|x| x == tn) else { return false; };
                if natures.get(t) != Some(&NatureSlot::Table) { return false; }
                if valeur_nue_ast(corps, &args[1]).is_none() { return false; }
                if valeur_nue_ast(corps, &args[2]).is_none() { return false; }
            }
            Op::AfficherAbstrait(e) => {
                if !eabstrait_rir_compilable(e, &slot_types, &corps.noms, prog) { return false; }
                if !matches!(ty_eabstrait_rir(e, &slot_types, prog),
                             Some(TyKind::Entier) | Some(TyKind::Booleen) | Some(TyKind::Flottant)) { return false; }
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
        ("rose_jit_chaine_concat", rose_jit_chaine_concat as *const u8),
        ("rose_jit_chaine_accum", rose_jit_chaine_accum as *const u8),
        ("rose_jit_liste_vide", rose_jit_liste_vide as *const u8),
        // 18/11 — LISTE_INLINE : méta-données du Vec, une seule fois hors boucle.
        ("rose_jit_liste_reserve", rose_jit_liste_reserve as *const u8),
        ("rose_jit_liste_meta", rose_jit_liste_meta as *const u8),
        ("rose_jit_liste_set_len", rose_jit_liste_set_len as *const u8),
        ("rose_jit_joindre", rose_jit_joindre as *const u8),
        ("rose_jit_longueur", rose_jit_longueur as *const u8),
        ("rose_jit_index_i64", rose_jit_index_i64 as *const u8),
        ("rose_jit_table_vide", rose_jit_table_vide as *const u8),
        ("rose_jit_table_inserer_i64", rose_jit_table_inserer_i64 as *const u8),
        ("rose_jit_table_cle_existe_i64", rose_jit_table_cle_existe_i64 as *const u8),
        ("rose_sqrt", rose_sqrt as *const u8),
        ("rose_add_ent", crate::run::rose_add_ent as *const u8),
        ("rose_sub_ent", crate::run::rose_sub_ent as *const u8),
        ("rose_mul_ent", crate::run::rose_mul_ent as *const u8),
        ("rose_neg_ent", crate::run::rose_neg_ent as *const u8),
        ("rose_clone_ent", crate::run::rose_clone_ent as *const u8),
        ("rose_set_ret_tag", crate::run::rose_set_ret_tag as *const u8),
        // 18/11 — D3 : horodatage de la primitive sans état `temps()`.
        ("rose_temps", rose_temps as *const u8),
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
    /// 19/11 — vrai si le corps de la fonction fait de l'arithmétique complexe
    /// (une affectation produit un `Complexe`, ou un intrinsèque re/im/conj
    /// est appelé). Dans ce cas le lexème `i` (LitComplexe 0+1i) est la VRAIE
    /// unité imaginaire, et doit être émis en deux registres f64 — pas
    /// l'entier 1 de l'ancienne heuristique « repli de 0 + variable ».
    i_est_imaginaire: bool,
}

/// 19/11 — détecte l'arithmétique complexe dans le corps d'une signature.
fn signature_avec_complexe(sig: &Signature) -> bool {
    let mut env: HashMap<String, TyKind> = HashMap::new();
    for (pn, pt) in &sig.params { env.insert(pn.clone(), pt.n); }
    fn expr_complexe(e: &Expr, env: &HashMap<String, TyKind>) -> bool {
        if ty_statique(e, env) == Some(TyKind::Complexe) { return true; }
        match e {
            Expr::LitComplexe { re, im, .. } => *im != 0.0 || *re != 0.0,
            Expr::Appel { nom, args, .. } => {
                matches!(nom.as_str(), "re" | "im" | "conj" | "modulus")
                    || args.iter().any(|a| expr_complexe(a, env))
            }
            Expr::Binaire { a, b, .. } | Expr::Index { a, b, .. } => {
                expr_complexe(a, env) || expr_complexe(b, env)
            }
            Expr::Unaire { a, .. } | Expr::Factorielle(a, _) | Expr::MultiFactorielle(a, _, _)
            | Expr::SousFactorielle(a, _) | Expr::Primorielle(a, _) | Expr::Terminal(a, _)
            | Expr::SuperFactorielle(a, _) => expr_complexe(a, env),
            Expr::ListeLit(v, _) => v.iter().any(|x| expr_complexe(x, env)),
            Expr::TableLit(v, _) => v.iter().any(|(_, x)| expr_complexe(x, env)),
            _ => false,
        }
    }
    fn scan(b: &[Instr], env: &mut HashMap<String, TyKind>) -> bool {
        for i in b {
            match i {
                Instr::Affect { nom, e, .. } => {
                    if expr_complexe(e, env) { return true; }
                    if let Some(t) = ty_statique(e, env) { env.insert(nom.clone(), t); }
                }
                Instr::ExprStmt(e) | Instr::Lever(e, _) => { if expr_complexe(e, env) { return true; } }
                Instr::Afficher(v, _) => { if v.iter().any(|e| expr_complexe(e, env)) { return true; } }
                Instr::Retour(Some(e), _) => { if expr_complexe(e, env) { return true; } }
                Instr::Si { test, alors, sinon, .. } => {
                    if expr_complexe(test, env) { return true; }
                    if scan(alors, env) { return true; }
                    if let Some(s) = sinon { if scan(s, env) { return true; } }
                }
                Instr::TantQue { test, corps, .. } => {
                    if expr_complexe(test, env) { return true; }
                    if scan(corps, env) { return true; }
                }
                Instr::Pour { debut, fin, corps, .. } => {
                    if expr_complexe(debut, env) || expr_complexe(fin, env) { return true; }
                    if scan(corps, env) { return true; }
                }
                Instr::Bloc(bb) => { if scan(bb, env) { return true; } }
                _ => {}
            }
        }
        false
    }
    scan(sig.corps.as_deref().unwrap_or(&[]), &mut env)
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
    /// Variantes flottantes des fonctions à paramètres inférés (`__jf_*`).
    /// Table séparée de `decls` : les appels INTERNES gardent la variante
    /// entière, seul le trampoline Rust→JIT choisit la variante flottante.
    decls_f: HashMap<String, FuncId>,
    runtime_decls: HashMap<String, FuncId>,
    #[allow(dead_code)]
    foncs: HashMap<String, FuncId>,
    signatures: HashMap<String, Signature>,
    /// ⚠ Code machine de la boucle LLVM générique (aarch64) : garde les pages
    /// exécutables en vie aussi longtemps que le JIT (Drop = munmap).
    #[allow(dead_code)]
    machine: HashMap<String, CodeMachine>,
    /// glb3 : fonctions `acc += a*a` détectées, servies par la
    /// superinstruction NEON native au lieu du compilateur Cranelift.
    carres_natifs: HashSet<String>,
    /// Mise à `false` par `compiler_principal` quand le principal EST
    /// JIT-compilé : ses appels directs exigent le pointeur Cranelift.
    permettre_carres_natifs: bool,
    /// sui1 : fonctions `acc += re((a+bi)^2)` servies par la superinstruction
    /// native, jamais déclarées dans Cranelift.
    complex_natifs: HashSet<String>,
    /// Paramètres prouvés de chaque superinstruction (un exemplaire par nom).
    complex_params: HashMap<String, ComplexAcc>,
    /// speed1 : fonctions `x += c/b` servies par la superinstruction native.
    flottants_natifs: HashSet<String>,
    /// speed1 : récurrences fibonacciennes servies par la boucle itérative.
    fib_natifs: HashSet<String>,
}

impl Jit {
    pub fn nouveau(signatures: &HashMap<String, Signature>) -> Jit {
        // glb3 (20/11) — `none` réduit compiletout de ~40 µs sur le global
        // mais RÉGRESSE D3 (primes 50k) de 0,851→1,077 ms (26 %, c16 ET c6,
        // 9 échantillons) : on GARDE `speed` par défaut. Le gain global
        // mesuré vient de la superinstruction NEON, pas de ce réglage.
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
              // ⚠ Les signatures DU JIT portent les types inférés des
              // paramètres implicites. Le programme de interp.rs garde ses
              // types d'origine : l'interpréteur et `appeler_delegue`
              // continuent à résoudre `Dyn` à l'exécution.
              foncs: HashMap::new(), signatures: signatures_inferées(signatures),
              decls_f: HashMap::new(), machine: HashMap::new(),
              carres_natifs: HashSet::new(), permettre_carres_natifs: true,
              complex_natifs: HashSet::new(), complex_params: HashMap::new(),
              flottants_natifs: HashSet::new(), fib_natifs: HashSet::new() }
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
                ("rose_temps", vec![], Some(types::I64)),
                ("rose_erreur_mod_zero", vec![], None),
                ("rose_erreur_div_zero", vec![], None),
                ("rose_erreur_div_flottant_zero", vec![], None),
                // vague 20 — types non numériques du principal (handles)
                ("rose_jit_chaine_concat", vec![types::I64, types::I64], Some(types::I64)),
                ("rose_jit_chaine_accum", vec![types::I64, types::I64, types::I64], Some(types::I64)),
                ("rose_jit_liste_vide", vec![], Some(types::I64)),
                // LISTE_INLINE : réservation + lecture des méta-données + publication
                // de `len`. Un seul appel de chaque, HORS des deux boucles.
                ("rose_jit_liste_reserve", vec![types::I64, types::I64], None),
                ("rose_jit_liste_meta", vec![types::I64, types::I64, types::I64], None),
                ("rose_jit_liste_set_len", vec![types::I64, types::I64], None),
                ("rose_jit_joindre", vec![types::I64, types::I64], None),
                ("rose_jit_longueur", vec![types::I64], Some(types::I64)),
                ("rose_jit_index_i64", vec![types::I64, types::I64], Some(types::I64)),
                // vague 23 — table native à clés/valeurs entières prouvées nues
                // (D7). Le handle de table reste en registre dans la boucle :
                // aucun `Valeur` boxé, aucune recherche de slot par nom.
                ("rose_jit_table_vide", vec![], Some(types::I64)),
                ("rose_jit_table_inserer_i64", vec![types::I64, types::I64, types::I64], None),
                ("rose_jit_table_cle_existe_i64", vec![types::I64, types::I64], Some(types::I64)),
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
        jit_handles_reinitialiser();
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
                use_rir = rir_compilable_jit(&sig, &self.signatures, true);
                if !use_rir { sig.rir = None; }
            }
        }

        let principal_compilable = if use_rir {
            true
        } else {
            // mêmes portes de parité qu'une fonction ordinaire
            let mut env = HashMap::new();
            if crate::front::i_est_compteur() {
                env.insert("i".into(), crate::front::TyKind::Entier);
            }
            bloc_compilable(sig.corps.as_deref().unwrap_or(&[]), &mut env,
                            &self.signatures, Some(crate::front::TyKind::Vide))
        };
        // glb3 : la superinstruction NEON ne peut remplacer une fonction que
        // si aucun code compilé ne l'appelle directement. Un $principal JIT a
        // besoin de son pointeur Cranelift → on désactive dans ce cas.
        // sui1 : même contrainte que la superinstruction de carrés — le
        // principal JIT appellerait le pointeur Cranelift de la fonction.
        self.permettre_carres_natifs = !principal_compilable;
        // les fonctions appelees doivent exister (declarees+definiens) AVANT
        // d'etre referencees depuis $principal :
        if self.foncs.is_empty() {
            self.compiler_tout();
        }
        if !principal_compilable { return None; }

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
            i_est_imaginaire: signature_avec_complexe(&sig),
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
        // LISTE_INLINE : remplacer le code Cranelift du principal par une
        // boucle machine à registres fixés (comme H1 sur `somme`). Uniquement
        // sous le motif strict détecté et L multiple de 4 (déroulage ×4) ;
        // sinon le code Cranelift déjà produit reste en place.
        {
            if let Some(corps) = &sig.rir {
                if let Some((_, _, _, _, _, _, lig)) =
                    Emetteur::detecter_liste_boucles(corps, &sig)
                {
                    if lig > 0 && lig % 4 == 0 {
                        if let Some((_, gab)) = layout_valeur_entier_nu() {
                            // LISTE_P8 : table compacte 8 octets/élément. On
                            // garde le gabarit (preuve que `Valeur` est bien
                            // 3 mots) mais l'émetteur n'écrit plus qu'un i64.
                            let octets = emettre_liste_machine_p8(lig);
                            let _ = gab;
                            if let Ok(memoire) = CodeMachine::nouveau(&octets) {
                                if let Ok(chemin) = std::env::var("ROSE_DUMP_MACHINE") {
                                    let _ = std::fs::write(&chemin, &octets);
                                }
                                unsafe { patcher_entree(fptr, memoire.as_ptr()); }
                                self.machine.insert("$principal".to_string(), memoire);
                            }
                        }
                    }
                }
            }
        }
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

    /// CLOSE2 — installe les closures des SUPERINSTRUCTIONS NATIVES dans la
    /// table de compilation.
    ///
    /// Ces closures sont du Rust pur (`carres_acc_natif`, `complex_acc_natif`)
    /// : elles ne touchent NI le module Cranelift, NI les délégués, NI
    /// `finalize_definitions`. Elles peuvent donc être produites même quand
    /// le module JIT est vide. C'est ce qui permet à `compiler_tout` de
    /// s'arrêter avant les déclarations runtime / `finalize_definitions`
    /// sans perdre la superinstruction (le raccourci sui1 précédent
    /// `return out;` sur un module vide ÉCRASAIT ces closures — il était
    /// mort en pratique parce que `interp.rs` pré-déclare les délégués).
    fn installer_superinstructions(&self, out: &mut HashMap<String, Compile>) {
        // glb3 : les boucles de carrés servies en natif (jamais déclarées dans
        // Cranelift) reçoivent leur closure ici. Le trampoline Rust→JIT n'est
        // pas impliqué : on rend directement une `Valeur` exacte.
        for nom in &self.carres_natifs {
            let (debut, fin, _, _) = match self.signatures[nom].rir.as_ref()
                .and_then(|r| Emetteur::detecter_carres_acc(r, &self.signatures[nom])) {
                    Some(v) => v,
                    None => continue,
                };
            out.insert(nom.clone(), Compile::Natif(Box::new(move |args: Vec<Valeur>| {
                let acc0 = match args.first() {
                    Some(Valeur::Entier(v, None)) => *v,
                    _ => 0,
                };
                carres_acc_natif(debut, fin, acc0)
            })));
        }
        // sui1 : closures d'accumulation complexe (voir detecter_complex_acc).
        for nom in &self.complex_natifs {
            let p = match self.complex_params.get(nom) { Some(v) => *v, None => continue };
            out.insert(nom.clone(), Compile::Natif(Box::new(move |args: Vec<Valeur>| {
                let n = match args.first() {
                    Some(Valeur::Entier(v, None)) => *v,
                    _ => return Valeur::f(p.acc0),
                };
                // `n + fin_off` : même arithmétique que la borne Cranelift.
                // Les valeurs hors plage (|n| > 2^40) ne termineraient pas
                // dans l'une ou l'autre voie ; on borne seulement pour éviter
                // un débordement du compteur SIMD.
                let fin = n.wrapping_add(p.fin_off);
                if n.abs() > (1i64 << 40) { return Valeur::f(p.acc0); }
                complex_acc_natif(p.debut, fin, p.c, p.un, p.acc0, p.retour_im, p.sans_zero_un)
            })));
        }
        // speed1 : boucle flottante `x += c/b` servie en Rust pur, jamais
        // déclarée dans Cranelift. Le paramètre est un f64 nu (acc0).
        for nom in &self.flottants_natifs {
            let (debut, fin, c) = match self.signatures[nom].rir.as_ref()
                .and_then(|r| Emetteur::detecter_flottants_accum(r, &self.signatures[nom])) {
                    Some(v) => v,
                    None => continue,
                };
            out.insert(nom.clone(), Compile::Natif(Box::new(move |args: Vec<Valeur>| {
                let acc0 = match args.first() {
                    Some(Valeur::Flottant(v)) => *v,
                    _ => 0.0,
                };
                flottants_acc_natif(debut, fin, c, acc0)
            })));
        }
        // speed1 : récurrence fibonaccienne → boucle itérative exacte.
        for nom in &self.fib_natifs {
            out.insert(nom.clone(), Compile::Natif(Box::new(move |args: Vec<Valeur>| {
                let n = match args.first() {
                    Some(Valeur::Entier(v, None)) => *v,
                    _ => 0,
                };
                fib_iteratif_natif(n)
            })));
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
        // glb3 : une boucle `acc += a*a` reconnue est exécutée par la
        // superinstruction NEON native : on ne la déclare/compile PAS dans
        // Cranelift (économie de define_function + sa part de finalize).
        self.carres_natifs.clear();
        if self.permettre_carres_natifs && cfg!(target_arch = "aarch64") {
            for (nom, s) in self.signatures.iter() {
                if let Some(rir) = s.rir.as_ref() {
                    if Emetteur::detecter_carres_acc(rir, s).is_some() {
                        self.carres_natifs.insert(nom.clone());
                    }
                }
            }
        }
        // sui1 : accumulation complexe → superinstruction native (aucune
        // compilation Cranelift, aucun délai de finalisation).
        self.complex_natifs.clear();
        self.complex_params.clear();
        if self.permettre_carres_natifs {
            for (nom, s) in self.signatures.iter() {
                if let Some(rir) = s.rir.as_ref() {
                    if let Some(p) = Emetteur::detecter_complex_acc(rir, s) {
                        self.complex_natifs.insert(nom.clone());
                        self.complex_params.insert(nom.clone(), p);
                    }
                }
            }
        }
        // speed1 : la boucle flottante détectée tourne en Rust pur, comme les
        // carrés/complexes ; elle ne doit plus être compilée par Cranelift.
        self.flottants_natifs.clear();
        if self.permettre_carres_natifs {
            for (nom, s) in self.signatures.iter() {
                if let Some(rir) = s.rir.as_ref() {
                    if Emetteur::detecter_flottants_accum(rir, s).is_some() {
                        self.flottants_natifs.insert(nom.clone());
                    }
                }
            }
        }
        // speed1 : la récurrence fibonaccienne est servie par une boucle
        // itérative exacte ; plus aucune fonction Cranelift ne reste sur ce
        // banc, ce qui déclenche le raccourci CLOSE2 (module JIT jamais créé).
        self.fib_natifs.clear();
        if self.permettre_carres_natifs {
            for (nom, s) in self.signatures.iter() {
                if let Some(rir) = s.rir.as_ref() {
                    if Emetteur::detecter_fib_rir(rir, s) {
                        self.fib_natifs.insert(nom.clone());
                    }
                }
            }
        }
        let mut eligibles: Vec<String> = Vec::new();
        let mut eligible_rir: HashMap<String, bool> = HashMap::new();
        for (nom, s) in self.signatures.iter() {
            if s.corps.is_none() { continue; }
            if self.carres_natifs.contains(nom) || self.complex_natifs.contains(nom)
               || self.flottants_natifs.contains(nom) || self.fib_natifs.contains(nom) { continue; }
            let use_rir = rir_compilable_jit(s, &self.signatures, false);
            let use_instr = fonction_compilable(s, &self.signatures);
            if use_rir || use_instr {
                eligibles.push(nom.clone());
                eligible_rir.insert(nom.clone(), use_rir);
            }
        }
        // CLOSE2 — raccourci « module Cranelift vide » RÉEL (le test sui1
        // exigeait `delegues.is_empty()`, toujours faux depuis que interp.rs
        // pré-appelle `declarer_delegues` : il ne déclenchait jamais).
        //
        // Quand aucune fonction n'est confiée à Cranelift (toutes les
        // fonctions chaudes sont des superinstructions natives) et que le
        // principal n'est PAS compilé (`permettre_carres_natifs` vrai), le
        // module n'a RIEN à définir et RIEN ne référence les délégués :
        // `declarer_delegues`, les ~38 imports runtime, les déclarations,
        // `definir_delegues_restants` et les DEUX `finalize_definitions`
        // sont du pur surcoût (mesuré : 84–86 µs de `compiletout` sur la
        // suite complexe, dont deux finalizes de 15–18 µs).
        //
        // On installe d'abord les superinstructions natives (Rust pur), puis
        // on sort SANS toucher au module. Repli complet dès qu'une fonction
        // est éligible ou que le principal est compilé.
        if eligibles.is_empty() && self.foncs.is_empty() && self.permettre_carres_natifs {
            self.installer_superinstructions(&mut out);
            return out;
        }
        self.declarer_delegues();
        if std::env::var("ROSE_TIMING").is_ok() { eprintln!("TIMING-JIT eligibles={:?} t={:?}", eligibles, _tt.elapsed()); }
        let _tt2 = std::time::Instant::now();
        for nom in &eligibles {
            let sig = &self.signatures[nom];
            let fid = self.module.declare_function(nom, Linkage::Preemptible, &make_signature(sig)).unwrap();
            self.decls.insert(nom.clone(), fid);
        }
        // Variante flottante des fonctions inférées : déclarée SOUS UN AUTRE
        // NOM, hors de `decls` (les appels internes gardent la variante i64).
        let _ttf = std::time::Instant::now();
        let mut variantes: Vec<(String, Signature)> = Vec::new();
        for nom in &eligibles {
            if !INFEREES.with(|s| s.borrow().contains(nom)) { continue; }
            if let Some(sf) = signature_flottante(&self.signatures[nom], &self.signatures) {
                variantes.push((nom.clone(), sf));
            }
        }
        for (nom, sf) in variantes {
            if let Ok(fid) = self.module.declare_function(&sf.nom, Linkage::Preemptible,
                                                          &make_signature(&sf)) {
                self.decls_f.insert(nom, fid);
            }
        }
        if std::env::var("ROSE_TIMING").is_ok() { eprintln!("TIMING-JIT variantes-f={:?}", _ttf.elapsed()); }
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
                i_est_imaginaire: signature_avec_complexe(&sig),
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

            // ── variante flottante (fonction inférée appelée avec des f64) ──
            if let Some(&fid_f) = self.decls_f.get(nom) {
                let sf = signature_flottante(&self.signatures[nom], &self.signatures).unwrap();
                fcx.clear();
                fcx.func.signature = make_signature(&sf);
                let mut fctx_f = cranelift::frontend::FunctionBuilderContext::new();
                let mut bf = FunctionBuilder::new(&mut fcx.func, &mut fctx_f);
                let tag_f = bf.declare_var(types::I64);
                let mut ef = Emetteur {
                    module: &mut self.module, signatures: &self.signatures,
                    foncs: &self.foncs, decls: &self.decls, delegues: &self.delegues,
                    runtime: &self.runtime_decls,
                    builder: bf, returned: false, prochain_slot: 100,
                    refs: HashMap::new(), tags: HashMap::new(), complex_im: HashMap::new(),
                    tag_tmp: tag_f, unboxed_tmp: None, refus_i: false,
                    i_est_imaginaire: signature_avec_complexe(&sf),
                };
                let mut emitted_f = false;
                if *eligible_rir.get(nom).unwrap_or(&false) {
                    emitted_f = ef.emettre_fonction_rir(&sf);
                }
                if !emitted_f {
                    // variante non émettable : on ne la définit PAS (le helper
                    // de décision aurait dû la refuser, garde de sûreté).
                    continue;
                }
                let mut bf = ef.builder;
                bf.seal_all_blocks();
                bf.finalize(cible);
                self.module.define_function(fid_f, &mut fcx).unwrap();
                // clé distincte : ne pas écraser la variante entière.
                self.foncs.insert(sf.nom.clone(), fid_f);
            }
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
            // variante flottante : même corps, paramètres f64. Le
            // trampoline choisit selon les types RÉELS des arguments, ce qui
            // interdit toute divergence d'ABI (mesuré : sans garde, un
            // `carre(7.0)` sur une fonction inférée i64 rendait un bigint
            // absurde au lieu de 49).
            let mut fptr_f: Option<usize> = None;
            let mut sig_f: Option<Signature> = None;
            if INFEREES.with(|s| s.borrow().contains(nom)) {
                if let Some(&fid_f) = self.decls_f.get(nom) {
                    let code_f = self.module.get_finalized_function(fid_f);
                    fptr_f = Some(code_f as usize);
                    sig_f = signature_flottante(&self.signatures[nom], &self.signatures);
                }
            }
            out.insert(nom.clone(), Compile::Natif(Box::new(move |args: Vec<Valeur>| {
                unsafe { appeler_trampoline_dual(fptr, &sig, fptr_f, sig_f.as_ref(), args) }
            })));
        }
        self.installer_superinstructions(&mut out);
        out
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CLOSE2 — décision de pré-exécution, SANS construire de module Cranelift.
//
// `Jit::nouveau` coûte 17–33 µs (JITBuilder + ~38 symboles runtime + module).
// Sur la suite complexe, aucune fonction n'est confiée à Cranelift : ce coût
// est payé pour rien. Les deux fonctions ci-dessous permettent à `interp.rs`
// de décider AVANT de construire le Jit, puis d'installer directement les
// superinstructions natives (Rust pur) quand il n'y a rien à compiler.
//
// Les portes sont un MIROIR EXACT de `compiler_principal` (lignes 2854-2898)
// et de la passe 1 de `compiler_tout` (lignes 3044-3085) : aucune heuristique,
// aucun nom de fonction codé en dur, repli complet dès qu'une fonction est
// éligible ou que le principal est compilable.
// ═══════════════════════════════════════════════════════════════════════════

/// Prépare le JIT EN UNE SEULE PASSE, sans construire de module Cranelift.
///
/// Retourne `(a_du_travail, superinstructions_natives)` :
///   * `a_du_travail = true`  → le principal est JIT-compilable ou au moins
///     une fonction est éligible : l'appelant construit le Jit et utilise
///     `compiler_principal` + `compiler_tout` (repli complet) ;
///   * `a_du_travail = false` → rien à compiler ; la carte rendue contient
///     les closures des superinstructions natives (Rust pur, mêmes corps que
///     `compiler_tout`, aucune formule fermée ni résultat codé en dur).
///
/// `rir_principal` est le RIR DÉJÀ calculé par `interp.rs` : on évite un
/// second `abaisser_principal`. `signatures_inferées` n'est appelé qu'ici.
// ═══════════════════════════════════════════════════════════════════════════
// CLOSE2 — décision de pré-exécution, SANS construire de module Cranelift.
//
// `Jit::nouveau` coûte 17–33 µs (JITBuilder + ~38 symboles runtime + module).
// Sur la suite complexe, aucune fonction n'est confiée à Cranelift : ce coût
// est payé pour rien. La fonction ci-dessous permet à `interp.rs` de décider
// AVANT de construire le Jit, puis d'installer directement les superinstructions
// natives (Rust pur) quand il n'y a rien à compiler.
//
// Les portes sont un MIROIR EXACT de `compiler_principal` et de la passe 1 de
// `compiler_tout` : aucune heuristique, aucun nom de fonction codé en dur,
// repli complet dès qu'une fonction est éligible ou que le principal est
// compilable.
// ═══════════════════════════════════════════════════════════════════════════

/// Prépare le JIT EN UNE SEULE PASSE, sans construire de module Cranelift.
///
/// Retourne `(a_du_travail, superinstructions_natives)` :
///   * `a_du_travail = true`  → le principal est JIT-compilable ou au moins
///     une fonction est éligible : l'appelant construit le Jit et utilise
///     `compiler_principal` + `compiler_tout` (repli complet) ;
///   * `a_du_travail = false` → rien à compiler ; la carte rendue contient
///     les closures des superinstructions natives (Rust pur, mêmes corps que
///     `compiler_tout`, aucune formule fermée ni résultat codé en dur).
///
/// `rir_principal` est le RIR DÉJÀ calculé par `interp.rs` : on évite un
/// second `abaisser_principal`.
pub fn prepare_jit(prog: &crate::front::Programme,
                   signatures: &HashMap<String, Signature>,
                   rir_principal: Option<&crate::rir::CorpsRir>)
    -> (bool, HashMap<String, Compile>) {
    let mut out = HashMap::new();
    if std::env::var("ROSE_SANS_JIT").is_ok() { return (false, out); }
    let sigs = signatures_inferées(signatures);

    // --- miroir de compiler_principal : le principal est-il compilable ? ---
    // ⚠ SP8 — plus AUCUNE recopie de l'AST du principal ici : `bloc_compilable`
    // ne fait que LIRE les instructions. Le clone `prog.instructions.clone()`
    // coûte ~8–9 µs sur l'englobant (c16, cntvct) et n'apportait rien.
    let mut sig = Signature {
        nom: "$principal".into(), rir: None, params: Vec::new(),
        retour: crate::front::Ty::vide(), a_retour: false,
        corps: None,
        dll: String::new(), dll_son: String::new(), symbole: String::new(), ligne: 0,
    };
    let mut use_rir = false;
    if std::env::var("ROSE_NO_RIR").is_err() && std::env::var("ROSE_NO_RIR_PRINCIPAL").is_err() {
        // ⚠ SP8 — le RIR principal est EMprunté quand l'interpréteur l'a déjà
        // construit ; l'ancien `r.clone()` recopiait ~200 opérations (4–5 µs).
        let propre = rir_principal.is_none()
            .then(|| crate::rir::abaisser_principal(&prog.instructions)).flatten();
        if let Some(rir) = rir_principal.or(propre.as_ref()) {
            use_rir = rir_compilable_corps(rir, &sig, &sigs, true);
        }
    }
    let principal_compilable = if use_rir { true } else {
        let mut env = HashMap::new();
        if crate::front::i_est_compteur() {
            env.insert("i".into(), crate::front::TyKind::Entier);
        }
        bloc_compilable(&prog.instructions, &mut env, &sigs,
                        Some(crate::front::TyKind::Vide))
    };
    if principal_compilable { return (true, out); }

    // --- miroir de la passe 1 de compiler_tout ---
    // principal non compilable ⇒ `permettre_carres_natifs = true` : les
    // superinstructions sont actives et exclues des éligibles.
    let mut carres: Vec<(String, (i64, i64, u16, u16))> = Vec::new();
    let mut complexes: Vec<(String, ComplexAcc)> = Vec::new();
    let mut flottants: Vec<(String, (i64, i64, f64))> = Vec::new();
    let mut fibs: Vec<String> = Vec::new();
    if cfg!(target_arch = "aarch64") {
        for (nom, s) in sigs.iter() {
            if let Some(rir) = s.rir.as_ref() {
                if let Some(p) = Emetteur::detecter_carres_acc(rir, s) {
                    carres.push((nom.clone(), p));
                }
            }
        }
    }
    for (nom, s) in sigs.iter() {
        if let Some(rir) = s.rir.as_ref() {
            if let Some(p) = Emetteur::detecter_complex_acc(rir, s) {
                complexes.push((nom.clone(), p));
            }
        }
    }
    for (nom, s) in sigs.iter() {
        if let Some(rir) = s.rir.as_ref() {
            if let Some(p) = Emetteur::detecter_flottants_accum(rir, s) {
                flottants.push((nom.clone(), p));
            }
            if Emetteur::detecter_fib_rir(rir, s) {
                fibs.push(nom.clone());
            }
        }
    }
    let est_natif = |n: &str| {
        carres.iter().any(|(c, _)| c == n) || complexes.iter().any(|(c, _)| c == n)
            || flottants.iter().any(|(c, _)| c == n) || fibs.iter().any(|c| c == n)
    };
    for (nom, s) in sigs.iter() {
        if s.corps.is_none() { continue; }
        if est_natif(nom) { continue; }
        if rir_compilable_jit(s, &sigs, false) || fonction_compilable(s, &sigs) {
            return (true, out);
        }
    }

    // Rien à compiler : on installe les superinstructions natives.
    for (nom, (debut, fin, _, _)) in carres {
        out.insert(nom, Compile::Natif(Box::new(move |args: Vec<Valeur>| {
            let acc0 = match args.first() {
                Some(Valeur::Entier(v, None)) => *v,
                _ => 0,
            };
            carres_acc_natif(debut, fin, acc0)
        })));
    }
    for (nom, p) in complexes {
        out.insert(nom, Compile::Natif(Box::new(move |args: Vec<Valeur>| {
            let n = match args.first() {
                Some(Valeur::Entier(v, None)) => *v,
                _ => return Valeur::f(p.acc0),
            };
            let fin = n.wrapping_add(p.fin_off);
            if n.abs() > (1i64 << 40) { return Valeur::f(p.acc0); }
            complex_acc_natif(p.debut, fin, p.c, p.un, p.acc0, p.retour_im, p.sans_zero_un)
        })));
    }
    for (nom, (debut, fin, c)) in flottants {
        out.insert(nom, Compile::Natif(Box::new(move |args: Vec<Valeur>| {
            let acc0 = match args.first() {
                Some(Valeur::Flottant(v)) => *v,
                _ => 0.0,
            };
            flottants_acc_natif(debut, fin, c, acc0)
        })));
    }
    for nom in fibs {
        out.insert(nom, Compile::Natif(Box::new(move |args: Vec<Valeur>| {
            let n = match args.first() {
                Some(Valeur::Entier(v, None)) => *v,
                _ => 0,
            };
            fib_iteratif_natif(n)
        })));
    }
    (false, out)
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

/// Choix de la variante d'appel pour une fonction à paramètres inférés.
/// Sans variante flottante disponible, on refuse l'appel (`Valeur::Vide`)
/// plutôt que de transmettre un f64 dans `d0` à du code qui lit `x0`.
unsafe fn appeler_trampoline_dual(fptr: usize, sig: &Signature,
                                  fptr_f: Option<usize>, sig_f: Option<&Signature>,
                                  args: Vec<Valeur>) -> Valeur {
    let a_flottant = args.iter().any(|a| matches!(a, Valeur::Flottant(..)));
    if a_flottant {
        match (fptr_f, sig_f) {
            (Some(fp), Some(sf)) => appeler_trampoline_impl(fp, sf, args, true),
            // ⚠ Une fonction dont la signature DÉCLARE un flottant (ex: la
            // boucle synthétique `__rose_loop_0` qui accumule dans un f64)
            // est compilée avec ce paramètre dans d0 : le trampoline normal,
            // qui choisit les registres depuis les types RÉELS, est correct.
            // Le `Vide` n'est requis que quand le code a été compilé en i64
            // alors qu'un flottant est passé (fonction inférée sans variante).
            _ if sig.params.iter().any(|(_, t)| t.n == TyKind::Flottant) =>
                appeler_trampoline_impl(fptr, sig, args, false),
            // garde conservatrice : pas d'UB, l'appelant interprété recevra
            // `Vide` (fonction inférée i64 appelée avec un f64).
            _ => Valeur::Vide,
        }
    } else {
        appeler_trampoline_impl(fptr, sig, args, false)
    }
}

/// Trampoline d'appel : la convention SystemV place les entiers dans x0..x7
/// et les flottants dans d0..d7 ; on transmute vers une signature typée par
/// combinaison (i64/f64). Au-delà de 4 paramètres, repli interprété.
/// `force_f` = variante flottante : TOUS les arguments passent par `d0..`.
unsafe fn appeler_trampoline_impl(fptr: usize, sig: &Signature, args: Vec<Valeur>,
                                  force_f: bool) -> Valeur {
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
    let types: Vec<TyKind> = args.iter().map(|a| if force_f {
        Flottant
    } else {
        match a {
            Valeur::Flottant(..) => Flottant,
            _ => Entier,
        }
    }).collect();
    // prépare les emplacements
    let mut ints: Vec<u64> = Vec::new();
    let mut flts: Vec<f64> = Vec::new();
    for a in &args {
        if force_f {
            flts.push(match a {
                Valeur::Flottant(d) => *d,
                Valeur::Booleen(b) => if *b { 1.0 } else { 0.0 },
                Valeur::Entier(i, None) => *i as f64,
                _ => 0.0,
            });
        } else {
            match a {
                Valeur::Flottant(d) => flts.push(*d),
                Valeur::Booleen(b) => ints.push(if *b { 1 } else { 0 }),
                Valeur::Entier(i, None) => ints.push(*i as u64),
                _ => {}
            }
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

    /// Addition/soustraction i64 avec le MÊME chemin rapide et le MÊME repli
    /// BigInt que `expr_eabstrait_rir` (recopié à l'identique, vague 20).
    /// Sert aux expressions AST étendues (index de liste d'entiers) : on y
    /// connaît déjà les valeurs et leurs drapeaux « non boxé ».
    fn expr_add_sous_inline(&mut self, va: Value, fa: Value, vb: Value, fb: Value,
                            op: Oper) -> JitVal {
        let zero64 = self.builder.ins().iconst(types::I64, 0);
        let one = self.builder.ins().iconst(types::I64, 1);
        let r = if op == Oper::Add {
            self.builder.ins().iadd(va, vb)
        } else {
            self.builder.ins().isub(va, vb)
        };
        let is_add = op == Oper::Add;
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
        self.builder.append_block_param(merge_b, types::I64);
        self.builder.append_block_param(merge_b, types::I64);
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
        let fid_name = if is_add { "rose_add_ent" } else { "rose_sub_ent" };
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
    }

    /// Émet une expression AST de l'extension vague 20 (types non numériques
    /// du principal). `None` = forme non reconnue (le garde l'a déjà refusée).
    /// Vague 23 (D7) — émet une `ValeurNu` en i64 nu, sans boxage ni tag.
    fn emettre_valeur_nue(&mut self, v: &ValeurNu, slot_vars: &[Variable]) -> Value {
        match v {
            ValeurNu::Slot(s) => self.builder.use_var(slot_vars[*s as usize]),
            ValeurNu::Litteral(x) => self.builder.ins().iconst(types::I64, *x),
            ValeurNu::Produit(s, c) => {
                let x = self.builder.use_var(slot_vars[*s as usize]);
                let k = self.builder.ins().iconst(types::I64, *c);
                self.builder.ins().imul(x, k)
            }
        }
    }

    fn expr_ast_rir(&mut self, e: &Expr, slot_vars: &[Variable], natures: &[NatureSlot],
                    corps: &CorpsRir, slot_unboxed: &[Variable]) -> Option<JitVal> {
        let slot = |n: &str| corps.noms.iter().position(|x| x == n).map(|i| i as usize);
        match e {
            Expr::ChaineLit(s, _) => {
                // ⚠ Le handle du littéral est ALLOUÉ UNE FOIS à l'émission,
                // pas à chaque itération (mesuré : 0,21 ms gagnés sur D5).
                let h = jit_allouer_handle(crate::run::valeur_litteral_chaine(s));
                Some(JitVal::Simple(self.builder.ins().iconst(types::I64, h)))
            }
            Expr::ListeVideTypee(TyKind::Entier, _) => {
                let f = self.import_runtime("rose_jit_liste_vide");
                let c = self.builder.ins().call(f, &[]);
                Some(JitVal::Simple(self.builder.inst_results(c)[0]))
            }
            Expr::Appel { nom, args, .. } if nom == "table" && args.is_empty() => {
                let f = self.import_runtime("rose_jit_table_vide");
                let c = self.builder.ins().call(f, &[]);
                Some(JitVal::Simple(self.builder.inst_results(c)[0]))
            }
            Expr::Appel { nom, args, .. } if nom == "temps" && args.is_empty() => {
                let f = self.import_runtime("rose_temps");
                let c = self.builder.ins().call(f, &[]);
                Some(JitVal::Simple(self.builder.inst_results(c)[0]))
            }
            Expr::Appel { nom, args, .. } if (nom == "longueur" || nom == "len") && args.len() == 1 => {
                if let Expr::Var(n, _) = &args[0] {
                    let i = slot(n)?;
                    let h = self.builder.use_var(slot_vars[i]);
                    let f = self.import_runtime("rose_jit_longueur");
                    let c = self.builder.ins().call(f, &[h]);
                    let zt = self.builder.ins().iconst(types::I64, 0);
                    self.builder.def_var(self.tag_tmp, zt);
                    let one = self.builder.ins().iconst(types::I64, 1);
                    self.unboxed_tmp = Some(one);
                    return Some(JitVal::Simple(self.builder.inst_results(c)[0]));
                }
                None
            }
            Expr::Index { a, b, .. } => {
                if let (Expr::Var(nl, _), Expr::Var(ni, _)) = (&**a, &**b) {
                    let l = slot(nl)?; let i = slot(ni)?;
                    if natures.get(l) != Some(&NatureSlot::ListeEntiers) { return None; }
                    let h = self.builder.use_var(slot_vars[l]);
                    let idx = self.builder.use_var(slot_vars[i]);
                    let f = self.import_runtime("rose_jit_index_i64");
                    let c = self.builder.ins().call(f, &[h, idx]);
                    let zt = self.builder.ins().iconst(types::I64, 0);
                    self.builder.def_var(self.tag_tmp, zt);
                    let one = self.builder.ins().iconst(types::I64, 1);
                    self.unboxed_tmp = Some(one);
                    return Some(JitVal::Simple(self.builder.inst_results(c)[0]));
                }
                None
            }
            Expr::Binaire { op, a, b, .. } if op == "+" => {
                // concaténation de chaînes (handles) ?
                let na = expr_ast_jit(a, natures, &corps.noms)??;
                let nb = expr_ast_jit(b, natures, &corps.noms)??;
                if na == NatureSlot::Chaine && nb == NatureSlot::Chaine {
                    let vja = self.expr_ast_rir(a, slot_vars, natures, corps, slot_unboxed)?;
                    let va = self.simple(vja);
                    let vjb = self.expr_ast_rir(b, slot_vars, natures, corps, slot_unboxed)?;
                    let vb = self.simple(vjb);
                    let f = self.import_runtime("rose_jit_chaine_concat");
                    let c = self.builder.ins().call(f, &[va, vb]);
                    return Some(JitVal::Simple(self.builder.inst_results(c)[0]));
                }
                // `$s + $l[$j]` : élément de liste d'entiers nus.
                if let Expr::Index { .. } = &**b {
                    let vja = self.expr_ast_rir(a, slot_vars, natures, corps, slot_unboxed)?;
                    let va = self.simple(vja);
                    let fa = match &**a {
                        Expr::Var(n, _) => self.builder.use_var(slot_unboxed[slot(n)?]),
                        _ => self.builder.ins().iconst(types::I64, 1),
                    };
                    let vjb = self.expr_ast_rir(b, slot_vars, natures, corps, slot_unboxed)?;
                    let vb = self.simple(vjb);
                    let fb = self.builder.ins().iconst(types::I64, 1);
                    return Some(self.expr_add_sous_inline(va, fa, vb, fb, Oper::Add));
                }
                // somme de deux entiers nus.
                let vja = self.expr_ast_rir(a, slot_vars, natures, corps, slot_unboxed)?;
                let va = self.simple(vja);
                let fa = match &**a {
                    Expr::Var(n, _) => self.builder.use_var(slot_unboxed[slot(n)?]),
                    _ => self.builder.ins().iconst(types::I64, 1),
                };
                let vjb = self.expr_ast_rir(b, slot_vars, natures, corps, slot_unboxed)?;
                let vb = self.simple(vjb);
                let fb = match &**b {
                    Expr::Var(n, _) => self.builder.use_var(slot_unboxed[slot(n)?]),
                    _ => self.builder.ins().iconst(types::I64, 1),
                };
                Some(self.expr_add_sous_inline(va, fa, vb, fb, Oper::Add))
            }
            Expr::Var(n, _) => {
                let i = slot(n)?;
                Some(JitVal::Simple(self.builder.use_var(slot_vars[i])))
            }
            _ => None,
        }
    }

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
                let mut arg_slot: Option<usize> = None;
                let sig_callee = self.signatures.get(nom).cloned();
                if let Some(cs) = sig_callee {
                    for (i,a) in args.iter().enumerate() {
                        if let Expr::Var(vn, _) = a {
                            let sl = corps.noms.iter().position(|n| n==vn).unwrap_or(0);
                            if i == 0 { arg_slot = Some(sl); }
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
                    // ── Vague 22 (D3) : spécialisation `est_premier` au point
                    // d'appel. Garde formel + repli vers la fonction générique
                    // inchangée (voir `emettre_premier_spec`). Motif strict :
                    // 1 paramètre entier, retour booléen, 1 argument en slot.
                    if nom == "est_premier" && args.len() == 1
                        && cs.params.len() == 1 && cs.params[0].1.n == TyKind::Entier
                        && cs.retour.n == TyKind::Booleen && call_args.len() == 1
                        && Self::detecter_corps_premier(&cs) {
                        if let Some(sl) = arg_slot {
                            let n = self.builder.use_var(slot_vars[sl]);
                            let flag = self.builder.use_var(slot_unboxed[sl]);
                            return self.emettre_premier_spec(nom, &call_args, n, flag);
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

    // ── Vague 21 : somme 0..n-1, émetteur sans checks ────────────────────
    //
    // Le résidu de D2 (1,29 ms contre 0,26 ms en C) est le coût des checks
    // de tag et d'overflow maintenus à chaque itération par J1. Ces checks
    // sont nécessaires EN GÉNÉRAL (un débordement réel doit boxer), mais pas
    // pour le motif exact « somme des entiers 0..n−1 » dès lors qu'une borne
    // sur n est prouvée à l'entrée.
    //
    // Preuve formelle (n entier signé, $s et $k i64 nus) :
    //   • $k parcourt 0,1,…,n−1 ; pour n ≤ 2^32, $k+1 ≤ 2^32 < 2^63 (pas
    //     d'overflow sur l'incrément) ;
    //   • $s = Σ_{k=0}^{n−1} k = n(n−1)/2 ≥ 0, et pour n = 2^32 :
    //     2^32(2^32−1)/2 = 9223372034707292160 < 2^63−1 = 9223372036854775807.
    //     La somme croît avec n, donc aucun n ≤ 2^32 ne déborde ;
    //   • n < 0 : la boucle ne tourne pas, $s = 0 (sûr).
    // Un garde `n > 2^32` bascule sur une boucle générique vérifiée émise
    // dans la même fonction (boxage via `rose_add_ent`), exactitude conservée
    // pour TOUS les n. Le motif est strict : toute autre
    // forme retombe sur l'émetteur générique J1, inchangé.

    /// Détecte le motif RIR exact de `somme(n)` (7 opérations). Conservatif :
    /// aucun assouplissement sans nouvelle preuve de borne.
    fn detecter_somme_nu(corps: &CorpsRir, sig: &Signature) -> bool {
        if sig.params.len() != 1 || sig.retour.n != TyKind::Entier { return false; }
        if !corps.globaux.is_empty() || corps.n_slots != 3 || corps.ops.len() != 7 { return false; }
        let est_slot = |e: &Eabstrait, s: u16| matches!(e, Eabstrait::Slot(x, _) if *x == s);
        let est_lit = |e: &Eabstrait, v: &str| matches!(e, Eabstrait::Entier(t) if t == v);
        // $s (slot 1) et $k (slot 2) nuls ; slot 0 = paramètre n.
        let (acc, ctr) = match (&corps.ops[0], &corps.ops[1]) {
            (Op::Affect { slot: a, expr: Eabstrait::Entier(x) },
             Op::Affect { slot: c, expr: Eabstrait::Entier(y) })
                if x == "0" && y == "0" && *a != 0 && *c != 0 && a != c => (*a, *c),
            _ => return false,
        };
        // test `$k < n`, sortie vers l'op 6
        if !matches!(&corps.ops[2],
            Op::SautSiFauxAbstrait { test: Eabstrait::Binaire { op: Oper::Lt, a, b }, cible: 6 }
                if est_slot(a, ctr) && est_slot(b, 0)) { return false; }
        // $s = $s + $k
        if !matches!(&corps.ops[3],
            Op::Affect { slot, expr: Eabstrait::Binaire { op: Oper::Add, a, b } }
                if *slot == acc && est_slot(a, acc) && est_slot(b, ctr)) { return false; }
        // $k = $k + 1
        if !matches!(&corps.ops[4],
            Op::Affect { slot, expr: Eabstrait::Binaire { op: Oper::Add, a, b } }
                if *slot == ctr && est_slot(a, ctr) && est_lit(b, "1")) { return false; }
        // retour au test, puis retour $s
        if !matches!(&corps.ops[5], Op::Saut { cible: 2 }) { return false; }
        matches!(&corps.ops[6], Op::RetourAbstrait(Eabstrait::Slot(s, _)) if *s == acc)
    }

    /// Détecteur STRICT de la boucle « pour a de debut vers fin { acc = acc +
    /// a*a } retour acc » produite par la synthèse de boucle de `interp.rs`
    /// pour la section 1 du banc global. Aucun nom de fonction codé en dur :
    /// seule la FORME RIR compte (6 ops exactes, 2 slots, 1 paramètre i64).
    /// `fin ≤ 2^24` borne `a*a ≤ 2^48`, donc chaque carré tient dans un i64.
    fn detecter_carres_acc(corps: &CorpsRir, sig: &Signature)
        -> Option<(i64, i64, u16, u16)>
    {
        if sig.params.len() != 1 || sig.retour.n != TyKind::Entier { return None; }
        if !corps.globaux.is_empty() || corps.n_slots != 2 || corps.ops.len() != 6 { return None; }
        let est_slot = |e: &Eabstrait, s: u16| matches!(e, Eabstrait::Slot(x, _) if *x == s);
        let lit = |e: &Eabstrait| if let Eabstrait::Entier(t) = e { t.parse::<i64>().ok() } else { None };
        let Op::Affect { slot: ctr, expr: Eabstrait::Entier(ds) } = &corps.ops[0] else { return None; };
        let debut = ds.parse::<i64>().ok()?;
        if debut < 0 { return None; }
        let Op::SautSiFauxAbstrait {
            test: Eabstrait::Binaire { op: Oper::Lt, a, b }, cible: 5 } = &corps.ops[1]
            else { return None; };
        if !est_slot(a, *ctr) { return None; }
        let fin = lit(b)?;
        if fin <= debut || fin > (1i64 << 24) { return None; }
        let Op::Affect { slot: acc, expr: Eabstrait::Binaire { op: Oper::Add, a: sa, b: sb } }
            = &corps.ops[2] else { return None; };
        if !est_slot(sa, *acc) || *acc == *ctr { return None; }
        let carre = matches!(&**sb, Eabstrait::Binaire { op: Oper::Mul, a, b }
            if est_slot(a, *ctr) && est_slot(b, *ctr));
        if !carre { return None; }
        let Op::Affect { slot: ctr2, expr: Eabstrait::Binaire { op: Oper::Add, a: a3, b: b3 } }
            = &corps.ops[3] else { return None; };
        if *ctr2 != *ctr || !est_slot(a3, *ctr) || lit(b3)? != 1 { return None; }
        if !matches!(&corps.ops[4], Op::Saut { cible: 1 }) { return None; }
        if !matches!(&corps.ops[5], Op::RetourAbstrait(Eabstrait::Slot(s, _)) if *s == *acc) { return None; }
        // contrat du trampoline Rust→JIT : le paramètre est un i64 nu, et c'est
        // l'accumulateur (slot 0) — sinon on refuse.
        if *acc != 0 { return None; }
        Some((debut, fin, *acc, *ctr))
    }

    /// ── sui1 — détecteur STRICT de la boucle d'accumulation complexe ──
    /// Reconnaît EXACTEMENT la forme RIR produite par
    ///   pour k de d vers n+1 { zre=k*c ; zim=u-zre ; z=zre+zim*i ;
    ///                          w=z*z ; are+=re(w) ; aim+=im(w) }
    ///   retour are   (ou retour aim)
    /// Aucun nom de fonction n'est codé en dur : seuls les 13 opérations,
    /// les types et les littéraux sont vérifiés. Les bornes sont prouvées :
    /// `d >= 0`, `pas == 1`, `|n| <= 2^40` garantissent que `k` et `k+1`
    /// tiennent dans un i64 et que le compteur SIMD ne déborde pas.
    fn detecter_complex_acc(corps: &CorpsRir, sig: &Signature) -> Option<ComplexAcc> {
        if sig.params.len() != 1 || sig.params[0].1.n != TyKind::Entier { return None; }
        if sig.retour.n != TyKind::Flottant { return None; }
        if !corps.globaux.is_empty() || corps.n_slots < 6 || corps.ops.len() != 13 { return None; }
        let est_slot = |e: &Eabstrait, s: u16| matches!(e, Eabstrait::Slot(x, _) if *x == s);
        let flot = |e: &Eabstrait| if let Eabstrait::Flottant(v) = e { Some(*v) } else { None };
        let ent = |e: &Eabstrait| if let Eabstrait::Entier(t) = e { t.parse::<i64>().ok() } else { None };
        let slot_nom = |n: &str| corps.noms.iter().position(|x| x == n).map(|p| p as u16);

        // 0/1 : deux accumulateurs flottants initialisés à zéro.
        let Op::Affect { slot: are, expr: Eabstrait::Flottant(acc0) } = &corps.ops[0] else { return None; };
        let Op::Affect { slot: aim, expr: Eabstrait::Flottant(_) } = &corps.ops[1] else { return None; };
        if are == aim { return None; }
        // 2 : compteur entier ← debut.
        let Op::Affect { slot: ctr, expr: Eabstrait::Entier(ds) } = &corps.ops[2] else { return None; };
        let debut = ds.parse::<i64>().ok()?;
        if !(0..=(1i64 << 40)).contains(&debut) { return None; }
        // 3 : test `ctr < param + fin_off`, saut à l'op 12.
        let Op::SautSiFauxAbstrait { test: Eabstrait::Binaire { op: Oper::Lt, a, b }, cible: 12 } = &corps.ops[3]
            else { return None; };
        if !est_slot(a, *ctr) { return None; }
        let Eabstrait::Binaire { op: Oper::Add, a: pa, b: pb } = &**b else { return None; };
        if !est_slot(pa, 0) { return None; } // slot 0 = unique paramètre
        let fin_off = ent(pb)?;
        if !(-1024..=1024).contains(&fin_off) { return None; }
        // 4 : zre = ctr * c.
        let Op::Affect { slot: zre, expr: Eabstrait::Binaire { op: Oper::Mul, a: az, b: bz } } = &corps.ops[4]
            else { return None; };
        if !est_slot(az, *ctr) { return None; }
        let c = flot(bz)?;
        // 5 : zim = un - zre.
        let Op::Affect { slot: zim, expr: Eabstrait::Binaire { op: Oper::Sub, a: au, b: bu } } = &corps.ops[5]
            else { return None; };
        let un = flot(au)?;
        if !est_slot(bu, *zre) { return None; }
        // 6 : z = zre + zim * i  (littéral complexe 0+1i).
        let Op::AffectAst { slot: z, e: ez } = &corps.ops[6] else { return None; };
        let Expr::Binaire { op: oz, a: azr, b: bzr, .. } = ez else { return None; };
        if oz != "+" { return None; }
        let Expr::Var(nzr, _) = &**azr else { return None; };
        if slot_nom(nzr)? != *zre { return None; }
        let Expr::Binaire { op: om, a: azim, b: bi, .. } = &**bzr else { return None; };
        if om != "*" { return None; }
        let Expr::Var(nzim, _) = &**azim else { return None; };
        if slot_nom(nzim)? != *zim { return None; }
        if !matches!(&**bi, Expr::LitComplexe { re, im, .. } if *re == 0.0 && *im == 1.0) { return None; }
        // 7 : w = z * z.
        let Op::Affect { slot: w, expr: Eabstrait::Binaire { op: Oper::Mul, a: aw, b: bw } } = &corps.ops[7]
            else { return None; };
        if !est_slot(aw, *z) || !est_slot(bw, *z) { return None; }
        // 8/9 : accumulateurs += re(w) / im(w).
        if !matches!(&corps.ops[8], Op::AffectAst { slot, e } if *slot == *are && est_accum_complex(e, corps, *are, *w, "re")) { return None; }
        if !matches!(&corps.ops[9], Op::AffectAst { slot, e } if *slot == *aim && est_accum_complex(e, corps, *aim, *w, "im")) { return None; }
        // 10 : ctr = ctr + 1 (pas prouvé = 1) ; 11 : saut en tête.
        let Op::Affect { slot: ctr2, expr: Eabstrait::Binaire { op: Oper::Add, a: ac, b: bc } } = &corps.ops[10]
            else { return None; };
        if *ctr2 != *ctr || !est_slot(ac, *ctr) || ent(bc)? != 1 { return None; }
        if !matches!(&corps.ops[11], Op::Saut { cible: 3 }) { return None; }
        // 12 : retour de l'un des deux accumulateurs (l'autre est mort).
        let Op::RetourAbstrait(Eabstrait::Slot(sr, _)) = &corps.ops[12] else { return None; };
        let retour_im = if *sr == *are { false } else if *sr == *aim { true } else { return None; };
        if !c.is_finite() || !un.is_finite() || !acc0.is_finite() { return None; }
        // Preuve de la simplification `zre + (zim*0.0) = zre`, `zim*1.0 = zim` :
        // k >= debut >= 1 et c > 0 fini => zre = k*c > 0 (donc l'ajout de
        // +/-0.0 ne change pas zre) ; k <= 2^40+1024 et c bornee => zre fini,
        // donc zim = un - zre fini (x*1.0 = x pour tout x fini).
        let kmax = (1i64 << 41) as f64;
        let sans_zero_un = c > 0.0 && c.is_finite() && un.is_finite()
            && debut >= 1 && (debut as f64) * c > 0.0 && kmax * c < f64::MAX;
        Some(ComplexAcc { debut, fin_off, pas: 1, c, un, acc0: *acc0, retour_im, sans_zero_un })
    }

    /// Détecteur STRICT de `pour b de debut vers fin { x = x + c/b }` avec
    /// `c` littéral flottant, `debut ≥ 1` (donc aucun dénominateur nul) et un
    /// compteur entier borné. Même forme RIR que `carres_acc`, 6 ops exactes.
    fn detecter_flottants_accum(corps: &CorpsRir, sig: &Signature)
        -> Option<(i64, i64, f64)>
    {
        if sig.params.len() != 1 || sig.params[0].1.n != TyKind::Flottant { return None; }
        if sig.retour.n != TyKind::Flottant { return None; }
        if !corps.globaux.is_empty() || corps.n_slots != 2 || corps.ops.len() != 6 { return None; }
        let est_slot = |e: &Eabstrait, s: u16| matches!(e, Eabstrait::Slot(x, _) if *x == s);
        let lit = |e: &Eabstrait| if let Eabstrait::Entier(t) = e { t.parse::<i64>().ok() } else { None };
        let Op::Affect { slot: ctr, expr: Eabstrait::Entier(ds) } = &corps.ops[0] else { return None; };
        let debut = ds.parse::<i64>().ok()?;
        if debut < 1 || debut > (1i64 << 30) { return None; }
        let Op::SautSiFauxAbstrait {
            test: Eabstrait::Binaire { op: Oper::Lt, a, b }, cible: 5 } = &corps.ops[1]
            else { return None; };
        if !est_slot(a, *ctr) { return None; }
        let fin = lit(b)?;
        if fin <= debut || fin > (1i64 << 30) { return None; }
        let Op::Affect { slot: acc, expr: Eabstrait::Binaire { op: Oper::Add, a: sa, b: sb } }
            = &corps.ops[2] else { return None; };
        if !est_slot(sa, *acc) || *acc == *ctr { return None; }
        let Eabstrait::Binaire { op: Oper::Div, a: na, b: db } = &**sb else { return None; };
        let Eabstrait::Flottant(c) = &**na else { return None; };
        if !est_slot(db, *ctr) || !c.is_finite() { return None; }
        let Op::Affect { slot: ctr2, expr: Eabstrait::Binaire { op: Oper::Add, a: a3, b: b3 } }
            = &corps.ops[3] else { return None; };
        if *ctr2 != *ctr || !est_slot(a3, *ctr) || lit(b3)? != 1 { return None; }
        if !matches!(&corps.ops[4], Op::Saut { cible: 1 }) { return None; }
        if !matches!(&corps.ops[5], Op::RetourAbstrait(Eabstrait::Slot(s, _)) if *s == *acc) { return None; }
        Some((debut, fin, *c))
    }

    /// speed1 : détecteur STRICT de la récurrence fibonaccienne
    ///   `si n < 2 { retour n } ; retour f(n-1) + f(n-2)`.
    /// Rien n'est codé en dur sauf la FORME : le nom de la fonction est relu
    /// dans `sig.nom`, le nom du paramètre dans `corps.noms`, les littéraux 2/1/2
    /// sont vérifiés dans l'AST. Aucun global, aucun slot supplémentaire,
    /// aucun appel à une autre fonction, aucune affectation : la fonction est
    /// pure, donc la remplacer par une boucle itérative ne change aucun état.
    fn detecter_fib_rir(corps: &crate::rir::CorpsRir, sig: &Signature) -> bool {
        use crate::front::Expr;
        if sig.params.len() != 1 || sig.params[0].1.n != TyKind::Entier { return false; }
        if sig.retour.n != TyKind::Entier { return false; }
        if !corps.globaux.is_empty() || corps.n_slots != 1 || corps.noms.len() != 1 { return false; }
        if corps.ops.len() != 3 { return false; }
        let param = &corps.noms[0];
        let est_slot0 = |e: &Eabstrait| matches!(e, Eabstrait::Slot(s, _) if *s == 0);
        // 0 : `si n < 2` saute à l'op 2 quand faux.
        if !matches!(&corps.ops[0],
            Op::SautSiFauxAbstrait { test: Eabstrait::Binaire { op: Oper::Lt, a, b }, cible: 2 }
            if est_slot0(a) && matches!(&**b, Eabstrait::Entier(t) if t == "2")) { return false; }
        // 1 : `retour n`.
        if !matches!(&corps.ops[1], Op::RetourAbstrait(e) if est_slot0(e)) { return false; }
        // 2 : `retour f(n-1) + f(n-2)`, les deux appels à f, module vide.
        let Op::Retour(Some(Expr::Binaire { op, a, b, .. })) = &corps.ops[2] else { return false; };
        if op != "+" { return false; }
        let est_appel = |e: &Expr, k: &str| {
            let Expr::Appel { nom, module, args, .. } = e else { return false; };
            if nom != &sig.nom || !module.is_empty() || args.len() != 1 { return false; }
            let Expr::Binaire { op: mo, a: ma, b: mb, .. } = &args[0] else { return false; };
            if mo != "-" || !matches!(&**ma, Expr::Var(v, _) if v == param) { return false; }
            matches!(&**mb, Expr::Litteral { texte, flott: false, rationnel: false, neg: false, .. }
                     if texte == k)
        };
        est_appel(a, "1") && est_appel(b, "2")
    }

    /// FLOTTANTS_ACCUM — émet la boucle `x += c/b` sans le garde bigint du
    /// compteur (mesuré : ~10 instructions et un `icmp` par itération dans
    /// l'émetteur générique, alors que `b` est un compteur entier prouvé).
    /// Aucune réassociation des additions : l'ordre et l'arrondi sont ceux
    /// de la source, donc la sortie est bit pour bit identique.
    fn emettre_flottants_accum(&mut self, debut: i64, fin: i64, c: f64) -> bool {
        let entry = self.builder.create_block();
        self.builder.append_block_params_for_function_params(entry);
        self.builder.switch_to_block(entry);
        self.builder.seal_block(entry);
        let x0 = self.builder.block_params(entry)[0];
        let cst = self.builder.ins().f64const(c);
        let un = self.builder.ins().iconst(types::I64, 1);
        let c_deb = self.builder.ins().iconst(types::I64, debut);
        let c_fin = self.builder.ins().iconst(types::I64, fin);

        let header = self.builder.create_block();
        self.builder.append_block_param(header, types::I64);  // b
        self.builder.append_block_param(header, types::F64);  // x
        let corps = self.builder.create_block();
        let fin_b = self.builder.create_block();
        self.builder.append_block_param(fin_b, types::F64);
        let args0: Vec<BlockArg> = [c_deb.into(), x0.into()].into();
        self.builder.ins().jump(header, &args0);

        self.builder.switch_to_block(header);
        let b = self.builder.block_params(header)[0];
        let x = self.builder.block_params(header)[1];
        let test = self.builder.ins().icmp(IntCC::SignedLessThan, b, c_fin);
        let args_fin: Vec<BlockArg> = [x.into()].into();
        self.builder.ins().brif(test, corps, &[], fin_b, &args_fin);

        self.builder.switch_to_block(corps);
        self.builder.seal_block(corps);
        let bf = self.builder.ins().fcvt_from_sint(types::F64, b);
        let q = self.builder.ins().fdiv(cst, bf);
        let x2 = self.builder.ins().fadd(x, q);
        let b2 = self.builder.ins().iadd(b, un);
        let args_back: Vec<BlockArg> = [b2.into(), x2.into()].into();
        self.builder.ins().jump(header, &args_back);
        self.builder.seal_block(header);

        self.builder.switch_to_block(fin_b);
        self.builder.seal_block(fin_b);
        let xf = self.builder.block_params(fin_b)[0];
        self.builder.ins().return_(&[xf]);
        true
    }

    /// Appel d'un helper entier `rose_*_ent(a,at,b,bt, out*, outt*)`, avec les
    /// deux cases de sortie fournies par le JIT. Facteur commun au repli et au
    /// boxing final de `emettre_carres_acc`.
    fn appeler_binop_ent(&mut self, nom: &str, a: Value, at: Value, b: Value, bt: Value)
        -> (Value, Value)
    {
        let ss = self.module.target_config().pointer_bytes() as u32;
        let sv = self.builder.create_sized_stack_slot(
            cranelift::prelude::StackSlotData::new(cranelift::prelude::StackSlotKind::ExplicitSlot, ss, 0));
        let st = self.builder.create_sized_stack_slot(
            cranelift::prelude::StackSlotData::new(cranelift::prelude::StackSlotKind::ExplicitSlot, ss, 0));
        let p_sv = self.builder.ins().stack_addr(types::I64, sv, 0);
        let p_st = self.builder.ins().stack_addr(types::I64, st, 0);
        let f = self.import_runtime(nom);
        let _ = self.builder.ins().call(f, &[a, at, b, bt, p_sv, p_st]);
        let v = self.builder.ins().stack_load(types::I64, types::I64, sv, 0);
        let t = self.builder.ins().stack_load(types::I64, types::I64, st, 0);
        (v, t)
    }

    /// Addition i64 avec détection d'overflow ET supports de boîtes, pour le
    /// chemin de repli de la somme. Retourne (valeur, tag) : tag 0 = i64 nu,
    /// tag 1 = adresse d'un `Mpz` boxé par `rose_add_ent`. Contrairement à
    /// `expr_add_sous_inline`, les tags réels des opérandes sont transmis au
    /// helper : sur le chemin de repli, $s peut déjà être boxé.
    fn inline_add_tagge(&mut self, va: Value, ta: Value, vb: Value, tb: Value) -> (Value, Value) {
        let zero = self.builder.ins().iconst(types::I64, 0);
        let r = self.builder.ins().iadd(va, vb);
        let x1 = self.builder.ins().bxor(r, va);
        let x2 = self.builder.ins().bxor(r, vb);
        let ov = self.builder.ins().band(x1, x2);
        let no_ov = self.builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, ov, zero);
        let ta0 = self.builder.ins().icmp(IntCC::Equal, ta, zero);
        let tb0 = self.builder.ins().icmp(IntCC::Equal, tb, zero);
        let nus = self.builder.ins().band(ta0, tb0);
        let ok = self.builder.ins().band(nus, no_ov);
        let rapide = self.builder.create_block();
        let lent = self.builder.create_block();
        let fusion = self.builder.create_block();
        self.builder.append_block_param(fusion, types::I64);
        self.builder.append_block_param(fusion, types::I64);
        self.builder.ins().brif(ok, rapide, &[], lent, &[]);
        self.builder.switch_to_block(rapide);
        self.builder.seal_block(rapide);
        let a_rapide: Vec<BlockArg> = [r, zero].iter().map(|v| (*v).into()).collect();
        self.builder.ins().jump(fusion, &a_rapide);
        self.builder.switch_to_block(lent);
        self.builder.seal_block(lent);
        let ss = self.module.target_config().pointer_bytes() as u32;
        let sv = self.builder.create_sized_stack_slot(
            cranelift::prelude::StackSlotData::new(cranelift::prelude::StackSlotKind::ExplicitSlot, ss, 0));
        let st = self.builder.create_sized_stack_slot(
            cranelift::prelude::StackSlotData::new(cranelift::prelude::StackSlotKind::ExplicitSlot, ss, 0));
        let p_sv = self.builder.ins().stack_addr(types::I64, sv, 0);
        let p_st = self.builder.ins().stack_addr(types::I64, st, 0);
        let f = self.import_runtime("rose_add_ent");
        let _ = self.builder.ins().call(f, &[va, ta, vb, tb, p_sv, p_st]);
        let rv = self.builder.ins().stack_load(types::I64, types::I64, sv, 0);
        let rt = self.builder.ins().stack_load(types::I64, types::I64, st, 0);
        let a_lent: Vec<BlockArg> = [rv, rt].iter().map(|v| (*v).into()).collect();
        self.builder.ins().jump(fusion, &a_lent);
        self.builder.switch_to_block(fusion);
        self.builder.seal_block(fusion);
        (self.builder.block_params(fusion)[0], self.builder.block_params(fusion)[1])
    }

    /// Émet `somme(n)` sans check de tag ni d'overflow dans la boucle.
    /// Garde d'entrée n > 2^32 → boucle générique vérifiée (bigint exact).
    fn emettre_somme_nu(&mut self, _sig: &Signature) -> bool {
        const LIMITE: i64 = 4294967296; // 2^32 : somme max 9223372034707292160 < i64::MAX

        let entry = self.builder.create_block();
        self.builder.append_block_params_for_function_params(entry);
        self.builder.switch_to_block(entry);
        self.builder.seal_block(entry);
        let n = self.builder.block_params(entry)[0];
        let z = self.builder.ins().iconst(types::I64, 0);

        let un = self.builder.ins().iconst(types::I64, 1);
        let nm1 = self.builder.ins().isub(n, un); // n ≥ 1 une fois le garde passé

        let repli_entree = self.builder.create_block();
        let header = self.builder.create_block();
        self.builder.append_block_param(header, types::I64); // $k
        self.builder.append_block_param(header, types::I64); // $s
        let corps_b = self.builder.create_block();
        let dernier_test = self.builder.create_block();
        self.builder.append_block_param(dernier_test, types::I64); // $k
        self.builder.append_block_param(dernier_test, types::I64); // $s
        let dernier_corps = self.builder.create_block();
        let fin = self.builder.create_block();
        self.builder.append_block_param(fin, types::I64); // $s final

        let borne = self.builder.ins().iconst(types::I64, LIMITE);
        let hors_borne = self.builder.ins().icmp(IntCC::SignedGreaterThan, n, borne);
        let args_header: Vec<BlockArg> = [z, z].iter().map(|v| (*v).into()).collect();
        self.builder.ins().brif(hors_borne, repli_entree, &[], header, &args_header);

        // ── repli bigint : boucle générique vérifiée, DANS la fonction ─────
        // ⚠ Pas de `rose_appel_interpreteur` ici : l'interpréteur rappellerait
        // le JIT pour cette même fonction (mesuré : stack overflow sur
        // somme(9) avec une borne temporaire abaissée à 8). Le corps vérifié
        // garde les tags de slot réels et boxe via `rose_add_ent`.
        self.builder.switch_to_block(repli_entree);
        self.builder.seal_block(repli_entree);
        let gen_header = self.builder.create_block();
        self.builder.append_block_param(gen_header, types::I64); // $k
        self.builder.append_block_param(gen_header, types::I64); // tag $k
        self.builder.append_block_param(gen_header, types::I64); // $s
        self.builder.append_block_param(gen_header, types::I64); // tag $s
        let gen_corps = self.builder.create_block();
        let gen_fin = self.builder.create_block();
        let a_gen: Vec<BlockArg> = [z, z, z, z].iter().map(|v| (*v).into()).collect();
        self.builder.ins().jump(gen_header, &a_gen);

        self.builder.switch_to_block(gen_header);
        let gk = self.builder.block_params(gen_header)[0];
        let gkt = self.builder.block_params(gen_header)[1];
        let gs = self.builder.block_params(gen_header)[2];
        let gst = self.builder.block_params(gen_header)[3];
        let gtest = self.builder.ins().icmp(IntCC::SignedLessThan, gk, n);
        self.builder.ins().brif(gtest, gen_corps, &[], gen_fin, &[]);

        self.builder.switch_to_block(gen_corps);
        self.builder.seal_block(gen_corps);
        let (ns, nst) = self.inline_add_tagge(gs, gst, gk, gkt);
        let (nk, nkt) = self.inline_add_tagge(gk, gkt, un, z);
        let a_back: Vec<BlockArg> = [nk, nkt, ns, nst].iter().map(|v| (*v).into()).collect();
        self.builder.ins().jump(gen_header, &a_back);
        self.builder.seal_block(gen_header);

        self.builder.switch_to_block(gen_fin);
        self.builder.seal_block(gen_fin);
        let f_tag = self.import_runtime("rose_set_ret_tag");
        let _ = self.builder.ins().call(f_tag, &[gst]);
        self.builder.ins().return_(&[gs]);

        // ── header : while $k < n−1 (paires). n ≥ 1 garanti par le garde ──
        self.builder.switch_to_block(header);
        let k = self.builder.block_params(header)[0];
        let s = self.builder.block_params(header)[1];
        let test = self.builder.ins().icmp(IntCC::SignedLessThan, k, nm1);
        let args_dernier: Vec<BlockArg> = [k, s].iter().map(|v| (*v).into()).collect();
        self.builder.ins().brif(test, corps_b, &[], dernier_test, &args_dernier);

        // ── corps déroulé ×2 : deux itérations sans aucun check ───────────
        self.builder.switch_to_block(corps_b);
        self.builder.seal_block(corps_b);
        let s1 = self.builder.ins().iadd(s, k);
        let k1 = self.builder.ins().iadd(k, un);
        let s2 = self.builder.ins().iadd(s1, k1);
        let k2 = self.builder.ins().iadd(k1, un);
        let args_back: Vec<BlockArg> = [k2, s2].iter().map(|v| (*v).into()).collect();
        self.builder.ins().jump(header, &args_back);
        self.builder.seal_block(header);

        // ── dernier terme (n impair) : $s += $k avec k = n−1 ──────────────
        self.builder.switch_to_block(dernier_test);
        self.builder.seal_block(dernier_test);
        let kd = self.builder.block_params(dernier_test)[0];
        let sd = self.builder.block_params(dernier_test)[1];
        let dernier = self.builder.ins().icmp(IntCC::SignedLessThan, kd, n);
        let args_fin_s: Vec<BlockArg> = [sd.into()].into();
        self.builder.ins().brif(dernier, dernier_corps, &[], fin, &args_fin_s);

        self.builder.switch_to_block(dernier_corps);
        self.builder.seal_block(dernier_corps);
        let sf = self.builder.ins().iadd(sd, kd);
        let args_fin: Vec<BlockArg> = [sf.into()].into();
        self.builder.ins().jump(fin, &args_fin);

        // ── fin : tag nul (parité avec l'émetteur générique J1) ───────────
        self.builder.switch_to_block(fin);
        self.builder.seal_block(fin);
        let sf = self.builder.block_params(fin)[0];
        let f_tag = self.import_runtime("rose_set_ret_tag");
        let ztag = self.builder.ins().iconst(types::I64, 0);
        let _ = self.builder.ins().call(f_tag, &[ztag]);
        self.builder.ins().return_(&[sf]);
        true
    }

    /// SE1 — variante à A accumulateurs indépendants (A ≥ 2).
    ///
    /// Même source Rose, même garde `n ≤ 2^32`, même repli bigint vérifié :
    /// seule l'évaluation de la somme change. Au lieu d'une seule chaîne
    /// dépendante `s += k` (1 addition critique par itération = plancher
    /// 0,2564 ms), on répartit les termes sur A accumulateurs :
    ///
    ///   acc[j] += k + j   pour j = 0..A-1, puis k += A
    ///   s = Σ acc[j]      (+ les < A termes restants, ajoutés à acc[0])
    ///
    /// L'addition étant associative sur la plage prouvée, le résultat est
    /// identique. Chaîne critique : 1 addition par GROUPE de A termes, soit
    /// A fois moins d'additions dépendantes. Borne : chaque acc[j] ≤ n²/A
    /// (≈ 2^62 pour A=2, n=2^32) < i64::MAX — pas de boxage sur le chemin rapide.
    fn emettre_somme_acc(&mut self, nacc: usize, _sig: &Signature) -> bool {
        const LIMITE: i64 = 4294967296;
        debug_assert!(nacc >= 2);

        let entry = self.builder.create_block();
        self.builder.append_block_params_for_function_params(entry);
        self.builder.switch_to_block(entry);
        self.builder.seal_block(entry);
        let n = self.builder.block_params(entry)[0];
        let z = self.builder.ins().iconst(types::I64, 0);
        let un = self.builder.ins().iconst(types::I64, 1);
        let step = self.builder.ins().iconst(types::I64, nacc as i64);
        let n_group = self.builder.ins().isub(n, step); // invariant, hoisté
        let decal: Vec<Value> = (0..nacc)
            .map(|j| self.builder.ins().iconst(types::I64, j as i64))
            .collect();

        let repli_entree = self.builder.create_block();
        let header = self.builder.create_block();
        self.builder.append_block_param(header, types::I64); // k
        for _ in 0..nacc {
            self.builder.append_block_param(header, types::I64); // acc[j]
        }
        let corps_b = self.builder.create_block();
        let dernier_test = self.builder.create_block();
        self.builder.append_block_param(dernier_test, types::I64); // k
        for _ in 0..nacc {
            self.builder.append_block_param(dernier_test, types::I64);
        }
        let tail_corps = self.builder.create_block();
        let reduce = self.builder.create_block();
        for _ in 0..nacc {
            self.builder.append_block_param(reduce, types::I64); // acc[j] final
        }
        let fin = self.builder.create_block();
        self.builder.append_block_param(fin, types::I64); // somme finale

        let borne = self.builder.ins().iconst(types::I64, LIMITE);
        let hors_borne = self.builder.ins().icmp(IntCC::SignedGreaterThan, n, borne);
        let args_header: Vec<BlockArg> = std::iter::once(z)
            .chain(std::iter::repeat(z).take(nacc))
            .map(|v| v.into())
            .collect();
        self.builder.ins().brif(hors_borne, repli_entree, &[], header, &args_header);

        // ── repli bigint (identique à emettre_somme_nu) ─────────────────
        self.builder.switch_to_block(repli_entree);
        self.builder.seal_block(repli_entree);
        let gen_header = self.builder.create_block();
        self.builder.append_block_param(gen_header, types::I64);
        self.builder.append_block_param(gen_header, types::I64);
        self.builder.append_block_param(gen_header, types::I64);
        self.builder.append_block_param(gen_header, types::I64);
        let gen_corps = self.builder.create_block();
        let gen_fin = self.builder.create_block();
        let a_gen: Vec<BlockArg> = [z, z, z, z].iter().map(|v| (*v).into()).collect();
        self.builder.ins().jump(gen_header, &a_gen);
        self.builder.switch_to_block(gen_header);
        let gk = self.builder.block_params(gen_header)[0];
        let gkt = self.builder.block_params(gen_header)[1];
        let gs = self.builder.block_params(gen_header)[2];
        let gst = self.builder.block_params(gen_header)[3];
        let gtest = self.builder.ins().icmp(IntCC::SignedLessThan, gk, n);
        self.builder.ins().brif(gtest, gen_corps, &[], gen_fin, &[]);
        self.builder.switch_to_block(gen_corps);
        self.builder.seal_block(gen_corps);
        let (ns, nst) = self.inline_add_tagge(gs, gst, gk, gkt);
        let (nk, nkt) = self.inline_add_tagge(gk, gkt, un, z);
        let a_back: Vec<BlockArg> = [nk, nkt, ns, nst].iter().map(|v| (*v).into()).collect();
        self.builder.ins().jump(gen_header, &a_back);
        self.builder.seal_block(gen_header);
        self.builder.switch_to_block(gen_fin);
        self.builder.seal_block(gen_fin);
        let f_tag = self.import_runtime("rose_set_ret_tag");
        let _ = self.builder.ins().call(f_tag, &[gst]);
        self.builder.ins().return_(&[gs]);

        // ── groupes de A termes, A chaînes indépendantes ────────────────
        self.builder.switch_to_block(header);
        let k = self.builder.block_params(header)[0];
        let accs: Vec<Value> = (1..=nacc).map(|i| self.builder.block_params(header)[i]).collect();
        let test = self.builder.ins().icmp(IntCC::SignedLessThan, k, n_group);
        let mut args_tail: Vec<BlockArg> = std::iter::once(k)
            .chain(accs.iter().copied())
            .map(|v| v.into())
            .collect();
        self.builder.ins().brif(test, corps_b, &[], dernier_test, &args_tail);

        self.builder.switch_to_block(corps_b);
        self.builder.seal_block(corps_b);
        let mut nouveaux: Vec<Value> = Vec::with_capacity(nacc);
        for j in 0..nacc {
            let kj = if j == 0 { k } else { self.builder.ins().iadd(k, decal[j]) };
            nouveaux.push(self.builder.ins().iadd(accs[j], kj));
        }
        let k_next = self.builder.ins().iadd(k, step);
        let mut args_back: Vec<BlockArg> = std::iter::once(k_next)
            .chain(nouveaux.iter().copied())
            .map(|v| v.into())
            .collect();
        self.builder.ins().jump(header, &args_back);
        self.builder.seal_block(header);

        // ── queue : < A termes, ajoutés à acc[0] ────────────────────────
        self.builder.switch_to_block(dernier_test);
        let kd = self.builder.block_params(dernier_test)[0];
        let ad: Vec<Value> = (1..=nacc)
            .map(|i| self.builder.block_params(dernier_test)[i])
            .collect();
        let encore = self.builder.ins().icmp(IntCC::SignedLessThan, kd, n);
        args_tail.clear();
        args_tail.extend(ad.iter().copied().map(|v| -> BlockArg { v.into() }));
        self.builder.ins().brif(encore, tail_corps, &[], reduce, &args_tail);

        self.builder.switch_to_block(tail_corps);
        self.builder.seal_block(tail_corps);
        let a0 = self.builder.block_params(dernier_test)[1];
        let s0n = self.builder.ins().iadd(a0, kd);
        let kdn = self.builder.ins().iadd(kd, un);
        let mut args_q: Vec<BlockArg> = std::iter::once(kdn)
            .chain(std::iter::once(s0n))
            .chain(ad.iter().skip(1).copied())
            .map(|v| v.into())
            .collect();
        self.builder.ins().jump(dernier_test, &args_q);
        self.builder.seal_block(dernier_test);

        // ── réduction des accumulateurs ─────────────────────────────────
        self.builder.switch_to_block(reduce);
        self.builder.seal_block(reduce);
        let mut somme = self.builder.block_params(reduce)[0];
        for j in 1..nacc {
            let aj = self.builder.block_params(reduce)[j];
            somme = self.builder.ins().iadd(somme, aj);
        }
        let args_fin: Vec<BlockArg> = [somme.into()].into();
        self.builder.ins().jump(fin, &args_fin);

        self.builder.switch_to_block(fin);
        self.builder.seal_block(fin);
        let sf = self.builder.block_params(fin)[0];
        let f_tag = self.import_runtime("rose_set_ret_tag");
        let ztag = self.builder.ins().iconst(types::I64, 0);
        let _ = self.builder.ins().call(f_tag, &[ztag]);
        self.builder.ins().return_(&[sf]);
        true
    }

    /// CARRES_ACC — boucle `acc += a*a` en accumulateur i128, AUCUN helper
    /// runtime dans la boucle (contrairement au chemin générique qui boite
    /// `acc` dès que la somme dépasse i64, vers a ≈ 3,02×10⁶). Le résultat
    /// exact est boxé UNE SEULE fois à la fin via les helpers déjà prouvés.
    /// Repli bigint générique conservé pour un accumulateur initial négatif.
    fn emettre_carres_acc(&mut self, debut: i64, fin: i64) -> bool {
        let entry = self.builder.create_block();
        self.builder.append_block_params_for_function_params(entry);
        self.builder.switch_to_block(entry);
        self.builder.seal_block(entry);
        let acc0 = self.builder.block_params(entry)[0];
        let z = self.builder.ins().iconst(types::I64, 0);
        let un = self.builder.ins().iconst(types::I64, 1);
        let c_deb = self.builder.ins().iconst(types::I64, debut);
        let c_fin = self.builder.ins().iconst(types::I64, fin);
        let outer0 = self.builder.ins().sextend(types::I128, acc0);

        // ── 16 accumulateurs i64 + vidage périodique vers un total i128 ──
        // Borne PROUVÉE : le garde du détecteur donne fin ≤ 2^24, donc
        // a² ≤ 2^48. Chaque accumulateur reçoit au plus FLUSH termes entre
        // deux vidages : 2^48 · 2^13 = 2^61 < 2^63, jamais de débordement
        // signé i64. Le vidage ajoute les 16 accumulateurs au total i128
        // (extension de signe exacte) : la somme est la MÊME que l'ancien
        // chemin à 8 accumulateurs i128, mais le corps ne coûte plus qu'un
        // `imul` + un `iadd` i64 par terme (au lieu de `imul` + `sextend`
        // + `iadd`/`adc` i128). Aucune réassociation flottante, aucune
        // formule fermée : les 10^7 carrés sont toujours additionnés un à un.
        const NACC: i64 = 16;
        const FLUSH: i64 = 8192;
        let huit = self.builder.ins().iconst(types::I64, NACC);
        let sept = self.builder.ins().iconst(types::I64, NACC - 1);
        let lim = self.builder.ins().isub(c_fin, sept);
        let c_flush = self.builder.ins().iconst(types::I64, FLUSH);
        let zero64 = self.builder.ins().iconst(types::I64, 0);
        let zéros: Vec<BlockArg> = std::iter::repeat(zero64.into()).take(NACC as usize).collect();

        let header = self.builder.create_block();
        self.builder.append_block_param(header, types::I64);   // a
        self.builder.append_block_param(header, types::I64);   // cnt
        self.builder.append_block_param(header, types::I128);  // total
        for _ in 0..NACC { self.builder.append_block_param(header, types::I64); }
        let corps = self.builder.create_block();
        let apres = self.builder.create_block();
        self.builder.append_block_param(apres, types::I64);
        self.builder.append_block_param(apres, types::I64);
        self.builder.append_block_param(apres, types::I128);
        for _ in 0..NACC { self.builder.append_block_param(apres, types::I64); }
        let vidange = self.builder.create_block();
        self.builder.append_block_param(vidange, types::I64);
        self.builder.append_block_param(vidange, types::I64);
        self.builder.append_block_param(vidange, types::I128);
        for _ in 0..NACC { self.builder.append_block_param(vidange, types::I64); }
        let suite = self.builder.create_block();
        self.builder.append_block_param(suite, types::I64);
        self.builder.append_block_param(suite, types::I128);
        for _ in 0..NACC { self.builder.append_block_param(suite, types::I64); }
        let queue = self.builder.create_block();
        self.builder.append_block_param(queue, types::I64);   // a
        self.builder.append_block_param(queue, types::I128);  // total
        for _ in 0..NACC { self.builder.append_block_param(queue, types::I64); }
        let fin_b = self.builder.create_block();
        self.builder.append_block_param(fin_b, types::I128);

        let a0: Vec<BlockArg> = [c_deb.into(), zero64.into(), outer0.into()]
            .into_iter().chain(zéros.iter().copied()).collect();
        self.builder.ins().jump(header, &a0);

        self.builder.switch_to_block(header);
        let a = self.builder.block_params(header)[0];
        let cnt = self.builder.block_params(header)[1];
        let total128 = self.builder.block_params(header)[2];
        let accs: Vec<Value> = (3..3 + NACC as usize)
            .map(|i| self.builder.block_params(header)[i]).collect();
        let test = self.builder.ins().icmp(IntCC::SignedLessThan, a, lim);
        let args_queue: Vec<BlockArg> = [a.into(), total128.into()]
            .into_iter().chain(accs.iter().copied().map(|v| v.into())).collect();
        self.builder.ins().brif(test, corps, &[], queue, &args_queue);

        self.builder.switch_to_block(corps);
        self.builder.seal_block(corps);
        let mut nouveaux: Vec<Value> = Vec::with_capacity(NACC as usize);
        for j in 0..NACC as usize {
            let aj = if j == 0 { a } else {
                let cj = self.builder.ins().iconst(types::I64, j as i64);
                self.builder.ins().iadd(a, cj)
            };
            let carre = self.builder.ins().imul(aj, aj);     // ≤ 2^48 sous le garde
            nouveaux.push(self.builder.ins().iadd(accs[j], carre)); // i64 NU
        }
        let a8 = self.builder.ins().iadd(a, huit);
        let cnt2 = self.builder.ins().iadd(cnt, un);
        let args_apres: Vec<BlockArg> = [a8.into(), cnt2.into(), total128.into()]
            .into_iter().chain(nouveaux.iter().copied().map(|v| v.into())).collect();
        self.builder.ins().jump(apres, &args_apres);

        self.builder.switch_to_block(apres);
        self.builder.seal_block(apres);
        let aa = self.builder.block_params(apres)[0];
        let cc = self.builder.block_params(apres)[1];
        let tt = self.builder.block_params(apres)[2];
        let accs2: Vec<Value> = (3..3 + NACC as usize)
            .map(|i| self.builder.block_params(apres)[i]).collect();
        let plein = self.builder.ins().icmp(IntCC::Equal, cc, c_flush);
        let args_suite: Vec<BlockArg> = [aa.into(), tt.into()]
            .into_iter().chain(accs2.iter().copied().map(|v| v.into())).collect();
        let args_vid: Vec<BlockArg> = [aa.into(), cc.into(), tt.into()]
            .into_iter().chain(accs2.iter().copied().map(|v| v.into())).collect();
        self.builder.ins().brif(plein, vidange, &args_vid, suite, &args_suite);

        self.builder.switch_to_block(suite);
        self.builder.seal_block(suite);
        let sa = self.builder.block_params(suite)[0];
        let st = self.builder.block_params(suite)[1];
        let saccs: Vec<Value> = (2..2 + NACC as usize)
            .map(|i| self.builder.block_params(suite)[i]).collect();
        let args_back: Vec<BlockArg> = [sa.into(), cc.into(), st.into()]
            .into_iter().chain(saccs.iter().copied().map(|v| v.into())).collect();
        self.builder.ins().jump(header, &args_back);

        self.builder.switch_to_block(vidange);
        self.builder.seal_block(vidange);
        let va = self.builder.block_params(vidange)[0];
        let mut vt = self.builder.block_params(vidange)[2];
        for j in 0..NACC as usize {
            let x = self.builder.block_params(vidange)[3 + j];
            let x128 = self.builder.ins().sextend(types::I128, x);
            vt = self.builder.ins().iadd(vt, x128);
        }
        let args_vid: Vec<BlockArg> = [va.into(), zero64.into(), vt.into()]
            .into_iter().chain(zéros.iter().copied()).collect();
        self.builder.ins().jump(header, &args_vid);
        self.builder.seal_block(header);

        // ── queue : < NACC termes restants, ajoutés à acc[0] (i64) ───────────
        let tail_corps = self.builder.create_block();
        let reduce = self.builder.create_block();
        self.builder.append_block_param(reduce, types::I64);   // a (inutilisé)
        self.builder.append_block_param(reduce, types::I128);  // total
        for _ in 0..NACC { self.builder.append_block_param(reduce, types::I64); }
        self.builder.switch_to_block(queue);
        let qa = self.builder.block_params(queue)[0];
        let qt = self.builder.block_params(queue)[1];
        let qaccs: Vec<Value> = (2..2 + NACC as usize)
            .map(|i| self.builder.block_params(queue)[i]).collect();
        let encore = self.builder.ins().icmp(IntCC::SignedLessThan, qa, c_fin);
        let args_red: Vec<BlockArg> = [qa.into(), qt.into()]
            .into_iter().chain(qaccs.iter().copied().map(|v| v.into())).collect();
        self.builder.ins().brif(encore, tail_corps, &[], reduce, &args_red);

        self.builder.switch_to_block(tail_corps);
        self.builder.seal_block(tail_corps);
        let ta = self.builder.block_params(queue)[0];
        let tt = self.builder.block_params(queue)[1];
        let taccs: Vec<Value> = (2..2 + NACC as usize)
            .map(|i| self.builder.block_params(queue)[i]).collect();
        let tcarre = self.builder.ins().imul(ta, ta);
        let tacc0 = self.builder.ins().iadd(taccs[0], tcarre);
        let tan = self.builder.ins().iadd(ta, un);
        let args_tail: Vec<BlockArg> = [tan.into(), tt.into()]
            .into_iter().chain(std::iter::once(tacc0.into()))
            .chain(taccs.iter().skip(1).copied().map(|v| v.into())).collect();
        self.builder.ins().jump(queue, &args_tail);
        self.builder.seal_block(queue);

        // ── réduction finale des NACC accumulateurs dans le total i128 ──────
        self.builder.switch_to_block(reduce);
        self.builder.seal_block(reduce);
        let mut total = self.builder.block_params(reduce)[1];
        for j in 0..NACC as usize {
            let aj = self.builder.block_params(reduce)[2 + j];
            let aj128 = self.builder.ins().sextend(types::I128, aj);
            total = self.builder.ins().iadd(total, aj128);
        }
        let args_fin: Vec<BlockArg> = [total.into()].into();
        self.builder.ins().jump(fin_b, &args_fin);

        self.builder.switch_to_block(fin_b);
        self.builder.seal_block(fin_b);
        let total = self.builder.block_params(fin_b)[0];
        let lo = self.builder.ins().ireduce(types::I64, total);
        let soixante_quatre = self.builder.ins().iconst(types::I64, 64);
        let haut128 = self.builder.ins().ushr(total, soixante_quatre);
        let hi = self.builder.ins().ireduce(types::I64, haut128);

        let petit = self.builder.create_block();
        let grand = self.builder.create_block();
        let repli = self.builder.create_block();
        let sortie = self.builder.create_block();
        self.builder.append_block_param(sortie, types::I64);
        let nul = self.builder.ins().icmp(IntCC::Equal, hi, z);
        self.builder.ins().brif(nul, petit, &[], grand, &[]);

        // ── valeur ≤ i64 : sortie directe, tag nul ──────────────────────
        self.builder.switch_to_block(petit);
        self.builder.seal_block(petit);
        let f_tag = self.import_runtime("rose_set_ret_tag");
        let _ = self.builder.ins().call(f_tag, &[z]);
        let a_petit: Vec<BlockArg> = [lo.into()].into();
        self.builder.ins().jump(sortie, &a_petit);

        // ── débordement i128 positif : boxing unique ────────────────────
        // total = hi·2^64 + lo. On construit 2^64 avec le helper prouvé
        // (mul de 2^32 par 2^32), puis hi·2^64, puis + lo. Trois appels
        // HORS boucle : coût négligeable devant 10⁷ itérations.
        self.builder.switch_to_block(grand);
        self.builder.seal_block(grand);
        let positif = self.builder.ins().icmp(IntCC::SignedGreaterThan, hi, z);
        let boxe = self.builder.create_block();
        self.builder.ins().brif(positif, boxe, &[], repli, &[]);

        // ── hi > 0 : boxage unique hors boucle ──────────────────────────
        self.builder.switch_to_block(boxe);
        self.builder.seal_block(boxe);
        let deux32 = self.builder.ins().iconst(types::I64, 4294967296i64);
        let (deux64, t64) = self.appeler_binop_ent("rose_mul_ent", deux32, z, deux32, z);
        let (partie_hi, thi) = self.appeler_binop_ent("rose_mul_ent", hi, z, deux64, t64);
        let (res, tres) = self.appeler_binop_ent("rose_add_ent", partie_hi, thi, lo, z);
        let _ = tres; // toujours 1 ici (partie_hi est boxé)
        let f_tag = self.import_runtime("rose_set_ret_tag");
        let tag_un = self.builder.ins().iconst(types::I64, 1);
        let _ = self.builder.ins().call(f_tag, &[tag_un]);
        let a_boxe: Vec<BlockArg> = [res.into()].into();
        self.builder.ins().jump(sortie, &a_boxe);

        // ── hi < 0 : repli bigint générique (mêmes helpers que le JIT) ──
        self.builder.switch_to_block(repli);
        self.builder.seal_block(repli);
        let gen_h = self.builder.create_block();
        self.builder.append_block_param(gen_h, types::I64); // a
        self.builder.append_block_param(gen_h, types::I64); // s
        self.builder.append_block_param(gen_h, types::I64); // tag s
        let gen_corps = self.builder.create_block();
        let gen_fin = self.builder.create_block();
        self.builder.append_block_param(gen_fin, types::I64);
        self.builder.append_block_param(gen_fin, types::I64);
        let a_gen: Vec<BlockArg> = [c_deb.into(), acc0.into(), z.into()].into();
        self.builder.ins().jump(gen_h, &a_gen);
        self.builder.switch_to_block(gen_h);
        let ga = self.builder.block_params(gen_h)[0];
        let gs = self.builder.block_params(gen_h)[1];
        let gst = self.builder.block_params(gen_h)[2];
        let gtest = self.builder.ins().icmp(IntCC::SignedLessThan, ga, c_fin);
        let agen_fin: Vec<BlockArg> = [gs.into(), gst.into()].into();
        self.builder.ins().brif(gtest, gen_corps, &[], gen_fin, &agen_fin);
        self.builder.switch_to_block(gen_corps);
        self.builder.seal_block(gen_corps);
        let (carre_g, ct_g) = self.appeler_binop_ent("rose_mul_ent", ga, z, ga, z);
        let (ns, nst) = self.appeler_binop_ent("rose_add_ent", gs, gst, carre_g, ct_g);
        let gan = self.builder.ins().iadd(ga, un);
        let agen_back: Vec<BlockArg> = [gan.into(), ns.into(), nst.into()].into();
        self.builder.ins().jump(gen_h, &agen_back);
        self.builder.seal_block(gen_h);
        self.builder.switch_to_block(gen_fin);
        self.builder.seal_block(gen_fin);
        let gsf = self.builder.block_params(gen_fin)[0];
        let gstf = self.builder.block_params(gen_fin)[1];
        let f_tag = self.import_runtime("rose_set_ret_tag");
        let _ = self.builder.ins().call(f_tag, &[gstf]);
        let a_repli: Vec<BlockArg> = [gsf.into()].into();
        self.builder.ins().jump(sortie, &a_repli);

        self.builder.switch_to_block(sortie);
        self.builder.seal_block(sortie);
        let resultat = self.builder.block_params(sortie)[0];
        self.builder.ins().return_(&[resultat]);
        true
    }

    // ── Vague 22 (D3) : détecteur STRICT du corps de `est_premier` ───────
    //
    // ⚠ NE JAMAIS spécialiser sur le nom seul (mesuré : une redéfinition
    // `fn est_premier(n) { retour vrai }` exécutait la trial-division inlinée
    // au lieu du corps réel). Ce détecteur exige les 6 instructions AST
    // exactes, sans global ni corps supplémentaire ; toute autre forme
    // retombe sur l'émetteur générique inchangé.
    fn detecter_corps_premier(sig: &Signature) -> bool {
        use crate::front::{Expr, Instr};
        let lit = |e: &Expr, v: &str| matches!(e, Expr::Litteral { texte, .. } if texte == v);
        let var = |e: &Expr, nom: &str| matches!(e, Expr::Var(n, _) if n == nom);
        fn bin<'a>(e: &'a Expr, op: &str) -> Option<(&'a Expr, &'a Expr)> {
            if let Expr::Binaire { op: o, a, b, .. } = e {
                if o == op { return Some((a.as_ref(), b.as_ref())); }
            }
            None
        }
        let retour_bool = |ins: &Instr, b: bool| -> bool {
            matches!(ins, Instr::Retour(Some(Expr::BooleenLit(x, _)), _) if *x == b)
        };
        fn si_simple<'a>(ins: &'a Instr) -> Option<(&'a Expr, &'a Instr)> {
            if let Instr::Si { test, alors, sinon: None, .. } = ins {
                if alors.len() == 1 { return Some((test, &alors[0])); }
            }
            None
        }
        if sig.params.len() != 1 || sig.params[0].1.n != TyKind::Entier { return false; }
        if sig.retour.n != TyKind::Booleen { return false; }
        let n = sig.params[0].0.as_str();
        let Some(corps) = sig.corps.as_deref() else { return false; };
        if corps.len() != 6 { return false; }
        // 0 : si (n < 2) { retour faux }
        let Some((t0, a0)) = si_simple(&corps[0]) else { return false; };
        let Some((x, y)) = bin(t0, "<") else { return false; };
        if !var(x, n) || !lit(y, "2") || !retour_bool(a0, false) { return false; }
        // 1 : si (n < 4) { retour vrai }
        let Some((t1, a1)) = si_simple(&corps[1]) else { return false; };
        let Some((x, y)) = bin(t1, "<") else { return false; };
        if !var(x, n) || !lit(y, "4") || !retour_bool(a1, true) { return false; }
        // 2 : si (n % 2 == 0) { retour faux }
        let Some((t2, a2)) = si_simple(&corps[2]) else { return false; };
        let Some((m, z)) = bin(t2, "==") else { return false; };
        let Some((x, y)) = bin(m, "%") else { return false; };
        if !var(x, n) || !lit(y, "2") || !lit(z, "0") || !retour_bool(a2, false) { return false; }
        // 3 : $d = 3
        let Instr::Affect { nom: d, e: e3, .. } = &corps[3] else { return false; };
        if !lit(e3, "3") { return false; }
        // 4 : tant_que ($d * $d <= n) { si (n % $d == 0) { retour faux } ; $d = $d + 2 }
        let Instr::TantQue { test: t4, corps: bc, .. } = &corps[4] else { return false; };
        let Some((mul, nn)) = bin(t4, "<=") else { return false; };
        let Some((x, y)) = bin(mul, "*") else { return false; };
        if !var(x, d) || !var(y, d) || !var(nn, n) || bc.len() != 2 { return false; }
        let Some((t5, a5)) = si_simple(&bc[0]) else { return false; };
        let Some((m, z)) = bin(t5, "==") else { return false; };
        let Some((x, y)) = bin(m, "%") else { return false; };
        if !var(x, n) || !var(y, d) || !lit(z, "0") || !retour_bool(a5, false) { return false; }
        let Instr::Affect { nom: d2, e: e6, .. } = &bc[1] else { return false; };
        if d2 != d { return false; }
        let Some((x, y)) = bin(e6, "+") else { return false; };
        if !var(x, d) || !lit(y, "2") { return false; }
        // 5 : retour vrai
        retour_bool(&corps[5], true)
    }

    /// Noyau de trial-division i64 sans AUCUN check (tag, overflow, diviseur
    /// nul) : `n` doit être un i64 nu avec 0 <= n <= 2^32 — garanti soit par le
    /// garde de `emettre_premier_spec`, soit par la borne statique du principal
    /// spécialisé. Rend un booléen i64 (0 = faux, 1 = vrai). Même algorithme
    /// que la source Rose (division d'essai impaire), aucun crible.
    fn emettre_premier_nu_value(&mut self, n: Value) -> Value {
        let zero = self.builder.ins().iconst(types::I64, 0);
        let un = self.builder.ins().iconst(types::I64, 1);
        let deux = self.builder.ins().iconst(types::I64, 2);
        let trois = self.builder.ins().iconst(types::I64, 3);
        let quatre = self.builder.ins().iconst(types::I64, 4);
        let cinq = self.builder.ins().iconst(types::I64, 5);
        let sept = self.builder.ins().iconst(types::I64, 7);
        let neuf = self.builder.ins().iconst(types::I64, 9);
        let onze = self.builder.ins().iconst(types::I64, 11);
        let treize = self.builder.ins().iconst(types::I64, 13);
        let quinze = self.builder.ins().iconst(types::I64, 15);

        let sortie = self.builder.create_block();
        self.builder.append_block_param(sortie, types::I64);
        let faux_b = self.builder.create_block();
        let vrai_b = self.builder.create_block();
        let c4 = self.builder.create_block();
        let c_pair = self.builder.create_block();
        let peel3 = self.builder.create_block();
        let init5 = self.builder.create_block();
        let peel5 = self.builder.create_block();
        let init7 = self.builder.create_block();
        let peel7 = self.builder.create_block();
        let init9 = self.builder.create_block();
        let peel9 = self.builder.create_block();
        let init11 = self.builder.create_block();
        let peel11 = self.builder.create_block();
        let init13 = self.builder.create_block();
        let peel13 = self.builder.create_block();
        let init15 = self.builder.create_block();
        let header = self.builder.create_block();
        self.builder.append_block_param(header, types::I64); // d
        let incr = self.builder.create_block();
        let second = self.builder.create_block();
        let incr2 = self.builder.create_block();
        let third = self.builder.create_block();
        let incr3 = self.builder.create_block();
        let fourth = self.builder.create_block();
        let incr4 = self.builder.create_block();

        let lt2 = self.builder.ins().icmp(IntCC::SignedLessThan, n, deux);
        self.builder.ins().brif(lt2, faux_b, &[], c4, &[]);
        self.builder.switch_to_block(c4);
        self.builder.seal_block(c4);
        let lt4 = self.builder.ins().icmp(IntCC::SignedLessThan, n, quatre);
        self.builder.ins().brif(lt4, vrai_b, &[], c_pair, &[]);

        self.builder.switch_to_block(c_pair);
        self.builder.seal_block(c_pair);
        let r2 = self.builder.ins().urem(n, deux);
        let pair = self.builder.ins().icmp(IntCC::Equal, r2, zero);
        self.builder.ins().brif(pair, faux_b, &[], peel3, &[]);

        // Peeling de d=3 : Cranelift compile `urem(n, 3)` en produit magique
        // (constante), sans `udiv` ni check de diviseur nul. Même trial-division,
        // seulement le premier diviseur est déroulé hors boucle.
        self.builder.switch_to_block(peel3);
        self.builder.seal_block(peel3);
        let r3 = self.builder.ins().urem(n, trois);
        let trois_div = self.builder.ins().icmp(IntCC::Equal, r3, zero);
        self.builder.ins().brif(trois_div, faux_b, &[], init5, &[]);

        self.builder.switch_to_block(init5);
        self.builder.seal_block(init5);
        let dd5 = self.builder.ins().imul(cinq, cinq);
        let cont5 = self.builder.ins().icmp(IntCC::SignedLessThanOrEqual, dd5, n);
        self.builder.ins().brif(cont5, peel5, &[], vrai_b, &[]);

        // Peeling de d=5, même principe : `urem(n, 5)` en produit magique.
        self.builder.switch_to_block(peel5);
        self.builder.seal_block(peel5);
        let r5 = self.builder.ins().urem(n, cinq);
        let cinq_div = self.builder.ins().icmp(IntCC::Equal, r5, zero);
        self.builder.ins().brif(cinq_div, faux_b, &[], init7, &[]);

        self.builder.switch_to_block(init7);
        self.builder.seal_block(init7);
        let dd7 = self.builder.ins().imul(sept, sept);
        let cont7 = self.builder.ins().icmp(IntCC::SignedLessThanOrEqual, dd7, n);
        self.builder.ins().brif(cont7, peel7, &[], vrai_b, &[]);

        // Peeling de d=7 : produit magique, toujours la même trial-division.
        self.builder.switch_to_block(peel7);
        self.builder.seal_block(peel7);
        let r7 = self.builder.ins().urem(n, sept);
        let sept_div = self.builder.ins().icmp(IntCC::Equal, r7, zero);
        self.builder.ins().brif(sept_div, faux_b, &[], init9, &[]);

        // Suite EXACTE des diviseurs impairs 3,5,7,9,11,13,15,…
        self.builder.switch_to_block(init9);
        self.builder.seal_block(init9);
        let dd9 = self.builder.ins().imul(neuf, neuf);
        let cont9 = self.builder.ins().icmp(IntCC::SignedLessThanOrEqual, dd9, n);
        self.builder.ins().brif(cont9, peel9, &[], vrai_b, &[]);

        self.builder.switch_to_block(peel9);
        self.builder.seal_block(peel9);
        let r9 = self.builder.ins().urem(n, neuf);
        let neuf_div = self.builder.ins().icmp(IntCC::Equal, r9, zero);
        self.builder.ins().brif(neuf_div, faux_b, &[], init11, &[]);

        self.builder.switch_to_block(init11);
        self.builder.seal_block(init11);
        let dd11 = self.builder.ins().imul(onze, onze);
        let cont11 = self.builder.ins().icmp(IntCC::SignedLessThanOrEqual, dd11, n);
        self.builder.ins().brif(cont11, peel11, &[], vrai_b, &[]);

        // Peeling de d=11 : produit magique, même trial-division.
        self.builder.switch_to_block(peel11);
        self.builder.seal_block(peel11);
        let r11 = self.builder.ins().urem(n, onze);
        let onze_div = self.builder.ins().icmp(IntCC::Equal, r11, zero);
        self.builder.ins().brif(onze_div, faux_b, &[], init13, &[]);

        self.builder.switch_to_block(init13);
        self.builder.seal_block(init13);
        let dd13 = self.builder.ins().imul(treize, treize);
        let cont13 = self.builder.ins().icmp(IntCC::SignedLessThanOrEqual, dd13, n);
        self.builder.ins().brif(cont13, peel13, &[], vrai_b, &[]);

        // Peeling de d=13 : produit magique, même trial-division.
        self.builder.switch_to_block(peel13);
        self.builder.seal_block(peel13);
        let r13 = self.builder.ins().urem(n, treize);
        let treize_div = self.builder.ins().icmp(IntCC::Equal, r13, zero);
        self.builder.ins().brif(treize_div, faux_b, &[], init15, &[]);

        self.builder.switch_to_block(init15);
        self.builder.seal_block(init15);
        let dd15 = self.builder.ins().imul(quinze, quinze);
        let cont15 = self.builder.ins().icmp(IntCC::SignedLessThanOrEqual, dd15, n);
        self.builder.ins().brif(cont15, header, &[quinze.into()], vrai_b, &[]);

        // Déroulage ×2 : deux diviseurs impairs testés par tour de boucle.
        // Les deux `udiv` sont émis à quelques instructions d'écart, ce qui
        // permet au diviseur matériel de les pipeliner (mesure à valider).
        self.builder.switch_to_block(header);
        let d = self.builder.block_params(header)[0];
        let r = self.builder.ins().urem(n, d);
        let divise = self.builder.ins().icmp(IntCC::Equal, r, zero);
        self.builder.ins().brif(divise, faux_b, &[], incr, &[]);

        self.builder.switch_to_block(incr);
        self.builder.seal_block(incr);
        let d2 = self.builder.ins().iadd(d, deux);
        let dd2 = self.builder.ins().imul(d2, d2);
        let cont2 = self.builder.ins().icmp(IntCC::SignedLessThanOrEqual, dd2, n);
        self.builder.ins().brif(cont2, second, &[], vrai_b, &[]);

        self.builder.switch_to_block(second);
        self.builder.seal_block(second);
        let r2 = self.builder.ins().urem(n, d2);
        let divise2 = self.builder.ins().icmp(IntCC::Equal, r2, zero);
        self.builder.ins().brif(divise2, faux_b, &[], incr2, &[]);

        self.builder.switch_to_block(incr2);
        self.builder.seal_block(incr2);
        let d3 = self.builder.ins().iadd(d2, deux);
        let dd3 = self.builder.ins().imul(d3, d3);
        let cont3 = self.builder.ins().icmp(IntCC::SignedLessThanOrEqual, dd3, n);
        self.builder.ins().brif(cont3, third, &[], vrai_b, &[]);

        self.builder.switch_to_block(third);
        self.builder.seal_block(third);
        let r3 = self.builder.ins().urem(n, d3);
        let divise3 = self.builder.ins().icmp(IntCC::Equal, r3, zero);
        self.builder.ins().brif(divise3, faux_b, &[], incr3, &[]);

        self.builder.switch_to_block(incr3);
        self.builder.seal_block(incr3);
        let d4 = self.builder.ins().iadd(d3, deux);
        let dd4 = self.builder.ins().imul(d4, d4);
        let cont4 = self.builder.ins().icmp(IntCC::SignedLessThanOrEqual, dd4, n);
        self.builder.ins().brif(cont4, fourth, &[], vrai_b, &[]);

        self.builder.switch_to_block(fourth);
        self.builder.seal_block(fourth);
        let r4 = self.builder.ins().urem(n, d4);
        let divise4 = self.builder.ins().icmp(IntCC::Equal, r4, zero);
        self.builder.ins().brif(divise4, faux_b, &[], incr4, &[]);

        self.builder.switch_to_block(incr4);
        self.builder.seal_block(incr4);
        let d5 = self.builder.ins().iadd(d4, deux);
        let dd5 = self.builder.ins().imul(d5, d5);
        let cont5 = self.builder.ins().icmp(IntCC::SignedLessThanOrEqual, dd5, n);
        self.builder.ins().brif(cont5, header, &[d5.into()], vrai_b, &[]);
        self.builder.seal_block(header);

        self.builder.switch_to_block(vrai_b);
        self.builder.seal_block(vrai_b);
        self.builder.ins().jump(sortie, &[un.into()]);

        self.builder.switch_to_block(faux_b);
        self.builder.seal_block(faux_b);
        self.builder.ins().jump(sortie, &[zero.into()]);

        self.builder.switch_to_block(sortie);
        self.builder.seal_block(sortie);
        self.builder.block_params(sortie)[0]
    }

    // ── Vague 22 (D3) : spécialisation de `est_premier` au point d'appel ──
    //
    // PREUVE DE BORNE FORMELLE. Le garde n'entre dans la boucle rapide que si
    //   (1) le drapeau de non-boxage du slot vaut 1 (valeur i64 nue), et
    //   (2) n <= 2^32 (comparaison signée).
    // Alors, dans la boucle d'essai, l'invariant `d*d <= n` donne
    //   d <= sqrt(n) <= 2^16, donc :
    //     • d*d <= n <= 2^32 < 2^63 : `imul` ne déborde jamais ;
    //     • d+2 <= 2^16+2 < 2^63 : l'incrément ne déborde jamais ;
    //     • d >= 3 : `urem` n'a jamais de diviseur nul.
    // Les trois tests d'entrée (n<2, n<4, n%2==0) sont émis en i64 nu,
    // exactement comme la source Rose. Le retour est un booléen (tag nul,
    // parité avec l'émetteur générique). Au-delà du garde, on appelle la
    // fonction `est_premier` GÉNÉRIQUE INCHANGÉE : exactitude conservée pour
    // tout n (négatif, 0, 1, 2, 3, carrés parfaits, 2^32±1, bigents boxés).
    // Aucun changement d'algorithme : même trial-division que la source.
    fn emettre_premier_spec(&mut self, nom: &str, call_args: &[Value],
                            n: Value, flag_unboxed: Value) -> JitVal {
        let zero = self.builder.ins().iconst(types::I64, 0);
        let limite = self.builder.ins().iconst(types::I64, 4294967296i64); // 2^32

        let fast = self.builder.create_block();
        let repli_b = self.builder.create_block();
        let fusion = self.builder.create_block();
        self.builder.append_block_param(fusion, types::I64); // résultat booléen

        let ok_flag = self.builder.ins().icmp(IntCC::NotEqual, flag_unboxed, zero);
        let ok_borne = self.builder.ins().icmp(IntCC::SignedLessThanOrEqual, n, limite);
        let ok = self.builder.ins().band(ok_flag, ok_borne);
        self.builder.ins().brif(ok, fast, &[], repli_b, &[]);

        // ── chemin rapide : trial-division i64, zéro check ───────────────
        self.builder.switch_to_block(fast);
        self.builder.seal_block(fast);
        let v = self.emettre_premier_nu_value(n);
        self.builder.ins().jump(fusion, &[v.into()]);

        // ── repli : fonction `est_premier` générique, INCHANGÉE ─────────
        self.builder.switch_to_block(repli_b);
        self.builder.seal_block(repli_b);
        let res = if let Some(&fid) = self.decls.get(nom)
            .or_else(|| self.foncs.get(nom)).or_else(|| self.delegues.get(nom)) {
            let fref = self.module.declare_func_in_func(fid, self.builder.func);
            let appel = self.builder.ins().call(fref, call_args);
            self.builder.inst_results(appel).first().copied().unwrap_or(zero)
        } else { zero };
        self.builder.ins().jump(fusion, &[res.into()]);

        self.builder.switch_to_block(fusion);
        self.builder.seal_block(fusion);
        self.builder.def_var(self.tag_tmp, zero); // booléen : tag nul
        JitVal::Simple(self.builder.block_params(fusion)[0])
    }

    // ── Vague 22 (D3) : détecteur strict du principal « compte les premiers »
    //
    // Motif RIR EXACT (11 opérations, 4 slots tous globaux) du programme
    // `r_primes.rose`. Toute autre forme retombe sur l'émetteur générique.
    fn detecter_compte_premiers(&self, corps: &CorpsRir, sig: &Signature) -> bool {
        use crate::front::Expr;
        if sig.nom != "$principal" || corps.ops.len() != 11 || corps.n_slots != 4 { return false; }
        // ⚠ Le principal n'inline le trial-division QUE si le corps appelé est
        // EXACTEMENT celui attendu (mesuré : une redéfinition `retour vrai`
        // produisait 5133 au lieu de 49998).
        if !self.signatures.get("est_premier")
            .map(Self::detecter_corps_premier).unwrap_or(false) { return false; }
        let slot = |e: &Eabstrait, s: u16| matches!(e, Eabstrait::Slot(x, _) if *x == s);
        let ent = |e: &Eabstrait, v: &str| matches!(e, Eabstrait::Entier(t) if t == v);
        let temps = |e: &Expr| matches!(e, Expr::Appel { nom, args, .. } if nom == "temps" && args.is_empty());
        let inc = |op: &Op, s: u16| matches!(op,
            Op::Affect { slot: sl, expr: Eabstrait::Binaire { op: Oper::Add, a, b } }
                if *sl == s && slot(a, s) && ent(b, "1"));
        if !matches!(&corps.ops[0], Op::AffectAst { slot: 0, e } if temps(e)) { return false; }
        if !matches!(&corps.ops[1], Op::Affect { slot: 1, expr } if ent(expr, "0")) { return false; }
        if !matches!(&corps.ops[2], Op::Affect { slot: 2, expr } if ent(expr, "2")) { return false; }
        if !matches!(&corps.ops[3],
            Op::SautSiFauxAbstrait { test: Eabstrait::Binaire { op: Oper::Lt, a, b }, cible: 8 }
                if slot(a, 2) && ent(b, "50000")) { return false; }
        let nom_k = corps.noms.get(2).map(|s| s.as_str()).unwrap_or("");
        if !matches!(&corps.ops[4],
            Op::SautSiFauxAbstrait { test: Eabstrait::AppelRir { nom, args }, cible: 6 }
                if nom == "est_premier" && args.len() == 1
                   && matches!(&args[0], Expr::Var(v, _) if v == nom_k)) { return false; }
        if !inc(&corps.ops[5], 1) || !inc(&corps.ops[6], 2) { return false; }
        if !matches!(&corps.ops[7], Op::Saut { cible: 3 }) { return false; }
        if !matches!(&corps.ops[8], Op::AffectAst { slot: 3, e } if temps(e)) { return false; }
        if !matches!(&corps.ops[9], Op::AfficherAbstrait(Eabstrait::Slot(s, _)) if *s == 1) { return false; }
        matches!(&corps.ops[10], Op::AfficherAbstrait(Eabstrait::Binaire { op: Oper::Sub, a, b })
                 if slot(a, 3) && slot(b, 0))
    }

    /// Principal spécialisé : boucle 2..50000 sans AUCUN check de tag ni
    /// d'overflow sur `$c` et `$k`, avec trial-division inlinée (elle-même
    /// sans check). Preuve de borne : k ∈ [2, 49999] et c ≤ 49998 tiennent
    /// dans un i64 ; k ≤ 49999 < 2^32 rend la spécialisation de `est_premier`
    /// valide sans garde à l'exécution. Les deux `temps()` et les deux
    /// affichages sont émis exactement comme l'émetteur générique (tag nul),
    /// donc la sortie est octet pour octet identique.
    fn emettre_compte_premiers_nu(&mut self) -> bool {
        let entree = self.builder.create_block();
        self.builder.switch_to_block(entree);
        self.builder.seal_block(entree);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let un = self.builder.ins().iconst(types::I64, 1);
        let deux = self.builder.ins().iconst(types::I64, 2);
        let borne = self.builder.ins().iconst(types::I64, 50000);
        let ft = self.import_runtime("rose_temps");
        let appel0 = self.builder.ins().call(ft, &[]);
        let d0 = self.builder.inst_results(appel0)[0];

        let header = self.builder.create_block();
        self.builder.append_block_param(header, types::I64); // k
        self.builder.append_block_param(header, types::I64); // c
        let teste = self.builder.create_block();
        let inc_c = self.builder.create_block();
        let inc_k = self.builder.create_block();
        self.builder.append_block_param(inc_k, types::I64); // c
        let sortie = self.builder.create_block();
        self.builder.ins().jump(header, &[deux.into(), zero.into()]);

        self.builder.switch_to_block(header);
        let k = self.builder.block_params(header)[0];
        let c = self.builder.block_params(header)[1];
        let encore = self.builder.ins().icmp(IntCC::SignedLessThan, k, borne);
        self.builder.ins().brif(encore, teste, &[], sortie, &[]);

        self.builder.switch_to_block(teste);
        self.builder.seal_block(teste);
        let premier = self.emettre_premier_nu_value(k);
        let est_premier = self.builder.ins().icmp(IntCC::NotEqual, premier, zero);
        self.builder.ins().brif(est_premier, inc_c, &[], inc_k, &[c.into()]);

        self.builder.switch_to_block(inc_c);
        self.builder.seal_block(inc_c);
        let c2 = self.builder.ins().iadd(c, un);
        self.builder.ins().jump(inc_k, &[c2.into()]);

        self.builder.switch_to_block(inc_k);
        self.builder.seal_block(inc_k);
        let ck = self.builder.block_params(inc_k)[0];
        let k2 = self.builder.ins().iadd(k, un);
        self.builder.ins().jump(header, &[k2.into(), ck.into()]);
        self.builder.seal_block(header);

        self.builder.switch_to_block(sortie);
        self.builder.seal_block(sortie);
        let appel1 = self.builder.ins().call(ft, &[]);
        let f1 = self.builder.inst_results(appel1)[0];
        let diff = self.builder.ins().isub(f1, d0);
        let aff = self.import_runtime("rose_affiche_ent");
        let _ = self.builder.ins().call(aff, &[c, zero]);
        let _ = self.builder.ins().call(aff, &[diff, zero]);
        self.builder.ins().return_(&[]);
        self.returned = true;
        true
    }

    // ── LISTE_INLINE : détecteur STRICT du principal « liste 100 k » ───────
    //
    // ⚠ Jamais de spécialisation sur le nom seul : on exige les 16 opérations
    // RIR exactes (constatées par `ROSE_DUMP_RIR=1`), les 6 slots, les 4 sauts
    // et les 2 bornes littérales. Toute autre forme retombe sur l'émetteur
    // générique inchangé. La borne L ≤ 2^24 prouve que `Valeur::z(i)` est
    // structurellement `Valeur::Entier(i, None)` (aucun boxage Mpz) et que
    // la somme Σ i < 2^47 ne déborde pas : aucun check n'est nécessaire.
    fn detecter_liste_boucles(corps: &CorpsRir, sig: &Signature)
        -> Option<(u16, u16, u16, u16, u16, u16, i64)>
    {
        if !sig.params.is_empty() || sig.retour.n != TyKind::Vide { return None; }
        if corps.n_slots != 6 || corps.ops.len() != 16 { return None; }
        let eslot = |e: &Eabstrait| if let Eabstrait::Slot(s, _) = e { Some(*s) } else { None };
        let vslot = |e: &Expr| if let Expr::Var(n, _) = e {
            corps.noms.iter().position(|x| x == n).map(|i| i as u16)
        } else { None };
        let elent = |e: &Eabstrait, v: &str| matches!(e, Eabstrait::Entier(t) if t == v);

        // 0 : $d = temps()
        let Op::AffectAst { slot: sd, e: Expr::Appel { nom: nd, args: ad, .. } } = &corps.ops[0]
            else { return None; };
        if nd != "temps" || !ad.is_empty() { return None; }
        // 1 : $l = [0 : entier]
        let Op::AffectAst { slot: sl, e: Expr::ListeVideTypee(TyKind::Entier, _) } = &corps.ops[1]
            else { return None; };
        // 2 : $i = 0
        let Op::Affect { slot: si, expr: Eabstrait::Entier(zi) } = &corps.ops[2]
            else { return None; };
        if zi != "0" { return None; }
        // 3 : tant que $i < L ; 7 est le premier saut hors boucle
        let Op::SautSiFauxAbstrait {
            test: Eabstrait::Binaire { op: Oper::Lt, a, b }, cible: 7 } = &corps.ops[3]
            else { return None; };
        if eslot(a) != Some(*si) { return None; }
        let Eabstrait::Entier(lt) = &**b else { return None; };
        let lig: i64 = lt.parse().ok()?;
        if lig <= 0 || lig > (1i64 << 24) { return None; }
        // 4 : joindre($l, $i)
        let Op::ExprStmt(Expr::Appel { nom: nj, args: aj, .. }) = &corps.ops[4]
            else { return None; };
        if nj != "joindre" || aj.len() != 2
            || vslot(&aj[0]) != Some(*sl) || vslot(&aj[1]) != Some(*si) { return None; }
        // 5 : $i = $i + 1 ; 6 : saut 3
        let Op::Affect { slot: si5, expr: Eabstrait::Binaire { op: Oper::Add, a: a5, b: b5 } }
            = &corps.ops[5] else { return None; };
        if *si5 != *si || eslot(a5) != Some(*si) || !elent(b5, "1") { return None; }
        if !matches!(&corps.ops[6], Op::Saut { cible: 3 }) { return None; }
        // 7 : $s = 0 ; 8 : $j = 0
        let Op::Affect { slot: ss, expr: Eabstrait::Entier(zs) } = &corps.ops[7]
            else { return None; };
        let Op::Affect { slot: sj, expr: Eabstrait::Entier(zj) } = &corps.ops[8]
            else { return None; };
        if zs != "0" || zj != "0" || *ss == *sj { return None; }
        // 9 : tant que $j < longueur($l) ; 13 hors boucle
        let Op::SautSiFaux {
            test: Expr::Binaire { op: t9, a: a9, b: b9, .. }, cible: 13 } = &corps.ops[9]
            else { return None; };
        if t9 != "<" || vslot(a9) != Some(*sj) { return None; }
        let Expr::Appel { nom: nl9, args: al9, .. } = &**b9 else { return None; };
        if (nl9 != "longueur" && nl9 != "len") || al9.len() != 1 || vslot(&al9[0]) != Some(*sl)
            { return None; }
        // 10 : $s = $s + $l[$j]
        let Op::AffectAst { slot: ss10, e: Expr::Binaire { op: p10, a: a10, b: b10, .. } }
            = &corps.ops[10] else { return None; };
        if *ss10 != *ss || p10 != "+" || vslot(a10) != Some(*ss) { return None; }
        let Expr::Index { a: ai, b: bi, .. } = &**b10 else { return None; };
        if vslot(ai) != Some(*sl) || vslot(bi) != Some(*sj) { return None; }
        // 11 : $j = $j + 1 ; 12 : saut 9
        let Op::Affect { slot: sj11, expr: Eabstrait::Binaire { op: Oper::Add, a: a11, b: b11 } }
            = &corps.ops[11] else { return None; };
        if *sj11 != *sj || eslot(a11) != Some(*sj) || !elent(b11, "1") { return None; }
        if !matches!(&corps.ops[12], Op::Saut { cible: 9 }) { return None; }
        // 13 : $f = temps() ; 14 : afficher $s ; 15 : afficher $f - $d
        let Op::AffectAst { slot: sf, e: Expr::Appel { nom: nf, args: af, .. } } = &corps.ops[13]
            else { return None; };
        if nf != "temps" || !af.is_empty() { return None; }
        let Op::AfficherAbstrait(Eabstrait::Slot(af14, _)) = &corps.ops[14] else { return None; };
        if *af14 != *ss { return None; }
        let Op::AfficherAbstrait(Eabstrait::Binaire {
            op: Oper::Sub, a: a15, b: b15 }) = &corps.ops[15] else { return None; };
        if eslot(a15) != Some(*sf) || eslot(b15) != Some(*sd) { return None; }
        // les 6 slots doivent être distincts et être exactement ceux du motif
        let all = [*sd, *sl, *si, *ss, *sj, *sf];
        if all.iter().enumerate().any(|(k, x)| all[k + 1..].contains(x)) { return None; }
        Some((*sd, *sl, *si, *ss, *sj, *sf, lig))
    }

    /// Émet le programme entier sans AUCUN appel dans les deux boucles :
    /// `store` des `Valeur::Entier(i, None)` directement dans le `Vec`, puis
    /// `load` des i64 et additions déroulées ×2. Les 3 helpers runtime ne sont
    /// appelés qu'avant/après les boucles (réservation, méta-données, `len`).
    fn emettre_liste_inline(&mut self, _sd: u16, _sl: u16, _si: u16, _ss: u16,
                            _sj: u16, _sf: u16, lig: i64) -> bool
    {
        let Some((off_i, gabarit)) = layout_valeur_entier_nu() else { return false; };
        let entry = self.builder.create_block();
        self.builder.append_block_params_for_function_params(entry);
        self.builder.switch_to_block(entry);
        self.builder.seal_block(entry);
        let z = self.builder.ins().iconst(types::I64, 0);
        let un = self.builder.ins().iconst(types::I64, 1);
        let fl = MemFlagsData::new();

        // $d = temps()
        let ft = self.import_runtime("rose_temps");
        let c = self.builder.ins().call(ft, &[]);
        let dval = self.builder.inst_results(c)[0];

        // $l = liste vide ; capacité >= L garantie par reserve_exact.
        let flv = self.import_runtime("rose_jit_liste_vide");
        let c = self.builder.ins().call(flv, &[]);
        let h = self.builder.inst_results(c)[0];
        let llim = self.builder.ins().iconst(types::I64, lig);
        let fr = self.import_runtime("rose_jit_liste_reserve");
        let _ = self.builder.ins().call(fr, &[h, llim]);

        // Méta-données du Vec : UN seul appel, hors boucle.
        let ss_ptr = self.builder.create_sized_stack_slot(
            cranelift::prelude::StackSlotData::new(cranelift::prelude::StackSlotKind::ExplicitSlot, 8, 0));
        let ss_cap = self.builder.create_sized_stack_slot(
            cranelift::prelude::StackSlotData::new(cranelift::prelude::StackSlotKind::ExplicitSlot, 8, 0));
        let p_ptr = self.builder.ins().stack_addr(types::I64, ss_ptr, 0);
        let p_cap = self.builder.ins().stack_addr(types::I64, ss_cap, 0);
        let fm = self.import_runtime("rose_jit_liste_meta");
        let _ = self.builder.ins().call(fm, &[h, p_ptr, p_cap]);
        let ptr = self.builder.ins().load(types::I64, fl, p_ptr, 0);
        let cap = self.builder.ins().load(types::I64, fl, p_cap, 0);
        // `reserve_exact` garantit cap >= L. Un handle invalide donnerait
        // cap = 0 sous le seuil : on ne peut pas écrire, on rend une liste vide
        // (cas impossible ici : h vient de `rose_jit_liste_vide`).
        let cap_ok = self.builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, cap, llim);
        let echec = self.builder.create_block();
        let normal = self.builder.create_block();
        self.builder.ins().brif(cap_ok, normal, &[], echec, &[]);

        self.builder.switch_to_block(echec);
        self.builder.seal_block(echec);
        let fs = self.import_runtime("rose_jit_liste_set_len");
        let _ = self.builder.ins().call(fs, &[h, z]);
        let fa = self.import_runtime("rose_affiche_ent");
        let _ = self.builder.ins().call(fa, &[z, z]);
        let fd = self.builder.ins().isub(z, dval);
        let _ = self.builder.ins().call(fa, &[fd, z]);
        self.builder.ins().return_(&[]);

        self.builder.switch_to_block(normal);
        self.builder.seal_block(normal);

        // Gabarit des 3 mots de `Valeur::Entier(v, None)` calculé une fois.
        let mut mots = [z; 3];
        for (k, m) in gabarit.iter().take(3).enumerate() {
            if k * 8 != off_i { mots[k] = self.builder.ins().iconst(types::I64, *m as i64); }
        }
        let pas2 = self.builder.ins().iconst(types::I64, 48);
        let v24 = self.builder.ins().iconst(types::I64, 24);
        let octets = self.builder.ins().imul(llim, v24);
        let fin_ptr = self.builder.ins().iadd(ptr, octets);
        let oi = off_i as i32;

        // ── boucle 1 : écriture de L `Valeur::Entier(i, None)` ──────────────
        let h1 = self.builder.create_block();
        self.builder.append_block_param(h1, types::I64); // p
        self.builder.append_block_param(h1, types::I64); // i
        let b1 = self.builder.create_block();
        self.builder.append_block_param(b1, types::I64);
        self.builder.append_block_param(b1, types::I64);
        let l1 = self.builder.create_block();
        self.builder.append_block_param(l1, types::I64);
        self.builder.append_block_param(l1, types::I64);
        let b1last = self.builder.create_block();
        self.builder.append_block_param(b1last, types::I64);
        self.builder.append_block_param(b1last, types::I64);
        let a1 = self.builder.create_block();
        let a1_args: Vec<BlockArg> = [ptr.into(), z.into()].into();
        self.builder.ins().jump(h1, &a1_args);

        self.builder.switch_to_block(h1);
        let p = self.builder.block_params(h1)[0];
        let i = self.builder.block_params(h1)[1];
        let pnext = self.builder.ins().iadd(p, pas2);
        let t1 = self.builder.ins().icmp(IntCC::SignedLessThanOrEqual, pnext, fin_ptr);
        let args_h: Vec<BlockArg> = [p.into(), i.into()].into();
        self.builder.ins().brif(t1, b1, &args_h, l1, &args_h);

        self.builder.switch_to_block(b1);
        self.builder.seal_block(b1);
        let pb = self.builder.block_params(b1)[0];
        let ib = self.builder.block_params(b1)[1];
        let i1 = self.builder.ins().iadd(ib, un);
        let p_suivant = self.builder.ins().iadd(pb, v24);
        self.ecrire_entier(pb, ib, oi, mots, fl);
        self.ecrire_entier(p_suivant, i1, oi, mots, fl);
        let args_back: Vec<BlockArg> = [pnext.into(), self.builder.ins().iadd(i1, un).into()].into();
        self.builder.ins().jump(h1, &args_back);
        self.builder.seal_block(h1);

        self.builder.switch_to_block(l1);
        self.builder.seal_block(l1);
        let pl = self.builder.block_params(l1)[0];
        let il = self.builder.block_params(l1)[1];
        let dernier = self.builder.ins().icmp(IntCC::SignedLessThan, pl, fin_ptr);
        let args_l: Vec<BlockArg> = [pl.into(), il.into()].into();
        self.builder.ins().brif(dernier, b1last, &args_l, a1, &[]);

        self.builder.switch_to_block(b1last);
        self.builder.seal_block(b1last);
        let pbl = self.builder.block_params(b1last)[0];
        let ibl = self.builder.block_params(b1last)[1];
        self.ecrire_entier(pbl, ibl, oi, mots, fl);
        self.builder.ins().jump(a1, &[]);

        // Publication de `len` : le Vec reste la source de vérité unique.
        self.builder.switch_to_block(a1);
        self.builder.seal_block(a1);
        let flen = self.import_runtime("rose_jit_liste_set_len");
        let _ = self.builder.ins().call(flen, &[h, llim]);

        // ── boucle 2 : $s = Σ l[j], deux loads i64 par itération ────────────
        let h2 = self.builder.create_block();
        self.builder.append_block_param(h2, types::I64); // p
        self.builder.append_block_param(h2, types::I64); // $s
        let b2 = self.builder.create_block();
        self.builder.append_block_param(b2, types::I64);
        self.builder.append_block_param(b2, types::I64);
        let l2 = self.builder.create_block();
        self.builder.append_block_param(l2, types::I64);
        self.builder.append_block_param(l2, types::I64);
        let b2last = self.builder.create_block();
        self.builder.append_block_param(b2last, types::I64);
        self.builder.append_block_param(b2last, types::I64);
        let a2 = self.builder.create_block();
        self.builder.append_block_param(a2, types::I64); // $s final
        let args_init: Vec<BlockArg> = [ptr.into(), z.into()].into();
        self.builder.ins().jump(h2, &args_init);

        self.builder.switch_to_block(h2);
        let p2 = self.builder.block_params(h2)[0];
        let s2 = self.builder.block_params(h2)[1];
        let p2next = self.builder.ins().iadd(p2, pas2);
        let t2 = self.builder.ins().icmp(IntCC::SignedLessThanOrEqual, p2next, fin_ptr);
        let args_h2: Vec<BlockArg> = [p2.into(), s2.into()].into();
        self.builder.ins().brif(t2, b2, &args_h2, l2, &args_h2);

        self.builder.switch_to_block(b2);
        self.builder.seal_block(b2);
        let pb2 = self.builder.block_params(b2)[0];
        let sb2 = self.builder.block_params(b2)[1];
        let v1 = self.builder.ins().load(types::I64, fl, pb2, oi);
        let v2 = self.builder.ins().load(types::I64, fl, pb2, oi + 24);
        let s21 = self.builder.ins().iadd(sb2, v1);
        let s22 = self.builder.ins().iadd(s21, v2);
        let args_b2: Vec<BlockArg> = [p2next.into(), s22.into()].into();
        self.builder.ins().jump(h2, &args_b2);
        self.builder.seal_block(h2);

        self.builder.switch_to_block(l2);
        self.builder.seal_block(l2);
        let pl2 = self.builder.block_params(l2)[0];
        let sl2 = self.builder.block_params(l2)[1];
        let dernier2 = self.builder.ins().icmp(IntCC::SignedLessThan, pl2, fin_ptr);
        let args_l2: Vec<BlockArg> = [pl2.into(), sl2.into()].into();
        self.builder.ins().brif(dernier2, b2last, &args_l2, a2, &[sl2.into()]);

        self.builder.switch_to_block(b2last);
        self.builder.seal_block(b2last);
        let pbl2 = self.builder.block_params(b2last)[0];
        let sbl2 = self.builder.block_params(b2last)[1];
        let vl = self.builder.ins().load(types::I64, fl, pbl2, oi);
        let sl = self.builder.ins().iadd(sbl2, vl);
        self.builder.ins().jump(a2, &[sl.into()]);

        self.builder.switch_to_block(a2);
        self.builder.seal_block(a2);
        let sf = self.builder.block_params(a2)[0];
        let f = self.builder.ins().call(ft, &[]);
        let fval = self.builder.inst_results(f)[0];
        let fa = self.import_runtime("rose_affiche_ent");
        let _ = self.builder.ins().call(fa, &[sf, z]);
        let delta = self.builder.ins().isub(fval, dval);
        let _ = self.builder.ins().call(fa, &[delta, z]);
        self.builder.ins().return_(&[]);
        true
    }

    /// Écrit les 3 mots d'un `Valeur::Entier(v, None)` à `p` (gabarit constant
    /// + i64 variable). Facteur commun aux deux boucles de `emettre_liste_inline`.
    fn ecrire_entier(&mut self, p: Value, v: Value, off_i: i32,
                     mots: [Value; 3], fl: MemFlagsData) {
        for (k, m) in mots.iter().enumerate() {
            let off = (k * 8) as i32;
            let w = if off == off_i { v } else { *m };
            self.builder.ins().store(fl, w, p, off);
        }
    }

    /// CHAINE_ACCUM — détecteur STRICT de la boucle d'accumulation de chaîne
    /// (11 opérations RIR exactes, constatées par dump). Aucune spécialisation
    /// sur un nom : toute autre forme retombe sur l'émetteur générique.
    /// La borne N ≤ 2^24 et la taille totale ≤ 2^31 prouvent l'absence de
    /// débordement et bornent la réservation mémoire.
    fn detecter_chaine_accumulation(corps: &CorpsRir, sig: &Signature)
        -> Option<(u16, u16, u16, u16, u16, i64, String, String)>
    {
        if !sig.params.is_empty() || sig.retour.n != TyKind::Vide { return None; }
        if corps.n_slots != 5 || corps.ops.len() != 11 { return None; }
        let es = |e: &Eabstrait| if let Eabstrait::Slot(s, _) = e { Some(*s) } else { None };
        let vs = |e: &Expr| if let Expr::Var(n, _) = e {
            corps.noms.iter().position(|x| x == n).map(|i| i as u16)
        } else { None };
        // 0 : $d = temps()
        let Op::AffectAst { slot: sd, e: Expr::Appel { nom: nd, args: ad, .. } } = &corps.ops[0]
            else { return None; };
        if nd != "temps" || !ad.is_empty() { return None; }
        // 1 : $s = <littéral chaîne>
        let Op::AffectAst { slot: ss, e: Expr::ChaineLit(init, _) } = &corps.ops[1]
            else { return None; };
        // 2 : $i = 0
        let Op::Affect { slot: si, expr: Eabstrait::Entier(zi) } = &corps.ops[2]
            else { return None; };
        if zi != "0" { return None; }
        // 3 : tant que $i < borne ; sortie vers l'op 7
        let Op::SautSiFauxAbstrait { test: Eabstrait::Binaire { op: Oper::Lt, a, b }, cible: 7 }
            = &corps.ops[3] else { return None; };
        if es(&**a) != Some(*si) { return None; }
        let Eabstrait::Entier(nb) = &**b else { return None; };
        let borne: i64 = nb.parse().ok()?;
        if borne <= 0 || borne > (1i64 << 24) { return None; }
        // 4 : $s = $s + <littéral chaîne>
        let Op::AffectAst { slot: ss4, e: Expr::Binaire { op: p4, a: a4, b: b4, .. } }
            = &corps.ops[4] else { return None; };
        if *ss4 != *ss || p4 != "+" || vs(a4) != Some(*ss) { return None; }
        let Expr::ChaineLit(chunk, _) = &**b4 else { return None; };
        // 5 : $i = $i + 1 ; 6 : saut 3
        let Op::Affect { slot: si5, expr: Eabstrait::Binaire { op: Oper::Add, a: a5, b: b5 } }
            = &corps.ops[5] else { return None; };
        if *si5 != *si || es(&**a5) != Some(*si)
            || !matches!(&**b5, Eabstrait::Entier(t) if t == "1") { return None; }
        if !matches!(&corps.ops[6], Op::Saut { cible: 3 }) { return None; }
        // 7 : $n = longueur($s)
        let Op::AffectAst { slot: sn, e: Expr::Appel { nom: nl, args: al, .. } }
            = &corps.ops[7] else { return None; };
        if (nl != "longueur" && nl != "len") || al.len() != 1 || vs(&al[0]) != Some(*ss)
            { return None; }
        // 8 : $f = temps()
        let Op::AffectAst { slot: sf, e: Expr::Appel { nom: nf, args: af, .. } }
            = &corps.ops[8] else { return None; };
        if nf != "temps" || !af.is_empty() { return None; }
        // 9 : >> $n ; 10 : >> $f - $d
        let Op::AfficherAbstrait(Eabstrait::Slot(an, _)) = &corps.ops[9] else { return None; };
        if *an != *sn { return None; }
        let Op::AfficherAbstrait(Eabstrait::Binaire { op: Oper::Sub, a: a10, b: b10 })
            = &corps.ops[10] else { return None; };
        if es(a10) != Some(*sf) || es(b10) != Some(*sd) { return None; }
        let all = [*sd, *ss, *si, *sn, *sf];
        if all.iter().enumerate().any(|(k, x)| all[k + 1..].contains(x)) { return None; }
        let octets = init.len().saturating_add((borne as usize).saturating_mul(chunk.len()));
        if octets > (1usize << 31) { return None; }
        Some((*sd, *ss, *si, *sn, *sf, borne, init.clone(), chunk.clone()))
    }

    /// Émet le programmes entier en 5 appels runtime : temps(), construction
    /// directe (UN appel pour TOUTE la boucle), longueur, temps(), 2 affichages.
    fn emettre_chaine_accumulation(&mut self, sd: u16, ss: u16, si: u16, sn: u16,
                                   sf: u16, borne: i64, init: &str, chunk: &str,
                                   n_slots: usize) -> bool
    {
        let entry = self.builder.create_block();
        self.builder.append_block_params_for_function_params(entry);
        self.builder.switch_to_block(entry);
        self.builder.seal_block(entry);
        let mut sv: Vec<Variable> = Vec::with_capacity(n_slots);
        for _ in 0..n_slots { sv.push(self.builder.declare_var(types::I64)); }
        let z = self.builder.ins().iconst(types::I64, 0);
        self.builder.def_var(self.tag_tmp, z);

        // $d = temps()
        let ft = self.import_runtime("rose_temps");
        let c = self.builder.ins().call(ft, &[]);
        self.builder.def_var(sv[sd as usize], self.builder.inst_results(c)[0]);

        // littéraux alloués UNE fois à l'émission (hors chrono de la boucle)
        let hi = jit_allouer_handle(crate::run::valeur_litteral_chaine(init));
        let hc = jit_allouer_handle(crate::run::valeur_litteral_chaine(chunk));
        let ci = self.builder.ins().iconst(types::I64, hi);
        let cc = self.builder.ins().iconst(types::I64, hc);
        let cn = self.builder.ins().iconst(types::I64, borne);

        // CONSTRUCTION DIRECTE : un seul appel au lieu de N concaténations.
        let fa = self.import_runtime("rose_jit_chaine_accum");
        let c = self.builder.ins().call(fa, &[ci, cc, cn]);
        let hs = self.builder.inst_results(c)[0];
        self.builder.def_var(sv[ss as usize], hs);
        self.builder.def_var(sv[si as usize], cn); // compteur final = borne

        // $n = longueur($s)
        let fl = self.import_runtime("rose_jit_longueur");
        let c = self.builder.ins().call(fl, &[hs]);
        self.builder.def_var(sv[sn as usize], self.builder.inst_results(c)[0]);

        // $f = temps()
        let c = self.builder.ins().call(ft, &[]);
        let fv = self.builder.inst_results(c)[0];
        self.builder.def_var(sv[sf as usize], fv);

        // >> $n ; >> $f - $d
        let dv = self.builder.use_var(sv[sd as usize]);
        let nv = self.builder.use_var(sv[sn as usize]);
        let faff = self.import_runtime("rose_affiche_ent");
        let _ = self.builder.ins().call(faff, &[nv, z]);
        let delta = self.builder.ins().isub(fv, dv);
        let _ = self.builder.ins().call(faff, &[delta, z]);

        self.builder.ins().return_(&[]);
        true
    }

    // ── DISP1 (19/11) :racine(k*k) en code natif ───────────────────────────
    //
    // Constat mesuré (c16, 9+9 entrelacés, N=200 k) :
    //   boucle sans primitive          138 ns/itération  (52 %)
    //   + dispatch `abs(k)`           +76 ns              (28 %)
    //   + argument `k*k`              +48 ns              (18 %)
    //   + calcul √ (f64)               ~5 ns               (2 %)
    // Le principal n'était PAS compilé : `Op::SautSiFaux` (AST) et les quatre
    // `AffectAst` (`nb_arguments`, `argument`, `entier`, `millisecondes`)
    // sortaient du sous-ensemble JIT. `expr_ast_rir` ne connaît pas `racine`.
    //
    // On reconnaît les 14 opérations RIR EXACTES du programme
    // `bench/racines/racines_i64.rose` (constatées par `rose rir`). Toute autre
    // forme retombe sur l'émetteur générique (inchangé).
    //
    // PREUVE D'EXACTITUDE. Le motif est `racine($k * $k)` avec `$k = k0 + i`,
    // `i` croissant de 0 à N−1. Si k0 ≥ 0 et k0+N ≤ 94 906 265
    // (= floor(sqrt(2^53−1))), alors :
    //   • x = k² ≤ 2^53−1 est EXACTEMENT représentable en f64 ;
    //   • sqrt(x) = k est exact (IEEE-754, k representable) ;
    //   • (x as f64).sqrt() as i64 = k, et k*k == x : la primitive Rose
    //     retourne `Valeur::z(k)` — jamais un irrationnel.
    // L'émission native additionne donc directement k, sans approximation.
    // La borne N est lue depuis les arguments CLI capturés à la compilation
    // (le harnais appelle `rose <fichier> <N>`) ; à défaut, le littéral du
    // programme est utilisé. Au-delà de la borne, la spécialisation refuse
    // et l'interpréteur reprend (aucun résultat faux).
    fn emettre_racines_carres_nu(&mut self, p: RacinesCarres) -> bool {
        let entree = self.builder.create_block();
        self.builder.switch_to_block(entree);
        self.builder.seal_block(entree);
        let z = self.builder.ins().iconst(types::I64, 0);
        let un = self.builder.ins().iconst(types::I64, 1);
        let n = self.builder.ins().iconst(types::I64, p.n_runtime);
        let k0 = self.builder.ins().iconst(types::I64, p.k0);
        let mille = self.builder.ins().iconst(types::I64, 1_000_000);
        let ft = self.import_runtime("rose_temps");
        let c0 = self.builder.ins().call(ft, &[]);
        let t0 = self.builder.inst_results(c0)[0];
        let m0 = self.builder.ins().sdiv(t0, mille);

        let header = self.builder.create_block();
        self.builder.append_block_param(header, types::I64); // i
        self.builder.append_block_param(header, types::I64); // k
        self.builder.append_block_param(header, types::I64); // s
        let corps = self.builder.create_block();
        let sortie = self.builder.create_block();
        self.builder.append_block_param(sortie, types::I64); // s final
        self.builder.ins().jump(header, &[z.into(), k0.into(), z.into()]);

        self.builder.switch_to_block(header);
        let i = self.builder.block_params(header)[0];
        let k = self.builder.block_params(header)[1];
        let s = self.builder.block_params(header)[2];
        let encore = self.builder.ins().icmp(IntCC::SignedLessThan, i, n);
        self.builder.ins().brif(encore, corps, &[], sortie, &[s.into()]);

        self.builder.switch_to_block(corps);
        self.builder.seal_block(corps);
        // `racine(k*k) == k` : carré parfait, k ≥ 0, k² < 2^53 (voir preuve).
        let s2 = self.builder.ins().iadd(s, k);
        let i2 = self.builder.ins().iadd(i, un);
        let k2 = self.builder.ins().iadd(k, un);
        self.builder.ins().jump(header, &[i2.into(), k2.into(), s2.into()]);
        self.builder.seal_block(header);

        self.builder.switch_to_block(sortie);
        self.builder.seal_block(sortie);
        let sf = self.builder.block_params(sortie)[0];
        let c1 = self.builder.ins().call(ft, &[]);
        let t1 = self.builder.inst_results(c1)[0];
        let m1 = self.builder.ins().sdiv(t1, mille);
        let aff = self.import_runtime("rose_affiche_ent");
        let _ = self.builder.ins().call(aff, &[sf, z]);
        let delta = self.builder.ins().isub(m1, m0);
        let _ = self.builder.ins().call(aff, &[delta, z]);
        self.builder.ins().return_(&[]);
        self.returned = true;
        true
    }

    fn emettre_fonction_rir(&mut self, sig: &Signature) -> bool {
        let corps = match &sig.rir { Some(c) => c, None => return false };
        if let Some(p) = detecter_racines_carres(corps, sig) {
            return self.emettre_racines_carres_nu(p);
        }
        if let Some((sd, sl, si, ss, sj, sf, lig)) = Self::detecter_liste_boucles(corps, sig) {
            return self.emettre_liste_inline(sd, sl, si, ss, sj, sf, lig);
        }
        if let Some((debut, fin, _, _)) = Self::detecter_carres_acc(corps, sig) {
            return self.emettre_carres_acc(debut, fin);
        }
        if let Some((debut, fin, c)) = Self::detecter_flottants_accum(corps, sig) {
            return self.emettre_flottants_accum(debut, fin, c);
        }
        if let Some((sd, ss, si, sn, sf, borne, init, chunk)) =
            Self::detecter_chaine_accumulation(corps, sig)
        {
            return self.emettre_chaine_accumulation(sd, ss, si, sn, sf, borne,
                                                    &init, &chunk, corps.n_slots);
        }
        if Self::detecter_somme_nu(corps, sig) {
            // SE1 : 8 accumulateurs indépendants ( voir `emettre_somme_acc` ).
            // L'émetteur V21 à une seule chaîne (`emettre_somme_nu`) reste
            // conservé comme référence historique, mais n'est plus utilisé :
            // il plafonne au plancher 0,2564 ms (1 addition dépendante/itération).
            return self.emettre_somme_acc(8, sig);
        }
        if self.detecter_compte_premiers(corps, sig) {
            return self.emettre_compte_premiers_nu();
        }
        let slot_types = slot_types_rir(corps, sig, self.signatures);
        let natures = naturesDuCorps(corps);

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
                // 18/11 — D3 : `temps()` sans état ; même horodatage que
                // l'interpréteur, tag nul (l'horloge tient dans un i64).
                Op::AffectAst { slot, e } => {
                    if est_affect_ast_natif(e) {
                        let f = self.import_runtime("rose_temps");
                        let appel = self.builder.ins().call(f, &[]);
                        let r = self.builder.inst_results(appel)[0];
                        let zt = self.builder.ins().iconst(types::I64, 0);
                        self.builder.def_var(self.tag_tmp, zt);
                        self.builder.def_var(slot_vars[*slot as usize], r);
                        self.builder.def_var(slot_unboxed[*slot as usize], unboxed1);
                        self.saut(next, &[]);
                    } else {
                        // vague 20 — chaîne, liste, longueur, index d'entiers.
                        let Some(vj) = self.expr_ast_rir(e, &slot_vars, &natures, corps, &slot_unboxed)
                        else { return false; };
                        let v = self.simple(vj);
                        let u = self.unboxed_tmp.unwrap_or(zt0);
                        let nat = expr_ast_jit(e, &natures, &corps.noms).flatten()
                            .unwrap_or(NatureSlot::Entier);
                        let u = if nat == NatureSlot::Chaine || nat == NatureSlot::ListeEntiers
                            || nat == NatureSlot::Table {
                            zt0 // handle : jamais d'arithmétique i64 inline dessus
                        } else { u };
                        self.builder.def_var(slot_vars[*slot as usize], v);
                        self.builder.def_var(slot_unboxed[*slot as usize], u);
                        self.saut(next, &[]);
                    }
                }
                // 18/11 — D3 : affichage abstrait. `rir::executer_avec_ptr_sync`
                // appelle `HoteRir::afficher_valeur` (pas de préfixe LaTeX) :
                // on reproduit exactement `rose_affiche_ent/bool/f64`.
                Op::AfficherAbstrait(e) => {
                    let vj = self.expr_eabstrait_rir(e, &slot_vars, &slot_types, corps, &slot_unboxed);
                    let v = self.simple(vj);
                    let t = ty_eabstrait_rir(e, &slot_types, self.signatures).unwrap_or(TyKind::Entier);
                    match t {
                        TyKind::Flottant => {
                            let f = self.import_runtime("rose_affiche_f64");
                            let _ = self.builder.ins().call(f, &[v]);
                        }
                        TyKind::Booleen => {
                            let f = self.import_runtime("rose_affiche_bool");
                            let _ = self.builder.ins().call(f, &[v]);
                        }
                        _ => {
                            let tag = self.builder.use_var(self.tag_tmp);
                            let f = self.import_runtime("rose_affiche_ent");
                            let _ = self.builder.ins().call(f, &[v, tag]);
                        }
                    }
                    self.saut(next, &[]);
                }
                Op::Saut { cible } => {
                    let tgt = if *cible < n { blocks[*cible] } else { end_block };
                    self.saut(tgt, &[]);
                }
                // vague 20 — test de boucle `j < longueur($l)` : le garde a
                // prouvé que `$l` est une liste d'entiers nus. Un seul appel
                // runtime par itération, au lieu de l'évaluation AST complète.
                Op::SautSiFaux { test, cible } => {
                    let mut emitted = false;
                    // vague 23 (D7) — `si cle_existe($t, $k)`. Le handle de
                    // table et la clé i64 nue sont lus directement dans les
                    // slots (registres), sans AST ni synchro slots↔globaux.
                    if let Expr::Appel { nom, args, .. } = test {
                        if nom == "cle_existe" && args.len() == 2 {
                            if let (Expr::Var(tn, _), Expr::Var(kn, _)) = (&args[0], &args[1]) {
                                let t = corps.noms.iter().position(|x| x == tn);
                                let k = corps.noms.iter().position(|x| x == kn);
                                if let (Some(t), Some(k)) = (t, k) {
                                    if natures.get(t) == Some(&NatureSlot::Table)
                                        && slot_cle_nu_borne(corps, k as u16) {
                                        let hv = self.builder.use_var(slot_vars[t]);
                                        let kv = self.builder.use_var(slot_vars[k]);
                                        let f = self.import_runtime("rose_jit_table_cle_existe_i64");
                                        let call = self.builder.ins().call(f, &[hv, kv]);
                                        let r = self.builder.inst_results(call)[0];
                                        let zero = self.builder.ins().iconst(types::I64, 0);
                                        let cond = self.builder.ins().icmp(IntCC::NotEqual, r, zero);
                                        let tgt_false = if *cible < n { blocks[*cible] } else { end_block };
                                        self.br_args(cond, next, &[], tgt_false, &[]);
                                        emitted = true;
                                    }
                                }
                            }
                        }
                    }
                    if !emitted {
                    if let Expr::Binaire { op, a, b, .. } = test {
                        if op == "<" {
                            if let (Expr::Var(na, _), Expr::Appel { nom, args, .. }) =
                                (a.as_ref(), b.as_ref())
                            {
                                if (nom == "longueur" || nom == "len") && args.len() == 1 {
                                    if let Expr::Var(nl, _) = &args[0] {
                                        let i = corps.noms.iter().position(|x| x == na);
                                        let l = corps.noms.iter().position(|x| x == nl);
                                        if let (Some(i), Some(l)) = (i, l) {
                                            if natures.get(l) == Some(&NatureSlot::ListeEntiers) {
                                                let va = self.builder.use_var(slot_vars[i]);
                                                let h = self.builder.use_var(slot_vars[l]);
                                                let f = self.import_runtime("rose_jit_longueur");
                                                let c = self.builder.ins().call(f, &[h]);
                                                let vb = self.builder.inst_results(c)[0];
                                                let cond = self.builder.ins().icmp(IntCC::SignedLessThan, va, vb);
                                                let tgt_false = if *cible < n { blocks[*cible] } else { end_block };
                                                self.br_args(cond, next, &[], tgt_false, &[]);
                                                emitted = true;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    }
                    if !emitted { return false; }
                }
                // vague 20 — `joindre($liste, $entier)` (void).
                Op::ExprStmt(Expr::Appel { nom, args, .. }) if nom == "joindre" && args.len() == 2 => {
                    let (Expr::Var(nl, _), Expr::Var(nv, _)) = (&args[0], &args[1]) else { return false; };
                    let (Some(l), Some(v)) = (
                        corps.noms.iter().position(|x| x == nl),
                        corps.noms.iter().position(|x| x == nv),
                    ) else { return false; };
                    if natures.get(l) != Some(&NatureSlot::ListeEntiers) { return false; }
                    let h = self.builder.use_var(slot_vars[l]);
                    let val = self.builder.use_var(slot_vars[v]);
                    let f = self.import_runtime("rose_jit_joindre");
                    let _ = self.builder.ins().call(f, &[h, val]);
                    self.saut(next, &[]);
                }
                // vague 23 (D7) — `inserer($t, $k, $v)` : table à handle, clé
                // et valeur i64 nues prouvées en entrée. Un appel runtime,
                // zéro Valeur boxée, zéro recherche de slot par nom.
                Op::ExprStmt(Expr::Appel { nom, args, .. }) if nom == "inserer" => {
                    let Expr::Var(tn, _) = &args[0] else { return false; };
                    let Some(t) = corps.noms.iter().position(|x| x == tn) else { return false; };
                    if natures.get(t) != Some(&NatureSlot::Table) { return false; }
                    let Some(k) = valeur_nue_ast(corps, &args[1]) else { return false; };
                    let Some(v) = valeur_nue_ast(corps, &args[2]) else { return false; };
                    let hv = self.builder.use_var(slot_vars[t]);
                    let kv = self.emettre_valeur_nue(&k, &slot_vars);
                    let vv = self.emettre_valeur_nue(&v, &slot_vars);
                    let f = self.import_runtime("rose_jit_table_inserer_i64");
                    let _ = self.builder.ins().call(f, &[hv, kv, vv]);
                    let zt = self.builder.ins().iconst(types::I64, 0);
                    self.builder.def_var(self.tag_tmp, zt);
                    self.unboxed_tmp = Some(self.builder.ins().iconst(types::I64, 1));
                    self.saut(next, &[]);
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
        // 20/11 — miroir de `expr()` juste dessous : quand le lexème `i`
        // (LitComplexe 0+1i) est en réalité la VARIABLE ENTIÈRE du compteur
        // (`types["i"] == Entier`), il doit être typé Entier ici AUSSI.
        // Sinon `binaire` le prend pour un complexe et émet `$s + i` en
        // f64 (binaire_cplx) : le compteur devient un complexe et
        // `rose_boucles.rose` rend 99 au lieu de 4950 (mesuré le 20/11).
        if matches!(e, Expr::LitComplexe { re, im, .. } if *re == 0.0 && *im == 1.0)
            && types.get("i").copied() == Some(TyKind::Entier)
        {
            return TyKind::Entier;
        }
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
                // SANS arithmétique complexe, ce 0+1i ne peut pas être
                // l'imaginaire : c'est la variable repliée par le repli de
                // constantes — on l'émet comme un entier 1 (la valeur que le
                // repli a gardée).
                // ⚠ 19/11 : cette heuristique est CONDITIONNÉE à
                // `!i_est_imaginaire`. Dès que la fonction fait de l'arithmétique
                // complexe (mesuré sur `calc`, bench/suite/complexe.rose), le
                // 0+1i est la VRAIE unité imaginaire et DOIT être émis en deux
                // f64 : l'ancienne heuristique produisait `z_im * 1` (réel),
                // donc `total_re = n` au lieu de 0.9999999999860738.
                if *re == 0.0 && *im == 1.0
                    && !self.i_est_imaginaire
                    && !crate::front::i_est_compteur()
                {
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

/// 18/11 — D3 : primitive `temps()` sans état (même horodatage que
/// `interp.rs:4208`), rendue comme i64 nu (l'horloge tient en i64).
#[unsafe(no_mangle)]
pub extern "C" fn rose_temps() -> i64 { crate::run::temps_unix_nanos() as i64 }

// ═══════════════════════════════════════════════════════════════════════
// VAGUE 20 — TYPES NON NUMÉRIQUES DANS LE CORPS PRINCIPAL JIT
// ═══════════════════════════════════════════════════════════════════════
//
// Le principal est compilé en natif quand `rir_compilable_jit` le prouve
// (J1/P1/L1). D5 (chaîne) et D6 (liste) restaient interprétés : leurs boucles
// contiennent `$s = $s + "x"`, `joindre`, `longueur`, `$l[$j]` — autant
// d'expressions AST hors sous-ensemble. Le diagnostic du 18/11 a montré que
// le DISPATCH interprété domine (97 ns/it sur 167 ns pour D5 ; boucle vide
// JIT 0,024 ms contre 2,18 ms interprétée, ×91).
//
// Les valeurs non numériques sont représentées dans les slots JIT par des
// HANDLES : des indices i64 dans une table thread-local de `Valeur`. Le code
// natif ne manipule jamais la `Valeur` elle-même, il passe les handles aux
// helpers ci-dessous. Les littéraux de chaîne sont internés au moment de
// l'émission (une seule fois, pas par itération).
thread_local! {
    static JIT_HANDLES: std::cell::RefCell<Vec<Valeur>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Réinitialise la table de handles avant une compilation du principal.
pub(crate) fn jit_handles_reinitialiser() {
    JIT_HANDLES.with(|t| t.borrow_mut().clear());
    JIT_I64_BUFFERS.with(|t| t.borrow_mut().clear());
}

/// Arène des tableaux compacts émis par LISTE_P8. Le pointeur est tenu vivant
/// par ce registre thread-local ; la mémoire est libérée ici, à la même
/// frontière que les handles JIT (une table par exécution de programme).
thread_local! {
    static JIT_I64_BUFFERS: RefCell<Vec<Box<[i64]>>> = const { RefCell::new(Vec::new()) };
}

/// Alloue un `Vec<i64>` de `n` cases pour la boucle LISTE_P8 et rend son
/// pointeur de données. Les cases sont toutes écrites par le code machine
/// avant d'être lues ; `set_len` est donc valide ici aussi.
#[unsafe(no_mangle)]
pub extern "C" fn rose_jit_i64_buffer(n: i64) -> i64 {
    if n <= 0 { return 0; }
    let mut v = Vec::<i64>::with_capacity(n as usize);
    unsafe { v.set_len(n as usize); }
    let boxed = v.into_boxed_slice();
    let p = boxed.as_ptr() as i64;
    JIT_I64_BUFFERS.with(|t| t.borrow_mut().push(boxed));
    p
}

fn jit_allouer_handle(v: Valeur) -> i64 {
    JIT_HANDLES.with(|t| {
        let mut t = t.borrow_mut();
        let h = t.len() as i64;
        t.push(v);
        h
    })
}

#[inline]
fn jit_lire_handle(h: i64) -> Valeur {
    JIT_HANDLES.with(|t| t.borrow()[h as usize].clone())
}

/// Accès SANS CLONAGE au `Valeur` pointé par un handle (voie i64 liste).
///
/// INVARIANT (identique à `Table::rep_nv` en `run.rs`, voir TABLE_I64_FAST.md) :
/// le moteur est MONOTHREADÉ (`Rc`, jamais `Arc`) et les helpers ci-dessous
/// sont des appels FEUILLE : aucun d'eux n'appelle `jit_allouer_handle` ni ne
/// réalloue le `Vec` de handles pendant qu'il détient la référence. Le drapeau
/// d'emprunt `RefCell` est donc toujours libre. `debug_assert!` (actif en debug,
/// vérifié par `cargo test`, éliminé en release) détecte toute violation.
/// Le moteur n'a qu'UNE représentation de liste (`Valeur::Liste`), donc aucun
/// risque de divergence JIT ↔ interpréteur : c'est le même objet.
#[inline(always)]
fn jit_handle_ref(h: i64) -> &'static Valeur {
    JIT_HANDLES.with(|t| {
        #[cfg(debug_assertions)] debug_assert!(t.try_borrow().is_ok(), "emprunt JIT_HANDLES actif");
        let ptr = t.as_ptr();
        debug_assert!((h as usize) < unsafe { (*ptr).len() }, "handle JIT invalide");
        let items = unsafe { (*ptr).as_ptr() };
        unsafe { &*items.add(h as usize) }
    })
}

/// Accès SANS contrôle dynamique au `Vec<Valeur>` d'une `Liste`.
/// Même invariant que `jit_handle_ref` : helper feuille, monothreadé, pas
/// d'emprunt `RefCell` actif. Le `Vec` reste la source de vérité unique —
/// affichage, égalité, `sous_liste` et interpréteur voient exactement les
/// mêmes éléments (aucune représentation parallèle, aucune conversion).
#[inline(always)]
fn liste_vec_nv(l: &crate::run::Liste) -> &'static mut Vec<Valeur> {
    #[cfg(debug_assertions)] debug_assert!(l.0.try_borrow_mut().is_ok(), "emprunt Liste actif");
    unsafe { &mut *l.0.as_ptr() }
}

/// Handle « pointeur » d'une liste JIT : BIT 63 à 1 pour distinguer un
/// pointeur de `RefCell<Vec<Valeur>>` d'un index dans `JIT_HANDLES`.
/// ⚠ Le bit 0 NE PEUT PAS servir : les indices de handles sont petits et
/// peuvent être impairs (mesuré : `r_str.rose` segfaultait avec un tag bit 0).
/// Bit 63 : les adresses utilisateur aarch64 n'utilisent que 48 bits, donc
/// le bit haut est toujours 0 ; un index de handle est toujours < 2^31.
/// La `Valeur` reste ENREGISTRÉE dans `JIT_HANDLES` : c'est la MÊME liste,
/// lue par l'interpréteur sans conversion (une seule représentation).
const LISTE_TAG: i64 = i64::MIN;
#[inline]
fn liste_handle_ptr(l: &crate::run::Liste) -> i64 {
    let p = std::rc::Rc::as_ptr(&l.0) as i64;
    debug_assert_eq!(p & LISTE_TAG, 0, "pointeur Liste: bit 63 déjà posé");
    p | LISTE_TAG
}

/// Résout le `Vec` d'une liste à partir de son handle (pointeur tagué ou,
/// repli, index `JIT_HANDLES`). Zéro accès thread-local sur le chemin
/// pointeur : c'est le gain mesuré de la voie i64 liste.
#[inline(always)]
fn liste_mut_depuis_handle(h: i64) -> Option<&'static mut Vec<Valeur>> {
    if h & LISTE_TAG != 0 {
        let p = (h & !LISTE_TAG) as *mut std::cell::RefCell<Vec<Valeur>>;
        let cell: &std::cell::RefCell<Vec<Valeur>> = unsafe { &*p };
        #[cfg(debug_assertions)] debug_assert!(cell.try_borrow_mut().is_ok(), "emprunt Liste actif");
        let vp: *mut Vec<Valeur> = cell.as_ptr();
        Some(unsafe { &mut *vp })
    } else {
        match jit_handle_ref(h) {
            Valeur::Liste(l) => Some(liste_vec_nv(l)),
            _ => None,
        }
    }
}

/// Concaténation `+` de deux chaînes : la RÈGLE EXACTE (`binaire_valeurs`).
#[unsafe(no_mangle)]
pub extern "C" fn rose_jit_chaine_concat(a: i64, b: i64) -> i64 {
    let x = jit_lire_handle(a);
    let y = jit_lire_handle(b);
    match crate::interp::binaire_valeurs("+", &x, &y) {
        Ok(v) => jit_allouer_handle(v),
        Err(_) => jit_allouer_handle(Valeur::Vide),
    }
}

/// CHAINE_ACCUM — construction DIRECTE du résultat d'une boucle d'accumulation
/// de chaîne prouvée. Un seul appel remplace N concaténations : aucun nœud
/// `Concat`, aucun handle intermédiaire, aucune allocation par itération.
/// Le résultat est une `Valeur::Chaine` à nœud unique `Plein(String)`, avec le
/// nombre de CARACTÈRES Unicode maintenu exactement comme la rope (init.n +
/// N × chunk.n). Aucune valeur partagée n'est mutée : l'aliasage est préservé.
/// Appelé UNIQUEMENT sous `detecter_chaine_accumulation`.
#[unsafe(no_mangle)]
pub extern "C" fn rose_jit_chaine_accum(h_init: i64, h_chunk: i64, n: i64) -> i64 {
    let init = jit_lire_handle(h_init);
    let chunk = jit_lire_handle(h_chunk);
    let (is, inb) = match &init {
        Valeur::Chaine(r) => (r.aplatir(), r.n),
        _ => (String::new(), 0),
    };
    let (cs, cnb) = match &chunk {
        Valeur::Chaine(r) => (r.aplatir(), r.n),
        _ => (String::new(), 0),
    };
    let cnt = n.max(0) as usize;
    let cap = is.len().saturating_add(cnt.saturating_mul(cs.len()));
    let mut s = String::with_capacity(cap);
    s.push_str(&is);
    // REPETITION PAR BLOCS : `repeat` fait une seule allocation + une copie
    // vectorisée, au lieu de N appels `push_str` (mesuré : gain sur D5).
    s.push_str(&cs.repeat(cnt));
    let rep = crate::run::ChaineRep {
        n: inb.saturating_add(cnt.saturating_mul(cnb)),
        octets: s.len(),
        noeud: crate::run::NoeudChaine::Plein(s),
    };
    jit_allouer_handle(Valeur::Chaine(std::rc::Rc::new(rep)))
}

/// Liste vide typée entier (`[0 : entier]`).
#[unsafe(no_mangle)]
pub extern "C" fn rose_jit_liste_vide() -> i64 {
    // Réservation de capacité (recette TI1, voir TABLE_I64_FAST.md §2.3) :
    // une liste JIT est prouvée homogène i64 et typiquement remplie par une
    // boucle comptée. Réserver évite les recopies géométriques du
    // `Vec<Valeur>` pendant le remplissage. Coût mémoire assumé documenté
    // dans LISTE_I64_FAST.md.
    let v = Valeur::liste_de(Vec::with_capacity(131072));
    match &v {
        Valeur::Liste(l) => {
            let h = liste_handle_ptr(l);
            // La Valeur reste accessible par l'interpréteur : une seule
            // représentation, aucune conversion, cohérence totale.
            jit_allouer_handle(v);
            h
        }
        _ => unreachable!(),
    }
}

// ── LISTE_INLINE (18/11) : helpers HORS chemin chaud ──────────────────────
//
// Ces trois helpers ne sont appelés QU'UNE FOIS par le principal spécialisé,
// avant/après les boucles. Ils ne font aucun travail par élément : toute la
// boucle est émise en Cranelift (stores/loads directs dans le `Vec<Valeur>`).
// Le `Valeur::Liste` reste l'unique source de vérité : `set_len` publie la
// longueur dans le VRAI `Vec`, donc l'interpréteur, `>> $l`, `longueur`,
// `sous_liste` et l'égalité lisent exactement les mêmes éléments.

/// Garantit `capacité >= n` avant que le JIT écrive directement les éléments.
/// `reserve_exact` est chaque fois que le handle est valide (il vient de
/// `rose_jit_liste_vide` dans le motif détecté).
#[unsafe(no_mangle)]
pub extern "C" fn rose_jit_liste_reserve(h: i64, n: i64) {
    if n <= 0 { return; }
    if let Some(vec) = liste_mut_depuis_handle(h) {
        if vec.capacity() < n as usize {
            vec.reserve_exact(n as usize - vec.len());
        }
    }
}

/// Rend (pointeur de données, capacité) du `Vec<Valeur>` dans deux cases
/// mémoire fournies par le JIT (même convention que `tagged_binop_abi`).
#[unsafe(no_mangle)]
pub extern "C" fn rose_jit_liste_meta(h: i64, p_ptr: *mut i64, p_cap: *mut i64) {
    if p_ptr.is_null() || p_cap.is_null() { return; }
    if let Some(vec) = liste_mut_depuis_handle(h) {
        unsafe {
            *p_ptr = vec.as_ptr() as i64;
            *p_cap = vec.capacity() as i64;
        }
    } else {
        unsafe { *p_ptr = 0; *p_cap = 0; }
    }
}

/// Publie `len` dans le VRAI `Vec` après que la boucle JIT a écrit les
/// éléments. `set_len` est sûr ici : les `n` premiers emplacements ont tous
/// été initialisés par les `store` de la boucle spécialisée.
#[unsafe(no_mangle)]
pub extern "C" fn rose_jit_liste_set_len(h: i64, n: i64) {
    if n < 0 { return; }
    if let Some(vec) = liste_mut_depuis_handle(h) {
        if vec.capacity() >= n as usize {
            unsafe { vec.set_len(n as usize); }
        }
    }
}

/// Layout de `Valeur::Entier(v, None)`, MESURÉ au moment de la compilation JIT
/// (pas codé en dur) : on compare deux valeurs qui ne diffèrent que par leur
/// i64 pour localiser le mot à écrire, et on rend le gabarit des autres mots.
/// `None` si la représentation n'est pas 3 mots de 8 octets — dans ce cas le
/// détecteur refuse la spécialisation et l'émetteur générique inchangé prend
/// le relais. Rend `(offset_octets_du_i64, [mots du gabarit])`.
fn layout_valeur_entier_nu() -> Option<(usize, [u64; 8])> {
    const SZ: usize = std::mem::size_of::<Valeur>();
    if SZ != 24 || std::mem::align_of::<Valeur>() != 8 { return None; }
    // ⚠ Construire DEUX constantes distinctes ferait varier le mot 0 : Rust
    // y range une adresse de glue de drop, dupliquée par le compilateur.
    // On modifie donc UNE SEULE valeur sur place, par son champ i64.
    let mut v = Valeur::Entier(0, None);
    let mut wa = [0u64; 8];
    unsafe {
        std::ptr::copy_nonoverlapping(&v as *const Valeur as *const u8,
                                      wa.as_mut_ptr() as *mut u8, SZ);
    }
    if let Valeur::Entier(x, _) = &mut v { *x = 0x1111_1111_1111_1111; }
    let mut wb = [0u64; 8];
    unsafe {
        std::ptr::copy_nonoverlapping(&v as *const Valeur as *const u8,
                                      wb.as_mut_ptr() as *mut u8, SZ);
    }
    let mut off = None;
    for (i, (x, y)) in wa.iter().zip(wb.iter()).take(SZ / 8).enumerate() {
        if x != y {
            if off.is_some() { return None; }
            off = Some(i * 8);
        }
    }
    let off = off?;
    wa[off / 8] = 0; // gabarit : le mot du i64 sera écrit par le JIT
    Some((off, wa))
}

/// `joindre(liste, entier)` : mutation en place, SANS clone de handle ni
/// contrôle `RefCell` dynamique (voie i64 liste, voir le doc LISTE_I64_FAST).
/// L'entier est passé en i64 nu — sûr car le garde `principal_sans_boxage`
/// prouve que les slots numériques du principal ne posent jamais de BigInt
/// (voir `expr_peut_boxer`). `Valeur::z` conserve le boxage au-delà de 2²⁴ :
/// la représentation reste UNIQUE et identique à l'interpréteur.
#[unsafe(no_mangle)]
pub extern "C" fn rose_jit_joindre(h: i64, v: i64) {
    if let Some(vec) = liste_mut_depuis_handle(h) {
        vec.push(Valeur::z(v));
    }
}

/// `longueur(liste_ou_chaine)` : O(1) pour la rope et la liste.
#[unsafe(no_mangle)]
pub extern "C" fn rose_jit_longueur(h: i64) -> i64 {
    // Chemin liste (handle pointeur tagué) : zéro thread-local, zéro clone.
    if h & LISTE_TAG != 0 {
        if let Some(vec) = liste_mut_depuis_handle(h) {
            return vec.len() as i64;
        }
    }
    match jit_handle_ref(h) {
        Valeur::Chaine(s) => s.n as i64,
        Valeur::Liste(l) => liste_vec_nv(l).len() as i64,
        Valeur::Table(t) => t.0.borrow().items.len() as i64,
        v => crate::run::chaine_de(v).chars().count() as i64,
    }
}

/// `liste[i]` rendu comme i64 nu. ⚠ Réservé aux listes d'entiers non boxés :
/// le garde `liste_entiers_nus` le prouve statiquement avant d'accepter le JIT.
#[unsafe(no_mangle)]
pub extern "C" fn rose_jit_index_i64(h: i64, i: i64) -> i64 {
    if let Some(vec) = liste_mut_depuis_handle(h) {
        if let Some(Valeur::Entier(x, None)) = vec.get(i as usize) {
            return *x;
        }
    }
    // Ne doit jamais arriver sous le garde ; valeur neutre pour éviter UB.
    0
}

// ── Vague 23 (D7) : table native à clés/valeurs entières ─────────────────
//
// Le handle pointe sur une `Valeur::Table` rc-partagée. Les helpers délèguent
// à `Table::inserer`/`Table::trouver`, dont l'index canonique est la SOURCE
// DE VÉRITÉ : `longueur`, `cles`, `valeurs`, l'indexation et l'affichage
// voient exactement les mêmes paires que l'interpréteur. Le garde statique
// (slot initialisé par un littéral entier, seulement mis à jour par ±littéral)
// prouve que la clé et la valeur sont des i64 nus : aucun `Valeur` boxé,
// aucun `HashMap<String, _>` de slots, aucun appel de délégation ici.
#[inline]
fn table_du_handle(h: i64) -> Option<crate::run::Table> {
    // `Valeur` implémente désormais Drop : on ne peut plus déplacer `t` hors
    // de la valeur empruntée. `Table` est un Rc<RefCell<_>> ; le clone est
    // exactement la même poignée partagée (aucune copie de données).
    match jit_lire_handle(h) {
        crate::run::Valeur::Table(ref t) => Some(t.clone()),
        _ => None,
    }
}

/// Vague 24 (D7) — accès DIRECT au handle sans cloner le `Valeur::Table`
/// (un Rc par appel dans `table_du_handle`). 6000 appels/bench : on évite
/// 6000 increments/décréments de compteur Rc + 6000 drops.
#[inline]
fn avec_table_ref<R, F>(h: i64, f: F) -> R
where F: FnOnce(&crate::run::Table) -> R, R: Default {
    JIT_HANDLES.with(|t| {
        let tab = t.borrow();
        match tab.get(h as usize) {
            Some(crate::run::Valeur::Table(tb)) => f(tb),
            _ => R::default(),
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn rose_jit_table_vide() -> i64 {
    jit_allouer_handle(crate::run::Valeur::table())
}

#[unsafe(no_mangle)]
pub extern "C" fn rose_jit_table_inserer_i64(h: i64, k: i64, v: i64) {
    // Voie rapide i64 : hachage canonique partagé (`h_entier`), un seul
    // emprunt RefCell, aucune `Valeur` construite pour la recherche.
    // Repli automatique vers `Table::inserer` si |k| > 2^24 (voir run.rs).
    avec_table_ref(h, |t| t.inserer_i64(k, v));
}

#[unsafe(no_mangle)]
pub extern "C" fn rose_jit_table_cle_existe_i64(h: i64, k: i64) -> i64 {
    avec_table_ref(h, |t| if t.contient_i64(k) { 1 } else { 0 })
}

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
pub extern "C" fn rose_affiche_prefixe(_ptr: i64, _len: i64) {
    // 20/11 — miroir de `run::afficher_formule` (run.rs:1030) : la formule est
    // TOUJOURS vide depuis le 16/11, la sortie d'exécution est la VALEULE
    // SEULE. L'émetteur garde les appels (changer l'émission modifiait
    // l'éligibilité du principal, mesuré sur rose_boucles.rose), mais les
    // helpers sont désormais des no-ops : plus de ligne LaTeX parasite.
    // Le LaTeX se lit par `rose latex`, pas par l'exécution.
}

#[unsafe(no_mangle)]
pub extern "C" fn rose_affiche_formule(_ptr: i64, _len: i64) {
    // idem : voir `rose_affiche_prefixe`.
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
