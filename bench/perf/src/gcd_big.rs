use num_bigint::BigUint;
use num_integer::Integer;   /* gcd = Stein dans num-bigint 0.4.8 */
use std::hint::black_box;
use std::time::Instant;
fn main() {
    // a = "1234567890"*100, b = "9876543210"*100 — mêmes entrées que Rose/C/Python.
    let sa = "1234567890".repeat(100);
    let sb = "9876543210".repeat(100);
    let a: BigUint = sa.parse().unwrap();
    let b: BigUint = sb.parse().unwrap();
    let d = Instant::now();
    let g = a.gcd(black_box(&b));
    let ns = d.elapsed().as_nanos();
    println!("RESULT:{g}");
    println!("INTERNAL_NS:{ns}");
}
