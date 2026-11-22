use num_bigint::BigUint;
use std::hint::black_box;
use std::time::Instant;
fn main() {
    let n = black_box(3000u32);
    let d = Instant::now();
    let mut r = BigUint::from(1u32);
    for k in 2..=n { r *= BigUint::from(k); }
    let ns = d.elapsed().as_nanos();
    println!("RESULT:{r}");
    println!("INTERNAL_NS:{ns}");
}
