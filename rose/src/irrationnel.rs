// Rose — constantes irrationnelles exactes et algorithmes de calcul rapide
//
// Ce module implémente les constantes irrationnelles comme des entités
// symboliques exactes (π, e, φ, √n) — jamais des approximations figées —
// avec des algorithmes de calcul rapide pour générer des décimales à la
// demande.
//
// Algorithmes :
//   • π : formule de Machin (π/4 = 4·arctan(1/5) − arctan(1/239)), fiable
//   • e : série de Taylor Σ 1/k!
//   • φ : (1 + √5) / 2, racine carrée par Newton
//   • √n : Newton sur entiers scalés
//
// Toutes les fonctions prennent une précision (nombre de décimales) et
// retournent les digits bruts sans mise en forme. La mise en forme (≈,
// séparateur décimal, arrondi) se fait dans run.rs.

use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{One, Zero, ToPrimitive};

/// Constante irrationnelle symbolique : jamais une valeur finie figée.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConstIrrationnel {
    Pi,          // π
    E,           // e (nombre d'Euler)
    Phi,         // φ (nombre d'or)
    Racine(BigInt), // √n
}

impl ConstIrrationnel {
    /// Nom de la constante pour l'affichage.
    pub fn nom(&self) -> &'static str {
        match self {
            ConstIrrationnel::Pi => "π",
            ConstIrrationnel::E => "e",
            ConstIrrationnel::Phi => "φ",
            ConstIrrationnel::Racine(_) => "√",
        }
    }

    /// Argument (le n de √n, None pour les constantes pures).
    pub fn arg(&self) -> Option<&BigInt> {
        match self {
            ConstIrrationnel::Racine(n) => Some(n),
            _ => None,
        }
    }
}

/// Calcule π à `decimales` décimales par la formule de Machin :
/// π/4 = 4·arctan(1/5) − arctan(1/239)
/// Retourne les digits de π × 10^decimales (entier, sans virgule).
pub fn calcul_pi(decimales: usize) -> BigInt {
    if decimales == 0 {
        return BigInt::from(3);
    }
    let garde = 20usize;
    let total = decimales + garde;
    let scale = BigInt::from(10u64).pow(total as u32);

    // arctan(1/x) = Σ (-1)^k / ((2k+1) * x^(2k+1))
    // calculé en entier : atan_scaled(x) = Σ (-1)^k * scale / ((2k+1) * x^(2k+1))
    let atan5 = atan_scaled(5, &scale, total);
    let atan239 = atan_scaled(239, &scale, total);

    // π = 16 * atan(1/5) - 4 * atan(1/239)
    let mut pi = &atan5 * 16;
    pi = pi - &atan239 * 4;

    // Retirer les décimales de garde
    pi / BigInt::from(10u64).pow(garde as u32)
}

/// arctan(1/x) calculé en entier avec scale = 10^total.
/// Retourne scale * arctan(1/x).
fn atan_scaled(x: u64, scale: &BigInt, total: usize) -> BigInt {
    let x = BigInt::from(x);
    let mut sum = BigInt::zero();
    let mut k = 0u64;
    loop {
        let den = BigInt::from(2 * k + 1) * x.pow((2 * k + 1) as u32);
        let t = scale / &den;
        if t.is_zero() {
            break;
        }
        if k % 2 == 0 {
            sum = &sum + &t;
        } else {
            sum = &sum - &t;
        }
        k += 1;
        if k > 10_000_000 {
            break;
        }
    }
    sum
}

/// Calcul de e à `decimales` décimales par la série de Taylor Σ 1/k!.
/// Retourne e × 10^decimales (entier).
pub fn calcul_e(decimales: usize) -> BigInt {
    if decimales == 0 {
        return BigInt::from(2);
    }

    let scale = BigInt::from(10u64).pow(decimales as u32 + 10);

    // e = Σ 1/k!, on calcule s = Σ scale/k! pour k=0..n
    let mut sum = BigInt::zero();
    let mut fact = BigInt::one();

    for k in 0.. {
        if k > 0 {
            fact = &fact * BigInt::from(k as u64);
        }
        let t = &scale / &fact;
        if t.is_zero() {
            break;
        }
        sum += &t;
        if k > 10_000 {
            break;
        }
    }

    // Retirer les 10 décimales de garde
    sum / BigInt::from(10u64).pow(10)
}

/// Racine carrée entière (isqrt) de n.
fn isqrt(n: &BigInt) -> BigInt {
    if n.is_zero() {
        return BigInt::zero();
    }
    // Approximation initiale par bits
    let mut x = BigInt::one() << ((n.bits() + 1) / 2);
    loop {
        let y = (&x + n / &x) / 2;
        if &y >= &x {
            return x;
        }
        x = y;
    }
}

/// Calcule √n à `decimales` décimales par Newton sur entiers scalés.
/// Retourne √n × 10^decimales (entier).
pub fn calcul_racine(n: &BigInt, decimales: usize) -> BigInt {
    if n.is_zero() {
        return BigInt::zero();
    }
    if n < &BigInt::zero() {
        panic!("racine carrée d'un nombre négatif");
    }

    let scale = BigInt::from(10u64).pow(decimales as u32 + 10);
    let scaled_n = n * &scale * &scale; // n × 10^(2*(decimales+10))

    // Newton : x_{k+1} = (x_k + scaled_n / x_k) / 2
    let mut x = isqrt(&scaled_n);
    if x.is_zero() {
        x = BigInt::one();
    }
    for _ in 0..100 {
        let y = (&x + &scaled_n / &x) / 2;
        if y >= x {
            break;
        }
        x = y;
    }

    // Retirer les 10 décimales de garde
    x / BigInt::from(10u64).pow(10)
}

/// Calcule φ à `decimales` décimales : φ = (1 + √5) / 2.
/// Retourne φ × 10^decimales (entier).
pub fn calcul_phi(decimales: usize) -> BigInt {
    // √5 avec décimales de garde, puis ramener à la bonne échelle
    let sqrt5 = calcul_racine(&BigInt::from(5), decimales + 5)
        / BigInt::from(10u64).pow(5);
    let scale = BigInt::from(10u64).pow(decimales as u32);
    let num = &scale + &sqrt5;
    // φ = (1 + √5) / 2 → num / 2 (scale est déjà dans les deux termes)
    num / 2
}

/// Calcule π à `decimales` décimales par Chudnovsky itératif avec BigRational.
/// ~14 décimales par terme. Retourne π × 10^decimales (entier).
pub fn calcul_pi_chudnovsky(decimales: usize) -> BigInt {
    if decimales == 0 {
        return BigInt::from(3);
    }
    let garde = 30usize;
    let total = decimales + garde;
    let scale = BigInt::from(10u64).pow(total as u32);

    let c3 = BigInt::from(640320u64).pow(3);
    let a_coef = BigInt::from(545140134i64);
    let b_coef = BigInt::from(13591409i64);

    // C * sqrt(C3) = 426880 * sqrt(10005 * C3)
    let x = &BigInt::from(10005i64) * &c3;
    let sqrt_x = isqrt(&x);
    let c_times_sqrt_c3 = BigInt::from(426880i64) * &sqrt_x;

    // Nombre de termes : ~decimales / 14.18 + marge
    let n_terms = ((decimales as f64) / 14.1816).ceil() as usize + 5;

    // Somme itérative : Σ (-1)^k * t_k
    // t_0 = B / 1
    // t_{k+1} = t_k * -(6k+1)...(6k+6) * (A*(k+1)+B) / ((3k+1)(3k+2)(3k+3)*(k+1)^3 * C3 * (A*k+B))
    let mut sum = BigRational::from_integer(BigInt::zero());
    let mut term = BigRational::from_integer(b_coef.clone()); // t_0 = B

    for k in 0..n_terms {
        if k % 2 == 0 {
            sum = sum + &term;
        } else {
            sum = sum - &term;
        }

        // préparer le terme suivant (si pas le dernier)
        if k + 1 >= n_terms {
            break;
        }
        let k_next = k + 1;

        // numérateur du ratio : -(6k+1)...(6k+6) * (A*(k+1)+B)
        let mut ratio_num = BigInt::one();
        for j in 0..6 {
            ratio_num = ratio_num * BigInt::from(6 * k_next + j);
        }
        ratio_num = -ratio_num;
        ratio_num = ratio_num * (&a_coef * k_next as i64 + &b_coef);

        // dénominateur du ratio : (3k+1)(3k+2)(3k+3) * (k+1)^3 * C3 * (A*k+B)
        let mut ratio_den = BigInt::one();
        ratio_den = ratio_den
            * BigInt::from(3 * k_next + 1)
            * BigInt::from(3 * k_next + 2)
            * BigInt::from(3 * k_next + 3);
        ratio_den = ratio_den * BigInt::from(k_next).pow(3);
        ratio_den = ratio_den * &c3;
        ratio_den = ratio_den * (&a_coef * k as i64 + &b_coef);

        term = term * BigRational::new(ratio_num, ratio_den);
    }

    // π = C * sqrt(C3) * sum / 12
    let c_big = BigRational::from_integer(c_times_sqrt_c3);
    let twelve = BigRational::from_integer(BigInt::from(12i64));
    let pi_rational = &c_big * &sum / &twelve;

    // pi * scale = floor(pi_rational * scale)
    let pi_scaled = (pi_rational * BigRational::from_integer(scale)).to_integer();

    // Retirer la garde
    pi_scaled / BigInt::from(10u64).pow(garde as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pi_premier_chiffres() {
        let pi = calcul_pi(10);
        let s = pi.to_string();
        // π = 3.1415926535...
        assert_eq!(&s[0..1], "3");
        assert!(s.len() >= 10);
    }

    #[test]
    fn test_e_premier_chiffres() {
        let e = calcul_e(10);
        let s = e.to_string();
        // e = 2.7182818284...
        assert_eq!(&s[0..1], "2");
        assert!(s.len() >= 10);
    }

    #[test]
    fn test_phi_premier_chiffres() {
        let phi = calcul_phi(10);
        let s = phi.to_string();
        // φ = 1.6180339887...
        assert_eq!(&s[0..1], "1");
        assert!(s.len() >= 10);
    }

    #[test]
    fn test_racine_2() {
        let r = calcul_racine(&BigInt::from(2), 10);
        let s = r.to_string();
        // √2 = 1.4142135623...
        assert_eq!(&s[0..1], "1");
        assert!(s.len() >= 10);
    }
}
