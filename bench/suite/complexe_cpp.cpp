// Banc C++ : même calcul complexe que complexe.rose (1 000 000 itérations)
// z = re + im*i ; w = z*z ; accumulateur sur re(w) et im(w)
#include <iostream>

int main() {
    const long N = 1000000L;
    double total_re = 0.0;
    double total_im = 0.0;
    for (long k = 1; k <= N; ++k) {
        double z_re = (double)k * 0.000001;
        double z_im = 1.0 - z_re;
        double w_re = z_re * z_re - z_im * z_im;
        double w_im = 2.0 * z_re * z_im;
        total_re += w_re;
        total_im += w_im;
    }
    std::cout << total_re << "\n" << total_im << "\n";
    return 0;
}
