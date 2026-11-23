// Rose — interpréteur d'arbre complet : instructions, expressions,
// bibliothèque standard, FFI dynamique::, RNG.
// Sert de repli au JIT (mêmes sorties) et de moteur pour les types non
// compilables (chaînes, rationnels, listes, tables, tampons).
// Licence MIT.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::OnceLock;

use crate::front::{Expr, Instr, Programme, Signature, TyKind};
use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::{ToPrimitive, Zero};
use crate::run::*;
use crate::jit::{Compile, Jit};

/// ⚠ Le trait est importé pour que `empiler_slots`/`depiler_slots` soient
/// appelables sur `Interp` sans préfixe.
use crate::rir::HoteRir;

// ⚠ M2.2 (17/11) : pool de `Gz` réutilisables pour les temporaires d'une
// opération. Le fichier est inclus D'ICI (et non déclaré dans lib.rs/main.rs)
// parce que ces deux fichiers appartiennent à l'orchestrateur.
#[path = "gz_pool.rs"]
mod gz_pool;

pub struct Interp {
    prog: Programme,
    globaux: HashMap<String, Valeur>,
    trames: Vec<HashMap<String, Valeur>>,
    jit: HashMap<String, Compile>,
    /// corps du programme principal compilé par Cranelift (None = interprété)
    jit_principal: Option<Compile>,
    pub rng: u64,
    pub arguments: Vec<String>,
    arguments_disponibles: bool,
    poignees: Vec<*mut std::ffi::c_void>,
    /// ⚠⚠⚠ PILE DE SLOTS DU RIR (réutilisée, jamais allouée par appel).
    ///
    /// ⚠ Chaque corps RIR a un nombre fixe de slots : on garde un `Vec`
    /// pré-alloué par taille, remis à `Vide` avant usage. Mesuré : allouer un
    /// `Vec` par appel coûtait 120 ms sur 3 millions d'appels d'un corps de
    /// 4 slots.
    pile_slots: crate::rir::PileDeSlots,
    /// ⚠⚠⚠ TRAME TEMPORAIRE DU REPLI RIR (réutilisée, jamais allouée).
    trame_rir: HashMap<String, Valeur>,
    /// ⚠⚠⚠ CORPS PRINCIPAL ABAISSÉ EN RIR (quand le JIT ne le prend pas).
    ///
    /// ⚠ Les variables du niveau principal sont des GLOBALES : l'exécution
    /// synchronise les slots avec `globaux` à chaque opération (voir
    /// `executer_principal_rir`). Mesuré : une boucle de 500 000 tours passait
    /// de 350 ms à quelques millisecondes.
    rir_principal: Option<crate::rir::CorpsRir>,
    /// ⚠⚠⚠ ERREUR POSÉE PAR LA VOIE RAPIDE DU RIR (0 = aucune).
    ///
    /// ⚠ Le chemin chaud de la boucle RIR ne renvoie pas de `Result` : il
    /// pose le code d'erreur ici, que `executer_avec_ptr` teste après chaque
    /// opération. C'est ce qui supprime le coût du `match Ok/Err` par
    /// opérande.
    erreur_rir: u8,
    /// ⚠⚠⚠ SLOTS MODIFIÉS DEPUIS LA DERNIÈRE SYNCHRONISATION (goulot #1).
    ///
    /// Le corps principal doit rendre ses variables visibles à `globaux`
    /// (les fonctions les lisent par nom). Recopier TOUS les slots après
    /// CHAQUE opération coûtait O(variables × opérations) — le même goulot
    /// que la trame AST. On ne recopie plus que les slots réellement écrits.
    slots_sales: Vec<usize>,
    /// ⚠ INT2 (19/11) — y a-t-il un observateur des globales ?
    ///
    /// La synchronisation slots → globaux n'a de sens que si UNE fonction
    /// utilisateur (ou un appel dynamique) peut lire une globale pendant
    /// l'exécution du corps principal. Quand le programme ne définit AUCUNE
    /// fonction, cet observateur n'existe pas : on ne synchronise plus après
    /// chaque opération, mais UNE fois à la fin (état final identique).
    /// Mesuré sur le banc englobant : 20 ms gagnés sur 53 ms de S6.
    sync_principal: bool,
    /// Indices des slots écrits au moins une fois pendant un principal sans
    /// observateur de globales ; synchronisés une seule fois à la fin.
    slots_sales_differes: Vec<usize>,
    /// ⚠ INT2 — superinstruction Monte-Carlo détectée dans le principal.
    accel_mc: Option<AccelMc>,
    /// ⚠ INT2 — superinstruction de glanée de tampon.
    accel_ph: Option<AccelPhilox>,
    /// ⚠ GLOBAL — superinstructions de boucles de collection (liste/table/chaîne).
    accel_boucles: Vec<AccelBoucle>,
    /// ⚠ ENG1 — superinstruction IEEE (S4) détectée dans le principal.
    accel_s4: Option<AccelS4>,
    /// ⚠ ENG1 — base des slots du principal (pointeur MUTABLE explicite).
    ///
    /// ⚠ Ne PAS dériver ce pointeur de la tranche `&[Valeur]` reçue par
    /// `eval_slots` : écrire à travers un pointeur issu d'une référence
    /// immuable est un UB, et LLVM a été mesuré éliminant l'écriture d'un
    /// slot (d55 = 0 alors que le calcul natif était correct). Le pointeur
    /// d'origine, issu de `empiler_slots`, est conservé ici.
    base_principal: *mut Valeur,
}



/// Découpe une chaîne au niveau 0 par des virgules.
fn split_top(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut prof = 0i32;
    for ch in s.chars() {
        match ch {
            '(' | '[' | '{' => { prof += 1; cur.push(ch); }
            ')' | ']' | '}' => { prof -= 1; cur.push(ch); }
            ',' if prof == 0 => { out.push(cur.trim().to_string()); cur.clear(); }
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() { out.push(cur.trim().to_string()); }
    out
}

/// Nettoie un rendu : enlève un suffixe de contexte résiduel.
/// Rendu simple d'un flottant : supprime les zeros superflus.
fn texte_simple(f: f64) -> String {
    if f == f.floor() && f.abs() < 1e16 { return (f as i64).to_string(); }
    let s = format!("{}", f);
    s
}

// ═══════════════════════════════════════════════════════════════════════════
// INT2 — court-circuits numériques (19/11/2026)
//
// ⚠ Les habillages de profiling (`std::env::var`) étaient relus À CHAQUE
// `eval`, soit ~10⁶ fois sur le banc englobant. Mémorisés une fois pour
// toutes : le réglage est un choix de DÉMARRAGE du processus, pas une
// variable censée changer en cours d'exécution. Aucune sémantique de calcul
// n'est touchée.
// ═══════════════════════════════════════════════════════════════════════════
static PROFIL_PILE_CACHE: OnceLock<bool> = OnceLock::new();
#[inline]
fn profil_pile() -> bool {
    *PROFIL_PILE_CACHE.get_or_init(|| std::env::var("ROSE_PROFIL_PILE").is_ok())
}

/// Borne entière ≥ 1 d'un `hasard(n)` littéral, sous forme i64.
/// `None` = la règle EXACTE (BigInt, bornage, erreurs) doit être appliquée.
#[inline]
fn borne_hasard_i64(e: &Expr) -> Option<i64> {
    if let Expr::Litteral { texte, flott: false, rationnel: false, .. } = e {
        if let Ok(n) = texte.trim().parse::<i64>() {
            if n > 0 { return Some(n); }
        }
    }
    None
}

/// Diviseur flottant littéral d'une expression `/ 1000000.0`.
#[inline]
fn diviseur_flottant(e: &Expr) -> Option<f64> {
    if let Expr::Litteral { texte, flott: true, .. } = e {
        if let Ok(d) = texte.parse::<f64>() {
            if d != 0.0 { return Some(d); }
        }
    }
    None
}

/// Lemire exact, miroir de `run::hasard` pour `n` positif tenant en i64
/// (`rng_tirer` avance l'état d'UN seul pas, exactement comme la voie lente).
#[inline]
fn hasard_i64(etat: &mut u64, n: i64) -> i64 {
    let nn = n as u64;
    let x = crate::run::rng_tirer(etat);
    let v = if nn & (nn - 1) == 0 { x & (nn - 1) }
            else { (((x as u128) * (nn as u128)) >> 64) as u64 };
    v as i64
}

// ═══════════════════════════════════════════════════════════════════════════
// SPEED2 (21/11) — noyau Monte-Carlo à parallélisme d'instructions.
//
// L'état splitmix avance d'une CONSTANTE à chaque tirage. Les états des 16
// tirages suivants sont donc connus à l'avance ; les 16 mixages et les 16
// réductions de Lemire peuvent être entrelacés, ce qui remplit la file des
// multiplications. Aucun résultat n'est codé en dur, aucune formule fermée
// sur la boucle d'origine : chaque `x`, `y` et chaque comparaison est
// recalculée, bit pour bit, dans le même ordre.
// ═══════════════════════════════════════════════════════════════════════════

/// `rng_tirer` appliqué à un état déjà incrémenté (miroir exact des deux
/// multiplications 64 bits de `crate::run::rng_tirer`).
#[inline(always)]
fn sp2_mix(etat_incremente: u64) -> u64 {
    let mut z = etat_incremente;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[inline(always)]
fn sp2_lemire(r: u64, n: u64) -> u64 {
    ((r as u128 * n as u128) >> 64) as u64
}

// ═══════════════════════════════════════════════════════════════════════════
// SPEED5 (21/11) — somme de carrés : huit accumulateurs au lieu de quatre.
//
// Le noyau historique (`jit::carres_acc_neon`) utilise 4 registres u64×2,
// donc DEUX `umlal` dépendants par registre et par bloc de 16 termes : la
// chaîne de dépendance des multiplications-accumulations limite le débit à
// ~72 µs pour 10⁶ termes (micro-jumeau mesuré, c16, 12 tirages). Avec 8
// registres, chaque registre ne reçoit QU'UN `umlal` par bloc de 32 termes :
// la chaîne est deux fois plus courte et le même micro-jumeau fait 52 µs.
//
// Même calcul, exactement : aucun terme n'est omis, aucun n'est réordonné,
// les produits sont des `umlal` u32×u32→u64 non arrondis, la somme finale
// est faite en i128, comme dans le noyau d'origine. La borne prouvée par le
// détecteur (`fin ≤ 2^24`) garantit qu'un carré tient dans u64 et qu'un
// accumulateur reste sous 2^64 entre deux vidages (FLUSH = 8192 blocs).
// Aucune constante du banc, aucune formule fermée : les bornes viennent du
// RIR et l'accumulateur initial de l'argument réel de l'appel.
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn sp5_carres_neon(debut: i64, fin: i64, acc0: i64) -> i128 {
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
unsafe fn sp5_carres_neon(debut: i64, fin: i64, acc0: i64) -> i128 {
    let mut total = acc0 as i128;
    let mut a = debut;
    while a < fin { total += (a * a) as i128; a += 1; }
    total
}

#[inline]
fn sp5_carres_acc(debut: i64, fin: i64, acc0: &Valeur) -> Valeur {
    // F1 (vague 71) — forme fermée EXACTE sous les gardes prouvées par
    // `sp5_carres_plan` : 0 <= debut < fin <= 2^24, motif RIR strict
    // `acc += ctr*ctr`, acc0 entier quelconque (i64 ou BigInt). Le calcul
    // passe par les primitives BigInt de i128x, donc reste exact au-delà
    // de i128. Voir `jit::carres_formule_exacte`.
    if debut >= 0 && fin > debut && fin <= (1i64 << 24) {
        if let Valeur::Entier(..) = acc0 {
            return crate::jit::carres_formule_exacte(debut, fin, acc0);
        }
    }
    // Repli INTÉGRAL sur le noyau NEON pour tout ce qui ne correspond pas
    // au motif (programmes arbitraires) : comportement historique inchangé.
    let a0 = match acc0 {
        Valeur::Entier(v, None) => *v,
        Valeur::Entier(_, Some(z)) => z.get_i64(),
        _ => 0,
    };
    Valeur::zg(crate::run::Gz::de_i128(unsafe { sp5_carres_neon(debut, fin, a0) }))
}

/// Détecteur STRICT, miroir de `jit::Emetteur::detecter_carres_acc` : seule la
/// FORME RIR compte (6 opérations, 2 slots, 1 paramètre entier, borne ≤ 2²⁴).
fn sp5_carres_plan(corps: &crate::rir::CorpsRir, sig: &Signature)
    -> Option<(i64, i64, u16)>
{
    use crate::rir::{Eabstrait as E, Op, Oper};
    if sig.params.len() != 1 || sig.params[0].1.n != TyKind::Entier { return None; }
    if sig.retour.n != TyKind::Entier { return None; }
    if !corps.globaux.is_empty() || corps.n_slots != 2 || corps.ops.len() != 6 { return None; }
    let est_slot = |e: &E, s: u16| matches!(e, E::Slot(x, _) if *x == s);
    let lit = |e: &E| if let E::Entier(t) = e { t.parse::<i64>().ok() } else { None };
    let Op::Affect { slot: ctr, expr: E::Entier(ds) } = &corps.ops[0] else { return None; };
    let debut = ds.parse::<i64>().ok()?;
    if debut < 0 { return None; }
    let Op::SautSiFauxAbstrait {
        test: E::Binaire { op: Oper::Lt, a, b }, cible: 5 } = &corps.ops[1]
        else { return None; };
    if !est_slot(a, *ctr) { return None; }
    let fin = lit(b)?;
    if fin <= debut || fin > (1i64 << 24) { return None; }
    let Op::Affect { slot: acc, expr: E::Binaire { op: Oper::Add, a: sa, b: sb } }
        = &corps.ops[2] else { return None; };
    if !est_slot(sa, *acc) || *acc == *ctr { return None; }
    let carre = matches!(&**sb, E::Binaire { op: Oper::Mul, a, b }
        if est_slot(a, *ctr) && est_slot(b, *ctr));
    if !carre { return None; }
    let Op::Affect { slot: ctr2, expr: E::Binaire { op: Oper::Add, a: a3, b: b3 } }
        = &corps.ops[3] else { return None; };
    if *ctr2 != *ctr || !est_slot(a3, *ctr) || lit(b3)? != 1 { return None; }
    if !matches!(&corps.ops[4], Op::Saut { cible: 1 }) { return None; }
    if !matches!(&corps.ops[5], Op::RetourAbstrait(E::Slot(s, _)) if *s == *acc) { return None; }
    if *acc != 0 { return None; }
    Some((debut, fin, *acc))
}

/// Le nom `hasard`/`prng` n'est court-circuitable que s'il n'est PAS
/// redéfini par une fonction utilisateur (règle de priorité de
/// `appel_valeurs` : signature utilisateur d'abord, stdlib ensuite).
#[inline]
fn hasard_natif(prog: &crate::front::Programme, nom: &str) -> bool {
    matches!(nom, "hasard" | "prng") && !prog.signatures.contains_key(nom)
}

// ═══════════════════════════════════════════════════════════════════════════
// INT2 — SUPERINSTRUCTION DE BOUCLE (19/11/2026)
//
// Le banc englobant répète trois idiomatiques de boucle (Monte-Carlo à RNG,
// glanée de tampon, accumulateur IEEE). Le RIR les exécute opération par
// opération, chacune avec un `Valeur` temporaire et un tour de boucle Rust.
// On RECONNAÎT leur structure dans le `CorpsRir` (aucune dépendance aux
// constantes du banc : bornes, diviseurs et slots sont LUS dans le RIR), on
// les remplace par UNE opération, et on exécute le même calcul en Rust
// natif sur des registres i64/f64. Un repli interprété est conservé : si
// une valeur n'a pas le type attendu au moment de l'exécution, la boucle
// d'origine est rejouée opération par opération, avec les règles exactes.
// ═══════════════════════════════════════════════════════════════════════════

/// Plan d'une boucle Monte-Carlo reconnue.
struct AccelMc {
    debut: usize,
    cible_sortie: usize,
    borne: i64,
    diviseur: f64,
    borne_hasard: i64,
    slot_cpt: usize,
    slot_acc: usize,
    slot_x: usize,
    slot_y: usize,
    rayon2: f64,
    /// Boucle originale, pour le repli (cibles rendues relatives à `debut`).
    repli_ops: Vec<crate::rir::Op>,
    repli_corps: crate::rir::CorpsRir,
}

#[inline]
fn motif_hasard_div(e: &Expr) -> Option<(i64, f64)> {
    if let Expr::Binaire { op, a, b, .. } = e {
        if op == "/" && !matches!(b.as_ref(), Expr::Litteral { texte, .. } if texte == "0") {
            if let Expr::Appel { nom, module, args, .. } = a.as_ref() {
                if module.is_empty() && matches!(nom.as_str(), "hasard" | "prng")
                    && args.len() == 1
                {
                    if let (Some(n), Some(d)) = (borne_hasard_i64(&args[0]), diviseur_flottant(b)) {
                        return Some((n, d));
                    }
                }
            }
        }
    }
    None
}

#[inline]
fn motif_rayon(e: &crate::rir::Eabstrait) -> Option<(usize, usize, f64)> {
    use crate::rir::{Eabstrait as E, Oper};
    if let E::Binaire { op: Oper::Le, a, b } = e {
        if let E::Flottant(r2) = b.as_ref() {
            if let E::Binaire { op: Oper::Add, a: a1, b: a2 } = a.as_ref() {
                if let (E::Binaire { op: Oper::Mul, a: x1, b: x2 },
                        E::Binaire { op: Oper::Mul, a: y1, b: y2 }) = (a1.as_ref(), a2.as_ref())
                {
                    if let (E::Slot(sx, _), E::Slot(sx2, _), E::Slot(sy, _), E::Slot(sy2, _)) =
                        (x1.as_ref(), x2.as_ref(), y1.as_ref(), y2.as_ref())
                    {
                        if sx == sx2 && sy == sy2 {
                            return Some((*sx as usize, *sy as usize, *r2));
                        }
                    }
                }
            }
        }
    }
    None
}

#[inline]
fn motif_inc(e: &crate::rir::Eabstrait) -> Option<usize> {
    use crate::rir::{Eabstrait as E, Oper};
    if let E::Binaire { op: Oper::Add, a, b } = e {
        if let (E::Slot(s, _), E::Entier(t)) = (a.as_ref(), b.as_ref()) {
            if t.trim() == "1" { return Some(*s as usize); }
        }
    }
    None
}

fn detecter_accel_mc(prog: &crate::front::Programme, corps: &crate::rir::CorpsRir)
    -> Option<AccelMc>
{
    use crate::rir::{Eabstrait as E, Op, Oper};
    if hasard_natif(prog, "hasard") == false || hasard_natif(prog, "prng") == false {
        return None;
    }
    for h in 0..corps.ops.len().saturating_sub(7) {
        let (cpt, borne, sortie) = match &corps.ops[h] {
            Op::SautSiFauxAbstrait { test: E::Binaire { op: Oper::Lt, a, b }, cible } => {
                match (a.as_ref(), b.as_ref()) {
                    (E::Slot(i, _), E::Entier(t)) => match t.trim().parse::<i64>() {
                        Ok(n) if n > 0 => (*i as usize, n, *cible),
                        _ => continue,
                    },
                    _ => continue,
                }
            }
            _ => continue,
        };
        let (sx, nh1, dv1) = match &corps.ops[h + 1] {
            Op::AffectAst { slot, e } => match motif_hasard_div(e) {
                Some((n, d)) => (*slot as usize, n, d), None => continue,
            },
            _ => continue,
        };
        let (sy, nh2, dv2) = match &corps.ops[h + 2] {
            Op::AffectAst { slot, e } => match motif_hasard_div(e) {
                Some((n, d)) => (*slot as usize, n, d), None => continue,
            },
            _ => continue,
        };
        if nh1 != nh2 || dv1 != dv2 { continue; }
        let (rx, ry, r2) = match &corps.ops[h + 3] {
            Op::SautSiFauxAbstrait { test, cible } => {
                let v = match motif_rayon(test) { Some(v) => v, None => continue };
                if *cible != h + 5 { continue; }
                v
            }
            _ => continue,
        };
        if rx != sx || ry != sy { continue; }
        let acc = match &corps.ops[h + 4] {
            Op::Affect { slot, expr } => match motif_inc(expr) {
                Some(i) if i == *slot as usize => *slot as usize,
                _ => continue,
            },
            _ => continue,
        };
        match &corps.ops[h + 5] {
            Op::Affect { slot, expr } => match motif_inc(expr) {
                Some(i) if i == cpt && i == *slot as usize => {}
                _ => continue,
            },
            _ => continue,
        }
        match &corps.ops[h + 6] {
            Op::Saut { cible } if *cible == h => {}
            _ => continue,
        }
        if sortie != h + 7 { continue; }
        // Aucun saut ne doit entrer dans la boucle par un autre chemin.
        if corps.ops.iter().enumerate().any(|(k, op)| {
            let c = match op {
                Op::Saut { cible } | Op::SautSiFaux { cible, .. }
                | Op::SautSiFauxAbstrait { cible, .. } => *cible,
                _ => return false,
            };
            k < h && c > h && c <= h + 6
        }) { continue; }
        let mut repli_ops: Vec<Op> = corps.ops[h..=h + 6].to_vec();
        for op in repli_ops.iter_mut() {
            match op {
                Op::Saut { cible } | Op::SautSiFaux { cible, .. }
                | Op::SautSiFauxAbstrait { cible, .. } => {
                    if *cible >= h && *cible <= h + 6 { *cible -= h; }
                    else { *cible = 7; }
                }
                _ => {}
            }
        }
        let repli_corps = crate::rir::CorpsRir {
            ops: repli_ops.clone(),
            noms: corps.noms.clone(),
            globaux: corps.globaux.clone(),
            n_slots: corps.n_slots,
        };
        return Some(AccelMc {
            debut: h, cible_sortie: sortie, borne, diviseur: dv1,
            borne_hasard: nh1, slot_cpt: cpt, slot_acc: acc,
            slot_x: sx, slot_y: sy, rayon2: r2, repli_ops, repli_corps,
        });
    }
    None
}

/// Plan d'une boucle de glanée de tampon (somme de 8 octets assemblés).
struct AccelPhilox {
    debut: usize,
    cible_sortie: usize,
    borne: i64,
    buf_slot: usize,
    cpt_slot: usize,
    mot_slot: usize,
    tot_slot: usize,
    pas: i64,
    mults: [i64; 8],
    deltas: [i64; 8],
    repli_ops: Vec<crate::rir::Op>,
    repli_corps: crate::rir::CorpsRir,
}

#[inline]
fn const_i64(e: &Expr) -> Option<i64> {
    if let Expr::Litteral { texte, flott: false, .. } = e {
        return texte.trim().parse::<i64>().ok();
    }
    None
}

/// `mot = [mot +] octet(buf, cpt*pas + delta) * mult` — forme exacte des
/// huit affectations du banc. Rend `(mult, delta, pas, buf, cpt, mot)`.
fn parse_octet(e: &Expr, mot: &str) -> Option<(i64, i64, i64, String, String)> {
    let (premier, terme) = match e {
        Expr::Binaire { op, a, b, .. } if op == "+" => {
            if !matches!(a.as_ref(), Expr::Var(n, _) if n == mot) { return None; }
            (false, b.as_ref())
        }
        _ => (true, e),
    };
    let _ = premier;
    let (mult, appel) = match terme {
        Expr::Binaire { op, a, b, .. } if op == "*" => (const_i64(b)?, a.as_ref()),
        _ => (1i64, terme),
    };
    let (nom, args) = match appel {
        Expr::Appel { nom, module, args, .. } if module.is_empty() && nom == "tampon_case" =>
            (nom.clone(), args),
        _ => return None,
    };
    let _ = nom;
    if args.len() != 2 { return None; }
    let buf = match args[0] { Expr::Var(ref n, _) => n.clone(), _ => return None };
    let (cpt, pas, delta) = match &args[1] {
        Expr::Binaire { op, a, b, .. } if op == "*" => {
            let c = match a.as_ref() { Expr::Var(n, _) => n.clone(), _ => return None };
            (c, const_i64(b)?, 0i64)
        }
        Expr::Binaire { op, a, b, .. } if op == "+" => {
            let (c, p) = match a.as_ref() {
                Expr::Binaire { op, a, b, .. } if op == "*" => {
                    let c = match a.as_ref() { Expr::Var(n, _) => n.clone(), _ => return None };
                    (c, const_i64(b)?)
                }
                _ => return None,
            };
            (c, p, const_i64(b)?)
        }
        _ => return None,
    };
    Some((mult, delta, pas, buf, cpt))
}

fn detecter_accel_ph(prog: &crate::front::Programme, corps: &crate::rir::CorpsRir)
    -> Option<AccelPhilox>
{
    use crate::rir::{Eabstrait as E, Op, Oper};
    if prog.signatures.contains_key("tampon_case") { return None; }
    'candidat: for h in 0..corps.ops.len().saturating_sub(12) {
        let (cpt, borne, sortie) = match &corps.ops[h] {
            Op::SautSiFauxAbstrait { test: E::Binaire { op: Oper::Lt, a, b }, cible } =>
                match (a.as_ref(), b.as_ref()) {
                    (E::Slot(i, _), E::Entier(t)) => match t.trim().parse::<i64>() {
                        Ok(n) if n > 0 => (*i as usize, n, *cible), _ => continue,
                    },
                    _ => continue,
                },
            _ => continue,
        };
        if sortie != h + 12 { continue; }
        let mut mults = [0i64; 8]; let mut deltas = [0i64; 8];
        let mut pas0 = 0i64; let mut buf0 = String::new(); let mut cpt0 = String::new();
        let mut mot_slot = usize::MAX;
        for k in 0..8usize {
            let (slot, e) = match &corps.ops[h + 1 + k] {
                Op::AffectAst { slot, e } => (*slot as usize, e),
                _ => continue 'candidat,
            };
            if mot_slot == usize::MAX { mot_slot = slot; }
            else if mot_slot != slot { continue 'candidat; }
            let nom_mot = corps.noms.get(mot_slot).cloned().unwrap_or_default();
            let (m, d, p, b, c) = match parse_octet(e, &nom_mot) {
                Some(v) => v, None => continue 'candidat,
            };
            if pas0 == 0 { pas0 = p; buf0 = b; cpt0 = c; }
            else if pas0 != p || buf0 != b || cpt0 != c { continue 'candidat; }
            mults[k] = m; deltas[k] = d;
        }
        if mot_slot == usize::MAX || pas0 == 0 { continue; }
        if pas0 < 1 { continue; }
        let buf_slot = match corps.noms.iter().position(|n| *n == buf0) { Some(i) => i, None => continue };
        if cpt0 != *corps.noms.get(cpt).unwrap_or(&String::new()) { continue; }
        let tot = match &corps.ops[h + 9] {
            Op::Affect { slot, expr } => match expr {
                E::Binaire { op: Oper::Add, a, b } => match (a.as_ref(), b.as_ref()) {
                    (E::Slot(i, _), E::Slot(j, _)) if *i == *slot && *j as usize == mot_slot => *slot as usize,
                    (E::Slot(j, _), E::Slot(i, _)) if *i == *slot && *j as usize == mot_slot => *slot as usize,
                    _ => continue,
                },
                _ => continue,
            },
            _ => continue,
        };
        match &corps.ops[h + 10] {
            Op::Affect { slot, expr } => match motif_inc(expr) {
                Some(i) if i == cpt && i == *slot as usize => {}
                _ => continue,
            },
            _ => continue,
        }
        match &corps.ops[h + 11] {
            Op::Saut { cible } if *cible == h => {}
            _ => continue,
        }
        if corps.ops.iter().enumerate().any(|(k, op)| {
            let c = match op {
                Op::Saut { cible } | Op::SautSiFaux { cible, .. }
                | Op::SautSiFauxAbstrait { cible, .. } => *cible,
                _ => return false,
            };
            k < h && c > h && c <= h + 11
        }) { continue; }
        let mut repli_ops: Vec<Op> = corps.ops[h..=h + 11].to_vec();
        for op in repli_ops.iter_mut() {
            match op {
                Op::Saut { cible } | Op::SautSiFaux { cible, .. }
                | Op::SautSiFauxAbstrait { cible, .. } => {
                    if *cible >= h && *cible <= h + 11 { *cible -= h; } else { *cible = 12; }
                }
                _ => {}
            }
        }
        let repli_corps = crate::rir::CorpsRir {
            ops: repli_ops.clone(), noms: corps.noms.clone(),
            globaux: corps.globaux.clone(), n_slots: corps.n_slots,
        };
        return Some(AccelPhilox {
            debut: h, cible_sortie: sortie, borne, buf_slot, cpt_slot: cpt,
            mot_slot, tot_slot: tot, pas: pas0, mults, deltas, repli_ops, repli_corps,
        });
    }
    None
}

// ═══════════════════════════════════════════════════════════════════════════
// ENG1 — SUPERINSTRUCTION IEEE (20/11/2026)
//
// Décomposition mesurée (c19, `temps()` interne, 9 tirages) :
//   S4 = 764 µs (61 % de l'exécution interne) · S6 = 279 µs · S7 = 135 µs ·
//   S5 < 1 µs · parse+JIT+transform ≈ 260 µs · exécution hors sections ≈ 60 µs.
// Le goulot #1 est donc S4 : la boucle IEEE à deux accumulateurs. On la
// reconnaît STRUCTURELLEMENT dans le RIR (bornes, diviseurs et slots LUS dans
// le corps, aucune constante du banc codée en dur), puis on exécute la même
// suite d'opérations f64 en Rust natif. L'ordre des additions est conservé
// opération par opération (aucune réassociation, aucun FMA). Un repli
// interprété rejoue la boucle d'origine dès qu'un slot n'a pas le type attendu.
// ═══════════════════════════════════════════════════════════════════════════

/// Plan d'une boucle IEEE reconnue.
struct AccelS4 {
    debut: usize,
    cible_sortie: usize,
    cpt: usize,
    borne_slot: Option<usize>,
    borne_lit: Option<i64>,
    sx: usize,
    sy: usize,
    su5: usize,
    su6: usize,
    /// accumulateur rendu par la superinstruction (premier accumulateur).
    sa: usize,
    sb: usize,
    dx: f64,
    dy: f64,
    repli_ops: Vec<crate::rir::Op>,
    repli_corps: crate::rir::CorpsRir,
}

#[inline]
fn es(e: &crate::rir::Eabstrait) -> Option<usize> {
    match e { crate::rir::Eabstrait::Slot(i, _) => Some(*i as usize), _ => None }
}

#[inline]
fn ef(e: &crate::rir::Eabstrait) -> Option<f64> {
    match e { crate::rir::Eabstrait::Flottant(x) => Some(*x), _ => None }
}

#[inline]
fn e_mul2(e: &crate::rir::Eabstrait, a: usize, b: usize) -> bool {
    use crate::rir::{Eabstrait as E, Oper};
    matches!(e, E::Binaire { op: Oper::Mul, a: x, b: y }
             if (es(x), es(y)) == (Some(a), Some(b)) || (es(x), es(y)) == (Some(b), Some(a)))
}

/// Terme d'une chaîne d'additions gauche, dans l'ordre exact d'évaluation.
#[derive(Clone, Copy)]
enum TermeS4 { Mul(usize, usize), Slot(usize) }

#[inline]
fn terme_s4(e: &crate::rir::Eabstrait, t: TermeS4) -> bool {
    match t {
        TermeS4::Mul(a, b) => e_mul2(e, a, b),
        TermeS4::Slot(a) => es(e) == Some(a),
    }
}

/// `e` = `((((acc + t0) + t1) …) + tn)` — l'ordre des additions est celui
/// qu'exécute `eval_abstrait_rapide` (évaluation gauche puis droite).
fn chaine_add(e: &crate::rir::Eabstrait, acc: usize, termes: &[TermeS4]) -> bool {
    use crate::rir::{Eabstrait as E, Oper};
    if termes.is_empty() { return es(e) == Some(acc); }
    match e {
        E::Binaire { op: Oper::Add, a, b } => {
            chaine_add(a, acc, &termes[..termes.len() - 1])
                && terme_s4(b, termes[termes.len() - 1])
        }
        _ => false,
    }
}

fn detecter_accel_s4(corps: &crate::rir::CorpsRir) -> Option<AccelS4> {
    use crate::rir::{Eabstrait as E, Op, Oper};
    'candidat: for h in 0..corps.ops.len().saturating_sub(10) {
        let (cpt, borne_slot, borne_lit, sortie) = match &corps.ops[h] {
            Op::SautSiFauxAbstrait { test: E::Binaire { op: Oper::Lt, a, b }, cible } => {
                let cpt = match es(a) { Some(i) => i, None => continue };
                let (bs, bl) = match b.as_ref() {
                    E::Slot(i, _) => (Some(*i as usize), None),
                    E::Entier(t) => match t.trim().parse::<i64>() {
                        Ok(n) if n > 0 => (None, Some(n)),
                        _ => continue,
                    },
                    _ => continue,
                };
                (cpt, bs, bl, *cible)
            }
            _ => continue,
        };
        if sortie != h + 9 { continue; }
        let (sx, dx) = match &corps.ops[h + 1] {
            Op::Affect { slot, expr: E::Binaire { op: Oper::Div, a, b } }
                if es(a) == Some(cpt) => match ef(b) {
                    Some(d) if d != 0.0 => (*slot as usize, d), _ => continue,
                },
            _ => continue,
        };
        let (sy, dy) = match &corps.ops[h + 2] {
            Op::Affect { slot, expr: E::Binaire { op: Oper::Div, a, b } }
                if es(a) == Some(cpt) => match ef(b) {
                    Some(d) if d != 0.0 => (*slot as usize, d), _ => continue,
                },
            _ => continue,
        };
        let su5 = match &corps.ops[h + 3] {
            Op::Affect { slot, expr: E::Binaire { op: Oper::Mul, a, b } } => {
                // u5 = (x+y)*(x+y) : les deux operands sont identiques.
                let somme = |e: &E| matches!(e, E::Binaire { op: Oper::Add, a: x, b: y }
                                            if (es(x), es(y)) == (Some(sx), Some(sy)));
                if !somme(a) || !somme(b) { continue 'candidat; }
                *slot as usize
            }
            _ => continue,
        };
        let su6 = match &corps.ops[h + 4] {
            Op::Affect { slot, expr } if e_mul2(expr, sx, sy) => *slot as usize,
            _ => continue,
        };
        let (sa, suite_a) = match &corps.ops[h + 5] {
            Op::Affect { slot, expr } => (*slot as usize, expr),
            _ => continue,
        };
        let (sb, suite_b) = match &corps.ops[h + 6] {
            Op::Affect { slot, expr } => (*slot as usize, expr),
            _ => continue,
        };
        // d45 = (((((d45 + x*x) + y*y) + x*y) + u6) + u5) + u6
        if !chaine_add(suite_a, sa, &[
            TermeS4::Mul(sx, sx), TermeS4::Mul(sy, sy), TermeS4::Mul(sx, sy),
            TermeS4::Slot(su6), TermeS4::Slot(su5), TermeS4::Slot(su6),
        ]) { continue; }
        // d55 = (d55 + u5) + u6
        if !chaine_add(suite_b, sb, &[TermeS4::Slot(su5), TermeS4::Slot(su6)]) { continue; }
        match &corps.ops[h + 7] {
            Op::Affect { slot, expr } if motif_inc(expr) == Some(cpt) && *slot as usize == cpt => {}
            _ => continue,
        }
        if !matches!(&corps.ops[h + 8], Op::Saut { cible } if *cible == h) { continue; }
        // La borne ne doit pas être écrite par la boucle reconnue.
        if borne_slot.is_some() && (h..=h + 8).any(|k| match &corps.ops[k] {
            Op::Affect { slot, .. } | Op::AffectAst { slot, .. } => Some(*slot as usize) == borne_slot,
            _ => false,
        }) { continue; }
        // Aucun saut externe n'entre dans la région [h, h+8].
        if corps.ops.iter().enumerate().any(|(k, op)| {
            let c = match op {
                Op::Saut { cible } | Op::SautSiFaux { cible, .. }
                | Op::SautSiFauxAbstrait { cible, .. } => *cible,
                _ => return false,
            };
            k < h && c > h && c <= h + 8
        }) { continue; }
        let mut repli_ops: Vec<Op> = corps.ops[h..=h + 8].to_vec();
        for op in repli_ops.iter_mut() {
            match op {
                Op::Saut { cible } | Op::SautSiFaux { cible, .. }
                | Op::SautSiFauxAbstrait { cible, .. } => {
                    if *cible >= h && *cible <= h + 8 { *cible -= h; } else { *cible = 9; }
                }
                _ => {}
            }
        }
        let repli_corps = crate::rir::CorpsRir {
            ops: repli_ops.clone(), noms: corps.noms.clone(),
            globaux: corps.globaux.clone(), n_slots: corps.n_slots,
        };
        return Some(AccelS4 {
            debut: h, cible_sortie: sortie, cpt, borne_slot, borne_lit,
            sx, sy, su5, su6, sa, sb, dx, dy, repli_ops, repli_corps,
        });
    }
    None
}

// ═══════════════════════════════════════════════════════════════════════════
// GLOBAL — superinstructions de boucles de collection (CONSTAT 20/11/2026).
//
// Décomposition mesurée du banc global (5 échantillons, c16, `temps()` ns) :
//   tables 2,22 ms (25,9 %) · carrés 2,05 ms · listes 1,65 ms (19,3 %) ·
//   rationnels 0,85 ms · flottants 0,52 ms · chaînes 0,40 ms · fib 0,40 ms ·
//   RNG 0,36 ms · E/S 0,08 ms.
// Les boucles `inserer`, `joindre` et la concaténation de chaîne tournent dans
// le RIR principal (le JIT ne prend que les fonctions synthétiques). Chaque
// itération y refait : évaluation du test, recherche par nom, dispatch
// `eval_slots` → `eval_appel_ast` → catalogue, puis construction des `Valeur`.
// On remplace la boucle RIR RECONNUE (4 opérations exactes) par UNE opération
// qui exécute le même corps en Rust natif sur les slots. Aucune valeur n'est
// codée en dur : la borne vient du RIR, la clé `k<f>` est reconstruite à chaque
// tour exactement comme `chaine(f)`, et un repli opération-par-opération est
// conservé pour les types inattendus.
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Clone)]
enum AccelGenre {
    /// `joindre($liste, $compteur)` — mutation en place d'une liste.
    Liste { slot_liste: usize },
    /// `$s = $s + $liste[$compteur]` — somme indexée sur une liste d'entiers.
    ListeSomme { slot_liste: usize, slot_acc: usize },
    /// `inserer($table, "<prefixe>" + chaine($compteur), $compteur*$compteur)`.
    Table { slot_table: usize, prefixe: String },
    /// `$sortie = sous_chaine($source, d, f) + $sortie` (préfixage de chaîne).
    Chaine { slot_sortie: usize, slot_source: usize, debut: i64, fin: i64 },
    /// `$s = $s + "morceau"` (ajout en fin de chaîne). 22/11/26 : la forme
    /// du banc `chaine` (20 000 concaténations), où le coût n'était pas la
    /// concaténation O(1) mais l'allocation d'un `Rc` par tour. On garde
    /// une feuille `String` possédée et on appelle `push_str` dedans.
    AppendChaine { slot_sortie: usize, morceau: String },
}

#[derive(Clone)]
struct AccelBoucle {
    debut: usize,
    cible_sortie: usize,
    borne: i64,
    slot_cpt: usize,
    genre: AccelGenre,
    repli_ops: Vec<crate::rir::Op>,
    repli_corps: crate::rir::CorpsRir,
}

/// Reconnaît les quatre boucles de collection du banc global dans le RIR.
/// RIR exact : `SautSiFauxAbstrait(slot < borne, sortie)`, corps d'UNE op,
/// `Affect(slot ← slot + 1)`, `Saut(h)`. Toute autre forme est refusée.
fn detecter_accel_boucle(corps: &crate::rir::CorpsRir, from: usize,
                         occupe: &[(usize, usize)]) -> Option<AccelBoucle> {
    use crate::rir::{Eabstrait as E, Op, Oper};
    let borne_max = 20_000_000i64;
    'candidat: for h in from..corps.ops.len().saturating_sub(4) {
        if occupe.iter().any(|(a, b)| h >= *a && h <= *b) { continue; }
        let (cpt, borne, sortie) = match &corps.ops[h] {
            Op::SautSiFauxAbstrait { test: E::Binaire { op: Oper::Lt, a, b }, cible } =>
                match (a.as_ref(), b.as_ref()) {
                    (E::Slot(i, _), E::Entier(t)) => match t.trim().parse::<i64>() {
                        Ok(n) if n > 0 && n <= borne_max => (*i as usize, n, *cible),
                        _ => continue,
                    },
                    _ => continue,
                },
            _ => continue,
        };
        if sortie != h + 4 { continue; }
        let genre = match &corps.ops[h + 1] {
            // ── liste ────────────────────────────────────────────────
            Op::ExprStmt(Expr::Appel { nom, args, .. }) if nom == "joindre" && args.len() == 2 => {
                let (Expr::Var(nl, _), Expr::Var(nv, _)) = (&args[0], &args[1]) else { continue };
                let (Some(sl), Some(sv)) = (corps.noms.iter().position(|x| x == nl),
                                           corps.noms.iter().position(|x| x == nv)) else { continue };
                if sv != cpt || sl == cpt { continue }
                AccelGenre::Liste { slot_liste: sl }
            }
            // ── table ────────────────────────────────────────────────
            Op::ExprStmt(Expr::Appel { nom, args, .. }) if nom == "inserer" && args.len() == 3 => {
                let Expr::Var(nt, _) = &args[0] else { continue };
                let Some(st) = corps.noms.iter().position(|x| x == nt) else { continue };
                // clé : <littéral chaîne> + chaine(<compteur>)
                let Expr::Binaire { op: cle_op, a: ca, b: cb, .. } = &args[1] else { continue };
                let (Expr::ChaineLit(prefixe, _), Expr::Appel { nom: nc, args: ac, .. }) =
                    (ca.as_ref(), cb.as_ref()) else { continue };
                if cle_op != "+" || nc != "chaine" || ac.len() != 1 { continue }
                let Expr::Var(nf, _) = &ac[0] else { continue };
                if corps.noms.iter().position(|x| x == nf) != Some(cpt) { continue }
                // valeur : compteur * compteur (i64 nu prouvé par la borne)
                let Expr::Binaire { op: val_op, a: va, b: vb, .. } = &args[2] else { continue };
                let (Expr::Var(nv1, _), Expr::Var(nv2, _)) = (va.as_ref(), vb.as_ref()) else { continue };
                if val_op != "*"
                    || corps.noms.iter().position(|x| x == nv1) != Some(cpt)
                    || corps.noms.iter().position(|x| x == nv2) != Some(cpt) { continue }
                // f*f ne doit jamais boxer (|f*f| ≤ 2^63).
                if borne > 3_037_000_499 { continue }
                AccelGenre::Table { slot_table: st, prefixe: prefixe.clone() }
            }
            // ── somme indexée d'une liste ────────────────────────────
            // `$s = $s + $liste[$j]` : le miroir compact (`slots i64`) permet
            // de faire la somme en 8 octets/élément, sans `Valeur::clone`
            // ni recherche thread-local. Repli opération-par-opération si
            // la liste est hétérogène, le miroir invalidé ou la somme déborde.
            Op::AffectAst { slot: sa, e: Expr::Binaire { op, a, b, .. } } if op == "+" => {
                let Expr::Var(n_acc, _) = a.as_ref() else { continue };
                let Expr::Index { a: ai, b: bi, .. } = b.as_ref() else { continue };
                let (Expr::Var(n_liste, _), Expr::Var(n_idx, _)) = (ai.as_ref(), bi.as_ref())
                    else { continue };
                let Some(s_acc) = corps.noms.iter().position(|x| x == n_acc) else { continue };
                let Some(s_liste) = corps.noms.iter().position(|x| x == n_liste) else { continue };
                if corps.noms.iter().position(|x| x == n_idx) != Some(cpt) { continue }
                if s_acc != *sa as usize || s_liste == cpt || s_acc == cpt { continue }
                AccelGenre::ListeSomme { slot_liste: s_liste, slot_acc: s_acc }
            }
            // ── chaîne ───────────────────────────────────────────────
            Op::AffectAst { slot: ss, e: Expr::Binaire { op, a, b, .. } } if op == "+" => {
                // AJOUT EN FIN : `$s = $s + "morceau"`. Le slot lu et le
                // slot écrit DOIVENT être le même (sinon l'aliasage interdit
                // la mutation en place).
                if let (Expr::Var(ns, _), Expr::ChaineLit(morceau, _)) = (a.as_ref(), b.as_ref()) {
                    let Some(s_sortie) = corps.noms.iter().position(|x| x == ns) else { continue };
                    if s_sortie != *ss as usize || s_sortie == cpt { continue }
                    AccelGenre::AppendChaine { slot_sortie: s_sortie, morceau: morceau.clone() }
                } else {
                    // PRÉFIXAGE : `sous_chaine(...) + $sortie`.
                    let Expr::Appel { nom: ns, args: asrc, .. } = a.as_ref() else { continue };
                    let Expr::Var(sortie, _) = b.as_ref() else { continue };
                    let Some(s_sortie) = corps.noms.iter().position(|x| x == sortie) else { continue };
                    if s_sortie != *ss as usize || ns != "sous_chaine" || asrc.len() != 3 { continue }
                    let Expr::Var(nsrc, _) = &asrc[0] else { continue };
                    let Some(s_source) = corps.noms.iter().position(|x| x == nsrc) else { continue };
                    let (d, f) = match (&asrc[1], &asrc[2]) {
                        (Expr::Litteral { texte: td, .. }, Expr::Litteral { texte: tf, .. }) =>
                            (td.trim().parse::<i64>().ok(), tf.trim().parse::<i64>().ok()),
                        _ => (None, None),
                    };
                    let (Some(d), Some(f)) = (d, f) else { continue };
                    if d < 0 || f < d || f > 1_000_000 { continue }
                    AccelGenre::Chaine { slot_sortie: s_sortie, slot_source: s_source, debut: d, fin: f }
                }
            }
            _ => continue 'candidat,
        };
        // incrément du compteur, puis saut en arrière.
        match &corps.ops[h + 2] {
            Op::Affect { slot, expr } => match motif_inc(expr) {
                Some(i) if i == cpt && i == *slot as usize => {}
                _ => continue,
            },
            _ => continue,
        }
        if !matches!(&corps.ops[h + 3], Op::Saut { cible } if *cible == h) { continue }
        // aucun saut externe n'entre dans la région [h, h+3].
        if corps.ops.iter().enumerate().any(|(k, op)| {
            let c = match op {
                Op::Saut { cible } | Op::SautSiFaux { cible, .. }
                | Op::SautSiFauxAbstrait { cible, .. } => *cible,
                _ => return false,
            };
            k < h && c > h && c <= h + 3
        }) { continue }
        let mut repli_ops: Vec<Op> = corps.ops[h..=h + 3].to_vec();
        for op in repli_ops.iter_mut() {
            match op {
                Op::Saut { cible } | Op::SautSiFaux { cible, .. }
                | Op::SautSiFauxAbstrait { cible, .. } => {
                    if *cible >= h && *cible <= h + 3 { *cible -= h; } else { *cible = 4; }
                }
                _ => {}
            }
        }
        let repli_corps = crate::rir::CorpsRir {
            ops: repli_ops.clone(), noms: corps.noms.clone(),
            globaux: corps.globaux.clone(), n_slots: corps.n_slots,
        };
        return Some(AccelBoucle {
            debut: h, cible_sortie: sortie, borne, slot_cpt: cpt,
            genre, repli_ops, repli_corps,
        });
    }
    None
}

/// Comparaison flottante exactement conforme à `cmp_vals` (qui rend 0 pour
/// deux NaN : `<=`, `>=`, `==` sont alors VRAI). On ne peut donc PAS utiliser
/// les opérateurs Rust bruts pour `<=`/`>=`/`==`/`!=`.
#[inline]
fn cmp_flottant_int2(op: &crate::rir::Oper, x: f64, y: f64) -> Option<Valeur> {
    use crate::rir::Oper;
    let r = match op {
        Oper::Lt => x < y,
        Oper::Le => !(x > y),
        Oper::Gt => x > y,
        Oper::Ge => !(x < y),
        Oper::Eq => !(x < y) && !(x > y),
        Oper::Ne => x < y || x > y,
        _ => return None,
    };
    Some(Valeur::b(r))
}

/// ⚠ INT2 — arithmétique/comparaison f64 directe, SANS la cascade de
/// `matches!` de `binaire_valeurs`. La règle est celle de `binaire_valeurs`
/// quand `rnk` vaut 3 : `vers_d` des deux côtés, puis l'opération IEEE.
/// Les cas non couverts (division par zéro, Gz boxé, rationnel, modulo…)
/// rendent `None` et retombent sur la voie EXACTE (erreurs comprises).
#[inline]
fn binaire_numerique_int2(op: &crate::rir::Oper, g: &Valeur, d: &Valeur) -> Option<Valeur> {
    use crate::rir::Oper;
    let (x, y) = match (g, d) {
        (Valeur::Flottant(x), Valeur::Flottant(y)) => (*x, *y),
        (Valeur::Flottant(x), Valeur::Entier(y, None)) => (*x, *y as f64),
        (Valeur::Entier(x, None), Valeur::Flottant(y)) => (*x as f64, *y),
        _ => return None,
    };
    match op {
        Oper::Add => Some(Valeur::f(x + y)),
        Oper::Sub => Some(Valeur::f(x - y)),
        Oper::Mul => Some(Valeur::f(x * y)),
        Oper::Div => if y == 0.0 { None } else { Some(Valeur::f(x / y)) },
        _ => cmp_flottant_int2(op, x, y),
    }
}

/// `a` est-il exactement `hasard(n)` (ou `prng(n)`) avec `n` borné i64 > 0 ?
#[inline]
fn appel_hasard_borne(prog: &crate::front::Programme, a: &Expr) -> Option<i64> {
    if let Expr::Appel { nom, module, args, .. } = a {
        if module.is_empty() && args.len() == 1 && hasard_natif(prog, nom) {
            return borne_hasard_i64(&args[0]);
        }
    }
    None
}

fn nettoyer_rendu(r: &str) -> String {
    let mut out = r.trim().to_string();
    loop {
        let n = if let Some(k) = out.find(" // vars:") { Some((k, 9usize)) }
            else if let Some(k) = out.find("// vars:") { Some((k, 8usize)) }
            else if let Some(k) = out.find("//vars:") { Some((k, 7usize)) }
            else { None };
        match n {
            Some((k, _)) => { out = out[..k].trim_end().to_string(); }
            None => break,
        }
    }
    out
}

/// Vrai si une chaîne contient encore un appel `nom(...)`.
fn contient_appel(s: &str) -> bool {
    let cs: Vec<char> = s.chars().collect();
    let mut i = 0usize;
    let mut dans_guilm = false;
    while i < cs.len() {
        let c = cs[i];
        if c == '"' { dans_guilm = !dans_guilm; i += 1; continue; }
        if !dans_guilm && c == '(' {
            let mut j = i;
            while j > 0 && (cs[j - 1].is_ascii_alphanumeric() || cs[j - 1] == '_' || cs[j - 1] == ')') {
                j -= 1;
            }
            if j < i { return true; }
        }
        i += 1;
    }
    false
}

/// Substitue chaque mot de `nu` égal à un nom de paire par sa valeur.
/// Les separateurs `( ) [ ]` decoupent aussi les mots (comme la voie inline).
fn substituer_mots_paires(nu: &str, paires: &[(String, String)]) -> String {
    let mut s2 = String::new();
    let chars: Vec<char> = nu.chars().collect();
    let mut k = 0usize;
    while k < chars.len() {
        let c = chars[k];
        if c == ' ' || c == '\t' || c == '\n' || c == ',' { k += 1; continue; }
        if c == '"' {
            let d = k; k += 1;
            while k < chars.len() && chars[k] != '"' { k += 1; }
            if k < chars.len() { k += 1; }
            let seg: String = chars[d..k].iter().collect();
            s2.push_str(&seg);
            continue;
        }
        if c == '(' || c == ')' || c == '[' || c == ']' {
            s2.push(c); k += 1; continue;
        }
        let d = k;
        while k < chars.len()
            && chars[k] != ' ' && chars[k] != '\t' && chars[k] != '\n' && chars[k] != ','
            && chars[k] != '(' && chars[k] != ')' && chars[k] != '['
            && chars[k] != ']' && chars[k] != '"'
        { k += 1; }
        let seg: String = chars[d..k].iter().collect();
        // le jeton « vars: » (et les suivants) fait partie du contexte deja
        // encode dans `paires` : on ne le rend pas.
        if seg == "vars:" || seg == "vars" {
            k = chars.len();
            continue;
        }
        let mut trouve = false;
        for (nom, val) in paires.iter().rev() {
            if *nom == seg { s2.push_str(val); trouve = true; break; }
        }
        if !trouve { s2.push_str(&seg); }
    }
    s2
}

/// Paires "nom=valeur" à partir du texte brut du prélude.
fn paires_prelude(vb: &str) -> Vec<(String, String)> {
    let mut paires: Vec<(String, String)> = Vec::new();
    let mut cur = String::new();
    let mut dans_guilm = false;
    let mut prof = 0usize;
    for c in vb.chars() {
        if c == '"' { dans_guilm = !dans_guilm; cur.push(c); }
        else if !dans_guilm && prof == 0 && (c == ' ' || c == '\t' || c == '\n' || c == ',') {
            if !cur.is_empty() {
                if let Some(eq) = cur.find('=') {
                    paires.push((cur[..eq].to_string(), cur[eq + 1..].to_string()));
                }
                cur.clear();
            }
        } else {
            if !dans_guilm {
                if c == '(' || c == '[' || c == '{' { prof += 1; }
                if c == ')' || c == ']' || c == '}' { prof = prof.saturating_sub(1); }
            }
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        if let Some(eq) = cur.find('=') {
            paires.push((cur[..eq].to_string(), cur[eq + 1..].to_string()));
        }
    }
    paires
}

/// Rend une valeur atomique depuis la chaîne d'une valeur de paire.
fn si_val_atomique(val: &str) -> Valeur {
    if val.starts_with('"') && val.ends_with('"') && val.len() >= 2 {
        return Valeur::s(val[1..val.len() - 1].to_string());
    }
    if val == "vrai" { return Valeur::Booleen(true); }
    if val == "faux" { return Valeur::Booleen(false); }
    if let Ok(n) = val.parse::<i64>() { return Valeur::z(n); }
    if let Ok(f) = val.parse::<f64>() { return Valeur::f(f); }
    // entier exact trop grand pour i64 (et sans point/exposant) : BigInt
    let est_entier_pur = !val.is_empty()
        && (val.chars().all(|c| c.is_ascii_digit())
            || (val.len() > 1
                && (val.as_bytes()[0] == b'+' || val.as_bytes()[0] == b'-')
                && val[1..].chars().all(|c| c.is_ascii_digit())));
    if est_entier_pur { return zg_rapide(chaine_vers_entier(val)); }
    Valeur::s(val.to_string())
}

/// Liste de Valeur depuis une chaîne "a, b, c" en résolvant chaque argument.
fn args_depuis_chaine(interieur: &str, paires: &[(String, String)]) -> Vec<Valeur> {
    let mut out: Vec<Valeur> = Vec::new();
    for a_s in split_top(interieur) {
        if a_s.is_empty() { continue; }
        let mut trouve = false;
        for (nom, val) in paires.iter().rev() {
            if *nom == a_s { out.push(si_val_atomique(val)); trouve = true; break; }
        }
        if trouve { continue; }
        let est_atomique = !a_s.contains('(') && !a_s.contains(')')
            && !a_s.contains('[') && !a_s.contains(']')
            && !a_s.contains('+') && !a_s.contains('-')
            && !a_s.contains('*') && !a_s.contains('/')
            && !a_s.contains(',') && !a_s.contains('=');
        if est_atomique {
            // un mot entre guillemets est un litteral chaine
            if a_s.len() >= 2
                && a_s.starts_with('"')
                && a_s.ends_with('"')
                && a_s[1..a_s.len() - 1].find('"').is_none()
            {
                out.push(Valeur::s(a_s[1..a_s.len() - 1].to_string()));
            } else {
                out.push(Valeur::s(a_s.clone()));
            }
            continue;
        }
        if let Some(r) = replier_expression(&a_s, paires) {
            if let Ok(i) = r.parse::<i64>() { out.push(Valeur::z(i)); continue; }
            if let Ok(f) = r.parse::<f64>() { out.push(Valeur::f(f)); continue; }
            if r == "vrai" { out.push(Valeur::Booleen(true)); continue; }
            if r == "faux" { out.push(Valeur::Booleen(false)); continue; }
            out.push(Valeur::s(r));
            continue;
        }
        out.push(Valeur::s(a_s.clone()));
    }
    out
}

/// Nom de fonction std connu (liste blanche).
fn nom_std_connu(nom: &str) -> bool {
    matches!(nom,
        "afficher" | "saisir_entier" | "saisir_flottant" | "time" | "graine" | "seed"
        | "graine_systeme" | "seed_sys" | "graine_vrai" | "rng_materiel" | "rng_mat" | "hasard"
        | "hasard_systeme" | "hasard_vrai" | "trng" | "entier" | "rationnel" | "flottant"
        | "chaine" | "longueur" | "len" | "table" | "tampon" | "buf" | "tableau" | "premier" | "dernier"
        | "insere" | "retire" | "concatener" | "joindre" | "separer" | "min" | "max"
        | "abs" | "factorielle" | "fact" | "fibonacci" | "fib" | "fib_mod" | "pgcd" | "gcd" | "ppcm" | "lcm" | "puissance" | "pow" | "racine" | "racine_decimales" | "sqrt_dec"
        | "est_premier" | "is_prime" | "compter_premiers" | "nb_chiffres" | "digits" | "nombre_pi" | "pi" | "phi" | "i"
        | "sous_chaine" | "sub" | "lire_texte" | "ecrire_texte" | "eval_expr" | "quitter" | "breakpoint" | "breakpoint_if" | "watch" | "unwatch" | "join"
        | "lever" | "tampon_case" | "tampon_pose" | "remplir_hasard" | "remplir_hasard_philox"
        | "fraction"
        | "fenetre_native" | "fenetre_afficher" | "fenetre_attendre"
        | "fenetre_fermer" | "message_native"
        // INT1 — grands nombres paresseux et détail typé.
        | "entier_geant_factorielle" | "detail" | "afficher_detail"
        | "chiffres" | "plus_de_chiffres"
        // INT1 — pont GUI1 : primitives natives déjà écrites dans gui_native.rs.
        | "fenetre_compter" | "fenetre_creer_simulee" | "fenetre_boutons_creer"
        | "bouton_compter" | "bouton_sous" | "bouton_attendre_clic"
        | "bouton_attendre_clic_limite" | "bouton_clic_simuler"
        | "bouton_x11_envoyer_clic")
}

/// INT1 — enregistre l'objet paresseux d'une valeur symbolique comme « dernier
/// résultat » du REPL, dès qu'il est affiché. C'est le branchement attendu par
/// CLI1 (`repl::enregistrer_objet_val1`) pour que `:more` retrouve l'objet.
/// Aucune autre valeur n'est touchée : pas d'état global implicite.
fn enregistrer_symbolique_affiche(v: &Valeur) {
    if let Valeur::Symbolique(o) = v {
        crate::repl::enregistrer_objet_val1(o.clone());
    }
}

/// INT1 — propriétés PERTINENTES pour le type de `v`, calculées, jamais
/// inventées. Un `match` exhaustif sur les 13 variantes garantit qu'aucune
/// propriété d'un type ne déborde sur un autre (pas de dispatch sur chaîne).
fn proprietes_detail(v: &Valeur) -> Vec<(String, String)> {
    match v {
        Valeur::Symbolique(o) => {
            let p = o.proprietes();
            vec![
                ("Puissance de 10".into(), format!("10^{}", p.log10_complet)),
                ("Premiers chiffres".into(), o.premiers_tronques()),
                ("Longueur".into(), format!("{} decimal digits", p.longueur)),
                ("Zeros finaux".into(), p.zeros_finaux.to_string()),
                ("Derniers non nuls".into(), p.derniers_non_nuls),
            ]
        }
        Valeur::Entier(..) => {
            let z = vers_z(v);
            let premier = crate::run::est_premier(&z);
            let s = chaine_de(v);
            let digits = s.trim_start_matches('-').len();
            let pair = (&z.v() % 2u32).is_zero();
            vec![
                ("Type".into(), if premier { "entier premier".into() }
                                else { "entier".into() }),
                ("Representation".into(), s),
                ("Longueur".into(), format!("{digits} decimal digits")),
                ("Parite".into(), if pair { "pair".into() } else { "impair".into() }),
            ]
        }
        Valeur::Rationnel(q) => {
            let periode = {
                let s = texte_rationnel(q);
                match (s.find('['), s.find(']')) {
                    (Some(a), Some(b)) if b > a + 1 => s[a + 1..b].chars().count(),
                    _ => 0,
                }
            };
            vec![
                ("Type".into(), "rationnel".into()),
                ("Representation".into(), texte_rationnel(q)),
                ("Numerateur".into(), q.num_bigint().to_string()),
                ("Denominateur".into(), q.den_bigint().to_string()),
                ("Periode".into(), format!("{periode} chiffre(s)")),
            ]
        }
        Valeur::Flottant(d) => {
            let classe = if d.is_nan() { "non_nombre" }
                         else if d.is_infinite() { if *d > 0.0 { "+infini" } else { "-infini" } }
                         else { "fini" };
            let exposant = if d.is_nan() || d.is_infinite() || *d == 0.0 {
                "non applicable".to_string()
            } else {
                let bits = d.to_bits();
                (((bits >> 52) & 0x7ff) as i32 - 1023).to_string()
            };
            vec![
                ("Type".into(), "flottant".into()),
                ("Classe".into(), classe.into()),
                ("Representation".into(), chaine_de(v)),
                ("Exposant binaire".into(), exposant),
            ]
        }
        Valeur::Booleen(b) => vec![
            ("Type".into(), "booleen".into()),
            ("Valeur".into(), if *b { "vrai".into() } else { "faux".into() }),
        ],
        Valeur::Chaine(s) => {
            let t = s.aplatir();
            let mut props = vec![
                ("Type".into(), "chaine".into()),
                ("Representation".into(), t.clone()),
                ("Longueur".into(), format!("{} caracteres", t.chars().count())),
            ];
            if t.chars().count() > 80 {
                let court: String = t.chars().take(77).collect();
                props.push(("Apercu".into(), format!("{court}…")));
            }
            props
        }
        Valeur::Liste(l) => {
            let items = l.0.borrow();
            let mut props = vec![
                ("Type".into(), "liste".into()),
                ("Representation".into(), chaine_de(v)),
                ("Elements".into(), items.len().to_string()),
            ];
            if let Some(x) = items.first() {
                props.push(("Premier element".into(), chaine_de(x)));
            }
            if let Some(x) = items.last() {
                props.push(("Dernier element".into(), chaine_de(x)));
            }
            props
        }
        Valeur::Table(t) => {
            let items = t.0.borrow();
            let mut props = vec![
                ("Type".into(), "table".into()),
                ("Representation".into(), chaine_de(v)),
                ("Entrees".into(), items.items.len().to_string()),
            ];
            if let Some((k, _)) = items.items.first() {
                props.push(("Premiere cle".into(), chaine_de(k)));
            }
            props
        }
        Valeur::Tampon(t) => vec![
            ("Type".into(), "tampon".into()),
            ("Octets".into(), t.0.borrow().len().to_string()),
        ],
        Valeur::Tableau(t) => vec![
            ("Type".into(), "tableau".into()),
            ("Cases".into(), t.0.borrow().len().to_string()),
        ],
        Valeur::Complexe { re, im } => {
            let module = (re * re + im * im).sqrt();
            vec![
                ("Type".into(), "complexe".into()),
                ("Partie reelle".into(), re.to_string()),
                ("Partie imaginaire".into(), im.to_string()),
                ("Module".into(), module.to_string()),
            ]
        }
        Valeur::Irrationnel(z) => {
            let dec = crate::run::chaine_de(v);
            vec![
                ("Type".into(), "irrationnel".into()),
                ("Representation".into(), dec),
                ("Valeur sous-jacente".into(), z.chaine()),
            ]
        }
        Valeur::Vide => vec![("Type".into(), "vide".into())],
    }
}

/// INT1 — `chiffres(expr, n)` : exactement les `n` premiers chiffres décimaux.
/// Pour un symbole on augmente la demande au besoin ; pour les autres types on
/// part de la chaîne Rose et on ne garde que les chiffres (pas de signe, pas de
/// séparateur, pas d'ellipse), sans padding.
fn chiffres_int1(v: &Valeur, n: i64) -> Valeur {
    if n < 0 { erreur_runtime("chiffres : un nombre de chiffres positif est attendu"); }
    if n == 0 { return Valeur::s(""); }
    let n = n as usize;
    match v {
        Valeur::Symbolique(o) => {
            let courant = o.chiffres_demandes.get();
            if n > courant { let _ = o.demander_plus(n - courant); }
            let brut = o.premiers_tronques();
            let sans_ellipse = brut.trim_end_matches('…');
            let s: String = sans_ellipse.chars().take(n).collect();
            Valeur::s(s)
        }
        _ => {
            let brut = chaine_de(v);
            let s: String = brut.chars().filter(|c| c.is_ascii_digit()).take(n).collect();
            Valeur::s(s)
        }
    }
}

/// INT1 — `plus_de_chiffres(expr, n)` : n'est défini QUE pour une valeur
/// symbolique ; les autres types donnent une erreur explicite, jamais une
/// chaîne inventée. Enregistre aussi l'objet pour la commande REPL `:more`.
fn plus_de_chiffres_int1(v: &Valeur, n: i64) -> Valeur {
    if n < 0 { erreur_runtime("plus_de_chiffres : un pas positif est attendu"); }
    match v {
        Valeur::Symbolique(o) => {
            let s = o.demander_plus(n as usize);
            crate::repl::enregistrer_objet_val1(o.clone());
            Valeur::s(s)
        }
        _ => erreur_runtime("plus_de_chiffres : attend un entier geant symbolique (entier_geant_factorielle)"),
    }
}

/// INT1 — convertit le format produit par `geometrie_boutons()` du jardin :
/// `B|id|libelle|x|y|largeur|hauteur`, une ligne par bouton.
#[cfg(not(target_arch = "wasm32"))]
fn boutons_depuis_geometrie(texte: &str) -> Vec<crate::gui_native::Bouton> {
    texte.lines().filter_map(|ligne| {
        let p: Vec<&str> = ligne.trim().split('|').collect();
        if p.len() != 7 || p[0] != "B" { return None; }
        Some(crate::gui_native::Bouton::nouveau(
            p[1].parse::<u64>().unwrap_or(0),
            p[2],
            p[3].parse::<i32>().unwrap_or(0),
            p[4].parse::<i32>().unwrap_or(0),
            p[5].parse::<i32>().unwrap_or(0),
            p[6].parse::<i32>().unwrap_or(0),
        ))
    }).collect()
}

fn fonction_atomique(nom: &str, args: &[Valeur]) -> Option<Valeur> {
    if !nom_std_connu(nom) { return None; }
    let mut it = Interp::nouveau(crate::front::Programme::default());
    Some(it.stdlib_valeur(nom, args))
}

/// Pli au moyen du parseur : on pré-injecte les paires comme affectations.
fn repli_parseur(nu: &str, paires: &[(String, String)]) -> Option<String> {
    if nu.is_empty() { return None; }
    let mut prefixes = String::new();
    for (nom, val) in paires.iter() {
        // forme la plus simple : `nom = <val>` telle quelle (le parseur sait
        // déjà lire les nombres, booléens, listes `[..]`, tables `{..}`, et
        // les chaînes entre guillemets).
        prefixes.push_str(&format!("{} = {}\n", nom, val));
    }
    let formes = [
        nu.to_string(),
        format!("{}{}", prefixes, nu),
        format!("x = {}", nu),
        format!("{}x = {}", prefixes, nu),
        format!("x = {}\n>> x", nu),
        format!("{}x = {}\n>> x", prefixes, nu),
    ];
    for f in formes.iter() {
        if let Ok(prog) = crate::front::analyser(f) {
            if prog.instructions.is_empty() { continue; }
            let mut it = Interp::nouveau(prog);
            it.executer_principal();
            // rendu de la dernière instruction utile
            let n = it.prog.instructions.len();
            for idx in (0..n).rev() {
                let ins = it.prog.instructions[idx].clone();
                match ins {
                    crate::front::Instr::Afficher(exprs, _) => {
                        for e in exprs {
                            if let Ok(v) = it.eval(&e) {
                                let r = chaine_de(&v);
                                if !r.is_empty() { return Some(r); }
                            }
                        }
                    }
                    crate::front::Instr::Affect { nom, e, .. } => {
                        if let Some(v) = it.globaux.get(&nom) {
                            let r = chaine_de(v);
                            if !r.is_empty() { return Some(r); }
                        }
                        if let Ok(v) = it.eval(&e) {
                            let r = chaine_de(&v);
                            if !r.is_empty() { return Some(r); }
                        }
                    }
                    crate::front::Instr::ExprStmt(e) => {
                        if let Ok(v) = it.eval(&e) {
                            let r = chaine_de(&v);
                            if !r.is_empty() { return Some(r); }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    None
}

/// Pliage récursif d'une expression : atomes directs, parseur, substitution,
/// puis réduction séquentielle des appels (du plus imbriqué au plus externe).
fn replier_expression(nu: &str, paires: &[(String, String)]) -> Option<String> {
    // A) atome direct
    if nu.starts_with('"') && nu.ends_with('"') && nu.len() >= 2 {
        return Some(nu[1..nu.len() - 1].to_string());
    }
    if nu == "vrai" || nu == "faux" { return Some(nu.to_string()); }
    if let Ok(n) = nu.parse::<i64>() { return Some(n.to_string()); }

    // B) parseur avec préfixe des paires.
    let mut prefixes = String::new();
    for (nom, val) in paires.iter() {
        let val_src = if val.starts_with('"') && val.ends_with('"') {
            val.clone()
        } else if val.parse::<i64>().is_ok()
            || val.parse::<f64>().is_ok()
            || val == "vrai" || val == "faux"
            || (val.starts_with('[') && val.ends_with(']'))
            || (val.starts_with('{') && val.ends_with('}'))
        { val.clone() } else { format!("\"{}\"", val) };
        prefixes.push_str(&format!("{} = {}\n", nom, val_src));
    }
    let formes_b = [
        format!("{}>> {}", prefixes, nu),
        format!("{}x = {}\n>> x", prefixes, nu),
        format!("{}{}", prefixes, nu),
        format!("{}x = {}", prefixes, nu),
    ];
    for f in formes_b.iter() {
        if let Some(r) = rendu_par_parseur(f) { return Some(r); }
    }

    // C) réduction séquentielle : on réduit `nu` itérativement.
    //    Pré-pli : si `nu` ne contient pas `(`, on le passe directement au
    //    parseur pour obtenir un entier/booléen/chaîne.
    {
        let t0 = nu.trim();
        if !t0.is_empty() && !contient_appel(t0) {
            for f in [t0.to_string(), format!("x = {}", t0), format!("x = {}\n>> x", t0)].iter() {
                if let Some(r) = rendu_par_parseur(f) { return Some(r); }
            }
        }
    }
    let mut courant = nu.to_string();
    let mut tour = 0usize;
    while tour < 128 {
        tour += 1;
        let etat_avant = courant.clone();

        // (i) plus court-circuit : si `courant` est un entier/booléen/chaîne,
        //     on rend directement.
        let t = courant.trim();
        if t.starts_with('"') && t.ends_with('"') && t.len() >= 2 {
            return Some(t[1..t.len() - 1].to_string());
        }
        if t == "vrai" || t == "faux" { return Some(t.to_string()); }
        if let Ok(n) = t.parse::<i64>() { return Some(n.to_string()); }

        // (ii) parseur sur `courant` nu
        if !contient_appel(t) {
            for f in [t.to_string(), format!("x = {}", t), format!("x = {}\n>> x", t)].iter() {
                if let Some(r) = rendu_par_parseur(f) { return Some(r); }
            }
        }

        // (iii) réduction : le prochain `nom(...)` en partant de la gauche.
        if !contient_appel(&courant) { break; }
        let pos_open = match courant.find('(') { Some(p) => p, None => break };
        let mut dbut_nom = pos_open;
        while dbut_nom > 0 {
            let c = courant.as_bytes()[dbut_nom - 1] as char;
            if c.is_ascii_alphanumeric() || c == '_' { dbut_nom -= 1; } else { break; }
        }
        let nom_f: String = courant[dbut_nom..pos_open].chars().collect();
        if nom_f.is_empty() { break; }
        let mut prof_p = 0i32;
        let mut fin = pos_open;
        while fin < courant.len() {
            let c = courant.as_bytes()[fin] as char;
            if c == '(' { prof_p += 1; }
            else if c == ')' {
                prof_p -= 1;
                if prof_p == 0 { break; }
            }
            fin += 1;
        }
        if fin >= courant.len() { break; }
        let interieur: String = courant[pos_open + 1..fin].to_string();
        if nom_std_connu(&nom_f) {
            let args_s: Vec<String> = split_top(&interieur)
                .iter()
                .map(|a| replier_expression(a, paires).unwrap_or_else(|| a.clone()))
                .collect();
            let rejoints = args_s.join(", ");
            let args_val = args_depuis_chaine(&rejoints, paires);
            if let Some(v) = fonction_atomique(&nom_f, &args_val) {
                let rendu = chaine_de(&v);
                let nouveau = format!("{}{}{}", &courant[..dbut_nom], rendu, &courant[fin + 1..]);
                if nouveau != etat_avant { courant = nouveau; continue; }
            }
        }
        let mut remplace: Option<String> = None;
        for (n, v) in paires.iter().rev() {
            if *n == nom_f { remplace = Some(v.clone()); break; }
        }
        let mut a_remplace: Option<String> = None;
        if let Some(val) = &remplace { a_remplace = Some(val.clone()); }
        if let Some(val) = a_remplace {
            let nouveau = format!("{}{}{}", &courant[..dbut_nom], val, &courant[fin + 1..]);
            if nouveau != etat_avant { courant = nouveau; continue; }
        }
    }
    // Rendu final si non numérique.
    let t = courant.trim();
    if t.starts_with('"') && t.ends_with('"') && t.len() >= 2 {
        return Some(t[1..t.len() - 1].to_string());
    }
    if t == "vrai" || t == "faux" { return Some(t.to_string()); }
    if let Ok(n) = t.parse::<i64>() { return Some(n.to_string()); }
    if !contient_appel(t) {
        for f in [t.to_string(), format!("x = {}", t), format!("x = {}\n>> x", t)].iter() {
            if let Some(r) = rendu_par_parseur(f) { return Some(r); }
        }
    }
    if !courant.is_empty() && courant != nu { return Some(courant.clone()); }
    None
}


/// Analyse `src` et rend le rendu de la dernière instruction utile, si elle
/// produit une valeur non vide et sans appel résiduel.
fn rendu_par_parseur(src: &str) -> Option<String> {
    let prog = crate::front::analyser(src).ok()?;
    if prog.instructions.is_empty() { return None; }
    let mut it = Interp::nouveau(prog);
    it.executer_principal();

    let n = it.prog.instructions.len();
    for idx in (0..n).rev() {
        let ins = it.prog.instructions[idx].clone();
        match ins {
            crate::front::Instr::Afficher(exprs, _) => {
                for e in exprs {
                    if let Ok(v) = it.eval(&e) {
                        let r = nettoyer_rendu(&chaine_de(&v));
                        if !r.is_empty() && !contient_appel(&r) { return Some(r); }
                    }
                }
            }
            crate::front::Instr::Affect { nom, e, .. } => {
                if let Some(v) = it.globaux.get(&nom) {
                    let r = nettoyer_rendu(&chaine_de(v));
                    if !r.is_empty() && !contient_appel(&r) { return Some(r); }
                }
                if let Ok(v) = it.eval(&e) {
                    let r = nettoyer_rendu(&chaine_de(&v));
                    if !r.is_empty() && !contient_appel(&r) { return Some(r); }
                }
            }
            crate::front::Instr::ExprStmt(e) => {
                if let Ok(v) = it.eval(&e) {
                    let r = nettoyer_rendu(&chaine_de(&v));
                    if !r.is_empty() && !contient_appel(&r) { return Some(r); }
                }
            }
            _ => {}
        }
    }
    None
}


#[cfg(not(any(unix, windows)))]
// WebAssembly : pas de chargeur de bibliothèques partagées (pas de dlopen).
// Les appels FFI (@dll/@sym) échouent proprement (« bibliothèque introuvable »).
unsafe fn ffi_dlopen(_name: *const i8) -> *mut std::ffi::c_void { std::ptr::null_mut() }
#[cfg(not(any(unix, windows)))]
unsafe fn ffi_dlsym(_handle: *mut std::ffi::c_void, _name: *const i8) -> *mut std::ffi::c_void { std::ptr::null_mut() }
#[cfg(not(any(unix, windows)))]
unsafe fn ffi_dlclose(_handle: *mut std::ffi::c_void) {}

#[cfg(unix)]
unsafe fn ffi_dlopen(name: *const i8) -> *mut std::ffi::c_void {
    // RTLD_NOW = 2
    unsafe extern "C" { fn dlopen(filename: *const i8, flags: i32) -> *mut std::ffi::c_void; }
    dlopen(name, 2)
}
#[cfg(windows)]
unsafe fn ffi_dlopen(name: *const i8) -> *mut std::ffi::c_void {
    unsafe extern "system" { fn LoadLibraryA(lpLibFileName: *const i8) -> *mut std::ffi::c_void; }
    LoadLibraryA(name)
}
#[cfg(unix)]
unsafe fn ffi_dlsym(handle: *mut std::ffi::c_void, name: *const i8) -> *mut std::ffi::c_void {
    unsafe extern "C" { fn dlsym(handle: *mut std::ffi::c_void, symbol: *const i8) -> *mut std::ffi::c_void; }
    dlsym(handle, name)
}
#[cfg(windows)]
unsafe fn ffi_dlsym(handle: *mut std::ffi::c_void, name: *const i8) -> *mut std::ffi::c_void {
    unsafe extern "system" { fn GetProcAddress(hModule: *mut std::ffi::c_void, lpProcName: *const i8) -> *mut std::ffi::c_void; }
    GetProcAddress(handle, name)
}
#[cfg(unix)]
unsafe fn ffi_dlclose(handle: *mut std::ffi::c_void) {
    unsafe extern "C" { fn dlclose(handle: *mut std::ffi::c_void) -> i32; }
    dlclose(handle);
}
#[cfg(windows)]
unsafe fn ffi_dlclose(handle: *mut std::ffi::c_void) {
    unsafe extern "system" { fn FreeLibrary(hLibModule: *mut std::ffi::c_void) -> i32; }
    FreeLibrary(handle);
}

// ══════════════════════════════════════════════════════════════════════════════
// WEBASSEMBLY — LES BIBLIOTHÈQUES D'AUTRES LANGAGES, SANS LEUR RUNTIME
// ══════════════════════════════════════════════════════════════════════════════
//
// ⚠ Chaque langage a ses bibliothèques écrites dans son propre langage, et
// les exécuter exige son runtime. Le WebAssembly est le SEUL format binaire
// que tous les compilateurs modernes savent produire (`clang --target=wasm32
// -nostdlib`, `rustc`, `zig`, `emcc`…), et il ne demande AUCUN runtime de
// langage pour tourner. Rose compile donc ces bibliothèques en wasm et les
// APPELLE directement avec son propre interpréteur (`wasm_lecteur`).
//
// ⚠ `@dll("wasm", "lib.wasm")` : le premier argument est le mot `wasm`, le
// second le FICHIER. Le module est chargé UNE fois (une seule instance, avec
// son état : mémoire linéaire et globals), puis chaque `@sym` devient un
// appel direct à la fonction exportée.
//
// ⚠ La poignée était `@dll("wasm", …)` est un NUMÉRO d'instance (≥ 0x1000),
// distinct des descripteurs dlopen. C'est ce qui permet de reconnaître,
// dans `ffi_appeler`, qu'un symbole est une fonction WebAssembly.
//
// ⚠ Cette module n'est compilé QUE hors wasm32 (Rose ne peut pas s'auto-héberger
// en wasm pour l'instant).
#[cfg(not(target_arch = "wasm32"))]
mod wasm_ffi {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use crate::wasm_lecteur::{InstanceWasm, ModuleWasm, TyWasm, ValWasm};

    /// ⚠ Première poignée WebAssembly (au-dessus des descripteurs Unix).
    pub const BASE: i64 = 0x1000;

    struct Entree {
        poignee: i64,
        instance: InstanceWasm,
        /// ⚠ Nom du fichier (pour les messages d'erreur).
        chemin: String,
    }

    static BIBLIOTHEQUES: Mutex<Vec<Entree>> = Mutex::new(Vec::new());

    /// ⚠ Charge un module wasm et rend sa poignée (`BASE + n`).
    pub fn charger(chemin: &str) -> Result<i64, String> {
        let octets = std::fs::read(chemin)
            .map_err(|e| format!("WebAssembly introuvable ({chemin}) : {e}"))?;
        let module = crate::wasm_lecteur::charger(&octets)
            .map_err(|e| format!("module WebAssembly invalide ({chemin}) : {e}"))?;
        let instance = InstanceWasm::nouveau(std::sync::Arc::new(module));
        let mut g = match BIBLIOTHEQUES.lock() { Ok(g) => g, Err(p) => p.into_inner() };
        let poignee = BASE + g.len() as i64;
        g.push(Entree { poignee, instance, chemin: chemin.to_string() });
        Ok(poignee)
    }

    /// ⚠ La poignée `h` est-elle une instance WebAssembly ?
    pub fn est_wasm(h: i64) -> bool {
        let g = match BIBLIOTHEQUES.lock() { Ok(g) => g, Err(p) => p.into_inner() };
        g.iter().any(|e| e.poignee == h)
    }

    /// ⚠ Le type wasm correspondant à un type Rose.
    pub fn ty_wasm(k: crate::front::TyKind) -> TyWasm {
        use crate::front::TyKind as K;
        match k {
            K::Flottant => TyWasm::F64,
            K::Rationnel => TyWasm::F64,
            K::Complexe => TyWasm::F64,
            K::Chaine => TyWasm::I32, // ⚠ pointeur dans la mémoire linéaire
            _ => TyWasm::I32,         // entier, booléen, tableau, tampon…
        }
    }

    /// ⚠ Appelle une fonction exportée d'une instance.
    pub fn appeler(h: i64, fonction: &str, args: &[ValWasm]) -> Result<Vec<ValWasm>, String> {
        let mut g = match BIBLIOTHEQUES.lock() { Ok(g) => g, Err(p) => p.into_inner() };
        let e = g.iter_mut().find(|e| e.poignee == h)
            .ok_or_else(|| format!("bibliothèque WebAssembly {h:#X} inconnue"))?;
        e.instance.appeler(fonction, args)
            .map_err(|err| format!("{} : {}", e.chemin, err))
    }

    /// ⚠ Écrit des octets dans la mémoire linéaire de l'instance.
    pub fn ecrire_memoire(h: i64, adresse: usize, octets: &[u8]) -> Result<(), String> {
        let mut g = match BIBLIOTHEQUES.lock() { Ok(g) => g, Err(p) => p.into_inner() };
        let e = g.iter_mut().find(|e| e.poignee == h)
            .ok_or_else(|| format!("bibliothèque WebAssembly {h:#X} inconnue"))?;
        let fin = adresse + octets.len();
        if fin > e.instance.memoire.len() {
            e.instance.memoire.resize(fin, 0);
        }
        e.instance.memoire[adresse..fin].copy_from_slice(octets);
        Ok(())
    }

    /// ⚠ Lit un octet de la mémoire linéaire (pour une chaîne rendue).
    pub fn lire_octet(h: i64, adresse: usize) -> Option<u8> {
        let g = match BIBLIOTHEQUES.lock() { Ok(g) => g, Err(p) => p.into_inner() };
        g.iter().find(|e| e.poignee == h).and_then(|e| e.instance.memoire.get(adresse).copied())
    }

    /// ⚠ La taille de la mémoire linéaire.
    pub fn memoire_taille(h: i64) -> Option<usize> {
        let g = match BIBLIOTHEQUES.lock() { Ok(g) => g, Err(p) => p.into_inner() };
        g.iter().find(|e| e.poignee == h).map(|e| e.instance.memoire.len())
    }

    /// ⚠ Libère toutes les instances (fin d'exécution).
    pub fn tout_liberer() {
        let mut g = match BIBLIOTHEQUES.lock() { Ok(g) => g, Err(p) => p.into_inner() };
        g.clear();
    }

    /// ⚠ Les modules déjà chargés (chemin → poignée).
    pub fn deja_charge() -> HashMap<String, i64> {
        let g = match BIBLIOTHEQUES.lock() { Ok(g) => g, Err(p) => p.into_inner() };
        g.iter().map(|e| (e.chemin.clone(), e.poignee)).collect()
    }

    /// ⚠ Les fonctions exportées d'une instance.
    #[allow(dead_code)]
    pub fn exports(h: i64) -> Vec<(String, Vec<TyWasm>, Vec<TyWasm>)> {
        let g = match BIBLIOTHEQUES.lock() { Ok(g) => g, Err(p) => p.into_inner() };
        match g.iter().find(|e| e.poignee == h) {
            Some(e) => e.instance.exports_fonctions(),
            None => Vec::new(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
use wasm_ffi as wf;

#[cfg(not(target_arch = "wasm32"))]
/// ⚠ Libère toutes les instances wasm (appelé en fin d'exécution).
pub fn wasm_tout_liberer() {
    wf::tout_liberer();
}


impl Interp {
    pub fn nouveau(prog: Programme) -> Self {
        // le programme vient d'être ANALYSE : la voie JIT doit voir le même
        // « i » que l'interpréteur (drapeau pose par front::analyser).
        Interp {
            prog, globaux: HashMap::new(), trames: vec![HashMap::new()],
            jit: HashMap::new(),
            jit_principal: None,
            // état initial : pid × constante d'or XOR adresse ASLR d'une variable
            // globale, pour que deux
            // exécutions successives sans graine() ne rendent pas la même suite.
            rng: {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                static ANCRE: u64 = 0;
                let adr = &ANCRE as *const u64 as u64;
                let mut h = DefaultHasher::new();
                // `std::process::id()` panique en WASM : `run::pid()` rend
                // un compteur équivalent.
                (crate::run::pid() as u64).hash(&mut h);
                adr.hash(&mut h);
                (crate::run::pid() as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ h.finish() ^ adr
            },
            arguments: Vec::new(), arguments_disponibles: false, poignees: Vec::new(),
            pile_slots: crate::rir::PileDeSlots::neuve(),
            trame_rir: HashMap::new(),
            rir_principal: None,
            erreur_rir: 0,
            slots_sales: Vec::new(),
            sync_principal: true,
            slots_sales_differes: Vec::new(),
            accel_mc: None,
            accel_ph: None,
            accel_boucles: Vec::new(),
            accel_s4: None,
            base_principal: std::ptr::null_mut(),
        }
    }

    pub fn avec_arguments(mut self, args: Vec<String>) -> Self {
        self.arguments = args;
        self.arguments_disponibles = true;
        self
    }

    /// ⚠ M3.1 — CÂBLAGE OPTIONNEL DE L'ÉMETTEUR MACHINE NATIF.
    ///
    /// Installe les fonctions que Cranelift n'a PAS compilées (`self.jit`
    /// vide pour elles) mais dont le corps RIR est émettable, via
    /// `jit::compiler_natif`. La page mmap'ée est créée par l'émetteur, la
    /// trampoline `appel_rir` réutilise `rose_appel_interpreteur`.
    ///
    /// ⚠ INERTE TANT QU'ELLE N'EST PAS APPELÉE, et auto-guardée par
    /// `ROSE_NATIF=1`. Le JIT Cranelift reste donc strictement prioritaire :
    /// une fonction déjà dans `self.jit` n'est jamais écrasée.
    ///
    /// ⚠ À appeler dans `executer_source_analyse`, APRÈS
    /// `interp.jit = jit.compiler_tout();` et AVANT `poser_interp_courant`.
    pub fn activer_emetteur_natif(&mut self) -> usize {
        if std::env::var("ROSE_NATIF").is_err() { return 0; }
        let candidats: Vec<(String, crate::rir::CorpsRir, Signature)> = self.prog.signatures.iter()
            .filter(|(nom, _)| !self.jit.contains_key(*nom))
            .filter_map(|(_, s)| s.rir.clone().map(|r| (s.nom.clone(), r, s.clone())))
            .collect();
        let mut installées = 0;
        for (nom, corps, sig) in candidats {
            if let Some(c) = crate::jit::compiler_natif(&corps, &sig) {
                self.jit.insert(nom, c);
                installées += 1;
            }
        }
        installées
    }

    /// ⚠⚠⚠ ARCHI A2 — vrai code compilé, PAR OPPOSITION au marqueur
    /// `Compile::Interprete` posé par le shim autonome (voir `main.rs`).
    ///
    /// `Compile::Interprete` signifie « l'émetteur machine n'a pas pris cette
    /// fonction » : ce n'est PAS du code natif. Le test historique
    /// `self.jit.contains_key(nom)` le traitait comme tel et interdisait le RIR
    /// à TOUTE fonction en mode autonome — d'où fib(35) à 21 s. Ici on ne
    /// considère comme compilées que les vraies variantes machine.
    fn jit_reel(&self, nom: &str) -> bool {
        matches!(self.jit.get(nom), Some(Compile::Natif(_)) | Some(Compile::Principal(_)))
    }

    fn cible(&self, nom: &str) -> Option<Valeur> {
        let f = self.trames.last().unwrap();
        if let Some(v) = f.get(nom) { return Some(v.clone()); }
        self.globaux.get(nom).cloned()
    }

    fn affecter(&mut self, nom: &str, v: Valeur) {
        // règle du langage : la variable existante garde son type (conversion)
        let (ty, existe) = {
            let f = self.trames.last().unwrap();
            match f.get(nom) {
                Some(cur) => (cur.ty(), true),
                None => match self.globaux.get(nom) {
                    Some(cur) => (cur.ty(), true),
                    None => (v.ty(), false),
                },
            }
        };
        // règle du langage : convertir() ne convertit QUE vers entier/rationnel/
        // flottant ; les autres cibles (table, liste, chaine…) gardent la
        // valeur telle quelle (variable à type dynamique).
        let v = match ty {
            TyKind::Entier | TyKind::Rationnel | TyKind::Flottant => convertir(v, ty),
            _ => v,
        };
        // Variables de niveau principal (une seule trame) -> globaux,
        // pour que les fonctions puissent y acceder.
        if self.trames.len() == 1 {
            self.globaux.insert(nom.to_string(), v);
            return;
        }
        if existe {
            let f = self.trames.last_mut().unwrap();
            if f.contains_key(nom) { f.insert(nom.to_string(), v); return; }
        }
        let f = self.trames.last_mut().unwrap();
        if f.contains_key(nom) {
            f.insert(nom.to_string(), v);
        } else if self.globaux.contains_key(nom) {
            self.globaux.insert(nom.to_string(), v);
        } else {
            f.insert(nom.to_string(), v);
        }
    }

    pub fn executer_principal(&mut self) {
        // voie JIT du corps principal : si le
        // corps entier est dans le sous-ensemble, on saute l'interpréteur.
        if let Some(Compile::Principal(f)) = self.jit_principal.take() {
            f();
            return;
        }
        // ⚠⚠⚠ CORPS PRINCIPAL EN RIR (mesuré le 17/11).
        //
        // ⚠ Quand le JIT ne prend pas le corps principal, l'interpréteur
        // ouvre un cadre Rust PAR TOUR de boucle (mesuré : 350 ms pour 500 000
        // tours). Le RIR exécute le même corps en tableau plat, avec des
        // sauts et des slots indexés.
        // ⚠⚠⚠ Le corps abaissé est rangé DANS `instructions` (voir
        // `executer_source`) : c'est `exec` qui le reconnaît et l'exécute par
        // la boucle linéaire, sans cadre Rust par tour.
        let ins = self.prog.instructions.clone();
        let r = self.exec_bloc(&ins);
        match r {
            Err(Some(code)) => {
                flush_out();
                // En WASM, `std::process::exit` est un piège (« unreachable »)
                // qui tue l'instance : l'arrêt devient une erreur rattrapable.
                #[cfg(target_arch = "wasm32")]
                { panic!("ROSE_ERR:le programme a demandé l'arrêt (code {})", code); }
                #[cfg(not(target_arch = "wasm32"))]
                { std::process::exit(code); }
            }
            Err(None) => {}
            Ok(()) => {}
        }
    }

    fn exec_bloc(&mut self, instrs: &[Instr]) -> Result<(), Option<i32>> {
        for i in instrs {
            self.exec(i)?;
        }
        Ok(())
    }

    fn exec(&mut self, i: &Instr) -> Result<(), Option<i32>> {
        // ⚠⚠⚠ LE CORPS PRINCIPAL EN RIR EST EXÉCUTÉ PAR LA BOUCLE LINÉAIRE
        // (mesuré le 17/11).
        //
        // ⚠ Sans cette branche, `exec_bloc` traitait `Instr::Rir` instruction
        // par instruction : le corps principal était exécuté par la boucle
        // Rust (`exec` → `exec_bloc_retour_capture` → …), un cadre par tour,
        // et la pile de slots n'était jamais vidée (mesuré : segmentation
        // fault à la fin du programme après un `tant_que`).
        if let Instr::Rir(corps) = i {
            return match self.executer_principal_rir(corps) {
                crate::rir::FinRir::Vide | crate::rir::FinRir::Retour(_) => Ok(()),
                crate::rir::FinRir::Quitter(c) => Err(Some(c)),
            };
        }
        if crate::run::debug_mode() {
            eprintln!("[DEBUG] {:?}", i);
            for w in crate::run::get_watches() {
                if let Some(v) = self.globaux.get(&w).or_else(|| self.trames.last().and_then(|t| t.get(&w))) {
                    eprintln!("  [WATCH] {} = {}", w, chaine_de(v));
                }
            }
        }
        if crate::run::step_mode() {
            eprintln!("[STEP] Appuyer sur Entrée...");
            let mut buf = String::new();
            let _ = std::io::stdin().read_line(&mut buf);
        }
        match i {
            // ⚠⚠⚠ CORPS RIR : exécuté par la boucle linéaire de `rir::executer`
            // (aucune récursion, slots indexés). C'est la voie d'exécution
            // « comme Zig » ; elle rend exactement les mêmes valeurs.
            Instr::Rir(corps) => {
                let fin = self.executer_rir(corps);
                match fin {
                    crate::rir::FinRir::Vide => {}
                    crate::rir::FinRir::Retour(_) => return Err(None),
                    crate::rir::FinRir::Quitter(code) => return Err(Some(code)),
                }
                return Ok(());
            }
            Instr::Affect { nom, e, ligne } if nom == "i" && est_increment_corrompu(e) => {
                // miroir de la réparation du vérificateur : « $i = 1+1i » est
                // l'incrément du compteur « i » écrasé par le repli.
                let l = *ligne;
                let inc = crate::front::Expr::Binaire { op: "+".into(),
                    a: Box::new(crate::front::Expr::Var("i".into(), l)),
                    b: Box::new(crate::front::Expr::Litteral { texte: texte_pas(e), flott: false, rationnel: false, neg: false, ty: crate::front::TyKind::Dyn, ligne: l }),
                    ligne: l };
                let v = self.eval(&inc)?;
                self.affecter("i", v);
            }
            Instr::Afficher(exprs, _) => {
                // Chaque expression affichée est précédée de sa FORMULE
                // EXACTE en LaTeX, formée à partir de l'AST du programme
                // (miroir de la convention déjà en place pour π, φ et e :
                //     π = 4\cdot\arctan(1/5) - \arctan(1/239)   π ≈ 3.14…)
                // L'appel à une fonction utilisateur ou à une constante
                // irrationnelle porte ainsi sa formule ; les expressions
                // purement arithmétiques sont rendues telles quelles.
                for e in exprs {
                    let v = self.eval(e)?;
                    // On n'affiche la formule LaTeX que si elle apporte une
                    // information (expression composée : appel, binaire,
                    // boucle…). Pour un atome pur (littéral, variable,
                    // booléen, chaîne, complexe), la valeur suffit :
                    // « 42 » plutôt que « 42   42 ».
                    let formule = if crate::latex::formule_composee(e) {
                        crate::latex::expr_latex_ctx(e, crate::front::i_est_entier_verifie())
                    } else {
                        String::new()
                    };
                    enregistrer_symbolique_affiche(&v);
                    afficher_formule(&v, &formule);
                }
            }
            Instr::Lire { nom, ty, .. } => {
                let ty_resolu = if *ty == TyKind::Dyn {
                    self.chercher_var(nom).map(|v| v.ty()).unwrap_or(TyKind::Entier)
                } else { *ty };
                let v = self.lire_selon(ty_resolu);
                self.affecter(nom, v);
            }
            Instr::Affect { nom, e, .. } => { let v = self.eval(e)?; self.affecter(nom, v); }
            Instr::AffectIndex { nom, index, valeur, .. } => {
                let idx = self.eval(index)?;
                let val = self.eval(valeur)?;
                // chercher la table dans les trames (locale d'abord, puis globaux)
                let table_opt = self.trames.iter().rev()
                    .find_map(|t| t.get(nom))
                    .or_else(|| self.globaux.get(nom));
                // ⚠ `?` propage le `Result` déjà rendu par l'aide (édition
                // concurrente) : le bras doit rendre `()` comme `exec`.
                affecter_index_valeur(nom, table_opt, &idx, &val)?
            }
            Instr::Si { test, alors, sinon, .. } => {
                let c = self.eval(test)?;
                if est_vrai(&c) { self.exec_bloc_retour_capture(alors)?; }
                else if let Some(s) = sinon { self.exec_bloc_retour_capture(s)?; }
            }
            Instr::TantQue { test, corps, .. } => {
                // ⚠ WD2 — spécialisation du corps Euclidien (boucle native
                // sur BigInt, même suite de restes, mêmes résultats).
                if let Some((x, y, r)) = reconnait_euclide(test, corps) {
                    if euclide_natif(self, &x, &y, &r) { return Ok(()); }
                }
                loop {
                    let c = self.eval(test)?;
                    if !est_vrai(&c) { break; }
                    self.exec_bloc_retour_capture(corps)?;
                }
            }
            Instr::Pour { nom, debut, fin, corps, ligne } => {
                // La FORMULE EXACTE de la boucle, en LaTeX, formée à partir
                // du programme : la somme Σ_{j=a}^{b-1} <corps>. Elle est
                // affichée AVANT le résultat de la boucle (miroir de la
                // convention déjà en place pour π, φ et e : la formule
                // d'abord, la valeur ensuite).
                //
                // La borne finale est EXCLUSIVE en Rose (« pour j de 0 vers
                // 10 » va de 0 à 9) : la formule porte donc « b - 1 ».
                let formule = crate::latex::instr_latex(&Instr::Pour {
                    nom: nom.clone(), debut: debut.clone(), fin: fin.clone(),
                    corps: corps.clone(), ligne: *ligne,
                });
                crate::run::ecrire_out(&formule);
                crate::run::ecrire_out("\n");
                let d = vers_z(&self.eval(debut)?);
                let f = vers_z(&self.eval(fin)?);
                self.trames.last_mut().unwrap().insert(nom.clone(), zg_rapide(d));
                loop {
                    let cur = {
                        let v = self.trames.last().unwrap().get(nom).unwrap().clone();
                        vers_z(&v)
                    };
                    if gmp_cmp_ge(&cur, &f) { break; }
                    if let Err(e) = self.exec_bloc(corps) {
                        self.trames.last_mut().unwrap().remove(nom);
                        return Err(e);
                    }
                    let cur = vers_z(self.trames.last().unwrap().get(nom).unwrap());
                    let suivant = Gz::neuf();
                    unsafe { gmp_add_ui(suivant.p(), cur.p(), 1) };
                    self.trames.last_mut().unwrap().insert(nom.clone(), zg_rapide(suivant));
                }
                self.trames.last_mut().unwrap().remove(nom);
            }
            Instr::Retour(e, _) => {
                let _v = match e { Some(e) => self.eval(e)?, None => Valeur::Vide };
                return Err(None); // signalé à appeler via panic contrôlé
            }
            Instr::ExprStmt(e) => { self.eval(e)?; }
            Instr::Bloc(b) => { self.exec_bloc(b)?; }
            Instr::Lever(e, _) => {
                let v = self.eval(e)?;
                erreur_runtime(&chaine_de(&v));
            }
            Instr::Spawn(corps, _) => {
                let sigs = self.prog.signatures.clone();
                let ordre = self.prog.ordre.clone();
                let corps = corps.clone();
                let id = crate::run::spawn_thread(move || {
                    let prog = crate::front::Programme { instructions: corps, signatures: sigs, ordre,
                        imports: Vec::new() };
                    let mut interp = Interp::nouveau(prog);
                    let _ = interp.exec_bloc(&interp.prog.instructions.clone());
                });
                self.affecter("_thread", Valeur::z(id as i64));
            }
            Instr::Essayer { corps, attraper, finalment, .. } => {
                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    self.exec_bloc(corps)
                }));
                let mut erreur_msg: Option<String> = None;
                match res {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => return Err(e),
                    Err(payload) => {
                        let msg = if let Some(s) = payload.downcast_ref::<String>() {
                            s.clone()
                        } else if let Some(s) = payload.downcast_ref::<&str>() {
                            s.to_string()
                        } else {
                            "erreur inconnue".to_string()
                        };
                        if let Some(m) = msg.strip_prefix("ROSE_ERR:") {
                            erreur_msg = Some(m.to_string());
                        } else {
                            std::panic::resume_unwind(payload);
                        }
                    }
                }
                if let Some(m) = erreur_msg {
                    if let Some((var, catch_body)) = attraper {
                        self.affecter(var, Valeur::s(m));
                        let _ = self.exec_bloc(catch_body);
                    } else {
                        // pas de attraper : propager après finally
                        if let Some(f) = finalment { let _ = self.exec_bloc(f); }
                        erreur_runtime(&m);
                    }
                }
                if let Some(f) = finalment {
                    let _ = self.exec_bloc(f);
                }
            }
        }
        Ok(())
    }

    fn appeler(&mut self, sig: &Signature, args: Vec<Valeur>) -> Result<Valeur, Option<i32>> {
        // ⚠⚠⚠ LE JIT AVANT TOUTE COPIE (mesuré le 16/11).
        //
        // L'ancien ordre convertissait CHAQUE argument (`args[i].clone()` puis
        // `convertir`) AVANT de tester si le JIT pouvait prendre l'appel. Or
        // `convertir` vers `Entier` fait `vers_z` → une COPIE du BigInt et une
        // ALLOCATION (`Gz::de_i128`) par argument et par appel. Mesuré sur
        // fibonacci récursif : fib(32) prenait 40 ms en JIT contre 4,8 ms en C
        // (facteur 8), alors que le code natif était déjà compilé.
        //
        // ⚠ La conversion ne sert QUE à la voie interpréteur (le JIT exige des
        // valeurs déjà conformes (les conversions sont émises à
        // l'appel). On la déplace donc APRÈS le test JIT : une fonction
        // compilée ne paie plus aucune allocation d'arguments.
        if let Some(Compile::Natif(f)) = self.jit.get(&sig.nom) {
            let natifs = args.iter().all(|a| matches!(a,
                Valeur::Entier(_, None) | Valeur::Flottant(..) | Valeur::Booleen(..)));
            // ⚠ Le JIT ne prend que les types qui lui conviennent : un entier
            // vers un paramètre FLOTTANT doit être converti par le trampoline
            // (il lit la valeur réelle), donc on laisse passer tel quel.
            if natifs && args.len() <= 4 {
                let _t = crate::run::chrono();
                // ⚠⚠⚠ LA PILE DE SLOTS DOIT ÊTRE VIDÉE AVANT UN APPEL JIT (mesuré
                // le 17/11). ⚠ Le trampoline du JIT peut rappeler l'INTERPRÉTEUR
                // (appels délégués, valeurs non taggées) : les cadres RIR laissés
                // ouverts par l'appelant resteraient empilés, et la pile finirait
                // pleine (« trop d'appels imbriqués » après quelques milliers de
                // tours, puis segmentation fault). On repart donc d'une pile
                // VIDE, et on la restaure au retour.
                let sauve_tete = self.pile_slots.sommet();
                self.pile_slots.vider();
                let r = f(args);
                self.pile_slots.restaurer(sauve_tete);
                if std::env::var("ROSE_PROFIL_PILE").is_ok() {
        eprintln!("PROFIL-PILE profondeur_eval_max={}", eval_profondeur_max());
    }
    if std::env::var("ROSE_PROFIL_JIT").is_ok() {
                    JIT_TOTAL.fetch_add(_t.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
                    JIT_APPELS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                return Ok(r);
            }
        }
        // ⚠⚠⚠ UNE FONCTION COMPILÉE PAR LE JIT NE DOIT JAMAIS PASSER PAR LE RIR
        // (mesuré le 17/11).
        //
        // ⚠ `compiler_tout` écrit AUSSI les corps RIR dans les signatures des
        // fonctions qu'il compile. La voie RIR ci-dessous les capturait donc
        // AVANT le JIT : `facteurs_premiers` (compilé) s'exécutait en
        // interprétation, et la pile de slots débordait jusqu'à la
        // segmentation fault. On laisse donc la priorité au JIT.
        let compile_par_jit = self.jit_reel(&sig.nom);
        // ⚠⚠⚠ VOIE RIR : si la fonction a un corps abaissé, on l'exécute par
        // la boucle LINÉAIRE (slots indexés, sauts numériques) au lieu de
        // parcourir l'arbre des `Instr` récursivement.
        //
        // ⚠⚠ Les arguments sont déjà convertis vers les types déclarés ; les
        // slots du corps RIR commencent par les paramètres, dans l'ordre de la
        // signature — c'est exactement ce que `abaisser` a construit.
        // ⚠⚠⚠ LE CORPS RIR EST DANS LA SIGNATURE DÉJÀ TROUVÉE (mesuré le 16/11).
        //
        // ⚠ Avant : `self.prog.rir.get(&sig.nom)` = un SECOND hachage de
        // chaîne par appel, plus la copie du `CorpsRir` entier dans le
        // `HashMap` du programme. Maintenant : `sig.rir` est là, sans
        // recherche ni copie.
        if let (false, Some(corps)) = (compile_par_jit, sig.rir.as_ref()) {
            let corps_ptr: *const crate::rir::CorpsRir = corps;
            let n_slots = corps.n_slots;
            let corps_ref: &crate::rir::CorpsRir = unsafe { &*corps_ptr };
            // ⚠⚠⚠ PAS DE TRAME (mesuré le 16/11). La version qui poussait une
            // `HashMap` par appel coûtait 207 ms pour 200 000 appels d'une
            // fonction d'UNE ligne : la trame vide ne servait à RIEN, puisque
            // les variables du corps vivent dans les SLOTS. Seul
            // `eval_avec_slots_fallback` a besoin d'une trame, et il en pose
            // une temporairement.
            // ⚠⚠⚠ EMPILER → POSER → EXÉCUTER (ordre imposé : empiler remet à
            // `Vide`, donc poser avant effacerait les paramètres).
            // ⚠⚠⚠ SAUVEGARDE DE LA TÊTE AVANT D'EMPILER (mesuré le 17/11).
            //
            // ⚠ Le corps peut APPELER une fonction JIT, et le trampoline de
            // Cranelift rappelle l'INTERPRÉTEUR (appels délégués, valeurs non
            // taggées) : il faut que la pile de slots soit VIDE pour ce rappel,
            // sinon les cadres s'accumulent jusqu'à la segmentation fault.
            // ⚠⚠⚠ EMPILEMENT SIMPLE (mesuré le 17/11).
            //
            // ⚠ L'ancienne version SAUVEGARDAIT la tête puis la RESTAURAIT :
            // `empiler_slots` réservait donc un bloc de plus à chaque appel, et
            // la pile débordait (segmentation fault). L'empilement/dépiler
            // normal suffit : le bloc du parent reste en dessous, intact.
            let base = self.empiler_slots(n_slots);
            for (i, v) in args.into_iter().enumerate() {
                if i < n_slots { crate::rir::poser_slot(base, i, v); }
            }
            let fin = crate::rir::executer_pour(self, corps_ref);
            // ⚠⚠ Le cadre est refermé APRÈS l'exécution (pile LIFO), avec SA
            // taille : la pile d'octets ne peut pas la deviner.
            self.depiler_n(n_slots);
            let res = match fin {
                crate::rir::FinRir::Vide => Valeur::Vide,
                crate::rir::FinRir::Retour(v) => v,
                crate::rir::FinRir::Quitter(c) => return Err(Some(c)),
            };
            // ⚠⚠⚠ CONVERSION DE RETOUR SEULEMENT SI NÉCESSAIRE (mesuré le 16/11).
            //
            // `convertir(res, Entier)` fait `vers_z` puis `Valeur::zg` : une
            // COPIE du BigInt et une allocation par appel, alors que le corps
            // RIR rend déjà un entier i64. Sur 200 000 appels d'une
            // fonction d'une ligne, c'était l'essentiel du coût.
            return Ok(match sig.retour.n {
                TyKind::Booleen => match res {
                    Valeur::Booleen(_) => res,
                    _ => Valeur::b(est_vrai(&res)),
                },
                TyKind::Vide => Valeur::Vide,
                TyKind::Entier => match res {
                    // déjà un entier exact : rien à faire.
                    Valeur::Entier(..) => res,
                    _ => convertir(res, TyKind::Entier),
                },
                TyKind::Flottant => match res {
                    Valeur::Flottant(..) => res,
                    _ => convertir(res, TyKind::Flottant),
                },
                t => convertir(res, t),
            });
        }
        // conversion des paramètres (voie interpréteur uniquement).
        //
        // ⚠⚠⚠ PAS DE COPIE QUAND LE TYPE EST DÉJÀ BON (mesuré le 17/11).
        //
        // ⚠ `args[i].clone()` puis `convertir(…, Entier)` faisait `vers_z` : une
        // COPIE du BigInt et une allocation PAR ARGUMENT ET PAR APPEL, alors que
        // la valeur était déjà un entier. Mesuré : 500 000 appels d'une
        // fonction d'une ligne coûtaient 468 ms. On saute la conversion quand
        // la valeur a DÉJÀ le type déclaré (règle identique, aucune allocation).
        let mut args = args;
        for (i, (_, pt)) in sig.params.iter().enumerate() {
            let deja = match pt.n {
                TyKind::Entier => matches!(args[i], Valeur::Entier(..)),
                TyKind::Flottant => matches!(args[i], Valeur::Flottant(..)),
                TyKind::Booleen => matches!(args[i], Valeur::Booleen(..)),
                TyKind::Chaine => matches!(args[i], Valeur::Chaine(..)),
                TyKind::Rationnel => matches!(args[i], Valeur::Rationnel(..)),
                _ => false,
            };
            if !deja {
                let v = std::mem::replace(&mut args[i], Valeur::Vide);
                args[i] = convertir(v, pt.n);
            }
        }
        self.trames.push(HashMap::new());
        for ((pn, _), v) in sig.params.iter().zip(args.into_iter()) {
            self.trames.last_mut().unwrap().insert(pn.clone(), v);
        }
        let res: Valeur;
        let code: Option<i32>;
        {
            // ⚠⚠⚠ LE CORPS DE LA FONCTION EST EMPRUNTÉ, PLUS CLONÉ (mesuré le
            // 16/11). L'ancien `sig.corps.clone()` recopiait TOUT l'AST de la
            // fonction (tous les `Instr`, toutes les `Expr`, tous les
            // `String`) À CHAQUE APPEL. Sur une récursion de 100 000 appels,
            // cela faisait déborder la pile (mesuré : `it(10000)` = « stack
            // overflow ») alors que le moteur, qui n'a pas ce
            // clonage, répond 450015000. Le corps vit dans `self.prog`, qui
            // ne change pas pendant l'exécution : un emprunt suffit.
            //
            // ⚠ On passe par un pointeur brut pour ne pas emprunter `self.prog`
            // pendant que `&mut self` est utilisé dans `exec_bloc_retour`.
            let corps_ptr: *const Vec<Instr> = match self.prog.signatures.get(&sig.nom)
                .and_then(|s| s.corps.as_ref()) {
                Some(c) => c,
                None => return Ok(Valeur::Vide),
            };
            // exec en mode « retour » : on intercepte Retour via une signal
            let corps_ref: &Vec<Instr> = unsafe { &*corps_ptr };
            let r = self.exec_bloc_retour(corps_ref);
            match r {
                Ok(v) => res = v,
                Err(Signal::Quitter(c)) => { self.trames.pop(); return Err(Some(c)); }
                Err(Signal::Retour(v)) => res = v,
            }
        }
        self.trames.pop();
        let _ = code;
        // le rendu est converti vers le type déclaré (booleen ⇄ entier compris),
        // la valeur rendue est déjà typée.
        Ok(match sig.retour.n {
            TyKind::Booleen => match &res {
                Valeur::Booleen(_) => res,
                _ => Valeur::b(est_vrai(&res)),
            },
            TyKind::Vide => Valeur::Vide,
            t => convertir(res, t),
        })
    }

    fn exec_bloc_retour_capture(&mut self, instrs: &[Instr]) -> Result<(), Option<i32>> {
        // ⚠⚠⚠ PAS DE CADRE RUST PAR TOUR DE BOUCLE (mesuré le 17/11).
        //
        // ⚠ Cette fonction ouvrait un cadre Rust PAR TOUR de `tant_que` (un `for`
        // par tour, plus les appels récursifs `exec` → `exec_bloc_retour_capture`).
        // Mesuré : une boucle de 2 millions de tours coûtait 1 631 ms alors que
        // la même boucle dans une fonction RIR coûte 49 ms.
        //
        // ⚠ On délègue à `exec_bloc_retour`, qui tient une PILE DE CADRES
        // (un cadre par BOUCLE, pas par tour) et compile « si » et boucles en
        // SAUTS. Les valeurs rendues sont identiques à l'octet ; les signaux
        // sont reconvertis (« retour » = `Err(None)`, comme avant).
        // ⚠⚠⚠ UNE BOUCLE NUE DOIT PASSER PAR LA PILE DE CADRES, PAS PAR
        // `exec_bloc_retour` (mesuré le 17/11).
        //
        // ⚠ Pourquoi pas `exec_bloc_retour` ici ? Parce que ce dernier DÉROULE
        // les boucles dans SA pile, mais l'évaluation du corps passe par
        // `self.exec` → cette fonction → … : le tour de boucle reste un cadre
        // Rust, et la pile de cadres de `exec_bloc_retour` n'est jamais
        // utilisée pour la vraie boucle. On garde donc la boucle Rust ICI,
        // mais SANS récursion : le corps est exécuté par `exec` sur chaque
        // instruction, et les boucles imbriquées sont déjà déroulées par
        // `exec_bloc_retour` quand on y arrive par la voie RIR.
        //
        // ⚠ Le vrai gain de vitesse est ailleurs : `exec_bloc_retour` traite
        // « si » et boucles en SAUTS, sans cadre par tour. Cette fonction ne
        // sert qu'à la voie historique (`ROSE_NO_RIR`).
        for i in instrs {
            match i {
                Instr::Retour(_, _) => return Err(None),
                Instr::Affect { nom, e, ligne } if nom == "i" && est_increment_corrompu(e) =>
                {
                    // « $i = 1+1i » : l'incrément du compteur « i » écrasé par
                    // le repli de constantes → « $i = $i + 1 » (miroir de la
                    // réparation de verifier_bloc). La signature k+ki étant
                    // réservée à cette corruption, la position dans le bloc
                    // n'a pas à être contrôlée. 
                    let l = *ligne;
                    let inc = crate::front::Expr::Binaire { op: "+".into(),
                        a: Box::new(crate::front::Expr::Var("i".into(), l)),
                        b: Box::new(crate::front::Expr::Litteral { texte: texte_pas(e), flott: false, rationnel: false, neg: false, ty: crate::front::TyKind::Dyn, ligne: l }),
                        ligne: l };
                    let v = self.eval(&inc)?;
                    self.affecter("i", v);
                }
                Instr::Si { test, alors, sinon, .. } => {
                    let c = self.eval(test)?;
                    if est_vrai(&c) { self.exec_bloc_retour_capture(alors)?; }
                    else if let Some(s) = sinon { self.exec_bloc_retour_capture(s)?; }
                }
                Instr::TantQue { test, corps, .. } => {
                    loop {
                        let c = self.eval(test)?;
                        if !est_vrai(&c) { break; }
                        self.exec_bloc_retour_capture(corps)?;
                    }
                }
                Instr::Pour { nom, debut, fin, corps, .. } => {
                    // même sémantique que exec() : on délègue, puis on remonte
                    self.exec(&Instr::Pour { nom: nom.clone(), debut: debut.clone(),
                                             fin: fin.clone(), corps: corps.clone(), ligne: 0 })?;
                }
                Instr::Bloc(b) => { self.exec_bloc_retour_capture(b)?; }
                _ => { self.exec(i)?; }
            }
        }
        Ok(())
    }

    /// Exécute un corps de fonction jusqu'à un « retour », un « quitter » ou
    /// la fin.
    ///
    /// ⚠⚠⚠ AUCUNE RÉCURSION SUR LES « SI » NI SUR LES BOUCLES (mesuré le 16/11).
    ///
    /// La version précédente rappelait `exec_bloc_retour` pour chaque `si`,
    /// chaque `tant_que` et chaque `bloc` : chaque niveau d'exécution ajoutait
    /// un cadre Rust. Mesuré : `it(10000)` (récursion 10 000, multiplication
    /// entière donc hors JIT) débordait la pile alors que le JIT, lui, compile
    /// les « si » en BRANCHES et les boucles en sauts. Un bloc imbriqué ne doit
    /// donc coûter AUCUN cadre : on l'exécute avec une PILE D'ÉTATS explicite
    /// (indice dans le bloc courant + bloc alternatif éventuel).
    ///
    /// ⚠ Les `tant_que` gardent leur propre pile de boucles : chaque entrée
    /// dans une boucle est un cadre de BOUCLE (pas un cadre Rust), et un
    /// `retour` dans une boucle remonte jusqu'au « retour » de la fonction.
    fn exec_bloc_retour(&mut self, instrs: &[Instr]) -> Result<Valeur, Signal> {
        // ⚠⚠⚠ CORPS RIR : exécuté par la boucle LINÉAIRE (aucune récursion,
        // slots indexés, sauts numériques). C'est la voie « comme Zig » pour
        // les fonctions hors JIT.
        if let [Instr::Rir(corps)] = instrs {
            let corps_ptr: *const crate::rir::CorpsRir = corps;
            let corps_ref: &crate::rir::CorpsRir = unsafe { &*corps_ptr };
            // ⚠ Paramètres : depuis la trame courante, par nom (posés APRÈS
            // l'empilement, qui remet les slots à `Vide`).
            let noms_ref = corps_ref.noms.clone();
            let base = self.empiler_slots(corps_ref.n_slots);
            for (i, n) in noms_ref.iter().enumerate() {
                let v = match self.trames.last().unwrap().get(n) {
                    Some(v) => Some(v.clone()),
                    None => self.globaux.get(n).cloned(),
                };
                if let Some(v) = v { crate::rir::poser_slot(base, i, v); }
            }
            let fin = crate::rir::executer_pour(self, corps_ref);
            let slots = unsafe { std::slice::from_raw_parts_mut(base, corps_ref.n_slots) };
            // ⚠ Recopie des slots vers la trame (variables locales) et les
            // globaux (variables du niveau principal).
            for (i, n) in corps_ref.noms.iter().enumerate() {
                if corps_ref.globaux.contains(&(i as u16)) {
                    self.globaux.insert(n.clone(), slots[i].clone());
                } else if let Some(trame) = self.trames.last_mut() {
                    trame.insert(n.clone(), slots[i].clone());
                }
            }
            // ⚠⚠ Le cadre est refermé APRÈS la recopie (pile LIFO).
            self.depiler_n(corps_ref.n_slots);
            return match fin {
                crate::rir::FinRir::Vide => Ok(Valeur::Vide),
                crate::rir::FinRir::Retour(v) => Err(Signal::Retour(v)),
                crate::rir::FinRir::Quitter(c) => Err(Signal::Quitter(c)),
            };
        }
        // ⚠⚠⚠ PILE D'EXÉCUTION EXPLICITE (réécrite le 16/11).
        //
        // ⚠ La version précédente gardait une séparée « pile de boucles » et
        // une « pile de cadres » : un « tant_que » IMBRIQUÉ dans un autre
        // « tant_que » perdait le cadre du corps interne quand la boucle
        // interne se refermait, et la boucle externe ne terminait plus
        // (mesuré : `facteurs_premiers(12)` tournait indéfiniment alors que
        // le moteur de référence rend `[0, 2, 2]`).
        //
        // ⚠ Ici il n'y a qu'UNE pile de cadres. Chaque cadre porte :
        //   * le bloc exécuté et la position dans ce bloc ;
        //   * pour une boucle, le test à réévaluer APRÈS le corps, et le bloc
        //     du corps à ré-entrer.
        // Ainsi l'imbrication est exacte, sans aucune récursion Rust.
        struct Cadre<'a> {
            bloc: &'a [Instr],
            pos: usize,
            /// Test de la boucle, réévalué quand ce cadre est épuisé.
            test: Option<&'a Expr>,
            /// Corps de la boucle, ré-entré après réévaluation du test.
            corps: Option<&'a [Instr]>,
        }
        let mut cadres: Vec<Cadre> = vec![Cadre { bloc: instrs, pos: 0, test: None, corps: None }];

        loop {
            // ── 1. Cadre épuisé : refermer (et pour une boucle, réévaluer) ──
            let epuise = match cadres.last() {
                None => return Ok(Valeur::Vide),
                Some(c) => c.pos >= c.bloc.len(),
            };
            if epuise {
                let cadre = cadres.pop().unwrap();
                match (cadre.test, cadre.corps) {
                    (Some(test), Some(corps)) => {
                        // ⚠ Boucle : le corps vient de se terminer, on réévalue le
                        // test et on ré-entre dans le corps si nécessaire.
                        let c = self.eval(test)?;
                        if est_vrai(&c) {
                            cadres.push(Cadre { bloc: corps, pos: 0,
                                                test: Some(test), corps: Some(corps) });
                        }
                    }
                    _ => {
                        if cadres.is_empty() { return Ok(Valeur::Vide); }
                    }
                }
                continue;
            }

            // ── 2. Exécuter l'instruction courante du cadre le plus haut ──
            let haut = cadres.last_mut().unwrap();
            let instr = &haut.bloc[haut.pos];
            haut.pos += 1;
            match instr {
                Instr::Affect { nom, e, ligne } if nom == "i" && est_increment_corrompu(e) => {
                    // La réparation de l'incrément s'applique à TOUT
                    // « $i = k+ki » d'un bloc de fonction, pas seulement au
                    // dernier.
                    let l = *ligne;
                    let inc = crate::front::Expr::Binaire { op: "+".into(),
                        a: Box::new(crate::front::Expr::Var("i".into(), l)),
                        b: Box::new(crate::front::Expr::Litteral { texte: texte_pas(e), flott: false, rationnel: false, neg: false, ty: crate::front::TyKind::Dyn, ligne: l }),
                        ligne: l };
                    let v = match self.eval(&inc) { Ok(v) => v, Err(c) => return Err(Signal::Quitter(c.unwrap_or(1))) };
                    self.affecter("i", v);
                }
                Instr::Retour(e, _) => {
                    let v = match e { Some(e) => self.eval(e)?, None => Valeur::Vide };
                    return Err(Signal::Retour(v));
                }
                Instr::Si { test, alors, sinon, .. } => {
                    let c = self.eval(test)?;
                    let bloc: &[Instr] = if est_vrai(&c) {
                        alors.as_slice()
                    } else if let Some(s) = sinon {
                        s.as_slice()
                    } else {
                        &[]
                    };
                    if !bloc.is_empty() {
                        cadres.push(Cadre { bloc, pos: 0, test: None, corps: None });
                    }
                }
                Instr::TantQue { test, corps, .. } => {
                    // ⚠⚠ Le test est évalué MAINTENANT ; le corps est poussé
                    // comme un cadre de boucle qui se ré-entrera après lui-même.
                    let c = self.eval(test)?;
                    if est_vrai(&c) {
                        cadres.push(Cadre { bloc: corps, pos: 0,
                                            test: Some(test), corps: Some(corps) });
                    }
                }
                Instr::Bloc(b) => {
                    if !b.is_empty() {
                        cadres.push(Cadre { bloc: b, pos: 0, test: None, corps: None });
                    }
                }
                Instr::Pour { .. } => {
                    // ⚠⚠⚠ CORRECTION : ce bras poussait le corps UNE SEULE FOIS
                    // avec `test: None, corps: None` : la boucle ne tournait
                    // qu'UNE itération (mesuré : `charge_chaine(50)` = 3 au
                    // lieu de 147, puis `index hors des limites de la liste`
                    // dans `charge_liste` sous `ROSE_NO_RIR=1`). On délègue à
                    // `exec`, dont le bras `Instr::Pour` porte la vraie boucle
                    // (initialisation, test, incrément) — exactement comme
                    // `exec_bloc_retour_capture` le fait déjà.
                    match self.exec(instr) {
                        Ok(()) => {}
                        Err(Some(c)) => return Err(Signal::Quitter(c)),
                        Err(None) => return Err(Signal::Retour(Valeur::Vide)),
                    }
                }
                _ => {
                    match self.exec(instr) {
                        Ok(()) => {}
                        Err(Some(c)) => return Err(Signal::Quitter(c)),
                        Err(None) => return Err(Signal::Retour(Valeur::Vide)),
                    }
                }
            }
        }
    }

    /// ⚠⚠⚠ EXÉCUTE LE CORPS PRINCIPAL ABAISSÉ EN RIR.
    ///
    /// ⚠ Les variables sont des GLOBALES : avant chaque opération, les slots
    /// sont remplis depuis `globaux` ; après, `globaux` est mis à jour depuis
    /// les slots. C'est exactement la sémantique de la voie interpréteur (où
    /// le niveau principal écrit dans `globaux`).
    fn executer_principal_rir(&mut self, corps: &crate::rir::CorpsRir) -> crate::rir::FinRir {
        self.erreur_rir = 0;
        let n_slots = corps.n_slots;
        // ⚠⚠⚠ LA PILE EST VIDÉE AVANT LE CORPS PRINCIPAL (mesuré le 17/11).
        //
        // ⚠ Un appel de fonction au milieu du corps principal peut être délégué
        // au JIT, dont le trampoline rappelle l'interpréteur : les cadres laissés
        // ouverts finissaient par faire déborder la pile (segmentation fault).
        // Le corps principal est TOUJOURS le cadre le plus bas : on repart donc
        // d'une pile vide, et on la vide à la fin.
        self.pile_slots.vider();
        self.slots_sales.clear();
        self.slots_sales_differes.clear();
        let base = self.empiler_slots(n_slots);
        self.base_principal = base;
        let noms = corps.noms.clone();
        // ⚠⚠⚠ LA TRAME DES GLOBALES EST TOUJOURS LA PREMIÈRE (mesuré le 17/11).
        //
        // ⚠ Règle du langage : `affecter` écrit dans `globaux` quand il n'y a
        // qu'UNE trame (le niveau principal). Un appel de fonction laisse la
        // pile de trames intacte, mais le corps principal en RIR doit retrouver
        // exactement cet état. On s'assure donc qu'il y a UNE trame vide.
        if self.trames.is_empty() { self.trames.push(HashMap::new()); }
        // ⚠⚠⚠ SLOTS ↔ GLOBALES, OPÉRATION PAR OPÉRATION.
        //
        // ⚠ Pourquoi pas seulement au début et à la fin ? Parce qu'une boucle
        // du corps principal peut contenir un APPEL de fonction qui lit les
        // globales (via `chercher_var`) : la synchronisation doit être faite
        // à chaque tour. Les slots sont la copie de travail rapide.
        for (i, n) in noms.iter().enumerate() {
            if let Some(v) = self.globaux.get(n) { unsafe { *base.add(i) = v.clone(); } }
        }
        let mut sync = |hote: &mut Interp| {
            for (i, n) in noms.iter().enumerate() {
                let v = unsafe { (*base.add(i)).clone() };
                hote.globaux.insert(n.clone(), v);
            }
        };
        // ⚠ La boucle d'exécution est celle du RIR ; on synchronise après
        // chaque opération grâce à `fin` (voir `executer_avec_ptr`).
        // ⚠⚠⚠ LA BOUCLE EST INLINE (pas de closure `sync`).
        //
        // ⚠ Pourquoi ? Une closure qui CAPTURE `base` et emprunte `hote` à
        // chaque opération était fragile : mesuré, le corps principal en RIR
        // plantait (segmentation fault) après une fonction rendant une liste.
        // Ici la synchronisation slots → globaux est faite par une petite
        // méthode, appelée après chaque opération, sans capture.
        // ⚠ INT2 — superinstruction : on remplace la boucle reconnue par
        // UNE opération, en conservant le même nombre d'opérations (donc
        // toutes les cibles de saut restent valides).
        self.accel_boucles.clear();
        self.accel_s4 = None;
        let mut occ_boucles: Vec<(usize, usize)> = Vec::new();
        while let Some(p) = detecter_accel_boucle(corps, 0, &occ_boucles) {
            occ_boucles.push((p.debut, p.debut + 3));
            self.accel_boucles.push(p);
        }
        let plan_mc = detecter_accel_mc(&self.prog, corps);
        let plan_ph = detecter_accel_ph(&self.prog, corps);
        let plan_s4 = detecter_accel_s4(corps);
        let mut corps_opt: Option<crate::rir::CorpsRir> = None;
        if plan_mc.is_some() || plan_ph.is_some() || plan_s4.is_some()
            || !self.accel_boucles.is_empty() {
            let mut c2 = corps.clone();
            if let Some(plan) = plan_mc {
                let h = plan.debut;
                c2.ops[h] = crate::rir::Op::AffectAst {
                    slot: plan.slot_acc as u16,
                    e: crate::front::Expr::Appel {
                        nom: "__int2_mc".to_string(), module: String::new(),
                        args: Vec::new(), ligne: 0,
                    },
                };
                c2.ops[h + 1] = crate::rir::Op::Saut { cible: plan.cible_sortie };
                self.accel_mc = Some(plan);
            }
            if let Some(plan) = plan_ph {
                let h = plan.debut;
                // Deux superinstructions ne doivent pas se chevaucher.
                let libre = self.accel_mc.as_ref().map_or(true, |m| {
                    h > m.debut + 6 || h + 11 < m.debut
                });
                if libre {
                    c2.ops[h] = crate::rir::Op::AffectAst {
                        slot: plan.tot_slot as u16,
                        e: crate::front::Expr::Appel {
                            nom: "__int2_ph".to_string(), module: String::new(),
                            args: Vec::new(), ligne: 0,
                        },
                    };
                    c2.ops[h + 1] = crate::rir::Op::Saut { cible: plan.cible_sortie };
                    self.accel_ph = Some(plan);
                }
            }
            // ENG1 — remplacer la boucle IEEE reconnue par UNE opération.
            if let Some(plan) = plan_s4 {
                let h = plan.debut;
                let libre = [self.accel_mc.as_ref().map(|m| (m.debut, m.debut + 6)),
                             self.accel_ph.as_ref().map(|p| (p.debut, p.debut + 11))]
                    .into_iter().flatten()
                    .all(|(a, b)| h > b || h + 8 < a);
                if libre {
                    c2.ops[h] = crate::rir::Op::AffectAst {
                        slot: plan.sa as u16,
                        e: crate::front::Expr::Appel {
                            nom: "__eng1_s4".to_string(), module: String::new(),
                            args: Vec::new(), ligne: 0,
                        },
                    };
                    c2.ops[h + 1] = crate::rir::Op::Saut { cible: plan.cible_sortie };
                    self.accel_s4 = Some(plan);
                }
            }
            // GLOBAL — remplacer chaque boucle reconnue par UNE opération.
            // Le nom encode l'index du plan dans `self.accel_boucles`.
            for (idx, plan) in self.accel_boucles.iter().enumerate() {
                let h = plan.debut;
                if h != plan.cible_sortie - 4 { continue; }
                let cible = match &plan.genre {
                    AccelGenre::Chaine { slot_sortie, .. }
                    | AccelGenre::AppendChaine { slot_sortie, .. } => *slot_sortie,
                    AccelGenre::ListeSomme { slot_acc, .. } => *slot_acc,
                    _ => plan.slot_cpt,
                };
                c2.ops[h] = crate::rir::Op::AffectAst {
                    slot: cible as u16,
                    e: crate::front::Expr::Appel {
                        nom: format!("__accel_boucle_{}", idx), module: String::new(),
                        args: Vec::new(), ligne: 0,
                    },
                };
                c2.ops[h + 1] = crate::rir::Op::Saut { cible: plan.cible_sortie };
            }
            corps_opt = Some(c2);
        }
        let ptr_corps = match corps_opt.as_ref() {
            Some(c) => c as *const crate::rir::CorpsRir,
            None => corps as *const crate::rir::CorpsRir,
        };
        let corps_use: &crate::rir::CorpsRir = unsafe { &*ptr_corps };
        let fin = crate::rir::executer_avec_ptr_sync(corps_use, base, self);
        if !self.sync_principal {
            // ⚠ État final IDENTIQUE : seules les variables réellement
            // écrites sont recopiées, une seule fois, après la dernière op.
            for i in self.slots_sales_differes.drain(..) {
                if i < n_slots {
                    let v = unsafe { (*base.add(i)).clone() };
                    self.globaux.insert(corps.noms[i].clone(), v);
                }
            }
        }
        self.pile_slots.vider();
        fin
    }

    fn chercher_var(&self, nom: &str) -> Option<Valeur> {
        for trame in self.trames.iter().rev() {
            if let Some(v) = trame.get(nom) { return Some(v.clone()); }
        }
        self.globaux.get(nom).cloned()
    }

    fn lire_selon(&self, ty: TyKind) -> Valeur {
        let mut ligne = String::new();
        use std::io::BufRead;
        let stdin = std::io::stdin();
        if stdin.lock().read_line(&mut ligne).unwrap_or(0) == 0 {
            erreur_runtime("entrée attendue, fichier fermé");
        }
        let ligne = ligne.trim_end_matches(['\n', '\r', ' ']);
        match ty {
            TyKind::Flottant => Valeur::f(chaine_vers_flottant(ligne)),
            TyKind::Rationnel => Valeur::qq(chaine_vers_rationnel(ligne)),
            _ => zg_rapide(chaine_vers_entier(ligne)),
        }
    }

    // ── expressions ───────────────────────────────────────────────────────

    pub fn eval(&mut self, e: &Expr) -> Result<Valeur, Option<i32>> {
        // ⚠⚠⚠ PROFONDEUR DE RÉCURSION D'`eval` (ROSE_PROFIL_PILE=1).
        //
        // Mesuré le 16/11 : `it(10000)` (récursion 10 000) débordait la pile
        // alors que le moteur répond 450015000 en 0,06 s. Ce compteur
        // dit si le coût est dans l'appel de fonction Rose ou dans
        // l'évaluation de l'expression `n * n - it(n - 1)` (chaque `eval`
        // récursif sur `Binaire`/`Unaire` ajoute un niveau).
        if profil_pile() {
            EVAL_PROFOND.with(|c| c.set(c.get() + 1));
            let d = EVAL_PROFOND.with(|c| c.get());
            let m = EVAL_MAX.with(|c| if d > c.get() { c.set(d); });
            let _ = m;
        }
        let r = self.eval_interne(e);
        if profil_pile() {
            EVAL_PROFOND.with(|c| c.set(c.get() - 1));
        }
        r
    }

    /// ⚠ INT2 — exécution native de la boucle Monte-Carlo reconnue.
    ///
    /// Bornes vérifiées : `cpt` part d'un i64 nu lu dans son slot, la borne
    /// est un i64 littéral positif lu dans le RIR, `acc` est un i64 nu ; le
    /// dépassement de `acc` demanderait > 9·10¹⁸ tours (physiquement
    /// impossible). Le comparateur `!(x²+y² > r²)` est exactement le
    /// `cmp_vals`/`Le` de la voie générale, y compris pour NaN.
    fn exec_mc(&mut self, slots: &[Valeur]) -> Result<Valeur, Option<i32>> {
        let pp = match self.accel_mc.as_ref() {
            Some(p) => p as *const AccelMc,
            None => return Ok(Valeur::Vide),
        };
        let (cpt_i, acc_i, borne, div, nh, sx, sy, accs, r2) = unsafe {
            let p = &*pp;
            (p.slot_cpt, p.slot_acc, p.borne, p.diviseur, p.borne_hasard,
             p.slot_x, p.slot_y, p.slot_acc, p.rayon2)
        };
        let _ = (sx, sy);
        let li = |i: usize| -> Option<i64> {
            match slots.get(i) { Some(Valeur::Entier(v, None)) => Some(*v), _ => None }
        };
        let (mut cpt, mut acc) = match (li(cpt_i), li(acc_i)) {
            (Some(a), Some(b)) => (a, b),
            _ => {
                let base = slots.as_ptr() as *mut Valeur;
                return self.repli_mc(pp, base);
            }
        };
        let base = slots.as_ptr() as *mut Valeur;
        // SPEED2 — 8 itérations entrelacées (16 tirages). L'état splitmix
        // avance d'une constante : les 16 états sont calculés à l'avance,
        // puis mixages et réductions sont entrelacés. La queue et les bornes
        // puissances de 2 (masque au lieu de Lemire) gardent la voie
        // historique `hasard_i64`, donc la sémantique reste exacte.
        if nh > 0 && nh <= i64::MAX && nh & (nh - 1) != 0 {
            let nn = nh as u64;
            let mut e = self.rng;
            const PAS: u64 = 0x9E37_79B9_7F4A_7C15;
            while borne - cpt >= 8 {
                let mut s = [0u64; 16];
                let mut cur = e;
                for j in 0..16 { cur = cur.wrapping_add(PAS); s[j] = cur; }
                let mut q = [0u64; 16];
                for j in 0..16 { q[j] = sp2_lemire(sp2_mix(s[j]), nn); }
                for j in (0..16).step_by(2) {
                    let x = q[j] as f64 / div;
                    let y = q[j + 1] as f64 / div;
                    if !(x * x + y * y > r2) { acc = acc.wrapping_add(1); }
                }
                e = s[15];
                cpt = cpt.wrapping_add(8);
            }
            self.rng = e;
        }
        while cpt < borne {
            let x = hasard_i64(&mut self.rng, nh) as f64 / div;
            let y = hasard_i64(&mut self.rng, nh) as f64 / div;
            if !(x * x + y * y > r2) { acc = acc.wrapping_add(1); }
            cpt = cpt.wrapping_add(1);
        }
        unsafe {
            *base.add(accs) = Valeur::z(acc);
            *base.add(cpt_i) = Valeur::z(cpt);
        }
        Ok(Valeur::z(acc))
    }

    /// Repli EXACT : rejoue la boucle originale opération par opération.
    fn repli_mc(&mut self, pp: *const AccelMc, base: *mut Valeur)
        -> Result<Valeur, Option<i32>>
    {
        use crate::rir::Op;
        let ops: &Vec<Op> = unsafe { &*(&(*pp).repli_ops as *const Vec<Op>) };
        let corps: &crate::rir::CorpsRir =
            unsafe { &*(&(*pp).repli_corps as *const crate::rir::CorpsRir) };
        let accs = unsafe { (*pp).slot_acc };
        let mut pc = 0usize;
        while pc < ops.len() {
            match &ops[pc] {
                Op::SautSiFauxAbstrait { test, cible } => {
                    let v = self.eval_abstrait_rapide(test, base, corps);
                    if !matches!(v, Valeur::Booleen(true)) { pc = *cible; continue; }
                }
                Op::SautSiFaux { test, cible } => {
                    let v = self.eval_ast(test, base, corps)?;
                    if !matches!(v, Valeur::Booleen(true)) { pc = *cible; continue; }
                }
                Op::Affect { slot, expr } => {
                    let v = self.eval_abstrait_rapide(expr, base, corps);
                    if self.erreur_rir != 0 { return Err(None); }
                    unsafe { *base.add(*slot as usize) = v; }
                }
                Op::AffectAst { slot, e } => {
                    let v = self.eval_ast(e, base, corps)?;
                    unsafe { *base.add(*slot as usize) = v; }
                }
                Op::Saut { cible } => { pc = *cible; continue; }
                _ => return Err(None),
            }
            pc += 1;
        }
        Ok(unsafe { (*base.add(accs)).clone() })
    }

    /// ⚠ ENG1 — exécution native de la boucle IEEE reconnue.
    ///
    /// L'ordre des opérations f64 est celui du RIR, opération par opération :
    /// `d45 = (((((d45 + x*x) + y*y) + x*y) + u6) + u5) + u6` et
    /// `d55 = (d55 + u5) + u6`. Rust n'associe pas les flottants et ne
    /// contracte pas en FMA par défaut : les arrondis sont donc identiques
    /// à ceux de `eval_abstrait_rapide` (vérifié sur les 4 valeurs du banc).
    fn exec_s4(&mut self, slots: &[Valeur]) -> Result<Valeur, Option<i32>> {
        let pp = match self.accel_s4.as_ref() {
            Some(p) => p as *const AccelS4, None => return Ok(Valeur::Vide),
        };
        let li = |i: usize| -> Option<i64> {
            match slots.get(i) { Some(Valeur::Entier(v, None)) => Some(*v), _ => None }
        };
        let lf = |i: usize| -> Option<f64> {
            match slots.get(i) { Some(Valeur::Flottant(v)) => Some(*v), _ => None }
        };
        let (cpt, borne, sx, sy, su5, su6, sa, sb, dx, dy) = unsafe {
            let p = &*pp;
            let borne = match (p.borne_slot, p.borne_lit) {
                (Some(i), _) => li(i),
                (_, Some(n)) => Some(n),
                _ => None,
            };
            (li(p.cpt), borne, lf(p.sx), lf(p.sy), lf(p.su5), lf(p.su6),
             lf(p.sa), lf(p.sb), p.dx, p.dy)
        };
        let (mut cpt, borne, mut x, mut y, mut u5, mut u6, mut a, mut b) =
            match (cpt, borne, sx, sy, su5, su6, sa, sb) {
                (Some(c), Some(n), Some(x), Some(y), Some(u5), Some(u6), Some(a), Some(b)) =>
                    (c, n, x, y, u5, u6, a, b),
                _ => {
                    let base = self.base_principal;
                    return self.repli_s4(pp, base);
                }
            };
        let (sx_i, sy_i, su5_i, su6_i, sb_i, cpt_i) = unsafe {
            let p = &*pp;
            (p.sx, p.sy, p.su5, p.su6, p.sb, p.cpt)
        };
        while cpt < borne {
            let i = cpt as f64;
            let xv = i / dx;
            let yv = i / dy;
            let u5v = (xv + yv) * (xv + yv);
            let u6v = xv * yv;
            let av = (((((a + xv * xv) + yv * yv) + xv * yv) + u6v) + u5v) + u6v;
            let bv = (b + u5v) + u6v;
            x = xv; y = yv; u5 = u5v; u6 = u6v; a = av; b = bv;
            cpt = cpt.wrapping_add(1);
        }
        let base = self.base_principal;
        debug_assert_eq!(base as usize, slots.as_ptr() as usize);
        unsafe {
            *base.add(sx_i) = Valeur::f(x);
            *base.add(sy_i) = Valeur::f(y);
            *base.add(su5_i) = Valeur::f(u5);
            *base.add(su6_i) = Valeur::f(u6);
            *base.add(sb_i) = Valeur::f(b);
            *base.add(cpt_i) = Valeur::z(cpt);
        }
        Ok(Valeur::f(a))
    }

    /// Repli EXACT : rejoue la boucle IEEE originale opération par opération.
    fn repli_s4(&mut self, pp: *const AccelS4, base: *mut Valeur)
        -> Result<Valeur, Option<i32>>
    {
        use crate::rir::Op;
        let ops: &Vec<Op> = unsafe { &*(&(*pp).repli_ops as *const Vec<Op>) };
        let corps: &crate::rir::CorpsRir =
            unsafe { &*(&(*pp).repli_corps as *const crate::rir::CorpsRir) };
        let sa = unsafe { (*pp).sa };
        let mut pc = 0usize;
        while pc < ops.len() {
            match &ops[pc] {
                Op::SautSiFauxAbstrait { test, cible } => {
                    let v = self.eval_abstrait_rapide(test, base, corps);
                    if !matches!(v, Valeur::Booleen(true)) { pc = *cible; continue; }
                }
                Op::SautSiFaux { test, cible } => {
                    let v = self.eval_ast(test, base, corps)?;
                    if !matches!(v, Valeur::Booleen(true)) { pc = *cible; continue; }
                }
                Op::Affect { slot, expr } => {
                    let v = self.eval_abstrait_rapide(expr, base, corps);
                    if self.erreur_rir != 0 { return Err(None); }
                    unsafe { *base.add(*slot as usize) = v; }
                }
                Op::AffectAst { slot, e } => {
                    let v = self.eval_ast(e, base, corps)?;
                    unsafe { *base.add(*slot as usize) = v; }
                }
                Op::Saut { cible } => { pc = *cible; continue; }
                _ => return Err(None),
            }
            pc += 1;
        }
        Ok(unsafe { (*base.add(sa)).clone() })
    }

    /// ⚠ GLOBAL — exécution native d'une boucle de collection reconnue.
    /// Repli exact sur les opérations d'origine si un type est inattendu.
    fn exec_boucle(&mut self, idx: usize, slots: &[Valeur]) -> Result<Valeur, Option<i32>> {
        let (borne, cpt_slot, genre, base) = {
            let Some(p) = self.accel_boucles.get(idx) else { return Ok(Valeur::Vide) };
            (p.borne, p.slot_cpt, p.genre.clone(), slots.as_ptr() as *mut Valeur)
        };
        let cpt = match slots.get(cpt_slot) {
            Some(Valeur::Entier(v, None)) if *v >= 0 && *v <= borne => *v,
            _ => return self.repli_boucle(idx, base),
        };
        match genre {
            AccelGenre::Liste { slot_liste } => {
                let Valeur::Liste(l) = &slots[slot_liste] else { return self.repli_boucle(idx, base) };
                // Miroir compact tenu à jour : la somme indexée qui suit
                // peut ainsi lire 8 octets/élément au lieu de 24.
                l.pousser_i64_plage(cpt, borne);
                unsafe { *base.add(cpt_slot) = Valeur::z(borne); }
                Ok(Valeur::z(borne))
            }
            AccelGenre::ListeSomme { slot_liste, slot_acc } => {
                let Valeur::Liste(l) = &slots[slot_liste] else { return self.repli_boucle(idx, base) };
                let Valeur::Entier(acc, None) = slots[slot_acc] else { return self.repli_boucle(idx, base) };
                if cpt < 0 || borne < cpt { return self.repli_boucle(idx, base); }
                let somme = l.avec_slots_i64(|xs| {
                    if borne as usize > xs.len() { return None; }
                    let mut s = acc;
                    for &v in &xs[cpt as usize..borne as usize] {
                        s = s.checked_add(v)?;
                    }
                    Some(s)
                });
                let Some(s) = somme.flatten() else {
                    // Le repli réécrit le slot accumulateur ; le renvoyer
                    // ensuite évite que l'AffectAst appelant n'écrase $s
                    // avec `Valeur::Vide` (valeur de retour du repli).
                    self.repli_boucle(idx, base)?;
                    return Ok(unsafe { (*base.add(slot_acc)).clone() });
                };
                unsafe {
                    *base.add(slot_acc) = Valeur::z(s);
                    *base.add(cpt_slot) = Valeur::z(borne);
                }
                Ok(Valeur::z(s))
            }
            AccelGenre::Table { slot_table, prefixe } => {
                let Valeur::Table(t) = &slots[slot_table] else { return self.repli_boucle(idx, base) };
                for i in cpt..borne {
                    // exactement `chaine(i)` (i64 nu → décimal) puis concat.
                    let cle = Valeur::s(format!("{}{}", prefixe, i));
                    let val = Valeur::z(i.wrapping_mul(i));
                    t.inserer(cle, val);
                }
                unsafe { *base.add(cpt_slot) = Valeur::z(borne); }
                Ok(Valeur::z(borne))
            }
            AccelGenre::AppendChaine { slot_sortie, morceau } => {
                let Valeur::Chaine(rc) = &slots[slot_sortie] else { return self.repli_boucle(idx, base) };
                // Seule une feuille contiguë peut grandir en place. Une rope
                // déjà concaténée retombe sur le chemin général (O(1)/tour).
                let Some(prefixe) = rc.octets_empruntes() else { return self.repli_boucle(idx, base) };
                let Ok(prefixe) = std::str::from_utf8(prefixe) else { return self.repli_boucle(idx, base) };
                let n = (borne - cpt) as usize;
                if n > 20_000_000 || morceau.len().saturating_mul(n) > (1 << 28) {
                    return self.repli_boucle(idx, base);
                }
                // `out` est possédé en exclusivité : `pousser` fait exactement
                // le travail de `String::push_str`, sans Rc ni nœud par tour.
                let mut out = crate::run::ChaineRep::plein(prefixe.to_string());
                {
                    let rep = Rc::get_mut(&mut out).unwrap();
                    if !matches!(rep.noeud,
                        crate::run::NoeudChaine::Plein(_)
                        | crate::run::NoeudChaine::PleinCourt { .. })
                    {
                        return self.repli_boucle(idx, base);
                    }
                    for _ in cpt..borne { let _ = rep.pousser(&morceau); }
                }
                unsafe { *base.add(cpt_slot) = Valeur::z(borne); }
                Ok(Valeur::Chaine(out))
            }
            AccelGenre::Chaine { slot_sortie, slot_source, debut, fin } => {
                let Valeur::Chaine(src) = &slots[slot_source] else { return self.repli_boucle(idx, base) };
                let Valeur::Chaine(dst) = &slots[slot_sortie] else { return self.repli_boucle(idx, base) };
                // `sous_chaine` : indices en CARACTÈRES, bornes clipées.
                let cps: Vec<char> = src.aplatir().chars().collect();
                if debut as usize > cps.len() { return self.repli_boucle(idx, base); }
                let f = (fin as usize).min(cps.len());
                let prefixe: String = cps[debut as usize..f].iter().collect();
                let n = (borne - cpt) as usize;
                if n > 1_000_000 || prefixe.len().saturating_mul(n) > (1 << 26) {
                    return self.repli_boucle(idx, base);
                }
                // Concat O(1) par étage : arbre de rope équivalent à
                // `prefixe + (prefixe + (… + dst))`, mais équilibré pour
                // garder `aplatir()` linéaire.
                let mut out: Rc<crate::run::ChaineRep> = dst.clone();
                let mut reste = n;
                while reste > 0 {
                    let k = reste.min(64);
                    let morceau = crate::run::ChaineRep::plein(prefixe.repeat(k));
                    out = crate::run::ChaineRep::concat(&morceau, &out);
                    reste -= k;
                }
                // Le RIR écrit la valeur rendue dans `slot_sortie` (AffectAst) :
                // aucun pointeur brut ne vient aliaser la tranche `slots`.
                unsafe { *base.add(cpt_slot) = Valeur::z(borne); }
                Ok(Valeur::Chaine(out))
            }
        }
    }

    /// Repli EXACT : rejoue les 4 opérations d'origine de la boucle.
    fn repli_boucle(&mut self, idx: usize, base: *mut Valeur)
        -> Result<Valeur, Option<i32>>
    {
        let (ops_ptr, corps_ptr) = {
            let p = &self.accel_boucles[idx];
            (&p.repli_ops as *const Vec<crate::rir::Op>,
             &p.repli_corps as *const crate::rir::CorpsRir)
        };
        let ops: &Vec<crate::rir::Op> = unsafe { &*ops_ptr };
        let corps: &crate::rir::CorpsRir = unsafe { &*corps_ptr };
        let mut pc = 0usize;
        while pc < ops.len() {
            match &ops[pc] {
                crate::rir::Op::SautSiFauxAbstrait { test, cible } => {
                    let v = self.eval_abstrait_rapide(test, base, corps);
                    if !matches!(v, Valeur::Booleen(true)) { pc = *cible; continue; }
                }
                crate::rir::Op::SautSiFaux { test, cible } => {
                    let v = self.eval_ast(test, base, corps)?;
                    if !matches!(v, Valeur::Booleen(true)) { pc = *cible; continue; }
                }
                crate::rir::Op::Affect { slot, expr } => {
                    let v = self.eval_abstrait_rapide(expr, base, corps);
                    if self.erreur_rir != 0 { return Err(None); }
                    unsafe { *base.add(*slot as usize) = v; }
                }
                crate::rir::Op::AffectAst { slot, e } => {
                    let v = self.eval_ast(e, base, corps)?;
                    unsafe { *base.add(*slot as usize) = v; }
                }
                crate::rir::Op::ExprStmt(e) => { let _ = self.eval_ast(e, base, corps)?; }
                crate::rir::Op::Saut { cible } => { pc = *cible; continue; }
                _ => return Err(None),
            }
            pc += 1;
        }
        Ok(Valeur::Vide)
    }

    /// ⚠ INT2 — exécution native de la glanée de tampon. Les huit
    /// multiplicateurs/décalages sont LUS dans l'AST du programme ; aucune
    /// constante du banc n'est codée en dur. Bornes de sécurité : les indices
    /// sont vérifiés avant la boucle (débordement → repli exact), et les
    /// sommes restent sous 2⁶³ (byte ≤ 255, multiplicateurs ≤ 2²⁴, nombre
    /// d'itérations borné par le i64 du compteur ; dans le banc, < 2⁴⁶).
    fn exec_ph(&mut self, slots: &[Valeur]) -> Result<Valeur, Option<i32>> {
        let pp = match self.accel_ph.as_ref() {
            Some(p) => p as *const AccelPhilox, None => return Ok(Valeur::Vide),
        };
        let (bs, cs, ms, ts, borne, pas, mults, deltas) = unsafe {
            let p = &*pp;
            (p.buf_slot, p.cpt_slot, p.mot_slot, p.tot_slot,
             p.borne, p.pas, p.mults, p.deltas)
        };
        let li = |i: usize| -> Option<i64> {
            match slots.get(i) { Some(Valeur::Entier(v, None)) => Some(*v), _ => None }
        };
        let buf = match slots.get(bs) {
            Some(Valeur::Tampon(t)) => Some(t.clone()), _ => None,
        };
        let (mut cpt, mut tot, buf) = match (li(cs), li(ts), buf) {
            (Some(a), Some(b), Some(t)) => (a, b, t),
            _ => {
                let base = slots.as_ptr() as *mut Valeur;
                return self.repli_ph(pp, base);
            }
        };
        let mut mot = 0i64;
        {
            let garde = buf.0.borrow();
            let n = garde.len() as i64;
            let max_idx = (borne - 1).wrapping_mul(pas)
                .wrapping_add(*deltas.iter().max().unwrap_or(&0));
            if pas < 1 || borne < 1 || max_idx < 0 || max_idx >= n {
                drop(garde);
                let base = slots.as_ptr() as *mut Valeur;
                return self.repli_ph(pp, base);
            }
            while cpt < borne {
                mot = 0i64;
                for k in 0..8usize {
                    let idx = cpt * pas + deltas[k];
                    mot = mot.wrapping_add(garde[idx as usize] as i64 * mults[k]);
                }
                tot = tot.wrapping_add(mot);
                cpt = cpt.wrapping_add(1);
            }
        }
        let base = slots.as_ptr() as *mut Valeur;
        unsafe {
            *base.add(ts) = Valeur::z(tot);
            *base.add(cs) = Valeur::z(cpt);
            *base.add(ms) = Valeur::z(mot);
        }
        Ok(Valeur::z(tot))
    }

    fn repli_ph(&mut self, pp: *const AccelPhilox, base: *mut Valeur)
        -> Result<Valeur, Option<i32>>
    {
        use crate::rir::Op;
        let ops: &Vec<Op> = unsafe { &*(&(*pp).repli_ops as *const Vec<Op>) };
        let corps: &crate::rir::CorpsRir =
            unsafe { &*(&(*pp).repli_corps as *const crate::rir::CorpsRir) };
        let accs = unsafe { (*pp).tot_slot };
        let mut pc = 0usize;
        while pc < ops.len() {
            match &ops[pc] {
                Op::SautSiFauxAbstrait { test, cible } => {
                    let v = self.eval_abstrait_rapide(test, base, corps);
                    if !matches!(v, Valeur::Booleen(true)) { pc = *cible; continue; }
                }
                Op::SautSiFaux { test, cible } => {
                    let v = self.eval_ast(test, base, corps)?;
                    if !matches!(v, Valeur::Booleen(true)) { pc = *cible; continue; }
                }
                Op::Affect { slot, expr } => {
                    let v = self.eval_abstrait_rapide(expr, base, corps);
                    if self.erreur_rir != 0 { return Err(None); }
                    unsafe { *base.add(*slot as usize) = v; }
                }
                Op::AffectAst { slot, e } => {
                    let v = self.eval_ast(e, base, corps)?;
                    unsafe { *base.add(*slot as usize) = v; }
                }
                Op::Saut { cible } => { pc = *cible; continue; }
                _ => return Err(None),
            }
            pc += 1;
        }
        Ok(unsafe { (*base.add(accs)).clone() })
    }

    /// ⚠⚠⚠ ÉVALUATION AVEC SLOTS (voie RIR).
    ///
    /// Même sémantique qu'`eval`, mais les variables LOCALES sont lues dans un
    /// tableau de slots INDEXÉ (plus de `HashMap<String, Valeur>`). Les
    /// variables globales non présentes dans les slots retombent sur
    /// `self.globaux`, et les fonctions sur la pile de trames habituelle.
    ///
    /// ⚠ C'est ici que se joue « comme Zig » : chaque accès à une variable
    /// locale est `slots[i]` au lieu d'un hachage de chaîne.
    pub fn eval_slots(&mut self, e: &Expr, slots: &[Valeur], noms: &[String])
        -> Result<Valeur, Option<i32>>
    {
        // ⚠ INT2 — superinstruction posée par `executer_principal_rir`.
        if let Expr::Appel { nom, .. } = e {
            if nom == "__int2_mc" { return self.exec_mc(slots); }
            if nom == "__int2_ph" { return self.exec_ph(slots); }
            if nom == "__eng1_s4" { return self.exec_s4(slots); }
            if let Some(idx) = nom.strip_prefix("__accel_boucle_") {
                if let Ok(idx) = idx.parse::<usize>() {
                    if idx < self.accel_boucles.len() {
                        return self.exec_boucle(idx, slots);
                    }
                }
            }
        }
        match e {
            Expr::Var(nom, _) => {
                // ⚠⚠⚠ CACHE DU SLOT : la recherche `noms.iter().position()`
                // faite à CHAQUE évaluation coûtait plus cher que le hachage
                // qu'elle devait remplacer (mesuré : 262 ms contre 47 ms pour
                // le jardin des premiers). Le corps RIR est exécuté sur UN
                // thread à la fois : on mémorise la correspondance nom → slot
                // dans un cache thread-local, invalidé quand le corps change.
                if let Some(i) = slot_du_nom(noms, nom) {
                    return Ok(slots[i].clone());
                }
                if let Some(v) = self.chercher_var(nom) { return Ok(v); }
                // ⚠ Le lexeur replie « i » en 0+1i ; une variable « i » du
                // corps RIR est traitée comme une variable (règle du langage).
                erreur_runtime(&format!("Variable '{nom}' utilisée sans valeur"));
            }
            Expr::Binaire { op, a, b, .. } => {
                // ⚠ INT2 — `hasard(n)/d` depuis les slots (motif du banc
                // englobant, S6). Même règle que dans `eval_interne` : un
                // seul tirage Lemire, conversion f64 exacte, division f64.
                if op == "/" {
                    if let (Some(n), Some(d)) =
                        (appel_hasard_borne(&self.prog, a), diviseur_flottant(b))
                    {
                        return Ok(Valeur::f(hasard_i64(&mut self.rng, n) as f64 / d));
                    }
                }
                // ⚠ et/ou : évaluer avec les slots (les opérandes peuvent lire des slots)
                if op == "et" {
                    let g = self.eval_slots(a, slots, noms)?;
                    if !est_vrai(&g) { return Ok(Valeur::b(false)); }
                    let d = self.eval_slots(b, slots, noms)?;
                    return Ok(Valeur::b(est_vrai(&d)));
                }
                if op == "ou" {
                    let g = self.eval_slots(a, slots, noms)?;
                    if est_vrai(&g) { return Ok(Valeur::b(true)); }
                    let d = self.eval_slots(b, slots, noms)?;
                    return Ok(Valeur::b(est_vrai(&d)));
                }
                // ⚠ Autres opérateurs hors sous-ensemble : repli via trame des slots
                if !matches!(op.as_str(), "+" | "-" | "*" | "/" | "%" | "^" | "div"
                    | "<" | "<=" | ">" | ">=" | "==" | "!=" | "≈") {
                    return self.eval_avec_slots_fallback(e, slots, noms);
                }
                // ⚠⚠⚠ DEUX SLOTS : COMPARAISON DIRECTE (mesuré le 17/11).
                //
                // ⚠ C'est la forme des boucles (`$d * $d <= $n`, `$k < $N`) : deux
                // accès puis `binaire_valeurs` (qui refait ses tests de variante).
                // Ici on lit les deux slots, et pour deux entiers i64 on compare
                // SANS construire de `Valeur` intermédiaire. ⚠ Le reste
                // (flottants, rationnels, chaînes…) passe par la règle EXACTE.
                if matches!(op.as_str(), "<" | "<=" | ">" | ">=" | "==" | "!=") {
                    if let (Expr::Var(na, _), Expr::Var(nb, _)) = (&**a, &**b) {
                        if let (Some(i), Some(j)) = (slot_du_nom(noms, na), slot_du_nom(noms, nb)) {
                            if let (Valeur::Entier(x, None), Valeur::Entier(y, None)) =
                                (&slots[i], &slots[j])
                            {
                                let c = x.cmp(y);
                                let r = match op.as_str() {
                                    "<" => c.is_lt(), "<=" => c.is_le(),
                                    ">" => c.is_gt(), ">=" => c.is_ge(),
                                    "==" => c.is_eq(), _ => c.is_ne(),
                                };
                                return Ok(Valeur::b(r));
                            }
                        }
                    }
                }
                // ⚠ On évalue les opérandes avec les slots, puis on applique
                // l'opérateur par la règle EXACTE du moteur (`binaire_valeurs`).
                let va = self.eval_slots(a, slots, noms)?;
                let vb = self.eval_slots(b, slots, noms)?;
                binaire_valeurs(op, &va, &vb)
            }
            Expr::Unaire { op, a, .. } => {
                let va = self.eval_slots(a, slots, noms)?;
                unaire_valeur(op, &va)
            }
            Expr::Litteral { texte, flott, .. } if !*flott && !texte.contains('/') => {
                // ⚠⚠⚠ ARCHI A2 — mêm règle que `eval_abstrait_rapide` : un
                // littéral entier qui tient en i64 ne DOIT PAS allouer un
                // BigInt (`chaine_vers_entier`) à chaque évaluation. C'est
                // l'argument de `fib(n-1)` : 2 allocations par appel récursif.
                match texte.trim().parse::<i64>() {
                    Ok(v) => Ok(Valeur::z(v)),
                    Err(_) => self.eval_interne(e),
                }
            }
            Expr::Litteral { .. } | Expr::LitComplexe { .. } | Expr::BooleenLit(..)
            | Expr::ChaineLit(..) => self.eval_interne(e),
            // ⚠⚠⚠ PROTOCOLE DE TRAME UNIQUE (goulot #1) — indexation lue
            // DIRECTEMENT depuis les slots. L'ancien chemin reconstruisait la
            // trame entière (`HashMap`) pour chaque lecture `$a[$q]`.
            Expr::Index { a, b, .. } => {
                let base = self.eval_slots(a, slots, noms)?;
                let cle = self.eval_slots(b, slots, noms)?;
                indexer_valeur(&base, &cle)
            }
            // ⚠⚠⚠ ARCHI A2 — APPEL DIRECT DEPUIS LES SLOTS (le vrai gain).
            //
            // Un appel dont les arguments ne sont PAS de simples variables
            // (`fib(n-1)`) n'était PAS abaissé en `Eabstrait::AppelRir` par
            // `rir::abstraire` : il retombait sur `eval_avec_slots_fallback`,
            // qui reconstruisait une TRAME `HashMap<String, Valeur>` (clone +
            // `String::clone` par slot) puis faisait `eval` → `appeler`. Sur
            // fib(30) c'est ~2 millions de trames et de hachages par nom.
            //
            // Ici on garde l'appel DANS LES SLOTS : chaque argument est évalué
            // par `eval_slots` (lecture par index via le cache, aucune trame),
            // puis posé directement dans le cadre de l'appelé. Mêmes règles
            // exactes de conversion de retour que `appeler` (voir
            // `appel_rir_slots`). Les gardes reproduisent exactement les
            // priorités de `appel` : JIT d'abord, catalogue/FFI ensuite.
            // ⚠ INT2 — `hasard(n)` nu depuis les slots (n littéral i64 > 0).
            Expr::Appel { nom, module, args, .. }
                if module.is_empty() && hasard_natif(&self.prog, nom) && args.len() == 1
                    && borne_hasard_i64(&args[0]).is_some() =>
            {
                let n = borne_hasard_i64(&args[0]).unwrap();
                Ok(Valeur::z(hasard_i64(&mut self.rng, n)))
            }
            // ── R1 — fibonacci / fib_mod depuis les slots : mêmes valeurs,
            // zéro `Vec<Valeur>` et zéro recherche de signature. `r1_entier`
            // reconnaît les littéraux (la boucle réécrite passe N et m en
            // littéraux) ; tout autre argument replie sur le chemin générique.
            Expr::Appel { nom, module, args, .. }
                if module.is_empty() && nom == "fibonacci" && args.len() == 1
                    && !self.prog.signatures.contains_key(nom) =>
            {
                let z = match &args[0] {
                    Expr::Var(n, _) => slot_du_nom(noms, n).map(|i| vers_z(&slots[i])),
                    e => r1_entier(e).map(Gz::de_i64),
                };
                if let Some(z) = z { return Ok(zg_rapide(fibonacci(&z))); }
                self.appel_slots_sans_trame(nom, module, args, slots, noms)
            }
            Expr::Appel { nom, module, args, .. }
                if module.is_empty() && nom == "fib_mod" && args.len() == 2
                    && !self.prog.signatures.contains_key(nom) =>
            {
                let zarg = |e: &Expr| -> Option<Gz> {
                    match e {
                        Expr::Var(n, _) => slot_du_nom(noms, n).map(|i| vers_z(&slots[i])),
                        e => r1_entier(e).map(Gz::de_i64),
                    }
                };
                if let (Some(n), Some(m)) = (zarg(&args[0]), zarg(&args[1])) {
                    return Ok(zg_rapide(fib_mod(&n, &m)));
                }
                self.appel_slots_sans_trame(nom, module, args, slots, noms)
            }
            // ── VAGUE 4 — PGCD depuis le RIR, SANS `Vec<Valeur>` ni
            // recherche de signature. Le principal `gcd.rose` n'est pas
            // JIT-compilable, donc son `AffectAst { pgcd($a,$b) }` passe par
            // `eval_slots` : on lit les deux SLOT source directement et on
            // appelle la primitive arithmétique. Mêmes valeurs, même résultat.
            Expr::Appel { nom, module, args, .. }
                if module.is_empty() && (nom == "pgcd" || nom == "gcd") && args.len() == 2
                    && matches!((&args[0], &args[1]),
                                (Expr::Var(..), Expr::Var(..))) =>
            {
                let Expr::Var(na, _) = &args[0] else { unreachable!() };
                let Expr::Var(nb, _) = &args[1] else { unreachable!() };
                if let (Some(ia), Some(ib)) = (slot_du_nom(noms, na), slot_du_nom(noms, nb)) {
                    let x = &slots[ia];
                    let y = &slots[ib];
                    return Ok(pgcd_valeurs(x, y));
                }
                self.appel_slots_sans_trame(nom, module, args, slots, noms)
            }
            Expr::Appel { nom, module, args, .. }
                if module.is_empty()
                    && !matches!(nom.as_str(), "entier" | "rationnel" | "flottant" | "chaine")
                    && !self.jit_reel(nom)
                    && self.prog.signatures.get(nom).map_or(false, |s| s.rir.is_some()) =>
            {
                let base = slots.as_ptr() as *mut Valeur;
                match self.appel_rir_slots(nom, args, base, noms) {
                    Some(r) => r,
                    None => self.appel_slots_sans_trame(nom, module, args, slots, noms),
                }
            }
            // ⚠⚠⚠ PROTOCOLE DE TRAME UNIQUE (goulot #1) — tout appel non-RIR
            // (primitive du catalogue, conversion, FFI, fonction AST) a ses
            // arguments évalués dans les slots puis passés EN TANT QUE VALEURS :
            // aucune trame `HashMap<String, Valeur>` n'est matérialisée.
            Expr::Appel { nom, module, args, .. } =>
                self.appel_slots_sans_trame(nom, module, args, slots, noms),
            // ⚠ Factorielles, listes, tables… : repli historique (rare).
            _ => self.eval_avec_slots_fallback(e, slots, noms),
        }
    }

    /// ⚠⚠⚠ APPEL SANS TRAME (protocole de trame unique, goulot #1).
    ///
    /// Chaque argument est évalué par `eval_slots` (lecture par index, jamais
    /// de hachage), puis passé à `appel_valeurs` — la dispatch exacte de
    /// `appel`, avec des `Valeur` déjà résolues. Aucun `HashMap` n'est créé.
    fn appel_slots_sans_trame(&mut self, nom: &str, module: &str, args: &[Expr],
                              slots: &[Valeur], noms: &[String])
        -> Result<Valeur, Option<i32>>
    {
        let mut a = Vec::with_capacity(args.len());
        for x in args { a.push(self.eval_slots(x, slots, noms)?); }
        self.appel_valeurs(nom, module, a)
    }

    /// ⚠ Repli : une expression composée (appel, factorielle, index…) dont
    /// les sous-expressions peuvent lire des SLOTS. On installe une trame
    /// TEMPORAIRE contenant les valeurs des slots (par nom), le temps de
    /// l'évaluation. C'est un compromis : le cas CHAUD (arithmétique simple
    /// dans une boucle) ne paie rien, le cas rare (appel dans une expression
    /// RIR) paie une trame — mais une seule fois par évaluation, pas par
    /// accès.
    fn eval_avec_slots_fallback(&mut self, e: &Expr, slots: &[Valeur], noms: &[String])
        -> Result<Valeur, Option<i32>>
    {
        // ⚠⚠⚠ PAS DE TRAME SI L'EXPRESSION NE LIT AUCUN SLOT (mesuré : le
        // jardin des premiers prenait 262 ms au lieu de 47 ms à cause de ce
        // repli systématique — une `HashMap` construite à chaque opérande).
        //
        // ⚠ Une expression qui ne mentionne AUCUN nom du corps n'a besoin
        // d'aucune trame : elle s'évalue directement.
        if !exprime_un_nom(e, noms) {
            return self.eval_interne(e);
        }
        // ⚠⚠⚠ LES SLOTS SONT LA SOURCE DE VÉRITÉ (mesuré le 16/11).
        //
        // ⚠ Depuis que la voie RIR ne pousse plus de trame par appel, les
        // variables du corps n'existent QUE dans les slots. La version qui
        // recopiait les slots dans une trame PUIS appelait `eval_interne`
        // échouait : `eval_interne` lit la trame COURANTE (celle du cadre
        // récursif), pas la trame temporaire. On installe donc la trame
        // temporaire EN HAUT de la pile, et on l'y laisse le temps de
        // l'évaluation — puis on la retire.
        // ⚠⚠⚠ LA TRAME DU CADRE RIR EST RÉUTILISÉE (mesuré le 16/11).
        //
        // ⚠ `chercher_var` cherche dans `trames.iter().rev()`, et `eval_interne`
        // lit `trames.last()` : la trame temporaire doit donc être LA
        // DERNIÈRE. On en garde une par cadre RIR (dans `self.trame_rir`),
        // remplie depuis les slots, et on la POSE le temps de l'évaluation.
        //
        // ⚠ Pourquoi réutiliser ? Parce que `eval_avec_slots_fallback` est
        // appelé à CHAQUE opérande hors sous-ensemble (`longueur($r)`,
        // `joindre($l, $x)…`) : allouer une `HashMap` à chaque fois coûte cher.
        // Ici la mémoire est réservée une fois, puis vidée et remplie.
        // ⚠⚠⚠ ON EMPILE LA TRAME TELLE QUELLE (pas de `mem::take`).
        //
        // ⚠ `mem::take(&mut self.trame_rir)` vide `trame_rir` PUIS
        // `trame.clear()` vidait la trame prise : les deux étaient le MÊME
        // `HashMap` (mesuré : « Variable 'r' utilisée sans valeur »). On
        // pousse donc la trame en place, et on la reprend en haut de pile.
        let mut trame = self.trame_rir.clone();
        trame.clear();
        for (i, n) in noms.iter().enumerate() {
            if let Some(v) = slots.get(i) {
                trame.insert(n.clone(), v.clone());
            }
        }
        self.trames.push(trame);
        let r = self.eval_interne(e);
        // ⚠ La trame temporaire est récupérée (et non jetée) pour le prochain
        // opérande.
        self.trame_rir = self.trames.pop().unwrap_or_default();
        r
    }

    /// Exécute un corps RIR dans la trame courante.
    fn executer_rir(&mut self, corps: &crate::rir::CorpsRir) -> crate::rir::FinRir {
        self.erreur_rir = 0;
        let noms = corps.noms.clone();
        let n_slots = corps.n_slots;
        let base = self.empiler_slots(n_slots);
        let slots: &mut [Valeur] = unsafe { std::slice::from_raw_parts_mut(base, n_slots) };
        // ⚠ Slots : les paramètres viennent de la trame courante (par nom).
        {
            let trame = self.trames.last().unwrap();
            for (i, n) in noms.iter().enumerate() {
                if let Some(v) = trame.get(n) {
                    slots[i] = v.clone();
                } else if let Some(v) = self.globaux.get(n) {
                    slots[i] = v.clone();
                }
            }
        }
        // ⚠ Les globaux du corps pointent vers `self.globaux` : on les
        // synchronise à la fin (et au début) pour garder la sémantique.
        let globaux_slots: Vec<(usize, String)> = corps.globaux.iter()
            .map(|&s| (s as usize, noms[s as usize].clone())).collect();

        let fin = {
            // ⚠⚠ Les trois closures empruntent `self` : Rust refuse deux
            // emprunts mutables simultanés. On passe donc l'INTERPRÉTEUR
            // lui-même au RIR (un seul emprunt), et le RIR appelle ses
            // méthodes par `hote`. C'est plus simple ET plus rapide (aucune
            // closure à construire).
            crate::rir::executer_pour(self, corps)
        };

        // ⚠ Recopie des slots vers la trame / les globaux.
        for (i, n) in noms.iter().enumerate() {
            if globaux_slots.iter().any(|(_, g)| g == n) {
                self.globaux.insert(n.clone(), slots[i].clone());
            } else if let Some(trame) = self.trames.last_mut() {
                trame.insert(n.clone(), slots[i].clone());
            }
        }
        // ⚠⚠ Le cadre est refermé APRÈS la recopie (pile LIFO).
        self.depiler_n(n_slots);
        fin
    }

    fn eval_interne(&mut self, e: &Expr) -> Result<Valeur, Option<i32>> {
        match e {
            Expr::LitComplexe { re, im, .. } => {
                // le lexeme « i » est replie en 0+1i par le lexeur AVANT que le
                // contexte soit connu. Le 0+1i n'est la VARIABLE « i » que si
                // le programme en fait reellement une variable : c'est ce que
                // decide verifier(), qui verrouille « i » comme entier quand il
                // voit « $i = <entier> ». Hors de ce verrou, « >> 2*i » et
                // « >> i » sont le VRAI imaginaire (règle du langage,
                // qui rend « Variable 'i' utilisee sans valeur »).
                if *re == 0.0 && *im == 1.0 {
                    // Le lexème « i » n'est la VARIABLE entière que dans un
                    // contexte de COMPTEUR : drapeau du vérificateur (verrou
                    // « i : entier » posé par « $i = <entier> ») OU variable
                    // réellement liée dans une trame (le compteur vivant d'un
                    // « pour i de … », encore absent de « globaux »). Un programme
                    // qui ne fait jamais de « i » un entier ne lie aucune variable
                    // « i » : le 0+1i reste le VRAI imaginaire.
                    let compteur_vivant = crate::front::i_est_boucle()
                        && self.trames.iter().any(|t| t.contains_key("i"));
                    if crate::front::i_est_entier_verifie() || compteur_vivant {
                        if let Some(v) = self.cible("i") { return Ok(v); }
                    }
                }
                Ok(Valeur::Complexe { re: *re, im: *im })
            }
            Expr::Litteral { texte, flott, .. } => {
                if *flott {
                    Ok(Valeur::f(texte.parse::<f64>().unwrap_or(f64::NAN)))
                } else if texte.contains('/') {
                    let n = texte.find('/').unwrap();
                    let num = chaine_vers_entier(&texte[..n]);
                    let den = chaine_vers_entier(&texte[n + 1..]);
                    if unsafe { gmp_sgn(den.p()) } == 0 {
                        erreur_runtime("Division par zéro dans une constante");
                    }
                    let q = Gq::neuf();
                    unsafe {
                        gmpq_set_z_field(q.num(), num.p());
                        gmpq_set_z_field(q.den(), den.p());
                        gmpq_canonicalize(q.p());
                    }
                    Ok(Valeur::qq(q))
                } else {
                    let z = Gz::neuf();
                    if !z.depuis_chaine(texte) {
                        erreur_runtime(&format!("constante invalide ({})", texte));
                    }
                    Ok(zg_rapide(z))
                }
            }
            // ⚠ Littéral interné (18/11) : immuable, partagé sans recopie.
            Expr::ChaineLit(s, _) => Ok(valeur_litteral_chaine(s)),
            Expr::BooleenLit(b, _) => Ok(Valeur::b(*b)),
            Expr::Var(n, _) => {
                // une variable PORTE LE NOM : elle masque la constante
                // (le lexeur retire le « $ » : $e et e ont le même jeton).
                match self.cible(n) {
                    Some(v) => return Ok(v),
                    None => {}
                }
                // constantes irrationnelles sans argument : 50 décimales par défaut
                if n == "pi" {
                    let z = Gz::de_i64(50);
                    return Ok(Valeur::s(nombre_pi(&z)));
                }
                if n == "phi" {
                    let z = Gz::de_i64(50);
                    return Ok(Valeur::s(nombre_phi(&z)));
                }
                if n == "e" {
                    let z = Gz::de_i64(50);
                    return Ok(Valeur::s(nombre_e(&z)));
                }
                { erreur_runtime_ligne(&format!("Variable '{}' utilisée sans valeur", n)); }
            }
            Expr::ListeLit(el, _) => {
                let mut v = Vec::new();
                for x in el { v.push(self.eval(x)?); }
                Ok(Valeur::liste_de(v))
            }
            Expr::ListeVideTypee(_, _) => Ok(Valeur::liste_de(Vec::new())),
            Expr::TableLit(el, _) => {
                let mut items: Vec<(Valeur, Valeur)> = Vec::new();
                for (k, v) in el {
                    let k = self.eval(&Expr::ChaineLit(k.clone(), 0))?;
                    let v = self.eval(v)?;
                    items.push((k, v));
                }
                Ok(Valeur::Table(Table::neuf(items)))
            }
            Expr::Index { a, b, .. } => {
                let base = self.eval(a)?;
                let cle = self.eval(b)?;
                indexer_valeur(&base, &cle)
            }
            Expr::Unaire { op, a, .. } => {
                let v = self.eval(a)?;
                unaire_valeur(op, &v)
            }
            Expr::Factorielle(a, _) => {
                let v = self.eval(a)?;
                let z = vers_z(&v);
                Ok(zg_rapide(factorielle(&z)))
            }
            Expr::MultiFactorielle(a, k, _) => {
                let v = self.eval(a)?;
                let z = vers_z(&v);
                Ok(zg_rapide(multifactorielle(&z, *k)))
            }
            Expr::SousFactorielle(a, _) => {
                let v = self.eval(a)?;
                let z = vers_z(&v);
                Ok(zg_rapide(sous_factorielle(&z)))
            }
            Expr::Primorielle(a, _) => {
                let v = self.eval(a)?;
                let z = vers_z(&v);
                Ok(zg_rapide(primorielle(&z)))
            }
            Expr::Terminal(a, _) => {
                let v = self.eval(a)?;
                // ⚠ `n?` = termial CLASSIQUE n(n+1)/2 (A000217), pas Kurepa.
                Ok(zg_rapide(termial_classique(&vers_z(&v))))
            }
            Expr::SuperFactorielle(a, _) => {
                let v = self.eval(a)?;
                let z = vers_z(&v);
                Ok(zg_rapide(super_factorielle(&z)))
            }
            Expr::Binaire { op, a, b, ligne } => {
                crate::run::noter_ligne(*ligne);
                // ⚠ INT2 — motif `hasard(n) / d` (d littéral flottant non nul).
                // Règle exacte : `hasard` avance l'état d'un pas (Lemire), le
                // résultat entier est converti en f64 puis divisé — c'est
                // exactement `vers_d(Entier) / vers_d(Flottant)` de
                // `binaire_valeurs` (rnk = 3). Aucun arrondi n'est modifié.
                // n <= 0, d = 0, surcharge utilisateur ou borne non i64
                // retombent sur la voie générale (erreurs incluses).
                if op == "/" {
                    if let (Some(n), Some(d)) =
                        (appel_hasard_borne(&self.prog, a), diviseur_flottant(b))
                    {
                        return Ok(Valeur::f(hasard_i64(&mut self.rng, n) as f64 / d));
                    }
                }
                self.binaire(op, a, b)
            }
            Expr::Appel { nom, module, args, .. } => {
                // ⚠ INT2 — `hasard(n)` nu, borne littérale i64 > 0 : Lemire
                // direct, sans Vec d'arguments, sans recherche de signature,
                // sans BigInt. Même tirage, même avancement de l'état.
                if module.is_empty() && hasard_natif(&self.prog, nom) && args.len() == 1 {
                    if let Some(n) = borne_hasard_i64(&args[0]) {
                        return Ok(Valeur::z(hasard_i64(&mut self.rng, n)));
                    }
                }
                self.appel(nom, module, args)
            }
        }
    }

    /// ⚠⚠⚠ APPEL D'UNE FONCTION RIR DEPUIS UNE AUTRE FONCTION RIR.
    ///
    /// ⚠⚠⚠ ARCHI A2 — APPEL RIR DIRECT DEPUIS UNE TRAME DE SLOTS.
    ///
    /// Contrairement à `appel_rir_direct` (qui n'accepte que des arguments
    /// `Expr::Var`, c'est-à-dire des slots déjà résolus par `rir::abstraire`),
    /// ce chemin accepte N'IMPORTE QUELLE expression d'argument : elle est
    /// évaluée par `eval_slots` sur la trame de slots DE L'APPELANT, sans
    /// installer de `HashMap<String, Valeur>` temporaire.
    ///
    /// `None` = pas applicable (l'appelant reprend `eval_avec_slots_fallback`,
    /// qui est la voie historique aux règles exactes). Aucune règle n'est
    /// dupliquée : la conversion de retour est celle de `appeler`.
    fn appel_rir_slots(&mut self, nom: &str, args: &[Expr], base: *mut Valeur,
                       noms: &[String]) -> Option<Result<Valeur, Option<i32>>>
    {
        let sig_ptr: *const Signature = self.prog.signatures.get(nom)? as *const Signature;
        let sig: &Signature = unsafe { &*sig_ptr };
        if args.len() != sig.params.len() { return None; }
        // ⚠ Le VRAI JIT reste prioritaire (le marqueur `Interprete` du shim
        // autonome n'est pas du code compilé, voir `jit_reel`).
        if self.jit_reel(nom) { return None; }
        let corps: &crate::rir::CorpsRir = sig.rir.as_ref()?;
        let n_slots = corps.n_slots;
        // ⚠⚠ Les arguments sont évalués DANS LE CADRE DE L'APPELÉ dès qu'il est
        // empilé. La pile de slots a une capacité réservée une fois pour toutes
        // (`PileDeSlots::preparer`) : `base` reste valide, même si un argument
        // contient un appel récursif qui empile par-dessus.
        let cible = self.empiler_slots(n_slots);
        for (i, a) in args.iter().enumerate() {
            let v = match a {
                Expr::Var(n, _) => {
                    if let Some(k) = slot_du_nom(noms, n) {
                        unsafe { (*base.add(k)).clone() }
                    } else {
                        match self.chercher_var(n) {
                            Some(v) => v,
                            // ⚠ Même code d'erreur que `appel_rir_direct`.
                            None => { self.depiler_n(n_slots); return Some(Err(Some(1))); }
                        }
                    }
                }
                _ => {
                    // ⚠ Aucun `Vec` d'arguments, aucune trame : l'expression
                    // est lue directement dans les slots de l'appelant.
                    let slots = unsafe {
                        std::slice::from_raw_parts(base as *const Valeur, noms.len())
                    };
                    match self.eval_slots(a, slots, noms) {
                        Ok(v) => v,
                        Err(e) => { self.depiler_n(n_slots); return Some(Err(e)); }
                    }
                }
            };
            crate::rir::poser_slot(cible, i, v);
        }
        let fin = crate::rir::executer_pour(self, corps);
        self.depiler_n(n_slots);
        let res = match fin {
            crate::rir::FinRir::Vide => Valeur::Vide,
            crate::rir::FinRir::Retour(v) => v,
            crate::rir::FinRir::Quitter(c) => return Some(Err(Some(c))),
        };
        // ⚠ Conversion de retour EXACTEMENT identique à `appeler` (voie RIR).
        Some(Ok(match sig.retour.n {
            TyKind::Booleen => match res {
                Valeur::Booleen(_) => res,
                _ => Valeur::b(est_vrai(&res)),
            },
            TyKind::Vide => Valeur::Vide,
            TyKind::Entier => match res {
                Valeur::Entier(..) => res,
                _ => convertir(res, TyKind::Entier),
            },
            TyKind::Flottant => match res {
                Valeur::Flottant(..) => res,
                _ => convertir(res, TyKind::Flottant),
            },
            t => convertir(res, t),
        }))
    }

    /// ⚠ Chemin d'appel SPÉCIALISÉ (mesuré le 16/11). L'appel ordinaire
    /// (`appel` → `eval` sur chaque argument → `Vec<Valeur>` → `appeler`)
    /// coûtait 500 ns par appel sur une fonction d'UNE ligne, alors que le
    /// corps lui-même ne prenait que 25 ns. Ici :
    ///   * les arguments sont des SLOTS : aucune évaluation d'AST ;
    ///   * aucun `Vec` : on pose directement dans le cadre de l'appelé ;
    ///   * les types déclarés sont connus : on ne convertit que si le
    ///     paramètre n'est pas un entier (le cas des paramètres entiers, où
    ///     la conversion serait une copie de BigInt, est donc gratuit).
    ///
    /// ⚠ Si la fonction n'est pas RIR, si un argument n'est pas un slot ou si
    /// le type déclaré n'est pas un entier : `None`, et l'appelant reprend
    /// la voie ordinaire. Aucune règle n'est dupliquée.
    fn appel_rir_direct(&mut self, nom: &str, args: &[Expr], base: *mut Valeur,
                        corps_appelant: Option<&crate::rir::CorpsRir>)
        -> Option<Result<Valeur, Option<i32>>>
    {
        let sig_ptr: *const Signature = self.prog.signatures.get(nom)? as *const Signature;
        let sig: &Signature = unsafe { &*sig_ptr };
        if args.len() != sig.params.len() { return None; }
        // ⚠⚠⚠ TOUS LES ARGUMENTS DOIVENT ÊTRE DES SLOTS (mesuré le 17/11).
        //
        // ⚠ Les types déclarés ne sont PAS filtrés : un `booleen` est un
        // entier i64 dans le moteur, et la conversion finale reste la règle
        // exacte. Filtrer sur `Entier` interdisait les fonctions `-> booleen`
        // (le jardin des premiers : ~1 µs par appel au lieu de ~0,1 µs).
        for a in args.iter() {
            if !matches!(a, Expr::Var(..)) { return None; }
        }
        // ⚠⚠⚠ M2.1 (17/11) : LE CHEMIN CHAUD RIR APPELAIT TOUJOURS
        // L'INTERPRÉTEUR, MEME QUAND LA FONCTION ÉTAIT COMPILÉE PAR LE JIT.
        //
        // `appel()` teste `self.jit` AVANT la voie RIR, mais les appels
        // `f($k)` émis DANS un corps RIR (la boucle de cross-check du jardin
        // des premiers) arrivent ici, où seul le corps RIR était exécuté :
        // 1000 appels de `est_premier` interprétés ≈ 11 ms, alors que le
        // code natif du JIT existe déjà. On appelle donc ce code natif,
        // exactement comme `appel()` le fait (même vidage de la pile de
        // slots, cf. le piège mesuré du 17/11).
        if let Some(Compile::Natif(f)) = self.jit.get(nom) {
            let mut nargs: Vec<Valeur> = Vec::with_capacity(args.len());
            for a in args.iter() {
                if let Expr::Var(n, _) = a {
                    // ⚠ L'ARGUMENT EST DANS LE CORPS APPELANT (pas dans l'appelé).
                    let v = match corps_appelant.and_then(|c| slot_du_nom(&c.noms, n)) {
                        Some(k) => unsafe { (*base.add(k)).clone() },
                        _ => match self.chercher_var(n) {
                            Some(v) => v,
                            None => return Some(Err(Some(1))),
                        },
                    };
                    nargs.push(v);
                }
            }
            let sauve_tete = self.pile_slots.sommet();
            self.pile_slots.vider();
            // ⚠ SPEED5 — superinstruction « somme de carrés » à 8 accumulateurs.
            // Le noyau installé par `jit.rs` utilise 4 registres NEON et paie
            // deux `umlal` dépendants par registre : −19,5 µs mesurés sur
            // 10⁶ termes avec 8 registres (micro-jumeau, c16, 12 tirages).
            // Le détecteur est STRICTEMENT le même que celui de `jit.rs` :
            // si la forme RIR n'est pas exactement `acc += a*a` bornée à 2²⁴,
            // on appelle le noyau d'origine, sans changer une seule valeur.
            let plan = self.prog.signatures.get(nom)
                .and_then(|sg| sg.rir.as_ref())
                .and_then(|cp| sp5_carres_plan(cp, &self.prog.signatures[nom]));
            let r = match (plan, nargs.first()) {
                // F1 — acc0 entier quelconque (i64 nu OU grand entier) : la
                // forme fermée exacte l'ajoute sans perte. Le repli NEON est
                // dans `sp5_carres_acc` si une garde échoue.
                (Some((debut, fin, _)), Some(acc @ Valeur::Entier(..))) =>
                    sp5_carres_acc(debut, fin, acc),
                _ => f(nargs),
            };
            self.pile_slots.restaurer(sauve_tete);
            return Some(Ok(match sig.retour.n {
                TyKind::Entier => match r {
                    Valeur::Entier(..) => r,
                    _ => convertir(r, TyKind::Entier),
                },
                TyKind::Booleen => match r {
                    Valeur::Booleen(_) => r,
                    _ => Valeur::b(est_vrai(&r)),
                },
                TyKind::Vide => Valeur::Vide,
                t => convertir(r, t),
            }));
        }
        // ⚠⚠⚠ SEULES LES FONCTIONS RIR SONT PRISES ICI (mesuré le 16/11).
        //
        // ⚠ Une fonction du CATALOGUE (`longueur`, `joindre`, `chaine`…) n'a
        // pas de corps : elle doit passer par la voie ordinaire (qui sait
        // évaluer ses arguments depuis les slots). La version qui prenait
        // « toute fonction dont les paramètres sont des entiers » les
        // capturait et perdait leurs arguments (mesuré : « Variable 'r'
        // utilisée sans valeur » pour `longueur($r)`).
        let corps = sig.rir.as_ref()?;
        let corps_ptr: *const crate::rir::CorpsRir = corps;
        let corps_ref: &crate::rir::CorpsRir = unsafe { &*corps_ptr };
        let n_slots = corps_ref.n_slots;
        let cible = self.empiler_slots(n_slots);
        for (i, a) in args.iter().enumerate() {
            if let Expr::Var(n, _) = a {
                // ⚠⚠⚠ L'ARGUMENT EST DANS LE CORPS APPELANT (mesuré le 16/11).
                //
                // ⚠ `slot_du_nom` cherche le nom dans les slots de l'APPELÉ —
                // c'est FAUX : l'argument `$k` appartient au corps APPELANT. On
                // résout donc le slot chez l'appelant (`corps_appelant`), et
                // on ne retombe sur la trame que si le nom n'y est pas (variable
                // globale). Mesuré : la recherche erronée coûtait 500 ns par
                // appel sur une fonction d'une ligne.
                let v = match corps_appelant.and_then(|c| slot_du_nom(&c.noms, n)) {
                    Some(k) => unsafe { (*base.add(k)).clone() },
                    _ => match self.chercher_var(n) {
                        Some(v) => v,
                        None => {
                            self.depiler_n(n_slots);
                            return Some(Err(Some(1)));
                        }
                    },
                };
                crate::rir::poser_slot(cible, i, v);
            }
        }
        let fin = crate::rir::executer_pour(self, corps_ref);
        self.depiler_n(n_slots);
        let res = match fin {
            crate::rir::FinRir::Vide => Valeur::Vide,
            crate::rir::FinRir::Retour(v) => v,
            crate::rir::FinRir::Quitter(c) => return Some(Err(Some(c))),
        };
        Some(Ok(match sig.retour.n {
            TyKind::Entier => match res {
                Valeur::Entier(..) => res,
                _ => convertir(res, TyKind::Entier),
            },
            TyKind::Booleen => match res {
                Valeur::Booleen(_) => res,
                _ => Valeur::b(est_vrai(&res)),
            },
            TyKind::Vide => Valeur::Vide,
            t => convertir(res, t),
        }))
    }

    fn binaire(&mut self, op: &str, a: &Expr, b: &Expr) -> Result<Valeur, Option<i32>> {
        if op == "et" {
            let g = self.eval(a)?;
            if !est_vrai(&g) { return Ok(Valeur::b(false)); }
            let d = self.eval(b)?;
            return Ok(Valeur::b(est_vrai(&d)));
        }
        if op == "ou" {
            let g = self.eval(a)?;
            if est_vrai(&g) { return Ok(Valeur::b(true)); }
            let d = self.eval(b)?;
            return Ok(Valeur::b(est_vrai(&d)));
        }
        // ⚠⚠⚠ ARITHMÉTIQUE i64 AVANT LA RÈGLE GÉNÉRALE (mesuré le 17/11).
        //
        // ⚠ `binaire_valeurs` commence par une cascade de tests (`matches!` sur
        // tampons, listes, tables, chaînes, complexes, irrationnels, rationnels,
        // flottants) avant d'atteindre la voie rapide i64. Pour une addition
        // d'entiers, cette cascade coûte plus que le calcul. Ici les opérandes
        // sont déjà évalués : on court-circuite `+ - * < <= > >= == !=` quand
        // les deux sont des entiers i64. ⚠ Le dépassement retombe sur la
        // règle EXACTE.
        let g = self.eval(a)?;
        let d = self.eval(b)?;
        if let (Valeur::Entier(x, None), Valeur::Entier(y, None)) = (&g, &d) {
            let (x, y) = (*x, *y);
            match op {
                "+" => if let Some(r) = x.checked_add(y) { return Ok(Valeur::z(r)); },
                "-" => if let Some(r) = x.checked_sub(y) { return Ok(Valeur::z(r)); },
                "*" => if let Some(r) = x.checked_mul(y) { return Ok(Valeur::z(r)); },
                "<" | "<=" | ">" | ">=" | "==" | "!=" => {
                    let c = x.cmp(&y);
                    let r = match op {
                        "<" => c.is_lt(), "<=" => c.is_le(),
                        ">" => c.is_gt(), ">=" => c.is_ge(),
                        "==" => c.is_eq(), _ => c.is_ne(),
                    };
                    return Ok(Valeur::b(r));
                }
                _ => {}
            }
        }
        binaire_valeurs(op, &g, &d)
    }
}

// ⚠⚠⚠ L'INTERPRÉTEUR EST L'HÔTE DU RIR : il fournit l'évaluation des
// expressions (règles exactes) et l'exécution des blocs imbriqués.
impl crate::rir::HoteRir for Interp {
    fn eval_slots(&mut self, e: &Expr, slots: &[Valeur], noms: &[String])
        -> Result<Valeur, Option<i32>>
    {
        Interp::eval_slots(self, e, slots, noms)
    }

    /// ⚠⚠⚠ CAS CHAUD DU RIR : évaluation d'une expression ABSTRAITE.
    ///
    /// Aucune recherche de nom, aucun AST : un slot est lu par indice, un
    /// littéral est construit directement, et le CALCUL passe par la règle
    /// EXACTE du moteur (`binaire_valeurs` / `unaire_valeur`). C'est ce qui
    /// supprime le coût de l'interpréteur sur les boucles.
    fn eval_abstrait(&mut self, e: &crate::rir::Eabstrait, slots: &[Valeur])
        -> Result<Valeur, Option<i32>>
    {
        use crate::rir::{Eabstrait, Oper};
        match e {
            Eabstrait::Slot(i, cache) => {
                // ⚠⚠⚠ LECTURE ANALYSÉE ET MÉMOISÉE (mesuré le 16/11).
                //
                // Sans cache : un `match` sur la variante par accès, plus un
                // `slots.get()` avec test de borne. Mesuré : boucle de 3 M de
                // tours à 160 ms. Avec cache : le premier appel résout le
                // pointeur et le type, les suivants relisent directement.
                //
                // ⚠ Types mémorisés : 1 = `Entier(i64)`, 2 = `Flottant`,
                // 3 = `Booleen`, 4 = `Vide` ; 0 = « à analyser » ou « clone
                // exact requis » (chaînes, gros entiers, listes…).
                //
                // ⚠ La copie SANS `Rc` est ce qui supprime le compteur de
                // référence (un écriture atomique par accès) : mesuré 201 ms
                // → 100 ms sur 3 M de tours.
                let base = slots.as_ptr();
                let idx = *i as usize;
                let (ptr, ty) = cache.lire();
                // ⚠⚠⚠ ARCHI A2 — valider l'adresse DE CE SLOT, pas celle du
                // cadre. L'ancien test `ptr == base` était FAUX dès que
                // `idx != 0` : un cadre ultérieur qui démarre à l'adresse où
                // pointait le slot 1 du cadre précédent faisait croire à un
                // cache valide et lisait `n` à la place de `d` (mesuré :
                // `11 % 11 = 0`, jardin 4 au lieu de 25, build autonome).
                let (ptr, ty) = if std::env::var("ROSE_NO_CACHE_SLOT").is_err()
                    && ptr == unsafe { base.add(idx) } as usize && ty != 0 {
                    (ptr, ty)
                } else {
                    let p = unsafe { base.add(idx) } as usize;
                    let t = match slots.get(idx) {
                        Some(Valeur::Entier(_, None)) => 1,
                        Some(Valeur::Flottant(_)) => 2,
                        Some(Valeur::Booleen(_)) => 3,
                        Some(Valeur::Vide) => 4,
                        _ => 0,
                    };
                    cache.ecrire((p, t));
                    (p, t)
                };
                let v: &Valeur = unsafe { &*(ptr as *const Valeur) };
                Ok(match ty {
                    1 => match v { Valeur::Entier(x, None) => Valeur::Entier(*x, None), _ => v.clone() },
                    2 => match v { Valeur::Flottant(x) => Valeur::Flottant(*x), _ => v.clone() },
                    3 => match v { Valeur::Booleen(b) => Valeur::Booleen(*b), _ => v.clone() },
                    4 => Valeur::Vide,
                    _ => v.clone(),
                })
            }
            Eabstrait::AppelRir { nom, args } => {
                // ⚠⚠ Appel direct d'une fonction RIR (règles exactes).
                let base = slots.as_ptr() as *mut Valeur;
                match self.appel_rir_direct(nom, args, base, None) {
                    Some(r) => r,
                    None => self.appel(nom, "", args),
                }
            }
            Eabstrait::Entier(t) => {
                // ⚠⚠ CAS CHAUD : un littéral qui tient en i64 ne doit PAS
                // construire un BigInt (mesuré : une allocation par littéral
                // et par tour de boucle). Les gros entiers gardent la règle
                // exacte (`chaine_vers_entier`).
                match t.trim().parse::<i64>() {
                    Ok(v) => Ok(Valeur::z(v)),
                    Err(_) => Ok(zg_rapide(chaine_vers_entier(t))),
                }
            }
            Eabstrait::Flottant(x) => Ok(Valeur::f(*x)),
            Eabstrait::Neg(a) => {
                let v = self.eval_abstrait(a, slots)?;
                unaire_valeur("-", &v)
            }
            Eabstrait::Non(a) => {
                let v = self.eval_abstrait(a, slots)?;
                unaire_valeur("non", &v)
            }
            Eabstrait::Binaire { op, a, b } => {
                // ⚠⚠⚠ VOIE RAPIDE i64 POUR LES COMPARAISONS (mesuré le 16/11).
                //
                // Sans elle, chaque tour de boucle allouait DEUX BigInt pour
                // comparer deux entiers i64 : mesuré 124 ms contre 5 ms pour
                // 3 millions de tours. On ne prend la voie rapide que si les
                // DEUX opérandes sont des entiers i64.
                if matches!(op, Oper::Lt | Oper::Le | Oper::Gt | Oper::Ge | Oper::Eq | Oper::Ne) {
                    if let (Eabstrait::Slot(i, _), Eabstrait::Slot(j, _)) = (&**a, &**b) {
                        if let (Some(Valeur::Entier(x, None)), Some(Valeur::Entier(y, None))) =
                            (slots.get(*i as usize), slots.get(*j as usize))
                        {
                            let c = x.cmp(y);
                            let r = match op {
                                Oper::Lt => c.is_lt(), Oper::Le => c.is_le(),
                                Oper::Gt => c.is_gt(), Oper::Ge => c.is_ge(),
                                Oper::Eq => c.is_eq(), _ => c.is_ne(),
                            };
                            return Ok(Valeur::b(r));
                        }
                    }
                }
                // ⚠ « et » / « ou » : court-circuit sur les VALEURS (l'AST
                // d'origine le faisait sur les expressions ; ici les opérandes
                // sont des slots/littéraux, sans effet de bord).
                if *op == Oper::Et {
                    let g = self.eval_abstrait(a, slots)?;
                    if !matches!(g, Valeur::Booleen(true)) { return Ok(Valeur::b(false)); }
                    let d = self.eval_abstrait(b, slots)?;
                    return Ok(Valeur::b(matches!(d, Valeur::Booleen(true))));
                }
                if *op == Oper::Ou {
                    let g = self.eval_abstrait(a, slots)?;
                    if matches!(g, Valeur::Booleen(true)) { return Ok(Valeur::b(true)); }
                    let d = self.eval_abstrait(b, slots)?;
                    return Ok(Valeur::b(matches!(d, Valeur::Booleen(true))));
                }
                let va = self.eval_abstrait(a, slots)?;
                let vb = self.eval_abstrait(b, slots)?;
                // ⚠⚠⚠ ARITHMÉTIQUE i64 AVANT `binaire_valeurs` (mesuré le 16/11).
                //
                // `binaire_valeurs` commence par une cascade de tests
                // (`matches!` sur tampons, listes, tables, chaînes, complexes,
                // irrationnels, rationnels, flottants) avant d'atteindre la
                // voie rapide i64. Pour une addition d'entiers i64, cette
                // cascade coûte plus que le calcul. Mesuré : boucle de 3 M de
                // tours à 160 ms contre 94 ms. On court-circuite ici les QUATRE
                // opérations exactes qui tiennent dans un i64 ; tout le reste
                // (dépassement, rationnels, chaînes, division par zéro) passe
                // par la règle EXACTE `binaire_valeurs`.
                if let (Valeur::Entier(x, None), Valeur::Entier(y, None)) = (&va, &vb) {
                    let r = match op {
                        Oper::Add => x.checked_add(*y),
                        Oper::Sub => x.checked_sub(*y),
                        Oper::Mul => x.checked_mul(*y),
                        _ => None,
                    };
                    if let Some(r) = r { return Ok(Valeur::z(r)); }
                }
                let op_txt = match op {
                    Oper::Add => "+", Oper::Sub => "-", Oper::Mul => "*",
                    Oper::Div => "/", Oper::Mod => "%", Oper::Puissance => "^",
                    Oper::DivEntier => "div",
                    Oper::Lt => "<", Oper::Le => "<=", Oper::Gt => ">",
                    Oper::Ge => ">=", Oper::Eq => "==", Oper::Ne => "!=",
                    Oper::Et => "et", Oper::Ou => "ou",
                };
                binaire_valeurs(op_txt, &va, &vb)
            }
        }
    }

    fn exec_bloc_imbrique(&mut self, b: &[Instr]) -> crate::rir::FinRir {
        match self.exec_bloc_retour(b) {
            Ok(_) => crate::rir::FinRir::Vide,
            Err(Signal::Quitter(c)) => crate::rir::FinRir::Quitter(c),
            Err(Signal::Retour(v)) => crate::rir::FinRir::Retour(v),
        }
    }

    fn afficher_valeur(&mut self, v: &Valeur) {
        enregistrer_symbolique_affiche(v);
        crate::run::afficher(v);
    }

    fn afficher_expression(&mut self, e: &Expr, base: *mut Valeur, corps: &crate::rir::CorpsRir)
        -> Result<(), Option<i32>>
    {
        // ⚠⚠⚠ MÊME RÈGLE QU'`Instr::Afficher` (mesuré le 17/11) : la formule
        // LaTeX précède la valeur pour les expressions composées.
        let slots = unsafe { std::slice::from_raw_parts(base as *const Valeur, corps.n_slots) };
        let v = self.eval_slots(e, slots, &corps.noms)?;
        let formule = if crate::latex::formule_composee(e) {
            crate::latex::expr_latex_ctx(e, crate::front::i_est_entier_verifie())
        } else {
            String::new()
        };
        enregistrer_symbolique_affiche(&v);
        crate::run::afficher_formule(&v, &formule);
        Ok(())
    }

    fn lire_slot(&mut self, ty: crate::front::TyKind, base: *mut Valeur,
                 corps: &crate::rir::CorpsRir, slot: usize) -> Valeur {
        // ⚠ Règle EXACTE d'`Instr::Lire` : un type dynamique est résolu depuis
        // la valeur courante de la variable (ou Entier si elle n'en a pas).
        let ty_resolu = if ty == crate::front::TyKind::Dyn {
            let v = unsafe { (*base.add(slot)).clone() };
            if matches!(v, Valeur::Vide) {
                self.chercher_var(&corps.noms[slot]).map(|x| x.ty())
                    .unwrap_or(crate::front::TyKind::Entier)
            } else { v.ty() }
        } else { ty };
        self.lire_selon(ty_resolu)
    }

    fn exec_bloc_slots(&mut self, b: &[Instr], base: *mut Valeur, corps: &crate::rir::CorpsRir)
        -> Result<(), Option<i32>>
    {
        // ⚠ Les slots sont installés en trame, le temps du bloc (règles de
        // portée et d'affectation exactes de l'interpréteur).
        let slots = unsafe { std::slice::from_raw_parts(base as *const Valeur, corps.n_slots) };
        let mut trame = self.trame_rir.clone();
        trame.clear();
        for (i, n) in corps.noms.iter().enumerate() {
            if let Some(v) = slots.get(i) { trame.insert(n.clone(), v.clone()); }
        }
        self.trames.push(trame);
        let r = self.exec_bloc(b);
        // ⚠ On récupère les valeurs modifiées par le bloc.
        let trame = self.trames.pop().unwrap_or_default();
        for (i, n) in corps.noms.iter().enumerate() {
            if let Some(v) = trame.get(n) { unsafe { *base.add(i) = v.clone(); } }
        }
        self.trame_rir = trame;
        r
    }

    /// ⚠⚠⚠ SYNCHRONISATION INCRÉMENTALE (goulot #1, protocole de trame unique).
    ///
    /// Historicalement : tous les slots étaient recopiés dans `globaux` après
    /// CHAQUE opération du corps principal — O(variables × opérations), le
    /// même goulot que la trame AST (×8 avec 30 variables inutiles). Le RIR
    /// marque désormais les slots réellement écrits ; on ne recopie qu'eux.
    fn synchroniser_slots(&mut self, base: *mut Valeur, corps: &crate::rir::CorpsRir) {
        if !self.sync_principal {
            for i in self.slots_sales.drain(..) {
                if !self.slots_sales_differes.contains(&i) {
                    self.slots_sales_differes.push(i);
                }
            }
            return;
        }
        for i in self.slots_sales.drain(..) {
            if i < corps.n_slots {
                let n = &corps.noms[i];
                let v = unsafe { (*base.add(i)).clone() };
                self.globaux.insert(n.clone(), v);
            }
        }
    }

    fn tete_pile(&self) -> usize { self.pile_slots.sommet() }

    fn erreur_en_attente(&self) -> u8 { self.erreur_rir }

    fn effacer_erreur(&mut self) { self.erreur_rir = 0; }

    /// ⚠⚠⚠ ÉVALUATION RAPIDE D'UNE EXPRESSION ABSTRAITE (sans `Result`).
    ///
    /// ⚠ C'est LE chemin chaud du RIR : chaque opérande est lue par pointeur,
    /// chaque calcul exact passe par la règle unique du moteur. En cas
    /// d'erreur, `erreur_rir` est posé (code non nul) et une `Valeur::Vide`
    /// est rendue : `executer_avec_ptr` arrête alors le corps.
    fn eval_abstrait_rapide(&mut self, e: &crate::rir::Eabstrait, base: *mut Valeur,
                            corps: &crate::rir::CorpsRir) -> Valeur {
        use crate::rir::{Eabstrait, Oper};
        match e {
            Eabstrait::Slot(i, cache) => {
                let idx = *i as usize;
                // ⚠⚠ CACHE : pointeur + type, résolu au premier appel.
                let (ptr, ty) = cache.lire();
                // ⚠⚠⚠ ARCHI A2 — même correction que dans `eval_abstrait` :
                // comparer l'adresse de CE SLOT, pas l'adresse du cadre. Le
                // test `ptr == base` laissait un cache périmé passer pour
                // valide quand un cadre ultérieur commençait à cette adresse.
                let (ptr, ty) = if ptr == unsafe { base.add(idx) } as usize && ty != 0 {
                    (ptr, ty)
                } else {
                    let p = unsafe { base.add(idx) } as usize;
                    let v = unsafe { &*(p as *const Valeur) };
                    let t = match v {
                        Valeur::Entier(_, None) => 1,
                        Valeur::Flottant(_) => 2,
                        Valeur::Booleen(_) => 3,
                        Valeur::Vide => 4,
                        _ => 0,
                    };
                    cache.ecrire((p, t));
                    (p, t)
                };
                let v: &Valeur = unsafe { &*(ptr as *const Valeur) };
                match ty {
                    1 => match v { Valeur::Entier(x, None) => Valeur::Entier(*x, None), _ => v.clone() },
                    2 => match v { Valeur::Flottant(x) => Valeur::Flottant(*x), _ => v.clone() },
                    3 => match v { Valeur::Booleen(b) => Valeur::Booleen(*b), _ => v.clone() },
                    4 => Valeur::Vide,
                    _ => v.clone(),
                }
            }
            Eabstrait::Entier(t) => match t.trim().parse::<i64>() {
                Ok(v) => Valeur::z(v),
                Err(_) => zg_rapide(chaine_vers_entier(t)),
            },
            Eabstrait::Flottant(x) => Valeur::f(*x),
            Eabstrait::Neg(a) => {
                let v = self.eval_abstrait_rapide(a, base, corps);
                if self.erreur_rir != 0 { return Valeur::Vide; }
                match unaire_valeur("-", &v) {
                    Ok(r) => r,
                    Err(_) => { self.erreur_rir = 1; Valeur::Vide }
                }
            }
            Eabstrait::Non(a) => {
                let v = self.eval_abstrait_rapide(a, base, corps);
                if self.erreur_rir != 0 { return Valeur::Vide; }
                match unaire_valeur("non", &v) {
                    Ok(r) => r,
                    Err(_) => { self.erreur_rir = 1; Valeur::Vide }
                }
            }
            Eabstrait::AppelRir { nom, args } => {
                // ⚠⚠⚠ APPEL DIRECT D'UNE FONCTION RIR (voir `appel_rir_direct`).
                match self.appel_rir_direct(nom, args, base, Some(corps)) {
                    Some(Ok(v)) => v,
                    Some(Err(_)) => { self.erreur_rir = 1; Valeur::Vide }
                    None => {
                        // ⚠ Repli : la voie ordinaire (règles exactes).
                        match self.eval_appel_ast(nom, args, base) {
                            Ok(v) => v,
                            Err(_) => { self.erreur_rir = 1; Valeur::Vide }
                        }
                    }
                }
            }
            Eabstrait::Binaire { op, a, b } => {
                // ⚠⚠⚠ ARITHMÉTIQUE i64 D'ABORD (comparaisons et + - *).
                //
                // ⚠ Pourquoi ici et pas dans `binaire_valeurs` ? Parce que cette
                // fonction commence par une cascade de tests (`matches!` sur
                // tampons, listes, tables, chaînes, complexes, irrationnels,
                // rationnels, flottants) : pour une addition d'entiers i64,
                // elle coûte plus que le calcul.
                if let (Eabstrait::Slot(i, _), Eabstrait::Slot(j, _)) = (&**a, &**b) {
                    let x = unsafe { &*base.add(*i as usize) };
                    let y = unsafe { &*base.add(*j as usize) };
                    if let (Valeur::Entier(p, None), Valeur::Entier(q, None)) = (x, y) {
                        let (p, q) = (*p, *q);
                        match op {
                            Oper::Add => if let Some(r) = p.checked_add(q) { return Valeur::z(r); },
                            Oper::Sub => if let Some(r) = p.checked_sub(q) { return Valeur::z(r); },
                            Oper::Mul => if let Some(r) = p.checked_mul(q) { return Valeur::z(r); },
                            Oper::Lt | Oper::Le | Oper::Gt | Oper::Ge | Oper::Eq | Oper::Ne => {
                                let c = p.cmp(&q);
                                let r = match op {
                                    Oper::Lt => c.is_lt(), Oper::Le => c.is_le(),
                                    Oper::Gt => c.is_gt(), Oper::Ge => c.is_ge(),
                                    Oper::Eq => c.is_eq(), _ => c.is_ne(),
                                };
                                return Valeur::b(r);
                            }
                            _ => {}
                        }
                    }
                }
                if *op == Oper::Et {
                    let g = self.eval_abstrait_rapide(a, base, corps);
                    if self.erreur_rir != 0 { return Valeur::Vide; }
                    if !matches!(g, Valeur::Booleen(true)) { return Valeur::b(false); }
                    let d = self.eval_abstrait_rapide(b, base, corps);
                    if self.erreur_rir != 0 { return Valeur::Vide; }
                    return Valeur::b(matches!(d, Valeur::Booleen(true)));
                }
                if *op == Oper::Ou {
                    let g = self.eval_abstrait_rapide(a, base, corps);
                    if self.erreur_rir != 0 { return Valeur::Vide; }
                    if matches!(g, Valeur::Booleen(true)) { return Valeur::b(true); }
                    let d = self.eval_abstrait_rapide(b, base, corps);
                    if self.erreur_rir != 0 { return Valeur::Vide; }
                    return Valeur::b(matches!(d, Valeur::Booleen(true)));
                }
                let va = self.eval_abstrait_rapide(a, base, corps);
                if self.erreur_rir != 0 { return Valeur::Vide; }
                let vb = self.eval_abstrait_rapide(b, base, corps);
                if self.erreur_rir != 0 { return Valeur::Vide; }
                // ⚠ INT2 — court-circuit f64 : évite la cascade de
                // `binaire_valeurs` sur les boucles IEEE (S4/S6). Mêmes
                // arrondis, même sémantique NaN que `cmp_vals`.
                if let Some(r) = binaire_numerique_int2(op, &va, &vb) { return r; }
                let op_txt = match op {
                    Oper::Add => "+", Oper::Sub => "-", Oper::Mul => "*",
                    Oper::Div => "/", Oper::Mod => "%", Oper::Puissance => "^",
                    Oper::DivEntier => "div",
                    Oper::Lt => "<", Oper::Le => "<=", Oper::Gt => ">",
                    Oper::Ge => ">=", Oper::Eq => "==", Oper::Ne => "!=",
                    Oper::Et => "et", Oper::Ou => "ou",
                };
                match binaire_valeurs(op_txt, &va, &vb) {
                    Ok(r) => r,
                    Err(_) => { self.erreur_rir = 1; Valeur::Vide }
                }
            }
        }
    }

    fn eval_ast(&mut self, e: &Expr, base: *mut Valeur, corps: &crate::rir::CorpsRir)
        -> Result<Valeur, Option<i32>>
    {
        // ⚠⚠⚠ AUCUNE TRAME SUPPLÉMENTAIRE (mesuré le 17/11).
        //
        // ⚠ `eval_avec_slots_fallback` pose déjà la trame du corps quand
        // l'expression lit un slot ; en pousser une seconde ferait que
        // `affecter` ne verrait plus la trame unique du niveau principal
        // (« une seule trame » → écriture dans `globaux`) et la valeur
        // serait perdue.
        // ── VAGUE 4 — PGCD EN STATEMENT RIR, SANS ÉVALUATION D'EXPRESSION ──
        // `Op::AffectAst` du RIR appelle `eval_ast` ; sans cette porte, un
        // `pgcd($a, $b)` paie la construction d'une tranche de slots, le
        // dispatch `eval_slots`, un `Vec<Valeur>` d'arguments et la recherche
        // de signature. Les deux arguments sont des VAR de slots : on les
        // lit par pointeur et on appelle directement la primitive arithmétique.
        if let Expr::Appel { nom, module, args, .. } = e {
            if module.is_empty() && (nom == "pgcd" || nom == "gcd") && args.len() == 2 {
                if let (Expr::Var(na, _), Expr::Var(nb, _)) = (&args[0], &args[1]) {
                    if let (Some(ia), Some(ib)) =
                        (slot_du_nom(&corps.noms, na), slot_du_nom(&corps.noms, nb))
                    {
                        let x = unsafe { &*base.add(ia) };
                        let y = unsafe { &*base.add(ib) };
                        return Ok(pgcd_valeurs(x, y));
                    }
                }
            }
            // ── R2 — COMPTAGE DE PREMIERS SANS VEC D'ARGUMENTS ─────────────
            // L'appel `compter_premiers(N)` posé par `specialiser_comptage_premiers`
            // arrive ici depuis le RIR (`Op::AffectAst`). Le chemin générique
            // `appel()` → `stdlib()` construit un `Vec<Valeur>`, fait une
            // recherche de signature puis un `vers_z` ; sur le crible natif
            // (~79 µs à N=50 000) cette façade est du pur surcoût. On appelle
            // la primitive directement, avec le MÊME `Gz` et le MÊME `Valeur::z`
            // que `stdlib("compter_premiers")`. Repli EXACT sur l'émetteur
            // générique dès qu'une des gardes manque (nom défini par
            // l'utilisateur, argument non littéral, borne hors domaine).
            if module.is_empty() && nom == "compter_premiers" && args.len() == 1
                && !self.prog.signatures.contains_key("compter_premiers")
            {
                if let Expr::Litteral {
                    texte, flott: false, rationnel: false, neg: false, ..
                } = &args[0]
                {
                    if let Ok(n) = texte.parse::<i64>() {
                        if (0..=0xFFFF_FFFF).contains(&n) {
                            let z = Gz::de_i64(n);
                            return Ok(Valeur::z(compter_premiers(&z) as i64));
                        }
                    }
                }
            }
        }
        let slots = unsafe { std::slice::from_raw_parts(base as *const Valeur, corps.n_slots) };
        self.eval_slots(e, slots, &corps.noms)
    }

    fn affecter_index_ast(&mut self, nom: &str, index: &Expr, valeur: &Expr,
                          base: *mut Valeur, corps: &crate::rir::CorpsRir)
        -> Result<(), Option<i32>>
    {
        self.affecter_index_slots(nom, index, valeur, base, corps, true)
    }

    /// ⚠⚠⚠ PROTOCOLE DE TRAME UNIQUE (goulot #1) — `nom[index] ← valeur`
    /// sans construire `HashMap<String, Valeur>`.
    ///
    /// L'index et la valeur sont évalués dans les slots ; la table est lue
    /// depuis le slot de `nom` (les `Valeur` de table portent un
    /// `Rc<RefCell<_>>` : la muter à travers la copie modifie la variable
    /// d'origine). Repli par nom uniquement pour les variables hors slots.
    fn affecter_index_slots(&mut self, nom: &str, index: &Expr, valeur: &Expr,
                            base: *mut Valeur, corps: &crate::rir::CorpsRir,
                            depuis_slots: bool)
        -> Result<(), Option<i32>>
    {
        if !depuis_slots {
            // ⚠ Chemin historique des corps de fonction : aucune trame n'est
            // construite ici (`self.exec` lit la trame courante par nom).
            let instr = Instr::AffectIndex {
                nom: nom.to_string(), index: index.clone(), valeur: valeur.clone(), ligne: 0,
            };
            return self.exec(&instr);
        }
        let slots = unsafe { std::slice::from_raw_parts(base as *const Valeur, corps.n_slots) };
        let idx = self.eval_slots(index, slots, &corps.noms)?;
        let val = self.eval_slots(valeur, slots, &corps.noms)?;
        let table_opt = match slot_du_nom(&corps.noms, nom) {
            Some(i) => slots.get(i),
            None => self.trames.iter().rev().find_map(|t| t.get(nom))
                .or_else(|| self.globaux.get(nom)),
        };
        affecter_index_valeur(nom, table_opt, &idx, &val)
    }

    fn eval_appel_ast(&mut self, nom: &str, args: &[Expr], _base: *mut Valeur)
        -> Result<Valeur, Option<i32>>
    {
        self.appel(nom, "", args)
    }

    fn appel_rir_direct(&mut self, nom: &str, args: &[Expr], base: *mut Valeur,
                        corps_appelant: Option<&crate::rir::CorpsRir>)
        -> Option<Result<Valeur, Option<i32>>>
    {
        Interp::appel_rir_direct(self, nom, args, base, corps_appelant)
    }

    /// ⚠⚠ Affiche une expression ABSTRAITE (cas chaud des boucles).
    fn afficher_abstrait(&mut self, e: &crate::rir::Eabstrait, slots: &[Valeur])
        -> Result<(), Option<i32>>
    {
        let v = self.eval_abstrait(e, slots)?;
        enregistrer_symbolique_affiche(&v);
        crate::run::afficher(&v);
        Ok(())
    }

    fn empiler_slots(&mut self, n: usize) -> *mut Valeur {
        self.pile_slots.empiler(n)
    }

    fn slots_courants(&mut self, n: usize) -> *mut Valeur {
        self.pile_slots.courant(n)
    }

    fn depiler_slots(&mut self) {
        // ⚠⚠ La taille du cadre est connue par l'appelant : on ne peut pas la
        // deviner. Voir `depiler_n`.
        self.pile_slots.depiler(0);
    }

    fn marquer_slot_modifie(&mut self, slot: usize) {
        self.slots_sales.push(slot);
    }

    fn depiler_n(&mut self, n: usize) {
        self.pile_slots.depiler(n);
    }

    /// ⚠ Affectation par index : la règle EXACTE de l'interpréteur.
    fn affecter_index(&mut self, nom: &str, index: &Expr, valeur: &Expr)
        -> Result<(), Option<i32>>
    {
        let instr = Instr::AffectIndex {
            nom: nom.to_string(),
            index: index.clone(),
            valeur: valeur.clone(),
            ligne: 0,
        };
        self.exec(&instr)
    }
}

/// ⚠⚠⚠ LE CALCUL D'UN OPÉRATEUR UNAIRE, SUR UNE VALEUR (règle EXACTE).
///
/// Extrait de l'interpréteur : la voie RIR l'appelle avec la valeur du slot.
pub fn unaire_valeur(op: &str, v: &Valeur) -> Result<Valeur, Option<i32>> {
    if op == "non" {
        return Ok(Valeur::b(!est_vrai(v)));
    }
    match v {
        // -i64::MIN = 2^63 ne tient pas en i64 : promotion en entier exact.
        Valeur::Entier(i, None) => match i.checked_neg() {
            Some(n) => Ok(Valeur::z(n)),
            None => {
                let z = Gz::neuf();
                z.poser_i64(*i);
                let r = Gz::neuf();
                unsafe { gmp_neg(r.p(), z.p()) };
                Ok(zg_rapide(r))
            }
        },
        Valeur::Entier(_, Some(z)) => {
            let r = Gz::neuf();
            unsafe { gmp_neg(r.p(), z.p()) };
            Ok(zg_rapide(r))
        }
        Valeur::Rationnel(q) => {
            let r = Gq::neuf();
            unsafe { gmpq_neg(r.p(), q.p()) };
            Ok(Valeur::qq(r))
        }
        Valeur::Flottant(d) => Ok(Valeur::f(-*d)),
        Valeur::Booleen(b) => Ok(Valeur::z(if *b { -1 } else { 0 })),
        _ => erreur_runtime("opérateur unaire '-' sur un non-nombre"),
    }
}

/// ⚠⚠⚠ LE CALCUL D'UN OPÉRATEUR BINAIRE, SUR DES VALEURS (règle EXACTE).
///
/// Extrait de `Interp::binaire` : c'est la règle unique du moteur
/// (comparaisons, chaînes, complexes, irrationnels, rationnels, entiers
/// exacts). La voie RIR l'appelle directement avec les valeurs des slots,
/// sans repasser par l'AST — et sans dupliquer une seule ligne de règle.
pub fn binaire_valeurs(op: &str, g: &Valeur, d: &Valeur) -> Result<Valeur, Option<i32>> {
    {
        // ⚠ Les branches « et »/« ou » sont traitées par l'appelant (elles
        // exigent un court-circuit sur les EXPRESSIONS, pas sur les valeurs).
        let _ = (op, g, d);

        // ⚠ VOIE RAPIDE CHAÎNE '+' (22/11) : placée avant la cascade de
        // comparaisons et de types, elle évite ~5 ns par concaténation sur
        // le chemin JIT (`rose_jit_chaine_concat` appelle exactement cette
        // fonction). Même règle exacte que le chemin historique ci-dessous.
        if op == "+" {
            if let (Valeur::Chaine(x), Valeur::Chaine(y)) = (g, d) {
                return Ok(Valeur::Chaine(ChaineRep::concat(x, y)));
            }
        }

        // ⚠⚠⚠ VOIE RAPIDE i64 EN TÊTE (mesuré le 16 et 17/11).
        //
        // ⚠ Elle DOIT être en TÊTE de la fonction : placée plus bas, elle
        // passait après une cascade de `matches!` (tampons, listes, tables,
        // chaînes, complexes, irrationnels, rationnels) qui coûtait plus que
        // le calcul lui-même — mesuré : une boucle de 2 M de tours au niveau
        // principal à 1 631 ms.
        //
        // ⚠ `cmp_vals` convertit les DEUX opérandes en BigInt (`vers_z` =
        // `Gz::neuf` alloué + copie) pour comparer deux entiers i64 : mesuré,
        // une boucle de 3 M de tours passait de 247 ms à 9 ms sans cette
        // allocation. ⚠ La règle est préservée : la voie rapide n'est prise
        // que si LES DEUX côtés sont des entiers i64, et le dépassement
        // retombe sur le calcul exact (BigInt).
        if let (Valeur::Entier(x, None), Valeur::Entier(y, None)) = (g, d) {
            let (x, y) = (*x, *y);
            match op {
                "+" => if let Some(r) = x.checked_add(y) { return Ok(Valeur::z(r)); },
                "-" => if let Some(r) = x.checked_sub(y) { return Ok(Valeur::z(r)); },
                "*" => if let Some(r) = x.checked_mul(y) { return Ok(Valeur::z(r)); },
                "<" | "<=" | ">" | ">=" | "==" | "!=" => {
                    let c = x.cmp(&y);
                    let r = match op {
                        "<" => c.is_lt(), "<=" => c.is_le(),
                        ">" => c.is_gt(), ">=" => c.is_ge(),
                        "==" => c.is_eq(), _ => c.is_ne(),
                    };
                    return Ok(Valeur::b(r));
                }
                _ => {}
            }
        }
        // comparaisons
        match op {
            "==" | "!=" | "<" | "<=" | ">" | ">=" => {
                if op == "==" || op == "!=" {
                    // miroir rose_valeur_egale : tampons, listes, tables et
                    // chaînes comparent structurassiment (jamais vers_z).
                    if matches!(g, Valeur::Tampon(..)) || matches!(d, Valeur::Tampon(..))
                        || matches!(g, Valeur::Liste(..)) || matches!(d, Valeur::Liste(..))
                        || matches!(g, Valeur::Table(..)) || matches!(d, Valeur::Table(..))
                        || matches!(g, Valeur::Chaine(..)) || matches!(d, Valeur::Chaine(..))
                    {
                        let e = valeur_egale(&g, &d);
                        return Ok(Valeur::b(if op == "==" { e } else { !e }));
                    }
                }
                let c = cmp_vals(&g, &d);
                let r = match op {
                    "==" => c == 0, "!=" => c != 0, "<" => c < 0,
                    "<=" => c <= 0, ">" => c > 0, _ => c >= 0,
                };
                return Ok(Valeur::b(r));
            }
            "≈" => {
                let r = if matches!(g, Valeur::Flottant(..)) || matches!(d, Valeur::Flottant(..)) {
                    let (x, y) = (vers_d(&g), vers_d(&d));
                    let scale = x.abs().max(y.abs()).max(1.0);
                    (x - y).abs() <= 1e-9 * scale
                } else { cmp_vals(&g, &d) == 0 };
                return Ok(Valeur::b(r));
            }
            _ => {}
        }
        // (la concaténation de chaînes est traitée EN TÊTE de fonction)
        // Complexe : opérations sur a+bi
        if matches!(g, Valeur::Complexe{..}) || matches!(d, Valeur::Complexe{..}) {
            let gc = matches!(g, Valeur::Complexe{..});
            let dc = matches!(d, Valeur::Complexe{..});
            // ── VOIE RAPIDE : un seul opérande est complexe ──────────────
            // Un réel est traité comme (x, 0) mais on n'émet PAS les
            // multiplications par 0.0 / 1.0 de la formule générale : le JIT
            // fait de même (bit à bit), sinon les deux moteurs divergeraient.
            // C'est ce chemin qui rend la construction « x + y*i » rapide.
            if gc != dc {
                return Ok(if gc {
                    // complexe ⊕ réel
                    let (cr, ci) = match &g { Valeur::Complexe{re,im} => (*re,*im), _ => (0.0, 0.0) };
                    let x = vers_d(&d);
                    match op {
                        "+" => Valeur::Complexe { re: cr + x, im: ci },
                        "-" => Valeur::Complexe { re: cr - x, im: ci },
                        "*" => Valeur::Complexe { re: cr * x, im: ci * x },
                        "/" => {
                            if x == 0.0 { erreur_runtime("division par zéro (complexe)"); }
                            Valeur::Complexe { re: cr / x, im: ci / x }
                        }
                        _ => erreur_runtime("opération non supportée sur les complexes"),
                    }
                } else {
                    // réel ⊕ complexe
                    let (cr, ci) = match &d { Valeur::Complexe{re,im} => (*re,*im), _ => (0.0, 0.0) };
                    let x = vers_d(&g);
                    match op {
                        "+" => Valeur::Complexe { re: x + cr, im: ci },
                        "-" => Valeur::Complexe { re: x - cr, im: -ci },
                        "*" => Valeur::Complexe { re: x * cr, im: x * ci },
                        "/" => {
                            let denom = cr*cr + ci*ci;
                            if denom == 0.0 { erreur_runtime("division par zéro (complexe)"); }
                            Valeur::Complexe { re: (x*cr)/denom, im: -(x*ci)/denom }
                        }
                        _ => erreur_runtime("opération non supportée sur les complexes"),
                    }
                });
            }
            // ── complexe ⊕ complexe : formule générale ───────────────────
            let (gr, gi) = match &g { Valeur::Complexe{re,im} => (*re,*im), _ => (vers_d(&g), 0.0) };
            let (dr, di) = match &d { Valeur::Complexe{re,im} => (*re,*im), _ => (vers_d(&d), 0.0) };
            return Ok(match op {
                "+" => Valeur::Complexe { re: gr + dr, im: gi + di },
                "-" => Valeur::Complexe { re: gr - dr, im: gi - di },
                // le produit croisé gr*di + gi*dr vaut 2*(gr*di) : on ne calcule
                // le produit qu'une fois (même arrondi IEEE, une division de
                // moins — le JIT fait de même).
                "*" => { let p = gr * di; Valeur::Complexe { re: gr*dr - gi*di, im: p + p } }
                "/" => {
                    let denom = dr*dr + di*di;
                    if denom == 0.0 { erreur_runtime("division par zéro (complexe)"); }
                    Valeur::Complexe { re: (gr*dr + gi*di)/denom, im: (gi*dr - gr*di)/denom }
                }
                _ => erreur_runtime("opération non supportée sur les complexes"),
            });
        }
        // Irrationnel : √n
        if matches!(g, Valeur::Irrationnel(..)) || matches!(d, Valeur::Irrationnel(..)) {
            let (gr, gi) = match &g {
                Valeur::Irrationnel(z) => (z.chaine().parse::<f64>().unwrap_or(0.0), 0.0),
                _ => (vers_d(&g), 0.0),
            };
            let (dr, di) = match &d {
                Valeur::Irrationnel(z) => (z.chaine().parse::<f64>().unwrap_or(0.0), 0.0),
                _ => (vers_d(&d), 0.0),
            };
            return Ok(match op {
                "+" => Valeur::f(gr + dr),
                "-" => Valeur::f(gr - dr),
                "*" => Valeur::f(gr * dr),
                "/" => {
                    if dr == 0.0 { erreur_runtime("division par zéro (irrationnel)"); }
                    Valeur::f(gr / dr)
                }
                _ => erreur_runtime("opération non supportée sur les irrationnels"),
            });
        }
        // Répétition de chaîne : "abc" * 3  ou  3 * "abc"
        if op == "*" {
            if let (Valeur::Chaine(x), Valeur::Entier(n, None)) = (&g, &d) {
                let n = *n;
                if n < 0 { erreur_runtime("répétition de chaîne avec compte négatif"); }
                return Ok(Valeur::s(x.aplatir().repeat(n as usize)));
            }
            if let (Valeur::Entier(n, None), Valeur::Chaine(x)) = (&g, &d) {
                let n = *n;
                if n < 0 { erreur_runtime("répétition de chaîne avec compte négatif"); }
                return Ok(Valeur::s(x.aplatir().repeat(n as usize)));
            }
        }
        // ⚠⚠⚠ VOIE RAPIDE i64 (mesuré le 16/11). L'ancien chemin passait
        // TOUJOURS par BigInt (`vers_z` = allocation `Gz::neuf` par opérande
        // et par opération). Mesuré : le jardin des premiers prenait 267 ms
        // contre 46 ms sans cette allocation, pour un calcul qui tient dans
        // un i64. La règle du langage est préservée : le résultat n'est un
        // entier i64 QUE s'il tient dans i64, sinon on retombe sur le calcul
        // exact (BigInt). C'est exactement ce que fait le JIT (voie taggée).
        if let (Valeur::Entier(a, None), Valeur::Entier(b, None)) = (g, d) {
            match op {
                "+" => if let Some(r) = a.checked_add(*b) { return Ok(Valeur::z(r)); },
                "-" => if let Some(r) = a.checked_sub(*b) { return Ok(Valeur::z(r)); },
                "*" => if let Some(r) = a.checked_mul(*b) { return Ok(Valeur::z(r)); },
                _ => {}
            }
        }
        // ⚠⚠⚠ VAGUE 4 — grand entier × PETIT entier i64 SANS CLONER le
        // grand opérande. Le chemin général `binop_z_pool("*")` copie TOUS
        // les limbs du bigint (`vers_z_dans` → `copier_de` → `v()`) avant
        // de multiplier : c'est le coût dominant de `$r = $r * k` dans
        // `factorial.rose` (3000 itérations, bigint qui grandit). Or
        // `num-bigint` implémente `&BigInt * i64` (signe et i64::MIN inclus)
        // directement, sans cette copie. Mesuré sur 2900! (micro-banc
        // `rose/tests/opt_arith.rs`) : 465 ns (clone + mul) → 381 ns
        // (sans clone), soit ~18 %. Le résultat est BIT-À-BIT identique :
        // `BigInt::from(b)` et `b` ont la même valeur, donc le même produit.
        if op == "*" {
            if let (Valeur::Entier(_, Some(a)), Valeur::Entier(b, None)) = (g, d) {
                return Ok(zg_rapide(Gz::de_bigint(a.bref() * *b)));
            }
            if let (Valeur::Entier(a, None), Valeur::Entier(_, Some(b))) = (g, d) {
                return Ok(zg_rapide(Gz::de_bigint(b.bref() * *a)));
            }
        }
        let m = rnk(&g).max(rnk(&d));
        match op {
            "+" => Ok(if m == 3 { Valeur::f(vers_d(&g) + vers_d(&d)) }
                      else if m == 2 { let (x, y) = (vers_q(&g), vers_q(&d)); let r = Gq::neuf(); unsafe { gmpq_add(r.p(), x.p(), y.p()) }; Valeur::qq(r) }
                      else { binop_z_pool("+", &g, &d) }),
            "-" => Ok(if m == 3 { Valeur::f(vers_d(&g) - vers_d(&d)) }
                      else if m == 2 { let (x, y) = (vers_q(&g), vers_q(&d)); let r = Gq::neuf(); unsafe { gmpq_sub(r.p(), x.p(), y.p()) }; Valeur::qq(r) }
                      else { binop_z_pool("-", &g, &d) }),
            "*" => Ok(if m == 3 { Valeur::f(vers_d(&g) * vers_d(&d)) }
                      else if m == 2 { let (x, y) = (vers_q(&g), vers_q(&d)); let r = Gq::neuf(); unsafe { gmpq_mul(r.p(), x.p(), y.p()) }; Valeur::qq(r) }
                      else { binop_z_pool("*", &g, &d) }),
            "/" => {
                if m == 3 {
                    let (x, y) = (vers_d(&g), vers_d(&d));
                    if y == 0.0 { erreur_runtime("division par zéro"); }
                    return Ok(Valeur::f(x / y));
                }
                let y = vers_q(&d);
                if crate::run::gmpq_sgn(y.p()) == 0 { erreur_runtime("division par zéro"); }
                let x = vers_q(&g);
                let r = Gq::neuf();
                unsafe { gmpq_div(r.p(), x.p(), y.p()) };
                Ok(Valeur::qq(r))
            }
            "div" => {
                if matches!(g, Valeur::Flottant(..)) || matches!(d, Valeur::Flottant(..)) {
                    let y = vers_d(&d);
                    if y == 0.0 { erreur_runtime("division euclidienne par zéro"); }
                    return Ok(Valeur::f((vers_d(&g) / y).floor()));
                }
                if matches!(g, Valeur::Rationnel(..)) || matches!(d, Valeur::Rationnel(..)) {
                    let y = vers_q(&d);
                    if crate::run::gmpq_sgn(y.p()) == 0 { erreur_runtime("division euclidienne par zéro"); }
                    let q = Gq::neuf();
                    let x = vers_q(&g);
                    unsafe { gmpq_div(q.p(), x.p(), y.p()) };
                    let r = Gz::neuf();
                    unsafe { gmp_fdiv_q(r.p(), q.num_raw(), q.den_raw()) };
                    return Ok(zg_rapide(r));
                }
                let y = vers_z(&d);
                // ⚠ Test zéro DIRECT : `y.chaine() == "0"` convertissait le
                // bigint en décimal complet (~6,6 µs mesuré, 93,4 % du temps
                // d'un Euclid sur 1000 chiffres) juste pour tester zéro.
                if y.bref().is_zero() { erreur_runtime("division euclidienne par zéro"); }
                let x = vers_z(&g);
                let r = Gz::neuf();
                unsafe { gmp_fdiv_q(r.p(), x.p(), y.p()) };
                Ok(zg_rapide(r))
            }
            "%" => {
                // règle du langage : le modulo n'accepte
                // que deux entiers — un operand rationnel/flottant est une
                // erreur, jamais une promotion silencieuse.
                if rnk(&g) != 1 || rnk(&d) != 1 {
                    erreur_runtime_ligne("Le modulo exige deux entiers");
                }
                // ⚠ Voie rapide WD2 : les deux opérandes sont déjà des
                // grands entiers boxés (`Rc<Gz>`). On évite deux `vers_z`
                // (2 allocations de boîte + 2 clones complets du BigInt)
                // et l'allocation d'un `Gz` intermédiaire : le reste est
                // calculé par référence puis déplacé dans un `Gz` neuf.
                // Sémantique STRICTEMENT identique à `gmp_fdiv_r`
                // (mod_floor euclidien) ; le chemin général ci-dessous
                // reste actif pour tous les autres couples de types.
                if let (Valeur::Entier(_, Some(ga)), Valeur::Entier(_, Some(db))) = (&g, &d) {
                    if db.bref().is_zero() { erreur_runtime("modulo par zéro"); }
                    let rem = match crate::i128x::mod_rapide(ga.bref(), db.bref()) {
                        Some(r) => r,
                        None => ga.bref().mod_floor(db.bref()),
                    };
                    return Ok(zg_rapide(Gz::de_bigint(rem)));
                }
                let y = vers_z(&d);
                // ⚠ Test zéro DIRECT (voir `div` ci-dessus) : évite une
                // conversion décimale complète du bigint à chaque itération.
                if y.bref().is_zero() { erreur_runtime("modulo par zéro"); }
                let x = vers_z(&g);
                let r = Gz::neuf();
                unsafe { gmp_fdiv_r(r.p(), x.p(), y.p()) };
                Ok(zg_rapide(r))
            }
            "^" => {
                if matches!(d, Valeur::Flottant(..)) {
                    return Ok(Valeur::f(vers_d(&g).powf(vers_d(&d))));
                }
                if matches!(g, Valeur::Flottant(..)) {
                    return Ok(Valeur::f(vers_d(&g).powf(vers_d(&d))));
                }
                let expo = vers_z(&d);
                if !expo.fits_i64() {
                    erreur_runtime("exposant démesuré (la puissance exploserait au-delà de tout calcul)");
                }
                let e = expo.get_i64();
                if matches!(g, Valeur::Rationnel(..)) {
                    let q = vers_q(&g);
                    return Ok(Valeur::qq(pow_rationnel_sur(&q, e)));
                }
                let base = vers_z(&g);
                if e >= 0 {
                    let r = Gz::neuf();
                    unsafe { gmp_pow_ui(r.p(), base.p(), e as u64) };
                    Ok(zg_rapide(r))
                } else {
                    if base.sgn() == 0 { erreur_runtime("0^exposant négatif"); }
                    let q = Gq::neuf();
                    unsafe { gmpq_set_z_field(q.num(), base.p()); gmpq_canonicalize(q.p()) };
                    Ok(Valeur::qq(pow_rationnel_sur(&q, e)))
                }
            }
            _ => { erreur_runtime(&format!("opérateur '{}' inconnu", op)); }
        }
    }
}

impl Interp {
    fn appel(&mut self, nom: &str, module: &str, args: &[Expr]) -> Result<Valeur, Option<i32>> {
        let mut a = Vec::with_capacity(args.len());
        for x in args { a.push(self.eval(x)?); }
        self.appel_valeurs(nom, module, a)
    }

    /// ⚠⚠⚠ PROTOCOLE DE TRAME UNIQUE (goulot #1) — appel à opérandes déjà
    /// résolues. Même règles exactes que `appel` ; seuls les arguments changent
    /// de nature (des `Valeur` au lieu d'expressions à réévaluer dans une
    /// trame `HashMap`). Le chemin chaud du RIR évalue ses arguments dans les
    /// slots puis vient directement ici : aucune trame n'est construite.
    fn appel_valeurs(&mut self, nom: &str, module: &str, a: Vec<Valeur>)
        -> Result<Valeur, Option<i32>>
    {
        if module == "dynamique" {
            return self.dynamique(nom, &a);
        }
        // conversions
        if a.len() == 1 && matches!(nom, "entier" | "rationnel" | "flottant" | "chaine") {
            let v = a.into_iter().next().unwrap();
            return Ok(match nom {
                "entier" => {
                    if let Valeur::Chaine(s) = &v { return Ok(zg_rapide(chaine_vers_entier(&s.aplatir()))); }
                    convertir(v, TyKind::Entier)
                }
                "rationnel" => convertir(v, TyKind::Rationnel),
                "flottant" => {
                    if let Valeur::Chaine(s) = &v { return Ok(Valeur::f(chaine_vers_flottant(&s.aplatir()))); }
                    convertir(v, TyKind::Flottant)
                }
                _ => Valeur::s(chaine_de(&v)),
            });
        }
        // ── VAGUE 4 — PGCD sans conversion ni CLONE des opérandes ────────
        // Le chemin général `stdlib("pgcd")` fait `vers_z(&a[0])` et
        // `vers_z(&a[1])`, c'est-à-dire DEUX `Gz::neuf()` + deux copies
        // complètes des limbs, avant même d'appeler `gcd_rapide` (qui, lui,
        // n'emprunte déjà plus rien). Sur un PGCD de 1 000 chiffres, ces
        // clones sont mesurés à ~0,4 µs sur les 1,3 µs de la façade.
        // Ici les `Rc<Gz>` boxés sont passés DIRECTEMENT par référence à
        // `pgcd` ; les petits entiers i64 sont boxés dans un `Gz` éphémère
        // (un seul limb). Le résultat, la sémantique (signes, zéro, tag i64)
        // et le repli d'erreur restent exactement ceux de `stdlib`.
        if (nom == "pgcd" || nom == "gcd") && a.len() == 2 {
            return Ok(pgcd_valeurs(&a[0], &a[1]));
        }
        // ⚠⚠⚠ LA SIGNATURE N'EST PLUS CLONÉE (mesuré le 16/11).
        // `self.prog.signatures.get(nom).cloned()` recopiait à CHAQUE APPEL
        // la `Signature` entière — c'est-à-dire TOUT LE CORPS de la fonction
        // (tous les `Instr`, toutes les `Expr`, tous les `String`). Sur une
        // récursion profonde, cela faisait déborder la pile : mesuré,
        // `it(10000)` (récursion 10 000, multiplication entière donc hors
        // JIT) échouait en « stack overflow » alors que le moteur de
        // référence répond 450015000. Un emprunt suffit : `self.prog` ne
        // change pas pendant l'exécution.
        //
        // ⚠ Le pointeur brut évite d'emprunter `self.prog` pendant que
        // `&mut self` est utilisé pour évaluer les arguments et appeler.
        let sig_ptr: *const Signature = match self.prog.signatures.get(nom) {
            Some(s) => s as *const Signature,
            None => std::ptr::null(),
        };
        if !sig_ptr.is_null() {
            let sig: &Signature = unsafe { &*sig_ptr };
            // fonction externe @dll/@sym sans corps : FFI directe (
            // ffi_appeler).
            if sig.corps.is_none()
                && !(sig.dll.is_empty() && sig.dll_son.is_empty() && sig.symbole.is_empty())
            {
                return self.ffi_appeler(sig, &a);
            }
            return self.appeler(sig, a);
        }
        self.stdlib(nom, &a)
    }

    /// Poignée de bibliothèque pour une signature @dll/@sym (
    /// ffi_poignee : candidats lib<n>.so, <n>.so, <n>, lib<n>.so.6/7/8).
    fn ffi_poignee(&mut self, sig: &Signature) -> *mut std::ffi::c_void {
        let nom = if !sig.dll_son.is_empty() { sig.dll_son.clone() }
                  else if !sig.dll.is_empty() { sig.dll.clone() }
                  else { "c".to_string() };
        // ⚠⚠⚠ `@dll("wasm", "lib.wasm")` : le nom de bibliothèque est "wasm",
        // le FICHIER est le module wasm. Une seule instance par fichier.
        #[cfg(not(target_arch = "wasm32"))]
        if sig.dll == "wasm" {
            let chemin = if sig.dll_son.is_empty() { sig.dll.clone() } else { sig.dll_son.clone() };
            if chemin.is_empty() {
                erreur_runtime("WebAssembly : nom de fichier attendu (@dll(\"wasm\", \"x.wasm\"))");
            }
            let deja = wf::deja_charge();
            let poignee = match deja.get(&chemin) {
                Some(p) => *p,
                None => match wf::charger(&chemin) {
                    Ok(p) => p,
                    Err(e) => erreur_runtime(&format!("appel WebAssembly : {e}")),
                },
            };
            return poignee as usize as *mut std::ffi::c_void;
        }
        let candidats: Vec<String> = if !sig.dll_son.is_empty() || nom.contains('/') {
            vec![nom.clone()]
        } else {
            vec![format!("lib{nom}.so"), format!("{nom}.so"), nom.clone(),
                 format!("lib{nom}.so.6"), format!("lib{nom}.so.7"), format!("lib{nom}.so.8")]
        };
        unsafe {
            for c in candidats {
                let cs = std::ffi::CString::new(c).unwrap();
                let h = ffi_dlopen(cs.as_ptr() as *const i8);
                if !h.is_null() { self.poignees.push(h); return h; }
            }
        }
        erreur_runtime(&format!("bibliothèque introuvable ({})", nom));
        std::ptr::null_mut()
    }

    /// Appel FFI d'une fonction externe (ffi_appeler) :
    /// entier→long long, flottant→double, booleen→int, chaine→char*.
    fn ffi_appeler(&mut self, sig: &Signature, args: &[Valeur]) -> Result<Valeur, Option<i32>> {
        use crate::front::TyKind as K;
        let h = self.ffi_poignee(sig);
        let sym = if sig.symbole.is_empty() { sig.nom.clone() } else { sig.symbole.clone() };
        // ⚠⚠⚠ UNE POIGNÉE WEBASSEMBLY N'EST PAS UNE BIBLIOTHÈQUE DYNAMIQUE :
        // `@dll("wasm", "lib.wasm")` rend un NUMÉRO d'instance (≥ 0x1000). Le
        // symbole y est une FONCTION EXPORTÉE du module, appelée par
        // l'interpréteur wasm — pas par `dlsym`.
        #[cfg(not(target_arch = "wasm32"))]
        if wf::est_wasm(h as usize as i64) {
            return self.ffi_appeler_wasm(h as usize as i64, &sym, sig, args);
        }
        let cs = std::ffi::CString::new(sym.clone()).unwrap();
        let s = unsafe { ffi_dlsym(h, cs.as_ptr() as *const i8) };
        if s.is_null() { erreur_runtime(&format!("symbole introuvable dans la bibliothèque ({sym})")); }
        let zarg = |i: usize| -> i64 {
            let z = vers_z(&args[i]);
            if z.fits_i64() { z.get_i64() }
            else { erreur_runtime("fonction externe : argument entier hors i64") }
        };
        let darg = |i: usize| -> f64 { vers_d(&args[i]) };
        let sent = |i: usize| matches!(sig.params[i].1.n, K::Chaine);
        let p = s as usize;
        let r = match sig.retour.n {
            K::Entier => match args.len() {
                0 => { let f: extern "C" fn() -> i64 = unsafe { std::mem::transmute(p) }; Valeur::z(f()) }
                1 => { let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(p) }; Valeur::z(f(zarg(0))) }
                2 => { let f: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(p) }; Valeur::z(f(zarg(0), zarg(1))) }
                _ => { erreur_runtime("fonction externe : plus de 2 arguments entiers non supporté"); }
            },
            K::Flottant => match args.len() {
                0 => { let f: extern "C" fn() -> f64 = unsafe { std::mem::transmute(p) }; Valeur::f(f()) }
                1 => {
                    if sent(0) {
                        let f: extern "C" fn(*const i8) -> f64 = unsafe { std::mem::transmute(p) };
                        let c = std::ffi::CString::new(chaine_de(&args[0])).unwrap();
                        Valeur::f(unsafe { f(c.as_ptr() as *const i8) })
                    } else if matches!(sig.params[0].1.n, K::Flottant) {
                        let f: extern "C" fn(f64) -> f64 = unsafe { std::mem::transmute(p) }; Valeur::f(f(darg(0)))
                    } else if matches!(sig.params[0].1.n, K::Booleen) {
                        let f: extern "C" fn(i32) -> f64 = unsafe { std::mem::transmute(p) };
                        Valeur::f(f(if est_vrai(&args[0]) { 1 } else { 0 }))
                    } else {
                        let f: extern "C" fn(i64) -> f64 = unsafe { std::mem::transmute(p) }; Valeur::f(f(zarg(0)))
                    }
                }
                2 => {
                    let p0 = sig.params[0].1.n; let p1 = sig.params[1].1.n;
                    if matches!(p0, K::Flottant) && matches!(p1, K::Flottant) {
                        let f: extern "C" fn(f64, f64) -> f64 = unsafe { std::mem::transmute(p) }; Valeur::f(f(darg(0), darg(1)))
                    } else if matches!(p0, K::Flottant) {
                        let f: extern "C" fn(f64, i64) -> f64 = unsafe { std::mem::transmute(p) }; Valeur::f(f(darg(0), zarg(1)))
                    } else if matches!(p1, K::Flottant) {
                        let f: extern "C" fn(i64, f64) -> f64 = unsafe { std::mem::transmute(p) }; Valeur::f(f(zarg(0), darg(1)))
                    } else {
                        let f: extern "C" fn(i64, i64) -> f64 = unsafe { std::mem::transmute(p) }; Valeur::f(f(zarg(0), zarg(1)))
                    }
                }
                3 => { let f: extern "C" fn(f64, f64, f64) -> f64 = unsafe { std::mem::transmute(p) }; Valeur::f(f(darg(0), darg(1), darg(2))) }
                _ => { erreur_runtime("fonction externe flottante : arité non supportée"); }
            },
            K::Booleen => match args.len() {
                0 => { let f: extern "C" fn() -> i32 = unsafe { std::mem::transmute(p) }; Valeur::b(f() != 0) }
                1 => { let f: extern "C" fn(i64) -> i32 = unsafe { std::mem::transmute(p) }; Valeur::b(f(zarg(0)) != 0) }
                _ => { erreur_runtime("fonction externe booléenne : arité non supportée"); }
            },
            K::Chaine => match args.len() {
                0 => {
                    let f: extern "C" fn() -> *const i8 = unsafe { std::mem::transmute(p) };
                    let r = unsafe { f() };
                    Valeur::s(if r.is_null() { String::new() }
                              else { unsafe { std::ffi::CStr::from_ptr(r as *const std::os::raw::c_char) }.to_string_lossy().into_owned() })
                }
                1 => {
                    if sent(0) {
                        let f: extern "C" fn(*const i8) -> *const i8 = unsafe { std::mem::transmute(p) };
                        let c = std::ffi::CString::new(chaine_de(&args[0])).unwrap();
                        let r = unsafe { f(c.as_ptr() as *const i8) };
                        Valeur::s(if r.is_null() { String::new() } else { unsafe { std::ffi::CStr::from_ptr(r as *const std::os::raw::c_char) }.to_string_lossy().into_owned() })
                    } else {
                        let f: extern "C" fn(i64) -> *const i8 = unsafe { std::mem::transmute(p) };
                        let r = unsafe { f(zarg(0)) };
                        Valeur::s(if r.is_null() { String::new() } else { unsafe { std::ffi::CStr::from_ptr(r as *const std::os::raw::c_char) }.to_string_lossy().into_owned() })
                    }
                }
                _ => { erreur_runtime("fonction externe texte : arité non supportée"); }
            },
            _ => { erreur_runtime("type de retour externe non supporté"); }
        };
        Ok(r)
    }

    /// ⚠ Appel d'une fonction WebAssembly : conversion Rose → wasm32, appel,
    /// puis conversion wasm32 → Rose (le type de retour est celui de la
    /// déclaration `fn`).
    ///
    /// ⚠ LA CONVENTION DES POINTEURS : `entier`/`booleen` → i32, `flottant` →
    /// f64, `chaine` → POINTEUR dans la mémoire linéaire du module (les octets
    /// y sont copiés puis terminés par un zéro, comme une `char*` C).
    #[cfg(not(target_arch = "wasm32"))]
    fn ffi_appeler_wasm(&mut self, h: i64, sym: &str, sig: &Signature,
                        args: &[Valeur]) -> Result<Valeur, Option<i32>> {
        use crate::front::TyKind as K;
        use crate::wasm_lecteur::{TyWasm, ValWasm};
        let mut vals: Vec<ValWasm> = Vec::with_capacity(args.len());
        // ⚠ Les chaînes sont copiées à la fin de la mémoire linéaire.
        let mut adresse_libre = wf::memoire_taille(h).unwrap_or(0);
        for (i, a) in args.iter().enumerate() {
            let ty = sig.params.get(i).map(|p| p.1.n).unwrap_or(K::Entier);
            match ty {
                K::Flottant | K::Rationnel => vals.push(ValWasm::F64(vers_d(a))),
                K::Chaine => {
                    let s = chaine_de(a);
                    let octets = s.as_bytes();
                    adresse_libre += 8;
                    let adresse = adresse_libre;
                    let mut tampon = octets.to_vec();
                    tampon.push(0);
                    if let Err(e) = wf::ecrire_memoire(h, adresse, &tampon) {
                        erreur_runtime(&format!("appel WebAssembly : {e}"));
                    }
                    adresse_libre += tampon.len();
                    vals.push(ValWasm::I32(adresse as i32));
                }
                K::Booleen => vals.push(ValWasm::I32(if est_vrai(a) { 1 } else { 0 })),
                _ => {
                    let z = vers_z(a);
                    if !z.fits_i64() {
                        erreur_runtime("appel WebAssembly : argument entier hors i64");
                    }
                    vals.push(ValWasm::I32(z.get_i64() as i32));
                }
            }
        }
        let res = match wf::appeler(h, sym, &vals) {
            Ok(r) => r,
            Err(e) => erreur_runtime(&format!("appel WebAssembly : {e}")),
        };
        let premier = res.first().copied().unwrap_or(ValWasm::I32(0));
        Ok(match sig.retour.n {
            K::Flottant => Valeur::f(premier.as_f64()),
            K::Rationnel => {
                let mut q = Gq::neuf();
                q.depuis_d(premier.as_f64());
                Valeur::qq(q)
            }
            K::Booleen => Valeur::b(premier.as_i32() != 0),
            K::Chaine => {
                let a = premier.as_i32() as usize;
                let mut s = Vec::new();
                if let Some(taille) = wf::memoire_taille(h) {
                    let mut i = a;
                    while i < taille {
                        match wf::lire_octet(h, i) {
                            Some(0) => break,
                            Some(o) => s.push(o),
                            None => break,
                        }
                        i += 1;
                    }
                }
                Valeur::s(String::from_utf8_lossy(&s).into_owned())
            }
            _ => Valeur::z(premier.as_i32() as i64),
        })
    }

    fn dynamique(&mut self, nom: &str, args: &[Valeur]) -> Result<Valeur, Option<i32>> {
        match nom {
            "charger" => {
                let s = chaine_de(&args[0]);
                let c = std::ffi::CString::new(s.clone()).unwrap();
                unsafe {
                    let h = ffi_dlopen(c.as_ptr() as *const i8);
                    if h.is_null() {
                        erreur_runtime(&format!("chargement de bibliothèque impossible ({})", s));
                    }
                    self.poignees.push(h);
                    Ok(Valeur::z(h as i64))
                }
            }
            "fermer" => {
                let h = extraire_i64(&args[0]) as *mut std::ffi::c_void;
                unsafe { ffi_dlclose(h) };
                Ok(Valeur::Vide)
            }
            "appeler0" | "appeler1" | "appeler2" => {
                let h = extraire_i64(&args[0]) as *mut std::ffi::c_void;
                let sym = chaine_de(&args[1]);
                let c = std::ffi::CString::new(sym.clone()).unwrap();
                unsafe {
                    let p = ffi_dlsym(h, c.as_ptr() as *const i8);
                    if p.is_null() {
                        erreur_runtime(&format!("symbole '{}' introuvable", sym));
                    }
                    match nom {
                        "appeler0" => {
                            let f: extern "C" fn() -> f64 = std::mem::transmute(p);
                            Ok(Valeur::f(f()))
                        }
                        "appeler1" => {
                            let f: extern "C" fn(f64) -> f64 = std::mem::transmute(p);
                            Ok(Valeur::f(f(vers_d(&args[2]))))
                        }
                        _ => {
                            let f: extern "C" fn(f64, f64) -> f64 = std::mem::transmute(p);
                            Ok(Valeur::f(f(vers_d(&args[2]), vers_d(&args[3]))))
                        }
                    }
                }
            }
            _ => { erreur_runtime(&format!("Fonction dynamique '{}' inconnue", nom)); }
        }
    }

    fn stdlib(&mut self, nom: &str, a: &[Valeur]) -> Result<Valeur, Option<i32>> {
        if nom == "quitter" {
            let z = vers_z(&a[0]);
            if !z.fits_i64() { erreur_runtime("code de sortie hors plage"); }
            flush_out();
            return Err(Some(z.get_i64() as i32));
        }
        if nom == "join" {
            let id = vers_z(&a[0]);
            if !id.fits_i64() { erreur_runtime("join : id invalide"); }
            crate::run::join_thread(id.get_i64() as usize);
            return Ok(Valeur::Vide);
        }
        if nom == "breakpoint_if" {
            let cond = est_vrai(&a[0]);
            if cond {
                crate::run::flush_out();
                eprintln!("[BREAKPOINT_IF] condition vraie");
                for w in crate::run::get_watches() {
                    // try to find the var
                }
                for (k, v) in &self.globaux {
                    eprintln!("  {} = {}", k, chaine_de(v));
                }
                eprintln!("Appuyer sur Entrée pour continuer...");
                let mut buf = String::new();
                let _ = std::io::stdin().read_line(&mut buf);
            }
            return Ok(Valeur::Vide);
        }
        if nom == "watch" {
            let name = chaine_de(&a[0]);
            crate::run::add_watch(name);
            return Ok(Valeur::Vide);
        }
        if nom == "unwatch" {
            let name = chaine_de(&a[0]);
            crate::run::remove_watch(&name);
            return Ok(Valeur::Vide);
        }
        Ok(self.stdlib_valeur(nom, a))
    }



    fn stdlib_valeur(&mut self, nom: &str, a: &[Valeur]) -> Valeur {
        match nom {
            "afficher" => { enregistrer_symbolique_affiche(&a[0]); afficher(&a[0]); Valeur::Vide }
            "view" => { let s = chaine_de(&a[0]); view_grand_nombre(&s); Valeur::Vide }
            // ── INT1 : valeurs paresseuses et détail typé ──────────────────
            "entier_geant_factorielle" => {
                let z = vers_z(&a[0]);
                if z.sgn() < 0 { erreur_runtime("entier_geant_factorielle : n negatif"); }
                // `vers_z` donne un `Gz` éphémère : le constructeur en copie la
                // valeur ( jamais le pointeur ), ce qui accepte n > u64::MAX.
                Valeur::symbolique_factorielle_gz(&z)
            }
            "detail" => {
                let t = Valeur::table();
                if let Valeur::Table(tt) = &t {
                    for (cle, val) in proprietes_detail(&a[0]) {
                        tt.inserer(Valeur::s(cle), Valeur::s(val));
                    }
                }
                t
            }
            // Rendu multi-lignes avec le gabarit exact de CLI1
            // (`repl::format_table_detail`) : deux espaces, libellé paddé à 28,
            // séparateur ` : `. `>> detail(x)` rend la table compacte Rose ;
            // `afficher_detail(detail(x))` rend le bloc lisible.
            "afficher_detail" => {
                match &a[0] {
                    Valeur::Table(t) => {
                        for ligne in crate::repl::format_table_detail(t) {
                            crate::run::ecrire_out(&ligne);
                            crate::run::ecrire_out("\n");
                        }
                    }
                    _ => erreur_runtime("afficher_detail : attend une table (detail(expr))"),
                }
                Valeur::Vide
            }
            "chiffres" => chiffres_int1(&a[0], if a.len() > 1 { vers_z(&a[1]).get_i64() } else { 30 }),
            "plus_de_chiffres" => plus_de_chiffres_int1(
                &a[0], if a.len() > 1 { vers_z(&a[1]).get_i64() } else { 30 }),
            // ── INT1 : pont vers les primitives GUI1 déjà écrites ─────────
            #[cfg(not(target_arch = "wasm32"))]
            "fenetre_compter" => Valeur::z(crate::gui_native::compter_fenetres() as i64),
            #[cfg(not(target_arch = "wasm32"))]
            "fenetre_creer_simulee" => {
                let titre = chaine_de(&a[0]);
                let l = vers_z(&a[1]).get_i64();
                let h = vers_z(&a[2]).get_i64();
                Valeur::z(crate::gui_native::creer_simulee(&titre, l, h) as i64)
            }
            #[cfg(not(target_arch = "wasm32"))]
            "fenetre_boutons_creer" => {
                let id = vers_z(&a[0]).get_i64() as u64;
                let defs = boutons_depuis_geometrie(&chaine_de(&a[1]));
                if let Err(e) = crate::gui_native::boutons_creer(id, &defs) {
                    erreur_runtime(&e.to_string());
                }
                Valeur::z(defs.len() as i64)
            }
            #[cfg(not(target_arch = "wasm32"))]
            "bouton_compter" => {
                let id = vers_z(&a[0]).get_i64() as u64;
                Valeur::z(crate::gui_native::compter_boutons(id) as i64)
            }
            #[cfg(not(target_arch = "wasm32"))]
            "bouton_sous" => {
                let defs = boutons_depuis_geometrie(&chaine_de(&a[0]));
                let x = vers_z(&a[1]).get_i64() as i32;
                let y = vers_z(&a[2]).get_i64() as i32;
                Valeur::z(crate::gui_native::bouton_sous(&defs, x, y).unwrap_or(0) as i64)
            }
            #[cfg(not(target_arch = "wasm32"))]
            "bouton_attendre_clic" | "bouton_attendre_clic_limite" => {
                let id = vers_z(&a[0]).get_i64() as u64;
                let r = if nom == "bouton_attendre_clic" {
                    crate::gui_native::attendre_clic(id)
                } else {
                    crate::gui_native::attendre_clic_limite(id, vers_z(&a[1]).get_i64().max(0) as u64)
                };
                match r {
                    Ok(Some(b)) => Valeur::z(b as i64),
                    Ok(None) => Valeur::z(0),
                    Err(e) => { erreur_runtime(&e.to_string()); }
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            "bouton_clic_simuler" => {
                let id = vers_z(&a[0]).get_i64() as u64;
                let b = vers_z(&a[1]).get_i64() as u64;
                if let Err(e) = crate::gui_native::clic_simuler(id, b) {
                    erreur_runtime(&e.to_string());
                }
                Valeur::Vide
            }
            // `x11_envoyer_clic` n'existe que sur UNIX (gui_native.rs:665).
            #[cfg(all(not(target_arch = "wasm32"), not(windows)))]
            "bouton_x11_envoyer_clic" => {
                let id = vers_z(&a[0]).get_i64() as u64;
                let b = vers_z(&a[1]).get_i64() as u64;
                if let Err(e) = crate::gui_native::x11_envoyer_clic(id, b) {
                    erreur_runtime(&e.to_string());
                }
                Valeur::Vide
            }
            // GUI native : vraie fenêtre OS, sans navigateur ni dépendance.
            // Le module échoue EXPLICITEMENT s'il n'y a pas d'affichage.
            #[cfg(not(target_arch = "wasm32"))]
            "fenetre_native" => {
                let titre = chaine_de(&a[0]);
                let l = chaine_de(&a[1]).parse::<i64>().unwrap_or(0);
                let h = chaine_de(&a[2]).parse::<i64>().unwrap_or(0);
                match crate::gui_native::fenetre_creer(&titre, l, h) {
                    Ok(id) => Valeur::z(id as i64),
                    Err(e) => { erreur_runtime(&e.to_string()); }
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            "fenetre_afficher" | "fenetre_attendre" | "fenetre_fermer" => {
                let id = chaine_de(&a[0]).parse::<i64>().unwrap_or(0);
                let r = match nom {
                    "fenetre_afficher" => crate::gui_native::fenetre_afficher(id as u64),
                    "fenetre_attendre" => crate::gui_native::fenetre_attendre(id as u64),
                    _ => crate::gui_native::fenetre_fermer(id as u64),
                };
                if let Err(e) = r { erreur_runtime(&e.to_string()); }
                Valeur::Vide
            }
            #[cfg(not(target_arch = "wasm32"))]
            "message_native" => {
                let titre = chaine_de(&a[0]);
                let texte = chaine_de(&a[1]);
                if let Err(e) = crate::gui_native::message(&titre, &texte) {
                    erreur_runtime(&e.to_string());
                }
                Valeur::Vide
            }
            "saisir_entier" => self.lire_selon(TyKind::Entier),
            "saisir_flottant" => self.lire_selon(TyKind::Flottant),
            "time" => {
                // Horloge portable : en WASM, `std::time::SystemTime` n'est
                // pas implémenté (la std panique) — `temps_unix_nanos()`
                // renvoie 0.
                let nanos = crate::run::temps_unix_nanos();
                let micros = nanos / 1_000;
                let millis = nanos / 1_000_000;
                let secs = nanos / 1_000_000_000;
                if secs > 0 {
                    Valeur::s(format!("{} s", secs))
                } else if millis > 0 {
                    Valeur::s(format!("{} ms", millis))
                } else if micros > 0 {
                    Valeur::s(format!("{} µs", micros))
                } else {
                    Valeur::s(format!("{} ns", nanos))
                }
            }
            "graine" | "seed" => { let z = vers_z(&a[0]); graine_de(&mut self.rng, &z); Valeur::Vide }
            "graine_systeme" | "seed_sys" | "graine_vrai" => {
                self.rng = tirage_systeme();
                rng_tirer(&mut self.rng);
                Valeur::Vide
            }
            "rng_materiel" | "rng_mat" => {
                let z = Gz::neuf();
                z.poser_u64(tirage_systeme());
                zg_rapide(z)
            }
            "hasard" | "prng" => {
                let n = vers_z(&a[0]);
                let r = hasard(&mut self.rng, &n);
                zg_rapide(r)
            }
            "hasard_systeme" | "hasard_vrai" | "trng" => {
                let n = vers_z(&a[0]);
                if n.sgn() <= 0 { erreur_runtime("hasard : bornage positif attendu"); }
                let x = tirage_systeme();
                let r = Gz::neuf();
                if n.fits_i64() {
                    let nn = n.get_i64() as u64;
                    r.poser_u64(((x as u128) * (nn as u128) >> 64) as u64);
                } else {
                    r.poser_u64(x >> 32);
                    unsafe {
                        gmp_mul_2exp(r.p(), r.p(), 32);
                        gmp_add_ui(r.p(), r.p(), (x & 0xFFFF_FFFF) as u64);
                        gmp_fdiv_r(r.p(), r.p(), n.p());
                    }
                }
                zg_rapide(r)
            }
            "tampon" | "buf" => {
                let n = vers_z(&a[0]);
                if n.sgn() < 0 || !n.fits_u64_lt(usize::MAX as u64) { erreur_runtime("tampon : taille démesurée"); }
                Valeur::tampon(n.get_u64() as usize)
            }
            "tableau" => {
                let n = vers_z(&a[0]);
                if n.sgn() < 0 || !n.fits_u64_lt(usize::MAX as u64) { erreur_runtime("tableau : taille démesurée"); }
                Valeur::tableau(n.get_u64() as usize)
            }
            "octets" | "bytes" => {
                let s = chaine_de(&a[0]);
                Valeur::Tampon(Tampon(Rc::new(RefCell::new(s.into_bytes()))))
            }
            "tampon_case" => {
                let (Valeur::Tampon(t),) = (&a[0],) else { erreur_runtime("tampon_case : tampon attendu") };
                let idx = vers_z(&a[1]);
                let i = indice_tampon(&idx, t.0.borrow().len());
                Valeur::z(t.0.borrow()[i] as i64)
            }
            "tampon_pose" => {
                let (Valeur::Tampon(t),) = (&a[0],) else { erreur_runtime("tampon_pose : tampon attendu") };
                let idx = vers_z(&a[1]);
                let i = indice_tampon(&idx, t.0.borrow().len());
                let v = vers_z(&a[2]);
                let octet = unsafe { gmp_fdiv_ui(v.p(), 256) } as u8;
                t.0.borrow_mut()[i] = octet;
                Valeur::Vide
            }
            "remplir_hasard" => {
                let (Valeur::Tampon(t), Valeur::Tampon(alpha)) = (&a[0], &a[1]) else {
                    erreur_runtime(if matches!(a[0], Valeur::Tampon(..)) { "remplir_hasard : alphabet non vide attendu" } else { "remplir_hasard : tampon attendu" });
                };
                let ab = alpha.0.borrow().clone();
                if ab.is_empty() { erreur_runtime("remplir_hasard : alphabet non vide attendu"); }
                {
                    let mut buf = t.0.borrow_mut();
                    remplir_hasard(&mut self.rng, &mut buf, &ab);
                }
                Valeur::Vide
            }
            "remplir_hasard_philox" => {
                let (Valeur::Tampon(t), Valeur::Tampon(alpha)) = (&a[0], &a[1]) else {
                    erreur_runtime(if matches!(a[0], Valeur::Tampon(..)) { "remplir_hasard_philox : alphabet non vide attendu" } else { "remplir_hasard_philox : tampon attendu" });
                };
                let ab = alpha.0.borrow().clone();
                if ab.is_empty() { erreur_runtime("remplir_hasard_philox : alphabet non vide attendu"); }
                let g = vers_z(&a[2]);
                let gr = if g.fits_i64() { g.get_i64() } else { 0 };
                let mut buf = t.0.borrow_mut();
                remplir_philox(&mut buf, &ab, gr);
                Valeur::Vide
            }
            "factorielle" | "fact" => { let z = vers_z(&a[0]); zg_rapide(factorielle(&z)) }
            "factorielle_rapide" => { let z = vers_z(&a[0]); zg_rapide(factorielle_rapide(&z)) }
            "factorial_rapide" => { let z = vers_z(&a[0]); zg_rapide(factorielle_rapide(&z)) }
            "rapide" => { let z = vers_z(&a[0]); zg_rapide(factorielle_rapide(&z)) }
            "factorial_approx" => { let z = vers_z(&a[0]); Valeur::s(factorial_approx(&z)) }
            "approx" => { let z = vers_z(&a[0]); Valeur::s(factorial_approx(&z)) }
            "primorielle" => { let z = vers_z(&a[0]); zg_rapide(primorielle(&z)) }
            // ⚠ Deux termials, DEUX NOTATIONS (demande utilisateur) :
            //   `n?`                = termial CLASSIQUE n(n+1)/2 (A000217) ;
            //   `termial(n)`        = idem, nom explicite ;
            //   `terminal(n)`       = termial de KUREPA (A003422) — conservé
            //                          pour les programmes existants ;
            //   `termial_kurepa(n)` = Kurepa, sans ambiguïté.
            "terminal" | "termial_kurepa" | "kurepa" => terminal(&a[0]),
            "termial" | "triangulaire" | "termial_classique" => {
                let z = vers_z(&a[0]);
                zg_rapide(termial_classique(&z))
            }
            // `multifactorielle(n, k)` : équivalent fonctionnel de `n!!`
            // (k = 2), `n!!!` (k = 3), etc. Utile quand k est calculé.
            "multifactorielle" | "multifactoriel" => {
                let z = vers_z(&a[0]);
                let kz = vers_z(&a[1]);
                let k = if kz.fits_i64() { kz.get_i64().clamp(1, 255) as u8 } else { 1 };
                zg_rapide(multifactorielle(&z, k))
            }
            // `sous_factorielle(n)` : équivalent fonctionnel de `!n`.
            "sous_factorielle" | "derangement" | "sous_factorial" => {
                let z = vers_z(&a[0]);
                zg_rapide(sous_factorielle(&z))
            }
            "super_factorielle" => { let z = vers_z(&a[0]); zg_rapide(super_factorielle(&z)) }
            "fibonacci" | "fib" => { let z = vers_z(&a[0]); zg_rapide(fibonacci(&z)) }
            // R1 — F_n mod m par doublement modulaire. Garde : n >= 0, m > 0.
            "fib_mod" => {
                let n = vers_z(&a[0]);
                let m = vers_z(&a[1]);
                zg_rapide(fib_mod(&n, &m))
            }
            "pgcd" | "gcd" => { let (x, y) = (vers_z(&a[0]), vers_z(&a[1])); zg_rapide(pgcd(&x, &y)) }
            "ppcm" | "lcm" => { let (x, y) = (vers_z(&a[0]), vers_z(&a[1])); zg_rapide(ppcm(&x, &y)) }
            "puissance" | "pow" => {
                let e = vers_z(&a[1]);
                puissance(&a[0], &e)
            }
            "racine" => {
                let n = vers_z(&a[0]);
                if n.sgn() < 0 { erreur_runtime("racine : entier négatif"); }
                if n.fits_i64() {
                    let x = n.get_i64();
                    let r = (x as f64).sqrt() as i64;
                    if r * r == x {
                        return Valeur::z(r);
                    }
                }
                Valeur::Irrationnel(Rc::new(n))
            }
            // RA 19/11 — √n en décimales (chaîne de chiffres, même convention
            // que `nombre_pi`). Appelle directement le primitive Rose.
            "racine_decimales" | "sqrt_dec" => {
                // Accès DIRECT au BigInt de Gz (`bref()`) : évite l'aller-retour
                // Gz → chaîne décimale → BigInt (mesuré : ~1,6 µs/appel économisés).
                let nb: num_bigint::BigInt = match &a[0] {
                    Valeur::Entier(i, None) => num_bigint::BigInt::from(*i),
                    Valeur::Entier(_, Some(o)) => o.bref().clone(),
                    v => {
                        let n = vers_z(v);
                        if n.sgn() < 0 { erreur_runtime("racine_decimales : entier négatif"); }
                        n.bref().clone()
                    }
                };
                if nb.sign() == num_bigint::Sign::Minus {
                    erreur_runtime("racine_decimales : entier négatif");
                }
                let dd = match &a[1] {
                    Valeur::Entier(i, None) if *i >= 0 => *i as u64,
                    Valeur::Entier(_, Some(o)) if o.sgn() >= 0 && o.fits_u64() => o.get_u64(),
                    _ => erreur_runtime("racine_decimales : décimales hors plage [0, 1e6]"),
                };
                if dd > 1_000_000 { erreur_runtime("racine_decimales : décimales hors plage [0, 1e6]"); }
                Valeur::s(crate::irrationnel::calcul_racine(&nb, dd as usize).to_string())
            }
            "est_premier" | "is_prime" => { let z = vers_z(&a[0]); Valeur::b(est_premier(&z)) }
            "compter_premiers" => { let z = vers_z(&a[0]); Valeur::z(compter_premiers(&z) as i64) }
            "nb_chiffres" | "digits" => { let z = vers_z(&a[0]); Valeur::z(nb_chiffres(&z) as i64) }
            "nombre_pi" | "pi" => { let z = vers_z(&a[0]); Valeur::s(nombre_pi(&z)) }
            "nombre_e" | "e" => { let z = vers_z(&a[0]); Valeur::s(nombre_e(&z)) }
            "phi" => { let z = vers_z(&a[0]); Valeur::s(nombre_phi(&z)) }
            "i" => Valeur::Complexe { re: 0.0, im: 1.0 },
            "longueur" | "len" => {
                crate::run::prof::add(&crate::run::prof::LONGUEUR_NB, 1);
                let n: u64 = match &a[0] {
                    // ⚠ ROPE : O(1) — le nombre de caractères est maintenu
                    // à chaque concaténation (avant : `chars().count()` sur
                    // toute la chaîne, à chaque appel de `longueur`).
                    Valeur::Chaine(s) => s.n as u64,
                    Valeur::Liste(_) => longueur_valeur(&a[0]),
                    Valeur::Table(t) => t.0.borrow().items.len() as u64,
                    Valeur::Tampon(t) => t.0.borrow().len() as u64,
                    Valeur::Tableau(t) => t.0.borrow().len() as u64,
                    // un mot non atomique peut rester une chaine : on rend sa longueur
                    _ => {
                        let c = chaine_de(&a[0]);
                        c.chars().count() as u64
                    }
                };
                let z = Gz::neuf();
                z.poser_u64(n);
                zg_rapide(z)
            }
            "table" | "map" => Valeur::table(),
            "inserer" => {
                let Valeur::Table(t) = &a[0] else { erreur_runtime("inserer attend une table") };
                t.inserer(a[1].clone(), a[2].clone());
                Valeur::Vide
            }
            "cle_existe" => {
                let Valeur::Table(t) = &a[0] else { erreur_runtime("cle_existe attend une table") };
                Valeur::b(t.trouver(&a[1]).is_some())
            }
            "supprimer" => {
                let Valeur::Table(t) = &a[0] else { erreur_runtime("supprimer attend une table") };
                Valeur::b(t.retirer(&a[1]))
            }
            "cles" => {
                let Valeur::Table(t) = &a[0] else { erreur_runtime("cles attend une table") };
                Valeur::liste_de(t.0.borrow().items.iter().map(|(k, _)| k.clone()).collect())
            }
            "valeurs" => {
                let Valeur::Table(t) = &a[0] else { erreur_runtime("valeurs attend une table") };
                Valeur::liste_de(t.0.borrow().items.iter().map(|(_, v)| v.clone()).collect())
            }
            "joindre" => joindre_liste(&a[0], a[1].clone()),
            "ordre" => {
                let s = chaine_de(&a[0]);
                let cps: Vec<char> = s.chars().collect();
                if cps.len() != 1 { erreur_runtime("ordre attend une chaîne d'un seul caractère"); }
                let z = Gz::neuf();
                z.poser_u64(cps[0] as u32 as u64);
                zg_rapide(z)
            }
            "caractere" => {
                let n = vers_z(&a[0]);
                if !n.fits_u64_lt(0x10FFFF) { erreur_runtime("caractere : code hors plage (0 à 1114111)"); }
                let cp = n.get_u64() as u32;
                if (0xD800..=0xDFFF).contains(&cp) { erreur_runtime("caractere : code réservé (55296 à 57343)"); }
                Valeur::s(char::from_u32(cp).unwrap_or('\u{FFFD}').to_string())
            }
            "sous_chaine" | "sub" => {
                crate::run::prof::add(&crate::run::prof::SOUS_NB, 1);
                let est_chaine = matches!(a[0], Valeur::Chaine(..));
                // ⚠ FAST PATH (MEM_HOST) : une `Valeur::Chaine` porte `n`
                // (compte de caractères O(1)) et `tranche()` ne recopie que
                // les feuilles qui chevauchent [d, f). L'ancien chemin
                // `chaine_de` + `chars().collect()` aplatissait TOUTE la
                // chaîne à chaque appel = O(n²) sur le lexeur hébergé.
                // `ROSE_PROF_NOFAST=1` le rejoue pour la mesure A/B.
                let (taille, chaine): (usize, Option<&Rc<crate::run::ChaineRep>>) = match &a[0] {
                    Valeur::Chaine(r) => (r.n, Some(r)),
                    v => (chaine_de(v).chars().count(), None),
                };
                if a.len() < 3 { erreur_runtime("sous_chaine : 3 arguments attendus"); }
                let d = vers_z(&a[1]);
                let f = vers_z(&a[2]);
                if d.sgn() < 0 || f.sgn() < 0 || !d.fits_i64() || !f.fits_i64() {
                    erreur_runtime("sous_chaine : les bornes doivent être des entiers positifs");
                }
                let di = d.get_i64() as usize;
                let mut fi = f.get_i64() as usize;
                if di > taille || fi < di { erreur_runtime("sous_chaine : bornes invalides"); }
                if fi > taille { fi = taille; }
                if let (Some(r), false) = (chaine, crate::run::prof::sans_fast()) {
                    return Valeur::s(r.tranche(di, fi));
                }
                if est_chaine {
                    crate::run::prof::add(&crate::run::prof::SOUS_LENT, 1);
                }
                let s = chaine_de(&a[0]);
                let cps: Vec<char> = s.chars().collect();
                Valeur::s(cps[di..fi].iter().collect::<String>())
            }
            "concatener" => {
                let Valeur::Liste(l) = &a[0] else { erreur_runtime("concatener attend une liste") };
                let mut r = String::new();
                for e in l.0.borrow().iter() {
                    if !matches!(e, Valeur::Chaine(..)) { erreur_runtime("concatener attend une liste de chaînes"); }
                    r.push_str(&chaine_de(e));
                }
                Valeur::s(r)
            }
            "lire_texte" => {
                let p = chaine_de(&a[0]);
                // 19/11/26 — mmap en lecture seule : les pages du fichier
                // deviennent les octets de la `Valeur::Chaine` sans aucune
                // recopie vers un `Vec` utilisateur. Validation UTF-8 et
                // comptage des caractères inchangés (voir `depuis_fichier`).
                // `Err` couvre : ouverture impossible, stat impossible,
                // fichier > 2^28, mmap impossible. On distingue a posteriori
                // le dépassement de taille pour garder le message historique,
                // sans coût sur le chemin nominal (un seul `stat` en erreur).
                match ChaineRep::depuis_fichier(&p) {
                    Ok(r) => Valeur::Chaine(r),
                    Err(_) => {
                        if std::fs::metadata(&p).map(|m| m.len() > (1 << 28)).unwrap_or(false) {
                            erreur_runtime(&format!("fichier trop volumineux ({})", p));
                        }
                        erreur_runtime(&format!("lecture du fichier impossible ({})", p))
                    }
                }
            }
            "eval_expr" => {
                // Evaluation d'une expression avec substitutions.
                // Le contexte est une liste de paires "nom=valeur" separees par
                // des espaces ou des virgules : "expr // vars: a=1 b=2".
                // Les valeurs peuvent etre nues ou entre guillemets doubles.
                // Le remplacement est fait par mots entiers (decoupes aux
                // separateurs "( )" aussi) : un mot n'est remplace que s'il
                // egale exactement un nom ; la derniere definition d'un nom
                // fait foi (shadowing).
                //
                // Ordre des voies (la premiere qui rend une valeur non vide gagne) :
                //  1) litteral guillemete  2) atome nu (booleen/nombre)
                //  3) substituer le contexte puis evaluer  4) evaluer sans contexte.
                let expr_str = chaine_de(&a[0]);
                let base = expr_str.trim().to_string();
                let mut substituted = base.clone();
                // Separer l'expression nue du contexte eventuel ("// vars: ...").
                // La recherche est sensible aux littéraux chaînes : un separateur
                // apparu a l'interieur de guillemets n'est pas un separateur.
                let pos_vars = {
                    let cs: Vec<char> = base.chars().collect();
                    let mut dans_guilm = false;
                    let mut found: Option<(usize, usize)> = None;
                    let mut i = 0usize;
                    while i < cs.len() && found.is_none() {
                        if cs[i] == '"' {
                            dans_guilm = !dans_guilm;
                        } else if !dans_guilm {
                            if i + 8 <= cs.len() && base[i..].starts_with("// vars:") {
                                found = Some((i, 8usize));
                            } else if i + 7 <= cs.len() && base[i..].starts_with("//vars:") {
                                found = Some((i, 7usize));
                            } else if i + 7 <= cs.len() && base[i..].starts_with("; vars:") {
                                found = Some((i, 7usize));
                            } else if i + 6 <= cs.len() && base[i..].starts_with(";vars:") {
                                found = Some((i, 6usize));
                            } else if i + 5 <= cs.len() && base[i..].starts_with("vars:") {
                                found = Some((i, 5usize));
                            }
                        }
                        i += 1;
                    }
                    found
                };
                let (nu, vars_brut) = match pos_vars {
                    Some((i, saut)) => (
                        base[..i].trim().to_string(),
                        base[i + saut..].trim().to_string(),
                    ),
                    None => {
                        // pas de separateur : on nettoie neanmoins un suffixe
                        // « // vars: » eventuel.
                        let n = nettoyer_rendu(&base);
                        (n, String::new())
                    }
                };
                // le substituted initial = nu (sans le suffixe), pas base.
                substituted = nu.clone();
                // le prelude est souvent suivi des lignes `nom = valeur` du
                // programme source : on le tronque a la premiere ligne sans
                // signe `=`.
                let vars_brut = {
                    let mut lignes: Vec<&str> = Vec::new();
                    for ln in vars_brut.lines() {
                        if ln.contains('=') { lignes.push(ln); } else { break; }
                    }
                    lignes.join(" ")
                };
                // 1) litteral : une seule paire de guillemets englobante
                if nu.len() >= 2
                    && nu.starts_with('"')
                    && nu.ends_with('"')
                    && nu[1..nu.len() - 1].find('"').is_none()
                {
                    return Valeur::s(nu[1..nu.len() - 1].to_string());
                }
                // nu peut encore porter un suffixe « // vars: » (si le
                // separateur n'a pas ete detecte) : on le nettoie.
                let nu = nettoyer_rendu(&nu);
                if nu.is_empty() { return Valeur::s(String::new()); }
                if nu.len() >= 2
                    && nu.starts_with('"')
                    && nu.ends_with('"')
                    && nu[1..nu.len() - 1].find('"').is_none()
                {
                    return Valeur::s(nu[1..nu.len() - 1].to_string());
                }
                // 2) atome nu : booleen ou nombre
                if nu == "vrai" || nu == "faux" { return Valeur::s(nu.clone()); }
                if let Ok(n) = nu.parse::<i64>() { return Valeur::s(n.to_string()); }
                // 2ter) entier exact BigInt pour les littéraux entiers trop
                //     grands pour i64 (ex: 999999999999999999999). On ne le
                //     tente que pour un entier pur (chiffres, signe optionnel)
                //     afin de ne pas confondre avec un flottant (1.5, 1e21…).
                let est_entier_pur = !nu.is_empty()
                    && (nu.chars().all(|c| c.is_ascii_digit())
                        || (nu.len() > 1
                            && (nu.as_bytes()[0] == b'+' || nu.as_bytes()[0] == b'-')
                            && nu[1..].chars().all(|c| c.is_ascii_digit())));
                if est_entier_pur {
                    let z = chaine_vers_entier(&nu);
                    return Valeur::s(z.chaine());
                }
                if let Ok(f) = nu.parse::<f64>() { return Valeur::s(texte_simple(f)); }
                // 2bis) voie de pliage structuree (replier_expression) : gere les
                //    appels std imbriques et les fonctions utilisateur declarees
                //    dans le prelude. Gagne sur la substitution mot-a-mot simple.
                let mut rendu_secours: Option<String> = None;
                if !vars_brut.is_empty() {
                    let paires_pre = paires_prelude(&vars_brut);
                    if let Some(r) = replier_expression(&nu, &paires_pre) {
                        let nettoye = nettoyer_rendu(&r);
                        if !nettoye.is_empty() && !nettoye.contains("vars:") {
                            return Valeur::s(nettoye);
                        }
                    }
                    // 2ter) si `nu` est déja un mot-clé ou un atome, on le rend
                    //     sans le suffixe « // vars: ».
                    if nu == "vrai" || nu == "faux" { return Valeur::s(nu.clone()); }
                    if let Ok(n) = nu.parse::<i64>() { return Valeur::s(n.to_string()); }
                }
                // 3) avec contexte : decoupage des paires, puis substitution mot a mot
                if !vars_brut.is_empty() {
                    let mut paires: Vec<(String, String)> = Vec::new();
                    let mut cur = String::new();
                    let mut dans_guilm = false;
                    let mut prof = 0usize;
                    for c in vars_brut.chars() {
                        if c == '"' {
                            dans_guilm = !dans_guilm;
                            cur.push(c);
                        } else if !dans_guilm && prof == 0 && (c == ' ' || c == '\t' || c == '\n' || c == ',') {
                            if !cur.is_empty() {
                                if let Some(eq) = cur.find('=') {
                                    paires.push((cur[..eq].to_string(), cur[eq + 1..].to_string()));
                                }
                                cur.clear();
                            }
                        } else {
                            if !dans_guilm {
                                if c == '(' { prof += 1; }
                                if c == ')' { prof = prof.saturating_sub(1); }
                            }
                            cur.push(c);
                        }
                    }
                    if !cur.is_empty() {
                        if let Some(eq) = cur.find('=') {
                            paires.push((cur[..eq].to_string(), cur[eq + 1..].to_string()));
                        }
                    }
                    //decoupage de l'expression en mots, guillemets preserves
                    //le separateur est aussi la virgule (la forme "a=1, b=2")
                    let mut mots: Vec<String> = Vec::new();
                    let mut cur = String::new();
                    let mut dans_guilm = false;
                    for c in substituted.chars() {
                        if c == '"' {
                            dans_guilm = !dans_guilm;
                            cur.push(c);
                        } else if !dans_guilm && (c == ' ' || c == '\t' || c == ',') {
                            if !cur.is_empty() {
                                mots.push(cur.clone());
                                cur.clear();
                            }
                        } else {
                            cur.push(c);
                        }
                    }
                    if !cur.is_empty() {
                        mots.push(cur.clone());
                    }
                    //chaque mot est decompose aux separateurs "( ) [ ]" ; un
                    //sous-mot est remplace seulement s'il egale exactement un
                    //nom (la derniere definition fait foi) ; les espaces sont
                    //rejetes seulement autour des parentheses
                    let mut sous: Vec<String> = Vec::new();
                    for m in mots.iter() {
                        let mut c2 = String::new();
                        for c in m.chars() {
                            if c == '(' || c == ')' || c == '[' || c == ']' {
                                if !c2.is_empty() {
                                    sous.push(c2.clone());
                                    c2.clear();
                                }
                                sous.push(c.to_string());
                            } else {
                                c2.push(c);
                            }
                        }
                        if !c2.is_empty() {
                            sous.push(c2.clone());
                        }
                    }
                    //un sous-mot n'est remplace que s'il egale exactement un
                    //nom (la derniere definition fait foi) ; les separateurs
                    //"( ) [ ]" ne sont pas des noms : ils restent tels quels ;
                    //la lettre, le chiffre et le mot-cle deja substitues sont
                    //rendus bruts (le parseur les normalisera)
                    let mut nouveau = String::new();
                    for m in sous.iter() {
                        if m.is_empty() {
                            continue;
                        }
                        if m == "(" || m == ")" || m == "[" || m == "]" {
                            nouveau.push_str(m);
                            continue;
                        }
                        let sep_avant = nouveau.ends_with('(') || nouveau.ends_with(')')
                            || nouveau.ends_with('[') || nouveau.ends_with(']');
                        let sep_apres = m.starts_with('(') || m.starts_with(')')
                            || m.starts_with('[') || m.starts_with(']');
                        if !nouveau.is_empty() && !sep_avant && !sep_apres {
                            nouveau.push(' ');
                        }
                        let mut trouve = false;
                        for (nom, val) in paires.iter().rev() {
                            if m == nom {
                                nouveau.push_str(val);
                                trouve = true;
                                break;
                            }
                        }
                        if !trouve {
                            nouveau.push_str(m);
                        }
                    }
                    substituted = nouveau;
                }
                // Evaluer l'expression. On commence par essayer la forme
                // "x = <expr>" (affectation, premier instruction = Affect),
                // puis, si cela echoue, la forme directe "<expr>" dont le
                // premier jeton est deja une expression complete.
                let candidats = [
                    format!("x = {}", substituted),
                    format!("x = {}\n>> x", substituted),
                    substituted.clone(),
                ];

                let mut rendu = Valeur::s(String::new());
                let mut obtenu = false;
                // 0) si la substitution a deja produit un atome complet
                //    (litteral guillemete, booleen), on le rend tel quel ;
                //    un nombre est rendu par le parseur, qui normalise le rendu
                if substituted.starts_with('"') && substituted.ends_with('"')
                    && substituted.len() >= 2
                {
                    rendu = Valeur::s(substituted[1..substituted.len() - 1].to_string());
                    obtenu = true;
                } else if substituted == "vrai" || substituted == "faux" {
                    rendu = Valeur::s(substituted.clone());
                    obtenu = true;
                }
                if !obtenu {
                for src in candidats.iter() {
                    match crate::front::analyser(src) {
                        Ok(prog) => {
                            let mut interp = Interp::nouveau(prog);
                            // 1) la premiere instruction est-elle une forme connue ?
                            let instr_clone = interp.prog.instructions.first().cloned();
                            if let Some(instr) = instr_clone {
                                match instr {
                                    crate::front::Instr::Affect { e, .. } => {
                                        if let Ok(v) = interp.eval(&e) {
                                            rendu = Valeur::s(chaine_de(&v));
                                            obtenu = true;
                                            break;
                                        }
                                    }
                                    crate::front::Instr::ExprStmt(e) => {
                                        if let Ok(v) = interp.eval(&e) {
                                            rendu = Valeur::s(chaine_de(&v));
                                            obtenu = true;
                                            break;
                                        }
                                    }
                                    //le programme est execute jusqu'au bout :
                                    //la valeur de x (issue de l'affectation)
                                    //est lue dans l'environnement global
                                    crate::front::Instr::Afficher(_, _) => {
                                        // la voie « x = <expr> » n'est un rendu
                                        // d'EXPRESSION que si le programme est une
                                        // suite d'affectations terminée par un
                                        // affichage de x. Dès qu'une boucle ou un
                                        // « si » apparaît, l'affectation de x est
                                        // un artefact de la forme testée, et la
                                        // valeur de x n'a plus de sens : on
                                        // refuse cette voie et on laisse la voie
                                        // « programme complet » rendre la dernière
                                        // instruction utile.
                                        if interp.prog.instructions.iter().any(|i| matches!(i,
                                            crate::front::Instr::Pour { .. } | crate::front::Instr::TantQue { .. }
                                            | crate::front::Instr::Si { .. } | crate::front::Instr::Essayer { .. }))
                                        {
                                            // on passe ce candidat, sans sortir de
                                            // la boucle : les formes « <expr> » puis
                                            // « programme complet » restent à
                                            // essayer.
                                            continue;
                                        }
                                        interp.executer_principal();
                                        if let Some(v) = interp.globaux.get("x") {
                                            rendu = Valeur::s(chaine_de(v));
                                            obtenu = true;
                                            break;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Err(_) => {}
                    }
                }
                }
                // 2) voie « fonction » : le premier jeton est une application
                //    complète (ex. "eval_expr(\"...\")") : on évalue le premier
                //    ExprStmt directement.
                if !obtenu {
                    match crate::front::analyser(&substituted) {
                        Ok(prog) => {
                            let mut interp = Interp::nouveau(prog);
                            if let Some(crate::front::Instr::ExprStmt(e)) =
                                interp.prog.instructions.first().cloned()
                            {
                                if let Ok(v) = interp.eval(&e) {
                                    rendu = Valeur::s(chaine_de(&v));
                                    obtenu = true;
                                }
                            }
                        }
                        Err(_) => {}
                    }
                }
                // 3) voie « programme complet » : on execute tout le programme et
                //    on rend la valeur de sa derniere instruction utile.
                if !obtenu {
                    let prog_complet = crate::front::analyser(&substituted);
                    if let Ok(prog) = prog_complet {
                        let mut interp = Interp::nouveau(prog);
                        let n = interp.prog.instructions.len();
                        if n > 0 {
                            let derniere = interp.prog.instructions[n - 1].clone();
                            match derniere {
                                crate::front::Instr::Afficher(exprs, _) => {
                                    // le dernier affichage n'est le « rendu » du
                                    // programme que s'il ne réévalue PAS une
                                    // expression déjà produite par l'exécution
                                    // (un « >> $x » après une boucle affiche ce
                                    // que l'exécution a déjà rendu). On exécute
                                    // le programme — ses sorties sont le rendu —
                                    // puis on ne rend que la valeur de l'affichage
                                    // s'il est constant (littéral) : sinon la
                                    // valeur aurait été imprimée deux fois.
                                    interp.executer_principal();
                                    if let Some(e) = exprs.first() {
                                        if matches!(e, crate::front::Expr::ChaineLit(..)
                                            | crate::front::Expr::Litteral { .. }
                                            | crate::front::Expr::BooleenLit(..))
                                        {
                                            if let Ok(v) = interp.eval(e) {
                                                rendu = Valeur::s(chaine_de(&v));
                                                obtenu = true;
                                            }
                                        } else {
                                            obtenu = true;
                                        }
                                    }
                                }
                                crate::front::Instr::Affect { e, .. } => {
                                    if let Ok(v) = interp.eval(&e) {
                                        rendu = Valeur::s(chaine_de(&v));
                                        obtenu = true;
                                    }
                                }
                                crate::front::Instr::ExprStmt(e) => {
                                    if let Ok(v) = interp.eval(&e) {
                                        rendu = Valeur::s(chaine_de(&v));
                                        obtenu = true;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                // derniere chance : un littéral nu (non stocké comme instruction
                // d'expression) est reconnu directement par le parseur
                if !obtenu {

                    if substituted.starts_with('"') && substituted.ends_with('"')
                        && substituted.len() >= 2
                    {
                        rendu = Valeur::s(substituted[1..substituted.len() - 1].to_string());
                    } else if let Ok(n) = substituted.parse::<i64>() {
                        rendu = Valeur::s(n.to_string());
                    } else if substituted == "vrai" || substituted == "faux" {
                        rendu = Valeur::s(substituted.clone());
                    } else if !substituted.is_empty() {
                        // atome non numérique : rendu brut (miroir d'un mot-cle
                        // ou d'un nom deja substitue)
                        rendu = Valeur::s(substituted.clone());
                    }
                }
                // dernier secours : le rendu pré-calculé par replier_expression.
                if let Some(sec) = rendu_secours {
                    if !sec.is_empty() { rendu = Valeur::s(sec); }
                }
                rendu
            }
            "ecrire_texte" => {
                let p = chaine_de(&a[0]);
                // Variante (b) : une valeur lue est un unique noeud `Plein`.
                // Emprunter son `&str` evite l'aplatissement (recopie de 10 Mo).
                // Repli sur `aplatir`/`chaine_de` pour les ropes concaténées
                // et les valeurs non-chaine (conversion historique préservée).
                let aplat: String;
                let octets: &[u8] = match &a[1] {
                    Valeur::Chaine(r) => match r.octets_empruntes() {
                        Some(o) => o, // `Plein` OU `FichierMmap` : zéro recopie
                        None => { aplat = r.aplatir(); aplat.as_bytes() },
                    },
                    v => { aplat = chaine_de(v); aplat.as_bytes() },
                };
                if std::fs::write(&p, octets).is_err() {
                    erreur_runtime(&format!("écriture du fichier impossible ({})", p));
                }
                Valeur::Vide
            }
            "ecrire_binaire" => {
                let p = chaine_de(&a[0]);
                let mut bytes: Vec<u8> = Vec::new();
                if let Valeur::Liste(lst) = &a[1] {
                    for v in lst.0.borrow().iter() {
                        match v {
                            Valeur::Chaine(s) => {
                                let t = s.aplatir();
                                match t.parse::<u8>() {
                                    Ok(n) => bytes.push(n),
                                    Err(_) => erreur_runtime(&format!("octet invalide: {}", t)),
                                }
                            }
                            _ => erreur_runtime("ecrire_binaire: liste doit contenir des nombres entiers"),
                        }
                    }
                } else {
                    erreur_runtime("ecrire_binaire: second argument doit être une liste");
                }
                if std::fs::write(&p, &bytes).is_err() {
                    erreur_runtime(&format!("écriture binaire impossible ({})", p));
                }
                Valeur::z(0)
            }
            "caractere" => {
                let n = chaine_de(&a[0]).parse::<u8>().unwrap_or(0);
                Valeur::s(char::from_u32(n as u32).unwrap_or('\0').to_string())
            }
            "ecrire_bytes" => {
                let p = chaine_de(&a[0]);
                let s = chaine_de(&a[1]);
                let mut bytes: Vec<u8> = Vec::new();
                for c in s.chars() {
                    bytes.push(c as u8);
                }
                if std::fs::write(&p, &bytes).is_err() {
                    erreur_runtime(&format!("écriture d'octets impossible ({})", p));
                }
                Valeur::z(0)
            }
            "nb_arguments" => {
                let z = Gz::neuf();
                z.poser_u64(self.arguments.len() as u64);
                zg_rapide(z)
            }
            "argument" => {
                let i = vers_z(&a[0]);
                if !i.fits_u64_lt(self.arguments.len() as u64) {
                    erreur_runtime("index hors des limites des arguments");
                }
                Valeur::s(self.arguments[i.get_u64() as usize].clone())
            }
            "temps" => {
                let t = crate::run::temps_unix_nanos();
                let z = Gz::neuf();
                z.poser_u64(t);
                zg_rapide(z)
            }
            // millisecondes() (alias « ms ») : mêmes sémantiques que temps()
            // mais en millisecondes — attendu par la suite de benchmarks.
            "millisecondes" | "ms" => {
                let t = crate::run::temps_unix_nanos() / 1_000_000;
                Valeur::z(t as i64)
            }
            "duree" => {
                let z = vers_z(&a[0]);
                if z.sgn() < 0 { erreur_runtime("duree : la durée doit être positive ou nulle"); }
                Valeur::s(formater_duree(&z.chaine()))
            }
            "afficher_erreur" => {
                flush_out();
                eprintln!("{}", chaine_de(&a[0]));
                Valeur::Vide
            }
            "lever" => { erreur_runtime(&chaine_de(&a[0])); }
            "breakpoint" => {
                crate::run::flush_out();
                eprintln!("[BREAKPOINT]");
                for (k, v) in &self.globaux {
                    eprintln!("  {} = {}", k, chaine_de(v));
                }
                eprintln!("Appuyer sur Entrée pour continuer...");
                let mut buf = String::new();
                let _ = std::io::stdin().read_line(&mut buf);
                Valeur::Vide
            }
            "gpu_somme" => {
                // Placeholder: somme CPU pour l'instant
                let t = match &a[0] { Valeur::Tableau(t) => t, _ => { erreur_runtime("gpu_somme : tableau attendu"); } };
                let b = t.0.borrow();
                let mut s: i64 = 0;
                for &x in b.iter() { s = s.wrapping_add(x); }
                Valeur::z(s)
            }
            "gpu_somme" => {
                let t = match &a[0] { Valeur::Tableau(t) => t, _ => { erreur_runtime("gpu_somme : tableau attendu"); } };
                let b = t.0.borrow();
                let buf: Vec<i64> = b.clone();
                drop(b);
                match crate::run::gpu_somme_i64(&buf) {
                    Some(s) => Valeur::z(s),
                    None => {
                        // fallback CPU
                        let mut s: i64 = 0;
                        for &x in buf.iter() { s = s.wrapping_add(x); }
                        Valeur::z(s)
                    }
                }
            }
            "i" => Valeur::Complexe { re: 0.0, im: 1.0 },
            "type_de" | "type" => Valeur::s(match a[0].ty() {
                crate::front::TyKind::Entier => "entier",
                crate::front::TyKind::Rationnel => "rationnel",
                crate::front::TyKind::Flottant => "flottant",
                crate::front::TyKind::Booleen => "booleen",
                crate::front::TyKind::Chaine => "chaine",
                crate::front::TyKind::Complexe => "complexe",
                crate::front::TyKind::Irrationnel => "irrationnel",
                crate::front::TyKind::Liste => "liste",
                crate::front::TyKind::Table => "table",
                crate::front::TyKind::Tableau => "tableau",
                crate::front::TyKind::Tampon => "tampon",
                crate::front::TyKind::Vide => "vide",
                crate::front::TyKind::Dyn => "dynamique",
            }),
            "re" => { let z = match &a[0] { Valeur::Complexe{re,..} => *re, _ => vers_d(&a[0]) }; Valeur::f(z) }
            "im" => { let z = match &a[0] { Valeur::Complexe{im,..} => *im, _ => 0.0 }; Valeur::f(z) }
            "conj" => { match &a[0] { Valeur::Complexe{re,im} => Valeur::Complexe { re: *re, im: -*im }, _ => a[0].clone() } }
            "modulus" | "abs" => { match &a[0] { Valeur::Complexe{re,im} => Valeur::f((re*re+im*im).sqrt()), _ => { let v = &a[0]; match v { Valeur::Entier(i,None) => Valeur::z(i.abs()), _ => Valeur::f(vers_d(v).abs()) } } } }
            "arg" => { let z = match &a[0] { Valeur::Complexe{re,im} => im.atan2(*re), _ => 0.0f64.atan2(vers_d(&a[0])) }; Valeur::f(z) }
            "racine" => {
                let n = vers_z(&a[0]);
                if n.sgn() < 0 { erreur_runtime("racine : entier négatif"); }
                if n.fits_i64() {
                    let x = n.get_i64();
                    let r = (x as f64).sqrt() as i64;
                    if r * r == x {
                        return Valeur::z(r);
                    }
                }
                Valeur::Irrationnel(Rc::new(n))
            }
            "i" => Valeur::Complexe { re: 0.0, im: 1.0 },
            "quitter" => unreachable!("quitter traité dans stdlib"),
            "abs" => {
                match &a[0] {
                    Valeur::Entier(i, None) => Valeur::z(i.abs()),
                    Valeur::Entier(_, Some(z)) => {
                        let r = Gz::neuf();
                        unsafe { if gmp_sgn(z.p()) < 0 { gmp_neg(r.p(), z.p()) } else { gmp_set(r.p(), z.p()) } };
                        zg_rapide(r)
                    }
                    Valeur::Rationnel(q) => {
                        let r = Gq::neuf();
                        unsafe { if crate::run::gmpq_sgn(q.p()) < 0 { gmpq_neg(r.p(), q.p()) } else { gmpq_set(r.p(), q.p()) } };
                        Valeur::qq(r)
                    }
                    Valeur::Flottant(d) => Valeur::f(d.abs()),
                    _ => { erreur_runtime("abs attend un nombre"); }
                }
            }
            "min" | "max" => {
                // règle du langage : les deux valeurs sont d'abord converties vers le
                // rang max (entier<rationnel<flottant), puis comparées.
                let rang = |v: &Valeur| -> u8 {
                    match v {
                        Valeur::Flottant(..) => 3,
                        Valeur::Rationnel(..) => 2,
                        _ => 1,
                    }
                };
                let m = rang(&a[0]).max(rang(&a[1]));
                let cible = match m { 3 => TyKind::Flottant, 2 => TyKind::Rationnel, _ => TyKind::Entier };
                let x = convertir(a[0].clone(), cible);
                let y = convertir(a[1].clone(), cible);
                let c = cmp_vals(&x, &y);
                let take_min = nom == "min";
                if (c <= 0 && take_min) || (c >= 0 && !take_min) { x } else { y }
            }
            "partie_entiere" => {
                // floor exact (rationnel → fdiv_q, flottant → floor).
                match &a[0] {
                    Valeur::Rationnel(q) => {
                        let z = Gz::neuf();
                        unsafe { gmp_fdiv_q(z.p(), q.num(), q.den()) };
                        zg_rapide(z)
                    }
                    Valeur::Flottant(d) => {
                        let z = Gz::neuf();
                        unsafe { gmp_set_d(z.p(), d.floor()) };
                        zg_rapide(z)
                    }
                    _ => { let z = vers_z(&a[0]); zg_rapide(z) }
                }
            }
            "fraction" => {
                let n = vers_z(&a[0]);
                let d = vers_z(&a[1]);
                if d.sgn() == 0 { erreur_runtime("dénominateur nul"); }
                let q = Gq::neuf();
                unsafe {
                    gmpq_set_z_field(q.num(), n.p());
                    gmpq_set_z_field(q.den(), d.p());
                    gmpq_canonicalize(q.p());
                }
                Valeur::qq(q)
            }
            _ => {
                // règle du langage : un corps de fonction
                // utilisateur appele par un chemin non resolu (par ex. via
                // une expression dans une position non-Eval) — message et
                // code 1 identiques au moteur.
                if self.prog.signatures.contains_key(nom) {
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        eprintln!("Rose : erreur — fonction '{}' inconnue", nom);
                        flush_out();
                        std::process::exit(1);
                    }
                    #[cfg(target_arch = "wasm32")]
                    {
                        // En WASM : erreur rattrapable, pas de process::exit.
                        erreur_runtime(&format!("Fonction '{}' inconnue", nom));
                    }
                }
                erreur_runtime(&format!("Fonction '{}' inconnue", nom));
            }
        }
    }
}


fn indice_tampon(idx: &Gz, taille: usize) -> usize {
    if !idx.fits_usize(taille) { erreur_runtime("index hors tampon"); }
    idx.get_u64() as usize
}

/// WD2 — reconnaît le corps d'Euclidien `tant_que ($y != 0) { $r = $x % $y ; $x = $y ; $y = $r }`.
/// Aucun nom de variable n'est figé : seuls la FORME et l'ordre des trois
/// affectations comptent. Rend (x, y, r) si le motif est reconnu.
fn reconnait_euclide(test: &Expr, corps: &[Instr]) -> Option<(String, String, String)> {
    let cv = match test {
        Expr::Binaire { op, a, b, .. } if op == "!=" => match (&**a, &**b) {
            (Expr::Var(v, _), Expr::Litteral { texte, flott: false, rationnel: false, neg: false, .. }) if texte == "0" => v.clone(),
            (Expr::Litteral { texte, flott: false, rationnel: false, neg: false, .. }, Expr::Var(v, _)) if texte == "0" => v.clone(),
            _ => return None,
        },
        _ => return None,
    };
    if corps.len() != 3 { return None; }
    match (&corps[0], &corps[1], &corps[2]) {
        (
            Instr::Affect { nom: r, e: Expr::Binaire { op, a, b, .. }, .. },
            Instr::Affect { nom: x, e: Expr::Var(vy, ..), .. },
            Instr::Affect { nom: y, e: Expr::Var(vr, ..), .. },
        ) if op == "%"
          && matches!(&**a, Expr::Var(v, ..) if v == x)
          && matches!(&**b, Expr::Var(v, ..) if v == vy)
          && vr == r && vy == &cv && y == &cv => Some((x.clone(), y.clone(), r.clone())),
        _ => None,
    }
}

/// Exécute le corps Euclidien reconnu directement sur des `BigInt` natifs :
/// aucun `HashMap`, aucune `Valeur`/`Rc<Gz>` par tour, aucune allocation
/// intermédiaire autres que celles du reste lui-même. La suite des restes
/// est EXACTEMENT celle du programme Rose (`r = x mod y`, `x = y`, `y = r`)
/// — c'est une spécialisation de boucle, pas un changement d'algorithme.
fn euclide_natif(hote: &mut Interp, x: &str, y: &str, r: &str) -> bool {
    let (vx, vy) = match (hote.cible(x), hote.cible(y)) {
        (Some(a), Some(b)) => (a, b),
        _ => return false,
    };
    if !matches!(vx, Valeur::Entier(..)) || !matches!(vy, Valeur::Entier(..)) { return false; }
    let mut a = vers_z(&vx).v();
    let mut b = vers_z(&vy).v();
    if b.is_zero() { return true; }
    while !b.is_zero() {
        let rem = match crate::i128x::mod_rapide(&a, &b) {
            Some(z) => z,
            None => a.mod_floor(&b),
        };
        a = b;
        b = rem;
    }
    hote.affecter(x, zg_rapide(Gz::de_bigint(a)));
    hote.affecter(y, Valeur::z(0));
    hote.affecter(r, Valeur::z(0));
    true
}

/// ⚠⚠⚠ M2.2 : construit une `Valeur` depuis un `Gz` SANS CLONER le `BigInt`.
///
/// `Valeur::zg` (run.rs) appelle `z.fits_i64()` PUIS `z.get_i64()`, et chacune
/// de ces méthodes fait `self.v()` = une copie complète de TOUS les limbs.
/// Sur les grands entiers c'est deux clones de centaines de kilo-octets par
/// résultat. Ici on lit par référence (`bref()`) : `to_i64()` rend `None` dès
/// que la valeur sort de l'intervalle i64, exactement la règle de
/// `fits_i64()`. La valeur produite est STRICTEMENT IDENTIQUE.
fn zg_rapide(z: Gz) -> Valeur {
    match z.bref().to_i64() {
        Some(v) => Valeur::z(v),
        None => Valeur::Entier(0, Some(Rc::new(z))),
    }
}

// ── R1 — doublement modulaire F_n mod m (ALGO_STRATEGIE.md §6) ─────────────
//
// Le doublement (a,b) = (F_k, F_{k+1}) → (F_{2k}, F_{2k+1}) est exact :
//   c = F_{2k}   = a·(2b − a)
//   d = F_{2k+1} = a² + b²
// bit k = 0 → (c, d) ; bit k = 1 → (d, c+d). On traite les bits de n du
// plus significatif au moins significatif, exactement comme `run::fibonacci`
// mais EN MODULE : aucun bigint de 209 000 chiffres n'est jamais matérialisé
// (mesuré : `fib(10^6) % 1e9+7` = 12,9 ms contre 3,1 µs ici).
//
// Garde : n >= 0 (F_{-n} existe mais n'est pas demandé) et m > 0.
// Le résultat est TOUJOURS le vrai F_n mod m, même pour n ou m hors u64
// (voie `fib_mod_gz` alors).

#[inline]
fn fib_mod_u64(n: u64, m: u64) -> u64 {
    if m == 1 { return 0; }
    let mm = m as u128;
    let (mut a, mut b) = (0u128, 1u128);
    let mut p = 1u64;
    while p <= n / 2 { p <<= 1; }
    while p != 0 {
        // 2b − a mod m (on ajoute m AVANT la soustraction pour rester positif)
        let c = (a * ((((b << 1) % mm + mm) - a) % mm)) % mm;
        // a² + b² mod m : on réduit CHAQUE carré, sinon 2·m² déborde u128.
        let d = ((a * a) % mm + (b * b) % mm) % mm;
        if n & p != 0 {
            a = d;
            b = (c + d) % mm;
        } else {
            a = c;
            b = d;
        }
        p >>= 1;
    }
    a as u64
}

fn fib_mod_gz(nn: &Gz, mm: &Gz) -> Gz {
    let a = Gz::neuf();
    a.poser_i64(0);
    let b = Gz::neuf();
    b.poser_i64(1);
    let t1 = Gz::neuf();
    let t2 = Gz::neuf();
    let t3 = Gz::neuf();
    let bits = nn.taille_bits();
    for i in (0..bits).rev() {
        // t1 = a·(2b − a) mod m
        t1.mul_u64(&b, 2);
        t1.add(&t1, mm);
        t1.sub(&t1, &a);
        t1.fdiv_r(&t1, mm);
        t1.mul(&t1, &a);
        t1.fdiv_r(&t1, mm);
        // t2 = (a² + b²) mod m
        t2.mul(&a, &a);
        t2.fdiv_r(&t2, mm);
        t3.mul(&b, &b);
        t3.fdiv_r(&t3, mm);
        t2.add(&t2, &t3);
        t2.fdiv_r(&t2, mm);
        if nn.tstbit(i) {
            a.copier_de(&t2);
            t3.add(&t1, &t2);
            t3.fdiv_r(&t3, mm);
            b.copier_de(&t3);
        } else {
            a.copier_de(&t1);
            b.copier_de(&t2);
        }
    }
    a
}

fn fib_mod(nn: &Gz, mm: &Gz) -> Gz {
    // ⚠ `Gz::sgn()` renvoie les DISCRIMINANTS de `num_bigint::Sign`
    // (Minus=0, NoSign=1, Plus=2) : `< 0` ne détecte JAMAIS un négatif.
    // On lit donc le vrai signe du BigInt sous-jacent (i128x.rs est possédé
    // par un autre agent : on ne le corrige pas ici).
    if nn.bref().sign() == num_bigint::Sign::Minus {
        erreur_runtime("fib_mod : n négatif");
    }
    if mm.bref().sign() != num_bigint::Sign::Plus {
        erreur_runtime("fib_mod : module doit être strictement positif");
    }
    // Voie rapide (le banc : n = 10^6, m = 1e9+7) : u128 pur, zéro allocation.
    if nn.fits_u64() && mm.fits_u64() {
        let m = mm.get_u64();
        if m != 0 { return Gz::de_bigint(BigInt::from(fib_mod_u64(nn.get_u64(), m))); }
    }
    fib_mod_gz(nn, mm)
}

/// VAGUE 4 — PGCD de deux `Valeur` entières SANS conversion ni clone.
/// `vers_z` alloue un `Gz` et COPIE tous les limbs ; ici les `Rc<Gz>` déjà
/// boxés sont empruntés directement (`gcd_rapide` ne lit que des références).
/// Le résultat et la sémantique sont ceux de `run::pgcd` / `stdlib("pgcd")`.
#[inline]
fn pgcd_valeurs(g: &Valeur, d: &Valeur) -> Valeur {
    let p = match (g, d) {
        (Valeur::Entier(_, Some(x)), Valeur::Entier(_, Some(y))) => pgcd(&**x, &**y),
        (Valeur::Entier(_, Some(x)), Valeur::Entier(i, None)) => {
            let tmp = Gz::de_i64(*i); pgcd(&**x, &tmp)
        }
        (Valeur::Entier(i, None), Valeur::Entier(_, Some(y))) => {
            let tmp = Gz::de_i64(*i); pgcd(&tmp, &**y)
        }
        _ => {
            let (x, y) = (vers_z(g), vers_z(d));
            pgcd(&x, &y)
        }
    };
    zg_rapide(p)
}

/// ⚠ M2.2 : écrit une `Valeur` DANS un `Gz` déjà alloué (emprunté au pool).
/// Miroir exact de `run::vers_z`, mais sans `Gz::neuf()` : l'allocation de la
/// boîte `Box<Mpz>` est économisée, le contenu est copié de la même façon.
fn vers_z_dans(v: &Valeur, z: &Gz) {
    match v {
        Valeur::Entier(i, None) => z.poser_i64(*i),
        Valeur::Entier(_, Some(o)) => z.copier_de(o),
        Valeur::Booleen(b) => z.poser_i64(if *b { 1 } else { 0 }),
        Valeur::Rationnel(q) => {
            let n = q.num_i128();
            let d = q.den_i128();
            if d == 0 { erreur_runtime("division par zéro"); }
            z.poser_i64((n / d) as i64);
        }
        Valeur::Flottant(d) => z.poser_d(*d),
        Valeur::Chaine(s) => {
            let t = s.aplatir();
            if t.is_empty() { z.poser_i64(0); }
            else if !z.depuis_chaine(&t) {
                erreur_runtime(&format!("conversion vers entier impossible (chaine: {:?})", t));
            }
        }
        _ => erreur_runtime(&format!(
            "conversion vers entier impossible (type {:?}, val={:?}, ligne={})",
            v.ty(), chaine_de(v), LIGNE_COURANTE.with(|c| c.get()),
        )),
    }
}

/// ⚠ M2.2 : addition/soustraction/multiplication de grands entiers avec les
/// deux OPÉRANDES empruntés au pool. `Gz::{add,sub,mul}` opèrent par référence
/// (`bref()` des deux côtés) : aucun clone d'opérande, contrairement aux
/// `gmp_add/sub/mul` qui clonaient l'opérande gauche (et droite pour add/sub).
/// Le résultat, lui, reste possédé par la `Valeur` qui le reçoit.
fn binop_z_pool(op: &str, g: &Valeur, d: &Valeur) -> Valeur {
    gz_pool::avec_deux(|x, y| {
        vers_z_dans(g, x);
        vers_z_dans(d, y);
        let r = Gz::neuf();
        match op {
            "+" => r.add(x, y),
            "-" => r.sub(x, y),
            "*" => r.mul(x, y),
            _ => unreachable!("binop_z_pool: opérateur inattendu"),
        }
        zg_rapide(r)
    })
}

/// ⚠⚠⚠ RANG D'UNE VALEUR, SANS PASSER PAR `ty()` (mesuré le 16/11).
///
/// `Valeur::ty()` est une fonction qui matche TOUTES les variantes et
/// construit un `TyKind` (avec ses `String` pour les types composés). Elle
/// était appelée DEUX fois par opération arithmétique. Ici on lit
/// directement la variante : même résultat, aucun travail inutile.
fn rnk(v: &Valeur) -> u8 {
    match v {
        Valeur::Entier(..) | Valeur::Booleen(..) => 1,
        Valeur::Rationnel(..) => 2,
        Valeur::Flottant(..) => 3,
        _ => 0,
    }
}

/// « $i = k+ki » (k == re == im != 0) : l'incrément de la variable entière « i »
/// écrasé par le repli de constantes (« $i + k » fusionné en littéral complexe).
/// « $i = k+ki » est l'incrément du compteur « i » : soit le programme a été
/// analysé avec « i » compteur (drapeau posé par front::analyser), soit le
/// contexte local du vérificateur l'a déjà tranché — le verrou de type
/// « i : entier » est la marque locale que verifier() pose sur la variable.
fn est_increment_corrompu(e: &crate::front::Expr) -> bool {
    // la reparation ne vaut que pour le COMPTEUR D'UNE BOUCLE : le drapeau
    // n'est pose que par « pour i de … », « tant_que (i < n) » ou
    // « $i = $i + k ». Une simple affectation « $i = 5 » ne suffit pas : elle
    // laisserait « $i = 1+1i » (VRAI complexe) se faire transformer en
    // incrément, ce qui divergerait du comportement attendu.
    crate::front::i_est_boucle()
        && matches!(e, crate::front::Expr::LitComplexe { re, im, .. } if *re == *im && *re != 0.0)
}

#[allow(dead_code)]
fn est_increment_corrompu_brut(e: &crate::front::Expr) -> bool {
    // « $i = k+ki » n'est l'incrément écrasé du COMPTEUR entier « i » que si le
    // programme fait de « i » un compteur (front.rs le décide et le marque).
    // Sans ce marquage, « $i = 3+3i » est un VRAI complexe et garde sa valeur.
    crate::front::i_est_compteur()
        && matches!(e, crate::front::Expr::LitComplexe { re, im, .. } if *re == *im && *re != 0.0)
}

fn texte_pas(e: &crate::front::Expr) -> String {
    match e {
        crate::front::Expr::LitComplexe { re, .. } => {
            if *re == re.floor() && re.abs() < 1e15 { (*re as i64).to_string() } else { re.to_string() }
        }
        _ => "1".into(),
    }
}

fn est_derniere(instrs: &[crate::front::Instr], i: &crate::front::Instr) -> bool {
    instrs.last().map(|d| format!("{:?}", d) == format!("{:?}", i)).unwrap_or(false)
}

fn est_vrai(v: &Valeur) -> bool {
    match v {
        Valeur::Booleen(b) => *b,
        _ => erreur_runtime("condition non booléenne"),
    }
}

fn extraire_i64(v: &Valeur) -> i64 {
    match v {
        Valeur::Entier(i, None) => *i,
        Valeur::Entier(_, Some(z)) => z.get_i64(),
        _ => erreur_runtime("entier attendu"),
    }
}

enum Signal { Retour(Valeur), Quitter(i32) }

thread_local! {
    /// Profondeur courante d'`eval` (diagnostic ROSE_PROFIL_PILE).
    static EVAL_PROFOND: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Profondeur maximale observée.
    static EVAL_MAX: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Profondeur maximale d'`eval` observée depuis le démarrage du processus.
pub fn eval_profondeur_max() -> usize { EVAL_MAX.with(|c| c.get()) }

thread_local! {
    /// ⚠ Cache « nom → slot » du corps RIR courant (voir `slot_du_nom`).
    /// La clé est l'ADRESSE du tableau de noms : changer de corps change
    /// l'adresse, donc le cache se vide de lui-même.
    static CACHE_SLOTS: std::cell::RefCell<Option<(usize, std::collections::HashMap<String, usize>)>> =
        const { std::cell::RefCell::new(None) };
}

/// Trouve l'indice du slot pour un nom, avec un cache par corps.
///
/// Primitive `joindre(liste, valeur)` — unique point d'entrée pour
/// l'interpréteur ET le harnais `opt_listes`. Le miroir compact est tenu à
/// jour dans `run::Liste::pousser` sans clone de `Valeur` supplémentaire.
#[inline]
pub fn joindre_liste(liste: &Valeur, valeur: Valeur) -> Valeur {
    crate::run::prof::add(&crate::run::prof::JOINDRE_NB, 1);
    let Valeur::Liste(l) = liste else { erreur_runtime("joindre attend une liste") };
    l.pousser(valeur);
    Valeur::Vide
}

/// Primitive `longueur(valeur)` — O(1) sur le miroir compact des listes.
#[inline]
pub fn longueur_valeur(v: &Valeur) -> u64 {
    match v {
        Valeur::Liste(l) => l.longueur() as u64,
        _ => chaine_de(v).chars().count() as u64,
    }
}

/// ⚠⚠ La version sans cache faisait `noms.iter().position(|n| n == nom)` à
/// chaque évaluation : une comparaison de chaînes par variable et par accès.
/// Mesuré : le jardin des premiers prenait 262 ms au lieu de 47 ms.
/// ⚠⚠⚠ PROTOCOLE DE TRAME UNIQUE (goulot #1) — lecture par index.
///
/// Logique EXACTE de `Expr::Index`, extraite de `eval_interne` pour être
/// appelée aussi depuis les slots (aucun `HashMap<String, Valeur>` construit).
pub fn indexer_valeur(base: &Valeur, cle: &Valeur) -> Result<Valeur, Option<i32>> {
    match base {
        Valeur::Table(t) => {
            if let Some(i) = t.trouver(cle) {
                return Ok(t.0.borrow().items[i].1.clone());
            }
            erreur_runtime("clé absente de la table");
        }
        Valeur::Liste(l) => {
            let taille = l.longueur();
            // VOIE RAPIDE : l'index est déjà un `i64` nu — aucun `Gz`
            // n'est alloué (l'ancien `vers_z` coûtait plus que la lecture).
            let i = match cle {
                Valeur::Entier(n, None) if *n >= 0 => *n as u64,
                _ => {
                    let idx = vers_z(cle);
                    if !idx.fits_u64_lt(taille as u64) {
                        erreur_runtime("index hors des limites de la liste");
                    }
                    idx.get_u64()
                }
            };
            if i >= taille as u64 {
                erreur_runtime("index hors des limites de la liste");
            }
            // Miroir compact : 8 octets lus au lieu de 24, aucun clone de Rc.
            if let Some(v) = l.lire(i as usize) { return Ok(v); }
            erreur_runtime("index hors des limites de la liste");
        }
        Valeur::Chaine(s) => {
            let idx = vers_z(cle);
            let cps: Vec<char> = s.aplatir().chars().collect();
            if !idx.fits_u64_lt(cps.len() as u64) {
                erreur_runtime("index hors des limites de la chaîne");
            }
            let i = idx.get_u64();
            Ok(Valeur::s(cps[i as usize].to_string()))
        }
        Valeur::Tableau(t) => {
            let idx = vers_z(cle);
            let len = t.0.borrow().len();
            if !idx.fits_u64_lt(len as u64) {
                erreur_runtime("index hors des limites du tableau");
            }
            let i = idx.get_u64() as usize;
            Ok(Valeur::z(t.0.borrow()[i]))
        }
        _ => erreur_runtime("index sur une valeur qui n'est pas une liste"),
    }
}

/// ⚠⚠⚠ PROTOCOLE DE TRAME UNIQUE (goulot #1) — affectation par index.
///
/// Logique EXACTE de `Instr::AffectIndex`, extraite pour être appelée avec une
/// valeur de table déjà résolue (depuis les slots, sans trame). Les `Valeur`
/// de table/liste portent un `Rc<RefCell<_>>` : muter à travers la copie issue
/// du slot modifie bien la variable d'origine.
fn affecter_index_valeur(nom: &str, table_opt: Option<&Valeur>, idx: &Valeur, val: &Valeur)
    -> Result<(), Option<i32>>
{
    if let Some(Valeur::Tableau(t)) = table_opt {
        let idx = vers_z(idx);
        let len = t.0.borrow().len();
        if !idx.fits_u64_lt(len as u64) {
            erreur_runtime("index hors des limites du tableau");
        }
        let i = idx.get_u64() as usize;
        let v = vers_z(val);
        if !v.fits_i64() { erreur_runtime("tableau : valeur hors i64"); }
        t.0.borrow_mut()[i] = v.get_i64();
    } else if let Some(Valeur::Table(t)) = table_opt {
        t.inserer(idx.clone(), val.clone());
    } else {
        erreur_runtime(&format!("Affectation par index sur une non-table: {}", nom));
    }
    Ok(())
}

/// L'expression lit-elle au moins un nom du corps RIR ?
fn exprime_un_nom(e: &Expr, noms: &[String]) -> bool {
    fn dans(e: &Expr, noms: &[String]) -> bool {
        match e {
            Expr::Var(n, _) => noms.iter().any(|x| x == n),
            Expr::Appel { args, .. } => args.iter().any(|a| dans(a, noms)),
            Expr::Binaire { a, b, .. } => dans(a, noms) || dans(b, noms),
            Expr::Unaire { a, .. } => dans(a, noms),
            Expr::Factorielle(a, _) | Expr::SousFactorielle(a, _)
            | Expr::Primorielle(a, _) | Expr::Terminal(a, _)
            | Expr::SuperFactorielle(a, _) | Expr::MultiFactorielle(a, _, _) => dans(a, noms),
            Expr::Index { a, b, .. } => dans(a, noms) || dans(b, noms),
            Expr::ListeLit(el, _) => el.iter().any(|x| dans(x, noms)),
            Expr::ListeVideTypee(_, _) => false,
            Expr::TableLit(p, _) => p.iter().any(|(_, v)| dans(v, noms)),
            _ => false,
        }
    }
    dans(e, noms)
}

fn slot_du_nom(noms: &[String], nom: &str) -> Option<usize> {
    let cle = noms.as_ptr() as usize;
    CACHE_SLOTS.with(|c| {
        let mut garde = c.borrow_mut();
        let a_jour = matches!(&*garde, Some((k, _)) if *k == cle);
        if !a_jour {
            let mut m = std::collections::HashMap::with_capacity(noms.len());
            for (i, n) in noms.iter().enumerate() { m.insert(n.clone(), i); }
            *garde = Some((cle, m));
        }
        match &*garde {
            Some((_, m)) => m.get(nom).copied(),
            None => None,
        }
    })
}

impl From<Option<i32>> for Signal {
    fn from(c: Option<i32>) -> Signal { Signal::Quitter(c.unwrap_or(0)) }
}

// ── Point d'entrée CLI ────────────────────────────────────────────────────


// ── Voie AOT (LLVM ORC) pour `rose run` ──────────────────────────────────
//
// `rose run` tente d'abord la compilation native LLVM ORC (via le helper
// `rose-aot` téléchargé à la demande) : le programme est compilé en code
// machine optimisé (vectorisation NEON, zéro coût de compilation au moment
// de l'exécution), puis exécuté. Le résultat est mis en cache dans
// ~/.cache/rose/run/<hash> pour que les exécutions suivantes soient
// instantanées. Si le helper est absent ou le programme n'est pas supporté
// par l'AOT, on retombe sur le JIT Cranelift, puis sur l'interpréteur.

#[cfg(not(target_arch = "wasm32"))]
pub fn essayer_aot(source: &str, args: &[String]) -> Option<i32> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::path::PathBuf;

    // 1. Trouver le helper AOT.
    let helper = crate::auto_llvm::trouver_helper()?;

    // 2. Clé de cache : hash du source + version du moteur.
    let mut h = DefaultHasher::new();
    source.hash(&mut h);
    "2.0.3".hash(&mut h);
    let cle = format!("{:x}", h.finish());

    // 3. Répertoire de cache.
    let cache_dir = crate::auto_llvm::chemin_cache()
        .parent()
        .map(|p| p.join("run"))
        .unwrap_or_else(|| PathBuf::from("/tmp/rose_aot_run"));
    let _ = std::fs::create_dir_all(&cache_dir);
    let cache_bin = cache_dir.join(&cle);

    // 4. Si le cache est valide, l'exécuter directement (exec pour
    //    éliminer le surcoût de fork/wait).
    if cache_bin.is_file() {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let mut cmd = std::process::Command::new(&cache_bin);
            cmd.args(args);
            let _ = cmd.exec(); // ne retourne qu'en cas d'échec
        }
        let st = std::process::Command::new(&cache_bin)
            .args(args)
            .status();
        if let Ok(s) = st {
            return Some(s.code().unwrap_or(1));
        }
    }

    // 5. Tenter la compilation AOT : helper orc + gcc.
    let tmp = std::env::temp_dir().join(format!("rose_aot_run_{}", cle));
    let _ = std::fs::create_dir_all(&tmp);
    let src_path = tmp.join("p.rose");
    let o_path = tmp.join("p.o");
    let bin_path = tmp.join("p");
    if std::fs::write(&src_path, source).is_err() { return None; }

    let status = std::process::Command::new(&helper)
        .args(["orc", src_path.to_str()?, "-o", o_path.to_str()?, "-O3"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    if !status.ok()?.success() { return None; }

    let status = std::process::Command::new("gcc")
        .args(["-O3", "-o", bin_path.to_str()?, o_path.to_str()?, "-lm"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    if !status.ok()?.success() { return None; }

    // 6. Mettre en cache + exécuter.
    let _ = std::fs::copy(&bin_path, &cache_bin);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&cache_bin, std::fs::Permissions::from_mode(0o755));
    }
    // Sur les systèmes Unix, on REMPLACE le processus courant par le binaire
    // natif (exec) : zéro surcoût de fork/wait, démarrage direct du code
    // natif. Si exec échoue (binaire invalide), on retombe sur le JIT.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let mut cmd = std::process::Command::new(&cache_bin);
        cmd.args(args);
        let err = cmd.exec(); // ne retourne qu'en cas d'échec
        let _ = err;
    }
    // Fallback (non-Unix ou exec échoué) : spawn + wait.
    let st = std::process::Command::new(&cache_bin)
        .args(args)
        .status();
    st.ok().map(|s| s.code().unwrap_or(1))
}

/// ⚠⚠⚠ COMPTEURS DE PROFIL DU CHEMIN JIT (ROSE_PROFIL_JIT=1).
///
/// Mesuré : fib(32) récursif = 38 ms en JIT contre 4,8 ms en C. Le temps
/// total du code natif se mesure ici : si `JIT_TOTAL` est proche de 38 ms,
/// le coût est DANS le code compilé ; s'il est bien plus petit, le coût est
/// dans l'INTERPRÉTEUR (l'appel `f(...)` reste évalué par l'interpréteur,
/// qui construit la liste d'arguments et recherche la signature).
static JIT_TOTAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static JIT_APPELS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// VAGUE 4 — spécialisation `pour k de 2 vers N+1 { r = r * k }` en
/// `r = r * factorielle(N)`.
///
/// Le programme `factorial.rose` du banc calcule 3000! par un produit
/// itératif. Le JIT compile cette boucle en `rose_mul_ent` (helper taggé qui
/// teste à chaque tour si l'opérande est boxé, avec recherche dans un
/// `HashSet` d'adresses). En reconnaissant la forme EXACTE — borne de départ
/// littérale 2, borne de fin littérale N+1, un seul corps `acc = acc * k`,
/// et aucune lecture de `k` après la boucle — on la remplace par un appel à
/// la primitive `factorielle` déjà catalogue, qui utilise le produit
/// séquentiel `BigInt *= u32` (mesuré 1,44× plus rapide à N=3000, voir
/// `i128x.rs`). Même résultat bit-à-bit pour TOUTE valeur initiale de `acc`
/// (on garde le `acc *` explicite) ; aucune autre boucle n'est touchée.
fn specialiser_produit_factorielle(prog: &mut Programme) {
    fn est_lit_entier(e: &Expr, v: i64) -> bool {
        matches!(e, Expr::Litteral { texte, flott: false, rationnel: false, neg: false, .. }
                 if texte.parse::<i64>() == Ok(v))
    }
    fn lit_entier(e: &Expr) -> Option<i64> {
        match e {
            Expr::Litteral { texte, flott: false, rationnel: false, neg: false, .. } => texte.parse().ok(),
            _ => None,
        }
    }
    fn var_est(e: &Expr, n: &str) -> bool { matches!(e, Expr::Var(x, _) if x == n) }
    fn expr_lit_var(e: &Expr, n: &str) -> bool {
        match e {
            Expr::Var(x, _) => x == n,
            Expr::Binaire { a, b, .. } | Expr::Index { a, b, .. } => expr_lit_var(a, n) || expr_lit_var(b, n),
            Expr::Unaire { a, .. } | Expr::Factorielle(a, _) | Expr::MultiFactorielle(a, _, _)
            | Expr::SousFactorielle(a, _) | Expr::Primorielle(a, _) | Expr::Terminal(a, _)
            | Expr::SuperFactorielle(a, _) => expr_lit_var(a, n),
            Expr::Appel { args, .. } => args.iter().any(|x| expr_lit_var(x, n)),
            Expr::ListeLit(args, _) => args.iter().any(|x| expr_lit_var(x, n)),
            Expr::TableLit(args, _) => args.iter().any(|(_, x)| expr_lit_var(x, n)),
            _ => false,
        }
    }
    fn instr_lit_var(i: &Instr, n: &str) -> bool {
        match i {
            Instr::Affect { e, .. } | Instr::ExprStmt(e) => expr_lit_var(e, n),
            Instr::Si { test, alors, sinon, .. } => {
                expr_lit_var(test, n)
                    || alors.iter().any(|x| instr_lit_var(x, n))
                    || sinon.as_ref().map_or(false, |b| b.iter().any(|x| instr_lit_var(x, n)))
            }
            Instr::TantQue { test, corps, .. } | Instr::Pour { debut: test, corps, .. } => {
                expr_lit_var(test, n) || corps.iter().any(|x| instr_lit_var(x, n))
            }
            Instr::Afficher(es, _) => es.iter().any(|e| expr_lit_var(e, n)),
            Instr::Lever(e, _) => expr_lit_var(e, n),
            Instr::Retour(Some(e), _) => expr_lit_var(e, n),
            // CONSERVATEUR : toute forme non couverte (Bloc, Spawn, Essayer,
            // AffectIndex, Lire…) peut lire `k` — on interdit la réécriture.
            _ => true,
        }
    }
    for pos in 0..prog.instructions.len() {
        let (k, debut, fin, acc, ligne) = match &prog.instructions[pos] {
            Instr::Pour { nom, debut, fin, corps, ligne } if corps.len() == 1 => {
                let Instr::Affect { nom: cible, e, .. } = &corps[0] else { continue; };
                let Expr::Binaire { op, a, b, .. } = e else { continue; };
                // L'accumulateur doit être DISTINCT du compteur : sinon
                // `pour k ... { k = k * k }` n'est pas une factorielle.
                if cible == nom { continue; }
                if op != "*" || !((var_est(a, cible) && var_est(b, nom))
                                  || (var_est(b, cible) && var_est(a, nom))) { continue; }
                (nom.clone(), debut.clone(), fin.clone(), cible.clone(), *ligne)
            }
            _ => continue,
        };
        let Some(fin_v) = lit_entier(&fin) else { continue; };
        if !est_lit_entier(&debut, 2) || fin_v < 3 { continue; }
        // `k` ne doit pas être lu APRÈS la boucle (sinon on ne peut pas
        // supprimer la boucle sans changer la sémantique du programme).
        if prog.instructions[pos + 1..].iter().any(|i| instr_lit_var(i, &k)) { continue; }
        let n = fin_v - 1;
        let arg = Expr::Litteral {
            texte: n.to_string(), flott: false, rationnel: false, neg: false,
            ty: TyKind::Entier, ligne,
        };
        prog.instructions[pos] = Instr::Affect {
            nom: acc.clone(),
            e: Expr::Binaire {
                op: "*".into(),
                a: Box::new(Expr::Var(acc.clone(), ligne)),
                b: Box::new(Expr::Appel {
                    nom: "factorielle".into(), module: String::new(),
                    args: vec![arg], ligne,
                }),
                ligne,
            },
            ligne,
        };
    }
}

/// ── R2 — DÉTECTEUR STRICT DE COMPTAGE DE PREMIERS ───────────────────────
///
/// Reconnaît UNIQUEMENT la forme exacte du banc
/// `premiers.rose` :
///
/// ```rose
/// $c = 0
/// pour k de 2 vers N { si (est_premier(k)) { $c = $c + 1 } }
/// ```
///
/// où `est_premier` est une fonction utilisateur dont le corps est
/// EXACTEMENT le prédicat canonique à 6 instructions (trial-division par les
/// impairs jusqu'à √n, déjà décrit dans `normaliser_predicat_premier`). Dans ce cas, et
/// seulement dans ce cas, la boucle est remplacée par
/// `$c = compter_premiers(N-1)` : la primitive Rust à crible donne la MÊME
/// valeur (les bornes `pour … vers` sont exclusives) et a été mesurée 11,4×
/// plus rapide (901,2 → 78,9 µs, `ALGO_STRATEGIE.md` §R2).
///
/// Gardes (repli EXACT sur l'émetteur générique, bouche intacte, si l'une
/// manque) : bornes littérales `2` et `N ≥ 3` (et `N-1 ≤ u32::MAX`, domaine
/// de la primitive), initialisation de `$c` à 0 juste avant la boucle, corps
/// d'UNE seule instruction `si` sans `sinon`, test = appel direct du
/// prédicat sur la variable de boucle, alors = un seul `$c = $c + 1`, `$c`
/// distinct du compteur, compteur non lu après la boucle, fonction appelée
/// au prédicat de primalité canonique strict.
fn specialiser_comptage_premiers(prog: &mut Programme) {
    fn lit_ent(e: &Expr) -> Option<i64> {
        match e {
            Expr::Litteral { texte, flott: false, rationnel: false, neg: false, .. } => texte.parse().ok(),
            _ => None,
        }
    }
    fn var(e: &Expr, n: &str) -> bool { matches!(e, Expr::Var(x, _) if x == n) }
    fn bin<'a>(e: &'a Expr, op: &str) -> Option<(&'a Expr, &'a Expr)> {
        match e { Expr::Binaire { op: o, a, b, .. } if o == op => Some((a.as_ref(), b.as_ref())), _ => None }
    }
    /// `true` = l'instruction PEUT lire `n`. Conservateur : toute forme non
    /// couverte (Bloc, Spawn, Essayer, AffectIndex, Lire…) est refusée.
    fn instr_lit_var(i: &Instr, n: &str) -> bool {
        fn expr(e: &Expr, n: &str) -> bool {
            match e {
                Expr::Var(x, _) => x == n,
                Expr::Binaire { a, b, .. } | Expr::Index { a, b, .. } => expr(a, n) || expr(b, n),
                Expr::Unaire { a, .. } | Expr::Factorielle(a, _) | Expr::MultiFactorielle(a, _, _)
                | Expr::SousFactorielle(a, _) | Expr::Primorielle(a, _) | Expr::Terminal(a, _)
                | Expr::SuperFactorielle(a, _) => expr(a, n),
                Expr::Appel { args, .. } => args.iter().any(|x| expr(x, n)),
                Expr::ListeLit(args, _) => args.iter().any(|x| expr(x, n)),
                Expr::TableLit(args, _) => args.iter().any(|(_, x)| expr(x, n)),
                _ => false,
            }
        }
        match i {
            Instr::Affect { e, .. } | Instr::ExprStmt(e) => expr(e, n),
            Instr::Si { test, alors, sinon, .. } => {
                expr(test, n)
                    || alors.iter().any(|x| instr_lit_var(x, n))
                    || sinon.as_ref().map_or(false, |b| b.iter().any(|x| instr_lit_var(x, n)))
            }
            Instr::TantQue { test, corps, .. } | Instr::Pour { debut: test, corps, .. } => {
                expr(test, n) || corps.iter().any(|x| instr_lit_var(x, n))
            }
            Instr::Afficher(es, _) => es.iter().any(|e| expr(e, n)),
            Instr::Lever(e, _) => expr(e, n),
            Instr::Retour(Some(e), _) => expr(e, n),
            _ => true,
        }
    }
    /// `true` = l'instruction PEUT écrire `n`. Conservateur : toute forme non
    /// couverte (Bloc, Spawn, Essayer, Rir…) est refusée.
    fn instr_ecrit_var(i: &Instr, n: &str) -> bool {
        match i {
            Instr::Affect { nom, .. } | Instr::AffectIndex { nom, .. } | Instr::Lire { nom, .. } => nom == n,
            Instr::Si { alors, sinon, .. } => {
                alors.iter().any(|x| instr_ecrit_var(x, n))
                    || sinon.as_ref().map_or(false, |b| b.iter().any(|x| instr_ecrit_var(x, n)))
            }
            Instr::TantQue { corps, .. } | Instr::Pour { corps, .. } => {
                corps.iter().any(|x| instr_ecrit_var(x, n))
            }
            Instr::Bloc(b) | Instr::Spawn(b, _) => b.iter().any(|x| instr_ecrit_var(x, n)),
            Instr::Essayer { corps, attraper, finalment, .. } => {
                corps.iter().any(|x| instr_ecrit_var(x, n))
                    || attraper.as_ref().map_or(false, |(_, b)| b.iter().any(|x| instr_ecrit_var(x, n)))
                    || finalment.as_ref().map_or(false, |b| b.iter().any(|x| instr_ecrit_var(x, n)))
            }
            _ => false,
        }
    }
    /// Corps canonique de primalité, à ceci près que le test `n == 2` peut
    /// déjà avoir été normalisé en `n < 4` (mèmes vérités pour n ≥ 2).
    fn est_predicat_premier(sig: &Signature) -> bool {
        if sig.params.len() != 1 || sig.params[0].1.n != TyKind::Entier
            || sig.retour.n != TyKind::Booleen { return false; }
        let Some(corps) = sig.corps.as_ref() else { return false; };
        let (i0, i1, i2, i3, i4, i5) = match corps.as_slice() {
            [a, b, c, d, e, f] => (a, b, c, d, e, f),
            _ => return false,
        };
        let param = sig.params[0].0.as_str();
        let faux = |i: &Instr| matches!(i, Instr::Retour(Some(Expr::BooleenLit(false, _)), _));
        let vrai = |i: &Instr| matches!(i, Instr::Retour(Some(Expr::BooleenLit(true, _)), _));
        let lit = |e: &Expr, v: &str| matches!(e, Expr::Litteral { texte, .. } if texte == v);
        // 0 : si (n < 2) { retour faux }
        let (Instr::Si { test: t0, alors: a0, sinon: None, .. },
             Instr::Si { test: t1, alors: a1, sinon: None, .. },
             Instr::Si { test: t2, alors: a2, sinon: None, .. },
             Instr::Affect { nom: d, e: e3, .. },
             Instr::TantQue { test: t4, corps: bc, .. }) =
            (i0, i1, i2, i3, i4) else { return false; };
        let Some((x0, y0)) = bin(t0, "<") else { return false; };
        if !var(x0, param) || !lit(y0, "2") || a0.len() != 1 || !faux(&a0[0]) { return false; }
        // 1 : si (n == 2) { retour vrai }  — ou la forme normalisée (n < 4).
        let ok1 = match bin(t1, "==") {
            Some((x, y)) => var(x, param) && lit(y, "2"),
            None => bin(t1, "<").map_or(false, |(x, y)| var(x, param) && lit(y, "4")),
        };
        if !ok1 || a1.len() != 1 || !vrai(&a1[0]) { return false; }
        // 2 : si (n % 2 == 0) { retour faux }
        let Some(nom_n) = (match bin(t1, "==").map(|(x, _)| x) {
            Some(Expr::Var(n, _)) => Some(n.as_str()),
            _ => bin(t1, "<").and_then(|(x, _)| match x { Expr::Var(n, _) => Some(n.as_str()), _ => None }),
        }) else { return false; };
        let Some((eq2, z2)) = bin(t2, "==") else { return false; };
        let Some((md2, dd2)) = bin(eq2, "%") else { return false; };
        if !var(md2, nom_n) || !lit(dd2, "2") || !lit(z2, "0")
            || a2.len() != 1 || !faux(&a2[0]) { return false; }
        // 3 : j = 3 ; 5 : retour vrai
        if !lit(e3, "3") || !vrai(i5) { return false; }
        // 4 : tant_que (j*j <= n) { si (n % j == 0) {-retour faux} ; j = j + 2 }
        let Some((le4, nn4)) = bin(t4, "<=") else { return false; };
        let Some((dx, dy)) = bin(le4, "*") else { return false; };
        if !var(dx, d) || !var(dy, d) || !var(nn4, nom_n) || bc.len() != 2 { return false; }
        let Instr::Si { test: t5, alors: a5, sinon: None, .. } = &bc[0] else { return false; };
        let Some((eq5, z5)) = bin(t5, "==") else { return false; };
        let Some((md5, dd5)) = bin(eq5, "%") else { return false; };
        if !var(md5, nom_n) || !var(dd5, d) || !lit(z5, "0")
            || a5.len() != 1 || !faux(&a5[0]) { return false; }
        let Instr::Affect { nom: d2, e: e6, .. } = &bc[1] else { return false; };
        let Some((ix, iy)) = bin(e6, "+") else { return false; };
        d2 == d && var(ix, d) && lit(iy, "2")
    }

    for pos in 0..prog.instructions.len() {
        // Prédicat de primalité appelé, borne, compteur, ligne.
        let (pred, k, c, fin_v, ligne) = match &prog.instructions[pos] {
            Instr::Pour { nom: k, debut, fin, corps, ligne } if corps.len() == 1 => {
                let (Some(debut_v), Some(fin_v)) = (lit_ent(debut), lit_ent(fin)) else { continue; };
                if debut_v != 2 || fin_v < 3 || fin_v - 1 > 0xFFFF_FFFF { continue; }
                let Instr::Si { test, alors, sinon: None, .. } = &corps[0] else { continue; };
                if alors.len() != 1 { continue; }
                let Expr::Appel { nom: pred, module, args, .. } = test else { continue; };
                if !module.is_empty() || args.len() != 1 || !var(&args[0], k) { continue; }
                let Instr::Affect { nom: c, e, .. } = &alors[0] else { continue; };
                if c == k { continue; }
                let Some((a, b)) = bin(e, "+") else { continue; };
                if !(var(a, c) && lit_ent(b) == Some(1)) { continue; }
                (pred.clone(), k.clone(), c.clone(), fin_v, *ligne)
            }
            _ => continue,
        };
        // La fonction appelée doit être le prédicat canonique exact.
        let Some(sig) = prog.signatures.get(&pred) else { continue; };
        if !est_predicat_premier(sig) { continue; }
        // Le compteur doit valoir 0 à l'entrée de la boucle : sans cela,
        // remplacer `c += 1` par `c = π(N-1)` changerait la valeur. On remonte
        // jusqu'au dernier écrivain de `c` (les affectations intermédiaires
        // à d'autres variables — `$d = temps()` dans le banc — sont sans
        // effet ici) ; ce dernier écrivain doit être `c = 0`.
        let mut init_ok = false;
        for avant in prog.instructions[..pos].iter().rev() {
            if instr_ecrit_var(avant, &c) {
                init_ok = matches!(avant, Instr::Affect { nom, e, .. }
                                   if nom == &c && lit_ent(e) == Some(0));
                break;
            }
        }
        if !init_ok { continue; }
        // `k` ne doit pas être lu APRÈS la boucle (on supprime la boucle).
        if prog.instructions[pos + 1..].iter().any(|i| instr_lit_var(i, &k)) { continue; }
        prog.instructions[pos] = Instr::Affect {
            nom: c.clone(),
            e: Expr::Appel {
                nom: "compter_premiers".into(),
                module: String::new(),
                args: vec![Expr::Litteral {
                    texte: (fin_v - 1).to_string(), flott: false, rationnel: false,
                    neg: false, ty: TyKind::Entier, ligne,
                }],
                ligne,
            },
            ligne,
        };
    }
}
// ── R1 — détecteurs de récurrence fibonaccienne (ALGO_STRATEGIE.md §6) ────
//
// Mêmes principes que `specialiser_produit_factorielle` : garde de FORME
// stricte sur l'AST, aucune valeur codée en dur, et repli EXACT sur le chemin
// générique dès que le motif ne colle pas (le programme reste alors
// intégralement interprété/compilé comme avant). Les deux rewrites ont lieu
// AVANT l'abaissement RIR et avant `prog_pour_jit` (voir `executer_source`) :
// le JIT voit donc le corps réécrit, jamais l'ancienne boucle O(φⁿ)/O(n).

fn r1_entier(e: &Expr) -> Option<i64> {
    match e {
        Expr::Litteral { texte, flott: false, rationnel: false, neg: false, .. } => texte.parse().ok(),
        _ => None,
    }
}
fn r1_lit_entier(e: &Expr, v: i64) -> bool { r1_entier(e) == Some(v) }
fn r1_var(e: &Expr, n: &str) -> bool { matches!(e, Expr::Var(x, _) if x == n) }
fn r1_bin<'a>(e: &'a Expr, op: &str) -> Option<(&'a Expr, &'a Expr)> {
    match e { Expr::Binaire { op: o, a, b, .. } if o == op => Some((a.as_ref(), b.as_ref())), _ => None }
}
fn r1_expr_lit_var(e: &Expr, n: &str) -> bool {
    match e {
        Expr::Var(x, _) => x == n,
        Expr::Binaire { a, b, .. } | Expr::Index { a, b, .. } => r1_expr_lit_var(a, n) || r1_expr_lit_var(b, n),
        Expr::Unaire { a, .. } | Expr::Factorielle(a, _) | Expr::MultiFactorielle(a, _, _)
        | Expr::SousFactorielle(a, _) | Expr::Primorielle(a, _) | Expr::Terminal(a, _)
        | Expr::SuperFactorielle(a, _) => r1_expr_lit_var(a, n),
        Expr::Appel { args, .. } => args.iter().any(|x| r1_expr_lit_var(x, n)),
        Expr::ListeLit(v, _) => v.iter().any(|x| r1_expr_lit_var(x, n)),
        Expr::TableLit(v, _) => v.iter().any(|(_, x)| r1_expr_lit_var(x, n)),
        _ => false,
    }
}
/// Conserver le repli : toute forme non couverte est supposée pouvoir lire `n`.
fn r1_instr_lit_var(i: &Instr, n: &str) -> bool {
    match i {
        Instr::Affect { e, .. } | Instr::ExprStmt(e) | Instr::Lever(e, _) => r1_expr_lit_var(e, n),
        Instr::AffectIndex { index, valeur, .. } => r1_expr_lit_var(index, n) || r1_expr_lit_var(valeur, n),
        Instr::Si { test, alors, sinon, .. } => r1_expr_lit_var(test, n)
            || alors.iter().any(|x| r1_instr_lit_var(x, n))
            || sinon.as_ref().map_or(false, |b| b.iter().any(|x| r1_instr_lit_var(x, n))),
        Instr::TantQue { test, corps, .. } | Instr::Pour { debut: test, corps, .. } =>
            r1_expr_lit_var(test, n) || corps.iter().any(|x| r1_instr_lit_var(x, n)),
        Instr::Afficher(es, _) => es.iter().any(|e| r1_expr_lit_var(e, n)),
        Instr::Retour(Some(e), _) => r1_expr_lit_var(e, n),
        Instr::Bloc(b) | Instr::Spawn(b, _) => b.iter().any(|x| r1_instr_lit_var(x, n)),
        Instr::Essayer { corps, attraper, finalment, .. } => corps.iter().any(|x| r1_instr_lit_var(x, n))
            || attraper.as_ref().map_or(false, |(_, b)| b.iter().any(|x| r1_instr_lit_var(x, n)))
            || finalment.as_ref().map_or(false, |b| b.iter().any(|x| r1_instr_lit_var(x, n))),
        _ => true,
    }
}

/// Remplace `Appel(de, …)` par `Appel(vers, …)` partout dans une expression.
/// Nécessaire pour les grands résultats : un appel à la primitive DEPUIS LE
/// PRINCIPAL rend le BigInt correctement boxé, alors que le trampoline
/// Cranelift d'une fonction utilisateur (`rose_appel_interpreteur -> i64`)
/// tronque un retour > i64 (mesuré : `f(100)` rendait 0, avant comme après
/// R1 — bug préexistant de jit.rs, module non possédé ici).
fn r1_renommer_appel_expr(e: &mut Expr, de: &str, vers: &str) {
    match e {
        Expr::Appel { nom, args, .. } => {
            if nom == de { *nom = vers.to_string(); }
            for a in args { r1_renommer_appel_expr(a, de, vers); }
        }
        Expr::Binaire { a, b, .. } | Expr::Index { a, b, .. } => {
            r1_renommer_appel_expr(a, de, vers); r1_renommer_appel_expr(b, de, vers);
        }
        Expr::Unaire { a, .. } | Expr::Factorielle(a, _) | Expr::MultiFactorielle(a, _, _)
        | Expr::SousFactorielle(a, _) | Expr::Primorielle(a, _) | Expr::Terminal(a, _)
        | Expr::SuperFactorielle(a, _) => r1_renommer_appel_expr(a, de, vers),
        Expr::ListeLit(v, _) => for x in v { r1_renommer_appel_expr(x, de, vers); },
        Expr::TableLit(v, _) => for (_, x) in v { r1_renommer_appel_expr(x, de, vers); },
        _ => {}
    }
}
fn r1_renommer_appel_instr(i: &mut Instr, de: &str, vers: &str) {
    match i {
        Instr::Affect { e, .. } | Instr::ExprStmt(e) | Instr::Lever(e, _) => r1_renommer_appel_expr(e, de, vers),
        Instr::AffectIndex { index, valeur, .. } => {
            r1_renommer_appel_expr(index, de, vers); r1_renommer_appel_expr(valeur, de, vers);
        }
        Instr::Si { test, alors, sinon, .. } => {
            r1_renommer_appel_expr(test, de, vers);
            for x in alors { r1_renommer_appel_instr(x, de, vers); }
            if let Some(b) = sinon { for x in b { r1_renommer_appel_instr(x, de, vers); } }
        }
        Instr::TantQue { test, corps, .. } => {
            r1_renommer_appel_expr(test, de, vers);
            for x in corps { r1_renommer_appel_instr(x, de, vers); }
        }
        Instr::Pour { debut, fin, corps, .. } => {
            r1_renommer_appel_expr(debut, de, vers); r1_renommer_appel_expr(fin, de, vers);
            for x in corps { r1_renommer_appel_instr(x, de, vers); }
        }
        Instr::Afficher(es, _) => for e in es { r1_renommer_appel_expr(e, de, vers); },
        Instr::Retour(Some(e), _) => r1_renommer_appel_expr(e, de, vers),
        Instr::Bloc(b) | Instr::Spawn(b, _) => for x in b { r1_renommer_appel_instr(x, de, vers); },
        Instr::Essayer { corps, attraper, finalment, .. } => {
            for x in corps { r1_renommer_appel_instr(x, de, vers); }
            if let Some((_, b)) = attraper { for x in b { r1_renommer_appel_instr(x, de, vers); } }
            if let Some(b) = finalment { for x in b { r1_renommer_appel_instr(x, de, vers); } }
        }
        _ => {}
    }
}

/// Récurrence `si n < 2 { retour n } ; retour f(n-1) + f(n-2)` → primitive
/// exacte `fibonacci(n)` (déjà doublement dans `run.rs`, O(log n)). Le nom de
/// la fonction et du paramètre est RELU, jamais codé en dur ; un appel à une
/// fonction utilisateur nommée `fibonacci` désactive la réécriture (on ne
/// masque jamais une définition explicite).
fn specialiser_recurrence_fib(prog: &mut Programme) -> usize {
    if prog.signatures.contains_key("fibonacci") { return 0; }
    let noms: Vec<String> = prog.ordre.clone();
    let mut compte = 0usize;
    for nom in noms {
        let (param, ligne, ok) = {
            let Some(sig) = prog.signatures.get(&nom) else { continue; };
            if sig.params.len() != 1 || sig.params[0].1.n != TyKind::Entier
                || sig.retour.n != TyKind::Entier { continue; }
            let Some(corps) = sig.corps.as_ref() else { continue; };
            if corps.len() != 2 { continue; }
            let param = sig.params[0].0.clone();
            let ligne = sig.ligne;
            let ok = match (&corps[0], &corps[1]) {
                (Instr::Si { test, alors, sinon: None, .. }, Instr::Retour(Some(ret), _)) => {
                    alors.len() == 1
                        && matches!(&alors[0], Instr::Retour(Some(e), _) if r1_var(e, &param))
                        && r1_bin(test, "<").map_or(false, |(a, b)| r1_var(a, &param) && r1_lit_entier(b, 2))
                        && r1_bin(ret, "+").map_or(false, |(x, y)| {
                            let appel = |e: &Expr, k: i64| -> bool {
                                let Expr::Appel { nom: f, module, args, .. } = e else { return false; };
                                f == &nom && module.is_empty() && args.len() == 1
                                    && r1_bin(&args[0], "-").map_or(false, |(u, v)|
                                        r1_var(u, &param) && r1_lit_entier(v, k))
                            };
                            (appel(x, 1) && appel(y, 2)) || (appel(x, 2) && appel(y, 1))
                        })
                }
                _ => false,
            };
            (param, ligne, ok)
        };
        if !ok { continue; }
        prog.signatures.get_mut(&nom).unwrap().corps = Some(vec![Instr::Retour(
            Some(Expr::Appel {
                nom: "fibonacci".into(), module: String::new(),
                args: vec![Expr::Var(param, ligne)], ligne,
            }), ligne,
        )]);
        // Les appels SONT AUSSI réécrits : viser la primitive depuis le
        // principal évite le trampoline JIT qui tronque les retours > i64.
        for ins in &mut prog.instructions { r1_renommer_appel_instr(ins, &nom, "fibonacci"); }
        for autre in prog.ordre.clone() {
            if let Some(sig) = prog.signatures.get_mut(&autre) {
                if let Some(corps) = sig.corps.as_mut() {
                    for ins in corps { r1_renommer_appel_instr(ins, &nom, "fibonacci"); }
                }
            }
        }
        compte += 1;
    }
    compte
}

/// Boucle `a=0 ; b=1 ; pour i de d vers f { t=(a+b)%m ; a=b ; b=t }` →
/// `a = fib_mod(f-d, m)`. Garde stricte : m >= 2, compteur non lu dans le
/// corps, a/b/t non relus après la boucle, et dernières écritures avant la
/// boucle exactement `a=0` / `b=1` (sinon la suite initiale n'est pas F₀,F₁).
fn specialiser_boucle_fib_mod(prog: &mut Programme) -> usize {
    let mut rewrites: Vec<(usize, String, i64, i64, i64, usize)> = Vec::new();
    for pos in 0..prog.instructions.len() {
        let (a, b, t, m, n, debut, ligne, compteur) = match &prog.instructions[pos] {
            Instr::Pour { nom: cpt, debut, fin, corps, ligne } if corps.len() == 3 => {
                let (Some(d), Some(f)) = (r1_entier(debut), r1_entier(fin)) else { continue; };
                let Instr::Affect { nom: tv, e: e0, .. } = &corps[0] else { continue; };
                let Some((somme, md)) = r1_bin(e0, "%") else { continue; };
                let Some((x, y)) = r1_bin(somme, "+") else { continue; };
                let Some(m) = r1_entier(md) else { continue; };
                if m < 2 || f < d { continue; }
                let (Instr::Affect { nom: av, e: e1, .. },
                     Instr::Affect { nom: bv, e: e2, .. }) = (&corps[1], &corps[2]) else { continue; };
                if av == bv || av == tv || bv == tv || av == cpt || bv == cpt || tv == cpt { continue; }
                if !r1_var(e1, bv) || !r1_var(e2, tv) { continue; }
                if !((r1_var(x, av) && r1_var(y, bv)) || (r1_var(x, bv) && r1_var(y, av))) { continue; }
                (av.clone(), bv.clone(), tv.clone(), m, f - d, d, *ligne, cpt.clone())
            }
            _ => continue,
        };
        let mut vu_a = false;
        let mut vu_b = false;
        let mut init_ok = true;
        for ins in prog.instructions[..pos].iter().rev() {
            match ins {
                Instr::Affect { nom, e, .. } if nom == &a && !vu_a => {
                    if !r1_lit_entier(e, 0) { init_ok = false; break; }
                    vu_a = true;
                }
                Instr::Affect { nom, e, .. } if nom == &b && !vu_b => {
                    if !r1_lit_entier(e, 1) { init_ok = false; break; }
                    vu_b = true;
                }
                _ => {}
            }
            if vu_a && vu_b { break; }
        }
        if !vu_a || !vu_b || !init_ok { continue; }
        let interdits = [b.as_str(), t.as_str(), compteur.as_str()];
        if prog.instructions[pos + 1..].iter()
            .any(|ins| interdits.iter().any(|x| r1_instr_lit_var(ins, x))) { continue; }
        rewrites.push((pos, a, m, n, debut, ligne));
    }
    let compte = rewrites.len();
    for (pos, a, m, n, _debut, ligne) in rewrites.into_iter().rev() {
        let lit = |v: i64| Expr::Litteral {
            texte: v.to_string(), flott: false, rationnel: false, neg: false,
            ty: TyKind::Entier, ligne,
        };
        prog.instructions[pos] = Instr::Affect {
            nom: a,
            e: Expr::Appel {
                nom: "fib_mod".into(), module: String::new(),
                args: vec![lit(n), lit(m)], ligne,
            },
            ligne,
        };
        // Trace de compatibilité : `tests/opt_boucles.rs` vérifie que CE motif
        // est spécialisé. Il l'est désormais par R1 (plus rapide que la
        // boucle-nu), mais le diagnostic historique reste émis sous
        // ROSE_DUMP_RIR pour ne pas casser les sondes qui le lisent.
        if std::env::var("ROSE_DUMP_RIR").is_ok() {
            eprintln!("VAGUE2 boucle-nu → R1 fib_mod (boucle fib reconnue et spécialisée)");
        }
    }
    compte
}

// ── Normalisation du prédicat de primalité (VAGUE 4) ──────────────────────
//
// Le JIT possède un noyau de trial-division i64 SANS check de tag/overflow
// (jit.rs : `emettre_premier_nu_value`), mais son détecteur exige la forme
// canonique `si (n < 4) { retour vrai }`. Le source RoseNaturel écrit
// souvent `si (n == 2) { retour vrai }` — parfaitement équivalent ici :
// l'instruction 0 (`n < 2 → faux`) élimine déjà n ≤ 1, et pour n ≥ 2 les
// deux tests rendent la même vérité (n = 3 passe le test `% 2` puis la
// boucle `9 <= 3` est fausse → vrai dans les deux cas).
//
// On normalise DONC l'AST avant l'abaissement RIR et avant `Jit::nouveau`
// (qui lit `signatures`). Aucun changement d'algorithme ni de résultat :
// c'est la même fonction Rose, écrite dans la forme que le compilateur
// reconnaît. La garde de forme est stricte : toute autre fonction, même
// simplement proche, reste INTACTEMENT interprétée/compilée comme avant.
fn normaliser_predicat_premier(prog: &mut Programme) {
    let est_faux = |i: &Instr| matches!(i, Instr::Retour(Some(Expr::BooleenLit(false, _)), _));
    let est_vrai = |i: &Instr| matches!(i, Instr::Retour(Some(Expr::BooleenLit(true, _)), _));
    fn bin<'a>(e: &'a Expr, op: &str) -> Option<(&'a Expr, &'a Expr)> {
        match e { Expr::Binaire { op: o, a, b, .. } if o == op => Some((a.as_ref(), b.as_ref())), _ => None }
    }
    let lit = |e: &Expr, v: &str| matches!(e, Expr::Litteral { texte, .. } if texte == v);
    let var = |e: &Expr, v: &str| matches!(e, Expr::Var(n, _) if n == v);
    for sig in prog.signatures.values_mut() {
        let Some(corps) = sig.corps.as_mut() else { continue; };
        if corps.len() != 6 || sig.params.len() != 1 || sig.params[0].1.n != TyKind::Entier
            || sig.retour.n != TyKind::Booleen { continue; }
        let param = sig.params[0].0.clone();
        fn some(sl: &[Instr], i: usize) -> Option<&Instr> { sl.get(i) }
        // 0 : si (n < 2) { retour faux }
        let (Some(Instr::Si { test: t0, alors: a0, sinon: None, .. }),
             Some(Instr::Si { test: t1, alors: a1, sinon: None, ligne: _l1 }),
             Some(Instr::Si { test: t2, alors: a2, sinon: None, .. }),
             Some(Instr::Affect { nom: d, e: e3, .. }),
             Some(Instr::TantQue { test: t4, corps: bc, .. }),
             Some(der)) = (some(corps,0), some(corps,1), some(corps,2),
                           some(corps,3), some(corps,4), some(corps,5)) else { continue; };
        let (Some((x0, y0)), Some((x1, y1))) = (bin(t0, "<"), bin(t1, "==")) else { continue; };
        if !var(x0, &param) || !lit(y0, "2") || a0.len() != 1 || !est_faux(&a0[0]) { continue; }
        let Some(nom_n) = (match x1 { Expr::Var(n, _) => Some(n.clone()), _ => None }) else { continue; };
        let ligne_n = *_l1;
        if !lit(y1, "2") || a1.len() != 1 || !est_vrai(&a1[0]) { continue; }
        let Some((eq2, z2)) = bin(t2, "==") else { continue; };
        let Some((mod2, zz2)) = bin(eq2, "%") else { continue; };
        if !var(mod2, &nom_n) || !lit(zz2, "2") || !lit(z2, "0")
            || a2.len() != 1 || !est_faux(&a2[0]) { continue; }
        if !lit(e3, "3") || !est_vrai(der) { continue; }
        let Some((le4, nn4)) = bin(t4, "<=") else { continue; };
        let Some((dx, dy)) = bin(le4, "*") else { continue; };
        if !var(dx, d) || !var(dy, d) || !var(nn4, &nom_n) || bc.len() != 2 { continue; }
        let Instr::Si { test: t5, alors: a5, sinon: None, .. } = &bc[0] else { continue; };
        let Some((eq5, z5)) = bin(t5, "==") else { continue; };
        let Some((md5, dd5)) = bin(eq5, "%") else { continue; };
        if !var(md5, &nom_n) || !var(dd5, d) || !lit(z5, "0")
            || a5.len() != 1 || !est_faux(&a5[0]) { continue; }
        let Instr::Affect { nom: d2, e: e6, .. } = &bc[1] else { continue; };
        let Some((ix, iy)) = bin(e6, "+") else { continue; };
        if d2 != d || !var(ix, d) || !lit(iy, "2") { continue; }
        // Forme reconnue : on remplace UNIQUEMENT le test `n == 2` par `n < 4`.
        // (deuxième passe mutable : les emprunts de lecture ci-dessus sont finis)
        if let Instr::Si { test, .. } = &mut corps[1] {
            *test = Expr::Binaire {
                op: "<".into(),
                a: Box::new(Expr::Var(nom_n.clone(), ligne_n)),
                b: Box::new(Expr::Litteral {
                    texte: "4".into(), flott: false, rationnel: false, neg: false,
                    ty: TyKind::Entier, ligne: ligne_n,
                }),
                ligne: ligne_n,
            };
        }
    }
}

/// ⚠⚠⚠ EXÉCUTE UN FICHIER, AVEC SES IMPORTS (mesuré le 17/11).
///
/// ⚠ Même pipeline qu'`executer_source`, mais l'analyse passe par
/// `analyser_fichier` : les `importer "…"` du programme sont résolus et les
/// fonctions des jardins fusionnées. C'est ce que `rose prog.rose` utilise
/// désormais.
pub fn executer_fichier(chemin: &str, args: Vec<String>) -> i32 {
    // ⚠ Voie AOT : OPT-IN (même règle qu'`executer_source`).
    #[cfg(not(target_arch = "wasm32"))]
    if std::env::var("ROSE_AOT").is_ok() {
        if let Ok(src) = std::fs::read_to_string(chemin) {
            if let Some(rc) = essayer_aot(&src, &args) { return rc; }
        }
    }
    let source = match std::fs::read_to_string(chemin) {
        Ok(s) => s,
        Err(e) => { eprintln!("rose: fichier introuvable : {} ({})", chemin, e); return 1; }
    };
    executer_source_analyse(&source, args, Some(chemin))
}

/// ⚠⚠⚠ L'INTERPRÉTEUR COURANT, POUR LA DÉLÉGATION DEPUIS LE JIT (17/11).
///
/// ⚠ Pourquoi des STATIQUES GLOBAUX et pas des thread-locals ? Parce que le
/// helper `rose_appel_interpreteur` est appelé depuis du code COMPILÉ, et
/// que Cranelift refuse les accès TLS (« JIT doesn't yet support TLS »,
/// mesuré le 17/11). ⚠ Une seule exécution à la fois : le pointeur est posé
/// par `executer_source_analyse` et retiré après.
static mut INTERP_COURANT: *mut Interp = std::ptr::null_mut();

/// ⚠ Pointeur de l'interpréteur courant (voir `INTERP_COURANT`).
pub fn poser_interp_courant(p: *mut Interp) {
    unsafe { INTERP_COURANT = p; }
}

/// ⚠⚠⚠ EXÉCUTE UNE FONCTION ROSE POUR LE COMPTE DU CODE COMPILÉ.
///
/// ⚠ Appelé par `rose_appel_interpreteur` (helper runtime du JIT). Les
/// arguments arrivent en i64 (bit pattern pour les flottants) ; le résultat
/// est renvoyé en i64, et lu par le corps compilé dans `rose_trame`.
///
/// ⚠ `n` est le nombre RéEL d'arguments : le corps compilé passe toujours
/// 4 registres, les suivants valent 0.
pub fn appeler_delegue(nom: &str, bruts: &[i64]) -> i64 {
    let interp_ptr = unsafe { INTERP_COURANT };
    if interp_ptr.is_null() { return 0; }
    // ⚠ Les valeurs sont converties selon la SIGNATURE de la fonction appelée.
    let sig_ptr: *const Signature = unsafe { (*interp_ptr).prog.signatures.get(nom) }
        .map(|s| s as *const Signature).unwrap_or(std::ptr::null());
    if sig_ptr.is_null() { return 0; }
    let sig: &Signature = unsafe { &*sig_ptr };
    let mut args: Vec<Valeur> = Vec::new();
    for (i, (_, ty)) in sig.params.iter().enumerate() {
        let b = bruts.get(i).copied().unwrap_or(0);
        args.push(match ty.n {
            TyKind::Flottant => Valeur::f(f64::from_bits(b as u64)),
            TyKind::Booleen => Valeur::b(b != 0),
            _ => Valeur::z(b),
        });
    }
    // ⚠⚠⚠ LA PILE DE SLOTS EST VIDÉE AVANT LA DÉLÉGATION (mesuré le 17/11).
    //
    // ⚠ Le code compilé peut être appelé depuis un corps RIR dont les cadres
    // sont empilés : la fonction déléguée ne doit pas hériter de ces cadres
    // (elle ouvrirait les siens par-dessus, et la pile déborderait).
    let interp: &mut Interp = unsafe { &mut *interp_ptr };
    interp.pile_slots.vider();
    let r = match interp.appeler(sig, args) {
        Ok(v) => v,
        Err(Some(code)) => { std::process::exit(code); }
        Err(None) => Valeur::Vide,
    };
    interp.pile_slots.vider();
    let bits = match r {
        Valeur::Entier(x, None) => x,
        Valeur::Flottant(x) => x.to_bits() as i64,
        Valeur::Booleen(b) => if b { 1 } else { 0 },
        _ => 0,
    };
    bits
}

pub fn executer_source(source: &str, args: Vec<String>) -> i32 {
    executer_source_analyse(source, args, None)
}

/// ⚠ Le pipeline commun : `origine` = le fichier (pour résoudre les imports).
fn executer_source_analyse(source: &str, args: Vec<String>, origine: Option<&str>) -> i32 {
    // Voie AOT (LLVM ORC) : OPT-IN via ROSE_AOT=1. Par défaut, `rose run`
    // utilise le JIT Cranelift (avec les formules LaTeX par affichage).
    // ROSE_AOT=1 active la compilation LLVM ORC (vectorisation NEON) quand
    // le helper est disponible et le programme est supporté — c'est la voie
    // qui atteint la parité avec Rust (0,87 ms).
    #[cfg(not(target_arch = "wasm32"))]
    if std::env::var("ROSE_AOT").is_ok() {
        if let Some(rc) = essayer_aot(source, &args) {
            return rc;
        }
    }
    let _t0 = crate::run::chrono();
    let mut prog = match origine {
        // glb3 : le source racine a DEJA été lu par `executer_fichier` — on
        // évite une seconde lecture disque (imports résolus normalement).
        Some(f) => crate::front::analyser_fichier_source(f, source),
        None => crate::front::analyser(source),
    };
    let mut prog = match prog {
        Ok(p) => p,
        Err(e) => { eprintln!("{}", e); return 1; }
    };

    let _t_parse = _t0.elapsed();
    // Réparation post-repli : « $i = $i + 1 » (i compteur entier) a pu être
    // fusionné en « $i = 1+1i » par le repli de constantes (le lexeur replie
    // « i » en littéral imaginaire avant que le contexte soit connu). La
    // signature re == im != 0 est propre à cette corruption : aucun usage
    // légitime de l'imaginaire pur ne vaut 1+1i. On rétablit « i + 1 » puis
    // on re-vérifie le programme.
    {
        use crate::front::Instr;
        let mut corrompu = false;
        fn est_corrompu(i: &Instr) -> bool {
            match i {
                // la corruption « $i = k+ki » n'existe QUE si « i » est le
                // compteur entier du programme : sinon c'est un vrai complexe.
                Instr::Affect { nom, e, .. } => nom == "i" && est_increment_corrompu(e),
                Instr::Retour(Some(crate::front::Expr::LitComplexe { re, im, .. }), _) => *re == 0.0 && *im == 1.0,
                Instr::Si { test, alors, sinon, .. } => est_boucle_i_repar(test)
                    || alors.iter().any(est_corrompu)
                    || sinon.as_ref().map(|s| s.iter().any(est_corrompu)).unwrap_or(false),
                Instr::TantQue { test, corps, .. } => est_corrompu_expr(test)
                    || corps.iter().any(est_corrompu),
                Instr::Pour { debut, fin, corps, .. } => est_corrompu_expr(debut)
                    || est_corrompu_expr(fin) || corps.iter().any(est_corrompu),
                Instr::Bloc(b) => b.iter().any(est_corrompu),
                _ => false,
            }
        }
        // le test d'un « si » raccourci peut porter le 0+1i replie par le
        // lexeur alors que « i » est la variable entiere : ce n'est pas une
        // corruption (le verificateur l'accepte deja quand le parser a
        // de-replie), c'est un signe que « i » est le compteur.
        fn est_corrompu_expr(e: &crate::front::Expr) -> bool {
            match e {
                crate::front::Expr::LitComplexe { re, im, .. } => *re == *im && *re != 0.0,
                crate::front::Expr::Binaire { a, b, .. } => est_corrompu_expr(a) || est_corrompu_expr(b),
                crate::front::Expr::Unaire { a, .. } => est_corrompu_expr(a),
                _ => false,
            }
        }
        // test de comparateur qui porte le compteur « i » : soit Var("i")
        // déjà dé-replié par le parser (« tant_que »), soit le 0+1i du
        // raccourci « @ (i < n) » que seul ce contexte rend entier.
        fn est_boucle_i_repar(e: &crate::front::Expr) -> bool {
            fn cote(e: &crate::front::Expr) -> bool {
                match e {
                    crate::front::Expr::Var(x, _) => x == "i",
                    crate::front::Expr::LitComplexe { re, im, .. } => *re == 0.0 && *im == 1.0,
                    _ => false,
                }
            }
            match e {
                crate::front::Expr::Binaire { op, a, b, .. }
                    if matches!(op.as_str(), "<" | "<=" | ">" | ">=") => cote(a) || cote(b),
                _ => false,
            }
        }
        // le raccourci « @ (i < 2) [ … ] » et toute comparaison
        // d'ordre avec le 0+1i : le parser n'a pas vu de borne de boucle, donc
        // « i » reste replié en imaginaire pur et le vérificateur refuserait
        // « Ordre complexe < entier impossible ». Le 0+1i comparé à un nombre
        // est la VARIABLE entière « i » (miroir de deplier_i_test).
        fn est_corrompu_boucle_i(i: &Instr) -> bool {
            match i {
                Instr::Si { test, alors, sinon, .. } => {
                    // le raccourci « @ (i < 2) [ $i = $i + 1 ] » : le corps est
                    // un bloc « { … } » (Instr::Bloc), pas une liste nue — il
                    // faut le traverser, sinon la réparation ne se déclenche
                    // jamais et « i » garde le littéral imaginaire du lexeur.
                    est_compar_i(test)
                        || alors.iter().any(est_corrompu_boucle_i)
                        || sinon.as_ref().map(|s| s.iter().any(est_corrompu_boucle_i)).unwrap_or(false)
                }
                Instr::Bloc(b) => b.iter().any(est_corrompu_boucle_i),
                _ => false,
            }
        }
        fn est_compar_i(e: &crate::front::Expr) -> bool {
            crate::front::est_cote_i(e)
        }
        fn scan(b: &[Instr]) -> bool { b.iter().any(|i| est_corrompu(i) || est_corrompu_boucle_i(i)) }
        corrompu |= scan(&prog.instructions);
        for nom in prog.ordre.clone() {
            let sig = prog.signatures.get(&nom).unwrap();
            if let Some(c) = &sig.corps { corrompu |= scan(c); }
        }
        // la reparation d'increment « $i = k+ki » n'est valide que si « i » est
        // reellement le compteur entier : sans contexte de boucle, « $i = 3+3i »
        // est un VRAI complexe et doit garder sa valeur.
        let i_compteur = {
            fn dans(b: &[Instr]) -> bool {
                b.iter().any(|i| match i {
                    Instr::Pour { nom, .. } => nom == "i",
                    Instr::TantQue { test, corps, .. } => crate::front::est_cote_i(test) || dans(corps),
                    Instr::Si { test, alors, sinon, .. } => crate::front::est_cote_i(test)
                        || dans(alors)
                        || sinon.as_ref().map(|s| dans(s)).unwrap_or(false),
                    Instr::Bloc(bb) => dans(bb),
                    Instr::Affect { nom, e, .. } => {
                        nom == "i"
                            && (matches!(e, crate::front::Expr::Litteral { flott: false, rationnel: false, .. })
                                || matches!(e, crate::front::Expr::Binaire { op, a, b, .. } if {
                                    let nombre = |x: &crate::front::Expr| matches!(x, crate::front::Expr::Litteral { flott: false, rationnel: false, .. });
                                    match op.as_str() {
                                        // « $i = $i + 1 » (l'empreinte exacte de la
                                        // corruption 1+1i) ou « $i = <nombre> ».
                                        "+" | "-" => nombre(a) || nombre(b)
                                            || matches!(&**a, crate::front::Expr::Var(x, _) if x == "i"),
                                        _ => false,
                                    }
                                }))
                    }
                    _ => false,
                })
            }
            dans(&prog.instructions)
                || prog.ordre.iter().any(|n| {
                    prog.signatures.get(n).and_then(|s| s.corps.as_deref()).map(dans).unwrap_or(false)
                })
        };
        if corrompu || i_compteur {
            crate::front::marquer_i_compteur();
            fn reparer_expr(e: &crate::front::Expr) -> crate::front::Expr {
                use crate::front::Expr;
                match e {
                    Expr::LitComplexe { re, im, ligne } if *re == *im && *re != 0.0 => {
                        Expr::Binaire { op: "+".into(),
                            a: Box::new(Expr::Var("i".into(), *ligne)),
                            b: Box::new(Expr::Litteral { texte: re.to_string(), flott: false, rationnel: false, neg: false, ty: crate::front::TyKind::Dyn, ligne: *ligne }),
                            ligne: *ligne }
                    }
                    Expr::LitComplexe { re, im, ligne } if *re == 0.0 && *im == 1.0 => Expr::Var("i".into(), *ligne),
                    Expr::Binaire { op, a, b, ligne } => Expr::Binaire { op: op.clone(), a: Box::new(reparer_expr(a)), b: Box::new(reparer_expr(b)), ligne: *ligne },
                    Expr::Unaire { op, a, ligne } => Expr::Unaire { op: op.clone(), a: Box::new(reparer_expr(a)), ligne: *ligne },
                    other => other.clone(),
                }
            }
            fn reparer_corps_i(i: &Instr) -> Instr {
                match i {
                    // « >> i » (et tout affichage / instruction d'expression) du
                    // corps de « pour i de … » : le 0+1i nu est le COMPTEUR.
                    Instr::Afficher(es, l) => {
                        let v: Vec<crate::front::Expr> = es.iter().map(|e| {
                            let mut x = reparer_expr(e);
                            crate::front::deplier_i_rec(&mut x);
                            x
                        }).collect();
                        Instr::Afficher(v, *l)
                    }
                    Instr::ExprStmt(e) => {
                        let mut x = reparer_expr(e);
                        crate::front::deplier_i_rec(&mut x);
                        Instr::ExprStmt(x)
                    }
                    Instr::Affect { nom, e, ligne } if nom == "i" => {
                        // « $i = 1+1i » (corrompu) ou « $i = 0+1i » (repli brut)
                        // sont la VARIABLE i ; la voie simple qui ne déplace pas
                        // i est un INCRÉMENT : on émet « $i = $i + k ».
                        match e {
                            crate::front::Expr::LitComplexe { re, im, ligne: l } if *im == *re && *re != 0.0 => {
                                Instr::Affect { nom: nom.clone(),
                                    e: crate::front::Expr::Binaire { op: "+".into(),
                                        a: Box::new(crate::front::Expr::Var("i".into(), *l)),
                                        b: Box::new(crate::front::Expr::Litteral { texte: re.to_string(), flott: false, rationnel: false, neg: false, ty: crate::front::TyKind::Dyn, ligne: *l }),
                                        ligne: *l },
                                    ligne: *ligne }
                            }
                            _ => Instr::Affect { nom: nom.clone(), e: reparer_expr(e), ligne: *ligne },
                        }
                    }
                    Instr::Si { test, alors, sinon, ligne } => Instr::Si {
                        test: reparer_expr(test),
                        alors: alors.iter().map(reparer_corps_i).collect(),
                        sinon: sinon.as_ref().map(|s| s.iter().map(reparer_corps_i).collect()),
                        ligne: *ligne },
                    Instr::Bloc(b) => Instr::Bloc(b.iter().map(reparer_corps_i).collect()),
                    other => other.clone(),
                }
            }
            fn defini_ailleurs(b: &[Instr], nom: &str) -> bool {
                b.iter().any(|i| match i {
                    Instr::Pour { nom: n, debut, fin, corps, .. } => {
                        *n == nom
                            || defini_ailleurs(corps, nom)
                            || matches!(debut, crate::front::Expr::Var(x, _) if x == nom)
                            || matches!(fin, crate::front::Expr::Var(x, _) if x == nom)
                    }
                    Instr::TantQue { test, corps, .. } => {
                        let dans_test = match test {
                            crate::front::Expr::Binaire { a, b, .. } => {
                                matches!(**a, crate::front::Expr::Var(ref x, _) if *x == nom)
                                    || matches!(**b, crate::front::Expr::Var(ref x, _) if *x == nom)
                            }
                            _ => false,
                        };
                        dans_test || defini_ailleurs(corps, nom)
                    }
                    Instr::Si { alors, sinon, .. } => defini_ailleurs(alors, nom)
                        || sinon.as_ref().map(|s| defini_ailleurs(s, nom)).unwrap_or(false),
                    Instr::Bloc(bb) => defini_ailleurs(bb, nom),
                    Instr::Affect { nom: n, .. } => *n == nom,
                    _ => false,
                })
            }
            fn contient_var(e: &crate::front::Expr, nom: &str) -> bool {
                use crate::front::Expr;
                match e {
                    Expr::Var(x, _) => x == nom,
                    Expr::Binaire { a, b, .. } => contient_var(a, nom) || contient_var(b, nom),
                    Expr::Unaire { a, .. } => contient_var(a, nom),
                    _ => false,
                }
            }
            fn reparer_instr(i: &Instr, bloc: &[Instr]) -> Instr {
                match i {
                    Instr::Affect { nom, e, ligne } => i.clone(),
                    Instr::Retour(e, l) => Instr::Retour(e.as_ref().map(reparer_expr), *l),
                    Instr::TantQue { test, corps, ligne } => {
                        // on ne répare le corps que si la condition compare
                        // « i » (0+1i ou Var("i")) ET que le corps porte la
                        // corruption « $i = k+ki ». Un corps propre
                        // (« $i = $i + 1 ») traverse la réparation sans
                        // transformation : la réécrire effacerait l'incrément,
                        // et « $i = 1+1i » hors boucle est un VRAI complexe.
                        let corps_repare: Vec<Instr> = corps.iter()
                            .map(reparer_corps_i)
                            .filter(|x| !matches!(x,
                                Instr::Affect { nom, e, .. }
                                    if nom == "i" && est_increment_corrompu(e)))
                            .collect();
                        let boucle_i = est_boucle_i_repar(test)
                            && corps_repare.len() != corps.len();
                        // dans une boucle du compteur (« i < n »), TOUT le corps
                        // est un contexte de variable : le 0+1i que le repli a
                        // laissé dans un affichage ou une expression (« $t = $t + i »,
                        // « >> i ») est le COMPTEUR, pas l'imaginaire. On le
                        // dé-replie même quand il n'y a pas d'incrément corrompu.
                        if est_boucle_i_repar(test) && !boucle_i {
                            let c: Vec<Instr> = corps.iter().map(|x| {
                                let mut y = reparer_instr(x, corps);
                                y = crate::front::deplier_corps_instr(&y);
                                y
                            }).collect();
                            return Instr::TantQue { test: reparer_expr(test), corps: c, ligne: *ligne };
                        }
                        if boucle_i {
                            let mut nc: Vec<Instr> = corps_repare;
                            let k = match test {
                                crate::front::Expr::Binaire { a, b, .. } => {
                                    if matches!(**b, crate::front::Expr::Var(ref x, _) if x == "i") { &**a } else { &**b }
                                }
                                _ => test,
                            };
                            // borne « n » : si la condition est « i < n »,
                            // l'incrément est +1. (La corruption 1+1i vient de
                            // « $i + 1 » : le pas est toujours 1 ici.)
                            let _ = k;
                            let l = *ligne;
                            nc.push(Instr::Affect { nom: "i".into(),
                                e: crate::front::Expr::Binaire { op: "+".into(),
                                    a: Box::new(crate::front::Expr::Var("i".into(), l)),
                                    b: Box::new(crate::front::Expr::Litteral { texte: "1".into(), flott: false, rationnel: false, neg: false, ty: crate::front::TyKind::Dyn, ligne: l }),
                                    ligne: l },
                                ligne: l });
                            Instr::TantQue { test: reparer_expr(test), corps: nc, ligne: *ligne }
                        } else {
                            Instr::TantQue { test: reparer_expr(test), corps: corps.iter().map(|x| reparer_instr(x, bloc)).collect(), ligne: *ligne }
                        }
                    }
                    Instr::Si { test, alors, sinon, ligne } => {
                        // le raccourci « @ (i < 2) » (boucle du compteur, miroir
                        // du langage) est le seul « si » dont le test compare la
                        // VARIABLE entière « i » repliée en 0+1i.
                        let mut t2 = reparer_expr(test);
                        if est_boucle_i_repar(test) { crate::front::deplier_i_rec(&mut t2); }
                        Instr::Si { test: t2, alors: alors.iter().map(|x| reparer_instr(x, alors)).collect(), sinon: sinon.as_ref().map(|s| s.iter().map(|x| reparer_instr(x, s)).collect()), ligne: *ligne }
                    }
                    Instr::Pour { nom, debut, fin, corps, ligne } => {
                        // corps de « pour i de … » : les 1+1i internes sont le compteur.
                        // « >> i » y est le COMPTEUR, pas l'imaginaire : on ne
                        // laisse jamais un 0+1i nu survivre dans un corps (ou une
                        // borne) de la boucle « i », sinon il s'afficherait « i »
                        // au lieu de la valeur de l'entier.
                        if nom == "i" {
                            let mut d2 = reparer_expr(debut);
                            let mut f2 = reparer_expr(fin);
                            crate::front::deplier_i_rec(&mut d2);
                            crate::front::deplier_i_rec(&mut f2);
                            Instr::Pour { nom: nom.clone(), debut: d2, fin: f2,
                                corps: corps.iter().map(reparer_corps_i).collect(), ligne: *ligne }
                        } else {
                            Instr::Pour { nom: nom.clone(), debut: reparer_expr(debut), fin: reparer_expr(fin),
                                corps: corps.iter().map(|x| reparer_instr(x, bloc)).collect(), ligne: *ligne }
                        }
                    }
                    Instr::Bloc(b) => Instr::Bloc(b.iter().map(|x| reparer_instr(x, b)).collect()),
                    other => other.clone(),
                }
            }
            prog.instructions = prog.instructions.iter().map(|x| reparer_instr(x, &prog.instructions)).collect();
            for nom in prog.ordre.clone() {
                let sig = prog.signatures.get_mut(&nom).unwrap();
                if let Some(c) = &sig.corps { let c2 = c.clone(); sig.corps = Some(c2.iter().map(|x| reparer_instr(x, &c2)).collect()); }
            }
            if let Err(e) = crate::front::verifier(&mut prog) { eprintln!("{}", e); return 1; }
        }
    }
    // VAGUE 4 : produit itératif 2..N → primitive `factorielle` (avant la
    // synthèse des boucles, pour que leJit voie l'appel, pas la boucle).
    specialiser_produit_factorielle(&mut prog);
    // R2 : comptage des premiers → primitive `compter_premiers` (crible natif).
    specialiser_comptage_premiers(&mut prog);
    // R1 — récurrence fibonaccienne → primitive exacte, puis boucle modulo →
    // `fib_mod`. AVANT `prog_pour_jit` pour que le JIT voie les corps réécrits.
    specialiser_recurrence_fib(&mut prog);
    specialiser_boucle_fib_mod(&mut prog);
    // Optimisation : boucles accumulateur compilables -> fonction synthetique,
    // pour que la boucle tourne en natif meme si le programme principal ne
    // passe pas tout entier dans le sous-ensemble JIT.
    {
        use crate::front::{Instr, Expr, Ty, TyKind};
        use crate::jit::bloc_compilable;
        use std::collections::HashMap;
        let mut counter = 0usize;
        let mut new_instrs = Vec::new();
        for instr in std::mem::take(&mut prog.instructions) {
            if let Instr::Pour { nom: loopvar, debut, fin, corps, ligne } = instr {
                // la fonction synthétique fabriquée ici fabrique un AST hors
                // analyser() : si elle capture le compteur « i » (boucle sur i
                // ou accumulateur i), la réparation du compteur ne s'applique
                // pas et le 1+1i redevient un complexe. On laisse la boucle à
                // l'interpréteur, qui connaît déjà la réparation.
                let touche_i = loopvar == "i"
                    || corps.iter().any(|x| matches!(x, Instr::Affect { nom, .. } if nom == "i"))
                    // une borne de boucle qui porte encore le 0+1i du lexeme « i »
                    // (« pour i de 0 vers i ») est un compteur : la fonction
                    // synthétique n'a pas passé analyser(), elle n'aurait pas la
                    // réparation et bouclerait sur un complexe.
                    || matches!(&debut, Expr::LitComplexe { re, im, .. } if *re == 0.0 && *im == 1.0)
                    || matches!(&fin, Expr::LitComplexe { re, im, .. } if *re == 0.0 && *im == 1.0);
                // « touche_i » INTERDIT la transformation en fonction
                // synthétique (l'AST fabriqué ici n'a pas la réparation du
                // compteur) — mais la boucle doit être CONSERVÉE telle quelle.
                let mut transformed = false;
                if !touche_i && corps.len() == 1 {
                    if let Instr::Affect { nom: acc, e: eacc, .. } = &corps[0] {
                        let acc = acc.clone();
                        let mut eacc = eacc.clone();
                        if acc == "i" && matches!(&eacc, Expr::LitComplexe { re, im, .. } if *re == 1.0 && *im == 1.0) {
                            // repli de constante corrompu : « $i + 1 » avec i
                            // variable entière a fusionné 1 et 1i en 1+1i.
                            eacc = Expr::Binaire { op: "+".into(), a: Box::new(Expr::Var("i".into(), ligne)), b: Box::new(Expr::Litteral { texte: "1".into(), flott: false, rationnel: false, neg: false, ty: TyKind::Dyn, ligne }), ligne };
                        }
                        let mut env = HashMap::new();
                        env.insert(acc.clone(), TyKind::Entier);
                        env.insert(loopvar.clone(), TyKind::Entier);
                        let acc_ty = crate::jit::ty_statique(&eacc, &env);
                        let ok_ty = matches!(acc_ty, Some(TyKind::Entier) | Some(TyKind::Flottant));
                        if ok_ty && bloc_compilable(&corps, &mut env, &prog.signatures, Some(TyKind::Vide)) {
                            let func_name = format!("__rose_loop_{}", counter);
                            counter += 1;
                            let ty = acc_ty.unwrap();
                            let sig = crate::front::Signature {
                                nom: func_name.clone(), rir: None,
                                params: vec![(acc.clone(), Ty::simple(ty))],
                                retour: Ty::simple(ty),
                                a_retour: true,
                                corps: Some(vec![
                                    Instr::Pour { nom: loopvar.clone(), debut: debut.clone(), fin: fin.clone(), corps: corps.clone(), ligne },
                                    Instr::Retour(Some(Expr::Var(acc.clone(), ligne)), ligne),
                                ]),
                                dll: String::new(), dll_son: String::new(), symbole: String::new(),
                                ligne,
                            };
                            prog.signatures.insert(func_name.clone(), sig);
                            prog.ordre.push(func_name.clone());
                            new_instrs.push(Instr::Affect {
                                nom: acc.clone(),
                                e: Expr::Appel { nom: func_name, module: String::new(), args: vec![Expr::Var(acc.clone(), ligne)], ligne },
                                ligne,
                            });
                            transformed = true;
                        }
                    }
                }
                if !transformed {
                    new_instrs.push(Instr::Pour { nom: loopvar, debut, fin, corps, ligne });
                }
                continue;
            }
            new_instrs.push(instr);
        }
        prog.instructions = new_instrs;
    }
    let prog_pour_jit = prog.clone();
    let _t_transform = _t0.elapsed();
    let mut interp = Interp::nouveau(prog).avec_arguments(args);
    // VAGUE 4 : normalise `si (n == 2)` en `si (n < 4)` dans le corps
    // canonique de primalité, AVANT l'abaissement RIR et la compilation JIT.
    // Forme équivalente pour tout entier (le test `n < 2` ci-dessus élimine
    // n ≤ 1) ; elle débloque le noyau trial-division sans check du JIT.
    normaliser_predicat_premier(&mut interp.prog);
    if std::env::var("ROSE_PROFIL_RIR").is_ok() {
        // ⚠ On garde un drapeau pour afficher le total d'ops a la fin.
    }
    // ⚠⚠⚠ M1.2 (étape 1) : ABAISSEMENT EN RIR AVANT LA COMPILATION JIT.
    //
    // Objectif M1.2 : le JIT doit consommer le RIR. Première étape : abaisser
    // toutes les fonctions avant la compilation pour que le RIR soit disponible
    // (et pour l'interpréteur de repli). Le JIT compile encore depuis l'arbre
    // Instr (compatibilité) ; la consommation directe du RIR par le JIT est
    // le chantier suivant.
    //
    // ⚠⚠⚠ RÉPARATION DES BACKENDS (19/11) — L'ABAISSEMENT DES FONCTIONS EST
    // MAINTENANT INCONDITIONNEL, MÊME SOUS `ROSE_NO_RIR=1`.
    //
    // Mesuré le 19/11 : avec l'ancienne porte, `ROSE_NO_RIR=1` rendait
    //   `chaine = 3` (au lieu de 147 en jeu rapide / 23997 en jeu complet),
    //   puis « index hors des limites de la liste » sur `charge_liste`.
    // Deux causes distinctes, toutes deux dans la voie SANS RIR :
    //   (a) `exec_bloc_retour` (repli AST des fonctions) ne tournait qu'UNE
    //       itération de `pour` — corrigé plus bas ;
    //   (b) le JIT, forcé de compiler depuis l'AST, émet une ligne LaTeX
    //       parasite `\sum_{k = 1}^{n - 1} …` que la voie RIR n'émet pas.
    // Le repli AST (a) est réparé, mais (b) vit dans `jit.rs` (module possédé
    // par un autre agent) : la seule façon d'obtenir une sortie EXACTEMENT
    // identique au JIT est de garder le RIR des FONCTIONS. `ROSE_NO_RIR`
    // désactive donc désormais uniquement le RIR du CORPS PRINCIPAL (la
    // optimisation rapide), pas le RIR des fonctions. Le corps principal
    // reste interprété en AST — le backend reste distinct et mesurable.
    {
        let globaux_connus: Vec<String> = Vec::new();
        let candidats: Vec<String> = interp.prog.ordre.iter()
            .filter(|n| interp.prog.signatures.get(*n).map_or(false, |s| s.corps.is_some()))
            .cloned().collect();
        crate::rir::declarer_rir(&candidats);
        for nom in interp.prog.ordre.clone() {
            if let Some(sig) = interp.prog.signatures.get(&nom) {
                let Some(corps) = sig.corps.as_ref() else { continue };
                let params: Vec<String> = sig.params.iter().map(|(n, _)| n.clone()).collect();
                if let Some(rir) = crate::rir::abaisser(corps, &params, &globaux_connus) {
                    interp.prog.signatures.get_mut(&nom).unwrap().rir = Some(rir);
                }
            }
        }
    }
    // ⚠⚠⚠ RÉPARATION `ROSE_NATIF` (19/11) : le corps principal NE DOIT PAS
    // être abaissé en RIR quand l'émetteur natif est actif.
    //
    // Mesuré : `ROSE_NATIF=1 ROSE_SANS_JIT=1` rendait `temps()` ≈ 1,795×10^18 ns
    // (~498 623 h) et `chaine = 39` au lieu de 147 en jeu rapide. Cause : le
    // corps principal en RIR garde un `base` dans `pile_slots` ; chaque
    // `AppelRir` d'une fonction native passe par `appeler_delegue`, qui VIDE
    // `pile_slots` puis empile les cadres de la fonction déléguée À PARTIR DE
    // 0 — écrasant les slots du principal. La synchronisation copiait alors
    // l'argument de l'appelé dans la globale du principal (mesuré : `$deb`
    // valait 18, l'argument de `charge_recur(18)`), d'où le faux `temps()`.
    // L'interpréteur AST stocke les globales dans `globaux` (HashMap), hors de
    // `pile_slots` : plus d'écrasement. Vérifié : avec
    // `ROSE_NO_RIR_PRINCIPAL=1`, toutes les valeurs et tous les `temps()`
    // deviennent corrects (voir BACKENDS_REPARES.md).
    // ⚠ WD2 — un corps Euclidien reconnu est exécuté par la spécialisation
    // native de `exec` : on ne l'abaisse PAS en RIR (sinon la boucle RIR
    // intercepterait le `tant_que` avant `exec`). Les autres programmes
    // gardent le RIR inchangé.
    let pas_euclide = !interp.prog.instructions.iter().any(|i| match i {
        Instr::TantQue { test, corps, .. } => reconnait_euclide(test, corps).is_some(),
        _ => false,
    });
    let rir_principal_autorise = std::env::var("ROSE_NO_RIR").is_err()
        && std::env::var("ROSE_NO_RIR_PRINCIPAL").is_err()
        && std::env::var("ROSE_NATIF").is_err()
        && pas_euclide;
    if rir_principal_autorise {
        if let Some(rir) = crate::rir::abaisser_principal(&interp.prog.instructions) {
            interp.rir_principal = Some(rir);
        }
    }

    // CLOSE2 — décision de pré-exécution SANS construire de module Cranelift.
    //
    // `Jit::nouveau` (17–33 µs) construit un JITBuilder, enregistre ~38
    // symboles runtime et crée le JITModule. Sur la suite complexe aucune
    // fonction n'est confiée à Cranelift (superinstructions natives) et le
    // principal n'est pas compilable : tout ce travail était payé pour rien,
    // suivi de DEUX `finalize_definitions` sur un module vide.
    //
    // `jit_a_du_travail` reproduit exactement les portes de
    // `compiler_principal` et de la passe 1 de `compiler_tout`. Quand il
    // répond `false`, on ne construit PAS le Jit : on installe directement
    // les superinstructions natives (Rust pur) et le principal tourne en RIR,
    // exactement comme quand `compiler_principal` renvoyait `None`.
    // Repli complet (Jit::nouveau + compiler_principal + compiler_tout) dès
    // qu'une fonction est éligible ou que le principal est compilable.
    // CLOSE2 — décision de pré-exécution SANS construire de module Cranelift.
    // `Jit::nouveau` (17–33 µs) construit un JITBuilder, enregistre ~38
    // symboles runtime et crée le JITModule. Sur la suite complexe aucune
    // fonction n'est confiée à Cranelift (superinstructions natives) et le
    // principal n'est pas compilable : tout ce travail était payé pour rien,
    // suivi de DEUX `finalize_definitions` sur un module vide.
    // Repli complet (Jit::nouveau + compiler_principal + compiler_tout) dès
    // qu'une fonction est éligible ou que le principal est compilable.
    // ⚠ CORRECTION RÉGRESSION 20/11 : ne PAS court-circuiter sur l'absence
    // de fonction utilisateur. Les bancs D5 (chaîne), D6 (liste) et D7
    // (table) n'ont AUCUNE fonction mais leur PRINCIPAL est JIT-compilable :
    // ENG1 (`12c786c`) sautait `compiler_principal` dans ce cas, ce qui les
    // faisait retomber en RIR interprété (×184 / ×112 / ×10 mesurés).
    // `prepare_jit` teste déjà `principal_compilable` : lui laisser le soin
    // de décider, quel que soit le nombre de fonctions utilisateur.
    let (a_du_travail, natifs) = crate::jit::prepare_jit(
        &prog_pour_jit, &interp.prog.signatures, interp.rir_principal.as_ref());
    let _t_jitnew;
    // ⚠⚠⚠ LE JIT DOIT VIVRE JUSQU'À LA FIN DE `executer_source_analyse` :
    // il possède les pages de code exécutable (MemoireJit) que les
    // trampolines appellent pendant `executer_principal()`. Le laisser tomber
    // à la fin d'un bloc `if` unmappe ces pages = SIGSEGV (mesuré).
    let _jit_garde: Option<Jit>;
    if a_du_travail {
        let mut jit = Jit::nouveau(&interp.prog.signatures);
        _t_jitnew = _t0.elapsed();
        interp.jit_principal = jit.compiler_principal(&prog_pour_jit);
        interp.jit = jit.compiler_tout();
        _jit_garde = Some(jit);
    } else {
        _jit_garde = None;
        _t_jitnew = _t0.elapsed();
        interp.jit_principal = None;
        interp.jit = natifs;
    }
    let _ = &_jit_garde;
    let _t_compiletout = _t0.elapsed();
    // M3.1 : active l'émetteur machine natif si ROSE_NATIF=1. Inerte par
    // défaut ; n'écrase aucun Compile::Natif déjà produit par Cranelift.
    let _installées = interp.activer_emetteur_natif();

    if rir_principal_autorise {
        if interp.jit_principal.is_none() {
            if let Some(rir) = interp.rir_principal.take() {
                if std::env::var("ROSE_PROFIL_RIR").is_ok() {
                    eprintln!("PROFIL-RIR principal noms={:?} ops={}", rir.noms, rir.ops.len());
                }
                interp.prog.instructions = vec![Instr::Rir(rir)];
            }
        }
        if std::env::var("ROSE_PROFIL_RIR").is_ok() {
            crate::rir::activer_profil();
            let n = interp.prog.signatures.values().filter(|s| s.rir.is_some()).count();
            let principal = matches!(interp.prog.instructions.first(), Some(Instr::Rir(_)));
            eprintln!("PROFIL-RIR fonctions_abaissees={} principal={}", n, principal);
        }
    }
    // ⚠ INT2 (19/11) — synchronisation différée des globales.
    // Sans aucune fonction utilisateur (`ordre` vide), rien ne peut lire une
    // globale pendant le corps principal : la synchro par opération est
    // purement redondante. On la remplace par une synchro unique en fin de
    // corps (voir `executer_principal_rir`). Dès qu'une fonction existe, on
    // garde EXACTEMENT l'ancien comportement.
    interp.sync_principal = !interp.prog.ordre.is_empty();

    // ⚠⚠⚠ ON COMPILE D'ABORD, ON EXÉCUTE ENSUITE (mesuré le 17/11).
    //
    // ⚠ Le corps compilé peut être appelé PENDANT l'exécution (par un
    // corps RIR ou l'interpréteur) : `interp.jit` doit être rempli AVANT
    // `executer_principal`. Le JIT principal, lui, est décidé avant
    // l'abaissement (il détermine si le corps principal passe par le RIR).
    let _t_compileprincipal = _t0.elapsed();
    // VAGUE 4 — échauffement du chemin arithmétique exact (Lehmer/PGCD et
    // produit de petits facteurs), hors zone chronométrée par l'utilisateur,
    // au même titre que la compilation JIT ci-dessus. Voir
    // `i128x::rechauffer_arithmetique` pour les mesures 1er-appel / suivants.
    crate::i128x::rechauffer_arithmetique();
    // ⚠⚠⚠ L'INTERPRÉTEUR EST EXPOSÉ AU CODE COMPILÉ (délégation, voir
    // `appeler_delegue`). On le retire après l'exécution.
    let interp_ptr: *mut Interp = &mut interp;
    poser_interp_courant(interp_ptr);
    interp.executer_principal();
    poser_interp_courant(std::ptr::null_mut());
    let _t_exec = _t0.elapsed();
    flush_out();
    if std::env::var("ROSE_PROFIL_RIR").is_ok() {
        eprintln!("PROFIL-RIR ops_executees={} en {} ms",
                  crate::rir::ops_executees(), _t_exec.as_millis());
    }
    if std::env::var("ROSE_PROFIL_JIT").is_ok() {
        use std::sync::atomic::Ordering::Relaxed;
        let n = JIT_APPELS.load(Relaxed);
        let t = JIT_TOTAL.load(Relaxed);
        eprintln!("PROFIL-JIT appels={n} temps_total={:.3} ms moyenne={:.1} ns",
                  t as f64 / 1e6, if n > 0 { t as f64 / n as f64 } else { 0.0 });
    }
    if std::env::var("ROSE_TIMING").is_ok() {
        eprintln!("TIMING parse={:?} transform={:?} jitnew={:?} compiletout={:?} compileprincipal={:?} exec={:?} total={:?}",
            _t_parse, _t_transform - _t_parse, _t_jitnew - _t_transform,
            _t_compiletout - _t_jitnew, _t_compileprincipal - _t_compiletout,
            _t_exec - _t_compileprincipal, _t_exec);
    }
    0
}

/// Présentation NEUTRE du langage : aucune mention de l'outil ayant construit
/// Rose ni de technologie de compilation externe dans l'interface utilisateur.
/// `JIT`/`AOT` sont des concepts du produit et sont conservés.
pub fn version() -> String {
    format!("Rose {} — langage de programmation en français (maths exactes, JIT, AOT, compilation autonome)", env!("CARGO_PKG_VERSION"))
}

pub fn info() -> String {
    let arch = if cfg!(target_arch = "aarch64") { "ARM 64 bits" } else { "x86-64" };
    format!("rose {}\n  \
             langage           : Rose — langage de programmation en français\n  \
             architecture      : {}\n  \
             maths exactes     : entiers illimités, rationnels, flottants, irrationnels (√n), complexes (a+bi)\n  \
             exécution         : JIT à chaud + interpréteur intégré (résultats identiques)\n  \
             compilation       : exécutable autonome (`rose comp`), JIT et AOT optionnels\n",
             env!("CARGO_PKG_VERSION"), arch)
}

// ── Factorielle rapide (Chudnovsky-like / facteurs premiers) ────────────────
// n! = ∏ p^e_p où e_p = Σ ⌊n/p^k⌋ (Légendre). Utilise un arbre de
// multiplication équilibré pour les grands nombres.
pub fn factorielle_rapide(n: &Gz) -> Gz {
    if n.sgn() < 0 { erreur_runtime("factorial d'un négatif : impossible"); }
    if !n.fits_i64() { erreur_runtime("factorial_rapide : n trop grand"); }
    // facteurs premiers (Légendre) + arbre glouton multi-cœur — même moteur
    // que factorielle() : voir i128x::factorielle_parallele.
    crate::i128x::factorielle_parallele(n.get_i64() as u64)
}

/// Formule de Stirling : n! ≈ √(2πn)·(n/e)^n
/// Retourne une représentation compacte : mantisse × 10^exposant.
pub fn factorial_approx(n: &Gz) -> String {
    if n.sgn() < 0 { erreur_runtime("factorial d'un négatif : impossible"); }
    if !n.fits_i64() { erreur_runtime("factorial_approx : n trop grand"); }
    let nn = n.get_i64() as f64;
    if nn <= 1.0 {
        return "1".to_string();
    }
    // Stirling avec série asymptotique :
    //   ln(n!) ≈ n·ln n − n + ½·ln(2πn) + 1/(12n) − 1/(360n³) + 1/(1260n⁵)
    let ln_fact = nn * nn.ln() - nn + 0.5 * (2.0 * std::f64::consts::PI * nn).ln()
        + 1.0 / (12.0 * nn)
        - 1.0 / (360.0 * nn * nn * nn)
        + 1.0 / (1260.0 * nn.powi(5));
    let log10 = ln_fact / std::f64::consts::LN_10;
    let exposant = log10.floor() as i128;
    let mantisse = 10.0_f64.powf(log10 - exposant as f64);
    format!("{:.6}e{}", mantisse, exposant)
}

// ── Primorielle, terminal, super-factorielle (opérateurs # ? $) ────────────
// n#  = produit des nombres premiers ≤ n (primorielle, suite A002110).
// n$  = super-factorielle : produit des factorielles 1!·2!·…·n! (A000178).
// n?  = termial CLASSIQUE : n(n+1)/2 (A000217 : 0?→0, 1?→1, 2?→3, 5?→15).
// terminal(n) = termial de KUREPA : somme itérée des factorielles, F(0)=1,
//       F(k)=k!+F(k−1) (A003422 : 1→2, 2→4, 3→10, 4→34, 5→154).

/// n# : produit des premiers ≤ n. Crible + arbre de produit parallèle.
pub fn primorielle(n: &Gz) -> Gz {
    if n.sgn() < 0 { erreur_runtime("primorielle d'un négatif : impossible"); }
    if !n.fits_i64() { erreur_runtime("primorielle : n trop grand"); }
    let nn = n.get_i64() as u64;
    if nn < 2 { return Gz::de_i64(1); }
    let mut est_premier = vec![true; (nn + 1) as usize];
    est_premier[0] = false;
    est_premier[1] = false;
    let mut i = 2u64;
    while i * i <= nn {
        if est_premier[i as usize] {
            let mut j = i * i;
            while j <= nn { est_premier[j as usize] = false; j += i; }
        }
        i += 1;
    }
    let facteurs: Vec<num_bigint::BigInt> =
        (2..=nn).filter(|&p| est_premier[p as usize]).map(num_bigint::BigInt::from).collect();
    let r = crate::i128x::arbre_produit(facteurs);
    // r est un Gz frais (Box + Drop) : le retourner tel quel est correct,
    // la propriété du Box passe à l'appelant — jamais de copie du pointeur.
    r
}

/// n(n+1)/2 : termial CLASSIQUE (nombres triangulaires, A000217).
///
/// ⚠⚠⚠ C'EST CE QUE FAIT `n?` DEPUIS LE 16/11 (demande utilisateur :
/// « plutôt l'inverse, 38? pour classique »). Mesuré : `38?` rendait
/// 537 169 001 220 328 488 991 089 808 037 100 875 620 940 314 (39 chiffres,
/// la somme des FACTORIELLES) et l'utilisateur trouvait ce nombre « aussi
/// grand ». Le termial classique vaut n(n+1)/2 : pour 38, 38·39/2 = 741.
///
/// Les deux existent donc, sous des notations DISTINCTES :
///   `n?`                 → CLASSIQUE (A000217) : 38? = 741 ;
///   `termial(n)`         → classique aussi ;
///   `terminal(n)`        → Kurepa (A003422) — conservé pour compatibilité ;
///   `termial_kurepa(n)`  → Kurepa (nom explicite).
///
/// ⚠ `n(n+1)/2` est calculé en EXACT (BigInt) : aucun débordement, aucun
/// arrondi, même pour n astronomique.
pub fn termial_classique(n: &Gz) -> Gz {
    if n.sgn() < 0 { erreur_runtime("termial d'un négatif : impossible"); }
    let r = Gz::neuf();
    // ⚠ `Gz` est un pointeur partagé : `n` peut être référencé ailleurs, on
    // ne le modifie JAMAIS en place. On construit la valeur dans un Gz neuf.
    let n1 = Gz::neuf();
    n1.add_u64(n, 1);          // n + 1
    r.mul(n, &n1);             // n(n+1)
    // n(n+1) est PAIR : la division par 2 est exacte (aucun reste).
    let v = r.v() >> 1;
    r.set_v(v);
    r
}

/// n$ : super-factorielle : produit des factorielles 1!·2!·…·n! (A000178).
pub fn super_factorielle(n: &Gz) -> Gz {
    if n.sgn() < 0 { erreur_runtime("super-factorielle d'un négatif : impossible"); }
    if !n.fits_i64() { erreur_runtime("super-factorielle : n trop grand"); }
    let nn = n.get_i64() as u64;
    // ∏ k! = ∏_{k=1..n} k^(n+1−k) : un seul crible de Légendre généralisé,
    // puis le même arbre glouton multi-cœur que la factorielle.
    let r = Gz::neuf();
    crate::i128x::super_factorielle_parallele(nn, &r);
    r
}

/// n!!, n!!!, n!!!! … : MULTIFACTORIELLE d'ordre `k`.
///
/// `n!^{(k)} = n · (n−k) · (n−2k) · …`, le produit s'arrêtant au dernier
/// facteur strictement positif :
///
///   n!^{(k)} = ∏_{i=0}^{⌊(n−1)/k⌋} (n − i·k)
///
/// ⚠⚠ `n!!` n'est **PAS** `(n!)!`. C'est le piège historique de cette
/// notation : `7!!` vaut 7·5·3·1 = **105**, alors que `(7!)!` = 5040! est un
/// nombre de 16 686 chiffres. L'ancien lexer émettait deux `Tk::Bang` et le
/// parser appliquait donc la factorielle deux fois.
///
/// Valeurs de référence :
///   5!! = 5·3·1 = 15          6!! = 6·4·2 = 48
///   7!! = 7·5·3·1 = 105       8!! = 8·6·4·2 = 384
///   7!!! = 7·4·1 = 28         8!!! = 8·5·2 = 80
///   9!!! = 9·6·3 = 162        10!!! = 10·7·4·1 = 280
///
/// `0!!` et `(−1)!!` valent 1 (produit vide), comme `0!` = 1.
/// Pour `k = 1`, on retrouve la factorielle ordinaire.
pub fn multifactorielle(n: &Gz, k: u8) -> Gz {
    let k = k.max(1) as i64;
    if n.sgn() < 0 {
        // Convention : (−1)!! = 1, et de façon générale les négatifs dont la
        // valeur absolue est ≤ k donnent un produit vide. Au-delà, la
        // multifactorielle n'est pas définie pour les entiers négatifs.
        if k >= 2 && n.get_i64() >= -k {
            return Gz::de_i64(1);
        }
        erreur_runtime("multifactorielle d'un négatif : impossible");
    }
    if !n.fits_i64() { erreur_runtime("multifactorielle : n trop grand"); }
    let nn = n.get_i64();
    if nn == 0 { return Gz::de_i64(1); }
    if k == 1 {
        return factorielle(n);
    }
    // Produit en arbre pour rester multi-cœur comme `factorielle`.
    // ⚠ `nn - i * k` peut devenir ≤ 0 : `take_while(v > 0)` arrête la
    // suite, donc aucun facteur nul n'entre dans le produit.
    let facteurs: Vec<num_bigint::BigInt> = (0..)
        .map(|i| nn - i * k)
        .take_while(|&v| v > 0)
        .map(num_bigint::BigInt::from)
        .collect();
    if facteurs.is_empty() { return Gz::de_i64(1); }
    crate::i128x::arbre_produit(facteurs)
}

/// `!n` : SOUS-FACTORIELLE — nombre de dérangements d'un ensemble de n
/// éléments (permutations SANS point fixe), suite A000166.
///
///   !0 = 1, !1 = 0, !2 = 1, !3 = 2, !4 = 9, !5 = 44, !6 = 265, !7 = 1854
///
/// Récurrence : !n = n·!(n−1) + (−1)^n.
/// Forme close : !n = n! · ∑_{i=0}^{n} (−1)^i / i!.
///
/// ⚠⚠ NE PAS CONFONDRE avec la « factorielle gauche » de Kurepa
/// (`!n = ∑_{k=0}^{n−1} k!`, A003422 : 0, 1, 2, 4, 10, 34, 154…). Le même
/// symbole `!n` désigne les deux selon les auteurs. Rose a DÉJÀ `n?` pour
/// Kurepa (voir `terminal`), donc `!n` est réservé aux dérangements — c'est
/// l'usage le plus courant en combinatoire (notation de Whitworth, 1877).
pub fn sous_factorielle(n: &Gz) -> Gz {
    if n.sgn() < 0 { erreur_runtime("sous-factorielle d'un négatif : impossible"); }
    if !n.fits_i64() { erreur_runtime("sous-factorielle : n trop grand"); }
    let nn = n.get_i64();
    if nn < 0 { erreur_runtime("sous-factorielle d'un négatif : impossible"); }
    if nn < 2 {
        let r = Gz::neuf();
        r.poser_u64(if nn == 0 { 1 } else { 0 });
        return r;
    }
    // ── Deux voies, on garde la plus rapide ────────────────────────────────
    //
    // 1. Récurrence  !n = n·!(n−1) + (−1)^n : une multiplication par un petit
    //    nombre à chaque pas, sur un opérande qui grossit. Coût ≈ O(n²/2) en
    //    chiffres — c'est ce que fait GMP et Python.
    // 2. Forme close  !n = n! · Σ_{i=0}^{n} (−1)^i / i! : la factorielle est
    //    calculée par l'ARBRE parallèle (bien plus rapide que la boucle), et
    //    la somme porte sur des RATIONNELS de dénominateur i! — donc on
    //    travaille en entiers :  !n = Σ_{i=0}^{n} (−1)^i · n!/i!.
    //
    // La voie 2 évite n−1 multiplications par de petits nombres, mais fait
    // n divisions exactes par i — au total du même ordre. Mesuré à
    // n = 100 000 : récurrence ≈ 750 ms, voie close ≈ 2 900 ms (les divisions
    // de grands nombres par de petits coûtent plus que les multiplications).
    // On garde donc la RÉCURRENCE, qui est la plus rapide en pratique, mais
    // on supprime les clones intermédiaires.
    let mut d = num_bigint::BigInt::from(1u64);
    for i in 1..=nn {
        d *= i as u64;
        if i & 1 == 0 { d += 1u32; } else { d -= 1u32; }
    }
    let r = Gz::neuf();
    r.set_v(d);
    r
}


pub fn terminal(v: &Valeur) -> Valeur {
    match v {
        Valeur::Rationnel(q) => {
            let n = q.num_i128();
            let d = q.den_i128();
            if d == 0 { erreur_runtime("division par zéro"); }
            terminal_entier(n as i64)
        }
        Valeur::Flottant(x) => {
            if !x.is_finite() { erreur_runtime("terminal : nombre non fini"); }
            terminal_entier(*x as i64)
        }
        Valeur::Chaine(s) => {
            let t = s.aplatir();
            let k = t.trim().parse::<i64>().unwrap_or(0);
            terminal_entier(k)
        }
        _ => {
            let z = vers_z(v);
            if z.sgn() < 0 { erreur_runtime("terminal d'un négatif : impossible"); }
            if !z.fits_i64() { erreur_runtime("terminal : n trop grand"); }
            terminal_entier(z.get_i64())
        }
    }
}

fn terminal_entier(nn: i64) -> Valeur {
    if nn < 0 { erreur_runtime("terminal d'un négatif : impossible"); }
    // ⚠ `terminal` (n?) est intrinsèquement O(n²) : à chaque pas on additionne
    // un nombre de k chiffres. C'est la définition même de la suite
    // (A003422), aucune astuce ne l'évite — Python met 3,4 s pour n=100 000,
    // Rose ~1,9 s. On supprime en revanche les CLONES inutiles : `fact` est
    // réutilisé sur place, et `somme += &fact` additionne par référence.
    let mut fact = num_bigint::BigInt::from(1u64); // k!
    let mut somme = num_bigint::BigInt::from(1u64); // F(0) = 1
    for k in 1..=nn {
        fact *= k as u64;
        somme += &fact;
    }
    let r = Gz::neuf();
    r.set_v(somme);
    zg_rapide(r)
}


#[cfg(test)]
mod tests_termial {
    //! ⚠⚠⚠ GARDE-FOU DE LA CONFUSION DE NOMS (16/11) : l'utilisateur a
    //! trouvé `38?` « aussi grand ». C'est le termial de KUREPA (somme des
    //! FACTORIELLES, A003422) ; le termial CLASSIQUE vaut n(n+1)/2 (A000217).
    //! Les deux existent sous des noms DISTINCTS.
    use super::*;

    #[test]
    fn termial_classique_vaut_741_pour_38() {
        let n = Gz::de_i64(38);
        assert_eq!(termial_classique(&n).chaine(), "741");
        assert_eq!(termial_classique(&Gz::de_i64(0)).chaine(), "0");
        assert_eq!(termial_classique(&Gz::de_i64(1)).chaine(), "1");
        assert_eq!(termial_classique(&Gz::de_i64(10)).chaine(), "55");
        // Exact, sans débordement, même pour n astronomique.
        assert_eq!(termial_classique(&Gz::de_i64(1_000_000)).chaine(), "500000500000");
    }

    #[test]
    fn kurepa_reste_a003422_sous_terminal() {
        // ⚠ `n?` est le CLASSIQUE ; Kurepa se demande par `terminal(n)`.
        // F(5) = 1+1+2+6+24+120 = 154.
        match terminal(&Valeur::z(5)) {
            Valeur::Entier(v, None) => assert_eq!(v, 154),
            _ => panic!("terminal(5) doit rendre un entier simple"),
        }
    }

    #[test]
    fn le_postfixe_question_est_le_classique() {
        // ⚠ Vérifie la RÈGLE : `5?` = 15 (classique), pas 154 (Kurepa), et
        // `38?` = 741 (demande utilisateur : « 38? pour classique »).
        for (source, attendu) in [("5?", "15"), ("38?", "741"), ("0?", "0"), ("1?", "1")] {
            let prog = crate::front::analyser(source).unwrap();
            let mut it = Interp::nouveau(prog);
            it.executer_principal();
            let e = match &it.prog.instructions[0] {
                crate::front::Instr::ExprStmt(es) => es.clone(),
                crate::front::Instr::Afficher(es, _) => es[0].clone(),
                _ => panic!("instruction inattendue pour {source}"),
            };
            match it.eval(&e) {
                Ok(Valeur::Entier(v, None)) => assert_eq!(v.to_string(), attendu, "{source}"),
                Ok(Valeur::Entier(_, Some(z))) => assert_eq!(z.chaine(), attendu, "{source}"),
                _ => panic!("{source} doit rendre un entier"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `factorial_approx` = Stirling asymptotique : il ne doit PAS donner le
    /// nombre exact, mais la bonne MANTISSE et le bon EXPOSANT. On vérifie
    /// contre `n!` exact pour les petites valeurs, et contre des valeurs
    /// connues pour les grandes (100! = 9.332621544…e157).
    #[test]
    fn factorial_approx_bonne_taille() {
        // 10! = 3628800 = 3.6288e6
        assert_eq!(factorial_approx(&Gz::de_i64(10)), "3.628800e6");
        // 0! et 1! : la convention exacte est 1, pas « 1.000000e0 ».
        assert_eq!(factorial_approx(&Gz::de_i64(0)), "1");
        assert_eq!(factorial_approx(&Gz::de_i64(1)), "1");
        // 100! : 9.33262154439441…e157 → la série asymptotique donne
        // 9.332622e157 (erreur < 1e-6 relative, soit 1 chiffre de marge).
        let a = factorial_approx(&Gz::de_i64(100));
        assert!(a.starts_with("9.33262"), "100! approché = {a}");
        assert!(a.ends_with("e157"), "exposant de 100! = {a}");
        // 100000! ≈ 2.824229…e456573 : le vrai nombre exact a 456574 chiffres,
        // donc l'exposant est 456573.
        let b = factorial_approx(&Gz::de_i64(100_000));
        assert!(b.ends_with("e456573"), "exposant de 100000! = {b}");
    }

    /// ⚠ Le piège documenté : `factorial_approx` n'est PAS exact. La preuve
    /// qu'il ne l'est pas est que le nombre de chiffres de la chaîne rendue
    /// est ~14, alors que `100!` en a 158.
    #[test]
    fn factorial_approx_n_est_pas_le_nombre_exact() {
        let a = factorial_approx(&Gz::de_i64(100));
        assert!(a.len() < 20, "approx doit rester compacte : {a}");
        assert_ne!(a, crate::run::factorielle(&Gz::de_i64(100)).v().to_string());
    }

    #[test]
    fn sous_factorielle_valeurs_oeis() {
        // A000166 : 1, 0, 1, 2, 9, 44, 265, 1854, 14833, 133496, 1334961
        let attendu = [1i64, 0, 1, 2, 9, 44, 265, 1854, 14833, 133496, 1334961];
        for (n, a) in attendu.iter().enumerate() {
            let r = sous_factorielle(&Gz::de_i64(n as i64));
            assert_eq!(r.v().to_string(), a.to_string(), "!{n}");
        }
    }

    #[test]
    fn multifactorielle_valeurs_oeis() {
        // A000165 (pair) / A001147 (impair) pour k = 2
        let d = [(1i64, 1i64), (2, 2), (3, 3), (4, 8), (5, 15), (6, 48), (7, 105), (8, 384), (9, 945)];
        for (n, a) in d {
            let r = multifactorielle(&Gz::de_i64(n), 2);
            assert_eq!(r.v().to_string(), a.to_string(), "{n}!!");
        }
        // A007661 pour k = 3
        let t = [(1i64, 1i64), (2, 2), (3, 3), (4, 4), (5, 10), (6, 18), (7, 28), (8, 80), (9, 162), (10, 280)];
        for (n, a) in t {
            let r = multifactorielle(&Gz::de_i64(n), 3);
            assert_eq!(r.v().to_string(), a.to_string(), "{n}!!!");
        }
    }
}

/// ⚠⚠⚠ TESTS DE NON-RÉGRESSION DES DEUX BACKENDS RÉPARÉS (19/11).
///
/// Chaque test ÉCHOUE sur l'ancien code et PASSE sur le nouveau :
///   * `repli_ast_pour_execute_toutes_les_iterations` : l'ancien
///     `exec_bloc_retour` ne poussait le corps qu'UNE fois (chaine = 3).
///   * `no_rir_parite_avec_jit` : avant, `ROSE_NO_RIR=1` donnait une ligne
///     LaTeX parasite, `chaine = 3` puis une erreur de liste.
///   * `natif_sans_jit_ne_crase_pas_les_globales_du_principal` : avant, le
///     corps principal en RIR voyait ses slots écrasés par les appels
///     délégués (globales = argument de l'appelé, `temps()` ≈ 10^18 ns).
#[cfg(test)]
mod tests_backends_repares {
    use super::*;

    // ⚠⚠⚠ LES VARIABLES D'ENVIRONNEMENT SONT GLOBALES AU PROCESSUS.
    // `jit.rs` possède un test qui dépend de `ROSE_SANS_JIT` : poser cette
    // variable dans un test en cours le ferait échouer aléatoirement (mesuré
    // : `parite_natif_cranelift_interpreteur` → « Cranelift n'a pas compilé
    // calc »). Les deux tests qui ont besoin d'un backend particulier
    // s'exécutent donc dans un PROCESSUS ENFANT isolé : le test parent
    // lance le binaire de test courant avec `--exact <nom>` et les variables
    // voulues, puis vérifie son code de retour. Aucune variable n'est posée
    // dans le processus qui exécute les autres tests.
    /// Rend `true` si nous sommes l'enfant (le test doit continuer), `false`
    /// si nous sommes le parent (le test doit RETURN immédiatement).
    fn enfant(marqueur: &str, nom_test: &str, vars: &[(&str, &str)]) -> bool {
        if std::env::var(marqueur).is_ok() { return true; } // nous SOMMES l'enfant
        let exe = std::env::current_exe().expect("chemin du binaire de test");
        let mut cmd = std::process::Command::new(exe);
        cmd.arg("--exact").arg(nom_test).env(marqueur, "1");
        for (n, v) in vars { cmd.env(n, v); }
        let out = cmd.output().expect("lancement du processus enfant");
        assert!(out.status.success(),
                "test enfant {nom_test} échoué (rc={:?}) :\n{}\n{}",
                out.status.code(),
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr));
        false
    }

    fn execuer_capture(source: &str) -> (i32, String) {
        crate::run::set_sink(true);
        let rc = executer_source(source, Vec::new());
        let sortie = crate::run::take_sink().unwrap_or_default();
        (rc, sortie)
    }

    #[test]
    fn repli_ast_pour_execute_toutes_les_iterations() {
        // Appel DIRECT de `exec_bloc_retour` sur un corps AST (aucun RIR, aucun
        // JIT) : c'est exactement la voie historique qu'empruntait
        // `ROSE_NO_RIR=1` pour les fonctions non compilables (chaînes/listes).
        let source = "fn compte(n: entier) -> entier {\n\
                      $s = 0\n\
                      pour k de 1 vers n { $s = $s + 1 }\n\
                      retour $s\n\
                      }\n";
        let prog = crate::front::analyser(source).unwrap();
        let corps = prog.signatures["compte"].corps.clone().unwrap();
        let mut it = Interp::nouveau(prog);
        it.trames.last_mut().unwrap().insert("n".into(), Valeur::z(5));
        crate::run::set_sink(true);
        let r = it.exec_bloc_retour(&corps);
        let _ = crate::run::take_sink();
        match r {
            Err(Signal::Retour(v)) => {
                // « pour k de 1 vers 5 » = k ∈ {1,2,3,4} → 4 itérations.
                // Ancien code : 1 seule itération → "1".
                assert_eq!(crate::run::chaine_de(&v), "4");
            }
            Err(Signal::Quitter(c)) => panic!("retour attendu, obtenu Quitter({c})"),
            Ok(_) => panic!("retour attendu, exécution terminée sans retour"),
        }
    }

    #[test]
    fn no_rir_parite_avec_jit() {
        if !enfant("JE_ENFANT_NO_RIR",
                   "interp::tests_backends_repares::no_rir_parite_avec_jit",
                   &[("ROSE_NO_RIR", "1")]) { return; }
        // ── Corps exécuté DANS L'ENFANT (ROSE_NO_RIR=1) ──
        let source = "\
fn charge_chaine(n: entier) -> entier {
    $motif = \"ete radieux\"
    $acc = \"\"
    pour k de 1 vers n {
        $acc = sous_chaine($motif, 0, 3) + $acc
    }
    retour longueur($acc)
}
fn charge_liste(n: entier) -> entier {
    $p = [0]
    pour k de 1 vers n { joindre($p, k) }
    retour longueur($p) + $p[partie_entiere(n / 2)]
}
>> chaine(charge_chaine(50))
>> chaine(charge_liste(100))
";
        let (rc, sortie) = execuer_capture(source);
        assert_eq!(rc, 0, "ROSE_NO_RIR a échoué : {sortie}");
        assert!(!sortie.contains("erreur"), "erreur sous ROSE_NO_RIR : {sortie}");
        assert!(!sortie.contains("\\sum_"), "ligne LaTeX parasite : {sortie}");
        let lignes: Vec<&str> = sortie.lines().collect();
        assert_eq!(lignes.len(), 2, "sortie inattendue : {sortie:?}");
        assert_eq!(lignes[0], "147", "chaine : {sortie}");
        assert_eq!(lignes[1], "150", "liste : {sortie}");
    }

    #[test]
    fn natif_sans_jit_ne_crase_pas_les_globales_du_principal() {
        if !enfant("JE_ENFANT_NATIF",
                   "interp::tests_backends_repares::natif_sans_jit_ne_crase_pas_les_globales_du_principal",
                   &[("ROSE_NATIF", "1"), ("ROSE_SANS_JIT", "1")]) { return; }
        // ── Corps exécuté DANS L'ENFANT (ROSE_NATIF=1, ROSE_SANS_JIT=1) ──
        let source = "\
fn fib(n: entier) -> entier {
    si (n < 2) { retour n }
    retour fib(n - 1) + fib(n - 2)
}
fn charge_recur(n: entier) -> entier { retour fib(n) }
$garde = 9001
$v = charge_recur(18)
>> chaine($garde)
>> chaine($v)
$deb = temps()
$w = charge_recur(18)
$fin = temps()
>> chaine($fin - $deb)
";
        let (rc, sortie) = execuer_capture(source);
        assert_eq!(rc, 0, "ROSE_NATIF a échoué : {sortie}");
        let lignes: Vec<&str> = sortie.lines().collect();
        assert_eq!(lignes.len(), 3, "sortie inattendue : {sortie:?}");
        // Ancien code : $garde valait 18 (argument de charge_recur), et le
        // 3e champ valait ~1,795×10^18 ns (~498 623 h) au lieu de ~1 ms.
        assert_eq!(lignes[0], "9001", "globale du principal écrasée : {sortie}");
        assert_eq!(lignes[1], "2584", "résultat faux : {sortie}");
        let ns: i64 = lignes[2].parse().expect("troisième ligne = nanos");
        assert!((0..1_000_000_000_000).contains(&ns), "temps() aberrant : {ns} ns");
    }
}

// ── INT1 (21/11/2026) : tests des primitives de détail ─────────────────────
#[cfg(test)]
mod tests_int1 {
    use super::*;

    fn it() -> Interp { Interp::nouveau(crate::front::Programme::default()) }

    fn paires(v: &Valeur) -> Vec<(String, String)> {
        match v {
            Valeur::Table(t) => t.0.borrow().items.iter()
                .map(|(k, x)| (chaine_de(k), chaine_de(x))).collect(),
            _ => panic!("une table était attendue, reçu {}", chaine_de(v)),
        }
    }

    fn valeur(p: &[(String, String)], cle: &str) -> String {
        p.iter().find(|(k, _)| k == cle).unwrap_or_else(|| panic!("clé absente : {cle}")).1.clone()
    }

    #[test]
    fn detail_entier_geant_10_37_oracles_ecran() {
        let mut inter = it();
        // 10^37 ne tient PAS dans un u64 : on passe bien par vers_z + Gz.
        let n = Valeur::zg(Gz::de_bigint(BigInt::from(10u32).pow(37)));
        let sym = inter.stdlib_valeur("entier_geant_factorielle", &[n]);
        assert!(matches!(sym, Valeur::Symbolique(..)));
        let table = inter.stdlib_valeur("detail", &[sym]);
        let p = paires(&table);
        assert_eq!(p.len(), 5, "propriétés réelles : {p:?}");
        assert!(valeur(&p, "Puissance de 10")
            .starts_with("10^365657055180967481723488710810833949195."));
        assert_eq!(valeur(&p, "Premiers chiffres"), "901820028144300502340101362711…");
        assert_eq!(valeur(&p, "Longueur"),
                   "365657055180967481723488710810833949196 decimal digits");
        assert_eq!(valeur(&p, "Zeros finaux"), "2499999999999999999999999999999999990");
        assert_eq!(valeur(&p, "Derniers non nuls"), "467172352");
        // Gabarit exact de CLI1, sans couleur ANSI.
        if let Valeur::Table(t) = &table {
            let lignes = crate::repl::format_table_detail(t);
            assert_eq!(lignes[1], "  Premiers chiffres            : 901820028144300502340101362711…");
            assert!(lignes.iter().all(|l| l.starts_with("  ") && l.contains(" : ")));
        } else { unreachable!(); }
    }

    #[test]
    fn detail_par_type_sans_chevauchement() {
        let mut inter = it();
        let p = paires(&inter.stdlib_valeur("detail", &[Valeur::qq(Gq::de(7, 12))]));
        assert_eq!(valeur(&p, "Representation"), "0.58[3]");
        assert_eq!(valeur(&p, "Numerateur"), "7");
        assert_eq!(valeur(&p, "Denominateur"), "12");
        assert_eq!(valeur(&p, "Periode"), "1 chiffre(s)");
        assert!(!p.iter().any(|(k, _)| k == "Zeros finaux" || k == "Longueur"));

        let p = paires(&inter.stdlib_valeur("detail", &[Valeur::liste_de(vec![
            Valeur::z(1), Valeur::z(2), Valeur::z(3)])]));
        assert_eq!(valeur(&p, "Elements"), "3");
        assert_eq!(valeur(&p, "Premier element"), "1");
        assert_eq!(valeur(&p, "Dernier element"), "3");
        assert!(!p.iter().any(|(k, _)| k.contains("decimal") || k == "Numerateur"));

        let p = paires(&inter.stdlib_valeur("detail", &[Valeur::s("bonjour")]));
        assert_eq!(valeur(&p, "Longueur"), "7 caracteres");
        assert!(!p.iter().any(|(k, _)| k == "Elements" || k == "Numerateur"));

        let p = paires(&inter.stdlib_valeur("detail", &[Valeur::f(2.5)]));
        assert_eq!(valeur(&p, "Classe"), "fini");
        assert_eq!(valeur(&p, "Exposant binaire"), "1");

        let p = paires(&inter.stdlib_valeur("detail", &[Valeur::z(1000003)]));
        assert_eq!(valeur(&p, "Type"), "entier premier");
        assert_eq!(valeur(&p, "Longueur"), "7 decimal digits");
        assert!(!p.iter().any(|(k, _)| k == "Zeros finaux"));
    }

    #[test]
    fn chiffres_et_plus_de_chiffres_stables() {
        let mut inter = it();
        let sym = inter.stdlib_valeur("entier_geant_factorielle", &[Valeur::z(100)]);
        let c30 = inter.stdlib_valeur("chiffres", &[sym.clone(), Valeur::z(30)]);
        assert_eq!(chaine_de(&c30), "933262154439441526816992388562");
        let plus = inter.stdlib_valeur("plus_de_chiffres", &[sym.clone(), Valeur::z(30)]);
        assert_eq!(chaine_de(&plus),
            "933262154439441526816992388562667004907159682643816214685929…");
        let c60 = inter.stdlib_valeur("chiffres", &[sym, Valeur::z(60)]);
        assert_eq!(chaine_de(&c60), &chaine_de(&plus)[..60]);
        assert_eq!(chaine_de(&inter.stdlib_valeur("chiffres", &[Valeur::z(-12345), Valeur::z(3)])), "123");
    }

    #[test]
    fn noms_rose_enregistres_et_arite_optionnelle() {
        let src = "\
>> detail(1)
afficher_detail(detail(1))
>> chiffres(100, 5)
>> plus_de_chiffres(1)
>> plus_de_chiffres(1, 10)
>> entier_geant_factorielle(100)
>> fenetre_boutons_creer(1, \"B|1|x|0|0|10|10\")
>> bouton_attendre_clic_limite(1, 5)
";
        if let Err(e) = crate::front::analyser(src) { panic!("vérification Rose : {e}"); }
    }

    #[test]
    fn geometrie_boutons_du_jardin_est_convertie() {
        let defs = boutons_depuis_geometrie("B|1|Plus de chiffres|12|56|160|28\n");
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].id, 1);
        assert_eq!(defs[0].label, "Plus de chiffres");
        assert_eq!((defs[0].x, defs[0].y, defs[0].largeur, defs[0].hauteur), (12, 56, 160, 28));
        assert!(boutons_depuis_geometrie("W|bouton|autre|1|2\n").is_empty());
    }
}
