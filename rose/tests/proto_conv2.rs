use std::time::Instant;

#[test]
fn echelle() {
    for &n in &[4096usize, 16384, 32768, 65536] {
        let x: Vec<u32> = (0..n).map(|i| ((i as u64).wrapping_mul(6364136223846793005) % 999_999_999) as u32).collect();
        let y: Vec<u32> = (0..n).map(|i| ((i as u64).wrapping_mul(1442695040888963407) % 999_999_999) as u32).collect();
        let mut ligne = format!("n={:>6} :", n);
        for f in [1usize, 2, 4, 8, 16, 20] {
            let t = Instant::now();
            let _ = rose::mul_rapide::ntt_mul_par_b32(&x, &y, f);
            ligne += &format!(" {:>3}f={:>7.2}ms", f, t.elapsed().as_secs_f64()*1000.0);
        }
        eprintln!("{}", ligne);
    }
}
