#include <stdio.h>
#include <stdint.h>
#include <inttypes.h>
#include <time.h>
static uint64_t fib(uint64_t n) { return n < 2 ? n : fib(n-1) + fib(n-2); }
int main(void) {
    volatile uint64_t arg = 35;         /* anti-optimisation */
    struct timespec a, b;
    clock_gettime(CLOCK_MONOTONIC, &a);
    uint64_t r = fib(arg);
    clock_gettime(CLOCK_MONOTONIC, &b);
    uint64_t ns = (uint64_t)(b.tv_sec-a.tv_sec)*1000000000ULL + (uint64_t)(b.tv_nsec-a.tv_nsec);
    printf("RESULT:%" PRIu64 "\nINTERNAL_NS:%" PRIu64 "\n", r, ns);
    return 0;
}
