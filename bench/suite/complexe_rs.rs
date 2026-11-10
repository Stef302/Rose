// Banc Rust : même calcul complexe que complexe.rose (1 000 000 itérations)
// z = re + im*i ; w = z*z ; accumulateur sur re(w) et im(w)
fn main() {
    const N: usize = 1_000_000;
    let mut total_re: f64 = 0.0;
    let mut total_im: f64 = 0.0;
    for k in 1..=N {
        let z_re = k as f64 * 0.000001;
        let z_im = 1.0 - z_re;
        let w_re = z_re * z_re - z_im * z_im;
        let w_im = 2.0 * z_re * z_im;
        total_re += w_re;
        total_im += w_im;
    }
    println!("{}", total_re);
    println!("{}", total_im);
}
