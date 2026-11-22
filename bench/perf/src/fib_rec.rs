use std::hint::black_box;
use std::time::Instant;
fn fib(n: u64) -> u64 {
    if n < 2 { n } else { fib(n-1) + fib(n-2) }
}
fn main() {
    let d = Instant::now();
    let r = fib(black_box(35u64));
    let ns = d.elapsed().as_nanos();
    println!("RESULT:{r}");
    println!("INTERNAL_NS:{ns}");
}
