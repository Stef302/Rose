#include <stdio.h>
#include <stdint.h>
#include <inttypes.h>
#include <time.h>
int main(void) {
    volatile uint64_t n = 1000000ULL;
    uint64_t a = 0, b = 1;
    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    for (uint64_t i = 0; i < n; i++) {
        uint64_t t = (a + b) % 1000000007ULL;
        a = b; b = t;
    }
    clock_gettime(CLOCK_MONOTONIC, &t1);
    uint64_t ns = (uint64_t)(t1.tv_sec-t0.tv_sec)*1000000000ULL + (uint64_t)(t1.tv_nsec-t0.tv_nsec);
    printf("RESULT:%" PRIu64 "\nINTERNAL_NS:%" PRIu64 "\n", a, ns);
    return 0;
}
