use std::hint::black_box;
use std::time::Instant;
fn main() {
    let n = black_box(50_000usize);
    let d = Instant::now();
    let mut s = vec![true; n];
    let mut p = 2usize;
    while p * p < n {
        if s[p] {
            let mut m = p * p;
            while m < n { s[m] = false; m += p; }
        }
        p += 1;
    }
    let c = s[2..].iter().filter(|&&x| x).count();
    let ns = d.elapsed().as_nanos();
    println!("RESULT:{c}");
    println!("INTERNAL_NS:{ns}");
}
