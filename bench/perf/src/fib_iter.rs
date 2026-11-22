use std::hint::black_box;
use std::time::Instant;
fn main() {
    let mut a: u64 = 0;
    let mut b: u64 = 1;
    let n = black_box(1_000_000usize);
    let d = Instant::now();
    for _ in 0..n {
        let t = (a + b) % 1_000_000_007;
        a = b;
        b = t;
    }
    let ns = d.elapsed().as_nanos();
    println!("RESULT:{a}");
    println!("INTERNAL_NS:{ns}");
}
