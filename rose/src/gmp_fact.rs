//! Backend GMP OPTIONNEL pour les opérateurs factoriels.
//!
//! ⚠⚠ POURQUOI CE MODULE EXISTE. Rose utilise `num-bigint` pour tous ses
//! entiers. Or `num-bigint` ne dispose que de la multiplication « basecase »
//! puis Toom-22 (Karatsuba) et Toom-33 : au-delà de quelques milliers de
//! chiffres, son coût par limb explose. Mesuré sur cette machine (aarch64) :
//!
//!   multiplication de deux nombres de 100 000 chiffres, ×10
//!     num-bigint : 27,7 ms
//!     GMP (rug)  :  5,3 ms          → facteur 5,2×
//!
//!   n! à n = 100 000 (456 574 chiffres)
//!     Rose (num-bigint) : ~30 ms
//!     GMP/C             : 15,4 ms   → facteur ~2
//!
//! GMP dispose de Toom-Cook complet et d'une FFT (NTT) qui donne un coût
//! quasi-linéaire en la taille. C'est ce qui permet à `gmpy2`/`mpmath`/`bc`
//! de calculer `1 000 000!` en moins d'une seconde.
//!
//! ACTIVATION : `cargo build --release --features gmp`. Sans la feature, tout
//! retombe sur `num-bigint` (comportement identique, seulement plus lent) et
//! le crate reste compilable partout, y compris en WebAssembly où GMP n'a
//! aucun sens.
//!
//! ⚠ Le module ne change AUCUNE sémantique : les fonctions rendent exactement
//! les mêmes valeurs, elles passent simplement par GMP quand il est là.
//! `bench/suite/verifier_factorielles.rose` doit rester à 58/58 dans les deux
//! configurations — c'est le garde-fou contre une divergence.

use num_bigint::BigInt;

/// Vrai si le backend GMP est compilé dans ce binaire.
pub const GMP: bool = cfg!(feature = "gmp");

/// Produit en arbre d'une liste de facteurs.
///
/// Avec GMP : conversion en `rug::Integer`, arbre par dichotomie, retour en
/// `BigInt`. Sans GMP : arbre `num-bigint` équivalent.
///
/// ⚠ Le produit en ARBRE (dichotomie) et non la boucle `1·2·…·n` : la boucle
/// fait croître l'opérande à chaque pas, ce qui coûte O(n²) en chiffres. Avec
/// l'arbre, les deux opérandes de chaque multiplication ont des tailles
/// comparables et la multiplication rapide (Toom/FFT) prend le relais.
pub fn produit_arbre(facteurs: &[BigInt]) -> BigInt {
    if facteurs.is_empty() { return BigInt::from(1u64); }
    #[cfg(feature = "gmp")]
    {
        use rug::integer::Order;
        use rug::Integer;

        // ⚠⚠ NE JAMAIS convertir par `to_string()` / `from_str_radix()` : le
        // passage par la base 10 coûte O(n²) et ANNULAIT tout le gain —
        // mesuré : n! à n = 100 000 passait de 25 ms (num-bigint) à 249 ms
        // (GMP + conversion décimale), soit 10× PLUS LENT. On convertit en
        // BINAIRE par `to_digits`/`from_digits` (ordre des digits par limb),
        // ce qui est linéaire.
        fn vers_gmp(x: &BigInt) -> Integer {
            let (signe, mag) = x.to_bytes_le();
            if mag.is_empty() {
                return Integer::from(0);
            }
            // `mag` est en octets little-endian : on le relit par chunks de 8
            // octets pour former des `u64` (l'ordre attendu par `from_digits`
            // est par limb, du plus petit au plus grand).
            let mut limbs = Vec::with_capacity((mag.len() + 7) / 8);
            for chunk in mag.chunks(8) {
                let mut b = [0u8; 8];
                b[..chunk.len()].copy_from_slice(chunk);
                limbs.push(u64::from_le_bytes(b));
            }
            let mut r = Integer::from_digits(&limbs, Order::Lsf);
            if signe == num_bigint::Sign::Minus {
                r = -r;
            }
            r
        }

        fn depuis_gmp(x: &Integer) -> BigInt {
            let signe = if x < &Integer::from(0) { num_bigint::Sign::Minus } else { num_bigint::Sign::Plus };
            let abs = Integer::from(x.abs_ref());
            let limbs: Vec<u64> = abs.to_digits(Order::Lsf);
            let mut octets = Vec::with_capacity(limbs.len() * 8);
            for l in limbs {
                octets.extend_from_slice(&l.to_le_bytes());
            }
            BigInt::from_bytes_le(signe, &octets)
        }

        fn arbre(v: &[Integer]) -> Integer {
            match v.len() {
                0 => Integer::from(1),
                1 => v[0].clone(),
                n => {
                    let m = n / 2;
                    Integer::from(&arbre(&v[..m]) * &arbre(&v[m..]))
                }
            }
        }
        let v: Vec<Integer> = facteurs.iter().map(vers_gmp).collect();
        let r = arbre(&v);
        return depuis_gmp(&r);
    }
    #[cfg(not(feature = "gmp"))]
    {
        let mut v: Vec<BigInt> = facteurs.to_vec();
        while v.len() > 1 {
            let m = v.len();
            let mut i = 0usize;
            while i + 1 < m {
                let x = std::mem::replace(&mut v[i], BigInt::from(0u64));
                let y = std::mem::replace(&mut v[i + 1], BigInt::from(0u64));
                v[i / 2] = x * y;
                i += 2;
            }
            if m % 2 == 1 {
                v[(m - 1) / 2] = std::mem::replace(&mut v[m - 1], BigInt::from(0u64));
            }
            v.truncate((m + 1) / 2);
        }
        v.into_iter().next().unwrap_or_else(|| BigInt::from(1u64))
    }
}

/// Factorielle par produit en arbre des entiers 1..=n.
pub fn factorielle(n: u64) -> BigInt {
    if n < 2 { return BigInt::from(1u64); }
    let facteurs: Vec<BigInt> = (1..=n).map(BigInt::from).collect();
    produit_arbre(&facteurs)
}

/// Multifactorielle d'ordre `k` : produit de n, n−k, n−2k… > 0.
///
/// ⚠ `n!!` n'est PAS `(n!)!` : 7!! = 7·5·3·1 = 105.
pub fn multifactorielle(n: u64, k: u64) -> BigInt {
    let k = k.max(1);
    if n == 0 { return BigInt::from(1u64); }
    let mut facteurs = Vec::new();
    let mut i = n;
    loop {
        facteurs.push(BigInt::from(i));
        // ⚠⚠ `i` est NON SIGNÉ : `i > 0` ne suffit pas. Quand `i < k`,
        // `i -= k` déborde à ~1,8e19 et la boucle ne s'arrête jamais (bug
        // mesuré dans le bench C, qui a produit un SIGSEGV). On sort
        // explicitement dès que le pas suivant serait ≤ 0.
        if i <= k { break; }
        i -= k;
    }
    produit_arbre(&facteurs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factorielle_juste() {
        assert_eq!(factorielle(0), BigInt::from(1u64));
        assert_eq!(factorielle(5), BigInt::from(120u64));
        assert_eq!(factorielle(10), BigInt::from(3_628_800u64));
    }

    #[test]
    fn multifactorielles_conformes_aux_valeurs_oeis() {
        // Double (A000165/A001147)
        let d = [(0u64, 1u64), (1, 1), (2, 2), (3, 3), (4, 8), (5, 15),
                 (6, 48), (7, 105), (8, 384), (9, 945), (10, 3840)];
        for (n, attendu) in d {
            assert_eq!(multifactorielle(n, 2), BigInt::from(attendu), "n={n}!!");
        }
        // Triple (A007661)
        let t = [(1u64, 1u64), (2, 2), (3, 3), (4, 4), (5, 10),
                 (6, 18), (7, 28), (8, 80), (9, 162), (10, 280)];
        for (n, attendu) in t {
            assert_eq!(multifactorielle(n, 3), BigInt::from(attendu), "n={n}!!!");
        }
        // Quadruple
        assert_eq!(multifactorielle(11, 4), BigInt::from(11u64 * 7 * 3));
    }

    #[test]
    fn multifactorielle_ne_boucle_pas_infiniment() {
        // ⚠ Le bug mesuré : `i -= k` sur un entier NON SIGNÉ déborde quand
        // i < k, et la boucle tourne à l'infini en écrivant hors du tableau.
        // Ces cas doivent se terminer instantanément.
        for k in 2..=8u64 {
            for n in 0..=(k + 2) {
                let _ = multifactorielle(n, k);
            }
        }
        assert_eq!(multifactorielle(1, 5), BigInt::from(1u64));
        assert_eq!(multifactorielle(4, 4), BigInt::from(4u64));
    }

    #[test]
    fn k_un_redonne_la_factorielle() {
        for n in 0..=12u64 {
            assert_eq!(multifactorielle(n, 1), factorielle(n), "n={n}");
        }
    }
}
