// Le jumeau Rust de remplir_hasard de Rose (splitmix64, aligné octet,
// 4 tirages par lot). Rust std uniquement, sans Rose.
const ALPHA: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_";

struct Rng(u64);
impl Rng {
    #[inline(always)]
    fn tirer(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }
}

fn main() {
    let n: usize = std::env::args()
        .nth(1).and_then(|s| s.parse().ok()).unwrap_or(1_000_000_000);
    let debut = std::time::Instant::now();
    let mut rng = Rng(2026);
    let mut buf = vec![0u8; n];
    let mut i = 0usize;
    while i + 32 <= n {
        let (r0, r1, r2, r3) = (rng.tirer(), rng.tirer(), rng.tirer(), rng.tirer());
        for k in 0..8usize {
            buf[i + k] = ALPHA[((r0 >> (8 * k)) & 63) as usize];
            buf[i + 8 + k] = ALPHA[((r1 >> (8 * k)) & 63) as usize];
            buf[i + 16 + k] = ALPHA[((r2 >> (8 * k)) & 63) as usize];
            buf[i + 24 + k] = ALPHA[((r3 >> (8 * k)) & 63) as usize];
        }
        i += 32;
    }
    while i < n { buf[i] = ALPHA[(rng.tirer() & 63) as usize]; i += 1; }
    println!("{n}");
    println!("{} ms", debut.elapsed().as_millis());
}
