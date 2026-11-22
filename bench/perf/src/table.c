#include <stdio.h>
#include <stdint.h>
#include <inttypes.h>
#include <time.h>
#include <stdlib.h>
/* table associative linéaire — même structure que Rose table() (liste d'association) */
int main(void) {
    volatile int n = 3000;
    uint64_t *keys = malloc((size_t)n * sizeof(uint64_t));
    uint64_t *vals = malloc((size_t)n * sizeof(uint64_t));
    int m = 0;
    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    for (int i = 1; i <= n; i++) { keys[m] = (uint64_t)i; vals[m] = (uint64_t)i*i; m++; }
    int c = 0;
    for (int j = 1; j <= n; j++) {
        for (int q = 0; q < m; q++) if (keys[q] == (uint64_t)j) { c++; break; }
    }
    clock_gettime(CLOCK_MONOTONIC, &t1);
    uint64_t ns = (uint64_t)(t1.tv_sec-t0.tv_sec)*1000000000ULL + (uint64_t)(t1.tv_nsec-t0.tv_nsec);
    printf("RESULT:%d\nINTERNAL_NS:%" PRIu64 "\n", c, ns);
    free(keys); free(vals);
    return 0;
}
