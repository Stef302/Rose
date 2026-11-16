// Prototypage : conversion binaire → base 10^9 par MULTIPLICATION.
//
// Au lieu de diviser (ce que fait `num-bigint`, et notre division est
// Knuth = O(n·m)), on utilise notre multiplication rapide (NTT) :
//
//   N = H·2^k + L
//   conv(N) = conv(H)·2^k + conv(L)
//
// Le « × 2^k » se fait en multipliant conv(H) par la représentation
// base 10^9 de 2^k (un nombre dense, mais le produit est une NTT).

use num_bigint::BigUint;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

const BASE: u64 = 1_000_000_000;

/// Multiplication de deux nombres en base 10^9 (limbs u32).
pub static T_MUL: Mutex<u128> = Mutex::new(0);
pub static T_POW: Mutex<u128> = Mutex::new(0);
pub static N_MUL: Mutex<u64> = Mutex::new(0);

/// Multiplication SCOLAIRE en base 10^9 (coût fixe quasi nul).
fn mul_scolaire(x: &[u32], y: &[u32]) -> Vec<u32> {
    let mut r = vec![0u32; x.len() + y.len()];
    for (j, &yj) in y.iter().enumerate() {
        if yj == 0 { continue; }
        let yj = yj as u64;
        let mut carry = 0u64;
        for (i, &xi) in x.iter().enumerate() {
            let t = xi as u64 * yj + r[i + j] as u64 + carry;
            r[i + j] = (t % BASE) as u32;
            carry = t / BASE;
        }
        let mut k = j + x.len();
        while carry > 0 {
            let t = r[k] as u64 + carry;
            r[k] = (t % BASE) as u32;
            carry = t / BASE;
            k += 1;
        }
    }
    r
}

/// Seuil (en limbs base 10^9) au-delà duquel on utilise la NTT.
const SEUIL_NTT_LIMBS: usize = 256;

fn mul_b1e9(x: &[u32], y: &[u32]) -> Vec<u32> {
    let t = Instant::now();
    let r = if x.len().max(y.len()) < SEUIL_NTT_LIMBS {
        mul_scolaire(x, y)
    } else {
        rose::mul_rapide::ntt_mul_b1e9(x, y)
    };
    *T_MUL.lock().unwrap() += t.elapsed().as_nanos();
    *N_MUL.lock().unwrap() += 1;
    r
}

/// 2^k en base 10^9, mis en cache par k.
fn pow2_b1e9(k: usize) -> Vec<u32> {
    static CACHE: Mutex<Option<HashMap<usize, Vec<u32>>>> = Mutex::new(None);
    {
        let g = CACHE.lock().unwrap();
        if let Some(map) = g.as_ref() {
            if let Some(v) = map.get(&k) { return v.clone(); }
        }
    }
    let t = Instant::now();
    let v = calc_pow2(k);
    *T_POW.lock().unwrap() += t.elapsed().as_nanos();
    let mut g = CACHE.lock().unwrap();
    g.get_or_insert_with(HashMap::new).insert(k, v.clone());
    v
}

/// ⚠ PAS de verrou tenu pendant l'appel récursif (sinon INTERBLOCAGE).
fn calc_pow2(k: usize) -> Vec<u32> {
    if k == 0 {
        vec![1u32]
    } else if k < 30 {
        vec![1u32 << k]
    } else {
        let h = calc_pow2(k / 2);
        let mut v = mul_b1e9(&h, &h);
        if k % 2 == 1 {
            let mut carry = 0u64;
            for l in v.iter_mut() {
                let t = *l as u64 * 2 + carry;
                *l = (t % BASE) as u32;
                carry = t / BASE;
            }
            if carry > 0 { v.push(carry as u32); }
        }
        v
    }
}

/// Conversion binaire → base 10^9.
/// Seuil (en bits) sous lequel on convertit par la méthode scolaire.
const SEUIL_FEUILLE_BITS: usize = 1024;

fn conv(binaire: &[u64]) -> Vec<u32> {
    // Cas de base : tient dans peu de bits → divisions scolaires par 10^9.
    if binaire.len() * 64 <= SEUIL_FEUILLE_BITS {
        // Divisions successives par 10^9 sur des limbs u64.
        let mut x = binaire.to_vec();
        let mut r = Vec::new();
        while x.iter().any(|&l| l != 0) {
            let mut rem: u128 = 0;
            for i in (0..x.len()).rev() {
                let cur = (rem << 64) | x[i] as u128;
                x[i] = (cur / BASE as u128) as u64;
                rem = cur % BASE as u128;
            }
            r.push(rem as u32);
            while x.last() == Some(&0) { x.pop(); }
        }
        if r.is_empty() { r.push(0); }
        return r;
    }
    // Cas de base : tient dans un u64.
    if binaire.len() <= 1 {
        let v = binaire.first().copied().unwrap_or(0);
        let mut r = Vec::new();
        let mut x = v;
        if x == 0 { return vec![0]; }
        while x > 0 { r.push((x % BASE) as u32); x /= BASE; }
        return r;
    }
    // Découpe au milieu, sur une frontière de limbs u64.
    let n2 = binaire.len() / 2;
    let haut = conv(&binaire[n2..]);
    let bas = conv(&binaire[..n2]);
    // N = haut·2^(64·n2) + bas
    let p = mul_b1e9(&haut, &pow2_b1e9(64 * n2));
    // p + bas
    addition_b1e9(p, bas)
}

fn addition_b1e9(mut a: Vec<u32>, b: Vec<u32>) -> Vec<u32> {
    if b.len() > a.len() { a.resize(b.len(), 0); }
    let mut carry = 0u64;
    for i in 0..a.len() {
        let t = a[i] as u64 + *b.get(i).unwrap_or(&0) as u64 + carry;
        a[i] = (t % BASE) as u32;
        carry = t / BASE;
    }
    if carry > 0 { a.push(carry as u32); }
    a
}

fn base1e9_vers_chaine(v: &[u32]) -> String {
    let mut s = String::with_capacity(v.len() * 9);
    let mut first = true;
    for i in (0..v.len()).rev() {
        if first {
            s.push_str(&v[i].to_string());
            first = false;
        } else {
            s.push_str(&format!("{:09}", v[i]));
        }
    }
    s
}

#[test]
fn prototype_conversion() {
    // Vérification sur des petits nombres.
    for n in [0u64, 1, 7, 12345, 1_000_000_000, u64::MAX] {
        let b = BigUint::from(n).to_u64_digits();
        let v = conv(&b);
        let s = base1e9_vers_chaine(&v);
        assert_eq!(s, n.to_string(), "n = {n}");
    }
    // Gros nombre aléatoire : comparaison avec num-bigint.
    let g = BigUint::from(123456789u64) << (200_000usize);
    let b = g.to_u64_digits();
    let t = Instant::now();
    let v = conv(&b);
    let d = t.elapsed();
    let s = base1e9_vers_chaine(&v);
    assert_eq!(s, g.to_string());
    eprintln!("200 000 bits : {:?}  ({} chiffres)", d, s.len());

    // Le VRAIT cas : 1 000 000!
    let t = Instant::now();
    let f = rose::i128x::factorielle_parallele(1_000_000);
    let t_fact = t.elapsed();
    let g = f.v().magnitude().clone();
    let t = Instant::now();
    let b = g.to_u64_digits();
    let v = conv(&b);
    let s = base1e9_vers_chaine(&v);
    let d = t.elapsed();
    eprintln!("1 000 000! : calcul {:?} | conversion {:?} ({} chiffres)", t_fact, d, s.len());
    eprintln!("  multiplications : {} appels, {:?} | puissances de 2 : {:?}",
        *N_MUL.lock().unwrap(), std::time::Duration::from_nanos(*T_MUL.lock().unwrap() as u64),
        std::time::Duration::from_nanos(*T_POW.lock().unwrap() as u64));
    assert_eq!(s, g.to_string());
}
