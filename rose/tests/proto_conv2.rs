use std::time::Instant;

#[test]
fn echelle_parallelisme() {
    let n = 309_000usize;
    let x: Vec<u32> = (0..n).map(|i| ((i as u64).wrapping_mul(6364136223846793005) % 999_999_999) as u32).collect();
    let y: Vec<u32> = (0..n).map(|i| ((i as u64).wrapping_mul(1442695040888963407) % 999_999_999) as u32).collect();
    for f in [1usize, 2, 4, 8, 12, 16, 20] {
        let t = Instant::now();
        let _ = rose::mul_rapide::ntt_mul_par_b32(&x, &y, f);
        let d = t.elapsed();
        eprintln!("{:>2} fils : {:?}  ({:.1} ms/fil)", f, d, d.as_secs_f64()*1000.0/f as f64);
    }
}
