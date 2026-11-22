/* bigint.h — bigint maison, base 10^9, pour le banc de performance.
 * GMP est ABSENT de cette machine (gmp.h introuvable) : ce fichier fournit
 * le minimum nécessaire (addition, soustraction, multiplication, division,
 * PGCD de Stein) pour comparer Rose/Rust/Python/C sur les mêmes entrées.
 * Ce n'est PAS une bibliothèque optimisée : les temps C des domaines
 * bigint mesurent ce bigint naïf, pas un équivalent de num-bigint.
 */
#ifndef BENCH_BIGINT_H
#define BENCH_BIGINT_H
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <inttypes.h>
#include <time.h>

#define BBASE 1000000000ULL

typedef struct { uint32_t *d; int n; int cap; } Big;

static void big_init(Big *a) { a->d = NULL; a->n = 0; a->cap = 0; }
static void big_free(Big *a) { free(a->d); a->d = NULL; a->n = a->cap = 0; }
static void big_reserve(Big *a, int n) {
    if (n > a->cap) {
        int c = a->cap ? a->cap : 8;
        while (c < n) c *= 2;
        uint32_t *p = (uint32_t *) realloc(a->d, (size_t)c * sizeof(uint32_t));
        if (!p) { fprintf(stderr, "big_reserve: OOM\n"); exit(2); }
        a->d = p; a->cap = c;
    }
}
static void big_norm(Big *a) { while (a->n > 0 && a->d[a->n-1] == 0) a->n--; }
static void big_set_u64(Big *a, uint64_t v) {
    big_reserve(a, 3); a->n = 0;
    while (v) { a->d[a->n++] = (uint32_t)(v % BBASE); v /= BBASE; }
    big_norm(a);
}
static void big_from_dec(Big *a, const char *s) {
    big_init(a);
    int L = (int) strlen(s);
    big_reserve(a, (L + 8) / 9 + 1);
    a->n = 0;
    for (int end = L; end > 0; end -= 9) {
        int start = end - 9; if (start < 0) start = 0;
        uint32_t v = 0;
        for (int i = start; i < end; i++) v = v * 10u + (uint32_t)(s[i] - '0');
        a->d[a->n++] = v;
    }
    big_norm(a);
}
static char *big_str(const Big *a) {
    if (a->n == 0) { char *s = (char*)malloc(2); strcpy(s, "0"); return s; }
    size_t cap = (size_t)a->n * 9 + 2;
    char *s = (char*)malloc(cap);
    int off = snprintf(s, cap, "%u", a->d[a->n-1]);
    for (int i = a->n-2; i >= 0; i--) off += snprintf(s+off, cap-(size_t)off, "%09u", a->d[i]);
    return s;
}
static int big_cmp(const Big *a, const Big *b) {
    if (a->n != b->n) return a->n < b->n ? -1 : 1;
    for (int i = a->n-1; i >= 0; i--) if (a->d[i] != b->d[i]) return a->d[i] < b->d[i] ? -1 : 1;
    return 0;
}
static void big_copy(Big *r, const Big *a) {
    if (r == a) return;
    big_reserve(r, a->n); memcpy(r->d, a->d, (size_t)a->n * sizeof(uint32_t)); r->n = a->n;
}
static void big_add(Big *r, const Big *a, const Big *b) {
    Big t; big_init(&t);
    int n = a->n > b->n ? a->n : b->n;
    big_reserve(&t, n + 1);
    uint64_t carry = 0;
    for (int i = 0; i < n; i++) {
        uint64_t v = carry + (i < a->n ? a->d[i] : 0) + (i < b->n ? b->d[i] : 0);
        t.d[i] = (uint32_t)(v % BBASE); carry = v / BBASE;
    }
    t.n = n; if (carry) t.d[t.n++] = (uint32_t)carry;
    big_free(r); *r = t;
}
static void big_sub(Big *r, const Big *a, const Big *b) { /* a >= b */
    Big t; big_init(&t); big_reserve(&t, a->n);
    int64_t borrow = 0;
    for (int i = 0; i < a->n; i++) {
        int64_t v = (int64_t)a->d[i] - borrow - (i < b->n ? (int64_t)b->d[i] : 0);
        if (v < 0) { v += (int64_t)BBASE; borrow = 1; } else borrow = 0;
        t.d[i] = (uint32_t)v;
    }
    t.n = a->n; big_norm(&t);
    big_free(r); *r = t;
}
static void big_mul_small(Big *r, const Big *a, uint32_t m) {
    Big t; big_init(&t); big_reserve(&t, a->n + 2);
    uint64_t carry = 0;
    for (int i = 0; i < a->n; i++) {
        uint64_t v = (uint64_t)a->d[i] * m + carry;
        t.d[i] = (uint32_t)(v % BBASE); carry = v / BBASE;
    }
    t.n = a->n; if (carry) t.d[t.n++] = (uint32_t)carry;
    big_free(r); *r = t;
}
static void big_mul(Big *r, const Big *a, const Big *b) {
    Big t; big_init(&t);
    if (a->n == 0 || b->n == 0) { big_free(r); big_init(r); return; }
    big_reserve(&t, a->n + b->n); memset(t.d, 0, (size_t)(a->n + b->n) * sizeof(uint32_t)); t.n = a->n + b->n;
    for (int i = 0; i < a->n; i++) {
        uint64_t carry = 0;
        for (int j = 0; j < b->n; j++) {
            uint64_t v = (uint64_t)t.d[i+j] + (uint64_t)a->d[i] * b->d[j] + carry;
            t.d[i+j] = (uint32_t)(v % BBASE); carry = v / BBASE;
        }
        int k = i + b->n;
        while (carry) { uint64_t v = (uint64_t)t.d[k] + carry; t.d[k] = (uint32_t)(v % BBASE); carry = v / BBASE; k++; }
    }
    big_norm(&t); big_free(r); *r = t;
}
/* r = a >> 1 (arithmétique sur les limbs base 10^9) */
static void big_shr1(Big *r, const Big *a) {
    Big t; big_init(&t); big_reserve(&t, a->n); t.n = a->n;
    uint32_t carry = 0;
    for (int i = a->n-1; i >= 0; i--) {
        uint64_t v = (uint64_t)a->d[i] + (uint64_t)carry * BBASE;
        t.d[i] = (uint32_t)(v / 2); carry = (uint32_t)(v % 2);
    }
    big_norm(&t); big_free(r); *r = t;
}
static void big_shl1(Big *r, const Big *a) {
    Big t; big_init(&t); big_reserve(&t, a->n + 1); t.n = a->n;
    uint32_t carry = 0;
    for (int i = 0; i < a->n; i++) {
        uint64_t v = (uint64_t)a->d[i] * 2 + carry;
        t.d[i] = (uint32_t)(v % BBASE); carry = (uint32_t)(v / BBASE);
    }
    if (carry) t.d[t.n++] = carry;
    big_free(r); *r = t;
}

/* Division longue DIGITALE (base 10) — les limbs sont en base 10^9, donc un
 * décalage binaire sur les limbs ne represente PAS 2^k : la division binaire
 * naïve etait fausse. Ici : R = R*10 + chiffre, puis au plus 9 soustractions
 * (car R < b avant l'etape). Lent mais exact. */
static void big_divmod(Big *q, Big *rem, const Big *a, const Big *b) {
    Big Q, R, one; big_init(&Q); big_init(&R); big_init(&one);
    big_set_u64(&one, 1);
    char *s = big_str(a);
    for (char *p = s; *p; p++) {
        big_mul_small(&Q, &Q, 10);
        big_mul_small(&R, &R, 10);
        Big digit; big_init(&digit); big_set_u64(&digit, (uint32_t)(*p - '0'));
        big_add(&R, &R, &digit);
        big_free(&digit);
        while (big_cmp(&R, b) >= 0) {
            Big t; big_init(&t); big_sub(&t, &R, b); big_free(&R); R = t;
            Big u; big_init(&u); big_add(&u, &Q, &one); big_free(&Q); Q = u;
        }
    }
    free(s); big_free(&one);
    big_free(q); *q = Q;
    big_free(rem); *rem = R;
}

static int big_is_even(const Big *a) { return a->n == 0 || (a->d[0] & 1u) == 0; }

/* PGCD de Stein — même algorithme que num-bigint / la primitive Rose pgcd. */
static void big_gcd_stein(Big *r, const Big *x, const Big *y) {
    Big a, b; big_init(&a); big_init(&b); big_copy(&a, x); big_copy(&b, y);
    if (a.n == 0) { big_free(r); *r = b; return; }
    if (b.n == 0) { big_free(r); *r = a; return; }
    int sh = 0;
    while (big_is_even(&a) && big_is_even(&b)) { big_shr1(&a,&a); big_shr1(&b,&b); sh++; }
    while (big_is_even(&a)) big_shr1(&a,&a);
    while (b.n) {
        while (big_is_even(&b)) big_shr1(&b,&b);
        if (big_cmp(&a,&b) > 0) { Big t = a; a = b; b = t; }
        big_sub(&b, &b, &a);
    }
    while (sh--) big_shl1(&a,&a);
    big_free(&b); big_free(r); *r = a;
}

/* exposition rapide par élévation au carré */
static void big_pow_u32(Big *r, uint32_t base, uint32_t exp) {
    Big res, b; big_init(&res); big_init(&b); big_set_u64(&res, 1); big_set_u64(&b, base);
    uint32_t e = exp;
    while (e) {
        if (e & 1u) { Big t; big_init(&t); big_mul(&t, &res, &b); big_free(&res); res = t; }
        e >>= 1;
        if (e) { Big t; big_init(&t); big_mul(&t, &b, &b); big_free(&b); b = t; }
    }
    big_free(r); *r = res; big_free(&b);
}
#endif
