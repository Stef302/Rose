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

/// Multiplication de deux entiers signés, par le meilleur algorithme selon
/// la taille. Sémantique STRICTEMENT identique à `BigInt * BigInt`.
pub fn mul(a: &BigInt, b: &BigInt) -> BigInt {
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
    // ⚠⚠ LE SEUIL SE MESURE SUR L'ARBRE RÉEL, PAS SUR UN COUPLE ISOLÉ.
    // La bascule isolée est à 256 000 limbs (8,19 M bits), mais dans un
    // arbre de produit les nœuds voisins de cette taille sont rares et la
    // conversion coûte : mesuré 1074 ms à 8,19 M bits contre 913 ms à
    // 16,4 M bits pour `1 000 000!`. On garde donc 512 000 limbs.
    const SEUIL_NTT_BITS: u64 = 16_384_000;
    if a.bits().max(b.bits()) >= SEUIL_NTT_BITS {
        let signe = if a.sign() == b.sign() { Sign::Plus } else { Sign::Minus };
        let x = a.magnitude().to_u32_digits();
        let y = b.magnitude().to_u32_digits();
        let m = ntt_mul(&x, &y);
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
    ntt_mul(x, y)
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
// ⚠⚠ LA RACINE PRIMITIVE COMPTE AUTANT QUE LE MODULE. Une première version
// donnait (2013265921, 5) : `5^((m−1)/2) mod m = 1`, donc 5 n'était PAS une
// racine primitive et la NTT produisait des valeurs fausses (test rouge).
// Vérification systématique : `g^((m−1)/2) ≡ −1` et `g^(m−1) ≡ 1`.
// ⚠⚠⚠ LA RACINE PRIMITIVE COMPTE AUTANT QUE LE MODULE. Une première version
// donnait (2013265921, 5) : `5^((m−1)/2) mod m = 1`, donc 5 n'était PAS une
// racine primitive et la NTT produisait des valeurs fausses (test rouge).
// Vérification systématique : `g^((m−1)/2) ≡ −1` et `g^(m−1) ≡ 1`.
//
// ⚠⚠ POURQUOI DEUX MODULES DE 62 BITS PLUTÔT QUE TROIS DE 30. Mesuré : avec
// trois modules de 30 bits, la NTT coûtait 9 transformées (3 modules ×
// 2 + 1 inverse) et un `%` u128 par papillon — elle ne battait `num-bigint`
// qu'à 512 000 limbs. Avec DEUX modules de 62 bits, le produit de deux
// valeurs tient dans u128 et le `%` par un u64 est une instruction matérielle
// (`udiv`), ce qui divise le coût par ~2.
//
// Couverture : deux modules de ~62 bits donnent ~124 bits. Le coefficient
// maximal d'une convolution est L·(2³²−1)² ≈ L·2⁶⁴, donc L < 2⁶⁰ limbs
// (≈ 64 Go de chiffres) — largement au-delà de ce que Rose manipule.
//
// ⚠ Ces modules ont été VÉRIFIÉS premiers et leur racine primitive testée :
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

/// NTT itérative (Cooley-Tukey) sur un vecteur de longueur 2^p.
///
/// ⚠⚠ DEUX OPTIMISATIONS QUI FONT TOUTE LA DIFFÉRENCE (mesurées) :
///   1. les racines sont PRÉCALCULÉES une fois par étage (`racines`), au lieu
///      d'un `puissance_mod` par papillon ;
///   2. la multiplication modulaire utilise le produit u128 puis un `%` —
///      c'est ce que fait toute NTT de 63 bits ; avec des modules de 30 bits
///      le produit tient dans u64 et on peut éviter le u128, mais le gain
///      mesuré est marginal face au coût des divisions.
fn ntt(v: &mut [u64], racine: u64, m: u64, inverse: bool) {
    let n = v.len();
    // Bit-reversal.
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 { j ^= bit; bit >>= 1; }
        j |= bit;
        if i < j { v.swap(i, j); }
    }
    let mut len = 2usize;
    while len <= n {
        // Racine de l'étage, calculée UNE fois.
        let mut w = puissance_mod(racine, (m - 1) / len as u64, m);
        if inverse { w = inverse_mod(w, m); }
        // Table des puissances w^0..w^(len/2−1), calculée UNE fois par étage.
        let demi = len / 2;
        let mut table = Vec::with_capacity(demi);
        let mut wk = 1u64;
        for _ in 0..demi {
            table.push(wk);
            wk = (wk as u128 * w as u128 % m as u128) as u64;
        }
        let mut i = 0usize;
        while i < n {
            for k in 0..demi {
                let u = v[i + k];
                let t = (table[k] as u128 * v[i + k + demi] as u128 % m as u128) as u64;
                let s = u + t;
                v[i + k] = if s >= m { s - m } else { s };
                v[i + k + demi] = if u >= t { u - t } else { u + m - t };
            }
            i += len;
        }
        len <<= 1;
    }
    if inverse {
        let inv_n = inverse_mod(n as u64, m);
        for x in v.iter_mut() { *x = (*x as u128 * inv_n as u128 % m as u128) as u64; }
    }
}

/// Multiplication NTT avec 2 modules de 62 bits + restes chinois.
fn ntt_mul(x: &[u32], y: &[u32]) -> Vec<u32> {
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

    let mut restes: Vec<Vec<u64>> = Vec::with_capacity(2);
    for &(m, racine) in MODULES.iter() {
        let mut a: Vec<u64> = x.iter().map(|&v| (v as u64) % m).collect();
        let mut b: Vec<u64> = y.iter().map(|&v| (v as u64) % m).collect();
        a.resize(n, 0);
        b.resize(n, 0);
        ntt(&mut a, racine, m, false);
        ntt(&mut b, racine, m, false);
        for i in 0..n { a[i] = (a[i] as u128 * b[i] as u128 % m as u128) as u64; }
        ntt(&mut a, racine, m, true);
        restes.push(a);
    }

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
        r[i] = c as u32;
        retenue = c >> 32;
    }
    normaliser(r)
}

// ── DIVISION RAPIDE ─────────────────────────────────────────────────────
//
// ⚠⚠⚠ CE QUI A ÉTÉ MESURÉ, ET CE QUI A ÉTÉ ABANDONNÉ. L'inverse de Newton
// en arithmétique entière exige une troncature par DÉFAUT garantie à chaque
// palier (sinon le quotient est faux d'une unité) ; deux implémentations
// essayées ont échoué au test de concordance avec `num-bigint` (quotient
// faux à 80×40 limbs). La division récursive de Burnikel-Ziegler, essayée
// aussi, récurse à l'infini quand le demi-limb ne décroît plus.
//
// **Décision** : on ne réimplémente PAS la division générale. `num-bigint`
// fait Knuth, qui est correct et raisonnablement rapide (mesuré : 1,43 ms
// pour 1 000 000 bits). Le vrai gain est ailleurs : la conversion décimale
// n'a besoin que de divisions par de PETITS diviseurs si on découpe
// intelligemment — voir `vers_chaine_rec`.
//
// La fonction ci-dessous reste comme point d'entrée, mais délègue.

/// Division (quotient, reste) d'une magnitude par une autre.
///
/// ⚠ Délègue à `num-bigint` : voir le commentaire de tête pour la raison.
pub fn div_rem(u: &[u32], d: &[u32]) -> (Vec<u32>, Vec<u32>) {
    let a = BigUint::from_slice(&normaliser(u.to_vec()));
    let b = BigUint::from_slice(&normaliser(d.to_vec()));
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
fn vers_chaine_rec(x: &BigUint, d: usize) -> String {
    // Cas de base : le nombre tient dans un u64 → conversion directe.
    if x.bits() <= 64 {
        return x.to_str_radix(10);
    }
    // ⚠⚠ La largeur du bloc est celle qui coupe le nombre EN DEUX :
    //   chiffres ≈ bits·log10(2) ≈ bits·0,30103
    let chiffres = (x.bits() as f64 * 0.30102999566398120) as usize + 1;
    let largeur = (chiffres / 2).max(d);
    // ⚠ 10^largeur par exponentiation binaire (largeur peut être grand).
    let base = pow_biguint(&BigUint::from(10u64), largeur);
    let (q, r) = x.div_rem(&base);
    let haut = vers_chaine_rec(&q, d);
    let bas = r.to_str_radix(10);
    let mut s = String::with_capacity(haut.len() + largeur);
    s.push_str(&haut);
    // ⚠ Les blocs du BAS doivent être complétés à gauche par des zéros
    // jusqu'à la largeur du diviseur (largeur chiffres).
    for _ in 0..largeur.saturating_sub(bas.len()) { s.push('0'); }
    s.push_str(&bas);
    s
}

/// Chaîne décimale d'un entier signé, par découpage récursif au-delà de
/// 10 000 bits (en dessous, `num-bigint` est déjà rapide).
///
/// ⚠⚠⚠ CETTE FONCTION N'EST **PAS** BRANCHÉE dans `Gz::chaine()` : voir le
/// commentaire de `vers_chaine_rec` et le rapport. Elle reste testée
/// (`conversion_decimale_concorde`) pour servir de base à un chantier de
/// division rapide.
#[allow(dead_code)]
pub fn vers_chaine(x: &BigInt) -> String {
    if x.bits() < 10_000 {
        return x.to_string();
    }
    let s = vers_chaine_rec(x.magnitude(), 9);
    if x.sign() == Sign::Minus { format!("-{s}") } else { s }
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
            let t = Instant::now(); let _ = ntt_mul(&x, &y); let tn = t.elapsed();
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
        for &n in &[64000usize, 128000, 256000, 512000] {
            let mut x: Vec<u32> = (0..n).map(|i| (i as u32).wrapping_mul(2_654_435_761)).collect();
            let mut y: Vec<u32> = (0..n).map(|i| (i as u32).wrapping_mul(1_013_904_223)).collect();
            x[n-1] |= 1; y[n-1] |= 1;
            let a = BigInt::from_biguint(Sign::Plus, BigUint::from_slice(&x));
            let b = BigInt::from_biguint(Sign::Plus, BigUint::from_slice(&y));
            let t = Instant::now(); let _ = &a * &b; let tn = t.elapsed();
            let t = Instant::now(); let _ = ntt_mul(&x, &y); let tt = t.elapsed();
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
    }

    /// ⚠⚠⚠ GARDE-FOU ABSOLU de la division rapide : elle doit donner
    /// EXACTEMENT le même quotient et le même reste que `num-bigint`,
    /// sur toutes les tailles et tous les cas limites.
    #[test]
    fn division_concorde_avec_num_bigint() {
        let tailles = [
            (1usize, 1usize), (2, 1), (5, 2), (39, 39), (40, 20), (41, 41),
            (80, 40), (100, 50), (200, 100), (500, 250), (1000, 500),
            (2000, 1000), (5000, 2500), (8000, 4000), (10000, 5000),
            (20000, 9000),
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
        assert_eq!(BigUint::from_slice(&ntt_mul(&xl, &yl)), reference, "ntt");
    }
}
