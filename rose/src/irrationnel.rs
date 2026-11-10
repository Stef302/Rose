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
use num_traits::{One, Zero};

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

/// Calcule π à `decimales` décimales.
/// Utilise Machin pour les petits nombres, Chudnovsky pour les grands.
/// Retourne les digits de π × 10^decimales (entier, sans virgule).
pub fn calcul_pi(decimales: usize) -> BigInt {
    if decimales == 0 {
        return BigInt::from(3);
    }
    // Chudnovsky est plus rapide que Machin dès quelques dizaines de
    // décimales (14,18 décimales par terme au lieu de ~1,4) : on l'utilise
    // partout. Machin reste implémenté (test de recoupement).
    calcul_pi_chudnovsky(decimales)
}

/// Calcule π à `decimales` décimales par la formule de Machin :
/// π/4 = 4·arctan(1/5) − arctan(1/239)
/// Retourne les digits de π × 10^decimales (entier, sans virgule).
fn calcul_pi_machin(decimales: usize) -> BigInt {
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

/// Scission binaire pour la série de Taylor de e : e = Σ 1/k!.
///
/// Les termes t_k = 1/k! s'écrivent Π_{j≤k} p_j/q_j avec p_j = 1 et
/// q_j = j — la même mécanique que pour Chudnovsky, ce qui donne une
/// complexité O(M(n)·log n) au lieu de O(n²) pour la série naïve
/// (factorielle complète puis additions).
///
/// Retourne (P, Q, T) ; la somme des termes [a, b[ vaut T/Q.
fn bs_e(a: u64, b: u64) -> (BigInt, BigInt, BigInt) {
    if b - a == 1 {
        let (pab, qab) = if a == 0 {
            (BigInt::one(), BigInt::one()) // 0! = 1
        } else {
            (BigInt::one(), BigInt::from(a))
        };
        // T = P·L_a avec L_a = 1
        let tab = pab.clone();
        (pab, qab, tab)
    } else {
        let m = (a + b) / 2;
        let (pam, qam, tam) = bs_e(a, m);
        let (pmb, qmb, tmb) = bs_e(m, b);
        let pab = &pam * &pmb;
        let qab = &qam * &qmb;
        let tab = &qmb * &tam + &pam * &tmb;
        (pab, qab, tab)
    }
}

/// Calcule e à `decimales` décimales par la série de Taylor Σ 1/k!,
/// en scission binaire.
/// Retourne e × 10^decimales (entier).
pub fn calcul_e(decimales: usize) -> BigInt {
    if decimales == 0 {
        return BigInt::from(2);
    }
    let garde = 10usize;
    let total = decimales + garde;
    let one = BigInt::from(10u64).pow(total as u32);

    // Nombre de termes : il faut n! > 10^total pour que la queue soit
    // négligeable. log10(n!) ≈ n·log10(n/e) → on itère jusqu'à le vérifier.
    let mut n: u64 = 1;
    let mut lg = 0f64; // log10(n!)
    while lg < total as f64 {
        n += 1;
        lg += (n as f64).log10();
    }
    let (_p, q, t) = bs_e(0, n);

    // e·one = T·one/Q  (le reste de la série est < 1/n! < 10^-total)
    let e = (&t * &one) / &q;

    e / BigInt::from(10u64).pow(garde as u32)
}

/// Calcule √n à `decimales` décimales.
/// Retourne √n × 10^decimales (entier).
///
/// On calcule la racine ENTIÈRE de n·10^(2·decimales) : la racine entière de
/// num-bigint utilise l'algorithme de Karatsuba (O(M(n)·log n)), ce qui
/// permet 1 000 000 de décimales ; la méthode de Newton avec division
/// serait en O(n²).
pub fn calcul_racine(n: &BigInt, decimales: usize) -> BigInt {
    if n.is_zero() {
        return BigInt::zero();
    }
    if n < &BigInt::zero() {
        panic!("racine carrée d'un nombre négatif");
    }
    if decimales == 0 {
        return n.sqrt();
    }
    let scale = BigInt::from(10u64).pow(decimales as u32);
    (n * &scale * &scale).sqrt()
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

/// Chudnovsky par SCISSION BINAIRE (« binary splitting »).
///
/// La série de Chudnovsky s'écrit
///   π = 426880·√10005 / Σ_k M_k·L_k,   M_k = (6k)!/((3k)!(k!)³·(-640320³)^k)
///                                        L_k = 13591409 + 545140134·k
///
/// En absorbant le facteur (-640320³) dans le dénominateur, on obtient
/// M_k = Π_{j≤k} p_j/q_j avec
///   p_j = (6j-5)…(6j)      q_j = j³·(3j-2)(3j-1)(3j)·(-640320³)
/// ce qui permet la scission binaire : chaque terme est un PRODUIT exact
/// d'entiers (aucune division intermédiaire) et la complexité est
/// O(M(n)·log n) au lieu de O(n²) — c'est ce qui rend 1 000 000 de
/// décimales calculables en quelques secondes.
///
/// Retourne (P, Q, T) pour l'intervalle de termes [a, b[ ; la somme
/// partielle de la série y vaut T/Q.
fn bs_chudnovsky(a: u64, b: u64) -> (BigInt, BigInt, BigInt) {
    if b - a == 1 {
        let (pab, qab) = if a == 0 {
            (BigInt::one(), BigInt::one())
        } else {
            // P = (6a-5)…(6a) ; Q = a³·(3a-2)(3a-1)(3a)·(-640320³)
            let p = BigInt::from(6 * a - 5) * BigInt::from(6 * a - 4)
                  * BigInt::from(6 * a - 3) * BigInt::from(6 * a - 2)
                  * BigInt::from(6 * a - 1) * BigInt::from(6 * a);
            let q = BigInt::from(a).pow(3) * BigInt::from(3 * a - 2)
                  * BigInt::from(3 * a - 1) * BigInt::from(3 * a)
                  * BigInt::from(-262537412640768000i64);
            (p, q)
        };
        // T = P·(13591409 + 545140134·a)   (PAS de division : le facteur
        // (-640320³)^a est déjà dans Q)
        let l = BigInt::from(13591409i64 + 545140134i64 * a as i64);
        let tab = &pab * l;
        (pab, qab, tab)
    } else {
        let m = (a + b) / 2;
        let (pam, qam, tam) = bs_chudnovsky(a, m);
        let (pmb, qmb, tmb) = bs_chudnovsky(m, b);
        let pab = &pam * &pmb;
        let qab = &qam * &qmb;
        let tab = &qmb * &tam + &pam * &tmb;
        (pab, qab, tab)
    }
}

/// Calcule π à `decimales` décimales par Chudnovsky (scission binaire).
///
///   π = 426880·√10005 · Q / T
///
/// Retourne π × 10^decimales (entier, sans virgule).
pub fn calcul_pi_chudnovsky(decimales: usize) -> BigInt {
    if decimales == 0 {
        return BigInt::from(3);
    }
    let garde = 10usize;
    let total = decimales + garde;
    let one = BigInt::from(10u64).pow(total as u32);

    // ~14,18 décimales par terme : on prend large.
    let n = (total as u64) / 14 + 2;
    let (_p, q, t) = bs_chudnovsky(0, n);

    // √10005 à `total` décimales : racine entière de 10005·10^(2·total).
    // (la racine entière de num-bigint utilise l'algorithme de Karatsuba :
    //  O(M(n)·log n), elle passe donc à 1 000 000 de décimales)
    let sqrt_c = (&one * &one * 10005u64).sqrt();

    let pi = (&q * 426880u64 * &sqrt_c) / &t;

    // Retirer les décimales de garde
    pi / BigInt::from(10u64).pow(garde as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Référence de π (1100 chiffres) produite par l'AGM de Gauss-Legendre
    /// (algorithme indépendant de Chudnovsky).
    const PI_REF: &str = "31415926535897932384626433832795028841971693993751058209749445923078164062862089986280348253421170679821480865132823066470938446095505822317253594081284811174502841027019385211055596446229489549303819644288109756659334461284756482337867831652712019091456485669234603486104543266482133936072602491412737245870066063155881748815209209628292540917153643678925903600113305305488204665213841469519415116094330572703657595919530921861173819326117931051185480744623799627495673518857527248912279381830119491298336733624406566430860213949463952247371907021798609437027705392171762931767523846748184676694051320005681271452635608277857713427577896091736371787214684409012249534301465495853710507922796892589235420199561121290219608640344181598136297747713099605187072113499999983729780499510597317328160963185950244594553469083026425223082533446850352619311881710100031378387528865875332083814206171776691473035982534904287554687311595628638823537875937519577818577805321712268066130019278766111959092164201989380952572010654858632788659361533818279682303019520353018529689957736225994138912497217752834791315";

    /// Référence de e (1200 chiffres) produite par la série de Taylor en
    /// arithmétique décimale exacte.
    const E_REF: &str = "271828182845904523536028747135266249775724709369995957496696762772407663035354759457138217852516642742746639193200305992181741359662904357290033429526059563073813232862794349076323382988075319525101901157383418793070215408914993488416750924476146066808226480016847741185374234544243710753907774499206955170276183860626133138458300075204493382656029760673711320070932870912744374704723069697720931014169283681902551510865746377211125238978442505695369677078544996996794686445490598793163688923009879312773617821542499922957635148220826989519366803318252886939849646510582093923982948879332036250944311730123819706841614039701983767932068328237646480429531180232878250981945581530175671736133206981125099618188159304169035159888851934580727386673858942287922849989208680582574927961048419844436346324496848756023362482704197862320900216099023530436994184914631409343173814364054625315209618369088870701676839642437814059271456354906130310720851038375051011574770417189861068739696552126715468895703503540212340784981933432106817012100562788023519303322474501585390473041995777709350366041699732972508868769664035557071622684471625607988183056776709492699530511627421163976077759050640532829967832986454";

    /// Référence de √2 (1100 chiffres) produite par Newton en décimal exact.
    const SQRT2_REF: &str = "14142135623730950488016887242096980785696718753769480731766797379907324784621070388503875343276415727350138462309122970249248360558507372126441214970999358314132226659275055927557999505011527820605714701095599716059702745345968620147285174186408891986095523292304843087143214508397626036279952514079896872533965463318088296406206152583523950547457502877599617298355752203375318570113543746034084988471603868999706990048150305440277903164542478230684929369186215805784631115966687130130156185689872372352885092648612494977154218334204285686060146824720771435854874155657069677653720226485447015858801620758474922657226002085584466521458398893944370926591800311388246468157082630100594858704003186480342194897278290641045072636881313739855256117322040245091227700226941127573627280495738108967504018369868368450725799364729060762996941380475654823728997180326802474420629269124859052181004459842150591120249441341728531478105803603371077309182869314710171111683916581726889419758716582152128229518488472089694633862891562882765952635140542267653239694617511291602408715510135150455381287560052631468017";

    #[test]
    fn test_pi_chudnovsky() {
        for d in [10usize, 50, 200, 1000] {
            let s = calcul_pi_chudnovsky(d).to_string();
            assert!(s.len() >= d, "longueur {} pour {} décimales", s.len(), d);
            assert_eq!(&s[..d], &PI_REF[..d], "mauvais chiffres à {} décimales", d);
        }
    }

    #[test]
    fn test_pi_machin_egal_chudnovsky() {
        // les deux algorithmes doivent concorder sur 500 décimales
        let a = calcul_pi_machin(500).to_string();
        let b = calcul_pi_chudnovsky(500).to_string();
        assert_eq!(a, b);
    }

    #[test]
    fn test_pi_premier_chiffres() {
        let pi = calcul_pi(10);
        let s = pi.to_string();
        assert_eq!(&s[0..1], "3");
        assert!(s.len() >= 10);
    }

    #[test]
    fn test_e_chiffres() {
        let s = calcul_e(1000).to_string();
        assert!(s.len() >= 1000);
        assert_eq!(&s[..1000], &E_REF[..1000]);
    }

    #[test]
    fn test_e_premier_chiffres() {
        let e = calcul_e(10);
        let s = e.to_string();
        assert_eq!(&s[0..1], "2");
        assert!(s.len() >= 10);
    }

    #[test]
    fn test_phi_chiffres() {
        // φ = 1.61803398874989484820458683436563811772030917980576…
        const REF: &str = "161803398874989484820458683436563811772030917980576286213544862270526046281890244970720720418939113748475408807538689175212663386222353693179317804980834791386680178";
        let s = calcul_phi(100).to_string();
        assert!(s.len() >= 100);
        assert_eq!(&s[..100], &REF[..100]);
    }

    #[test]
    fn test_phi_premier_chiffres() {
        let phi = calcul_phi(10);
        let s = phi.to_string();
        assert_eq!(&s[0..1], "1");
        assert!(s.len() >= 10);
    }

    #[test]
    fn test_racine_2_chiffres() {
        let s = calcul_racine(&BigInt::from(2), 1000).to_string();
        assert!(s.len() >= 1000);
        assert_eq!(&s[..1000], &SQRT2_REF[..1000]);
    }

    #[test]
    fn test_racine_2() {
        let r = calcul_racine(&BigInt::from(2), 10);
        let s = r.to_string();
        assert_eq!(&s[0..1], "1");
        assert!(s.len() >= 10);
    }
}
