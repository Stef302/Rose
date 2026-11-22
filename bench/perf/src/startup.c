#include <stdio.h>
#include <stdint.h>
#include <inttypes.h>
#include <time.h>
int main(void) {
    struct timespec a, b;
    clock_gettime(CLOCK_MONOTONIC, &a);
    clock_gettime(CLOCK_MONOTONIC, &b);
    uint64_t ns = (uint64_t)(b.tv_sec-a.tv_sec)*1000000000ULL + (uint64_t)(b.tv_nsec-a.tv_nsec);
    printf("RESULT:0\nINTERNAL_NS:%" PRIu64 "\n", ns);
    return 0;
}
