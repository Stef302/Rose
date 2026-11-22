use std::hint::black_box;
use std::time::Instant;
fn main() {
    let mut s = String::new();
    let n = black_box(20_000usize);
    let d = Instant::now();
    for _ in 0..n {
        s.push_str(black_box("x"));
    }
    let ns = d.elapsed().as_nanos();
    println!("RESULT:{}", s.len());
    println!("INTERNAL_NS:{ns}");
}
