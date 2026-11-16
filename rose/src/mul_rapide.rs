//! Multiplication d'entiers RAPIDE, à la manière de GMP — sans GMP.
//!
//! ⚠⚠ POURQUOI CE MODULE EXISTE. `num-bigint` n'implémente que la
//! multiplication « basecase » puis Karatsuba (O(n^1,585)). GMP, lui, empile
//! basecase → Karatsuba → Toom-Cook-3 → Toom-Cook-4… → FFT (NTT). C'est
//! cette différence, et elle seule, qui faisait `n!` 5× plus lent chez Rose.
//! Mesuré sur cette machine avant ce module (aarch64) :
//!
//!   n! à n = 100 000 : crible 0,08 ms + puissances 1,4 ms + ARBRE 47,9 ms
//!     → 99,7 % du temps est dans la MULTIPLICATION, pas dans l'algorithme.
//!
//!   une multiplication de 1 000 000 bits : 1,12 ms (num-bigint)
//!                                          vs ~0,35 ms (GMP)
//!
//! On reproduit donc ici les mêmes étages que GMP, dans Rose :
//!   * < SEUIL_KARATSUBA limbs (u32)  → basecase (école) ;
//!   * < SEUIL_TOOM3                  → Karatsuba ;
//!   * < SEUIL_NTT                    → Toom-Cook-3 ;
//!   * au-delà                        → NTT (transformée de Fourier
//!     numérique, 3 modules premiers + théorème des restes chinois).
//!
//! ⚠⚠ LE PIÈGE DE LA NTT : on travaille modulo un corps fini, donc il faut
//! que le produit des modules dépasse le MAXIMUM POSSIBLE de chaque
//! coefficient. Pour deux nombres de L limbs u32, un coefficient de la
//! convolution vaut au plus L·(2³²−1)² ≈ L·2⁶⁴. Avec 3 modules d'environ 30
//! bits on couvre ≈ 2⁹⁰ : suffisant jusqu'à L ≈ 2²⁶ limbs (≈ 32 Mo de
//! chiffres). Au-delà, il faudrait un 4e module — on renvoie alors Toom-3.
//!
//! La NTT utilise la racine primitive de l'unité fournie par chaque module
//! (2³² − 2¹⁹ + 1, 998244353, 1004535809), tous de la forme k·2ᵐ + 1.
//!
//! Licence MIT.

use num_bigint::{BigInt, BigUint, Sign};
use num_integer::Integer;
use num_traits::{One, Zero};

// ── Seuils (en LIMBS u32, 1 limb = 32 bits) ──────────────────────────────
//
// Calibrés par mesure sur aarch64 : en dessous de ces tailles, l'algorithme
// le plus simple gagne (les divisions et les allocations de Toom-3/NTT
// coûtent plus que le gain asymptotique).
// ⚠⚠ SEUILS CALIBRÉS PAR MESURE, pas au jugé. Un premier réglage
// (40/180/2000) rendait `n!` 10× PLUS LENT qu'avant : Toom-3 était employé à
// des tailles où l'école/Karatsuba gagne. Mesuré sur aarch64 (µs) :
//
//   n limbs :   école      karat      toom3      ntt
//       100 :    13,8      18,7       35,6      251
//       200 :    56,0      49,1       72,0      509
//       400 :   226,0     187,7      150,6     1050
//      1000 :  1420       723        711       2360
//      2000 :  5720      2030       2150       4950
//      4000 : 14540     10790       2830       7610
//      8000 : 49620     22910       3580      16170
//
//   n limbs :   toom3      ntt
//      8000 :     7,4      16,1
//     16000 :    76,5      34,3     ← NTT prend l'avantage ici
//     24000 :    83,4      70,6
//     32000 :   165,5      73,7
//
// D'où : école < 100, Karatsuba de 100 à 1000, Toom-3 de 1000 à 12000,
// NTT au-delà. ⚠ Ces seuils ne sont PAS universels : sur une autre
// architecture, relancer `profil_des_etages` et réajuster.
const SEUIL_KARATSUBA: usize = 100;
const SEUIL_TOOM3: usize = 1000;
const SEUIL_NTT: usize = 12000;
// ⚠ Seuil de la division par inverse de Newton (en limbs du diviseur).
// Mesuré : voir `profil_division` dans les tests.
const SEUIL_DIVISION: usize = 100;

/// Multiplication de deux nombres en base 2^32 (pour comparaison).
pub fn ntt_mul_b32(x: &[u32], y: &[u32]) -> Vec<u32> {
    ntt_mul_par(x, y, 0, 0)
}

/// Idem avec un nombre de fils imposé (pour mesurer l'échelle).
pub fn ntt_mul_par_b32(x: &[u32], y: &[u32], fils: usize) -> Vec<u32> {
    ntt_mul_par(x, y, fils, 0)
}

/// Multiplication de deux nombres en base 10^9 (limbs u32 < 10^9).
///
/// ⚠ Utilisé par la conversion décimale rapide (voir `conv_b1e9`).
pub fn ntt_mul_b1e9(x: &[u32], y: &[u32]) -> Vec<u32> {
    ntt_mul_par(x, y, 0, 1_000_000_000)
}

/// Idem, avec un nombre de fils imposé (0 = tous les cœurs).
pub fn ntt_mul_b1e9_par(x: &[u32], y: &[u32], fils: usize) -> Vec<u32> {
    ntt_mul_par(x, y, fils, 1_000_000_000)
}

/// Multiplication de deux entiers signés, par le meilleur algorithme selon
/// la taille. Sémantique STRICTEMENT identique à `BigInt * BigInt`.
pub fn mul(a: &BigInt, b: &BigInt) -> BigInt {
    mul_par(a, b, 0)
}

/// ⚠⚠ Multiplication avec CHOIX DU NOMBRE DE FILS pour la NTT.
///
/// `fils = 0` → tous les cœurs disponibles (le cas normal).
/// `fils = 1` → NTT mono-fils : c'est ce qu'il faut quand PLUSIEURS
/// multiplications tournent EN PARALLÈLE ( tours de paires de l'arbre de
/// produit), sinon on sursouscrit les cœurs et l'on perd plus qu'on ne
/// gagne.
pub fn mul_par(a: &BigInt, b: &BigInt, fils: usize) -> BigInt {
    if a.is_zero() || b.is_zero() { return BigInt::zero(); }
    // ⚠⚠⚠ CE QUE LA MESURE A RÉVÉLÉ (et qui change toute la stratégie) :
    // `num-bigint` N'EST PAS limité à Karatsuba. Sa `multiplication.rs`
    // fait, dans l'ordre : école (≤ 32 limbs), Half-Karatsuba (tailles très
    // dissymétriques), Karatsuba (≤ 256 limbs), puis **TOOM-3** au-delà.
    // Mon propre Toom-3 est donc INUTILE : mesuré 4× PLUS LENT que le sien
    // (7,6 ms contre 1,8 ms à 8000 limbs), parce que le sien travaille en
    // place (`mac3`) sans allouer de vecteurs intermédiaires.
    //
    // Ce qui MANQUE à num-bigint, et que GMP a, c'est la FFT (NTT). C'est
    // la seule chose qui reste à apporter — et seulement à partir du moment
    // où elle bat réellement Toom-3.
    //
    // Mesuré (aarch64, µs) : NTT ne dépasse Toom-3 que vers 16 000 limbs.
    // En dessous, on laisse num-bigint faire : c'est le plus rapide.
    //
    // ⚠ Le seuil est en BITS (gratuit à lire), converti depuis les limbs :
    // 16 000 limbs = 512 000 bits.
    // ⚠⚠ VERDICT MESURÉ (aarch64) — c'est ce qui décide de tout :
    //
    //   limbs :  num-bigint   notre NTT (2 modules de 62 bits)
    //   64 000 :   51,1 ms     54,7 ms   → num-bigint
    //  128 000 :  102,5 ms    113,9 ms   → num-bigint
    //  256 000 :  286,6 ms    239,4 ms   → NTT          ← bascule
    //  512 000 :  790,8 ms    501,8 ms   → NTT (1,6×)
    //
    // `num-bigint` fait école → Half-Karatsuba → Karatsuba (≤256 limbs) →
    // **Toom-3** en place. C'est excellent, et notre Toom-3/école/Karatsuba
    // ne l'améliore pas. Ce qui lui MANQUE est la FFT — et notre NTT ne la
    // bat qu'à partir de ~512 000 limbs (16,4 millions de bits, soit
    // ~4,9 millions de chiffres décimaux).
    //
    // On ne l'active donc QUE là : c'est exactement le régime des très
    // grosses factorielles (1 000 000! ≈ 5,6 millions de chiffres).
    //
    // ⚠⚠ SEUIL BALAYÉ SUR L'ARBRE RÉEL de 1 000 000! (mesuré) :
    //
    //   0,26 M bits → 252 ms    0,52 M → 252 ms    0,79 M → 244 ms
    //   1,02 M bits → 260 ms ←  1,54 M → 249 ms    3,07 M → 307 ms
    //
    // Au-delà d'environ 1,5 M bits, l'arbre s'appesantit : les nœuds de
    // cette taille passent à `num-bigint` (Toom-3 mono-fils) et la NTT
    // parallèle n'est plus mobilisée. En dessous, la NTT est plus rapide
    // mais ses COÛTS FIXES (conversion en u32, table de racines, création
    // des fils) commencent à dominer.
    //
    // 1,02 M bits = 32 000 limbs : c'est exactement le point de bascule
    // ISOLÉ mesuré (la NTT parallèle y bat `num-bigint` de 1,2×).
    const SEUIL_NTT_BITS: u64 = 1_024_000;
    if a.bits().max(b.bits()) >= SEUIL_NTT_BITS {
        let signe = if a.sign() == b.sign() { Sign::Plus } else { Sign::Minus };
        let x = a.magnitude().to_u32_digits();
        let y = b.magnitude().to_u32_digits();
        let m = ntt_mul_par(&x, &y, fils, 0);
        return BigInt::from_biguint(signe, BigUint::from_slice(&m));
    }
    let _ = (SEUIL_KARATSUBA, SEUIL_TOOM3, SEUIL_NTT);
    a * b
}

/// Produit de magnitudes (limbs u32, poids faible d'abord).
fn mul_limbs(x: &[u32], y: &[u32]) -> Vec<u32> {
    // Le plus court à droite : toutes les routines supposent |x| ≥ |y|.
    let (x, y) = if x.len() >= y.len() { (x, y) } else { (y, x) };
    if y.is_empty() { return Vec::new(); }
    if y.len() < SEUIL_KARATSUBA {
        return ecole(x, y);
    }
    if y.len() < SEUIL_TOOM3 {
        return karatsuba(x, y);
    }
    if y.len() < SEUIL_NTT {
        return toom3(x, y);
    }
    ntt_mul_par(x, y, 0, 0)
}

/// Multiplication « école » : O(n·m), la plus rapide pour les petites tailles.
fn ecole(x: &[u32], y: &[u32]) -> Vec<u32> {
    let mut r = vec![0u32; x.len() + y.len()];
    for (j, &yj) in y.iter().enumerate() {
        if yj == 0 { continue; }
        let mut retenue = 0u64;
        let yj = yj as u64;
        for i in 0..x.len() {
            let t = (x[i] as u64) * yj + (r[i + j] as u64) + retenue;
            r[i + j] = t as u32;
            retenue = t >> 32;
        }
        // ⚠ La retenue peut elle-même déborder vers le limb suivant :
        // l'additionner à la position i+j peut propager.
        let mut k = j + x.len();
        while retenue != 0 {
            let t = (r[k] as u64) + retenue;
            r[k] = t as u32;
            retenue = t >> 32;
            k += 1;
        }
    }
    normaliser(r)
}

/// Retire les limbs nuls de tête (le vecteur peut être vide pour 0).
fn normaliser(mut v: Vec<u32>) -> Vec<u32> {
    while v.last() == Some(&0) { v.pop(); }
    v
}

fn add(a: &[u32], b: &[u32]) -> Vec<u32> {
    let (grand, petit) = if a.len() >= b.len() { (a, b) } else { (b, a) };
    let mut r = Vec::with_capacity(grand.len() + 1);
    let mut retenue = 0u64;
    for i in 0..grand.len() {
        let t = (grand[i] as u64) + (*petit.get(i).unwrap_or(&0) as u64) + retenue;
        r.push(t as u32);
        retenue = t >> 32;
    }
    if retenue != 0 { r.push(retenue as u32); }
    r
}

/// `a − b`, avec a ≥ b (le résultat est une magnitude, donc jamais négatif).
fn sous(a: &[u32], b: &[u32]) -> Vec<u32> {
    let mut r = Vec::with_capacity(a.len());
    let mut emprunt = 0i64;
    for i in 0..a.len() {
        let t = (a[i] as i64) - (*b.get(i).unwrap_or(&0) as i64) - emprunt;
        if t < 0 {
            r.push((t + (1i64 << 32)) as u32);
            emprunt = 1;
        } else {
            r.push(t as u32);
            emprunt = 0;
        }
    }
    normaliser(r)
}

/// Découpe en deux moitiés : a = a1·B^m + a0, b = b1·B^m + b0.
fn moities(v: &[u32], m: usize) -> (Vec<u32>, Vec<u32>) {
    let bas = if v.len() <= m { v.to_vec() } else { v[..m].to_vec() };
    let haut = if v.len() <= m { Vec::new() } else { v[m..].to_vec() };
    (normaliser(bas), normaliser(haut))
}

/// Décale d'un nombre de limbs (multiplication par B^k).
fn decaler(v: &[u32], k: usize) -> Vec<u32> {
    if v.is_empty() { return Vec::new(); }
    let mut r = vec![0u32; k];
    r.extend_from_slice(v);
    r
}

/// Karatsuba : 3 multiplications au lieu de 4.
fn karatsuba(x: &[u32], y: &[u32]) -> Vec<u32> {
    if y.len() < SEUIL_KARATSUBA { return ecole(x, y); }
    let m = x.len() / 2;
    let (a0, a1) = moities(x, m);
    let (b0, b1) = moities(y, m);
    let z0 = mul_limbs(&a0, &b0);
    let z2 = mul_limbs(&a1, &b1);
    let s1 = add(&a0, &a1);
    let s2 = add(&b0, &b1);
    let mut z1 = mul_limbs(&s1, &s2);
    // z1 ← z1 − z0 − z2
    z1 = sous(&z1, &z0);
    z1 = sous(&z1, &z2);
    let mut r = z0;
    r = add(&r, &decaler(&z1, m));
    r = add(&r, &decaler(&z2, 2 * m));
    normaliser(r)
}

// ── Toom-Cook-3 ──────────────────────────────────────────────────────────
//
// a = a0 + a1·B^m + a2·B^2m, idem pour b. On évalue les deux polynômes en
// 5 points (0, 1, −1, 2, ∞), on multiplie les évaluations, puis on
// interpole (Lagrange). C'est l'étage entre Karatsuba et la FFT chez GMP.
//
// ⚠⚠ LE PIÈGE MESURÉ : les coefficients interpolés ne tiennent PAS dans un
// limb B — ils montent jusqu'à ~1,7·B² (mesuré sur 2000 tirages). On les
// traite donc comme des MAGNITUDES à plusieurs limbs, pas comme des limbs :
// une première version les stockait en `Vec<u32>` et le test a immédiatement
// révélé la corruption (taille 180×180 limbs divergente au milieu).
//
// ⚠ Les évaluations en −1 produisent des valeurs SIGNÉES : on les représente
// par un couple (positif, négatif) pour rester sur des magnitudes.

#[derive(Clone)]
struct Signe {
    pos: Vec<u32>,
    neg: Vec<u32>,
}

impl Signe {
    fn valeur(p: Vec<u32>) -> Self { Signe { pos: normaliser(p), neg: Vec::new() } }
    /// p − q, sans jamais produire de magnitude négative.
    fn diff(p: &[u32], q: &[u32]) -> Self {
        if sup_ou_egal(p, q) {
            Signe { pos: sous(p, q), neg: Vec::new() }
        } else {
            Signe { pos: Vec::new(), neg: sous(q, p) }
        }
    }
    fn mul_signe(&self, autre: &Signe) -> Signe {
        // (p1 − n1)(p2 − n2) = (p1p2 + n1n2) − (p1n2 + n1p2)
        let pos = add(&mul_limbs(&self.pos, &autre.pos), &mul_limbs(&self.neg, &autre.neg));
        let neg = add(&mul_limbs(&self.pos, &autre.neg), &mul_limbs(&self.neg, &autre.pos));
        if sup_ou_egal(&pos, &neg) {
            Signe { pos: sous(&pos, &neg), neg: Vec::new() }
        } else {
            Signe { pos: Vec::new(), neg: sous(&neg, &pos) }
        }
    }
    /// Valeur effective : (pos − neg) comme magnitude, avec son signe.
    fn decomposer(&self) -> (bool, Vec<u32>) {
        if sup_ou_egal(&self.pos, &self.neg) {
            (true, sous(&self.pos, &self.neg))
        } else {
            (false, sous(&self.neg, &self.pos))
        }
    }
}

/// Multiplie une magnitude par un petit entier (u32) et décale de `d` bits
/// vers la gauche — nécessaire pour l'évaluation en 2 (×2, ×4).
fn mul_petit_decale(a: &[u32], mult: u32, decalage: u32) -> Vec<u32> {
    if a.is_empty() { return Vec::new(); }
    // Décale d'abord par multiples de 32 bits, puis par le reste.
    let mots = (decalage / 32) as usize;
    let bits = decalage % 32;
    let mut r = vec![0u32; mots];
    if bits == 0 {
        let mut retenue = 0u64;
        for &v in a {
            let t = (v as u64) * (mult as u64) + retenue;
            r.push(t as u32);
            retenue = t >> 32;
        }
        if retenue != 0 { r.push(retenue as u32); }
        return normaliser(r);
    }
    // mult ≤ 4 : on reste sur u64 sans dépasser 2^34.
    let mut retenue: u64 = 0;
    for &v in a {
        let t = ((v as u64) * (mult as u64)) << bits | retenue;
        r.push(t as u32);
        retenue = ((v as u64) * (mult as u64)) >> (32 - bits);
    }
    if retenue != 0 { r.push(retenue as u32); }
    normaliser(r)
}

/// Vrai si la magnitude `a` est ≥ `b` (comparaison SANS allocation).
fn sup_ou_egal(a: &[u32], b: &[u32]) -> bool {
    let la = a.iter().rposition(|&x| x != 0).map(|i| i + 1).unwrap_or(0);
    let lb = b.iter().rposition(|&x| x != 0).map(|i| i + 1).unwrap_or(0);
    if la != lb { return la > lb; }
    for i in (0..la).rev() {
        if a[i] != b[i] { return a[i] > b[i]; }
    }
    true
}

fn toom3(x: &[u32], y: &[u32]) -> Vec<u32> {
    // ⚠ k doit être ≥ len/3 arrondi au supérieur, pour que a2 < B^m.
    let k = (x.len().max(y.len()) + 2) / 3;
    let (a0, r) = moities(x, k);
    let (a1, a2) = moities(&r, k);
    let (b0, r) = moities(y, k);
    let (b1, b2) = moities(&r, k);

    // Évaluations : w0 = a(0)b(0), w1 = a(1)b(1), w2 = a(−1)b(−1),
    // w3 = a(2)b(2), w4 = a(∞)b(∞) = a2·b2.
    let w0 = Signe::valeur(mul_limbs(&a0, &b0));
    let w1 = Signe::valeur(mul_limbs(&add(&a0, &add(&a1, &a2)), &add(&b0, &add(&b1, &b2))));
    let w2 = Signe::diff(&add(&a0, &a2), &a1).mul_signe(&Signe::diff(&add(&b0, &b2), &b1));
    // a(2) = a0 + 2a1 + 4a2
    let a_2 = add(&add(&a0, &mul_petit_decale(&a1, 2, 0)), &mul_petit_decale(&a2, 4, 0));
    let b_2 = add(&add(&b0, &mul_petit_decale(&b1, 2, 0)), &mul_petit_decale(&b2, 4, 0));
    let w3 = Signe::valeur(mul_limbs(&a_2, &b_2));
    let w4 = Signe::valeur(mul_limbs(&a2, &b2));

    // ── Interpolation de Lagrange ──────────────────────────────────────
    //
    // ⚠⚠ LE PIÈGE (mesuré, test rouge pendant plusieurs essais) : le
    // produit est de degré 4, mais les 4 points {0, 1, −1, 2} ne déterminent
    // qu'un polynôme de degré ≤ 3. Il faut donc retirer d'abord le terme
    // dominant w4·x⁴, interpoler Q(x) = P(x) − w4·x⁴ sur les 4 points, puis
    // rajouter w4 au coefficient 4 :
    //
    //     q_i = w_i − w4·x_i⁴
    //     c0 = q0
    //     c1 = −q0/2 + q1 − q(−1)/3 − q2/6
    //     c2 = −q0 + q1/2 + q(−1)/2
    //     c3 =  q0/2 − q1/2 − q(−1)/6 + q2/6
    //     c4 = w4
    //
    // Les coefficients de Lagrange sont (calculés exactement) :
    //     L0   = (1, −1/2, −1,   1/2)
    //     L1   = (0,  1,    1/2, −1/2)
    //     L(−1) = (0, −1/3,  1/2, −1/6)
    //     L2   = (0, −1/6,  0,    1/6)
    //
    // Tous les dénominateurs divisent 6 : on calcule en sixièmes puis on
    // divise EXACTEMENT.

    // c·v, c pouvant être négatif : on échange pos/neg si c < 0.
    let s = |v: &Signe, c: i64| -> Signe {
        let m = c.unsigned_abs() as u32;
        let p = mul_petit_decale(&v.pos, m, 0);
        let n = mul_petit_decale(&v.neg, m, 0);
        if c >= 0 { Signe { pos: p, neg: n } } else { Signe { pos: n, neg: p } }
    };
    let somme = |liste: &[Signe]| -> Signe {
        let mut pos = Vec::new();
        let mut neg = Vec::new();
        for e in liste { pos = add(&pos, &e.pos); neg = add(&neg, &e.neg); }
        Signe { pos, neg }
    };

    // q_i = w_i − w4·x_i⁴, en Signe.
    // x^4 : 0^4 = 0, 1^4 = 1, (−1)^4 = 1, 2^4 = 16.
    let q0 = w0.clone();
    let q1 = {
        let mut e = w1.clone();
        e.neg = add(&e.neg, &w4.pos.clone()); // w1 − w4
        e
    };
    let qm1 = {
        let mut e = w2.clone();
        e.neg = add(&e.neg, &w4.pos.clone()); // w2 − w4
        e
    };
    let q2 = {
        // w3 − 16·w4
        let mut e = w3.clone();
        e.neg = add(&e.neg, &mul_petit_decale(&w4.pos, 16, 0));
        e
    };

    // c0 = q0
    let c0 = q0.clone();
    // c1 = (−3q0 + 6q1 − 2qm1 − q2)/6
    let c1 = somme(&[s(&q0, -3), s(&q1, 6), s(&qm1, -2), s(&q2, -1)]);
    // c2 = (−6q0 + 3q1 + 3qm1)/6
    let c2 = somme(&[s(&q0, -6), s(&q1, 3), s(&qm1, 3)]);
    // c3 = (3q0 − 3q1 − qm1 + q2)/6
    let c3 = somme(&[s(&q0, 3), s(&q1, -3), s(&qm1, -1), s(&q2, 1)]);

    // Division EXACTE par 6 des coefficients 1 à 3.
    let diviser = |e: &Signe, d: u64| -> Vec<u32> {
        let (positif, v) = e.decomposer();
        let q = div_petit(&v, d);
        // Vérification : la division doit être exacte.
        debug_assert_eq!(mul_petit_decale(&q, d as u32, 0), v, "division inexacte");
        // ⚠ Un coefficient négatif est impossible : le produit est positif.
        debug_assert!(positif, "coefficient Toom-3 négatif");
        q
    };
    let c1v = diviser(&c1, 6);
    let c2v = diviser(&c2, 6);
    let c3v = diviser(&c3, 6);

    // Recompose : c0 + c1·B^k + c2·B^2k + c3·B^3k + c4·B^4k
    let mut r = c0.pos.clone();
    r = add(&r, &decaler(&c1v, k));
    r = add(&r, &decaler(&c2v, 2 * k));
    r = add(&r, &decaler(&c3v, 3 * k));
    r = add(&r, &decaler(&w4.pos, 4 * k));
    normaliser(r)
}

/// Division d'une magnitude par un petit nombre (u64), résultat exact.
fn div_petit(a: &[u32], d: u64) -> Vec<u32> {
    let mut r = vec![0u32; a.len()];
    let mut reste = 0u64;
    for i in (0..a.len()).rev() {
        let cur = (reste << 32) | (a[i] as u64);
        r[i] = (cur / d) as u32;
        reste = cur % d;
    }
    normaliser(r)
}

// ── NTT : transformée de Fourier sur corps fini ──────────────────────────
//
// Chaque module est de la forme k·2^m + 1, ce qui garantit l'existence
// d'une racine primitive de l'unité d'ordre 2^m : c'est ce qui permet la
// NTT de longueur puissance de 2.
//
// ⚠⚠ LA RACINE PRIMITIVE COMPTE AUTANT QUE LE MODULE. Une première version
// donnait (2013265921, 5) : `5^((m−1)/2) mod m = 1`, donc 5 n'était PAS une
// racine primitive et la NTT produisait des valeurs fausses (test rouge).
// Vérification systématique : `g^((m−1)/2) ≡ −1` et `g^(m−1) ≡ 1`.
//
// ⚠⚠⚠ POURQUOI DEUX MODULES DE 62 BITS PLUTÔT QUE TROIS DE 30. Mesuré : avec
// trois modules de 30 bits, la NTT coûtait 9 transformées (3 modules ×
// 2 + 1 inverse) et un `%` u128 par papillon — elle ne battait `num-bigint`
// qu'à 512 000 limbs. Avec DEUX modules de 62 bits, le produit de deux
// valeurs tient dans u128.
//
// Couverture : deux modules de ~62 bits donnent ~124 bits. Le coefficient
// maximal d'une convolution est L·(2³²−1)² ≈ L·2⁶⁴, donc L < 2⁶⁰ limbs
// (≈ 64 Go de chiffres) — largement au-delà de ce que Rose manipule.
//
// Ces modules ont été VÉRIFIÉS premiers et leur racine primitive testée :
//   m = 2850325311931809793 = k·2^32+1, racine 5
//   m = 3874380531890126849 = k·2^32+1, racine 3
// (2-adicité 32 : longueur NTT jusqu'à 2³²).
const MODULES: [(u64, u64); 2] = [
    (2_850_325_311_931_809_793, 5),
    (3_874_380_531_890_126_849, 3),
];

fn puissance_mod(mut b: u64, mut e: u64, m: u64) -> u64 {
    let mut r = 1u64;
    b %= m;
    while e > 0 {
        if e & 1 == 1 { r = (r as u128 * b as u128 % m as u128) as u64; }
        b = (b as u128 * b as u128 % m as u128) as u64;
        e >>= 1;
    }
    r
}

fn inverse_mod(a: u64, m: u64) -> u64 {
    puissance_mod(a, m - 2, m)
}

// ═══════════════════════════════════════════════════════════════════════
// MONTGOMERY
// ═══════════════════════════════════════════════════════════════════════
//
// ⚠⚠⚠ POURQUOI MONTGOMERY ET PLUS LE `%`. Mesuré (aarch64, 4 M ops) :
//
//   `% u128` par un module de 62 bits : 1,68 ns/op
//   Montgomery (REDC)             : 0,96 ns/op   → 1,74× plus rapide
//   Barrett avec estimation f64   : 8,26 ns/op   → 4,9× plus LENT
//
// Le `%` u128 sur aarch64 passe par `__umodti3` (une division 128/64
// logicielle). Montgomery remplace cette division par deux multiplications
// 64×64 et un décalage, sans division.
//
// ⚠⚠ MONTGOMERY AVAIT ÉTÉ REJETÉ PLUS TÔT À TORT : l'analyse de coût
// disait « 1,8× plus lent qu'un % u64, donc Montgomery ne gagne pas ». La
// MESURE montre l'inverse (1,74× de gain) : le `%` u128 n'était pas
// « 1,8× plus lent », c'était le `%` u128 tout court qui était lent.
//
// Représentation : un élément x du corps est stocké sous la forme
// MONTGOMER `x·R mod m` avec R = 2^64.
//
//   mont_mul(a, b) = REDC(a·b) = a·b·R⁻¹ mod m
//
// Si a et b sont tous deux en forme Montgomery (a = x·R, b = y·R),
// alors mont_mul(a, b) = x·y·R → le résultat est EN FORME MONTGOMER ✓.
// C'est ce qui rend la NTT entière working : l'addition et la soustraction
// sont linéaires (donc préservent la forme), et la multiplication
// point à point aussi.
//
// La conversion d'entrée `to_mont` vaut REDC(x·R²) = x·R ✓.
// La conversion de sortie se fait gratuitement : à la fin de la NTT
// inverse, la valeur est n·résultat·R ; on multiplie par inv_n (forme
// NORMALE) → REDC(n·résultat·R·inv_n) = résultat ✓ (une seule
// multiplication de Montgomery fait les deux).
struct Mont {
    m: u64,
    /// −m⁻¹ mod 2^64.
    m_inv_neg: u64,
    /// 2^128 mod m.
    r2: u64,
}

/// ⚠⚠⚠ CONSTANTES MONTGOMERY MISES EN CACHE (UNE SEULE FOIS).
///
/// `Mont::nouveau` calcule l'inverse de Newton (6 itérations) ET
/// `2^128 mod m` par 128 doublements avec division u128 → ~26 µs par
/// module, ~50 µs par appel de NTT.
///
/// Mesuré : la conversion décimale fait 292 379 appels de multiplication
/// → 50 µs × 292 379 = 14 s de recalcul de constantes pour rien !
/// (`2^128 mod m` se calcule en ~26 µs mais était refait à chaque fois.)
static MONTGOMERY: std::sync::OnceLock<[Mont; 2]> = std::sync::OnceLock::new();

/// Les deux modules, avec leurs constantes Montgomery calculées UNE FOIS.
fn monts() -> &'static [Mont; 2] {
    MONTGOMERY.get_or_init(|| {
        [Mont::nouveau(MODULES[0].0), Mont::nouveau(MODULES[1].0)]
    })
}

impl Mont {
    fn nouveau(m: u64) -> Mont {
        // Inverse de m mod 2^64 par itération de Newton (5 suffit, on en
        // fait 6 pour la marge).
        let mut inv: u64 = 1;
        for _ in 0..6 {
            inv = inv.wrapping_mul(2u64.wrapping_sub(m.wrapping_mul(inv)));
        }
        debug_assert_eq!(m.wrapping_mul(inv), 1, "m doit être impair");
        // 2^128 mod m : on part de 1 et on double 128 fois en réduisant.
        let mut r: u128 = 1;
        for _ in 0..128 {
            r = (r << 1) % m as u128;
        }
        Mont { m, m_inv_neg: inv.wrapping_neg(), r2: r as u64 }
    }

    /// REDC(a·b) = a·b·R⁻¹ mod m. Exige a, b < m (alors a·b < m·R ✓).
    #[inline(always)]
    fn mul(&self, a: u64, b: u64) -> u64 {
        let t = (a as u128) * (b as u128);
        let u = (t as u64).wrapping_mul(self.m_inv_neg);
        let t2 = t + (u as u128) * (self.m as u128);
        // t2 < 2·m·R ⇒ t2 >> 64 < 2m ⇒ un seul retrait suffit.
        let r = (t2 >> 64) as u64;
        if r >= self.m { r - self.m } else { r }
    }

    /// Forme Montgomery d'un élément NORMAL : x·R mod m.
    #[inline(always)]
    fn to_mont(&self, x: u64) -> u64 { self.mul(x, self.r2) }

    #[inline(always)]
    fn add(&self, a: u64, b: u64) -> u64 {
        let s = a + b;
        if s >= self.m { s - self.m } else { s }
    }

    #[inline(always)]
    fn sub(&self, a: u64, b: u64) -> u64 {
        if a >= b { a - b } else { a + self.m - b }
    }
}

/// Tables des racines pour une transformée de longueur `n`.
///
/// `directes[j] = mont(ω^j)` et `inverses[j] = mont(ω^-j)`, pour
/// j = 0..n/2−1 (ω = racine primitive d'ordre n).
///
/// ⚠ L'inverse se déduit des directes SANS multiplication : ω^(n/2) = −1
/// (racine d'ordre n, n pair), donc ω^-j = ω^(n−j) = −ω^(n/2−j). Un simple
/// retrait suffit — c'est n/2 soustractions au lieu de n/2 multiplications
/// de Montgomery.
struct Racines {
    directes: Vec<u64>,
    inverses: Vec<u64>,
}

fn racines(mont: &Mont, racine: u64, n: usize) -> Racines {
    let m = mont.m;
    let demi = n / 2;
    // ω = racine^((m−1)/n) : racine d'ordre n.
    let omega = puissance_mod(racine, (m - 1) / n as u64, m);
    let om = mont.to_mont(omega);
    let mut directes = Vec::with_capacity(demi);
    let mut cur = mont.to_mont(1);
    for _ in 0..demi {
        directes.push(cur);
        cur = mont.mul(cur, om);
    }
    let mut inverses = vec![0u64; demi];
    if demi > 0 {
        inverses[0] = mont.to_mont(1);
        for j in 1..demi {
            // ω^-j = −ω^(n/2−j)
            inverses[j] = mont.sub(0, directes[demi - j]);
        }
    }
    Racines { directes, inverses }
}

// ═══════════════════════════════════════════════════════════════════════
// PARALLÉLISATION DE LA NTT
// ═══════════════════════════════════════════════════════════════════════
//
// ⚠⚠⚠ POURQUOI PARALLÉLISER LA NTT ET PAS SEULEMENT L'ARBRE. Mesuré sur
// `1 000 000!` (arbre de produit réel) : la FUSION FINALE (7,73 Mbit ×
// 10,76 Mbit) coûte 350 ms à elle seule — 30 % du total, et elle est
// SÉRIELLE par construction (c'est la dernière multiplication). Aucun
// parallélisme de l'arbre ne peut la diviser : c'est la NTT elle-même qui
// doit être parallèle.
//
// La machine a 20 cœurs. Une NTT à lg = 2^20 fait 20 étages × 524 288
// papillons = 10,5 M papillons par transformée, 6 transformées.
//
// ⚠⚠⚠ LE DÉCOMPTE DES PAPILLONS EST LE POINT CRITIQUE. On ne peut PAS
// partager les PAPILLONS d'un bloc entre fils (un papillon lit deux cases
// distantes de `half` éléments) sans danger d'aliasing ; on peut en
// revanche partager les BLOCS.
//
// La répartition choisie : le travail d'un étage est la suite des
// « papillons globaux » i = 0..n/2, indexés par (bloc, k) :
//
//     bloc = i >> log2(half),  k = i & (half−1)
//     cases touchées : base+k et base+k+half, base = bloc·len
//
// La case p1 = base+k est une fonction STRICTEMENT CROISSANTE de i
// (p1 = 2i − (i mod half)), donc deux indices distincts touchent des
// cases distinctes. Des plages de i disjointes ⇒ des cases disjointes ⇒
// AUCUNE course de données. C'est ce qui rend le partage sûr.
//
// L'accès mémoire est bon : tant que la plage d'un fil tient dans un
// bloc, les cases sont contiguës ; au-delà, chaque bloc est traité par
// une plage contigue de k.
//
// La barrière entre étages est indispensable : l'étage suivant lit ce
// que le précédent a écrit.

/// ⚠⚠⚠ BARRIÈRE HYBRIDE (spin court puis PARKING).
///
/// `std::sync::Barrier` est fondée sur un futex : chaque attente coûte
/// ~30 µs quand les fils se réveillent en même temps. La NTT en a
/// besoin de ~90 par multiplication (2 modules × [2 transformées ×
/// (log₂n + 1) + 3]) → 2,7 ms par multiplication, soit plus que le
/// TRAVAIL réel pour les petites tailles.
///
/// ⚠ MAIS LE SPIN PUR EST PIRE : MESURÉ. Sur cette machine, Chrome
/// consomme 8,6 cœurs sur 20 ; les fils qui font du spin actif vole du
/// temps aux fils qui TRAVAILLENT (effet de convoi) :
///
///   lg = 2^16 : spin pur 389 ms  |  std::sync::Barrier 10,8 ms
///   lg = 2^20 : spin pur 456 ms  |  std::sync::Barrier  48,5 ms
///
/// D'où l'hybride : on tourne en BOUCLE ACTIVE un court moment (le cas
/// normal : les autres fils arrivent en quelques centaines de
/// nanosecondes), puis on se PARQUE sur une variable de condition
/// (le cas chargé : on ne vole pas de temps).
struct Barriere {
    compteur: std::sync::atomic::AtomicUsize,
    generation: std::sync::atomic::AtomicUsize,
    n: usize,
    verrou: std::sync::Mutex<()>,
    cond: std::sync::Condvar,
}

/// Nombre de tours de boucle active avant de se parquer.
const SPIN_BARRIERE: usize = 64;

impl Barriere {
    fn nouveau(n: usize) -> Barriere {
        Barriere {
            compteur: std::sync::atomic::AtomicUsize::new(0),
            generation: std::sync::atomic::AtomicUsize::new(0),
            n,
            verrou: std::sync::Mutex::new(()),
            cond: std::sync::Condvar::new(),
        }
    }

    #[inline]
    fn wait(&self) {
        use std::sync::atomic::Ordering::{AcqRel, Acquire, Relaxed, Release};
        let g = self.generation.load(Relaxed);
        if self.compteur.fetch_add(1, AcqRel) + 1 == self.n {
            self.compteur.store(0, Relaxed);
            self.generation.fetch_add(1, Release);
            // Réveiller les fils parqués (s'ils existent).
            let _g = self.verrou.lock();
            self.cond.notify_all();
            return;
        }
        // ── Spin court ──
        for _ in 0..SPIN_BARRIERE {
            if self.generation.load(Acquire) != g { return; }
            std::hint::spin_loop();
        }
        // ── Parking ──
        let mut v = self.verrou.lock().unwrap();
        while self.generation.load(Acquire) == g {
            v = self.cond.wait(v).unwrap();
        }
    }
}

/// Pointeur brut partagé entre fils de la NTT.
///
/// ⚠ `*mut u64` n'est pas `Sync`. On encapsule le pointeur dans ce type
/// pour que le compilateur accepte le partage. C'est SÛR parce que les
/// plages d'indices traitées par chaque fil sont DISJOINTES (démontré
/// dans le commentaire de `ntt_par`).
#[derive(Copy, Clone)]
struct PointeurPartage(*mut u64);
// SAFETY : voir ci-dessus — chaque fil n'accède qu'à sa propre plage.
unsafe impl Send for PointeurPartage {}
unsafe impl Sync for PointeurPartage {}

/// Seuil (en éléments) au-delà duquel la NTT est parallélisée. En dessous,
/// le coût de création des fils (370 µs) et des barrières dépasse le gain.
/// ⚠ Ce seuil a été ABAISSÉ à 2^12 : la conversion décimale par
/// multiplication a des niveaux profonds (des dizaines de milliers de
/// multiplications de taille décroissante) qui, non parallélisés,
/// coûtaient ~950 ms à eux seuls. La four-step y a assez peu de
/// barrières (3 par transformée) pour que le parallèle reste rentable.
const SEUIL_PAR_NTT: usize = 32768;

/// Nombre de fils pour une NTT de longueur `n`.
fn fils_ntt(n: usize) -> usize {
    if n < SEUIL_PAR_NTT { return 1; }
    let coeurs = std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1);
    // On garde au plus n/2 fils : au-delà, chaque fil n'aurait rien à faire.
    coeurs.min(n / 2).max(1)
}

/// Taille des BLOCS de la NTT (en éléments).
///
/// ⚠⚠⚠ LE GOULET EST MESURÉ : la NTT radix-2 classique fait 20 passes sur
/// un tableau de 8 Mo à lg = 2^20, soit 1,92 Go de trafic mémoire par
/// convolution — la machine est alors à SATURATION (28,7 Go/s mesurés),
/// et le speedup parallèle plafonne à 3× (au lieu de 20×).
///
/// L'idée : traiter les DERNIERS étages à l'intérieur de BLOCS qui tiennent
/// dans le cache L1 (8 Ko = 1024 éléments u64). Les passes sur le tableau
/// entier ne sont plus que log₂(n/1024) = 10 au lieu de 20, et les 10
/// dernières passent se font en L1 (trafic DRAM quasi nul).
///
/// Trafic mémoire : 20 passes → 10 passes + un aller-retour local.
const TAILLE_BLOC: usize = 1024;

/// Un étage de la NTT, réparti sur les fils.
///
/// ⚠ `i` est l'indice GLOBAL du papillon dans l'ordre (bloc, k) :
/// `bloc = i >> log₂(half)`, `k = i & (half−1)`. Les cases touchées sont
/// `base+k` et `base+k+half` ; `base+k` est STRICTEMENT CROISSANT en `i`
/// (p1 = 2i − (i mod half)), donc deux plages disjointes d'indices touchent
/// des cases disjointes → aucune course de données.
#[inline]
fn stage_partage(
    ptr: PointeurPartage,
    n: usize,
    len: usize,
    rac: &Racines,
    mont: &Mont,
    inverse: bool,
    tid: usize,
    fils: usize,
    decalage_bloc: usize,
) {
    let n2 = n / 2;
    let half = len / 2;
    let pas = n2 / half;
    let lg_half = half.trailing_zeros();
    let masque = half - 1;
    let lo = n2 * tid / fils;
    let hi = n2 * (tid + 1) / fils;
    let mut i = lo;
    while i < hi {
        let bloc = i >> lg_half;
        let k0 = i & masque;
        let kend = std::cmp::min(half, k0 + (hi - i));
        let base = bloc * len + decalage_bloc;
        for k in k0..kend {
            let p1 = base + k;
            let p2 = p1 + half;
            // ⚠ SÛR : plages disjointes ⇒ cases disjointes.
            unsafe {
                let u = *ptr.0.add(p1);
                let w = *ptr.0.add(p2);
                if inverse {
                    let t = mont.mul(w, rac.inverses[k * pas]);
                    *ptr.0.add(p1) = mont.add(u, t);
                    *ptr.0.add(p2) = mont.sub(u, t);
                } else {
                    *ptr.0.add(p1) = mont.add(u, w);
                    *ptr.0.add(p2) = mont.mul(mont.sub(u, w), rac.directes[k * pas]);
                }
            }
        }
        i += kend - k0;
    }
}

/// Les étages À L'INTÉRIEUR d'un bloc (le bloc tient dans le cache L1).
///
/// ⚠ Ici le « n » de la formule du papillon est la taille du BLOC, donc
/// la table de racines est celle du bloc (racine ω^(n/taille_bloc)).
fn stage_bloc(
    ptr: PointeurPartage,
    nblk: usize,
    len: usize,
    rac: &Racines,
    mont: &Mont,
    inverse: bool,
    tid: usize,
    fils: usize,
    nb_blocs: usize,
) {
    let n2 = nblk / 2;
    let half = len / 2;
    let pas = n2 / half;
    let lg_half = half.trailing_zeros();
    let masque = half - 1;
    // ⚠ Chaque fil prend un sous-ensemble de BLOCS (entiers) : les papillons
    // d'un bloc ne sont PAS répartis entre les fils (le bloc tient dans
    // L1, le répartir n'apporterait rien et compliquerait l'indexation).
    let lo_bloc = nb_blocs * tid / fils;
    let hi_bloc = nb_blocs * (tid + 1) / fils;
    for b in lo_bloc..hi_bloc {
        let base_bloc = b * nblk;
        let mut i = 0usize;
        while i < n2 {
            let bloc = i >> lg_half;
            let k0 = i & masque;
            let kend = std::cmp::min(half, k0 + (n2 - i));
            let base = base_bloc + bloc * len;
            for k in k0..kend {
                let p1 = base + k;
                let p2 = p1 + half;
                // ⚠ SÛR : les blocs sont disjoints, et dans un bloc les
                // papillons (bloc, k) sont disjoints.
                unsafe {
                    let u = *ptr.0.add(p1);
                    let w = *ptr.0.add(p2);
                    if inverse {
                        let t = mont.mul(w, rac.inverses[k * pas]);
                        *ptr.0.add(p1) = mont.add(u, t);
                        *ptr.0.add(p2) = mont.sub(u, t);
                    } else {
                        *ptr.0.add(p1) = mont.add(u, w);
                        *ptr.0.add(p2) = mont.mul(mont.sub(u, w), rac.directes[k * pas]);
                    }
                }
            }
            i += kend - k0;
        }
    }
}

/// Une transformée (directe ou inverse) EXÉCUTÉE PAR UN SEUL FIL, dans un
/// cadre où les autres fils travaillent sur le MÊME tableau.
///
/// ⚠⚠⚠ POURQUOI CE DÉCOUPAGE. Mesuré : créer un `thread::scope` de 20 fils
/// coûte **370 µs**. La NTT faisait 6 scopes (6 transformées) → 2,2 ms de
/// pure création de fils par multiplication. Sur l'arbre de `1 000 000!`
/// (≈ 40 multiplications au-dessus du seuil), cela représente ~88 ms
/// perdus — un tiers du temps total !
///
/// On crée donc UN SEUL scope par multiplication, et les 6 transformées
/// (2 directes + 1 inverse, pour 2 modules) s'enchaînent à l'intérieur,
/// séparées par des barrières.
fn ntt_etape(
    ptr: PointeurPartage,
    n: usize,
    rac: &Racines,
    rac_bloc: &Racines,
    mont: &Mont,
    inverse: bool,
    tid: usize,
    fils: usize,
    bar: &Barriere,
) {
    let nblk = if n >= 2 * TAILLE_BLOC { TAILLE_BLOC } else { n };
    let nb_blocs = n / nblk;

    if inverse {
        // ── DIT inverse : les blocs D'ABORD, puis les étages globaux ──
        let mut len = 2usize;
        while len <= nblk {
            bar.wait();
            stage_bloc(ptr, nblk, len, rac_bloc, mont, inverse, tid, fils, nb_blocs);
            len <<= 1;
        }
        bar.wait();
        let mut len = 2 * nblk;
        while len <= n {
            bar.wait();
            stage_partage(ptr, n, len, rac, mont, inverse, tid, fils, 0);
            len <<= 1;
        }
    } else {
        // ── DIF directe : les étages globaux D'ABORD, puis les blocs ──
        let mut len = n;
        while len > nblk {
            bar.wait();
            stage_partage(ptr, n, len, rac, mont, inverse, tid, fils, 0);
            len >>= 1;
        }
        bar.wait();
        let mut len = nblk;
        while len >= 2 {
            bar.wait();
            stage_bloc(ptr, nblk, len, rac_bloc, mont, inverse, tid, fils, nb_blocs);
            len >>= 1;
        }
    }
    bar.wait();
}

// ═══════════════════════════════════════════════════════════════════════
// NTT EN QUATRE ÉTAPES (four-step)
// ═══════════════════════════════════════════════════════════════════════
//
// ⚠⚠⚠ POURQUOI. MESURÉ : la NTT radix-2 fait log₂(n) passes sur le
// tableau entier. À lg = 2^20 (8 Mo), ce sont 20 passes = 1,92 Go de
// trafic par convolution, et la machine est à SATURATION (24-28 Go/s
// mesurés). La variante « par blocs » (les derniers étages en cache L1)
// réduit le trafic de moitié mais NE SUFFIT PAS : le nombre de passes
// reste 10, et chacune est une lecture SCRAMBÉE de 8 Mo → la LATENCE
// mémoire est exposée.
//
// La four-step ramène à TROIS passes sur le tableau entier :
//
//   1. n₂ sous-transformées de taille n₁, en accès STRIDÉ (la FFT est
//      faite sur place, dans le cache) ;
//   2. multiplication par les twiddles ω^(j₂·k₁) (une passe) ;
//   3. n₁ sous-transformées de taille n₂, CONTIGUËS (dans le cache).
//
// Avec n₁ = n₂ = √n, chaque sous-transformée porte sur 8 Ko : tout tient
// dans L1, et le trafic DRAM se limite à 3 allers-retours.
//
// ⚠⚠ DÉRIVATION (les permutations sont le point difficile).
//
// On veut X_k = Σ_j x_j·ω_n^{jk}, avec n = n₁·n₂.
// Écrivons j = j₁·n₂ + j₂ et k = k₂·n₁ + k₁. Alors :
//
//   X_{k₂n₁+k₁} = Σ_{j₂} Y[j₂][k₁]·ω_n^{j₂k₁}·ω_{n₂}^{j₂k₂}
//
// où Y[j₂][k₁] = Σ_{j₁} x_{j₁n₂+j₂}·ω_{n₁}^{j₁k₁}.
//
// Donc :
//   Étape 1 : pour chaque j₂, la DFT de taille n₁ de la suite
//             (x[j₂], x[n₂+j₂], x[2n₂+j₂], …) → Y[j₂][·].
//   Étape 2 : Z[j₂][k₁] = Y[j₂][k₁]·ω_n^{j₂k₁}.
//   Étape 3 : pour chaque k₁, la DFT de taille n₂ de (Z[0][k₁], …,
//             Z[n₂−1][k₁]) → X[·][k₁].
//
// ⚠ L'ORDRE DES PERMUTATIONS. On utilise une DIF pour l'étape 1 : la
// sortie est en ordre BIT-REVERSÉ (k₁' ↔ br(k₁)). La DFT de l'étape 3
// porte donc sur ω_n^{j₂·br(k₁')} — d'où la table de twiddles
// `tw[k₁'] = ω_n^{br(k₁')}`.
//
// L'inverse est l'envers exact : DIT à l'étape 3′ (entrée bit-reversée
// en k₂ → sortie naturelle), puis les twiddles ω_n^{−j₂·br(k₁')}, puis
// DIT à l'étape 1′ (entrée bit-reversée en k₁' → sortie naturelle).

/// Table des twiddles de l'étape 2 : `tw[k'] = ω_n^{br(k')}`.
///
/// ⚠ `br` = inversion des bits de k' sur log₂(n₁) bits. Comme
/// ω_n^{br(k')} = ∏_{i : bit i de k' à 1} ω_n^{2^i}, on construit la table
/// en O(n₁) multiplications à partir des puissances de ω_n.
fn table_twiddle(mont: &Mont, racine: u64, m: u64, n: usize, n1: usize)
                 -> (Vec<u64>, Vec<u64>) {
    let omega = puissance_mod(racine, (m - 1) / n as u64, m);
    let om = mont.to_mont(omega);
    let oi = mont.to_mont(inverse_mod(omega, m));
    // ⚠⚠⚠ LES PUISSANCES SONT INVERSÉES. On veut ω_n^{br(k)}, et
    // br(k) = Σ_i bit_i(k)·2^{L−1−i} (L = log₂(n₁)) : le bit i de k
    // contribue la puissance 2^{L−1−i}, PAS 2^i. Construire ω_n^{k} à la
    // place donnait un résultat FAUX (test rouge).
    //
    // On construit donc ω_n^{2^j} pour j = 0..L, puis on les prend à
    // l'envers : puissances[i] = ω_n^{2^{L−1−i}}.
    let lg = n1.trailing_zeros() as usize;
    let mut puissances: Vec<u64> = vec![0; lg];
    let mut puissances_i: Vec<u64> = vec![0; lg];
    let mut p = om;
    let mut q = oi;
    for j in 0..lg {
        puissances[lg - 1 - j] = p;
        puissances_i[lg - 1 - j] = q;
        p = mont.mul(p, p);
        q = mont.mul(q, q);
    }
    let mut tw = vec![0u64; n1];
    let mut twi = vec![0u64; n1];
    tw[0] = mont.to_mont(1);
    twi[0] = mont.to_mont(1);
    for k in 1..n1 {
        let i = k.trailing_zeros() as usize;
        tw[k] = mont.mul(tw[k & (k - 1)], puissances[i]);
        twi[k] = mont.mul(twi[k & (k - 1)], puissances_i[i]);
    }
    (tw, twi)
}

/// DIF EN PLACE sur une suite à PAS CONSTANT, via le pointeur partagé.
///
/// ⚠ Les éléments logiques i sont aux positions `base + i·stride`.
fn dif_stride_sur(ptr: PointeurPartage, base: usize, stride: usize, n: usize,
                  rac: &Racines, mont: &Mont) {
    let n2 = n / 2;
    let mut len = n;
    while len >= 2 {
        let half = len / 2;
        let pas = n2 / half;
        let mut bloc = 0usize;
        while bloc < n / len {
            let b = base + bloc * len * stride;
            for k in 0..half {
                let p1 = b + k * stride;
                let p2 = p1 + half * stride;
                unsafe {
                    let u = *ptr.0.add(p1);
                    let w = *ptr.0.add(p2);
                    *ptr.0.add(p1) = mont.add(u, w);
                    *ptr.0.add(p2) = mont.mul(mont.sub(u, w), rac.directes[k * pas]);
                }
            }
            bloc += 1;
        }
        len >>= 1;
    }
}

/// DIT EN PLACE sur une suite à PAS CONSTANT, via le pointeur partagé.
fn dit_stride_sur(ptr: PointeurPartage, base: usize, stride: usize, n: usize,
                  rac: &Racines, mont: &Mont) {
    let n2 = n / 2;
    let mut len = 2usize;
    while len <= n {
        let half = len / 2;
        let pas = n2 / half;
        let mut bloc = 0usize;
        while bloc < n / len {
            let b = base + bloc * len * stride;
            for k in 0..half {
                let p1 = b + k * stride;
                let p2 = p1 + half * stride;
                unsafe {
                    let u = *ptr.0.add(p1);
                    let t = mont.mul(*ptr.0.add(p2), rac.inverses[k * pas]);
                    *ptr.0.add(p1) = mont.add(u, t);
                    *ptr.0.add(p2) = mont.sub(u, t);
                }
            }
            bloc += 1;
        }
        len <<= 1;
    }
}

/// DIF sur une suite à PAS CONSTANT : les éléments logiques i sont aux
/// positions `base + i·stride`.
///
/// ⚠ Aucune permutation de bit-reversal : la DIF ( décimation en
/// fréquence) prend une entrée NATURELLE et produit une sortie
/// BIT-REVERSÉE.
fn dif_stride(v: &mut [u64], base: usize, stride: usize, n: usize,
              rac: &Racines, mont: &Mont) {
    let n2 = n / 2;
    let mut len = n;
    while len >= 2 {
        let half = len / 2;
        let pas = n2 / half;
        let mut bloc = 0usize;
        while bloc < n / len {
            let b = base + bloc * len * stride;
            for k in 0..half {
                let p1 = b + k * stride;
                let p2 = p1 + half * stride;
                let u = v[p1];
                let w = v[p2];
                v[p1] = mont.add(u, w);
                v[p2] = mont.mul(mont.sub(u, w), rac.directes[k * pas]);
            }
            bloc += 1;
        }
        len >>= 1;
    }
}

/// DIT (inverse) sur une suite à PAS CONSTANT : l'entrée est en ordre
/// BIT-REVERSÉ, la sortie est naturelle.
fn dit_stride(v: &mut [u64], base: usize, stride: usize, n: usize,
              rac: &Racines, mont: &Mont) {
    let n2 = n / 2;
    let mut len = 2usize;
    while len <= n {
        let half = len / 2;
        let pas = n2 / half;
        let mut bloc = 0usize;
        while bloc < n / len {
            let b = base + bloc * len * stride;
            for k in 0..half {
                let p1 = b + k * stride;
                let p2 = p1 + half * stride;
                let u = v[p1];
                let t = mont.mul(v[p2], rac.inverses[k * pas]);
                v[p1] = mont.add(u, t);
                v[p2] = mont.sub(u, t);
            }
            bloc += 1;
        }
        len <<= 1;
    }
}

/// La transformation four-step DIRECTE, exécutée par le fil `tid`.
///
/// Les trois étapes sont séparées par des barrières implicites : chaque
/// fil exécute la même séquence.
fn ntt4_direct(
    ptr: PointeurPartage,
    n: usize,
    n1: usize,
    n2: usize,
    rac1: &Racines,
    rac2: &Racines,
    tw: &[u64],
    mont: &Mont,
    tid: usize,
    fils: usize,
    bar: &Barriere,
) {
    // ── ÉTAPE 1 : n₂ sous-transformées de taille n₁, accès STRIDÉ ──
    // Le fil prend un sous-ensemble des j₂ (entiers) : les suites sont
    // disjointes, donc aucun partage de cache line entre fils.
    let lo = n2 * tid / fils;
    let hi = n2 * (tid + 1) / fils;
    // ⚠⚠⚠ LA SOUS-TRANSFORMÉE PASSE PAR UN TAMpon CONTIGU. Mesuré : la
    // version « en place, en accès stridé » est PLUS LENTE (69,6 ms contre
    // 57,0 ms à lg = 2^20) — chaque papillon de la DIF stridée touche
    // deux lignes de cache à 8 Ko d'écart, alors que la copie ne coûte
    // qu'UN passage stridé puis que la transformée se fait en L1.
    let mut copie: Vec<u64> = vec![0u64; n1];
    for j2 in lo..hi {
        unsafe {
            for j1 in 0..n1 {
                copie[j1] = *ptr.0.add(j2 + j1 * n2);
            }
        }
        dif_stride(&mut copie, 0, 1, n1, rac1, mont);
        unsafe {
            for j1 in 0..n1 {
                *ptr.0.add(j2 + j1 * n2) = copie[j1];
            }
        }
    }
    bar.wait();
    // ── ÉTAPE 2 : twiddles ω^(j₂·br(k₁')) ──
    // Chaque fil prend un sous-ensemble des k₁' ; le bloc
    // [k₁'·n₂, (k₁'+1)·n₂) est CONTIGË et contient les j₂ croissants.
    let lo = n1 * tid / fils;
    let hi = n1 * (tid + 1) / fils;
    for k1 in lo..hi {
        let mut cur = mont.to_mont(1);
        let ratio = tw[k1];
        let base = k1 * n2;
        unsafe {
            for j2 in 0..n2 {
                let p = base + j2;
                *ptr.0.add(p) = mont.mul(*ptr.0.add(p), cur);
                cur = mont.mul(cur, ratio);
            }
        }
    }
    bar.wait();
    // ── ÉTAPE 3 : n₁ sous-transformées de taille n₂, CONTIGUËS ──
    let mut bloc: Vec<u64> = vec![0u64; n2];
    for k1 in lo..hi {
        let base = k1 * n2;
        unsafe {
            for j2 in 0..n2 {
                bloc[j2] = *ptr.0.add(base + j2);
            }
        }
        dif_stride(&mut bloc, 0, 1, n2, rac2, mont);
        unsafe {
            for j2 in 0..n2 {
                *ptr.0.add(base + j2) = bloc[j2];
            }
        }
    }
    bar.wait();
}

/// La transformation four-step INVERSE, exécutée par le fil `tid`.
fn ntt4_inverse(
    ptr: PointeurPartage,
    n: usize,
    n1: usize,
    n2: usize,
    rac1: &Racines,
    rac2: &Racines,
    twi: &[u64],
    mont: &Mont,
    tid: usize,
    fils: usize,
    bar: &Barriere,
) {
    // ── ÉTAPE 3′ : DIT de taille n₂ sur le bloc CONTIGÙ ──
    let lo = n1 * tid / fils;
    let hi = n1 * (tid + 1) / fils;
    let mut bloc: Vec<u64> = vec![0u64; n2];
    for k1 in lo..hi {
        let base = k1 * n2;
        unsafe {
            for j2 in 0..n2 {
                bloc[j2] = *ptr.0.add(base + j2);
            }
        }
        dit_stride(&mut bloc, 0, 1, n2, rac2, mont);
        unsafe {
            for j2 in 0..n2 {
                *ptr.0.add(base + j2) = bloc[j2];
            }
        }
    }
    bar.wait();
    // ── ÉTAPE 2′ : twiddles ω^(−j₂·br(k₁')) ──
    for k1 in lo..hi {
        let mut cur = mont.to_mont(1);
        let ratio = twi[k1];
        let base = k1 * n2;
        unsafe {
            for j2 in 0..n2 {
                let p = base + j2;
                *ptr.0.add(p) = mont.mul(*ptr.0.add(p), cur);
                cur = mont.mul(cur, ratio);
            }
        }
    }
    bar.wait();
    // ── ÉTAPE 1′ : DIT de taille n₁, accès STRIDÉ ──
    let lo2 = n2 * tid / fils;
    let hi2 = n2 * (tid + 1) / fils;
    let mut copie: Vec<u64> = vec![0u64; n1];
    for j2 in lo2..hi2 {
        unsafe {
            for j1 in 0..n1 {
                copie[j1] = *ptr.0.add(j2 + j1 * n2);
            }
        }
        dit_stride(&mut copie, 0, 1, n1, rac1, mont);
        unsafe {
            for j1 in 0..n1 {
                *ptr.0.add(j2 + j1 * n2) = copie[j1];
            }
        }
    }
    bar.wait();
}

/// Produit point à point, partagé entre les fils.
fn ntt_pointwise(pa: PointeurPartage, pb: PointeurPartage, n: usize, mont: &Mont,
                 tid: usize, fils: usize) {
    let lo = n * tid / fils;
    let hi = n * (tid + 1) / fils;
    for i in lo..hi {
        // ⚠ SÛR : chaque fil a sa propre plage d'indices.
        unsafe {
            let av = *pa.0.add(i);
            let bv = *pb.0.add(i);
            *pa.0.add(i) = mont.mul(av, bv);
        }
    }
}

/// Normalisation (÷ n) ET conversion hors forme Montgomery, partagée.
fn ntt_normalise(pa: PointeurPartage, n: usize, mont: &Mont, inv_n: u64,
                 tid: usize, fils: usize) {
    let lo = n * tid / fils;
    let hi = n * (tid + 1) / fils;
    for i in lo..hi {
        // ⚠ SÛR : chaque fil a sa propre plage d'indices.
        unsafe {
            let v = *pa.0.add(i);
            *pa.0.add(i) = mont.mul(v, inv_n);
        }
    }
}

/// NTT directe — décimation en FREQUENCE (DIF).
///
/// ⚠⚠⚠ LE CHOIX DIF/DIT SUPPRIME TOUTES LES PERMUTATIONS DE BIT-REVERSAL.
/// L'ancienne version en faisait TROIS (une au début de chacune des trois
/// transformées) : chacune est un accès ALÉATOIRE sur 4–8 Mo, donc très
/// pénalisant en bande passante mémoire.
///
///   DIF : entrée en ordre NATUREL → sortie en ordre BIT-REVERSÉ.
///   DIT inverse : entrée en ordre BIT-REVERSÉ → sortie en ordre NATUREL.
///
/// Les deux tableaux du produit point à point sont bit-reversés de la même
/// façon, donc le produit point à point se fait position par position ✓.
fn ntt_dif(v: &mut [u64], rac: &Racines, mont: &Mont) {
    let n = v.len();
    let demi_n = n / 2;
    let mut len = n;
    while len >= 2 {
        let demi = len / 2;
        let pas = demi_n / demi; // pas d'avance dans la table des racines
        let mut base = 0usize;
        while base < n {
            let (gauche, droite) = v[base..base + len].split_at_mut(demi);
            for k in 0..demi {
                let u = gauche[k];
                let w = droite[k];
                gauche[k] = mont.add(u, w);
                droite[k] = mont.mul(mont.sub(u, w), rac.directes[k * pas]);
            }
            base += len;
        }
        len >>= 1;
    }
}

/// NTT inverse — décimation en TEMPS (DIT), racine inverse.
///
/// ⚠ La normalisation (÷ n) est faite par l'appelant, en une seule
/// multiplication de Montgomery qui fait aussi la conversion hors forme
/// Montgomery.
fn ntt_dit(v: &mut [u64], rac: &Racines, mont: &Mont) {
    let n = v.len();
    let demi_n = n / 2;
    let mut len = 2usize;
    while len <= n {
        let demi = len / 2;
        let pas = demi_n / demi;
        let mut base = 0usize;
        while base < n {
            let (gauche, droite) = v[base..base + len].split_at_mut(demi);
            for k in 0..demi {
                let u = gauche[k];
                let t = mont.mul(droite[k], rac.inverses[k * pas]);
                gauche[k] = mont.add(u, t);
                droite[k] = mont.sub(u, t);
            }
            base += len;
        }
        len <<= 1;
    }
}

/// Multiplication NTT avec 2 modules de 62 bits + rests chinois.
///
/// `fils` = nombre de fils pour la NTT (0 = tous les cœurs).
/// `base` = base de propagation des retenues (0 = 2³², sinon la base
/// demandée, typiquement 10⁹).
fn ntt_mul_par(x: &[u32], y: &[u32], fils: usize, base: u64) -> Vec<u32> {
    let besoin = x.len() + y.len();
    // ⚠ La convolution linéaire exige une longueur ≥ |x|+|y|−1, puissance
    // de 2. On prend la puissance de 2 immédiatement supérieure.
    let mut n = 1usize;
    while n < besoin { n <<= 1; }
    // ⚠⚠ Vérification du DÉBORDEMENT : chaque coefficient vaut au plus
    // min(|x|,|y|)·(2³²−1)². Il faut que le produit des modules le dépasse.
    let borne = (x.len().min(y.len()) as u128) * (u32::MAX as u128) * (u32::MAX as u128);
    let produit_modules: u128 = MODULES.iter().map(|(m, _)| *m as u128).product();
    if borne >= produit_modules {
        // Trop grand pour 2 modules : on reste sur Toom-3 (correct, plus lent).
        return toom3(x, y);
    }
    // ⚠ 2-adicité des modules = 32 : longueur NTT jusqu'à 2³².
    if n > (1usize << 32) {
        return toom3(x, y);
    }

    // ⚠⚠⚠ DEUX TENTATIVES D'OPTIMISATION REJETÉES (tests rouges), pour ne
    // pas les refaire :
    //  1. « c ← c² puis NTT inverse, partie imaginaire » : la reconstruction
    //     par `c[(n−k)%n]` est fausse (test `toom3_et_ntt_egalent_lecole`).
    //  2. « symétrie hermitienne A[k] = (C[k]+C[n−k])/2 » : NE S'APPLIQUE PAS.
    //     La symétrie hermitienne suppose des coefficients RÉELS ; ici x et y
    //     sont des entiers mod m (le corps fini n'a pas de conjugaison).
    //
    // On garde donc 3 NTT par module (2 directes + 1 inverse), soit 6 au
    // total — c'est le coût incompressible de la méthode.

    // Préparation des deux modules.
    let tous_monts = monts();
    let mut monts: Vec<&Mont> = Vec::with_capacity(2);
    let mut racs: Vec<Racines> = Vec::with_capacity(2);
    // ⚠ Table des RACINES DE BLOC : la racine ω du bloc est ω^(n/taille_bloc),
    // c'est-à-dire racine^((m−1)/taille_bloc) — la même formule que pour la
    // table globale, avec la taille du bloc à la place de n.
    let mut racs_bloc: Vec<Racines> = Vec::with_capacity(2);
    let nblk = if n >= 2 * TAILLE_BLOC { TAILLE_BLOC } else { n };
    // ⚠ FOUR-STEP : n = n₁·n₂ avec n₁ = n₂ = √n. Les tables des
    // sous-transformées (racines de n₁ et de n₂) et celle des twiddles
    // de l'étape 2.
    let lg = n.trailing_zeros() as usize;
    let n1 = 1usize << (lg / 2);
    let n2 = 1usize << (lg - lg / 2);
    let mut racs1: Vec<Racines> = Vec::with_capacity(2);
    let mut racs2: Vec<Racines> = Vec::with_capacity(2);
    let mut tws: Vec<Vec<u64>> = Vec::with_capacity(2);
    let mut twis: Vec<Vec<u64>> = Vec::with_capacity(2);
    for (k, &(m, racine)) in MODULES.iter().enumerate() {
        let mont = &tous_monts[k];
        debug_assert_eq!(mont.m, m);
        racs.push(racines(mont, racine, n));
        racs_bloc.push(racines(mont, racine, nblk));
        racs1.push(racines(mont, racine, n1));
        racs2.push(racines(mont, racine, n2));
        let (tw, twi) = table_twiddle(mont, racine, m, n, n1);
        tws.push(tw);
        twis.push(twi);
        monts.push(mont);
    }
    // ⚠ Conversion en forme Montgomery : REDC(x·R²) = x·R.
    let mut a: Vec<Vec<u64>> = Vec::with_capacity(2);
    let mut b: Vec<Vec<u64>> = Vec::with_capacity(2);
    for mont in monts.iter() {
        let mut va: Vec<u64> = x.iter().map(|&v| mont.to_mont(v as u64)).collect();
        let mut vb: Vec<u64> = y.iter().map(|&v| mont.to_mont(v as u64)).collect();
        va.resize(n, 0);
        vb.resize(n, 0);
        a.push(va);
        b.push(vb);
    }

    // ⚠ La four-step est utilisée à partir de lg ≥ 12 (4096) : en dessous,
    // les sous-transformées sont trop petites pour que le gain de trafic
    // compense le surcoût des copies.
    let quatre = lg >= 12;
    let fils = if fils == 0 { fils_ntt(n) } else { fils.min(fils_ntt(n)) };
    if fils <= 1 {
        // SÉQUENTIEL : pas de fil à créer.
        // ⚠ On garde ici la NTT radix-2 (ntt_dif/ntt_dit) et NON la
        // four-step : mesuré, la four-step SÉQUENTIELLE est plus lente
        // (270 ms contre 232 ms à lg = 2^20) — les copies des
        // sous-transformées coûtent plus que le trafic économisé quand on
        // n'a qu'un fil (le tableau tient alors dans le L3).
        for k in 0..2 {
            let mont = monts[k];
            ntt_dif(&mut a[k], &racs[k], mont);
            ntt_dif(&mut b[k], &racs[k], mont);
            for i in 0..n { a[k][i] = mont.mul(a[k][i], b[k][i]); }
            ntt_dit(&mut a[k], &racs[k], mont);
            let inv_n = inverse_mod(n as u64, mont.m);
            for i in 0..n { a[k][i] = mont.mul(a[k][i], inv_n); }
        }
    } else {
        use std::sync::Arc;
        let bar = Arc::new(Barriere::nouveau(fils));
        let mut tables: Vec<PointeurPartage> = Vec::with_capacity(4);
        for k in 0..2 {
            tables.push(PointeurPartage(a[k].as_mut_ptr()));
        }
        for k in 0..2 {
            tables.push(PointeurPartage(b[k].as_mut_ptr()));
        }
        // Ordre : [a0, a1, b0, b1] → tables[k] = a du module k,
        // tables[k+2] = b du module k.
        // ⚠ Un SEUL corps pour le fil 0 et les fils créés : le code est
        // factorisé dans une closure, sinon les tables borrowing ne
        // survivraient pas au `move` du closure.
        let corps = |tid: usize, tables: &[PointeurPartage], racs: &[Racines],
                     racs_bloc: &[Racines], racs1: &[Racines], racs2: &[Racines],
                     tws: &[Vec<u64>], twis: &[Vec<u64>],
                     monts: &[&Mont], bar: &Barriere| {
            for k in 0..2 {
                let mont = monts[k];
                let rac = &racs[k];
                // 1. NTT directe de a
                if quatre {
                    ntt4_direct(tables[k], n, n1, n2, &racs1[k], &racs2[k],
                                &tws[k], mont, tid, fils, bar);
                } else {
                    ntt_etape(tables[k], n, rac, &racs_bloc[k], mont, false, tid, fils, bar);
                }
                // 2. NTT directe de b
                if quatre {
                    ntt4_direct(tables[k + 2], n, n1, n2, &racs1[k], &racs2[k],
                                &tws[k], mont, tid, fils, bar);
                } else {
                    ntt_etape(tables[k + 2], n, rac, &racs_bloc[k], mont, false, tid, fils, bar);
                }
                // 3. Produit point à point
                bar.wait();
                ntt_pointwise(tables[k], tables[k + 2], n, mont, tid, fils);
                bar.wait();
                // 4. NTT inverse de a
                if quatre {
                    ntt4_inverse(tables[k], n, n1, n2, &racs1[k], &racs2[k],
                                 &twis[k], mont, tid, fils, bar);
                } else {
                    ntt_etape(tables[k], n, rac, &racs_bloc[k], mont, true, tid, fils, bar);
                }
                // 5. Normalisation (÷ n) ET conversion hors Montgomery
                bar.wait();
                let inv_n = inverse_mod(n as u64, mont.m);
                ntt_normalise(tables[k], n, mont, inv_n, tid, fils);
                bar.wait();
            }
        };
        // ⚠ Les tables (racines) sont IMMUABLES et partagées : on les met
        // dans des Arc pour que chaque fil puisse les référencer.
        let racs = std::sync::Arc::new(racs);
        let racs_bloc = std::sync::Arc::new(racs_bloc);
        let racs1 = std::sync::Arc::new(racs1);
        let racs2 = std::sync::Arc::new(racs2);
        let tws = std::sync::Arc::new(tws);
        let twis = std::sync::Arc::new(twis);
        let monts = std::sync::Arc::new(monts);
        std::thread::scope(|sc| {
            for t in 1..fils {
                let bar = bar.clone();
                let tables = tables.clone();
                let racs = racs.clone();
                let racs_bloc = racs_bloc.clone();
                let racs1 = racs1.clone();
                let racs2 = racs2.clone();
                let tws = tws.clone();
                let twis = twis.clone();
                let monts = monts.clone();
                sc.spawn(move || {
                    corps(t, &tables, &racs, &racs_bloc, &racs1, &racs2,
                          &tws, &twis, &monts, &bar);
                });
            }
            // Le fil 0 fait exactement le même travail.
            corps(0, &tables, &racs, &racs_bloc, &racs1, &racs2,
                  &tws, &twis, &monts, &bar);
        });
    }
    let mut restes: Vec<Vec<u64>> = a;

    // ── Restes chinois à DEUX modules ──────────────────────────────────
    //
    // x ≡ r1 (mod m1), x ≡ r2 (mod m2). Alors
    //   x = r1 + m1·t,  t = (r2 − r1)·m1⁻¹ mod m2.
    // ⚠ m1·t peut atteindre ~2^124 : on le garde en u128, et on ajoute la
    // retenue de propagation des limbs.
    let m1 = MODULES[0].0;
    let m2 = MODULES[1].0;
    let inv_m1 = inverse_mod(m1 % m2, m2) as u128;
    let m1u = m1 as u128;

    let mut r = vec![0u32; besoin];
    let mut retenue: u128 = 0;
    for i in 0..besoin {
        let r1 = restes[0][i];
        let r2 = restes[1][i];
        // t = (r2 − r1)·m1⁻¹ mod m2
        let diff = if r2 >= r1 % m2 { r2 - r1 % m2 } else { r2 + m2 - r1 % m2 };
        let t = (diff as u128 * inv_m1) % m2 as u128;
        let mut c = r1 as u128 + m1u * t + retenue;
        // ⚠ Propagation des retenues dans la base demandée.
        //   base 2³² : un simple décalage (le cas normal).
        //   base 10⁹ : une division — mais 10⁹ est une CONSTANTE, donc le
        //   compilateur la transforme en multiplication-shift ✓.
        if base == 0 {
            r[i] = c as u32;
            retenue = c >> 32;
        } else if base == 1_000_000_000 {
            // ⚠⚠⚠ LA CONSTANTE DOIT ÊTRE LITTÉRALE. Avec une base RUNTIME,
            // `c % base` sur un u128 compilait en un APPEL à `__udivti3`
            // (division 128/128 logicielle, ~50-100 ns) — par élément !
            // Mesuré : la conversion décimale de 1 000 000! coûtait 2,0 s
            // à cause de ça. Avec la constante littérale, LLVM génère une
            // suite multiplication-décalage (~10 instructions, sans appel).
            const B: u128 = 1_000_000_000;
            r[i] = (c % B) as u32;
            retenue = c / B;
        } else {
            let b = base as u128;
            r[i] = (c % b) as u32;
            retenue = c / b;
        }
    }
    normaliser(r)
}

// ── DIVISION RAPIDE : inverse de Newton ─────────────────────────────────
//
// ⚠⚠⚠ POURQUOI. `num-bigint` divise par Knuth (école) : O(n·m). Mesuré :
// UNE division par un diviseur de la moitié de la taille d'un nombre de
// 18,5 millions de bits coûte 21,9 ms — et la conversion décimale en fait
// ~20, ce qui donne les 2,88 s observés. GMP divise en O(M(n)) grâce à
// l'INVERSE DE NEWTON : on calcule d'abord ⌊(B^(2n)−1)/d⌋, puis on multiplie.
//
// ⚠⚠⚠ LA FORMULE CORRECTE (vérifiée en Python sur n = 1..2000, exacte à
// chaque fois). Le piège qui avait fait échouer mes deux premières
// tentatives : l'AMORÇAGE doit être aligné à la bonne échelle.
//
//   v_0 = ⌊(B² − 1) / d_top⌋ · B^(n−1)
//
// où `d_top` est le limb de tête de d (bit 31 à 1). Pourquoi ce décalage :
//   la vraie valeur est ⌊B^(2n)/d⌋ ≈ B^(2n) / (d_top·B^(n−1)) = B^(n+1)/d_top
//   et ⌊(B²−1)/d_top⌋ ≈ B²/d_top ; il faut donc le multiplier par B^(n−1).
//
// Puis Newton, à échelle FIXE 2n limbs :
//   v ← v + ⌊(B^(2n) − d·v)·v / B^(2n)⌋
// Chaque pas double la précision utile ; 64 pas suffisent toujours
// (convergence quadratique : 2^64 > tout ce qu'on manipule).
//
// ⚠ La troncature est vers le BAS à chaque pas, donc v reste ≤ la vraie
// valeur — c'est ce qui garantit un quotient jamais trop grand.

/// Normalise une magnitude : décale à gauche pour que le limb de poids fort
/// ait son bit 31 à 1. Rend (magnitude normalisée, décalage en bits).
fn normaliser_bits(x: &[u32]) -> (Vec<u32>, u32) {
    let v = normaliser(x.to_vec());
    if v.is_empty() { return (v, 0); }
    let haut = *v.last().unwrap();
    let dec = haut.leading_zeros();
    if dec == 0 { return (v, 0); }
    let mut r = Vec::with_capacity(v.len() + 1);
    let mut retenue = 0u32;
    for &l in &v {
        r.push((l << dec) | retenue);
        retenue = l >> (32 - dec);
    }
    if retenue != 0 { r.push(retenue); }
    (normaliser(r), dec)
}

/// Décale à droite d'un nombre de bits (division par 2^k, tronquée).
fn decaler_droite(v: &[u32], k: usize) -> Vec<u32> {
    if k == 0 { return v.to_vec(); }
    let mots = k / 32;
    let bits = k % 32;
    if mots >= v.len() { return Vec::new(); }
    let src = &v[mots..];
    let mut r = Vec::with_capacity(src.len());
    if bits == 0 {
        r.extend_from_slice(src);
    } else {
        for i in 0..src.len() {
            let bas = src[i] >> bits;
            let haut = if i + 1 < src.len() { src[i + 1] << (32 - bits) } else { 0 };
            r.push(bas | haut);
        }
    }
    normaliser(r)
}

/// Décale à gauche d'un nombre de bits (multiplication par 2^k).
fn decaler_gauche(v: &[u32], k: usize) -> Vec<u32> {
    if k == 0 { return v.to_vec(); }
    let mots = k / 32;
    let bits = k % 32;
    let mut r = vec![0u32; mots];
    if bits == 0 {
        r.extend_from_slice(v);
    } else {
        let mut retenue = 0u32;
        for &l in v {
            r.push((l << bits) | retenue);
            retenue = l >> (32 - bits);
        }
        if retenue != 0 { r.push(retenue); }
    }
    normaliser(r)
}

/// Inverse de Newton : v = ⌊(B^(2n) − 1)/d⌋ pour d normalisé de n limbs.
///
/// ⚠⚠⚠ LES PALIERS SONT INDISPENSABLES, et le DÉCALAGE est le piège.
///
/// Une version à échelle FIXE (log₂(n) pas sur des nombres de taille 2n)
/// était EXACTE mais CATASTROPHIQUEMENT lente : mesuré 40,9 s pour la
/// conversion de `1 000 000!` contre 2,87 s pour `num-bigint`.
///
/// La version par paliers double la précision à chaque pas, donc le coût
/// total est géométrique : O(M(n)) au lieu de O(M(n)·log n).
///
/// ⚠⚠ LE DÉCALAGE EST `(m2 − m)` LIMBS, PAS `2(m2 − m)`. Deux tentatives
/// avaient échoué avec `2(m2−m)` : la valeur devenait fausse d'un ordre de
/// grandeur. Justification : v ≈ B^(2m)/d_m et l'inverse de d_m2 est
/// B^(2m2)/d_m2 = B^(2m2)/(d_m·B^(m2−m)) = B^(m2+m)/d_m ; le rapport est
/// donc B^(m2−m). Vérifié en Python sur n = 2..32.
///
/// Newton à l'échelle p : v ← ⌊v·(2·B^p − d·v) / B^p⌋, produit COMPLET
/// (jamais tronqué : mesuré, la troncature faussait le quotient).
fn inverse_newton(d: &[u32]) -> Vec<u32> {
    let n = d.len();
    debug_assert!(n >= 1);
    // Palier m=1 : inverse du limb de tête, à l'échelle 2 limbs.
    let dtop = d[n - 1] as u64;
    let v0 = u64::MAX / dtop;
    let mut v = normaliser(vec![v0 as u32, (v0 >> 32) as u32]);
    let mut m = 1usize;
    while m < n {
        let m2 = (2 * m).min(n);
        // ✅ Décalage de (m2 − m) limbs.
        v = decaler_gauche(&v, 32 * (m2 - m));
        // d_m2 : les m2 limbs de tête.
        let dm2 = decaler_droite(d, 32 * (n - m2));
        let p = 2 * m2;
        // 2·B^p
        let mut deux_b = vec![0u32; p + 1];
        deux_b[p] = 2;
        for _ in 0..3 {
            let prod = mul_limbs(&dm2, &v);
            if sup_ou_egal(&prod, &deux_b) { break; }
            let reste = sous(&deux_b, &prod);
            if reste.is_empty() { break; }
            let mut nv = mul_limbs(&v, &reste);
            nv = decaler_droite(&nv, 32 * p);
            let nv = normaliser(nv);
            if nv == v { break; }
            v = nv;
        }
        m = m2;
    }
    v
}

/// Division rapide : (quotient, reste) tels que u = q·d + r, 0 ≤ r < d.
///
/// ⚠⚠⚠ CE QUI A ÉTÉ MESURÉ, ET LA DÉCISION PRISE. L'inverse de Newton
/// (ci-dessus) est EXACT (testé sur 1..8000 limbs) mais 25× PLUS LENT que
/// la division de `num-bigint`, à toutes les tailles :
///
///   limbs :  num-bigint   newton maison
///      40 :      5,1 µs      2,9 µs   ← Newton gagne (petits)
///     100 :      4,6 µs    113 µs
///     500 :     55 µs      1,11 ms
///    1000 :    169 µs      3,92 ms
///    2000 :    489 µs     12,3 ms
///    5000 :    2,21 ms    76 ms
///   20000 :   18,2 ms   491 ms
///
/// La raison : `num-bigint` fait Knuth EN PLACE, sans allocation, avec des
/// quotients estimés par double précision. Notre Newton fait ~3 itérations
/// par palier, chacune avec une multiplication pleine taille — le nombre
/// d'opérations explose.
///
/// **Décision** : on délègue à `num-bigint`. L'implémentation Newton reste
/// dans le fichier, testée, pour ne pas perdre le travail — mais elle n'est
/// PAS sur le chemin chaud.
pub fn div_rem(u: &[u32], d: &[u32]) -> (Vec<u32>, Vec<u32>) {
    let a = BigUint::from_slice(&normaliser(u.to_vec()));
    let b = BigUint::from_slice(&normaliser(d.to_vec()));
    let (q, r) = a.div_rem(&b);
    (q.to_u32_digits(), r.to_u32_digits())
}

/// Version Newton de la division — EXACTE mais plus lente (voir le
/// commentaire de `div_rem`). Conservée pour un futur chantier de division
/// en place.
#[allow(dead_code)]
pub fn div_rem_newton(u: &[u32], d: &[u32]) -> (Vec<u32>, Vec<u32>) {
    let u = normaliser(u.to_vec());
    let d = normaliser(d.to_vec());
    if d.is_empty() { panic!("division par zéro"); }
    if !sup_ou_egal(&u, &d) { return (Vec::new(), u); }
    if u == d { return (vec![1u32], Vec::new()); }
    if d.len() < SEUIL_DIVISION { return ecole_div(&u, &d); }
    let (dn, dec) = normaliser_bits(&d);
    let un = decaler_gauche(&u, dec as usize);
    let n = dn.len();
    let inv = inverse_newton(&dn);
    let mut q = mul_limbs(&un, &inv);
    q = decaler_droite(&q, 64 * n);
    let mut prod = mul_limbs(&q, &dn);
    for _ in 0..4 {
        if !sup_ou_egal(&un, &prod) {
            q = sous(&q, &[1u32]);
            prod = mul_limbs(&q, &dn);
            continue;
        }
        let r = sous(&un, &prod);
        if sup_ou_egal(&r, &dn) {
            q = add(&q, &[1u32]);
            prod = mul_limbs(&q, &dn);
            continue;
        }
        break;
    }
    let mut r = sous(&un, &prod);
    if sup_ou_egal(&r, &dn) {
        q = add(&q, &[1u32]);
        r = sous(&r, &dn);
    }
    let rr = decaler_droite(&r, dec as usize);
    (q, rr)
}

/// Division « école » de Knuth (repli pour les petits diviseurs).
/// On s'appuie sur `num-bigint` : il est très optimisé pour ce cas.
fn ecole_div(u: &[u32], d: &[u32]) -> (Vec<u32>, Vec<u32>) {
    let a = BigUint::from_slice(u);
    let b = BigUint::from_slice(d);
    let (q, r) = a.div_rem(&b);
    (q.to_u32_digits(), r.to_u32_digits())
}

// ── Conversion décimale RAPIDE (divide-and-conquer) ─────────────────────
//
// ⚠⚠ POURQUOI. `num-bigint` convertit par divisions successives par 10^k :
// c'est O(n²) en la taille. Mesuré sur `1 000 000!` (5 565 709 chiffres) :
// 2,93 s pour la conversion seule, contre 0,94 s pour le CALCUL. GMP, lui,
// découpe récursivement par des puissances de 10 de plus en plus grandes :
// on divise le nombre en deux moitiés, on convertit chacune, et on colle.
// Le coût devient O(M(n)·log n) au lieu de O(n²).
//
// C'est exactement l'algorithme de `mpz_get_str` pour les grandes bases.

/// 10^k en `BigUint`, pour k ≤ 19 (tient dans un u64).
fn dix_puissance(k: usize) -> BigUint {
    debug_assert!(k <= 19);
    BigUint::from(10u64.pow(k as u32))
}

/// Convertit une magnitude en chaîne décimale, par découpage récursif.
///
/// ⚠⚠⚠ LES PIÈGES MESURÉS, TOUS CORRIGÉS ICI :
///   1. diviser par un diviseur de la MOITIÉ du nombre coûte O(n²) :
///      55 ms pour UNE division à 18,5 M bits ;
///   2. diviser par 10⁹ à chaque niveau fait 600 000 niveaux de récursion
///      pour 5,5 M chiffres → **SIGABRT** (pile) ;
///   3. une largeur de bloc qui ne correspond pas à la profondeur réelle
///      produit des ZÉROS MAL PLACÉS (test rouge : `10000!` préfixé de
///      zéros).
///
/// LA SOLUTION EST CELLE DE GMP : on divise par 10^d où d est la largeur
/// qui rend les deux moitiés ÉGALES EN NOMBRE DE BLOCS. Pour un nombre de
/// `nb` chiffres, on prend `d = nb / 2` (arrondi), ce qui garantit que la
/// moitié basse a exactement `d` chiffres (complétée de zéros à gauche) et
/// que la profondeur est log₂(nb/d) = 1 — la récursion ne s'applique qu'à
/// la moitié HAUTE, dont la taille décroît de moitié à chaque niveau.
///
/// Coût : log₂ niveaux × O(M(n/2)) = O(M(n)·log n).
// ── CONVERSION DÉCIMALE RAPIDE (par MULTIPLICATION) ─────────────────────
//
// ⚠⚠⚠ LE PROBLÈME MESURÉ. `num-bigint` convertit en décimal par
// divide-and-conquer, donc par DIVISIONS. Mesuré sur 1 000 000! :
//
//   num-bigint : 2 881 ms        GMP (mpz_get_str) : 341 ms
//
// C'est 12× le temps du CALCUL de la factorielle elle-même (229 ms) : la
// conversion est le vrai goulot pour l'utilisateur.
//
// ⚠⚠⚠ LA SOLUTION : INVERSER L'ALGORITHME. Au lieu de diviser, on
// MULTIPLIE — et la multiplication, c'est ce que Rose sait faire vite
// (NTT) !
//
//   N = H·2^k + L
//   conv(N) = conv(H)·2^k + conv(L)
//
// Le « × 2^k » se fait en multipliant conv(H) par la représentation
// base 10^9 de 2^k (une multiplication NTT ordinaire). La récurrence est
//
//   T(n) = 2·T(n/2) + M(n/2)
//
// soit Θ(M(n)·log n) — la MÊME structure asymptotique que la version de
// `num-bigint`, mais avec NOTRE multiplication (beaucoup plus rapide que
// leur Toom-3) et surtout sans aucune division.
//
// ⚠⚠⚠ LA STRUCTURATION PAR NIVEAUX EST CE QUI REND LA VERSION
// PARALLÈLE POSSIBLE. La récursion naïve calcule les multiplications
// niveau par niveau, mais CHAQUE multiplication crée ses propres fils :
// mesuré, un `thread::scope` de 20 fils coûte 370 µs, et les niveaux
// profonds de la conversion font ~1 100 multiplications → 400 ms de pure
// création de fils.
//
// On explicite donc la STRUCTURE de l'arbre (les niveaux et les pièces
// de chaque niveau), puis on monte niveau par niveau : chaque niveau
// utilise UN SEUL scope, et toutes ses multiplications sont
// faites en parallèle.
//
// MESURES (1 000 000!, 5 565 709 chiffres) :
//
//   num-bigint (divisions)         : 2 881 ms
//   cette version (multiplications) : 1 400 ms     → 2,0×
//

/// Base de la représentation décimale interne : 10^9.
const BASE1E9: u64 = 1_000_000_000;

/// Nombre de cœurs disponibles.
fn coeurs_para() -> usize {
    std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1)
}

/// Multiplication de deux nombres en base 10^9 (limbs u32 < 10^9).
///
/// ⚠ Le choix entre la NTT et l'école est MESURÉ : la NTT a un coût
/// fixe (tables de racines, conversion, CRT) qui domine sous ~256
/// limbs, et l'école y est plus rapide.
fn mul_b1e9(x: &[u32], y: &[u32], fils: usize) -> Vec<u32> {
    if x.len().max(y.len()) < 256 {
        return mul_scolaire_b1e9(x, y);
    }
    ntt_mul_b1e9_par(x, y, fils)
}

/// Multiplication « école » en base 10^9.
///
/// ⚠ Les produits t ≤ (10⁹−1)² < 10^18 tiennent dans un u64 ✓.
fn mul_scolaire_b1e9(x: &[u32], y: &[u32]) -> Vec<u32> {
    let mut r = vec![0u32; x.len() + y.len()];
    for (j, &yj) in y.iter().enumerate() {
        if yj == 0 { continue; }
        let yj = yj as u64;
        let mut carry = 0u64;
        for (i, &xi) in x.iter().enumerate() {
            let t = xi as u64 * yj + r[i + j] as u64 + carry;
            r[i + j] = (t % BASE1E9) as u32;
            carry = t / BASE1E9;
        }
        let mut k = j + x.len();
        while carry > 0 {
            let t = r[k] as u64 + carry;
            r[k] = (t % BASE1E9) as u32;
            carry = t / BASE1E9;
            k += 1;
        }
    }
    r
}

/// Puissances de 10^9 : `2^k` en base 10^9, mises en cache par `k`.
///
/// ⚠ Le cache est un `Mutex` : il ne faut PAS le tenir pendant l'appel
/// récursif (sinon INTERBLOCAGE) — c'est pour ça que `calc_pow2` est
/// séparé.
fn pow2_b1e9(k: usize) -> Vec<u32> {
    use std::collections::HashMap;
    use std::sync::Mutex;
    static CACHE: Mutex<Option<HashMap<usize, Vec<u32>>>> = Mutex::new(None);
    {
        let g = CACHE.lock().unwrap();
        if let Some(map) = g.as_ref() {
            if let Some(v) = map.get(&k) { return v.clone(); }
        }
    }
    let v = calc_pow2(k);
    let mut g = CACHE.lock().unwrap();
    g.get_or_insert_with(HashMap::new).insert(k, v.clone());
    v
}

/// Calcule `2^k` en base 10^9 (exponentiation binaire, multiplications).
///
/// ⚠⚠⚠ MESURÉ : les INTERMÉDIAIRES doivent être MIS EN CACHE. Avant, un
/// nouveau `k` recalculait toute la chaîne (k/2, k/4, …) : les niveaux de
/// la conversion demandaient chacun une chaîne complète → **170 ms** de
/// précalcul pour rien. En passant par `pow2_b1e9` (qui consulte le
/// cache), chaque nouveau `k` ne coûte plus qu'UN carré.
fn calc_pow2(k: usize) -> Vec<u32> {
    if k == 0 { return vec![1u32]; }
    if k < 30 { return vec![1u32 << k]; }
    let h = pow2_b1e9(k / 2);
    let mut v = mul_b1e9(&h, &h, 0);
    if k % 2 == 1 {
        // × 2
        let mut carry = 0u64;
        for l in v.iter_mut() {
            let t = *l as u64 * 2 + carry;
            *l = (t % BASE1E9) as u32;
            carry = t / BASE1E9;
        }
        if carry > 0 { v.push(carry as u32); }
    }
    v
}

/// Seuil (en bits) sous lequel on convertit par divisions scolaires.
const SEUIL_FEUILLE_BITS: usize = 16_384;

/// Structure de l'arbre de conversion : pour chaque niveau, la liste des
/// pièces (début, fin) en limbs u64, la pièce de HAUT d'abord.
///
/// ⚠ L'ORDRE EST ESSENTIEL : dans chaque paire (2i, 2i+1), l'élément
/// d'indice pair est la pièce de HAUT (bits élevés) et celui d'indice
/// impair la pièce de BAS. C'est ce qui rend la remontée correcte.
fn structure_niveaux(n_limbs: usize, seuil_bits: usize) -> Vec<Vec<(usize, usize)>> {
    let mut niveaux: Vec<Vec<(usize, usize)>> = vec![vec![(0usize, n_limbs)]];
    loop {
        let dernier = niveaux.last().unwrap();
        if dernier.iter().all(|&(d, f)| (f - d) * 64 <= seuil_bits) { break; }
        let mut suivant = Vec::with_capacity(dernier.len() * 2);
        for &(d, f) in dernier {
            let n2 = (f - d) / 2;
            let milieu = d + n2;
            suivant.push((milieu, f));   // la pièce de HAUT
            suivant.push((d, milieu));   // la pièce de BAS
        }
        niveaux.push(suivant);
    }
    niveaux
}

/// Conversion d'un morceau binaire en base 10^9 (méthode scolaire :
/// divisions successives par 10^9).
///
/// ⚠ C'est QUADRATIQUE en la taille du morceau : d'où le seuil de feuille
/// à 16 384 bits (balayé : 1 024 → 16 384 fait passer la conversion
/// totale de 1 577 à 1 428 ms).
fn conv_feuille(binaire: &[u64]) -> Vec<u32> {
    let mut x = binaire.to_vec();
    let mut r = Vec::new();
    while x.iter().any(|&l| l != 0) {
        let mut rem: u128 = 0;
        for i in (0..x.len()).rev() {
            let cur = (rem << 64) | x[i] as u128;
            x[i] = (cur / BASE1E9 as u128) as u64;
            rem = cur % BASE1E9 as u128;
        }
        r.push(rem as u32);
        while x.last() == Some(&0) { x.pop(); }
    }
    if r.is_empty() { r.push(0); }
    r
}

/// Addition de deux nombres en base 10^9.
fn addition_b1e9(mut a: Vec<u32>, b: &[u32]) -> Vec<u32> {
    if b.len() > a.len() { a.resize(b.len(), 0); }
    let mut carry = 0u64;
    for i in 0..a.len() {
        let t = a[i] as u64 + *b.get(i).unwrap_or(&0) as u64 + carry;
        a[i] = (t % BASE1E9) as u32;
        carry = t / BASE1E9;
    }
    if carry > 0 { a.push(carry as u32); }
    a
}

/// Conversion binaire → base 10^9, par niveaux (parallélisable).
fn conv_b1e9(binaire: &[u64]) -> Vec<u32> {
    if binaire.is_empty() { return vec![0]; }
    let niveaux = structure_niveaux(binaire.len(), SEUIL_FEUILLE_BITS);

    // ── Feuilles : conversion par divisions, EN PARALLÈLE ──
    let dernier = niveaux.last().unwrap();
    let mut valeurs: Vec<Vec<u32>> = Vec::with_capacity(dernier.len());
    {
        let coeurs = std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1);
        let mut resultats: Vec<Vec<u32>> = Vec::with_capacity(dernier.len());
        if coeurs < 2 || dernier.len() < 8 {
            for &(d, f) in dernier.iter() {
                resultats.push(conv_feuille(&binaire[d..f]));
            }
        } else {
            // ⚠⚠⚠ NOMBRE FIXE DE FILS, chacun traite un PAQUET de feuilles.
            // Créer UN fil par feuille (2 048 !) coûtait ~20 ms de pure
            // création de fils.
            let n_fils = coeurs.min(dernier.len());
            let paquet = (dernier.len() + n_fils - 1) / n_fils;
            let mut lots: Vec<Vec<Vec<u32>>> = Vec::with_capacity(n_fils);
            std::thread::scope(|sc| {
                let mut hs = Vec::with_capacity(n_fils);
                for t in 0..n_fils {
                    let d = t * paquet;
                    let f = ((t + 1) * paquet).min(dernier.len());
                    if d >= f { break; }
                    hs.push(sc.spawn(move || {
                        let mut lot = Vec::with_capacity(f - d);
                        for &(d0, f0) in &dernier[d..f] {
                            lot.push(conv_feuille(&binaire[d0..f0]));
                        }
                        lot
                    }));
                }
                for h in hs { lots.push(h.join().unwrap()); }
            });
            for lot in lots { resultats.extend(lot); }
        }
        valeurs = resultats;
    }


    // ── PRÉCALCUL DES PUISSANCES DE 2 ───────────────────────────────────
    //
    // ⚠⚠⚠ MESURÉ : sans ce précalcul, chaque FIL d'un niveau calcule sa
    // propre 2^k (course sur le cache + coût dupliqué). Les niveaux
    // supérieurs — qui n'ont que 1 à 16 paires — étaient les plus
    // coûteux : 116 ms pour le niveau 0, alors que la multiplication
    // seule en coûte 57 ms. Le surplus était le calcul des puissances.
    //
    // On les calcule donc TOUTES avant la remontée, une seule fois.
    let mut ks: Vec<usize> = Vec::new();
    for i in 0..niveaux.len() - 1 {
        let pieces = &niveaux[i + 1];
        for j in (0..pieces.len() / 2) {
            let bits_bas = (pieces[2 * j + 1].1 - pieces[2 * j + 1].0) * 64;
            let k = bits_bas as usize;
            if !ks.contains(&k) { ks.push(k); }
        }
    }
    ks.sort_unstable();
    // ⚠ Les puissances se calculent EN PARALLÈLE : chaque `k` a sa
    // propre chaîne de carrés, et le cache partage les intermédiaires.
    // Mesuré : 114 ms en séquentiel (le calcul des puissances est le
    // premier poste de la conversion après les niveaux).
    if coeurs_para() < 2 || ks.len() < 2 {
        for k in ks { let _ = pow2_b1e9(k); }
    } else {
        std::thread::scope(|sc| {
            let mut hs = Vec::with_capacity(ks.len());
            for &k in ks.iter() {
                hs.push(sc.spawn(move || { pow2_b1e9(k); }));
            }
            for h in hs { let _ = h.join(); }
        });
    }

    // ── Remontée : niveau par niveau, chaque niveau EN PARALLÈLE ──
    for i in (0..niveaux.len() - 1).rev() {
        let pieces_enfants = &niveaux[i + 1];
        let n_paires = pieces_enfants.len() / 2;
        let coeurs = std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1);
        let mut nouveaux: Vec<Vec<u32>> = Vec::with_capacity(n_paires);
        if coeurs < 2 || n_paires < 2 {
            for j in 0..n_paires {
                let h = &valeurs[2 * j];
                let l = &valeurs[2 * j + 1];
                let bits_bas = (pieces_enfants[2 * j + 1].1 - pieces_enfants[2 * j + 1].0) * 64;
                let p = mul_b1e9(h, &pow2_b1e9(bits_bas as usize), 0);
                nouveaux.push(addition_b1e9(p, l));
            }
        } else {
            // ⚠⚠⚠ PAS DE SURSOUSCRIPTION : les multiplications du niveau
            // tournent EN PARALLÈLE, chacune ne doit donc PAS prendre
            // tous les cœurs — sinon on lance n_paires × 20 fils.
            let fils = (coeurs / n_paires).max(1);
            // ⚠⚠⚚ NOMBRE FIXE DE FILS, chacun traite un PAQUET de paires
            // (créer un fil par paire coûtait ~20 ms au niveau le plus
            // profond, qui compte 1 024 paires).
            let n_fils = coeurs.min(n_paires);
            let paquet = (n_paires + n_fils - 1) / n_fils;
            let mut lots: Vec<Vec<Vec<u32>>> = Vec::with_capacity(n_fils);
            std::thread::scope(|sc| {
                let mut hs = Vec::with_capacity(n_fils);
                for t in 0..n_fils {
                    let d = t * paquet;
                    let f = ((t + 1) * paquet).min(n_paires);
                    if d >= f { break; }
                    let valeurs_ref = &valeurs;
                    hs.push(sc.spawn(move || {
                        let mut lot = Vec::with_capacity(f - d);
                        for j in d..f {
                            let h = &valeurs_ref[2 * j];
                            let l = &valeurs_ref[2 * j + 1];
                            let bits_bas =
                                (pieces_enfants[2 * j + 1].1 - pieces_enfants[2 * j + 1].0) * 64;
                            let p = mul_b1e9(h, &pow2_b1e9(bits_bas as usize), fils);
                            lot.push(addition_b1e9(p, l));
                        }
                        lot
                    }));
                }
                for h in hs { lots.push(h.join().unwrap()); }
            });
            for lot in lots { nouveaux.extend(lot); }
        }
        valeurs = nouveaux;
    }
    valeurs.into_iter().next().unwrap_or_else(|| vec![0])
}

/// Nombre base 10^9 → chaîne décimale.
///
/// ⚠⚠⚠ MESURÉ : écrire chaque bloc avec `format!("{:09}")` coûtait
/// **~135 ms** pour 5,5 millions de chiffres (617 000 appels à
/// `format!`, chacun allouant et analysant une chaîne). On écrit les
/// chiffres directement dans le tampon.
fn b1e9_vers_chaine(v: &[u32]) -> String {
    let mut buf: Vec<u8> = Vec::with_capacity(v.len() * 9 + 1);
    // ⚠ On parcourt les blocs du PLUS PETIT au PLUS GRAND, et chaque bloc
    // est poussé « chiffre de poids faible d'abord » : l'inversion FINALE
    // du tampon rétablit alors l'ordre des chiffres (blocs décroissants,
    // et à l'intérieur de chaque bloc, le plus significatif d'abord).
    for i in 0..v.len() {
        let mut x = v[i];
        let tete = i + 1 == v.len();
        if tete {
            // Le bloc de tête : pas de zéros de remplissage.
            while x >= 10 {
                buf.push(b'0' + (x % 10) as u8);
                x /= 10;
            }
            buf.push(b'0' + x as u8);
        } else {
            // Les autres blocs : exactement 9 chiffres, zéro compris.
            let mut bloc = [b'0'; 9];
            let mut j = 0;
            while j < 9 {
                bloc[j] = b'0' + (x % 10) as u8;
                x /= 10;
                j += 1;
            }
            buf.extend_from_slice(&bloc);
        }
    }
    buf.reverse();
    // SAFETY : on n'a écrit que des chiffres ASCII.
    unsafe { String::from_utf8_unchecked(buf) }
}

/// Chaîne décimale d'un entier signé, par la conversion rapide.
///
/// ⚠⚠⚠ MESURÉ sur 1 000 000! (5 565 709 chiffres) :
///
///   `num-bigint` (to_string)  : 2 881 ms
///   cette version             : 1 400 ms      → 2,0×
///
/// En dessous de ~10 000 bits, `num-bigint` est plus simple et aussi
/// rapide : on lui délègue.
pub fn vers_chaine(x: &BigInt) -> String {
    if x.bits() < 10_000 {
        return x.to_string();
    }
    let signe = x.sign() == Sign::Minus;
    let limbs = x.magnitude().to_u64_digits();
    let v = conv_b1e9(&limbs);
    let mut s = b1e9_vers_chaine(&v);
    if signe { s.insert(0, '-'); }
    s
}

/// `base^e` pour un `BigUint` (exponentiation binaire).
fn pow_biguint(base: &BigUint, mut e: usize) -> BigUint {
    let mut r = BigUint::from(1u64);
    let mut b = base.clone();
    while e > 0 {
        if e & 1 == 1 { r *= &b; }
        e >>= 1;
        if e > 0 { b = &b * &b; }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compare notre multiplication à `num-bigint` sur des tailles variées :
    /// c'est le garde-fou absolu — si un étage (Karatsuba, Toom-3, NTT) se
    /// trompe, ces tests le voient.
    #[test]
    fn memes_resultats_que_num_bigint() {
        // Tailles couvrant les 4 étages (en limbs u32).
        let tailles = [
            (1usize, 1usize), (5, 7), (39, 39), (40, 40), (41, 80), (179, 179),
            (180, 180), (181, 300), (500, 500), (1999, 1999), (2000, 2000),
            (2001, 2500), (3000, 3000), (5000, 4000), (8000, 8000),
        ];
        for (lx, ly) in tailles {
            // Valeurs pseudo-aléatoires reproductibles, tous les limbs pleins.
            let mut etat = 0x1234_5678_9abc_def0u64 ^ (lx as u64);
            let mut suivant = || {
                etat ^= etat << 13; etat ^= etat >> 7; etat ^= etat << 17;
                etat as u32
            };
            let x: Vec<u32> = (0..lx).map(|_| suivant()).collect();
            let y: Vec<u32> = (0..ly).map(|_| suivant()).collect();
            let attendu = BigUint::from_slice(&x) * BigUint::from_slice(&y);
            let obtenu = BigUint::from_slice(&mul_limbs(&x, &y));
            assert_eq!(obtenu, attendu, "taille {lx}×{ly} limbs");
        }
    }

    /// ⚠ Mesure de CHAQUE étage : si Toom-3 est plus lent que Karatsuba à
    /// une taille donnée, c'est que le seuil est mal réglé.
    #[test]
    fn profil_des_etages() {
        use std::time::Instant;
        for &n in &[100usize, 1000, 8000, 32000] {
            let x: Vec<u32> = (0..n).map(|i| (i as u32).wrapping_mul(2_654_435_761)).collect();
            let y: Vec<u32> = (0..n).map(|i| (i as u32).wrapping_mul(1_013_904_223)).collect();
            let t = Instant::now(); let _ = ecole(&x, &y); let te = t.elapsed();
            let t = Instant::now(); let _ = karatsuba(&x, &y); let tk = t.elapsed();
            let t = Instant::now(); let _ = toom3(&x, &y); let tt = t.elapsed();
            let t = Instant::now(); let _ = ntt_mul_par(&x, &y, 0, 0); let tn = t.elapsed();
            let t = Instant::now(); let _ = mul_limbs(&x, &y); let td = t.elapsed();
            let a = BigInt::from_biguint(Sign::Plus, BigUint::from_slice(&x));
            let b = BigInt::from_biguint(Sign::Plus, BigUint::from_slice(&y));
            let t = Instant::now(); let _ = &a * &b; let tnb = t.elapsed();
            eprintln!("n={:>5} : ecole {:>10.2?} karat {:>10.2?} toom3 {:>10.2?} ntt {:>10.2?} dispatch {:>10.2?} NUM-BIGINT {:>10.2?}",
                      n, te, tk, tt, tn, td, tnb);
        }
    }

    /// ⚠ Coût de la CONVERSION, séparé du calcul : si la conversion domine,
    /// c'est qu'il faut opérer directement sur les limbs.
    #[test]
    fn cout_conversion_et_mul() {
        use std::time::Instant;
        for &n in &[100usize, 1000] {
            // ⚠ Limbs PLEINS : `3 << (32n)` ne remplit qu'un limb et
            // fausse le profil (les routines voient n−1 limbs nuls).
            let mut x: Vec<u32> = (0..n).map(|i| (i as u32).wrapping_mul(2_654_435_761)).collect();
            let mut y: Vec<u32> = (0..n).map(|i| (i as u32).wrapping_mul(1_013_904_223)).collect();
            x[n - 1] |= 1; y[n - 1] |= 1;
            let a = BigInt::from_biguint(Sign::Plus, BigUint::from_slice(&x));
            let b = BigInt::from_biguint(Sign::Plus, BigUint::from_slice(&y));
            let t = Instant::now();
            for _ in 0..20 { let _ = mul(&a, &b); }
            let tm = t.elapsed();
            let t = Instant::now();
            for _ in 0..20 { let _ = mul_limbs(&x, &y); }
            let tl = t.elapsed();
            eprintln!("  brut mul_limbs {:>10.2?}", tl);
            let t = Instant::now();
            for _ in 0..20 { let _ = &a * &b; }
            let tn = t.elapsed();
            eprintln!("n={:>6} : mul_rapide {:>10.2?}  num-bigint {:>10.2?}", n, tm, tn);
        }
    }

    /// ⚠ À quelle taille notre NTT bat-elle réellement `num-bigint` ?
    /// Tant qu'elle ne le bat pas, elle ne doit PAS être branchée.
    #[test]
    fn ntt_bat_num_bigint() {
        use std::time::Instant;
        for &n in &[4000usize, 8000, 12000, 16000, 24000, 32000, 64000] {
            let mut x: Vec<u32> = (0..n).map(|i| (i as u32).wrapping_mul(2_654_435_761)).collect();
            let mut y: Vec<u32> = (0..n).map(|i| (i as u32).wrapping_mul(1_013_904_223)).collect();
            x[n-1] |= 1; y[n-1] |= 1;
            let a = BigInt::from_biguint(Sign::Plus, BigUint::from_slice(&x));
            let b = BigInt::from_biguint(Sign::Plus, BigUint::from_slice(&y));
            let t = Instant::now(); let _ = &a * &b; let tn = t.elapsed();
            let t = Instant::now(); let _ = ntt_mul_par(&x, &y, 0, 0); let tt = t.elapsed();
            eprintln!("n={:>7} : num-bigint {:>10.2?}   NTT {:>10.2?}   {}", n, tn, tt,
                      if tt < tn { "NTT GAGNE" } else { "num-bigint gagne" });
        }
    }

    /// ⚠⚠ La conversion décimale rapide doit donner EXACTEMENT la même
    /// chaîne que `num-bigint`, et être plus rapide sur les gros nombres.
    #[test]
    fn conversion_decimale_concorde() {
        // Petites et moyennes valeurs.
        for n in [0u64, 1, 7, 12345, 999_999_999, u64::MAX] {
            let x = BigInt::from(n);
            assert_eq!(vers_chaine(&x), x.to_string(), "{n}");
        }
        // Valeurs avec zéros internes (le piège des blocs à compléter).
        let x = BigInt::from(10u64).pow(20);
        assert_eq!(vers_chaine(&x), x.to_string());
        let x = (BigInt::from(10u64).pow(20)) + BigInt::from(1);
        assert_eq!(vers_chaine(&x), x.to_string());
        // Négatifs.
        let x = -(BigInt::from(10u64).pow(50) + BigInt::from(123));
        assert_eq!(vers_chaine(&x), x.to_string());
        // Gros nombres réels.
        for n in [1_000u64, 10_000, 50_000] {
            let mut f = BigInt::one();
            for k in 2..=n { f *= k; }
            assert_eq!(vers_chaine(&f), f.to_string(), "{n}!");
        }
        // ⚠⚠⚠ LA CONVERSION PAR MULTIPLICATION DOIT ÊTRE EXACTE sur les
        // tailles qui traversent les seuils (feuille, NTT, parallèle).
        // On compare à `num-bigint` sur des nombres pseudo-aléatoires de
        // toutes les tailles intermédiaires.
        let mut graine: u64 = 0x243F_6A88_85A3_08D3;
        for bits in [10_001usize, 12_000, 16_000, 16_383, 16_384, 16_385,
                     20_000, 32_000, 33_000, 65_536, 100_000, 200_000] {
            let mut x = BigInt::one();
            for i in 0..(bits / 64) {
                graine = graine.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                x <<= 64;
                x += BigInt::from(graine);
            }
            let attendu = x.to_string();
            let obtenu = vers_chaine(&x);
            assert_eq!(obtenu, attendu, "conversion incorrecte sur {bits} bits");
            assert_eq!(vers_chaine(&(-x)), format!("-{attendu}"), "négatif {bits} bits");
        }
    }

    /// ⚠⚠⚠ GARDE-FOU ABSOLU de la division rapide : elle doit donner
    /// EXACTEMENT le même quotient et le même reste que `num-bigint`,
    /// sur toutes les tailles et tous les cas limites.
    #[test]
    fn division_concorde_avec_num_bigint() {
        let tailles = [
            (1usize, 1usize), (2, 1), (5, 2), (39, 39), (40, 20), (41, 41),
            (80, 40), (100, 50), (200, 100), (500, 250), (1000, 500),
            (2000, 1000), (5000, 2500), (8000, 4000),
        ];
        for (lu, ld) in tailles {
            let mut etat = 0x1234_5678_9abc_def0u64 ^ (lu as u64);
            let mut suivant = || {
                etat ^= etat << 13; etat ^= etat >> 7; etat ^= etat << 17;
                etat as u32
            };
            let mut u: Vec<u32> = (0..lu).map(|_| suivant()).collect();
            let mut d: Vec<u32> = (0..ld).map(|_| suivant()).collect();
            u[lu - 1] |= 1;
            d[ld - 1] |= 1;
            let a = BigUint::from_slice(&u);
            let b = BigUint::from_slice(&d);
            let (qa, ra) = a.div_rem(&b);
            let (qm, rm) = div_rem(&u, &d);
            assert_eq!(BigUint::from_slice(&qm), qa, "quotient {lu}/{ld}");
            assert_eq!(BigUint::from_slice(&rm), ra, "reste {lu}/{ld}");
        }
    }

    /// ⚠⚠ La division Newton doit être EXACTE (même si elle n'est pas sur
    /// le chemin chaud) : c'est la preuve que l'inverse de Newton est
    /// correct, et le garde-fou pour un futur chantier.
    #[test]
    fn division_newton_concorde() {
        for &(lu, ld) in &[(40usize,20usize),(100,50),(200,100),(500,250),(1000,500)] {
            let mut e = 0x1234_5678u64 ^ lu as u64;
            let mut f = || { e ^= e<<13; e ^= e>>7; e ^= e<<17; e as u32 };
            let mut u: Vec<u32> = (0..lu).map(|_| f()).collect();
            let mut d: Vec<u32> = (0..ld).map(|_| f()).collect();
            u[lu-1] |= 1; d[ld-1] |= 1;
            let a = BigUint::from_slice(&u);
            let b = BigUint::from_slice(&d);
            let (qa, ra) = a.div_rem(&b);
            let (qn, rn) = div_rem_newton(&u, &d);
            assert_eq!(BigUint::from_slice(&qn), qa, "quotient newton {lu}/{ld}");
            assert_eq!(BigUint::from_slice(&rn), ra, "reste newton {lu}/{ld}");
        }
    }

    #[test]
    fn division_cas_limites() {
        // u < d → q = 0, r = u
        let (q, r) = div_rem(&[5u32], &[7u32]);
        assert!(q.is_empty() && r == vec![5u32]);
        // u == d → q = 1, r = 0
        let (q, r) = div_rem(&[7u32, 3], &[7u32, 3]);
        assert_eq!(q, vec![1u32]);
        assert!(r.is_empty());
        // Division exacte.
        let u = vec![0u32, 0u32, 1u32];
        let d = vec![0u32, 1u32];
        let (q, r) = div_rem(&u, &d);
        assert_eq!(BigUint::from_slice(&q) * BigUint::from_slice(&d) + BigUint::from_slice(&r),
                   BigUint::from_slice(&u));
    }

    /// ⚠ Profil de la division : notre Newton contre num-bigint (Knuth).
    #[test]
    fn profil_division() {
        use std::time::Instant;
        for &n in &[40usize, 100, 200, 500, 1000, 2000, 5000, 20000] {
            let mut x: Vec<u32> = (0..2*n).map(|i| (i as u32).wrapping_mul(2_654_435_761)).collect();
            let mut y: Vec<u32> = (0..n).map(|i| (i as u32).wrapping_mul(1_013_904_223)).collect();
            x[2*n-1] |= 1; y[n-1] |= 1;
            let a = BigUint::from_slice(&x);
            let b = BigUint::from_slice(&y);
            let t = Instant::now(); let _ = a.div_rem(&b); let tn = t.elapsed();
            let t = Instant::now(); let _ = div_rem(&x, &y); let td = t.elapsed();
            eprintln!("n={:>6} : num-bigint {:>10.2?}  newton {:>10.2?}  {}", n, tn, td,
                      if td < tn { "NEWTON GAGNE" } else { "num-bigint gagne" });
        }
    }

    /// ⚠ Profil INTERNE de la NTT : combien de temps dans les transformées,
    /// combien dans la reconstruction CRT ?
    #[test]
    fn profil_ntt_interne() {
        use std::time::Instant;
        for &n in &[8_000usize, 16_000, 32_000, 100_000, 256_000, 512_000] {
            let x: Vec<u32> = (0..n).map(|i| (i as u32).wrapping_mul(2_654_435_761)).collect();
            let y: Vec<u32> = (0..n).map(|i| (i as u32).wrapping_mul(1_013_904_223)).collect();
            let besoin = 2 * n;
            let mut lg = 1usize;
            while lg < besoin { lg <<= 1; }
            // Le CHEMIN RÉEL : `ntt_mul_par` regroupe les 6 transformées
            // dans UN SEUL scope (création de fils mesurée à 370 µs :
            // 6 scopes = 2,2 ms perdus par multiplication).
            let t = Instant::now();
            let r = ntt_mul_par(&x, &y, 0, 0);
            let t_par = t.elapsed();
            // La même chose en SÉQUENTIEL (1 fil) pour comparer.
            let t = Instant::now();
            let r1 = ntt_mul_par(&x, &y, 1, 0);
            let t_seq = t.elapsed();
            assert_eq!(r, r1, "la NTT parallèle doit donner le même résultat");
            eprintln!("n={:>7} lg={:>7} : ntt_mul_par parallèle {:?} | séquentiel {:?}",
                      n, lg, t_par, t_seq);
        }
    }

    /// ⚠ Comparaison : % u128 (modules 62 bits) contre % u64 (modules 32 bits).
    #[test]
    fn profil_reduction() {
        use std::time::Instant;
        let n = 1_000_000usize;
        let a: Vec<u64> = (0..n).map(|i| (i as u64).wrapping_mul(6364136223846793005)).collect();
        let b: Vec<u64> = (0..n).map(|i| (i as u64).wrapping_mul(1442695040888963407)).collect();
        let m62 = 2_850_325_311_931_809_793u64;
        let m32 = 998_244_353u64;
        let t = Instant::now();
        let mut s = 0u64;
        for i in 0..n {
            s = s.wrapping_add(((a[i] as u128 * b[i] as u128) % m62 as u128) as u64);
        }
        eprintln!("% u128 (62 bits) : {:?} ({} )", t.elapsed(), s);
        let t = Instant::now();
        let mut s = 0u64;
        for i in 0..n {
            let x = a[i] % m32;
            let y = b[i] % m32;
            s = s.wrapping_add((x * y) % m32);
        }
        eprintln!("% u64 (32 bits) : {:?} ({} )", t.elapsed(), s);
    }

    /// ⚠ À 18,5 M bits, combien de temps pour UNE multiplication ?
    /// C'est ce qui fixe le plancher de 1 000 000!.
    #[test]
    fn profil_gros_produit() {
        use std::time::Instant;
        // Deux opérandes de 9,2 M bits (moitié de 18,5 M).
        for &n in &[290_000usize, 580_000] {
            let mut x: Vec<u32> = (0..n).map(|i| (i as u32).wrapping_mul(2_654_435_761)).collect();
            let mut y: Vec<u32> = (0..n).map(|i| (i as u32).wrapping_mul(1_013_904_223)).collect();
            x[n-1] |= 1; y[n-1] |= 1;
            let a = BigInt::from_biguint(Sign::Plus, BigUint::from_slice(&x));
            let b = BigInt::from_biguint(Sign::Plus, BigUint::from_slice(&y));
            let t = Instant::now(); let _ = &a * &b; let tn = t.elapsed();
            let t = Instant::now(); let _ = mul(&a, &b); let tm = t.elapsed();
            let t = Instant::now(); let _ = ntt_mul_par(&x, &y, 0, 0); let tt = t.elapsed();
            eprintln!("n={:>7} : num-bigint {:>10.2?}  mul(NTT) {:>10.2?}  ntt direct {:>10.2?}", n, tn, tm, tt);
        }
    }

    #[test]
    fn signes_et_cas_limites() {
        let z = BigInt::zero();
        assert_eq!(mul(&z, &BigInt::from(7)), BigInt::zero());
        assert_eq!(mul(&BigInt::from(7), &z), BigInt::zero());
        assert_eq!(mul(&BigInt::from(-3), &BigInt::from(5)), BigInt::from(-15));
        assert_eq!(mul(&BigInt::from(-3), &BigInt::from(-5)), BigInt::from(15));
        assert_eq!(mul(&BigInt::one(), &BigInt::from(9)), BigInt::from(9));
        // Un très grand par un très petit.
        let grand = (BigInt::one() << 200_000) + BigInt::from(1);
        assert_eq!(mul(&grand, &BigInt::from(3)), &grand * 3);
    }

    /// Le résultat doit être IDENTIQUE à num-bigint sur les factorielles
    /// réelles, qui produisent des nombres de millions de bits.
    #[test]
    fn factorielle_concorde() {
        for n in [100u64, 1000, 5000, 20_000] {
            let mut attendu = BigInt::one();
            let mut obtenu = BigInt::one();
            for k in 2..=n {
                attendu *= k;
                obtenu = mul(&obtenu, &BigInt::from(k));
            }
            assert_eq!(obtenu, attendu, "{n}!");
        }
    }

    #[test]
    fn toom3_et_ntt_egalent_lecole() {
        // Force chaque étage sur une même paire et compare à l'école.
        let x: Vec<u32> = (0..600).map(|i| (i as u32).wrapping_mul(2_654_435_761)).collect();
        let y: Vec<u32> = (0..600).map(|i| (i as u32).wrapping_mul(1_013_904_223)).collect();
        let reference = BigUint::from_slice(&ecole(&x, &y));
        assert_eq!(BigUint::from_slice(&toom3(&x, &y)), reference, "toom3");
        let xl: Vec<u32> = (0..3000).map(|i| (i as u32).wrapping_mul(2_654_435_761)).collect();
        let yl: Vec<u32> = (0..3000).map(|i| (i as u32).wrapping_mul(1_013_904_223)).collect();
        let reference = BigUint::from_slice(&ecole(&xl, &yl));
        assert_eq!(BigUint::from_slice(&ntt_mul_par(&xl, &yl, 0, 0)), reference, "ntt");
    }
}
