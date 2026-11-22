use num_bigint::BigInt;
use num_rational::Ratio;
use std::hint::black_box;
use std::time::Instant;
fn main() {
    let n = black_box(200u32);
    let d = Instant::now();
    let mut r = Ratio::new(BigInt::from(1), BigInt::from(2));
    for k in 3..=n { r += Ratio::new(BigInt::from(1), BigInt::from(k)); }
    let ns = d.elapsed().as_nanos();
    println!("RESULT:{}/{}", r.numer(), r.denom());
    println!("INTERNAL_NS:{ns}");
}
