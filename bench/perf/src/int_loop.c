#include <stdio.h>
#include <stdint.h>
#include <inttypes.h>
#include <time.h>
int main(void) {
    volatile uint64_t n = 1000000ULL;   /* anti-optimisation */
    struct timespec a, b;
    uint64_t s = 0;
    clock_gettime(CLOCK_MONOTONIC, &a);
    for (uint64_t k = 1; k <= n; k++) s += k;
    clock_gettime(CLOCK_MONOTONIC, &b);
    uint64_t ns = (uint64_t)(b.tv_sec-a.tv_sec)*1000000000ULL + (uint64_t)(b.tv_nsec-a.tv_nsec);
    printf("RESULT:%" PRIu64 "\nINTERNAL_NS:%" PRIu64 "\n", s, ns);
    return 0;
}
