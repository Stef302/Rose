// Bench Rust PUR (`num-bigint` seul, AUCUNE dépendance GMP).
//
// ⚠ C'est la comparaison la plus honnête avec Rose : même langage hôte
// (Rust), même compilateur (LLVM), même bibliothèque d'entiers
// (`num-bigint`). L'écart qui reste est celui du RUNTIME de Rose
// (interprète exact, `Gz` boxé), pas de l'arithmétique.
//
// Comparé à :
//   - `fact_rs.rs` (rug)   : même Rust mais arithmétique GMP → mesure GMP ;
//   - `fact_gmp.c`         : la référence absolue en C ;
//   - `fact_gmp.cpp`       : gmpxx.
//
// Compilation : voir `bench_factorielles.sh` (projet cargo temporaire).
// Usage       : fact_rs_pur [N]

use num_bigint::BigInt;
use num_traits::One;
use std::time::Instant;

fn vrais_chiffres(x: &BigInt) -> usize {
    x.to_string().len()
}

/// Produit en arbre équilibré (même structure que Rose).
fn arbre(v: &mut Vec<BigInt>) {
    while v.len() > 1 {
        let m = v.len();
        let mut i = 0usize;
        while i + 1 < m {
            // ⚠ En place : `i/2 ≤ i`, la cible a déjà été consommée.
            let p = &v[i] * &v[i + 1];
            v[i / 2] = p;
            i += 2;
        }
        if m % 2 == 1 { v[(m - 1) / 2] = v[m - 1].clone(); }
        v.truncate((m + 1) / 2);
    }
}

/// Multifactorielle d'ordre k : produit de n, n−k, n−2k… > 0.
fn multifact(n: u64, k: u64) -> BigInt {
    if n == 0 { return BigInt::one(); }
    let mut f = Vec::with_capacity(((n - 1) / k + 1) as usize);
    let mut i = n;
    loop {
        f.push(BigInt::from(i));
        // ⚠ `i` non signé : on sort AVANT que `i -= k` ne déborde.
        if i <= k { break; }
        i -= k;
    }
    arbre(&mut f);
    f.pop().unwrap()
}

/// Factorielle : facteurs premiers (Légendre) + arbre, comme Rose.
fn factorielle(n: u64) -> BigInt {
    if n < 2 { return BigInt::one(); }
    let mut prem = vec![true; (n + 1) as usize];
    prem[0] = false;
    prem[1] = false;
    let mut i = 2u64;
    while i * i <= n {
        if prem[i as usize] {
            let mut j = i * i;
            while j <= n { prem[j as usize] = false; j += i; }
        }
        i += 1;
    }
    let mut f: Vec<BigInt> = Vec::new();
    for p in 2..=n {
        if !prem[p as usize] { continue; }
        let mut e = 0u64;
        let mut q = p;
        while q <= n {
            e += n / q;
            if q > n / p { break; }
            q *= p;
        }
        // p^e par exponentiation binaire.
        let mut base = BigInt::from(p);
        let mut res = BigInt::one();
        let mut ee = e;
        while ee > 0 {
            if ee & 1 == 1 { res *= &base; }
            ee >>= 1;
            if ee > 0 { base = &base * &base; }
        }
        f.push(res);
    }
    arbre(&mut f);
    f.pop().unwrap()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let n: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(100_000);
    println!("N = {n}");

    let t = Instant::now();
    let a = factorielle(n);
    println!("n!       : {:.2} ms, {} chiffres", t.elapsed().as_secs_f64() * 1000.0, vrais_chiffres(&a));

    let t = Instant::now();
    let a = multifact(n, 2);
    println!("n!!      : {:.2} ms, {} chiffres", t.elapsed().as_secs_f64() * 1000.0, vrais_chiffres(&a));

    let t = Instant::now();
    let a = multifact(n, 3);
    println!("n!!!     : {:.2} ms, {} chiffres", t.elapsed().as_secs_f64() * 1000.0, vrais_chiffres(&a));

    let t = Instant::now();
    let a = multifact(n, 4);
    println!("n!!!!    : {:.2} ms, {} chiffres", t.elapsed().as_secs_f64() * 1000.0, vrais_chiffres(&a));

    // !n : récurrence !n = n·!(n−1) + (−1)^n
    let t = Instant::now();
    let mut d = BigInt::one();
    for i in 1..=n {
        d *= i;
        if i % 2 == 0 { d += 1; } else { d -= 1; }
    }
    println!("!n       : {:.2} ms, {} chiffres", t.elapsed().as_secs_f64() * 1000.0, vrais_chiffres(&d));

    // n# : primorielle
    let t = Instant::now();
    let mut prem = vec![false; (n + 1) as usize];
    let mut i = 2u64;
    while i * i <= n {
        if !prem[i as usize] {
            let mut j = i * i;
            while j <= n { prem[j as usize] = true; j += i; }
        }
        i += 1;
    }
    let mut f: Vec<BigInt> = (2..=n).filter(|&p| !prem[p as usize]).map(BigInt::from).collect();
    arbre(&mut f);
    let a = f.pop().unwrap();
    println!("n#       : {:.2} ms, {} chiffres", t.elapsed().as_secs_f64() * 1000.0, vrais_chiffres(&a));
}
