// Bench Rust : factorielles étendues avec `rug` (GMP lié en Rust).
//
// C'est la comparaison la plus juste avec Rose : même langage hôte (Rust),
// même compilateur (LLVM), même backend arithmétique (GMP via `rug`).
// L'écart qui reste est donc celui du RUNTIME de Rose (interprète exact,
// `Gz` boxé), pas celui de la bibliothèque d'entiers.
//
// Compilation : cargo run --release --bin fact_rs [-- N]
// ou directement : rustc -O fact_rs.rs --extern rug=... (voir le README)
//
// ⚠ `rug` nécessite `gmp-mpfr-sys` ; sur cette machine GMP est présent
// (aarch64-linux-gnu). Sans `rug`, le bench retombe sur `num-bigint`
// (voir `fact_rs_num.rs`).

use rug::Integer;
use std::time::Instant;

fn vrais_chiffres(x: &Integer) -> usize {
    // `x.significant_bits()` donne une estimation : le nombre EXACT de
    // chiffres se lit par la conversion décimale (comme Rose).
    x.to_string().len()
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

fn multifact(n: u64, k: u64) -> Integer {
    if n == 0 { return Integer::from(1); }
    let mut f = Vec::with_capacity(((n - 1) / k + 1) as usize);
    let mut i = n;
    loop {
        f.push(Integer::from(i));
        if i <= k { break; }
        i -= k;
    }
    arbre(&f)
}

fn factorielle(n: u64) -> Integer {
    let f: Vec<Integer> = (1..=n).map(Integer::from).collect();
    arbre(&f)
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

    let t = Instant::now();
    let mut d = Integer::from(1);
    for i in 1..=n {
        d *= i;
        if i % 2 == 0 { d += 1; } else { d -= 1; }
    }
    println!("!n       : {:.2} ms, {} chiffres", t.elapsed().as_secs_f64() * 1000.0, vrais_chiffres(&d));

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
    let f: Vec<Integer> = (2..=n).filter(|&p| !prem[p as usize]).map(Integer::from).collect();
    let a = arbre(&f);
    println!("n#       : {:.2} ms, {} chiffres", t.elapsed().as_secs_f64() * 1000.0, vrais_chiffres(&a));

}
