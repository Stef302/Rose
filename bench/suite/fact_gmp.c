// Bench GMP : factorielle et sous-factorielle, comparaison avec Rose.
//
// ⚠ Ce programme mesure exactement les mêmes opérations que
// `bench/suite/factorielles.rose`, pour que la comparaison soit honnête :
//   n!      factorielle
//   n!!     multifactorielle d'ordre 2
//   n!!!    multifactorielle d'ordre 3
//   !n      sous-factorielle (dérangements)
//   n#      primorielle
//
// Compilation : gcc -O3 -march=native bench/suite/fact_gmp.c -o /tmp/fact_gmp -lgmp
// Usage       : /tmp/fact_gmp [N]

#include <gmp.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

/// Nombre EXACT de chiffres décimaux.
///
/// ⚠⚠ `mpz_sizeinbase(x, 10)` est APPROXIMATIF : la documentation GMP
/// prévient qu'il « may be one too large ». Mesuré : pour 20000!! il rend
/// 38671 alors que le vrai nombre en a 38670 — ce qui faisait diverger ce
/// bench de Rose d'une unité. On passe donc par `mpz_get_str`, qui est exact
/// (c'est aussi ce que fait Rose avec `longueur(chaine(x))`).
static size_t vrais_chiffres(const mpz_t x) {
    char *s = mpz_get_str(NULL, 10, x);
    size_t n = strlen(s);
    free(s);
    return n;
}

static double ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec * 1000.0 + ts.tv_nsec / 1e6;
}

/// Produit en ARBRE d'un tableau de facteurs (dichotomie).
///
/// ⚠ C'est la méthode de GMP et de Python : la boucle 1·2·…·n fait croître
/// l'opérande à chaque multiplication (O(n²) en coût de chiffres), alors que
/// l'arbre garde des opérandes de taille comparable et profite de la
/// multiplication rapide (Toom-Cook / FFT) — c'est le gain principal.
static void produit_arbre(mpz_t r, const unsigned long *f, size_t deb, size_t fin) {
    if (fin - deb == 0) { mpz_set_ui(r, 1); return; }
    if (fin - deb <= 16) {
        mpz_set_ui(r, 1);
        for (size_t i = deb; i < fin; i++) mpz_mul_ui(r, r, f[i]);
        return;
    }
    size_t m = deb + (fin - deb) / 2;
    mpz_t g, d;
    mpz_init(g); mpz_init(d);
    produit_arbre(g, f, deb, m);
    produit_arbre(d, f, m, fin);
    mpz_mul(r, g, d);
    mpz_clear(g); mpz_clear(d);
}

/// Multifactorielle d'ordre k : produit de n, n−k, n−2k… > 0.
static void multifact(mpz_t r, unsigned long n, unsigned long k) {
    if (n == 0) { mpz_set_ui(r, 1); return; }
    size_t nb = (n - 1) / k + 1;
    unsigned long *f = malloc(nb * sizeof(unsigned long));
    if (!f) { fprintf(stderr, "malloc(%zu) a échoué\n", nb); exit(1); }
    size_t j = 0;
    // ⚠⚠ `i` est NON SIGNÉ : la condition `i > 0` ne suffit PAS. Quand
    // `i < k`, `i -= k` déborde à ~1.8e19, qui reste > 0, et la boucle
    // écrit hors du tableau (mesuré : j = 16223 pour nb = 34 → SIGSEGV).
    // On s'arrête explicitement quand le prochain pas serait ≤ 0.
    for (unsigned long i = n; i >= 1; i -= k) {
        f[j++] = i;
        // Dernier facteur : on s'arrête AVANT de soustraire k si le pas
        // suivant déborderait (i ≤ k). `i == k` doit encore être inclus
        // (ex. 4!! = 4·2, donc 2 est le dernier facteur).
        if (i == k || i < k) break;
    }
    if (j != nb) { fprintf(stderr, "incohérence : j=%zu nb=%zu\n", j, nb); exit(1); }
    produit_arbre(r, f, 0, nb);
    free(f);
}

/// Factorielle : produit en arbre de 1..n.
static void factorielle(mpz_t r, unsigned long n) {
    if (n == 0) { mpz_set_ui(r, 1); return; }
    unsigned long *f = malloc(n * sizeof(unsigned long));
    for (unsigned long i = 0; i < n; i++) f[i] = i + 1;
    produit_arbre(r, f, 0, n);
    free(f);
}

int main(int argc, char **argv) {
    unsigned long N = argc > 1 ? strtoul(argv[1], NULL, 10) : 20000;
    mpz_t a;
    mpz_init(a);

    printf("N = %lu\n", N);

    double t = ms();
    factorielle(a, N);
    printf("n!       : %.2f ms, %zu chiffres\n", ms() - t, vrais_chiffres(a));

    t = ms();
    multifact(a, N, 2);
    printf("n!!      : %.2f ms, %zu chiffres\n", ms() - t, vrais_chiffres(a));

    t = ms();
    multifact(a, N, 3);
    printf("n!!!     : %.2f ms, %zu chiffres\n", ms() - t, vrais_chiffres(a));

    t = ms();
    multifact(a, N, 4);
    printf("n!!!!    : %.2f ms, %zu chiffres\n", ms() - t, vrais_chiffres(a));

    // !n : récurrence !n = n·!(n-1) + (−1)^n
    t = ms();
    mpz_set_ui(a, 1);           // !0 = 1
    for (unsigned long i = 1; i <= N; i++) {
        mpz_mul_ui(a, a, i);
        if (i % 2 == 0) mpz_add_ui(a, a, 1); else mpz_sub_ui(a, a, 1);
    }
    printf("!n       : %.2f ms, %zu chiffres\n", ms() - t, vrais_chiffres(a));

    // n# : primorielle (crible + arbre)
    t = ms();
    {
        char *prem = calloc(N + 1, 1);
        for (unsigned long i = 2; i * i <= N; i++)
            if (!prem[i]) for (unsigned long j = i * i; j <= N; j += i) prem[j] = 1;
        size_t nb = 0;
        for (unsigned long i = 2; i <= N; i++) if (!prem[i]) nb++;
        unsigned long *f = malloc(nb * sizeof(unsigned long));
        size_t j = 0;
        for (unsigned long i = 2; i <= N; i++) if (!prem[i]) f[j++] = i;
        produit_arbre(a, f, 0, nb);
        free(f); free(prem);
    }
    printf("n#       : %.2f ms, %zu chiffres\n", ms() - t, vrais_chiffres(a));

    mpz_clear(a);
    return 0;
}
