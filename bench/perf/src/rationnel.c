#include <stdio.h>
#include "bigint.h"
/* Somme 1/k pour k=2..200 en rationnels exacts, réduits par PGCD de Stein. */
int main(void) {
    Big num, den; big_init(&num); big_init(&den);
    big_set_u64(&num, 1); big_set_u64(&den, 2);
    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    for (uint32_t k = 3; k <= 200; k++) {
        Big kn, N, kd, g, q, r;
        big_init(&kn); big_init(&N); big_init(&kd); big_init(&g); big_init(&q); big_init(&r);
        big_mul_small(&kn, &num, k);       /* num*k */
        big_add(&N, &kn, &den);            /* num*k + den */
        big_mul_small(&kd, &den, k);       /* den*k */
        big_gcd_stein(&g, &N, &kd);
        big_divmod(&q, &r, &N, &g);        /* nouveau numérateur */
        big_free(&num); big_copy(&num, &q);
        big_divmod(&q, &r, &kd, &g);       /* nouveau dénominateur */
        big_free(&den); big_copy(&den, &q);
        big_free(&kn); big_free(&N); big_free(&kd); big_free(&g); big_free(&q); big_free(&r);
    }
    clock_gettime(CLOCK_MONOTONIC, &t1);
    char *sn = big_str(&num), *sd = big_str(&den);
    uint64_t ns = (uint64_t)(t1.tv_sec-t0.tv_sec)*1000000000ULL + (uint64_t)(t1.tv_nsec-t0.tv_nsec);
    printf("RESULT:%s/%s\nINTERNAL_NS:%" PRIu64 "\n", sn, sd, ns);
    free(sn); free(sd); big_free(&num); big_free(&den);
    return 0;
}
