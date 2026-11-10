// Banc Rust : même calcul complexe que complexe.rose (1 000 000 d'itérations)
//
// Le programme Rose fait :
//     $z = $z_re + $z_im * i
//     $w = $z * $z
//     $total_re += re($w) ; $total_im += im($w)
//
// On exprime EXACTEMENT le même calcul avec un type complexe : le produit
// complexe général (a+bi)(c+di), pas une formule « wi = 2*zr*zi » qui
// ferait moins de travail que Rose.
#[derive(Clone, Copy)]
struct C {
    re: f64,
    im: f64,
}

// (a+bi)(c+di) = (ac - bd) + (ad + bc)i
#[inline]
fn mul(a: C, b: C) -> C {
    let p = a.re * b.im;
    C {
        re: a.re * b.re - a.im * b.im,
        im: p + p,
    }
}

fn main() {
    const N: usize = 1_000_000;
    let mut total_re = 0.0f64;
    let mut total_im = 0.0f64;
    for k in 1..=N {
        let z_re = k as f64 * 0.000001;
        let z_im = 1.0 - z_re;
        // $z_im * i  avec i = 0 + 1i
        let zi = C { re: z_im * 0.0, im: z_im * 1.0 };
        // $z = $z_re + $z_im * i   (réel + complexe)
        let z = C { re: z_re + zi.re, im: 0.0 + zi.im };
        // $w = $z * $z
        let w = mul(z, z);
        total_re += w.re;
        total_im += w.im;
    }
    println!("{}", total_re);
    println!("{}", total_im);
}
