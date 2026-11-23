use std::hint::black_box;
use std::time::Instant;
fn main() {
    let d = Instant::now();
    let n = black_box(1_000_000u64);
    let s = n * (n + 1) / 2;
    let ns = d.elapsed().as_nanos();
    println!("RESULT:{s}");
    println!("INTERNAL_NS:{ns}");
}
