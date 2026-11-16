// Bench GMP/C++ (gmpxx) : mêmes opérations que `bench/suite/fact_gmp.c`
// et `bench/suite/factorielles.rose`, pour une comparaison honnête.
//
// Compilation : g++ -O3 -march=native bench/suite/fact_gmp.cpp -o /tmp/fact_gmp_cpp -lgmpxx -lgmp
// Usage       : /tmp/fact_gmp_cpp [N]

#include <gmpxx.h>
#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <vector>

using namespace std;

static double ms() {
    using clk = chrono::steady_clock;
    return chrono::duration<double, milli>(clk::now().time_since_epoch()).count();
}

/// Nombre EXACT de chiffres (mpz_sizeinbase est approximatif : peut être +1).
static size_t vrais_chiffres(const mpz_class& x) {
    return strlen(x.get_str(10).c_str());
}

/// Produit en arbre d'un vecteur de facteurs (dichotomie).
static mpz_class produit_arbre(const vector<mpz_class>& f, size_t deb, size_t fin) {
    if (fin - deb == 0) return mpz_class(1);
    if (fin - deb == 1) return f[deb];
    size_t m = deb + (fin - deb) / 2;
    return produit_arbre(f, deb, m) * produit_arbre(f, m, fin);
}

/// Multifactorielle d'ordre k : produit de n, n−k, n−2k… > 0.
static mpz_class multifact(unsigned long n, unsigned long k) {
    if (n == 0) return mpz_class(1);
    vector<mpz_class> f;
    f.reserve((n - 1) / k + 1);
    // ⚠ `i` non signé : on sort AVANT que `i -= k` ne déborde.
    for (unsigned long i = n;; i -= k) {
        f.emplace_back(i);
        if (i <= k) break;
    }
    return produit_arbre(f, 0, f.size());
}

static mpz_class factorielle(unsigned long n) {
    if (n == 0) return mpz_class(1);
    vector<mpz_class> f;
    f.reserve(n);
    for (unsigned long i = 1; i <= n; i++) f.emplace_back(i);
    return produit_arbre(f, 0, f.size());
}

int main(int argc, char** argv) {
    unsigned long N = argc > 1 ? strtoul(argv[1], nullptr, 10) : 100000;
    printf("N = %lu\n", N);

    double t = ms();
    mpz_class a = factorielle(N);
    printf("n!       : %.2f ms, %zu chiffres\n", ms() - t, vrais_chiffres(a));

    t = ms();
    a = multifact(N, 2);
    printf("n!!      : %.2f ms, %zu chiffres\n", ms() - t, vrais_chiffres(a));

    t = ms();
    a = multifact(N, 3);
    printf("n!!!     : %.2f ms, %zu chiffres\n", ms() - t, vrais_chiffres(a));

    t = ms();
    a = multifact(N, 4);
    printf("n!!!!    : %.2f ms, %zu chiffres\n", ms() - t, vrais_chiffres(a));

    // !n : récurrence !n = n·!(n−1) + (−1)^n
    t = ms();
    mpz_class d = 1;
    for (unsigned long i = 1; i <= N; i++) {
        d *= i;
        if (i % 2 == 0) d += 1; else d -= 1;
    }
    printf("!n       : %.2f ms, %zu chiffres\n", ms() - t, vrais_chiffres(d));

    // n# : primorielle (crible + arbre)
    t = ms();
    {
        vector<char> prem(N + 1, 0);
        for (unsigned long i = 2; i * i <= N; i++)
            if (!prem[i]) for (unsigned long j = i * i; j <= N; j += i) prem[j] = 1;
        vector<mpz_class> f;
        for (unsigned long i = 2; i <= N; i++) if (!prem[i]) f.emplace_back(i);
        a = produit_arbre(f, 0, f.size());
    }
    printf("n#       : %.2f ms, %zu chiffres\n", ms() - t, vrais_chiffres(a));
    return 0;
}
