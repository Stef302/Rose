use std::collections::HashMap;
use std::hint::black_box;
use std::time::Instant;
fn main() {
    let n = black_box(3_000u64);
    let d = Instant::now();
    let mut t = HashMap::with_capacity(n as usize);
    for i in 1..=n { t.insert(black_box(i), i*i); }
    let mut c = 0u64;
    for j in 1..=n { if t.contains_key(&black_box(j)) { c += 1; } }
    let ns = d.elapsed().as_nanos();
    println!("RESULT:{c}");
    println!("INTERNAL_NS:{ns}");
}
