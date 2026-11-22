use std::hint::black_box;
use std::time::Instant;
fn main() {
    let n = black_box(100_000usize);
    let d = Instant::now();
    let mut l = Vec::with_capacity(n);
    for i in 1..=n { l.push(black_box(i as u64)); }
    let mut s = 0u64;
    for j in 0..n { s = s.wrapping_add(l[j]); }
    let ns = d.elapsed().as_nanos();
    println!("RESULT:{s}");
    println!("INTERNAL_NS:{ns}");
}
