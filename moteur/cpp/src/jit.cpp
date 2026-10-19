// =============================================================================
// rose-jit — Compilateur JIT natif pour le RIR de Rose (AArch64, sans asmjit)
// Jalon M2 : assembleur AArch64 minimal intégré + génération de code machine
// pour le sous-ensemble RIR : cst.z / add.z / sub.z / mul.z / cmp.lt.z /
// ret.z / br / br.cond. Les opérations GMP délèguent à des helpers C++.
// =============================================================================

#include "jit.h"
#include "rir.h"   // définitions complètes : Valeur, RirOp, RirInstr, RirBloc, RirFonction, RirModule, RirType

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <cerrno>
#include <cctype>
#include <cmath>
#include <stdexcept>
#include <algorithm>
#include <vector>
#include <map>
#include <set>
#include <unordered_set>
#include <unordered_map>
#include <sys/mman.h>
#include <gmp.h>
#include <gmpxx.h>

// Trace de compilation JIT (activée par ROSE_JIT_DEBUG=1)
static bool jit_verbose() { static const bool v = getenv("ROSE_JIT_DEBUG") != nullptr; return v; }
#define JITLOG(...) do { if (jit_verbose()) fprintf(stderr, __VA_ARGS__); } while (0)

// Ensemble des objets de constantes partagées par les modules JIT. Ces
// objets vivent aussi longtemps que le code émis : un code généré qui
// retourne une constante (temp aliasé) ne doit JAMAIS les envoyer au
// réservoir d'objets — jit_free_valeur les y bloque.
static std::unordered_set<Valeur*> g_shared_const_lookup;

// =============================================================================
// Helpers C++ (opérations GMP, allouent sur le tas)
// ── Fast-path entier machine (jalon M2.5) ───────────────────────────────────
// Quand les deux opérandes tiennent dans un signed long, l'opération se fait
// en arithmétique machine avec détection de débordement ; le débordement (ou
// un opérande hors plage) retombe sur GMP. L'arithmétique de Rose restant
// exactement la même (entiers arbitraires), ces chemins sont invisibles
// sémantiquement — ils ne changent que la vitesse.
// =============================================================================

// Les deux opérandes sont-ils des entiers machine ? (extraits dans *x, *y)
// Accès DIRECT aux membres de __mpz_struct (ABI GMP 6 stable) : un nombre à
// un seul membre tient en machine sauf si le signe l'empêche. Zéro appel
// libgmp sur le chemin rapide — c'est tout l'intérêt du fast-path.
static inline bool zfast(mpz_ptr a, long *x) {
    int sz = a->_mp_size;
    if (sz == 0) { *x = 0; return true; }
    if (sz == 1) {
        mp_limb_t d = a->_mp_d[0];
        if ((long)d >= 0) { *x = (long)d; return true; }   // membre < 2^63
        return false;
    }
    if (sz == -1) {
        mp_limb_t d = a->_mp_d[0];
        unsigned long u = 0 - (unsigned long)d;            // |v| → négatif
        long v = (long)u;                                  // wrap défini (gcc/clang)
        if (v < 0) { *x = v; return true; }                // inclut LONG_MIN exact
        return false;
    }
    return false;                                          // multi-membre : GMP
}
static inline bool zs2(Valeur* a, Valeur* b, long *x, long *y) {
    return zfast(a->z.get_mpz_t(), x) && zfast(b->z.get_mpz_t(), y);
}
static inline bool zs1(Valeur* a, long *x) { return zfast(a->z.get_mpz_t(), x); }
// Écrit un entier machine dans dst sans appel libgmp (capacité déjà réservée ;
// sinon mpz_set_si se charge de l'allocation — premier passage seulement).
static inline void zput(Valeur* dst, long v) {
    __mpz_struct *m = dst->z.get_mpz_t();
    if (m->_mp_alloc >= 1) {
        if (v > 0)      { m->_mp_d[0] = (unsigned long)v;   m->_mp_size = 1;  }
        else if (v < 0) { m->_mp_d[0] = 0 - (unsigned long)v; m->_mp_size = -1; }
        else            { m->_mp_size = 0; }
    } else mpz_set_si(m, v);
}

// Division euclidienne (plancher) en machine : corrige la troncature C en
// arrondi plancher, comme mpz_fdiv_q/r.
static inline void fdiv_rem(long x, long y, long *q, long *r) {
    *q = x / y; *r = x % y;
    if (*r != 0 && ((*r < 0) != (y < 0))) { *q -= 1; *r += y; }
}

// ── Réservoirs d'objets et de frames (jalon M2.5) ─────────────────────────
// Les appels JIT (jit_call) créaient et détruisaient frame + objets à
// chaque passage. Les objets retournés au réservoir sont rendus « neufs »
// par leurs repreneurs (les helpers _into écrivent la valeur et le ty de
// toute façon) ; la capacité GMP des mpz se réutilise, ce qui évite aussi
// les malloc internes de GMP. Valeur a des champs indépendants (z/b/s/…) :
// le recyclage d'un type à l'autre est sûr. Fuite volontaire bornée, comme
// le code émis lui-même.
static std::vector<Valeur*> g_valeur_pool;
static std::map<int, std::vector<Valeur**>> g_frame_pool;
static const size_t VALEUR_POOL_MAX = 1u << 16;
static const size_t FRAME_POOL_MAX  = 1u << 12;

static inline void valeur_pool_push(Valeur* v) {
    if (g_valeur_pool.size() < VALEUR_POOL_MAX) g_valeur_pool.push_back(v);
    else delete v;
}
static inline Valeur* z_new0() {           // Valeur entière 0 (réservoir si possible)
    if (!g_valeur_pool.empty()) {
        Valeur* v = g_valeur_pool.back(); g_valeur_pool.pop_back();
        zput(v, 0); v->ty = T_ENTIER; return v;
    }
    return new Valeur(Valeur::de_z(mpz_class(0)));
}
static inline Valeur* b_new(bool x) {      // Valeur booléenne (réservoir si possible)
    if (!g_valeur_pool.empty()) {
        Valeur* v = g_valeur_pool.back(); g_valeur_pool.pop_back();
        v->b = x; v->ty = T_BOOLEEN; return v;
    }
    return new Valeur(Valeur::de_b(x));
}

Valeur* __attribute__((noinline)) jit_val_z(int64_t v) {
    mpz_class z(v);
    Valeur* p = new Valeur(Valeur::de_z(z));
    g_shared_const_lookup.insert(p);   // ces objets sont partagés : jamais à la poubelle
    return p;
}
Valeur* __attribute__((noinline)) jit_val_b(bool v) {
    Valeur* p = new Valeur(Valeur::de_b(v));
    g_shared_const_lookup.insert(p);
    return p;
}

Valeur* __attribute__((noinline)) jit_add_z(Valeur* a, Valeur* b) {
    mpz_class r; mpz_add(r.get_mpz_t(), a->z.get_mpz_t(), b->z.get_mpz_t());
    return new Valeur(Valeur::de_z(r));
}
Valeur* __attribute__((noinline)) jit_sub_z(Valeur* a, Valeur* b) {
    mpz_class r; mpz_sub(r.get_mpz_t(), a->z.get_mpz_t(), b->z.get_mpz_t());
    return new Valeur(Valeur::de_z(r));
}
Valeur* __attribute__((noinline)) jit_mul_z(Valeur* a, Valeur* b) {
    mpz_class r; mpz_mul(r.get_mpz_t(), a->z.get_mpz_t(), b->z.get_mpz_t());
    return new Valeur(Valeur::de_z(r));
}
Valeur* __attribute__((noinline)) jit_div_z(Valeur* a, Valeur* b) {
    if (mpz_sgn(b->z.get_mpz_t()) == 0) { fprintf(stderr, "Rose : erreur — division par zéro\n"); std::fflush(stdout); std::exit(1); }
    mpz_class r; mpz_fdiv_q(r.get_mpz_t(), a->z.get_mpz_t(), b->z.get_mpz_t());
    return new Valeur(Valeur::de_z(r));
}
Valeur* __attribute__((noinline)) jit_rem_z(Valeur* a, Valeur* b) {
    if (mpz_sgn(b->z.get_mpz_t()) == 0) { fprintf(stderr, "Rose : erreur — modulo par zéro\n"); std::fflush(stdout); std::exit(1); }
    mpz_class r; mpz_fdiv_r(r.get_mpz_t(), a->z.get_mpz_t(), b->z.get_mpz_t());
    return new Valeur(Valeur::de_z(r));
}
Valeur* __attribute__((noinline)) jit_neg_z(Valeur* a) {
    mpz_class r = a->z; mpz_neg(r.get_mpz_t(), r.get_mpz_t());
    return new Valeur(Valeur::de_z(r));
}

// Comparaisons : si les deux opérandes tiennent en machine, la comparaison de
// longs coïncide avec mpz_cmp (ordre total des entiers) ; sinon GMP.
static inline long cmp_z(Valeur* a, Valeur* b) {
    long x, y;
    if (zs2(a, b, &x, &y)) return (x < y) ? -1 : (x > y ? 1 : 0);
    return mpz_cmp(a->z.get_mpz_t(), b->z.get_mpz_t());
}
int __attribute__((noinline)) jit_cmp_lt_z(Valeur* a, Valeur* b) { return cmp_z(a, b) < 0; }
int __attribute__((noinline)) jit_cmp_eq_z(Valeur* a, Valeur* b) { return cmp_z(a, b) == 0; }
int __attribute__((noinline)) jit_cmp_ne_z(Valeur* a, Valeur* b) { return cmp_z(a, b) != 0; }
int __attribute__((noinline)) jit_cmp_le_z(Valeur* a, Valeur* b) { return cmp_z(a, b) <= 0; }
int __attribute__((noinline)) jit_cmp_gt_z(Valeur* a, Valeur* b) { return cmp_z(a, b) > 0; }
int __attribute__((noinline)) jit_cmp_ge_z(Valeur* a, Valeur* b) { return cmp_z(a, b) >= 0; }
int __attribute__((noinline)) jit_test_b(Valeur* v) { return v->b ? 1 : 0; }

Valeur* __attribute__((noinline)) jit_and_b(Valeur* a, Valeur* b) { return new Valeur(Valeur::de_b(a->b && b->b)); }
Valeur* __attribute__((noinline)) jit_or_b(Valeur* a, Valeur* b)  { return new Valeur(Valeur::de_b(a->b || b->b)); }
Valeur* __attribute__((noinline)) jit_not_b(Valeur* a)            { return new Valeur(Valeur::de_b(!a->b)); }

// ── Helpers à réutilisation du slot de destination ────────────────────
// ABI : x0 = dst (ancien objet du slot, ou null au premier passage),
//       x1/x2 = opérandes. Retourne dst. Zéro allocation par itération
//       de boucle après le premier passage (mpz_* accepte dst == opérande).

Valeur* __attribute__((noinline)) jit_add_z_into(Valeur* dst, Valeur* a, Valeur* b) {
    if (!dst) dst = z_new0();
    long x, y, r;
    if (zs2(a, b, &x, &y) && !__builtin_add_overflow(x, y, &r)) zput(dst, r);
    else mpz_add(dst->z.get_mpz_t(), a->z.get_mpz_t(), b->z.get_mpz_t());
    dst->ty = T_ENTIER; return dst;
}
Valeur* __attribute__((noinline)) jit_sub_z_into(Valeur* dst, Valeur* a, Valeur* b) {
    if (!dst) dst = z_new0();
    long x, y, r;
    if (zs2(a, b, &x, &y) && !__builtin_sub_overflow(x, y, &r)) zput(dst, r);
    else mpz_sub(dst->z.get_mpz_t(), a->z.get_mpz_t(), b->z.get_mpz_t());
    dst->ty = T_ENTIER; return dst;
}
Valeur* __attribute__((noinline)) jit_mul_z_into(Valeur* dst, Valeur* a, Valeur* b) {
    if (!dst) dst = z_new0();
    long x, y, r;
    if (zs2(a, b, &x, &y) && !__builtin_mul_overflow(x, y, &r)) zput(dst, r);
    else mpz_mul(dst->z.get_mpz_t(), a->z.get_mpz_t(), b->z.get_mpz_t());
    dst->ty = T_ENTIER; return dst;
}
Valeur* __attribute__((noinline)) jit_div_z_into(Valeur* dst, Valeur* a, Valeur* b) {
    if (!dst) dst = z_new0();
    long x, y, q, r;
    if (zs2(a, b, &x, &y)) {
        if (y == 0) { fprintf(stderr, "Rose : erreur — division par zéro\n"); std::fflush(stdout); std::exit(1); }
        fdiv_rem(x, y, &q, &r); zput(dst, q);
    } else {
        if (mpz_sgn(b->z.get_mpz_t()) == 0) { fprintf(stderr, "Rose : erreur — division par zéro\n"); std::fflush(stdout); std::exit(1); }
        mpz_fdiv_q(dst->z.get_mpz_t(), a->z.get_mpz_t(), b->z.get_mpz_t());
    }
    dst->ty = T_ENTIER; return dst;
}
Valeur* __attribute__((noinline)) jit_rem_z_into(Valeur* dst, Valeur* a, Valeur* b) {
    if (!dst) dst = z_new0();
    long x, y, q, r;
    if (zs2(a, b, &x, &y)) {
        if (y == 0) { fprintf(stderr, "Rose : erreur — modulo par zéro\n"); std::fflush(stdout); std::exit(1); }
        fdiv_rem(x, y, &q, &r); zput(dst, r);
    } else {
        if (mpz_sgn(b->z.get_mpz_t()) == 0) { fprintf(stderr, "Rose : erreur — modulo par zéro\n"); std::fflush(stdout); std::exit(1); }
        mpz_fdiv_r(dst->z.get_mpz_t(), a->z.get_mpz_t(), b->z.get_mpz_t());
    }
    dst->ty = T_ENTIER; return dst;
}
Valeur* __attribute__((noinline)) jit_neg_z_into(Valeur* dst, Valeur* a) {
    if (!dst) dst = z_new0();
    long x;
    if (zs1(a, &x) && x != LONG_MIN) zput(dst, -x);
    else mpz_neg(dst->z.get_mpz_t(), a->z.get_mpz_t());
    dst->ty = T_ENTIER; return dst;
}
// copie de valeur (affectations) : dst possède son propre objet, jamais partagé
Valeur* __attribute__((noinline)) jit_z_copy_into(Valeur* dst, Valeur* src) {
    if (!src) return dst;
    if (!dst) dst = z_new0();
    long x;
    if (zs1(src, &x)) zput(dst, x);
    else mpz_set(dst->z.get_mpz_t(), src->z.get_mpz_t());
    dst->ty = T_ENTIER; return dst;
}
Valeur* __attribute__((noinline)) jit_b_copy_into(Valeur* dst, Valeur* src) {
    if (!src) return dst;
    if (!dst) dst = b_new(src->b);
    else { dst->b = src->b; dst->ty = T_BOOLEEN; }
    return dst;
}
Valeur* __attribute__((noinline)) jit_bool_into(Valeur* dst, int cond) {
    if (!dst) dst = b_new(cond != 0);
    else { dst->b = (cond != 0); dst->ty = T_BOOLEEN; }
    return dst;
}
int __attribute__((noinline)) jit_cmp_eq_b(Valeur* a, Valeur* b) { return a->b == b->b; }
int __attribute__((noinline)) jit_cmp_ne_b(Valeur* a, Valeur* b) { return a->b != b->b; }

// ── Flottants (M2.12) ────────────────────────────────────────────────────
// Conversions et formatage miroirs exacts de vers_d / texte_flottant / ecri
// de l'interpréteur : mêmes entrées, mêmes sorties, mêmes erreurs.

static void jit_erreur(const char *msg) {
    std::fflush(stdout);
    fprintf(stderr, "Rose : erreur — %s\n", msg);
    std::exit(1);
}

static double val_d(const Valeur *v) {              // miroir de vers_d
    switch (v->ty) {
        case T_FLOTTANT:  return v->f;
        case T_ENTIER:    return mpz_get_d(v->z.get_mpz_t());
        case T_RATIONNEL: return mpq_get_d(v->q.get_mpq_t());
        case T_BOOLEEN:   return v->b ? 1.0 : 0.0;
    }
    jit_erreur("conversion vers flottant impossible");
    return 0.0;
}

static void ecrit_f(double d) {                     // miroir de ecri T_FLOTTANT
    char buf[64];
    if (d != d) { fputs("non_nombre", stdout); return; }
    if (d == 1.0 / 0.0) { fputs("infini", stdout); return; }
    if (d == -1.0 / 0.0) { fputs("-infini", stdout); return; }
    if (d == std::floor(d) && std::fabs(d) < 1e16) {
        snprintf(buf, sizeof buf, "%lld", (long long)d);
        fputs(buf, stdout); return;
    }
    for (int p = 1; p <= 17; p++) {                 // miroir de texte_flottant
        snprintf(buf, sizeof buf, "%.*g", p, d);
        if (strtod(buf, nullptr) == d) {
            std::string s(buf);
            if (s.find('.') == std::string::npos && s.find('e') == std::string::npos
                    && s.find("inf") == std::string::npos && s.find("nan") == std::string::npos)
                s += ".0";
            fputs(s.c_str(), stdout); return;
        }
    }
    snprintf(buf, sizeof buf, "%.17g", d);
    fputs(buf, stdout);
}

static inline Valeur* f_new0() {                    // flottant 0 (réservoir si possible)
    if (!g_valeur_pool.empty()) {
        Valeur* v = g_valeur_pool.back(); g_valeur_pool.pop_back();
        v->f = 0.0; v->ty = T_FLOTTANT; return v;
    }
    return new Valeur(Valeur::de_f(0.0));
}
static inline Valeur* fput_f(Valeur* dst, double x) {
    if (!dst) dst = f_new0();
    dst->f = x; dst->ty = T_FLOTTANT; return dst;
}

Valeur* __attribute__((noinline)) jit_f_copy_into(Valeur* dst, Valeur* src) {
    double x = val_d(src);                          // tolérant comme l'interpréteur
    if (!dst) dst = f_new0();
    dst->f = x; dst->ty = T_FLOTTANT; return dst;
}
Valeur* __attribute__((noinline)) jit_add_f_into(Valeur* dst, Valeur* a, Valeur* b) {
    return fput_f(dst, val_d(a) + val_d(b));
}
Valeur* __attribute__((noinline)) jit_sub_f_into(Valeur* dst, Valeur* a, Valeur* b) {
    return fput_f(dst, val_d(a) - val_d(b));
}
Valeur* __attribute__((noinline)) jit_mul_f_into(Valeur* dst, Valeur* a, Valeur* b) {
    return fput_f(dst, val_d(a) * val_d(b));
}
Valeur* __attribute__((noinline)) jit_div_f_into(Valeur* dst, Valeur* a, Valeur* b) {
    double B = val_d(b);
    if (B == 0.0) jit_erreur("division par zéro");
    return fput_f(dst, val_d(a) / B);
}
Valeur* __attribute__((noinline)) jit_divf_f_into(Valeur* dst, Valeur* a, Valeur* b) {
    double B = val_d(b);
    if (B == 0.0) jit_erreur("division euclidienne par zéro");
    return fput_f(dst, std::floor(val_d(a) / B));
}
Valeur* __attribute__((noinline)) jit_pow_f_into(Valeur* dst, Valeur* a, Valeur* b) {
    return fput_f(dst, std::pow(val_d(a), val_d(b)));
}
Valeur* __attribute__((noinline)) jit_neg_f_into(Valeur* dst, Valeur* a) {
    return fput_f(dst, -val_d(a));
}
void __attribute__((noinline)) jit_afficher_f(Valeur* v) {
    ecrit_f(v->f); fputc('\n', stdout);
}
// Comparaisons : miroir de cmp_vals côté flottant (A<B / A>B → 0 sinon,
// donc eq(NaN,NaN) = vrai comme l'interpréteur, jamais via A==B).
static int cmp_f(Valeur* a, Valeur* b) {
    double A = val_d(a), B = val_d(b);
    return A < B ? -1 : (A > B ? 1 : 0);
}
int __attribute__((noinline)) jit_cmp_lt_f(Valeur* a, Valeur* b) { return cmp_f(a,b) < 0; }
int __attribute__((noinline)) jit_cmp_le_f(Valeur* a, Valeur* b) { return cmp_f(a,b) <= 0; }
int __attribute__((noinline)) jit_cmp_gt_f(Valeur* a, Valeur* b) { return cmp_f(a,b) > 0; }
int __attribute__((noinline)) jit_cmp_ge_f(Valeur* a, Valeur* b) { return cmp_f(a,b) >= 0; }
int __attribute__((noinline)) jit_cmp_eq_f(Valeur* a, Valeur* b) { return cmp_f(a,b) == 0; }
int __attribute__((noinline)) jit_cmp_ne_f(Valeur* a, Valeur* b) { return cmp_f(a,b) != 0; }

void __attribute__((noinline)) jit_afficher_z(Valeur* v) {
    gmp_printf("%Zd\n", v->z.get_mpz_t());
}
void __attribute__((noinline)) jit_afficher_b(Valeur* v) {
    fputs(v->b ? "vrai\n" : "faux\n", stdout);
}
void __attribute__((noinline)) jit_afficher_s(Valeur* v) {
    fwrite(v->s.data(), 1, v->s.size(), stdout);
    fputc('\n', stdout);
}

// Constante flottante partagée (M2.12) : texte canonique relu par strtod.
Valeur* jit_val_f(double d) {
    Valeur* p = new Valeur(Valeur::de_f(d));
    g_shared_const_lookup.insert(p);
    return p;
}

// Constante entière arbitrairement grande : texte décimal (après préfixe @c)
Valeur* __attribute__((noinline)) jit_val_z_txt(const char* txt) {
    mpz_class z;
    if (mpz_set_str(z.get_mpz_t(), txt, 10) != 0) {
        fprintf(stderr, "jit: constante entière invalide: %s\n", txt);
        std::abort();
    }
    Valeur* p = new Valeur(Valeur::de_z(z));
    g_shared_const_lookup.insert(p);
    return p;
}

// =============================================================================
// Mini assembleur AArch64 (encodage direct d'opcodes, buffer mmap exécutable)
// =============================================================================

struct A64Buf {
    uint8_t* ptr = nullptr;
    size_t cap = 0;
    size_t len = 0;

    ~A64Buf() {
        if (ptr) munmap(ptr, cap);
    }

    void ensure(size_t needed) {
        if (cap >= needed) return;
        size_t new_cap = cap ? cap * 2 : 4096;
        while (new_cap < needed) new_cap *= 2;
        size_t page = 4096;
        new_cap = (new_cap + page - 1) & ~(page - 1);
        void* p = mmap(nullptr, new_cap, PROT_READ | PROT_WRITE,
                       MAP_ANONYMOUS | MAP_PRIVATE, -1, 0);
        if (p == MAP_FAILED) { perror("mmap"); exit(1); }
        if (ptr) {
            memcpy(p, ptr, len);
            munmap(ptr, cap);
        }
        ptr = (uint8_t*)p;
        cap = new_cap;
    }

    void emit32(uint32_t insn) {
        ensure(len + 4);
        ptr[len+0] = (uint8_t)(insn & 0xFF);
        ptr[len+1] = (uint8_t)((insn >> 8) & 0xFF);
        ptr[len+2] = (uint8_t)((insn >> 16) & 0xFF);
        ptr[len+3] = (uint8_t)((insn >> 24) & 0xFF);
        len += 4;
    }

    void align16() {
        while (len % 16) emit32(0xD503201F);  // NOP
    }

    void make_exec() {
        size_t page = 4096;
        uintptr_t start = (uintptr_t)ptr;
        uintptr_t end = start + len;
        uintptr_t pstart = start & ~(page - 1);
        uintptr_t pend = (end + page - 1) & ~(page - 1);
        size_t msize = pend - pstart;
        if (mprotect((void*)pstart, msize, PROT_READ | PROT_WRITE | PROT_EXEC) != 0) {
            perror("mprotect");
        }
    }

    uint8_t* data_ptr() { return ptr; }
    size_t size() const { return len; }
};

// Registres AArch64
static const int X0 = 0,  X1 = 1,  X2 = 2,  X3 = 3;
static const int X4 = 4,  X5 = 5,  X6 = 6,  X7 = 7;
static const int X8 = 8,  X9 = 9;
static const int X17 = 17, X19 = 19, X20 = 20, X29 = 29, X30 = 30;
static const int SP = 31;

// Encodages AArch64 64-bit (vérifiés avec objdump)
static const uint32_t A64_STP_FP_LR_PRE  = 0xA9BF7BFD;  // stp x29, x30, [sp, #-16]!
static const uint32_t A64_MOV_X29_SP     = 0x910003FD;  // mov x29, sp
static const uint32_t A64_LDP_FP_LR_POST = 0xA8C17BFD;  // ldp x29, x30, [sp], #16
static const uint32_t A64_RET            = 0xD65F03C0;  // ret

struct Fixup {
    uint32_t insn_pos;
    std::string label;
    bool is_cond;
    int rt;
    int cc = -1;    // >=0 : B.cond <cc> ; -2 : CBZ ; -1 : B ou CBNZ (is_cond)
};

class A64Asm {
public:
    A64Buf &buf;
    std::unordered_map<std::string, uint32_t> label_pos;
    std::vector<Fixup> fixups;

    explicit A64Asm(A64Buf &b) : buf(b) {}

    void emit(uint32_t insn) { buf.emit32(insn); }

    uint32_t cur_pos() const { return (uint32_t)(buf.size() / 4); }

    void bind(const std::string &name) {
        label_pos[name] = cur_pos();
    }

    void declare(const std::string &name) {
        if (label_pos.find(name) == label_pos.end())
            label_pos[name] = 0xFFFFFFFF;
    }

    void mov_imm(int xd, uint64_t imm) {
        bool first = true;
        for (int i = 0; i < 4; i++) {
            uint16_t part = (imm >> (i * 16)) & 0xFFFF;
            if (part == 0 && !first) continue;
            first = false;
            if (i == 0) {
                uint32_t insn = 0xD2800000 | (part << 5) | xd;  // movz Xd, 64-bit
                emit(insn);
            } else {
                uint32_t insn = 0xF2800000 | (part << 5) | xd;  // movk Xd, 64-bit
                insn |= (i << 21);    // shift = i * 16 (hw = i)
                emit(insn);
            }
        }
    }

    void mov_reg(int xd, int xn) {
        emit(0xAA0003E0 | (xn << 16) | xd);  // orr xd, xzr, xn  (64-bit MOV)
    }

    // ATTENTION: orr avec Rm=31 donne XZR, pas SP → utiliser add xd, sp, #0
    void mov_from_sp(int xd) {
        emit(0x910003E0 | xd);               // add xd, sp, #0
    }

    void ldr(int xd, int xn, int imm_scaled8) {
        emit(0xF9400000 | (imm_scaled8 << 10) | (xn << 5) | xd);
    }

    void str(int xt, int xn, int imm_scaled8) {
        emit(0xF9000000 | (imm_scaled8 << 10) | (xn << 5) | xt);
    }

    void sub_sp(int n) {   // sub sp, sp, #n (n multiple de 8, < 4096)
        emit(0xD10003FF | (n << 10));
    }

    void add_sp(int n) {   // add sp, sp, #n
        emit(0x910003FF | (n << 10));
    }

    void blr_helper(int helper_index) {
        // ldr x17, [x19, #helper_index * 8]  (charger l'adresse du helper depuis le contexte)
        emit(0xF9400000 | (helper_index << 10) | (19 << 5) | 17);
        // blr x17
        emit(0xD63F0200 | (X17 << 5));
    }

    void b_label(const std::string &name) {
        declare(name);
        fixups.push_back({cur_pos(), name, false, 0});
        emit(0x14000000);
    }

    void cbnz_label(int wd, const std::string &name) {
        declare(name);
        fixups.push_back({cur_pos(), name, true, wd});
        emit(0xB5000000 | (wd << 5));   // CBNZ 64-bit (0xB4… serait CBZ !)
    }

    // CBZ / B.cond <cc> — mêmes fixups que CBNZ/B (offsets imm19 signés)
    void cbz_label(int wd, const std::string &name) {
        declare(name);
        fixups.push_back({cur_pos(), name, false, wd, -2});
        emit(0xB4000000 | (wd << 5));
    }
    void bcond_label(int cc, const std::string &name) {
        declare(name);
        fixups.push_back({cur_pos(), name, false, 0, cc});
        emit(0x54000000 | (cc & 15));
    }

    void ret() {
        emit(A64_RET);
    }

    // ── encodages arithmétiques de l'inline JIT (tous vérifiés via llvm-mc) ──
    // LDR/STR w : l'imm12 est l'échelle de 4 (champ = octets/4) ; STRB non mis à l'échelle.
    void ldr_w(int wd, int xn, int off)  { emit(0xB9400000 | ((off >> 2) << 10) | (xn << 5) | wd); }
    void str_w(int wd, int xn, int off)  { emit(0xB9000000 | ((off >> 2) << 10) | (xn << 5) | wd); }
    void str_b(int wd, int xn, int off)  { emit(0x39000000 | (off << 10) | (xn << 5) | wd); }
    void cmp_w_imm(int wn, uint32_t imm) { emit(0x71000000 | (imm << 10) | (wn << 5) | 31); }
    void cmp_x_imm0(int xn)              { emit(0xF1000000 | (xn << 5) | 31); }
    void cmp_x(int xn, int xm)           { emit(0xEB000000 | (xm << 16) | (xn << 5) | 31); }
    // cmp Xn, Xm ASR #63   (SUBS XZR, Xn, Xm asr 63)
    void cmp_x_asr63(int xn, int xm)     { emit(0xEB000000 | (1u << 23) | (63u << 10) | (xm << 16) | (xn << 5) | 31); }
    void adds_x(int wd, int wn, int wm)  { emit(0xAB000000 | (wm << 16) | (wn << 5) | wd); }
    void subs_x(int wd, int wn, int wm)  { emit(0xCB000000 | (wm << 16) | (wn << 5) | wd); }
    void mul_x(int wd, int wn, int wm)   { emit(0x9B007C00 | (wm << 16) | (wn << 5) | wd); }
    void smulh_x(int wd, int wn, int wm) { emit(0x9B407C00 | (wm << 16) | (wn << 5) | wd); }
    void neg_x(int wd, int wm)           { emit(0xCB0003E0 | (wm << 16) | wd); }
    // ASR Xd, Xn, #n  (== SBFM Xd, Xn, #n, #63)
    void asr_x(int wd, int wn, int n)    { emit(0x93400000 | (1u << 21) | (n << 16) | (63u << 10) | (wn << 5) | wd); }
    void orr_x_imm1(int wd, int wn)      { emit(0xB2400000 | (wn << 5) | wd); }   // orr Xd, Xn, #1
    // CSET Xd, <cc>  (== SBCS Xd, XZR, XZR avec condition inversée)
    void cset_x(int wd, int cc)          { emit(0x9A9F07E0 | (((cc ^ 1) & 15) << 12) | wd); }
    void csel_x(int wd, int wn, int wm, int cc) { emit(0x9A800000 | (wm << 16) | ((cc & 15) << 12) | (wn << 5) | wd); }
    void eor_x(int wd, int wn, int wm)   { emit(0xCA000000 | (wm << 16) | (wn << 5) | wd); }

    // ── encodages flottants (M2.12, vérifiés llvm-mc : fd401c20 / 1e612800…) ──
    // LDR/STR D : champ d'immédiat identique au LDR X (octets/8), base D = bit 23.
    void ldr_d(int dd, int xn, int off)  { emit(0xFD400000 | (off << 7) | (xn << 5) | dd); }
    void str_d(int dd, int xn, int off)  { emit(0xFD000000 | (off << 7) | (xn << 5) | dd); }
    void fadd_d(int dd, int dn, int dm)  { emit(0x1E602800 | (dm << 16) | (dn << 5) | dd); }
    void fsub_d(int dd, int dn, int dm)  { emit(0x1E603800 | (dm << 16) | (dn << 5) | dd); }
    void fmul_d(int dd, int dn, int dm)  { emit(0x1E600800 | (dm << 16) | (dn << 5) | dd); }
    void fneg_d(int dd, int dn)          { emit(0x1E614000 | (dn << 5) | dd); }

    void resolve() {
        for (auto &fx : fixups) {
            auto it = label_pos.find(fx.label);
            if (it == label_pos.end() || it->second == 0xFFFFFFFF) {
                fprintf(stderr, "jit: label non résolu: %s\n", fx.label.c_str());
                continue;
            }
            uint32_t target = it->second;
            // Offset AArch64 mesuré depuis l'adresse de l'instruction de saut elle-même
            uint32_t from = fx.insn_pos;
            int64_t offset_words = (int64_t)target * 4 - (int64_t)from * 4;
            offset_words /= 4;

            uint32_t insn = 0;
            if (fx.cc != -1) {              // CBZ (-2) ou B.cond (cc >= 0)
                if (offset_words < -0x40000 || offset_words > 0x3FFFF) {
                    fprintf(stderr, "jit: saut conditionnel trop loin: %s\n", fx.label.c_str());
                    continue;
                }
                uint32_t imm19 = (uint32_t)(offset_words & 0x7FFFF);
                if (fx.cc == -2) insn = 0xB4000000 | (imm19 << 5) | fx.rt;      // CBZ
                else             insn = 0x54000000 | (imm19 << 5) | (uint32_t)(fx.cc & 15);
            } else if (fx.is_cond) {
                if (offset_words < -0x40000 || offset_words > 0x3FFFF) {
                    fprintf(stderr, "jit: saut CBNZ trop loin: %s\n", fx.label.c_str());
                    continue;
                }
                uint32_t imm19 = (uint32_t)(offset_words & 0x7FFFF);
                insn = 0xB5000000 | (imm19 << 5) | fx.rt;   // CBNZ 64-bit
            } else {
                if (offset_words < -0x2000000 || offset_words > 0x1FFFFFF) {
                    fprintf(stderr, "jit: saut B trop loin: %s\n", fx.label.c_str());
                    continue;
                }
                uint32_t imm26 = (uint32_t)(offset_words & 0x3FFFFFF);
                insn = 0x14000000 | imm26;
            }
            uint8_t *ptr = buf.data_ptr() + fx.insn_pos * 4;
            ptr[0] = (insn >> 0)  & 0xFF;
            ptr[1] = (insn >> 8)  & 0xFF;
            ptr[2] = (insn >> 16) & 0xFF;
            ptr[3] = (insn >> 24) & 0xFF;
        }
    }
};

// =============================================================================
// Appel de fonctions JIT (récursion via trampoline C++)
// =============================================================================

struct JitCallTarget {
    JitEntryFn fn = nullptr;
    int nslots = 0;
    std::vector<int> param_slots;   // slot de chaque paramètre (-1 = non lié)
    std::vector<int> param_tys;     // RirType en int
    std::vector<int> const_slots;   // slots des constantes : durée de vie module
};
static std::vector<JitCallTarget> g_call_targets;
static std::map<std::string,int> g_call_names;
// Constantes JIT partagées : une Valeur par constante de module, créée à la
// compilation, jamais libérée (même durée de vie que le code émis).
std::vector<Valeur*> g_shared_consts;   // vu par jit_x64.cpp (déclaré dans jit.h)

// x0=ctx, x1=index cible, x2=table d'arguments Valeur*, x3=nargs. Retour x0=résultat.
// Le cadre du callee est créé/libéré ici ; le résultat (objet d'un slot callee)
// devient la propriété de l'appelant (qui le copie puis le libère).
Valeur* __attribute__((noinline)) jit_call(JitContext* ctx, int target, Valeur** args, int nargs) {
    if (target < 0 || target >= (int)g_call_targets.size() || !g_call_targets[target].fn) {
        fprintf(stderr, "Rose : erreur — fonction inconnue [Ligne 0]\n");
        std::fflush(stdout); std::exit(1);
    }
    JitCallTarget &T = g_call_targets[target];
    size_t fsz = T.nslots > 0 ? (size_t)T.nslots : 1;
    auto &bucket = g_frame_pool[T.nslots];
    Valeur** frame;
    if (!bucket.empty()) { frame = bucket.back(); bucket.pop_back(); }
    else frame = new Valeur*[fsz];
    for (size_t i = 0; i < fsz; i++) frame[i] = nullptr;
    for (size_t i = 0; i < T.param_slots.size() && i < (size_t)nargs; i++) {
        int s = T.param_slots[i];
        if (s < 0 || s >= T.nslots || !args[i]) continue;
        frame[s] = (T.param_tys[i] == (int)RirB) ? jit_b_copy_into(nullptr, args[i])
                 : (T.param_tys[i] == (int)RirF) ? jit_f_copy_into(nullptr, args[i])
                                                 : jit_z_copy_into(nullptr, args[i]);
    }
    Valeur* res = reinterpret_cast<Valeur*>(T.fn(ctx, frame, nullptr, 0));
    for (int i = 0; i < T.nslots; i++) {
        if (!frame[i] || frame[i] == res) continue;
        bool cst = false;
        for (int cs : T.const_slots) if (cs == i) { cst = true; break; }
        if (!cst) valeur_pool_push(frame[i]);
    }
    if (bucket.size() < FRAME_POOL_MAX) bucket.push_back(frame);
    else delete[] frame;
    return res;
}

void __attribute__((noinline)) jit_free_valeur(Valeur* v) {
    if (g_shared_const_lookup.count(v)) return;   // constante partagée : jamais libérée
    valeur_pool_push(v);
}

// =============================================================================
// JitContext
// =============================================================================
JitContext::JitContext() {}
JitContext::~JitContext() {
    for (auto &kv : globaux) delete kv.second;
}

// =============================================================================
// JitCompiler
// =============================================================================

JitCompiler::JitCompiler() {}
JitCompiler::~JitCompiler() { reset(); }

void JitCompiler::reset() {
    // m_entries.clear();  // non utilisé
    // for (auto *buf : m_buffers) delete buf;  // désactivé pour debug
    m_buffers.clear();
    m_helpers.clear();
}

int JitCompiler::add_helper(void* fn) {
    m_helpers.push_back(fn);
    return (int)(m_helpers.size() - 1);
}

// ── Tables de helpers (index dans JitContext::helpers) ─────────────────
//  0 jit_val_z      1 jit_val_b      2 jit_add_z    3 jit_sub_z
//  4 jit_mul_z      5 jit_cmp_lt_z   6 jit_test_b   7 jit_afficher_z
//  8 jit_val_z_txt  9 jit_div_z     10 jit_rem_z   11 jit_cmp_eq_z
// 12 jit_cmp_ne_z  13 jit_cmp_le_z 14 jit_cmp_gt_z 15 jit_cmp_ge_z
// 16 jit_afficher_b 17 jit_afficher_s 18 jit_and_b  19 jit_or_b
// 20 jit_not_b     21 jit_neg_z     22 jit_add_z_into  23 jit_sub_z_into
// 24 jit_mul_z_into 25 jit_z_copy_into 26 jit_b_copy_into 27 jit_div_z_into
// 28 jit_rem_z_into 29 jit_neg_z_into  30 jit_bool_into
// (helpers 22-30 : réutilisation du slot destination, zéro allocation/itération)
// 31 jit_call 32 jit_free_valeur 33 cmp.eq.b 34 cmp.ne.b
// 35 jit_f_copy_into 36 jit_add_f_into 37 jit_sub_f_into 38 jit_mul_f_into
// 39 jit_div_f_into 40 jit_neg_f_into 41 jit_afficher_f 42-47 cmp lt/le/gt/ge/eq/ne.f
// 48 jit_pow_f_into 49 jit_divf_f_into   (flottants : M2.12)

static int z_binop_helper(const std::string &op) {
    if (op == "add.z")   return 22;
    if (op == "sub.z")   return 23;
    if (op == "mul.z")   return 24;
    if (op == "div.z")   return 27;
    if (op == "rem.z.z") return 28;
    return -1;
}

static bool is_temp_name(const std::string &nm) {
    if (nm.size() < 3 || nm[0] != '%' || nm[1] != 't') return false;
    for (size_t i = 2; i < nm.size(); i++)
        if (!isdigit((unsigned char)nm[i])) return false;
    return true;
}

// Texte d'une constante inline « @c<texte> », dupliqué et gardé vivant
// pour la durée de vie du programme (le code JIT émis référence ce pointeur).
static const char* dup_const_text(const std::string &name) {
    static std::vector<char*> pool;   // pointeurs stables (jamais retirés)
    if (name.rfind("@c", 0) != 0) return nullptr;
    char* s = strdup(name.substr(2).c_str());
    pool.push_back(s);
    return s;
}

static bool parse_i64(const std::string &txt, int64_t *out) {
    if (txt.empty()) return false;
    errno = 0;
    char *end = nullptr;
    long long v = strtoll(txt.c_str(), &end, 10);
    if (errno == ERANGE || !end || *end != '\0') return false;
    *out = (int64_t)v;
    return true;
}

static const int FRAME_SLOTS_MAX = 256;   // doit cadrer avec rose.cpp (frame_size)

JitEntryFn JitCompiler::compile_function(RirFonction* fn) {
#if defined(__x86_64__)
    return compile_function_x64(fn);   // backend x86-64 (jit_x64.cpp)
#elif !defined(__aarch64__)
    // Pas d'émetteur pour cette architecture : le code émis serait illégal
    // ailleurs → porte module = repli interpréteur complet.
    (void)fn;
    JITLOG("jit: architecture non prise en charge — repli interpréteur\n");
    return nullptr;
#endif
    // ═══ Pré-analyse : ops supportées, noms utilisés, labels, constantes ═══
    int max_t = -1;
    std::set<std::string> variables;                          // %x nommés
    std::vector<std::pair<std::string, RirType>> constantes;  // @c… (dédupliqués)
    std::set<std::string> constantes_vues;
    std::set<std::string> labels;
    for (auto &b : fn->blocs) labels.insert(b.nom);

    bool unsupported = false;
    // Nombre de lectures statiques de chaque temp %tN (fusion M2.8 + sûreté)
    std::map<int,int> temp_reads;
    for (auto &b : fn->blocs) {
        for (auto &instr : b.instrs) {
            const std::string &op = instr.op;
            if (op != "cst.z" && op != "cst.b" && op != "cst.f" &&
                op != "add.z" && op != "sub.z" && op != "mul.z" &&
                op != "div.z" && op != "rem.z.z" && op != "neg.z" &&
                op != "add.f" && op != "sub.f" && op != "mul.f" &&
                op != "div.f" && op != "divf.f" && op != "pow.f" && op != "neg.f" &&
                op.rfind("cmp.", 0) != 0 &&          // cmp.<rel>.<ty> : vérifié à l'émission
                op != "and.b" && op != "or.b" && op != "not.b" &&
                op != "affect" && op != "afficher" && op != "call" &&
                op != "ret.z" && op != "ret" && op != "ret.u" && op != "br" && op != "br.cond") {
                JITLOG("jit: instruction non supportée: %s\n", op.c_str());
                unsupported = true; break;
            }
            if (op == "call") {
                // appel de fonction utilisateur $nom déjà compilée dans le module
                if (instr.args.empty() || instr.args[0].kind != RirOp::Temp ||
                    instr.args[0].name.rfind("$", 0) != 0 ||
                    !m_call_idx.count(instr.args[0].name)) {
                    JITLOG("jit: appel non supporté\n");
                    unsupported = true; break;
                }
                if (instr.dest.empty() || (instr.dest_ty != RirZ && instr.dest_ty != RirB &&
                                           instr.dest_ty != RirF)) {
                    JITLOG("jit: call avec dest_ty=%d\n", (int)instr.dest_ty);
                    unsupported = true; break;
                }
            }
            auto consider = [&](const std::string &nm) {
                if (nm.empty() || nm[0] != '%') return;
                if (is_temp_name(nm)) { int n = atoi(nm.c_str()+2); if (n > max_t) max_t = n; }
                else variables.insert(nm);
            };
            consider(instr.dest);
            for (auto &a : instr.args)
                if (a.kind == RirOp::Temp && is_temp_name(a.name))
                    temp_reads[atoi(a.name.c_str() + 2)]++;
            if (op == "affect") {
                if (instr.message.empty()) { unsupported = true; break; }
                consider("%" + instr.message);
            }
            if (op == "br" && !labels.count(instr.label_cible)) { unsupported = true; break; }
            if (op == "br.cond" &&
                (!labels.count(instr.label_cible) || !labels.count(instr.label_cible2))) {
                unsupported = true; break;
            }
            for (auto &a : instr.args) {
                if (a.kind == RirOp::Temp) consider(a.name);
                else if (a.kind == RirOp::Const) {
                    if (a.name.rfind("@c", 0) != 0 ||
                        (a.ty != RirZ && a.ty != RirB && a.ty != RirU && a.ty != RirF)) {
                        JITLOG("jit: constante non supportée: %s\n", a.name.c_str());
                        unsupported = true; break;
                    }
                    if (!constantes_vues.count(a.name)) {
                        constantes_vues.insert(a.name);
                        constantes.push_back({a.name, a.ty});
                    }
                }
            }
            if (unsupported) break;
        }
        if (unsupported) break;
    }
    if (unsupported) return nullptr;

    // ═══ Attribution des slots ═══
    // slots 0..max_t : temporaires SSA %tN ; puis variables nommées ;
    // puis constantes (Valeur* matérialisées UNE fois au prologue).
    std::map<std::string,int> slot_vars;
    int next = max_t + 1;
    for (auto &nm : variables) slot_vars[nm] = next++;
    std::map<std::string,int> slot_const;
    for (auto &c : constantes) slot_const[c.first] = next++;
    if (next > FRAME_SLOTS_MAX) {
        JITLOG("jit: %d slots > %d, repli interpréteur\n", next, FRAME_SLOTS_MAX);
        return nullptr;
    }
    std::map<int,RirType> slot_ty;   // type connu d'un slot (pour afficher)
    auto slot_of = [&](const std::string &nm, int &out) -> bool {
        if (is_temp_name(nm)) { out = atoi(nm.c_str()+2); return true; }
        auto it = slot_vars.find(nm);
        if (it == slot_vars.end()) return false;
        out = it->second; return true;
    };
    // Alias SSA : un temp écrit uniquement par « cst.z/cst.b %t @c » est
    // chargé directement depuis le slot de la constante (simple assignation,
    // opérandes lus en lecture seule — la copie vers un objet propre devient
    // inutile). Les rares chemins qui libéreraient l'objet (ret d'appel) sont
    // bloqués par g_shared_const_lookup dans jit_free_valeur.
    std::map<int,int> const_alias;   // slot temp -> slot constante
    auto load_operand = [&](A64Asm &ax, const RirOp &a, int rd) -> bool {
        int s;
        if (a.kind == RirOp::Temp) {
            if (!slot_of(a.name, s)) return false;
            auto al = const_alias.find(s);
            ax.ldr(rd, X20, al != const_alias.end() ? al->second : s);
            return true;
        }
        if (a.kind == RirOp::Const) {
            auto it = slot_const.find(a.name);
            if (it == slot_const.end()) return false;
            ax.ldr(rd, X20, it->second);
            return true;
        }
        return false;
    };
    // Les paramètres portent le type de la signature
    for (auto &p : fn->params) {
        int s;
        if (slot_of("%" + p.first, s)) slot_ty[s] = p.second;
    }

    A64Buf* buf = new A64Buf();
    A64Asm asm_(*buf);

    // ═══ Prologue ═══
    // x19=ctx, x20=frame. Ces registres sont callee-saved dans l'ABI AArch64 :
    // ils doivent être préservés pour les fonctions C++ (jit_call) qui nous appellent.
    asm_.sub_sp(32);
    asm_.emit(0xA80053F3);       // stp x19, x20, [sp, #0]
    asm_.emit(0xA8017BFD);       // stp x29, x30, [sp, #16]
    asm_.mov_reg(X19, X0);  // x19 = contexte
    asm_.mov_reg(X20, X1);  // x20 = frame (Valeur**)

    // Matérialisation des constantes : une seule Valeur par constante du
    // module (créée à la compilation, jamais libérée — même durée de vie
    // que le code émis). Le prologue ne fait que poser le pointeur partagé.
    for (auto &c : constantes) {
        std::string txt = c.first.substr(2);
        int sl = slot_const[c.first];
        int64_t v;
        Valeur *shared;
        if (c.second == RirB || txt == "vrai" || txt == "faux") {
            shared = jit_val_b(txt == "vrai");
        } else if (c.second == RirF) {
            // constante flottante : texte canonique de texte_flottant, relu
            // exactement (round-trip %g) — jamais la voie entière parse_i64.
            shared = jit_val_f(strtod(txt.c_str(), nullptr));
        } else if (parse_i64(txt, &v)) {
            shared = jit_val_z(v);
        } else {
            shared = jit_val_z_txt(dup_const_text(c.first));
        }
        g_shared_consts.push_back(shared);
        asm_.mov_imm(X0, (uint64_t)(uintptr_t)shared);
        asm_.str(X0, X20, sl);
    }

    // ═══ Corps : chaque bloc ═══
    for (auto &bloc : fn->blocs) {
        asm_.bind(bloc.nom);

        // Fusion producteur→affect (M2.8) : quand une temp %t n'est lue QUE
        // par l'« affect %x %t » qui écrit son producteur immédiatement, le
        // producteur écrit directement dans le slot de %x et l'affect est
        // supprimé (zéro copie). Sûr : %t mono-lecteur (temp_reads), aucune
        // instruction entre les deux, et tous les helpers producteurs sont
        // « _into » (lisent leurs opérandes avant d'écrire dst — aliasing
        // dst==opérande toléré par GMP pour add/sub/mul/div/rem/neg).
        std::set<size_t> skip_affect;
        auto fused_dst = [&](const RirInstr &instr, size_t ii, int &x_out) -> bool {
            if (ii + 1 >= bloc.instrs.size()) return false;
            const RirInstr &nx = bloc.instrs[ii + 1];
            if (nx.op != "affect" || nx.args.empty()) return false;
            if (nx.args[0].kind != RirOp::Temp || nx.args[0].name != instr.dest) return false;
            if (!is_temp_name(instr.dest)) return false;
            auto r = temp_reads.find(atoi(instr.dest.c_str() + 2));
            if (r == temp_reads.end() || r->second != 1) return false;
            int xs;
            if (!slot_of("%" + nx.message, xs)) return false;
            if (const_alias.count(xs)) return false;
            skip_affect.insert(ii + 1);
            x_out = xs;
            return true;
        };

        for (size_t ii = 0; ii < bloc.instrs.size(); ii++) {
            if (skip_affect.count(ii)) continue;
            const RirInstr &instr = bloc.instrs[ii];
            const std::string &op = instr.op;
            int d = -1;

            if (op == "cst.z" || op == "cst.b" || op == "cst.f") {
                // Alias SSA : aucune instruction émise, le temp pointe la
                // constante partagée (voir const_alias + jit_free_valeur).
                if (!slot_of(instr.dest, d)) { unsupported = true; break; }
                auto it = (instr.args[0].kind == RirOp::Const)
                              ? slot_const.find(instr.args[0].name) : slot_const.end();
                if (it != slot_const.end()) {
                    const_alias[d] = it->second;
                } else {
                    if (!load_operand(asm_, instr.args[0], X1)) { unsupported = true; break; }
                    asm_.ldr(X0, X20, d);
                    asm_.blr_helper(op == "cst.z" ? 25 : 26);   // z/b_copy_into
                    asm_.str(X0, X20, d);
                }
                slot_ty[d] = instr.dest_ty;
                continue;
            }
            int h;
            if ((h = z_binop_helper(op)) >= 0) {
                if (!slot_of(instr.dest, d)) { unsupported = true; break; }
                { int fd; if (fused_dst(instr, ii, fd)) d = fd; }   // fusion M2.8
                // ── inline machine (M2.7) : add/sub/mul sur entiers positifs
                // d'un seul membre ; toute autre situation retombe sur le
                // même appel helper qu'avant (sémantique identique). ──
                if (h == 22 || h == 23 || h == 24) {
                    static int inl_ctr = 0;
                    int ic = inl_ctr++;
                    std::string slow = "__inl_s" + std::to_string(ic);
                    std::string end  = "__inl_e" + std::to_string(ic);
                    asm_.ldr(X0, X20, d);
                    if (!load_operand(asm_, instr.args[0], X1) ||
                        !load_operand(asm_, instr.args[1], X2)) { unsupported = true; break; }
                    asm_.cbz_label(X0, slow);                              // dst à créer
                    asm_.ldr_w(X8, X1, 4); asm_.cmp_w_imm(X8, 1);
                    asm_.bcond_label(1, slow);                             // b.ne : taille != +1
                    asm_.ldr(X3, X1, 1);                                   // membre de a
                    asm_.cmp_x_imm0(X3); asm_.bcond_label(4, slow);        // b.mi : >= 2^63
                    asm_.ldr_w(X8, X2, 4); asm_.cmp_w_imm(X8, 1);
                    asm_.bcond_label(1, slow);
                    asm_.ldr(X4, X2, 1);
                    asm_.cmp_x_imm0(X4); asm_.bcond_label(4, slow);
                    if (h == 22)      asm_.adds_x(X5, X3, X4);
                    else if (h == 23) asm_.subs_x(X5, X3, X4);
                    else              asm_.mul_x(X5, X3, X4);
                    if (h != 24) asm_.bcond_label(6, slow);               // b.vs : débordement
                    else { asm_.smulh_x(X6, X3, X4);
                           asm_.cmp_x_asr63(X6, X5); asm_.bcond_label(1, slow); }
                    asm_.ldr_w(X8, X0, 0); asm_.cmp_w_imm(X8, 1);
                    asm_.bcond_label(11, slow);                            // b.lt : alloc < 1
                    asm_.asr_x(X6, X5, 63);                               // masque de signe
                    asm_.eor_x(X4, X5, X6); asm_.subs_x(X4, X4, X6);      // |r|
                    asm_.orr_x_imm1(X8, X6);                              // taille = signe|1
                    asm_.cmp_x_imm0(X4); asm_.csel_x(X8, 31, X8, 0);      // |r|==0 → 0
                    asm_.ldr(X7, X0, 1); asm_.str(X4, X7, 0);             // membre
                    asm_.str_w(X8, X0, 4);                                // taille
                    asm_.mov_imm(X8, (uint64_t)T_ENTIER); asm_.str_w(X8, X0, 0);
                    asm_.b_label(end);
                    asm_.bind(slow);
                    asm_.blr_helper(h);   // X0/X1/X2 intacts sur tous les chemins lents
                    asm_.str(X0, X20, d);
                    asm_.bind(end);
                    slot_ty[d] = RirZ;
                    continue;
                }
                asm_.ldr(X0, X20, d);                      // dst (réutilisé)
                if (!load_operand(asm_, instr.args[0], X1) ||
                    !load_operand(asm_, instr.args[1], X2)) { unsupported = true; break; }
                asm_.blr_helper(h);
                asm_.str(X0, X20, d);
                slot_ty[d] = RirZ;
                continue;
            }
            if (op == "neg.z") {
                if (!slot_of(instr.dest, d)) { unsupported = true; break; }
                { int fd; if (fused_dst(instr, ii, fd)) d = fd; }   // fusion M2.8
                asm_.ldr(X0, X20, d);
                if (!load_operand(asm_, instr.args[0], X1)) { unsupported = true; break; }
                asm_.blr_helper(29);                       // jit_neg_z_into
                asm_.str(X0, X20, d);
                slot_ty[d] = RirZ;
                continue;
            }
            // ── Flottants (M2.12) ────────────────────────────────────────
            // add/sub/mul : chemin inline (registres D, zéro allocation après
            // le premier passage) ; le helper C++ est le chemin lent exact
            // (dst null → allocation ; opérande non flottant → vers_d).
            if (op == "add.f" || op == "sub.f" || op == "mul.f") {
                if (!slot_of(instr.dest, d)) { unsupported = true; break; }
                { int fd; if (fused_dst(instr, ii, fd)) d = fd; }   // fusion M2.8
                static int fp_ctr = 0;
                int ic = fp_ctr++;
                std::string slow = "__fp_s" + std::to_string(ic);
                std::string end  = "__fp_e" + std::to_string(ic);
                asm_.ldr(X0, X20, d);
                if (!load_operand(asm_, instr.args[0], X1) ||
                    !load_operand(asm_, instr.args[1], X2)) { unsupported = true; break; }
                asm_.cbz_label(X0, slow);                              // dst à créer
                asm_.ldr_w(X8, X1, 0); asm_.cmp_w_imm(X8, T_FLOTTANT);
                asm_.bcond_label(1, slow);                             // b.ne : a non flottant
                asm_.ldr_w(X8, X2, 0); asm_.cmp_w_imm(X8, T_FLOTTANT);
                asm_.bcond_label(1, slow);                             // b.ne : b non flottant
                asm_.ldr_d(0, X1, (int)offsetof(Valeur, f));
                asm_.ldr_d(1, X2, (int)offsetof(Valeur, f));
                if (op == "add.f") asm_.fadd_d(0, 0, 1);
                else if (op == "sub.f") asm_.fsub_d(0, 0, 1);
                else asm_.fmul_d(0, 0, 1);
                asm_.str_d(0, X0, (int)offsetof(Valeur, f));
                asm_.mov_imm(X8, (uint64_t)T_FLOTTANT); asm_.str_w(X8, X0, 0);
                asm_.b_label(end);
                asm_.bind(slow);
                asm_.blr_helper(op == "add.f" ? 36 : op == "sub.f" ? 37 : 38);
                asm_.str(X0, X20, d);                                  // dst (ré)alloué
                asm_.bind(end);
                slot_ty[d] = RirF;
                continue;
            }
            if (op == "div.f" || op == "divf.f" || op == "pow.f" || op == "neg.f") {
                if (!slot_of(instr.dest, d)) { unsupported = true; break; }
                { int fd; if (fused_dst(instr, ii, fd)) d = fd; }   // fusion M2.8
                asm_.ldr(X0, X20, d);
                if (op == "neg.f") {
                    if (!load_operand(asm_, instr.args[0], X1)) { unsupported = true; break; }
                    asm_.blr_helper(40);                   // jit_neg_f_into
                } else {
                    if (!load_operand(asm_, instr.args[0], X1) ||
                        !load_operand(asm_, instr.args[1], X2)) { unsupported = true; break; }
                    asm_.blr_helper(op == "div.f" ? 39 : op == "divf.f" ? 49 : 48);
                }
                asm_.str(X0, X20, d);
                slot_ty[d] = RirF;
                continue;
            }
            if (op.rfind("cmp.", 0) == 0) {
                if (!slot_of(instr.dest, d)) { unsupported = true; break; }
                { int fd; if (fused_dst(instr, ii, fd)) d = fd; }   // fusion M2.8
                std::string core = op.substr(4);
                size_t dot = core.find('.');
                std::string rel = (dot == std::string::npos) ? core : core.substr(0, dot);
                auto operand_ty = [&](const RirOp &a, RirType &out) -> bool {
                    if (a.kind == RirOp::Const) { out = a.ty; return true; }
                    int s;
                    if (!slot_of(a.name, s)) return false;
                    auto it = slot_ty.find(s);
                    if (it == slot_ty.end()) return false;
                    out = it->second; return true;
                };
                RirType t0 = RirU, t1 = RirU;
                if (!operand_ty(instr.args[0], t0) || !operand_ty(instr.args[1], t1)) {
                    JITLOG("jit: cmp: type d'opérande inconnu (%s)\n", op.c_str());
                    unsupported = true; break;
                }
                int hc = -1;
                if (t0 == RirZ && t1 == RirZ) {
                    if (rel == "lt") hc = 5;
                    else if (rel == "eq") hc = 11;
                    else if (rel == "ne") hc = 12;
                    else if (rel == "le") hc = 13;
                    else if (rel == "gt") hc = 14;
                    else if (rel == "ge") hc = 15;
                } else if (t0 == RirB && t1 == RirB) {
                    if (rel == "eq") hc = 33;
                    else if (rel == "ne") hc = 34;
                } else if (t0 == RirF || t1 == RirF) {
                    // au moins un flottant : conversion tolérante des deux
                    // côtés (miroir de cmp_vals), même chemin lent que z.
                    if (rel == "lt") hc = 42;
                    else if (rel == "eq") hc = 46;
                    else if (rel == "ne") hc = 47;
                    else if (rel == "le") hc = 43;
                    else if (rel == "gt") hc = 44;
                    else if (rel == "ge") hc = 45;
                }
                if (hc < 0) {
                    JITLOG("jit: cmp non supporté: %s (ty %d,%d)\n", op.c_str(), (int)t0, (int)t1);
                    unsupported = true; break;
                }
                // ── inline machine (M2.7) : comparaison de deux positifs
                // d'un membre ; le booléen est écrit sur place dans le slot
                // destination (mêmes effets que cmp-h + jit_bool_into). ──
                int cc = -1;
                if (t0 == RirZ && t1 == RirZ) {
                    if (rel == "lt") cc = 3; else if (rel == "le") cc = 9;
                    else if (rel == "gt") cc = 8; else if (rel == "ge") cc = 2;
                    else if (rel == "eq") cc = 0; else if (rel == "ne") cc = 1;
                }
                if (cc >= 0) {
                    static int cmp_ctr = 0;
                    int ic = cmp_ctr++;
                    std::string slow = "__cmp_s" + std::to_string(ic);
                    std::string end  = "__cmp_e" + std::to_string(ic);
                    asm_.ldr(X0, X20, d);
                    if (!load_operand(asm_, instr.args[0], X1) ||
                        !load_operand(asm_, instr.args[1], X2)) { unsupported = true; break; }
                    asm_.cbz_label(X0, slow);
                    asm_.ldr_w(X8, X1, 4); asm_.cmp_w_imm(X8, 1); asm_.bcond_label(1, slow);
                    asm_.ldr(X3, X1, 1); asm_.cmp_x_imm0(X3); asm_.bcond_label(4, slow);
                    asm_.ldr_w(X8, X2, 4); asm_.cmp_w_imm(X8, 1); asm_.bcond_label(1, slow);
                    asm_.ldr(X4, X2, 1); asm_.cmp_x_imm0(X4); asm_.bcond_label(4, slow);
                    asm_.cmp_x(X3, X4); asm_.cset_x(X5, cc);
                    asm_.str_b(X5, X0, (int)offsetof(Valeur, b));
                    asm_.mov_imm(X8, (uint64_t)T_BOOLEEN); asm_.str_w(X8, X0, 0);
                    asm_.b_label(end);
                    asm_.bind(slow);
                    if (!load_operand(asm_, instr.args[0], X0) ||
                        !load_operand(asm_, instr.args[1], X1)) { unsupported = true; break; }
                    asm_.blr_helper(hc);
                    asm_.mov_reg(X1, X0);
                    asm_.ldr(X0, X20, d);
                    asm_.blr_helper(30);
                    asm_.str(X0, X20, d);
                    asm_.bind(end);
                    slot_ty[d] = RirB;
                    continue;
                }
                if (!load_operand(asm_, instr.args[0], X0) ||
                    !load_operand(asm_, instr.args[1], X1)) { unsupported = true; break; }
                asm_.blr_helper(hc);                       // → int dans x0
                asm_.mov_reg(X1, X0);                      // x1 = cond
                asm_.ldr(X0, X20, d);
                asm_.blr_helper(30);                       // jit_bool_into
                asm_.str(X0, X20, d);
                slot_ty[d] = RirB;
                continue;
            }
            if (op == "and.b" || op == "or.b" || op == "not.b") {
                if (!slot_of(instr.dest, d)) { unsupported = true; break; }
                { int fd; if (fused_dst(instr, ii, fd)) d = fd; }   // fusion M2.8
                if (op == "not.b") {
                    if (!load_operand(asm_, instr.args[0], X0)) { unsupported = true; break; }
                    asm_.blr_helper(20);                   // jit_not_b
                } else {
                    if (!load_operand(asm_, instr.args[0], X0) ||
                        !load_operand(asm_, instr.args[1], X1)) { unsupported = true; break; }
                    asm_.blr_helper(op == "and.b" ? 18 : 19);
                }
                asm_.mov_reg(X1, X0);                      // x1 = Valeur* temporaire
                asm_.ldr(X0, X20, d);
                asm_.blr_helper(26);                       // jit_b_copy_into
                asm_.str(X0, X20, d);
                slot_ty[d] = RirB;
                continue;
            }
            if (op == "affect") {
                if (!slot_of("%" + instr.message, d)) { unsupported = true; break; }
                if (!load_operand(asm_, instr.args[0], X1)) { unsupported = true; break; }
                RirType sty = RirU;
                int src;
                if (instr.args[0].kind == RirOp::Temp &&
                    slot_of(instr.args[0].name, src) && slot_ty.count(src))
                    sty = slot_ty[src];
                asm_.ldr(X0, X20, d);
                if (sty == RirZ)      asm_.blr_helper(25);  // jit_z_copy_into
                else if (sty == RirB) asm_.blr_helper(26);  // jit_b_copy_into
                else if (sty == RirF) asm_.blr_helper(35);  // jit_f_copy_into
                else { JITLOG("jit: affect: type inconnu pour %s\n", instr.args[0].name.c_str());
                       unsupported = true; break; }
                asm_.str(X0, X20, d);
                slot_ty[d] = sty;
                continue;
            }
            if (op == "afficher") {
                int s;
                if (!slot_of(instr.args[0].name, s)) { unsupported = true; break; }
                auto al = const_alias.find(s);
                RirType ty = slot_ty.count(s) ? slot_ty[s] : RirU;
                asm_.ldr(X0, X20, al != const_alias.end() ? al->second : s);
                if (ty == RirZ)      asm_.blr_helper(7);   // jit_afficher_z
                else if (ty == RirB) asm_.blr_helper(16);  // jit_afficher_b
                else if (ty == RirS) asm_.blr_helper(17);  // jit_afficher_s
                else if (ty == RirF) asm_.blr_helper(41);  // jit_afficher_f
                else { JITLOG("jit: afficher: type inconnu pour %s\n", instr.args[0].name.c_str());
                       unsupported = true; break; }
                continue;
            }
            if (op == "ret.z" || op == "ret") {
                if (!load_operand(asm_, instr.args[0], X0)) { unsupported = true; break; }
                asm_.emit(0xA8417BFD);              // ldp x29, x30, [sp, #16]
                asm_.emit(0xA84053F3);              // ldp x19, x20, [sp, #0]
                asm_.add_sp(32);
                asm_.ret();
                continue;
            }
            if (op == "ret.u") {
                asm_.mov_imm(X0, 0);
                asm_.emit(0xA8417BFD);              // ldp x29, x30, [sp, #16]
                asm_.emit(0xA84053F3);              // ldp x19, x20, [sp, #0]
                asm_.add_sp(32);
                asm_.ret();
                continue;
            }
            if (op == "br") {
                asm_.b_label(instr.label_cible);
                continue;
            }
            if (op == "br.cond") {
                if (!load_operand(asm_, instr.args[0], X0)) { unsupported = true; break; }
                // jit_test_b lisait v->b sans condition : lecture directe du
                // champ (ldrb = exactement la même valeur que l'appel helper).
                asm_.emit(0x39400000 | (offsetof(Valeur, b) << 10));   // ldrb w0,[x0]
                asm_.cbnz_label(X0, instr.label_cible);
                asm_.b_label(instr.label_cible2);
                continue;
            }
            if (op == "call") {
                int d;
                if (!slot_of(instr.dest, d)) { unsupported = true; break; }
                { int fd; if (fused_dst(instr, ii, fd)) d = fd; }   // fusion M2.8
                int na = (int)instr.args.size() - 1;
                int bytes = na * 8;
                if (bytes < 16) bytes = 16; else bytes = (bytes + 15) & ~15;
                if (bytes > 4080) { unsupported = true; break; }
                asm_.sub_sp(bytes);
                bool ok = true;
                for (int i = 0; i < na; i++) {
                    if (!load_operand(asm_, instr.args[i+1], X0)) { ok = false; break; }
                    asm_.str(X0, SP, i);              // pile : args[i] à sp+i*8
                }
                if (!ok) { unsupported = true; break; }
                asm_.mov_reg(X0, X19);                // ctx
                asm_.mov_imm(X1, (uint64_t)m_call_idx[instr.args[0].name]);
                asm_.mov_from_sp(X2);                 // table d'arguments
                asm_.mov_imm(X3, (uint64_t)na);
                asm_.blr_helper(31);                  // jit_call → x0 = résultat
                asm_.str(X0, SP, 0);                  // sauvegarde (zone allouée)
                asm_.mov_reg(X1, X0);                 // x1 = src (résultat callee)
                asm_.ldr(X0, X20, d);                 // x0 = dst (slot propre)
                asm_.blr_helper(instr.dest_ty == RirB ? 26 :
                                instr.dest_ty == RirF ? 35 : 25);  // copie b/f/z
                asm_.str(X0, X20, d);
                asm_.ldr(X0, SP, 0);
                asm_.blr_helper(32);                  // jit_free_valeur (copie callee)
                asm_.add_sp(bytes);
                slot_ty[d] = instr.dest_ty;
                continue;
            }
            JITLOG("jit: instruction non supportée: %s\n", op.c_str());
            unsupported = true;
            break;
        }
        if (unsupported) break;
    }

    if (unsupported) {
        // Ne pas enregistrer cette fonction : repli sur l'interpréteur
        delete buf;
        return nullptr;
    }

    asm_.resolve();
    buf->align16();
    buf->make_exec();

    // Métadonnées pour la table d'appels (paramètres, constantes)
    m_nslots[fn->nom] = next;
    auto &ps = m_pslots[fn->nom]; ps.clear();
    auto &pt = m_ptys[fn->nom]; pt.clear();
    for (auto &p : fn->params) {
        int s;
        ps.push_back(slot_of("%" + p.first, s) ? s : -1);
        pt.push_back((int)p.second);
    }
    auto &cs = m_cslots[fn->nom]; cs.clear();
    for (auto &c : constantes) cs.push_back(slot_const[c.first]);

    if (jit_verbose()) {   // dump du code machine pour objdump
        std::string safe;
        for (char ch : fn->nom) if (isalnum((unsigned char)ch)) safe += ch;
        std::string path = "/tmp/jit_dump_" + safe + ".bin";
        FILE *f = fopen(path.c_str(), "wb");
        if (f) { fwrite(buf->data_ptr(), 1, buf->size(), f); fclose(f); }
        JITLOG("jit: dump %s (%zu octets)\n", path.c_str(), buf->size());
    }

    m_buffers.push_back(buf);
    return reinterpret_cast<JitEntryFn>(buf->data_ptr());
}

// ═══ M2.10 : inlining RIR des appels aux fonctions feuilles ═══
// Un callee est inlinable si : aucun call interne (feuille), aucun ret.u,
// chaque bloc se termine par br/br.cond/ret/ret.z, aucune branche interne ne
// vise le bloc d'entrée, et tout ret porte un opérande temp. L'appel
// « %d = call $g %a… » devient : copies des arguments vers des slots frais,
// blocs de g renommés/renumérotés, chaque ret remplacé par « affect %d src ;
// br reprise », la reprise étant le reste du bloc d'origine (scindé). La
// transformation est purement RIR : les deux backend (AArch64, x86-64) la
// partagent sans rien changer à leurs émetteurs.
static void collect_slot_names(RirFonction &fn, std::set<std::string> &temps,
                               std::set<std::string> &vars) {
    for (auto &b : fn.blocs)
        for (auto &in : b.instrs) {
            auto consider = [&](const std::string &nm) {
                if (nm.empty() || nm[0] != '%') return;
                if (is_temp_name(nm)) temps.insert(nm); else vars.insert(nm);
            };
            consider(in.dest);
            if (in.op == "affect" && !in.message.empty()) consider("%" + in.message);
            for (auto &a : in.args) if (a.kind == RirOp::Temp) consider(a.name);
        }
}

static bool callee_inlinable(RirFonction &g) {
    if (g.blocs.empty()) return false;
    for (size_t bi = 0; bi < g.blocs.size(); bi++) {
        RirBloc &b = g.blocs[bi];
        if (b.instrs.empty()) return false;
        for (auto &in : b.instrs) {
            if (in.op == "call" || in.op == "ret.u") return false;
            if ((in.op == "br" || in.op == "br.cond")) {
                if (in.label_cible == g.blocs[0].nom) return false;
                if (in.op == "br.cond" && in.label_cible2 == g.blocs[0].nom) return false;
            }
            if ((in.op == "ret" || in.op == "ret.z") &&
                (in.args.empty() || in.args[0].kind != RirOp::Temp)) return false;
        }
        const std::string &last = b.instrs.back().op;
        if (last != "br" && last != "br.cond" && last != "ret" && last != "ret.z")
            return false;
    }
    return true;
}

// Inline le PREMIER site d'appel à g trouvé dans F (F modifié en place).
static bool inline_one_call_site(RirFonction &F, RirFonction &g, unsigned &ctr) {
    for (size_t bi = 0; bi < F.blocs.size(); bi++) {
        RirBloc &B = F.blocs[bi];
        for (size_t ii = 0; ii < B.instrs.size(); ii++) {
            RirInstr &call = B.instrs[ii];
            if (call.op != "call" || call.args.empty()) continue;
            if (call.args[0].kind != RirOp::Temp || call.args[0].name != g.nom) continue;
            if (call.dest_ty != RirZ && call.dest_ty != RirB) continue;
            if (g.params.size() != call.args.size() - 1) continue;
            bool ops_ok = true;
            for (size_t k = 1; k < call.args.size(); k++) {
                const RirOp &a = call.args[k];
                if (a.kind == RirOp::Temp) continue;
                if (a.kind == RirOp::Const && (a.ty == RirZ || a.ty == RirB)) continue;
                ops_ok = false;
            }
            if (!ops_ok) continue;

            // ── slots frais : params, temps et variables de g ──
            std::set<std::string> gtemps, gvars;
            collect_slot_names(g, gtemps, gvars);
            for (auto &p : g.params) gvars.erase("%" + p.first);
            std::set<std::string> ftemps, fvars;
            collect_slot_names(F, ftemps, fvars);
            int mt = -1;
            for (auto &nm : ftemps) { int n = atoi(nm.c_str() + 2); if (n > mt) mt = n; }
            std::map<std::string,std::string> ren;
            auto fresh = [&]() { return "%t" + std::to_string(++mt); };
            std::vector<std::string> ptemp;
            for (auto &p : g.params) {
                std::string nt = fresh();
                ren["%" + p.first] = nt;
                ptemp.push_back(nt);
            }
            for (auto &nm : gtemps) ren[nm] = fresh();
            for (auto &nm : gvars)  ren[nm] = fresh();
            auto remap = [&](std::string &nm) {
                auto it = ren.find(nm);
                if (it != ren.end()) nm = it->second;
            };

            // ── copies de g (blocs renommés, slots remappés, ret → affect+br) ──
            std::map<std::string,std::string> bl;
            for (size_t k = 0; k < g.blocs.size(); k++)
                bl[g.blocs[k].nom] = "__ic" + std::to_string(ctr) + "_b" + std::to_string(k);
            std::string tail_nom = "__ic" + std::to_string(ctr) + "_tail";
            const std::string &d = call.dest;
            std::string dmsg = d.size() > 1 ? d.substr(1) : d;   // message sans %
            std::vector<RirBloc> gb;
            for (size_t k = 0; k < g.blocs.size(); k++) {
                RirBloc nb; nb.nom = bl[g.blocs[k].nom];
                for (auto &in : g.blocs[k].instrs) {
                    if (in.op == "ret" || in.op == "ret.z") {
                        std::string src = in.args[0].name;
                        remap(src);
                        RirInstr af; af.op = "affect"; af.message = dmsg;
                        af.dest_ty = call.dest_ty;
                        RirOp so; so.kind = RirOp::Temp; so.name = src; so.ty = call.dest_ty;
                        af.args.push_back(so);
                        nb.instrs.push_back(af);
                        RirInstr br; br.op = "br"; br.label_cible = tail_nom;
                        nb.instrs.push_back(br);
                        continue;
                    }
                    RirInstr ni = in;
                    remap(ni.dest);
                    if (ni.op == "affect") {
                        auto it = ren.find("%" + ni.message);
                        if (it != ren.end()) ni.message = it->second.substr(1);
                    }
                    for (auto &a : ni.args)
                        if (a.kind == RirOp::Temp) remap(a.name);
                    auto rl = [&](std::string &lb) {
                        auto it = bl.find(lb);
                        if (it != bl.end()) lb = it->second;
                    };
                    rl(ni.label_cible); rl(ni.label_cible2);
                    nb.instrs.push_back(ni);
                }
                gb.push_back(std::move(nb));
            }

            // ── assemblage : head remplace B, tail inséré juste après,
            //    blocs 1..n de g en fin de liste (ordres de chute gardés) ──
            RirBloc head; head.nom = B.nom;
            for (size_t k = 0; k < ii; k++) head.instrs.push_back(B.instrs[k]);
            for (size_t k = 0; k < g.params.size(); k++) {
                const RirOp &a = call.args[k + 1];
                RirInstr cp;
                if (a.kind == RirOp::Const) {
                    cp.op = (a.ty == RirB) ? "cst.b" : "cst.z";
                    cp.dest = ptemp[k]; cp.dest_ty = a.ty;
                    cp.args.push_back(a);
                } else {
                    cp.op = "affect"; cp.message = ptemp[k].substr(1);
                    cp.dest_ty = call.dest_ty;
                    cp.args.push_back(a);
                }
                head.instrs.push_back(cp);
            }
            for (auto &in : gb[0].instrs) head.instrs.push_back(in);
            RirBloc tail; tail.nom = tail_nom;
            for (size_t k = ii + 1; k < B.instrs.size(); k++)
                tail.instrs.push_back(B.instrs[k]);
            B = std::move(head);
            F.blocs.insert(F.blocs.begin() + bi + 1, std::move(tail));
            for (size_t k = 1; k < gb.size(); k++) F.blocs.push_back(std::move(gb[k]));
            ctr++;
            return true;
        }
    }
    return false;
}

static bool inline_leaf_calls(RirFonction &F,
                              const std::map<std::string,RirFonction*> &leaf,
                              unsigned &ctr) {
    bool any = false;
    for (auto &kv : leaf) {
        if (kv.second == &F) continue;
        int guard = 100;   // les inlinés n'introduisent aucun call : borne large
        while (guard-- > 0 && inline_one_call_site(F, *kv.second, ctr)) any = true;
    }
    return any;
}

bool JitCompiler::compile_module(RirModule* mod) {
    // Les appels sont résolus par index de fonction dans le module.
    m_call_idx.clear();
    for (size_t i = 0; i < mod->fonctions.size(); i++)
        m_call_idx[mod->fonctions[i].nom] = (int)i;

    // M2.10 : feuilles inlinables (aucun appel interne)
    std::map<std::string, RirFonction*> leaf;
    for (auto &f : mod->fonctions)
        if (callee_inlinable(f)) leaf[f.nom] = &f;
    unsigned ictr = 0;

    // Porte module : si UNE fonction est hors support JIT, tout le module
    // repasse à l'interpréteur (sinon les `call` casséraient).
    bool all_ok = true;
    for (auto &fn : mod->fonctions) {
        RirFonction *target = &fn;
        RirFonction inlined;
        if (!leaf.empty()) {
            inlined = fn;
            if (inline_leaf_calls(inlined, leaf, ictr)) target = &inlined;
        }
        JitEntryFn e = compile_function(target);
        if (!e && target != &fn) e = compile_function(&fn);  // inlinage trop gros : original
        if (e) m_entries[fn.nom] = e;
        else all_ok = false;
    }
    return all_ok;
}

JitEntryFn JitCompiler::get_entry(const std::string& nom) const {
    auto it = m_entries.find(nom);
    if (it == m_entries.end())
        return nullptr;
    return it->second;
}

void JitCompiler::run(RirModule* mod) {
    (void)mod;
    fprintf(stderr, "jit: run() non encore implémenté\n");
}

// Construit la table d'appels globaux après compilation d'un module entier.
void jit_build_call_table(JitCompiler& compiler, RirModule* mod) {
    g_call_targets.clear();
    g_call_names.clear();
    for (size_t i = 0; i < mod->fonctions.size(); i++) {
        RirFonction &fn = mod->fonctions[i];
        JitCallTarget T;
        T.fn = compiler.get_entry(fn.nom);
        if (compiler.m_nslots.count(fn.nom)) T.nslots = compiler.m_nslots[fn.nom];
        if (compiler.m_pslots.count(fn.nom)) T.param_slots = compiler.m_pslots[fn.nom];
        if (compiler.m_ptys.count(fn.nom))   T.param_tys = compiler.m_ptys[fn.nom];
        if (compiler.m_cslots.count(fn.nom)) T.const_slots = compiler.m_cslots[fn.nom];
        g_call_targets.push_back(T);
    }
}

// =============================================================================
// Wrapper C pour l'intégration dans rose.cpp (évite les conflits de headers)
// =============================================================================

extern "C" {

// Compile toutes les fonctions d'un module RIR avec le JIT.
//   mod       : pointeur vers le module RIR (même layout que rir::RirModule)
//   helpers   : tableau d'adresses de fonctions helper C++
//   nhelpers  : nombre de helpers
// Retour : pointeur vers le code machine de $principal, ou nullptr si échec.
void* rose_jit_compile(void* mod, void** helpers, int nhelpers) {
    JitCompiler compiler;
    for (int i = 0; i < nhelpers; i++)
        compiler.add_helper(helpers[i]);
    RirModule* rm = reinterpret_cast<RirModule*>(mod);
    if (!compiler.compile_module(rm))
        return nullptr;
    JitEntryFn entry = compiler.get_entry("$principal");
    if (!entry)
        return nullptr;
    jit_build_call_table(compiler, rm);
    return reinterpret_cast<void*>(entry);
}

// Exécute le code JIT de $principal.
//   entry    : pointeur retourné par rose_jit_compile
//   helpers  : tableau d'adresses de fonctions helper C++
//   nhelpers : nombre de helpers
//   frame    : tableau de Valeur* alloué par l'appelant
// Retour : Valeur* résultat (propriété de l'appelant)
Valeur* rose_jit_run(void* entry, void** helpers, int nhelpers, Valeur** frame) {
    JitContext ctx;
    for (int i = 0; i < nhelpers && i < JitContext::MAX_HELPERS; i++)
        ctx.helpers[i] = helpers[i];
    // Helpers internes au JIT (non exportés hors de jit.cpp)
    ctx.helpers[31] = (void*)&jit_call;
    ctx.helpers[32] = (void*)&jit_free_valeur;
    ctx.helpers[33] = (void*)&jit_cmp_eq_b;
    ctx.helpers[34] = (void*)&jit_cmp_ne_b;
    // Flottants (M2.12)
    ctx.helpers[35] = (void*)&jit_f_copy_into;
    ctx.helpers[36] = (void*)&jit_add_f_into;
    ctx.helpers[37] = (void*)&jit_sub_f_into;
    ctx.helpers[38] = (void*)&jit_mul_f_into;
    ctx.helpers[39] = (void*)&jit_div_f_into;
    ctx.helpers[40] = (void*)&jit_neg_f_into;
    ctx.helpers[41] = (void*)&jit_afficher_f;
    ctx.helpers[42] = (void*)&jit_cmp_lt_f;
    ctx.helpers[43] = (void*)&jit_cmp_le_f;
    ctx.helpers[44] = (void*)&jit_cmp_gt_f;
    ctx.helpers[45] = (void*)&jit_cmp_ge_f;
    ctx.helpers[46] = (void*)&jit_cmp_eq_f;
    ctx.helpers[47] = (void*)&jit_cmp_ne_f;
    ctx.helpers[48] = (void*)&jit_pow_f_into;
    ctx.helpers[49] = (void*)&jit_divf_f_into;
    JitEntryFn fn = reinterpret_cast<JitEntryFn>(entry);
    return reinterpret_cast<Valeur*>(fn(&ctx, frame, nullptr, 0));
}

} // extern "C"

