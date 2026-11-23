use std::hint::black_box;
use std::time::Instant;
fn fib(n: u64) -> u64 {
    let mut a = 0u64;
    let mut b = 1u64;
    let mut p = 1u64;
    while p * 2 <= n { p *= 2; }
    while p >= 1 {
        let c = a * (2 * b - a);
        let d = a * a + b * b;
        if n % (2 * p) >= p { a = d; b = c + d; } else { a = c; b = d; }
        p /= 2;
    }
    a
}
fn main() {
    let d = Instant::now();
    let r = fib(black_box(35u64));
    let ns = d.elapsed().as_nanos();
    println!("RESULT:{r}");
    println!("INTERNAL_NS:{ns}");
}
