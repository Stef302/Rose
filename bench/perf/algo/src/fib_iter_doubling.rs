use std::hint::black_box;
use std::time::Instant;
const M: u128 = 1_000_000_007;
fn fib(n: u64) -> u64 {
    let mut a = 0u128;
    let mut b = 1u128;
    let mut p = 1u64;
    while p * 2 <= n { p *= 2; }
    while p >= 1 {
        let c = (a * ((2 * b + M - a) % M)) % M;
        let d = (a * a + b * b) % M;
        if n % (2 * p) >= p { a = d; b = (c + d) % M; } else { a = c; b = d; }
        p /= 2;
    }
    a as u64
}
fn main() {
    let d = Instant::now();
    let r = fib(black_box(1_000_000u64));
    let ns = d.elapsed().as_nanos();
    println!("RESULT:{r}");
    println!("INTERNAL_NS:{ns}");
}
