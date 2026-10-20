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

// Représentation la plus courte qui relit exactement (miroir de
// texte_flottant de l'interprète), puis rendu comme ecri().
static std::string rose_texte_flottant(double d) {
    char buf[64];
    for (int p = 1; p <= 17; p++) {
        snprintf(buf, sizeof buf, "%.*g", p, d);
        if (strtod(buf, nullptr) == d) {
            std::string s(buf);
            if (s.find('.') == std::string::npos && s.find('e') == std::string::npos
                    && s.find("inf") == std::string::npos && s.find("nan") == std::string::npos)
                s += ".0";
            return s;
        }
    }
    snprintf(buf, sizeof buf, "%.17g", d);
    return std::string(buf);
}

void rose_afficher_float(rose_float v) {
    double d = v;
    if (d != d) { printf("non_nombre\n"); return; }
    if (d == 1.0 / 0.0) { printf("infini\n"); return; }
    if (d == -1.0 / 0.0) { printf("-infini\n"); return; }
    if (d == std::floor(d) && std::fabs(d) < 1e16) { printf("%lld\n", (long long)d); return; }
    printf("%s\n", rose_texte_flottant(d).c_str());
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

// factorielle : arbre de produits (miroir de l'interprète rose.cpp).
static void rose_factorielle_arbre(mpz_class &dest, unsigned long lo, unsigned long hi) {
    if (lo > hi) { dest = 1; return; }
    if (hi - lo < 16) {
        dest = lo;
        for (unsigned long x = lo + 1; x <= hi; x++) mpz_mul_ui(dest.get_mpz_t(), dest.get_mpz_t(), x);
        return;
    }
    unsigned long mid = lo + (hi - lo) / 2;
    mpz_class g, d;
    rose_factorielle_arbre(g, lo, mid);
    rose_factorielle_arbre(d, mid + 1, hi);
    dest = g * d;
}
rose_gd rose_factorielle_gd(rose_int n) {
    if (n < 0) return 0;
    if (n > (rose_int)0xFFFFFFFFUL) return 0;
    mpz_class r;
    rose_factorielle_arbre(r, 1, (unsigned long)n);
    return r;
}

// fibonacci : doublement rapide (miroir de l'interprète rose.cpp).
rose_gd rose_fibonacci_gd(rose_int n) {
    if (n < 0) return 0;
    mpz_class a = 0, b = 1, t1, t2, t3;
    mpz_class nn = n;
    mp_bitcnt_t bits = mpz_sizeinbase(nn.get_mpz_t(), 2);
    for (mp_bitcnt_t i = bits; i-- > 0;) {
        mpz_mul_ui(t1.get_mpz_t(), b.get_mpz_t(), 2);
        mpz_sub(t1.get_mpz_t(), t1.get_mpz_t(), a.get_mpz_t());
        mpz_mul(t1.get_mpz_t(), t1.get_mpz_t(), a.get_mpz_t());
        mpz_mul(t2.get_mpz_t(), a.get_mpz_t(), a.get_mpz_t());
        mpz_mul(t3.get_mpz_t(), b.get_mpz_t(), b.get_mpz_t());
        mpz_add(t2.get_mpz_t(), t2.get_mpz_t(), t3.get_mpz_t());
        if (mpz_tstbit(nn.get_mpz_t(), i)) {
            a = t2;
            mpz_add(b.get_mpz_t(), t1.get_mpz_t(), t2.get_mpz_t());
        } else { a = t1; b = t2; }
    }
    return a;
}

// puissance entière : base^expo, expo >= 0.
rose_gd rose_puissance_gd(const rose_gd &base, rose_int expo) {
    if (expo < 0) return 0;
    mpz_class r;
    mpz_pow_ui(r.get_mpz_t(), base.get_mpz_t(), (unsigned long)expo);
    return r;
}

rose_int rose_nb_chiffres_gd(const rose_gd &n) {
    if (mpz_sgn(n.get_mpz_t()) == 0) return 1;
    rose_gd t; mpz_abs(t.get_mpz_t(), n.get_mpz_t());
    return (rose_int)mpz_sizeinbase(t.get_mpz_t(), 10);   // quirks GMP = sémantique Rose
}

void rose_afficher_gd(const rose_gd &n) {
    gmp_printf("%Zd\n", n.get_mpz_t());
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
    mpz_class z(n);
    if (mpz_sgn(z.get_mpz_t()) == 0) return 1;
    mpz_abs(z.get_mpz_t(), z.get_mpz_t());
    return (rose_int)mpz_sizeinbase(z.get_mpz_t(), 10);   // quirks GMP = sémantique Rose
}

rose_int rose_hasard(rose_int n) {
    if (n <= 0) return 0;
    if ((n & (n - 1)) == 0) return (rose_int)(rng_tirer() & (uint64_t)(n - 1));
    return (rose_int)((__uint128_t)rng_tirer() * (__uint128_t)(uint64_t)n >> 64);
}

rose_int rose_abs(rose_int n) { return n < 0 ? -n : n; }
rose_int rose_min(rose_int a, rose_int b) { return a < b ? a : b; }
rose_int rose_max(rose_int a, rose_int b) { return a > b ? a : b; }
rose_float rose_abs_f(rose_float n) { return n < 0 ? -n : n; }
rose_float rose_min_f(rose_float a, rose_float b) { return a < b ? a : b; }
rose_float rose_max_f(rose_float a, rose_float b) { return a > b ? a : b; }

// ── Listes ──────────────────────────────────────────────────────────────
rose_liste rose_liste_new() { return rose_liste(); }
rose_str rose_liste_get_str(rose_liste &l, rose_int idx) { if (idx < 0 || idx >= (rose_int)l.size()) return ""; return l[(size_t)idx]; }
rose_int rose_liste_len(rose_liste &l) { return (rose_int)l.size(); }

// ── Listes et chaînes ───────────────────────────────────────────────────
void rose_liste_push(rose_liste &l, const char *v) { l.push_back(v); }
rose_str rose_liste_concat(rose_liste &l) {
    rose_str r;
    for (auto &s : l) r += s;
    return r;
}
rose_str rose_index_str(rose_str &s, rose_int idx) {
    if (idx < 0 || idx >= (rose_int)s.size()) return "";
    return std::string(1, s[(size_t)idx]);
}

// ── Rationnels exacts et divisions euclidiennes ──────────────────────
rose_qd rose_q_div(const rose_qd &a, const rose_qd &b) {
    if (mpq_sgn(b.get_mpq_t()) == 0) { fprintf(stderr, "rose : division par zéro\n"); exit(1); }
    return a / b;
}

int rose_q_cmp(const rose_qd &a, const rose_qd &b) {
    return mpq_cmp(a.get_mpq_t(), b.get_mpq_t());
}

bool rose_approx_q(const rose_qd &a, const rose_qd &b, bool au_moins_un_flottant) {
    if (!au_moins_un_flottant) return mpq_cmp(a.get_mpq_t(), b.get_mpq_t()) == 0;
    double x = mpq_get_d(a.get_mpq_t()), y = mpq_get_d(b.get_mpq_t());
    double ecart = x > y ? x - y : y - x;
    double echelle = (x > 0 ? x : -x) > (y > 0 ? y : -y) ? (x > 0 ? x : -x) : (y > 0 ? y : -y);
    if (echelle < 1.0) echelle = 1.0;
    return ecart <= 1e-9 * echelle;
}

bool rose_approx_f(rose_float a, rose_float b) {
    double ecart = a > b ? a - b : b - a;
    double echelle = (a > 0 ? a : -a) > (b > 0 ? b : -b) ? (a > 0 ? a : -a) : (b > 0 ? b : -b);
    if (echelle < 1.0) echelle = 1.0;
    return ecart <= 1e-9 * echelle;
}

rose_int rose_entier_q(const rose_qd &q) {
    mpz_class r;
    mpz_fdiv_q(r.get_mpz_t(), q.get_num().get_mpz_t(), q.get_den().get_mpz_t());
    return (rose_int)mpz_get_si(r.get_mpz_t());
}

// Écriture exacte avec périodes entre crochets (miroir de ecri de l'interprète).
std::string rose_texte_q(const rose_qd &q_in) {
    std::string out_str;
    {
    mpq_class q = q_in;
    q.canonicalize();
    if (q.get_den() == 1) { return q.get_num().get_str().c_str(); }
    if (mpz_sizeinbase(q.get_num().get_mpz_t(), 10)
            + mpz_sizeinbase(q.get_den().get_mpz_t(), 10) > 500) {
        return std::string(q.get_num().get_str().c_str()) + "/" + q.get_den().get_str().c_str();
    }
    mpz_class entier;
    mpz_fdiv_q(entier.get_mpz_t(), q.get_num().get_mpz_t(), q.get_den().get_mpz_t());
    mpz_class reste; mpz_abs(reste.get_mpz_t(), q.get_num().get_mpz_t());
    mpz_mod(reste.get_mpz_t(), reste.get_mpz_t(), q.get_den().get_mpz_t());
    char digits[201];
    int ndig = 0, peri = -1, nrestes = 0;
    mpz_class restes[200]; int pos[200];
    while (mpz_sgn(reste.get_mpz_t()) != 0 && ndig < 200) {
        for (int j = 0; j < nrestes; j++)
            if (restes[j] == reste) { peri = pos[j]; break; }
        if (peri >= 0) break;
        restes[nrestes] = reste; pos[nrestes++] = ndig;
        mpz_mul_ui(reste.get_mpz_t(), reste.get_mpz_t(), 10);
        mpz_class chiffre;
        mpz_fdiv_q(chiffre.get_mpz_t(), reste.get_mpz_t(), q.get_den().get_mpz_t());
        mpz_mod(reste.get_mpz_t(), reste.get_mpz_t(), q.get_den().get_mpz_t());
        digits[ndig++] = (char)('0' + mpz_get_ui(chiffre.get_mpz_t()));
    }
    out_str = entier.get_str().c_str();
    if (ndig) {
        out_str += ".";
        out_str.append(digits, (size_t)(peri >= 0 ? peri : ndig));
        if (peri >= 0) {
            out_str += "[";
            out_str.append(digits + peri, (size_t)(ndig - peri));
            out_str += "]";
        } else if (ndig >= 200) {
            out_str += "…";
        }
    }
    }
    return out_str;
}
void rose_afficher_q(const rose_qd &q) {
    printf("%s\n", rose_texte_q(q).c_str());
}

rose_int rose_div_florentin(rose_int a, rose_int b) {
    if (b == 0) { fprintf(stderr, "rose : division euclidienne par zéro\n"); exit(1); }
    mpz_class x = a, y = b, r;
    mpz_fdiv_q(r.get_mpz_t(), x.get_mpz_t(), y.get_mpz_t());
    return (rose_int)mpz_get_si(r.get_mpz_t());
}

rose_int rose_rem_florentin(rose_int a, rose_int b) {
    if (b == 0) { fprintf(stderr, "rose : reste d'une division par zéro\n"); exit(1); }
    mpz_class x = a, y = b, r;
    mpz_fdiv_r(r.get_mpz_t(), x.get_mpz_t(), y.get_mpz_t());
    return (rose_int)mpz_get_si(r.get_mpz_t());
}

// Rendu texte d'un flottant (miroir de ecri de l'interprète).
std::string rose_texte_flt(rose_float v) {
    double d = v;
    if (d != d) return "non_nombre";
    if (d == 1.0 / 0.0) return "infini";
    if (d == -1.0 / 0.0) return "-infini";
    if (d == std::floor(d) && std::fabs(d) < 1e16) {
        char buf[32]; snprintf(buf, sizeof buf, "%lld", (long long)d);
        return std::string(buf);
    }
    return rose_texte_flottant(d);
}

void rose_liste_push_int(rose_liste &l, rose_int v) { l.push_back(std::to_string(v)); }
void rose_liste_push_gd(rose_liste &l, const rose_gd &v) { l.push_back(v.get_str().c_str()); }
void rose_liste_push_q(rose_liste &l, const rose_qd &v) { l.push_back(rose_texte_q(v)); }
void rose_liste_push_flt(rose_liste &l, rose_float v) { l.push_back(rose_texte_flt(v)); }

// Chaîne → entier (lecture des éléments d'une liste d'entiers).
rose_int rose_a_entier(const rose_str &s) {
    mpz_class r;
    if (mpz_set_str(r.get_mpz_t(), s.c_str(), 10)) return 0;
    return (rose_int)mpz_get_si(r.get_mpz_t());
}

rose_int rose_entier_f(rose_float v) { return (rose_int)v; }   // troncature (mpz_set_d)

void rose_afficher_liste(const rose_liste &l) {
    putchar('[');
    for (size_t i = 0; i < l.size(); i++) {
        if (i) fputs(", ", stdout);
        fputs(l[i].c_str(), stdout);
    }
    fputs("]\n", stdout);
}

// ── Builtin nombre_pi : π avec N décimales (formule de Machin, miroir) ──
rose_str rose_nombre_pi(rose_int decimales) {
    unsigned long chiffres = (unsigned long)decimales;
    if (chiffres == 0 || chiffres > 2000000) { fprintf(stderr, "rose : décimales de π hors limites (1..2000000)\n"); exit(1); }
    unsigned long S = chiffres + 40;
    mpz_class dixS; mpz_ui_pow_ui(dixS.get_mpz_t(), 10, S);
    unsigned long xs[2] = {5, 239};
    mpz_class cibles[2];
    for (int w = 0; w < 2; w++) {
        mpz_class &dest = cibles[w];
        mpz_class x2, p, terme;
        mpz_set_ui(x2.get_mpz_t(), xs[w]); mpz_mul(x2.get_mpz_t(), x2.get_mpz_t(), x2.get_mpz_t());
        mpz_set_ui(p.get_mpz_t(), xs[w]);
        unsigned long k = 1;
        mpz_fdiv_q(terme.get_mpz_t(), dixS.get_mpz_t(), p.get_mpz_t());
        dest = terme;
        int signe = -1;
        for (;;) {
            mpz_mul(p.get_mpz_t(), p.get_mpz_t(), x2.get_mpz_t());
            k += 2;
            mpz_mul_ui(terme.get_mpz_t(), p.get_mpz_t(), k);
            mpz_fdiv_q(terme.get_mpz_t(), dixS.get_mpz_t(), terme.get_mpz_t());
            if (mpz_sgn(terme.get_mpz_t()) == 0) break;
            if (signe < 0) mpz_sub(dest.get_mpz_t(), dest.get_mpz_t(), terme.get_mpz_t());
            else mpz_add(dest.get_mpz_t(), dest.get_mpz_t(), terme.get_mpz_t());
            signe = -signe;
        }
    }
    mpz_class at5 = cibles[0], at239 = cibles[1], pi;
    mpz_mul_ui(at5.get_mpz_t(), at5.get_mpz_t(), 16);
    mpz_mul_ui(at239.get_mpz_t(), at239.get_mpz_t(), 4);
    mpz_sub(pi.get_mpz_t(), at5.get_mpz_t(), at239.get_mpz_t());
    char *brut = mpz_get_str(nullptr, 10, pi.get_mpz_t());
    size_t lbrut = strlen(brut);
    std::string sortie(chiffres + 2, ' ');
    sortie[0] = brut[0]; sortie[1] = '.';
    for (unsigned long i = 0; i < chiffres; i++)
        sortie[2 + i] = (i + 1 < lbrut) ? brut[i + 1] : '0';
    if (1 + chiffres < lbrut && brut[1 + chiffres] >= '5') {
        long i = (long)chiffres + 1;
        while (i >= 0) {
            if (sortie[(size_t)i] == '.') { i--; continue; }
            sortie[(size_t)i]++;
            if (sortie[(size_t)i] <= '9') break;
            sortie[(size_t)i] = '0'; i--;
        }
    }
    free(brut);
    return sortie;
}

// partie_entiere : plancher (miroir de l'interprète).
rose_int rose_partie_entiere_f(rose_float v) { return (rose_int)std::floor(v); }
rose_int rose_partie_entiere_q(const rose_qd &q) { return rose_entier_q(q); }

// Division euclidienne (plancher) sur grands entiers.
rose_gd rose_div_florentin_gd(const rose_gd &a, const rose_gd &b) {
    if (mpz_sgn(b.get_mpz_t()) == 0) { fprintf(stderr, "rose : division euclidienne par zéro\n"); exit(1); }
    mpz_class r;
    mpz_fdiv_q(r.get_mpz_t(), a.get_mpz_t(), b.get_mpz_t());
    return r;
}

// Reste euclidien (plancher) sur grands entiers.
rose_gd rose_rem_florentin_gd(const rose_gd &a, const rose_gd &b) {
    if (mpz_sgn(b.get_mpz_t()) == 0) { fprintf(stderr, "rose : reste d'une division par zéro\n"); exit(1); }
    mpz_class r;
    mpz_fdiv_r(r.get_mpz_t(), a.get_mpz_t(), b.get_mpz_t());
    return r;
}

// Variantes grands entiers exactes (GMP) — les arguments int64 se convertissent.
rose_gd rose_pgcd_gd(const rose_gd &a, const rose_gd &b) {
    rose_gd r; mpz_gcd(r.get_mpz_t(), a.get_mpz_t(), b.get_mpz_t()); return r;
}
rose_gd rose_ppcm_gd(const rose_gd &a, const rose_gd &b) {
    rose_gd r; mpz_lcm(r.get_mpz_t(), a.get_mpz_t(), b.get_mpz_t()); return r;
}
rose_gd rose_abs_gd(const rose_gd &n) {
    rose_gd r; mpz_abs(r.get_mpz_t(), n.get_mpz_t()); return r;
}
rose_gd rose_min_gd(const rose_gd &a, const rose_gd &b) { return a < b ? a : b; }
rose_gd rose_max_gd(const rose_gd &a, const rose_gd &b) { return a > b ? a : b; }
