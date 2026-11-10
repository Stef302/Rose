// Banc C++ : même calcul complexe que complexe.rose (1 000 000 d'itérations)
//
// Le programme Rose fait :
//     $z = $z_re + $z_im * i
//     $w = $z * $z
//     $total_re += re($w) ; $total_im += im($w)
//
// On exprime EXACTEMENT le même calcul avec un type complexe : le produit
// complexe général (a+bi)(c+di), pas une formule « wi = 2*zr*zi » qui
// ferait moins de travail que Rose.
#include <cstdio>

struct C { double re, im; };

// (a+bi)(c+di) = (ac - bd) + (ad + bc)i
static inline C mul(C a, C b) {
    double p = a.re * b.im;
    C r;
    r.re = a.re * b.re - a.im * b.im;
    r.im = p + p;
    return r;
}

int main() {
    const long N = 1000000L;
    double total_re = 0.0;
    double total_im = 0.0;
    for (long k = 1; k <= N; ++k) {
        double z_re = (double)k * 0.000001;
        double z_im = 1.0 - z_re;
        // $z_im * i  avec i = 0 + 1i
        C zi;
        zi.re = z_im * 0.0;
        zi.im = z_im * 1.0;
        // $z = $z_re + $z_im * i   (réel + complexe)
        C z;
        z.re = z_re + zi.re;
        z.im = 0.0 + zi.im;
        // $w = $z * $z
        C w = mul(z, z);
        total_re += w.re;
        total_im += w.im;
    }
    std::printf("%.17g\n%.17g\n", total_re, total_im);
    return 0;
}
