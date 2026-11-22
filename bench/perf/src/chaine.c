#include <stdio.h>
#include <stdint.h>
#include <inttypes.h>
#include <time.h>
#include <stdlib.h>
int main(void) {
    volatile int n = 20000;            /* anti-optimisation */
    char *s = malloc(2); s[0] = 0;
    size_t L = 0;
    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    for (int i = 0; i < n; i++) {
        char *p = realloc(s, L + 2);
        if (!p) return 2;
        s = p; s[L++] = 'x'; s[L] = 0;
    }
    clock_gettime(CLOCK_MONOTONIC, &t1);
    uint64_t ns = (uint64_t)(t1.tv_sec-t0.tv_sec)*1000000000ULL + (uint64_t)(t1.tv_nsec-t0.tv_nsec);
    printf("RESULT:%zu\nINTERNAL_NS:%" PRIu64 "\n", L, ns);
    free(s);
    return 0;
}
