use std::hint::black_box;
use std::time::Instant;
fn est_premier(n: u64) -> bool {
    if n < 2 { return false; }
    if n == 2 { return true; }
    if n % 2 == 0 { return false; }
    let mut j = 3u64;
    while j*j <= n {
        if n % j == 0 { return false; }
        j += 2;
    }
    true
}
fn main() {
    let n = black_box(50_000u64);
    let d = Instant::now();
    let mut c = 0u64;
    for k in 2..n {
        if est_premier(black_box(k)) { c += 1; }
    }
    let ns = d.elapsed().as_nanos();
    println!("RESULT:{c}");
    println!("INTERNAL_NS:{ns}");
}
