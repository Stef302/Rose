#include <stdio.h>
#include "bigint.h"
int main(void) {
    /* a = "1234567890"*100, b = "9876543210"*100 — construits hors chrono. */
    Big a, b; big_init(&a); big_init(&b); big_set_u64(&a, 0); big_set_u64(&b, 0);
    Big c; big_init(&c);
    for (int i = 0; i < 100; i++) {
        big_mul_small(&a, &a, 100000); big_mul_small(&a, &a, 100000);
        big_set_u64(&c, 1234567890ULL); big_add(&a, &a, &c);
        big_mul_small(&b, &b, 100000); big_mul_small(&b, &b, 100000);
        big_set_u64(&c, 9876543210ULL); big_add(&b, &b, &c);
    }
    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    Big g; big_init(&g); big_gcd_stein(&g, &a, &b);
    clock_gettime(CLOCK_MONOTONIC, &t1);
    char *s = big_str(&g);
    uint64_t ns = (uint64_t)(t1.tv_sec-t0.tv_sec)*1000000000ULL + (uint64_t)(t1.tv_nsec-t0.tv_nsec);
    printf("RESULT:%s\nINTERNAL_NS:%" PRIu64 "\n", s, ns);
    free(s); big_free(&g); big_free(&a); big_free(&b); big_free(&c);
    return 0;
}
