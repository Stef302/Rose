#include <stdio.h>
#include <stdint.h>
#include <inttypes.h>
#include <time.h>
static int est_premier(uint64_t n) {
    if (n < 2) return 0;
    if (n == 2) return 1;
    if (n % 2 == 0) return 0;
    for (uint64_t j = 3; j*j <= n; j += 2) if (n % j == 0) return 0;
    return 1;
}
int main(void) {
    volatile uint64_t lim = 50000;      /* anti-optimisation */
    struct timespec t0, t1;
    uint64_t c = 0;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    for (uint64_t k = 2; k < lim; k++) c += est_premier(k);
    clock_gettime(CLOCK_MONOTONIC, &t1);
    uint64_t ns = (uint64_t)(t1.tv_sec-t0.tv_sec)*1000000000ULL + (uint64_t)(t1.tv_nsec-t0.tv_nsec);
    printf("RESULT:%" PRIu64 "\nINTERNAL_NS:%" PRIu64 "\n", c, ns);
    return 0;
}
