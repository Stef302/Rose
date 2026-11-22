#include <stdio.h>
#include <stdint.h>
#include <inttypes.h>
#include <time.h>
#include <stdlib.h>
int main(void) {
    volatile int n = 100000;
    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    uint64_t *l = malloc((size_t)n * sizeof(uint64_t));
    for (int i = 0; i < n; i++) l[i] = (uint64_t)(i+1);
    uint64_t s = 0;
    for (int j = 0; j < n; j++) s += l[j];
    clock_gettime(CLOCK_MONOTONIC, &t1);
    uint64_t ns = (uint64_t)(t1.tv_sec-t0.tv_sec)*1000000000ULL + (uint64_t)(t1.tv_nsec-t0.tv_nsec);
    printf("RESULT:%" PRIu64 "\nINTERNAL_NS:%" PRIu64 "\n", s, ns);
    free(l);
    return 0;
}
