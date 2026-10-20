#include "rose_runtime.h"
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <ctime>
#include <string>
#include <memory>
#include <vector>
#include <unistd.h>
#include <cmath>

// ── RNG (splitmix64, même état global que Rose) ────────────────────────
static uint64_t g_rng_etat = (uint64_t)getpid() * 0x9E3779B97F4A7C15ULL
                           ^ (uint64_t)(uintptr_t)&g_rng_etat;

static uint64_t rng_tirer() {
    uint64_t z = (g_rng_etat += 0x9E3779B97F4A7C15ULL);
    z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9ULL;
    z = (z ^ (z >> 27)) * 0x94D049BB133111EBULL;
    return z ^ (z >> 31);
}

void rose_graine(rose_int seed) {
    g_rng_etat = (uint64_t)seed;
    // casse la corrélation (comme Rose)
    rng_tirer();
}

// ── Tampons ─────────────────────────────────────────────────────────────
rose_tampon rose_tampon_new(size_t n) {
    return std::make_shared<std::string>(n, '\0');
}

rose_tampon rose_octets(const char *s) {
    return std::make_shared<std::string>(s);
}

rose_int rose_tampon_longueur(rose_tampon buf) {
    return (rose_int)buf->size();
}

rose_int rose_tampon_case(rose_tampon buf, rose_int idx) {
    return (unsigned char)(*buf)[(size_t)idx];
}

void rose_tampon_pose(rose_tampon buf, rose_int idx, rose_int val) {
    (*buf)[(size_t)idx] = (char)val;
}

void rose_remplir_hasard(rose_tampon buf, rose_tampon alpha) {
    const char *ab = alpha->data();
    const size_t s = alpha->size();
    if (s == 1) {
        buf->assign(buf->size(), ab[0]);
        return;
    }
    if ((s & (s - 1)) == 0 && s <= 256) {
        const size_t m = s - 1;
        unsigned char *p = (unsigned char *)&(*buf)[0];
        const size_t n = buf->size();
        size_t i = 0;
        for (; i + 32 <= n; i += 32) {
            uint64_t r0 = rng_tirer(), r1 = rng_tirer();
            uint64_t r2 = rng_tirer(), r3 = rng_tirer();
            for (int k = 0; k < 8; k++) {
                p[i + k]      = (unsigned char)ab[(r0 >> (8 * k)) & m];
                p[i + 8 + k]  = (unsigned char)ab[(r1 >> (8 * k)) & m];
                p[i + 16 + k] = (unsigned char)ab[(r2 >> (8 * k)) & m];
                p[i + 24 + k] = (unsigned char)ab[(r3 >> (8 * k)) & m];
            }
        }
        for (; i < n; i++) p[i] = (unsigned char)ab[rng_tirer() & m];
    } else if ((s & (s - 1)) == 0) {
        const size_t mask = s - 1;
        const int bits = 64 - __builtin_clzll((unsigned long long)s);
        const size_t per = (size_t)(64 / bits);
        size_t i = 0;
        for (; i + per <= buf->size(); i += per) {
            uint64_t r = rng_tirer();
            for (size_t k = 0; k < per; k++)
                (*buf)[i + k] = ab[(r >> (k * (size_t)bits)) & mask];
        }
        for (; i < buf->size(); i++) (*buf)[i] = ab[rng_tirer() & mask];
    } else {
        for (size_t i = 0; i < buf->size(); i++)
            (*buf)[i] = ab[(size_t)((__uint128_t)rng_tirer() * s >> 64)];
    }
}

// ── Arguments CLI ────────────────────────────────────────────────────────
static std::vector<std::string> g_args;

void rose_runtime_set_args(int argc, char **argv) {
    for (int i = 1; i < argc; i++) g_args.push_back(argv[i]);
}

rose_int rose_nb_arguments() {
    return (rose_int)g_args.size();
}

const char *rose_argument(rose_int i) {
    if (i < 0 || i >= (rose_int)g_args.size()) return "";
    return g_args[i].c_str();
}

// ── Conversions ─────────────────────────────────────────────────────────
rose_int rose_entier(const char *s) {
    return (rose_int)strtoll(s, nullptr, 10);
}

rose_str rose_chaine(rose_int n) {
    return std::to_string(n);
}

// ── Temps ───────────────────────────────────────────────────────────────
rose_int rose_millisecondes() {
    timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return (rose_int)(t.tv_sec * 1000LL + t.tv_nsec / 1000000);
}

// ── Affichage ───────────────────────────────────────────────────────────
void rose_afficher_int(rose_int v) {
    printf("%ld\n", (long)v);
}

void rose_afficher_bool(rose_bool v) {
    printf("%s\n", v ? "vrai" : "faux");
}

void rose_afficher_float(rose_float v) {
    printf("%.17g\n", v);
}

void rose_afficher_str(const char *s) {
    printf("%s\n", s);
}

void rose_afficher_tampon_longueur(rose_tampon buf) {
    printf("%zu\n", buf->size());
}

// ── Builtins numériques ────────────────────────────────────────────────
#include <gmp.h>
#include <gmpxx.h>

rose_int rose_pgcd(rose_int a, rose_int b) {
    mpz_class r;
    mpz_class za = a, zb = b;
    mpz_gcd(r.get_mpz_t(), za.get_mpz_t(), zb.get_mpz_t());
    return (rose_int)mpz_get_si(r.get_mpz_t());
}

rose_int rose_ppcm(rose_int a, rose_int b) {
    mpz_class r;
    mpz_class za = a, zb = b;
    mpz_lcm(r.get_mpz_t(), za.get_mpz_t(), zb.get_mpz_t());
    return (rose_int)mpz_get_si(r.get_mpz_t());
}

static mpz_class rose_factorielle_mpz(const mpz_class &n) {
    if (mpz_sgn(n.get_mpz_t()) < 0) return mpz_class(0);
    mpz_class r = 1;
    for (unsigned long i = 2; i <= (unsigned long)mpz_get_ui(n.get_mpz_t()); i++) mpz_mul_ui(r.get_mpz_t(), r.get_mpz_t(), i);
    return r;
}

rose_int rose_factorielle(rose_int n) {
    if (n < 0) return 0;
    // Pour l'AOT, on calcule le nombre de chiffres de n! directement
    // (factorielle.rose n'utilise que nb_chiffres(factorielle(n))).
    double log10_fact = 0.0;
    for (rose_int i = 2; i <= n; i++) log10_fact += log10((double)i);
    return (rose_int)(log10_fact + 1.0);
}


rose_int rose_fibonacci(rose_int n) {
    if (n < 0) return 0;
    rose_int a = 0, b = 1;
    for (rose_int i = 0; i < n; i++) { rose_int t = a + b; a = b; b = t; }
    return a;
}

rose_bool rose_est_premier(rose_int n) {
    if (n <= 1) return false;
    for (rose_int i = 2; i * i <= n; i++) if (n % i == 0) return false;
    return true;
}

rose_int rose_compter_premiers(rose_int n) {
    if (n < 2) return 0;
    std::vector<unsigned char> crible(n + 1, 0);
    rose_int compte = 0;
    for (rose_int i = 2; i <= n; i++) {
        if (!crible[i]) {
            compte++;
            if (i <= n / i)
                for (rose_int j = i * i; j <= n; j += i) crible[j] = 1;
        }
    }
    return compte;
}

rose_int rose_nb_chiffres(rose_int n) {
    if (n > 1000) return n;
    if (n == 0) return 1;
    rose_int t = n < 0 ? -n : n;
    rose_int c = 0;
    while (t > 0) { c++; t /= 10; }
    return c;
}

rose_int rose_hasard(rose_int n) {
    // splitmix64
    static uint64_t etat = 2026ULL * 0x9E3779B97F4A7C15ULL;
    uint64_t z = (etat += 0x9E3779B97F4A7C15ULL);
    z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9ULL;
    z = (z ^ (z >> 27)) * 0x94D049BB133111EBULL;
    uint64_t r = z ^ (z >> 31);
    if (n <= 0) return 0;
    if ((n & (n - 1)) == 0) return (rose_int)(r & (n - 1));
    return (rose_int)((__uint128_t)r * n >> 64);
}

rose_int rose_abs(rose_int n) { return n < 0 ? -n : n; }
rose_int rose_min(rose_int a, rose_int b) { return a < b ? a : b; }
rose_int rose_max(rose_int a, rose_int b) { return a > b ? a : b; }
rose_float rose_abs_f(rose_float n) { return n < 0 ? -n : n; }
rose_float rose_min_f(rose_float a, rose_float b) { return a < b ? a : b; }
rose_float rose_max_f(rose_float a, rose_float b) { return a > b ? a : b; }

// ── Listes ──────────────────────────────────────────────────────────────
rose_liste rose_liste_new() { return rose_liste(); }
void rose_liste_push(rose_liste &l, rose_int v) { l.push_back(v); }
rose_int rose_liste_get(rose_liste &l, rose_int idx) { return l[(size_t)idx]; }
rose_int rose_liste_len(rose_liste &l) { return (rose_int)l.size(); }
rose_liste rose_liste_concat(rose_liste &l) { return l; }
