use std::hint::black_box;
use std::time::Instant;
fn main() {
    let d = Instant::now();
    let n = black_box(1_000_000usize);
    let mut s: u64 = 0;
    for k in 1..=n {
        s = s.wrapping_add(black_box(k) as u64);
    }
    let ns = d.elapsed().as_nanos();
    println!("RESULT:{s}");
    println!("INTERNAL_NS:{ns}");
}
