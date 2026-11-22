use num_bigint::BigUint;
use std::hint::black_box;
use std::time::Instant;
fn main() {
    let e = black_box(20000u32);
    let d = Instant::now();
    let p = BigUint::from(2u32).pow(e);
    let ns = d.elapsed().as_nanos();
    println!("RESULT:{p}");
    println!("INTERNAL_NS:{ns}");
}
