use num_bigint::BigInt;
use std::time::Instant;

#[test]
fn comparaison_gmp() {
    for &bits in &[1_840_000u64, 3_680_000, 7_360_000] {
        let a = BigInt::from(3u32) << bits;
        let b = BigInt::from(5u32) << bits;
        let t = Instant::now();
        let _ = rose::mul_rapide::mul(&a, &b);
        eprintln!("Rose {} x {} bits : {:?}", bits, bits, t.elapsed());
    }
}
